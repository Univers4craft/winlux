//! Préprocesseur HLSL : `#define` (objets et fonctions, `#`, `##`),
//! `#undef`, `#if`/`#ifdef`/`#ifndef`/`#elif`/`#else`/`#endif`, `#include`
//! (par une fonction de rappel), `#pragma pack_matrix`, `#error`.

use super::lex::{strip_comments, tokenize, Tok, Token};
use std::collections::HashMap;

#[derive(Clone)]
struct Macro {
    params: Option<Vec<String>>,
    body: Vec<Token>,
}

/// Rappel d'inclusion : (nom demandé, inclusion système) → texte.
pub(crate) type Includer<'b> = dyn FnMut(&str, bool) -> Option<String> + 'b;

pub(crate) struct Pre<'a, 'b> {
    macros: HashMap<String, Macro>,
    include: &'a mut Includer<'b>,
    depth: u32,
    out: Vec<Token>,
}

pub(crate) fn preprocess(
    src: &str,
    defines: &[(String, String)],
    include: &mut Includer<'_>,
) -> Result<Vec<Token>, String> {
    let mut p = Pre {
        macros: HashMap::new(),
        include,
        depth: 0,
        out: Vec::new(),
    };
    for (k, v) in defines {
        let body = tokenize(v, 0)?;
        p.macros.insert(k.clone(), Macro { params: None, body });
    }
    p.file(src)?;
    Ok(p.out)
}

impl Pre<'_, '_> {
    fn file(&mut self, src: &str) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 32 {
            return Err("inclusions trop profondes".into());
        }
        let text = strip_comments(src);
        // Lignes logiques (continuations « \ » jointes), numéro de la première.
        let mut lines: Vec<(u32, String)> = Vec::new();
        let mut cur = String::new();
        let mut start = 1u32;
        for (n, l) in text.split('\n').enumerate() {
            let l = l.strip_suffix('\r').unwrap_or(l);
            if cur.is_empty() {
                start = n as u32 + 1;
            }
            if let Some(s) = l.strip_suffix('\\') {
                cur.push_str(s);
                cur.push(' ');
                continue;
            }
            cur.push_str(l);
            lines.push((start, std::mem::take(&mut cur)));
        }
        // Pile des conditions : (actif, une branche déjà prise, parent actif).
        let mut stack: Vec<(bool, bool, bool)> = Vec::new();
        let active = |s: &Vec<(bool, bool, bool)>| s.last().is_none_or(|c| c.0);
        let mut i = 0;
        while i < lines.len() {
            let (line, ref l) = lines[i];
            let l = l.clone();
            i += 1;
            let t = l.trim_start();
            if let Some(d) = t.strip_prefix('#') {
                let d = d.trim_start();
                let (word, rest) = d
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .map_or((d, ""), |p| (&d[..p], &d[p..]));
                let on = active(&stack);
                match word {
                    "if" | "ifdef" | "ifndef" => {
                        let v = if !on {
                            false
                        } else if word == "if" {
                            self.eval(rest, line)?
                        } else {
                            let name = rest.trim();
                            let def = self.macros.contains_key(name);
                            if word == "ifdef" {
                                def
                            } else {
                                !def
                            }
                        };
                        stack.push((on && v, v, on));
                    }
                    "elif" => {
                        let Some(top) = stack.last().copied() else {
                            return Err(format!("({line}) : #elif sans #if"));
                        };
                        let v = top.2 && !top.1 && self.eval(rest, line)?;
                        let n = stack.len() - 1;
                        stack[n] = (v, top.1 || v, top.2);
                    }
                    "else" => {
                        let Some(top) = stack.last().copied() else {
                            return Err(format!("({line}) : #else sans #if"));
                        };
                        let n = stack.len() - 1;
                        stack[n] = (top.2 && !top.1, true, top.2);
                    }
                    "endif" => {
                        if stack.pop().is_none() {
                            return Err(format!("({line}) : #endif sans #if"));
                        }
                    }
                    _ if !on => {}
                    "define" => self.define(rest, line)?,
                    "undef" => {
                        self.macros.remove(rest.trim());
                    }
                    "include" => {
                        let r = rest.trim();
                        let (name, system) = if let Some(s) = r.strip_prefix('"') {
                            (s.trim_end_matches('"'), false)
                        } else if let Some(s) = r.strip_prefix('<') {
                            (s.trim_end_matches('>'), true)
                        } else {
                            return Err(format!("({line}) : #include mal formé"));
                        };
                        let Some(text) = (self.include)(name, system) else {
                            return Err(format!("({line}) : fichier inclus introuvable : {name}"));
                        };
                        self.file(&text)?;
                    }
                    "pragma" => {
                        let r: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
                        if let Some(m) = r.strip_prefix("pack_matrix(") {
                            self.out.push(Token {
                                t: Tok::PackMatrix(m.starts_with("row_major")),
                                line,
                            });
                        }
                    }
                    "error" => return Err(format!("({line}) : #error{rest}")),
                    "line" | "" | "warning" => {}
                    _ => return Err(format!("({line}) : directive #{word} inconnue")),
                }
                continue;
            }
            if !active(&stack) {
                continue;
            }
            let mut toks = tokenize(&l, line)?;
            // Appel de macro dont les arguments continuent sur les lignes suivantes.
            while unbalanced_call(&toks, &self.macros) && i < lines.len() {
                let (n, ref next) = lines[i];
                if next.trim_start().starts_with('#') {
                    break;
                }
                toks.extend(tokenize(next, n)?);
                i += 1;
            }
            let mut off = Vec::new();
            let e = self.expand(&toks, &mut off)?;
            self.out.extend(e);
        }
        if !stack.is_empty() {
            return Err("#if sans #endif".into());
        }
        self.depth -= 1;
        Ok(())
    }

    fn define(&mut self, rest: &str, line: u32) -> Result<(), String> {
        let rest = rest.trim_start();
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let name = rest[..end].to_string();
        if name.is_empty() {
            return Err(format!("({line}) : #define sans nom"));
        }
        let after = &rest[end..];
        if let Some(p) = after.strip_prefix('(') {
            let close = p
                .find(')')
                .ok_or_else(|| format!("({line}) : #define {name}( non fermé"))?;
            let params: Vec<String> = p[..close]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let body = tokenize(&p[close + 1..], line)?;
            self.macros.insert(
                name,
                Macro {
                    params: Some(params),
                    body,
                },
            );
        } else {
            let body = tokenize(after, line)?;
            self.macros.insert(name, Macro { params: None, body });
        }
        Ok(())
    }

    /// Développe les macros d'une suite de jetons.
    fn expand(&self, toks: &[Token], off: &mut Vec<String>) -> Result<Vec<Token>, String> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let tk = &toks[i];
            let Tok::Ident(name) = &tk.t else {
                out.push(tk.clone());
                i += 1;
                continue;
            };
            let Some(m) = self.macros.get(name).filter(|_| !off.contains(name)) else {
                out.push(tk.clone());
                i += 1;
                continue;
            };
            match &m.params {
                None => {
                    off.push(name.clone());
                    let body: Vec<Token> = m
                        .body
                        .iter()
                        .map(|t| Token {
                            t: t.t.clone(),
                            line: tk.line,
                        })
                        .collect();
                    let body = paste(&body);
                    out.extend(self.expand(&body, off)?);
                    off.pop();
                    i += 1;
                }
                Some(params) => {
                    if toks.get(i + 1).map(|t| &t.t) != Some(&Tok::Punct("(")) {
                        out.push(tk.clone());
                        i += 1;
                        continue;
                    }
                    let (args, next) = collect_args(toks, i + 2)
                        .ok_or_else(|| format!("({}) : appel de {name} non fermé", tk.line))?;
                    let mut body = Vec::new();
                    let mut j = 0;
                    while j < m.body.len() {
                        let b = &m.body[j];
                        // « #param » : chaîne.
                        if b.t == Tok::Punct("#") {
                            if let Some(Tok::Ident(p)) = m.body.get(j + 1).map(|t| &t.t) {
                                if let Some(k) = params.iter().position(|x| x == p) {
                                    let s: Vec<String> = args
                                        .get(k)
                                        .map(|a| a.iter().map(tok_text).collect())
                                        .unwrap_or_default();
                                    body.push(Token {
                                        t: Tok::Str(s.join(" ")),
                                        line: tk.line,
                                    });
                                    j += 2;
                                    continue;
                                }
                            }
                        }
                        if let Tok::Ident(p) = &b.t {
                            if let Some(k) = params.iter().position(|x| x == p) {
                                let raw = args.get(k).cloned().unwrap_or_default();
                                let pasted = j > 0 && m.body[j - 1].t == Tok::Punct("##")
                                    || m.body.get(j + 1).map(|t| &t.t) == Some(&Tok::Punct("##"));
                                if pasted {
                                    body.extend(raw);
                                } else {
                                    body.extend(self.expand(&raw, off)?);
                                }
                                j += 1;
                                continue;
                            }
                        }
                        body.push(Token {
                            t: b.t.clone(),
                            line: tk.line,
                        });
                        j += 1;
                    }
                    let body = paste(&body);
                    off.push(name.clone());
                    out.extend(self.expand(&body, off)?);
                    off.pop();
                    i = next;
                }
            }
        }
        Ok(out)
    }

    /// Évalue l'expression d'un `#if`.
    fn eval(&self, expr: &str, line: u32) -> Result<bool, String> {
        let toks = tokenize(expr, line)?;
        // defined(X) / defined X avant tout développement.
        let mut pre = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            if toks[i].t == Tok::Ident("defined".into()) {
                let (name, n) = match (toks.get(i + 1).map(|t| &t.t), toks.get(i + 2)) {
                    (
                        Some(Tok::Punct("(")),
                        Some(Token {
                            t: Tok::Ident(n), ..
                        }),
                    ) => (n.clone(), 4),
                    (Some(Tok::Ident(n)), _) => (n.clone(), 2),
                    _ => return Err(format!("({line}) : defined mal formé")),
                };
                pre.push(Token {
                    t: Tok::Int(self.macros.contains_key(&name) as u64, false),
                    line,
                });
                i += n;
                continue;
            }
            pre.push(toks[i].clone());
            i += 1;
        }
        let mut off = Vec::new();
        let e = self.expand(&pre, &mut off)?;
        let mut p = 0;
        let v = cond_expr(&e, &mut p, 0)?;
        Ok(v != 0)
    }
}

fn tok_text(t: &Token) -> String {
    match &t.t {
        Tok::Ident(s) => s.clone(),
        Tok::Int(v, u) => format!("{v}{}", if *u { "u" } else { "" }),
        Tok::Float(f) => format!("{f:?}"),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Punct(p) => p.to_string(),
        Tok::PackMatrix(_) => String::new(),
    }
}

/// Colle les jetons séparés par `##`.
fn paste(toks: &[Token]) -> Vec<Token> {
    if !toks.iter().any(|t| t.t == Tok::Punct("##")) {
        return toks.to_vec();
    }
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i].t == Tok::Punct("##") && !out.is_empty() && i + 1 < toks.len() {
            let a = out.pop().unwrap();
            let s = tok_text(&a) + &tok_text(&toks[i + 1]);
            match tokenize(&s, a.line) {
                Ok(v) => out.extend(v),
                Err(_) => out.push(a),
            }
            i += 2;
            continue;
        }
        out.push(toks[i].clone());
        i += 1;
    }
    out
}

fn unbalanced_call(toks: &[Token], macros: &HashMap<String, Macro>) -> bool {
    for (i, t) in toks.iter().enumerate() {
        if let Tok::Ident(n) = &t.t {
            if macros.get(n).is_some_and(|m| m.params.is_some())
                && toks.get(i + 1).map(|t| &t.t) == Some(&Tok::Punct("("))
                && collect_args(toks, i + 2).is_none()
            {
                return true;
            }
        }
    }
    false
}

/// Arguments d'un appel de macro à partir de `i` (après la parenthèse) :
/// (arguments, indice après la parenthèse fermante).
fn collect_args(toks: &[Token], mut i: usize) -> Option<(Vec<Vec<Token>>, usize)> {
    let mut args = vec![Vec::new()];
    let mut depth = 0;
    while i < toks.len() {
        let t = &toks[i];
        match t.t {
            Tok::Punct("(") => depth += 1,
            Tok::Punct(")") if depth == 0 => {
                if args.len() == 1 && args[0].is_empty() {
                    args.clear();
                }
                return Some((args, i + 1));
            }
            Tok::Punct(")") => depth -= 1,
            Tok::Punct(",") if depth == 0 => {
                args.push(Vec::new());
                i += 1;
                continue;
            }
            _ => {}
        }
        args.last_mut().unwrap().push(t.clone());
        i += 1;
    }
    None
}

// ---------- Expressions de #if ----------

fn prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

fn cond_expr(t: &[Token], p: &mut usize, min: u8) -> Result<i64, String> {
    let mut l = unary(t, p)?;
    while let Some(Tok::Punct(op)) = t.get(*p).map(|x| &x.t) {
        if *op == "?" && min == 0 {
            *p += 1;
            let a = cond_expr(t, p, 0)?;
            if t.get(*p).map(|x| &x.t) != Some(&Tok::Punct(":")) {
                return Err("#if : « : » attendu".into());
            }
            *p += 1;
            let b = cond_expr(t, p, 0)?;
            l = if l != 0 { a } else { b };
            continue;
        }
        let Some(pr) = prec(op) else { break };
        if pr <= min {
            break;
        }
        *p += 1;
        let r = cond_expr(t, p, pr)?;
        l = match *op {
            "||" => (l != 0 || r != 0) as i64,
            "&&" => (l != 0 && r != 0) as i64,
            "|" => l | r,
            "^" => l ^ r,
            "&" => l & r,
            "==" => (l == r) as i64,
            "!=" => (l != r) as i64,
            "<" => (l < r) as i64,
            ">" => (l > r) as i64,
            "<=" => (l <= r) as i64,
            ">=" => (l >= r) as i64,
            "<<" => l.wrapping_shl(r as u32),
            ">>" => l.wrapping_shr(r as u32),
            "+" => l.wrapping_add(r),
            "-" => l.wrapping_sub(r),
            "*" => l.wrapping_mul(r),
            "/" if r != 0 => l / r,
            "%" if r != 0 => l % r,
            _ => return Err("#if : division par zéro".into()),
        };
    }
    Ok(l)
}

fn unary(t: &[Token], p: &mut usize) -> Result<i64, String> {
    let Some(tok) = t.get(*p) else {
        return Err("#if : expression incomplète".into());
    };
    *p += 1;
    Ok(match &tok.t {
        Tok::Int(v, _) => *v as i64,
        Tok::Float(f) => *f as i64,
        Tok::Ident(_) => 0,
        Tok::Punct("!") => (unary(t, p)? == 0) as i64,
        Tok::Punct("-") => -unary(t, p)?,
        Tok::Punct("+") => unary(t, p)?,
        Tok::Punct("~") => !unary(t, p)?,
        Tok::Punct("(") => {
            let v = cond_expr(t, p, 0)?;
            if t.get(*p).map(|x| &x.t) != Some(&Tok::Punct(")")) {
                return Err("#if : « ) » attendu".into());
            }
            *p += 1;
            v
        }
        _ => return Err("#if : jeton inattendu".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(src: &str) -> String {
        let mut inc = |_: &str, _: bool| Some("float inc_v;".to_string());
        let t = preprocess(src, &[("EXT".into(), "3".into())], &mut inc).unwrap();
        t.iter().map(tok_text).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn macros_and_conditions() {
        let s = text(
            "#define N 4\n#define SQ(x) ((x)*(x))\n#if N > 2 && defined(EXT)\nfloat a[N]; float b = SQ(N+1);\n#else\nbad\n#endif\n#ifndef EXT\nbad\n#elif EXT == 3\nok\n#endif\n#include \"x.h\"\n",
        );
        assert_eq!(
            s,
            "float a [ 4 ] ; float b = ( ( 4 + 1 ) * ( 4 + 1 ) ) ; ok float inc_v ;"
        );
    }

    #[test]
    fn paste_and_continuation() {
        let s = text("#define CAT(a, b) a ## b\n#define LONG 1 + \\\n 2\nint CAT(x, 1) = LONG;");
        assert_eq!(s, "int x1 = 1 + 2 ;");
    }
}
