//! Analyse de JScript (ECMAScript 3, d'après la spécification publique).

use super::ast::*;
use super::{SyntaxError, V};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
enum T {
    Num(f64),
    Str(String),
    Id(String),
    P(&'static str),
    Re(String, String),
    Eof,
}

#[derive(Clone, Debug)]
struct Tok {
    t: T,
    line: usize,
    nl: bool,
}

const PUNCT: &[&str] = &[
    ">>>=", "===", "!==", ">>>", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "++", "--",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "{", "}", "(", ")", "[", "]", ";",
    ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":", "=", ".",
];

const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "finally",
    "for",
    "function",
    "if",
    "in",
    "instanceof",
    "new",
    "return",
    "switch",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
];

fn err(line: usize, text: impl Into<String>) -> SyntaxError {
    SyntaxError {
        line,
        text: text.into(),
    }
}

fn lex(src: &str) -> Result<Vec<Tok>, SyntaxError> {
    let s: Vec<char> = src.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut nl = false;
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            line += 1;
            nl = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
            continue;
        }
        if c == '/' && s.get(i + 1) == Some(&'/') {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && s.get(i + 1) == Some(&'*') {
            i += 2;
            while i < s.len() && !(s[i] == '*' && s.get(i + 1) == Some(&'/')) {
                if s[i] == '\n' {
                    line += 1;
                    nl = true;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        let start_line = line;
        let t =
            if c.is_ascii_digit() || c == '.' && s.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                let start = i;
                if c == '0' && matches!(s.get(i + 1), Some('x' | 'X')) {
                    i += 2;
                    while i < s.len() && s[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    let h: String = s[start + 2..i].iter().collect();
                    T::Num(
                        i64::from_str_radix(&h, 16).map_err(|_| err(line, "nombre hexadécimal"))?
                            as f64,
                    )
                } else {
                    while i < s.len() && (s[i].is_ascii_digit() || s[i] == '.') {
                        i += 1;
                    }
                    if i < s.len() && matches!(s[i], 'e' | 'E') {
                        i += 1;
                        if i < s.len() && matches!(s[i], '+' | '-') {
                            i += 1;
                        }
                        while i < s.len() && s[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let n: String = s[start..i].iter().collect();
                    T::Num(
                        n.parse()
                            .map_err(|_| err(line, format!("nombre « {n} »")))?,
                    )
                }
            } else if c == '"' || c == '\'' {
                i += 1;
                let mut v = String::new();
                loop {
                    let Some(&d) = s.get(i) else {
                        return Err(err(line, "chaîne non terminée"));
                    };
                    i += 1;
                    if d == c {
                        break;
                    }
                    if d == '\n' {
                        return Err(err(line, "chaîne non terminée"));
                    }
                    if d != '\\' {
                        v.push(d);
                        continue;
                    }
                    let Some(&e) = s.get(i) else { break };
                    i += 1;
                    match e {
                        'n' => v.push('\n'),
                        'r' => v.push('\r'),
                        't' => v.push('\t'),
                        'b' => v.push('\u{8}'),
                        'f' => v.push('\u{c}'),
                        'v' => v.push('\u{b}'),
                        '0' if !s.get(i).is_some_and(|d| d.is_ascii_digit()) => v.push('\0'),
                        'x' | 'u' => {
                            let n = if e == 'x' { 2 } else { 4 };
                            let h: String = s[i..(i + n).min(s.len())].iter().collect();
                            match u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                                Some(ch) if h.len() == n => {
                                    v.push(ch);
                                    i += n;
                                }
                                _ => v.push(e),
                            }
                        }
                        '\n' => line += 1,
                        '\r' => {
                            if s.get(i) == Some(&'\n') {
                                i += 1;
                            }
                            line += 1;
                        }
                        o => v.push(o),
                    }
                }
                T::Str(v)
            } else if c.is_alphabetic() || c == '_' || c == '$' {
                let start = i;
                while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_' || s[i] == '$') {
                    i += 1;
                }
                T::Id(s[start..i].iter().collect())
            } else if c == '/' && regex_allowed(out.last()) {
                i += 1;
                let mut pat = String::new();
                let mut in_class = false;
                loop {
                    let Some(&d) = s.get(i) else {
                        return Err(err(line, "expression régulière non terminée"));
                    };
                    i += 1;
                    if d == '\n' {
                        return Err(err(line, "expression régulière non terminée"));
                    }
                    if d == '\\' {
                        pat.push(d);
                        if let Some(&e) = s.get(i) {
                            pat.push(e);
                            i += 1;
                        }
                        continue;
                    }
                    if d == '[' {
                        in_class = true;
                    } else if d == ']' {
                        in_class = false;
                    } else if d == '/' && !in_class {
                        break;
                    }
                    pat.push(d);
                }
                let mut flags = String::new();
                while i < s.len() && s[i].is_alphabetic() {
                    flags.push(s[i]);
                    i += 1;
                }
                T::Re(pat, flags)
            } else {
                let rest: String = s[i..(i + 4).min(s.len())].iter().collect();
                let Some(p) = PUNCT.iter().find(|p| rest.starts_with(**p)) else {
                    return Err(err(line, format!("caractère inattendu « {c} »")));
                };
                i += p.chars().count();
                T::P(p)
            };
        out.push(Tok {
            t,
            line: start_line,
            nl,
        });
        nl = false;
    }
    out.push(Tok {
        t: T::Eof,
        line,
        nl: true,
    });
    Ok(out)
}

fn regex_allowed(prev: Option<&Tok>) -> bool {
    match prev.map(|t| &t.t) {
        None => true,
        Some(T::Num(_) | T::Str(_) | T::Re(..)) => false,
        Some(T::Id(w)) => KEYWORDS.contains(&w.as_str()) && w != "this",
        Some(T::P(p)) => !matches!(*p, ")" | "]" | "}"),
        Some(T::Eof) => true,
    }
}

struct P {
    t: Vec<Tok>,
    i: usize,
    no_in: bool,
}

type PR<T> = Result<T, SyntaxError>;

impl P {
    fn peek(&self) -> &T {
        &self.t[self.i].t
    }
    fn line(&self) -> usize {
        self.t[self.i].line
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), T::P(x) if *x == p)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), T::Id(x) if x == k)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        if self.is_kw(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> PR<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(err(
                self.line(),
                format!("« {p} » attendu, trouvé {}", self.describe()),
            ))
        }
    }
    fn describe(&self) -> String {
        match self.peek() {
            T::Num(n) => format!("le nombre {n}"),
            T::Str(_) => "une chaîne".into(),
            T::Id(n) => format!("« {n} »"),
            T::P(p) => format!("« {p} »"),
            T::Re(..) => "une expression régulière".into(),
            T::Eof => "la fin du script".into(),
        }
    }
    fn ident(&mut self) -> PR<String> {
        match self.peek().clone() {
            T::Id(n) if !KEYWORDS.contains(&n.as_str()) => {
                self.i += 1;
                Ok(n)
            }
            _ => Err(err(
                self.line(),
                format!("nom attendu, trouvé {}", self.describe()),
            )),
        }
    }
    /// Fin d'instruction : `;`, ou insertion automatique (nouvelle ligne,
    /// `}` ou fin du script).
    fn end(&mut self) -> PR<()> {
        if self.eat(";") || self.is("}") || matches!(self.peek(), T::Eof) || self.t[self.i].nl {
            Ok(())
        } else {
            Err(err(
                self.line(),
                format!("« ; » attendu, trouvé {}", self.describe()),
            ))
        }
    }

    fn program(&mut self) -> PR<Vec<Stmt>> {
        let mut out = Vec::new();
        while !matches!(self.peek(), T::Eof) {
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn block_body(&mut self) -> PR<Vec<Stmt>> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.eat("}") {
            if matches!(self.peek(), T::Eof) {
                return Err(err(self.line(), "« } » manquant"));
            }
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn body(&mut self) -> PR<Vec<Stmt>> {
        Ok(match self.stmt()? {
            Stmt::Block(b) => b,
            s => vec![s],
        })
    }

    fn var_list(&mut self) -> PR<Vec<(String, Option<Expr>, Option<Vec<Expr>>)>> {
        let mut list = Vec::new();
        loop {
            let name = self.ident()?;
            let init = if self.eat("=") {
                Some(self.assign()?)
            } else {
                None
            };
            list.push((name, init, None));
            if !self.eat(",") {
                break;
            }
        }
        Ok(list)
    }

    fn function(&mut self) -> PR<Rc<FuncDef>> {
        let name = match self.peek() {
            T::Id(_) => self.ident()?,
            _ => String::new(),
        };
        self.expect("(")?;
        let mut params = Vec::new();
        if !self.eat(")") {
            loop {
                params.push(self.ident()?);
                if self.eat(")") {
                    break;
                }
                self.expect(",")?;
            }
        }
        let no_in = std::mem::replace(&mut self.no_in, false);
        let body = self.block_body()?;
        self.no_in = no_in;
        Ok(Rc::new(FuncDef {
            name,
            params,
            body,
            is_sub: false,
        }))
    }

    fn stmt(&mut self) -> PR<Stmt> {
        let line = self.line();
        if self.is("{") {
            return Ok(Stmt::Block(self.block_body()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        let T::Id(word) = self.peek().clone() else {
            let e = self.expr()?;
            self.end()?;
            return Ok(Stmt::Expr(e));
        };
        // Étiquette `nom:` : ignorée (break/continue visent la boucle la plus proche).
        if !KEYWORDS.contains(&word.as_str())
            && matches!(self.t.get(self.i + 1).map(|t| &t.t), Some(T::P(":")))
        {
            self.i += 2;
            return self.stmt();
        }
        match word.as_str() {
            "var" | "let" | "const" => {
                self.i += 1;
                let list = self.var_list()?;
                self.end()?;
                Ok(Stmt::Var(list))
            }
            "function" => {
                self.i += 1;
                let f = self.function()?;
                if f.name.is_empty() {
                    return Err(err(line, "nom de fonction attendu"));
                }
                Ok(Stmt::Func(f))
            }
            "if" => {
                self.i += 1;
                self.expect("(")?;
                let c = self.expr()?;
                self.expect(")")?;
                let then = self.body()?;
                let mut arms = vec![(c, then)];
                let mut other = None;
                if self.eat_kw("else") {
                    match self.stmt()? {
                        Stmt::If(more, rest) => {
                            arms.extend(more);
                            other = rest;
                        }
                        Stmt::Block(b) => other = Some(b),
                        s => other = Some(vec![s]),
                    }
                }
                Ok(Stmt::If(arms, other))
            }
            "while" => {
                self.i += 1;
                self.expect("(")?;
                let c = self.expr()?;
                self.expect(")")?;
                Ok(Stmt::Loop {
                    cond: Some(c),
                    post: false,
                    until: false,
                    body: self.body()?,
                })
            }
            "do" => {
                self.i += 1;
                let body = self.body()?;
                if !self.eat_kw("while") {
                    return Err(err(self.line(), "« while » attendu après do"));
                }
                self.expect("(")?;
                let c = self.expr()?;
                self.expect(")")?;
                self.eat(";");
                Ok(Stmt::Loop {
                    cond: Some(c),
                    post: true,
                    until: false,
                    body,
                })
            }
            "for" => {
                self.i += 1;
                self.expect("(")?;
                let mut init = Vec::new();
                if !self.is(";") {
                    self.no_in = true;
                    if self.eat_kw("var") || self.eat_kw("let") || self.eat_kw("const") {
                        let list = self.var_list()?;
                        if list.len() == 1 && list[0].1.is_none() && self.eat_kw("in") {
                            self.no_in = false;
                            let coll = self.expr()?;
                            self.expect(")")?;
                            let name = list[0].0.clone();
                            return Ok(Stmt::ForEach {
                                var: Expr::Ident(name),
                                coll,
                                keys: true,
                                body: self.body()?,
                            });
                        }
                        init.push(Stmt::Var(list));
                    } else {
                        let e = self.expr()?;
                        if self.eat_kw("in") {
                            self.no_in = false;
                            let coll = self.expr()?;
                            self.expect(")")?;
                            return Ok(Stmt::ForEach {
                                var: e,
                                coll,
                                keys: true,
                                body: self.body()?,
                            });
                        }
                        init.push(Stmt::Expr(e));
                    }
                    self.no_in = false;
                }
                self.expect(";")?;
                let cond = if self.is(";") {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(";")?;
                let step = if self.is(")") {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(")")?;
                Ok(Stmt::For {
                    init,
                    cond,
                    step,
                    body: self.body()?,
                })
            }
            "return" => {
                self.i += 1;
                let e = if self.is(";")
                    || self.is("}")
                    || self.t[self.i].nl
                    || matches!(self.peek(), T::Eof)
                {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.end()?;
                Ok(Stmt::Return(e))
            }
            "break" | "continue" => {
                self.i += 1;
                if !self.t[self.i].nl {
                    if let T::Id(n) = self.peek() {
                        if !KEYWORDS.contains(&n.as_str()) {
                            self.i += 1; // étiquette
                        }
                    }
                }
                self.end()?;
                Ok(if word == "break" {
                    Stmt::Break
                } else {
                    Stmt::Continue
                })
            }
            "throw" => {
                self.i += 1;
                let e = self.expr()?;
                self.end()?;
                Ok(Stmt::Throw(e))
            }
            "try" => {
                self.i += 1;
                let body = self.block_body()?;
                let mut catch = None;
                let mut finally = None;
                if self.eat_kw("catch") {
                    self.expect("(")?;
                    let name = self.ident()?;
                    self.expect(")")?;
                    catch = Some((name, self.block_body()?));
                }
                if self.eat_kw("finally") {
                    finally = Some(self.block_body()?);
                }
                if catch.is_none() && finally.is_none() {
                    return Err(err(line, "« catch » ou « finally » attendu"));
                }
                Ok(Stmt::Try {
                    body,
                    catch,
                    finally,
                })
            }
            "switch" => {
                self.i += 1;
                self.expect("(")?;
                let subject = self.expr()?;
                self.expect(")")?;
                self.expect("{")?;
                let mut cases = Vec::new();
                while !self.eat("}") {
                    let test = if self.eat_kw("case") {
                        Some(self.expr()?)
                    } else if self.eat_kw("default") {
                        None
                    } else {
                        return Err(err(self.line(), "« case » attendu"));
                    };
                    self.expect(":")?;
                    let mut body = Vec::new();
                    while !self.is("}") && !self.is_kw("case") && !self.is_kw("default") {
                        if matches!(self.peek(), T::Eof) {
                            return Err(err(self.line(), "« } » manquant"));
                        }
                        body.push(self.stmt()?);
                    }
                    cases.push((test, body));
                }
                Ok(Stmt::Switch(subject, cases))
            }
            "with" => {
                self.i += 1;
                Err(err(line, "« with » n'est pas pris en charge"))
            }
            _ => {
                let e = self.expr()?;
                self.end()?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn expr(&mut self) -> PR<Expr> {
        let first = self.assign()?;
        if !self.is(",") {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(",") {
            items.push(self.assign()?);
        }
        Ok(Expr::Seq(items))
    }

    fn assign(&mut self) -> PR<Expr> {
        let lhs = self.cond()?;
        let op = match self.peek() {
            T::P("=") => None,
            T::P("+=") => Some(BOp::Add),
            T::P("-=") => Some(BOp::Sub),
            T::P("*=") => Some(BOp::Mul),
            T::P("/=") => Some(BOp::Div),
            T::P("%=") => Some(BOp::Mod),
            T::P("&=") => Some(BOp::BitAnd),
            T::P("|=") => Some(BOp::BitOr),
            T::P("^=") => Some(BOp::BitXor),
            T::P("<<=") => Some(BOp::Shl),
            T::P(">>=") => Some(BOp::Shr),
            T::P(">>>=") => Some(BOp::UShr),
            _ => return Ok(lhs),
        };
        if !matches!(
            lhs,
            Expr::Ident(_) | Expr::Member(..) | Expr::Index(..) | Expr::Call(..)
        ) {
            return Err(err(
                self.line(),
                "affectation à une valeur qui n'est pas une variable",
            ));
        }
        self.i += 1;
        let rhs = self.assign()?;
        Ok(Expr::Assign(Box::new(lhs), op, Box::new(rhs)))
    }

    fn cond(&mut self) -> PR<Expr> {
        let c = self.or()?;
        if !self.eat("?") {
            return Ok(c);
        }
        let no_in = std::mem::replace(&mut self.no_in, false);
        let a = self.assign()?;
        self.no_in = no_in;
        self.expect(":")?;
        let b = self.assign()?;
        Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)))
    }

    fn or(&mut self) -> PR<Expr> {
        let mut l = self.and()?;
        while self.eat("||") {
            let r = self.and()?;
            l = Expr::Or(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> PR<Expr> {
        let mut l = self.binary(0)?;
        while self.eat("&&") {
            let r = self.binary(0)?;
            l = Expr::And(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    /// Opérateurs binaires par niveaux de priorité croissante.
    fn binary(&mut self, level: usize) -> PR<Expr> {
        const LEVELS: &[&[(&str, BOp)]] = &[
            &[("|", BOp::BitOr)],
            &[("^", BOp::BitXor)],
            &[("&", BOp::BitAnd)],
            &[
                ("===", BOp::StrictEq),
                ("!==", BOp::StrictNe),
                ("==", BOp::Eq),
                ("!=", BOp::Ne),
            ],
            &[
                ("<=", BOp::Le),
                (">=", BOp::Ge),
                ("<", BOp::Lt),
                (">", BOp::Gt),
                ("instanceof", BOp::InstanceOf),
                ("in", BOp::In),
            ],
            &[("<<", BOp::Shl), (">>>", BOp::UShr), (">>", BOp::Shr)],
            &[("+", BOp::Add), ("-", BOp::Sub)],
            &[("*", BOp::Mul), ("/", BOp::Div), ("%", BOp::Mod)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut l = self.binary(level + 1)?;
        loop {
            let found = LEVELS[level].iter().find(|(p, _)| match self.peek() {
                T::P(x) => x == p,
                T::Id(x) => {
                    x == p && (*p != "in" || !self.no_in) && (*p == "in" || *p == "instanceof")
                }
                _ => false,
            });
            let Some((_, op)) = found else { break };
            self.i += 1;
            let r = self.binary(level + 1)?;
            l = Expr::Bin(*op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> PR<Expr> {
        let op = match self.peek() {
            T::P("!") => Some(UOp::Not),
            T::P("-") => Some(UOp::Neg),
            T::P("+") => Some(UOp::Plus),
            T::P("~") => Some(UOp::BitNot),
            T::Id(w) if w == "typeof" => Some(UOp::TypeOf),
            T::Id(w) if w == "void" => Some(UOp::Void),
            T::Id(w) if w == "delete" => Some(UOp::Delete),
            _ => None,
        };
        if let Some(op) = op {
            self.i += 1;
            let x = self.unary()?;
            return Ok(Expr::Unary(op, Box::new(x)));
        }
        if self.is("++") || self.is("--") {
            let d = if self.is("++") { 1.0 } else { -1.0 };
            self.i += 1;
            let x = self.unary()?;
            return Ok(Expr::Update(Box::new(x), d, true));
        }
        let x = self.postfix()?;
        if (self.is("++") || self.is("--")) && !self.t[self.i].nl {
            let d = if self.is("++") { 1.0 } else { -1.0 };
            self.i += 1;
            return Ok(Expr::Update(Box::new(x), d, false));
        }
        Ok(x)
    }

    fn args(&mut self) -> PR<Vec<Expr>> {
        let mut out = Vec::new();
        let no_in = std::mem::replace(&mut self.no_in, false);
        if !self.eat(")") {
            loop {
                out.push(self.assign()?);
                if self.eat(")") {
                    break;
                }
                self.expect(",")?;
            }
        }
        self.no_in = no_in;
        Ok(out)
    }

    fn postfix(&mut self) -> PR<Expr> {
        let mut e = if self.eat_kw("new") {
            let callee = self.member_only()?;
            let args = if self.eat("(") {
                self.args()?
            } else {
                Vec::new()
            };
            Expr::New(Box::new(callee), args)
        } else {
            self.primary()?
        };
        loop {
            if self.eat(".") {
                let name = match self.peek().clone() {
                    T::Id(n) => {
                        self.i += 1;
                        n
                    }
                    _ => return Err(err(self.line(), "nom de membre attendu après « . »")),
                };
                e = Expr::Member(Box::new(e), name);
            } else if self.eat("[") {
                let no_in = std::mem::replace(&mut self.no_in, false);
                let k = self.expr()?;
                self.no_in = no_in;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(k));
            } else if self.eat("(") {
                let args = self.args()?;
                e = Expr::Call(Box::new(e), args);
            } else {
                break;
            }
        }
        Ok(e)
    }

    /// Cible de `new` : nom et membres, sans appel.
    fn member_only(&mut self) -> PR<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(".") {
                let T::Id(n) = self.peek().clone() else {
                    return Err(err(self.line(), "nom de membre attendu"));
                };
                self.i += 1;
                e = Expr::Member(Box::new(e), n);
            } else if self.eat("[") {
                let k = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(k));
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> PR<Expr> {
        let line = self.line();
        let t = self.peek().clone();
        self.i += 1;
        Ok(match t {
            T::Num(n) => Expr::Lit(V::Num(n)),
            T::Str(s) => Expr::Lit(V::str(s)),
            T::Re(p, f) => Expr::Regex(p, f),
            T::P("(") => {
                let no_in = std::mem::replace(&mut self.no_in, false);
                let e = self.expr()?;
                self.no_in = no_in;
                self.expect(")")?;
                e
            }
            T::P("[") => {
                let mut items = Vec::new();
                while !self.eat("]") {
                    if self.is(",") {
                        self.i += 1;
                        items.push(Expr::Lit(V::Empty));
                        continue;
                    }
                    items.push(self.assign()?);
                    if !self.eat(",") {
                        self.expect("]")?;
                        break;
                    }
                }
                Expr::Array(items)
            }
            T::P("{") => {
                let mut props = Vec::new();
                while !self.eat("}") {
                    let key = match self.peek().clone() {
                        T::Id(n) | T::Str(n) => n,
                        T::Num(n) => super::num_str(n),
                        _ => return Err(err(self.line(), "nom de propriété attendu")),
                    };
                    self.i += 1;
                    self.expect(":")?;
                    let v = self.assign()?;
                    props.push((key, v));
                    if !self.eat(",") {
                        self.expect("}")?;
                        break;
                    }
                }
                Expr::Object(props)
            }
            T::Id(w) => match w.as_str() {
                "true" => Expr::Lit(V::Bool(true)),
                "false" => Expr::Lit(V::Bool(false)),
                "null" => Expr::Lit(V::Null),
                "this" => Expr::This,
                "function" => Expr::Func(self.function()?),
                k if KEYWORDS.contains(&k) => return Err(err(line, format!("« {k} » inattendu"))),
                _ => Expr::Ident(w),
            },
            T::Eof => return Err(err(line, "fin du script inattendue")),
            T::P(p) => return Err(err(line, format!("« {p} » inattendu"))),
        })
    }
}

pub fn parse(src: &str) -> Result<Vec<Stmt>, SyntaxError> {
    let t = lex(src)?;
    P {
        t,
        i: 0,
        no_in: false,
    }
    .program()
}
