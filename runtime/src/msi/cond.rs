//! Conditions et chaînes formatées de Windows Installer
//! (`VersionNT64 AND NOT Installed`, `[INSTALLDIR]app.exe`, `[#fichier]`…).

use std::collections::HashMap;

#[derive(Default)]
pub struct Context {
    /// Propriétés (noms sensibles à la casse, comme Windows Installer).
    pub props: HashMap<String, String>,
    /// Clé de fichier → chemin Windows complet.
    pub files: HashMap<String, String>,
    /// Clé de composant → dossier Windows (terminé par `\`).
    pub components: HashMap<String, String>,
}

impl Context {
    pub fn get(&self, name: &str) -> String {
        self.props.get(name).cloned().unwrap_or_default()
    }

    /// Développe une chaîne formatée.
    pub fn format(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '[' {
                // Crochet correspondant (les crochets s'imbriquent).
                let mut depth = 0;
                let mut j = i;
                while j < chars.len() {
                    if chars[j] == '\\' && depth == 1 {
                        j += 2;
                        continue;
                    }
                    match chars[j] {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    out.extend(&chars[i..]);
                    break;
                }
                let inner: String = chars[i + 1..j].iter().collect();
                let inner = if inner.contains('[') && !inner.starts_with('\\') {
                    self.format(&inner)
                } else {
                    inner
                };
                out.push_str(&self.resolve(&inner));
                i = j + 1;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    fn resolve(&self, token: &str) -> String {
        if token == "~" {
            return "\0".into();
        }
        if let Some(rest) = token.strip_prefix('\\') {
            return rest.chars().next().map(String::from).unwrap_or_default();
        }
        if let Some(key) = token.strip_prefix('#').or_else(|| token.strip_prefix('!')) {
            return self.files.get(key).cloned().unwrap_or_default();
        }
        if let Some(key) = token.strip_prefix('$') {
            return self.components.get(key).cloned().unwrap_or_default();
        }
        if let Some(var) = token.strip_prefix('%') {
            return std::env::var(var).unwrap_or_default();
        }
        if token.is_empty() {
            return String::new();
        }
        self.get(token)
    }

    /// Évalue une condition ; une condition vide est vraie, une condition
    /// illisible aussi (on préfère installer que bloquer).
    pub fn eval(&self, cond: &str) -> bool {
        let tokens = match lex(cond) {
            Some(t) => t,
            None => return true,
        };
        if tokens.is_empty() {
            return true;
        }
        let mut p = Parser {
            t: &tokens,
            i: 0,
            ctx: self,
        };
        match p.expr() {
            Some(v) if p.i == tokens.len() => v.truthy(),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(i64),
    Op(String),
    Open,
    Close,
}

fn lex(s: &str) -> Option<Vec<Tok>> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '(' {
            out.push(Tok::Open);
            i += 1;
        } else if ch == ')' {
            out.push(Tok::Close);
            i += 1;
        } else if ch == '"' {
            let start = i + 1;
            i = start;
            while i < c.len() && c[i] != '"' {
                i += 1;
            }
            if i >= c.len() {
                return None;
            }
            out.push(Tok::Str(c[start..i].iter().collect()));
            i += 1;
        } else if ch.is_ascii_digit()
            || (ch == '-' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            let text: String = c[start..i].iter().collect();
            out.push(Tok::Num(text.parse().ok()?));
        } else if "<>=~".contains(ch) {
            let start = i;
            while i < c.len() && "<>=~".contains(c[i]) {
                i += 1;
            }
            out.push(Tok::Op(c[start..i].iter().collect()));
        } else if ch.is_alphanumeric() || "_.$&!?%#".contains(ch) {
            let start = i;
            i += 1;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            match word.to_ascii_uppercase().as_str() {
                "NOT" | "AND" | "OR" | "XOR" | "EQV" | "IMP" => {
                    out.push(Tok::Op(word.to_ascii_uppercase()))
                }
                _ => out.push(Tok::Ident(word)),
            }
        } else {
            return None;
        }
    }
    Some(out)
}

#[derive(Clone, Debug)]
enum Val {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl Val {
    fn truthy(&self) -> bool {
        match self {
            Val::Str(s) => !s.is_empty(),
            Val::Int(n) => *n != 0,
            Val::Bool(b) => *b,
        }
    }

    fn as_text(&self) -> String {
        match self {
            Val::Str(s) => s.clone(),
            Val::Int(n) => n.to_string(),
            Val::Bool(b) => (*b as i32).to_string(),
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            Val::Int(n) => Some(*n),
            Val::Str(s) => s.trim().parse().ok(),
            Val::Bool(b) => Some(*b as i64),
        }
    }
}

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
    ctx: &'a Context,
}

impl Parser<'_> {
    fn peek_op(&self) -> Option<&str> {
        match self.t.get(self.i) {
            Some(Tok::Op(o)) => Some(o.as_str()),
            _ => None,
        }
    }

    fn expr(&mut self) -> Option<Val> {
        let mut left = self.and()?;
        while let Some(op) = self.peek_op() {
            let op = op.to_string();
            if !matches!(op.as_str(), "OR" | "XOR" | "EQV" | "IMP") {
                break;
            }
            self.i += 1;
            let right = self.and()?;
            let (a, b) = (left.truthy(), right.truthy());
            left = Val::Bool(match op.as_str() {
                "OR" => a || b,
                "XOR" => a != b,
                "EQV" => a == b,
                _ => !a || b,
            });
        }
        Some(left)
    }

    fn and(&mut self) -> Option<Val> {
        let mut left = self.not()?;
        while self.peek_op() == Some("AND") {
            self.i += 1;
            let right = self.not()?;
            left = Val::Bool(left.truthy() && right.truthy());
        }
        Some(left)
    }

    fn not(&mut self) -> Option<Val> {
        if self.peek_op() == Some("NOT") {
            self.i += 1;
            return Some(Val::Bool(!self.not()?.truthy()));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Option<Val> {
        let left = self.term()?;
        let Some(op) = self.peek_op().map(str::to_string) else {
            return Some(left);
        };
        if matches!(op.as_str(), "AND" | "OR" | "XOR" | "EQV" | "IMP" | "NOT") {
            return Some(left);
        }
        self.i += 1;
        let right = self.term()?;
        let (fold, op) = match op.strip_prefix('~') {
            Some(o) => (true, o.to_string()),
            None => (false, op),
        };
        if let (Some(a), Some(b), false) = (
            left.as_int(),
            right.as_int(),
            op == "><" || op == "<<" || op == ">>",
        ) {
            if matches!(left, Val::Int(_)) || matches!(right, Val::Int(_)) {
                return Some(Val::Bool(match op.as_str() {
                    "=" => a == b,
                    "<>" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return None,
                }));
            }
        }
        let (mut a, mut b) = (left.as_text(), right.as_text());
        if fold {
            a = a.to_lowercase();
            b = b.to_lowercase();
        }
        Some(Val::Bool(match op.as_str() {
            "=" => a == b,
            "<>" => a != b,
            "<" => a < b,
            ">" => a > b,
            "<=" => a <= b,
            ">=" => a >= b,
            "><" => a.contains(&b),
            "<<" => a.starts_with(&b),
            ">>" => a.ends_with(&b),
            _ => return None,
        }))
    }

    fn term(&mut self) -> Option<Val> {
        let tok = self.t.get(self.i)?.clone();
        self.i += 1;
        match tok {
            Tok::Open => {
                let v = self.expr()?;
                if self.t.get(self.i) != Some(&Tok::Close) {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            Tok::Str(s) => Some(Val::Str(s)),
            Tok::Num(n) => Some(Val::Int(n)),
            Tok::Ident(name) => Some(match name.chars().next() {
                // Pendant une installation, composants et fonctionnalités
                // sont « installés en local » (3) ; l'état actuel est absent (2).
                Some('$') | Some('&') => Val::Int(3),
                Some('?') | Some('!') => Val::Int(2),
                Some('%') => Val::Str(std::env::var(&name[1..]).unwrap_or_default()),
                _ => Val::Str(self.ctx.get(&name)),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let mut c = Context::default();
        c.props.insert("VersionNT64".into(), "603".into());
        c.props
            .insert("INSTALLDIR".into(), "C:\\Program Files\\App\\".into());
        c.props.insert("Name".into(), "INSTALLDIR".into());
        c.files
            .insert("app.exe".into(), "C:\\Program Files\\App\\app.exe".into());
        c
    }

    #[test]
    fn conditions() {
        let c = ctx();
        assert!(c.eval(""));
        assert!(c.eval("VersionNT64"));
        assert!(c.eval("NOT Installed"));
        assert!(c.eval("VersionNT64 >= 601 AND NOT Installed"));
        assert!(!c.eval("VersionNT64 < 600"));
        assert!(c.eval("(Installed OR VersionNT64) AND $Comp = 3"));
        assert!(c.eval("INSTALLDIR >< \"App\""));
        assert!(c.eval("Name ~= \"installdir\""));
        assert!(!c.eval("REMOVE=\"ALL\""));
    }

    #[test]
    fn chaines_formatees() {
        let c = ctx();
        assert_eq!(
            c.format("[INSTALLDIR]app.exe"),
            "C:\\Program Files\\App\\app.exe"
        );
        assert_eq!(
            c.format("\"[#app.exe]\" --x"),
            "\"C:\\Program Files\\App\\app.exe\" --x"
        );
        assert_eq!(c.format("[[Name]]"), "C:\\Program Files\\App\\");
        assert_eq!(c.format("a[~]b[\\[]"), "a\0b[");
        assert_eq!(c.format("[Inconnu]x"), "x");
    }
}
