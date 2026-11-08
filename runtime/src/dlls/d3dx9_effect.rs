//! D3DX9 : effets (#834).
//!
//! ID3DXEffect sur les fichiers .fx en texte, d'après la documentation
//! publique de l'interface et du format d'effet Direct3D 9 : paramètres
//! (valeurs, annotations, éléments de tableau), techniques et passes,
//! états de rendu, d'échantillonneur et d'étage de texture, shaders
//! compilés par `d3dx9_hlsl` (`compile vs_2_0 f()`) ou assemblés (`asm { }`).
//! Les effets déjà compilés (fx_2_0 binaire, .fxo) sont lus par `binaire` (#909).
#![allow(clippy::too_many_arguments)]

mod binaire;

use super::d3dx9_hlsl::{self as hlsl, Base, Expr, Program, StateAssign, Ty, VarDecl};
use super::legacy_com::{get_ptr, ptr_size, put_ptr, rd32, vtable_sig, wr32};
use crate::com_call;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const D3DERR_INVALIDCALL: i32 = 0x8876_086Cu32 as i32;
const D3DXERR_INVALIDDATA: i32 = 0x8876_0B59u32 as i32;

const DEV_ADDREF: usize = 1;
const DEV_RELEASE: usize = 2;
const DEV_SET_TRANSFORM: usize = 44;
const DEV_SET_RS: usize = 57;
const DEV_GET_RS: usize = 58;
const DEV_GET_TEXTURE: usize = 64;
const DEV_SET_TEXTURE: usize = 65;
const DEV_GET_TSS: usize = 66;
const DEV_SET_TSS: usize = 67;
const DEV_GET_SS: usize = 68;
const DEV_SET_SS: usize = 69;
const DEV_CREATE_VS: usize = 91;
const DEV_SET_VS: usize = 92;
const DEV_GET_VS: usize = 93;
const DEV_SET_VS_F: usize = 94;
const DEV_CREATE_PS: usize = 106;
const DEV_SET_PS: usize = 107;
const DEV_GET_PS: usize = 108;
const DEV_SET_PS_F: usize = 109;

/// D3DXPARAMETER_TYPE
const PT_BOOL: u32 = 1;
const PT_INT: u32 = 2;
const PT_FLOAT: u32 = 3;
const PT_STRING: u32 = 4;
const PT_TEXTURE: u32 = 5;
const PT_SAMPLER: u32 = 10;
const PT_PIXELSHADER: u32 = 15;
const PT_VERTEXSHADER: u32 = 16;

// ═══════════════════════════════════════════════════════════════════════
// Tables d'états (valeurs publiques des énumérations Direct3D 9)
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq)]
enum Cat {
    Num,
    Float,
    Bool,
    Blend,
    BlendOp,
    Cmp,
    Cull,
    Fill,
    Shade,
    StencilOp,
    Fog,
    ZB,
    Mcs,
    Filter,
    Address,
    Top,
    Ta,
    Cw,
    Ttf,
}

const RENDER_STATES: &[(&str, u32, Cat)] = &[
    ("zenable", 7, Cat::ZB),
    ("fillmode", 8, Cat::Fill),
    ("shademode", 9, Cat::Shade),
    ("zwriteenable", 14, Cat::Bool),
    ("alphatestenable", 15, Cat::Bool),
    ("lastpixel", 16, Cat::Bool),
    ("srcblend", 19, Cat::Blend),
    ("destblend", 20, Cat::Blend),
    ("cullmode", 22, Cat::Cull),
    ("zfunc", 23, Cat::Cmp),
    ("alpharef", 24, Cat::Num),
    ("alphafunc", 25, Cat::Cmp),
    ("ditherenable", 26, Cat::Bool),
    ("alphablendenable", 27, Cat::Bool),
    ("fogenable", 28, Cat::Bool),
    ("specularenable", 29, Cat::Bool),
    ("fogcolor", 34, Cat::Num),
    ("fogtablemode", 35, Cat::Fog),
    ("fogstart", 36, Cat::Float),
    ("fogend", 37, Cat::Float),
    ("fogdensity", 38, Cat::Float),
    ("rangefogenable", 48, Cat::Bool),
    ("stencilenable", 52, Cat::Bool),
    ("stencilfail", 53, Cat::StencilOp),
    ("stencilzfail", 54, Cat::StencilOp),
    ("stencilpass", 55, Cat::StencilOp),
    ("stencilfunc", 56, Cat::Cmp),
    ("stencilref", 57, Cat::Num),
    ("stencilmask", 58, Cat::Num),
    ("stencilwritemask", 59, Cat::Num),
    ("texturefactor", 60, Cat::Num),
    ("wrap0", 128, Cat::Num),
    ("wrap1", 129, Cat::Num),
    ("wrap2", 130, Cat::Num),
    ("wrap3", 131, Cat::Num),
    ("wrap4", 132, Cat::Num),
    ("wrap5", 133, Cat::Num),
    ("wrap6", 134, Cat::Num),
    ("wrap7", 135, Cat::Num),
    ("wrap8", 198, Cat::Num),
    ("wrap9", 199, Cat::Num),
    ("wrap10", 200, Cat::Num),
    ("wrap11", 201, Cat::Num),
    ("wrap12", 202, Cat::Num),
    ("wrap13", 203, Cat::Num),
    ("wrap14", 204, Cat::Num),
    ("wrap15", 205, Cat::Num),
    ("clipping", 136, Cat::Bool),
    ("lighting", 137, Cat::Bool),
    ("ambient", 139, Cat::Num),
    ("fogvertexmode", 140, Cat::Fog),
    ("colorvertex", 141, Cat::Bool),
    ("localviewer", 142, Cat::Bool),
    ("normalizenormals", 143, Cat::Bool),
    ("diffusematerialsource", 145, Cat::Mcs),
    ("specularmaterialsource", 146, Cat::Mcs),
    ("ambientmaterialsource", 147, Cat::Mcs),
    ("emissivematerialsource", 148, Cat::Mcs),
    ("vertexblend", 151, Cat::Num),
    ("clipplaneenable", 152, Cat::Num),
    ("pointsize", 154, Cat::Float),
    ("pointsize_min", 155, Cat::Float),
    ("pointspriteenable", 156, Cat::Bool),
    ("pointscaleenable", 157, Cat::Bool),
    ("pointscale_a", 158, Cat::Float),
    ("pointscale_b", 159, Cat::Float),
    ("pointscale_c", 160, Cat::Float),
    ("multisampleantialias", 161, Cat::Bool),
    ("multisamplemask", 162, Cat::Num),
    ("patchedgestyle", 163, Cat::Num),
    ("debugmonitortoken", 165, Cat::Num),
    ("pointsize_max", 166, Cat::Float),
    ("indexedvertexblendenable", 167, Cat::Bool),
    ("colorwriteenable", 168, Cat::Cw),
    ("tweenfactor", 170, Cat::Float),
    ("blendop", 171, Cat::BlendOp),
    ("positiondegree", 172, Cat::Num),
    ("normaldegree", 173, Cat::Num),
    ("scissortestenable", 174, Cat::Bool),
    ("slopescaledepthbias", 175, Cat::Float),
    ("antialiasedlineenable", 176, Cat::Bool),
    ("mintessellationlevel", 178, Cat::Float),
    ("maxtessellationlevel", 179, Cat::Float),
    ("adaptivetess_x", 180, Cat::Float),
    ("adaptivetess_y", 181, Cat::Float),
    ("adaptivetess_z", 182, Cat::Float),
    ("adaptivetess_w", 183, Cat::Float),
    ("enableadaptivetessellation", 184, Cat::Bool),
    ("twosidedstencilmode", 185, Cat::Bool),
    ("ccw_stencilfail", 186, Cat::StencilOp),
    ("ccw_stencilzfail", 187, Cat::StencilOp),
    ("ccw_stencilpass", 188, Cat::StencilOp),
    ("ccw_stencilfunc", 189, Cat::Cmp),
    ("colorwriteenable1", 190, Cat::Cw),
    ("colorwriteenable2", 191, Cat::Cw),
    ("colorwriteenable3", 192, Cat::Cw),
    ("blendfactor", 193, Cat::Num),
    ("srgbwriteenable", 194, Cat::Bool),
    ("depthbias", 195, Cat::Float),
    ("separatealphablendenable", 206, Cat::Bool),
    ("srcblendalpha", 207, Cat::Blend),
    ("destblendalpha", 208, Cat::Blend),
    ("blendopalpha", 209, Cat::BlendOp),
];

const SAMPLER_STATES: &[(&str, u32, Cat)] = &[
    ("addressu", 1, Cat::Address),
    ("addressv", 2, Cat::Address),
    ("addressw", 3, Cat::Address),
    ("bordercolor", 4, Cat::Num),
    ("magfilter", 5, Cat::Filter),
    ("minfilter", 6, Cat::Filter),
    ("mipfilter", 7, Cat::Filter),
    ("mipmaplodbias", 8, Cat::Float),
    ("maxmiplevel", 9, Cat::Num),
    ("maxanisotropy", 10, Cat::Num),
    ("srgbtexture", 11, Cat::Bool),
];

const STAGE_STATES: &[(&str, u32, Cat)] = &[
    ("colorop", 1, Cat::Top),
    ("colorarg1", 2, Cat::Ta),
    ("colorarg2", 3, Cat::Ta),
    ("alphaop", 4, Cat::Top),
    ("alphaarg1", 5, Cat::Ta),
    ("alphaarg2", 6, Cat::Ta),
    ("bumpenvmat00", 7, Cat::Float),
    ("bumpenvmat01", 8, Cat::Float),
    ("bumpenvmat10", 9, Cat::Float),
    ("bumpenvmat11", 10, Cat::Float),
    ("texcoordindex", 11, Cat::Num),
    ("bumpenvlscale", 22, Cat::Float),
    ("bumpenvloffset", 23, Cat::Float),
    ("texturetransformflags", 24, Cat::Ttf),
    ("colorarg0", 26, Cat::Ta),
    ("alphaarg0", 27, Cat::Ta),
    ("resultarg", 28, Cat::Ta),
    ("constant", 32, Cat::Num),
];

fn enum_value(cat: Cat, name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    let n = n.as_str();
    let t: &[(&str, u32)] = match cat {
        Cat::Bool | Cat::ZB => &[("true", 1), ("false", 0), ("usew", 2)],
        Cat::Blend => &[
            ("zero", 1),
            ("one", 2),
            ("srccolor", 3),
            ("invsrccolor", 4),
            ("srcalpha", 5),
            ("invsrcalpha", 6),
            ("destalpha", 7),
            ("invdestalpha", 8),
            ("destcolor", 9),
            ("invdestcolor", 10),
            ("srcalphasat", 11),
            ("bothsrcalpha", 12),
            ("bothinvsrcalpha", 13),
            ("blendfactor", 14),
            ("invblendfactor", 15),
        ],
        Cat::BlendOp => &[
            ("add", 1),
            ("subtract", 2),
            ("revsubtract", 3),
            ("min", 4),
            ("max", 5),
        ],
        Cat::Cmp => &[
            ("never", 1),
            ("less", 2),
            ("equal", 3),
            ("lessequal", 4),
            ("greater", 5),
            ("notequal", 6),
            ("greaterequal", 7),
            ("always", 8),
        ],
        Cat::Cull => &[("none", 1), ("cw", 2), ("ccw", 3)],
        Cat::Fill => &[("point", 1), ("wireframe", 2), ("solid", 3)],
        Cat::Shade => &[("flat", 1), ("gouraud", 2), ("phong", 3)],
        Cat::StencilOp => &[
            ("keep", 1),
            ("zero", 2),
            ("replace", 3),
            ("incrsat", 4),
            ("decrsat", 5),
            ("invert", 6),
            ("incr", 7),
            ("decr", 8),
        ],
        Cat::Fog => &[("none", 0), ("exp", 1), ("exp2", 2), ("linear", 3)],
        Cat::Mcs => &[("material", 0), ("color1", 1), ("color2", 2)],
        Cat::Filter => &[
            ("none", 0),
            ("point", 1),
            ("linear", 2),
            ("anisotropic", 3),
            ("pyramidalquad", 6),
            ("gaussianquad", 7),
        ],
        Cat::Address => &[
            ("wrap", 1),
            ("mirror", 2),
            ("clamp", 3),
            ("border", 4),
            ("mirroronce", 5),
        ],
        Cat::Top => &[
            ("disable", 1),
            ("selectarg1", 2),
            ("selectarg2", 3),
            ("modulate", 4),
            ("modulate2x", 5),
            ("modulate4x", 6),
            ("add", 7),
            ("addsigned", 8),
            ("addsigned2x", 9),
            ("subtract", 10),
            ("addsmooth", 11),
            ("blenddiffusealpha", 12),
            ("blendtexturealpha", 13),
            ("blendfactoralpha", 14),
            ("blendtexturealphapm", 15),
            ("blendcurrentalpha", 16),
            ("premodulate", 17),
            ("modulatealpha_addcolor", 18),
            ("modulatecolor_addalpha", 19),
            ("modulateinvalpha_addcolor", 20),
            ("modulateinvcolor_addalpha", 21),
            ("bumpenvmap", 22),
            ("bumpenvmapluminance", 23),
            ("dotproduct3", 24),
            ("multiplyadd", 25),
            ("lerp", 26),
        ],
        Cat::Ta => &[
            ("diffuse", 0),
            ("current", 1),
            ("texture", 2),
            ("tfactor", 3),
            ("specular", 4),
            ("temp", 5),
            ("constant", 6),
            ("complement", 0x10),
            ("alphareplicate", 0x20),
        ],
        Cat::Cw => &[("red", 1), ("green", 2), ("blue", 4), ("alpha", 8)],
        Cat::Ttf => &[
            ("disable", 0),
            ("count1", 1),
            ("count2", 2),
            ("count3", 3),
            ("count4", 4),
            ("projected", 256),
        ],
        Cat::Num | Cat::Float => &[("true", 1), ("false", 0)],
    };
    t.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
}

// ═══════════════════════════════════════════════════════════════════════
// Effet
// ═══════════════════════════════════════════════════════════════════════

struct Param {
    name: Vec<u8>,
    semantic: Vec<u8>,
    class: u32,
    ptype: u32,
    rows: u32,
    cols: u32,
    elements: u32,
    flags: u32,
    /// Élément d'un tableau : (paramètre racine, indice).
    elem_of: Option<(usize, usize)>,
    elems: Vec<usize>,
    annotations: Vec<usize>,
    data: Vec<f32>,
    obj: usize,
    string: Vec<u8>,
    sampler: Vec<StateAssign>,
    shader: Option<Shader>,
}

impl Param {
    fn name_str(&self) -> &str {
        std::str::from_utf8(&self.name[..self.name.len().saturating_sub(1)]).unwrap_or("")
    }
    fn per_elem(&self) -> usize {
        (self.rows * self.cols) as usize
    }
}

#[derive(Clone)]
struct Shader {
    tokens: Vec<u32>,
    ctab: Vec<hlsl::CtabEntry>,
    obj: usize,
}

struct PassC {
    name: Vec<u8>,
    annotations: Vec<usize>,
    states: Vec<StateAssign>,
    vs: Option<Option<Shader>>,
    ps: Option<Option<Shader>>,
}

struct Tech {
    name: Vec<u8>,
    annotations: Vec<usize>,
    passes: Vec<PassC>,
    valid: bool,
}

enum Saved {
    Rs(u32, u32),
    Ss(u32, u32, u32),
    Tss(u32, u32, u32),
    Tex(u32, usize),
    Vs(usize),
    Ps(usize),
}

struct State {
    dev: usize,
    params: Vec<Param>,
    top: Vec<usize>,
    techs: Vec<Tech>,
    handles: Vec<u8>,
    cur_tech: usize,
    active_pass: Option<usize>,
    saved: Vec<Saved>,
    blocks: Vec<Option<Vec<(usize, Vec<f32>)>>>,
    recording: Option<HashSet<usize>>,
    source: Vec<u8>,
    defines: Vec<(String, String)>,
    dir: Option<std::path::PathBuf>,
    flags: u32,
    creator: Vec<u8>,
    state_manager: usize,
}

#[repr(C)]
struct Effect {
    vtbl: usize,
    refs: AtomicU32,
    st: State,
}

#[repr(C)]
struct Pool {
    vtbl: usize,
    refs: AtomicU32,
}

enum H {
    Param(usize),
    Tech(usize),
    Pass(usize, usize),
}

fn cstring(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

/// Évaluation constante (valeurs initiales, annotations, états).
fn eval_const(e: &Expr, params: &[Param], top: &[usize]) -> Option<Vec<f32>> {
    Some(match e {
        Expr::Num(v, _) => vec![*v as f32],
        Expr::Bool(b) => vec![*b as i32 as f32],
        Expr::Unary("-", x) => eval_const(x, params, top)?.iter().map(|v| -v).collect(),
        Expr::Unary("+", x) => eval_const(x, params, top)?,
        Expr::Unary("!", x) => eval_const(x, params, top)?
            .iter()
            .map(|v| (*v == 0.0) as i32 as f32)
            .collect(),
        Expr::List(items) | Expr::Ctor(_, items) => {
            let mut v = Vec::new();
            for i in items {
                v.extend(eval_const(i, params, top)?);
            }
            v
        }
        Expr::Cast(_, x) => eval_const(x, params, top)?,
        Expr::Ident(n) => {
            let p = top
                .iter()
                .map(|&i| &params[i])
                .find(|p| p.name_str() == n)?;
            if p.data.is_empty() {
                return None;
            }
            p.data.clone()
        }
        Expr::Bin(op, a, b) => {
            let (x, y) = (eval_const(a, params, top)?, eval_const(b, params, top)?);
            let n = x.len().max(y.len());
            let g = |v: &Vec<f32>, i: usize| v.get(i).or(v.last()).copied().unwrap_or(0.0);
            (0..n)
                .map(|i| {
                    let (p, q) = (g(&x, i), g(&y, i));
                    match *op {
                        "+" => p + q,
                        "-" => p - q,
                        "*" => p * q,
                        "/" => {
                            if q == 0.0 {
                                0.0
                            } else {
                                p / q
                            }
                        }
                        "|" => ((p as i64) | (q as i64)) as f32,
                        "&" => ((p as i64) & (q as i64)) as f32,
                        "<" => (p < q) as i32 as f32,
                        ">" => (p > q) as i32 as f32,
                        "==" => (p == q) as i32 as f32,
                        _ => 0.0,
                    }
                })
                .collect()
        }
        _ => return None,
    })
}

fn param_type(t: &Ty) -> (u32, u32) {
    let ptype = match &t.base {
        Base::Bool => PT_BOOL,
        Base::Int => PT_INT,
        Base::Float => PT_FLOAT,
        Base::Str => PT_STRING,
        Base::Texture => PT_TEXTURE,
        Base::Sampler(0) => PT_SAMPLER,
        Base::Sampler(d) => PT_SAMPLER + (*d as u32).min(4),
        Base::Shader(true) => PT_PIXELSHADER,
        Base::Shader(false) => PT_VERTEXSHADER,
        Base::Void | Base::Struct(_) => 0,
    };
    let class = match (&t.base, t.class) {
        (Base::Struct(_), _) => 5,
        (_, 0) => 0,
        (_, 1) => 1,
        (_, 2) if t.row_major == Some(true) => 2,
        (_, 2) => 3,
        _ => 4,
    };
    (class, ptype)
}

fn new_param(d: &VarDecl, flags: u32) -> Param {
    let (class, ptype) = param_type(&d.ty);
    let numeric = matches!(ptype, PT_BOOL | PT_INT | PT_FLOAT);
    let (rows, cols) = if numeric {
        (d.ty.rows as u32, d.ty.cols as u32)
    } else {
        (0, 0)
    };
    Param {
        name: cstring(&d.name),
        semantic: d.semantic.as_deref().map(cstring).unwrap_or_default(),
        class,
        ptype,
        rows,
        cols,
        elements: d.ty.array.unwrap_or(0),
        flags,
        elem_of: None,
        elems: Vec::new(),
        annotations: Vec::new(),
        data: vec![
            0.0;
            if numeric {
                (rows * cols) as usize * d.ty.array.unwrap_or(1).max(1) as usize
            } else {
                0
            }
        ],
        obj: 0,
        string: Vec::new(),
        sampler: Vec::new(),
        shader: None,
    }
}

fn add_annotations(params: &mut Vec<Param>, top: &[usize], list: &[VarDecl]) -> Vec<usize> {
    let mut out = Vec::new();
    for a in list {
        let mut p = new_param(a, 4);
        match &a.init {
            Some(Expr::Str(s)) => p.string = cstring(s),
            Some(e) => {
                if let Some(v) = eval_const(e, params, top) {
                    for (k, x) in p.data.iter_mut().enumerate() {
                        *x = v.get(k).copied().unwrap_or(0.0);
                    }
                }
            }
            None => {}
        }
        params.push(p);
        out.push(params.len() - 1);
    }
    out
}

impl State {
    fn handle_of(&self, h: H) -> usize {
        let base = self.handles.as_ptr() as usize;
        let np = self.params.len();
        let nt = self.techs.len();
        base + match h {
            H::Param(i) => i,
            H::Tech(t) => np + t,
            H::Pass(t, p) => {
                np + nt
                    + self.techs[..t]
                        .iter()
                        .map(|x| x.passes.len())
                        .sum::<usize>()
                    + p
            }
        }
    }

    unsafe fn resolve(&self, h: usize) -> Option<H> {
        if h == 0 {
            return None;
        }
        let base = self.handles.as_ptr() as usize;
        if h >= base && h < base + self.handles.len() {
            let mut k = h - base;
            if k < self.params.len() {
                return Some(H::Param(k));
            }
            k -= self.params.len();
            if k < self.techs.len() {
                return Some(H::Tech(k));
            }
            k -= self.techs.len();
            for (t, tech) in self.techs.iter().enumerate() {
                if k < tech.passes.len() {
                    return Some(H::Pass(t, k));
                }
                k -= tech.passes.len();
            }
            return None;
        }
        // Sinon c'est un nom (D3DXHANDLE peut être une chaîne).
        let name = super::cstr(h as *const u8);
        if let Some(i) = self.param_by_name(&name) {
            return Some(H::Param(i));
        }
        self.techs
            .iter()
            .position(|t| t.name[..t.name.len() - 1] == *name.as_bytes())
            .map(H::Tech)
    }

    fn param_by_name(&self, name: &str) -> Option<usize> {
        if let Some(open) = name.find('[') {
            let base = self.param_by_name(&name[..open])?;
            let idx: usize = name[open + 1..].trim_end_matches(']').parse().ok()?;
            return self.params[base].elems.get(idx).copied();
        }
        self.top
            .iter()
            .copied()
            .find(|&i| self.params[i].name_str() == name)
    }

    unsafe fn param(&self, h: usize) -> Option<usize> {
        match self.resolve(h)? {
            H::Param(i) => Some(i),
            _ => None,
        }
    }

    /// Données d'un paramètre : (racine, début, longueur) en flottants.
    fn slot(&self, i: usize) -> (usize, usize, usize) {
        match self.params[i].elem_of {
            Some((root, e)) => {
                let per = self.params[root].per_elem();
                (root, e * per, per)
            }
            None => (i, 0, self.params[i].data.len()),
        }
    }

    fn values(&self, i: usize) -> Vec<f32> {
        let (r, s, n) = self.slot(i);
        self.params[r].data[s..s + n].to_vec()
    }

    fn set_values(&mut self, i: usize, v: &[f32]) {
        let (r, s, n) = self.slot(i);
        let ptype = self.params[r].ptype;
        for (k, x) in self.params[r].data[s..s + n].iter_mut().enumerate() {
            if let Some(&y) = v.get(k) {
                *x = match ptype {
                    PT_BOOL => (y != 0.0) as i32 as f32,
                    PT_INT => y.trunc(),
                    _ => y,
                };
            }
        }
        if let Some(rec) = &mut self.recording {
            rec.insert(r);
        }
    }

    fn annotations_of(&self, h: &H) -> &[usize] {
        match h {
            H::Param(i) => &self.params[*i].annotations,
            H::Tech(t) => &self.techs[*t].annotations,
            H::Pass(t, p) => &self.techs[*t].passes[*p].annotations,
        }
    }
}

unsafe fn eff<'a>(this: usize) -> &'a mut State {
    &mut (*(this as *mut Effect)).st
}

// ═══════════════════════════════════════════════════════════════════════
// Chargement
// ═══════════════════════════════════════════════════════════════════════

fn compile_shader_expr(
    prog: &Program,
    e: &Expr,
    pixel: bool,
    flags: u32,
) -> Result<Option<Shader>, String> {
    match e {
        Expr::Null => Ok(None),
        Expr::Num(v, _) if *v == 0.0 => Ok(None),
        Expr::Compile(profile, func, args) => {
            if profile.starts_with("ps") != pixel {
                return Err(format!("{func} : profil {profile} inattendu"));
            }
            let c = hlsl::compile_function(prog, func, profile, flags, args)?;
            Ok(Some(Shader {
                tokens: c.tokens,
                ctab: c.ctab,
                obj: 0,
            }))
        }
        Expr::Asm(text) => {
            let tokens = super::d3dx9_asm::assemble(text, &[])?;
            Ok(Some(Shader {
                tokens,
                ctab: Vec::new(),
                obj: 0,
            }))
        }
        Expr::Ident(n) => {
            let g = prog
                .globals
                .iter()
                .find(|g| g.name == *n)
                .ok_or_else(|| format!("shader inconnu : {n}"))?;
            match &g.init {
                Some(e) => compile_shader_expr(prog, e, pixel, flags),
                None => Ok(None),
            }
        }
        _ => Err("valeur de shader invalide".into()),
    }
}

unsafe fn create_device_shader(dev: usize, s: &mut Shader, pixel: bool) {
    let mut obj: usize = 0;
    let idx = if pixel { DEV_CREATE_PS } else { DEV_CREATE_VS };
    let hr = com_call!(
        dev,
        idx,
        fn(*const u32, *mut usize) -> i32,
        s.tokens.as_ptr(),
        &mut obj
    );
    if hr < 0 {
        crate::log_warn!("d3dx9", "effet : création du shader refusée ({hr:#x})");
    }
    s.obj = obj;
}

pub(crate) unsafe fn load(
    dev: usize,
    src: &[u8],
    defines: Vec<(String, String)>,
    dir: Option<std::path::PathBuf>,
    flags: u32,
) -> Result<usize, String> {
    if src.len() >= 4 && matches!(rd32(src.as_ptr() as usize), binaire::MAGIC | 0xBCF0_0BCF) {
        // Effet déjà compilé (fx_2_0 binaire, .fxo) (#909).
        let (params, top, mut techs) = binaire::read(src)?;
        for t in techs.iter_mut() {
            for p in t.passes.iter_mut() {
                if let Some(Some(x)) = p.vs.as_mut() {
                    create_device_shader(dev, x, false);
                }
                if let Some(Some(x)) = p.ps.as_mut() {
                    create_device_shader(dev, x, true);
                }
            }
        }
        return Ok(finish(dev, params, top, techs, src, defines, dir, flags));
    }
    let text = String::from_utf8_lossy(src);
    let text = text.trim_end_matches('\0').to_string();
    let inc = super::d3dx9_shader::include_resolver(dir.clone());
    let prog = hlsl::parse(&text, &defines, &inc)?;
    let mut params: Vec<Param> = Vec::new();
    let mut top = Vec::new();
    for g in &prog.globals {
        if g.mods.stat {
            continue;
        }
        let mut p = new_param(g, 0);
        match &g.init {
            Some(Expr::Str(s)) => p.string = cstring(s),
            Some(Expr::SamplerState(st)) => p.sampler = st.clone(),
            Some(e @ (Expr::Compile(..) | Expr::Asm(_))) => {
                let pixel = p.ptype == PT_PIXELSHADER;
                match compile_shader_expr(&prog, e, pixel, flags) {
                    Ok(s) => p.shader = s,
                    Err(m) => crate::log_warn!("d3dx9", "effet : {} : {m}", g.name),
                }
            }
            Some(e) => {
                if let Some(v) = eval_const(e, &params, &top) {
                    let n = p.data.len();
                    for (k, x) in p.data.iter_mut().enumerate().take(n) {
                        *x = v.get(k).copied().unwrap_or(0.0);
                    }
                }
            }
            None => {}
        }
        let ann = add_annotations(&mut params, &top, &g.annotations);
        p.annotations = ann;
        params.push(p);
        let idx = params.len() - 1;
        top.push(idx);
        push_elements(&mut params, idx);
    }
    let mut techs = Vec::new();
    let mut errors = Vec::new();
    for t in &prog.techniques {
        let tann = add_annotations(&mut params, &top, &t.annotations);
        let mut passes = Vec::new();
        let mut valid = true;
        for p in &t.passes {
            let pann = add_annotations(&mut params, &top, &p.annotations);
            let mut pc = PassC {
                name: cstring(&p.name),
                annotations: pann,
                states: Vec::new(),
                vs: None,
                ps: None,
            };
            for s in &p.states {
                let low = s.name.to_ascii_lowercase();
                if low == "vertexshader" || low == "pixelshader" {
                    let pixel = low == "pixelshader";
                    match compile_shader_expr(&prog, &s.value, pixel, flags) {
                        Ok(mut sh) => {
                            if let Some(x) = sh.as_mut() {
                                create_device_shader(dev, x, pixel);
                            }
                            if pixel {
                                pc.ps = Some(sh);
                            } else {
                                pc.vs = Some(sh);
                            }
                        }
                        Err(m) => {
                            errors.push(format!("{}/{} : {m}", t.name, p.name));
                            valid = false;
                        }
                    }
                } else {
                    pc.states.push(s.clone());
                }
            }
            passes.push(pc);
        }
        techs.push(Tech {
            name: cstring(&t.name),
            annotations: tann,
            passes,
            valid,
        });
    }
    if !techs.is_empty() && techs.iter().all(|t| !t.valid) {
        return Err(errors.join("\n"));
    }
    for e in &errors {
        crate::log_warn!("d3dx9", "effet : technique écartée : {e}");
    }
    Ok(finish(dev, params, top, techs, src, defines, dir, flags))
}

/// Éléments de tableau : leurs propres poignées.
fn push_elements(params: &mut Vec<Param>, idx: usize) {
    let n = params[idx].elements;
    for e in 0..n as usize {
        let root = &params[idx];
        let el = Param {
            name: root.name.clone(),
            semantic: root.semantic.clone(),
            class: root.class,
            ptype: root.ptype,
            rows: root.rows,
            cols: root.cols,
            elements: 0,
            flags: root.flags,
            elem_of: Some((idx, e)),
            elems: Vec::new(),
            annotations: Vec::new(),
            data: Vec::new(),
            obj: 0,
            string: Vec::new(),
            sampler: root.sampler.clone(),
            shader: None,
        };
        params.push(el);
        let k = params.len() - 1;
        params[idx].elems.push(k);
    }
}

/// Objet effet à partir de ses paramètres et techniques.
unsafe fn finish(
    dev: usize,
    params: Vec<Param>,
    top: Vec<usize>,
    techs: Vec<Tech>,
    src: &[u8],
    defines: Vec<(String, String)>,
    dir: Option<std::path::PathBuf>,
    flags: u32,
) -> usize {
    let nh = params.len() + techs.len() + techs.iter().map(|t| t.passes.len()).sum::<usize>();
    com_call!(dev, DEV_ADDREF, fn() -> u32);
    let cur = techs.iter().position(|t| t.valid).unwrap_or(0);
    let st = State {
        dev,
        params,
        top,
        techs,
        handles: vec![0; nh.max(1)],
        cur_tech: cur,
        active_pass: None,
        saved: Vec::new(),
        blocks: Vec::new(),
        recording: None,
        source: src.to_vec(),
        defines,
        dir,
        flags,
        creator: cstring("Winlux D3DX9"),
        state_manager: 0,
    };
    Box::into_raw(Box::new(Effect {
        vtbl: effect_vtable(),
        refs: AtomicU32::new(1),
        st,
    })) as usize
}

// ═══════════════════════════════════════════════════════════════════════
// Application des passes
// ═══════════════════════════════════════════════════════════════════════

impl State {
    /// Valeur entière d'un état (nom d'énumération, paramètre ou constante).
    fn state_value(&self, cat: Cat, e: &Expr) -> u32 {
        let float = cat == Cat::Float;
        let num = |v: f32| if float { v.to_bits() } else { v as i64 as u32 };
        match e {
            Expr::Ident(n) => {
                if let Some(v) = enum_value(cat, n) {
                    return v;
                }
                if let Some(i) = self.param_by_name(n) {
                    let v = self.values(i);
                    return num(v.first().copied().unwrap_or(0.0));
                }
                crate::log_warn!("d3dx9", "effet : valeur d'état inconnue : {n}");
                0
            }
            Expr::Bin("|", a, b) => self.state_value(cat, a) | self.state_value(cat, b),
            // Entier littéral : ses 32 bits exacts (couleurs 0xAARRGGBB).
            Expr::Num(v, false) if !float && *v >= 0.0 && *v <= u32::MAX as f64 => *v as u32,
            e => match eval_const(e, &self.params, &self.top) {
                Some(v) => num(v.first().copied().unwrap_or(0.0)),
                None => 0,
            },
        }
    }

    fn texture_of(&self, e: &Expr) -> usize {
        match e {
            Expr::Ident(n) => self
                .param_by_name(n)
                .map(|i| self.params[self.slot(i).0].obj)
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn matrix_of(&self, e: &Expr) -> Option<[f32; 16]> {
        let v = match e {
            Expr::Ident(n) => self.values(self.param_by_name(n)?),
            e => eval_const(e, &self.params, &self.top)?,
        };
        let mut m = [0f32; 16];
        for (k, x) in v.iter().take(16).enumerate() {
            m[k] = *x;
        }
        Some(m)
    }

    unsafe fn apply_sampler(&self, stage: u32, states: &[StateAssign]) {
        for s in states {
            let low = s.name.to_ascii_lowercase();
            if low == "texture" {
                com_call!(
                    self.dev,
                    DEV_SET_TEXTURE,
                    fn(u32, usize) -> i32,
                    stage,
                    self.texture_of(&s.value)
                );
            } else if let Some(&(_, id, cat)) = SAMPLER_STATES.iter().find(|x| x.0 == low) {
                let v = self.state_value(cat, &s.value);
                com_call!(self.dev, DEV_SET_SS, fn(u32, u32, u32) -> i32, stage, id, v);
            }
        }
    }

    unsafe fn apply_state(&self, s: &StateAssign) {
        let low = s.name.to_ascii_lowercase();
        let dev = self.dev;
        let idx = s.index.unwrap_or(0);
        if let Some(&(_, id, cat)) = RENDER_STATES.iter().find(|x| x.0 == low) {
            let v = self.state_value(cat, &s.value);
            com_call!(dev, DEV_SET_RS, fn(u32, u32) -> i32, id, v);
        } else if let Some(&(_, id, cat)) = SAMPLER_STATES.iter().find(|x| x.0 == low) {
            let v = self.state_value(cat, &s.value);
            com_call!(dev, DEV_SET_SS, fn(u32, u32, u32) -> i32, idx, id, v);
        } else if let Some(&(_, id, cat)) = STAGE_STATES.iter().find(|x| x.0 == low) {
            let v = self.state_value(cat, &s.value);
            com_call!(dev, DEV_SET_TSS, fn(u32, u32, u32) -> i32, idx, id, v);
        } else if low == "texture" {
            com_call!(
                dev,
                DEV_SET_TEXTURE,
                fn(u32, usize) -> i32,
                idx,
                self.texture_of(&s.value)
            );
        } else if low == "sampler" {
            if let Expr::Ident(n) = &s.value {
                if let Some(i) = self.param_by_name(n) {
                    self.apply_sampler(idx, &self.params[self.slot(i).0].sampler);
                }
            }
        } else if let Some(t) = match low.as_str() {
            "worldtransform" => Some(256 + idx),
            "viewtransform" => Some(2),
            "projectiontransform" => Some(3),
            "texturetransform" => Some(16 + idx),
            _ => None,
        } {
            if let Some(m) = self.matrix_of(&s.value) {
                com_call!(
                    dev,
                    DEV_SET_TRANSFORM,
                    fn(u32, *const f32) -> i32,
                    t,
                    m.as_ptr()
                );
            }
        } else if low.starts_with("vertexshaderconstant") || low.starts_with("pixelshaderconstant")
        {
            let v = match &s.value {
                Expr::Ident(n) => self.param_by_name(n).map(|i| self.values(i)),
                e => eval_const(e, &self.params, &self.top),
            };
            if let Some(mut v) = v {
                v.resize(v.len().div_ceil(4).max(1) * 4, 0.0);
                let f = if low.starts_with("vertex") {
                    DEV_SET_VS_F
                } else {
                    DEV_SET_PS_F
                };
                com_call!(
                    dev,
                    f,
                    fn(u32, *const f32, u32) -> i32,
                    idx,
                    v.as_ptr(),
                    (v.len() / 4) as u32
                );
            }
        } else {
            crate::log_warn!("d3dx9", "effet : état non pris en charge : {}", s.name);
        }
    }

    /// Range un paramètre dans les registres d'une constante de shader.
    fn layout(&self, c: &hlsl::CtabEntry, i: usize) -> Vec<f32> {
        let p = &self.params[self.slot(i).0];
        let v = self.values(i);
        let (rows, cols) = (p.rows.max(1) as usize, p.cols.max(1) as usize);
        let per = rows * cols;
        let elems = (v.len() / per.max(1)).max(1);
        let mut out = Vec::new();
        for e in 0..elems {
            let m = &v[e * per..(e * per + per).min(v.len())];
            let g = |r: usize, k: usize| m.get(r * cols + k).copied().unwrap_or(0.0);
            match c.class {
                3 => {
                    for k in 0..cols {
                        let mut r4 = [0f32; 4];
                        for (r, x) in r4.iter_mut().enumerate().take(rows) {
                            *x = g(r, k);
                        }
                        out.extend(r4);
                    }
                }
                _ => {
                    for r in 0..rows {
                        let mut r4 = [0f32; 4];
                        for (k, x) in r4.iter_mut().enumerate().take(cols) {
                            *x = g(r, k);
                        }
                        out.extend(r4);
                    }
                }
            }
        }
        out.truncate(c.count as usize * 4);
        out
    }

    unsafe fn upload(&self, s: &Shader, pixel: bool) {
        for c in &s.ctab {
            let Some(i) = self.param_by_name(&c.name) else {
                continue;
            };
            if c.set == 3 {
                let stage = if pixel {
                    c.index as u32
                } else {
                    257 + c.index as u32
                };
                let root = self.slot(i).0;
                self.apply_sampler(stage, &self.params[root].sampler);
                continue;
            }
            let v = self.layout(c, i);
            if v.is_empty() {
                continue;
            }
            let f = if pixel { DEV_SET_PS_F } else { DEV_SET_VS_F };
            com_call!(
                self.dev,
                f,
                fn(u32, *const f32, u32) -> i32,
                c.index as u32,
                v.as_ptr(),
                (v.len() / 4) as u32
            );
        }
    }

    unsafe fn commit(&self) {
        let Some(p) = self.active_pass else { return };
        let pass = &self.techs[self.cur_tech].passes[p];
        if let Some(Some(s)) = &pass.vs {
            self.upload(s, false);
        }
        if let Some(Some(s)) = &pass.ps {
            self.upload(s, true);
        }
    }

    unsafe fn save_states(&mut self) {
        let dev = self.dev;
        let mut saved = Vec::new();
        let Some(tech) = self.techs.get(self.cur_tech) else {
            return;
        };
        for pass in &tech.passes {
            if pass.vs.is_some() {
                let mut o = 0usize;
                com_call!(dev, DEV_GET_VS, fn(*mut usize) -> i32, &mut o);
                saved.push(Saved::Vs(o));
            }
            if pass.ps.is_some() {
                let mut o = 0usize;
                com_call!(dev, DEV_GET_PS, fn(*mut usize) -> i32, &mut o);
                saved.push(Saved::Ps(o));
            }
            for s in &pass.states {
                let low = s.name.to_ascii_lowercase();
                let idx = s.index.unwrap_or(0);
                if let Some(&(_, id, _)) = RENDER_STATES.iter().find(|x| x.0 == low) {
                    let mut v = 0u32;
                    com_call!(dev, DEV_GET_RS, fn(u32, *mut u32) -> i32, id, &mut v);
                    saved.push(Saved::Rs(id, v));
                } else if let Some(&(_, id, _)) = SAMPLER_STATES.iter().find(|x| x.0 == low) {
                    let mut v = 0u32;
                    com_call!(
                        dev,
                        DEV_GET_SS,
                        fn(u32, u32, *mut u32) -> i32,
                        idx,
                        id,
                        &mut v
                    );
                    saved.push(Saved::Ss(idx, id, v));
                } else if let Some(&(_, id, _)) = STAGE_STATES.iter().find(|x| x.0 == low) {
                    let mut v = 0u32;
                    com_call!(
                        dev,
                        DEV_GET_TSS,
                        fn(u32, u32, *mut u32) -> i32,
                        idx,
                        id,
                        &mut v
                    );
                    saved.push(Saved::Tss(idx, id, v));
                } else if low == "texture" {
                    let mut o = 0usize;
                    com_call!(
                        dev,
                        DEV_GET_TEXTURE,
                        fn(u32, *mut usize) -> i32,
                        idx,
                        &mut o
                    );
                    saved.push(Saved::Tex(idx, o));
                }
            }
        }
        self.saved = saved;
    }

    unsafe fn restore_states(&mut self) {
        let dev = self.dev;
        for s in std::mem::take(&mut self.saved).into_iter().rev() {
            match s {
                Saved::Rs(id, v) => {
                    com_call!(dev, DEV_SET_RS, fn(u32, u32) -> i32, id, v);
                }
                Saved::Ss(st, id, v) => {
                    com_call!(dev, DEV_SET_SS, fn(u32, u32, u32) -> i32, st, id, v);
                }
                Saved::Tss(st, id, v) => {
                    com_call!(dev, DEV_SET_TSS, fn(u32, u32, u32) -> i32, st, id, v);
                }
                Saved::Tex(st, o) => {
                    com_call!(dev, DEV_SET_TEXTURE, fn(u32, usize) -> i32, st, o);
                    if o != 0 {
                        com_call!(o, 2, fn() -> u32);
                    }
                }
                Saved::Vs(o) | Saved::Ps(o) => {
                    let f = if matches!(s, Saved::Vs(_)) {
                        DEV_SET_VS
                    } else {
                        DEV_SET_PS
                    };
                    com_call!(dev, f, fn(usize) -> i32, o);
                    if o != 0 {
                        com_call!(o, 2, fn() -> u32);
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ID3DXBaseEffect
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn e_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    e_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn e_addref(this: usize) -> u32 {
    (*(this as *mut Effect))
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}
unsafe extern "win64" fn e_release(this: usize) -> u32 {
    let e = this as *mut Effect;
    let n = (*e).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let st = &mut (*e).st;
        for p in &st.params {
            if p.obj != 0 && p.elem_of.is_none() {
                com_call!(p.obj, 2, fn() -> u32);
            }
        }
        for t in &st.techs {
            for p in &t.passes {
                for s in [&p.vs, &p.ps].into_iter().flatten().flatten() {
                    if s.obj != 0 {
                        com_call!(s.obj, 2, fn() -> u32);
                    }
                }
            }
        }
        com_call!(st.dev, DEV_RELEASE, fn() -> u32);
        drop(Box::from_raw(e));
    }
    n
}

/// Écrit une structure de description champ par champ (pointeurs alignés).
struct Desc {
    at: usize,
    off: usize,
}
impl Desc {
    unsafe fn ptr(&mut self, v: usize) {
        let ps = ptr_size();
        self.off = self.off.div_ceil(ps) * ps;
        put_ptr(self.at + self.off, v);
        self.off += ps;
    }
    unsafe fn u32(&mut self, v: u32) {
        wr32(self.at + self.off, v);
        self.off += 4;
    }
}

unsafe extern "win64" fn e_get_desc(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let s = eff(this);
    let mut d = Desc { at: out, off: 0 };
    d.ptr(s.creator.as_ptr() as usize);
    d.u32(s.top.len() as u32);
    d.u32(s.techs.len() as u32);
    d.u32(0);
    S_OK
}
unsafe extern "win64" fn e_get_parameter_desc(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), out != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let p = &s.params[i];
    let mut d = Desc { at: out, off: 0 };
    d.ptr(p.name.as_ptr() as usize);
    d.ptr(if p.semantic.is_empty() {
        0
    } else {
        p.semantic.as_ptr() as usize
    });
    d.u32(p.class);
    d.u32(p.ptype);
    d.u32(p.rows);
    d.u32(p.cols);
    d.u32(p.elements);
    d.u32(p.annotations.len() as u32);
    d.u32(0);
    d.u32(p.flags);
    let bytes = match p.ptype {
        PT_BOOL | PT_INT | PT_FLOAT => s.slot(i).2 * 4,
        _ => 4 * p.elements.max(1) as usize,
    };
    d.u32(bytes as u32);
    S_OK
}
unsafe extern "win64" fn e_get_technique_desc(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let t = match s.resolve(h) {
        Some(H::Tech(t)) => t,
        None if h == 0 => s.cur_tech,
        _ => return D3DERR_INVALIDCALL,
    };
    let Some(tech) = s.techs.get(t) else {
        return D3DERR_INVALIDCALL;
    };
    let mut d = Desc { at: out, off: 0 };
    d.ptr(tech.name.as_ptr() as usize);
    d.u32(tech.passes.len() as u32);
    d.u32(tech.annotations.len() as u32);
    S_OK
}
unsafe extern "win64" fn e_get_pass_desc(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let Some(H::Pass(t, p)) = s.resolve(h) else {
        return D3DERR_INVALIDCALL;
    };
    let pass = &s.techs[t].passes[p];
    let sh = |x: &Option<Option<Shader>>| match x {
        Some(Some(s)) => s.tokens.as_ptr() as usize,
        _ => 0,
    };
    let mut d = Desc { at: out, off: 0 };
    d.ptr(pass.name.as_ptr() as usize);
    d.u32(pass.annotations.len() as u32);
    d.ptr(sh(&pass.vs));
    d.ptr(sh(&pass.ps));
    S_OK
}
unsafe extern "win64" fn e_get_function_desc(_this: usize, _h: usize, _out: usize) -> i32 {
    D3DERR_INVALIDCALL
}
unsafe extern "win64" fn e_get_parameter(this: usize, parent: usize, index: u32) -> usize {
    let s = eff(this);
    if parent != 0 {
        return 0;
    }
    s.top
        .get(index as usize)
        .map(|&i| s.handle_of(H::Param(i)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_parameter_by_name(this: usize, parent: usize, name: usize) -> usize {
    let s = eff(this);
    if name == 0 {
        return 0;
    }
    let n = super::cstr(name as *const u8);
    if parent != 0 {
        // Membres de structure : non pris en charge.
        return 0;
    }
    s.param_by_name(&n)
        .map(|i| s.handle_of(H::Param(i)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_parameter_by_semantic(
    this: usize,
    parent: usize,
    sem: usize,
) -> usize {
    let s = eff(this);
    if sem == 0 || parent != 0 {
        return 0;
    }
    let want = super::cstr(sem as *const u8).to_ascii_uppercase();
    s.top
        .iter()
        .copied()
        .find(|&i| {
            let p = &s.params[i].semantic;
            !p.is_empty() && String::from_utf8_lossy(&p[..p.len() - 1]).to_ascii_uppercase() == want
        })
        .map(|i| s.handle_of(H::Param(i)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_parameter_element(this: usize, h: usize, index: u32) -> usize {
    let s = eff(this);
    let Some(i) = s.param(h) else { return 0 };
    s.params[i]
        .elems
        .get(index as usize)
        .map(|&k| s.handle_of(H::Param(k)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_technique(this: usize, index: u32) -> usize {
    let s = eff(this);
    if (index as usize) < s.techs.len() {
        s.handle_of(H::Tech(index as usize))
    } else {
        0
    }
}
unsafe extern "win64" fn e_get_technique_by_name(this: usize, name: usize) -> usize {
    let s = eff(this);
    if name == 0 {
        return 0;
    }
    let n = super::cstr(name as *const u8);
    s.techs
        .iter()
        .position(|t| t.name[..t.name.len() - 1] == *n.as_bytes())
        .map(|t| s.handle_of(H::Tech(t)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_pass(this: usize, tech: usize, index: u32) -> usize {
    let s = eff(this);
    match s.resolve(tech) {
        Some(H::Tech(t)) if (index as usize) < s.techs[t].passes.len() => {
            s.handle_of(H::Pass(t, index as usize))
        }
        _ => 0,
    }
}
unsafe extern "win64" fn e_get_pass_by_name(this: usize, tech: usize, name: usize) -> usize {
    let s = eff(this);
    let (Some(H::Tech(t)), true) = (s.resolve(tech), name != 0) else {
        return 0;
    };
    let n = super::cstr(name as *const u8);
    s.techs[t]
        .passes
        .iter()
        .position(|p| p.name[..p.name.len() - 1] == *n.as_bytes())
        .map(|p| s.handle_of(H::Pass(t, p)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_function(_this: usize, _index: u32) -> usize {
    0
}
unsafe extern "win64" fn e_get_function_by_name(_this: usize, _name: usize) -> usize {
    0
}
unsafe extern "win64" fn e_get_annotation(this: usize, obj: usize, index: u32) -> usize {
    let s = eff(this);
    let Some(h) = s.resolve(obj) else { return 0 };
    s.annotations_of(&h)
        .get(index as usize)
        .map(|&i| s.handle_of(H::Param(i)))
        .unwrap_or(0)
}
unsafe extern "win64" fn e_get_annotation_by_name(this: usize, obj: usize, name: usize) -> usize {
    let s = eff(this);
    let (Some(h), true) = (s.resolve(obj), name != 0) else {
        return 0;
    };
    let n = super::cstr(name as *const u8);
    s.annotations_of(&h)
        .iter()
        .copied()
        .find(|&i| s.params[i].name_str() == n)
        .map(|i| s.handle_of(H::Param(i)))
        .unwrap_or(0)
}

unsafe fn read_f32(p: usize, n: usize) -> Vec<f32> {
    (0..n).map(|k| f32::from_bits(rd32(p + 4 * k))).collect()
}
unsafe fn read_i32(p: usize, n: usize) -> Vec<f32> {
    (0..n).map(|k| rd32(p + 4 * k) as i32 as f32).collect()
}
unsafe fn write_f32(p: usize, v: &[f32]) {
    for (k, x) in v.iter().enumerate() {
        wr32(p + 4 * k, x.to_bits());
    }
}
unsafe fn write_i32(p: usize, v: &[f32]) {
    for (k, x) in v.iter().enumerate() {
        wr32(p + 4 * k, *x as i32 as u32);
    }
}

unsafe fn set_texture_param(s: &mut State, i: usize, tex: usize) -> i32 {
    let r = s.slot(i).0;
    if !matches!(s.params[r].ptype, PT_TEXTURE..=9) {
        return D3DERR_INVALIDCALL;
    }
    if tex != 0 {
        com_call!(tex, 1, fn() -> u32);
    }
    let old = std::mem::replace(&mut s.params[r].obj, tex);
    if old != 0 {
        com_call!(old, 2, fn() -> u32);
    }
    S_OK
}

unsafe extern "win64" fn e_set_value(this: usize, h: usize, data: usize, bytes: u32) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), data != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let r = s.slot(i).0;
    match s.params[r].ptype {
        PT_FLOAT => s.set_values(i, &read_f32(data, bytes as usize / 4)),
        PT_BOOL | PT_INT => s.set_values(i, &read_i32(data, bytes as usize / 4)),
        PT_STRING => s.params[r].string = cstring(&super::cstr(get_ptr(data) as *const u8)),
        PT_TEXTURE..=9 => return set_texture_param(s, i, get_ptr(data)),
        _ => return D3DERR_INVALIDCALL,
    }
    S_OK
}
unsafe extern "win64" fn e_get_value(this: usize, h: usize, data: usize, bytes: u32) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), data != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let r = s.slot(i).0;
    let v = s.values(i);
    let n = (bytes as usize / 4).min(v.len());
    match s.params[r].ptype {
        PT_FLOAT => write_f32(data, &v[..n]),
        PT_BOOL | PT_INT => write_i32(data, &v[..n]),
        PT_STRING => put_ptr(data, s.params[r].string.as_ptr() as usize),
        PT_TEXTURE..=9 => {
            let o = s.params[r].obj;
            if o != 0 {
                com_call!(o, 1, fn() -> u32);
            }
            put_ptr(data, o);
        }
        _ => return D3DERR_INVALIDCALL,
    }
    S_OK
}
unsafe fn set_list(this: usize, h: usize, v: Vec<f32>) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    if !matches!(s.params[s.slot(i).0].ptype, PT_BOOL | PT_INT | PT_FLOAT) {
        return D3DERR_INVALIDCALL;
    }
    s.set_values(i, &v);
    S_OK
}
unsafe fn get_list(this: usize, h: usize) -> Option<Vec<f32>> {
    let s = eff(this);
    let i = s.param(h)?;
    if !matches!(s.params[s.slot(i).0].ptype, PT_BOOL | PT_INT | PT_FLOAT) {
        return None;
    }
    Some(s.values(i))
}
unsafe extern "win64" fn e_set_bool(this: usize, h: usize, b: i32) -> i32 {
    set_list(this, h, vec![(b != 0) as i32 as f32])
}
unsafe extern "win64" fn e_get_bool(this: usize, h: usize, out: usize) -> i32 {
    match get_list(this, h) {
        Some(v) if out != 0 => {
            wr32(out, (v.first().copied().unwrap_or(0.0) != 0.0) as u32);
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_set_bool_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    set_list(
        this,
        h,
        read_i32(p, n as usize)
            .iter()
            .map(|x| (*x != 0.0) as i32 as f32)
            .collect(),
    )
}
unsafe extern "win64" fn e_get_bool_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    match get_list(this, h) {
        Some(v) if p != 0 => {
            let b: Vec<f32> = v
                .iter()
                .map(|x| (*x != 0.0) as i32 as f32)
                .take(n as usize)
                .collect();
            write_i32(p, &b);
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_set_int(this: usize, h: usize, v: i32) -> i32 {
    set_list(this, h, vec![v as f32])
}
unsafe extern "win64" fn e_get_int(this: usize, h: usize, out: usize) -> i32 {
    match get_list(this, h) {
        Some(v) if out != 0 => {
            wr32(out, v.first().copied().unwrap_or(0.0) as i32 as u32);
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_set_int_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    set_list(this, h, read_i32(p, n as usize))
}
unsafe extern "win64" fn e_get_int_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    match get_list(this, h) {
        Some(v) if p != 0 => {
            write_i32(p, &v[..v.len().min(n as usize)]);
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_set_float(this: usize, h: usize, v: f32) -> i32 {
    set_list(this, h, vec![v])
}
unsafe extern "win64" fn e_get_float(this: usize, h: usize, out: usize) -> i32 {
    match get_list(this, h) {
        Some(v) if out != 0 => {
            wr32(out, v.first().copied().unwrap_or(0.0).to_bits());
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_set_float_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    set_list(this, h, read_f32(p, n as usize))
}
unsafe extern "win64" fn e_get_float_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    match get_list(this, h) {
        Some(v) if p != 0 => {
            write_f32(p, &v[..v.len().min(n as usize)]);
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}

/// Vecteurs de 4 flottants posés sur les éléments (cols composantes chacun).
unsafe fn set_vectors(this: usize, h: usize, p: usize, n: usize) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    let r = s.slot(i).0;
    let cols = s.params[r].cols.max(1) as usize;
    let src = read_f32(p, 4 * n);
    let mut v = Vec::new();
    for k in 0..n {
        v.extend_from_slice(&src[k * 4..k * 4 + cols.min(4)]);
    }
    if s.params[r].ptype != PT_FLOAT {
        return set_list(this, h, v);
    }
    s.set_values(i, &v);
    S_OK
}
unsafe fn get_vectors(this: usize, h: usize, p: usize, n: usize) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), p != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let cols = s.params[s.slot(i).0].cols.max(1) as usize;
    let v = s.values(i);
    for k in 0..n {
        let mut r4 = [0f32; 4];
        for (c, x) in r4.iter_mut().enumerate().take(cols) {
            *x = v.get(k * cols + c).copied().unwrap_or(0.0);
        }
        write_f32(p + 16 * k, &r4);
    }
    S_OK
}
unsafe extern "win64" fn e_set_vector(this: usize, h: usize, p: usize) -> i32 {
    set_vectors(this, h, p, 1)
}
unsafe extern "win64" fn e_get_vector(this: usize, h: usize, p: usize) -> i32 {
    get_vectors(this, h, p, 1)
}
unsafe extern "win64" fn e_set_vector_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    set_vectors(this, h, p, n as usize)
}
unsafe extern "win64" fn e_get_vector_array(this: usize, h: usize, p: usize, n: u32) -> i32 {
    get_vectors(this, h, p, n as usize)
}

/// Matrices 4×4 (lignes) → paramètre rows×cols par élément.
unsafe fn set_matrices(this: usize, h: usize, mats: &[[f32; 16]], transpose: bool) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    let r = s.slot(i).0;
    let (rows, cols) = (
        s.params[r].rows.max(1) as usize,
        s.params[r].cols.max(1) as usize,
    );
    let mut v = Vec::new();
    for m in mats {
        for a in 0..rows {
            for b in 0..cols {
                v.push(if transpose {
                    m[b * 4 + a]
                } else {
                    m[a * 4 + b]
                });
            }
        }
    }
    s.set_values(i, &v);
    S_OK
}
unsafe fn get_matrices(this: usize, h: usize, out: &[usize], transpose: bool) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    let r = s.slot(i).0;
    let (rows, cols) = (
        s.params[r].rows.max(1) as usize,
        s.params[r].cols.max(1) as usize,
    );
    let v = s.values(i);
    for (k, &p) in out.iter().enumerate() {
        if p == 0 {
            return D3DERR_INVALIDCALL;
        }
        let mut m = [0f32; 16];
        for a in 0..rows {
            for b in 0..cols {
                let x = v
                    .get(k * rows * cols + a * cols + b)
                    .copied()
                    .unwrap_or(0.0);
                if transpose {
                    m[b * 4 + a] = x;
                } else {
                    m[a * 4 + b] = x;
                }
            }
        }
        write_f32(p, &m);
    }
    S_OK
}
unsafe fn mats_at(p: usize, n: usize) -> Vec<[f32; 16]> {
    (0..n)
        .map(|k| {
            let v = read_f32(p + 64 * k, 16);
            let mut m = [0f32; 16];
            m.copy_from_slice(&v);
            m
        })
        .collect()
}
unsafe fn mats_ptrs(p: usize, n: usize) -> Vec<[f32; 16]> {
    (0..n)
        .flat_map(|k| mats_at(get_ptr(p + k * ptr_size()), 1))
        .collect()
}
unsafe extern "win64" fn e_set_matrix(this: usize, h: usize, m: usize) -> i32 {
    if m == 0 {
        return D3DERR_INVALIDCALL;
    }
    set_matrices(this, h, &mats_at(m, 1), false)
}
unsafe extern "win64" fn e_get_matrix(this: usize, h: usize, m: usize) -> i32 {
    get_matrices(this, h, &[m], false)
}
unsafe extern "win64" fn e_set_matrix_array(this: usize, h: usize, m: usize, n: u32) -> i32 {
    set_matrices(this, h, &mats_at(m, n as usize), false)
}
unsafe extern "win64" fn e_get_matrix_array(this: usize, h: usize, m: usize, n: u32) -> i32 {
    let out: Vec<usize> = (0..n as usize).map(|k| m + 64 * k).collect();
    get_matrices(this, h, &out, false)
}
unsafe extern "win64" fn e_set_matrix_pointer_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    set_matrices(this, h, &mats_ptrs(m, n as usize), false)
}
unsafe extern "win64" fn e_get_matrix_pointer_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    let out: Vec<usize> = (0..n as usize)
        .map(|k| get_ptr(m + k * ptr_size()))
        .collect();
    get_matrices(this, h, &out, false)
}
unsafe extern "win64" fn e_set_matrix_transpose(this: usize, h: usize, m: usize) -> i32 {
    if m == 0 {
        return D3DERR_INVALIDCALL;
    }
    set_matrices(this, h, &mats_at(m, 1), true)
}
unsafe extern "win64" fn e_get_matrix_transpose(this: usize, h: usize, m: usize) -> i32 {
    get_matrices(this, h, &[m], true)
}
unsafe extern "win64" fn e_set_matrix_transpose_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    set_matrices(this, h, &mats_at(m, n as usize), true)
}
unsafe extern "win64" fn e_get_matrix_transpose_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    let out: Vec<usize> = (0..n as usize).map(|k| m + 64 * k).collect();
    get_matrices(this, h, &out, true)
}
unsafe extern "win64" fn e_set_matrix_transpose_pointer_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    set_matrices(this, h, &mats_ptrs(m, n as usize), true)
}
unsafe extern "win64" fn e_get_matrix_transpose_pointer_array(
    this: usize,
    h: usize,
    m: usize,
    n: u32,
) -> i32 {
    let out: Vec<usize> = (0..n as usize)
        .map(|k| get_ptr(m + k * ptr_size()))
        .collect();
    get_matrices(this, h, &out, true)
}
unsafe extern "win64" fn e_set_string(this: usize, h: usize, v: usize) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), v != 0) else {
        return D3DERR_INVALIDCALL;
    };
    if s.params[i].ptype != PT_STRING {
        return D3DERR_INVALIDCALL;
    }
    s.params[i].string = cstring(&super::cstr(v as *const u8));
    S_OK
}
unsafe extern "win64" fn e_get_string(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), out != 0) else {
        return D3DERR_INVALIDCALL;
    };
    if s.params[i].ptype != PT_STRING {
        return D3DERR_INVALIDCALL;
    }
    if s.params[i].string.is_empty() {
        s.params[i].string = vec![0];
    }
    put_ptr(out, s.params[i].string.as_ptr() as usize);
    S_OK
}
unsafe extern "win64" fn e_set_texture(this: usize, h: usize, tex: usize) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    set_texture_param(s, i, tex)
}
unsafe extern "win64" fn e_get_texture(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), out != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let o = s.params[s.slot(i).0].obj;
    if o != 0 {
        com_call!(o, 1, fn() -> u32);
    }
    put_ptr(out, o);
    S_OK
}
unsafe fn shader_param(this: usize, h: usize, out: usize, pixel: bool) -> i32 {
    let s = eff(this);
    put_ptr(out, 0);
    let Some(i) = s.param(h) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = s.dev;
    let p = &mut s.params[i];
    let Some(sh) = p.shader.as_mut() else {
        return D3DERR_INVALIDCALL;
    };
    if sh.obj == 0 {
        create_device_shader(dev, sh, pixel);
    }
    if sh.obj != 0 {
        com_call!(sh.obj, 1, fn() -> u32);
    }
    put_ptr(out, sh.obj);
    S_OK
}
unsafe extern "win64" fn e_get_pixel_shader(this: usize, h: usize, out: usize) -> i32 {
    shader_param(this, h, out, true)
}
unsafe extern "win64" fn e_get_vertex_shader(this: usize, h: usize, out: usize) -> i32 {
    shader_param(this, h, out, false)
}
unsafe extern "win64" fn e_set_array_range(_this: usize, _h: usize, _a: u32, _b: u32) -> i32 {
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// ID3DXEffect
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn e_get_pool(_this: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    S_OK
}
unsafe extern "win64" fn e_set_technique(this: usize, h: usize) -> i32 {
    let s = eff(this);
    match s.resolve(h) {
        Some(H::Tech(t)) => {
            s.cur_tech = t;
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_get_current_technique(this: usize) -> usize {
    let s = eff(this);
    if s.techs.is_empty() {
        0
    } else {
        s.handle_of(H::Tech(s.cur_tech))
    }
}
unsafe extern "win64" fn e_validate_technique(this: usize, h: usize) -> i32 {
    let s = eff(this);
    match s.resolve(h) {
        Some(H::Tech(t)) if s.techs[t].valid => S_OK,
        _ => E_FAIL,
    }
}
unsafe extern "win64" fn e_find_next_valid_technique(this: usize, h: usize, out: usize) -> i32 {
    let s = eff(this);
    let start = match s.resolve(h) {
        Some(H::Tech(t)) => t + 1,
        _ => 0,
    };
    let next = (start..s.techs.len()).find(|&t| s.techs[t].valid);
    put_ptr(out, next.map(|t| s.handle_of(H::Tech(t))).unwrap_or(0));
    if next.is_some() {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn e_is_parameter_used(this: usize, h: usize, tech: usize) -> i32 {
    let s = eff(this);
    let Some(i) = s.param(h) else { return 0 };
    let name = s.params[s.slot(i).0].name_str().to_string();
    let t = match s.resolve(tech) {
        Some(H::Tech(t)) => t,
        _ => s.cur_tech,
    };
    let Some(tech) = s.techs.get(t) else { return 0 };
    let used = tech.passes.iter().any(|p| {
        let in_shader = [&p.vs, &p.ps]
            .into_iter()
            .flatten()
            .flatten()
            .any(|sh| sh.ctab.iter().any(|c| c.name == name));
        let in_state = p
            .states
            .iter()
            .any(|st| matches!(&st.value, Expr::Ident(n) if *n == name));
        in_shader || in_state
    });
    used as i32
}
unsafe extern "win64" fn e_begin(this: usize, passes: usize, flags: u32) -> i32 {
    let s = eff(this);
    let Some(tech) = s.techs.get(s.cur_tech) else {
        return D3DERR_INVALIDCALL;
    };
    if passes != 0 {
        wr32(passes, tech.passes.len() as u32);
    }
    // D3DXFX_DONOTSAVESTATE = 1
    if flags & 1 == 0 {
        s.save_states();
    } else {
        s.saved.clear();
    }
    S_OK
}
unsafe extern "win64" fn e_begin_pass(this: usize, pass: u32) -> i32 {
    let s = eff(this);
    let Some(tech) = s.techs.get(s.cur_tech) else {
        return D3DERR_INVALIDCALL;
    };
    let Some(p) = tech.passes.get(pass as usize) else {
        return D3DERR_INVALIDCALL;
    };
    for st in &p.states {
        s.apply_state(st);
    }
    if let Some(vs) = &p.vs {
        let o = vs.as_ref().map(|x| x.obj).unwrap_or(0);
        com_call!(s.dev, DEV_SET_VS, fn(usize) -> i32, o);
    }
    if let Some(ps) = &p.ps {
        let o = ps.as_ref().map(|x| x.obj).unwrap_or(0);
        com_call!(s.dev, DEV_SET_PS, fn(usize) -> i32, o);
    }
    s.active_pass = Some(pass as usize);
    s.commit();
    S_OK
}
unsafe extern "win64" fn e_commit_changes(this: usize) -> i32 {
    let s = eff(this);
    if s.active_pass.is_none() {
        return D3DERR_INVALIDCALL;
    }
    s.commit();
    S_OK
}
unsafe extern "win64" fn e_end_pass(this: usize) -> i32 {
    eff(this).active_pass = None;
    S_OK
}
unsafe extern "win64" fn e_end(this: usize) -> i32 {
    let s = eff(this);
    s.active_pass = None;
    s.restore_states();
    S_OK
}
unsafe extern "win64" fn e_get_device(this: usize, out: usize) -> i32 {
    let s = eff(this);
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    com_call!(s.dev, DEV_ADDREF, fn() -> u32);
    put_ptr(out, s.dev);
    S_OK
}
unsafe extern "win64" fn e_on_lost_device(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn e_on_reset_device(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn e_set_state_manager(this: usize, m: usize) -> i32 {
    if m != 0 {
        crate::log_warn!(
            "d3dx9",
            "ID3DXEffect::SetStateManager : états posés directement sur l'appareil"
        );
    }
    eff(this).state_manager = m;
    S_OK
}
unsafe extern "win64" fn e_get_state_manager(this: usize, out: usize) -> i32 {
    put_ptr(out, eff(this).state_manager);
    S_OK
}
unsafe extern "win64" fn e_begin_parameter_block(this: usize) -> i32 {
    let s = eff(this);
    if s.recording.is_some() {
        return D3DERR_INVALIDCALL;
    }
    s.recording = Some(HashSet::new());
    S_OK
}
unsafe extern "win64" fn e_end_parameter_block(this: usize) -> usize {
    let s = eff(this);
    let Some(rec) = s.recording.take() else {
        return 0;
    };
    let snap: Vec<(usize, Vec<f32>)> = rec
        .into_iter()
        .map(|r| (r, s.params[r].data.clone()))
        .collect();
    s.blocks.push(Some(snap));
    // Poignée opaque non nulle.
    s.blocks.len()
}
unsafe extern "win64" fn e_apply_parameter_block(this: usize, h: usize) -> i32 {
    let s = eff(this);
    let Some(Some(b)) = h.checked_sub(1).and_then(|k| s.blocks.get(k)) else {
        return D3DERR_INVALIDCALL;
    };
    for (r, d) in b.clone() {
        s.params[r].data = d;
    }
    S_OK
}
unsafe extern "win64" fn e_delete_parameter_block(this: usize, h: usize) -> i32 {
    let s = eff(this);
    match h.checked_sub(1).and_then(|k| s.blocks.get_mut(k)) {
        Some(b) => {
            *b = None;
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn e_clone_effect(this: usize, dev: usize, out: usize) -> i32 {
    let s = eff(this);
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let dev = if dev == 0 { s.dev } else { dev };
    match load(
        dev,
        &s.source.clone(),
        s.defines.clone(),
        s.dir.clone(),
        s.flags,
    ) {
        Ok(e) => {
            // Valeurs courantes recopiées.
            let n = eff(e);
            for (k, p) in s.params.iter().enumerate() {
                if let Some(q) = n.params.get_mut(k) {
                    q.data = p.data.clone();
                    q.string = p.string.clone();
                    if p.obj != 0 && p.elem_of.is_none() {
                        com_call!(p.obj, 1, fn() -> u32);
                        q.obj = p.obj;
                    }
                }
            }
            put_ptr(out, e);
            S_OK
        }
        Err(_) => E_FAIL,
    }
}
unsafe extern "win64" fn e_set_raw_value(
    this: usize,
    h: usize,
    data: usize,
    offset: u32,
    bytes: u32,
) -> i32 {
    let s = eff(this);
    let (Some(i), true) = (s.param(h), data != 0) else {
        return D3DERR_INVALIDCALL;
    };
    let mut v = s.values(i);
    let start = offset as usize / 4;
    for (k, x) in read_f32(data, bytes as usize / 4).into_iter().enumerate() {
        if let Some(y) = v.get_mut(start + k) {
            *y = x;
        }
    }
    s.set_values(i, &v);
    S_OK
}

fn f(p: *const ()) -> usize {
    p as usize
}

fn effect_vtable() -> usize {
    vtable_sig(
        "ID3DXEffect",
        &[
            (f(e_qi as *const ()), "ii"),
            (f(e_addref as *const ()), ""),
            (f(e_release as *const ()), ""),
            (f(e_get_desc as *const ()), "i"),
            (f(e_get_parameter_desc as *const ()), "ii"),
            (f(e_get_technique_desc as *const ()), "ii"),
            (f(e_get_pass_desc as *const ()), "ii"),
            (f(e_get_function_desc as *const ()), "ii"),
            (f(e_get_parameter as *const ()), "ii"),
            (f(e_get_parameter_by_name as *const ()), "ii"),
            (f(e_get_parameter_by_semantic as *const ()), "ii"),
            (f(e_get_parameter_element as *const ()), "ii"),
            (f(e_get_technique as *const ()), "i"),
            (f(e_get_technique_by_name as *const ()), "i"),
            (f(e_get_pass as *const ()), "ii"),
            (f(e_get_pass_by_name as *const ()), "ii"),
            (f(e_get_function as *const ()), "i"),
            (f(e_get_function_by_name as *const ()), "i"),
            (f(e_get_annotation as *const ()), "ii"),
            (f(e_get_annotation_by_name as *const ()), "ii"),
            (f(e_set_value as *const ()), "iii"),
            (f(e_get_value as *const ()), "iii"),
            (f(e_set_bool as *const ()), "ii"),
            (f(e_get_bool as *const ()), "ii"),
            (f(e_set_bool_array as *const ()), "iii"),
            (f(e_get_bool_array as *const ()), "iii"),
            (f(e_set_int as *const ()), "ii"),
            (f(e_get_int as *const ()), "ii"),
            (f(e_set_int_array as *const ()), "iii"),
            (f(e_get_int_array as *const ()), "iii"),
            (f(e_set_float as *const ()), "ii"),
            (f(e_get_float as *const ()), "ii"),
            (f(e_set_float_array as *const ()), "iii"),
            (f(e_get_float_array as *const ()), "iii"),
            (f(e_set_vector as *const ()), "ii"),
            (f(e_get_vector as *const ()), "ii"),
            (f(e_set_vector_array as *const ()), "iii"),
            (f(e_get_vector_array as *const ()), "iii"),
            (f(e_set_matrix as *const ()), "ii"),
            (f(e_get_matrix as *const ()), "ii"),
            (f(e_set_matrix_array as *const ()), "iii"),
            (f(e_get_matrix_array as *const ()), "iii"),
            (f(e_set_matrix_pointer_array as *const ()), "iii"),
            (f(e_get_matrix_pointer_array as *const ()), "iii"),
            (f(e_set_matrix_transpose as *const ()), "ii"),
            (f(e_get_matrix_transpose as *const ()), "ii"),
            (f(e_set_matrix_transpose_array as *const ()), "iii"),
            (f(e_get_matrix_transpose_array as *const ()), "iii"),
            (f(e_set_matrix_transpose_pointer_array as *const ()), "iii"),
            (f(e_get_matrix_transpose_pointer_array as *const ()), "iii"),
            (f(e_set_string as *const ()), "ii"),
            (f(e_get_string as *const ()), "ii"),
            (f(e_set_texture as *const ()), "ii"),
            (f(e_get_texture as *const ()), "ii"),
            (f(e_get_pixel_shader as *const ()), "ii"),
            (f(e_get_vertex_shader as *const ()), "ii"),
            (f(e_set_array_range as *const ()), "iii"),
            (f(e_get_pool as *const ()), "i"),
            (f(e_set_technique as *const ()), "i"),
            (f(e_get_current_technique as *const ()), ""),
            (f(e_validate_technique as *const ()), "i"),
            (f(e_find_next_valid_technique as *const ()), "ii"),
            (f(e_is_parameter_used as *const ()), "ii"),
            (f(e_begin as *const ()), "ii"),
            (f(e_begin_pass as *const ()), "i"),
            (f(e_commit_changes as *const ()), ""),
            (f(e_end_pass as *const ()), ""),
            (f(e_end as *const ()), ""),
            (f(e_get_device as *const ()), "i"),
            (f(e_on_lost_device as *const ()), ""),
            (f(e_on_reset_device as *const ()), ""),
            (f(e_set_state_manager as *const ()), "i"),
            (f(e_get_state_manager as *const ()), "i"),
            (f(e_begin_parameter_block as *const ()), ""),
            (f(e_end_parameter_block as *const ()), ""),
            (f(e_apply_parameter_block as *const ()), "i"),
            (f(e_delete_parameter_block as *const ()), "i"),
            (f(e_clone_effect as *const ()), "ii"),
            (f(e_set_raw_value as *const ()), "iiii"),
        ],
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Création
// ═══════════════════════════════════════════════════════════════════════

unsafe fn create(
    dev: usize,
    src: &[u8],
    dir: Option<std::path::PathBuf>,
    defines: usize,
    flags: u32,
    effect: usize,
    errors: usize,
) -> i32 {
    put_ptr(effect, 0);
    put_ptr(errors, 0);
    if dev == 0 || effect == 0 {
        return D3DERR_INVALIDCALL;
    }
    match load(
        dev,
        src,
        super::d3dx9_shader::read_defines(defines),
        dir,
        flags,
    ) {
        Ok(e) => {
            put_ptr(effect, e);
            S_OK
        }
        Err(m) => {
            crate::log_warn!("d3dx9", "D3DXCreateEffect : {m}");
            super::d3dx9::error_buffer(errors, &format!("effect{m}\n"));
            D3DXERR_INVALIDDATA
        }
    }
}

unsafe fn from_path(
    dev: usize,
    p: Option<std::path::PathBuf>,
    defines: usize,
    flags: u32,
    effect: usize,
    errors: usize,
) -> i32 {
    let Some(p) = p else {
        return D3DERR_INVALIDCALL;
    };
    match std::fs::read(&p) {
        Ok(d) => {
            let dir = p.parent().map(|x| x.to_path_buf());
            create(dev, &d, dir, defines, flags, effect, errors)
        }
        Err(_) => {
            crate::log_warn!("d3dx9", "effet introuvable : {}", p.display());
            put_ptr(effect, 0);
            0x8007_0002u32 as i32
        }
    }
}

/// D3DXCreateEffect(pDevice, pSrcData, SrcDataLen, pDefines, pInclude,
/// Flags, pPool, ppEffect, ppCompilationErrors).
pub unsafe extern "win64" fn create_effect(
    dev: usize,
    src: usize,
    len: u32,
    defines: usize,
    _include: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    if src == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(src as *const u8, len as usize);
    create(dev, d, None, defines, flags, effect, errors)
}
/// D3DXCreateEffectEx(…, pSkipConstants, Flags, pPool, ppEffect, ppErrors).
pub unsafe extern "win64" fn create_effect_ex(
    dev: usize,
    src: usize,
    len: u32,
    defines: usize,
    _include: usize,
    _skip: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    if src == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(src as *const u8, len as usize);
    create(dev, d, None, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_file_a(
    dev: usize,
    path: *const u8,
    defines: usize,
    _include: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::cstr(path)));
    from_path(dev, p, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_file_w(
    dev: usize,
    path: *const u16,
    defines: usize,
    _include: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::wstr(path)));
    from_path(dev, p, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_file_ex_a(
    dev: usize,
    path: *const u8,
    defines: usize,
    _include: usize,
    _skip: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::cstr(path)));
    from_path(dev, p, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_file_ex_w(
    dev: usize,
    path: *const u16,
    defines: usize,
    _include: usize,
    _skip: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    let p = (!path.is_null()).then(|| crate::paths::to_linux(&super::wstr(path)));
    from_path(dev, p, defines, flags, effect, errors)
}
unsafe fn from_resource(
    dev: usize,
    module: usize,
    name: usize,
    wide: bool,
    defines: usize,
    flags: u32,
    effect: usize,
    errors: usize,
) -> i32 {
    match super::d3dx9_shader::resource(module, name, wide) {
        Some(d) => create(dev, &d, None, defines, flags, effect, errors),
        None => {
            put_ptr(effect, 0);
            D3DXERR_INVALIDDATA
        }
    }
}
pub unsafe extern "win64" fn create_effect_from_resource_a(
    dev: usize,
    module: usize,
    name: usize,
    defines: usize,
    _include: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    from_resource(dev, module, name, false, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_resource_w(
    dev: usize,
    module: usize,
    name: usize,
    defines: usize,
    _include: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    from_resource(dev, module, name, true, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_resource_ex_a(
    dev: usize,
    module: usize,
    name: usize,
    defines: usize,
    _include: usize,
    _skip: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    from_resource(dev, module, name, false, defines, flags, effect, errors)
}
pub unsafe extern "win64" fn create_effect_from_resource_ex_w(
    dev: usize,
    module: usize,
    name: usize,
    defines: usize,
    _include: usize,
    _skip: usize,
    flags: u32,
    _pool: usize,
    effect: usize,
    errors: usize,
) -> i32 {
    from_resource(dev, module, name, true, defines, flags, effect, errors)
}

unsafe extern "win64" fn pool_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    pool_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn pool_addref(this: usize) -> u32 {
    (*(this as *mut Pool)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn pool_release(this: usize) -> u32 {
    let n = (*(this as *mut Pool)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Pool));
    }
    n
}
/// D3DXCreateEffectPool : les paramètres partagés ne le sont pas encore,
/// chaque effet garde ses valeurs.
pub unsafe extern "win64" fn create_effect_pool(out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let vt = vtable_sig(
        "ID3DXEffectPool",
        &[
            (f(pool_qi as *const ()), "ii"),
            (f(pool_addref as *const ()), ""),
            (f(pool_release as *const ()), ""),
        ],
    );
    put_ptr(
        out,
        Box::into_raw(Box::new(Pool {
            vtbl: vt,
            refs: AtomicU32::new(1),
        })) as usize,
    );
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DXCreateEffect" => fn create_effect,
        "D3DXCreateEffectEx" => fn create_effect_ex,
        "D3DXCreateEffectFromFileA" => fn create_effect_from_file_a,
        "D3DXCreateEffectFromFileW" => fn create_effect_from_file_w,
        "D3DXCreateEffectFromFileExA" => fn create_effect_from_file_ex_a,
        "D3DXCreateEffectFromFileExW" => fn create_effect_from_file_ex_w,
        "D3DXCreateEffectFromResourceA" => fn create_effect_from_resource_a,
        "D3DXCreateEffectFromResourceW" => fn create_effect_from_resource_w,
        "D3DXCreateEffectFromResourceExA" => fn create_effect_from_resource_ex_a,
        "D3DXCreateEffectFromResourceExW" => fn create_effect_from_resource_ex_w,
        "D3DXCreateEffectPool" => fn create_effect_pool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse_d_un_effet() {
        let src = "float4x4 WVP : WORLDVIEWPROJECTION;\n\
            float4 Color < string UIName = \"Couleur\"; > = {1, 0.5, 0.25, 1};\n\
            texture Tex;\n\
            sampler2D Samp = sampler_state { Texture = <Tex>; MinFilter = Linear; AddressU = Clamp; };\n\
            float4 VS(float4 p : POSITION) : POSITION { return mul(p, WVP); }\n\
            float4 PS(uniform bool tint) : COLOR { return tint ? Color * tex2D(Samp, float2(0.5, 0.5)) : Color; }\n\
            technique T0 < int Level = 2; > {\n\
              pass P0 { VertexShader = compile vs_2_0 VS(); PixelShader = compile ps_2_0 PS(true);\n\
                        AlphaBlendEnable = TRUE; SrcBlend = SrcAlpha; DestBlend = InvSrcAlpha; CullMode = None; }\n\
              pass P1 { VertexShader = asm { vs_1_1\n dcl_position v0\n m4x4 oPos, v0, c0\n };\n\
                        PixelShader = NULL; ZWriteEnable = false; }\n\
            }";
        let p = hlsl::parse(src, &[], &|_| None).unwrap();
        assert_eq!(p.techniques.len(), 1);
        assert_eq!(p.techniques[0].passes.len(), 2);
        let ps = &p.techniques[0].passes[0].states;
        let Expr::Compile(prof, func, args) = &ps[1].value else {
            panic!()
        };
        assert_eq!(
            (prof.as_str(), func.as_str(), args.len()),
            ("ps_2_0", "PS", 1)
        );
        let sh = compile_shader_expr(&p, &ps[1].value, true, 0)
            .unwrap()
            .unwrap();
        assert!(sh.ctab.iter().any(|c| c.name == "Color"));
        assert!(sh.ctab.iter().any(|c| c.name == "Samp" && c.set == 3));
        let asm = compile_shader_expr(&p, &p.techniques[0].passes[1].states[0].value, false, 0)
            .unwrap()
            .unwrap();
        assert_eq!(asm.tokens[0], 0xFFFE_0101);
        let col = p.globals.iter().find(|g| g.name == "Color").unwrap();
        assert_eq!(col.annotations.len(), 1);
        assert_eq!(
            eval_const(col.init.as_ref().unwrap(), &[], &[]).unwrap(),
            vec![1.0, 0.5, 0.25, 1.0]
        );
        assert_eq!(enum_value(Cat::Blend, "InvSrcAlpha"), Some(6));
        assert_eq!(enum_value(Cat::Cull, "None"), Some(1));
        assert_eq!(enum_value(Cat::Filter, "None"), Some(0));
    }
}
