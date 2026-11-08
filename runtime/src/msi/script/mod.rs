//! Actions personnalisées script de Windows Installer (#973) : VBScript
//! (types 6, 22, 38, 54) et JScript (5, 21, 37, 53).
//!
//! Interpréteurs maison, écrits d'après les spécifications publiques des
//! deux langages (ECMAScript 3 pour JScript, la référence du langage
//! VBScript) et la documentation publique de l'objet `Session` de Windows
//! Installer ; aucun code d'un autre moteur n'est repris.
//!
//! Comme les actions DLL, un script s'exécute dans un processus à part
//! (`winlux --msi-script-action LANGAGE FICHIER [FONCTION]`) qui partage le
//! fichier de session : propriétés, `DoAction`, base du paquet (vues SQL),
//! enregistrements et messages passent par les mêmes fonctions que
//! msi.dll. Objets disponibles : `Session` (et `Session.Installer`,
//! `Session.Database`, vues, enregistrements), `WScript.Shell`,
//! `Scripting.FileSystemObject`, `Scripting.Dictionary` et `RegExp` ;
//! `MsgBox` est écrit dans le journal (installation sans fenêtre).
//!
//! Valeur de retour de la fonction appelée (documentation de Windows
//! Installer) : 1 (ou rien) succès, 2 abandon par l'utilisateur (1602),
//! 3 échec (1603), 4 suspension (1604). Une erreur de script donne 1720.

pub mod ast;
pub mod host;
mod js;
pub mod regex;
#[cfg(test)]
mod tests;
mod vbs;

use ast::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    JScript,
    VBScript,
}

/// Objet fourni par l'hôte (COM côté Windows) : membres sans casse.
pub trait Host {
    fn type_name(&self) -> &str;
    /// Lecture d'une propriété ou appel d'une méthode. Nom vide : membre
    /// par défaut (`dico("clé")`).
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V>;
    fn put(&self, _it: &mut Interp, name: &str, _args: Vec<V>, _value: V) -> R<()> {
        Err(Ex::no_member(name))
    }
    /// Éléments parcourus par `For Each`.
    fn items(&self, _it: &mut Interp) -> Option<Vec<V>> {
        None
    }
    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct Closure {
    pub def: Rc<FuncDef>,
    pub env: Env,
}

#[derive(Clone)]
pub enum V {
    /// `undefined` de JScript, `Empty` de VBScript.
    Empty,
    Null,
    /// `Nothing` de VBScript.
    Nothing,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Arr(Rc<RefCell<Vec<V>>>),
    /// Objet JScript : propriétés dans l'ordre de création.
    Obj(Rc<RefCell<Vec<(String, V)>>>),
    Host(Rc<dyn Host>),
    Func(Rc<Closure>),
    /// Fonction intégrée (nom).
    Native(&'static str),
}

impl std::fmt::Debug for V {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            V::Empty => write!(f, "Empty"),
            V::Null => write!(f, "Null"),
            V::Nothing => write!(f, "Nothing"),
            V::Bool(b) => write!(f, "{b}"),
            V::Num(n) => write!(f, "{n}"),
            V::Str(s) => write!(f, "{s:?}"),
            V::Arr(a) => write!(f, "{:?}", a.borrow()),
            V::Obj(_) => write!(f, "[objet]"),
            V::Host(h) => write!(f, "[{}]", h.type_name()),
            V::Func(c) => write!(f, "[fonction {}]", c.def.name),
            V::Native(n) => write!(f, "[fonction {n}]"),
        }
    }
}

impl V {
    pub fn str(s: impl AsRef<str>) -> V {
        V::Str(Rc::from(s.as_ref()))
    }
    pub fn arr(items: Vec<V>) -> V {
        V::Arr(Rc::new(RefCell::new(items)))
    }
    pub fn host(h: impl Host + 'static) -> V {
        V::Host(Rc::new(h))
    }
    fn is_object(&self) -> bool {
        matches!(
            self,
            V::Obj(_) | V::Host(_) | V::Arr(_) | V::Func(_) | V::Native(_)
        )
    }
}

/// Exception : valeur lancée par `throw` ou erreur d'exécution numérotée
/// (numéros de VBScript, aussi rendus par `e.number` en JScript).
#[derive(Debug, Clone)]
pub enum Ex {
    Throw(V),
    Err(i32, String),
}

impl Ex {
    pub fn new(number: i32, text: impl Into<String>) -> Ex {
        Ex::Err(number, text.into())
    }
    pub fn no_member(name: &str) -> Ex {
        Ex::new(438, format!("l'objet n'a pas de membre « {name} »"))
    }
    pub fn mismatch(what: &str) -> Ex {
        Ex::new(13, format!("type incompatible : {what}"))
    }
}

pub type R<T> = Result<T, Ex>;

pub type Env = Rc<RefCell<Scope>>;

pub struct Scope {
    vars: HashMap<String, V>,
    parent: Option<Env>,
}

fn new_scope(parent: Option<Env>) -> Env {
    Rc::new(RefCell::new(Scope {
        vars: HashMap::new(),
        parent,
    }))
}

enum Flow {
    Next,
    Break,
    Continue,
    Return(V),
}

/// Erreur de syntaxe, avec la ligne.
#[derive(Debug)]
pub struct SyntaxError {
    pub line: usize,
    pub text: String,
}

/// Nombre d'instructions au-delà duquel le script est arrêté (boucle sans
/// fin dans une installation sans fenêtre).
const MAX_STEPS: u64 = 50_000_000;

pub struct Interp {
    pub lang: Lang,
    global: Env,
    /// `Err` de VBScript : numéro, description, source.
    pub err: (i32, String, String),
    resume: Vec<bool>,
    with: Vec<V>,
    this: Vec<V>,
    steps: u64,
    /// Messages de `MsgBox` et de `Session.Message` (aussi écrits sur la
    /// sortie d'erreur pour le journal du msiexec).
    pub messages: Vec<String>,
    seed: u64,
}

pub fn num_str(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if n == n.trunc() && n.abs() < 1e18 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Nombre écrit dans une chaîne (`12`, ` 3.5 `, `0x1F`, `&H1F`) ; `None`
/// si ce n'en est pas un.
pub fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower
        .strip_prefix("0x")
        .or_else(|| lower.strip_prefix("&h"))
    {
        return i64::from_str_radix(h, 16).ok().map(|v| v as f64);
    }
    if let Some(o) = lower.strip_prefix("&o") {
        return i64::from_str_radix(o, 8).ok().map(|v| v as f64);
    }
    if lower == "infinity" || lower == "+infinity" {
        return Some(f64::INFINITY);
    }
    if lower == "-infinity" {
        return Some(f64::NEG_INFINITY);
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
    {
        return None;
    }
    t.parse::<f64>().ok()
}

fn same_object(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Arr(x), V::Arr(y)) => Rc::ptr_eq(x, y),
        (V::Obj(x), V::Obj(y)) => Rc::ptr_eq(x, y),
        (V::Host(x), V::Host(y)) => Rc::as_ptr(x) as *const u8 == Rc::as_ptr(y) as *const u8,
        (V::Func(x), V::Func(y)) => Rc::ptr_eq(x, y),
        (V::Native(x), V::Native(y)) => x == y,
        (V::Nothing, V::Nothing) => true,
        _ => false,
    }
}

impl Interp {
    pub fn new(lang: Lang) -> Interp {
        let mut it = Interp {
            lang,
            global: new_scope(None),
            err: (0, String::new(), String::new()),
            resume: vec![false],
            with: Vec::new(),
            this: vec![V::Empty],
            steps: 0,
            messages: Vec::new(),
            seed: 0x2545_F491_4F6C_DD1D,
        };
        host::install_globals(&mut it);
        it
    }

    pub fn key(&self, name: &str) -> String {
        match self.lang {
            Lang::VBScript => name.to_ascii_lowercase(),
            Lang::JScript => name.to_string(),
        }
    }

    pub fn define_global(&mut self, name: &str, v: V) {
        let k = self.key(name);
        self.global.borrow_mut().vars.insert(k, v);
    }

    pub fn random(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }

    // ---------- conversions ----------

    pub fn to_str(&self, v: &V) -> String {
        match v {
            V::Empty => match self.lang {
                Lang::JScript => "undefined".into(),
                Lang::VBScript => String::new(),
            },
            V::Null => match self.lang {
                Lang::JScript => "null".into(),
                Lang::VBScript => String::new(),
            },
            V::Nothing => String::new(),
            V::Bool(b) => match (self.lang, b) {
                (Lang::JScript, true) => "true".into(),
                (Lang::JScript, false) => "false".into(),
                (Lang::VBScript, true) => "True".into(),
                (Lang::VBScript, false) => "False".into(),
            },
            V::Num(n) => num_str(*n),
            V::Str(s) => s.to_string(),
            V::Arr(a) => a
                .borrow()
                .iter()
                .map(|x| match x {
                    V::Empty | V::Null => String::new(),
                    o => self.to_str(o),
                })
                .collect::<Vec<_>>()
                .join(","),
            V::Obj(o) => {
                // Objet Error de JScript : son message.
                let o = o.borrow();
                match o.iter().find(|(k, _)| k == "message") {
                    Some((_, m)) => {
                        let name = o
                            .iter()
                            .find(|(k, _)| k == "name")
                            .map(|(_, n)| self.to_str(n))
                            .unwrap_or_else(|| "Error".into());
                        format!("{name}: {}", self.to_str(m))
                    }
                    None => "[object Object]".into(),
                }
            }
            V::Host(h) => h.type_name().to_string(),
            V::Func(c) => format!("function {}() {{ … }}", c.def.name),
            V::Native(n) => format!("function {n}() {{ [native code] }}"),
        }
    }

    pub fn to_num(&self, v: &V) -> R<f64> {
        Ok(match v {
            V::Empty => match self.lang {
                Lang::JScript => f64::NAN,
                Lang::VBScript => 0.0,
            },
            V::Null | V::Nothing => 0.0,
            V::Bool(b) => match (self.lang, b) {
                (_, false) => 0.0,
                (Lang::JScript, true) => 1.0,
                (Lang::VBScript, true) => -1.0,
            },
            V::Num(n) => *n,
            V::Str(s) => match parse_num(s) {
                Some(n) => n,
                None if s.trim().is_empty() && self.lang == Lang::JScript => 0.0,
                None => match self.lang {
                    Lang::JScript => f64::NAN,
                    Lang::VBScript => match s.to_ascii_lowercase().trim() {
                        "true" => -1.0,
                        "false" => 0.0,
                        _ => return Err(Ex::mismatch(&format!("« {s} »"))),
                    },
                },
            },
            V::Arr(_) if self.lang == Lang::JScript => {
                parse_num(&self.to_str(v)).unwrap_or(f64::NAN)
            }
            _ => match self.lang {
                Lang::JScript => f64::NAN,
                Lang::VBScript => return Err(Ex::mismatch("objet")),
            },
        })
    }

    pub fn to_int(&self, v: &V) -> R<i64> {
        let n = self.to_num(v)?;
        Ok(if n.is_finite() {
            match self.lang {
                // VBScript arrondit au pair le plus proche.
                Lang::VBScript => round_even(n) as i64,
                Lang::JScript => n.trunc() as i64,
            }
        } else {
            0
        })
    }

    fn to_i32(&self, v: &V) -> R<i32> {
        let n = self.to_num(v)?;
        Ok(if n.is_finite() {
            match self.lang {
                Lang::VBScript => round_even(n) as i64 as i32,
                Lang::JScript => (n.trunc() as i64 as u64 & 0xFFFF_FFFF) as u32 as i32,
            }
        } else {
            0
        })
    }

    pub fn truthy(&self, v: &V) -> R<bool> {
        Ok(match v {
            V::Empty | V::Null | V::Nothing => false,
            V::Bool(b) => *b,
            V::Num(n) => *n != 0.0 && !n.is_nan(),
            V::Str(s) => match self.lang {
                Lang::JScript => !s.is_empty(),
                Lang::VBScript => self.to_num(v)? != 0.0,
            },
            _ => match self.lang {
                Lang::JScript => true,
                Lang::VBScript => return Err(Ex::mismatch("objet utilisé comme booléen")),
            },
        })
    }

    // ---------- comparaisons ----------

    fn loose_eq(&self, a: &V, b: &V) -> R<bool> {
        Ok(match (a, b) {
            (V::Empty | V::Null, V::Empty | V::Null) => true,
            (V::Empty | V::Null, _) | (_, V::Empty | V::Null) => false,
            (V::Str(x), V::Str(y)) => x == y,
            (V::Num(_) | V::Bool(_) | V::Str(_), V::Num(_) | V::Bool(_) | V::Str(_)) => {
                self.to_num(a)? == self.to_num(b)?
            }
            (x, y) if x.is_object() && y.is_object() => same_object(x, y),
            (x, y) => self.to_str(x) == self.to_str(y),
        })
    }

    fn strict_eq(&self, a: &V, b: &V) -> bool {
        match (a, b) {
            (V::Empty, V::Empty) | (V::Null, V::Null) => true,
            (V::Bool(x), V::Bool(y)) => x == y,
            (V::Num(x), V::Num(y)) => x == y,
            (V::Str(x), V::Str(y)) => x == y,
            (x, y) => same_object(x, y),
        }
    }

    /// Comparaison de VBScript : chaînes entre elles (binaire), sinon
    /// nombres ; -1, 0 ou 1.
    fn vb_cmp(&self, a: &V, b: &V) -> R<std::cmp::Ordering> {
        use std::cmp::Ordering;
        let str_side = |v: &V| matches!(v, V::Str(_));
        if str_side(a) && str_side(b)
            || str_side(a) && matches!(b, V::Empty)
            || str_side(b) && matches!(a, V::Empty)
        {
            return Ok(self.to_str(a).cmp(&self.to_str(b)));
        }
        if a.is_object() || b.is_object() {
            return Err(Ex::mismatch("comparaison d'objets (utiliser Is)"));
        }
        // Nombre et chaîne non numérique : le nombre est plus petit.
        match (a, b) {
            (V::Str(s), _) if parse_num(s).is_none() => return Ok(Ordering::Greater),
            (_, V::Str(s)) if parse_num(s).is_none() => return Ok(Ordering::Less),
            _ => {}
        }
        let (x, y) = (self.to_num(a)?, self.to_num(b)?);
        Ok(x.partial_cmp(&y).unwrap_or(Ordering::Equal))
    }

    fn js_less(&self, a: &V, b: &V) -> R<Option<bool>> {
        if let (V::Str(x), V::Str(y)) = (a, b) {
            return Ok(Some(x.encode_utf16().lt(y.encode_utf16())));
        }
        let (x, y) = (self.to_num(a)?, self.to_num(b)?);
        Ok(if x.is_nan() || y.is_nan() {
            None
        } else {
            Some(x < y)
        })
    }

    fn binary(&mut self, op: BOp, a: V, b: V) -> R<V> {
        use std::cmp::Ordering::*;
        let vb = self.lang == Lang::VBScript;
        if vb
            && matches!(
                op,
                BOp::Add
                    | BOp::Sub
                    | BOp::Mul
                    | BOp::Div
                    | BOp::Lt
                    | BOp::Gt
                    | BOp::Le
                    | BOp::Ge
                    | BOp::Eq
                    | BOp::Ne
            )
            && (matches!(a, V::Null) || matches!(b, V::Null))
        {
            return Ok(V::Null);
        }
        Ok(match op {
            BOp::Add => {
                if vb {
                    if let (V::Str(x), V::Str(y)) = (&a, &b) {
                        V::str(format!("{x}{y}"))
                    } else {
                        V::Num(self.to_num(&a)? + self.to_num(&b)?)
                    }
                } else if matches!(a, V::Str(_) | V::Obj(_) | V::Arr(_) | V::Host(_))
                    || matches!(b, V::Str(_) | V::Obj(_) | V::Arr(_) | V::Host(_))
                {
                    V::str(format!("{}{}", self.to_str(&a), self.to_str(&b)))
                } else {
                    V::Num(self.to_num(&a)? + self.to_num(&b)?)
                }
            }
            BOp::Sub => V::Num(self.to_num(&a)? - self.to_num(&b)?),
            BOp::Mul => V::Num(self.to_num(&a)? * self.to_num(&b)?),
            BOp::Div => {
                let d = self.to_num(&b)?;
                if vb && d == 0.0 {
                    return Err(Ex::new(11, "division par zéro"));
                }
                V::Num(self.to_num(&a)? / d)
            }
            BOp::IntDiv => {
                let d = self.to_int(&b)?;
                if d == 0 {
                    return Err(Ex::new(11, "division par zéro"));
                }
                V::Num((self.to_int(&a)? / d) as f64)
            }
            BOp::Mod => {
                if vb {
                    let d = self.to_int(&b)?;
                    if d == 0 {
                        return Err(Ex::new(11, "division par zéro"));
                    }
                    V::Num((self.to_int(&a)? % d) as f64)
                } else {
                    V::Num(self.to_num(&a)? % self.to_num(&b)?)
                }
            }
            BOp::Pow => V::Num(self.to_num(&a)?.powf(self.to_num(&b)?)),
            BOp::Concat => V::str(format!("{}{}", self.to_str(&a), self.to_str(&b))),
            BOp::Eq | BOp::Ne => {
                let eq = if vb {
                    self.vb_cmp(&a, &b)? == Equal
                } else {
                    self.loose_eq(&a, &b)?
                };
                V::Bool(eq == (op == BOp::Eq))
            }
            BOp::StrictEq => V::Bool(self.strict_eq(&a, &b)),
            BOp::StrictNe => V::Bool(!self.strict_eq(&a, &b)),
            BOp::Lt | BOp::Gt | BOp::Le | BOp::Ge => {
                if vb {
                    let o = self.vb_cmp(&a, &b)?;
                    V::Bool(match op {
                        BOp::Lt => o == Less,
                        BOp::Gt => o == Greater,
                        BOp::Le => o != Greater,
                        _ => o != Less,
                    })
                } else {
                    V::Bool(match op {
                        BOp::Lt => self.js_less(&a, &b)? == Some(true),
                        BOp::Gt => self.js_less(&b, &a)? == Some(true),
                        BOp::Le => self.js_less(&b, &a)? == Some(false),
                        _ => self.js_less(&a, &b)? == Some(false),
                    })
                }
            }
            BOp::Is => V::Bool(same_object(&a, &b) || matches!((&a, &b), (V::Nothing, V::Nothing))),
            BOp::BitAnd => V::Num((self.to_i32(&a)? & self.to_i32(&b)?) as f64),
            BOp::BitOr => V::Num((self.to_i32(&a)? | self.to_i32(&b)?) as f64),
            BOp::BitXor => V::Num((self.to_i32(&a)? ^ self.to_i32(&b)?) as f64),
            BOp::Shl => {
                V::Num((self.to_i32(&a)?.wrapping_shl(self.to_i32(&b)? as u32 & 31)) as f64)
            }
            BOp::Shr => V::Num((self.to_i32(&a)? >> (self.to_i32(&b)? as u32 & 31)) as f64),
            BOp::UShr => {
                V::Num(((self.to_i32(&a)? as u32) >> (self.to_i32(&b)? as u32 & 31)) as f64)
            }
            BOp::VbAnd | BOp::VbOr | BOp::VbXor | BOp::VbEqv | BOp::VbImp => {
                if let (V::Bool(x), V::Bool(y)) = (&a, &b) {
                    V::Bool(match op {
                        BOp::VbAnd => *x && *y,
                        BOp::VbOr => *x || *y,
                        BOp::VbXor => x != y,
                        BOp::VbEqv => x == y,
                        _ => !*x || *y,
                    })
                } else {
                    let (x, y) = (self.to_i32(&a)?, self.to_i32(&b)?);
                    V::Num(match op {
                        BOp::VbAnd => x & y,
                        BOp::VbOr => x | y,
                        BOp::VbXor => x ^ y,
                        BOp::VbEqv => !(x ^ y),
                        _ => !x | y,
                    } as f64)
                }
            }
            BOp::In => {
                let key = self.to_str(&a);
                V::Bool(match &b {
                    V::Obj(o) => o.borrow().iter().any(|(k, _)| *k == key),
                    V::Arr(x) => {
                        key.parse::<usize>().is_ok_and(|i| i < x.borrow().len()) || key == "length"
                    }
                    _ => return Err(Ex::new(5, "« in » sur une valeur qui n'est pas un objet")),
                })
            }
            BOp::InstanceOf => V::Bool(match (&a, &b) {
                (V::Arr(_), V::Native("Array")) => true,
                (V::Obj(o), V::Native("Error")) => o.borrow().iter().any(|(k, _)| k == "message"),
                (V::Obj(_), V::Native("Object")) => true,
                _ => false,
            }),
        })
    }

    // ---------- variables ----------

    fn lookup(&self, name: &str, env: &Env) -> Option<V> {
        let mut cur = Some(env.clone());
        while let Some(e) = cur {
            if let Some(v) = e.borrow().vars.get(name) {
                return Some(v.clone());
            }
            cur = e.borrow().parent.clone();
        }
        None
    }

    fn set_var(&mut self, name: &str, v: V, env: &Env) {
        let mut cur = Some(env.clone());
        while let Some(e) = cur {
            if let Some(slot) = e.borrow_mut().vars.get_mut(name) {
                *slot = v;
                return;
            }
            cur = e.borrow().parent.clone();
        }
        match self.lang {
            // Variable non déclarée : globale en JScript, locale en VBScript.
            Lang::JScript => self.global.borrow_mut().vars.insert(name.into(), v),
            Lang::VBScript => env.borrow_mut().vars.insert(name.into(), v),
        };
    }

    fn declare(&self, name: &str, v: V, env: &Env) {
        env.borrow_mut().vars.insert(name.into(), v);
    }

    fn hoist(&self, body: &[Stmt], env: &Env) {
        for s in body {
            match s {
                Stmt::Func(def) => self.declare(
                    &def.name,
                    V::Func(Rc::new(Closure {
                        def: def.clone(),
                        env: env.clone(),
                    })),
                    env,
                ),
                Stmt::Var(list) if self.lang == Lang::JScript => {
                    for (n, _, _) in list {
                        if !env.borrow().vars.contains_key(n) {
                            self.declare(n, V::Empty, env);
                        }
                    }
                }
                Stmt::If(arms, other) if self.lang == Lang::JScript => {
                    for (_, b) in arms {
                        self.hoist(b, env);
                    }
                    if let Some(b) = other {
                        self.hoist(b, env);
                    }
                }
                Stmt::Block(b)
                | Stmt::Loop { body: b, .. }
                | Stmt::For { body: b, .. }
                | Stmt::ForEach { body: b, .. }
                    if self.lang == Lang::JScript =>
                {
                    self.hoist(b, env)
                }
                Stmt::Try {
                    body,
                    catch,
                    finally,
                } if self.lang == Lang::JScript => {
                    self.hoist(body, env);
                    if let Some((_, b)) = catch {
                        self.hoist(b, env);
                    }
                    if let Some(b) = finally {
                        self.hoist(b, env);
                    }
                }
                _ => {}
            }
        }
    }

    // ---------- instructions ----------

    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(Ex::new(
                28,
                "script arrêté : trop d'instructions (boucle sans fin ?)",
            ));
        }
        Ok(())
    }

    fn block(&mut self, body: &[Stmt], env: &Env) -> R<Flow> {
        for s in body {
            match self.exec(s, env) {
                Ok(Flow::Next) => {}
                Ok(f) => return Ok(f),
                Err(e) => {
                    if self.lang == Lang::VBScript && *self.resume.last().unwrap_or(&false) {
                        self.set_err(&e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(Flow::Next)
    }

    pub fn set_err(&mut self, e: &Ex) {
        self.err = match e {
            Ex::Err(n, t) => (*n, t.clone(), "Erreur d'exécution".into()),
            Ex::Throw(v) => (1, self.to_str(v), "Erreur d'exécution".into()),
        };
    }

    fn exec(&mut self, s: &Stmt, env: &Env) -> R<Flow> {
        self.tick()?;
        match s {
            Stmt::Empty | Stmt::Func(_) => {}
            Stmt::Expr(e) => {
                self.eval_stmt_expr(e, env)?;
            }
            Stmt::Var(list) => {
                for (name, init, dims) in list {
                    let v = match (init, dims) {
                        (Some(e), _) => self.eval(e, env)?,
                        (None, Some(d)) => {
                            let sizes = d
                                .iter()
                                .map(|e| self.eval(e, env).and_then(|v| self.to_int(&v)))
                                .collect::<R<Vec<_>>>()?;
                            make_array(&sizes)
                        }
                        (None, None) => {
                            if self.lang == Lang::JScript {
                                continue; // `var x;` ne remet pas x à undefined
                            }
                            V::Empty
                        }
                    };
                    if self.lang == Lang::JScript {
                        self.set_var(name, v, env);
                    } else {
                        self.declare(name, v, env);
                    }
                }
            }
            Stmt::ReDim(preserve, name, dims) => {
                let sizes = dims
                    .iter()
                    .map(|e| self.eval(e, env).and_then(|v| self.to_int(&v)))
                    .collect::<R<Vec<_>>>()?;
                let new = make_array(&sizes);
                if *preserve {
                    if let (Some(V::Arr(old)), V::Arr(n)) = (self.lookup(name, env), &new) {
                        let old = old.borrow();
                        let mut n = n.borrow_mut();
                        for (i, v) in old.iter().enumerate().take(n.len()) {
                            n[i] = v.clone();
                        }
                    }
                }
                self.set_var(name, new, env);
            }
            Stmt::Assign(target, value) => {
                let v = self.eval(value, env)?;
                self.assign(target, v, env)?;
            }
            Stmt::If(arms, other) => {
                for (c, body) in arms {
                    let cv = self.eval(c, env)?;
                    if self.truthy(&cv)? {
                        return self.block(body, env);
                    }
                }
                if let Some(body) = other {
                    return self.block(body, env);
                }
            }
            Stmt::Loop {
                cond,
                post,
                until,
                body,
            } => loop {
                if !post {
                    if let Some(c) = cond {
                        let v = self.eval(c, env)?;
                        if self.truthy(&v)? == *until {
                            break;
                        }
                    }
                }
                self.tick()?;
                match self.block(body, env)? {
                    Flow::Break => break,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    Flow::Next | Flow::Continue => {}
                }
                if *post {
                    if let Some(c) = cond {
                        let v = self.eval(c, env)?;
                        if self.truthy(&v)? == *until {
                            break;
                        }
                    }
                }
            },
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                for s in init {
                    self.exec(s, env)?;
                }
                loop {
                    if let Some(c) = cond {
                        let v = self.eval(c, env)?;
                        if !self.truthy(&v)? {
                            break;
                        }
                    }
                    self.tick()?;
                    match self.block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Next | Flow::Continue => {}
                    }
                    if let Some(s) = step {
                        self.eval(s, env)?;
                    }
                }
            }
            Stmt::ForNum {
                var,
                from,
                to,
                step,
                body,
            } => {
                let from = self.eval(from, env)?;
                let start = self.to_num(&from)?;
                let to = self.eval(to, env)?;
                let end = self.to_num(&to)?;
                let step = match step {
                    Some(s) => {
                        let v = self.eval(s, env)?;
                        self.to_num(&v)?
                    }
                    None => 1.0,
                };
                self.set_var(var, V::Num(start), env);
                loop {
                    let cur = self.lookup(var, env).unwrap_or(V::Empty);
                    let i = self.to_num(&cur)?;
                    if step >= 0.0 && i > end || step < 0.0 && i < end {
                        break;
                    }
                    self.tick()?;
                    match self.block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Next | Flow::Continue => {}
                    }
                    let cur = self.lookup(var, env).unwrap_or(V::Empty);
                    let i = self.to_num(&cur)?;
                    self.set_var(var, V::Num(i + step), env);
                }
            }
            Stmt::ForEach {
                var,
                coll,
                keys,
                body,
            } => {
                let c = self.eval(coll, env)?;
                let items: Vec<V> = match (&c, keys) {
                    (V::Arr(a), false) => a.borrow().clone(),
                    (V::Arr(a), true) => (0..a.borrow().len())
                        .map(|i| V::str(i.to_string()))
                        .collect(),
                    (V::Obj(o), true) => o.borrow().iter().map(|(k, _)| V::str(k)).collect(),
                    (V::Obj(o), false) => o.borrow().iter().map(|(_, v)| v.clone()).collect(),
                    (V::Str(s), true) => (0..s.chars().count())
                        .map(|i| V::str(i.to_string()))
                        .collect(),
                    (V::Host(h), _) => {
                        let h = h.clone();
                        h.items(self)
                            .ok_or_else(|| Ex::new(451, "objet non énumérable"))?
                    }
                    (V::Empty | V::Null, true) => Vec::new(),
                    _ => return Err(Ex::new(451, "objet non énumérable")),
                };
                for item in items {
                    self.assign(var, item, env)?;
                    self.tick()?;
                    match self.block(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Next | Flow::Continue => {}
                    }
                }
            }
            Stmt::Switch(subject, cases) => {
                let v = self.eval(subject, env)?;
                let mut start = None;
                for (i, (test, _)) in cases.iter().enumerate() {
                    if let Some(t) = test {
                        let tv = self.eval(t, env)?;
                        if self.strict_eq(&v, &tv) {
                            start = Some(i);
                            break;
                        }
                    }
                }
                let start = start.or_else(|| cases.iter().position(|(t, _)| t.is_none()));
                if let Some(start) = start {
                    for (_, body) in &cases[start..] {
                        match self.block(body, env)? {
                            Flow::Next => {}
                            Flow::Break => break,
                            f => return Ok(f),
                        }
                    }
                }
            }
            Stmt::Select(subject, cases, other) => {
                let v = self.eval(subject, env)?;
                for (tests, body) in cases {
                    let mut hit = false;
                    for t in tests {
                        hit = match t {
                            CaseTest::Value(e) => {
                                let tv = self.eval(e, env)?;
                                self.vb_cmp(&v, &tv)? == std::cmp::Ordering::Equal
                            }
                            CaseTest::Range(a, b) => {
                                let (a, b) = (self.eval(a, env)?, self.eval(b, env)?);
                                self.vb_cmp(&v, &a)? != std::cmp::Ordering::Less
                                    && self.vb_cmp(&v, &b)? != std::cmp::Ordering::Greater
                            }
                            CaseTest::Is(op, e) => {
                                let tv = self.eval(e, env)?;
                                let r = self.binary(*op, v.clone(), tv)?;
                                self.truthy(&r)?
                            }
                        };
                        if hit {
                            break;
                        }
                    }
                    if hit {
                        return self.block(body, env);
                    }
                }
                if let Some(body) = other {
                    return self.block(body, env);
                }
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, env)?,
                    None => V::Empty,
                };
                return Ok(Flow::Return(v));
            }
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Continue => return Ok(Flow::Continue),
            Stmt::Throw(e) => {
                let v = self.eval(e, env)?;
                return Err(Ex::Throw(v));
            }
            Stmt::Try {
                body,
                catch,
                finally,
            } => {
                let r = self.block(body, env);
                let r = match (r, catch) {
                    (Err(e), Some((name, handler))) => {
                        let v = self.error_value(&e);
                        self.declare(name, v, env);
                        self.block(handler, env)
                    }
                    (r, _) => r,
                };
                if let Some(f) = finally {
                    match self.block(f, env)? {
                        Flow::Next => {}
                        other => return Ok(other),
                    }
                }
                return r;
            }
            Stmt::OnError(on) => {
                if let Some(r) = self.resume.last_mut() {
                    *r = *on;
                }
                self.err = (0, String::new(), String::new());
            }
            Stmt::With(e, body) => {
                let v = self.eval(e, env)?;
                self.with.push(v);
                let r = self.block(body, env);
                self.with.pop();
                return r;
            }
            Stmt::Block(b) => return self.block(b, env),
        }
        Ok(Flow::Next)
    }

    /// Valeur d'une exception attrapée par `catch` : un objet Error
    /// (`message`, `description`, `number`, `name`).
    pub fn error_value(&self, e: &Ex) -> V {
        match e {
            Ex::Throw(v) => v.clone(),
            Ex::Err(n, t) => {
                let number = if *n > 0 && *n < 0x10000 {
                    (0x800A_0000u32 as i32 | *n) as f64
                } else {
                    *n as f64
                };
                make_error(t, number)
            }
        }
    }

    /// Instruction-expression : en VBScript, un nom de procédure seul est
    /// un appel (`MaProcedure` ou `obj.Methode`).
    fn eval_stmt_expr(&mut self, e: &Expr, env: &Env) -> R<V> {
        if self.lang == Lang::VBScript {
            match e {
                Expr::Member(..) | Expr::WithRef(_) | Expr::Ident(_) => {
                    return self.call_expr(e, Vec::new(), env);
                }
                _ => {}
            }
        }
        self.eval(e, env)
    }

    // ---------- expressions ----------

    pub fn eval(&mut self, e: &Expr, env: &Env) -> R<V> {
        Ok(match e {
            Expr::Lit(v) => v.clone(),
            Expr::Ident(n) => match self.lookup(n, env) {
                Some(V::Func(c)) if self.lang == Lang::VBScript => {
                    self.call_closure(&c, Vec::new(), V::Empty)?
                }
                Some(V::Native(f)) if self.lang == Lang::VBScript => self.native(f, Vec::new())?,
                Some(v) => v,
                None if self.lang == Lang::VBScript => V::Empty,
                None => return Err(Ex::new(5009, format!("« {n} » n'est pas défini"))),
            },
            Expr::WithRef(n) => {
                let obj = self
                    .with
                    .last()
                    .cloned()
                    .ok_or_else(|| Ex::new(424, "objet requis (With)"))?;
                self.get_member(obj, n, Vec::new())?
            }
            Expr::Member(o, n) => {
                let obj = self.eval(o, env)?;
                self.get_member(obj, n, Vec::new())?
            }
            Expr::Index(o, k) => {
                let obj = self.eval(o, env)?;
                let key = self.eval(k, env)?;
                self.index(obj, key)?
            }
            Expr::Call(f, args) => {
                let args = self.eval_args(args, env)?;
                self.call_expr(f, args, env)?
            }
            Expr::New(f, args) => {
                let args = self.eval_args(args, env)?;
                self.construct(f, args, env)?
            }
            Expr::Unary(op, x) => {
                if *op == UOp::TypeOf {
                    if let Expr::Ident(n) = &**x {
                        if self.lookup(n, env).is_none() {
                            return Ok(V::str("undefined"));
                        }
                    }
                }
                if *op == UOp::Delete {
                    if let Expr::Member(o, _) | Expr::Index(o, _) = &**x {
                        let obj = self.eval(o, env)?;
                        let key = match &**x {
                            Expr::Index(_, k) => {
                                let k = self.eval(k, env)?;
                                self.to_str(&k)
                            }
                            Expr::Member(_, n) => n.clone(),
                            _ => String::new(),
                        };
                        if let V::Obj(o) = obj {
                            o.borrow_mut().retain(|(k, _)| *k != key);
                        }
                    }
                    return Ok(V::Bool(true));
                }
                let v = self.eval(x, env)?;
                match op {
                    UOp::Neg => {
                        if self.lang == Lang::VBScript && matches!(v, V::Null) {
                            V::Null
                        } else {
                            V::Num(-self.to_num(&v)?)
                        }
                    }
                    UOp::Plus => V::Num(self.to_num(&v)?),
                    UOp::Not => V::Bool(!self.truthy(&v)?),
                    UOp::VbNot => match v {
                        V::Bool(b) => V::Bool(!b),
                        V::Null => V::Null,
                        o => V::Num(!self.to_i32(&o)? as f64),
                    },
                    UOp::BitNot => V::Num(!self.to_i32(&v)? as f64),
                    UOp::TypeOf => V::str(match v {
                        V::Empty => "undefined",
                        V::Null | V::Nothing => "object",
                        V::Bool(_) => "boolean",
                        V::Num(_) => "number",
                        V::Str(_) => "string",
                        V::Func(_) | V::Native(_) => "function",
                        V::Arr(_) | V::Obj(_) | V::Host(_) => "object",
                    }),
                    UOp::Void => V::Empty,
                    UOp::Delete => V::Bool(true),
                }
            }
            Expr::Bin(op, a, b) => {
                let a = self.eval(a, env)?;
                let b = self.eval(b, env)?;
                self.binary(*op, a, b)?
            }
            Expr::And(a, b) => {
                let a = self.eval(a, env)?;
                if !self.truthy(&a)? {
                    a
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Or(a, b) => {
                let a = self.eval(a, env)?;
                if self.truthy(&a)? {
                    a
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Cond(c, a, b) => {
                let c = self.eval(c, env)?;
                if self.truthy(&c)? {
                    self.eval(a, env)?
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Assign(target, op, value) => {
                let v = match op {
                    None => self.eval(value, env)?,
                    Some(op) => {
                        let cur = self.eval(target, env)?;
                        let rhs = self.eval(value, env)?;
                        self.binary(*op, cur, rhs)?
                    }
                };
                self.assign(target, v.clone(), env)?;
                v
            }
            Expr::Update(target, delta, prefix) => {
                let cur = self.eval(target, env)?;
                let old = self.to_num(&cur)?;
                self.assign(target, V::Num(old + delta), env)?;
                V::Num(if *prefix { old + delta } else { old })
            }
            Expr::Array(items) => V::arr(self.eval_args(items, env)?),
            Expr::Object(props) => {
                let mut out = Vec::new();
                for (k, e) in props {
                    let v = self.eval(e, env)?;
                    out.push((k.clone(), v));
                }
                V::Obj(Rc::new(RefCell::new(out)))
            }
            Expr::Func(def) => V::Func(Rc::new(Closure {
                def: def.clone(),
                env: env.clone(),
            })),
            Expr::This => self.this.last().cloned().unwrap_or(V::Empty),
            Expr::Regex(p, f) => host::regexp(p, f)?,
            Expr::Seq(items) => {
                let mut last = V::Empty;
                for e in items {
                    last = self.eval(e, env)?;
                }
                last
            }
        })
    }

    fn eval_args(&mut self, args: &[Expr], env: &Env) -> R<Vec<V>> {
        args.iter().map(|a| self.eval(a, env)).collect()
    }

    fn index(&mut self, obj: V, key: V) -> R<V> {
        match &obj {
            V::Arr(a) => {
                if let V::Num(n) = key {
                    return Ok(a.borrow().get(n as usize).cloned().unwrap_or(V::Empty));
                }
                let k = self.to_str(&key);
                if let Ok(i) = k.parse::<usize>() {
                    return Ok(a.borrow().get(i).cloned().unwrap_or(V::Empty));
                }
                self.get_member(obj.clone(), &k, Vec::new())
            }
            V::Str(s) => {
                let k = self.to_str(&key);
                match k.parse::<usize>() {
                    Ok(i) => Ok(s
                        .chars()
                        .nth(i)
                        .map(|c| V::str(c.to_string()))
                        .unwrap_or(V::Empty)),
                    Err(_) => self.get_member(obj.clone(), &k, Vec::new()),
                }
            }
            V::Host(h) => {
                let h = h.clone();
                h.get(self, "", vec![key])
            }
            _ => {
                let k = self.to_str(&key);
                self.get_member(obj, &k, Vec::new())
            }
        }
    }

    pub fn get_member(&mut self, obj: V, name: &str, args: Vec<V>) -> R<V> {
        match &obj {
            V::Host(h) => {
                let h = h.clone();
                h.get(self, name, args)
            }
            V::Obj(o) => {
                let v = o
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone());
                match v {
                    Some(v) if !args.is_empty() => self.call_value(v, args, obj.clone()),
                    Some(v) => Ok(v),
                    None if name == "hasOwnProperty" => Ok(V::Native("hasOwnProperty")),
                    None => Ok(V::Empty),
                }
            }
            V::Str(_) | V::Arr(_) | V::Num(_) | V::Bool(_) => {
                if name == "length" {
                    return Ok(match &obj {
                        V::Str(s) => V::Num(s.encode_utf16().count() as f64),
                        V::Arr(a) => V::Num(a.borrow().len() as f64),
                        _ => V::Empty,
                    });
                }
                Err(Ex::no_member(name))
            }
            V::Empty | V::Null | V::Nothing => {
                Err(Ex::new(424, format!("objet requis pour « {name} »")))
            }
            _ => Err(Ex::no_member(name)),
        }
    }

    /// Appel `f(args)` : méthode d'un objet, fonction, ou indice de tableau
    /// en VBScript.
    fn call_expr(&mut self, f: &Expr, args: Vec<V>, env: &Env) -> R<V> {
        match f {
            Expr::Member(o, n) => {
                let obj = self.eval(o, env)?;
                self.call_method(obj, n, args)
            }
            Expr::WithRef(n) => {
                let obj = self
                    .with
                    .last()
                    .cloned()
                    .ok_or_else(|| Ex::new(424, "objet requis (With)"))?;
                self.call_method(obj, n, args)
            }
            Expr::Index(o, k) => {
                let obj = self.eval(o, env)?;
                let key = self.eval(k, env)?;
                let name = self.to_str(&key);
                self.call_method(obj, &name, args)
            }
            Expr::Ident(n) => match self.lookup(n, env) {
                Some(v) => self.call_value(v, args, V::Empty),
                None if self.lang == Lang::VBScript => {
                    Err(Ex::new(13, format!("type incompatible : « {n} »")))
                }
                None => Err(Ex::new(5009, format!("« {n} » n'est pas défini"))),
            },
            _ => {
                let v = self.eval(f, env)?;
                self.call_value(v, args, V::Empty)
            }
        }
    }

    pub fn call_method(&mut self, obj: V, name: &str, args: Vec<V>) -> R<V> {
        match &obj {
            V::Host(h) => {
                let h = h.clone();
                h.get(self, name, args)
            }
            V::Str(s) => host::string_method(self, s.clone(), name, args),
            V::Arr(a) => host::array_method(self, a.clone(), name, args),
            V::Num(n) => host::number_method(self, *n, name, args),
            V::Obj(o) => {
                let v = o
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone());
                match v {
                    Some(v) => self.call_value(v, args, obj.clone()),
                    None if name == "hasOwnProperty" => {
                        let k = args.first().map(|a| self.to_str(a)).unwrap_or_default();
                        Ok(V::Bool(o.borrow().iter().any(|(x, _)| *x == k)))
                    }
                    None if name == "toString" => Ok(V::str(self.to_str(&obj))),
                    None => Err(Ex::new(
                        438,
                        format!("l'objet n'a pas de méthode « {name} »"),
                    )),
                }
            }
            V::Func(c) if name == "call" || name == "apply" => {
                let c = c.clone();
                let mut args = args.into_iter();
                let this = args.next().unwrap_or(V::Empty);
                let rest: Vec<V> = if name == "call" {
                    args.collect()
                } else {
                    match args.next() {
                        Some(V::Arr(a)) => a.borrow().clone(),
                        _ => Vec::new(),
                    }
                };
                self.call_closure(&c, rest, this)
            }
            V::Native(n) if name == "call" || name == "apply" => {
                let mut args = args.into_iter();
                args.next();
                let rest: Vec<V> = if name == "call" {
                    args.collect()
                } else {
                    match args.next() {
                        Some(V::Arr(a)) => a.borrow().clone(),
                        _ => Vec::new(),
                    }
                };
                self.native(n, rest)
            }
            V::Bool(_) if name == "toString" => Ok(V::str(self.to_str(&obj))),
            V::Empty | V::Null | V::Nothing => {
                Err(Ex::new(424, format!("objet requis pour « {name} »")))
            }
            _ => Err(Ex::no_member(name)),
        }
    }

    pub fn call_value(&mut self, f: V, args: Vec<V>, this: V) -> R<V> {
        match f {
            V::Func(c) => self.call_closure(&c, args, this),
            V::Native(n) => self.native(n, args),
            V::Arr(a) if self.lang == Lang::VBScript => {
                let mut cur = V::Arr(a);
                for i in args {
                    let i = self.to_int(&i)?;
                    cur = match cur {
                        V::Arr(a) => a
                            .borrow()
                            .get(
                                usize::try_from(i)
                                    .map_err(|_| Ex::new(9, "indice hors limites"))?,
                            )
                            .cloned()
                            .ok_or_else(|| Ex::new(9, "indice hors limites"))?,
                        _ => return Err(Ex::new(9, "indice hors limites")),
                    };
                }
                Ok(cur)
            }
            V::Host(h) => h.get(self, "", args),
            V::Str(_) if self.lang == Lang::VBScript && args.is_empty() => Ok(f),
            other => {
                if self.lang == Lang::VBScript && args.is_empty() {
                    return Ok(other);
                }
                Err(Ex::new(
                    13,
                    format!("{} n'est pas une fonction", self.to_str(&other)),
                ))
            }
        }
    }

    pub fn call_closure(&mut self, c: &Closure, args: Vec<V>, this: V) -> R<V> {
        let scope = new_scope(Some(c.env.clone()));
        let key_name = self.key(&c.def.name);
        {
            let mut s = scope.borrow_mut();
            for (i, p) in c.def.params.iter().enumerate() {
                s.vars
                    .insert(p.clone(), args.get(i).cloned().unwrap_or(V::Empty));
            }
            match self.lang {
                Lang::JScript => {
                    s.vars.insert("arguments".into(), V::arr(args.clone()));
                }
                Lang::VBScript if !c.def.is_sub => {
                    s.vars.insert(key_name.clone(), V::Empty);
                }
                Lang::VBScript => {}
            }
        }
        self.hoist(&c.def.body, &scope);
        if self.resume.len() > 200 {
            return Err(Ex::new(28, "pile d'appels épuisée"));
        }
        self.resume.push(false);
        self.this.push(this);
        let r = self.block(&c.def.body, &scope);
        self.resume.pop();
        self.this.pop();
        let flow = r?;
        Ok(match self.lang {
            Lang::JScript => match flow {
                Flow::Return(v) => v,
                _ => V::Empty,
            },
            Lang::VBScript => {
                if c.def.is_sub {
                    V::Empty
                } else {
                    let v = scope.borrow().vars.get(&key_name).cloned();
                    v.unwrap_or(V::Empty)
                }
            }
        })
    }

    fn construct(&mut self, f: &Expr, args: Vec<V>, env: &Env) -> R<V> {
        // `New RegExp` (VBScript) ou `new ActiveXObject("…")` (JScript).
        let name = match f {
            Expr::Ident(n) => n.clone(),
            _ => String::new(),
        };
        if self.lang == Lang::VBScript {
            return match name.as_str() {
                "regexp" => host::regexp("", ""),
                _ => Err(Ex::new(429, format!("classe « {name} » inconnue"))),
            };
        }
        let fv = self.eval(f, env)?;
        match fv {
            V::Native(n) => host::construct_native(self, n, args),
            V::Func(c) => {
                let obj = V::Obj(Rc::new(RefCell::new(Vec::new())));
                let r = self.call_closure(&c, args, obj.clone())?;
                Ok(if matches!(r, V::Obj(_) | V::Host(_) | V::Arr(_)) {
                    r
                } else {
                    obj
                })
            }
            other => Err(Ex::new(
                5011,
                format!("{} n'est pas un constructeur", self.to_str(&other)),
            )),
        }
    }

    fn native(&mut self, name: &'static str, args: Vec<V>) -> R<V> {
        host::native(self, name, args)
    }

    pub fn assign(&mut self, target: &Expr, v: V, env: &Env) -> R<()> {
        match target {
            Expr::Ident(n) => {
                self.set_var(n, v, env);
                Ok(())
            }
            Expr::WithRef(n) => {
                let obj = self
                    .with
                    .last()
                    .cloned()
                    .ok_or_else(|| Ex::new(424, "objet requis (With)"))?;
                self.put_member(obj, n, Vec::new(), v)
            }
            Expr::Member(o, n) => {
                let obj = self.eval(o, env)?;
                self.put_member(obj, n, Vec::new(), v)
            }
            Expr::Index(o, k) => {
                let obj = self.eval(o, env)?;
                let key = self.eval(k, env)?;
                match &obj {
                    V::Arr(a) => {
                        let i = self.to_num(&key)?;
                        if i >= 0.0 && i == i.trunc() {
                            let i = i as usize;
                            let mut a = a.borrow_mut();
                            if i >= a.len() {
                                a.resize(i + 1, V::Empty);
                            }
                            a[i] = v;
                            Ok(())
                        } else {
                            let k = self.to_str(&key);
                            self.put_member(obj.clone(), &k, Vec::new(), v)
                        }
                    }
                    V::Host(h) => {
                        let h = h.clone();
                        h.put(self, "", vec![key], v)
                    }
                    _ => {
                        let k = self.to_str(&key);
                        self.put_member(obj, &k, Vec::new(), v)
                    }
                }
            }
            Expr::Call(f, args) => {
                let args = self.eval_args(args, env)?;
                match &**f {
                    Expr::Member(o, n) => {
                        let obj = self.eval(o, env)?;
                        self.put_member(obj, n, args, v)
                    }
                    Expr::WithRef(n) => {
                        let obj = self
                            .with
                            .last()
                            .cloned()
                            .ok_or_else(|| Ex::new(424, "objet requis (With)"))?;
                        self.put_member(obj, n, args, v)
                    }
                    Expr::Ident(n) => match self.lookup(n, env) {
                        Some(V::Arr(a)) => {
                            let idx = args.iter().map(|x| self.to_int(x)).collect::<R<Vec<_>>>()?;
                            set_element(&a, &idx, v)
                        }
                        Some(V::Host(h)) => h.put(self, "", args, v),
                        _ => Err(Ex::new(13, format!("« {n} » n'est pas un tableau"))),
                    },
                    _ => Err(Ex::new(5008, "affectation impossible")),
                }
            }
            _ => Err(Ex::new(5008, "affectation impossible")),
        }
    }

    fn put_member(&mut self, obj: V, name: &str, args: Vec<V>, v: V) -> R<()> {
        match &obj {
            V::Host(h) => {
                let h = h.clone();
                h.put(self, name, args, v)
            }
            V::Obj(o) => {
                let mut o = o.borrow_mut();
                match o.iter_mut().find(|(k, _)| k == name) {
                    Some(slot) => slot.1 = v,
                    None => o.push((name.to_string(), v)),
                }
                Ok(())
            }
            V::Arr(a) if name == "length" => {
                let n = self.to_num(&v)? as usize;
                a.borrow_mut().resize(n, V::Empty);
                Ok(())
            }
            V::Empty | V::Null | V::Nothing => {
                Err(Ex::new(424, format!("objet requis pour « {name} »")))
            }
            _ => Ok(()), // propriété posée sur une valeur simple : sans effet
        }
    }

    /// Exécute le programme, puis la fonction demandée (sans arguments).
    pub fn run(&mut self, body: &[Stmt], function: &str) -> R<V> {
        let global = self.global.clone();
        self.hoist(body, &global);
        match self.block(body, &global)? {
            Flow::Return(v) if function.is_empty() => return Ok(v),
            _ => {}
        }
        if function.is_empty() {
            return Ok(V::Empty);
        }
        let key = self.key(function);
        match self.lookup(&key, &global) {
            Some(V::Func(c)) => self.call_closure(&c, Vec::new(), V::Empty),
            _ => Err(Ex::new(
                5009,
                format!("fonction « {function} » absente du script"),
            )),
        }
    }

    #[cfg(test)]
    pub fn global_value(&self, name: &str) -> Option<V> {
        let k = self.key(name);
        self.lookup(&k, &self.global)
    }
}

pub fn round_even(n: f64) -> f64 {
    let r = n.round();
    if (n - n.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - n.signum()
    } else {
        r
    }
}

pub fn make_error(message: &str, number: f64) -> V {
    V::Obj(Rc::new(RefCell::new(vec![
        ("name".into(), V::str("Error")),
        ("message".into(), V::str(message)),
        ("description".into(), V::str(message)),
        ("number".into(), V::Num(number)),
    ])))
}

/// Tableau de VBScript : `Dim a(2)` a trois éléments (0 à 2) ; `Dim a()`
/// est vide ; plusieurs dimensions donnent des tableaux de tableaux.
fn make_array(sizes: &[i64]) -> V {
    match sizes.split_first() {
        None => V::arr(Vec::new()),
        Some((&n, rest)) => {
            let len = (n + 1).max(0) as usize;
            V::arr(
                (0..len)
                    .map(|_| {
                        if rest.is_empty() {
                            V::Empty
                        } else {
                            make_array(rest)
                        }
                    })
                    .collect(),
            )
        }
    }
}

fn set_element(a: &Rc<RefCell<Vec<V>>>, idx: &[i64], v: V) -> R<()> {
    let out_of_range = || Ex::new(9, "indice hors limites");
    let (&first, rest) = idx.split_first().ok_or_else(out_of_range)?;
    let i = usize::try_from(first).map_err(|_| out_of_range())?;
    if rest.is_empty() {
        let mut a = a.borrow_mut();
        let slot = a.get_mut(i).ok_or_else(out_of_range)?;
        *slot = v;
        return Ok(());
    }
    let inner = match a.borrow().get(i) {
        Some(V::Arr(x)) => x.clone(),
        _ => return Err(out_of_range()),
    };
    set_element(&inner, rest, v)
}

/// Lit un script (texte du paquet) : UTF-16 avec BOM, UTF-8 (avec ou
/// sans BOM), sinon page de code Windows-1252.
pub fn decode_source(data: &[u8]) -> String {
    if data.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = data[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = data[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    match std::str::from_utf8(data) {
        Ok(s) => s.to_string(),
        Err(_) => crate::dlls::decode_ansi(data),
    }
}

pub fn parse(lang: Lang, source: &str) -> Result<Vec<Stmt>, SyntaxError> {
    match lang {
        Lang::JScript => js::parse(source),
        Lang::VBScript => vbs::parse(source),
    }
}

/// Résultat d'un script : valeur rendue par la fonction, ou erreur
/// (syntaxe ou exécution) déjà mise en forme.
pub fn run_source(
    lang: Lang,
    source: &str,
    function: &str,
    globals: Vec<(&str, V)>,
) -> (Result<V, String>, Interp) {
    let mut it = Interp::new(lang);
    for (n, v) in globals {
        it.define_global(n, v);
    }
    let body = match parse(lang, source) {
        Ok(b) => b,
        Err(e) => {
            return (
                Err(format!("erreur de syntaxe ligne {} : {}", e.line, e.text)),
                it,
            )
        }
    };
    let r = it.run(&body, function).map_err(|e| match &e {
        Ex::Err(n, t) => format!("erreur {n} : {t}"),
        Ex::Throw(v) => format!("exception non attrapée : {}", it.to_str(v)),
    });
    (r, it)
}

/// Code Windows Installer rendu par la valeur de retour d'un script.
pub fn status_code(it: &Interp, v: &V) -> u32 {
    match v {
        V::Empty | V::Null | V::Nothing => 0,
        other => match it.to_num(other).map(|n| n as i64) {
            Ok(2) => 1602,
            Ok(3) => 1603,
            Ok(4) => 1604,
            _ => 0,
        },
    }
}

/// `winlux --msi-script-action LANGAGE FICHIER [FONCTION]` : exécute le
/// script dans une pile assez grande, avec l'objet Session ; rend le code
/// Windows Installer (1720 pour une erreur de script).
pub fn msi_action(lang: &str, path: &str, function: &str) -> u32 {
    let lang = if lang.eq_ignore_ascii_case("jscript") {
        Lang::JScript
    } else {
        Lang::VBScript
    };
    let Ok(data) = std::fs::read(path) else {
        eprintln!("msi.dll : script {path} illisible");
        return 1720;
    };
    let source = decode_source(&data);
    let function = function.to_string();
    let worker = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let (r, it) = run_source(lang, &source, &function, vec![("Session", host::session())]);
            match r {
                Ok(v) => status_code(&it, &v),
                Err(e) => {
                    eprintln!(
                        "msi.dll : script {} : {e}",
                        if lang == Lang::JScript {
                            "JScript"
                        } else {
                            "VBScript"
                        }
                    );
                    1720
                }
            }
        });
    match worker.map(|w| w.join()) {
        Ok(Ok(code)) => code,
        _ => 1720,
    }
}
