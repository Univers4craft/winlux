//! Pinceaux Direct2D autres que la couleur unie : motif de bitmap
//! (ID2D1BitmapBrush1), image (ID2D1ImageBrush), dégradés linéaire et
//! radial. Chaque pinceau devient, au moment du tracé, une fonction
//! « pixel de la cible → couleur prémultipliée » pour le rastériseur.
//! Écrit d'après la documentation publique de Direct2D et les en-têtes
//! mingw-w64 (ordre des méthodes, structures de propriétés).

use super::*;

const IID_D2D1_BITMAP_BRUSH: Guid = guid(
    0x2cd906aa,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_LINEAR_BRUSH: Guid = guid(
    0x2cd906ab,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_RADIAL_BRUSH: Guid = guid(
    0x2cd906ac,
    0x12e2,
    0x11dc,
    [0x9f, 0xed, 0x00, 0x11, 0x43, 0xa0, 0x55, 0xf9],
);
const IID_D2D1_IMAGE_BRUSH: Guid = guid(
    0xfe9e984d,
    0x3f95,
    0x407c,
    [0xb5, 0xdb, 0xcb, 0x94, 0xd4, 0xe8, 0xf8, 0x7c],
);
const IID_D2D1_BITMAP_BRUSH1: Guid = guid(
    0x41343a53,
    0xe41a,
    0x49a2,
    [0x91, 0xcd, 0x21, 0x79, 0x3b, 0xbb, 0x62, 0xe5],
);

/// D2D1_EXTEND_MODE.
const CLAMP: u32 = 0;
const WRAP: u32 = 1;
const MIRROR: u32 = 2;

/// D2D1_BRUSH_PROPERTIES : opacité puis matrice 3×2.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct BrushProps {
    opacity: f32,
    transform: [f32; 6],
}

/// D2D1_BITMAP_BRUSH_PROPERTIES et D2D1_BITMAP_BRUSH_PROPERTIES1 (même
/// disposition ; l'interpolation 1.0 vaut 0 plus proche, 1 linéaire).
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct BitmapBrushProps {
    extend_x: u32,
    extend_y: u32,
    interpolation: u32,
}

/// D2D1_IMAGE_BRUSH_PROPERTIES.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ImageBrushProps {
    source: RectF,
    extend_x: u32,
    extend_y: u32,
    interpolation: u32,
}

/// D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct RadialProps {
    center: [f32; 2],
    offset: [f32; 2],
    rx: f32,
    ry: f32,
}

/// Interface tenue par le pinceau (AddRef à la prise, Release à l'abandon).
pub(super) struct Held(*mut c_void);

impl Held {
    unsafe fn new(p: *mut c_void) -> Held {
        if !p.is_null() {
            method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 1)(p);
        }
        Held(p)
    }
    /// Rend l'interface avec une référence de plus pour l'appelant.
    unsafe fn give(&self, out: *mut *mut c_void) {
        if out.is_null() {
            return;
        }
        if !self.0.is_null() {
            method::<unsafe extern "win64" fn(*mut c_void) -> u32>(self.0, 1)(self.0);
        }
        *out = self.0;
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(self.0, 2)(self.0) };
        }
    }
}

pub(super) enum Kind {
    Solid,
    /// Bitmap ou image : `source` borne la tuile dans l'espace de l'image
    /// (None : l'image entière). `interpolation` : 0 plus proche, sinon
    /// linéaire.
    Image {
        image: Held,
        source: Option<RectF>,
        extend: [u32; 2],
        interpolation: u32,
    },
    Linear {
        start: Pt,
        end: Pt,
        stops: Held,
    },
    Radial {
        center: Pt,
        offset: Pt,
        rx: f32,
        ry: f32,
        stops: Held,
    },
}

fn matrix(m: [f32; 6]) -> Matrix {
    Matrix {
        m11: m[0],
        m12: m[1],
        m21: m[2],
        m22: m[3],
        dx: m[4],
        dy: m[5],
    }
}

pub(super) unsafe fn new_brush(color: ColorF, props: *const BrushProps, kind: Kind) -> Brush {
    let (opacity, transform) = match props.as_ref() {
        Some(p) => (p.opacity.clamp(0.0, 1.0), matrix(p.transform)),
        None => (1.0, Matrix::IDENTITY),
    };
    Brush {
        color: Mutex::new(color),
        opacity: Mutex::new(opacity),
        transform: Mutex::new(transform),
        kind: Mutex::new(kind),
    }
}

unsafe fn brush<'a>(this: *mut c_void) -> &'a Brush {
    &Com::get(this as *mut BrushObj).data
}

unsafe fn kind<'a>(this: *mut c_void) -> std::sync::MutexGuard<'a, Kind> {
    crate::dlls::lock(&brush(this).kind)
}

// --- ID2D1Brush (commun) -----------------------------------------------------------------

pub(super) unsafe extern "win64" fn set_transform(this: *mut c_void, m: *const Matrix) {
    if let Some(m) = m.as_ref() {
        *crate::dlls::lock(&brush(this).transform) = *m;
    }
}
pub(super) unsafe extern "win64" fn get_transform(this: *mut c_void, m: *mut Matrix) {
    if !m.is_null() {
        *m = *crate::dlls::lock(&brush(this).transform);
    }
}

/// Les vtables de pinceaux Winlux, uni compris.
fn is_brush(vt: *const ()) -> bool {
    vt == BRUSH_VT.ptr()
        || vt == BITMAP_BRUSH_VT.ptr()
        || vt == IMAGE_BRUSH_VT.ptr()
        || vt == LINEAR_VT.ptr()
        || vt == RADIAL_VT.ptr()
}

// --- Peinture -----------------------------------------------------------------------------

pub(super) type Paint = Box<dyn Fn(f32, f32) -> Color>;

/// Fonction de peinture d'un pinceau sous la transformation `world` de la
/// cible : (x, y) du centre d'un pixel → couleur prémultipliée. None pour
/// un pinceau inconnu ou dont l'image ne se calcule pas.
pub(super) unsafe fn paint(brush_obj: *mut c_void, world: &Matrix) -> Option<Paint> {
    if brush_obj.is_null() || !is_brush(*(brush_obj as *const *const ())) {
        return None;
    }
    let b = brush(brush_obj);
    let opacity = *crate::dlls::lock(&b.opacity);
    let to_brush = crate::dlls::lock(&b.transform).then(world).invert();
    let k = crate::dlls::lock(&b.kind);
    match &*k {
        Kind::Solid => {
            let c = *crate::dlls::lock(&b.color);
            let c = premul(c.r, c.g, c.b, c.a * opacity);
            Some(Box::new(move |_, _| c))
        }
        Kind::Image {
            image,
            source,
            extend,
            interpolation,
        } => {
            let inv = to_brush?;
            let image = image.0;
            let source = *source;
            let (extend, interpolation) = (*extend, *interpolation);
            drop(k);
            let img = v11::image_img(image, 0, (4096, 4096))?;
            let tile = Tile::new(img, source, extend, interpolation, opacity)?;
            Some(Box::new(move |x, y| tile.at(inv.apply((x, y)))))
        }
        Kind::Linear { start, end, stops } => {
            let inv = to_brush?;
            let ramp = Ramp::new(stops.0, opacity)?;
            let (s, e) = (*start, *end);
            let (dx, dy) = (e.0 - s.0, e.1 - s.1);
            let len2 = dx * dx + dy * dy;
            Some(Box::new(move |x, y| {
                let (u, v) = inv.apply((x, y));
                let t = if len2 > 0.0 {
                    ((u - s.0) * dx + (v - s.1) * dy) / len2
                } else {
                    1.0
                };
                ramp.at(t)
            }))
        }
        Kind::Radial {
            center,
            offset,
            rx,
            ry,
            stops,
        } => {
            let inv = to_brush?;
            let ramp = Ramp::new(stops.0, opacity)?;
            let (c, (rx, ry)) = (*center, (*rx, *ry));
            if rx <= 0.0 || ry <= 0.0 {
                let last = ramp.at(1.0);
                return Some(Box::new(move |_, _| last));
            }
            // Foyer ramené dans le cercle unité, un peu en deçà du bord.
            let (mut fx, mut fy) = (offset.0 / rx, offset.1 / ry);
            let fl = (fx * fx + fy * fy).sqrt();
            if fl > 0.999 {
                fx *= 0.999 / fl;
                fy *= 0.999 / fl;
            }
            Some(Box::new(move |x, y| {
                let (u, v) = inv.apply((x, y));
                ramp.at(radial_t((u - c.0) / rx, (v - c.1) / ry, fx, fy))
            }))
        }
    }
}

/// Position dans le dégradé radial, en coordonnées où l'ellipse est le
/// cercle unité et le foyer (fx, fy) est à l'intérieur : le point vaut
/// F + t·(Q − F) avec Q sur le cercle.
fn radial_t(px: f32, py: f32, fx: f32, fy: f32) -> f32 {
    let (dx, dy) = (px - fx, py - fy);
    let a = fx * fx + fy * fy - 1.0;
    let b = 2.0 * (fx * dx + fy * dy);
    let c = dx * dx + dy * dy;
    if c == 0.0 {
        return 0.0;
    }
    let disc = (b * b - 4.0 * a * c).max(0.0);
    (-b - disc.sqrt()) / (2.0 * a)
}

/// Arrêts de dégradé prémultipliés, opacité du pinceau comprise.
struct Ramp {
    stops: Vec<(f32, Color)>,
    extend: u32,
}

impl Ramp {
    unsafe fn new(collection: *mut c_void, opacity: f32) -> Option<Ramp> {
        let (stops, extend) = v11::gradient_of(collection)?;
        if stops.is_empty() {
            return None;
        }
        Some(Ramp {
            stops: stops
                .into_iter()
                .map(|(p, c)| (p, premul(c[0], c[1], c[2], c[3] * opacity)))
                .collect(),
            extend,
        })
    }

    fn at(&self, t: f32) -> Color {
        let t = match self.extend {
            WRAP => t.rem_euclid(1.0),
            MIRROR => {
                let k = t.rem_euclid(2.0);
                if k > 1.0 {
                    2.0 - k
                } else {
                    k
                }
            }
            _ => t,
        };
        let s = &self.stops;
        if t.is_nan() || t <= s[0].0 {
            return s[0].1;
        }
        for w in s.windows(2) {
            let ((p0, c0), (p1, c1)) = (w[0], w[1]);
            if t <= p1 {
                let f = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
                let mut o = [0f32; 4];
                for i in 0..4 {
                    o[i] = c0[i] + (c1[i] - c0[i]) * f;
                }
                return o;
            }
        }
        s[s.len() - 1].1
    }
}

/// Tuile de pixels d'un pinceau image ou bitmap.
struct Tile {
    px: Vec<u8>,
    w: i32,
    /// Origine de l'image dans l'espace du pinceau.
    ox: f32,
    oy: f32,
    /// Pixels de la tuile : [x0, x1) × [y0, y1).
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    extend: [u32; 2],
    linear: bool,
    opacity: f32,
}

/// Indice entier ramené dans [lo, hi) selon le mode d'extension.
fn extend_index(i: i32, lo: i32, hi: i32, mode: u32) -> i32 {
    let n = hi - lo;
    match mode {
        WRAP => lo + (i - lo).rem_euclid(n),
        MIRROR => {
            let k = (i - lo).rem_euclid(2 * n);
            lo + if k < n { k } else { 2 * n - 1 - k }
        }
        _ => i.clamp(lo, hi - 1),
    }
}

impl Tile {
    fn new(
        img: v11::Img,
        source: Option<RectF>,
        extend: [u32; 2],
        interpolation: u32,
        opacity: f32,
    ) -> Option<Tile> {
        let (w, h) = (img.w as i32, img.h as i32);
        if w <= 0 || h <= 0 || img.px.len() < (w * h * 4) as usize {
            return None;
        }
        let (ox, oy) = (img.x as f32, img.y as f32);
        let (mut x0, mut y0, mut x1, mut y1) = (0, 0, w, h);
        if let Some(s) = source {
            x0 = ((s.l - ox).floor() as i32).clamp(0, w);
            y0 = ((s.t - oy).floor() as i32).clamp(0, h);
            x1 = ((s.r - ox).ceil() as i32).clamp(0, w);
            y1 = ((s.b - oy).ceil() as i32).clamp(0, h);
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Tile {
            px: img.px,
            w,
            ox,
            oy,
            x0,
            x1,
            y0,
            y1,
            extend,
            linear: interpolation != 0,
            opacity: opacity / 255.0,
        })
    }

    fn texel(&self, x: i32, y: i32) -> [f32; 4] {
        let x = extend_index(x, self.x0, self.x1, self.extend[0]);
        let y = extend_index(y, self.y0, self.y1, self.extend[1]);
        let o = (y * self.w + x) as usize * 4;
        let p = &self.px;
        [
            p[o + 2] as f32,
            p[o + 1] as f32,
            p[o] as f32,
            p[o + 3] as f32,
        ]
    }

    fn at(&self, (u, v): Pt) -> Color {
        let (u, v) = (u - self.ox, v - self.oy);
        let c = if self.linear {
            let (fu, fv) = (u - 0.5, v - 0.5);
            let (x, y) = (fu.floor() as i32, fv.floor() as i32);
            let (tx, ty) = (fu - x as f32, fv - y as f32);
            let (a, b, c, d) = (
                self.texel(x, y),
                self.texel(x + 1, y),
                self.texel(x, y + 1),
                self.texel(x + 1, y + 1),
            );
            let mut o = [0f32; 4];
            for i in 0..4 {
                o[i] = (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty)
                    + (c[i] * (1.0 - tx) + d[i] * tx) * ty;
            }
            o
        } else {
            self.texel(u.floor() as i32, v.floor() as i32)
        };
        [
            c[0] * self.opacity,
            c[1] * self.opacity,
            c[2] * self.opacity,
            c[3] * self.opacity,
        ]
    }
}

// --- Fabriques ----------------------------------------------------------------------------

unsafe fn create<const N: usize>(
    this: *mut TargetObj,
    vt: &'static VTable<N>,
    props: *const BrushProps,
    kind: Kind,
    out: *mut *mut c_void,
) -> i32 {
    let b = new_brush(ColorF::default(), props, kind);
    put(out, new_res(Com::get(this).factory, vt.ptr(), b))
}

/// Les bitmaps et images Winlux seulement : un pinceau ne peut rien lire
/// d'autre.
unsafe fn is_image(p: *mut c_void) -> bool {
    !p.is_null() && v11::image_img(p, 0, (1, 1)).is_some()
}

/// ID2D1RenderTarget::CreateBitmapBrush (emplacement 7).
pub(super) unsafe extern "win64" fn create_bitmap_brush(
    this: *mut TargetObj,
    bitmap: *mut c_void,
    bprops: *const BitmapBrushProps,
    props: *const BrushProps,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    // Le bitmap peut être nul (SetBitmap plus tard) : le pinceau ne
    // peint alors rien.
    if !bitmap.is_null() && *(bitmap as *const *const ()) != BITMAP_VT.ptr() {
        return E_INVALIDARG;
    }
    let (extend, interpolation) = match bprops.as_ref() {
        Some(p) => ([p.extend_x, p.extend_y], p.interpolation),
        None => ([CLAMP, CLAMP], 1),
    };
    let kind = Kind::Image {
        image: Held::new(bitmap),
        source: None,
        extend,
        interpolation,
    };
    create(this, &BITMAP_BRUSH_VT, props, kind, out)
}

/// ID2D1DeviceContext::CreateBitmapBrush (emplacement 66) : mêmes
/// propriétés, interpolation de D2D1_INTERPOLATION_MODE.
pub(super) unsafe extern "win64" fn create_bitmap_brush1(
    this: *mut TargetObj,
    bitmap: *mut c_void,
    bprops: *const BitmapBrushProps,
    props: *const BrushProps,
    out: *mut *mut c_void,
) -> i32 {
    create_bitmap_brush(this, bitmap, bprops, props, out)
}

/// ID2D1DeviceContext::CreateImageBrush (emplacement 65).
pub(super) unsafe extern "win64" fn create_image_brush(
    this: *mut TargetObj,
    image: *mut c_void,
    iprops: *const ImageBrushProps,
    props: *const BrushProps,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(ip) = iprops.as_ref() else {
        return E_INVALIDARG;
    };
    if !image.is_null() && !is_image(image) {
        crate::log_warn!("d2d1", "CreateImageBrush : image étrangère, pinceau vide");
    }
    let kind = Kind::Image {
        image: Held::new(image),
        source: Some(ip.source),
        extend: [ip.extend_x, ip.extend_y],
        interpolation: ip.interpolation,
    };
    create(this, &IMAGE_BRUSH_VT, props, kind, out)
}

/// ID2D1RenderTarget::CreateLinearGradientBrush (emplacement 10).
pub(super) unsafe extern "win64" fn create_linear_brush(
    this: *mut TargetObj,
    lprops: *const [f32; 4],
    props: *const BrushProps,
    stops: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(l) = lprops.as_ref() else {
        return E_INVALIDARG;
    };
    if v11::gradient_of(stops).is_none() {
        return E_INVALIDARG;
    }
    let kind = Kind::Linear {
        start: (l[0], l[1]),
        end: (l[2], l[3]),
        stops: Held::new(stops),
    };
    create(this, &LINEAR_VT, props, kind, out)
}

/// ID2D1RenderTarget::CreateRadialGradientBrush (emplacement 11).
pub(super) unsafe extern "win64" fn create_radial_brush(
    this: *mut TargetObj,
    rprops: *const RadialProps,
    props: *const BrushProps,
    stops: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(r) = rprops.as_ref() else {
        return E_INVALIDARG;
    };
    if v11::gradient_of(stops).is_none() {
        return E_INVALIDARG;
    }
    let kind = Kind::Radial {
        center: (r.center[0], r.center[1]),
        offset: (r.offset[0], r.offset[1]),
        rx: r.rx,
        ry: r.ry,
        stops: Held::new(stops),
    };
    create(this, &RADIAL_VT, props, kind, out)
}

// --- ID2D1BitmapBrush1 et ID2D1ImageBrush ------------------------------------------------

unsafe extern "win64" fn bitmap_brush_qi(
    this: *mut BrushObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_D2D1_RESOURCE,
            IID_D2D1_BRUSH,
            IID_D2D1_BITMAP_BRUSH,
            IID_D2D1_BITMAP_BRUSH1,
        ],
    )
}
unsafe extern "win64" fn image_brush_qi(
    this: *mut BrushObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_BRUSH, IID_D2D1_IMAGE_BRUSH],
    )
}

macro_rules! image_field {
    ($set:ident, $get:ident, |$k:ident| $field:expr) => {
        unsafe extern "win64" fn $set(this: *mut c_void, v: u32) {
            if let Kind::Image {
                extend,
                interpolation,
                ..
            } = &mut *kind(this)
            {
                let $k = (extend, interpolation);
                *$field = v;
            }
        }
        unsafe extern "win64" fn $get(this: *mut c_void) -> u32 {
            if let Kind::Image {
                extend,
                interpolation,
                ..
            } = &mut *kind(this)
            {
                let $k = (extend, interpolation);
                return *$field;
            }
            0
        }
    };
}
image_field!(set_extend_x, get_extend_x, |k| &mut k.0[0]);
image_field!(set_extend_y, get_extend_y, |k| &mut k.0[1]);
image_field!(set_interpolation, get_interpolation, |k| k.1);

unsafe extern "win64" fn set_image(this: *mut c_void, image: *mut c_void) {
    if let Kind::Image { image: held, .. } = &mut *kind(this) {
        *held = Held::new(image);
    }
}
unsafe extern "win64" fn get_image(this: *mut c_void, out: *mut *mut c_void) {
    if let Kind::Image { image, .. } = &*kind(this) {
        image.give(out);
    } else if !out.is_null() {
        *out = std::ptr::null_mut();
    }
}
unsafe extern "win64" fn set_source(this: *mut c_void, r: *const RectF) {
    if let Kind::Image { source, .. } = &mut *kind(this) {
        *source = r.as_ref().copied();
    }
}
unsafe extern "win64" fn get_source(this: *mut c_void, out: *mut RectF) {
    if out.is_null() {
        return;
    }
    if let Kind::Image { source, .. } = &*kind(this) {
        *out = source.unwrap_or_default();
    }
}

/// ID2D1BitmapBrush1 : ID2D1Brush (8) + BitmapBrush (8) + 2.
static BITMAP_BRUSH_VT: VTable<18> = VTable([
    bitmap_brush_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    set_transform as *const (),
    brush_get_opacity as *const (),
    get_transform as *const (),
    set_extend_x as *const (),
    set_extend_y as *const (),
    set_interpolation as *const (),
    set_image as *const (), // SetBitmap
    get_extend_x as *const (),
    get_extend_y as *const (),
    get_interpolation as *const (),
    get_image as *const (),         // GetBitmap
    set_interpolation as *const (), // SetInterpolationMode1
    get_interpolation as *const (), // GetInterpolationMode1
]);

/// ID2D1ImageBrush : ID2D1Brush (8) + 10.
static IMAGE_BRUSH_VT: VTable<18> = VTable([
    image_brush_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    set_transform as *const (),
    brush_get_opacity as *const (),
    get_transform as *const (),
    set_image as *const (),
    set_extend_x as *const (),
    set_extend_y as *const (),
    set_interpolation as *const (),
    set_source as *const (),
    get_image as *const (),
    get_extend_x as *const (),
    get_extend_y as *const (),
    get_interpolation as *const (),
    get_source as *const (),
]);

// --- Dégradés ------------------------------------------------------------------------------

unsafe extern "win64" fn linear_qi(
    this: *mut BrushObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_BRUSH, IID_D2D1_LINEAR_BRUSH],
    )
}
unsafe extern "win64" fn radial_qi(
    this: *mut BrushObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_D2D1_RESOURCE, IID_D2D1_BRUSH, IID_D2D1_RADIAL_BRUSH],
    )
}

/// Point d'un dégradé : 0 début ou centre, 1 fin ou décalage du foyer.
unsafe fn with_point<R>(this: *mut c_void, which: u32, f: impl FnOnce(&mut Pt) -> R) -> Option<R> {
    match &mut *kind(this) {
        Kind::Linear { start, end, .. } => Some(f(if which == 0 { start } else { end })),
        Kind::Radial { center, offset, .. } => Some(f(if which == 0 { center } else { offset })),
        _ => None,
    }
}
/// D2D1_POINT_2F reçu par valeur.
unsafe extern "win64" fn set_point0(this: *mut c_void, p: u64) {
    with_point(this, 0, |f| *f = point(p));
}
unsafe extern "win64" fn set_point1(this: *mut c_void, p: u64) {
    with_point(this, 1, |f| *f = point(p));
}
/// Les D2D1_POINT_2F rendus le sont par adresse cachée (convention MSVC
/// des méthodes C++).
unsafe extern "win64" fn get_point0(this: *mut c_void, out: *mut [f32; 2]) -> *mut [f32; 2] {
    if !out.is_null() {
        with_point(this, 0, |f| *out = [f.0, f.1]);
    }
    out
}
unsafe extern "win64" fn get_point1(this: *mut c_void, out: *mut [f32; 2]) -> *mut [f32; 2] {
    if !out.is_null() {
        with_point(this, 1, |f| *out = [f.0, f.1]);
    }
    out
}
unsafe extern "win64" fn get_stops(this: *mut c_void, out: *mut *mut c_void) {
    match &*kind(this) {
        Kind::Linear { stops, .. } | Kind::Radial { stops, .. } => stops.give(out),
        _ if !out.is_null() => *out = std::ptr::null_mut(),
        _ => {}
    }
}
unsafe extern "win64" fn set_rx(this: *mut c_void, v: f32) {
    if let Kind::Radial { rx, .. } = &mut *kind(this) {
        *rx = v;
    }
}
unsafe extern "win64" fn set_ry(this: *mut c_void, v: f32) {
    if let Kind::Radial { ry, .. } = &mut *kind(this) {
        *ry = v;
    }
}
unsafe extern "win64" fn get_rx(this: *mut c_void) -> f32 {
    match &*kind(this) {
        Kind::Radial { rx, .. } => *rx,
        _ => 0.0,
    }
}
unsafe extern "win64" fn get_ry(this: *mut c_void) -> f32 {
    match &*kind(this) {
        Kind::Radial { ry, .. } => *ry,
        _ => 0.0,
    }
}

/// ID2D1LinearGradientBrush : ID2D1Brush (8) + 5.
static LINEAR_VT: VTable<13> = VTable([
    linear_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    set_transform as *const (),
    brush_get_opacity as *const (),
    get_transform as *const (),
    set_point0 as *const (), // SetStartPoint
    set_point1 as *const (), // SetEndPoint
    get_point0 as *const (), // GetStartPoint
    get_point1 as *const (), // GetEndPoint
    get_stops as *const (),
]);

/// ID2D1RadialGradientBrush : ID2D1Brush (8) + 9.
static RADIAL_VT: VTable<17> = VTable([
    radial_qi as *const (),
    add_ref::<Res<Brush>> as *const (),
    release::<Res<Brush>> as *const (),
    get_factory::<Brush> as *const (),
    brush_set_opacity as *const (),
    set_transform as *const (),
    brush_get_opacity as *const (),
    get_transform as *const (),
    set_point0 as *const (), // SetCenter
    set_point1 as *const (), // SetGradientOriginOffset
    set_rx as *const (),
    set_ry as *const (),
    get_point0 as *const (), // GetCenter
    get_point1 as *const (), // GetGradientOriginOffset
    get_rx as *const (),
    get_ry as *const (),
    get_stops as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_have_windows_sizes() {
        assert_eq!(BITMAP_BRUSH_VT.0.len(), 18);
        assert_eq!(IMAGE_BRUSH_VT.0.len(), 18);
        assert_eq!(LINEAR_VT.0.len(), 13);
        assert_eq!(RADIAL_VT.0.len(), 17);
        assert_eq!(std::mem::size_of::<BrushProps>(), 28);
        assert_eq!(std::mem::size_of::<BitmapBrushProps>(), 12);
        assert_eq!(std::mem::size_of::<ImageBrushProps>(), 28);
        assert_eq!(std::mem::size_of::<RadialProps>(), 24);
    }

    #[test]
    fn extend_modes_fold_indices() {
        assert_eq!(extend_index(-1, 0, 4, CLAMP), 0);
        assert_eq!(extend_index(5, 0, 4, CLAMP), 3);
        assert_eq!(extend_index(5, 0, 4, WRAP), 1);
        assert_eq!(extend_index(-1, 0, 4, WRAP), 3);
        assert_eq!(extend_index(4, 0, 4, MIRROR), 3);
        assert_eq!(extend_index(-1, 0, 4, MIRROR), 0);
    }

    #[test]
    fn radial_position_is_distance_from_a_centred_focus() {
        assert!((radial_t(0.5, 0.0, 0.0, 0.0) - 0.5).abs() < 1e-5);
        assert!((radial_t(0.0, -1.0, 0.0, 0.0) - 1.0).abs() < 1e-5);
        // Foyer décalé : le foyer vaut 0, le bord vaut 1.
        assert!(radial_t(0.5, 0.0, 0.5, 0.0).abs() < 1e-5);
        assert!((radial_t(-1.0, 0.0, 0.5, 0.0) - 1.0).abs() < 1e-5);
    }
}
