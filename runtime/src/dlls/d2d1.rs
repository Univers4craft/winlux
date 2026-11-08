//! Direct2D 1.0 (`d2d1.dll`) écrit pour Winlux, en rendu logiciel.
//!
//! La fabrique crée des géométries (rectangle, rectangle arrondi, ellipse,
//! chemin avec son `ID2D1GeometrySink`) et des cibles de rendu : bitmap WIC,
//! fenêtre (`ID2D1HwndRenderTarget`, posée sur la zone client à EndDraw), DC
//! (`ID2D1DCRenderTarget` et BindDC) et hors écran
//! (`CreateCompatibleRenderTarget`, `ID2D1BitmapRenderTarget`). Une cible
//! dessine avec `d2d_raster` : effacement, rectangles, ellipses, géométries,
//! lignes, bitmaps, texte DirectWrite, transformations, découpes alignées sur
//! les axes, pinceaux unis avec opacité. Les dégradés et les calques
//! viendront ensuite ; ces méthodes répondent E_NOTIMPL ou ne dessinent rien.

use super::audio_com::{
    guid, put, Com, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::d2d_raster::{premul, Canvas, Clip, Color, FillRule, Matrix, Path, Pt};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::Mutex;

pub(crate) fn unsupported_call(name: &'static str) -> i32 {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut seen = crate::dlls::lock(&SEEN);
    if !seen.contains(&name) {
        seen.push(name);
        crate::log_warn!("com", "méthode {} non prise en charge (E_NOTIMPL)", name);
    }
    E_NOTIMPL
}

#[path = "d2d1_1.rs"]
mod v11;

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const D2DERR_WRONG_STATE: i32 = 0x8899_0001u32 as i32;
const D2DERR_UNSUPPORTED_PIXEL_FORMAT: i32 = 0x8898_2F80u32 as i32;

const IID_D2D1_FACTORY: Guid = guid(
    0x06152247,
    0x6f50,
    0x465a,
    [0x92, 0x45, 0x11, 0x8b, 0xfd, 0x3b, 0x60, 0x07],
);
const IID_D2D1_RESOURCE: Guid = guid(
    0x2cd90691,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_RENDER_TARGET: Guid = guid(
    0x2cd90694,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_HWND_RENDER_TARGET: Guid = guid(
    0x2cd90698,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_DC_RENDER_TARGET: Guid = guid(
    0x1c51bc64,
    0xde61,
    0x46fd,
    [0x98, 0x99, 0x63, 0xa5, 0xd8, 0xf0, 0x39, 0x50],
);
const IID_D2D1_BITMAP_RENDER_TARGET: Guid = guid(
    0x2cd90695,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_BRUSH: Guid = guid(
    0x2cd906a8,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_SOLID_BRUSH: Guid = guid(
    0x2cd906a9,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_BITMAP: Guid = guid(
    0xa2296057,
    0xea42,
    0x4099,
    [0x98, 0x3b, 0x53, 0x9f, 0xb6, 0x50, 0x54, 0x26],
);
const IID_D2D1_IMAGE: Guid = guid(
    0x65019f75,
    0x8da2,
    0x497c,
    [0xb3, 0x2c, 0xdf, 0xa3, 0x4e, 0x48, 0xed, 0xe6],
);
const IID_D2D1_GEOMETRY: Guid = guid(
    0x2cd906a1,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_RECT_GEOMETRY: Guid = guid(
    0x2cd906a2,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_ROUNDED_RECT_GEOMETRY: Guid = guid(
    0x2cd906a3,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_ELLIPSE_GEOMETRY: Guid = guid(
    0x2cd906a4,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_PATH_GEOMETRY: Guid = guid(
    0x2cd906a5,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_SIMPLIFIED_SINK: Guid = guid(
    0x2cd9069e,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_GEOMETRY_SINK: Guid = guid(
    0x2cd9069f,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);

/// Un point D2D1_POINT_2F passé par valeur arrive dans un registre 64 bits.
pub(super) fn point(v: u64) -> Pt {
    (f32::from_bits(v as u32), f32::from_bits((v >> 32) as u32))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RectF {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct ColorF {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[repr(C)]
struct RoundedRect {
    rect: RectF,
    rx: f32,
    ry: f32,
}

#[repr(C)]
struct Ellipse {
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
}

/// Appelle la méthode n° `slot` de l'objet COM `obj`.
pub(super) unsafe fn method<F: Copy>(obj: *mut c_void, slot: usize) -> F {
    let vtbl = *(obj as *const *const usize);
    std::mem::transmute_copy(&*vtbl.add(slot))
}

pub(super) unsafe extern "win64" fn add_ref<T>(this: *mut Com<T>) -> u32 {
    Com::add_ref(this)
}
pub(super) unsafe extern "win64" fn release<T>(this: *mut Com<T>) -> u32 {
    Com::release(this)
}
pub(super) unsafe extern "win64" fn not_implemented() -> i32 {
    E_NOTIMPL
}
pub(super) unsafe extern "win64" fn nothing() {}

pub(super) unsafe fn qi_list<T>(
    this: *mut Com<T>,
    riid: *const Guid,
    out: *mut *mut c_void,
    iids: &[Guid],
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if riid.is_null() {
        return E_INVALIDARG;
    }
    if *riid == IID_IUNKNOWN || iids.contains(&*riid) {
        Com::add_ref(this);
        return put(out, this);
    }
    super::comobj::log_no_interface("d2d1/dwrite", riid);
    E_NOINTERFACE
}

/// Toute ressource Direct2D connaît sa fabrique (ID2D1Resource::GetFactory).
struct Res<T> {
    factory: *mut FactoryObj,
    data: T,
}

impl<T> Drop for Res<T> {
    fn drop(&mut self) {
        unsafe { Com::release(self.factory) };
    }
}

unsafe fn new_res<T>(factory: *mut FactoryObj, vt: *const (), data: T) -> *mut Com<Res<T>> {
    Com::add_ref(factory);
    Com::create(vt, Res { factory, data })
}

unsafe extern "win64" fn get_factory<T>(this: *mut Com<Res<T>>, out: *mut *mut c_void) {
    if !out.is_null() {
        let f = Com::get(this).factory;
        Com::add_ref(f);
        *out = f as *mut c_void;
    }
}

// --- Fabrique -------------------------------------------------------------------

struct Factory;
type FactoryObj = Com<Factory>;

unsafe extern "win64" fn factory_qi(
    this: *mut FactoryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() && !riid.is_null() && v11::is_multithread_iid(&*riid) {
        return v11::multithread(out);
    }
    if !riid.is_null() && v11::is_factory_n_iid(&*riid) {
        return qi_list(this, riid, out, &[*riid]);
    }
    qi_list(this, riid, out, &[IID_D2D1_FACTORY])
}

unsafe extern "win64" fn factory_desktop_dpi(_this: *mut FactoryObj, x: *mut f32, y: *mut f32) {
    if !x.is_null() {
        *x = 96.0;
    }
    if !y.is_null() {
        *y = 96.0;
    }
}

unsafe fn new_geometry(
    f: *mut FactoryObj,
    kind: GeomKind,
    path: Path,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let g = Geometry {
        kind,
        path: Mutex::new(path),
        rule: Mutex::new(FillRule::EvenOdd),
        open: Mutex::new(false),
    };
    put(out, new_res(f, GEOMETRY_VT.ptr(), g))
}

unsafe extern "win64" fn create_rect_geometry(
    this: *mut FactoryObj,
    r: *const RectF,
    out: *mut *mut c_void,
) -> i32 {
    if r.is_null() {
        return E_INVALIDARG;
    }
    let r = *r;
    new_geometry(this, GeomKind::Rect, Path::rect(r.l, r.t, r.r, r.b), out)
}

unsafe extern "win64" fn create_rounded_rect_geometry(
    this: *mut FactoryObj,
    r: *const RoundedRect,
    out: *mut *mut c_void,
) -> i32 {
    if r.is_null() {
        return E_INVALIDARG;
    }
    let r = &*r;
    let p = Path::rounded_rect(r.rect.l, r.rect.t, r.rect.r, r.rect.b, r.rx, r.ry, 4.0);
    new_geometry(this, GeomKind::RoundedRect, p, out)
}

unsafe extern "win64" fn create_ellipse_geometry(
    this: *mut FactoryObj,
    e: *const Ellipse,
    out: *mut *mut c_void,
) -> i32 {
    if e.is_null() {
        return E_INVALIDARG;
    }
    let e = &*e;
    new_geometry(
        this,
        GeomKind::Ellipse,
        Path::ellipse(e.x, e.y, e.rx, e.ry, 4.0),
        out,
    )
}

unsafe extern "win64" fn create_path_geometry(this: *mut FactoryObj, out: *mut *mut c_void) -> i32 {
    new_geometry(this, GeomKind::Path, Path::default(), out)
}

/// CreateWicBitmapRenderTarget : la cible dessine dans le bitmap WIC
/// (32 bits BGRA prémultiplié ou BGR), lu à BeginDraw et écrit à EndDraw.
unsafe extern "win64" fn create_wic_render_target(
    this: *mut FactoryObj,
    bitmap: *mut c_void,
    _props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if bitmap.is_null() {
        return E_INVALIDARG;
    }
    type GetSize = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut u32) -> i32;
    type GetFormat = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
    let (mut w, mut h) = (0, 0);
    let hr = method::<GetSize>(bitmap, 3)(bitmap, &mut w, &mut h);
    if hr < 0 {
        return hr;
    }
    let mut fmt = Guid::default();
    method::<GetFormat>(bitmap, 4)(bitmap, &mut fmt);
    use super::windowscodecs::{FMT_BGR32, FMT_PBGRA32};
    if fmt != FMT_PBGRA32 && fmt != FMT_BGR32 {
        return D2DERR_UNSUPPORTED_PIXEL_FORMAT;
    }
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(bitmap, 1)(bitmap);
    let target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: fmt == FMT_BGR32,
        out: Output::Wic(bitmap),
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    put(out, new_res(this, TARGET_VT.ptr(), Mutex::new(target)))
}

/// D2D1_HWND_RENDER_TARGET_PROPERTIES.
#[repr(C)]
struct HwndProps {
    hwnd: usize,
    w: u32,
    h: u32,
    present: u32,
}

unsafe fn new_screen_target(
    f: *mut FactoryObj,
    w: u32,
    h: u32,
    out_to: Output,
    vt: *const (),
    out: *mut *mut c_void,
) -> i32 {
    let target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: true,
        out: out_to,
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    put(out, new_res(f, vt, Mutex::new(target)))
}

/// CreateHwndRenderTarget : rendu logiciel posé sur la fenêtre à EndDraw
/// (Scintilla de Notepad++ en DirectWrite, #551).
unsafe extern "win64" fn create_hwnd_render_target(
    this: *mut FactoryObj,
    _props: *const c_void,
    hwnd_props: *const HwndProps,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(hp) = hwnd_props.as_ref() else {
        return E_INVALIDARG;
    };
    let (mut w, mut h) = (hp.w, hp.h);
    if w == 0 || h == 0 {
        if let Some(r) = super::gui::window_client_rect(hp.hwnd) {
            w = r.width().max(0) as u32;
            h = r.height().max(0) as u32;
        }
    }
    new_screen_target(this, w, h, Output::Hwnd(hp.hwnd), HWND_TARGET_VT.ptr(), out)
}

/// CreateDCRenderTarget : la taille et le DC viennent de BindDC.
unsafe extern "win64" fn create_dc_render_target(
    this: *mut FactoryObj,
    _props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    new_screen_target(
        this,
        0,
        0,
        Output::Dc { hdc: 0, x: 0, y: 0 },
        DC_TARGET_VT.ptr(),
        out,
    )
}

static FACTORY_VT: VTable<33> = VTable([
    factory_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    unsupported!("ReloadSystemMetrics"),
    factory_desktop_dpi as *const (),
    create_rect_geometry as *const (),
    create_rounded_rect_geometry as *const (),
    create_ellipse_geometry as *const (),
    unsupported!("CreateGeometryGroup"),
    unsupported!("CreateTransformedGeometry"),
    create_path_geometry as *const (),
    v11::factory_stroke_style as *const (),
    v11::factory_state_block as *const (),
    create_wic_render_target as *const (),
    create_hwnd_render_target as *const (),
    unsupported!("CreateDxgiSurfaceRenderTarget"),
    create_dc_render_target as *const (),
    // ID2D1Factory1 (17-26)
    v11::factory_create_device as *const (),
    v11::factory_stroke_style1 as *const (),
    create_path_geometry as *const (), // ID2D1PathGeometry1
    v11::factory_state_block1 as *const (),
    unsupported!("CreateGdiMetafile"),
    v11::factory_register_stream as *const (),
    v11::factory_register_string as *const (),
    v11::factory_unregister as *const (),
    v11::factory_registered as *const (),
    v11::factory_effect_properties as *const (),
    // ID2D1Factory2 à 7 : CreateDevice rend ID2D1Device1 à 6.
    v11::factory_create_device as *const (),
    v11::factory_create_device as *const (),
    v11::factory_create_device as *const (),
    v11::factory_create_device as *const (),
    v11::factory_create_device as *const (),
    v11::factory_create_device as *const (),
]);

// --- Géométries ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum GeomKind {
    Rect,
    RoundedRect,
    Ellipse,
    Path,
}

struct Geometry {
    kind: GeomKind,
    path: Mutex<Path>,
    rule: Mutex<FillRule>,
    /// Un puits (sink) est ouvert et la géométrie n'est pas encore figée.
    open: Mutex<bool>,
}

type GeometryObj = Com<Res<Geometry>>;

unsafe extern "win64" fn geometry_qi(
    this: *mut GeometryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let own = match Com::get(this).data.kind {
        GeomKind::Rect => IID_D2D1_RECT_GEOMETRY,
        GeomKind::RoundedRect => IID_D2D1_ROUNDED_RECT_GEOMETRY,
        GeomKind::Ellipse => IID_D2D1_ELLIPSE_GEOMETRY,
        GeomKind::Path => IID_D2D1_PATH_GEOMETRY,
    };
    // ID2D1PathGeometry1 (62baa2d2) prolonge ID2D1PathGeometry.
    const IID_PATH1: Guid = guid(
        0x62baa2d2,
        0xab54,
        0x41b7,
        [0xb8, 0x72, 0x78, 0x7e, 0x01, 0x06, 0xa4, 0x21],
    );
    let path1 = if own == IID_D2D1_PATH_GEOMETRY {
        IID_PATH1
    } else {
        own
    };
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_GEOMETRY, own, path1],
    )
}

unsafe extern "win64" fn geometry_bounds(
    this: *mut GeometryObj,
    m: *const Matrix,
    out: *mut RectF,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    let path = crate::dlls::lock(&Com::get(this).data.path);
    let mut r = RectF {
        l: f32::MAX,
        t: f32::MAX,
        r: f32::MIN,
        b: f32::MIN,
    };
    for p in path.fill_polys(&m).iter().flatten() {
        r.l = r.l.min(p.0);
        r.t = r.t.min(p.1);
        r.r = r.r.max(p.0);
        r.b = r.b.max(p.1);
    }
    *out = r;
    S_OK
}

/// FillContainsPoint(point, transform, tolérance, *BOOL) : test pair-impair
/// ou non nul sur les polygones aplatis.
unsafe extern "win64" fn geometry_fill_contains(
    this: *mut GeometryObj,
    pt: u64,
    m: *const Matrix,
    _tol: f32,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    let g = &Com::get(this).data;
    let polys = crate::dlls::lock(&g.path).fill_polys(&m);
    let rule = *crate::dlls::lock(&g.rule);
    let (x, y) = point(pt);
    let mut wind = 0i32;
    for poly in &polys {
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a.1 <= y) != (b.1 <= y) {
                let t = (y - a.1) / (b.1 - a.1);
                if a.0 + t * (b.0 - a.0) > x {
                    wind += if b.1 > a.1 { 1 } else { -1 };
                }
            }
        }
    }
    *out = i32::from(match rule {
        FillRule::EvenOdd => wind % 2 != 0,
        FillRule::NonZero => wind != 0,
    });
    S_OK
}

unsafe extern "win64" fn path_open(this: *mut GeometryObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let g = &Com::get(this).data;
    let mut open = crate::dlls::lock(&g.open);
    if *open {
        return D2DERR_WRONG_STATE;
    }
    *open = true;
    *crate::dlls::lock(&g.path) = Path::default();
    Com::add_ref(this);
    put(out, Com::create(SINK_VT.ptr(), Sink { geometry: this }))
}

unsafe extern "win64" fn path_figure_count(this: *mut GeometryObj, n: *mut u32) -> i32 {
    if n.is_null() {
        return E_INVALIDARG;
    }
    *n = crate::dlls::lock(&Com::get(this).data.path).figures.len() as u32;
    S_OK
}

/// Table commune : ID2D1Geometry (17 entrées) puis, pour un chemin,
/// Open, Stream, GetSegmentCount et GetFigureCount.
static GEOMETRY_VT: VTable<22> = VTable([
    geometry_qi as *const (),
    add_ref::<Res<Geometry>> as *const (),
    release::<Res<Geometry>> as *const (),
    get_factory::<Geometry> as *const (),
    geometry_bounds as *const (),
    unsupported!("GetWidenedBounds"),
    unsupported!("StrokeContainsPoint"),
    geometry_fill_contains as *const (),
    unsupported!("CompareWithGeometry"),
    unsupported!("Simplify"),
    unsupported!("Tessellate"),
    unsupported!("CombineWithGeometry"),
    unsupported!("Outline"),
    unsupported!("ComputeArea"),
    unsupported!("ComputeLength"),
    unsupported!("ComputePointAtLength"),
    unsupported!("Widen"),
    path_open as *const (), // ou GetRect / GetRoundedRect / GetEllipse
    unsupported!("Stream"),
    path_figure_count as *const (), // GetSegmentCount (approché)
    path_figure_count as *const (),
    unsupported!("ComputePointAndSegmentAtLength"), // ID2D1PathGeometry1
]);

// --- Puits de géométrie ------------------------------------------------------------

struct Sink {
    geometry: *mut GeometryObj,
}

impl Drop for Sink {
    fn drop(&mut self) {
        unsafe {
            *crate::dlls::lock(&Com::get(self.geometry).data.open) = false;
            Com::release(self.geometry);
        }
    }
}

type SinkObj = Com<Sink>;

unsafe fn sink_path<'a>(this: *mut SinkObj) -> std::sync::MutexGuard<'a, Path> {
    crate::dlls::lock(&Com::get(Com::get(this).geometry).data.path)
}

unsafe extern "win64" fn sink_qi(
    this: *mut SinkObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_SIMPLIFIED_SINK, IID_D2D1_GEOMETRY_SINK],
    )
}
unsafe extern "win64" fn sink_fill_mode(this: *mut SinkObj, mode: u32) {
    *crate::dlls::lock(&Com::get(Com::get(this).geometry).data.rule) = if mode == 1 {
        FillRule::NonZero
    } else {
        FillRule::EvenOdd
    };
}
unsafe extern "win64" fn sink_begin(this: *mut SinkObj, start: u64, _begin: u32) {
    sink_path(this).begin(point(start));
}
unsafe extern "win64" fn sink_lines(this: *mut SinkObj, pts: *const Pt, n: u32) {
    if pts.is_null() {
        return;
    }
    let mut p = sink_path(this);
    for i in 0..n as usize {
        p.line(*pts.add(i));
    }
}
unsafe extern "win64" fn sink_beziers(this: *mut SinkObj, b: *const [Pt; 3], n: u32) {
    if b.is_null() {
        return;
    }
    let mut p = sink_path(this);
    for i in 0..n as usize {
        let [c1, c2, e] = *b.add(i);
        p.cubic(c1, c2, e, 4.0);
    }
}
unsafe extern "win64" fn sink_end(this: *mut SinkObj, end: u32) {
    if end == 1 {
        sink_path(this).close();
    }
}
unsafe extern "win64" fn sink_close(_this: *mut SinkObj) -> i32 {
    S_OK
}
unsafe extern "win64" fn sink_line(this: *mut SinkObj, pt: u64) {
    sink_path(this).line(point(pt));
}
unsafe extern "win64" fn sink_bezier(this: *mut SinkObj, b: *const [Pt; 3]) {
    sink_beziers(this, b, 1);
}
unsafe extern "win64" fn sink_quadratics(this: *mut SinkObj, q: *const [Pt; 2], n: u32) {
    if q.is_null() {
        return;
    }
    let mut p = sink_path(this);
    for i in 0..n as usize {
        let [c, e] = *q.add(i);
        p.quadratic(c, e, 4.0);
    }
}
unsafe extern "win64" fn sink_quadratic(this: *mut SinkObj, q: *const [Pt; 2]) {
    sink_quadratics(this, q, 1);
}

#[repr(C)]
struct ArcSegment {
    point: Pt,
    rx: f32,
    ry: f32,
    rotation: f32,
    sweep: u32,
    size: u32,
}

unsafe extern "win64" fn sink_arc(this: *mut SinkObj, a: *const ArcSegment) {
    if a.is_null() {
        return;
    }
    let a = &*a;
    // D2D1_SWEEP_DIRECTION_CLOCKWISE = 1, D2D1_ARC_SIZE_LARGE = 1.
    sink_path(this).arc(
        a.point,
        a.rx,
        a.ry,
        a.rotation,
        a.sweep == 1,
        a.size == 1,
        4.0,
    );
}

static SINK_VT: VTable<15> = VTable([
    sink_qi as *const (),
    add_ref::<Sink> as *const (),
    release::<Sink> as *const (),
    sink_fill_mode as *const (),
    nothing as *const (), // SetSegmentFlags
    sink_begin as *const (),
    sink_lines as *const (),
    sink_beziers as *const (),
    sink_end as *const (),
    sink_close as *const (),
    sink_line as *const (),
    sink_bezier as *const (),
    sink_quadratic as *const (),
    sink_quadratics as *const (),
    sink_arc as *const (),
]);

// --- Pinceau uni --------------------------------------------------------------------

struct Brush {
    color: Mutex<ColorF>,
    opacity: Mutex<f32>,
}
type BrushObj = Com<Res<Brush>>;

impl Brush {
    fn premul(&self) -> Color {
        let c = *crate::dlls::lock(&self.color);
        premul(c.r, c.g, c.b, c.a * *crate::dlls::lock(&self.opacity))
    }
}

unsafe extern "win64" fn brush_qi(
    this: *mut BrushObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_BRUSH, IID_D2D1_SOLID_BRUSH],
    )
}
unsafe extern "win64" fn brush_set_opacity(this: *mut BrushObj, o: f32) {
    *crate::dlls::lock(&Com::get(this).data.opacity) = o.clamp(0.0, 1.0);
}
unsafe extern "win64" fn brush_get_opacity(this: *mut BrushObj) -> f32 {
    *crate::dlls::lock(&Com::get(this).data.opacity)
}
unsafe extern "win64" fn brush_set_color(this: *mut BrushObj, c: *const ColorF) {
    if !c.is_null() {
        *crate::dlls::lock(&Com::get(this).data.color) = *c;
    }
}
/// GetColor renvoie une structure : MSVC la place à l'adresse cachée `out`.
unsafe extern "win64" fn brush_get_color(this: *mut BrushObj, out: *mut ColorF) -> *mut ColorF {
    if !out.is_null() {
        *out = *crate::dlls::lock(&Com::get(this).data.color);
    }
    out
}
unsafe extern "win64" fn get_identity_transform(_this: *mut c_void, m: *mut Matrix) {
    if !m.is_null() {
        *m = Matrix::IDENTITY;
    }
}

static BRUSH_VT: VTable<10> = VTable([
    brush_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    nothing as *const (), // SetTransform
    brush_get_opacity as *const (),
    get_identity_transform as *const (),
    brush_set_color as *const (),
    brush_get_color as *const (),
]);

// --- Bitmap --------------------------------------------------------------------------

struct Bitmap {
    w: u32,
    h: u32,
    /// BGRA prémultiplié, sans marge.
    px: Mutex<Vec<u8>>,
    /// D2D1_BITMAP_OPTIONS (ID2D1Bitmap1::GetOptions).
    options: u32,
    /// Texture D3D11 sous-jacente (CreateBitmapFromDxgiSurface), tenue.
    surface: *mut c_void,
}

impl Bitmap {
    fn new(w: u32, h: u32, px: Vec<u8>) -> Bitmap {
        Bitmap {
            w,
            h,
            px: Mutex::new(px),
            options: 0,
            surface: std::ptr::null_mut(),
        }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.surface.is_null() {
            unsafe {
                method::<unsafe extern "win64" fn(*mut c_void) -> u32>(self.surface, 2)(
                    self.surface,
                )
            };
        }
    }
}
type BitmapObj = Com<Res<Bitmap>>;

unsafe extern "win64" fn bitmap_qi(
    this: *mut BitmapObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_IMAGE,
            IID_D2D1_BITMAP,
            v11::IID_D2D1_BITMAP1,
        ],
    )
}
unsafe extern "win64" fn bitmap_size(this: *mut BitmapObj, out: *mut [f32; 2]) -> *mut [f32; 2] {
    if !out.is_null() {
        let b = &Com::get(this).data;
        *out = [b.w as f32, b.h as f32];
    }
    out
}
unsafe extern "win64" fn bitmap_pixel_size(
    this: *mut BitmapObj,
    out: *mut [u32; 2],
) -> *mut [u32; 2] {
    if !out.is_null() {
        let b = &Com::get(this).data;
        *out = [b.w, b.h];
    }
    out
}
/// D2D1_PIXEL_FORMAT : DXGI_FORMAT_B8G8R8A8_UNORM (87), alpha prémultiplié (1).
unsafe extern "win64" fn pixel_format(_this: *mut c_void, out: *mut [u32; 2]) -> *mut [u32; 2] {
    if !out.is_null() {
        *out = [87, 1];
    }
    out
}
unsafe extern "win64" fn dpi_96(_this: *mut c_void, x: *mut f32, y: *mut f32) {
    if !x.is_null() {
        *x = 96.0;
    }
    if !y.is_null() {
        *y = 96.0;
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RectU {
    l: u32,
    t: u32,
    r: u32,
    b: u32,
}

unsafe extern "win64" fn bitmap_copy_from_memory(
    this: *mut BitmapObj,
    dst: *const RectU,
    src: *const u8,
    pitch: u32,
) -> i32 {
    if src.is_null() {
        return E_INVALIDARG;
    }
    let b = &Com::get(this).data;
    let r = if dst.is_null() {
        RectU {
            l: 0,
            t: 0,
            r: b.w,
            b: b.h,
        }
    } else {
        *dst
    };
    if r.l > r.r || r.t > r.b || r.r > b.w || r.b > b.h {
        return E_INVALIDARG;
    }
    let mut px = crate::dlls::lock(&b.px);
    let row = (r.r - r.l) as usize * 4;
    for y in 0..(r.b - r.t) as usize {
        let d = ((r.t as usize + y) * b.w as usize + r.l as usize) * 4;
        std::ptr::copy_nonoverlapping(src.add(y * pitch as usize), px[d..].as_mut_ptr(), row);
    }
    S_OK
}

static BITMAP_VT: VTable<16> = VTable([
    bitmap_qi as *const (),
    add_ref::<Res<Bitmap>> as *const (),
    release::<Res<Bitmap>> as *const (),
    get_factory::<Bitmap> as *const (),
    bitmap_size as *const (),
    bitmap_pixel_size as *const (),
    pixel_format as *const (),
    dpi_96 as *const (),
    unsupported!("CopyFromBitmap"),
    unsupported!("CopyFromRenderTarget"),
    bitmap_copy_from_memory as *const (),
    // ID2D1Bitmap1
    v11::bitmap_color_context as *const (),
    v11::bitmap_options as *const (),
    v11::bitmap_surface as *const (),
    v11::bitmap_map as *const (),
    v11::bitmap_unmap as *const (),
]);

// --- Cible de rendu -------------------------------------------------------------------

#[derive(Default)]
struct TargetState {
    transform: Option<Matrix>,
    clips: Vec<Clip>,
    tags: (u64, u64),
}

struct Target {
    w: u32,
    h: u32,
    px: Vec<u8>,
    opaque: bool,
    /// Destination des pixels à EndDraw.
    out: Output,
    state: TargetState,
    drawing: bool,
    /// Réglages du contexte d'appareil Direct2D 1.1.
    ext: v11::TargetExt,
}

/// Où une cible rend ses pixels.
enum Output {
    /// Bitmap WIC (référence tenue), relu à BeginDraw.
    Wic(*mut c_void),
    /// ID2D1HwndRenderTarget : la zone client de la fenêtre.
    Hwnd(usize),
    /// ID2D1DCRenderTarget : le DC et le rectangle donnés par BindDC.
    Dc { hdc: usize, x: i32, y: i32 },
    /// ID2D1BitmapRenderTarget : son ID2D1Bitmap (référence tenue), ou
    /// bitmap choisi par ID2D1DeviceContext::SetTarget.
    Bitmap(*mut c_void),
    /// Contexte d'appareil sans cible.
    None,
}

impl Drop for Target {
    fn drop(&mut self) {
        if let Output::Wic(obj) | Output::Bitmap(obj) = self.out {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(obj, 2)(obj) };
        }
    }
}

type TargetObj = Com<Res<Mutex<Target>>>;

impl Target {
    fn transform(&self) -> Matrix {
        self.state.transform.unwrap_or(Matrix::IDENTITY)
    }
    fn clip(&self) -> Option<Clip> {
        self.state.clips.last().copied()
    }
    fn fill(&mut self, polys: &[Vec<Pt>], rule: FillRule, paint: &dyn Fn(f32, f32) -> Color) {
        let clip = self.clip();
        let (w, h) = (self.w, self.h);
        let mut c = Canvas {
            w,
            h,
            stride: w as usize * 4,
            px: &mut self.px,
        };
        c.fill(polys, rule, clip, paint);
    }

    /// Copie entre la cible et sa destination (`to_wic` à EndDraw et Flush).
    /// Les cibles de fenêtre et de DC gardent leurs pixels d'un tracé à
    /// l'autre et les posent sur l'écran ou le DC, comme D2D le présente.
    unsafe fn sync_wic(&mut self, to_wic: bool) -> i32 {
        let wic = match self.out {
            Output::Wic(wic) => wic,
            Output::Hwnd(hwnd) => {
                if to_wic {
                    let hdc = super::user32::get_dc(hwnd);
                    self.present(hdc, 0, 0);
                    super::user32::release_dc(hwnd, hdc);
                }
                return S_OK;
            }
            Output::Dc { hdc, x, y } => {
                if to_wic && hdc != 0 {
                    self.present(hdc, x, y);
                }
                return S_OK;
            }
            Output::Bitmap(bitmap) => {
                let b = &Com::get(bitmap as *mut BitmapObj).data;
                let mut px = crate::dlls::lock(&b.px);
                if px.len() == self.px.len() {
                    if to_wic {
                        px.copy_from_slice(&self.px);
                    } else {
                        self.px.copy_from_slice(&px);
                    }
                }
                drop(px);
                if to_wic {
                    v11::flush_surface(bitmap);
                }
                return S_OK;
            }
            Output::None => return S_OK,
        };
        type Lock =
            unsafe extern "win64" fn(*mut c_void, *const c_void, u32, *mut *mut c_void) -> i32;
        type Stride = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
        type Data = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
        let mut lock = std::ptr::null_mut();
        let hr =
            method::<Lock>(wic, 8)(wic, std::ptr::null(), if to_wic { 2 } else { 1 }, &mut lock);
        if hr < 0 {
            return hr;
        }
        let (mut stride, mut size, mut data) = (0u32, 0u32, std::ptr::null_mut());
        method::<Stride>(lock, 4)(lock, &mut stride);
        method::<Data>(lock, 5)(lock, &mut size, &mut data);
        let row = self.w as usize * 4;
        for y in 0..self.h as usize {
            let wic = data.add(y * stride as usize);
            let own = self.px[y * row..].as_mut_ptr();
            if to_wic {
                std::ptr::copy_nonoverlapping(own, wic, row);
                if self.opaque {
                    for x in 0..self.w as usize {
                        *wic.add(x * 4 + 3) = 255;
                    }
                }
            } else {
                std::ptr::copy_nonoverlapping(wic, own, row);
                if self.opaque {
                    for x in 0..self.w as usize {
                        *own.add(x * 4 + 3) = 255;
                    }
                }
            }
        }
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(lock, 2)(lock);
        S_OK
    }
}

impl Target {
    /// Nouvelle taille : les pixels déjà rendus restent en haut à gauche.
    fn resize(&mut self, w: u32, h: u32) {
        if (w, h) == (self.w, self.h) {
            return;
        }
        let mut px = vec![0u8; w as usize * h as usize * 4];
        let rows = self.h.min(h) as usize;
        let cols = self.w.min(w) as usize * 4;
        for y in 0..rows {
            px[y * w as usize * 4..][..cols]
                .copy_from_slice(&self.px[y * self.w as usize * 4..][..cols]);
        }
        self.px = px;
        self.w = w;
        self.h = h;
    }

    /// Pose les pixels (BGRA prémultipliés, rendus opaques) sur `hdc`.
    unsafe fn present(&self, hdc: usize, x: i32, y: i32) {
        if self.w == 0 || self.h == 0 {
            return;
        }
        let mut px = self.px.clone();
        for p in px.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
        // BITMAPINFOHEADER 32 bits, lignes de haut en bas (hauteur négative).
        let mut info = [0u8; 40];
        info[0..4].copy_from_slice(&40u32.to_le_bytes());
        info[4..8].copy_from_slice(&(self.w as i32).to_le_bytes());
        info[8..12].copy_from_slice(&(-(self.h as i32)).to_le_bytes());
        info[12..14].copy_from_slice(&1u16.to_le_bytes());
        info[14..16].copy_from_slice(&32u16.to_le_bytes());
        super::gdi32_ext::set_di_bits_to_device(
            hdc,
            x,
            y,
            self.w,
            self.h,
            0,
            0,
            0,
            self.h,
            px.as_ptr() as *const c_void,
            info.as_ptr() as *const c_void,
            0,
        );
    }
}

unsafe fn target<'a>(this: *mut TargetObj) -> std::sync::MutexGuard<'a, Target> {
    crate::dlls::lock(&Com::get(this).data)
}

/// Couleur d'un pinceau Winlux (les pinceaux étrangers n'existent pas en D2D).
unsafe fn brush_color(brush: *mut c_void) -> Option<Color> {
    if brush.is_null() || *(brush as *const *const ()) != BRUSH_VT.ptr() {
        return None;
    }
    Some(Com::get(brush as *mut BrushObj).data.premul())
}

unsafe extern "win64" fn target_qi(
    this: *mut TargetObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if v11::is_device_context(this) {
        let mut iids = vec![IID_D2D1_RESOURCE, IID_D2D1_RENDER_TARGET];
        iids.extend_from_slice(v11::context_iids());
        return qi_list(this, riid, out, &iids);
    }
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_RENDER_TARGET,
            match target(this).out {
                Output::Hwnd(_) => IID_D2D1_HWND_RENDER_TARGET,
                Output::Dc { .. } => IID_D2D1_DC_RENDER_TARGET,
                Output::Bitmap(_) => IID_D2D1_BITMAP_RENDER_TARGET,
                Output::Wic(_) | Output::None => IID_D2D1_RENDER_TARGET,
            },
        ],
    )
}

/// D2D1_SIZE_U passé par valeur : un registre 64 bits.
unsafe extern "win64" fn target_create_bitmap(
    this: *mut TargetObj,
    size: u64,
    src: *const u8,
    pitch: u32,
    _props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let (w, h) = (size as u32, (size >> 32) as u32);
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let b = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    let obj = new_res(Com::get(this).factory, BITMAP_VT.ptr(), b);
    if !src.is_null() {
        bitmap_copy_from_memory(obj, std::ptr::null(), src, pitch);
    }
    put(out, obj)
}

/// CreateBitmapFromWicBitmap : lit la source WIC convertie en BGRA prémultiplié.
unsafe extern "win64" fn target_bitmap_from_wic(
    this: *mut TargetObj,
    src: *mut c_void,
    _props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let mut conv = std::ptr::null_mut();
    let hr = super::windowscodecs::wic_convert_bitmap_source(
        &super::windowscodecs::FMT_PBGRA32,
        src,
        &mut conv,
    );
    if hr < 0 {
        return hr;
    }
    type GetSize = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut u32) -> i32;
    type Copy = unsafe extern "win64" fn(*mut c_void, *const c_void, u32, u32, *mut u8) -> i32;
    let (mut w, mut h) = (0u32, 0u32);
    method::<GetSize>(conv, 3)(conv, &mut w, &mut h);
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let hr = method::<Copy>(conv, 7)(
        conv,
        std::ptr::null(),
        w * 4,
        px.len() as u32,
        px.as_mut_ptr(),
    );
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(conv, 2)(conv);
    if hr < 0 {
        return hr;
    }
    let b = Bitmap::new(w, h, px);
    put(out, new_res(Com::get(this).factory, BITMAP_VT.ptr(), b))
}

unsafe extern "win64" fn target_solid_brush(
    this: *mut TargetObj,
    c: *const ColorF,
    props: *const f32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || c.is_null() {
        return E_INVALIDARG;
    }
    let b = Brush {
        color: Mutex::new(*c),
        opacity: Mutex::new(if props.is_null() {
            1.0
        } else {
            (*props).clamp(0.0, 1.0)
        }),
    };
    put(out, new_res(Com::get(this).factory, BRUSH_VT.ptr(), b))
}

unsafe fn fill_path(this: *mut TargetObj, path: &Path, rule: FillRule, brush: *mut c_void) {
    let Some(c) = brush_color(brush) else { return };
    let mut t = target(this);
    let polys = path.fill_polys(&t.transform());
    t.fill(&polys, rule, &|_, _| c);
}

unsafe fn stroke_path(this: *mut TargetObj, path: &Path, brush: *mut c_void, width: f32) {
    let Some(c) = brush_color(brush) else { return };
    let mut t = target(this);
    let polys = path.stroke_polys(&t.transform(), width);
    t.fill(&polys, FillRule::NonZero, &|_, _| c);
}

// --- Texte (DirectWrite maison) ----------------------------------------------------

unsafe fn fill_text(
    this: *mut TargetObj,
    brush: *mut c_void,
    make: impl FnOnce(&mut Path, f32) -> bool,
) {
    let scale = target(this).transform().scale();
    let mut p = Path::default();
    if make(&mut p, scale) {
        fill_path(this, &p, FillRule::NonZero, brush);
    }
}

unsafe extern "win64" fn draw_text(
    this: *mut TargetObj,
    s: *const u16,
    len: u32,
    format: *mut c_void,
    rect: *const RectF,
    brush: *mut c_void,
    options: u32,
    _measuring: u32,
) {
    let Some(r) = rect.as_ref() else { return };
    if s.is_null() {
        return;
    }
    let text = std::slice::from_raw_parts(s, len as usize);
    let clip = options & 2 != 0; // D2D1_DRAW_TEXT_OPTIONS_CLIP
    if clip {
        push_clip(this, rect, 0);
    }
    fill_text(this, brush, |p, scale| {
        super::dwrite::text_outline(text, format, (r.l, r.t, r.r, r.b), p, scale)
    });
    if clip {
        pop_clip(this);
    }
}

unsafe extern "win64" fn draw_text_layout(
    this: *mut TargetObj,
    origin: u64,
    layout: *mut c_void,
    brush: *mut c_void,
    _options: u32,
) {
    fill_text(this, brush, |p, scale| {
        super::dwrite::layout_outline(layout, point(origin), p, scale)
    });
}

unsafe extern "win64" fn draw_glyph_run(
    this: *mut TargetObj,
    origin: u64,
    run: *const super::dwrite::GlyphRun,
    brush: *mut c_void,
    _measuring: u32,
) {
    fill_text(this, brush, |p, scale| {
        super::dwrite::glyph_run_outline(run, point(origin), p, scale)
    });
}

unsafe extern "win64" fn draw_line(
    this: *mut TargetObj,
    a: u64,
    b: u64,
    brush: *mut c_void,
    width: f32,
    _style: *mut c_void,
) {
    let mut p = Path::default();
    p.begin(point(a));
    p.line(point(b));
    stroke_path(this, &p, brush, width);
}
unsafe extern "win64" fn draw_rect(
    this: *mut TargetObj,
    r: *const RectF,
    brush: *mut c_void,
    width: f32,
    _style: *mut c_void,
) {
    if let Some(r) = r.as_ref() {
        stroke_path(this, &Path::rect(r.l, r.t, r.r, r.b), brush, width);
    }
}
unsafe extern "win64" fn fill_rect(this: *mut TargetObj, r: *const RectF, brush: *mut c_void) {
    if let Some(r) = r.as_ref() {
        fill_path(
            this,
            &Path::rect(r.l, r.t, r.r, r.b),
            FillRule::NonZero,
            brush,
        );
    }
}
unsafe extern "win64" fn draw_rounded(
    this: *mut TargetObj,
    r: *const RoundedRect,
    brush: *mut c_void,
    width: f32,
    _s: *mut c_void,
) {
    if let Some(r) = r.as_ref() {
        let p = Path::rounded_rect(r.rect.l, r.rect.t, r.rect.r, r.rect.b, r.rx, r.ry, 4.0);
        stroke_path(this, &p, brush, width);
    }
}
unsafe extern "win64" fn fill_rounded(
    this: *mut TargetObj,
    r: *const RoundedRect,
    brush: *mut c_void,
) {
    if let Some(r) = r.as_ref() {
        let p = Path::rounded_rect(r.rect.l, r.rect.t, r.rect.r, r.rect.b, r.rx, r.ry, 4.0);
        fill_path(this, &p, FillRule::NonZero, brush);
    }
}
unsafe extern "win64" fn draw_ellipse(
    this: *mut TargetObj,
    e: *const Ellipse,
    brush: *mut c_void,
    width: f32,
    _s: *mut c_void,
) {
    if let Some(e) = e.as_ref() {
        stroke_path(
            this,
            &Path::ellipse(e.x, e.y, e.rx, e.ry, 4.0),
            brush,
            width,
        );
    }
}
unsafe extern "win64" fn fill_ellipse(this: *mut TargetObj, e: *const Ellipse, brush: *mut c_void) {
    if let Some(e) = e.as_ref() {
        fill_path(
            this,
            &Path::ellipse(e.x, e.y, e.rx, e.ry, 4.0),
            FillRule::NonZero,
            brush,
        );
    }
}

/// Géométrie Winlux sous-jacente, ou `None` pour un objet étranger.
unsafe fn geometry_of(g: *mut c_void) -> Option<(Path, FillRule)> {
    if g.is_null() || *(g as *const *const ()) != GEOMETRY_VT.ptr() {
        return None;
    }
    let geo = &Com::get(g as *mut GeometryObj).data;
    Some((
        crate::dlls::lock(&geo.path).clone(),
        *crate::dlls::lock(&geo.rule),
    ))
}

unsafe extern "win64" fn draw_geometry(
    this: *mut TargetObj,
    g: *mut c_void,
    brush: *mut c_void,
    width: f32,
    _s: *mut c_void,
) {
    if let Some((p, _)) = geometry_of(g) {
        stroke_path(this, &p, brush, width);
    }
}
unsafe extern "win64" fn fill_geometry(
    this: *mut TargetObj,
    g: *mut c_void,
    brush: *mut c_void,
    _mask: *mut c_void,
) {
    if let Some((p, rule)) = geometry_of(g) {
        fill_path(this, &p, rule, brush);
    }
}

/// DrawBitmap(bitmap, destination, opacité, interpolation, source).
unsafe extern "win64" fn draw_bitmap(
    this: *mut TargetObj,
    bitmap: *mut c_void,
    dst: *const RectF,
    opacity: f32,
    interpolation: u32,
    src: *const RectF,
) {
    if bitmap.is_null() || *(bitmap as *const *const ()) != BITMAP_VT.ptr() {
        return;
    }
    let b = &Com::get(bitmap as *mut BitmapObj).data;
    let px = crate::dlls::lock(&b.px).clone();
    let (bw, bh) = (b.w as f32, b.h as f32);
    let s = src.as_ref().copied().unwrap_or(RectF {
        l: 0.0,
        t: 0.0,
        r: bw,
        b: bh,
    });
    let d = dst.as_ref().copied().unwrap_or(RectF {
        l: 0.0,
        t: 0.0,
        r: s.r - s.l,
        b: s.b - s.t,
    });
    draw_bitmap_pixels(this, &px, b.w, b.h, &d, opacity, interpolation, &s);
}

/// Dessine des pixels BGRA prémultipliés `w`×`h` : la zone `s` de l'image
/// vers le rectangle `d`, sous la transformation courante.
#[allow(clippy::too_many_arguments)]
unsafe fn draw_bitmap_pixels(
    this: *mut TargetObj,
    px: &[u8],
    w: u32,
    h: u32,
    d: &RectF,
    opacity: f32,
    interpolation: u32,
    s: &RectF,
) {
    let (d, s) = (*d, *s);
    if d.r == d.l || d.b == d.t || s.r == s.l || s.b == s.t || w == 0 || h == 0 {
        return;
    }
    if px.len() < w as usize * h as usize * 4 {
        return;
    }
    let mut t = target(this);
    let m = t.transform();
    let Some(inv) = m.invert() else { return };
    let polys = Path::rect(d.l, d.t, d.r, d.b).fill_polys(&m);
    let (sx, sy) = ((s.r - s.l) / (d.r - d.l), (s.b - s.t) / (d.b - d.t));
    let (w, h) = (w as i32, h as i32);
    let at = |x: i32, y: i32| -> [f32; 4] {
        let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        let o = (y * w + x) as usize * 4;
        [
            px[o + 2] as f32,
            px[o + 1] as f32,
            px[o] as f32,
            px[o + 3] as f32,
        ]
    };
    let opacity = opacity.clamp(0.0, 1.0) / 255.0;
    t.fill(&polys, FillRule::NonZero, &|x, y| {
        let (ux, uy) = inv.apply((x, y));
        let (u, v) = (s.l + (ux - d.l) * sx, s.t + (uy - d.t) * sy);
        let c = if interpolation == 0 {
            at(u.floor() as i32, v.floor() as i32)
        } else {
            let (fu, fv) = (u - 0.5, v - 0.5);
            let (x0, y0) = (fu.floor() as i32, fv.floor() as i32);
            let (tx, ty) = (fu - x0 as f32, fv - y0 as f32);
            let (a, b2, c2, d2) = (
                at(x0, y0),
                at(x0 + 1, y0),
                at(x0, y0 + 1),
                at(x0 + 1, y0 + 1),
            );
            let mut o = [0f32; 4];
            for i in 0..4 {
                o[i] = (a[i] * (1.0 - tx) + b2[i] * tx) * (1.0 - ty)
                    + (c2[i] * (1.0 - tx) + d2[i] * tx) * ty;
            }
            o
        };
        [
            c[0] * opacity,
            c[1] * opacity,
            c[2] * opacity,
            c[3] * opacity,
        ]
    });
}

unsafe extern "win64" fn set_transform(this: *mut TargetObj, m: *const Matrix) {
    if !m.is_null() {
        target(this).state.transform = Some(*m);
    }
}
unsafe extern "win64" fn get_transform(this: *mut TargetObj, m: *mut Matrix) {
    if !m.is_null() {
        *m = target(this).transform();
    }
}
unsafe extern "win64" fn get_zero(_this: *mut c_void) -> u32 {
    0
}
unsafe extern "win64" fn set_tags(this: *mut TargetObj, a: u64, b: u64) {
    target(this).state.tags = (a, b);
}
unsafe extern "win64" fn get_tags(this: *mut TargetObj, a: *mut u64, b: *mut u64) {
    let tags = target(this).state.tags;
    if !a.is_null() {
        *a = tags.0;
    }
    if !b.is_null() {
        *b = tags.1;
    }
}
unsafe extern "win64" fn flush(this: *mut TargetObj, a: *mut u64, b: *mut u64) -> i32 {
    get_tags(this, a, b);
    let mut t = target(this);
    if t.drawing {
        t.sync_wic(true)
    } else {
        S_OK
    }
}

/// PushAxisAlignedClip : rectangle transformé, réduit à son enveloppe et
/// croisé avec la découpe courante.
unsafe extern "win64" fn push_clip(this: *mut TargetObj, r: *const RectF, _mode: u32) {
    let Some(r) = r.as_ref() else { return };
    let mut t = target(this);
    let m = t.transform();
    let pts = [
        m.apply((r.l, r.t)),
        m.apply((r.r, r.t)),
        m.apply((r.r, r.b)),
        m.apply((r.l, r.b)),
    ];
    let mut c = Clip {
        l: pts.iter().map(|p| p.0).fold(f32::MAX, f32::min),
        t: pts.iter().map(|p| p.1).fold(f32::MAX, f32::min),
        r: pts.iter().map(|p| p.0).fold(f32::MIN, f32::max),
        b: pts.iter().map(|p| p.1).fold(f32::MIN, f32::max),
    };
    if let Some(o) = t.clip() {
        c = Clip {
            l: c.l.max(o.l),
            t: c.t.max(o.t),
            r: c.r.min(o.r),
            b: c.b.min(o.b),
        };
    }
    t.state.clips.push(c);
}
unsafe extern "win64" fn pop_clip(this: *mut TargetObj) {
    target(this).state.clips.pop();
}

unsafe extern "win64" fn clear(this: *mut TargetObj, c: *const ColorF) {
    let c = c.as_ref().copied().unwrap_or_default();
    let col = premul(c.r, c.g, c.b, c.a);
    let mut t = target(this);
    let (w, h) = (t.w as f32, t.h as f32);
    let clip = t.clip().unwrap_or(Clip {
        l: 0.0,
        t: 0.0,
        r: w,
        b: h,
    });
    let stride = t.w as usize * 4;
    let v = super::d2d_raster::to_bgra(col);
    let (x0, x1) = (
        clip.l.max(0.0).round() as usize,
        clip.r.min(w).round() as usize,
    );
    let (y0, y1) = (
        clip.t.max(0.0).round() as usize,
        clip.b.min(h).round() as usize,
    );
    for y in y0..y1 {
        for x in x0..x1 {
            t.px[y * stride + x * 4..][..4].copy_from_slice(&v);
        }
    }
}

unsafe extern "win64" fn begin_draw(this: *mut TargetObj) {
    let mut t = target(this);
    if !t.drawing {
        t.drawing = true;
        t.sync_wic(false);
    }
}

unsafe extern "win64" fn end_draw(this: *mut TargetObj, a: *mut u64, b: *mut u64) -> i32 {
    get_tags(this, a, b);
    let mut t = target(this);
    if !t.drawing {
        return D2DERR_WRONG_STATE;
    }
    t.drawing = false;
    t.state.clips.clear();
    t.sync_wic(true)
}

unsafe extern "win64" fn get_size(this: *mut TargetObj, out: *mut [f32; 2]) -> *mut [f32; 2] {
    if !out.is_null() {
        let t = target(this);
        *out = [t.w as f32, t.h as f32];
    }
    out
}
unsafe extern "win64" fn get_pixel_size(this: *mut TargetObj, out: *mut [u32; 2]) -> *mut [u32; 2] {
    if !out.is_null() {
        let t = target(this);
        *out = [t.w, t.h];
    }
    out
}
unsafe extern "win64" fn max_bitmap_size(_this: *mut c_void) -> u32 {
    16384
}
unsafe extern "win64" fn is_supported(_this: *mut c_void, _props: *const c_void) -> i32 {
    1
}

const TARGET_METHODS: [*const (); 57] = [
    target_qi as *const (),
    add_ref::<Res<Mutex<Target>>> as *const (),
    release::<Res<Mutex<Target>>> as *const (),
    get_factory::<Mutex<Target>> as *const (),
    target_create_bitmap as *const (),
    target_bitmap_from_wic as *const (),
    unsupported!("CreateSharedBitmap"),
    unsupported!("CreateBitmapBrush"),
    target_solid_brush as *const (),
    v11::gradient_stops as *const (),
    unsupported!("CreateLinearGradientBrush"),
    unsupported!("CreateRadialGradientBrush"),
    create_compatible_target as *const (),
    unsupported!("CreateLayer"),
    unsupported!("CreateMesh"),
    draw_line as *const (),
    draw_rect as *const (),
    fill_rect as *const (),
    draw_rounded as *const (),
    fill_rounded as *const (),
    draw_ellipse as *const (),
    fill_ellipse as *const (),
    draw_geometry as *const (),
    fill_geometry as *const (),
    nothing as *const (), // FillMesh
    nothing as *const (), // FillOpacityMask
    draw_bitmap as *const (),
    draw_text as *const (),
    draw_text_layout as *const (),
    draw_glyph_run as *const (),
    set_transform as *const (),
    get_transform as *const (),
    nothing as *const (),  // SetAntialiasMode
    get_zero as *const (), // GetAntialiasMode : D2D1_ANTIALIAS_MODE_PER_PRIMITIVE
    nothing as *const (),  // SetTextAntialiasMode
    get_zero as *const (), // GetTextAntialiasMode
    nothing as *const (),  // SetTextRenderingParams
    nothing as *const (),  // GetTextRenderingParams
    set_tags as *const (),
    get_tags as *const (),
    v11::push_layer as *const (),
    v11::pop_layer as *const (),
    flush as *const (),
    v11::save_state as *const (),
    v11::restore_state as *const (),
    push_clip as *const (),
    pop_clip as *const (),
    clear as *const (),
    begin_draw as *const (),
    end_draw as *const (),
    pixel_format as *const (),
    v11::set_dpi as *const (),
    v11::get_dpi as *const (),
    get_size as *const (),
    get_pixel_size as *const (),
    max_bitmap_size as *const (),
    is_supported as *const (),
];

static TARGET_VT: VTable<57> = VTable(TARGET_METHODS);

/// Les 57 méthodes d'ID2D1RenderTarget suivies de celles de l'interface
/// dérivée.
const fn derived<const N: usize>(extra: &[*const ()]) -> [*const (); N] {
    let mut out = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < N {
        out[i] = if i < 57 {
            TARGET_METHODS[i]
        } else {
            extra[i - 57]
        };
        i += 1;
    }
    out
}

/// ID2D1HwndRenderTarget : CheckWindowState, Resize, GetHwnd.
static HWND_TARGET_VT: VTable<60> = VTable(derived::<60>(&[
    check_window_state as *const (),
    hwnd_resize as *const (),
    get_hwnd as *const (),
]));

/// ID2D1DCRenderTarget : BindDC.
static DC_TARGET_VT: VTable<58> = VTable(derived::<58>(&[bind_dc as *const ()]));

/// ID2D1BitmapRenderTarget : GetBitmap.
static BITMAP_TARGET_VT: VTable<58> = VTable(derived::<58>(&[get_bitmap as *const ()]));

/// CreateCompatibleRenderTarget : cible hors écran dont le bitmap se dessine
/// ensuite sur la cible d'origine (tampon de ligne de Scintilla, #551).
unsafe extern "win64" fn create_compatible_target(
    this: *mut TargetObj,
    size: *const [f32; 2],
    pixel_size: *const [u32; 2],
    _format: *const c_void,
    _options: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (w, h) = if let Some(&[w, h]) = pixel_size.as_ref() {
        (w, h)
    } else if let Some(&[w, h]) = size.as_ref() {
        (w.ceil().max(0.0) as u32, h.ceil().max(0.0) as u32)
    } else {
        let t = target(this);
        (t.w, t.h)
    };
    let (w, h) = (w.max(1), h.max(1));
    if crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let factory = Com::get(this).factory;
    let bitmap = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    let bitmap = new_res(factory, BITMAP_VT.ptr(), bitmap) as *mut c_void;
    let target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: false,
        out: Output::Bitmap(bitmap),
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    put(
        out,
        new_res(factory, BITMAP_TARGET_VT.ptr(), Mutex::new(target)),
    )
}

unsafe extern "win64" fn get_bitmap(this: *mut TargetObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Output::Bitmap(bitmap) = target(this).out else {
        *out = std::ptr::null_mut();
        return E_NOINTERFACE;
    };
    Com::add_ref(bitmap as *mut BitmapObj);
    *out = bitmap;
    S_OK
}

unsafe extern "win64" fn check_window_state(_this: *mut TargetObj) -> u32 {
    0 // D2D1_WINDOW_STATE_NONE
}

unsafe extern "win64" fn hwnd_resize(this: *mut TargetObj, size: *const [u32; 2]) -> i32 {
    let Some(&[w, h]) = size.as_ref() else {
        return E_INVALIDARG;
    };
    let mut t = target(this);
    t.resize(w, h);
    S_OK
}

unsafe extern "win64" fn get_hwnd(this: *mut TargetObj) -> usize {
    match target(this).out {
        Output::Hwnd(hwnd) => hwnd,
        _ => 0,
    }
}

/// BindDC(hdc, rect) : la cible prend la taille du rectangle et rendra à
/// son coin haut gauche dans ce DC.
unsafe extern "win64" fn bind_dc(this: *mut TargetObj, hdc: usize, rect: *const [i32; 4]) -> i32 {
    let Some(&[l, t, r, b]) = rect.as_ref() else {
        return E_INVALIDARG;
    };
    if hdc == 0 {
        return E_INVALIDARG;
    }
    let mut tg = target(this);
    tg.resize((r - l).max(0) as u32, (b - t).max(0) as u32);
    tg.out = Output::Dc { hdc, x: l, y: t };
    S_OK
}

// --- Point d'entrée -----------------------------------------------------------------

/// D2D1CreateFactory(type, riid, options, out) : une fabrique (mono ou
/// multithread, le rendu logiciel est protégé par verrous dans les deux cas).
pub unsafe extern "win64" fn d2d1_create_factory(
    _kind: u32,
    riid: *const Guid,
    _options: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let f = Com::create(FACTORY_VT.ptr(), Factory);
    let hr = factory_qi(f, riid, out);
    Com::release(f);
    hr
}

fn translation(dx: f32, dy: f32) -> Matrix {
    Matrix {
        dx,
        dy,
        ..Matrix::IDENTITY
    }
}

/// Une transformation `m` appliquée autour du point `center`.
fn around(m: Matrix, center: u64) -> Matrix {
    let (cx, cy) = point(center);
    translation(-cx, -cy).then(&m).then(&translation(cx, cy))
}

/// D2D1MakeRotateMatrix(angle en degrés, centre, sortie).
pub unsafe extern "win64" fn make_rotate_matrix(angle: f32, center: u64, out: *mut Matrix) {
    let (s, c) = angle.to_radians().sin_cos();
    if let Some(out) = out.as_mut() {
        *out = around(
            Matrix {
                m11: c,
                m12: s,
                m21: -s,
                m22: c,
                dx: 0.0,
                dy: 0.0,
            },
            center,
        );
    }
}

/// D2D1MakeSkewMatrix(angle x, angle y, centre, sortie).
pub unsafe extern "win64" fn make_skew_matrix(ax: f32, ay: f32, center: u64, out: *mut Matrix) {
    if let Some(out) = out.as_mut() {
        let m = Matrix {
            m12: ay.to_radians().tan(),
            m21: ax.to_radians().tan(),
            ..Matrix::IDENTITY
        };
        *out = around(m, center);
    }
}

pub unsafe extern "win64" fn is_matrix_invertible(m: *const Matrix) -> i32 {
    m.as_ref().and_then(|m| m.invert()).is_some() as i32
}

pub unsafe extern "win64" fn invert_matrix(m: *mut Matrix) -> i32 {
    match m.as_mut() {
        Some(m) => match m.invert() {
            Some(i) => {
                *m = i;
                1
            }
            None => 0,
        },
        None => 0,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D2D1CreateFactory" => fn d2d1_create_factory,
        "D2D1CreateDevice" => fn v11::d2d1_create_device,
        "D2D1CreateDeviceContext" => fn v11::d2d1_create_device_context,
        "D2D1MakeRotateMatrix" => fn make_rotate_matrix,
        "D2D1MakeSkewMatrix" => fn make_skew_matrix,
        "D2D1IsMatrixInvertible" => fn is_matrix_invertible,
        "D2D1InvertMatrix" => fn invert_matrix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::windowscodecs::{wic_create_imaging_factory_proxy, FMT_PBGRA32};

    type Unary = unsafe extern "win64" fn(*mut c_void) -> u32;

    fn pt(x: f32, y: f32) -> u64 {
        x.to_bits() as u64 | (y.to_bits() as u64) << 32
    }

    unsafe fn release_obj(o: *mut c_void) {
        method::<Unary>(o, 2)(o);
    }

    /// Bitmap WIC PBGRA 8×8, fabrique Direct2D et cible de rendu dessus.
    unsafe fn setup() -> (*mut c_void, *mut c_void, *mut c_void) {
        let mut wic = std::ptr::null_mut();
        assert_eq!(wic_create_imaging_factory_proxy(0x237, &mut wic), S_OK);
        type CreateBmp = unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            *const Guid,
            u32,
            *mut *mut c_void,
        ) -> i32;
        let mut bmp = std::ptr::null_mut();
        assert_eq!(
            method::<CreateBmp>(wic, 17)(wic, 8, 8, &FMT_PBGRA32, 0, &mut bmp),
            S_OK
        );
        release_obj(wic);
        let mut f = std::ptr::null_mut();
        assert_eq!(
            d2d1_create_factory(0, &IID_D2D1_FACTORY, std::ptr::null(), &mut f),
            S_OK
        );
        type CreateRt = unsafe extern "win64" fn(
            *mut c_void,
            *mut c_void,
            *const c_void,
            *mut *mut c_void,
        ) -> i32;
        let mut rt = std::ptr::null_mut();
        assert_eq!(
            method::<CreateRt>(f, 13)(f, bmp, std::ptr::null(), &mut rt),
            S_OK
        );
        (f, bmp, rt)
    }

    unsafe fn pixels(bmp: *mut c_void) -> Vec<u8> {
        type Copy = unsafe extern "win64" fn(*mut c_void, *const c_void, u32, u32, *mut u8) -> i32;
        let mut buf = vec![0u8; 8 * 8 * 4];
        assert_eq!(
            method::<Copy>(bmp, 7)(
                bmp,
                std::ptr::null(),
                32,
                buf.len() as u32,
                buf.as_mut_ptr()
            ),
            S_OK
        );
        buf
    }

    fn px(buf: &[u8], x: usize, y: usize) -> [u8; 4] {
        buf[(y * 8 + x) * 4..][..4].try_into().unwrap()
    }

    unsafe fn solid(rt: *mut c_void, c: ColorF) -> *mut c_void {
        type Brush = unsafe extern "win64" fn(
            *mut c_void,
            *const ColorF,
            *const f32,
            *mut *mut c_void,
        ) -> i32;
        let mut b = std::ptr::null_mut();
        assert_eq!(
            method::<Brush>(rt, 8)(rt, &c, std::ptr::null(), &mut b),
            S_OK
        );
        b
    }

    #[test]
    fn dessine_sur_un_bitmap_wic() {
        unsafe {
            let (f, bmp, rt) = setup();
            method::<Unary>(rt, 48)(rt);
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            method::<Clear>(rt, 47)(
                rt,
                &ColorF {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            let red = solid(
                rt,
                ColorF {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            type FillRect = unsafe extern "win64" fn(*mut c_void, *const RectF, *mut c_void);
            method::<FillRect>(rt, 17)(
                rt,
                &RectF {
                    l: 2.0,
                    t: 2.0,
                    r: 6.0,
                    b: 6.0,
                },
                red,
            );
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let p = pixels(bmp);
            assert_eq!(px(&p, 0, 0), [255, 255, 255, 255]);
            assert_eq!(px(&p, 3, 3), [0, 0, 255, 255]);
            assert_eq!(px(&p, 6, 6), [255, 255, 255, 255]);
            // Une deuxième passe repart du contenu du bitmap.
            method::<Unary>(rt, 48)(rt);
            let half = solid(
                rt,
                ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.5,
                },
            );
            method::<FillRect>(rt, 17)(
                rt,
                &RectF {
                    l: 0.0,
                    t: 0.0,
                    r: 1.0,
                    b: 1.0,
                },
                half,
            );
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let p = pixels(bmp);
            let c = px(&p, 0, 0);
            assert!(
                c[0] == 255 && (126..=129).contains(&c[1]) && c[3] == 255,
                "{c:?}"
            );
            assert_eq!(px(&p, 3, 3), [0, 0, 255, 255]);
            for o in [half, red, rt, bmp, f] {
                release_obj(o);
            }
        }
    }

    #[test]
    fn geometrie_de_chemin_et_test_de_contenance() {
        unsafe {
            let (f, bmp, rt) = setup();
            let mut g = std::ptr::null_mut();
            type CreatePath = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            assert_eq!(method::<CreatePath>(f, 10)(f, &mut g), S_OK);
            let mut sink = std::ptr::null_mut();
            assert_eq!(method::<CreatePath>(g, 17)(g, &mut sink), S_OK);
            type Begin = unsafe extern "win64" fn(*mut c_void, u64, u32);
            type Line = unsafe extern "win64" fn(*mut c_void, u64);
            type EndFig = unsafe extern "win64" fn(*mut c_void, u32);
            type Close = unsafe extern "win64" fn(*mut c_void) -> i32;
            method::<Begin>(sink, 5)(sink, pt(0.0, 0.0), 0);
            method::<Line>(sink, 10)(sink, pt(8.0, 0.0));
            method::<Line>(sink, 10)(sink, pt(0.0, 8.0));
            method::<EndFig>(sink, 8)(sink, 1);
            assert_eq!(method::<Close>(sink, 9)(sink), S_OK);
            release_obj(sink);
            type Contains =
                unsafe extern "win64" fn(*mut c_void, u64, *const Matrix, f32, *mut i32) -> i32;
            let mut inside = 0;
            assert_eq!(
                method::<Contains>(g, 7)(g, pt(1.0, 1.0), std::ptr::null(), 0.25, &mut inside),
                S_OK
            );
            assert_eq!(inside, 1);
            assert_eq!(
                method::<Contains>(g, 7)(g, pt(7.0, 7.0), std::ptr::null(), 0.25, &mut inside),
                S_OK
            );
            assert_eq!(inside, 0);
            type Bounds = unsafe extern "win64" fn(*mut c_void, *const Matrix, *mut RectF) -> i32;
            let mut r = RectF {
                l: 0.0,
                t: 0.0,
                r: 0.0,
                b: 0.0,
            };
            assert_eq!(method::<Bounds>(g, 4)(g, std::ptr::null(), &mut r), S_OK);
            assert_eq!((r.l, r.t, r.r, r.b), (0.0, 0.0, 8.0, 8.0));

            method::<Unary>(rt, 48)(rt);
            let black = solid(
                rt,
                ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            type Fill =
                unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void);
            method::<Fill>(rt, 23)(rt, g, black, std::ptr::null_mut());
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let p = pixels(bmp);
            assert_eq!(px(&p, 1, 1), [0, 0, 0, 255]);
            assert_eq!(px(&p, 6, 6), [0, 0, 0, 0]);
            for o in [black, g, rt, bmp, f] {
                release_obj(o);
            }
        }
    }

    #[test]
    fn matrices_utilitaires() {
        unsafe {
            let mut m = Matrix::IDENTITY;
            make_rotate_matrix(90.0, pt(1.0, 1.0), &mut m);
            let (x, y) = m.apply((2.0, 1.0));
            assert!((x - 1.0).abs() < 1e-5 && (y - 2.0).abs() < 1e-5, "{x} {y}");
            assert_eq!(is_matrix_invertible(&m), 1);
            assert_eq!(invert_matrix(&mut m), 1);
            let (x, y) = m.apply((1.0, 2.0));
            assert!((x - 2.0).abs() < 1e-5 && (y - 1.0).abs() < 1e-5, "{x} {y}");
            let flat = Matrix {
                m22: 0.0,
                ..Matrix::IDENTITY
            };
            assert_eq!(is_matrix_invertible(&flat), 0);
            make_skew_matrix(45.0, 0.0, pt(0.0, 0.0), &mut m);
            let (x, y) = m.apply((0.0, 1.0));
            assert!((x - 1.0).abs() < 1e-5 && y == 1.0, "{x} {y}");
        }
    }

    #[test]
    fn texte_dessine_par_directwrite() {
        use crate::dlls::dwrite::tests as dw;
        unsafe {
            let (f, bmp, rt) = setup();
            let dwf = dw::factory();
            let layout = dw::layout(dwf, "A", 100.0);
            method::<Unary>(rt, 48)(rt);
            let black = solid(
                rt,
                ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            type DrawLayout =
                unsafe extern "win64" fn(*mut c_void, u64, *mut c_void, *mut c_void, u32);
            method::<DrawLayout>(rt, 28)(rt, pt(0.0, 0.0), layout, black, 0);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            // « A » est un carré de 1 à 6 en x, de 2 à 8 en y (ligne de base 8).
            let p = pixels(bmp);
            assert_eq!(px(&p, 3, 5), [0, 0, 0, 255]);
            assert_eq!(px(&p, 7, 1), [0, 0, 0, 0]);
            assert_eq!(px(&p, 0, 5), [0, 0, 0, 0]);

            // Contour d'une suite de glyphes vers une géométrie Direct2D.
            let mut g = std::ptr::null_mut();
            type CreatePath = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            assert_eq!(method::<CreatePath>(f, 10)(f, &mut g), S_OK);
            let mut sink = std::ptr::null_mut();
            assert_eq!(method::<CreatePath>(g, 17)(g, &mut sink), S_OK);
            let mut face = std::ptr::null_mut();
            {
                let mut c = std::ptr::null_mut();
                type Sys = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void, i32) -> i32;
                assert_eq!(method::<Sys>(dwf, 3)(dwf, &mut c, 0), S_OK);
                let name = dw::wide("Winlux Test");
                let (mut idx, mut ex) = (0u32, 0i32);
                type Find =
                    unsafe extern "win64" fn(*mut c_void, *const u16, *mut u32, *mut i32) -> i32;
                method::<Find>(c, 5)(c, name.as_ptr(), &mut idx, &mut ex);
                let mut fam = std::ptr::null_mut();
                type Get = unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
                assert_eq!(method::<Get>(c, 4)(c, idx, &mut fam), S_OK);
                let mut font = std::ptr::null_mut();
                assert_eq!(method::<Get>(fam, 5)(fam, 0, &mut font), S_OK);
                type Mk = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
                assert_eq!(method::<Mk>(font, 13)(font, &mut face), S_OK);
                for o in [font, fam, c] {
                    release_obj(o);
                }
            }
            type Outline = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                *const u16,
                *const f32,
                *const c_void,
                u32,
                i32,
                i32,
                *mut c_void,
            ) -> i32;
            let glyphs = [1u16, 2];
            assert_eq!(
                method::<Outline>(face, 14)(
                    face,
                    10.0,
                    glyphs.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    2,
                    0,
                    0,
                    sink
                ),
                S_OK
            );
            type Close = unsafe extern "win64" fn(*mut c_void) -> i32;
            assert_eq!(method::<Close>(sink, 9)(sink), S_OK);
            release_obj(sink);
            type Contains =
                unsafe extern "win64" fn(*mut c_void, u64, *const Matrix, f32, *mut i32) -> i32;
            let mut inside = 0;
            method::<Contains>(g, 7)(g, pt(3.0, -3.0), std::ptr::null(), 0.25, &mut inside);
            assert_eq!(inside, 1);
            method::<Contains>(g, 7)(g, pt(3.0, 3.0), std::ptr::null(), 0.25, &mut inside);
            assert_eq!(inside, 0);
            // Le losange du deuxième glyphe commence après l'avance du premier (7).
            method::<Contains>(g, 7)(g, pt(10.5, -3.0), std::ptr::null(), 0.25, &mut inside);
            assert_eq!(inside, 1);
            for o in [g, face, black, layout, dwf, rt, bmp, f] {
                release_obj(o);
            }
        }
    }

    /// Cible hors écran (CreateCompatibleRenderTarget + GetBitmap) dessinée
    /// sur la cible d'origine, puis cible de DC (BindDC) : chemins de
    /// Scintilla en DirectWrite (#551).
    #[test]
    fn cible_compatible_et_cible_de_dc() {
        unsafe {
            let (f, bmp, rt) = setup();
            type Compat = unsafe extern "win64" fn(
                *mut c_void,
                *const [f32; 2],
                *const [u32; 2],
                *const c_void,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut off = std::ptr::null_mut();
            assert_eq!(
                method::<Compat>(rt, 12)(
                    rt,
                    &[4.0, 4.0],
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    &mut off
                ),
                S_OK
            );
            let red = ColorF {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            method::<Unary>(off, 48)(off);
            method::<Clear>(off, 47)(off, &red);
            assert_eq!(
                method::<End>(off, 49)(off, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let mut b = std::ptr::null_mut();
            type GetBitmap = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            assert_eq!(method::<GetBitmap>(off, 57)(off, &mut b), S_OK);
            type DrawBitmap = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const RectF,
                f32,
                u32,
                *const RectF,
            );
            method::<Unary>(rt, 48)(rt);
            method::<Clear>(rt, 47)(
                rt,
                &ColorF {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            let dst = RectF {
                l: 0.0,
                t: 0.0,
                r: 4.0,
                b: 4.0,
            };
            method::<DrawBitmap>(rt, 26)(rt, b, &dst, 1.0, 0, std::ptr::null());
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let p = pixels(bmp);
            assert_eq!(px(&p, 1, 1), [0, 0, 255, 255]);
            assert_eq!(px(&p, 6, 6), [255, 255, 255, 255]);
            release_obj(b);
            release_obj(off);

            // Cible de DC : rendu posé dans le DC lié, à l'emplacement donné.
            use crate::dlls::gdi32::{self, BitmapInfoHeader};
            let info = BitmapInfoHeader {
                size: 40,
                width: 8,
                height: -8,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_pels_per_meter: 0,
                y_pels_per_meter: 0,
                colors_used: 0,
                colors_important: 0,
            };
            let mut bits = std::ptr::null_mut();
            let dib = gdi32::create_dib_section(0, &info, 0, &mut bits, 0, 0);
            let hdc = gdi32::create_compatible_dc(0);
            gdi32::select_object(hdc, dib);
            type CreateDc =
                unsafe extern "win64" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32;
            let mut dct = std::ptr::null_mut();
            assert_eq!(
                method::<CreateDc>(f, 16)(f, std::ptr::null(), &mut dct),
                S_OK
            );
            type Bind = unsafe extern "win64" fn(*mut c_void, usize, *const [i32; 4]) -> i32;
            assert_eq!(method::<Bind>(dct, 57)(dct, hdc, &[2, 2, 6, 6]), S_OK);
            method::<Unary>(dct, 48)(dct);
            method::<Clear>(dct, 47)(
                dct,
                &ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            assert_eq!(
                method::<End>(dct, 49)(dct, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let at = |x: usize, y: usize| *(bits as *const u32).add(y * 8 + x) & 0xFF_FFFF;
            assert_eq!(at(3, 3), 0x00_00FF, "bleu dans le rectangle lié");
            assert_eq!(at(0, 0), 0, "rien hors du rectangle");
            release_obj(dct);
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
    }
}
