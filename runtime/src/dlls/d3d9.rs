//! Direct3D 9 (`d3d9.dll`) traduit vers OpenGL (profil de compatibilité).
//!
//! Couvre `IDirect3D9(Ex)`, `IDirect3DDevice9(Ex)`, surfaces, textures (2D,
//! cube), vertex/index buffers, déclarations de sommets et FVF, shaders
//! SM 1.x à 3.0 (via `d3d9_shader`), états de rendu du pipeline fixe
//! (transformations, éclairage, matériaux, brouillard, étages de texture),
//! state blocks, requêtes et présentation dans la fenêtre.
//!
//! Choix d'implémentation :
//! - les ressources gardent une copie CPU (Lock/Unlock sans aller-retour
//!   GPU) ; les sommets sont fournis à OpenGL en tableaux côté client ;
//! - le rendu est retourné verticalement dans les cibles (ligne 0 = haut,
//!   comme Direct3D) et la présentation inverse la copie : les textures
//!   rendues et relues ont la même orientation que sous Windows ;
//! - garde-fous : sans X11 ou sans OpenGL, l'appareil est créé en mode
//!   « sans rendu » (ressources, états et verrous fonctionnent, rien ne
//!   s'affiche) au lieu d'échouer ou de planter.
#![allow(non_snake_case, dead_code, clippy::missing_safety_doc)]

use crate::dlls::d3d9_shader::{self, Sm9Shader};
use crate::dlls::dxgi::GlContext;
use crate::dlls::gl_bindings as gl;
use crate::dlls::gl_legacy::{self as glc, GlLegacy};
use crate::dlls::ole32::Guid;
use crate::{log_info, log_warn};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

// ── HRESULT ────────────────────────────────────────────────────────────────
const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
pub(crate) const D3DERR_INVALIDCALL: i32 = 0x8876_086cu32 as i32;
pub(crate) const D3DERR_NOTAVAILABLE: i32 = 0x8876_086au32 as i32;
const D3DERR_NOTFOUND: i32 = 0x8876_0866u32 as i32;
const D3D_OK: i32 = 0;

// ── Formats ────────────────────────────────────────────────────────────────
pub(crate) const D3DFMT_UNKNOWN: u32 = 0;
pub(crate) const D3DFMT_R8G8B8: u32 = 20;
pub(crate) const D3DFMT_A8R8G8B8: u32 = 21;
pub(crate) const D3DFMT_X8R8G8B8: u32 = 22;
pub(crate) const D3DFMT_R5G6B5: u32 = 23;
pub(crate) const D3DFMT_X1R5G5B5: u32 = 24;
pub(crate) const D3DFMT_A1R5G5B5: u32 = 25;
pub(crate) const D3DFMT_A4R4G4B4: u32 = 26;
pub(crate) const D3DFMT_A8: u32 = 28;
pub(crate) const D3DFMT_A8B8G8R8: u32 = 32;
pub(crate) const D3DFMT_X8B8G8R8: u32 = 33;
pub(crate) const D3DFMT_L8: u32 = 50;
pub(crate) const D3DFMT_A8L8: u32 = 51;
pub(crate) const D3DFMT_D16_LOCKABLE: u32 = 70;
pub(crate) const D3DFMT_D32: u32 = 71;
pub(crate) const D3DFMT_D24S8: u32 = 75;
pub(crate) const D3DFMT_D24X8: u32 = 77;
pub(crate) const D3DFMT_D16: u32 = 80;
pub(crate) const D3DFMT_D32F_LOCKABLE: u32 = 82;
pub(crate) const D3DFMT_INDEX16: u32 = 101;
pub(crate) const D3DFMT_INDEX32: u32 = 102;
pub(crate) const D3DFMT_R16F: u32 = 111;
pub(crate) const D3DFMT_G16R16F: u32 = 112;
pub(crate) const D3DFMT_A16B16G16R16F: u32 = 113;
pub(crate) const D3DFMT_R32F: u32 = 114;
pub(crate) const D3DFMT_G32R32F: u32 = 115;
pub(crate) const D3DFMT_A32B32G32R32F: u32 = 116;
pub(crate) const D3DFMT_DXT1: u32 = u32::from_le_bytes(*b"DXT1");
pub(crate) const D3DFMT_DXT2: u32 = u32::from_le_bytes(*b"DXT2");
pub(crate) const D3DFMT_DXT3: u32 = u32::from_le_bytes(*b"DXT3");
pub(crate) const D3DFMT_DXT4: u32 = u32::from_le_bytes(*b"DXT4");
pub(crate) const D3DFMT_DXT5: u32 = u32::from_le_bytes(*b"DXT5");
pub(crate) const D3DFMT_NV12: u32 = u32::from_le_bytes(*b"NV12");
pub(crate) const D3DFMT_P010: u32 = u32::from_le_bytes(*b"P010");
pub(crate) const D3DFMT_YV12: u32 = u32::from_le_bytes(*b"YV12");

const D3DPOOL_DEFAULT: u32 = 0;
const D3DPOOL_MANAGED: u32 = 1;
const D3DPOOL_SYSTEMMEM: u32 = 2;
const D3DUSAGE_RENDERTARGET: u32 = 0x1;
const D3DUSAGE_DEPTHSTENCIL: u32 = 0x2;
const D3DUSAGE_DYNAMIC: u32 = 0x200;
const D3DUSAGE_AUTOGENMIPMAP: u32 = 0x400;

const D3DRTYPE_SURFACE: u32 = 1;
const D3DRTYPE_VOLUME: u32 = 2;
const D3DRTYPE_TEXTURE: u32 = 3;
const D3DRTYPE_VOLUMETEXTURE: u32 = 4;
const D3DRTYPE_CUBETEXTURE: u32 = 5;
const D3DRTYPE_VERTEXBUFFER: u32 = 6;
const D3DRTYPE_INDEXBUFFER: u32 = 7;

/// Description GL d'un format D3D9 : (interne, format, type, octets/pixel,
/// taille de bloc compressé ou 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FormatInfo {
    pub internal: u32,
    pub format: u32,
    pub ty: u32,
    pub bpp: u32,
    pub block: u32,
}

pub(crate) fn format_info(f: u32) -> Option<FormatInfo> {
    let fi = |internal, format, ty, bpp| FormatInfo {
        internal,
        format,
        ty,
        bpp,
        block: 0,
    };
    let bc = |internal, block| FormatInfo {
        internal,
        format: 0,
        ty: 0,
        bpp: 0,
        block,
    };
    Some(match f {
        D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8 => fi(gl::GL_RGBA8, gl::GL_BGRA, gl::GL_UNSIGNED_BYTE, 4),
        D3DFMT_A8B8G8R8 | D3DFMT_X8B8G8R8 => fi(gl::GL_RGBA8, gl::GL_RGBA, gl::GL_UNSIGNED_BYTE, 4),
        D3DFMT_R5G6B5 => fi(gl::GL_RGB, gl::GL_RGB, glc::GL_UNSIGNED_SHORT_5_6_5, 2),
        D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 => {
            fi(gl::GL_RGBA, gl::GL_BGRA, glc::GL_UNSIGNED_SHORT_1_5_5_5_REV, 2)
        }
        D3DFMT_A4R4G4B4 => fi(gl::GL_RGBA, gl::GL_BGRA, glc::GL_UNSIGNED_SHORT_4_4_4_4_REV, 2),
        D3DFMT_A8 => fi(glc::GL_ALPHA, glc::GL_ALPHA, gl::GL_UNSIGNED_BYTE, 1),
        D3DFMT_L8 => fi(glc::GL_LUMINANCE, glc::GL_LUMINANCE, gl::GL_UNSIGNED_BYTE, 1),
        D3DFMT_A8L8 => fi(glc::GL_LUMINANCE_ALPHA, glc::GL_LUMINANCE_ALPHA, gl::GL_UNSIGNED_BYTE, 2),
        D3DFMT_R16F => fi(gl::GL_R16F, gl::GL_RED, gl::GL_HALF_FLOAT, 2),
        D3DFMT_G16R16F => fi(gl::GL_RG16F, gl::GL_RG, gl::GL_HALF_FLOAT, 4),
        D3DFMT_A16B16G16R16F => fi(gl::GL_RGBA16F, gl::GL_RGBA, gl::GL_HALF_FLOAT, 8),
        D3DFMT_R32F => fi(gl::GL_R32F, gl::GL_RED, gl::GL_FLOAT, 4),
        D3DFMT_G32R32F => fi(gl::GL_RG32F, gl::GL_RG, gl::GL_FLOAT, 8),
        D3DFMT_A32B32G32R32F => fi(gl::GL_RGBA32F, gl::GL_RGBA, gl::GL_FLOAT, 16),
        D3DFMT_DXT1 => bc(glc::GL_COMPRESSED_RGBA_S3TC_DXT1_EXT, 8),
        D3DFMT_DXT2 | D3DFMT_DXT3 => bc(glc::GL_COMPRESSED_RGBA_S3TC_DXT3_EXT, 16),
        D3DFMT_DXT4 | D3DFMT_DXT5 => bc(glc::GL_COMPRESSED_RGBA_S3TC_DXT5_EXT, 16),
        D3DFMT_D16 | D3DFMT_D16_LOCKABLE => fi(gl::GL_DEPTH_COMPONENT16, gl::GL_DEPTH_COMPONENT, gl::GL_UNSIGNED_SHORT, 2),
        D3DFMT_D24S8 | D3DFMT_D24X8 | D3DFMT_D32 => {
            fi(gl::GL_DEPTH24_STENCIL8, gl::GL_DEPTH_STENCIL, gl::GL_UNSIGNED_INT_24_8, 4)
        }
        D3DFMT_D32F_LOCKABLE => fi(gl::GL_DEPTH_COMPONENT32F, gl::GL_DEPTH_COMPONENT, gl::GL_FLOAT, 4),
        // Formats vidéo (DXVA2) : stockage CPU uniquement.
        D3DFMT_NV12 | D3DFMT_YV12 => fi(0, 0, 0, 1),
        D3DFMT_P010 => fi(0, 0, 0, 2),
        _ => return None,
    })
}

fn is_depth(f: u32) -> bool {
    matches!(
        f,
        D3DFMT_D16 | D3DFMT_D16_LOCKABLE | D3DFMT_D24S8 | D3DFMT_D24X8 | D3DFMT_D32 | D3DFMT_D32F_LOCKABLE
    )
}

fn is_video(f: u32) -> bool {
    matches!(f, D3DFMT_NV12 | D3DFMT_YV12 | D3DFMT_P010)
}

/// (pitch, taille) d'un niveau.
pub(crate) fn level_layout(f: u32, w: u32, h: u32) -> (u32, usize) {
    let Some(fi) = format_info(f) else {
        return (w * 4, (w * 4 * h) as usize);
    };
    if fi.block != 0 {
        let bw = w.div_ceil(4).max(1);
        let bh = h.div_ceil(4).max(1);
        return (bw * fi.block, (bw * fi.block * bh) as usize);
    }
    let pitch = (w * fi.bpp).next_multiple_of(4);
    let rows = if is_video(f) { h + h.div_ceil(2) } else { h };
    (pitch, (pitch * rows) as usize)
}

// ── Objets COM ─────────────────────────────────────────────────────────────

/// Tous les objets D3D9 partagent cet en-tête : vtable, compteur, appareil.
#[repr(C)]
pub(crate) struct Obj {
    vtbl: *const usize,
    refs: AtomicU32,
    device: *mut Obj,
    kind: Kind,
}

unsafe impl Send for Obj {}
unsafe impl Sync for Obj {}

enum Kind {
    D3D9 { ex: bool },
    Device(Box<Mutex<Dev>>),
    Surface(Mutex<SurfaceData>),
    Texture(Mutex<TexData>),
    Cube(Mutex<TexData>),
    Volume(Mutex<VolData>),
    VertexBuffer(Mutex<BufData>),
    IndexBuffer(Mutex<BufData>),
    Decl(Vec<VertexElement>),
    VertexShader(Box<ShaderData>),
    PixelShader(Box<ShaderData>),
    Query(u32),
    StateBlock(Mutex<Option<Box<Settable>>>),
    SwapChain,
}

pub(crate) struct SurfaceData {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub usage: u32,
    pub pool: u32,
    /// Texture parente (GetSurfaceLevel / GetCubeMapSurface) : (texture, face, niveau).
    pub parent: Option<(*mut Obj, u32, u32)>,
    pub data: Vec<u8>,
    pub pitch: u32,
    /// Texture GL (couleur) ou renderbuffer (profondeur) quand la surface est une cible.
    pub gl_tex: u32,
    pub gl_rb: u32,
    pub cpu_dirty: bool,
    pub gpu_dirty: bool,
}

struct Level {
    width: u32,
    height: u32,
    pitch: u32,
    data: Vec<u8>,
}

struct TexData {
    format: u32,
    usage: u32,
    pool: u32,
    faces: u32,
    levels: Vec<Level>, // face * nlevels + level
    nlevels: u32,
    gl_tex: u32,
    dirty: bool,
    /// Surfaces déjà remises à l'application (face, niveau) → objet.
    surfaces: Vec<((u32, u32), *mut Obj)>,
}

struct VolData {
    format: u32,
    width: u32,
    height: u32,
    depth: u32,
    levels: Vec<Vec<u8>>,
}

struct BufData {
    data: Vec<u8>,
    usage: u32,
    pool: u32,
    fvf_or_format: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct VertexElement {
    pub stream: u16,
    pub offset: u16,
    pub ty: u8,
    pub method: u8,
    pub usage: u8,
    pub usage_index: u8,
}

struct ShaderData {
    tokens: Vec<u32>,
    translated: Option<Sm9Shader>,
    gl_shader: u32,
}

// ── Vtables ────────────────────────────────────────────────────────────────

unsafe extern "win64" fn stub_invalid() -> i32 {
    D3DERR_INVALIDCALL
}
unsafe extern "win64" fn stub_ok() -> i32 {
    S_OK
}
unsafe extern "win64" fn stub_zero() -> u32 {
    0
}

macro_rules! f {
    ($f:expr) => {
        $f as *const () as usize
    };
}

/// Construit une vtable de `n` entrées ; les cases non fournies renvoient
/// `default` (sûr en x64 : l'appelant nettoie la pile).
fn build(n: usize, default: usize, entries: &[(usize, usize)]) -> &'static [usize] {
    let mut v = vec![default; n];
    for &(i, p) in entries {
        v[i] = p;
    }
    Box::leak(v.into_boxed_slice())
}

unsafe extern "win64" fn obj_addref(this: *mut Obj) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "win64" fn obj_release(this: *mut Obj) -> u32 {
    let prev = (*this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev != 1 {
        return prev.saturating_sub(1);
    }
    destroy(this);
    0
}

unsafe fn destroy(this: *mut Obj) {
    let obj = Box::from_raw(this);
    match &obj.kind {
        Kind::Surface(s) => {
            let s = lock(s);
            if let Some((parent, _, _)) = s.parent {
                if let Kind::Texture(t) | Kind::Cube(t) = &(*parent).kind {
                    lock(t).surfaces.retain(|(_, o)| *o != this);
                }
                obj_release(parent);
            }
            free_gl(s.gl_tex, s.gl_rb);
        }
        Kind::Texture(t) | Kind::Cube(t) => free_gl(lock(t).gl_tex, 0),
        Kind::Device(d) => {
            let mut d = lock(d);
            d.release_bindings();
            for p in [d.backbuffer, d.auto_depth] {
                if !p.is_null() {
                    obj_release(p);
                }
            }
            if let Some(g) = d.gl.as_ref() {
                if let Some(gf) = gl::gl() {
                    g.make_current();
                    (gf.DeleteFramebuffers)(1, &g.fbo);
                    for (_, p) in d.programs.drain() {
                        (gf.DeleteProgram)(p);
                    }
                }
            }
        }
        Kind::VertexShader(s) | Kind::PixelShader(s) => {
            if s.gl_shader != 0 {
                if let Some(gf) = gl::gl() {
                    (gf.DeleteShader)(s.gl_shader);
                }
            }
        }
        _ => {}
    }
    if !obj.device.is_null() {
        obj_release(obj.device);
    }
}

unsafe fn free_gl(tex: u32, rb: u32) {
    let Some(g) = gl::gl() else { return };
    if tex != 0 {
        (g.DeleteTextures)(1, &tex);
    }
    if rb != 0 {
        (g.DeleteRenderbuffers)(1, &rb);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// IID → l'objet l'implémente-t-il ? On compare `data1`, unique entre les
/// interfaces Direct3D 9.
fn supports(kind: &Kind, iid: &Guid) -> bool {
    const IUNKNOWN: u32 = 0;
    const RESOURCE: u32 = 0x05eec05d;
    const BASE_TEX: u32 = 0x580ca87e;
    let d1 = iid.data1;
    if d1 == IUNKNOWN && iid.data4 == [0xc0, 0, 0, 0, 0, 0, 0, 0x46] {
        return true;
    }
    match kind {
        Kind::D3D9 { ex } => d1 == 0x81bdcbca || (*ex && d1 == 0x02177241),
        Kind::Device(_) => d1 == 0xd0223b96 || d1 == 0xb18b10ce,
        Kind::Surface(_) => d1 == 0x0cfbaf3a || d1 == RESOURCE,
        Kind::Texture(_) => d1 == 0x85c31227 || d1 == BASE_TEX || d1 == RESOURCE,
        Kind::Cube(_) => d1 == 0xfff32f81 || d1 == BASE_TEX || d1 == RESOURCE,
        Kind::Volume(_) => d1 == 0x2518526c || d1 == BASE_TEX || d1 == RESOURCE,
        Kind::VertexBuffer(_) => d1 == 0xb64bb1b5 || d1 == RESOURCE,
        Kind::IndexBuffer(_) => d1 == 0x7c9dd65e || d1 == RESOURCE,
        Kind::Decl(_) => d1 == 0xdd13c59c,
        Kind::VertexShader(_) => d1 == 0xefc5557e,
        Kind::PixelShader(_) => d1 == 0x6d3bdbdc,
        Kind::Query(_) => d1 == 0xd9771460,
        Kind::StateBlock(_) => d1 == 0xb07c4fe5,
        Kind::SwapChain => d1 == 0x794950f2 || d1 == 0x91886caf,
    }
}

unsafe extern "win64" fn obj_qi(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_NOINTERFACE;
    }
    if supports(&(*this).kind, &*iid) {
        obj_addref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "win64" fn obj_get_device(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    if out.is_null() || (*this).device.is_null() {
        return D3DERR_INVALIDCALL;
    }
    obj_addref((*this).device);
    *out = (*this).device;
    S_OK
}

fn new_obj(vtbl: &'static [usize], device: *mut Obj, kind: Kind) -> *mut Obj {
    if !device.is_null() {
        unsafe {
            obj_addref(device);
        }
    }
    Box::into_raw(Box::new(Obj {
        vtbl: vtbl.as_ptr(),
        refs: AtomicU32::new(1),
        device,
        kind,
    }))
}

// ── IDirect3D9 ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DisplayMode {
    width: u32,
    height: u32,
    refresh: u32,
    format: u32,
}

const MODES: [(u32, u32); 8] = [
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 1024),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
];

fn desktop_mode() -> DisplayMode {
    let (w, h) = crate::dlls::gui::screen_size();
    DisplayMode {
        width: w.max(640) as u32,
        height: h.max(480) as u32,
        refresh: 60,
        format: D3DFMT_X8R8G8B8,
    }
}

fn modes() -> Vec<DisplayMode> {
    let d = desktop_mode();
    let mut v: Vec<DisplayMode> = MODES
        .iter()
        .filter(|&&(w, h)| w <= d.width && h <= d.height)
        .map(|&(width, height)| DisplayMode {
            width,
            height,
            refresh: 60,
            format: D3DFMT_X8R8G8B8,
        })
        .collect();
    if !v.iter().any(|m| m.width == d.width && m.height == d.height) {
        v.push(d);
    }
    v
}

unsafe extern "win64" fn d3d_adapter_count(_: *mut Obj) -> u32 {
    1
}

unsafe extern "win64" fn d3d_adapter_identifier(_: *mut Obj, adapter: u32, _flags: u32, out: *mut u8) -> i32 {
    if adapter != 0 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    std::ptr::write_bytes(out, 0, 1100);
    let put = |off: usize, s: &str, max: usize| {
        let b = s.as_bytes();
        let n = b.len().min(max - 1);
        std::ptr::copy_nonoverlapping(b.as_ptr(), out.add(off), n);
    };
    let caps = crate::dlls::gpu_caps::caps();
    let name = caps
        .vulkan
        .as_ref()
        .and_then(|v| v.devices.iter().find(|d| d.is_hardware()).or(v.devices.first()))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Winlux OpenGL".into());
    let vendor = caps
        .vulkan
        .as_ref()
        .and_then(|v| v.devices.first())
        .map(|d| d.vendor_id)
        .unwrap_or(0);
    put(0, "winlux_d3d9.dll", 512);
    put(512, &name, 512);
    put(1024, "\\\\.\\DISPLAY1", 32);
    // DriverVersion (LARGE_INTEGER) : 9.18.13.0
    let ver: u64 = (9u64 << 48) | (18u64 << 32) | (13u64 << 16);
    std::ptr::write_unaligned(out.add(1056) as *mut u64, ver);
    std::ptr::write_unaligned(out.add(1064) as *mut u32, vendor);
    std::ptr::write_unaligned(out.add(1068) as *mut u32, 0x1);
    // DeviceIdentifier (GUID) : fixe, propre à Winlux.
    let guid = *b"WinluxD3D9Adaptr";
    std::ptr::copy_nonoverlapping(guid.as_ptr(), out.add(1080), 16);
    std::ptr::write_unaligned(out.add(1096) as *mut u32, 1);
    S_OK
}

unsafe extern "win64" fn d3d_mode_count(_: *mut Obj, adapter: u32, format: u32) -> u32 {
    if adapter != 0 || !matches!(format, D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8 | D3DFMT_R5G6B5) {
        return 0;
    }
    modes().len() as u32
}

unsafe extern "win64" fn d3d_enum_modes(_: *mut Obj, adapter: u32, format: u32, i: u32, out: *mut DisplayMode) -> i32 {
    if adapter != 0 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    match modes().get(i as usize) {
        Some(m) => {
            *out = DisplayMode { format, ..*m };
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}

unsafe extern "win64" fn d3d_display_mode(_: *mut Obj, adapter: u32, out: *mut DisplayMode) -> i32 {
    if adapter != 0 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = desktop_mode();
    S_OK
}

unsafe extern "win64" fn d3d_check_type(_: *mut Obj, adapter: u32, _ty: u32, _af: u32, bf: u32, _w: i32) -> i32 {
    if adapter != 0 {
        return D3DERR_INVALIDCALL;
    }
    if matches!(bf, D3DFMT_UNKNOWN | D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8 | D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5) {
        S_OK
    } else {
        D3DERR_NOTAVAILABLE
    }
}

unsafe extern "win64" fn d3d_check_format(_: *mut Obj, adapter: u32, _ty: u32, _af: u32, usage: u32, rtype: u32, f: u32) -> i32 {
    if adapter != 0 {
        return D3DERR_INVALIDCALL;
    }
    let Some(fi) = format_info(f) else {
        return D3DERR_NOTAVAILABLE;
    };
    if usage & D3DUSAGE_DEPTHSTENCIL != 0 && !is_depth(f) {
        return D3DERR_NOTAVAILABLE;
    }
    if usage & D3DUSAGE_RENDERTARGET != 0 && (fi.block != 0 || is_depth(f) || is_video(f)) {
        return D3DERR_NOTAVAILABLE;
    }
    if rtype == D3DRTYPE_VOLUMETEXTURE && (is_depth(f) || is_video(f)) {
        return D3DERR_NOTAVAILABLE;
    }
    S_OK
}

unsafe extern "win64" fn d3d_check_ms(_: *mut Obj, _a: u32, _t: u32, _f: u32, _w: i32, ms: u32, q: *mut u32) -> i32 {
    if !q.is_null() {
        *q = 1;
    }
    if ms == 0 {
        S_OK
    } else {
        D3DERR_NOTAVAILABLE
    }
}

pub(crate) fn fill_caps(out: *mut u32) {
    let mut c = [0u32; 76];
    c[0] = 1; // D3DDEVTYPE_HAL
    c[2] = 0x20000; // Caps : READ_SCANLINE
    c[3] = 0x6002_0000; // Caps2 : CANAUTOGENMIPMAP | DYNAMICTEXTURES | FULLSCREENGAMMA
    c[4] = 0x320; // Caps3
    c[5] = 0x8000_0001; // PresentationIntervals
    c[6] = 1; // CursorCaps
    c[7] = 0x10000 | 0x8000 | 0x80000 | 0x100000 | 0x200 | 0x400 | 0x800 | 0x10 | 0x40; // DevCaps
    c[8] = 0x10 | 0x20 | 0x40 | 0x80 | 0x800 | 0x20000 | 0x2; // PrimitiveMiscCaps
    c[9] = 0x10 | 0x80 | 0x100 | 0x2000 | 0x100_0000 | 0x200_0000 | 0x400_0000; // RasterCaps
    c[10] = 0xff; // ZCmpCaps
    c[11] = 0x3fff; // SrcBlendCaps
    c[12] = 0x3fff; // DestBlendCaps
    c[13] = 0xff; // AlphaCmpCaps
    c[14] = 0x8 | 0x200 | 0x4000 | 0x80000; // ShadeCaps
    c[15] = 0x1 | 0x4 | 0x400 | 0x800 | 0x2000 | 0x4000 | 0x10000; // TextureCaps
    let filt = 0x100 | 0x200 | 0x400 | 0x10000 | 0x20000 | 0x100_0000 | 0x200_0000;
    c[16] = filt;
    c[17] = filt;
    c[18] = filt;
    c[19] = 0x3f; // TextureAddressCaps
    c[20] = 0x3f;
    c[21] = 0x1f; // LineCaps
    c[22] = 8192;
    c[23] = 8192;
    c[24] = 2048;
    c[25] = 8192;
    c[26] = 8192;
    c[27] = 16;
    c[28] = 1e10f32.to_bits(); // MaxVertexW
    c[29] = (-32768.0f32).to_bits();
    c[30] = (-32768.0f32).to_bits();
    c[31] = 32768.0f32.to_bits();
    c[32] = 32768.0f32.to_bits();
    c[33] = 0; // ExtentsAdjust
    c[34] = 0x1ff; // StencilCaps
    c[35] = 8 | 0x80000 | 0x100000; // FVFCaps
    c[36] = 0x3ff_ffff; // TextureOpCaps
    c[37] = 8; // MaxTextureBlendStages
    c[38] = 8; // MaxSimultaneousTextures
    c[39] = 0x1 | 0x2 | 0x8 | 0x10 | 0x20; // VertexProcessingCaps
    c[40] = 8; // MaxActiveLights
    c[41] = 0; // MaxUserClipPlanes
    c[42] = 0;
    c[43] = 0;
    c[44] = 64.0f32.to_bits(); // MaxPointSize
    c[45] = 0xfffff;
    c[46] = 0xff_ffff;
    c[47] = 16; // MaxStreams
    c[48] = 255;
    c[49] = 0xfffe_0300; // VertexShaderVersion
    c[50] = 256;
    c[51] = 0xffff_0300; // PixelShaderVersion
    c[52] = 8.0f32.to_bits();
    c[53] = 0x1 | 0x40; // DevCaps2
    c[57] = 0;
    c[58] = 0;
    c[59] = 1;
    c[60] = 0x33f; // DeclTypes
    c[61] = 4; // NumSimultaneousRTs
    c[62] = 0x100 | 0x200 | 0x100_0000 | 0x200_0000;
    c[63] = 0; // VS20Caps.Caps (pas de prédicats)
    c[64] = 24;
    c[65] = 32;
    c[66] = 4;
    c[67] = 0x1 | 0x2 | 0x8 | 0x10; // PS20Caps.Caps
    c[68] = 24;
    c[69] = 32;
    c[70] = 4;
    c[71] = 512;
    c[72] = 0x100 | 0x200 | 0x100_0000 | 0x200_0000; // VertexTextureFilterCaps
    c[73] = 65535;
    c[74] = 65535;
    c[75] = 32768; // MaxVertexShader30InstructionSlots
    unsafe {
        std::ptr::copy_nonoverlapping(c.as_ptr(), out, 76);
        // MaxPixelShader30InstructionSlots est le 77e mot (structure de 304 octets).
        *out.add(75) = 32768;
    }
}

unsafe extern "win64" fn d3d_caps(_: *mut Obj, adapter: u32, _ty: u32, out: *mut u32) -> i32 {
    if adapter != 0 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    fill_caps(out);
    S_OK
}

unsafe extern "win64" fn d3d_monitor(_: *mut Obj, _adapter: u32) -> usize {
    0x0001_0001
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct PresentParams {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub count: u32,
    pub ms_type: u32,
    pub ms_quality: u32,
    pub swap_effect: u32,
    pub device_window: usize,
    pub windowed: i32,
    pub auto_depth: i32,
    pub auto_depth_format: u32,
    pub flags: u32,
    pub refresh: u32,
    pub interval: u32,
}

unsafe extern "win64" fn d3d_create_device(
    this: *mut Obj,
    adapter: u32,
    _ty: u32,
    focus: usize,
    _behavior: u32,
    pp: *mut PresentParams,
    out: *mut *mut Obj,
) -> i32 {
    if out.is_null() || pp.is_null() || adapter != 0 {
        return D3DERR_INVALIDCALL;
    }
    *out = std::ptr::null_mut();
    let ex = matches!((*this).kind, Kind::D3D9 { ex: true });
    match create_device(this, focus, &mut *pp, ex) {
        Some(d) => {
            *out = d;
            S_OK
        }
        None => D3DERR_NOTAVAILABLE,
    }
}

unsafe extern "win64" fn d3d_create_device_ex(
    this: *mut Obj,
    adapter: u32,
    ty: u32,
    focus: usize,
    behavior: u32,
    pp: *mut PresentParams,
    _mode: *mut c_void,
    out: *mut *mut Obj,
) -> i32 {
    d3d_create_device(this, adapter, ty, focus, behavior, pp, out)
}

unsafe extern "win64" fn d3d_mode_count_ex(this: *mut Obj, adapter: u32, _filter: *const c_void) -> u32 {
    d3d_mode_count(this, adapter, D3DFMT_X8R8G8B8)
}

unsafe extern "win64" fn d3d_luid(_: *mut Obj, _adapter: u32, out: *mut u64) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = 0x5749_4e4c;
    S_OK
}

fn d3d9_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            22,
            f!(stub_invalid),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(stub_ok)),
                (4, f!(d3d_adapter_count)),
                (5, f!(d3d_adapter_identifier)),
                (6, f!(d3d_mode_count)),
                (7, f!(d3d_enum_modes)),
                (8, f!(d3d_display_mode)),
                (9, f!(d3d_check_type)),
                (10, f!(d3d_check_format)),
                (11, f!(d3d_check_ms)),
                (12, f!(stub_ok)),
                (13, f!(stub_ok)),
                (14, f!(d3d_caps)),
                (15, f!(d3d_monitor)),
                (16, f!(d3d_create_device)),
                (17, f!(d3d_mode_count_ex)),
                (18, f!(stub_invalid)),
                (19, f!(stub_invalid)),
                (20, f!(d3d_create_device_ex)),
                (21, f!(d3d_luid)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 22) }
}

// ── État de l'appareil ─────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub min_z: f32,
    pub max_z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Material {
    diffuse: [f32; 4],
    ambient: [f32; 4],
    specular: [f32; 4],
    emissive: [f32; 4],
    power: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Light {
    ty: u32,
    diffuse: [f32; 4],
    specular: [f32; 4],
    ambient: [f32; 4],
    position: [f32; 3],
    direction: [f32; 3],
    range: f32,
    falloff: f32,
    att0: f32,
    att1: f32,
    att2: f32,
    theta: f32,
    phi: f32,
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

const TS_VIEW: u32 = 2;
const TS_PROJECTION: u32 = 3;
const TS_TEXTURE0: u32 = 16;
const TS_WORLD: u32 = 256;

const MAX_SAMPLERS: usize = 20; // 16 pixel + 4 vertex (D3DVERTEXTEXTURESAMPLER0 = 257)

/// État « réglable » : celui que capturent les state blocks.
#[derive(Clone)]
pub(crate) struct Settable {
    rs: Box<[u32; 256]>,
    transforms: HashMap<u32, [f32; 16]>,
    viewport: Viewport,
    material: Material,
    lights: HashMap<u32, (Light, bool)>,
    textures: [*mut Obj; MAX_SAMPLERS],
    tss: [[u32; 33]; 8],
    samp: [[u32; 14]; MAX_SAMPLERS],
    streams: [(*mut Obj, u32, u32, u32); 16],
    indices: *mut Obj,
    decl: *mut Obj,
    fvf: u32,
    vs: *mut Obj,
    ps: *mut Obj,
    vs_f: Box<[[f32; 4]; 256]>,
    vs_i: [[i32; 4]; 16],
    vs_b: [i32; 16],
    ps_f: Box<[[f32; 4]; 224]>,
    ps_i: [[i32; 4]; 16],
    ps_b: [i32; 16],
    scissor: [i32; 4],
}

unsafe impl Send for Settable {}

fn rs_defaults(auto_depth: bool) -> Box<[u32; 256]> {
    let mut rs = Box::new([0u32; 256]);
    let set = |rs: &mut [u32; 256], k: usize, v: u32| rs[k] = v;
    set(&mut rs, 7, auto_depth as u32); // ZENABLE
    set(&mut rs, 8, 3); // FILLMODE solid
    set(&mut rs, 9, 2); // SHADEMODE gouraud
    set(&mut rs, 14, 1); // ZWRITEENABLE
    set(&mut rs, 16, 1); // LASTPIXEL
    set(&mut rs, 19, 2); // SRCBLEND one
    set(&mut rs, 20, 1); // DESTBLEND zero
    set(&mut rs, 22, 3); // CULLMODE ccw
    set(&mut rs, 23, 4); // ZFUNC lessequal
    set(&mut rs, 25, 8); // ALPHAFUNC always
    set(&mut rs, 37, 1.0f32.to_bits()); // FOGEND
    set(&mut rs, 38, 1.0f32.to_bits()); // FOGDENSITY
    set(&mut rs, 53, 1);
    set(&mut rs, 54, 1);
    set(&mut rs, 55, 1);
    set(&mut rs, 56, 8);
    set(&mut rs, 58, u32::MAX);
    set(&mut rs, 59, u32::MAX);
    set(&mut rs, 60, u32::MAX); // TEXTUREFACTOR
    set(&mut rs, 136, 1); // CLIPPING
    set(&mut rs, 137, 1); // LIGHTING
    set(&mut rs, 141, 1); // COLORVERTEX
    set(&mut rs, 142, 1); // LOCALVIEWER
    set(&mut rs, 145, 1); // DIFFUSEMATERIALSOURCE color1
    set(&mut rs, 146, 2); // SPECULARMATERIALSOURCE color2
    set(&mut rs, 154, 1.0f32.to_bits()); // POINTSIZE
    set(&mut rs, 155, 1.0f32.to_bits());
    set(&mut rs, 161, 1);
    set(&mut rs, 162, u32::MAX);
    set(&mut rs, 166, 64.0f32.to_bits());
    set(&mut rs, 168, 0xf); // COLORWRITEENABLE
    set(&mut rs, 171, 1); // BLENDOP add
    set(&mut rs, 190, 0xf);
    set(&mut rs, 191, 0xf);
    set(&mut rs, 192, 0xf);
    set(&mut rs, 207, 2);
    set(&mut rs, 208, 1);
    set(&mut rs, 209, 1);
    rs
}

impl Settable {
    fn new(auto_depth: bool, w: u32, h: u32) -> Self {
        let mut tss = [[0u32; 33]; 8];
        for (i, s) in tss.iter_mut().enumerate() {
            s[1] = if i == 0 { 4 } else { 1 }; // COLOROP : MODULATE / DISABLE
            s[2] = 2; // COLORARG1 texture
            s[3] = 1; // COLORARG2 current
            s[4] = if i == 0 { 2 } else { 1 }; // ALPHAOP : SELECTARG1 / DISABLE
            s[5] = 2;
            s[6] = 1;
            s[11] = i as u32; // TEXCOORDINDEX
        }
        let mut samp = [[0u32; 14]; MAX_SAMPLERS];
        for s in samp.iter_mut() {
            s[1] = 1; // ADDRESSU wrap
            s[2] = 1;
            s[3] = 1;
            s[5] = 1; // MAGFILTER point
            s[6] = 1; // MINFILTER point
            s[7] = 0; // MIPFILTER none
            s[10] = 1;
        }
        Settable {
            rs: rs_defaults(auto_depth),
            transforms: HashMap::new(),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: w,
                height: h,
                min_z: 0.0,
                max_z: 1.0,
            },
            material: Material::default(),
            lights: HashMap::new(),
            textures: [std::ptr::null_mut(); MAX_SAMPLERS],
            tss,
            samp,
            streams: [(std::ptr::null_mut(), 0, 0, 1); 16],
            indices: std::ptr::null_mut(),
            decl: std::ptr::null_mut(),
            fvf: 0,
            vs: std::ptr::null_mut(),
            ps: std::ptr::null_mut(),
            vs_f: Box::new([[0.0; 4]; 256]),
            vs_i: [[0; 4]; 16],
            vs_b: [0; 16],
            ps_f: Box::new([[0.0; 4]; 224]),
            ps_i: [[0; 4]; 16],
            ps_b: [0; 16],
            scissor: [0, 0, w as i32, h as i32],
        }
    }

    fn transform(&self, t: u32) -> [f32; 16] {
        self.transforms.get(&t).copied().unwrap_or(IDENTITY)
    }

    /// Tous les objets référencés (pour gérer les compteurs).
    fn bound_objects(&self) -> Vec<*mut Obj> {
        let mut v: Vec<*mut Obj> = self.textures.to_vec();
        v.extend(self.streams.iter().map(|s| s.0));
        v.extend([self.indices, self.decl, self.vs, self.ps]);
        v.retain(|p| !p.is_null());
        v
    }
}

struct GlState {
    ctx: GlContext,
    fbo: u32,
    owner: Mutex<Option<std::thread::ThreadId>>,
    legacy: &'static GlLegacy,
}

impl GlState {
    fn make_current(&self) {
        let me = std::thread::current().id();
        let mut owner = lock(&self.owner);
        if *owner == Some(me) {
            return;
        }
        if let Some(glx) = gl::glx() {
            unsafe {
                (glx.MakeCurrent)(self.ctx.display, self.ctx.drawable, self.ctx.context);
            }
        }
        *owner = Some(me);
    }
}

pub(crate) struct Dev {
    gl: Option<GlState>,
    pp: PresentParams,
    window: usize,
    backbuffer: *mut Obj,
    auto_depth: *mut Obj,
    rts: [*mut Obj; 4],
    ds: *mut Obj,
    s: Settable,
    recording: bool,
    programs: HashMap<(usize, usize), u32>,
    in_scene: bool,
    present_count: u32,
    swapchain: *mut Obj,
    gamma: Vec<u8>,
}

unsafe impl Send for Dev {}

impl Dev {
    unsafe fn release_bindings(&mut self) {
        for p in self.s.bound_objects() {
            obj_release(p);
        }
        for p in self.rts.iter().chain([&self.ds]) {
            if !p.is_null() {
                obj_release(*p);
            }
        }
        self.rts = [std::ptr::null_mut(); 4];
        self.ds = std::ptr::null_mut();
        if !self.swapchain.is_null() {
            // La swapchain implicite ne possède pas de référence propre.
            let _ = Box::from_raw(self.swapchain);
            self.swapchain = std::ptr::null_mut();
        }
    }
}

fn dev_of(this: *mut Obj) -> Option<&'static Mutex<Dev>> {
    unsafe {
        match &(*this).kind {
            Kind::Device(d) => Some(&**(d as *const Box<Mutex<Dev>>)),
            _ => None,
        }
    }
}

/// Remplace une liaison en gérant les références (AddRef nouveau, Release ancien).
unsafe fn rebind(slot: &mut *mut Obj, new: *mut Obj) {
    if *slot == new {
        return;
    }
    if !new.is_null() {
        obj_addref(new);
    }
    if !slot.is_null() {
        obj_release(*slot);
    }
    *slot = new;
}

// ── Création de l'appareil ─────────────────────────────────────────────────

unsafe fn window_size(hwnd: usize) -> (u32, u32) {
    crate::dlls::gui::window_rect(hwnd)
        .map(|r| (r.width().max(1) as u32, r.height().max(1) as u32))
        .unwrap_or((800, 600))
}

unsafe fn create_device(d3d: *mut Obj, focus: usize, pp: &mut PresentParams, ex: bool) -> Option<*mut Obj> {
    let window = if pp.device_window != 0 { pp.device_window } else { focus };
    let (ww, wh) = window_size(window);
    if pp.width == 0 {
        pp.width = ww;
    }
    if pp.height == 0 {
        pp.height = wh;
    }
    if pp.format == D3DFMT_UNKNOWN {
        pp.format = D3DFMT_X8R8G8B8;
    }
    pp.count = pp.count.max(1);
    let gl_state = create_gl(window, pp.width, pp.height);
    if gl_state.is_none() {
        log_warn!(
            "d3d9",
            "pas de contexte OpenGL pour la fenêtre 0x{:x} : appareil sans rendu (aucun crash, rien ne s'affiche)",
            window
        );
    }
    let dev = Dev {
        gl: gl_state,
        pp: *pp,
        window,
        backbuffer: std::ptr::null_mut(),
        auto_depth: std::ptr::null_mut(),
        rts: [std::ptr::null_mut(); 4],
        ds: std::ptr::null_mut(),
        s: Settable::new(pp.auto_depth != 0, pp.width, pp.height),
        recording: false,
        programs: HashMap::new(),
        in_scene: false,
        present_count: 0,
        swapchain: std::ptr::null_mut(),
        gamma: Vec::new(),
    };
    let device = Box::into_raw(Box::new(Obj {
        vtbl: device_vtable().as_ptr(),
        refs: AtomicU32::new(1),
        device: std::ptr::null_mut(),
        kind: Kind::Device(Box::new(Mutex::new(dev))),
    }));
    let m = dev_of(device)?;
    {
        let mut d = lock(m);
        // Les surfaces implicites ne retiennent pas l'appareil (sinon cycle).
        let bb = make_surface(&d, std::ptr::null_mut(), pp.width, pp.height, pp.format, D3DUSAGE_RENDERTARGET, D3DPOOL_DEFAULT);
        d.backbuffer = bb;
        obj_addref(bb);
        d.rts[0] = bb;
        if pp.auto_depth != 0 {
            let fmt = if format_info(pp.auto_depth_format).is_some() { pp.auto_depth_format } else { D3DFMT_D24S8 };
            let ds = make_surface(&d, std::ptr::null_mut(), pp.width, pp.height, fmt, D3DUSAGE_DEPTHSTENCIL, D3DPOOL_DEFAULT);
            d.auto_depth = ds;
            obj_addref(ds);
            d.ds = ds;
        }
        d.swapchain = Box::into_raw(Box::new(Obj {
            vtbl: swapchain_vtable().as_ptr(),
            refs: AtomicU32::new(1),
            device,
            kind: Kind::SwapChain,
        }));
    }
    let _ = (d3d, ex);
    log_info!(
        "d3d9",
        "appareil Direct3D 9 {}x{} créé ({})",
        pp.width,
        pp.height,
        if lock(m).gl.is_some() { "OpenGL" } else { "sans rendu" }
    );
    Some(device)
}

fn create_gl(window: usize, w: u32, h: u32) -> Option<GlState> {
    if window == 0 {
        return None;
    }
    // Garde-fou : sans fenêtre X réelle, on ne tente pas GLX (en headless,
    // dessiner sur la racine reviendrait à peindre le bureau).
    crate::dlls::gui::native_window(window)?;
    let legacy = glc::legacy()?;
    let mut ctx = GlContext::create_for_window(window)?;
    ctx.resize(w, h);
    let g = gl::gl()?;
    let mut fbo = 0;
    unsafe {
        (g.GenFramebuffers)(1, &mut fbo);
    }
    Some(GlState {
        ctx,
        fbo,
        owner: Mutex::new(Some(std::thread::current().id())),
        legacy,
    })
}

/// Crée une surface autonome (cible, profondeur, hors écran).
unsafe fn make_surface(d: &Dev, device: *mut Obj, w: u32, h: u32, format: u32, usage: u32, pool: u32) -> *mut Obj {
    let (pitch, size) = level_layout(format, w, h);
    let mut s = SurfaceData {
        width: w,
        height: h,
        format,
        usage,
        pool,
        parent: None,
        data: vec![0; size],
        pitch,
        gl_tex: 0,
        gl_rb: 0,
        cpu_dirty: false,
        gpu_dirty: false,
    };
    if let (Some(gs), Some(g), Some(fi)) = (d.gl.as_ref(), gl::gl(), format_info(format)) {
        gs.make_current();
        if usage & D3DUSAGE_DEPTHSTENCIL != 0 || is_depth(format) {
            (g.GenRenderbuffers)(1, &mut s.gl_rb);
            (g.BindRenderbuffer)(gl::GL_RENDERBUFFER, s.gl_rb);
            (g.RenderbufferStorage)(gl::GL_RENDERBUFFER, gl::GL_DEPTH24_STENCIL8, w as i32, h as i32);
        } else if usage & D3DUSAGE_RENDERTARGET != 0 && fi.internal != 0 {
            (g.GenTextures)(1, &mut s.gl_tex);
            (g.BindTexture)(gl::GL_TEXTURE_2D, s.gl_tex);
            (g.TexImage2D)(gl::GL_TEXTURE_2D, 0, fi.internal as i32, w as i32, h as i32, 0, fi.format, fi.ty, std::ptr::null());
            (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MIN_FILTER, gl::GL_LINEAR);
            (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MAG_FILTER, gl::GL_LINEAR);
        }
    }
    new_obj(surface_vtable(), device, Kind::Surface(Mutex::new(s)))
}

// ── Surfaces ───────────────────────────────────────────────────────────────

#[repr(C)]
struct LockedRect {
    pitch: i32,
    bits: *mut u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect32 {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct SurfaceDesc {
    format: u32,
    ty: u32,
    usage: u32,
    pool: u32,
    ms_type: u32,
    ms_quality: u32,
    width: u32,
    height: u32,
}

/// Adresse du pixel (x, y) dans un tampon de format `f`.
fn texel_offset(f: u32, pitch: u32, x: i32, y: i32) -> usize {
    let fi = format_info(f);
    match fi {
        Some(fi) if fi.block != 0 => (y.max(0) as usize / 4) * pitch as usize + (x.max(0) as usize / 4) * fi.block as usize,
        Some(fi) => y.max(0) as usize * pitch as usize + x.max(0) as usize * fi.bpp.max(1) as usize,
        None => y.max(0) as usize * pitch as usize + x.max(0) as usize * 4,
    }
}

unsafe fn surface_lock_impl(this: *mut Obj, out: *mut LockedRect, rect: *const Rect32) -> i32 {
    let Kind::Surface(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut s = lock(m);
    if let Some((parent, face, level)) = s.parent {
        drop(s);
        return tex_lock(parent, face, level, out, rect);
    }
    // Relecture d'une cible de rendu : le contenu GPU est plus récent.
    if s.gl_tex != 0 && !(*this).device.is_null() {
        if let Some(dm) = dev_of((*this).device) {
            let d = lock(dm);
            read_back(&d, &mut s);
        }
    }
    let (x, y) = if rect.is_null() { (0, 0) } else { ((*rect).left, (*rect).top) };
    let off = texel_offset(s.format, s.pitch, x, y).min(s.data.len());
    (*out).pitch = s.pitch as i32;
    (*out).bits = s.data.as_mut_ptr().add(off);
    s.cpu_dirty = true;
    S_OK
}

unsafe extern "win64" fn surface_lock(this: *mut Obj, out: *mut LockedRect, rect: *const Rect32, _flags: u32) -> i32 {
    surface_lock_impl(this, out, rect)
}

unsafe extern "win64" fn surface_unlock(this: *mut Obj) -> i32 {
    let Kind::Surface(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    let mut s = lock(m);
    if let Some((parent, face, level)) = s.parent {
        drop(s);
        return tex_unlock(parent, face, level);
    }
    if s.cpu_dirty && s.gl_tex != 0 && !(*this).device.is_null() {
        if let (Some(dm), Some(g), Some(fi)) = (dev_of((*this).device), gl::gl(), format_info(s.format)) {
            let d = lock(dm);
            if let Some(gs) = d.gl.as_ref() {
                gs.make_current();
                (g.BindTexture)(gl::GL_TEXTURE_2D, s.gl_tex);
                (g.PixelStorei)(gl::GL_UNPACK_ALIGNMENT, 4);
                (g.TexSubImage2D)(gl::GL_TEXTURE_2D, 0, 0, 0, s.width as i32, s.height as i32, fi.format, fi.ty, s.data.as_ptr() as *const c_void);
            }
        }
    }
    s.cpu_dirty = false;
    S_OK
}

/// Copie GPU → CPU d'une surface cible.
unsafe fn read_back(d: &Dev, s: &mut SurfaceData) {
    let (Some(gs), Some(g), Some(fi)) = (d.gl.as_ref(), gl::gl(), format_info(s.format)) else {
        return;
    };
    if s.gl_tex == 0 || fi.internal == 0 {
        return;
    }
    gs.make_current();
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, gs.fbo);
    (g.FramebufferTexture2D)(gl::GL_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0, gl::GL_TEXTURE_2D, s.gl_tex, 0);
    (g.PixelStorei)(gl::GL_PACK_ALIGNMENT, 4);
    (g.ReadPixels)(0, 0, s.width as i32, s.height as i32, fi.format, fi.ty, s.data.as_mut_ptr() as *mut c_void);
}

unsafe extern "win64" fn surface_desc(this: *mut Obj, out: *mut SurfaceDesc) -> i32 {
    let Kind::Surface(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let s = lock(m);
    *out = SurfaceDesc {
        format: s.format,
        ty: D3DRTYPE_SURFACE,
        usage: s.usage,
        pool: s.pool,
        ms_type: 0,
        ms_quality: 0,
        width: s.width,
        height: s.height,
    };
    S_OK
}

unsafe extern "win64" fn surface_container(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    let Kind::Surface(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    let parent = lock(m).parent.map(|p| p.0);
    match parent {
        Some(p) => obj_qi(p, iid, out),
        None if !(*this).device.is_null() => obj_qi((*this).device, iid, out),
        None => E_NOINTERFACE,
    }
}

unsafe extern "win64" fn surface_type(_: *mut Obj) -> u32 {
    D3DRTYPE_SURFACE
}

fn surface_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            17,
            f!(stub_invalid),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(obj_get_device)),
                (4, f!(stub_ok)),
                (5, f!(stub_invalid)),
                (6, f!(stub_ok)),
                (7, f!(stub_zero)),
                (8, f!(stub_zero)),
                (9, f!(stub_ok)),
                (10, f!(surface_type)),
                (11, f!(surface_container)),
                (12, f!(surface_desc)),
                (13, f!(surface_lock)),
                (14, f!(surface_unlock)),
                (15, f!(stub_invalid)),
                (16, f!(stub_invalid)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 17) }
}

// ── Textures ───────────────────────────────────────────────────────────────

unsafe fn tex_data(this: *mut Obj) -> Option<&'static Mutex<TexData>> {
    match &(*this).kind {
        Kind::Texture(t) | Kind::Cube(t) => Some(&*(t as *const Mutex<TexData>)),
        _ => None,
    }
}

unsafe fn tex_lock(this: *mut Obj, face: u32, level: u32, out: *mut LockedRect, rect: *const Rect32) -> i32 {
    let Some(m) = tex_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut t = lock(m);
    if level >= t.nlevels || face >= t.faces {
        return D3DERR_INVALIDCALL;
    }
    let idx = (face * t.nlevels + level) as usize;
    let fmt = t.format;
    let l = &mut t.levels[idx];
    let (x, y) = if rect.is_null() { (0, 0) } else { ((*rect).left, (*rect).top) };
    let off = texel_offset(fmt, l.pitch, x, y).min(l.data.len());
    (*out).pitch = l.pitch as i32;
    (*out).bits = l.data.as_mut_ptr().add(off);
    S_OK
}

unsafe fn tex_unlock(this: *mut Obj, _face: u32, level: u32) -> i32 {
    let Some(m) = tex_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    let mut t = lock(m);
    if level >= t.nlevels {
        return D3DERR_INVALIDCALL;
    }
    t.dirty = true;
    S_OK
}

unsafe extern "win64" fn tex_level_count(this: *mut Obj) -> u32 {
    tex_data(this).map(|m| lock(m).nlevels).unwrap_or(0)
}

unsafe extern "win64" fn tex_level_desc(this: *mut Obj, level: u32, out: *mut SurfaceDesc) -> i32 {
    let Some(m) = tex_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    let t = lock(m);
    if out.is_null() || level >= t.nlevels {
        return D3DERR_INVALIDCALL;
    }
    let l = &t.levels[level as usize];
    *out = SurfaceDesc {
        format: t.format,
        ty: D3DRTYPE_SURFACE,
        usage: t.usage,
        pool: t.pool,
        ms_type: 0,
        ms_quality: 0,
        width: l.width,
        height: l.height,
    };
    S_OK
}

unsafe fn tex_surface(this: *mut Obj, face: u32, level: u32, out: *mut *mut Obj) -> i32 {
    let Some(m) = tex_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut t = lock(m);
    if level >= t.nlevels || face >= t.faces {
        return D3DERR_INVALIDCALL;
    }
    if let Some((_, o)) = t.surfaces.iter().find(|(k, _)| *k == (face, level)) {
        obj_addref(*o);
        *out = *o;
        return S_OK;
    }
    let l = &t.levels[(face * t.nlevels + level) as usize];
    let s = SurfaceData {
        width: l.width,
        height: l.height,
        format: t.format,
        usage: t.usage,
        pool: t.pool,
        parent: Some((this, face, level)),
        data: Vec::new(),
        pitch: l.pitch,
        gl_tex: 0,
        gl_rb: 0,
        cpu_dirty: false,
        gpu_dirty: false,
    };
    obj_addref(this);
    let o = new_obj(surface_vtable(), (*this).device, Kind::Surface(Mutex::new(s)));
    t.surfaces.push(((face, level), o));
    *out = o;
    S_OK
}

unsafe extern "win64" fn tex_get_surface_level(this: *mut Obj, level: u32, out: *mut *mut Obj) -> i32 {
    tex_surface(this, 0, level, out)
}
unsafe extern "win64" fn tex_lock_rect(this: *mut Obj, level: u32, out: *mut LockedRect, rect: *const Rect32, _f: u32) -> i32 {
    tex_lock(this, 0, level, out, rect)
}
unsafe extern "win64" fn tex_unlock_rect(this: *mut Obj, level: u32) -> i32 {
    tex_unlock(this, 0, level)
}
unsafe extern "win64" fn cube_get_surface(this: *mut Obj, face: u32, level: u32, out: *mut *mut Obj) -> i32 {
    tex_surface(this, face, level, out)
}
unsafe extern "win64" fn cube_lock_rect(this: *mut Obj, face: u32, level: u32, out: *mut LockedRect, rect: *const Rect32, _f: u32) -> i32 {
    tex_lock(this, face, level, out, rect)
}
unsafe extern "win64" fn cube_unlock_rect(this: *mut Obj, face: u32, level: u32) -> i32 {
    tex_unlock(this, face, level)
}
unsafe extern "win64" fn tex_type(this: *mut Obj) -> u32 {
    match (*this).kind {
        Kind::Cube(_) => D3DRTYPE_CUBETEXTURE,
        Kind::Volume(_) => D3DRTYPE_VOLUMETEXTURE,
        _ => D3DRTYPE_TEXTURE,
    }
}
unsafe extern "win64" fn tex_generate_mips(this: *mut Obj) {
    if let Some(m) = tex_data(this) {
        lock(m).dirty = true;
    }
}

fn texture_vtable(cube: bool) -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    static C: OnceLock<usize> = OnceLock::new();
    let mk = |cube: bool| {
        let mut e = vec![
            (0, f!(obj_qi)),
            (1, f!(obj_addref)),
            (2, f!(obj_release)),
            (3, f!(obj_get_device)),
            (4, f!(stub_ok)),
            (5, f!(stub_invalid)),
            (6, f!(stub_ok)),
            (7, f!(stub_zero)),
            (8, f!(stub_zero)),
            (9, f!(stub_ok)),
            (10, f!(tex_type)),
            (11, f!(stub_zero)),
            (12, f!(stub_zero)),
            (13, f!(tex_level_count)),
            (14, f!(stub_ok)),
            (15, f!(stub_zero)),
            (16, f!(tex_generate_mips)),
            (17, f!(tex_level_desc)),
            (21, f!(stub_ok)),
        ];
        if cube {
            e.extend([(18, f!(cube_get_surface)), (19, f!(cube_lock_rect)), (20, f!(cube_unlock_rect))]);
        } else {
            e.extend([(18, f!(tex_get_surface_level)), (19, f!(tex_lock_rect)), (20, f!(tex_unlock_rect))]);
        }
        build(22, f!(stub_invalid), &e).as_ptr() as usize
    };
    let p = if cube { *C.get_or_init(|| mk(true)) } else { *V.get_or_init(|| mk(false)) };
    unsafe { std::slice::from_raw_parts(p as *const usize, 22) }
}

fn mip_count(w: u32, h: u32, levels: u32) -> u32 {
    let full = 32 - w.max(h).max(1).leading_zeros();
    if levels == 0 {
        full
    } else {
        levels.min(full)
    }
}

unsafe fn make_texture(device: *mut Obj, w: u32, h: u32, levels: u32, usage: u32, format: u32, pool: u32, faces: u32) -> Option<*mut Obj> {
    format_info(format)?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return None;
    }
    let n = if usage & D3DUSAGE_AUTOGENMIPMAP != 0 { 1 } else { mip_count(w, h, levels) };
    let mut lv = Vec::new();
    for _ in 0..faces {
        for l in 0..n {
            let (lw, lh) = ((w >> l).max(1), (h >> l).max(1));
            let (pitch, size) = level_layout(format, lw, lh);
            lv.push(Level {
                width: lw,
                height: lh,
                pitch,
                data: vec![0; size],
            });
        }
    }
    let t = TexData {
        format,
        usage,
        pool,
        faces,
        levels: lv,
        nlevels: n,
        gl_tex: 0,
        dirty: true,
        surfaces: Vec::new(),
    };
    let kind = if faces == 6 { Kind::Cube(Mutex::new(t)) } else { Kind::Texture(Mutex::new(t)) };
    Some(new_obj(texture_vtable(faces == 6), device, kind))
}

/// Envoie au GPU une texture modifiée côté CPU ; renvoie (cible GL, id).
unsafe fn upload_texture(t: &mut TexData, g: &gl::GlFunctions, l: &GlLegacy) -> (u32, u32) {
    let cube = t.faces == 6;
    let target = if cube { glc::GL_TEXTURE_CUBE_MAP } else { gl::GL_TEXTURE_2D };
    let Some(fi) = format_info(t.format) else {
        return (target, 0);
    };
    if t.gl_tex == 0 {
        (g.GenTextures)(1, &mut t.gl_tex);
        t.dirty = true;
    }
    (g.BindTexture)(target, t.gl_tex);
    if !t.dirty || fi.internal == 0 {
        return (target, t.gl_tex);
    }
    (g.PixelStorei)(gl::GL_UNPACK_ALIGNMENT, 4);
    (g.TexParameteri)(target, gl::GL_TEXTURE_MAX_LEVEL, t.nlevels as i32 - 1);
    for face in 0..t.faces {
        let ft = if cube { glc::GL_TEXTURE_CUBE_MAP_POSITIVE_X + face } else { gl::GL_TEXTURE_2D };
        for level in 0..t.nlevels {
            let lv = &t.levels[(face * t.nlevels + level) as usize];
            if fi.block != 0 {
                (l.CompressedTexImage2D)(ft, level as i32, fi.internal, lv.width as i32, lv.height as i32, 0, lv.data.len() as i32, lv.data.as_ptr() as *const c_void);
            } else {
                (g.TexImage2D)(ft, level as i32, fi.internal as i32, lv.width as i32, lv.height as i32, 0, fi.format, fi.ty, lv.data.as_ptr() as *const c_void);
            }
        }
    }
    if t.usage & D3DUSAGE_AUTOGENMIPMAP != 0 {
        (g.TexParameteri)(target, gl::GL_TEXTURE_MAX_LEVEL, 1000);
        (g.GenerateMipmap)(target);
    }
    t.dirty = false;
    (target, t.gl_tex)
}

// ── Vertex / index buffers ─────────────────────────────────────────────────

unsafe fn buf_data(this: *mut Obj) -> Option<&'static Mutex<BufData>> {
    match &(*this).kind {
        Kind::VertexBuffer(b) | Kind::IndexBuffer(b) => Some(&*(b as *const Mutex<BufData>)),
        _ => None,
    }
}

unsafe extern "win64" fn buf_lock(this: *mut Obj, offset: u32, _size: u32, out: *mut *mut u8, _flags: u32) -> i32 {
    let Some(m) = buf_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut b = lock(m);
    if offset as usize > b.data.len() {
        return D3DERR_INVALIDCALL;
    }
    *out = b.data.as_mut_ptr().add(offset as usize);
    S_OK
}

#[repr(C)]
struct BufferDesc {
    format: u32,
    ty: u32,
    usage: u32,
    pool: u32,
    size: u32,
    fvf: u32,
}

unsafe extern "win64" fn buf_desc(this: *mut Obj, out: *mut BufferDesc) -> i32 {
    let Some(m) = buf_data(this) else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let b = lock(m);
    let vb = matches!((*this).kind, Kind::VertexBuffer(_));
    let d = BufferDesc {
        format: if vb { 100 } else { b.fvf_or_format }, // D3DFMT_VERTEXDATA
        ty: if vb { D3DRTYPE_VERTEXBUFFER } else { D3DRTYPE_INDEXBUFFER },
        usage: b.usage,
        pool: b.pool,
        size: b.data.len() as u32,
        fvf: if vb { b.fvf_or_format } else { 0 },
    };
    // D3DINDEXBUFFER_DESC n'a pas de champ FVF.
    std::ptr::copy_nonoverlapping(&d as *const BufferDesc as *const u8, out as *mut u8, if vb { 24 } else { 20 });
    S_OK
}

unsafe extern "win64" fn buf_type(this: *mut Obj) -> u32 {
    if matches!((*this).kind, Kind::VertexBuffer(_)) {
        D3DRTYPE_VERTEXBUFFER
    } else {
        D3DRTYPE_INDEXBUFFER
    }
}

fn buffer_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            14,
            f!(stub_invalid),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(obj_get_device)),
                (4, f!(stub_ok)),
                (5, f!(stub_invalid)),
                (6, f!(stub_ok)),
                (7, f!(stub_zero)),
                (8, f!(stub_zero)),
                (9, f!(stub_ok)),
                (10, f!(buf_type)),
                (11, f!(buf_lock)),
                (12, f!(stub_ok)),
                (13, f!(buf_desc)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 14) }
}

// ── Déclarations, shaders, requêtes, state blocks ──────────────────────────

unsafe extern "win64" fn decl_get(this: *mut Obj, out: *mut VertexElement, n: *mut u32) -> i32 {
    let Kind::Decl(elems) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    if n.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let total = elems.len() as u32 + 1;
    if !out.is_null() {
        std::ptr::copy_nonoverlapping(elems.as_ptr(), out, elems.len());
        *out.add(elems.len()) = VertexElement {
            stream: 0xff,
            offset: 0,
            ty: 17,
            method: 0,
            usage: 0,
            usage_index: 0,
        };
    }
    *n = total;
    S_OK
}

fn decl_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(5, f!(stub_invalid), &[(0, f!(obj_qi)), (1, f!(obj_addref)), (2, f!(obj_release)), (3, f!(obj_get_device)), (4, f!(decl_get))])
            .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 5) }
}

unsafe extern "win64" fn shader_get_function(this: *mut Obj, out: *mut u8, size: *mut u32) -> i32 {
    let (Kind::VertexShader(s) | Kind::PixelShader(s)) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    if size.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let bytes = s.tokens.len() as u32 * 4;
    if !out.is_null() {
        if *size < bytes {
            return D3DERR_INVALIDCALL;
        }
        std::ptr::copy_nonoverlapping(s.tokens.as_ptr() as *const u8, out, bytes as usize);
    }
    *size = bytes;
    S_OK
}

fn shader_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(5, f!(stub_invalid), &[(0, f!(obj_qi)), (1, f!(obj_addref)), (2, f!(obj_release)), (3, f!(obj_get_device)), (4, f!(shader_get_function))])
            .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 5) }
}

const D3DQUERYTYPE_EVENT: u32 = 8;
const D3DQUERYTYPE_OCCLUSION: u32 = 9;
const D3DQUERYTYPE_TIMESTAMP: u32 = 10;
const D3DQUERYTYPE_TIMESTAMPFREQ: u32 = 12;

fn query_size(ty: u32) -> Option<u32> {
    Some(match ty {
        D3DQUERYTYPE_EVENT => 4,
        D3DQUERYTYPE_OCCLUSION => 4,
        D3DQUERYTYPE_TIMESTAMP | D3DQUERYTYPE_TIMESTAMPFREQ => 8,
        _ => return None,
    })
}

unsafe extern "win64" fn query_type(this: *mut Obj) -> u32 {
    match (*this).kind {
        Kind::Query(t) => t,
        _ => 0,
    }
}
unsafe extern "win64" fn query_size_fn(this: *mut Obj) -> u32 {
    query_size(query_type(this)).unwrap_or(0)
}
unsafe extern "win64" fn query_get_data(this: *mut Obj, data: *mut u8, size: u32, _flags: u32) -> i32 {
    // Exécution synchrone : le résultat est toujours prêt.
    let ty = query_type(this);
    if data.is_null() || size == 0 {
        return S_OK;
    }
    let v: u64 = match ty {
        D3DQUERYTYPE_EVENT => 1,
        D3DQUERYTYPE_OCCLUSION => 1,
        D3DQUERYTYPE_TIMESTAMP => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
        D3DQUERYTYPE_TIMESTAMPFREQ => 1_000_000_000,
        _ => 0,
    };
    let bytes = v.to_le_bytes();
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, (size as usize).min(8));
    S_OK
}

fn query_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            8,
            f!(stub_invalid),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(obj_get_device)),
                (4, f!(query_type)),
                (5, f!(query_size_fn)),
                (6, f!(stub_ok)),
                (7, f!(query_get_data)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 8) }
}

unsafe extern "win64" fn sb_capture(this: *mut Obj) -> i32 {
    let Kind::StateBlock(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    let Some(dm) = dev_of((*this).device) else {
        return D3DERR_INVALIDCALL;
    };
    let snap = lock(dm).s.clone();
    *lock(m) = Some(Box::new(snap));
    S_OK
}

unsafe extern "win64" fn sb_apply(this: *mut Obj) -> i32 {
    let Kind::StateBlock(m) = &(*this).kind else {
        return D3DERR_INVALIDCALL;
    };
    let Some(dm) = dev_of((*this).device) else {
        return D3DERR_INVALIDCALL;
    };
    let Some(snap) = lock(m).clone() else {
        return S_OK;
    };
    let mut d = lock(dm);
    // Références : on prend celles du bloc avant de rendre les actuelles.
    for p in snap.bound_objects() {
        obj_addref(p);
    }
    for p in d.s.bound_objects() {
        obj_release(p);
    }
    d.s = *snap;
    S_OK
}

fn stateblock_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(6, f!(stub_invalid), &[(0, f!(obj_qi)), (1, f!(obj_addref)), (2, f!(obj_release)), (3, f!(obj_get_device)), (4, f!(sb_capture)), (5, f!(sb_apply))])
            .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 6) }
}

// ── Swapchain implicite ────────────────────────────────────────────────────

unsafe extern "win64" fn sc_present(this: *mut Obj, _src: *const Rect32, _dst: *const Rect32, _wnd: usize, _dirty: *const c_void, _flags: u32) -> i32 {
    dev_present((*this).device, std::ptr::null(), std::ptr::null(), 0, std::ptr::null())
}
unsafe extern "win64" fn sc_back_buffer(this: *mut Obj, i: u32, ty: u32, out: *mut *mut Obj) -> i32 {
    dev_get_back_buffer((*this).device, 0, i, ty, out)
}
unsafe extern "win64" fn sc_display_mode(_: *mut Obj, out: *mut DisplayMode) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = desktop_mode();
    S_OK
}
unsafe extern "win64" fn sc_present_params(this: *mut Obj, out: *mut PresentParams) -> i32 {
    let Some(dm) = dev_of((*this).device) else {
        return D3DERR_INVALIDCALL;
    };
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = lock(dm).pp;
    S_OK
}
unsafe extern "win64" fn sc_addref(this: *mut Obj) -> u32 {
    obj_addref((*this).device)
}
unsafe extern "win64" fn sc_release(this: *mut Obj) -> u32 {
    obj_release((*this).device)
}
unsafe extern "win64" fn sc_get_device(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    obj_get_device(this, out)
}
unsafe extern "win64" fn sc_qi(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_NOINTERFACE;
    }
    if supports(&(*this).kind, &*iid) {
        sc_addref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    E_NOINTERFACE
}

fn swapchain_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            13,
            f!(stub_invalid),
            &[
                (0, f!(sc_qi)),
                (1, f!(sc_addref)),
                (2, f!(sc_release)),
                (3, f!(sc_present)),
                (4, f!(stub_invalid)),
                (5, f!(sc_back_buffer)),
                (6, f!(stub_invalid)),
                (7, f!(sc_display_mode)),
                (8, f!(sc_get_device)),
                (9, f!(sc_present_params)),
                (10, f!(stub_invalid)),
                (11, f!(stub_invalid)),
                (12, f!(stub_invalid)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 13) }
}

// ── IDirect3DDevice9 : méthodes ────────────────────────────────────────────

macro_rules! dev {
    ($this:expr) => {
        match dev_of($this) {
            Some(m) => lock(m),
            None => return D3DERR_INVALIDCALL,
        }
    };
}

unsafe extern "win64" fn dev_test_coop(_: *mut Obj) -> i32 {
    S_OK
}
unsafe extern "win64" fn dev_tex_mem(_: *mut Obj) -> u32 {
    512 << 20
}
unsafe extern "win64" fn dev_get_d3d(_: *mut Obj, out: *mut *mut Obj) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = new_obj(d3d9_vtable(), std::ptr::null_mut(), Kind::D3D9 { ex: false });
    S_OK
}
unsafe extern "win64" fn dev_caps(_: *mut Obj, out: *mut u32) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    fill_caps(out);
    S_OK
}
unsafe extern "win64" fn dev_display_mode(_: *mut Obj, _sc: u32, out: *mut DisplayMode) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = desktop_mode();
    S_OK
}

#[repr(C)]
struct CreationParams {
    adapter: u32,
    ty: u32,
    focus: usize,
    behavior: u32,
}

unsafe extern "win64" fn dev_creation_params(this: *mut Obj, out: *mut CreationParams) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = CreationParams {
        adapter: 0,
        ty: 1,
        focus: d.window,
        behavior: 0x40,
    };
    S_OK
}

unsafe extern "win64" fn dev_get_swapchain(this: *mut Obj, i: u32, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if i != 0 || out.is_null() || d.swapchain.is_null() {
        return D3DERR_INVALIDCALL;
    }
    sc_addref(d.swapchain);
    *out = d.swapchain;
    S_OK
}

unsafe extern "win64" fn dev_num_swapchains(_: *mut Obj) -> u32 {
    1
}

unsafe extern "win64" fn dev_reset(this: *mut Obj, pp: *mut PresentParams) -> i32 {
    if pp.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    let pp = &mut *pp;
    let (ww, wh) = window_size(d.window);
    if pp.width == 0 {
        pp.width = ww;
    }
    if pp.height == 0 {
        pp.height = wh;
    }
    if pp.format == D3DFMT_UNKNOWN {
        pp.format = D3DFMT_X8R8G8B8;
    }
    d.release_bindings();
    for p in [d.backbuffer, d.auto_depth] {
        if !p.is_null() {
            obj_release(p);
        }
    }
    if let Some(gs) = d.gl.as_mut() {
        gs.make_current();
        gs.ctx.resize(pp.width, pp.height);
    }
    d.pp = *pp;
    d.s = Settable::new(pp.auto_depth != 0, pp.width, pp.height);
    let bb = make_surface(&d, std::ptr::null_mut(), pp.width, pp.height, pp.format, D3DUSAGE_RENDERTARGET, D3DPOOL_DEFAULT);
    d.backbuffer = bb;
    obj_addref(bb);
    d.rts[0] = bb;
    d.auto_depth = std::ptr::null_mut();
    if pp.auto_depth != 0 {
        let fmt = if format_info(pp.auto_depth_format).is_some() { pp.auto_depth_format } else { D3DFMT_D24S8 };
        let ds = make_surface(&d, std::ptr::null_mut(), pp.width, pp.height, fmt, D3DUSAGE_DEPTHSTENCIL, D3DPOOL_DEFAULT);
        d.auto_depth = ds;
        obj_addref(ds);
        d.ds = ds;
    }
    d.swapchain = Box::into_raw(Box::new(Obj {
        vtbl: swapchain_vtable().as_ptr(),
        refs: AtomicU32::new(1),
        device: this,
        kind: Kind::SwapChain,
    }));
    S_OK
}

unsafe extern "win64" fn dev_present(this: *mut Obj, _src: *const Rect32, _dst: *const Rect32, _wnd: usize, _dirty: *const c_void) -> i32 {
    let mut d = dev!(this);
    d.present_count += 1;
    let Some(gs) = d.gl.as_ref() else {
        return S_OK;
    };
    let (Some(g), Some(glx)) = (gl::gl(), gl::glx()) else {
        return S_OK;
    };
    let Kind::Surface(m) = &(*d.backbuffer).kind else {
        return S_OK;
    };
    let s = lock(m);
    gs.make_current();
    (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, gs.fbo);
    (g.FramebufferTexture2D)(gl::GL_READ_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0, gl::GL_TEXTURE_2D, s.gl_tex, 0);
    (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, 0);
    (g.Disable)(gl::GL_SCISSOR_TEST);
    let (w, h) = window_size(d.window);
    // Rendu stocké tête en bas (ligne 0 = haut) : on inverse à la copie.
    (g.BlitFramebuffer)(0, 0, s.width as i32, s.height as i32, 0, h as i32, w as i32, 0, gl::GL_COLOR_BUFFER_BIT, gl::GL_LINEAR as u32);
    (glx.SwapBuffers)(gs.ctx.display, gs.ctx.drawable);
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    S_OK
}

unsafe extern "win64" fn dev_present_ex(this: *mut Obj, src: *const Rect32, dst: *const Rect32, wnd: usize, dirty: *const c_void, _flags: u32) -> i32 {
    dev_present(this, src, dst, wnd, dirty)
}

unsafe extern "win64" fn dev_get_back_buffer(this: *mut Obj, sc: u32, i: u32, _ty: u32, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if sc != 0 || i >= d.pp.count || out.is_null() || d.backbuffer.is_null() {
        return D3DERR_INVALIDCALL;
    }
    obj_addref(d.backbuffer);
    *out = d.backbuffer;
    S_OK
}

#[repr(C)]
struct RasterStatus {
    in_vblank: i32,
    scanline: u32,
}

unsafe extern "win64" fn dev_raster_status(_: *mut Obj, _sc: u32, out: *mut RasterStatus) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = RasterStatus {
        in_vblank: 0,
        scanline: 0,
    };
    S_OK
}

unsafe extern "win64" fn dev_set_gamma(this: *mut Obj, _sc: u32, _flags: u32, ramp: *const u8) {
    if let Some(m) = dev_of(this) {
        if !ramp.is_null() {
            lock(m).gamma = std::slice::from_raw_parts(ramp, 1536).to_vec();
        }
    }
}

unsafe extern "win64" fn dev_get_gamma(this: *mut Obj, _sc: u32, ramp: *mut u8) {
    let Some(m) = dev_of(this) else { return };
    if ramp.is_null() {
        return;
    }
    let d = lock(m);
    if d.gamma.len() == 1536 {
        std::ptr::copy_nonoverlapping(d.gamma.as_ptr(), ramp, 1536);
    } else {
        // Rampe identité (3 × 256 WORD).
        for c in 0..3 {
            for i in 0..256u32 {
                let v = (i * 257) as u16;
                std::ptr::write_unaligned(ramp.add((c * 256 + i as usize) * 2) as *mut u16, v);
            }
        }
    }
}

unsafe extern "win64" fn dev_create_texture(this: *mut Obj, w: u32, h: u32, levels: u32, usage: u32, format: u32, pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    match make_texture(this, w, h, levels, usage, format, pool, 1) {
        Some(t) => {
            *out = t;
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}

unsafe extern "win64" fn dev_create_cube(this: *mut Obj, edge: u32, levels: u32, usage: u32, format: u32, pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    match make_texture(this, edge, edge, levels, usage, format, pool, 6) {
        Some(t) => {
            *out = t;
            S_OK
        }
        None => D3DERR_INVALIDCALL,
    }
}

unsafe extern "win64" fn dev_create_volume(this: *mut Obj, w: u32, h: u32, depth: u32, levels: u32, _usage: u32, format: u32, _pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || format_info(format).is_none() || w == 0 || h == 0 || depth == 0 {
        return D3DERR_INVALIDCALL;
    }
    // Stockage CPU seulement : les textures 3D ne sont pas encore échantillonnées.
    let n = mip_count(w, h.max(depth), levels);
    let mut lv = Vec::new();
    for l in 0..n {
        let (pitch, size) = level_layout(format, (w >> l).max(1), (h >> l).max(1));
        let _ = pitch;
        lv.push(vec![0u8; size * (depth >> l).max(1) as usize]);
    }
    *out = new_obj(
        texture_vtable(false),
        this,
        Kind::Volume(Mutex::new(VolData {
            format,
            width: w,
            height: h,
            depth,
            levels: lv,
        })),
    );
    log_warn!("d3d9", "texture volumique créée en mémoire seule (pas d'échantillonnage 3D)");
    S_OK
}

unsafe extern "win64" fn dev_create_vb(this: *mut Obj, len: u32, usage: u32, fvf: u32, pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || len == 0 {
        return D3DERR_INVALIDCALL;
    }
    *out = new_obj(
        buffer_vtable(),
        this,
        Kind::VertexBuffer(Mutex::new(BufData {
            data: vec![0; len as usize],
            usage,
            pool,
            fvf_or_format: fvf,
        })),
    );
    S_OK
}

unsafe extern "win64" fn dev_create_ib(this: *mut Obj, len: u32, usage: u32, format: u32, pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || len == 0 || !matches!(format, D3DFMT_INDEX16 | D3DFMT_INDEX32) {
        return D3DERR_INVALIDCALL;
    }
    *out = new_obj(
        buffer_vtable(),
        this,
        Kind::IndexBuffer(Mutex::new(BufData {
            data: vec![0; len as usize],
            usage,
            pool,
            fvf_or_format: format,
        })),
    );
    S_OK
}

unsafe extern "win64" fn dev_create_rt(this: *mut Obj, w: u32, h: u32, format: u32, _ms: u32, _q: u32, _lockable: i32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || format_info(format).is_none() || w == 0 || h == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *out = make_surface(&d, this, w, h, format, D3DUSAGE_RENDERTARGET, D3DPOOL_DEFAULT);
    S_OK
}

unsafe extern "win64" fn dev_create_ds(this: *mut Obj, w: u32, h: u32, format: u32, _ms: u32, _q: u32, _discard: i32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || !is_depth(format) || w == 0 || h == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *out = make_surface(&d, this, w, h, format, D3DUSAGE_DEPTHSTENCIL, D3DPOOL_DEFAULT);
    S_OK
}

unsafe extern "win64" fn dev_create_offscreen(this: *mut Obj, w: u32, h: u32, format: u32, pool: u32, out: *mut *mut Obj, _shared: *mut usize) -> i32 {
    if out.is_null() || format_info(format).is_none() || w == 0 || h == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *out = make_surface(&d, this, w, h, format, 0, pool);
    S_OK
}

/// Contenu CPU (pitch, octets) d'une surface, en suivant sa texture parente.
unsafe fn surface_bytes(s: *mut Obj) -> Option<(u32, u32, u32, u32, Vec<u8>)> {
    let Kind::Surface(m) = &(*s).kind else {
        return None;
    };
    let sd = lock(m);
    if let Some((parent, face, level)) = sd.parent {
        let tm = tex_data(parent)?;
        let t = lock(tm);
        let l = t.levels.get((face * t.nlevels + level) as usize)?;
        return Some((l.width, l.height, t.format, l.pitch, l.data.clone()));
    }
    Some((sd.width, sd.height, sd.format, sd.pitch, sd.data.clone()))
}

unsafe fn surface_write(s: *mut Obj, data: &[u8]) {
    let Kind::Surface(m) = &(*s).kind else { return };
    let mut sd = lock(m);
    if let Some((parent, face, level)) = sd.parent {
        drop(sd);
        if let Some(tm) = tex_data(parent) {
            let mut t = lock(tm);
            let idx = (face * t.nlevels + level) as usize;
            if let Some(l) = t.levels.get_mut(idx) {
                let n = l.data.len().min(data.len());
                l.data[..n].copy_from_slice(&data[..n]);
            }
            t.dirty = true;
        }
        return;
    }
    let n = sd.data.len().min(data.len());
    sd.data[..n].copy_from_slice(&data[..n]);
    sd.cpu_dirty = true;
}

unsafe extern "win64" fn dev_update_surface(_this: *mut Obj, src: *mut Obj, _sr: *const Rect32, dst: *mut Obj, _dp: *const c_void) -> i32 {
    if src.is_null() || dst.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let Some((_, _, _, _, data)) = surface_bytes(src) else {
        return D3DERR_INVALIDCALL;
    };
    surface_write(dst, &data);
    S_OK
}

unsafe extern "win64" fn dev_update_texture(_this: *mut Obj, src: *mut Obj, dst: *mut Obj) -> i32 {
    let (Some(sm), Some(dm)) = (tex_data(src), tex_data(dst)) else {
        return D3DERR_INVALIDCALL;
    };
    if src == dst {
        return S_OK;
    }
    let s = lock(sm);
    let mut d = lock(dm);
    for (a, b) in d.levels.iter_mut().zip(s.levels.iter()) {
        let n = a.data.len().min(b.data.len());
        a.data[..n].copy_from_slice(&b.data[..n]);
    }
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn dev_get_rt_data(this: *mut Obj, rt: *mut Obj, dst: *mut Obj) -> i32 {
    if rt.is_null() || dst.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if let Kind::Surface(m) = &(*rt).kind {
        let d = dev!(this);
        let mut s = lock(m);
        read_back(&d, &mut s);
    }
    dev_update_surface(this, rt, std::ptr::null(), dst, std::ptr::null())
}

unsafe extern "win64" fn dev_stretch_rect(this: *mut Obj, src: *mut Obj, sr: *const Rect32, dst: *mut Obj, dr: *const Rect32, _filter: u32) -> i32 {
    if src.is_null() || dst.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    // Copie GPU si les deux surfaces sont des cibles GL.
    let tex_of = |o: *mut Obj| -> (u32, u32, u32) {
        if let Kind::Surface(m) = &(*o).kind {
            let s = lock(m);
            (s.gl_tex, s.width, s.height)
        } else {
            (0, 0, 0)
        }
    };
    let (st, sw, sh) = tex_of(src);
    let (dt, dw, dh) = tex_of(dst);
    if let (Some(gs), Some(g), true) = (d.gl.as_ref(), gl::gl(), st != 0 && dt != 0) {
        let r = |p: *const Rect32, w: u32, h: u32| {
            if p.is_null() {
                (0, 0, w as i32, h as i32)
            } else {
                ((*p).left, (*p).top, (*p).right, (*p).bottom)
            }
        };
        let (a, b, c, e) = r(sr, sw, sh);
        let (f, gg, hh, ii) = r(dr, dw, dh);
        gs.make_current();
        let mut tmp = 0;
        (g.GenFramebuffers)(1, &mut tmp);
        (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, tmp);
        (g.FramebufferTexture2D)(gl::GL_READ_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0, gl::GL_TEXTURE_2D, st, 0);
        (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, gs.fbo);
        (g.FramebufferTexture2D)(gl::GL_DRAW_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0, gl::GL_TEXTURE_2D, dt, 0);
        (g.Disable)(gl::GL_SCISSOR_TEST);
        (g.BlitFramebuffer)(a, b, c, e, f, gg, hh, ii, gl::GL_COLOR_BUFFER_BIT, gl::GL_LINEAR as u32);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
        (g.DeleteFramebuffers)(1, &tmp);
        return S_OK;
    }
    drop(d);
    dev_update_surface(this, src, sr, dst, std::ptr::null())
}

unsafe extern "win64" fn dev_color_fill(this: *mut Obj, surf: *mut Obj, rect: *const Rect32, color: u32) -> i32 {
    if surf.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let Some((w, h, fmt, pitch, mut data)) = surface_bytes(surf) else {
        return D3DERR_INVALIDCALL;
    };
    let gl_target = if let Kind::Surface(m) = &(*surf).kind { lock(m).gl_tex } else { 0 };
    if gl_target != 0 {
        // Cible GPU : effacement par Clear sur la surface.
        let d = dev!(this);
        if let (Some(gs), Some(g)) = (d.gl.as_ref(), gl::gl()) {
            gs.make_current();
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, gs.fbo);
            (g.FramebufferTexture2D)(gl::GL_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0, gl::GL_TEXTURE_2D, gl_target, 0);
            if rect.is_null() {
                (g.Disable)(gl::GL_SCISSOR_TEST);
            } else {
                (g.Enable)(gl::GL_SCISSOR_TEST);
                (g.Scissor)((*rect).left, (*rect).top, (*rect).right - (*rect).left, (*rect).bottom - (*rect).top);
            }
            let c = argb_to_rgba(color);
            (g.ClearColor)(c[0], c[1], c[2], c[3]);
            (g.ColorMask)(1, 1, 1, 1);
            (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
            (g.Disable)(gl::GL_SCISSOR_TEST);
        }
        return S_OK;
    }
    let Some(fi) = format_info(fmt) else {
        return D3DERR_INVALIDCALL;
    };
    if fi.bpp != 4 {
        return D3DERR_INVALIDCALL;
    }
    let (l, t, r, b) = if rect.is_null() { (0, 0, w as i32, h as i32) } else { ((*rect).left, (*rect).top, (*rect).right, (*rect).bottom) };
    for y in t.max(0)..b.min(h as i32) {
        for x in l.max(0)..r.min(w as i32) {
            let o = y as usize * pitch as usize + x as usize * 4;
            if o + 4 <= data.len() {
                data[o..o + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }
    surface_write(surf, &data);
    S_OK
}

fn argb_to_rgba(c: u32) -> [f32; 4] {
    [
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
        (c >> 24) as f32 / 255.0,
    ]
}

unsafe extern "win64" fn dev_set_rt(this: *mut Obj, i: u32, surf: *mut Obj) -> i32 {
    let mut d = dev!(this);
    if i >= 4 || (i == 0 && surf.is_null()) {
        return D3DERR_INVALIDCALL;
    }
    rebind(&mut d.rts[i as usize], surf);
    if i == 0 && !surf.is_null() {
        // D3D9 réinitialise viewport et scissor sur la nouvelle cible.
        if let Kind::Surface(m) = &(*surf).kind {
            let (w, h) = {
                let s = lock(m);
                (s.width, s.height)
            };
            d.s.viewport = Viewport {
                x: 0,
                y: 0,
                width: w,
                height: h,
                min_z: 0.0,
                max_z: 1.0,
            };
            d.s.scissor = [0, 0, w as i32, h as i32];
        }
    }
    S_OK
}

unsafe extern "win64" fn dev_get_rt(this: *mut Obj, i: u32, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if i >= 4 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let p = d.rts[i as usize];
    if p.is_null() {
        *out = std::ptr::null_mut();
        return D3DERR_NOTFOUND;
    }
    obj_addref(p);
    *out = p;
    S_OK
}

unsafe extern "win64" fn dev_set_ds(this: *mut Obj, surf: *mut Obj) -> i32 {
    let mut d = dev!(this);
    rebind(&mut d.ds, surf);
    S_OK
}

unsafe extern "win64" fn dev_get_ds(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if d.ds.is_null() {
        *out = std::ptr::null_mut();
        return D3DERR_NOTFOUND;
    }
    obj_addref(d.ds);
    *out = d.ds;
    S_OK
}

unsafe extern "win64" fn dev_begin_scene(this: *mut Obj) -> i32 {
    let mut d = dev!(this);
    if d.in_scene {
        return D3DERR_INVALIDCALL;
    }
    d.in_scene = true;
    S_OK
}

unsafe extern "win64" fn dev_end_scene(this: *mut Obj) -> i32 {
    let mut d = dev!(this);
    if !d.in_scene {
        return D3DERR_INVALIDCALL;
    }
    d.in_scene = false;
    if let (Some(_), Some(g)) = (d.gl.as_ref(), gl::gl()) {
        (g.Flush)();
    }
    S_OK
}

/// Attache les cibles courantes au FBO de l'appareil.
unsafe fn bind_targets(d: &Dev, g: &gl::GlFunctions) -> Option<(u32, u32)> {
    let gs = d.gl.as_ref()?;
    gs.make_current();
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, gs.fbo);
    let mut size = None;
    for (i, &rt) in d.rts.iter().enumerate() {
        let mut tex = 0;
        if !rt.is_null() {
            if let Kind::Surface(m) = &(*rt).kind {
                let s = lock(m);
                if let Some((parent, face, level)) = s.parent {
                    // Rendu dans une texture : on attache son niveau GL.
                    if let (Some(tm), Some(l)) = (tex_data(parent), Some(gs.legacy)) {
                        let mut t = lock(tm);
                        let (target, id) = upload_texture(&mut t, g, l);
                        let ft = if target == glc::GL_TEXTURE_CUBE_MAP { glc::GL_TEXTURE_CUBE_MAP_POSITIVE_X + face } else { gl::GL_TEXTURE_2D };
                        (g.FramebufferTexture2D)(gl::GL_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0 + i as u32, ft, id, level as i32);
                        if i == 0 {
                            size = Some((s.width, s.height));
                        }
                        continue;
                    }
                }
                tex = s.gl_tex;
                if i == 0 {
                    size = Some((s.width, s.height));
                }
            }
        }
        (g.FramebufferTexture2D)(gl::GL_FRAMEBUFFER, gl::GL_COLOR_ATTACHMENT0 + i as u32, gl::GL_TEXTURE_2D, tex, 0);
    }
    let mut rb = 0;
    if !d.ds.is_null() {
        if let Kind::Surface(m) = &(*d.ds).kind {
            rb = lock(m).gl_rb;
        }
    }
    (g.FramebufferRenderbuffer)(gl::GL_FRAMEBUFFER, gl::GL_DEPTH_STENCIL_ATTACHMENT, gl::GL_RENDERBUFFER, rb);
    size
}

#[repr(C)]
#[derive(Clone, Copy)]
struct D3dRect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

unsafe extern "win64" fn dev_clear(this: *mut Obj, count: u32, rects: *const D3dRect, flags: u32, color: u32, z: f32, stencil: u32) -> i32 {
    let d = dev!(this);
    let Some(g) = gl::gl() else { return S_OK };
    let Some(gs) = d.gl.as_ref() else { return S_OK };
    if bind_targets(&d, g).is_none() {
        return S_OK;
    }
    let mut mask = 0;
    if flags & 1 != 0 {
        let c = argb_to_rgba(color);
        (g.ClearColor)(c[0], c[1], c[2], c[3]);
        (g.ColorMask)(1, 1, 1, 1);
        mask |= gl::GL_COLOR_BUFFER_BIT;
    }
    if flags & 2 != 0 {
        (gs.legacy.ClearDepth)(z as f64);
        (g.DepthMask)(1);
        mask |= gl::GL_DEPTH_BUFFER_BIT;
    }
    if flags & 4 != 0 {
        (gs.legacy.ClearStencil)(stencil as i32);
        (gs.legacy.StencilMask)(0xffff_ffff);
        mask |= gl::GL_STENCIL_BUFFER_BIT;
    }
    let vp = d.s.viewport;
    (g.Enable)(gl::GL_SCISSOR_TEST);
    let list: Vec<D3dRect> = if count == 0 || rects.is_null() {
        vec![D3dRect {
            x1: vp.x as i32,
            y1: vp.y as i32,
            x2: (vp.x + vp.width) as i32,
            y2: (vp.y + vp.height) as i32,
        }]
    } else {
        std::slice::from_raw_parts(rects, count as usize).to_vec()
    };
    for r in list {
        (g.Scissor)(r.x1, r.y1, (r.x2 - r.x1).max(0), (r.y2 - r.y1).max(0));
        (g.Clear)(mask);
    }
    (g.Disable)(gl::GL_SCISSOR_TEST);
    S_OK
}

unsafe extern "win64" fn dev_set_transform(this: *mut Obj, state: u32, m: *const [f32; 16]) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    d.s.transforms.insert(state, *m);
    S_OK
}

unsafe extern "win64" fn dev_get_transform(this: *mut Obj, state: u32, m: *mut [f32; 16]) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *m = d.s.transform(state);
    S_OK
}

pub(crate) fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}

unsafe extern "win64" fn dev_multiply_transform(this: *mut Obj, state: u32, m: *const [f32; 16]) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    let cur = d.s.transform(state);
    d.s.transforms.insert(state, mat_mul(&cur, &*m));
    S_OK
}

unsafe extern "win64" fn dev_set_viewport(this: *mut Obj, vp: *const Viewport) -> i32 {
    if vp.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    d.s.viewport = *vp;
    S_OK
}

unsafe extern "win64" fn dev_get_viewport(this: *mut Obj, vp: *mut Viewport) -> i32 {
    if vp.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *vp = d.s.viewport;
    S_OK
}

unsafe extern "win64" fn dev_set_material(this: *mut Obj, m: *const Material) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    d.s.material = *m;
    S_OK
}

unsafe extern "win64" fn dev_get_material(this: *mut Obj, m: *mut Material) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    *m = d.s.material;
    S_OK
}

unsafe extern "win64" fn dev_set_light(this: *mut Obj, i: u32, l: *const Light) -> i32 {
    if l.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    let enabled = d.s.lights.get(&i).map(|x| x.1).unwrap_or(false);
    d.s.lights.insert(i, (*l, enabled));
    S_OK
}

unsafe extern "win64" fn dev_get_light(this: *mut Obj, i: u32, l: *mut Light) -> i32 {
    let d = dev!(this);
    match d.s.lights.get(&i) {
        Some((light, _)) if !l.is_null() => {
            *l = *light;
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}

unsafe extern "win64" fn dev_light_enable(this: *mut Obj, i: u32, enable: i32) -> i32 {
    let mut d = dev!(this);
    let e = d.s.lights.entry(i).or_insert_with(|| {
        // Lumière par défaut D3D9 : directionnelle blanche vers +Z.
        (
            Light {
                ty: 3,
                diffuse: [1.0, 1.0, 1.0, 0.0],
                direction: [0.0, 0.0, 1.0],
                ..Default::default()
            },
            false,
        )
    });
    e.1 = enable != 0;
    S_OK
}

unsafe extern "win64" fn dev_get_light_enable(this: *mut Obj, i: u32, out: *mut i32) -> i32 {
    let d = dev!(this);
    match d.s.lights.get(&i) {
        Some(&(_, e)) if !out.is_null() => {
            *out = if e { 128 } else { 0 };
            S_OK
        }
        _ => D3DERR_INVALIDCALL,
    }
}

unsafe extern "win64" fn dev_set_rs(this: *mut Obj, state: u32, value: u32) -> i32 {
    let mut d = dev!(this);
    if state < 256 {
        d.s.rs[state as usize] = value;
    }
    S_OK
}

unsafe extern "win64" fn dev_get_rs(this: *mut Obj, state: u32, out: *mut u32) -> i32 {
    let d = dev!(this);
    if out.is_null() || state >= 256 {
        return D3DERR_INVALIDCALL;
    }
    *out = d.s.rs[state as usize];
    S_OK
}

unsafe extern "win64" fn dev_create_stateblock(this: *mut Obj, _ty: u32, out: *mut *mut Obj) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let snap = lock(dev_of(this).expect("appareil")).s.clone();
    *out = new_obj(stateblock_vtable(), this, Kind::StateBlock(Mutex::new(Some(Box::new(snap)))));
    S_OK
}

unsafe extern "win64" fn dev_begin_stateblock(this: *mut Obj) -> i32 {
    let mut d = dev!(this);
    if d.recording {
        return D3DERR_INVALIDCALL;
    }
    d.recording = true;
    S_OK
}

unsafe extern "win64" fn dev_end_stateblock(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let snap = {
        let mut d = dev!(this);
        if !d.recording {
            return D3DERR_INVALIDCALL;
        }
        d.recording = false;
        d.s.clone()
    };
    *out = new_obj(stateblock_vtable(), this, Kind::StateBlock(Mutex::new(Some(Box::new(snap)))));
    S_OK
}

fn sampler_slot(stage: u32) -> Option<usize> {
    match stage {
        0..=15 => Some(stage as usize),
        257..=260 => Some(16 + (stage - 257) as usize),
        _ => None,
    }
}

unsafe extern "win64" fn dev_get_texture(this: *mut Obj, stage: u32, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    let (Some(i), false) = (sampler_slot(stage), out.is_null()) else {
        return D3DERR_INVALIDCALL;
    };
    let t = d.s.textures[i];
    if !t.is_null() {
        obj_addref(t);
    }
    *out = t;
    S_OK
}

unsafe extern "win64" fn dev_set_texture(this: *mut Obj, stage: u32, tex: *mut Obj) -> i32 {
    let mut d = dev!(this);
    let Some(i) = sampler_slot(stage) else {
        return D3DERR_INVALIDCALL;
    };
    rebind(&mut d.s.textures[i], tex);
    S_OK
}

unsafe extern "win64" fn dev_get_tss(this: *mut Obj, stage: u32, ty: u32, out: *mut u32) -> i32 {
    let d = dev!(this);
    if stage >= 8 || ty >= 33 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = d.s.tss[stage as usize][ty as usize];
    S_OK
}

unsafe extern "win64" fn dev_set_tss(this: *mut Obj, stage: u32, ty: u32, v: u32) -> i32 {
    let mut d = dev!(this);
    if stage >= 8 || ty >= 33 {
        return D3DERR_INVALIDCALL;
    }
    d.s.tss[stage as usize][ty as usize] = v;
    S_OK
}

unsafe extern "win64" fn dev_get_ss(this: *mut Obj, stage: u32, ty: u32, out: *mut u32) -> i32 {
    let d = dev!(this);
    let (Some(i), true) = (sampler_slot(stage), ty < 14 && !out.is_null()) else {
        return D3DERR_INVALIDCALL;
    };
    *out = d.s.samp[i][ty as usize];
    S_OK
}

unsafe extern "win64" fn dev_set_ss(this: *mut Obj, stage: u32, ty: u32, v: u32) -> i32 {
    let mut d = dev!(this);
    let (Some(i), true) = (sampler_slot(stage), ty < 14) else {
        return D3DERR_INVALIDCALL;
    };
    d.s.samp[i][ty as usize] = v;
    S_OK
}

unsafe extern "win64" fn dev_validate(_: *mut Obj, passes: *mut u32) -> i32 {
    if !passes.is_null() {
        *passes = 1;
    }
    S_OK
}

unsafe extern "win64" fn dev_set_scissor(this: *mut Obj, r: *const Rect32) -> i32 {
    if r.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut d = dev!(this);
    d.s.scissor = [(*r).left, (*r).top, (*r).right, (*r).bottom];
    S_OK
}

unsafe extern "win64" fn dev_get_scissor(this: *mut Obj, r: *mut Rect32) -> i32 {
    if r.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = dev!(this);
    let s = d.s.scissor;
    *r = Rect32 {
        left: s[0],
        top: s[1],
        right: s[2],
        bottom: s[3],
    };
    S_OK
}

unsafe extern "win64" fn dev_get_sw_vp(_: *mut Obj) -> i32 {
    0
}

unsafe extern "win64" fn dev_create_decl(this: *mut Obj, elems: *const VertexElement, out: *mut *mut Obj) -> i32 {
    if elems.is_null() || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let mut v = Vec::new();
    for i in 0..64 {
        let e = *elems.add(i);
        if e.stream == 0xff {
            break;
        }
        v.push(e);
    }
    *out = new_obj(decl_vtable(), this, Kind::Decl(v));
    S_OK
}

unsafe extern "win64" fn dev_set_decl(this: *mut Obj, decl: *mut Obj) -> i32 {
    let mut d = dev!(this);
    rebind(&mut d.s.decl, decl);
    if !decl.is_null() {
        d.s.fvf = 0;
    }
    S_OK
}

unsafe extern "win64" fn dev_get_decl(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !d.s.decl.is_null() {
        obj_addref(d.s.decl);
    }
    *out = d.s.decl;
    S_OK
}

unsafe extern "win64" fn dev_set_fvf(this: *mut Obj, fvf: u32) -> i32 {
    let mut d = dev!(this);
    d.s.fvf = fvf;
    if fvf != 0 {
        rebind(&mut d.s.decl, std::ptr::null_mut());
    }
    S_OK
}

unsafe extern "win64" fn dev_get_fvf(this: *mut Obj, out: *mut u32) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = d.s.fvf;
    S_OK
}

unsafe fn create_shader(this: *mut Obj, func: *const u32, out: *mut *mut Obj, pixel: bool) -> i32 {
    if func.is_null() || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let n = d3d9_shader::token_len(func, 1 << 20);
    let tokens = std::slice::from_raw_parts(func, n).to_vec();
    let translated = match d3d9_shader::translate(&tokens) {
        Ok(s) if s.pixel == pixel => Some(s),
        Ok(_) => None,
        Err(e) => {
            log_warn!("d3d9", "shader non traduit ({}) : repli sur le pipeline fixe", e);
            None
        }
    };
    let data = Box::new(ShaderData {
        tokens,
        translated,
        gl_shader: 0,
    });
    let kind = if pixel { Kind::PixelShader(data) } else { Kind::VertexShader(data) };
    *out = new_obj(shader_vtable(), this, kind);
    S_OK
}

unsafe extern "win64" fn dev_create_vs(this: *mut Obj, func: *const u32, out: *mut *mut Obj) -> i32 {
    create_shader(this, func, out, false)
}
unsafe extern "win64" fn dev_create_ps(this: *mut Obj, func: *const u32, out: *mut *mut Obj) -> i32 {
    create_shader(this, func, out, true)
}
unsafe extern "win64" fn dev_set_vs(this: *mut Obj, s: *mut Obj) -> i32 {
    let mut d = dev!(this);
    rebind(&mut d.s.vs, s);
    S_OK
}
unsafe extern "win64" fn dev_set_ps(this: *mut Obj, s: *mut Obj) -> i32 {
    let mut d = dev!(this);
    rebind(&mut d.s.ps, s);
    S_OK
}
unsafe extern "win64" fn dev_get_vs(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !d.s.vs.is_null() {
        obj_addref(d.s.vs);
    }
    *out = d.s.vs;
    S_OK
}
unsafe extern "win64" fn dev_get_ps(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !d.s.ps.is_null() {
        obj_addref(d.s.ps);
    }
    *out = d.s.ps;
    S_OK
}

macro_rules! const_setters {
    ($set:ident, $get:ident, $field:ident, $ty:ty, $width:expr, $max:expr) => {
        unsafe extern "win64" fn $set(this: *mut Obj, start: u32, data: *const $ty, count: u32) -> i32 {
            let mut d = dev!(this);
            if data.is_null() || start as usize + count as usize > $max {
                return D3DERR_INVALIDCALL;
            }
            let src = std::slice::from_raw_parts(data, count as usize * $width);
            for (i, chunk) in src.chunks($width).enumerate() {
                let dst = &mut d.s.$field[start as usize + i];
                let dst: &mut [$ty] = std::slice::from_raw_parts_mut(dst as *mut _ as *mut $ty, $width);
                dst.copy_from_slice(chunk);
            }
            S_OK
        }
        unsafe extern "win64" fn $get(this: *mut Obj, start: u32, data: *mut $ty, count: u32) -> i32 {
            let d = dev!(this);
            if data.is_null() || start as usize + count as usize > $max {
                return D3DERR_INVALIDCALL;
            }
            for i in 0..count as usize {
                let src = &d.s.$field[start as usize + i];
                let src: &[$ty] = std::slice::from_raw_parts(src as *const _ as *const $ty, $width);
                std::ptr::copy_nonoverlapping(src.as_ptr(), data.add(i * $width), $width);
            }
            S_OK
        }
    };
}

const_setters!(dev_set_vs_f, dev_get_vs_f, vs_f, f32, 4, 256);
const_setters!(dev_set_vs_i, dev_get_vs_i, vs_i, i32, 4, 16);
const_setters!(dev_set_vs_b, dev_get_vs_b, vs_b, i32, 1, 16);
const_setters!(dev_set_ps_f, dev_get_ps_f, ps_f, f32, 4, 224);
const_setters!(dev_set_ps_i, dev_get_ps_i, ps_i, i32, 4, 16);
const_setters!(dev_set_ps_b, dev_get_ps_b, ps_b, i32, 1, 16);

unsafe extern "win64" fn dev_set_stream(this: *mut Obj, n: u32, vb: *mut Obj, offset: u32, stride: u32) -> i32 {
    let mut d = dev!(this);
    if n >= 16 {
        return D3DERR_INVALIDCALL;
    }
    let slot = &mut d.s.streams[n as usize];
    rebind(&mut slot.0, vb);
    slot.1 = offset;
    slot.2 = stride;
    S_OK
}

unsafe extern "win64" fn dev_get_stream(this: *mut Obj, n: u32, out: *mut *mut Obj, offset: *mut u32, stride: *mut u32) -> i32 {
    let d = dev!(this);
    if n >= 16 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let s = d.s.streams[n as usize];
    if !s.0.is_null() {
        obj_addref(s.0);
    }
    *out = s.0;
    if !offset.is_null() {
        *offset = s.1;
    }
    if !stride.is_null() {
        *stride = s.2;
    }
    S_OK
}

unsafe extern "win64" fn dev_set_stream_freq(this: *mut Obj, n: u32, div: u32) -> i32 {
    let mut d = dev!(this);
    if n >= 16 {
        return D3DERR_INVALIDCALL;
    }
    d.s.streams[n as usize].3 = div;
    S_OK
}

unsafe extern "win64" fn dev_get_stream_freq(this: *mut Obj, n: u32, out: *mut u32) -> i32 {
    let d = dev!(this);
    if n >= 16 || out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    *out = d.s.streams[n as usize].3;
    S_OK
}

unsafe extern "win64" fn dev_set_indices(this: *mut Obj, ib: *mut Obj) -> i32 {
    let mut d = dev!(this);
    rebind(&mut d.s.indices, ib);
    S_OK
}

unsafe extern "win64" fn dev_get_indices(this: *mut Obj, out: *mut *mut Obj) -> i32 {
    let d = dev!(this);
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !d.s.indices.is_null() {
        obj_addref(d.s.indices);
    }
    *out = d.s.indices;
    S_OK
}

unsafe extern "win64" fn dev_create_query(this: *mut Obj, ty: u32, out: *mut *mut Obj) -> i32 {
    if query_size(ty).is_none() {
        return D3DERR_NOTAVAILABLE;
    }
    if out.is_null() {
        return S_OK; // test de prise en charge
    }
    *out = new_obj(query_vtable(), this, Kind::Query(ty));
    S_OK
}

unsafe extern "win64" fn dev_check_device_state(_: *mut Obj, _wnd: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn dev_get_max_latency(_: *mut Obj, out: *mut u32) -> i32 {
    if !out.is_null() {
        *out = 3;
    }
    S_OK
}
unsafe extern "win64" fn dev_create_rt_ex(this: *mut Obj, w: u32, h: u32, format: u32, ms: u32, q: u32, lockable: i32, out: *mut *mut Obj, shared: *mut usize, _usage: u32) -> i32 {
    dev_create_rt(this, w, h, format, ms, q, lockable, out, shared)
}
unsafe extern "win64" fn dev_create_offscreen_ex(this: *mut Obj, w: u32, h: u32, format: u32, pool: u32, out: *mut *mut Obj, shared: *mut usize, _usage: u32) -> i32 {
    dev_create_offscreen(this, w, h, format, pool, out, shared)
}
unsafe extern "win64" fn dev_create_ds_ex(this: *mut Obj, w: u32, h: u32, format: u32, ms: u32, q: u32, discard: i32, out: *mut *mut Obj, shared: *mut usize, _usage: u32) -> i32 {
    dev_create_ds(this, w, h, format, ms, q, discard, out, shared)
}
unsafe extern "win64" fn dev_reset_ex(this: *mut Obj, pp: *mut PresentParams, _mode: *mut c_void) -> i32 {
    dev_reset(this, pp)
}
unsafe extern "win64" fn dev_display_mode_ex(_: *mut Obj, _sc: u32, out: *mut u8, rot: *mut u32) -> i32 {
    // D3DDISPLAYMODEEX : Size, Width, Height, RefreshRate, Format, ScanLineOrdering
    if !out.is_null() {
        let m = desktop_mode();
        let v = [24u32, m.width, m.height, m.refresh, m.format, 1];
        std::ptr::copy_nonoverlapping(v.as_ptr() as *const u8, out, 24);
    }
    if !rot.is_null() {
        *rot = 1;
    }
    S_OK
}

// ── Dessin ─────────────────────────────────────────────────────────────────

/// Déclaration équivalente à un FVF.
pub(crate) fn fvf_to_decl(fvf: u32) -> (Vec<VertexElement>, u32) {
    let mut v = Vec::new();
    let mut off = 0u16;
    let mut push = |ty: u8, usage: u8, idx: u8, size: u16| {
        v.push(VertexElement {
            stream: 0,
            offset: off,
            ty,
            method: 0,
            usage,
            usage_index: idx,
        });
        off += size;
    };
    let pos = fvf & 0x400e;
    match pos {
        0x2 => push(2, d3d9_shader::USAGE_POSITION, 0, 12),
        0x4 => push(3, d3d9_shader::USAGE_POSITIONT, 0, 16),
        0x4002 => push(3, d3d9_shader::USAGE_POSITION, 0, 16),
        0x6 | 0x8 | 0xa | 0xc | 0xe => {
            push(2, d3d9_shader::USAGE_POSITION, 0, 12);
            let betas = (pos - 0x4) / 2;
            let last_is_indices = fvf & 0x1000 != 0 || fvf & 0x8000 != 0;
            let weights = if last_is_indices { betas - 1 } else { betas };
            if weights > 0 {
                push((weights - 1) as u8, d3d9_shader::USAGE_BLENDWEIGHT, 0, (weights * 4) as u16);
            }
            if last_is_indices {
                push(if fvf & 0x8000 != 0 { 4 } else { 5 }, d3d9_shader::USAGE_BLENDINDICES, 0, 4);
            }
        }
        _ => {}
    }
    if fvf & 0x10 != 0 {
        push(2, d3d9_shader::USAGE_NORMAL, 0, 12);
    }
    if fvf & 0x20 != 0 {
        push(0, d3d9_shader::USAGE_PSIZE, 0, 4);
    }
    if fvf & 0x40 != 0 {
        push(4, d3d9_shader::USAGE_COLOR, 0, 4);
    }
    if fvf & 0x80 != 0 {
        push(4, d3d9_shader::USAGE_COLOR, 1, 4);
    }
    let ntex = (fvf >> 8) & 0xf;
    for i in 0..ntex.min(8) {
        let (ty, size) = match (fvf >> (16 + i * 2)) & 3 {
            0 => (1u8, 8u16),
            1 => (2, 12),
            2 => (3, 16),
            _ => (0, 4),
        };
        push(ty, d3d9_shader::USAGE_TEXCOORD, i as u8, size);
    }
    (v, off as u32)
}

/// (composantes, type GL, normalisé) d'un D3DDECLTYPE.
fn decl_type_gl(ty: u8) -> Option<(i32, u32, bool)> {
    Some(match ty {
        0 => (1, gl::GL_FLOAT, false),
        1 => (2, gl::GL_FLOAT, false),
        2 => (3, gl::GL_FLOAT, false),
        3 => (4, gl::GL_FLOAT, false),
        4 => (gl::GL_BGRA as i32, gl::GL_UNSIGNED_BYTE, true), // D3DCOLOR
        5 => (4, gl::GL_UNSIGNED_BYTE, false),
        6 => (2, gl::GL_SHORT, false),
        7 => (4, gl::GL_SHORT, false),
        8 => (4, gl::GL_UNSIGNED_BYTE, true),
        9 => (2, gl::GL_SHORT, true),
        10 => (4, gl::GL_SHORT, true),
        11 => (2, gl::GL_UNSIGNED_SHORT, true),
        12 => (4, gl::GL_UNSIGNED_SHORT, true),
        15 => (2, gl::GL_HALF_FLOAT, false),
        16 => (4, gl::GL_HALF_FLOAT, false),
        _ => return None,
    })
}

pub(crate) fn primitive_count(ty: u32, prims: u32) -> Option<(u32, i32)> {
    Some(match ty {
        1 => (gl::GL_POINTS, prims as i32),
        2 => (gl::GL_LINES, prims as i32 * 2),
        3 => (gl::GL_LINE_STRIP, prims as i32 + 1),
        4 => (gl::GL_TRIANGLES, prims as i32 * 3),
        5 => (gl::GL_TRIANGLE_STRIP, prims as i32 + 2),
        6 => (gl::GL_TRIANGLE_FAN, prims as i32 + 2),
        _ => return None,
    })
}

fn blend_gl(b: u32) -> u32 {
    match b {
        1 => gl::GL_ZERO,
        2 => gl::GL_ONE,
        3 => gl::GL_SRC_COLOR,
        4 => gl::GL_ONE_MINUS_SRC_COLOR,
        5 | 12 => gl::GL_SRC_ALPHA,
        6 | 13 => gl::GL_ONE_MINUS_SRC_ALPHA,
        7 => gl::GL_DST_ALPHA,
        8 => gl::GL_ONE_MINUS_DST_ALPHA,
        9 => gl::GL_DST_COLOR,
        10 => gl::GL_ONE_MINUS_DST_COLOR,
        11 => gl::GL_SRC_ALPHA_SATURATE,
        14 => gl::GL_CONSTANT_COLOR,
        15 => 0x8002,
        _ => gl::GL_ONE,
    }
}

fn cmp_gl(c: u32) -> u32 {
    0x0200 + c.clamp(1, 8) - 1
}

fn stencil_op_gl(o: u32) -> u32 {
    match o {
        2 => gl::GL_ZERO,
        3 => gl::GL_REPLACE,
        4 => gl::GL_INCR,
        5 => gl::GL_DECR,
        6 => gl::GL_INVERT,
        7 => gl::GL_INCR_WRAP,
        8 => gl::GL_DECR_WRAP,
        _ => gl::GL_KEEP,
    }
}

fn wrap_gl(a: u32) -> i32 {
    match a {
        2 | 5 => glc::GL_MIRRORED_REPEAT,
        3 => gl::GL_CLAMP_TO_EDGE,
        4 => glc::GL_CLAMP_TO_BORDER,
        _ => gl::GL_REPEAT,
    }
}

unsafe fn apply_render_states(d: &Dev, g: &gl::GlFunctions, l: &GlLegacy) {
    let rs = &d.s.rs;
    let on = |cap: u32, v: bool| {
        if v {
            (g.Enable)(cap)
        } else {
            (g.Disable)(cap)
        }
    };
    on(gl::GL_DEPTH_TEST, rs[7] != 0 && !d.ds.is_null());
    (g.DepthMask)((rs[14] != 0) as u8);
    (g.DepthFunc)(cmp_gl(rs[23]));
    (g.PolygonMode)(
        gl::GL_FRONT_AND_BACK,
        match rs[8] {
            1 => gl::GL_POINT,
            2 => gl::GL_LINE,
            _ => gl::GL_FILL,
        },
    );
    (l.ShadeModel)(if rs[9] == 1 { glc::GL_FLAT } else { glc::GL_SMOOTH });
    // Rendu retourné : la face avant D3D (horaire) devient antihoraire.
    (g.FrontFace)(gl::GL_CCW);
    match rs[22] {
        1 => (g.Disable)(gl::GL_CULL_FACE),
        2 => {
            (g.Enable)(gl::GL_CULL_FACE);
            (g.CullFace)(gl::GL_FRONT);
        }
        _ => {
            (g.Enable)(gl::GL_CULL_FACE);
            (g.CullFace)(gl::GL_BACK);
        }
    }
    on(gl::GL_BLEND, rs[27] != 0);
    if rs[206] != 0 {
        (g.BlendFuncSeparate)(blend_gl(rs[19]), blend_gl(rs[20]), blend_gl(rs[207]), blend_gl(rs[208]));
    } else {
        let (s, dd) = match rs[19] {
            12 => (gl::GL_SRC_ALPHA, gl::GL_ONE_MINUS_SRC_ALPHA),
            13 => (gl::GL_ONE_MINUS_SRC_ALPHA, gl::GL_SRC_ALPHA),
            s => (blend_gl(s), blend_gl(rs[20])),
        };
        (g.BlendFunc)(s, dd);
    }
    (l.BlendEquation)(match rs[171] {
        2 => glc::GL_FUNC_SUBTRACT,
        3 => glc::GL_FUNC_REVERSE_SUBTRACT,
        4 => glc::GL_MIN,
        5 => glc::GL_MAX,
        _ => glc::GL_FUNC_ADD,
    });
    on(glc::GL_ALPHA_TEST, rs[15] != 0);
    (l.AlphaFunc)(cmp_gl(rs[25]), (rs[24] & 0xff) as f32 / 255.0);
    let cw = rs[168];
    (g.ColorMask)((cw & 1 != 0) as u8, (cw & 2 != 0) as u8, (cw & 4 != 0) as u8, (cw & 8 != 0) as u8);
    on(gl::GL_STENCIL_TEST, rs[52] != 0 && !d.ds.is_null());
    (l.StencilFunc)(cmp_gl(rs[56]), rs[57] as i32, rs[58]);
    (l.StencilOp)(stencil_op_gl(rs[53]), stencil_op_gl(rs[54]), stencil_op_gl(rs[55]));
    (l.StencilMask)(rs[59]);
    // Brouillard (table ou sommet), couleur ARGB.
    let fog = rs[28] != 0;
    on(glc::GL_FOG, fog);
    if fog {
        let mode = if rs[35] != 0 { rs[35] } else { rs[140] };
        (l.Fogi)(
            glc::GL_FOG_MODE,
            match mode {
                1 => glc::GL_EXP as i32,
                2 => glc::GL_EXP2 as i32,
                _ => gl::GL_LINEAR,
            },
        );
        (l.Fogf)(glc::GL_FOG_START, f32::from_bits(rs[36]));
        (l.Fogf)(glc::GL_FOG_END, f32::from_bits(rs[37]));
        (l.Fogf)(glc::GL_FOG_DENSITY, f32::from_bits(rs[38]));
        let c = argb_to_rgba(rs[34]);
        (l.Fogfv)(glc::GL_FOG_COLOR, c.as_ptr());
    }
    (l.PointSize)(f32::from_bits(rs[154]).max(1.0));
    // Scissor
    on(gl::GL_SCISSOR_TEST, rs[174] != 0);
    let s = d.s.scissor;
    (g.Scissor)(s[0], s[1], (s[2] - s[0]).max(0), (s[3] - s[1]).max(0));
    let vp = d.s.viewport;
    (g.Viewport)(vp.x as i32, vp.y as i32, vp.width as i32, vp.height as i32);
    (l.DepthRange)(vp.min_z as f64, vp.max_z as f64);
}

unsafe fn bind_samplers(d: &Dev, g: &gl::GlFunctions, l: &GlLegacy, fixed: bool) {
    for (slot, &tex) in d.s.textures.iter().enumerate() {
        let unit = slot as u32;
        (g.ActiveTexture)(gl::GL_TEXTURE0 + unit);
        if tex.is_null() {
            (g.BindTexture)(gl::GL_TEXTURE_2D, 0);
            if fixed && slot < 8 {
                (g.Disable)(gl::GL_TEXTURE_2D);
                (g.Disable)(glc::GL_TEXTURE_CUBE_MAP);
            }
            continue;
        }
        let Some(tm) = tex_data(tex) else { continue };
        let mut t = lock(tm);
        let (target, _) = upload_texture(&mut t, g, l);
        let ss = &d.s.samp[slot];
        let mip = ss[7] != 0 && t.nlevels > 1 || t.usage & D3DUSAGE_AUTOGENMIPMAP != 0;
        let min = match (ss[6], mip, ss[7]) {
            (1, false, _) => gl::GL_NEAREST,
            (_, false, _) => gl::GL_LINEAR,
            (1, true, 1) => gl::GL_NEAREST_MIPMAP_NEAREST,
            (1, true, _) => 0x2702, // NEAREST_MIPMAP_LINEAR
            (_, true, 1) => 0x2701, // LINEAR_MIPMAP_NEAREST
            _ => gl::GL_LINEAR_MIPMAP_LINEAR,
        };
        (g.TexParameteri)(target, gl::GL_TEXTURE_MIN_FILTER, min);
        (g.TexParameteri)(target, gl::GL_TEXTURE_MAG_FILTER, if ss[5] == 1 { gl::GL_NEAREST } else { gl::GL_LINEAR });
        (g.TexParameteri)(target, gl::GL_TEXTURE_WRAP_S, wrap_gl(ss[1]));
        (g.TexParameteri)(target, gl::GL_TEXTURE_WRAP_T, wrap_gl(ss[2]));
        (g.TexParameteri)(target, glc::GL_TEXTURE_WRAP_R, wrap_gl(ss[3]));
        if fixed && slot < 8 {
            let ts = &d.s.tss[slot];
            let colorop = ts[1];
            let enabled = colorop != 1 && !(colorop == 2 && ts[2] == 0) && !(colorop == 3 && ts[3] == 0);
            if enabled {
                (g.Enable)(target);
                let mode = match colorop {
                    2 => glc::GL_REPLACE,
                    7 => glc::GL_ADD,
                    13 => glc::GL_DECAL,
                    _ => glc::GL_MODULATE,
                };
                (l.TexEnvi)(glc::GL_TEXTURE_ENV, glc::GL_TEXTURE_ENV_MODE, mode as i32);
            } else {
                (g.Disable)(target);
            }
        }
    }
    (g.ActiveTexture)(gl::GL_TEXTURE0);
}

unsafe fn apply_fixed_function(d: &Dev, l: &GlLegacy, pretransformed: bool, has_color: bool, has_normal: bool) {
    let vp = d.s.viewport;
    (l.MatrixMode)(glc::GL_PROJECTION);
    if pretransformed {
        // XYZRHW : coordonnées écran ; centre de pixel D3D9 sur l'entier.
        (l.LoadIdentity)();
        let (rw, rh) = target_size(d);
        (l.Ortho)(-0.5, rw as f64 - 0.5, -0.5, rh as f64 - 0.5, 0.0, -1.0);
        (l.MatrixMode)(glc::GL_MODELVIEW);
        (l.LoadIdentity)();
        l.Disable_lighting();
        return;
    }
    // Profondeur [0,1] → [-1,1], retournement vertical, demi-pixel.
    let (w, h) = (vp.width.max(1) as f32, vp.height.max(1) as f32);
    let fix: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0 / w, 1.0 / h, -1.0, 1.0,
    ];
    (l.LoadMatrixf)(fix.as_ptr());
    let proj = d.s.transform(TS_PROJECTION);
    (l.MultMatrixf)(proj.as_ptr());
    (l.MatrixMode)(glc::GL_MODELVIEW);
    let view = d.s.transform(TS_VIEW);
    (l.LoadMatrixf)(view.as_ptr());
    // Lumières (exprimées en espace monde : modelview = vue au moment de les poser).
    let lighting = d.s.rs[137] != 0;
    if lighting {
        l.Enable_lighting();
        let amb = argb_to_rgba(d.s.rs[139]);
        (l.LightModelfv)(glc::GL_LIGHT_MODEL_AMBIENT, amb.as_ptr());
        (l.LightModeli)(
            glc::GL_LIGHT_MODEL_COLOR_CONTROL,
            if d.s.rs[29] != 0 { glc::GL_SEPARATE_SPECULAR_COLOR } else { glc::GL_SINGLE_COLOR } as i32,
        );
        let mut slot = 0u32;
        let mut lights: Vec<(&u32, &(Light, bool))> = d.s.lights.iter().filter(|(_, v)| v.1).collect();
        lights.sort_by_key(|(k, _)| **k);
        for (_, (li, _)) in lights.into_iter().take(8) {
            let gl_l = glc::GL_LIGHT0 + slot;
            slot += 1;
            l.Enable_cap(gl_l);
            (l.Lightfv)(gl_l, glc::GL_DIFFUSE, li.diffuse.as_ptr());
            (l.Lightfv)(gl_l, glc::GL_SPECULAR, li.specular.as_ptr());
            (l.Lightfv)(gl_l, glc::GL_AMBIENT, li.ambient.as_ptr());
            if li.ty == 3 {
                // Directionnelle : GL attend la direction VERS la lumière.
                let p = [-li.direction[0], -li.direction[1], -li.direction[2], 0.0];
                (l.Lightfv)(gl_l, glc::GL_POSITION, p.as_ptr());
                (l.Lightf)(gl_l, glc::GL_SPOT_CUTOFF, 180.0);
            } else {
                let p = [li.position[0], li.position[1], li.position[2], 1.0];
                (l.Lightfv)(gl_l, glc::GL_POSITION, p.as_ptr());
                (l.Lightf)(gl_l, glc::GL_CONSTANT_ATTENUATION, li.att0.max(1e-4));
                (l.Lightf)(gl_l, glc::GL_LINEAR_ATTENUATION, li.att1);
                (l.Lightf)(gl_l, glc::GL_QUADRATIC_ATTENUATION, li.att2);
                if li.ty == 2 {
                    (l.Lightfv)(gl_l, glc::GL_SPOT_DIRECTION, li.direction.as_ptr());
                    (l.Lightf)(gl_l, glc::GL_SPOT_CUTOFF, (li.phi.to_degrees() / 2.0).clamp(0.0, 90.0));
                    (l.Lightf)(gl_l, glc::GL_SPOT_EXPONENT, li.falloff.clamp(0.0, 128.0));
                } else {
                    (l.Lightf)(gl_l, glc::GL_SPOT_CUTOFF, 180.0);
                }
            }
        }
        for s in slot..8 {
            l.Disable_cap(glc::GL_LIGHT0 + s);
        }
        let m = &d.s.material;
        (l.Materialfv)(glc::GL_FRONT_AND_BACK, glc::GL_DIFFUSE, m.diffuse.as_ptr());
        (l.Materialfv)(glc::GL_FRONT_AND_BACK, glc::GL_AMBIENT, m.ambient.as_ptr());
        (l.Materialfv)(glc::GL_FRONT_AND_BACK, glc::GL_SPECULAR, m.specular.as_ptr());
        (l.Materialfv)(glc::GL_FRONT_AND_BACK, glc::GL_EMISSION, m.emissive.as_ptr());
        (l.Materialf)(glc::GL_FRONT_AND_BACK, glc::GL_SHININESS, m.power.clamp(0.0, 128.0));
        if has_color && d.s.rs[141] != 0 && d.s.rs[145] == 1 {
            (l.ColorMaterial)(glc::GL_FRONT_AND_BACK, glc::GL_AMBIENT_AND_DIFFUSE);
            l.Enable_cap(glc::GL_COLOR_MATERIAL);
        } else {
            l.Disable_cap(glc::GL_COLOR_MATERIAL);
        }
        if d.s.rs[143] != 0 || !has_normal {
            l.Enable_cap(glc::GL_NORMALIZE);
        }
    } else {
        l.Disable_lighting();
    }
    let world = d.s.transform(TS_WORLD);
    (l.MultMatrixf)(world.as_ptr());
}

impl GlLegacy {
    unsafe fn Enable_cap(&self, cap: u32) {
        if let Some(g) = gl::gl() {
            (g.Enable)(cap);
        }
    }
    unsafe fn Disable_cap(&self, cap: u32) {
        if let Some(g) = gl::gl() {
            (g.Disable)(cap);
        }
    }
    unsafe fn Enable_lighting(&self) {
        self.Enable_cap(glc::GL_LIGHTING);
    }
    unsafe fn Disable_lighting(&self) {
        self.Disable_cap(glc::GL_LIGHTING);
        self.Disable_cap(glc::GL_COLOR_MATERIAL);
    }
}

fn target_size(d: &Dev) -> (u32, u32) {
    let rt = d.rts[0];
    if rt.is_null() {
        return (d.pp.width, d.pp.height);
    }
    unsafe {
        if let Kind::Surface(m) = &(*rt).kind {
            let s = lock(m);
            return (s.width, s.height);
        }
    }
    (d.pp.width, d.pp.height)
}

/// Compile et lie (avec cache) le programme GLSL des shaders courants.
unsafe fn program_for(d: &mut Dev, g: &gl::GlFunctions, l: &GlLegacy) -> Option<u32> {
    let vs = d.s.vs;
    let ps = d.s.ps;
    let vs_ok = !vs.is_null() && matches!(&(*vs).kind, Kind::VertexShader(s) if s.translated.is_some());
    let ps_ok = !ps.is_null() && matches!(&(*ps).kind, Kind::PixelShader(s) if s.translated.is_some());
    if !vs_ok && !ps_ok {
        return None;
    }
    let key = (if vs_ok { vs as usize } else { 0 }, if ps_ok { ps as usize } else { 0 });
    if let Some(&p) = d.programs.get(&key) {
        return (p != 0).then_some(p);
    }
    let compile = |obj: *mut Obj, ty: u32| -> u32 {
        let (Kind::VertexShader(s) | Kind::PixelShader(s)) = &mut (*obj).kind else {
            return 0;
        };
        if s.gl_shader != 0 {
            return s.gl_shader;
        }
        let Some(t) = &s.translated else { return 0 };
        let src = std::ffi::CString::new(t.glsl.clone()).unwrap_or_default();
        let sh = (g.CreateShader)(ty);
        let ptr = src.as_ptr();
        (g.ShaderSource)(sh, 1, &ptr, std::ptr::null());
        (g.CompileShader)(sh);
        let mut ok = 0;
        (g.GetShaderiv)(sh, gl::GL_COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut buf = vec![0u8; 2048];
            let mut n = 0;
            (g.GetShaderInfoLog)(sh, 2048, &mut n, buf.as_mut_ptr() as *mut gl::GLchar);
            log_warn!("d3d9", "GLSL refusé : {}", String::from_utf8_lossy(&buf[..n.max(0) as usize]));
            (g.DeleteShader)(sh);
            s.translated = None;
            return 0;
        }
        s.gl_shader = sh;
        sh
    };
    let prog = (g.CreateProgram)();
    let mut attached = 0;
    if vs_ok {
        let sh = compile(vs, gl::GL_VERTEX_SHADER);
        if sh != 0 {
            (g.AttachShader)(prog, sh);
            attached += 1;
            if let Kind::VertexShader(s) = &(*vs).kind {
                if let Some(t) = &s.translated {
                    if let Some(p) = t.inputs.iter().find(|i| i.usage == d3d9_shader::USAGE_POSITION || i.usage == d3d9_shader::USAGE_POSITIONT) {
                        let n = std::ffi::CString::new(p.name.clone()).unwrap_or_default();
                        (l.BindAttribLocation)(prog, 0, n.as_ptr());
                    }
                }
            }
        }
    }
    if ps_ok {
        let sh = compile(ps, gl::GL_FRAGMENT_SHADER);
        if sh != 0 {
            (g.AttachShader)(prog, sh);
            attached += 1;
        }
    }
    let mut ok = 0;
    if attached > 0 {
        (g.LinkProgram)(prog);
        (g.GetProgramiv)(prog, gl::GL_LINK_STATUS, &mut ok);
    }
    if ok == 0 {
        log_warn!("d3d9", "édition de liens GLSL refusée : pipeline fixe");
        (g.DeleteProgram)(prog);
        d.programs.insert(key, 0);
        return None;
    }
    d.programs.insert(key, prog);
    Some(prog)
}

unsafe fn upload_uniforms(d: &Dev, g: &gl::GlFunctions, l: &GlLegacy, prog: u32) {
    let loc = |name: &std::ffi::CStr| (g.GetUniformLocation)(prog, name.as_ptr() as *const gl::GLchar);
    let v = loc(c"vc");
    if v >= 0 {
        (l.Uniform4fv)(v, 256, d.s.vs_f.as_ptr() as *const f32);
    }
    let v = loc(c"vci");
    if v >= 0 {
        (l.Uniform4iv)(v, 16, d.s.vs_i.as_ptr() as *const i32);
    }
    let v = loc(c"vcb");
    if v >= 0 {
        (l.Uniform1iv)(v, 16, d.s.vs_b.as_ptr());
    }
    let v = loc(c"pc");
    if v >= 0 {
        (l.Uniform4fv)(v, 224, d.s.ps_f.as_ptr() as *const f32);
    }
    let v = loc(c"pci");
    if v >= 0 {
        (l.Uniform4iv)(v, 16, d.s.ps_i.as_ptr() as *const i32);
    }
    let v = loc(c"pcb");
    if v >= 0 {
        (l.Uniform1iv)(v, 16, d.s.ps_b.as_ptr());
    }
    let vp = d.s.viewport;
    let v = loc(c"wl_pos_fixup");
    if v >= 0 {
        (g.Uniform4f)(v, 1.0 / vp.width.max(1) as f32, 1.0 / vp.height.max(1) as f32, 0.0, 0.0);
    }
    for i in 0..16 {
        let name = std::ffi::CString::new(format!("s{i}")).unwrap_or_default();
        let v = (g.GetUniformLocation)(prog, name.as_ptr() as *const gl::GLchar);
        if v >= 0 {
            // En VS, sN désigne D3DVERTEXTEXTURESAMPLERn (unités 16 à 19).
            (g.Uniform1i)(v, i);
        }
    }
}

/// Branche les tableaux de sommets (client) depuis les flux et la déclaration.
unsafe fn setup_arrays(d: &Dev, l: &GlLegacy, prog: Option<u32>, base_vertex: i32, up: Option<(*const u8, u32)>) -> (bool, bool, bool) {
    let (elems, _) = if !d.s.decl.is_null() {
        match &(*d.s.decl).kind {
            Kind::Decl(e) => (e.clone(), 0),
            _ => (Vec::new(), 0),
        }
    } else {
        fvf_to_decl(d.s.fvf)
    };
    for cap in [glc::GL_VERTEX_ARRAY, glc::GL_NORMAL_ARRAY, glc::GL_COLOR_ARRAY, glc::GL_SECONDARY_COLOR_ARRAY] {
        (l.DisableClientState)(cap);
    }
    for t in 0..8 {
        (l.ClientActiveTexture)(gl::GL_TEXTURE0 + t);
        (l.DisableClientState)(glc::GL_TEXTURE_COORD_ARRAY);
    }
    for a in 0..16 {
        (l.DisableVertexAttribArray)(a);
    }
    let (mut pretransformed, mut has_color, mut has_normal) = (false, false, false);
    for e in &elems {
        let (base, stride): (*const u8, u32) = match up {
            Some((p, stride)) if e.stream == 0 => (p, stride),
            _ => {
                let (vb, off, stride, _) = d.s.streams[e.stream as usize & 15];
                if vb.is_null() {
                    continue;
                }
                let Some(bm) = buf_data(vb) else { continue };
                // Le Vec du tampon reste en place : on garde le pointeur
                // le temps du dessin (pas de redimensionnement possible).
                let p = lock(bm).data.as_ptr();
                (p.add(off as usize), stride)
            }
        };
        let Some((size, ty, norm)) = decl_type_gl(e.ty) else { continue };
        let ptr = base.offset(e.offset as isize + base_vertex as isize * stride as isize) as *const c_void;
        let s = stride as i32;
        use d3d9_shader as sh;
        match (e.usage, e.usage_index) {
            (sh::USAGE_POSITION, 0) => {
                (l.EnableClientState)(glc::GL_VERTEX_ARRAY);
                (l.VertexPointer)(if size > 4 { 4 } else { size.min(4).max(2) }, ty, s, ptr);
            }
            (sh::USAGE_POSITIONT, 0) => {
                pretransformed = true;
                (l.EnableClientState)(glc::GL_VERTEX_ARRAY);
                (l.VertexPointer)(3, ty, s, ptr);
            }
            (sh::USAGE_NORMAL, 0) => {
                has_normal = true;
                (l.EnableClientState)(glc::GL_NORMAL_ARRAY);
                (l.NormalPointer)(ty, s, ptr);
            }
            (sh::USAGE_COLOR, 0) => {
                has_color = true;
                (l.EnableClientState)(glc::GL_COLOR_ARRAY);
                (l.ColorPointer)(size, ty, s, ptr);
            }
            (sh::USAGE_COLOR, 1) => {
                (l.EnableClientState)(glc::GL_SECONDARY_COLOR_ARRAY);
                (l.SecondaryColorPointer)(if size == gl::GL_BGRA as i32 { size } else { 3 }, ty, s, ptr);
            }
            (sh::USAGE_TEXCOORD, i) if i < 8 => {
                // TEXCOORDINDEX : l'étage t lit le jeu de coordonnées tss[11].
                (l.ClientActiveTexture)(gl::GL_TEXTURE0 + i as u32);
                (l.EnableClientState)(glc::GL_TEXTURE_COORD_ARRAY);
                (l.TexCoordPointer)(size.min(4), ty, s, ptr);
            }
            _ => {}
        }
        if let Some(prog) = prog {
            let name = std::ffi::CString::new(d3d9_shader::attr_name(e.usage, e.usage_index)).unwrap_or_default();
            let loc = (l.GetAttribLocation)(prog, name.as_ptr());
            if loc >= 0 {
                (l.EnableVertexAttribArray)(loc as u32);
                (l.VertexAttribPointer)(loc as u32, size, ty, norm as u8, s, ptr);
            }
        }
    }
    (l.ClientActiveTexture)(gl::GL_TEXTURE0);
    (pretransformed, has_color, has_normal)
}

/// Prépare tout l'état GL d'un dessin ; `None` en mode sans rendu.
unsafe fn prepare_draw(d: &mut Dev, base_vertex: i32, up: Option<(*const u8, u32)>) -> Option<()> {
    let g = gl::gl()?;
    let l = d.gl.as_ref()?.legacy;
    bind_targets(d, g)?;
    (g.BindBuffer)(gl::GL_ARRAY_BUFFER, 0);
    (g.BindBuffer)(gl::GL_ELEMENT_ARRAY_BUFFER, 0);
    (g.BindVertexArray)(0);
    apply_render_states(d, g, l);
    let prog = program_for(d, g, l);
    (g.UseProgram)(prog.unwrap_or(0));
    if let Some(p) = prog {
        upload_uniforms(d, g, l, p);
    }
    let vs_active = prog.is_some() && !d.s.vs.is_null() && matches!(&(*d.s.vs).kind, Kind::VertexShader(s) if s.gl_shader != 0);
    let ps_active = prog.is_some() && !d.s.ps.is_null() && matches!(&(*d.s.ps).kind, Kind::PixelShader(s) if s.gl_shader != 0);
    bind_samplers(d, g, l, !ps_active);
    let (pre, color, normal) = setup_arrays(d, l, prog, base_vertex, up);
    if !vs_active {
        apply_fixed_function(d, l, pre, color, normal);
    }
    Some(())
}

unsafe extern "win64" fn dev_draw(this: *mut Obj, ty: u32, start: u32, prims: u32) -> i32 {
    let mut d = dev!(this);
    let Some((mode, count)) = primitive_count(ty, prims) else {
        return D3DERR_INVALIDCALL;
    };
    if prims == 0 {
        return S_OK;
    }
    if prepare_draw(&mut d, 0, None).is_some() {
        if let Some(g) = gl::gl() {
            (g.DrawArrays)(mode, start as i32, count);
        }
    }
    S_OK
}

unsafe extern "win64" fn dev_draw_indexed(this: *mut Obj, ty: u32, base: i32, _min: u32, _num: u32, start: u32, prims: u32) -> i32 {
    let mut d = dev!(this);
    let Some((mode, count)) = primitive_count(ty, prims) else {
        return D3DERR_INVALIDCALL;
    };
    if prims == 0 {
        return S_OK;
    }
    let ib = d.s.indices;
    if ib.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let Some(bm) = buf_data(ib) else {
        return D3DERR_INVALIDCALL;
    };
    let (ptr, fmt, len) = {
        let b = lock(bm);
        (b.data.as_ptr(), b.fvf_or_format, b.data.len())
    };
    let isz = if fmt == D3DFMT_INDEX32 { 4 } else { 2 };
    if (start as usize + count as usize) * isz > len {
        return D3DERR_INVALIDCALL;
    }
    if prepare_draw(&mut d, base, None).is_some() {
        if let Some(g) = gl::gl() {
            let ity = if isz == 4 { gl::GL_UNSIGNED_INT } else { gl::GL_UNSIGNED_SHORT };
            (g.DrawElements)(mode, count, ity, ptr.add(start as usize * isz) as *const c_void);
        }
    }
    S_OK
}

unsafe extern "win64" fn dev_draw_up(this: *mut Obj, ty: u32, prims: u32, data: *const u8, stride: u32) -> i32 {
    let mut d = dev!(this);
    let Some((mode, count)) = primitive_count(ty, prims) else {
        return D3DERR_INVALIDCALL;
    };
    if data.is_null() || prims == 0 {
        return if data.is_null() { D3DERR_INVALIDCALL } else { S_OK };
    }
    if prepare_draw(&mut d, 0, Some((data, stride))).is_some() {
        if let Some(g) = gl::gl() {
            (g.DrawArrays)(mode, 0, count);
        }
    }
    // D3D9 délie le flux 0 après un dessin « UP ».
    rebind(&mut d.s.streams[0].0, std::ptr::null_mut());
    S_OK
}

unsafe extern "win64" fn dev_draw_indexed_up(this: *mut Obj, ty: u32, _min: u32, _num: u32, prims: u32, idx: *const u8, fmt: u32, data: *const u8, stride: u32) -> i32 {
    let mut d = dev!(this);
    let Some((mode, count)) = primitive_count(ty, prims) else {
        return D3DERR_INVALIDCALL;
    };
    if data.is_null() || idx.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if prims == 0 {
        return S_OK;
    }
    if prepare_draw(&mut d, 0, Some((data, stride))).is_some() {
        if let Some(g) = gl::gl() {
            let ity = if fmt == D3DFMT_INDEX32 { gl::GL_UNSIGNED_INT } else { gl::GL_UNSIGNED_SHORT };
            (g.DrawElements)(mode, count, ity, idx as *const c_void);
        }
    }
    rebind(&mut d.s.streams[0].0, std::ptr::null_mut());
    rebind(&mut d.s.indices, std::ptr::null_mut());
    S_OK
}

fn device_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            134,
            f!(stub_invalid),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(dev_test_coop)),
                (4, f!(dev_tex_mem)),
                (5, f!(stub_ok)),
                (6, f!(dev_get_d3d)),
                (7, f!(dev_caps)),
                (8, f!(dev_display_mode)),
                (9, f!(dev_creation_params)),
                (10, f!(stub_ok)),
                (11, f!(stub_ok)),
                (12, f!(stub_zero)),
                (13, f!(stub_invalid)),
                (14, f!(dev_get_swapchain)),
                (15, f!(dev_num_swapchains)),
                (16, f!(dev_reset)),
                (17, f!(dev_present)),
                (18, f!(dev_get_back_buffer)),
                (19, f!(dev_raster_status)),
                (20, f!(stub_ok)),
                (21, f!(dev_set_gamma)),
                (22, f!(dev_get_gamma)),
                (23, f!(dev_create_texture)),
                (24, f!(dev_create_volume)),
                (25, f!(dev_create_cube)),
                (26, f!(dev_create_vb)),
                (27, f!(dev_create_ib)),
                (28, f!(dev_create_rt)),
                (29, f!(dev_create_ds)),
                (30, f!(dev_update_surface)),
                (31, f!(dev_update_texture)),
                (32, f!(dev_get_rt_data)),
                (33, f!(stub_invalid)),
                (34, f!(dev_stretch_rect)),
                (35, f!(dev_color_fill)),
                (36, f!(dev_create_offscreen)),
                (37, f!(dev_set_rt)),
                (38, f!(dev_get_rt)),
                (39, f!(dev_set_ds)),
                (40, f!(dev_get_ds)),
                (41, f!(dev_begin_scene)),
                (42, f!(dev_end_scene)),
                (43, f!(dev_clear)),
                (44, f!(dev_set_transform)),
                (45, f!(dev_get_transform)),
                (46, f!(dev_multiply_transform)),
                (47, f!(dev_set_viewport)),
                (48, f!(dev_get_viewport)),
                (49, f!(dev_set_material)),
                (50, f!(dev_get_material)),
                (51, f!(dev_set_light)),
                (52, f!(dev_get_light)),
                (53, f!(dev_light_enable)),
                (54, f!(dev_get_light_enable)),
                (55, f!(stub_ok)),
                (56, f!(stub_ok)),
                (57, f!(dev_set_rs)),
                (58, f!(dev_get_rs)),
                (59, f!(dev_create_stateblock)),
                (60, f!(dev_begin_stateblock)),
                (61, f!(dev_end_stateblock)),
                (62, f!(stub_ok)),
                (63, f!(stub_ok)),
                (64, f!(dev_get_texture)),
                (65, f!(dev_set_texture)),
                (66, f!(dev_get_tss)),
                (67, f!(dev_set_tss)),
                (68, f!(dev_get_ss)),
                (69, f!(dev_set_ss)),
                (70, f!(dev_validate)),
                (71, f!(stub_ok)),
                (72, f!(stub_ok)),
                (73, f!(stub_ok)),
                (74, f!(stub_ok)),
                (75, f!(dev_set_scissor)),
                (76, f!(dev_get_scissor)),
                (77, f!(stub_ok)),
                (78, f!(dev_get_sw_vp)),
                (79, f!(stub_ok)),
                (80, f!(stub_zero)),
                (81, f!(dev_draw)),
                (82, f!(dev_draw_indexed)),
                (83, f!(dev_draw_up)),
                (84, f!(dev_draw_indexed_up)),
                (85, f!(stub_invalid)),
                (86, f!(dev_create_decl)),
                (87, f!(dev_set_decl)),
                (88, f!(dev_get_decl)),
                (89, f!(dev_set_fvf)),
                (90, f!(dev_get_fvf)),
                (91, f!(dev_create_vs)),
                (92, f!(dev_set_vs)),
                (93, f!(dev_get_vs)),
                (94, f!(dev_set_vs_f)),
                (95, f!(dev_get_vs_f)),
                (96, f!(dev_set_vs_i)),
                (97, f!(dev_get_vs_i)),
                (98, f!(dev_set_vs_b)),
                (99, f!(dev_get_vs_b)),
                (100, f!(dev_set_stream)),
                (101, f!(dev_get_stream)),
                (102, f!(dev_set_stream_freq)),
                (103, f!(dev_get_stream_freq)),
                (104, f!(dev_set_indices)),
                (105, f!(dev_get_indices)),
                (106, f!(dev_create_ps)),
                (107, f!(dev_set_ps)),
                (108, f!(dev_get_ps)),
                (109, f!(dev_set_ps_f)),
                (110, f!(dev_get_ps_f)),
                (111, f!(dev_set_ps_i)),
                (112, f!(dev_get_ps_i)),
                (113, f!(dev_set_ps_b)),
                (114, f!(dev_get_ps_b)),
                (115, f!(stub_invalid)),
                (116, f!(stub_invalid)),
                (117, f!(stub_invalid)),
                (118, f!(dev_create_query)),
                // IDirect3DDevice9Ex
                (119, f!(stub_ok)),
                (120, f!(stub_invalid)),
                (121, f!(dev_present_ex)),
                (122, f!(stub_ok)),
                (123, f!(stub_ok)),
                (124, f!(stub_ok)),
                (125, f!(stub_ok)),
                (126, f!(stub_ok)),
                (127, f!(dev_get_max_latency)),
                (128, f!(dev_check_device_state)),
                (129, f!(dev_create_rt_ex)),
                (130, f!(dev_create_offscreen_ex)),
                (131, f!(dev_create_ds_ex)),
                (132, f!(dev_reset_ex)),
                (133, f!(dev_display_mode_ex)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 134) }
}

// ── Points d'entrée ────────────────────────────────────────────────────────

const D3D_SDK_VERSION_MIN: u32 = 31;

pub unsafe extern "win64" fn Direct3DCreate9(sdk: u32) -> *mut Obj {
    log_info!("d3d9", "Direct3DCreate9 (SDK {}) → traduction Winlux OpenGL", sdk);
    new_obj(d3d9_vtable(), std::ptr::null_mut(), Kind::D3D9 { ex: false })
}

pub unsafe extern "win64" fn Direct3DCreate9Ex(sdk: u32, out: *mut *mut Obj) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let _ = D3D_SDK_VERSION_MIN;
    log_info!("d3d9", "Direct3DCreate9Ex (SDK {})", sdk);
    *out = new_obj(d3d9_vtable(), std::ptr::null_mut(), Kind::D3D9 { ex: true });
    S_OK
}

/// API de marqueurs de profilage (PIX) : appelée par beaucoup de jeux.
pub unsafe extern "win64" fn D3DPERF_BeginEvent(_color: u32, _name: *const u16) -> i32 {
    0
}
pub unsafe extern "win64" fn D3DPERF_EndEvent() -> i32 {
    0
}
pub unsafe extern "win64" fn D3DPERF_SetMarker(_color: u32, _name: *const u16) {}
pub unsafe extern "win64" fn D3DPERF_SetRegion(_color: u32, _name: *const u16) {}
pub unsafe extern "win64" fn D3DPERF_QueryRepeatFrame() -> i32 {
    0
}
pub unsafe extern "win64" fn D3DPERF_SetOptions(_opts: u32) {}
pub unsafe extern "win64" fn D3DPERF_GetStatus() -> u32 {
    0
}

/// Accès interne (DXVA2) : crée une surface vidéo CPU rattachée à un appareil.
pub(crate) unsafe fn create_video_surface(device: *mut Obj, w: u32, h: u32, format: u32) -> Option<*mut Obj> {
    let m = dev_of(device)?;
    let d = lock(m);
    Some(make_surface(&d, device, w, h, format, 0, D3DPOOL_DEFAULT))
}

/// Accès interne (DXVA2) : tampon CPU d'une surface, verrouillé le temps de `f`.
pub(crate) unsafe fn with_surface_bytes<R>(surface: *mut Obj, f: impl FnOnce(u32, u32, u32, &mut [u8]) -> R) -> Option<R> {
    let Kind::Surface(m) = &(*surface).kind else {
        return None;
    };
    let mut s = lock(m);
    let (w, h, p) = (s.width, s.height, s.pitch);
    Some(f(w, h, p, &mut s.data))
}

pub(crate) unsafe fn is_device(p: *mut c_void) -> bool {
    !p.is_null() && *(p as *const *const usize) == device_vtable().as_ptr()
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "Direct3DCreate9" => fn Direct3DCreate9,
        "Direct3DCreate9Ex" => fn Direct3DCreate9Ex,
        "D3DPERF_BeginEvent" => fn D3DPERF_BeginEvent,
        "D3DPERF_EndEvent" => fn D3DPERF_EndEvent,
        "D3DPERF_SetMarker" => fn D3DPERF_SetMarker,
        "D3DPERF_SetRegion" => fn D3DPERF_SetRegion,
        "D3DPERF_QueryRepeatFrame" => fn D3DPERF_QueryRepeatFrame,
        "D3DPERF_SetOptions" => fn D3DPERF_SetOptions,
        "D3DPERF_GetStatus" => fn D3DPERF_GetStatus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<F>(obj: *mut Obj, idx: usize) -> F
    where
        F: Copy,
    {
        let vt = (*obj).vtbl;
        std::mem::transmute_copy::<usize, F>(&*vt.add(idx))
    }

    #[test]
    fn fvf_classiques() {
        // XYZ | DIFFUSE | TEX1
        let (d, stride) = fvf_to_decl(0x2 | 0x40 | 0x100);
        assert_eq!(stride, 24);
        assert_eq!(d.len(), 3);
        assert_eq!(d[1].usage, d3d9_shader::USAGE_COLOR);
        assert_eq!(d[2].offset, 16);
        // XYZRHW | DIFFUSE
        let (d, stride) = fvf_to_decl(0x4 | 0x40);
        assert_eq!(stride, 20);
        assert_eq!(d[0].usage, d3d9_shader::USAGE_POSITIONT);
        // XYZ | NORMAL | TEX2 avec TEX1 en 3D (D3DFVF_TEXCOORDSIZE3(1))
        let (d, stride) = fvf_to_decl(0x2 | 0x10 | 0x200 | (1 << 18));
        assert_eq!(stride, 12 + 12 + 8 + 12);
        assert_eq!(d[3].ty, 2);
    }

    #[test]
    fn primitives() {
        assert_eq!(primitive_count(4, 2), Some((gl::GL_TRIANGLES, 6)));
        assert_eq!(primitive_count(5, 2), Some((gl::GL_TRIANGLE_STRIP, 4)));
        assert_eq!(primitive_count(3, 3), Some((gl::GL_LINE_STRIP, 4)));
        assert_eq!(primitive_count(9, 1), None);
    }

    #[test]
    fn dispositions_de_niveaux() {
        assert_eq!(level_layout(D3DFMT_A8R8G8B8, 3, 2), (12, 24));
        assert_eq!(level_layout(D3DFMT_DXT1, 5, 5), (16, 32));
        assert_eq!(level_layout(D3DFMT_DXT5, 4, 4), (16, 16));
        assert_eq!(level_layout(D3DFMT_R5G6B5, 3, 1), (8, 8));
        // NV12 : luma + chroma entrelacée (moitié de hauteur)
        assert_eq!(level_layout(D3DFMT_NV12, 16, 16), (16, 16 * 24));
        assert_eq!(mip_count(256, 64, 0), 9);
        assert_eq!(mip_count(256, 64, 3), 3);
    }

    #[test]
    fn multiplication_matrices() {
        let mut t = IDENTITY;
        t[12] = 5.0;
        let r = mat_mul(&t, &IDENTITY);
        assert_eq!(r, t);
    }

    /// Parcours complet en mode sans rendu (pas de fenêtre) : création de
    /// l'appareil, ressources, verrous, états, dessin, présentation.
    #[test]
    fn appareil_sans_rendu_complet() {
        unsafe {
            let d3d = Direct3DCreate9(32);
            type CreateDevice = unsafe extern "win64" fn(*mut Obj, u32, u32, usize, u32, *mut PresentParams, *mut *mut Obj) -> i32;
            let mut pp = PresentParams {
                windowed: 1,
                swap_effect: 1,
                auto_depth: 1,
                auto_depth_format: D3DFMT_D24S8,
                ..Default::default()
            };
            let mut dev = std::ptr::null_mut();
            let hr = call::<CreateDevice>(d3d, 16)(d3d, 0, 1, 0, 0x40, &mut pp, &mut dev);
            assert_eq!(hr, S_OK);
            assert!(is_device(dev as *mut c_void));
            assert_eq!(pp.width, 800);

            // Texture 2D : verrou, écriture, déverrouillage, niveaux.
            type CreateTex = unsafe extern "win64" fn(*mut Obj, u32, u32, u32, u32, u32, u32, *mut *mut Obj, *mut usize) -> i32;
            let mut tex = std::ptr::null_mut();
            assert_eq!(call::<CreateTex>(dev, 23)(dev, 64, 32, 0, 0, D3DFMT_A8R8G8B8, D3DPOOL_MANAGED, &mut tex, std::ptr::null_mut()), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj) -> u32>(tex, 13)(tex), 7);
            type LockRect = unsafe extern "win64" fn(*mut Obj, u32, *mut LockedRect, *const Rect32, u32) -> i32;
            let mut lr = LockedRect {
                pitch: 0,
                bits: std::ptr::null_mut(),
            };
            assert_eq!(call::<LockRect>(tex, 19)(tex, 0, &mut lr, std::ptr::null(), 0), S_OK);
            assert_eq!(lr.pitch, 256);
            *lr.bits = 0xab;
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32) -> i32>(tex, 20)(tex, 0), S_OK);
            // Surface du niveau 0 : même mémoire.
            let mut surf = std::ptr::null_mut();
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, *mut *mut Obj) -> i32>(tex, 18)(tex, 0, &mut surf), S_OK);
            let mut lr2 = LockedRect {
                pitch: 0,
                bits: std::ptr::null_mut(),
            };
            type SLock = unsafe extern "win64" fn(*mut Obj, *mut LockedRect, *const Rect32, u32) -> i32;
            assert_eq!(call::<SLock>(surf, 13)(surf, &mut lr2, std::ptr::null(), 0), S_OK);
            assert_eq!(*lr2.bits, 0xab);

            // Vertex buffer + FVF + dessin (aucun rendu, aucun crash).
            type CreateVb = unsafe extern "win64" fn(*mut Obj, u32, u32, u32, u32, *mut *mut Obj, *mut usize) -> i32;
            let mut vb = std::ptr::null_mut();
            assert_eq!(call::<CreateVb>(dev, 26)(dev, 72, 0, 0x42, 0, &mut vb, std::ptr::null_mut()), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, *mut Obj, u32, u32) -> i32>(dev, 100)(dev, 0, vb, 0, 16), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32) -> i32>(dev, 89)(dev, 0x42), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj) -> i32>(dev, 41)(dev), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, u32, u32) -> i32>(dev, 81)(dev, 4, 0, 1), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj) -> i32>(dev, 42)(dev), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, *const Rect32, *const Rect32, usize, *const c_void) -> i32>(dev, 17)(dev, std::ptr::null(), std::ptr::null(), 0, std::ptr::null()), S_OK);

            // États de rendu et state block.
            type SetRs = unsafe extern "win64" fn(*mut Obj, u32, u32) -> i32;
            type GetRs = unsafe extern "win64" fn(*mut Obj, u32, *mut u32) -> i32;
            let mut v = 0;
            call::<GetRs>(dev, 58)(dev, 137, &mut v);
            assert_eq!(v, 1, "LIGHTING vaut TRUE par défaut");
            call::<SetRs>(dev, 57)(dev, 27, 1);
            let mut sb = std::ptr::null_mut();
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, *mut *mut Obj) -> i32>(dev, 59)(dev, 1, &mut sb), S_OK);
            call::<SetRs>(dev, 57)(dev, 27, 0);
            call::<unsafe extern "win64" fn(*mut Obj) -> i32>(sb, 5)(sb);
            call::<GetRs>(dev, 58)(dev, 27, &mut v);
            assert_eq!(v, 1, "Apply restaure l'état capturé");

            // Shader SM2 traduit.
            let ps: [u32; 5] = [0xffff_0200, 0x0200_0001, 0x800f_0800, 0xa0e4_0000, 0x0000_ffff];
            let mut pso: *mut Obj = std::ptr::null_mut();
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, *const u32, *mut *mut Obj) -> i32>(dev, 106)(dev, ps.as_ptr(), &mut pso), S_OK);
            assert!(!pso.is_null());
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, *mut Obj) -> i32>(dev, 107)(dev, pso), S_OK);
            call::<unsafe extern "win64" fn(*mut Obj, *mut Obj) -> i32>(dev, 107)(dev, std::ptr::null_mut());
            obj_release(pso);

            // Requête d'évènement : test de prise en charge sans objet.
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, *mut *mut Obj) -> i32>(dev, 118)(dev, 8, std::ptr::null_mut()), S_OK);
            assert_eq!(call::<unsafe extern "win64" fn(*mut Obj, u32, *mut *mut Obj) -> i32>(dev, 118)(dev, 99, std::ptr::null_mut()), D3DERR_NOTAVAILABLE);

            // Libérations dans l'ordre inverse.
            for o in [sb, vb, surf, tex] {
                obj_release(o);
            }
            call::<unsafe extern "win64" fn(*mut Obj, u32, *mut Obj, u32, u32) -> i32>(dev, 100)(dev, 0, std::ptr::null_mut(), 0, 0);
            assert_eq!(obj_release(dev), 0);
            obj_release(d3d);
        }
    }

    #[test]
    fn formats_annonces() {
        unsafe {
            let d3d = Direct3DCreate9(32);
            assert_eq!(d3d_check_format(d3d, 0, 1, D3DFMT_X8R8G8B8, 0, D3DRTYPE_TEXTURE, D3DFMT_DXT5), S_OK);
            assert_eq!(d3d_check_format(d3d, 0, 1, D3DFMT_X8R8G8B8, D3DUSAGE_RENDERTARGET, D3DRTYPE_TEXTURE, D3DFMT_DXT1), D3DERR_NOTAVAILABLE);
            assert_eq!(d3d_check_format(d3d, 0, 1, D3DFMT_X8R8G8B8, 0, D3DRTYPE_TEXTURE, 9999), D3DERR_NOTAVAILABLE);
            let mut caps = [0u32; 76];
            assert_eq!(d3d_caps(d3d, 0, 1, caps.as_mut_ptr()), S_OK);
            assert_eq!(caps[49], 0xfffe_0300);
            assert_eq!(caps[51], 0xffff_0300);
            obj_release(d3d);
        }
    }
}
