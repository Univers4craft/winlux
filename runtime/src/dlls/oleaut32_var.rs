//! VARIANT d'OLE Automation : conversions (VariantChangeType(Ex)), calcul
//! (VarAdd, VarCmp…), conversions vers et depuis les chaînes (VarBstrFrom*,
//! Var*FromStr) et dates OLE (VariantTimeToSystemTime…).
//!
//! L'unité Variants de Delphi les importe toutes d'oleaut32 ; un programme
//! Delphi 32 bits (WinSCP x86, #232) s'arrêtait au premier appel. Écrit
//! d'après la documentation publique Microsoft Learn de chaque fonction
//! (types de résultat, codes d'erreur, VARCMP_*, VAR_*VALUEONLY) et de la
//! structure VARIANT.
//!
//! Disposition d'un VARIANT : `vt` (u16) à +0, valeur à +8. Un programme
//! x86 n'y range que 16 octets et des pointeurs de 4 octets : les BSTR et
//! les références passent par `wow32::read_ptr`/`write_ptr`.

#![allow(non_snake_case)]

use super::oleaut32::{sys_alloc_string_len, sys_string_len, variant_clear};
use std::ffi::c_void;

pub const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
pub const DISP_E_TYPEMISMATCH: i32 = 0x8002_0005u32 as i32;
pub const DISP_E_OVERFLOW: i32 = 0x8002_000Au32 as i32;
pub const DISP_E_DIVBYZERO: i32 = 0x8002_0012u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

const VT_EMPTY: u16 = 0;
const VT_NULL: u16 = 1;
const VT_I2: u16 = 2;
const VT_I4: u16 = 3;
const VT_R4: u16 = 4;
const VT_R8: u16 = 5;
const VT_CY: u16 = 6;
const VT_DATE: u16 = 7;
const VT_BSTR: u16 = 8;
const VT_ERROR: u16 = 10;
const VT_BOOL: u16 = 11;
const VT_VARIANT: u16 = 12;
const VT_DECIMAL: u16 = 14;
const VT_I1: u16 = 16;
const VT_UI1: u16 = 17;
const VT_UI2: u16 = 18;
const VT_UI4: u16 = 19;
const VT_I8: u16 = 20;
const VT_UI8: u16 = 21;
const VT_INT: u16 = 22;
const VT_UINT: u16 = 23;
const VT_BYREF: u16 = 0x4000;

const VARCMP_LT: i32 = 0;
const VARCMP_EQ: i32 = 1;
const VARCMP_GT: i32 = 2;
const VARCMP_NULL: i32 = 3;

const VAR_TIMEVALUEONLY: u32 = 1;
const VAR_DATEVALUEONLY: u32 = 2;
const NORM_IGNORECASE: u32 = 1;

/// Valeur d'un VARIANT, sans sa disposition mémoire.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Empty,
    Null,
    Int(i64),
    Real(f64),
    /// Monnaie : entier en dix-millièmes.
    Cy(i64),
    Date(f64),
    Bool(bool),
    Str(String),
    Error(i32),
}

// ── Lecture et écriture d'un VARIANT ────────────────────────────────

unsafe fn bstr_text(b: *const u16) -> String {
    if b.is_null() {
        return String::new();
    }
    let len = sys_string_len(b) as usize;
    String::from_utf16_lossy(std::slice::from_raw_parts(b, len))
}

fn decimal_value(p: *const u8) -> f64 {
    unsafe {
        let scale = *p.add(2) as i32;
        let sign = *p.add(3);
        let hi = (p.add(4) as *const u32).read_unaligned() as u128;
        let lo = (p.add(8) as *const u64).read_unaligned() as u128;
        let v = ((hi << 64) | lo) as f64 / 10f64.powi(scale);
        if sign & 0x80 != 0 {
            -v
        } else {
            v
        }
    }
}

/// Valeur portée par `p` (pointeur vers la donnée, pas vers le VARIANT).
unsafe fn read_scalar(vt: u16, p: *const u8) -> Option<Val> {
    Some(match vt {
        VT_EMPTY => Val::Empty,
        VT_NULL => Val::Null,
        VT_I1 => Val::Int(*(p as *const i8) as i64),
        VT_UI1 => Val::Int(*p as i64),
        VT_I2 => Val::Int((p as *const i16).read_unaligned() as i64),
        VT_UI2 => Val::Int((p as *const u16).read_unaligned() as i64),
        VT_I4 | VT_INT => Val::Int((p as *const i32).read_unaligned() as i64),
        VT_UI4 | VT_UINT => Val::Int((p as *const u32).read_unaligned() as i64),
        VT_I8 => Val::Int((p as *const i64).read_unaligned()),
        VT_UI8 => {
            let v = (p as *const u64).read_unaligned();
            if v > i64::MAX as u64 {
                Val::Real(v as f64)
            } else {
                Val::Int(v as i64)
            }
        }
        VT_R4 => Val::Real((p as *const f32).read_unaligned() as f64),
        VT_R8 => Val::Real((p as *const f64).read_unaligned()),
        VT_CY => Val::Cy((p as *const i64).read_unaligned()),
        VT_DATE => Val::Date((p as *const f64).read_unaligned()),
        VT_BOOL => Val::Bool((p as *const i16).read_unaligned() != 0),
        VT_ERROR => Val::Error((p as *const i32).read_unaligned()),
        VT_BSTR => Val::Str(bstr_text(crate::wow32::read_ptr(p as usize) as *const u16)),
        _ => return None,
    })
}

/// Lit un VARIANT (références VT_BYREF et VT_VARIANT suivies).
pub unsafe fn read(v: *const c_void) -> Option<Val> {
    if v.is_null() {
        return None;
    }
    let vt = *(v as *const u16);
    let data = (v as *const u8).add(8);
    if vt == VT_DECIMAL {
        return Some(Val::Real(decimal_value(v as *const u8)));
    }
    if vt & VT_BYREF != 0 {
        let target = crate::wow32::read_ptr(data as usize) as *const u8;
        if target.is_null() {
            return None;
        }
        return match vt & !VT_BYREF {
            VT_VARIANT => read(target as *const c_void),
            VT_DECIMAL => Some(Val::Real(decimal_value(target))),
            base => read_scalar(base, target),
        };
    }
    read_scalar(vt, data)
}

/// Écrit `val` dans `dest` avec le type `vt` (le contenu précédent est
/// libéré). Rend 0 ou le HRESULT d'échec, sans toucher `dest` en cas
/// d'échec.
pub unsafe fn write(dest: *mut c_void, val: &Val, vt: u16) -> i32 {
    let mut data = [0u8; 8];
    let mut decimal: Option<[u8; 16]> = None;
    let mut text: Option<String> = None;
    macro_rules! put {
        ($v:expr) => {{
            let b = $v.to_le_bytes();
            data[..b.len()].copy_from_slice(&b);
        }};
    }
    macro_rules! int_in {
        ($t:ty) => {{
            let i = match to_int(val) {
                Ok(i) => i,
                Err(e) => return e,
            };
            match <$t>::try_from(i) {
                Ok(x) => put!(x),
                Err(_) => return DISP_E_OVERFLOW,
            }
        }};
    }
    match vt {
        VT_EMPTY | VT_NULL => {}
        VT_I1 => int_in!(i8),
        VT_UI1 => int_in!(u8),
        VT_I2 => int_in!(i16),
        VT_UI2 => int_in!(u16),
        VT_I4 | VT_INT => int_in!(i32),
        VT_UI4 | VT_UINT => int_in!(u32),
        VT_I8 => int_in!(i64),
        VT_UI8 => int_in!(u64),
        VT_R4 => match to_real(val) {
            Ok(r) if r.is_finite() && r.abs() > f32::MAX as f64 => return DISP_E_OVERFLOW,
            Ok(r) => put!(r as f32),
            Err(e) => return e,
        },
        VT_R8 => match to_real(val) {
            Ok(r) => put!(r),
            Err(e) => return e,
        },
        VT_DATE => match to_date(val) {
            Ok(d) => put!(d),
            Err(e) => return e,
        },
        VT_CY => match to_cy(val) {
            Ok(c) => put!(c),
            Err(e) => return e,
        },
        VT_BOOL => match to_bool(val) {
            Ok(b) => put!(if b { -1i16 } else { 0i16 }),
            Err(e) => return e,
        },
        VT_ERROR => match val {
            Val::Error(e) => put!(*e),
            _ => return DISP_E_TYPEMISMATCH,
        },
        VT_BSTR => match to_text(val, 0x400, 0) {
            Ok(s) => text = Some(s),
            Err(e) => return e,
        },
        VT_DECIMAL => match to_decimal(val) {
            Ok(d) => decimal = Some(d),
            Err(e) => return e,
        },
        _ => return DISP_E_TYPEMISMATCH,
    }
    let bstr = match &text {
        Some(s) => {
            let w: Vec<u16> = s.encode_utf16().collect();
            let b = sys_alloc_string_len(w.as_ptr(), w.len() as u32);
            if b.is_null() {
                return E_OUTOFMEMORY;
            }
            b as usize
        }
        None => 0,
    };
    variant_clear(dest);
    let d = dest as *mut u8;
    if let Some(dec) = decimal {
        std::ptr::copy_nonoverlapping(dec.as_ptr(), d, 16);
        *(d as *mut u16) = VT_DECIMAL;
        return 0;
    }
    *(d as *mut u16) = vt;
    if vt == VT_BSTR {
        crate::wow32::write_ptr(d as usize + 8, bstr);
    } else {
        std::ptr::copy_nonoverlapping(data.as_ptr(), d.add(8), 8);
    }
    0
}

// ── Conversions de valeurs ──────────────────────────────────────────

fn round_half_even(r: f64) -> f64 {
    let f = r.floor();
    let diff = r - f;
    if diff > 0.5 || (diff == 0.5 && f % 2.0 != 0.0) {
        f + 1.0
    } else {
        f
    }
}

/// Entier (les réels sont arrondis au pair le plus proche, comme Windows).
pub fn to_int(v: &Val) -> Result<i64, i32> {
    match v {
        Val::Empty => Ok(0),
        Val::Int(i) => Ok(*i),
        Val::Bool(b) => Ok(if *b { -1 } else { 0 }),
        Val::Cy(c) => Ok(round_half_even(*c as f64 / 10_000.0) as i64),
        Val::Real(r) | Val::Date(r) => {
            if !r.is_finite() || r.abs() >= 9.2e18 {
                return Err(DISP_E_OVERFLOW);
            }
            Ok(round_half_even(*r) as i64)
        }
        Val::Str(s) => match parse_number(s) {
            Some(Val::Int(i)) => Ok(i),
            Some(Val::Real(r)) => to_int(&Val::Real(r)),
            _ => match parse_bool(s) {
                Some(b) => Ok(if b { -1 } else { 0 }),
                None => Err(DISP_E_TYPEMISMATCH),
            },
        },
        Val::Null | Val::Error(_) => Err(DISP_E_TYPEMISMATCH),
    }
}

pub fn to_real(v: &Val) -> Result<f64, i32> {
    match v {
        Val::Real(r) | Val::Date(r) => Ok(*r),
        Val::Cy(c) => Ok(*c as f64 / 10_000.0),
        Val::Str(s) => match parse_number(s) {
            Some(Val::Int(i)) => Ok(i as f64),
            Some(Val::Real(r)) => Ok(r),
            _ => match parse_bool(s) {
                Some(b) => Ok(if b { -1.0 } else { 0.0 }),
                None => Err(DISP_E_TYPEMISMATCH),
            },
        },
        other => to_int(other).map(|i| i as f64),
    }
}

fn to_cy(v: &Val) -> Result<i64, i32> {
    if let Val::Cy(c) = v {
        return Ok(*c);
    }
    let r = to_real(v)? * 10_000.0;
    if !r.is_finite() || r.abs() >= 9.2e18 {
        return Err(DISP_E_OVERFLOW);
    }
    Ok(round_half_even(r) as i64)
}

fn to_bool(v: &Val) -> Result<bool, i32> {
    match v {
        Val::Bool(b) => Ok(*b),
        Val::Str(s) => match parse_bool(s) {
            Some(b) => Ok(b),
            None => to_real(v).map(|r| r != 0.0),
        },
        other => to_real(other).map(|r| r != 0.0),
    }
}

fn to_date(v: &Val) -> Result<f64, i32> {
    match v {
        Val::Str(s) => parse_date(s, 0x400).ok_or(DISP_E_TYPEMISMATCH),
        other => {
            let d = to_real(other)?;
            // Limites d'une date OLE : 1er janvier 100 à 31 décembre 9999.
            if !(-657_434.0..2_958_466.0).contains(&d) {
                return Err(DISP_E_OVERFLOW);
            }
            Ok(d)
        }
    }
}

fn to_decimal(v: &Val) -> Result<[u8; 16], i32> {
    let (mant, scale): (i128, u8) = match v {
        Val::Int(i) => (*i as i128, 0),
        Val::Cy(c) => (*c as i128, 4),
        other => {
            let r = to_real(other)?;
            if !r.is_finite() || r.abs() >= 7.9e28 {
                return Err(DISP_E_OVERFLOW);
            }
            // 15 chiffres significatifs, comme la conversion de Windows.
            let digits = (14 - r.abs().log10().floor() as i32).clamp(0, 28) as u8;
            let mut m = (r * 10f64.powi(digits as i32)).round() as i128;
            let mut s = digits;
            while s > 0 && m % 10 == 0 {
                m /= 10;
                s -= 1;
            }
            (m, s)
        }
    };
    let mut d = [0u8; 16];
    d[2] = scale;
    d[3] = if mant < 0 { 0x80 } else { 0 };
    let a = mant.unsigned_abs();
    d[4..8].copy_from_slice(&((a >> 64) as u32).to_le_bytes());
    d[8..16].copy_from_slice(&(a as u64).to_le_bytes());
    Ok(d)
}

fn locale(lcid: u32) -> &'static crate::intl::locale::Locale {
    crate::intl::api::resolve_lcid(lcid).unwrap_or_else(crate::intl::locale::current)
}

/// Réel en texte : 15 chiffres significatifs au plus, séparateur décimal
/// de la langue.
fn real_text(r: f64, lcid: u32) -> String {
    if r == 0.0 {
        return "0".into();
    }
    let rounded: f64 = format!("{:.14e}", r).parse().unwrap_or(r);
    let mut s = format!("{rounded}");
    if s.contains('e') {
        s = format!("{rounded:E}");
    }
    let dec = locale(lcid).decimal;
    if dec != "." {
        s = s.replacen('.', dec, 1);
    }
    s
}

pub fn to_text(v: &Val, lcid: u32, flags: u32) -> Result<String, i32> {
    Ok(match v {
        Val::Empty => String::new(),
        Val::Str(s) => s.clone(),
        Val::Int(i) => i.to_string(),
        Val::Real(r) => real_text(*r, lcid),
        Val::Cy(c) => real_text(*c as f64 / 10_000.0, lcid),
        // VARIANT_TRUE en texte : « True » (VAR_LOCALBOOL non géré).
        Val::Bool(b) => if *b { "True" } else { "False" }.into(),
        Val::Date(d) => date_text(*d, lcid, flags)?,
        Val::Null | Val::Error(_) => return Err(DISP_E_TYPEMISMATCH),
    })
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "#true#" => Some(true),
        "false" | "#false#" => Some(false),
        _ => None,
    }
}

/// Nombre écrit en texte : point ou virgule décimale, espaces autour,
/// signe, exposant.
pub fn parse_number(s: &str) -> Option<Val> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(i) = t.parse::<i64>() {
        return Some(Val::Int(i));
    }
    if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")) {
        return i64::from_str_radix(h, 16).ok().map(Val::Int);
    }
    let norm = if t.contains(',') && !t.contains('.') {
        t.replacen(',', ".", 1)
    } else {
        t.replace(',', "")
    };
    let r: f64 = norm.parse().ok()?;
    r.is_finite().then_some(Val::Real(r))
}

// ── Dates OLE ───────────────────────────────────────────────────────

/// Jours depuis le 1er janvier 1970 (calendrier grégorien proleptique).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// Jour 0 d'une date OLE : 30 décembre 1899.
const OLE_EPOCH_DAYS: i64 = -25_569;

/// Date OLE → (année, mois, jour, heure, minute, seconde, ms, jour de la
/// semaine). Partie entière = jour (négative avant 1899), partie décimale
/// = heure, toujours positive (documentation de VariantTimeToSystemTime).
pub fn ole_to_parts(d: f64) -> Option<[u16; 8]> {
    if !d.is_finite() || !(-657_434.0..2_958_466.0).contains(&d) {
        return None;
    }
    let day = d.trunc();
    let frac = (d - day).abs();
    let mut ms = (frac * 86_400_000.0).round() as i64;
    let mut days = day as i64;
    if ms >= 86_400_000 {
        ms -= 86_400_000;
        days += if d < 0.0 { -1 } else { 1 };
    }
    let z = days + OLE_EPOCH_DAYS;
    let (y, m, dd) = civil_from_days(z);
    let dow = (z + 4).rem_euclid(7);
    Some([
        y as u16,
        m as u16,
        dow as u16,
        dd as u16,
        (ms / 3_600_000) as u16,
        (ms / 60_000 % 60) as u16,
        (ms / 1000 % 60) as u16,
        (ms % 1000) as u16,
    ])
}

pub fn parts_to_ole(y: i64, m: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> Option<f64> {
    if !(100..=9999).contains(&y) || !(1..=12).contains(&m) || d == 0 || h > 23 || mi > 59 || s > 59
    {
        return None;
    }
    let dim = days_from_civil(
        if m == 12 { y + 1 } else { y },
        if m == 12 { 1 } else { m + 1 },
        1,
    ) - days_from_civil(y, m, 1);
    if d as i64 > dim {
        return None;
    }
    let days = (days_from_civil(y, m, d) - OLE_EPOCH_DAYS) as f64;
    let t = (h as f64 * 3600.0 + mi as f64 * 60.0 + s as f64 + ms as f64 / 1000.0) / 86_400.0;
    Some(if days < 0.0 { days - t } else { days + t })
}

fn date_text(d: f64, lcid: u32, flags: u32) -> Result<String, i32> {
    let st = ole_to_parts(d).ok_or(DISP_E_OVERFLOW)?;
    let has_time = st[4] != 0 || st[5] != 0 || st[6] != 0;
    let date_only = flags & VAR_DATEVALUEONLY != 0 || (!has_time && flags & VAR_TIMEVALUEONLY == 0);
    let time_only = flags & VAR_TIMEVALUEONLY != 0 || (d.trunc() == 0.0 && has_time);
    let mut out = String::new();
    let mut buf = [0u16; 128];
    unsafe {
        let st_ptr = st.as_ptr() as *const _;
        if !time_only {
            let n = crate::intl::api::get_date_format_w(
                lcid,
                0,
                st_ptr,
                std::ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as i32,
            );
            if n > 0 {
                out.push_str(&String::from_utf16_lossy(&buf[..n as usize - 1]));
            }
        }
        if !date_only {
            let n = crate::intl::api::get_time_format_w(
                lcid,
                0,
                st_ptr,
                std::ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as i32,
            );
            if n > 0 {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&String::from_utf16_lossy(&buf[..n as usize - 1]));
            }
        }
    }
    Ok(out)
}

/// Date en texte : nombres de la date dans l'ordre de la langue (jour,
/// mois, année d'après la date courte), ou année d'abord quand elle a 4
/// chiffres ; heure « h:mm[:ss] » avec AM/PM facultatif.
pub fn parse_date(s: &str, lcid: u32) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (date_part, time_part) = match t.find(':') {
        Some(colon) => {
            let start = t[..colon].rfind([' ', 'T']).map_or(0, |i| i + 1);
            (t[..start].trim_end_matches([' ', 'T']), &t[start..])
        }
        None => (t, ""),
    };
    let (mut h, mut mi, mut sec) = (0u32, 0u32, 0u32);
    if !time_part.is_empty() {
        let lower = time_part.to_ascii_lowercase();
        let pm = lower.ends_with("pm");
        let am = lower.ends_with("am");
        let clock = lower.trim_end_matches("pm").trim_end_matches("am").trim();
        let nums: Vec<u32> = clock
            .split(':')
            .map(|p| p.trim().split('.').next().unwrap_or("").parse().ok())
            .collect::<Option<_>>()?;
        h = *nums.first()?;
        mi = nums.get(1).copied().unwrap_or(0);
        sec = nums.get(2).copied().unwrap_or(0);
        if pm && h < 12 {
            h += 12;
        } else if am && h == 12 {
            h = 0;
        }
    }
    if date_part.is_empty() {
        let t = (h as f64 * 3600.0 + mi as f64 * 60.0 + sec as f64) / 86_400.0;
        return (h < 24 && mi < 60 && sec < 60).then_some(t);
    }
    let fields: Vec<&str> = date_part
        .split(['/', '-', '.', ' '])
        .filter(|f| !f.is_empty())
        .collect();
    let nums: Vec<i64> = fields
        .iter()
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    let (y, m, d) = match nums.as_slice() {
        [a, b, c] if fields[0].len() == 4 => (*a, *b, *c),
        [a, b, c] => {
            let pat = locale(lcid).short_date.to_ascii_lowercase();
            let pos = |ch: char| pat.find(ch).unwrap_or(usize::MAX);
            let mut order = [('d', pos('d')), ('m', pos('m')), ('y', pos('y'))];
            order.sort_by_key(|o| o.1);
            let vals = [*a, *b, *c];
            let get = |ch: char| vals[order.iter().position(|o| o.0 == ch).unwrap_or(0)];
            (get('y'), get('m'), get('d'))
        }
        _ => return None,
    };
    let y = if y < 100 {
        if y < 30 {
            2000 + y
        } else {
            1900 + y
        }
    } else {
        y
    };
    parts_to_ole(y, m as u32, d as u32, h, mi, sec, 0)
}

// ── Opérations ──────────────────────────────────────────────────────

fn is_text(v: &Val) -> bool {
    matches!(v, Val::Str(_))
}

/// Résultat entier en VT_I4 s'il tient, sinon en VT_R8 (débordement d'un
/// calcul entier, comme Windows).
fn int_result(r: Option<i64>, real: f64) -> (Val, u16) {
    match r {
        Some(i) if i32::try_from(i).is_ok() => (Val::Int(i), VT_I4),
        _ => (Val::Real(real), VT_R8),
    }
}

fn kind_rank(v: &Val) -> u8 {
    match v {
        Val::Empty | Val::Int(_) | Val::Bool(_) => 0,
        Val::Cy(_) => 1,
        Val::Date(_) => 2,
        Val::Real(_) | Val::Str(_) => 3,
        _ => 4,
    }
}

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Idiv,
    Mod,
    And,
    Or,
    Xor,
    Eqv,
    Imp,
    Pow,
}

fn arith(op: Op, a: &Val, b: &Val) -> Result<(Val, u16), i32> {
    if matches!(a, Val::Error(_)) || matches!(b, Val::Error(_)) {
        return Err(DISP_E_TYPEMISMATCH);
    }
    // Logique : Null And False = False, Null Or True = True, sinon Null.
    let logic = matches!(op, Op::And | Op::Or | Op::Xor | Op::Eqv | Op::Imp);
    if matches!(a, Val::Null) || matches!(b, Val::Null) {
        if logic {
            let other = if matches!(a, Val::Null) { b } else { a };
            if !matches!(other, Val::Null) {
                let o = to_int(other)?;
                match op {
                    Op::And if o == 0 => return Ok((Val::Int(0), VT_I4)),
                    Op::Or if o == -1 => return Ok((Val::Int(-1), VT_I4)),
                    _ => {}
                }
            }
        }
        return Ok((Val::Null, VT_NULL));
    }
    if let (Op::Add, true, true) = (op, is_text(a), is_text(b)) {
        let s = format!("{}{}", to_text(a, 0x400, 0)?, to_text(b, 0x400, 0)?);
        return Ok((Val::Str(s), VT_BSTR));
    }
    if logic || matches!(op, Op::Idiv | Op::Mod) {
        let (x, y) = (to_int(a)?, to_int(b)?);
        let both_bool = matches!((a, b), (Val::Bool(_), Val::Bool(_)));
        let r = match op {
            Op::And => x & y,
            Op::Or => x | y,
            Op::Xor => x ^ y,
            Op::Eqv => !(x ^ y),
            Op::Imp => !x | y,
            Op::Idiv | Op::Mod if y == 0 => return Err(DISP_E_DIVBYZERO),
            Op::Idiv => x.wrapping_div(y),
            _ => x.wrapping_rem(y),
        };
        if both_bool && logic {
            return Ok((Val::Bool(r != 0), VT_BOOL));
        }
        return Ok(int_result(Some(r), r as f64));
    }
    let rank = kind_rank(a).max(kind_rank(b));
    let (x, y) = (to_real(a)?, to_real(b)?);
    match op {
        Op::Div => {
            if y == 0.0 {
                return Err(DISP_E_DIVBYZERO);
            }
            Ok((Val::Real(x / y), VT_R8))
        }
        Op::Pow => Ok((Val::Real(x.powf(y)), VT_R8)),
        _ if rank == 0 => {
            let (i, j) = (to_int(a)?, to_int(b)?);
            let r = match op {
                Op::Add => i.checked_add(j),
                Op::Sub => i.checked_sub(j),
                _ => i.checked_mul(j),
            };
            let real = match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                _ => x * y,
            };
            Ok(int_result(r, real))
        }
        _ => {
            let r = match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                _ => x * y,
            };
            match rank {
                1 => Ok((Val::Cy(round_half_even(r * 10_000.0) as i64), VT_CY)),
                // Date ± nombre reste une date ; date − date est un nombre.
                2 if !(matches!(op, Op::Sub)
                    && matches!(a, Val::Date(_))
                    && matches!(b, Val::Date(_))) =>
                {
                    Ok((Val::Date(r), VT_DATE))
                }
                _ => Ok((Val::Real(r), VT_R8)),
            }
        }
    }
}

unsafe fn binary(op: Op, left: *const c_void, right: *const c_void, result: *mut c_void) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    let (Some(a), Some(b)) = (read(left), read(right)) else {
        return if left.is_null() || right.is_null() {
            E_INVALIDARG
        } else {
            DISP_E_TYPEMISMATCH
        };
    };
    match arith(op, &a, &b) {
        Ok((v, vt)) => write(result, &v, vt),
        Err(e) => e,
    }
}

// Écrites une à une : build.rs relève la signature x86 de chaque
// fonction dans le texte source, pas dans une macro.
pub unsafe extern "win64" fn VarAdd(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Add, l, r, out)
}

pub unsafe extern "win64" fn VarSub(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Sub, l, r, out)
}

pub unsafe extern "win64" fn VarMul(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Mul, l, r, out)
}

pub unsafe extern "win64" fn VarDiv(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Div, l, r, out)
}

pub unsafe extern "win64" fn VarIdiv(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Idiv, l, r, out)
}

pub unsafe extern "win64" fn VarMod(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Mod, l, r, out)
}

pub unsafe extern "win64" fn VarAnd(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::And, l, r, out)
}

pub unsafe extern "win64" fn VarOr(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Or, l, r, out)
}

pub unsafe extern "win64" fn VarXor(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Xor, l, r, out)
}

pub unsafe extern "win64" fn VarEqv(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Eqv, l, r, out)
}

pub unsafe extern "win64" fn VarImp(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Imp, l, r, out)
}

pub unsafe extern "win64" fn VarPow(l: *const c_void, r: *const c_void, out: *mut c_void) -> i32 {
    binary(Op::Pow, l, r, out)
}

pub unsafe extern "win64" fn VarCat(
    left: *const c_void,
    right: *const c_void,
    result: *mut c_void,
) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    let (Some(a), Some(b)) = (read(left), read(right)) else {
        return DISP_E_TYPEMISMATCH;
    };
    if matches!(a, Val::Null) && matches!(b, Val::Null) {
        return write(result, &Val::Null, VT_NULL);
    }
    let text = |v: &Val| match v {
        Val::Null => Ok(String::new()),
        other => to_text(other, 0x400, 0),
    };
    match (text(&a), text(&b)) {
        (Ok(x), Ok(y)) => write(result, &Val::Str(x + &y), VT_BSTR),
        (Err(e), _) | (_, Err(e)) => e,
    }
}

unsafe fn unary(v: *const c_void, result: *mut c_void, neg: bool) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    let Some(a) = read(v) else {
        return DISP_E_TYPEMISMATCH;
    };
    let src_vt = *(v as *const u16) & !VT_BYREF;
    let (val, vt) = match (&a, neg) {
        (Val::Null, _) => (Val::Null, VT_NULL),
        (Val::Bool(b), false) => (Val::Bool(!b), VT_BOOL),
        (Val::Bool(b), true) => (Val::Int(if *b { 1 } else { 0 }), VT_I2),
        (Val::Real(r), true) => (Val::Real(-r), if src_vt == VT_R4 { VT_R4 } else { VT_R8 }),
        (Val::Date(d), true) => (Val::Date(-d), VT_DATE),
        (Val::Cy(c), true) => (Val::Cy(-c), VT_CY),
        (Val::Str(_), true) => match to_real(&a) {
            Ok(r) => (Val::Real(-r), VT_R8),
            Err(e) => return e,
        },
        (Val::Empty, true) => (Val::Int(0), VT_I2),
        (Val::Error(_), _) => return DISP_E_TYPEMISMATCH,
        (_, true) => match to_int(&a) {
            Ok(i) => {
                let vt = match src_vt {
                    VT_I2 | VT_I4 | VT_I8 => src_vt,
                    _ => VT_I4,
                };
                match i.checked_neg() {
                    Some(n) => (Val::Int(n), vt),
                    None => return DISP_E_OVERFLOW,
                }
            }
            Err(e) => return e,
        },
        (_, false) => match to_int(&a) {
            Ok(i) => {
                let vt = match src_vt {
                    VT_UI1 | VT_I2 | VT_I4 | VT_I8 => src_vt,
                    VT_EMPTY => VT_I2,
                    _ => VT_I4,
                };
                let r = if vt == VT_UI1 { !(i as u8) as i64 } else { !i };
                (Val::Int(r), vt)
            }
            Err(e) => return e,
        },
    };
    write(result, &val, vt)
}

pub unsafe extern "win64" fn VarNeg(v: *const c_void, result: *mut c_void) -> i32 {
    unary(v, result, true)
}

pub unsafe extern "win64" fn VarNot(v: *const c_void, result: *mut c_void) -> i32 {
    unary(v, result, false)
}

pub unsafe extern "win64" fn VarAbs(v: *const c_void, result: *mut c_void) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    let Some(a) = read(v) else {
        return DISP_E_TYPEMISMATCH;
    };
    let src_vt = *(v as *const u16) & !VT_BYREF;
    match a {
        Val::Null => write(result, &Val::Null, VT_NULL),
        Val::Real(r) => write(result, &Val::Real(r.abs()), src_vt.max(VT_R4)),
        Val::Cy(c) => write(result, &Val::Cy(c.abs()), VT_CY),
        Val::Date(d) => write(result, &Val::Real(d.abs()), VT_R8),
        other => match to_int(&other) {
            Ok(i) => write(
                result,
                &Val::Int(i.abs()),
                if src_vt == VT_I8 { VT_I8 } else { VT_I4 },
            ),
            Err(e) => e,
        },
    }
}

fn compare_text(a: &str, b: &str, flags: u32) -> std::cmp::Ordering {
    if flags & NORM_IGNORECASE != 0 {
        a.to_lowercase().cmp(&b.to_lowercase())
    } else {
        a.cmp(b)
    }
}

pub fn compare(a: &Val, b: &Val, flags: u32) -> i32 {
    use std::cmp::Ordering::*;
    if matches!(a, Val::Null) || matches!(b, Val::Null) {
        return VARCMP_NULL;
    }
    let ord = match (a, b) {
        (Val::Str(x), Val::Str(y)) => compare_text(x, y, flags),
        // Une chaîne est toujours plus grande qu'un nombre (VarCmp).
        (Val::Str(_), _) if !matches!(b, Val::Empty) => Greater,
        (_, Val::Str(_)) if !matches!(a, Val::Empty) => Less,
        (Val::Empty, Val::Str(y)) => compare_text("", y, flags),
        (Val::Str(x), Val::Empty) => compare_text(x, "", flags),
        _ => {
            let (x, y) = (to_real(a).unwrap_or(0.0), to_real(b).unwrap_or(0.0));
            x.partial_cmp(&y).unwrap_or(Equal)
        }
    };
    match ord {
        Less => VARCMP_LT,
        Equal => VARCMP_EQ,
        Greater => VARCMP_GT,
    }
}

pub unsafe extern "win64" fn VarCmp(
    left: *const c_void,
    right: *const c_void,
    _lcid: u32,
    flags: u32,
) -> i32 {
    match (read(left), read(right)) {
        (Some(a), Some(b)) => compare(&a, &b, flags),
        _ => DISP_E_TYPEMISMATCH,
    }
}

// ── Changement de type ──────────────────────────────────────────────

pub unsafe fn change_type(
    dest: *mut c_void,
    src: *const c_void,
    lcid: u32,
    flags: u16,
    vt: u16,
) -> i32 {
    if dest.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    let src_vt = *(src as *const u16);
    if src_vt == vt && vt & VT_BYREF == 0 {
        if !std::ptr::eq(dest, src) {
            return super::oleaut32::variant_copy(dest, src);
        }
        return 0;
    }
    let Some(val) = read(src) else {
        return DISP_E_TYPEMISMATCH;
    };
    if matches!(val, Val::Null) && vt != VT_NULL {
        return DISP_E_TYPEMISMATCH;
    }
    if vt == VT_BSTR {
        return match to_text(&val, lcid, flags as u32) {
            Ok(s) => write(dest, &Val::Str(s), VT_BSTR),
            Err(e) => e,
        };
    }
    if vt == VT_DATE {
        if let Val::Str(s) = &val {
            return match parse_date(s, lcid) {
                Some(d) => write(dest, &Val::Date(d), VT_DATE),
                None => DISP_E_TYPEMISMATCH,
            };
        }
    }
    write(dest, &val, vt)
}

pub unsafe extern "win64" fn VariantChangeTypeEx(
    dest: *mut c_void,
    src: *const c_void,
    lcid: u32,
    flags: u16,
    vt: u16,
) -> i32 {
    change_type(dest, src, lcid, flags, vt)
}

// ── Chaînes ─────────────────────────────────────────────────────────

unsafe fn out_bstr(out: *mut *mut u16, s: &str) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    let b = sys_alloc_string_len(w.as_ptr(), w.len() as u32);
    if b.is_null() {
        return E_OUTOFMEMORY;
    }
    *out = b;
    0
}

unsafe fn bstr_from(v: Val, lcid: u32, flags: u32, out: *mut *mut u16) -> i32 {
    match to_text(&v, lcid, flags) {
        Ok(s) => out_bstr(out, &s),
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn VarBstrFromI4(
    v: i32,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Int(v as i64), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromI2(
    v: i16,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Int(v as i64), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromUI1(
    v: u8,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Int(v as i64), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromR8(
    v: f64,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Real(v), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromR4(
    v: f32,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Real(v as f64), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromDate(
    v: f64,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Date(v), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromCy(
    v: i64,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Cy(v), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrFromBool(
    v: i16,
    lcid: u32,
    flags: u32,
    out: *mut *mut u16,
) -> i32 {
    bstr_from(Val::Bool(v != 0), lcid, flags, out)
}

pub unsafe extern "win64" fn VarBstrCat(a: *const u16, b: *const u16, out: *mut *mut u16) -> i32 {
    let s = bstr_text(a) + &bstr_text(b);
    out_bstr(out, &s)
}

unsafe fn from_str<T>(
    s: *const u16,
    out: *mut T,
    conv: impl FnOnce(&Val) -> Result<T, i32>,
) -> i32 {
    if s.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let text = super::wstr(s);
    match conv(&Val::Str(text)) {
        Ok(v) => {
            out.write_unaligned(v);
            0
        }
        Err(e) => e,
    }
}

fn ranged<T: TryFrom<i64>>(v: &Val) -> Result<T, i32> {
    T::try_from(to_int(v)?).map_err(|_| DISP_E_OVERFLOW)
}

pub unsafe extern "win64" fn VarI4FromStr(s: *const u16, _l: u32, _f: u32, out: *mut i32) -> i32 {
    from_str(s, out, ranged::<i32>)
}

pub unsafe extern "win64" fn VarI2FromStr(s: *const u16, _l: u32, _f: u32, out: *mut i16) -> i32 {
    from_str(s, out, ranged::<i16>)
}

pub unsafe extern "win64" fn VarUI1FromStr(s: *const u16, _l: u32, _f: u32, out: *mut u8) -> i32 {
    from_str(s, out, ranged::<u8>)
}

pub unsafe extern "win64" fn VarR8FromStr(s: *const u16, _l: u32, _f: u32, out: *mut f64) -> i32 {
    from_str(s, out, to_real)
}

pub unsafe extern "win64" fn VarR4FromStr(s: *const u16, _l: u32, _f: u32, out: *mut f32) -> i32 {
    from_str(s, out, |v| match to_real(v)? {
        r if r.is_finite() && r.abs() > f32::MAX as f64 => Err(DISP_E_OVERFLOW),
        r => Ok(r as f32),
    })
}

pub unsafe extern "win64" fn VarCyFromStr(s: *const u16, _l: u32, _f: u32, out: *mut i64) -> i32 {
    from_str(s, out, to_cy)
}

pub unsafe extern "win64" fn VarBoolFromStr(s: *const u16, _l: u32, _f: u32, out: *mut i16) -> i32 {
    from_str(s, out, |v| to_bool(v).map(|b| if b { -1 } else { 0 }))
}

pub unsafe extern "win64" fn VarDateFromStr(
    s: *const u16,
    lcid: u32,
    _f: u32,
    out: *mut f64,
) -> i32 {
    from_str(s, out, |v| match v {
        Val::Str(t) => parse_date(t, lcid).ok_or(DISP_E_TYPEMISMATCH),
        _ => Err(DISP_E_TYPEMISMATCH),
    })
}

// ── Dates système ───────────────────────────────────────────────────

/// SYSTEMTIME : huit u16 (année, mois, jour de la semaine, jour, heure,
/// minute, seconde, millième).
pub unsafe extern "win64" fn VariantTimeToSystemTime(d: f64, st: *mut u16) -> i32 {
    if st.is_null() {
        return 0;
    }
    match ole_to_parts(d) {
        Some(p) => {
            std::ptr::copy_nonoverlapping(p.as_ptr(), st, 8);
            1
        }
        None => 0,
    }
}

pub unsafe extern "win64" fn SystemTimeToVariantTime(st: *const u16, out: *mut f64) -> i32 {
    if st.is_null() || out.is_null() {
        return 0;
    }
    let p = std::slice::from_raw_parts(st, 8);
    match parts_to_ole(
        p[0] as i64,
        p[1] as u32,
        p[3] as u32,
        p[4] as u32,
        p[5] as u32,
        p[6] as u32,
        p[7] as u32,
    ) {
        Some(d) => {
            out.write_unaligned(d);
            1
        }
        None => 0,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "VariantChangeTypeEx" => fn VariantChangeTypeEx,
        "VarAdd" => fn VarAdd, "VarSub" => fn VarSub, "VarMul" => fn VarMul,
        "VarDiv" => fn VarDiv, "VarIdiv" => fn VarIdiv, "VarMod" => fn VarMod,
        "VarAnd" => fn VarAnd, "VarOr" => fn VarOr, "VarXor" => fn VarXor,
        "VarEqv" => fn VarEqv, "VarImp" => fn VarImp, "VarPow" => fn VarPow,
        "VarCat" => fn VarCat, "VarNeg" => fn VarNeg, "VarNot" => fn VarNot,
        "VarAbs" => fn VarAbs, "VarCmp" => fn VarCmp,
        "VarBstrFromI2" => fn VarBstrFromI2, "VarBstrFromI4" => fn VarBstrFromI4,
        "VarBstrFromUI1" => fn VarBstrFromUI1, "VarBstrFromR4" => fn VarBstrFromR4,
        "VarBstrFromR8" => fn VarBstrFromR8, "VarBstrFromCy" => fn VarBstrFromCy,
        "VarBstrFromDate" => fn VarBstrFromDate, "VarBstrFromBool" => fn VarBstrFromBool,
        "VarBstrCat" => fn VarBstrCat,
        "VarI2FromStr" => fn VarI2FromStr, "VarI4FromStr" => fn VarI4FromStr,
        "VarUI1FromStr" => fn VarUI1FromStr, "VarR4FromStr" => fn VarR4FromStr,
        "VarR8FromStr" => fn VarR8FromStr, "VarCyFromStr" => fn VarCyFromStr,
        "VarBoolFromStr" => fn VarBoolFromStr, "VarDateFromStr" => fn VarDateFromStr,
        "VariantTimeToSystemTime" => fn VariantTimeToSystemTime,
        "SystemTimeToVariantTime" => fn SystemTimeToVariantTime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn var(vt: u16, bits: u64) -> [u8; 24] {
        let mut v = [0u8; 24];
        v[..2].copy_from_slice(&vt.to_le_bytes());
        v[8..16].copy_from_slice(&bits.to_le_bytes());
        v
    }

    unsafe fn bstr_var(s: &str) -> [u8; 24] {
        let mut v = [0u8; 24];
        let w: Vec<u16> = s.encode_utf16().collect();
        v[..2].copy_from_slice(&VT_BSTR.to_le_bytes());
        let b = sys_alloc_string_len(w.as_ptr(), w.len() as u32);
        crate::wow32::write_ptr(v.as_mut_ptr() as usize + 8, b as usize);
        v
    }

    #[test]
    fn calcul_sur_les_variants() {
        unsafe {
            let a = var(VT_I4, 40);
            let b = var(VT_I2, 2);
            let mut r = [0u8; 24];
            assert_eq!(
                VarAdd(a.as_ptr().cast(), b.as_ptr().cast(), r.as_mut_ptr().cast()),
                0
            );
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Int(42)));
            assert_eq!(u16::from_le_bytes([r[0], r[1]]), VT_I4);
            // Débordement d'un I4 : le résultat passe en R8.
            let big = var(VT_I4, i32::MAX as u32 as u64);
            VarAdd(
                big.as_ptr().cast(),
                b.as_ptr().cast(),
                r.as_mut_ptr().cast(),
            );
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Real(2_147_483_649.0)));
            // Division par zéro.
            let z = var(VT_I4, 0);
            assert_eq!(
                VarDiv(a.as_ptr().cast(), z.as_ptr().cast(), r.as_mut_ptr().cast()),
                DISP_E_DIVBYZERO
            );
            // Chaîne + chaîne = concaténation, chaîne + nombre = nombre.
            let s1 = bstr_var("4");
            let s2 = bstr_var("2");
            VarAdd(
                s1.as_ptr().cast(),
                s2.as_ptr().cast(),
                r.as_mut_ptr().cast(),
            );
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Str("42".into())));
            VarMul(s1.as_ptr().cast(), b.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Real(8.0)));
            // Null se propage.
            let n = var(VT_NULL, 0);
            VarSub(a.as_ptr().cast(), n.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(u16::from_le_bytes([r[0], r[1]]), VT_NULL);
            assert_eq!(
                VarCmp(a.as_ptr().cast(), n.as_ptr().cast(), 0, 0),
                VARCMP_NULL
            );
            assert_eq!(
                VarCmp(a.as_ptr().cast(), b.as_ptr().cast(), 0, 0),
                VARCMP_GT
            );
            let s3 = bstr_var("ABC");
            let s4 = bstr_var("abc");
            assert_eq!(
                VarCmp(s3.as_ptr().cast(), s4.as_ptr().cast(), 0, NORM_IGNORECASE),
                VARCMP_EQ
            );
            assert_eq!(
                VarCmp(s3.as_ptr().cast(), s4.as_ptr().cast(), 0, 0),
                VARCMP_LT
            );
            // Not et Neg.
            VarNot(z.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Int(-1)));
            VarNeg(a.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Int(-40)));
            let t = var(VT_BOOL, 0xFFFF);
            VarAnd(t.as_ptr().cast(), t.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Bool(true)));
            VarCat(a.as_ptr().cast(), s3.as_ptr().cast(), r.as_mut_ptr().cast());
            assert_eq!(read(r.as_ptr().cast()), Some(Val::Str("40ABC".into())));
            variant_clear(r.as_mut_ptr().cast());
        }
    }

    #[test]
    fn dates_ole() {
        // 0 = 30/12/1899 minuit ; 2.25 = 1er janvier 1900 à 6 h ;
        // -1.25 = 29/12/1899 à 6 h (partie décimale toujours positive).
        assert_eq!(ole_to_parts(0.0).unwrap()[..4], [1899, 12, 6, 30]);
        assert_eq!(ole_to_parts(2.25).unwrap(), [1900, 1, 1, 1, 6, 0, 0, 0]);
        assert_eq!(ole_to_parts(-1.25).unwrap()[3..5], [29, 6]);
        assert_eq!(parts_to_ole(1900, 1, 1, 6, 0, 0, 0), Some(2.25));
        assert_eq!(parts_to_ole(1899, 12, 29, 6, 0, 0, 0), Some(-1.25));
        // 8 novembre 2026 = 46334.
        assert_eq!(parts_to_ole(2026, 11, 8, 0, 0, 0, 0), Some(46_334.0));
        assert_eq!(parts_to_ole(2026, 2, 29, 0, 0, 0, 0), None);
        assert_eq!(parse_date("2026-11-08 12:00", 0x409), Some(46_334.5));
        assert_eq!(parse_date("11/08/2026", 0x409), Some(46_334.0));
        assert_eq!(parse_date("08/11/2026", 0x40C), Some(46_334.0));
        assert_eq!(parse_date("6:00 PM", 0x409), Some(0.75));
        unsafe {
            let mut st = [0u16; 8];
            assert_eq!(VariantTimeToSystemTime(46_334.5, st.as_mut_ptr()), 1);
            assert_eq!(st, [2026, 11, 0, 8, 12, 0, 0, 0]);
            let mut d = 0.0;
            assert_eq!(SystemTimeToVariantTime(st.as_ptr(), &mut d), 1);
            assert_eq!(d, 46_334.5);
            let mut b: *mut u16 = std::ptr::null_mut();
            assert_eq!(VarBstrFromDate(46_334.0, 0x409, 0, &mut b), 0);
            assert_eq!(bstr_text(b), "11/8/2026");
            super::super::oleaut32::sys_free_string(b);
        }
    }

    #[test]
    fn chaines_et_changement_de_type() {
        unsafe {
            let mut i = 0i32;
            let w: Vec<u16> = " 123 \0".encode_utf16().collect();
            assert_eq!(VarI4FromStr(w.as_ptr(), 0, 0, &mut i), 0);
            assert_eq!(i, 123);
            let w: Vec<u16> = "abc\0".encode_utf16().collect();
            assert_eq!(VarI4FromStr(w.as_ptr(), 0, 0, &mut i), DISP_E_TYPEMISMATCH);
            let w: Vec<u16> = "99999999999\0".encode_utf16().collect();
            assert_eq!(VarI4FromStr(w.as_ptr(), 0, 0, &mut i), DISP_E_OVERFLOW);
            let mut r = 0f64;
            let w: Vec<u16> = "2,5\0".encode_utf16().collect();
            assert_eq!(VarR8FromStr(w.as_ptr(), 0, 0, &mut r), 0);
            assert_eq!(r, 2.5);
            let mut b: *mut u16 = std::ptr::null_mut();
            VarBstrFromR8(0.1 + 0.2, 0x409, 0, &mut b);
            assert_eq!(bstr_text(b), "0.3");
            VarBstrFromR8(2.5, 0x40C, 0, &mut b);
            assert_eq!(bstr_text(b), "2,5");
            VarBstrFromBool(-1, 0x409, 0, &mut b);
            assert_eq!(bstr_text(b), "True");

            // VariantChangeTypeEx : R8 → I4 arrondi au pair, BSTR → DATE.
            let src = var(VT_R8, 2.5f64.to_bits());
            let mut dst = [0u8; 24];
            assert_eq!(
                VariantChangeTypeEx(
                    dst.as_mut_ptr().cast(),
                    src.as_ptr().cast(),
                    0x409,
                    0,
                    VT_I4
                ),
                0
            );
            assert_eq!(read(dst.as_ptr().cast()), Some(Val::Int(2)));
            let s = bstr_var("2026-11-08");
            assert_eq!(
                VariantChangeTypeEx(
                    dst.as_mut_ptr().cast(),
                    s.as_ptr().cast(),
                    0x409,
                    0,
                    VT_DATE
                ),
                0
            );
            assert_eq!(read(dst.as_ptr().cast()), Some(Val::Date(46_334.0)));
            let n = var(VT_NULL, 0);
            assert_eq!(
                VariantChangeTypeEx(dst.as_mut_ptr().cast(), n.as_ptr().cast(), 0x409, 0, VT_I4),
                DISP_E_TYPEMISMATCH
            );
            // Référence (VT_BYREF | VT_I4).
            let target = 7i32;
            let byref = var(VT_BYREF | VT_I4, &target as *const i32 as u64);
            assert_eq!(
                VariantChangeTypeEx(
                    dst.as_mut_ptr().cast(),
                    byref.as_ptr().cast(),
                    0x409,
                    0,
                    VT_BSTR
                ),
                0
            );
            assert_eq!(read(dst.as_ptr().cast()), Some(Val::Str("7".into())));
            // DECIMAL aller-retour.
            let i = var(VT_I4, 1234);
            assert_eq!(
                VariantChangeTypeEx(dst.as_mut_ptr().cast(), i.as_ptr().cast(), 0, 0, VT_DECIMAL),
                0
            );
            assert_eq!(read(dst.as_ptr().cast()), Some(Val::Real(1234.0)));
        }
    }
}
