//! Arbre commun aux deux langages : JScript et VBScript sont lus vers les
//! mêmes nœuds, et l'interpréteur applique les règles de chaque langage
//! (comparaisons, conversions, `+`) selon `Lang`.

use super::V;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `\` de VBScript.
    IntDiv,
    Mod,
    Pow,
    /// `&` de VBScript.
    Concat,
    Eq,
    Ne,
    /// `===` et `!==` de JScript.
    StrictEq,
    StrictNe,
    Lt,
    Gt,
    Le,
    Ge,
    /// `Is` de VBScript (même objet).
    Is,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    /// `And`, `Or`, `Xor`, `Eqv`, `Imp` de VBScript : logiques sur des
    /// booléens, bit à bit sur des nombres.
    VbAnd,
    VbOr,
    VbXor,
    VbEqv,
    VbImp,
    In,
    InstanceOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UOp {
    Neg,
    Plus,
    /// `!` de JScript.
    Not,
    /// `Not` de VBScript.
    VbNot,
    BitNot,
    TypeOf,
    Void,
    Delete,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Lit(V),
    Ident(String),
    /// `.membre` dans un bloc `With` de VBScript.
    WithRef(String),
    Member(Box<Expr>, String),
    /// `a[b]` de JScript.
    Index(Box<Expr>, Box<Expr>),
    /// Appel, ou en VBScript indice de tableau : `f(a, b)`.
    Call(Box<Expr>, Vec<Expr>),
    New(Box<Expr>, Vec<Expr>),
    Unary(UOp, Box<Expr>),
    Bin(BOp, Box<Expr>, Box<Expr>),
    /// `&&` et `||` de JScript (court-circuit, rendent un des opérandes).
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Assign(Box<Expr>, Option<BOp>, Box<Expr>),
    /// `++`/`--` : (cible, +1 ou -1, préfixe).
    Update(Box<Expr>, f64, bool),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Func(Rc<FuncDef>),
    This,
    /// Expression régulière littérale : (motif, drapeaux).
    Regex(String, String),
    /// Opérande `,` de JScript : la dernière valeur.
    Seq(Vec<Expr>),
}

#[derive(Debug)]
pub struct FuncDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    /// `Sub` de VBScript (pas de valeur de retour).
    pub is_sub: bool,
}

#[derive(Clone, Debug)]
pub enum CaseTest {
    Value(Expr),
    Range(Expr, Expr),
    /// `Case Is > 3`.
    Is(BOp, Expr),
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr),
    /// `var`, `Dim`, `Const` : (nom, valeur initiale, dimensions).
    Var(Vec<(String, Option<Expr>, Option<Vec<Expr>>)>),
    ReDim(bool, String, Vec<Expr>),
    /// Affectation de VBScript (`Set` ou non).
    Assign(Expr, Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    /// Boucle générale : condition avant ou après, `Until` inverse le test.
    Loop {
        cond: Option<Expr>,
        post: bool,
        until: bool,
        body: Vec<Stmt>,
    },
    /// `for (init; cond; step)` de JScript.
    For {
        init: Vec<Stmt>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    /// `For i = a To b Step s` de VBScript.
    ForNum {
        var: String,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    /// `For Each x In c` (valeurs) ou `for (x in o)` (clés).
    ForEach {
        var: Expr,
        coll: Expr,
        keys: bool,
        body: Vec<Stmt>,
    },
    /// `switch` de JScript (avec passage au cas suivant).
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    /// `Select Case` de VBScript.
    Select(Expr, Vec<(Vec<CaseTest>, Vec<Stmt>)>, Option<Vec<Stmt>>),
    Return(Option<Expr>),
    Break,
    Continue,
    Throw(Expr),
    Try {
        body: Vec<Stmt>,
        catch: Option<(String, Vec<Stmt>)>,
        finally: Option<Vec<Stmt>>,
    },
    Func(std::rc::Rc<FuncDef>),
    /// `On Error Resume Next` (vrai) ou `On Error GoTo 0` (faux).
    OnError(bool),
    With(Expr, Vec<Stmt>),
    Block(Vec<Stmt>),
    Empty,
}
