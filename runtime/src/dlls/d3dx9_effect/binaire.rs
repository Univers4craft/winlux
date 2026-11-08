//! Effets déjà compilés (fx_2_0 binaire, fichiers .fxo) (#909).
//!
//! Disposition du format d'après MojoShader (mojoshader_effects.c, licence
//! zlib, écrit par Ryan C. Gordon et Ethan Lee pour FNA) :
//!
//! - mot de version 0xFEFF0901, puis le décalage de la table des comptes ;
//!   tous les décalages suivants partent de l'octet qui suit ce mot (`base`) ;
//! - comptes : paramètres, techniques, un mot réservé, objets ;
//! - paramètre : décalage du type, décalage de la valeur, drapeaux, nombre
//!   d'annotations, puis les annotations (type, valeur) ;
//! - type : type, classe, nom, sémantique, éléments, puis colonnes et lignes
//!   (numériques) ou la liste des membres (structures) ;
//! - technique : nom, annotations, passes ; passe : nom, annotations, états ;
//!   état : numéro, mot réservé, type, valeur ;
//! - objets « petits » (indice, longueur, octets alignés sur 4) puis
//!   « grands » (technique, passe ou paramètre, mot réservé, état, genre,
//!   longueur, octets) : chaînes, noms de texture et code des shaders.
//!
//! Les numéros d'états de passe suivent l'ordre de D3DRENDERSTATETYPE sans
//! ses trous, les huit WRAP8…15 rangés après WRAP7 ; 146 et 147 désignent
//! les shaders. Les états d'échantillonneur sont décalés de 4 (4 = Texture).
//! Les autres états (étages de texture, transformations, lumières) et les
//! préshaders ne sont pas évalués : ils sont signalés au journal.

use super::{cstring, Param, PassC, Shader, Tech, PT_PIXELSHADER, PT_SAMPLER, PT_STRING};
use super::{PT_BOOL, PT_FLOAT, PT_INT, PT_TEXTURE, PT_VERTEXSHADER};
use crate::dlls::d3dx9_hlsl::{CtabEntry, Expr, StateAssign};
use std::collections::HashMap;

pub(super) const MAGIC: u32 = 0xFEFF_0901;
/// En-tête ajouté par le compilateur d'effets de XNA 4.
const XNA_MAGIC: u32 = 0xBCF0_0BCF;

/// États de passe, dans l'ordre du format binaire ("" : non pris en charge).
const PASS_STATES: [&str; 103] = [
    "zenable",
    "fillmode",
    "shademode",
    "zwriteenable",
    "alphatestenable",
    "lastpixel",
    "srcblend",
    "destblend",
    "cullmode",
    "zfunc",
    "alpharef",
    "alphafunc",
    "ditherenable",
    "alphablendenable",
    "fogenable",
    "specularenable",
    "fogcolor",
    "fogtablemode",
    "fogstart",
    "fogend",
    "fogdensity",
    "rangefogenable",
    "stencilenable",
    "stencilfail",
    "stencilzfail",
    "stencilpass",
    "stencilfunc",
    "stencilref",
    "stencilmask",
    "stencilwritemask",
    "texturefactor",
    "wrap0",
    "wrap1",
    "wrap2",
    "wrap3",
    "wrap4",
    "wrap5",
    "wrap6",
    "wrap7",
    "wrap8",
    "wrap9",
    "wrap10",
    "wrap11",
    "wrap12",
    "wrap13",
    "wrap14",
    "wrap15",
    "clipping",
    "lighting",
    "ambient",
    "fogvertexmode",
    "colorvertex",
    "localviewer",
    "normalizenormals",
    "diffusematerialsource",
    "specularmaterialsource",
    "ambientmaterialsource",
    "emissivematerialsource",
    "vertexblend",
    "clipplaneenable",
    "pointsize",
    "pointsize_min",
    "pointspriteenable",
    "pointscaleenable",
    "pointscale_a",
    "pointscale_b",
    "pointscale_c",
    "multisampleantialias",
    "multisamplemask",
    "patchedgestyle",
    "debugmonitortoken",
    "pointsize_max",
    "indexedvertexblendenable",
    "colorwriteenable",
    "tweenfactor",
    "blendop",
    "positiondegree",
    "normaldegree",
    "scissortestenable",
    "slopescaledepthbias",
    "antialiasedlineenable",
    "mintessellationlevel",
    "maxtessellationlevel",
    "adaptivetess_x",
    "adaptivetess_y",
    "adaptivetess_z",
    "adaptivetess_w",
    "enableadaptivetessellation",
    "twosidedstencilmode",
    "ccw_stencilfail",
    "ccw_stencilzfail",
    "ccw_stencilpass",
    "ccw_stencilfunc",
    "colorwriteenable1",
    "colorwriteenable2",
    "colorwriteenable3",
    "blendfactor",
    "srgbwriteenable",
    "depthbias",
    "separatealphablendenable",
    "srcblendalpha",
    "destblendalpha",
    "blendopalpha",
];
const STATE_VERTEXSHADER: u32 = 146;
const STATE_PIXELSHADER: u32 = 147;

/// États d'échantillonneur à partir de 5 (4 = Texture).
const SAMPLER_STATES: [&str; 11] = [
    "addressu",
    "addressv",
    "addressw",
    "bordercolor",
    "magfilter",
    "minfilter",
    "mipfilter",
    "mipmaplodbias",
    "maxmiplevel",
    "maxanisotropy",
    "srgbtexture",
];
const SAMPLER_TEXTURE: u32 = 4;

/// Lecture bornée : toute sortie du fichier rend une erreur.
struct Rd<'a> {
    b: &'a [u8],
}

impl Rd<'_> {
    fn u32(&self, at: usize) -> Result<u32, String> {
        self.b
            .get(at..at.checked_add(4).ok_or("décalage invalide")?)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .ok_or_else(|| format!("effet binaire tronqué (décalage {at:#x})"))
    }
    fn bytes(&self, at: usize, n: usize) -> Result<&[u8], String> {
        self.b
            .get(at..at.checked_add(n).ok_or("longueur invalide")?)
            .ok_or_else(|| format!("effet binaire tronqué (décalage {at:#x})"))
    }
    /// Chaîne (longueur, octets) ; vide si la longueur est nulle.
    fn string(&self, at: usize) -> Result<String, String> {
        let n = self.u32(at)? as usize;
        if n == 0 {
            return Ok(String::new());
        }
        let s = self.bytes(at + 4, n)?;
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        Ok(String::from_utf8_lossy(&s[..end]).into_owned())
    }
}

/// Valeur lue (paramètre, annotation ou état).
#[derive(Default)]
struct Value {
    ty: u32,
    class: u32,
    name: String,
    semantic: String,
    elements: u32,
    rows: u32,
    cols: u32,
    data: Vec<f32>,
    /// Indices d'objets (chaînes, textures, shaders).
    objects: Vec<u32>,
    /// Échantillonneur : (état, valeur).
    sampler: Vec<(u32, Value)>,
}

fn is_sampler(ty: u32) -> bool {
    (PT_SAMPLER..PT_SAMPLER + 5).contains(&ty)
}

fn read_value(r: &Rd, ty_at: usize, val_at: usize, depth: u32) -> Result<Value, String> {
    if depth > 4 {
        return Err("effet binaire : échantillonneurs imbriqués".into());
    }
    let mut v = Value {
        ty: r.u32(ty_at)?,
        class: r.u32(ty_at + 4)?,
        name: r.string(r.u32(ty_at + 8)? as usize)?,
        semantic: r.string(r.u32(ty_at + 12)? as usize)?,
        elements: r.u32(ty_at + 16)?,
        ..Value::default()
    };
    let count = v.elements.max(1) as usize;
    match v.class {
        0..=3 => {
            v.cols = r.u32(ty_at + 20)?.min(4);
            v.rows = r.u32(ty_at + 24)?.min(4);
            let n = (v.rows * v.cols) as usize * count;
            if n > 1 << 20 {
                return Err("effet binaire : valeur trop grande".into());
            }
            for k in 0..n {
                let w = r.u32(val_at + 4 * k)?;
                v.data.push(match v.ty {
                    PT_FLOAT => f32::from_bits(w),
                    PT_INT => w as i32 as f32,
                    _ => (w != 0) as u32 as f32,
                });
            }
        }
        4 if is_sampler(v.ty) => {
            let n = r.u32(val_at)? as usize;
            for k in 0..n.min(64) {
                let at = val_at + 4 + 16 * k;
                let state = r.u32(at)? & !0xA0;
                let t = r.u32(at + 8)? as usize;
                let val = r.u32(at + 12)? as usize;
                v.sampler.push((state, read_value(r, t, val, depth + 1)?));
            }
        }
        4 => {
            for k in 0..count.min(4096) {
                v.objects.push(r.u32(val_at + 4 * k)?);
            }
        }
        5 => {
            // Structures : la valeur n'est pas reprise (cas rare des effets).
            crate::log_warn!("d3dx9", "effet binaire : structure {} non lue", v.name);
        }
        c => return Err(format!("effet binaire : classe {c} inconnue")),
    }
    Ok(v)
}

struct RawPass {
    name: String,
    annotations: Vec<Value>,
    states: Vec<(u32, Value)>,
}

struct RawTech {
    name: String,
    annotations: Vec<Value>,
    passes: Vec<RawPass>,
}

fn read_annotations(r: &Rd, at: &mut usize, n: u32) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for _ in 0..n.min(1024) {
        out.push(read_value(
            r,
            r.u32(*at)? as usize,
            r.u32(*at + 4)? as usize,
            0,
        )?);
        *at += 8;
    }
    Ok(out)
}

/// Effet binaire lu, avant sa conversion en paramètres et techniques.
struct Raw {
    params: Vec<(Value, Vec<Value>)>,
    techs: Vec<RawTech>,
    /// Indice d'objet → octets (chaîne, nom de texture ou code de shader).
    objects: HashMap<u32, Vec<u8>>,
}

fn read_raw(src: &[u8]) -> Result<Raw, String> {
    let mut start = 0usize;
    let first = Rd { b: src }.u32(0)?;
    if first == XNA_MAGIC {
        start = Rd { b: src }.u32(4)? as usize;
    }
    let all = Rd { b: src };
    if all.u32(start)? != MAGIC {
        return Err("ce n'est pas un effet fx_2_0".into());
    }
    let offset = all.u32(start + 4)? as usize;
    let r = Rd {
        b: src.get(start + 8..).ok_or("effet binaire tronqué")?,
    };
    let mut at = offset;
    let nparams = r.u32(at)?;
    let ntechs = r.u32(at + 4)?;
    at += 16;
    let mut params = Vec::new();
    for _ in 0..nparams.min(65536) {
        let ty = r.u32(at)? as usize;
        let val = r.u32(at + 4)? as usize;
        let nann = r.u32(at + 12)?;
        at += 16;
        let ann = read_annotations(&r, &mut at, nann)?;
        params.push((read_value(&r, ty, val, 0)?, ann));
    }
    let mut techs = Vec::new();
    for _ in 0..ntechs.min(4096) {
        let name = r.string(r.u32(at)? as usize)?;
        let nann = r.u32(at + 4)?;
        let npass = r.u32(at + 8)?;
        at += 12;
        let annotations = read_annotations(&r, &mut at, nann)?;
        let mut passes = Vec::new();
        for _ in 0..npass.min(256) {
            let name = r.string(r.u32(at)? as usize)?;
            let nann = r.u32(at + 4)?;
            let nstates = r.u32(at + 8)?;
            at += 12;
            let annotations = read_annotations(&r, &mut at, nann)?;
            let mut states = Vec::new();
            for _ in 0..nstates.min(1024) {
                let id = r.u32(at)?;
                let ty = r.u32(at + 8)? as usize;
                let val = r.u32(at + 12)? as usize;
                at += 16;
                states.push((id, read_value(&r, ty, val, 0)?));
            }
            passes.push(RawPass {
                name,
                annotations,
                states,
            });
        }
        techs.push(RawTech {
            name,
            annotations,
            passes,
        });
    }
    let nsmall = r.u32(at)?;
    let nlarge = r.u32(at + 4)?;
    at += 8;
    let block = |n: usize| (n + 3) & !3;
    let mut objects = HashMap::new();
    for _ in 0..nsmall.min(65536) {
        let index = r.u32(at)?;
        let n = r.u32(at + 4)? as usize;
        objects.insert(index, r.bytes(at + 8, n)?.to_vec());
        at += 8 + block(n);
    }
    for _ in 0..nlarge.min(65536) {
        let tech = r.u32(at)?;
        let index = r.u32(at + 4)? as usize;
        let state = r.u32(at + 12)? as usize;
        let kind = r.u32(at + 16)?;
        let n = r.u32(at + 20)? as usize;
        let data = r.bytes(at + 24, n)?.to_vec();
        at += 24 + block(n);
        let value = if tech == u32::MAX {
            params
                .get(index)
                .and_then(|p| p.0.sampler.get(state))
                .map(|s| &s.1)
        } else {
            techs
                .get(tech as usize)
                .and_then(|t| t.passes.get(index))
                .and_then(|p| p.states.get(state))
                .map(|s| &s.1)
        };
        let Some(&obj) = value.and_then(|v| v.objects.first()) else {
            continue;
        };
        if kind == 2 {
            // Préshader seul, sans shader : non évalué.
            crate::log_warn!("d3dx9", "effet binaire : préshader seul non évalué");
            continue;
        }
        objects.insert(obj, data);
    }
    Ok(Raw {
        params,
        techs,
        objects,
    })
}

/// Code de shader d'un objet (None : objet vide, shader NULL).
fn shader_of(objects: &HashMap<u32, Vec<u8>>, obj: u32) -> Option<Shader> {
    let b = objects.get(&obj)?;
    if b.len() < 8 {
        return None;
    }
    let tokens: Vec<u32> = b
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let ctab = crate::dlls::d3dx9_asm::constant_table(&tokens)
        .map(|t| {
            t.constants
                .iter()
                .map(|c| CtabEntry {
                    name: c.name.clone(),
                    set: c.set,
                    index: c.index,
                    count: c.count,
                    class: c.class,
                    ty: c.ty,
                    rows: c.rows,
                    cols: c.columns,
                    elements: c.elements,
                    default: c.default.as_ref().map(|d| {
                        d.chunks_exact(4)
                            .map(|w| f32::from_le_bytes(w.try_into().unwrap()))
                            .collect()
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    if tokens.iter().any(|&t| t == u32::from_le_bytes(*b"PRES")) {
        crate::log_warn!("d3dx9", "effet binaire : préshader non évalué");
    }
    Some(Shader {
        tokens,
        ctab,
        obj: 0,
    })
}

fn object_name(objects: &HashMap<u32, Vec<u8>>, obj: u32) -> Option<String> {
    let b = objects.get(&obj)?;
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    (end > 0).then(|| String::from_utf8_lossy(&b[..end]).into_owned())
}

/// Valeur numérique d'un état, telle qu'écrite dans le fichier.
fn state_expr(v: &Value) -> Expr {
    let x = v.data.first().copied().unwrap_or(0.0);
    match v.ty {
        // Les entiers gardent leurs 32 bits (couleurs) : relus bit à bit.
        PT_FLOAT => Expr::Num(x as f64, true),
        _ => Expr::Num(x as i64 as u32 as f64, false),
    }
}

fn new_param(v: &Value, flags: u32) -> Param {
    let numeric = matches!(v.ty, PT_BOOL | PT_INT | PT_FLOAT) && v.class <= 3;
    let mut data = v.data.clone();
    if numeric {
        data.resize((v.rows * v.cols) as usize * v.elements.max(1) as usize, 0.0);
    } else {
        data.clear();
    }
    Param {
        name: cstring(&v.name),
        semantic: if v.semantic.is_empty() {
            Vec::new()
        } else {
            cstring(&v.semantic)
        },
        class: v.class,
        ptype: v.ty,
        rows: if numeric { v.rows } else { 0 },
        cols: if numeric { v.cols } else { 0 },
        elements: v.elements,
        flags,
        elem_of: None,
        elems: Vec::new(),
        annotations: Vec::new(),
        data,
        obj: 0,
        string: Vec::new(),
        sampler: Vec::new(),
        shader: None,
    }
}

/// Paramètres, liste des paramètres de premier niveau et techniques d'un
/// effet fx_2_0 binaire.
pub(super) fn read(src: &[u8]) -> Result<(Vec<Param>, Vec<usize>, Vec<Tech>), String> {
    let raw = read_raw(src)?;
    let objects = &raw.objects;
    // Nom du paramètre texture d'un objet : celui que nomme l'objet, sinon
    // le paramètre qui porte cet objet.
    let texture_name = |obj: u32| -> Option<String> {
        object_name(objects, obj).or_else(|| {
            raw.params
                .iter()
                .find(|(p, _)| {
                    p.objects.contains(&obj) && (PT_TEXTURE..PT_TEXTURE + 5).contains(&p.ty)
                })
                .map(|(p, _)| p.name.clone())
        })
    };
    let annotations = |params: &mut Vec<Param>, list: &[Value]| -> Vec<usize> {
        list.iter()
            .map(|a| {
                let mut p = new_param(a, 4);
                if a.ty == PT_STRING {
                    p.string = cstring(
                        &a.objects
                            .first()
                            .and_then(|&o| object_name(objects, o))
                            .unwrap_or_default(),
                    );
                }
                params.push(p);
                params.len() - 1
            })
            .collect()
    };
    let mut params: Vec<Param> = Vec::new();
    let mut top = Vec::new();
    for (v, ann) in &raw.params {
        let mut p = new_param(v, 0);
        let first = v.objects.first().copied();
        match v.ty {
            PT_STRING => {
                p.string = cstring(
                    &first
                        .and_then(|o| object_name(objects, o))
                        .unwrap_or_default(),
                )
            }
            PT_PIXELSHADER | PT_VERTEXSHADER => {
                p.shader = first.and_then(|o| shader_of(objects, o))
            }
            t if is_sampler(t) => {
                for (state, sv) in &v.sampler {
                    let (name, value) = match *state {
                        SAMPLER_TEXTURE => (
                            "texture",
                            match sv.objects.first().and_then(|&o| texture_name(o)) {
                                Some(n) => Expr::Ident(n),
                                None => continue,
                            },
                        ),
                        s @ 5..=15 => (SAMPLER_STATES[s as usize - 5], state_expr(sv)),
                        s => {
                            crate::log_warn!(
                                "d3dx9",
                                "effet binaire : état d'échantillonneur {s} ignoré"
                            );
                            continue;
                        }
                    };
                    p.sampler.push(StateAssign {
                        name: name.into(),
                        index: None,
                        value,
                    });
                }
            }
            _ => {}
        }
        p.annotations = annotations(&mut params, ann);
        params.push(p);
        let idx = params.len() - 1;
        top.push(idx);
        super::push_elements(&mut params, idx);
    }
    let mut techs = Vec::new();
    for t in &raw.techs {
        let tann = annotations(&mut params, &t.annotations);
        let mut passes = Vec::new();
        for p in &t.passes {
            let pann = annotations(&mut params, &p.annotations);
            let mut pc = PassC {
                name: cstring(&p.name),
                annotations: pann,
                states: Vec::new(),
                vs: None,
                ps: None,
            };
            for (id, v) in &p.states {
                match *id {
                    STATE_VERTEXSHADER | STATE_PIXELSHADER => {
                        let sh = v.objects.first().and_then(|&o| shader_of(objects, o));
                        if *id == STATE_PIXELSHADER {
                            pc.ps = Some(sh);
                        } else {
                            pc.vs = Some(sh);
                        }
                    }
                    i => match PASS_STATES.get(i as usize) {
                        Some(name) if !name.is_empty() => pc.states.push(StateAssign {
                            name: (*name).into(),
                            index: None,
                            value: state_expr(v),
                        }),
                        _ => crate::log_warn!("d3dx9", "effet binaire : état de passe {i} ignoré"),
                    },
                }
            }
            passes.push(pc);
        }
        techs.push(Tech {
            name: cstring(&t.name),
            annotations: tann,
            passes,
            valid: true,
        });
    }
    Ok((params, top, techs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Écriture d'un effet fx_2_0 binaire selon la disposition lue plus haut.
    #[derive(Default)]
    struct W {
        b: Vec<u8>,
    }

    impl W {
        fn put(&mut self, v: u32) -> usize {
            self.b.extend(v.to_le_bytes());
            self.b.len() - 4
        }
        fn string(&mut self, s: &str) -> u32 {
            let at = self.put(s.len() as u32 + 1);
            self.b.extend(s.as_bytes());
            self.b.push(0);
            while self.b.len() % 4 != 0 {
                self.b.push(0);
            }
            at as u32
        }
        /// Type numérique ou objet : (type, classe, nom, sémantique, éléments, colonnes, lignes).
        fn ty(&mut self, ty: u32, class: u32, name: &str, cols: u32, rows: u32) -> u32 {
            let n = self.string(name);
            let at = self.put(ty);
            self.put(class);
            self.put(n);
            self.put(0);
            self.put(0);
            if class <= 3 {
                self.put(cols);
                self.put(rows);
            }
            at as u32
        }
        fn words(&mut self, v: &[u32]) -> u32 {
            let at = self.b.len() as u32;
            for &x in v {
                self.put(x);
            }
            at
        }
    }

    fn effet_de_test(vs: &[u32]) -> Vec<u8> {
        let mut w = W::default();
        w.put(0); // les décalages nuls désignent une chaîne vide
                  // Paramètres.
        let t_color = w.ty(PT_FLOAT, 1, "Color", 4, 1);
        let v_color = w.words(&[
            1f32.to_bits(),
            0.5f32.to_bits(),
            0.25f32.to_bits(),
            1f32.to_bits(),
        ]);
        let t_tex = w.ty(PT_TEXTURE, 4, "Tex", 0, 0);
        let v_tex = w.words(&[1]);
        let t_samp = w.ty(PT_SAMPLER + 2, 4, "Samp", 0, 0);
        let t_ss_tex = w.ty(PT_TEXTURE, 4, "", 0, 0);
        let v_ss_tex = w.words(&[2]);
        let t_ss_min = w.ty(PT_INT, 0, "", 1, 1);
        let v_ss_min = w.words(&[2]);
        let v_samp = w.words(&[2, 0xA4, 0, t_ss_tex, v_ss_tex, 10, 0, t_ss_min, v_ss_min]);
        // États de la passe.
        let t_bool = w.ty(PT_BOOL, 0, "", 1, 1);
        let v_true = w.words(&[1]);
        let t_int = w.ty(PT_INT, 0, "", 1, 1);
        let v_five = w.words(&[5]);
        let v_factor = w.words(&[0xFF80_8080]);
        let t_float = w.ty(PT_FLOAT, 0, "", 1, 1);
        let v_half = w.words(&[0.5f32.to_bits()]);
        let t_vs = w.ty(PT_VERTEXSHADER, 4, "", 0, 0);
        let v_vs = w.words(&[3]);
        let t_ps = w.ty(PT_PIXELSHADER, 4, "", 0, 0);
        let v_ps = w.words(&[4]);
        let t_ann = w.ty(PT_STRING, 4, "Note", 0, 0);
        let v_ann = w.words(&[5]);
        let tech = w.string("T0");
        let pass = w.string("P0");
        // Table des comptes.
        let offset = w.words(&[3, 1, 0, 6]);
        w.words(&[t_color, v_color, 0, 0]);
        w.words(&[t_tex, v_tex, 0, 0]);
        w.words(&[t_samp, v_samp, 0, 0]);
        w.words(&[tech, 1, 1, t_ann, v_ann]);
        w.words(&[pass, 0, 6]);
        w.words(&[13, 0, t_bool, v_true]); // AlphaBlendEnable
        w.words(&[6, 0, t_int, v_five]); // SrcBlend
        w.words(&[18, 0, t_float, v_half]); // FogStart
        w.words(&[30, 0, t_int, v_factor]); // TextureFactor
        w.words(&[146, 0, t_vs, v_vs]);
        w.words(&[147, 0, t_ps, v_ps]);
        // Objets : 3 petits, 1 grand (le shader de sommets de la passe).
        w.words(&[3, 1]);
        w.words(&[2, 4]);
        w.b.extend(b"Tex\0");
        w.words(&[4, 0]);
        w.words(&[5, 8]);
        w.b.extend(b"Bonjour\0");
        w.words(&[0, 0, 0, 4, 0, 4 * vs.len() as u32]);
        w.words(vs);
        let mut out = Vec::new();
        out.extend(MAGIC.to_le_bytes());
        out.extend(offset.to_le_bytes());
        out.extend(w.b);
        out
    }

    #[test]
    fn effet_fx_2_0_binaire_lu() {
        let vs = crate::dlls::d3dx9_asm::assemble("vs_1_1\ndcl_position v0\nmov oPos, v0\n", &[])
            .unwrap();
        let src = effet_de_test(&vs);
        let (params, top, techs) = read(&src).expect("effet lu");
        let named = |n: &str| {
            top.iter()
                .map(|&i| &params[i])
                .find(|p| p.name_str() == n)
                .unwrap()
        };
        assert_eq!(top.len(), 3);
        let color = named("Color");
        assert_eq!(
            (color.class, color.ptype, color.rows, color.cols),
            (1, PT_FLOAT, 1, 4)
        );
        assert_eq!(color.data, vec![1.0, 0.5, 0.25, 1.0]);
        assert_eq!(named("Tex").ptype, PT_TEXTURE);
        let samp = named("Samp");
        assert_eq!(samp.sampler.len(), 2);
        assert_eq!(samp.sampler[0].name, "texture");
        assert!(matches!(&samp.sampler[0].value, Expr::Ident(n) if n == "Tex"));
        assert_eq!(samp.sampler[1].name, "minfilter");
        assert_eq!(techs.len(), 1);
        assert_eq!(techs[0].annotations.len(), 1);
        let ann = &params[techs[0].annotations[0]];
        assert_eq!(ann.string, b"Bonjour\0");
        let p = &techs[0].passes[0];
        assert_eq!(p.name, b"P0\0");
        let names: Vec<&str> = p.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["alphablendenable", "srcblend", "fogstart", "texturefactor"]
        );
        assert!(matches!(p.states[3].value, Expr::Num(v, false) if v == 0xFF80_8080u32 as f64));
        assert!(matches!(p.states[2].value, Expr::Num(v, true) if v == 0.5));
        assert_eq!(p.vs.as_ref().unwrap().as_ref().unwrap().tokens, vs);
        assert!(matches!(p.ps, Some(None)));
        // Fichier tronqué : erreur, jamais de lecture hors du tampon.
        for cut in [8, src.len() / 2, src.len() - 4] {
            assert!(read(&src[..cut]).is_err());
        }
    }
}
