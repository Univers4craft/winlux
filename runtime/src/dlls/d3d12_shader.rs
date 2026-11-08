//! Shaders DXBC de Direct3D 12 sur le chemin OpenGL (#1177).
//!
//! - Signature racine : lue depuis la partie `RTS0` (versions 1.0 et 1.1),
//!   seule ou dans un conteneur DXBC, comme la produisent le compilateur et
//!   `D3D12SerializeRootSignature`. Disposition relevée dans le code public
//!   du compilateur DirectX Shader Compiler (licence MIT/UIUC).
//! - Arguments racine enregistrés par la liste de commandes : adresse d'un
//!   tampon de constantes, constantes 32 bits, ou table de descripteurs.
//! - Au dessin, chaque tampon de constantes `bN` visible d'un étage est lu
//!   (au moment de l'exécution, comme sur le GPU) puis lié en bloc uniforme
//!   sur `étage * 16 + N`, la convention de `dxbc.rs`.
//!
//! Textures et échantillonneurs : étape suivante de #1177.

use super::{bounded, descriptor_from_gpu, DESC_CBV, DESC_SIZE};
use crate::dlls::dxbc::{self, SigElement, Stage};
use crate::dlls::gl_bindings as gl;
use crate::log_warn;
use std::collections::HashMap;
use std::ffi::c_void;

/// Visibilité D3D12_SHADER_VISIBILITY : 0 tous, 1 sommets, 5 pixels.
const VIS_ALL: u32 = 0;
const VIS_VERTEX: u32 = 1;
const VIS_PIXEL: u32 = 5;

/// Plage d'une table de descripteurs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Range {
    /// D3D12_DESCRIPTOR_RANGE_TYPE : 0 SRV, 1 UAV, 2 CBV, 3 échantillonneur.
    pub kind: u32,
    pub count: u32,
    pub base: u32,
    pub space: u32,
    /// Décalage depuis le début de la table, `u32::MAX` : à la suite.
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Param {
    Table(Vec<Range>),
    Constants { reg: u32, space: u32, count: u32 },
    Cbv { reg: u32, space: u32 },
    Srv { reg: u32, space: u32 },
    Uav { reg: u32, space: u32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RootLayout {
    /// (paramètre, visibilité).
    pub params: Vec<(Param, u32)>,
}

fn rd(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// Signature racine d'un blob : conteneur DXBC (partie `RTS0`) ou `RTS0`
/// nu. Un blob illisible donne une signature vide.
pub(super) fn parse_root(blob: &[u8]) -> RootLayout {
    let data = if blob.starts_with(b"DXBC") {
        match dxbc::chunks(blob).into_iter().find(|c| &c.0 == b"RTS0") {
            Some((_, d)) => d,
            None => return RootLayout::default(),
        }
    } else {
        blob
    };
    parse_rts0(data).unwrap_or_default()
}

fn parse_rts0(d: &[u8]) -> Option<RootLayout> {
    let version = rd(d, 0)?;
    if version != 1 && version != 2 {
        return None;
    }
    let n = rd(d, 4)?.min(64) as usize;
    let at = rd(d, 8)? as usize;
    let mut params = Vec::with_capacity(n);
    for i in 0..n {
        let p = at + i * 12;
        let (ty, vis, payload) = (rd(d, p)?, rd(d, p + 4)?, rd(d, p + 8)? as usize);
        let param = match ty {
            0 => {
                let count = rd(d, payload)?.min(64) as usize;
                let ranges_at = rd(d, payload + 4)? as usize;
                let size = if version == 1 { 20 } else { 24 };
                let mut ranges = Vec::with_capacity(count);
                for r in 0..count {
                    let q = ranges_at + r * size;
                    ranges.push(Range {
                        kind: rd(d, q)?,
                        count: rd(d, q + 4)?,
                        base: rd(d, q + 8)?,
                        space: rd(d, q + 12)?,
                        offset: rd(d, q + size - 4)?,
                    });
                }
                Param::Table(ranges)
            }
            1 => Param::Constants {
                reg: rd(d, payload)?,
                space: rd(d, payload + 4)?,
                count: rd(d, payload + 8)?.min(64),
            },
            2..=4 => {
                let (reg, space) = (rd(d, payload)?, rd(d, payload + 4)?);
                match ty {
                    2 => Param::Cbv { reg, space },
                    3 => Param::Srv { reg, space },
                    _ => Param::Uav { reg, space },
                }
            }
            _ => return None,
        };
        params.push((param, vis));
    }
    Some(RootLayout { params })
}

/// Sérialise D3D12_ROOT_SIGNATURE_DESC (version 1, 1.0) ou sa version 1.1
/// (D3D12_ROOT_SIGNATURE_DESC1, `v11`) en `RTS0` dans un conteneur DXBC.
///
/// # Safety
/// `desc` pointe une description valide dans sa disposition x64.
pub(super) unsafe fn serialize_root(desc: *const u8, v11: bool) -> Vec<u8> {
    let r32 = |p: *const u8, o: usize| std::ptr::read_unaligned(p.add(o) as *const u32);
    let rptr = |p: *const u8, o: usize| std::ptr::read_unaligned(p.add(o) as *const *const u8);
    let n = r32(desc, 0).min(64) as usize;
    let params = rptr(desc, 8);
    let n_samplers = r32(desc, 16).min(64) as usize;
    let samplers = rptr(desc, 24);
    let flags = r32(desc, 32);
    let version: u32 = if v11 { 2 } else { 1 };

    // En-tête (24 octets), paramètres (12 octets chacun), puis leurs données.
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let params_at = 24u32;
    let mut data_at = params_at + 12 * n as u32;
    let push = |v: &mut Vec<u8>, x: u32| v.extend(x.to_le_bytes());
    for i in 0..if params.is_null() { 0 } else { n } {
        let p = params.add(i * 32);
        let ty = r32(p, 0);
        let vis = r32(p, 24);
        let start = data_at + tail.len() as u32;
        push(&mut head, ty);
        push(&mut head, vis);
        push(&mut head, start);
        match ty {
            0 => {
                let count = r32(p, 8).min(64) as usize;
                let ranges = rptr(p, 16);
                let size = if v11 { 24 } else { 20 };
                push(&mut tail, count as u32);
                push(&mut tail, start + 8);
                for r in 0..if ranges.is_null() { 0 } else { count } {
                    let q = ranges.add(r * size);
                    for w in 0..size / 4 {
                        push(&mut tail, r32(q, w * 4));
                    }
                }
            }
            1 => {
                for w in 0..3 {
                    push(&mut tail, r32(p, 8 + w * 4));
                }
            }
            _ => {
                for w in 0..if v11 { 3 } else { 2 } {
                    push(&mut tail, r32(p, 8 + w * 4));
                }
            }
        }
    }
    data_at += tail.len() as u32;
    let samplers_at = data_at;
    let mut out = Vec::new();
    push(&mut out, version);
    push(&mut out, n as u32);
    push(&mut out, params_at);
    push(
        &mut out,
        if samplers.is_null() {
            0
        } else {
            n_samplers as u32
        },
    );
    push(&mut out, samplers_at);
    push(&mut out, flags);
    out.extend(head);
    out.extend(tail);
    if !samplers.is_null() {
        out.extend_from_slice(std::slice::from_raw_parts(samplers, n_samplers * 52));
    }
    container(b"RTS0", &out)
}

/// Conteneur DXBC à une partie (somme de contrôle laissée à zéro).
fn container(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let total = 32 + 4 + 8 + data.len();
    let mut v = b"DXBC".to_vec();
    v.extend([0u8; 16]);
    v.extend(1u32.to_le_bytes());
    v.extend((total as u32).to_le_bytes());
    v.extend(1u32.to_le_bytes());
    v.extend(36u32.to_le_bytes());
    v.extend(tag);
    v.extend((data.len() as u32).to_le_bytes());
    v.extend(data);
    v
}

/// Argument racine enregistré par la liste.
#[derive(Clone, Debug, Default)]
pub(super) enum RootArg {
    #[default]
    None,
    Va(u64),
    Constants(Vec<u32>),
    Table(u64),
}

/// Source d'un tampon de constantes, lue au moment du dessin.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum CbSource {
    /// Adresse GPU (racine) : jusqu'à 64 Kio, bornés à la ressource.
    Va(u64),
    /// Constantes 32 bits copiées à l'enregistrement.
    Data(Vec<u8>),
    /// Descripteur CBV d'une table (poignée GPU).
    Descriptor(u64),
}

/// Tampon de constantes à lier : visibilité, registre, source.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CbBinding {
    pub vis: u32,
    pub reg: u32,
    pub src: CbSource,
}

/// Tampons de constantes vus par les shaders, d'après la signature racine
/// et les arguments en place (espace de registres 0 seulement).
pub(super) fn constant_buffers(root: &RootLayout, args: &[RootArg]) -> Vec<CbBinding> {
    let mut out = Vec::new();
    for (i, (param, vis)) in root.params.iter().enumerate() {
        let arg = args.get(i).cloned().unwrap_or_default();
        match (param, arg) {
            (Param::Cbv { reg, space: 0 }, RootArg::Va(va)) if va != 0 => out.push(CbBinding {
                vis: *vis,
                reg: *reg,
                src: CbSource::Va(va),
            }),
            (
                Param::Constants {
                    reg,
                    space: 0,
                    count,
                },
                RootArg::Constants(v),
            ) => {
                let mut bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                bytes.resize(*count as usize * 4, 0);
                out.push(CbBinding {
                    vis: *vis,
                    reg: *reg,
                    src: CbSource::Data(bytes),
                });
            }
            (Param::Table(ranges), RootArg::Table(base)) if base != 0 => {
                let mut next = 0u32;
                for r in ranges {
                    let start = if r.offset == u32::MAX { next } else { r.offset };
                    // Plage non bornée (UINT_MAX) : 14 tampons au plus par étage.
                    let count = r.count.min(14);
                    if r.kind == 2 && r.space == 0 {
                        for k in 0..count {
                            out.push(CbBinding {
                                vis: *vis,
                                reg: r.base + k,
                                src: CbSource::Descriptor(
                                    base + (start + k) as u64 * DESC_SIZE as u64,
                                ),
                            });
                        }
                    }
                    next = start.saturating_add(count);
                }
            }
            _ => {}
        }
    }
    out
}

/// Octets d'un tampon de constantes au moment du dessin.
unsafe fn cb_bytes(src: &CbSource) -> Option<Vec<u8>> {
    match src {
        CbSource::Data(d) => Some(d.clone()),
        CbSource::Va(va) => bounded(*va, 65536).map(<[u8]>::to_vec),
        CbSource::Descriptor(h) => {
            let d = descriptor_from_gpu(*h)?;
            if d.kind != DESC_CBV {
                return None;
            }
            bounded(d.va, d.size.clamp(16, 65536)).map(<[u8]>::to_vec)
        }
    }
}

/// Shaders traduits d'un PSO graphique.
pub(super) struct Translated {
    pub vs: String,
    pub ps: Option<String>,
    pub vs_inputs: Vec<SigElement>,
    /// Clé du programme GL dans le cache du moteur.
    pub key: u64,
}

/// Traduit le vertex shader et le pixel shader d'un PSO (DXBC seulement ;
/// le DXIL relève de #1178).
pub(super) fn translate(vs: &[u8], ps: &[u8]) -> Option<Translated> {
    if vs.is_empty() {
        return None;
    }
    let v = match dxbc::translate(vs) {
        Ok(s) if s.stage == Stage::Vertex => s,
        Ok(_) => return None,
        Err(e) => {
            log_warn!("d3d12", "vertex shader non traduit : {e}");
            return None;
        }
    };
    let p = if ps.is_empty() {
        None
    } else {
        match dxbc::translate(ps) {
            Ok(s) if s.stage == Stage::Pixel => Some(s.glsl),
            Ok(_) => return None,
            Err(e) => {
                log_warn!("d3d12", "pixel shader non traduit : {e}");
                return None;
            }
        }
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&(vs, ps), &mut h);
    Some(Translated {
        vs: v.glsl,
        ps: p,
        vs_inputs: v.inputs,
        key: std::hash::Hasher::finish(&h),
    })
}

/// Élément de la disposition d'entrée d'un PSO.
#[derive(Clone, Debug)]
pub(super) struct Element {
    pub name: String,
    pub index: u32,
    pub format: u32,
    pub slot: u32,
    pub offset: u32,
}

/// (composantes, type GL, normalisé, entier) d'un attribut de sommet.
fn attr_format(f: u32) -> Option<(i32, u32, u8, bool)> {
    const GL_INT: u32 = 0x1404;
    const GL_UNSIGNED_INT: u32 = 0x1405;
    const GL_HALF_FLOAT: u32 = 0x140B;
    Some(match f {
        2 => (4, gl::GL_FLOAT, 0, false),
        3 => (4, GL_UNSIGNED_INT, 0, true),
        4 => (4, GL_INT, 0, true),
        6 => (3, gl::GL_FLOAT, 0, false),
        10 => (4, GL_HALF_FLOAT, 0, false),
        16 => (2, gl::GL_FLOAT, 0, false),
        17 => (2, GL_UNSIGNED_INT, 0, true),
        28 | 29 => (4, gl::GL_UNSIGNED_BYTE, 1, false),
        34 => (2, GL_HALF_FLOAT, 0, false),
        41 => (1, gl::GL_FLOAT, 0, false),
        42 => (1, GL_UNSIGNED_INT, 0, true),
        87 => (4, gl::GL_UNSIGNED_BYTE, 1, false),
        _ => return None,
    })
}

/// Programmes et tampons uniformes du moteur OpenGL de D3D12.
#[derive(Default)]
pub(super) struct GlShaders {
    programs: HashMap<u64, u32>,
    ubos: Vec<u32>,
}

impl GlShaders {
    unsafe fn program(&mut self, g: &gl::GlFunctions, t: &Translated) -> u32 {
        if let Some(&p) = self.programs.get(&t.key) {
            return p;
        }
        let compile = |ty: u32, src: &str| -> Option<u32> {
            let s = (g.CreateShader)(ty);
            let c = std::ffi::CString::new(src).ok()?;
            let p = c.as_ptr();
            (g.ShaderSource)(s, 1, &p, std::ptr::null());
            (g.CompileShader)(s);
            let mut ok = 0;
            (g.GetShaderiv)(s, gl::GL_COMPILE_STATUS, &mut ok);
            if ok == 0 {
                let mut len = 0;
                (g.GetShaderiv)(s, gl::GL_INFO_LOG_LENGTH, &mut len);
                let mut buf = vec![0u8; len.max(1) as usize];
                (g.GetShaderInfoLog)(s, len, std::ptr::null_mut(), buf.as_mut_ptr().cast());
                log_warn!(
                    "d3d12",
                    "shader GLSL refusé : {}",
                    String::from_utf8_lossy(&buf)
                );
                (g.DeleteShader)(s);
                return None;
            }
            Some(s)
        };
        let mut prog = 0;
        if let Some(vs) = compile(gl::GL_VERTEX_SHADER, &t.vs) {
            let ps = match &t.ps {
                Some(src) => compile(gl::GL_FRAGMENT_SHADER, src),
                None => Some(0),
            };
            if let Some(ps) = ps {
                let p = (g.CreateProgram)();
                (g.AttachShader)(p, vs);
                if ps != 0 {
                    (g.AttachShader)(p, ps);
                }
                (g.LinkProgram)(p);
                let mut ok = 0;
                (g.GetProgramiv)(p, gl::GL_LINK_STATUS, &mut ok);
                if ok == 0 {
                    log_warn!("d3d12", "édition de liens GL refusée");
                    (g.DeleteProgram)(p);
                } else {
                    prog = p;
                }
                if ps != 0 {
                    (g.DeleteShader)(ps);
                }
            }
            (g.DeleteShader)(vs);
        }
        // Mémorisé même à 0 : un programme refusé ne se recompile pas.
        self.programs.insert(t.key, prog);
        prog
    }

    /// Lie le programme, les attributs (emplacement = registre d'entrée du
    /// vertex shader) et les tampons de constantes. Rend les emplacements
    /// d'attributs activés, à désactiver après le dessin, ou `None` quand
    /// le programme n'est pas utilisable.
    ///
    /// # Safety
    /// Contexte GL courant ; le tampon de sommets est lié à GL_ARRAY_BUFFER.
    pub(super) unsafe fn bind(
        &mut self,
        g: &gl::GlFunctions,
        t: &Translated,
        layout: &[Element],
        stride: u32,
        base: usize,
        cbs: &[CbBinding],
    ) -> Option<Vec<u32>> {
        let prog = self.program(g, t);
        if prog == 0 {
            return None;
        }
        (g.UseProgram)(prog);
        let mut enabled = Vec::new();
        for e in layout.iter().filter(|e| e.slot == 0) {
            let Some(sig) = t
                .vs_inputs
                .iter()
                .find(|i| i.name == e.name && i.index == e.index && i.system_value == 0)
            else {
                continue;
            };
            let Some((n, ty, norm, int)) = attr_format(e.format) else {
                continue;
            };
            let loc = sig.register;
            let ptr = (e.offset as usize + base) as *const c_void;
            if int {
                (g.VertexAttribIPointer)(loc, n, ty, stride as i32, ptr);
            } else {
                (g.VertexAttribPointer)(loc, n, ty, norm, stride as i32, ptr);
            }
            (g.EnableVertexAttribArray)(loc);
            enabled.push(loc);
        }
        for (i, cb) in cbs.iter().enumerate() {
            let Some(mut bytes) = cb_bytes(&cb.src) else {
                continue;
            };
            // Bloc std140 de vec4 : longueur multiple de 16 octets.
            let len = bytes.len().div_ceil(16).max(1) * 16;
            bytes.resize(len, 0);
            while self.ubos.len() <= i {
                let mut b = 0;
                (g.GenBuffers)(1, &mut b);
                self.ubos.push(b);
            }
            let ubo = self.ubos[i];
            (g.BindBuffer)(gl::GL_UNIFORM_BUFFER, ubo);
            (g.BufferData)(
                gl::GL_UNIFORM_BUFFER,
                bytes.len() as isize,
                bytes.as_ptr().cast(),
                gl::GL_STREAM_DRAW,
            );
            for (stage, wanted) in [(Stage::Vertex, VIS_VERTEX), (Stage::Pixel, VIS_PIXEL)] {
                if cb.vis == VIS_ALL || cb.vis == wanted {
                    (g.BindBufferBase)(gl::GL_UNIFORM_BUFFER, stage.binding_base() + cb.reg, ubo);
                }
            }
        }
        (g.BindBuffer)(gl::GL_UNIFORM_BUFFER, 0);
        Some(enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D3D12_ROOT_PARAMETER (32 octets en x64) sérialisé puis relu.
    #[test]
    fn root_signature_round_trip() {
        // Plage CBV b2..b3 à la suite, en version 1.0 (20 octets).
        let range: [u32; 5] = [2, 2, 2, 0, u32::MAX];
        let mut params = [0u8; 96];
        let put = |p: &mut [u8], o: usize, v: u32| p[o..o + 4].copy_from_slice(&v.to_le_bytes());
        // 0 : table, visibilité pixels.
        put(&mut params, 0, 0);
        put(&mut params, 8, 1);
        params[16..24].copy_from_slice(&(range.as_ptr() as u64).to_le_bytes());
        put(&mut params, 24, 5);
        // 1 : 4 constantes en b1, tous les étages.
        put(&mut params, 32, 1);
        put(&mut params, 40, 1);
        put(&mut params, 48, 4);
        // 2 : CBV racine b0, sommets.
        put(&mut params, 64, 2);
        put(&mut params, 88, 1);
        let mut desc = [0u8; 40];
        put(&mut desc, 0, 3);
        desc[8..16].copy_from_slice(&(params.as_ptr() as u64).to_le_bytes());
        let blob = unsafe { serialize_root(desc.as_ptr(), false) };
        assert!(blob.starts_with(b"DXBC"));
        let root = parse_root(&blob);
        assert_eq!(
            root.params,
            vec![
                (
                    Param::Table(vec![Range {
                        kind: 2,
                        count: 2,
                        base: 2,
                        space: 0,
                        offset: u32::MAX
                    }]),
                    5
                ),
                (
                    Param::Constants {
                        reg: 1,
                        space: 0,
                        count: 4
                    },
                    0
                ),
                (Param::Cbv { reg: 0, space: 0 }, 1),
            ]
        );
    }

    #[test]
    fn root_arguments_become_constant_buffers() {
        let root = RootLayout {
            params: vec![
                (
                    Param::Table(vec![Range {
                        kind: 2,
                        count: 2,
                        base: 2,
                        space: 0,
                        offset: u32::MAX,
                    }]),
                    5,
                ),
                (
                    Param::Constants {
                        reg: 1,
                        space: 0,
                        count: 2,
                    },
                    0,
                ),
                (Param::Cbv { reg: 0, space: 0 }, 1),
            ],
        };
        let args = vec![
            RootArg::Table(0x1000),
            RootArg::Constants(vec![7]),
            RootArg::Va(0x5000),
        ];
        let cbs = constant_buffers(&root, &args);
        assert_eq!(cbs.len(), 4);
        assert_eq!(cbs[0].reg, 2);
        assert_eq!(cbs[1].src, CbSource::Descriptor(0x1000 + DESC_SIZE as u64));
        assert_eq!(cbs[2].src, CbSource::Data(vec![7, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!((cbs[3].vis, cbs[3].src.clone()), (1, CbSource::Va(0x5000)));
    }
}
