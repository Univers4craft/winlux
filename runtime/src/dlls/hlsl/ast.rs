//! Arbre syntaxique et types HLSL.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Base {
    Bool,
    Int,
    Uint,
    Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TexKind {
    Buffer,
    T1D,
    T1DArray,
    T2D,
    T2DArray,
    T2DMS,
    T2DMSArray,
    T3D,
    Cube,
    CubeArray,
}

impl TexKind {
    /// Nombre de coordonnées d'échantillonnage (avec l'indice de tranche).
    pub(crate) fn coords(self) -> u8 {
        match self {
            TexKind::Buffer | TexKind::T1D => 1,
            TexKind::T1DArray | TexKind::T2D | TexKind::T2DMS => 2,
            TexKind::T2DArray | TexKind::T2DMSArray | TexKind::T3D | TexKind::Cube => 3,
            TexKind::CubeArray => 4,
        }
    }
    /// Coordonnées entières de `Load` (sans le niveau).
    pub(crate) fn load_coords(self) -> u8 {
        match self {
            TexKind::Cube | TexKind::CubeArray => 3,
            k => k.coords(),
        }
    }
    pub(crate) fn glsl(self) -> &'static str {
        match self {
            TexKind::Buffer => "Buffer",
            TexKind::T1D => "1D",
            TexKind::T1DArray => "1DArray",
            TexKind::T2D => "2D",
            TexKind::T2DArray => "2DArray",
            TexKind::T2DMS => "2DMS",
            TexKind::T2DMSArray => "2DMSArray",
            TexKind::T3D => "3D",
            TexKind::Cube => "Cube",
            TexKind::CubeArray => "CubeArray",
        }
    }
    pub(crate) fn has_mips(self) -> bool {
        !matches!(self, TexKind::Buffer | TexKind::T2DMS | TexKind::T2DMSArray)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ty {
    Void,
    Scalar(Base),
    Vector(Base, u8),
    /// Lignes, colonnes.
    Matrix(Base, u8, u8),
    Array(Box<Ty>, u32),
    Struct(usize),
    /// Texture et type d'élément (scalaire ou vecteur).
    Texture(TexKind, Box<Ty>),
    /// Vrai pour SamplerComparisonState.
    Sampler(bool),
}

impl Ty {
    pub(crate) fn base(&self) -> Option<Base> {
        match self {
            Ty::Scalar(b) | Ty::Vector(b, _) | Ty::Matrix(b, _, _) => Some(*b),
            _ => None,
        }
    }
    /// Composantes d'un scalaire ou d'un vecteur.
    pub(crate) fn comps(&self) -> Option<u8> {
        match self {
            Ty::Scalar(_) => Some(1),
            Ty::Vector(_, n) => Some(*n),
            _ => None,
        }
    }
    pub(crate) fn with_base(&self, b: Base) -> Ty {
        match self {
            Ty::Scalar(_) => Ty::Scalar(b),
            Ty::Vector(_, n) => Ty::Vector(b, *n),
            Ty::Matrix(_, r, c) => Ty::Matrix(b, *r, *c),
            t => t.clone(),
        }
    }
    pub(crate) fn vec(b: Base, n: u8) -> Ty {
        if n == 1 {
            Ty::Scalar(b)
        } else {
            Ty::Vector(b, n)
        }
    }
    pub(crate) fn is_resource(&self) -> bool {
        match self {
            Ty::Texture(..) | Ty::Sampler(_) => true,
            Ty::Array(t, _) => t.is_resource(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub k: ExprKind,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Int(u64, bool),
    Float(f64),
    Bool(bool),
    Ident(String),
    Unary(&'static str, Box<Expr>),
    Postfix(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Ctor(Ty, Vec<Expr>),
    Cast(Ty, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Method(Box<Expr>, String, Vec<Expr>),
    InitList(Vec<Expr>),
    Comma(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Interp {
    pub flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VarDecl {
    pub name: String,
    pub ty: Ty,
    pub init: Option<Expr>,
    pub is_static: bool,
    pub is_const: bool,
    #[allow(dead_code)] // sémantique d'une globale : sans effet en SM4+
    pub semantic: Option<String>,
    /// ('b'|'t'|'s'|'u'|'c', numéro).
    pub register: Option<(char, u32)>,
    /// packoffset(cN.x) en composantes.
    pub packoffset: Option<u32>,
    #[allow(dead_code)] // interpolation d'une globale : sans effet
    pub interp: Interp,
    pub row_major: bool,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug)]
pub(crate) struct Param {
    pub name: String,
    pub ty: Ty,
    pub dir: Dir,
    pub semantic: Option<String>,
    pub interp: Interp,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Func {
    pub name: String,
    pub ret: Ty,
    pub ret_semantic: Option<String>,
    pub params: Vec<Param>,
    pub body: Option<Vec<Stmt>>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub name: String,
    pub ty: Ty,
    pub semantic: Option<String>,
    pub interp: Interp,
    pub row_major: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct StructDef {
    pub name: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub(crate) enum Stmt {
    Expr(Expr),
    Decl(Vec<VarDecl>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Switch(Expr, Vec<Stmt>),
    Case(Expr),
    Default,
    Break,
    Continue,
    Return(Option<Expr>, u32),
    Discard,
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Clone, Debug)]
pub(crate) struct CBuffer {
    pub name: String,
    pub register: Option<u32>,
    pub members: Vec<VarDecl>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub structs: Vec<StructDef>,
    pub globals: Vec<VarDecl>,
    pub cbuffers: Vec<CBuffer>,
    pub funcs: Vec<Func>,
}
