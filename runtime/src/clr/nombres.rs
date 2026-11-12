//! Mise en texte des nombres pour la mscorlib Winlux (#1956) : chaînes de
//! format standard (G, D, X, N, F, E, P, R, C) et personnalisées simples
//! (`0.00`, `#,##0`…), avec les conventions de la culture invariante.
//!
//! Écrit d'après la documentation publique « Standard numeric format
//! strings » et « Custom numeric format strings » ; aucun code de Microsoft,
//! de Mono ou de Wine. Comme le .NET Framework, un double est d'abord ramené
//! à 15 chiffres significatifs (7 pour un float) avant toute mise en forme,
//! et les arrondis décimaux se font à la demi-unité loin de zéro.

/// Nombre décimal : valeur = 0,d₁d₂d₃… × 10^exp. Zéro : `d` vide.
#[derive(Debug, Clone, PartialEq)]
struct Num {
    neg: bool,
    d: Vec<u8>,
    exp: i32,
}

impl Num {
    fn from_int(v: i128) -> Num {
        let s = v.unsigned_abs().to_string();
        let mut n = Num {
            neg: v < 0,
            exp: s.len() as i32,
            d: s.into_bytes(),
        };
        n.trim();
        n
    }

    /// Chiffres significatifs d'un flottant fini : `sig` chiffres, ou le
    /// plus court qui relit la même valeur si `sig` vaut 0 (en float si
    /// `single`).
    fn from_float(v: f64, sig: usize, single: bool) -> Num {
        let s = match (sig, single) {
            (0, true) => format!("{:e}", (v as f32).abs()),
            (0, false) => format!("{:e}", v.abs()),
            _ => format!("{:.*e}", sig - 1, v.abs()),
        };
        let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let exp: i32 = exp.parse().unwrap_or(0);
        let mut n = Num {
            neg: v.is_sign_negative(),
            d: mantissa.bytes().filter(u8::is_ascii_digit).collect(),
            exp: exp + 1,
        };
        n.trim();
        n
    }

    fn trim(&mut self) {
        while self.d.last() == Some(&b'0') {
            self.d.pop();
        }
        if self.d.is_empty() {
            self.exp = 0;
        }
    }

    fn is_zero(&self) -> bool {
        self.d.is_empty()
    }

    /// Garde `keep` chiffres significatifs, arrondis loin de zéro.
    fn round(&mut self, keep: i32) {
        if keep < 0 {
            self.d.clear();
        } else if (keep as usize) < self.d.len() {
            let keep = keep as usize;
            let up = self.d[keep] >= b'5';
            self.d.truncate(keep);
            if up {
                let mut i = keep;
                loop {
                    if i == 0 {
                        self.d.insert(0, b'1');
                        self.exp += 1;
                        break;
                    }
                    i -= 1;
                    if self.d[i] == b'9' {
                        self.d[i] = b'0';
                    } else {
                        self.d[i] += 1;
                        break;
                    }
                }
            }
        }
        self.trim();
    }

    fn digit(&self, i: i32) -> char {
        if i < 0 || i as usize >= self.d.len() {
            '0'
        } else {
            self.d[i as usize] as char
        }
    }

    fn int_digits(&self) -> String {
        if self.exp <= 0 {
            "0".into()
        } else {
            (0..self.exp).map(|i| self.digit(i)).collect()
        }
    }

    fn frac_digits(&self, count: i32) -> String {
        (0..count).map(|k| self.digit(self.exp + k)).collect()
    }

    fn sign(&self) -> &'static str {
        if self.neg && !self.is_zero() {
            "-"
        } else {
            ""
        }
    }
}

fn group(int: &str) -> String {
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fixed(mut n: Num, decimals: i32, grouped: bool) -> String {
    n.round(n.exp + decimals);
    let int = n.int_digits();
    let int = if grouped { group(&int) } else { int };
    let mut s = format!("{}{}", n.sign(), int);
    if decimals > 0 {
        s.push('.');
        s.push_str(&n.frac_digits(decimals));
    }
    s
}

fn exponent(e: i32, min_digits: usize, letter: char) -> String {
    let sign = if e < 0 { '-' } else { '+' };
    format!("{letter}{sign}{:0min_digits$}", e.abs())
}

fn scientific(mut n: Num, decimals: i32, letter: char) -> String {
    n.round(decimals + 1);
    let e = if n.is_zero() { 0 } else { n.exp - 1 };
    let mut s = format!("{}{}", n.sign(), n.digit(0));
    if decimals > 0 {
        s.push('.');
        s.push_str(&(1..=decimals).map(|i| n.digit(i)).collect::<String>());
    }
    s + &exponent(e, 3, letter)
}

/// Format « G » : virgule fixe tant que l'exposant reste dans [-5, p[.
fn general(n: Num, precision: i32, letter: char) -> String {
    general_cut(n, precision, precision, letter)
}

/// `keep` chiffres gardés, virgule fixe tant que l'exposant est < `cutoff`.
fn general_cut(mut n: Num, keep: i32, precision: i32, letter: char) -> String {
    n.round(keep);
    if n.is_zero() {
        return "0".into();
    }
    let e = n.exp - 1;
    let len = n.d.len() as i32;
    if e >= -5 && e < precision {
        let mut s = format!("{}{}", n.sign(), n.int_digits());
        if len > n.exp {
            s.push('.');
            s.push_str(&n.frac_digits(len - n.exp));
        }
        s
    } else {
        let mut s = format!("{}{}", n.sign(), n.digit(0));
        if len > 1 {
            s.push('.');
            s.push_str(&(1..len).map(|i| n.digit(i)).collect::<String>());
        }
        s + &exponent(e, 2, letter)
    }
}

/// Format personnalisé : préfixe, chiffres `0`/`#`, séparateur `,`,
/// décimales après `.`, suffixe. Une seule section.
fn custom(n: Num, pattern: &str) -> String {
    let pattern = pattern.split(';').next().unwrap_or("");
    let is_spec = |c: char| matches!(c, '0' | '#' | '.' | ',');
    let Some(start) = pattern.find(is_spec) else {
        return pattern.to_string();
    };
    let end = pattern.rfind(is_spec).map_or(start, |i| i + 1);
    let (prefix, body, suffix) = (&pattern[..start], &pattern[start..end], &pattern[end..]);
    let (int_pat, frac_pat) = body.split_once('.').unwrap_or((body, ""));
    let min_int = int_pat.chars().filter(|&c| c == '0').count();
    let grouped = int_pat.contains(',') && int_pat.trim_end_matches(',').contains(',');
    // Une virgule en fin de partie entière divise par mille.
    let scale = int_pat.len() - int_pat.trim_end_matches(',').len();
    let min_frac = frac_pat.chars().take_while(|&c| c == '0').count() as i32;
    let max_frac = frac_pat.chars().filter(|&c| c == '0' || c == '#').count() as i32;
    let mut n = n;
    if !n.is_zero() {
        n.exp -= 3 * scale as i32;
    }
    n.round(n.exp + max_frac);
    let mut int = n.int_digits();
    if int == "0" && min_int == 0 {
        int.clear();
    }
    while int.len() < min_int {
        int.insert(0, '0');
    }
    if grouped {
        int = group(&int);
    }
    let mut frac = n.frac_digits(max_frac);
    while frac.len() as i32 > min_frac && frac.ends_with('0') {
        frac.pop();
    }
    let mut s = format!("{}{}{}", n.sign(), prefix, int);
    if !frac.is_empty() {
        s.push('.');
        s.push_str(&frac);
    }
    s + suffix
}

/// Lettre et précision d'un format standard (« X8 » → ('X', Some(8))).
fn standard(format: &str) -> Option<(char, Option<i32>)> {
    let mut chars = format.chars();
    let letter = chars.next()?;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let rest = chars.as_str();
    if rest.is_empty() {
        return Some((letter, None));
    }
    if rest.len() <= 2 && rest.bytes().all(|b| b.is_ascii_digit()) {
        return Some((letter, rest.parse().ok()));
    }
    None
}

fn apply(n: Num, format: &str, default_g: i32) -> String {
    let Some((letter, precision)) = standard(format) else {
        return custom(n, format);
    };
    let upper = letter.to_ascii_uppercase();
    let e = if letter.is_ascii_lowercase() {
        'e'
    } else {
        'E'
    };
    match upper {
        'D' => {
            let mut digits = n.int_digits();
            while (digits.len() as i32) < precision.unwrap_or(0) {
                digits.insert(0, '0');
            }
            format!("{}{}", n.sign(), digits)
        }
        'F' => fixed(n, precision.unwrap_or(2), false),
        'N' => fixed(n, precision.unwrap_or(2), true),
        'C' => {
            let neg = n.neg && !n.is_zero();
            let mut n = n;
            n.neg = false;
            let s = format!("¤{}", fixed(n, precision.unwrap_or(2), true));
            if neg {
                format!("({s})")
            } else {
                s
            }
        }
        'P' => {
            let mut n = n;
            if !n.is_zero() {
                n.exp += 2;
            }
            format!("{} %", fixed(n, precision.unwrap_or(2), true))
        }
        'E' => scientific(n, precision.unwrap_or(6), e),
        'G' => {
            let p = precision.filter(|&p| p > 0).unwrap_or(default_g);
            general(n, p, e)
        }
        _ => general(n, default_g, 'E'),
    }
}

/// Entier signé ou non sur `size` octets.
pub fn format_integer(value: i64, unsigned: bool, size: usize, format: &str) -> String {
    let bits = (size.clamp(1, 8) * 8) as u32;
    let mask: u64 = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let raw = value as u64 & mask;
    let wide: i128 = if unsigned {
        raw as i128
    } else {
        // Extension de signe depuis `bits`.
        let shift = 64 - bits;
        (((raw << shift) as i64) >> shift) as i128
    };
    if let Some((letter, precision)) = standard(format) {
        if letter.eq_ignore_ascii_case(&'X') {
            let s = if letter == 'x' {
                format!("{raw:x}")
            } else {
                format!("{raw:X}")
            };
            let width = precision.unwrap_or(0) as usize;
            return format!("{s:0>width$}");
        }
        if letter.eq_ignore_ascii_case(&'R') {
            return wide.to_string();
        }
    }
    if format.is_empty() {
        return wide.to_string();
    }
    apply(Num::from_int(wide), format, i32::MAX)
}

/// Double (ou float si `single`).
pub fn format_double(value: f64, single: bool, format: &str) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sig = if single { 7 } else { 15 };
    if let Some((letter, _)) = standard(format) {
        if letter.eq_ignore_ascii_case(&'R') {
            // Le plus court qui relit la même valeur.
            return general_cut(Num::from_float(value, 0, single), i32::MAX, sig, 'E');
        }
    }
    let n = Num::from_float(value, sig as usize, single);
    if format.is_empty() {
        return general(n, sig, 'E');
    }
    apply(n, format, sig)
}

/// Lecture invariante d'un double (« 1.5 », « -2e3 », « Infinity », « NaN »).
pub fn parse_double(text: &str) -> Option<f64> {
    let t = text.trim();
    match t {
        "Infinity" | "+Infinity" | "∞" => return Some(f64::INFINITY),
        "-Infinity" | "-∞" => return Some(f64::NEG_INFINITY),
        "NaN" => return Some(f64::NAN),
        _ => {}
    }
    // Pas de « inf » ni de « nan » minuscules : .NET les refuse.
    if t.is_empty()
        || t.bytes()
            .any(|b| b.is_ascii_alphabetic() && b != b'e' && b != b'E')
    {
        return None;
    }
    let cleaned: String = t.chars().filter(|&c| c != ',').collect();
    cleaned.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entiers() {
        assert_eq!(format_integer(42, false, 4, ""), "42");
        assert_eq!(format_integer(-42, false, 4, "D5"), "-00042");
        assert_eq!(format_integer(-1, false, 4, "X"), "FFFFFFFF");
        assert_eq!(format_integer(255, false, 1, "x2"), "ff");
        assert_eq!(format_integer(10, false, 4, "X4"), "000A");
        assert_eq!(format_integer(1234567, false, 4, "N0"), "1,234,567");
        assert_eq!(format_integer(1234, false, 4, "N"), "1,234.00");
        assert_eq!(format_integer(5, false, 4, "00"), "05");
        assert_eq!(format_integer(-1, true, 2, ""), "65535");
        assert_eq!(format_integer(3, false, 4, "P0"), "300 %");
    }

    #[test]
    fn doubles() {
        assert_eq!(format_double(0.1 + 0.2, false, ""), "0.3");
        assert_eq!(format_double(1.5, false, ""), "1.5");
        assert_eq!(format_double(-2.0, false, ""), "-2");
        assert_eq!(format_double(1e20, false, ""), "1E+20");
        assert_eq!(format_double(1e-7, false, ""), "1E-07");
        assert_eq!(format_double(0.0001, false, ""), "0.0001");
        assert_eq!(format_double(2.5, false, "F0"), "3");
        assert_eq!(format_double(3.14159, false, "F2"), "3.14");
        assert_eq!(format_double(1234.5, false, "N1"), "1,234.5");
        assert_eq!(format_double(0.125, false, "P1"), "12.5 %");
        assert_eq!(format_double(1234.5678, false, "E2"), "1.23E+003");
        assert_eq!(format_double(3.14159, false, "0.00"), "3.14");
        assert_eq!(format_double(3.1, false, "0.##"), "3.1");
        assert_eq!(format_double(1234567.0, false, "#,##0"), "1,234,567");
        assert_eq!(format_double(f64::NAN, false, ""), "NaN");
        assert_eq!(format_double(f64::from(0.1f32), true, ""), "0.1");
        assert_eq!(format_double(0.1 + 0.2, false, "R"), "0.30000000000000004");
        assert_eq!(format_double(-0.001, false, "F2"), "0.00");
    }

    #[test]
    fn lecture() {
        assert_eq!(parse_double(" 1.5 "), Some(1.5));
        assert_eq!(parse_double("-2e3"), Some(-2000.0));
        assert_eq!(parse_double("abc"), None);
        assert_eq!(parse_double("Infinity"), Some(f64::INFINITY));
    }
}
