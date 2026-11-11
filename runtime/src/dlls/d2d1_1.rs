//! Direct2D 1.1 à 1.3 (#561) : ID2D1Factory1 à 7, ID2D1Device à
//! ID2D1Device6, ID2D1DeviceContext à ID2D1DeviceContext6, ID2D1Bitmap1,
//! ID2D1Effect, styles de trait, blocs d'état, collections de dégradés,
//! contextes de couleur et ID2D1Multithread.
//!
//! L'ordre des méthodes suit `d2d1_1.h` (MinGW-w64, domaine public) et, pour
//! les versions 1.2 et 1.3, les définitions de TerraFX.Interop.Windows
//! (licence MIT). Le contexte d'appareil est une cible de rendu logicielle de
//! `d2d1.rs` dont la destination est le bitmap choisi par SetTarget ; un
//! bitmap créé sur une surface DXGI (texture Direct3D 11 Winlux) recopie ses
//! pixels dans la texture à chaque EndDraw.
//!
//! Effets : les objets ID2D1Effect gardent leurs entrées et leurs propriétés.
//! Les effets personnalisés enregistrés par RegisterEffectFromString sont
//! calculés (module `custom`, pixel shaders exécutés sur OpenGL) ; les effets
//! intégrés de Direct2D dessinent encore leur entrée 0 telle quelle.

use super::{
    add_ref, draw_bitmap_pixels, draw_glyph_run, draw_text, draw_text_layout, get_factory, method,
    new_res, nothing, pop_clip, push_clip, qi_list, release, target, target_bitmap_from_wic,
    Bitmap, BitmapObj, FactoryObj, Output, RectF, Res, Target, TargetObj, TargetState, BITMAP_VT,
    IID_D2D1_IMAGE, IID_D2D1_RESOURCE,
};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::d2d_raster::Matrix;
use crate::dlls::ole32::Guid;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

#[path = "d2d1_effects.rs"]
mod custom;

const D2DERR_EFFECT_IS_NOT_REGISTERED: i32 = 0x8899_0028u32 as i32;
const D2DERR_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
/// d2derr.h de mingw-w64.
const D2DERR_UNSUPPORTED_OPERATION: i32 = 0x8899_0003u32 as i32;

// --- Identifiants ----------------------------------------------------------------

/// ID2D1Factory1 à 7.
const FACTORY_IIDS: [Guid; 7] = [
    guid(
        0xbb12d362,
        0xdaee,
        0x4b9a,
        [0xaa, 0x1d, 0x14, 0xba, 0x40, 0x1c, 0xfa, 0x1f],
    ),
    guid(
        0x94f81a73,
        0x9212,
        0x4376,
        [0x9c, 0x58, 0xb1, 0x6a, 0x3a, 0x0d, 0x39, 0x92],
    ),
    guid(
        0x0869759f,
        0x4f00,
        0x413f,
        [0xb0, 0x3e, 0x2b, 0xda, 0x45, 0x40, 0x4d, 0x0f],
    ),
    guid(
        0xbd4ec2d2,
        0x0662,
        0x4bee,
        [0xba, 0x8e, 0x6f, 0x29, 0xf0, 0x32, 0xe0, 0x96],
    ),
    guid(
        0xc4349994,
        0x838e,
        0x4b0f,
        [0x8c, 0xab, 0x44, 0x99, 0x7d, 0x9e, 0xea, 0xcc],
    ),
    guid(
        0xf9976f46,
        0xf642,
        0x44c1,
        [0x97, 0xca, 0xda, 0x32, 0xea, 0x2a, 0x26, 0x35],
    ),
    guid(
        0xbdc2bdd3,
        0xb96c,
        0x4de6,
        [0xbd, 0xf7, 0x99, 0xd4, 0x74, 0x54, 0x54, 0xde],
    ),
];

/// ID2D1Device à ID2D1Device6.
const DEVICE_IIDS: [Guid; 7] = [
    guid(
        0x47dd575d,
        0xac05,
        0x4cdd,
        [0x80, 0x49, 0x9b, 0x02, 0xcd, 0x16, 0xf4, 0x4c],
    ),
    guid(
        0xd21768e1,
        0x23a4,
        0x4823,
        [0xa1, 0x4b, 0x7c, 0x3e, 0xba, 0x85, 0xd6, 0x58],
    ),
    guid(
        0xa44472e1,
        0x8dfb,
        0x4e60,
        [0x84, 0x92, 0x6e, 0x28, 0x61, 0xc9, 0xca, 0x8b],
    ),
    guid(
        0x852f2087,
        0x802c,
        0x4037,
        [0xab, 0x60, 0xff, 0x2e, 0x7e, 0xe6, 0xfc, 0x01],
    ),
    guid(
        0xd7bdb159,
        0x5683,
        0x4a46,
        [0xbc, 0x9c, 0x72, 0xdc, 0x72, 0x0b, 0x85, 0x8b],
    ),
    guid(
        0xd55ba0a4,
        0x6405,
        0x4694,
        [0xae, 0xf5, 0x08, 0xee, 0x1a, 0x43, 0x58, 0xb4],
    ),
    guid(
        0x7bfef914,
        0x2d75,
        0x4bad,
        [0xbe, 0x87, 0xe1, 0x8d, 0xdb, 0x07, 0x7b, 0x6d],
    ),
];

/// ID2D1DeviceContext à ID2D1DeviceContext6.
const CONTEXT_IIDS: [Guid; 7] = [
    guid(
        0xe8f7fe7a,
        0x191c,
        0x466d,
        [0xad, 0x95, 0x97, 0x56, 0x78, 0xbd, 0xa9, 0x98],
    ),
    guid(
        0xd37f57e4,
        0x6908,
        0x459f,
        [0xa1, 0x99, 0xe7, 0x2f, 0x24, 0xf7, 0x99, 0x87],
    ),
    guid(
        0x394ea6a3,
        0x0c34,
        0x4321,
        [0x95, 0x0b, 0x6c, 0xa2, 0x0f, 0x0b, 0xe6, 0xc7],
    ),
    guid(
        0x235a7496,
        0x8351,
        0x414c,
        [0xbc, 0xd4, 0x66, 0x72, 0xab, 0x2d, 0x8e, 0x00],
    ),
    guid(
        0x8c427831,
        0x3d90,
        0x4476,
        [0xb6, 0x47, 0xc4, 0xfa, 0xe3, 0x49, 0xe4, 0xdb],
    ),
    guid(
        0x7836d248,
        0x68cc,
        0x4df6,
        [0xb9, 0xe8, 0xde, 0x99, 0x1b, 0xf6, 0x2e, 0xb7],
    ),
    guid(
        0x985f7e37,
        0x4ed0,
        0x4a19,
        [0x98, 0xa3, 0x15, 0xb0, 0xed, 0xfd, 0xe3, 0x06],
    ),
];

pub(super) const IID_D2D1_BITMAP1: Guid = guid(
    0xa898a84c,
    0x3873,
    0x4588,
    [0xb0, 0x8b, 0xeb, 0xbf, 0x97, 0x8d, 0xf0, 0x41],
);
const IID_D2D1_MULTITHREAD: Guid = guid(
    0x31e6e7bc,
    0xe0ff,
    0x4d46,
    [0x8c, 0x64, 0xa0, 0xa8, 0xc4, 0x1c, 0x15, 0xd3],
);
const IID_D2D1_PROPERTIES: Guid = guid(
    0x483473d7,
    0xcd46,
    0x4f9d,
    [0x9d, 0x3a, 0x31, 0x12, 0xaa, 0x80, 0x15, 0x9d],
);
const IID_D2D1_EFFECT: Guid = guid(
    0x28211a43,
    0x7d89,
    0x476f,
    [0x81, 0x81, 0x2d, 0x61, 0x59, 0xb2, 0x20, 0xad],
);
const IID_D2D1_STROKE_STYLE: Guid = guid(
    0x2cd9069d,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_STROKE_STYLE1: Guid = guid(
    0x10a72a66,
    0xe91c,
    0x43f4,
    [0x99, 0x3f, 0xdd, 0xf4, 0xb8, 0x2b, 0x0b, 0x4a],
);
const IID_D2D1_STATE_BLOCK: Guid = guid(
    0x28506e39,
    0xebf6,
    0x46a1,
    [0xbb, 0x47, 0xfd, 0x85, 0x56, 0x5a, 0xb9, 0x57],
);
const IID_D2D1_STATE_BLOCK1: Guid = guid(
    0x689f1f85,
    0xc72e,
    0x4e33,
    [0x8f, 0x19, 0x85, 0x75, 0x4e, 0xfd, 0x5a, 0xce],
);
const IID_D2D1_GRADIENT_STOPS: Guid = guid(
    0x2cd906a7,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_GRADIENT_STOPS1: Guid = guid(
    0xae1572f4,
    0x5dd0,
    0x4777,
    [0x99, 0x8b, 0x92, 0x79, 0x47, 0x2a, 0xe6, 0x3b],
);
const IID_D2D1_COLOR_CONTEXT: Guid = guid(
    0x1c4820bb,
    0x5771,
    0x4518,
    [0xa5, 0x81, 0x2f, 0xe4, 0xdd, 0x0e, 0xc6, 0x57],
);
const IID_D2D1_COLOR_CONTEXT1: Guid = guid(
    0x1ab42875,
    0xc57f,
    0x4be9,
    [0xbd, 0x85, 0x9c, 0xd7, 0x8d, 0x6f, 0x55, 0xee],
);

/// ID2D1Factory1 (fabrique créée pour un appareil de composition).
pub(super) fn factory1_iid() -> Guid {
    FACTORY_IIDS[0]
}

/// ID2D1Device.
pub(super) fn device_iid() -> Guid {
    DEVICE_IIDS[0]
}

/// La fabrique répond aux interfaces 1.1 à 1.3 en plus de ID2D1Factory.
pub(super) fn is_factory_n_iid(riid: &Guid) -> bool {
    FACTORY_IIDS.contains(riid)
}

pub(super) fn is_multithread_iid(riid: &Guid) -> bool {
    *riid == IID_D2D1_MULTITHREAD
}

pub(super) fn context_iids() -> &'static [Guid] {
    &CONTEXT_IIDS
}

/// Le contexte d'appareil est une cible dont la table est DC_VT.
pub(super) unsafe fn is_device_context(this: *mut TargetObj) -> bool {
    *(this as *const *const ()) == DC_VT.ptr()
}

// --- Vue « contexte d'appareil » des cibles 1.0 ------------------------------------

/// Sous Windows 8 et suivants, toute cible Direct2D répond à QueryInterface
/// vers ID2D1DeviceContext à ID2D1DeviceContext6 (#1085). Les méthodes 57 et
/// suivantes des cibles 1.0 (Hwnd, DC, Bitmap) n'ont pas le sens de celles du
/// contexte : la cible remet donc un second pointeur d'interface dont chaque
/// méthode est un relais qui remplace `this` par la cible puis saute dans
/// DC_VT. AddRef, Release et QueryInterface relaient aussi : la vue vit
/// autant que sa cible et IUnknown reste la cible elle-même.
#[repr(C)]
pub(super) struct ContextView {
    vtbl: *const (),
    target: *mut TargetObj,
    dc: *const (),
}

// Chaque relais `i` (16 octets alignés) : rax = DC_VT, rcx = cible, saut
// vers DC_VT[i]. La table des relais est construite au même endroit.
core::arch::global_asm!(
    ".pushsection .text.winlux_d2d_context_view,\"ax\",@progbits",
    ".p2align 4",
    "winlux_d2d_context_thunks:",
    ".set winlux_d2d_k, 0",
    ".rept 120",
    ".p2align 4",
    "mov rax, qword ptr [rcx + 16]",
    "mov rcx, qword ptr [rcx + 8]",
    "jmp qword ptr [rax + 8 * winlux_d2d_k]",
    ".set winlux_d2d_k, winlux_d2d_k + 1",
    ".endr",
    ".popsection",
    ".pushsection .data.rel.ro.winlux_d2d_context_view,\"aw\",@progbits",
    ".p2align 3",
    ".globl winlux_d2d_context_view_vt",
    "winlux_d2d_context_view_vt:",
    ".set winlux_d2d_k, 0",
    ".rept 120",
    ".quad winlux_d2d_context_thunks + 16 * winlux_d2d_k",
    ".set winlux_d2d_k, winlux_d2d_k + 1",
    ".endr",
    ".popsection",
);

extern "C" {
    static winlux_d2d_context_view_vt: [*const (); 120];
}

pub(super) fn view_vt() -> *const () {
    std::ptr::addr_of!(winlux_d2d_context_view_vt) as *const ()
}

/// QueryInterface d'une cible 1.0 vers ID2D1DeviceContext* : sa vue.
pub(super) unsafe fn context_view(this: *mut TargetObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let p = {
        let mut t = target(this);
        let v = t.ext.view.get_or_insert_with(|| {
            Box::new(ContextView {
                vtbl: view_vt(),
                target: this,
                dc: DC_VT.ptr(),
            })
        });
        &mut **v as *mut ContextView
    };
    Com::add_ref(this);
    put(out, p)
}

// --- Prolongement de la cible pour le contexte d'appareil --------------------------

/// Champs propres au contexte d'appareil (vides pour les cibles 1.0).
pub(super) struct TargetExt {
    /// ID2D1Device parent (référence tenue).
    device: *mut c_void,
    unit_mode: u32,
    primitive_blend: u32,
    /// D2D1_RENDERING_CONTROLS : précision et taille des tuiles.
    controls: [u32; 3],
    dpi: (f32, f32),
    /// Vue ID2D1DeviceContext d'une cible 1.0 (créée à la première demande).
    view: Option<Box<ContextView>>,
    /// Destination d'origine d'une cible fenêtre ou HDC mise de côté par
    /// SetTarget vers un bitmap, rendue par SetTarget(NULL) (#1590).
    home: Option<Box<Home>>,
    /// Bitmap rendu par GetTarget pour la fenêtre ou le HDC (référence
    /// tenue) : SetTarget de ce bitmap rétablit la destination d'origine.
    home_bitmap: *mut c_void,
}

/// Fenêtre ou HDC d'une cible 1.0, avec ses pixels, le temps qu'un bitmap
/// lui serve de cible.
pub(super) struct Home {
    out: Output,
    w: u32,
    h: u32,
    px: Vec<u8>,
}

/// Échange la destination courante et celle mise de côté.
fn swap_home(t: &mut Target, home: &mut Home) {
    std::mem::swap(&mut t.out, &mut home.out);
    std::mem::swap(&mut t.w, &mut home.w);
    std::mem::swap(&mut t.h, &mut home.h);
    std::mem::swap(&mut t.px, &mut home.px);
}

/// BindDC ou Resize pendant qu'un bitmap sert de cible : la fenêtre ou le
/// HDC d'origine redevient la cible avant d'être changé (#1609). Rend le
/// bitmap quitté, à relâcher une fois le verrou de la cible rendu.
pub(super) unsafe fn leave_bitmap(t: &mut Target) -> *mut c_void {
    let Some(mut home) = t.ext.home.take() else {
        return std::ptr::null_mut();
    };
    if t.drawing && matches!(t.out, Output::Bitmap(_)) {
        t.sync_wic(true);
    }
    swap_home(t, &mut home);
    match home.out {
        Output::Bitmap(b) => b,
        _ => std::ptr::null_mut(),
    }
}

/// WINLUX_D2D_TRACE (#1609) : journal des cibles, bitmaps, effets et tracés.
pub(super) fn trace_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("WINLUX_D2D_TRACE").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

/// Une ligne sur stderr, au plus 200 000 par processus.
pub(super) fn trace(line: std::fmt::Arguments) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    if n < 200_000 {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) };
        eprintln!("winlux[d2d] {tid} {line}");
    } else if n == 200_000 {
        eprintln!("winlux[d2d] journal plein, suite ignorée");
    }
}

/// Pixels non transparents d'un tampon BGRA, avec la moyenne BGRA de ces
/// pixels pour le journal (une image blanche et une image de texte noir
/// ont autant de pixels visibles).
fn visible(px: &[u8]) -> String {
    let mut n = 0u64;
    let mut sum = [0u64; 4];
    for p in px.as_chunks::<4>().0.iter().filter(|p| p[3] != 0) {
        n += 1;
        for (s, &c) in sum.iter_mut().zip(p) {
            *s += c as u64;
        }
    }
    if n == 0 {
        return "0".into();
    }
    let m = sum.map(|s| s / n);
    format!("{n} moy {:?}", m)
}

/// Destination d'une cible pour le journal.
pub(super) fn describe_target(t: &Target) -> String {
    let out = match t.out {
        Output::Wic(w) => format!("wic {w:p}"),
        Output::Hwnd(h) => format!("hwnd {h:#x}"),
        Output::Dc { hdc, x, y } => format!("hdc {hdc:#x} @{x},{y}"),
        Output::Bitmap(b) => format!("bitmap {b:p}"),
        Output::None => "aucune".into(),
    };
    format!(
        "{out} {}x{} visibles {}{}",
        t.w,
        t.h,
        visible(&t.px),
        if t.ext.home.is_some() {
            " (fenêtre de côté)"
        } else {
            ""
        }
    )
}

/// Image pour le journal : nature, taille, pixels visibles ou CLSID.
pub(super) unsafe fn describe_image(image: *mut c_void) -> String {
    if image.is_null() {
        return "NULL".into();
    }
    let vt = *(image as *const *const ());
    if vt == BITMAP_VT.ptr() || vt == COMMAND_LIST_VT.ptr() {
        let b = &Com::get(image as *mut BitmapObj).data;
        b.refresh();
        let kind = if vt == BITMAP_VT.ptr() {
            "bitmap"
        } else {
            "liste"
        };
        let n = visible(&crate::dlls::lock(&b.px));
        return format!(
            "{kind} {image:p} {}x{} options {:#x} visibles {n}",
            b.w, b.h, b.options
        );
    }
    let effect = if vt == EFFECT_IMAGE_VT.ptr() || vt == EFFECT_RESOURCE_VT.ptr() {
        Com::get(image as *mut Com<Res<EffectImage>>).data.effect
    } else if vt == EFFECT_VT.ptr() {
        image as *mut EffectObj
    } else {
        return format!("image inconnue {image:p}");
    };
    let e = Com::get(effect);
    format!(
        "effet {effect:p} {:?}{}",
        e.clsid,
        if e.custom.get().is_some() {
            " (personnalisé)"
        } else {
            ""
        }
    )
}

/// EndDraw alors qu'un bitmap sert de cible : la fenêtre ou le HDC
/// d'origine reçoit aussi ce qui lui a été dessiné avant SetTarget.
pub(super) unsafe fn end_draw_home(t: &mut Target) {
    if let Some(mut home) = t.ext.home.take() {
        swap_home(t, &mut home);
        t.sync_wic(true);
        swap_home(t, &mut home);
        t.ext.home = Some(home);
    }
}

impl Default for TargetExt {
    fn default() -> Self {
        TargetExt {
            device: std::ptr::null_mut(),
            unit_mode: 0,
            primitive_blend: 0,
            controls: [1, 1024, 1024],
            dpi: (96.0, 96.0),
            view: None,
            home: None,
            home_bitmap: std::ptr::null_mut(),
        }
    }
}

impl Drop for TargetExt {
    fn drop(&mut self) {
        if !self.device.is_null() {
            unsafe { com_release(self.device) };
        }
        if !self.home_bitmap.is_null() {
            unsafe { com_release(self.home_bitmap) };
        }
    }
}

unsafe fn com_release(p: *mut c_void) -> u32 {
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 2)(p)
}
unsafe fn com_add_ref(p: *mut c_void) -> u32 {
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 1)(p)
}

// --- Fabrique : méthodes 17 à 32 -------------------------------------------------------

/// CreateDevice (et ses variantes 1.2/1.3, de même signature).
pub(super) unsafe extern "win64" fn factory_create_device(
    this: *mut FactoryObj,
    dxgi: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if dxgi.is_null() {
        return E_INVALIDARG;
    }
    com_add_ref(dxgi);
    put(out, new_res(this, DEVICE_VT.ptr(), D2Device { dxgi }))
}

/// D2D1_STROKE_STYLE_PROPERTIES1 : caps, jointure, onglet, tirets, décalage,
/// type de transformation (absent de la version 1.0).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StrokeProps {
    start_cap: u32,
    end_cap: u32,
    dash_cap: u32,
    line_join: u32,
    miter_limit: f32,
    dash_style: u32,
    dash_offset: f32,
    transform_type: u32,
}

struct StrokeStyle {
    props: StrokeProps,
    dashes: Vec<f32>,
}

unsafe fn new_stroke_style(
    f: *mut FactoryObj,
    props: *const u8,
    size: usize,
    dashes: *const f32,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if props.is_null() || (n > 0 && dashes.is_null()) {
        return E_INVALIDARG;
    }
    let mut p = StrokeProps::default();
    std::ptr::copy_nonoverlapping(props, &mut p as *mut StrokeProps as *mut u8, size);
    let dashes = if n > 0 {
        std::slice::from_raw_parts(dashes, n as usize).to_vec()
    } else {
        Vec::new()
    };
    put(
        out,
        new_res(f, STROKE_VT.ptr(), StrokeStyle { props: p, dashes }),
    )
}

/// ID2D1Factory::CreateStrokeStyle (D2D1_STROKE_STYLE_PROPERTIES, 28 octets).
pub(super) unsafe extern "win64" fn factory_stroke_style(
    this: *mut FactoryObj,
    props: *const u8,
    dashes: *const f32,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_stroke_style(this, props, 28, dashes, n, out)
}

/// ID2D1Factory1::CreateStrokeStyle (PROPERTIES1, 32 octets).
pub(super) unsafe extern "win64" fn factory_stroke_style1(
    this: *mut FactoryObj,
    props: *const u8,
    dashes: *const f32,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_stroke_style(this, props, 32, dashes, n, out)
}

/// D2D1_DRAWING_STATE_DESCRIPTION1 ; la version 1.0 s'arrête avant
/// primitive_blend.
#[repr(C)]
#[derive(Clone, Copy)]
struct StateDesc {
    antialias: u32,
    text_antialias: u32,
    tag1: u64,
    tag2: u64,
    transform: Matrix,
    primitive_blend: u32,
    unit_mode: u32,
}

impl Default for StateDesc {
    fn default() -> Self {
        StateDesc {
            antialias: 0,
            text_antialias: 0,
            tag1: 0,
            tag2: 0,
            transform: Matrix::IDENTITY,
            primitive_blend: 0,
            unit_mode: 0,
        }
    }
}

const STATE_DESC_SIZE: usize = 48;
const STATE_DESC1_SIZE: usize = 56;

struct StateBlock {
    desc: Mutex<StateDesc>,
    params: Mutex<*mut c_void>,
}

impl Drop for StateBlock {
    fn drop(&mut self) {
        let p = *crate::dlls::lock(&self.params);
        if !p.is_null() {
            unsafe { com_release(p) };
        }
    }
}

type StateBlockObj = Com<Res<StateBlock>>;

unsafe fn new_state_block(
    f: *mut FactoryObj,
    desc: *const u8,
    size: usize,
    params: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut d = StateDesc::default();
    if !desc.is_null() {
        std::ptr::copy_nonoverlapping(desc, &mut d as *mut StateDesc as *mut u8, size);
    }
    if !params.is_null() {
        com_add_ref(params);
    }
    let b = StateBlock {
        desc: Mutex::new(d),
        params: Mutex::new(params),
    };
    put(out, new_res(f, STATE_BLOCK_VT.ptr(), b))
}

pub(super) unsafe extern "win64" fn factory_state_block(
    this: *mut FactoryObj,
    desc: *const u8,
    params: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    new_state_block(this, desc, STATE_DESC_SIZE, params, out)
}

pub(super) unsafe extern "win64" fn factory_state_block1(
    this: *mut FactoryObj,
    desc: *const u8,
    params: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    new_state_block(this, desc, STATE_DESC1_SIZE, params, out)
}

/// Effets enregistrés par l'application : identifiant, description lue.
static EFFECTS: Mutex<Vec<(Guid, Arc<custom::Registration>)>> = Mutex::new(Vec::new());

unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

unsafe fn register_effect(
    clsid: *const Guid,
    xml: String,
    bindings: *const usize,
    n: u32,
    create: usize,
) -> i32 {
    let Some(id) = clsid.as_ref().copied() else {
        return E_INVALIDARG;
    };
    let mut reg = custom::parse(&xml);
    reg.create = create;
    custom::bind(&mut reg, bindings, n);
    crate::log_info!(
        "d2d1",
        "effet personnalisé {:?} enregistré ({} propriété(s), {} entrée(s))",
        id,
        reg.props.len(),
        reg.inputs
    );
    let mut list = crate::dlls::lock(&EFFECTS);
    list.retain(|(g, _)| *g != id);
    list.push((id, Arc::new(reg)));
    S_OK
}

/// Contenu d'un IStream (ISequentialStream::Read), en texte UTF-8 ou UTF-16.
unsafe fn read_stream(stream: *mut c_void) -> String {
    if stream.is_null() {
        return String::new();
    }
    let read =
        method::<unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32>(stream, 3);
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut got = 0u32;
        let hr = read(stream, buf.as_mut_ptr(), buf.len() as u32, &mut got);
        all.extend_from_slice(&buf[..got as usize]);
        if hr != S_OK || got == 0 || all.len() > 16 << 20 {
            break;
        }
    }
    if all.starts_with(&[0xff, 0xfe]) {
        let w: Vec<u16> = all[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        return String::from_utf16_lossy(&w);
    }
    String::from_utf8_lossy(all.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&all)).into_owned()
}

pub(super) unsafe extern "win64" fn factory_register_stream(
    _this: *mut FactoryObj,
    clsid: *const Guid,
    stream: *mut c_void,
    bindings: *const usize,
    n: u32,
    create: usize,
) -> i32 {
    register_effect(clsid, read_stream(stream), bindings, n, create)
}

pub(super) unsafe extern "win64" fn factory_register_string(
    _this: *mut FactoryObj,
    clsid: *const Guid,
    xml: *const u16,
    bindings: *const usize,
    n: u32,
    create: usize,
) -> i32 {
    register_effect(clsid, wide(xml), bindings, n, create)
}

pub(super) unsafe extern "win64" fn factory_unregister(
    _this: *mut FactoryObj,
    clsid: *const Guid,
) -> i32 {
    let Some(id) = clsid.as_ref().copied() else {
        return E_INVALIDARG;
    };
    let mut list = crate::dlls::lock(&EFFECTS);
    let before = list.len();
    list.retain(|(g, _)| *g != id);
    if list.len() == before {
        D2DERR_EFFECT_IS_NOT_REGISTERED
    } else {
        S_OK
    }
}

pub(super) unsafe extern "win64" fn factory_registered(
    _this: *mut FactoryObj,
    clsids: *mut Guid,
    n: u32,
    returned: *mut u32,
    registered: *mut u32,
) -> i32 {
    let list = crate::dlls::lock(&EFFECTS);
    let count = list.len() as u32;
    if !registered.is_null() {
        *registered = count;
    }
    let k = count.min(n);
    if !clsids.is_null() {
        for (i, (g, _)) in list.iter().take(k as usize).enumerate() {
            *clsids.add(i) = *g;
        }
    }
    if !returned.is_null() {
        *returned = k;
    }
    if k < count && !clsids.is_null() {
        D2DERR_INSUFFICIENT_BUFFER
    } else {
        S_OK
    }
}

/// ID2D1Factory1::GetEffectProperties : propriétés d'un effet enregistré
/// (ou intégré), sans instance du code de l'effet.
pub(super) unsafe extern "win64" fn factory_effect_properties(
    this: *mut FactoryObj,
    clsid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(id) = clsid.as_ref().copied() else {
        return E_INVALIDARG;
    };
    let reg = registration(&id);
    if reg.is_none() && !is_builtin_effect(&id) {
        return D2DERR_EFFECT_IS_NOT_REGISTERED;
    }
    let n = reg
        .as_ref()
        .map_or_else(|| builtin_inputs(&id), |r| r.inputs);
    let mut props = std::collections::BTreeMap::new();
    if let Some(r) = &reg {
        for (i, p) in r.props.iter().enumerate() {
            props.insert(i as u32, p.default.clone());
        }
    }
    Com::add_ref(this);
    let e = Effect {
        clsid: id,
        inputs: Mutex::new(vec![std::ptr::null_mut(); n]),
        props: Mutex::new(props),
        factory: this,
        custom: OnceLock::new(),
        desc: reg,
    };
    put(out, Com::create(EFFECT_VT.ptr(), e))
}

/// Description d'un effet enregistré par l'application.
fn registration(clsid: &Guid) -> Option<Arc<custom::Registration>> {
    let list = crate::dlls::lock(&EFFECTS);
    list.iter()
        .find(|(g, _)| g == clsid)
        .map(|(_, r)| r.clone())
}

// --- ID2D1Multithread ------------------------------------------------------------------

/// Verrou réentrant commun à toutes les fabriques multifil.
struct D2Lock {
    state: Mutex<(u64, u32)>,
    cv: std::sync::Condvar,
}

static D2_LOCK: D2Lock = D2Lock {
    state: Mutex::new((0, 0)),
    cv: std::sync::Condvar::new(),
};

fn thread_key() -> u64 {
    thread_local!(static KEY: u8 = const { 0 });
    KEY.with(|k| k as *const u8 as u64)
}

unsafe extern "win64" fn mt_qi(this: *mut c_void, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if riid.is_null() {
        return E_INVALIDARG;
    }
    if *riid == crate::dlls::audio_com::IID_IUNKNOWN || *riid == IID_D2D1_MULTITHREAD {
        *out = this;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    0x8000_4002u32 as i32
}
unsafe extern "win64" fn mt_ref(_: *mut c_void) -> u32 {
    1
}
unsafe extern "win64" fn mt_protected(_: *mut c_void) -> i32 {
    1
}
unsafe extern "win64" fn mt_enter(_: *mut c_void) {
    let me = thread_key();
    let mut g = crate::dlls::lock(&D2_LOCK.state);
    while g.1 > 0 && g.0 != me {
        g = D2_LOCK.cv.wait(g).unwrap_or_else(|e| e.into_inner());
    }
    g.0 = me;
    g.1 += 1;
}
unsafe extern "win64" fn mt_leave(_: *mut c_void) {
    let me = thread_key();
    let mut g = crate::dlls::lock(&D2_LOCK.state);
    if g.1 > 0 && g.0 == me {
        g.1 -= 1;
        if g.1 == 0 {
            g.0 = 0;
            D2_LOCK.cv.notify_one();
        }
    }
}

static MT_VT: VTable<6> = VTable([
    mt_qi as *const (),
    mt_ref as *const (),
    mt_ref as *const (),
    mt_protected as *const (),
    mt_enter as *const (),
    mt_leave as *const (),
]);

/// Objet COM réduit à sa table (premier mot).
#[repr(C)]
struct MtObj(*const *const ());
unsafe impl Sync for MtObj {}
static MT_OBJ: MtObj = MtObj(MT_VT.0.as_ptr());

/// ID2D1Multithread : un objet statique (AddRef/Release sans effet).
pub(super) unsafe fn multithread(out: *mut *mut c_void) -> i32 {
    *out = &MT_OBJ as *const MtObj as *mut c_void;
    S_OK
}

// --- ID2D1Device ------------------------------------------------------------------------

struct D2Device {
    /// IDXGIDevice (référence tenue).
    dxgi: *mut c_void,
}

impl Drop for D2Device {
    fn drop(&mut self) {
        if !self.dxgi.is_null() {
            unsafe { com_release(self.dxgi) };
        }
    }
}

type DeviceObj = Com<Res<D2Device>>;

unsafe extern "win64" fn device_qi(
    this: *mut DeviceObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let mut iids = vec![IID_D2D1_RESOURCE];
    iids.extend_from_slice(&DEVICE_IIDS);
    qi_list(this, riid, out, &iids)
}

/// CreateDeviceContext(options, out) : toutes les versions ont cette forme.
unsafe extern "win64" fn device_create_context(
    this: *mut DeviceObj,
    _options: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    Com::add_ref(this);
    let t = Target {
        w: 0,
        h: 0,
        px: Vec::new(),
        opaque: false,
        out: Output::None,
        state: TargetState::default(),
        drawing: false,
        ext: {
            let mut ext = TargetExt::default();
            ext.device = this.cast();
            ext
        },
    };
    put(
        out,
        new_res(Com::get(this).factory, DC_VT.ptr(), Mutex::new(t)),
    )
}

unsafe extern "win64" fn device_max_memory(_: *mut DeviceObj) -> u64 {
    u64::MAX
}
unsafe extern "win64" fn device_get_zero(_: *mut DeviceObj) -> u32 {
    0
}
unsafe extern "win64" fn device_get_dxgi(this: *mut DeviceObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let d = Com::get(this).data.dxgi;
    if d.is_null() {
        // Appareil d'une cible 1.0 : pas d'appareil DXGI derrière.
        *out = std::ptr::null_mut();
        return D2DERR_UNSUPPORTED_OPERATION;
    }
    com_add_ref(d);
    *out = d;
    S_OK
}
unsafe extern "win64" fn device_flush(_: *mut DeviceObj, _: *mut c_void) {}

static DEVICE_VT: VTable<21> = VTable([
    device_qi as *const (),
    add_ref::<Res<D2Device>> as *const (),
    release::<Res<D2Device>> as *const (),
    get_factory::<D2Device> as *const (),
    device_create_context as *const (),
    unsupported!("CreatePrintControl"),
    nothing as *const (), // SetMaximumTextureMemory
    device_max_memory as *const (),
    nothing as *const (),         // ClearResources
    device_get_zero as *const (), // GetRenderingPriority : NORMAL
    nothing as *const (),         // SetRenderingPriority
    device_create_context as *const (),
    device_create_context as *const (),
    device_flush as *const (), // FlushDeviceContexts
    device_get_dxgi as *const (),
    device_create_context as *const (),
    device_create_context as *const (),
    nothing as *const (), // SetMaximumColorGlyphCacheMemory
    device_max_memory as *const (),
    device_create_context as *const (),
    device_create_context as *const (),
]);

// --- Bitmaps 1.1 ------------------------------------------------------------------------

/// D2D1_BITMAP_PROPERTIES1 : format, dpi, options, contexte de couleur.
#[repr(C)]
struct BitmapProps1 {
    format: u32,
    alpha: u32,
    dpi_x: f32,
    dpi_y: f32,
    options: u32,
    color_context: *mut c_void,
}

unsafe fn props_options(props: *const BitmapProps1) -> (u32, u32) {
    props.as_ref().map_or((87, 0), |p| (p.format, p.options))
}

/// ID2D1DeviceContext::CreateBitmap (D2D1_SIZE_U par valeur).
unsafe extern "win64" fn dc_create_bitmap(
    this: *mut TargetObj,
    size: u64,
    src: *const u8,
    pitch: u32,
    props: *const BitmapProps1,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (w, h) = (size as u32, (size >> 32) as u32);
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let (format, options) = props_options(props);
    if !matches!(format, 0 | 87 | 91 | 28 | 29 | super::DXGI_FORMAT_A8_UNORM) {
        crate::log_warn!(
            "d2d1",
            "CreateBitmap : format DXGI {} traité comme BGRA 8 bits",
            format
        );
    }
    let mut b = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    b.options = options;
    b.alpha_only = format == super::DXGI_FORMAT_A8_UNORM;
    let obj = new_res(Com::get(this).factory, BITMAP_VT.ptr(), b);
    if !src.is_null() {
        super::bitmap_copy_from_memory(obj, std::ptr::null(), src, pitch);
    }
    d2d_trace!(
        "CreateBitmap1 {:p} → {:p} {w}x{h} format {format} options {options:#x}",
        this,
        obj
    );
    put(out, obj)
}

unsafe extern "win64" fn dc_bitmap_from_wic(
    this: *mut TargetObj,
    src: *mut c_void,
    _props: *const BitmapProps1,
    out: *mut *mut c_void,
) -> i32 {
    target_bitmap_from_wic(this, src, std::ptr::null(), out)
}

/// CreateBitmapFromDxgiSurface : la surface doit être une texture Winlux.
unsafe extern "win64" fn dc_bitmap_from_dxgi(
    this: *mut TargetObj,
    surface: *mut c_void,
    props: *const BitmapProps1,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some((w, h)) = super::super::d3d11::d2d_surface_size(surface) else {
        crate::log_warn!("d2d1", "CreateBitmapFromDxgiSurface : surface inconnue");
        return E_INVALIDARG;
    };
    let mut px = vec![0; w as usize * h as usize * 4];
    super::super::d3d11::d2d_surface_read(surface, &mut px);
    let mut b = Bitmap::new(w, h, px);
    b.options = props_options(props).1;
    com_add_ref(surface);
    b.surface = surface;
    put(out, new_res(Com::get(this).factory, BITMAP_VT.ptr(), b))
}

// ID2D1Bitmap1 (méthodes 11 à 15).
pub(super) unsafe extern "win64" fn bitmap_color_context(_: *mut BitmapObj, out: *mut *mut c_void) {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
}
pub(super) unsafe extern "win64" fn bitmap_options(this: *mut BitmapObj) -> u32 {
    Com::get(this).data.options
}
pub(super) unsafe extern "win64" fn bitmap_surface(
    this: *mut BitmapObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = Com::get(this).data.surface;
    if s.is_null() {
        *out = std::ptr::null_mut();
        return 0x8899_0013u32 as i32; // D2DERR_BITMAP_BOUND_AS_TARGET : pas de surface
    }
    com_add_ref(s);
    *out = s;
    S_OK
}

/// D2D1_MAPPED_RECT { pitch, bits }.
#[repr(C)]
pub(super) struct MappedRect {
    pitch: u32,
    bits: *mut u8,
}

/// Map : accès direct aux pixels (bitmap CPU_READ) jusqu'à Unmap.
pub(super) unsafe extern "win64" fn bitmap_map(
    this: *mut BitmapObj,
    _options: u32,
    out: *mut MappedRect,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let b = &Com::get(this).data;
    let mut px = crate::dlls::lock(&b.px);
    *out = MappedRect {
        pitch: b.w * 4,
        bits: px.as_mut_ptr(),
    };
    d2d_trace!("Map {:p} {}x{} options {:#x}", this, b.w, b.h, _options);
    S_OK
}
pub(super) unsafe extern "win64" fn bitmap_unmap(_: *mut BitmapObj) -> i32 {
    S_OK
}

// --- Effets -------------------------------------------------------------------------------

/// D2D1_PROPERTY_CLSID et suivantes (propriétés système des effets).
const PROP_CLSID: u32 = 0x8000_0000;
const PROP_INPUTS: u32 = 0x8000_0005;
const PROP_CACHED: u32 = 0x8000_0006;
const PROP_PRECISION: u32 = 0x8000_0007;
const PROP_MIN_INPUTS: u32 = 0x8000_0008;
const PROP_MAX_INPUTS: u32 = 0x8000_0009;

pub(super) struct Effect {
    clsid: Guid,
    inputs: Mutex<Vec<*mut c_void>>,
    props: Mutex<std::collections::BTreeMap<u32, Vec<u8>>>,
    factory: *mut FactoryObj,
    /// Code de l'application pour un effet personnalisé.
    custom: OnceLock<custom::Custom>,
    /// Description seule (GetEffectProperties d'un effet enregistré).
    desc: Option<Arc<custom::Registration>>,
}

impl Effect {
    /// Description de l'effet personnalisé, s'il en est un.
    fn reg(&self) -> Option<&custom::Registration> {
        self.custom.get().map(|c| &*c.reg).or(self.desc.as_deref())
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        for &i in crate::dlls::lock(&self.inputs).iter() {
            if !i.is_null() {
                unsafe { com_release(i) };
            }
        }
        if self.reg().is_none() {
            let props = crate::dlls::lock(&self.props);
            for (i, &(_, t, _)) in builtin_props(&self.clsid).iter().enumerate() {
                let p = props
                    .get(&(i as u32))
                    .map_or(std::ptr::null_mut(), |v| stored_interface(v));
                if is_interface_type(t) && !p.is_null() {
                    unsafe { com_release(p) };
                }
            }
        }
        unsafe { Com::release(self.factory) };
    }
}

type EffectObj = Com<Effect>;

/// Effets intégrés de Direct2D, reconnus au premier mot de leur CLSID :
/// ceux de `d2d1effects.h` (MinGW-w64) puis ceux de Windows 8.1 et 10.
const BUILTIN_EFFECTS: &[u32] = &[
    0x6aa9_7485,
    0xc284_4d0b,
    0xe846_7b04,
    0xfc15_1437,
    0x913e_2be4,
    0x5fb6_c24d,
    0x81c5_b77b,
    0x2a2d_49c0,
    0x8cea_8d1e,
    0x1a28_524c,
    0x921f_03d6,
    0x48fc_9f51,
    0x407f_8c08,
    0xe23f_7110,
    0x1743_19a6,
    0x9086_6fcd,
    0xedc4_8364,
    0x3e7e_fd62,
    0x428c_1ee5,
    0x6c26_c5c7,
    0x61c2_3c20,
    0x4094_44c4,
    0x1feb_6d69,
    0x9daf_9369,
    0x881d_b7d0,
    0x0f44_58ec,
    0xad47_c8fd,
    0x4125_1ab7,
    0xeae6_c40d,
    0x6c53_006a,
    0xb9e3_03c3,
    0x09c3_ca26,
    0x06ea_b419,
    0x5cb2_d9cf,
    0xc67e_a361,
    0x818a_1105,
    0xedae_421e,
    0x5bf8_18c3,
    0xb078_4138,
    0xcf2b_b6ae,
    0xfb9a_c489,
    // Windows 8.1 et 10 : YCbCr, contraste, teinte ↔ RVB, incrustation,
    // relief, exposition, niveaux de gris, inversion, postérisation, sépia,
    // netteté, redressement, température, vignette, contours, ombres et
    // lumières, table 3D, opacité, masque alpha, fondu, teinte, niveau de
    // blanc, tons HDR.
    0x9950_3cc1,
    0xb648_a78a,
    0x23f3_e5ec,
    0x7b78_a6bd,
    0x74c0_1f5b,
    0xb1c5_eb2b,
    0xb56c_8cfa,
    0x36dd_e0eb,
    0xe0c3_784d,
    0x2188_945e,
    0x3a1a_f410,
    0xc9b8_87cb,
    0x4da4_7b12,
    0x8917_6087,
    0xc00c_40be,
    0xeff5_83ca,
    0xcadc_8384,
    0x349e_0eda,
    0x811d_79a4,
    0xc80e_cff0,
    0x12f5_75e8,
    0x3631_2b17,
    0x44a1_cadb,
    0x7b0b_748d,
];

fn is_builtin_effect(clsid: &Guid) -> bool {
    BUILTIN_EFFECTS.contains(&clsid.data1)
}

/// Nombre d'entrées des effets intégrés ; 1 pour la plupart.
fn builtin_inputs(clsid: &Guid) -> usize {
    match clsid.data1 {
        // Flood, source bitmap, turbulence.
        0x61c2_3c20 | 0x5fb6_c24d | 0xcf2b_b6ae => 0,
        // Composite, mélange, composition arithmétique, déplacement,
        // masque alpha, fondu.
        0x48fc_9f51 | 0x81c5_b77b | 0xfc15_1437 | 0xedc4_8364 | 0xc80e_cff0 | 0x12f5_75e8 => 2,
        _ => 1,
    }
}

unsafe extern "win64" fn dc_create_effect(
    this: *mut TargetObj,
    clsid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let hr = create_effect(Com::get(this).factory, clsid, out);
    d2d_trace!("CreateEffect {:?} → {hr:#x} {:p}", clsid.as_ref(), *out);
    hr
}

/// ID2D1DeviceContext::CreateEffect et ID2D1EffectContext::CreateEffect.
unsafe fn create_effect(
    factory: *mut FactoryObj,
    clsid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(id) = clsid.as_ref().copied() else {
        return E_INVALIDARG;
    };
    let reg = registration(&id);
    if reg.is_none() && !is_builtin_effect(&id) {
        crate::log_debug!("d2d1", "CreateEffect {:?} : effet non enregistré", id);
        return D2DERR_EFFECT_IS_NOT_REGISTERED;
    }
    let n = reg
        .as_ref()
        .map_or_else(|| builtin_inputs(&id), |r| r.inputs);
    Com::add_ref(factory);
    let e = Effect {
        clsid: id,
        inputs: Mutex::new(vec![std::ptr::null_mut(); n]),
        props: Mutex::new(Default::default()),
        factory,
        custom: OnceLock::new(),
        desc: None,
    };
    crate::log_debug!("d2d1", "CreateEffect {:?} ({} entrée(s))", id, n);
    let obj = Com::create(EFFECT_VT.ptr(), e);
    if let Some(reg) = reg {
        let hr = custom::instantiate(reg, obj);
        if hr < 0 {
            Com::release(obj);
            return hr;
        }
    }
    put(out, obj)
}

unsafe extern "win64" fn effect_qi(
    this: *mut EffectObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    // ID2D1Resource : Paint.NET relie ses objets à leur fabrique par
    // GetFactory, effets compris (#1291). La table de l'effet suit
    // ID2D1Properties, d'où une facette à part qui tient l'effet.
    // ID2D1Image : la sortie de l'effet, comme GetOutput ; Paint.NET passe
    // ainsi ses effets à DrawImage (#1297).
    if !out.is_null() && !riid.is_null() && *riid == IID_D2D1_IMAGE {
        effect_get_output(this, out);
        return S_OK;
    }
    if !out.is_null() && !riid.is_null() && *riid == IID_D2D1_RESOURCE {
        Com::add_ref(this);
        let f = Com::get(this).factory;
        *out = new_res(f, EFFECT_RESOURCE_VT.ptr(), EffectImage { effect: this }).cast();
        return S_OK;
    }
    qi_list(this, riid, out, &[IID_D2D1_PROPERTIES, IID_D2D1_EFFECT])
}

/// ID2D1Resource d'un effet : GetFactory rend sa fabrique, les autres
/// interfaces viennent de l'effet lui-même.
unsafe extern "win64" fn effect_resource_qi(
    this: *mut Com<Res<EffectImage>>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !riid.is_null() && *riid == IID_D2D1_RESOURCE {
        return qi_list(this, riid, out, &[IID_D2D1_RESOURCE]);
    }
    effect_qi(Com::get(this).data.effect, riid, out)
}

// --- Listes de commandes (#1331) ---------------------------------------------------

/// ID2D1CommandList {B4F34A19-2383-4D76-94F6-EC343657C3DC} (d2d1_1.h).
const IID_D2D1_COMMAND_LIST: Guid = guid(
    0xb4f34a19,
    0x2383,
    0x4d76,
    [0x94, 0xf6, 0xec, 0x34, 0x36, 0x57, 0xc3, 0xdc],
);

/// Taille d'une liste de commandes créée sans cible : la liste n'a pas de
/// bornes chez Direct2D, ici elle est rendue dans une surface fixe.
const COMMAND_LIST_DEFAULT_SIZE: u32 = 1024;

/// Une liste de commandes est rendue au fil de l'eau dans des pixels,
/// comme un bitmap cible : SetTarget l'accepte, EndDraw y recopie le
/// dessin, et elle sert d'image (DrawImage, entrée d'effet, pinceau).
/// Stream n'est pas rejouable : E_NOTIMPL.
pub(super) static COMMAND_LIST_VT: VTable<6> = VTable([
    command_list_qi as *const (),
    add_ref::<Res<Bitmap>> as *const (),
    release::<Res<Bitmap>> as *const (),
    get_factory::<Bitmap> as *const (),
    unsupported!("ID2D1CommandList::Stream"),
    command_list_close as *const (),
]);

unsafe extern "win64" fn command_list_qi(
    this: *mut BitmapObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_IMAGE, IID_D2D1_COMMAND_LIST],
    )
}

unsafe extern "win64" fn command_list_close(_: *mut BitmapObj) -> i32 {
    S_OK
}

/// Bitmap ou liste de commandes : objets dont les pixels sont tenus ici.
pub(super) unsafe fn is_pixel_image(image: *mut c_void) -> bool {
    if image.is_null() {
        return false;
    }
    let vt = *(image as *const *const ());
    vt == BITMAP_VT.ptr() || vt == COMMAND_LIST_VT.ptr()
}

/// CreateCommandList : surface de la taille de la cible courante.
unsafe extern "win64" fn dc_create_command_list(
    this: *mut TargetObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (w, h) = {
        let t = target(this);
        (t.w, t.h)
    };
    let (w, h) = if w == 0 || h == 0 {
        (COMMAND_LIST_DEFAULT_SIZE, COMMAND_LIST_DEFAULT_SIZE)
    } else {
        (w, h)
    };
    let b = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    put(
        out,
        new_res(Com::get(this).factory, COMMAND_LIST_VT.ptr(), b),
    )
}

static EFFECT_RESOURCE_VT: VTable<4> = VTable([
    effect_resource_qi as *const (),
    add_ref::<Res<EffectImage>> as *const (),
    release::<Res<EffectImage>> as *const (),
    get_factory::<EffectImage> as *const (),
]);
/// Noms des propriétés système (D2D1_PROPERTY_CLSID = 0x80000000 et suivantes).
const SYSTEM_PROPS: [(&str, u32); 10] = [
    ("CLSID", custom::ty::CLSID),
    ("DisplayName", custom::ty::STRING),
    ("Author", custom::ty::STRING),
    ("Category", custom::ty::STRING),
    ("Description", custom::ty::STRING),
    ("Inputs", custom::ty::ARRAY),
    ("Cached", custom::ty::BOOL),
    ("Precision", custom::ty::ENUM),
    ("MinInputs", custom::ty::UINT32),
    ("MaxInputs", custom::ty::UINT32),
];

/// Propriétés publiques d'un effet intégré : nom, type, valeur par défaut
/// (énumérations ; nul pour les interfaces). Gestion des couleurs d'après
/// `D2D1_COLORMANAGEMENT_PROP` de `d2d1effects_1.h` (MinGW-w64) : contextes
/// source et destination, intentions perceptuelles, alpha prémultiplié,
/// qualité normale (#1367).
fn builtin_props(clsid: &Guid) -> &'static [(&'static str, u32, u32)] {
    use custom::ty::{COLOR_CONTEXT, ENUM};
    match clsid.data1 {
        0x1a28_524c => &[
            ("SourceColorContext", COLOR_CONTEXT, 0),
            ("SourceRenderingIntent", ENUM, 0),
            ("DestinationColorContext", COLOR_CONTEXT, 0),
            ("DestinationRenderingIntent", ENUM, 0),
            ("AlphaMode", ENUM, 1),
            ("Quality", ENUM, 1),
        ],
        _ => &[],
    }
}

/// Type interface : la valeur est un pointeur COM (IUnknown, contexte de couleur).
fn is_interface_type(t: u32) -> bool {
    t == custom::ty::IUNKNOWN || t == custom::ty::COLOR_CONTEXT
}

/// Valeur par défaut d'une propriété publique d'un effet intégré.
fn builtin_default(e: &Effect, index: u32) -> Option<Vec<u8>> {
    let &(_, t, d) = builtin_props(&e.clsid).get(index as usize)?;
    Some(if is_interface_type(t) {
        vec![0; std::mem::size_of::<usize>()]
    } else {
        d.to_le_bytes().to_vec()
    })
}

/// Pointeur COM rangé dans une valeur de propriété interface.
fn stored_interface(v: &[u8]) -> *mut c_void {
    match <[u8; 8]>::try_from(v) {
        Ok(b) => usize::from_le_bytes(b) as *mut c_void,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Nom et type d'une propriété (personnalisée, intégrée ou système).
fn prop_info(e: &Effect, index: u32) -> Option<(String, u32)> {
    if index >= PROP_CLSID {
        let (n, t) = SYSTEM_PROPS.get((index - PROP_CLSID) as usize)?;
        return Some((n.to_string(), *t));
    }
    if e.reg().is_none() {
        let &(n, t, _) = builtin_props(&e.clsid).get(index as usize)?;
        return Some((n.to_string(), t));
    }
    let p = e.reg()?.props.get(index as usize)?;
    Some((p.name.clone(), p.ty))
}

unsafe fn prop_by_name(e: &Effect, name: *const u16) -> u32 {
    let name = wide(name);
    if let Some(r) = e.reg() {
        if let Some(i) = r.props.iter().position(|p| p.name == name) {
            return i as u32;
        }
    } else if let Some(i) = builtin_props(&e.clsid)
        .iter()
        .position(|(n, _, _)| *n == name)
    {
        return i as u32;
    }
    SYSTEM_PROPS
        .iter()
        .position(|(n, _)| *n == name)
        .map_or(u32::MAX, |i| PROP_CLSID + i as u32)
}

unsafe extern "win64" fn effect_prop_count(this: *mut EffectObj) -> u32 {
    let e = Com::get(this);
    match e.reg() {
        Some(r) => r.props.len() as u32,
        None => crate::dlls::lock(&e.props)
            .keys()
            .filter(|&&k| k < PROP_CLSID)
            .count()
            .max(builtin_props(&e.clsid).len()) as u32,
    }
}
unsafe extern "win64" fn effect_prop_name(
    this: *mut EffectObj,
    index: u32,
    name: *mut u16,
    n: u32,
) -> i32 {
    let Some((s, _)) = prop_info(Com::get(this), index) else {
        if !name.is_null() && n > 0 {
            *name = 0;
        }
        return E_INVALIDARG;
    };
    let w: Vec<u16> = s.encode_utf16().collect();
    if name.is_null() || (n as usize) <= w.len() {
        return D2DERR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), name, w.len());
    *name.add(w.len()) = 0;
    S_OK
}
unsafe extern "win64" fn effect_name_length(this: *mut EffectObj, index: u32) -> u32 {
    prop_info(Com::get(this), index).map_or(0, |(s, _)| s.encode_utf16().count() as u32)
}
unsafe extern "win64" fn effect_prop_type(this: *mut EffectObj, index: u32) -> u32 {
    prop_info(Com::get(this), index).map_or(0, |(_, t)| t) // 0 : D2D1_PROPERTY_TYPE_UNKNOWN
}
unsafe extern "win64" fn effect_prop_index(this: *mut EffectObj, name: *const u16) -> u32 {
    prop_by_name(Com::get(this), name) // u32::MAX : D2D1_INVALID_PROPERTY_INDEX
}
unsafe extern "win64" fn effect_set_by_name(
    this: *mut EffectObj,
    name: *const u16,
    ty: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    let i = prop_by_name(Com::get(this), name);
    if i == u32::MAX {
        return E_INVALIDARG;
    }
    effect_set_value(this, i, ty, data, size)
}
unsafe extern "win64" fn effect_set_value(
    this: *mut EffectObj,
    index: u32,
    _ty: u32,
    data: *const u8,
    size: u32,
) -> i32 {
    if data.is_null() && size > 0 {
        return E_INVALIDARG;
    }
    let v = if size > 0 {
        std::slice::from_raw_parts(data, size as usize).to_vec()
    } else {
        Vec::new()
    };
    let e = Com::get(this);
    if let Some(hr) = custom::set_value(e, index, &v) {
        return hr;
    }
    let interface = e.reg().is_none()
        && builtin_props(&e.clsid)
            .get(index as usize)
            .is_some_and(|p| is_interface_type(p.1));
    if interface {
        // Pointeur COM : l'effet garde sa propre référence.
        if size as usize != std::mem::size_of::<usize>() {
            return E_INVALIDARG;
        }
        let p = stored_interface(&v);
        if !p.is_null() {
            com_add_ref(p);
        }
    }
    let old = crate::dlls::lock(&e.props).insert(index, v);
    if interface {
        let p = old.map_or(std::ptr::null_mut(), |o| stored_interface(&o));
        if !p.is_null() {
            com_release(p);
        }
    }
    S_OK
}
unsafe extern "win64" fn effect_get_by_name(
    this: *mut EffectObj,
    name: *const u16,
    ty: u32,
    data: *mut u8,
    size: u32,
) -> i32 {
    let i = prop_by_name(Com::get(this), name);
    if i == u32::MAX {
        if !data.is_null() {
            std::ptr::write_bytes(data, 0, size as usize);
        }
        return E_INVALIDARG;
    }
    effect_get_value(this, i, ty, data, size)
}
unsafe fn system_prop(e: &Effect, index: u32) -> Option<Vec<u8>> {
    let n = crate::dlls::lock(&e.inputs).len() as u32;
    let bytes = |v: u32| v.to_le_bytes().to_vec();
    Some(match index {
        PROP_CLSID => std::slice::from_raw_parts(&e.clsid as *const Guid as *const u8, 16).to_vec(),
        0x8000_0001..=0x8000_0004 => {
            custom::system_string(e, (index - 0x8000_0001) as usize).unwrap_or_else(|| vec![0, 0])
        }
        PROP_CACHED => bytes(0),
        PROP_PRECISION => bytes(0),
        PROP_MIN_INPUTS | PROP_MAX_INPUTS | PROP_INPUTS => bytes(n),
        _ => return None,
    })
}
unsafe extern "win64" fn effect_get_value(
    this: *mut EffectObj,
    index: u32,
    _ty: u32,
    data: *mut u8,
    size: u32,
) -> i32 {
    if data.is_null() {
        return E_INVALIDARG;
    }
    let e = Com::get(this);
    if let Some((hr, _)) = custom::get_value(e, index, data, size) {
        return hr;
    }
    let v = crate::dlls::lock(&e.props)
        .get(&index)
        .cloned()
        .or_else(|| system_prop(e, index))
        .or_else(|| builtin_default(e, index))
        .unwrap_or_default();
    std::ptr::write_bytes(data, 0, size as usize);
    std::ptr::copy_nonoverlapping(v.as_ptr(), data, v.len().min(size as usize));
    if e.reg().is_none()
        && size as usize >= v.len()
        && builtin_props(&e.clsid)
            .get(index as usize)
            .is_some_and(|p| is_interface_type(p.1))
    {
        // Comme tout paramètre de sortie COM, l'appelant reçoit une référence.
        let p = stored_interface(&v);
        if !p.is_null() {
            com_add_ref(p);
        }
    }
    S_OK
}
unsafe extern "win64" fn effect_value_size(this: *mut EffectObj, index: u32) -> u32 {
    let e = Com::get(this);
    if let Some((_, n)) = custom::get_value(e, index, std::ptr::null_mut(), 0) {
        return n;
    }
    crate::dlls::lock(&e.props)
        .get(&index)
        .map(|v| v.len())
        .or_else(|| system_prop(e, index).map(|v| v.len()))
        .or_else(|| builtin_default(e, index).map(|v| v.len()))
        .unwrap_or(0) as u32
}
unsafe extern "win64" fn effect_sub_props(_: *mut EffectObj, _: u32, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0x8007_0490u32 as i32 // HRESULT_FROM_WIN32(ERROR_NOT_FOUND)
}
unsafe extern "win64" fn effect_set_input(
    this: *mut EffectObj,
    index: u32,
    input: *mut c_void,
    _invalidate: i32,
) {
    d2d_trace!(
        "SetInput effet {:p} [{index}] ← {}",
        this,
        describe_image(input)
    );
    let mut inputs = crate::dlls::lock(&Com::get(this).inputs);
    let i = index as usize;
    if i >= inputs.len() {
        inputs.resize(i + 1, std::ptr::null_mut());
    }
    if !input.is_null() {
        com_add_ref(input);
    }
    let old = std::mem::replace(&mut inputs[i], input);
    drop(inputs);
    custom::inputs_changed(Com::get(this));
    if !old.is_null() {
        com_release(old);
    }
}
unsafe extern "win64" fn effect_set_input_count(this: *mut EffectObj, n: u32) -> i32 {
    let mut inputs = crate::dlls::lock(&Com::get(this).inputs);
    let keep = (n as usize).min(inputs.len());
    for old in inputs.drain(keep..) {
        if !old.is_null() {
            com_release(old);
        }
    }
    inputs.resize(n as usize, std::ptr::null_mut());
    drop(inputs);
    custom::inputs_changed(Com::get(this));
    S_OK
}
unsafe extern "win64" fn effect_get_input(this: *mut EffectObj, index: u32, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let inputs = crate::dlls::lock(&Com::get(this).inputs);
    let p = inputs
        .get(index as usize)
        .copied()
        .unwrap_or(std::ptr::null_mut());
    if !p.is_null() {
        com_add_ref(p);
    }
    *out = p;
}
unsafe extern "win64" fn effect_input_count(this: *mut EffectObj) -> u32 {
    crate::dlls::lock(&Com::get(this).inputs).len() as u32
}
unsafe extern "win64" fn effect_get_output(this: *mut EffectObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    Com::add_ref(this);
    let f = Com::get(this).factory;
    *out = new_res(f, EFFECT_IMAGE_VT.ptr(), EffectImage { effect: this }).cast();
}

static EFFECT_VT: VTable<19> = VTable([
    effect_qi as *const (),
    add_ref::<Effect> as *const (),
    release::<Effect> as *const (),
    effect_prop_count as *const (),
    effect_prop_name as *const (),
    effect_name_length as *const (),
    effect_prop_type as *const (),
    effect_prop_index as *const (),
    effect_set_by_name as *const (),
    effect_set_value as *const (),
    effect_get_by_name as *const (),
    effect_get_value as *const (),
    effect_value_size as *const (),
    effect_sub_props as *const (),
    effect_set_input as *const (),
    effect_set_input_count as *const (),
    effect_get_input as *const (),
    effect_input_count as *const (),
    effect_get_output as *const (),
]);

/// Sortie d'un effet (ID2D1Image) : tient l'effet.
struct EffectImage {
    effect: *mut EffectObj,
}

impl Drop for EffectImage {
    fn drop(&mut self) {
        unsafe { Com::release(self.effect) };
    }
}

unsafe extern "win64" fn effect_image_qi(
    this: *mut Com<Res<EffectImage>>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_D2D1_RESOURCE, IID_D2D1_IMAGE])
}

static EFFECT_IMAGE_VT: VTable<4> = VTable([
    effect_image_qi as *const (),
    add_ref::<Res<EffectImage>> as *const (),
    release::<Res<EffectImage>> as *const (),
    get_factory::<EffectImage> as *const (),
]);

/// Image calculée : pixels BGRA prémultipliés et position dans la scène.
pub(super) struct Img {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// Pixels d'une image (bitmap ou sortie d'effet). `hint` : taille de la
/// cible, qui borne les sorties sans limite.
pub(super) unsafe fn image_img(image: *mut c_void, depth: u32, hint: (u32, u32)) -> Option<Img> {
    if image.is_null() || depth > 32 {
        return None;
    }
    let vt = *(image as *const *const ());
    if vt == BITMAP_VT.ptr() || vt == COMMAND_LIST_VT.ptr() {
        let b = &Com::get(image as *mut BitmapObj).data;
        b.refresh();
        return Some(Img {
            x: 0,
            y: 0,
            w: b.w,
            h: b.h,
            px: crate::dlls::lock(&b.px).clone(),
        });
    }
    if vt == EFFECT_IMAGE_VT.ptr() || vt == EFFECT_RESOURCE_VT.ptr() {
        let e = Com::get(image as *mut Com<Res<EffectImage>>).data.effect;
        return image_of_effect(e.cast(), depth, hint);
    }
    // L'effet lui-même (DrawImage(ID2D1Effect*) des en-têtes C++).
    if vt == EFFECT_VT.ptr() {
        return image_of_effect(image, depth, hint);
    }
    None
}

/// Sortie d'un ID2D1Effect.
unsafe fn image_of_effect(effect: *mut c_void, depth: u32, hint: (u32, u32)) -> Option<Img> {
    if effect.is_null() || *(effect as *const *const ()) != EFFECT_VT.ptr() {
        return None;
    }
    let obj = effect as *mut EffectObj;
    let e = Com::get(obj);
    if e.custom.get().is_some() {
        if let Some(img) = custom::render(obj, depth, hint) {
            return Some(img);
        }
    }
    if let Some(img) = builtin_render(e, depth, hint) {
        return img;
    }
    warn_effect_once(&e.clsid);
    let first = crate::dlls::lock(&e.inputs).first().copied()?;
    image_img(first, depth + 1, hint)
}

// Effets intégrés calculés (CLSID de d2d1effects.h, mingw-w64).
/// CLSID_D2D1Flood {61C23C20-AE69-4D8E-94CF-50078DF638F2}.
const CLSID_FLOOD: Guid = guid(
    0x61c2_3c20,
    0xae69,
    0x4d8e,
    [0x94, 0xcf, 0x50, 0x07, 0x8d, 0xf6, 0x38, 0xf2],
);
/// CLSID_D2D1Crop {E23F7110-0E9A-4324-AF47-6A2C0C46F35B}.
const CLSID_CROP: Guid = guid(
    0xe23f_7110,
    0x0e9a,
    0x4324,
    [0xaf, 0x47, 0x6a, 0x2c, 0x0c, 0x46, 0xf3, 0x5b],
);
/// CLSID_D2D1Premultiply {06EAB419-DEED-4018-80D2-3E1D471ADEB2}.
const CLSID_PREMULTIPLY: Guid = guid(
    0x06ea_b419,
    0xdeed,
    0x4018,
    [0x80, 0xd2, 0x3e, 0x1d, 0x47, 0x1a, 0xde, 0xb2],
);
/// CLSID_D2D1ColorManagement {1A28524C-FDD6-4AA4-AE8F-837EB8267B37}.
const CLSID_COLOR_MANAGEMENT: Guid = guid(
    0x1a28_524c,
    0xfdd6,
    0x4aa4,
    [0xae, 0x8f, 0x83, 0x7e, 0xb8, 0x26, 0x7b, 0x37],
);

/// Propriété en flottants (vecteurs D2D1_VECTOR_*F).
fn prop_f32<const N: usize>(e: &Effect, index: u32) -> Option<[f32; N]> {
    let props = crate::dlls::lock(&e.props);
    let v = props.get(&index)?;
    if v.len() < N * 4 {
        return None;
    }
    let mut out = [0f32; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = f32::from_le_bytes([v[i * 4], v[i * 4 + 1], v[i * 4 + 2], v[i * 4 + 3]]);
    }
    Some(out)
}

/// Sortie d'un effet intégré calculé : `None` si l'effet n'est pas pris en
/// charge, `Some(None)` si sa sortie est vide (#1609 : Paint.NET remplit
/// les tuiles unies de son canevas par un bitmap 1x1 tiré d'un Flood).
unsafe fn builtin_render(e: &Effect, depth: u32, hint: (u32, u32)) -> Option<Option<Img>> {
    let input = || {
        let first = crate::dlls::lock(&e.inputs).first().copied()?;
        image_img(first, depth + 1, hint)
    };
    if e.clsid == CLSID_FLOOD {
        // D2D1_FLOOD_PROP_COLOR : RVBA non prémultiplié, noir opaque par défaut.
        let c = prop_f32::<4>(e, 0).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let a = c[3].clamp(0.0, 1.0);
        let q = |v: f32| (v.clamp(0.0, 1.0) * a * 255.0).round() as u8;
        let p = [q(c[2]), q(c[1]), q(c[0]), (a * 255.0).round() as u8];
        let (w, h) = (hint.0.max(1), hint.1.max(1));
        return Some(Some(Img {
            x: 0,
            y: 0,
            w,
            h,
            px: p.repeat(w as usize * h as usize),
        }));
    }
    if e.clsid == CLSID_CROP {
        let Some(img) = input() else {
            return Some(None);
        };
        // D2D1_CROP_PROP_RECT : gauche, haut, droite, bas ; infini par défaut.
        let r = prop_f32::<4>(e, 0).unwrap_or([f32::MIN, f32::MIN, f32::MAX, f32::MAX]);
        let clamp = |v: f32| v.clamp(-1.0e7, 1.0e7) as i64;
        let x0 = (img.x as i64).max(clamp(r[0].floor()));
        let y0 = (img.y as i64).max(clamp(r[1].floor()));
        let x1 = (img.x as i64 + img.w as i64).min(clamp(r[2].ceil()));
        let y1 = (img.y as i64 + img.h as i64).min(clamp(r[3].ceil()));
        if x1 <= x0 || y1 <= y0 {
            return Some(None);
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut px = vec![0u8; w * h * 4];
        let (sx, sy) = ((x0 - img.x as i64) as usize, (y0 - img.y as i64) as usize);
        for y in 0..h {
            let s = ((sy + y) * img.w as usize + sx) * 4;
            px[y * w * 4..][..w * 4].copy_from_slice(&img.px[s..s + w * 4]);
        }
        return Some(Some(Img {
            x: x0 as i32,
            y: y0 as i32,
            w: w as u32,
            h: h as u32,
            px,
        }));
    }
    if e.clsid == CLSID_PREMULTIPLY {
        let Some(mut img) = input() else {
            return Some(None);
        };
        for p in img.px.as_chunks_mut::<4>().0 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
        return Some(Some(img));
    }
    if e.clsid == CLSID_COLOR_MANAGEMENT {
        // Écran sRGB : l'image passe telle quelle.
        return Some(input());
    }
    None
}

fn warn_effect_once(id: &Guid) {
    static SEEN: Mutex<Vec<Guid>> = Mutex::new(Vec::new());
    let mut s = crate::dlls::lock(&SEEN);
    if !s.contains(id) {
        s.push(*id);
        crate::log_warn!(
            "d2d1",
            "effet {:?} pas encore calculé : son entrée est dessinée telle quelle",
            id
        );
    }
}

/// D2D1_INTERPOLATION_MODE → plus proche (0) ou bilinéaire.
fn interpolation(mode: u32) -> u32 {
    (mode != 0) as u32
}

unsafe extern "win64" fn dc_draw_image(
    this: *mut TargetObj,
    image: *mut c_void,
    offset: *const [f32; 2],
    rect: *const RectF,
    interp: u32,
    _composite: u32,
) {
    let hint = {
        let t = target(this);
        (t.w, t.h)
    };
    let img = image_img(image, 0, hint);
    d2d_trace!(
        "DrawImage {:p} ← {} offset {:?} src {:?} → {}",
        this,
        describe_image(image),
        offset.as_ref(),
        rect.as_ref().map(|r| (r.l, r.t, r.r, r.b)),
        img.as_ref().map_or("rien".to_string(), |i| format!(
            "{}x{} @{},{} visibles {}",
            i.w,
            i.h,
            i.x,
            i.y,
            visible(&i.px)
        ))
    );
    let Some(img) = img else {
        return;
    };
    // Rectangle source dans l'espace de l'image (son origine peut ne pas
    // être 0, 0 pour une sortie d'effet).
    let (ix, iy) = (img.x as f32, img.y as f32);
    let src = rect.as_ref().copied().unwrap_or(RectF {
        l: ix,
        t: iy,
        r: ix + img.w as f32,
        b: iy + img.h as f32,
    });
    let (ox, oy) = offset.as_ref().map_or((0.0, 0.0), |o| (o[0], o[1]));
    // Avec un rectangle source, son coin haut gauche est placé au décalage
    // (Paint.NET passe le même point aux deux, #1651) ; sans rectangle,
    // l'image entière est translatée du décalage.
    let (dx, dy) = if rect.is_null() {
        (ox, oy)
    } else {
        (ox - src.l, oy - src.t)
    };
    let dst = RectF {
        l: dx + src.l,
        t: dy + src.t,
        r: dx + src.r,
        b: dy + src.b,
    };
    let local = RectF {
        l: src.l - ix,
        t: src.t - iy,
        r: src.r - ix,
        b: src.b - iy,
    };
    draw_bitmap_pixels(
        this,
        &img.px,
        img.w,
        img.h,
        &dst,
        1.0,
        interpolation(interp),
        &local,
    );
}

unsafe extern "win64" fn dc_blend_image(
    this: *mut TargetObj,
    image: *mut c_void,
    _blend: u32,
    offset: *const [f32; 2],
    rect: *const RectF,
    interp: u32,
) {
    dc_draw_image(this, image, offset, rect, interp, 0);
}

/// ID2D1DeviceContext::DrawBitmap : la transformation en perspective est
/// ignorée.
unsafe extern "win64" fn dc_draw_bitmap(
    this: *mut TargetObj,
    bitmap: *mut c_void,
    dst: *const RectF,
    opacity: f32,
    interp: u32,
    src: *const RectF,
    _perspective: *const c_void,
) {
    super::draw_bitmap(this, bitmap, dst, opacity, interpolation(interp), src);
}

unsafe extern "win64" fn dc_image_local_bounds(
    _this: *mut TargetObj,
    image: *mut c_void,
    out: *mut RectF,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let hint = {
        let t = target(_this);
        (t.w, t.h)
    };
    *out = image_img(image, 0, hint).map_or(
        RectF {
            l: 0.0,
            t: 0.0,
            r: 0.0,
            b: 0.0,
        },
        |i| RectF {
            l: i.x as f32,
            t: i.y as f32,
            r: (i.x + i.w as i32) as f32,
            b: (i.y + i.h as i32) as f32,
        },
    );
    S_OK
}

unsafe extern "win64" fn dc_image_world_bounds(
    this: *mut TargetObj,
    image: *mut c_void,
    out: *mut RectF,
) -> i32 {
    let hr = dc_image_local_bounds(this, image, out);
    if hr < 0 {
        return hr;
    }
    let m = target(this).transform();
    let r = *out;
    let pts = [
        m.apply((r.l, r.t)),
        m.apply((r.r, r.t)),
        m.apply((r.l, r.b)),
        m.apply((r.r, r.b)),
    ];
    let fold = |f: fn(f32, f32) -> f32, i: usize| {
        pts.iter()
            .map(|p| if i == 0 { p.0 } else { p.1 })
            .reduce(f)
            .unwrap_or(0.0)
    };
    *out = RectF {
        l: fold(f32::min, 0),
        t: fold(f32::min, 1),
        r: fold(f32::max, 0),
        b: fold(f32::max, 1),
    };
    S_OK
}

// --- Contexte d'appareil : cible, réglages --------------------------------------------

/// GetDevice : l'appareil parent. Une cible 1.0 (fenêtre, HDC, WIC) vue en
/// contexte d'appareil n'en a pas : un appareil sans DXGI, rattaché à la
/// fabrique de la cible, lui est créé à la première demande puis rendu à
/// chaque appel (#1554, canevas de Paint.NET).
unsafe extern "win64" fn dc_get_device(this: *mut TargetObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let mut t = target(this);
    if t.ext.device.is_null() {
        let dev = D2Device {
            dxgi: std::ptr::null_mut(),
        };
        t.ext.device = new_res(Com::get(this).factory, DEVICE_VT.ptr(), dev).cast();
    }
    let d = t.ext.device;
    com_add_ref(d);
    *out = d;
}

/// SetTarget : la cible prend la taille et le contenu du bitmap ; EndDraw
/// les lui rend (et à sa texture s'il vient d'une surface DXGI). Une cible
/// fenêtre ou HDC vue en contexte d'appareil peut aussi dessiner dans un
/// bitmap : sa fenêtre est mise de côté avec ses pixels et revient à
/// SetTarget(NULL) ou vers une autre image (#1590, canevas et panneaux de
/// Paint.NET, qui dessinaient jusque-là dans la fenêtre).
unsafe extern "win64" fn dc_set_target(this: *mut TargetObj, image: *mut c_void) {
    d2d_trace!("SetTarget {:p} ← {}", this, describe_image(image));
    let mut t = target(this);
    // Le bitmap rendu par GetTarget pour la fenêtre ou le HDC la rétablit
    // (#1609), comme SetTarget(NULL).
    let to_home = !image.is_null() && image == t.ext.home_bitmap;
    let to_bitmap = !to_home && is_pixel_image(image);
    if !to_bitmap && !to_home && !image.is_null() {
        crate::log_warn!(
            "d2d1",
            "SetTarget : seule une ID2D1Bitmap1 ou une liste de commandes peut être cible"
        );
    }
    // Ce qui a été dessiné dans le bitmap quitté lui revient tout de suite.
    if t.drawing && matches!(t.out, Output::Bitmap(_)) {
        t.sync_wic(true);
    }
    let old = if matches!(t.out, Output::Hwnd(_) | Output::Dc { .. } | Output::Wic(_)) {
        let out = std::mem::replace(&mut t.out, Output::None);
        let px = std::mem::take(&mut t.px);
        t.ext.home = Some(Box::new(Home {
            out,
            w: t.w,
            h: t.h,
            px,
        }));
        Output::None
    } else {
        std::mem::replace(&mut t.out, Output::None)
    };
    if to_bitmap {
        let b = &Com::get(image as *mut BitmapObj).data;
        t.w = b.w;
        t.h = b.h;
        t.px = crate::dlls::lock(&b.px).clone();
        com_add_ref(image);
        t.out = Output::Bitmap(image);
    } else if let Some(mut home) = t.ext.home.take() {
        swap_home(&mut t, &mut home);
    } else {
        t.w = 0;
        t.h = 0;
        t.px = Vec::new();
    }
    drop(t);
    if let Output::Bitmap(b) = old {
        com_release(b);
    }
}

unsafe extern "win64" fn dc_get_target(this: *mut TargetObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let mut t = target(this);
    *out = match t.out {
        Output::Bitmap(b) => {
            com_add_ref(b);
            b
        }
        // Une cible fenêtre, HDC ou WIC a toujours une cible : un bitmap qui
        // la représente, que Paint.NET garde pour la remettre après avoir
        // dessiné dans ses propres bitmaps (#1609, #1669 pour WIC).
        Output::Hwnd(_) | Output::Dc { .. } | Output::Wic(_) => {
            if t.ext.home_bitmap.is_null() {
                let (w, h) = (t.w.max(1), t.h.max(1));
                let mut b = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
                // D2D1_BITMAP_OPTIONS_TARGET | CANNOT_DRAW.
                b.options = 1 | 2;
                t.ext.home_bitmap = new_res(Com::get(this).factory, BITMAP_VT.ptr(), b).cast();
            }
            let b = t.ext.home_bitmap;
            com_add_ref(b);
            b
        }
        _ => std::ptr::null_mut(),
    };
    d2d_trace!("GetTarget {:p} → {:p} {}", this, *out, describe_target(&t));
}

unsafe extern "win64" fn dc_set_controls(this: *mut TargetObj, c: *const [u32; 3]) {
    if let Some(c) = c.as_ref() {
        target(this).ext.controls = *c;
    }
}
unsafe extern "win64" fn dc_get_controls(this: *mut TargetObj, c: *mut [u32; 3]) {
    if !c.is_null() {
        *c = target(this).ext.controls;
    }
}
unsafe extern "win64" fn dc_set_blend(this: *mut TargetObj, v: u32) {
    target(this).ext.primitive_blend = v;
}
unsafe extern "win64" fn dc_get_blend(this: *mut TargetObj) -> u32 {
    target(this).ext.primitive_blend
}
unsafe extern "win64" fn dc_set_unit(this: *mut TargetObj, v: u32) {
    target(this).ext.unit_mode = v;
}
unsafe extern "win64" fn dc_get_unit(this: *mut TargetObj) -> u32 {
    target(this).ext.unit_mode
}
pub(super) unsafe extern "win64" fn set_dpi(this: *mut TargetObj, x: f32, y: f32) {
    target(this).ext.dpi = (x, y);
}
pub(super) unsafe extern "win64" fn get_dpi(this: *mut TargetObj, x: *mut f32, y: *mut f32) {
    let (dx, dy) = target(this).ext.dpi;
    if !x.is_null() {
        *x = dx;
    }
    if !y.is_null() {
        *y = dy;
    }
}

unsafe extern "win64" fn dc_format_supported(_: *mut TargetObj, format: u32) -> i32 {
    // BGRA/RGBA 8 bits (et sRGB), A8.
    matches!(format, 28 | 29 | 87 | 91 | 65) as i32
}
unsafe extern "win64" fn dc_precision_supported(_: *mut TargetObj, p: u32) -> i32 {
    // D2D1_BUFFER_PRECISION_8BPC_UNORM(_SRGB) seulement : le rendu est 8 bits.
    matches!(p, 1 | 2) as i32
}

/// D2D1_LAYER_PARAMETERS(1) : la zone de contenu devient une découpe ; le
/// masque géométrique et l'opacité ne sont pas encore appliqués.
pub(super) unsafe extern "win64" fn push_layer(
    this: *mut TargetObj,
    params: *const RectF,
    _layer: *mut c_void,
) {
    d2d_trace!(
        "PushLayer {:p} {:?}",
        this,
        params.as_ref().map(|r| (r.l, r.t, r.r, r.b))
    );
    let r = params.as_ref().copied().unwrap_or(RectF {
        l: -f32::MAX,
        t: -f32::MAX,
        r: f32::MAX,
        b: f32::MAX,
    });
    let r = RectF {
        l: r.l.max(-1.0e7),
        t: r.t.max(-1.0e7),
        r: r.r.min(1.0e7),
        b: r.b.min(1.0e7),
    };
    push_clip(this, &r, 0);
}
pub(super) unsafe extern "win64" fn pop_layer(this: *mut TargetObj) {
    pop_clip(this);
}

/// SaveDrawingState : transformation, étiquettes et réglages 1.1.
pub(super) unsafe extern "win64" fn save_state(this: *mut TargetObj, block: *mut c_void) {
    if block.is_null() || *(block as *const *const ()) != STATE_BLOCK_VT.ptr() {
        return;
    }
    let t = target(this);
    let b = &Com::get(block as *mut StateBlockObj).data;
    let mut d = crate::dlls::lock(&b.desc);
    d.transform = t.transform();
    d.tag1 = t.state.tags.0;
    d.tag2 = t.state.tags.1;
    d.primitive_blend = t.ext.primitive_blend;
    d.unit_mode = t.ext.unit_mode;
}
pub(super) unsafe extern "win64" fn restore_state(this: *mut TargetObj, block: *mut c_void) {
    if block.is_null() || *(block as *const *const ()) != STATE_BLOCK_VT.ptr() {
        return;
    }
    let d = *crate::dlls::lock(&Com::get(block as *mut StateBlockObj).data.desc);
    let mut t = target(this);
    t.state.transform = Some(d.transform);
    t.state.tags = (d.tag1, d.tag2);
    t.ext.primitive_blend = d.primitive_blend;
    t.ext.unit_mode = d.unit_mode;
}

// Texte : les variantes de ID2D1DeviceContext4 reprennent celles de 1.0.
unsafe extern "win64" fn dc_draw_text4(
    this: *mut TargetObj,
    s: *const u16,
    len: u32,
    format: *mut c_void,
    rect: *const RectF,
    brush: *mut c_void,
    _svg: *mut c_void,
    _palette: u32,
    options: u32,
    measuring: u32,
) {
    draw_text(this, s, len, format, rect, brush, options, measuring);
}
unsafe extern "win64" fn dc_draw_layout4(
    this: *mut TargetObj,
    origin: u64,
    layout: *mut c_void,
    brush: *mut c_void,
    _svg: *mut c_void,
    _palette: u32,
    options: u32,
) {
    draw_text_layout(this, origin, layout, brush, options);
}
unsafe extern "win64" fn dc_draw_glyph_run(
    this: *mut TargetObj,
    origin: u64,
    run: *const super::super::dwrite::GlyphRun,
    _desc: *const c_void,
    brush: *mut c_void,
    measuring: u32,
) {
    draw_glyph_run(this, origin, run, brush, measuring);
}

unsafe extern "win64" fn dc_ok(_: *mut TargetObj) -> i32 {
    S_OK
}
unsafe extern "win64" fn dc_invalid_count(_: *mut TargetObj, _: *mut c_void, n: *mut u32) -> i32 {
    if !n.is_null() {
        *n = 0;
    }
    S_OK
}

/// CreateGradientStopCollection (1.1) : arrêts, espaces, précision, mode.
unsafe extern "win64" fn dc_gradient_stops1(
    this: *mut TargetObj,
    stops: *const GradientStop,
    n: u32,
    pre: u32,
    post: u32,
    precision: u32,
    extend: u32,
    mode: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_gradient(
        Com::get(this).factory,
        stops,
        n,
        0,
        extend,
        [pre, post, precision, mode],
        out,
    )
}

/// ID2D1RenderTarget::CreateGradientStopCollection.
pub(super) unsafe extern "win64" fn gradient_stops(
    this: *mut TargetObj,
    stops: *const GradientStop,
    n: u32,
    gamma: u32,
    extend: u32,
    out: *mut *mut c_void,
) -> i32 {
    // Gamma 2.2 (0) : espaces sRGB ; 1.0 : espace linéaire.
    let space = if gamma == 0 { 0 } else { 1 };
    new_gradient(
        Com::get(this).factory,
        stops,
        n,
        gamma,
        extend,
        [space, space, 1, 0],
        out,
    )
}

// --- Table du contexte d'appareil -----------------------------------------------------

const DC_EXTRA: [*const (); 63] = [
    dc_create_bitmap as *const (),                     // 57 CreateBitmap
    dc_bitmap_from_wic as *const (),                   // 58 CreateBitmapFromWicBitmap
    dc_color_context as *const (),                     // 59 CreateColorContext
    dc_color_context_file as *const (),                // 60 CreateColorContextFromFilename
    dc_color_context_wic as *const (),                 // 61 CreateColorContextFromWicColorContext
    dc_bitmap_from_dxgi as *const (),                  // 62 CreateBitmapFromDxgiSurface
    dc_create_effect as *const (),                     // 63 CreateEffect
    dc_gradient_stops1 as *const (),                   // 64 CreateGradientStopCollection
    super::brushes::create_image_brush as *const (),   // 65 CreateImageBrush
    super::brushes::create_bitmap_brush1 as *const (), // 66 CreateBitmapBrush
    dc_create_command_list as *const (),               // 67 CreateCommandList
    dc_format_supported as *const (),                  // 68 IsDxgiFormatSupported
    dc_precision_supported as *const (),               // 69 IsBufferPrecisionSupported
    dc_image_local_bounds as *const (),                // 70 GetImageLocalBounds
    dc_image_world_bounds as *const (),                // 71 GetImageWorldBounds
    unsupported!("GetGlyphRunWorldBounds"),            // 72
    dc_get_device as *const (),                        // 73 GetDevice
    dc_set_target as *const (),                        // 74 SetTarget
    dc_get_target as *const (),                        // 75 GetTarget
    dc_set_controls as *const (),                      // 76 SetRenderingControls
    dc_get_controls as *const (),                      // 77 GetRenderingControls
    dc_set_blend as *const (),                         // 78 SetPrimitiveBlend
    dc_get_blend as *const (),                         // 79 GetPrimitiveBlend
    dc_set_unit as *const (),                          // 80 SetUnitMode
    dc_get_unit as *const (),                          // 81 GetUnitMode
    dc_draw_glyph_run as *const (),                    // 82 DrawGlyphRun
    dc_draw_image as *const (),                        // 83 DrawImage
    nothing as *const (),                              // 84 DrawGdiMetafile
    dc_draw_bitmap as *const (),                       // 85 DrawBitmap
    push_layer as *const (),                           // 86 PushLayer
    dc_ok as *const (),                                // 87 InvalidateEffectInputRectangle
    dc_invalid_count as *const (),                     // 88 GetEffectInvalidRectangleCount
    dc_ok as *const (),                                // 89 GetEffectInvalidRectangles
    dc_ok as *const (),                                // 90 GetEffectRequiredInputRectangles
    nothing as *const (),                              // 91 FillOpacityMask
    unsupported!("CreateFilledGeometryRealization"),   // 92
    unsupported!("CreateStrokedGeometryRealization"),  // 93
    nothing as *const (),                              // 94 DrawGeometryRealization
    unsupported!("CreateInk"),                         // 95
    unsupported!("CreateInkStyle"),                    // 96
    unsupported!("CreateGradientMesh"),                // 97
    unsupported!("CreateImageSourceFromWic"),          // 98
    unsupported!("CreateLookupTable3D"),               // 99
    unsupported!("CreateImageSourceFromDxgi"),         // 100
    unsupported!("GetGradientMeshWorldBounds"),        // 101
    nothing as *const (),                              // 102 DrawInk
    nothing as *const (),                              // 103 DrawGradientMesh
    nothing as *const (),                              // 104 DrawGdiMetafile
    unsupported!("CreateTransformedImageSource"),      // 105
    unsupported!("CreateSpriteBatch"),                 // 106
    nothing as *const (),                              // 107 DrawSpriteBatch
    unsupported!("CreateSvgGlyphStyle"),               // 108
    dc_draw_text4 as *const (),                        // 109 DrawText
    dc_draw_layout4 as *const (),                      // 110 DrawTextLayout
    nothing as *const (),                              // 111 DrawColorBitmapGlyphRun
    nothing as *const (),                              // 112 DrawSvgGlyphRun
    unsupported!("GetColorBitmapGlyphImage"),          // 113
    unsupported!("GetSvgGlyphImage"),                  // 114
    unsupported!("CreateSvgDocument"),                 // 115
    nothing as *const (),                              // 116 DrawSvgDocument
    dc_color_context_dxgi as *const (),                // 117 CreateColorContextFromDxgiColorSpace
    dc_color_context_simple as *const (), // 118 CreateColorContextFromSimpleColorProfile
    dc_blend_image as *const (),          // 119 BlendImage
];

pub(super) static DC_VT: VTable<120> = VTable(super::derived::<120>(&DC_EXTRA));

// --- Styles de trait ---------------------------------------------------------------------

type StrokeObj = Com<Res<StrokeStyle>>;

unsafe extern "win64" fn stroke_qi(
    this: *mut StrokeObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_STROKE_STYLE,
            IID_D2D1_STROKE_STYLE1,
        ],
    )
}
macro_rules! stroke_get {
    ($name:ident, $field:ident, $ty:ty) => {
        unsafe extern "win64" fn $name(this: *mut StrokeObj) -> $ty {
            Com::get(this).data.props.$field
        }
    };
}
stroke_get!(stroke_start_cap, start_cap, u32);
stroke_get!(stroke_end_cap, end_cap, u32);
stroke_get!(stroke_dash_cap, dash_cap, u32);
stroke_get!(stroke_miter, miter_limit, f32);
stroke_get!(stroke_join, line_join, u32);
stroke_get!(stroke_dash_offset, dash_offset, f32);
stroke_get!(stroke_dash_style, dash_style, u32);
stroke_get!(stroke_transform, transform_type, u32);
unsafe extern "win64" fn stroke_dash_count(this: *mut StrokeObj) -> u32 {
    Com::get(this).data.dashes.len() as u32
}
unsafe extern "win64" fn stroke_dashes(this: *mut StrokeObj, out: *mut f32, n: u32) {
    if out.is_null() {
        return;
    }
    let d = &Com::get(this).data.dashes;
    for i in 0..n as usize {
        *out.add(i) = d.get(i).copied().unwrap_or(0.0);
    }
}

static STROKE_VT: VTable<14> = VTable([
    stroke_qi as *const (),
    add_ref::<Res<StrokeStyle>> as *const (),
    release::<Res<StrokeStyle>> as *const (),
    get_factory::<StrokeStyle> as *const (),
    stroke_start_cap as *const (),
    stroke_end_cap as *const (),
    stroke_dash_cap as *const (),
    stroke_miter as *const (),
    stroke_join as *const (),
    stroke_dash_offset as *const (),
    stroke_dash_style as *const (),
    stroke_dash_count as *const (),
    stroke_dashes as *const (),
    stroke_transform as *const (),
]);

// --- Blocs d'état de dessin ---------------------------------------------------------------

unsafe extern "win64" fn block_qi(
    this: *mut StateBlockObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_STATE_BLOCK,
            IID_D2D1_STATE_BLOCK1,
        ],
    )
}
unsafe fn block_get(this: *mut StateBlockObj, out: *mut u8, size: usize) {
    if !out.is_null() {
        let d = *crate::dlls::lock(&Com::get(this).data.desc);
        std::ptr::copy_nonoverlapping(&d as *const StateDesc as *const u8, out, size);
    }
}
unsafe fn block_set(this: *mut StateBlockObj, src: *const u8, size: usize) {
    if !src.is_null() {
        let mut d = crate::dlls::lock(&Com::get(this).data.desc);
        std::ptr::copy_nonoverlapping(src, &mut *d as *mut StateDesc as *mut u8, size);
    }
}
unsafe extern "win64" fn block_get_desc(this: *mut StateBlockObj, out: *mut u8) {
    block_get(this, out, STATE_DESC_SIZE);
}
unsafe extern "win64" fn block_set_desc(this: *mut StateBlockObj, src: *const u8) {
    block_set(this, src, STATE_DESC_SIZE);
}
unsafe extern "win64" fn block_get_desc1(this: *mut StateBlockObj, out: *mut u8) {
    block_get(this, out, STATE_DESC1_SIZE);
}
unsafe extern "win64" fn block_set_desc1(this: *mut StateBlockObj, src: *const u8) {
    block_set(this, src, STATE_DESC1_SIZE);
}
unsafe extern "win64" fn block_set_params(this: *mut StateBlockObj, p: *mut c_void) {
    if !p.is_null() {
        com_add_ref(p);
    }
    let old = std::mem::replace(&mut *crate::dlls::lock(&Com::get(this).data.params), p);
    if !old.is_null() {
        com_release(old);
    }
}
unsafe extern "win64" fn block_get_params(this: *mut StateBlockObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let p = *crate::dlls::lock(&Com::get(this).data.params);
    if !p.is_null() {
        com_add_ref(p);
    }
    *out = p;
}

static STATE_BLOCK_VT: VTable<10> = VTable([
    block_qi as *const (),
    add_ref::<Res<StateBlock>> as *const (),
    release::<Res<StateBlock>> as *const (),
    get_factory::<StateBlock> as *const (),
    block_get_desc as *const (),
    block_set_desc as *const (),
    block_set_params as *const (),
    block_get_params as *const (),
    block_get_desc1 as *const (),
    block_set_desc1 as *const (),
]);

// --- Collections d'arrêts de dégradé ------------------------------------------------------

/// D2D1_GRADIENT_STOP : position puis couleur.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct GradientStop {
    position: f32,
    color: [f32; 4],
}

struct Gradient {
    stops: Vec<GradientStop>,
    gamma: u32,
    extend: u32,
    /// Espaces avant/après interpolation, précision, mode d'interpolation.
    modes: [u32; 4],
}

type GradientObj = Com<Res<Gradient>>;

unsafe fn new_gradient(
    f: *mut FactoryObj,
    stops: *const GradientStop,
    n: u32,
    gamma: u32,
    extend: u32,
    modes: [u32; 4],
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if n == 0 || stops.is_null() {
        return E_INVALIDARG;
    }
    let mut v = std::slice::from_raw_parts(stops, n as usize).to_vec();
    v.sort_by(|a, b| a.position.total_cmp(&b.position));
    let g = Gradient {
        stops: v,
        gamma,
        extend,
        modes,
    };
    put(out, new_res(f, GRADIENT_VT.ptr(), g))
}

unsafe extern "win64" fn gradient_qi(
    this: *mut GradientObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_GRADIENT_STOPS,
            IID_D2D1_GRADIENT_STOPS1,
        ],
    )
}
/// Arrêts (position, couleur RVBA non prémultipliée) et mode d'extension
/// d'une ID2D1GradientStopCollection Winlux, pour les pinceaux dégradés.
pub(super) unsafe fn gradient_of(obj: *mut c_void) -> Option<(Vec<(f32, [f32; 4])>, u32)> {
    if obj.is_null() || *(obj as *const *const ()) != GRADIENT_VT.ptr() {
        return None;
    }
    let g = &Com::get(obj as *mut GradientObj).data;
    Some((
        g.stops.iter().map(|s| (s.position, s.color)).collect(),
        g.extend,
    ))
}

unsafe extern "win64" fn gradient_count(this: *mut GradientObj) -> u32 {
    Com::get(this).data.stops.len() as u32
}
unsafe extern "win64" fn gradient_get(this: *mut GradientObj, out: *mut GradientStop, n: u32) {
    if out.is_null() {
        return;
    }
    for (i, s) in Com::get(this)
        .data
        .stops
        .iter()
        .take(n as usize)
        .enumerate()
    {
        *out.add(i) = *s;
    }
}
unsafe extern "win64" fn gradient_gamma(this: *mut GradientObj) -> u32 {
    Com::get(this).data.gamma
}
unsafe extern "win64" fn gradient_extend(this: *mut GradientObj) -> u32 {
    Com::get(this).data.extend
}
macro_rules! gradient_mode {
    ($name:ident, $i:expr) => {
        unsafe extern "win64" fn $name(this: *mut GradientObj) -> u32 {
            Com::get(this).data.modes[$i]
        }
    };
}
gradient_mode!(gradient_pre, 0);
gradient_mode!(gradient_post, 1);
gradient_mode!(gradient_precision, 2);
gradient_mode!(gradient_mode, 3);

static GRADIENT_VT: VTable<13> = VTable([
    gradient_qi as *const (),
    add_ref::<Res<Gradient>> as *const (),
    release::<Res<Gradient>> as *const (),
    get_factory::<Gradient> as *const (),
    gradient_count as *const (),
    gradient_get as *const (),
    gradient_gamma as *const (),
    gradient_extend as *const (),
    gradient_get as *const (), // GetGradientStops1 (couleurs non retouchées)
    gradient_pre as *const (),
    gradient_post as *const (),
    gradient_precision as *const (),
    gradient_mode as *const (),
]);

// --- Contextes de couleur -----------------------------------------------------------------

struct ColorContext {
    /// D2D1_COLOR_SPACE : 0 personnalisé (profil), 1 sRGB, 2 scRGB.
    space: u32,
    profile: Vec<u8>,
    /// DXGI_COLOR_SPACE_TYPE (ID2D1ColorContext1).
    dxgi_space: u32,
}

type ColorObj = Com<Res<ColorContext>>;

unsafe fn new_color(this: *mut TargetObj, c: ColorContext, out: *mut *mut c_void) -> i32 {
    new_color_in(
        Com::get(this).factory,
        c.space,
        c.profile,
        c.dxgi_space,
        out,
    )
}

/// Contexte de couleur rattaché à une fabrique : sert aussi au contexte des
/// effets personnalisés (ID2D1EffectContext::CreateColorContext*, #1206).
pub(super) unsafe fn new_color_in(
    factory: *mut FactoryObj,
    space: u32,
    profile: Vec<u8>,
    dxgi_space: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // sRGB et scRGB se créent sans profil, mais GetProfile rend le profil
    // ICC de l'espace : Paint.NET le hache et refuse moins de 128 octets
    // (#1354).
    let profile = match (space, profile.is_empty()) {
        (1, true) => crate::dlls::icc::srgb(),
        (2, true) => crate::dlls::icc::scrgb(),
        _ => profile,
    };
    put(
        out,
        new_res(
            factory,
            COLOR_VT.ptr(),
            ColorContext {
                space,
                profile,
                dxgi_space,
            },
        ),
    )
}

/// Profil ICC lu depuis un chemin Windows (CreateColorContextFromFilename).
pub(super) unsafe fn read_profile(path: *const u16) -> Option<Vec<u8>> {
    if path.is_null() {
        return None;
    }
    std::fs::read(crate::paths::to_linux(&wide(path))).ok()
}

unsafe extern "win64" fn dc_color_context(
    this: *mut TargetObj,
    space: u32,
    profile: *const u8,
    size: u32,
    out: *mut *mut c_void,
) -> i32 {
    let profile = if profile.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(profile, size as usize).to_vec()
    };
    let dxgi_space = if space == 2 { 10 } else { 0 };
    new_color(
        this,
        ColorContext {
            space,
            profile,
            dxgi_space,
        },
        out,
    )
}
unsafe extern "win64" fn dc_color_context_file(
    this: *mut TargetObj,
    path: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    match read_profile(path) {
        Some(bytes) => new_color(
            this,
            ColorContext {
                space: 0,
                profile: bytes,
                dxgi_space: 0,
            },
            out,
        ),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            0x8007_0002u32 as i32
        }
    }
}
unsafe extern "win64" fn dc_color_context_wic(
    this: *mut TargetObj,
    _wic: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    // Profil WIC non lu : traité comme sRGB.
    new_color(
        this,
        ColorContext {
            space: 1,
            profile: Vec::new(),
            dxgi_space: 0,
        },
        out,
    )
}
unsafe extern "win64" fn dc_color_context_dxgi(
    this: *mut TargetObj,
    space: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_color(
        this,
        ColorContext {
            space: if space == 1 { 2 } else { 1 },
            profile: Vec::new(),
            dxgi_space: space,
        },
        out,
    )
}
unsafe extern "win64" fn dc_color_context_simple(
    this: *mut TargetObj,
    _profile: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    new_color(
        this,
        ColorContext {
            space: 0,
            profile: Vec::new(),
            dxgi_space: 0,
        },
        out,
    )
}

unsafe extern "win64" fn color_qi(
    this: *mut ColorObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_COLOR_CONTEXT,
            IID_D2D1_COLOR_CONTEXT1,
        ],
    )
}
unsafe extern "win64" fn color_space(this: *mut ColorObj) -> u32 {
    Com::get(this).data.space
}
unsafe extern "win64" fn color_profile_size(this: *mut ColorObj) -> u32 {
    Com::get(this).data.profile.len() as u32
}
unsafe extern "win64" fn color_profile(this: *mut ColorObj, out: *mut u8, size: u32) -> i32 {
    let p = &Com::get(this).data.profile;
    if (size as usize) < p.len() || (out.is_null() && !p.is_empty()) {
        return D2DERR_INSUFFICIENT_BUFFER;
    }
    if !p.is_empty() {
        std::ptr::copy_nonoverlapping(p.as_ptr(), out, p.len());
    }
    S_OK
}
unsafe extern "win64" fn color_type(this: *mut ColorObj) -> u32 {
    // D2D1_COLOR_CONTEXT_TYPE : ICC (0), SIMPLE (1), DXGI (2).
    if Com::get(this).data.dxgi_space != 0 {
        2
    } else {
        0
    }
}
unsafe extern "win64" fn color_dxgi(this: *mut ColorObj) -> u32 {
    Com::get(this).data.dxgi_space
}
unsafe extern "win64" fn color_simple(_: *mut ColorObj, _: *mut c_void) -> i32 {
    0x8899_0005u32 as i32 // D2DERR_INVALID_CALL : pas un profil simple
}

static COLOR_VT: VTable<10> = VTable([
    color_qi as *const (),
    add_ref::<Res<ColorContext>> as *const (),
    release::<Res<ColorContext>> as *const (),
    get_factory::<ColorContext> as *const (),
    color_space as *const (),
    color_profile_size as *const (),
    color_profile as *const (),
    color_type as *const (),
    color_dxgi as *const (),
    color_simple as *const (),
]);

// --- Fonctions exportées ------------------------------------------------------------------

/// D2D1CreateDevice : une fabrique multifil puis son appareil.
pub unsafe extern "win64" fn d2d1_create_device(
    dxgi: *mut c_void,
    _props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let f = Com::create(super::FACTORY_VT.ptr(), super::Factory);
    let hr = factory_create_device(f, dxgi, out);
    Com::release(f);
    hr
}

/// D2D1CreateDeviceContext : appareil sur l'IDXGIDevice de la surface, puis
/// contexte dont la cible est cette surface.
pub unsafe extern "win64" fn d2d1_create_device_context(
    surface: *mut c_void,
    props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if super::super::d3d11::d2d_surface_size(surface).is_none() {
        return E_INVALIDARG;
    }
    // L'appareil D3D11 n'est pas requis par le rendu logiciel : la surface
    // sert de cible, et GetDxgiDevice rend la surface elle-même.
    let mut dev = std::ptr::null_mut();
    let hr = d2d1_create_device(surface, props, &mut dev);
    if hr < 0 {
        return hr;
    }
    let mut dc = std::ptr::null_mut();
    let hr = device_create_context(dev.cast(), 0, &mut dc);
    com_release(dev);
    if hr < 0 {
        return hr;
    }
    let mut bmp = std::ptr::null_mut();
    let hr = dc_bitmap_from_dxgi(dc.cast(), surface, std::ptr::null(), &mut bmp);
    if hr >= 0 {
        dc_set_target(dc.cast(), bmp);
        com_release(bmp);
    }
    *out = dc;
    S_OK
}

/// EndDraw d'un contexte dont la cible vient d'une surface DXGI.
pub(super) unsafe fn flush_surface(bitmap: *mut c_void) {
    let b = &Com::get(bitmap as *mut BitmapObj).data;
    if !b.surface.is_null() {
        let px = crate::dlls::lock(&b.px);
        super::super::d3d11::d2d_surface_write(b.surface, &px);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_have_windows_sizes() {
        // ID2D1DeviceContext6 : 120 méthodes ; ID2D1Device6 : 21 ;
        // ID2D1Effect : 19 ; ID2D1StrokeStyle1 : 14 ; ID2D1DrawingStateBlock1 : 10 ;
        // ID2D1GradientStopCollection1 : 13 ; ID2D1ColorContext1 : 10.
        assert_eq!(DC_VT.0.len(), 120);
        assert_eq!(DEVICE_VT.0.len(), 21);
        assert_eq!(EFFECT_VT.0.len(), 19);
        assert_eq!(STROKE_VT.0.len(), 14);
        assert_eq!(STATE_BLOCK_VT.0.len(), 10);
        assert_eq!(GRADIENT_VT.0.len(), 13);
        assert_eq!(COLOR_VT.0.len(), 10);
        assert!(DC_VT.0.iter().all(|p| !p.is_null()));
        assert_eq!(std::mem::size_of::<StateDesc>(), STATE_DESC1_SIZE);
        assert_eq!(std::mem::size_of::<GradientStop>(), 20);
        assert_eq!(std::mem::size_of::<StrokeProps>(), 32);
    }

    #[test]
    fn factory_answers_every_version() {
        for g in FACTORY_IIDS {
            assert!(is_factory_n_iid(&g));
        }
        // ID2D1Factory7, tel que demandé par Paint.NET 5.1.
        assert_eq!(FACTORY_IIDS[6].data1, 0xbdc2_bdd3);
    }

    /// #1590 : une cible HDC vue en contexte d'appareil dessine dans un
    /// bitmap choisi par SetTarget, puis retrouve ses pixels à SetTarget(NULL).
    #[test]
    fn cible_hdc_dessine_dans_un_bitmap_puis_revient() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut rt = std::ptr::null_mut();
            assert_eq!(
                super::super::create_dc_render_target(f, std::ptr::null(), &mut rt),
                S_OK
            );
            let rt: *mut TargetObj = rt.cast();
            target(rt).resize(2, 2);
            let color = |r: f32, b: f32| super::super::ColorF {
                r,
                g: 0.0,
                b,
                a: 1.0,
            };
            super::super::begin_draw(rt);
            super::super::clear(rt, &color(1.0, 0.0));
            let mut bmp = std::ptr::null_mut();
            let size = 4u64 | (3u64 << 32);
            assert_eq!(
                dc_create_bitmap(rt, size, std::ptr::null(), 0, std::ptr::null(), &mut bmp),
                S_OK
            );
            dc_set_target(rt, bmp);
            {
                let t = target(rt);
                assert_eq!((t.w, t.h), (4, 3));
            }
            let mut got = std::ptr::null_mut();
            dc_get_target(rt, &mut got);
            assert_eq!(got, bmp);
            com_release(got);
            super::super::clear(rt, &color(0.0, 1.0));
            dc_set_target(rt, std::ptr::null_mut());
            {
                let t = target(rt);
                assert_eq!((t.w, t.h), (2, 2));
                assert!(matches!(t.out, Output::Dc { .. }));
                assert_eq!(&t.px[0..4], &[0, 0, 255, 255]);
            }
            let (_, _, px) = super::super::bitmap_pixels(bmp).unwrap();
            assert_eq!(&px[0..4], &[255, 0, 0, 255]);
            assert_eq!(
                super::super::end_draw(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            com_release(bmp);
            com_release(rt.cast());
            Com::release(f);
        }
    }

    /// #1609 : GetTarget d'une cible HDC rend un bitmap qui la rétablit, et
    /// BindDC pendant qu'un bitmap sert de cible rétablit d'abord le HDC.
    #[test]
    fn cible_hdc_get_target_et_bind_dc_retablissent_le_hdc() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut rt = std::ptr::null_mut();
            assert_eq!(
                super::super::create_dc_render_target(f, std::ptr::null(), &mut rt),
                S_OK
            );
            let rt: *mut TargetObj = rt.cast();
            target(rt).resize(2, 2);
            let mut home = std::ptr::null_mut();
            dc_get_target(rt, &mut home);
            assert!(!home.is_null());
            let mut bmp = std::ptr::null_mut();
            let size = 4u64 | (3u64 << 32);
            assert_eq!(
                dc_create_bitmap(rt, size, std::ptr::null(), 0, std::ptr::null(), &mut bmp),
                S_OK
            );
            dc_set_target(rt, bmp);
            dc_set_target(rt, home);
            {
                let t = target(rt);
                assert_eq!((t.w, t.h), (2, 2));
                assert!(matches!(t.out, Output::Dc { .. }));
            }
            dc_set_target(rt, bmp);
            let rect = [0, 0, 5, 6];
            assert_eq!(super::super::bind_dc(rt, 0x1234, &rect), S_OK);
            {
                let t = target(rt);
                assert_eq!((t.w, t.h), (5, 6));
                assert!(matches!(t.out, Output::Dc { hdc: 0x1234, .. }));
                assert!(t.ext.home.is_none());
            }
            com_release(home);
            com_release(bmp);
            com_release(rt.cast());
            Com::release(f);
        }
    }

    /// #1669 : une cible WIC rend un bitmap par GetTarget, et SetTarget de ce
    /// bitmap (ou NULL) la rétablit : Paint.NET y dessine l'empreinte du
    /// pinceau après être passé par une liste de commandes.
    #[test]
    fn cible_wic_get_target_puis_retour() {
        unsafe {
            let (f, wic, rt) = super::super::tests::setup();
            let rt: *mut TargetObj = rt.cast();
            super::super::begin_draw(rt);
            let mut home = std::ptr::null_mut();
            dc_get_target(rt, &mut home);
            assert!(!home.is_null());
            let mut bmp = std::ptr::null_mut();
            let size = 2u64 | (2u64 << 32);
            assert_eq!(
                dc_create_bitmap(rt, size, std::ptr::null(), 0, std::ptr::null(), &mut bmp),
                S_OK
            );
            dc_set_target(rt, bmp);
            let rouge = super::super::ColorF {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            super::super::clear(rt, &rouge);
            dc_set_target(rt, home);
            {
                let t = target(rt);
                assert_eq!((t.w, t.h), (8, 8));
                assert!(matches!(t.out, Output::Wic(_)));
            }
            let vert = super::super::ColorF {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            };
            super::super::clear(rt, &vert);
            // Même retour avec SetTarget(NULL).
            dc_set_target(rt, bmp);
            dc_set_target(rt, std::ptr::null_mut());
            assert!(matches!(target(rt).out, Output::Wic(_)));
            assert_eq!(
                super::super::end_draw(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            type Lock =
                unsafe extern "win64" fn(*mut c_void, *const c_void, u32, *mut *mut c_void) -> i32;
            type Data = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
            let mut lock = std::ptr::null_mut();
            assert_eq!(
                method::<Lock>(wic, 8)(wic, std::ptr::null(), 1, &mut lock),
                S_OK
            );
            let (mut n, mut data) = (0u32, std::ptr::null_mut());
            assert_eq!(method::<Data>(lock, 5)(lock, &mut n, &mut data), S_OK);
            assert_eq!(std::slice::from_raw_parts(data, 4), &[0, 255, 0, 255]);
            com_release(lock);
            {
                let b = &Com::get(bmp as *mut BitmapObj).data;
                assert_eq!(&crate::dlls::lock(&b.px)[0..4], &[0, 0, 255, 255]);
            }
            com_release(home);
            com_release(bmp);
            com_release(rt.cast());
            com_release(wic);
            com_release(f);
        }
    }

    /// #1609 : le journal WINLUX_D2D_TRACE décrit bitmaps et cibles.
    #[test]
    fn journal_d2d_decrit_bitmap_et_cible() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut rt = std::ptr::null_mut();
            assert_eq!(
                super::super::create_dc_render_target(f, std::ptr::null(), &mut rt),
                S_OK
            );
            let rt: *mut TargetObj = rt.cast();
            let mut bmp = std::ptr::null_mut();
            let size = 2u64 | (1u64 << 32);
            let src = [0u8, 0, 0, 255, 0, 0, 0, 0];
            assert_eq!(
                dc_create_bitmap(rt, size, src.as_ptr(), 8, std::ptr::null(), &mut bmp),
                S_OK
            );
            let d = describe_image(bmp);
            assert!(
                d.starts_with("bitmap ")
                    && d.ends_with("2x1 options 0x0 visibles 1 moy [0, 0, 0, 255]"),
                "{d}"
            );
            assert_eq!(describe_image(std::ptr::null_mut()), "NULL");
            let t = describe_target(&target(rt));
            assert!(t.starts_with("hdc 0x0 @0,0 0x0 visibles 0"), "{t}");
            com_release(bmp);
            com_release(rt.cast());
            Com::release(f);
        }
    }

    /// #1609 : Flood, Crop et Premultiply sont calculés (tuiles unies du
    /// canevas de Paint.NET : un bitmap 1x1 rempli par un Flood blanc).
    #[test]
    fn effets_flood_crop_premultiply() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(factory_create_device(f, fake.cast(), &mut dev), S_OK);
            Com::release(fake);
            let mut dc = std::ptr::null_mut();
            assert_eq!(device_create_context(dev.cast(), 0, &mut dc), S_OK);
            let dc: *mut TargetObj = dc.cast();
            let mut bmp = std::ptr::null_mut();
            assert_eq!(
                dc_create_bitmap(
                    dc,
                    1 | (1 << 32),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut bmp
                ),
                S_OK
            );
            dc_set_target(dc, bmp);
            let mut flood = std::ptr::null_mut();
            assert_eq!(dc_create_effect(dc, &CLSID_FLOOD, &mut flood), S_OK);
            let color: [f32; 4] = [1.0, 1.0, 1.0, 0.5];
            let bytes: Vec<u8> = color.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                effect_set_value(flood.cast(), 0, 0, bytes.as_ptr(), 16),
                S_OK
            );
            super::super::begin_draw(dc);
            dc_draw_image(dc, flood, std::ptr::null(), std::ptr::null(), 0, 0);
            assert_eq!(
                super::super::end_draw(dc, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let b = &Com::get(bmp as *mut BitmapObj).data;
            assert_eq!(&crate::dlls::lock(&b.px)[0..4], &[128, 128, 128, 128]);

            // DrawImage avec rectangle source : son coin haut gauche va au
            // décalage, sans s'y ajouter (#1651).
            let mut large = std::ptr::null_mut();
            assert_eq!(
                dc_create_bitmap(
                    dc,
                    4 | (1 << 32),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut large
                ),
                S_OK
            );
            dc_set_target(dc, large);
            let rouge: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
            let bytes: Vec<u8> = rouge.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                effect_set_value(flood.cast(), 0, 0, bytes.as_ptr(), 16),
                S_OK
            );
            super::super::begin_draw(dc);
            let decalage = [1.0f32, 0.0];
            let source = RectF {
                l: 1.0,
                t: 0.0,
                r: 2.0,
                b: 1.0,
            };
            dc_draw_image(dc, flood, &decalage, &source, 0, 0);
            assert_eq!(
                super::super::end_draw(dc, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            {
                let b = &Com::get(large as *mut BitmapObj).data;
                let px = crate::dlls::lock(&b.px);
                assert_eq!(&px[0..4], &[0, 0, 0, 0]);
                assert_eq!(&px[4..8], &[0, 0, 255, 255]);
                assert_eq!(&px[8..12], &[0, 0, 0, 0]);
            }
            dc_set_target(dc, bmp);
            com_release(large);

            // Crop d'un Flood opaque à (1, 2, 3, 3) : image 2x1 en (1, 2).
            let mut crop = std::ptr::null_mut();
            assert_eq!(dc_create_effect(dc, &CLSID_CROP, &mut crop), S_OK);
            let rect: [f32; 4] = [1.0, 2.0, 3.0, 3.0];
            let bytes: Vec<u8> = rect.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                effect_set_value(crop.cast(), 0, 0, bytes.as_ptr(), 16),
                S_OK
            );
            let opaque: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
            let bytes: Vec<u8> = opaque.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                effect_set_value(flood.cast(), 0, 0, bytes.as_ptr(), 16),
                S_OK
            );
            effect_set_input(crop.cast(), 0, flood, 1);
            let img = image_img(crop, 0, (8, 8)).unwrap();
            assert_eq!((img.x, img.y, img.w, img.h), (1, 2, 2, 1));
            assert_eq!(&img.px[0..4], &[255, 0, 0, 255]);

            // Premultiply : (200, 100, 50, a 128) → (100, 50, 25).
            let mut pm = std::ptr::null_mut();
            assert_eq!(dc_create_effect(dc, &CLSID_PREMULTIPLY, &mut pm), S_OK);
            let mut src = std::ptr::null_mut();
            let raw = [200u8, 100, 50, 128];
            assert_eq!(
                dc_create_bitmap(
                    dc,
                    1 | (1 << 32),
                    raw.as_ptr(),
                    4,
                    std::ptr::null(),
                    &mut src
                ),
                S_OK
            );
            effect_set_input(pm.cast(), 0, src, 1);
            let img = image_img(pm, 0, (1, 1)).unwrap();
            assert_eq!(&img.px[0..4], &[100, 50, 25, 128]);

            for o in [src, pm, crop, flood, bmp] {
                com_release(o);
            }
            com_release(dc.cast());
            com_release(dev);
            Com::release(f);
        }
    }

    #[test]
    fn device_context_draws_into_its_target_bitmap() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            // Un IDXGIDevice factice : seul AddRef/Release est appelé.
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(factory_create_device(f, fake.cast(), &mut dev), S_OK);
            Com::release(fake);
            let mut dc = std::ptr::null_mut();
            assert_eq!(device_create_context(dev.cast(), 0, &mut dc), S_OK);
            let dc: *mut TargetObj = dc.cast();
            assert!(is_device_context(dc));
            let mut bmp = std::ptr::null_mut();
            let size = 4u64 | (3u64 << 32);
            assert_eq!(
                dc_create_bitmap(dc, size, std::ptr::null(), 0, std::ptr::null(), &mut bmp),
                S_OK
            );
            dc_set_target(dc, bmp);
            super::super::begin_draw(dc);
            let red = super::super::ColorF {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            super::super::clear(dc, &red);
            assert_eq!(
                super::super::end_draw(dc, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let b = &Com::get(bmp as *mut BitmapObj).data;
            let px = crate::dlls::lock(&b.px);
            assert_eq!(&px[0..4], &[0, 0, 255, 255]);
            drop(px);
            let mut back = std::ptr::null_mut();
            dc_get_target(dc, &mut back);
            assert_eq!(back, bmp);
            com_release(back);
            com_release(bmp);
            com_release(dc.cast());
            com_release(dev);
            Com::release(f);
        }
    }

    /// Paint.NET (ColorsForm.SetColorAddIcon) : CreateCommandList, dessin
    /// dans la liste prise pour cible, Close, puis la liste comme image
    /// (#1331).
    /// #1354 : un contexte sRGB créé sans profil rend un profil ICC complet
    /// (Paint.NET refuse moins de 128 octets).
    #[test]
    fn contexte_srgb_rend_un_profil_icc() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(factory_create_device(f, fake.cast(), &mut dev), S_OK);
            Com::release(fake);
            let mut dc = std::ptr::null_mut();
            assert_eq!(device_create_context(dev.cast(), 0, &mut dc), S_OK);
            let dc: *mut TargetObj = dc.cast();
            for (space, expected) in [
                (1, crate::dlls::icc::srgb()),
                (2, crate::dlls::icc::scrgb()),
            ] {
                let mut cc = std::ptr::null_mut();
                assert_eq!(
                    dc_color_context(dc, space, std::ptr::null(), 0, &mut cc),
                    S_OK
                );
                let cc: *mut ColorObj = cc.cast();
                let size = color_profile_size(cc);
                assert!(size > 128);
                let mut buf = vec![0u8; size as usize];
                assert_eq!(color_profile(cc, buf.as_mut_ptr(), size), S_OK);
                assert_eq!(buf, expected);
                assert_eq!(color_space(cc), space);
                Com::release(cc);
            }
            // Un profil fourni est gardé tel quel.
            let own = [7u8; 200];
            let mut cc = std::ptr::null_mut();
            assert_eq!(dc_color_context(dc, 0, own.as_ptr(), 200, &mut cc), S_OK);
            assert_eq!(color_profile_size(cc.cast()), 200);
            Com::release(cc.cast::<ColorObj>());
        }
    }

    /// #1367 : l'effet intégré de gestion des couleurs décrit ses propriétés
    /// (Paint.NET lit ses contextes de couleur par GetValueRef).
    #[test]
    fn gestion_des_couleurs_decrit_ses_contextes() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(factory_create_device(f, fake.cast(), &mut dev), S_OK);
            Com::release(fake);
            let mut dc = std::ptr::null_mut();
            assert_eq!(device_create_context(dev.cast(), 0, &mut dc), S_OK);
            let dc: *mut TargetObj = dc.cast();
            let id = guid(
                0x1a28_524c,
                0xfdd6,
                0x4aa4,
                [0xae, 0x8f, 0x83, 0x7e, 0xb8, 0x26, 0x7b, 0x37],
            );
            let mut e = std::ptr::null_mut();
            assert_eq!(dc_create_effect(dc, &id, &mut e), S_OK);
            let e: *mut EffectObj = e.cast();
            assert_eq!(effect_prop_count(e), 6);
            assert_eq!(effect_prop_type(e, 0), custom::ty::COLOR_CONTEXT);
            assert_eq!(effect_prop_type(e, 1), custom::ty::ENUM);
            assert_eq!(effect_prop_type(e, 2), custom::ty::COLOR_CONTEXT);
            let nom: Vec<u16> = "DestinationColorContext"
                .encode_utf16()
                .chain([0])
                .collect();
            assert_eq!(effect_prop_index(e, nom.as_ptr()), 2);
            assert_eq!(effect_value_size(e, 0), 8);
            assert_eq!(effect_value_size(e, 4), 4);
            let mut p: usize = 0xdead;
            let pp = &mut p as *mut usize as *mut u8;
            assert_eq!(
                effect_get_value(e, 0, custom::ty::COLOR_CONTEXT, pp, 8),
                S_OK
            );
            assert_eq!(p, 0);
            let mut alpha = 0u32;
            let pa = &mut alpha as *mut u32 as *mut u8;
            assert_eq!(effect_get_value(e, 4, custom::ty::ENUM, pa, 4), S_OK);
            assert_eq!(alpha, 1);
            // Contexte posé : l'effet garde une référence, la lecture en rend une.
            let mut cc = std::ptr::null_mut();
            assert_eq!(dc_color_context(dc, 1, std::ptr::null(), 0, &mut cc), S_OK);
            let v = (cc as usize).to_le_bytes();
            assert_eq!(
                effect_set_value(e, 0, custom::ty::COLOR_CONTEXT, v.as_ptr(), 8),
                S_OK
            );
            assert_eq!(
                effect_get_value(e, 0, custom::ty::COLOR_CONTEXT, pp, 8),
                S_OK
            );
            assert_eq!(p, cc as usize);
            com_release(cc); // celle de la lecture
            assert_eq!(com_add_ref(cc), 3); // création + effet + celle-ci
            com_release(cc);
            Com::release(e);
            assert_eq!(com_release(cc), 0);
            Com::release(dc);
            Com::release(f);
        }
    }

    #[test]
    fn liste_de_commandes_dessinee_puis_lue_comme_image() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let fake = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            let mut dev = std::ptr::null_mut();
            assert_eq!(factory_create_device(f, fake.cast(), &mut dev), S_OK);
            Com::release(fake);
            let mut dc = std::ptr::null_mut();
            assert_eq!(device_create_context(dev.cast(), 0, &mut dc), S_OK);
            let dc: *mut TargetObj = dc.cast();
            // Cible 8×6, puis la liste prend sa taille.
            let mut bmp = std::ptr::null_mut();
            let size = 8u64 | (6u64 << 32);
            assert_eq!(
                dc_create_bitmap(dc, size, std::ptr::null(), 0, std::ptr::null(), &mut bmp),
                S_OK
            );
            dc_set_target(dc, bmp);
            let create = method::<unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32>(
                dc.cast(),
                67,
            );
            let mut list = std::ptr::null_mut();
            assert_eq!(create(dc.cast(), &mut list), S_OK);
            assert!(!list.is_null());
            // QueryInterface : ID2D1CommandList et ID2D1Image.
            let qi = method::<
                unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
            >(list, 0);
            for iid in [IID_D2D1_COMMAND_LIST, IID_D2D1_IMAGE] {
                let mut p = std::ptr::null_mut();
                assert_eq!(qi(list, &iid, &mut p), S_OK);
                assert_eq!(p, list);
                com_release(p);
            }
            // Dessin dans la liste.
            dc_set_target(dc, list);
            let mut back = std::ptr::null_mut();
            dc_get_target(dc, &mut back);
            assert_eq!(back, list);
            com_release(back);
            super::super::begin_draw(dc);
            let green = super::super::ColorF {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            };
            super::super::clear(dc, &green);
            assert_eq!(
                super::super::end_draw(dc, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let close = method::<unsafe extern "win64" fn(*mut c_void) -> i32>(list, 5);
            assert_eq!(close(list), S_OK);
            // La liste se lit comme une image de la taille de la cible.
            let img = image_img(list, 0, (8, 6)).expect("image de la liste");
            assert_eq!((img.w, img.h), (8, 6));
            assert_eq!(&img.px[0..4], &[0, 255, 0, 255]);
            dc_set_target(dc, std::ptr::null_mut());
            com_release(list);
            com_release(bmp);
            com_release(dc.cast());
            com_release(dev);
            Com::release(f);
        }
    }

    #[test]
    fn effect_keeps_inputs_and_values() {
        unsafe {
            let f = Com::create(super::super::FACTORY_VT.ptr(), super::super::Factory);
            Com::add_ref(f);
            let e = Com::create(
                EFFECT_VT.ptr(),
                Effect {
                    clsid: FACTORY_IIDS[0],
                    inputs: Mutex::new(vec![std::ptr::null_mut()]),
                    props: Mutex::new(Default::default()),
                    factory: f,
                    custom: OnceLock::new(),
                    desc: None,
                },
            );
            let v = 7u32.to_le_bytes();
            assert_eq!(effect_set_value(e, 0, 5, v.as_ptr(), 4), S_OK);
            let mut got = [0u8; 4];
            assert_eq!(effect_get_value(e, 0, 5, got.as_mut_ptr(), 4), S_OK);
            assert_eq!(got, v);
            assert_eq!(effect_input_count(e), 1);
            assert_eq!(effect_set_input_count(e, 3), S_OK);
            assert_eq!(effect_input_count(e), 3);
            let mut clsid = [0u8; 16];
            effect_get_value(e, PROP_CLSID, 0, clsid.as_mut_ptr(), 16);
            assert_eq!(&clsid[0..4], &0xbb12_d362u32.to_le_bytes());
            Com::release(e);
            Com::release(f);
        }
    }
}
