#![allow(non_snake_case, non_upper_case_globals, dead_code)]
//! Chargement dynamique d'OpenGL et GLX via dlopen(libGL.so.1).
//! Utilisé par dxgi.rs (swap chain, contexte GLX) et d3d11.rs (rendu).

use std::ffi::c_void;
use std::sync::OnceLock;

// ── Types GL ────────────────────────────────────────────────────────────────

pub(crate) type GLuint = u32;
pub(crate) type GLint = i32;
pub(crate) type GLenum = u32;
pub(crate) type GLsizei = i32;
pub(crate) type GLboolean = u8;
pub(crate) type GLfloat = f32;
pub(crate) type GLdouble = f64;
pub(crate) type GLchar = i8;
pub(crate) type GLsizeiptr = isize;
pub(crate) type GLintptr = isize;
pub(crate) type GLbitfield = u32;
pub(crate) type GLubyte = u8;
pub(crate) type GLvoid = c_void;
pub(crate) type GLclampf = f32;

// ── Constantes GL ───────────────────────────────────────────────────────────

// Booléens
pub(crate) const GL_TRUE: GLboolean = 1;
pub(crate) const GL_FALSE: GLboolean = 0;

// Types de primitive
pub(crate) const GL_POINTS: GLenum = 0x0000;
pub(crate) const GL_LINES: GLenum = 0x0001;
pub(crate) const GL_LINE_STRIP: GLenum = 0x0003;
pub(crate) const GL_TRIANGLES: GLenum = 0x0004;
pub(crate) const GL_TRIANGLE_STRIP: GLenum = 0x0005;
pub(crate) const GL_TRIANGLE_FAN: GLenum = 0x0006;

// Flags de clear
pub(crate) const GL_COLOR_BUFFER_BIT: GLbitfield = 0x00004000;
pub(crate) const GL_DEPTH_BUFFER_BIT: GLbitfield = 0x00000100;
pub(crate) const GL_STENCIL_BUFFER_BIT: GLbitfield = 0x00000400;

// Textures
pub(crate) const GL_TEXTURE_2D: GLenum = 0x0DE1;
pub(crate) const GL_TEXTURE0: GLenum = 0x84C0;
pub(crate) const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub(crate) const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub(crate) const GL_TEXTURE_WRAP_S: GLenum = 0x2802;
pub(crate) const GL_TEXTURE_WRAP_T: GLenum = 0x2803;
pub(crate) const GL_NEAREST: GLint = 0x2600;
pub(crate) const GL_LINEAR: GLint = 0x2601;
pub(crate) const GL_LINEAR_MIPMAP_LINEAR: GLint = 0x2703;
pub(crate) const GL_NEAREST_MIPMAP_NEAREST: GLint = 0x2700;
pub(crate) const GL_CLAMP_TO_EDGE: GLint = 0x812F;
pub(crate) const GL_REPEAT: GLint = 0x2901;
pub(crate) const GL_TEXTURE_MAX_LEVEL: GLenum = 0x813D;

// Formats internes / pixel
pub(crate) const GL_RED: GLenum = 0x1903;
pub(crate) const GL_RG: GLenum = 0x8227;
pub(crate) const GL_RGB: GLenum = 0x1907;
pub(crate) const GL_RGBA: GLenum = 0x1908;
pub(crate) const GL_BGR: GLenum = 0x80E0;
pub(crate) const GL_BGRA: GLenum = 0x80E1;
pub(crate) const GL_R8: GLenum = 0x8229;
pub(crate) const GL_RG8: GLenum = 0x822B;
pub(crate) const GL_RGB8: GLenum = 0x8051;
pub(crate) const GL_RGBA8: GLenum = 0x8058;
pub(crate) const GL_SRGB8_ALPHA8: GLenum = 0x8C43;
pub(crate) const GL_R16F: GLenum = 0x822D;
pub(crate) const GL_RG16F: GLenum = 0x822F;
pub(crate) const GL_RGBA16F: GLenum = 0x881A;
pub(crate) const GL_R32F: GLenum = 0x822E;
pub(crate) const GL_RG32F: GLenum = 0x8230;
pub(crate) const GL_RGB32F: GLenum = 0x8815;
pub(crate) const GL_RGBA32F: GLenum = 0x8814;
pub(crate) const GL_DEPTH_COMPONENT16: GLenum = 0x81A5;
pub(crate) const GL_DEPTH_COMPONENT24: GLenum = 0x81A6;
pub(crate) const GL_DEPTH_COMPONENT32F: GLenum = 0x8CAC;
pub(crate) const GL_DEPTH24_STENCIL8: GLenum = 0x88F0;
pub(crate) const GL_DEPTH32F_STENCIL8: GLenum = 0x8CAD;
pub(crate) const GL_DEPTH_COMPONENT: GLenum = 0x1902;
pub(crate) const GL_DEPTH_STENCIL: GLenum = 0x84F9;
pub(crate) const GL_R11F_G11F_B10F: GLenum = 0x8C3A;
pub(crate) const GL_RGB10_A2: GLenum = 0x8059;

// Types de données
pub(crate) const GL_UNSIGNED_BYTE: GLenum = 0x1401;
pub(crate) const GL_BYTE: GLenum = 0x1400;
pub(crate) const GL_UNSIGNED_SHORT: GLenum = 0x1403;
pub(crate) const GL_SHORT: GLenum = 0x1402;
pub(crate) const GL_UNSIGNED_INT: GLenum = 0x1405;
pub(crate) const GL_INT: GLenum = 0x1404;
pub(crate) const GL_HALF_FLOAT: GLenum = 0x140B;
pub(crate) const GL_FLOAT: GLenum = 0x1406;
pub(crate) const GL_UNSIGNED_INT_24_8: GLenum = 0x84FA;
pub(crate) const GL_UNSIGNED_INT_8_8_8_8_REV: GLenum = 0x8367;

// Framebuffer
pub(crate) const GL_FRAMEBUFFER: GLenum = 0x8D40;
pub(crate) const GL_READ_FRAMEBUFFER: GLenum = 0x8CA8;
pub(crate) const GL_DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
pub(crate) const GL_COLOR_ATTACHMENT0: GLenum = 0x8CE0;
pub(crate) const GL_DEPTH_ATTACHMENT: GLenum = 0x8D00;
pub(crate) const GL_STENCIL_ATTACHMENT: GLenum = 0x8D20;
pub(crate) const GL_DEPTH_STENCIL_ATTACHMENT: GLenum = 0x821A;
pub(crate) const GL_FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
pub(crate) const GL_RENDERBUFFER: GLenum = 0x8D41;

// Buffers
pub(crate) const GL_ARRAY_BUFFER: GLenum = 0x8892;
pub(crate) const GL_ELEMENT_ARRAY_BUFFER: GLenum = 0x8893;
pub(crate) const GL_UNIFORM_BUFFER: GLenum = 0x8A11;
pub(crate) const GL_STREAM_DRAW: GLenum = 0x88E0;
pub(crate) const GL_STREAM_READ: GLenum = 0x88E1;
pub(crate) const GL_STATIC_DRAW: GLenum = 0x88E4;
pub(crate) const GL_DYNAMIC_DRAW: GLenum = 0x88E8;
pub(crate) const GL_READ_ONLY: GLenum = 0x88B8;
pub(crate) const GL_WRITE_ONLY: GLenum = 0x88B9;
pub(crate) const GL_READ_WRITE: GLenum = 0x88BA;

// Shaders
pub(crate) const GL_VERTEX_SHADER: GLenum = 0x8B31;
pub(crate) const GL_FRAGMENT_SHADER: GLenum = 0x8B30;
pub(crate) const GL_GEOMETRY_SHADER: GLenum = 0x8DD9;
pub(crate) const GL_COMPILE_STATUS: GLenum = 0x8B81;
pub(crate) const GL_LINK_STATUS: GLenum = 0x8B82;
pub(crate) const GL_INFO_LOG_LENGTH: GLenum = 0x8B84;

// Blend
pub(crate) const GL_BLEND: GLenum = 0x0BE2;
pub(crate) const GL_ZERO: GLenum = 0;
pub(crate) const GL_ONE: GLenum = 1;
pub(crate) const GL_SRC_COLOR: GLenum = 0x0300;
pub(crate) const GL_ONE_MINUS_SRC_COLOR: GLenum = 0x0301;
pub(crate) const GL_SRC_ALPHA: GLenum = 0x0302;
pub(crate) const GL_ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub(crate) const GL_DST_ALPHA: GLenum = 0x0304;
pub(crate) const GL_ONE_MINUS_DST_ALPHA: GLenum = 0x0305;
pub(crate) const GL_DST_COLOR: GLenum = 0x0306;
pub(crate) const GL_ONE_MINUS_DST_COLOR: GLenum = 0x0307;
pub(crate) const GL_SRC_ALPHA_SATURATE: GLenum = 0x0308;
pub(crate) const GL_CONSTANT_COLOR: GLenum = 0x8001;

// Depth / Stencil
pub(crate) const GL_DEPTH_TEST: GLenum = 0x0B71;
pub(crate) const GL_STENCIL_TEST: GLenum = 0x0B90;
pub(crate) const GL_NEVER: GLenum = 0x0200;
pub(crate) const GL_LESS: GLenum = 0x0201;
pub(crate) const GL_EQUAL: GLenum = 0x0202;
pub(crate) const GL_LEQUAL: GLenum = 0x0203;
pub(crate) const GL_GREATER: GLenum = 0x0204;
pub(crate) const GL_NOTEQUAL: GLenum = 0x0205;
pub(crate) const GL_GEQUAL: GLenum = 0x0206;
pub(crate) const GL_ALWAYS: GLenum = 0x0207;
pub(crate) const GL_KEEP: GLenum = 0x1E00;
pub(crate) const GL_REPLACE: GLenum = 0x1E01;
pub(crate) const GL_INCR: GLenum = 0x1E02;
pub(crate) const GL_DECR: GLenum = 0x1E03;
pub(crate) const GL_INCR_WRAP: GLenum = 0x8507;
pub(crate) const GL_DECR_WRAP: GLenum = 0x8508;
pub(crate) const GL_INVERT: GLenum = 0x150A;

// Rasterisation
pub(crate) const GL_CULL_FACE: GLenum = 0x0B44;
pub(crate) const GL_FRONT: GLenum = 0x0404;
pub(crate) const GL_BACK: GLenum = 0x0405;
pub(crate) const GL_FRONT_AND_BACK: GLenum = 0x0408;
pub(crate) const GL_CW: GLenum = 0x0900;
pub(crate) const GL_CCW: GLenum = 0x0901;
pub(crate) const GL_POINT: GLenum = 0x1B00;
pub(crate) const GL_LINE: GLenum = 0x1B01;
pub(crate) const GL_FILL: GLenum = 0x1B02;
pub(crate) const GL_SCISSOR_TEST: GLenum = 0x0C11;

// Misc
pub(crate) const GL_NO_ERROR: GLenum = 0;
pub(crate) const GL_VERSION: GLenum = 0x1F02;
pub(crate) const GL_RENDERER: GLenum = 0x1F01;
pub(crate) const GL_VENDOR: GLenum = 0x1F00;
pub(crate) const GL_EXTENSIONS: GLenum = 0x1F03;
pub(crate) const GL_MAX_TEXTURE_SIZE: GLenum = 0x0D33;
pub(crate) const GL_MAX_VERTEX_ATTRIBS: GLenum = 0x8869;
pub(crate) const GL_UNPACK_ALIGNMENT: GLenum = 0x0CF5;
pub(crate) const GL_PACK_ALIGNMENT: GLenum = 0x0D05;
pub(crate) const GL_UNPACK_ROW_LENGTH: GLenum = 0x0CF2;

// Renderbuffer
pub(crate) const GL_MAX_RENDERBUFFER_SIZE: GLenum = 0x84E8;

// ── Constantes GLX ──────────────────────────────────────────────────────────

pub(crate) const GLX_RGBA: libc::c_int = 4;
pub(crate) const GLX_DOUBLEBUFFER: libc::c_int = 5;
pub(crate) const GLX_RED_SIZE: libc::c_int = 8;
pub(crate) const GLX_GREEN_SIZE: libc::c_int = 9;
pub(crate) const GLX_BLUE_SIZE: libc::c_int = 10;
pub(crate) const GLX_ALPHA_SIZE: libc::c_int = 11;
pub(crate) const GLX_DEPTH_SIZE: libc::c_int = 12;
pub(crate) const GLX_STENCIL_SIZE: libc::c_int = 13;
pub(crate) const GLX_RENDER_TYPE: libc::c_int = 0x8011;
pub(crate) const GLX_RGBA_BIT: libc::c_int = 0x0001;
pub(crate) const GLX_DRAWABLE_TYPE: libc::c_int = 0x8010;
pub(crate) const GLX_WINDOW_BIT: libc::c_int = 0x0001;
pub(crate) const GLX_X_RENDERABLE: libc::c_int = 0x8012;
pub(crate) const GLX_RGBA_TYPE: libc::c_int = 0x8014;
pub(crate) const GLX_NONE: libc::c_int = 0x8000;

// Opaque GLX types
pub(crate) type GLXContext = *mut c_void;
pub(crate) type GLXDrawable = libc::c_ulong;
pub(crate) type GLXFBConfig = *mut c_void;
pub(crate) type XDisplay = c_void;
pub(crate) type XVisualInfo = c_void;

// (macros de chargement définies localement dans load_gl / load_glx)

// ── Fonctions GL ────────────────────────────────────────────────────────────

pub(crate) struct GlFunctions {
    lib: *mut c_void,

    // Textures
    pub GenTextures: unsafe extern "C" fn(GLsizei, *mut GLuint),
    pub DeleteTextures: unsafe extern "C" fn(GLsizei, *const GLuint),
    pub BindTexture: unsafe extern "C" fn(GLenum, GLuint),
    pub TexImage2D: unsafe extern "C" fn(
        GLenum,
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLint,
        GLenum,
        GLenum,
        *const c_void,
    ),
    pub TexSubImage2D: unsafe extern "C" fn(
        GLenum,
        GLint,
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLenum,
        GLenum,
        *const c_void,
    ),
    pub TexParameteri: unsafe extern "C" fn(GLenum, GLenum, GLint),
    pub GenerateMipmap: unsafe extern "C" fn(GLenum),
    pub ActiveTexture: unsafe extern "C" fn(GLenum),

    // Framebuffer
    pub GenFramebuffers: unsafe extern "C" fn(GLsizei, *mut GLuint),
    pub DeleteFramebuffers: unsafe extern "C" fn(GLsizei, *const GLuint),
    pub BindFramebuffer: unsafe extern "C" fn(GLenum, GLuint),
    pub FramebufferTexture2D: unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint, GLint),
    pub CheckFramebufferStatus: unsafe extern "C" fn(GLenum) -> GLenum,

    // Renderbuffer
    pub GenRenderbuffers: unsafe extern "C" fn(GLsizei, *mut GLuint),
    pub DeleteRenderbuffers: unsafe extern "C" fn(GLsizei, *const GLuint),
    pub BindRenderbuffer: unsafe extern "C" fn(GLenum, GLuint),
    pub RenderbufferStorage: unsafe extern "C" fn(GLenum, GLenum, GLsizei, GLsizei),
    pub FramebufferRenderbuffer: unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint),

    // Viewport / clear
    pub Viewport: unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei),
    pub Scissor: unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei),
    pub Clear: unsafe extern "C" fn(GLbitfield),
    pub ClearColor: unsafe extern "C" fn(GLclampf, GLclampf, GLclampf, GLclampf),

    // State
    pub Enable: unsafe extern "C" fn(GLenum),
    pub Disable: unsafe extern "C" fn(GLenum),
    pub BlendFunc: unsafe extern "C" fn(GLenum, GLenum),
    pub BlendFuncSeparate: unsafe extern "C" fn(GLenum, GLenum, GLenum, GLenum),
    pub DepthFunc: unsafe extern "C" fn(GLenum),
    pub DepthMask: unsafe extern "C" fn(GLboolean),
    pub StencilFunc: unsafe extern "C" fn(GLenum, GLint, GLuint),
    pub StencilOp: unsafe extern "C" fn(GLenum, GLenum, GLenum),
    pub StencilMask: unsafe extern "C" fn(GLuint),
    pub CullFace: unsafe extern "C" fn(GLenum),
    pub FrontFace: unsafe extern "C" fn(GLenum),
    pub PolygonMode: unsafe extern "C" fn(GLenum, GLenum),
    pub LineWidth: unsafe extern "C" fn(GLfloat),
    pub PointSize: unsafe extern "C" fn(GLfloat),
    pub ColorMask: unsafe extern "C" fn(GLboolean, GLboolean, GLboolean, GLboolean),

    // Shaders
    pub CreateShader: unsafe extern "C" fn(GLenum) -> GLuint,
    pub DeleteShader: unsafe extern "C" fn(GLuint),
    pub ShaderSource: unsafe extern "C" fn(GLuint, GLsizei, *const *const GLchar, *const GLint),
    pub CompileShader: unsafe extern "C" fn(GLuint),
    pub GetShaderiv: unsafe extern "C" fn(GLuint, GLenum, *mut GLint),
    pub GetShaderInfoLog: unsafe extern "C" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar),

    // Programs
    pub CreateProgram: unsafe extern "C" fn() -> GLuint,
    pub DeleteProgram: unsafe extern "C" fn(GLuint),
    pub AttachShader: unsafe extern "C" fn(GLuint, GLuint),
    pub LinkProgram: unsafe extern "C" fn(GLuint),
    pub GetProgramiv: unsafe extern "C" fn(GLuint, GLenum, *mut GLint),
    pub GetProgramInfoLog: unsafe extern "C" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar),
    pub UseProgram: unsafe extern "C" fn(GLuint),

    // Uniforms
    pub GetUniformLocation: unsafe extern "C" fn(GLuint, *const GLchar) -> GLint,
    pub Uniform1i: unsafe extern "C" fn(GLint, GLint),
    pub Uniform1f: unsafe extern "C" fn(GLint, GLfloat),
    pub Uniform2f: unsafe extern "C" fn(GLint, GLfloat, GLfloat),
    pub Uniform3f: unsafe extern "C" fn(GLint, GLfloat, GLfloat, GLfloat),
    pub Uniform4f: unsafe extern "C" fn(GLint, GLfloat, GLfloat, GLfloat, GLfloat),
    pub UniformMatrix4fv: unsafe extern "C" fn(GLint, GLsizei, GLboolean, *const GLfloat),

    // Buffers
    pub GenBuffers: unsafe extern "C" fn(GLsizei, *mut GLuint),
    pub DeleteBuffers: unsafe extern "C" fn(GLsizei, *const GLuint),
    pub BindBuffer: unsafe extern "C" fn(GLenum, GLuint),
    pub BufferData: unsafe extern "C" fn(GLenum, GLsizeiptr, *const c_void, GLenum),
    pub BufferSubData: unsafe extern "C" fn(GLenum, GLintptr, GLsizeiptr, *const c_void),
    pub MapBuffer: unsafe extern "C" fn(GLenum, GLenum) -> *mut c_void,
    pub UnmapBuffer: unsafe extern "C" fn(GLenum) -> GLboolean,

    // VAO
    pub GenVertexArrays: unsafe extern "C" fn(GLsizei, *mut GLuint),
    pub DeleteVertexArrays: unsafe extern "C" fn(GLsizei, *const GLuint),
    pub BindVertexArray: unsafe extern "C" fn(GLuint),

    // Vertex attribs
    pub VertexAttribPointer:
        unsafe extern "C" fn(GLuint, GLint, GLenum, GLboolean, GLsizei, *const c_void),
    pub EnableVertexAttribArray: unsafe extern "C" fn(GLuint),
    pub DisableVertexAttribArray: unsafe extern "C" fn(GLuint),

    // Draw
    pub DrawArrays: unsafe extern "C" fn(GLenum, GLint, GLsizei),
    pub DrawElements: unsafe extern "C" fn(GLenum, GLsizei, GLenum, *const c_void),
    pub DrawArraysInstanced: unsafe extern "C" fn(GLenum, GLint, GLsizei, GLsizei),
    pub DrawElementsInstanced:
        unsafe extern "C" fn(GLenum, GLsizei, GLenum, *const c_void, GLsizei),

    // Blit
    pub BlitFramebuffer: unsafe extern "C" fn(
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLint,
        GLint,
        GLsizei,
        GLsizei,
        GLbitfield,
        GLenum,
    ),

    // Misc
    pub GetError: unsafe extern "C" fn() -> GLenum,
    pub GetString: unsafe extern "C" fn(GLenum) -> *const GLubyte,
    pub GetIntegerv: unsafe extern "C" fn(GLenum, *mut GLint),
    pub Flush: unsafe extern "C" fn(),
    pub Finish: unsafe extern "C" fn(),
    pub PixelStorei: unsafe extern "C" fn(GLenum, GLint),
    pub ReadPixels:
        unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut c_void),
}

unsafe impl Send for GlFunctions {}
unsafe impl Sync for GlFunctions {}

// ── Fonctions GLX ───────────────────────────────────────────────────────────

pub(crate) struct GlxFunctions {
    pub available: bool,
    // X11 basics (loaded from libX11 via libGL)
    pub XOpenDisplay: unsafe extern "C" fn(*const libc::c_char) -> *mut XDisplay,
    pub XCloseDisplay: unsafe extern "C" fn(*mut XDisplay) -> libc::c_int,
    pub XDefaultScreen: unsafe extern "C" fn(*mut XDisplay) -> libc::c_int,
    pub XDefaultRootWindow: unsafe extern "C" fn(*mut XDisplay) -> libc::c_ulong,
    // GLX
    pub ChooseVisual:
        unsafe extern "C" fn(*mut XDisplay, libc::c_int, *const libc::c_int) -> *mut XVisualInfo,
    pub CreateContext: unsafe extern "C" fn(
        *mut XDisplay,
        *mut XVisualInfo,
        GLXContext,
        libc::c_int,
    ) -> GLXContext,
    pub MakeCurrent: unsafe extern "C" fn(*mut XDisplay, GLXDrawable, GLXContext) -> libc::c_int,
    pub SwapBuffers: unsafe extern "C" fn(*mut XDisplay, GLXDrawable),
    pub DestroyContext: unsafe extern "C" fn(*mut XDisplay, GLXContext),
    pub GetProcAddressARB: unsafe extern "C" fn(*const GLubyte) -> *mut c_void,
    pub ChooseFBConfig: unsafe extern "C" fn(
        *mut XDisplay,
        libc::c_int,
        *const libc::c_int,
        *mut libc::c_int,
    ) -> *mut GLXFBConfig,
    pub GetVisualFromFBConfig: unsafe extern "C" fn(*mut XDisplay, GLXFBConfig) -> *mut XVisualInfo,
    pub CreateNewContext: unsafe extern "C" fn(
        *mut XDisplay,
        GLXFBConfig,
        libc::c_int,
        GLXContext,
        libc::c_int,
    ) -> GLXContext,
}

unsafe impl Send for GlxFunctions {}
unsafe impl Sync for GlxFunctions {}

// ── Chargement ──────────────────────────────────────────────────────────────

static GL: OnceLock<Option<GlFunctions>> = OnceLock::new();
static GLX: OnceLock<Option<GlxFunctions>> = OnceLock::new();

unsafe fn noop_void() {}
unsafe fn noop_ret0() -> GLuint {
    0
}

#[allow(clippy::manual_c_str_literals, clippy::missing_transmute_annotations)]
unsafe fn load_gl(lib: *mut c_void) -> GlFunctions {
    // Stub par défaut : ne fait rien / renvoie 0. Utilisé si un symbole manque
    // (évite un crash sur un driver minimal).
    let nv = noop_void as *mut c_void;
    let n0 = noop_ret0 as *mut c_void;

    macro_rules! sym {
        ($name:literal, $ty:ty) => {{
            let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                crate::log_debug!("gl", "symbole manquant : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { nv } else { p })
        }};
        (ret $name:literal, $ty:ty) => {{
            let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                crate::log_debug!("gl", "symbole manquant : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { n0 } else { p })
        }};
    }

    // For GL 3.0+ extension functions, try glXGetProcAddressARB first
    let get_proc: Option<unsafe extern "C" fn(*const GLubyte) -> *mut c_void> = {
        let p = libc::dlsym(lib, b"glXGetProcAddressARB\0".as_ptr().cast());
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute(p))
        }
    };

    macro_rules! ext {
        ($name:literal, $ty:ty) => {{
            let mut p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                if let Some(gpa) = get_proc {
                    p = gpa(concat!($name, "\0").as_ptr().cast());
                }
            }
            if p.is_null() {
                crate::log_debug!("gl", "extension manquante : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { nv } else { p })
        }};
        (ret $name:literal, $ty:ty) => {{
            let mut p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                if let Some(gpa) = get_proc {
                    p = gpa(concat!($name, "\0").as_ptr().cast());
                }
            }
            if p.is_null() {
                crate::log_debug!("gl", "extension manquante : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { n0 } else { p })
        }};
    }

    type FnVoid = unsafe extern "C" fn();
    type FnRet = unsafe extern "C" fn() -> GLuint;

    GlFunctions {
        lib,

        // Textures (core GL, always available)
        GenTextures: sym!("glGenTextures", unsafe extern "C" fn(GLsizei, *mut GLuint)),
        DeleteTextures: sym!(
            "glDeleteTextures",
            unsafe extern "C" fn(GLsizei, *const GLuint)
        ),
        BindTexture: sym!("glBindTexture", unsafe extern "C" fn(GLenum, GLuint)),
        TexImage2D: sym!(
            "glTexImage2D",
            unsafe extern "C" fn(
                GLenum,
                GLint,
                GLint,
                GLsizei,
                GLsizei,
                GLint,
                GLenum,
                GLenum,
                *const c_void,
            )
        ),
        TexSubImage2D: sym!(
            "glTexSubImage2D",
            unsafe extern "C" fn(
                GLenum,
                GLint,
                GLint,
                GLint,
                GLsizei,
                GLsizei,
                GLenum,
                GLenum,
                *const c_void,
            )
        ),
        TexParameteri: sym!(
            "glTexParameteri",
            unsafe extern "C" fn(GLenum, GLenum, GLint)
        ),
        GenerateMipmap: ext!("glGenerateMipmap", unsafe extern "C" fn(GLenum)),
        ActiveTexture: sym!("glActiveTexture", unsafe extern "C" fn(GLenum)),

        // Framebuffer (GL 3.0 / ARB_framebuffer_object)
        GenFramebuffers: ext!(
            "glGenFramebuffers",
            unsafe extern "C" fn(GLsizei, *mut GLuint)
        ),
        DeleteFramebuffers: ext!(
            "glDeleteFramebuffers",
            unsafe extern "C" fn(GLsizei, *const GLuint)
        ),
        BindFramebuffer: ext!("glBindFramebuffer", unsafe extern "C" fn(GLenum, GLuint)),
        FramebufferTexture2D: ext!(
            "glFramebufferTexture2D",
            unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint, GLint)
        ),
        CheckFramebufferStatus: ext!(ret "glCheckFramebufferStatus", unsafe extern "C" fn(GLenum) -> GLenum),

        // Renderbuffer
        GenRenderbuffers: ext!(
            "glGenRenderbuffers",
            unsafe extern "C" fn(GLsizei, *mut GLuint)
        ),
        DeleteRenderbuffers: ext!(
            "glDeleteRenderbuffers",
            unsafe extern "C" fn(GLsizei, *const GLuint)
        ),
        BindRenderbuffer: ext!("glBindRenderbuffer", unsafe extern "C" fn(GLenum, GLuint)),
        RenderbufferStorage: ext!(
            "glRenderbufferStorage",
            unsafe extern "C" fn(GLenum, GLenum, GLsizei, GLsizei)
        ),
        FramebufferRenderbuffer: ext!(
            "glFramebufferRenderbuffer",
            unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint)
        ),

        // Viewport / clear
        Viewport: sym!(
            "glViewport",
            unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei)
        ),
        Scissor: sym!(
            "glScissor",
            unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei)
        ),
        Clear: sym!("glClear", unsafe extern "C" fn(GLbitfield)),
        ClearColor: sym!(
            "glClearColor",
            unsafe extern "C" fn(GLclampf, GLclampf, GLclampf, GLclampf)
        ),

        // State
        Enable: sym!("glEnable", unsafe extern "C" fn(GLenum)),
        Disable: sym!("glDisable", unsafe extern "C" fn(GLenum)),
        BlendFunc: sym!("glBlendFunc", unsafe extern "C" fn(GLenum, GLenum)),
        BlendFuncSeparate: ext!(
            "glBlendFuncSeparate",
            unsafe extern "C" fn(GLenum, GLenum, GLenum, GLenum)
        ),
        DepthFunc: sym!("glDepthFunc", unsafe extern "C" fn(GLenum)),
        DepthMask: sym!("glDepthMask", unsafe extern "C" fn(GLboolean)),
        StencilFunc: sym!("glStencilFunc", unsafe extern "C" fn(GLenum, GLint, GLuint)),
        StencilOp: sym!("glStencilOp", unsafe extern "C" fn(GLenum, GLenum, GLenum)),
        StencilMask: sym!("glStencilMask", unsafe extern "C" fn(GLuint)),
        CullFace: sym!("glCullFace", unsafe extern "C" fn(GLenum)),
        FrontFace: sym!("glFrontFace", unsafe extern "C" fn(GLenum)),
        PolygonMode: sym!("glPolygonMode", unsafe extern "C" fn(GLenum, GLenum)),
        LineWidth: sym!("glLineWidth", unsafe extern "C" fn(GLfloat)),
        PointSize: sym!("glPointSize", unsafe extern "C" fn(GLfloat)),
        ColorMask: sym!(
            "glColorMask",
            unsafe extern "C" fn(GLboolean, GLboolean, GLboolean, GLboolean)
        ),

        // Shaders (GL 2.0)
        CreateShader: ext!(ret "glCreateShader", unsafe extern "C" fn(GLenum) -> GLuint),
        DeleteShader: ext!("glDeleteShader", unsafe extern "C" fn(GLuint)),
        ShaderSource: ext!(
            "glShaderSource",
            unsafe extern "C" fn(GLuint, GLsizei, *const *const GLchar, *const GLint)
        ),
        CompileShader: ext!("glCompileShader", unsafe extern "C" fn(GLuint)),
        GetShaderiv: ext!(
            "glGetShaderiv",
            unsafe extern "C" fn(GLuint, GLenum, *mut GLint)
        ),
        GetShaderInfoLog: ext!(
            "glGetShaderInfoLog",
            unsafe extern "C" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar)
        ),

        // Programs (GL 2.0)
        CreateProgram: ext!(ret "glCreateProgram", unsafe extern "C" fn() -> GLuint),
        DeleteProgram: ext!("glDeleteProgram", unsafe extern "C" fn(GLuint)),
        AttachShader: ext!("glAttachShader", unsafe extern "C" fn(GLuint, GLuint)),
        LinkProgram: ext!("glLinkProgram", unsafe extern "C" fn(GLuint)),
        GetProgramiv: ext!(
            "glGetProgramiv",
            unsafe extern "C" fn(GLuint, GLenum, *mut GLint)
        ),
        GetProgramInfoLog: ext!(
            "glGetProgramInfoLog",
            unsafe extern "C" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar)
        ),
        UseProgram: ext!("glUseProgram", unsafe extern "C" fn(GLuint)),

        // Uniforms
        GetUniformLocation: ext!(ret "glGetUniformLocation", unsafe extern "C" fn(GLuint, *const GLchar) -> GLint),
        Uniform1i: ext!("glUniform1i", unsafe extern "C" fn(GLint, GLint)),
        Uniform1f: ext!("glUniform1f", unsafe extern "C" fn(GLint, GLfloat)),
        Uniform2f: ext!("glUniform2f", unsafe extern "C" fn(GLint, GLfloat, GLfloat)),
        Uniform3f: ext!(
            "glUniform3f",
            unsafe extern "C" fn(GLint, GLfloat, GLfloat, GLfloat)
        ),
        Uniform4f: ext!(
            "glUniform4f",
            unsafe extern "C" fn(GLint, GLfloat, GLfloat, GLfloat, GLfloat)
        ),
        UniformMatrix4fv: ext!(
            "glUniformMatrix4fv",
            unsafe extern "C" fn(GLint, GLsizei, GLboolean, *const GLfloat)
        ),

        // Buffers
        GenBuffers: ext!("glGenBuffers", unsafe extern "C" fn(GLsizei, *mut GLuint)),
        DeleteBuffers: ext!(
            "glDeleteBuffers",
            unsafe extern "C" fn(GLsizei, *const GLuint)
        ),
        BindBuffer: ext!("glBindBuffer", unsafe extern "C" fn(GLenum, GLuint)),
        BufferData: ext!(
            "glBufferData",
            unsafe extern "C" fn(GLenum, GLsizeiptr, *const c_void, GLenum)
        ),
        BufferSubData: ext!(
            "glBufferSubData",
            unsafe extern "C" fn(GLenum, GLintptr, GLsizeiptr, *const c_void)
        ),
        MapBuffer: ext!(ret "glMapBuffer", unsafe extern "C" fn(GLenum, GLenum) -> *mut c_void),
        UnmapBuffer: ext!(ret "glUnmapBuffer", unsafe extern "C" fn(GLenum) -> GLboolean),

        // VAO (GL 3.0 / ARB_vertex_array_object)
        GenVertexArrays: ext!(
            "glGenVertexArrays",
            unsafe extern "C" fn(GLsizei, *mut GLuint)
        ),
        DeleteVertexArrays: ext!(
            "glDeleteVertexArrays",
            unsafe extern "C" fn(GLsizei, *const GLuint)
        ),
        BindVertexArray: ext!("glBindVertexArray", unsafe extern "C" fn(GLuint)),

        // Vertex attribs
        VertexAttribPointer: ext!(
            "glVertexAttribPointer",
            unsafe extern "C" fn(GLuint, GLint, GLenum, GLboolean, GLsizei, *const c_void)
        ),
        EnableVertexAttribArray: ext!("glEnableVertexAttribArray", unsafe extern "C" fn(GLuint)),
        DisableVertexAttribArray: ext!("glDisableVertexAttribArray", unsafe extern "C" fn(GLuint)),

        // Draw
        DrawArrays: sym!("glDrawArrays", unsafe extern "C" fn(GLenum, GLint, GLsizei)),
        DrawElements: sym!(
            "glDrawElements",
            unsafe extern "C" fn(GLenum, GLsizei, GLenum, *const c_void)
        ),
        DrawArraysInstanced: ext!(
            "glDrawArraysInstanced",
            unsafe extern "C" fn(GLenum, GLint, GLsizei, GLsizei)
        ),
        DrawElementsInstanced: ext!(
            "glDrawElementsInstanced",
            unsafe extern "C" fn(GLenum, GLsizei, GLenum, *const c_void, GLsizei)
        ),

        // Blit
        BlitFramebuffer: ext!(
            "glBlitFramebuffer",
            unsafe extern "C" fn(
                GLint,
                GLint,
                GLsizei,
                GLsizei,
                GLint,
                GLint,
                GLsizei,
                GLsizei,
                GLbitfield,
                GLenum,
            )
        ),

        // Misc
        GetError: sym!(ret "glGetError", unsafe extern "C" fn() -> GLenum),
        GetString: sym!(ret "glGetString", unsafe extern "C" fn(GLenum) -> *const GLubyte),
        GetIntegerv: sym!("glGetIntegerv", unsafe extern "C" fn(GLenum, *mut GLint)),
        Flush: sym!("glFlush", unsafe extern "C" fn()),
        Finish: sym!("glFinish", unsafe extern "C" fn()),
        PixelStorei: sym!("glPixelStorei", unsafe extern "C" fn(GLenum, GLint)),
        ReadPixels: sym!(
            "glReadPixels",
            unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut c_void)
        ),
    }
}

#[allow(clippy::manual_c_str_literals, clippy::missing_transmute_annotations)]
unsafe fn load_glx(lib: *mut c_void) -> GlxFunctions {
    let nv = noop_void as *mut c_void;
    let n0 = noop_ret0 as *mut c_void;

    macro_rules! sym {
        ($name:literal, $ty:ty) => {{
            let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                crate::log_debug!("gl", "symbole GLX manquant : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { nv } else { p })
        }};
        (ret $name:literal, $ty:ty) => {{
            let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                crate::log_debug!("gl", "symbole GLX manquant : {}", $name);
            }
            std::mem::transmute::<*mut c_void, $ty>(if p.is_null() { n0 } else { p })
        }};
    }

    // Load X11 from libX11.so.6 (libGL links against it)
    let x11lib = libc::dlopen(
        b"libX11.so.6\0".as_ptr().cast(),
        libc::RTLD_LAZY | libc::RTLD_LOCAL,
    );
    let x_open = if !x11lib.is_null() {
        libc::dlsym(x11lib, b"XOpenDisplay\0".as_ptr().cast())
    } else {
        libc::dlsym(lib, b"XOpenDisplay\0".as_ptr().cast())
    };
    let x_close = if !x11lib.is_null() {
        libc::dlsym(x11lib, b"XCloseDisplay\0".as_ptr().cast())
    } else {
        libc::dlsym(lib, b"XCloseDisplay\0".as_ptr().cast())
    };
    let x_screen = if !x11lib.is_null() {
        libc::dlsym(x11lib, b"XDefaultScreen\0".as_ptr().cast())
    } else {
        libc::dlsym(lib, b"XDefaultScreen\0".as_ptr().cast())
    };
    let x_root = if !x11lib.is_null() {
        libc::dlsym(x11lib, b"XDefaultRootWindow\0".as_ptr().cast())
    } else {
        libc::dlsym(lib, b"XDefaultRootWindow\0".as_ptr().cast())
    };

    let x11_ok =
        !x_open.is_null() && !x_close.is_null() && !x_screen.is_null() && !x_root.is_null();

    GlxFunctions {
        available: x11_ok,
        XOpenDisplay: std::mem::transmute(if x_open.is_null() { nv } else { x_open }),
        XCloseDisplay: std::mem::transmute(if x_close.is_null() { n0 } else { x_close }),
        XDefaultScreen: std::mem::transmute(if x_screen.is_null() { n0 } else { x_screen }),
        XDefaultRootWindow: std::mem::transmute(if x_root.is_null() { n0 } else { x_root }),
        ChooseVisual: sym!(ret "glXChooseVisual", unsafe extern "C" fn(*mut XDisplay, libc::c_int, *const libc::c_int) -> *mut XVisualInfo),
        CreateContext: sym!(ret "glXCreateContext", unsafe extern "C" fn(*mut XDisplay, *mut XVisualInfo, GLXContext, libc::c_int) -> GLXContext),
        MakeCurrent: sym!(ret "glXMakeCurrent", unsafe extern "C" fn(*mut XDisplay, GLXDrawable, GLXContext) -> libc::c_int),
        SwapBuffers: sym!(
            "glXSwapBuffers",
            unsafe extern "C" fn(*mut XDisplay, GLXDrawable)
        ),
        DestroyContext: sym!(
            "glXDestroyContext",
            unsafe extern "C" fn(*mut XDisplay, GLXContext)
        ),
        GetProcAddressARB: sym!(ret "glXGetProcAddressARB", unsafe extern "C" fn(*const GLubyte) -> *mut c_void),
        ChooseFBConfig: sym!(ret "glXChooseFBConfig", unsafe extern "C" fn(*mut XDisplay, libc::c_int, *const libc::c_int, *mut libc::c_int) -> *mut GLXFBConfig),
        GetVisualFromFBConfig: sym!(ret "glXGetVisualFromFBConfig", unsafe extern "C" fn(*mut XDisplay, GLXFBConfig) -> *mut XVisualInfo),
        CreateNewContext: sym!(ret "glXCreateNewContext", unsafe extern "C" fn(*mut XDisplay, GLXFBConfig, libc::c_int, GLXContext, libc::c_int) -> GLXContext),
    }
}

pub(crate) fn gl() -> Option<&'static GlFunctions> {
    GL.get_or_init(|| unsafe {
        let lib = libc::dlopen(c"libGL.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            crate::log_error!("gl", "impossible de charger libGL.so.1");
            return None;
        }
        crate::log_info!("gl", "libGL.so.1 chargé");
        Some(load_gl(lib))
    })
    .as_ref()
}

pub(crate) fn glx() -> Option<&'static GlxFunctions> {
    GLX.get_or_init(|| unsafe {
        // GLX lives inside libGL.so.1 too
        let lib = libc::dlopen(c"libGL.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            crate::log_error!("gl", "impossible de charger libGL.so.1 pour GLX");
            return None;
        }
        Some(load_glx(lib))
    })
    .as_ref()
}

/// Returns true if the OpenGL backend loaded successfully.
pub(crate) fn is_available() -> bool {
    gl().is_some()
}
