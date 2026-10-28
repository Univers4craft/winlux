//! Moteur de formatage façon printf, tel que le décrit la documentation
//! publique de la CRT Microsoft (spécifications de format, préfixes de
//! taille I32/I64/I, `long` sur 32 bits, exposants à 3 chiffres pour msvcrt).

use crate::thunks::VaList;

#[derive(Clone, Copy)]
pub struct Opts {
    /// Fonction « large » (wprintf…) : %s et %c désignent des caractères 16 bits.
    pub wide: bool,
    /// msvcrt.dll écrit 1e+020 ; la CRT universelle (ucrtbase) écrit 1e+20.
    pub three_digit_exp: bool,
}

pub const MSVCRT: Opts = Opts { wide: false, three_digit_exp: true };

#[derive(Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: Option<usize>,
    prec: Option<usize>,
}

pub unsafe fn wide_len(p: *const u16) -> usize {
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    n
}

pub unsafe fn narrow_len(p: *const u8) -> usize {
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    n
}

fn pad(out: &mut Vec<u8>, body: &[u8], s: &Spec, numeric_prefix_len: usize) {
    let w = s.width.unwrap_or(0);
    if body.len() >= w {
        out.extend_from_slice(body);
    } else if s.left {
        out.extend_from_slice(body);
        out.resize(out.len() + w - body.len(), b' ');
    } else if s.zero {
        // Les zéros se placent après le signe / préfixe 0x.
        out.extend_from_slice(&body[..numeric_prefix_len]);
        out.resize(out.len() + w - body.len(), b'0');
        out.extend_from_slice(&body[numeric_prefix_len..]);
    } else {
        out.resize(out.len() + w - body.len(), b' ');
        out.extend_from_slice(body);
    }
}

fn fmt_int(out: &mut Vec<u8>, s: &Spec, neg: bool, mag: u64, conv: u8) {
    let (radix, upper) = match conv {
        b'x' => (16, false),
        b'X' => (16, true),
        b'o' => (8, false),
        _ => (10, false),
    };
    let mut digits = Vec::new();
    let mut v = mag;
    while v > 0 {
        let d = (v % radix) as u8;
        digits.push(if d < 10 { b'0' + d } else if upper { b'A' + d - 10 } else { b'a' + d - 10 });
        v /= radix;
    }
    if let Some(p) = s.prec {
        while digits.len() < p {
            digits.push(b'0');
        }
    } else if digits.is_empty() {
        digits.push(b'0');
    }
    if conv == b'o' && s.alt && digits.last() != Some(&b'0') {
        digits.push(b'0');
    }
    digits.reverse();
    let mut prefix: Vec<u8> = Vec::new();
    if neg {
        prefix.push(b'-');
    } else if matches!(conv, b'd' | b'i') && s.plus {
        prefix.push(b'+');
    } else if matches!(conv, b'd' | b'i') && s.space {
        prefix.push(b' ');
    }
    if s.alt && mag != 0 && radix == 16 {
        prefix.extend_from_slice(if upper { b"0X" } else { b"0x" });
    }
    let plen = prefix.len();
    prefix.extend_from_slice(&digits);
    let s2 = Spec { zero: s.zero && s.prec.is_none(), ..*s };
    pad(out, &prefix, &s2, plen);
}

impl Copy for Spec {}
impl Clone for Spec {
    fn clone(&self) -> Self {
        *self
    }
}

fn exp_string(e: i32, three: bool) -> String {
    let sign = if e < 0 { '-' } else { '+' };
    if three {
        format!("{sign}{:03}", e.abs())
    } else {
        format!("{sign}{:02}", e.abs())
    }
}

/// Écrit `v` en notation scientifique avec `p` décimales : (mantisse, exposant).
fn sci(v: f64, p: usize) -> (String, i32) {
    let s = format!("{:.*e}", p, v);
    let (m, e) = s.split_once('e').unwrap();
    (m.to_string(), e.parse().unwrap())
}

fn strip_zeros(s: &mut String) {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
}

fn fmt_float(out: &mut Vec<u8>, s: &Spec, v: f64, conv: u8, o: Opts) {
    let upper = conv.is_ascii_uppercase();
    let neg = v.is_sign_negative() && !(v == 0.0 && !v.is_sign_negative());
    let a = v.abs();
    let mut body = if a.is_nan() {
        "nan".to_string()
    } else if a.is_infinite() {
        "inf".to_string()
    } else {
        let p = s.prec.unwrap_or(6);
        match conv.to_ascii_lowercase() {
            b'f' => {
                let mut r = format!("{:.*}", p, a);
                if s.alt && p == 0 {
                    r.push('.');
                }
                r
            }
            b'e' => {
                let (mut m, e) = sci(a, p);
                if s.alt && p == 0 {
                    m.push('.');
                }
                format!("{m}e{}", exp_string(e, o.three_digit_exp))
            }
            b'g' => {
                let p = if p == 0 { 1 } else { p };
                let (_, x) = sci(a, p - 1);
                if x < -4 || x >= p as i32 {
                    let (mut m, e) = sci(a, p - 1);
                    if !s.alt {
                        strip_zeros(&mut m);
                    }
                    format!("{m}e{}", exp_string(e, o.three_digit_exp))
                } else {
                    let mut r = format!("{:.*}", (p as i32 - 1 - x).max(0) as usize, a);
                    if !s.alt {
                        strip_zeros(&mut r);
                    }
                    r
                }
            }
            _ => {
                // %a : hexadécimal flottant.
                let bits = a.to_bits();
                let exp = ((bits >> 52) & 0x7ff) as i32;
                let man = bits & ((1u64 << 52) - 1);
                if a == 0.0 {
                    "0x0p+0".into()
                } else {
                    let (lead, e) = if exp == 0 { (0, -1022) } else { (1, exp - 1023) };
                    let mut h = format!("{:013x}", man);
                    while h.ends_with('0') {
                        h.pop();
                    }
                    if h.is_empty() {
                        format!("0x{lead}p{e:+}")
                    } else {
                        format!("0x{lead}.{h}p{e:+}")
                    }
                }
            }
        }
    };
    if upper {
        body = body.to_uppercase();
    }
    let mut full = String::new();
    if neg {
        full.push('-');
    } else if s.plus {
        full.push('+');
    } else if s.space {
        full.push(' ');
    }
    let plen = full.len();
    full.push_str(&body);
    let s2 = Spec { zero: s.zero && a.is_finite(), ..*s };
    pad(out, full.as_bytes(), &s2, plen);
}

#[derive(PartialEq, Clone, Copy)]
enum Len {
    None,
    Hh,
    H,
    L,
    Ll,
    Size,
    LongDouble,
    Wide,
}

/// Formate `fmt` (octets, terminés ou non par 0) avec les arguments de `va`.
pub unsafe fn format(fmt: &[u8], va: &mut VaList, o: Opts) -> Vec<u8> {
    let mut out = Vec::with_capacity(fmt.len() + 16);
    let mut i = 0;
    let n = fmt.iter().position(|&c| c == 0).unwrap_or(fmt.len());
    let fmt = &fmt[..n];
    while i < fmt.len() {
        let c = fmt[i];
        if c != b'%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let mut s = Spec::default();
        while i < fmt.len() {
            match fmt[i] {
                b'-' => s.left = true,
                b'+' => s.plus = true,
                b' ' => s.space = true,
                b'#' => s.alt = true,
                b'0' => s.zero = true,
                _ => break,
            }
            i += 1;
        }
        if i < fmt.len() && fmt[i] == b'*' {
            let w = va.next_u64() as i32;
            if w < 0 {
                s.left = true;
            }
            s.width = Some(w.unsigned_abs() as usize);
            i += 1;
        } else {
            let st = i;
            while i < fmt.len() && fmt[i].is_ascii_digit() {
                i += 1;
            }
            if i > st {
                s.width = std::str::from_utf8(&fmt[st..i]).ok().and_then(|x| x.parse().ok());
            }
        }
        if i < fmt.len() && fmt[i] == b'.' {
            i += 1;
            if i < fmt.len() && fmt[i] == b'*' {
                let p = va.next_u64() as i32;
                s.prec = if p < 0 { None } else { Some(p as usize) };
                i += 1;
            } else {
                let st = i;
                while i < fmt.len() && fmt[i].is_ascii_digit() {
                    i += 1;
                }
                s.prec = Some(std::str::from_utf8(&fmt[st..i]).ok().and_then(|x| x.parse().ok()).unwrap_or(0));
            }
        }
        let mut len = Len::None;
        while i < fmt.len() {
            let rest = &fmt[i..];
            if rest.starts_with(b"I64") {
                len = Len::Ll;
                i += 3;
            } else if rest.starts_with(b"I32") {
                len = Len::L;
                i += 3;
            } else if rest.starts_with(b"hh") {
                len = Len::Hh;
                i += 2;
            } else if rest.starts_with(b"ll") {
                len = Len::Ll;
                i += 2;
            } else {
                match rest[0] {
                    b'h' => len = Len::H,
                    b'l' => len = Len::L,
                    b'w' => len = Len::Wide,
                    b'L' => len = Len::LongDouble,
                    b'I' | b'z' | b'j' | b't' => len = Len::Size,
                    _ => break,
                }
                i += 1;
            }
        }
        if i >= fmt.len() {
            break;
        }
        let conv = fmt[i];
        i += 1;
        match conv {
            b'd' | b'i' => {
                let raw = va.next_u64();
                let v: i64 = match len {
                    Len::Hh => raw as i8 as i64,
                    Len::H => raw as i16 as i64,
                    Len::Ll | Len::Size => raw as i64,
                    _ => raw as i32 as i64, // int et long font 32 bits sous Windows
                };
                fmt_int(&mut out, &s, v < 0, v.unsigned_abs(), conv);
            }
            b'u' | b'x' | b'X' | b'o' => {
                let raw = va.next_u64();
                let v: u64 = match len {
                    Len::Hh => raw as u8 as u64,
                    Len::H => raw as u16 as u64,
                    Len::Ll | Len::Size => raw,
                    _ => raw as u32 as u64,
                };
                fmt_int(&mut out, &s, false, v, conv);
            }
            b'p' => {
                let v = va.next_u64();
                // La CRT Microsoft affiche les pointeurs sur 16 chiffres hexadécimaux majuscules.
                let body = format!("{v:016X}");
                pad(&mut out, body.as_bytes(), &Spec { zero: false, ..s }, 0);
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                let v = va.next_f64();
                fmt_float(&mut out, &s, v, conv, o);
            }
            b'c' | b'C' => {
                let raw = va.next_u64();
                let wide = match len {
                    Len::H => false,
                    Len::L | Len::Wide => true,
                    _ => o.wide ^ (conv == b'C'),
                };
                let body: Vec<u8> = if wide {
                    String::from_utf16_lossy(&[raw as u16]).into_bytes()
                } else {
                    vec![raw as u8]
                };
                pad(&mut out, &body, &Spec { zero: false, ..s }, 0);
            }
            b's' | b'S' | b'Z' => {
                let p = va.next_u64() as usize;
                let wide = match len {
                    Len::H => false,
                    Len::L | Len::Wide => true,
                    _ => o.wide ^ (conv == b'S'),
                };
                let mut body: Vec<u8> = if p == 0 {
                    b"(null)".to_vec()
                } else if wide {
                    let n = wide_len(p as *const u16);
                    let n = s.prec.map_or(n, |pr| pr.min(n));
                    String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, n)).into_bytes()
                } else {
                    let mut n = 0;
                    let lim = s.prec.unwrap_or(usize::MAX);
                    while n < lim && *(p as *const u8).add(n) != 0 {
                        n += 1;
                    }
                    std::slice::from_raw_parts(p as *const u8, n).to_vec()
                };
                if let (Some(pr), true) = (s.prec, wide) {
                    body.truncate(pr.max(body.len().min(pr)));
                }
                pad(&mut out, &body, &Spec { zero: false, ..s }, 0);
            }
            b'n' => {
                // %n est désactivé par défaut dans la CRT moderne : on consomme l'argument sans écrire.
                va.next_u64();
            }
            b'%' => out.push(b'%'),
            other => {
                out.push(b'%');
                out.push(other);
            }
        }
    }
    out
}
