//! gdiplus.dll : l'API « plate » de GDI+ (fonctions `Gdip*`), écrite pour
//! Winlux d'après la documentation publique de Microsoft Learn.
//!
//! - Images : chargement par les décodeurs maison (`crate::imaging`),
//!   bitmaps en mémoire (BGRA prémultiplié), `LockBits`, HBITMAP/HICON,
//!   enregistrement en PNG et BMP, trames des GIF animés.
//! - Dessin : rectangles, ellipses, arcs, polygones, Bézier, chemins,
//!   images, texte ; anticrénelage et transformations par le rastériseur
//!   de Direct2D (`d2d_raster`). Sur un HDC, chaque primitive est rendue
//!   dans une petite image transparente puis fondue dans le contexte.
//! - Pinceaux unis, dégradés linéaires, textures, hachures (couleur avant).
//!
//! Les objets rendus au programme (GpGraphics*, GpBitmap*…) sont des
//! poignées opaques tenues dans une table.

use super::d2d_raster::{Canvas, Clip, FillRule, Matrix, Path, Pt};
use super::gui::{self, FontSpec};
use super::icons;
use super::{lock, lookup, wstr, Exports};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[path = "gdiplus_text.rs"]
mod text;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                // SAFETY: adresse de la table d'exports de Winlux, signature
                // de l'API documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// --- Codes de retour (Status) ---------------------------------------------------------

pub(crate) const OK: i32 = 0;
pub(crate) const GENERIC_ERROR: i32 = 1;
pub(crate) const INVALID_PARAMETER: i32 = 2;
const OUT_OF_MEMORY: i32 = 3;
const INSUFFICIENT_BUFFER: i32 = 5;
const NOT_IMPLEMENTED: i32 = 6;
const WIN32_ERROR: i32 = 7;
const WRONG_STATE: i32 = 8;
const FILE_NOT_FOUND: i32 = 10;
const UNKNOWN_IMAGE_FORMAT: i32 = 13;
const PROPERTY_NOT_FOUND: i32 = 19;

// --- Formats de pixels ------------------------------------------------------------

pub(crate) const PF_RGB24: u32 = 0x0002_1808;
pub(crate) const PF_RGB32: u32 = 0x0002_2009;
pub(crate) const PF_ARGB32: u32 = 0x0026_200A;
pub(crate) const PF_PARGB32: u32 = 0x000E_200B;

fn bytes_per_pixel(f: u32) -> Option<usize> {
    match f {
        PF_RGB24 => Some(3),
        PF_RGB32 | PF_ARGB32 | PF_PARGB32 => Some(4),
        _ => None,
    }
}

// --- Objets ----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Raw {
    MemoryBmp,
    Bmp,
    Jpeg,
    Png,
    Gif,
    Tiff,
    Icon,
}

impl Raw {
    fn guid(self) -> [u8; 16] {
        let d1: u32 = match self {
            Raw::MemoryBmp => 0xb96b_3caa,
            Raw::Bmp => 0xb96b_3cab,
            Raw::Jpeg => 0xb96b_3cae,
            Raw::Png => 0xb96b_3caf,
            Raw::Gif => 0xb96b_3cb0,
            Raw::Tiff => 0xb96b_3cb1,
            Raw::Icon => 0xb96b_3cb5,
        };
        guid(
            d1,
            0x0728,
            0x11d3,
            [0x9d, 0x7b, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
        )
    }
}

fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> [u8; 16] {
    let mut g = [0u8; 16];
    g[..4].copy_from_slice(&d1.to_le_bytes());
    g[4..6].copy_from_slice(&d2.to_le_bytes());
    g[6..8].copy_from_slice(&d3.to_le_bytes());
    g[8..].copy_from_slice(&d4);
    g
}

/// Tampon d'un programme partagé par une bitmap (`GdipCreateBitmapFromScan0`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct External {
    ptr: usize,
    stride: isize,
}

#[derive(Clone, Debug)]
pub(crate) struct Lock {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    format: u32,
    mode: u32,
    buf: Vec<u8>,
    user: Option<External>,
}

#[derive(Clone, Debug)]
pub(crate) struct Bitmap {
    pub(crate) w: u32,
    pub(crate) h: u32,
    /// BGRA prémultiplié, lignes de w*4 octets.
    pub(crate) px: Vec<u8>,
    pub(crate) format: u32,
    raw: Raw,
    dpi: (f32, f32),
    /// Autres trames (GIF animé, TIFF multipage) et délais en 1/100 s.
    frames: Vec<Vec<u8>>,
    delays: Vec<u32>,
    frame: usize,
    animated: bool,
    external: Option<External>,
    lock: Option<Lock>,
}

impl Bitmap {
    pub(crate) fn new(w: u32, h: u32, format: u32) -> Bitmap {
        Bitmap {
            w,
            h,
            px: vec![0; w as usize * h as usize * 4],
            format,
            raw: Raw::MemoryBmp,
            dpi: (96.0, 96.0),
            frames: Vec::new(),
            delays: Vec::new(),
            frame: 0,
            animated: false,
            external: None,
            lock: None,
        }
    }

    /// Depuis des pixels RGBA non prémultipliés.
    fn from_rgba(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut px = vec![0u8; w as usize * h as usize * 4];
        for (d, s) in px
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(rgba.as_chunks::<4>().0.iter())
        {
            let a = s[3] as u32;
            d[0] = (s[2] as u32 * a / 255) as u8;
            d[1] = (s[1] as u32 * a / 255) as u8;
            d[2] = (s[0] as u32 * a / 255) as u8;
            d[3] = s[3];
        }
        px
    }

    fn has_alpha(&self) -> bool {
        self.px.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    }

    /// ARGB non prémultiplié du pixel (x, y).
    fn argb(&self, x: u32, y: u32) -> u32 {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        unpremul(&self.px[i..i + 4])
    }

    pub(crate) fn canvas(&mut self) -> Canvas<'_> {
        Canvas {
            w: self.w,
            h: self.h,
            stride: self.w as usize * 4,
            px: &mut self.px,
        }
    }

    /// Relit le tampon partagé avec le programme.
    unsafe fn sync_in(&mut self) {
        if let Some(e) = self.external {
            let px = read_pixels(e.ptr, e.stride, self.w, self.h, self.format);
            self.px = px;
        }
    }

    /// Recopie les pixels dans le tampon partagé avec le programme.
    unsafe fn sync_out(&self) {
        if let Some(e) = self.external {
            write_pixels(
                &self.px,
                e.ptr,
                e.stride,
                0,
                0,
                self.w,
                self.h,
                self.w,
                self.format,
            );
        }
    }
}

pub(crate) fn unpremul(p: &[u8]) -> u32 {
    let a = p[3] as u32;
    if a == 0 {
        return 0;
    }
    let un = |v: u8| ((v as u32 * 255 + a / 2) / a).min(255);
    (a << 24) | (un(p[2]) << 16) | (un(p[1]) << 8) | un(p[0])
}

fn premul_argb(c: u32) -> [u8; 4] {
    let a = c >> 24;
    let m = |v: u32| ((v & 0xFF) * a / 255) as u8;
    [m(c), m(c >> 8), m(c >> 16), a as u8]
}

/// Lit des pixels au format `format` et les rend en BGRA prémultiplié.
unsafe fn read_pixels(ptr: usize, stride: isize, w: u32, h: u32, format: u32) -> Vec<u8> {
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let Some(bpp) = bytes_per_pixel(format) else {
        return px;
    };
    for y in 0..h as usize {
        let row = (ptr as isize + y as isize * stride) as *const u8;
        for x in 0..w as usize {
            let s = row.add(x * bpp);
            let d = &mut px[(y * w as usize + x) * 4..][..4];
            let (b, g, r) = (*s, *s.add(1), *s.add(2));
            match format {
                PF_ARGB32 => {
                    let a = *s.add(3) as u32;
                    d.copy_from_slice(&premul_argb(
                        (a << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
                    ));
                }
                PF_PARGB32 => d.copy_from_slice(std::slice::from_raw_parts(s, 4)),
                _ => d.copy_from_slice(&[b, g, r, 255]),
            }
        }
    }
    px
}

/// Écrit la zone (x, y, w, h) de `px` (largeur `pw`) au format `format`.
#[allow(clippy::too_many_arguments)]
unsafe fn write_pixels(
    px: &[u8],
    ptr: usize,
    stride: isize,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    pw: u32,
    format: u32,
) {
    let Some(bpp) = bytes_per_pixel(format) else {
        return;
    };
    for y in 0..h as usize {
        let row = (ptr as isize + y as isize * stride) as *mut u8;
        for x in 0..w as usize {
            let s = &px[((y0 as usize + y) * pw as usize + x0 as usize + x) * 4..][..4];
            let d = row.add(x * bpp);
            match format {
                PF_ARGB32 => {
                    let c = unpremul(s);
                    *d = c as u8;
                    *d.add(1) = (c >> 8) as u8;
                    *d.add(2) = (c >> 16) as u8;
                    *d.add(3) = (c >> 24) as u8;
                }
                PF_PARGB32 => std::ptr::copy_nonoverlapping(s.as_ptr(), d, 4),
                _ => {
                    // Sans alpha : composé sur du noir, comme GDI+.
                    *d = s[0];
                    *d.add(1) = s[1];
                    *d.add(2) = s[2];
                    if bpp == 4 {
                        *d.add(3) = 255;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Brush {
    Solid(u32),
    Linear {
        p1: Pt,
        p2: Pt,
        c1: u32,
        c2: u32,
        wrap: i32,
        m: Matrix,
    },
    Hatch {
        style: i32,
        fore: u32,
        back: u32,
    },
    Texture {
        img: usize,
        wrap: i32,
        m: Matrix,
    },
    PathGradient {
        center: u32,
        surround: u32,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Pen {
    brush: Brush,
    width: f32,
    unit: i32,
    dash: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Font {
    pub(crate) family: String,
    pub(crate) size: f32,
    pub(crate) style: i32,
    pub(crate) unit: i32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StringFormat {
    pub(crate) flags: i32,
    pub(crate) align: i32,
    pub(crate) line_align: i32,
    pub(crate) trimming: i32,
    pub(crate) hotkey: i32,
    pub(crate) typographic: bool,
    pub(crate) ranges: Vec<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub(crate) struct GPath {
    path: Path,
    fill: FillRule,
    /// Figure ouverte : la prochaine ligne s'y raccorde.
    open: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Attributes {
    key: Option<(u32, u32)>,
    matrix: Option<[[f32; 5]; 5]>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Target {
    Hdc(usize),
    Bitmap(usize),
}

#[derive(Clone, Debug)]
struct State {
    m: Matrix,
    clip: Option<Clip>,
    smoothing: i32,
    offset: i32,
    interp: i32,
    text_hint: i32,
    unit: i32,
    scale: f32,
    compositing: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Graphics {
    target: Target,
    hwnd: usize,
    st: State,
    saved: Vec<(u32, State)>,
    dc_lent: Option<(usize, usize, usize)>,
}

#[derive(Clone, Debug)]
pub(crate) enum Obj {
    Graphics(Box<Graphics>),
    Image(Box<Bitmap>),
    Brush(Brush),
    Pen(Pen),
    Font(Font),
    Family(String),
    Format(StringFormat),
    Path(GPath),
    Matrix(Matrix),
    /// Rectangle (gauche, haut, droite, bas) ; `None` : région infinie.
    Region(Option<Clip>),
    Attributes(Attributes),
    Collection,
}

struct Table {
    next: usize,
    objs: HashMap<usize, Obj>,
}

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| {
        Mutex::new(Table {
            next: 0x6d70_0000,
            objs: HashMap::new(),
        })
    })
}

pub(crate) fn add(o: Obj) -> usize {
    let mut t = lock(table());
    t.next += 0x10;
    let h = t.next;
    t.objs.insert(h, o);
    h
}

fn get(h: usize) -> Option<Obj> {
    lock(table()).objs.get(&h).cloned()
}

fn with<R>(h: usize, f: impl FnOnce(&mut Obj) -> R) -> Option<R> {
    lock(table()).objs.get_mut(&h).map(f)
}

fn remove(h: usize) -> i32 {
    match lock(table()).objs.remove(&h) {
        Some(_) => OK,
        None => INVALID_PARAMETER,
    }
}

unsafe fn out<T>(p: *mut T, v: T) -> i32 {
    if p.is_null() {
        return INVALID_PARAMETER;
    }
    p.write_unaligned(v);
    OK
}

fn graphics(h: usize) -> Option<Graphics> {
    match get(h)? {
        Obj::Graphics(g) => Some(*g),
        _ => None,
    }
}

fn with_graphics<R>(h: usize, f: impl FnOnce(&mut Graphics) -> R) -> Option<R> {
    with(h, |o| match o {
        Obj::Graphics(g) => Some(f(g)),
        _ => None,
    })
    .flatten()
}

fn with_bitmap<R>(h: usize, f: impl FnOnce(&mut Bitmap) -> R) -> Option<R> {
    with(h, |o| match o {
        Obj::Image(b) => Some(f(b)),
        _ => None,
    })
    .flatten()
}

fn bitmap(h: usize) -> Option<Bitmap> {
    match get(h)? {
        Obj::Image(b) => Some(*b),
        _ => None,
    }
}

fn brush(h: usize) -> Option<Brush> {
    match get(h)? {
        Obj::Brush(b) => Some(b),
        _ => None,
    }
}

fn pen(h: usize) -> Option<Pen> {
    match get(h)? {
        Obj::Pen(p) => Some(p),
        _ => None,
    }
}

// --- Démarrage ---------------------------------------------------------------------

unsafe extern "win64" fn hook_stub(_token: *mut usize) -> i32 {
    OK
}
unsafe extern "win64" fn unhook_stub(_token: usize) {}

/// GdiplusStartup(token, input, output).
unsafe extern "win64" fn startup(token: *mut usize, input: *const u8, output: *mut usize) -> i32 {
    if token.is_null() || input.is_null() {
        return INVALID_PARAMETER;
    }
    let version = (input as *const u32).read_unaligned();
    if version == 0 || version > 3 {
        return 17; // UnsupportedGdiplusVersion
    }
    let suppress = (input.add(16) as *const i32).read_unaligned() != 0;
    if suppress && !output.is_null() {
        *output = hook_stub as *const () as usize;
        *output.add(1) = unhook_stub as *const () as usize;
    }
    *token = 0x6d70_0001;
    OK
}

unsafe extern "win64" fn shutdown(_token: usize) {}

unsafe extern "win64" fn gdip_alloc(size: usize) -> *mut u8 {
    libc::malloc(size.max(1)) as *mut u8
}

unsafe extern "win64" fn gdip_free(p: *mut u8) {
    libc::free(p as *mut libc::c_void)
}

unsafe extern "win64" fn not_implemented() -> i32 {
    NOT_IMPLEMENTED
}

unsafe extern "win64" fn ok_stub() -> i32 {
    OK
}

// --- Images : création et chargement --------------------------------------------

fn decoded(img: crate::imaging::Image) -> Option<Bitmap> {
    let raw = match img.format {
        f if f.eq_ignore_ascii_case("png") => Raw::Png,
        f if f.eq_ignore_ascii_case("jpeg") || f.eq_ignore_ascii_case("jpg") => Raw::Jpeg,
        f if f.eq_ignore_ascii_case("gif") => Raw::Gif,
        f if f.eq_ignore_ascii_case("bmp") => Raw::Bmp,
        f if f.eq_ignore_ascii_case("tiff") || f.eq_ignore_ascii_case("tif") => Raw::Tiff,
        f if f.eq_ignore_ascii_case("ico") || f.eq_ignore_ascii_case("icon") => Raw::Icon,
        _ => Raw::Png,
    };
    let animated = img.animated;
    let mut rasters = Vec::new();
    let mut delays = Vec::new();
    for p in img.pages {
        if let crate::imaging::Content::Raster(_) = p.content {
            delays.push(p.delay_ms / 10);
            rasters.push(p.into_raster());
        }
    }
    let first = rasters.first()?;
    let (w, h) = (first.w, first.h);
    let mut b = Bitmap::new(w, h, PF_ARGB32);
    b.raw = raw;
    b.animated = animated;
    b.frames = rasters
        .iter()
        .filter(|r| (r.w, r.h) == (w, h))
        .map(|r| Bitmap::from_rgba(w, h, &r.px))
        .collect();
    b.delays = delays;
    b.px = b.frames[0].clone();
    let opaque = !b.has_alpha();
    b.format = match raw {
        Raw::Jpeg if opaque => PF_RGB24,
        _ if opaque && raw != Raw::Png => PF_RGB32,
        _ => PF_ARGB32,
    };
    if b.frames.len() < 2 {
        b.frames.clear();
    }
    Some(b)
}

fn load_bytes(bytes: &[u8], ext: &str) -> Result<Bitmap, i32> {
    let img = crate::imaging::decode_bytes(bytes, ext).map_err(|_| UNKNOWN_IMAGE_FORMAT)?;
    decoded(img).ok_or(UNKNOWN_IMAGE_FORMAT)
}

unsafe fn load_file(name: *const u16, out_img: *mut usize) -> i32 {
    if name.is_null() || out_img.is_null() {
        return INVALID_PARAMETER;
    }
    let win = wstr(name);
    let path = crate::paths::to_linux(&win);
    let Ok(bytes) = std::fs::read(&path) else {
        return FILE_NOT_FOUND;
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    match load_bytes(&bytes, &ext) {
        Ok(b) => out(out_img, add(Obj::Image(Box::new(b)))),
        Err(e) => {
            crate::log_warn!("gdiplus", "image illisible : {win}");
            e
        }
    }
}

unsafe extern "win64" fn load_image_from_file(name: *const u16, img: *mut usize) -> i32 {
    load_file(name, img)
}

/// Lit tout le flux IStream à partir de sa position courante.
unsafe fn read_stream(stream: usize) -> Option<Vec<u8>> {
    if stream == 0 {
        return None;
    }
    let vt = *(stream as *const *const usize);
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(*vt.add(3));
    let mut data = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        let hr = read(stream, buf.as_mut_ptr(), buf.len() as u32, &mut n);
        if hr < 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
        if n == 0 || data.len() > 512 << 20 {
            break;
        }
    }
    Some(data)
}

unsafe extern "win64" fn load_image_from_stream(stream: usize, img: *mut usize) -> i32 {
    if img.is_null() {
        return INVALID_PARAMETER;
    }
    let Some(bytes) = read_stream(stream) else {
        return INVALID_PARAMETER;
    };
    match load_bytes(&bytes, "") {
        Ok(b) => out(img, add(Obj::Image(Box::new(b)))),
        Err(e) => e,
    }
}

unsafe extern "win64" fn create_bitmap_from_scan0(
    w: i32,
    h: i32,
    stride: i32,
    format: u32,
    scan0: usize,
    bmp: *mut usize,
) -> i32 {
    if w <= 0 || h <= 0 || bmp.is_null() || (w as i64 * h as i64) > (1 << 28) {
        return INVALID_PARAMETER;
    }
    let mut b = Bitmap::new(w as u32, h as u32, format);
    if bytes_per_pixel(format).is_none() {
        // Formats indexés ou 16 bits : tenus en 32 bits, sans partage.
        b.format = format;
    } else if scan0 != 0 {
        b.external = Some(External {
            ptr: scan0,
            stride: stride as isize,
        });
        b.sync_in();
    } else if format != PF_ARGB32 && format != PF_PARGB32 {
        // Formats sans alpha : noir opaque au départ.
        for p in b.px.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
    }
    out(bmp, add(Obj::Image(Box::new(b))))
}

unsafe extern "win64" fn create_bitmap_from_graphics(
    w: i32,
    h: i32,
    _g: usize,
    bmp: *mut usize,
) -> i32 {
    create_bitmap_from_scan0(w, h, 0, PF_PARGB32, 0, bmp)
}

/// Pixels d'une HBITMAP de Winlux (COLORREF + alpha éventuel).
fn hbitmap_pixels(hbm: usize, with_alpha: bool) -> Option<Bitmap> {
    let (w, h, px, has_alpha) = gui::bitmap_pixels(hbm)?;
    let mut b = Bitmap::new(w as u32, h as u32, PF_RGB32);
    for (d, &p) in b.px.as_chunks_mut::<4>().0.iter_mut().zip(px.iter()) {
        let a = if with_alpha && has_alpha {
            (p >> 24) as u8
        } else {
            255
        };
        let c = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
        if a == 255 {
            d.copy_from_slice(&[c[0], c[1], c[2], 255]);
        } else {
            // GDI tient l'alpha prémultiplié dans les DIB 32 bits.
            d.copy_from_slice(&[c[0].min(a), c[1].min(a), c[2].min(a), a]);
        }
    }
    if with_alpha && has_alpha {
        b.format = PF_ARGB32;
    }
    Some(b)
}

unsafe extern "win64" fn create_bitmap_from_hbitmap(
    hbm: usize,
    _pal: usize,
    bmp: *mut usize,
) -> i32 {
    match hbitmap_pixels(hbm, false) {
        Some(b) => out(bmp, add(Obj::Image(Box::new(b)))),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn create_bitmap_from_hicon(hicon: usize, bmp: *mut usize) -> i32 {
    // ICONINFO : fIcon, xHotspot, yHotspot, hbmMask (8), hbmColor (16).
    let mut info = [0usize; 4];
    let ok = api!(
        "user32",
        "GetIconInfo",
        fn(usize, *mut usize) -> i32,
        hicon,
        info.as_mut_ptr()
    )
    .unwrap_or(0);
    if ok == 0 || info[3] == 0 {
        return WIN32_ERROR;
    }
    let r = match hbitmap_pixels(info[3], true) {
        Some(mut b) => {
            b.format = PF_ARGB32;
            if !b.has_alpha() {
                // Pas d'alpha : le masque monochrome dit ce qui est transparent.
                if let Some((mw, _, mask, _)) = gui::bitmap_pixels(info[2]) {
                    for (i, p) in b.px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (x, y) = (i % b.w as usize, i / b.w as usize);
                        if mask.get(y * mw + x).is_some_and(|&m| m & 0xFF_FFFF != 0) {
                            p.copy_from_slice(&[0, 0, 0, 0]);
                        }
                    }
                }
            }
            out(bmp, add(Obj::Image(Box::new(b))))
        }
        None => INVALID_PARAMETER,
    };
    for h in [info[2], info[3]] {
        api!("gdi32", "DeleteObject", fn(usize) -> i32, h);
    }
    r
}

/// GdipCreateBitmapFromGdiDib(BITMAPINFO*, bits, bitmap).
unsafe extern "win64" fn create_bitmap_from_gdi_dib(
    info: *const u8,
    bits: *const u8,
    bmp: *mut usize,
) -> i32 {
    if info.is_null() || bits.is_null() {
        return INVALID_PARAMETER;
    }
    let w = (info.add(4) as *const i32).read_unaligned();
    let hh = (info.add(8) as *const i32).read_unaligned();
    let bpp = (info.add(14) as *const u16).read_unaligned();
    let (h, top_down) = (hh.unsigned_abs(), hh < 0);
    let format = match bpp {
        24 => PF_RGB24,
        32 => PF_RGB32,
        _ => return NOT_IMPLEMENTED,
    };
    if w <= 0 || h == 0 {
        return INVALID_PARAMETER;
    }
    let stride = ((w as usize * bpp as usize / 8) + 3) & !3;
    let (start, step) = if top_down {
        (bits as isize, stride as isize)
    } else {
        (
            bits as isize + (h as isize - 1) * stride as isize,
            -(stride as isize),
        )
    };
    let mut b = Bitmap::new(w as u32, h, format);
    b.px = read_pixels(start as usize, step, w as u32, h, format);
    out(bmp, add(Obj::Image(Box::new(b))))
}

unsafe extern "win64" fn create_bitmap_from_resource(
    hinst: usize,
    name: *const u16,
    bmp: *mut usize,
) -> i32 {
    let hbm = api!(
        "user32",
        "LoadBitmapW",
        fn(usize, *const u16) -> usize,
        hinst,
        name
    )
    .unwrap_or(0);
    if hbm == 0 {
        return WIN32_ERROR;
    }
    let r = create_bitmap_from_hbitmap(hbm, 0, bmp);
    api!("gdi32", "DeleteObject", fn(usize) -> i32, hbm);
    r
}

/// Crée une DIB 32 bits de haut en bas ; renvoie (HBITMAP, pixels).
unsafe fn create_dib(w: u32, h: u32) -> Option<(usize, *mut u8)> {
    let mut bmi = [0u8; 44];
    bmi[..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&(w as i32).to_le_bytes());
    bmi[8..12].copy_from_slice(&(-(h as i32)).to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&32u16.to_le_bytes());
    let mut bits: *mut u8 = std::ptr::null_mut();
    let hbm = api!(
        "gdi32",
        "CreateDIBSection",
        fn(usize, *const u8, u32, *mut *mut u8, usize, u32) -> usize,
        0,
        bmi.as_ptr(),
        0,
        &mut bits,
        0,
        0
    )?;
    (hbm != 0 && !bits.is_null()).then_some((hbm, bits))
}

unsafe extern "win64" fn create_hbitmap_from_bitmap(
    bmp: usize,
    out_hbm: *mut usize,
    bg: u32,
) -> i32 {
    let Some(mut b) = bitmap(bmp) else {
        return INVALID_PARAMETER;
    };
    b.sync_in();
    let Some((hbm, bits)) = create_dib(b.w, b.h) else {
        return GENERIC_ERROR;
    };
    let bgp = premul_argb(bg);
    let dst = std::slice::from_raw_parts_mut(bits, b.px.len());
    for (d, s) in dst
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(b.px.as_chunks::<4>().0.iter())
    {
        let inv = 255 - s[3] as u32;
        for i in 0..4 {
            d[i] = (s[i] as u32 + bgp[i] as u32 * inv / 255).min(255) as u8;
        }
    }
    out(out_hbm, hbm)
}

unsafe extern "win64" fn create_hicon_from_bitmap(bmp: usize, hicon: *mut usize) -> i32 {
    let Some(b) = bitmap(bmp) else {
        return INVALID_PARAMETER;
    };
    let Some((color, bits)) = create_dib(b.w, b.h) else {
        return GENERIC_ERROR;
    };
    std::ptr::copy_nonoverlapping(b.px.as_ptr(), bits, b.px.len());
    let mask = api!(
        "gdi32",
        "CreateBitmap",
        fn(i32, i32, u32, u32, *const u8) -> usize,
        b.w as i32,
        b.h as i32,
        1,
        1,
        std::ptr::null()
    )
    .unwrap_or(0);
    // ICONINFO : fIcon (4), xHotspot (4), yHotspot (4), pad, hbmMask, hbmColor.
    let mut info = [0usize; 4];
    info[0] = 1;
    info[2] = mask;
    info[3] = color;
    let icon = api!(
        "user32",
        "CreateIconIndirect",
        fn(*const usize) -> usize,
        info.as_ptr()
    )
    .unwrap_or(0);
    for h in [mask, color] {
        api!("gdi32", "DeleteObject", fn(usize) -> i32, h);
    }
    if icon == 0 {
        return WIN32_ERROR;
    }
    out(hicon, icon)
}

unsafe extern "win64" fn dispose_image(img: usize) -> i32 {
    remove(img)
}

unsafe extern "win64" fn clone_image(img: usize, out_img: *mut usize) -> i32 {
    match get(img) {
        Some(Obj::Image(mut b)) => {
            b.sync_in();
            b.external = None;
            b.lock = None;
            out(out_img, add(Obj::Image(b)))
        }
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn clone_bitmap_area(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    format: u32,
    src: usize,
    dst: *mut usize,
) -> i32 {
    clone_bitmap_area_i(x as i32, y as i32, w as i32, h as i32, format, src, dst)
}

unsafe extern "win64" fn clone_bitmap_area_i(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    format: u32,
    src: usize,
    dst: *mut usize,
) -> i32 {
    let Some(mut b) = bitmap(src) else {
        return INVALID_PARAMETER;
    };
    b.sync_in();
    if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > b.w as i32 || y + h > b.h as i32 {
        return INVALID_PARAMETER;
    }
    let mut n = Bitmap::new(
        w as u32,
        h as u32,
        if format == 0 { b.format } else { format },
    );
    for row in 0..h as usize {
        let s = ((y as usize + row) * b.w as usize + x as usize) * 4;
        let d = row * w as usize * 4;
        n.px[d..d + w as usize * 4].copy_from_slice(&b.px[s..s + w as usize * 4]);
    }
    n.raw = b.raw;
    out(dst, add(Obj::Image(Box::new(n))))
}

// --- Images : propriétés ------------------------------------------------------------

unsafe extern "win64" fn get_image_width(img: usize, w: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => out(w, b.w),
        None => INVALID_PARAMETER,
    }
}

/// GdipImageForceValidation (Image.FromStream(…, validateImageData)) :
/// Winlux décode l'image dès son chargement, elle est donc déjà validée.
unsafe extern "win64" fn image_force_validation(img: usize) -> i32 {
    match bitmap(img) {
        Some(_) => OK,
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_height(img: usize, h: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => out(h, b.h),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_dimension(img: usize, w: *mut f32, h: *mut f32) -> i32 {
    match bitmap(img) {
        Some(b) => {
            out(w, b.w as f32);
            out(h, b.h as f32)
        }
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_bounds(img: usize, rect: *mut f32, unit: *mut i32) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    for (i, v) in [0.0, 0.0, b.w as f32, b.h as f32].into_iter().enumerate() {
        *rect.add(i) = v;
    }
    if !unit.is_null() {
        *unit = 2;
    }
    OK
}

unsafe extern "win64" fn get_image_pixel_format(img: usize, f: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => out(f, b.format),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_type(img: usize, t: *mut i32) -> i32 {
    match bitmap(img) {
        Some(_) => out(t, 1),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_flags(img: usize, f: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => {
            let alpha = if b.has_alpha() || b.format == PF_ARGB32 {
                2
            } else {
                0
            };
            out(
                f,
                0x10 | alpha | (0x1_0000 * (b.raw != Raw::MemoryBmp) as u32),
            )
        }
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_image_raw_format(img: usize, g: *mut [u8; 16]) -> i32 {
    match bitmap(img) {
        Some(b) => out(g, b.raw.guid()),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_resolution_x(img: usize, r: *mut f32) -> i32 {
    match bitmap(img) {
        Some(b) => out(r, b.dpi.0),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_resolution_y(img: usize, r: *mut f32) -> i32 {
    match bitmap(img) {
        Some(b) => out(r, b.dpi.1),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn bitmap_set_resolution(img: usize, x: f32, y: f32) -> i32 {
    if x <= 0.0 || y <= 0.0 {
        return INVALID_PARAMETER;
    }
    with_bitmap(img, |b| b.dpi = (x, y)).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn get_palette_size(img: usize, size: *mut i32) -> i32 {
    match bitmap(img) {
        Some(_) => out(size, 12),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_palette(img: usize, pal: *mut u32, size: i32) -> i32 {
    if bitmap(img).is_none() || pal.is_null() || size < 8 {
        return INVALID_PARAMETER;
    }
    *pal = 0;
    *pal.add(1) = 0;
    OK
}

unsafe extern "win64" fn get_pixel(img: usize, x: i32, y: i32, c: *mut u32) -> i32 {
    let r = with_bitmap(img, |b| {
        b.sync_in();
        (x >= 0 && y >= 0 && (x as u32) < b.w && (y as u32) < b.h)
            .then(|| b.argb(x as u32, y as u32))
    })
    .flatten();
    match r {
        Some(v) => out(c, v),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn set_pixel(img: usize, x: i32, y: i32, c: u32) -> i32 {
    with_bitmap(img, |b| {
        if x < 0 || y < 0 || x as u32 >= b.w || y as u32 >= b.h {
            return INVALID_PARAMETER;
        }
        b.sync_in();
        let c = if bytes_per_pixel(b.format).is_some()
            && b.format != PF_ARGB32
            && b.format != PF_PARGB32
        {
            c | 0xFF00_0000
        } else {
            c
        };
        let i = (y as usize * b.w as usize + x as usize) * 4;
        b.px[i..i + 4].copy_from_slice(&premul_argb(c));
        b.sync_out();
        OK
    })
    .unwrap_or(INVALID_PARAMETER)
}

/// GdipBitmapLockBits(bitmap, rect*, flags, format, BitmapData*).
unsafe extern "win64" fn lock_bits(
    img: usize,
    rect: *const i32,
    flags: u32,
    format: u32,
    data: *mut u8,
) -> i32 {
    if data.is_null() {
        return INVALID_PARAMETER;
    }
    with_bitmap(img, |b| {
        if b.lock.is_some() {
            return WRONG_STATE;
        }
        let (x, y, w, h) = if rect.is_null() {
            (0, 0, b.w as i32, b.h as i32)
        } else {
            (*rect, *rect.add(1), *rect.add(2), *rect.add(3))
        };
        if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > b.w as i32 || y + h > b.h as i32 {
            return INVALID_PARAMETER;
        }
        let format = if format == 0 { b.format } else { format };
        let Some(bpp) = bytes_per_pixel(format) else {
            crate::log_warn!(
                "gdiplus",
                "LockBits : format {format:#x} non pris en charge"
            );
            return INVALID_PARAMETER;
        };
        b.sync_in();
        let user = (flags & 4 != 0).then(|| External {
            ptr: (data.add(16) as *const usize).read_unaligned(),
            stride: (data.add(8) as *const i32).read_unaligned() as isize,
        });
        let stride = (w as usize * bpp + 3) & !3;
        let mut lock = Lock {
            x: x as u32,
            y: y as u32,
            w: w as u32,
            h: h as u32,
            format,
            mode: flags,
            buf: Vec::new(),
            user,
        };
        let (ptr, st) = match user {
            Some(u) => (u.ptr, u.stride),
            None => {
                lock.buf = vec![0u8; stride * h as usize];
                (lock.buf.as_mut_ptr() as usize, stride as isize)
            }
        };
        if flags & 1 != 0 || flags & 2 == 0 {
            write_pixels(
                &b.px, ptr, st, x as u32, y as u32, w as u32, h as u32, b.w, format,
            );
        }
        (data as *mut u32).write_unaligned(w as u32);
        (data.add(4) as *mut u32).write_unaligned(h as u32);
        (data.add(8) as *mut i32).write_unaligned(st as i32);
        (data.add(12) as *mut u32).write_unaligned(format);
        (data.add(16) as *mut usize).write_unaligned(ptr);
        (data.add(24) as *mut usize).write_unaligned(0);
        b.lock = Some(lock);
        OK
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn unlock_bits(img: usize, _data: *mut u8) -> i32 {
    with_bitmap(img, |b| {
        let Some(l) = b.lock.take() else {
            return WRONG_STATE;
        };
        if l.mode & 2 != 0 {
            let (ptr, st) = match l.user {
                Some(u) => (u.ptr, u.stride),
                None => (
                    l.buf.as_ptr() as usize,
                    ((l.w as usize * bytes_per_pixel(l.format).unwrap_or(4) + 3) & !3) as isize,
                ),
            };
            let part = read_pixels(ptr, st, l.w, l.h, l.format);
            for row in 0..l.h as usize {
                let d = ((l.y as usize + row) * b.w as usize + l.x as usize) * 4;
                let s = row * l.w as usize * 4;
                b.px[d..d + l.w as usize * 4].copy_from_slice(&part[s..s + l.w as usize * 4]);
            }
            b.sync_out();
        }
        OK
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn rotate_flip(img: usize, kind: i32) -> i32 {
    with_bitmap(img, |b| {
        b.sync_in();
        let (w, h) = (b.w as usize, b.h as usize);
        let rot = kind & 3;
        let flip = kind & 4 != 0;
        let (nw, nh) = if rot % 2 == 1 { (h, w) } else { (w, h) };
        let mut px = vec![0u8; b.px.len()];
        for y in 0..nh {
            for x in 0..nw {
                let x2 = if flip { nw - 1 - x } else { x };
                let (sx, sy) = match rot {
                    1 => (y, h - 1 - x2),
                    2 => (w - 1 - x2, h - 1 - y),
                    3 => (w - 1 - y, x2),
                    _ => (x2, y),
                };
                let s = (sy * w + sx) * 4;
                px[(y * nw + x) * 4..][..4].copy_from_slice(&b.px[s..s + 4]);
            }
        }
        b.px = px;
        b.w = nw as u32;
        b.h = nh as u32;
        b.frames.clear();
        OK
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn get_thumbnail(
    img: usize,
    w: u32,
    h: u32,
    thumb: *mut usize,
    _cb: usize,
    _data: usize,
) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    let (w, h) = if w == 0 || h == 0 { (120, 120) } else { (w, h) };
    let mut n = Bitmap::new(w, h, PF_ARGB32);
    let m = Matrix {
        m11: w as f32 / b.w as f32,
        m22: h as f32 / b.h as f32,
        ..Matrix::IDENTITY
    };
    draw_bitmap_into(
        &mut n,
        &b,
        (0.0, 0.0, b.w as f32, b.h as f32),
        &m,
        None,
        None,
        7,
    );
    out(thumb, add(Obj::Image(Box::new(n))))
}

// --- Trames (GIF animés) et propriétés -------------------------------------------

const FRAME_TIME: [u8; 16] = [
    0x6d, 0xbd, 0xed, 0x6a, 0x6d, 0x3f, 0x8a, 0x41, 0x83, 0xa6, 0x7f, 0x45, 0x22, 0x9d, 0xc8, 0x72,
];
const FRAME_PAGE: [u8; 16] = [
    0x86, 0xdc, 0x62, 0x74, 0x80, 0x61, 0x7c, 0x4e, 0x8e, 0x3f, 0xee, 0x73, 0x33, 0xa7, 0xa4, 0x83,
];

unsafe extern "win64" fn frame_dims_count(img: usize, n: *mut u32) -> i32 {
    match bitmap(img) {
        Some(_) => out(n, 1),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn frame_dims_list(img: usize, ids: *mut [u8; 16], n: u32) -> i32 {
    match bitmap(img) {
        Some(b) if n >= 1 => out(
            ids,
            if b.raw == Raw::Gif {
                FRAME_TIME
            } else {
                FRAME_PAGE
            },
        ),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn frame_count(img: usize, _dim: *const u8, n: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => out(n, b.frames.len().max(1) as u32),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn select_active_frame(img: usize, _dim: *const u8, index: u32) -> i32 {
    with_bitmap(img, |b| {
        if b.frames.is_empty() {
            return if index == 0 { OK } else { INVALID_PARAMETER };
        }
        let Some(f) = b.frames.get(index as usize) else {
            return INVALID_PARAMETER;
        };
        b.px = f.clone();
        b.frame = index as usize;
        OK
    })
    .unwrap_or(INVALID_PARAMETER)
}

/// Propriétés tenues : délais des trames (0x5100) et nombre de boucles (0x5101).
fn property(b: &Bitmap, id: u32) -> Option<(u16, Vec<u8>)> {
    match id {
        0x5100 if b.raw == Raw::Gif => {
            let n = b.frames.len().max(1);
            let mut v = Vec::with_capacity(n * 4);
            for i in 0..n {
                v.extend_from_slice(&b.delays.get(i).copied().unwrap_or(10).to_le_bytes());
            }
            Some((4, v))
        }
        0x5101 if b.raw == Raw::Gif => Some((3, 0u16.to_le_bytes().to_vec())),
        _ => None,
    }
}

fn property_ids(b: &Bitmap) -> Vec<u32> {
    [0x5100, 0x5101]
        .into_iter()
        .filter(|&i| property(b, i).is_some())
        .collect()
}

unsafe extern "win64" fn property_count(img: usize, n: *mut u32) -> i32 {
    match bitmap(img) {
        Some(b) => out(n, property_ids(&b).len() as u32),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn property_id_list(img: usize, n: u32, list: *mut u32) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    let ids = property_ids(&b);
    if list.is_null() || (n as usize) < ids.len() {
        return INVALID_PARAMETER;
    }
    for (i, id) in ids.iter().enumerate() {
        *list.add(i) = *id;
    }
    OK
}

unsafe extern "win64" fn property_item_size(img: usize, id: u32, size: *mut u32) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    match property(&b, id) {
        Some((_, v)) => out(size, 24 + v.len() as u32),
        None => PROPERTY_NOT_FOUND,
    }
}

unsafe extern "win64" fn property_item(img: usize, id: u32, size: u32, item: *mut u8) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    let Some((ty, v)) = property(&b, id) else {
        return PROPERTY_NOT_FOUND;
    };
    if item.is_null() || (size as usize) < 24 + v.len() {
        return INVALID_PARAMETER;
    }
    (item as *mut u32).write_unaligned(id);
    (item.add(4) as *mut u32).write_unaligned(v.len() as u32);
    (item.add(8) as *mut u16).write_unaligned(ty);
    (item.add(16) as *mut usize).write_unaligned(item.add(24) as usize);
    std::ptr::copy_nonoverlapping(v.as_ptr(), item.add(24), v.len());
    OK
}

unsafe extern "win64" fn property_size(img: usize, total: *mut u32, n: *mut u32) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    let ids = property_ids(&b);
    let size: usize = ids
        .iter()
        .filter_map(|&i| property(&b, i))
        .map(|(_, v)| 24 + v.len())
        .sum();
    out(n, ids.len() as u32);
    out(total, size as u32)
}

unsafe extern "win64" fn all_property_items(img: usize, total: u32, n: u32, items: *mut u8) -> i32 {
    let Some(b) = bitmap(img) else {
        return INVALID_PARAMETER;
    };
    let ids = property_ids(&b);
    if items.is_null() || (n as usize) < ids.len() {
        return INVALID_PARAMETER;
    }
    let mut data = items.add(24 * ids.len());
    let end = items.add(total as usize);
    for (k, &id) in ids.iter().enumerate() {
        let (ty, v) = property(&b, id).unwrap_or((1, Vec::new()));
        if data.add(v.len()) > end {
            return INVALID_PARAMETER;
        }
        let it = items.add(24 * k);
        (it as *mut u32).write_unaligned(id);
        (it.add(4) as *mut u32).write_unaligned(v.len() as u32);
        (it.add(8) as *mut u16).write_unaligned(ty);
        (it.add(16) as *mut usize).write_unaligned(data as usize);
        std::ptr::copy_nonoverlapping(v.as_ptr(), data, v.len());
        data = data.add(v.len());
    }
    OK
}

// --- Codecs ----------------------------------------------------------------------

struct Codec {
    clsid: [u8; 16],
    raw: Raw,
    name: &'static str,
    ext: &'static str,
    mime: &'static str,
    encode: bool,
}

fn codec_clsid(n: u8) -> [u8; 16] {
    guid(
        0x557c_f400 + n as u32,
        0x1a04,
        0x11d3,
        [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    )
}

fn codecs() -> Vec<Codec> {
    let c = |n, raw, name, ext, mime, encode| Codec {
        clsid: codec_clsid(n),
        raw,
        name,
        ext,
        mime,
        encode,
    };
    vec![
        c(0, Raw::Bmp, "BMP", "*.BMP;*.DIB;*.RLE", "image/bmp", true),
        c(
            1,
            Raw::Jpeg,
            "JPEG",
            "*.JPG;*.JPEG;*.JPE;*.JFIF",
            "image/jpeg",
            false,
        ),
        c(2, Raw::Gif, "GIF", "*.GIF", "image/gif", false),
        c(5, Raw::Tiff, "TIFF", "*.TIF;*.TIFF", "image/tiff", false),
        c(6, Raw::Png, "PNG", "*.PNG", "image/png", true),
        c(7, Raw::Icon, "ICO", "*.ICO", "image/x-icon", false),
    ]
}

/// Écrit des ImageCodecInfo (104 octets chacun) suivis de leurs chaînes.
unsafe fn write_codecs(list: &[&Codec], size: u32, buf: *mut u8) -> i32 {
    let need = codecs_size(list);
    if buf.is_null() || size < need {
        return INSUFFICIENT_BUFFER;
    }
    std::ptr::write_bytes(buf, 0, need as usize);
    let mut strings = buf.add(104 * list.len()) as *mut u16;
    let mut put = |s: &str| -> usize {
        let p = strings as usize;
        for (i, u) in s.encode_utf16().chain(std::iter::once(0)).enumerate() {
            *strings.add(i) = u;
        }
        strings = strings.add(s.encode_utf16().count() + 1);
        p
    };
    for (i, c) in list.iter().enumerate() {
        let e = buf.add(104 * i);
        std::ptr::copy_nonoverlapping(c.clsid.as_ptr(), e, 16);
        std::ptr::copy_nonoverlapping(c.raw.guid().as_ptr(), e.add(16), 16);
        let name = format!("Built-in {} Codec", c.name);
        (e.add(32) as *mut usize).write_unaligned(put(&name));
        (e.add(40) as *mut usize).write_unaligned(0);
        (e.add(48) as *mut usize).write_unaligned(put(c.name));
        (e.add(56) as *mut usize).write_unaligned(put(c.ext));
        (e.add(64) as *mut usize).write_unaligned(put(c.mime));
        let flags = 0x1_0000 | 4 | if c.encode { 1 } else { 2 };
        (e.add(72) as *mut u32).write_unaligned(flags);
        (e.add(76) as *mut u32).write_unaligned(1);
    }
    OK
}

fn codecs_size(list: &[&Codec]) -> u32 {
    let strings: usize = list
        .iter()
        .map(|c| {
            (format!("Built-in {} Codec", c.name).len()
                + c.name.len()
                + c.ext.len()
                + c.mime.len()
                + 4)
                * 2
        })
        .sum();
    (104 * list.len() + strings) as u32
}

unsafe extern "win64" fn encoders_size(n: *mut u32, size: *mut u32) -> i32 {
    let all = codecs();
    let list: Vec<&Codec> = all.iter().filter(|c| c.encode).collect();
    out(n, list.len() as u32);
    out(size, codecs_size(&list))
}

unsafe extern "win64" fn get_encoders(n: u32, size: u32, buf: *mut u8) -> i32 {
    let all = codecs();
    let list: Vec<&Codec> = all.iter().filter(|c| c.encode).collect();
    if n as usize != list.len() {
        return INVALID_PARAMETER;
    }
    write_codecs(&list, size, buf)
}

unsafe extern "win64" fn decoders_size(n: *mut u32, size: *mut u32) -> i32 {
    let all = codecs();
    let list: Vec<&Codec> = all.iter().collect();
    out(n, list.len() as u32);
    out(size, codecs_size(&list))
}

unsafe extern "win64" fn get_decoders(n: u32, size: u32, buf: *mut u8) -> i32 {
    let all = codecs();
    let list: Vec<&Codec> = all.iter().collect();
    if n as usize != list.len() {
        return INVALID_PARAMETER;
    }
    write_codecs(&list, size, buf)
}

fn encode(b: &Bitmap, clsid: &[u8; 16]) -> Result<Vec<u8>, i32> {
    let mut rgba = Vec::with_capacity(b.px.len());
    for p in b.px.as_chunks::<4>().0 {
        let c = unpremul(p);
        rgba.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8]);
    }
    if *clsid == codec_clsid(6) {
        return crate::clipbridge::png_encode(b.w, b.h, &rgba).ok_or(GENERIC_ERROR);
    }
    if *clsid == codec_clsid(0) {
        let row = b.w as usize * 4;
        let size = 54 + row * b.h as usize;
        let mut v = Vec::with_capacity(size);
        v.extend_from_slice(b"BM");
        v.extend_from_slice(&(size as u32).to_le_bytes());
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&54u32.to_le_bytes());
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&(b.w as i32).to_le_bytes());
        v.extend_from_slice(&(b.h as i32).to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&32u16.to_le_bytes());
        v.extend_from_slice(&[0; 24]);
        for y in (0..b.h as usize).rev() {
            for p in rgba[y * row..(y + 1) * row].as_chunks::<4>().0 {
                v.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        return Ok(v);
    }
    Err(NOT_IMPLEMENTED)
}

unsafe extern "win64" fn save_image_to_file(
    img: usize,
    name: *const u16,
    clsid: *const [u8; 16],
    _params: usize,
) -> i32 {
    let (Some(mut b), false, false) = (bitmap(img), name.is_null(), clsid.is_null()) else {
        return INVALID_PARAMETER;
    };
    b.sync_in();
    match encode(&b, &*clsid) {
        Ok(data) => match std::fs::write(crate::paths::to_linux(&wstr(name)), data) {
            Ok(()) => OK,
            Err(_) => WIN32_ERROR,
        },
        Err(e) => e,
    }
}

unsafe extern "win64" fn save_image_to_stream(
    img: usize,
    stream: usize,
    clsid: *const [u8; 16],
    _params: usize,
) -> i32 {
    let (Some(mut b), false, false) = (bitmap(img), stream == 0, clsid.is_null()) else {
        return INVALID_PARAMETER;
    };
    b.sync_in();
    match encode(&b, &*clsid) {
        Ok(data) => {
            let vt = *(stream as *const *const usize);
            let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
                std::mem::transmute(*vt.add(4));
            let mut n = 0u32;
            if write(stream, data.as_ptr(), data.len() as u32, &mut n) < 0 {
                WIN32_ERROR
            } else {
                OK
            }
        }
        Err(e) => e,
    }
}

// --- Graphics ----------------------------------------------------------------------

fn new_graphics(target: Target, hwnd: usize) -> usize {
    add(Obj::Graphics(Box::new(Graphics {
        target,
        hwnd,
        st: State {
            m: Matrix::IDENTITY,
            clip: None,
            smoothing: 1,
            offset: 1,
            interp: 2,
            text_hint: 0,
            unit: 1,
            scale: 1.0,
            compositing: 0,
        },
        saved: Vec::new(),
        dc_lent: None,
    })))
}

unsafe extern "win64" fn create_from_hdc(hdc: usize, g: *mut usize) -> i32 {
    if hdc == 0 {
        return OUT_OF_MEMORY;
    }
    out(g, new_graphics(Target::Hdc(hdc), 0))
}

unsafe extern "win64" fn create_from_hdc2(hdc: usize, _device: usize, g: *mut usize) -> i32 {
    create_from_hdc(hdc, g)
}

unsafe extern "win64" fn create_from_hwnd(hwnd: usize, g: *mut usize) -> i32 {
    let hdc = api!("user32", "GetDC", fn(usize) -> usize, hwnd).unwrap_or(0);
    if hdc == 0 {
        return OUT_OF_MEMORY;
    }
    out(g, new_graphics(Target::Hdc(hdc), hwnd))
}

unsafe extern "win64" fn image_graphics(img: usize, g: *mut usize) -> i32 {
    match bitmap(img) {
        Some(_) => out(g, new_graphics(Target::Bitmap(img), 0)),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn delete_graphics(g: usize) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    if let (Target::Hdc(hdc), true) = (gr.target, gr.hwnd != 0) {
        api!("user32", "ReleaseDC", fn(usize, usize) -> i32, gr.hwnd, hdc);
    }
    remove(g)
}

/// GdipGetDC : un HDC où dessiner avec GDI. Pour une bitmap, une DIB
/// temporaire dont les pixels reviennent dans la bitmap à `GdipReleaseDC`.
unsafe extern "win64" fn get_dc(g: usize, hdc: *mut usize) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    match gr.target {
        Target::Hdc(h) => out(hdc, h),
        Target::Bitmap(img) => {
            let Some(mut b) = bitmap(img) else {
                return INVALID_PARAMETER;
            };
            b.sync_in();
            let Some((dib, bits)) = create_dib(b.w, b.h) else {
                return GENERIC_ERROR;
            };
            std::ptr::copy_nonoverlapping(b.px.as_ptr(), bits, b.px.len());
            let dc = api!("gdi32", "CreateCompatibleDC", fn(usize) -> usize, 0).unwrap_or(0);
            let old =
                api!("gdi32", "SelectObject", fn(usize, usize) -> usize, dc, dib).unwrap_or(0);
            with_graphics(g, |gr| gr.dc_lent = Some((dc, dib, old)));
            out(hdc, dc)
        }
    }
}

unsafe extern "win64" fn release_dc(g: usize, hdc: usize) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let (Target::Bitmap(img), Some((dc, dib, old))) = (gr.target, gr.dc_lent) else {
        return OK;
    };
    if dc != hdc {
        return INVALID_PARAMETER;
    }
    api!("gdi32", "SelectObject", fn(usize, usize) -> usize, dc, old);
    if let Some((w, h, px, _)) = gui::bitmap_pixels(dib) {
        with_bitmap(img, |b| {
            if (w, h) == (b.w as usize, b.h as usize) {
                for (d, &p) in b.px.as_chunks_mut::<4>().0.iter_mut().zip(px.iter()) {
                    let c = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
                    // GDI ne tient pas l'alpha : un pixel changé devient opaque.
                    if c != [d[0], d[1], d[2]] {
                        d.copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
                b.sync_out();
            }
        });
    }
    api!("gdi32", "DeleteDC", fn(usize) -> i32, dc);
    api!("gdi32", "DeleteObject", fn(usize) -> i32, dib);
    with_graphics(g, |gr| gr.dc_lent = None);
    OK
}

macro_rules! setter {
    ($set:ident, $get:ident, $field:ident, $t:ty) => {
        unsafe extern "win64" fn $set(g: usize, v: $t) -> i32 {
            with_graphics(g, |gr| gr.st.$field = v).map_or(INVALID_PARAMETER, |_| OK)
        }
        unsafe extern "win64" fn $get(g: usize, v: *mut $t) -> i32 {
            match graphics(g) {
                Some(gr) => out(v, gr.st.$field),
                None => INVALID_PARAMETER,
            }
        }
    };
}

setter!(set_smoothing, get_smoothing, smoothing, i32);
setter!(set_pixel_offset, get_pixel_offset, offset, i32);
setter!(set_interpolation, get_interpolation, interp, i32);
setter!(set_text_hint, get_text_hint, text_hint, i32);
setter!(set_page_unit, get_page_unit, unit, i32);
setter!(set_page_scale, get_page_scale, scale, f32);
setter!(set_compositing_mode, get_compositing_mode, compositing, i32);

unsafe extern "win64" fn set_ignored(g: usize, _v: i32) -> i32 {
    if graphics(g).is_some() {
        OK
    } else {
        INVALID_PARAMETER
    }
}

unsafe extern "win64" fn get_compositing_quality(g: usize, v: *mut i32) -> i32 {
    match graphics(g) {
        Some(_) => out(v, 1),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_dpi(g: usize, v: *mut f32) -> i32 {
    match graphics(g) {
        Some(_) => out(v, 96.0),
        None => INVALID_PARAMETER,
    }
}

/// Facteur des unités de page vers les pixels (96 ppp).
fn unit_scale(unit: i32) -> f32 {
    match unit {
        3 => 96.0 / 72.0,
        4 => 96.0,
        5 => 96.0 / 300.0,
        6 => 96.0 / 25.4,
        _ => 1.0,
    }
}

impl Graphics {
    /// Monde → pixels du périphérique.
    fn device(&self) -> Matrix {
        let s = unit_scale(self.st.unit) * if self.st.unit > 1 { self.st.scale } else { 1.0 };
        let page = Matrix {
            m11: s,
            m22: s,
            ..Matrix::IDENTITY
        };
        self.st.m.then(&page)
    }

    fn antialias(&self) -> bool {
        matches!(self.st.smoothing, 2 | 4 | 5)
    }

    /// Correction de pixel : GDI+ centre ses pixels sur les entiers, sauf
    /// en mode « demi-pixel ».
    fn pixel_shift(&self) -> f32 {
        if self.antialias() && !matches!(self.st.offset, 2 | 4) {
            0.5
        } else {
            0.0
        }
    }
}

unsafe extern "win64" fn set_world_transform(g: usize, m: usize) -> i32 {
    let Some(Obj::Matrix(mm)) = get(m) else {
        return INVALID_PARAMETER;
    };
    with_graphics(g, |gr| gr.st.m = mm).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn get_world_transform(g: usize, m: usize) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    with(m, |o| *o = Obj::Matrix(gr.st.m)).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn reset_world_transform(g: usize) -> i32 {
    with_graphics(g, |gr| gr.st.m = Matrix::IDENTITY).map_or(INVALID_PARAMETER, |_| OK)
}

fn combine(cur: Matrix, op: Matrix, order: i32) -> Matrix {
    if order == 1 {
        cur.then(&op)
    } else {
        op.then(&cur)
    }
}

fn translation(dx: f32, dy: f32) -> Matrix {
    Matrix {
        dx,
        dy,
        ..Matrix::IDENTITY
    }
}

fn scaling(sx: f32, sy: f32) -> Matrix {
    Matrix {
        m11: sx,
        m22: sy,
        ..Matrix::IDENTITY
    }
}

fn rotation(deg: f32) -> Matrix {
    let (s, c) = deg.to_radians().sin_cos();
    Matrix {
        m11: c,
        m12: s,
        m21: -s,
        m22: c,
        ..Matrix::IDENTITY
    }
}

fn transform_graphics(g: usize, op: Matrix, order: i32) -> i32 {
    with_graphics(g, |gr| gr.st.m = combine(gr.st.m, op, order)).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn translate_world(g: usize, dx: f32, dy: f32, order: i32) -> i32 {
    transform_graphics(g, translation(dx, dy), order)
}

unsafe extern "win64" fn scale_world(g: usize, sx: f32, sy: f32, order: i32) -> i32 {
    transform_graphics(g, scaling(sx, sy), order)
}

unsafe extern "win64" fn rotate_world(g: usize, deg: f32, order: i32) -> i32 {
    transform_graphics(g, rotation(deg), order)
}

unsafe extern "win64" fn multiply_world(g: usize, m: usize, order: i32) -> i32 {
    match get(m) {
        Some(Obj::Matrix(mm)) => transform_graphics(g, mm, order),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn save_graphics(g: usize, state: *mut u32) -> i32 {
    let r = with_graphics(g, |gr| {
        let id = gr.saved.last().map_or(1, |s| s.0 + 1);
        gr.saved.push((id, gr.st.clone()));
        id
    });
    match r {
        Some(id) => out(state, id),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn restore_graphics(g: usize, state: u32) -> i32 {
    with_graphics(g, |gr| {
        if let Some(i) = gr.saved.iter().position(|s| s.0 == state) {
            gr.st = gr.saved[i].1.clone();
            gr.saved.truncate(i);
        }
    })
    .map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn begin_container_noparams(g: usize, state: *mut u32) -> i32 {
    save_graphics(g, state)
}

/// Rectangle (x, y, w, h) en monde → boîte en pixels.
fn device_box(m: &Matrix, x: f32, y: f32, w: f32, h: f32) -> Clip {
    let pts = [
        m.apply((x, y)),
        m.apply((x + w, y)),
        m.apply((x, y + h)),
        m.apply((x + w, y + h)),
    ];
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        l = l.min(p.0);
        t = t.min(p.1);
        r = r.max(p.0);
        b = b.max(p.1);
    }
    Clip { l, t, r, b }
}

fn intersect(a: Clip, b: Clip) -> Clip {
    Clip {
        l: a.l.max(b.l),
        t: a.t.max(b.t),
        r: a.r.min(b.r).max(a.l.max(b.l)),
        b: a.b.min(b.b).max(a.t.max(b.t)),
    }
}

fn union(a: Clip, b: Clip) -> Clip {
    Clip {
        l: a.l.min(b.l),
        t: a.t.min(b.t),
        r: a.r.max(b.r),
        b: a.b.max(b.b),
    }
}

fn apply_clip(g: usize, rect: Option<Clip>, mode: i32) -> i32 {
    with_graphics(g, |gr| {
        gr.st.clip = match (mode, gr.st.clip, rect) {
            (0, _, r) => r,
            (1, Some(c), Some(r)) => Some(intersect(c, r)),
            (1, None, r) => r,
            (2, Some(c), Some(r)) => Some(union(c, r)),
            (2, _, _) => None,
            // Exclusion et autres : on garde la découpe courante.
            (_, c, _) => c,
        }
    })
    .map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn set_clip_rect(g: usize, x: f32, y: f32, w: f32, h: f32, mode: i32) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    apply_clip(g, Some(device_box(&gr.device(), x, y, w, h)), mode)
}

unsafe extern "win64" fn set_clip_rect_i(
    g: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    mode: i32,
) -> i32 {
    set_clip_rect(g, x as f32, y as f32, w as f32, h as f32, mode)
}

unsafe extern "win64" fn set_clip_region(g: usize, region: usize, mode: i32) -> i32 {
    let Some(Obj::Region(r)) = get(region) else {
        return INVALID_PARAMETER;
    };
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let r = r.map(|c| device_box(&gr.device(), c.l, c.t, c.r - c.l, c.b - c.t));
    apply_clip(g, r, mode)
}

unsafe extern "win64" fn set_clip_hrgn(g: usize, hrgn: usize, mode: i32) -> i32 {
    let mut r = [0i32; 4];
    let kind = api!(
        "gdi32",
        "GetRgnBox",
        fn(usize, *mut i32) -> i32,
        hrgn,
        r.as_mut_ptr()
    )
    .unwrap_or(0);
    if kind == 0 {
        return INVALID_PARAMETER;
    }
    let c = Clip {
        l: r[0] as f32,
        t: r[1] as f32,
        r: r[2] as f32,
        b: r[3] as f32,
    };
    apply_clip(g, Some(c), mode)
}

unsafe extern "win64" fn set_clip_graphics(g: usize, src: usize, mode: i32) -> i32 {
    match graphics(src) {
        Some(s) => apply_clip(g, s.st.clip, mode),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn set_clip_path(g: usize, path: usize, mode: i32) -> i32 {
    let (Some(gr), Some(Obj::Path(p))) = (graphics(g), get(path)) else {
        return INVALID_PARAMETER;
    };
    let polys = p.path.fill_polys(&gr.device());
    let b = poly_bounds(&polys);
    apply_clip(g, b, mode)
}

unsafe extern "win64" fn reset_clip(g: usize) -> i32 {
    with_graphics(g, |gr| gr.st.clip = None).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn translate_clip(g: usize, dx: f32, dy: f32) -> i32 {
    with_graphics(g, |gr| {
        if let Some(c) = &mut gr.st.clip {
            c.l += dx;
            c.r += dx;
            c.t += dy;
            c.b += dy;
        }
    })
    .map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn translate_clip_i(g: usize, dx: i32, dy: i32) -> i32 {
    translate_clip(g, dx as f32, dy as f32)
}

/// Bornes de la cible en pixels.
fn target_bounds(gr: &Graphics) -> Clip {
    let (w, h) = match gr.target {
        Target::Bitmap(img) => bitmap(img).map_or((0, 0), |b| (b.w, b.h)),
        Target::Hdc(hdc) => {
            let bmp = gui::dc_bitmap(hdc);
            match (bmp != 0).then(|| gui::bitmap_pixels(bmp)).flatten() {
                Some((w, h, _, _)) => (w as u32, h as u32),
                None => window_size(gr.hwnd, hdc),
            }
        }
    };
    Clip {
        l: 0.0,
        t: 0.0,
        r: w as f32,
        b: h as f32,
    }
}

fn window_size(hwnd: usize, hdc: usize) -> (u32, u32) {
    unsafe {
        let hwnd = if hwnd != 0 {
            hwnd
        } else {
            api!("user32", "WindowFromDC", fn(usize) -> usize, hdc).unwrap_or(0)
        };
        let mut r = [0i32; 4];
        if hwnd != 0
            && api!(
                "user32",
                "GetClientRect",
                fn(usize, *mut i32) -> i32,
                hwnd,
                r.as_mut_ptr()
            )
            .unwrap_or(0)
                != 0
        {
            return ((r[2] - r[0]).max(0) as u32, (r[3] - r[1]).max(0) as u32);
        }
        (4096, 4096)
    }
}

/// Clip en monde pour les fonctions qui le relisent.
fn world_rect(gr: &Graphics, c: Clip) -> [f32; 4] {
    let inv = gr.device().invert().unwrap_or(Matrix::IDENTITY);
    let b = device_box(&inv, c.l, c.t, c.r - c.l, c.b - c.t);
    [b.l, b.t, b.r - b.l, b.b - b.t]
}

unsafe fn put_rect(rect: *mut f32, v: [f32; 4]) -> i32 {
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    for (i, x) in v.into_iter().enumerate() {
        *rect.add(i) = x;
    }
    OK
}

unsafe extern "win64" fn get_clip_bounds(g: usize, rect: *mut f32) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let c = gr.st.clip.unwrap_or(Clip {
        l: -4_194_304.0,
        t: -4_194_304.0,
        r: 4_194_304.0,
        b: 4_194_304.0,
    });
    put_rect(rect, world_rect(&gr, c))
}

unsafe extern "win64" fn get_clip_bounds_i(g: usize, rect: *mut i32) -> i32 {
    let mut f = [0f32; 4];
    let r = get_clip_bounds(g, f.as_mut_ptr());
    if r == OK && !rect.is_null() {
        for (i, v) in f.iter().enumerate() {
            *rect.add(i) = v.round() as i32;
        }
    }
    r
}

unsafe extern "win64" fn get_visible_clip_bounds(g: usize, rect: *mut f32) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let t = target_bounds(&gr);
    let c = gr.st.clip.map_or(t, |c| intersect(c, t));
    put_rect(rect, world_rect(&gr, c))
}

unsafe extern "win64" fn get_visible_clip_bounds_i(g: usize, rect: *mut i32) -> i32 {
    let mut f = [0f32; 4];
    let r = get_visible_clip_bounds(g, f.as_mut_ptr());
    if r == OK && !rect.is_null() {
        for (i, v) in f.iter().enumerate() {
            *rect.add(i) = v.round() as i32;
        }
    }
    r
}

unsafe extern "win64" fn is_clip_empty(g: usize, res: *mut i32) -> i32 {
    match graphics(g) {
        Some(gr) => out(
            res,
            gr.st.clip.is_some_and(|c| c.l >= c.r || c.t >= c.b) as i32,
        ),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_clip(g: usize, region: usize) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let r = gr.st.clip.map(|c| {
        let v = world_rect(&gr, c);
        Clip {
            l: v[0],
            t: v[1],
            r: v[0] + v[2],
            b: v[1] + v[3],
        }
    });
    with(region, |o| *o = Obj::Region(r)).map_or(INVALID_PARAMETER, |_| OK)
}

unsafe extern "win64" fn is_visible_point(g: usize, x: f32, y: f32, res: *mut i32) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let p = gr.device().apply((x, y));
    let t = target_bounds(&gr);
    let c = gr.st.clip.map_or(t, |c| intersect(c, t));
    out(
        res,
        (p.0 >= c.l && p.0 < c.r && p.1 >= c.t && p.1 < c.b) as i32,
    )
}

unsafe extern "win64" fn is_visible_rect(
    g: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    res: *mut i32,
) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let b = device_box(&gr.device(), x, y, w, h);
    let t = target_bounds(&gr);
    let c = gr.st.clip.map_or(t, |c| intersect(c, t));
    out(
        res,
        (b.l < c.r && b.r > c.l && b.t < c.b && b.b > c.t) as i32,
    )
}

unsafe extern "win64" fn transform_points(
    g: usize,
    dst: i32,
    src: i32,
    pts: *mut f32,
    n: i32,
) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    if pts.is_null() || n < 0 {
        return INVALID_PARAMETER;
    }
    // Espaces : 0 monde, 1 page, 2 périphérique.
    let to_device = |space: i32| match space {
        0 => gr.device(),
        1 => scaling(unit_scale(gr.st.unit), unit_scale(gr.st.unit)),
        _ => Matrix::IDENTITY,
    };
    let m = to_device(src).then(&to_device(dst).invert().unwrap_or(Matrix::IDENTITY));
    for i in 0..n as usize {
        let p = m.apply((*pts.add(2 * i), *pts.add(2 * i + 1)));
        *pts.add(2 * i) = p.0;
        *pts.add(2 * i + 1) = p.1;
    }
    OK
}

unsafe extern "win64" fn transform_points_i(
    g: usize,
    dst: i32,
    src: i32,
    pts: *mut i32,
    n: i32,
) -> i32 {
    if pts.is_null() || n < 0 {
        return INVALID_PARAMETER;
    }
    let mut f: Vec<f32> = (0..2 * n as usize).map(|i| *pts.add(i) as f32).collect();
    let r = transform_points(g, dst, src, f.as_mut_ptr(), n);
    for (i, v) in f.iter().enumerate() {
        *pts.add(i) = v.round() as i32;
    }
    r
}

// --- Rendu ---------------------------------------------------------------------------

fn poly_bounds(polys: &[Vec<Pt>]) -> Option<Clip> {
    let mut b: Option<Clip> = None;
    for p in polys.iter().flatten() {
        if !(p.0.is_finite() && p.1.is_finite()) {
            continue;
        }
        let c = Clip {
            l: p.0,
            t: p.1,
            r: p.0,
            b: p.1,
        };
        b = Some(b.map_or(c, |b| union(b, c)));
    }
    b
}

type Paint<'a> = dyn Fn(f32, f32) -> [f32; 4] + 'a;

/// Couleur de pinceau au point (x, y) en pixels, prémultipliée.
fn brush_paint<'a>(b: &'a Brush, device: Matrix, tex: Option<&'a Bitmap>) -> Box<Paint<'a>> {
    fn col(c: u32) -> [f32; 4] {
        let a = (c >> 24) as f32 / 255.0;
        [
            ((c >> 16) & 0xFF) as f32 / 255.0 * a,
            ((c >> 8) & 0xFF) as f32 / 255.0 * a,
            (c & 0xFF) as f32 / 255.0 * a,
            a,
        ]
    }
    fn lerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
        [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
    }
    match b {
        Brush::Solid(c) => {
            let c = col(*c);
            Box::new(move |_, _| c)
        }
        Brush::Hatch { style, fore, back } => {
            let (f, b, st) = (col(*fore), col(*back), *style);
            Box::new(move |x, y| {
                if hatch_on(st, x.floor() as i32, y.floor() as i32) {
                    f
                } else {
                    b
                }
            })
        }
        Brush::Linear {
            p1,
            p2,
            c1,
            c2,
            wrap,
            m,
        } => {
            let to_world = m.then(&device).invert().unwrap_or(Matrix::IDENTITY);
            let (a, b) = (col(*c1), col(*c2));
            let (d0, d1) = (p2.0 - p1.0, p2.1 - p1.1);
            let len2 = (d0 * d0 + d1 * d1).max(1e-6);
            let (p1, wrap) = (*p1, *wrap);
            Box::new(move |x, y| {
                let p = to_world.apply((x, y));
                let mut t = ((p.0 - p1.0) * d0 + (p.1 - p1.1) * d1) / len2;
                t = match wrap {
                    // Tile : répété ; TileFlip* : en miroir ; Clamp : borné.
                    0 => t.rem_euclid(1.0),
                    1..=3 => {
                        let r = t.rem_euclid(2.0);
                        if r > 1.0 {
                            2.0 - r
                        } else {
                            r
                        }
                    }
                    _ => t.clamp(0.0, 1.0),
                };
                lerp(a, b, t)
            })
        }
        Brush::PathGradient {
            center,
            surround,
            cx,
            cy,
            rx,
            ry,
        } => {
            let to_world = device.invert().unwrap_or(Matrix::IDENTITY);
            let (a, b) = (col(*center), col(*surround));
            let (cx, cy, rx, ry) = (*cx, *cy, rx.max(1e-3), ry.max(1e-3));
            Box::new(move |x, y| {
                let p = to_world.apply((x, y));
                let d = (((p.0 - cx) / rx).powi(2) + ((p.1 - cy) / ry).powi(2)).sqrt();
                lerp(a, b, d.min(1.0))
            })
        }
        Brush::Texture { wrap, m, .. } => {
            let Some(t) = tex else {
                return Box::new(|_, _| [0.0; 4]);
            };
            let to_tex = m.then(&device).invert().unwrap_or(Matrix::IDENTITY);
            let wrap = *wrap;
            Box::new(move |x, y| {
                let p = to_tex.apply((x, y));
                let (w, h) = (t.w as f32, t.h as f32);
                let (mut u, mut v) = (p.0, p.1);
                if wrap == 4 {
                    if u < 0.0 || v < 0.0 || u >= w || v >= h {
                        return [0.0; 4];
                    }
                } else {
                    u = u.rem_euclid(w);
                    v = v.rem_euclid(h);
                }
                let i = ((v as usize).min(t.h as usize - 1) * t.w as usize
                    + (u as usize).min(t.w as usize - 1))
                    * 4;
                let s = &t.px[i..i + 4];
                [
                    s[2] as f32 / 255.0,
                    s[1] as f32 / 255.0,
                    s[0] as f32 / 255.0,
                    s[3] as f32 / 255.0,
                ]
            })
        }
    }
}

/// Image d'un pinceau texture (lue avant de verrouiller la cible).
/// Motif 8×8 des hachures (HatchStyle) en pixels du périphérique.
pub(crate) fn hatch_on(style: i32, x: i32, y: i32) -> bool {
    const BAYER: [[u8; 8]; 8] = [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ];
    const PERCENT: [u32; 12] = [5, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90];
    let (x, y) = (x.rem_euclid(8), y.rem_euclid(8));
    match style {
        0 => y == 0,
        1 => x == 0,
        2 => x == y,
        3 => x == 7 - y,
        4 => x == 0 || y == 0,
        5 => x == y || x == 7 - y,
        6..=17 => (BAYER[y as usize][x as usize] as u32) < PERCENT[(style - 6) as usize] * 64 / 100,
        _ => (x + y) % 4 == 0,
    }
}

fn texture_of(b: &Brush) -> Option<Bitmap> {
    match b {
        Brush::Texture { img, .. } => bitmap(*img).map(|mut b| {
            unsafe { b.sync_in() };
            b
        }),
        _ => None,
    }
}

/// Remplit des polygones (pixels) sur la cible du Graphics.
fn render(gr: &Graphics, mut polys: Vec<Vec<Pt>>, rule: FillRule, paint: &Paint<'_>) -> i32 {
    let shift = gr.pixel_shift();
    let aa = gr.antialias();
    for p in polys.iter_mut().flatten() {
        if aa {
            p.0 += shift;
            p.1 += shift;
        } else {
            p.0 = p.0.round();
            p.1 = p.1.round();
        }
    }
    let Some(bb) = poly_bounds(&polys) else {
        return OK;
    };
    let bounds = target_bounds(gr);
    let mut area = intersect(bounds, gr.st.clip.unwrap_or(bounds));
    let clip = area;
    area = intersect(
        area,
        Clip {
            l: bb.l.floor() - 1.0,
            t: bb.t.floor() - 1.0,
            r: bb.r.ceil() + 1.0,
            b: bb.b.ceil() + 1.0,
        },
    );
    let (x0, y0) = (area.l.floor().max(0.0), area.t.floor().max(0.0));
    let (w, h) = ((area.r.ceil() - x0) as i64, (area.b.ceil() - y0) as i64);
    if w <= 0 || h <= 0 {
        return OK;
    }
    match gr.target {
        Target::Bitmap(img) => {
            with_bitmap(img, |b| {
                unsafe { b.sync_in() };
                let mut c = b.canvas();
                c.fill(&polys, rule, Some(clip), paint);
                unsafe { b.sync_out() };
            });
            OK
        }
        Target::Hdc(hdc) => {
            if w * h > 4096 * 4096 {
                return OK;
            }
            let (w, h) = (w as u32, h as u32);
            let mut px = vec![0u8; w as usize * h as usize * 4];
            let local: Vec<Vec<Pt>> = polys
                .iter()
                .map(|p| p.iter().map(|q| (q.0 - x0, q.1 - y0)).collect())
                .collect();
            let lclip = Clip {
                l: clip.l - x0,
                t: clip.t - y0,
                r: clip.r - x0,
                b: clip.b - y0,
            };
            let shifted = |x: f32, y: f32| paint(x + x0, y + y0);
            Canvas {
                w,
                h,
                stride: w as usize * 4,
                px: &mut px,
            }
            .fill(&local, rule, Some(lclip), &shifted);
            present(hdc, x0 as i32, y0 as i32, w, h, &px);
            OK
        }
    }
}

/// Fond une image BGRA prémultipliée dans un HDC.
pub(crate) fn present(hdc: usize, x: i32, y: i32, w: u32, h: u32, px: &[u8]) {
    if px.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        return;
    }
    let img = icons::Image {
        w: w as usize,
        h: h as usize,
        px: px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let c = unpremul(p);
                // ARGB → COLORREF + alpha.
                (c & 0xFF00_FF00) | ((c >> 16) & 0xFF) | ((c & 0xFF) << 16)
            })
            .collect(),
    };
    super::common_apps::blend_image(hdc, x, y, &img);
}

fn fill_path(g: usize, b: usize, path: &Path, rule: FillRule) -> i32 {
    let (Some(gr), Some(br)) = (graphics(g), brush(b)) else {
        return INVALID_PARAMETER;
    };
    let device = gr.device();
    let tex = texture_of(&br);
    let paint = brush_paint(&br, device, tex.as_ref());
    render(&gr, path.fill_polys(&device), rule, &*paint)
}

fn stroke_path(g: usize, p: usize, path: &Path) -> i32 {
    let (Some(gr), Some(pen)) = (graphics(g), pen(p)) else {
        return INVALID_PARAMETER;
    };
    let device = gr.device();
    let width = if pen.unit == 2 || pen.unit == 1 {
        pen.width / device.scale()
    } else {
        pen.width
    };
    // Trait de largeur 0 : un pixel quelle que soit l'échelle.
    let width = if width <= 0.0 {
        1.0 / device.scale()
    } else {
        width
    };
    let path = if pen.dash != 0 {
        dashed(path, pen.dash, width)
    } else {
        path.clone()
    };
    let tex = texture_of(&pen.brush);
    let paint = brush_paint(&pen.brush, device, tex.as_ref());
    render(
        &gr,
        path.stroke_polys(&device, width),
        FillRule::NonZero,
        &*paint,
    )
}

/// Découpe les figures en tirets (styles Dash 1, Dot 2, DashDot 3, DashDotDot 4).
fn dashed(path: &Path, style: i32, width: f32) -> Path {
    let pattern: &[f32] = match style {
        1 => &[3.0, 1.0],
        2 => &[1.0, 1.0],
        3 => &[3.0, 1.0, 1.0, 1.0],
        4 => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => return path.clone(),
    };
    let w = width.max(1.0);
    let mut out = Path::default();
    for (pts, closed) in &path.figures {
        let mut pts = pts.clone();
        if *closed && !pts.is_empty() {
            pts.push(pts[0]);
        }
        let (mut k, mut left, mut on) = (0usize, pattern[0] * w, true);
        let mut cur: Vec<Pt> = Vec::new();
        for s in pts.windows(2) {
            let (mut a, b) = (s[0], s[1]);
            let mut len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            while len > 0.0 {
                let step = left.min(len);
                let t = step / len;
                let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                if on {
                    if cur.is_empty() {
                        cur.push(a);
                    }
                    cur.push(p);
                }
                left -= step;
                len -= step;
                a = p;
                if left <= 1e-4 {
                    if on && cur.len() > 1 {
                        out.figures.push((std::mem::take(&mut cur), false));
                    }
                    cur.clear();
                    on = !on;
                    k = (k + 1) % pattern.len();
                    left = pattern[k] * w;
                }
            }
        }
        if on && cur.len() > 1 {
            out.figures.push((cur, false));
        }
    }
    out
}

// --- Primitives -----------------------------------------------------------------------

const CURVE_SCALE: f32 = 4.0;

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Path {
    Path::rect(x, y, x + w, y + h)
}

fn ellipse_path(x: f32, y: f32, w: f32, h: f32) -> Path {
    Path::ellipse(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0, CURVE_SCALE)
}

/// Point de l'ellipse inscrite dans (x, y, w, h) à l'angle `deg` (sens
/// horaire, angle réel du point comme GDI+).
fn ellipse_point(x: f32, y: f32, w: f32, h: f32, deg: f32) -> Pt {
    let (rx, ry) = (w / 2.0, h / 2.0);
    let (s, c) = deg.to_radians().sin_cos();
    let r = if rx <= 0.0 || ry <= 0.0 {
        0.0
    } else {
        1.0 / ((c / rx).powi(2) + (s / ry).powi(2)).sqrt()
    };
    (x + rx + r * c, y + ry + r * s)
}

fn arc_points(x: f32, y: f32, w: f32, h: f32, start: f32, sweep: f32) -> Vec<Pt> {
    let sweep = sweep.clamp(-360.0, 360.0);
    let n = ((sweep.abs() / 360.0) * 64.0).ceil().max(2.0) as usize;
    (0..=n)
        .map(|i| ellipse_point(x, y, w, h, start + sweep * i as f32 / n as f32))
        .collect()
}

fn poly_path(pts: &[Pt], closed: bool) -> Path {
    let mut p = Path::default();
    if pts.is_empty() {
        return p;
    }
    p.begin(pts[0]);
    for q in &pts[1..] {
        p.line(*q);
    }
    if closed {
        p.close();
    }
    p
}

unsafe fn points_f(pts: *const f32, n: i32) -> Vec<Pt> {
    if pts.is_null() || n <= 0 {
        return Vec::new();
    }
    (0..n as usize)
        .map(|i| (*pts.add(2 * i), *pts.add(2 * i + 1)))
        .collect()
}

unsafe fn points_i(pts: *const i32, n: i32) -> Vec<Pt> {
    if pts.is_null() || n <= 0 {
        return Vec::new();
    }
    (0..n as usize)
        .map(|i| (*pts.add(2 * i) as f32, *pts.add(2 * i + 1) as f32))
        .collect()
}

fn bezier_path(pts: &[Pt]) -> Path {
    let mut p = Path::default();
    if pts.is_empty() {
        return p;
    }
    p.begin(pts[0]);
    for c in pts[1..].as_chunks::<3>().0 {
        p.cubic(c[0], c[1], c[2], CURVE_SCALE);
    }
    p
}

/// Courbe cardinale (tension 0.5) passant par les points.
fn curve_points(pts: &[Pt], closed: bool, tension: f32) -> Path {
    let n = pts.len();
    let mut p = Path::default();
    if n < 2 {
        return p;
    }
    let t = tension / 3.0;
    let at = |i: isize| -> Pt {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    p.begin(pts[0]);
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let c1 = (p1.0 + (p2.0 - p0.0) * t, p1.1 + (p2.1 - p0.1) * t);
        let c2 = (p2.0 - (p3.0 - p1.0) * t, p2.1 - (p3.1 - p1.1) * t);
        p.cubic(c1, c2, p2, CURVE_SCALE);
    }
    if closed {
        p.close();
    }
    p
}

fn fill_mode(m: i32) -> FillRule {
    if m == 1 {
        FillRule::NonZero
    } else {
        FillRule::EvenOdd
    }
}

unsafe extern "win64" fn fill_rectangle(g: usize, b: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    fill_path(g, b, &rect_path(x, y, w, h), FillRule::NonZero)
}
unsafe extern "win64" fn fill_rectangle_i(
    g: usize,
    b: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> i32 {
    fill_rectangle(g, b, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn fill_rectangles(g: usize, b: usize, r: *const f32, n: i32) -> i32 {
    let mut p = Path::default();
    for i in 0..n.max(0) as usize {
        let q = r.add(4 * i);
        p.append(&rect_path(*q, *q.add(1), *q.add(2), *q.add(3)));
    }
    fill_path(g, b, &p, FillRule::NonZero)
}
unsafe extern "win64" fn fill_rectangles_i(g: usize, b: usize, r: *const i32, n: i32) -> i32 {
    let f: Vec<f32> = (0..4 * n.max(0) as usize)
        .map(|i| *r.add(i) as f32)
        .collect();
    fill_rectangles(g, b, f.as_ptr(), n)
}
unsafe extern "win64" fn fill_ellipse(g: usize, b: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    fill_path(g, b, &ellipse_path(x, y, w, h), FillRule::NonZero)
}
unsafe extern "win64" fn fill_ellipse_i(g: usize, b: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    fill_ellipse(g, b, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn fill_pie(
    g: usize,
    b: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    sw: f32,
) -> i32 {
    let mut pts = vec![(x + w / 2.0, y + h / 2.0)];
    pts.extend(arc_points(x, y, w, h, s, sw));
    fill_path(g, b, &poly_path(&pts, true), FillRule::NonZero)
}
unsafe extern "win64" fn fill_pie_i(
    g: usize,
    b: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: f32,
    sw: f32,
) -> i32 {
    fill_pie(g, b, x as f32, y as f32, w as f32, h as f32, s, sw)
}
unsafe extern "win64" fn fill_polygon(
    g: usize,
    b: usize,
    pts: *const f32,
    n: i32,
    mode: i32,
) -> i32 {
    fill_path(g, b, &poly_path(&points_f(pts, n), true), fill_mode(mode))
}
unsafe extern "win64" fn fill_polygon_i(
    g: usize,
    b: usize,
    pts: *const i32,
    n: i32,
    mode: i32,
) -> i32 {
    fill_path(g, b, &poly_path(&points_i(pts, n), true), fill_mode(mode))
}
unsafe extern "win64" fn fill_polygon2(g: usize, b: usize, pts: *const f32, n: i32) -> i32 {
    fill_polygon(g, b, pts, n, 0)
}
unsafe extern "win64" fn fill_polygon2_i(g: usize, b: usize, pts: *const i32, n: i32) -> i32 {
    fill_polygon_i(g, b, pts, n, 0)
}
unsafe extern "win64" fn fill_closed_curve(g: usize, b: usize, pts: *const f32, n: i32) -> i32 {
    fill_path(
        g,
        b,
        &curve_points(&points_f(pts, n), true, 0.5),
        FillRule::EvenOdd,
    )
}
unsafe extern "win64" fn fill_closed_curve_i(g: usize, b: usize, pts: *const i32, n: i32) -> i32 {
    fill_path(
        g,
        b,
        &curve_points(&points_i(pts, n), true, 0.5),
        FillRule::EvenOdd,
    )
}
unsafe extern "win64" fn fill_gpath(g: usize, b: usize, path: usize) -> i32 {
    match get(path) {
        Some(Obj::Path(p)) => fill_path(g, b, &p.path, p.fill),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn fill_region(g: usize, b: usize, region: usize) -> i32 {
    match get(region) {
        Some(Obj::Region(Some(c))) => fill_rectangle(g, b, c.l, c.t, c.r - c.l, c.b - c.t),
        Some(Obj::Region(None)) => {
            let Some(gr) = graphics(g) else {
                return INVALID_PARAMETER;
            };
            let v = world_rect(&gr, target_bounds(&gr));
            fill_rectangle(g, b, v[0], v[1], v[2], v[3])
        }
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn draw_line(g: usize, p: usize, x1: f32, y1: f32, x2: f32, y2: f32) -> i32 {
    stroke_path(g, p, &poly_path(&[(x1, y1), (x2, y2)], false))
}
unsafe extern "win64" fn draw_line_i(
    g: usize,
    p: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> i32 {
    draw_line(g, p, x1 as f32, y1 as f32, x2 as f32, y2 as f32)
}
unsafe extern "win64" fn draw_lines(g: usize, p: usize, pts: *const f32, n: i32) -> i32 {
    stroke_path(g, p, &poly_path(&points_f(pts, n), false))
}
unsafe extern "win64" fn draw_lines_i(g: usize, p: usize, pts: *const i32, n: i32) -> i32 {
    stroke_path(g, p, &poly_path(&points_i(pts, n), false))
}
unsafe extern "win64" fn draw_rectangle(g: usize, p: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    stroke_path(g, p, &rect_path(x, y, w, h))
}
unsafe extern "win64" fn draw_rectangle_i(
    g: usize,
    p: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> i32 {
    draw_rectangle(g, p, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn draw_rectangles(g: usize, p: usize, r: *const f32, n: i32) -> i32 {
    let mut path = Path::default();
    for i in 0..n.max(0) as usize {
        let q = r.add(4 * i);
        path.append(&rect_path(*q, *q.add(1), *q.add(2), *q.add(3)));
    }
    stroke_path(g, p, &path)
}
unsafe extern "win64" fn draw_rectangles_i(g: usize, p: usize, r: *const i32, n: i32) -> i32 {
    let f: Vec<f32> = (0..4 * n.max(0) as usize)
        .map(|i| *r.add(i) as f32)
        .collect();
    draw_rectangles(g, p, f.as_ptr(), n)
}
unsafe extern "win64" fn draw_ellipse(g: usize, p: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    stroke_path(g, p, &ellipse_path(x, y, w, h))
}
unsafe extern "win64" fn draw_ellipse_i(g: usize, p: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    draw_ellipse(g, p, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn draw_arc(
    g: usize,
    p: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    sw: f32,
) -> i32 {
    stroke_path(g, p, &poly_path(&arc_points(x, y, w, h, s, sw), false))
}
unsafe extern "win64" fn draw_arc_i(
    g: usize,
    p: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: f32,
    sw: f32,
) -> i32 {
    draw_arc(g, p, x as f32, y as f32, w as f32, h as f32, s, sw)
}
unsafe extern "win64" fn draw_pie(
    g: usize,
    p: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    sw: f32,
) -> i32 {
    let mut pts = vec![(x + w / 2.0, y + h / 2.0)];
    pts.extend(arc_points(x, y, w, h, s, sw));
    stroke_path(g, p, &poly_path(&pts, true))
}
unsafe extern "win64" fn draw_pie_i(
    g: usize,
    p: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: f32,
    sw: f32,
) -> i32 {
    draw_pie(g, p, x as f32, y as f32, w as f32, h as f32, s, sw)
}
unsafe extern "win64" fn draw_polygon(g: usize, p: usize, pts: *const f32, n: i32) -> i32 {
    stroke_path(g, p, &poly_path(&points_f(pts, n), true))
}
unsafe extern "win64" fn draw_polygon_i(g: usize, p: usize, pts: *const i32, n: i32) -> i32 {
    stroke_path(g, p, &poly_path(&points_i(pts, n), true))
}
unsafe extern "win64" fn draw_bezier(
    g: usize,
    p: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    x4: f32,
    y4: f32,
) -> i32 {
    stroke_path(
        g,
        p,
        &bezier_path(&[(x1, y1), (x2, y2), (x3, y3), (x4, y4)]),
    )
}
unsafe extern "win64" fn draw_bezier_i(
    g: usize,
    p: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    x3: i32,
    y3: i32,
    x4: i32,
    y4: i32,
) -> i32 {
    draw_bezier(
        g, p, x1 as f32, y1 as f32, x2 as f32, y2 as f32, x3 as f32, y3 as f32, x4 as f32,
        y4 as f32,
    )
}
unsafe extern "win64" fn draw_beziers(g: usize, p: usize, pts: *const f32, n: i32) -> i32 {
    stroke_path(g, p, &bezier_path(&points_f(pts, n)))
}
unsafe extern "win64" fn draw_beziers_i(g: usize, p: usize, pts: *const i32, n: i32) -> i32 {
    stroke_path(g, p, &bezier_path(&points_i(pts, n)))
}
unsafe extern "win64" fn draw_curve(g: usize, p: usize, pts: *const f32, n: i32) -> i32 {
    stroke_path(g, p, &curve_points(&points_f(pts, n), false, 0.5))
}
unsafe extern "win64" fn draw_curve_i(g: usize, p: usize, pts: *const i32, n: i32) -> i32 {
    stroke_path(g, p, &curve_points(&points_i(pts, n), false, 0.5))
}
unsafe extern "win64" fn draw_curve2(g: usize, p: usize, pts: *const f32, n: i32, t: f32) -> i32 {
    stroke_path(g, p, &curve_points(&points_f(pts, n), false, t))
}
unsafe extern "win64" fn draw_curve2_i(g: usize, p: usize, pts: *const i32, n: i32, t: f32) -> i32 {
    stroke_path(g, p, &curve_points(&points_i(pts, n), false, t))
}
unsafe extern "win64" fn draw_closed_curve(g: usize, p: usize, pts: *const f32, n: i32) -> i32 {
    stroke_path(g, p, &curve_points(&points_f(pts, n), true, 0.5))
}
unsafe extern "win64" fn draw_closed_curve_i(g: usize, p: usize, pts: *const i32, n: i32) -> i32 {
    stroke_path(g, p, &curve_points(&points_i(pts, n), true, 0.5))
}
unsafe extern "win64" fn draw_gpath(g: usize, p: usize, path: usize) -> i32 {
    match get(path) {
        Some(Obj::Path(gp)) => stroke_path(g, p, &gp.path),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn graphics_clear(g: usize, color: u32) -> i32 {
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let bounds = target_bounds(&gr);
    let area = intersect(bounds, gr.st.clip.unwrap_or(bounds));
    match gr.target {
        Target::Bitmap(img) => {
            let c = premul_argb(color);
            with_bitmap(img, |b| {
                b.sync_in();
                let (l, t) = (area.l.max(0.0) as usize, area.t.max(0.0) as usize);
                let (r, bt) = (
                    (area.r.ceil() as usize).min(b.w as usize),
                    (area.b.ceil() as usize).min(b.h as usize),
                );
                for y in t..bt {
                    for x in l..r {
                        let i = (y * b.w as usize + x) * 4;
                        b.px[i..i + 4].copy_from_slice(&c);
                    }
                }
                b.sync_out();
            });
            OK
        }
        Target::Hdc(hdc) => {
            // Un HDC n'a pas d'alpha : la couleur est posée telle quelle.
            let rect = gui::Rect {
                left: area.l as i32,
                top: area.t as i32,
                right: area.r.ceil() as i32,
                bottom: area.b.ceil() as i32,
            };
            let cref = ((color >> 16) & 0xFF) | (color & 0xFF00) | ((color & 0xFF) << 16);
            gui::record(hdc, gui::DrawCmd::FillRect { rect, color: cref });
            OK
        }
    }
}

// --- Images dessinées ---------------------------------------------------------------

/// Matrice de couleurs et clé de couleur appliquées à un pixel prémultiplié.
fn adjust_color(px: [f32; 4], attrs: Option<&Attributes>) -> [f32; 4] {
    let Some(a) = attrs else {
        return px;
    };
    let alpha = px[3];
    let (mut r, mut g, mut b) = if alpha > 0.0 {
        (px[0] / alpha, px[1] / alpha, px[2] / alpha)
    } else {
        (0.0, 0.0, 0.0)
    };
    let mut alpha = alpha;
    if let Some((lo, hi)) = a.key {
        let in_range = |v: f32, sh: u32| {
            let v = (v * 255.0).round() as u32;
            v >= (lo >> sh) & 0xFF && v <= (hi >> sh) & 0xFF
        };
        if in_range(r, 16) && in_range(g, 8) && in_range(b, 0) {
            return [0.0; 4];
        }
    }
    if let Some(m) = a.matrix {
        let v = [r, g, b, alpha, 1.0];
        let o = |j: usize| (0..5).map(|i| v[i] * m[i][j]).sum::<f32>().clamp(0.0, 1.0);
        (r, g, b, alpha) = (o(0), o(1), o(2), o(3));
    }
    [r * alpha, g * alpha, b * alpha, alpha]
}

/// Échantillonne `src` (BGRA prémultiplié) au point (u, v) en pixels.
fn sample(src: &Bitmap, u: f32, v: f32, smooth: bool, area: (f32, f32, f32, f32)) -> [f32; 4] {
    let (sx, sy, sw, sh) = area;
    let px = |x: i64, y: i64| -> [f32; 4] {
        let x = x
            .clamp(sx.floor() as i64, (sx + sw).ceil() as i64 - 1)
            .clamp(0, src.w as i64 - 1);
        let y = y
            .clamp(sy.floor() as i64, (sy + sh).ceil() as i64 - 1)
            .clamp(0, src.h as i64 - 1);
        let i = (y as usize * src.w as usize + x as usize) * 4;
        let p = &src.px[i..i + 4];
        [
            p[2] as f32 / 255.0,
            p[1] as f32 / 255.0,
            p[0] as f32 / 255.0,
            p[3] as f32 / 255.0,
        ]
    };
    if !smooth {
        return px(u.floor() as i64, v.floor() as i64);
    }
    let (fu, fv) = (u - 0.5, v - 0.5);
    let (x0, y0) = (fu.floor(), fv.floor());
    let (tx, ty) = (fu - x0, fv - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (
        px(x0, y0),
        px(x0 + 1, y0),
        px(x0, y0 + 1),
        px(x0 + 1, y0 + 1),
    );
    [0, 1, 2, 3].map(|i| {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bot = c[i] + (d[i] - c[i]) * tx;
        top + (bot - top) * ty
    })
}

/// Dessine la zone `area` de `src` sur une bitmap, avec la matrice `m`
/// (pixels source → pixels destination).
#[allow(clippy::too_many_arguments)]
fn draw_bitmap_into(
    dst: &mut Bitmap,
    src: &Bitmap,
    area: (f32, f32, f32, f32),
    m: &Matrix,
    clip: Option<Clip>,
    attrs: Option<&Attributes>,
    interp: i32,
) {
    let (sx, sy, sw, sh) = area;
    let quad = poly_path(
        &[(sx, sy), (sx + sw, sy), (sx + sw, sy + sh), (sx, sy + sh)],
        true,
    );
    let polys = quad.fill_polys(m);
    let inv = m.invert().unwrap_or(Matrix::IDENTITY);
    let smooth = interp != 5;
    let paint = |x: f32, y: f32| {
        let p = inv.apply((x, y));
        adjust_color(sample(src, p.0, p.1, smooth, area), attrs)
    };
    dst.canvas().fill(&polys, FillRule::NonZero, clip, &paint);
}

#[allow(clippy::too_many_arguments)]
fn draw_image_impl(
    g: usize,
    img: usize,
    dest: [Pt; 3],
    area: (f32, f32, f32, f32),
    unit: i32,
    attrs: usize,
) -> i32 {
    let (Some(gr), Some(mut src)) = (graphics(g), bitmap(img)) else {
        return INVALID_PARAMETER;
    };
    unsafe { src.sync_in() };
    let attrs = match get(attrs) {
        Some(Obj::Attributes(a)) => Some(a),
        _ => None,
    };
    let s = unit_scale(unit);
    let area = (area.0 * s, area.1 * s, area.2 * s, area.3 * s);
    if area.2 == 0.0 || area.3 == 0.0 {
        return OK;
    }
    // Source → monde : le parallélogramme (dest[0], dest[1], dest[2]).
    let (p0, p1, p2) = (dest[0], dest[1], dest[2]);
    let to_world = Matrix {
        m11: (p1.0 - p0.0) / area.2,
        m12: (p1.1 - p0.1) / area.2,
        m21: (p2.0 - p0.0) / area.3,
        m22: (p2.1 - p0.1) / area.3,
        dx: 0.0,
        dy: 0.0,
    };
    let to_world = translation(-area.0, -area.1)
        .then(&to_world)
        .then(&translation(p0.0, p0.1));
    let m = to_world.then(&gr.device());
    let quad = poly_path(
        &[
            (area.0, area.1),
            (area.0 + area.2, area.1),
            (area.0 + area.2, area.1 + area.3),
            (area.0, area.1 + area.3),
        ],
        true,
    );
    let polys = quad.fill_polys(&m);
    let inv = m.invert().unwrap_or(Matrix::IDENTITY);
    let smooth = gr.st.interp != 5;
    let src_ref = &src;
    let attrs_ref = attrs.as_ref();
    // Les images ne sont pas décalées d'un demi-pixel : on compense.
    let shift = if gr.antialias() {
        gr.pixel_shift()
    } else {
        0.0
    };
    let paint = move |x: f32, y: f32| {
        let p = inv.apply((x - shift, y - shift));
        adjust_color(sample(src_ref, p.0, p.1, smooth, area), attrs_ref)
    };
    // Rendu sans arrondi des sommets : bords nets aux entiers.
    let mut gr2 = gr.clone();
    gr2.st.smoothing = 4;
    gr2.st.offset = 4;
    render(&gr2, polys, FillRule::NonZero, &paint)
}

fn image_size(img: usize) -> Option<(f32, f32)> {
    bitmap(img).map(|b| (b.w as f32 * 96.0 / b.dpi.0, b.h as f32 * 96.0 / b.dpi.1))
}

fn full_area(img: usize) -> Option<(f32, f32, f32, f32)> {
    bitmap(img).map(|b| (0.0, 0.0, b.w as f32, b.h as f32))
}

fn rect_dest(x: f32, y: f32, w: f32, h: f32) -> [Pt; 3] {
    [(x, y), (x + w, y), (x, y + h)]
}

unsafe extern "win64" fn draw_image(g: usize, img: usize, x: f32, y: f32) -> i32 {
    let (Some((w, h)), Some(a)) = (image_size(img), full_area(img)) else {
        return INVALID_PARAMETER;
    };
    draw_image_impl(g, img, rect_dest(x, y, w, h), a, 2, 0)
}
unsafe extern "win64" fn draw_image_i(g: usize, img: usize, x: i32, y: i32) -> i32 {
    draw_image(g, img, x as f32, y as f32)
}
unsafe extern "win64" fn draw_image_rect(
    g: usize,
    img: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> i32 {
    let Some(a) = full_area(img) else {
        return INVALID_PARAMETER;
    };
    draw_image_impl(g, img, rect_dest(x, y, w, h), a, 2, 0)
}
unsafe extern "win64" fn draw_image_rect_i(
    g: usize,
    img: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> i32 {
    draw_image_rect(g, img, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn draw_image_points(g: usize, img: usize, pts: *const f32, n: i32) -> i32 {
    let (Some(a), true) = (full_area(img), n >= 3) else {
        return INVALID_PARAMETER;
    };
    let p = points_f(pts, 3);
    draw_image_impl(g, img, [p[0], p[1], p[2]], a, 2, 0)
}
unsafe extern "win64" fn draw_image_points_i(g: usize, img: usize, pts: *const i32, n: i32) -> i32 {
    let (Some(a), true) = (full_area(img), n >= 3) else {
        return INVALID_PARAMETER;
    };
    let p = points_i(pts, 3);
    draw_image_impl(g, img, [p[0], p[1], p[2]], a, 2, 0)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_point_rect(
    g: usize,
    img: usize,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
    unit: i32,
) -> i32 {
    let s = unit_scale(unit);
    draw_image_impl(
        g,
        img,
        rect_dest(x, y, sw * s, sh * s),
        (sx, sy, sw, sh),
        unit,
        0,
    )
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_point_rect_i(
    g: usize,
    img: usize,
    x: i32,
    y: i32,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    unit: i32,
) -> i32 {
    draw_image_point_rect(
        g, img, x as f32, y as f32, sx as f32, sy as f32, sw as f32, sh as f32, unit,
    )
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_rect_rect(
    g: usize,
    img: usize,
    dx: f32,
    dy: f32,
    dw: f32,
    dh: f32,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
    unit: i32,
    attrs: usize,
    _cb: usize,
    _data: usize,
) -> i32 {
    draw_image_impl(
        g,
        img,
        rect_dest(dx, dy, dw, dh),
        (sx, sy, sw, sh),
        unit,
        attrs,
    )
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_rect_rect_i(
    g: usize,
    img: usize,
    dx: i32,
    dy: i32,
    dw: i32,
    dh: i32,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    unit: i32,
    attrs: usize,
    cb: usize,
    data: usize,
) -> i32 {
    draw_image_rect_rect(
        g, img, dx as f32, dy as f32, dw as f32, dh as f32, sx as f32, sy as f32, sw as f32,
        sh as f32, unit, attrs, cb, data,
    )
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_points_rect(
    g: usize,
    img: usize,
    pts: *const f32,
    n: i32,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
    unit: i32,
    attrs: usize,
    _cb: usize,
    _data: usize,
) -> i32 {
    if n < 3 {
        return INVALID_PARAMETER;
    }
    let p = points_f(pts, 3);
    draw_image_impl(g, img, [p[0], p[1], p[2]], (sx, sy, sw, sh), unit, attrs)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_image_points_rect_i(
    g: usize,
    img: usize,
    pts: *const i32,
    n: i32,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    unit: i32,
    attrs: usize,
    _cb: usize,
    _data: usize,
) -> i32 {
    if n < 3 {
        return INVALID_PARAMETER;
    }
    let p = points_i(pts, 3);
    draw_image_impl(
        g,
        img,
        [p[0], p[1], p[2]],
        (sx as f32, sy as f32, sw as f32, sh as f32),
        unit,
        attrs,
    )
}

unsafe extern "win64" fn create_cached_bitmap(img: usize, _g: usize, out_cb: *mut usize) -> i32 {
    clone_image(img, out_cb)
}
unsafe extern "win64" fn draw_cached_bitmap(g: usize, cb: usize, x: i32, y: i32) -> i32 {
    let Some(b) = bitmap(cb) else {
        return INVALID_PARAMETER;
    };
    // Une bitmap en cache ignore la transformation, sauf la translation.
    let Some(gr) = graphics(g) else {
        return INVALID_PARAMETER;
    };
    let saved = gr.st.m;
    with_graphics(g, |gr| gr.st.m = translation(saved.dx, saved.dy));
    let r = draw_image_impl(
        g,
        cb,
        rect_dest(x as f32, y as f32, b.w as f32, b.h as f32),
        (0.0, 0.0, b.w as f32, b.h as f32),
        2,
        0,
    );
    with_graphics(g, |gr| gr.st.m = saved);
    r
}

// --- Attributs d'image ---------------------------------------------------------------

unsafe extern "win64" fn create_image_attributes(a: *mut usize) -> i32 {
    out(a, add(Obj::Attributes(Attributes::default())))
}

unsafe extern "win64" fn clone_image_attributes(a: usize, c: *mut usize) -> i32 {
    match get(a) {
        Some(o @ Obj::Attributes(_)) => out(c, add(o)),
        _ => INVALID_PARAMETER,
    }
}

fn with_attributes(a: usize, f: impl FnOnce(&mut Attributes)) -> i32 {
    with(a, |o| match o {
        Obj::Attributes(at) => {
            f(at);
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn set_color_keys(
    a: usize,
    _kind: i32,
    enable: i32,
    lo: u32,
    hi: u32,
) -> i32 {
    with_attributes(a, |at| at.key = (enable != 0).then_some((lo, hi)))
}

unsafe extern "win64" fn set_color_matrix(
    a: usize,
    _kind: i32,
    enable: i32,
    m: *const f32,
    _gray: *const f32,
    _flags: i32,
) -> i32 {
    let mm = (!m.is_null() && enable != 0).then(|| {
        let mut r = [[0f32; 5]; 5];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = *m.add(i * 5 + j);
            }
        }
        r
    });
    with_attributes(a, |at| at.matrix = mm)
}

unsafe extern "win64" fn attributes_ignored(a: usize) -> i32 {
    with_attributes(a, |_| ())
}

// --- Pinceaux ---------------------------------------------------------------------

unsafe extern "win64" fn create_solid_fill(c: u32, b: *mut usize) -> i32 {
    out(b, add(Obj::Brush(Brush::Solid(c))))
}

unsafe extern "win64" fn get_solid_fill_color(b: usize, c: *mut u32) -> i32 {
    match brush(b) {
        Some(Brush::Solid(v)) => out(c, v),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn set_solid_fill_color(b: usize, c: u32) -> i32 {
    with(b, |o| match o {
        Obj::Brush(br @ Brush::Solid(_)) => {
            *br = Brush::Solid(c);
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn create_hatch_brush(
    style: i32,
    fore: u32,
    back: u32,
    b: *mut usize,
) -> i32 {
    out(b, add(Obj::Brush(Brush::Hatch { style, fore, back })))
}

unsafe extern "win64" fn create_line_brush(
    p1: *const f32,
    p2: *const f32,
    c1: u32,
    c2: u32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if p1.is_null() || p2.is_null() {
        return INVALID_PARAMETER;
    }
    out(
        b,
        add(Obj::Brush(Brush::Linear {
            p1: (*p1, *p1.add(1)),
            p2: (*p2, *p2.add(1)),
            c1,
            c2,
            wrap,
            m: Matrix::IDENTITY,
        })),
    )
}

unsafe extern "win64" fn create_line_brush_i(
    p1: *const i32,
    p2: *const i32,
    c1: u32,
    c2: u32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if p1.is_null() || p2.is_null() {
        return INVALID_PARAMETER;
    }
    let a = [*p1 as f32, *p1.add(1) as f32];
    let z = [*p2 as f32, *p2.add(1) as f32];
    create_line_brush(a.as_ptr(), z.as_ptr(), c1, c2, wrap, b)
}

/// Dégradé sur un rectangle selon un angle (degrés, sens horaire).
unsafe fn line_from_rect(
    r: [f32; 4],
    c1: u32,
    c2: u32,
    angle: f32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    let [x, y, w, h] = r;
    let (s, c) = angle.to_radians().sin_cos();
    // Projection des coins sur la direction : le dégradé couvre le rectangle.
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let half = (w * c.abs() + h * s.abs()) / 2.0;
    let p1 = [cx - c * half, cy - s * half];
    let p2 = [cx + c * half, cy + s * half];
    create_line_brush(p1.as_ptr(), p2.as_ptr(), c1, c2, wrap, b)
}

unsafe extern "win64" fn create_line_brush_from_rect(
    r: *const f32,
    c1: u32,
    c2: u32,
    mode: i32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if r.is_null() {
        return INVALID_PARAMETER;
    }
    // Modes : 0 horizontal, 1 vertical, 2 diagonale avant, 3 arrière.
    let angle = [0.0, 90.0, 45.0, 135.0][mode.clamp(0, 3) as usize];
    line_from_rect(
        [*r, *r.add(1), *r.add(2), *r.add(3)],
        c1,
        c2,
        angle,
        wrap,
        b,
    )
}

unsafe extern "win64" fn create_line_brush_from_rect_i(
    r: *const i32,
    c1: u32,
    c2: u32,
    mode: i32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if r.is_null() {
        return INVALID_PARAMETER;
    }
    let f = [
        *r as f32,
        *r.add(1) as f32,
        *r.add(2) as f32,
        *r.add(3) as f32,
    ];
    create_line_brush_from_rect(f.as_ptr(), c1, c2, mode, wrap, b)
}

unsafe extern "win64" fn create_line_brush_angle(
    r: *const f32,
    c1: u32,
    c2: u32,
    angle: f32,
    _scalable: i32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if r.is_null() {
        return INVALID_PARAMETER;
    }
    line_from_rect(
        [*r, *r.add(1), *r.add(2), *r.add(3)],
        c1,
        c2,
        angle,
        wrap,
        b,
    )
}

unsafe extern "win64" fn create_line_brush_angle_i(
    r: *const i32,
    c1: u32,
    c2: u32,
    angle: f32,
    s: i32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    if r.is_null() {
        return INVALID_PARAMETER;
    }
    let f = [
        *r as f32,
        *r.add(1) as f32,
        *r.add(2) as f32,
        *r.add(3) as f32,
    ];
    create_line_brush_angle(f.as_ptr(), c1, c2, angle, s, wrap, b)
}

unsafe extern "win64" fn get_line_colors(b: usize, c: *mut u32) -> i32 {
    match brush(b) {
        Some(Brush::Linear { c1, c2, .. }) if !c.is_null() => {
            *c = c1;
            *c.add(1) = c2;
            OK
        }
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn set_line_colors(b: usize, a: u32, z: u32) -> i32 {
    with(b, |o| match o {
        Obj::Brush(Brush::Linear { c1, c2, .. }) => {
            *c1 = a;
            *c2 = z;
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn create_texture(img: usize, wrap: i32, b: *mut usize) -> i32 {
    let mut copy = 0usize;
    if clone_image(img, &mut copy) != OK {
        return INVALID_PARAMETER;
    }
    out(
        b,
        add(Obj::Brush(Brush::Texture {
            img: copy,
            wrap,
            m: Matrix::IDENTITY,
        })),
    )
}

unsafe extern "win64" fn create_texture2(
    img: usize,
    wrap: i32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    b: *mut usize,
) -> i32 {
    let mut copy = 0usize;
    let r = clone_bitmap_area_i(x as i32, y as i32, w as i32, h as i32, 0, img, &mut copy);
    if r != OK {
        return create_texture(img, wrap, b);
    }
    out(
        b,
        add(Obj::Brush(Brush::Texture {
            img: copy,
            wrap,
            m: Matrix::IDENTITY,
        })),
    )
}

unsafe extern "win64" fn create_texture2_i(
    img: usize,
    wrap: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    b: *mut usize,
) -> i32 {
    create_texture2(img, wrap, x as f32, y as f32, w as f32, h as f32, b)
}

unsafe extern "win64" fn create_texture_ia(
    img: usize,
    _attrs: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    b: *mut usize,
) -> i32 {
    create_texture2(img, 0, x, y, w, h, b)
}

fn brush_matrix(b: usize, f: impl FnOnce(&mut Matrix)) -> i32 {
    with(b, |o| match o {
        Obj::Brush(Brush::Linear { m, .. }) | Obj::Brush(Brush::Texture { m, .. }) => {
            f(m);
            OK
        }
        Obj::Brush(_) => OK,
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn set_brush_transform(b: usize, m: usize) -> i32 {
    let Some(Obj::Matrix(mm)) = get(m) else {
        return INVALID_PARAMETER;
    };
    brush_matrix(b, |x| *x = mm)
}
unsafe extern "win64" fn reset_brush_transform(b: usize) -> i32 {
    brush_matrix(b, |x| *x = Matrix::IDENTITY)
}
unsafe extern "win64" fn translate_brush(b: usize, dx: f32, dy: f32, order: i32) -> i32 {
    brush_matrix(b, |x| *x = combine(*x, translation(dx, dy), order))
}
unsafe extern "win64" fn scale_brush(b: usize, sx: f32, sy: f32, order: i32) -> i32 {
    brush_matrix(b, |x| *x = combine(*x, scaling(sx, sy), order))
}
unsafe extern "win64" fn rotate_brush(b: usize, deg: f32, order: i32) -> i32 {
    brush_matrix(b, |x| *x = combine(*x, rotation(deg), order))
}
unsafe extern "win64" fn brush_ignored(b: usize) -> i32 {
    match brush(b) {
        Some(_) => OK,
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn create_path_gradient_from_path(path: usize, b: *mut usize) -> i32 {
    let Some(Obj::Path(p)) = get(path) else {
        return INVALID_PARAMETER;
    };
    let Some(bb) = poly_bounds(&p.path.fill_polys(&Matrix::IDENTITY)) else {
        return INVALID_PARAMETER;
    };
    out(
        b,
        add(Obj::Brush(Brush::PathGradient {
            center: 0xFF00_0000,
            surround: 0xFFFF_FFFF,
            cx: (bb.l + bb.r) / 2.0,
            cy: (bb.t + bb.b) / 2.0,
            rx: (bb.r - bb.l) / 2.0,
            ry: (bb.b - bb.t) / 2.0,
        })),
    )
}

unsafe extern "win64" fn create_path_gradient(
    pts: *const f32,
    n: i32,
    _wrap: i32,
    b: *mut usize,
) -> i32 {
    let mut path = 0usize;
    create_path(0, &mut path);
    add_path_polygon(path, pts, n);
    let r = create_path_gradient_from_path(path, b);
    remove(path);
    r
}

unsafe extern "win64" fn create_path_gradient_i(
    pts: *const i32,
    n: i32,
    wrap: i32,
    b: *mut usize,
) -> i32 {
    let f: Vec<f32> = (0..2 * n.max(0) as usize)
        .map(|i| *pts.add(i) as f32)
        .collect();
    create_path_gradient(f.as_ptr(), n, wrap, b)
}

unsafe extern "win64" fn set_center_color(b: usize, c: u32) -> i32 {
    with(b, |o| match o {
        Obj::Brush(Brush::PathGradient { center, .. }) => {
            *center = c;
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn set_surround_colors(b: usize, c: *const u32, n: *mut i32) -> i32 {
    if c.is_null() || n.is_null() || *n < 1 {
        return INVALID_PARAMETER;
    }
    let v = *c;
    with(b, |o| match o {
        Obj::Brush(Brush::PathGradient { surround, .. }) => {
            *surround = v;
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn set_center_point(b: usize, p: *const f32) -> i32 {
    if p.is_null() {
        return INVALID_PARAMETER;
    }
    let (x, y) = (*p, *p.add(1));
    with(b, |o| match o {
        Obj::Brush(Brush::PathGradient { cx, cy, .. }) => {
            *cx = x;
            *cy = y;
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn get_brush_type(b: usize, t: *mut i32) -> i32 {
    let kind = match brush(b) {
        Some(Brush::Solid(_)) => 0,
        Some(Brush::Hatch { .. }) => 1,
        Some(Brush::Texture { .. }) => 2,
        Some(Brush::PathGradient { .. }) => 3,
        Some(Brush::Linear { .. }) => 4,
        None => return INVALID_PARAMETER,
    };
    out(t, kind)
}

unsafe extern "win64" fn clone_brush(b: usize, c: *mut usize) -> i32 {
    match get(b) {
        Some(Obj::Brush(Brush::Texture { img, wrap, m })) => {
            let mut copy = 0usize;
            clone_image(img, &mut copy);
            out(c, add(Obj::Brush(Brush::Texture { img: copy, wrap, m })))
        }
        Some(o @ Obj::Brush(_)) => out(c, add(o)),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn delete_brush(b: usize) -> i32 {
    if let Some(Brush::Texture { img, .. }) = brush(b) {
        remove(img);
    }
    remove(b)
}

// --- Stylos -----------------------------------------------------------------------

unsafe extern "win64" fn create_pen1(c: u32, width: f32, unit: i32, p: *mut usize) -> i32 {
    out(
        p,
        add(Obj::Pen(Pen {
            brush: Brush::Solid(c),
            width,
            unit,
            dash: 0,
        })),
    )
}

unsafe extern "win64" fn create_pen2(b: usize, width: f32, unit: i32, p: *mut usize) -> i32 {
    let Some(br) = brush(b) else {
        return INVALID_PARAMETER;
    };
    out(
        p,
        add(Obj::Pen(Pen {
            brush: br,
            width,
            unit,
            dash: 0,
        })),
    )
}

fn with_pen(p: usize, f: impl FnOnce(&mut Pen) -> i32) -> i32 {
    with(p, |o| match o {
        Obj::Pen(pen) => f(pen),
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

unsafe extern "win64" fn delete_pen(p: usize) -> i32 {
    remove(p)
}
unsafe extern "win64" fn clone_pen(p: usize, c: *mut usize) -> i32 {
    match get(p) {
        Some(o @ Obj::Pen(_)) => out(c, add(o)),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn set_pen_width(p: usize, w: f32) -> i32 {
    with_pen(p, |pen| {
        pen.width = w;
        OK
    })
}
unsafe extern "win64" fn get_pen_width(p: usize, w: *mut f32) -> i32 {
    match pen(p) {
        Some(pen) => out(w, pen.width),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn set_pen_color(p: usize, c: u32) -> i32 {
    with_pen(p, |pen| {
        pen.brush = Brush::Solid(c);
        OK
    })
}
unsafe extern "win64" fn get_pen_color(p: usize, c: *mut u32) -> i32 {
    match pen(p) {
        Some(Pen {
            brush: Brush::Solid(v),
            ..
        }) => out(c, v),
        Some(_) => out(c, 0xFF00_0000),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn set_pen_brush(p: usize, b: usize) -> i32 {
    let Some(br) = brush(b) else {
        return INVALID_PARAMETER;
    };
    with_pen(p, |pen| {
        pen.brush = br;
        OK
    })
}
unsafe extern "win64" fn set_pen_dash_style(p: usize, s: i32) -> i32 {
    with_pen(p, |pen| {
        pen.dash = s;
        OK
    })
}
unsafe extern "win64" fn get_pen_dash_style(p: usize, s: *mut i32) -> i32 {
    match pen(p) {
        Some(pen) => out(s, pen.dash),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_pen_unit(p: usize, u: *mut i32) -> i32 {
    match pen(p) {
        Some(pen) => out(u, pen.unit),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn pen_ignored(p: usize) -> i32 {
    with_pen(p, |_| OK)
}

// --- Matrices ----------------------------------------------------------------------

unsafe extern "win64" fn create_matrix(m: *mut usize) -> i32 {
    out(m, add(Obj::Matrix(Matrix::IDENTITY)))
}

unsafe extern "win64" fn create_matrix2(
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    dx: f32,
    dy: f32,
    m: *mut usize,
) -> i32 {
    out(
        m,
        add(Obj::Matrix(Matrix {
            m11,
            m12,
            m21,
            m22,
            dx,
            dy,
        })),
    )
}

/// GdipCreateMatrix3 : le rectangle `r` envoyé sur le parallélogramme `pts`.
unsafe extern "win64" fn create_matrix3(r: *const f32, pts: *const f32, m: *mut usize) -> i32 {
    if r.is_null() || pts.is_null() {
        return INVALID_PARAMETER;
    }
    let (x, y, w, h) = (*r, *r.add(1), *r.add(2), *r.add(3));
    if w == 0.0 || h == 0.0 {
        return INVALID_PARAMETER;
    }
    let p = points_f(pts, 3);
    let to = Matrix {
        m11: (p[1].0 - p[0].0) / w,
        m12: (p[1].1 - p[0].1) / w,
        m21: (p[2].0 - p[0].0) / h,
        m22: (p[2].1 - p[0].1) / h,
        dx: 0.0,
        dy: 0.0,
    };
    let mm = translation(-x, -y)
        .then(&to)
        .then(&translation(p[0].0, p[0].1));
    out(m, add(Obj::Matrix(mm)))
}

fn with_matrix(m: usize, f: impl FnOnce(&mut Matrix) -> i32) -> i32 {
    with(m, |o| match o {
        Obj::Matrix(mm) => f(mm),
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

fn matrix(m: usize) -> Option<Matrix> {
    match get(m)? {
        Obj::Matrix(mm) => Some(mm),
        _ => None,
    }
}

unsafe extern "win64" fn delete_matrix(m: usize) -> i32 {
    remove(m)
}
unsafe extern "win64" fn clone_matrix(m: usize, c: *mut usize) -> i32 {
    match matrix(m) {
        Some(mm) => out(c, add(Obj::Matrix(mm))),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn set_matrix_elements(
    m: usize,
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    dx: f32,
    dy: f32,
) -> i32 {
    with_matrix(m, |mm| {
        *mm = Matrix {
            m11,
            m12,
            m21,
            m22,
            dx,
            dy,
        };
        OK
    })
}
unsafe extern "win64" fn get_matrix_elements(m: usize, v: *mut f32) -> i32 {
    let (Some(mm), false) = (matrix(m), v.is_null()) else {
        return INVALID_PARAMETER;
    };
    for (i, x) in [mm.m11, mm.m12, mm.m21, mm.m22, mm.dx, mm.dy]
        .into_iter()
        .enumerate()
    {
        *v.add(i) = x;
    }
    OK
}
unsafe extern "win64" fn translate_matrix(m: usize, dx: f32, dy: f32, order: i32) -> i32 {
    with_matrix(m, |mm| {
        *mm = combine(*mm, translation(dx, dy), order);
        OK
    })
}
unsafe extern "win64" fn scale_matrix(m: usize, sx: f32, sy: f32, order: i32) -> i32 {
    with_matrix(m, |mm| {
        *mm = combine(*mm, scaling(sx, sy), order);
        OK
    })
}
unsafe extern "win64" fn rotate_matrix(m: usize, deg: f32, order: i32) -> i32 {
    with_matrix(m, |mm| {
        *mm = combine(*mm, rotation(deg), order);
        OK
    })
}
unsafe extern "win64" fn shear_matrix(m: usize, sx: f32, sy: f32, order: i32) -> i32 {
    let op = Matrix {
        m21: sx,
        m12: sy,
        ..Matrix::IDENTITY
    };
    with_matrix(m, |mm| {
        *mm = combine(*mm, op, order);
        OK
    })
}
unsafe extern "win64" fn multiply_matrix(m: usize, o: usize, order: i32) -> i32 {
    let Some(op) = matrix(o) else {
        return INVALID_PARAMETER;
    };
    with_matrix(m, |mm| {
        *mm = combine(*mm, op, order);
        OK
    })
}
unsafe extern "win64" fn invert_matrix(m: usize) -> i32 {
    with_matrix(m, |mm| match mm.invert() {
        Some(i) => {
            *mm = i;
            OK
        }
        None => INVALID_PARAMETER,
    })
}
unsafe extern "win64" fn transform_matrix_points(m: usize, pts: *mut f32, n: i32) -> i32 {
    let (Some(mm), false) = (matrix(m), pts.is_null()) else {
        return INVALID_PARAMETER;
    };
    for i in 0..n.max(0) as usize {
        let p = mm.apply((*pts.add(2 * i), *pts.add(2 * i + 1)));
        *pts.add(2 * i) = p.0;
        *pts.add(2 * i + 1) = p.1;
    }
    OK
}
unsafe extern "win64" fn transform_matrix_points_i(m: usize, pts: *mut i32, n: i32) -> i32 {
    let (Some(mm), false) = (matrix(m), pts.is_null()) else {
        return INVALID_PARAMETER;
    };
    for i in 0..n.max(0) as usize {
        let p = mm.apply((*pts.add(2 * i) as f32, *pts.add(2 * i + 1) as f32));
        *pts.add(2 * i) = p.0.round() as i32;
        *pts.add(2 * i + 1) = p.1.round() as i32;
    }
    OK
}
unsafe extern "win64" fn vector_transform_matrix_points(m: usize, pts: *mut f32, n: i32) -> i32 {
    let (Some(mut mm), false) = (matrix(m), pts.is_null()) else {
        return INVALID_PARAMETER;
    };
    mm.dx = 0.0;
    mm.dy = 0.0;
    for i in 0..n.max(0) as usize {
        let p = mm.apply((*pts.add(2 * i), *pts.add(2 * i + 1)));
        *pts.add(2 * i) = p.0;
        *pts.add(2 * i + 1) = p.1;
    }
    OK
}
unsafe extern "win64" fn is_matrix_identity(m: usize, r: *mut i32) -> i32 {
    match matrix(m) {
        Some(mm) => out(r, (mm == Matrix::IDENTITY) as i32),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn is_matrix_invertible(m: usize, r: *mut i32) -> i32 {
    match matrix(m) {
        Some(mm) => out(r, mm.invert().is_some() as i32),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn is_matrix_equal(m: usize, o: usize, r: *mut i32) -> i32 {
    match (matrix(m), matrix(o)) {
        (Some(a), Some(b)) => out(r, (a == b) as i32),
        _ => INVALID_PARAMETER,
    }
}

// --- Chemins ---------------------------------------------------------------------------

unsafe extern "win64" fn create_path(mode: i32, p: *mut usize) -> i32 {
    out(
        p,
        add(Obj::Path(GPath {
            path: Path::default(),
            fill: fill_mode(mode),
            open: false,
        })),
    )
}

/// GdipCreatePath2(points, types, count, fillMode, path).
unsafe extern "win64" fn create_path2(
    pts: *const f32,
    types: *const u8,
    n: i32,
    mode: i32,
    p: *mut usize,
) -> i32 {
    let pts = points_f(pts, n);
    if types.is_null() {
        return INVALID_PARAMETER;
    }
    let mut path = Path::default();
    let mut i = 0;
    while i < pts.len() {
        let t = *types.add(i);
        match t & 7 {
            0 => path.begin(pts[i]),
            3 if i + 2 < pts.len() => {
                path.cubic(pts[i], pts[i + 1], pts[i + 2], CURVE_SCALE);
                i += 2;
            }
            _ => path.line(pts[i]),
        }
        let last = *types.add(i);
        if last & 0x80 != 0 {
            path.close();
        }
        i += 1;
    }
    out(
        p,
        add(Obj::Path(GPath {
            path,
            fill: fill_mode(mode),
            open: false,
        })),
    )
}

unsafe extern "win64" fn create_path2_i(
    pts: *const i32,
    types: *const u8,
    n: i32,
    mode: i32,
    p: *mut usize,
) -> i32 {
    let f: Vec<f32> = (0..2 * n.max(0) as usize)
        .map(|i| *pts.add(i) as f32)
        .collect();
    create_path2(f.as_ptr(), types, n, mode, p)
}

fn with_path(p: usize, f: impl FnOnce(&mut GPath)) -> i32 {
    with(p, |o| match o {
        Obj::Path(gp) => {
            f(gp);
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

/// Ajoute une suite de points : raccordée à la figure ouverte, sinon nouvelle.
fn path_add_open(gp: &mut GPath, pts: &[Pt]) {
    if pts.is_empty() {
        return;
    }
    if gp.open && !gp.path.figures.is_empty() {
        for q in pts {
            gp.path.line(*q);
        }
    } else {
        gp.path.begin(pts[0]);
        for q in &pts[1..] {
            gp.path.line(*q);
        }
        gp.open = true;
    }
}

fn path_add_closed(gp: &mut GPath, figure: Path) {
    gp.path.append(&figure);
    gp.open = false;
}

unsafe extern "win64" fn delete_path(p: usize) -> i32 {
    remove(p)
}
unsafe extern "win64" fn clone_path(p: usize, c: *mut usize) -> i32 {
    match get(p) {
        Some(o @ Obj::Path(_)) => out(c, add(o)),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn reset_path(p: usize) -> i32 {
    with_path(p, |gp| {
        gp.path = Path::default();
        gp.open = false;
    })
}
unsafe extern "win64" fn start_figure(p: usize) -> i32 {
    with_path(p, |gp| gp.open = false)
}
unsafe extern "win64" fn close_figure(p: usize) -> i32 {
    with_path(p, |gp| {
        if gp.open {
            gp.path.close();
        }
        gp.open = false;
    })
}
unsafe extern "win64" fn close_figures(p: usize) -> i32 {
    with_path(p, |gp| {
        for f in &mut gp.path.figures {
            f.1 = true;
        }
        gp.open = false;
    })
}
unsafe extern "win64" fn set_path_fill_mode(p: usize, m: i32) -> i32 {
    with_path(p, |gp| gp.fill = fill_mode(m))
}
unsafe extern "win64" fn get_path_fill_mode(p: usize, m: *mut i32) -> i32 {
    match get(p) {
        Some(Obj::Path(gp)) => out(m, (gp.fill == FillRule::NonZero) as i32),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn add_path_line(p: usize, x1: f32, y1: f32, x2: f32, y2: f32) -> i32 {
    with_path(p, |gp| path_add_open(gp, &[(x1, y1), (x2, y2)]))
}
unsafe extern "win64" fn add_path_line_i(p: usize, x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    add_path_line(p, x1 as f32, y1 as f32, x2 as f32, y2 as f32)
}
unsafe extern "win64" fn add_path_line2(p: usize, pts: *const f32, n: i32) -> i32 {
    let v = points_f(pts, n);
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_line2_i(p: usize, pts: *const i32, n: i32) -> i32 {
    let v = points_i(pts, n);
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_arc(
    p: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    sw: f32,
) -> i32 {
    let v = arc_points(x, y, w, h, s, sw);
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_arc_i(
    p: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: f32,
    sw: f32,
) -> i32 {
    add_path_arc(p, x as f32, y as f32, w as f32, h as f32, s, sw)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn add_path_bezier(
    p: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    x4: f32,
    y4: f32,
) -> i32 {
    let b = bezier_path(&[(x1, y1), (x2, y2), (x3, y3), (x4, y4)]);
    let v = b
        .figures
        .into_iter()
        .next()
        .map(|f| f.0)
        .unwrap_or_default();
    with_path(p, |gp| path_add_open(gp, &v))
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn add_path_bezier_i(
    p: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    x3: i32,
    y3: i32,
    x4: i32,
    y4: i32,
) -> i32 {
    add_path_bezier(
        p, x1 as f32, y1 as f32, x2 as f32, y2 as f32, x3 as f32, y3 as f32, x4 as f32, y4 as f32,
    )
}
unsafe extern "win64" fn add_path_beziers(p: usize, pts: *const f32, n: i32) -> i32 {
    let b = bezier_path(&points_f(pts, n));
    let v = b
        .figures
        .into_iter()
        .next()
        .map(|f| f.0)
        .unwrap_or_default();
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_beziers_i(p: usize, pts: *const i32, n: i32) -> i32 {
    let b = bezier_path(&points_i(pts, n));
    let v = b
        .figures
        .into_iter()
        .next()
        .map(|f| f.0)
        .unwrap_or_default();
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_curve(p: usize, pts: *const f32, n: i32) -> i32 {
    let c = curve_points(&points_f(pts, n), false, 0.5);
    let v = c
        .figures
        .into_iter()
        .next()
        .map(|f| f.0)
        .unwrap_or_default();
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_curve_i(p: usize, pts: *const i32, n: i32) -> i32 {
    let c = curve_points(&points_i(pts, n), false, 0.5);
    let v = c
        .figures
        .into_iter()
        .next()
        .map(|f| f.0)
        .unwrap_or_default();
    with_path(p, |gp| path_add_open(gp, &v))
}
unsafe extern "win64" fn add_path_closed_curve(p: usize, pts: *const f32, n: i32) -> i32 {
    let c = curve_points(&points_f(pts, n), true, 0.5);
    with_path(p, |gp| path_add_closed(gp, c))
}
unsafe extern "win64" fn add_path_closed_curve_i(p: usize, pts: *const i32, n: i32) -> i32 {
    let c = curve_points(&points_i(pts, n), true, 0.5);
    with_path(p, |gp| path_add_closed(gp, c))
}
unsafe extern "win64" fn add_path_rectangle(p: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    with_path(p, |gp| path_add_closed(gp, rect_path(x, y, w, h)))
}
unsafe extern "win64" fn add_path_rectangle_i(p: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    add_path_rectangle(p, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn add_path_rectangles(p: usize, r: *const f32, n: i32) -> i32 {
    for i in 0..n.max(0) as usize {
        let q = r.add(4 * i);
        add_path_rectangle(p, *q, *q.add(1), *q.add(2), *q.add(3));
    }
    OK
}
unsafe extern "win64" fn add_path_rectangles_i(p: usize, r: *const i32, n: i32) -> i32 {
    for i in 0..n.max(0) as usize {
        let q = r.add(4 * i);
        add_path_rectangle_i(p, *q, *q.add(1), *q.add(2), *q.add(3));
    }
    OK
}
unsafe extern "win64" fn add_path_ellipse(p: usize, x: f32, y: f32, w: f32, h: f32) -> i32 {
    with_path(p, |gp| path_add_closed(gp, ellipse_path(x, y, w, h)))
}
unsafe extern "win64" fn add_path_ellipse_i(p: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    add_path_ellipse(p, x as f32, y as f32, w as f32, h as f32)
}
unsafe extern "win64" fn add_path_pie(
    p: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    sw: f32,
) -> i32 {
    let mut pts = vec![(x + w / 2.0, y + h / 2.0)];
    pts.extend(arc_points(x, y, w, h, s, sw));
    with_path(p, |gp| path_add_closed(gp, poly_path(&pts, true)))
}
unsafe extern "win64" fn add_path_pie_i(
    p: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: f32,
    sw: f32,
) -> i32 {
    add_path_pie(p, x as f32, y as f32, w as f32, h as f32, s, sw)
}
unsafe extern "win64" fn add_path_polygon(p: usize, pts: *const f32, n: i32) -> i32 {
    let v = points_f(pts, n);
    with_path(p, |gp| path_add_closed(gp, poly_path(&v, true)))
}
unsafe extern "win64" fn add_path_polygon_i(p: usize, pts: *const i32, n: i32) -> i32 {
    let v = points_i(pts, n);
    with_path(p, |gp| path_add_closed(gp, poly_path(&v, true)))
}
unsafe extern "win64" fn add_path_path(p: usize, o: usize, connect: i32) -> i32 {
    let Some(Obj::Path(other)) = get(o) else {
        return INVALID_PARAMETER;
    };
    with_path(p, |gp| {
        let mut figs = other.path.figures.clone().into_iter();
        if connect != 0 && gp.open {
            if let Some((pts, closed)) = figs.next() {
                for q in &pts {
                    gp.path.line(*q);
                }
                if closed {
                    gp.path.close();
                }
            }
        }
        gp.path.figures.extend(figs);
        gp.open = other.open;
    })
}
unsafe extern "win64" fn get_point_count(p: usize, n: *mut i32) -> i32 {
    match get(p) {
        Some(Obj::Path(gp)) => out(
            n,
            gp.path.figures.iter().map(|f| f.0.len()).sum::<usize>() as i32,
        ),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_path_points(p: usize, pts: *mut f32, n: i32) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let all: Vec<Pt> = gp
        .path
        .figures
        .iter()
        .flat_map(|f| f.0.iter().copied())
        .collect();
    if pts.is_null() || (n as usize) < all.len() {
        return INSUFFICIENT_BUFFER;
    }
    for (i, q) in all.iter().enumerate() {
        *pts.add(2 * i) = q.0;
        *pts.add(2 * i + 1) = q.1;
    }
    OK
}
/// Points du chemin arrondis à l'entier (GpPoint).
unsafe extern "win64" fn get_path_points_i(p: usize, pts: *mut i32, n: i32) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let all: Vec<Pt> = gp
        .path
        .figures
        .iter()
        .flat_map(|f| f.0.iter().copied())
        .collect();
    if pts.is_null() || (n as usize) < all.len() {
        return INSUFFICIENT_BUFFER;
    }
    for (i, q) in all.iter().enumerate() {
        *pts.add(2 * i) = q.0.round() as i32;
        *pts.add(2 * i + 1) = q.1.round() as i32;
    }
    OK
}

/// Dernier point du chemin. wxWidgets y lit son « point courant » : sans
/// lui, chaque arc d'un rectangle arrondi ouvrait une nouvelle figure et
/// seuls les quatre coins étaient remplis (boutons d'Audacity, #1054).
unsafe extern "win64" fn get_path_last_point(p: usize, last: *mut f32) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let Some(q) = gp
        .path
        .figures
        .iter()
        .rev()
        .find_map(|f| f.0.last().copied())
    else {
        return INVALID_PARAMETER;
    };
    if last.is_null() {
        return INVALID_PARAMETER;
    }
    *last = q.0;
    *last.add(1) = q.1;
    OK
}

unsafe extern "win64" fn get_path_types(p: usize, types: *mut u8, n: i32) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let mut v = Vec::new();
    for (pts, closed) in &gp.path.figures {
        for i in 0..pts.len() {
            let mut t = if i == 0 { 0u8 } else { 1 };
            if *closed && i + 1 == pts.len() {
                t |= 0x80;
            }
            v.push(t);
        }
    }
    if types.is_null() || (n as usize) < v.len() {
        return INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), types, v.len());
    OK
}
unsafe extern "win64" fn get_path_bounds(p: usize, rect: *mut f32, m: usize, pen_h: usize) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let mm = matrix(m).unwrap_or(Matrix::IDENTITY);
    let Some(mut b) = poly_bounds(&gp.path.fill_polys(&mm)) else {
        return put_rect(rect, [0.0; 4]);
    };
    if let Some(pen) = pen(pen_h) {
        let h = pen.width / 2.0;
        b = Clip {
            l: b.l - h,
            t: b.t - h,
            r: b.r + h,
            b: b.b + h,
        };
    }
    put_rect(rect, [b.l, b.t, b.r - b.l, b.b - b.t])
}
unsafe extern "win64" fn get_path_bounds_i(
    p: usize,
    rect: *mut i32,
    m: usize,
    pen_h: usize,
) -> i32 {
    let mut f = [0f32; 4];
    let r = get_path_bounds(p, f.as_mut_ptr(), m, pen_h);
    if r == OK && !rect.is_null() {
        for (i, v) in f.iter().enumerate() {
            *rect.add(i) = v.round() as i32;
        }
    }
    r
}
unsafe extern "win64" fn transform_path(p: usize, m: usize) -> i32 {
    let Some(mm) = matrix(m) else {
        return if m == 0 { OK } else { INVALID_PARAMETER };
    };
    with_path(p, |gp| {
        for f in &mut gp.path.figures {
            for q in &mut f.0 {
                *q = mm.apply(*q);
            }
        }
    })
}
unsafe extern "win64" fn is_visible_path_point(
    p: usize,
    x: f32,
    y: f32,
    _g: usize,
    r: *mut i32,
) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    let mut inside = false;
    for poly in gp.path.fill_polys(&Matrix::IDENTITY) {
        let n = poly.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let (a, b) = (poly[i], poly[j]);
            if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
                inside = !inside;
            }
            j = i;
        }
    }
    out(r, inside as i32)
}
unsafe extern "win64" fn is_visible_path_point_i(
    p: usize,
    x: i32,
    y: i32,
    g: usize,
    r: *mut i32,
) -> i32 {
    is_visible_path_point(p, x as f32, y as f32, g, r)
}
unsafe extern "win64" fn path_ignored(p: usize) -> i32 {
    with_path(p, |_| ())
}

// --- Régions (rectangles) -----------------------------------------------------------

unsafe extern "win64" fn create_region(r: *mut usize) -> i32 {
    out(r, add(Obj::Region(None)))
}
unsafe extern "win64" fn create_region_rect(rect: *const f32, r: *mut usize) -> i32 {
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    let (x, y, w, h) = (*rect, *rect.add(1), *rect.add(2), *rect.add(3));
    out(
        r,
        add(Obj::Region(Some(Clip {
            l: x,
            t: y,
            r: x + w,
            b: y + h,
        }))),
    )
}
unsafe extern "win64" fn create_region_rect_i(rect: *const i32, r: *mut usize) -> i32 {
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    let f = [
        *rect as f32,
        *rect.add(1) as f32,
        *rect.add(2) as f32,
        *rect.add(3) as f32,
    ];
    create_region_rect(f.as_ptr(), r)
}
unsafe extern "win64" fn create_region_path(p: usize, r: *mut usize) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    out(
        r,
        add(Obj::Region(poly_bounds(
            &gp.path.fill_polys(&Matrix::IDENTITY),
        ))),
    )
}
unsafe extern "win64" fn create_region_hrgn(hrgn: usize, r: *mut usize) -> i32 {
    let mut b = [0i32; 4];
    if api!(
        "gdi32",
        "GetRgnBox",
        fn(usize, *mut i32) -> i32,
        hrgn,
        b.as_mut_ptr()
    )
    .unwrap_or(0)
        == 0
    {
        return INVALID_PARAMETER;
    }
    out(
        r,
        add(Obj::Region(Some(Clip {
            l: b[0] as f32,
            t: b[1] as f32,
            r: b[2] as f32,
            b: b[3] as f32,
        }))),
    )
}
unsafe extern "win64" fn delete_region(r: usize) -> i32 {
    remove(r)
}
unsafe extern "win64" fn clone_region(r: usize, c: *mut usize) -> i32 {
    match get(r) {
        Some(o @ Obj::Region(_)) => out(c, add(o)),
        _ => INVALID_PARAMETER,
    }
}
fn with_region(r: usize, f: impl FnOnce(&mut Option<Clip>)) -> i32 {
    with(r, |o| match o {
        Obj::Region(c) => {
            f(c);
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}
unsafe extern "win64" fn set_infinite(r: usize) -> i32 {
    with_region(r, |c| *c = None)
}
unsafe extern "win64" fn set_empty(r: usize) -> i32 {
    with_region(r, |c| {
        *c = Some(Clip {
            l: 0.0,
            t: 0.0,
            r: 0.0,
            b: 0.0,
        })
    })
}
fn combine_region(r: usize, other: Option<Clip>, mode: i32) -> i32 {
    with_region(r, |c| {
        *c = match (mode, *c, other) {
            (0, _, o) => o,
            (1, Some(a), Some(b)) => Some(intersect(a, b)),
            (1, None, o) => o,
            (2, Some(a), Some(b)) => Some(union(a, b)),
            (2, _, _) => None,
            (_, a, _) => a,
        }
    })
}
unsafe extern "win64" fn combine_region_rect(r: usize, rect: *const f32, mode: i32) -> i32 {
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    let (x, y, w, h) = (*rect, *rect.add(1), *rect.add(2), *rect.add(3));
    combine_region(
        r,
        Some(Clip {
            l: x,
            t: y,
            r: x + w,
            b: y + h,
        }),
        mode,
    )
}
unsafe extern "win64" fn combine_region_rect_i(r: usize, rect: *const i32, mode: i32) -> i32 {
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    let f = [
        *rect as f32,
        *rect.add(1) as f32,
        *rect.add(2) as f32,
        *rect.add(3) as f32,
    ];
    combine_region_rect(r, f.as_ptr(), mode)
}
unsafe extern "win64" fn combine_region_region(r: usize, o: usize, mode: i32) -> i32 {
    match get(o) {
        Some(Obj::Region(c)) => combine_region(r, c, mode),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn combine_region_path(r: usize, p: usize, mode: i32) -> i32 {
    let Some(Obj::Path(gp)) = get(p) else {
        return INVALID_PARAMETER;
    };
    combine_region(r, poly_bounds(&gp.path.fill_polys(&Matrix::IDENTITY)), mode)
}
unsafe extern "win64" fn get_region_bounds(r: usize, _g: usize, rect: *mut f32) -> i32 {
    match get(r) {
        Some(Obj::Region(Some(c))) => put_rect(rect, [c.l, c.t, c.r - c.l, c.b - c.t]),
        Some(Obj::Region(None)) => {
            put_rect(rect, [-4_194_304.0, -4_194_304.0, 8_388_608.0, 8_388_608.0])
        }
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_region_bounds_i(r: usize, g: usize, rect: *mut i32) -> i32 {
    let mut f = [0f32; 4];
    let res = get_region_bounds(r, g, f.as_mut_ptr());
    if res == OK && !rect.is_null() {
        for (i, v) in f.iter().enumerate() {
            *rect.add(i) = v.round() as i32;
        }
    }
    res
}
unsafe extern "win64" fn is_empty_region(r: usize, _g: usize, res: *mut i32) -> i32 {
    match get(r) {
        Some(Obj::Region(c)) => out(res, c.is_some_and(|c| c.l >= c.r || c.t >= c.b) as i32),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn is_infinite_region(r: usize, _g: usize, res: *mut i32) -> i32 {
    match get(r) {
        Some(Obj::Region(c)) => out(res, c.is_none() as i32),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_region_hrgn(r: usize, _g: usize, hrgn: *mut usize) -> i32 {
    match get(r) {
        Some(Obj::Region(Some(c))) => {
            let h = api!(
                "gdi32",
                "CreateRectRgn",
                fn(i32, i32, i32, i32) -> usize,
                c.l.floor() as i32,
                c.t.floor() as i32,
                c.r.ceil() as i32,
                c.b.ceil() as i32
            )
            .unwrap_or(0);
            out(hrgn, h)
        }
        Some(Obj::Region(None)) => out(hrgn, 0),
        _ => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn translate_region(r: usize, dx: f32, dy: f32) -> i32 {
    with_region(r, |c| {
        if let Some(c) = c {
            c.l += dx;
            c.r += dx;
            c.t += dy;
            c.b += dy;
        }
    })
}
unsafe extern "win64" fn translate_region_i(r: usize, dx: i32, dy: i32) -> i32 {
    translate_region(r, dx as f32, dy as f32)
}
unsafe extern "win64" fn is_visible_region_point(
    r: usize,
    x: f32,
    y: f32,
    _g: usize,
    res: *mut i32,
) -> i32 {
    match get(r) {
        Some(Obj::Region(c)) => out(
            res,
            c.is_none_or(|c| x >= c.l && x < c.r && y >= c.t && y < c.b) as i32,
        ),
        _ => INVALID_PARAMETER,
    }
}

// --- Polices et texte (gdiplus_text.rs) ----------------------------------------------

pub(crate) fn font(h: usize) -> Option<Font> {
    match get(h)? {
        Obj::Font(f) => Some(f),
        _ => None,
    }
}

pub(crate) fn string_format(h: usize) -> Option<StringFormat> {
    match get(h)? {
        Obj::Format(f) => Some(f),
        _ => None,
    }
}

/// Description GDI d'une police GDI+ à `em_px` pixels par em.
pub(crate) fn font_spec(f: &Font, em_px: f32) -> FontSpec {
    FontSpec {
        height: -(em_px.round().max(1.0) as i32),
        width: 0,
        weight: if f.style & 1 != 0 { 700 } else { 400 },
        italic: f.style & 2 != 0,
        underline: f.style & 4 != 0,
        strikeout: f.style & 8 != 0,
        face: f.family.clone(),
    }
}

/// Taille de l'em en unités du monde du Graphics (`gr`), ou en pixels.
pub(crate) fn font_em(f: &Font, gr: Option<&Graphics>) -> f32 {
    if f.unit == 0 {
        return f.size;
    }
    let px = f.size * unit_scale(f.unit);
    match gr {
        Some(g) => {
            let page = unit_scale(g.st.unit) * if g.st.unit > 1 { g.st.scale } else { 1.0 };
            px / page.max(1e-6)
        }
        None => px,
    }
}

/// Le texte est dessiné glyphe par glyphe, en couverture.
pub(crate) fn draw_glyphs(
    g: usize,
    b: usize,
    glyphs: &[(f32, f32, std::sync::Arc<super::usp10::Raster>)],
    rects: &[(f32, f32, f32, f32)],
    clip_world: Option<[f32; 4]>,
) -> i32 {
    let (Some(gr), Some(br)) = (graphics(g), brush(b)) else {
        return INVALID_PARAMETER;
    };
    let device = gr.device();
    let tex = texture_of(&br);
    let paint = brush_paint(&br, device, tex.as_ref());
    let bounds = target_bounds(&gr);
    let mut clip = intersect(bounds, gr.st.clip.unwrap_or(bounds));
    if let Some(c) = clip_world {
        clip = intersect(clip, device_box(&device, c[0], c[1], c[2], c[3]));
    }
    // Boîte des glyphes en pixels.
    let mut bb: Option<Clip> = None;
    for (x, y, r) in glyphs {
        let p = device.apply((*x, *y));
        let c = Clip {
            l: p.0.floor(),
            t: p.1.floor(),
            r: p.0.floor() + r.w as f32,
            b: p.1.floor() + r.h as f32,
        };
        bb = Some(bb.map_or(c, |b| union(b, c)));
    }
    for &(x, y, w, h) in rects {
        let c = device_box(&device, x, y, w, h);
        bb = Some(bb.map_or(c, |b| union(b, c)));
    }
    let Some(bb) = bb else {
        return OK;
    };
    let area = intersect(clip, bb);
    let (x0, y0) = (
        area.l.floor().max(0.0) as i32,
        area.t.floor().max(0.0) as i32,
    );
    let (w, h) = (
        (area.r.ceil() as i32 - x0).max(0) as u32,
        (area.b.ceil() as i32 - y0).max(0) as u32,
    );
    if w == 0 || h == 0 || w as u64 * h as u64 > 4096 * 4096 {
        return OK;
    }
    let mut px = match gr.target {
        Target::Bitmap(img) => {
            let Some(mut bm) = bitmap(img) else {
                return INVALID_PARAMETER;
            };
            unsafe { bm.sync_in() };
            let mut v = vec![0u8; w as usize * h as usize * 4];
            for row in 0..h as usize {
                let sy = y0 as usize + row;
                if sy >= bm.h as usize {
                    break;
                }
                let n = (w as usize).min(bm.w as usize - x0 as usize);
                let s = (sy * bm.w as usize + x0 as usize) * 4;
                v[row * w as usize * 4..][..n * 4].copy_from_slice(&bm.px[s..s + n * 4]);
            }
            v
        }
        Target::Hdc(_) => vec![0u8; w as usize * h as usize * 4],
    };
    let mut blend = |dx: i32, dy: i32, cov: f32| {
        if dx < 0 || dy < 0 || dx >= w as i32 || dy >= h as i32 || cov <= 0.0 {
            return;
        }
        let c = paint((dx + x0) as f32 + 0.5, (dy + y0) as f32 + 0.5);
        let o = (dy as usize * w as usize + dx as usize) * 4;
        let sa = c[3] * cov;
        let inv = 1.0 - sa;
        let s = [c[2] * cov, c[1] * cov, c[0] * cov, sa];
        for i in 0..4 {
            px[o + i] = (s[i] * 255.0 + px[o + i] as f32 * inv + 0.5).min(255.0) as u8;
        }
    };
    for (x, y, r) in glyphs {
        let p = device.apply((*x, *y));
        let (gx, gy) = (p.0.floor() as i32 - x0, p.1.floor() as i32 - y0);
        for yy in 0..r.h {
            for xx in 0..r.w {
                let cov = r.cov[yy * r.w + xx];
                if cov != 0 {
                    blend(gx + xx as i32, gy + yy as i32, cov as f32 / 255.0);
                }
            }
        }
    }
    for &(x, y, rw, rh) in rects {
        let c = device_box(&device, x, y, rw, rh);
        for yy in c.t.round() as i32..c.b.round().max(c.t.round() + 1.0) as i32 {
            for xx in c.l.round() as i32..c.r.round() as i32 {
                blend(xx - x0, yy - y0, 1.0);
            }
        }
    }
    match gr.target {
        Target::Bitmap(img) => {
            with_bitmap(img, |bm| {
                for row in 0..h as usize {
                    let sy = y0 as usize + row;
                    if sy >= bm.h as usize {
                        break;
                    }
                    let n = (w as usize).min(bm.w as usize - x0 as usize);
                    let d = (sy * bm.w as usize + x0 as usize) * 4;
                    bm.px[d..d + n * 4].copy_from_slice(&px[row * w as usize * 4..][..n * 4]);
                }
                unsafe { bm.sync_out() };
            });
        }
        Target::Hdc(hdc) => present(hdc, x0, y0, w, h, &px),
    }
    OK
}

// --- Exports ---------------------------------------------------------------------------

/// Fonctions de l'API plate que Winlux ne fait pas encore : présentes (les
/// programmes comme wxWidgets résolvent toute la table avant de s'en
/// servir), elles répondent NotImplemented.
const NOT_YET: &[&str] = &[
    "GdipAddPathClosedCurve2",
    "GdipAddPathClosedCurve2I",
    "GdipAddPathCurve2",
    "GdipAddPathCurve2I",
    "GdipAddPathCurve3",
    "GdipAddPathCurve3I",
    "GdipAddPathString",
    "GdipAddPathStringI",
    "GdipBeginContainer",
    "GdipBeginContainerI",
    "GdipClearPathMarkers",
    "GdipCloneCustomLineCap",
    "GdipComment",
    "GdipCreateAdjustableArrowCap",
    "GdipCreateBitmapFromDirectDrawSurface",
    "GdipCreateCustomLineCap",
    "GdipCreateMatrix3I",
    "GdipCreateMetafileFromEmf",
    "GdipCreateMetafileFromFile",
    "GdipCreateMetafileFromStream",
    "GdipCreateMetafileFromWmf",
    "GdipCreateMetafileFromWmfFile",
    "GdipCreatePathIter",
    "GdipCreateRegionRgnData",
    "GdipCreateStreamOnFile",
    "GdipCreateTextureIAI",
    "GdipDeleteCustomLineCap",
    "GdipDeletePathIter",
    "GdipDrawClosedCurve2",
    "GdipDrawClosedCurve2I",
    "GdipDrawCurve3",
    "GdipDrawCurve3I",
    "GdipDrawDriverString",
    "GdipEnumerateMetafileDestPoint",
    "GdipEnumerateMetafileDestPointI",
    "GdipEnumerateMetafileDestPoints",
    "GdipEnumerateMetafileDestPointsI",
    "GdipEnumerateMetafileDestRect",
    "GdipEnumerateMetafileDestRectI",
    "GdipEnumerateMetafileSrcRectDestPoint",
    "GdipEnumerateMetafileSrcRectDestPointI",
    "GdipEnumerateMetafileSrcRectDestPoints",
    "GdipEnumerateMetafileSrcRectDestPointsI",
    "GdipEnumerateMetafileSrcRectDestRect",
    "GdipEnumerateMetafileSrcRectDestRectI",
    "GdipFillClosedCurve2",
    "GdipFillClosedCurve2I",
    "GdipGetAdjustableArrowCapFillState",
    "GdipGetAdjustableArrowCapHeight",
    "GdipGetAdjustableArrowCapMiddleInset",
    "GdipGetAdjustableArrowCapWidth",
    "GdipGetCustomLineCapBaseCap",
    "GdipGetCustomLineCapBaseInset",
    "GdipGetCustomLineCapStrokeCaps",
    "GdipGetCustomLineCapStrokeJoin",
    "GdipGetCustomLineCapType",
    "GdipGetCustomLineCapWidthScale",
    "GdipGetHatchBackgroundColor",
    "GdipGetHatchForegroundColor",
    "GdipGetHatchStyle",
    "GdipGetHemfFromMetafile",
    "GdipGetImageAttributesAdjustedPalette",
    "GdipGetLineBlend",
    "GdipGetLineBlendCount",
    "GdipGetLineGammaCorrection",
    "GdipGetLinePresetBlend",
    "GdipGetLinePresetBlendCount",
    "GdipGetLineRect",
    "GdipGetLineRectI",
    "GdipGetLineTransform",
    "GdipGetLineWrapMode",
    "GdipGetMetafileDownLevelRasterizationLimit",
    "GdipGetMetafileHeaderFromEmf",
    "GdipGetMetafileHeaderFromFile",
    "GdipGetMetafileHeaderFromMetafile",
    "GdipGetMetafileHeaderFromStream",
    "GdipGetNearestColor",
    "GdipGetPathData",
    "GdipGetPathGradientBlend",
    "GdipGetPathGradientBlendCount",
    "GdipGetPathGradientCenterColor",
    "GdipGetPathGradientCenterPoint",
    "GdipGetPathGradientCenterPointI",
    "GdipGetPathGradientFocusScales",
    "GdipGetPathGradientGammaCorrection",
    "GdipGetPathGradientPath",
    "GdipGetPathGradientPointCount",
    "GdipGetPathGradientPresetBlend",
    "GdipGetPathGradientPresetBlendCount",
    "GdipGetPathGradientRect",
    "GdipGetPathGradientRectI",
    "GdipGetPathGradientSurroundColorCount",
    "GdipGetPathGradientSurroundColorsWithCount",
    "GdipGetPathGradientTransform",
    "GdipGetPathGradientWrapMode",
    "GdipGetPenBrushFill",
    "GdipGetPenCompoundArray",
    "GdipGetPenCompoundCount",
    "GdipGetPenCustomEndCap",
    "GdipGetPenCustomStartCap",
    "GdipGetPenDashArray",
    "GdipGetPenDashCap197819",
    "GdipGetPenDashCount",
    "GdipGetPenDashOffset",
    "GdipGetPenEndCap",
    "GdipGetPenFillType",
    "GdipGetPenLineJoin",
    "GdipGetPenMiterLimit",
    "GdipGetPenMode",
    "GdipGetPenStartCap",
    "GdipGetPenTransform",
    "GdipGetRegionData",
    "GdipGetRegionDataSize",
    "GdipGetRegionScans",
    "GdipGetRegionScansCount",
    "GdipGetRegionScansI",
    "GdipGetRenderingOrigin",
    "GdipGetStringFormatDigitSubstitution",
    "GdipGetStringFormatTabStops",
    "GdipGetTextContrast",
    "GdipGetTextureImage",
    "GdipGetTextureTransform",
    "GdipGetTextureWrapMode",
    "GdipIsEqualRegion",
    "GdipIsOutlineVisiblePathPoint",
    "GdipIsOutlineVisiblePathPointI",
    "GdipIsVisiblePointI",
    "GdipIsVisibleRectI",
    "GdipIsVisibleRegionPointI",
    "GdipIsVisibleRegionRect",
    "GdipIsVisibleRegionRectI",
    "GdipMeasureDriverString",
    "GdipMultiplyLineTransform",
    "GdipMultiplyPathGradientTransform",
    "GdipMultiplyPenTransform",
    "GdipMultiplyTextureTransform",
    "GdipPathIterCopyData",
    "GdipPathIterEnumerate",
    "GdipPathIterGetCount",
    "GdipPathIterGetSubpathCount",
    "GdipPathIterHasCurve",
    "GdipPathIterIsValid",
    "GdipPathIterNextMarker",
    "GdipPathIterNextMarkerPath",
    "GdipPathIterNextPathType",
    "GdipPathIterNextSubpath",
    "GdipPathIterNextSubpathPath",
    "GdipPathIterRewind",
    "GdipPlayMetafileRecord",
    "GdipRecordMetafile",
    "GdipRecordMetafileFileName",
    "GdipRecordMetafileFileNameI",
    "GdipRecordMetafileI",
    "GdipRecordMetafileStream",
    "GdipRecordMetafileStreamI",
    "GdipResetPageTransform",
    "GdipResetPathGradientTransform",
    "GdipRotatePathGradientTransform",
    "GdipRotatePenTransform",
    "GdipSaveAdd",
    "GdipSaveAddImage",
    "GdipScalePathGradientTransform",
    "GdipScalePenTransform",
    "GdipSetAdjustableArrowCapFillState",
    "GdipSetAdjustableArrowCapHeight",
    "GdipSetAdjustableArrowCapMiddleInset",
    "GdipSetAdjustableArrowCapWidth",
    "GdipSetCustomLineCapBaseCap",
    "GdipSetCustomLineCapBaseInset",
    "GdipSetCustomLineCapStrokeCaps",
    "GdipSetCustomLineCapStrokeJoin",
    "GdipSetCustomLineCapWidthScale",
    "GdipSetImageAttributesCachedBackground",
    "GdipSetImageAttributesOutputChannel",
    "GdipSetImageAttributesOutputChannelColorProfile",
    "GdipSetImageAttributesToIdentity",
    "GdipSetMetafileDownLevelRasterizationLimit",
    "GdipSetPathGradientCenterPointI",
    "GdipSetPathGradientPath",
    "GdipSetPathGradientTransform",
    "GdipSetPathMarker",
    "GdipTestControl",
    "GdipTransformRegion",
    "GdipTranslatePathGradientTransform",
    "GdipTranslatePenTransform",
    "GdipVectorTransformMatrixPointsI",
    "GdipWarpPath",
    "GdipWindingModeOutline",
];

unsafe extern "win64" fn not_yet() -> i32 {
    NOT_IMPLEMENTED
}

pub fn exports() -> Exports {
    let mut e = crate::exports! {
        "GdiplusStartup" => fn startup,
        "GdiplusShutdown" => fn shutdown,
        "GdipAlloc" => fn gdip_alloc,
        "GdipFree" => fn gdip_free,
        "GdiplusNotificationHook" => fn hook_stub,
        "GdiplusNotificationUnhook" => fn unhook_stub,
        // Images
        "GdipLoadImageFromFile" => fn load_image_from_file,
        "GdipLoadImageFromFileICM" => fn load_image_from_file,
        "GdipCreateBitmapFromFile" => fn load_image_from_file,
        "GdipCreateBitmapFromFileICM" => fn load_image_from_file,
        "GdipLoadImageFromStream" => fn load_image_from_stream,
        "GdipLoadImageFromStreamICM" => fn load_image_from_stream,
        "GdipCreateBitmapFromStream" => fn load_image_from_stream,
        "GdipCreateBitmapFromStreamICM" => fn load_image_from_stream,
        "GdipCreateBitmapFromScan0" => fn create_bitmap_from_scan0,
        "GdipCreateBitmapFromGraphics" => fn create_bitmap_from_graphics,
        "GdipCreateBitmapFromHBITMAP" => fn create_bitmap_from_hbitmap,
        "GdipCreateBitmapFromHICON" => fn create_bitmap_from_hicon,
        "GdipCreateBitmapFromGdiDib" => fn create_bitmap_from_gdi_dib,
        "GdipCreateBitmapFromResource" => fn create_bitmap_from_resource,
        "GdipCreateHBITMAPFromBitmap" => fn create_hbitmap_from_bitmap,
        "GdipCreateHICONFromBitmap" => fn create_hicon_from_bitmap,
        "GdipDisposeImage" => fn dispose_image,
        "GdipCloneImage" => fn clone_image,
        "GdipCloneBitmapArea" => fn clone_bitmap_area,
        "GdipCloneBitmapAreaI" => fn clone_bitmap_area_i,
        "GdipGetImageWidth" => fn get_image_width,
        "GdipImageForceValidation" => fn image_force_validation,
        "GdipGetImageHeight" => fn get_image_height,
        "GdipGetImageDimension" => fn get_image_dimension,
        "GdipGetImageBounds" => fn get_image_bounds,
        "GdipGetImagePixelFormat" => fn get_image_pixel_format,
        "GdipGetImageType" => fn get_image_type,
        "GdipGetImageFlags" => fn get_image_flags,
        "GdipGetImageRawFormat" => fn get_image_raw_format,
        "GdipGetImageHorizontalResolution" => fn get_resolution_x,
        "GdipGetImageVerticalResolution" => fn get_resolution_y,
        "GdipBitmapSetResolution" => fn bitmap_set_resolution,
        "GdipGetImagePaletteSize" => fn get_palette_size,
        "GdipGetImagePalette" => fn get_palette,
        "GdipSetImagePalette" => fn ok_stub,
        "GdipBitmapGetPixel" => fn get_pixel,
        "GdipBitmapSetPixel" => fn set_pixel,
        "GdipBitmapLockBits" => fn lock_bits,
        "GdipBitmapUnlockBits" => fn unlock_bits,
        "GdipImageRotateFlip" => fn rotate_flip,
        "GdipGetImageThumbnail" => fn get_thumbnail,
        "GdipImageGetFrameDimensionsCount" => fn frame_dims_count,
        "GdipImageGetFrameDimensionsList" => fn frame_dims_list,
        "GdipImageGetFrameCount" => fn frame_count,
        "GdipImageSelectActiveFrame" => fn select_active_frame,
        "GdipGetPropertyCount" => fn property_count,
        "GdipGetPropertyIdList" => fn property_id_list,
        "GdipGetPropertyItemSize" => fn property_item_size,
        "GdipGetPropertyItem" => fn property_item,
        "GdipGetPropertySize" => fn property_size,
        "GdipGetAllPropertyItems" => fn all_property_items,
        "GdipRemovePropertyItem" => fn ok_stub,
        "GdipSetPropertyItem" => fn ok_stub,
        "GdipGetImageEncodersSize" => fn encoders_size,
        "GdipGetImageEncoders" => fn get_encoders,
        "GdipGetImageDecodersSize" => fn decoders_size,
        "GdipGetImageDecoders" => fn get_decoders,
        "GdipGetEncoderParameterListSize" => fn not_implemented,
        "GdipGetEncoderParameterList" => fn not_implemented,
        "GdipSaveImageToFile" => fn save_image_to_file,
        "GdipSaveImageToStream" => fn save_image_to_stream,
        "GdipCreateCachedBitmap" => fn create_cached_bitmap,
        "GdipDeleteCachedBitmap" => fn dispose_image,
        "GdipDrawCachedBitmap" => fn draw_cached_bitmap,
        // Graphics
        "GdipCreateFromHDC" => fn create_from_hdc,
        "GdipCreateFromHDC2" => fn create_from_hdc2,
        "GdipCreateFromHWND" => fn create_from_hwnd,
        "GdipCreateFromHWNDICM" => fn create_from_hwnd,
        "GdipGetImageGraphicsContext" => fn image_graphics,
        "GdipDeleteGraphics" => fn delete_graphics,
        "GdipGetDC" => fn get_dc,
        "GdipReleaseDC" => fn release_dc,
        "GdipFlush" => fn set_ignored,
        "GdipSetSmoothingMode" => fn set_smoothing,
        "GdipGetSmoothingMode" => fn get_smoothing,
        "GdipSetPixelOffsetMode" => fn set_pixel_offset,
        "GdipGetPixelOffsetMode" => fn get_pixel_offset,
        "GdipSetInterpolationMode" => fn set_interpolation,
        "GdipGetInterpolationMode" => fn get_interpolation,
        "GdipSetTextRenderingHint" => fn set_text_hint,
        "GdipGetTextRenderingHint" => fn get_text_hint,
        "GdipSetPageUnit" => fn set_page_unit,
        "GdipGetPageUnit" => fn get_page_unit,
        "GdipSetPageScale" => fn set_page_scale,
        "GdipGetPageScale" => fn get_page_scale,
        "GdipSetCompositingMode" => fn set_compositing_mode,
        "GdipGetCompositingMode" => fn get_compositing_mode,
        "GdipSetCompositingQuality" => fn set_ignored,
        "GdipGetCompositingQuality" => fn get_compositing_quality,
        "GdipSetTextContrast" => fn set_ignored,
        "GdipSetRenderingOrigin" => fn set_ignored,
        "GdipGetDpiX" => fn get_dpi,
        "GdipGetDpiY" => fn get_dpi,
        "GdipSetWorldTransform" => fn set_world_transform,
        "GdipGetWorldTransform" => fn get_world_transform,
        "GdipResetWorldTransform" => fn reset_world_transform,
        "GdipTranslateWorldTransform" => fn translate_world,
        "GdipScaleWorldTransform" => fn scale_world,
        "GdipRotateWorldTransform" => fn rotate_world,
        "GdipMultiplyWorldTransform" => fn multiply_world,
        "GdipSaveGraphics" => fn save_graphics,
        "GdipRestoreGraphics" => fn restore_graphics,
        "GdipBeginContainer2" => fn begin_container_noparams,
        "GdipEndContainer" => fn restore_graphics,
        "GdipSetClipRect" => fn set_clip_rect,
        "GdipSetClipRectI" => fn set_clip_rect_i,
        "GdipSetClipRegion" => fn set_clip_region,
        "GdipSetClipHrgn" => fn set_clip_hrgn,
        "GdipSetClipGraphics" => fn set_clip_graphics,
        "GdipSetClipPath" => fn set_clip_path,
        "GdipResetClip" => fn reset_clip,
        "GdipTranslateClip" => fn translate_clip,
        "GdipTranslateClipI" => fn translate_clip_i,
        "GdipGetClip" => fn get_clip,
        "GdipGetClipBounds" => fn get_clip_bounds,
        "GdipGetClipBoundsI" => fn get_clip_bounds_i,
        "GdipGetVisibleClipBounds" => fn get_visible_clip_bounds,
        "GdipGetVisibleClipBoundsI" => fn get_visible_clip_bounds_i,
        "GdipIsClipEmpty" => fn is_clip_empty,
        "GdipIsVisibleClipEmpty" => fn is_clip_empty,
        "GdipIsVisiblePoint" => fn is_visible_point,
        "GdipIsVisibleRect" => fn is_visible_rect,
        "GdipTransformPoints" => fn transform_points,
        "GdipTransformPointsI" => fn transform_points_i,
        "GdipGraphicsClear" => fn graphics_clear,
        // Remplissages et traits
        "GdipFillRectangle" => fn fill_rectangle,
        "GdipFillRectangleI" => fn fill_rectangle_i,
        "GdipFillRectangles" => fn fill_rectangles,
        "GdipFillRectanglesI" => fn fill_rectangles_i,
        "GdipFillEllipse" => fn fill_ellipse,
        "GdipFillEllipseI" => fn fill_ellipse_i,
        "GdipFillPie" => fn fill_pie,
        "GdipFillPieI" => fn fill_pie_i,
        "GdipFillPolygon" => fn fill_polygon,
        "GdipFillPolygonI" => fn fill_polygon_i,
        "GdipFillPolygon2" => fn fill_polygon2,
        "GdipFillPolygon2I" => fn fill_polygon2_i,
        "GdipFillClosedCurve" => fn fill_closed_curve,
        "GdipFillClosedCurveI" => fn fill_closed_curve_i,
        "GdipFillPath" => fn fill_gpath,
        "GdipFillRegion" => fn fill_region,
        "GdipDrawLine" => fn draw_line,
        "GdipDrawLineI" => fn draw_line_i,
        "GdipDrawLines" => fn draw_lines,
        "GdipDrawLinesI" => fn draw_lines_i,
        "GdipDrawRectangle" => fn draw_rectangle,
        "GdipDrawRectangleI" => fn draw_rectangle_i,
        "GdipDrawRectangles" => fn draw_rectangles,
        "GdipDrawRectanglesI" => fn draw_rectangles_i,
        "GdipDrawEllipse" => fn draw_ellipse,
        "GdipDrawEllipseI" => fn draw_ellipse_i,
        "GdipDrawArc" => fn draw_arc,
        "GdipDrawArcI" => fn draw_arc_i,
        "GdipDrawPie" => fn draw_pie,
        "GdipDrawPieI" => fn draw_pie_i,
        "GdipDrawPolygon" => fn draw_polygon,
        "GdipDrawPolygonI" => fn draw_polygon_i,
        "GdipDrawBezier" => fn draw_bezier,
        "GdipDrawBezierI" => fn draw_bezier_i,
        "GdipDrawBeziers" => fn draw_beziers,
        "GdipDrawBeziersI" => fn draw_beziers_i,
        "GdipDrawCurve" => fn draw_curve,
        "GdipDrawCurveI" => fn draw_curve_i,
        "GdipDrawCurve2" => fn draw_curve2,
        "GdipDrawCurve2I" => fn draw_curve2_i,
        "GdipDrawClosedCurve" => fn draw_closed_curve,
        "GdipDrawClosedCurveI" => fn draw_closed_curve_i,
        "GdipDrawPath" => fn draw_gpath,
        // Images dessinées
        "GdipDrawImage" => fn draw_image,
        "GdipDrawImageI" => fn draw_image_i,
        "GdipDrawImageRect" => fn draw_image_rect,
        "GdipDrawImageRectI" => fn draw_image_rect_i,
        "GdipDrawImagePoints" => fn draw_image_points,
        "GdipDrawImagePointsI" => fn draw_image_points_i,
        "GdipDrawImagePointRect" => fn draw_image_point_rect,
        "GdipDrawImagePointRectI" => fn draw_image_point_rect_i,
        "GdipDrawImageRectRect" => fn draw_image_rect_rect,
        "GdipDrawImageRectRectI" => fn draw_image_rect_rect_i,
        "GdipDrawImagePointsRect" => fn draw_image_points_rect,
        "GdipDrawImagePointsRectI" => fn draw_image_points_rect_i,
        "GdipCreateImageAttributes" => fn create_image_attributes,
        "GdipCloneImageAttributes" => fn clone_image_attributes,
        "GdipDisposeImageAttributes" => fn dispose_image,
        "GdipSetImageAttributesColorKeys" => fn set_color_keys,
        "GdipSetImageAttributesColorMatrix" => fn set_color_matrix,
        "GdipSetImageAttributesWrapMode" => fn attributes_ignored,
        "GdipSetImageAttributesGamma" => fn attributes_ignored,
        "GdipSetImageAttributesNoOp" => fn attributes_ignored,
        "GdipSetImageAttributesThreshold" => fn attributes_ignored,
        "GdipSetImageAttributesRemapTable" => fn attributes_ignored,
        "GdipResetImageAttributes" => fn attributes_ignored,
        // Pinceaux
        "GdipCreateSolidFill" => fn create_solid_fill,
        "GdipGetSolidFillColor" => fn get_solid_fill_color,
        "GdipSetSolidFillColor" => fn set_solid_fill_color,
        "GdipCreateHatchBrush" => fn create_hatch_brush,
        "GdipCreateLineBrush" => fn create_line_brush,
        "GdipCreateLineBrushI" => fn create_line_brush_i,
        "GdipCreateLineBrushFromRect" => fn create_line_brush_from_rect,
        "GdipCreateLineBrushFromRectI" => fn create_line_brush_from_rect_i,
        "GdipCreateLineBrushFromRectWithAngle" => fn create_line_brush_angle,
        "GdipCreateLineBrushFromRectWithAngleI" => fn create_line_brush_angle_i,
        "GdipGetLineColors" => fn get_line_colors,
        "GdipSetLineColors" => fn set_line_colors,
        "GdipSetLineBlend" => fn brush_ignored,
        "GdipSetLinePresetBlend" => fn brush_ignored,
        "GdipSetLineGammaCorrection" => fn brush_ignored,
        "GdipSetLineSigmaBlend" => fn brush_ignored,
        "GdipSetLineLinearBlend" => fn brush_ignored,
        "GdipSetLineWrapMode" => fn brush_ignored,
        "GdipSetLineTransform" => fn set_brush_transform,
        "GdipResetLineTransform" => fn reset_brush_transform,
        "GdipTranslateLineTransform" => fn translate_brush,
        "GdipScaleLineTransform" => fn scale_brush,
        "GdipRotateLineTransform" => fn rotate_brush,
        "GdipCreateTexture" => fn create_texture,
        "GdipCreateTexture2" => fn create_texture2,
        "GdipCreateTexture2I" => fn create_texture2_i,
        "GdipCreateTextureIA" => fn create_texture_ia,
        "GdipSetTextureTransform" => fn set_brush_transform,
        "GdipResetTextureTransform" => fn reset_brush_transform,
        "GdipTranslateTextureTransform" => fn translate_brush,
        "GdipScaleTextureTransform" => fn scale_brush,
        "GdipRotateTextureTransform" => fn rotate_brush,
        "GdipSetTextureWrapMode" => fn brush_ignored,
        "GdipCreatePathGradient" => fn create_path_gradient,
        "GdipCreatePathGradientI" => fn create_path_gradient_i,
        "GdipCreatePathGradientFromPath" => fn create_path_gradient_from_path,
        "GdipSetPathGradientCenterColor" => fn set_center_color,
        "GdipSetPathGradientSurroundColorsWithCount" => fn set_surround_colors,
        "GdipSetPathGradientCenterPoint" => fn set_center_point,
        "GdipSetPathGradientBlend" => fn brush_ignored,
        "GdipSetPathGradientPresetBlend" => fn brush_ignored,
        "GdipSetPathGradientFocusScales" => fn brush_ignored,
        "GdipSetPathGradientGammaCorrection" => fn brush_ignored,
        "GdipSetPathGradientSigmaBlend" => fn brush_ignored,
        "GdipSetPathGradientLinearBlend" => fn brush_ignored,
        "GdipSetPathGradientWrapMode" => fn brush_ignored,
        "GdipGetBrushType" => fn get_brush_type,
        "GdipCloneBrush" => fn clone_brush,
        "GdipDeleteBrush" => fn delete_brush,
        // Stylos
        "GdipCreatePen1" => fn create_pen1,
        "GdipCreatePen2" => fn create_pen2,
        "GdipDeletePen" => fn delete_pen,
        "GdipClonePen" => fn clone_pen,
        "GdipSetPenWidth" => fn set_pen_width,
        "GdipGetPenWidth" => fn get_pen_width,
        "GdipSetPenColor" => fn set_pen_color,
        "GdipGetPenColor" => fn get_pen_color,
        "GdipSetPenBrushFill" => fn set_pen_brush,
        "GdipSetPenDashStyle" => fn set_pen_dash_style,
        "GdipGetPenDashStyle" => fn get_pen_dash_style,
        "GdipGetPenUnit" => fn get_pen_unit,
        "GdipSetPenUnit" => fn pen_ignored,
        "GdipSetPenLineCap197819" => fn pen_ignored,
        "GdipSetPenStartCap" => fn pen_ignored,
        "GdipSetPenEndCap" => fn pen_ignored,
        "GdipSetPenDashCap197819" => fn pen_ignored,
        "GdipSetPenLineJoin" => fn pen_ignored,
        "GdipSetPenMiterLimit" => fn pen_ignored,
        "GdipSetPenMode" => fn pen_ignored,
        "GdipSetPenDashArray" => fn pen_ignored,
        "GdipSetPenDashOffset" => fn pen_ignored,
        "GdipSetPenCustomStartCap" => fn pen_ignored,
        "GdipSetPenCustomEndCap" => fn pen_ignored,
        "GdipSetPenTransform" => fn pen_ignored,
        "GdipResetPenTransform" => fn pen_ignored,
        "GdipSetPenCompoundArray" => fn pen_ignored,
        // Matrices
        "GdipCreateMatrix" => fn create_matrix,
        "GdipCreateMatrix2" => fn create_matrix2,
        "GdipCreateMatrix3" => fn create_matrix3,
        "GdipDeleteMatrix" => fn delete_matrix,
        "GdipCloneMatrix" => fn clone_matrix,
        "GdipSetMatrixElements" => fn set_matrix_elements,
        "GdipGetMatrixElements" => fn get_matrix_elements,
        "GdipTranslateMatrix" => fn translate_matrix,
        "GdipScaleMatrix" => fn scale_matrix,
        "GdipRotateMatrix" => fn rotate_matrix,
        "GdipShearMatrix" => fn shear_matrix,
        "GdipMultiplyMatrix" => fn multiply_matrix,
        "GdipInvertMatrix" => fn invert_matrix,
        "GdipTransformMatrixPoints" => fn transform_matrix_points,
        "GdipTransformMatrixPointsI" => fn transform_matrix_points_i,
        "GdipVectorTransformMatrixPoints" => fn vector_transform_matrix_points,
        "GdipIsMatrixIdentity" => fn is_matrix_identity,
        "GdipIsMatrixInvertible" => fn is_matrix_invertible,
        "GdipIsMatrixEqual" => fn is_matrix_equal,
        // Chemins
        "GdipCreatePath" => fn create_path,
        "GdipCreatePath2" => fn create_path2,
        "GdipCreatePath2I" => fn create_path2_i,
        "GdipDeletePath" => fn delete_path,
        "GdipClonePath" => fn clone_path,
        "GdipResetPath" => fn reset_path,
        "GdipStartPathFigure" => fn start_figure,
        "GdipClosePathFigure" => fn close_figure,
        "GdipClosePathFigures" => fn close_figures,
        "GdipSetPathFillMode" => fn set_path_fill_mode,
        "GdipGetPathFillMode" => fn get_path_fill_mode,
        "GdipAddPathLine" => fn add_path_line,
        "GdipAddPathLineI" => fn add_path_line_i,
        "GdipAddPathLine2" => fn add_path_line2,
        "GdipAddPathLine2I" => fn add_path_line2_i,
        "GdipAddPathArc" => fn add_path_arc,
        "GdipAddPathArcI" => fn add_path_arc_i,
        "GdipAddPathBezier" => fn add_path_bezier,
        "GdipAddPathBezierI" => fn add_path_bezier_i,
        "GdipAddPathBeziers" => fn add_path_beziers,
        "GdipAddPathBeziersI" => fn add_path_beziers_i,
        "GdipAddPathCurve" => fn add_path_curve,
        "GdipAddPathCurveI" => fn add_path_curve_i,
        "GdipAddPathClosedCurve" => fn add_path_closed_curve,
        "GdipAddPathClosedCurveI" => fn add_path_closed_curve_i,
        "GdipAddPathRectangle" => fn add_path_rectangle,
        "GdipAddPathRectangleI" => fn add_path_rectangle_i,
        "GdipAddPathRectangles" => fn add_path_rectangles,
        "GdipAddPathRectanglesI" => fn add_path_rectangles_i,
        "GdipAddPathEllipse" => fn add_path_ellipse,
        "GdipAddPathEllipseI" => fn add_path_ellipse_i,
        "GdipAddPathPie" => fn add_path_pie,
        "GdipAddPathPieI" => fn add_path_pie_i,
        "GdipAddPathPolygon" => fn add_path_polygon,
        "GdipAddPathPolygonI" => fn add_path_polygon_i,
        "GdipAddPathPath" => fn add_path_path,
        "GdipGetPointCount" => fn get_point_count,
        "GdipGetPathPoints" => fn get_path_points,
        "GdipGetPathPointsI" => fn get_path_points_i,
        "GdipGetPathLastPoint" => fn get_path_last_point,
        "GdipGetPathTypes" => fn get_path_types,
        "GdipGetPathWorldBounds" => fn get_path_bounds,
        "GdipGetPathWorldBoundsI" => fn get_path_bounds_i,
        "GdipTransformPath" => fn transform_path,
        "GdipIsVisiblePathPoint" => fn is_visible_path_point,
        "GdipIsVisiblePathPointI" => fn is_visible_path_point_i,
        "GdipFlattenPath" => fn path_ignored,
        "GdipWidenPath" => fn path_ignored,
        "GdipReversePath" => fn path_ignored,
        // Régions
        "GdipCreateRegion" => fn create_region,
        "GdipCreateRegionRect" => fn create_region_rect,
        "GdipCreateRegionRectI" => fn create_region_rect_i,
        "GdipCreateRegionPath" => fn create_region_path,
        "GdipCreateRegionHrgn" => fn create_region_hrgn,
        "GdipDeleteRegion" => fn delete_region,
        "GdipCloneRegion" => fn clone_region,
        "GdipSetInfinite" => fn set_infinite,
        "GdipSetEmpty" => fn set_empty,
        "GdipCombineRegionRect" => fn combine_region_rect,
        "GdipCombineRegionRectI" => fn combine_region_rect_i,
        "GdipCombineRegionRegion" => fn combine_region_region,
        "GdipCombineRegionPath" => fn combine_region_path,
        "GdipGetRegionBounds" => fn get_region_bounds,
        "GdipGetRegionBoundsI" => fn get_region_bounds_i,
        "GdipIsEmptyRegion" => fn is_empty_region,
        "GdipIsInfiniteRegion" => fn is_infinite_region,
        "GdipGetRegionHRgn" => fn get_region_hrgn,
        "GdipTranslateRegion" => fn translate_region,
        "GdipTranslateRegionI" => fn translate_region_i,
        "GdipIsVisibleRegionPoint" => fn is_visible_region_point,
    };
    e.extend(text::exports());
    for name in NOT_YET {
        e.entry(name).or_insert(not_yet as *const () as usize);
    }
    e
}

#[cfg(test)]
#[path = "gdiplus_tests.rs"]
mod tests;
