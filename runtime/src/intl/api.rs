//! API Win32 de langue, de texte, de clavier et d'heure branchées sur les
//! réglages Linux (modules `codepage`, `locale`, `keyboard`, `tz`).

use super::codepage::{self, CP_UTF8};
use super::locale::{self, DateTime, Locale};
use super::tz::{self, Rule, SystemTime, Zone};
use crate::process::set_last_error;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_FLAGS: u32 = 1004;
const ERROR_NO_UNICODE_TRANSLATION: u32 = 1113;

// ---------------------------------------------------------------------
// Tampons de sortie (convention « taille avec le zéro final »)
// ---------------------------------------------------------------------

/// Copie une chaîne Unicode ; `cap` = 0 demande la taille nécessaire.
unsafe fn out_w(s: &str, buf: *mut u16, cap: i32) -> i32 {
    let mut w: Vec<u16> = s.encode_utf16().collect();
    w.push(0);
    if cap == 0 {
        return w.len() as i32;
    }
    if buf.is_null() || cap < 0 || (cap as usize) < w.len() {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    w.len() as i32
}

/// Copie une chaîne dans la page ANSI de la locale.
unsafe fn out_a(s: &str, buf: *mut u8, cap: i32) -> i32 {
    let cp = codepage::resolve(codepage::CP_ACP);
    let mut b = codepage::encode(cp, s, b'?', true)
        .map(|e| e.bytes)
        .unwrap_or_default();
    b.push(0);
    if cap == 0 {
        return b.len() as i32;
    }
    if buf.is_null() || cap < 0 || (cap as usize) < b.len() {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    b.len() as i32
}

unsafe fn wide_opt(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::wstr(p))
}

// ---------------------------------------------------------------------
// Pages de code
// ---------------------------------------------------------------------

pub extern "win64" fn get_acp() -> u32 {
    locale::current().ansi_cp
}
pub extern "win64" fn get_oemcp() -> u32 {
    locale::current().oem_cp
}
pub extern "win64" fn is_valid_code_page(cp: u32) -> i32 {
    codepage::is_valid(cp) as i32
}

/// CPINFO : taille maximale d'un caractère, caractère par défaut, octets de
/// tête (aucun : Winlux ne décrit que des pages à un octet et UTF-8).
pub unsafe extern "win64" fn get_cp_info(cp: u32, info: *mut u8) -> i32 {
    let cp = codepage::resolve(cp);
    if info.is_null() || !codepage::is_valid(cp) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    std::ptr::write_bytes(info, 0, 20);
    (info as *mut u32).write_unaligned(if cp == CP_UTF8 { 4 } else { 1 });
    *info.add(4) = b'?';
    1
}

fn code_page_name(cp: u32) -> String {
    let label = match cp {
        437 => "(OEM - United States)",
        850 => "(OEM - Multilingual Latin I)",
        858 => "(OEM - Multilingual Latin I + Euro)",
        1252 => "(ANSI - Latin I)",
        20127 => "(US-ASCII)",
        28591 => "(ISO 8859-1 Latin I)",
        28605 => "(ISO 8859-15 Latin 9)",
        65001 => "(UTF-8)",
        _ => "",
    };
    format!("{cp}  {label}")
}

/// CPINFOEXW : CPINFO, caractère Unicode par défaut, page, nom.
pub unsafe extern "win64" fn get_cp_info_ex_w(cp: u32, _flags: u32, info: *mut u8) -> i32 {
    let cp = codepage::resolve(cp);
    if get_cp_info(cp, info) == 0 {
        return 0;
    }
    std::ptr::write_bytes(info.add(20), 0, 548 - 20);
    (info.add(20) as *mut u16).write_unaligned(0x3F);
    (info.add(24) as *mut u32).write_unaligned(cp);
    let name: Vec<u16> = code_page_name(cp).encode_utf16().take(259).collect();
    std::ptr::copy_nonoverlapping(name.as_ptr() as *const u8, info.add(28), name.len() * 2);
    1
}

pub unsafe extern "win64" fn get_cp_info_ex_a(cp: u32, flags: u32, info: *mut u8) -> i32 {
    let cp = codepage::resolve(cp);
    if get_cp_info(cp, info) == 0 {
        return 0;
    }
    let _ = flags;
    std::ptr::write_bytes(info.add(20), 0, 288 - 20);
    (info.add(20) as *mut u16).write_unaligned(0x3F);
    (info.add(24) as *mut u32).write_unaligned(cp);
    let name = code_page_name(cp);
    let n = name.len().min(259);
    std::ptr::copy_nonoverlapping(name.as_ptr(), info.add(28), n);
    1
}

pub unsafe extern "win64" fn multi_byte_to_wide_char(
    cp: u32,
    flags: u32,
    s: *const u8,
    n: i32,
    w: *mut u16,
    wn: i32,
) -> i32 {
    const MB_ERR_INVALID_CHARS: u32 = 8;
    if s.is_null() || n == 0 || n < -1 || wn < 0 || (wn > 0 && w.is_null()) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let cp = codepage::resolve(cp);
    if !codepage::is_valid(cp) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let allowed = if cp == CP_UTF8 {
        MB_ERR_INVALID_CHARS
    } else {
        0x0F
    };
    if flags & !allowed != 0 {
        set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    }
    let count = if n == -1 {
        crate::printf::narrow_len(s) + 1
    } else {
        n as usize
    };
    let bytes = std::slice::from_raw_parts(s, count);
    let Ok(text) = codepage::decode(cp, bytes, flags & MB_ERR_INVALID_CHARS != 0) else {
        set_last_error(ERROR_NO_UNICODE_TRANSLATION);
        return 0;
    };
    let out: Vec<u16> = text.encode_utf16().collect();
    if out.len() > i32::MAX as usize {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if wn == 0 {
        return out.len() as i32;
    }
    if (wn as usize) < out.len() {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), w, out.len());
    out.len() as i32
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn wide_char_to_multi_byte(
    cp: u32,
    flags: u32,
    w: *const u16,
    n: i32,
    out: *mut u8,
    cap: i32,
    default: *const u8,
    used: *mut i32,
) -> i32 {
    const WC_ERR_INVALID_CHARS: u32 = 0x80;
    const WC_NO_BEST_FIT_CHARS: u32 = 0x400;
    if w.is_null() || n == 0 || n < -1 || cap < 0 || (cap > 0 && out.is_null()) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let cp = codepage::resolve(cp);
    if !codepage::is_valid(cp) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if cp == CP_UTF8 {
        if flags & !WC_ERR_INVALID_CHARS != 0 {
            set_last_error(ERROR_INVALID_FLAGS);
            return 0;
        }
        if !default.is_null() || !used.is_null() {
            set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
    } else if flags & !(0x400 | 0x200 | 0x40 | 0x20 | 0x10) != 0 {
        set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    }
    let count = if n == -1 {
        crate::printf::wide_len(w) + 1
    } else {
        n as usize
    };
    let units = std::slice::from_raw_parts(w, count);
    let text = if flags & WC_ERR_INVALID_CHARS != 0 {
        match String::from_utf16(units) {
            Ok(s) => s,
            Err(_) => {
                set_last_error(ERROR_NO_UNICODE_TRANSLATION);
                return 0;
            }
        }
    } else {
        String::from_utf16_lossy(units)
    };
    let def = if default.is_null() { b'?' } else { *default };
    let Ok(enc) = codepage::encode(cp, &text, def, flags & WC_NO_BEST_FIT_CHARS == 0) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if !used.is_null() {
        *used = i32::from(enc.used_default);
    }
    if enc.bytes.len() > i32::MAX as usize {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if cap == 0 {
        return enc.bytes.len() as i32;
    }
    if (cap as usize) < enc.bytes.len() {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(enc.bytes.as_ptr(), out, enc.bytes.len());
    enc.bytes.len() as i32
}

/// CharToOem / OemToChar (user32) : ANSI ↔ OEM, sur place si src == dst.
unsafe fn convert_buffer(src: *const u8, dst: *mut u8, len: Option<usize>, to_oem: bool) -> i32 {
    if src.is_null() || dst.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = len.unwrap_or_else(|| crate::printf::narrow_len(src));
    let bytes = std::slice::from_raw_parts(src, n).to_vec();
    let (from, to) = if to_oem {
        (codepage::resolve(0), codepage::resolve(1))
    } else {
        (codepage::resolve(1), codepage::resolve(0))
    };
    let text = codepage::decode(from, &bytes, false).unwrap_or_default();
    let enc = codepage::encode(to, &text, b'?', true)
        .map(|e| e.bytes)
        .unwrap_or_default();
    let m = enc.len().min(n);
    std::ptr::copy_nonoverlapping(enc.as_ptr(), dst, m);
    if len.is_none() {
        *dst.add(m) = 0;
    }
    1
}

unsafe fn convert_wide_to(src: *const u16, dst: *mut u8, len: Option<usize>, cp: u32) -> i32 {
    if src.is_null() || dst.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = len.unwrap_or_else(|| crate::printf::wide_len(src));
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(src, n));
    let enc = codepage::encode(cp, &text, b'?', true)
        .map(|e| e.bytes)
        .unwrap_or_default();
    std::ptr::copy_nonoverlapping(enc.as_ptr(), dst, enc.len());
    if len.is_none() {
        *dst.add(enc.len()) = 0;
    }
    1
}

unsafe fn convert_to_wide(src: *const u8, dst: *mut u16, len: Option<usize>, cp: u32) -> i32 {
    if src.is_null() || dst.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = len.unwrap_or_else(|| crate::printf::narrow_len(src));
    let text = codepage::decode(cp, std::slice::from_raw_parts(src, n), false).unwrap_or_default();
    let w: Vec<u16> = text.encode_utf16().collect();
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    if len.is_none() {
        *dst.add(w.len()) = 0;
    }
    1
}

pub unsafe extern "win64" fn char_to_oem_a(src: *const u8, dst: *mut u8) -> i32 {
    convert_buffer(src, dst, None, true)
}
pub unsafe extern "win64" fn oem_to_char_a(src: *const u8, dst: *mut u8) -> i32 {
    convert_buffer(src, dst, None, false)
}
pub unsafe extern "win64" fn char_to_oem_buff_a(src: *const u8, dst: *mut u8, n: u32) -> i32 {
    convert_buffer(src, dst, Some(n as usize), true)
}
pub unsafe extern "win64" fn oem_to_char_buff_a(src: *const u8, dst: *mut u8, n: u32) -> i32 {
    convert_buffer(src, dst, Some(n as usize), false)
}
pub unsafe extern "win64" fn char_to_oem_w(src: *const u16, dst: *mut u8) -> i32 {
    convert_wide_to(src, dst, None, codepage::resolve(1))
}
pub unsafe extern "win64" fn char_to_oem_buff_w(src: *const u16, dst: *mut u8, n: u32) -> i32 {
    convert_wide_to(src, dst, Some(n as usize), codepage::resolve(1))
}
pub unsafe extern "win64" fn oem_to_char_w(src: *const u8, dst: *mut u16) -> i32 {
    convert_to_wide(src, dst, None, codepage::resolve(1))
}
pub unsafe extern "win64" fn oem_to_char_buff_w(src: *const u8, dst: *mut u16, n: u32) -> i32 {
    convert_to_wide(src, dst, Some(n as usize), codepage::resolve(1))
}

/// IsTextUnicode (advapi32) : vrai pour un tampon UTF-16 LE.
pub unsafe extern "win64" fn is_text_unicode(buf: *const u8, len: i32, result: *mut i32) -> i32 {
    if buf.is_null() || len < 2 {
        return 0;
    }
    let data = std::slice::from_raw_parts(buf, len as usize);
    let yes = super::text::detect(data, 1252) == super::text::Encoding::Utf16Le;
    if !result.is_null() {
        // IS_TEXT_UNICODE_STATISTICS (2) ou SIGNATURE (8).
        *result = if !yes {
            0
        } else if data.starts_with(&[0xFF, 0xFE]) {
            8
        } else {
            2
        };
    }
    yes as i32
}

// ---------------------------------------------------------------------
// Locales
// ---------------------------------------------------------------------

/// LCID → locale (LOCALE_USER_DEFAULT, LOCALE_SYSTEM_DEFAULT… compris).
fn resolve_lcid(lcid: u32) -> Option<&'static Locale> {
    match lcid & 0xFFFF {
        0 | 0x0400 | 0x0800 | 0x0C00 | 0x1000 => Some(locale::current()),
        0x1400 => Some(locale::ui_locale()),
        id => locale::from_lcid(id),
    }
}

/// Nom de locale → locale (NULL = utilisateur, "" = invariante).
unsafe fn resolve_name(name: *const u16) -> Option<&'static Locale> {
    match wide_opt(name) {
        None => Some(locale::current()),
        Some(n) if n.eq_ignore_ascii_case("!x-sys-default-locale") => Some(locale::current()),
        Some(n) => locale::from_tag(&n),
    }
}

pub extern "win64" fn get_user_default_lcid() -> u32 {
    locale::current().lcid
}
pub extern "win64" fn get_user_default_lang_id() -> u16 {
    locale::current().lcid as u16
}
pub extern "win64" fn get_user_default_ui_language() -> u16 {
    locale::ui_locale().lcid as u16
}
pub extern "win64" fn set_thread_locale(lcid: u32) -> i32 {
    resolve_lcid(lcid).is_some() as i32
}
pub extern "win64" fn get_user_geo_id(_class: u32) -> i32 {
    locale::current().geo_id as i32
}
pub unsafe extern "win64" fn get_user_default_locale_name(buf: *mut u16, cap: i32) -> i32 {
    out_w(locale::current().name, buf, cap)
}
pub unsafe extern "win64" fn locale_name_to_lcid(name: *const u16, _flags: u32) -> u32 {
    match resolve_name(name) {
        Some(l) => l.lcid,
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}
pub unsafe extern "win64" fn lcid_to_locale_name(
    lcid: u32,
    buf: *mut u16,
    cap: i32,
    _f: u32,
) -> i32 {
    match resolve_lcid(lcid) {
        Some(l) => out_w(l.name, buf, cap),
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}
pub extern "win64" fn is_valid_locale(lcid: u32, _flags: u32) -> i32 {
    resolve_lcid(lcid).is_some() as i32
}
pub unsafe extern "win64" fn is_valid_locale_name(name: *const u16) -> i32 {
    (!name.is_null() && resolve_name(name).is_some()) as i32
}

/// Get{User,System,Thread,Process}PreferredUILanguages : langue de
/// Paramètres, en liste multi-chaîne. MUI_LANGUAGE_ID (4) la donne en
/// hexadécimal (« 040C »).
pub unsafe extern "win64" fn get_preferred_ui_languages(
    flags: u32,
    count: *mut u32,
    buf: *mut u16,
    len: *mut u32,
) -> i32 {
    if count.is_null() || len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let ui = locale::ui_locale();
    let item = if flags & 4 != 0 {
        format!("{:04X}", ui.lcid)
    } else {
        ui.name.to_string()
    };
    let mut wide: Vec<u16> = item.encode_utf16().collect();
    wide.extend([0, 0]);
    *count = 1;
    if buf.is_null() {
        *len = wide.len() as u32;
        return 1;
    }
    if (*len as usize) < wide.len() {
        *len = wide.len() as u32;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    *len = wide.len() as u32;
    1
}

/// Valeur d'un LCTYPE : texte ou nombre (LOCALE_I* acceptent
/// LOCALE_RETURN_NUMBER).
enum Info {
    Text(String),
    Number(u32),
}

fn date_order(picture: &str) -> u32 {
    let pos = |c: char| picture.find(c).unwrap_or(usize::MAX);
    let (d, m, y) = (pos('d'), pos('M'), pos('y'));
    if m < d && d < y {
        0
    } else if y < m {
        2
    } else {
        1
    }
}

fn separator(picture: &str) -> String {
    picture
        .chars()
        .find(|c| !c.is_alphanumeric() && *c != '\'')
        .map(String::from)
        .unwrap_or_default()
}

fn locale_info(l: &Locale, lctype: u32) -> Option<Info> {
    use Info::{Number as N, Text as T};
    let lang = l.lang;
    let display = |native: bool| {
        if l.lcid == 0x7F {
            return lang.english.to_string();
        }
        if native {
            format!("{} ({})", lang.native, l.country_native)
        } else {
            format!("{} ({})", lang.english, l.country_english)
        }
    };
    // LOCALE_SABBREVLANGNAME : codes historiques de Windows.
    let abbr3 = || {
        match l.name {
            "fr-FR" => "FRA",
            "fr-BE" => "FRB",
            "fr-CH" => "FRS",
            "fr-CA" => "FRC",
            "en-US" => "ENU",
            "en-GB" => "ENG",
            "es-ES" => "ESN",
            "es-MX" => "ESM",
            "de-DE" => "DEU",
            "de-AT" => "DEA",
            "de-CH" => "DES",
            "it-IT" => "ITA",
            "pt-PT" => "PTG",
            "pt-BR" => "PTB",
            _ => "IVL",
        }
        .to_string()
    };
    let cur_first = matches!(l.currency_pos, 0 | 2);
    let cur_space = matches!(l.currency_pos, 2 | 3);
    Some(match lctype {
        0x01 => T(format!("{:04x}", l.lcid)), // LOCALE_ILANGUAGE
        0x02 => T(display(true)),             // LOCALE_SLOCALIZEDDISPLAYNAME
        0x03 => T(abbr3()),                   // LOCALE_SABBREVLANGNAME
        0x04 => T(lang.native.into()),        // LOCALE_SNATIVELANGUAGENAME
        0x05 => N(match l.iso3166 {
            "US" | "CA" => 1,
            "FR" => 33,
            "BE" => 32,
            "CH" => 41,
            "GB" => 44,
            "ES" => 34,
            "MX" => 52,
            "DE" => 49,
            "AT" => 43,
            "IT" => 39,
            "PT" => 351,
            "BR" => 55,
            _ => 1,
        }), // LOCALE_ICOUNTRY
        0x06 => T(l.country_native.into()),   // LOCALE_SLOCALIZEDCOUNTRYNAME
        0x07 => T(l.iso3166_3.into()),        // LOCALE_SABBREVCTRYNAME
        0x08 => T(l.country_native.into()),   // LOCALE_SNATIVECOUNTRYNAME
        0x09 => T(format!("{:04x}", l.lcid)), // LOCALE_IDEFAULTLANGUAGE
        0x0A => return locale_info(l, 0x05),  // LOCALE_IDEFAULTCOUNTRY
        0x0B => N(l.oem_cp),                  // LOCALE_IDEFAULTCODEPAGE
        0x0C => T(l.list.into()),             // LOCALE_SLIST
        0x0D => N(l.measure),                 // LOCALE_IMEASURE
        0x0E | 0x16 => T(l.decimal.into()),   // LOCALE_SDECIMAL, SMONDECIMALSEP
        0x0F | 0x17 => T(l.thousand.into()),  // LOCALE_STHOUSAND, SMONTHOUSANDSEP
        0x10 | 0x18 => T("3;0".into()),       // LOCALE_SGROUPING, SMONGROUPING
        0x11 | 0x19 | 0x1A => N(2),           // LOCALE_IDIGITS, ICURRDIGITS…
        0x12 => N(1),                         // LOCALE_ILZERO
        0x13 => T("0123456789".into()),       // LOCALE_SNATIVEDIGITS
        0x14 => T(l.currency.into()),         // LOCALE_SCURRENCY
        0x15 => T(l.int_currency.into()),     // LOCALE_SINTLSYMBOL
        0x1B => N(l.currency_pos),            // LOCALE_ICURRENCY
        0x1C => N(match l.currency_pos {
            0 => 1,
            2 => 9,
            _ => 8,
        }), // LOCALE_INEGCURR
        0x1D => T(separator(l.short_date)),   // LOCALE_SDATE
        0x1E => T(separator(l.time_format)),  // LOCALE_STIME
        0x1F => T(l.short_date.into()),       // LOCALE_SSHORTDATE
        0x20 => T(l.long_date.into()),        // LOCALE_SLONGDATE
        0x21 => N(date_order(l.short_date)),  // LOCALE_IDATE
        0x22 => N(date_order(l.long_date)),   // LOCALE_ILDATE
        0x23 => N(l.time_format.contains('H') as u32), // LOCALE_ITIME
        0x24 => N(l.short_date.contains("yyyy") as u32), // LOCALE_ICENTURY
        0x25 => N(l.time_format.contains("HH") as u32 | l.time_format.contains("hh") as u32), // LOCALE_ITLZERO
        0x26 => N(l.short_date.contains("dd") as u32), // LOCALE_IDAYLZERO
        0x27 => N(l.short_date.contains("MM") as u32), // LOCALE_IMONLZERO
        0x28 => T(lang.am.into()),                     // LOCALE_S1159
        0x29 => T(lang.pm.into()),                     // LOCALE_S2359
        0x2A..=0x30 => T(lang.days[(lctype - 0x2A) as usize].into()),
        0x31..=0x37 => T(lang.days_abbr[(lctype - 0x31) as usize].into()),
        0x38..=0x43 => T(lang.months[(lctype - 0x38) as usize].into()),
        0x44..=0x4F => T(lang.months_abbr[(lctype - 0x44) as usize].into()),
        0x100E | 0x100F => T(String::new()), // LOCALE_SMONTHNAME13…
        0x50 => T(String::new()),            // LOCALE_SPOSITIVESIGN
        0x51 => T("-".into()),               // LOCALE_SNEGATIVESIGN
        0x52 | 0x53 => N(if cur_first { 3 } else { 1 }), // LOCALE_IPOSSIGNPOSN…
        0x54 | 0x56 => N(cur_first as u32),  // LOCALE_IPOSSYMPRECEDES…
        0x55 | 0x57 => N(cur_space as u32),  // LOCALE_IPOSSEPBYSPACE…
        0x59 => T(lang.iso639.into()),       // LOCALE_SISO639LANGNAME
        0x5A => T(l.iso3166.into()),         // LOCALE_SISO3166CTRYNAME
        0x5B => N(l.geo_id),                 // LOCALE_IGEOID
        0x5C => T(l.name.into()),            // LOCALE_SNAME
        0x5D => T("h:mm:ss".into()),         // LOCALE_SDURATION
        0x5E => T(format!("{:04x}:{:08x}", l.lcid, l.keyboard)), // LOCALE_SKEYBOARDSTOINSTALL
        0x60..=0x66 => T(lang.days_abbr[(lctype - 0x60) as usize]
            .chars()
            .take(2)
            .collect()), // LOCALE_SSHORTESTDAYNAME1…
        0x67 => T(lang.iso639_3.to_ascii_lowercase()), // LOCALE_SISO639LANGNAME2
        0x68 => T(l.iso3166_3.into()),       // LOCALE_SISO3166CTRYNAME2
        0x69 => T("NaN".into()),             // LOCALE_SNAN
        0x6A => T("\u{221e}".into()),        // LOCALE_SPOSINFINITY
        0x6B => T("-\u{221e}".into()),       // LOCALE_SNEGINFINITY
        0x6C => T("Latn;".into()),           // LOCALE_SSCRIPTS
        0x6D => T(lang.iso639.into()),       // LOCALE_SPARENT
        0x6E => T(l.name.into()),            // LOCALE_SCONSOLEFALLBACKNAME
        0x6F => T(display(true)),            // LOCALE_SNATIVELANGUAGENAME (affichage)
        0x70 | 0x71 => N(0),                 // LOCALE_IREADINGLAYOUT, INEUTRAL
        0x72 => T(display(false)),           // LOCALE_SENGLISHDISPLAYNAME
        0x73 => T(display(true)),            // LOCALE_SNATIVEDISPLAYNAME
        0x74 => N(if l.lang.iso639 == "en" { 1 } else { 0 }), // LOCALE_INEGATIVEPERCENT
        0x75 => N(if l.lang.iso639 == "en" { 1 } else { 0 }), // LOCALE_IPOSITIVEPERCENT
        0x76 => T("%".into()),               // LOCALE_SPERCENT
        0x77 => T("\u{2030}".into()),        // LOCALE_SPERMILLE
        0x78 => T(if l.lang.iso639 == "en" {
            "MMMM d"
        } else {
            "d MMMM"
        }
        .into()), // LOCALE_SMONTHDAY
        0x79 => T(l.short_time.into()),      // LOCALE_SSHORTTIME
        0x7A => T(lang.iso639_3.to_ascii_uppercase()), // LOCALE_SOPENTYPELANGUAGETAG
        0x7B => T(l.name.into()),            // LOCALE_SSORTLOCALE
        0x1001 => T(lang.english.into()),    // LOCALE_SENGLISHLANGUAGENAME
        0x1002 => T(l.country_english.into()), // LOCALE_SENGLISHCOUNTRYNAME
        0x1003 => T(String::new()),          // LOCALE_STIMEMARKERPOSN… non utilisé
        0x1004 => N(l.ansi_cp),              // LOCALE_IDEFAULTANSICODEPAGE
        0x1005 => N(0),                      // LOCALE_ITIMEMARKPOSN
        0x1006 => T(l.year_month.into()),    // LOCALE_SYEARMONTH
        0x1007 | 0x1008 => T(l.int_currency.into()), // LOCALE_SENGCURRNAME, SNATIVECURRNAME
        0x1009 => N(1),                      // LOCALE_ICALENDARTYPE (grégorien)
        0x100A => N(if l.measure == 1 { 1 } else { 9 }), // LOCALE_IPAPERSIZE (Letter / A4)
        0x100B => N(0),                      // LOCALE_IOPTIONALCALENDAR
        0x100C => N(l.first_day),            // LOCALE_IFIRSTDAYOFWEEK
        0x100D => N(if l.first_day == 0 { 2 } else { 0 }), // LOCALE_IFIRSTWEEKOFYEAR
        0x1010 => N(1),                      // LOCALE_IDIGITSUBSTITUTION
        0x1011 => N(10000),                  // LOCALE_IDEFAULTMACCODEPAGE
        0x1012 => N(500),                    // LOCALE_IDEFAULTEBCDICCODEPAGE
        0x1013 => T(String::new()),          // LOCALE_SSORTNAME
        0x1014 => N(1),                      // LOCALE_IDIGITSUBSTITUTION
        _ => return None,
    })
}

const LOCALE_RETURN_NUMBER: u32 = 0x2000_0000;

fn info_for(l: &Locale, lctype: u32) -> Result<Info, u32> {
    let base = lctype & 0x0FFF_FFFF & !0x8000_0000;
    let v = locale_info(l, base).ok_or(ERROR_INVALID_FLAGS)?;
    Ok(match v {
        Info::Number(n) if lctype & LOCALE_RETURN_NUMBER == 0 => Info::Text(n.to_string()),
        Info::Text(t) if lctype & LOCALE_RETURN_NUMBER != 0 => {
            Info::Number(u32::from_str_radix(&t, 16).map_err(|_| ERROR_INVALID_FLAGS)?)
        }
        other => other,
    })
}

unsafe fn put_info_w(info: Info, buf: *mut u16, cap: i32) -> i32 {
    match info {
        Info::Text(t) => out_w(&t, buf, cap),
        Info::Number(n) => {
            if cap == 0 {
                return 2;
            }
            if buf.is_null() || cap < 2 {
                set_last_error(ERROR_INSUFFICIENT_BUFFER);
                return 0;
            }
            (buf as *mut u32).write_unaligned(n);
            2
        }
    }
}

pub unsafe extern "win64" fn get_locale_info_w(
    lcid: u32,
    lctype: u32,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let Some(l) = resolve_lcid(lcid) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    match info_for(l, lctype) {
        Ok(i) => put_info_w(i, buf, cap),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn get_locale_info_ex(
    name: *const u16,
    lctype: u32,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let Some(l) = resolve_name(name) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    match info_for(l, lctype) {
        Ok(i) => put_info_w(i, buf, cap),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn get_locale_info_a(
    lcid: u32,
    lctype: u32,
    buf: *mut u8,
    cap: i32,
) -> i32 {
    let Some(l) = resolve_lcid(lcid) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    match info_for(l, lctype) {
        Ok(Info::Text(t)) => out_a(&t, buf, cap),
        Ok(Info::Number(n)) => {
            if cap == 0 {
                return 4;
            }
            if buf.is_null() || cap < 4 {
                set_last_error(ERROR_INSUFFICIENT_BUFFER);
                return 0;
            }
            (buf as *mut u32).write_unaligned(n);
            4
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

// ---------------------------------------------------------------------
// Dates, heures et nombres mis en forme
// ---------------------------------------------------------------------

/// Date/heure à mettre en forme : celle fournie (validée, jour de la
/// semaine recalculé) ou l'heure locale actuelle.
unsafe fn date_input(st: *const SystemTime, check_date: bool) -> Option<DateTime> {
    let s = if st.is_null() {
        let now = now_filetime();
        let local = now - current_bias_ticks(now);
        tz::filetime_to_system(local)?
    } else {
        let mut s = std::ptr::read_unaligned(st);
        if check_date {
            let ft = tz::system_to_filetime(&s)?;
            s.day_of_week = tz::filetime_to_system(ft)?.day_of_week;
        } else if s.hour > 23 || s.minute > 59 || s.second > 59 || s.millis > 999 {
            return None;
        }
        s
    };
    Some(DateTime {
        year: s.year,
        month: s.month,
        day_of_week: s.day_of_week,
        day: s.day,
        hour: s.hour,
        minute: s.minute,
        second: s.second,
        millis: s.millis,
    })
}

fn date_picture(l: &Locale, flags: u32) -> Option<&'static str> {
    const DATE_SHORTDATE: u32 = 1;
    const DATE_LONGDATE: u32 = 2;
    const DATE_YEARMONTH: u32 = 8;
    const DATE_MONTHDAY: u32 = 0x80;
    let kind = flags & (DATE_SHORTDATE | DATE_LONGDATE | DATE_YEARMONTH | DATE_MONTHDAY);
    Some(match kind {
        0 | DATE_SHORTDATE => l.short_date,
        DATE_LONGDATE => l.long_date,
        DATE_YEARMONTH => l.year_month,
        DATE_MONTHDAY => {
            if l.lang.iso639 == "en" {
                "MMMM d"
            } else {
                "d MMMM"
            }
        }
        _ => return None,
    })
}

/// Image d'heure adaptée aux options TIME_NOSECONDS, TIME_FORCE24HOURFORMAT…
fn time_picture(base: &str, flags: u32) -> String {
    const TIME_NOMINUTESORSECONDS: u32 = 1;
    const TIME_NOSECONDS: u32 = 2;
    const TIME_NOTIMEMARKER: u32 = 4;
    const TIME_FORCE24HOURFORMAT: u32 = 8;
    let mut p = base.to_string();
    let drop_token = |p: &str, c: char| -> String {
        // Retire le jeton et le séparateur qui le précède (« :ss »).
        let mut out = String::new();
        let chars: Vec<char> = p.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == c {
                while out.ends_with(|ch: char| !ch.is_alphanumeric() && ch != '\'') {
                    out.pop();
                }
                while i < chars.len() && chars[i] == c {
                    i += 1;
                }
                continue;
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    };
    if flags & (TIME_NOSECONDS | TIME_NOMINUTESORSECONDS) != 0 {
        p = drop_token(&p, 's');
    }
    if flags & TIME_NOMINUTESORSECONDS != 0 {
        p = drop_token(&p, 'm');
    }
    if flags & (TIME_NOTIMEMARKER | TIME_FORCE24HOURFORMAT) != 0 {
        p = p.replace('t', "").trim().to_string();
    }
    if flags & TIME_FORCE24HOURFORMAT != 0 {
        p = p.replace('h', "H");
    }
    p
}

enum Picture {
    Given(String),
    Default,
}

unsafe fn format_date(
    l: &Locale,
    flags: u32,
    st: *const SystemTime,
    pic: Picture,
) -> Result<String, u32> {
    let t = date_input(st, true).ok_or(ERROR_INVALID_PARAMETER)?;
    let picture = match pic {
        Picture::Given(p) => p,
        Picture::Default => date_picture(l, flags)
            .ok_or(ERROR_INVALID_FLAGS)?
            .to_string(),
    };
    Ok(locale::format_picture(l, &picture, &t))
}

unsafe fn format_time(
    l: &Locale,
    flags: u32,
    st: *const SystemTime,
    pic: Picture,
) -> Result<String, u32> {
    let t = date_input(st, false).ok_or(ERROR_INVALID_PARAMETER)?;
    let picture = match pic {
        Picture::Given(p) => time_picture(&p, flags),
        Picture::Default => time_picture(l.time_format, flags),
    };
    Ok(locale::format_picture(l, &picture, &t))
}

unsafe fn finish_w(r: Result<String, u32>, buf: *mut u16, cap: i32) -> i32 {
    match r {
        Ok(s) => out_w(&s, buf, cap),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

unsafe fn finish_a(r: Result<String, u32>, buf: *mut u8, cap: i32) -> i32 {
    match r {
        Ok(s) => out_a(&s, buf, cap),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

unsafe fn picture_w(p: *const u16) -> Picture {
    wide_opt(p).map_or(Picture::Default, Picture::Given)
}

unsafe fn picture_a(p: *const u8) -> Picture {
    if p.is_null() {
        Picture::Default
    } else {
        Picture::Given(crate::dlls::cstr(p))
    }
}

macro_rules! with_lcid {
    ($lcid:expr) => {
        match resolve_lcid($lcid) {
            Some(l) => l,
            None => {
                set_last_error(ERROR_INVALID_PARAMETER);
                return 0;
            }
        }
    };
}

macro_rules! with_name {
    ($name:expr) => {
        match resolve_name($name) {
            Some(l) => l,
            None => {
                set_last_error(ERROR_INVALID_PARAMETER);
                return 0;
            }
        }
    };
}

pub unsafe extern "win64" fn get_date_format_w(
    lcid: u32,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u16,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let l = with_lcid!(lcid);
    finish_w(format_date(l, flags, st, picture_w(fmt)), buf, cap)
}

pub unsafe extern "win64" fn get_date_format_a(
    lcid: u32,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u8,
    buf: *mut u8,
    cap: i32,
) -> i32 {
    let l = with_lcid!(lcid);
    finish_a(format_date(l, flags, st, picture_a(fmt)), buf, cap)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn get_date_format_ex(
    name: *const u16,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u16,
    buf: *mut u16,
    cap: i32,
    _calendar: *const u16,
) -> i32 {
    let l = with_name!(name);
    finish_w(format_date(l, flags, st, picture_w(fmt)), buf, cap)
}

pub unsafe extern "win64" fn get_time_format_w(
    lcid: u32,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u16,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let l = with_lcid!(lcid);
    finish_w(format_time(l, flags, st, picture_w(fmt)), buf, cap)
}

pub unsafe extern "win64" fn get_time_format_a(
    lcid: u32,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u8,
    buf: *mut u8,
    cap: i32,
) -> i32 {
    let l = with_lcid!(lcid);
    finish_a(format_time(l, flags, st, picture_a(fmt)), buf, cap)
}

pub unsafe extern "win64" fn get_time_format_ex(
    name: *const u16,
    flags: u32,
    st: *const SystemTime,
    fmt: *const u16,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let l = with_name!(name);
    finish_w(format_time(l, flags, st, picture_w(fmt)), buf, cap)
}

/// NUMBERFMTW.
#[repr(C)]
struct NumberFmtW {
    digits: u32,
    leading_zero: u32,
    grouping: u32,
    decimal: *const u16,
    thousand: *const u16,
    negative_order: u32,
}

unsafe fn format_number_w(
    l: &Locale,
    value: *const u16,
    fmt: *const NumberFmtW,
) -> Result<String, u32> {
    let v = wide_opt(value).ok_or(ERROR_INVALID_PARAMETER)?;
    if fmt.is_null() {
        return locale::format_number(l, v.trim(), None).ok_or(ERROR_INVALID_PARAMETER);
    }
    let f = &*fmt;
    let base = locale::format_number(&locale::INVARIANT, v.trim(), Some(f.digits as usize))
        .ok_or(ERROR_INVALID_PARAMETER)?;
    let dec = wide_opt(f.decimal).unwrap_or_else(|| l.decimal.into());
    let th = wide_opt(f.thousand).unwrap_or_else(|| l.thousand.into());
    let (neg, body) = match base.strip_prefix('-') {
        Some(b) => (true, b.to_string()),
        None => (false, base),
    };
    let (int_part, frac) = body.split_once('.').unwrap_or((&body, ""));
    let mut int_part = if f.grouping == 0 {
        int_part.replace(',', "")
    } else {
        int_part.replace(',', &th)
    };
    if f.leading_zero == 0 && int_part == "0" && !frac.is_empty() {
        int_part.clear();
    }
    let mut num = int_part;
    if !frac.is_empty() {
        num.push_str(&dec);
        num.push_str(frac);
    }
    Ok(if !neg {
        num
    } else {
        match f.negative_order {
            0 => format!("({num})"),
            2 => format!("- {num}"),
            3 => format!("{num}-"),
            4 => format!("{num} -"),
            _ => format!("-{num}"),
        }
    })
}

pub unsafe extern "win64" fn get_number_format_w(
    lcid: u32,
    _flags: u32,
    value: *const u16,
    fmt: *const u8,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let l = with_lcid!(lcid);
    finish_w(
        format_number_w(l, value, fmt as *const NumberFmtW),
        buf,
        cap,
    )
}

pub unsafe extern "win64" fn get_number_format_ex(
    name: *const u16,
    _flags: u32,
    value: *const u16,
    fmt: *const u8,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let l = with_name!(name);
    finish_w(
        format_number_w(l, value, fmt as *const NumberFmtW),
        buf,
        cap,
    )
}

// ---------------------------------------------------------------------
// Fuseau horaire et conversions de FILETIME
// ---------------------------------------------------------------------

fn now_filetime() -> i64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    tz::FILETIME_UNIX_EPOCH
        + d.as_secs() as i64 * tz::TICKS_PER_SECOND
        + (d.subsec_nanos() / 100) as i64
}

fn unix_seconds(ft: i64) -> i64 {
    (ft - tz::FILETIME_UNIX_EPOCH).div_euclid(tz::TICKS_PER_SECOND)
}

/// Biais (en 100 ns) en vigueur maintenant : FileTimeToLocalFileTime
/// l'applique quelle que soit la date convertie, comme Windows.
fn current_bias_ticks(now: i64) -> i64 {
    tz::current().bias_at_utc(unix_seconds(now)) as i64 * tz::TICKS_PER_MINUTE
}

fn rule_to_systemtime(r: Option<Rule>) -> SystemTime {
    match r {
        None => SystemTime::default(),
        Some(r) => {
            let secs = r.seconds.clamp(0, 86_399);
            SystemTime {
                year: 0,
                month: r.month,
                day_of_week: r.weekday,
                day: r.week,
                hour: (secs / 3600) as u16,
                minute: (secs % 3600 / 60) as u16,
                second: (secs % 60) as u16,
                millis: 0,
            }
        }
    }
}

fn systemtime_to_rule(st: &SystemTime) -> Option<Rule> {
    (st.month != 0 && st.year == 0).then_some(Rule {
        month: st.month,
        week: st.day,
        weekday: st.day_of_week,
        seconds: st.hour as i32 * 3600 + st.minute as i32 * 60 + st.second as i32,
    })
}

fn put_name(dst: *mut u8, name: &str, max: usize) {
    let w: Vec<u16> = name.encode_utf16().take(max - 1).collect();
    unsafe {
        std::ptr::write_bytes(dst, 0, max * 2);
        std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, dst, w.len() * 2);
    }
}

/// Remplit TIME_ZONE_INFORMATION (172 octets).
unsafe fn fill_tzi(z: &Zone, p: *mut u8) {
    std::ptr::write_bytes(p, 0, 172);
    (p as *mut i32).write_unaligned(z.bias);
    put_name(p.add(4), &z.windows_key(), 32);
    (p.add(68) as *mut SystemTime).write_unaligned(rule_to_systemtime(z.dst_end));
    (p.add(84) as *mut i32).write_unaligned(0);
    put_name(p.add(88), &z.windows_daylight_name(), 32);
    (p.add(152) as *mut SystemTime).write_unaligned(rule_to_systemtime(z.dst_start));
    (p.add(168) as *mut i32).write_unaligned(if z.has_dst() { z.dst_bias } else { 0 });
}

/// Lit TIME_ZONE_INFORMATION fourni par l'appelant.
unsafe fn read_tzi(p: *const u8) -> Zone {
    let bias = (p as *const i32).read_unaligned();
    let std_bias = (p.add(84) as *const i32).read_unaligned();
    let dst_bias = (p.add(168) as *const i32).read_unaligned();
    let std_date = (p.add(68) as *const SystemTime).read_unaligned();
    let dst_date = (p.add(152) as *const SystemTime).read_unaligned();
    let mut z = Zone {
        bias: bias + std_bias,
        dst_bias: dst_bias - std_bias,
        dst_start: systemtime_to_rule(&dst_date),
        dst_end: systemtime_to_rule(&std_date),
        ..Zone::default()
    };
    if !z.has_dst() {
        z.dst_start = None;
        z.dst_end = None;
        z.dst_bias = 0;
    }
    z
}

fn zone_state(z: &Zone) -> u32 {
    if !z.has_dst() {
        0 // TIME_ZONE_ID_UNKNOWN
    } else if z.is_dst_utc(unix_seconds(now_filetime())) {
        2 // TIME_ZONE_ID_DAYLIGHT
    } else {
        1 // TIME_ZONE_ID_STANDARD
    }
}

pub unsafe extern "win64" fn get_time_zone_information(p: *mut u8) -> u32 {
    if p.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let z = tz::current();
    fill_tzi(z, p);
    zone_state(z)
}

/// DYNAMIC_TIME_ZONE_INFORMATION (432 octets) : TIME_ZONE_INFORMATION, nom
/// de clé (« Romance Standard Time ») et l'indicateur d'heure d'été.
pub unsafe extern "win64" fn get_dynamic_time_zone_information(p: *mut u8) -> u32 {
    if p.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let z = tz::current();
    std::ptr::write_bytes(p, 0, 432);
    fill_tzi(z, p);
    put_name(p.add(172), &z.windows_key(), 128);
    zone_state(z)
}

pub unsafe extern "win64" fn get_time_zone_information_for_year(
    _year: u16,
    _dynamic: *const u8,
    p: *mut u8,
) -> i32 {
    if p.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    fill_tzi(tz::current(), p);
    1
}

pub unsafe extern "win64" fn system_time_to_file_time(st: *const SystemTime, ft: *mut i64) -> i32 {
    if st.is_null() || ft.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match tz::system_to_filetime(&st.read_unaligned()) {
        Some(v) => {
            ft.write_unaligned(v);
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn file_time_to_system_time(ft: *const i64, st: *mut SystemTime) -> i32 {
    if ft.is_null() || st.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match tz::filetime_to_system(ft.read_unaligned()) {
        Some(v) => {
            st.write_unaligned(v);
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn file_time_to_local_file_time(ft: *const i64, local: *mut i64) -> i32 {
    if ft.is_null() || local.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let v = ft.read_unaligned();
    local.write_unaligned(v - current_bias_ticks(now_filetime()));
    1
}

pub unsafe extern "win64" fn local_file_time_to_file_time(local: *const i64, ft: *mut i64) -> i32 {
    if ft.is_null() || local.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let v = local.read_unaligned();
    ft.write_unaligned(v + current_bias_ticks(now_filetime()));
    1
}

unsafe fn zone_arg(p: *const u8) -> Zone {
    if p.is_null() {
        tz::current().clone()
    } else {
        read_tzi(p)
    }
}

/// UTC → heure locale du fuseau (heure d'été de la date convertie).
pub unsafe extern "win64" fn system_time_to_tz_specific_local_time(
    zone: *const u8,
    utc: *const SystemTime,
    local: *mut SystemTime,
) -> i32 {
    if utc.is_null() || local.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(ft) = tz::system_to_filetime(&utc.read_unaligned()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let z = zone_arg(zone);
    let bias = z.bias_at_utc(unix_seconds(ft)) as i64 * tz::TICKS_PER_MINUTE;
    match tz::filetime_to_system(ft - bias) {
        Some(v) => {
            local.write_unaligned(v);
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn tz_specific_local_time_to_system_time(
    zone: *const u8,
    local: *const SystemTime,
    utc: *mut SystemTime,
) -> i32 {
    if utc.is_null() || local.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(ft) = tz::system_to_filetime(&local.read_unaligned()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let z = zone_arg(zone);
    let rem = (ft - tz::FILETIME_UNIX_EPOCH).rem_euclid(tz::TICKS_PER_SECOND);
    let utc_secs = z.local_to_utc(unix_seconds(ft));
    let out = tz::FILETIME_UNIX_EPOCH + utc_secs * tz::TICKS_PER_SECOND + rem;
    match tz::filetime_to_system(out) {
        Some(v) => {
            utc.write_unaligned(v);
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

/// FILETIME → date et heure MS-DOS (FAT, ZIP), sans changement de fuseau.
pub unsafe extern "win64" fn file_time_to_dos_date_time(
    ft: *const i64,
    date: *mut u16,
    time: *mut u16,
) -> i32 {
    if ft.is_null() || date.is_null() || time.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(st) = tz::filetime_to_system(ft.read_unaligned()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if !(1980..=2107).contains(&st.year) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *date = ((st.year - 1980) << 9) | (st.month << 5) | st.day;
    *time = (st.hour << 11) | (st.minute << 5) | (st.second / 2);
    1
}

pub unsafe extern "win64" fn dos_date_time_to_file_time(date: u16, time: u16, ft: *mut i64) -> i32 {
    if ft.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let st = SystemTime {
        year: 1980 + (date >> 9),
        month: (date >> 5) & 0xF,
        day: date & 0x1F,
        hour: time >> 11,
        minute: (time >> 5) & 0x3F,
        second: (time & 0x1F) * 2,
        ..SystemTime::default()
    };
    match tz::system_to_filetime(&st) {
        Some(v) => {
            ft.write_unaligned(v);
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

/// CRT : `_tzset` relit le fuseau (déjà lu une fois par processus).
pub extern "win64" fn crt_tzset() {}

/// `_get_timezone` : secondes à ajouter à l'heure locale normale pour UTC.
pub unsafe extern "win64" fn crt_get_timezone(out: *mut i32) -> i32 {
    if out.is_null() {
        return 22;
    }
    *out = tz::current().bias * 60;
    0
}

pub unsafe extern "win64" fn crt_get_daylight(out: *mut i32) -> i32 {
    if out.is_null() {
        return 22;
    }
    *out = tz::current().has_dst() as i32;
    0
}

pub unsafe extern "win64" fn crt_get_dstbias(out: *mut i32) -> i32 {
    if out.is_null() {
        return 22;
    }
    *out = tz::current().dst_bias * 60;
    0
}

// ---------------------------------------------------------------------
// Clavier (user32)
// ---------------------------------------------------------------------

pub extern "win64" fn get_keyboard_layout(_thread: u32) -> usize {
    super::keyboard::current().hkl()
}

pub unsafe extern "win64" fn get_keyboard_layout_name_w(buf: *mut u16) -> i32 {
    if buf.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    (out_w(&super::keyboard::current().name(), buf, 9) != 0) as i32
}

pub unsafe extern "win64" fn get_keyboard_layout_name_a(buf: *mut u8) -> i32 {
    if buf.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    (out_a(&super::keyboard::current().name(), buf, 9) != 0) as i32
}

pub unsafe extern "win64" fn get_keyboard_layout_list(n: i32, list: *mut usize) -> i32 {
    if n > 0 && !list.is_null() {
        *list = super::keyboard::current().hkl();
    }
    1
}

/// Winlux suit la disposition de Linux : l'activation renvoie la
/// disposition en place (changement via Paramètres).
pub extern "win64" fn activate_keyboard_layout(_hkl: usize, _flags: u32) -> usize {
    super::keyboard::current().hkl()
}

pub unsafe extern "win64" fn load_keyboard_layout_w(_klid: *const u16, _flags: u32) -> usize {
    super::keyboard::current().hkl()
}

pub unsafe extern "win64" fn load_keyboard_layout_a(_klid: *const u8, _flags: u32) -> usize {
    super::keyboard::current().hkl()
}

// ---------------------------------------------------------------------
// Tables d'exports (étendues en dernier : elles priment)
// ---------------------------------------------------------------------

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetACP" => fn get_acp,
        "GetOEMCP" => fn get_oemcp,
        "IsValidCodePage" => fn is_valid_code_page,
        "GetCPInfo" => fn get_cp_info,
        "GetCPInfoExA" => fn get_cp_info_ex_a,
        "GetCPInfoExW" => fn get_cp_info_ex_w,
        "MultiByteToWideChar" => fn multi_byte_to_wide_char,
        "WideCharToMultiByte" => fn wide_char_to_multi_byte,
        "GetUserDefaultLCID" => fn get_user_default_lcid,
        "GetSystemDefaultLCID" => fn get_user_default_lcid,
        "GetThreadLocale" => fn get_user_default_lcid,
        "SetThreadLocale" => fn set_thread_locale,
        "GetUserDefaultLangID" => fn get_user_default_lang_id,
        "GetSystemDefaultLangID" => fn get_user_default_lang_id,
        "GetUserDefaultUILanguage" => fn get_user_default_ui_language,
        "GetSystemDefaultUILanguage" => fn get_user_default_ui_language,
        "GetThreadUILanguage" => fn get_user_default_ui_language,
        "GetUserGeoID" => fn get_user_geo_id,
        "GetUserDefaultLocaleName" => fn get_user_default_locale_name,
        "GetSystemDefaultLocaleName" => fn get_user_default_locale_name,
        "LocaleNameToLCID" => fn locale_name_to_lcid,
        "LCIDToLocaleName" => fn lcid_to_locale_name,
        "IsValidLocale" => fn is_valid_locale,
        "IsValidLocaleName" => fn is_valid_locale_name,
        "GetUserPreferredUILanguages" => fn get_preferred_ui_languages,
        "GetSystemPreferredUILanguages" => fn get_preferred_ui_languages,
        "GetThreadPreferredUILanguages" => fn get_preferred_ui_languages,
        "GetProcessPreferredUILanguages" => fn get_preferred_ui_languages,
        "GetLocaleInfoA" => fn get_locale_info_a,
        "GetLocaleInfoW" => fn get_locale_info_w,
        "GetLocaleInfoEx" => fn get_locale_info_ex,
        "GetDateFormatA" => fn get_date_format_a,
        "GetDateFormatW" => fn get_date_format_w,
        "GetDateFormatEx" => fn get_date_format_ex,
        "GetTimeFormatA" => fn get_time_format_a,
        "GetTimeFormatW" => fn get_time_format_w,
        "GetTimeFormatEx" => fn get_time_format_ex,
        "GetNumberFormatW" => fn get_number_format_w,
        "GetNumberFormatEx" => fn get_number_format_ex,
        "GetTimeZoneInformation" => fn get_time_zone_information,
        "GetDynamicTimeZoneInformation" => fn get_dynamic_time_zone_information,
        "GetTimeZoneInformationForYear" => fn get_time_zone_information_for_year,
        "SystemTimeToFileTime" => fn system_time_to_file_time,
        "FileTimeToSystemTime" => fn file_time_to_system_time,
        "FileTimeToLocalFileTime" => fn file_time_to_local_file_time,
        "LocalFileTimeToFileTime" => fn local_file_time_to_file_time,
        "SystemTimeToTzSpecificLocalTime" => fn system_time_to_tz_specific_local_time,
        "TzSpecificLocalTimeToSystemTime" => fn tz_specific_local_time_to_system_time,
        "FileTimeToDosDateTime" => fn file_time_to_dos_date_time,
        "DosDateTimeToFileTime" => fn dos_date_time_to_file_time,
    }
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetKeyboardLayout" => fn get_keyboard_layout,
        "GetKeyboardLayoutNameA" => fn get_keyboard_layout_name_a,
        "GetKeyboardLayoutNameW" => fn get_keyboard_layout_name_w,
        "GetKeyboardLayoutList" => fn get_keyboard_layout_list,
        "ActivateKeyboardLayout" => fn activate_keyboard_layout,
        "LoadKeyboardLayoutA" => fn load_keyboard_layout_a,
        "LoadKeyboardLayoutW" => fn load_keyboard_layout_w,
        "CharToOemA" => fn char_to_oem_a,
        "CharToOemW" => fn char_to_oem_w,
        "CharToOemBuffA" => fn char_to_oem_buff_a,
        "CharToOemBuffW" => fn char_to_oem_buff_w,
        "OemToCharA" => fn oem_to_char_a,
        "OemToCharW" => fn oem_to_char_w,
        "OemToCharBuffA" => fn oem_to_char_buff_a,
        "OemToCharBuffW" => fn oem_to_char_buff_w,
        "AnsiToOem" => fn char_to_oem_a,
        "OemToAnsi" => fn oem_to_char_a,
    }
}

pub fn crt_exports() -> crate::dlls::Exports {
    crate::exports! {
        "_tzset" => fn crt_tzset,
        "_get_timezone" => fn crt_get_timezone,
        "_get_daylight" => fn crt_get_daylight,
        "_get_dstbias" => fn crt_get_dstbias,
    }
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsTextUnicode" => fn is_text_unicode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn read_w(buf: &[u16]) -> String {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    #[test]
    fn conversions_multi_octets() {
        unsafe {
            let src = b"\xe9t\xe9 \x80";
            let mut w = [0u16; 8];
            assert_eq!(
                multi_byte_to_wide_char(1252, 0, src.as_ptr(), 5, w.as_mut_ptr(), 8),
                5
            );
            assert_eq!(read_w(&w[..5]), "été €");
            // Taille seule, puis tampon trop petit.
            assert_eq!(
                multi_byte_to_wide_char(1252, 0, src.as_ptr(), 5, std::ptr::null_mut(), 0),
                5
            );
            assert_eq!(
                multi_byte_to_wide_char(1252, 0, src.as_ptr(), 5, w.as_mut_ptr(), 2),
                0
            );
            // OEM 850 : 0x82 = é.
            assert_eq!(
                multi_byte_to_wide_char(850, 0, [0x82u8].as_ptr(), 1, w.as_mut_ptr(), 8),
                1
            );
            assert_eq!(w[0], 0xe9);
            // UTF-8 strict : octet invalide refusé.
            assert_eq!(
                multi_byte_to_wide_char(65001, 8, [0xffu8].as_ptr(), 1, w.as_mut_ptr(), 8),
                0
            );
            assert_eq!(
                multi_byte_to_wide_char(65001, 1, b"a".as_ptr(), 1, w.as_mut_ptr(), 8),
                0
            );

            let text = wide("Noël – œuf");
            let mut out = [0u8; 32];
            let mut used = 0;
            let n = wide_char_to_multi_byte(
                1252,
                0,
                text.as_ptr(),
                -1,
                out.as_mut_ptr(),
                32,
                std::ptr::null(),
                &mut used,
            );
            assert_eq!(&out[..n as usize], b"No\xebl \x96 \x9cuf\0");
            assert_eq!(used, 0);
            let n = wide_char_to_multi_byte(
                850,
                0x400,
                text.as_ptr(),
                -1,
                out.as_mut_ptr(),
                32,
                std::ptr::null(),
                &mut used,
            );
            assert_eq!(&out[..n as usize], b"No\x89l ? ?uf\0");
            assert_eq!(used, 1);
            let n = wide_char_to_multi_byte(
                65001,
                0,
                text.as_ptr(),
                -1,
                out.as_mut_ptr(),
                32,
                std::ptr::null(),
                std::ptr::null_mut(),
            );
            assert_eq!(&out[..n as usize - 1], "Noël – œuf".as_bytes());
            assert_eq!(
                wide_char_to_multi_byte(
                    65001,
                    0,
                    text.as_ptr(),
                    -1,
                    out.as_mut_ptr(),
                    32,
                    std::ptr::null(),
                    &mut used
                ),
                0
            );
        }
    }

    #[test]
    fn oem_et_ansi() {
        unsafe {
            let mut buf = *b"\xe9\xe8\xe0\0";
            assert_eq!(char_to_oem_a(buf.as_ptr(), buf.as_mut_ptr()), 1);
            if get_oemcp() == 850 {
                assert_eq!(&buf, b"\x82\x8a\x85\0");
            }
            assert_eq!(oem_to_char_a(buf.as_ptr(), buf.as_mut_ptr()), 1);
            assert_eq!(&buf, b"\xe9\xe8\xe0\0");
        }
    }

    #[test]
    fn informations_de_locale() {
        let fr = locale::from_tag("fr-FR").unwrap();
        let text = |l: &Locale, t: u32| match info_for(l, t) {
            Ok(Info::Text(s)) => s,
            Ok(Info::Number(n)) => format!("#{n}"),
            Err(e) => format!("E{e}"),
        };
        assert_eq!(text(fr, 0x01), "040c");
        assert_eq!(text(fr, 0x0E), ",");
        assert_eq!(text(fr, 0x1D), "/");
        assert_eq!(text(fr, 0x21), "1");
        assert_eq!(text(fr, 0x2A), "lundi");
        assert_eq!(text(fr, 0x30), "dimanche");
        assert_eq!(text(fr, 0x43), "décembre");
        assert_eq!(text(fr, 0x5C), "fr-FR");
        assert_eq!(text(fr, 0x73), "français (France)");
        assert_eq!(text(fr, 0x01 | LOCALE_RETURN_NUMBER), "#1036");
        assert_eq!(text(fr, 0x100C | LOCALE_RETURN_NUMBER), "#0");
        assert_eq!(text(fr, 0x1004), "1252");
        assert_eq!(text(fr, 0x0B), "850");
        assert_eq!(text(fr, 0x03), "FRA");
        let us = locale::from_tag("en-US").unwrap();
        assert_eq!(text(us, 0x23), "0");
        assert_eq!(text(us, 0x21), "0");
        assert_eq!(text(us, 0x03), "ENU");
        assert_eq!(text(locale::from_tag("fr-BE").unwrap(), 0x03), "FRB");
        assert_eq!(text(fr, 0x9999), "E1004");
        assert_eq!(resolve_lcid(0x0411).map(|l| l.name), None);
        assert_eq!(resolve_lcid(0x7F).map(|l| l.name), Some(""));
        unsafe {
            let name = wide("de-DE");
            let mut buf = [0u16; 32];
            let n = get_locale_info_ex(name.as_ptr(), 0x1001, buf.as_mut_ptr(), 32);
            assert_eq!((n, read_w(&buf)), (7, "German".into()));
            assert_eq!(
                get_locale_info_ex(name.as_ptr(), 0x1001, buf.as_mut_ptr(), 3),
                0
            );
            assert_eq!(
                get_locale_info_ex(name.as_ptr(), 0x1001, std::ptr::null_mut(), 0),
                7
            );
        }
    }

    #[test]
    fn dates_et_heures_mises_en_forme() {
        let st = SystemTime {
            year: 2026,
            month: 10,
            day_of_week: 0, // faux : recalculé (samedi)
            day: 31,
            hour: 9,
            minute: 7,
            second: 3,
            millis: 0,
        };
        let fr = locale::from_tag("fr-FR").unwrap();
        let us = locale::from_tag("en-US").unwrap();
        unsafe {
            assert_eq!(
                format_date(fr, 2, &st, Picture::Default).unwrap(),
                "samedi 31 octobre 2026"
            );
            assert_eq!(
                format_date(fr, 0, &st, Picture::Default).unwrap(),
                "31/10/2026"
            );
            assert_eq!(
                format_date(us, 0, &st, Picture::Default).unwrap(),
                "10/31/2026"
            );
            assert_eq!(
                format_date(fr, 8, &st, Picture::Default).unwrap(),
                "octobre 2026"
            );
            assert_eq!(
                format_time(fr, 0, &st, Picture::Default).unwrap(),
                "09:07:03"
            );
            assert_eq!(format_time(fr, 2, &st, Picture::Default).unwrap(), "09:07");
            assert_eq!(
                format_time(us, 2, &st, Picture::Default).unwrap(),
                "9:07 AM"
            );
            assert_eq!(
                format_time(us, 8, &st, Picture::Default).unwrap(),
                "9:07:03"
            );
            let bad = SystemTime { day: 32, ..st };
            assert!(format_date(fr, 0, &bad, Picture::Default).is_err());
            assert_eq!(
                format_date(fr, 0, &st, Picture::Given("yyyy-MM-dd".into())).unwrap(),
                "2026-10-31"
            );
        }
    }

    #[test]
    fn nombres_et_numberfmt() {
        let fr = locale::from_tag("fr-FR").unwrap();
        let v = wide("-1234.5");
        unsafe {
            assert_eq!(
                format_number_w(fr, v.as_ptr(), std::ptr::null()).unwrap(),
                "-1\u{202f}234,50"
            );
            let dec = wide(",");
            let th = wide(" ");
            let f = NumberFmtW {
                digits: 1,
                leading_zero: 1,
                grouping: 3,
                decimal: dec.as_ptr(),
                thousand: th.as_ptr(),
                negative_order: 0,
            };
            assert_eq!(format_number_w(fr, v.as_ptr(), &f).unwrap(), "(1 234,5)");
        }
    }

    #[test]
    fn fuseau_et_filetime() {
        unsafe {
            let st = SystemTime {
                year: 2026,
                month: 7,
                day: 14,
                hour: 12,
                ..SystemTime::default()
            };
            let mut ft = 0i64;
            assert_eq!(system_time_to_file_time(&st, &mut ft), 1);
            let mut back = SystemTime::default();
            assert_eq!(file_time_to_system_time(&ft, &mut back), 1);
            assert_eq!(
                (back.year, back.month, back.day, back.hour, back.day_of_week),
                (2026, 7, 14, 12, 2)
            );
            // Fuseau de Paris fourni par l'appelant : 12:00 UTC → 14:00.
            let mut paris = tz::parse_posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
            paris.iana = "Europe/Paris".into();
            let mut tzi = [0u8; 172];
            fill_tzi(&paris, tzi.as_mut_ptr());
            assert_eq!(i32::from_le_bytes(tzi[0..4].try_into().unwrap()), -60);
            assert_eq!(i32::from_le_bytes(tzi[168..172].try_into().unwrap()), -60);
            let mut local = SystemTime::default();
            assert_eq!(
                system_time_to_tz_specific_local_time(tzi.as_ptr(), &st, &mut local),
                1
            );
            assert_eq!(local.hour, 14);
            let mut utc = SystemTime::default();
            assert_eq!(
                tz_specific_local_time_to_system_time(tzi.as_ptr(), &local, &mut utc),
                1
            );
            assert_eq!(utc.hour, 12);
            let winter = SystemTime { month: 1, ..st };
            system_time_to_tz_specific_local_time(tzi.as_ptr(), &winter, &mut local);
            assert_eq!(local.hour, 13);
            assert_eq!(read_tzi(tzi.as_ptr()).dst_start, paris.dst_start);
            // Date MS-DOS.
            let (mut d, mut t) = (0u16, 0u16);
            assert_eq!(file_time_to_dos_date_time(&ft, &mut d, &mut t), 1);
            let mut ft2 = 0i64;
            assert_eq!(dos_date_time_to_file_time(d, t, &mut ft2), 1);
            assert_eq!(ft2, ft);
            let mut dyn_tzi = [0xAAu8; 432];
            assert!(get_dynamic_time_zone_information(dyn_tzi.as_mut_ptr()) <= 2);
            assert_eq!(dyn_tzi[428], 0);
        }
    }

    #[test]
    fn disposition_clavier() {
        unsafe {
            let mut name = [0u16; 9];
            assert_eq!(get_keyboard_layout_name_w(name.as_mut_ptr()), 1);
            let klid = read_w(&name);
            assert_eq!(klid.len(), 8);
            assert_eq!(
                get_keyboard_layout(0) >> 16,
                usize::from_str_radix(&klid, 16).unwrap() & 0xFFFF
            );
        }
    }
}
