//! Analyse de VBScript (d'après la référence publique du langage). Les
//! noms sont mis en minuscules : le langage ignore la casse.

use super::ast::*;
use super::{SyntaxError, V};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
enum T {
    Num(f64),
    Str(String),
    Id(String),
    Op(&'static str),
    /// Fin d'instruction : nouvelle ligne ou `:`.
    End,
    Eof,
}

#[derive(Clone, Debug)]
struct Tok {
    t: T,
    line: usize,
}

const OPS: &[&str] = &[
    "<=", ">=", "<>", "=", "<", ">", "+", "-", "*", "/", "\\", "^", "&", "(", ")", ",", ".",
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
    let push_end = |out: &mut Vec<Tok>, line: usize| {
        if !matches!(out.last().map(|t| &t.t), Some(T::End) | None) {
            out.push(Tok { t: T::End, line });
        }
    };
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            push_end(&mut out, line);
            line += 1;
            i += 1;
            continue;
        }
        if c == ':' {
            push_end(&mut out, line);
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
            continue;
        }
        if c == '\'' {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Suite de ligne : « _ » en fin de ligne.
        if c == '_'
            && !s
                .get(i + 1)
                .is_some_and(|d| d.is_alphanumeric() || *d == '_')
        {
            let mut j = i + 1;
            while j < s.len() && s[j] != '\n' && s[j].is_whitespace() {
                j += 1;
            }
            if j >= s.len() || s[j] == '\n' {
                i = j + 1;
                line += 1;
                continue;
            }
        }
        let t = if c.is_ascii_digit()
            || c == '.'
                && s.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                && !matches!(out.last().map(|t| &t.t), Some(T::Id(_) | T::Op(")")))
        {
            let start = i;
            while i < s.len() && (s[i].is_ascii_digit() || s[i] == '.') {
                i += 1;
            }
            if i < s.len()
                && matches!(s[i], 'e' | 'E')
                && s.get(i + 1)
                    .is_some_and(|d| d.is_ascii_digit() || *d == '+' || *d == '-')
            {
                i += 2;
                while i < s.len() && s[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let n: String = s[start..i].iter().collect();
            if i < s.len()
                && matches!(s[i], '&' | '!' | '#' | '@' | '%')
                && !s.get(i + 1).is_some_and(|d| d.is_alphanumeric())
            {
                i += 1; // suffixe de type
            }
            T::Num(
                n.parse()
                    .map_err(|_| err(line, format!("nombre « {n} »")))?,
            )
        } else if c == '&'
            && matches!(s.get(i + 1), Some('h' | 'H' | 'o' | 'O'))
            && s.get(i + 2).is_some_and(|d| d.is_ascii_hexdigit())
        {
            let radix = if matches!(s[i + 1], 'h' | 'H') { 16 } else { 8 };
            i += 2;
            let start = i;
            while i < s.len() && s[i].is_digit(radix) {
                i += 1;
            }
            let digits: String = s[start..i].iter().collect();
            if i < s.len() && s[i] == '&' {
                i += 1;
            }
            let v =
                u64::from_str_radix(&digits, radix).map_err(|_| err(line, "nombre hexadécimal"))?;
            // &HFFFF est -1 (entier 16 bits), &HFFFFFFFF aussi (32 bits).
            let n = if digits.len() <= 4 && radix == 16 {
                v as u16 as i16 as f64
            } else {
                v as u32 as i32 as f64
            };
            T::Num(n)
        } else if c == '"' {
            i += 1;
            let mut v = String::new();
            loop {
                match s.get(i) {
                    None | Some('\n') => return Err(err(line, "chaîne non terminée")),
                    Some('"') if s.get(i + 1) == Some(&'"') => {
                        v.push('"');
                        i += 2;
                    }
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some(&d) => {
                        v.push(d);
                        i += 1;
                    }
                }
            }
            T::Str(v)
        } else if c == '#' {
            // Date littérale : gardée en texte.
            i += 1;
            let start = i;
            while i < s.len() && s[i] != '#' && s[i] != '\n' {
                i += 1;
            }
            let v: String = s[start..i].iter().collect();
            i += 1;
            T::Str(v)
        } else if c == '[' {
            i += 1;
            let start = i;
            while i < s.len() && s[i] != ']' && s[i] != '\n' {
                i += 1;
            }
            let v: String = s[start..i].iter().collect();
            i += 1;
            T::Id(v.to_ascii_lowercase())
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            // Suffixe de type de l'ancien Basic (`nom$`) : ignoré.
            let name: String = s[start..i].iter().collect::<String>().to_lowercase();
            if i < s.len() && s[i] == '$' {
                i += 1;
            }
            if name == "rem" {
                while i < s.len() && s[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            T::Id(name)
        } else {
            let rest: String = s[i..(i + 2).min(s.len())].iter().collect();
            let Some(p) = OPS.iter().find(|p| rest.starts_with(**p)) else {
                return Err(err(line, format!("caractère inattendu « {c} »")));
            };
            i += p.len();
            T::Op(p)
        };
        out.push(Tok { t, line });
    }
    push_end(&mut out, line);
    out.push(Tok { t: T::Eof, line });
    Ok(out)
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

type PR<T> = Result<T, SyntaxError>;

/// Mots qui ne peuvent pas commencer une expression (fin de bloc…).
const RESERVED: &[&str] = &[
    "and", "or", "xor", "eqv", "imp", "mod", "is", "then", "else", "elseif", "end", "next", "loop",
    "wend", "to", "step", "in", "each", "case", "dim", "set", "if", "for", "do", "while", "until",
    "function", "sub", "exit",
];

impl P {
    fn peek(&self) -> &T {
        &self.t[self.i].t
    }
    fn peek_at(&self, n: usize) -> &T {
        &self.t[(self.i + n).min(self.t.len() - 1)].t
    }
    fn line(&self) -> usize {
        self.t[self.i].line
    }
    fn is_op(&self, p: &str) -> bool {
        matches!(self.peek(), T::Op(x) if *x == p)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), T::Id(x) if x == k)
    }
    fn is_kw_at(&self, n: usize, k: &str) -> bool {
        matches!(self.peek_at(n), T::Id(x) if x == k)
    }
    fn eat_op(&mut self, p: &str) -> bool {
        if self.is_op(p) {
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
    fn describe(&self) -> String {
        match self.peek() {
            T::Num(n) => format!("le nombre {n}"),
            T::Str(_) => "une chaîne".into(),
            T::Id(n) => format!("« {n} »"),
            T::Op(p) => format!("« {p} »"),
            T::End => "la fin de l'instruction".into(),
            T::Eof => "la fin du script".into(),
        }
    }
    fn expect_op(&mut self, p: &str) -> PR<()> {
        if self.eat_op(p) {
            Ok(())
        } else {
            Err(err(
                self.line(),
                format!("« {p} » attendu, trouvé {}", self.describe()),
            ))
        }
    }
    fn expect_kw(&mut self, k: &str) -> PR<()> {
        if self.eat_kw(k) {
            Ok(())
        } else {
            Err(err(
                self.line(),
                format!("« {k} » attendu, trouvé {}", self.describe()),
            ))
        }
    }
    fn name(&mut self) -> PR<String> {
        match self.peek().clone() {
            T::Id(n) => {
                self.i += 1;
                Ok(n)
            }
            _ => Err(err(
                self.line(),
                format!("nom attendu, trouvé {}", self.describe()),
            )),
        }
    }
    fn at_end(&self) -> bool {
        matches!(self.peek(), T::End | T::Eof)
    }
    fn skip_ends(&mut self) {
        while matches!(self.peek(), T::End) {
            self.i += 1;
        }
    }
    fn skip_to_end(&mut self) {
        while !self.at_end() {
            self.i += 1;
        }
    }

    /// Instructions jusqu'à l'un des mots de fin (non consommé).
    fn stmts_until(&mut self, stop: &dyn Fn(&P) -> bool) -> PR<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            self.skip_ends();
            if matches!(self.peek(), T::Eof) || stop(self) {
                return Ok(out);
            }
            out.push(self.stmt()?);
        }
    }

    fn program(&mut self) -> PR<Vec<Stmt>> {
        let out = self.stmts_until(&|_| false)?;
        Ok(out)
    }

    fn end_block(&mut self, what: &str) -> PR<()> {
        if self.is_kw("end") && self.is_kw_at(1, what) {
            self.i += 2;
            Ok(())
        } else {
            Err(err(
                self.line(),
                format!("« End {what} » attendu, trouvé {}", self.describe()),
            ))
        }
    }

    fn dims(&mut self) -> PR<Option<Vec<Expr>>> {
        if !self.eat_op("(") {
            return Ok(None);
        }
        let mut d = Vec::new();
        if !self.eat_op(")") {
            loop {
                d.push(self.expr()?);
                if self.eat_op(")") {
                    break;
                }
                self.expect_op(",")?;
            }
        }
        Ok(Some(d))
    }

    fn stmt(&mut self) -> PR<Stmt> {
        let line = self.line();
        let word = match self.peek() {
            T::Id(w) => w.clone(),
            _ => String::new(),
        };
        let s = match word.as_str() {
            "option" | "randomize" => {
                self.skip_to_end();
                Stmt::Empty
            }
            "public" | "private"
                if self.is_kw_at(1, "function")
                    || self.is_kw_at(1, "sub")
                    || self.is_kw_at(1, "const")
                    || self.is_kw_at(1, "default") =>
            {
                self.i += 1;
                self.eat_kw("default");
                return self.stmt();
            }
            "dim" | "public" | "private" => {
                self.i += 1;
                let mut list = Vec::new();
                loop {
                    let n = self.name()?;
                    let d = self.dims()?;
                    list.push((n, None, d));
                    if !self.eat_op(",") {
                        break;
                    }
                }
                Stmt::Var(list)
            }
            "const" => {
                self.i += 1;
                let mut list = Vec::new();
                loop {
                    let n = self.name()?;
                    self.expect_op("=")?;
                    let v = self.expr()?;
                    list.push((n, Some(v), None));
                    if !self.eat_op(",") {
                        break;
                    }
                }
                Stmt::Var(list)
            }
            "redim" => {
                self.i += 1;
                let preserve = self.eat_kw("preserve");
                let mut out = Vec::new();
                loop {
                    let n = self.name()?;
                    let d = self
                        .dims()?
                        .ok_or_else(|| err(line, "dimensions attendues après ReDim"))?;
                    out.push(Stmt::ReDim(preserve, n, d));
                    if !self.eat_op(",") {
                        break;
                    }
                }
                if out.len() == 1 {
                    out.pop().unwrap()
                } else {
                    Stmt::Block(out)
                }
            }
            "erase" => {
                self.i += 1;
                let n = self.name()?;
                Stmt::Assign(Expr::Ident(n), Expr::Array(Vec::new()))
            }
            "set" => {
                self.i += 1;
                let target = self.postfix()?;
                self.expect_op("=")?;
                let v = self.expr()?;
                Stmt::Assign(target, v)
            }
            "call" => {
                self.i += 1;
                let e = self.postfix()?;
                Stmt::Expr(e)
            }
            "if" => return self.if_stmt(),
            "for" => return self.for_stmt(),
            "do" => {
                self.i += 1;
                let mut cond = None;
                let mut until = false;
                if self.is_kw("while") || self.is_kw("until") {
                    until = self.is_kw("until");
                    self.i += 1;
                    cond = Some(self.expr()?);
                }
                let body = self.stmts_until(&|p| p.is_kw("loop"))?;
                self.expect_kw("loop")?;
                let mut post = false;
                if cond.is_none() && (self.is_kw("while") || self.is_kw("until")) {
                    until = self.is_kw("until");
                    self.i += 1;
                    cond = Some(self.expr()?);
                    post = true;
                }
                Stmt::Loop {
                    cond,
                    post,
                    until,
                    body,
                }
            }
            "while" => {
                self.i += 1;
                let c = self.expr()?;
                let body = self.stmts_until(&|p| p.is_kw("wend"))?;
                self.expect_kw("wend")?;
                Stmt::Loop {
                    cond: Some(c),
                    post: false,
                    until: false,
                    body,
                }
            }
            "select" => {
                self.i += 1;
                self.expect_kw("case")?;
                let subject = self.expr()?;
                let mut cases = Vec::new();
                let mut other = None;
                loop {
                    self.skip_ends();
                    if self.is_kw("end") && self.is_kw_at(1, "select") {
                        self.i += 2;
                        break;
                    }
                    self.expect_kw("case")?;
                    let stop = |p: &P| p.is_kw("case") || p.is_kw("end") && p.is_kw_at(1, "select");
                    if self.eat_kw("else") {
                        other = Some(self.stmts_until(&stop)?);
                        continue;
                    }
                    let mut tests = Vec::new();
                    loop {
                        if self.eat_kw("is") {
                            let op = self.cmp_op().ok_or_else(|| {
                                err(self.line(), "comparaison attendue après Case Is")
                            })?;
                            tests.push(CaseTest::Is(op, self.concat()?));
                        } else {
                            let a = self.expr()?;
                            if self.eat_kw("to") {
                                tests.push(CaseTest::Range(a, self.expr()?));
                            } else {
                                tests.push(CaseTest::Value(a));
                            }
                        }
                        if !self.eat_op(",") {
                            break;
                        }
                    }
                    let body = self.stmts_until(&stop)?;
                    cases.push((tests, body));
                }
                Stmt::Select(subject, cases, other)
            }
            "with" => {
                self.i += 1;
                let e = self.expr()?;
                let body = self.stmts_until(&|p| p.is_kw("end") && p.is_kw_at(1, "with"))?;
                self.end_block("with")?;
                Stmt::With(e, body)
            }
            "exit" => {
                self.i += 1;
                let what = self.name()?;
                match what.as_str() {
                    "do" | "for" => Stmt::Break,
                    "function" | "sub" | "property" => Stmt::Return(None),
                    _ => return Err(err(line, format!("Exit {what} inconnu"))),
                }
            }
            "on" => {
                self.i += 1;
                self.expect_kw("error")?;
                if self.eat_kw("resume") {
                    self.expect_kw("next")?;
                    Stmt::OnError(true)
                } else {
                    self.expect_kw("goto")?;
                    match self.peek() {
                        T::Num(n) if *n == 0.0 => {
                            self.i += 1;
                            Stmt::OnError(false)
                        }
                        _ => {
                            return Err(err(
                                line,
                                "seul « On Error GoTo 0 » est permis en VBScript",
                            ))
                        }
                    }
                }
            }
            "function" | "sub" => {
                self.i += 1;
                let name = self.name()?;
                let mut params = Vec::new();
                if self.eat_op("(") && !self.eat_op(")") {
                    loop {
                        while self.eat_kw("byval")
                            || self.eat_kw("byref")
                            || self.eat_kw("optional")
                        {}
                        params.push(self.name()?);
                        if self.eat_op("(") {
                            self.expect_op(")")?;
                        }
                        if self.eat_op(")") {
                            break;
                        }
                        self.expect_op(",")?;
                    }
                }
                let kind = word.clone();
                let body = self.stmts_until(&|p| p.is_kw("end") && p.is_kw_at(1, &kind))?;
                self.end_block(&word)?;
                Stmt::Func(Rc::new(FuncDef {
                    name,
                    params,
                    body,
                    is_sub: word == "sub",
                }))
            }
            "class" => {
                return Err(err(
                    line,
                    "les classes VBScript ne sont pas prises en charge",
                ))
            }
            _ => self.simple()?,
        };
        Ok(s)
    }

    /// Affectation, ou appel de procédure avec ou sans parenthèses.
    fn simple(&mut self) -> PR<Stmt> {
        let target = self.postfix()?;
        if self.eat_op("=") {
            let v = self.expr()?;
            return Ok(Stmt::Assign(target, v));
        }
        if self.at_end() || self.is_kw("else") {
            return Ok(Stmt::Expr(target));
        }
        // `Proc a, b` ou `Proc (a), b`.
        let (callee, mut args) = match target {
            Expr::Call(f, a) if a.len() == 1 && self.is_op(",") => (*f, a),
            other => (other, Vec::new()),
        };
        if !args.is_empty() {
            self.i += 1; // `,`
        }
        loop {
            if self.is_op(",") {
                args.push(Expr::Lit(V::Empty));
            } else {
                args.push(self.expr()?);
            }
            if !self.eat_op(",") {
                break;
            }
        }
        Ok(Stmt::Expr(Expr::Call(Box::new(callee), args)))
    }

    fn if_stmt(&mut self) -> PR<Stmt> {
        self.i += 1; // if
        let c = self.expr()?;
        self.expect_kw("then")?;
        if !self.at_end() {
            // Forme sur une ligne : `If c Then a : b Else d`.
            let mut then = Vec::new();
            while !self.at_end() && !self.is_kw("else") {
                then.push(self.stmt()?);
                if matches!(self.peek(), T::End)
                    && !matches!(self.peek_at(1), T::Eof)
                    && self.t[self.i + 1].line == self.t[self.i].line
                {
                    self.i += 1; // `:` sur la même ligne
                    continue;
                }
                break;
            }
            let mut other = None;
            if self.eat_kw("else") {
                let mut e = Vec::new();
                while !self.at_end() {
                    e.push(self.stmt()?);
                    if matches!(self.peek(), T::End)
                        && !matches!(self.peek_at(1), T::Eof)
                        && self.t[self.i + 1].line == self.t[self.i].line
                    {
                        self.i += 1;
                        continue;
                    }
                    break;
                }
                other = Some(e);
            }
            // `End If` toléré en fin de ligne.
            if self.is_kw("end") && self.is_kw_at(1, "if") {
                self.i += 2;
            }
            return Ok(Stmt::If(vec![(c, then)], other));
        }
        let stop =
            |p: &P| p.is_kw("elseif") || p.is_kw("else") || p.is_kw("end") && p.is_kw_at(1, "if");
        let body = self.stmts_until(&stop)?;
        let mut arms = vec![(c, body)];
        let mut other = None;
        loop {
            if self.eat_kw("elseif") {
                let c = self.expr()?;
                self.expect_kw("then")?;
                let body = self.stmts_until(&stop)?;
                arms.push((c, body));
            } else if self.eat_kw("else") {
                if self.is_kw("if") {
                    // `Else If` sur une ligne : un If imbriqué.
                    let nested = self.if_stmt()?;
                    other = Some(vec![nested]);
                    let rest = self.stmts_until(&|p| p.is_kw("end") && p.is_kw_at(1, "if"))?;
                    other.as_mut().unwrap().extend(rest);
                } else {
                    other = Some(self.stmts_until(&|p| p.is_kw("end") && p.is_kw_at(1, "if"))?);
                }
            } else {
                self.end_block("if")?;
                break;
            }
        }
        Ok(Stmt::If(arms, other))
    }

    fn for_stmt(&mut self) -> PR<Stmt> {
        self.i += 1; // for
        if self.eat_kw("each") {
            let var = self.name()?;
            self.expect_kw("in")?;
            let coll = self.expr()?;
            let body = self.stmts_until(&|p| p.is_kw("next"))?;
            self.expect_kw("next")?;
            if let T::Id(_) = self.peek() {
                self.i += 1;
            }
            return Ok(Stmt::ForEach {
                var: Expr::Ident(var),
                coll,
                keys: false,
                body,
            });
        }
        let var = self.name()?;
        self.expect_op("=")?;
        let from = self.expr()?;
        self.expect_kw("to")?;
        let to = self.expr()?;
        let step = if self.eat_kw("step") {
            Some(self.expr()?)
        } else {
            None
        };
        let body = self.stmts_until(&|p| p.is_kw("next"))?;
        self.expect_kw("next")?;
        if let T::Id(_) = self.peek() {
            self.i += 1;
        }
        Ok(Stmt::ForNum {
            var,
            from,
            to,
            step,
            body,
        })
    }

    // ---------- expressions ----------

    fn expr(&mut self) -> PR<Expr> {
        self.logic(0)
    }

    fn logic(&mut self, level: usize) -> PR<Expr> {
        const LEVELS: &[(&str, BOp)] = &[
            ("imp", BOp::VbImp),
            ("eqv", BOp::VbEqv),
            ("xor", BOp::VbXor),
            ("or", BOp::VbOr),
            ("and", BOp::VbAnd),
        ];
        if level == LEVELS.len() {
            return self.not();
        }
        let mut l = self.logic(level + 1)?;
        while self.eat_kw(LEVELS[level].0) {
            let r = self.logic(level + 1)?;
            l = Expr::Bin(LEVELS[level].1, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn not(&mut self) -> PR<Expr> {
        if self.eat_kw("not") {
            let x = self.not()?;
            return Ok(Expr::Unary(UOp::VbNot, Box::new(x)));
        }
        self.compare()
    }

    fn cmp_op(&mut self) -> Option<BOp> {
        let op = match self.peek() {
            T::Op("=") => BOp::Eq,
            T::Op("<>") => BOp::Ne,
            T::Op("<") => BOp::Lt,
            T::Op(">") => BOp::Gt,
            T::Op("<=") => BOp::Le,
            T::Op(">=") => BOp::Ge,
            T::Id(w) if w == "is" => BOp::Is,
            _ => return None,
        };
        self.i += 1;
        Some(op)
    }

    fn compare(&mut self) -> PR<Expr> {
        let mut l = self.concat()?;
        while let Some(op) = self.cmp_op() {
            let r = self.concat()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn concat(&mut self) -> PR<Expr> {
        let mut l = self.additive()?;
        while self.eat_op("&") {
            let r = self.additive()?;
            l = Expr::Bin(BOp::Concat, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn additive(&mut self) -> PR<Expr> {
        let mut l = self.modulo()?;
        loop {
            let op = if self.eat_op("+") {
                BOp::Add
            } else if self.eat_op("-") {
                BOp::Sub
            } else {
                break;
            };
            let r = self.modulo()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn modulo(&mut self) -> PR<Expr> {
        let mut l = self.intdiv()?;
        while self.eat_kw("mod") {
            let r = self.intdiv()?;
            l = Expr::Bin(BOp::Mod, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn intdiv(&mut self) -> PR<Expr> {
        let mut l = self.mul()?;
        while self.eat_op("\\") {
            let r = self.mul()?;
            l = Expr::Bin(BOp::IntDiv, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn mul(&mut self) -> PR<Expr> {
        let mut l = self.neg()?;
        loop {
            let op = if self.eat_op("*") {
                BOp::Mul
            } else if self.eat_op("/") {
                BOp::Div
            } else {
                break;
            };
            let r = self.neg()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn neg(&mut self) -> PR<Expr> {
        if self.eat_op("-") {
            let x = self.neg()?;
            return Ok(Expr::Unary(UOp::Neg, Box::new(x)));
        }
        if self.eat_op("+") {
            return self.neg();
        }
        self.pow()
    }

    fn pow(&mut self) -> PR<Expr> {
        let mut l = self.postfix()?;
        while self.eat_op("^") {
            let r = if self.eat_op("-") {
                Expr::Unary(UOp::Neg, Box::new(self.postfix()?))
            } else {
                self.postfix()?
            };
            l = Expr::Bin(BOp::Pow, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn args(&mut self) -> PR<Vec<Expr>> {
        let mut out = Vec::new();
        if self.eat_op(")") {
            return Ok(out);
        }
        loop {
            if self.is_op(",") || self.is_op(")") {
                out.push(Expr::Lit(V::Empty));
            } else {
                out.push(self.expr()?);
            }
            if self.eat_op(")") {
                return Ok(out);
            }
            self.expect_op(",")?;
        }
    }

    fn postfix(&mut self) -> PR<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.is_op(".") && matches!(self.peek_at(1), T::Id(_)) {
                self.i += 1;
                let n = self.name()?;
                e = Expr::Member(Box::new(e), n);
            } else if self.eat_op("(") {
                let args = self.args()?;
                e = Expr::Call(Box::new(e), args);
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> PR<Expr> {
        let line = self.line();
        let t = self.peek().clone();
        Ok(match t {
            T::Num(n) => {
                self.i += 1;
                Expr::Lit(V::Num(n))
            }
            T::Str(s) => {
                self.i += 1;
                Expr::Lit(V::str(s))
            }
            T::Op("(") => {
                self.i += 1;
                let e = self.expr()?;
                self.expect_op(")")?;
                e
            }
            T::Op(".") => {
                self.i += 1;
                let n = self.name()?;
                Expr::WithRef(n)
            }
            T::Id(w) => {
                self.i += 1;
                match w.as_str() {
                    "true" => Expr::Lit(V::Bool(true)),
                    "false" => Expr::Lit(V::Bool(false)),
                    "empty" => Expr::Lit(V::Empty),
                    "null" => Expr::Lit(V::Null),
                    "nothing" => Expr::Lit(V::Nothing),
                    "new" => {
                        let n = self.name()?;
                        Expr::New(Box::new(Expr::Ident(n)), Vec::new())
                    }
                    k if RESERVED.contains(&k) => {
                        return Err(err(line, format!("« {k} » inattendu")))
                    }
                    _ => Expr::Ident(w),
                }
            }
            _ => {
                return Err(err(
                    line,
                    format!("expression attendue, trouvé {}", self.describe()),
                ))
            }
        })
    }
}

pub fn parse(src: &str) -> Result<Vec<Stmt>, SyntaxError> {
    let t = lex(src)?;
    P { t, i: 0 }.program()
}
