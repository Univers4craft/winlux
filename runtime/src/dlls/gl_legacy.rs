//! Fonctions OpenGL « compatibilité » (pipeline fixe) utilisées par la
//! traduction Direct3D 9 : matrices, éclairage, brouillard, tableaux de
//! sommets côté client, et quelques entrées GL 2.0 absentes de
//! `gl_bindings`. Un symbole manquant donne `None` pour toute la table :
//! Direct3D 9 passe alors en mode sans rendu au lieu d'appeler un pointeur nul.
#![allow(non_snake_case, dead_code)]

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

pub(crate) const GL_MODELVIEW: u32 = 0x1700;
pub(crate) const GL_PROJECTION: u32 = 0x1701;
pub(crate) const GL_TEXTURE: u32 = 0x1702;
pub(crate) const GL_VERTEX_ARRAY: u32 = 0x8074;
pub(crate) const GL_NORMAL_ARRAY: u32 = 0x8075;
pub(crate) const GL_COLOR_ARRAY: u32 = 0x8076;
pub(crate) const GL_TEXTURE_COORD_ARRAY: u32 = 0x8078;
pub(crate) const GL_SECONDARY_COLOR_ARRAY: u32 = 0x845E;
pub(crate) const GL_LIGHTING: u32 = 0x0B50;
pub(crate) const GL_LIGHT0: u32 = 0x4000;
pub(crate) const GL_AMBIENT: u32 = 0x1200;
pub(crate) const GL_DIFFUSE: u32 = 0x1201;
pub(crate) const GL_SPECULAR: u32 = 0x1202;
pub(crate) const GL_POSITION: u32 = 0x1203;
pub(crate) const GL_SPOT_DIRECTION: u32 = 0x1204;
pub(crate) const GL_SPOT_EXPONENT: u32 = 0x1205;
pub(crate) const GL_SPOT_CUTOFF: u32 = 0x1206;
pub(crate) const GL_CONSTANT_ATTENUATION: u32 = 0x1207;
pub(crate) const GL_LINEAR_ATTENUATION: u32 = 0x1208;
pub(crate) const GL_QUADRATIC_ATTENUATION: u32 = 0x1209;
pub(crate) const GL_EMISSION: u32 = 0x1600;
pub(crate) const GL_SHININESS: u32 = 0x1601;
pub(crate) const GL_AMBIENT_AND_DIFFUSE: u32 = 0x1602;
pub(crate) const GL_FRONT_AND_BACK: u32 = 0x0408;
pub(crate) const GL_COLOR_MATERIAL: u32 = 0x0B57;
pub(crate) const GL_LIGHT_MODEL_AMBIENT: u32 = 0x0B53;
pub(crate) const GL_LIGHT_MODEL_COLOR_CONTROL: u32 = 0x81F8;
pub(crate) const GL_SEPARATE_SPECULAR_COLOR: u32 = 0x81FA;
pub(crate) const GL_SINGLE_COLOR: u32 = 0x81F9;
pub(crate) const GL_NORMALIZE: u32 = 0x0BA1;
pub(crate) const GL_FOG: u32 = 0x0B60;
pub(crate) const GL_FOG_MODE: u32 = 0x0B65;
pub(crate) const GL_FOG_DENSITY: u32 = 0x0B62;
pub(crate) const GL_FOG_START: u32 = 0x0B63;
pub(crate) const GL_FOG_END: u32 = 0x0B64;
pub(crate) const GL_FOG_COLOR: u32 = 0x0B66;
pub(crate) const GL_EXP: u32 = 0x0800;
pub(crate) const GL_EXP2: u32 = 0x0801;
pub(crate) const GL_ALPHA_TEST: u32 = 0x0BC0;
pub(crate) const GL_FLAT: u32 = 0x1D00;
pub(crate) const GL_SMOOTH: u32 = 0x1D01;
pub(crate) const GL_TEXTURE_ENV: u32 = 0x2300;
pub(crate) const GL_TEXTURE_ENV_MODE: u32 = 0x2200;
pub(crate) const GL_MODULATE: u32 = 0x2100;
pub(crate) const GL_DECAL: u32 = 0x2101;
pub(crate) const GL_ADD: u32 = 0x0104;
pub(crate) const GL_REPLACE: u32 = 0x1E01;
pub(crate) const GL_TEXTURE_CUBE_MAP: u32 = 0x8513;
pub(crate) const GL_TEXTURE_CUBE_MAP_POSITIVE_X: u32 = 0x8515;
pub(crate) const GL_CLAMP_TO_BORDER: i32 = 0x812D;
pub(crate) const GL_MIRRORED_REPEAT: i32 = 0x8370;
pub(crate) const GL_COMPRESSED_RGBA_S3TC_DXT1_EXT: u32 = 0x83F1;
pub(crate) const GL_COMPRESSED_RGBA_S3TC_DXT3_EXT: u32 = 0x83F2;
pub(crate) const GL_COMPRESSED_RGBA_S3TC_DXT5_EXT: u32 = 0x83F3;
pub(crate) const GL_UNSIGNED_SHORT_5_6_5: u32 = 0x8363;
pub(crate) const GL_UNSIGNED_SHORT_1_5_5_5_REV: u32 = 0x8366;
pub(crate) const GL_UNSIGNED_SHORT_4_4_4_4_REV: u32 = 0x8365;
pub(crate) const GL_UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;
pub(crate) const GL_LUMINANCE: u32 = 0x1909;
pub(crate) const GL_LUMINANCE_ALPHA: u32 = 0x190A;
pub(crate) const GL_ALPHA: u32 = 0x1906;
pub(crate) const GL_STENCIL_TEST: u32 = 0x0B90;
pub(crate) const GL_SCISSOR_TEST: u32 = 0x0C11;
pub(crate) const GL_POINT_SMOOTH: u32 = 0x0B10;
pub(crate) const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
pub(crate) const GL_DEPTH24_STENCIL8: u32 = 0x88F0;
pub(crate) const GL_FUNC_ADD: u32 = 0x8006;
pub(crate) const GL_FUNC_SUBTRACT: u32 = 0x800A;
pub(crate) const GL_FUNC_REVERSE_SUBTRACT: u32 = 0x800B;
pub(crate) const GL_MIN: u32 = 0x8007;
pub(crate) const GL_MAX: u32 = 0x8008;
pub(crate) const GL_TEXTURE_WRAP_R: u32 = 0x8072;

macro_rules! legacy_table {
    ($( $field:ident : $sym:literal => fn($($arg:ty),*) $(-> $ret:ty)? ),* $(,)?) => {
        pub(crate) struct GlLegacy {
            $( pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
        }
        unsafe impl Send for GlLegacy {}
        unsafe impl Sync for GlLegacy {}
        impl GlLegacy {
            unsafe fn load(get: &dyn Fn(&[u8]) -> *mut c_void) -> Option<Self> {
                Some(Self {
                    $( $field: {
                        let p = get(concat!($sym, "\0").as_bytes());
                        if p.is_null() {
                            crate::log_warn!("gl", "fonction GL de compatibilité absente : {}", $sym);
                            return None;
                        }
                        std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p)
                    }, )*
                })
            }
        }
    };
}

legacy_table! {
    MatrixMode: "glMatrixMode" => fn(u32),
    LoadIdentity: "glLoadIdentity" => fn(),
    LoadMatrixf: "glLoadMatrixf" => fn(*const f32),
    MultMatrixf: "glMultMatrixf" => fn(*const f32),
    Ortho: "glOrtho" => fn(f64, f64, f64, f64, f64, f64),
    EnableClientState: "glEnableClientState" => fn(u32),
    DisableClientState: "glDisableClientState" => fn(u32),
    VertexPointer: "glVertexPointer" => fn(i32, u32, i32, *const c_void),
    NormalPointer: "glNormalPointer" => fn(u32, i32, *const c_void),
    ColorPointer: "glColorPointer" => fn(i32, u32, i32, *const c_void),
    SecondaryColorPointer: "glSecondaryColorPointer" => fn(i32, u32, i32, *const c_void),
    TexCoordPointer: "glTexCoordPointer" => fn(i32, u32, i32, *const c_void),
    ClientActiveTexture: "glClientActiveTexture" => fn(u32),
    Lightfv: "glLightfv" => fn(u32, u32, *const f32),
    Lightf: "glLightf" => fn(u32, u32, f32),
    LightModelfv: "glLightModelfv" => fn(u32, *const f32),
    LightModeli: "glLightModeli" => fn(u32, i32),
    Materialfv: "glMaterialfv" => fn(u32, u32, *const f32),
    Materialf: "glMaterialf" => fn(u32, u32, f32),
    ColorMaterial: "glColorMaterial" => fn(u32, u32),
    Fogf: "glFogf" => fn(u32, f32),
    Fogfv: "glFogfv" => fn(u32, *const f32),
    Fogi: "glFogi" => fn(u32, i32),
    AlphaFunc: "glAlphaFunc" => fn(u32, f32),
    ShadeModel: "glShadeModel" => fn(u32),
    TexEnvi: "glTexEnvi" => fn(u32, u32, i32),
    ClearDepth: "glClearDepth" => fn(f64),
    ClearStencil: "glClearStencil" => fn(i32),
    DepthRange: "glDepthRange" => fn(f64, f64),
    BlendEquation: "glBlendEquation" => fn(u32),
    CompressedTexImage2D: "glCompressedTexImage2D" => fn(u32, i32, u32, i32, i32, i32, i32, *const c_void),
    GetAttribLocation: "glGetAttribLocation" => fn(u32, *const c_char) -> i32,
    BindAttribLocation: "glBindAttribLocation" => fn(u32, u32, *const c_char),
    Uniform4fv: "glUniform4fv" => fn(i32, i32, *const f32),
    Uniform4iv: "glUniform4iv" => fn(i32, i32, *const i32),
    Uniform1iv: "glUniform1iv" => fn(i32, i32, *const i32),
    VertexAttribPointer: "glVertexAttribPointer" => fn(u32, i32, u32, u8, i32, *const c_void),
    EnableVertexAttribArray: "glEnableVertexAttribArray" => fn(u32),
    DisableVertexAttribArray: "glDisableVertexAttribArray" => fn(u32),
    PointSize: "glPointSize" => fn(f32),
    StencilFunc: "glStencilFunc" => fn(u32, i32, u32),
    StencilOp: "glStencilOp" => fn(u32, u32, u32),
    StencilMask: "glStencilMask" => fn(u32),
    TexParameteri: "glTexParameteri" => fn(u32, u32, i32),
    DrawBuffer: "glDrawBuffer" => fn(u32),
}

/// Table chargée une fois ; `None` si libGL ou une entrée manque.
pub(crate) fn legacy() -> Option<&'static GlLegacy> {
    static L: OnceLock<Option<GlLegacy>> = OnceLock::new();
    L.get_or_init(|| unsafe {
        let lib = libc::dlopen(c"libGL.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            return None;
        }
        let gpa = libc::dlsym(lib, c"glXGetProcAddressARB".as_ptr());
        let gpa: Option<unsafe extern "C" fn(*const u8) -> *mut c_void> =
            (!gpa.is_null()).then(|| std::mem::transmute(gpa));
        GlLegacy::load(&|name: &[u8]| {
            let p = libc::dlsym(lib, name.as_ptr().cast());
            if !p.is_null() {
                return p;
            }
            match gpa {
                Some(f) => f(name.as_ptr()),
                None => std::ptr::null_mut(),
            }
        })
    })
    .as_ref()
}
