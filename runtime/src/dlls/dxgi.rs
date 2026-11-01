//! Implémentation DXGI → OpenGL/GLX pour Winlux.
//! Traduit les interfaces DXGI (Factory, Adapter, Output, SwapChain)
//! vers un backend OpenGL via GLX sur X11.
#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use crate::dlls::gl_bindings as gl;
use crate::dlls::ole32::Guid;
use crate::{log_debug, log_error, log_info, log_warn};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const DXGI_ERROR_NOT_FOUND: i32 = 0x887A_0002u32 as i32;

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_IDXGI_FACTORY: Guid = Guid {
    data1: 0x7b7166ec,
    data2: 0x21c7,
    data3: 0x44ae,
    data4: [0xb2, 0x1a, 0xc9, 0xae, 0x32, 0x1a, 0xe3, 0x69],
};
const IID_IDXGI_FACTORY1: Guid = Guid {
    data1: 0x770aae78,
    data2: 0xf26f,
    data3: 0x4dba,
    data4: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
};
const IID_IDXGI_FACTORY2: Guid = Guid {
    data1: 0x50c83a1c,
    data2: 0xe072,
    data3: 0x4c48,
    data4: [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0],
};
const IID_IDXGI_ADAPTER: Guid = Guid {
    data1: 0x2411e7e1,
    data2: 0x12ac,
    data3: 0x4ccf,
    data4: [0xbd, 0x14, 0x97, 0x98, 0xe8, 0x53, 0x4d, 0xc0],
};
const IID_IDXGI_SWAP_CHAIN: Guid = Guid {
    data1: 0x310d36a0,
    data2: 0xd2e7,
    data3: 0x4c0a,
    data4: [0xaa, 0x04, 0x6a, 0x9d, 0x23, 0xb8, 0x88, 0x6a],
};

// ────────────────────── DXGI_ADAPTER_DESC ──────────────────────

#[repr(C)]
#[derive(Clone)]
pub(crate) struct DxgiAdapterDesc {
    pub description: [u16; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: u64,
}

impl Default for DxgiAdapterDesc {
    fn default() -> Self {
        let mut desc = Self {
            description: [0u16; 128],
            vendor_id: 0x10de,
            device_id: 0x2204,
            sub_sys_id: 0,
            revision: 0,
            dedicated_video_memory: 2 * 1024 * 1024 * 1024,
            dedicated_system_memory: 0,
            shared_system_memory: 4 * 1024 * 1024 * 1024,
            adapter_luid: 0x0001_0000,
        };
        let name = "Winlux OpenGL Adapter";
        for (i, c) in name.encode_utf16().enumerate() {
            if i >= 127 {
                break;
            }
            desc.description[i] = c;
        }
        desc
    }
}

// ────────────────────── DXGI_SWAP_CHAIN_DESC ──────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct DxgiModeDesc {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_numerator: u32,
    pub refresh_rate_denominator: u32,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct DxgiSampleDesc {
    pub count: u32,
    pub quality: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct DxgiSwapChainDesc {
    pub buffer_desc: DxgiModeDesc,
    pub sample_desc: DxgiSampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub output_window: usize,
    pub windowed: i32,
    pub swap_effect: u32,
    pub flags: u32,
}

// ────────────────────── GL context ──────────────────────

/// Garde-fou : par défaut Xlib termine le processus sur une erreur X
/// (visuel incompatible, fenêtre détruite…). On la journalise à la place,
/// et l'appel GLX concerné échoue proprement.
pub(crate) fn install_x_error_guard() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        unsafe extern "C" fn on_error(_d: *mut c_void, ev: *mut u8) -> libc::c_int {
            // XErrorEvent : type, display*, resourceid, serial, error_code (u8)…
            let code = if ev.is_null() { 0 } else { *ev.add(32) };
            log_warn!("dxgi", "erreur X11 ignorée (code {})", code);
            0
        }
        let lib = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            return;
        }
        let f = libc::dlsym(lib, c"XSetErrorHandler".as_ptr());
        if !f.is_null() {
            type Handler = unsafe extern "C" fn(*mut c_void, *mut u8) -> libc::c_int;
            let set: unsafe extern "C" fn(Option<Handler>) -> *mut c_void = std::mem::transmute(f);
            set(Some(on_error));
        }
    });
}

pub(crate) struct GlContext {
    pub display: *mut c_void,
    pub context: *mut c_void,
    pub drawable: u64,
    pub width: u32,
    pub height: u32,
    pub back_texture: gl::GLuint,
    pub framebuffer: gl::GLuint,
}

unsafe impl Send for GlContext {}
unsafe impl Sync for GlContext {}

impl GlContext {
    pub fn create_for_window(hwnd: usize) -> Option<Self> {
        let glx = gl::glx()?;
        if !glx.available {
            log_error!("dxgi", "GLX non disponible");
            return None;
        }
        let g = gl::gl()?;

        unsafe {
            let display = (glx.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                log_error!("dxgi", "impossible d'ouvrir le display X11");
                return None;
            }

            let screen = (glx.XDefaultScreen)(display);

            install_x_error_guard();
            // Pas de canal alpha : le visuel doit correspondre à celui (24 bits)
            // des fenêtres Winlux, sinon glXMakeCurrent échoue (BadMatch).
            let attribs: [libc::c_int; 11] = [
                gl::GLX_RGBA,
                gl::GLX_DEPTH_SIZE,
                24,
                gl::GLX_DOUBLEBUFFER,
                gl::GLX_RED_SIZE,
                8,
                gl::GLX_GREEN_SIZE,
                8,
                gl::GLX_BLUE_SIZE,
                8,
                0, // None
            ];

            let vi = (glx.ChooseVisual)(display, screen, attribs.as_ptr());
            if vi.is_null() {
                log_error!("dxgi", "glXChooseVisual a échoué");
                (glx.XCloseDisplay)(display);
                return None;
            }

            let context = (glx.CreateContext)(display, vi, std::ptr::null_mut(), 1);
            if context.is_null() {
                log_error!("dxgi", "glXCreateContext a échoué");
                (glx.XCloseDisplay)(display);
                return None;
            }

            // Un HWND Winlux n'est pas un identifiant X : on demande la vraie
            // fenêtre au backend graphique (repli : valeur brute, puis racine).
            let drawable = if hwnd != 0 {
                crate::dlls::gui::native_window(hwnd).unwrap_or(hwnd as u64)
            } else {
                (glx.XDefaultRootWindow)(display) as u64
            };

            if (glx.MakeCurrent)(display, drawable, context) == 0 {
                log_error!("dxgi", "glXMakeCurrent a échoué");
                (glx.DestroyContext)(display, context);
                (glx.XCloseDisplay)(display);
                return None;
            }

            let version = (g.GetString)(gl::GL_VERSION);
            if !version.is_null() {
                let v = std::ffi::CStr::from_ptr(version as *const i8);
                log_info!("dxgi", "contexte OpenGL créé : {}", v.to_string_lossy());
            }

            let mut ctx = GlContext {
                display,
                context,
                drawable,
                width: 1280,
                height: 720,
                back_texture: 0,
                framebuffer: 0,
            };
            ctx.create_backbuffer(g);
            Some(ctx)
        }
    }

    unsafe fn create_backbuffer(&mut self, g: &gl::GlFunctions) {
        if self.framebuffer != 0 {
            (g.DeleteFramebuffers)(1, &self.framebuffer);
        }
        if self.back_texture != 0 {
            (g.DeleteTextures)(1, &self.back_texture);
        }

        (g.GenTextures)(1, &mut self.back_texture);
        (g.BindTexture)(gl::GL_TEXTURE_2D, self.back_texture);
        (g.TexImage2D)(
            gl::GL_TEXTURE_2D,
            0,
            gl::GL_RGBA8 as gl::GLint,
            self.width as i32,
            self.height as i32,
            0,
            gl::GL_RGBA,
            gl::GL_UNSIGNED_BYTE,
            std::ptr::null(),
        );
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MIN_FILTER, gl::GL_LINEAR);
        (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MAG_FILTER, gl::GL_LINEAR);

        (g.GenFramebuffers)(1, &mut self.framebuffer);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, self.framebuffer);
        (g.FramebufferTexture2D)(
            gl::GL_FRAMEBUFFER,
            gl::GL_COLOR_ATTACHMENT0,
            gl::GL_TEXTURE_2D,
            self.back_texture,
            0,
        );

        let status = (g.CheckFramebufferStatus)(gl::GL_FRAMEBUFFER);
        if status != gl::GL_FRAMEBUFFER_COMPLETE {
            log_warn!("dxgi", "framebuffer incomplet : 0x{:x}", status);
        }
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    }

    pub fn present(&self) {
        let Some(glx) = gl::glx() else { return };
        let Some(g) = gl::gl() else { return };
        unsafe {
            (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, self.framebuffer);
            (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, 0);
            (g.BlitFramebuffer)(
                0,
                0,
                self.width as i32,
                self.height as i32,
                0,
                0,
                self.width as i32,
                self.height as i32,
                gl::GL_COLOR_BUFFER_BIT,
                gl::GL_LINEAR as u32,
            );
            (glx.SwapBuffers)(self.display, self.drawable);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        if let Some(g) = gl::gl() {
            unsafe {
                self.create_backbuffer(g);
            }
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        let g = gl::gl();
        let glx = gl::glx();
        unsafe {
            if let Some(g) = g {
                if self.framebuffer != 0 {
                    (g.DeleteFramebuffers)(1, &self.framebuffer);
                }
                if self.back_texture != 0 {
                    (g.DeleteTextures)(1, &self.back_texture);
                }
            }
            if let Some(glx) = glx {
                if !self.context.is_null() {
                    (glx.MakeCurrent)(self.display, 0, std::ptr::null_mut());
                    (glx.DestroyContext)(self.display, self.context);
                }
                if !self.display.is_null() {
                    (glx.XCloseDisplay)(self.display);
                }
            }
        }
    }
}

// ────────────────────── IDXGIAdapter ──────────────────────

#[repr(C)]
struct DxgiAdapterVTable {
    query_interface:
        unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DxgiAdapter) -> u32,
    release: unsafe extern "win64" fn(*mut DxgiAdapter) -> u32,
    set_private_data:
        unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, *const c_void) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, *mut u32, *mut c_void) -> i32,
    get_parent: unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, *mut *mut c_void) -> i32,
    enum_outputs: unsafe extern "win64" fn(*mut DxgiAdapter, u32, *mut *mut c_void) -> i32,
    get_desc: unsafe extern "win64" fn(*mut DxgiAdapter, *mut DxgiAdapterDesc) -> i32,
    check_interface_support:
        unsafe extern "win64" fn(*mut DxgiAdapter, *const Guid, *mut i64) -> i32,
    // IDXGIAdapter1..3
    get_desc1: unsafe extern "win64" fn(*mut DxgiAdapter, *mut DxgiAdapterDesc1) -> i32,
    get_desc2: unsafe extern "win64" fn(*mut DxgiAdapter, *mut DxgiAdapterDesc1) -> i32,
    register_protection: unsafe extern "win64" fn() -> i32,
    unregister_protection: unsafe extern "win64" fn() -> i32,
    query_video_memory: unsafe extern "win64" fn(*mut DxgiAdapter, u32, u32, *mut [u64; 4]) -> i32,
    set_video_memory_reservation: unsafe extern "win64" fn() -> i32,
    register_budget: unsafe extern "win64" fn() -> i32,
    unregister_budget: unsafe extern "win64" fn() -> i32,
}

/// DXGI_ADAPTER_DESC1/2 : DESC + Flags (+ champs DESC2 laissés à zéro).
#[repr(C)]
pub(crate) struct DxgiAdapterDesc1 {
    pub desc: DxgiAdapterDesc,
    pub flags: u32,
}

#[repr(C)]
struct DxgiAdapter {
    vtable: *const DxgiAdapterVTable,
    ref_count: AtomicU32,
    desc: DxgiAdapterDesc,
    factory: *mut DxgiFactory,
}

impl DxgiAdapter {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let riid = &*riid;
        // IDXGIAdapter1 (29038f61), 2 (0aa1ae0a), 3 (645967a4), IDXGIObject.
        if *riid == IID_IUNKNOWN
            || *riid == IID_IDXGI_ADAPTER
            || [0x2903_8f61, 0x0aa1_ae0a, 0x6459_67a4, 0xaec2_2fb8].contains(&riid.data1)
        {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let _ = Box::from_raw(this);
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn adapter_stub3(
    _: *mut DxgiAdapter,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn adapter_stub2(
    _: *mut DxgiAdapter,
    _: *const Guid,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn adapter_get_priv(
    _: *mut DxgiAdapter,
    _: *const Guid,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn adapter_get_parent(
    this: *mut DxgiAdapter,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let factory = (*this).factory;
    if factory.is_null() {
        return E_FAIL;
    }
    DxgiFactory::query_interface(factory, riid, out)
}

unsafe extern "win64" fn adapter_enum_outputs(
    _: *mut DxgiAdapter,
    _: u32,
    _: *mut *mut c_void,
) -> i32 {
    DXGI_ERROR_NOT_FOUND
}

unsafe extern "win64" fn adapter_get_desc(
    this: *mut DxgiAdapter,
    out: *mut DxgiAdapterDesc,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write(out, (*this).desc.clone());
    S_OK
}

unsafe extern "win64" fn adapter_check_iface(
    _: *mut DxgiAdapter,
    _: *const Guid,
    _: *mut i64,
) -> i32 {
    S_OK
}

unsafe extern "win64" fn adapter_get_desc1(
    this: *mut DxgiAdapter,
    out: *mut DxgiAdapterDesc1,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write(
        out,
        DxgiAdapterDesc1 {
            desc: (*this).desc.clone(),
            flags: 0,
        },
    );
    S_OK
}
/// DESC2 ajoute 12 octets (granularités, zéro) : on les efface d'abord.
unsafe extern "win64" fn adapter_get_desc2(
    this: *mut DxgiAdapter,
    out: *mut DxgiAdapterDesc1,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(
        out as *mut u8,
        0,
        std::mem::size_of::<DxgiAdapterDesc1>() + 12,
    );
    adapter_get_desc1(this, out)
}
unsafe extern "win64" fn adapter_notimpl() -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn adapter_ok() -> i32 {
    S_OK
}
/// DXGI_QUERY_VIDEO_MEMORY_INFO : budget, usage, réservable, réservé.
unsafe extern "win64" fn adapter_query_memory(
    this: *mut DxgiAdapter,
    _node: u32,
    group: u32,
    out: *mut [u64; 4],
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let d = &(*this).desc;
    let total = if group == 0 {
        d.dedicated_video_memory
    } else {
        d.shared_system_memory
    } as u64;
    *out = [total, 0, total / 2, 0];
    S_OK
}

static ADAPTER_VTABLE: DxgiAdapterVTable = DxgiAdapterVTable {
    query_interface: DxgiAdapter::query_interface,
    add_ref: DxgiAdapter::add_ref,
    release: DxgiAdapter::release,
    set_private_data: adapter_stub3,
    set_private_data_interface: adapter_stub2,
    get_private_data: adapter_get_priv,
    get_parent: adapter_get_parent,
    enum_outputs: adapter_enum_outputs,
    get_desc: adapter_get_desc,
    check_interface_support: adapter_check_iface,
    get_desc1: adapter_get_desc1,
    get_desc2: adapter_get_desc2,
    register_protection: adapter_notimpl,
    unregister_protection: adapter_ok,
    query_video_memory: adapter_query_memory,
    set_video_memory_reservation: adapter_ok,
    register_budget: adapter_notimpl,
    unregister_budget: adapter_ok,
};

/// Description de l'adaptateur : le vrai GPU quand Vulkan le connaît.
fn adapter_desc() -> DxgiAdapterDesc {
    let mut desc = DxgiAdapterDesc::default();
    let caps = crate::dlls::gpu_caps::caps();
    let prefs = crate::dlls::gpu_caps::prefs();
    let dev = crate::dlls::gpu_caps::pick_vulkan_device(caps, &prefs).or_else(|| {
        caps.vulkan
            .as_ref()
            .and_then(|v| v.devices.iter().find(|d| d.is_hardware()).cloned())
    });
    if let Some(d) = dev {
        desc.description = [0; 128];
        for (i, c) in d.name.encode_utf16().take(127).enumerate() {
            desc.description[i] = c;
        }
        desc.vendor_id = d.vendor_id;
    }
    desc
}

fn create_adapter(factory: *mut DxgiFactory) -> *mut DxgiAdapter {
    Box::into_raw(Box::new(DxgiAdapter {
        vtable: &ADAPTER_VTABLE,
        ref_count: AtomicU32::new(1),
        desc: adapter_desc(),
        factory,
    }))
}

// ────────────────────── IDXGISwapChain ──────────────────────

#[repr(C)]
struct DxgiSwapChainVTable {
    query_interface:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DxgiSwapChain) -> u32,
    release: unsafe extern "win64" fn(*mut DxgiSwapChain) -> u32,
    set_private_data:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *const c_void) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *mut u32, *mut c_void) -> i32,
    get_parent: unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *mut *mut c_void) -> i32,
    get_device: unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *mut *mut c_void) -> i32,
    present: unsafe extern "win64" fn(*mut DxgiSwapChain, u32, u32) -> i32,
    get_buffer:
        unsafe extern "win64" fn(*mut DxgiSwapChain, u32, *const Guid, *mut *mut c_void) -> i32,
    set_fullscreen_state: unsafe extern "win64" fn(*mut DxgiSwapChain, i32, *mut c_void) -> i32,
    get_fullscreen_state:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *mut i32, *mut *mut c_void) -> i32,
    get_desc: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut DxgiSwapChainDesc) -> i32,
    resize_buffers: unsafe extern "win64" fn(*mut DxgiSwapChain, u32, u32, u32, u32, u32) -> i32,
    resize_target: unsafe extern "win64" fn(*mut DxgiSwapChain, *const DxgiModeDesc) -> i32,
    get_containing_output: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut *mut c_void) -> i32,
    get_frame_statistics: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut c_void) -> i32,
    get_last_present_count: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut u32) -> i32,
}

#[repr(C)]
pub(crate) struct DxgiSwapChain {
    vtable: *const DxgiSwapChainVTable,
    ref_count: AtomicU32,
    desc: DxgiSwapChainDesc,
    pub gl_ctx: Mutex<Option<GlContext>>,
    device: *mut c_void,
    present_count: AtomicU32,
}

unsafe impl Send for DxgiSwapChain {}
unsafe impl Sync for DxgiSwapChain {}

impl DxgiSwapChain {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let riid = &*riid;
        if *riid == IID_IUNKNOWN || *riid == IID_IDXGI_SWAP_CHAIN {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let _ = Box::from_raw(this);
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn sc_stub3(
    _: *mut DxgiSwapChain,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn sc_stub2(_: *mut DxgiSwapChain, _: *const Guid, _: *const c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn sc_get_priv(
    _: *mut DxgiSwapChain,
    _: *const Guid,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn sc_get_parent(
    _: *mut DxgiSwapChain,
    _: *const Guid,
    _: *mut *mut c_void,
) -> i32 {
    E_NOTIMPL
}

unsafe extern "win64" fn sc_get_device(
    this: *mut DxgiSwapChain,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let dev = (*this).device;
    if dev.is_null() {
        return E_FAIL;
    }
    let vtable = *(dev as *const *const usize);
    let qi: unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtable);
    qi(dev, riid, out)
}

unsafe extern "win64" fn sc_present(this: *mut DxgiSwapChain, _sync: u32, _flags: u32) -> i32 {
    let guard = (*this).gl_ctx.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref ctx) = *guard {
        ctx.present();
    }
    (*this).present_count.fetch_add(1, Ordering::Relaxed);
    S_OK
}

unsafe extern "win64" fn sc_get_buffer(
    this: *mut DxgiSwapChain,
    buffer: u32,
    _riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || buffer > 0 {
        return E_INVALIDARG;
    }
    let guard = (*this).gl_ctx.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref ctx) = *guard {
        let tex =
            crate::dlls::d3d11::create_backbuffer_texture(ctx.back_texture, ctx.width, ctx.height);
        *out = tex as *mut c_void;
        return S_OK;
    }
    E_FAIL
}

unsafe extern "win64" fn sc_set_fs(_: *mut DxgiSwapChain, _: i32, _: *mut c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn sc_get_fs(
    _: *mut DxgiSwapChain,
    fs: *mut i32,
    _: *mut *mut c_void,
) -> i32 {
    if !fs.is_null() {
        *fs = 0;
    }
    S_OK
}

unsafe extern "win64" fn sc_get_desc(this: *mut DxgiSwapChain, out: *mut DxgiSwapChainDesc) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write(out, (*this).desc);
    S_OK
}

unsafe extern "win64" fn sc_resize(
    this: *mut DxgiSwapChain,
    _count: u32,
    w: u32,
    h: u32,
    _fmt: u32,
    _flags: u32,
) -> i32 {
    // 0 = garder la taille courante (comme DXGI) ; au-delà de 16384 la
    // texture GL ne peut être créée : appel refusé plutôt qu'un tampon invalide.
    if w > MAX_BACKBUFFER_DIM || h > MAX_BACKBUFFER_DIM {
        return E_INVALIDARG;
    }
    let mut guard = (*this).gl_ctx.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut ctx) = *guard {
        let w = if w == 0 { ctx.width } else { w };
        let h = if h == 0 { ctx.height } else { h };
        ctx.resize(w, h);
        (*this).desc.buffer_desc.width = ctx.width;
        (*this).desc.buffer_desc.height = ctx.height;
    }
    S_OK
}

/// Dimension maximale du tampon arrière (limite des textures GL courantes).
const MAX_BACKBUFFER_DIM: u32 = 16384;

unsafe extern "win64" fn sc_resize_target(_: *mut DxgiSwapChain, _: *const DxgiModeDesc) -> i32 {
    S_OK
}
unsafe extern "win64" fn sc_get_output(_: *mut DxgiSwapChain, _: *mut *mut c_void) -> i32 {
    DXGI_ERROR_NOT_FOUND
}
unsafe extern "win64" fn sc_get_stats(_: *mut DxgiSwapChain, _: *mut c_void) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn sc_get_present_count(this: *mut DxgiSwapChain, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = (*this).present_count.load(Ordering::Relaxed);
    S_OK
}

static SWAP_CHAIN_VTABLE: DxgiSwapChainVTable = DxgiSwapChainVTable {
    query_interface: DxgiSwapChain::query_interface,
    add_ref: DxgiSwapChain::add_ref,
    release: DxgiSwapChain::release,
    set_private_data: sc_stub3,
    set_private_data_interface: sc_stub2,
    get_private_data: sc_get_priv,
    get_parent: sc_get_parent,
    get_device: sc_get_device,
    present: sc_present,
    get_buffer: sc_get_buffer,
    set_fullscreen_state: sc_set_fs,
    get_fullscreen_state: sc_get_fs,
    get_desc: sc_get_desc,
    resize_buffers: sc_resize,
    resize_target: sc_resize_target,
    get_containing_output: sc_get_output,
    get_frame_statistics: sc_get_stats,
    get_last_present_count: sc_get_present_count,
};

pub(crate) fn create_swap_chain(
    desc: &DxgiSwapChainDesc,
    device: *mut c_void,
) -> *mut DxgiSwapChain {
    let hwnd = desc.output_window;
    let gl_ctx = GlContext::create_for_window(hwnd);
    if gl_ctx.is_none() {
        log_warn!(
            "dxgi",
            "SwapChain créée sans contexte GL (hwnd=0x{:x})",
            hwnd
        );
    }
    Box::into_raw(Box::new(DxgiSwapChain {
        vtable: &SWAP_CHAIN_VTABLE,
        ref_count: AtomicU32::new(1),
        desc: *desc,
        gl_ctx: Mutex::new(gl_ctx),
        device,
        present_count: AtomicU32::new(0),
    }))
}

// ────────────────────── IDXGIFactory ──────────────────────

#[repr(C)]
struct DxgiFactoryVTable {
    query_interface:
        unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut DxgiFactory) -> u32,
    release: unsafe extern "win64" fn(*mut DxgiFactory) -> u32,
    set_private_data:
        unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, *const c_void) -> i32,
    get_private_data:
        unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, *mut u32, *mut c_void) -> i32,
    get_parent: unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, *mut *mut c_void) -> i32,
    enum_adapters: unsafe extern "win64" fn(*mut DxgiFactory, u32, *mut *mut c_void) -> i32,
    make_window_association: unsafe extern "win64" fn(*mut DxgiFactory, usize, u32) -> i32,
    get_window_association: unsafe extern "win64" fn(*mut DxgiFactory, *mut usize) -> i32,
    create_swap_chain: unsafe extern "win64" fn(
        *mut DxgiFactory,
        *mut c_void,
        *const DxgiSwapChainDesc,
        *mut *mut c_void,
    ) -> i32,
    create_software_adapter:
        unsafe extern "win64" fn(*mut DxgiFactory, usize, *mut *mut c_void) -> i32,
    enum_adapters1: unsafe extern "win64" fn(*mut DxgiFactory, u32, *mut *mut c_void) -> i32,
    is_current: unsafe extern "win64" fn(*mut DxgiFactory) -> i32,
    // IDXGIFactory2
    is_windowed_stereo: unsafe extern "win64" fn(*mut DxgiFactory) -> i32,
    create_sc_for_hwnd: unsafe extern "win64" fn(
        *mut DxgiFactory,
        *mut c_void,
        usize,
        *const crate::dlls::d3d12::SwapDesc1,
        *const c_void,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32,
    create_sc_for_core_window: unsafe extern "win64" fn() -> i32,
    get_shared_luid: unsafe extern "win64" fn(*mut DxgiFactory, usize, *mut u64) -> i32,
    register_stereo_window: unsafe extern "win64" fn() -> i32,
    register_stereo_event: unsafe extern "win64" fn() -> i32,
    unregister_stereo: unsafe extern "win64" fn() -> i32,
    register_occlusion_window: unsafe extern "win64" fn() -> i32,
    register_occlusion_event: unsafe extern "win64" fn() -> i32,
    unregister_occlusion: unsafe extern "win64" fn() -> i32,
    create_sc_for_composition: unsafe extern "win64" fn() -> i32,
    // IDXGIFactory3
    get_creation_flags: unsafe extern "win64" fn(*mut DxgiFactory) -> u32,
    // IDXGIFactory4
    enum_adapter_by_luid:
        unsafe extern "win64" fn(*mut DxgiFactory, u64, *const Guid, *mut *mut c_void) -> i32,
    enum_warp_adapter:
        unsafe extern "win64" fn(*mut DxgiFactory, *const Guid, *mut *mut c_void) -> i32,
    // IDXGIFactory5
    check_feature_support: unsafe extern "win64" fn(*mut DxgiFactory, u32, *mut u8, u32) -> i32,
    // IDXGIFactory6
    enum_adapter_by_pref:
        unsafe extern "win64" fn(*mut DxgiFactory, u32, u32, *const Guid, *mut *mut c_void) -> i32,
    // IDXGIFactory7
    register_adapters_changed: unsafe extern "win64" fn() -> i32,
    unregister_adapters_changed: unsafe extern "win64" fn() -> i32,
}

#[repr(C)]
struct DxgiFactory {
    vtable: *const DxgiFactoryVTable,
    ref_count: AtomicU32,
}

impl DxgiFactory {
    unsafe extern "win64" fn query_interface(
        this: *mut Self,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let riid = &*riid;
        // IDXGIFactory3..7 et IDXGIObject reconnus par leur premier mot.
        if *riid == IID_IUNKNOWN
            || *riid == IID_IDXGI_FACTORY
            || *riid == IID_IDXGI_FACTORY1
            || *riid == IID_IDXGI_FACTORY2
            || [
                0x2548_3823,
                0x1bc6_ea02,
                0x7632_e1f5,
                0xc1b6_694f,
                0xa496_6eed,
                0xaec2_2fb8,
            ]
            .contains(&riid.data1)
        {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            let _ = Box::from_raw(this);
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn fac_stub3(
    _: *mut DxgiFactory,
    _: *const Guid,
    _: u32,
    _: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn fac_stub2(_: *mut DxgiFactory, _: *const Guid, _: *const c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn fac_get_priv(
    _: *mut DxgiFactory,
    _: *const Guid,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn fac_get_parent(
    _: *mut DxgiFactory,
    _: *const Guid,
    _: *mut *mut c_void,
) -> i32 {
    E_NOINTERFACE
}

unsafe extern "win64" fn fac_enum_adapters(
    this: *mut DxgiFactory,
    index: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if index > 0 {
        *out = std::ptr::null_mut();
        return DXGI_ERROR_NOT_FOUND;
    }
    let adapter = create_adapter(this);
    *out = adapter as *mut c_void;
    log_debug!("dxgi", "EnumAdapters(0) → adaptateur Winlux OpenGL");
    S_OK
}

unsafe extern "win64" fn fac_make_assoc(_: *mut DxgiFactory, _: usize, _: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn fac_get_assoc(_: *mut DxgiFactory, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    S_OK
}

unsafe extern "win64" fn fac_create_sc(
    _: *mut DxgiFactory,
    device: *mut c_void,
    desc: *const DxgiSwapChainDesc,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if crate::dlls::d3d12::is_queue(device) {
        let d = &*desc;
        let d1 = crate::dlls::d3d12::SwapDesc1 {
            width: d.buffer_desc.width,
            height: d.buffer_desc.height,
            format: d.buffer_desc.format,
            stereo: 0,
            sample: d.sample_desc,
            usage: d.buffer_usage,
            count: d.buffer_count,
            scaling: 0,
            effect: d.swap_effect,
            alpha: 0,
            flags: d.flags,
        };
        return match crate::dlls::d3d12::create_swap_chain(device, d.output_window, &d1) {
            Ok(p) => {
                *out = p;
                S_OK
            }
            Err(e) => e,
        };
    }
    let sc = create_swap_chain(&*desc, device);
    *out = sc as *mut c_void;
    log_info!(
        "dxgi",
        "SwapChain créée : {}x{}",
        (*desc).buffer_desc.width,
        (*desc).buffer_desc.height
    );
    S_OK
}

unsafe extern "win64" fn fac_create_soft(
    _: *mut DxgiFactory,
    _: usize,
    _: *mut *mut c_void,
) -> i32 {
    DXGI_ERROR_NOT_FOUND
}

unsafe extern "win64" fn fac_enum1(this: *mut DxgiFactory, idx: u32, out: *mut *mut c_void) -> i32 {
    fac_enum_adapters(this, idx, out)
}
unsafe extern "win64" fn fac_is_current(_: *mut DxgiFactory) -> i32 {
    1
}

unsafe extern "win64" fn fac_zero(_: *mut DxgiFactory) -> i32 {
    0
}
unsafe extern "win64" fn fac_notimpl() -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn fac_ok() -> i32 {
    S_OK
}

unsafe extern "win64" fn fac_create_sc_hwnd(
    this: *mut DxgiFactory,
    device: *mut c_void,
    hwnd: usize,
    desc: *const crate::dlls::d3d12::SwapDesc1,
    _fs: *const c_void,
    _output: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() || out.is_null() || device.is_null() {
        return E_INVALIDARG;
    }
    let d = &*desc;
    if crate::dlls::d3d12::is_queue(device) {
        return match crate::dlls::d3d12::create_swap_chain(device, hwnd, d) {
            Ok(p) => {
                *out = p;
                S_OK
            }
            Err(e) => e,
        };
    }
    // Direct3D 11 : même chemin que CreateSwapChain.
    let legacy = DxgiSwapChainDesc {
        buffer_desc: DxgiModeDesc {
            width: d.width,
            height: d.height,
            refresh_rate_numerator: 60,
            refresh_rate_denominator: 1,
            format: d.format,
            scanline_ordering: 0,
            scaling: 0,
        },
        sample_desc: d.sample,
        buffer_usage: d.usage,
        buffer_count: d.count,
        output_window: hwnd,
        windowed: 1,
        swap_effect: d.effect,
        flags: d.flags,
    };
    fac_create_sc(this, device, &legacy, out)
}

unsafe extern "win64" fn fac_shared_luid(_: *mut DxgiFactory, _h: usize, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = DxgiAdapterDesc::default().adapter_luid;
    S_OK
}

unsafe extern "win64" fn fac_flags(_: *mut DxgiFactory) -> u32 {
    0
}

unsafe extern "win64" fn fac_adapter_qi(
    this: *mut DxgiFactory,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let a = create_adapter(this);
    let hr = DxgiAdapter::query_interface(a, riid, out);
    DxgiAdapter::release(a);
    hr
}

unsafe extern "win64" fn fac_by_luid(
    this: *mut DxgiFactory,
    _luid: u64,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    fac_adapter_qi(this, riid, out)
}

/// Pas d'adaptateur WARP séparé : l'adaptateur Winlux sert aussi de repli.
unsafe extern "win64" fn fac_warp(
    this: *mut DxgiFactory,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    fac_adapter_qi(this, riid, out)
}

/// DXGI_FEATURE_PRESENT_ALLOW_TEARING (0) : non, sans risque.
unsafe extern "win64" fn fac_check_feature(
    _: *mut DxgiFactory,
    _f: u32,
    data: *mut u8,
    size: u32,
) -> i32 {
    if data.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(data, 0, size as usize);
    S_OK
}

unsafe extern "win64" fn fac_by_pref(
    this: *mut DxgiFactory,
    index: u32,
    _pref: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if index > 0 {
        *out = std::ptr::null_mut();
        return DXGI_ERROR_NOT_FOUND;
    }
    fac_adapter_qi(this, riid, out)
}

/// Fabrique remise par `IDXGISwapChain::GetParent` (D3D12).
pub(crate) unsafe fn factory_for(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let f = create_factory();
    let hr = DxgiFactory::query_interface(f, riid, out);
    DxgiFactory::release(f);
    hr
}

static FACTORY_VTABLE: DxgiFactoryVTable = DxgiFactoryVTable {
    query_interface: DxgiFactory::query_interface,
    add_ref: DxgiFactory::add_ref,
    release: DxgiFactory::release,
    set_private_data: fac_stub3,
    set_private_data_interface: fac_stub2,
    get_private_data: fac_get_priv,
    get_parent: fac_get_parent,
    enum_adapters: fac_enum_adapters,
    make_window_association: fac_make_assoc,
    get_window_association: fac_get_assoc,
    create_swap_chain: fac_create_sc,
    create_software_adapter: fac_create_soft,
    enum_adapters1: fac_enum1,
    is_current: fac_is_current,
    is_windowed_stereo: fac_zero,
    create_sc_for_hwnd: fac_create_sc_hwnd,
    create_sc_for_core_window: fac_notimpl,
    get_shared_luid: fac_shared_luid,
    register_stereo_window: fac_notimpl,
    register_stereo_event: fac_notimpl,
    unregister_stereo: fac_ok,
    register_occlusion_window: fac_notimpl,
    register_occlusion_event: fac_notimpl,
    unregister_occlusion: fac_ok,
    create_sc_for_composition: fac_notimpl,
    get_creation_flags: fac_flags,
    enum_adapter_by_luid: fac_by_luid,
    enum_warp_adapter: fac_warp,
    check_feature_support: fac_check_feature,
    enum_adapter_by_pref: fac_by_pref,
    register_adapters_changed: fac_notimpl,
    unregister_adapters_changed: fac_ok,
};

fn create_factory() -> *mut DxgiFactory {
    Box::into_raw(Box::new(DxgiFactory {
        vtable: &FACTORY_VTABLE,
        ref_count: AtomicU32::new(1),
    }))
}

// ────────────────────── Entry points ──────────────────────

pub unsafe extern "win64" fn CreateDXGIFactory(_riid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = create_factory() as usize;
    log_info!("dxgi", "CreateDXGIFactory → factory Winlux OpenGL");
    S_OK
}

pub unsafe extern "win64" fn CreateDXGIFactory1(_riid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = create_factory() as usize;
    log_info!("dxgi", "CreateDXGIFactory1 → factory Winlux OpenGL");
    S_OK
}

pub unsafe extern "win64" fn CreateDXGIFactory2(
    _flags: u32,
    _riid: *const u8,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = create_factory() as usize;
    log_info!("dxgi", "CreateDXGIFactory2 → factory Winlux OpenGL");
    S_OK
}

pub unsafe extern "win64" fn DXGIGetDebugInterface1(
    _flags: u32,
    _riid: *const u8,
    _out: *mut usize,
) -> i32 {
    E_NOINTERFACE
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateDXGIFactory" => fn CreateDXGIFactory,
        "CreateDXGIFactory1" => fn CreateDXGIFactory1,
        "CreateDXGIFactory2" => fn CreateDXGIFactory2,
        "DXGIGetDebugInterface1" => fn DXGIGetDebugInterface1,
    }
}
