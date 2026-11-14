//! Compléments de msvcp140 importés par iCUE et encore absents (#2309) :
//! fonctions de flux (`basic_streambuf`, `basic_istream`, `basic_ostream`,
//! `basic_iostream`, `basic_ios`), `locale::id`, `std::call_once`,
//! `_Dtest` et des lanceurs `_X*`.
//!
//! Écrit d'après la MS STL publique (Apache 2.0 avec exception LLVM) :
//! `stl/inc/streambuf`, `istream`, `__msvc_ostream.hpp`, `ios`,
//! `xiosbase`, `xlocale`, `xlocnum`, et `stl/src/xonce.cpp`,
//! `xdtest.cpp`, `xthrow.cpp`, `future.cpp`. Les dispositions sont celles
//! du module parent.
//!
//! Nos choix, dits :
//!
//! - `operator>>(float&)` et `operator>>(double&)` lisent un nombre décimal
//!   (signe, chiffres, point, exposant) dans la locale « C », puis le
//!   convertissent par `strtod`/`strtof` ; la forme hexadécimale que
//!   `_Parse_fp_with_locale` accepte aussi n'est pas lue.
//! - `operator>>(bool&)` avec `boolalpha` compare aux noms « false » et
//!   « true » de la locale « C » (la seule de notre CRT).
//! - `_Throw_future_error`, `_XGetLastError` et `_Xregex_error` arrêtent le
//!   programme avec un message, comme nos autres fonctions `_X*` (aucun
//!   ThrowInfo de ces exceptions n'existe chez nous).

use super::*;

// ---------- basic_streambuf<char> ----------

/// `basic_streambuf::operator=` (protégé) : zones et locale de `right`.
unsafe extern "win64" fn sb_assign<C: Ch>(sb: P, right: P) -> P {
    if sb != right {
        sb_setp3::<C>(
            sb,
            sb_pbase::<C>(right),
            sb_pptr::<C>(right),
            sb_epptr::<C>(right),
        );
        sb_setg::<C>(
            sb,
            sb_eback::<C>(right),
            sb_gptr::<C>(right),
            sb_egptr::<C>(right),
        );
        sb_pubimbue(sb, rd::<*const usize>(right, SB_PLOCALE));
    }
    sb
}

/// `basic_streambuf::swap` : zones de lecture et d'écriture, locale.
unsafe extern "win64" fn sb_swap<C: Ch>(sb: P, right: P) {
    if sb == right {
        return;
    }
    let (pf, pn, pe) = (sb_pbase::<C>(sb), sb_pptr::<C>(sb), sb_epptr::<C>(sb));
    let (gf, gn, ge) = (sb_eback::<C>(sb), sb_gptr::<C>(sb), sb_egptr::<C>(sb));
    sb_setp3::<C>(
        sb,
        sb_pbase::<C>(right),
        sb_pptr::<C>(right),
        sb_epptr::<C>(right),
    );
    sb_setp3::<C>(right, pf, pn, pe);
    sb_setg::<C>(
        sb,
        sb_eback::<C>(right),
        sb_gptr::<C>(right),
        sb_egptr::<C>(right),
    );
    sb_setg::<C>(right, gf, gn, ge);
    let (a, b): (usize, usize) = (rd(sb, SB_PLOCALE), rd(right, SB_PLOCALE));
    wr(sb, SB_PLOCALE, b);
    wr(right, SB_PLOCALE, a);
}

// ---------- ios_base, basic_ios ----------

const ARR_INDEX: usize = 8;
const ARR_LO: usize = 12;
const ARR_VP: usize = 16;
const ARR_SIZE: usize = 24;
const FN_INDEX: usize = 8;
const FN_PFN: usize = 16;
const FN_SIZE: usize = 24;
const COPYFMT_EVENT: i32 = 2;

/// `ios_base::_Findarr` : nœud de l'indice `idx`, recyclé ou créé.
unsafe fn findarr(ios: P, idx: i32) -> P {
    let mut spare: P = std::ptr::null_mut();
    let mut node: P = rd(ios, IOS_ARR);
    while !node.is_null() {
        if rd::<i32>(node, ARR_INDEX) == idx {
            return node;
        }
        if spare.is_null() && rd::<i32>(node, ARR_LO) == 0 && rd::<usize>(node, ARR_VP) == 0 {
            spare = node;
        }
        node = rd(node, 0);
    }
    if !spare.is_null() {
        wr(spare, ARR_INDEX, idx);
        return spare;
    }
    let n = alloc_zeroed(ARR_SIZE);
    wr(n, 0, rd::<P>(ios, IOS_ARR));
    wr(n, ARR_INDEX, idx);
    wr(ios, IOS_ARR, n);
    n
}

/// `ios_base::_Tidy` : rappel `erase_event`, listes libérées.
unsafe fn ios_tidy(ios: P) {
    ios_callfns(ios, 0);
    for off in [IOS_ARR, IOS_CALLS] {
        let mut node: P = rd(ios, off);
        while !node.is_null() {
            let next: P = rd(node, 0);
            free(node);
            node = next;
        }
        wr(ios, off, std::ptr::null_mut::<u8>());
    }
}

/// `basic_ios::copyfmt` : lien, remplissage, puis `ios_base::copyfmt`
/// (tout sauf l'état).
unsafe extern "win64" fn bios_copyfmt<C: Ch>(ios: P, right: P) -> P {
    wr(ios, BIOS_TIE, rd::<P>(right, BIOS_TIE));
    wr(ios, BIOS_FILL, rd::<C>(right, BIOS_FILL));
    if ios != right {
        ios_tidy(ios);
        locale_assign(rd(ios, IOS_PLOC), rd(right, IOS_PLOC));
        wr(ios, IOS_FMTFL, rd::<i32>(right, IOS_FMTFL));
        wr(ios, IOS_PREC, rd::<i64>(right, IOS_PREC));
        wr(ios, IOS_WIDE, rd::<i64>(right, IOS_WIDE));
        let mut node: P = rd(right, IOS_ARR);
        while !node.is_null() {
            let (lo, vp): (i32, usize) = (rd(node, ARR_LO), rd(node, ARR_VP));
            if lo != 0 || vp != 0 {
                let mine = findarr(ios, rd(node, ARR_INDEX));
                wr(mine, ARR_LO, lo);
                wr(mine, ARR_VP, vp);
            }
            node = rd(node, 0);
        }
        let mut node: P = rd(right, IOS_CALLS);
        while !node.is_null() {
            let n = alloc_zeroed(FN_SIZE);
            wr(n, 0, rd::<P>(ios, IOS_CALLS));
            wr(n, FN_INDEX, rd::<i32>(node, FN_INDEX));
            wr(n, FN_PFN, rd::<usize>(node, FN_PFN));
            wr(ios, IOS_CALLS, n);
            node = rd(node, 0);
        }
        ios_callfns(ios, COPYFMT_EVENT);
        // exceptions(right._Except) : masque, puis clear(rdstate()).
        wr(ios, IOS_EXCEPT, rd::<i32>(right, IOS_EXCEPT) & STATMASK);
        bios_clear::<C>(ios, rd(ios, IOS_STATE), false);
    }
    ios
}

/// `basic_ios::rdbuf(strbuf)` : nouveau tampon, état remis à zéro.
unsafe extern "win64" fn bios_set_rdbuf<C: Ch>(ios: P, sb: P) -> P {
    let old: P = rd(ios, BIOS_STRBUF);
    wr(ios, BIOS_STRBUF, sb);
    bios_clear::<C>(ios, GOODBIT, false);
    old
}

/// `explicit basic_ios(strbuf)` : `init(strbuf)`.
unsafe extern "win64" fn bios_ctor_sb<C: Ch>(ios: P, sb: P) -> P {
    bios_ctor::<C>(ios);
    bios_init::<C>(ios, sb, false);
    ios
}

/// `basic_ios::swap` : tout sauf le tampon (`ios_base::swap`, remplissage,
/// lien).
unsafe fn bios_swap<C: Ch>(a: P, b: P) {
    if a == b {
        return;
    }
    unsafe fn ech<T: Copy>(a: P, b: P, off: usize) {
        let (x, y): (T, T) = (rd(a, off), rd(b, off));
        wr(a, off, y);
        wr(b, off, x);
    }
    ech::<i32>(a, b, IOS_STATE);
    ech::<i32>(a, b, IOS_EXCEPT);
    ech::<i32>(a, b, IOS_FMTFL);
    ech::<i64>(a, b, IOS_PREC);
    ech::<i64>(a, b, IOS_WIDE);
    ech::<usize>(a, b, IOS_ARR);
    ech::<usize>(a, b, IOS_CALLS);
    ech::<usize>(a, b, IOS_PLOC);
    ech::<C>(a, b, BIOS_FILL);
    ech::<usize>(a, b, BIOS_TIE);
}

// ---------- basic_istream<char> ----------

/// `get()` : un caractère, ou EOF.
unsafe extern "win64" fn is_get<C: Ch>(is: P) -> C::I {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    let meta;
    if !is_sentry::<C>(is, true) {
        meta = C::eof();
    } else {
        let sb: P = rd(b, BIOS_STRBUF);
        meta = sb_sgetc::<C>(sb);
        if C::is_eof(meta) {
            state |= EOFBIT | FAILBIT;
        } else {
            sb_sbumpc::<C>(sb);
            wr(is, IS_CHCOUNT, 1i64);
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    meta
}

/// `get(s, count)` : jusqu'à `count - 1` caractères, arrêt avant '\n'.
unsafe extern "win64" fn is_get_str<C: Ch>(is: P, s: *mut C, count: i64) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    let mut out = s;
    if is_sentry::<C>(is, true) && count > 0 {
        let sb: P = rd(b, BIOS_STRBUF);
        let delim = C::widen(b'\n');
        let mut left = count;
        let mut meta = sb_sgetc::<C>(sb);
        loop {
            left -= 1;
            if left <= 0 {
                break;
            }
            if C::is_eof(meta) {
                state |= EOFBIT;
                break;
            }
            let c = C::from_int(meta);
            if c == delim {
                break;
            }
            *out = c;
            out = out.add(1);
            bump_gcount(is);
            meta = sb_snextc::<C>(sb);
        }
    }
    if count > 0 {
        *out = C::default();
    }
    let n: i64 = rd(is, IS_CHCOUNT);
    ios_setstate::<C>(b, if n == 0 { state | FAILBIT } else { state });
    is_sentry_end(is);
    is
}

/// `peek()` : le prochain caractère, sans l'extraire.
unsafe extern "win64" fn is_peek<C: Ch>(is: P) -> C::I {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    let meta;
    if !is_sentry::<C>(is, true) {
        meta = C::eof();
    } else {
        meta = sb_sgetc::<C>(rd(b, BIOS_STRBUF));
        if C::is_eof(meta) {
            state |= EOFBIT;
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    meta
}

/// `ignore(count, delim)` : saute jusqu'à `count` caractères, délimiteur
/// compris ; `count` = maximum de `streamsize` : sans limite.
unsafe extern "win64" fn is_ignore<C: Ch>(is: P, count: i64, delim: C::I) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    if is_sentry::<C>(is, true) && count > 0 {
        let sb: P = rd(b, BIOS_STRBUF);
        let mut left = count;
        loop {
            if left != i64::MAX {
                left -= 1;
                if left < 0 {
                    break;
                }
            }
            let meta = sb_sbumpc::<C>(sb);
            if C::is_eof(meta) {
                state |= EOFBIT;
                break;
            }
            bump_gcount(is);
            if meta == delim {
                break;
            }
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

/// Champ d'un flottant décimal (`_Parse_fp_with_locale`, locale « C ») :
/// signe, chiffres, point, chiffres, exposant. Rend le texte lu.
unsafe fn read_float_field<C: Ch>(b: P, state: &mut i32) -> Vec<u8> {
    let sb: P = rd(b, BIOS_STRBUF);
    let mut out = Vec::new();
    let mut meta = sb_sgetc::<C>(sb);
    let cur = |m: C::I| -> Option<u8> {
        if C::is_eof(m) {
            None
        } else {
            u8::try_from(C::from_int(m).code()).ok()
        }
    };
    let take = |out: &mut Vec<u8>, c: u8, meta: &mut C::I| {
        out.push(c);
        *meta = sb_snextc::<C>(sb);
    };
    if let Some(c @ (b'+' | b'-')) = cur(meta) {
        take(&mut out, c, &mut meta);
    }
    let mut digits = 0;
    while let Some(c @ b'0'..=b'9') = cur(meta) {
        take(&mut out, c, &mut meta);
        digits += 1;
    }
    if cur(meta) == Some(b'.') {
        take(&mut out, b'.', &mut meta);
        while let Some(c @ b'0'..=b'9') = cur(meta) {
            take(&mut out, c, &mut meta);
            digits += 1;
        }
    }
    if digits > 0 {
        if let Some(c @ (b'e' | b'E')) = cur(meta) {
            take(&mut out, c, &mut meta);
            if let Some(s @ (b'+' | b'-')) = cur(meta) {
                take(&mut out, s, &mut meta);
            }
            while let Some(c @ b'0'..=b'9') = cur(meta) {
                take(&mut out, c, &mut meta);
            }
        }
    } else {
        out.clear();
    }
    if C::is_eof(meta) {
        *state |= EOFBIT;
    }
    out
}

/// `operator>>(double&)` / `operator>>(float&)`.
unsafe fn is_extract_float<C: Ch>(is: P, store: impl FnOnce(&[u8], &mut i32)) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    if is_sentry::<C>(is, false) {
        let field = read_float_field::<C>(b, &mut state);
        store(&field, &mut state);
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

/// Conversion d'un champ : (valeur, conversion complète et sans ERANGE).
fn convertir(field: &[u8], simple: bool) -> (f64, bool) {
    if field.is_empty() {
        return (0.0, false);
    }
    let mut z = field.to_vec();
    z.push(0);
    unsafe {
        *libc::__errno_location() = 0;
        let mut end: *mut libc::c_char = std::ptr::null_mut();
        let p = z.as_ptr() as *const libc::c_char;
        let v = if simple {
            libc::strtof(p, &mut end) as f64
        } else {
            libc::strtod(p, &mut end)
        };
        let ok = end as usize == p as usize + field.len() && *libc::__errno_location() == 0;
        (v, ok)
    }
}

unsafe extern "win64" fn is_double<C: Ch>(is: P, v: *mut f64) -> P {
    is_extract_float::<C>(is, |field, state| {
        let (x, ok) = convertir(field, false);
        *v = if field.is_empty() { 0.0 } else { x };
        if !ok {
            *state |= FAILBIT;
        }
    })
}

unsafe extern "win64" fn is_float<C: Ch>(is: P, v: *mut f32) -> P {
    is_extract_float::<C>(is, |field, state| {
        let (x, ok) = convertir(field, true);
        *v = if field.is_empty() { 0.0 } else { x as f32 };
        if !ok {
            *state |= FAILBIT;
        }
    })
}

/// `operator>>(bool&)` : 0 ou 1, ou « false » / « true » avec `boolalpha`.
unsafe extern "win64" fn is_bool<C: Ch>(is: P, v: *mut bool) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    if is_sentry::<C>(is, false) {
        if rd::<i32>(b, IOS_FMTFL) & BOOLALPHA != 0 {
            let sb: P = rd(b, BIOS_STRBUF);
            let noms: [&[u8]; 2] = [b"false", b"true"];
            let mut vivants = [true, true];
            let mut lus = 0;
            let mut meta = sb_sgetc::<C>(sb);
            loop {
                if C::is_eof(meta) {
                    state |= EOFBIT;
                    break;
                }
                let c = C::from_int(meta).code();
                let mut garde = false;
                for (i, nom) in noms.iter().enumerate() {
                    vivants[i] &= nom.get(lus).is_some_and(|&n| n as u32 == c);
                    garde |= vivants[i];
                }
                if !garde {
                    break;
                }
                lus += 1;
                meta = sb_snextc::<C>(sb);
                if noms.iter().zip(vivants).any(|(n, a)| a && n.len() == lus) {
                    break;
                }
            }
            match (0..2).find(|&i| vivants[i] && noms[i].len() == lus) {
                Some(i) => *v = i == 1,
                None => {
                    *v = false;
                    state |= FAILBIT;
                }
            }
        } else {
            match read_integer::<C>(b, &mut state) {
                None => {
                    *v = false;
                    state |= FAILBIT;
                }
                Some((neg, mag)) => {
                    *v = mag != 0;
                    if neg && mag != 0 || mag > 1 {
                        state |= FAILBIT;
                    }
                }
            }
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

/// `basic_istream::swap` : `basic_ios::swap`, puis le compte de `gcount`.
unsafe extern "win64" fn is_swap<C: Ch>(is: P, right: P) {
    bios_swap::<C>(vbase(is), vbase(right));
    let (x, y): (i64, i64) = (rd(is, IS_CHCOUNT), rd(right, IS_CHCOUNT));
    wr(is, IS_CHCOUNT, y);
    wr(right, IS_CHCOUNT, x);
}

// ---------- basic_ostream<char> ----------

/// `??_D` : destructeur de l'objet complet, base virtuelle comprise.
unsafe extern "win64" fn os_vbase_dtor<C: Ch>(os: P) {
    let b = vbase(os);
    stream_dtor(b);
    bios_dtor::<C>(b);
}

/// `operator<<(short)` : en octal et en hexadécimal, valeur non signée.
unsafe extern "win64" fn os_short<C: Ch>(os: P, v: i16) -> P {
    os_insert::<C>(os, |f, _| {
        if matches!(f & BASEFIELD, OCT | HEX) {
            fmt_int(f, v as u16 as i128, 32, true)
        } else {
            fmt_int(f, v as i128, 32, true)
        }
    })
}

/// `operator<<(float)` : comme un `double`.
unsafe extern "win64" fn os_float<C: Ch>(os: P, v: f32) -> P {
    os_insert::<C>(os, |f, prec| fmt_double(f, prec, v as f64))
}

/// `operator<<(basic_streambuf*)` : copie jusqu'à la fin du tampon source.
unsafe extern "win64" fn os_streambuf<C: Ch>(os: P, src: P) -> P {
    let b = vbase(os);
    let mut state = GOODBIT;
    let mut copied = false;
    if os_sentry::<C>(os) && !src.is_null() {
        let dst: P = rd(b, BIOS_STRBUF);
        let mut meta = C::eof();
        loop {
            meta = if C::is_eof(meta) {
                sb_sgetc::<C>(src)
            } else {
                sb_snextc::<C>(src)
            };
            if C::is_eof(meta) {
                break;
            }
            if C::is_eof(sb_sputc::<C>(dst, C::from_int(meta))) {
                state |= BADBIT;
                break;
            }
            copied = true;
        }
    }
    wr(b, IOS_WIDE, 0i64);
    let fin = if src.is_null() {
        BADBIT
    } else if copied {
        state
    } else {
        state | FAILBIT
    };
    ios_setstate::<C>(b, fin);
    os_sentry_end::<C>(os);
    os
}

/// `seekp(off, way)`.
unsafe extern "win64" fn os_seekp_off<C: Ch>(os: P, off: i64, way: i32) -> P {
    let b = vbase(os);
    os_sentry::<C>(os);
    if !ios_fail(b) && sb_pubseekoff(rd(b, BIOS_STRBUF), off, way, MODE_OUT).offset() == -1 {
        ios_setstate::<C>(b, FAILBIT);
    }
    os_sentry_end::<C>(os);
    os
}

/// `tellp()` : position courante d'écriture, ou -1.
unsafe extern "win64" fn os_tellp<C: Ch>(os: P, out: *mut Fpos) -> *mut Fpos {
    let b = vbase(os);
    os_sentry::<C>(os);
    *out = if !ios_fail(b) {
        sb_pubseekoff(rd(b, BIOS_STRBUF), 0, 1, MODE_OUT)
    } else {
        Fpos::at(-1)
    };
    os_sentry_end::<C>(os);
    out
}

/// `basic_ostream::swap` : `basic_ios::swap`.
unsafe extern "win64" fn os_swap<C: Ch>(os: P, right: P) {
    if os != right {
        bios_swap::<C>(vbase(os), vbase(right));
    }
}

/// `basic_iostream::swap` : `basic_istream::swap`.
unsafe extern "win64" fn iostream_swap<C: Ch>(ios: P, right: P) {
    if ios != right {
        is_swap::<C>(ios, right);
    }
}

// ---------- locale::id, call_once, divers ----------

/// `locale::id::operator size_t` : numéro attribué au premier usage.
unsafe extern "win64" fn id_to_size(id: *const AtomicUsize) -> usize {
    id_index(&*id)
}

/// `std::_Execute_once(once_flag&, rappel, contexte)` : `InitOnceExecuteOnce`
/// sur le mot opaque de `once_flag`.
unsafe extern "win64" fn execute_once(flag: usize, callback: usize, pv: usize) -> i32 {
    crate::dlls::kernel32::init_once_execute_once(flag, callback, pv, std::ptr::null_mut())
}

/// `std::_Raise_handler` : gestionnaire de `stdext::exception::_Raise`,
/// nul par défaut.
static RAISE_HANDLER: AtomicUsize = AtomicUsize::new(0);

/// `_Dtest(double*)` : `fpclassify` de la CRT Microsoft (FP_INFINITE 1,
/// FP_NAN 2, FP_NORMAL -1, FP_SUBNORMAL -2, FP_ZERO 0).
unsafe extern "win64" fn dtest(p: *const f64) -> i16 {
    use std::num::FpCategory::*;
    match p.read_unaligned().classify() {
        Infinite => 1,
        Nan => 2,
        Normal => -1,
        Subnormal => -2,
        Zero => 0,
    }
}

unsafe extern "win64" fn throw_future_error(code: *const i32) {
    let msg = format!("code {}\0", *code);
    super::super::msvcp140::fatal("future_error", msg.as_ptr());
}

unsafe extern "win64" fn x_get_last_error() {
    let code = match crate::dlls::lookup("kernel32", "GetLastError") {
        Some(f) => {
            let f: extern "win64" fn() -> u32 = std::mem::transmute(f);
            f()
        }
        None => 0,
    };
    let msg = format!("GetLastError {code}\0");
    super::super::msvcp140::fatal("system_error", msg.as_ptr());
}

unsafe extern "win64" fn x_regex_error(code: i32) {
    let msg = format!("error_type {code}\0");
    super::super::msvcp140::fatal("regex_error", msg.as_ptr());
}

pub(super) fn exports() -> Exports {
    crate::exports! {
        "?underflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ" => fn sb_underflow::<u8>,
        "?overflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHH@Z" => fn sb_eof::<u8>,
        "?pbackfail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHH@Z" => fn sb_eof::<u8>,
        "?seekoff@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA?AV?$fpos@U_Mbstatet@@@2@_JHH@Z" => fn sb_seekoff,
        "?seekpos@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA?AV?$fpos@U_Mbstatet@@@2@V32@H@Z" => fn sb_seekpos,
        "?swap@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z" => fn sb_swap::<u8>,
        "??4?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAAEAV01@AEBV01@@Z" => fn sb_assign::<u8>,
        "??0?$basic_ios@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z" => fn bios_ctor_sb::<u8>,
        "?copyfmt@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAAEAV12@AEBV12@@Z" => fn bios_copyfmt::<u8>,
        "?rdbuf@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAPEAV?$basic_streambuf@DU?$char_traits@D@std@@@2@PEAV32@@Z" => fn bios_set_rdbuf::<u8>,
        "?get@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAHXZ" => fn is_get::<u8>,
        "?get@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEAD_J@Z" => fn is_get_str::<u8>,
        "?peek@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAHXZ" => fn is_peek::<u8>,
        "?ignore@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@_JH@Z" => fn is_ignore::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAM@Z" => fn is_float::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAN@Z" => fn is_double::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEA_N@Z" => fn is_bool::<u8>,
        "?swap@?$basic_istream@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z" => fn is_swap::<u8>,
        "??_D?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAXXZ" => fn os_vbase_dtor::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@F@Z" => fn os_short::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@M@Z" => fn os_float::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z" => fn os_streambuf::<u8>,
        "?seekp@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@_JH@Z" => fn os_seekp_off::<u8>,
        "?tellp@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAA?AV?$fpos@U_Mbstatet@@@2@XZ" => fn os_tellp::<u8>,
        "?swap@?$basic_ostream@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z" => fn os_swap::<u8>,
        "?swap@?$basic_iostream@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z" => fn iostream_swap::<u8>,
        "??Bid@locale@std@@QEAA_KXZ" => fn id_to_size,
        "?_Execute_once@std@@YAHAEAUonce_flag@1@P6AHPEAX1PEAPEAX@Z1@Z" => fn execute_once,
        "?_Raise_handler@std@@3P6AXAEBVexception@stdext@@@ZEA" => data RAISE_HANDLER,
        "_Dtest" => fn dtest,
        "?_Throw_future_error@std@@YAXAEBVerror_code@1@@Z" => fn throw_future_error,
        "?_XGetLastError@std@@YAXXZ" => fn x_get_last_error,
        "?_Xregex_error@std@@YAXW4error_type@regex_constants@1@@Z" => fn x_regex_error,
    }
}

#[cfg(test)]
mod tests;
