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
use super::d2d_raster::{premul, Canvas, Clip, Color, FillMode, FillRule, Matrix, Path, Pt};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

pub(crate) fn unsupported_call(name: &'static str) -> i32 {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut seen = crate::dlls::lock(&SEEN);
    if !seen.contains(&name) {
        seen.push(name);
        crate::log_warn!("com", "méthode {} non prise en charge (E_NOTIMPL)", name);
    }
    E_NOTIMPL
}

/// WINLUX_D2D_TRACE : une ligne par opération Direct2D notable (#1609).
macro_rules! d2d_trace {
    ($($a:tt)*) => {
        if crate::dlls::d2d1::v11::trace_on() {
            crate::dlls::d2d1::v11::trace(format_args!($($a)*));
        }
    };
}

#[path = "d2d1_1.rs"]
mod v11;

#[path = "d2d1_brush.rs"]
mod brushes;

/// Table de 120 relais « this = [this + 8], saut vers ([this + 16])[i] »
/// pour les vues d'interface (ID2D1DeviceContext d'une ancienne cible,
/// IDWriteTextFormat3 d'une mise en page).
pub(crate) fn tear_off_vt() -> *const () {
    v11::view_vt()
}

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const DXGI_FORMAT_A8_UNORM: u32 = 65;
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
        sources: Vec::new(),
        transform: Matrix::IDENTITY,
    };
    put(out, new_res(f, GEOMETRY_VT.ptr(), g))
}

/// CreateGeometryGroup(fillMode, geometries, count, out) : les figures des
/// sources (aplaties) sont réunies sous la règle du groupe.
unsafe extern "win64" fn create_geometry_group(
    this: *mut FactoryObj,
    mode: u32,
    geoms: *const *mut c_void,
    count: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if geoms.is_null() && count > 0 {
        return E_INVALIDARG;
    }
    let mut path = Path::default();
    let mut sources = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let g = *geoms.add(i);
        if g.is_null() {
            continue;
        }
        if let Some((p, _)) = geometry_of(g) {
            path.figures.extend(p.figures);
        }
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(g, 1)(g);
        sources.push(g);
    }
    let g = Geometry {
        kind: GeomKind::Group,
        path: Mutex::new(path),
        // D2D1_FILL_MODE_ALTERNATE 0, WINDING 1.
        rule: Mutex::new(if mode == 1 {
            FillRule::NonZero
        } else {
            FillRule::EvenOdd
        }),
        open: Mutex::new(false),
        sources,
        transform: Matrix::IDENTITY,
    };
    put(out, new_res(this, GROUP_VT.ptr(), g))
}

/// CreateTransformedGeometry(source, transform, out).
unsafe extern "win64" fn create_transformed_geometry(
    this: *mut FactoryObj,
    src: *mut c_void,
    m: *const Matrix,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if src.is_null() || m.is_null() {
        return E_INVALIDARG;
    }
    let m = *m;
    let (path, rule) = geometry_of(src).unwrap_or((Path::default(), FillRule::EvenOdd));
    let path = Path {
        figures: path
            .fill_polys(&m)
            .into_iter()
            .zip(path.figures.iter().map(|f| f.1))
            .collect(),
    };
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(src, 1)(src);
    let g = Geometry {
        kind: GeomKind::Transformed,
        path: Mutex::new(path),
        rule: Mutex::new(rule),
        open: Mutex::new(false),
        sources: vec![src],
        transform: m,
    };
    put(out, new_res(this, TRANSFORMED_VT.ptr(), g))
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

/// Disposition d'un bitmap WIC qui peut servir de cible (#1573) : BGRA ou
/// RGBA 32 bits prémultiplié, BGR 32 bits, ou alpha seul sur 8 bits (masque
/// de pinceau de Paint.NET).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WicLayout {
    Pbgra,
    Prgba,
    Bgr,
    Alpha8,
    /// 128bppPRGBAFloat : contexte WARP intermédiaire des effets GPU de
    /// Paint.NET (#1749), rendu ici en 8 bits.
    PrgbaF32,
}

impl WicLayout {
    fn of(fmt: &Guid) -> Option<WicLayout> {
        use super::wic_pixels::FMT_PRGBA_FLOAT128;
        use super::windowscodecs::{FMT_ALPHA8, FMT_BGR32, FMT_PBGRA32, FMT_PRGBA32};
        Some(match *fmt {
            FMT_PBGRA32 => WicLayout::Pbgra,
            FMT_PRGBA_FLOAT128 => WicLayout::PrgbaF32,
            FMT_PRGBA32 => WicLayout::Prgba,
            FMT_BGR32 => WicLayout::Bgr,
            FMT_ALPHA8 => WicLayout::Alpha8,
            _ => return None,
        })
    }

    fn bpp(self) -> usize {
        match self {
            WicLayout::Alpha8 => 1,
            WicLayout::PrgbaF32 => 16,
            _ => 4,
        }
    }

    /// Ligne du bitmap WIC vers la ligne BGRA prémultipliée de la cible.
    fn read(self, src: &[u8], dst: &mut [u8]) {
        for (d, s) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.chunks_exact(self.bpp()))
        {
            *d = match self {
                WicLayout::Pbgra => [s[0], s[1], s[2], s[3]],
                WicLayout::Prgba => [s[2], s[1], s[0], s[3]],
                WicLayout::Bgr => [s[0], s[1], s[2], 255],
                WicLayout::Alpha8 => [0, 0, 0, s[0]],
                WicLayout::PrgbaF32 => {
                    let c = |i: usize| {
                        let v = f32::from_le_bytes(s[i * 4..i * 4 + 4].try_into().unwrap());
                        (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                    };
                    [c(2), c(1), c(0), c(3)]
                }
            };
        }
    }

    /// Ligne BGRA prémultipliée de la cible vers la ligne du bitmap WIC.
    fn write(self, src: &[u8], dst: &mut [u8]) {
        for (d, s) in dst.chunks_exact_mut(self.bpp()).zip(src.as_chunks::<4>().0) {
            match self {
                WicLayout::Pbgra => d.copy_from_slice(s),
                WicLayout::Prgba => d.copy_from_slice(&[s[2], s[1], s[0], s[3]]),
                WicLayout::Bgr => d.copy_from_slice(&[s[0], s[1], s[2], 255]),
                WicLayout::Alpha8 => d[0] = s[3],
                WicLayout::PrgbaF32 => {
                    for (i, v) in [s[2], s[1], s[0], s[3]].into_iter().enumerate() {
                        d[i * 4..i * 4 + 4].copy_from_slice(&(v as f32 / 255.0).to_le_bytes());
                    }
                }
            }
        }
    }
}

/// CreateWicBitmapRenderTarget : la cible dessine dans le bitmap WIC
/// (formats de `WicLayout`), lu à BeginDraw et écrit à EndDraw.
unsafe extern "win64" fn create_wic_render_target(
    this: *mut FactoryObj,
    bitmap: *mut c_void,
    props: *const c_void,
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
    if WicLayout::of(&fmt).is_none() {
        crate::log_warn!(
            "d2d1",
            "CreateWicBitmapRenderTarget : format {:?} non pris en charge",
            fmt
        );
        return D2DERR_UNSUPPORTED_PIXEL_FORMAT;
    }
    use super::windowscodecs::FMT_BGR32;
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(bitmap, 1)(bitmap);
    let mut target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: fmt == FMT_BGR32,
        out: Output::Wic(bitmap),
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    let dpi = props_dpi(props);
    target.ext.set_dpi(dpi.0, dpi.1);
    let hr = put(out, new_res(this, TARGET_VT.ptr(), Mutex::new(target)));
    d2d_trace!(
        "CreateWicBitmapRenderTarget {:p} {w}x{h} {:?} dpi {}x{}",
        *out,
        fmt,
        dpi.0,
        dpi.1
    );
    hr
}

/// dpiX, dpiY de D2D1_RENDER_TARGET_PROPERTIES (type, format de pixel,
/// puis les DPI aux octets 12 et 16) ; 0 veut dire la valeur par défaut.
unsafe fn props_dpi(props: *const c_void) -> (f32, f32) {
    if props.is_null() {
        return (0.0, 0.0);
    }
    let p = props as *const u8;
    (
        std::ptr::read_unaligned(p.add(12) as *const f32),
        std::ptr::read_unaligned(p.add(16) as *const f32),
    )
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
    props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    let mut target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: true,
        out: out_to,
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    let dpi = props_dpi(props);
    target.ext.set_dpi(dpi.0, dpi.1);
    let hr = put(out, new_res(f, vt, Mutex::new(target)));
    d2d_trace!("cible écran {:p} {w}x{h}", *out);
    hr
}

/// CreateHwndRenderTarget : rendu logiciel posé sur la fenêtre à EndDraw
/// (Scintilla de Notepad++ en DirectWrite, #551).
unsafe extern "win64" fn create_hwnd_render_target(
    this: *mut FactoryObj,
    props: *const c_void,
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
    new_screen_target(
        this,
        w,
        h,
        Output::Hwnd(hp.hwnd),
        HWND_TARGET_VT.ptr(),
        props,
        out,
    )
}

/// CreateDCRenderTarget : la taille et le DC viennent de BindDC.
unsafe extern "win64" fn create_dc_render_target(
    this: *mut FactoryObj,
    props: *const c_void,
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
        props,
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
    create_geometry_group as *const (),
    create_transformed_geometry as *const (),
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
    /// ID2D1GeometryGroup : figures des sources réunies (#1400).
    Group,
    /// ID2D1TransformedGeometry : figures de la source transformées.
    Transformed,
}

struct Geometry {
    kind: GeomKind,
    path: Mutex<Path>,
    rule: Mutex<FillRule>,
    /// Un puits (sink) est ouvert et la géométrie n'est pas encore figée.
    open: Mutex<bool>,
    /// Géométries sources d'un groupe ou d'une géométrie transformée, tenues.
    sources: Vec<*mut c_void>,
    /// Transformation d'une géométrie transformée.
    transform: Matrix,
}

impl Drop for Geometry {
    fn drop(&mut self) {
        for &g in &self.sources {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(g, 2)(g) };
        }
    }
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
        GeomKind::Group => IID_D2D1_GEOMETRY_GROUP,
        GeomKind::Transformed => IID_D2D1_TRANSFORMED_GEOMETRY,
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

/// Point (x, y) dans des polygones, selon la règle de remplissage.
fn polys_contain(polys: &[Vec<Pt>], rule: FillRule, x: f32, y: f32) -> bool {
    let mut wind = 0i32;
    for poly in polys {
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
    match rule {
        FillRule::EvenOdd => wind % 2 != 0,
        FillRule::NonZero => wind != 0,
    }
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
    *out = i32::from(polys_contain(&polys, rule, x, y));
    S_OK
}

/// Polygones du trait d'épaisseur `width` (jointures et extrémités rondes ;
/// le style de trait n'est pas appliqué), à unir par la règle non nulle.
unsafe fn widened_polys(this: *mut GeometryObj, width: f32, m: *const Matrix) -> Vec<Vec<Pt>> {
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    crate::dlls::lock(&Com::get(this).data.path).stroke_polys(&m, width.max(0.0))
}

/// GetWidenedBounds(largeur, style, transform, tolérance, *rect) (#1470).
unsafe extern "win64" fn geometry_widened_bounds(
    this: *mut GeometryObj,
    width: f32,
    _style: *mut c_void,
    m: *const Matrix,
    _tol: f32,
    out: *mut RectF,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let mut r = RectF {
        l: f32::MAX,
        t: f32::MAX,
        r: f32::MIN,
        b: f32::MIN,
    };
    for p in widened_polys(this, width, m).iter().flatten() {
        r.l = r.l.min(p.0);
        r.t = r.t.min(p.1);
        r.r = r.r.max(p.0);
        r.b = r.b.max(p.1);
    }
    *out = r;
    S_OK
}

/// StrokeContainsPoint(point, largeur, style, transform, tolérance, *BOOL).
unsafe extern "win64" fn geometry_stroke_contains(
    this: *mut GeometryObj,
    pt: u64,
    width: f32,
    _style: *mut c_void,
    m: *const Matrix,
    _tol: f32,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let (x, y) = point(pt);
    let polys = widened_polys(this, width, m);
    *out = i32::from(polys_contain(&polys, FillRule::NonZero, x, y));
    S_OK
}

/// ComputeLength(transform, tolérance, *longueur) : somme des segments
/// des figures aplaties.
unsafe extern "win64" fn geometry_length(
    this: *mut GeometryObj,
    m: *const Matrix,
    _tol: f32,
    out: *mut f32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    let path = crate::dlls::lock(&Com::get(this).data.path).clone();
    let mut len = 0.0;
    for (poly, (_, closed)) in path.fill_polys(&m).iter().zip(&path.figures) {
        let n = if *closed {
            poly.len()
        } else {
            poly.len().saturating_sub(1)
        };
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            len += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        }
    }
    *out = len;
    S_OK
}

/// Widen(largeur, style, transform, tolérance, puits) : le trait de la
/// géométrie, envoyé en figures fermées à unir par la règle non nulle
/// (#1470). Paint.NET en tire les limites de ses éléments dessinés.
unsafe extern "win64" fn geometry_widen(
    this: *mut GeometryObj,
    width: f32,
    _style: *mut c_void,
    m: *const Matrix,
    _tol: f32,
    sink: *mut c_void,
) -> i32 {
    if sink.is_null() {
        return E_INVALIDARG;
    }
    let figures: Vec<(Vec<Pt>, bool)> = widened_polys(this, width, m)
        .into_iter()
        .map(|p| (p, true))
        .collect();
    emit_figures(sink, &figures, FillRule::NonZero)
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

/// ID2D1GeometryGroup {2CD906A6-12E2-11DC-9FED-001143A055F9}.
const IID_D2D1_GEOMETRY_GROUP: Guid = guid(
    0x2cd906a6,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
/// ID2D1TransformedGeometry {2CD906BB-12E2-11DC-9FED-001143A055F9}.
const IID_D2D1_TRANSFORMED_GEOMETRY: Guid = guid(
    0x2cd906bb,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);

/// Envoie des figures aplaties à un ID2D1SimplifiedGeometrySink (ou un
/// ID2D1GeometrySink, qui le prolonge) : SetFillMode, BeginFigure,
/// AddLines, EndFigure, Close.
unsafe fn emit_figures(sink: *mut c_void, figures: &[(Vec<Pt>, bool)], rule: FillRule) -> i32 {
    type SetFill = unsafe extern "win64" fn(*mut c_void, u32);
    type Begin = unsafe extern "win64" fn(*mut c_void, u64, u32);
    type Lines = unsafe extern "win64" fn(*mut c_void, *const Pt, u32);
    type End = unsafe extern "win64" fn(*mut c_void, u32);
    type Close = unsafe extern "win64" fn(*mut c_void) -> i32;
    let packed = |p: Pt| (p.0.to_bits() as u64) | ((p.1.to_bits() as u64) << 32);
    method::<SetFill>(sink, 3)(sink, u32::from(rule == FillRule::NonZero));
    for (pts, closed) in figures {
        let Some(&first) = pts.first() else { continue };
        // D2D1_FIGURE_BEGIN_FILLED 0 ; D2D1_FIGURE_END_CLOSED 1.
        method::<Begin>(sink, 5)(sink, packed(first), 0);
        if pts.len() > 1 {
            method::<Lines>(sink, 6)(sink, pts[1..].as_ptr(), (pts.len() - 1) as u32);
        }
        method::<End>(sink, 8)(sink, u32::from(*closed));
    }
    method::<Close>(sink, 9)(sink)
}

/// Simplify(option, transform, tolérance, sink) : nos figures sont déjà
/// aplaties, elles partent en segments droits quelle que soit l'option.
unsafe extern "win64" fn geometry_simplify(
    this: *mut GeometryObj,
    _option: u32,
    m: *const Matrix,
    _tol: f32,
    sink: *mut c_void,
) -> i32 {
    if sink.is_null() {
        return E_INVALIDARG;
    }
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    let g = &Com::get(this).data;
    let path = crate::dlls::lock(&g.path).clone();
    let rule = *crate::dlls::lock(&g.rule);
    let figures: Vec<(Vec<Pt>, bool)> = path
        .fill_polys(&m)
        .into_iter()
        .zip(path.figures.iter().map(|f| f.1))
        .collect();
    emit_figures(sink, &figures, rule)
}

/// Géométrie de Winlux (chemin, forme simple, groupe ou transformée).
unsafe fn is_geometry(p: *mut c_void) -> bool {
    if p.is_null() {
        return false;
    }
    let vt = *(p as *const *const ());
    vt == GEOMETRY_VT.ptr() || vt == GROUP_VT.ptr() || vt == TRANSFORMED_VT.ptr()
}

/// CombineWithGeometry(entrée, mode, transform de l'entrée, tolérance,
/// puits) (#1549) : union, intersection, ou exclusif ou différence des
/// figures aplaties, envoyée en contours fermés.
unsafe extern "win64" fn geometry_combine(
    this: *mut GeometryObj,
    input: *mut c_void,
    mode: u32,
    m: *const Matrix,
    _tol: f32,
    sink: *mut c_void,
) -> i32 {
    use super::d2d_combine::{combine, CombineMode};
    if sink.is_null() || !is_geometry(input) {
        return E_INVALIDARG;
    }
    let Some(mode) = CombineMode::from_d2d(mode) else {
        return E_INVALIDARG;
    };
    let polys = |g: *mut GeometryObj, m: &Matrix| {
        let g = &Com::get(g).data;
        let path = crate::dlls::lock(&g.path).fill_polys(m);
        (path, *crate::dlls::lock(&g.rule))
    };
    let (a, ra) = polys(this, &Matrix::IDENTITY);
    let m = if m.is_null() { Matrix::IDENTITY } else { *m };
    let (b, rb) = polys(input as *mut GeometryObj, &m);
    let figures: Vec<(Vec<Pt>, bool)> = combine(&a, ra, &b, rb, mode)
        .into_iter()
        .map(|p| (p, true))
        .collect();
    emit_figures(sink, &figures, FillRule::NonZero)
}

/// ID2D1PathGeometry::Stream(sink) : recopie le chemin fermé.
unsafe extern "win64" fn path_stream(this: *mut GeometryObj, sink: *mut c_void) -> i32 {
    if sink.is_null() {
        return E_INVALIDARG;
    }
    let g = &Com::get(this).data;
    if *crate::dlls::lock(&g.open) {
        return D2DERR_WRONG_STATE;
    }
    let path = crate::dlls::lock(&g.path).clone();
    let rule = *crate::dlls::lock(&g.rule);
    emit_figures(sink, &path.figures, rule)
}

unsafe extern "win64" fn group_fill_mode(this: *mut GeometryObj) -> u32 {
    u32::from(*crate::dlls::lock(&Com::get(this).data.rule) == FillRule::NonZero)
}
unsafe extern "win64" fn group_source_count(this: *mut GeometryObj) -> u32 {
    Com::get(this).data.sources.len() as u32
}
unsafe extern "win64" fn group_sources(this: *mut GeometryObj, out: *mut *mut c_void, n: u32) {
    if out.is_null() {
        return;
    }
    let src = &Com::get(this).data.sources;
    for i in 0..n as usize {
        let g = src.get(i).copied().unwrap_or(std::ptr::null_mut());
        if !g.is_null() {
            method::<unsafe extern "win64" fn(*mut c_void) -> u32>(g, 1)(g);
        }
        *out.add(i) = g;
    }
}
unsafe extern "win64" fn transformed_source(this: *mut GeometryObj, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let g = Com::get(this)
        .data
        .sources
        .first()
        .copied()
        .unwrap_or(std::ptr::null_mut());
    if !g.is_null() {
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(g, 1)(g);
    }
    *out = g;
}
unsafe extern "win64" fn transformed_matrix(this: *mut GeometryObj, out: *mut Matrix) {
    if !out.is_null() {
        *out = Com::get(this).data.transform;
    }
}

/// Les 17 méthodes d'ID2D1Geometry, suivies de celles de l'interface fille.
macro_rules! geometry_vtable {
    ($name:ident, $n:literal, [$($extra:expr),*]) => {
        static $name: VTable<$n> = VTable([
            geometry_qi as *const (),
            add_ref::<Res<Geometry>> as *const (),
            release::<Res<Geometry>> as *const (),
            get_factory::<Geometry> as *const (),
            geometry_bounds as *const (),
            geometry_widened_bounds as *const (),
            geometry_stroke_contains as *const (),
            geometry_fill_contains as *const (),
            unsupported!("CompareWithGeometry"),
            geometry_simplify as *const (),
            unsupported!("Tessellate"),
            geometry_combine as *const (),
            unsupported!("Outline"),
            unsupported!("ComputeArea"),
            geometry_length as *const (),
            unsupported!("ComputePointAtLength"),
            geometry_widen as *const (),
            $($extra),*
        ]);
    };
}
geometry_vtable!(
    GROUP_VT,
    20,
    [
        group_fill_mode as *const (),
        group_source_count as *const (),
        group_sources as *const ()
    ]
);
geometry_vtable!(
    TRANSFORMED_VT,
    19,
    [
        transformed_source as *const (),
        transformed_matrix as *const ()
    ]
);

/// Table commune : ID2D1Geometry (17 entrées) puis, pour un chemin,
/// Open, Stream, GetSegmentCount et GetFigureCount.
static GEOMETRY_VT: VTable<22> = VTable([
    geometry_qi as *const (),
    add_ref::<Res<Geometry>> as *const (),
    release::<Res<Geometry>> as *const (),
    get_factory::<Geometry> as *const (),
    geometry_bounds as *const (),
    geometry_widened_bounds as *const (),
    geometry_stroke_contains as *const (),
    geometry_fill_contains as *const (),
    unsupported!("CompareWithGeometry"),
    geometry_simplify as *const (),
    unsupported!("Tessellate"),
    geometry_combine as *const (),
    unsupported!("Outline"),
    unsupported!("ComputeArea"),
    geometry_length as *const (),
    unsupported!("ComputePointAtLength"),
    geometry_widen as *const (),
    path_open as *const (), // ou GetRect / GetRoundedRect / GetEllipse
    path_stream as *const (),
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
    /// D2D1_BRUSH_PROPERTIES::transform (SetTransform).
    transform: Mutex<Matrix>,
    /// Pinceau à motif, dégradé ou image ; `Solid` pour le pinceau uni.
    kind: Mutex<brushes::Kind>,
}
type BrushObj = Com<Res<Brush>>;

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

static BRUSH_VT: VTable<10> = VTable([
    brush_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    brushes::set_transform as *const (),
    brush_get_opacity as *const (),
    brushes::get_transform as *const (),
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
    /// DXGI_FORMAT_A8_UNORM : la mémoire de l'appelant n'a qu'un octet
    /// d'alpha par pixel (#1471).
    alpha_only: bool,
    /// Mémoire d'un IWICBitmapLock partagée (CreateSharedBitmap) : relue à
    /// chaque usage, comme le partage réel de Direct2D (#1609).
    shared: Option<SharedLock>,
    /// Liste de commandes : boîte de ce qui y a été dessiné, même hors de
    /// sa surface (#1743). GetImageLocalBounds la rend, comme Direct2D.
    drawn: Mutex<Option<[f32; 4]>>,
}

/// Verrou WIC dont un bitmap partage les pixels.
struct SharedLock {
    /// IWICBitmapLock tenu jusqu'à la fin du bitmap.
    lock: *mut c_void,
    ptr: *const u8,
    stride: usize,
    straight: bool,
    ignore: bool,
    wide: Option<super::wic_pixels::Layout>,
    /// GUID_WICPixelFormat8bppAlpha : un octet d'alpha par pixel (#1849).
    alpha8: bool,
}

impl Bitmap {
    fn new(w: u32, h: u32, px: Vec<u8>) -> Bitmap {
        Bitmap {
            w,
            h,
            px: Mutex::new(px),
            options: 0,
            surface: std::ptr::null_mut(),
            alpha_only: false,
            shared: None,
            drawn: Mutex::new(None),
        }
    }

    /// Bitmap partagé : recopie la mémoire actuelle du verrou WIC. Paint.NET
    /// remplit le tampon de son tampon de pinceau après CreateSharedBitmap.
    unsafe fn refresh(&self) {
        if let Some(s) = &self.shared {
            let px = lock_pixels(
                s.ptr,
                s.stride,
                self.w as usize,
                self.h as usize,
                s.straight,
                s.ignore,
                s.wide,
                s.alpha8,
            );
            *crate::dlls::lock(&self.px) = px;
        }
    }
}

/// Pixels d'un verrou WIC (BGRA prémultiplié ou non, BGR, formats larges)
/// en BGRA prémultiplié.
unsafe fn lock_pixels(
    ptr: *const u8,
    stride: usize,
    w: usize,
    h: usize,
    straight: bool,
    ignore: bool,
    wide: Option<super::wic_pixels::Layout>,
    alpha8: bool,
) -> Vec<u8> {
    if alpha8 {
        // Masque d'alpha : noir prémultiplié de cette opacité, comme un
        // bitmap DXGI_FORMAT_A8_UNORM.
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            let line = std::slice::from_raw_parts(ptr.add(y * stride), w);
            for (x, &a) in line.iter().enumerate() {
                px[(y * w + x) * 4 + 3] = a;
            }
        }
        return px;
    }
    let src_bpp = wide.map_or(4, |l| l.bpp as usize);
    let src_row = w * src_bpp;
    if let Some(l) = wide {
        let src = std::slice::from_raw_parts(ptr, stride * (h - 1) + src_row);
        return wide_to_pbgra(&l, src, w, h, stride, ignore);
    }
    let row = w * 4;
    let mut px = vec![0u8; row * h];
    for y in 0..h {
        std::ptr::copy_nonoverlapping(ptr.add(y * stride), px[y * row..].as_mut_ptr(), row);
    }
    for p in px.as_chunks_mut::<4>().0 {
        if ignore {
            p[3] = 255;
        } else if straight {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    px
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        if let Some(s) = &self.shared {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(s.lock, 2)(s.lock) };
        }
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
    let hr = copy_from_memory(this, dst, src, pitch);
    d2d_trace!(
        "CopyFromMemory {:p} {:?} → {hr:#x} {}",
        this,
        dst.as_ref().map(|r| (r.l, r.t, r.r, r.b)),
        v11::describe_image(this.cast())
    );
    hr
}

unsafe fn copy_from_memory(
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
    let cols = (r.r - r.l) as usize;
    for y in 0..(r.b - r.t) as usize {
        let d = ((r.t as usize + y) * b.w as usize + r.l as usize) * 4;
        let line = src.add(y * pitch as usize);
        if b.alpha_only {
            // Un octet d'alpha par pixel : noir prémultiplié de cette opacité.
            for x in 0..cols {
                px[d + x * 4..d + x * 4 + 4].copy_from_slice(&[0, 0, 0, *line.add(x)]);
            }
        } else {
            std::ptr::copy_nonoverlapping(line, px[d..].as_mut_ptr(), cols * 4);
        }
    }
    S_OK
}

/// Bitmaps dans lesquels une cible dessine (SetTarget, cible compatible),
/// avec cette cible : CopyFromBitmap lit les pixels en cours de tracé
/// (#1749), que Direct2D aurait vidés avant la copie.
fn drawing_into() -> &'static Mutex<std::collections::HashMap<usize, usize>> {
    static M: OnceLock<Mutex<std::collections::HashMap<usize, usize>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

pub(super) fn register_drawing(bitmap: *mut c_void, target: *mut c_void) {
    if !bitmap.is_null() {
        crate::dlls::lock(drawing_into()).insert(bitmap as usize, target as usize);
    }
}

pub(super) fn unregister_drawing(bitmap: *mut c_void) {
    crate::dlls::lock(drawing_into()).remove(&(bitmap as usize));
}

/// Pixels BGRA prémultipliés à jour d'un bitmap, avec sa taille.
unsafe fn current_pixels(bitmap: *mut c_void) -> (u32, u32, Vec<u8>) {
    let owner = crate::dlls::lock(drawing_into())
        .get(&(bitmap as usize))
        .copied();
    if let Some(owner) = owner {
        let t = target(owner as *mut TargetObj);
        if matches!(t.out, Output::Bitmap(b) if b == bitmap) && t.drawing {
            return (t.w, t.h, t.px.clone());
        }
    }
    let b = &Com::get(bitmap as *mut BitmapObj).data;
    b.refresh();
    let px = crate::dlls::lock(&b.px).clone();
    (b.w, b.h, px)
}

/// Copie la zone `src` (pixels w×h) vers `dst` au point `at`.
unsafe fn copy_pixels_into(
    dst: *mut BitmapObj,
    at: *const [u32; 2],
    (w, h, px): (u32, u32, Vec<u8>),
    src: *const RectU,
) -> i32 {
    let r = src.as_ref().copied().unwrap_or(RectU {
        l: 0,
        t: 0,
        r: w,
        b: h,
    });
    let (x, y) = at.as_ref().map_or((0, 0), |p| (p[0], p[1]));
    let d = &Com::get(dst).data;
    let (cw, ch) = (r.r.saturating_sub(r.l), r.b.saturating_sub(r.t));
    if r.l > r.r || r.t > r.b || r.r > w || r.b > h || x + cw > d.w || y + ch > d.h {
        return E_INVALIDARG;
    }
    if px.len() < w as usize * h as usize * 4 {
        return E_INVALIDARG;
    }
    let mut out = crate::dlls::lock(&d.px);
    let row = cw as usize * 4;
    for j in 0..ch as usize {
        let s = ((r.t as usize + j) * w as usize + r.l as usize) * 4;
        let o = ((y as usize + j) * d.w as usize + x as usize) * 4;
        out[o..o + row].copy_from_slice(&px[s..s + row]);
    }
    S_OK
}

/// ID2D1Bitmap::CopyFromBitmap (#1749) : Paint.NET recopie la cible de
/// ses effets GPU dans un bitmap lisible par le processeur.
unsafe extern "win64" fn bitmap_copy_from_bitmap(
    this: *mut BitmapObj,
    at: *const [u32; 2],
    bitmap: *mut c_void,
    src: *const RectU,
) -> i32 {
    if bitmap.is_null() || !v11::is_pixel_image(bitmap) {
        return E_INVALIDARG;
    }
    let pixels = current_pixels(bitmap);
    let hr = copy_pixels_into(this, at, pixels, src);
    d2d_trace!(
        "CopyFromBitmap {:p} ← {} @{:?} src {:?} → {hr:#x}",
        this,
        v11::describe_image(bitmap),
        at.as_ref(),
        src.as_ref().map(|r| (r.l, r.t, r.r, r.b))
    );
    hr
}

/// ID2D1Bitmap::CopyFromRenderTarget : pixels courants de la cible.
unsafe extern "win64" fn bitmap_copy_from_target(
    this: *mut BitmapObj,
    at: *const [u32; 2],
    rt: *mut c_void,
    src: *const RectU,
) -> i32 {
    if rt.is_null() {
        return E_INVALIDARG;
    }
    let vt = *(rt as *const *const ());
    let known = [
        TARGET_VT.ptr(),
        HWND_TARGET_VT.ptr(),
        DC_TARGET_VT.ptr(),
        BITMAP_TARGET_VT.ptr(),
        v11::DC_VT.ptr(),
    ];
    if !known.contains(&vt) {
        return E_INVALIDARG;
    }
    let pixels = {
        let t = target(rt as *mut TargetObj);
        (t.w, t.h, t.px.clone())
    };
    let hr = copy_pixels_into(this, at, pixels, src);
    d2d_trace!("CopyFromRenderTarget {:p} ← {:p} → {hr:#x}", this, rt);
    hr
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
    bitmap_copy_from_bitmap as *const (),
    bitmap_copy_from_target as *const (),
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
    /// D2D1_ANTIALIAS_MODE : 0 par primitive, 1 crénelé.
    antialias: u32,
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
        if let Output::Bitmap(b) = self.out {
            unregister_drawing(b);
        }
        if let Output::Wic(obj) | Output::Bitmap(obj) = self.out {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(obj, 2)(obj) };
        }
    }
}

type TargetObj = Com<Res<Mutex<Target>>>;

impl Target {
    /// Transformation de l'application (en DIP), celle de GetTransform.
    fn world(&self) -> Matrix {
        self.state.transform.unwrap_or(Matrix::IDENTITY)
    }
    /// Des DIP aux pixels de la cible : la transformation de l'application
    /// puis l'échelle DPI / 96 (#1743). Paint.NET dessine les empreintes du
    /// pinceau en DIP sur des tuiles de masque à plus haute résolution ;
    /// sans l'échelle, elles couvraient un tiers de pixel et le trait noir
    /// sortait gris clair.
    fn transform(&self) -> Matrix {
        let (sx, sy) = self.ext.dip_scale();
        let world = self.world();
        if (sx, sy) == (1.0, 1.0) {
            return world;
        }
        world.then(&Matrix {
            m11: sx,
            m12: 0.0,
            m21: 0.0,
            m22: sy,
            dx: 0.0,
            dy: 0.0,
        })
    }
    fn fill_mode(&self) -> FillMode {
        FillMode {
            aliased: self.state.antialias == 1,
            blend: self.ext.primitive_blend(),
        }
    }
    fn clip(&self) -> Option<Clip> {
        self.state.clips.last().copied()
    }
    fn fill(&mut self, polys: &[Vec<Pt>], rule: FillRule, paint: &dyn Fn(f32, f32) -> Color) {
        let clip = self.clip();
        if let Output::Bitmap(list) = self.out {
            unsafe { v11::record_list_bounds(list, polys, clip) };
        }
        let mode = self.fill_mode();
        let (w, h) = (self.w, self.h);
        let mut c = Canvas {
            w,
            h,
            stride: w as usize * 4,
            px: &mut self.px,
        };
        c.fill_mode(polys, rule, clip, paint, mode);
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
                    d2d_trace!(
                        "présentation hdc {:#x} @{},{} {}x{} → {}",
                        hdc,
                        x,
                        y,
                        self.w,
                        self.h,
                        super::gui::describe_dc(hdc)
                    );
                    let lignes = self.present(hdc, x, y);
                    d2d_trace!("présentation hdc {:#x} : {} lignes", hdc, lignes);
                } else if hdc != 0 {
                    self.load_dc(hdc, x, y);
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
        type GetFormat = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
        let mut fmt = Guid::default();
        method::<GetFormat>(wic, 4)(wic, &mut fmt);
        let layout = WicLayout::of(&fmt).unwrap_or(WicLayout::Pbgra);
        if layout != WicLayout::Pbgra && !data.is_null() {
            // Formats convertis ligne par ligne (#1573).
            let row = self.w as usize * 4;
            let wrow = self.w as usize * layout.bpp();
            let (stride, h) = (stride as usize, self.h as usize);
            if h > 0 && stride >= wrow && (size as usize) >= stride * (h - 1) + wrow {
                for y in 0..h {
                    let wic = std::slice::from_raw_parts_mut(data.add(y * stride), wrow);
                    let own = &mut self.px[y * row..][..row];
                    if to_wic {
                        layout.write(own, wic);
                    } else {
                        layout.read(wic, own);
                    }
                }
            }
            method::<unsafe extern "win64" fn(*mut c_void) -> u32>(lock, 2)(lock);
            return S_OK;
        }
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
    /// BeginDraw d'une cible de DC : elle part du contenu actuel du DC, et
    /// ce qui n'est pas dessiné garde le fond posé par GDI (#1368). Sans
    /// cela, la zone non peinte d'un bouton de Paint.NET devenait noire.
    fn load_dc(&mut self, hdc: usize, x: i32, y: i32) {
        if self.w == 0 || self.h == 0 {
            return;
        }
        let rect = super::gui::Rect {
            left: x,
            top: y,
            right: x + self.w as i32,
            bottom: y + self.h as i32,
        };
        let snap = match super::gui::dc_window(hdc) {
            Some(hwnd) if hwnd != 0 => super::gui::capture_window(hwnd, rect),
            _ => super::gui::bitmap_snapshot(hdc, rect),
        };
        let Some(snap) = snap else { return };
        if snap.width != self.w as usize || snap.height != self.h as usize {
            return;
        }
        for (p, &c) in self.px.as_chunks_mut::<4>().0.iter_mut().zip(&snap.pixels) {
            // COLORREF 0x00BBGGRR → BGRA opaque.
            *p = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
        }
    }

    unsafe fn present(&self, hdc: usize, x: i32, y: i32) -> i32 {
        if self.w == 0 || self.h == 0 {
            return 0;
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
        )
    }
}

unsafe fn target<'a>(this: *mut TargetObj) -> std::sync::MutexGuard<'a, Target> {
    crate::dlls::lock(&Com::get(this).data)
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
    if !riid.is_null() && v11::context_iids().contains(&*riid) {
        return v11::context_view(this, out);
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
    props: *const u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let (w, h) = (size as u32, (size >> 32) as u32);
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let mut b = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    // D2D1_BITMAP_PROPERTIES commence par le format DXGI.
    b.alpha_only = !props.is_null() && props.read_unaligned() == DXGI_FORMAT_A8_UNORM;
    let obj = new_res(Com::get(this).factory, BITMAP_VT.ptr(), b);
    if !src.is_null() {
        bitmap_copy_from_memory(obj, std::ptr::null(), src, pitch);
    }
    d2d_trace!("CreateBitmap {:p} → {:p} {w}x{h}", this, obj);
    put(out, obj)
}

/// IWICBitmapLock {00000123-A8F2-4877-BA0A-FD2B6645FB94} (wincodec.h).
const IID_WIC_BITMAP_LOCK: Guid = guid(
    0x0000_0123,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);

/// CreateSharedBitmap(riid, data, props, out) (#1399). Un ID2D1Bitmap de
/// Winlux est rendu tel quel ; un IWICBitmapLock (Paint.NET :
/// Direct2DControlImpl.DrawBackground) donne un bitmap aux pixels du
/// verrou, lus à la création (BGRA, prémultiplié ou non, ou BGR).
unsafe extern "win64" fn target_shared_bitmap(
    this: *mut TargetObj,
    riid: *const Guid,
    data: *mut c_void,
    props: *const [u32; 2],
    out: *mut *mut c_void,
) -> i32 {
    let hr = shared_bitmap(this, riid, data, props, out);
    if !out.is_null() {
        d2d_trace!(
            "CreateSharedBitmap {:p} {:?} → {hr:#x} {}",
            this,
            riid.as_ref(),
            v11::describe_image(*out)
        );
    }
    hr
}

unsafe fn shared_bitmap(
    this: *mut TargetObj,
    riid: *const Guid,
    data: *mut c_void,
    props: *const [u32; 2],
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if riid.is_null() || data.is_null() {
        return E_INVALIDARG;
    }
    if *riid == IID_D2D1_BITMAP || *riid == v11::IID_D2D1_BITMAP1 {
        if *(data as *const *const ()) != BITMAP_VT.ptr() {
            return E_INVALIDARG;
        }
        Com::add_ref(data as *mut BitmapObj);
        *out = data;
        return S_OK;
    }
    if *riid != IID_WIC_BITMAP_LOCK {
        crate::log_warn!(
            "d2d1",
            "CreateSharedBitmap : interface {:?} non prise en charge",
            *riid
        );
        return E_INVALIDARG;
    }
    type GetSize = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut u32) -> i32;
    type GetU32 = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
    type GetData = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
    type GetFormat = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
    let (mut w, mut h, mut stride, mut size) = (0u32, 0u32, 0u32, 0u32);
    let mut ptr = std::ptr::null_mut();
    let mut fmt = Guid::default();
    for hr in [
        method::<GetSize>(data, 3)(data, &mut w, &mut h),
        method::<GetU32>(data, 4)(data, &mut stride),
        method::<GetData>(data, 5)(data, &mut size, &mut ptr),
        method::<GetFormat>(data, 6)(data, &mut fmt),
    ] {
        if hr < 0 {
            return hr;
        }
    }
    use super::windowscodecs::{FMT_ALPHA8, FMT_BGR32, FMT_BGRA32, FMT_PBGRA32};
    let straight = fmt == FMT_BGRA32;
    // Masque de sélection de Paint.NET (#1849).
    let alpha8 = fmt == FMT_ALPHA8;
    // D2D1_ALPHA_MODE_IGNORE (3) : alpha ignoré, pixels opaques.
    let ignore = fmt == FMT_BGR32 || props.as_ref().is_some_and(|p| p[1] == 3);
    // Formats larges (#1520) : 16 bits, demi-flottants, flottants.
    let wide = super::wic_pixels::layout(&fmt);
    if fmt != FMT_PBGRA32 && !straight && fmt != FMT_BGR32 && !alpha8 && wide.is_none() {
        crate::log_warn!(
            "d2d1",
            "CreateSharedBitmap : format de verrou {:?} non pris en charge",
            fmt
        );
        return D2DERR_UNSUPPORTED_PIXEL_FORMAT;
    }
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let src_bpp = if alpha8 {
        1
    } else {
        wide.map_or(4, |l| l.bpp as usize)
    };
    let src_row = w as usize * src_bpp;
    if ptr.is_null()
        || (stride as usize) < src_row
        || (size as usize) < stride as usize * (h as usize - 1) + src_row
    {
        return E_INVALIDARG;
    }
    let px = lock_pixels(
        ptr,
        stride as usize,
        w as usize,
        h as usize,
        straight,
        ignore,
        wide,
        alpha8,
    );
    let mut b = Bitmap::new(w, h, px);
    b.alpha_only = alpha8;
    // Le bitmap partage la mémoire du verrou (#1609) : il le tient.
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(data, 1)(data);
    b.shared = Some(SharedLock {
        lock: data,
        ptr,
        stride: stride as usize,
        straight,
        ignore,
        wide,
        alpha8,
    });
    put(out, new_res(Com::get(this).factory, BITMAP_VT.ptr(), b))
}

/// Pixels d'un format WIC large (16 bits, demi-flottants, flottants) en
/// BGRA 8 bits prémultiplié. Les formats flottants portent des valeurs
/// linéaires (scRGB) : elles sont encodées sRGB pour la cible 8 bits.
fn wide_to_pbgra(
    l: &super::wic_pixels::Layout,
    src: &[u8],
    w: usize,
    h: usize,
    stride: usize,
    ignore: bool,
) -> Vec<u8> {
    use super::wic_pixels::{linear_to_srgb, read};
    let bpp = l.bpp as usize;
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut c = read(l, &src[y * stride + x * bpp..][..bpp]);
            if l.linear() {
                for v in &mut c[..3] {
                    *v = linear_to_srgb(*v);
                }
            }
            let a = if ignore { 1.0 } else { c[3].clamp(0.0, 1.0) };
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let d = &mut px[(y * w + x) * 4..][..4];
            d.copy_from_slice(&[q(c[2] * a), q(c[1] * a), q(c[0] * a), q(a)]);
        }
    }
    px
}

/// CreateBitmapFromWicBitmap : lit la source WIC convertie en BGRA prémultiplié.
unsafe extern "win64" fn target_bitmap_from_wic(
    this: *mut TargetObj,
    src: *mut c_void,
    props: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    let hr = bitmap_from_wic(this, src, props, out);
    if !out.is_null() {
        d2d_trace!(
            "CreateBitmapFromWicBitmap {:p} {:p} → {hr:#x} {}",
            this,
            src,
            v11::describe_image(*out)
        );
    }
    hr
}

unsafe fn bitmap_from_wic(
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
    props: *const brushes::BrushProps,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || c.is_null() {
        return E_INVALIDARG;
    }
    let b = brushes::new_brush(*c, props, brushes::Kind::Solid);
    put(out, new_res(Com::get(this).factory, BRUSH_VT.ptr(), b))
}

/// La peinture est préparée avant de verrouiller la cible : un pinceau
/// peut lire un bitmap ou calculer un effet.
unsafe fn fill_path(this: *mut TargetObj, path: &Path, rule: FillRule, brush: *mut c_void) {
    let world = target(this).transform();
    let paint = brushes::paint(brush, &world);
    trace_fill(this, "Remplissage", path, brush, paint.is_some());
    let Some(paint) = paint else {
        return;
    };
    let polys = path.fill_polys(&world);
    target(this).fill(&polys, rule, &*paint);
}

unsafe fn stroke_path(
    this: *mut TargetObj,
    path: &Path,
    brush: *mut c_void,
    width: f32,
    style: *mut c_void,
) {
    let world = target(this).transform();
    let paint = brushes::paint(brush, &world);
    trace_fill(this, "Trait", path, brush, paint.is_some());
    let Some(paint) = paint else {
        return;
    };
    // Tirets du style de trait (#1867), en multiples de l'épaisseur. Un
    // trait « fixe » (1) ou « fin » (2) garde son épaisseur en pixels.
    let original = path;
    let dashed;
    let (path, width) = match v11::stroke_pattern(style) {
        Some((pattern, offset, kind)) => {
            let scale = world.scale();
            let w = match kind {
                1 => width / scale,
                2 => 1.0 / scale,
                _ => width,
            };
            let unit = w.max(1e-3);
            let pattern: Vec<f32> = pattern.iter().map(|d| d * unit).collect();
            dashed = path.dashed(&pattern, offset * unit);
            (&dashed, w)
        }
        None => match v11::stroke_pattern_kind(style) {
            1 => (path, width / world.scale()),
            2 => (path, 1.0 / world.scale()),
            _ => (path, width),
        },
    };
    let polys = path.stroke_polys(&world, width);
    if v11::trace_on() {
        trace_stroke(original, path.figures.len(), &polys, width, style);
    }
    target(this).fill(&polys, FillRule::NonZero, &*paint);
}

/// WINLUX_D2D_TRACE (#2431) : détail d'un trait. Figures (points, fermée),
/// épaisseur, motif de tirets, et boîte des polygones en pixels de la
/// cible, pour voir quel côté d'un contour se perd.
unsafe fn trace_stroke(
    path: &Path,
    pieces: usize,
    polys: &[Vec<Pt>],
    width: f32,
    style: *mut c_void,
) {
    let figures: Vec<String> = path
        .figures
        .iter()
        .take(4)
        .map(|(pts, closed)| {
            let pts: Vec<String> = pts
                .iter()
                .take(6)
                .map(|p| format!("({},{})", p.0, p.1))
                .collect();
            format!("{}{}", pts.join(" "), if *closed { " fermée" } else { "" })
        })
        .collect();
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in polys.iter().flatten() {
        b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
    }
    d2d_trace!(
        "  trait : {} figures [{}] épaisseur {} style {} motif {:?} → {} morceaux, {} polygones boîte {:?}",
        path.figures.len(),
        figures.join(" | "),
        width,
        v11::stroke_pattern_kind(style),
        v11::stroke_pattern(style).map(|(m, o, _)| (m, o)),
        pieces,
        polys.len(),
        b
    );
}

/// WINLUX_D2D_TRACE (#1743) : transformation, DPI, anticrénelage et
/// mélange courants quand ils ne sont pas ceux par défaut.
unsafe fn describe_state(this: *mut TargetObj) -> String {
    if !v11::trace_on() {
        return String::new();
    }
    let t = target(this);
    t.ext.describe_state(&t.world(), t.state.antialias)
}

/// WINLUX_D2D_TRACE (#1669) : forme, boîte et pinceau d'un remplissage,
/// et si le pinceau a pu être préparé (sinon rien n'est dessiné).
unsafe fn trace_fill(this: *mut TargetObj, what: &str, path: &Path, brush: *mut c_void, ok: bool) {
    if !v11::trace_on() {
        return;
    }
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (pts, _) in &path.figures {
        for p in pts {
            b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
        }
    }
    d2d_trace!(
        "{} {:p} {} figures boîte {:?} pinceau {}{}{}",
        what,
        this,
        path.figures.len(),
        b,
        brushes::describe(brush),
        describe_state(this),
        if ok {
            ""
        } else {
            " → rien (pinceau non préparé)"
        }
    );
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
    style: *mut c_void,
) {
    let mut p = Path::default();
    p.begin(point(a));
    p.line(point(b));
    stroke_path(this, &p, brush, width, style);
}
unsafe extern "win64" fn draw_rect(
    this: *mut TargetObj,
    r: *const RectF,
    brush: *mut c_void,
    width: f32,
    style: *mut c_void,
) {
    if let Some(r) = r.as_ref() {
        stroke_path(this, &Path::rect(r.l, r.t, r.r, r.b), brush, width, style);
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
    s: *mut c_void,
) {
    if let Some(r) = r.as_ref() {
        let p = Path::rounded_rect(r.rect.l, r.rect.t, r.rect.r, r.rect.b, r.rx, r.ry, 4.0);
        stroke_path(this, &p, brush, width, s);
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
    s: *mut c_void,
) {
    if let Some(e) = e.as_ref() {
        stroke_path(
            this,
            &Path::ellipse(e.x, e.y, e.rx, e.ry, 4.0),
            brush,
            width,
            s,
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
    if g.is_null() {
        return None;
    }
    let vt = *(g as *const *const ());
    if vt != GEOMETRY_VT.ptr() && vt != GROUP_VT.ptr() && vt != TRANSFORMED_VT.ptr() {
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
    s: *mut c_void,
) {
    if let Some((p, _)) = geometry_of(g) {
        stroke_path(this, &p, brush, width, s);
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
    d2d_trace!(
        "DrawBitmap {:p} ← {} dst {:?} src {:?} opacité {opacity}{}",
        this,
        v11::describe_image(bitmap),
        dst.as_ref().map(|r| (r.l, r.t, r.r, r.b)),
        src.as_ref().map(|r| (r.l, r.t, r.r, r.b)),
        describe_state(this)
    );
    if bitmap.is_null() || *(bitmap as *const *const ()) != BITMAP_VT.ptr() {
        return;
    }
    let b = &Com::get(bitmap as *mut BitmapObj).data;
    b.refresh();
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
        *m = target(this).world();
    }
}
unsafe extern "win64" fn set_antialias(this: *mut TargetObj, mode: u32) {
    target(this).state.antialias = mode;
}
unsafe extern "win64" fn get_antialias(this: *mut TargetObj) -> u32 {
    target(this).state.antialias
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
    d2d_trace!("Clear {:p} ({}, {}, {}, {})", this, c.r, c.g, c.b, c.a);
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
    d2d_trace!("BeginDraw {:p} {}", this, v11::describe_target(&t));
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
    let hr = t.sync_wic(true);
    v11::end_draw_home(&mut t);
    d2d_trace!("EndDraw {:p} {} → {hr:#x}", this, v11::describe_target(&t));
    hr
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
    target_shared_bitmap as *const (),
    brushes::create_bitmap_brush as *const (),
    target_solid_brush as *const (),
    v11::gradient_stops as *const (),
    brushes::create_linear_brush as *const (),
    brushes::create_radial_brush as *const (),
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
    set_antialias as *const (),
    get_antialias as *const (),
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
    // Taille en DIP au DPI de la cible parente, ou en pixels ; les deux
    // ensemble fixent le DPI (#1743).
    let (parent_px, parent_dpi) = {
        let t = target(this);
        ((t.w, t.h), t.ext.dpi())
    };
    let size = size.as_ref().filter(|s| s[0] > 0.0 && s[1] > 0.0);
    let ((w, h), dpi) = match (pixel_size.as_ref(), size) {
        (Some(&[w, h]), Some(&[sw, sh])) => ((w, h), (w as f32 * 96.0 / sw, h as f32 * 96.0 / sh)),
        (Some(&[w, h]), None) => ((w, h), parent_dpi),
        (None, Some(&[sw, sh])) => (
            (
                (sw * parent_dpi.0 / 96.0).ceil() as u32,
                (sh * parent_dpi.1 / 96.0).ceil() as u32,
            ),
            parent_dpi,
        ),
        (None, None) => (parent_px, parent_dpi),
    };
    let (w, h) = (w.max(1), h.max(1));
    if crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    let factory = Com::get(this).factory;
    let bitmap = Bitmap::new(w, h, vec![0; w as usize * h as usize * 4]);
    let bitmap = new_res(factory, BITMAP_VT.ptr(), bitmap) as *mut c_void;
    let mut target = Target {
        w,
        h,
        px: vec![0; w as usize * h as usize * 4],
        opaque: false,
        out: Output::Bitmap(bitmap),
        state: TargetState::default(),
        drawing: false,
        ext: Default::default(),
    };
    target.ext.set_dpi(dpi.0, dpi.1);
    let hr = put(
        out,
        new_res(factory, BITMAP_TARGET_VT.ptr(), Mutex::new(target)),
    );
    register_drawing(bitmap, *out);
    hr
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
    d2d_trace!("Resize {:p} {w}x{h}", this);
    let mut t = target(this);
    let left = v11::leave_bitmap(&mut t);
    t.resize(w, h);
    drop(t);
    if !left.is_null() {
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(left, 2)(left);
    }
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
    d2d_trace!("BindDC {:p} hdc {hdc:#x} ({l}, {t}, {r}, {b})", this);
    let mut tg = target(this);
    let left = v11::leave_bitmap(&mut tg);
    tg.resize((r - l).max(0) as u32, (b - t).max(0) as u32);
    tg.out = Output::Dc { hdc, x: l, y: t };
    drop(tg);
    if !left.is_null() {
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(left, 2)(left);
    }
    S_OK
}

// --- Pour la composition WinRT ---------------------------------------------------------

/// Pixels BGRA prémultipliés d'une ID2D1Bitmap de Winlux (surfaces de
/// dessin de Windows.UI.Composition, #1416).
pub(crate) unsafe fn bitmap_pixels(b: *mut c_void) -> Option<(u32, u32, Vec<u8>)> {
    if b.is_null() || *(b as *const *const ()) != BITMAP_VT.ptr() {
        return None;
    }
    let bm = &Com::get(b as *mut BitmapObj).data;
    Some((bm.w, bm.h, crate::dlls::lock(&bm.px).clone()))
}

/// ID2D1Device à partir de l'appareil de rendu donné à la composition :
/// l'appareil Direct2D lui-même, ou un appareil D3D/DXGI pour lequel une
/// fabrique 1.1 en crée un. Pointeur tenu (à libérer) ou nul.
pub(crate) unsafe fn device_for(render: *mut c_void) -> *mut c_void {
    if render.is_null() {
        return std::ptr::null_mut();
    }
    let mut dev = std::ptr::null_mut();
    let qi: unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
        method(render, 0);
    if qi(render, &v11::device_iid(), &mut dev) >= 0 && !dev.is_null() {
        return dev;
    }
    let mut f = std::ptr::null_mut();
    if d2d1_create_factory(0, &v11::factory1_iid(), std::ptr::null(), &mut f) < 0 {
        return std::ptr::null_mut();
    }
    let create: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32 =
        method(f, 17);
    let mut dev = std::ptr::null_mut();
    if create(f, render, &mut dev) < 0 {
        dev = std::ptr::null_mut();
    }
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(f, 2)(f);
    dev
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
    pub(super) unsafe fn setup() -> (*mut c_void, *mut c_void, *mut c_void) {
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

    /// Une cible 1.0 répond à ID2D1DeviceContext6 par une vue qui dessine
    /// sur elle et garde son identité COM (#1085).
    #[test]
    fn old_target_answers_device_context6_with_a_view_on_itself() {
        unsafe {
            let (f, bmp, rt) = setup();
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let dc6 = v11::context_iids()[6];
            assert_eq!(dc6.data1, 0x985f_7e37);
            let mut view = std::ptr::null_mut();
            assert_eq!(method::<Qi>(rt, 0)(rt, &dc6, &mut view), S_OK);
            assert!(!view.is_null() && view != rt);
            let mut again = std::ptr::null_mut();
            assert_eq!(method::<Qi>(view, 0)(view, &dc6, &mut again), S_OK);
            assert_eq!(again, view);
            release_obj(again);
            let mut unk = std::ptr::null_mut();
            assert_eq!(method::<Qi>(view, 0)(view, &IID_IUNKNOWN, &mut unk), S_OK);
            assert_eq!(unk, rt);
            release_obj(unk);
            // AddRef et Release de la vue comptent sur la cible.
            assert_eq!(method::<Unary>(view, 1)(view), 3);
            assert_eq!(method::<Unary>(rt, 2)(rt), 2);
            // Méthodes propres au contexte (73 GetDevice, 75 GetTarget).
            // #1554 : GetDevice rend toujours le même appareil, de la même
            // fabrique que la cible (GetFactory, case 3).
            type Get = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void);
            let mut d1 = std::ptr::null_mut();
            method::<Get>(view, 73)(view, &mut d1);
            assert!(!d1.is_null());
            let mut d2 = std::ptr::null_mut();
            method::<Get>(view, 73)(view, &mut d2);
            assert_eq!(d1, d2);
            let (mut fd, mut fv) = (std::ptr::null_mut(), std::ptr::null_mut());
            method::<Get>(d1, 3)(d1, &mut fd);
            method::<Get>(view, 3)(view, &mut fv);
            assert!(!fd.is_null());
            assert_eq!(fd, fv);
            release_obj(fd);
            release_obj(fv);
            type GetDxgi = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            let mut dx = 1usize as *mut c_void;
            assert!(method::<GetDxgi>(d1, 14)(d1, &mut dx) < 0);
            assert!(dx.is_null());
            release_obj(d2);
            release_obj(d1);
            // Une cible WIC a toujours une cible : le bitmap qui la
            // représente (#1669).
            let mut p = std::ptr::null_mut();
            method::<Get>(view, 75)(view, &mut p);
            assert!(!p.is_null());
            release_obj(p);
            // Dessin par la vue : il arrive dans le bitmap WIC de la cible.
            let red = ColorF {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            method::<Unary>(view, 48)(view);
            method::<Clear>(view, 47)(view, &red);
            assert_eq!(
                method::<End>(view, 49)(view, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            assert_eq!(&pixels(bmp)[0..4], &[0, 0, 255, 255]);
            release_obj(view);
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
    }

    /// #1573 : cibles WIC en RGBA prémultiplié et en alpha seul 8 bits
    /// (masque du pinceau de Paint.NET).
    #[test]
    fn cible_wic_rgba_et_alpha_seul() {
        unsafe {
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
            let prgba = super::super::windowscodecs::FMT_PRGBA32;
            assert_eq!(
                method::<CreateBmp>(wic, 17)(wic, 8, 8, &prgba, 0, &mut bmp),
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
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            method::<Unary>(rt, 48)(rt);
            method::<Clear>(rt, 47)(
                rt,
                &ColorF {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            assert_eq!(&pixels(bmp)[0..4], &[255, 0, 0, 255], "rouge en RGBA");
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
        // Alpha seul : seule la couverture passe, dans les deux sens.
        let a8 = WicLayout::of(&super::super::windowscodecs::FMT_ALPHA8).unwrap();
        let mut own = [0u8; 8];
        a8.read(&[0x80, 0xff], &mut own);
        assert_eq!(own, [0, 0, 0, 0x80, 0, 0, 0, 0xff]);
        let mut wic = [0u8; 2];
        a8.write(&[10, 20, 30, 40, 1, 2, 3, 200], &mut wic);
        assert_eq!(wic, [40, 200]);
        assert!(WicLayout::of(&Guid::default()).is_none());
    }

    /// #1749 : cible WIC 128bppPRGBAFloat (contexte WARP des effets GPU de
    /// Paint.NET) acceptée, lue et écrite en flottants prémultipliés.
    #[test]
    fn cible_wic_flottante_128_bits() {
        let fmt = super::super::wic_pixels::FMT_PRGBA_FLOAT128;
        let l = WicLayout::of(&fmt).expect("128bppPRGBAFloat");
        let f = |v: [f32; 4]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let mut own = [0u8; 4];
        l.read(&f([1.0, 0.5, 0.0, 1.0]), &mut own);
        assert_eq!(own, [0, 128, 255, 255], "RGBA flottant vers BGRA");
        let mut wic = [0u8; 16];
        l.write(&[0, 0, 255, 255], &mut wic);
        assert_eq!(wic.to_vec(), f([1.0, 0.0, 0.0, 1.0]));
        unsafe {
            let mut w = std::ptr::null_mut();
            assert_eq!(wic_create_imaging_factory_proxy(0x237, &mut w), S_OK);
            type CreateBmp = unsafe extern "win64" fn(
                *mut c_void,
                u32,
                u32,
                *const Guid,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut bmp = std::ptr::null_mut();
            assert_eq!(method::<CreateBmp>(w, 17)(w, 2, 2, &fmt, 0, &mut bmp), S_OK);
            release_obj(w);
            let mut fac = std::ptr::null_mut();
            assert_eq!(
                d2d1_create_factory(0, &IID_D2D1_FACTORY, std::ptr::null(), &mut fac),
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
                method::<CreateRt>(fac, 13)(fac, bmp, std::ptr::null(), &mut rt),
                S_OK
            );
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            method::<Unary>(rt, 48)(rt);
            method::<Clear>(rt, 47)(
                rt,
                &ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            type Copy =
                unsafe extern "win64" fn(*mut c_void, *const c_void, u32, u32, *mut u8) -> i32;
            let mut buf = vec![0u8; 2 * 2 * 16];
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
            assert_eq!(buf[..16].to_vec(), f([0.0, 0.0, 1.0, 1.0]), "bleu opaque");
            release_obj(rt);
            release_obj(bmp);
            release_obj(fac);
        }
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

    /// #1124 : pinceau bitmap répété (motif de Scintilla), transformation
    /// du pinceau, dégradé linéaire.
    #[test]
    fn bitmap_and_gradient_brushes_paint() {
        unsafe {
            let (f, bmp, rt) = setup();
            method::<Unary>(rt, 48)(rt);
            // Motif 2×2 : rouge, bleu / bleu, rouge (BGRA prémultiplié).
            let (r, b) = ([0u8, 0, 255, 255], [255u8, 0, 0, 255]);
            let src: Vec<u8> = [r, b, b, r].concat();
            type CreateBitmap = unsafe extern "win64" fn(
                *mut c_void,
                u64,
                *const u8,
                u32,
                *const c_void,
                *mut *mut c_void,
            ) -> i32;
            let mut pattern = std::ptr::null_mut();
            assert_eq!(
                method::<CreateBitmap>(rt, 4)(
                    rt,
                    2 | 2 << 32,
                    src.as_ptr(),
                    8,
                    std::ptr::null(),
                    &mut pattern
                ),
                S_OK
            );
            type BitmapBrush = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const [u32; 3],
                *const c_void,
                *mut *mut c_void,
            ) -> i32;
            let mut brush = std::ptr::null_mut();
            let wrap = [1u32, 1, 0];
            assert_eq!(
                method::<BitmapBrush>(rt, 7)(rt, pattern, &wrap, std::ptr::null(), &mut brush),
                S_OK
            );
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let mut bb1 = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(brush, 0)(brush, &brushes_iid_bitmap1(), &mut bb1),
                S_OK
            );
            release_obj(bb1);
            type FillRect = unsafe extern "win64" fn(*mut c_void, *const RectF, *mut c_void);
            let all = RectF {
                l: 0.0,
                t: 0.0,
                r: 8.0,
                b: 8.0,
            };
            method::<FillRect>(rt, 17)(rt, &all, brush);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            assert_eq!(
                method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            let p = pixels(bmp);
            assert_eq!(px(&p, 0, 0), r);
            assert_eq!(px(&p, 1, 0), b);
            assert_eq!(px(&p, 6, 7), b);
            assert_eq!(px(&p, 7, 7), r);
            // SetTransform décale le motif d'un pixel.
            let shift = Matrix {
                dx: 1.0,
                ..Matrix::IDENTITY
            };
            type SetTransform = unsafe extern "win64" fn(*mut c_void, *const Matrix);
            method::<SetTransform>(brush, 5)(brush, &shift);
            method::<Unary>(rt, 48)(rt);
            method::<FillRect>(rt, 17)(rt, &all, brush);
            method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut());
            let p = pixels(bmp);
            assert_eq!(px(&p, 0, 0), b);
            assert_eq!(px(&p, 1, 0), r);
            release_obj(brush);
            release_obj(pattern);

            // Dégradé linéaire noir → blanc de x = 0 à x = 8.
            let stops: [f32; 10] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
            type Stops = unsafe extern "win64" fn(
                *mut c_void,
                *const f32,
                u32,
                u32,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut coll = std::ptr::null_mut();
            assert_eq!(
                method::<Stops>(rt, 9)(rt, stops.as_ptr(), 2, 0, 0, &mut coll),
                S_OK
            );
            type Linear = unsafe extern "win64" fn(
                *mut c_void,
                *const [f32; 4],
                *const c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> i32;
            let mut lin = std::ptr::null_mut();
            let ends = [0.0f32, 0.0, 8.0, 0.0];
            assert_eq!(
                method::<Linear>(rt, 10)(rt, &ends, std::ptr::null(), coll, &mut lin),
                S_OK
            );
            method::<Unary>(rt, 48)(rt);
            method::<FillRect>(rt, 17)(rt, &all, lin);
            method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut());
            let p = pixels(bmp);
            let grey: Vec<u8> = (0..8).map(|x| px(&p, x, 3)[1]).collect();
            assert!(grey[0] < 24 && grey[7] > 230, "{grey:?}");
            assert!(grey.windows(2).all(|w| w[0] < w[1]), "{grey:?}");
            // Le pinceau rend sa collection.
            type GetStops = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void);
            let mut back = std::ptr::null_mut();
            method::<GetStops>(lin, 12)(lin, &mut back);
            assert_eq!(back, coll);
            release_obj(back);
            release_obj(lin);
            release_obj(coll);
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
    }

    fn brushes_iid_bitmap1() -> Guid {
        guid(
            0x41343a53,
            0xe41a,
            0x49a2,
            [0x91, 0xcd, 0x21, 0x79, 0x3b, 0xbb, 0x62, 0xe5],
        )
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
            // #1368 : un tracé suivant part du contenu du DC ; ce qui n'est
            // pas dessiné garde le fond posé par GDI au lieu de noircir.
            *(bits as *mut u32).add(4 * 8 + 4) = 0x0033_6699;
            method::<Unary>(dct, 48)(dct);
            assert_eq!(
                method::<End>(dct, 49)(dct, std::ptr::null_mut(), std::ptr::null_mut()),
                S_OK
            );
            assert_eq!(at(4, 4), 0x33_6699, "fond GDI conservé");
            assert_eq!(at(3, 3), 0x00_00FF, "tracé précédent conservé");
            release_obj(dct);
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
    }

    /// #1520 : un verrou WIC en 64bppPRGBAHalf (valeurs linéaires
    /// prémultipliées) devient du BGRA 8 bits prémultiplié encodé sRGB.
    #[test]
    fn verrou_demi_flottant_en_bgra() {
        use super::super::wic_pixels::{f32_to_half, layout, FMT_PRGBA_HALF64};
        let l = layout(&FMT_PRGBA_HALF64).unwrap();
        let px = |c: [f32; 4]| -> Vec<u8> {
            c.iter()
                .flat_map(|v| f32_to_half(*v).to_le_bytes())
                .collect()
        };
        // Blanc opaque, puis rouge linéaire 0,5 à demi transparent.
        let src = [px([1.0, 1.0, 1.0, 1.0]), px([0.25, 0.0, 0.0, 0.5])].concat();
        let out = wide_to_pbgra(&l, &src, 2, 1, 16, false);
        assert_eq!(&out[..4], &[255, 255, 255, 255]);
        assert_eq!(&out[4..], &[0, 0, 94, 128]);
        let opaque = wide_to_pbgra(&l, &src, 2, 1, 16, true);
        assert_eq!(&opaque[4..], &[0, 0, 188, 255]);
    }

    /// #1849 : verrou WIC 8bppAlpha (masque de sélection de Paint.NET) lu
    /// en noir prémultiplié de l'opacité de chaque octet, pas refusé.
    #[test]
    fn verrou_wic_alpha_8_bits() {
        // 3×2, pas de ligne 4 octets.
        let src = [10u8, 20, 30, 0, 40, 50, 60, 0];
        let px = unsafe { lock_pixels(src.as_ptr(), 4, 3, 2, false, false, None, true) };
        assert_eq!(px.len(), 3 * 2 * 4);
        assert_eq!(&px[0..4], &[0, 0, 0, 10]);
        assert_eq!(&px[8..12], &[0, 0, 0, 30]);
        assert_eq!(&px[12..16], &[0, 0, 0, 40]);
        assert_eq!(&px[20..24], &[0, 0, 0, 60]);
    }

    /// #1399, #1609 : CreateSharedBitmap partage un IWICBitmapLock
    /// (Paint.NET : DrawBackground, tampon du pinceau) en ID2D1Bitmap.
    #[test]
    fn bitmap_partage_depuis_un_verrou_wic() {
        unsafe {
            let (f, bmp, rt) = setup();
            type Lock =
                unsafe extern "win64" fn(*mut c_void, *const c_void, u32, *mut *mut c_void) -> i32;
            let mut lock = std::ptr::null_mut();
            assert_eq!(
                method::<Lock>(bmp, 8)(bmp, std::ptr::null(), 3, &mut lock),
                S_OK
            );
            type Data = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
            let (mut size, mut data) = (0u32, std::ptr::null_mut());
            assert_eq!(method::<Data>(lock, 5)(lock, &mut size, &mut data), S_OK);
            // Pixel (1, 2) : vert opaque en BGRA prémultiplié.
            std::ptr::copy_nonoverlapping(
                [0u8, 255, 0, 255].as_ptr(),
                data.add((2 * 8 + 1) * 4),
                4,
            );
            type Shared = unsafe extern "win64" fn(
                *mut c_void,
                *const Guid,
                *mut c_void,
                *const [u32; 2],
                *mut *mut c_void,
            ) -> i32;
            let mut b = std::ptr::null_mut();
            assert_eq!(
                method::<Shared>(rt, 6)(rt, &IID_WIC_BITMAP_LOCK, lock, std::ptr::null(), &mut b),
                S_OK
            );
            let bm = &Com::get(b as *mut BitmapObj).data;
            assert_eq!((bm.w, bm.h), (8, 8));
            let px = crate::dlls::lock(&bm.px).clone();
            assert_eq!(&px[(2 * 8 + 1) * 4..][..4], &[0, 255, 0, 255]);
            // #1609 : la mémoire est partagée. Ce que l'appelant écrit dans le
            // verrou après la création (tampon du pinceau de Paint.NET) se
            // retrouve dans le bitmap à l'usage suivant.
            std::ptr::copy_nonoverlapping([0u8, 0, 255, 255].as_ptr(), data, 4);
            let img = v11::image_img(b, 0, (8, 8)).unwrap();
            assert_eq!(&img.px[0..4], &[0, 0, 255, 255]);
            // Un ID2D1Bitmap de Winlux se partage tel quel.
            let mut again = std::ptr::null_mut();
            assert_eq!(
                method::<Shared>(rt, 6)(rt, &IID_D2D1_BITMAP, b, std::ptr::null(), &mut again),
                S_OK
            );
            assert_eq!(again, b);
            release_obj(again);
            // Interface inconnue : refusée.
            let mut none = std::ptr::null_mut();
            assert_eq!(
                method::<Shared>(rt, 6)(rt, &IID_D2D1_GEOMETRY, lock, std::ptr::null(), &mut none),
                E_INVALIDARG
            );
            for o in [b, lock, rt, bmp, f] {
                release_obj(o);
            }
        }
    }

    mod puits {
        use super::*;
        use std::cell::RefCell;
        thread_local! {
            /// (figures, points, mode de remplissage, Close appelé).
            pub static RECU: RefCell<(u32, u32, u32, bool)> = const { RefCell::new((0, 0, 9, false)) };
        }
        unsafe extern "win64" fn un(_: *mut c_void) -> u32 {
            1
        }
        unsafe extern "win64" fn qi(_: *mut c_void, _: *const Guid, _: *mut *mut c_void) -> i32 {
            E_NOINTERFACE
        }
        unsafe extern "win64" fn mode(_: *mut c_void, m: u32) {
            RECU.with(|r| r.borrow_mut().2 = m);
        }
        unsafe extern "win64" fn rien(_: *mut c_void, _: u32) {}
        unsafe extern "win64" fn debut(_: *mut c_void, _: u64, _: u32) {
            RECU.with(|r| {
                let mut r = r.borrow_mut();
                r.0 += 1;
                r.1 += 1;
            });
        }
        unsafe extern "win64" fn lignes(_: *mut c_void, _: *const Pt, n: u32) {
            RECU.with(|r| r.borrow_mut().1 += n);
        }
        unsafe extern "win64" fn courbes(_: *mut c_void, _: *const c_void, _: u32) {}
        unsafe extern "win64" fn fermer(_: *mut c_void) -> i32 {
            RECU.with(|r| r.borrow_mut().3 = true);
            S_OK
        }
        pub static VT: VTable<10> = VTable([
            qi as *const (),
            un as *const (),
            un as *const (),
            mode as *const (),
            rien as *const (),
            debut as *const (),
            lignes as *const (),
            courbes as *const (),
            rien as *const (),
            fermer as *const (),
        ]);
    }

    /// #1471 : une bitmap A8 lit un octet d'alpha par pixel.
    #[test]
    fn bitmap_a8_lit_un_octet_par_pixel() {
        unsafe {
            let f = Com::create(FACTORY_VT.ptr(), Factory);
            let mut b = Bitmap::new(3, 2, vec![0xEE; 3 * 2 * 4]);
            b.alpha_only = true;
            let obj = new_res(f, BITMAP_VT.ptr(), b);
            // Pas de 4 octets par ligne, trois utiles.
            let src = [10u8, 20, 30, 0xFF, 40, 50, 60, 0xFF];
            assert_eq!(
                bitmap_copy_from_memory(obj, std::ptr::null(), src.as_ptr(), 4),
                S_OK
            );
            let (w, h, px) = bitmap_pixels(obj.cast()).unwrap();
            assert_eq!((w, h), (3, 2));
            assert_eq!(&px[0..4], &[0, 0, 0, 10]);
            assert_eq!(&px[20..24], &[0, 0, 0, 60]);
            Com::release(obj);
            Com::release(f);
        }
    }

    /// #1470 : Widen, GetWidenedBounds, StrokeContainsPoint et ComputeLength.
    /// #1549 : CombineWithGeometry (boutons de couleur de Paint.NET) remplit
    /// un chemin avec l'intersection de deux rectangles.
    #[test]
    fn geometries_combinees() {
        unsafe {
            let f = Com::create(FACTORY_VT.ptr(), Factory);
            type RectGeo =
                unsafe extern "win64" fn(*mut c_void, *const RectF, *mut *mut c_void) -> i32;
            type Create = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            type Combine = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                u32,
                *const Matrix,
                f32,
                *mut c_void,
            ) -> i32;
            type Contains =
                unsafe extern "win64" fn(*mut c_void, u64, *const Matrix, f32, *mut i32) -> i32;
            type Close = unsafe extern "win64" fn(*mut c_void) -> i32;
            let rect = |l, t, r, b| {
                let mut g = std::ptr::null_mut();
                let rc = RectF { l, t, r, b };
                assert_eq!(method::<RectGeo>(f.cast(), 5)(f.cast(), &rc, &mut g), S_OK);
                g
            };
            let a = rect(0.0, 0.0, 10.0, 10.0);
            let b = rect(0.0, 0.0, 10.0, 10.0);
            // L'entrée est déplacée de (5, 5) par sa transformation.
            let m = Matrix {
                dx: 5.0,
                dy: 5.0,
                ..Matrix::IDENTITY
            };
            let mut path = std::ptr::null_mut();
            assert_eq!(method::<Create>(f.cast(), 10)(f.cast(), &mut path), S_OK);
            let mut sink = std::ptr::null_mut();
            assert_eq!(method::<Create>(path, 17)(path, &mut sink), S_OK);
            assert_eq!(method::<Combine>(a, 11)(a, b, 1, &m, 0.25, sink), S_OK);
            assert_eq!(method::<Close>(sink, 9)(sink), S_OK);
            release_obj(sink);
            let at = |x, y| {
                let mut v = 0;
                method::<Contains>(path, 7)(path, pt(x, y), std::ptr::null(), 0.25, &mut v);
                v
            };
            assert_eq!(at(7.0, 7.0), 1, "dans l'intersection");
            assert_eq!(at(2.0, 2.0), 0, "seulement dans a");
            assert_eq!(at(12.0, 12.0), 0, "seulement dans b");
            assert_eq!(
                method::<Combine>(a, 11)(a, std::ptr::null_mut(), 0, &m, 0.25, sink),
                E_INVALIDARG
            );
            release_obj(path);
            release_obj(b);
            release_obj(a);
            release_obj(f.cast());
        }
    }

    #[test]
    fn geometrie_elargie() {
        unsafe {
            let f = Com::create(FACTORY_VT.ptr(), Factory);
            type RectGeo =
                unsafe extern "win64" fn(*mut c_void, *const RectF, *mut *mut c_void) -> i32;
            let mut g = std::ptr::null_mut();
            assert_eq!(
                method::<RectGeo>(f.cast(), 5)(
                    f.cast(),
                    &RectF {
                        l: 0.0,
                        t: 0.0,
                        r: 10.0,
                        b: 10.0
                    },
                    &mut g
                ),
                S_OK
            );
            // Géométrie transformée (échelle 2), comme chez Paint.NET.
            type Transformed = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const Matrix,
                *mut *mut c_void,
            ) -> i32;
            let m = Matrix {
                m11: 2.0,
                m22: 2.0,
                ..Matrix::IDENTITY
            };
            let mut tg = std::ptr::null_mut();
            assert_eq!(
                method::<Transformed>(f.cast(), 9)(f.cast(), g, &m, &mut tg),
                S_OK
            );
            type WBounds = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                *mut c_void,
                *const Matrix,
                f32,
                *mut RectF,
            ) -> i32;
            let mut r = RectF {
                l: 0.0,
                t: 0.0,
                r: 0.0,
                b: 0.0,
            };
            assert_eq!(
                method::<WBounds>(tg, 5)(
                    tg,
                    4.0,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    0.25,
                    &mut r
                ),
                S_OK
            );
            assert!(
                (r.l + 2.0).abs() < 0.1 && (r.r - 22.0).abs() < 0.1,
                "{:?}",
                (r.l, r.r)
            );
            type SContains = unsafe extern "win64" fn(
                *mut c_void,
                u64,
                f32,
                *mut c_void,
                *const Matrix,
                f32,
                *mut i32,
            ) -> i32;
            let mut inside = 0;
            method::<SContains>(tg, 6)(
                tg,
                pt(21.0, 10.0),
                4.0,
                std::ptr::null_mut(),
                std::ptr::null(),
                0.25,
                &mut inside,
            );
            assert_eq!(inside, 1, "sur le bord droit");
            method::<SContains>(tg, 6)(
                tg,
                pt(10.0, 10.0),
                4.0,
                std::ptr::null_mut(),
                std::ptr::null(),
                0.25,
                &mut inside,
            );
            assert_eq!(inside, 0, "au centre");
            type Length =
                unsafe extern "win64" fn(*mut c_void, *const Matrix, f32, *mut f32) -> i32;
            let mut len = 0.0;
            assert_eq!(
                method::<Length>(tg, 14)(tg, std::ptr::null(), 0.25, &mut len),
                S_OK
            );
            assert!((len - 80.0).abs() < 0.01, "{len}");
            // Widen : figures fermées, règle non nulle, Close appelé.
            puits::RECU.with(|r| *r.borrow_mut() = (0, 0, 9, false));
            let mut sink = puits::VT.ptr();
            let sink_ptr = &mut sink as *mut *const () as *mut c_void;
            type Widen = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                *mut c_void,
                *const Matrix,
                f32,
                *mut c_void,
            ) -> i32;
            assert_eq!(
                method::<Widen>(tg, 16)(
                    tg,
                    4.0,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    0.25,
                    sink_ptr
                ),
                S_OK
            );
            puits::RECU.with(|r| {
                let r = r.borrow();
                assert!(r.0 >= 8, "figures : {}", r.0);
                assert_eq!((r.2, r.3), (1, true));
            });
            for o in [tg, g] {
                release_obj(o);
            }
            Com::release(f);
        }
    }

    /// #1400 : groupe de géométries, géométrie transformée, Simplify et Stream.
    #[test]
    fn groupe_transformation_simplify_et_stream() {
        unsafe {
            let f = Com::create(FACTORY_VT.ptr(), Factory);
            type RectGeo =
                unsafe extern "win64" fn(*mut c_void, *const RectF, *mut *mut c_void) -> i32;
            let rect = |l: f32, t: f32, r: f32, b: f32| {
                let mut g = std::ptr::null_mut();
                assert_eq!(
                    method::<RectGeo>(f.cast(), 5)(f.cast(), &RectF { l, t, r, b }, &mut g),
                    S_OK
                );
                g
            };
            let a = rect(0.0, 0.0, 2.0, 2.0);
            let b = rect(10.0, 10.0, 12.0, 12.0);
            type Group = unsafe extern "win64" fn(
                *mut c_void,
                u32,
                *const *mut c_void,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut grp = std::ptr::null_mut();
            assert_eq!(
                method::<Group>(f.cast(), 8)(f.cast(), 1, [a, b].as_ptr(), 2, &mut grp),
                S_OK
            );
            type Contains =
                unsafe extern "win64" fn(*mut c_void, u64, *const Matrix, f32, *mut i32) -> i32;
            let mut inside = 0;
            method::<Contains>(grp, 7)(grp, pt(11.0, 11.0), std::ptr::null(), 0.25, &mut inside);
            assert_eq!(inside, 1);
            method::<Contains>(grp, 7)(grp, pt(5.0, 5.0), std::ptr::null(), 0.25, &mut inside);
            assert_eq!(inside, 0);
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void) -> u32>(grp, 17)(grp),
                1
            );
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void) -> u32>(grp, 18)(grp),
                2
            );
            // Géométrie transformée : translation de (100, 0).
            type Transformed = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const Matrix,
                *mut *mut c_void,
            ) -> i32;
            let m = Matrix {
                dx: 100.0,
                ..Matrix::IDENTITY
            };
            let mut tg = std::ptr::null_mut();
            assert_eq!(
                method::<Transformed>(f.cast(), 9)(f.cast(), a, &m, &mut tg),
                S_OK
            );
            type Bounds = unsafe extern "win64" fn(*mut c_void, *const Matrix, *mut RectF) -> i32;
            let mut r = RectF {
                l: 0.0,
                t: 0.0,
                r: 0.0,
                b: 0.0,
            };
            assert_eq!(method::<Bounds>(tg, 4)(tg, std::ptr::null(), &mut r), S_OK);
            assert_eq!((r.l, r.r), (100.0, 102.0));
            // Simplify du groupe : deux figures de quatre points, règle non nulle.
            let mut sink = puits::VT.ptr();
            let sink_ptr = &mut sink as *mut *const () as *mut c_void;
            type Simplify =
                unsafe extern "win64" fn(*mut c_void, u32, *const Matrix, f32, *mut c_void) -> i32;
            assert_eq!(
                method::<Simplify>(grp, 9)(grp, 0, std::ptr::null(), 0.25, sink_ptr),
                S_OK
            );
            puits::RECU.with(|r| assert_eq!(*r.borrow(), (2, 8, 1, true)));
            // Stream d'un chemin.
            puits::RECU.with(|r| *r.borrow_mut() = (0, 0, 9, false));
            let mut path = std::ptr::null_mut();
            assert_eq!(create_path_geometry(f, &mut path), S_OK);
            let mut s = std::ptr::null_mut();
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32>(path, 17)(
                    path, &mut s
                ),
                S_OK
            );
            method::<unsafe extern "win64" fn(*mut c_void, u64, u32)>(s, 5)(s, pt(0.0, 0.0), 0);
            method::<unsafe extern "win64" fn(*mut c_void, u64)>(s, 10)(s, pt(4.0, 0.0));
            method::<unsafe extern "win64" fn(*mut c_void, u64)>(s, 10)(s, pt(4.0, 4.0));
            method::<unsafe extern "win64" fn(*mut c_void, u32)>(s, 8)(s, 1);
            method::<unsafe extern "win64" fn(*mut c_void) -> i32>(s, 9)(s);
            release_obj(s);
            type Stream = unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32;
            assert_eq!(method::<Stream>(path, 18)(path, sink_ptr), S_OK);
            puits::RECU.with(|r| assert_eq!(*r.borrow(), (1, 3, 0, true)));
            for o in [path, tg, grp, a, b] {
                release_obj(o);
            }
            Com::release(f);
        }
    }

    /// #1743 : le DPI de la cible (SetDpi ou D2D1_RENDER_TARGET_PROPERTIES)
    /// met les DIP à l'échelle, SetAntialiasMode(ALIASED) couvre les pixels
    /// dont le centre est dans la forme, et le mélange MAX garde le plus fort.
    /// #1867 : DrawLine avec un style de trait en tirets laisse les vides
    /// transparents (contour de sélection de Paint.NET, tracé en tirets).
    #[test]
    fn trait_en_tirets_dans_une_cible() {
        unsafe {
            type DrawLine =
                unsafe extern "win64" fn(*mut c_void, u64, u64, *mut c_void, f32, *mut c_void);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            type SetAa = unsafe extern "win64" fn(*mut c_void, u32);
            let (f, bmp, rt) = setup();
            let noir = solid(
                rt,
                ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            // Tirets personnalisés (5) de 3 et vides de 3, en épaisseurs.
            let props: [u32; 7] = [0, 0, 0, 0, 10f32.to_bits(), 5, 0f32.to_bits()];
            let dashes = [3.0f32, 3.0];
            let mut style = std::ptr::null_mut();
            assert_eq!(
                v11::factory_stroke_style(
                    f.cast(),
                    props.as_ptr().cast(),
                    dashes.as_ptr(),
                    2,
                    &mut style
                ),
                S_OK
            );
            let pt = |x: f32, y: f32| (x.to_bits() as u64) | ((y.to_bits() as u64) << 32);
            method::<Unary>(rt, 48)(rt);
            method::<SetAa>(rt, 32)(rt, 1);
            method::<DrawLine>(rt, 15)(rt, pt(0.0, 4.5), pt(8.0, 4.5), noir, 1.0, style);
            method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut());
            let p = pixels(bmp);
            // Tirets sur [0,3] et [6,8], vide sur [3,6].
            assert_eq!(px(&p, 1, 4)[3], 255);
            assert_eq!(px(&p, 6, 4)[3], 255);
            assert_eq!(px(&p, 4, 4)[3], 0, "vide du motif");
            for o in [style, noir, rt, bmp, f] {
                release_obj(o);
            }
        }
    }

    #[test]
    fn dpi_anticrenelage_et_melange() {
        unsafe {
            type FillRect = unsafe extern "win64" fn(*mut c_void, *const RectF, *mut c_void);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            type SetDpi = unsafe extern "win64" fn(*mut c_void, f32, f32);
            type SetAa = unsafe extern "win64" fn(*mut c_void, u32);
            type GetTransform = unsafe extern "win64" fn(*mut c_void, *mut Matrix);
            let rect = |l: f32, t: f32, r: f32, b: f32| RectF { l, t, r, b };
            let (f, bmp, rt) = setup();
            let noir = solid(
                rt,
                ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            method::<Unary>(rt, 48)(rt);
            method::<SetDpi>(rt, 51)(rt, 192.0, 192.0);
            method::<FillRect>(rt, 17)(rt, &rect(0.0, 0.0, 1.0, 1.0), noir);
            let mut m = Matrix {
                m11: 0.0,
                ..Matrix::IDENTITY
            };
            method::<GetTransform>(rt, 31)(rt, &mut m);
            assert_eq!(m, Matrix::IDENTITY, "GetTransform ne voit pas le DPI");
            // SetDpi(0, 0) revient à 96.
            method::<SetDpi>(rt, 51)(rt, 0.0, 0.0);
            // Un demi-pixel centré : crénelé, le pixel est plein.
            method::<SetAa>(rt, 32)(rt, 1);
            assert_eq!(method::<Unary>(rt, 33)(rt), 1);
            method::<FillRect>(rt, 17)(rt, &rect(4.25, 4.25, 4.75, 4.75), noir);
            method::<SetAa>(rt, 32)(rt, 0);
            method::<FillRect>(rt, 17)(rt, &rect(6.25, 6.25, 6.75, 6.75), noir);
            method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut());
            let p = pixels(bmp);
            for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                assert_eq!(px(&p, x, y), [0, 0, 0, 255], "DIP à 192 ppp ({x},{y})");
            }
            assert_eq!(px(&p, 2, 0)[3], 0);
            assert_eq!(px(&p, 4, 4), [0, 0, 0, 255]);
            assert_eq!(px(&p, 5, 4)[3], 0);
            let a = px(&p, 6, 6)[3];
            assert!((40..=90).contains(&a), "anticrénelé : {a}");
            release_obj(noir);
            release_obj(rt);

            // DPI donné à la création de la cible WIC.
            #[repr(C)]
            struct Props {
                kind: u32,
                format: u32,
                alpha: u32,
                dpi: [f32; 2],
                usage: u32,
                level: u32,
            }
            let props = Props {
                kind: 0,
                format: 0,
                alpha: 0,
                dpi: [288.0, 288.0],
                usage: 0,
                level: 0,
            };
            type CreateRt = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const c_void,
                *mut *mut c_void,
            ) -> i32;
            let mut rt = std::ptr::null_mut();
            assert_eq!(
                method::<CreateRt>(f, 13)(f, bmp, &props as *const Props as *const c_void, &mut rt),
                S_OK
            );
            let mut dpi = (0f32, 0f32);
            type GetDpi = unsafe extern "win64" fn(*mut c_void, *mut f32, *mut f32);
            method::<GetDpi>(rt, 52)(rt, &mut dpi.0, &mut dpi.1);
            assert_eq!(dpi, (288.0, 288.0));
            let blanc = solid(
                rt,
                ColorF {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            method::<Unary>(rt, 48)(rt);
            method::<FillRect>(rt, 17)(rt, &rect(1.0, 1.0, 2.0, 2.0), blanc);
            method::<End>(rt, 49)(rt, std::ptr::null_mut(), std::ptr::null_mut());
            let p = pixels(bmp);
            assert_eq!(px(&p, 3, 3), [255, 255, 255, 255]);
            assert_eq!(px(&p, 5, 5), [255, 255, 255, 255]);
            assert_eq!(px(&p, 2, 2)[3], 0);
            assert_eq!(px(&p, 6, 5)[3], 0);
            release_obj(blanc);
            release_obj(rt);
            release_obj(bmp);
            Com::release(f as *mut FactoryObj);
        }
    }

    /// #1743 : D2D1_PRIMITIVE_BLEND MIN, ADD, MAX et COPY.
    #[test]
    fn melange_des_primitives() {
        let carre = vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]];
        let essai = |dest: [u8; 4], src: Color, blend: u32| {
            let mut px = dest.to_vec();
            let mut c = Canvas {
                w: 1,
                h: 1,
                stride: 4,
                px: &mut px,
            };
            c.fill_mode(
                &carre,
                FillRule::NonZero,
                None,
                &|_, _| src,
                FillMode {
                    aliased: false,
                    blend,
                },
            );
            px
        };
        // Destination BGRA (100, 100, 100, 100), source rgba 0.2 / 0.8.
        let src = [0.8, 0.2, 0.2, 0.8];
        assert_eq!(essai([100; 4], src, 2), [51, 51, 100, 100]);
        assert_eq!(essai([100; 4], src, 4), [100, 100, 204, 204]);
        assert_eq!(essai([100; 4], src, 3), [151, 151, 255, 255]);
        assert_eq!(essai([100; 4], src, 1), [51, 51, 204, 204]);
    }

    /// #1749 : CopyFromBitmap lit les pixels en cours de tracé d'un bitmap
    /// cible et les place au point demandé ; CopyFromRenderTarget lit ceux
    /// de la cible ; une zone hors du bitmap est refusée.
    #[test]
    fn copie_entre_bitmaps() {
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
            type Clear = unsafe extern "win64" fn(*mut c_void, *const ColorF);
            type End = unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32;
            // Rouge dessiné, EndDraw pas encore appelé.
            method::<Unary>(off, 48)(off);
            method::<Clear>(off, 47)(
                off,
                &ColorF {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            let mut src = std::ptr::null_mut();
            type GetBitmap = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            assert_eq!(method::<GetBitmap>(off, 57)(off, &mut src), S_OK);
            type CreateBitmap = unsafe extern "win64" fn(
                *mut c_void,
                u64,
                *const u8,
                u32,
                *const c_void,
                *mut *mut c_void,
            ) -> i32;
            let mut dst = std::ptr::null_mut();
            assert_eq!(
                method::<CreateBitmap>(rt, 4)(
                    rt,
                    4 | 4 << 32,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut dst
                ),
                S_OK
            );
            type Copy = unsafe extern "win64" fn(
                *mut c_void,
                *const [u32; 2],
                *mut c_void,
                *const RectU,
            ) -> i32;
            let zone = RectU {
                l: 0,
                t: 0,
                r: 2,
                b: 2,
            };
            assert_eq!(method::<Copy>(dst, 8)(dst, &[1, 1], src, &zone), S_OK);
            let px = |b: *mut c_void, x: usize, y: usize| -> [u8; 4] {
                let d = &Com::get(b as *mut BitmapObj).data;
                let p = crate::dlls::lock(&d.px);
                p[(y * d.w as usize + x) * 4..][..4].try_into().unwrap()
            };
            assert_eq!(px(dst, 1, 1), [0, 0, 255, 255], "rouge en cours de tracé");
            assert_eq!(px(dst, 2, 2), [0, 0, 255, 255]);
            assert_eq!(px(dst, 0, 0)[3], 0);
            assert_eq!(px(dst, 3, 3)[3], 0);
            // Hors du bitmap de destination.
            assert_eq!(
                method::<Copy>(dst, 8)(dst, &[3, 3], src, &zone),
                E_INVALIDARG
            );
            // CopyFromRenderTarget depuis la cible hors écran.
            method::<Clear>(off, 47)(
                off,
                &ColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            );
            assert_eq!(
                method::<Copy>(dst, 9)(dst, std::ptr::null(), off, std::ptr::null()),
                S_OK
            );
            assert_eq!(px(dst, 3, 3), [255, 0, 0, 255], "bleu de la cible");
            method::<End>(off, 49)(off, std::ptr::null_mut(), std::ptr::null_mut());
            release_obj(dst);
            release_obj(src);
            release_obj(off);
            release_obj(rt);
            release_obj(bmp);
            release_obj(f);
        }
    }
}
