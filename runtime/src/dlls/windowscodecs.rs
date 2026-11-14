//! Windows Imaging Component (`windowscodecs.dll`) écrit pour Winlux.
//!
//! La fabrique WIC décode avec les décodeurs maison de `crate::imaging`
//! (PNG, JPEG, GIF, BMP, TIFF, WebP, DDS…) et fournit les objets dont se
//! servent les applis : décodeurs et images, bitmaps en mémoire avec
//! verrou, convertisseur de format, mise à l'échelle, découpe, rotation et
//! flux `IWICStream`. Les encodeurs ne sont pas encore fournis.
//!
//! Toutes les sources sont matérialisées : chaque objet garde ses pixels
//! dans son format, ce qui suffit aux images de bureau et rend
//! `CopyPixels` trivial. Une source étrangère (objet de l'appli) est lue
//! par ses propres méthodes `GetSize`, `GetPixelFormat` et `CopyPixels`.

use super::audio_com::dll_get_class_object;
use super::audio_com::{
    guid, put, Com, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::Mutex;

/// Encodeur PNG (#1868).
#[path = "windowscodecs_encoder.rs"]
mod encoder;

const E_FAIL: i32 = 0x8000_4005u32 as i32;
const WINCODEC_ERR_NOTINITIALIZED: i32 = 0x8898_2F0Cu32 as i32;
const WINCODEC_ERR_CODECNOTHUMBNAIL: i32 = 0x8898_2F44u32 as i32;
const WINCODEC_ERR_PALETTEUNAVAILABLE: i32 = 0x8898_2F45u32 as i32;
const WINCODEC_ERR_COMPONENTNOTFOUND: i32 = 0x8898_2F50u32 as i32;
const WINCODEC_ERR_BADIMAGE: i32 = 0x8898_2F60u32 as i32;
const WINCODEC_ERR_FRAMEMISSING: i32 = 0x8898_2F62u32 as i32;
const WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT: i32 = 0x8898_2F80u32 as i32;
const WINCODEC_ERR_UNSUPPORTEDOPERATION: i32 = 0x8898_2F81u32 as i32;
const WINCODEC_ERR_ALREADYLOCKED: i32 = 0x8898_2F0Du32 as i32;
const STG_E_INVALIDFUNCTION: i32 = 0x8003_0001u32 as i32;
const STG_E_FILENOTFOUND: i32 = 0x8003_0002u32 as i32;
const STG_E_ACCESSDENIED: i32 = 0x8003_0005u32 as i32;

// --- Identifiants publics (wincodec.h) -----------------------------------------

pub const CLSID_WIC_IMAGING_FACTORY: Guid = guid(
    0x317d06e8,
    0x5f24,
    0x433d,
    [0xbd, 0xf7, 0x79, 0xce, 0x68, 0xd8, 0xab, 0xc2],
);
pub const CLSID_WIC_IMAGING_FACTORY1: Guid = guid(
    0xcacaf262,
    0x9370,
    0x4615,
    [0xa1, 0x3b, 0x9f, 0x55, 0x39, 0xda, 0x4c, 0x0a],
);

const IID_IMAGING_FACTORY: Guid = guid(
    0xec5ec8a9,
    0xc395,
    0x4314,
    [0x9c, 0x77, 0x54, 0xd7, 0xa9, 0x35, 0xff, 0x70],
);
const IID_IMAGING_FACTORY2: Guid = guid(
    0x7b816b45,
    0x1996,
    0x4476,
    [0xb1, 0x32, 0xde, 0x9e, 0x24, 0x7c, 0x8a, 0xf0],
);
const IID_BITMAP_SOURCE: Guid = guid(
    0x00000120,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const IID_BITMAP: Guid = guid(
    0x00000121,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const IID_BITMAP_LOCK: Guid = guid(
    0x00000123,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const IID_FORMAT_CONVERTER: Guid = guid(
    0x00000301,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const IID_BITMAP_SCALER: Guid = guid(
    0x00000302,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const IID_BITMAP_CLIPPER: Guid = guid(
    0xe4fbcf03,
    0x223d,
    0x4e81,
    [0x93, 0x33, 0xd6, 0x35, 0x55, 0x6d, 0xd1, 0xb5],
);
const IID_BITMAP_FLIP_ROTATOR: Guid = guid(
    0x5009834f,
    0x2d6a,
    0x41ce,
    [0x9e, 0x1b, 0x17, 0xc5, 0xaf, 0xf7, 0xa7, 0x82],
);
const IID_BITMAP_FRAME_DECODE: Guid = guid(
    0x3b16811b,
    0x6a43,
    0x4ec9,
    [0xa8, 0x13, 0x3d, 0x93, 0x0c, 0x13, 0xb9, 0x40],
);
const IID_BITMAP_DECODER: Guid = guid(
    0x9edde9e7,
    0x8dee,
    0x47ea,
    [0x99, 0xdf, 0xe6, 0xfa, 0xf2, 0xed, 0x44, 0xbf],
);
const IID_ISTREAM: Guid = guid(0x0000000c, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ISEQUENTIAL_STREAM: Guid = guid(
    0x0c733a30,
    0x2a1c,
    0x11ce,
    [0xad, 0xe5, 0x00, 0xaa, 0x00, 0x44, 0x77, 0x3d],
);
const IID_WIC_STREAM: Guid = guid(
    0x135ff860,
    0x22b7,
    0x4ddf,
    [0xb0, 0xf6, 0x21, 0x8f, 0x4f, 0x29, 0x9a, 0x43],
);

const fn pixel_format(last: u8) -> Guid {
    guid(
        0x6fddc324,
        0x4e03,
        0x4bfe,
        [0xb1, 0x85, 0x3d, 0x77, 0x76, 0x8d, 0xc9, last],
    )
}
pub const FMT_GRAY8: Guid = pixel_format(0x08);
pub const FMT_BGR24: Guid = pixel_format(0x0c);
pub const FMT_RGB24: Guid = pixel_format(0x0d);
pub const FMT_BGR32: Guid = pixel_format(0x0e);
pub const FMT_BGRA32: Guid = pixel_format(0x0f);
pub const FMT_PBGRA32: Guid = pixel_format(0x10);
pub const FMT_RGBA32: Guid = guid(
    0xf5c7ad2d,
    0x6a8d,
    0x43dd,
    [0xa7, 0xa8, 0xa2, 0x99, 0x35, 0x26, 0x1a, 0xe9],
);
pub const FMT_PRGBA32: Guid = guid(
    0x3cc4a650,
    0xa527,
    0x4d37,
    [0xa9, 0x16, 0x31, 0x42, 0xc7, 0xeb, 0xed, 0xba],
);

const CONTAINER_PNG: Guid = guid(
    0x1b7cfaf4,
    0x713f,
    0x473c,
    [0xbb, 0xcd, 0x61, 0x37, 0x42, 0x5f, 0xae, 0xaf],
);
const CONTAINER_JPEG: Guid = guid(
    0x19e4a5aa,
    0x5662,
    0x4fc5,
    [0xa0, 0xc0, 0x17, 0x58, 0x02, 0x8e, 0x10, 0x57],
);
const CONTAINER_BMP: Guid = guid(
    0x0af1d87e,
    0xfcfe,
    0x4188,
    [0xbd, 0xeb, 0xa7, 0x90, 0x64, 0x71, 0xcb, 0xe3],
);
const CONTAINER_GIF: Guid = guid(
    0x1f8a5601,
    0x7d4d,
    0x4cbd,
    [0x9c, 0x82, 0x1b, 0xc8, 0xd4, 0xee, 0xb9, 0xa5],
);
const CONTAINER_TIFF: Guid = guid(
    0x163bcc30,
    0xe2e9,
    0x4f0b,
    [0x96, 0x1d, 0xa3, 0xe9, 0xfd, 0xb7, 0x88, 0xa3],
);
const CONTAINER_WEBP: Guid = guid(
    0xe094b0e2,
    0x67f2,
    0x45b3,
    [0xb0, 0xea, 0x11, 0x53, 0x37, 0xca, 0x7c, 0xf3],
);
const CONTAINER_ICO: Guid = guid(
    0xa3a860c4,
    0x338f,
    0x4c17,
    [0x91, 0x9a, 0xfb, 0xa4, 0xb5, 0x62, 0x8f, 0x21],
);
const CONTAINER_DDS: Guid = guid(
    0x9967cb95,
    0x2e85,
    0x4ac8,
    [0x8c, 0xa2, 0x83, 0xd7, 0xcc, 0xd4, 0x25, 0xc9],
);

use super::wic_pixels::{self, FMT_RGB32};

/// Octets par pixel des formats que Winlux sait lire et produire : 8 bits
/// par canal ici, plus larges dans `wic_pixels` (#1384).
fn bytes_per_pixel(fmt: &Guid) -> Option<u32> {
    Some(match *fmt {
        FMT_GRAY8 => 1,
        FMT_BGR24 | FMT_RGB24 => 3,
        FMT_BGR32 | FMT_RGB32 | FMT_BGRA32 | FMT_PBGRA32 | FMT_RGBA32 | FMT_PRGBA32 => 4,
        _ => return wic_pixels::layout(fmt).map(|l| l.bpp),
    })
}

/// Nom lisible d'un GUID de format, pour le journal.
fn format_name(fmt: &Guid) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{}}}",
        fmt.data1,
        fmt.data2,
        fmt.data3,
        fmt.data4
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>()
    )
}

// --- Pixels -------------------------------------------------------------------

/// Taille maximale du tampon d'un bitmap WIC (1 Gio).
const MAX_PIX_BYTES: u64 = 1 << 30;

/// Rectangle valide et inclus dans `w`×`h` (calcul en 64 bits : `x + w`
/// peut déborder un `i32`).
fn rect_in(r: &WicRect, w: u32, h: u32) -> bool {
    r.x >= 0
        && r.y >= 0
        && r.w >= 0
        && r.h >= 0
        && r.x as i64 + r.w as i64 <= w as i64
        && r.y as i64 + r.h as i64 <= h as i64
}

/// Vrai si un tampon de `size` octets au pas `stride` contient `h` lignes de
/// `row` octets (sans débordement).
fn buffer_fits(stride: u32, size: u32, row: u64, h: u64) -> bool {
    if h == 0 {
        return true;
    }
    (stride as u64) >= row && (size as u64) >= (stride as u64) * (h - 1) + row
}

#[derive(Clone, Debug)]
pub struct Pix {
    pub w: u32,
    pub h: u32,
    pub fmt: Guid,
    pub stride: u32,
    pub data: Vec<u8>,
    pub dpi: (f64, f64),
}

impl Pix {
    fn zeroed(w: u32, h: u32, fmt: Guid) -> Option<Pix> {
        let bpp = bytes_per_pixel(&fmt)?;
        // Stride et taille calculés sans débordement, plafonnés à 1 Gio :
        // les dimensions viennent de l'application ou d'un fichier.
        let stride = (w as u64 * bpp as u64 + 3) & !3;
        let total = stride * h as u64;
        if stride > u32::MAX as u64 || total > MAX_PIX_BYTES {
            return None;
        }
        Some(Pix {
            w,
            h,
            fmt,
            stride: stride as u32,
            data: vec![0; total as usize],
            dpi: (96.0, 96.0),
        })
    }

    /// Pixels en RGBA 8 bits non prémultiplié, ligne à ligne sans marge.
    fn to_rgba(&self) -> Vec<u8> {
        if wic_pixels::layout(&self.fmt).is_some() {
            return self
                .to_raw()
                .iter()
                .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                .collect();
        }
        let bpp = bytes_per_pixel(&self.fmt).unwrap_or(4) as usize;
        let mut out = Vec::with_capacity(self.w as usize * self.h as usize * 4);
        for y in 0..self.h as usize {
            let row = &self.data[y * self.stride as usize..];
            for x in 0..self.w as usize {
                let p = &row[x * bpp..x * bpp + bpp];
                let px = match self.fmt {
                    FMT_GRAY8 => [p[0], p[0], p[0], 255],
                    FMT_BGR24 => [p[2], p[1], p[0], 255],
                    FMT_RGB24 => [p[0], p[1], p[2], 255],
                    FMT_BGR32 => [p[2], p[1], p[0], 255],
                    FMT_RGB32 => [p[0], p[1], p[2], 255],
                    FMT_BGRA32 => [p[2], p[1], p[0], p[3]],
                    FMT_RGBA32 => [p[0], p[1], p[2], p[3]],
                    FMT_PBGRA32 => unpremultiply([p[2], p[1], p[0], p[3]]),
                    _ => unpremultiply([p[0], p[1], p[2], p[3]]),
                };
                out.extend_from_slice(&px);
            }
        }
        out
    }

    fn from_rgba(w: u32, h: u32, rgba: &[u8], fmt: Guid, dpi: (f64, f64)) -> Option<Pix> {
        if wic_pixels::layout(&fmt).is_some() {
            let raw: Vec<[f32; 4]> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p.map(|v| v as f32 / 255.0))
                .collect();
            return Pix::from_raw(w, h, &raw, fmt, dpi);
        }
        let mut pix = Pix::zeroed(w, h, fmt)?;
        pix.dpi = dpi;
        let bpp = bytes_per_pixel(&fmt)? as usize;
        for y in 0..h as usize {
            for x in 0..w as usize {
                let s = &rgba[(y * w as usize + x) * 4..][..4];
                let d = &mut pix.data[y * pix.stride as usize + x * bpp..][..bpp];
                match fmt {
                    FMT_GRAY8 => {
                        d[0] =
                            ((s[0] as u32 * 77 + s[1] as u32 * 150 + s[2] as u32 * 29) >> 8) as u8
                    }
                    FMT_BGR24 => d.copy_from_slice(&[s[2], s[1], s[0]]),
                    FMT_RGB24 => d.copy_from_slice(&[s[0], s[1], s[2]]),
                    FMT_BGR32 => d.copy_from_slice(&[s[2], s[1], s[0], 255]),
                    FMT_RGB32 => d.copy_from_slice(&[s[0], s[1], s[2], 255]),
                    FMT_BGRA32 => d.copy_from_slice(&[s[2], s[1], s[0], s[3]]),
                    FMT_RGBA32 => d.copy_from_slice(s),
                    FMT_PBGRA32 => {
                        let p = premultiply([s[0], s[1], s[2], s[3]]);
                        d.copy_from_slice(&[p[2], p[1], p[0], p[3]]);
                    }
                    _ => d.copy_from_slice(&premultiply([s[0], s[1], s[2], s[3]])),
                }
            }
        }
        Some(pix)
    }

    fn convert(&self, fmt: Guid) -> Option<Pix> {
        if fmt == self.fmt {
            return Some(self.clone());
        }
        bytes_per_pixel(&fmt)?;
        let wide = |f: &Guid| wic_pixels::layout(f).is_some();
        if !wide(&self.fmt) && !wide(&fmt) {
            return Pix::from_rgba(self.w, self.h, &self.to_rgba(), fmt, self.dpi);
        }
        // Au moins un format large : passage par des valeurs linéaires en
        // flottant, sans perdre la précision ni les valeurs hors 0..1.
        let lin: Vec<[f32; 4]> = self
            .to_raw()
            .into_iter()
            .map(|c| {
                if format_linear(&self.fmt) {
                    c
                } else {
                    let f = wic_pixels::srgb_to_linear;
                    [f(c[0]), f(c[1]), f(c[2]), c[3]]
                }
            })
            .collect();
        let raw: Vec<[f32; 4]> = if format_linear(&fmt) {
            lin
        } else {
            let f = wic_pixels::linear_to_srgb;
            lin.into_iter()
                .map(|c| [f(c[0]), f(c[1]), f(c[2]), c[3]])
                .collect()
        };
        Pix::from_raw(self.w, self.h, &raw, fmt, self.dpi)
    }

    /// Pixels RVBA droits, valeurs telles que stockées par le format
    /// (0..1 pour les entiers ; sans changement de courbe).
    fn to_raw(&self) -> Vec<[f32; 4]> {
        let Some(l) = wic_pixels::layout(&self.fmt) else {
            return self
                .to_rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p.map(|v| v as f32 / 255.0))
                .collect();
        };
        let bpp = l.bpp as usize;
        let mut out = Vec::with_capacity(self.w as usize * self.h as usize);
        for y in 0..self.h as usize {
            let row = &self.data[y * self.stride as usize..];
            for x in 0..self.w as usize {
                out.push(wic_pixels::read(&l, &row[x * bpp..x * bpp + bpp]));
            }
        }
        out
    }

    /// Inverse de `to_raw`.
    fn from_raw(w: u32, h: u32, raw: &[[f32; 4]], fmt: Guid, dpi: (f64, f64)) -> Option<Pix> {
        let Some(l) = wic_pixels::layout(&fmt) else {
            let rgba: Vec<u8> = raw
                .iter()
                .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                .collect();
            return Pix::from_rgba(w, h, &rgba, fmt, dpi);
        };
        let mut pix = Pix::zeroed(w, h, fmt)?;
        pix.dpi = dpi;
        let bpp = l.bpp as usize;
        for y in 0..h as usize {
            for x in 0..w as usize {
                let d = &mut pix.data[y * pix.stride as usize + x * bpp..][..bpp];
                wic_pixels::write(&l, d, raw[y * w as usize + x]);
            }
        }
        Some(pix)
    }

    fn from_raster(r: &crate::imaging::Raster) -> Pix {
        Pix::from_rgba(r.w, r.h, &r.px, FMT_BGRA32, (96.0, 96.0)).expect("format connu")
    }
}

/// Valeurs linéaires (formats flottants) plutôt qu'encodées sRGB.
fn format_linear(fmt: &Guid) -> bool {
    wic_pixels::layout(fmt).is_some_and(|l| l.linear())
}

fn premultiply(p: [u8; 4]) -> [u8; 4] {
    let a = p[3] as u32;
    let m = |c: u8| ((c as u32 * a + 127) / 255) as u8;
    [m(p[0]), m(p[1]), m(p[2]), p[3]]
}

fn unpremultiply(p: [u8; 4]) -> [u8; 4] {
    let a = p[3] as u32;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let u = |c: u8| ((c as u32 * 255 + a / 2) / a).min(255) as u8;
    [u(p[0]), u(p[1]), u(p[2]), p[3]]
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WicRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// Copie le rectangle `rect` (ou toute l'image) de `pix` dans le tampon
/// de l'appelant, avec les contrôles de WIC.
unsafe fn copy_pixels(
    pix: &Pix,
    rect: *const WicRect,
    stride: u32,
    size: u32,
    buf: *mut u8,
) -> i32 {
    if buf.is_null() {
        return E_INVALIDARG;
    }
    let r = if rect.is_null() {
        WicRect {
            x: 0,
            y: 0,
            w: pix.w as i32,
            h: pix.h as i32,
        }
    } else {
        *rect
    };
    if !rect_in(&r, pix.w, pix.h) {
        return E_INVALIDARG;
    }
    let bpp = bytes_per_pixel(&pix.fmt).unwrap_or(4) as usize;
    let row = r.w as usize * bpp;
    if r.h == 0 || r.w == 0 {
        return S_OK;
    }
    if !buffer_fits(stride, size, row as u64, r.h as u64) {
        return E_INVALIDARG;
    }
    for y in 0..r.h as usize {
        let src = (r.y as usize + y) * pix.stride as usize + r.x as usize * bpp;
        std::ptr::copy_nonoverlapping(
            pix.data[src..src + row].as_ptr(),
            buf.add(y * stride as usize),
            row,
        );
    }
    S_OK
}

/// Appelle la méthode n° `slot` de l'objet COM `obj`.
unsafe fn method<F: Copy>(obj: *mut c_void, slot: usize) -> F {
    let vtbl = *(obj as *const *const usize);
    std::mem::transmute_copy(&*vtbl.add(slot))
}

/// Lit entièrement une source `IWICBitmapSource`, de Winlux ou de l'appli.
unsafe fn read_source(src: *mut c_void) -> Result<Pix, i32> {
    if src.is_null() {
        return Err(E_INVALIDARG);
    }
    type GetSize = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut u32) -> i32;
    type GetFormat = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
    type GetRes = unsafe extern "win64" fn(*mut c_void, *mut f64, *mut f64) -> i32;
    type Copy = unsafe extern "win64" fn(*mut c_void, *const WicRect, u32, u32, *mut u8) -> i32;
    let (mut w, mut h) = (0u32, 0u32);
    let hr = method::<GetSize>(src, 3)(src, &mut w, &mut h);
    if hr < 0 {
        return Err(hr);
    }
    let mut fmt = Guid::default();
    let hr = method::<GetFormat>(src, 4)(src, &mut fmt);
    if hr < 0 {
        return Err(hr);
    }
    let mut pix = Pix::zeroed(w, h, fmt).ok_or(WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT)?;
    let (mut dx, mut dy) = (96.0, 96.0);
    if method::<GetRes>(src, 5)(src, &mut dx, &mut dy) >= 0 {
        pix.dpi = (dx, dy);
    }
    let (stride, len) = (pix.stride, pix.data.len() as u32);
    let hr = method::<Copy>(src, 7)(src, std::ptr::null(), stride, len, pix.data.as_mut_ptr());
    if hr < 0 {
        return Err(hr);
    }
    Ok(pix)
}

// --- Fabrique ---------------------------------------------------------------------

struct Factory;

unsafe extern "win64" fn add_ref<T>(this: *mut Com<T>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn release<T>(this: *mut Com<T>) -> u32 {
    Com::release(this)
}

/// QueryInterface commun : renvoie l'objet lui-même pour les IID listés.
unsafe fn qi_list<T>(
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
    let iid = *riid;
    if iid == IID_IUNKNOWN || iids.contains(&iid) {
        Com::add_ref(this);
        return put(out, this);
    }
    E_NOINTERFACE
}

/// IWICComponentFactory {412D0C3A-9650-44FA-AF5B-DD2A06C8E8FB}
/// (wincodecsdk.h) : IWICImagingFactory suivie des méthodes de
/// métadonnées. IWICImagingFactory2 ajoute une autre méthode à la même
/// case : la fabrique de composants est donc une facette à part (#1343).
const IID_COMPONENT_FACTORY: Guid = guid(
    0x412d_0c3a,
    0x9650,
    0x44fa,
    [0xaf, 0x5b, 0xdd, 0x2a, 0x06, 0xc8, 0xe8, 0xfb],
);

unsafe extern "win64" fn factory_qi(
    this: *mut Com<Factory>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() && !riid.is_null() && *riid == IID_COMPONENT_FACTORY {
        *out = std::ptr::null_mut();
        return put(out, Com::create(COMPONENT_FACTORY_VT.ptr(), Factory));
    }
    qi_list(
        this,
        riid,
        out,
        &[IID_IMAGING_FACTORY, IID_IMAGING_FACTORY2],
    )
}

/// QueryInterface de la facette IWICComponentFactory : elle-même pour son
/// IID et IWICImagingFactory ; la fabrique complète pour IWICImagingFactory2.
unsafe extern "win64" fn component_factory_qi(
    this: *mut Com<Factory>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() && !riid.is_null() && *riid == IID_IMAGING_FACTORY2 {
        *out = std::ptr::null_mut();
        return put(out, Com::create(FACTORY_VT.ptr(), Factory));
    }
    qi_list(
        this,
        riid,
        out,
        &[IID_COMPONENT_FACTORY, IID_IMAGING_FACTORY],
    )
}

unsafe fn decoder_from_bytes(bytes: &[u8]) -> Result<Decoder, i32> {
    let img =
        crate::imaging::decode_bytes(bytes, "").map_err(|_| WINCODEC_ERR_COMPONENTNOTFOUND)?;
    let frames: Vec<Pix> = img
        .pages
        .into_iter()
        .filter_map(|p| match p.content {
            crate::imaging::Content::Raster(r) => Some(Pix::from_raster(&r)),
            _ => None,
        })
        .collect();
    if frames.is_empty() {
        return Err(WINCODEC_ERR_BADIMAGE);
    }
    Ok(Decoder {
        container: container_of(bytes),
        frames,
    })
}

fn container_of(d: &[u8]) -> Guid {
    if d.starts_with(b"\x89PNG") {
        CONTAINER_PNG
    } else if d.starts_with(&[0xFF, 0xD8]) {
        CONTAINER_JPEG
    } else if d.starts_with(b"GIF8") {
        CONTAINER_GIF
    } else if d.starts_with(b"BM") {
        CONTAINER_BMP
    } else if d.starts_with(b"II*\0") || d.starts_with(b"MM\0*") {
        CONTAINER_TIFF
    } else if d.starts_with(b"RIFF") {
        CONTAINER_WEBP
    } else if d.starts_with(b"DDS ") {
        CONTAINER_DDS
    } else if d.len() > 6
        && (d.starts_with(&[0, 0, 1, 0]) || d.starts_with(&[0, 0, 2, 0]))
        && d[4..6] != [0, 0]
    {
        // Icône (type 1) ou curseur (type 2) : le décodeur ICO lit les deux.
        CONTAINER_ICO
    } else {
        Guid::default()
    }
}

/// Lit tout le contenu d'un `IStream` depuis le début.
unsafe fn read_stream(stream: *mut c_void) -> Result<Vec<u8>, i32> {
    if stream.is_null() {
        return Err(E_INVALIDARG);
    }
    type Seek = unsafe extern "win64" fn(*mut c_void, i64, u32, *mut u64) -> i32;
    type Read = unsafe extern "win64" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32;
    let mut pos = 0u64;
    let hr = method::<Seek>(stream, 5)(stream, 0, 0, &mut pos);
    if hr < 0 {
        return Err(hr);
    }
    let mut out = Vec::new();
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        let mut got = 0u32;
        let hr =
            method::<Read>(stream, 3)(stream, chunk.as_mut_ptr(), chunk.len() as u32, &mut got);
        if hr < 0 {
            return Err(hr);
        }
        out.extend_from_slice(&chunk[..got as usize]);
        if got == 0 || hr == 1 {
            break;
        }
    }
    Ok(out)
}

unsafe fn new_decoder(dec: Decoder, out: *mut *mut c_void) -> i32 {
    put(out, Com::create(DECODER_VT.ptr(), Mutex::new(Some(dec))))
}

unsafe extern "win64" fn create_decoder_from_filename(
    _this: *mut c_void,
    name: *const u16,
    _vendor: *const Guid,
    _access: u32,
    _options: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if name.is_null() {
        return E_INVALIDARG;
    }
    let path = crate::paths::to_linux(&crate::dlls::wstr(name));
    let Ok(bytes) = std::fs::read(&path) else {
        return 0x8007_0002u32 as i32; // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
    };
    match decoder_from_bytes(&bytes) {
        Ok(d) => new_decoder(d, out),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn create_decoder_from_stream(
    _this: *mut c_void,
    stream: *mut c_void,
    _vendor: *const Guid,
    _options: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match read_stream(stream).and_then(|b| decoder_from_bytes(&b)) {
        Ok(d) => new_decoder(d, out),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn create_decoder_from_file_handle(
    _this: *mut c_void,
    file: usize,
    _vendor: *const Guid,
    _options: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    use crate::dlls::kernel32::{get_file_size_ex, read_file, set_file_pointer_ex};
    let mut size = 0i64;
    if get_file_size_ex(file, &mut size) == 0
        || set_file_pointer_ex(file, 0, std::ptr::null_mut(), 0) == 0
    {
        return E_INVALIDARG;
    }
    let mut bytes = vec![0u8; size.max(0) as usize];
    let mut got = 0u32;
    if read_file(file, bytes.as_mut_ptr(), bytes.len() as u32, &mut got, 0) == 0 {
        return E_FAIL;
    }
    bytes.truncate(got as usize);
    match decoder_from_bytes(&bytes) {
        Ok(d) => new_decoder(d, out),
        Err(hr) => hr,
    }
}

/// CreateDecoder : décodeur vide, rempli par `IWICBitmapDecoder::Initialize`.
unsafe extern "win64" fn create_decoder(
    _this: *mut c_void,
    container: *const Guid,
    _vendor: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let known = [
        CONTAINER_PNG,
        CONTAINER_JPEG,
        CONTAINER_BMP,
        CONTAINER_GIF,
        CONTAINER_TIFF,
        CONTAINER_WEBP,
        CONTAINER_DDS,
        CONTAINER_ICO,
    ];
    if container.is_null() || !known.contains(&*container) {
        return WINCODEC_ERR_COMPONENTNOTFOUND;
    }
    put(
        out,
        Com::create(DECODER_VT.ptr(), Mutex::<Option<Decoder>>::new(None)),
    )
}

unsafe fn new_source(kind: Kind, pix: Option<Pix>, out: *mut *mut c_void) -> i32 {
    let vt = match kind {
        Kind::Frame => FRAME_VT.ptr(),
        Kind::Bitmap => BITMAP_VT.ptr(),
        Kind::Converter => CONVERTER_VT.ptr(),
        Kind::ColorTransform => COLOR_TRANSFORM_VT.ptr(),
        Kind::Scaler | Kind::Clipper | Kind::FlipRotator => TRANSFORM_VT.ptr(),
    };
    put(
        out,
        Com::create(
            vt,
            Source {
                kind,
                pix: Mutex::new(pix),
                locked: Mutex::new(false),
            },
        ),
    )
}

macro_rules! creator_of {
    ($name:ident, $kind:expr) => {
        unsafe extern "win64" fn $name(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            new_source($kind, None, out)
        }
    };
}
creator_of!(create_format_converter, Kind::Converter);
creator_of!(create_color_transformer, Kind::ColorTransform);
creator_of!(create_bitmap_scaler, Kind::Scaler);
creator_of!(create_bitmap_clipper, Kind::Clipper);
creator_of!(create_bitmap_flip_rotator, Kind::FlipRotator);

unsafe extern "win64" fn create_stream(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(STREAM_VT.ptr(), Mutex::new(Stream::default())),
    )
}

unsafe extern "win64" fn create_bitmap(
    _this: *mut c_void,
    w: u32,
    h: u32,
    fmt: *const Guid,
    _option: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || fmt.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    match Pix::zeroed(w, h, *fmt) {
        Some(p) => new_source(Kind::Bitmap, Some(p), out),
        None => {
            crate::log_warn!(
                "wic",
                "CreateBitmap {}×{} : format de pixel {} non pris en charge",
                w,
                h,
                format_name(&*fmt)
            );
            WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT
        }
    }
}

unsafe extern "win64" fn create_bitmap_from_source(
    _this: *mut c_void,
    src: *mut c_void,
    _option: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match read_source(src) {
        Ok(p) => new_source(Kind::Bitmap, Some(p), out),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn create_bitmap_from_source_rect(
    _this: *mut c_void,
    src: *mut c_void,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    match clip(
        &pix,
        WicRect {
            x: x as i32,
            y: y as i32,
            w: w as i32,
            h: h as i32,
        },
    ) {
        Some(p) => new_source(Kind::Bitmap, Some(p), out),
        None => E_INVALIDARG,
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn create_bitmap_from_memory(
    _this: *mut c_void,
    w: u32,
    h: u32,
    fmt: *const Guid,
    stride: u32,
    size: u32,
    buf: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || fmt.is_null() || buf.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let Some(bpp) = bytes_per_pixel(&*fmt) else {
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    };
    let row = w as u64 * bpp as u64;
    // Le tampon de l'appelant doit couvrir toutes les lignes avant d'allouer.
    if !buffer_fits(stride, size, row, h as u64) {
        return E_INVALIDARG;
    }
    let Some(mut pix) = Pix::zeroed(w, h, *fmt) else {
        return E_INVALIDARG;
    };
    let row = row as usize;
    for y in 0..h as usize {
        let dst = y * pix.stride as usize;
        std::ptr::copy_nonoverlapping(
            buf.add(y * stride as usize),
            pix.data[dst..].as_mut_ptr(),
            row,
        );
    }
    new_source(Kind::Bitmap, Some(pix), out)
}

static FACTORY_VT: VTable<29> = VTable([
    factory_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    create_decoder_from_filename as *const (),
    create_decoder_from_stream as *const (),
    create_decoder_from_file_handle as *const (),
    create_component_info as *const (),
    create_decoder as *const (),
    encoder::create_encoder as *const (),
    create_palette as *const (),
    create_format_converter as *const (),
    create_bitmap_scaler as *const (),
    create_bitmap_clipper as *const (),
    create_bitmap_flip_rotator as *const (),
    create_stream as *const (),
    create_color_context as *const (),
    create_color_transformer as *const (),
    create_bitmap as *const (),
    create_bitmap_from_source as *const (),
    create_bitmap_from_source_rect as *const (),
    create_bitmap_from_memory as *const (),
    unsupported!("CreateBitmapFromHBITMAP"),
    unsupported!("CreateBitmapFromHICON"),
    unsupported!("CreateComponentEnumerator"),
    unsupported!("CreateFastMetadataEncoderFromDecoder"),
    unsupported!("CreateFastMetadataEncoderFromFrameDecode"),
    unsupported!("CreateQueryWriter"),
    unsupported!("CreateQueryWriterFromReader"),
    unsupported!("CreateImageEncoder"),
]);

/// IWICComponentFactory : les 28 méthodes d'IWICImagingFactory, puis les
/// sept méthodes de métadonnées (non prises en charge).
static COMPONENT_FACTORY_VT: VTable<35> = VTable([
    component_factory_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    create_decoder_from_filename as *const (),
    create_decoder_from_stream as *const (),
    create_decoder_from_file_handle as *const (),
    create_component_info as *const (),
    create_decoder as *const (),
    encoder::create_encoder as *const (),
    create_palette as *const (),
    create_format_converter as *const (),
    create_bitmap_scaler as *const (),
    create_bitmap_clipper as *const (),
    create_bitmap_flip_rotator as *const (),
    create_stream as *const (),
    create_color_context as *const (),
    create_color_transformer as *const (),
    create_bitmap as *const (),
    create_bitmap_from_source as *const (),
    create_bitmap_from_source_rect as *const (),
    create_bitmap_from_memory as *const (),
    unsupported!("CreateBitmapFromHBITMAP"),
    unsupported!("CreateBitmapFromHICON"),
    unsupported!("CreateComponentEnumerator"),
    unsupported!("CreateFastMetadataEncoderFromDecoder"),
    unsupported!("CreateFastMetadataEncoderFromFrameDecode"),
    unsupported!("CreateQueryWriter"),
    unsupported!("CreateQueryWriterFromReader"),
    // IWICComponentFactory
    unsupported!("CreateMetadataReader"),
    unsupported!("CreateMetadataReaderFromContainer"),
    unsupported!("CreateMetadataWriter"),
    unsupported!("CreateMetadataWriterFromReader"),
    unsupported!("CreateQueryReaderFromBlockReader"),
    unsupported!("CreateQueryWriterFromBlockWriter"),
    unsupported!("CreateEncoderPropertyBag"),
]);

// --- Contexte de couleur (#1343) ----------------------------------------------------

/// IWICColorContext {3C613A02-34B2-44EA-9A7C-45AEA9C6FD6D} (wincodec.h).
const IID_COLOR_CONTEXT: Guid = guid(
    0x3c61_3a02,
    0x34b2,
    0x44ea,
    [0x9a, 0x7c, 0x45, 0xae, 0xa9, 0xc6, 0xfd, 0x6d],
);
/// WICColorContextType.
const COLOR_CONTEXT_PROFILE: u32 = 1;
const COLOR_CONTEXT_EXIF: u32 = 2;

/// Contexte : non initialisé, profil ICC (octets) ou espace EXIF (1 sRGB,
/// 2 Adobe RGB). Aucune conversion n'est faite : le contenu est rendu tel
/// qu'il a été donné.
enum ColorContext {
    Uninitialized,
    Profile(Vec<u8>),
    Exif(u32),
}

type ColorContextObj = Com<Mutex<ColorContext>>;

static COLOR_CONTEXT_VT: VTable<9> = VTable([
    color_context_qi as *const (),
    add_ref::<Mutex<ColorContext>> as *const (),
    release::<Mutex<ColorContext>> as *const (),
    cc_init_filename as *const (),
    cc_init_memory as *const (),
    cc_init_exif as *const (),
    cc_get_type as *const (),
    cc_get_profile_bytes as *const (),
    cc_get_exif as *const (),
]);

unsafe extern "win64" fn create_color_context(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(
            COLOR_CONTEXT_VT.ptr(),
            Mutex::new(ColorContext::Uninitialized),
        ),
    )
}

unsafe extern "win64" fn color_context_qi(
    this: *mut ColorContextObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_COLOR_CONTEXT])
}

unsafe fn cc_set(this: *mut ColorContextObj, c: ColorContext) -> i32 {
    *crate::dlls::lock(Com::get(this)) = c;
    S_OK
}

unsafe extern "win64" fn cc_init_filename(this: *mut ColorContextObj, name: *const u16) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    let path = crate::paths::to_linux(&crate::dlls::wstr(name));
    match std::fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => cc_set(this, ColorContext::Profile(bytes)),
        Ok(_) => E_INVALIDARG,
        Err(_) => 0x8007_0002u32 as i32, // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
    }
}

unsafe extern "win64" fn cc_init_memory(
    this: *mut ColorContextObj,
    data: *const u8,
    size: u32,
) -> i32 {
    if data.is_null() || size == 0 {
        return E_INVALIDARG;
    }
    let bytes = std::slice::from_raw_parts(data, size as usize).to_vec();
    cc_set(this, ColorContext::Profile(bytes))
}

unsafe extern "win64" fn cc_init_exif(this: *mut ColorContextObj, value: u32) -> i32 {
    // 1 : sRGB ; 2 : Adobe RGB.
    if !matches!(value, 1 | 2) {
        return E_INVALIDARG;
    }
    cc_set(this, ColorContext::Exif(value))
}

unsafe extern "win64" fn cc_get_type(this: *mut ColorContextObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = match &*crate::dlls::lock(Com::get(this)) {
        ColorContext::Uninitialized => 0,
        ColorContext::Profile(_) => COLOR_CONTEXT_PROFILE,
        ColorContext::Exif(_) => COLOR_CONTEXT_EXIF,
    };
    S_OK
}

/// GetProfileBytes(cbBuffer, pbBuffer, pcbActual) : sans tampon, la taille.
unsafe extern "win64" fn cc_get_profile_bytes(
    this: *mut ColorContextObj,
    size: u32,
    buf: *mut u8,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() {
        return E_INVALIDARG;
    }
    let guard = crate::dlls::lock(Com::get(this));
    let ColorContext::Profile(bytes) = &*guard else {
        *actual = 0;
        return WINCODEC_ERR_NOTINITIALIZED;
    };
    *actual = bytes.len() as u32;
    if buf.is_null() {
        return S_OK;
    }
    if (size as usize) < bytes.len() {
        return E_INVALIDARG;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    S_OK
}

unsafe extern "win64" fn cc_get_exif(this: *mut ColorContextObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // Un contexte qui n'est pas EXIF répond 0xFFFFFFFF (non calibré).
    *out = match &*crate::dlls::lock(Com::get(this)) {
        ColorContext::Exif(v) => *v,
        _ => 0xFFFF_FFFF,
    };
    S_OK
}

// --- Décodeur ---------------------------------------------------------------------

struct Decoder {
    container: Guid,
    frames: Vec<Pix>,
}

type DecoderObj = Com<Mutex<Option<Decoder>>>;

unsafe extern "win64" fn decoder_qi(
    this: *mut DecoderObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_BITMAP_DECODER])
}

unsafe extern "win64" fn decoder_query_capability(
    _this: *mut DecoderObj,
    stream: *mut c_void,
    caps: *mut u32,
) -> i32 {
    if caps.is_null() {
        return E_POINTER;
    }
    *caps = match read_stream(stream).map(|b| decoder_from_bytes(&b)) {
        // WICBitmapDecoderCapabilityCanDecodeAllImages | CanDecodeSomeImages
        Ok(Ok(_)) => 0x4 | 0x8,
        _ => 0,
    };
    S_OK
}

unsafe extern "win64" fn decoder_initialize(
    this: *mut DecoderObj,
    stream: *mut c_void,
    _options: u32,
) -> i32 {
    match read_stream(stream).and_then(|b| decoder_from_bytes(&b)) {
        Ok(d) => {
            *crate::dlls::lock(Com::get(this)) = Some(d);
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn decoder_container(this: *mut DecoderObj, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match &*crate::dlls::lock(Com::get(this)) {
        Some(d) => {
            *out = d.container;
            S_OK
        }
        None => WINCODEC_ERR_NOTINITIALIZED,
    }
}

unsafe extern "win64" fn palette_unavailable() -> i32 {
    WINCODEC_ERR_PALETTEUNAVAILABLE
}
unsafe extern "win64" fn unsupported_operation() -> i32 {
    WINCODEC_ERR_UNSUPPORTEDOPERATION
}
unsafe extern "win64" fn no_thumbnail() -> i32 {
    WINCODEC_ERR_CODECNOTHUMBNAIL
}

/// GetColorContexts : aucun profil ICC n'est extrait (sRGB implicite).
unsafe extern "win64" fn no_color_contexts(
    _this: *mut c_void,
    _count: u32,
    _contexts: *mut c_void,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() {
        return E_INVALIDARG;
    }
    *actual = 0;
    S_OK
}

unsafe extern "win64" fn decoder_frame_count(this: *mut DecoderObj, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    match &*crate::dlls::lock(Com::get(this)) {
        Some(d) => {
            *count = d.frames.len() as u32;
            S_OK
        }
        None => WINCODEC_ERR_NOTINITIALIZED,
    }
}

unsafe extern "win64" fn decoder_frame(
    this: *mut DecoderObj,
    index: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (pix, icon) = match &*crate::dlls::lock(Com::get(this)) {
        Some(d) => match d.frames.get(index as usize) {
            Some(p) => (p.clone(), d.container == CONTAINER_ICO),
            None => return WINCODEC_ERR_FRAMEMISSING,
        },
        None => return WINCODEC_ERR_NOTINITIALIZED,
    };
    if icon {
        return put(
            out,
            Com::create(
                ICON_FRAME_VT.ptr(),
                Source {
                    kind: Kind::Frame,
                    pix: Mutex::new(Some(pix)),
                    locked: Mutex::new(false),
                },
            ),
        );
    }
    new_source(Kind::Frame, Some(pix), out)
}

/// GetThumbnail d'une image d'icône (#1099) : comme le décodeur ICO de
/// Windows, chaque image a pour vignette elle-même. WPF lit la profondeur
/// de couleur des icônes sur cette vignette (IconHelper.GetBestMatch) et
/// s'arrête sur une référence nulle sans elle.
unsafe extern "win64" fn icon_frame_thumbnail(this: *mut SourceObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let pix = crate::dlls::lock(&Com::get(this).pix).clone();
    match pix {
        Some(p) => new_source(Kind::Bitmap, Some(p), out),
        None => WINCODEC_ERR_NOTINITIALIZED,
    }
}

static DECODER_VT: VTable<14> = VTable([
    decoder_qi as *const (),
    add_ref::<Mutex<Option<Decoder>>> as *const (),
    release::<Mutex<Option<Decoder>>> as *const (),
    decoder_query_capability as *const (),
    decoder_initialize as *const (),
    decoder_container as *const (),
    decoder_info as *const (),
    palette_unavailable as *const (),
    unsupported_operation as *const (), // GetMetadataQueryReader
    unsupported_operation as *const (), // GetPreview
    no_color_contexts as *const (),
    no_thumbnail as *const (),
    decoder_frame_count as *const (),
    decoder_frame as *const (),
]);

// --- Informations de codec (IWICBitmapDecoderInfo) ----------------------------------
//
// WPF demande à chaque décodeur ses informations (GetDecoderInfo) pour lire
// le format de conteneur et les types MIME avant de choisir sa classe de
// décodeur (#1063). Ordre des méthodes et identifiants : wincodec.idl des
// en-têtes publics mingw-w64.

const IID_COMPONENT_INFO: Guid = guid(
    0x23bc3f0a,
    0x698b,
    0x4357,
    [0x88, 0x6b, 0xf2, 0x4d, 0x50, 0x67, 0x13, 0x34],
);
const IID_BITMAP_CODEC_INFO: Guid = guid(
    0xe87a44c4,
    0xb76e,
    0x4c47,
    [0x8b, 0x09, 0x29, 0x8e, 0xb1, 0x2a, 0x27, 0x14],
);
const IID_BITMAP_DECODER_INFO: Guid = guid(
    0xd8cd007f,
    0xd08f,
    0x4191,
    [0x9b, 0xfc, 0x23, 0x6e, 0xa7, 0xf0, 0xe4, 0xb5],
);
/// Identifiant de fournisseur des codecs intégrés, que des applis comparent.
const GUID_VENDOR_BUILTIN: Guid = guid(
    0xf0e749ca,
    0xedef,
    0x4589,
    [0xa7, 0x3a, 0xee, 0x0e, 0x62, 0x6a, 0x2a, 0x2b],
);
/// IWICBitmapEncoderInfo (wincodec.h des en-têtes mingw-w64).
const IID_BITMAP_ENCODER_INFO: Guid = guid(
    0x94c9b4ee,
    0xa09f,
    0x4f92,
    [0x8a, 0x1e, 0x4a, 0x9b, 0xce, 0x7e, 0x76, 0xfb],
);
const WIC_DECODER: u32 = 1;
const WIC_ENCODER: u32 = 2;
const WIC_COMPONENT_SIGNED: u32 = 1;
const ERROR_INSUFFICIENT_BUFFER_HR: i32 = 0x8007_007Au32 as i32;

struct CodecDesc {
    container: Guid,
    clsid: Guid,
    name: &'static str,
    mimes: &'static str,
    exts: &'static str,
    multiframe: bool,
    animation: bool,
    lossless: bool,
}

const CODECS: [CodecDesc; 8] = [
    CodecDesc {
        container: CONTAINER_ICO,
        clsid: guid(
            0xc61bfcdf,
            0x2e0f,
            0x4aad,
            [0xa8, 0xd7, 0xe0, 0x6b, 0xaf, 0xeb, 0xcd, 0xfe],
        ),
        name: "ICO Decoder",
        mimes: "image/ico,image/x-icon",
        exts: ".ico,.icon",
        multiframe: true,
        animation: false,
        lossless: true,
    },
    CodecDesc {
        container: CONTAINER_BMP,
        clsid: guid(
            0x6b462062,
            0x7cbf,
            0x400d,
            [0x9f, 0xdb, 0x81, 0x3d, 0xd1, 0x0f, 0x27, 0x78],
        ),
        name: "BMP Decoder",
        mimes: "image/bmp",
        exts: ".bmp,.dib,.rle",
        multiframe: false,
        animation: false,
        lossless: true,
    },
    CodecDesc {
        container: CONTAINER_PNG,
        clsid: guid(
            0x389ea17b,
            0x5078,
            0x4cde,
            [0xb6, 0xef, 0x25, 0xc1, 0x51, 0x75, 0xc7, 0x51],
        ),
        name: "PNG Decoder",
        mimes: "image/png",
        exts: ".png",
        multiframe: false,
        animation: false,
        lossless: true,
    },
    CodecDesc {
        container: CONTAINER_JPEG,
        clsid: guid(
            0x9456a480,
            0xe88b,
            0x43ea,
            [0x9e, 0x73, 0x0b, 0x2d, 0x9b, 0x71, 0xb1, 0xca],
        ),
        name: "JPEG Decoder",
        mimes: "image/jpeg,image/jpe,image/jpg",
        exts: ".jpeg,.jpe,.jpg,.jfif,.exif",
        multiframe: false,
        animation: false,
        lossless: false,
    },
    CodecDesc {
        container: CONTAINER_GIF,
        clsid: guid(
            0x381dda3c,
            0x9ce9,
            0x4834,
            [0xa2, 0x3e, 0x1f, 0x98, 0xf8, 0xfc, 0x52, 0xbe],
        ),
        name: "GIF Decoder",
        mimes: "image/gif",
        exts: ".gif",
        multiframe: true,
        animation: true,
        lossless: true,
    },
    CodecDesc {
        container: CONTAINER_TIFF,
        clsid: guid(
            0xb54e85d9,
            0xfe23,
            0x499f,
            [0x8b, 0x88, 0x6a, 0xce, 0xa7, 0x13, 0x75, 0x2b],
        ),
        name: "TIFF Decoder",
        mimes: "image/tiff,image/tif",
        exts: ".tiff,.tif",
        multiframe: true,
        animation: false,
        lossless: true,
    },
    CodecDesc {
        container: CONTAINER_WEBP,
        clsid: guid(
            0x7693e886,
            0x51c9,
            0x4070,
            [0x84, 0x19, 0x9f, 0x70, 0x73, 0x8e, 0xc8, 0xfa],
        ),
        name: "WebP Decoder",
        mimes: "image/webp",
        exts: ".webp",
        multiframe: false,
        animation: false,
        lossless: false,
    },
    CodecDesc {
        container: CONTAINER_DDS,
        clsid: guid(
            0x9053699f,
            0xa341,
            0x429d,
            [0x9e, 0x90, 0xee, 0x43, 0x7c, 0xf8, 0x0c, 0x73],
        ),
        name: "DDS Decoder",
        mimes: "image/vnd.ms-dds",
        exts: ".dds",
        multiframe: true,
        animation: false,
        lossless: false,
    },
];

/// Encodeurs intégrés (#1868). Paint.NET demande les informations de
/// l'encodeur PNG (IWICBitmapEncoderInfo) avant de copier une image dans
/// le presse-papiers ; leur indice suit ceux des décodeurs.
const ENCODER_CODECS: [CodecDesc; 1] = [CodecDesc {
    container: CONTAINER_PNG,
    clsid: guid(
        0x27949969,
        0x876a,
        0x41d7,
        [0x94, 0x47, 0x56, 0x8f, 0x6a, 0x35, 0xa4, 0xdc],
    ),
    name: "PNG Encoder",
    mimes: "image/png",
    exts: ".png",
    multiframe: false,
    animation: false,
    lossless: true,
}];

type InfoObj = Com<usize>;

unsafe fn new_codec_info(index: usize, out: *mut *mut c_void) -> i32 {
    put(out, Com::create(CODEC_INFO_VT.ptr(), index))
}

/// Informations du décodeur intégré pour un format de conteneur.
unsafe fn codec_info_for_container(container: &Guid, out: *mut *mut c_void) -> i32 {
    match CODECS.iter().position(|c| c.container == *container) {
        Some(i) => new_codec_info(i, out),
        None => WINCODEC_ERR_COMPONENTNOTFOUND,
    }
}

unsafe fn codec(this: *mut InfoObj) -> &'static CodecDesc {
    let i = *Com::get(this);
    match CODECS.get(i) {
        Some(c) => c,
        None => &ENCODER_CODECS[i - CODECS.len()],
    }
}

unsafe fn is_encoder_info(this: *mut InfoObj) -> bool {
    *Com::get(this) >= CODECS.len()
}

/// Informations de l'encodeur intégré pour un format de conteneur
/// (IWICBitmapEncoder::GetEncoderInfo, #1868).
pub(super) unsafe fn encoder_info_for_container(container: &Guid, out: *mut *mut c_void) -> i32 {
    match ENCODER_CODECS
        .iter()
        .position(|c| c.container == *container)
    {
        Some(i) => put(out, Com::create(ENCODER_INFO_VT.ptr(), CODECS.len() + i)),
        None => WINCODEC_ERR_COMPONENTNOTFOUND,
    }
}

/// Chaîne WIC : (taille du tampon en caractères, tampon, taille réelle).
/// Tampon nul et taille 0 : seule la taille nécessaire est rendue.
unsafe fn wic_string(s: &str, cch: u32, buf: *mut u16, actual: *mut u32) -> i32 {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    if actual.is_null() {
        return E_INVALIDARG;
    }
    *actual = w.len() as u32;
    if buf.is_null() {
        return if cch == 0 { S_OK } else { E_INVALIDARG };
    }
    if (cch as usize) < w.len() {
        return ERROR_INSUFFICIENT_BUFFER_HR;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    S_OK
}

unsafe fn put_value<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = v;
    S_OK
}

unsafe extern "win64" fn info_qi(
    this: *mut InfoObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let last = if is_encoder_info(this) {
        IID_BITMAP_ENCODER_INFO
    } else {
        IID_BITMAP_DECODER_INFO
    };
    qi_list(
        this,
        riid,
        out,
        &[IID_COMPONENT_INFO, IID_BITMAP_CODEC_INFO, last],
    )
}
unsafe extern "win64" fn info_component_type(this: *mut InfoObj, out: *mut u32) -> i32 {
    put_value(
        out,
        if is_encoder_info(this) {
            WIC_ENCODER
        } else {
            WIC_DECODER
        },
    )
}
unsafe extern "win64" fn info_clsid(this: *mut InfoObj, out: *mut Guid) -> i32 {
    put_value(out, codec(this).clsid)
}
unsafe extern "win64" fn info_signing(_this: *mut InfoObj, out: *mut u32) -> i32 {
    put_value(out, WIC_COMPONENT_SIGNED)
}
unsafe extern "win64" fn info_author(
    _this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string("Winlux", cch, buf, actual)
}
unsafe extern "win64" fn info_vendor(_this: *mut InfoObj, out: *mut Guid) -> i32 {
    put_value(out, GUID_VENDOR_BUILTIN)
}
unsafe extern "win64" fn info_version(
    _this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string("1.0.0.0", cch, buf, actual)
}
unsafe extern "win64" fn info_friendly_name(
    this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string(codec(this).name, cch, buf, actual)
}
unsafe extern "win64" fn info_container(this: *mut InfoObj, out: *mut Guid) -> i32 {
    put_value(out, codec(this).container)
}
/// Les décodeurs rendent des images en BGRA 32 bits.
unsafe extern "win64" fn info_pixel_formats(
    _this: *mut InfoObj,
    count: u32,
    formats: *mut Guid,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() {
        return E_INVALIDARG;
    }
    *actual = 1;
    if count == 0 || formats.is_null() {
        return S_OK;
    }
    *formats = FMT_BGRA32;
    S_OK
}
unsafe extern "win64" fn info_empty_string(
    _this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string("", cch, buf, actual)
}
unsafe extern "win64" fn info_mime_types(
    this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string(codec(this).mimes, cch, buf, actual)
}
unsafe extern "win64" fn info_extensions(
    this: *mut InfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string(codec(this).exts, cch, buf, actual)
}
unsafe extern "win64" fn info_animation(this: *mut InfoObj, out: *mut i32) -> i32 {
    put_value(out, codec(this).animation as i32)
}
unsafe extern "win64" fn info_chromakey(this: *mut InfoObj, out: *mut i32) -> i32 {
    put_value(out, (codec(this).container == CONTAINER_GIF) as i32)
}
unsafe extern "win64" fn info_lossless(this: *mut InfoObj, out: *mut i32) -> i32 {
    put_value(out, codec(this).lossless as i32)
}
unsafe extern "win64" fn info_multiframe(this: *mut InfoObj, out: *mut i32) -> i32 {
    put_value(out, codec(this).multiframe as i32)
}
unsafe extern "win64" fn info_matches_mime(
    this: *mut InfoObj,
    mime: *const u16,
    out: *mut i32,
) -> i32 {
    if mime.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let wanted = crate::dlls::wstr(mime);
    *out = codec(this)
        .mimes
        .split(',')
        .any(|m| m.eq_ignore_ascii_case(wanted.trim())) as i32;
    S_OK
}
/// Pas de motifs d'en-tête publiés : la reconnaissance passe par MatchesPattern.
unsafe extern "win64" fn info_patterns(
    _this: *mut InfoObj,
    _size: u32,
    _patterns: *mut c_void,
    count: *mut u32,
    actual: *mut u32,
) -> i32 {
    if count.is_null() || actual.is_null() {
        return E_INVALIDARG;
    }
    *count = 0;
    *actual = 0;
    S_OK
}
unsafe extern "win64" fn info_matches_pattern(
    this: *mut InfoObj,
    stream: *mut c_void,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match read_stream(stream) {
        Ok(bytes) => {
            *out = (container_of(&bytes) == codec(this).container) as i32;
            S_OK
        }
        Err(hr) => hr,
    }
}
unsafe extern "win64" fn info_create_instance(this: *mut InfoObj, out: *mut *mut c_void) -> i32 {
    let container = codec(this).container;
    create_decoder(std::ptr::null_mut(), &container, std::ptr::null(), out)
}

static CODEC_INFO_VT: VTable<26> = VTable([
    info_qi as *const (),
    add_ref::<usize> as *const (),
    release::<usize> as *const (),
    info_component_type as *const (),
    info_clsid as *const (),
    info_signing as *const (),
    info_author as *const (),
    info_vendor as *const (),
    info_version as *const (),
    info_version as *const (), // GetSpecVersion
    info_friendly_name as *const (),
    info_container as *const (),
    info_pixel_formats as *const (),
    info_version as *const (),      // GetColorManagementVersion
    info_empty_string as *const (), // GetDeviceManufacturer
    info_empty_string as *const (), // GetDeviceModels
    info_mime_types as *const (),
    info_extensions as *const (),
    info_animation as *const (),
    info_chromakey as *const (),
    info_lossless as *const (),
    info_multiframe as *const (),
    info_matches_mime as *const (),
    info_patterns as *const (),
    info_matches_pattern as *const (),
    info_create_instance as *const (),
]);

unsafe extern "win64" fn encoder_info_create_instance(
    this: *mut InfoObj,
    out: *mut *mut c_void,
) -> i32 {
    let container = codec(this).container;
    encoder::create_encoder(std::ptr::null_mut(), &container, std::ptr::null(), out)
}

/// IWICBitmapEncoderInfo : IWICBitmapCodecInfo puis CreateInstance.
static ENCODER_INFO_VT: VTable<24> = VTable([
    info_qi as *const (),
    add_ref::<usize> as *const (),
    release::<usize> as *const (),
    info_component_type as *const (),
    info_clsid as *const (),
    info_signing as *const (),
    info_author as *const (),
    info_vendor as *const (),
    info_version as *const (),
    info_version as *const (), // GetSpecVersion
    info_friendly_name as *const (),
    info_container as *const (),
    info_pixel_formats as *const (),
    info_version as *const (),      // GetColorManagementVersion
    info_empty_string as *const (), // GetDeviceManufacturer
    info_empty_string as *const (), // GetDeviceModels
    info_mime_types as *const (),
    info_extensions as *const (),
    info_animation as *const (),
    info_chromakey as *const (),
    info_lossless as *const (),
    info_multiframe as *const (),
    info_matches_mime as *const (),
    encoder_info_create_instance as *const (),
]);

unsafe extern "win64" fn decoder_info(this: *mut DecoderObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let container = match &*crate::dlls::lock(Com::get(this)) {
        Some(d) => d.container,
        None => return WINCODEC_ERR_NOTINITIALIZED,
    };
    codec_info_for_container(&container, out)
}

// --- Informations de format de pixel (IWICPixelFormatInfo2) ---------------------
//
// Paint.NET lit les caractéristiques d'un format (bits par pixel, canaux,
// transparence) par CreateComponentInfo(GUID du format) (#1503). Ordre des
// méthodes, IID, GUID et valeurs d'énumération : wincodec.h des en-têtes
// mingw-w64.

const IID_PIXEL_FORMAT_INFO: Guid = guid(
    0xe8eda601,
    0x3d48,
    0x431a,
    [0xab, 0x44, 0x69, 0x05, 0x9b, 0xe8, 0x8b, 0xbe],
);
const IID_PIXEL_FORMAT_INFO2: Guid = guid(
    0xa9db33a2,
    0xaf5f,
    0x43c7,
    [0xb6, 0x79, 0x74, 0xf5, 0x98, 0x4b, 0x5a, 0xa4],
);
/// WICComponentType::WICPixelFormat.
const WIC_PIXEL_FORMAT: u32 = 0x20;
/// WICPixelFormatNumericRepresentation.
const NUM_INDEXED: u32 = 1;
const NUM_UNSIGNED: u32 = 2;
const NUM_FIXED: u32 = 4;
const NUM_FLOAT: u32 = 5;

/// Description d'un format : GUID, nom, bits par pixel, nombre de canaux,
/// bits par canal (0 : champs irréguliers, voir `irregular_fields`),
/// représentation numérique et présence d'un canal alpha.
struct PixelFormatDesc {
    guid: Guid,
    name: &'static str,
    bpp: u32,
    channels: u32,
    bpc: u32,
    numeric: u32,
    alpha: bool,
}

const fn pf(
    guid: Guid,
    name: &'static str,
    bpp: u32,
    channels: u32,
    bpc: u32,
    numeric: u32,
    alpha: bool,
) -> PixelFormatDesc {
    PixelFormatDesc {
        guid,
        name,
        bpp,
        channels,
        bpc,
        numeric,
        alpha,
    }
}

pub const FMT_ALPHA8: Guid = guid(
    0xe6cd0116,
    0xeeba,
    0x4161,
    [0xaa, 0x85, 0x27, 0xdd, 0x9f, 0xb3, 0xa8, 0x95],
);
const FMT_BGRA5551: Guid = guid(
    0x05ec7c2b,
    0xf1e6,
    0x4961,
    [0xad, 0x46, 0xe1, 0xcc, 0x81, 0x0a, 0x87, 0xd2],
);

static PIXEL_FORMATS: &[PixelFormatDesc] = {
    use wic_pixels::*;
    &[
        pf(
            pixel_format(0x01),
            "1bpp Indexed",
            1,
            1,
            1,
            NUM_INDEXED,
            false,
        ),
        pf(
            pixel_format(0x02),
            "2bpp Indexed",
            2,
            1,
            2,
            NUM_INDEXED,
            false,
        ),
        pf(
            pixel_format(0x03),
            "4bpp Indexed",
            4,
            1,
            4,
            NUM_INDEXED,
            false,
        ),
        pf(
            pixel_format(0x04),
            "8bpp Indexed",
            8,
            1,
            8,
            NUM_INDEXED,
            false,
        ),
        pf(
            pixel_format(0x05),
            "Black White",
            1,
            1,
            1,
            NUM_UNSIGNED,
            false,
        ),
        pf(
            pixel_format(0x06),
            "2bpp Gray",
            2,
            1,
            2,
            NUM_UNSIGNED,
            false,
        ),
        pf(
            pixel_format(0x07),
            "4bpp Gray",
            4,
            1,
            4,
            NUM_UNSIGNED,
            false,
        ),
        pf(FMT_GRAY8, "8bpp Gray", 8, 1, 8, NUM_UNSIGNED, false),
        pf(FMT_ALPHA8, "8bpp Alpha", 8, 1, 8, NUM_UNSIGNED, true),
        pf(
            pixel_format(0x09),
            "16bpp BGR555",
            16,
            3,
            0,
            NUM_UNSIGNED,
            false,
        ),
        pf(
            pixel_format(0x0a),
            "16bpp BGR565",
            16,
            3,
            0,
            NUM_UNSIGNED,
            false,
        ),
        pf(FMT_BGRA5551, "16bpp BGRA5551", 16, 4, 0, NUM_UNSIGNED, true),
        pf(FMT_GRAY16, "16bpp Gray", 16, 1, 16, NUM_UNSIGNED, false),
        pf(
            FMT_GRAY_HALF16,
            "16bpp Gray Half",
            16,
            1,
            16,
            NUM_FLOAT,
            false,
        ),
        pf(
            pixel_format(0x13),
            "16bpp Gray FixedPoint",
            16,
            1,
            16,
            NUM_FIXED,
            false,
        ),
        pf(FMT_BGR24, "24bpp BGR", 24, 3, 8, NUM_UNSIGNED, false),
        pf(FMT_RGB24, "24bpp RGB", 24, 3, 8, NUM_UNSIGNED, false),
        pf(FMT_BGR32, "32bpp BGR", 32, 3, 8, NUM_UNSIGNED, false),
        pf(FMT_RGB32, "32bpp RGB", 32, 3, 8, NUM_UNSIGNED, false),
        pf(FMT_BGRA32, "32bpp BGRA", 32, 4, 8, NUM_UNSIGNED, true),
        pf(FMT_PBGRA32, "32bpp PBGRA", 32, 4, 8, NUM_UNSIGNED, true),
        pf(FMT_RGBA32, "32bpp RGBA", 32, 4, 8, NUM_UNSIGNED, true),
        pf(FMT_PRGBA32, "32bpp PRGBA", 32, 4, 8, NUM_UNSIGNED, true),
        pf(
            pixel_format(0x14),
            "32bpp BGR101010",
            32,
            3,
            0,
            NUM_UNSIGNED,
            false,
        ),
        pf(
            FMT_GRAY_FLOAT32,
            "32bpp Gray Float",
            32,
            1,
            32,
            NUM_FLOAT,
            false,
        ),
        pf(
            pixel_format(0x3f),
            "32bpp Gray FixedPoint",
            32,
            1,
            32,
            NUM_FIXED,
            false,
        ),
        pf(
            pixel_format(0x1c),
            "32bpp CMYK",
            32,
            4,
            8,
            NUM_UNSIGNED,
            false,
        ),
        pf(FMT_RGB48, "48bpp RGB", 48, 3, 16, NUM_UNSIGNED, false),
        pf(FMT_BGR48, "48bpp BGR", 48, 3, 16, NUM_UNSIGNED, false),
        pf(
            FMT_RGB_HALF48,
            "48bpp RGB Half",
            48,
            3,
            16,
            NUM_FLOAT,
            false,
        ),
        pf(FMT_RGB64, "64bpp RGB", 64, 3, 16, NUM_UNSIGNED, false),
        pf(FMT_RGBA64, "64bpp RGBA", 64, 4, 16, NUM_UNSIGNED, true),
        pf(FMT_BGRA64, "64bpp BGRA", 64, 4, 16, NUM_UNSIGNED, true),
        pf(FMT_PRGBA64, "64bpp PRGBA", 64, 4, 16, NUM_UNSIGNED, true),
        pf(FMT_PBGRA64, "64bpp PBGRA", 64, 4, 16, NUM_UNSIGNED, true),
        pf(
            FMT_RGB_HALF64,
            "64bpp RGB Half",
            64,
            3,
            16,
            NUM_FLOAT,
            false,
        ),
        pf(
            FMT_RGBA_HALF64,
            "64bpp RGBA Half",
            64,
            4,
            16,
            NUM_FLOAT,
            true,
        ),
        pf(
            FMT_PRGBA_HALF64,
            "64bpp PRGBA Half",
            64,
            4,
            16,
            NUM_FLOAT,
            true,
        ),
        pf(
            pixel_format(0x1f),
            "64bpp CMYK",
            64,
            4,
            16,
            NUM_UNSIGNED,
            false,
        ),
        pf(
            FMT_RGB_FLOAT96,
            "96bpp RGB Float",
            96,
            3,
            32,
            NUM_FLOAT,
            false,
        ),
        pf(
            FMT_RGB_FLOAT128,
            "128bpp RGB Float",
            128,
            3,
            32,
            NUM_FLOAT,
            false,
        ),
        pf(
            FMT_RGBA_FLOAT128,
            "128bpp RGBA Float",
            128,
            4,
            32,
            NUM_FLOAT,
            true,
        ),
        pf(
            FMT_PRGBA_FLOAT128,
            "128bpp PRGBA Float",
            128,
            4,
            32,
            NUM_FLOAT,
            true,
        ),
    ]
};

/// Champs (premier bit, largeur) des formats aux canaux de tailles inégales,
/// dans l'ordre des canaux du nom (bits de poids faible d'abord).
fn irregular_fields(fmt: &Guid) -> &'static [(u32, u32)] {
    if *fmt == pixel_format(0x09) {
        &[(0, 5), (5, 5), (10, 5)]
    } else if *fmt == pixel_format(0x0a) {
        &[(0, 5), (5, 6), (11, 5)]
    } else if *fmt == FMT_BGRA5551 {
        &[(0, 5), (5, 5), (10, 5), (15, 1)]
    } else if *fmt == pixel_format(0x14) {
        &[(0, 10), (10, 10), (20, 10)]
    } else {
        &[]
    }
}

/// Masque d'un canal : `bpp / 8` octets (arrondi au-dessus), petit-boutiste.
fn channel_mask(d: &PixelFormatDesc, index: u32) -> Option<Vec<u8>> {
    if index >= d.channels {
        return None;
    }
    let (start, width) = if d.bpc == 0 {
        *irregular_fields(&d.guid).get(index as usize)?
    } else {
        (index * d.bpc, d.bpc)
    };
    let mut mask = vec![0u8; d.bpp.div_ceil(8) as usize];
    for bit in start..start + width {
        mask[(bit / 8) as usize] |= 1 << (bit % 8);
    }
    Some(mask)
}

type PixelInfoObj = Com<usize>;

unsafe fn pixel_desc(this: *mut PixelInfoObj) -> &'static PixelFormatDesc {
    &PIXEL_FORMATS[*Com::get(this)]
}

unsafe extern "win64" fn pfi_qi(
    this: *mut PixelInfoObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_COMPONENT_INFO,
            IID_PIXEL_FORMAT_INFO,
            IID_PIXEL_FORMAT_INFO2,
        ],
    )
}
unsafe extern "win64" fn pfi_component_type(_this: *mut PixelInfoObj, out: *mut u32) -> i32 {
    put_value(out, WIC_PIXEL_FORMAT)
}
/// GetCLSID et GetFormatGUID rendent le GUID du format.
unsafe extern "win64" fn pfi_guid(this: *mut PixelInfoObj, out: *mut Guid) -> i32 {
    put_value(out, pixel_desc(this).guid)
}
unsafe extern "win64" fn pfi_friendly_name(
    this: *mut PixelInfoObj,
    cch: u32,
    buf: *mut u16,
    actual: *mut u32,
) -> i32 {
    wic_string(pixel_desc(this).name, cch, buf, actual)
}
/// Contexte de couleur du format : sRGB.
unsafe extern "win64" fn pfi_color_context(_this: *mut PixelInfoObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    put(
        out,
        Com::create(COLOR_CONTEXT_VT.ptr(), Mutex::new(ColorContext::Exif(1))),
    )
}
unsafe extern "win64" fn pfi_bits_per_pixel(this: *mut PixelInfoObj, out: *mut u32) -> i32 {
    put_value(out, pixel_desc(this).bpp)
}
unsafe extern "win64" fn pfi_channel_count(this: *mut PixelInfoObj, out: *mut u32) -> i32 {
    put_value(out, pixel_desc(this).channels)
}
unsafe extern "win64" fn pfi_channel_mask(
    this: *mut PixelInfoObj,
    index: u32,
    cb: u32,
    buf: *mut u8,
    actual: *mut u32,
) -> i32 {
    let Some(mask) = channel_mask(pixel_desc(this), index) else {
        return E_INVALIDARG;
    };
    if actual.is_null() {
        return E_INVALIDARG;
    }
    *actual = mask.len() as u32;
    if buf.is_null() {
        return S_OK;
    }
    if (cb as usize) < mask.len() {
        return E_INVALIDARG;
    }
    std::ptr::copy_nonoverlapping(mask.as_ptr(), buf, mask.len());
    S_OK
}
unsafe extern "win64" fn pfi_transparency(this: *mut PixelInfoObj, out: *mut i32) -> i32 {
    put_value(out, pixel_desc(this).alpha as i32)
}
unsafe extern "win64" fn pfi_numeric(this: *mut PixelInfoObj, out: *mut u32) -> i32 {
    put_value(out, pixel_desc(this).numeric)
}

static PIXEL_FORMAT_INFO_VT: VTable<18> = VTable([
    pfi_qi as *const (),
    add_ref::<usize> as *const (),
    release::<usize> as *const (),
    pfi_component_type as *const (),
    pfi_guid as *const (), // GetCLSID
    info_signing as *const (),
    info_author as *const (),
    info_vendor as *const (),
    info_version as *const (),
    info_version as *const (), // GetSpecVersion
    pfi_friendly_name as *const (),
    pfi_guid as *const (), // GetFormatGUID
    pfi_color_context as *const (),
    pfi_bits_per_pixel as *const (),
    pfi_channel_count as *const (),
    pfi_channel_mask as *const (),
    pfi_transparency as *const (),
    pfi_numeric as *const (),
]);

/// IWICImagingFactory::CreateComponentInfo : décodeurs et encodeurs
/// intégrés, formats de pixel (#1503, #1868).
unsafe extern "win64" fn create_component_info(
    _this: *mut c_void,
    clsid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if clsid.is_null() {
        return E_INVALIDARG;
    }
    if let Some(e) = ENCODER_CODECS.iter().find(|c| c.clsid == *clsid) {
        return encoder_info_for_container(&e.container, out);
    }
    match CODECS.iter().position(|c| c.clsid == *clsid) {
        Some(i) => new_codec_info(i, out),
        None => match PIXEL_FORMATS.iter().position(|p| p.guid == *clsid) {
            Some(i) => put(out, Com::create(PIXEL_FORMAT_INFO_VT.ptr(), i)),
            None => WINCODEC_ERR_COMPONENTNOTFOUND,
        },
    }
}

// --- Palette (IWICPalette) ------------------------------------------------------
//
// WPF crée une palette pour chaque vignette d'image décodée
// (BitmapPalette.CreateInternalPalette, puis CopyPalette) : sans
// CreatePalette, toute icône échoue (#1110). Ordre des méthodes et
// valeurs de WICBitmapPaletteType : wincodec.idl des en-têtes mingw-w64.

const IID_PALETTE: Guid = guid(
    0x00000040,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
const PALETTE_CUSTOM: u32 = 0;
const PALETTE_MEDIAN_CUT: u32 = 1;
const PALETTE_FIXED_BW: u32 = 2;

#[derive(Default)]
struct Palette {
    kind: u32,
    /// Couleurs 0xAARRGGBB (WICColor).
    colors: Vec<u32>,
}

type PaletteObj = Com<Mutex<Option<Palette>>>;

/// Couleurs des palettes prédéfinies : noir et blanc, gris réguliers, ou
/// cube de couleurs (niveaux par canal rouge, vert, bleu).
fn predefined_colors(kind: u32) -> Option<Vec<u32>> {
    let ramp = |n: u32| -> Vec<u32> {
        (0..n)
            .map(|i| {
                let v = i * 255 / (n - 1);
                0xFF00_0000 | (v << 16) | (v << 8) | v
            })
            .collect()
    };
    let cube = |r: u32, g: u32, b: u32| -> Vec<u32> {
        let mut out = Vec::with_capacity((r * g * b) as usize);
        for ri in 0..r {
            for gi in 0..g {
                for bi in 0..b {
                    let c = |i: u32, n: u32| i * 255 / (n - 1);
                    out.push(0xFF00_0000 | (c(ri, r) << 16) | (c(gi, g) << 8) | c(bi, b));
                }
            }
        }
        out
    };
    Some(match kind {
        PALETTE_FIXED_BW => ramp(2),
        3 => cube(2, 2, 2),
        4 => cube(3, 3, 3),
        5 => cube(4, 4, 4),
        6 => cube(5, 5, 5),
        7 => cube(6, 6, 6),
        8 => cube(6, 7, 6),
        9 => cube(8, 8, 4),
        10 => ramp(4),
        11 => ramp(16),
        12 => ramp(256),
        _ => return None,
    })
}

unsafe fn new_palette(out: *mut *mut c_void) -> i32 {
    put(
        out,
        Com::create(PALETTE_VT.ptr(), Mutex::<Option<Palette>>::new(None)),
    )
}

unsafe extern "win64" fn create_palette(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    new_palette(out)
}

unsafe fn set_palette(this: *mut PaletteObj, p: Palette) -> i32 {
    *crate::dlls::lock(Com::get(this)) = Some(p);
    S_OK
}

unsafe fn with_palette<R>(this: *mut PaletteObj, f: impl FnOnce(&Palette) -> R) -> Result<R, i32> {
    match &*crate::dlls::lock(Com::get(this)) {
        Some(p) => Ok(f(p)),
        None => Err(WINCODEC_ERR_NOTINITIALIZED),
    }
}

unsafe extern "win64" fn palette_qi(
    this: *mut PaletteObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_PALETTE])
}

unsafe extern "win64" fn palette_init_predefined(
    this: *mut PaletteObj,
    kind: u32,
    add_transparent: i32,
) -> i32 {
    let Some(mut colors) = predefined_colors(kind) else {
        return E_INVALIDARG;
    };
    if add_transparent != 0 && colors.len() < 256 {
        colors.push(0);
    }
    set_palette(this, Palette { kind, colors })
}

unsafe extern "win64" fn palette_init_custom(
    this: *mut PaletteObj,
    colors: *const u32,
    count: u32,
) -> i32 {
    if count > 256 || (colors.is_null() && count != 0) {
        return E_INVALIDARG;
    }
    let colors = if count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(colors, count as usize).to_vec()
    };
    set_palette(
        this,
        Palette {
            kind: PALETTE_CUSTOM,
            colors,
        },
    )
}

/// Palette tirée d'une image : ses couleurs distinctes, dans l'ordre où
/// elles apparaissent, jusqu'au nombre demandé.
unsafe extern "win64" fn palette_init_from_bitmap(
    this: *mut PaletteObj,
    src: *mut c_void,
    count: u32,
    add_transparent: i32,
) -> i32 {
    if !(2..=256).contains(&count) {
        return E_INVALIDARG;
    }
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    if bytes_per_pixel(&pix.fmt) != Some(4) {
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    }
    let limit = count as usize - (add_transparent != 0) as usize;
    let mut colors: Vec<u32> = Vec::new();
    for px in pix.data.as_chunks::<4>().0 {
        let c = 0xFF00_0000 | u32::from_le_bytes([px[0], px[1], px[2], 0]);
        if !colors.contains(&c) {
            colors.push(c);
            if colors.len() == limit {
                break;
            }
        }
    }
    if add_transparent != 0 {
        colors.push(0);
    }
    set_palette(
        this,
        Palette {
            kind: PALETTE_MEDIAN_CUT,
            colors,
        },
    )
}

unsafe extern "win64" fn palette_init_from_palette(
    this: *mut PaletteObj,
    other: *mut c_void,
) -> i32 {
    if other.is_null() {
        return E_INVALIDARG;
    }
    type GetType = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
    type GetCount = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
    type GetColors = unsafe extern "win64" fn(*mut c_void, u32, *mut u32, *mut u32) -> i32;
    let (mut kind, mut n) = (0u32, 0u32);
    let hr = method::<GetType>(other, 7)(other, &mut kind);
    if hr < 0 {
        return hr;
    }
    let hr = method::<GetCount>(other, 8)(other, &mut n);
    if hr < 0 {
        return hr;
    }
    let mut colors = vec![0u32; n.min(256) as usize];
    let mut got = 0u32;
    let hr =
        method::<GetColors>(other, 9)(other, colors.len() as u32, colors.as_mut_ptr(), &mut got);
    if hr < 0 {
        return hr;
    }
    colors.truncate(got as usize);
    set_palette(this, Palette { kind, colors })
}

unsafe extern "win64" fn palette_type(this: *mut PaletteObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match with_palette(this, |p| p.kind) {
        Ok(k) => put_value(out, k),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn palette_count(this: *mut PaletteObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match with_palette(this, |p| p.colors.len() as u32) {
        Ok(n) => put_value(out, n),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn palette_colors(
    this: *mut PaletteObj,
    count: u32,
    colors: *mut u32,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (colors.is_null() && count != 0) {
        return E_INVALIDARG;
    }
    match with_palette(this, |p| {
        let n = p.colors.len().min(count as usize);
        if n > 0 {
            std::ptr::copy_nonoverlapping(p.colors.as_ptr(), colors, n);
        }
        n as u32
    }) {
        Ok(n) => put_value(actual, n),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn palette_black_white(this: *mut PaletteObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match with_palette(this, |p| p.kind == PALETTE_FIXED_BW) {
        Ok(b) => put_value(out, b as i32),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn palette_grayscale(this: *mut PaletteObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let gray = |c: &u32| {
        let [b, g, r, _] = c.to_le_bytes();
        r == g && g == b
    };
    match with_palette(this, |p| {
        matches!(p.kind, PALETTE_FIXED_BW | 10..=12)
            || (p.kind == PALETTE_CUSTOM && p.colors.iter().all(gray))
    }) {
        Ok(b) => put_value(out, b as i32),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn palette_has_alpha(this: *mut PaletteObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match with_palette(this, |p| p.colors.iter().any(|c| c >> 24 != 0xFF)) {
        Ok(b) => put_value(out, b as i32),
        Err(hr) => hr,
    }
}

static PALETTE_VT: VTable<13> = VTable([
    palette_qi as *const (),
    add_ref::<Mutex<Option<Palette>>> as *const (),
    release::<Mutex<Option<Palette>>> as *const (),
    palette_init_predefined as *const (),
    palette_init_custom as *const (),
    palette_init_from_bitmap as *const (),
    palette_init_from_palette as *const (),
    palette_type as *const (),
    palette_count as *const (),
    palette_colors as *const (),
    palette_black_white as *const (),
    palette_grayscale as *const (),
    palette_has_alpha as *const (),
]);

// --- Sources : image décodée, bitmap, convertisseur, transformations ----------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Frame,
    Bitmap,
    Converter,
    ColorTransform,
    Scaler,
    Clipper,
    FlipRotator,
}

struct Source {
    kind: Kind,
    pix: Mutex<Option<Pix>>,
    locked: Mutex<bool>,
}

type SourceObj = Com<Source>;

unsafe extern "win64" fn source_qi(
    this: *mut SourceObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let own = match Com::get(this).kind {
        Kind::Frame => IID_BITMAP_FRAME_DECODE,
        Kind::Bitmap => IID_BITMAP,
        Kind::Converter => IID_FORMAT_CONVERTER,
        Kind::ColorTransform => IID_COLOR_TRANSFORM,
        Kind::Scaler => IID_BITMAP_SCALER,
        Kind::Clipper => IID_BITMAP_CLIPPER,
        Kind::FlipRotator => IID_BITMAP_FLIP_ROTATOR,
    };
    qi_list(this, riid, out, &[IID_BITMAP_SOURCE, own])
}

unsafe fn with_pix<R>(this: *mut SourceObj, f: impl FnOnce(&Pix) -> R) -> Result<R, i32> {
    match &*crate::dlls::lock(&Com::get(this).pix) {
        Some(p) => Ok(f(p)),
        None => Err(WINCODEC_ERR_NOTINITIALIZED),
    }
}

unsafe extern "win64" fn source_size(this: *mut SourceObj, w: *mut u32, h: *mut u32) -> i32 {
    if w.is_null() || h.is_null() {
        return E_INVALIDARG;
    }
    with_pix(this, |p| {
        *w = p.w;
        *h = p.h;
        S_OK
    })
    .unwrap_or_else(|e| e)
}

unsafe extern "win64" fn source_format(this: *mut SourceObj, fmt: *mut Guid) -> i32 {
    if fmt.is_null() {
        return E_INVALIDARG;
    }
    with_pix(this, |p| {
        *fmt = p.fmt;
        S_OK
    })
    .unwrap_or_else(|e| e)
}

unsafe extern "win64" fn source_resolution(this: *mut SourceObj, x: *mut f64, y: *mut f64) -> i32 {
    if x.is_null() || y.is_null() {
        return E_INVALIDARG;
    }
    with_pix(this, |p| {
        *x = p.dpi.0;
        *y = p.dpi.1;
        S_OK
    })
    .unwrap_or_else(|e| e)
}

unsafe extern "win64" fn source_copy_pixels(
    this: *mut SourceObj,
    rect: *const WicRect,
    stride: u32,
    size: u32,
    buf: *mut u8,
) -> i32 {
    with_pix(this, |p| copy_pixels(p, rect, stride, size, buf)).unwrap_or_else(|e| e)
}

unsafe extern "win64" fn bitmap_lock(
    this: *mut SourceObj,
    rect: *const WicRect,
    flags: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let s = Com::get(this);
    let guard = crate::dlls::lock(&s.pix);
    let Some(p) = guard.as_ref() else {
        return WINCODEC_ERR_NOTINITIALIZED;
    };
    let r = if rect.is_null() {
        WicRect {
            x: 0,
            y: 0,
            w: p.w as i32,
            h: p.h as i32,
        }
    } else {
        *rect
    };
    if r.w <= 0 || r.h <= 0 || !rect_in(&r, p.w, p.h) {
        return E_INVALIDARG;
    }
    // Un verrou en écriture est exclusif ; plusieurs lectures sont permises.
    let write = flags & 2 != 0;
    {
        let mut locked = crate::dlls::lock(&s.locked);
        if *locked {
            return WINCODEC_ERR_ALREADYLOCKED;
        }
        if write {
            *locked = true;
        }
    }
    let bpp = bytes_per_pixel(&p.fmt).unwrap_or(4);
    let offset = r.y as usize * p.stride as usize + r.x as usize * bpp as usize;
    let len = p.stride as usize * (r.h as usize - 1) + r.w as usize * bpp as usize;
    let data = p.data.as_ptr().add(offset) as *mut u8;
    let lock = BitmapLock {
        owner: this,
        write,
        w: r.w as u32,
        h: r.h as u32,
        stride: p.stride,
        fmt: p.fmt,
        data,
        len: len as u32,
    };
    drop(guard);
    Com::add_ref(this);
    put(out, Com::create(LOCK_VT.ptr(), lock))
}

unsafe extern "win64" fn bitmap_set_palette(_this: *mut SourceObj, _palette: *mut c_void) -> i32 {
    S_OK
}

unsafe extern "win64" fn bitmap_set_resolution(this: *mut SourceObj, x: f64, y: f64) -> i32 {
    match &mut *crate::dlls::lock(&Com::get(this).pix) {
        Some(p) => {
            p.dpi = (x, y);
            S_OK
        }
        None => WINCODEC_ERR_NOTINITIALIZED,
    }
}

fn convertible(fmt: &Guid) -> bool {
    bytes_per_pixel(fmt).is_some()
}

unsafe extern "win64" fn converter_initialize(
    this: *mut SourceObj,
    src: *mut c_void,
    dst: *const Guid,
    _dither: u32,
    _palette: *mut c_void,
    _alpha_threshold: f64,
    _palette_kind: u32,
) -> i32 {
    if dst.is_null() {
        return E_INVALIDARG;
    }
    if !convertible(&*dst) {
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    }
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    let Some(converted) = pix.convert(*dst) else {
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    };
    *crate::dlls::lock(&Com::get(this).pix) = Some(converted);
    S_OK
}

unsafe extern "win64" fn converter_can_convert(
    _this: *mut SourceObj,
    src: *const Guid,
    dst: *const Guid,
    ok: *mut i32,
) -> i32 {
    if src.is_null() || dst.is_null() || ok.is_null() {
        return E_INVALIDARG;
    }
    *ok = i32::from(convertible(&*src) && convertible(&*dst));
    S_OK
}

fn clip(pix: &Pix, r: WicRect) -> Option<Pix> {
    if r.w <= 0 || r.h <= 0 || !rect_in(&r, pix.w, pix.h) {
        return None;
    }
    let mut out = Pix::zeroed(r.w as u32, r.h as u32, pix.fmt)?;
    out.dpi = pix.dpi;
    let bpp = bytes_per_pixel(&pix.fmt)? as usize;
    let row = r.w as usize * bpp;
    for y in 0..r.h as usize {
        let s = (r.y as usize + y) * pix.stride as usize + r.x as usize * bpp;
        let d = y * out.stride as usize;
        out.data[d..d + row].copy_from_slice(&pix.data[s..s + row]);
    }
    Some(out)
}

/// Mise à l'échelle : plus proche voisin (mode 0) ou bilinéaire (autres
/// modes), calculée en RGBA puis rendue dans le format d'origine.
fn scale(pix: &Pix, w: u32, h: u32, mode: u32) -> Option<Pix> {
    if w == 0 || h == 0 {
        return None;
    }
    let src = pix.to_rgba();
    let (sw, sh) = (pix.w as usize, pix.h as usize);
    let mut dst = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).max(0.0);
            let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).max(0.0);
            let px: [u8; 4] = if mode == 0 {
                let (ix, iy) = ((fx + 0.5) as usize, (fy + 0.5) as usize);
                let i = (iy.min(sh - 1) * sw + ix.min(sw - 1)) * 4;
                [src[i], src[i + 1], src[i + 2], src[i + 3]]
            } else {
                let (x0, y0) = (fx as usize, fy as usize);
                let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let at = |xx: usize, yy: usize, c: usize| src[(yy * sw + xx) * 4 + c] as f32;
                let mut o = [0u8; 4];
                for (c, v) in o.iter_mut().enumerate() {
                    let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                    let bot = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                    *v = (top * (1.0 - ty) + bot * ty).round() as u8;
                }
                o
            };
            dst[(y * w as usize + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    Pix::from_rgba(w, h, &dst, pix.fmt, pix.dpi)
}

/// WICBitmapTransformOptions : rotations de 0/90/180/270° (0 à 3), puis
/// miroir horizontal (8) ou vertical (0x10).
fn flip_rotate(pix: &Pix, options: u32) -> Option<Pix> {
    let src = pix.to_rgba();
    let (w, h) = (pix.w as usize, pix.h as usize);
    let rot = options & 3;
    let (nw, nh) = if rot % 2 == 1 { (h, w) } else { (w, h) };
    let mut dst = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let (mut dx, mut dy) = (x, y);
            if options & 8 != 0 {
                dx = nw - 1 - dx;
            }
            if options & 0x10 != 0 {
                dy = nh - 1 - dy;
            }
            // Point source correspondant à (dx, dy) après rotation horaire.
            let (sx, sy) = match rot {
                0 => (dx, dy),
                1 => (dy, h - 1 - dx),
                2 => (w - 1 - dx, h - 1 - dy),
                _ => (w - 1 - dy, dx),
            };
            let s = (sy * w + sx) * 4;
            dst[(y * nw + x) * 4..][..4].copy_from_slice(&src[s..s + 4]);
        }
    }
    let dpi = if rot % 2 == 1 {
        (pix.dpi.1, pix.dpi.0)
    } else {
        pix.dpi
    };
    Pix::from_rgba(nw as u32, nh as u32, &dst, pix.fmt, dpi)
}

/// Initialize des transformations : (source, a, b, c) selon le type.
unsafe extern "win64" fn transform_initialize(
    this: *mut SourceObj,
    src: *mut c_void,
    a: usize,
    b: u32,
    c: u32,
) -> i32 {
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    let result = match Com::get(this).kind {
        // Scaler : Initialize(source, largeur, hauteur, mode)
        Kind::Scaler => scale(&pix, a as u32, b, c),
        // Clipper : Initialize(source, const WICRect*)
        Kind::Clipper => {
            if a == 0 {
                return E_INVALIDARG;
            }
            clip(&pix, *(a as *const WicRect))
        }
        // FlipRotator : Initialize(source, options)
        _ => flip_rotate(&pix, a as u32),
    };
    match result {
        Some(p) => {
            *crate::dlls::lock(&Com::get(this).pix) = Some(p);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

macro_rules! source_vtable {
    ($name:ident, $n:literal, [$($extra:expr),*]) => {
        static $name: VTable<$n> = VTable([
            source_qi as *const (),
            add_ref::<Source> as *const (),
            release::<Source> as *const (),
            source_size as *const (),
            source_format as *const (),
            source_resolution as *const (),
            palette_unavailable as *const (),
            source_copy_pixels as *const (),
            $($extra as *const ()),*
        ]);
    };
}
source_vtable!(
    FRAME_VT,
    11,
    [unsupported_operation, no_color_contexts, no_thumbnail]
);
source_vtable!(
    ICON_FRAME_VT,
    11,
    [
        unsupported_operation,
        no_color_contexts,
        icon_frame_thumbnail
    ]
);
source_vtable!(
    BITMAP_VT,
    11,
    [bitmap_lock, bitmap_set_palette, bitmap_set_resolution]
);
source_vtable!(
    CONVERTER_VT,
    10,
    [converter_initialize, converter_can_convert]
);
source_vtable!(TRANSFORM_VT, 9, [transform_initialize]);
source_vtable!(COLOR_TRANSFORM_VT, 9, [color_transform_initialize]);

/// IWICColorTransform {B66F034F-D0E2-40AB-B436-6DE39E321A94} (wincodec.h).
const IID_COLOR_TRANSFORM: Guid = guid(
    0xb66f_034f,
    0xd0e2,
    0x40ab,
    [0xb4, 0x36, 0x6d, 0xe3, 0x9e, 0x32, 0x1a, 0x94],
);

/// Profil matrice + courbes d'un IWICColorContext de Winlux.
unsafe fn context_shaper(ctx: *mut c_void) -> Option<crate::dlls::icc::Shaper> {
    if ctx.is_null() || !std::ptr::eq(*(ctx as *const *const ()), COLOR_CONTEXT_VT.ptr()) {
        return None;
    }
    match &*crate::dlls::lock(Com::get(ctx as *mut ColorContextObj)) {
        ColorContext::Profile(bytes) => crate::dlls::icc::shaper(bytes),
        ColorContext::Exif(1) => crate::dlls::icc::shaper(&crate::dlls::icc::srgb()),
        ColorContext::Exif(2) => Some(crate::dlls::icc::adobe_rgb_shaper()),
        _ => None,
    }
}

/// IWICColorTransform::Initialize : convertit la source du profil source
/// au profil de destination (profils RVB matrice + courbes), puis au
/// format demandé (#1384). Les profils non lus (tables LUT…) laissent les
/// couleurs telles quelles, avec un avertissement.
unsafe extern "win64" fn color_transform_initialize(
    this: *mut SourceObj,
    src: *mut c_void,
    ctx_src: *mut c_void,
    ctx_dst: *mut c_void,
    fmt: *const Guid,
) -> i32 {
    if src.is_null() || ctx_src.is_null() || ctx_dst.is_null() || fmt.is_null() {
        return E_INVALIDARG;
    }
    if bytes_per_pixel(&*fmt).is_none() {
        crate::log_warn!(
            "wic",
            "ColorTransform : format de pixel {} non pris en charge",
            format_name(&*fmt)
        );
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    }
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    let transform = match (context_shaper(ctx_src), context_shaper(ctx_dst)) {
        (Some(a), Some(b)) => crate::dlls::icc::Transform::new(a, b),
        _ => None,
    };
    let Some(transform) = transform else {
        crate::log_warn!(
            "wic",
            "ColorTransform : profil non matrice + courbes, couleurs inchangées"
        );
        let Some(p) = pix.convert(*fmt) else {
            return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
        };
        *crate::dlls::lock(&Com::get(this).pix) = Some(p);
        return S_OK;
    };
    let raw: Vec<[f32; 4]> = pix
        .to_raw()
        .into_iter()
        .map(|c| {
            let o = transform.apply([c[0], c[1], c[2]]);
            [o[0], o[1], o[2], c[3]]
        })
        .collect();
    match Pix::from_raw(pix.w, pix.h, &raw, *fmt, pix.dpi) {
        Some(p) => {
            *crate::dlls::lock(&Com::get(this).pix) = Some(p);
            S_OK
        }
        None => WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT,
    }
}

// --- Verrou de bitmap -----------------------------------------------------------------

struct BitmapLock {
    owner: *mut SourceObj,
    write: bool,
    w: u32,
    h: u32,
    stride: u32,
    fmt: Guid,
    data: *mut u8,
    len: u32,
}

impl Drop for BitmapLock {
    fn drop(&mut self) {
        unsafe {
            if self.write {
                *crate::dlls::lock(&Com::get(self.owner).locked) = false;
            }
            Com::release(self.owner);
        }
    }
}

type LockObj = Com<BitmapLock>;

unsafe extern "win64" fn lock_qi(
    this: *mut LockObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_BITMAP_LOCK])
}
unsafe extern "win64" fn lock_size(this: *mut LockObj, w: *mut u32, h: *mut u32) -> i32 {
    if w.is_null() || h.is_null() {
        return E_INVALIDARG;
    }
    let l = Com::get(this);
    *w = l.w;
    *h = l.h;
    S_OK
}
unsafe extern "win64" fn lock_stride(this: *mut LockObj, stride: *mut u32) -> i32 {
    if stride.is_null() {
        return E_INVALIDARG;
    }
    *stride = Com::get(this).stride;
    S_OK
}
unsafe extern "win64" fn lock_data(this: *mut LockObj, size: *mut u32, data: *mut *mut u8) -> i32 {
    if size.is_null() || data.is_null() {
        return E_INVALIDARG;
    }
    let l = Com::get(this);
    *size = l.len;
    *data = l.data;
    S_OK
}
unsafe extern "win64" fn lock_format(this: *mut LockObj, fmt: *mut Guid) -> i32 {
    if fmt.is_null() {
        return E_INVALIDARG;
    }
    *fmt = Com::get(this).fmt;
    S_OK
}

static LOCK_VT: VTable<7> = VTable([
    lock_qi as *const (),
    add_ref::<BitmapLock> as *const (),
    release::<BitmapLock> as *const (),
    lock_size as *const (),
    lock_stride as *const (),
    lock_data as *const (),
    lock_format as *const (),
]);

// --- IWICStream ---------------------------------------------------------------------

#[derive(Default)]
struct Stream {
    data: Vec<u8>,
    pos: usize,
    /// Mémoire de l'appli (InitializeFromMemory) : lectures et écritures
    /// s'y font directement, sans pouvoir l'agrandir.
    memory: Option<(*mut u8, usize)>,
    /// Fichier ouvert en écriture : écrit à Commit et à la libération.
    file: Option<std::path::PathBuf>,
    ready: bool,
}

impl Stream {
    fn with(
        data: Vec<u8>,
        memory: Option<(*mut u8, usize)>,
        file: Option<std::path::PathBuf>,
    ) -> Stream {
        Stream {
            data,
            pos: 0,
            memory,
            file,
            ready: true,
        }
    }
    fn len(&self) -> usize {
        self.memory.map_or(self.data.len(), |(_, n)| n)
    }
    fn flush(&self) -> bool {
        match &self.file {
            Some(path) => std::fs::write(path, &self.data).is_ok(),
            None => true,
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.flush();
    }
}

type StreamObj = Com<Mutex<Stream>>;

unsafe extern "win64" fn stream_qi(
    this: *mut StreamObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_ISEQUENTIAL_STREAM, IID_ISTREAM, IID_WIC_STREAM],
    )
}

unsafe extern "win64" fn stream_read(
    this: *mut StreamObj,
    buf: *mut u8,
    cb: u32,
    read: *mut u32,
) -> i32 {
    let mut s = crate::dlls::lock(Com::get(this));
    if !s.ready {
        return WINCODEC_ERR_NOTINITIALIZED;
    }
    if buf.is_null() && cb != 0 {
        return E_POINTER;
    }
    let n = (cb as usize).min(s.len().saturating_sub(s.pos));
    if n > 0 {
        let src = match s.memory {
            Some((p, _)) => p.add(s.pos) as *const u8,
            None => s.data.as_ptr().add(s.pos),
        };
        std::ptr::copy_nonoverlapping(src, buf, n);
    }
    s.pos += n;
    if !read.is_null() {
        *read = n as u32;
    }
    if n < cb as usize {
        1 // S_FALSE : fin du flux
    } else {
        S_OK
    }
}

unsafe extern "win64" fn stream_write(
    this: *mut StreamObj,
    buf: *const u8,
    cb: u32,
    written: *mut u32,
) -> i32 {
    let mut s = crate::dlls::lock(Com::get(this));
    if !s.ready {
        return WINCODEC_ERR_NOTINITIALIZED;
    }
    if buf.is_null() && cb != 0 {
        return E_POINTER;
    }
    let pos = s.pos;
    let n = match s.memory {
        Some((p, len)) => {
            let n = (cb as usize).min(len.saturating_sub(pos));
            std::ptr::copy_nonoverlapping(buf, p.add(pos), n);
            n
        }
        None => {
            let end = pos + cb as usize;
            if s.data.len() < end {
                s.data.resize(end, 0);
            }
            std::ptr::copy_nonoverlapping(buf, s.data.as_mut_ptr().add(pos), cb as usize);
            cb as usize
        }
    };
    s.pos += n;
    if !written.is_null() {
        *written = n as u32;
    }
    if n < cb as usize {
        0x8003_0070u32 as i32 // STG_E_MEDIUMFULL
    } else {
        S_OK
    }
}

unsafe extern "win64" fn stream_seek(
    this: *mut StreamObj,
    offset: i64,
    origin: u32,
    new_pos: *mut u64,
) -> i32 {
    let mut s = crate::dlls::lock(Com::get(this));
    if !s.ready {
        return WINCODEC_ERR_NOTINITIALIZED;
    }
    let base = match origin {
        0 => 0i64,
        1 => s.pos as i64,
        2 => s.len() as i64,
        _ => return STG_E_INVALIDFUNCTION,
    };
    let target = base.saturating_add(offset);
    if target < 0 || (s.memory.is_some() && target as usize > s.len()) {
        return E_INVALIDARG;
    }
    s.pos = target as usize;
    if !new_pos.is_null() {
        *new_pos = target as u64;
    }
    S_OK
}

unsafe extern "win64" fn stream_set_size(this: *mut StreamObj, size: u64) -> i32 {
    let mut s = crate::dlls::lock(Com::get(this));
    if s.memory.is_some() {
        return STG_E_INVALIDFUNCTION;
    }
    s.data.resize(size as usize, 0);
    S_OK
}

unsafe extern "win64" fn stream_commit(this: *mut StreamObj, _flags: u32) -> i32 {
    if crate::dlls::lock(Com::get(this)).flush() {
        S_OK
    } else {
        STG_E_ACCESSDENIED
    }
}

unsafe extern "win64" fn invalid_function() -> i32 {
    STG_E_INVALIDFUNCTION
}

/// Stat : STATSTG de 80 octets, nom omis (STATFLAG_NONAME implicite).
unsafe extern "win64" fn stream_stat(this: *mut StreamObj, stat: *mut u8, _flag: u32) -> i32 {
    if stat.is_null() {
        return E_POINTER;
    }
    let len = crate::dlls::lock(Com::get(this)).len() as u64;
    std::ptr::write_bytes(stat, 0, 80);
    (stat.add(8) as *mut u32).write_unaligned(2); // STGTY_STREAM
    (stat.add(16) as *mut u64).write_unaligned(len);
    S_OK
}

unsafe extern "win64" fn stream_init_from_istream(this: *mut StreamObj, other: *mut c_void) -> i32 {
    match read_stream(other) {
        Ok(d) => {
            let mut s = crate::dlls::lock(Com::get(this));
            *s = Stream::with(d, None, None);
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn stream_init_from_filename(
    this: *mut StreamObj,
    name: *const u16,
    access: u32,
) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    let path = crate::paths::to_linux(&crate::dlls::wstr(name));
    let write = access & 0x4000_0000 != 0; // GENERIC_WRITE
    let data = if write {
        if std::fs::write(&path, b"").is_err() {
            return STG_E_ACCESSDENIED;
        }
        Vec::new()
    } else {
        match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => return STG_E_FILENOTFOUND,
        }
    };
    let mut s = crate::dlls::lock(Com::get(this));
    *s = Stream::with(data, None, write.then_some(path));
    S_OK
}

unsafe extern "win64" fn stream_init_from_memory(
    this: *mut StreamObj,
    buf: *mut u8,
    size: u32,
) -> i32 {
    if buf.is_null() {
        return E_INVALIDARG;
    }
    let mut s = crate::dlls::lock(Com::get(this));
    *s = Stream::with(Vec::new(), Some((buf, size as usize)), None);
    S_OK
}

unsafe extern "win64" fn stream_init_from_region(
    this: *mut StreamObj,
    other: *mut c_void,
    offset: u64,
    size: u64,
) -> i32 {
    match read_stream(other) {
        Ok(d) => {
            let start = (offset as usize).min(d.len());
            let end = start.saturating_add(size as usize).min(d.len());
            let mut s = crate::dlls::lock(Com::get(this));
            *s = Stream::with(d[start..end].to_vec(), None, None);
            S_OK
        }
        Err(hr) => hr,
    }
}

static STREAM_VT: VTable<18> = VTable([
    stream_qi as *const (),
    add_ref::<Mutex<Stream>> as *const (),
    release::<Mutex<Stream>> as *const (),
    stream_read as *const (),
    stream_write as *const (),
    stream_seek as *const (),
    stream_set_size as *const (),
    unsupported!("CopyTo"),
    stream_commit as *const (),
    unsupported!("Revert"),
    invalid_function as *const (), // LockRegion
    invalid_function as *const (), // UnlockRegion
    stream_stat as *const (),
    unsupported!("Clone"),
    stream_init_from_istream as *const (),
    stream_init_from_filename as *const (),
    stream_init_from_memory as *const (),
    stream_init_from_region as *const (),
]);

// --- Points d'entrée ------------------------------------------------------------------

unsafe fn create_factory(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let f = Com::create(FACTORY_VT.ptr(), Factory);
    let hr = factory_qi(f, riid, out);
    Com::release(f);
    hr
}

/// Fabrique de classes pour CoCreateInstance (voir `audio_com`).
pub fn creator(clsid: &Guid) -> Option<super::audio_com::Creator> {
    if *clsid == CLSID_WIC_IMAGING_FACTORY || *clsid == CLSID_WIC_IMAGING_FACTORY1 {
        Some(create_factory)
    } else {
        None
    }
}

/// WICCreateImagingFactory_Proxy : point d'entrée direct utilisé par
/// certaines applis et bibliothèques à la place de CoCreateInstance.
pub unsafe extern "win64" fn wic_create_imaging_factory_proxy(
    _version: u32,
    out: *mut *mut c_void,
) -> i32 {
    create_factory(&IID_IMAGING_FACTORY, out)
}

/// WICConvertBitmapSource : nouvelle source au format demandé.
pub unsafe extern "win64" fn wic_convert_bitmap_source(
    dst: *const Guid,
    src: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || dst.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    match pix.convert(*dst) {
        Some(p) => new_source(Kind::Converter, Some(p), out),
        None => WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT,
    }
}

pub fn exports() -> crate::dlls::Exports {
    let mut m = crate::exports! {
        "WICCreateImagingFactory_Proxy" => fn wic_create_imaging_factory_proxy,
        "WICConvertBitmapSource" => fn wic_convert_bitmap_source,
        "DllGetClassObject" => fn dll_get_class_object,
    };
    // Relais `Interface_Méthode_Proxy` importés par WPF (#1063).
    m.extend(super::windowscodecs_proxy::exports());
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    type Unary = unsafe extern "win64" fn(*mut c_void) -> u32;
    type Create = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;

    unsafe fn release_obj(o: *mut c_void) {
        method::<Unary>(o, 2)(o);
    }

    /// BMP 24 bits 2×2 : en bas rouge puis vert, en haut bleu puis blanc.
    fn bmp_2x2() -> Vec<u8> {
        let mut d = Vec::new();
        let row = [0u8, 0, 255, 0, 255, 0, 0, 0]; // BGR rouge, BGR vert, 2 octets de marge
        let top = [255u8, 0, 0, 255, 255, 255, 0, 0];
        let pixels = [row, top].concat();
        let size = 54 + pixels.len() as u32;
        d.extend_from_slice(b"BM");
        d.extend_from_slice(&size.to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(&54u32.to_le_bytes());
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        d.extend_from_slice(&pixels);
        d
    }

    unsafe fn factory() -> *mut c_void {
        let mut f = std::ptr::null_mut();
        assert_eq!(create_factory(&IID_IMAGING_FACTORY, &mut f), S_OK);
        f
    }

    /// Paint.NET (ImagingFactoryWrapper, ColorContexts.Srgb) : la fabrique
    /// répond à IWICComponentFactory, puis crée des contextes de couleur
    /// (#1343).
    #[test]
    fn fabrique_de_composants_et_contextes_de_couleur() {
        type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
        type InitU32 = unsafe extern "win64" fn(*mut c_void, u32) -> i32;
        type GetU32 = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
        type InitMem = unsafe extern "win64" fn(*mut c_void, *const u8, u32) -> i32;
        type Bytes = unsafe extern "win64" fn(*mut c_void, u32, *mut u8, *mut u32) -> i32;
        unsafe {
            let f = factory();
            let mut cf = std::ptr::null_mut();
            assert_eq!(method::<Qi>(f, 0)(f, &IID_COMPONENT_FACTORY, &mut cf), S_OK);
            assert!(!cf.is_null());
            // La facette a 35 cases : la 15 est CreateColorContext.
            let mut back = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(cf, 0)(cf, &IID_COMPONENT_FACTORY, &mut back),
                S_OK
            );
            assert_eq!(back, cf);
            release_obj(back);
            let mut f2 = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(cf, 0)(cf, &IID_IMAGING_FACTORY2, &mut f2),
                S_OK
            );
            release_obj(f2);
            let mut cc = std::ptr::null_mut();
            assert_eq!(method::<Create>(cf, 15)(cf, &mut cc), S_OK);
            let mut kind = 9;
            assert_eq!(method::<GetU32>(cc, 6)(cc, &mut kind), S_OK);
            assert_eq!(kind, 0);
            // sRGB par l'espace EXIF.
            assert_eq!(method::<InitU32>(cc, 5)(cc, 1), S_OK);
            assert_eq!(method::<GetU32>(cc, 6)(cc, &mut kind), S_OK);
            assert_eq!(kind, COLOR_CONTEXT_EXIF);
            let mut exif = 0;
            assert_eq!(method::<GetU32>(cc, 8)(cc, &mut exif), S_OK);
            assert_eq!(exif, 1);
            assert_eq!(method::<InitU32>(cc, 5)(cc, 7), E_INVALIDARG);
            // Profil ICC en mémoire, relu octet pour octet.
            let profil = [1u8, 2, 3, 4, 5];
            assert_eq!(method::<InitMem>(cc, 4)(cc, profil.as_ptr(), 5), S_OK);
            let mut n = 0;
            assert_eq!(
                method::<Bytes>(cc, 7)(cc, 0, std::ptr::null_mut(), &mut n),
                S_OK
            );
            assert_eq!(n, 5);
            let mut lu = [0u8; 5];
            assert_eq!(method::<Bytes>(cc, 7)(cc, 5, lu.as_mut_ptr(), &mut n), S_OK);
            assert_eq!(lu, profil);
            release_obj(cc);
            // La fabrique principale crée aussi des contextes.
            let mut cc2 = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 15)(f, &mut cc2), S_OK);
            release_obj(cc2);
            release_obj(cf);
            release_obj(f);
        }
    }

    /// #1063 : PresentationCore appelle WIC par les fonctions exportées
    /// `Interface_Method_Proxy` ; elles doivent atteindre la bonne case de la
    /// table de l'objet.
    #[test]
    fn fonctions_proxy_exportees_vers_l_objet() {
        use super::super::windowscodecs_proxy as px;
        unsafe {
            let f = factory();
            let mut bmp: *mut c_void = std::ptr::null_mut();
            let fmt = FMT_PBGRA32;
            assert_eq!(
                px::p_iwicimaging_factory_create_bitmap(
                    f,
                    5,
                    4,
                    &fmt as *const Guid as usize,
                    0,
                    &mut bmp as *mut *mut c_void as usize,
                    0,
                    0
                ),
                S_OK
            );
            let (mut w, mut h) = (0u32, 0u32);
            assert_eq!(
                px::p_iwicbitmap_source_get_size(
                    bmp,
                    &mut w as *mut u32 as usize,
                    &mut h as *mut u32 as usize,
                    0,
                    0,
                    0,
                    0,
                    0
                ),
                S_OK
            );
            assert_eq!((w, h), (5, 4));
            assert_eq!(
                px::p_iwicbitmap_source_get_size(std::ptr::null_mut(), 0, 0, 0, 0, 0, 0, 0),
                0x8000_4003u32 as i32
            );
            let noms = px::exports();
            assert!(noms.contains_key("IWICImagingFactory_CreateDecoderFromFileHandle_Proxy"));
            assert!(noms.contains_key("WICCreateColorContext_Proxy"));
            release_obj(bmp);
        }
    }

    /// Icône 1×1 en 32 bits : en-tête ICO, une entrée, DIB double hauteur.
    fn ico_1x1() -> Vec<u8> {
        let mut d = vec![0, 0, 1, 0, 1, 0];
        let dib_len = 40u32 + 4 + 4;
        d.extend_from_slice(&[1, 1, 0, 0, 1, 0, 32, 0]);
        d.extend_from_slice(&dib_len.to_le_bytes());
        d.extend_from_slice(&22u32.to_le_bytes());
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&1i32.to_le_bytes());
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        d.extend_from_slice(&[255, 0, 0, 255]); // bleu opaque
        d.extend_from_slice(&[0; 4]); // masque ET
        d
    }

    /// #1099 : WPF refuse une image dont le décodeur n'a pas d'informations :
    /// les icônes (.ico) des ressources XAML doivent en avoir.
    #[test]
    fn icone_reconnue_avec_ses_informations() {
        unsafe {
            let bytes = ico_1x1();
            assert_eq!(container_of(&bytes), CONTAINER_ICO);
            let dec = decoder_from_bytes(&bytes).unwrap();
            assert_eq!(dec.container, CONTAINER_ICO);
            let mut obj = std::ptr::null_mut();
            assert_eq!(new_decoder(dec, &mut obj), S_OK);
            let mut info = std::ptr::null_mut();
            assert_eq!(method::<Create>(obj, 6)(obj, &mut info), S_OK);
            type GetGuid = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
            let mut c = Guid::default();
            assert_eq!(method::<GetGuid>(info, 11)(info, &mut c), S_OK);
            assert_eq!(c, CONTAINER_ICO);
            release_obj(info);
            // Chaque image d'icône a une vignette de même taille.
            type GetFrame = unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
            let mut frame = std::ptr::null_mut();
            assert_eq!(method::<GetFrame>(obj, 13)(obj, 0, &mut frame), S_OK);
            let mut thumb = std::ptr::null_mut();
            assert_eq!(method::<Create>(frame, 10)(frame, &mut thumb), S_OK);
            let pix = read_source(thumb).unwrap();
            assert_eq!((pix.w, pix.h), (1, 1));
            release_obj(thumb);
            release_obj(frame);
            release_obj(obj);
        }
    }

    /// #1110 : palettes WIC, prédéfinies, personnalisées et copiées.
    #[test]
    fn palettes_wic() {
        unsafe {
            let f = factory();
            let mut pal = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 9)(f, &mut pal), S_OK);
            type Count = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
            type Flag = unsafe extern "win64" fn(*mut c_void, *mut i32) -> i32;
            type Colors = unsafe extern "win64" fn(*mut c_void, u32, *mut u32, *mut u32) -> i32;
            let mut n = 0u32;
            assert_eq!(
                method::<Count>(pal, 8)(pal, &mut n),
                WINCODEC_ERR_NOTINITIALIZED
            );
            type Pre = unsafe extern "win64" fn(*mut c_void, u32, i32) -> i32;
            assert_eq!(method::<Pre>(pal, 3)(pal, 7, 1), S_OK);
            assert_eq!(method::<Count>(pal, 8)(pal, &mut n), S_OK);
            assert_eq!(n, 217);
            let mut alpha = 0;
            assert_eq!(method::<Flag>(pal, 12)(pal, &mut alpha), S_OK);
            assert_eq!(alpha, 1);
            type Custom = unsafe extern "win64" fn(*mut c_void, *const u32, u32) -> i32;
            let gris = [0xFF00_0000u32, 0xFF80_8080, 0xFFFF_FFFF];
            assert_eq!(method::<Custom>(pal, 4)(pal, gris.as_ptr(), 3), S_OK);
            let mut g = 0;
            assert_eq!(method::<Flag>(pal, 11)(pal, &mut g), S_OK);
            assert_eq!(g, 1);
            let mut copie = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 9)(f, &mut copie), S_OK);
            type FromPal = unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32;
            assert_eq!(method::<FromPal>(copie, 6)(copie, pal), S_OK);
            let mut out = [0u32; 4];
            let mut got = 0u32;
            assert_eq!(
                method::<Colors>(copie, 9)(copie, 4, out.as_mut_ptr(), &mut got),
                S_OK
            );
            assert_eq!(&out[..got as usize], &gris);
            release_obj(copie);
            release_obj(pal);
        }
    }

    #[test]
    fn decode_un_bmp_depuis_un_flux_memoire() {
        unsafe {
            let f = factory();
            let mut bytes = bmp_2x2();
            let mut stream = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 14)(f, &mut stream), S_OK);
            type InitMem = unsafe extern "win64" fn(*mut c_void, *mut u8, u32) -> i32;
            assert_eq!(
                method::<InitMem>(stream, 16)(stream, bytes.as_mut_ptr(), bytes.len() as u32),
                S_OK
            );
            type FromStream = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const Guid,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut dec = std::ptr::null_mut();
            assert_eq!(
                method::<FromStream>(f, 4)(f, stream, std::ptr::null(), 0, &mut dec),
                S_OK
            );
            let mut container = Guid::default();
            type GetGuid = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
            assert_eq!(method::<GetGuid>(dec, 5)(dec, &mut container), S_OK);
            assert_eq!(container, CONTAINER_BMP);
            // #1063 : WPF lit le format et les types MIME par GetDecoderInfo.
            let mut info = std::ptr::null_mut();
            assert_eq!(method::<Create>(dec, 6)(dec, &mut info), S_OK);
            let mut c2 = Guid::default();
            assert_eq!(method::<GetGuid>(info, 11)(info, &mut c2), S_OK);
            assert_eq!(c2, CONTAINER_BMP);
            type GetStr = unsafe extern "win64" fn(*mut c_void, u32, *mut u16, *mut u32) -> i32;
            let mut len = 0u32;
            assert_eq!(
                method::<GetStr>(info, 16)(info, 0, std::ptr::null_mut(), &mut len),
                S_OK
            );
            assert_eq!(len, 10);
            let mut buf = vec![0u16; len as usize];
            assert_eq!(
                method::<GetStr>(info, 16)(info, len, buf.as_mut_ptr(), &mut len),
                S_OK
            );
            assert_eq!(String::from_utf16_lossy(&buf[..9]), "image/bmp");
            let mut clsid = Guid::default();
            assert_eq!(method::<GetGuid>(info, 4)(info, &mut clsid), S_OK);
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let mut same = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(info, 0)(info, &IID_BITMAP_DECODER_INFO, &mut same),
                S_OK
            );
            release_obj(same);
            release_obj(info);
            // La fabrique rend les mêmes informations à partir du CLSID.
            type ById = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let mut info2 = std::ptr::null_mut();
            assert_eq!(method::<ById>(f, 6)(f, &clsid, &mut info2), S_OK);
            let mut c3 = Guid::default();
            assert_eq!(method::<GetGuid>(info2, 11)(info2, &mut c3), S_OK);
            assert_eq!(c3, CONTAINER_BMP);
            let mut dec2 = std::ptr::null_mut();
            assert_eq!(method::<Create>(info2, 25)(info2, &mut dec2), S_OK);
            release_obj(dec2);
            release_obj(info2);
            assert_eq!(
                method::<ById>(f, 6)(f, &Guid::default(), &mut std::ptr::null_mut()),
                WINCODEC_ERR_COMPONENTNOTFOUND
            );
            type GetFrame = unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
            let mut frame = std::ptr::null_mut();
            assert_eq!(method::<GetFrame>(dec, 13)(dec, 0, &mut frame), S_OK);
            assert_eq!(
                method::<GetFrame>(dec, 13)(dec, 1, &mut std::ptr::null_mut()),
                WINCODEC_ERR_FRAMEMISSING
            );
            let pix = read_source(frame).unwrap();
            assert_eq!((pix.w, pix.h, pix.fmt), (2, 2, FMT_BGRA32));
            // Ligne du haut : bleu puis blanc (BGRA).
            assert_eq!(&pix.data[..8], &[255, 0, 0, 255, 255, 255, 255, 255]);
            assert_eq!(&pix.data[8..16], &[0, 0, 255, 255, 0, 255, 0, 255]);

            // Convertisseur vers RGB 24 bits, puis CopyPixels d'un rectangle.
            let mut conv = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 10)(f, &mut conv), S_OK);
            type Init = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const Guid,
                u32,
                *mut c_void,
                f64,
                u32,
            ) -> i32;
            assert_eq!(
                method::<Init>(conv, 8)(conv, frame, &FMT_RGB24, 0, std::ptr::null_mut(), 0.0, 0),
                S_OK
            );
            type CopyPx =
                unsafe extern "win64" fn(*mut c_void, *const WicRect, u32, u32, *mut u8) -> i32;
            let mut out = [0u8; 3];
            let r = WicRect {
                x: 1,
                y: 1,
                w: 1,
                h: 1,
            };
            assert_eq!(
                method::<CopyPx>(conv, 7)(conv, &r, 3, 3, out.as_mut_ptr()),
                S_OK
            );
            assert_eq!(out, [0, 255, 0]);
            let bad = WicRect {
                x: 1,
                y: 1,
                w: 2,
                h: 1,
            };
            assert_eq!(
                method::<CopyPx>(conv, 7)(conv, &bad, 6, 6, out.as_mut_ptr()),
                E_INVALIDARG
            );

            for o in [conv, frame, dec, stream, f] {
                release_obj(o);
            }
        }
    }

    #[test]
    fn bitmap_verrouille_et_ecrit() {
        unsafe {
            let f = factory();
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
                method::<CreateBmp>(f, 17)(f, 3, 2, &FMT_PBGRA32, 0, &mut bmp),
                S_OK
            );
            type Lock =
                unsafe extern "win64" fn(*mut c_void, *const WicRect, u32, *mut *mut c_void) -> i32;
            let mut lock = std::ptr::null_mut();
            assert_eq!(
                method::<Lock>(bmp, 8)(bmp, std::ptr::null(), 2, &mut lock),
                S_OK
            );
            let mut again = std::ptr::null_mut();
            assert_eq!(
                method::<Lock>(bmp, 8)(bmp, std::ptr::null(), 2, &mut again),
                WINCODEC_ERR_ALREADYLOCKED
            );
            type Data = unsafe extern "win64" fn(*mut c_void, *mut u32, *mut *mut u8) -> i32;
            let (mut size, mut data) = (0u32, std::ptr::null_mut());
            assert_eq!(method::<Data>(lock, 5)(lock, &mut size, &mut data), S_OK);
            assert_eq!(size, 12 * 2);
            *data.add(12) = 9; // premier pixel de la deuxième ligne, bleu
            release_obj(lock);
            assert_eq!(
                method::<Lock>(bmp, 8)(bmp, std::ptr::null(), 2, &mut again),
                S_OK
            );
            release_obj(again);
            let pix = read_source(bmp).unwrap();
            assert_eq!(pix.data[12], 9);
            release_obj(bmp);
            release_obj(f);
        }
    }

    /// #1384 : Paint.NET (ColorProfile.CanWicColorConvertFrom) crée un
    /// bitmap flottant puis le convertit d'un profil à l'autre.
    #[test]
    fn bitmap_flottant_et_transformation_de_couleur() {
        use crate::dlls::wic_pixels::{FMT_PRGBA_FLOAT128, FMT_RGBA64, FMT_RGBA_FLOAT128};
        type CreateBmp = unsafe extern "win64" fn(
            *mut c_void,
            u32,
            u32,
            *const Guid,
            u32,
            *mut *mut c_void,
        ) -> i32;
        type InitU32 = unsafe extern "win64" fn(*mut c_void, u32) -> i32;
        type InitMem = unsafe extern "win64" fn(*mut c_void, *const u8, u32) -> i32;
        type Init = unsafe extern "win64" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *const Guid,
        ) -> i32;
        type Copy = unsafe extern "win64" fn(*mut c_void, *const WicRect, u32, u32, *mut u8) -> i32;
        unsafe {
            let f = factory();
            for fmt in [FMT_RGBA_FLOAT128, FMT_PRGBA_FLOAT128, FMT_RGBA64] {
                let mut bmp = std::ptr::null_mut();
                assert_eq!(method::<CreateBmp>(f, 17)(f, 2, 1, &fmt, 0, &mut bmp), S_OK);
                release_obj(bmp);
            }
            // Un format inconnu reste refusé.
            let mut bmp = std::ptr::null_mut();
            let inconnu = guid(1, 2, 3, [4; 8]);
            assert_eq!(
                method::<CreateBmp>(f, 17)(f, 1, 1, &inconnu, 0, &mut bmp),
                WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT
            );
            // Gris sRGB 50 % → scRGB (profil linéaire) en flottant : ≈ 0,214.
            let src =
                Pix::from_rgba(1, 1, &[128, 128, 128, 255], FMT_BGRA32, (96.0, 96.0)).unwrap();
            let mut sb = std::ptr::null_mut();
            assert_eq!(new_source(Kind::Bitmap, Some(src), &mut sb), S_OK);
            let (mut cs, mut cd) = (std::ptr::null_mut(), std::ptr::null_mut());
            assert_eq!(method::<Create>(f, 15)(f, &mut cs), S_OK);
            assert_eq!(method::<Create>(f, 15)(f, &mut cd), S_OK);
            assert_eq!(method::<InitU32>(cs, 5)(cs, 1), S_OK);
            let lin = crate::dlls::icc::scrgb();
            assert_eq!(
                method::<InitMem>(cd, 4)(cd, lin.as_ptr(), lin.len() as u32),
                S_OK
            );
            let mut t = std::ptr::null_mut();
            assert_eq!(method::<Create>(f, 16)(f, &mut t), S_OK);
            assert_eq!(
                method::<Init>(t, 8)(t, sb, cs, cd, &FMT_RGBA_FLOAT128),
                S_OK
            );
            let mut px = [0u8; 16];
            assert_eq!(
                method::<Copy>(t, 7)(t, std::ptr::null(), 16, 16, px.as_mut_ptr()),
                S_OK
            );
            let r = f32::from_le_bytes(px[0..4].try_into().unwrap());
            let a = f32::from_le_bytes(px[12..16].try_into().unwrap());
            assert!((r - 0.2158).abs() < 0.003, "{r}");
            assert_eq!(a, 1.0);
            // Conversion de format seule : flottant linéaire → BGRA sRGB.
            let p = read_source(t).unwrap().convert(FMT_BGRA32).unwrap();
            assert!((p.data[0] as i32 - 128).abs() <= 1, "{}", p.data[0]);
            for o in [t, cs, cd, sb, f] {
                release_obj(o);
            }
        }
    }

    #[test]
    fn rotation_miroir_et_mise_a_l_echelle() {
        // 2×1 : rouge puis vert.
        let p = Pix::from_rgba(
            2,
            1,
            &[255, 0, 0, 255, 0, 255, 0, 255],
            FMT_RGBA32,
            (96.0, 72.0),
        )
        .unwrap();
        let r = flip_rotate(&p, 1).unwrap(); // 90° horaire : 1×2, rouge en haut
        assert_eq!((r.w, r.h, r.dpi), (1, 2, (72.0, 96.0)));
        assert_eq!(&r.data[..4], &[255, 0, 0, 255]);
        let m = flip_rotate(&p, 8).unwrap(); // miroir horizontal
        assert_eq!(&m.data[..4], &[0, 255, 0, 255]);
        let s = scale(&p, 4, 2, 0).unwrap();
        assert_eq!((s.w, s.h), (4, 2));
        assert_eq!(&s.data[..4], &[255, 0, 0, 255]);
        assert_eq!(&s.data[12..16], &[0, 255, 0, 255]);
        let c = clip(
            &s,
            WicRect {
                x: 2,
                y: 1,
                w: 2,
                h: 1,
            },
        )
        .unwrap();
        assert_eq!(&c.data[..4], &[0, 255, 0, 255]);
        assert!(clip(
            &s,
            WicRect {
                x: 3,
                y: 0,
                w: 2,
                h: 1
            }
        )
        .is_none());
    }

    #[test]
    fn premultiplication_aller_retour() {
        let p = Pix::from_rgba(1, 1, &[200, 100, 50, 128], FMT_PBGRA32, (96.0, 96.0)).unwrap();
        assert_eq!(&p.data[..4], &[25, 50, 100, 128]);
        let back = p.to_rgba();
        assert!((back[0] as i32 - 200).abs() <= 2 && back[3] == 128);
    }

    #[test]
    fn fabrique_par_cocreateinstance() {
        assert!(creator(&CLSID_WIC_IMAGING_FACTORY).is_some());
        assert!(creator(&CLSID_WIC_IMAGING_FACTORY1).is_some());
        assert!(creator(&IID_IMAGING_FACTORY).is_none());
    }

    /// #1503 : Paint.NET lit bits par pixel, canaux et transparence d'un
    /// format par CreateComponentInfo(GUID du format).
    #[test]
    fn informations_de_format_de_pixel() {
        unsafe {
            let mut f = std::ptr::null_mut();
            assert_eq!(create_factory(&IID_IMAGING_FACTORY, &mut f), S_OK);
            type ById = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            type GetU32 = unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32;
            type GetGuid = unsafe extern "win64" fn(*mut c_void, *mut Guid) -> i32;
            type Mask = unsafe extern "win64" fn(*mut c_void, u32, u32, *mut u8, *mut u32) -> i32;
            let mut info = std::ptr::null_mut();
            assert_eq!(method::<ById>(f, 6)(f, &FMT_PBGRA32, &mut info), S_OK);
            let mut info2 = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(info, 0)(info, &IID_PIXEL_FORMAT_INFO2, &mut info2),
                S_OK
            );
            release_obj(info);
            let mut v = 0u32;
            assert_eq!(method::<GetU32>(info2, 3)(info2, &mut v), S_OK);
            assert_eq!(v, WIC_PIXEL_FORMAT);
            let mut g = Guid::default();
            assert_eq!(method::<GetGuid>(info2, 11)(info2, &mut g), S_OK);
            assert_eq!(g, FMT_PBGRA32);
            assert_eq!(method::<GetU32>(info2, 13)(info2, &mut v), S_OK);
            assert_eq!(v, 32);
            assert_eq!(method::<GetU32>(info2, 14)(info2, &mut v), S_OK);
            assert_eq!(v, 4);
            let mut len = 0u32;
            assert_eq!(
                method::<Mask>(info2, 15)(info2, 3, 0, std::ptr::null_mut(), &mut len),
                S_OK
            );
            assert_eq!(len, 4);
            let mut mask = [0u8; 4];
            assert_eq!(
                method::<Mask>(info2, 15)(info2, 3, 4, mask.as_mut_ptr(), &mut len),
                S_OK
            );
            assert_eq!(mask, [0, 0, 0, 0xff]);
            assert_eq!(
                method::<Mask>(info2, 15)(info2, 4, 4, mask.as_mut_ptr(), &mut len),
                E_INVALIDARG
            );
            assert_eq!(method::<GetU32>(info2, 16)(info2, &mut v), S_OK);
            assert_eq!(v, 1);
            assert_eq!(method::<GetU32>(info2, 17)(info2, &mut v), S_OK);
            assert_eq!(v, NUM_UNSIGNED);
            let mut ctx = std::ptr::null_mut();
            assert_eq!(method::<Create>(info2, 12)(info2, &mut ctx), S_OK);
            assert!(!ctx.is_null());
            release_obj(ctx);
            release_obj(info2);
            // Champs inégaux et format flottant sans alpha.
            let mut i565 = std::ptr::null_mut();
            assert_eq!(
                method::<ById>(f, 6)(f, &pixel_format(0x0a), &mut i565),
                S_OK
            );
            let mut m = [0u8; 2];
            assert_eq!(
                method::<Mask>(i565, 15)(i565, 1, 2, m.as_mut_ptr(), &mut len),
                S_OK
            );
            assert_eq!(u16::from_le_bytes(m), 0x07e0);
            release_obj(i565);
            let mut fl = std::ptr::null_mut();
            assert_eq!(
                method::<ById>(f, 6)(f, &wic_pixels::FMT_RGB_FLOAT128, &mut fl),
                S_OK
            );
            assert_eq!(method::<GetU32>(fl, 16)(fl, &mut v), S_OK);
            assert_eq!(v, 0);
            assert_eq!(method::<GetU32>(fl, 17)(fl, &mut v), S_OK);
            assert_eq!(v, NUM_FLOAT);
            release_obj(fl);
            release_obj(f);
        }
    }
}
