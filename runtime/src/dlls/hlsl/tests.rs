use super::*;

fn build(src: &str, entry: &str, target: &str) -> Compiled {
    let mut inc = |_: &str, _: bool| None;
    match compile(src, &[], &mut inc, entry, target, 0) {
        Ok(c) => {
            validate(&c);
            c
        }
        Err(e) => panic!("{e}"),
    }
}

/// Fait valider le GLSL par glslangValidator quand il est installé.
fn validate(c: &Compiled) {
    let Some(tool) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("glslangValidator"))
            .find(|f| f.is_file())
    }) else {
        return;
    };
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "winlux-hlsl-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(match c.stage {
        Stage::Vertex => "s.vert",
        Stage::Geometry => "s.geom",
        Stage::Compute => "s.comp",
        _ => "s.frag",
    });
    std::fs::write(&file, &c.glsl).unwrap();
    let out = std::process::Command::new(&tool)
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}\n{}",
        numbered(&c.glsl),
        String::from_utf8_lossy(&out.stdout)
    );
}

fn numbered(s: &str) -> String {
    s.lines()
        .enumerate()
        .map(|(i, l)| format!("{:4} {l}\n", i + 1))
        .collect()
}

const QUAD: &str = r#"
cbuffer Scene : register(b0) {
    float4x4 world_view_proj;
    float4 tint;
    float2 offsets[3];
};
struct VS_IN { float3 pos : POSITION; float2 uv : TEXCOORD0; uint id : SV_VertexID; };
struct VS_OUT { float4 pos : SV_Position; float2 uv : TEXCOORD0; nointerpolation uint layer : BLENDINDICES0; };
VS_OUT vs_main(VS_IN i) {
    VS_OUT o = (VS_OUT)0;
    o.pos = mul(float4(i.pos + float3(offsets[i.id % 3], 0), 1.0), world_view_proj);
    o.uv = i.uv * tint.xy;
    o.layer = i.id;
    return o;
}
Texture2D<float4> tex : register(t1);
SamplerState smp : register(s2);
float4 ps_main(VS_OUT i, bool front : SV_IsFrontFace) : SV_Target {
    float4 c = tex.Sample(smp, i.uv) * tint;
    clip(c.a - 0.01);
    if (!front) c.rgb = 1 - c.rgb;
    return saturate(c);
}
"#;

#[test]
fn quad_textured() {
    let vs = build(QUAD, "vs_main", "vs_5_0");
    let g = &vs.glsl;
    assert!(g.contains("layout(location = 0) in vec4 a0;"), "{g}");
    assert!(g.contains("gl_Position = vec4(pos.x, -pos.y"), "{g}");
    assert!(g.contains("transpose(mat4(cb0[0].xyzw"), "{g}");
    // offsets[3] : float2 dans des registres entiers, après la matrice et tint.
    assert!(g.contains("cb0[5 + int("), "{g}");
    assert_eq!(vs.inputs.len(), 3);
    assert_eq!(vs.inputs[1].name, "TEXCOORD");
    assert_eq!(vs.inputs[2].system_value, 6);
    let ps = build(QUAD, "ps_main", "ps_5_0");
    let g = &ps.glsl;
    assert!(
        g.contains("layout(binding = 17) uniform sampler2D t_tex;"),
        "{g}"
    );
    assert!(g.contains("discard"), "{g}");
    assert!(g.contains("layout(location = 0) out vec4 target0;"), "{g}");
    assert_eq!(ps.pairs, vec![(1, 2)]);
    // Même location de varying des deux côtés.
    let loc = crate::dlls::dxbc::varying_location("TEXCOORD", 0);
    assert!(vs.glsl.contains(&format!("out vec4 vary{loc};")));
    assert!(ps.glsl.contains(&format!("in vec4 vary{loc};")));
}

/// Forme des shaders produits par ANGLE (moteur D3D11 de libGLESv2).
const ANGLE_VS: &str = r#"
#pragma warning( disable: 3556 3571 )
float4 vec4_ctor(float2 x0, float x1, float x2)
{
    return float4(x0, x1, x2);
}
// Uniforms

uniform float4x4 _u_mvp : register(c0);
#ifdef ANGLE_ENABLE_LOOP_FLATTEN
#define LOOP [loop]
#define FLATTEN [flatten]
#else
#define LOOP
#define FLATTEN
#endif

#define ATOMIC_COUNTER_ARRAY_STRIDE 4

// Attributes
static float2 _a_position = {0, 0};
static float2 _a_texcoord = {0, 0};

static float4 gl_Position = float4(0, 0, 0, 0);

// Varyings
static  float2 _v_texcoord = {0, 0};

cbuffer DriverConstants : register(b1)
{
    float4 dx_ViewAdjust : packoffset(c1);
    float2 dx_ViewCoords : packoffset(c2);
    float2 dx_ViewScale  : packoffset(c3);
    float clipControlOrigin : packoffset(c3.z);
    float clipControlZeroToOne : packoffset(c3.w);
};

@@ VERTEX ATTRIBUTES @@

;
;
;
;
;
;
;

void gl_main()
{
(gl_Position = mul(transpose(_u_mvp), vec4_ctor(_a_position, 0.0, 1.0)));
(_v_texcoord = _a_texcoord);
}
struct VS_OUTPUT
{
    float4 dx_Position : SV_Position;
    float4 gl_Position : TEXCOORD1;
    float2 v0 : TEXCOORD0;
};
VS_OUTPUT generateOutput(VS_INPUT input)
{
    VS_OUTPUT output;
    output.gl_Position = gl_Position;
    output.dx_Position.x = gl_Position.x;
    output.dx_Position.y = clipControlOrigin * gl_Position.y;
    if (clipControlZeroToOne)
    {
        output.dx_Position.z = gl_Position.z;
    } else {
        output.dx_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    }
    output.dx_Position.w = gl_Position.w;
    output.v0 = _v_texcoord;
    return output;
}

VS_OUTPUT main(VS_INPUT input){
    initAttributes(input);

    gl_main();

    return generateOutput(input);
}
"#;

const ANGLE_VS_INPUT: &str = r#"
struct VS_INPUT
{
    float2 _a_position : TEXCOORD0;
    float2 _a_texcoord : TEXCOORD1;
};

void initAttributes(VS_INPUT input)
{
    _a_position = input._a_position;
    _a_texcoord = input._a_texcoord;
}
"#;

const ANGLE_PS: &str = r#"
#pragma warning( disable: 3556 3571 )
// Uniforms

static const uint _u_tex = 0;
uniform Texture2D<float4> textures2D[1] : register(t0);
uniform SamplerState samplers2D[1] : register(s0);
#ifdef ANGLE_ENABLE_LOOP_FLATTEN
#define LOOP [loop]
#define FLATTEN [flatten]
#else
#define LOOP
#define FLATTEN
#endif

#define ATOMIC_COUNTER_ARRAY_STRIDE 4

// Varyings
static  float2 _v_texcoord = {0, 0};

static float4 out_gl_FragColor = {0, 0, 0, 0};

cbuffer DriverConstants : register(b1)
{
    float4 dx_ViewCoords : packoffset(c1);
    float2 dx_FragCoordOffset : packoffset(c3);
    float3 dx_DepthFront : packoffset(c2);
    float2 dx_ViewScale : packoffset(c3.z);
    uint dx_Misc : packoffset(c2.w);
    struct SamplerMetadata
    {
        int baseLevel;
        int wrapModes;
        int2 padding;
        int4 intBorderColor;
    };
    SamplerMetadata samplerMetadata[1] : packoffset(c4);
};

float4 gl_texture2D(uint samplerIndex, float2 t)
{
    return textures2D[samplerIndex].Sample(samplers2D[samplerIndex], float2(t.x, t.y));
}

#define GL_USES_FRAG_COLOR
@@ PIXEL OUTPUT @@

PS_OUTPUT main(PS_INPUT input){
    _v_texcoord = input.v0.xy;

    gl_main();

    return generateOutput();
}
void gl_main()
{
(out_gl_FragColor = gl_texture2D(_u_tex, _v_texcoord));
}
"#;

#[test]
fn angle_shaders() {
    let vs = ANGLE_VS
        .replace("@@ VERTEX ATTRIBUTES @@", ANGLE_VS_INPUT)
        .replace(
            "cbuffer DriverConstants",
            "uniform float4 dx_unused;\ncbuffer DriverConstants",
        );
    let c = build(&vs, "main", "vs_4_0");
    assert!(
        c.glsl.contains("layout(std140, binding = 1) uniform"),
        "{}",
        c.glsl
    );
    // $Globals en b0, _u_mvp en c0.
    assert!(
        c.glsl.contains("layout(std140, binding = 0) uniform"),
        "{}",
        c.glsl
    );
    assert_eq!(c.inputs[0].name, "TEXCOORD");
    assert_eq!(c.outputs[0].system_value, 1);

    let out = r#"
struct PS_INPUT { float4 dx_Position : SV_Position; float4 gl_Position : TEXCOORD1; float2 v0 : TEXCOORD0; };
struct PS_OUTPUT { float4 gl_Color0 : SV_Target0; };
void gl_main();
PS_OUTPUT generateOutput() { PS_OUTPUT output; output.gl_Color0 = out_gl_FragColor; return output; }
"#;
    let ps = ANGLE_PS.replace("@@ PIXEL OUTPUT @@", out);
    let c = build(&ps, "main", "ps_4_0");
    assert_eq!(c.pairs, vec![(0, 0)]);
    assert!(c.glsl.contains("t_textures2D[1]"), "{}", c.glsl);
}

#[test]
fn language_features() {
    let src = r#"
#define N 4
struct Light { float3 dir; float intensity; float4 color; };
cbuffer Lights { Light lights[N]; row_major float3x4 bones[2]; int count; bool enabled; };
static const float weights[3] = { 0.25, 0.5, 0.25 };
float shade(float3 n, Light l, inout float acc) {
    float d = max(dot(n, -l.dir), 0.0f) * l.intensity;
    acc += d;
    return d;
}
float4 main(float4 p : SV_Position, float3 n : NORMAL, int2 k : TEXCOORD3) : SV_Target0 {
    float acc = 0;
    float4 c = 0;
    [loop] for (int i = 0; i < count && i < N; ++i) {
        c += lights[i].color * shade(normalize(n), lights[i], acc);
    }
    float3 b = mul(bones[k.x & 1], float4(n, 1));
    switch (k.y) { case 0: c.r = 1; break; case 1: c.g = fmod(c.g, 2.0); break; default: break; }
    float2x2 m = { 1, 2, 3, 4 };
    float2 v = mul(float2(1, 0), m);
    uint u = asuint(c.x) >> 3u;
    c.xy += v * weights[1] + (float)u * 1e-9f + b.xy;
    c = enabled ? c : float4(0.5.xxx, 1);
    c.a = any(c.rgb > 0.5) ? lerp(c.a, 1, 0.5) : sign(c.a) * acc;
    return c;
}
"#;
    let c = build(src, "main", "ps_5_0");
    let g = &c.glsl;
    // Matrice row_major : lignes lues directement.
    assert!(g.contains("mat3x4(cb0["), "{g}");
    assert!(g.contains("w_mod("), "{g}");
    assert!(g.contains("switch ("), "{g}");
    assert!(g.contains("floatBitsToInt(cb0["), "{g}");
}

#[test]
fn errors_have_lines() {
    let mut inc = |_: &str, _: bool| None;
    let e = compile(
        "float4 main() : SV_Target {\n  return undefined_thing;\n}",
        &[],
        &mut inc,
        "main",
        "ps_5_0",
        0,
    )
    .err()
    .unwrap();
    assert!(e.starts_with("(2)"), "{e}");
}

#[test]
fn container_roundtrip() {
    let c = build(QUAD, "ps_main", "ps_5_0");
    let blob = container(&c);
    let s = crate::dlls::dxbc::translate(&blob).unwrap();
    assert_eq!(s.stage, Stage::Pixel);
    assert_eq!(s.glsl, c.glsl);
    assert_eq!(s.sampler_pairs, c.pairs);
    assert_eq!(s.inputs, c.inputs);
}

#[test]
#[ignore]
fn dump() {
    println!("{}", build(QUAD, "vs_main", "vs_5_0").glsl);
    println!("{}", build(QUAD, "ps_main", "ps_5_0").glsl);
}

#[test]
fn game_style_shaders() {
    let src = r#"
Texture2D gAlbedo : register(t0);
Texture2DArray<float> gShadow : register(t1);
TextureCube gSky : register(t2);
Buffer<float4> gInstances : register(t3);
SamplerState gLinear : register(s0);
SamplerComparisonState gCmp : register(s1);
cbuffer PerFrame : register(b0) {
    float4x4 gViewProj;
    float4x4 gLightVP[4];
    float3 gEye; float gTime;
    float4 gFog;
    uint gFlags;
};
struct V2P { float4 pos : SV_POSITION; float3 n : NORMAL; float2 uv : TEXCOORD0; float3 wp : TEXCOORD1; };
float shadow(float3 wp, int cascade) {
    float4 lp = mul(float4(wp, 1), gLightVP[cascade]);
    lp.xyz /= lp.w;
    float2 suv = lp.xy * float2(0.5, -0.5) + 0.5;
    uint w, h, el;
    gShadow.GetDimensions(w, h, el);
    float sum = 0;
    [unroll] for (int y = -1; y <= 1; y++)
        for (int x = -1; x <= 1; x++)
            sum += gShadow.SampleCmpLevelZero(gCmp, float3(suv + float2(x, y) / w, cascade), lp.z);
    return sum / 9.0;
}
V2P VS(float3 p : POSITION, float3 n : NORMAL, float2 uv : TEXCOORD0, uint inst : SV_InstanceID) {
    V2P o;
    float4 off = gInstances.Load(inst);
    float3 wp = p * off.w + off.xyz;
    o.pos = mul(float4(wp, 1.0), gViewProj);
    o.n = n; o.uv = uv; o.wp = wp;
    return o;
}
float4 PS(V2P i) : SV_TARGET {
    float3 n = normalize(i.n);
    float3 v = normalize(gEye - i.wp);
    float4 alb = gAlbedo.Sample(gLinear, i.uv);
    float3 refl = gSky.SampleLevel(gLinear, reflect(-v, n), 3).rgb;
    int c = (int)min(3, floor(length(gEye - i.wp) / 50));
    float sh = (gFlags & 1) ? shadow(i.wp, c) : 1.0;
    float ndl = saturate(dot(n, normalize(float3(0.3, 1, 0.2))));
    float3 col = alb.rgb * (0.2 + 0.8 * ndl * sh) + refl * 0.1;
    float fog = exp(-gFog.w * distance(gEye, i.wp));
    col = lerp(gFog.rgb, col, fog);
    float4 t = gAlbedo.Load(int3(int2(i.pos.xy) & 7, 0));
    uint2 dim; gAlbedo.GetDimensions(dim.x, dim.y);
    while (col.r > 1) { col.r -= 1; }
    do { col.g *= 0.5; } while (col.g > 1);
    float k[2] = { 0.5, t.a };
    return float4(col + k[i.pos.x > dim.x / 2] * 0.0, alb.a);
}
"#;
    let vs = build(src, "VS", "vs_5_0");
    if std::env::var_os("HLSL_DUMP_TEST").is_some() {
        println!("{}\n{}", vs.glsl, build(src, "PS", "ps_5_0").glsl);
    }
    assert_eq!(vs.inputs[3].system_value, 8);
    let ps = build(src, "PS", "ps_5_0");
    assert!(
        ps.pairs.contains(&(0, 0)) && ps.pairs.contains(&(1, 1)),
        "{:?}",
        ps.pairs
    );
}

/// Vertex et pixel shaders liés ensemble (glslangValidator -l) : leurs
/// tampons de même numéro, de tailles différentes, ne doivent pas se
/// confondre dans le programme (Warzone 2100 en ANGLE, passage 13).
#[test]
fn stages_link_together() {
    let src = r#"
float4x4 posMatrix;
float4 color;
struct V { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
V vs(float4 p : POSITION, float2 uv : TEXCOORD0) { V o; o.pos = mul(p, posMatrix); o.uv = uv; return o; }
Texture2D t; SamplerState s;
float4 ps(V i) : SV_Target { return t.Sample(s, i.uv) * color; }
"#;
    let vs = build(&src.replace("float4 color;", ""), "vs", "vs_5_0");
    let ps = build(&src.replace("float4x4 posMatrix;", ""), "ps", "ps_5_0");
    let Some(tool) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("glslangValidator"))
            .find(|f| f.is_file())
    }) else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("winlux-hlsl-lien-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("s.vert"), &vs.glsl).unwrap();
    std::fs::write(dir.join("s.frag"), &ps.glsl).unwrap();
    let out = std::process::Command::new(&tool)
        .arg("-l")
        .arg(dir.join("s.vert"))
        .arg(dir.join("s.frag"))
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}\n{}\n{}",
        vs.glsl,
        ps.glsl,
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Shaders d'ANGLE vus dans Warzone 2100 (#517) : matrice par instance en
/// entrée de VS (quatre sémantiques TEXCOORD consécutives, une colonne par
/// registre) et PS de profondeur seule dont la structure de sortie est vide.
#[test]
fn matrix_inputs_and_empty_output() {
    let vs_src = r#"
float4x4 viewProj;
struct VS_INPUT { float4 pos : TEXCOORD0; float4x4 model : TEXCOORD1; row_major float2x3 extra : TEXCOORD5; };
struct VS_OUTPUT { float4 pos : SV_Position; float4x4 m : TEXCOORD0; };
VS_OUTPUT main(VS_INPUT input) {
    VS_OUTPUT o;
    o.pos = mul(mul(input.pos, input.model), viewProj) + float4(input.extra[1], 0);
    o.m = input.model;
    return o;
}
"#;
    let vs = build(vs_src, "main", "vs_5_0");
    let names: Vec<(String, u32, u32)> = vs
        .inputs
        .iter()
        .map(|e| (e.name.clone(), e.index, e.register))
        .collect();
    assert_eq!(
        names,
        [
            ("TEXCOORD".to_string(), 0, 0),
            ("TEXCOORD".to_string(), 1, 1),
            ("TEXCOORD".to_string(), 2, 2),
            ("TEXCOORD".to_string(), 3, 3),
            ("TEXCOORD".to_string(), 4, 4),
            ("TEXCOORD".to_string(), 5, 5),
            ("TEXCOORD".to_string(), 6, 6),
        ]
    );
    // Registre 1 = colonne 0 de la matrice column_major.
    assert!(vs.glsl.contains("[1][0] = c.y;"), "{}", vs.glsl);

    let ps_src = r#"
struct PS_INPUT { float4 pos : SV_Position; };
struct PS_OUTPUT { };
PS_OUTPUT generateOutput() { PS_OUTPUT output; return output; }
PS_OUTPUT main(PS_INPUT input) { if (input.pos.x < 0) discard; return generateOutput(); }
"#;
    let ps = build(ps_src, "main", "ps_5_0");
    assert!(ps.outputs.is_empty());
}

/// Geometry shader d'ANGLE (#520) : sommet provoquant en dernier pour les
/// varyings nointerpolation, flux TriangleStream. Les trois étages doivent
/// se lier.
#[test]
fn angle_geometry_shader() {
    let io = r#"
struct VS_OUTPUT { float4 dx_Position : SV_Position; float4 gl_Position : TEXCOORD2; nointerpolation uint4 v0 : TEXCOORD0; nointerpolation int v1 : TEXCOORD1; float2 uv : TEXCOORD3; };
"#;
    let vs_src = format!(
        "{io}VS_OUTPUT main(float3 p : TEXCOORD0, uint4 t : TEXCOORD1) {{ VS_OUTPUT o = (VS_OUTPUT)0; o.dx_Position = float4(p, 1); o.gl_Position = o.dx_Position; o.v0 = t; o.v1 = (int)t.x; o.uv = p.xy; return o; }}"
    );
    let gs_src = r#"
struct GS_INPUT { float4 dx_Position : SV_Position; float4 gl_Position : TEXCOORD2; nointerpolation uint4 v0 : TEXCOORD0; nointerpolation int v1 : TEXCOORD1; float2 uv : TEXCOORD3; };
typedef GS_INPUT GS_OUTPUT;
void copyVertex(inout GS_OUTPUT output, GS_INPUT input, GS_INPUT flatinput)
{
    output.gl_Position = input.gl_Position;
    output.v0 = flatinput.v0;
    output.v1 = flatinput.v1;
    output.uv = input.uv;
    output.dx_Position = input.dx_Position;
}
[maxvertexcount(3)]
void main(triangle GS_INPUT input[3], inout TriangleStream<GS_OUTPUT> outStream)
{
    GS_OUTPUT output = (GS_OUTPUT)0;
    int lastVertexIndex = 2;
    for (int i = 0; i < 3; ++i)
    {
        copyVertex(output, input[i], input[lastVertexIndex]);
        outStream.Append(output);
    }
    outStream.RestartStrip();
}
"#;
    let ps_src = format!(
        "{io}float4 main(VS_OUTPUT i) : SV_Target {{ return float4(i.v0) / 255.0 + float4(i.uv, i.v1, 1); }}"
    );
    let vs = build(&vs_src, "main", "vs_5_0");
    let gs = build(gs_src, "main", "gs_5_0");
    let ps = build(&ps_src, "main", "ps_5_0");
    assert_eq!(gs.stage, Stage::Geometry);
    assert!(gs.glsl.contains("layout(triangles) in;"), "{}", gs.glsl);
    assert!(gs.glsl.contains("max_vertices = 3"), "{}", gs.glsl);
    assert!(gs.glsl.contains("EmitVertex()"), "{}", gs.glsl);
    assert!(gs.glsl.contains("EndPrimitive()"), "{}", gs.glsl);
    let Some(tool) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("glslangValidator"))
            .find(|f| f.is_file())
    }) else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("winlux-hlsl-gs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("s.vert"), &vs.glsl).unwrap();
    std::fs::write(dir.join("s.geom"), &gs.glsl).unwrap();
    std::fs::write(dir.join("s.frag"), &ps.glsl).unwrap();
    let out = std::process::Command::new(&tool)
        .arg("-l")
        .arg(dir.join("s.vert"))
        .arg(dir.join("s.geom"))
        .arg(dir.join("s.frag"))
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}\n{}\n{}\n{}",
        vs.glsl,
        gs.glsl,
        ps.glsl,
        String::from_utf8_lossy(&out.stdout)
    );
}

/// SampleCmp sur une texture lue seulement par comparaison : sampler
/// « shadow », la carte applique la ComparisonFunc (#523). Lue aussi
/// normalement : comparaison à la main.
#[test]
fn sample_cmp_uses_shadow_samplers() {
    let src = r#"
Texture2DArray<float> cascades : register(t0);
Texture2D<float> mixed : register(t1);
Texture2D<float> plain : register(t2);
SamplerComparisonState cmp : register(s0);
SamplerState lin : register(s1);
float4 main(float4 p : SV_Position, float3 uv : TEXCOORD0) : SV_Target {
    uint w, h, n;
    cascades.GetDimensions(w, h, n);
    float a = cascades.SampleCmp(cmp, uv, 0.5);
    float b = cascades.SampleCmpLevelZero(cmp, uv, 0.25) / w;
    float c = mixed.SampleCmp(cmp, uv.xy, 0.5) + mixed.Sample(lin, uv.xy);
    float d = plain.SampleCmpLevelZero(cmp, uv.xy, 0.5);
    return float4(a, b, c, d);
}
"#;
    let ps = build(src, "main", "ps_5_0");
    assert!(
        ps.glsl.contains("sampler2DArrayShadow t_cascades"),
        "{}",
        ps.glsl
    );
    assert!(ps.glsl.contains("sampler2D t_mixed"), "{}", ps.glsl);
    assert!(ps.glsl.contains("sampler2DShadow t_plain"), "{}", ps.glsl);
    assert!(ps.glsl.contains("textureGrad(t_cascades"), "{}", ps.glsl);
    assert!(!ps.glsl.contains("w_cmp"), "{}", ps.glsl);
}

/// Compute shader (#580) : images, tampons structurés et bruts, mémoire
/// partagée, barrières, atomiques et entrées SV_*.
#[test]
fn compute_shader() {
    let src = r#"
#define GROUP 8
struct Particle { float3 pos; float life; float4 color; };
cbuffer Params : register(b0) { float dt; uint count; };
Texture2D<float4> input : register(t0);
StructuredBuffer<float> weights : register(t1);
RWTexture2D<float4> output : register(u0);
RWStructuredBuffer<Particle> particles : register(u1);
RWByteAddressBuffer counters : register(u2);
RWTexture2D<uint> histogram : register(u3);
groupshared float4 tile[GROUP * GROUP];
groupshared uint hits;

[numthreads(GROUP, GROUP, 1)]
void main(uint3 id : SV_DispatchThreadID, uint3 gtid : SV_GroupThreadID,
          uint gi : SV_GroupIndex, uint3 gid : SV_GroupID)
{
    if (gi == 0) hits = 0;
    GroupMemoryBarrierWithGroupSync();
    tile[gi] = input.Load(int3(id.xy, 0)) * weights[gi % 4];
    GroupMemoryBarrierWithGroupSync();
    float4 sum = 0;
    [unroll] for (uint k = 0; k < 4; k++) sum += tile[(gi + k) % (GROUP * GROUP)];
    uint w, h;
    output.GetDimensions(w, h);
    if (id.x < w && id.y < h) output[id.xy] = sum * 0.25;
    InterlockedAdd(hits, 1);
    uint old;
    InterlockedAdd(histogram[uint2(gid.x, 0)], 1, old);
    uint n, stride;
    particles.GetDimensions(n, stride);
    if (id.x < n && id.y == 0) {
        Particle p = particles[id.x];
        p.pos += float3(0, -9.81, 0) * dt;
        p.life -= dt;
        particles[id.x] = p;
        particles[id.x].color.a = saturate(p.life);
    }
    counters.InterlockedAdd(0, 1);
    uint avant;
    counters.InterlockedAdd(4, 2, avant);
    counters.Store2(8, uint2(gtid.xy));
    uint first = counters.Load(4);
    if (first == 0) output[uint2(0, 0)] += float4(1, 0, 0, 0);
}
"#;
    let c = build(src, "main", "cs_5_0");
    assert_eq!(c.stage, Stage::Compute);
    let g = &c.glsl;
    for want in [
        "layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;",
        "uniform image2D t_output;",
        "layout(binding = 3, r32ui) uniform uimage2D t_histogram;",
        "buffer B_t_particles",
        "readonly buffer B_t_weights",
        "shared vec4[64] htile;",
        "gl_GlobalInvocationID",
        "barrier()",
        "atomicAdd(hhits, 1u)",
        "imageAtomicAdd(t_histogram",
        "imageStore(t_output",
    ] {
        assert!(g.contains(want), "« {want} » absent :\n{}", numbered(g));
    }
}

/// Échantillonneur déclaré avec son bloc d'états (syntaxe Direct3D 10),
/// comme dans le HLSL qu'OBS Studio tire de ses fichiers .effect (#2288).
#[test]
fn sampler_avec_bloc_d_etats() {
    let src = "static const bool deja_glsl = false;\n\n\
               uniform Texture2D image;\n\n\n\
               SamplerState echant {\n\tFilter = Linear;\n\n\tAddressU = Clamp;\n\n\tAddressV = Wrap;\n\n};\n\
               SamplerState paire[2] { MaxAnisotropy = 4; BorderColor = { 0, 0, 0, 1 }; };\n\
               struct Entree { float4 pos : SV_Position; float2 uv : TEXCOORD0; };\n\
               float4 main(Entree e) : SV_Target\n\
               {\n\treturn image.Sample(echant, e.uv) + image.Sample(paire[1], e.uv);\n}\n";
    let ps = build(src, "main", "ps_4_0");
    assert!(ps.glsl.contains("uniform sampler2D"), "{}", ps.glsl);
    assert!(!ps.pairs.is_empty());
    // Un bloc non fermé reste une erreur, sur la ligne de la déclaration.
    let mut inc = |_: &str, _: bool| None;
    let e = compile(
        "SamplerState s { Filter = Linear;\n",
        &[],
        &mut inc,
        "main",
        "ps_4_0",
        0,
    )
    .err()
    .expect("bloc non fermé accepté");
    assert!(e.contains("« } » attendu"), "{e}");
}
