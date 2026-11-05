//! Traduction des shaders Direct3D 10/11 compilés (DXBC, modèles 4 et 5)
//! en GLSL 4.30, pour la couche D3D11 → OpenGL (#137).
//!
//! Écrit d'après la description publique du format à jetons (en-tête
//! `d3d11TokenizedProgramFormat.hpp` du kit de pilotes) et des signatures
//! `ISGN`/`OSGN`.
//!
//! Choix de traduction :
//! - registres sans type, comme dans DXBC : chaque registre est un `vec4`,
//!   les opérations entières passent par `floatBitsToInt`/`intBitsToFloat` ;
//! - tampons de constantes `cbN` : blocs `std140` de `vec4`, liaison
//!   `binding = étage * 16 + N` ;
//! - textures `tN` : `sampler*` combinés, unité `étage * 16 + N` ; l'état
//!   d'échantillonnage du `sM` utilisé avec elle est noté dans
//!   [`Shader::sampler_pairs`] pour être lié sur la même unité ;
//! - entrées du vertex shader : `location = registre` (la disposition
//!   d'entrée y place chaque sémantique) ;
//! - varyings : une `location` par sémantique (nom + index), attribuée par
//!   une table commune à tous les shaders du processus, ce qui relie un
//!   vertex shader et un pixel shader comme le fait Direct3D ;
//! - le rendu est retourné verticalement (`gl_Position.y = -y`) et la
//!   profondeur passe de [0, 1] à [-1, 1] : la ligne 0 d'une cible est le
//!   haut de l'image, comme en Direct3D ; la présentation retourne l'image.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Pixel,
    Vertex,
    Geometry,
    Compute,
}

impl Stage {
    /// Base des liaisons (tampons, unités de texture) de l'étage.
    pub(crate) fn binding_base(self) -> u32 {
        match self {
            Stage::Vertex => 0,
            Stage::Pixel => 16,
            Stage::Geometry => 32,
            Stage::Compute => 48,
        }
    }
}

/// Élément d'une signature d'entrée ou de sortie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SigElement {
    pub name: String,
    pub index: u32,
    /// D3D_NAME (0 : quelconque, 1 : SV_Position, 6 : SV_VertexID…).
    pub system_value: u32,
    /// 1 uint, 2 int, 3 float.
    pub component_type: u32,
    pub register: u32,
    pub mask: u8,
}

/// Shader traduit.
pub(crate) struct Shader {
    pub stage: Stage,
    pub glsl: String,
    pub inputs: Vec<SigElement>,
    #[allow(dead_code)] // signature de sortie, gardée pour le diagnostic
    pub outputs: Vec<SigElement>,
    /// (registre de texture, registre d'échantillonneur) utilisés ensemble.
    pub sampler_pairs: Vec<(u32, u32)>,
}

// ============================================================
//  Conteneur DXBC
// ============================================================

fn rd(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// Chunks du conteneur : (fourcc, données).
pub(crate) fn chunks(blob: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    if blob.len() < 32 || &blob[..4] != b"DXBC" {
        return out;
    }
    let n = rd(blob, 28).unwrap_or(0) as usize;
    for i in 0..n.min(64) {
        let Some(off) = rd(blob, 32 + i * 4).map(|o| o as usize) else {
            break;
        };
        let (Some(tag), Some(len)) = (blob.get(off..off + 4), rd(blob, off + 4)) else {
            continue;
        };
        if let Some(data) = blob.get(off + 8..off + 8 + len as usize) {
            out.push(([tag[0], tag[1], tag[2], tag[3]], data));
        }
    }
    out
}

/// Signature `ISGN`/`OSGN` (éléments de 24 octets) ou `OSG5`/`ISG1`.
pub(crate) fn parse_signature(data: &[u8], tag: &[u8; 4]) -> Vec<SigElement> {
    let mut out = Vec::new();
    let Some(count) = rd(data, 0) else {
        return out;
    };
    let (stride, skip) = match tag {
        b"OSG5" => (28, 4),
        b"ISG1" | b"OSG1" | b"PSG1" => (32, 4),
        _ => (24, 0),
    };
    for i in 0..count.min(64) as usize {
        let e = 8 + i * stride + skip;
        let (Some(name_off), Some(index), Some(sv), Some(ct), Some(reg)) = (
            rd(data, e),
            rd(data, e + 4),
            rd(data, e + 8),
            rd(data, e + 12),
            rd(data, e + 16),
        ) else {
            break;
        };
        let mask = data.get(e + 20).copied().unwrap_or(0);
        let name: Vec<u8> = data
            .get(name_off as usize..)
            .unwrap_or(&[])
            .iter()
            .take_while(|&&c| c != 0)
            .copied()
            .collect();
        out.push(SigElement {
            name: String::from_utf8_lossy(&name).to_ascii_uppercase(),
            index,
            system_value: sv,
            component_type: ct,
            register: reg,
            mask,
        });
    }
    out
}

/// Location d'un varying pour une sémantique (même numéro dans tous les
/// shaders du processus).
pub(crate) fn varying_location(name: &str, index: u32) -> u32 {
    static TABLE: Mutex<BTreeMap<(String, u32), u32>> = Mutex::new(BTreeMap::new());
    let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    let next = t.len() as u32;
    *t.entry((name.to_string(), index)).or_insert(next)
}

// ============================================================
//  Jetons
// ============================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    F,
    I,
    U,
}

#[derive(Clone, Debug)]
enum Index {
    Imm(u32),
    /// Registre relatif (+ décalage).
    Rel(Box<Operand>, u32),
}

#[derive(Clone, Debug)]
struct Operand {
    kind: u32,
    /// Composantes lues dans l'ordre (sélection, swizzle) ou écrites (masque).
    comps: Vec<u8>,
    /// Vrai pour un masque d'écriture (destination).
    #[allow(dead_code)]
    is_mask: bool,
    indices: Vec<Index>,
    imm: Vec<u32>,
    neg: bool,
    abs: bool,
}

const OP_TEMP: u32 = 0;
const OP_INPUT: u32 = 1;
const OP_OUTPUT: u32 = 2;
const OP_INDEXABLE_TEMP: u32 = 3;
const OP_IMM32: u32 = 4;
#[cfg_attr(not(test), allow(dead_code))]
const OP_SAMPLER: u32 = 6;
#[cfg_attr(not(test), allow(dead_code))]
const OP_RESOURCE: u32 = 7;
const OP_CB: u32 = 8;
const OP_ICB: u32 = 9;
const OP_PRIMITIVE_ID: u32 = 11;
const OP_OUTPUT_DEPTH: u32 = 12;
const OP_NULL: u32 = 13;

struct Reader<'a> {
    t: &'a [u32],
    p: usize,
}

impl Reader<'_> {
    fn next(&mut self) -> Result<u32, String> {
        let v = *self.t.get(self.p).ok_or("programme tronqué")?;
        self.p += 1;
        Ok(v)
    }

    fn operand(&mut self) -> Result<Operand, String> {
        let tok = self.next()?;
        let ncomp = match tok & 3 {
            0 => 0,
            1 => 1,
            2 => 4,
            _ => return Err("opérande à N composantes".into()),
        };
        let kind = (tok >> 12) & 0xFF;
        let mut comps = Vec::new();
        let mut is_mask = false;
        if ncomp == 4 {
            match (tok >> 2) & 3 {
                0 => {
                    is_mask = true;
                    let m = (tok >> 4) & 0xF;
                    comps = (0..4).filter(|c| m & (1 << c) != 0).collect();
                }
                1 => comps = (0..4).map(|c| ((tok >> (4 + 2 * c)) & 3) as u8).collect(),
                2 => comps = vec![((tok >> 4) & 3) as u8],
                _ => return Err("sélection de composantes inconnue".into()),
            }
        } else if ncomp == 1 {
            comps = vec![0];
        }
        let (mut neg, mut abs) = (false, false);
        let mut ext = tok & 0x8000_0000 != 0;
        while ext {
            let e = self.next()?;
            if e & 0x3F == 1 {
                match (e >> 6) & 0xFF {
                    1 => neg = true,
                    2 => abs = true,
                    3 => {
                        neg = true;
                        abs = true
                    }
                    _ => {}
                }
            }
            ext = e & 0x8000_0000 != 0;
        }
        let dims = ((tok >> 20) & 3) as usize;
        let mut indices = Vec::new();
        for d in 0..dims {
            let rep = (tok >> (22 + 3 * d)) & 7;
            indices.push(match rep {
                0 => Index::Imm(self.next()?),
                1 => {
                    let _hi = self.next()?;
                    Index::Imm(self.next()?)
                }
                2 => Index::Rel(Box::new(self.operand()?), 0),
                3 => {
                    let base = self.next()?;
                    Index::Rel(Box::new(self.operand()?), base)
                }
                _ => return Err("représentation d'index non prise en charge".into()),
            });
        }
        let mut imm = Vec::new();
        if kind == OP_IMM32 {
            for _ in 0..ncomp.max(1) {
                imm.push(self.next()?);
            }
        } else if kind == 5 {
            return Err("immédiat 64 bits non pris en charge".into());
        }
        Ok(Operand {
            kind,
            comps,
            is_mask,
            indices,
            imm,
            neg,
            abs,
        })
    }
}

fn idx(o: &Operand, n: usize) -> u32 {
    match o.indices.get(n) {
        Some(Index::Imm(v)) => *v,
        _ => 0,
    }
}

const XYZW: [char; 4] = ['x', 'y', 'z', 'w'];

fn swz(c: &[u8]) -> String {
    c.iter().map(|&i| XYZW[i as usize & 3]).collect()
}

fn vty(ty: Ty, n: usize) -> &'static str {
    match (ty, n) {
        (Ty::F, 1) => "float",
        (Ty::F, 2) => "vec2",
        (Ty::F, 3) => "vec3",
        (Ty::F, _) => "vec4",
        (Ty::I, 1) => "int",
        (Ty::I, 2) => "ivec2",
        (Ty::I, 3) => "ivec3",
        (Ty::I, _) => "ivec4",
        (Ty::U, 1) => "uint",
        (Ty::U, 2) => "uvec2",
        (Ty::U, 3) => "uvec3",
        (Ty::U, _) => "uvec4",
    }
}

fn float_lit(bits: u32) -> String {
    let f = f32::from_bits(bits);
    if f.is_finite() && (f == 0.0 || (f.abs() >= 1e-6 && f.abs() < 1e9)) && f.to_string().len() < 14
    {
        let s = format!("{f:?}");
        if s.contains('.') || s.contains('e') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        format!("uintBitsToFloat({bits}u)")
    }
}

// ============================================================
//  Génération
// ============================================================

#[derive(Default)]
struct Decls {
    temps: u32,
    indexable: BTreeMap<u32, (u32, u32)>,
    cbs: BTreeMap<u32, u32>,
    /// tN → dimension GLSL (« 2D », « 2DArray »…) et type rendu.
    resources: BTreeMap<u32, (&'static str, Ty)>,
    icb: Vec<u32>,
    inputs: BTreeMap<u32, u8>,
    outputs: BTreeMap<u32, u8>,
    /// Interpolation constante des entrées du pixel shader.
    flat_inputs: Vec<u32>,
    depth_out: bool,
    gs_in: Option<&'static str>,
    gs_out: Option<&'static str>,
    gs_max: u32,
    threads: (u32, u32, u32),
}

struct Gen {
    stage: Stage,
    body: String,
    depth: usize,
    d: Decls,
    pairs: Vec<(u32, u32)>,
    inputs: Vec<SigElement>,
    outputs: Vec<SigElement>,
}

impl Gen {
    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.body.push_str("  ");
        }
        self.body.push_str(s);
        self.body.push('\n');
    }

    /// Expression du registre (sans swizzle), en `vec4` flottant.
    fn reg(&self, o: &Operand) -> Result<String, String> {
        let i0 = idx(o, 0);
        Ok(match o.kind {
            OP_TEMP => format!("r{i0}"),
            OP_INPUT => match self.stage {
                Stage::Geometry if o.indices.len() == 2 => {
                    format!("vin[{}][{}]", self.index_expr(&o.indices[0])?, idx(o, 1))
                }
                _ => match &o.indices.first() {
                    Some(Index::Rel(..)) => format!("v[{}]", self.index_expr(&o.indices[0])?),
                    _ => format!("v{i0}"),
                },
            },
            OP_OUTPUT => format!("o{i0}"),
            OP_INDEXABLE_TEMP => format!("x{i0}[{}]", self.index_expr(&o.indices[1])?),
            OP_CB => format!("cb{i0}[{}]", self.index_expr(&o.indices[1])?),
            OP_ICB => format!("icb[{}]", self.index_expr(&o.indices[0])?),
            OP_OUTPUT_DEPTH | 38 | 39 => "odepth".into(),
            OP_PRIMITIVE_ID => "vec4(intBitsToFloat(gl_PrimitiveID))".into(),
            32 => "uintBitsToFloat(uvec4(gl_GlobalInvocationID, 0u))".into(),
            33 => "uintBitsToFloat(uvec4(gl_WorkGroupID, 0u))".into(),
            34 => "uintBitsToFloat(uvec4(gl_LocalInvocationID, 0u))".into(),
            36 => "vec4(uintBitsToFloat(gl_LocalInvocationIndex))".into(),
            37 => "vec4(intBitsToFloat(gl_InvocationID))".into(),
            k => return Err(format!("type d'opérande {k} non pris en charge")),
        })
    }

    fn index_expr(&self, i: &Index) -> Result<String, String> {
        Ok(match i {
            Index::Imm(v) => v.to_string(),
            Index::Rel(op, base) => {
                let r = self.reg(op)?;
                let c = XYZW[*op.comps.first().unwrap_or(&0) as usize & 3];
                if *base == 0 {
                    format!("floatBitsToInt({r}.{c})")
                } else {
                    format!("(floatBitsToInt({r}.{c}) + {base})")
                }
            }
        })
    }

    /// Source lue avec `n` composantes (choisies par `sel`), dans le type `ty`.
    fn src_sel(&self, o: &Operand, ty: Ty, sel: &[u8]) -> Result<String, String> {
        let n = sel.len();
        let pick = |c: u8| -> u8 {
            if o.comps.len() == 1 {
                o.comps[0]
            } else {
                *o.comps.get(c as usize).unwrap_or(&c)
            }
        };
        let comps: Vec<u8> = sel.iter().map(|&c| pick(c)).collect();
        let mut e = if o.kind == OP_IMM32 {
            let vals: Vec<String> = comps
                .iter()
                .map(|&c| {
                    let bits = *o.imm.get(c as usize).or(o.imm.first()).unwrap_or(&0);
                    match ty {
                        Ty::F => float_lit(bits),
                        Ty::I => format!("{}", bits as i32),
                        Ty::U => format!("{bits}u"),
                    }
                })
                .collect();
            if n == 1 {
                vals[0].clone()
            } else {
                format!("{}({})", vty(ty, n), vals.join(", "))
            }
        } else {
            let r = format!("{}.{}", self.reg(o)?, swz(&comps));
            match ty {
                Ty::F => r,
                Ty::I => format!("floatBitsToInt({r})"),
                Ty::U => format!("floatBitsToUint({r})"),
            }
        };
        if o.abs {
            e = format!("abs({e})");
        }
        if o.neg {
            e = format!("(-{e})");
        }
        Ok(e)
    }

    fn src(&self, o: &Operand, ty: Ty, dst: &Operand) -> Result<String, String> {
        self.src_sel(o, ty, &dst.comps)
    }

    fn assign(&mut self, dst: &Operand, ty: Ty, value: &str, sat: bool) -> Result<(), String> {
        if dst.kind == OP_NULL {
            return Ok(());
        }
        let n = dst.comps.len().max(1);
        let mut v = value.to_string();
        if sat && ty == Ty::F {
            v = format!("clamp({v}, 0.0, 1.0)");
        }
        let v = match ty {
            Ty::F => v,
            Ty::I => format!("intBitsToFloat({v})"),
            Ty::U => format!("uintBitsToFloat({v})"),
        };
        let target = if matches!(dst.kind, OP_OUTPUT_DEPTH | 38 | 39) {
            "odepth".to_string()
        } else {
            format!("{}.{}", self.reg(dst)?, swz(&dst.comps))
        };
        let _ = n;
        self.line(&format!("{target} = {v};"));
        Ok(())
    }

    /// Vecteur de composantes `n` de la destination (pour un résultat
    /// scalaire répété : produit scalaire…).
    fn splat(dst: &Operand, ty: Ty, scalar: &str) -> String {
        let n = dst.comps.len().max(1);
        if n == 1 {
            scalar.to_string()
        } else {
            format!("{}({scalar})", vty(ty, n))
        }
    }

    fn texture(&self, t: u32) -> String {
        format!("t{t}")
    }

    fn note_pair(&mut self, t: u32, s: u32) {
        if !self.pairs.contains(&(t, s)) {
            self.pairs.push((t, s));
        }
    }

    /// Coordonnées d'échantillonnage selon la dimension de la texture.
    fn coord_count(&self, t: u32) -> usize {
        match self.d.resources.get(&t).map(|r| r.0) {
            Some("1D") => 1,
            Some("1DArray") | Some("2D") | Some("2DMS") | Some("Buffer") => 2,
            Some("2DArray") | Some("3D") | Some("Cube") | Some("2DMSArray") => 3,
            Some("CubeArray") => 4,
            _ => 2,
        }
    }

    fn resource_result(&self, t: u32, res: &Operand, dst: &Operand, value: &str) -> (Ty, String) {
        let ty = self.d.resources.get(&t).map_or(Ty::F, |r| r.1);
        let comps: Vec<u8> = dst
            .comps
            .iter()
            .map(|&c| *res.comps.get(c as usize).unwrap_or(&c))
            .collect();
        (ty, format!("({value}).{}", swz(&comps)))
    }
}

/// Traduit un blob DXBC. Err : instruction ou forme non prise en charge.
pub(crate) fn translate(blob: &[u8]) -> Result<Shader, String> {
    // Conteneur du compilateur HLSL maison : GLSL déjà produit.
    if let Some(s) = crate::dlls::hlsl::from_container(blob) {
        return Ok(s);
    }
    let mut code = None;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (tag, data) in chunks(blob) {
        match &tag {
            b"SHDR" | b"SHEX" => code = Some(data),
            b"ISGN" | b"ISG1" => inputs = parse_signature(data, &tag),
            b"OSGN" | b"OSG5" | b"OSG1" => outputs = parse_signature(data, &tag),
            _ => {}
        }
    }
    let code = code.ok_or("chunk SHDR/SHEX absent")?;
    let tokens: Vec<u32> = code
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    if tokens.len() < 3 {
        return Err("programme vide (pas de code compilé)".into());
    }
    let stage = match tokens[0] >> 16 {
        0 => Stage::Pixel,
        1 => Stage::Vertex,
        2 => Stage::Geometry,
        5 => Stage::Compute,
        t => return Err(format!("étage {t} non pris en charge (tessellation)")),
    };
    let len = (tokens[1] as usize).min(tokens.len());
    let mut g = Gen {
        stage,
        body: String::new(),
        depth: 1,
        d: Decls::default(),
        pairs: Vec::new(),
        inputs,
        outputs,
    };
    let mut r = Reader {
        t: &tokens[..len],
        p: 2,
    };
    while r.p < len {
        let start = r.p;
        let tok = r.next()?;
        let op = tok & 0x7FF;
        let ilen = if op == 0x35 {
            // customdata : longueur dans le jeton suivant.
            let class = (tok >> 11) & 0x1FFFFF;
            let n = r.next()? as usize;
            if class == 3 {
                // Tampon de constantes immédiates.
                g.d.icb = tokens.get(start + 2..start + n).unwrap_or(&[]).to_vec();
            }
            r.p = start + n.max(2);
            continue;
        } else {
            ((tok >> 24) & 0x7F) as usize
        };
        if ilen == 0 {
            return Err(format!("instruction {op} de longueur nulle"));
        }
        let end = start + ilen;
        let mut ext = tok & 0x8000_0000 != 0;
        let mut res_dim_ext = None;
        while ext {
            let e = r.next()?;
            if e & 0x3F == 2 {
                res_dim_ext = Some((e >> 6) & 0x1F);
            }
            ext = e & 0x8000_0000 != 0;
        }
        let _ = res_dim_ext;
        instruction(&mut g, &mut r, tok, op, end)?;
        r.p = end;
    }
    Ok(finish(g))
}

fn resource_dim(d: u32) -> &'static str {
    match d {
        1 => "Buffer",
        2 => "1D",
        3 => "2D",
        4 => "2DMS",
        5 => "3D",
        6 => "Cube",
        7 => "1DArray",
        8 => "2DArray",
        9 => "2DMSArray",
        10 => "CubeArray",
        _ => "2D",
    }
}

fn instruction(g: &mut Gen, r: &mut Reader, tok: u32, op: u32, end: usize) -> Result<(), String> {
    let sat = tok & (1 << 13) != 0;
    let nz = tok & (1 << 18) != 0;
    let mut ops = Vec::new();
    let read_all = |r: &mut Reader, ops: &mut Vec<Operand>| -> Result<(), String> {
        while r.p < end {
            ops.push(r.operand()?);
        }
        Ok(())
    };
    // Instructions à N opérandes : (opcode, type, expression).
    macro_rules! bin {
        ($ty:expr, $f:expr) => {{
            read_all(r, &mut ops)?;
            let (d, a, b) = (&ops[0], &ops[1], &ops[2]);
            let (a, b) = (g.src(a, $ty, d)?, g.src(b, $ty, d)?);
            let v = $f(a, b, d);
            g.assign(&ops[0].clone(), $ty, &v, sat)?;
        }};
    }
    macro_rules! un {
        ($ty:expr, $f:expr) => {{
            read_all(r, &mut ops)?;
            let a = g.src(&ops[1], $ty, &ops[0])?;
            let v = $f(a, &ops[0]);
            g.assign(&ops[0].clone(), $ty, &v, sat)?;
        }};
    }
    macro_rules! cmp {
        ($ty:expr, $o:expr) => {{
            read_all(r, &mut ops)?;
            let d = &ops[0];
            let n = d.comps.len().max(1);
            let (a, b) = (g.src(&ops[1], $ty, d)?, g.src(&ops[2], $ty, d)?);
            let v = if n == 1 {
                format!("(({a}) {} ({b}) ? 0xFFFFFFFFu : 0u)", $o)
            } else {
                let f = match $o {
                    "<" => "lessThan",
                    ">=" => "greaterThanEqual",
                    "==" => "equal",
                    _ => "notEqual",
                };
                format!(
                    "{}(0xFFFFFFFFu) * {}({f}({a}, {b}))",
                    vty(Ty::U, n),
                    vty(Ty::U, n)
                )
            };
            g.assign(&ops[0].clone(), Ty::U, &v, false)?;
        }};
    }
    let cond = |g: &Gen, o: &Operand| -> Result<String, String> {
        let c = g.src_sel(o, Ty::U, &[0])?;
        Ok(if nz {
            format!("({c}) != 0u")
        } else {
            format!("({c}) == 0u")
        })
    };
    match op {
        // ---------- Déclarations ----------
        88 => {
            // dcl_resource tN, dimension, type de retour.
            let o = r.operand()?;
            let ret = r.next().unwrap_or(0x5555);
            let dim = resource_dim((tok >> 11) & 0x1F);
            let ty = match ret & 0xF {
                3 => Ty::I,
                4 => Ty::U,
                _ => Ty::F,
            };
            g.d.resources.insert(idx(&o, 0), (dim, ty));
        }
        161 | 162 => {
            let o = r.operand()?;
            g.d.resources.insert(idx(&o, 0), ("Buffer", Ty::U));
            return Err(
                "ressources brutes ou structurées (StructuredBuffer) non prises en charge".into(),
            );
        }
        89 => {
            let o = r.operand()?;
            let n = idx(&o, 1).max(1);
            g.d.cbs.insert(idx(&o, 0), n);
        }
        90 | 106 | 91 => {}
        104 => g.d.temps = r.next()?,
        105 => {
            let reg = r.next()?;
            let size = r.next()?;
            let comps = r.next()?;
            g.d.indexable.insert(reg, (size, comps));
        }
        95..=100 => {
            let o = r.operand()?;
            let m: u8 = o.comps.iter().fold(0, |a, &c| a | (1 << c));
            let reg = if g.stage == Stage::Geometry {
                idx(&o, 1)
            } else {
                idx(&o, 0)
            };
            *g.d.inputs.entry(reg).or_default() |= m;
            if (98..=100).contains(&op) && (tok >> 11) & 0xF == 1 {
                g.d.flat_inputs.push(reg);
            }
        }
        101..=103 => {
            let o = r.operand()?;
            if o.kind == OP_OUTPUT_DEPTH || o.kind == 38 || o.kind == 39 {
                g.d.depth_out = true;
            } else if o.kind == OP_OUTPUT {
                let m: u8 = o.comps.iter().fold(0, |a, &c| a | (1 << c));
                *g.d.outputs.entry(idx(&o, 0)).or_default() |= m;
            }
        }
        92 => {
            g.d.gs_out = Some(match (tok >> 11) & 0x3F {
                1 => "points",
                2 | 3 => "line_strip",
                _ => "triangle_strip",
            })
        }
        93 => {
            g.d.gs_in = Some(match (tok >> 11) & 0x3F {
                1 => "points",
                2 => "lines",
                6 => "lines_adjacency",
                7 => "triangles_adjacency",
                _ => "triangles",
            })
        }
        94 => g.d.gs_max = r.next()?,
        155 => g.d.threads = (r.next()?, r.next()?, r.next()?),
        143 => {}
        // ---------- Arithmétique flottante ----------
        0 => bin!(Ty::F, |a, b, _| format!("{a} + {b}")),
        56 => bin!(Ty::F, |a, b, _| format!("{a} * {b}")),
        14 => bin!(Ty::F, |a, b, _| format!("{a} / {b}")),
        51 => bin!(Ty::F, |a, b, _| format!("min({a}, {b})")),
        52 => bin!(Ty::F, |a, b, _| format!("max({a}, {b})")),
        50 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let (a, b, c) = (
                g.src(&ops[1], Ty::F, &d)?,
                g.src(&ops[2], Ty::F, &d)?,
                g.src(&ops[3], Ty::F, &d)?,
            );
            g.assign(&d, Ty::F, &format!("{a} * {b} + {c}"), sat)?;
        }
        15..=17 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let n = (op - 13) as usize;
            let sel: Vec<u8> = (0..n as u8).collect();
            let (a, b) = (
                g.src_sel(&ops[1], Ty::F, &sel)?,
                g.src_sel(&ops[2], Ty::F, &sel)?,
            );
            let v = Gen::splat(&d, Ty::F, &format!("dot({a}, {b})"));
            g.assign(&d, Ty::F, &v, sat)?;
        }
        54 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::F, &d)?;
            g.assign(&d, Ty::F, &a, sat)?;
        }
        55 => {
            // movc : choix composante par composante.
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let c = g.src(&ops[1], Ty::U, &d)?;
            let (a, b) = (g.src(&ops[2], Ty::F, &d)?, g.src(&ops[3], Ty::F, &d)?);
            let n = d.comps.len().max(1);
            let v = if n == 1 {
                format!("({c}) != 0u ? {a} : {b}")
            } else {
                format!("mix({b}, {a}, notEqual({c}, {}(0u)))", vty(Ty::U, n))
            };
            g.assign(&d, Ty::F, &v, sat)?;
        }
        25 => un!(Ty::F, |a, _| format!("exp2({a})")),
        47 => un!(Ty::F, |a, _| format!("log2({a})")),
        26 => un!(Ty::F, |a, _| format!("fract({a})")),
        68 => un!(Ty::F, |a, _| format!("inversesqrt({a})")),
        75 => un!(Ty::F, |a, _| format!("sqrt({a})")),
        129 => un!(Ty::F, |a, _| format!("(1.0 / {a})")),
        64 => un!(Ty::F, |a, _| format!("roundEven({a})")),
        65 => un!(Ty::F, |a, _| format!("floor({a})")),
        66 => un!(Ty::F, |a, _| format!("ceil({a})")),
        67 => un!(Ty::F, |a, _| format!("trunc({a})")),
        11 | 122 | 123 => un!(Ty::F, |a, _| format!("dFdx({a})")),
        12 | 124 | 125 => un!(Ty::F, |a, _| format!("dFdy({a})")),
        77 => {
            read_all(r, &mut ops)?;
            let (ds, dc) = (ops[0].clone(), ops[1].clone());
            if ds.kind != OP_NULL {
                let a = g.src(&ops[2], Ty::F, &ds)?;
                g.assign(&ds, Ty::F, &format!("sin({a})"), sat)?;
            }
            if dc.kind != OP_NULL {
                let a = g.src(&ops[2], Ty::F, &dc)?;
                g.assign(&dc, Ty::F, &format!("cos({a})"), sat)?;
            }
        }
        24 => cmp!(Ty::F, "=="),
        57 => cmp!(Ty::F, "!="),
        49 => cmp!(Ty::F, "<"),
        29 => cmp!(Ty::F, ">="),
        32 => cmp!(Ty::I, "=="),
        39 => cmp!(Ty::I, "!="),
        34 => cmp!(Ty::I, "<"),
        33 => cmp!(Ty::I, ">="),
        79 => cmp!(Ty::U, "<"),
        80 => cmp!(Ty::U, ">="),
        // ---------- Entiers ----------
        30 => bin!(Ty::I, |a, b, _| format!("{a} + {b}")),
        38 => {
            read_all(r, &mut ops)?;
            let d = ops[1].clone();
            let (a, b) = (g.src(&ops[2], Ty::I, &d)?, g.src(&ops[3], Ty::I, &d)?);
            g.assign(&d, Ty::I, &format!("{a} * {b}"), false)?;
        }
        81 => {
            read_all(r, &mut ops)?;
            let d = ops[1].clone();
            let (a, b) = (g.src(&ops[2], Ty::U, &d)?, g.src(&ops[3], Ty::U, &d)?);
            g.assign(&d, Ty::U, &format!("{a} * {b}"), false)?;
        }
        35 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let (a, b, c) = (
                g.src(&ops[1], Ty::I, &d)?,
                g.src(&ops[2], Ty::I, &d)?,
                g.src(&ops[3], Ty::I, &d)?,
            );
            g.assign(&d, Ty::I, &format!("{a} * {b} + {c}"), false)?;
        }
        82 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let (a, b, c) = (
                g.src(&ops[1], Ty::U, &d)?,
                g.src(&ops[2], Ty::U, &d)?,
                g.src(&ops[3], Ty::U, &d)?,
            );
            g.assign(&d, Ty::U, &format!("{a} * {b} + {c}"), false)?;
        }
        78 => {
            read_all(r, &mut ops)?;
            let (q, rem) = (ops[0].clone(), ops[1].clone());
            if q.kind != OP_NULL {
                let (a, b) = (g.src(&ops[2], Ty::U, &q)?, g.src(&ops[3], Ty::U, &q)?);
                g.assign(&q, Ty::U, &format!("{a} / {b}"), false)?;
            }
            if rem.kind != OP_NULL {
                let (a, b) = (g.src(&ops[2], Ty::U, &rem)?, g.src(&ops[3], Ty::U, &rem)?);
                g.assign(&rem, Ty::U, &format!("{a} % {b}"), false)?;
            }
        }
        36 => bin!(Ty::I, |a, b, _| format!("max({a}, {b})")),
        37 => bin!(Ty::I, |a, b, _| format!("min({a}, {b})")),
        83 => bin!(Ty::U, |a, b, _| format!("max({a}, {b})")),
        84 => bin!(Ty::U, |a, b, _| format!("min({a}, {b})")),
        1 => bin!(Ty::U, |a, b, _| format!("{a} & {b}")),
        60 => bin!(Ty::U, |a, b, _| format!("{a} | {b}")),
        87 => bin!(Ty::U, |a, b, _| format!("{a} ^ {b}")),
        59 => un!(Ty::U, |a, _| format!("~{a}")),
        40 => un!(Ty::I, |a, _| format!("-{a}")),
        41 => bin!(Ty::I, |a, b, _| format!("{a} << ({b} & 31)")),
        42 => bin!(Ty::I, |a, b, _| format!("{a} >> ({b} & 31)")),
        85 => bin!(Ty::U, |a, b, _| format!("{a} >> ({b} & 31u)")),
        43 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::I, &d)?;
            g.assign(
                &d,
                Ty::F,
                &format!("{}({a})", vty(Ty::F, d.comps.len())),
                sat,
            )?;
        }
        86 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::U, &d)?;
            g.assign(
                &d,
                Ty::F,
                &format!("{}({a})", vty(Ty::F, d.comps.len())),
                sat,
            )?;
        }
        27 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::F, &d)?;
            g.assign(
                &d,
                Ty::I,
                &format!("{}({a})", vty(Ty::I, d.comps.len())),
                false,
            )?;
        }
        28 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::F, &d)?;
            g.assign(
                &d,
                Ty::U,
                &format!("{}(max({a}, 0.0))", vty(Ty::U, d.comps.len())),
                false,
            )?;
        }
        130 => un!(Ty::F, |a: String, d: &Operand| {
            if d.comps.len() <= 1 {
                format!("packHalf2x16(vec2({a}, 0.0))")
            } else {
                String::new()
            }
        }),
        131 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let a = g.src(&ops[1], Ty::U, &d)?;
            if d.comps.len() != 1 {
                return Err("f16tof32 vectoriel non pris en charge".into());
            }
            g.assign(&d, Ty::F, &format!("unpackHalf2x16({a}).x"), false)?;
        }
        134 => un!(Ty::U, |a, d: &Operand| format!(
            "{}(bitCount({a}))",
            vty(Ty::U, d.comps.len())
        )),
        141 => un!(Ty::U, |a, _| format!("bitfieldReverse({a})")),
        138 | 139 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let ty = if op == 138 { Ty::U } else { Ty::I };
            let (w, o, v) = (
                g.src(&ops[1], Ty::I, &d)?,
                g.src(&ops[2], Ty::I, &d)?,
                g.src(&ops[3], ty, &d)?,
            );
            if d.comps.len() != 1 {
                return Err("ubfe/ibfe vectoriel non pris en charge".into());
            }
            g.assign(
                &d,
                ty,
                &format!("bitfieldExtract({v}, {o} & 31, {w} & 31)"),
                false,
            )?;
        }
        140 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            if d.comps.len() != 1 {
                return Err("bfi vectoriel non pris en charge".into());
            }
            let (w, o, ins, base) = (
                g.src(&ops[1], Ty::I, &d)?,
                g.src(&ops[2], Ty::I, &d)?,
                g.src(&ops[3], Ty::U, &d)?,
                g.src(&ops[4], Ty::U, &d)?,
            );
            g.assign(
                &d,
                Ty::U,
                &format!("bitfieldInsert({base}, {ins}, {o} & 31, {w} & 31)"),
                false,
            )?;
        }
        // ---------- Textures ----------
        69..=74 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let (res, smp) = (ops[2].clone(), ops[3].clone());
            let t = idx(&res, 0);
            g.note_pair(t, idx(&smp, 0));
            let n = g.coord_count(t);
            let sel: Vec<u8> = (0..n as u8).collect();
            let coord = g.src_sel(&ops[1], Ty::F, &sel)?;
            let tex = g.texture(t);
            let value = match op {
                69 => format!("texture({tex}, {coord})"),
                72 => format!(
                    "textureLod({tex}, {coord}, {})",
                    g.src_sel(&ops[4], Ty::F, &[0])?
                ),
                74 => format!(
                    "texture({tex}, {coord}, {})",
                    g.src_sel(&ops[4], Ty::F, &[0])?
                ),
                73 => {
                    let dsel: Vec<u8> = (0..n.min(3) as u8).collect();
                    format!(
                        "textureGrad({tex}, {coord}, {}, {})",
                        g.src_sel(&ops[4], Ty::F, &dsel)?,
                        g.src_sel(&ops[5], Ty::F, &dsel)?
                    )
                }
                _ => {
                    // Comparaison (ombres) : sampler*Shadow.
                    let r = g.src_sel(&ops[4], Ty::F, &[0])?;
                    if let Some(e) = g.d.resources.get_mut(&t) {
                        if !e.0.ends_with("Shadow") {
                            e.0 = match e.0 {
                                "2DArray" => "2DArrayShadow",
                                "Cube" => "CubeShadow",
                                _ => "2DShadow",
                            };
                        }
                    }
                    let c = format!("vec{}({coord}, {r})", n + 1);
                    if op == 71 {
                        format!("vec4(textureLod({tex}, {c}, 0.0))")
                    } else {
                        format!("vec4(texture({tex}, {c}))")
                    }
                }
            };
            let (ty, v) = g.resource_result(t, &res, &d, &value);
            g.assign(&d, ty, &v, sat)?;
        }
        45 => {
            // ld : texelFetch(coordonnées entières, lod en .w).
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let res = ops[2].clone();
            let t = idx(&res, 0);
            let dim = g.d.resources.get(&t).map_or("2D", |r| r.0);
            let tex = g.texture(t);
            let value = if dim == "Buffer" {
                format!("texelFetch({tex}, {})", g.src_sel(&ops[1], Ty::I, &[0])?)
            } else {
                let n = g.coord_count(t);
                let sel: Vec<u8> = (0..n as u8).collect();
                format!(
                    "texelFetch({tex}, {}, {})",
                    g.src_sel(&ops[1], Ty::I, &sel)?,
                    g.src_sel(&ops[1], Ty::I, &[3])?
                )
            };
            let (ty, v) = g.resource_result(t, &res, &d, &value);
            g.assign(&d, ty, &v, sat)?;
        }
        61 => {
            // resinfo : largeur, hauteur, profondeur, nombre de niveaux.
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let res = ops[2].clone();
            let t = idx(&res, 0);
            let tex = g.texture(t);
            let lod = g.src_sel(&ops[1], Ty::I, &[0])?;
            let n = g.coord_count(t).min(3);
            let size = match n {
                1 => format!("ivec4(textureSize({tex}, {lod}), 0, 0, textureQueryLevels({tex}))"),
                2 => format!("ivec4(textureSize({tex}, {lod}), 0, textureQueryLevels({tex}))"),
                _ => format!("ivec4(textureSize({tex}, {lod}), textureQueryLevels({tex}))"),
            };
            let as_uint = (tok >> 11) & 3 == 2;
            let value = if as_uint {
                format!("uvec4({size})")
            } else {
                format!("vec4({size})")
            };
            let (_, v) = g.resource_result(t, &res, &d, &value);
            g.assign(&d, if as_uint { Ty::U } else { Ty::F }, &v, sat)?;
        }
        108 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let t = idx(&ops[2], 0);
            g.note_pair(t, idx(&ops[3], 0));
            let n = g.coord_count(t);
            let sel: Vec<u8> = (0..n as u8).collect();
            let v = format!(
                "vec4(textureQueryLod({}, {}), 0.0, 0.0)",
                g.texture(t),
                g.src_sel(&ops[1], Ty::F, &sel)?
            );
            let (_, v) = g.resource_result(t, &ops[2].clone(), &d, &v);
            g.assign(&d, Ty::F, &v, sat)?;
        }
        109 => {
            read_all(r, &mut ops)?;
            let d = ops[0].clone();
            let res = ops[2].clone();
            let t = idx(&res, 0);
            g.note_pair(t, idx(&ops[3], 0));
            let n = g.coord_count(t);
            let sel: Vec<u8> = (0..n as u8).collect();
            let comp = *ops[3].comps.first().unwrap_or(&0);
            let value = format!(
                "textureGather({}, {}, {comp})",
                g.texture(t),
                g.src_sel(&ops[1], Ty::F, &sel)?
            );
            let (ty, v) = g.resource_result(t, &res, &d, &value);
            g.assign(&d, ty, &v, sat)?;
        }
        // ---------- Contrôle ----------
        31 => {
            let o = r.operand()?;
            let c = cond(g, &o)?;
            g.line(&format!("if ({c}) {{"));
            g.depth += 1;
        }
        18 => {
            g.depth = g.depth.saturating_sub(1).max(1);
            g.line("} else {");
            g.depth += 1;
        }
        21..=23 => {
            g.depth = g.depth.saturating_sub(1).max(1);
            g.line("}");
        }
        48 => {
            g.line("while (true) {");
            g.depth += 1;
        }
        2 => g.line("break;"),
        7 => g.line("continue;"),
        3 | 8 => {
            let o = r.operand()?;
            let c = cond(g, &o)?;
            g.line(&format!(
                "if ({c}) {}",
                if op == 3 { "break;" } else { "continue;" }
            ));
        }
        76 => {
            let o = r.operand()?;
            let c = g.src_sel(&o, Ty::I, &[0])?;
            g.line(&format!("switch ({c}) {{"));
            g.depth += 1;
        }
        6 => {
            let o = r.operand()?;
            let v = *o.imm.first().unwrap_or(&0) as i32;
            g.line(&format!("case {v}:"));
        }
        10 => g.line("default:"),
        13 => {
            let o = r.operand()?;
            let c = cond(g, &o)?;
            g.line(&format!("if ({c}) discard;"));
        }
        62 => g.line("epilogue(); return;"),
        63 => {
            let o = r.operand()?;
            let c = cond(g, &o)?;
            g.line(&format!("if ({c}) {{ epilogue(); return; }}"));
        }
        58 => {}
        19 | 117 => g.line("epilogue(); EmitVertex();"),
        9 | 118 => g.line("EndPrimitive();"),
        20 | 119 => g.line("epilogue(); EmitVertex(); EndPrimitive();"),
        _ => return Err(format!("instruction DXBC {op} non prise en charge")),
    }
    Ok(())
}

fn sampler_type(dim: &str, ty: Ty) -> String {
    let prefix = match ty {
        Ty::F => "",
        Ty::I => "i",
        Ty::U => "u",
    };
    if dim == "Buffer" {
        return format!("{prefix}samplerBuffer");
    }
    format!("{prefix}sampler{dim}")
}

fn finish(g: Gen) -> Shader {
    let base = g.stage.binding_base();
    let mut s = String::new();
    s.push_str("#version 430 core\n");
    if g.stage == Stage::Geometry {
        let _ = writeln!(s, "layout({}) in;", g.d.gs_in.unwrap_or("triangles"));
        let _ = writeln!(
            s,
            "layout({}, max_vertices = {}) out;",
            g.d.gs_out.unwrap_or("triangle_strip"),
            g.d.gs_max.max(1)
        );
    }
    if g.stage == Stage::Compute {
        let (x, y, z) = g.d.threads;
        let _ = writeln!(
            s,
            "layout(local_size_x = {}, local_size_y = {}, local_size_z = {}) in;",
            x.max(1),
            y.max(1),
            z.max(1)
        );
    }
    let tag = match g.stage {
        Stage::Vertex => "vs",
        Stage::Pixel => "ps",
        Stage::Geometry => "gs",
        Stage::Compute => "cs",
    };
    for (&n, &size) in &g.d.cbs {
        let _ = writeln!(
            s,
            // Membre propre à l'étage : les blocs sans nom d'instance mettent
            // leurs membres en commun dans le programme, et le cb0 du vertex
            // shader n'est pas celui du pixel shader (taille différente).
            "layout(std140, binding = {}) uniform cb{n}_{tag} {{ vec4 cb{n}_{tag}_r[{}]; }};\n#define cb{n} cb{n}_{tag}_r",
            base + n,
            size.min(4096)
        );
    }
    for (&t, &(dim, ty)) in &g.d.resources {
        let _ = writeln!(
            s,
            "layout(binding = {}) uniform {} t{t};",
            base + t,
            sampler_type(dim, ty)
        );
    }
    if !g.d.icb.is_empty() {
        let vals: Vec<String> =
            g.d.icb
                .chunks(4)
                .map(|c| {
                    let v: Vec<String> = (0..4)
                        .map(|i| format!("{}u", c.get(i).copied().unwrap_or(0)))
                        .collect();
                    format!("uintBitsToFloat(uvec4({}))", v.join(", "))
                })
                .collect();
        let _ = writeln!(
            s,
            "const vec4 icb[{}] = vec4[]({});",
            vals.len(),
            vals.join(", ")
        );
    }

    // Entrées.
    let mut prologue = String::new();
    let max_in = g.d.inputs.keys().max().copied().map_or(0, |m| m + 1);
    match g.stage {
        Stage::Vertex => {
            for &reg in g.d.inputs.keys() {
                let sv = g
                    .inputs
                    .iter()
                    .find(|e| e.register == reg)
                    .map_or(0, |e| e.system_value);
                match sv {
                    6 => {
                        let _ = writeln!(prologue, "  v{reg} = vec4(intBitsToFloat(gl_VertexID));");
                    }
                    8 => {
                        let _ =
                            writeln!(prologue, "  v{reg} = vec4(intBitsToFloat(gl_InstanceID));");
                    }
                    _ => {
                        // Entier : lu par glVertexAttribIPointer, bits gardés.
                        let ct = g
                            .inputs
                            .iter()
                            .find(|e| e.register == reg)
                            .map_or(3, |e| e.component_type);
                        let (ty, conv) = match ct {
                            1 => ("uvec4", "uintBitsToFloat"),
                            2 => ("ivec4", "intBitsToFloat"),
                            _ => ("vec4", ""),
                        };
                        let _ = writeln!(s, "layout(location = {reg}) in {ty} a{reg};");
                        let _ = writeln!(prologue, "  v{reg} = {conv}(a{reg});");
                    }
                }
            }
        }
        Stage::Pixel | Stage::Geometry => {
            let mut declared = Vec::new();
            for e in &g.inputs {
                if !g.d.inputs.contains_key(&e.register) {
                    continue;
                }
                let comps: Vec<u8> = (0..4).filter(|c| e.mask & (1 << c) != 0).collect();
                if comps.is_empty() {
                    continue;
                }
                let dst = swz(&comps);
                let src: String = swz(&(0..comps.len() as u8).collect::<Vec<_>>());
                let v = if g.stage == Stage::Pixel {
                    format!("v{}", e.register)
                } else {
                    format!("vin[i][{}]", e.register)
                };
                match (g.stage, e.system_value) {
                    (Stage::Pixel, 1) => {
                        let _ = writeln!(prologue, "  {v}.{dst} = gl_FragCoord.{dst};");
                    }
                    (Stage::Pixel, 9) => {
                        let _ = writeln!(prologue, "  {v}.{dst} = vec4(gl_FrontFacing ? uintBitsToFloat(0xFFFFFFFFu) : 0.0).{dst};");
                    }
                    (Stage::Pixel, 7) => {
                        let _ = writeln!(
                            prologue,
                            "  {v}.{dst} = vec4(intBitsToFloat(gl_PrimitiveID)).{dst};"
                        );
                    }
                    (Stage::Geometry, 1) => {
                        let _ = writeln!(prologue, "  for (int i = 0; i < gl_in.length(); i++) {v}.{dst} = gl_in[i].gl_Position.{dst};");
                    }
                    _ => {
                        let loc = varying_location(&e.name, e.index);
                        let flat = g.d.flat_inputs.contains(&e.register) || e.component_type != 3;
                        if !declared.contains(&loc) {
                            declared.push(loc);
                            let arr = if g.stage == Stage::Geometry { "[]" } else { "" };
                            let _ = writeln!(
                                s,
                                "layout(location = {loc}) {}in vec4 vary{loc}{arr};",
                                if flat { "flat " } else { "" }
                            );
                        }
                        if g.stage == Stage::Geometry {
                            let _ = writeln!(prologue, "  for (int i = 0; i < gl_in.length(); i++) {v}.{dst} = vary{loc}[i].{src};");
                        } else {
                            let _ = writeln!(prologue, "  {v}.{dst} = vary{loc}.{src};");
                        }
                    }
                }
            }
        }
        Stage::Compute => {}
    }

    // Sorties.
    let mut epilogue = String::new();
    let max_out = g.d.outputs.keys().max().copied().map_or(0, |m| m + 1);
    match g.stage {
        Stage::Pixel => {
            for &reg in g.d.outputs.keys() {
                let _ = writeln!(s, "layout(location = {reg}) out vec4 target{reg};");
                let ty = g
                    .outputs
                    .iter()
                    .find(|e| e.register == reg)
                    .map_or(3, |e| e.component_type);
                let _ = writeln!(
                    epilogue,
                    "  target{reg} = {};",
                    match ty {
                        1 => format!("vec4(floatBitsToUint(o{reg}))"),
                        2 => format!("vec4(floatBitsToInt(o{reg}))"),
                        _ => format!("o{reg}"),
                    }
                );
            }
            if g.d.depth_out {
                epilogue.push_str("  gl_FragDepth = odepth;\n");
            }
        }
        Stage::Vertex | Stage::Geometry => {
            let mut declared = Vec::new();
            for e in &g.outputs {
                let comps: Vec<u8> = (0..4).filter(|c| e.mask & (1 << c) != 0).collect();
                if comps.is_empty() || !g.d.outputs.contains_key(&e.register) {
                    continue;
                }
                let src = swz(&comps);
                let dst = swz(&(0..comps.len() as u8).collect::<Vec<_>>());
                match e.system_value {
                    1 => {
                        // Direct3D → OpenGL : haut de l'image en ligne 0, profondeur [0, 1] → [-1, 1].
                        let _ = writeln!(
                            epilogue,
                            "  gl_Position = vec4(o{0}.x, -o{0}.y, o{0}.z * 2.0 - o{0}.w, o{0}.w);",
                            e.register
                        );
                    }
                    2 => {
                        let _ = writeln!(
                            epilogue,
                            "  gl_ClipDistance[{}] = o{}.{};",
                            e.index,
                            e.register,
                            &src[..1]
                        );
                    }
                    _ => {
                        let loc = varying_location(&e.name, e.index);
                        if !declared.contains(&loc) {
                            declared.push(loc);
                            let flat = e.component_type != 3;
                            let _ = writeln!(
                                s,
                                "layout(location = {loc}) {}out vec4 vary{loc};",
                                if flat { "flat " } else { "" }
                            );
                        }
                        let _ = writeln!(epilogue, "  vary{loc}.{dst} = o{}.{src};", e.register);
                    }
                }
            }
        }
        Stage::Compute => {}
    }

    // Registres.
    for i in 0..g.d.temps {
        let _ = writeln!(s, "vec4 r{i};");
    }
    for (&n, &(size, _)) in &g.d.indexable {
        let _ = writeln!(s, "vec4 x{n}[{}];", size.max(1));
    }
    if g.stage == Stage::Geometry {
        let _ = writeln!(s, "vec4 vin[gl_in.length()][{}];", max_in.max(1));
    } else {
        for i in 0..max_in {
            let _ = writeln!(s, "vec4 v{i} = vec4(0.0);");
        }
    }
    for i in 0..max_out {
        let _ = writeln!(s, "vec4 o{i} = vec4(0.0);");
    }
    if g.d.depth_out {
        s.push_str("float odepth = 0.0;\n");
    }
    let _ = write!(s, "void epilogue() {{\n{epilogue}}}\n");
    let _ = write!(s, "void main() {{\n{prologue}{}  epilogue();\n}}\n", g.body);
    Shader {
        stage: g.stage,
        glsl: s,
        inputs: g.inputs,
        outputs: g.outputs,
        sampler_pairs: g.pairs,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Petit assembleur de jetons pour les tests.
    pub(crate) struct Asm(pub Vec<u32>);

    pub(crate) const MASK_XYZW: u32 = 0xF;

    /// Opérande registre à 4 composantes avec masque.
    pub(crate) fn dst(kind: u32, reg: u32, mask: u32) -> Vec<u32> {
        vec![2 | (mask << 4) | (kind << 12) | (1 << 20), reg]
    }
    /// Opérande registre avec swizzle (0..3 par composante).
    pub(crate) fn src(kind: u32, reg: u32, sw: [u32; 4]) -> Vec<u32> {
        let s = sw[0] | (sw[1] << 2) | (sw[2] << 4) | (sw[3] << 6);
        vec![2 | (1 << 2) | (s << 4) | (kind << 12) | (1 << 20), reg]
    }
    pub(crate) fn cb(slot: u32, index: u32, sw: [u32; 4]) -> Vec<u32> {
        let s = sw[0] | (sw[1] << 2) | (sw[2] << 4) | (sw[3] << 6);
        vec![
            2 | (1 << 2) | (s << 4) | (OP_CB << 12) | (2 << 20),
            slot,
            index,
        ]
    }
    pub(crate) fn imm4(v: [f32; 4]) -> Vec<u32> {
        let mut o = vec![2 | (OP_IMM32 << 12)];
        o.extend(v.iter().map(|f| f.to_bits()));
        o
    }

    impl Asm {
        pub(crate) fn new(stage: u32) -> Asm {
            Asm(vec![(stage << 16) | 0x50, 0])
        }
        pub(crate) fn ins(&mut self, op: u32, operands: &[Vec<u32>]) -> &mut Asm {
            let body: Vec<u32> = operands.concat();
            self.0.push(op | ((1 + body.len() as u32) << 24));
            self.0.extend(body);
            self
        }
        pub(crate) fn raw(&mut self, op: u32, extra: &[u32]) -> &mut Asm {
            self.0.push(op | ((1 + extra.len() as u32) << 24));
            self.0.extend(extra);
            self
        }
        pub(crate) fn finish(&mut self) -> Vec<u32> {
            self.0[1] = self.0.len() as u32;
            self.0.clone()
        }
    }

    pub(crate) fn signature(elems: &[(&str, u32, u32, u32, u8)]) -> Vec<u8> {
        // (nom, index, valeur système, registre, masque) ; type float.
        let mut head = Vec::new();
        head.extend((elems.len() as u32).to_le_bytes());
        head.extend(8u32.to_le_bytes());
        let names_at = 8 + 24 * elems.len();
        let mut names = Vec::new();
        for (name, index, sv, reg, mask) in elems {
            head.extend(((names_at + names.len()) as u32).to_le_bytes());
            head.extend(index.to_le_bytes());
            head.extend(sv.to_le_bytes());
            head.extend(3u32.to_le_bytes());
            head.extend(reg.to_le_bytes());
            head.extend([*mask, *mask, 0, 0]);
            names.extend(name.as_bytes());
            names.push(0);
        }
        head.extend(names);
        head
    }

    pub(crate) fn container(parts: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let header = 32 + 4 * parts.len();
        let mut body = Vec::new();
        let mut offsets = Vec::new();
        for (tag, data) in parts {
            offsets.push((header + body.len()) as u32);
            body.extend(*tag);
            body.extend((data.len() as u32).to_le_bytes());
            body.extend(data);
        }
        let mut out = b"DXBC".to_vec();
        out.extend([0u8; 16]);
        out.extend(1u32.to_le_bytes());
        out.extend(((header + body.len()) as u32).to_le_bytes());
        out.extend((parts.len() as u32).to_le_bytes());
        for o in offsets {
            out.extend(o.to_le_bytes());
        }
        out.extend(body);
        out
    }

    fn words(t: &[u32]) -> Vec<u8> {
        t.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Vertex shader : o0 = mul(v0, cb0[0..3]) façon dp4, o1 = v1.
    pub(crate) fn sample_vs() -> Vec<u8> {
        let mut a = Asm::new(1);
        a.ins(89, &[cb(0, 4, [0, 1, 2, 3])]) // dcl_constantbuffer cb0[4]
            .ins(95, &[dst(OP_INPUT, 0, MASK_XYZW)])
            .ins(95, &[dst(OP_INPUT, 1, 0x3)])
            .ins(103, &[dst(OP_OUTPUT, 0, MASK_XYZW), vec![1]])
            .ins(101, &[dst(OP_OUTPUT, 1, 0x3)])
            .raw(104, &[1]);
        for i in 0..4 {
            a.ins(
                17,
                &[
                    dst(OP_OUTPUT, 0, 1 << i),
                    src(OP_INPUT, 0, [0, 1, 2, 3]),
                    cb(0, i, [0, 1, 2, 3]),
                ],
            );
        }
        a.ins(
            54,
            &[dst(OP_OUTPUT, 1, 0x3), src(OP_INPUT, 1, [0, 1, 1, 1])],
        )
        .raw(62, &[]);
        let code = a.finish();
        container(&[
            (
                b"ISGN",
                signature(&[("POSITION", 0, 0, 0, 0xF), ("TEXCOORD", 0, 0, 1, 0x3)]),
            ),
            (
                b"OSGN",
                signature(&[("SV_POSITION", 0, 1, 0, 0xF), ("TEXCOORD", 0, 0, 1, 0x3)]),
            ),
            (b"SHDR", words(&code)),
        ])
    }

    /// Pixel shader : o0 = t0.Sample(s0, v1.xy) * cb0[0].
    pub(crate) fn sample_ps() -> Vec<u8> {
        let mut a = Asm::new(0);
        let res = vec![
            2 | (1 << 2) | (0xE4 << 4) | (OP_RESOURCE << 12) | (1 << 20),
            0,
        ];
        let smp = vec![(OP_SAMPLER << 12) | (1 << 20), 0];
        // dcl_resource_texture2d (float,float,float,float) t0
        a.ins(89, &[cb(0, 1, [0, 1, 2, 3])])
            .raw(
                88 | (3 << 11),
                &[(OP_RESOURCE << 12) | (1 << 20), 0, 0x5555],
            )
            .ins(90, std::slice::from_ref(&smp))
            .ins(98 | (2 << 11), &[dst(OP_INPUT, 1, 0x3)])
            .ins(101, &[dst(OP_OUTPUT, 0, MASK_XYZW)])
            .raw(104, &[1])
            .ins(
                69,
                &[
                    dst(OP_TEMP, 0, MASK_XYZW),
                    src(OP_INPUT, 1, [0, 1, 1, 1]),
                    res,
                    smp,
                ],
            )
            .ins(
                56,
                &[
                    dst(OP_OUTPUT, 0, MASK_XYZW),
                    src(OP_TEMP, 0, [0, 1, 2, 3]),
                    cb(0, 0, [0, 1, 2, 3]),
                ],
            )
            .raw(62, &[]);
        let code = a.finish();
        container(&[
            (
                b"ISGN",
                signature(&[("SV_POSITION", 0, 1, 0, 0xF), ("TEXCOORD", 0, 0, 1, 0x3)]),
            ),
            (b"OSGN", signature(&[("SV_TARGET", 0, 64, 0, 0xF)])),
            (b"SHDR", words(&code)),
        ])
    }

    #[test]
    fn vertex_shader_traduit() {
        let s = translate(&sample_vs()).unwrap();
        assert_eq!(s.stage, Stage::Vertex);
        let g = &s.glsl;
        assert!(
            g.contains("layout(std140, binding = 0) uniform cb0_vs { vec4 cb0_vs_r[4]; };"),
            "{g}"
        );
        assert!(g.contains("layout(location = 0) in vec4 a0;"), "{g}");
        assert!(g.contains("o0.x = dot(v0.xyzw, cb0[0].xyzw);"), "{g}");
        assert!(
            g.contains("gl_Position = vec4(o0.x, -o0.y, o0.z * 2.0 - o0.w, o0.w);"),
            "{g}"
        );
        assert!(g.contains("o1.xy = v1.xy;"), "{g}");
        let loc = varying_location("TEXCOORD", 0);
        assert!(g.contains(&format!("out vec4 vary{loc};")), "{g}");
    }

    #[test]
    fn pixel_shader_traduit() {
        let s = translate(&sample_ps()).unwrap();
        assert_eq!(s.stage, Stage::Pixel);
        let g = &s.glsl;
        assert!(
            g.contains("layout(binding = 16) uniform sampler2D t0;"),
            "{g}"
        );
        assert!(g.contains("r0.xyzw = (texture(t0, v1.xy)).xyzw;"), "{g}");
        assert!(g.contains("o0.xyzw = r0.xyzw * cb0[0].xyzw;"), "{g}");
        assert!(g.contains("target0 = o0;"), "{g}");
        assert_eq!(s.sampler_pairs, vec![(0, 0)]);
        let loc = varying_location("TEXCOORD", 0);
        assert!(g.contains(&format!("v1.xy = vary{loc}.xy;")), "{g}");
    }

    #[test]
    fn immediats_et_controle() {
        let mut a = Asm::new(0);
        a.ins(101, &[dst(OP_OUTPUT, 0, MASK_XYZW)]).raw(104, &[1]);
        a.ins(
            54,
            &[dst(OP_TEMP, 0, MASK_XYZW), imm4([1.0, 0.5, 0.0, 2.0])],
        );
        // if_nz r0.x
        a.ins(
            31 | (1 << 18),
            &[vec![2 | (2 << 2) | (OP_TEMP << 12) | (1 << 20), 0]],
        );
        a.ins(
            54,
            &[dst(OP_OUTPUT, 0, MASK_XYZW), src(OP_TEMP, 0, [3, 2, 1, 0])],
        );
        a.raw(18, &[])
            .ins(54, &[dst(OP_OUTPUT, 0, MASK_XYZW), imm4([0.0; 4])])
            .raw(21, &[])
            .raw(62, &[]);
        let code = a.finish();
        let blob = container(&[
            (b"OSGN", signature(&[("SV_TARGET", 0, 64, 0, 0xF)])),
            (b"SHDR", words(&code)),
        ]);
        let g = translate(&blob).unwrap().glsl;
        assert!(g.contains("r0.xyzw = vec4(1.0, 0.5, 0.0, 2.0);"), "{g}");
        assert!(g.contains("if ((floatBitsToUint(r0.x)) != 0u) {"), "{g}");
        assert!(g.contains("o0.xyzw = r0.wzyx;"), "{g}");
        assert!(g.contains("} else {"), "{g}");
    }

    /// Compile le GLSL produit avec glslangValidator quand il est installé
    /// (paquet glslang-tools) : le texte doit être du GLSL 4.30 valide.
    #[test]
    fn glsl_valide_selon_glslang() {
        let Some(tool) = std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("glslangValidator"))
                .find(|f| f.is_file())
        }) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("winlux-dxbc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, blob) in [("vs.vert", sample_vs()), ("ps.frag", sample_ps())] {
            let glsl = translate(&blob).unwrap().glsl;
            let file = dir.join(name);
            std::fs::write(&file, &glsl).unwrap();
            let out = std::process::Command::new(&tool)
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{name}:\n{glsl}\n{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn programme_vide_refuse() {
        let blob = container(&[(b"SHEX", words(&[1 << 16]))]);
        assert!(translate(&blob).is_err());
    }
}
