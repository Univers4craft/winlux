//! D3DX9 : compilateur HLSL (#834).
//!
//! Écrit d'après la référence publique du langage HLSL de Direct3D 9 :
//! préprocesseur, analyse, puis génération d'assembleur vs/ps 2.0 ou 3.0
//! confié à `d3dx9_asm`. Comme le font les compilateurs pour ces profils,
//! tout est mis en ligne : fonctions inlinées, boucles déroulées, `if`
//! aplatis par sélection. Les registres temporaires sont attribués à la fin
//! par balayage linéaire (le code produit est rectiligne).
//!
//! Le même analyseur sert aux fichiers d'effets (.fx) : techniques, passes,
//! `sampler_state`, `compile vs_2_0 f()` et blocs `asm { }`.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════
// Préprocesseur
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct Macro {
    params: Option<Vec<String>>,
    body: String,
}

/// Retire les commentaires en gardant les fins de ligne.
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            out.push_str(&src[start..i]);
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                if b[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
        } else {
            let ch = src[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

struct Pre<'a> {
    macros: HashMap<String, Macro>,
    include: &'a dyn Fn(&str) -> Option<String>,
    out: Vec<String>,
}

impl Pre<'_> {
    /// Développe les macros d'une ligne.
    fn expand(&self, line: &str, depth: usize, hide: &[String]) -> String {
        if depth > 32 {
            return line.to_string();
        }
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                let s = i;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                i = (i + 1).min(chars.len());
                out.extend(&chars[s..i]);
                continue;
            }
            if c.is_ascii_digit() {
                while i < chars.len() && (is_ident(chars[i]) || chars[i] == '.') {
                    out.push(chars[i]);
                    i += 1;
                }
                continue;
            }
            if !is_ident_start(c) {
                out.push(c);
                i += 1;
                continue;
            }
            let s = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let name: String = chars[s..i].iter().collect();
            let Some(m) = self.macros.get(&name).filter(|_| !hide.contains(&name)) else {
                out.push_str(&name);
                continue;
            };
            let mut hide2 = hide.to_vec();
            hide2.push(name.clone());
            match &m.params {
                None => out.push_str(&self.expand(&m.body, depth + 1, &hide2)),
                Some(params) => {
                    let mut j = i;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if chars.get(j) != Some(&'(') {
                        out.push_str(&name);
                        continue;
                    }
                    // Arguments séparés par des virgules hors parenthèses.
                    let mut args = vec![String::new()];
                    let mut lvl = 0;
                    j += 1;
                    while j < chars.len() {
                        let ch = chars[j];
                        if ch == '(' {
                            lvl += 1;
                        } else if ch == ')' {
                            if lvl == 0 {
                                break;
                            }
                            lvl -= 1;
                        } else if ch == ',' && lvl == 0 {
                            args.push(String::new());
                            j += 1;
                            continue;
                        }
                        if let Some(a) = args.last_mut() {
                            a.push(ch);
                        }
                        j += 1;
                    }
                    i = (j + 1).min(chars.len());
                    let args: Vec<String> = args
                        .iter()
                        .map(|a| self.expand(a.trim(), depth + 1, hide))
                        .collect();
                    let body = subst(&m.body, params, &args);
                    out.push_str(&self.expand(&body, depth + 1, &hide2));
                }
            }
        }
        out
    }

    fn define(&mut self, rest: &str) {
        let rest = rest.trim_start();
        let name: String = rest.chars().take_while(|&c| is_ident(c)).collect();
        let after = &rest[name.len()..];
        if let Some(a) = after.strip_prefix('(') {
            let close = a.find(')').unwrap_or(a.len());
            let params = a[..close]
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let body = a.get(close + 1..).unwrap_or("").trim().to_string();
            self.macros.insert(
                name,
                Macro {
                    params: Some(params),
                    body,
                },
            );
        } else {
            self.macros.insert(
                name,
                Macro {
                    params: None,
                    body: after.trim().to_string(),
                },
            );
        }
    }

    fn eval_if(&self, expr: &str) -> Result<bool, String> {
        // defined(X) / defined X d'abord, puis les macros.
        let mut s = String::new();
        let chars: Vec<char> = expr.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if is_ident_start(chars[i]) {
                let st = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                let w: String = chars[st..i].iter().collect();
                if w == "defined" {
                    while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '(') {
                        i += 1;
                    }
                    let st = i;
                    while i < chars.len() && is_ident(chars[i]) {
                        i += 1;
                    }
                    let n: String = chars[st..i].iter().collect();
                    while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ')') {
                        i += 1;
                    }
                    s.push_str(if self.macros.contains_key(&n) {
                        " 1 "
                    } else {
                        " 0 "
                    });
                } else {
                    s.push_str(&w);
                }
            } else {
                s.push(chars[i]);
                i += 1;
            }
        }
        let s = self.expand(&s, 0, &[]);
        let toks = lex_line(&s, 0)?;
        let mut p = IfEval { t: &toks, i: 0 };
        Ok(p.or()? != 0.0)
    }

    fn run(&mut self, src: &str, level: usize) -> Result<(), String> {
        if level > 16 {
            return Err("#include trop imbriqués".into());
        }
        let text = strip_comments(&src.replace("\r\n", "\n").replace('\r', "\n"));
        // (actif, une branche a déjà été prise)
        let mut stack: Vec<(bool, bool)> = Vec::new();
        let lines: Vec<&str> = text.split('\n').collect();
        let mut n = 0;
        while n < lines.len() {
            let mut line = lines[n].to_string();
            let mut extra = 0;
            while line.ends_with('\\') && n + extra + 1 < lines.len() {
                line.pop();
                extra += 1;
                line.push_str(lines[n + extra]);
            }
            n += 1 + extra;
            let active = stack.iter().all(|s| s.0);
            let t = line.trim_start();
            if let Some(d) = t.strip_prefix('#') {
                let d = d.trim_start();
                let word: String = d.chars().take_while(|&c| is_ident(c)).collect();
                let rest = d[word.len()..].trim();
                match word.as_str() {
                    "ifdef" | "ifndef" => {
                        let def = self
                            .macros
                            .contains_key(rest.split_whitespace().next().unwrap_or(""));
                        let v = active && (def == (word == "ifdef"));
                        stack.push((v, v || !active));
                    }
                    "if" => {
                        let v = active && self.eval_if(rest)?;
                        stack.push((v, v || !active));
                    }
                    "elif" => {
                        let Some(top) = stack.last().copied() else {
                            return Err("#elif sans #if".into());
                        };
                        stack.pop();
                        let parent = stack.iter().all(|s| s.0);
                        let v = parent && !top.1 && self.eval_if(rest)?;
                        stack.push((v, top.1 || v));
                    }
                    "else" => {
                        let Some(top) = stack.pop() else {
                            return Err("#else sans #if".into());
                        };
                        let parent = stack.iter().all(|s| s.0);
                        stack.push((parent && !top.1, true));
                    }
                    "endif" => {
                        if stack.pop().is_none() {
                            return Err("#endif sans #if".into());
                        }
                    }
                    _ if !active => {}
                    "define" => self.define(rest),
                    "undef" => {
                        self.macros.remove(rest.trim());
                    }
                    "include" => {
                        let name = rest.trim_matches(|c| c == '"' || c == '<' || c == '>');
                        let inc = (self.include)(name)
                            .ok_or_else(|| format!("#include introuvable : {name}"))?;
                        self.run(&inc, level + 1)?;
                    }
                    "error" => return Err(format!("#error {rest}")),
                    _ => {} // #pragma, #line…
                }
                self.out.push(String::new());
                for _ in 0..extra {
                    self.out.push(String::new());
                }
                continue;
            }
            if active {
                self.out.push(self.expand(&line, 0, &[]));
            } else {
                self.out.push(String::new());
            }
            for _ in 0..extra {
                self.out.push(String::new());
            }
        }
        if !stack.is_empty() {
            return Err("#if sans #endif".into());
        }
        Ok(())
    }
}

fn subst(body: &str, params: &[String], args: &[String]) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if is_ident_start(chars[i]) {
            let s = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let w: String = chars[s..i].iter().collect();
            match params.iter().position(|p| *p == w) {
                Some(k) => out.push_str(args.get(k).map(String::as_str).unwrap_or("")),
                None => out.push_str(&w),
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

struct IfEval<'a> {
    t: &'a [Tok],
    i: usize,
}
impl IfEval<'_> {
    fn peek(&self) -> Option<&TokKind> {
        self.t.get(self.i).map(|t| &t.kind)
    }
    fn eat(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Some(TokKind::P(q)) if *q == p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn or(&mut self) -> Result<f64, String> {
        let mut a = self.and()?;
        while self.eat("||") {
            let b = self.and()?;
            a = ((a != 0.0) || (b != 0.0)) as i32 as f64;
        }
        Ok(a)
    }
    fn and(&mut self) -> Result<f64, String> {
        let mut a = self.cmp()?;
        while self.eat("&&") {
            let b = self.cmp()?;
            a = ((a != 0.0) && (b != 0.0)) as i32 as f64;
        }
        Ok(a)
    }
    fn cmp(&mut self) -> Result<f64, String> {
        let mut a = self.add()?;
        loop {
            let op = match self.peek() {
                Some(TokKind::P(p)) if ["==", "!=", "<", ">", "<=", ">="].contains(p) => *p,
                _ => return Ok(a),
            };
            self.i += 1;
            let b = self.add()?;
            a = match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                _ => a >= b,
            } as i32 as f64;
        }
    }
    fn add(&mut self) -> Result<f64, String> {
        let mut a = self.mul()?;
        loop {
            if self.eat("+") {
                a += self.mul()?;
            } else if self.eat("-") {
                a -= self.mul()?;
            } else {
                return Ok(a);
            }
        }
    }
    fn mul(&mut self) -> Result<f64, String> {
        let mut a = self.un()?;
        loop {
            if self.eat("*") {
                a *= self.un()?;
            } else if self.eat("/") {
                let b = self.un()?;
                a = if b == 0.0 { 0.0 } else { (a / b).trunc() };
            } else {
                return Ok(a);
            }
        }
    }
    fn un(&mut self) -> Result<f64, String> {
        if self.eat("!") {
            return Ok((self.un()? == 0.0) as i32 as f64);
        }
        if self.eat("-") {
            return Ok(-self.un()?);
        }
        if self.eat("(") {
            let v = self.or()?;
            self.eat(")");
            return Ok(v);
        }
        let v = match self.peek() {
            Some(TokKind::Num(v, _)) => *v,
            Some(TokKind::Id(_)) => 0.0,
            _ => return Err("expression #if invalide".into()),
        };
        self.i += 1;
        Ok(v)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Lexique
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
enum TokKind {
    Id(String),
    Num(f64, bool),
    Str(String),
    P(&'static str),
}

#[derive(Clone, Debug)]
struct Tok {
    kind: TokKind,
    line: u32,
    col: u32,
    end: u32,
}

const PUNCTS: &[&str] = &[
    "<<=", ">>=", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "==", "!=", "<=",
    ">=", "&&", "||", "<<", ">>", "::", "+", "-", "*", "/", "%", "<", ">", "=", "!", "~", "&", "|",
    "^", "?", ":", ";", ",", ".", "(", ")", "[", "]", "{", "}", "#", "@",
];

fn lex_line(s: &str, line: u32) -> Result<Vec<Tok>, String> {
    let b = s.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let st = i;
        let kind = if is_ident_start(c) {
            while i < b.len() && is_ident(b[i] as char) {
                i += 1;
            }
            TokKind::Id(s[st..i].to_string())
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            if c == '0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                while i < b.len() && (b[i] as char).is_ascii_hexdigit() {
                    i += 1;
                }
                let v = u64::from_str_radix(&s[st + 2..i], 16).unwrap_or(0) as f64;
                while i < b.len() && matches!(b[i], b'u' | b'U' | b'l' | b'L') {
                    i += 1;
                }
                TokKind::Num(v, false)
            } else {
                let mut float = false;
                while i < b.len() {
                    let ch = b[i] as char;
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' {
                        float = true;
                        i += 1;
                    } else if (ch == 'e' || ch == 'E')
                        && (b.get(i + 1).is_some_and(u8::is_ascii_digit)
                            || (matches!(b.get(i + 1), Some(b'+' | b'-'))
                                && b.get(i + 2).is_some_and(u8::is_ascii_digit)))
                    {
                        float = true;
                        i += 2;
                    } else {
                        break;
                    }
                }
                let num: f64 = s[st..i].parse().unwrap_or(0.0);
                while i < b.len()
                    && matches!(b[i], b'f' | b'F' | b'h' | b'H' | b'u' | b'U' | b'l' | b'L')
                {
                    if matches!(b[i], b'f' | b'F' | b'h' | b'H') {
                        float = true;
                    }
                    i += 1;
                }
                TokKind::Num(num, float)
            }
        } else if c == '"' {
            i += 1;
            let mut out = String::new();
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' && i + 1 < b.len() {
                    i += 1;
                    out.push(match b[i] {
                        b'n' => '\n',
                        b't' => '\t',
                        x => x as char,
                    });
                } else {
                    let ch = s[i..].chars().next().unwrap_or(' ');
                    out.push(ch);
                    i += ch.len_utf8() - 1;
                }
                i += 1;
            }
            i += 1;
            TokKind::Str(out)
        } else {
            let p = PUNCTS
                .iter()
                .find(|p| s[i..].starts_with(**p))
                .ok_or_else(|| format!("({}) : caractère inattendu « {c} »", line + 1))?;
            i += p.len();
            TokKind::P(p)
        };
        v.push(Tok {
            kind,
            line,
            col: st as u32,
            end: i.min(b.len()) as u32,
        });
    }
    Ok(v)
}

/// Prétraite puis découpe un source HLSL.
fn lex(
    src: &str,
    defines: &[(String, String)],
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<(Vec<Tok>, Vec<String>), String> {
    let mut pre = Pre {
        macros: HashMap::new(),
        include,
        out: Vec::new(),
    };
    for (k, v) in defines {
        pre.macros.insert(
            k.clone(),
            Macro {
                params: None,
                body: if v.is_empty() { "1".into() } else { v.clone() },
            },
        );
    }
    pre.run(src, 0)?;
    let mut toks = Vec::new();
    for (n, l) in pre.out.iter().enumerate() {
        toks.extend(lex_line(l, n as u32)?);
    }
    Ok((toks, pre.out))
}

// ═══════════════════════════════════════════════════════════════════════
// Arbre syntaxique
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Base {
    Void,
    Bool,
    Int,
    Float,
    Str,
    Texture,
    /// 1 = 1D, 2 = 2D, 3 = 3D, 4 = cube, 0 = générique.
    Sampler(u8),
    /// true pour un pixel shader.
    Shader(bool),
    Struct(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ty {
    pub base: Base,
    /// 0 scalaire, 1 vecteur, 2 matrice, 3 objet, 4 structure.
    pub class: u8,
    pub rows: u8,
    pub cols: u8,
    pub array: Option<u32>,
    pub row_major: Option<bool>,
}

impl Ty {
    fn scalar(base: Base) -> Ty {
        let class = match base {
            Base::Bool | Base::Int | Base::Float | Base::Void => 0,
            Base::Struct(_) => 4,
            _ => 3,
        };
        Ty {
            base,
            class,
            rows: 1,
            cols: 1,
            array: None,
            row_major: None,
        }
    }
    fn vec(base: Base, n: u8) -> Ty {
        Ty {
            class: if n == 1 { 0 } else { 1 },
            cols: n,
            ..Ty::scalar(base)
        }
    }
    fn float(n: u8) -> Ty {
        Ty::vec(Base::Float, n)
    }
    fn elem(&self) -> Ty {
        Ty {
            array: None,
            ..self.clone()
        }
    }
    fn numeric(&self) -> bool {
        matches!(self.base, Base::Bool | Base::Int | Base::Float)
    }
    pub(crate) fn components(&self) -> u32 {
        self.rows as u32 * self.cols as u32
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StateAssign {
    pub name: String,
    pub index: Option<u32>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Num(f64, bool),
    Bool(bool),
    Str(String),
    Ident(String),
    Unary(&'static str, Box<Expr>),
    Post(&'static str, Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Ctor(Ty, Vec<Expr>),
    Cast(Ty, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Compile(String, String, Vec<Expr>),
    Asm(String),
    SamplerState(Vec<StateAssign>),
    Null,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Mods {
    pub stat: bool,
    pub uniform: bool,
    pub konst: bool,
    pub out: bool,
    pub inn: bool,
    pub row_major: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct VarDecl {
    pub name: String,
    pub ty: Ty,
    pub mods: Mods,
    pub semantic: Option<String>,
    pub register: Option<(char, u32)>,
    pub init: Option<Expr>,
    pub annotations: Vec<VarDecl>,
}

#[derive(Clone, Debug)]
pub(crate) enum Stmt {
    Decl(Vec<VarDecl>),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    Do(Box<Stmt>, Expr),
    Block(Vec<Stmt>),
    Return(Option<Expr>),
    Discard,
    Break,
    Continue,
    Empty,
}

#[derive(Clone, Debug)]
pub(crate) struct Param {
    pub name: String,
    pub ty: Ty,
    pub mods: Mods,
    pub semantic: Option<String>,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub name: String,
    pub ret: Ty,
    pub params: Vec<Param>,
    pub semantic: Option<String>,
    pub body: Option<Vec<Stmt>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub name: String,
    pub ty: Ty,
    pub semantic: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Pass {
    pub name: String,
    pub annotations: Vec<VarDecl>,
    pub states: Vec<StateAssign>,
}

#[derive(Clone, Debug)]
pub(crate) struct Technique {
    pub name: String,
    pub annotations: Vec<VarDecl>,
    pub passes: Vec<Pass>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub structs: HashMap<String, Vec<Field>>,
    pub globals: Vec<VarDecl>,
    pub funcs: Vec<Function>,
    pub techniques: Vec<Technique>,
}

// ═══════════════════════════════════════════════════════════════════════
// Analyse
// ═══════════════════════════════════════════════════════════════════════

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
    lines: &'a [String],
    structs: HashMap<String, Vec<Field>>,
    typedefs: HashMap<String, Ty>,
}

fn scalar_base(s: &str) -> Option<Base> {
    Some(match s {
        "float" | "half" | "double" | "min16float" | "min10float" | "FLOAT" => Base::Float,
        "int" | "uint" | "dword" | "DWORD" | "min16int" | "min12int" | "min16uint" | "INT" => {
            Base::Int
        }
        "bool" | "BOOL" => Base::Bool,
        _ => return None,
    })
}

/// « float4x3 » → (Float, 4, 3), « int2 » → (Int, 1, 2).
fn builtin_type(s: &str) -> Option<Ty> {
    let low = s.to_ascii_lowercase();
    match low.as_str() {
        "void" => return Some(Ty::scalar(Base::Void)),
        "string" => return Some(Ty::scalar(Base::Str)),
        "vector" => return Some(Ty::float(4)),
        "matrix" => {
            return Some(Ty {
                class: 2,
                rows: 4,
                cols: 4,
                ..Ty::scalar(Base::Float)
            })
        }
        "texture" | "texture1d" | "texture2d" | "texture3d" | "texturecube" => {
            return Some(Ty::scalar(Base::Texture))
        }
        "sampler" | "sampler_state" => return Some(Ty::scalar(Base::Sampler(0))),
        "sampler1d" => return Some(Ty::scalar(Base::Sampler(1))),
        "sampler2d" => return Some(Ty::scalar(Base::Sampler(2))),
        "sampler3d" => return Some(Ty::scalar(Base::Sampler(3))),
        "samplercube" => return Some(Ty::scalar(Base::Sampler(4))),
        "vertexshader" => return Some(Ty::scalar(Base::Shader(false))),
        "pixelshader" => return Some(Ty::scalar(Base::Shader(true))),
        _ => {}
    }
    let digits = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let base = scalar_base(&s[..digits])?;
    let rest = &s[digits..];
    if rest.is_empty() {
        return Some(Ty::scalar(base));
    }
    let b = rest.as_bytes();
    let d = |c: u8| (b'1'..=b'4').contains(&c).then_some(c - b'0');
    match b.len() {
        1 => Some(Ty::vec(base, d(b[0])?)),
        3 if b[1] == b'x' => Some(Ty {
            class: 2,
            rows: d(b[0])?,
            cols: d(b[2])?,
            ..Ty::scalar(base)
        }),
        _ => None,
    }
}

const BIN_PREC: &[(&str, u8)] = &[
    ("||", 1),
    ("&&", 2),
    ("|", 3),
    ("^", 4),
    ("&", 5),
    ("==", 6),
    ("!=", 6),
    ("<", 7),
    (">", 7),
    ("<=", 7),
    (">=", 7),
    ("<<", 8),
    (">>", 8),
    ("+", 9),
    ("-", 9),
    ("*", 10),
    ("/", 10),
    ("%", 10),
];

impl Parser<'_> {
    fn err<T>(&self, m: &str) -> Result<T, String> {
        let line = self
            .t
            .get(self.i)
            .or(self.t.last())
            .map(|t| t.line + 1)
            .unwrap_or(0);
        let near = match self.t.get(self.i).map(|t| &t.kind) {
            Some(TokKind::Id(s)) => format!(" près de « {s} »"),
            Some(TokKind::P(p)) => format!(" près de « {p} »"),
            _ => String::new(),
        };
        Err(format!("({line}) : {m}{near}"))
    }
    fn peek(&self) -> Option<&TokKind> {
        self.t.get(self.i).map(|t| &t.kind)
    }
    fn peek_at(&self, k: usize) -> Option<&TokKind> {
        self.t.get(self.i + k).map(|t| &t.kind)
    }
    fn is_p(&self, p: &str) -> bool {
        matches!(self.peek(), Some(TokKind::P(q)) if *q == p)
    }
    fn is_id(&self, s: &str) -> bool {
        matches!(self.peek(), Some(TokKind::Id(q)) if q == s)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is_p(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> Result<(), String> {
        if self.eat(p) {
            Ok(())
        } else {
            self.err(&format!("« {p} » attendu"))
        }
    }
    fn ident(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(TokKind::Id(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => self.err("identifiant attendu"),
        }
    }
    fn number(&mut self) -> Result<f64, String> {
        let neg = self.eat("-");
        match self.peek() {
            Some(TokKind::Num(v, _)) => {
                let v = *v;
                self.i += 1;
                Ok(if neg { -v } else { v })
            }
            _ => self.err("nombre attendu"),
        }
    }

    fn type_at(&self, k: usize) -> Option<Ty> {
        match self.peek_at(k) {
            Some(TokKind::Id(s)) => builtin_type(s)
                .or_else(|| {
                    self.structs
                        .contains_key(s)
                        .then(|| Ty::scalar(Base::Struct(s.clone())))
                })
                .or_else(|| self.typedefs.get(s).cloned()),
            _ => None,
        }
    }

    /// Type (sans modificateurs) : float4, vector<float,3>, matrix<float,4,4>, struct.
    fn ty(&mut self) -> Result<Ty, String> {
        if self.is_id("struct") {
            self.i += 1;
            let name = self.ident()?;
            if self.is_p("{") {
                self.struct_body(name.clone())?;
            }
            return Ok(Ty::scalar(Base::Struct(name)));
        }
        let Some(mut t) = self.type_at(0) else {
            return self.err("type attendu");
        };
        let word = self.ident()?;
        if (word == "vector" || word == "matrix") && self.eat("<") {
            let b = self.ident()?;
            let base = scalar_base(&b).unwrap_or(Base::Float);
            self.expect(",")?;
            let r = self.number()? as u8;
            if word == "vector" {
                t = Ty::vec(base, r.clamp(1, 4));
            } else {
                self.expect(",")?;
                let c = self.number()? as u8;
                t = Ty {
                    class: 2,
                    rows: r.clamp(1, 4),
                    cols: c.clamp(1, 4),
                    ..Ty::scalar(base)
                };
            }
            self.expect(">")?;
        } else if word.eq_ignore_ascii_case("texture") && self.is_p("<") {
            // texture2D<float4> (rare en D3D9)
            while !self.eat(">") && self.peek().is_some() {
                self.i += 1;
            }
        }
        Ok(t)
    }

    fn mods(&mut self) -> Mods {
        let mut m = Mods::default();
        loop {
            match self.peek() {
                Some(TokKind::Id(s)) => match s.as_str() {
                    "static" => m.stat = true,
                    "uniform" => m.uniform = true,
                    "const" => m.konst = true,
                    "in" => m.inn = true,
                    "out" => m.out = true,
                    "inout" => {
                        m.inn = true;
                        m.out = true
                    }
                    "row_major" => m.row_major = Some(true),
                    "column_major" => m.row_major = Some(false),
                    "extern" | "shared" | "volatile" | "inline" | "precise" | "linear"
                    | "centroid" | "nointerpolation" | "noperspective" | "sample" | "snorm"
                    | "unorm" => {}
                    _ => return m,
                },
                Some(TokKind::P("[")) => {
                    // Attributs [unroll], [loop], [branch]…
                    let mut lvl = 0;
                    loop {
                        if self.eat("[") {
                            lvl += 1;
                        } else if self.eat("]") {
                            lvl -= 1;
                            if lvl == 0 {
                                break;
                            }
                        } else if self.peek().is_none() {
                            break;
                        } else {
                            self.i += 1;
                        }
                    }
                    continue;
                }
                _ => return m,
            }
            self.i += 1;
        }
    }

    fn struct_body(&mut self, name: String) -> Result<(), String> {
        self.expect("{")?;
        let mut fields = Vec::new();
        while !self.eat("}") {
            let m = self.mods();
            let mut ty = self.ty()?;
            if m.row_major.is_some() {
                ty.row_major = m.row_major;
            }
            loop {
                let fname = self.ident()?;
                let mut fty = ty.clone();
                if self.eat("[") {
                    fty.array = Some(self.number()? as u32);
                    self.expect("]")?;
                }
                let mut semantic = None;
                if self.eat(":") {
                    semantic = Some(self.ident()?);
                }
                fields.push(Field {
                    name: fname,
                    ty: fty,
                    semantic,
                });
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.structs.insert(name, fields);
        Ok(())
    }

    fn annotations(&mut self) -> Result<Vec<VarDecl>, String> {
        let mut v = Vec::new();
        if !self.eat("<") {
            return Ok(v);
        }
        while !self.eat(">") {
            if self.peek().is_none() {
                return self.err("annotation non fermée");
            }
            let m = self.mods();
            let ty = self.ty()?;
            let name = self.ident()?;
            let mut ty2 = ty;
            if self.eat("[") {
                ty2.array = Some(self.number()? as u32);
                self.expect("]")?;
            }
            self.expect("=")?;
            let init = if self.is_p("{") {
                self.init_list()?
            } else {
                self.unary()?
            };
            self.expect(";")?;
            v.push(VarDecl {
                name,
                ty: ty2,
                mods: m,
                semantic: None,
                register: None,
                init: Some(init),
                annotations: Vec::new(),
            });
        }
        Ok(v)
    }

    fn init_list(&mut self) -> Result<Expr, String> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.eat("}") {
            v.push(if self.is_p("{") {
                self.init_list()?
            } else {
                self.assign()?
            });
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
        }
        Ok(Expr::List(v))
    }

    fn state_list(&mut self) -> Result<Vec<StateAssign>, String> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.eat("}") {
            if self.peek().is_none() {
                return self.err("bloc d'états non fermé");
            }
            let name = self.ident()?;
            let mut index = None;
            if self.eat("[") {
                index = Some(self.number()? as u32);
                self.expect("]")?;
            }
            self.expect("=")?;
            let value = self.state_value()?;
            self.expect(";")?;
            v.push(StateAssign { name, index, value });
        }
        Ok(v)
    }

    /// Valeur d'état d'effet : compile, asm, <param>, NULL ou expression.
    fn state_value(&mut self) -> Result<Expr, String> {
        if self.is_id("compile") {
            self.i += 1;
            let profile = self.ident()?;
            let func = self.ident()?;
            let mut args = Vec::new();
            self.expect("(")?;
            while !self.eat(")") {
                args.push(self.assign()?);
                if !self.eat(",") {
                    self.expect(")")?;
                    break;
                }
            }
            return Ok(Expr::Compile(profile, func, args));
        }
        if self.is_id("asm") {
            self.i += 1;
            return self.asm_block();
        }
        if self.is_id("NULL") || self.is_id("null") {
            self.i += 1;
            return Ok(Expr::Null);
        }
        if self.is_id("sampler_state") {
            self.i += 1;
            return Ok(Expr::SamplerState(self.state_list()?));
        }
        if self.eat("<") {
            let n = self.ident()?;
            self.expect(">")?;
            return Ok(Expr::Ident(n));
        }
        if self.is_p("{") {
            return self.init_list();
        }
        self.assign()
    }

    fn asm_block(&mut self) -> Result<Expr, String> {
        let open = self.i;
        self.expect("{")?;
        let mut lvl = 1;
        let start = self.i;
        while lvl > 0 {
            match self.peek() {
                None => return self.err("bloc asm non fermé"),
                Some(TokKind::P("{")) => lvl += 1,
                Some(TokKind::P("}")) => lvl -= 1,
                _ => {}
            }
            self.i += 1;
        }
        let end = self.i - 1;
        let _ = open;
        // Texte brut, ligne par ligne, d'après les positions des jetons.
        let mut text = String::new();
        let mut k = start;
        while k < end {
            let line = self.t[k].line;
            let first = self.t[k].col as usize;
            let mut last = self.t[k].end as usize;
            while k < end && self.t[k].line == line {
                last = self.t[k].end as usize;
                k += 1;
            }
            let l = &self.lines[line as usize];
            text.push_str(l.get(first..last).unwrap_or(""));
            text.push('\n');
        }
        Ok(Expr::Asm(text))
    }

    fn semantic_or_register(
        &mut self,
        semantic: &mut Option<String>,
        register: &mut Option<(char, u32)>,
    ) -> Result<(), String> {
        while self.eat(":") {
            if self.is_id("register") {
                self.i += 1;
                self.expect("(")?;
                // register(vs_2_0, c4) ou register(c4)
                let mut r = self.ident()?;
                if self.eat(",") {
                    r = self.ident()?;
                }
                self.expect(")")?;
                let c = r.chars().next().unwrap_or('c').to_ascii_lowercase();
                let n = r[1..].parse().unwrap_or(0);
                *register = Some((c, n));
            } else if self.is_id("packoffset") {
                while !self.eat(")") && self.peek().is_some() {
                    self.i += 1;
                }
            } else {
                *semantic = Some(self.ident()?);
            }
        }
        Ok(())
    }

    /// Déclarations après le type : a = 1, b[4] : register(c0), ...
    fn declarators(&mut self, ty: &Ty, mods: &Mods) -> Result<Vec<VarDecl>, String> {
        let mut v = Vec::new();
        loop {
            let name = self.ident()?;
            let mut t = ty.clone();
            if mods.row_major.is_some() {
                t.row_major = mods.row_major;
            }
            if self.eat("[") {
                t.array = Some(if self.is_p("]") {
                    0
                } else {
                    self.number()? as u32
                });
                self.expect("]")?;
            }
            let (mut semantic, mut register) = (None, None);
            self.semantic_or_register(&mut semantic, &mut register)?;
            let annotations = self.annotations()?;
            let init = if self.eat("=") {
                Some(if self.is_p("{") {
                    self.init_list()?
                } else if self.is_id("sampler_state") {
                    self.i += 1;
                    Expr::SamplerState(self.state_list()?)
                } else if self.is_id("compile") || self.is_id("asm") {
                    self.state_value()?
                } else {
                    self.assign()?
                })
            } else if self.is_p("{") && matches!(t.base, Base::Sampler(_)) {
                // sampler s { ... } (forme ancienne)
                Some(Expr::SamplerState(self.state_list()?))
            } else {
                None
            };
            if let (Some(Expr::List(l)), Some(0)) = (&init, t.array) {
                t.array = Some(l.len() as u32);
            }
            v.push(VarDecl {
                name,
                ty: t,
                mods: mods.clone(),
                semantic,
                register,
                init,
                annotations,
            });
            if !self.eat(",") {
                return Ok(v);
            }
        }
    }

    fn program(&mut self) -> Result<Program, String> {
        let mut p = Program::default();
        while self.peek().is_some() {
            if self.eat(";") {
                continue;
            }
            if self.is_id("technique") || self.is_id("technique9") || self.is_id("Technique") {
                self.i += 1;
                p.techniques.push(self.technique()?);
                continue;
            }
            if self.is_id("technique10") || self.is_id("technique11") {
                return self.err("technique10 n'existe pas en Direct3D 9");
            }
            if self.is_id("typedef") {
                self.i += 1;
                let _ = self.mods();
                let t = self.ty()?;
                let n = self.ident()?;
                self.expect(";")?;
                self.typedefs.insert(n, t);
                continue;
            }
            if self.is_id("struct") && matches!(self.peek_at(2), Some(TokKind::P("{"))) {
                self.i += 1;
                let name = self.ident()?;
                self.struct_body(name.clone())?;
                if self.eat(";") {
                    continue;
                }
                let mods = Mods::default();
                let decls = self.declarators(&Ty::scalar(Base::Struct(name)), &mods)?;
                self.expect(";")?;
                p.globals.extend(decls);
                continue;
            }
            let mods = self.mods();
            let mut ty = self.ty()?;
            if mods.row_major.is_some() {
                ty.row_major = mods.row_major;
            }
            // Fonction ?
            if matches!(self.peek_at(1), Some(TokKind::P("("))) {
                let name = self.ident()?;
                p.funcs.push(self.function(name, ty)?);
                continue;
            }
            let decls = self.declarators(&ty, &mods)?;
            self.expect(";")?;
            p.globals.extend(decls);
        }
        p.structs = self.structs.clone();
        Ok(p)
    }

    fn technique(&mut self) -> Result<Technique, String> {
        let name = match self.peek() {
            Some(TokKind::Id(_)) => self.ident()?,
            _ => String::new(),
        };
        let annotations = self.annotations()?;
        self.expect("{")?;
        let mut passes = Vec::new();
        while !self.eat("}") {
            if !self.is_id("pass") {
                return self.err("« pass » attendu");
            }
            self.i += 1;
            let pname = match self.peek() {
                Some(TokKind::Id(_)) => self.ident()?,
                _ => String::new(),
            };
            let pann = self.annotations()?;
            let states = self.state_list()?;
            passes.push(Pass {
                name: pname,
                annotations: pann,
                states,
            });
        }
        Ok(Technique {
            name,
            annotations,
            passes,
        })
    }

    fn function(&mut self, name: String, ret: Ty) -> Result<Function, String> {
        self.expect("(")?;
        let mut params = Vec::new();
        if self.is_id("void") && matches!(self.peek_at(1), Some(TokKind::P(")"))) {
            self.i += 1;
        }
        while !self.eat(")") {
            let mods = self.mods();
            let mut ty = self.ty()?;
            if mods.row_major.is_some() {
                ty.row_major = mods.row_major;
            }
            let pname = self.ident()?;
            if self.eat("[") {
                ty.array = Some(self.number()? as u32);
                self.expect("]")?;
            }
            let (mut semantic, mut reg) = (None, None);
            self.semantic_or_register(&mut semantic, &mut reg)?;
            let default = if self.eat("=") {
                Some(self.assign()?)
            } else {
                None
            };
            params.push(Param {
                name: pname,
                ty,
                mods,
                semantic,
                default,
            });
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        let (mut semantic, mut reg) = (None, None);
        self.semantic_or_register(&mut semantic, &mut reg)?;
        let body = if self.eat(";") {
            None
        } else {
            Some(self.block()?)
        };
        Ok(Function {
            name,
            ret,
            params,
            semantic,
            body,
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.eat("}") {
            if self.peek().is_none() {
                return self.err("« } » attendu");
            }
            v.push(self.stmt()?);
        }
        Ok(v)
    }

    fn is_decl_start(&self) -> bool {
        let mut k = 0;
        while let Some(TokKind::Id(s)) = self.peek_at(k) {
            if [
                "static",
                "const",
                "uniform",
                "row_major",
                "column_major",
                "extern",
                "volatile",
                "precise",
            ]
            .contains(&s.as_str())
            {
                k += 1;
            } else {
                break;
            }
        }
        if matches!(self.peek_at(k), Some(TokKind::Id(s)) if s == "struct") {
            return true;
        }
        if self.type_at(k).is_none() {
            return false;
        }
        // type suivi d'un identifiant, ou vector<…>/matrix<…>
        matches!(self.peek_at(k + 1), Some(TokKind::Id(_)))
            || matches!(self.peek_at(k + 1), Some(TokKind::P("<")))
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        if self.is_p("[") {
            let _ = self.mods();
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if self.is_p("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if let Some(TokKind::Id(k)) = self.peek() {
            match k.as_str() {
                "if" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let a = self.stmt()?;
                    let b = if self.is_id("else") {
                        self.i += 1;
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, Box::new(a), b));
                }
                "for" => {
                    self.i += 1;
                    self.expect("(")?;
                    let init = if self.eat(";") {
                        None
                    } else {
                        let s = if self.is_decl_start() {
                            let m = self.mods();
                            let t = self.ty()?;
                            Stmt::Decl(self.declarators(&t, &m)?)
                        } else {
                            Stmt::Expr(self.expr()?)
                        };
                        self.expect(";")?;
                        Some(Box::new(s))
                    };
                    let cond = if self.is_p(";") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(";")?;
                    let step = if self.is_p(")") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(")")?;
                    let body = self.stmt()?;
                    return Ok(Stmt::For(init, cond, step, Box::new(body)));
                }
                "while" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "do" => {
                    self.i += 1;
                    let body = self.stmt()?;
                    if !self.is_id("while") {
                        return self.err("« while » attendu");
                    }
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::Do(Box::new(body), c));
                }
                "return" => {
                    self.i += 1;
                    let e = if self.is_p(";") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(";")?;
                    return Ok(Stmt::Return(e));
                }
                "discard" => {
                    self.i += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Discard);
                }
                "break" => {
                    self.i += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.i += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Continue);
                }
                _ => {}
            }
        }
        if self.is_decl_start() {
            let m = self.mods();
            let t = self.ty()?;
            let d = self.declarators(&t, &m)?;
            self.expect(";")?;
            return Ok(Stmt::Decl(d));
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(Stmt::Expr(e))
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut e = self.assign()?;
        // Opérateur virgule : on garde la dernière valeur.
        while self.is_p(",") && !matches!(e, Expr::List(_)) {
            self.i += 1;
            let b = self.assign()?;
            e = Expr::Bin(",", Box::new(e), Box::new(b));
        }
        Ok(e)
    }

    fn assign(&mut self) -> Result<Expr, String> {
        let lhs = self.cond()?;
        for op in ["=", "+=", "-=", "*=", "/=", "%="] {
            if self.eat(op) {
                let rhs = self.assign()?;
                let op: &'static str = PUNCTS.iter().find(|p| **p == op).copied().unwrap_or("=");
                return Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }

    fn cond(&mut self) -> Result<Expr, String> {
        let c = self.binary(1)?;
        if self.eat("?") {
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            let Some((op, prec)) = (match self.peek() {
                Some(TokKind::P(p)) => BIN_PREC.iter().find(|(o, _)| o == p).copied(),
                _ => None,
            }) else {
                return Ok(lhs);
            };
            if prec < min {
                return Ok(lhs);
            }
            self.i += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        for op in ["-", "+", "!", "~", "++", "--"] {
            if self.is_p(op) {
                self.i += 1;
                let e = self.unary()?;
                let op: &'static str = PUNCTS.iter().find(|p| **p == op).copied().unwrap_or("-");
                return Ok(Expr::Unary(op, Box::new(e)));
            }
        }
        // Conversion (float3)x
        if self.is_p("(") && self.type_at(1).is_some() {
            let save = self.i;
            self.i += 1;
            let _ = self.mods();
            if let Ok(t) = self.ty() {
                let mut t = t;
                if self.eat("[") {
                    t.array = Some(self.number()? as u32);
                    self.expect("]")?;
                }
                if self.eat(")") {
                    let e = self.unary()?;
                    return Ok(Expr::Cast(t, Box::new(e)));
                }
            }
            self.i = save;
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat(".") {
                let m = self.ident()?;
                if self.is_p("(") {
                    // méthodes d'objets (rares en D3D9) : s.Sample(…)
                    let args = self.args()?;
                    let mut a = vec![e];
                    a.extend(args);
                    e = Expr::Call(format!(".{m}"), a);
                } else {
                    e = Expr::Member(Box::new(e), m);
                }
            } else if self.eat("[") {
                let idx = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else if self.eat("++") {
                e = Expr::Post("++", Box::new(e));
            } else if self.eat("--") {
                e = Expr::Post("--", Box::new(e));
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        self.expect("(")?;
        let mut v = Vec::new();
        while !self.eat(")") {
            v.push(self.assign()?);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(v)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(TokKind::Num(v, f)) => {
                self.i += 1;
                Ok(Expr::Num(v, f))
            }
            Some(TokKind::Str(s)) => {
                self.i += 1;
                Ok(Expr::Str(s))
            }
            Some(TokKind::P("(")) => {
                self.i += 1;
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(TokKind::P("{")) => self.init_list(),
            Some(TokKind::Id(s)) => {
                if s == "true" || s == "TRUE" {
                    self.i += 1;
                    return Ok(Expr::Bool(true));
                }
                if s == "false" || s == "FALSE" {
                    self.i += 1;
                    return Ok(Expr::Bool(false));
                }
                if s == "compile" || s == "asm" {
                    return self.state_value();
                }
                if s == "sampler_state" {
                    self.i += 1;
                    return Ok(Expr::SamplerState(self.state_list()?));
                }
                if self.type_at(0).is_some()
                    && (matches!(self.peek_at(1), Some(TokKind::P("(")))
                        || matches!(self.peek_at(1), Some(TokKind::P("<"))))
                    && !matches!(self.type_at(0).map(|t| t.base), Some(Base::Struct(_)))
                {
                    let t = self.ty()?;
                    let a = self.args()?;
                    return Ok(Expr::Ctor(t, a));
                }
                self.i += 1;
                if self.is_p("(") {
                    let a = self.args()?;
                    return Ok(Expr::Call(s, a));
                }
                Ok(Expr::Ident(s))
            }
            _ => self.err("expression attendue"),
        }
    }
}

/// Texte prétraité (D3DXPreprocessShader).
pub(crate) fn preprocess(
    src: &str,
    defines: &[(String, String)],
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let mut pre = Pre {
        macros: HashMap::new(),
        include,
        out: Vec::new(),
    };
    for (k, v) in defines {
        pre.define(&format!("{k} {v}"));
    }
    pre.run(src, 0)?;
    Ok(pre.out.join("\n"))
}

/// Analyse un source HLSL ou un fichier d'effet.
pub(crate) fn parse(
    src: &str,
    defines: &[(String, String)],
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<Program, String> {
    let (toks, lines) = lex(src, defines, include)?;
    let mut p = Parser {
        t: &toks,
        i: 0,
        lines: &lines,
        structs: HashMap::new(),
        typedefs: HashMap::new(),
    };
    p.program()
}

// ═══════════════════════════════════════════════════════════════════════
// Génération : valeurs et instructions
// ═══════════════════════════════════════════════════════════════════════

/// Registre (les temporaires sont virtuels jusqu'à l'attribution finale).
#[derive(Clone, Copy, Debug, PartialEq)]
enum R {
    T(u32),
    C(u32),
    CRel(u32),
    Lit(u32),
    V(u32),
    Tx(u32),
    S(u32),
    A,
    /// Registre nommé (oPos, oC0, vPos…) : index dans `named`.
    N(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Src {
    r: R,
    swz: [u8; 4],
    n: u8,
    neg: bool,
}

impl Src {
    fn new(r: R, n: u8) -> Src {
        Src {
            r,
            swz: [0, 1, 2, 3],
            n,
            neg: false,
        }
        .fixed()
    }
    /// Les composantes au-delà de `n` répètent la dernière (diffusion).
    fn fixed(mut self) -> Src {
        let n = self.n.clamp(1, 4) as usize;
        for i in n..4 {
            self.swz[i] = self.swz[n - 1];
        }
        self
    }
    fn pick(&self, sel: &[u8]) -> Src {
        let mut s = *self;
        for (i, &c) in sel.iter().enumerate().take(4) {
            s.swz[i] = self.swz[c.min(3) as usize];
        }
        s.n = sel.len().clamp(1, 4) as u8;
        s.fixed()
    }
}

#[derive(Clone, Debug)]
enum V {
    Lit(Vec<f32>),
    S(Src),
    /// Matrice : lignes (rows = true) ou colonnes (false), chacune un vecteur.
    Mat(bool, Vec<V>),
    Sampler(u32, u8),
    Struct(Vec<(String, V)>),
    Arr(Vec<V>),
    /// Tableau uniforme : premier registre, registres par élément, type.
    UArr(u32, u32, Ty),
    Str(#[allow(dead_code)] String),
    Void,
}

impl V {
    fn n(&self) -> u8 {
        match self {
            V::Lit(v) => v.len() as u8,
            V::S(s) => s.n,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug)]
struct Inst {
    op: String,
    dst: Option<(R, u8)>,
    srcs: Vec<Src>,
}

struct Var {
    v: V,
    ty: Ty,
    depth: usize,
}

#[derive(Default)]
struct Frame {
    ret: Option<V>,
    if_depth: usize,
    base: usize,
    done: bool,
    retmask: Option<V>,
}

/// Entrée de la table des constantes produite.
#[derive(Clone, Debug)]
pub(crate) struct CtabEntry {
    pub name: String,
    pub set: u16,
    pub index: u16,
    pub count: u16,
    pub class: u16,
    pub ty: u16,
    pub rows: u16,
    pub cols: u16,
    pub elements: u16,
    pub default: Option<Vec<f32>>,
}

const SWZ: [char; 4] = ['x', 'y', 'z', 'w'];

fn usage_of(sem: &str) -> (String, u32) {
    let up = sem.to_ascii_uppercase();
    let digits = up.trim_end_matches(|c: char| c.is_ascii_digit());
    let idx = up[digits.len()..].parse().unwrap_or(0);
    let name = match digits {
        "SV_POSITION" => "POSITION",
        "SV_TARGET" => "COLOR",
        "SV_DEPTH" => "DEPTH",
        d => d,
    };
    (name.to_string(), idx)
}

fn type_code(t: &Ty) -> u16 {
    match t.base {
        Base::Void => 0,
        Base::Bool => 1,
        Base::Int => 2,
        Base::Float => 3,
        Base::Str => 4,
        Base::Texture => 5,
        Base::Sampler(0) => 10,
        Base::Sampler(1) => 11,
        Base::Sampler(2) => 12,
        Base::Sampler(3) => 13,
        Base::Sampler(_) => 14,
        Base::Shader(true) => 15,
        Base::Shader(false) => 16,
        Base::Struct(_) => 0,
    }
}

struct Gen<'p> {
    p: &'p Program,
    pixel: bool,
    sm3: bool,
    row_major: bool,
    code: Vec<Inst>,
    ntemps: u32,
    lits: Vec<[f32; 4]>,
    lit_fill: Vec<u8>,
    named: Vec<String>,
    dcls: Vec<String>,
    globals: HashMap<String, (V, Ty)>,
    scopes: Vec<HashMap<String, Var>>,
    conds: Vec<V>,
    frames: Vec<Frame>,
    block_pushes: Vec<usize>,
    ctab: Vec<CtabEntry>,
    samplers: HashMap<u32, u8>,
    used: std::collections::HashSet<String>,
    next_input: u32,
    next_output: u32,
    call_depth: usize,
}

fn fold1(v: &[f32], f: impl Fn(f32) -> f32) -> V {
    V::Lit(v.iter().map(|&x| f(x)).collect())
}

fn lit_get(v: &[f32], i: usize) -> f32 {
    v.get(i).or(v.last()).copied().unwrap_or(0.0)
}

impl<'p> Gen<'p> {
    fn temp(&mut self) -> R {
        self.ntemps += 1;
        R::T(self.ntemps - 1)
    }

    fn push(&mut self, op: &str, dst: Option<(R, u8)>, srcs: Vec<Src>) {
        self.code.push(Inst {
            op: op.into(),
            dst,
            srcs,
        });
    }

    /// Constante littérale : scalaires regroupés, vecteurs en `def` entiers.
    fn lit_src(&mut self, v: &[f32]) -> Src {
        let n = v.len().clamp(1, 4);
        if v.iter().all(|&x| x == v[0]) {
            let x = v[0];
            for (k, d) in self.lits.iter().enumerate() {
                if let Some(c) = (0..(self.lit_fill[k] as usize).min(4)).find(|&c| d[c] == x) {
                    return Src {
                        r: R::Lit(k as u32),
                        swz: [c as u8; 4],
                        n: n as u8,
                        neg: false,
                    };
                }
            }
            if let Some(k) = (0..self.lits.len()).find(|&k| self.lit_fill[k] < 4) {
                let c = self.lit_fill[k] as usize;
                self.lits[k][c] = x;
                self.lit_fill[k] += 1;
                return Src {
                    r: R::Lit(k as u32),
                    swz: [c as u8; 4],
                    n: n as u8,
                    neg: false,
                };
            }
            self.lits.push([x, 0.0, 0.0, 0.0]);
            self.lit_fill.push(1);
            return Src {
                r: R::Lit(self.lits.len() as u32 - 1),
                swz: [0; 4],
                n: n as u8,
                neg: false,
            };
        }
        let mut d = [0f32; 4];
        d[..n].copy_from_slice(&v[..n]);
        let k = match self
            .lits
            .iter()
            .enumerate()
            .find(|(k, l)| self.lit_fill[*k] == 5 && l[..n] == d[..n])
        {
            Some((k, _)) => k,
            None => {
                self.lits.push(d);
                // 5 : registre réservé à un vecteur (pas de regroupement).
                self.lit_fill.push(5);
                self.lits.len() - 1
            }
        };
        Src::new(R::Lit(k as u32), n as u8)
    }

    fn src(&mut self, v: &V) -> Result<Src, String> {
        match v {
            V::Lit(l) => Ok(self.lit_src(l)),
            V::S(s) => Ok(*s),
            V::Void => Err("valeur vide utilisée".into()),
            _ => Err("opérande non vectoriel".into()),
        }
    }

    /// Instruction à résultat de `n` composantes dans un nouveau temporaire.
    fn emit(&mut self, op: &str, n: u8, srcs: &[V]) -> Result<V, String> {
        let s: Vec<Src> = srcs.iter().map(|v| self.src(v)).collect::<Result<_, _>>()?;
        let t = self.temp();
        let n = n.clamp(1, 4);
        self.push(op, Some((t, (1u8 << n) - 1)), s);
        Ok(V::S(Src::new(t, n)))
    }

    /// Instruction scalaire (rcp, rsq, exp, log, pow) composante par composante.
    fn emit_scalar(&mut self, op: &str, srcs: &[V]) -> Result<V, String> {
        let n = srcs.iter().map(V::n).max().unwrap_or(1);
        let s: Vec<Src> = srcs.iter().map(|v| self.src(v)).collect::<Result<_, _>>()?;
        let t = self.temp();
        for c in 0..n {
            let ss = s.iter().map(|x| x.pick(&[c.min(x.n - 1)])).collect();
            self.push(op, Some((t, 1 << c)), ss);
        }
        Ok(V::S(Src::new(t, n)))
    }

    fn size2(a: &V, b: &V) -> u8 {
        let (na, nb) = (a.n(), b.n());
        if na == 1 {
            nb
        } else if nb == 1 {
            na
        } else {
            na.min(nb)
        }
    }

    fn trunc(v: V, n: u8) -> V {
        match v {
            V::Lit(mut l) if l.len() > n as usize => {
                l.truncate(n as usize);
                V::Lit(l)
            }
            V::S(s) if s.n > n => V::S(Src { n, ..s }.fixed()),
            v => v,
        }
    }

    fn arith(&mut self, op: &str, a: V, b: V) -> Result<V, String> {
        if let (V::Mat(ra, la), V::Mat(rb, lb)) = (&a, &b) {
            if ra != rb || la.len() != lb.len() {
                return Err("opération entre matrices d'orientations différentes".into());
            }
            let mut out = Vec::new();
            for (x, y) in la.clone().into_iter().zip(lb.clone()) {
                out.push(self.arith(op, x, y)?);
            }
            return Ok(V::Mat(*ra, out));
        }
        if let V::Mat(r, l) = &a {
            let mut out = Vec::new();
            for x in l.clone() {
                out.push(self.arith(op, x, b.clone())?);
            }
            return Ok(V::Mat(*r, out));
        }
        if let V::Mat(r, l) = &b {
            let mut out = Vec::new();
            for x in l.clone() {
                out.push(self.arith(op, a.clone(), x)?);
            }
            return Ok(V::Mat(*r, out));
        }
        let n = Self::size2(&a, &b);
        if let (V::Lit(x), V::Lit(y)) = (&a, &b) {
            let f = |p: f32, q: f32| match op {
                "add" => p + q,
                "sub" => p - q,
                "mul" => p * q,
                "div" => {
                    if q == 0.0 {
                        0.0
                    } else {
                        p / q
                    }
                }
                "min" => p.min(q),
                "max" => p.max(q),
                _ => 0.0,
            };
            return Ok(V::Lit(
                (0..n as usize)
                    .map(|i| f(lit_get(x, i), lit_get(y, i)))
                    .collect(),
            ));
        }
        let (a, b) = (Self::trunc(a, n), Self::trunc(b, n));
        match op {
            "sub" => {
                let nb = self.neg(b);
                self.emit("add", n, &[a, nb])
            }
            "div" => {
                let r = match b {
                    V::Lit(l) => V::Lit(
                        l.iter()
                            .map(|x| if *x == 0.0 { 0.0 } else { 1.0 / x })
                            .collect(),
                    ),
                    b => self.emit_scalar("rcp", &[b])?,
                };
                self.arith("mul", a, r)
            }
            _ => {
                // Simplifications : x*1, x+0.
                if op == "mul" {
                    if let V::Lit(l) = &b {
                        if l.iter().all(|&x| x == 1.0) && a.n() == n {
                            return Ok(a);
                        }
                    }
                }
                if op == "add" {
                    if let V::Lit(l) = &b {
                        if l.iter().all(|&x| x == 0.0) && a.n() == n {
                            return Ok(a);
                        }
                    }
                }
                self.emit(op, n, &[a, b])
            }
        }
    }

    fn neg(&mut self, v: V) -> V {
        match v {
            V::Lit(l) => V::Lit(l.iter().map(|x| -x).collect()),
            V::S(s) => V::S(Src { neg: !s.neg, ..s }),
            V::Mat(r, l) => V::Mat(r, l.into_iter().map(|x| self.neg(x)).collect()),
            v => v,
        }
    }

    fn mad(&mut self, a: V, b: V, c: V) -> Result<V, String> {
        if matches!((&a, &b), (V::Lit(_), V::Lit(_))) {
            let m = self.arith("mul", a, b)?;
            return self.arith("add", m, c);
        }
        let ab = Self::size2(&a, &b);
        let n = if c.n() == 1 {
            ab
        } else if ab == 1 {
            c.n()
        } else {
            ab.min(c.n())
        };
        self.emit("mad", n, &[a, b, c])
    }

    fn comp(&self, v: &V, i: u8) -> V {
        match v {
            V::Lit(l) => V::Lit(vec![lit_get(l, i as usize)]),
            V::S(s) => V::S(s.pick(&[i.min(s.n - 1)])),
            v => v.clone(),
        }
    }

    /// Assemble des scalaires en un vecteur (mov groupés).
    fn gather(&mut self, parts: &[V]) -> Result<V, String> {
        if parts.len() == 1 {
            return Ok(parts[0].clone());
        }
        if parts.iter().all(|p| matches!(p, V::Lit(_))) {
            return Ok(V::Lit(
                parts
                    .iter()
                    .map(|p| match p {
                        V::Lit(l) => lit_get(l, 0),
                        _ => 0.0,
                    })
                    .collect(),
            ));
        }
        let n = parts.len().min(4);
        let t = self.temp();
        // Littéraux d'abord, en un seul mov.
        let lits: Vec<usize> = (0..n).filter(|&i| matches!(parts[i], V::Lit(_))).collect();
        if !lits.is_empty() {
            let mut vals = [0f32; 4];
            for &i in &lits {
                if let V::Lit(l) = &parts[i] {
                    vals[i] = lit_get(l, 0);
                }
            }
            let uniform = lits.iter().all(|&i| vals[i] == vals[lits[0]]);
            let s = if uniform {
                self.lit_src(&[vals[lits[0]]])
            } else {
                self.lit_src(&vals)
            };
            let mask = lits.iter().fold(0u8, |m, &i| m | 1 << i);
            self.push("mov", Some((t, mask)), vec![Src { n: 4, ..s }]);
        }
        let mut done = vec![false; n];
        for i in 0..n {
            let V::S(si) = &parts[i] else { continue };
            if done[i] {
                continue;
            }
            let mut sw = [si.swz[0]; 4];
            let mut mask = 0u8;
            for j in i..n {
                if let V::S(sj) = &parts[j] {
                    if !done[j] && sj.r == si.r && sj.neg == si.neg {
                        sw[j] = sj.swz[0];
                        mask |= 1 << j;
                        done[j] = true;
                    }
                }
            }
            let s = Src {
                r: si.r,
                swz: sw,
                n: 4,
                neg: si.neg,
            };
            self.push("mov", Some((t, mask)), vec![s]);
        }
        Ok(V::S(Src::new(t, n as u8)))
    }

    fn dot(&mut self, a: V, b: V) -> Result<V, String> {
        let n = a.n().min(b.n());
        if let (V::Lit(x), V::Lit(y)) = (&a, &b) {
            return Ok(V::Lit(vec![(0..n as usize)
                .map(|i| lit_get(x, i) * lit_get(y, i))
                .sum()]));
        }
        match n {
            4 => self.emit("dp4", 1, &[a, b]),
            3 => self.emit("dp3", 1, &[a, b]),
            2 => {
                if self.pixel {
                    let z = V::Lit(vec![0.0]);
                    self.emit("dp2add", 1, &[a, b, z])
                } else {
                    let m = self.emit("mul", 2, &[a, b])?;
                    let (x, y) = (self.comp(&m, 0), self.comp(&m, 1));
                    self.emit("add", 1, &[x, y])
                }
            }
            _ => self.arith("mul", a, b),
        }
    }

    /// Masque 0/1 d'une comparaison.
    fn compare(&mut self, op: &str, a: V, b: V) -> Result<V, String> {
        let n = Self::size2(&a, &b);
        if let (V::Lit(x), V::Lit(y)) = (&a, &b) {
            return Ok(V::Lit(
                (0..n as usize)
                    .map(|i| {
                        let (p, q) = (lit_get(x, i), lit_get(y, i));
                        (match op {
                            "<" => p < q,
                            ">" => p > q,
                            "<=" => p <= q,
                            ">=" => p >= q,
                            "==" => p == q,
                            _ => p != q,
                        }) as i32 as f32
                    })
                    .collect(),
            ));
        }
        let (a, b) = (Self::trunc(a, n), Self::trunc(b, n));
        let one = V::Lit(vec![1.0]);
        let zero = V::Lit(vec![0.0]);
        if !self.pixel {
            return match op {
                "<" => self.emit("slt", n, &[a, b]),
                ">" => self.emit("slt", n, &[b, a]),
                ">=" => self.emit("sge", n, &[a, b]),
                "<=" => self.emit("sge", n, &[b, a]),
                _ => {
                    let x = self.emit("sge", n, &[a.clone(), b.clone()])?;
                    let y = self.emit("sge", n, &[b, a])?;
                    let eq = self.emit("mul", n, &[x, y])?;
                    if op == "==" {
                        Ok(eq)
                    } else {
                        let ne = self.neg(eq);
                        self.emit("add", n, &[ne, one])
                    }
                }
            };
        }
        match op {
            ">=" | "<" => {
                let d = self.arith("sub", a, b)?;
                let (p, q) = if op == ">=" { (one, zero) } else { (zero, one) };
                self.emit("cmp", n, &[d, p, q])
            }
            "<=" | ">" => {
                let d = self.arith("sub", b, a)?;
                let (p, q) = if op == "<=" { (one, zero) } else { (zero, one) };
                self.emit("cmp", n, &[d, p, q])
            }
            _ => {
                let d = self.arith("sub", a, b)?;
                let sq = self.emit("mul", n, &[d.clone(), d])?;
                let nsq = self.neg(sq);
                let (p, q) = if op == "==" { (one, zero) } else { (zero, one) };
                self.emit("cmp", n, &[nsq, p, q])
            }
        }
    }

    /// m ? a : b, m valant 0 ou 1.
    fn select(&mut self, m: &V, a: V, b: V) -> Result<V, String> {
        if let V::Lit(l) = m {
            return Ok(if lit_get(l, 0) != 0.0 { a } else { b });
        }
        match (a, b) {
            (V::Struct(fa), V::Struct(fb)) => {
                let mut out = Vec::new();
                for ((n, x), (_, y)) in fa.into_iter().zip(fb) {
                    out.push((n, self.select(m, x, y)?));
                }
                Ok(V::Struct(out))
            }
            (V::Arr(xa), V::Arr(xb)) => {
                let mut out = Vec::new();
                for (x, y) in xa.into_iter().zip(xb) {
                    out.push(self.select(m, x, y)?);
                }
                Ok(V::Arr(out))
            }
            (V::Mat(r, xa), V::Mat(r2, xb)) if r == r2 => {
                let mut out = Vec::new();
                for (x, y) in xa.into_iter().zip(xb) {
                    out.push(self.select(m, x, y)?);
                }
                Ok(V::Mat(r, out))
            }
            (a @ (V::Lit(_) | V::S(_)), b @ (V::Lit(_) | V::S(_))) => {
                let n = a.n().max(b.n());
                self.emit("lrp", n, &[m.clone(), a, b])
            }
            (a, _) => Ok(a),
        }
    }

    /// Valeur booléenne 0/1 d'une expression quelconque.
    fn truth(&mut self, v: V, ty: &Ty) -> Result<V, String> {
        if ty.base == Base::Bool {
            return Ok(v);
        }
        self.compare("!=", v, V::Lit(vec![0.0]))
    }

    fn cur_mask(&self) -> Option<V> {
        self.conds.last().cloned()
    }

    fn push_mask(&mut self, m: V) -> Result<(), String> {
        let m = Self::trunc(m, 1);
        let cum = match self.cur_mask() {
            Some(c) => self.arith("mul", c, m)?,
            None => m,
        };
        self.conds.push(cum);
        Ok(())
    }

    fn not(&mut self, m: V) -> Result<V, String> {
        let nm = self.neg(m);
        self.arith("add", nm, V::Lit(vec![1.0]))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Génération : expressions
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
enum Acc {
    F(String),
    I(usize),
}

#[derive(PartialEq)]
enum Flow {
    Next,
    Break,
    Continue,
}

fn parse_swz(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() > 4 {
        return None;
    }
    let a: Option<Vec<u8>> = s.chars().map(|c| "xyzw".find(c).map(|i| i as u8)).collect();
    a.or_else(|| s.chars().map(|c| "rgba".find(c).map(|i| i as u8)).collect())
}

/// « _m01_m02 » ou « _11_12 » → [(ligne, colonne)].
fn parse_mat_swz(s: &str) -> Option<Vec<(u8, u8)>> {
    let mut v = Vec::new();
    for part in s.split('_').skip(1) {
        let b = part.as_bytes();
        let (r, c) = if b.first() == Some(&b'm') && b.len() == 3 {
            (b[1].checked_sub(b'0')?, b[2].checked_sub(b'0')?)
        } else if b.len() == 2 {
            (b[0].checked_sub(b'1')?, b[1].checked_sub(b'1')?)
        } else {
            return None;
        };
        if r > 3 || c > 3 {
            return None;
        }
        v.push((r, c));
    }
    (!v.is_empty() && s.starts_with('_')).then_some(v)
}

fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::S(x), V::S(y)) => x == y,
        (V::Lit(x), V::Lit(y)) => x == y,
        _ => false,
    }
}

impl<'p> Gen<'p> {
    fn lookup(&self, name: &str) -> Option<(V, Ty)> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some((v.v.clone(), v.ty.clone()));
            }
        }
        self.globals.get(name).cloned()
    }

    fn fields_of(&self, ty: &Ty) -> Result<&'p [Field], String> {
        match &ty.base {
            Base::Struct(n) => self
                .p
                .structs
                .get(n)
                .map(|v| v.as_slice())
                .ok_or_else(|| format!("structure inconnue : {n}")),
            _ => Err("structure attendue".into()),
        }
    }

    fn zero(&self, ty: &Ty) -> Result<V, String> {
        if let Some(n) = ty.array {
            let e = self.zero(&ty.elem())?;
            return Ok(V::Arr(vec![e; n as usize]));
        }
        Ok(match &ty.base {
            Base::Struct(_) => {
                let mut v = Vec::new();
                for f in self.fields_of(ty)? {
                    v.push((f.name.clone(), self.zero(&f.ty)?));
                }
                V::Struct(v)
            }
            _ if ty.class == 2 => V::Mat(
                true,
                vec![V::Lit(vec![0.0; ty.cols as usize]); ty.rows as usize],
            ),
            Base::Sampler(_) | Base::Texture | Base::Shader(_) | Base::Str => V::Void,
            _ => V::Lit(vec![0.0; ty.cols as usize]),
        })
    }

    /// Conversion implicite ou explicite vers `to`.
    fn convert(&mut self, v: V, from: &Ty, to: &Ty) -> Result<V, String> {
        if to.array.is_some() || !to.numeric() {
            if let (Base::Struct(_), V::Lit(l)) = (&to.base, &v) {
                if l.iter().all(|&x| x == 0.0) {
                    return self.zero(to);
                }
            }
            return Ok(v);
        }
        if to.class == 2 {
            return match v {
                V::Mat(rows, lines) => {
                    let (nl, nc) = if rows {
                        (to.rows, to.cols)
                    } else {
                        (to.cols, to.rows)
                    };
                    Ok(V::Mat(
                        rows,
                        lines
                            .into_iter()
                            .take(nl as usize)
                            .map(|l| Self::trunc(l, nc))
                            .collect(),
                    ))
                }
                v if v.n() == 1 => {
                    let line = self.broadcast(v, to.cols);
                    Ok(V::Mat(true, vec![line; to.rows as usize]))
                }
                _ => Err("conversion vers une matrice impossible".into()),
            };
        }
        if matches!(v, V::Mat(..)) {
            return Err("matrice utilisée comme vecteur".into());
        }
        let n = to.cols;
        let mut v = if v.n() == 1 && n > 1 {
            self.broadcast(v, n)
        } else {
            Self::trunc(v, n)
        };
        if to.base == Base::Bool && from.base != Base::Bool && from.numeric() {
            v = self.compare("!=", v, V::Lit(vec![0.0]))?;
        }
        Ok(v)
    }

    fn broadcast(&self, v: V, n: u8) -> V {
        match v {
            V::Lit(l) => V::Lit(vec![lit_get(&l, 0); n as usize]),
            V::S(s) => V::S(Src {
                n,
                swz: [s.swz[0]; 4],
                ..s
            }),
            v => v,
        }
    }

    fn swizzle(&self, v: &V, sel: &[u8]) -> Result<V, String> {
        let n = v.n();
        if sel.iter().any(|&c| c >= n.max(1) && n > 1) {
            return Err("composante hors du vecteur".into());
        }
        Ok(match v {
            V::Lit(l) => V::Lit(sel.iter().map(|&c| lit_get(l, c as usize)).collect()),
            V::S(s) => V::S(s.pick(sel)),
            _ => return Err("permutation sur une valeur non vectorielle".into()),
        })
    }

    fn mat_elem(&self, m: &V, r: u8, c: u8) -> V {
        match m {
            V::Mat(true, l) => self.comp(&l[(r as usize).min(l.len() - 1)], c),
            V::Mat(false, l) => self.comp(&l[(c as usize).min(l.len() - 1)], r),
            v => v.clone(),
        }
    }

    fn const_index(&mut self, e: &Expr) -> Result<Option<usize>, String> {
        let (v, _) = self.expr(e)?;
        Ok(match v {
            V::Lit(l) => Some(lit_get(&l, 0).max(0.0) as usize),
            _ => None,
        })
    }

    /// Élément d'un tableau uniforme (indice constant ou dynamique).
    fn uarr_elem(&mut self, first: u32, stride: u32, ty: &Ty, idx: V) -> Result<V, String> {
        let et = ty.elem();
        let mk = |r: &dyn Fn(u32) -> R, me: &Self| -> V {
            if et.class == 2 {
                let rows = et.row_major.unwrap_or(me.row_major);
                let (nl, nc) = if rows {
                    (et.rows, et.cols)
                } else {
                    (et.cols, et.rows)
                };
                V::Mat(
                    rows,
                    (0..nl as u32).map(|k| V::S(Src::new(r(k), nc))).collect(),
                )
            } else {
                V::S(Src::new(r(0), et.cols))
            }
        };
        if let V::Lit(l) = &idx {
            let i = lit_get(l, 0).max(0.0) as u32;
            let base = first + i * stride;
            return Ok(mk(&|k| R::C(base + k), self));
        }
        if self.pixel {
            return Err("indice dynamique de tableau dans un pixel shader".into());
        }
        let a = self.arith("mul", Self::trunc(idx, 1), V::Lit(vec![stride as f32]))?;
        let s = self.src(&a)?;
        self.push("mova", Some((R::A, 1)), vec![s]);
        // Copie immédiate : a0 peut être réutilisé ensuite.
        let rel = mk(&|k| R::CRel(first + k), self);
        Ok(match rel {
            V::Mat(r, lines) => {
                let mut out = Vec::new();
                for l in lines {
                    let n = l.n();
                    out.push(self.emit("mov", n, &[l])?);
                }
                V::Mat(r, out)
            }
            v => {
                let n = v.n();
                self.emit("mov", n, &[v])?
            }
        })
    }

    fn expr(&mut self, e: &Expr) -> Result<(V, Ty), String> {
        let f1 = Ty::float(1);
        Ok(match e {
            Expr::Num(v, f) => (
                V::Lit(vec![*v as f32]),
                Ty::scalar(if *f { Base::Float } else { Base::Int }),
            ),
            Expr::Bool(b) => (V::Lit(vec![*b as i32 as f32]), Ty::scalar(Base::Bool)),
            Expr::Str(s) => (V::Str(s.clone()), Ty::scalar(Base::Str)),
            Expr::Null => (V::Void, Ty::scalar(Base::Void)),
            Expr::Ident(n) => self
                .lookup(n)
                .ok_or_else(|| format!("identifiant inconnu : {n}"))?,
            Expr::Unary(op, x) => {
                let (v, t) = self.expr(x)?;
                match *op {
                    "-" => (self.neg(v), t),
                    "+" => (v, t),
                    "!" => {
                        let m = self.truth(v, &t)?;
                        let n = self.not(m)?;
                        let c = t.cols;
                        (n, Ty::vec(Base::Bool, c))
                    }
                    "++" | "--" => {
                        let d = if *op == "++" { 1.0 } else { -1.0 };
                        let nv = self.arith("add", v, V::Lit(vec![d]))?;
                        self.store(x, nv.clone(), &t)?;
                        (nv, t)
                    }
                    _ => return Err(format!("opérateur {op} non pris en charge")),
                }
            }
            Expr::Post(op, x) => {
                let (v, t) = self.expr(x)?;
                let d = if *op == "++" { 1.0 } else { -1.0 };
                let nv = self.arith("add", v.clone(), V::Lit(vec![d]))?;
                self.store(x, nv, &t)?;
                (v, t)
            }
            Expr::Bin(op, a, b) => self.binary(op, a, b)?,
            Expr::Assign(op, l, r) => {
                let (rv, rt) = self.expr(r)?;
                let (val, ty) = if *op == "=" {
                    (rv, rt)
                } else {
                    let (lv, lt) = self.expr(l)?;
                    let o = match *op {
                        "+=" => "add",
                        "-=" => "sub",
                        "*=" => "mul",
                        "/=" => "div",
                        _ => return Err(format!("opérateur {op} non pris en charge")),
                    };
                    (self.arith(o, lv, rv)?, lt)
                };
                self.store(l, val, &ty)?
            }
            Expr::Cond(c, a, b) => {
                let (cv, ct) = self.expr(c)?;
                let m = self.truth(cv, &ct)?;
                let (av, at) = self.expr(a)?;
                let (bv, bt) = self.expr(b)?;
                let n = av.n().max(bv.n());
                let ty = if at.cols >= bt.cols { at } else { bt };
                let (av, bv) = (self.broadcast_to(av, n), self.broadcast_to(bv, n));
                let m = if m.n() == 1 { m } else { Self::trunc(m, n) };
                (self.select(&m, av, bv)?, ty)
            }
            Expr::Call(name, args) => self.call(name, args)?,
            Expr::Ctor(ty, args) => self.construct(ty, args)?,
            Expr::Cast(ty, x) => {
                let (v, t) = self.expr(x)?;
                let to = if ty.class == 2 {
                    ty.clone()
                } else {
                    Ty {
                        array: None,
                        ..ty.clone()
                    }
                };
                (self.convert(v, &t, &to)?, to)
            }
            Expr::Member(x, m) => {
                let (v, t) = self.expr(x)?;
                match (&v, &t.base) {
                    (V::Struct(fs), _) => {
                        let ft = self
                            .fields_of(&t)?
                            .iter()
                            .find(|f| f.name == *m)
                            .map(|f| f.ty.clone())
                            .ok_or_else(|| format!("champ inconnu : {m}"))?;
                        let fv = fs
                            .iter()
                            .find(|(n, _)| n == m)
                            .map(|(_, v)| v.clone())
                            .unwrap_or(V::Void);
                        (fv, ft)
                    }
                    (V::Mat(..), _) => {
                        let sel = parse_mat_swz(m)
                            .ok_or_else(|| format!("membre de matrice invalide : {m}"))?;
                        let parts: Vec<V> =
                            sel.iter().map(|&(r, c)| self.mat_elem(&v, r, c)).collect();
                        let n = parts.len() as u8;
                        (self.gather(&parts)?, Ty::vec(t.base.clone(), n))
                    }
                    _ => {
                        let sel =
                            parse_swz(m).ok_or_else(|| format!("permutation invalide : {m}"))?;
                        let n = sel.len() as u8;
                        (self.swizzle(&v, &sel)?, Ty::vec(t.base.clone(), n))
                    }
                }
            }
            Expr::Index(x, i) => {
                let (v, t) = self.expr(x)?;
                match v {
                    V::Arr(items) => {
                        let k = self
                            .const_index(i)?
                            .ok_or("indice dynamique d'un tableau local non pris en charge")?;
                        (
                            items.get(k).cloned().ok_or("indice hors du tableau")?,
                            t.elem(),
                        )
                    }
                    V::UArr(first, stride, ty) => {
                        let (iv, _) = self.expr(i)?;
                        (self.uarr_elem(first, stride, &ty, iv)?, ty.elem())
                    }
                    V::Mat(..) => {
                        let k = self.const_index(i)?.ok_or("indice dynamique de matrice")?;
                        let parts: Vec<V> =
                            (0..t.cols).map(|c| self.mat_elem(&v, k as u8, c)).collect();
                        (self.gather(&parts)?, Ty::vec(t.base.clone(), t.cols))
                    }
                    v => {
                        let k = self.const_index(i)?.ok_or("indice dynamique de vecteur")?;
                        (self.comp(&v, k as u8), Ty::scalar(t.base.clone()))
                    }
                }
            }
            Expr::List(items) => {
                let mut parts = Vec::new();
                for it in items {
                    let (v, _) = self.expr(it)?;
                    for c in 0..v.n() {
                        parts.push(self.comp(&v, c));
                    }
                }
                let n = parts.len().min(4) as u8;
                parts.truncate(4);
                (self.gather(&parts)?, Ty::float(n))
            }
            Expr::Compile(..) | Expr::Asm(_) | Expr::SamplerState(_) => {
                return Err("objet d'effet dans une expression de shader".into())
            }
        })
        .map(|(v, t)| {
            let _ = &f1;
            (v, t)
        })
    }

    fn broadcast_to(&self, v: V, n: u8) -> V {
        if v.n() == 1 && n > 1 {
            self.broadcast(v, n)
        } else {
            v
        }
    }

    fn binary(&mut self, op: &str, a: &Expr, b: &Expr) -> Result<(V, Ty), String> {
        let (av, at) = self.expr(a)?;
        let (bv, bt) = self.expr(b)?;
        if op == "," {
            return Ok((bv, bt));
        }
        let wide = |x: &Ty, y: &Ty| -> Ty {
            let base = if x.base == Base::Float || y.base == Base::Float {
                Base::Float
            } else if x.base == Base::Int || y.base == Base::Int {
                Base::Int
            } else {
                x.base.clone()
            };
            let t = if x.class == 2
                || (y.class != 2 && x.cols >= y.cols && x.cols > 1)
                || y.cols == 1
            {
                x
            } else {
                y
            };
            Ty {
                base,
                array: None,
                ..t.clone()
            }
        };
        let ty = wide(&at, &bt);
        let n = Self::size2(&av, &bv);
        let ty = if ty.class == 2 {
            ty
        } else {
            Ty::vec(ty.base.clone(), n)
        };
        Ok(match op {
            "+" => (self.arith("add", av, bv)?, ty),
            "-" => (self.arith("sub", av, bv)?, ty),
            "*" => (self.arith("mul", av, bv)?, ty),
            "/" => {
                let int = at.base != Base::Float && bt.base != Base::Float;
                let q = self.arith("div", av, bv)?;
                if int {
                    if let V::Lit(l) = &q {
                        (fold1(l, f32::trunc), ty)
                    } else {
                        (q, ty)
                    }
                } else {
                    (q, ty)
                }
            }
            "%" => (self.fmod(av, bv)?, ty),
            "<" | ">" | "<=" | ">=" | "==" | "!=" => {
                (self.compare(op, av, bv)?, Ty::vec(Base::Bool, n))
            }
            "&&" | "||" => {
                let x = self.truth(av, &at)?;
                let y = self.truth(bv, &bt)?;
                let o = if op == "&&" { "mul" } else { "max" };
                (self.arith(o, x, y)?, Ty::vec(Base::Bool, n))
            }
            _ => {
                // Opérateurs entiers : seulement sur des constantes.
                if let (V::Lit(x), V::Lit(y)) = (&av, &bv) {
                    let r: Vec<f32> = (0..n as usize)
                        .map(|i| {
                            let (p, q) = (lit_get(x, i) as i64, lit_get(y, i) as i64);
                            (match op {
                                "&" => p & q,
                                "|" => p | q,
                                "^" => p ^ q,
                                "<<" => p << (q & 31),
                                _ => p >> (q & 31),
                            }) as f32
                        })
                        .collect();
                    (V::Lit(r), ty)
                } else {
                    return Err(format!(
                        "opérateur {op} non pris en charge sur des variables"
                    ));
                }
            }
        })
    }

    fn fmod(&mut self, a: V, b: V) -> Result<V, String> {
        if let (V::Lit(x), V::Lit(y)) = (&a, &b) {
            let n = x.len().max(y.len());
            return Ok(V::Lit(
                (0..n).map(|i| lit_get(x, i) % lit_get(y, i)).collect(),
            ));
        }
        let q = self.arith("div", a, b.clone())?;
        let n = q.n();
        let aq = self.emit("abs", n, std::slice::from_ref(&q))?;
        let fr = self.emit("frc", n, std::slice::from_ref(&aq))?;
        let fl = self.arith("sub", aq, fr)?;
        let pos = self.compare(">=", q.clone(), V::Lit(vec![0.0]))?;
        let nfl = self.neg(fl.clone());
        let tr = self.select(&pos, fl, nfl)?;
        let f = self.arith("sub", q, tr)?;
        self.arith("mul", f, b)
    }

    fn construct(&mut self, ty: &Ty, args: &[Expr]) -> Result<(V, Ty), String> {
        let mut parts = Vec::new();
        for a in args {
            let (v, _) = self.expr(a)?;
            match &v {
                V::Mat(..) => {
                    let (r, c) = match &v {
                        V::Mat(true, l) => (l.len() as u8, l[0].n()),
                        V::Mat(false, l) => (l[0].n(), l.len() as u8),
                        _ => (0, 0),
                    };
                    for i in 0..r {
                        for j in 0..c {
                            parts.push(self.mat_elem(&v, i, j));
                        }
                    }
                }
                _ => {
                    for c in 0..v.n() {
                        parts.push(self.comp(&v, c));
                    }
                }
            }
        }
        let need = ty.components() as usize;
        if parts.len() == 1 && need > 1 {
            parts = vec![parts[0].clone(); need];
        }
        if parts.len() < need {
            return Err(format!(
                "constructeur : {} composantes au lieu de {need}",
                parts.len()
            ));
        }
        let ty = Ty {
            array: None,
            ..ty.clone()
        };
        if ty.class == 2 {
            let mut lines = Vec::new();
            for r in 0..ty.rows as usize {
                let c = ty.cols as usize;
                lines.push(self.gather(&parts[r * c..r * c + c])?);
            }
            return Ok((V::Mat(true, lines), ty));
        }
        let v = self.gather(&parts[..need])?;
        let v = if ty.base == Base::Bool {
            self.truth(v, &Ty::float(ty.cols))?
        } else {
            v
        };
        Ok((v, ty))
    }

    /// Valeur initiale d'une déclaration (listes { } comprises).
    fn init_value(&mut self, e: &Expr, ty: &Ty) -> Result<V, String> {
        if let Expr::List(items) = e {
            if let Some(n) = ty.array {
                let et = ty.elem();
                // Liste plate de scalaires ou liste d'éléments.
                let flat = items.len() as u32 != n && et.components() > 1;
                if flat {
                    let mut scal = Vec::new();
                    for it in items {
                        let (v, _) = self.expr(it)?;
                        for c in 0..v.n() {
                            scal.push(self.comp(&v, c));
                        }
                    }
                    let k = et.components() as usize;
                    let mut out = Vec::new();
                    for i in 0..n as usize {
                        let chunk = scal
                            .get(i * k..i * k + k)
                            .ok_or("liste d'initialisation trop courte")?;
                        out.push(self.shape(chunk, &et)?);
                    }
                    return Ok(V::Arr(out));
                }
                let mut out = Vec::new();
                for it in items {
                    out.push(self.init_value(it, &et)?);
                }
                return Ok(V::Arr(out));
            }
            if let Base::Struct(_) = &ty.base {
                let fields = self.fields_of(ty)?;
                let mut out = Vec::new();
                for (f, it) in fields.iter().zip(items) {
                    out.push((f.name.clone(), self.init_value(it, &f.ty)?));
                }
                return Ok(V::Struct(out));
            }
            let mut scal = Vec::new();
            for it in items {
                let (v, _) = self.expr(it)?;
                match &v {
                    V::Mat(..) => return Err("matrice dans une liste".into()),
                    _ => {
                        for c in 0..v.n() {
                            scal.push(self.comp(&v, c));
                        }
                    }
                }
            }
            let k = ty.components() as usize;
            if scal.len() == 1 && k > 1 {
                scal = vec![scal[0].clone(); k];
            }
            let chunk = scal
                .get(..k)
                .ok_or("liste d'initialisation trop courte")?
                .to_vec();
            return self.shape(&chunk, ty);
        }
        let (v, t) = self.expr(e)?;
        self.convert(v, &t, ty)
    }

    fn shape(&mut self, scal: &[V], ty: &Ty) -> Result<V, String> {
        if ty.class == 2 {
            let c = ty.cols as usize;
            let mut lines = Vec::new();
            for r in 0..ty.rows as usize {
                lines.push(self.gather(&scal[r * c..r * c + c])?);
            }
            return Ok(V::Mat(true, lines));
        }
        self.gather(scal)
    }

    /// Chemin d'une valeur modifiable : variable racine puis accès.
    fn lpath(&mut self, e: &Expr) -> Result<(String, Vec<Acc>), String> {
        match e {
            Expr::Ident(n) => Ok((n.clone(), Vec::new())),
            Expr::Member(x, m) => {
                let (n, mut p) = self.lpath(x)?;
                p.push(Acc::F(m.clone()));
                Ok((n, p))
            }
            Expr::Index(x, i) => {
                let (n, mut p) = self.lpath(x)?;
                let k = self.const_index(i)?.ok_or("indice dynamique en écriture")?;
                p.push(Acc::I(k));
                Ok((n, p))
            }
            _ => Err("expression non modifiable".into()),
        }
    }

    /// Écrit `val` dans la valeur `cur` (de type `ty`) au chemin `path`.
    fn write_at(&mut self, cur: V, ty: &Ty, path: &[Acc], val: V, vty: &Ty) -> Result<V, String> {
        let Some((first, rest)) = path.split_first() else {
            return self.convert(val, vty, ty);
        };
        match (first, cur) {
            (Acc::F(f), V::Struct(mut fs)) => {
                let ft = self
                    .fields_of(ty)?
                    .iter()
                    .find(|x| x.name == *f)
                    .map(|x| x.ty.clone())
                    .ok_or_else(|| format!("champ inconnu : {f}"))?;
                let pos = fs.iter().position(|(n, _)| n == f).ok_or("champ absent")?;
                let old = fs[pos].1.clone();
                fs[pos].1 = self.write_at(old, &ft, rest, val, vty)?;
                Ok(V::Struct(fs))
            }
            (Acc::I(i), V::Arr(mut items)) => {
                let et = ty.elem();
                let old = items.get(*i).cloned().ok_or("indice hors du tableau")?;
                items[*i] = self.write_at(old, &et, rest, val, vty)?;
                Ok(V::Arr(items))
            }
            (Acc::I(i), m @ V::Mat(..)) => {
                if !rest.is_empty() {
                    return Err("écriture partielle d'une ligne de matrice".into());
                }
                let row = self.convert(val, vty, &Ty::float(ty.cols))?;
                let V::Mat(rows, mut lines) = m else {
                    unreachable!()
                };
                if rows {
                    lines[*i] = row;
                    Ok(V::Mat(true, lines))
                } else {
                    // Repasse en lignes.
                    let mut rl = Vec::new();
                    let tmp = V::Mat(false, lines);
                    for r in 0..ty.rows {
                        if r as usize == *i {
                            rl.push(row.clone());
                        } else {
                            let parts: Vec<V> =
                                (0..ty.cols).map(|c| self.mat_elem(&tmp, r, c)).collect();
                            rl.push(self.gather(&parts)?);
                        }
                    }
                    Ok(V::Mat(true, rl))
                }
            }
            (acc, cur @ (V::S(_) | V::Lit(_))) if rest.is_empty() => {
                let sel = match acc {
                    Acc::F(f) => {
                        parse_swz(f).ok_or_else(|| format!("permutation invalide : {f}"))?
                    }
                    Acc::I(i) => vec![*i as u8],
                };
                let n = ty.cols.max(1);
                let val = self.convert(val, vty, &Ty::float(sel.len() as u8))?;
                if let (V::Lit(c), V::Lit(v)) = (&cur, &val) {
                    let mut c = c.clone();
                    c.resize(n as usize, 0.0);
                    for (k, &p) in sel.iter().enumerate() {
                        if let Some(x) = c.get_mut(p as usize) {
                            *x = lit_get(v, k);
                        }
                    }
                    return Ok(V::Lit(c));
                }
                let t = self.temp();
                let full = (1u8 << n) - 1;
                let mask = sel.iter().fold(0u8, |m, &p| m | 1 << p);
                if mask & full != full {
                    let cs = self.src(&cur)?;
                    self.push("mov", Some((t, full & !mask)), vec![cs]);
                }
                let vs = self.src(&val)?;
                let mut sw = [vs.swz[0]; 4];
                for (k, &p) in sel.iter().enumerate() {
                    sw[p as usize & 3] = vs.swz[k.min(3)];
                }
                self.push(
                    "mov",
                    Some((t, mask)),
                    vec![Src {
                        swz: sw,
                        n: 4,
                        ..vs
                    }],
                );
                Ok(V::S(Src::new(t, n)))
            }
            _ => Err("écriture non prise en charge".into()),
        }
    }

    fn store(&mut self, lv: &Expr, val: V, vty: &Ty) -> Result<(V, Ty), String> {
        let (name, path) = self.lpath(lv)?;
        let pos = self
            .scopes
            .iter()
            .rposition(|s| s.contains_key(&name))
            .ok_or_else(|| format!("écriture dans {name}, qui n'est pas une variable locale"))?;
        let (old, ty, depth) = {
            let v = &self.scopes[pos][&name];
            (v.v.clone(), v.ty.clone(), v.depth)
        };
        let mut new = self.write_at(old.clone(), &ty, &path, val, vty)?;
        if depth < self.conds.len() {
            if let Some(m) = self.cur_mask() {
                new = self.select_deep(&m, new, old)?;
            }
        }
        if let Some(v) = self.scopes[pos].get_mut(&name) {
            v.v = new;
        }
        let (rv, rt) = self.expr(lv)?;
        Ok((rv, rt))
    }

    fn select_deep(&mut self, m: &V, a: V, b: V) -> Result<V, String> {
        if same(&a, &b) {
            return Ok(a);
        }
        match (a, b) {
            (V::Struct(fa), V::Struct(fb)) => {
                let mut out = Vec::new();
                for ((n, x), (_, y)) in fa.into_iter().zip(fb) {
                    out.push((n, self.select_deep(m, x, y)?));
                }
                Ok(V::Struct(out))
            }
            (V::Arr(xa), V::Arr(xb)) => {
                let mut out = Vec::new();
                for (x, y) in xa.into_iter().zip(xb) {
                    out.push(self.select_deep(m, x, y)?);
                }
                Ok(V::Arr(out))
            }
            (a, b) => self.select(m, a, b),
        }
    }

    fn declare(&mut self, name: &str, v: V, ty: Ty) {
        let depth = self.conds.len();
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), Var { v, ty, depth });
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Génération : fonctions intrinsèques et fonctions du programme
// ═══════════════════════════════════════════════════════════════════════

#[allow(clippy::excessive_precision)]
const SINCOS1: [f32; 4] = [
    -1.550_099_2e-6,
    -2.170_138_9e-5,
    0.002_604_166_7,
    0.000_260_416_68,
];
#[allow(clippy::excessive_precision)]
const SINCOS2: [f32; 4] = [-0.020_833_334, -0.125, 1.0, 0.5];

impl<'p> Gen<'p> {
    fn call(&mut self, name: &str, args: &[Expr]) -> Result<(V, Ty), String> {
        // Fonctions du programme d'abord (elles masquent les intrinsèques).
        let user = self
            .p
            .funcs
            .iter()
            .filter(|f| f.name == name && f.body.is_some())
            .find(|f| {
                f.params.len() == args.len()
                    || (f.params.len() > args.len()
                        && f.params[args.len()..].iter().all(|p| p.default.is_some()))
            });
        if let Some(f) = user {
            return self.inline(f, args);
        }
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a)?);
        }
        self.intrinsic(name, vals, args)
    }

    fn inline(&mut self, f: &'p Function, args: &[Expr]) -> Result<(V, Ty), String> {
        if self.call_depth > 32 {
            return Err(format!("{} : récursion non prise en charge", f.name));
        }
        let mut scope = HashMap::new();
        let depth = self.conds.len();
        for (i, prm) in f.params.iter().enumerate() {
            let v = if prm.mods.out && !prm.mods.inn {
                self.zero(&prm.ty)?
            } else {
                let e = args
                    .get(i)
                    .or(prm.default.as_ref())
                    .ok_or("argument manquant")?;
                let (v, t) = self.expr(e)?;
                self.convert(v, &t, &prm.ty)?
            };
            scope.insert(
                prm.name.clone(),
                Var {
                    v,
                    ty: prm.ty.clone(),
                    depth,
                },
            );
        }
        let saved = std::mem::replace(&mut self.scopes, vec![scope]);
        self.call_depth += 1;
        self.frames.push(Frame {
            base: depth,
            ..Frame::default()
        });
        let r = self.block(f.body.as_deref().unwrap_or(&[]));
        let frame = self.frames.pop().unwrap_or_default();
        self.conds.truncate(depth);
        self.call_depth -= 1;
        let fscope = std::mem::replace(&mut self.scopes, saved);
        r?;
        for (i, prm) in f.params.iter().enumerate() {
            if prm.mods.out {
                if let (Some(a), Some(v)) = (args.get(i), fscope[0].get(&prm.name)) {
                    let (vv, tt) = (v.v.clone(), v.ty.clone());
                    self.store(a, vv, &tt)?;
                }
            }
        }
        let ret = match frame.ret {
            Some(v) => v,
            None if f.ret.base == Base::Void => V::Void,
            None => return Err(format!("{} : aucune valeur renvoyée", f.name)),
        };
        Ok((ret, f.ret.clone()))
    }

    fn sincos(&mut self, x: V) -> Result<(V, V), String> {
        if let V::Lit(l) = &x {
            return Ok((fold1(l, f32::sin), fold1(l, f32::cos)));
        }
        let n = x.n();
        let (mut s, mut c) = (Vec::new(), Vec::new());
        for k in 0..n {
            let xi = self.comp(&x, k);
            // Réduction dans [-π, π].
            let t = self.mad(
                xi,
                V::Lit(vec![1.0 / std::f32::consts::TAU]),
                V::Lit(vec![0.5]),
            )?;
            let fr = self.emit("frc", 1, &[t])?;
            let r = self.mad(
                fr,
                V::Lit(vec![std::f32::consts::TAU]),
                V::Lit(vec![-std::f32::consts::PI]),
            )?;
            let rs = self.src(&r)?;
            let d = self.temp();
            let mut srcs = vec![rs];
            if !self.sm3 {
                srcs.push(self.lit_src(&SINCOS1));
                srcs.push(self.lit_src(&SINCOS2));
            }
            self.push("sincos", Some((d, 3)), srcs);
            c.push(V::S(Src::new(d, 1)));
            s.push(V::S(Src::new(d, 2).pick(&[1])));
        }
        Ok((self.gather(&s)?, self.gather(&c)?))
    }

    fn tex(&mut self, op: &str, s: &V, coord: V, extra: &[V], dim: u8) -> Result<V, String> {
        let V::Sampler(k, sd) = s else {
            return Err("échantillonneur attendu".into());
        };
        let d = if *sd == 0 { dim } else { *sd };
        self.samplers.insert(*k, d);
        // Coordonnées lues en entier quand c'est déjà un registre (forme usuelle).
        let coord = match coord {
            V::S(s) if s.swz[..s.n as usize] == [0, 1, 2, 3][..s.n as usize] && !s.neg => {
                V::S(Src::new(s.r, 4))
            }
            c => c,
        };
        let mut srcs = vec![coord];
        srcs.push(V::S(Src::new(R::S(*k), 4)));
        srcs.extend(extra.iter().cloned());
        let op = if op == "texldl" && !self.sm3 {
            crate::log_warn!("d3dx9", "tex*lod en profil 2.0 : niveau de mipmap ignoré");
            "texld"
        } else {
            op
        };
        if !self.pixel && op != "texldl" {
            return Err(
                "échantillonnage dans un vertex shader : utiliser tex2Dlod (vs_3_0)".into(),
            );
        }
        self.emit(op, 4, &srcs)
    }

    fn mul(&mut self, a: (V, Ty), b: (V, Ty)) -> Result<(V, Ty), String> {
        let (av, at) = a;
        let (bv, bt) = b;
        match (&av, &bv) {
            (V::Mat(..), V::Mat(..)) => {
                // Ligne par ligne : ligne_i(A) × B.
                let mut lines = Vec::new();
                for r in 0..at.rows {
                    let parts: Vec<V> = (0..at.cols).map(|c| self.mat_elem(&av, r, c)).collect();
                    let row = self.gather(&parts)?;
                    let (v, _) = self.mul((row, Ty::float(at.cols)), (bv.clone(), bt.clone()))?;
                    lines.push(v);
                }
                let ty = Ty {
                    class: 2,
                    rows: at.rows,
                    cols: bt.cols,
                    ..Ty::scalar(Base::Float)
                };
                Ok((V::Mat(true, lines), ty))
            }
            (_, V::Mat(rows, lines)) => {
                // vecteur × matrice
                let out = bt.cols;
                let v = Self::trunc(av, bt.rows);
                let res = if !*rows {
                    let mut parts = Vec::new();
                    for l in lines.clone() {
                        parts.push(self.dot(v.clone(), l)?);
                    }
                    self.gather(&parts)?
                } else {
                    let mut acc = None;
                    for (i, l) in lines.clone().into_iter().enumerate() {
                        let vi = self.comp(&v, i as u8);
                        acc = Some(match acc {
                            None => self.arith("mul", l, vi)?,
                            Some(a) => self.mad(l, vi, a)?,
                        });
                    }
                    acc.ok_or("matrice vide")?
                };
                Ok((res, Ty::float(out)))
            }
            (V::Mat(rows, lines), _) => {
                let out = at.rows;
                let v = Self::trunc(bv, at.cols);
                let res = if *rows {
                    let mut parts = Vec::new();
                    for l in lines.clone() {
                        parts.push(self.dot(l, v.clone())?);
                    }
                    self.gather(&parts)?
                } else {
                    let mut acc = None;
                    for (j, l) in lines.clone().into_iter().enumerate() {
                        let vj = self.comp(&v, j as u8);
                        acc = Some(match acc {
                            None => self.arith("mul", l, vj)?,
                            Some(a) => self.mad(l, vj, a)?,
                        });
                    }
                    acc.ok_or("matrice vide")?
                };
                Ok((res, Ty::float(out)))
            }
            _ if av.n() > 1 && bv.n() > 1 => Ok((self.dot(av, bv)?, Ty::float(1))),
            _ => {
                let n = Self::size2(&av, &bv);
                Ok((self.arith("mul", av, bv)?, Ty::float(n)))
            }
        }
    }

    fn length(&mut self, v: V) -> Result<V, String> {
        let d = self.dot(v.clone(), v)?;
        self.sqrt(d)
    }

    fn sqrt(&mut self, v: V) -> Result<V, String> {
        if let V::Lit(l) = &v {
            return Ok(fold1(l, |x| x.max(0.0).sqrt()));
        }
        let r = self.emit_scalar("rsq", &[v])?;
        self.emit_scalar("rcp", &[r])
    }

    fn intrinsic(
        &mut self,
        name: &str,
        mut a: Vec<(V, Ty)>,
        raw: &[Expr],
    ) -> Result<(V, Ty), String> {
        let argc = a.len();
        let need = |k: usize| -> Result<(), String> {
            if argc < k {
                Err(format!("{name} : {k} arguments attendus"))
            } else {
                Ok(())
            }
        };
        let t0 = a.first().map(|x| x.1.clone()).unwrap_or(Ty::float(1));
        let fty = |n: u8| Ty::float(n);
        let v = |a: &mut Vec<(V, Ty)>, i: usize| a[i].0.clone();
        macro_rules! lit1 {
            ($f:expr) => {{
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                if let V::Lit(l) = &x {
                    return Ok((fold1(l, $f), fty(n)));
                }
            }};
        }
        Ok(match name {
            "abs" => {
                lit1!(f32::abs);
                let x = v(&mut a, 0);
                let n = x.n();
                (self.emit("abs", n, &[x])?, t0)
            }
            "saturate" => {
                lit1!(|x: f32| x.clamp(0.0, 1.0));
                let x = v(&mut a, 0);
                let n = x.n();
                (self.emit("mov_sat", n, &[x])?, fty(n))
            }
            "min" | "max" => {
                need(2)?;
                let (x, y) = (v(&mut a, 0), v(&mut a, 1));
                let n = Self::size2(&x, &y);
                (self.arith(name, x, y)?, fty(n))
            }
            "clamp" => {
                need(3)?;
                let (x, lo, hi) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                let m = self.arith("max", x, lo)?;
                let n = m.n();
                (self.arith("min", m, hi)?, fty(n))
            }
            "lerp" => {
                need(3)?;
                let (x, y, t) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                if let (V::Lit(_), V::Lit(_), V::Lit(_)) = (&x, &y, &t) {
                    let d = self.arith("sub", y, x.clone())?;
                    let m = self.arith("mul", d, t)?;
                    let n = m.n();
                    return Ok((self.arith("add", x, m)?, fty(n)));
                }
                let n = x.n().max(y.n());
                (self.emit("lrp", n, &[t, y, x])?, fty(n))
            }
            "dot" => {
                need(2)?;
                (self.dot(v(&mut a, 0), v(&mut a, 1))?, fty(1))
            }
            "cross" => {
                need(2)?;
                let (x, y) = (Self::trunc(v(&mut a, 0), 3), Self::trunc(v(&mut a, 1), 3));
                if let (V::Lit(p), V::Lit(q)) = (&x, &y) {
                    let g = |i| lit_get(p, i);
                    let h = |i| lit_get(q, i);
                    return Ok((
                        V::Lit(vec![
                            g(1) * h(2) - g(2) * h(1),
                            g(2) * h(0) - g(0) * h(2),
                            g(0) * h(1) - g(1) * h(0),
                        ]),
                        fty(3),
                    ));
                }
                (self.emit("crs", 3, &[x, y])?, fty(3))
            }
            "normalize" => {
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                if n == 3 && !matches!(x, V::Lit(_)) {
                    (self.emit("nrm", 3, &[x])?, fty(3))
                } else {
                    let d = self.dot(x.clone(), x.clone())?;
                    let r = match d {
                        V::Lit(l) => fold1(&l, |q| if q > 0.0 { 1.0 / q.sqrt() } else { 0.0 }),
                        d => self.emit_scalar("rsq", &[d])?,
                    };
                    (self.arith("mul", x, r)?, fty(n))
                }
            }
            "length" => {
                need(1)?;
                (self.length(v(&mut a, 0))?, fty(1))
            }
            "distance" => {
                need(2)?;
                let d = self.arith("sub", v(&mut a, 0), v(&mut a, 1))?;
                (self.length(d)?, fty(1))
            }
            "sqrt" => {
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                (self.sqrt(x)?, fty(n))
            }
            "rsqrt" => {
                lit1!(|x: f32| 1.0 / x.sqrt());
                let x = v(&mut a, 0);
                let n = x.n();
                (self.emit_scalar("rsq", &[x])?, fty(n))
            }
            "rcp" => {
                lit1!(|x: f32| 1.0 / x);
                let x = v(&mut a, 0);
                let n = x.n();
                (self.emit_scalar("rcp", &[x])?, fty(n))
            }
            "pow" => {
                need(2)?;
                let (x, y) = (v(&mut a, 0), v(&mut a, 1));
                let n = Self::size2(&x, &y);
                if let (V::Lit(p), V::Lit(q)) = (&x, &y) {
                    return Ok((
                        V::Lit(
                            (0..n as usize)
                                .map(|i| lit_get(p, i).powf(lit_get(q, i)))
                                .collect(),
                        ),
                        fty(n),
                    ));
                }
                let (x, y) = (self.broadcast_to(x, n), self.broadcast_to(y, n));
                (self.emit_scalar("pow", &[x, y])?, fty(n))
            }
            "exp2" | "exp" | "log2" | "log" | "log10" => {
                need(1)?;
                let mut x = v(&mut a, 0);
                let n = x.n();
                if let V::Lit(l) = &x {
                    let f: fn(f32) -> f32 = match name {
                        "exp2" => f32::exp2,
                        "exp" => f32::exp,
                        "log2" => f32::log2,
                        "log" => f32::ln,
                        _ => f32::log10,
                    };
                    return Ok((fold1(l, f), fty(n)));
                }
                if name == "exp" {
                    x = self.arith("mul", x, V::Lit(vec![std::f32::consts::LOG2_E]))?;
                }
                let op = if name.starts_with("exp") {
                    "exp"
                } else {
                    "log"
                };
                let r = self.emit_scalar(op, &[x])?;
                let r = match name {
                    "log" => self.arith("mul", r, V::Lit(vec![std::f32::consts::LN_2]))?,
                    "log10" => self.arith("mul", r, V::Lit(vec![std::f32::consts::LOG10_2]))?,
                    _ => r,
                };
                (r, fty(n))
            }
            "frac" => {
                lit1!(|x: f32| x - x.floor());
                let x = v(&mut a, 0);
                let n = x.n();
                (self.emit("frc", n, &[x])?, fty(n))
            }
            "floor" | "ceil" | "round" => {
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                if let V::Lit(l) = &x {
                    let f: fn(f32) -> f32 = match name {
                        "floor" => f32::floor,
                        "ceil" => f32::ceil,
                        _ => |x: f32| (x + 0.5).floor(),
                    };
                    return Ok((fold1(l, f), fty(n)));
                }
                let x = match name {
                    "ceil" => self.neg(x),
                    "round" => self.arith("add", x, V::Lit(vec![0.5]))?,
                    _ => x,
                };
                let fr = self.emit("frc", n, std::slice::from_ref(&x))?;
                let fl = self.arith("sub", x, fr)?;
                (if name == "ceil" { self.neg(fl) } else { fl }, fty(n))
            }
            "fmod" => {
                need(2)?;
                let (x, y) = (v(&mut a, 0), v(&mut a, 1));
                let n = Self::size2(&x, &y);
                (self.fmod(x, y)?, fty(n))
            }
            "sign" => {
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                let p = self.compare(">", x.clone(), V::Lit(vec![0.0]))?;
                let q = self.compare("<", x, V::Lit(vec![0.0]))?;
                (self.arith("sub", p, q)?, fty(n))
            }
            "step" => {
                need(2)?;
                let (e, x) = (v(&mut a, 0), v(&mut a, 1));
                let n = Self::size2(&e, &x);
                (self.compare(">=", x, e)?, fty(n))
            }
            "smoothstep" => {
                need(3)?;
                let (lo, hi, x) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                let d = self.arith("sub", x, lo.clone())?;
                let w = self.arith("sub", hi, lo)?;
                let q = self.arith("div", d, w)?;
                let n = q.n();
                let t = match q {
                    V::Lit(l) => fold1(&l, |x| x.clamp(0.0, 1.0)),
                    q => self.emit("mov_sat", n, &[q])?,
                };
                let k = self.mad(t.clone(), V::Lit(vec![-2.0]), V::Lit(vec![3.0]))?;
                let t2 = self.arith("mul", t.clone(), t)?;
                (self.arith("mul", t2, k)?, fty(n))
            }
            "sin" | "cos" | "tan" => {
                need(1)?;
                let x = v(&mut a, 0);
                let n = x.n();
                let (s, c) = self.sincos(x)?;
                let r = match name {
                    "sin" => s,
                    "cos" => c,
                    _ => self.arith("div", s, c)?,
                };
                (r, fty(n))
            }
            "sincos" => {
                need(3)?;
                let x = v(&mut a, 0);
                let (s, c) = self.sincos(x)?;
                let t = a[1].1.clone();
                self.store(&raw[1], s, &t)?;
                self.store(&raw[2], c, &t)?;
                (V::Void, Ty::scalar(Base::Void))
            }
            "radians" | "degrees" => {
                need(1)?;
                let k = if name == "radians" {
                    std::f32::consts::PI / 180.0
                } else {
                    180.0 / std::f32::consts::PI
                };
                let x = v(&mut a, 0);
                let n = x.n();
                (self.arith("mul", x, V::Lit(vec![k]))?, fty(n))
            }
            "reflect" => {
                need(2)?;
                let (i, nrm) = (v(&mut a, 0), v(&mut a, 1));
                let n = i.n();
                let d = self.dot(nrm.clone(), i.clone())?;
                let d2 = self.arith("mul", d, V::Lit(vec![-2.0]))?;
                (self.mad(nrm, d2, i)?, fty(n))
            }
            "refract" => {
                need(3)?;
                let (i, nrm, eta) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                let n = i.n();
                let d = self.dot(nrm.clone(), i.clone())?;
                let d2 = self.arith("mul", d.clone(), d.clone())?;
                let one_m = self.not(d2)?;
                let e2 = self.arith("mul", eta.clone(), eta.clone())?;
                let ne2 = self.neg(e2);
                let k = self.mad(ne2, one_m, V::Lit(vec![1.0]))?;
                let sk = self.sqrt(k.clone())?;
                let ed = self.mad(eta.clone(), d, sk)?;
                let ei = self.arith("mul", eta, i)?;
                let edn = self.arith("mul", ed, nrm)?;
                let r = self.arith("sub", ei, edn)?;
                let ok = self.compare(">=", k, V::Lit(vec![0.0]))?;
                let z = V::Lit(vec![0.0; n as usize]);
                (self.select(&ok, r, z)?, fty(n))
            }
            "faceforward" => {
                need(3)?;
                let (nrm, i, ng) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                let n = nrm.n();
                let d = self.dot(i, ng)?;
                let m = self.compare("<", d, V::Lit(vec![0.0]))?;
                let nn = self.neg(nrm.clone());
                (self.select(&m, nrm, nn)?, fty(n))
            }
            "lit" => {
                need(3)?;
                let (l, h, m) = (v(&mut a, 0), v(&mut a, 1), v(&mut a, 2));
                let diff = self.arith("max", l.clone(), V::Lit(vec![0.0]))?;
                let hp = self.arith("max", h.clone(), V::Lit(vec![1e-6]))?;
                let p = self.emit_scalar("pow", &[hp, m])?;
                let c1 = self.compare(">", l, V::Lit(vec![0.0]))?;
                let c2 = self.compare(">", h, V::Lit(vec![0.0]))?;
                let c = self.arith("mul", c1, c2)?;
                let spec = self.arith("mul", p, c)?;
                let parts = [V::Lit(vec![1.0]), diff, spec, V::Lit(vec![1.0])];
                (self.gather(&parts)?, fty(4))
            }
            "any" | "all" => {
                need(1)?;
                let x = v(&mut a, 0);
                let m = self.compare("!=", x, V::Lit(vec![0.0]))?;
                let mut acc = self.comp(&m, 0);
                for c in 1..m.n() {
                    let mc = self.comp(&m, c);
                    acc = self.arith(if name == "any" { "max" } else { "mul" }, acc, mc)?;
                }
                (acc, Ty::scalar(Base::Bool))
            }
            "mul" => {
                need(2)?;
                let b = a.pop().ok_or("mul")?;
                let x = a.pop().ok_or("mul")?;
                self.mul(x, b)?
            }
            "transpose" => {
                need(1)?;
                match v(&mut a, 0) {
                    V::Mat(r, l) => (
                        V::Mat(!r, l),
                        Ty {
                            rows: t0.cols,
                            cols: t0.rows,
                            ..t0
                        },
                    ),
                    _ => return Err("transpose : matrice attendue".into()),
                }
            }
            "ddx" | "ddy" => {
                need(1)?;
                if !self.pixel {
                    return Err(format!("{name} n'existe que dans un pixel shader"));
                }
                let x = v(&mut a, 0);
                let n = x.n();
                (
                    self.emit(if name == "ddx" { "dsx" } else { "dsy" }, n, &[x])?,
                    fty(n),
                )
            }
            "clip" => {
                need(1)?;
                let x = v(&mut a, 0);
                let x = match self.cur_mask() {
                    Some(m) => {
                        let n = x.n();
                        self.select(&m, x, V::Lit(vec![0.0; n as usize]))?
                    }
                    None => x,
                };
                self.kill(x)?;
                (V::Void, Ty::scalar(Base::Void))
            }
            "tex1D" | "tex2D" | "tex3D" | "texCUBE" | "tex1Dproj" | "tex2Dproj" | "tex3Dproj"
            | "texCUBEproj" | "tex1Dbias" | "tex2Dbias" | "tex3Dbias" | "texCUBEbias"
            | "tex1Dlod" | "tex2Dlod" | "tex3Dlod" | "texCUBElod" | "tex1Dgrad" | "tex2Dgrad"
            | "tex3Dgrad" | "texCUBEgrad" => {
                need(2)?;
                let dim = if name.starts_with("texCUBE") {
                    4
                } else if name.starts_with("tex3D") {
                    3
                } else {
                    2
                };
                let s = v(&mut a, 0);
                let mut c = v(&mut a, 1);
                let suffix = name
                    .trim_start_matches("texCUBE")
                    .trim_start_matches("tex1D");
                let suffix = suffix
                    .trim_start_matches("tex2D")
                    .trim_start_matches("tex3D");
                if name.starts_with("tex1D") && c.n() == 1 && suffix.is_empty() {
                    c = self.gather(&[c, V::Lit(vec![0.5])])?;
                }
                let r = match suffix {
                    "proj" => self.tex("texldp", &s, c, &[], dim)?,
                    "bias" => self.tex("texldb", &s, c, &[], dim)?,
                    "lod" => self.tex("texldl", &s, c, &[], dim)?,
                    "grad" => {
                        need(4)?;
                        let (gx, gy) = (v(&mut a, 2), v(&mut a, 3));
                        self.tex("texldd", &s, c, &[gx, gy], dim)?
                    }
                    _ => self.tex("texld", &s, c, &[], dim)?,
                };
                (r, fty(4))
            }
            _ => return Err(format!("fonction inconnue ou non prise en charge : {name}")),
        })
    }

    /// texkill sur un temporaire (les composantes x, y, z sont testées).
    fn kill(&mut self, x: V) -> Result<(), String> {
        let n = x.n();
        let s = self.src(&x)?;
        let t = self.temp();
        let s = if n < 3 { s.pick(&[0, 0, 0]) } else { s };
        self.push("mov", Some((t, 15)), vec![Src { n: 4, ..s }]);
        self.push("texkill", Some((t, 15)), vec![]);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Génération : instructions de contrôle
// ═══════════════════════════════════════════════════════════════════════

impl<'p> Gen<'p> {
    fn frame(&mut self) -> &mut Frame {
        if self.frames.is_empty() {
            self.frames.push(Frame::default());
        }
        let n = self.frames.len() - 1;
        &mut self.frames[n]
    }

    fn block(&mut self, stmts: &[Stmt]) -> Result<Flow, String> {
        self.scopes.push(HashMap::new());
        self.block_pushes.push(0);
        let mut flow = Flow::Next;
        for s in stmts {
            if self.frame().done {
                break;
            }
            flow = self.stmt(s)?;
            if flow != Flow::Next {
                break;
            }
        }
        let pushes = self.block_pushes.pop().unwrap_or(0);
        let len = self.conds.len();
        self.conds.truncate(len - pushes.min(len));
        self.scopes.pop();
        Ok(flow)
    }

    fn stmt(&mut self, s: &Stmt) -> Result<Flow, String> {
        match s {
            Stmt::Empty => {}
            Stmt::Block(b) => return self.block(b),
            Stmt::Expr(e) => {
                self.expr(e)?;
            }
            Stmt::Decl(ds) => {
                for d in ds {
                    let v = match &d.init {
                        Some(e) => self.init_value(e, &d.ty)?,
                        None => self.zero(&d.ty)?,
                    };
                    self.declare(&d.name, v, d.ty.clone());
                }
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.expr(e)?.0,
                    None => V::Void,
                };
                let mask = self.cur_mask();
                let base = self.frame().base;
                let masked = self.conds.len() > base;
                let old = self.frame().ret.take();
                let new = match (old, masked, &mask) {
                    (Some(o), true, Some(m)) => self.select_deep(m, v, o)?,
                    _ => v,
                };
                let f = self.frame();
                f.ret = Some(new);
                if f.if_depth == 0 {
                    f.done = true;
                } else if let Some(m) = mask {
                    let acc = f.retmask.take();
                    let r = match acc {
                        Some(a) => self.arith("max", a, m)?,
                        None => m,
                    };
                    self.frame().retmask = Some(r);
                }
            }
            Stmt::Discard => {
                let v = match self.cur_mask() {
                    Some(m) => self.neg(m),
                    None => V::Lit(vec![-1.0]),
                };
                self.kill(v)?;
            }
            Stmt::Break | Stmt::Continue => {
                if self.frame().if_depth > 0 && self.cur_mask().is_some() {
                    return Err("break/continue sous condition non pris en charge".into());
                }
                return Ok(if matches!(s, Stmt::Break) {
                    Flow::Break
                } else {
                    Flow::Continue
                });
            }
            Stmt::If(c, a, b) => {
                let (cv, ct) = self.expr(c)?;
                let m = self.truth(cv, &ct)?;
                let m = self.comp(&m, 0);
                if let V::Lit(l) = &m {
                    if lit_get(l, 0) != 0.0 {
                        return self.stmt(a);
                    }
                    return match b {
                        Some(b) => self.stmt(b),
                        None => Ok(Flow::Next),
                    };
                }
                let saved = {
                    let f = self.frame();
                    f.if_depth += 1;
                    f.retmask.take()
                };
                self.push_mask(m.clone())?;
                let fa = self.block(std::slice::from_ref(a));
                self.conds.pop();
                fa?;
                if let Some(b) = b {
                    let nm = self.not(m)?;
                    self.push_mask(nm)?;
                    let fb = self.block(std::slice::from_ref(b));
                    self.conds.pop();
                    fb?;
                }
                let r = {
                    let f = self.frame();
                    f.if_depth -= 1;
                    f.retmask.take()
                };
                let merged = match (saved, r.clone()) {
                    (Some(x), Some(y)) => Some(self.arith("max", x, y)?),
                    (x, y) => x.or(y),
                };
                self.frame().retmask = merged;
                if let Some(r) = r {
                    let alive = self.not(r)?;
                    self.push_mask(alive)?;
                    if let Some(p) = self.block_pushes.last_mut() {
                        *p += 1;
                    }
                }
            }
            Stmt::For(init, cond, step, body) => {
                self.scopes.push(HashMap::new());
                if let Some(i) = init {
                    self.stmt(i)?;
                }
                let r = self.unroll(cond.as_ref(), step.as_ref(), body, false);
                self.scopes.pop();
                r?;
            }
            Stmt::While(c, body) => self.unroll(Some(c), None, body, false)?,
            Stmt::Do(body, c) => self.unroll(Some(c), None, body, true)?,
        }
        Ok(Flow::Next)
    }

    fn unroll(
        &mut self,
        cond: Option<&Expr>,
        step: Option<&Expr>,
        body: &Stmt,
        first: bool,
    ) -> Result<(), String> {
        let mut skip_test = first;
        for _ in 0..1024 {
            if !skip_test {
                if let Some(c) = cond {
                    let (v, t) = self.expr(c)?;
                    match self.truth(v, &t)? {
                        V::Lit(l) if lit_get(&l, 0) == 0.0 => return Ok(()),
                        V::Lit(_) => {}
                        _ => return Err("boucle à condition dynamique non prise en charge (déroulage impossible)".into()),
                    }
                }
            }
            skip_test = false;
            let flow = self.block(std::slice::from_ref(body))?;
            if flow == Flow::Break || self.frame().done {
                return Ok(());
            }
            if let Some(s) = step {
                self.expr(s)?;
            }
        }
        Err("boucle de plus de 1024 tours".into())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Point d'entrée, constantes, attribution des registres, texte
// ═══════════════════════════════════════════════════════════════════════

fn walk_expr(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Ident(n) => out.push(n.clone()),
        Expr::Unary(_, x) | Expr::Post(_, x) | Expr::Cast(_, x) | Expr::Member(x, _) => {
            walk_expr(x, out)
        }
        Expr::Bin(_, a, b) | Expr::Assign(_, a, b) | Expr::Index(a, b) => {
            walk_expr(a, out);
            walk_expr(b, out);
        }
        Expr::Cond(a, b, c) => {
            walk_expr(a, out);
            walk_expr(b, out);
            walk_expr(c, out);
        }
        Expr::Call(n, args) => {
            out.push(n.clone());
            args.iter().for_each(|a| walk_expr(a, out));
        }
        Expr::Ctor(_, args) | Expr::List(args) | Expr::Compile(_, _, args) => {
            args.iter().for_each(|a| walk_expr(a, out))
        }
        _ => {}
    }
}

fn walk_stmt(s: &Stmt, out: &mut Vec<String>) {
    match s {
        Stmt::Decl(ds) => ds
            .iter()
            .filter_map(|d| d.init.as_ref())
            .for_each(|e| walk_expr(e, out)),
        Stmt::Expr(e) | Stmt::Return(Some(e)) => walk_expr(e, out),
        Stmt::If(c, a, b) => {
            walk_expr(c, out);
            walk_stmt(a, out);
            if let Some(b) = b {
                walk_stmt(b, out);
            }
        }
        Stmt::For(i, c, st, b) => {
            if let Some(i) = i {
                walk_stmt(i, out);
            }
            c.iter().chain(st.iter()).for_each(|e| walk_expr(e, out));
            walk_stmt(b, out);
        }
        Stmt::While(c, b) | Stmt::Do(b, c) => {
            walk_expr(c, out);
            walk_stmt(b, out);
        }
        Stmt::Block(b) => b.iter().for_each(|s| walk_stmt(s, out)),
        _ => {}
    }
}

/// Identifiants utilisés depuis une fonction (appels suivis).
fn used_names(p: &Program, entry: &str) -> std::collections::HashSet<String> {
    let mut seen = std::collections::HashSet::new();
    let mut todo = vec![entry.to_string()];
    let mut funcs_done = std::collections::HashSet::new();
    while let Some(f) = todo.pop() {
        if !funcs_done.insert(f.clone()) {
            continue;
        }
        for func in p.funcs.iter().filter(|x| x.name == f) {
            let mut v = Vec::new();
            for prm in &func.params {
                if let Some(d) = &prm.default {
                    walk_expr(d, &mut v);
                }
            }
            for s in func.body.iter().flatten() {
                walk_stmt(s, &mut v);
            }
            for n in v {
                if p.funcs.iter().any(|x| x.name == n) {
                    todo.push(n.clone());
                }
                // Initialiseurs des statiques utilisées.
                if let Some(g) = p.globals.iter().find(|g| g.name == n && g.mods.stat) {
                    if let Some(e) = &g.init {
                        let mut w = Vec::new();
                        walk_expr(e, &mut w);
                        for x in w {
                            if p.funcs.iter().any(|f| f.name == x) {
                                todo.push(x.clone());
                            }
                            seen.insert(x);
                        }
                    }
                }
                seen.insert(n);
            }
        }
    }
    seen
}

/// Résultat d'une compilation.
#[derive(Clone, Debug, Default)]
pub(crate) struct Compiled {
    pub tokens: Vec<u32>,
    /// Assembleur produit (diagnostic).
    #[allow(dead_code)]
    pub asm: String,
    pub ctab: Vec<CtabEntry>,
}

/// Valeur littérale « à plat » (pour les valeurs par défaut).
fn lit_flat(v: &V) -> Option<Vec<Vec<f32>>> {
    match v {
        V::Lit(l) => Some(vec![l.clone()]),
        V::Mat(_, lines) => {
            let mut out = Vec::new();
            for l in lines {
                out.extend(lit_flat(l)?);
            }
            Some(out)
        }
        V::Arr(items) => {
            let mut out = Vec::new();
            for i in items {
                out.extend(lit_flat(i)?);
            }
            Some(out)
        }
        _ => None,
    }
}

impl<'p> Gen<'p> {
    fn regs_per_elem(&self, ty: &Ty) -> u32 {
        if ty.class == 2 {
            if ty.row_major.unwrap_or(self.row_major) {
                ty.rows as u32
            } else {
                ty.cols as u32
            }
        } else {
            1
        }
    }

    fn uniform_value(&self, first: u32, ty: &Ty) -> V {
        if ty.array.is_some() {
            return V::UArr(first, self.regs_per_elem(ty), ty.clone());
        }
        if ty.class == 2 {
            let rows = ty.row_major.unwrap_or(self.row_major);
            let (nl, nc) = if rows {
                (ty.rows, ty.cols)
            } else {
                (ty.cols, ty.rows)
            };
            return V::Mat(
                rows,
                (0..nl as u32)
                    .map(|k| V::S(Src::new(R::C(first + k), nc)))
                    .collect(),
            );
        }
        V::S(Src::new(R::C(first), ty.cols))
    }

    /// Place les uniformes utilisés (registres explicites respectés).
    fn alloc_uniforms(&mut self, extra: &[(String, Ty)]) -> Result<(), String> {
        let mut taken_c: Vec<(u32, u32)> = Vec::new();
        let mut taken_s: Vec<u32> = Vec::new();
        let mut list: Vec<(String, Ty, Option<(char, u32)>, Option<Expr>)> = Vec::new();
        for g in &self.p.globals {
            if g.mods.stat || !self.used.contains(&g.name) {
                continue;
            }
            list.push((g.name.clone(), g.ty.clone(), g.register, g.init.clone()));
        }
        for (n, t) in extra {
            list.push((n.clone(), t.clone(), None, None));
        }
        // Registres explicites d'abord.
        for (_, ty, reg, _) in &list {
            if let Some((c, n)) = reg {
                if *c == 's' {
                    taken_s.push(*n);
                } else {
                    let count = self.regs_per_elem(ty) * ty.array.unwrap_or(1).max(1);
                    taken_c.push((*n, count));
                }
            }
        }
        for (name, ty, reg, init) in list {
            match &ty.base {
                Base::Sampler(d) => {
                    let s = match reg {
                        Some(('s', n)) => n,
                        _ => {
                            let mut k = 0;
                            while taken_s.contains(&k) {
                                k += 1;
                            }
                            taken_s.push(k);
                            k
                        }
                    };
                    self.globals
                        .insert(name.clone(), (V::Sampler(s, *d), ty.clone()));
                    self.ctab.push(CtabEntry {
                        name,
                        set: 3,
                        index: s as u16,
                        count: 1,
                        class: 4,
                        ty: type_code(&ty),
                        rows: 1,
                        cols: 1,
                        elements: 1,
                        default: None,
                    });
                }
                Base::Float | Base::Int | Base::Bool => {
                    let elems = ty.array.unwrap_or(1).max(1);
                    let count = self.regs_per_elem(&ty) * elems;
                    let first = match reg {
                        Some(('c', n)) => n,
                        _ => {
                            let mut k = 0u32;
                            while taken_c.iter().any(|&(a, c)| k < a + c && a < k + count) {
                                k += 1;
                            }
                            taken_c.push((k, count));
                            k
                        }
                    };
                    let v = self.uniform_value(first, &ty);
                    let default = match &init {
                        Some(e) => {
                            let saved = self.code.len();
                            let d = self.init_value(e, &ty).ok().and_then(|v| lit_flat(&v));
                            self.code.truncate(saved);
                            d.map(|rows| self.default_layout(&rows, &ty))
                        }
                        None => None,
                    };
                    let rows_major = ty.row_major.unwrap_or(self.row_major);
                    self.globals.insert(name.clone(), (v, ty.clone()));
                    self.ctab.push(CtabEntry {
                        name,
                        set: 2,
                        index: first as u16,
                        count: count as u16,
                        class: match ty.class {
                            0 => 0,
                            1 => 1,
                            _ if rows_major => 2,
                            _ => 3,
                        },
                        ty: type_code(&ty),
                        rows: ty.rows as u16,
                        cols: ty.cols as u16,
                        elements: elems as u16,
                        default,
                    });
                }
                Base::Struct(_) => {
                    // Structure uniforme (WPF en génère) : chaque champ
                    // commence sur un registre, dans l'ordre de déclaration.
                    if ty.array.is_some() {
                        return Err(format!("{name} : tableau de structures uniforme"));
                    }
                    let count = self.struct_regs(&ty)?;
                    let first = match reg {
                        Some(('c', n)) => n,
                        _ => {
                            let mut k = 0u32;
                            while taken_c.iter().any(|&(a, c)| k < a + c && a < k + count) {
                                k += 1;
                            }
                            taken_c.push((k, count));
                            k
                        }
                    };
                    let v = self.struct_uniform(&name, first, &ty)?;
                    self.globals.insert(name, (v, ty.clone()));
                }
                _ => {
                    let v = match &init {
                        Some(Expr::Str(s)) => V::Str(s.clone()),
                        _ => V::Void,
                    };
                    self.globals.insert(name, (v, ty));
                }
            }
        }
        Ok(())
    }

    /// Registres c occupés par une structure uniforme.
    fn struct_regs(&self, ty: &Ty) -> Result<u32, String> {
        let elems = ty.array.unwrap_or(1).max(1);
        let one = match &ty.base {
            Base::Struct(_) => {
                let mut n = 0;
                for f in self.fields_of(ty)? {
                    n += self.struct_regs(&f.ty)?;
                }
                n
            }
            Base::Float | Base::Int | Base::Bool => self.regs_per_elem(ty),
            _ => return Err("champ de structure uniforme non numérique".into()),
        };
        Ok(one * elems)
    }

    /// Valeur d'une structure uniforme placée à partir de c`first` ; chaque
    /// champ numérique reçoit son entrée dans la table des constantes.
    fn struct_uniform(&mut self, path: &str, first: u32, ty: &Ty) -> Result<V, String> {
        let mut at = first;
        let mut out = Vec::new();
        for f in self.fields_of(ty)? {
            let full = format!("{path}.{}", f.name);
            let n = self.struct_regs(&f.ty)?;
            let v = if let Base::Struct(_) = f.ty.base {
                if f.ty.array.is_some() {
                    return Err(format!("{full} : tableau de structures uniforme"));
                }
                self.struct_uniform(&full, at, &f.ty)?
            } else {
                let rows_major = f.ty.row_major.unwrap_or(self.row_major);
                self.ctab.push(CtabEntry {
                    name: full,
                    set: 2,
                    index: at as u16,
                    count: n as u16,
                    class: match f.ty.class {
                        0 => 0,
                        1 => 1,
                        _ if rows_major => 2,
                        _ => 3,
                    },
                    ty: type_code(&f.ty),
                    rows: f.ty.rows as u16,
                    cols: f.ty.cols as u16,
                    elements: f.ty.array.unwrap_or(1).max(1) as u16,
                    default: None,
                });
                self.uniform_value(at, &f.ty)
            };
            out.push((f.name.clone(), v));
            at += n;
        }
        Ok(V::Struct(out))
    }

    /// Valeur par défaut rangée comme dans les registres (4 flottants chacun).
    fn default_layout(&self, rows: &[Vec<f32>], ty: &Ty) -> Vec<f32> {
        let mut out = Vec::new();
        let elems = ty.array.unwrap_or(1).max(1) as usize;
        let per = if ty.class == 2 { ty.rows as usize } else { 1 };
        let colmajor = ty.class == 2 && !ty.row_major.unwrap_or(self.row_major);
        for e in 0..elems {
            let m = &rows[(e * per).min(rows.len())..((e + 1) * per).min(rows.len())];
            if colmajor {
                for c in 0..ty.cols as usize {
                    let mut r4 = [0f32; 4];
                    for (r, row) in m.iter().enumerate().take(4) {
                        r4[r] = lit_get(row, c);
                    }
                    out.extend(r4);
                }
            } else {
                for row in m {
                    let mut r4 = [0f32; 4];
                    for (k, x) in row.iter().take(4).enumerate() {
                        r4[k] = *x;
                    }
                    out.extend(r4);
                }
            }
        }
        out
    }

    fn named(&mut self, s: String) -> R {
        if let Some(k) = self.named.iter().position(|x| *x == s) {
            return R::N(k as u32);
        }
        self.named.push(s);
        R::N(self.named.len() as u32 - 1)
    }

    fn dcl(&mut self, s: String) {
        if !self.dcls.contains(&s) {
            self.dcls.push(s);
        }
    }

    fn input(&mut self, ty: &Ty, sem: Option<&str>) -> Result<V, String> {
        if let Base::Struct(_) = ty.base {
            let mut v = Vec::new();
            for f in self.fields_of(ty)? {
                v.push((f.name.clone(), self.input(&f.ty, f.semantic.as_deref())?));
            }
            return Ok(V::Struct(v));
        }
        let sem = sem.ok_or("paramètre d'entrée sans sémantique")?;
        if ty.class == 2 || ty.array.is_some() {
            return Err(format!("{sem} : entrée matricielle non prise en charge"));
        }
        let (usage, idx) = usage_of(sem);
        if self.pixel && usage == "POSITION" {
            // Sortie de vertex shader reprise telle quelle en entrée de
            // pixel shader (WPF le fait) : tolérée tant qu'elle n'est pas lue.
            return Ok(V::Void);
        }
        let n = ty.cols;
        let low = usage.to_ascii_lowercase();
        let num = if idx == 0 {
            String::new()
        } else {
            idx.to_string()
        };
        let r = if !self.pixel {
            let k = self.next_input;
            self.next_input += 1;
            self.dcl(format!("dcl_{low}{num} v{k}"));
            R::V(k)
        } else if self.sm3 {
            match usage.as_str() {
                "VPOS" => {
                    self.dcl("dcl vPos.xy".into());
                    self.named("vPos".into())
                }
                "VFACE" => {
                    self.dcl("dcl vFace".into());
                    self.named("vFace".into())
                }
                _ => {
                    let k = self.next_input;
                    self.next_input += 1;
                    self.dcl(format!("dcl_{low}{num} v{k}"));
                    R::V(k)
                }
            }
        } else {
            match usage.as_str() {
                "COLOR" => {
                    self.dcl(format!("dcl v{idx}"));
                    R::V(idx)
                }
                "TEXCOORD" => {
                    self.dcl(format!("dcl t{idx}"));
                    R::Tx(idx)
                }
                _ => {
                    return Err(format!(
                        "{sem} : entrée de pixel shader 2.0 non prise en charge"
                    ))
                }
            }
        };
        let v = V::S(Src::new(r, n));
        Ok(v)
    }

    fn output(&mut self, v: V, ty: &Ty, sem: Option<&str>) -> Result<(), String> {
        if let Base::Struct(_) = ty.base {
            let V::Struct(fs) = v else {
                return Err("structure de sortie attendue".into());
            };
            for f in self.fields_of(ty)? {
                let fv = fs
                    .iter()
                    .find(|(n, _)| *n == f.name)
                    .map(|(_, v)| v.clone());
                if let Some(fv) = fv {
                    self.output(fv, &f.ty, f.semantic.as_deref())?;
                }
            }
            return Ok(());
        }
        let sem = sem.ok_or("valeur de sortie sans sémantique")?;
        let (usage, idx) = usage_of(sem);
        let n = ty.cols.clamp(1, 4);
        let (r, scalar) = if self.pixel {
            match usage.as_str() {
                "COLOR" => (self.named(format!("oC{idx}")), false),
                "DEPTH" => (self.named("oDepth".into()), true),
                _ => return Err(format!("{sem} : sortie de pixel shader inconnue")),
            }
        } else if self.sm3 {
            let k = self.next_output;
            self.next_output += 1;
            let low = usage.to_ascii_lowercase();
            let num = if idx == 0 {
                String::new()
            } else {
                idx.to_string()
            };
            let mask = if usage == "POSITION" || n == 4 {
                String::new()
            } else {
                format!(".{}", SWZ[..n as usize].iter().collect::<String>())
            };
            self.dcl(format!("dcl_{low}{num} o{k}{mask}"));
            (self.named(format!("o{k}")), false)
        } else {
            match usage.as_str() {
                "POSITION" => (self.named("oPos".into()), false),
                "COLOR" => (self.named(format!("oD{idx}")), false),
                "TEXCOORD" => (self.named(format!("oT{idx}")), false),
                "FOG" => (self.named("oFog".into()), true),
                "PSIZE" => (self.named("oPts".into()), true),
                _ => {
                    return Err(format!(
                        "{sem} : sortie de vertex shader 2.0 inconnue (utiliser TEXCOORDn)"
                    ))
                }
            }
        };
        let v = match v {
            V::Mat(..) => return Err("sortie matricielle".into()),
            v => v,
        };
        let s = self.src(&v)?;
        if scalar {
            self.push("mov", Some((r, 0)), vec![s.pick(&[0])]);
        } else {
            let s = if s.n < n { Src { n, ..s }.fixed() } else { s };
            self.push("mov", Some((r, (1u8 << n) - 1)), vec![s]);
        }
        Ok(())
    }

    fn entry(&mut self, f: &'p Function, args: &[(V, Ty)]) -> Result<(), String> {
        // Statiques globales utilisées, dans l'ordre de déclaration.
        self.scopes.push(HashMap::new());
        let statics: Vec<&VarDecl> = self
            .p
            .globals
            .iter()
            .filter(|g| g.mods.stat && self.used.contains(&g.name))
            .collect();
        for g in statics {
            let v = match &g.init {
                Some(e) => self.init_value(e, &g.ty)?,
                None => self.zero(&g.ty)?,
            };
            self.globals.insert(g.name.clone(), (v, g.ty.clone()));
        }
        let mut scope = HashMap::new();
        let mut uarg = args.iter();
        let mut outs = Vec::new();
        for prm in &f.params {
            let v = if prm.mods.uniform {
                match uarg.next() {
                    Some((v, t)) => self.convert(v.clone(), t, &prm.ty)?,
                    None => self
                        .globals
                        .get(&prm.name)
                        .map(|x| x.0.clone())
                        .ok_or_else(|| format!("{} : paramètre uniforme sans valeur", prm.name))?,
                }
            } else if prm.mods.out && !prm.mods.inn {
                outs.push(prm);
                self.zero(&prm.ty)?
            } else {
                if prm.mods.out {
                    outs.push(prm);
                }
                self.input(&prm.ty, prm.semantic.as_deref())?
            };
            scope.insert(
                prm.name.clone(),
                Var {
                    v,
                    ty: prm.ty.clone(),
                    depth: 0,
                },
            );
        }
        self.scopes.push(scope);
        self.frames.push(Frame::default());
        self.block(f.body.as_deref().unwrap_or(&[]))?;
        let frame = self.frames.pop().unwrap_or_default();
        let params = self.scopes.pop().unwrap_or_default();
        if f.ret.base != Base::Void {
            let r = frame.ret.ok_or("le point d'entrée ne renvoie rien")?;
            self.output(r, &f.ret, f.semantic.as_deref())?;
        }
        for prm in outs {
            if let Some(v) = params.get(&prm.name) {
                let vv = v.v.clone();
                self.output(vv, &prm.ty, prm.semantic.as_deref())?;
            }
        }
        Ok(())
    }

    /// Attribue les registres physiques (code rectiligne : balayage linéaire).
    fn allocate(&self) -> Result<(Vec<u32>, u32), String> {
        let n = self.ntemps as usize;
        let mut first = vec![usize::MAX; n];
        let mut last = vec![0usize; n];
        for (i, ins) in self.code.iter().enumerate() {
            if let Some((R::T(t), _)) = ins.dst {
                let t = t as usize;
                first[t] = first[t].min(i);
                last[t] = last[t].max(i);
            }
            for s in &ins.srcs {
                if let R::T(t) = s.r {
                    let t = t as usize;
                    first[t] = first[t].min(i);
                    last[t] = last[t].max(i);
                }
            }
        }
        let mut phys = vec![0u32; n];
        let mut free: Vec<u32> = Vec::new();
        let mut next = 0u32;
        let mut ends: Vec<Vec<usize>> = vec![Vec::new(); self.code.len()];
        let mut starts: Vec<Vec<usize>> = vec![Vec::new(); self.code.len()];
        for t in 0..n {
            if first[t] != usize::MAX {
                starts[first[t]].push(t);
                ends[last[t]].push(t);
            }
        }
        for i in 0..self.code.len() {
            for &t in &starts[i] {
                phys[t] = match free.pop() {
                    Some(r) => r,
                    None => {
                        next += 1;
                        next - 1
                    }
                };
            }
            for &t in &ends[i] {
                free.push(phys[t]);
                free.sort_unstable_by(|a, b| b.cmp(a));
            }
        }
        let limit = if self.sm3 { 32 } else { 12 };
        if next > limit {
            crate::log_warn!(
                "d3dx9",
                "HLSL : {next} registres temporaires (limite du profil : {limit})"
            );
        }
        Ok((phys, next))
    }

    fn reg_text(&self, r: R, phys: &[u32], lit_base: u32) -> String {
        match r {
            R::T(t) => format!("r{}", phys[t as usize]),
            R::C(k) => format!("c{k}"),
            R::CRel(k) => format!("c[a0.x + {k}]"),
            R::Lit(k) => format!("c{}", lit_base + k),
            R::V(k) => format!("v{k}"),
            R::Tx(k) => format!("t{k}"),
            R::S(k) => format!("s{k}"),
            R::A => "a0".into(),
            R::N(k) => self.named[k as usize].clone(),
        }
    }

    fn text(&self, profile: &str) -> Result<String, String> {
        let (phys, _) = self.allocate()?;
        let lit_base = self
            .ctab
            .iter()
            .filter(|c| c.set == 2)
            .map(|c| c.index as u32 + c.count as u32)
            .max()
            .unwrap_or(0);
        let mut out = format!("{profile}\n");
        for (k, l) in self.lits.iter().enumerate() {
            out.push_str(&format!(
                "def c{}, {:?}, {:?}, {:?}, {:?}\n",
                lit_base + k as u32,
                l[0],
                l[1],
                l[2],
                l[3]
            ));
        }
        for d in &self.dcls {
            out.push_str(d);
            out.push('\n');
        }
        let mut samplers: Vec<_> = self.samplers.iter().collect();
        samplers.sort();
        for (s, d) in samplers {
            let kind = match d {
                4 => "cube",
                3 => "volume",
                _ => "2d",
            };
            out.push_str(&format!("dcl_{kind} s{s}\n"));
        }
        for ins in &self.code {
            out.push_str(&ins.op);
            let mut first = true;
            if let Some((r, mask)) = ins.dst {
                out.push(' ');
                out.push_str(&self.reg_text(r, &phys, lit_base));
                if mask != 0 && mask != 15 {
                    out.push('.');
                    for (c, ch) in SWZ.iter().enumerate() {
                        if mask & (1 << c) != 0 {
                            out.push(*ch);
                        }
                    }
                }
                first = false;
            }
            for s in &ins.srcs {
                out.push_str(if first { " " } else { ", " });
                first = false;
                if s.neg {
                    out.push('-');
                }
                out.push_str(&self.reg_text(s.r, &phys, lit_base));
                if s.swz != [0, 1, 2, 3] && !matches!(s.r, R::S(_)) {
                    out.push('.');
                    if s.swz.iter().all(|&c| c == s.swz[0]) {
                        out.push(SWZ[s.swz[0] as usize]);
                    } else {
                        for &c in &s.swz {
                            out.push(SWZ[c as usize]);
                        }
                    }
                }
            }
            out.push('\n');
        }
        Ok(out)
    }
}

/// Table « CTAB » (format public D3DXSHADER_CONSTANTTABLE) en commentaire.
fn ctab_comment(entries: &[CtabEntry], version: u32, target: &str) -> Vec<u32> {
    let mut b: Vec<u8> = vec![0; 28];
    let info_off = b.len();
    b.resize(info_off + 20 * entries.len(), 0);
    let put32 =
        |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let put16 =
        |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let add_str = |b: &mut Vec<u8>, s: &str| -> u32 {
        let at = b.len() as u32;
        b.extend_from_slice(s.as_bytes());
        b.push(0);
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        at
    };
    for (i, e) in entries.iter().enumerate() {
        let at = info_off + 20 * i;
        let name = add_str(&mut b, &e.name);
        let ti = b.len();
        b.resize(ti + 16, 0);
        put16(&mut b, ti, e.class);
        put16(&mut b, ti + 2, e.ty);
        put16(&mut b, ti + 4, e.rows);
        put16(&mut b, ti + 6, e.cols);
        put16(&mut b, ti + 8, e.elements);
        let def = match &e.default {
            Some(d) => {
                let at = b.len() as u32;
                for x in d {
                    b.extend_from_slice(&x.to_le_bytes());
                }
                at
            }
            None => 0,
        };
        put32(&mut b, at, name);
        put16(&mut b, at + 4, e.set);
        put16(&mut b, at + 6, e.index);
        put16(&mut b, at + 8, e.count);
        put32(&mut b, at + 12, ti as u32);
        put32(&mut b, at + 16, def);
    }
    let creator = add_str(&mut b, "Winlux HLSL");
    let tgt = add_str(&mut b, target);
    put32(&mut b, 0, 28);
    put32(&mut b, 4, creator);
    put32(&mut b, 8, version);
    put32(&mut b, 12, entries.len() as u32);
    put32(
        &mut b,
        16,
        if entries.is_empty() {
            0
        } else {
            info_off as u32
        },
    );
    put32(&mut b, 20, 0);
    put32(&mut b, 24, tgt);
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let mut words = vec![0u32, u32::from_le_bytes(*b"CTAB")];
    words.extend(
        b.chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
    );
    words[0] = 0xFFFE | (((words.len() - 1) as u32) << 16);
    words
}

/// Profil demandé → profil produit (les 1.x passent en 2.0, comme le permet
/// le pilote Direct3D 9 de Winlux).
fn target_profile(profile: &str) -> Result<(bool, bool, &'static str), String> {
    let p = profile.to_ascii_lowercase();
    Ok(match p.as_str() {
        "vs_1_1" | "vs_2_0" | "vs_2_a" | "vs_2_sw" => (false, false, "vs_2_0"),
        "vs_3_0" | "vs_3_sw" => (false, true, "vs_3_0"),
        "ps_1_1" | "ps_1_2" | "ps_1_3" | "ps_1_4" | "ps_2_0" | "ps_2_a" | "ps_2_b" | "ps_2_sw" => {
            (true, false, "ps_2_0")
        }
        "ps_3_0" | "ps_3_sw" => (true, true, "ps_3_0"),
        "fx_2_0" => return Err("fx_2_0 : utiliser D3DXCreateEffect".into()),
        _ => return Err(format!("profil inconnu : {profile}")),
    })
}

pub(crate) const FLAG_ROWMAJOR: u32 = 0x8;

/// Compile une fonction d'un programme déjà analysé. `args` donne les
/// valeurs des paramètres `uniform` (effets : compile ps_2_0 f(true)).
pub(crate) fn compile_function(
    p: &Program,
    entry: &str,
    profile: &str,
    flags: u32,
    args: &[Expr],
) -> Result<Compiled, String> {
    let (pixel, sm3, target) = target_profile(profile)?;
    let f = p
        .funcs
        .iter()
        .find(|f| f.name == entry && f.body.is_some())
        .ok_or_else(|| format!("point d'entrée introuvable : {entry}"))?;
    let mut g = Gen {
        p,
        pixel,
        sm3,
        row_major: flags & FLAG_ROWMAJOR != 0,
        code: Vec::new(),
        ntemps: 0,
        lits: Vec::new(),
        lit_fill: Vec::new(),
        named: Vec::new(),
        dcls: Vec::new(),
        globals: HashMap::new(),
        scopes: Vec::new(),
        conds: Vec::new(),
        frames: Vec::new(),
        block_pushes: Vec::new(),
        ctab: Vec::new(),
        samplers: HashMap::new(),
        used: used_names(p, entry),
        next_input: 0,
        next_output: 0,
        call_depth: 0,
    };
    for a in args {
        let mut w = Vec::new();
        walk_expr(a, &mut w);
        g.used.extend(w);
    }
    // Paramètres uniformes sans valeur : constantes nommées comme eux.
    let extra: Vec<(String, Ty)> = f
        .params
        .iter()
        .filter(|prm| prm.mods.uniform)
        .skip(args.len())
        .map(|prm| (prm.name.clone(), prm.ty.clone()))
        .collect();
    g.alloc_uniforms(&extra)?;
    let mut argv = Vec::new();
    for a in args {
        argv.push(g.expr(a)?);
    }
    g.entry(f, &argv)?;
    // Uniformes jamais lus : retirées de la table.
    let asm = g.text(target)?;
    let mut tokens = super::d3dx9_asm::assemble(&asm, &[])
        .map_err(|e| format!("assembleur interne{e}\n{asm}"))?;
    let version = tokens.first().copied().unwrap_or(0);
    let comment = ctab_comment(&g.ctab, version, target);
    tokens.splice(1..1, comment);
    Ok(Compiled {
        tokens,
        asm,
        ctab: g.ctab,
    })
}

/// D3DXCompileShader : source → bytecode.
pub(crate) fn compile(
    src: &str,
    defines: &[(String, String)],
    include: &dyn Fn(&str) -> Option<String>,
    entry: &str,
    profile: &str,
    flags: u32,
) -> Result<Compiled, String> {
    let p = parse(src, defines, include)?;
    compile_function(&p, entry, profile, flags, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    fn ok(src: &str, entry: &str, profile: &str) -> Compiled {
        match compile(src, &[], &none, entry, profile, 0) {
            Ok(c) => c,
            Err(e) => panic!("{e}"),
        }
    }

    /// Structure uniforme et sortie de vertex shader reprise en entrée de
    /// pixel shader avec POSITION, comme les shaders générés par WPF (#931).
    #[test]
    fn structure_uniforme_et_position_ignoree_en_pixel_shader() {
        let src = "struct T { float4x4 m; float4 c; };\n\
                   struct D { T t; float4 k; };\n\
                   D Data;\n\
                   struct O { float4 Pos : POSITION; float4 Col : COLOR0; };\n\
                   O vs(float4 p : POSITION) { O o; o.Pos = mul(p, Data.t.m); o.Col = Data.t.c * Data.k; return o; }\n\
                   float4 ps(O i) : COLOR { return i.Col; }";
        let v = ok(src, "vs", "vs_2_0");
        let names: Vec<(&str, u16, u16)> = v
            .ctab
            .iter()
            .map(|e| (e.name.as_str(), e.index, e.count))
            .collect();
        assert!(names.contains(&("Data.t.m", 0, 4)), "{names:?}");
        assert!(names.contains(&("Data.t.c", 4, 1)), "{names:?}");
        assert!(names.contains(&("Data.k", 5, 1)), "{names:?}");
        let p = ok(src, "ps", "ps_2_0");
        assert_eq!(p.tokens[0], 0xFFFF_0200);
    }

    #[test]
    fn vertex_shader_simple() {
        let c = ok(
            "float4x4 WorldViewProj : WORLDVIEWPROJECTION;\n\
             struct VS_OUT { float4 Pos : POSITION; float2 Tex : TEXCOORD0; float4 Col : COLOR0; };\n\
             VS_OUT main(float4 p : POSITION, float2 t : TEXCOORD0) {\n\
               VS_OUT o;\n\
               o.Pos = mul(p, WorldViewProj);\n\
               o.Tex = t * 2.0;\n\
               o.Col = float4(1, 0.5, 0, 1);\n\
               return o;\n\
             }",
            "main",
            "vs_2_0",
        );
        assert!(c.asm.contains("dcl_position v0"), "{}", c.asm);
        assert!(c.asm.contains("dp4 "), "{}", c.asm);
        assert!(c.asm.contains("oPos"), "{}", c.asm);
        assert_eq!(c.ctab.len(), 1);
        assert_eq!(c.ctab[0].count, 4);
        let t = super::super::d3dx9_asm::constant_table(&c.tokens).unwrap();
        assert_eq!(t.constants[0].name, "WorldViewProj");
        assert_eq!(t.constants[0].class, 3);
    }

    #[test]
    fn pixel_shader_texture_et_conditions() {
        let c = ok(
            "sampler2D Base : register(s0);\n\
             float4 Tint = {1, 1, 1, 1};\n\
             float Threshold;\n\
             float4 main(float2 uv : TEXCOORD0, float4 col : COLOR0) : COLOR {\n\
               float4 t = tex2D(Base, uv) * col;\n\
               if (t.a < Threshold) discard;\n\
               float3 n = normalize(t.rgb * 2 - 1);\n\
               float l = saturate(dot(n, float3(0, 0, 1)));\n\
               for (int i = 0; i < 2; i++) l *= 0.5;\n\
               return t.a > 0.5 ? float4(n * l, 1) * Tint : Tint;\n\
             }",
            "main",
            "ps_2_0",
        );
        let a = &c.asm;
        assert!(a.contains("texld "), "{a}");
        assert!(a.contains("dcl_2d s0"), "{a}");
        assert!(a.contains("texkill"), "{a}");
        assert!(a.contains("nrm"), "{a}");
        assert!(a.contains("cmp"), "{a}");
        assert!(a.contains("mov oC0"), "{a}");
        let tint = c.ctab.iter().find(|e| e.name == "Tint").unwrap();
        assert_eq!(tint.default.as_deref(), Some(&[1.0f32, 1.0, 1.0, 1.0][..]));
    }

    #[test]
    fn fonctions_retours_et_macros() {
        let c = compile(
            "#define SCALE 2\n\
             #ifdef MODE\n float k = MODE;\n #endif\n\
             float sq(float x) { return x * x; }\n\
             float pick(float a) { if (a > 1) return a; return -a; }\n\
             void sc(float x, out float s, out float c) { sincos(x, s, c); }\n\
             float4 main(float4 c : COLOR0) : COLOR {\n\
               float s, co; sc(c.x, s, co);\n\
               return float4(sq(c.r) * SCALE, pick(c.g), s + co, k);\n\
             }",
            &[("MODE".into(), "3".into())],
            &none,
            "main",
            "ps_3_0",
            0,
        )
        .unwrap();
        assert!(c.asm.contains("sincos"), "{}", c.asm);
        assert!(c.asm.contains("lrp"), "{}", c.asm);
        assert!(c.ctab.iter().any(|e| e.name == "k"));
    }

    #[test]
    fn tableau_dynamique_vs() {
        let c = ok(
            "float4x3 Bones[20];\n float4x4 VP;\n\
             float4 main(float4 p : POSITION, float4 bi : BLENDINDICES) : POSITION {\n\
               float3 w = mul(p, Bones[bi.x]);\n\
               return mul(float4(w, 1), VP);\n\
             }",
            "main",
            "vs_3_0",
        );
        assert!(c.asm.contains("mova"), "{}", c.asm);
        assert!(c.asm.contains("c[a0.x"), "{}", c.asm);
        assert_eq!(c.ctab.iter().find(|e| e.name == "Bones").unwrap().count, 60);
    }

    #[test]
    fn erreurs() {
        assert!(compile(
            "float4 main() : COLOR { return foo; }",
            &[],
            &none,
            "main",
            "ps_2_0",
            0
        )
        .is_err());
        assert!(compile(
            "float4 main() : COLOR { return 1 }",
            &[],
            &none,
            "main",
            "ps_2_0",
            0
        )
        .is_err());
        assert!(compile(
            "float4 f() : COLOR { return 1; }",
            &[],
            &none,
            "main",
            "ps_2_0",
            0
        )
        .is_err());
    }
}
