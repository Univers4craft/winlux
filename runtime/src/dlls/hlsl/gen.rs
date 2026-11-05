//! Génération GLSL 4.30 depuis l'arbre HLSL, aux conventions de `dxbc.rs` :
//! tampons de constantes en tableaux `vec4` std140, textures en `sampler*`
//! combinés (unité = base de l'étage + registre t), entrées du vertex shader
//! en `location = registre`, varyings `vary{loc}` partagés par sémantique,
//! `gl_Position` retourné (Direct3D → OpenGL).
//!
//! Matrices : une matrice HLSL R×C devient un `mat{R}x{C}` GLSL dont la
//! colonne i est la ligne i HLSL ; `m[i]` garde donc son sens, et
//! `mul(v, M)` s'écrit `M * v`, `mul(M, v)` s'écrit `v * M`,
//! `mul(A, B)` s'écrit `B * A`.

use super::ast::*;
use super::parse::const_eval;
use crate::dlls::dxbc::{varying_location, SigElement, Stage};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

type R<T> = Result<T, String>;

pub(crate) struct Output {
    pub stage: Stage,
    pub glsl: String,
    pub inputs: Vec<SigElement>,
    pub outputs: Vec<SigElement>,
    pub pairs: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GKind {
    Static,
    /// (tampon, décalage en composantes).
    Cb(usize, u32),
    Res,
}

struct Global {
    d: VarDecl,
    kind: GKind,
}

struct CbInfo {
    name: String,
    register: Option<u32>,
    /// Taille en composantes.
    size: u32,
}

#[derive(Clone, Debug, PartialEq)]
enum Idx {
    None,
    Const(u32),
    Dyn(String),
}

#[derive(Clone, Debug)]
struct ResRef {
    g: usize,
    idx: Idx,
    ty: Ty,
}

#[derive(Clone, Debug)]
struct CbRef {
    cb: usize,
    dynr: String,
    reg: u32,
    comp: u32,
    ty: Ty,
    rm: bool,
}

#[derive(Clone, Debug)]
enum V {
    E(String, Ty),
    Cb(CbRef),
    Res(ResRef),
}

#[derive(Clone)]
enum Local {
    Var(String, Ty),
    Res(ResRef),
}

struct El {
    path: String,
    ty: Ty,
    name: String,
    index: u32,
    flat: bool,
}

pub(crate) struct Gen<'p> {
    p: &'p Program,
    stage: Stage,
    globals: Vec<Global>,
    gmap: HashMap<String, usize>,
    cbs: Vec<CbInfo>,
    scopes: Vec<HashMap<String, Local>>,
    used_cb: BTreeSet<usize>,
    used_res: BTreeSet<usize>,
    pairs: Vec<(usize, Idx, usize, Idx)>,
    helpers: BTreeSet<&'static str>,
    funcs_done: HashMap<String, (String, Ty)>,
    protos: Vec<String>,
    defs: Vec<String>,
    body: String,
    depth: usize,
    cur_ret: Ty,
    nfunc: u32,
}

/// Nom GLSL d'un identifiant HLSL : préfixe, et pas de « __ » (réservé).
fn mg(prefix: &str, name: &str) -> String {
    let mut s = String::from(prefix);
    for c in name.chars() {
        if c == '_' && s.ends_with('_') {
            s.push('0');
        }
        s.push(c);
    }
    s
}

fn scalar_name(b: Base) -> &'static str {
    match b {
        Base::Bool => "bool",
        Base::Int => "int",
        Base::Uint => "uint",
        Base::Float => "float",
    }
}

fn vec_prefix(b: Base) -> &'static str {
    match b {
        Base::Bool => "b",
        Base::Int => "i",
        Base::Uint => "u",
        Base::Float => "",
    }
}

fn vec_name(b: Base, n: u8) -> String {
    if n == 1 {
        scalar_name(b).into()
    } else {
        format!("{}vec{n}", vec_prefix(b))
    }
}

const SW: [&str; 4] = ["x", "y", "z", "w"];

fn swz(from: u32, n: u32) -> String {
    (from..from + n).map(|i| SW[i as usize]).collect()
}

/// « TEXCOORD12 » → (« TEXCOORD », 12).
fn split_semantic(s: &str) -> (String, u32) {
    let u = s.to_ascii_uppercase();
    let end = u.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let idx = u[end..].parse().unwrap_or(0);
    (u[..end].to_string(), idx)
}

fn ct_of(b: Base) -> u32 {
    match b {
        Base::Float => 3,
        Base::Int => 2,
        _ => 1,
    }
}

fn float_lit(v: f64) -> String {
    let f = v as f32;
    if f.is_nan() {
        return "uintBitsToFloat(0x7FC00000u)".into();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "uintBitsToFloat(0x7F800000u)".into()
        } else {
            "uintBitsToFloat(0xFF800000u)".into()
        };
    }
    let s = format!("{f:?}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.0")
    }
}

fn is_int_literal(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
}

impl<'p> Gen<'p> {
    pub(crate) fn new(p: &'p Program, stage: Stage) -> R<Gen<'p>> {
        let mut g = Gen {
            p,
            stage,
            globals: Vec::new(),
            gmap: HashMap::new(),
            cbs: Vec::new(),
            scopes: Vec::new(),
            used_cb: BTreeSet::new(),
            used_res: BTreeSet::new(),
            pairs: Vec::new(),
            helpers: BTreeSet::new(),
            funcs_done: HashMap::new(),
            protos: Vec::new(),
            defs: Vec::new(),
            body: String::new(),
            depth: 1,
            cur_ret: Ty::Void,
            nfunc: 0,
        };
        // Tampons déclarés.
        for (ci, cb) in p.cbuffers.iter().enumerate() {
            let mut off = 0;
            let mut end = 0;
            for m in &cb.members {
                let start = g.place(&mut off, &m.ty, m.row_major, m.packoffset)?;
                end = end.max(off);
                g.add_global(m.clone(), GKind::Cb(ci, start));
            }
            g.cbs.push(CbInfo {
                name: cb.name.clone(),
                register: cb.register,
                size: end,
            });
        }
        // $Globals : variables globales ni statiques ni ressources.
        let gi = g.cbs.len();
        let mut off = 0;
        let mut end = 0;
        let mut any = false;
        for d in &p.globals {
            if d.is_static || d.ty.is_resource() {
                let kind = if d.ty.is_resource() {
                    GKind::Res
                } else {
                    GKind::Static
                };
                g.add_global(d.clone(), kind);
                continue;
            }
            let pack = match d.register {
                Some(('c', n)) => Some(n * 4),
                _ => d.packoffset,
            };
            let start = g.place(&mut off, &d.ty, d.row_major, pack)?;
            end = end.max(off);
            any = true;
            g.add_global(d.clone(), GKind::Cb(gi, start));
        }
        if any {
            g.cbs.push(CbInfo {
                name: "$Globals".into(),
                register: None,
                size: end,
            });
        }
        Ok(g)
    }

    fn add_global(&mut self, d: VarDecl, kind: GKind) {
        self.gmap.insert(d.name.clone(), self.globals.len());
        self.globals.push(Global { d, kind });
    }

    // ---------- Disposition des tampons de constantes ----------

    /// Taille en composantes (sans arrondi final).
    fn size_of(&self, t: &Ty, rm: bool) -> R<u32> {
        Ok(match t {
            Ty::Scalar(_) => 1,
            Ty::Vector(_, n) => *n as u32,
            Ty::Matrix(_, r, c) => {
                let (r, c) = (*r as u32, *c as u32);
                if rm {
                    (r - 1) * 4 + c
                } else {
                    (c - 1) * 4 + r
                }
            }
            Ty::Array(e, n) => {
                let s = self.size_of(e, rm)?;
                s.div_ceil(4) * 4 * n.saturating_sub(1) + s
            }
            Ty::Struct(i) => self.struct_layout(*i)?.1,
            _ => return Err("ressource dans un tampon de constantes".into()),
        })
    }

    /// Place un membre ; renvoie son début (composantes) et avance `off`.
    fn place(&self, off: &mut u32, t: &Ty, rm: bool, pack: Option<u32>) -> R<u32> {
        let size = self.size_of(t, rm)?;
        let start = if let Some(p) = pack {
            p
        } else {
            let aligned = matches!(t, Ty::Matrix(..) | Ty::Array(..) | Ty::Struct(_));
            if aligned || (*off % 4) + size > 4 {
                off.div_ceil(4) * 4
            } else {
                *off
            }
        };
        *off = start + size;
        Ok(start)
    }

    /// Décalages des membres d'une structure et taille arrondie au registre.
    fn struct_layout(&self, i: usize) -> R<(Vec<u32>, u32)> {
        let s = self.p.structs.get(i).ok_or("structure inconnue")?;
        let mut off = 0;
        let mut v = Vec::new();
        for m in &s.members {
            v.push(self.place(&mut off, &m.ty, m.row_major, None)?);
        }
        Ok((v, off.div_ceil(4) * 4))
    }

    // ---------- Types ----------

    fn tyname(&self, t: &Ty) -> R<String> {
        Ok(match t {
            Ty::Void => "void".into(),
            Ty::Scalar(b) => scalar_name(*b).into(),
            Ty::Vector(b, n) => vec_name(*b, *n),
            Ty::Matrix(b, r, c) => {
                if *b != Base::Float {
                    return Err("matrice non flottante non prise en charge".into());
                }
                if r == c {
                    format!("mat{r}")
                } else {
                    format!("mat{r}x{c}")
                }
            }
            Ty::Array(..) => {
                let mut dims = String::new();
                let mut t = t;
                while let Ty::Array(e, n) = t {
                    let _ = write!(dims, "[{n}]");
                    t = e;
                }
                format!("{}{dims}", self.tyname(t)?)
            }
            Ty::Struct(i) => self.struct_name(*i),
            Ty::Texture(..) | Ty::Sampler(_) => {
                return Err("ressource utilisée comme valeur".into());
            }
        })
    }

    fn struct_name(&self, i: usize) -> String {
        let n = self.p.structs.get(i).map_or("", |s| s.name.as_str());
        mg(&format!("S{i}_"), n)
    }

    fn zero(&self, t: &Ty) -> R<String> {
        Ok(match t {
            Ty::Scalar(b) => match b {
                Base::Bool => "false".into(),
                Base::Int => "0".into(),
                Base::Uint => "0u".into(),
                Base::Float => "0.0".into(),
            },
            Ty::Vector(b, n) => format!("{}({})", vec_name(*b, *n), self.zero(&Ty::Scalar(*b))?),
            Ty::Matrix(..) => format!("{}(0.0)", self.tyname(t)?),
            Ty::Array(e, n) => {
                let z = self.zero(e)?;
                let v: Vec<String> = (0..*n).map(|_| z.clone()).collect();
                format!("{}({})", self.tyname(t)?, v.join(", "))
            }
            Ty::Struct(i) => {
                let s = &self.p.structs[*i];
                let v: R<Vec<String>> = s.members.iter().map(|m| self.zero(&m.ty)).collect();
                format!("{}({})", self.struct_name(*i), v?.join(", "))
            }
            _ => return Err("valeur nulle d'une ressource".into()),
        })
    }

    /// Conversion implicite ou explicite (troncature, diffusion).
    fn conv(&self, s: &str, from: &Ty, to: &Ty) -> R<String> {
        if from == to {
            return Ok(s.to_string());
        }
        Ok(match (from, to) {
            (Ty::Scalar(_), Ty::Scalar(Base::Float)) if is_int_literal(s) => format!("{s}.0"),
            (Ty::Scalar(Base::Int), Ty::Scalar(Base::Uint)) if is_int_literal(s) => format!("{s}u"),
            (Ty::Scalar(_), Ty::Scalar(b)) => format!("{}({s})", scalar_name(*b)),
            (Ty::Scalar(a), Ty::Vector(b, n)) => {
                let s = self.conv(s, &Ty::Scalar(*a), &Ty::Scalar(*b))?;
                format!("{}({s})", vec_name(*b, *n))
            }
            (Ty::Vector(a, _), Ty::Scalar(_)) => {
                self.conv(&format!("{s}.x"), &Ty::Scalar(*a), to)?
            }
            (Ty::Vector(a, n), Ty::Vector(b, m)) if m <= n => {
                let t = if m < n {
                    format!("{s}.{}", swz(0, *m as u32))
                } else {
                    s.to_string()
                };
                if a == b {
                    t
                } else {
                    format!("{}({t})", vec_name(*b, *m))
                }
            }
            (Ty::Scalar(a), Ty::Matrix(Base::Float, r, c)) => {
                let s = self.conv(s, &Ty::Scalar(*a), &Ty::Scalar(Base::Float))?;
                let col = format!("vec{c}({s})");
                let v: Vec<String> = (0..*r).map(|_| col.clone()).collect();
                format!("{}({})", self.tyname(to)?, v.join(", "))
            }
            (Ty::Matrix(Base::Float, r, c), Ty::Matrix(Base::Float, r2, c2))
                if r2 <= r && c2 <= c =>
            {
                format!("{}({s})", self.tyname(to)?)
            }
            (Ty::Matrix(Base::Float, r, c), Ty::Vector(b, n)) if (*r * *c) >= *n && *r >= 1 => {
                // Troncature d'une matrice en vecteur : composantes de la ligne 0…
                let comps: Vec<String> = (0..*n)
                    .map(|k| format!("{s}[{}][{}]", k / *c, k % *c))
                    .collect();
                let v = format!("vec{n}({})", comps.join(", "));
                self.conv(&v, &Ty::Vector(Base::Float, *n), &Ty::Vector(*b, *n))?
            }
            (Ty::Vector(a, n), Ty::Matrix(Base::Float, r, c)) if *n == r * c => {
                let s = self.conv(s, &Ty::Vector(*a, *n), &Ty::Vector(Base::Float, *n))?;
                format!("{}({s})", self.tyname(to)?)
            }
            _ => {
                return Err(format!(
                    "conversion impossible de {} vers {}",
                    self.show(from),
                    self.show(to)
                ))
            }
        })
    }

    fn show(&self, t: &Ty) -> String {
        match t {
            Ty::Void => "void".into(),
            Ty::Scalar(b) => scalar_name(*b).into(),
            Ty::Vector(b, n) => format!("{}{n}", scalar_name(*b)),
            Ty::Matrix(b, r, c) => format!("{}{r}x{c}", scalar_name(*b)),
            Ty::Array(e, n) => format!("{}[{n}]", self.show(e)),
            Ty::Struct(i) => self
                .p
                .structs
                .get(*i)
                .map_or("struct".into(), |s| s.name.clone()),
            Ty::Texture(k, _) => format!("Texture{}", k.glsl()),
            Ty::Sampler(_) => "SamplerState".into(),
        }
    }

    // ---------- Portées ----------

    fn lookup(&mut self, name: &str) -> R<V> {
        for s in self.scopes.iter().rev() {
            if let Some(l) = s.get(name) {
                return Ok(match l {
                    Local::Var(g, t) => V::E(g.clone(), t.clone()),
                    Local::Res(r) => V::Res(r.clone()),
                });
            }
        }
        let gi = *self
            .gmap
            .get(name)
            .ok_or_else(|| format!("identifiant inconnu « {name} »"))?;
        let g = &self.globals[gi];
        Ok(match g.kind {
            GKind::Static => V::E(mg("h", name), g.d.ty.clone()),
            GKind::Cb(cb, off) => {
                self.used_cb.insert(cb);
                V::Cb(CbRef {
                    cb,
                    dynr: String::new(),
                    reg: off / 4,
                    comp: off % 4,
                    ty: g.d.ty.clone(),
                    rm: g.d.row_major,
                })
            }
            GKind::Res => {
                self.used_res.insert(gi);
                V::Res(ResRef {
                    g: gi,
                    idx: Idx::None,
                    ty: g.d.ty.clone(),
                })
            }
        })
    }

    fn declare(&mut self, name: &str, l: Local) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), l);
        }
    }

    // ---------- Lecture des tampons de constantes ----------

    fn cb_reg(&self, c: &CbRef, plus: u32) -> String {
        format!("cb{}[{}{}]", c.cb, c.reg + plus, c.dynr)
    }

    fn cb_load(&self, c: &CbRef) -> R<String> {
        let bits = |b: Base, s: String| -> String {
            match b {
                Base::Float => s,
                Base::Int => format!("floatBitsToInt({s})"),
                Base::Uint => format!("floatBitsToUint({s})"),
                Base::Bool => s,
            }
        };
        Ok(match &c.ty {
            Ty::Scalar(b) => {
                let s = format!("{}.{}", self.cb_reg(c, 0), SW[c.comp as usize]);
                match b {
                    Base::Bool => format!("(floatBitsToUint({s}) != 0u)"),
                    _ => bits(*b, s),
                }
            }
            Ty::Vector(b, n) => {
                let s = format!("{}.{}", self.cb_reg(c, 0), swz(c.comp, *n as u32));
                match b {
                    Base::Bool => format!("notEqual(floatBitsToUint({s}), uvec{n}(0u))"),
                    _ => bits(*b, s),
                }
            }
            Ty::Matrix(Base::Float, r, cc) => {
                let (r, cc) = (*r as u32, *cc as u32);
                if c.rm {
                    let rows: Vec<String> = (0..r)
                        .map(|i| format!("{}.{}", self.cb_reg(c, i), swz(0, cc)))
                        .collect();
                    format!("{}({})", self.tyname(&c.ty)?, rows.join(", "))
                } else {
                    let cols: Vec<String> = (0..cc)
                        .map(|j| format!("{}.{}", self.cb_reg(c, j), swz(0, r)))
                        .collect();
                    let t = if r == cc {
                        format!("mat{r}")
                    } else {
                        format!("mat{cc}x{r}")
                    };
                    format!("transpose({t}({}))", cols.join(", "))
                }
            }
            Ty::Array(e, n) => {
                let stride = self.size_of(e, c.rm)?.div_ceil(4);
                let mut v = Vec::new();
                for k in 0..*n {
                    v.push(self.cb_load(&CbRef {
                        reg: c.reg + k * stride,
                        comp: 0,
                        ty: (**e).clone(),
                        ..c.clone()
                    })?);
                }
                format!("{}({})", self.tyname(&c.ty)?, v.join(", "))
            }
            Ty::Struct(i) => {
                let (offs, _) = self.struct_layout(*i)?;
                let s = &self.p.structs[*i];
                let mut v = Vec::new();
                for (m, o) in s.members.iter().zip(offs) {
                    let o = c.reg * 4 + o;
                    v.push(self.cb_load(&CbRef {
                        reg: o / 4,
                        comp: o % 4,
                        ty: m.ty.clone(),
                        rm: m.row_major,
                        ..c.clone()
                    })?);
                }
                format!("{}({})", self.struct_name(*i), v.join(", "))
            }
            t => return Err(format!("lecture de {} dans un tampon", self.show(t))),
        })
    }

    fn rv(&mut self, e: &Expr) -> R<(String, Ty)> {
        let v = self.expr(e)?;
        self.load(v).map_err(|m| self.at(e, &m))
    }

    fn load(&self, v: V) -> R<(String, Ty)> {
        match v {
            V::E(s, t) => Ok((s, t)),
            V::Cb(c) => Ok((self.cb_load(&c)?, c.ty)),
            V::Res(_) => Err("ressource utilisée comme valeur".into()),
        }
    }

    fn at(&self, e: &Expr, m: &str) -> String {
        if m.starts_with('(') {
            m.to_string()
        } else {
            format!("({}) : {m}", e.line)
        }
    }

    // ---------- Expressions ----------

    fn expr(&mut self, e: &Expr) -> R<V> {
        self.expr_inner(e).map_err(|m| self.at(e, &m))
    }

    fn expr_inner(&mut self, e: &Expr) -> R<V> {
        Ok(match &e.k {
            ExprKind::Int(v, u) => {
                if *u || *v > i32::MAX as u64 {
                    V::E(format!("{}u", *v as u32), Ty::Scalar(Base::Uint))
                } else {
                    V::E(format!("{v}"), Ty::Scalar(Base::Int))
                }
            }
            ExprKind::Float(f) => V::E(float_lit(*f), Ty::Scalar(Base::Float)),
            ExprKind::Bool(b) => V::E(format!("{b}"), Ty::Scalar(Base::Bool)),
            ExprKind::Ident(n) => self.lookup(n)?,
            ExprKind::Unary(op, a) => {
                let (s, t) = self.rv(a)?;
                let (s, t) = self.unary(op, s, &t)?;
                V::E(s, t)
            }
            ExprKind::Postfix(op, a) => {
                let (s, t) = self.rv(a)?;
                V::E(format!("({s}{op})"), t)
            }
            ExprKind::Binary(op, a, b) => {
                let (x, tx) = self.rv(a)?;
                let (y, ty) = self.rv(b)?;
                let (s, t) = self.binary(op, x, &tx, y, &ty)?;
                V::E(s, t)
            }
            ExprKind::Assign(op, l, r) => {
                let (ls, lt) = match self.expr(l)? {
                    V::E(s, t) => (s, t),
                    _ => return Err("affectation d'une constante de tampon".into()),
                };
                let rs = if let ExprKind::InitList(items) = &r.k {
                    self.init_list(items, &lt)?
                } else {
                    let (rs, rt) = self.rv(r)?;
                    if *op == "=" {
                        self.conv(&rs, &rt, &lt)?
                    } else {
                        let target = if rt.comps() == Some(1) && lt.base().is_some() {
                            Ty::Scalar(lt.base().unwrap_or(Base::Float))
                        } else {
                            lt.clone()
                        };
                        self.conv(&rs, &rt, &target)?
                    }
                };
                let o = &op[..op.len() - 1];
                let s = match (*op, &lt) {
                    ("=", _) => format!("({ls} = {rs})"),
                    ("*=", Ty::Matrix(..)) => format!("({ls} = matrixCompMult({ls}, {rs}))"),
                    ("%=", t) if t.base() == Some(Base::Float) => {
                        self.helpers.insert("mod");
                        let rs = self.conv(&rs, &Ty::Scalar(Base::Float), t).unwrap_or(rs);
                        format!("({ls} = w_mod({ls}, {rs}))")
                    }
                    _ => format!("({ls} {o}= {rs})"),
                };
                V::E(s, lt)
            }
            ExprKind::Ternary(c, a, b) => {
                let (cs, ct) = self.rv(c)?;
                let (x, tx) = self.rv(a)?;
                let (y, ty) = self.rv(b)?;
                let mut t = if tx == ty {
                    tx.clone()
                } else {
                    self.common(&tx, &ty, false)?
                };
                let vec_cond = ct.comps().unwrap_or(1) > 1 && t.comps().is_some();
                if vec_cond {
                    let n = ct.comps().unwrap_or(1).max(t.comps().unwrap_or(1));
                    t = Ty::vec(t.base().unwrap_or(Base::Float), n);
                }
                let x = self.conv(&x, &tx, &t)?;
                let y = self.conv(&y, &ty, &t)?;
                match t.comps() {
                    Some(n) if vec_cond => {
                        let cb = self.conv(&cs, &ct, &Ty::Vector(Base::Bool, n))?;
                        if t.base() == Some(Base::Float) {
                            V::E(format!("mix({y}, {x}, {cb})"), t)
                        } else {
                            let v: Vec<String> = (0..n as u32)
                                .map(|i| {
                                    let k = SW[i as usize];
                                    format!("({cb}.{k} ? {x}.{k} : {y}.{k})")
                                })
                                .collect();
                            V::E(format!("{}({})", self.tyname(&t)?, v.join(", ")), t)
                        }
                    }
                    _ => {
                        let cb = self.to_bool_scalar(&cs, &ct)?;
                        V::E(format!("({cb} ? {x} : {y})"), t)
                    }
                }
            }
            ExprKind::Call(name, args) => {
                if self.p.funcs.iter().any(|f| &f.name == name) {
                    let (s, t) = self.user_call(name, args)?;
                    V::E(s, t)
                } else {
                    let (s, t) = self.intrinsic(name, args)?;
                    V::E(s, t)
                }
            }
            ExprKind::Ctor(t, args) => {
                let (s, t) = self.ctor(t, args)?;
                V::E(s, t)
            }
            ExprKind::Cast(t, a) => {
                let (s, ft) = self.rv(a)?;
                if matches!(t, Ty::Struct(_) | Ty::Array(..)) && ft.comps() == Some(1) {
                    V::E(self.zero(t)?, t.clone())
                } else {
                    V::E(self.conv(&s, &ft, t)?, t.clone())
                }
            }
            ExprKind::Member(a, m) => {
                let v = self.expr(a)?;
                self.member(v, m)?
            }
            ExprKind::Index(a, i) => {
                let v = self.expr(a)?;
                self.index(v, i)?
            }
            ExprKind::Method(o, m, args) => {
                let v = self.expr(o)?;
                let (s, t) = self.method(v, m, args)?;
                V::E(s, t)
            }
            ExprKind::InitList(_) => return Err("liste d'initialisation inattendue".into()),
            ExprKind::Comma(a, b) => {
                let (x, _) = self.rv(a)?;
                let (y, t) = self.rv(b)?;
                V::E(format!("({x}, {y})"), t)
            }
        })
    }

    fn to_bool_scalar(&self, s: &str, t: &Ty) -> R<String> {
        match t {
            Ty::Scalar(Base::Bool) => Ok(s.to_string()),
            Ty::Scalar(_) | Ty::Vector(..) => {
                let st = Ty::Scalar(t.base().unwrap_or(Base::Float));
                let s = self.conv(s, t, &st)?;
                self.conv(&s, &st, &Ty::Scalar(Base::Bool))
            }
            _ => Err(format!("condition de type {}", self.show(t))),
        }
    }

    fn unary(&mut self, op: &str, s: String, t: &Ty) -> R<(String, Ty)> {
        Ok(match op {
            "-" => {
                let t2 = if t.base() == Some(Base::Bool) {
                    t.with_base(Base::Int)
                } else {
                    t.clone()
                };
                let s = self.conv(&s, t, &t2)?;
                (format!("(-{s})"), t2)
            }
            "+" => (s, t.clone()),
            "!" => {
                let bt = t.with_base(Base::Bool);
                let s = self.conv(&s, t, &bt)?;
                if t.comps().unwrap_or(1) > 1 {
                    (format!("not({s})"), bt)
                } else {
                    (format!("(!{s})"), bt)
                }
            }
            "~" => (format!("(~{s})"), t.clone()),
            "++" | "--" => (format!("({op}{s})"), t.clone()),
            _ => return Err(format!("opérateur {op}")),
        })
    }

    /// Type commun de deux opérandes numériques.
    fn common(&self, a: &Ty, b: &Ty, arith: bool) -> R<Ty> {
        let (Some(ba), Some(bb)) = (a.base(), b.base()) else {
            return Err(format!(
                "opérandes incompatibles {} et {}",
                self.show(a),
                self.show(b)
            ));
        };
        let mut base = ba.max(bb);
        if arith && base == Base::Bool {
            base = Base::Int;
        }
        Ok(match (a, b) {
            (Ty::Scalar(_), x) | (x, Ty::Scalar(_)) => x.with_base(base),
            (Ty::Vector(_, n), Ty::Vector(_, m)) => Ty::Vector(base, (*n).min(*m)),
            (Ty::Matrix(_, r, c), Ty::Matrix(_, r2, c2)) => {
                Ty::Matrix(base, (*r).min(*r2), (*c).min(*c2))
            }
            (Ty::Vector(_, n), Ty::Matrix(_, r, c)) | (Ty::Matrix(_, r, c), Ty::Vector(_, n))
                if *r == 1 || *c == 1 =>
            {
                Ty::vec(base, (*n).min((*r).max(*c)))
            }
            _ => {
                return Err(format!(
                    "opérandes incompatibles {} et {}",
                    self.show(a),
                    self.show(b)
                ))
            }
        })
    }

    /// Convertit un opérande vers le type commun (un scalaire reste scalaire).
    fn operand(&self, s: &str, t: &Ty, common: &Ty) -> R<String> {
        if t.comps() == Some(1) {
            self.conv(s, t, &Ty::Scalar(common.base().unwrap_or(Base::Float)))
        } else {
            self.conv(s, t, common)
        }
    }

    fn binary(&mut self, op: &str, x: String, tx: &Ty, y: String, ty: &Ty) -> R<(String, Ty)> {
        match op {
            "&&" | "||" => {
                let c = self.common(tx, ty, false)?.with_base(Base::Bool);
                let a = self.conv(&x, tx, &c)?;
                let b = self.conv(&y, ty, &c)?;
                Ok(match c.comps() {
                    Some(1) => (format!("({a} {op} {b})"), c),
                    Some(_) => {
                        let h = if op == "&&" { "and" } else { "or" };
                        self.helpers.insert(h);
                        (format!("w_{h}({a}, {b})"), c)
                    }
                    None => return Err("opérateur logique sur une matrice".into()),
                })
            }
            "<" | ">" | "<=" | ">=" | "==" | "!=" => {
                let c = self.common(tx, ty, false)?;
                let a = self.conv(&x, tx, &c)?;
                let b = self.conv(&y, ty, &c)?;
                let rt = c.with_base(Base::Bool);
                Ok(match c.comps() {
                    Some(1) => (format!("({a} {op} {b})"), rt),
                    Some(_) => {
                        let f = match op {
                            "<" => "lessThan",
                            ">" => "greaterThan",
                            "<=" => "lessThanEqual",
                            ">=" => "greaterThanEqual",
                            "==" => "equal",
                            _ => "notEqual",
                        };
                        (format!("{f}({a}, {b})"), rt)
                    }
                    None => return Err("comparaison de matrices".into()),
                })
            }
            "<<" | ">>" => {
                let c = self.common(tx, ty, true)?;
                let lt = if tx.comps() == Some(1) && c.comps() != Some(1) {
                    c.clone()
                } else {
                    tx.with_base(tx.base().map_or(Base::Int, |b| b.max(Base::Int)))
                };
                let lt = if lt.base() == Some(Base::Float) {
                    lt.with_base(Base::Int)
                } else {
                    lt
                };
                let a = self.conv(&x, tx, &lt)?;
                let b = self.operand(&y, ty, &lt.with_base(Base::Int))?;
                Ok((format!("({a} {op} {b})"), lt))
            }
            _ => {
                let mut c = self.common(tx, ty, true)?;
                if matches!(op, "&" | "|" | "^") && c.base() == Some(Base::Float) {
                    c = c.with_base(Base::Int);
                }
                let a = self.operand(&x, tx, &c)?;
                let b = self.operand(&y, ty, &c)?;
                if op == "*" && matches!(tx, Ty::Matrix(..)) && matches!(ty, Ty::Matrix(..)) {
                    return Ok((format!("matrixCompMult({a}, {b})"), c));
                }
                if op == "%" && c.base() == Some(Base::Float) {
                    self.helpers.insert("mod");
                    let a = self.conv(&a, &self.after_operand(tx, &c), &c)?;
                    let b = self.conv(&b, &self.after_operand(ty, &c), &c)?;
                    return Ok((format!("w_mod({a}, {b})"), c));
                }
                Ok((format!("({a} {op} {b})"), c))
            }
        }
    }

    fn after_operand(&self, t: &Ty, c: &Ty) -> Ty {
        if t.comps() == Some(1) {
            Ty::Scalar(c.base().unwrap_or(Base::Float))
        } else {
            c.clone()
        }
    }

    fn member(&mut self, v: V, m: &str) -> R<V> {
        match v {
            V::Cb(c) => {
                if let Ty::Struct(i) = &c.ty {
                    let (offs, _) = self.struct_layout(*i)?;
                    let s = &self.p.structs[*i];
                    let k = s
                        .members
                        .iter()
                        .position(|x| x.name == m)
                        .ok_or_else(|| format!("membre « {m} » absent de {}", s.name))?;
                    let o = c.reg * 4 + c.comp + offs[k];
                    return Ok(V::Cb(CbRef {
                        reg: o / 4,
                        comp: o % 4,
                        ty: s.members[k].ty.clone(),
                        rm: s.members[k].row_major,
                        ..c
                    }));
                }
                let (s, t) = self.load(V::Cb(c))?;
                self.member(V::E(s, t), m)
            }
            V::E(s, t) => match &t {
                Ty::Struct(i) => {
                    let st = &self.p.structs[*i];
                    let mm = st
                        .members
                        .iter()
                        .find(|x| x.name == m)
                        .ok_or_else(|| format!("membre « {m} » absent de {}", st.name))?;
                    Ok(V::E(format!("{s}.{}", mg("m_", m)), mm.ty.clone()))
                }
                Ty::Scalar(b) | Ty::Vector(b, _) => {
                    let n = t.comps().unwrap_or(1);
                    let mut idx = Vec::new();
                    for ch in m.chars() {
                        let k = match ch {
                            'x' | 'r' => 0,
                            'y' | 'g' => 1,
                            'z' | 'b' => 2,
                            'w' | 'a' => 3,
                            _ => return Err(format!("composante « {m} » invalide")),
                        };
                        if k >= n {
                            return Err(format!("composante « {m} » hors du vecteur"));
                        }
                        idx.push(k);
                    }
                    if idx.is_empty() || idx.len() > 4 {
                        return Err(format!("composante « {m} » invalide"));
                    }
                    let rt = Ty::vec(*b, idx.len() as u8);
                    if n == 1 {
                        return Ok(V::E(
                            if idx.len() == 1 {
                                s
                            } else {
                                format!("{}({s})", vec_name(*b, idx.len() as u8))
                            },
                            rt,
                        ));
                    }
                    let sw: String = idx.iter().map(|&k| SW[k as usize]).collect();
                    Ok(V::E(format!("{s}.{sw}"), rt))
                }
                Ty::Matrix(b, _, _) => {
                    // ._m01_m12 (base 0) ou ._12_23 (base 1).
                    let mut els = Vec::new();
                    for part in m.split('_').filter(|x| !x.is_empty()) {
                        let (z, digits) = match part.strip_prefix('m') {
                            Some(d) => (0, d),
                            None => (1, part),
                        };
                        let d: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
                        if d.len() != 2 || d[0] < z || d[1] < z {
                            return Err(format!("composante de matrice « {m} » invalide"));
                        }
                        els.push(format!("{s}[{}][{}]", d[0] - z, d[1] - z));
                    }
                    if els.is_empty() || els.len() > 4 {
                        return Err(format!("composante de matrice « {m} » invalide"));
                    }
                    if els.len() == 1 {
                        Ok(V::E(els.remove(0), Ty::Scalar(*b)))
                    } else {
                        let n = els.len() as u8;
                        Ok(V::E(
                            format!("{}({})", vec_name(*b, n), els.join(", ")),
                            Ty::Vector(*b, n),
                        ))
                    }
                }
                _ => Err(format!("« .{m} » sur {}", self.show(&t))),
            },
            V::Res(_) => Err(format!("membre « {m} » d'une ressource")),
        }
    }

    fn index_str(&mut self, i: &Expr) -> R<String> {
        let (s, t) = self.rv(i)?;
        match t {
            Ty::Scalar(Base::Int) | Ty::Scalar(Base::Uint) => Ok(s),
            _ => self.conv(&s, &t, &Ty::Scalar(Base::Int)),
        }
    }

    fn const_index(&self, i: &Expr) -> Option<u32> {
        const_eval(i, &self.p.globals)
            .filter(|v| *v >= 0.0 && v.fract() == 0.0)
            .map(|v| v as u32)
    }

    fn index(&mut self, v: V, i: &Expr) -> R<V> {
        match v {
            V::Cb(c) => match &c.ty {
                Ty::Array(e, _) => {
                    let stride = self.size_of(e, c.rm)?.div_ceil(4);
                    let e = (**e).clone();
                    if let Some(k) = self.const_index(i) {
                        Ok(V::Cb(CbRef {
                            reg: c.reg + k * stride,
                            comp: 0,
                            ty: e,
                            ..c
                        }))
                    } else {
                        let s = self.index_str(i)?;
                        let dynr = if stride == 1 {
                            format!("{} + int({s})", c.dynr)
                        } else {
                            format!("{} + int({s}) * {stride}", c.dynr)
                        };
                        Ok(V::Cb(CbRef {
                            dynr,
                            comp: 0,
                            ty: e,
                            ..c
                        }))
                    }
                }
                _ => {
                    let (s, t) = self.load(V::Cb(c))?;
                    self.index(V::E(s, t), i)
                }
            },
            V::E(s, t) => {
                let k = self.index_str(i)?;
                let rt = match &t {
                    Ty::Array(e, _) => (**e).clone(),
                    Ty::Vector(b, _) => Ty::Scalar(*b),
                    Ty::Matrix(b, _, c) => Ty::vec(*b, *c),
                    _ => return Err(format!("indexation de {}", self.show(&t))),
                };
                Ok(V::E(format!("{s}[{k}]"), rt))
            }
            V::Res(r) => match &r.ty {
                Ty::Array(e, _) => {
                    let idx = if let Some(k) = self.const_index(i) {
                        Idx::Const(k)
                    } else {
                        Idx::Dyn(self.index_str(i)?)
                    };
                    Ok(V::Res(ResRef {
                        g: r.g,
                        idx,
                        ty: (**e).clone(),
                    }))
                }
                Ty::Texture(..) => {
                    let (s, t) = self.load_texel(&r, i, None)?;
                    Ok(V::E(s, t))
                }
                _ => Err("indexation d'un échantillonneur".into()),
            },
        }
    }

    // ---------- Constructeurs et listes ----------

    fn ctor(&mut self, t: &Ty, args: &[Expr]) -> R<(String, Ty)> {
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.rv(a)?);
        }
        if vals.len() == 1 {
            let (s, ft) = &vals[0];
            if ft.comps() == Some(1)
                || ft == t
                || matches!((ft, t), (Ty::Matrix(..), Ty::Matrix(..)))
            {
                return Ok((self.conv(s, ft, t)?, t.clone()));
            }
        }
        let tn = self.tyname(t)?;
        let mut parts = Vec::new();
        for (s, ft) in &vals {
            match ft {
                Ty::Scalar(_) | Ty::Vector(..) => {
                    let b = t.base().unwrap_or(Base::Float);
                    let n = ft.comps().unwrap_or(1);
                    parts.push(self.conv(s, ft, &Ty::vec(b, n))?);
                }
                Ty::Matrix(_, r, c) => {
                    for i in 0..*r {
                        let _ = c;
                        parts.push(format!("{s}[{i}]"));
                    }
                }
                _ => return Err(format!("constructeur {} : argument {}", tn, self.show(ft))),
            }
        }
        Ok((format!("{tn}({})", parts.join(", ")), t.clone()))
    }

    /// Composantes scalaires d'une liste d'initialisation.
    fn flatten_list(&mut self, items: &[Expr], out: &mut Vec<(String, Ty)>) -> R<()> {
        for it in items {
            if let ExprKind::InitList(l) = &it.k {
                self.flatten_list(l, out)?;
                continue;
            }
            let (s, t) = self.rv(it)?;
            self.flatten_val(&s, &t, out)?;
        }
        Ok(())
    }

    fn flatten_val(&self, s: &str, t: &Ty, out: &mut Vec<(String, Ty)>) -> R<()> {
        match t {
            Ty::Scalar(_) => out.push((s.to_string(), t.clone())),
            Ty::Vector(b, n) => {
                for k in 0..*n {
                    out.push((format!("{s}.{}", SW[k as usize]), Ty::Scalar(*b)));
                }
            }
            Ty::Matrix(b, r, c) => {
                for i in 0..*r {
                    for j in 0..*c {
                        out.push((format!("{s}[{i}][{j}]"), Ty::Scalar(*b)));
                    }
                }
            }
            Ty::Array(e, n) => {
                for k in 0..*n {
                    self.flatten_val(&format!("{s}[{k}]"), e, out)?;
                }
            }
            Ty::Struct(i) => {
                for m in &self.p.structs[*i].members {
                    self.flatten_val(&format!("{s}.{}", mg("m_", &m.name)), &m.ty, out)?;
                }
            }
            _ => return Err("ressource dans une liste d'initialisation".into()),
        }
        Ok(())
    }

    fn init_list(&mut self, items: &[Expr], t: &Ty) -> R<String> {
        // Une seule valeur du bon type : affectation directe.
        if items.len() == 1 && !matches!(items[0].k, ExprKind::InitList(_)) {
            let (s, ft) = self.rv(&items[0])?;
            if &ft == t {
                return Ok(s);
            }
            let mut v = vec![(s, ft)];
            let mut flat = Vec::new();
            for (s, t) in v.drain(..) {
                self.flatten_val(&s, &t, &mut flat)?;
            }
            let mut it = flat.into_iter();
            return self.build(t, &mut it);
        }
        let mut flat = Vec::new();
        self.flatten_list(items, &mut flat)?;
        let mut it = flat.into_iter();
        self.build(t, &mut it)
    }

    fn build(&self, t: &Ty, it: &mut std::vec::IntoIter<(String, Ty)>) -> R<String> {
        Ok(match t {
            Ty::Scalar(b) => match it.next() {
                Some((s, ft)) => self.conv(&s, &ft, &Ty::Scalar(*b))?,
                None => self.zero(t)?,
            },
            Ty::Vector(b, n) => {
                let mut v = Vec::new();
                for _ in 0..*n {
                    v.push(self.build(&Ty::Scalar(*b), it)?);
                }
                format!("{}({})", vec_name(*b, *n), v.join(", "))
            }
            Ty::Matrix(b, r, c) => {
                let mut v = Vec::new();
                for _ in 0..(*r as u32 * *c as u32) {
                    v.push(self.build(&Ty::Scalar(*b), it)?);
                }
                format!("{}({})", self.tyname(t)?, v.join(", "))
            }
            Ty::Array(e, n) => {
                let mut v = Vec::new();
                for _ in 0..*n {
                    v.push(self.build(e, it)?);
                }
                format!("{}({})", self.tyname(t)?, v.join(", "))
            }
            Ty::Struct(i) => {
                let mut v = Vec::new();
                for m in &self.p.structs[*i].members {
                    v.push(self.build(&m.ty, it)?);
                }
                format!("{}({})", self.struct_name(*i), v.join(", "))
            }
            _ => return Err("liste d'initialisation d'une ressource".into()),
        })
    }

    // ---------- Fonctions de l'utilisateur ----------

    fn arg_ty(v: &V) -> Ty {
        match v {
            V::E(_, t) => t.clone(),
            V::Cb(c) => c.ty.clone(),
            V::Res(r) => r.ty.clone(),
        }
    }

    fn cost(from: &Ty, to: &Ty) -> Option<u32> {
        if from == to {
            return Some(0);
        }
        match (from, to) {
            (Ty::Texture(a, _), Ty::Texture(b, _)) if a == b => Some(0),
            (Ty::Sampler(_), Ty::Sampler(_)) => Some(0),
            (Ty::Scalar(_), Ty::Scalar(_)) => Some(1),
            (Ty::Vector(_, n), Ty::Vector(_, m)) if n == m => Some(1),
            (Ty::Matrix(_, r, c), Ty::Matrix(_, r2, c2)) if r == r2 && c == c2 => Some(1),
            (Ty::Scalar(_), Ty::Vector(..) | Ty::Matrix(..)) => Some(2),
            (Ty::Vector(_, n), Ty::Vector(_, m)) if m < n => Some(3),
            (Ty::Vector(..), Ty::Scalar(_)) => Some(4),
            (Ty::Matrix(_, r, c), Ty::Matrix(_, r2, c2)) if r2 <= r && c2 <= c => Some(3),
            _ => None,
        }
    }

    fn user_call(&mut self, name: &str, args: &[Expr]) -> R<(String, Ty)> {
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a)?);
        }
        let tys: Vec<Ty> = vals.iter().map(Self::arg_ty).collect();
        let p = self.p;
        let mut best: Option<(u32, usize)> = None;
        for (fi, f) in p.funcs.iter().enumerate() {
            if f.name != name || tys.len() > f.params.len() {
                continue;
            }
            if f.params[tys.len()..].iter().any(|x| x.default.is_none()) {
                continue;
            }
            let mut total = 0;
            let mut ok = true;
            for (t, prm) in tys.iter().zip(&f.params) {
                match Self::cost(t, &prm.ty) {
                    Some(c) => total += c,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && best.is_none_or(|(b, _)| total < b) {
                best = Some((total, fi));
            }
        }
        let (_, mut fi) = best.ok_or_else(|| {
            let t: Vec<String> = tys.iter().map(|t| self.show(t)).collect();
            format!("aucune surcharge de {name}({})", t.join(", "))
        })?;
        // Prototype : chercher la définition de même signature.
        if p.funcs[fi].body.is_none() {
            let sig: Vec<&Ty> = p.funcs[fi].params.iter().map(|x| &x.ty).collect();
            fi = p
                .funcs
                .iter()
                .position(|f| {
                    f.name == name
                        && f.body.is_some()
                        && f.params.iter().map(|x| &x.ty).collect::<Vec<_>>() == sig
                })
                .ok_or_else(|| format!("fonction {name} sans définition"))?;
        }
        let f = &p.funcs[fi];
        // Clé de spécialisation : ressources passées.
        let mut key = format!("{fi}");
        let mut res_args = Vec::new();
        for (k, prm) in f.params.iter().enumerate() {
            if prm.ty.is_resource() {
                let Some(V::Res(r)) = vals.get(k) else {
                    return Err(format!("{name} : ressource attendue en argument {}", k + 1));
                };
                if let Idx::Dyn(_) = r.idx {
                    return Err(format!(
                        "{name} : ressource d'indice dynamique passée en argument"
                    ));
                }
                let _ = write!(key, "|{}:{:?}", r.g, r.idx);
                res_args.push((k, r.clone()));
            }
        }
        let (fname, ret) = match self.funcs_done.get(&key) {
            Some(x) => x.clone(),
            None => self.gen_func(fi, &key, &res_args)?,
        };
        // Arguments.
        let mut a = Vec::new();
        for (k, prm) in f.params.iter().enumerate() {
            if prm.ty.is_resource() {
                continue;
            }
            let (s, t) = match vals.get(k) {
                Some(v) => self.load(v.clone())?,
                None => {
                    let d = prm.default.as_ref().ok_or("argument manquant")?;
                    self.rv(d)?
                }
            };
            if prm.dir == Dir::In {
                a.push(self.conv(&s, &t, &prm.ty)?);
            } else if t == prm.ty {
                a.push(s);
            } else {
                return Err(format!(
                    "{name} : argument out {} de type {} au lieu de {}",
                    k + 1,
                    self.show(&t),
                    self.show(&prm.ty)
                ));
            }
        }
        Ok((format!("{fname}({})", a.join(", ")), ret))
    }

    fn gen_func(&mut self, fi: usize, key: &str, res: &[(usize, ResRef)]) -> R<(String, Ty)> {
        let f = &self.p.funcs[fi];
        self.nfunc += 1;
        let fname = mg(&format!("f{}_", self.nfunc), &f.name);
        self.funcs_done
            .insert(key.to_string(), (fname.clone(), f.ret.clone()));
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_body = std::mem::take(&mut self.body);
        let saved_ret = std::mem::replace(&mut self.cur_ret, f.ret.clone());
        let saved_depth = std::mem::replace(&mut self.depth, 1);
        self.scopes.push(HashMap::new());
        let mut params = Vec::new();
        let mut result = Ok(());
        for (k, prm) in f.params.iter().enumerate() {
            if let Some((_, r)) = res.iter().find(|(i, _)| *i == k) {
                self.declare(&prm.name, Local::Res(r.clone()));
                continue;
            }
            let n = mg("h", &prm.name);
            let q = match prm.dir {
                Dir::In => "",
                Dir::Out => "out ",
                Dir::InOut => "inout ",
            };
            match self.tyname(&prm.ty) {
                Ok(t) => params.push(format!("{q}{t} {n}")),
                Err(e) => {
                    result = Err(format!("({}) : {e}", f.line));
                    break;
                }
            }
            self.declare(&prm.name, Local::Var(n, prm.ty.clone()));
        }
        if result.is_ok() {
            for s in f.body.as_deref().unwrap_or(&[]) {
                if let Err(e) = self.stmt(s) {
                    result = Err(e);
                    break;
                }
            }
        }
        let body = std::mem::replace(&mut self.body, saved_body);
        self.scopes = saved_scopes;
        self.cur_ret = saved_ret;
        self.depth = saved_depth;
        result?;
        let ret = self
            .tyname(&f.ret)
            .map_err(|e| format!("({}) : {e}", f.line))?;
        let sig = format!("{ret} {fname}({})", params.join(", "));
        self.protos.push(format!("{sig};"));
        self.defs.push(format!("{sig} {{\n{body}}}\n"));
        Ok((fname, f.ret.clone()))
    }

    // ---------- Fonctions intrinsèques ----------

    fn args(&mut self, args: &[Expr]) -> R<Vec<(String, Ty)>> {
        args.iter().map(|a| self.rv(a)).collect()
    }

    fn need(name: &str, args: &[Expr], n: usize) -> R<()> {
        if args.len() == n {
            Ok(())
        } else {
            Err(format!("{name} attend {n} argument(s)"))
        }
    }

    /// Arguments convertis vers leur type commun (base forcée si `base`).
    fn unify(&self, v: &[(String, Ty)], base: Option<Base>) -> R<(Vec<String>, Ty)> {
        let mut t = v[0].1.clone();
        for (_, x) in &v[1..] {
            t = self.common(&t, x, false)?;
        }
        if let Some(b) = base {
            t = t.with_base(b);
        } else if t.base() == Some(Base::Bool) {
            t = t.with_base(Base::Int);
        }
        let s: R<Vec<String>> = v.iter().map(|(s, ft)| self.conv(s, ft, &t)).collect();
        Ok((s?, t))
    }

    fn intrinsic(&mut self, name: &str, args: &[Expr]) -> R<(String, Ty)> {
        let f1 = |n: &str| -> Option<&'static str> {
            Some(match n {
                "acos" => "acos",
                "asin" => "asin",
                "atan" => "atan",
                "ceil" => "ceil",
                "cos" => "cos",
                "cosh" => "cosh",
                "degrees" => "degrees",
                "exp" => "exp",
                "exp2" => "exp2",
                "floor" => "floor",
                "frac" => "fract",
                "log" => "log",
                "log2" => "log2",
                "radians" => "radians",
                "round" => "roundEven",
                "rsqrt" => "inversesqrt",
                "sin" => "sin",
                "sinh" => "sinh",
                "sqrt" => "sqrt",
                "tan" => "tan",
                "tanh" => "tanh",
                "trunc" => "trunc",
                "ddx" => "dFdx",
                "ddy" => "dFdy",
                "ddx_coarse" => "dFdxCoarse",
                "ddy_coarse" => "dFdyCoarse",
                "ddx_fine" => "dFdxFine",
                "ddy_fine" => "dFdyFine",
                "fwidth" => "fwidth",
                "normalize" => "normalize",
                _ => return None,
            })
        };
        if let Some(g) = f1(name) {
            Self::need(name, args, 1)?;
            let v = self.args(args)?;
            let (s, t) = self.unify(&v, Some(Base::Float))?;
            return Ok((format!("{g}({})", s[0]), t));
        }
        let fl = Some(Base::Float);
        Ok(match name {
            "abs" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                if t.base() == Some(Base::Uint) {
                    (s[0].clone(), t)
                } else {
                    (format!("abs({})", s[0]), t)
                }
            }
            "saturate" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                (format!("clamp({}, 0.0, 1.0)", s[0]), t)
            }
            "rcp" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                (format!("(1.0 / {})", s[0]), t)
            }
            "log10" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                (format!("(log2({}) * 0.30102999566)", s[0]), t)
            }
            "sign" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                let it = t.with_base(Base::Int);
                let g = format!("sign({})", s[0]);
                (self.conv(&g, &t, &it)?, it)
            }
            "atan2" | "pow" | "step" | "fmod" | "reflect" | "ldexp" => {
                Self::need(name, args, 2)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                let (a, b) = (&s[0], &s[1]);
                match name {
                    "atan2" => (format!("atan({a}, {b})"), t),
                    "fmod" => {
                        self.helpers.insert("mod");
                        (format!("w_mod({a}, {b})"), t)
                    }
                    "ldexp" => (format!("({a} * exp2({b}))"), t),
                    _ => (format!("{name}({a}, {b})"), t),
                }
            }
            "min" | "max" => {
                Self::need(name, args, 2)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                (format!("{name}({}, {})", s[0], s[1]), t)
            }
            "clamp" => {
                Self::need(name, args, 3)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                (format!("clamp({}, {}, {})", s[0], s[1], s[2]), t)
            }
            "lerp" | "smoothstep" | "fma" | "faceforward" => {
                Self::need(name, args, 3)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                let g = match name {
                    "lerp" => "mix",
                    n => n,
                };
                (format!("{g}({}, {}, {})", s[0], s[1], s[2]), t)
            }
            "mad" => {
                Self::need(name, args, 3)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                (format!("({} * {} + {})", s[0], s[1], s[2]), t)
            }
            "refract" => {
                Self::need(name, args, 3)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v[..2], fl)?;
                let e = self.conv(&v[2].0, &v[2].1, &Ty::Scalar(Base::Float))?;
                (format!("refract({}, {}, {e})", s[0], s[1]), t)
            }
            "dot" => {
                Self::need(name, args, 2)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, None)?;
                let b = t.base().unwrap_or(Base::Float);
                match t.comps() {
                    Some(1) => (format!("({} * {})", s[0], s[1]), t),
                    Some(n) if b == Base::Float => {
                        let _ = n;
                        (format!("dot({}, {})", s[0], s[1]), Ty::Scalar(b))
                    }
                    Some(n) => {
                        let p: Vec<String> = (0..n as usize)
                            .map(|k| format!("{}.{} * {}.{}", s[0], SW[k], s[1], SW[k]))
                            .collect();
                        (format!("({})", p.join(" + ")), Ty::Scalar(b))
                    }
                    None => return Err("dot sur une matrice".into()),
                }
            }
            "length" | "distance" => {
                let v = self.args(args)?;
                if v.is_empty() {
                    return Err(format!("{name} sans argument"));
                }
                let (s, _) = self.unify(&v, fl)?;
                (format!("{name}({})", s.join(", ")), Ty::Scalar(Base::Float))
            }
            "cross" => {
                Self::need(name, args, 2)?;
                let v = self.args(args)?;
                let t = Ty::Vector(Base::Float, 3);
                let a = self.conv(&v[0].0, &v[0].1, &t)?;
                let b = self.conv(&v[1].0, &v[1].1, &t)?;
                (format!("cross({a}, {b})"), t)
            }
            "determinant" | "transpose" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let Ty::Matrix(b, r, c) = t else {
                    return Err(format!("{name} attend une matrice"));
                };
                if name == "determinant" {
                    (format!("determinant({s})"), Ty::Scalar(b))
                } else {
                    (format!("transpose({s})"), Ty::Matrix(b, c, r))
                }
            }
            "all" | "any" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let bt = t.with_base(Base::Bool);
                let b = self.conv(&s, &t, &bt)?;
                match t.comps() {
                    Some(1) => (b, Ty::Scalar(Base::Bool)),
                    Some(_) => (format!("{name}({b})"), Ty::Scalar(Base::Bool)),
                    None => return Err(format!("{name} sur une matrice")),
                }
            }
            "isnan" | "isinf" | "isfinite" => {
                Self::need(name, args, 1)?;
                let v = self.args(args)?;
                let (s, t) = self.unify(&v, fl)?;
                let bt = t.with_base(Base::Bool);
                let n = t.comps().unwrap_or(1);
                if name == "isfinite" {
                    let inf = "uintBitsToFloat(0x7F800000u)";
                    if n == 1 {
                        (format!("(abs({}) < {inf})", s[0]), bt)
                    } else {
                        (format!("lessThan(abs({}), vec{n}({inf}))", s[0]), bt)
                    }
                } else {
                    (format!("{name}({})", s[0]), bt)
                }
            }
            "asfloat" | "asint" | "asuint" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let to = match name {
                    "asfloat" => Base::Float,
                    "asint" => Base::Int,
                    _ => Base::Uint,
                };
                let from = t
                    .base()
                    .ok_or_else(|| format!("{name} sur {}", self.show(&t)))?;
                let rt = t.with_base(to);
                let n = t.comps().unwrap_or(1);
                let s = match (from, to) {
                    (a, b) if a == b => s,
                    (Base::Float, Base::Int) => format!("floatBitsToInt({s})"),
                    (Base::Float, Base::Uint) => format!("floatBitsToUint({s})"),
                    (Base::Int, Base::Float) => format!("intBitsToFloat({s})"),
                    (Base::Uint, Base::Float) => format!("uintBitsToFloat({s})"),
                    (Base::Bool, Base::Float) => {
                        format!(
                            "uintBitsToFloat({})",
                            self.conv(&s, &t, &t.with_base(Base::Uint))?
                        )
                    }
                    _ => format!("{}({s})", vec_name(to, n)),
                };
                (s, rt)
            }
            "f16tof32" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let n = t.comps().unwrap_or(1);
                let u = self.conv(&s, &t, &Ty::vec(Base::Uint, n))?;
                if n == 1 {
                    (format!("unpackHalf2x16({u}).x"), Ty::Scalar(Base::Float))
                } else {
                    let p: Vec<String> = (0..n as usize)
                        .map(|k| format!("unpackHalf2x16({u}.{}).x", SW[k]))
                        .collect();
                    (
                        format!("vec{n}({})", p.join(", ")),
                        Ty::Vector(Base::Float, n),
                    )
                }
            }
            "f32tof16" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let n = t.comps().unwrap_or(1);
                let f = self.conv(&s, &t, &Ty::vec(Base::Float, n))?;
                if n == 1 {
                    (
                        format!("(packHalf2x16(vec2({f}, 0.0)) & 0xFFFFu)"),
                        Ty::Scalar(Base::Uint),
                    )
                } else {
                    let p: Vec<String> = (0..n as usize)
                        .map(|k| format!("(packHalf2x16(vec2({f}.{}, 0.0)) & 0xFFFFu)", SW[k]))
                        .collect();
                    (
                        format!("uvec{n}({})", p.join(", ")),
                        Ty::Vector(Base::Uint, n),
                    )
                }
            }
            "countbits" | "firstbithigh" | "firstbitlow" | "reversebits" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let ut = t.with_base(if t.base() == Some(Base::Int) {
                    Base::Int
                } else {
                    Base::Uint
                });
                let s = self.conv(&s, &t, &ut)?;
                let g = match name {
                    "countbits" => "bitCount",
                    "firstbithigh" => "findMSB",
                    "firstbitlow" => "findLSB",
                    _ => "bitfieldReverse",
                };
                let r = format!("{g}({s})");
                let rt = ut.with_base(Base::Uint);
                if name == "reversebits" {
                    (self.conv(&r, &ut, &rt)?, rt)
                } else {
                    (self.conv(&r, &ut.with_base(Base::Int), &rt)?, rt)
                }
            }
            "mul" => {
                Self::need(name, args, 2)?;
                let (a, ta) = self.rv(&args[0])?;
                let (b, tb) = self.rv(&args[1])?;
                self.mul(a, ta, b, tb)?
            }
            "sincos" => {
                Self::need(name, args, 3)?;
                let v = self.args(args)?;
                let t = v[0].1.with_base(Base::Float);
                let x = self.conv(&v[0].0, &v[0].1, &t)?;
                let s = self.conv(&format!("sin({x})"), &t, &v[1].1)?;
                let c = self.conv(&format!("cos({x})"), &t, &v[2].1)?;
                (format!("({} = {s}, {} = {c})", v[1].0, v[2].0), Ty::Void)
            }
            "modf" => {
                Self::need(name, args, 2)?;
                let v = self.args(args)?;
                let t = v[0].1.with_base(Base::Float);
                let x = self.conv(&v[0].0, &v[0].1, &t)?;
                (format!("modf({x}, {})", v[1].0), t)
            }
            "D3DCOLORtoUBYTE4" => {
                Self::need(name, args, 1)?;
                let (s, t) = self.rv(&args[0])?;
                let f = self.conv(&s, &t, &Ty::Vector(Base::Float, 4))?;
                (
                    format!("ivec4({f}.zyxw * 255.001953)"),
                    Ty::Vector(Base::Int, 4),
                )
            }
            "clip" => return Err("clip() hors d'une instruction".into()),
            _ => return Err(format!("fonction inconnue « {name} »")),
        })
    }

    fn mul(&self, a: String, ta: Ty, b: String, tb: Ty) -> R<(String, Ty)> {
        match (&ta, &tb) {
            (Ty::Scalar(_), _) | (_, Ty::Scalar(_)) => {
                let c = self.common(&ta, &tb, true)?;
                let x = self.operand(&a, &ta, &c)?;
                let y = self.operand(&b, &tb, &c)?;
                Ok((format!("({x} * {y})"), c))
            }
            (Ty::Vector(_, n), Ty::Vector(_, m)) => {
                let k = (*n).min(*m);
                let t = Ty::Vector(Base::Float, k);
                let x = self.conv(&a, &ta, &t)?;
                let y = self.conv(&b, &tb, &t)?;
                Ok((format!("dot({x}, {y})"), Ty::Scalar(Base::Float)))
            }
            (Ty::Vector(_, n), Ty::Matrix(_, r, c)) => {
                let k = (*n).min(*r);
                let m = if k < *r {
                    self.conv(&b, &tb, &Ty::Matrix(Base::Float, k, *c))?
                } else {
                    b
                };
                let x = self.conv(&a, &ta, &Ty::Vector(Base::Float, k))?;
                Ok((format!("({m} * {x})"), Ty::vec(Base::Float, *c)))
            }
            (Ty::Matrix(_, r, c), Ty::Vector(_, n)) => {
                let k = (*n).min(*c);
                let m = if k < *c {
                    self.conv(&a, &ta, &Ty::Matrix(Base::Float, *r, k))?
                } else {
                    a
                };
                let y = self.conv(&b, &tb, &Ty::Vector(Base::Float, k))?;
                Ok((format!("({y} * {m})"), Ty::vec(Base::Float, *r)))
            }
            (Ty::Matrix(_, r, k), Ty::Matrix(_, k2, c)) => {
                if k != k2 {
                    return Err("mul : dimensions de matrices incompatibles".into());
                }
                Ok((format!("({b} * {a})"), Ty::Matrix(Base::Float, *r, *c)))
            }
            _ => Err(format!(
                "mul({}, {}) non pris en charge",
                self.show(&ta),
                self.show(&tb)
            )),
        }
    }

    // ---------- Textures ----------

    fn res_glsl(&mut self, r: &ResRef) -> String {
        self.used_res.insert(r.g);
        let n = mg("t_", &self.globals[r.g].d.name);
        match &r.idx {
            Idx::None => n,
            Idx::Const(k) => format!("{n}[{k}]"),
            Idx::Dyn(s) => format!("{n}[{s}]"),
        }
    }

    fn sampler_arg(&mut self, tex: &ResRef, e: &Expr) -> R<()> {
        match self.expr(e)? {
            V::Res(s) if matches!(s.ty, Ty::Sampler(_)) => {
                self.pairs
                    .push((tex.g, tex.idx.clone(), s.g, s.idx.clone()));
                Ok(())
            }
            _ => Err("échantillonneur attendu".into()),
        }
    }

    fn tex_result(&self, s: String, elem: &Ty) -> R<(String, Ty)> {
        let b = elem.base().unwrap_or(Base::Float);
        let n = elem.comps().unwrap_or(4);
        let r = if n == 4 {
            s
        } else {
            format!("{s}.{}", swz(0, n as u32))
        };
        let t = Ty::vec(b, n);
        if b == Base::Bool {
            return Ok((
                format!("notEqual({r}, uvec{n}(0u))"),
                t.with_base(Base::Bool),
            ));
        }
        Ok((r, t))
    }

    fn fvec(&mut self, e: &Expr, n: u8) -> R<String> {
        let (s, t) = self.rv(e)?;
        self.conv(&s, &t, &Ty::vec(Base::Float, n))
    }

    fn ivec(&mut self, e: &Expr, n: u8) -> R<String> {
        let (s, t) = self.rv(e)?;
        self.conv(&s, &t, &Ty::vec(Base::Int, n))
    }

    /// Coordonnées sans l'indice de tranche (décalages, gradients).
    fn off_dims(k: TexKind) -> u8 {
        match k {
            TexKind::T1DArray => 1,
            TexKind::T2DArray | TexKind::T2DMSArray | TexKind::Cube => 2,
            TexKind::CubeArray => 3,
            k => k.coords(),
        }
    }

    fn load_texel(&mut self, r: &ResRef, loc: &Expr, extra: Option<&Expr>) -> R<(String, Ty)> {
        let Ty::Texture(k, elem) = r.ty.clone() else {
            return Err("Load sur un non-texture".into());
        };
        let t = self.res_glsl(r);
        let n = k.load_coords();
        let s = match k {
            TexKind::Buffer => format!("texelFetch({t}, {})", self.ivec(loc, 1)?),
            TexKind::T2DMS | TexKind::T2DMSArray => {
                let l = self.ivec(loc, n)?;
                let smp = match extra {
                    Some(e) => self.ivec(e, 1)?,
                    None => "0".into(),
                };
                format!("texelFetch({t}, {l}, {smp})")
            }
            _ => {
                let (s, ty) = self.rv(loc)?;
                let have = ty.comps().unwrap_or(1);
                if have > n {
                    let l = self.conv(&s, &ty, &Ty::vec(Base::Int, n + 1))?;
                    let c = if n == 1 {
                        format!("{l}.x")
                    } else {
                        format!("{l}.{}", swz(0, n as u32))
                    };
                    format!("texelFetch({t}, {c}, {l}.{})", SW[n as usize])
                } else {
                    let l = self.conv(&s, &ty, &Ty::vec(Base::Int, n))?;
                    format!("texelFetch({t}, {l}, 0)")
                }
            }
        };
        let s = match (extra, k) {
            (Some(o), k) if k.has_mips() && k != TexKind::Buffer => {
                let off = self.ivec(o, Self::off_dims(k))?;
                s.replacen("texelFetch(", "texelFetchOffset(", 1)
                    .trim_end_matches(')')
                    .to_string()
                    + &format!(", {off})")
            }
            _ => s,
        };
        self.tex_result(s, &elem)
    }

    fn method(&mut self, v: V, m: &str, args: &[Expr]) -> R<(String, Ty)> {
        let V::Res(r) = v else {
            return Err(format!("méthode « {m} » sur une valeur"));
        };
        let Ty::Texture(k, elem) = r.ty.clone() else {
            return Err(format!("méthode « {m} » sur un échantillonneur"));
        };
        let c = k.coords();
        let od = Self::off_dims(k);
        match m {
            "Sample" | "SampleLevel" | "SampleBias" | "SampleGrad" => {
                if args.len() < 2 {
                    return Err(format!("{m} : arguments manquants"));
                }
                self.sampler_arg(&r, &args[0])?;
                let t = self.res_glsl(&r);
                let uv = self.fvec(&args[1], c)?;
                let (s, rest) = match m {
                    "Sample" => {
                        if args.len() > 2 {
                            let o = self.ivec(&args[2], od)?;
                            (format!("textureOffset({t}, {uv}, {o})"), 3)
                        } else {
                            (format!("texture({t}, {uv})"), 2)
                        }
                    }
                    "SampleLevel" => {
                        let l = self.fvec(args.get(2).ok_or("SampleLevel : niveau")?, 1)?;
                        if args.len() > 3 {
                            let o = self.ivec(&args[3], od)?;
                            (format!("textureLodOffset({t}, {uv}, {l}, {o})"), 4)
                        } else {
                            (format!("textureLod({t}, {uv}, {l})"), 3)
                        }
                    }
                    "SampleBias" => {
                        let b = self.fvec(args.get(2).ok_or("SampleBias : biais")?, 1)?;
                        if args.len() > 3 {
                            let o = self.ivec(&args[3], od)?;
                            (format!("textureOffset({t}, {uv}, {o}, {b})"), 4)
                        } else {
                            (format!("texture({t}, {uv}, {b})"), 3)
                        }
                    }
                    _ => {
                        let dx = self.fvec(args.get(2).ok_or("SampleGrad : ddx")?, od)?;
                        let dy = self.fvec(args.get(3).ok_or("SampleGrad : ddy")?, od)?;
                        if args.len() > 4 {
                            let o = self.ivec(&args[4], od)?;
                            (format!("textureGradOffset({t}, {uv}, {dx}, {dy}, {o})"), 5)
                        } else {
                            (format!("textureGrad({t}, {uv}, {dx}, {dy})"), 4)
                        }
                    }
                };
                let _ = rest;
                self.tex_result(s, &elem)
            }
            "SampleCmp" | "SampleCmpLevelZero" => {
                if args.len() < 3 {
                    return Err(format!("{m} : arguments manquants"));
                }
                self.sampler_arg(&r, &args[0])?;
                let t = self.res_glsl(&r);
                let uv = self.fvec(&args[1], c)?;
                let z = self.fvec(&args[2], 1)?;
                let fetch = if m == "SampleCmp" {
                    format!("texture({t}, {uv})")
                } else {
                    format!("textureLod({t}, {uv}, 0.0)")
                };
                Ok((
                    format!("(({z}) <= {fetch}.x ? 1.0 : 0.0)"),
                    Ty::Scalar(Base::Float),
                ))
            }
            "Load" => {
                let a = args.first().ok_or("Load : coordonnées")?;
                self.load_texel(&r, a, args.get(1))
            }
            "Gather" | "GatherRed" | "GatherGreen" | "GatherBlue" | "GatherAlpha" => {
                if args.len() < 2 {
                    return Err(format!("{m} : arguments manquants"));
                }
                self.sampler_arg(&r, &args[0])?;
                let t = self.res_glsl(&r);
                let uv = self.fvec(&args[1], c)?;
                let comp = match m {
                    "GatherGreen" => ", 1",
                    "GatherBlue" => ", 2",
                    "GatherAlpha" => ", 3",
                    _ => "",
                };
                let s = if args.len() > 2 {
                    let o = self.ivec(&args[2], od)?;
                    format!("textureGatherOffset({t}, {uv}, {o}{comp})")
                } else {
                    format!("textureGather({t}, {uv}{comp})")
                };
                let b = elem.base().unwrap_or(Base::Float);
                Ok((s, Ty::Vector(b, 4)))
            }
            "CalculateLevelOfDetail" | "CalculateLevelOfDetailUnclamped" => {
                if args.len() < 2 {
                    return Err(format!("{m} : arguments manquants"));
                }
                self.sampler_arg(&r, &args[0])?;
                let t = self.res_glsl(&r);
                let uv = self.fvec(&args[1], od)?;
                let k = if m == "CalculateLevelOfDetail" {
                    "x"
                } else {
                    "y"
                };
                Ok((
                    format!("textureQueryLod({t}, {uv}).{k}"),
                    Ty::Scalar(Base::Float),
                ))
            }
            "GetDimensions" => {
                let t = self.res_glsl(&r);
                let dims: u8 = match k {
                    TexKind::Buffer | TexKind::T1D => 1,
                    TexKind::T1DArray | TexKind::T2D | TexKind::Cube => 2,
                    TexKind::T2DArray | TexKind::T3D | TexKind::CubeArray => 3,
                    TexKind::T2DMS => 2,
                    TexKind::T2DMSArray => 3,
                };
                let ms = matches!(k, TexKind::T2DMS | TexKind::T2DMSArray);
                let (lod, outs) = if k.has_mips() && args.len() == dims as usize + 2 {
                    (Some(&args[0]), &args[1..])
                } else {
                    (None, args)
                };
                let size = if k == TexKind::Buffer || ms {
                    format!("textureSize({t})")
                } else {
                    let l = match lod {
                        Some(e) => self.ivec(e, 1)?,
                        None => "0".into(),
                    };
                    format!("textureSize({t}, {l})")
                };
                let mut parts = Vec::new();
                for (i, o) in outs.iter().enumerate() {
                    let (os, ot) = self.rv(o)?;
                    let (val, vt) = if (i as u8) < dims {
                        let v = if dims == 1 {
                            size.clone()
                        } else {
                            format!("{size}.{}", SW[i])
                        };
                        (v, Ty::Scalar(Base::Int))
                    } else if ms {
                        ("1".to_string(), Ty::Scalar(Base::Int))
                    } else {
                        (format!("textureQueryLevels({t})"), Ty::Scalar(Base::Int))
                    };
                    parts.push(format!("{os} = {}", self.conv(&val, &vt, &ot)?));
                }
                Ok((format!("({})", parts.join(", ")), Ty::Void))
            }
            _ => Err(format!("méthode de texture « {m} » non prise en charge")),
        }
    }

    // ---------- Instructions ----------

    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.body.push_str("  ");
        }
        self.body.push_str(s);
        self.body.push('\n');
    }

    fn cond(&mut self, e: &Expr) -> R<String> {
        let (s, t) = self.rv(e)?;
        self.to_bool_scalar(&s, &t).map_err(|m| self.at(e, &m))
    }

    fn block(&mut self, s: &Stmt) -> R<()> {
        self.scopes.push(HashMap::new());
        self.depth += 1;
        let r = match s {
            Stmt::Block(v) => v.iter().try_for_each(|x| self.stmt(x)),
            s => self.stmt(s),
        };
        self.depth -= 1;
        self.scopes.pop();
        r
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Expr(e) => {
                if let ExprKind::Call(n, a) = &e.k {
                    if n == "clip" && !self.p.funcs.iter().any(|f| &f.name == n) {
                        let (x, t) = self.rv(a.first().ok_or("clip sans argument")?)?;
                        let ft = t.with_base(Base::Float);
                        let x = self.conv(&x, &t, &ft)?;
                        let c = match ft.comps() {
                            Some(1) => format!("{x} < 0.0"),
                            Some(n) => format!("any(lessThan({x}, vec{n}(0.0)))"),
                            None => return Err(format!("({}) : clip sur une matrice", e.line)),
                        };
                        self.line(&format!("if ({c}) discard;"));
                        return Ok(());
                    }
                }
                let (x, _) = self.rv(e)?;
                self.line(&format!("{x};"));
            }
            Stmt::Decl(vars) => {
                for d in vars {
                    self.local_decl(d)?;
                }
            }
            Stmt::If(c, a, b) => {
                let c = self.cond(c)?;
                self.line(&format!("if ({c}) {{"));
                self.block(a)?;
                if let Some(b) = b {
                    self.line("} else {");
                    self.block(b)?;
                }
                self.line("}");
            }
            Stmt::For(init, c, step, body) => {
                self.line("{");
                self.depth += 1;
                self.scopes.push(HashMap::new());
                let r = (|| -> R<()> {
                    if let Some(i) = init {
                        self.stmt(i)?;
                    }
                    let c = match c {
                        Some(c) => self.cond(c)?,
                        None => String::new(),
                    };
                    let st = match step {
                        Some(e) => self.rv(e)?.0,
                        None => String::new(),
                    };
                    self.line(&format!("for (; {c}; {st}) {{"));
                    self.block(body)?;
                    self.line("}");
                    Ok(())
                })();
                self.scopes.pop();
                self.depth -= 1;
                r?;
                self.line("}");
            }
            Stmt::While(c, b) => {
                let c = self.cond(c)?;
                self.line(&format!("while ({c}) {{"));
                self.block(b)?;
                self.line("}");
            }
            Stmt::DoWhile(b, c) => {
                self.line("do {");
                self.block(b)?;
                let c = self.cond(c)?;
                self.line(&format!("}} while ({c});"));
            }
            Stmt::Switch(e, body) => {
                let (x, t) = self.rv(e)?;
                let x = self.conv(&x, &t, &Ty::Scalar(Base::Int))?;
                self.line(&format!("switch ({x}) {{"));
                self.scopes.push(HashMap::new());
                self.depth += 1;
                let r = body.iter().try_for_each(|s| self.stmt(s));
                self.depth -= 1;
                self.scopes.pop();
                r?;
                self.line("}");
            }
            Stmt::Case(e) => {
                let v = const_eval(e, &self.p.globals)
                    .ok_or_else(|| format!("({}) : étiquette case non constante", e.line))?;
                self.line(&format!("case {}:", v as i64 as i32));
            }
            Stmt::Default => self.line("default:"),
            Stmt::Break => self.line("break;"),
            Stmt::Continue => self.line("continue;"),
            Stmt::Discard => self.line("discard;"),
            Stmt::Return(e, line) => match e {
                None => self.line("return;"),
                Some(e) => {
                    let rt = self.cur_ret.clone();
                    let s = if let ExprKind::InitList(l) = &e.k {
                        self.init_list(l, &rt)?
                    } else {
                        let (s, t) = self.rv(e)?;
                        if rt == Ty::Void {
                            self.line(&format!("{s};"));
                            self.line("return;");
                            return Ok(());
                        }
                        self.conv(&s, &t, &rt)
                            .map_err(|m| format!("({line}) : {m}"))?
                    };
                    self.line(&format!("return {s};"));
                }
            },
            Stmt::Block(v) => {
                self.line("{");
                self.block(&Stmt::Block(v.clone()))?;
                self.line("}");
            }
            Stmt::Empty => {}
        }
        Ok(())
    }

    fn local_decl(&mut self, d: &VarDecl) -> R<()> {
        if d.ty.is_resource() {
            let init = d
                .init
                .as_ref()
                .ok_or_else(|| format!("({}) : ressource locale sans valeur", d.line))?;
            match self.expr(init)? {
                V::Res(r) => {
                    self.declare(&d.name, Local::Res(r));
                    return Ok(());
                }
                _ => return Err(format!("({}) : ressource attendue", d.line)),
            }
        }
        let n = mg("h", &d.name);
        let tn = self
            .tyname(&d.ty)
            .map_err(|m| format!("({}) : {m}", d.line))?;
        let init = match &d.init {
            None => None,
            Some(e) => Some(if let ExprKind::InitList(l) = &e.k {
                self.init_list(l, &d.ty)?
            } else {
                let (s, t) = self.rv(e)?;
                self.conv(&s, &t, &d.ty)
                    .map_err(|m| format!("({}) : {m}", d.line))?
            }),
        };
        match init {
            Some(i) => self.line(&format!("{tn} {n} = {i};")),
            None => self.line(&format!("{tn} {n};")),
        }
        self.declare(&d.name, Local::Var(n, d.ty.clone()));
        Ok(())
    }

    // ---------- Point d'entrée ----------

    fn flatten_io(
        &self,
        t: &Ty,
        sem: Option<(String, u32)>,
        path: String,
        flat: bool,
        out: &mut Vec<El>,
    ) -> R<()> {
        match (t, sem) {
            (Ty::Struct(i), _) => {
                for m in &self.p.structs[*i].members {
                    self.flatten_io(
                        &m.ty,
                        m.semantic.as_deref().map(split_semantic),
                        format!("{path}.{}", mg("m_", &m.name)),
                        flat || m.interp.flat,
                        out,
                    )?;
                }
            }
            (Ty::Array(e, n), Some((name, idx))) => {
                for k in 0..*n {
                    self.flatten_io(
                        e,
                        Some((name.clone(), idx + k)),
                        format!("{path}[{k}]"),
                        flat,
                        out,
                    )?;
                }
            }
            (_, Some((name, index))) => out.push(El {
                path,
                ty: t.clone(),
                name,
                index,
                flat,
            }),
            (_, None) => return Err(format!("{path} : sémantique manquante")),
        }
        Ok(())
    }

    pub(crate) fn run(mut self, entry: &str) -> R<Output> {
        let p = self.p;
        let fi = p
            .funcs
            .iter()
            .position(|f| f.name == entry && f.body.is_some())
            .ok_or_else(|| format!("point d'entrée « {entry} » introuvable"))?;
        let f = &p.funcs[fi];
        let key = format!("{fi}");
        let (fname, _) = self.gen_func(fi, &key, &[])?;

        // Variables statiques : déclarées sans valeur, initialisées dans main.
        let mut decls = String::new();
        let mut init = String::new();
        self.scopes = vec![HashMap::new()];
        for gi in 0..self.globals.len() {
            if self.globals[gi].kind != GKind::Static {
                continue;
            }
            let d = self.globals[gi].d.clone();
            let n = mg("h", &d.name);
            let Ok(tn) = self.tyname(&d.ty) else {
                continue;
            };
            let _ = writeln!(decls, "{tn} {n};");
            let v = match &d.init {
                Some(e) => {
                    if let ExprKind::InitList(l) = &e.k {
                        self.init_list(l, &d.ty)?
                    } else {
                        let (s, t) = self.rv(e)?;
                        self.conv(&s, &t, &d.ty)
                            .map_err(|m| format!("({}) : {m}", d.line))?
                    }
                }
                None => self.zero(&d.ty)?,
            };
            let _ = writeln!(init, "  {n} = {v};");
        }

        // Entrées et sorties.
        let mut io = String::new();
        let mut pre = String::new();
        let mut post = String::new();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut args = Vec::new();
        let mut in_els = Vec::new();
        let mut out_els = Vec::new();
        for (k, prm) in f.params.iter().enumerate() {
            let n = format!("p{k}");
            let tn = self.tyname(&prm.ty)?;
            let _ = writeln!(pre, "  {tn} {n};");
            args.push(n.clone());
            let sem = prm.semantic.as_deref().map(split_semantic);
            if prm.dir != Dir::Out {
                self.flatten_io(
                    &prm.ty,
                    sem.clone(),
                    n.clone(),
                    prm.interp.flat,
                    &mut in_els,
                )?;
            }
            if prm.dir != Dir::In {
                self.flatten_io(&prm.ty, sem, n.clone(), prm.interp.flat, &mut out_els)?;
            }
        }
        let call = format!("{fname}({})", args.join(", "));
        if f.ret != Ty::Void {
            let tn = self.tyname(&f.ret)?;
            let sem = f.ret_semantic.as_deref().map(split_semantic);
            self.flatten_io(&f.ret, sem, "ret".into(), false, &mut out_els)?;
            let _ = writeln!(post, "  {tn} ret = {call};");
        } else {
            let _ = writeln!(post, "  {call};");
        }

        // Entrées.
        for (reg, e) in (0u32..).zip(&in_els) {
            let b =
                e.ty.base()
                    .ok_or_else(|| format!("{} : entrée non numérique", e.path))?;
            let n =
                e.ty.comps()
                    .ok_or_else(|| format!("{} : entrée matricielle", e.path))?;
            let mut sv = 0;
            let src: (String, Ty) = match (self.stage, e.name.as_str()) {
                (Stage::Vertex, "SV_VERTEXID") => {
                    sv = 6;
                    ("gl_VertexID".into(), Ty::Scalar(Base::Int))
                }
                (Stage::Vertex, "SV_INSTANCEID") => {
                    sv = 8;
                    ("gl_InstanceID".into(), Ty::Scalar(Base::Int))
                }
                (Stage::Vertex, _) => {
                    let (vt, b4) = match b {
                        Base::Int => ("ivec4", Base::Int),
                        Base::Uint | Base::Bool => ("uvec4", Base::Uint),
                        Base::Float => ("vec4", Base::Float),
                    };
                    let _ = writeln!(io, "layout(location = {reg}) in {vt} a{reg};");
                    (format!("a{reg}"), Ty::Vector(b4, 4))
                }
                (Stage::Pixel, "SV_POSITION") => {
                    sv = 1;
                    (
                        "vec4(gl_FragCoord.xyz, 1.0 / gl_FragCoord.w)".into(),
                        Ty::Vector(Base::Float, 4),
                    )
                }
                (Stage::Pixel, "SV_ISFRONTFACE") => {
                    sv = 9;
                    ("gl_FrontFacing".into(), Ty::Scalar(Base::Bool))
                }
                (Stage::Pixel, "SV_PRIMITIVEID") => {
                    sv = 7;
                    ("gl_PrimitiveID".into(), Ty::Scalar(Base::Int))
                }
                (Stage::Pixel, "SV_SAMPLEINDEX") => {
                    sv = 10;
                    ("gl_SampleID".into(), Ty::Scalar(Base::Int))
                }
                _ => {
                    let loc = varying_location(&e.name, e.index);
                    let flat = e.flat || b != Base::Float;
                    let _ = writeln!(
                        io,
                        "layout(location = {loc}) {}in vec4 vary{loc};",
                        if flat { "flat " } else { "" }
                    );
                    let v = format!("vary{loc}");
                    match b {
                        Base::Float => (v, Ty::Vector(Base::Float, 4)),
                        Base::Int => (format!("floatBitsToInt({v})"), Ty::Vector(Base::Int, 4)),
                        _ => (format!("floatBitsToUint({v})"), Ty::Vector(Base::Uint, 4)),
                    }
                }
            };
            let val = self.conv(&src.0, &src.1, &e.ty.with_base(src.1.base().unwrap_or(b)))?;
            let val = self.conv(&val, &e.ty.with_base(src.1.base().unwrap_or(b)), &e.ty)?;
            let _ = writeln!(pre, "  {} = {val};", e.path);
            inputs.push(SigElement {
                name: e.name.clone(),
                index: e.index,
                system_value: sv,
                component_type: ct_of(b),
                register: reg,
                mask: ((1u32 << n) - 1) as u8,
            });
        }

        // Sorties.
        let mut reg = 0u32;
        let mut clip = 0u32;
        for e in &out_els {
            let b =
                e.ty.base()
                    .ok_or_else(|| format!("{} : sortie non numérique", e.path))?;
            let n =
                e.ty.comps()
                    .ok_or_else(|| format!("{} : sortie matricielle", e.path))?;
            let p = &e.path;
            let mut sv = 0;
            let mut r = reg;
            match (self.stage, e.name.as_str()) {
                (Stage::Vertex, "SV_POSITION") => {
                    sv = 1;
                    let v = self
                        .conv(p, &e.ty, &Ty::Vector(Base::Float, 4))
                        .unwrap_or_else(|_| p.clone());
                    let _ = writeln!(post, "  vec4 pos = {v};");
                    let _ = writeln!(
                        post,
                        "  gl_Position = vec4(pos.x, -pos.y, pos.z * 2.0 - pos.w, pos.w);"
                    );
                }
                (Stage::Vertex, "SV_CLIPDISTANCE") => {
                    sv = 2;
                    for k in 0..n {
                        let c = if n == 1 {
                            p.clone()
                        } else {
                            format!("{p}.{}", SW[k as usize])
                        };
                        let _ = writeln!(post, "  gl_ClipDistance[{clip}] = float({c});");
                        clip += 1;
                    }
                }
                (Stage::Vertex, "SV_CULLDISTANCE") => sv = 3,
                (Stage::Vertex, _) => {
                    let loc = varying_location(&e.name, e.index);
                    let flat = e.flat || b != Base::Float;
                    let _ = writeln!(
                        io,
                        "layout(location = {loc}) {}out vec4 vary{loc};",
                        if flat { "flat " } else { "" }
                    );
                    let v = match b {
                        Base::Float => p.clone(),
                        Base::Int => format!("intBitsToFloat({p})"),
                        Base::Uint => format!("uintBitsToFloat({p})"),
                        Base::Bool => format!(
                            "uintBitsToFloat({})",
                            self.conv(p, &e.ty, &e.ty.with_base(Base::Uint))?
                        ),
                    };
                    let _ = writeln!(post, "  vary{loc}.{} = {v};", swz(0, n as u32));
                }
                (Stage::Pixel, "SV_TARGET") | (Stage::Pixel, "COLOR") => {
                    sv = 64;
                    r = e.index;
                    let _ = writeln!(io, "layout(location = {r}) out vec4 target{r};");
                    let f = self.conv(p, &e.ty, &e.ty.with_base(Base::Float))?;
                    let v = match n {
                        4 => f,
                        1 => format!("vec4({f}, 0.0, 0.0, 1.0)"),
                        2 => format!("vec4({f}, 0.0, 1.0)"),
                        _ => format!("vec4({f}, 1.0)"),
                    };
                    let _ = writeln!(post, "  target{r} = {v};");
                }
                (Stage::Pixel, "SV_DEPTH")
                | (Stage::Pixel, "SV_DEPTHGREATEREQUAL")
                | (Stage::Pixel, "SV_DEPTHLESSEQUAL")
                | (Stage::Pixel, "DEPTH") => {
                    sv = 65;
                    r = u32::MAX;
                    let f = self.conv(p, &e.ty, &Ty::Scalar(Base::Float))?;
                    let _ = writeln!(post, "  gl_FragDepth = {f};");
                }
                (Stage::Pixel, "SV_COVERAGE") => {
                    sv = 66;
                    r = u32::MAX;
                    let f = self.conv(p, &e.ty, &Ty::Scalar(Base::Int))?;
                    let _ = writeln!(post, "  gl_SampleMask[0] = {f};");
                }
                (_, other) => {
                    return Err(format!("sémantique de sortie {other} non prise en charge"));
                }
            }
            outputs.push(SigElement {
                name: e.name.clone(),
                index: e.index,
                system_value: sv,
                component_type: ct_of(b),
                register: r,
                mask: ((1u32 << n) - 1) as u8,
            });
            if r == reg {
                reg += 1;
            }
        }

        // Registres implicites des tampons et ressources utilisés.
        let mut cb_slot: HashMap<usize, u32> = HashMap::new();
        let explicit: BTreeSet<u32> = self
            .used_cb
            .iter()
            .filter_map(|&c| self.cbs[c].register)
            .collect();
        let mut order: Vec<usize> = self.used_cb.iter().copied().collect();
        order.sort_by_key(|&c| (self.cbs[c].name != "$Globals", c));
        let mut next = 0;
        for c in order {
            let slot = match self.cbs[c].register {
                Some(r) => r,
                None => {
                    while explicit.contains(&next) {
                        next += 1;
                    }
                    next += 1;
                    next - 1
                }
            };
            cb_slot.insert(c, slot);
        }
        let mut res_reg: HashMap<usize, u32> = HashMap::new();
        for class in ['t', 's'] {
            let is_class = |t: &Ty| -> bool {
                let mut t = t;
                while let Ty::Array(e, _) = t {
                    t = e;
                }
                match t {
                    Ty::Texture(..) => class == 't',
                    Ty::Sampler(_) => class == 's',
                    _ => false,
                }
            };
            let count = |t: &Ty| -> u32 {
                match t {
                    Ty::Array(_, n) => *n,
                    _ => 1,
                }
            };
            let mut taken: Vec<(u32, u32)> = Vec::new();
            for &gi in &self.used_res {
                let d = &self.globals[gi].d;
                if !is_class(&d.ty) {
                    continue;
                }
                if let Some((_, r)) = d.register {
                    res_reg.insert(gi, r);
                    taken.push((r, r + count(&d.ty)));
                }
            }
            for &gi in &self.used_res {
                let d = &self.globals[gi].d;
                if !is_class(&d.ty) || d.register.is_some() {
                    continue;
                }
                let n = count(&d.ty);
                let mut r = 0;
                while taken.iter().any(|&(a, b)| r < b && a < r + n) {
                    r += 1;
                }
                taken.push((r, r + n));
                res_reg.insert(gi, r);
            }
        }

        // Paires texture / échantillonneur.
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for (tg, ti, sg, si) in &self.pairs {
            let (Some(&tr), Some(&sr)) = (res_reg.get(tg), res_reg.get(sg)) else {
                continue;
            };
            let len = |g: usize| match &self.globals[g].d.ty {
                Ty::Array(_, n) => *n,
                _ => 1,
            };
            let off = |i: &Idx| match i {
                Idx::Const(k) => Some(*k),
                Idx::None => Some(0),
                Idx::Dyn(_) => None,
            };
            match (off(ti), off(si)) {
                (Some(a), Some(b)) => pairs.push((tr + a, sr + b)),
                (a, b) => {
                    for k in 0..len(*tg).min(len(*sg)) {
                        pairs.push((tr + a.unwrap_or(k), sr + b.unwrap_or(k)));
                    }
                }
            }
        }
        let mut seen = BTreeSet::new();
        pairs.retain(|p| seen.insert(p.0));

        // Assemblage.
        let base = self.stage.binding_base();
        let tag = if self.stage == Stage::Vertex {
            "vs"
        } else {
            "ps"
        };
        let mut s = String::from("#version 430 core\n");
        for h in &self.helpers {
            s.push_str(match *h {
                "mod" => {
                    "float w_mod(float a, float b) { return a - b * trunc(a / b); }\n\
                     vec2 w_mod(vec2 a, vec2 b) { return a - b * trunc(a / b); }\n\
                     vec3 w_mod(vec3 a, vec3 b) { return a - b * trunc(a / b); }\n\
                     vec4 w_mod(vec4 a, vec4 b) { return a - b * trunc(a / b); }\n"
                }
                "and" => {
                    "bvec2 w_and(bvec2 a, bvec2 b) { return bvec2(a.x && b.x, a.y && b.y); }\n\
                     bvec3 w_and(bvec3 a, bvec3 b) { return bvec3(a.x && b.x, a.y && b.y, a.z && b.z); }\n\
                     bvec4 w_and(bvec4 a, bvec4 b) { return bvec4(a.x && b.x, a.y && b.y, a.z && b.z, a.w && b.w); }\n"
                }
                _ => {
                    "bvec2 w_or(bvec2 a, bvec2 b) { return bvec2(a.x || b.x, a.y || b.y); }\n\
                     bvec3 w_or(bvec3 a, bvec3 b) { return bvec3(a.x || b.x, a.y || b.y, a.z || b.z); }\n\
                     bvec4 w_or(bvec4 a, bvec4 b) { return bvec4(a.x || b.x, a.y || b.y, a.z || b.z, a.w || b.w); }\n"
                }
            });
        }
        for (i, st) in p.structs.iter().enumerate() {
            if st.members.is_empty() || st.members.iter().any(|m| m.ty.is_resource()) {
                continue;
            }
            let _ = writeln!(s, "struct {} {{", self.struct_name(i));
            for m in &st.members {
                let _ = writeln!(s, "  {} {};", self.tyname(&m.ty)?, mg("m_", &m.name));
            }
            s.push_str("};\n");
        }
        for (&c, &slot) in &cb_slot {
            let regs = self.cbs[c].size.div_ceil(4).max(1);
            let _ = writeln!(
                s,
                "layout(std140, binding = {}) uniform cb{c}_{tag} {{ vec4 cb{c}[{regs}]; }};",
                base + slot
            );
        }
        for (&gi, &r) in &res_reg {
            let d = &self.globals[gi].d;
            let (t, arr) = match &d.ty {
                Ty::Array(e, n) => ((**e).clone(), format!("[{n}]")),
                t => (t.clone(), String::new()),
            };
            if let Ty::Texture(k, elem) = t {
                let pre = vec_prefix(elem.base().unwrap_or(Base::Float));
                let pre = if pre == "b" { "u" } else { pre };
                let _ = writeln!(
                    s,
                    "layout(binding = {}) uniform {pre}sampler{} {}{arr};",
                    base + r,
                    k.glsl(),
                    mg("t_", &d.name)
                );
            }
        }
        s.push_str(&io);
        s.push_str(&decls);
        for x in &self.protos {
            s.push_str(x);
            s.push('\n');
        }
        for x in &self.defs {
            s.push_str(x);
        }
        let _ = write!(s, "void main() {{\n{init}{pre}{post}}}\n");
        Ok(Output {
            stage: self.stage,
            glsl: s,
            inputs,
            outputs,
            pairs,
        })
    }
}
