//! Pipeline Direct3D 11 sur OpenGL 4.3 (#137).
//!
//! Les shaders DXBC sont traduits en GLSL par `dxbc.rs` : chaque étage a ses
//! tampons de constantes et ses unités de texture à partir de
//! `Stage::binding_base` (VS 0, PS 16, GS 32). Ce module retient l'état que
//! le contexte reçoit (tampons, vues, échantillonneurs, états de mélange, de
//! profondeur et de rastérisation, mise en page des entrées) et l'applique à
//! chaque tirage.
//!
//! Repère : le vertex shader traduit inverse Y. La ligne 0 d'une texture GL
//! est donc la ligne du haut, comme sous Direct3D : viewports et ciseaux se
//! passent tels quels, l'échantillonnage garde ses coordonnées, et seule la
//! présentation retourne l'image. L'inversion change le sens des triangles :
//! la face avant D3D (horaire par défaut) devient anti-horaire pour GL.

use super::*;
use crate::dlls::dxbc::{self, SigElement, Stage};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

// Constantes GL propres au pipeline.
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_ARRAY_BUFFER: u32 = 0x8892;
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_TEXTURE_WRAP_R: u32 = 0x8072;
const GL_TEXTURE_MIN_LOD: u32 = 0x813A;
const GL_TEXTURE_MAX_LOD: u32 = 0x813B;
const GL_TEXTURE_LOD_BIAS: u32 = 0x8501;
const GL_TEXTURE_BORDER_COLOR: u32 = 0x1004;
const GL_TEXTURE_MAX_ANISOTROPY: u32 = 0x84FE;
const GL_TEXTURE_COMPARE_MODE: u32 = 0x884C;
const GL_TEXTURE_COMPARE_FUNC: u32 = 0x884D;
const GL_COMPARE_REF_TO_TEXTURE: i32 = 0x884E;
const GL_NEAREST: i32 = 0x2600;
const GL_LINEAR: i32 = 0x2601;
const GL_NEAREST_MIPMAP_NEAREST: i32 = 0x2700;
const GL_LINEAR_MIPMAP_NEAREST: i32 = 0x2701;
const GL_NEAREST_MIPMAP_LINEAR: i32 = 0x2702;
const GL_LINEAR_MIPMAP_LINEAR: i32 = 0x2703;
const GL_REPEAT: i32 = 0x2901;
const GL_MIRRORED_REPEAT: i32 = 0x8370;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_CLAMP_TO_BORDER: i32 = 0x812D;
const GL_MIRROR_CLAMP_TO_EDGE: i32 = 0x8743;
const GL_UNIFORM_BUFFER: u32 = 0x8A11;
const GL_BYTE: u32 = 0x1400;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_SHORT: u32 = 0x1402;
const GL_UNSIGNED_SHORT: u32 = 0x1403;
const GL_INT: u32 = 0x1404;
const GL_UNSIGNED_INT: u32 = 0x1405;
const GL_FLOAT: u32 = 0x1406;
const GL_HALF_FLOAT: u32 = 0x140B;
const GL_UNSIGNED_INT_2_10_10_10_REV: u32 = 0x8368;
const GL_BGRA: i32 = 0x80E1;
const GL_BLEND: u32 = 0x0BE2;
const GL_CULL_FACE: u32 = 0x0B44;
const GL_DEPTH_TEST: u32 = 0x0B71;
const GL_STENCIL_TEST: u32 = 0x0B90;
const GL_SCISSOR_TEST: u32 = 0x0C11;
const GL_POLYGON_OFFSET_FILL: u32 = 0x8037;
const GL_FRONT: u32 = 0x0404;
const GL_BACK: u32 = 0x0405;
const GL_FRONT_AND_BACK: u32 = 0x0408;
const GL_CW: u32 = 0x0900;
const GL_CCW: u32 = 0x0901;
const GL_LINE: u32 = 0x1B01;
const GL_FILL: u32 = 0x1B02;
const GL_KEEP: u32 = 0x1E00;
const GL_REPLACE: u32 = 0x1E01;
const GL_INCR: u32 = 0x1E02;
const GL_DECR: u32 = 0x1E03;
const GL_INVERT: u32 = 0x150A;
const GL_INCR_WRAP: u32 = 0x8507;
const GL_DECR_WRAP: u32 = 0x8508;
const GL_FUNC_ADD: u32 = 0x8006;
const GL_MIN: u32 = 0x8007;
const GL_MAX: u32 = 0x8008;
const GL_FUNC_SUBTRACT: u32 = 0x800A;
const GL_FUNC_REVERSE_SUBTRACT: u32 = 0x800B;
const GL_TRIANGLES_ADJACENCY: u32 = 0x000C;
const GL_TRIANGLE_STRIP_ADJACENCY: u32 = 0x000D;
const GL_LINES_ADJACENCY: u32 = 0x000A;
const GL_LINE_STRIP_ADJACENCY: u32 = 0x000B;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_TEXTURE_MAX_LEVEL: u32 = 0x813D;

/// Étages suivis : VS, PS, GS.
pub(super) const STAGES: [Stage; 3] = [Stage::Vertex, Stage::Pixel, Stage::Geometry];
const MAX_CB: usize = 14;
const MAX_SRV: usize = 16;
const MAX_SAMPLER: usize = 16;
const MAX_VB: usize = 16;

fn stage_slot(st: Stage) -> usize {
    match st {
        Stage::Vertex => 0,
        Stage::Pixel => 1,
        _ => 2,
    }
}

// ────────────────────── Objets décrits ──────────────────────

/// Élément d'une mise en page des entrées.
#[derive(Clone, Debug)]
pub(super) struct LayoutElement {
    pub name: String,
    pub index: u32,
    pub slot: u32,
    pub offset: u32,
    pub per_instance: bool,
    pub step_rate: u32,
    pub format: VertexFormat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VertexFormat {
    pub components: i32,
    pub gl_type: u32,
    pub normalized: bool,
    /// Entier lu tel quel (glVertexAttribIPointer).
    pub integer: bool,
    pub size: u32,
    /// GL_BGRA comme nombre de composantes (B8G8R8A8).
    pub bgra: bool,
}

/// Format DXGI d'un attribut de sommet.
pub(super) fn vertex_format(f: u32) -> Option<VertexFormat> {
    let v = |components, gl_type, normalized, integer, size| VertexFormat {
        components,
        gl_type,
        normalized,
        integer,
        size,
        bgra: false,
    };
    Some(match f {
        2 => v(4, GL_FLOAT, false, false, 16),
        3 => v(4, GL_UNSIGNED_INT, false, true, 16),
        4 => v(4, GL_INT, false, true, 16),
        6 => v(3, GL_FLOAT, false, false, 12),
        7 => v(3, GL_UNSIGNED_INT, false, true, 12),
        8 => v(3, GL_INT, false, true, 12),
        10 => v(4, GL_HALF_FLOAT, false, false, 8),
        11 => v(4, GL_UNSIGNED_SHORT, true, false, 8),
        12 => v(4, GL_UNSIGNED_SHORT, false, true, 8),
        13 => v(4, GL_SHORT, true, false, 8),
        14 => v(4, GL_SHORT, false, true, 8),
        16 => v(2, GL_FLOAT, false, false, 8),
        17 => v(2, GL_UNSIGNED_INT, false, true, 8),
        18 => v(2, GL_INT, false, true, 8),
        24 => v(4, GL_UNSIGNED_INT_2_10_10_10_REV, true, false, 4),
        28 | 29 => v(4, GL_UNSIGNED_BYTE, true, false, 4),
        30 => v(4, GL_UNSIGNED_BYTE, false, true, 4),
        31 => v(4, GL_BYTE, true, false, 4),
        32 => v(4, GL_BYTE, false, true, 4),
        34 => v(2, GL_HALF_FLOAT, false, false, 4),
        35 => v(2, GL_UNSIGNED_SHORT, true, false, 4),
        36 => v(2, GL_UNSIGNED_SHORT, false, true, 4),
        37 => v(2, GL_SHORT, true, false, 4),
        38 => v(2, GL_SHORT, false, true, 4),
        41 => v(1, GL_FLOAT, false, false, 4),
        42 => v(1, GL_UNSIGNED_INT, false, true, 4),
        43 => v(1, GL_INT, false, true, 4),
        49 => v(2, GL_UNSIGNED_BYTE, true, false, 2),
        50 => v(2, GL_UNSIGNED_BYTE, false, true, 2),
        51 => v(2, GL_BYTE, true, false, 2),
        52 => v(2, GL_BYTE, false, true, 2),
        54 => v(1, GL_HALF_FLOAT, false, false, 2),
        56 => v(1, GL_UNSIGNED_SHORT, true, false, 2),
        57 => v(1, GL_UNSIGNED_SHORT, false, true, 2),
        58 => v(1, GL_SHORT, true, false, 2),
        59 => v(1, GL_SHORT, false, true, 2),
        61 => v(1, GL_UNSIGNED_BYTE, true, false, 1),
        62 => v(1, GL_UNSIGNED_BYTE, false, true, 1),
        63 => v(1, GL_BYTE, true, false, 1),
        64 => v(1, GL_BYTE, false, true, 1),
        87 | 88 => VertexFormat {
            bgra: true,
            ..v(4, GL_UNSIGNED_BYTE, true, false, 4)
        },
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub(super) struct RtBlend {
    pub enable: i32,
    pub src: u32,
    pub dst: u32,
    pub op: u32,
    pub src_alpha: u32,
    pub dst_alpha: u32,
    pub op_alpha: u32,
    pub write_mask: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct BlendDesc {
    pub alpha_to_coverage: i32,
    pub independent: i32,
    pub rt: [RtBlend; 8],
}

impl Default for BlendDesc {
    fn default() -> Self {
        let rt = RtBlend {
            enable: 0,
            src: 2,
            dst: 1,
            op: 1,
            src_alpha: 2,
            dst_alpha: 1,
            op_alpha: 1,
            write_mask: 0xF,
        };
        BlendDesc {
            alpha_to_coverage: 0,
            independent: 0,
            rt: [rt; 8],
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct RasterDesc {
    pub fill: u32,
    pub cull: u32,
    pub front_ccw: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_bias: f32,
    pub depth_clip: i32,
    pub scissor: i32,
    pub multisample: i32,
    pub aa_line: i32,
}

impl Default for RasterDesc {
    fn default() -> Self {
        RasterDesc {
            fill: 3,
            cull: 3,
            front_ccw: 0,
            depth_bias: 0,
            depth_bias_clamp: 0.0,
            slope_scaled_bias: 0.0,
            depth_clip: 1,
            scissor: 0,
            multisample: 0,
            aa_line: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub(super) struct StencilFace {
    pub fail: u32,
    pub depth_fail: u32,
    pub pass: u32,
    pub func: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct DepthDesc {
    pub depth_enable: i32,
    pub write_mask: u32,
    pub func: u32,
    pub stencil_enable: i32,
    pub read_mask: u8,
    pub write_mask_stencil: u8,
    pub _pad: [u8; 2],
    pub front: StencilFace,
    pub back: StencilFace,
}

impl Default for DepthDesc {
    fn default() -> Self {
        let face = StencilFace {
            fail: 1,
            depth_fail: 1,
            pass: 1,
            func: 8,
        };
        DepthDesc {
            depth_enable: 1,
            write_mask: 1,
            func: 2,
            stencil_enable: 0,
            read_mask: 0xFF,
            write_mask_stencil: 0xFF,
            _pad: [0; 2],
            front: face,
            back: face,
        }
    }
}

/// D3D11_SAMPLER_DESC.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct SamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison: u32,
    pub border: [f32; 4],
    pub min_lod: f32,
    pub max_lod: f32,
}

/// Ce que l'on sait d'un objet COM du périphérique (clé : son adresse).
pub(super) enum Meta {
    Shader {
        stage: Stage,
        inputs: Vec<SigElement>,
        pairs: Vec<(u32, u32)>,
    },
    Layout(Vec<LayoutElement>),
    Sampler(gl::GLuint),
    Blend(Box<BlendDesc>),
    Raster(RasterDesc),
    Depth(DepthDesc),
}

static META: Mutex<Option<HashMap<usize, Arc<Meta>>>> = Mutex::new(None);

pub(super) fn register(obj: *mut c_void, meta: Meta) {
    let mut m = META.lock().unwrap_or_else(|e| e.into_inner());
    m.get_or_insert_with(HashMap::new)
        .insert(obj as usize, Arc::new(meta));
}

pub(super) fn meta(obj: *mut c_void) -> Option<Arc<Meta>> {
    if obj.is_null() {
        return None;
    }
    let m = META.lock().unwrap_or_else(|e| e.into_inner());
    m.as_ref()?.get(&(obj as usize)).cloned()
}

/// Libération d'un objet : oublie sa description et rend ses objets GL.
pub(super) fn forget(obj: *mut c_void, gl_id: gl::GLuint) {
    let removed = {
        let mut m = META.lock().unwrap_or_else(|e| e.into_inner());
        m.as_mut().and_then(|m| m.remove(&(obj as usize)))
    };
    let Some(meta) = removed else { return };
    let Some(g) = gl::gl() else { return };
    unsafe {
        match &*meta {
            Meta::Sampler(s) => (g.DeleteSamplers)(1, s),
            Meta::Shader { .. } if gl_id != 0 => {
                forget_programs(gl_id);
                (g.DeleteShader)(gl_id);
            }
            _ => {}
        }
    }
}

// ────────────────────── Création ──────────────────────

/// Shader DXBC traduit en GLSL, ou `None` si la traduction échoue.
pub(super) fn translate(bytes: &[u8], wanted: Stage) -> Option<dxbc::Shader> {
    match dxbc::translate(bytes) {
        Ok(s) if s.stage == wanted => Some(s),
        Ok(s) => {
            log_warn!(
                "d3d11",
                "shader {:?} fourni pour l'étage {:?}",
                s.stage,
                wanted
            );
            None
        }
        Err(e) => {
            log_warn!("d3d11", "traduction DXBC impossible : {e}");
            None
        }
    }
}

/// Mise en page des entrées.
pub(super) unsafe fn layout_from(
    descs: *const D3D11InputElementDesc,
    count: u32,
) -> Vec<LayoutElement> {
    let mut out = Vec::new();
    let mut next = [0u32; MAX_VB];
    for i in 0..count as usize {
        let d = &*descs.add(i);
        let name = if d.semantic_name.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(d.semantic_name.cast())
                .to_string_lossy()
                .to_ascii_uppercase()
        };
        let Some(format) = vertex_format(d.format) else {
            log_warn!("d3d11", "format de sommet {} non géré ({name})", d.format);
            continue;
        };
        let slot = (d.input_slot as usize).min(MAX_VB - 1);
        // D3D11_APPEND_ALIGNED_ELEMENT : à la suite du précédent.
        let offset = if d.aligned_byte_offset == u32::MAX {
            next[slot]
        } else {
            d.aligned_byte_offset
        };
        next[slot] = offset + format.size;
        out.push(LayoutElement {
            name,
            index: d.semantic_index,
            slot: slot as u32,
            offset,
            per_instance: d.input_slot_class == 1,
            step_rate: d.instance_data_step_rate,
            format,
        });
    }
    out
}

fn address(mode: u32) -> i32 {
    match mode {
        2 => GL_MIRRORED_REPEAT,
        3 => GL_CLAMP_TO_EDGE,
        4 => GL_CLAMP_TO_BORDER,
        5 => GL_MIRROR_CLAMP_TO_EDGE,
        _ => GL_REPEAT,
    }
}

fn compare_func(f: u32) -> u32 {
    // D3D11_COMPARISON_NEVER (1) … ALWAYS (8) → GL_NEVER (0x200) … GL_ALWAYS.
    0x0200 + f.clamp(1, 8) - 1
}

/// Objet échantillonneur GL d'après un D3D11_SAMPLER_DESC.
pub(super) unsafe fn create_sampler(g: &gl::GlFunctions, d: &SamplerDesc) -> gl::GLuint {
    let mut s = 0;
    (g.GenSamplers)(1, &mut s);
    if s == 0 {
        return 0;
    }
    // Filtre : bit 0 mip, bit 2 mag, bit 4 min linéaires ; 0x55 anisotrope ;
    // bit 7 comparaison.
    let f = d.filter;
    let aniso = f & 0x40 != 0;
    let min_lin = aniso || f & 0x10 != 0;
    let mag_lin = aniso || f & 0x04 != 0;
    let mip_lin = aniso || f & 0x01 != 0;
    let min = match (min_lin, mip_lin) {
        (false, false) => GL_NEAREST_MIPMAP_NEAREST,
        (true, false) => GL_LINEAR_MIPMAP_NEAREST,
        (false, true) => GL_NEAREST_MIPMAP_LINEAR,
        (true, true) => GL_LINEAR_MIPMAP_LINEAR,
    };
    (g.SamplerParameteri)(s, GL_TEXTURE_MIN_FILTER, min);
    (g.SamplerParameteri)(
        s,
        GL_TEXTURE_MAG_FILTER,
        if mag_lin { GL_LINEAR } else { GL_NEAREST },
    );
    (g.SamplerParameteri)(s, GL_TEXTURE_WRAP_S, address(d.address_u));
    (g.SamplerParameteri)(s, GL_TEXTURE_WRAP_T, address(d.address_v));
    (g.SamplerParameteri)(s, GL_TEXTURE_WRAP_R, address(d.address_w));
    (g.SamplerParameterf)(s, GL_TEXTURE_LOD_BIAS, d.mip_lod_bias);
    (g.SamplerParameterf)(s, GL_TEXTURE_MIN_LOD, d.min_lod);
    (g.SamplerParameterf)(s, GL_TEXTURE_MAX_LOD, d.max_lod);
    (g.SamplerParameterfv)(s, GL_TEXTURE_BORDER_COLOR, d.border.as_ptr());
    if aniso {
        (g.SamplerParameterf)(
            s,
            GL_TEXTURE_MAX_ANISOTROPY,
            d.max_anisotropy.clamp(1, 16) as f32,
        );
    }
    if f & 0x80 != 0 {
        (g.SamplerParameteri)(s, GL_TEXTURE_COMPARE_MODE, GL_COMPARE_REF_TO_TEXTURE);
        (g.SamplerParameteri)(
            s,
            GL_TEXTURE_COMPARE_FUNC,
            compare_func(d.comparison) as i32,
        );
    }
    s
}

/// Échantillonneur par défaut de Direct3D (linéaire, bords serrés).
pub(super) fn default_sampler_desc() -> SamplerDesc {
    SamplerDesc {
        filter: 0x15,
        address_u: 3,
        address_v: 3,
        address_w: 3,
        mip_lod_bias: 0.0,
        max_anisotropy: 1,
        comparison: 1,
        border: [1.0; 4],
        min_lod: -f32::MAX,
        max_lod: f32::MAX,
    }
}

// ────────────────────── Programmes ──────────────────────

/// Programmes liés, par (VS, PS, GS) : partagés entre contextes GL.
static PROGRAMS: Mutex<Option<HashMap<(u32, u32, u32), gl::GLuint>>> = Mutex::new(None);

fn forget_programs(shader: gl::GLuint) {
    let mut p = PROGRAMS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = p.as_mut() else { return };
    let gone: Vec<_> = map
        .keys()
        .filter(|k| k.0 == shader || k.1 == shader || k.2 == shader)
        .copied()
        .collect();
    for k in gone {
        if let (Some(prog), Some(g)) = (map.remove(&k), gl::gl()) {
            unsafe { (g.DeleteProgram)(prog) };
        }
    }
}

unsafe fn program(g: &gl::GlFunctions, vs: u32, ps: u32, gs: u32) -> gl::GLuint {
    let mut p = PROGRAMS.lock().unwrap_or_else(|e| e.into_inner());
    let map = p.get_or_insert_with(HashMap::new);
    if let Some(&prog) = map.get(&(vs, ps, gs)) {
        return prog;
    }
    let prog = (g.CreateProgram)();
    for s in [vs, ps, gs] {
        if s != 0 {
            (g.AttachShader)(prog, s);
        }
    }
    (g.LinkProgram)(prog);
    let mut ok = 0;
    (g.GetProgramiv)(prog, gl::GL_LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut len = 0;
        (g.GetProgramiv)(prog, gl::GL_INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(1) as usize];
        (g.GetProgramInfoLog)(prog, len, std::ptr::null_mut(), buf.as_mut_ptr().cast());
        log_warn!(
            "d3d11",
            "édition de liens refusée : {}",
            String::from_utf8_lossy(&buf)
        );
    }
    map.insert((vs, ps, gs), prog);
    prog
}

// ────────────────────── État du contexte ──────────────────────

#[derive(Clone, Copy, Default)]
pub(super) struct BoundShader {
    pub obj: usize,
    pub gl: gl::GLuint,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ConstantBuffer {
    pub buffer: gl::GLuint,
    /// Fenêtre (octets) des tampons de constantes D3D11.1 ; 0 : tout.
    pub offset: isize,
    pub size: isize,
}

#[derive(Clone)]
pub(super) struct State {
    pub shaders: [BoundShader; 3],
    pub cbs: [[ConstantBuffer; MAX_CB]; 3],
    pub srvs: [[gl::GLuint; MAX_SRV]; 3],
    pub samplers: [[usize; MAX_SAMPLER]; 3],
    pub vbs: [(gl::GLuint, u32, u32); MAX_VB],
    pub index_buffer: gl::GLuint,
    pub index_offset: u32,
    pub layout: usize,
    pub blend: usize,
    pub blend_factor: [f32; 4],
    pub raster: usize,
    pub depth: usize,
    pub stencil_ref: u32,
    pub scissors: Vec<[i32; 4]>,
}

impl Default for State {
    fn default() -> Self {
        State {
            shaders: [BoundShader::default(); 3],
            cbs: [[ConstantBuffer::default(); MAX_CB]; 3],
            srvs: [[0; MAX_SRV]; 3],
            samplers: [[0; MAX_SAMPLER]; 3],
            vbs: [(0, 0, 0); MAX_VB],
            index_buffer: 0,
            index_offset: 0,
            layout: 0,
            blend: 0,
            blend_factor: [1.0; 4],
            raster: 0,
            depth: 0,
            stencil_ref: 0,
            scissors: Vec::new(),
        }
    }
}

thread_local! {
    /// Un VAO par contexte GL (ils ne se partagent pas) : (contexte, VAO).
    /// Un thread peut changer de contexte, par exemple quand la chaîne
    /// d'échange est créée après le premier tirage (#509).
    static VAO: Cell<(usize, gl::GLuint)> = const { Cell::new((0, 0)) };
    /// Attributs activés au tirage précédent.
    static ENABLED: Cell<u32> = const { Cell::new(0) };
    /// Échantillonneur par défaut de ce contexte.
    static DEFAULT_SAMPLER: Cell<gl::GLuint> = const { Cell::new(0) };
}

fn blend_factor(f: u32) -> u32 {
    match f {
        1 => 0,
        2 => 1,
        3 => 0x0300,
        4 => 0x0301,
        5 => 0x0302,
        6 => 0x0303,
        7 => 0x0304,
        8 => 0x0305,
        9 => 0x0306,
        10 => 0x0307,
        11 => 0x0308,
        14 => 0x8001,
        15 => 0x8002,
        16 => 0x88F9,
        17 => 0x88FA,
        18 => 0x8589,
        19 => 0x88FB,
        _ => 1,
    }
}

fn blend_op(op: u32) -> u32 {
    match op {
        2 => GL_FUNC_SUBTRACT,
        3 => GL_FUNC_REVERSE_SUBTRACT,
        4 => GL_MIN,
        5 => GL_MAX,
        _ => GL_FUNC_ADD,
    }
}

fn stencil_op(op: u32) -> u32 {
    match op {
        2 => 0,
        3 => GL_REPLACE,
        4 => GL_INCR,
        5 => GL_DECR,
        6 => GL_INVERT,
        7 => GL_INCR_WRAP,
        8 => GL_DECR_WRAP,
        _ => GL_KEEP,
    }
}

fn desc_of<T: Copy + Default>(obj: usize, pick: impl Fn(&Meta) -> Option<T>) -> T {
    meta(obj as *mut c_void)
        .and_then(|m| pick(&m))
        .unwrap_or_default()
}

unsafe fn apply_fixed_function(g: &gl::GlFunctions, s: &State) {
    // Rastérisation.
    let r = desc_of(s.raster, |m| match m {
        Meta::Raster(r) => Some(*r),
        _ => None,
    });
    (g.PolygonMode)(
        GL_FRONT_AND_BACK,
        if r.fill == 2 { GL_LINE } else { GL_FILL },
    );
    match r.cull {
        2 | 3 => {
            (g.Enable)(GL_CULL_FACE);
            (g.CullFace)(if r.cull == 2 { GL_FRONT } else { GL_BACK });
        }
        _ => (g.Disable)(GL_CULL_FACE),
    }
    // Y inversé : le sens des faces s'inverse aussi.
    (g.FrontFace)(if r.front_ccw != 0 { GL_CW } else { GL_CCW });
    if r.depth_bias != 0 || r.slope_scaled_bias != 0.0 {
        (g.Enable)(GL_POLYGON_OFFSET_FILL);
        (g.PolygonOffset)(r.slope_scaled_bias, r.depth_bias as f32);
    } else {
        (g.Disable)(GL_POLYGON_OFFSET_FILL);
    }
    if r.scissor != 0 {
        (g.Enable)(GL_SCISSOR_TEST);
        if let Some(sc) = s.scissors.first() {
            (g.Scissor)(sc[0], sc[1], (sc[2] - sc[0]).max(0), (sc[3] - sc[1]).max(0));
        }
    } else {
        (g.Disable)(GL_SCISSOR_TEST);
    }

    // Profondeur et pochoir.
    let d = desc_of(s.depth, |m| match m {
        Meta::Depth(d) => Some(*d),
        _ => None,
    });
    if d.depth_enable != 0 {
        (g.Enable)(GL_DEPTH_TEST);
        (g.DepthFunc)(compare_func(d.func));
    } else {
        (g.Disable)(GL_DEPTH_TEST);
    }
    (g.DepthMask)(u8::from(d.depth_enable != 0 && d.write_mask != 0));
    if d.stencil_enable != 0 {
        (g.Enable)(GL_STENCIL_TEST);
        for (face, f) in [(GL_FRONT, d.front), (GL_BACK, d.back)] {
            (g.StencilFuncSeparate)(
                face,
                compare_func(f.func),
                s.stencil_ref as i32,
                d.read_mask as u32,
            );
            (g.StencilOpSeparate)(
                face,
                stencil_op(f.fail),
                stencil_op(f.depth_fail),
                stencil_op(f.pass),
            );
        }
        (g.StencilMask)(d.write_mask_stencil as u32);
    } else {
        (g.Disable)(GL_STENCIL_TEST);
    }

    // Mélange (cible 0 ; masques d'écriture par cible).
    let b = desc_of(s.blend, |m| match m {
        Meta::Blend(b) => Some(**b),
        _ => None,
    });
    let rt = b.rt[0];
    if rt.enable != 0 {
        (g.Enable)(GL_BLEND);
        (g.BlendFuncSeparate)(
            blend_factor(rt.src),
            blend_factor(rt.dst),
            blend_factor(rt.src_alpha),
            blend_factor(rt.dst_alpha),
        );
        (g.BlendEquationSeparate)(blend_op(rt.op), blend_op(rt.op_alpha));
        let f = s.blend_factor;
        (g.BlendColor)(f[0], f[1], f[2], f[3]);
    } else {
        (g.Disable)(GL_BLEND);
    }
    for (i, t) in b.rt.iter().enumerate() {
        let m = if b.independent != 0 || i == 0 {
            t.write_mask
        } else {
            rt.write_mask
        };
        (g.ColorMaski)(
            i as u32,
            (m & 1) as u8,
            (m >> 1 & 1) as u8,
            (m >> 2 & 1) as u8,
            (m >> 3 & 1) as u8,
        );
    }
}

/// Lie programme, sommets, tampons de constantes, textures et états avant
/// un tirage. Faux si aucun vertex shader traduit n'est en place.
pub(super) unsafe fn prepare(g: &gl::GlFunctions, s: &State) -> bool {
    let [vs, ps, gs] = s.shaders;
    if vs.gl == 0 {
        return false;
    }
    let prog = program(g, vs.gl, ps.gl, gs.gl);
    (g.UseProgram)(prog);

    // Sommets.
    let vao = VAO.with(|v| {
        let context = crate::dlls::dxgi::current_binding().context;
        let (owner, id) = v.get();
        if id != 0 && owner == context {
            return id;
        }
        let mut id = 0;
        (g.GenVertexArrays)(1, &mut id);
        v.set((context, id));
        // Nouveau VAO : aucun attribut n'y est encore activé.
        ENABLED.with(|e| e.set(0));
        id
    });
    (g.BindVertexArray)(vao);
    let inputs: Vec<SigElement> = match meta(vs.obj as *mut c_void).as_deref() {
        Some(Meta::Shader { inputs, .. }) => inputs.clone(),
        _ => Vec::new(),
    };
    let layout: Vec<LayoutElement> = match meta(s.layout as *mut c_void).as_deref() {
        Some(Meta::Layout(l)) => l.clone(),
        _ => Vec::new(),
    };
    let mut enabled = 0u32;
    for e in &layout {
        let Some(sig) = inputs
            .iter()
            .find(|i| i.name == e.name && i.index == e.index && i.system_value == 0)
        else {
            continue;
        };
        let loc = sig.register;
        if loc >= 32 {
            continue;
        }
        let (buf, stride, offset) = s.vbs[e.slot as usize];
        if buf == 0 {
            continue;
        }
        (g.BindBuffer)(GL_ARRAY_BUFFER, buf);
        let ptr = (offset as usize + e.offset as usize) as *const c_void;
        let f = e.format;
        let comps = if f.bgra { GL_BGRA } else { f.components };
        if f.integer {
            (g.VertexAttribIPointer)(loc, comps, f.gl_type, stride as i32, ptr);
        } else {
            (g.VertexAttribPointer)(
                loc,
                comps,
                f.gl_type,
                u8::from(f.normalized),
                stride as i32,
                ptr,
            );
        }
        (g.VertexAttribDivisor)(
            loc,
            if e.per_instance {
                e.step_rate.max(1)
            } else {
                0
            },
        );
        (g.EnableVertexAttribArray)(loc);
        enabled |= 1 << loc;
    }
    let previously = ENABLED.with(|c| c.replace(enabled));
    for loc in 0..32 {
        if previously & !enabled & (1 << loc) != 0 {
            (g.DisableVertexAttribArray)(loc);
        }
    }
    (g.BindBuffer)(GL_ELEMENT_ARRAY_BUFFER, s.index_buffer);

    // Ressources par étage.
    let default_sampler = DEFAULT_SAMPLER.with(|c| {
        if c.get() == 0 {
            c.set(create_sampler(g, &default_sampler_desc()));
        }
        c.get()
    });
    for (i, st) in STAGES.iter().enumerate() {
        let sh = s.shaders[i];
        if sh.gl == 0 {
            continue;
        }
        let base = st.binding_base();
        for (slot, cb) in s.cbs[i].iter().enumerate() {
            if cb.buffer == 0 {
                continue;
            }
            if cb.size > 0 {
                (g.BindBufferRange)(
                    GL_UNIFORM_BUFFER,
                    base + slot as u32,
                    cb.buffer,
                    cb.offset,
                    cb.size,
                );
            } else {
                (g.BindBufferBase)(GL_UNIFORM_BUFFER, base + slot as u32, cb.buffer);
            }
        }
        let pairs: Vec<(u32, u32)> = match meta(sh.obj as *mut c_void).as_deref() {
            Some(Meta::Shader { pairs, .. }) => pairs.clone(),
            _ => Vec::new(),
        };
        for (t, &tex) in s.srvs[i].iter().enumerate() {
            let unit = base + t as u32;
            (g.ActiveTexture)(gl::GL_TEXTURE0 + unit);
            (g.BindTexture)(GL_TEXTURE_2D, tex);
            let sampler_slot = pairs.iter().find(|p| p.0 == t as u32).map(|p| p.1 as usize);
            let sampler = sampler_slot
                .and_then(|n| s.samplers[i].get(n).copied())
                .and_then(|obj| match meta(obj as *mut c_void).as_deref() {
                    Some(Meta::Sampler(id)) => Some(*id),
                    _ => None,
                })
                .unwrap_or(default_sampler);
            (g.BindSampler)(unit, sampler);
        }
    }
    (g.ActiveTexture)(gl::GL_TEXTURE0);

    apply_fixed_function(g, s);
    true
}

/// Mode GL d'une topologie D3D11 (avec adjacence).
pub(super) fn topology(t: u32) -> u32 {
    match t {
        1 => gl::GL_POINTS,
        2 => gl::GL_LINES,
        3 => gl::GL_LINE_STRIP,
        5 => gl::GL_TRIANGLE_STRIP,
        10 => GL_LINES_ADJACENCY,
        11 => GL_LINE_STRIP_ADJACENCY,
        12 => GL_TRIANGLES_ADJACENCY,
        13 => GL_TRIANGLE_STRIP_ADJACENCY,
        _ => gl::GL_TRIANGLES,
    }
}

/// Copie GPU → mémoire d'une texture 2D (niveau 0), par un cadre de lecture.
pub(super) unsafe fn read_texture(g: &gl::GlFunctions, tex: &Texture2D) -> Option<Vec<u8>> {
    let bytes = texture_bytes(&tex.desc)?;
    if tex.gl_texture == 0 {
        return None;
    }
    let (_, format, ty) = dxgi_format_to_gl(tex.desc.format);
    let mut out = vec![0u8; bytes];
    let mut fbo = 0;
    let mut previous = 0;
    (g.GetIntegerv)(0x8CAA /* GL_READ_FRAMEBUFFER_BINDING */, &mut previous);
    (g.GenFramebuffers)(1, &mut fbo);
    (g.BindFramebuffer)(GL_READ_FRAMEBUFFER, fbo);
    (g.FramebufferTexture2D)(
        GL_READ_FRAMEBUFFER,
        GL_COLOR_ATTACHMENT0,
        GL_TEXTURE_2D,
        tex.gl_texture,
        0,
    );
    (g.PixelStorei)(0x0D05 /* GL_PACK_ALIGNMENT */, 1);
    (g.ReadPixels)(
        0,
        0,
        tex.desc.width as i32,
        tex.desc.height as i32,
        format,
        ty,
        out.as_mut_ptr().cast(),
    );
    (g.BindFramebuffer)(GL_READ_FRAMEBUFFER, previous as u32);
    (g.DeleteFramebuffers)(1, &fbo);
    Some(out)
}

/// Limite les niveaux de mip d'une texture à ceux qui existent : sans cela
/// un échantillonneur à mips voit une texture incomplète (noire).
pub(super) unsafe fn set_max_level(g: &gl::GlFunctions, tex: gl::GLuint, levels: i32) {
    (g.BindTexture)(GL_TEXTURE_2D, tex);
    (g.TexParameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAX_LEVEL, levels.max(0));
    (g.BindTexture)(GL_TEXTURE_2D, 0);
}

/// Rattache la texture d'une vue de profondeur au cadre lié (ou la détache).
pub(super) unsafe fn attach_depth(g: &gl::GlFunctions, dsv: *mut SimpleComObj) {
    const GL_FRAMEBUFFER: u32 = 0x8D40;
    const GL_DEPTH_ATTACHMENT: u32 = 0x8D00;
    const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
    let tex = if dsv.is_null() {
        None
    } else {
        as_texture2d((*dsv).view_resource)
    };
    let (id, stencil) = match tex {
        Some(t) => ((*t).gl_texture, super::dxgi_has_stencil((*t).desc.format)),
        None => (0, false),
    };
    (g.FramebufferTexture2D)(
        GL_FRAMEBUFFER,
        GL_DEPTH_STENCIL_ATTACHMENT,
        GL_TEXTURE_2D,
        0,
        0,
    );
    let point = if stencil {
        GL_DEPTH_STENCIL_ATTACHMENT
    } else {
        GL_DEPTH_ATTACHMENT
    };
    (g.FramebufferTexture2D)(GL_FRAMEBUFFER, point, GL_TEXTURE_2D, id, 0);
    // Cadre incomplet : plus rien ne se dessine. Signalé une fois par état.
    static LAST: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let status = (g.CheckFramebufferStatus)(GL_FRAMEBUFFER);
    if status != gl::GL_FRAMEBUFFER_COMPLETE
        && LAST.swap(status, std::sync::atomic::Ordering::Relaxed) != status
    {
        let format = tex.map_or(0, |t| (*t).desc.format);
        log_warn!(
            "d3d11",
            "cadre de rendu incomplet (0x{status:04X}), profondeur au format DXGI {format}"
        );
    }
}

/// Effacement à la manière de D3D : sans ciseaux. Les masques changés par
/// `f` sont rétablis au tirage suivant (apply_fixed_function).
pub(super) unsafe fn clear(g: &gl::GlFunctions, f: impl FnOnce()) {
    (g.Disable)(GL_SCISSOR_TEST);
    f();
}

// ────────────────────── Méthodes du contexte ──────────────────────

unsafe fn with_state(this: *mut DeviceContext, f: impl FnOnce(&mut State)) {
    let mut s = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut s.pipe);
}

unsafe fn set_cbs(
    this: *mut DeviceContext,
    stage: usize,
    start: u32,
    n: u32,
    bufs: *const *mut c_void,
) {
    with_state(this, |s| {
        for i in 0..n as usize {
            let slot = start as usize + i;
            if slot >= MAX_CB {
                break;
            }
            let b = if bufs.is_null() {
                std::ptr::null_mut()
            } else {
                *bufs.add(i)
            };
            s.cbs[stage][slot] = ConstantBuffer {
                buffer: as_buffer(b).map_or(0, |b| (*b).gl_buffer),
                offset: 0,
                size: 0,
            };
        }
    });
}

unsafe fn set_srvs(
    this: *mut DeviceContext,
    stage: usize,
    start: u32,
    n: u32,
    views: *const *mut c_void,
) {
    with_state(this, |s| {
        for i in 0..n as usize {
            let slot = start as usize + i;
            if slot >= MAX_SRV {
                break;
            }
            let v = if views.is_null() {
                std::ptr::null_mut()
            } else {
                *views.add(i)
            };
            s.srvs[stage][slot] = as_simple(v).map_or(0, |o| (*o).resource_tex);
        }
    });
}

unsafe fn set_samplers(
    this: *mut DeviceContext,
    stage: usize,
    start: u32,
    n: u32,
    smp: *const *mut c_void,
) {
    with_state(this, |s| {
        for i in 0..n as usize {
            let slot = start as usize + i;
            if slot >= MAX_SAMPLER {
                break;
            }
            s.samplers[stage][slot] = if smp.is_null() {
                0
            } else {
                *smp.add(i) as usize
            };
        }
    });
}

macro_rules! per_stage {
    ($cb:ident, $srv:ident, $smp:ident, $n:expr) => {
        pub(super) unsafe extern "win64" fn $cb(
            this: *mut DeviceContext,
            start: u32,
            n: u32,
            b: *const *mut c_void,
        ) {
            set_cbs(this, $n, start, n, b)
        }
        pub(super) unsafe extern "win64" fn $srv(
            this: *mut DeviceContext,
            start: u32,
            n: u32,
            v: *const *mut c_void,
        ) {
            set_srvs(this, $n, start, n, v)
        }
        pub(super) unsafe extern "win64" fn $smp(
            this: *mut DeviceContext,
            start: u32,
            n: u32,
            v: *const *mut c_void,
        ) {
            set_samplers(this, $n, start, n, v)
        }
    };
}
per_stage!(vs_set_cbs, vs_set_srvs, vs_set_samplers, 0);
per_stage!(ps_set_cbs, ps_set_srvs, ps_set_samplers, 1);
per_stage!(gs_set_cbs, gs_set_srvs, gs_set_samplers, 2);

pub(super) unsafe fn set_shader(this: *mut DeviceContext, stage: usize, shader: *mut c_void) {
    let gl_id = as_simple(shader).map_or(0, |o| (*o).gl_id);
    with_state(this, |s| {
        s.shaders[stage] = BoundShader {
            obj: shader as usize,
            gl: gl_id,
        }
    });
}

pub(super) unsafe extern "win64" fn gs_set_shader(
    this: *mut DeviceContext,
    shader: *mut c_void,
    _class: *const *mut c_void,
    _n: u32,
) {
    set_shader(this, 2, shader);
}

pub(super) unsafe extern "win64" fn ia_set_input_layout(
    this: *mut DeviceContext,
    layout: *mut c_void,
) {
    with_state(this, |s| s.layout = layout as usize);
}

pub(super) unsafe extern "win64" fn ia_set_vertex_buffers(
    this: *mut DeviceContext,
    start: u32,
    n: u32,
    bufs: *const *mut c_void,
    strides: *const u32,
    offsets: *const u32,
) {
    with_state(this, |s| {
        for i in 0..n as usize {
            let slot = start as usize + i;
            if slot >= MAX_VB {
                break;
            }
            let b = if bufs.is_null() {
                std::ptr::null_mut()
            } else {
                *bufs.add(i)
            };
            let stride = if strides.is_null() {
                0
            } else {
                *strides.add(i)
            };
            let offset = if offsets.is_null() {
                0
            } else {
                *offsets.add(i)
            };
            s.vbs[slot] = (as_buffer(b).map_or(0, |b| (*b).gl_buffer), stride, offset);
        }
    });
}

pub(super) unsafe extern "win64" fn ia_set_index_buffer(
    this: *mut DeviceContext,
    buffer: *mut c_void,
    format: u32,
    offset: u32,
) {
    let mut st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    st.index_format = format;
    st.pipe.index_buffer = as_buffer(buffer).map_or(0, |b| (*b).gl_buffer);
    st.pipe.index_offset = offset;
}

pub(super) unsafe extern "win64" fn om_set_blend_state(
    this: *mut DeviceContext,
    state: *mut c_void,
    factor: *const f32,
    _mask: u32,
) {
    with_state(this, |s| {
        s.blend = state as usize;
        s.blend_factor = if factor.is_null() {
            [1.0; 4]
        } else {
            [*factor, *factor.add(1), *factor.add(2), *factor.add(3)]
        };
    });
}

pub(super) unsafe extern "win64" fn om_set_depth_stencil_state(
    this: *mut DeviceContext,
    state: *mut c_void,
    r: u32,
) {
    with_state(this, |s| {
        s.depth = state as usize;
        s.stencil_ref = r;
    });
}

pub(super) unsafe extern "win64" fn rs_set_state(this: *mut DeviceContext, state: *mut c_void) {
    with_state(this, |s| s.raster = state as usize);
}

pub(super) unsafe extern "win64" fn rs_set_scissor_rects(
    this: *mut DeviceContext,
    n: u32,
    rects: *const c_void,
) {
    with_state(this, |s| {
        s.scissors.clear();
        let r = rects as *const [i32; 4];
        for i in 0..n.min(16) as usize {
            if !r.is_null() {
                s.scissors.push(*r.add(i));
            }
        }
    });
}

pub(super) unsafe extern "win64" fn rs_set_viewports(
    this: *mut DeviceContext,
    n: u32,
    vps: *const D3D11Viewport,
) {
    let Some(g) = gl_here() else { return };
    let mut st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let n = n.min(16);
    for i in 0..n as usize {
        st.viewports[i] = *vps.add(i);
    }
    st.num_viewports = n;
    if n > 0 && !vps.is_null() {
        let vp = &*vps;
        // Ligne 0 en haut (voir l'en-tête) : pas de conversion de Y.
        (g.Viewport)(
            vp.top_left_x as i32,
            vp.top_left_y as i32,
            vp.width as i32,
            vp.height as i32,
        );
        (g.DepthRange)(vp.min_depth as f64, vp.max_depth as f64);
    }
}

enum Draw {
    Arrays {
        count: u32,
        start: u32,
        instances: u32,
        base_instance: u32,
    },
    Indexed {
        count: u32,
        start: u32,
        base_vertex: i32,
        instances: u32,
        base_instance: u32,
    },
}

unsafe fn draw(this: *mut DeviceContext, d: Draw) {
    let Some(g) = gl_here() else { return };
    let st = (*this).state.lock().unwrap_or_else(|e| e.into_inner());
    let fbo = if st.current_rtv.is_null() {
        0
    } else {
        (*st.current_rtv).gl_id
    };
    (g.BindFramebuffer)(0x8D40 /* GL_FRAMEBUFFER */, fbo);
    if !prepare(g, &st.pipe) {
        return;
    }
    let mode = topology(st.topology);
    match d {
        Draw::Arrays {
            count,
            start,
            instances,
            base_instance,
        } => (g.DrawArraysInstancedBaseInstance)(
            mode,
            start as i32,
            count as i32,
            instances as i32,
            base_instance,
        ),
        Draw::Indexed {
            count,
            start,
            base_vertex,
            instances,
            base_instance,
        } => {
            let (ty, size) = index_type(st.index_format);
            let offset = (st.pipe.index_offset as usize + start as usize * size) as *const c_void;
            (g.DrawElementsInstancedBaseVertexBaseInstance)(
                mode,
                count as i32,
                ty,
                offset,
                instances as i32,
                base_vertex,
                base_instance,
            )
        }
    }
}

pub(super) unsafe extern "win64" fn ctx_draw(this: *mut DeviceContext, count: u32, start: u32) {
    draw(
        this,
        Draw::Arrays {
            count,
            start,
            instances: 1,
            base_instance: 0,
        },
    );
}

pub(super) unsafe extern "win64" fn ctx_draw_indexed(
    this: *mut DeviceContext,
    count: u32,
    start: u32,
    base_vertex: i32,
) {
    draw(
        this,
        Draw::Indexed {
            count,
            start,
            base_vertex,
            instances: 1,
            base_instance: 0,
        },
    );
}

pub(super) unsafe extern "win64" fn ctx_draw_instanced(
    this: *mut DeviceContext,
    count: u32,
    instances: u32,
    start: u32,
    base_instance: u32,
) {
    draw(
        this,
        Draw::Arrays {
            count,
            start,
            instances,
            base_instance,
        },
    );
}

pub(super) unsafe extern "win64" fn ctx_draw_indexed_instanced(
    this: *mut DeviceContext,
    count: u32,
    instances: u32,
    start: u32,
    base_vertex: i32,
    base_instance: u32,
) {
    draw(
        this,
        Draw::Indexed {
            count,
            start,
            base_vertex,
            instances,
            base_instance,
        },
    );
}

// ────────────────────── Objets d'état du périphérique ──────────────────────

unsafe fn state_object(
    desc: *const c_void,
    out: *mut *mut c_void,
    make: impl FnOnce(*const c_void) -> Option<Meta>,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if desc.is_null() {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let obj = create_dummy_com_obj();
    match make(desc) {
        Some(m) => register(obj, m),
        None => {
            com_release(obj);
            *out = std::ptr::null_mut();
            return E_FAIL;
        }
    }
    *out = obj;
    S_OK
}

pub(super) unsafe extern "win64" fn dev_create_blend(
    _: *mut Device,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    state_object(desc, out, |d| {
        Some(Meta::Blend(Box::new(*(d as *const BlendDesc))))
    })
}

pub(super) unsafe extern "win64" fn dev_create_raster(
    _: *mut Device,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    state_object(desc, out, |d| Some(Meta::Raster(*(d as *const RasterDesc))))
}

pub(super) unsafe extern "win64" fn dev_create_depth(
    _: *mut Device,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    state_object(desc, out, |d| Some(Meta::Depth(*(d as *const DepthDesc))))
}

pub(super) unsafe extern "win64" fn dev_create_sampler(
    _: *mut Device,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    state_object(desc, out, |d| {
        let g = gl_here()?;
        let id = create_sampler(g, &*(d as *const SamplerDesc));
        (id != 0).then_some(Meta::Sampler(id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_de_sommets() {
        let f = vertex_format(DXGI_FORMAT_R32G32B32_FLOAT).unwrap();
        assert_eq!(
            (f.components, f.gl_type, f.size, f.integer),
            (3, GL_FLOAT, 12, false)
        );
        let f = vertex_format(DXGI_FORMAT_R8G8B8A8_UNORM).unwrap();
        assert!(f.normalized && !f.integer && f.size == 4);
        let f = vertex_format(87).unwrap();
        assert!(f.bgra);
        let f = vertex_format(30).unwrap();
        assert!(f.integer && !f.normalized);
        assert!(vertex_format(999).is_none());
    }

    #[test]
    fn mise_en_page_alignee() {
        let pos = c"POSITION";
        let tex = c"texcoord";
        let descs = [
            D3D11InputElementDesc {
                semantic_name: pos.as_ptr().cast(),
                semantic_index: 0,
                format: DXGI_FORMAT_R32G32B32_FLOAT,
                input_slot: 0,
                aligned_byte_offset: 0,
                input_slot_class: 0,
                instance_data_step_rate: 0,
            },
            D3D11InputElementDesc {
                semantic_name: tex.as_ptr().cast(),
                semantic_index: 1,
                format: DXGI_FORMAT_R32G32_FLOAT,
                input_slot: 0,
                aligned_byte_offset: u32::MAX,
                input_slot_class: 1,
                instance_data_step_rate: 2,
            },
        ];
        let l = unsafe { layout_from(descs.as_ptr(), 2) };
        assert_eq!(l.len(), 2);
        assert_eq!(
            (l[1].name.as_str(), l[1].index, l[1].offset),
            ("TEXCOORD", 1, 12)
        );
        assert!(l[1].per_instance && l[1].step_rate == 2);
    }

    #[test]
    fn tailles_des_descripteurs() {
        // Tailles des structures D3D11 lues telles quelles.
        assert_eq!(std::mem::size_of::<BlendDesc>(), 8 + 8 * 32);
        assert_eq!(std::mem::size_of::<RasterDesc>(), 40);
        assert_eq!(std::mem::size_of::<DepthDesc>(), 52);
        assert_eq!(std::mem::size_of::<SamplerDesc>(), 52);
    }

    #[test]
    fn conversions_d_etat() {
        assert_eq!(compare_func(1), 0x0200);
        assert_eq!(compare_func(8), 0x0207);
        assert_eq!(blend_factor(6), 0x0303);
        assert_eq!(stencil_op(3), GL_REPLACE);
        assert_eq!(address(4), GL_CLAMP_TO_BORDER);
        assert_eq!(topology(12), GL_TRIANGLES_ADJACENCY);
    }
}
