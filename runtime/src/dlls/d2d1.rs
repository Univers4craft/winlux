//! Direct2D 1.0 (`d2d1.dll`) écrit pour Winlux, en rendu logiciel.
//!
//! La fabrique crée des géométries (rectangle, rectangle arrondi, ellipse,
//! chemin avec son `ID2D1GeometrySink`) et des cibles de rendu sur bitmap
//! WIC. Une cible dessine avec `d2d_raster` : effacement, rectangles,
//! ellipses, géométries, lignes, bitmaps, transformations, découpes
//! alignées sur les axes, pinceaux unis avec opacité. Les dégradés, les
//! calques, le texte (DirectWrite) et les cibles de fenêtre viendront
//! ensuite ; ces méthodes répondent E_NOTIMPL ou ne dessinent rien.

use super::audio_com::{
    guid, put, Com, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::d2d_raster::{premul, Canvas, Clip, Color, FillRule, Matrix, Path, Pt};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::Mutex;

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
        wic: bitmap,
        state: TargetState::default(),
        drawing: false,
    };
    put(out, new_res(this, TARGET_VT.ptr(), Mutex::new(target)))
}

static FACTORY_VT: VTable<17> = VTable([
    factory_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    not_implemented as *const (), // ReloadSystemMetrics
    factory_desktop_dpi as *const (),
    create_rect_geometry as *const (),
    create_rounded_rect_geometry as *const (),
    create_ellipse_geometry as *const (),
    not_implemented as *const (), // CreateGeometryGroup
    not_implemented as *const (), // CreateTransformedGeometry
    create_path_geometry as *const (),
    not_implemented as *const (), // CreateStrokeStyle
    not_implemented as *const (), // CreateDrawingStateBlock
    create_wic_render_target as *const (),
    not_implemented as *const (), // CreateHwndRenderTarget
    not_implemented as *const (), // CreateDxgiSurfaceRenderTarget
    not_implemented as *const (), // CreateDCRenderTarget
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
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_GEOMETRY, own],
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
static GEOMETRY_VT: VTable<21> = VTable([
    geometry_qi as *const (),
    add_ref::<Res<Geometry>> as *const (),
    release::<Res<Geometry>> as *const (),
    get_factory::<Geometry> as *const (),
    geometry_bounds as *const (),
    not_implemented as *const (), // GetWidenedBounds
    not_implemented as *const (), // StrokeContainsPoint
    geometry_fill_contains as *const (),
    not_implemented as *const (),   // CompareWithGeometry
    not_implemented as *const (),   // Simplify
    not_implemented as *const (),   // Tessellate
    not_implemented as *const (),   // CombineWithGeometry
    not_implemented as *const (),   // Outline
    not_implemented as *const (),   // ComputeArea
    not_implemented as *const (),   // ComputeLength
    not_implemented as *const (),   // ComputePointAtLength
    not_implemented as *const (),   // Widen
    path_open as *const (),         // ou GetRect / GetRoundedRect / GetEllipse
    not_implemented as *const (),   // Stream
    path_figure_count as *const (), // GetSegmentCount (approché)
    path_figure_count as *const (),
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
        &[IID_D2D1_RESOURCE, IID_D2D1_IMAGE, IID_D2D1_BITMAP],
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

static BITMAP_VT: VTable<11> = VTable([
    bitmap_qi as *const (),
    add_ref::<Res<Bitmap>> as *const (),
    release::<Res<Bitmap>> as *const (),
    get_factory::<Bitmap> as *const (),
    bitmap_size as *const (),
    bitmap_pixel_size as *const (),
    pixel_format as *const (),
    dpi_96 as *const (),
    not_implemented as *const (), // CopyFromBitmap
    not_implemented as *const (), // CopyFromRenderTarget
    bitmap_copy_from_memory as *const (),
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
    /// Bitmap WIC de destination (référence tenue).
    wic: *mut c_void,
    state: TargetState,
    drawing: bool,
}

impl Drop for Target {
    fn drop(&mut self) {
        unsafe {
            method::<unsafe extern "win64" fn(*mut c_void) -> u32>(self.wic, 2)(self.wic);
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

    /// Copie entre la cible et son bitmap WIC (`to_wic` à EndDraw).
    unsafe fn sync_wic(&mut self, to_wic: bool) -> i32 {
        type Lock =
            unsafe extern "win64" fn(*mut c_void, *const c_void, u32, *mut *mut c_void) -> i32;
        type Stride = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
        type Data = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
        let mut lock = std::ptr::null_mut();
        let hr = method::<Lock>(self.wic, 8)(
            self.wic,
            std::ptr::null(),
            if to_wic { 2 } else { 1 },
            &mut lock,
        );
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
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_RENDER_TARGET],
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
    let b = Bitmap {
        w,
        h,
        px: Mutex::new(vec![0; w as usize * h as usize * 4]),
    };
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
    let b = Bitmap {
        w,
        h,
        px: Mutex::new(px),
    };
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
    if d.r == d.l || d.b == d.t || s.r == s.l || s.b == s.t {
        return;
    }
    let px = crate::dlls::lock(&b.px).clone();
    let mut t = target(this);
    let m = t.transform();
    let Some(inv) = m.invert() else { return };
    let polys = Path::rect(d.l, d.t, d.r, d.b).fill_polys(&m);
    let (sx, sy) = ((s.r - s.l) / (d.r - d.l), (s.b - s.t) / (d.b - d.t));
    let (w, h) = (b.w as i32, b.h as i32);
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

static TARGET_VT: VTable<57> = VTable([
    target_qi as *const (),
    add_ref::<Res<Mutex<Target>>> as *const (),
    release::<Res<Mutex<Target>>> as *const (),
    get_factory::<Mutex<Target>> as *const (),
    target_create_bitmap as *const (),
    target_bitmap_from_wic as *const (),
    not_implemented as *const (), // CreateSharedBitmap
    not_implemented as *const (), // CreateBitmapBrush
    target_solid_brush as *const (),
    not_implemented as *const (), // CreateGradientStopCollection
    not_implemented as *const (), // CreateLinearGradientBrush
    not_implemented as *const (), // CreateRadialGradientBrush
    not_implemented as *const (), // CreateCompatibleRenderTarget
    not_implemented as *const (), // CreateLayer
    not_implemented as *const (), // CreateMesh
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
    nothing as *const (), // PushLayer
    nothing as *const (), // PopLayer
    flush as *const (),
    nothing as *const (), // SaveDrawingState
    nothing as *const (), // RestoreDrawingState
    push_clip as *const (),
    pop_clip as *const (),
    clear as *const (),
    begin_draw as *const (),
    end_draw as *const (),
    pixel_format as *const (),
    nothing as *const (), // SetDpi
    dpi_96 as *const (),
    get_size as *const (),
    get_pixel_size as *const (),
    max_bitmap_size as *const (),
    is_supported as *const (),
]);

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
}
