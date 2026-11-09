//! opengl32.dll : OpenGL pour les programmes Windows, sur la libGL de l'hôte
//! (Mesa ou pilote propriétaire), avec les contextes WGL reliés à GLX sur la
//! fenêtre X11 du HWND.
//!
//! - Les 336 fonctions d'OpenGL 1.0/1.1 sont exportées directement, comme
//!   dans la vraie opengl32.dll ; les autres passent par `wglGetProcAddress`.
//!   Toutes traversent le pont générique de [`super::hostcall`].
//! - Les formats de pixel sont une petite table fixe (RGBA 8 bits, avec ou
//!   sans profondeur, stencil, double tampon, multiéchantillonnage) traduite
//!   en FBConfig GLX compatible avec le visuel de la fenêtre.
//! - Extensions WGL : ARB_create_context(+profile), ARB_pixel_format,
//!   ARB_multisample, EXT_swap_control, ARB/EXT_extensions_string,
//!   ARB_framebuffer_sRGB.
//!
//! Les fonctions `ChoosePixelFormat`, `DescribePixelFormat`,
//! `SetPixelFormat`, `GetPixelFormat` et `SwapBuffers` de gdi32 sont servies
//! ici aussi ([`gdi32_exports`]).

#![allow(non_snake_case)]

use super::hostcall::{self, Resolver};
use super::{lock, Exports};
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

// ---------- Table des formats de pixel ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub double: bool,
    pub depth: u8,
    pub stencil: u8,
    pub samples: u8,
}

/// Formats proposés (index Windows = position + 1). Le premier est celui
/// que presque tous les programmes demandent.
pub const FORMATS: [Format; 9] = [
    Format {
        double: true,
        depth: 24,
        stencil: 8,
        samples: 0,
    },
    Format {
        double: true,
        depth: 24,
        stencil: 0,
        samples: 0,
    },
    Format {
        double: true,
        depth: 16,
        stencil: 0,
        samples: 0,
    },
    Format {
        double: true,
        depth: 0,
        stencil: 0,
        samples: 0,
    },
    Format {
        double: false,
        depth: 24,
        stencil: 8,
        samples: 0,
    },
    Format {
        double: false,
        depth: 0,
        stencil: 0,
        samples: 0,
    },
    Format {
        double: true,
        depth: 24,
        stencil: 8,
        samples: 2,
    },
    Format {
        double: true,
        depth: 24,
        stencil: 8,
        samples: 4,
    },
    Format {
        double: true,
        depth: 24,
        stencil: 8,
        samples: 8,
    },
];

fn format_at(i: i32) -> Option<Format> {
    (i >= 1)
        .then(|| FORMATS.get(i as usize - 1).copied())
        .flatten()
}

const PFD_DOUBLEBUFFER: u32 = 0x1;
#[cfg(test)]
const PFD_STEREO: u32 = 0x2;
const PFD_DRAW_TO_WINDOW: u32 = 0x4;
const PFD_SUPPORT_OPENGL: u32 = 0x20;
const PFD_SUPPORT_COMPOSITION: u32 = 0x8000;
const PFD_DEPTH_DONTCARE: u32 = 0x2000_0000;
const PFD_DOUBLEBUFFER_DONTCARE: u32 = 0x4000_0000;

/// Remplit un PIXELFORMATDESCRIPTOR (40 octets).
pub fn describe(f: Format, out: &mut [u8; 40]) {
    *out = [0; 40];
    out[0..2].copy_from_slice(&40u16.to_le_bytes());
    out[2..4].copy_from_slice(&1u16.to_le_bytes());
    let mut flags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_SUPPORT_COMPOSITION;
    if f.double {
        flags |= PFD_DOUBLEBUFFER;
    }
    out[4..8].copy_from_slice(&flags.to_le_bytes());
    out[8] = 0; // PFD_TYPE_RGBA
    out[9] = 32; // cColorBits
    out[10] = 8; // rouge
    out[11] = 16;
    out[12] = 8; // vert
    out[13] = 8;
    out[14] = 8; // bleu
    out[15] = 0;
    out[16] = 8; // alpha
    out[17] = 24;
    out[23] = f.depth;
    out[24] = f.stencil;
}

/// ChoosePixelFormat : le format le plus proche de la demande.
pub fn choose(pfd: &[u8; 40]) -> i32 {
    let flags = u32::from_le_bytes([pfd[4], pfd[5], pfd[6], pfd[7]]);
    let want_double = flags & PFD_DOUBLEBUFFER != 0;
    let dc_double = flags & PFD_DOUBLEBUFFER_DONTCARE != 0;
    let dc_depth = flags & PFD_DEPTH_DONTCARE != 0;
    let (depth, stencil) = (pfd[23] as i32, pfd[24] as i32);
    let mut best = (i32::MAX, 1);
    for (i, f) in FORMATS.iter().enumerate() {
        let mut score = 0;
        if !dc_double && f.double != want_double {
            score += 10_000;
        }
        if !dc_depth {
            let d = f.depth as i32;
            score += if d < depth { 1000 } else { d - depth };
        }
        let s = f.stencil as i32;
        score += if s < stencil { 1000 } else { s - stencil };
        score += f.samples as i32 * 50;
        if score < best.0 {
            best = (score, i as i32 + 1);
        }
    }
    best.1
}

// Attributs WGL_ARB_pixel_format.
const WGL_NUMBER_PIXEL_FORMATS: i32 = 0x2000;
const WGL_DRAW_TO_WINDOW: i32 = 0x2001;
const WGL_DRAW_TO_BITMAP: i32 = 0x2002;
const WGL_ACCELERATION: i32 = 0x2003;
const WGL_SWAP_METHOD: i32 = 0x2007;
const WGL_SUPPORT_GDI: i32 = 0x200F;
const WGL_SUPPORT_OPENGL: i32 = 0x2010;
const WGL_DOUBLE_BUFFER: i32 = 0x2011;
const WGL_STEREO: i32 = 0x2012;
const WGL_PIXEL_TYPE: i32 = 0x2013;
const WGL_COLOR_BITS: i32 = 0x2014;
const WGL_DEPTH_BITS: i32 = 0x2022;
const WGL_STENCIL_BITS: i32 = 0x2023;
const WGL_SAMPLE_BUFFERS: i32 = 0x2041;
const WGL_SAMPLES: i32 = 0x2042;
const WGL_FRAMEBUFFER_SRGB_CAPABLE: i32 = 0x20A9;
const WGL_NO_ACCELERATION: i32 = 0x2025;
const WGL_FULL_ACCELERATION: i32 = 0x2027;
const WGL_SWAP_EXCHANGE: i32 = 0x2028;
const WGL_TYPE_RGBA: i32 = 0x202B;

/// Valeur d'un attribut WGL pour un format.
pub fn attrib(f: Format, a: i32) -> i32 {
    match a {
        WGL_NUMBER_PIXEL_FORMATS => FORMATS.len() as i32,
        WGL_DRAW_TO_WINDOW | WGL_SUPPORT_OPENGL | WGL_FRAMEBUFFER_SRGB_CAPABLE => 1,
        WGL_ACCELERATION => WGL_FULL_ACCELERATION,
        WGL_SWAP_METHOD => WGL_SWAP_EXCHANGE,
        WGL_DOUBLE_BUFFER => f.double as i32,
        WGL_PIXEL_TYPE => WGL_TYPE_RGBA,
        WGL_COLOR_BITS => 32,
        0x2015 | 0x2017 | 0x2019 | 0x201B => 8, // bits rouge, vert, bleu, alpha
        0x2016 => 16,
        0x2018 => 8,
        0x201C => 24,
        WGL_DEPTH_BITS => f.depth as i32,
        WGL_STENCIL_BITS => f.stencil as i32,
        WGL_SAMPLE_BUFFERS => (f.samples > 0) as i32,
        WGL_SAMPLES => f.samples as i32,
        _ => 0,
    }
}

/// wglChoosePixelFormatARB : formats compatibles, du plus proche au moins proche.
pub fn choose_arb(attribs: &[(i32, i32)]) -> Vec<i32> {
    let mut found: Vec<(i32, i32)> = Vec::new();
    'formats: for (i, f) in FORMATS.iter().enumerate() {
        let mut score = 0;
        for &(a, v) in attribs {
            let have = attrib(*f, a);
            let ok = match a {
                WGL_DRAW_TO_BITMAP | WGL_STEREO | WGL_SUPPORT_GDI => v == 0,
                WGL_ACCELERATION => v != WGL_NO_ACCELERATION,
                WGL_PIXEL_TYPE => v == WGL_TYPE_RGBA,
                WGL_DRAW_TO_WINDOW | WGL_SUPPORT_OPENGL | WGL_DOUBLE_BUFFER => have == v,
                WGL_SWAP_METHOD => true,
                0x2014..=0x2024
                | WGL_SAMPLE_BUFFERS
                | WGL_SAMPLES
                | WGL_FRAMEBUFFER_SRGB_CAPABLE => {
                    score += have - v;
                    have >= v
                }
                _ => true,
            };
            if !ok {
                continue 'formats;
            }
        }
        // À égalité, le moins de multiéchantillonnage.
        found.push((score * 16 + f.samples as i32, i as i32 + 1));
    }
    found.sort();
    found.into_iter().map(|(_, i)| i).collect()
}

// ---------- GLX ----------

const GLX_DOUBLEBUFFER: i32 = 5;
const GLX_RED_SIZE: i32 = 8;
const GLX_GREEN_SIZE: i32 = 9;
const GLX_BLUE_SIZE: i32 = 10;
const GLX_DEPTH_SIZE: i32 = 12;
const GLX_STENCIL_SIZE: i32 = 13;
const GLX_VISUAL_ID: i32 = 0x800B;
const GLX_DRAWABLE_TYPE: i32 = 0x8010;
const GLX_RENDER_TYPE: i32 = 0x8011;
const GLX_X_RENDERABLE: i32 = 0x8012;
const GLX_RGBA_TYPE: i32 = 0x8014;
const GLX_SAMPLE_BUFFERS: i32 = 100_000;
const GLX_SAMPLES: i32 = 100_001;

struct Glx {
    dpy: usize,
    choose_fbconfig: unsafe extern "C" fn(usize, i32, *const i32, *mut i32) -> *mut usize,
    get_fbconfig_attrib: unsafe extern "C" fn(usize, usize, i32, *mut i32) -> i32,
    create_new_context: unsafe extern "C" fn(usize, usize, i32, usize, i32) -> usize,
    create_context_attribs:
        Option<unsafe extern "C" fn(usize, usize, usize, i32, *const i32) -> usize>,
    make_current: unsafe extern "C" fn(usize, u64, usize) -> i32,
    destroy_context: unsafe extern "C" fn(usize, usize),
    swap_buffers: unsafe extern "C" fn(usize, u64),
    swap_interval_ext: Option<unsafe extern "C" fn(usize, u64, i32)>,
    swap_interval_mesa: Option<unsafe extern "C" fn(u32) -> i32>,
    x_free: unsafe extern "C" fn(*mut c_void) -> i32,
    x_default_screen: unsafe extern "C" fn(usize) -> i32,
    x_get_window_attributes: unsafe extern "C" fn(usize, u64, *mut u8) -> i32,
    x_visual_id: unsafe extern "C" fn(usize) -> u64,
}

unsafe impl Send for Glx {}
unsafe impl Sync for Glx {}

#[allow(clippy::missing_transmute_annotations)]
fn glx() -> Option<&'static Glx> {
    static G: OnceLock<Option<Glx>> = OnceLock::new();
    G.get_or_init(|| unsafe {
        let gl = hostcall::libgl();
        let x = hostcall::libx11();
        if gl == 0 || x == 0 {
            crate::log_warn!("opengl32", "libGL ou libX11 absente : OpenGL indisponible");
            return None;
        }
        macro_rules! need {
            ($lib:expr, $n:literal) => {{
                let p = hostcall::sym($lib, concat!($n, "\0").as_bytes());
                if p == 0 {
                    crate::log_warn!("opengl32", "symbole {} absent", $n);
                    return None;
                }
                std::mem::transmute::<usize, _>(p)
            }};
        }
        macro_rules! opt {
            ($n:literal) => {{
                let p = hostcall::gl_proc(concat!($n, "\0").as_bytes());
                (p != 0).then(|| std::mem::transmute::<usize, _>(p))
            }};
        }
        let open: unsafe extern "C" fn(*const i8) -> usize = need!(x, "XOpenDisplay");
        let dpy = open(std::ptr::null());
        if dpy == 0 {
            crate::log_warn!("opengl32", "pas d'affichage X11 : OpenGL indisponible");
            return None;
        }
        super::dxgi::install_x_error_guard();
        Some(Glx {
            dpy,
            choose_fbconfig: need!(gl, "glXChooseFBConfig"),
            get_fbconfig_attrib: need!(gl, "glXGetFBConfigAttrib"),
            create_new_context: need!(gl, "glXCreateNewContext"),
            create_context_attribs: opt!("glXCreateContextAttribsARB"),
            make_current: need!(gl, "glXMakeCurrent"),
            destroy_context: need!(gl, "glXDestroyContext"),
            swap_buffers: need!(gl, "glXSwapBuffers"),
            swap_interval_ext: opt!("glXSwapIntervalEXT"),
            swap_interval_mesa: opt!("glXSwapIntervalMESA"),
            x_free: need!(x, "XFree"),
            x_default_screen: need!(x, "XDefaultScreen"),
            x_get_window_attributes: need!(x, "XGetWindowAttributes"),
            x_visual_id: need!(x, "XVisualIDFromVisual"),
        })
    })
    .as_ref()
}

/// Fenêtre X11 derrière un HDC (celui d'un HWND).
fn dc_drawable(hdc: usize) -> Option<(usize, u64)> {
    let hwnd = super::gui::dc_window(hdc).filter(|&h| h != 0)?;
    super::gui::native_window(hwnd).map(|w| (hwnd, w))
}

/// FBConfig GLX pour un format, de préférence avec le visuel de la fenêtre.
unsafe fn fbconfig(g: &Glx, f: Format, window: u64) -> Option<usize> {
    let mut a = vec![
        GLX_X_RENDERABLE,
        1,
        GLX_DRAWABLE_TYPE,
        1,
        GLX_RENDER_TYPE,
        1,
        GLX_DOUBLEBUFFER,
        f.double as i32,
        GLX_RED_SIZE,
        8,
        GLX_GREEN_SIZE,
        8,
        GLX_BLUE_SIZE,
        8,
        GLX_DEPTH_SIZE,
        f.depth as i32,
        GLX_STENCIL_SIZE,
        f.stencil as i32,
    ];
    if f.samples > 0 {
        a.extend_from_slice(&[GLX_SAMPLE_BUFFERS, 1, GLX_SAMPLES, f.samples as i32]);
    }
    a.push(0);
    let mut wanted = 0u64;
    if window != 0 {
        let mut attrs = [0u8; 256];
        if (g.x_get_window_attributes)(g.dpy, window, attrs.as_mut_ptr()) != 0 {
            let visual = usize::from_ne_bytes(attrs[24..32].try_into().unwrap_or_default());
            if visual != 0 {
                wanted = (g.x_visual_id)(visual);
            }
        }
    }
    let screen = (g.x_default_screen)(g.dpy);
    let mut n = 0;
    let list = (g.choose_fbconfig)(g.dpy, screen, a.as_ptr(), &mut n);
    if list.is_null() || n <= 0 {
        return None;
    }
    let configs = std::slice::from_raw_parts(list, n as usize);
    let mut pick = configs[0];
    if wanted != 0 {
        for &c in configs {
            let mut v = 0;
            (g.get_fbconfig_attrib)(g.dpy, c, GLX_VISUAL_ID, &mut v);
            if v as u64 == wanted {
                pick = c;
                break;
            }
        }
    }
    (g.x_free)(list.cast());
    Some(pick)
}

// ---------- Contextes ----------

struct Context {
    glx: usize,
    format: i32,
    window: u64,
    attribs: Vec<i32>,
    used: bool,
}

fn contexts() -> &'static Mutex<HashMap<usize, Context>> {
    static C: OnceLock<Mutex<HashMap<usize, Context>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Format choisi pour chaque fenêtre (SetPixelFormat).
fn window_formats() -> &'static Mutex<HashMap<usize, i32>> {
    static F: OnceLock<Mutex<HashMap<usize, i32>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_HGLRC: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x7_0000);
static SWAP_INTERVAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);

thread_local! {
    static CURRENT: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

unsafe fn create_glx(g: &Glx, format: i32, window: u64, share: usize, attribs: &[i32]) -> usize {
    let Some(cfg) = format_at(format).and_then(|f| fbconfig(g, f, window)) else {
        return 0;
    };
    if !attribs.is_empty() {
        if let Some(create) = g.create_context_attribs {
            let mut a: Vec<i32> = Vec::new();
            for p in attribs.chunks(2) {
                // WGL_CONTEXT_LAYER_PLANE_ARB n'existe pas en GLX ; les
                // autres attributs ont les mêmes valeurs.
                if p.len() == 2 && p[0] != 0x2093 {
                    a.extend_from_slice(p);
                }
            }
            a.push(0);
            return create(g.dpy, cfg, share, 1, a.as_ptr());
        }
    }
    (g.create_new_context)(g.dpy, cfg, GLX_RGBA_TYPE, share, 1)
}

fn format_of_dc(hdc: usize) -> i32 {
    super::gui::dc_window(hdc)
        .and_then(|h| lock(window_formats()).get(&h).copied())
        .unwrap_or(1)
}

unsafe fn create_context(hdc: usize, share: usize, attribs: &[i32]) -> usize {
    let Some(g) = glx() else { return 0 };
    // Sans fenêtre X11 (session sans affichage, HDC mémoire), le contexte ne
    // pourrait jamais devenir courant : échec dès la création, comme un
    // pilote Windows sans format accéléré.
    let Some((_, window)) = dc_drawable(hdc) else {
        crate::log_warn!(
            "opengl32",
            "contexte demandé sur le HDC {hdc:#x} sans fenêtre X11"
        );
        crate::process::set_last_error(2000); // ERROR_INVALID_PIXEL_FORMAT
        return 0;
    };
    let format = format_of_dc(hdc);
    let share_glx = lock(contexts()).get(&share).map(|c| c.glx).unwrap_or(0);
    let ctx = create_glx(g, format, window, share_glx, attribs);
    if ctx == 0 {
        crate::log_warn!(
            "opengl32",
            "création du contexte GLX impossible (format {format})"
        );
        return 0;
    }
    let h = NEXT_HGLRC.fetch_add(0x10, std::sync::atomic::Ordering::Relaxed);
    lock(contexts()).insert(
        h,
        Context {
            glx: ctx,
            format,
            window,
            attribs: attribs.to_vec(),
            used: false,
        },
    );
    h
}

pub extern "win64" fn wgl_create_context(hdc: usize) -> usize {
    unsafe { create_context(hdc, 0, &[]) }
}

extern "win64" fn wgl_create_layer_context(hdc: usize, layer: i32) -> usize {
    if layer != 0 {
        return 0;
    }
    wgl_create_context(hdc)
}

extern "win64" fn wgl_create_context_attribs(
    hdc: usize,
    share: usize,
    attribs: *const i32,
) -> usize {
    let mut list = Vec::new();
    unsafe {
        let mut p = attribs;
        while !p.is_null() && *p != 0 {
            list.extend_from_slice(&[*p, *p.add(1)]);
            p = p.add(2);
        }
    }
    if list.is_empty() {
        // Liste vide : contexte par défaut, mais par le chemin ARB.
        list.extend_from_slice(&[0x2091, 1, 0x2092, 0]);
    }
    unsafe { create_context(hdc, share, &list) }
}

extern "win64" fn wgl_delete_context(h: usize) -> i32 {
    let Some(g) = glx() else { return 0 };
    let Some(c) = lock(contexts()).remove(&h) else {
        return 0;
    };
    CURRENT.with(|cur| {
        if cur.get().1 == h {
            unsafe { (g.make_current)(g.dpy, 0, 0) };
            cur.set((0, 0));
        }
    });
    unsafe { (g.destroy_context)(g.dpy, c.glx) };
    1
}

pub extern "win64" fn wgl_make_current(hdc: usize, h: usize) -> i32 {
    let Some(g) = glx() else { return 0 };
    if h == 0 {
        unsafe { (g.make_current)(g.dpy, 0, 0) };
        CURRENT.with(|c| c.set((0, 0)));
        return 1;
    }
    let Some((_, window)) = dc_drawable(hdc) else {
        crate::log_warn!("opengl32", "wglMakeCurrent : HDC {hdc:#x} sans fenêtre");
        return 0;
    };
    let ctx = {
        let mut map = lock(contexts());
        let Some(c) = map.get_mut(&h) else { return 0 };
        c.used = true;
        if c.window == 0 {
            c.window = window;
        }
        c.glx
    };
    if unsafe { (g.make_current)(g.dpy, window, ctx) } == 0 {
        crate::log_warn!("opengl32", "glXMakeCurrent a échoué");
        return 0;
    }
    // Fenêtre présentée par OpenGL : le dessin GDI de Winlux (thème, tampon
    // arrière) ne doit plus la recouvrir. La fenêtre SDL de Xonotic restait
    // figée sur sa première image, recopiée à chaque peinture (#1421).
    mark_gl_window(window);
    CURRENT.with(|c| c.set((hdc, h)));
    1
}

extern "win64" fn wgl_make_context_current(draw: usize, _read: usize, h: usize) -> i32 {
    wgl_make_current(draw, h)
}

extern "win64" fn wgl_get_current_context() -> usize {
    CURRENT.with(|c| c.get().1)
}

extern "win64" fn wgl_get_current_dc() -> usize {
    CURRENT.with(|c| c.get().0)
}

/// wglShareLists : GLX ne partage qu'à la création. Si le contexte cible n'a
/// encore jamais servi (le cas habituel), il est recréé en partage.
extern "win64" fn wgl_share_lists(src: usize, dst: usize) -> i32 {
    let Some(g) = glx() else { return 0 };
    let mut map = lock(contexts());
    let Some(src_glx) = map.get(&src).map(|c| c.glx) else {
        return 0;
    };
    let Some(d) = map.get_mut(&dst) else { return 0 };
    if d.used {
        crate::log_warn!(
            "opengl32",
            "wglShareLists sur un contexte déjà utilisé : refusé"
        );
        return 0;
    }
    unsafe {
        let new = create_glx(g, d.format, d.window, src_glx, &d.attribs.clone());
        if new == 0 {
            return 0;
        }
        (g.destroy_context)(g.dpy, d.glx);
        d.glx = new;
    }
    1
}

extern "win64" fn wgl_copy_context(_src: usize, _dst: usize, _mask: u32) -> i32 {
    0
}

/// Signale une fois au backend la fenêtre X où OpenGL présente.
fn mark_gl_window(window: u64) {
    static MARKED: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
    let first = lock(MARKED.get_or_init(Default::default)).insert(window);
    if first {
        super::gui::mark_direct3d_window(window);
    }
}

pub extern "win64" fn swap_buffers(hdc: usize) -> i32 {
    let Some(g) = glx() else { return 0 };
    let Some((_, window)) = dc_drawable(hdc) else {
        return 0;
    };
    unsafe { (g.swap_buffers)(g.dpy, window) };
    1
}

extern "win64" fn wgl_swap_layer_buffers(hdc: usize, _planes: u32) -> i32 {
    swap_buffers(hdc)
}

pub extern "win64" fn choose_pixel_format(_hdc: usize, pfd: *const u8) -> i32 {
    if pfd.is_null() {
        return 0;
    }
    let mut buf = [0u8; 40];
    unsafe { std::ptr::copy_nonoverlapping(pfd, buf.as_mut_ptr(), 40) };
    choose(&buf)
}

pub extern "win64" fn describe_pixel_format(_hdc: usize, i: i32, size: u32, pfd: *mut u8) -> i32 {
    if !pfd.is_null() {
        let Some(f) = format_at(i) else { return 0 };
        if size < 40 {
            return 0;
        }
        let mut buf = [0u8; 40];
        describe(f, &mut buf);
        unsafe { std::ptr::copy_nonoverlapping(buf.as_ptr(), pfd, 40) };
    }
    FORMATS.len() as i32
}

pub extern "win64" fn set_pixel_format(hdc: usize, i: i32, _pfd: *const u8) -> i32 {
    if format_at(i).is_none() {
        return 0;
    }
    let Some(hwnd) = super::gui::dc_window(hdc).filter(|&h| h != 0) else {
        return 0;
    };
    let mut map = lock(window_formats());
    // Comme sous Windows : le format d'une fenêtre ne change plus.
    match map.get(&hwnd) {
        Some(&old) => (old == i) as i32,
        None => {
            map.insert(hwnd, i);
            1
        }
    }
}

pub extern "win64" fn get_pixel_format(hdc: usize) -> i32 {
    super::gui::dc_window(hdc)
        .and_then(|h| lock(window_formats()).get(&h).copied())
        .unwrap_or(0)
}

extern "win64" fn wgl_get_pixel_format_attribiv(
    _hdc: usize,
    i: i32,
    _layer: i32,
    n: u32,
    attribs: *const i32,
    values: *mut i32,
) -> i32 {
    if attribs.is_null() || values.is_null() {
        return 0;
    }
    for k in 0..n as usize {
        let a = unsafe { *attribs.add(k) };
        let v = if a == WGL_NUMBER_PIXEL_FORMATS {
            FORMATS.len() as i32
        } else {
            match format_at(i) {
                Some(f) => attrib(f, a),
                None => return 0,
            }
        };
        unsafe { *values.add(k) = v };
    }
    1
}

extern "win64" fn wgl_get_pixel_format_attribfv(
    hdc: usize,
    i: i32,
    layer: i32,
    n: u32,
    attribs: *const i32,
    values: *mut f32,
) -> i32 {
    let mut tmp = vec![0i32; n as usize];
    if wgl_get_pixel_format_attribiv(hdc, i, layer, n, attribs, tmp.as_mut_ptr()) == 0 {
        return 0;
    }
    for (k, v) in tmp.iter().enumerate() {
        unsafe { *values.add(k) = *v as f32 };
    }
    1
}

extern "win64" fn wgl_choose_pixel_format_arb(
    _hdc: usize,
    iattribs: *const i32,
    _fattribs: *const f32,
    max: u32,
    formats: *mut i32,
    count: *mut u32,
) -> i32 {
    let mut list = Vec::new();
    unsafe {
        let mut p = iattribs;
        while !p.is_null() && *p != 0 {
            list.push((*p, *p.add(1)));
            p = p.add(2);
        }
    }
    let found = choose_arb(&list);
    let n = found.len().min(max as usize);
    unsafe {
        if !formats.is_null() {
            std::ptr::copy_nonoverlapping(found.as_ptr(), formats, n);
        }
        if !count.is_null() {
            *count = n as u32;
        }
    }
    1
}

extern "win64" fn wgl_swap_interval(interval: i32) -> i32 {
    let Some(g) = glx() else { return 0 };
    SWAP_INTERVAL.store(interval, std::sync::atomic::Ordering::Relaxed);
    let hdc = CURRENT.with(|c| c.get().0);
    unsafe {
        if let (Some(f), Some((_, w))) = (g.swap_interval_ext, dc_drawable(hdc)) {
            f(g.dpy, w, interval);
            return 1;
        }
        if let Some(f) = g.swap_interval_mesa {
            return (f(interval.max(0) as u32) == 0) as i32;
        }
    }
    1
}

extern "win64" fn wgl_get_swap_interval() -> i32 {
    SWAP_INTERVAL.load(std::sync::atomic::Ordering::Relaxed)
}

const WGL_EXTENSIONS: &[u8] = b"WGL_ARB_extensions_string WGL_EXT_extensions_string \
WGL_ARB_create_context WGL_ARB_create_context_profile WGL_ARB_pixel_format \
WGL_ARB_multisample WGL_ARB_framebuffer_sRGB WGL_EXT_framebuffer_sRGB \
WGL_EXT_swap_control WGL_ARB_make_current_read\0";

extern "win64" fn wgl_get_extensions_string_arb(_hdc: usize) -> *const u8 {
    WGL_EXTENSIONS.as_ptr()
}

extern "win64" fn wgl_get_extensions_string_ext() -> *const u8 {
    WGL_EXTENSIONS.as_ptr()
}

extern "win64" fn wgl_false() -> i32 {
    0
}

extern "win64" fn wgl_use_font(_hdc: usize, _first: u32, _count: u32, _base: u32) -> i32 {
    crate::log_warn!("opengl32", "wglUseFontBitmaps/Outlines non pris en charge");
    0
}

extern "win64" fn wgl_describe_layer_plane(
    _h: usize,
    _f: i32,
    _l: i32,
    _n: u32,
    _p: *mut u8,
) -> i32 {
    0
}

extern "win64" fn wgl_layer_palette(_h: usize, _l: i32, _s: i32, _n: i32, _c: *mut u32) -> i32 {
    0
}

extern "win64" fn wgl_realize_layer_palette(_h: usize, _l: i32, _r: i32) -> i32 {
    0
}

// ---------- Rappels de débogage (glDebugMessageCallback) ----------

struct DebugCb {
    cb: usize,
    user: usize,
}

extern "C" fn debug_tramp(
    src: u32,
    ty: u32,
    id: u32,
    sev: u32,
    len: i32,
    msg: *const u8,
    user: *const DebugCb,
) {
    let d = unsafe { &*user };
    let f: extern "win64" fn(u32, u32, u32, u32, i32, *const u8, usize) =
        unsafe { std::mem::transmute(d.cb) };
    f(src, ty, id, sev, len, msg, d.user);
}

extern "C" fn debug_tramp_amd(
    id: u32,
    cat: u32,
    sev: u32,
    len: i32,
    msg: *const u8,
    user: *const DebugCb,
) {
    let d = unsafe { &*user };
    let f: extern "win64" fn(u32, u32, u32, i32, *const u8, usize) =
        unsafe { std::mem::transmute(d.cb) };
    f(id, cat, sev, len, msg, d.user);
}

fn debug_callback(name: &'static [u8], amd: bool, cb: usize, user: usize) {
    let host = hostcall::gl_proc(name);
    if host == 0 {
        return;
    }
    let f: unsafe extern "C" fn(usize, usize) = unsafe { std::mem::transmute(host) };
    if cb == 0 {
        unsafe { f(0, 0) };
        return;
    }
    let data = Box::leak(Box::new(DebugCb { cb, user })) as *const DebugCb as usize;
    let tramp = if amd {
        debug_tramp_amd as *const () as usize
    } else {
        debug_tramp as *const () as usize
    };
    unsafe { f(tramp, data) };
}

extern "win64" fn gl_debug_message_callback(cb: usize, user: usize) {
    debug_callback(b"glDebugMessageCallback\0", false, cb, user);
}
extern "win64" fn gl_debug_message_callback_arb(cb: usize, user: usize) {
    debug_callback(b"glDebugMessageCallbackARB\0", false, cb, user);
}
extern "win64" fn gl_debug_message_callback_khr(cb: usize, user: usize) {
    debug_callback(b"glDebugMessageCallbackKHR\0", false, cb, user);
}
extern "win64" fn gl_debug_message_callback_amd(cb: usize, user: usize) {
    debug_callback(b"glDebugMessageCallbackAMD\0", true, cb, user);
}

// ---------- wglGetProcAddress ----------

/// Fonctions WGL et OpenGL réécrites ici plutôt que pontées telles quelles.
fn special(name: &str) -> Option<usize> {
    let f: usize = match name {
        "wglCreateContextAttribsARB" => wgl_create_context_attribs as *const () as usize,
        "wglChoosePixelFormatARB" | "wglChoosePixelFormatEXT" => {
            wgl_choose_pixel_format_arb as *const () as usize
        }
        "wglGetPixelFormatAttribivARB" | "wglGetPixelFormatAttribivEXT" => {
            wgl_get_pixel_format_attribiv as *const () as usize
        }
        "wglGetPixelFormatAttribfvARB" | "wglGetPixelFormatAttribfvEXT" => {
            wgl_get_pixel_format_attribfv as *const () as usize
        }
        "wglSwapIntervalEXT" => wgl_swap_interval as *const () as usize,
        "wglGetSwapIntervalEXT" => wgl_get_swap_interval as *const () as usize,
        "wglGetExtensionsStringARB" => wgl_get_extensions_string_arb as *const () as usize,
        "wglGetExtensionsStringEXT" => wgl_get_extensions_string_ext as *const () as usize,
        "wglMakeContextCurrentARB" => wgl_make_context_current as *const () as usize,
        "wglGetCurrentReadDCARB" => wgl_get_current_dc as *const () as usize,
        "glDebugMessageCallback" => gl_debug_message_callback as *const () as usize,
        "glDebugMessageCallbackARB" => gl_debug_message_callback_arb as *const () as usize,
        "glDebugMessageCallbackKHR" => gl_debug_message_callback_khr as *const () as usize,
        "glDebugMessageCallbackAMD" => gl_debug_message_callback_amd as *const () as usize,
        _ => return None,
    };
    Some(f)
}

/// Adresse Windows d'une fonction OpenGL ou WGL, ou 0.
pub fn proc_address(name: &str) -> usize {
    if let Some(f) = special(name) {
        return f;
    }
    let Some(kinds) = hostcall::gl_signatures().get(name) else {
        return 0;
    };
    // La fonction doit exister chez l'hôte (glXGetProcAddress renvoie
    // parfois une entrée générique : on s'en contente, comme Mesa).
    let mut z = name.as_bytes().to_vec();
    z.push(0);
    if hostcall::gl_proc(&z) == 0 {
        return 0;
    }
    hostcall::lazy_bridge(name, kinds, Resolver::Gl)
}

extern "win64" fn wgl_get_proc_address(name: *const u8) -> usize {
    if name.is_null() {
        return 0;
    }
    let n = unsafe { super::cstr(name) };
    let p = proc_address(&n);
    if p == 0 {
        crate::log_warn!("opengl32", "wglGetProcAddress({n}) : absente");
    }
    p
}

extern "win64" fn wgl_get_default_proc_address(name: *const u8) -> usize {
    wgl_get_proc_address(name)
}

pub fn exports() -> Exports {
    let mut e = crate::exports! {
        "wglCreateContext" => fn wgl_create_context,
        "wglCreateLayerContext" => fn wgl_create_layer_context,
        "wglDeleteContext" => fn wgl_delete_context,
        "wglMakeCurrent" => fn wgl_make_current,
        "wglGetCurrentContext" => fn wgl_get_current_context,
        "wglGetCurrentDC" => fn wgl_get_current_dc,
        "wglShareLists" => fn wgl_share_lists,
        "wglCopyContext" => fn wgl_copy_context,
        "wglGetProcAddress" => fn wgl_get_proc_address,
        "wglGetDefaultProcAddress" => fn wgl_get_default_proc_address,
        "wglSwapBuffers" => fn swap_buffers,
        "wglSwapLayerBuffers" => fn wgl_swap_layer_buffers,
        "wglChoosePixelFormat" => fn choose_pixel_format,
        "wglDescribePixelFormat" => fn describe_pixel_format,
        "wglSetPixelFormat" => fn set_pixel_format,
        "wglGetPixelFormat" => fn get_pixel_format,
        "wglUseFontBitmapsA" => fn wgl_use_font,
        "wglUseFontBitmapsW" => fn wgl_use_font,
        "wglUseFontOutlinesA" => fn wgl_false,
        "wglUseFontOutlinesW" => fn wgl_false,
        "wglDescribeLayerPlane" => fn wgl_describe_layer_plane,
        "wglGetLayerPaletteEntries" => fn wgl_layer_palette,
        "wglSetLayerPaletteEntries" => fn wgl_layer_palette,
        "wglRealizeLayerPalette" => fn wgl_realize_layer_palette,
    };
    for (name, kinds) in hostcall::gl_signatures().core() {
        e.insert(name, hostcall::lazy_bridge(name, kinds, Resolver::Gl));
    }
    e
}

/// Fonctions de format de pixel et SwapBuffers de gdi32.
pub fn gdi32_exports() -> Exports {
    crate::exports! {
        "ChoosePixelFormat" => fn choose_pixel_format,
        "DescribePixelFormat" => fn describe_pixel_format,
        "SetPixelFormat" => fn set_pixel_format,
        "GetPixelFormat" => fn get_pixel_format,
        "SwapBuffers" => fn swap_buffers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_pixel_format_classique() {
        // 32 bits, double tampon, profondeur 24, stencil 8.
        let mut pfd = [0u8; 40];
        pfd[4..8].copy_from_slice(
            &(PFD_DOUBLEBUFFER | PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL).to_le_bytes(),
        );
        pfd[9] = 32;
        pfd[23] = 24;
        pfd[24] = 8;
        assert_eq!(choose(&pfd), 1);
        // Simple tampon, profondeur 16 : pas de format à 16 en simple
        // tampon, le plus proche reste en simple tampon.
        pfd[4..8].copy_from_slice(&(PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL).to_le_bytes());
        pfd[23] = 16;
        pfd[24] = 0;
        assert!(!FORMATS[choose(&pfd) as usize - 1].double);
        // Sans profondeur.
        pfd[4..8].copy_from_slice(&(PFD_DOUBLEBUFFER | PFD_SUPPORT_OPENGL).to_le_bytes());
        pfd[23] = 0;
        assert_eq!(FORMATS[choose(&pfd) as usize - 1].depth, 0);
    }

    #[test]
    fn describe_remplit_le_pfd() {
        let mut b = [0u8; 40];
        describe(FORMATS[0], &mut b);
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 40);
        let flags = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        assert!(flags & PFD_DOUBLEBUFFER != 0 && flags & PFD_SUPPORT_OPENGL != 0);
        assert_eq!((b[9], b[23], b[24]), (32, 24, 8));
        assert_eq!(flags & PFD_STEREO, 0);
    }

    #[test]
    fn choose_pixel_format_arb_multisample() {
        let list = choose_arb(&[
            (WGL_DRAW_TO_WINDOW, 1),
            (WGL_SUPPORT_OPENGL, 1),
            (WGL_DOUBLE_BUFFER, 1),
            (WGL_PIXEL_TYPE, WGL_TYPE_RGBA),
            (WGL_COLOR_BITS, 32),
            (WGL_DEPTH_BITS, 24),
            (WGL_STENCIL_BITS, 8),
            (WGL_SAMPLE_BUFFERS, 1),
            (WGL_SAMPLES, 4),
        ]);
        assert_eq!(
            list.first().map(|&i| FORMATS[i as usize - 1].samples),
            Some(4)
        );
        assert!(list.iter().all(|&i| FORMATS[i as usize - 1].samples >= 4));
        let none = choose_arb(&[(WGL_DRAW_TO_BITMAP, 1)]);
        assert!(none.is_empty());
        let plain = choose_arb(&[(WGL_DOUBLE_BUFFER, 1), (WGL_DEPTH_BITS, 24)]);
        assert_eq!(plain.first(), Some(&1));
    }

    #[test]
    fn exports_complets() {
        let e = exports();
        for n in [
            "glClear",
            "glBegin",
            "glVertex3f",
            "glTexImage2D",
            "wglCreateContext",
            "wglGetProcAddress",
        ] {
            assert!(e.contains_key(n), "{n}");
        }
        assert!(e.len() >= 336 + 20);
        assert!(special("wglCreateContextAttribsARB").is_some());
        assert!(gdi32_exports().contains_key("SwapBuffers"));
    }
}
