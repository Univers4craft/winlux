//! icuuc.dll / icu.dll : convertisseurs de jeux de caractères de l'ICU de
//! Windows (#1792). Qt6Core (OBS) les importe pour QStringConverter :
//! ouverture par nom, conversion par morceaux avec état, rappels de
//! substitution, liste des noms disponibles.
//!
//! Les conversions passent par les pages de code de Winlux
//! (`intl::codepage`, `intl::dbcs`), UTF-8, UTF-16 et UTF-32 étant faits
//! ici. Seul le chemin 64 bits est pris en charge (rappels appelés en
//! win64).
//!
//! Sources : documentation publique de l'API C d'ICU (ucnv.h, ucnv_err.h,
//! utypes.h, licence Unicode) pour les prototypes, les structures
//! UConverterToUnicodeArgs / UConverterFromUnicodeArgs, les codes
//! UErrorCode et la sémantique des rappels ; aucun code d'ICU repris.

use crate::intl::{codepage, dbcs};
use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;

type UErrorCode = i32;
const U_ZERO_ERROR: i32 = 0;
const U_ILLEGAL_ARGUMENT_ERROR: i32 = 1;
const U_FILE_ACCESS_ERROR: i32 = 4;
const U_INVALID_CHAR_FOUND: i32 = 10;
const U_TRUNCATED_CHAR_FOUND: i32 = 11;
const U_ILLEGAL_CHAR_FOUND: i32 = 12;
const U_BUFFER_OVERFLOW_ERROR: i32 = 15;

/// UConverterCallbackReason.
const UCNV_UNASSIGNED: i32 = 0;
const UCNV_ILLEGAL: i32 = 1;
const UCNV_IRREGULAR: i32 = 2;

/// Encodage d'un convertisseur.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Utf8,
    Utf16(bool), // grand-boutiste ?
    Utf32(bool),
    Page(u32),
}

/// (nom canonique, nom MIME s'il en a un, encodage, alias).
const NAMES: &[(&str, bool, Kind, &[&str])] = &[
    (
        "UTF-8",
        true,
        Kind::Utf8,
        &["utf8", "unicode-1-1-utf-8", "cp1208", "ibm-1208"],
    ),
    (
        "UTF-16LE",
        true,
        Kind::Utf16(false),
        &["utf16le", "x-utf-16le", "unicodelittleunmarked"],
    ),
    (
        "UTF-16BE",
        true,
        Kind::Utf16(true),
        &["utf16be", "x-utf-16be", "unicodebigunmarked"],
    ),
    ("UTF-32LE", true, Kind::Utf32(false), &["utf32le"]),
    ("UTF-32BE", true, Kind::Utf32(true), &["utf32be"]),
    (
        "US-ASCII",
        true,
        Kind::Page(20127),
        &[
            "ascii",
            "ansi_x3.4-1968",
            "us",
            "cp367",
            "iso646-us",
            "646",
            "ibm-367",
        ],
    ),
    (
        "ISO-8859-1",
        true,
        Kind::Page(28591),
        &["latin1", "l1", "cp819", "ibm-819", "8859_1", "iso_8859-1"],
    ),
    (
        "ISO-8859-2",
        true,
        Kind::Page(28592),
        &["latin2", "l2", "cp912", "ibm-912", "8859_2"],
    ),
    (
        "ISO-8859-5",
        true,
        Kind::Page(28595),
        &["cyrillic", "cp915", "ibm-915", "8859_5"],
    ),
    (
        "ISO-8859-15",
        true,
        Kind::Page(28605),
        &["latin-9", "latin9", "l9", "8859_15", "ibm-923"],
    ),
    (
        "windows-1250",
        false,
        Kind::Page(1250),
        &["cp1250", "ibm-5346"],
    ),
    (
        "windows-1251",
        false,
        Kind::Page(1251),
        &["cp1251", "ibm-5347"],
    ),
    (
        "windows-1252",
        false,
        Kind::Page(1252),
        &["cp1252", "ibm-5348"],
    ),
    (
        "IBM437",
        false,
        Kind::Page(437),
        &["cp437", "437", "ibm-437"],
    ),
    (
        "IBM850",
        false,
        Kind::Page(850),
        &["cp850", "850", "ibm-850"],
    ),
    (
        "IBM852",
        false,
        Kind::Page(852),
        &["cp852", "852", "ibm-852"],
    ),
    ("IBM00858", false, Kind::Page(858), &["cp858", "ibm-858"]),
    (
        "IBM866",
        false,
        Kind::Page(866),
        &["cp866", "866", "ibm-866"],
    ),
    (
        "KOI8-R",
        true,
        Kind::Page(20866),
        &["koi8", "cp878", "ibm-878"],
    ),
    (
        "Shift_JIS",
        true,
        Kind::Page(932),
        &[
            "sjis",
            "ms_kanji",
            "cp932",
            "windows-31j",
            "csshiftjis",
            "ibm-943",
        ],
    ),
    (
        "GBK",
        false,
        Kind::Page(936),
        &["cp936", "ms936", "windows-936", "gb2312"],
    ),
    (
        "EUC-KR",
        true,
        Kind::Page(949),
        &["cp949", "windows-949", "ks_c_5601-1987", "ksc_5601"],
    ),
    (
        "Big5",
        true,
        Kind::Page(950),
        &["cp950", "windows-950", "csbig5"],
    ),
];

/// Comparaison des noms à la manière d'ICU : casse ignorée, seuls les
/// lettres et chiffres comptent.
fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn usable(kind: Kind) -> bool {
    match kind {
        Kind::Page(cp) if dbcs::is_dbcs(cp) => dbcs::available(cp),
        Kind::Page(cp) => codepage::is_single_byte(cp),
        _ => true,
    }
}

fn find(name: &str) -> Option<usize> {
    let want = squash(name);
    NAMES.iter().position(|(n, _, k, aliases)| {
        usable(*k) && (squash(n) == want || aliases.iter().any(|a| squash(a) == want))
    })
}

/// Noms canoniques en C, alloués une fois (le pointeur rendu reste valable).
fn c_names() -> &'static Vec<CString> {
    static N: OnceLock<Vec<CString>> = OnceLock::new();
    N.get_or_init(|| {
        NAMES
            .iter()
            .map(|(n, ..)| CString::new(*n).unwrap_or_default())
            .collect()
    })
}

fn available() -> Vec<usize> {
    (0..NAMES.len()).filter(|&i| usable(NAMES[i].2)).collect()
}

/// Rappel ICU vers Unicode : (context, args, codeUnits, length, reason, err).
type ToUCallback = unsafe extern "win64" fn(usize, *mut ToUArgs, *const c_char, i32, i32, *mut i32);
/// Rappel ICU depuis Unicode : (context, args, codeUnits, length, codePoint, reason, err).
type FromUCallback =
    unsafe extern "win64" fn(usize, *mut FromUArgs, *const u16, i32, i32, i32, *mut i32);

/// UConverterToUnicodeArgs.
#[repr(C)]
pub struct ToUArgs {
    size: u16,
    flush: i8,
    converter: usize,
    source: *const c_char,
    source_limit: *const c_char,
    target: *mut u16,
    target_limit: *const u16,
    offsets: *mut i32,
}

/// UConverterFromUnicodeArgs.
#[repr(C)]
pub struct FromUArgs {
    size: u16,
    flush: i8,
    converter: usize,
    source: *const u16,
    source_limit: *const u16,
    target: *mut c_char,
    target_limit: *const c_char,
    offsets: *mut i32,
}

struct Converter {
    index: usize,
    kind: Kind,
    to_cb: usize,
    to_ctx: usize,
    from_cb: usize,
    from_ctx: usize,
    /// Octets d'un caractère commencé (toUCountPending).
    to_pending: Vec<u8>,
    /// Demi-surrogat haut en attente (fromUCountPending).
    from_pending: Option<u16>,
    /// Sorties qui n'ont pas tenu dans la cible : rendues à l'appel suivant.
    to_overflow: Vec<u16>,
    from_overflow: Vec<u8>,
}

unsafe fn failed(err: *mut UErrorCode) -> bool {
    err.is_null() || *err > 0
}

#[allow(non_snake_case)]
unsafe extern "win64" fn ucnv_open(name: *const c_char, err: *mut UErrorCode) -> usize {
    if failed(err) {
        return 0;
    }
    let index = if name.is_null() || *name == 0 {
        // Convertisseur par défaut : la page ANSI du système.
        let cp = codepage::resolve(0);
        NAMES
            .iter()
            .position(|(_, _, k, _)| *k == Kind::Page(cp))
            .or_else(|| find("windows-1252"))
    } else {
        find(&CStr::from_ptr(name).to_string_lossy())
    };
    let Some(index) = index else {
        *err = U_FILE_ACCESS_ERROR;
        return 0;
    };
    let to_sub = UCNV_TO_U_CALLBACK_SUBSTITUTE as *const () as usize;
    let from_sub = UCNV_FROM_U_CALLBACK_SUBSTITUTE as *const () as usize;
    Box::into_raw(Box::new(Converter {
        index,
        kind: NAMES[index].2,
        to_cb: to_sub,
        to_ctx: 0,
        from_cb: from_sub,
        from_ctx: 0,
        to_pending: Vec::new(),
        from_pending: None,
        to_overflow: Vec::new(),
        from_overflow: Vec::new(),
    })) as usize
}

unsafe extern "win64" fn ucnv_close(cnv: usize) {
    if cnv != 0 {
        drop(Box::from_raw(cnv as *mut Converter));
    }
}

unsafe fn conv<'a>(cnv: usize) -> Option<&'a mut Converter> {
    (cnv as *mut Converter).as_mut()
}

unsafe extern "win64" fn ucnv_reset(cnv: usize) {
    if let Some(c) = conv(cnv) {
        c.to_pending.clear();
        c.from_pending = None;
        c.to_overflow.clear();
        c.from_overflow.clear();
    }
}

unsafe extern "win64" fn ucnv_get_max_char_size(cnv: usize) -> i8 {
    match conv(cnv).map(|c| c.kind) {
        Some(Kind::Utf8) => 3,
        Some(Kind::Utf16(_)) => 2,
        Some(Kind::Utf32(_)) => 4,
        Some(Kind::Page(cp)) if dbcs::is_dbcs(cp) => 2,
        _ => 1,
    }
}

unsafe extern "win64" fn ucnv_get_name(cnv: usize, err: *mut UErrorCode) -> *const c_char {
    if failed(err) {
        return std::ptr::null();
    }
    match conv(cnv) {
        Some(c) => c_names()[c.index].as_ptr(),
        None => {
            *err = U_ILLEGAL_ARGUMENT_ERROR;
            std::ptr::null()
        }
    }
}

unsafe extern "win64" fn ucnv_get_to_u_callback(cnv: usize, action: *mut usize, ctx: *mut usize) {
    if let Some(c) = conv(cnv) {
        if !action.is_null() {
            *action = c.to_cb;
        }
        if !ctx.is_null() {
            *ctx = c.to_ctx;
        }
    }
}

unsafe extern "win64" fn ucnv_get_from_u_callback(cnv: usize, action: *mut usize, ctx: *mut usize) {
    if let Some(c) = conv(cnv) {
        if !action.is_null() {
            *action = c.from_cb;
        }
        if !ctx.is_null() {
            *ctx = c.from_ctx;
        }
    }
}

unsafe extern "win64" fn ucnv_set_to_u_callback(
    cnv: usize,
    action: usize,
    ctx: usize,
    old_action: *mut usize,
    old_ctx: *mut usize,
    err: *mut UErrorCode,
) {
    if failed(err) {
        return;
    }
    let Some(c) = conv(cnv) else {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    };
    if !old_action.is_null() {
        *old_action = c.to_cb;
    }
    if !old_ctx.is_null() {
        *old_ctx = c.to_ctx;
    }
    c.to_cb = action;
    c.to_ctx = ctx;
}

unsafe extern "win64" fn ucnv_set_from_u_callback(
    cnv: usize,
    action: usize,
    ctx: usize,
    old_action: *mut usize,
    old_ctx: *mut usize,
    err: *mut UErrorCode,
) {
    if failed(err) {
        return;
    }
    let Some(c) = conv(cnv) else {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    };
    if !old_action.is_null() {
        *old_action = c.from_cb;
    }
    if !old_ctx.is_null() {
        *old_ctx = c.from_ctx;
    }
    c.from_cb = action;
    c.from_ctx = ctx;
}

unsafe extern "win64" fn ucnv_to_u_count_pending(cnv: usize, err: *mut UErrorCode) -> i32 {
    if failed(err) {
        return -1;
    }
    match conv(cnv) {
        Some(c) => c.to_pending.len() as i32,
        None => {
            *err = U_ILLEGAL_ARGUMENT_ERROR;
            -1
        }
    }
}

unsafe extern "win64" fn ucnv_from_u_count_pending(cnv: usize, err: *mut UErrorCode) -> i32 {
    if failed(err) {
        return -1;
    }
    match conv(cnv) {
        Some(c) => i32::from(c.from_pending.is_some()),
        None => {
            *err = U_ILLEGAL_ARGUMENT_ERROR;
            -1
        }
    }
}

unsafe extern "win64" fn ucnv_count_available() -> i32 {
    available().len() as i32
}

unsafe extern "win64" fn ucnv_get_available_name(n: i32) -> *const c_char {
    match available().get(n.max(0) as usize) {
        Some(&i) if n >= 0 => c_names()[i].as_ptr(),
        _ => std::ptr::null(),
    }
}

/// ucnv_getStandardName(name, standard, err) : nom du convertisseur dans
/// la norme « MIME » ou « IANA », NULL s'il n'y en a pas.
unsafe extern "win64" fn ucnv_get_standard_name(
    name: *const c_char,
    standard: *const c_char,
    err: *mut UErrorCode,
) -> *const c_char {
    if failed(err) {
        return std::ptr::null();
    }
    if name.is_null() || standard.is_null() {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return std::ptr::null();
    }
    let Some(i) = find(&CStr::from_ptr(name).to_string_lossy()) else {
        return std::ptr::null();
    };
    let standard = CStr::from_ptr(standard)
        .to_string_lossy()
        .to_ascii_uppercase();
    match standard.as_str() {
        "MIME" if !NAMES[i].1 => std::ptr::null(),
        "MIME" | "IANA" => c_names()[i].as_ptr(),
        _ => std::ptr::null(),
    }
}

/// Écrit des UChar dans la cible des arguments, le reste dans le tampon
/// de débordement du convertisseur (U_BUFFER_OVERFLOW_ERROR).
unsafe fn write_uchars(args: *mut ToUArgs, chars: &[u16], offset: i32, err: *mut UErrorCode) {
    let a = &mut *args;
    for (k, &u) in chars.iter().enumerate() {
        if a.target as *const u16 >= a.target_limit {
            if let Some(c) = conv(a.converter) {
                c.to_overflow.extend_from_slice(&chars[k..]);
            }
            *err = U_BUFFER_OVERFLOW_ERROR;
            return;
        }
        *a.target = u;
        a.target = a.target.add(1);
        if !a.offsets.is_null() {
            *a.offsets = offset;
            a.offsets = a.offsets.add(1);
        }
    }
}

unsafe fn write_bytes(args: *mut FromUArgs, bytes: &[u8], offset: i32, err: *mut UErrorCode) {
    let a = &mut *args;
    for (k, &b) in bytes.iter().enumerate() {
        if a.target as *const c_char >= a.target_limit {
            if let Some(c) = conv(a.converter) {
                c.from_overflow.extend_from_slice(&bytes[k..]);
            }
            *err = U_BUFFER_OVERFLOW_ERROR;
            return;
        }
        *a.target = b as c_char;
        a.target = a.target.add(1);
        if !a.offsets.is_null() {
            *a.offsets = offset;
            a.offsets = a.offsets.add(1);
        }
    }
}

#[allow(non_snake_case)]
unsafe extern "win64" fn ucnv_cbToUWriteUChars(
    args: *mut ToUArgs,
    source: *const u16,
    length: i32,
    offset: i32,
    err: *mut UErrorCode,
) {
    if failed(err) || args.is_null() || (source.is_null() && length > 0) {
        return;
    }
    let chars = std::slice::from_raw_parts(source, length.max(0) as usize);
    write_uchars(args, chars, offset, err);
}

#[allow(non_snake_case)]
unsafe extern "win64" fn ucnv_cbFromUWriteUChars(
    args: *mut FromUArgs,
    source: *mut *const u16,
    limit: *const u16,
    offset: i32,
    err: *mut UErrorCode,
) {
    if failed(err) || args.is_null() || source.is_null() || (*source).is_null() {
        return;
    }
    let Some(kind) = conv((*args).converter).map(|c| c.kind) else {
        return;
    };
    let n = limit.offset_from(*source).max(0) as usize;
    let units = std::slice::from_raw_parts(*source, n);
    let mut bytes = Vec::new();
    for c in char::decode_utf16(units.iter().copied()) {
        let c = c.unwrap_or('?');
        bytes.extend(encode(kind, c).unwrap_or_else(|| vec![b'?']));
    }
    *source = limit;
    write_bytes(args, &bytes, offset, err);
}

/// Rappel par défaut vers Unicode : U+FFFD à la place de la séquence
/// (sauf contexte « i », qui arrête sur une séquence illégale).
#[allow(non_snake_case)]
unsafe extern "win64" fn UCNV_TO_U_CALLBACK_SUBSTITUTE(
    ctx: usize,
    args: *mut ToUArgs,
    _units: *const c_char,
    _len: i32,
    reason: i32,
    err: *mut UErrorCode,
) {
    if reason > UCNV_IRREGULAR || err.is_null() {
        return;
    }
    if ctx != 0 && *(ctx as *const u8) == b'i' && reason != UCNV_UNASSIGNED {
        return;
    }
    *err = U_ZERO_ERROR;
    write_uchars(args, &[0xfffd], 0, err);
}

/// Rappel par défaut depuis Unicode : « ? » à la place du caractère.
#[allow(non_snake_case)]
unsafe extern "win64" fn UCNV_FROM_U_CALLBACK_SUBSTITUTE(
    ctx: usize,
    args: *mut FromUArgs,
    _units: *const u16,
    _len: i32,
    _cp: i32,
    reason: i32,
    err: *mut UErrorCode,
) {
    if reason > UCNV_IRREGULAR || err.is_null() {
        return;
    }
    if ctx != 0 && *(ctx as *const u8) == b'i' && reason != UCNV_UNASSIGNED {
        return;
    }
    *err = U_ZERO_ERROR;
    write_bytes(args, b"?", 0, err);
}

/// Octets d'un caractère dans l'encodage, `None` s'il n'y figure pas.
fn encode(kind: Kind, c: char) -> Option<Vec<u8>> {
    let mut buf = [0u16; 2];
    Some(match kind {
        Kind::Utf8 => c.to_string().into_bytes(),
        Kind::Utf16(be) => c
            .encode_utf16(&mut buf)
            .iter()
            .flat_map(|u| if be { u.to_be_bytes() } else { u.to_le_bytes() })
            .collect(),
        Kind::Utf32(be) => {
            let n = c as u32;
            (if be { n.to_be_bytes() } else { n.to_le_bytes() }).to_vec()
        }
        Kind::Page(cp) if dbcs::is_dbcs(cp) => dbcs::encode_char(cp, c)?,
        Kind::Page(cp) => vec![codepage::encode_char(cp, c)?],
    })
}

/// Où en est le décodage d'une séquence d'octets.
enum Step {
    More,
    Char(Vec<u16>),
    Bad(i32), // raison ICU
}

fn decode_step(kind: Kind, seq: &[u8]) -> Step {
    match kind {
        Kind::Utf8 => {
            let need = match seq[0] {
                0x00..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Step::Bad(UCNV_ILLEGAL),
            };
            if seq.len() > 1 && seq[seq.len() - 1] & 0xc0 != 0x80 {
                return Step::Bad(UCNV_ILLEGAL);
            }
            if seq.len() < need {
                return Step::More;
            }
            match std::str::from_utf8(seq) {
                Ok(s) => Step::Char(s.encode_utf16().collect()),
                Err(_) => Step::Bad(UCNV_ILLEGAL),
            }
        }
        Kind::Utf16(be) => {
            if seq.len() < 2 {
                return Step::More;
            }
            let u = if be {
                u16::from_be_bytes([seq[0], seq[1]])
            } else {
                u16::from_le_bytes([seq[0], seq[1]])
            };
            Step::Char(vec![u])
        }
        Kind::Utf32(be) => {
            if seq.len() < 4 {
                return Step::More;
            }
            let b = [seq[0], seq[1], seq[2], seq[3]];
            let n = if be {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            };
            match char::from_u32(n) {
                Some(c) => Step::Char(c.encode_utf16(&mut [0; 2]).to_vec()),
                None => Step::Bad(UCNV_ILLEGAL),
            }
        }
        Kind::Page(cp) if dbcs::is_dbcs(cp) => {
            if seq.len() == 1 && dbcs::is_lead_byte(cp, seq[0]) {
                return Step::More;
            }
            match dbcs::decode_seq(cp, seq) {
                Some(c) => Step::Char(c.encode_utf16(&mut [0; 2]).to_vec()),
                None => Step::Bad(UCNV_UNASSIGNED),
            }
        }
        Kind::Page(cp) => match codepage::decode_byte(cp, seq[0]) {
            Some(c) => Step::Char(c.encode_utf16(&mut [0; 2]).to_vec()),
            None => Step::Bad(UCNV_UNASSIGNED),
        },
    }
}

/// Appelle le rappel vers Unicode avec la séquence fautive ; rend vrai si
/// la conversion doit s'arrêter (erreur laissée par le rappel).
unsafe fn to_u_error(
    c: &Converter,
    args: &mut ToUArgs,
    seq: &[u8],
    reason: i32,
    code: i32,
    err: *mut UErrorCode,
) -> bool {
    *err = code;
    if c.to_cb != 0 {
        let f: ToUCallback = std::mem::transmute(c.to_cb);
        f(
            c.to_ctx,
            args,
            seq.as_ptr().cast(),
            seq.len() as i32,
            reason,
            err,
        );
    }
    *err > 0
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn ucnv_to_unicode(
    cnv: usize,
    target: *mut *mut u16,
    target_limit: *const u16,
    source: *mut *const c_char,
    source_limit: *const c_char,
    offsets: *mut i32,
    flush: i8,
    err: *mut UErrorCode,
) {
    if failed(err) {
        return;
    }
    let Some(c) = conv(cnv) else {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    };
    if target.is_null() || source.is_null() || (*target as *const u16) > target_limit {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    }
    let start = *source;
    let mut args = ToUArgs {
        size: std::mem::size_of::<ToUArgs>() as u16,
        flush,
        converter: cnv,
        source: *source,
        source_limit,
        target: *target,
        target_limit,
        offsets,
    };
    let done = |args: &ToUArgs| {
        *target = args.target;
        *source = args.source;
    };
    // Débordement de l'appel précédent d'abord.
    let pending = std::mem::take(&mut c.to_overflow);
    write_uchars(&mut args, &pending, -1, err);
    if *err > 0 {
        done(&args);
        return;
    }
    while args.source < source_limit {
        let b = *args.source as u8;
        args.source = args.source.add(1);
        c.to_pending.push(b);
        let at = (args.source.offset_from(start) - c.to_pending.len() as isize).max(0) as i32;
        match decode_step(c.kind, &c.to_pending) {
            Step::More => continue,
            Step::Char(units) => {
                c.to_pending.clear();
                write_uchars(&mut args, &units, at, err);
                if *err > 0 {
                    done(&args);
                    return;
                }
            }
            Step::Bad(reason) => {
                let seq = std::mem::take(&mut c.to_pending);
                let code = if reason == UCNV_UNASSIGNED {
                    U_INVALID_CHAR_FOUND
                } else {
                    U_ILLEGAL_CHAR_FOUND
                };
                if to_u_error(c, &mut args, &seq, reason, code, err) {
                    done(&args);
                    return;
                }
            }
        }
    }
    if flush != 0 && !c.to_pending.is_empty() {
        let seq = std::mem::take(&mut c.to_pending);
        if to_u_error(
            c,
            &mut args,
            &seq,
            UCNV_ILLEGAL,
            U_TRUNCATED_CHAR_FOUND,
            err,
        ) {
            done(&args);
            return;
        }
    }
    done(&args);
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn ucnv_from_unicode(
    cnv: usize,
    target: *mut *mut c_char,
    target_limit: *const c_char,
    source: *mut *const u16,
    source_limit: *const u16,
    offsets: *mut i32,
    flush: i8,
    err: *mut UErrorCode,
) {
    if failed(err) {
        return;
    }
    let Some(c) = conv(cnv) else {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    };
    if target.is_null() || source.is_null() || (*target as *const c_char) > target_limit {
        *err = U_ILLEGAL_ARGUMENT_ERROR;
        return;
    }
    let start = *source;
    let mut args = FromUArgs {
        size: std::mem::size_of::<FromUArgs>() as u16,
        flush,
        converter: cnv,
        source: *source,
        source_limit,
        target: *target,
        target_limit,
        offsets,
    };
    let done = |args: &FromUArgs| {
        *target = args.target;
        *source = args.source;
    };
    let pending = std::mem::take(&mut c.from_overflow);
    write_bytes(&mut args, &pending, -1, err);
    if *err > 0 {
        done(&args);
        return;
    }
    loop {
        let at = args.source.offset_from(start) as i32;
        // Unités du prochain caractère : demi-surrogat gardé, puis source.
        let (units, cp): (Vec<u16>, Option<char>) = match c.from_pending.take() {
            Some(hi) if args.source < source_limit => {
                let lo = *args.source;
                if (0xdc00..0xe000).contains(&lo) {
                    args.source = args.source.add(1);
                    let n = 0x10000 + ((u32::from(hi) - 0xd800) << 10) + (u32::from(lo) - 0xdc00);
                    (vec![hi, lo], char::from_u32(n))
                } else {
                    (vec![hi], None)
                }
            }
            Some(hi) if flush != 0 => (vec![hi], None),
            Some(hi) => {
                c.from_pending = Some(hi);
                break;
            }
            None if args.source < source_limit => {
                let u = *args.source;
                args.source = args.source.add(1);
                if (0xd800..0xdc00).contains(&u) {
                    c.from_pending = Some(u);
                    continue;
                }
                (vec![u], char::from_u32(u32::from(u)))
            }
            None => break,
        };
        let encoded = cp.and_then(|ch| encode(c.kind, ch));
        match encoded {
            Some(bytes) => {
                write_bytes(&mut args, &bytes, at, err);
                if *err > 0 {
                    done(&args);
                    return;
                }
            }
            None => {
                let reason = if cp.is_some() {
                    UCNV_UNASSIGNED
                } else {
                    UCNV_ILLEGAL
                };
                *err = if cp.is_some() {
                    U_INVALID_CHAR_FOUND
                } else {
                    U_ILLEGAL_CHAR_FOUND
                };
                if c.from_cb != 0 {
                    let f: FromUCallback = std::mem::transmute(c.from_cb);
                    let point = cp.map_or(i32::from(units[0]), |ch| ch as i32);
                    f(
                        c.from_ctx,
                        &mut args,
                        units.as_ptr(),
                        units.len() as i32,
                        point,
                        reason,
                        err,
                    );
                }
                if *err > 0 {
                    done(&args);
                    return;
                }
            }
        }
    }
    done(&args);
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "ucnv_open" => fn ucnv_open,
        "ucnv_close" => fn ucnv_close,
        "ucnv_reset" => fn ucnv_reset,
        "ucnv_getMaxCharSize" => fn ucnv_get_max_char_size,
        "ucnv_getName" => fn ucnv_get_name,
        "ucnv_getToUCallBack" => fn ucnv_get_to_u_callback,
        "ucnv_getFromUCallBack" => fn ucnv_get_from_u_callback,
        "ucnv_setToUCallBack" => fn ucnv_set_to_u_callback,
        "ucnv_setFromUCallBack" => fn ucnv_set_from_u_callback,
        "ucnv_cbToUWriteUChars" => fn ucnv_cbToUWriteUChars,
        "ucnv_cbFromUWriteUChars" => fn ucnv_cbFromUWriteUChars,
        "ucnv_toUCountPending" => fn ucnv_to_u_count_pending,
        "ucnv_fromUCountPending" => fn ucnv_from_u_count_pending,
        "ucnv_getStandardName" => fn ucnv_get_standard_name,
        "ucnv_getAvailableName" => fn ucnv_get_available_name,
        "ucnv_countAvailable" => fn ucnv_count_available,
        "ucnv_toUnicode" => fn ucnv_to_unicode,
        "ucnv_fromUnicode" => fn ucnv_from_unicode,
        "UCNV_TO_U_CALLBACK_SUBSTITUTE" => fn UCNV_TO_U_CALLBACK_SUBSTITUTE,
        "UCNV_FROM_U_CALLBACK_SUBSTITUTE" => fn UCNV_FROM_U_CALLBACK_SUBSTITUTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn open(name: &str) -> usize {
        let n = CString::new(name).unwrap();
        let mut err = 0;
        let c = ucnv_open(n.as_ptr(), &mut err);
        assert_eq!(err, 0, "{name}");
        c
    }

    unsafe fn to_u(c: usize, bytes: &[u8], flush: bool, out: &mut Vec<u16>) -> i32 {
        let mut buf = [0u16; 64];
        let mut t = buf.as_mut_ptr();
        let mut s = bytes.as_ptr().cast::<c_char>();
        let mut err = 0;
        ucnv_to_unicode(
            c,
            &mut t,
            buf.as_ptr().add(buf.len()),
            &mut s,
            bytes.as_ptr().add(bytes.len()).cast(),
            std::ptr::null_mut(),
            i8::from(flush),
            &mut err,
        );
        out.extend_from_slice(&buf[..t.offset_from(buf.as_ptr()) as usize]);
        err
    }

    #[test]
    fn convertisseurs_icu() {
        unsafe {
            // Noms : alias et casse ignorés, nom inconnu refusé.
            let c = open("latin1");
            let mut err = 0;
            let name = CStr::from_ptr(ucnv_get_name(c, &mut err));
            assert_eq!(name.to_str().unwrap(), "ISO-8859-1");
            let bogus = CString::new("x-inexistant").unwrap();
            assert_eq!(ucnv_open(bogus.as_ptr(), &mut err), 0);
            assert_eq!(err, U_FILE_ACCESS_ERROR);
            let mut out = Vec::new();
            assert_eq!(to_u(c, b"caf\xe9", true, &mut out), 0);
            assert_eq!(String::from_utf16(&out).unwrap(), "café");
            // Encodage avec substitution par défaut.
            let text: Vec<u16> = "é€".encode_utf16().collect();
            let mut buf = [0 as c_char; 8];
            let mut t = buf.as_mut_ptr();
            let mut s = text.as_ptr();
            let mut err = 0;
            ucnv_from_unicode(
                c,
                &mut t,
                buf.as_ptr().add(8),
                &mut s,
                text.as_ptr().add(text.len()),
                std::ptr::null_mut(),
                1,
                &mut err,
            );
            assert_eq!(err, 0);
            assert_eq!(t.offset_from(buf.as_ptr()), 2);
            assert_eq!(buf[0] as u8, 0xe9);
            assert_eq!(buf[1] as u8, b'?');
            ucnv_close(c);

            // UTF-8 coupé entre deux appels : l'état est gardé.
            let u = open("UTF8");
            let mut out = Vec::new();
            assert_eq!(to_u(u, b"a\xe2\x82", false, &mut out), 0);
            assert_eq!(ucnv_to_u_count_pending(u, &mut 0), 2);
            assert_eq!(to_u(u, b"\xac", true, &mut out), 0);
            assert_eq!(String::from_utf16(&out).unwrap(), "a€");
            // Séquence illégale : U+FFFD par le rappel par défaut.
            let mut out = Vec::new();
            assert_eq!(to_u(u, b"\xffz", true, &mut out), 0);
            assert_eq!(out, vec![0xfffd, u16::from(b'z')]);
            // Rappel remplacé : l'erreur reste et la conversion s'arrête.
            let mut old = 0usize;
            let mut err = 0;
            ucnv_set_to_u_callback(u, 0, 0, &mut old, std::ptr::null_mut(), &mut err);
            assert_eq!(old, UCNV_TO_U_CALLBACK_SUBSTITUTE as *const () as usize);
            let mut out = Vec::new();
            assert_eq!(to_u(u, b"\xff", true, &mut out), U_ILLEGAL_CHAR_FOUND);
            ucnv_close(u);

            // Liste et noms normalisés.
            assert!(ucnv_count_available() >= 10);
            assert!(!ucnv_get_available_name(0).is_null());
            assert!(ucnv_get_available_name(ucnv_count_available()).is_null());
            let n = CString::new("cp1252").unwrap();
            let mime = CString::new("MIME").unwrap();
            let iana = CString::new("IANA").unwrap();
            assert!(ucnv_get_standard_name(n.as_ptr(), mime.as_ptr(), &mut 0).is_null());
            let s = ucnv_get_standard_name(n.as_ptr(), iana.as_ptr(), &mut 0);
            assert_eq!(CStr::from_ptr(s).to_str().unwrap(), "windows-1252");
        }
    }

    #[test]
    fn debordement_rendu_a_l_appel_suivant() {
        unsafe {
            let c = open("UTF-16BE");
            let bytes = [0u8, b'a', 0, b'b', 0, b'c'];
            let mut buf = [0u16; 2];
            let mut t = buf.as_mut_ptr();
            let mut s = bytes.as_ptr().cast::<c_char>();
            let mut err = 0;
            ucnv_to_unicode(
                c,
                &mut t,
                buf.as_ptr().add(2),
                &mut s,
                bytes.as_ptr().add(6).cast(),
                std::ptr::null_mut(),
                1,
                &mut err,
            );
            assert_eq!(err, U_BUFFER_OVERFLOW_ERROR);
            assert_eq!(buf, [u16::from(b'a'), u16::from(b'b')]);
            let mut out = Vec::new();
            assert_eq!(to_u(c, b"", true, &mut out), 0);
            assert_eq!(out, vec![u16::from(b'c')]);
            ucnv_close(c);
        }
    }

    /// .NET charge icu.dll en premier et exige toute l'API ICU : une
    /// icu.dll partielle fait avorter Paint.NET (#2056). Seule icuuc.dll,
    /// importée par Qt6Core d'OBS (#1792), est servie.
    #[test]
    fn pas_d_icu_dll_partielle() {
        assert!(crate::dlls::is_builtin("icuuc"));
        assert!(!crate::dlls::is_builtin("icu"));
        assert!(!crate::dlls::is_builtin("icuin"));
    }
}
