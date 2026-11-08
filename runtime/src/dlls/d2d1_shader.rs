//! Exécution des pixel shaders des effets Direct2D personnalisés (#561).
//!
//! Un effet écrit pour Direct2D 1.1 (ComputeSharp.D2D1 pour Paint.NET, par
//! exemple) fournit un pixel shader DXBC compilé selon les conventions
//! publiques de `d2d1effecthelpers.hlsli` : entrée N lue dans `tN` avec
//! l'échantillonneur `sN`, coordonnée `TEXCOORDN` (xy : position dans la
//! texture de l'entrée, zw : taille d'un pixel de la scène dans cette
//! texture), position de scène `SCENE_POSITION` quand l'effet la demande,
//! tampon de constantes `b0`.
//!
//! Le shader passe par la traduction DXBC → GLSL de la couche D3D11, avec un
//! vertex shader écrit ici qui couvre le rectangle de sortie. Les pixels
//! entrent et sortent en BGRA prémultiplié, comme dans le reste de Direct2D.

use super::{compile_glsl, gl, gl_here};
use crate::dlls::dxbc;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Mutex;

/// Une entrée de l'effet : pixels BGRA prémultipliés, lus à travers le
/// rectangle `rect` (origine et taille dans l'espace de la scène).
pub(crate) struct Input<'a> {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub px: &'a [u8],
    /// Filtre bilinéaire (sinon plus proche voisin).
    pub linear: bool,
}

/// Programmes liés, par empreinte du bytecode.
static PROGRAMS: Mutex<Option<HashMap<u64, u32>>> = Mutex::new(None);

fn hash(b: &[u8]) -> u64 {
    // FNV-1a : suffisant pour reconnaître un bytecode déjà lié.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h ^ b.len() as u64
}

/// Vertex shader qui couvre la sortie et donne à chaque entrée du pixel
/// shader la valeur que Direct2D lui fournit.
fn vertex_glsl(ps: &dxbc::Shader, inputs: usize) -> String {
    let mut s = String::from("#version 430 core\n");
    s.push_str("uniform vec4 d2d_out;\n"); // origine (xy) et taille (zw) de la sortie
    let _ = writeln!(s, "uniform vec4 d2d_in[{}];", inputs.max(1)); // idem par entrée
    let mut body = String::new();
    let mut declared = Vec::new();
    for e in &ps.inputs {
        if e.system_value != 0 {
            continue;
        }
        let loc = dxbc::varying_location(&e.name, e.index);
        if declared.contains(&loc) {
            continue;
        }
        declared.push(loc);
        let flat = if e.component_type != 3 { "flat " } else { "" };
        let _ = writeln!(s, "layout(location = {loc}) {flat}out vec4 vary{loc};");
        let value = match e.name.as_str() {
            "SCENE_POSITION" => "vec4(scene, 0.0, 1.0)".to_string(),
            "TEXCOORD" if (e.index as usize) < inputs => {
                let i = e.index;
                format!("vec4((scene - d2d_in[{i}].xy) / d2d_in[{i}].zw, 1.0 / d2d_in[{i}].zw)")
            }
            _ => "vec4(0.0)".to_string(),
        };
        let _ = writeln!(body, "  vary{loc} = {value};");
    }
    s.push_str("void main() {\n");
    s.push_str("  vec2 c = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));\n");
    s.push_str("  vec2 scene = d2d_out.xy + c * d2d_out.zw;\n");
    s.push_str(&body);
    s.push_str("  gl_Position = vec4(c * 2.0 - 1.0, 0.0, 1.0);\n}\n");
    s
}

unsafe fn program(g: &gl::GlFunctions, bytecode: &[u8], inputs: usize) -> u32 {
    let key = hash(bytecode) ^ (inputs as u64).rotate_left(56);
    let mut p = PROGRAMS.lock().unwrap_or_else(|e| e.into_inner());
    let map = p.get_or_insert_with(HashMap::new);
    if let Some(&prog) = map.get(&key) {
        return prog;
    }
    let prog = (|| {
        let ps = match dxbc::translate(bytecode) {
            Ok(s) => s,
            Err(e) => {
                crate::log_warn!("d2d1", "pixel shader d'effet non traduit : {e}");
                return 0;
            }
        };
        let vs = vertex_glsl(&ps, inputs);
        let (Some(v), Some(f)) = (
            compile_glsl(g, gl::GL_VERTEX_SHADER, &vs),
            compile_glsl(g, gl::GL_FRAGMENT_SHADER, &ps.glsl),
        ) else {
            return 0;
        };
        let prog = (g.CreateProgram)();
        (g.AttachShader)(prog, v);
        (g.AttachShader)(prog, f);
        (g.LinkProgram)(prog);
        (g.DeleteShader)(v);
        (g.DeleteShader)(f);
        let mut ok = 0;
        (g.GetProgramiv)(prog, gl::GL_LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            (g.GetProgramiv)(prog, gl::GL_INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len.max(1) as usize];
            (g.GetProgramInfoLog)(prog, len, std::ptr::null_mut(), buf.as_mut_ptr().cast());
            crate::log_warn!(
                "d2d1",
                "pixel shader d'effet refusé à l'édition de liens : {}",
                String::from_utf8_lossy(&buf)
            );
            (g.DeleteProgram)(prog);
            return 0;
        }
        prog
    })();
    map.insert(key, prog);
    prog
}

const GL_VIEWPORT: u32 = 0x0BA2;
const GL_CURRENT_PROGRAM: u32 = 0x8B8D;
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_READ_FRAMEBUFFER_BINDING: u32 = 0x8CAA;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_ACTIVE_TEXTURE: u32 = 0x84E0;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
const GL_VERTEX_ARRAY_BINDING: u32 = 0x85B5;
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_UNPACK_BUFFER: u32 = 0x88EC;
const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
const GL_PIXEL_UNPACK_BUFFER_BINDING: u32 = 0x88EF;
const GL_UNIFORM_BUFFER_BINDING: u32 = 0x8A28;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_CLAMP_TO_BORDER: i32 = 0x812D;
const GL_TEXTURE_MAX_LEVEL: u32 = 0x813D;
const GL_STREAM_DRAW: u32 = 0x88E0;
const GL_RASTERIZER_DISCARD: u32 = 0x8C89;
const GL_FRAMEBUFFER_SRGB: u32 = 0x8DB9;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

/// Calcule `w × h` pixels de sortie (origine `x, y` dans la scène) avec le
/// pixel shader `bytecode`. `None` si le shader ne passe pas sur OpenGL.
pub(crate) unsafe fn run(
    bytecode: &[u8],
    inputs: &[Input],
    constants: &[u8],
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || inputs.len() > 16 {
        return None;
    }
    // Un contexte de dessin Direct2D sans appareil Direct3D n'a pas encore
    // de contexte OpenGL.
    crate::dlls::dxgi::ensure_offscreen_context();
    let g = gl_here()?;
    let prog = program(g, bytecode, inputs.len());
    if prog == 0 {
        return None;
    }
    let get = |e: u32| {
        let mut v = [0i32; 4];
        (g.GetIntegerv)(e, v.as_mut_ptr());
        v
    };
    // État d'OpenGL que la couche D3D11 pourrait ne pas reposer elle-même.
    let saved_prog = get(GL_CURRENT_PROGRAM)[0];
    let saved_draw = get(GL_DRAW_FRAMEBUFFER_BINDING)[0];
    let saved_read = get(GL_READ_FRAMEBUFFER_BINDING)[0];
    let saved_vp = get(GL_VIEWPORT);
    let saved_active = get(GL_ACTIVE_TEXTURE)[0];
    let saved_vao = get(GL_VERTEX_ARRAY_BINDING)[0];
    let saved_pack = get(GL_PIXEL_PACK_BUFFER_BINDING)[0];
    let saved_unpack = get(GL_PIXEL_UNPACK_BUFFER_BINDING)[0];
    let saved_ubo = get(GL_UNIFORM_BUFFER_BINDING)[0];
    let caps = [
        gl::GL_BLEND,
        gl::GL_DEPTH_TEST,
        gl::GL_STENCIL_TEST,
        gl::GL_SCISSOR_TEST,
        gl::GL_CULL_FACE,
        GL_RASTERIZER_DISCARD,
        GL_FRAMEBUFFER_SRGB,
    ];
    let saved_caps: Vec<i32> = caps.iter().map(|&c| get(c)[0]).collect();
    let base = dxbc::Stage::Pixel.binding_base();
    let mut saved_tex = Vec::new();
    for i in 0..inputs.len() as u32 {
        (g.ActiveTexture)(GL_TEXTURE0 + base + i);
        saved_tex.push(get(GL_TEXTURE_BINDING_2D)[0]);
    }

    for &c in &caps {
        (g.Disable)(c);
    }
    (g.BindBuffer)(GL_PIXEL_PACK_BUFFER, 0);
    (g.BindBuffer)(GL_PIXEL_UNPACK_BUFFER, 0);
    (g.PixelStorei)(0x0CF5, 1); // GL_UNPACK_ALIGNMENT
    (g.PixelStorei)(0x0D05, 1); // GL_PACK_ALIGNMENT

    // Entrées.
    let mut textures = vec![0u32; inputs.len()];
    if !inputs.is_empty() {
        (g.GenTextures)(inputs.len() as i32, textures.as_mut_ptr());
    }
    for (i, (inp, &t)) in inputs.iter().zip(&textures).enumerate() {
        (g.ActiveTexture)(GL_TEXTURE0 + base + i as u32);
        (g.BindSampler)(base + i as u32, 0);
        (g.BindTexture)(gl::GL_TEXTURE_2D, t);
        let ok = inp.px.len() == inp.w as usize * inp.h as usize * 4 && inp.w > 0 && inp.h > 0;
        let (iw, ih) = if ok { (inp.w, inp.h) } else { (1, 1) };
        let zero = [0u8; 4];
        (g.TexImage2D)(
            gl::GL_TEXTURE_2D,
            0,
            gl::GL_RGBA8 as i32,
            iw as i32,
            ih as i32,
            0,
            gl::GL_BGRA,
            gl::GL_UNSIGNED_BYTE,
            if ok { inp.px.as_ptr() } else { zero.as_ptr() }.cast(),
        );
        let filter = if inp.linear {
            gl::GL_LINEAR
        } else {
            gl::GL_NEAREST
        };
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MIN_FILTER, filter);
        (g.TexParameteri)(gl::GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, filter);
        (g.TexParameteri)(gl::GL_TEXTURE_2D, GL_TEXTURE_MAX_LEVEL, 0);
        // Hors de son rectangle, une entrée Direct2D est transparente : la
        // couleur de bord par défaut d'OpenGL est (0, 0, 0, 0).
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_WRAP_S, GL_CLAMP_TO_BORDER);
        (g.TexParameteri)(gl::GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_BORDER);
    }

    // Constantes (b0).
    let mut ubo = 0u32;
    if !constants.is_empty() {
        let mut data = constants.to_vec();
        data.resize(constants.len().div_ceil(16) * 16, 0);
        (g.GenBuffers)(1, &mut ubo);
        (g.BindBuffer)(gl::GL_UNIFORM_BUFFER, ubo);
        (g.BufferData)(
            gl::GL_UNIFORM_BUFFER,
            data.len() as isize,
            data.as_ptr().cast(),
            GL_STREAM_DRAW,
        );
        (g.BindBufferBase)(gl::GL_UNIFORM_BUFFER, base, ubo);
    }

    // Sortie.
    let mut out_tex = 0u32;
    (g.GenTextures)(1, &mut out_tex);
    (g.ActiveTexture)(GL_TEXTURE0 + base + inputs.len() as u32);
    let saved_out_unit = get(GL_TEXTURE_BINDING_2D)[0];
    (g.BindTexture)(gl::GL_TEXTURE_2D, out_tex);
    (g.TexImage2D)(
        gl::GL_TEXTURE_2D,
        0,
        gl::GL_RGBA8 as i32,
        w as i32,
        h as i32,
        0,
        gl::GL_BGRA,
        gl::GL_UNSIGNED_BYTE,
        std::ptr::null(),
    );
    let mut fbo = 0u32;
    (g.GenFramebuffers)(1, &mut fbo);
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
    (g.FramebufferTexture2D)(
        gl::GL_FRAMEBUFFER,
        gl::GL_COLOR_ATTACHMENT0,
        gl::GL_TEXTURE_2D,
        out_tex,
        0,
    );
    let complete = (g.CheckFramebufferStatus)(gl::GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE;
    let mut vao = 0u32;
    (g.GenVertexArrays)(1, &mut vao);
    let mut result = None;
    if complete {
        (g.Viewport)(0, 0, w as i32, h as i32);
        (g.ColorMask)(1, 1, 1, 1);
        (g.ClearColor)(0.0, 0.0, 0.0, 0.0);
        (g.Clear)(0x4000); // GL_COLOR_BUFFER_BIT
        (g.UseProgram)(prog);
        let loc = (g.GetUniformLocation)(prog, c"d2d_out".as_ptr());
        if loc >= 0 {
            (g.Uniform4f)(loc, x as f32, y as f32, w as f32, h as f32);
        }
        for (i, inp) in inputs.iter().enumerate() {
            let name = std::ffi::CString::new(format!("d2d_in[{i}]")).unwrap_or_default();
            let loc = (g.GetUniformLocation)(prog, name.as_ptr());
            if loc >= 0 {
                (g.Uniform4f)(
                    loc,
                    inp.x as f32,
                    inp.y as f32,
                    inp.w.max(1) as f32,
                    inp.h.max(1) as f32,
                );
            }
        }
        (g.BindVertexArray)(vao);
        (g.DrawArrays)(gl::GL_TRIANGLE_STRIP, 0, 4);
        let mut px = vec![0u8; w as usize * h as usize * 4];
        (g.BindFramebuffer)(GL_READ_FRAMEBUFFER, fbo);
        (g.ReadPixels)(
            0,
            0,
            w as i32,
            h as i32,
            gl::GL_BGRA,
            gl::GL_UNSIGNED_BYTE,
            px.as_mut_ptr().cast(),
        );
        result = Some(px);
    }

    // Nettoyage et retour à l'état d'avant.
    (g.DeleteVertexArrays)(1, &vao);
    (g.DeleteFramebuffers)(1, &fbo);
    (g.DeleteTextures)(1, &out_tex);
    if !textures.is_empty() {
        (g.DeleteTextures)(textures.len() as i32, textures.as_ptr());
    }
    if ubo != 0 {
        (g.DeleteBuffers)(1, &ubo);
    }
    (g.BindBuffer)(gl::GL_UNIFORM_BUFFER, saved_ubo as u32);
    (g.BindTexture)(gl::GL_TEXTURE_2D, saved_out_unit as u32);
    for (i, &t) in saved_tex.iter().enumerate() {
        (g.ActiveTexture)(GL_TEXTURE0 + base + i as u32);
        (g.BindTexture)(gl::GL_TEXTURE_2D, t as u32);
    }
    (g.ActiveTexture)(saved_active as u32);
    for (&c, &on) in caps.iter().zip(&saved_caps) {
        if on != 0 {
            (g.Enable)(c);
        }
    }
    (g.BindBuffer)(GL_PIXEL_PACK_BUFFER, saved_pack as u32);
    (g.BindBuffer)(GL_PIXEL_UNPACK_BUFFER, saved_unpack as u32);
    (g.PixelStorei)(0x0CF5, 4);
    (g.PixelStorei)(0x0D05, 4);
    (g.BindVertexArray)(saved_vao as u32);
    (g.UseProgram)(saved_prog as u32);
    (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, saved_draw as u32);
    (g.BindFramebuffer)(GL_READ_FRAMEBUFFER, saved_read as u32);
    (g.Viewport)(saved_vp[0], saved_vp[1], saved_vp[2], saved_vp[3]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shader d'effet écrit comme ceux de Direct2D, compilé par le
    /// compilateur HLSL de Winlux, exécuté sur OpenGL. Demande un serveur X
    /// (`xvfb-run cargo test -- --ignored d2d_effect_shader`).
    #[test]
    #[ignore]
    fn d2d_effect_shader_runs_on_opengl() {
        let src = "Texture2D InputTexture0 : register(t0);\n\
            SamplerState InputSampler0 : register(s0);\n\
            cbuffer constants : register(b0) { float4 tint; };\n\
            float4 main(float4 pos : SV_POSITION, float4 scene : SCENE_POSITION, float4 uv0 : TEXCOORD0) : SV_Target {\n\
              float4 c = InputTexture0.Sample(InputSampler0, uv0.xy);\n\
              return float4(c.rgb * tint.rgb, c.a) + float4(0, 0, scene.x >= 2.0 ? 0.0 : 0.0, 0);\n\
            }\n";
        let mut inc = |_: &str, _: bool| None;
        let c = crate::dlls::hlsl::compile(src, &[], &mut inc, "main", "ps_4_0", 0).expect("HLSL");
        let blob = crate::dlls::hlsl::container(&c);
        // 2 × 1 pixels BGRA : rouge opaque, vert opaque.
        let px = [0u8, 0, 255, 255, 0, 255, 0, 255];
        let input = Input {
            x: 0,
            y: 0,
            w: 2,
            h: 1,
            px: &px,
            linear: false,
        };
        let mut cb = Vec::new();
        for f in [0.5f32, 1.0, 1.0, 1.0] {
            cb.extend_from_slice(&f.to_le_bytes());
        }
        let out = unsafe { run(&blob, &[input], &cb, 0, 0, 2, 1) }.expect("rendu");
        // Rouge multiplié par 0,5 ; vert inchangé (BGRA).
        assert!((out[2] as i32 - 128).abs() <= 1, "{out:?}");
        assert_eq!(out[3], 255);
        assert_eq!(out[5], 255, "{out:?}");
    }

    #[test]
    fn vertex_shader_feeds_d2d_inputs() {
        let ps = dxbc::Shader {
            stage: dxbc::Stage::Pixel,
            glsl: String::new(),
            inputs: vec![
                dxbc::SigElement {
                    name: "SV_POSITION".into(),
                    index: 0,
                    system_value: 1,
                    component_type: 3,
                    register: 0,
                    mask: 15,
                },
                dxbc::SigElement {
                    name: "SCENE_POSITION".into(),
                    index: 0,
                    system_value: 0,
                    component_type: 3,
                    register: 1,
                    mask: 15,
                },
                dxbc::SigElement {
                    name: "TEXCOORD".into(),
                    index: 0,
                    system_value: 0,
                    component_type: 3,
                    register: 2,
                    mask: 15,
                },
            ],
            outputs: Vec::new(),
            sampler_pairs: Vec::new(),
        };
        let vs = vertex_glsl(&ps, 1);
        let scene = dxbc::varying_location("SCENE_POSITION", 0);
        let uv = dxbc::varying_location("TEXCOORD", 0);
        assert!(
            vs.contains(&format!("vary{scene} = vec4(scene, 0.0, 1.0);")),
            "{vs}"
        );
        assert!(
            vs.contains(&format!("vary{uv} = vec4((scene - d2d_in[0].xy)")),
            "{vs}"
        );
        assert!(!vs.contains("SV_POSITION"));
    }
}
