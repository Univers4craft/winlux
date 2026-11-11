//! Implémentation DXGI → OpenGL/GLX pour Winlux.
//! Traduit les interfaces DXGI (Factory, Adapter, Output, SwapChain)
//! vers un backend OpenGL via GLX sur X11.
#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use crate::dlls::gl_bindings as gl;
use crate::dlls::ole32::Guid;
use crate::{log_debug, log_error, log_info, log_warn};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const DXGI_ERROR_NOT_FOUND: i32 = 0x887A_0002u32 as i32;
const DXGI_ERROR_INVALID_CALL: i32 = 0x887a_0001u32 as i32;

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
/// IDXGISwapChain1 (DXGI 1.2), demandé par ANGLE et Chromium (#1244).
const IID_IDXGI_SWAP_CHAIN1: Guid = Guid {
    data1: 0x790a45f7,
    data2: 0x0d42,
    data3: 0x4876,
    data4: [0x98, 0x3a, 0x0a, 0x55, 0xcf, 0xe6, 0xf4, 0xaa],
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
/// et l'appel GLX, Xft ou cairo concerné échoue proprement.
pub(crate) fn install_x_error_guard() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        unsafe extern "C" fn on_error(_d: *mut c_void, ev: *mut u8) -> libc::c_int {
            // XErrorEvent : type, display*, resourceid, serial, error_code (u8)…
            if ev.is_null() {
                return 0;
            }
            // XErrorEvent : resourceid à 16, error_code, request_code et
            // minor_code à 32, 33 et 34.
            let resource = std::ptr::read_unaligned(ev.add(16) as *const u64);
            log_warn!(
                "x11",
                "erreur X11 ignorée (code {}, requête {}.{}, ressource {:#x})",
                *ev.add(32),
                *ev.add(33),
                *ev.add(34),
                resource
            );
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

/// Connexions X des contextes GL détruits, gardées ouvertes et resservies.
/// Fermer la connexion détruit l'écran GLX de Mesa qui lui est attaché, et
/// avec lui les ressources des textures créées sous ce contexte. Or ces
/// textures sont partagées : un cadre d'une vue de rendu du périphérique
/// peut encore tenir le tampon arrière d'une chaîne disparue. Sa
/// suppression appelait alors une fonction de l'écran détruit (adresse 0)
/// et le processus GPU de Chromium mourait (#1565).
static SPARE_DISPLAYS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

unsafe fn open_display(glx: &gl::GlxFunctions) -> *mut gl::XDisplay {
    let spare = SPARE_DISPLAYS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pop();
    match spare {
        Some(display) => display as *mut gl::XDisplay,
        None => (glx.XOpenDisplay)(std::ptr::null()),
    }
}

fn keep_display(display: *mut gl::XDisplay) {
    if !display.is_null() {
        SPARE_DISPLAYS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(display as usize);
    }
}

/// Contexte GLX de la première chaîne d'échange : les contextes de travail
/// des autres threads partagent ses objets (shaders, tampons, textures).
static MAIN_CONTEXT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static WORKER_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Sous Windows, les méthodes de création d'ID3D11Device s'appellent depuis
/// n'importe quel thread (ANGLE crée ses shaders dans ses threads de travail).
/// Ici ces objets vivent dans OpenGL : un thread sans contexte courant reçoit
/// un contexte GLX partagé avec le contexte principal, rendu courant sur la
/// fenêtre racine (il ne sert qu'à créer des objets, jamais à présenter).
/// Contexte GL hors écran pour un périphérique créé sans chaîne d'échange
/// (rendu dans des textures, D3D11CreateDevice seul) ; sans effet s'il en
/// existe déjà un ou sans affichage X.
pub(crate) fn ensure_offscreen_context() {
    static DONE: std::sync::Once = std::sync::Once::new();
    if MAIN_CONTEXT.load(Ordering::Acquire) != 0 || std::env::var_os("DISPLAY").is_none() {
        return;
    }
    DONE.call_once(|| {
        if let Some(ctx) = GlContext::create_for_window(0) {
            // Vit autant que le processus, comme le périphérique.
            std::mem::forget(ctx);
        }
    });
}

/// Contexte GLX courant du thread (0 si aucun).
pub(crate) struct Binding {
    pub display: usize,
    pub drawable: u64,
    pub context: usize,
}

pub(crate) fn current_binding() -> Binding {
    type Getter = Option<unsafe extern "C" fn() -> usize>;
    static GETTERS: std::sync::OnceLock<[Getter; 3]> = std::sync::OnceLock::new();
    let getters = GETTERS.get_or_init(|| {
        let mut out: [Getter; 3] = [None; 3];
        let Some(glx) = gl::glx() else { return out };
        if !glx.available {
            return out;
        }
        let names = [
            c"glXGetCurrentContext",
            c"glXGetCurrentDrawable",
            c"glXGetCurrentDisplay",
        ];
        for (slot, name) in out.iter_mut().zip(names) {
            let f = unsafe { (glx.GetProcAddressARB)(name.as_ptr().cast()) };
            if !f.is_null() {
                *slot = Some(unsafe {
                    std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> usize>(f)
                });
            }
        }
        out
    });
    let call = |i: usize| getters[i].map_or(0, |f| unsafe { f() });
    Binding {
        context: call(0),
        drawable: call(1) as u64,
        display: call(2),
    }
}

pub(crate) fn ensure_thread_context() {
    let main = MAIN_CONTEXT.load(Ordering::Acquire);
    if main == 0 || WORKER_CONTEXT.with(|w| w.get()) {
        return;
    }
    let Some(glx) = gl::glx() else { return };
    if !glx.available {
        return;
    }
    unsafe {
        let f = (glx.GetProcAddressARB)(c"glXGetCurrentContext".as_ptr().cast());
        if !f.is_null() {
            let current: unsafe extern "C" fn() -> gl::GLXContext = std::mem::transmute(f);
            if !current().is_null() {
                return;
            }
        }
        let display = (glx.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return;
        }
        let screen = (glx.XDefaultScreen)(display);
        let vi = (glx.ChooseVisual)(display, screen, VISUAL_ATTRIBS.as_ptr());
        if vi.is_null() {
            (glx.XCloseDisplay)(display);
            return;
        }
        let context = (glx.CreateContext)(display, vi, main as gl::GLXContext, 1);
        if context.is_null() {
            log_warn!("dxgi", "contexte GL de travail partagé impossible");
            (glx.XCloseDisplay)(display);
            return;
        }
        let root = (glx.XDefaultRootWindow)(display) as u64;
        if (glx.MakeCurrent)(display, root, context) == 0 {
            (glx.DestroyContext)(display, context);
            (glx.XCloseDisplay)(display);
            return;
        }
        WORKER_CONTEXT.with(|w| w.set(true));
        log_debug!("dxgi", "contexte GL partagé créé pour un thread de travail");
    }
}

// Pas de canal alpha : le visuel doit correspondre à celui (24 bits)
// des fenêtres Winlux, sinon glXMakeCurrent échoue (BadMatch).
const VISUAL_ATTRIBS: [libc::c_int; 11] = [
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

pub(crate) struct GlContext {
    pub display: *mut c_void,
    pub context: *mut c_void,
    pub drawable: u64,
    pub width: u32,
    pub height: u32,
    pub back_texture: gl::GLuint,
    pub framebuffer: gl::GLuint,
    /// Présentations faites : la première est décrite dans le journal.
    presented: AtomicU32,
    /// Taille de la fenêtre X à la dernière présentation (largeur << 32 |
    /// hauteur), pour voir qu'elle a changé (#1542).
    window_size: AtomicU64,
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
            let display = open_display(glx);
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
                keep_display(display);
                return None;
            }

            // Partagé avec le premier contexte (périphérique créé avant la
            // chaîne d'échange) : ses tampons et textures restent visibles.
            let share = MAIN_CONTEXT.load(Ordering::Acquire) as gl::GLXContext;
            let context = (glx.CreateContext)(display, vi, share, 1);
            if context.is_null() {
                log_error!("dxgi", "glXCreateContext a échoué");
                keep_display(display);
                return None;
            }

            // Un HWND Winlux n'est pas un identifiant X : on demande la vraie
            // fenêtre au backend graphique (repli : valeur brute, puis racine).
            let drawable = if hwnd != 0 {
                crate::dlls::gui::native_window(hwnd).unwrap_or(hwnd as u64)
            } else {
                (glx.XDefaultRootWindow)(display)
            };
            if hwnd != 0 {
                crate::dlls::gui::mark_direct3d_window(drawable);
            }

            if (glx.MakeCurrent)(display, drawable, context) == 0 {
                log_error!("dxgi", "glXMakeCurrent a échoué");
                (glx.DestroyContext)(display, context);
                keep_display(display);
                return None;
            }

            let _ = MAIN_CONTEXT.compare_exchange(
                0,
                context as usize,
                Ordering::AcqRel,
                Ordering::Acquire,
            );

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
                presented: AtomicU32::new(0),
                window_size: AtomicU64::new(0),
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
            // Le contexte de la chaîne doit être courant sur SA fenêtre : un
            // thread qui présente avec un autre contexte (hors écran, de
            // travail) copierait l'image dans la fenêtre racine, et
            // glXSwapBuffers montrerait un tampon jamais dessiné (#509).
            let previous = current_binding();
            let switched =
                previous.context != self.context as usize || previous.drawable != self.drawable;
            if switched && (glx.MakeCurrent)(self.display, self.drawable, self.context) == 0 {
                if self.presented.fetch_add(1, Ordering::Relaxed) == 0 {
                    log_warn!(
                        "dxgi",
                        "Present : contexte de la chaîne d'échange indisponible sur ce thread"
                    );
                }
                return;
            }
            crate::dlls::d3d11::FRAMES.fetch_add(1, Ordering::Relaxed);
            let first = self.presented.fetch_add(1, Ordering::Relaxed) == 0;
            if first {
                self.describe(g, &previous, switched);
            }
            // Taille réelle de la fenêtre : elle a pu changer (agrandie,
            // restaurée) avant que le programme ne redimensionne sa chaîne.
            let (dw, dh) = self.drawable_size(glx).unwrap_or((self.width, self.height));
            let packed = (u64::from(dw) << 32) | u64::from(dh);
            let changed = self.window_size.swap(packed, Ordering::Relaxed) != packed;
            // Taille changée : Mesa ne relit celle de la fenêtre qu'après un
            // échange. La première image serait placée et coupée d'après
            // l'ancienne taille (contenu décalé d'autant, bande noire), et
            // restait ainsi tant que la page ne redessinait pas (#1542) :
            // l'image est présentée une seconde fois, à la bonne taille.
            for _ in 0..if changed { 2 } else { 1 } {
                (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, self.framebuffer);
                (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, 0);
                // Le rendu D3D11 garde la ligne 0 en haut (voir
                // d3d11_pipeline.rs) : l'image est retournée en la copiant.
                // Elle couvre toute la fenêtre, comme DXGI_SCALING_STRETCH :
                // jamais d'ancienne image ni de noir autour.
                (g.BlitFramebuffer)(
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    0,
                    dh as i32,
                    dw as i32,
                    0,
                    gl::GL_COLOR_BUFFER_BIT,
                    gl::GL_LINEAR as u32,
                );
                (glx.SwapBuffers)(self.display, self.drawable);
            }
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
            // Rendre au thread le contexte qu'il avait (le sien n'est pas
            // lié ailleurs, la reprise réussit).
            if switched && previous.context != 0 {
                (glx.MakeCurrent)(
                    previous.display as *mut _,
                    previous.drawable,
                    previous.context as gl::GLXContext,
                );
            }
        }
    }

    /// Taille de la fenêtre X où l'on présente (XGetGeometry).
    unsafe fn drawable_size(&self, glx: &gl::GlxFunctions) -> Option<(u32, u32)> {
        let get = glx.XGetGeometry?;
        let (mut root, mut x, mut y) = (0, 0, 0);
        let (mut w, mut h, mut border, mut depth) = (0, 0, 0, 0);
        let ok = get(
            self.display.cast(),
            self.drawable as libc::c_ulong,
            &mut root,
            &mut x,
            &mut y,
            &mut w,
            &mut h,
            &mut border,
            &mut depth,
        );
        (ok != 0 && w > 0 && h > 0).then_some((w, h))
    }

    /// Décrit la première présentation : taille, fenêtre, contexte et un
    /// pixel du tampon arrière, pour diagnostiquer une fenêtre noire.
    unsafe fn describe(&self, g: &gl::GlFunctions, previous: &Binding, switched: bool) {
        let mut px = [0u8; 4];
        (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, self.framebuffer);
        (g.ReadPixels)(
            (self.width / 2) as i32,
            (self.height / 2) as i32,
            1,
            1,
            gl::GL_RGBA,
            gl::GL_UNSIGNED_BYTE,
            px.as_mut_ptr().cast(),
        );
        (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, 0);
        log_info!(
            "dxgi",
            "première présentation : {}x{} vers la fenêtre X 0x{:x}, contexte {} (thread : contexte 0x{:x}, surface 0x{:x}), pixel central {:?}",
            self.width,
            self.height,
            self.drawable,
            if switched { "rendu courant pour l'occasion" } else { "déjà courant" },
            previous.context,
            previous.drawable,
            px
        );
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        let Some(g) = gl::gl() else { return };
        unsafe {
            // Les framebuffers ne sont pas partagés entre contextes GL. Celui
            // du tampon arrière doit naître dans le contexte de la chaîne,
            // où Present le relit. ResizeBuffers arrive sur un thread où le
            // contexte courant est celui du périphérique : le framebuffer y
            // était créé, Present lisait un nom inconnu de son contexte et
            // l'image ne suivait plus la fenêtre. La suppression de l'ancien
            // nom touchait en plus un framebuffer du périphérique (#1565).
            let restore = self.make_own_context_current();
            self.create_backbuffer(g);
            self.restore_binding(restore);
        }
    }

    /// Rend le contexte de la chaîne courant sur ce thread, s'il ne l'est
    /// pas déjà. Renvoie la liaison à rétablir ensuite (`None` : rien à
    /// rétablir). En cas d'échec (contexte courant sur un autre thread),
    /// le contexte du thread reste en place.
    unsafe fn make_own_context_current(&self) -> Option<Binding> {
        let glx = gl::glx()?;
        let previous = current_binding();
        if previous.context == self.context as usize && previous.drawable == self.drawable {
            return None;
        }
        if (glx.MakeCurrent)(self.display, self.drawable, self.context) == 0 {
            log_warn!(
                "dxgi",
                "contexte de la chaîne d'échange indisponible sur ce thread pour ResizeBuffers"
            );
            return None;
        }
        Some(previous)
    }

    unsafe fn restore_binding(&self, previous: Option<Binding>) {
        let (Some(glx), Some(previous)) = (gl::glx(), previous) else {
            return;
        };
        if previous.context != 0 {
            (glx.MakeCurrent)(
                previous.display as *mut _,
                previous.drawable,
                previous.context as gl::GLXContext,
            );
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        let g = gl::gl();
        let glx = gl::glx();
        unsafe {
            if let Some(g) = g {
                // Le framebuffer appartient au contexte de la chaîne (#1565).
                let restore = self.make_own_context_current();
                if self.framebuffer != 0 {
                    (g.DeleteFramebuffers)(1, &self.framebuffer);
                }
                if self.back_texture != 0 {
                    (g.DeleteTextures)(1, &self.back_texture);
                }
                self.restore_binding(restore);
            }
            if let Some(glx) = glx {
                if !self.context.is_null() {
                    let _ = MAIN_CONTEXT.compare_exchange(
                        self.context as usize,
                        0,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    // Ne détacher que notre contexte : celui du thread (le
                    // périphérique) reste courant.
                    if current_binding().context == self.context as usize {
                        (glx.MakeCurrent)(self.display, 0, std::ptr::null_mut());
                    }
                    (glx.DestroyContext)(self.display, self.context);
                }
                keep_display(self.display);
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
    // IDXGIAdapter4
    get_desc3: unsafe extern "win64" fn(*mut DxgiAdapter, *mut DxgiAdapterDesc1) -> i32,
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
        // IDXGIAdapter1 (29038f61), 2 (0aa1ae0a), 3 (645967a4), 4 (3c8d99d1),
        // IDXGIObject.
        if *riid == IID_IUNKNOWN
            || *riid == IID_IDXGI_ADAPTER
            || [
                0x2903_8f61,
                0x0aa1_ae0a,
                0x6459_67a4,
                0x3c8d_99d1,
                0xaec2_2fb8,
            ]
            .contains(&riid.data1)
        {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        crate::dlls::comobj::log_no_interface("dxgi (adaptateur)", riid);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            crate::dlls::dxgi_private::forget(this as *const c_void);
            // L'adaptateur tient une référence sur sa fabrique (create_adapter).
            let adapter = Box::from_raw(this);
            if !adapter.factory.is_null() {
                DxgiFactory::release(adapter.factory);
            }
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn adapter_stub3(
    this: *mut DxgiAdapter,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn adapter_stub2(
    this: *mut DxgiAdapter,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
}
unsafe extern "win64" fn adapter_get_priv(
    this: *mut DxgiAdapter,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
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
/// DESC2 (et DESC3) ajoutent les deux granularités de préemption après
/// Flags : 316 octets, 320 avec l'alignement. Elles restent à zéro.
const DXGI_ADAPTER_DESC2_SIZE: usize = 320;

unsafe extern "win64" fn adapter_get_desc2(
    this: *mut DxgiAdapter,
    out: *mut DxgiAdapterDesc1,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(out as *mut u8, 0, DXGI_ADAPTER_DESC2_SIZE);
    adapter_get_desc1(this, out)
}
unsafe extern "win64" fn adapter_notimpl() -> i32 {
    crate::dlls::d2d1::unsupported_call("IDXGIAdapter (méthode non prise en charge)")
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
    // DXGI_ADAPTER_DESC3 a la même disposition que DESC2.
    get_desc3: adapter_get_desc2,
};

/// Description de l'adaptateur : le vrai GPU quand Vulkan le connaît.
pub(crate) fn adapter_desc() -> DxgiAdapterDesc {
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

/// Adaptateur (et sa fabrique) pour IDXGIDevice::GetAdapter de d3d11.
pub(crate) fn new_default_adapter() -> *mut c_void {
    let factory = create_factory();
    let adapter = create_adapter(factory);
    // L'adaptateur garde sa propre référence : on rend celle de création.
    unsafe { DxgiFactory::release(factory) };
    adapter.cast()
}

pub(crate) unsafe fn adapter_query_interface(
    adapter: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    unsafe { DxgiAdapter::query_interface(adapter.cast(), riid, out) }
}

pub(crate) unsafe fn adapter_release(adapter: *mut c_void) -> u32 {
    unsafe { DxgiAdapter::release(adapter.cast()) }
}

/// Crée un adaptateur qui garde sa fabrique en vie : ANGLE relâche la
/// fabrique juste après EnumAdapters puis remonte à elle par GetParent.
fn create_adapter(factory: *mut DxgiFactory) -> *mut DxgiAdapter {
    if !factory.is_null() {
        unsafe { (*factory).ref_count.fetch_add(1, Ordering::Relaxed) };
    }
    Box::into_raw(Box::new(DxgiAdapter {
        vtable: crate::dlls::dx11_x86::abi(&ADAPTER_VTABLE, "dlls::dxgi", "DxgiAdapterVTable"),
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
    get_frame_statistics: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut FrameStatistics) -> i32,
    get_last_present_count: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut u32) -> i32,
    // IDXGISwapChain1 (#1244)
    get_desc1:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *mut crate::dlls::d3d12::SwapDesc1) -> i32,
    get_fullscreen_desc: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut FullscreenDesc) -> i32,
    get_hwnd: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut usize) -> i32,
    get_core_window:
        unsafe extern "win64" fn(*mut DxgiSwapChain, *const Guid, *mut *mut c_void) -> i32,
    present1: unsafe extern "win64" fn(*mut DxgiSwapChain, u32, u32, *const c_void) -> i32,
    is_temporary_mono_supported: unsafe extern "win64" fn(*mut DxgiSwapChain) -> i32,
    get_restrict_to_output: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut *mut c_void) -> i32,
    set_background_color: unsafe extern "win64" fn(*mut DxgiSwapChain, *const [f32; 4]) -> i32,
    get_background_color: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut [f32; 4]) -> i32,
    set_rotation: unsafe extern "win64" fn(*mut DxgiSwapChain, u32) -> i32,
    get_rotation: unsafe extern "win64" fn(*mut DxgiSwapChain, *mut u32) -> i32,
}

/// DXGI_SWAP_CHAIN_FULLSCREEN_DESC (20 octets, sans pointeur).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct FullscreenDesc {
    refresh_numerator: u32,
    refresh_denominator: u32,
    scanline_ordering: u32,
    scaling: u32,
    windowed: i32,
}

/// DXGI_FRAME_STATISTICS (32 octets).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct FrameStatistics {
    present_count: u32,
    present_refresh_count: u32,
    sync_refresh_count: u32,
    sync_qpc_time: i64,
    sync_gpu_time: i64,
}

#[repr(C)]
pub(crate) struct DxgiSwapChain {
    vtable: *const DxgiSwapChainVTable,
    ref_count: AtomicU32,
    desc: DxgiSwapChainDesc,
    pub gl_ctx: Mutex<Option<GlContext>>,
    device: *mut c_void,
    present_count: AtomicU32,
    /// Champs de DXGI_SWAP_CHAIN_DESC1 absents de DXGI_SWAP_CHAIN_DESC.
    scaling: u32,
    alpha: u32,
    background: Mutex<[f32; 4]>,
    rotation: AtomicU32,
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
        if *riid == IID_IUNKNOWN || *riid == IID_IDXGI_SWAP_CHAIN || *riid == IID_IDXGI_SWAP_CHAIN1
        {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            return S_OK;
        }
        crate::dlls::comobj::log_no_interface("dxgi (chaîne d'échange)", riid);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            crate::dlls::dxgi_private::forget(this as *const c_void);
            let _ = Box::from_raw(this);
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn sc_stub3(
    this: *mut DxgiSwapChain,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn sc_stub2(
    this: *mut DxgiSwapChain,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
}
unsafe extern "win64" fn sc_get_priv(
    this: *mut DxgiSwapChain,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn sc_get_parent(
    _: *mut DxgiSwapChain,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    // Parent d'une chaîne d'échange : la fabrique DXGI.
    factory_for(riid, out)
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
    let qi: unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(crate::dlls::legacy_com::method(dev as usize, 0));
    qi(dev, riid, out)
}

/// DXGI_PRESENT_TEST : vérifier sans présenter.
const DXGI_PRESENT_TEST: u32 = 0x1;

unsafe extern "win64" fn sc_present(this: *mut DxgiSwapChain, _sync: u32, flags: u32) -> i32 {
    if flags & DXGI_PRESENT_TEST != 0 {
        return S_OK;
    }
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
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || buffer > 0 {
        return E_INVALIDARG;
    }
    let guard = (*this).gl_ctx.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref ctx) = *guard {
        let tex =
            crate::dlls::d3d11::create_backbuffer_texture(ctx.back_texture, ctx.width, ctx.height);
        // Direct3D 10 (#1090) : l'enveloppe ID3D10Texture2D du tampon.
        if riid.as_ref() == Some(&crate::dlls::d3d10::IID_TEXTURE2D) {
            *out = crate::dlls::d3d10::wrap_backbuffer(tex as usize, (*this).device as usize)
                as *mut c_void;
            return S_OK;
        }
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
/// Statistiques de présentation : nombre de Present et instant courant,
/// sur une cadence de 60 Hz (#1244).
unsafe extern "win64" fn sc_get_stats(this: *mut DxgiSwapChain, out: *mut FrameStatistics) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let (mut now, mut freq) = (0i64, 0i64);
    crate::dlls::kernel32::query_performance_counter(&mut now);
    crate::dlls::kernel32::query_performance_frequency(&mut freq);
    let count = (*this).present_count.load(Ordering::Relaxed);
    let refresh = if freq > 0 {
        (now as i128 * 60 / freq as i128) as u32
    } else {
        count
    };
    *out = FrameStatistics {
        present_count: count,
        present_refresh_count: refresh,
        sync_refresh_count: refresh,
        sync_qpc_time: now,
        sync_gpu_time: 0,
    };
    S_OK
}

// ── IDXGISwapChain1 (#1244) ──

unsafe extern "win64" fn sc_get_desc1(
    this: *mut DxgiSwapChain,
    out: *mut crate::dlls::d3d12::SwapDesc1,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let d = &(*this).desc;
    *out = crate::dlls::d3d12::SwapDesc1 {
        width: d.buffer_desc.width,
        height: d.buffer_desc.height,
        format: d.buffer_desc.format,
        stereo: 0,
        sample: d.sample_desc,
        usage: d.buffer_usage,
        count: d.buffer_count,
        scaling: (*this).scaling,
        effect: d.swap_effect,
        alpha: (*this).alpha,
        flags: d.flags,
    };
    S_OK
}
unsafe extern "win64" fn sc_get_fullscreen_desc(
    this: *mut DxgiSwapChain,
    out: *mut FullscreenDesc,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let m = &(*this).desc.buffer_desc;
    *out = FullscreenDesc {
        refresh_numerator: m.refresh_rate_numerator,
        refresh_denominator: m.refresh_rate_denominator,
        scanline_ordering: m.scanline_ordering,
        scaling: m.scaling,
        windowed: 1,
    };
    S_OK
}
unsafe extern "win64" fn sc_get_hwnd(this: *mut DxgiSwapChain, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = (*this).desc.output_window;
    S_OK
}
unsafe extern "win64" fn sc_get_core_window(
    _: *mut DxgiSwapChain,
    _: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    // Chaîne d'échange d'une fenêtre HWND : pas de CoreWindow.
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DXGI_ERROR_INVALID_CALL
}
/// Present1 : les rectangles modifiés et le défilement de
/// DXGI_PRESENT_PARAMETERS sont des indications ; le tampon entier est
/// présenté, ce qui les respecte toujours.
unsafe extern "win64" fn sc_present1(
    this: *mut DxgiSwapChain,
    sync: u32,
    flags: u32,
    _params: *const c_void,
) -> i32 {
    sc_present(this, sync, flags)
}
unsafe extern "win64" fn sc_temporary_mono(_: *mut DxgiSwapChain) -> i32 {
    0
}
unsafe extern "win64" fn sc_restrict_output(_: *mut DxgiSwapChain, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    S_OK
}
unsafe extern "win64" fn sc_set_background(this: *mut DxgiSwapChain, c: *const [f32; 4]) -> i32 {
    if c.is_null() {
        return E_INVALIDARG;
    }
    *(*this).background.lock().unwrap_or_else(|e| e.into_inner()) = *c;
    S_OK
}
unsafe extern "win64" fn sc_get_background(this: *mut DxgiSwapChain, c: *mut [f32; 4]) -> i32 {
    if c.is_null() {
        return E_INVALIDARG;
    }
    *c = *(*this).background.lock().unwrap_or_else(|e| e.into_inner());
    S_OK
}
unsafe extern "win64" fn sc_set_rotation(this: *mut DxgiSwapChain, r: u32) -> i32 {
    // DXGI_MODE_ROTATION_UNSPECIFIED (0) à ROTATE270 (4).
    if r > 4 {
        return E_INVALIDARG;
    }
    (*this).rotation.store(r, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn sc_get_rotation(this: *mut DxgiSwapChain, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = (*this).rotation.load(Ordering::Relaxed);
    S_OK
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
    get_desc1: sc_get_desc1,
    get_fullscreen_desc: sc_get_fullscreen_desc,
    get_hwnd: sc_get_hwnd,
    get_core_window: sc_get_core_window,
    present1: sc_present1,
    is_temporary_mono_supported: sc_temporary_mono,
    get_restrict_to_output: sc_restrict_output,
    set_background_color: sc_set_background,
    get_background_color: sc_get_background,
    set_rotation: sc_set_rotation,
    get_rotation: sc_get_rotation,
};

pub(crate) fn create_swap_chain(
    desc: &DxgiSwapChainDesc,
    device: *mut c_void,
) -> *mut DxgiSwapChain {
    let hwnd = desc.output_window;
    let mut gl_ctx = GlContext::create_for_window(hwnd);
    let mut desc = *desc;
    if let Some(ctx) = gl_ctx.as_mut() {
        // Taille demandée, ou 0 = zone client de la fenêtre (règle DXGI).
        // Le tampon arrière était toujours créé en 1280x720 (#509).
        let client = crate::dlls::gui::window_rect(hwnd)
            .or_else(|| crate::dlls::gui::foreign_client_rect(hwnd))
            .map(|r| {
                (
                    (r.right - r.left).max(0) as u32,
                    (r.bottom - r.top).max(0) as u32,
                )
            });
        let pick = |asked: u32, window: Option<u32>| match asked {
            0 => window.filter(|&v| v > 0).unwrap_or(0),
            v => v.min(MAX_BACKBUFFER_DIM),
        };
        let w = pick(desc.buffer_desc.width, client.map(|c| c.0));
        let h = pick(desc.buffer_desc.height, client.map(|c| c.1));
        if w > 0 && h > 0 && (w, h) != (ctx.width, ctx.height) {
            ctx.resize(w, h);
        }
        desc.buffer_desc.width = ctx.width;
        desc.buffer_desc.height = ctx.height;
    }
    if gl_ctx.is_none() {
        log_warn!(
            "dxgi",
            "SwapChain créée sans contexte GL (hwnd=0x{:x})",
            hwnd
        );
    }
    Box::into_raw(Box::new(DxgiSwapChain {
        vtable: crate::dlls::dx11_x86::abi(&SWAP_CHAIN_VTABLE, "dlls::dxgi", "DxgiSwapChainVTable"),
        ref_count: AtomicU32::new(1),
        desc,
        gl_ctx: Mutex::new(gl_ctx),
        device,
        present_count: AtomicU32::new(0),
        scaling: 0,
        alpha: 0,
        background: Mutex::new([0.0; 4]),
        rotation: AtomicU32::new(0),
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
        crate::dlls::comobj::log_no_interface("dxgi (fabrique)", riid);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    unsafe extern "win64" fn add_ref(this: *mut Self) -> u32 {
        (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "win64" fn release(this: *mut Self) -> u32 {
        let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            crate::dlls::dxgi_private::forget(this as *const c_void);
            let _ = Box::from_raw(this);
            return 0;
        }
        prev - 1
    }
}

unsafe extern "win64" fn fac_stub3(
    this: *mut DxgiFactory,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set(this as *const c_void, g, size, data) }
}
unsafe extern "win64" fn fac_stub2(
    this: *mut DxgiFactory,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface) }
}
unsafe extern "win64" fn fac_get_priv(
    this: *mut DxgiFactory,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    unsafe { crate::dlls::dxgi_private::get(this as *const c_void, g, size, data) }
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
/// Méthode de fabrique absente, nommée au journal pour savoir laquelle
/// écrire ensuite (#1470).
macro_rules! fac_notimpl {
    ($f:ident, $name:literal) => {
        unsafe extern "win64" fn $f() -> i32 {
            crate::dlls::d2d1::unsupported_call(concat!("IDXGIFactory::", $name))
        }
    };
}
fac_notimpl!(
    fac_create_sc_for_core_window,
    "CreateSwapChainForCoreWindow"
);
fac_notimpl!(fac_register_stereo_window, "RegisterStereoStatusWindow");
fac_notimpl!(fac_register_stereo_event, "RegisterStereoStatusEvent");
fac_notimpl!(
    fac_register_occlusion_window,
    "RegisterOcclusionStatusWindow"
);
fac_notimpl!(fac_register_occlusion_event, "RegisterOcclusionStatusEvent");
fac_notimpl!(
    fac_create_sc_for_composition,
    "CreateSwapChainForComposition"
);
fac_notimpl!(
    fac_register_adapters_changed,
    "RegisterAdaptersChangedEvent"
);
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
    let hr = fac_create_sc(this, device, &legacy, out);
    // DXGI_SWAP_CHAIN_DESC1 garde l'échelle et le mode alpha, que GetDesc1
    // rend tels quels (#1244).
    if hr == S_OK && !(*out).is_null() {
        let sc = *out as *mut DxgiSwapChain;
        (*sc).scaling = d.scaling;
        (*sc).alpha = d.alpha;
    }
    hr
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
    create_sc_for_core_window: fac_create_sc_for_core_window,
    get_shared_luid: fac_shared_luid,
    register_stereo_window: fac_register_stereo_window,
    register_stereo_event: fac_register_stereo_event,
    unregister_stereo: fac_ok,
    register_occlusion_window: fac_register_occlusion_window,
    register_occlusion_event: fac_register_occlusion_event,
    unregister_occlusion: fac_ok,
    create_sc_for_composition: fac_create_sc_for_composition,
    get_creation_flags: fac_flags,
    enum_adapter_by_luid: fac_by_luid,
    enum_warp_adapter: fac_warp,
    check_feature_support: fac_check_feature,
    enum_adapter_by_pref: fac_by_pref,
    register_adapters_changed: fac_register_adapters_changed,
    unregister_adapters_changed: fac_ok,
};

fn create_factory() -> *mut DxgiFactory {
    Box::into_raw(Box::new(DxgiFactory {
        vtable: crate::dlls::dx11_x86::abi(&FACTORY_VTABLE, "dlls::dxgi", "DxgiFactoryVTable"),
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

/// `DXGIDeclareAdapterRemovalSupport` : l'application sait gérer le retrait
/// d'un adaptateur ; rien à préparer ici.
#[allow(non_snake_case)]
pub extern "win64" fn DXGIDeclareAdapterRemovalSupport() -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DXGIDeclareAdapterRemovalSupport" => fn DXGIDeclareAdapterRemovalSupport,
        "CreateDXGIFactory" => fn CreateDXGIFactory,
        "CreateDXGIFactory1" => fn CreateDXGIFactory1,
        "CreateDXGIFactory2" => fn CreateDXGIFactory2,
        "DXGIGetDebugInterface1" => fn DXGIGetDebugInterface1,
    }
}

#[cfg(test)]
mod refcount_tests {
    use super::*;

    #[test]
    fn adapter_keeps_its_factory_alive() {
        // ANGLE : CreateDXGIFactory1 → EnumAdapters → Release(fabrique)
        // → adaptateur.GetParent(IDXGIFactory1) doit encore réussir.
        unsafe {
            let f = create_factory();
            let a = create_adapter(f);
            assert_eq!(DxgiFactory::release(f), 1);
            let mut parent = std::ptr::null_mut();
            assert_eq!(
                adapter_get_parent(a, &IID_IDXGI_FACTORY1, &mut parent),
                S_OK
            );
            assert_eq!(parent, f.cast());
            assert_eq!(DxgiFactory::release(parent.cast()), 1);
            assert_eq!(DxgiAdapter::release(a), 0);
        }
    }
}

#[cfg(test)]
mod adapter_desc_tests {
    #[test]
    fn adapter_desc_sizes_match_windows() {
        // DXGI_ADAPTER_DESC 304 octets, DESC1 312 ; DESC2/3 = 320.
        assert_eq!(std::mem::size_of::<super::DxgiAdapterDesc>(), 304);
        assert_eq!(std::mem::size_of::<super::DxgiAdapterDesc1>(), 312);
        assert_eq!(super::DXGI_ADAPTER_DESC2_SIZE, 320);
    }
}

#[cfg(test)]
mod swap_chain1_tests {
    use super::*;

    /// Méthode `i` de la table de l'objet COM `p`.
    unsafe fn slot(p: *mut c_void, i: usize) -> usize {
        *(*(p as *const *const usize)).add(i)
    }

    #[test]
    fn chaine_d_echange_direct3d11_en_idxgiswapchain1() {
        unsafe {
            let f = create_factory();
            // Appareil fictif : seul le test « file Direct3D 12 ? » le lit.
            let mut fake_device = [0usize; 4];
            let desc = crate::dlls::d3d12::SwapDesc1 {
                width: 64,
                height: 32,
                format: 87,
                stereo: 0,
                sample: DxgiSampleDesc {
                    count: 1,
                    quality: 0,
                },
                usage: 0x20,
                count: 2,
                scaling: 1,
                effect: 3,
                alpha: 0,
                flags: 0,
            };
            let mut sc = std::ptr::null_mut();
            assert_eq!(
                fac_create_sc_hwnd(
                    f,
                    fake_device.as_mut_ptr().cast(),
                    0x4_0000,
                    &desc,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut sc,
                ),
                S_OK
            );
            let mut sc1 = std::ptr::null_mut();
            assert_eq!(
                DxgiSwapChain::query_interface(sc.cast(), &IID_IDXGI_SWAP_CHAIN1, &mut sc1),
                S_OK,
                "IDXGISwapChain1 accepté"
            );
            assert_eq!(sc1, sc);
            // GetDesc1 (18) rend l'échelle et l'effet demandés.
            let get_desc1: unsafe extern "win64" fn(
                *mut c_void,
                *mut crate::dlls::d3d12::SwapDesc1,
            ) -> i32 = std::mem::transmute(slot(sc1, 18));
            let mut d1: crate::dlls::d3d12::SwapDesc1 = std::mem::zeroed();
            assert_eq!(get_desc1(sc1, &mut d1), S_OK);
            assert_eq!((d1.format, d1.scaling, d1.effect, d1.count), (87, 1, 3, 2));
            // GetHwnd (20).
            let get_hwnd: unsafe extern "win64" fn(*mut c_void, *mut usize) -> i32 =
                std::mem::transmute(slot(sc1, 20));
            let mut h = 0usize;
            assert_eq!(get_hwnd(sc1, &mut h), S_OK);
            assert_eq!(h, 0x4_0000);
            // Present1 (22) compte comme Present ; DXGI_PRESENT_TEST ne compte pas.
            let present1: unsafe extern "win64" fn(*mut c_void, u32, u32, *const c_void) -> i32 =
                std::mem::transmute(slot(sc1, 22));
            assert_eq!(present1(sc1, 1, 0, std::ptr::null()), S_OK);
            assert_eq!(present1(sc1, 0, DXGI_PRESENT_TEST, std::ptr::null()), S_OK);
            // GetFrameStatistics (16) rend le nombre de présentations.
            let stats: unsafe extern "win64" fn(*mut c_void, *mut FrameStatistics) -> i32 =
                std::mem::transmute(slot(sc1, 16));
            let mut st = FrameStatistics::default();
            assert_eq!(stats(sc1, &mut st), S_OK);
            assert_eq!(st.present_count, 1);
            assert!(st.sync_qpc_time > 0);
            // SetRotation (27) / GetRotation (28).
            let set_rot: unsafe extern "win64" fn(*mut c_void, u32) -> i32 =
                std::mem::transmute(slot(sc1, 27));
            let get_rot: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(slot(sc1, 28));
            assert_eq!(set_rot(sc1, 1), S_OK);
            let mut r = 0;
            assert_eq!(get_rot(sc1, &mut r), S_OK);
            assert_eq!(r, 1);
            assert_eq!(DxgiSwapChain::release(sc1.cast()), 1);
            assert_eq!(DxgiSwapChain::release(sc.cast()), 0);
            DxgiFactory::release(f);
        }
    }

    #[test]
    fn tables_dxgi_aux_tailles_de_windows() {
        assert_eq!(std::mem::size_of::<DxgiSwapChainVTable>() / 8, 29);
        assert_eq!(std::mem::size_of::<FrameStatistics>(), 32);
        assert_eq!(std::mem::size_of::<FullscreenDesc>(), 20);
        assert_eq!(std::mem::size_of::<crate::dlls::d3d12::SwapDesc1>(), 48);
    }
}

#[cfg(test)]
mod presentation_x11_tests {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, ImageFormat,
    };
    use x11rb::wrapper::ConnectionExt as _;

    /// #1542 : la fenêtre change de taille avant la chaîne d'échange
    /// (agrandie par le gestionnaire). L'image présentée couvre toute la
    /// fenêtre, à sa nouvelle taille : ni noir, ni ancienne image, ni
    /// décalage. Il faut un serveur X avec GLX (Xvfb + Mesa).
    #[test]
    #[ignore = "requires an X11 server with GLX"]
    fn image_presentee_couvre_la_fenetre_redimensionnee_x11() {
        let (conn, screen) = x11rb::connect(None).expect("X11");
        let root = conn.setup().roots[screen].clone();
        let win = conn.generate_id().unwrap();
        conn.create_window(
            root.root_depth,
            win,
            root.root,
            0,
            0,
            200,
            100,
            0,
            x11rb::protocol::xproto::WindowClass::INPUT_OUTPUT,
            root.root_visual,
            &CreateWindowAux::new().background_pixel(root.black_pixel),
        )
        .unwrap();
        conn.map_window(win).unwrap();
        conn.sync().unwrap();
        let glx = gl::glx().expect("GLX");
        let g = gl::gl().expect("GL");
        let mut ctx = GlContext::create_for_window(0).expect("contexte GL");
        ctx.drawable = u64::from(win);
        let pixel = |x: i16, y: i16| -> u32 {
            let img = conn
                .get_image(ImageFormat::Z_PIXMAP, win, x, y, 1, 1, !0)
                .unwrap()
                .reply()
                .unwrap();
            u32::from_le_bytes([img.data[0], img.data[1], img.data[2], 0])
        };
        unsafe {
            assert_ne!((glx.MakeCurrent)(ctx.display, ctx.drawable, ctx.context), 0);
            ctx.resize(200, 100);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, ctx.framebuffer);
            (g.ClearColor)(1.0, 0.0, 0.0, 1.0);
            (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
            ctx.present();
            (g.Finish)();
            assert_eq!(pixel(10, 10), 0xFF0000);
            // Agrandie : la chaîne garde 200x100 pour l'instant.
            conn.configure_window(win, &ConfigureWindowAux::new().width(400).height(300))
                .unwrap();
            conn.sync().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            ctx.present();
            (g.Finish)();
            for (x, y) in [(10, 10), (390, 10), (10, 290), (390, 290)] {
                assert_eq!(pixel(x, y), 0xFF0000, "pixel ({x},{y})");
            }
        }
        conn.destroy_window(win).unwrap();
        conn.sync().unwrap();
    }

    /// #1565 : le processus GPU de Chromium mourait (appel à 0 dans Mesa)
    /// en libérant une vue de rendu après la fin d'une chaîne d'échange :
    /// le cadre de la vue tenait encore le tampon arrière de la chaîne.
    #[test]
    #[ignore = "requires an X11 server with GLX"]
    fn vue_liberee_apres_la_fin_de_sa_chaine_x11() {
        let glx = gl::glx().expect("GLX");
        let g = gl::gl().expect("GL");
        unsafe {
            let device = GlContext::create_for_window(0).expect("contexte du périphérique");
            let chain = GlContext::create_for_window(0).expect("contexte de la chaîne");
            assert_ne!(
                (glx.MakeCurrent)(device.display, device.drawable, device.context),
                0
            );
            // Vue de rendu sur le tampon arrière, dans le contexte du périphérique.
            let mut fb = 0;
            (g.GenFramebuffers)(1, &mut fb);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fb);
            (g.FramebufferTexture2D)(
                gl::GL_FRAMEBUFFER,
                gl::GL_COLOR_ATTACHMENT0,
                gl::GL_TEXTURE_2D,
                chain.back_texture,
                0,
            );
            (g.ClearColor)(0.0, 0.0, 1.0, 1.0);
            (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
            (g.Finish)();
            drop(chain);
            assert_eq!(current_binding().context, device.context as usize);
            (g.DeleteFramebuffers)(1, &fb);
            (g.Finish)();
        }
    }

    /// #1565 : ResizeBuffers arrive avec le contexte du périphérique courant
    /// sur le thread. Le nouveau tampon arrière, rempli par ce contexte, doit
    /// quand même être présenté par le contexte de la chaîne.
    #[test]
    #[ignore = "requires an X11 server with GLX"]
    fn tampon_redimensionne_sous_un_autre_contexte_reste_presente_x11() {
        let (conn, screen) = x11rb::connect(None).expect("X11");
        let root = conn.setup().roots[screen].clone();
        let win = conn.generate_id().unwrap();
        conn.create_window(
            root.root_depth,
            win,
            root.root,
            0,
            0,
            200,
            100,
            0,
            x11rb::protocol::xproto::WindowClass::INPUT_OUTPUT,
            root.root_visual,
            &CreateWindowAux::new().background_pixel(root.black_pixel),
        )
        .unwrap();
        conn.map_window(win).unwrap();
        conn.sync().unwrap();
        let glx = gl::glx().expect("GLX");
        let g = gl::gl().expect("GL");
        let pixel = |x: i16, y: i16| -> u32 {
            let img = conn
                .get_image(ImageFormat::Z_PIXMAP, win, x, y, 1, 1, !0)
                .unwrap()
                .reply()
                .unwrap();
            u32::from_le_bytes([img.data[0], img.data[1], img.data[2], 0])
        };
        // Remplit la texture `tex` d'une couleur, avec le contexte courant
        // et un framebuffer à lui (comme une vue de rendu D3D11).
        let fill = |tex: gl::GLuint, r: f32, gr: f32| unsafe {
            let mut fb = 0;
            (g.GenFramebuffers)(1, &mut fb);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fb);
            (g.FramebufferTexture2D)(
                gl::GL_FRAMEBUFFER,
                gl::GL_COLOR_ATTACHMENT0,
                gl::GL_TEXTURE_2D,
                tex,
                0,
            );
            (g.ClearColor)(r, gr, 0.0, 1.0);
            (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
            (g.DeleteFramebuffers)(1, &fb);
            (g.Finish)();
        };
        unsafe {
            let mut chain = GlContext::create_for_window(0).expect("contexte de la chaîne");
            chain.drawable = u64::from(win);
            assert_ne!(
                (glx.MakeCurrent)(chain.display, chain.drawable, chain.context),
                0
            );
            chain.resize(200, 100);
            // Le contexte du périphérique, partagé, courant sur le thread.
            let device = GlContext::create_for_window(0).expect("contexte du périphérique");
            assert_ne!(
                (glx.MakeCurrent)(device.display, device.drawable, device.context),
                0
            );
            fill(chain.back_texture, 1.0, 0.0);
            chain.present();
            (g.Finish)();
            assert_eq!(pixel(10, 10), 0xFF0000, "avant ResizeBuffers");
            assert_eq!(current_binding().context, device.context as usize);
            // ResizeBuffers sous le contexte du périphérique.
            chain.resize(400, 300);
            assert_eq!(
                current_binding().context,
                device.context as usize,
                "le contexte du thread est rétabli"
            );
            fill(chain.back_texture, 0.0, 1.0);
            chain.present();
            (g.Finish)();
            chain.present();
            (g.Finish)();
            for (x, y) in [(10, 10), (190, 10), (10, 90), (190, 90)] {
                assert_eq!(pixel(x, y), 0x00FF00, "pixel ({x},{y}) après ResizeBuffers");
            }
            drop(chain);
            assert_eq!(
                current_binding().context,
                device.context as usize,
                "la fin de la chaîne laisse le contexte du thread"
            );
        }
        conn.destroy_window(win).unwrap();
        conn.sync().unwrap();
    }
}
