//! Analyse syntaxique HLSL (modèles 4 et 5).

use super::ast::*;
use super::lex::{Tok, Token};
use std::collections::HashMap;

pub(crate) struct Parser {
    t: Vec<Token>,
    p: usize,
    prog: Program,
    typedefs: HashMap<String, Ty>,
    row_major_default: bool,
    /// Dernier `[maxvertexcount(N)]` lu.
    max_vertex_count: u32,
}

type R<T> = Result<T, String>;

const MODIFIERS: &[&str] = &[
    "static",
    "const",
    "uniform",
    "extern",
    "volatile",
    "shared",
    "groupshared",
    "precise",
    "inline",
    "row_major",
    "column_major",
    "nointerpolation",
    "linear",
    "centroid",
    "noperspective",
    "sample",
    "in",
    "out",
    "inout",
    "snorm",
    "unorm",
    "export",
    "globallycoherent",
];

#[derive(Default, Clone, Copy)]
struct Mods {
    is_static: bool,
    is_const: bool,
    row_major: Option<bool>,
    flat: bool,
    dir_in: bool,
    dir_out: bool,
}

pub(crate) fn parse(tokens: Vec<Token>, row_major: bool) -> R<Program> {
    let mut p = Parser {
        t: tokens,
        p: 0,
        prog: Program::default(),
        typedefs: HashMap::new(),
        row_major_default: row_major,
        max_vertex_count: 0,
    };
    while p.p < p.t.len() {
        p.top()?;
    }
    Ok(p.prog)
}

fn scalar_base(s: &str) -> Option<Base> {
    Some(match s {
        "bool" => Base::Bool,
        "int" | "min16int" | "min12int" | "int16_t" | "int32_t" => Base::Int,
        "uint" | "dword" | "min16uint" | "uint16_t" | "uint32_t" => Base::Uint,
        "float" | "half" | "double" | "min16float" | "min10float" | "float16_t" | "float32_t" => {
            Base::Float
        }
        _ => return None,
    })
}

/// « float4x3 », « int2 », « half » → type.
fn builtin_numeric(s: &str) -> Option<Ty> {
    let end = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    // « int16_t » et compagnie : nom complet d'abord.
    if let Some(b) = scalar_base(s) {
        return Some(Ty::Scalar(b));
    }
    let b = scalar_base(&s[..end])?;
    let dims = &s[end..];
    let d: Vec<u8> = dims.split('x').filter_map(|x| x.parse().ok()).collect();
    match d.as_slice() {
        [n] if (1..=4).contains(n) && !dims.contains('x') => Some(Ty::vec(b, *n)),
        [r, c] if (1..=4).contains(r) && (1..=4).contains(c) => Some(matrix(b, *r, *c)),
        _ => None,
    }
}

fn matrix(b: Base, r: u8, c: u8) -> Ty {
    if r == 1 {
        Ty::vec(b, c)
    } else if c == 1 {
        Ty::vec(b, r)
    } else {
        Ty::Matrix(b, r, c)
    }
}

fn tex_kind(s: &str) -> Option<TexKind> {
    Some(match s {
        "Buffer" => TexKind::Buffer,
        "Texture1D" => TexKind::T1D,
        "Texture1DArray" => TexKind::T1DArray,
        "Texture2D" | "texture" | "texture2D" | "Texture" => TexKind::T2D,
        "Texture2DArray" => TexKind::T2DArray,
        "Texture2DMS" => TexKind::T2DMS,
        "Texture2DMSArray" => TexKind::T2DMSArray,
        "Texture3D" | "texture3D" => TexKind::T3D,
        "TextureCube" | "textureCUBE" => TexKind::Cube,
        "TextureCubeArray" => TexKind::CubeArray,
        _ => return None,
    })
}

impl Parser {
    fn err<T>(&self, msg: &str) -> R<T> {
        let line = self.t.get(self.p).or(self.t.last()).map_or(0, |t| t.line);
        Err(format!("({line}) : {msg}"))
    }
    fn line(&self) -> u32 {
        self.t.get(self.p).or(self.t.last()).map_or(0, |t| t.line)
    }
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p).map(|t| &t.t)
    }
    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.t.get(self.p + n).map(|t| &t.t)
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(x)) if *x == p)
    }
    fn is_ident(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(x)) if x == s)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            self.err(&format!("« {p} » attendu"))
        }
    }
    fn ident(&mut self) -> R<String> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.p += 1;
                Ok(s)
            }
            _ => self.err("identifiant attendu"),
        }
    }

    /// Saute un attribut `[...]` (numthreads, unroll, loop…) ; retient
    /// `[maxvertexcount(N)]` pour la fonction qui suit.
    fn skip_attributes(&mut self) {
        while self.is("[") {
            if let (Some(Tok::Ident(a)), Some(Tok::Punct("(")), Some(Tok::Int(n, _))) =
                (self.peek_at(1), self.peek_at(2), self.peek_at(3))
            {
                if a == "maxvertexcount" {
                    self.max_vertex_count = *n as u32;
                }
            }
            let mut depth = 0;
            while let Some(t) = self.peek() {
                match t {
                    Tok::Punct("[") => depth += 1,
                    Tok::Punct("]") => {
                        depth -= 1;
                        if depth == 0 {
                            self.p += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                self.p += 1;
            }
        }
    }

    fn mods(&mut self) -> Mods {
        let mut m = Mods::default();
        while let Some(Tok::Ident(s)) = self.peek() {
            if !MODIFIERS.contains(&s.as_str()) {
                break;
            }
            match s.as_str() {
                "static" => m.is_static = true,
                "const" => m.is_const = true,
                "row_major" => m.row_major = Some(true),
                "column_major" => m.row_major = Some(false),
                "nointerpolation" => m.flat = true,
                "in" => m.dir_in = true,
                "out" => m.dir_out = true,
                "inout" => {
                    m.dir_in = true;
                    m.dir_out = true;
                }
                _ => {}
            }
            self.p += 1;
        }
        m
    }

    fn starts_type(&self) -> bool {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                builtin_numeric(s).is_some()
                    || tex_kind(s).is_some()
                    || matches!(
                        s.as_str(),
                        "void"
                            | "vector"
                            | "matrix"
                            | "SamplerState"
                            | "SamplerComparisonState"
                            | "sampler"
                            | "sampler_state"
                            | "struct"
                    )
                    || self.typedefs.contains_key(s)
                    || self.prog.structs.iter().any(|d| &d.name == s)
            }
            _ => false,
        }
    }

    /// Lit un type (sans les dimensions de tableau du déclarateur).
    fn ty(&mut self) -> R<Ty> {
        let name = self.ident()?;
        if let Some(t) = builtin_numeric(&name) {
            return Ok(t);
        }
        if let Some(t) = self.typedefs.get(&name) {
            return Ok(t.clone());
        }
        if let Some(i) = self.prog.structs.iter().position(|d| d.name == name) {
            return Ok(Ty::Struct(i));
        }
        if let Some(k) = tex_kind(&name) {
            let mut elem = Ty::Vector(Base::Float, 4);
            if self.eat("<") {
                elem = self.ty()?;
                if self.eat(",") {
                    // Texture2DMS<float4, 4> : nombre d'échantillons.
                    self.p += 1;
                }
                self.expect(">")?;
            }
            return Ok(Ty::Texture(k, Box::new(elem)));
        }
        match name.as_str() {
            "void" => Ok(Ty::Void),
            "SamplerState" | "sampler" | "sampler_state" => Ok(Ty::Sampler(false)),
            "SamplerComparisonState" => Ok(Ty::Sampler(true)),
            "PointStream" | "LineStream" | "TriangleStream" => {
                let kind = match name.as_str() {
                    "PointStream" => 0,
                    "LineStream" => 1,
                    _ => 2,
                };
                self.expect("<")?;
                let t = self.ty()?;
                self.expect(">")?;
                Ok(Ty::Stream(kind, Box::new(t)))
            }
            "vector" => {
                if self.eat("<") {
                    let b = self.ty()?.base().ok_or("vector<> : type de base attendu")?;
                    self.expect(",")?;
                    let n = self.int_const()?;
                    self.expect(">")?;
                    Ok(Ty::vec(b, n as u8))
                } else {
                    Ok(Ty::Vector(Base::Float, 4))
                }
            }
            "matrix" => {
                if self.eat("<") {
                    let b = self.ty()?.base().ok_or("matrix<> : type de base attendu")?;
                    self.expect(",")?;
                    let r = self.int_const()?;
                    self.expect(",")?;
                    let c = self.int_const()?;
                    self.expect(">")?;
                    Ok(matrix(b, r as u8, c as u8))
                } else {
                    Ok(Ty::Matrix(Base::Float, 4, 4))
                }
            }
            "struct" => {
                let i = self.struct_def()?;
                Ok(Ty::Struct(i))
            }
            _ => {
                self.p -= 1;
                self.err(&format!("type inconnu « {name} »"))
            }
        }
    }

    fn int_const(&mut self) -> R<u64> {
        match self.peek() {
            Some(Tok::Int(v, _)) => {
                let v = *v;
                self.p += 1;
                Ok(v)
            }
            _ => self.err("entier attendu"),
        }
    }

    /// `struct Nom { membres };` (le mot struct déjà lu).
    fn struct_def(&mut self) -> R<usize> {
        let name = if let Some(Tok::Ident(_)) = self.peek() {
            self.ident()?
        } else {
            format!("anon{}", self.prog.structs.len())
        };
        if !self.is("{") {
            // Référence à une structure déjà déclarée : « struct S x; ».
            return self
                .prog
                .structs
                .iter()
                .position(|d| d.name == name)
                .ok_or_else(|| format!("({}) : structure {name} inconnue", self.line()));
        }
        self.expect("{")?;
        let mut members = Vec::new();
        while !self.eat("}") {
            let m = self.mods();
            let base = self.ty()?;
            loop {
                let n = self.ident()?;
                let ty = self.array_suffix(base.clone())?;
                let (semantic, _, _) = self.annotations()?;
                members.push(Member {
                    name: n,
                    ty,
                    semantic,
                    interp: Interp { flat: m.flat },
                    row_major: m.row_major.unwrap_or(self.row_major_default),
                });
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.prog.structs.push(StructDef { name, members });
        Ok(self.prog.structs.len() - 1)
    }

    fn array_suffix(&mut self, base: Ty) -> R<Ty> {
        let mut dims = Vec::new();
        while self.eat("[") {
            if self.eat("]") {
                dims.push(0);
                continue;
            }
            let e = self.expr()?;
            let n = self
                .const_int(&e)
                .ok_or_else(|| format!("({}) : taille de tableau non constante", e.line))?;
            dims.push(n as u32);
            self.expect("]")?;
        }
        let mut t = base;
        for d in dims.into_iter().rev() {
            t = Ty::Array(Box::new(t), d);
        }
        Ok(t)
    }

    /// Valeur entière constante (littéraux, constantes statiques, opérations).
    pub(crate) fn const_int(&self, e: &Expr) -> Option<i64> {
        const_eval(e, &self.prog.globals).map(|v| v as i64)
    }

    /// `: SEMANTIQUE`, `: register(t0)`, `: packoffset(c1.y)` (plusieurs possibles).
    fn annotations(&mut self) -> R<(Option<String>, Option<(char, u32)>, Option<u32>)> {
        let (mut sem, mut reg, mut pack) = (None, None, None);
        while self.is(":") && !matches!(self.peek_at(1), Some(Tok::Punct(_))) {
            self.p += 1;
            let w = self.ident()?;
            match w.as_str() {
                "register" => {
                    self.expect("(")?;
                    let mut last = self.ident()?;
                    // register(ps, t0) ou register(t0, space0).
                    while self.eat(",") {
                        let n = self.ident()?;
                        if !n.starts_with("space") {
                            last = n;
                        }
                    }
                    self.expect(")")?;
                    let c = last.chars().next().unwrap_or('t').to_ascii_lowercase();
                    let n: u32 = last[1..].parse().unwrap_or(0);
                    reg = Some((c, n));
                }
                "packoffset" => {
                    self.expect("(")?;
                    let r = self.ident()?;
                    let n: u32 = r[1..].parse().unwrap_or(0);
                    let mut comp = 0;
                    if self.eat(".") {
                        let c = self.ident()?;
                        comp = match c.chars().next() {
                            Some('y') | Some('g') => 1,
                            Some('z') | Some('b') => 2,
                            Some('w') | Some('a') => 3,
                            _ => 0,
                        };
                    }
                    self.expect(")")?;
                    pack = Some(n * 4 + comp);
                }
                _ => sem = Some(w),
            }
        }
        Ok((sem, reg, pack))
    }

    fn top(&mut self) -> R<()> {
        if let Some(Tok::PackMatrix(r)) = self.peek() {
            self.row_major_default = *r;
            self.p += 1;
            return Ok(());
        }
        if self.eat(";") {
            return Ok(());
        }
        self.skip_attributes();
        if self.is_ident("typedef") {
            self.p += 1;
            let _ = self.mods();
            let t = self.ty()?;
            let n = self.ident()?;
            let t = self.array_suffix(t)?;
            self.typedefs.insert(n, t);
            return self.expect(";");
        }
        if self.is_ident("cbuffer") || self.is_ident("tbuffer") {
            self.p += 1;
            let name = self.ident()?;
            let (_, reg, _) = self.annotations()?;
            self.expect("{")?;
            let mut members = Vec::new();
            while !self.eat("}") {
                let m = self.mods();
                let base = self.ty()?;
                // Structure définie dans le tampon : « struct S { … }; ».
                if self.eat(";") {
                    continue;
                }
                self.declarators(base, m, &mut members)?;
                self.expect(";")?;
            }
            self.eat(";");
            self.prog.cbuffers.push(CBuffer {
                name,
                register: reg.map(|r| r.1),
                members,
            });
            return Ok(());
        }
        if self.is_ident("struct") && matches!(self.peek_at(2), Some(Tok::Punct("{"))) {
            self.p += 1;
            let i = self.struct_def()?;
            if self.eat(";") {
                return Ok(());
            }
            let mut v = Vec::new();
            self.declarators(Ty::Struct(i), Mods::default(), &mut v)?;
            self.prog.globals.extend(v);
            return self.expect(";");
        }
        let m = self.mods();
        let ty = self.ty()?;
        let m2 = self.mods();
        let m = Mods {
            row_major: m.row_major.or(m2.row_major),
            ..m
        };
        // Fonction ?
        if matches!(self.peek(), Some(Tok::Ident(_)))
            && matches!(self.peek_at(1), Some(Tok::Punct("(")))
        {
            return self.function(ty);
        }
        let mut v = Vec::new();
        self.declarators(ty, m, &mut v)?;
        self.prog.globals.extend(v);
        self.expect(";")
    }

    fn declarators(&mut self, base: Ty, m: Mods, out: &mut Vec<VarDecl>) -> R<()> {
        loop {
            let line = self.line();
            let name = self.ident()?;
            let mut ty = self.array_suffix(base.clone())?;
            let (semantic, register, packoffset) = self.annotations()?;
            // Annotations d'effet « < ... > » : ignorées.
            if self.is("<") {
                while !self.eat(">") && self.p < self.t.len() {
                    self.p += 1;
                }
            }
            let mut init = None;
            if self.eat("=") {
                let e = self.assign_expr_or_list()?;
                if let (Ty::Array(_, 0), ExprKind::InitList(l)) = (&ty, &e.k) {
                    if let Ty::Array(t, _) = &ty {
                        let n = flat_len(l);
                        let per = scalars_of(t, &self.prog.structs).max(1);
                        let len = if l.iter().all(|x| matches!(x.k, ExprKind::InitList(_))) {
                            l.len()
                        } else {
                            n.div_ceil(per)
                        };
                        ty = Ty::Array(t.clone(), len as u32);
                    }
                }
                init = Some(e);
            }
            out.push(VarDecl {
                name,
                ty,
                init,
                is_static: m.is_static,
                is_const: m.is_const,
                semantic,
                register,
                packoffset,
                interp: Interp { flat: m.flat },
                row_major: m.row_major.unwrap_or(self.row_major_default),
                line,
            });
            if !self.eat(",") {
                break;
            }
        }
        Ok(())
    }

    fn assign_expr_or_list(&mut self) -> R<Expr> {
        if self.is("{") {
            let line = self.line();
            self.p += 1;
            let mut v = Vec::new();
            while !self.eat("}") {
                v.push(self.assign_expr_or_list()?);
                if !self.eat(",") {
                    self.expect("}")?;
                    break;
                }
            }
            return Ok(Expr {
                k: ExprKind::InitList(v),
                line,
            });
        }
        self.assign()
    }

    fn function(&mut self, ret: Ty) -> R<()> {
        let line = self.line();
        let name = self.ident()?;
        self.expect("(")?;
        let mut params = Vec::new();
        if !self.eat(")") {
            if self.is_ident("void") && matches!(self.peek_at(1), Some(Tok::Punct(")"))) {
                self.p += 2;
            } else {
                loop {
                    let mut m = self.mods();
                    // Primitive de geometry shader : « triangle Vert v[3] ».
                    let mut prim = None;
                    if let Some(Tok::Ident(s)) = self.peek() {
                        if matches!(
                            s.as_str(),
                            "point" | "line" | "triangle" | "lineadj" | "triangleadj"
                        ) {
                            prim = Some(s.clone());
                            self.p += 1;
                            let more = self.mods();
                            m.dir_in |= more.dir_in;
                            m.dir_out |= more.dir_out;
                            m.flat |= more.flat;
                        }
                    }
                    let base = self.ty()?;
                    let m2 = self.mods();
                    let pname = self.ident()?;
                    let ty = self.array_suffix(base)?;
                    let (semantic, _, _) = self.annotations()?;
                    let default = if self.eat("=") {
                        Some(self.assign()?)
                    } else {
                        None
                    };
                    let (i, o) = (m.dir_in || m2.dir_in, m.dir_out || m2.dir_out);
                    params.push(Param {
                        prim,
                        name: pname,
                        ty,
                        dir: match (i, o) {
                            (true, true) => Dir::InOut,
                            (false, true) => Dir::Out,
                            _ => Dir::In,
                        },
                        semantic,
                        interp: Interp {
                            flat: m.flat || m2.flat,
                        },
                        default,
                    });
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect(")")?;
            }
        }
        let (ret_semantic, _, _) = self.annotations()?;
        let body = if self.eat(";") {
            None
        } else {
            Some(self.block()?)
        };
        let max_vertex_count = std::mem::take(&mut self.max_vertex_count);
        self.prog.funcs.push(Func {
            name,
            ret,
            ret_semantic,
            params,
            max_vertex_count,
            body,
            line,
        });
        Ok(())
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.eat("}") {
            if self.p >= self.t.len() {
                return self.err("« } » attendu");
            }
            v.push(self.stmt()?);
        }
        Ok(v)
    }

    fn is_decl_start(&self) -> bool {
        if let Some(Tok::Ident(s)) = self.peek() {
            if MODIFIERS.contains(&s.as_str()) || s == "struct" || s == "typedef" {
                return true;
            }
        }
        if !self.starts_type() {
            return false;
        }
        // Type suivi d'un identifiant (ou de « < » pour vector<>/Texture<>).
        matches!(self.peek_at(1), Some(Tok::Ident(_)) | Some(Tok::Punct("<")))
    }

    fn stmt(&mut self) -> R<Stmt> {
        self.skip_attributes();
        let line = self.line();
        if self.is("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if let Some(Tok::Ident(w)) = self.peek() {
            let w = w.clone();
            match w.as_str() {
                "if" => {
                    self.p += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let a = self.stmt()?;
                    let b = if self.is_ident("else") {
                        self.p += 1;
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, Box::new(a), b));
                }
                "for" => {
                    self.p += 1;
                    self.expect("(")?;
                    let init = if self.eat(";") {
                        None
                    } else if self.is_decl_start() {
                        let s = self.decl_stmt()?;
                        Some(Box::new(s))
                    } else {
                        let e = self.expr()?;
                        self.expect(";")?;
                        Some(Box::new(Stmt::Expr(e)))
                    };
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
                    let body = self.stmt()?;
                    return Ok(Stmt::For(init, cond, step, Box::new(body)));
                }
                "while" => {
                    self.p += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let b = self.stmt()?;
                    return Ok(Stmt::While(c, Box::new(b)));
                }
                "do" => {
                    self.p += 1;
                    let b = self.stmt()?;
                    if !self.is_ident("while") {
                        return self.err("« while » attendu");
                    }
                    self.p += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::DoWhile(Box::new(b), c));
                }
                "switch" => {
                    self.p += 1;
                    self.expect("(")?;
                    let e = self.expr()?;
                    self.expect(")")?;
                    let body = self.block()?;
                    return Ok(Stmt::Switch(e, body));
                }
                "case" => {
                    self.p += 1;
                    let e = self.ternary()?;
                    self.expect(":")?;
                    return Ok(Stmt::Case(e));
                }
                "default" => {
                    self.p += 1;
                    self.expect(":")?;
                    return Ok(Stmt::Default);
                }
                "break" => {
                    self.p += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.p += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Continue);
                }
                "discard" => {
                    self.p += 1;
                    self.expect(";")?;
                    return Ok(Stmt::Discard);
                }
                "return" => {
                    self.p += 1;
                    if self.eat(";") {
                        return Ok(Stmt::Return(None, line));
                    }
                    let e = self.expr()?;
                    self.expect(";")?;
                    return Ok(Stmt::Return(Some(e), line));
                }
                _ => {}
            }
        }
        if self.is_decl_start() {
            return self.decl_stmt();
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(Stmt::Expr(e))
    }

    fn decl_stmt(&mut self) -> R<Stmt> {
        if self.is_ident("typedef") {
            self.p += 1;
            let t = self.ty()?;
            let n = self.ident()?;
            let t = self.array_suffix(t)?;
            self.typedefs.insert(n, t);
            self.expect(";")?;
            return Ok(Stmt::Empty);
        }
        let m = self.mods();
        if self.is_ident("struct") {
            self.p += 1;
            let i = self.struct_def()?;
            if self.eat(";") {
                return Ok(Stmt::Empty);
            }
            let mut v = Vec::new();
            self.declarators(Ty::Struct(i), m, &mut v)?;
            self.expect(";")?;
            return Ok(Stmt::Decl(v));
        }
        let ty = self.ty()?;
        let mut v = Vec::new();
        self.declarators(ty, m, &mut v)?;
        self.expect(";")?;
        Ok(Stmt::Decl(v))
    }

    // ---------- Expressions ----------

    pub(crate) fn expr(&mut self) -> R<Expr> {
        let mut e = self.assign()?;
        while self.is(",") {
            let line = self.line();
            self.p += 1;
            let r = self.assign()?;
            e = Expr {
                k: ExprKind::Comma(Box::new(e), Box::new(r)),
                line,
            };
        }
        Ok(e)
    }

    fn assign(&mut self) -> R<Expr> {
        let l = self.ternary()?;
        if let Some(Tok::Punct(op)) = self.peek() {
            let op: &'static str = op;
            if matches!(
                op,
                "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "|=" | "^="
            ) {
                let line = self.line();
                self.p += 1;
                let r = self.assign_expr_or_list()?;
                return Ok(Expr {
                    k: ExprKind::Assign(op, Box::new(l), Box::new(r)),
                    line,
                });
            }
        }
        Ok(l)
    }

    fn ternary(&mut self) -> R<Expr> {
        let c = self.binary(0)?;
        if self.is("?") {
            let line = self.line();
            self.p += 1;
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr {
                k: ExprKind::Ternary(Box::new(c), Box::new(a), Box::new(b)),
                line,
            });
        }
        Ok(c)
    }

    fn binary(&mut self, min: u8) -> R<Expr> {
        let mut l = self.unary()?;
        while let Some(Tok::Punct(op)) = self.peek() {
            let op: &'static str = op;
            let pr = match op {
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
                _ => break,
            };
            if pr <= min {
                break;
            }
            let line = self.line();
            self.p += 1;
            let r = self.binary(pr)?;
            l = Expr {
                k: ExprKind::Binary(op, Box::new(l), Box::new(r)),
                line,
            };
        }
        Ok(l)
    }

    fn unary(&mut self) -> R<Expr> {
        let line = self.line();
        if let Some(Tok::Punct(op)) = self.peek() {
            let op: &'static str = op;
            if matches!(op, "-" | "+" | "!" | "~" | "++" | "--") {
                self.p += 1;
                let e = self.unary()?;
                return Ok(Expr {
                    k: ExprKind::Unary(op, Box::new(e)),
                    line,
                });
            }
            // Conversion « (float3)x ».
            if op == "(" {
                let save = self.p;
                self.p += 1;
                if self.starts_type() {
                    let m_ok = !matches!(self.peek_at(1), Some(Tok::Punct("(")));
                    if m_ok {
                        if let Ok(t) = self.ty() {
                            if let Ok(t) = self.array_suffix(t) {
                                if self.eat(")") {
                                    let e = self.unary()?;
                                    return Ok(Expr {
                                        k: ExprKind::Cast(t, Box::new(e)),
                                        line,
                                    });
                                }
                            }
                        }
                    }
                }
                self.p = save;
            }
        }
        self.postfix()
    }

    fn args(&mut self) -> R<Vec<Expr>> {
        self.expect("(")?;
        let mut v = Vec::new();
        if self.eat(")") {
            return Ok(v);
        }
        loop {
            v.push(self.assign()?);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(v)
    }

    fn postfix(&mut self) -> R<Expr> {
        let mut e = self.primary()?;
        loop {
            let line = self.line();
            if self.eat(".") {
                let n = self.ident()?;
                if self.is("(") {
                    let a = self.args()?;
                    e = Expr {
                        k: ExprKind::Method(Box::new(e), n, a),
                        line,
                    };
                } else {
                    e = Expr {
                        k: ExprKind::Member(Box::new(e), n),
                        line,
                    };
                }
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr {
                    k: ExprKind::Index(Box::new(e), Box::new(i)),
                    line,
                };
            } else if self.is("++") || self.is("--") {
                let op = if self.is("++") { "++" } else { "--" };
                self.p += 1;
                e = Expr {
                    k: ExprKind::Postfix(op, Box::new(e)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> R<Expr> {
        let line = self.line();
        let Some(t) = self.peek().cloned() else {
            return self.err("expression attendue");
        };
        let k = match t {
            Tok::Int(v, u) => {
                self.p += 1;
                ExprKind::Int(v, u)
            }
            Tok::Float(f) => {
                self.p += 1;
                ExprKind::Float(f)
            }
            Tok::Punct("(") => {
                self.p += 1;
                let e = self.expr()?;
                self.expect(")")?;
                return Ok(e);
            }
            Tok::Punct("{") => return self.assign_expr_or_list(),
            Tok::Ident(s) => {
                if s == "true" || s == "false" {
                    self.p += 1;
                    ExprKind::Bool(s == "true")
                } else if self.starts_type() && !self.prog.structs.iter().any(|d| d.name == s) {
                    let ty = self.ty()?;
                    let a = self.args()?;
                    ExprKind::Ctor(ty, a)
                } else {
                    self.p += 1;
                    if self.is("(") {
                        let a = self.args()?;
                        ExprKind::Call(s, a)
                    } else {
                        ExprKind::Ident(s)
                    }
                }
            }
            _ => return self.err("expression attendue"),
        };
        Ok(Expr { k, line })
    }
}

fn flat_len(l: &[Expr]) -> usize {
    l.iter()
        .map(|e| match &e.k {
            ExprKind::InitList(v) => flat_len(v),
            _ => 1,
        })
        .sum()
}

/// Nombre de scalaires d'un type (listes d'initialisation).
pub(crate) fn scalars_of(t: &Ty, structs: &[StructDef]) -> usize {
    match t {
        Ty::Scalar(_) => 1,
        Ty::Vector(_, n) => *n as usize,
        Ty::Matrix(_, r, c) => (*r as usize) * (*c as usize),
        Ty::Array(t, n) => scalars_of(t, structs) * *n as usize,
        Ty::Struct(i) => structs.get(*i).map_or(0, |s| {
            s.members.iter().map(|m| scalars_of(&m.ty, structs)).sum()
        }),
        _ => 0,
    }
}

/// Évaluation constante (tailles de tableau, étiquettes de case…).
pub(crate) fn const_eval(e: &Expr, globals: &[VarDecl]) -> Option<f64> {
    Some(match &e.k {
        ExprKind::Int(v, _) => *v as f64,
        ExprKind::Float(f) => *f,
        ExprKind::Bool(b) => *b as i32 as f64,
        ExprKind::Ident(n) => {
            let g = globals.iter().rev().find(|g| &g.name == n && g.is_const)?;
            const_eval(g.init.as_ref()?, globals)?
        }
        ExprKind::Unary(op, a) => {
            let a = const_eval(a, globals)?;
            match *op {
                "-" => -a,
                "+" => a,
                "!" => (a == 0.0) as i32 as f64,
                "~" => !(a as i64) as f64,
                _ => return None,
            }
        }
        ExprKind::Binary(op, a, b) => {
            let (a, b) = (const_eval(a, globals)?, const_eval(b, globals)?);
            match *op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" if b != 0.0 => {
                    if a.fract() == 0.0 && b.fract() == 0.0 {
                        ((a as i64) / (b as i64)) as f64
                    } else {
                        a / b
                    }
                }
                "%" if b != 0.0 => ((a as i64) % (b as i64)) as f64,
                "<<" => ((a as i64) << (b as i64)) as f64,
                ">>" => ((a as i64) >> (b as i64)) as f64,
                "&" => ((a as i64) & (b as i64)) as f64,
                "|" => ((a as i64) | (b as i64)) as f64,
                "^" => ((a as i64) ^ (b as i64)) as f64,
                "<" => (a < b) as i32 as f64,
                ">" => (a > b) as i32 as f64,
                "<=" => (a <= b) as i32 as f64,
                ">=" => (a >= b) as i32 as f64,
                "==" => (a == b) as i32 as f64,
                "!=" => (a != b) as i32 as f64,
                "&&" => (a != 0.0 && b != 0.0) as i32 as f64,
                "||" => (a != 0.0 || b != 0.0) as i32 as f64,
                _ => return None,
            }
        }
        ExprKind::Ternary(c, a, b) => {
            if const_eval(c, globals)? != 0.0 {
                const_eval(a, globals)?
            } else {
                const_eval(b, globals)?
            }
        }
        ExprKind::Cast(t, _) | ExprKind::Ctor(t, _) if t.comps() == Some(1) => {
            let v = match &e.k {
                ExprKind::Cast(_, a) => const_eval(a, globals)?,
                ExprKind::Ctor(_, args) if args.len() == 1 => const_eval(&args[0], globals)?,
                _ => return None,
            };
            match t.base()? {
                Base::Int | Base::Uint => v.trunc(),
                Base::Bool => (v != 0.0) as i32 as f64,
                Base::Float => v,
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog(src: &str) -> Program {
        let mut inc = |_: &str, _: bool| None;
        let t = super::super::pp::preprocess(src, &[], &mut inc).unwrap();
        parse(t, false).unwrap()
    }

    #[test]
    fn declarations_and_functions() {
        let p = prog(
            "struct VS_IN { float3 pos : POSITION; float2 uv : TEXCOORD0; };\n\
             cbuffer C : register(b1) { float4x4 wvp; float4 tint : packoffset(c4); };\n\
             Texture2D<float4> tex : register(t0); SamplerState smp : register(s0);\n\
             static const int N = 2 * 2; static float w[N] = {1, 2, 3, 4};\n\
             float4 main(VS_IN i, out float4 c : COLOR0) : SV_Position {\n\
               [unroll] for (int k = 0; k < N; k++) { c += (float4)w[k]; }\n\
               return mul(float4(i.pos, 1), wvp) * tex.SampleLevel(smp, i.uv, 0).x;\n\
             }",
        );
        assert_eq!(
            p.structs[0].members[1].semantic.as_deref(),
            Some("TEXCOORD0")
        );
        assert_eq!(p.cbuffers[0].register, Some(1));
        assert_eq!(p.cbuffers[0].members[1].packoffset, Some(16));
        assert_eq!(p.globals[0].register, Some(('t', 0)));
        assert_eq!(
            p.globals[3].ty,
            Ty::Array(Box::new(Ty::Scalar(Base::Float)), 4)
        );
        assert_eq!(p.funcs[0].params[1].dir, Dir::Out);
        assert_eq!(p.funcs[0].ret_semantic.as_deref(), Some("SV_Position"));
    }
}
