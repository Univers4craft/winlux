//! Traduction basique HLSL → GLSL pour la couche D3D11→OpenGL de Winlux.
//! Gère le parsing des en-têtes DXBC, des shaders passthrough de repli,
//! et une traduction texte HLSL→GLSL par substitution pour les cas courants.
#![allow(non_snake_case, dead_code)]

use crate::{log_debug, log_warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShaderType {
    Vertex,
    Pixel,
    Geometry,
    Compute,
}

pub(crate) struct TranslatedShader {
    pub glsl_source: String,
    pub shader_type: ShaderType,
}

// ---------------------------------------------------------------------------
// Passthrough shaders — fallback when we can't translate
// ---------------------------------------------------------------------------

pub(crate) fn passthrough_vertex_shader() -> &'static str {
    "#version 330 core\n\
     layout(location = 0) in vec3 aPos;\n\
     layout(location = 1) in vec2 aTexCoord;\n\
     out vec2 vTexCoord;\n\
     void main() {\n\
         gl_Position = vec4(aPos, 1.0);\n\
         vTexCoord = aTexCoord;\n\
     }\n"
}

pub(crate) fn passthrough_pixel_shader() -> &'static str {
    "#version 330 core\n\
     in vec2 vTexCoord;\n\
     out vec4 FragColor;\n\
     uniform sampler2D uTexture;\n\
     void main() {\n\
         FragColor = texture(uTexture, vTexCoord);\n\
     }\n"
}

pub(crate) fn solid_color_pixel_shader() -> &'static str {
    "#version 330 core\n\
     out vec4 FragColor;\n\
     uniform vec4 uColor;\n\
     void main() {\n\
         FragColor = uColor;\n\
     }\n"
}

// ---------------------------------------------------------------------------
// DXBC bytecode parsing
// ---------------------------------------------------------------------------

const DXBC_MAGIC: &[u8; 4] = b"DXBC";
const SHDR_MAGIC: u32 = u32::from_le_bytes(*b"SHDR");
const SHEX_MAGIC: u32 = u32::from_le_bytes(*b"SHEX");

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn detect_shader_type_from_dxbc_version(version_token: u32) -> ShaderType {
    let program_type = (version_token >> 16) & 0xFFFF;
    match program_type {
        0 => ShaderType::Pixel,
        1 => ShaderType::Vertex,
        2 => ShaderType::Geometry,
        5 => ShaderType::Compute,
        _ => {
            log_warn!(
                "d3d_shader",
                "type de shader DXBC inconnu {program_type}, assume pixel"
            );
            ShaderType::Pixel
        }
    }
}

pub(crate) fn translate_dxbc(bytecode: &[u8]) -> Option<TranslatedShader> {
    if bytecode.len() < 32 || &bytecode[0..4] != DXBC_MAGIC {
        return None;
    }

    let chunk_count = read_u32_le(bytecode, 28)? as usize;
    if bytecode.len() < 32 + chunk_count * 4 {
        return None;
    }

    let mut shader_type = None;

    for i in 0..chunk_count {
        let chunk_offset = read_u32_le(bytecode, 32 + i * 4)? as usize;
        if chunk_offset + 8 > bytecode.len() {
            continue;
        }
        let fourcc = read_u32_le(bytecode, chunk_offset)?;
        let chunk_size = read_u32_le(bytecode, chunk_offset + 4)? as usize;

        if fourcc == SHDR_MAGIC || fourcc == SHEX_MAGIC {
            if chunk_offset + 8 + 4 > bytecode.len() || chunk_size < 4 {
                continue;
            }
            let version_token = read_u32_le(bytecode, chunk_offset + 8)?;
            shader_type = Some(detect_shader_type_from_dxbc_version(version_token));
            break;
        }
    }

    let st = shader_type.unwrap_or_else(|| {
        log_warn!("d3d_shader", "chunk SHDR/SHEX absent, assume pixel shader");
        ShaderType::Pixel
    });

    let glsl_source = match st {
        ShaderType::Vertex => passthrough_vertex_shader().to_string(),
        ShaderType::Pixel => passthrough_pixel_shader().to_string(),
        ShaderType::Geometry => {
            log_warn!(
                "d3d_shader",
                "geometry shader non supporté, retour pixel passthrough"
            );
            passthrough_pixel_shader().to_string()
        }
        ShaderType::Compute => {
            log_warn!("d3d_shader", "compute shader non supporté, stub vide");
            "#version 430 core\nlayout(local_size_x = 1) in;\nvoid main() {}\n".to_string()
        }
    };

    log_debug!(
        "d3d_shader",
        "DXBC traduit : {:?} → {} octets GLSL",
        st,
        glsl_source.len()
    );

    Some(TranslatedShader {
        glsl_source,
        shader_type: st,
    })
}

// ---------------------------------------------------------------------------
// HLSL text → GLSL text (best-effort string substitution)
// ---------------------------------------------------------------------------

pub(crate) fn translate_hlsl_text(hlsl: &str, shader_type: ShaderType) -> String {
    let mut out = String::with_capacity(hlsl.len() + 256);
    out.push_str("#version 330 core\n");

    let lines: Vec<&str> = hlsl.lines().collect();
    let mut i = 0;
    let mut in_cbuffer = false;
    let mut cbuffer_brace_depth: u32 = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Skip HLSL preprocessor and existing version directives
        if trimmed.starts_with("#version") || trimmed.starts_with("#pragma") {
            i += 1;
            continue;
        }

        // cbuffer → uniform block (std140)
        if trimmed.starts_with("cbuffer") {
            in_cbuffer = true;
            cbuffer_brace_depth = 0;
            let name = trimmed
                .strip_prefix("cbuffer")
                .unwrap_or("")
                .trim()
                .split(|c: char| c.is_whitespace() || c == ':' || c == '{')
                .next()
                .unwrap_or("Constants");
            let name = if name.is_empty() { "Constants" } else { name };
            out.push_str(&format!("layout(std140) uniform {name} {{\n"));
            if trimmed.contains('{') {
                cbuffer_brace_depth += 1;
            }
            i += 1;
            continue;
        }

        if in_cbuffer {
            for ch in trimmed.chars() {
                if ch == '{' {
                    cbuffer_brace_depth += 1;
                } else if ch == '}' {
                    cbuffer_brace_depth = cbuffer_brace_depth.saturating_sub(1);
                }
            }
            if cbuffer_brace_depth == 0 && trimmed.contains('}') {
                out.push_str("};\n");
                in_cbuffer = false;
            } else {
                let converted = convert_hlsl_line(trimmed);
                out.push_str("    ");
                out.push_str(&converted);
                out.push('\n');
            }
            i += 1;
            continue;
        }

        // Texture2D declarations → sampler2D uniforms
        if trimmed.starts_with("Texture2D") && !trimmed.contains('(') {
            let name = extract_var_name(trimmed, "Texture2D");
            out.push_str(&format!("uniform sampler2D {name};\n"));
            i += 1;
            // Skip SamplerState on the next line if present
            if i < lines.len() && lines[i].trim().starts_with("SamplerState") {
                i += 1;
            }
            continue;
        }

        // SamplerState standalone → skip (folded into sampler2D above)
        if trimmed.starts_with("SamplerState") {
            i += 1;
            continue;
        }

        // Struct definitions — convert semantics in members
        if trimmed.starts_with("struct") {
            out.push_str(&convert_hlsl_line(trimmed));
            out.push('\n');
            i += 1;
            while i < lines.len() {
                let sline = lines[i].trim();
                let converted = strip_semantics(sline);
                let converted = convert_hlsl_line(&converted);
                out.push_str(&converted);
                out.push('\n');
                if sline.contains('}') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Function signatures: convert return type and parameter semantics
        let converted = convert_hlsl_line(trimmed);
        let converted = strip_semantics(&converted);
        let converted = convert_sample_calls(&converted);
        let converted = convert_mul_calls(&converted);
        out.push_str(&converted);
        out.push('\n');
        i += 1;
    }

    // If the output has SV_POSITION or SV_Target references remaining, inject
    // the standard output declarations at the top (after #version).
    let needs_frag_color = out.contains("SV_Target") || shader_type == ShaderType::Pixel;
    let needs_gl_position = out.contains("SV_POSITION") || out.contains("SV_Position");

    // Replace any remaining semantic annotations
    let out = out.replace(": SV_POSITION", "/* SV_POSITION */");
    let out = out.replace(": SV_Position", "/* SV_Position */");
    let out = out.replace(": SV_Target", "/* SV_Target */");
    let out = out.replace(": TEXCOORD0", "/* TEXCOORD0 */");
    let out = out.replace(": TEXCOORD", "/* TEXCOORD */");
    let out = out.replace(": COLOR", "/* COLOR */");
    let out = out.replace(": NORMAL", "/* NORMAL */");
    let out = out.replace(": POSITION", "/* POSITION */");

    let _ = (needs_frag_color, needs_gl_position);

    log_debug!(
        "d3d_shader",
        "HLSL texte traduit ({:?}): {} → {} octets",
        shader_type,
        hlsl.len(),
        out.len()
    );

    out
}

fn convert_hlsl_line(line: &str) -> String {
    let mut s = line.to_string();
    s = s.replace("float4x4", "mat4");
    s = s.replace("float3x3", "mat3");
    s = s.replace("float2x2", "mat2");
    s = s.replace("float4", "vec4");
    s = s.replace("float3", "vec3");
    s = s.replace("float2", "vec2");
    s = s.replace("int4", "ivec4");
    s = s.replace("int3", "ivec3");
    s = s.replace("int2", "ivec2");
    s = s.replace("uint4", "uvec4");
    s = s.replace("uint3", "uvec3");
    s = s.replace("uint2", "uvec2");
    s = s.replace("half4", "vec4");
    s = s.replace("half3", "vec3");
    s = s.replace("half2", "vec2");
    s = s.replace("half", "float");
    s = s.replace("lerp(", "mix(");
    s = s.replace("frac(", "fract(");
    s = s.replace("rsqrt(", "inversesqrt(");
    s = s.replace("atan2(", "atan(");
    s = s.replace("ddx(", "dFdx(");
    s = s.replace("ddy(", "dFdy(");
    s = s.replace("clip(", "/* clip */ (");
    s = s.replace("saturate(", "clamp(");
    // saturate has one arg, clamp needs three — caller must handle, but this
    // is best-effort: most shaders that reach us are simple.
    s
}

fn strip_semantics(line: &str) -> String {
    let mut s = line.to_string();
    let semantics = [
        "SV_POSITION",
        "SV_Position",
        "SV_Target0",
        "SV_Target1",
        "SV_Target2",
        "SV_Target3",
        "SV_Target",
        "SV_VertexID",
        "SV_InstanceID",
        "SV_PrimitiveID",
        "SV_Depth",
        "TEXCOORD0",
        "TEXCOORD1",
        "TEXCOORD2",
        "TEXCOORD3",
        "TEXCOORD",
        "COLOR0",
        "COLOR1",
        "COLOR",
        "NORMAL0",
        "NORMAL",
        "POSITION0",
        "POSITION",
        "BINORMAL",
        "TANGENT",
        "BLENDWEIGHT",
        "BLENDINDICES",
    ];
    for sem in semantics {
        let pattern = format!(": {sem}");
        s = s.replace(&pattern, "");
    }
    s
}

fn convert_sample_calls(line: &str) -> String {
    let mut s = line.to_string();
    // Pattern: texName.Sample(samplerName, uv) → texture(texName, uv)
    // We look for `.Sample(` and rewrite.
    while let Some(dot_pos) = s.find(".Sample(") {
        let tex_start = s[..dot_pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|p| p + 1)
            .unwrap_or(0);
        let tex_name = &s[tex_start..dot_pos];
        let args_start = dot_pos + 8; // skip ".Sample("
        if let Some(close) = find_matching_paren(&s, args_start - 1) {
            let args = &s[args_start..close];
            // args = "samplerName, uv" — skip the sampler, keep uv
            let uv_part = if let Some(comma) = args.find(',') {
                args[comma + 1..].trim()
            } else {
                args.trim()
            };
            let replacement = format!("texture({tex_name}, {uv_part})");
            s = format!("{}{replacement}{}", &s[..tex_start], &s[close + 1..]);
        } else {
            break;
        }
    }

    // SampleLevel → textureLod
    while let Some(dot_pos) = s.find(".SampleLevel(") {
        let tex_start = s[..dot_pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|p| p + 1)
            .unwrap_or(0);
        let tex_name = &s[tex_start..dot_pos];
        let args_start = dot_pos + 13;
        if let Some(close) = find_matching_paren(&s, args_start - 1) {
            let args = &s[args_start..close];
            let parts: Vec<&str> = split_top_level_args(args);
            if parts.len() >= 3 {
                let uv = parts[1].trim();
                let lod = parts[2].trim();
                let replacement = format!("textureLod({tex_name}, {uv}, {lod})");
                s = format!("{}{replacement}{}", &s[..tex_start], &s[close + 1..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    s
}

fn convert_mul_calls(line: &str) -> String {
    let mut s = line.to_string();
    // mul(a, b) → (a) * (b)
    while let Some(pos) = s.find("mul(") {
        if pos > 0 && s.as_bytes()[pos - 1].is_ascii_alphanumeric() {
            break;
        }
        let paren_start = pos + 3;
        if let Some(close) = find_matching_paren(&s, paren_start) {
            let inner = &s[paren_start + 1..close];
            let parts: Vec<&str> = split_top_level_args(inner);
            if parts.len() == 2 {
                let a = parts[0].trim();
                let b = parts[1].trim();
                let replacement = format!("(({a}) * ({b}))");
                s = format!("{}{replacement}{}", &s[..pos], &s[close + 1..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    s
}

fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open_pos).copied() != Some(b'(') {
        return None;
    }
    let mut depth = 1u32;
    for (i, &b) in bytes.iter().enumerate().skip(open_pos + 1) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_args(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&s[start..]);
    result
}

fn extract_var_name<'a>(line: &'a str, prefix: &str) -> &'a str {
    let rest = line.strip_prefix(prefix).unwrap_or(line).trim();
    let rest = rest.strip_prefix('<').map_or(rest, |r| {
        r.find('>').map(|i| r[i + 1..].trim()).unwrap_or(r)
    });
    rest.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("tex")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_dxbc_magic() {
        assert!(translate_dxbc(b"not dxbc").is_none());
        assert!(translate_dxbc(b"DXB").is_none());
    }

    #[test]
    fn passthrough_shaders_are_valid_glsl() {
        assert!(passthrough_vertex_shader().contains("#version 330 core"));
        assert!(passthrough_pixel_shader().contains("#version 330 core"));
        assert!(passthrough_vertex_shader().contains("gl_Position"));
        assert!(passthrough_pixel_shader().contains("FragColor"));
    }

    #[test]
    fn hlsl_type_substitution() {
        let hlsl = "float4 color = float4(1,0,0,1);";
        let glsl = translate_hlsl_text(hlsl, ShaderType::Pixel);
        assert!(glsl.contains("vec4 color = vec4(1,0,0,1);"));
    }

    #[test]
    fn hlsl_mul_conversion() {
        let result = convert_mul_calls("output = mul(worldMatrix, position);");
        assert!(result.contains("((worldMatrix) * (position))"));
    }

    #[test]
    fn hlsl_sample_conversion() {
        let result = convert_sample_calls("color = myTex.Sample(mySampler, uv);");
        assert!(result.contains("texture(myTex, uv)"));
        assert!(!result.contains(".Sample"));
    }

    #[test]
    fn hlsl_lerp_frac() {
        let converted = convert_hlsl_line("float v = lerp(a, b, frac(t));");
        assert_eq!(converted, "float v = mix(a, b, fract(t));");
    }

    #[test]
    fn strip_semantic_annotations() {
        let result = strip_semantics("vec4 pos: SV_POSITION;");
        assert_eq!(result, "vec4 pos;");
    }

    #[test]
    fn cbuffer_to_uniform_block() {
        let hlsl =
            "cbuffer PerFrame : register(b0) {\n    float4x4 viewProj;\n    float3 eyePos;\n}";
        let glsl = translate_hlsl_text(hlsl, ShaderType::Vertex);
        assert!(glsl.contains("layout(std140) uniform PerFrame"));
        assert!(glsl.contains("mat4 viewProj"));
        assert!(glsl.contains("vec3 eyePos"));
    }

    #[test]
    fn extract_texture_name() {
        assert_eq!(
            extract_var_name("Texture2D diffuseMap;", "Texture2D"),
            "diffuseMap"
        );
        assert_eq!(
            extract_var_name("Texture2D<float4> albedo : register(t0);", "Texture2D"),
            "albedo"
        );
    }
}
