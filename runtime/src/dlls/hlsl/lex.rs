//! Jetons HLSL (et lignes du préprocesseur).

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    Ident(String),
    /// Entier, et vrai s'il porte le suffixe `u`.
    Int(u64, bool),
    Float(f64),
    Str(String),
    Punct(&'static str),
    /// `#pragma pack_matrix(row_major)` : vrai pour row_major.
    PackMatrix(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub t: Tok,
    pub line: u32,
}

const PUNCTS: [&str; 49] = [
    "<<=", ">>=", "...", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
    "<=", ">=", "==", "!=", "&&", "||", "::", "##", "+", "-", "*", "/", "%", "=", "<", ">", "!",
    "~", "&", "|", "^", "?", ":", ";", ",", ".", "(", ")", "[", "]", "{", "}", "#", "@",
];

/// Retire les commentaires en gardant les fins de ligne (numéros de ligne justes).
pub(crate) fn strip_comments(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < b.len() && b[i] != '"' && b[i] != '\n' {
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(b[i]);
                    i += 1;
                }
                out.push(b[i]);
                i += 1;
            }
            if i < b.len() && b[i] == '"' {
                out.push('"');
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            out.push(' ');
            while i < b.len() && !(b[i] == '*' && i + 1 < b.len() && b[i + 1] == '/') {
                if b[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Découpe une ligne logique en jetons.
pub(crate) fn tokenize(s: &str, line: u32) -> Result<Vec<Token>, String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push(Token {
                t: Tok::Ident(b[st..i].iter().collect()),
                line,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let (t, n) = number(&b[i..]).ok_or_else(|| format!("({line}) : nombre invalide"))?;
            out.push(Token { t, line });
            i += n;
            continue;
        }
        if c == '"' {
            let mut v = String::new();
            i += 1;
            while i < b.len() && b[i] != '"' {
                v.push(b[i]);
                i += 1;
            }
            i += 1;
            out.push(Token {
                t: Tok::Str(v),
                line,
            });
            continue;
        }
        let rest: String = b[i..(i + 3).min(b.len())].iter().collect();
        if let Some(p) = PUNCTS.iter().find(|p| rest.starts_with(**p)) {
            out.push(Token {
                t: Tok::Punct(p),
                line,
            });
            i += p.len();
            continue;
        }
        if c == '\\' || c == '\'' || c == '$' || c == '`' {
            i += 1;
            continue;
        }
        return Err(format!("({line}) : caractère inattendu « {c} »"));
    }
    Ok(out)
}

fn number(b: &[char]) -> Option<(Tok, usize)> {
    let mut i = 0;
    if b.len() > 1 && b[0] == '0' && (b[1] == 'x' || b[1] == 'X') {
        i = 2;
        let st = i;
        while i < b.len() && b[i].is_ascii_hexdigit() {
            i += 1;
        }
        let s: String = b[st..i].iter().collect();
        let v = u64::from_str_radix(&s, 16).ok()?;
        let mut u = false;
        while i < b.len() && matches!(b[i], 'u' | 'U' | 'l' | 'L') {
            u |= matches!(b[i], 'u' | 'U');
            i += 1;
        }
        return Some((Tok::Int(v, u), i));
    }
    let mut float = false;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == '.' {
        float = true;
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && (b[i] == 'e' || b[i] == 'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == '+' || b[j] == '-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            float = true;
            i = j;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    let text: String = b[..i].iter().collect();
    if i < b.len() && matches!(b[i], 'f' | 'F' | 'h' | 'H' | 'l' | 'L') && float {
        i += 1;
    } else if i < b.len() && matches!(b[i], 'f' | 'F') {
        // « 1f » : flottant sans point.
        i += 1;
        float = true;
    }
    if float {
        return Some((Tok::Float(text.parse().ok()?), i));
    }
    // Octal en tête de zéro (« 010 ») : rare, lu en décimal comme fxc le signale.
    let v: u64 = text.parse().ok()?;
    let mut u = false;
    while i < b.len() && matches!(b[i], 'u' | 'U' | 'l' | 'L') {
        u |= matches!(b[i], 'u' | 'U');
        i += 1;
    }
    Some((Tok::Int(v, u), i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_punctuation() {
        let t = tokenize("x += 1.5f * 0x10u - .5 >>= 2u", 1).unwrap();
        let k: Vec<Tok> = t.into_iter().map(|t| t.t).collect();
        assert_eq!(
            k,
            vec![
                Tok::Ident("x".into()),
                Tok::Punct("+="),
                Tok::Float(1.5),
                Tok::Punct("*"),
                Tok::Int(16, true),
                Tok::Punct("-"),
                Tok::Float(0.5),
                Tok::Punct(">>="),
                Tok::Int(2, true),
            ]
        );
        assert_eq!(strip_comments("a // b\nc /* d\n e */ f"), "a \nc  \n f");
    }
}
