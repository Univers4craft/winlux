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

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
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
const CONTAINER_DDS: Guid = guid(
    0x9967cb95,
    0x2e85,
    0x4ac8,
    [0x8c, 0xa2, 0x83, 0xd7, 0xcc, 0xd4, 0x25, 0xc9],
);

/// Octets par pixel des formats que Winlux sait lire et produire.
fn bytes_per_pixel(fmt: &Guid) -> Option<u32> {
    Some(match *fmt {
        FMT_GRAY8 => 1,
        FMT_BGR24 | FMT_RGB24 => 3,
        FMT_BGR32 | FMT_BGRA32 | FMT_PBGRA32 | FMT_RGBA32 | FMT_PRGBA32 => 4,
        _ => return None,
    })
}

// --- Pixels -------------------------------------------------------------------

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
        let stride = (w * bpp + 3) & !3;
        Some(Pix {
            w,
            h,
            fmt,
            stride,
            data: vec![0; stride as usize * h as usize],
            dpi: (96.0, 96.0),
        })
    }

    /// Pixels en RGBA 8 bits non prémultiplié, ligne à ligne sans marge.
    fn to_rgba(&self) -> Vec<u8> {
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
        Pix::from_rgba(self.w, self.h, &self.to_rgba(), fmt, self.dpi)
    }

    fn from_raster(r: &crate::imaging::Raster) -> Pix {
        Pix::from_rgba(r.w, r.h, &r.px, FMT_BGRA32, (96.0, 96.0)).expect("format connu")
    }
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
    if r.x < 0
        || r.y < 0
        || r.w < 0
        || r.h < 0
        || r.x + r.w > pix.w as i32
        || r.y + r.h > pix.h as i32
    {
        return E_INVALIDARG;
    }
    let bpp = bytes_per_pixel(&pix.fmt).unwrap_or(4) as usize;
    let row = r.w as usize * bpp;
    if r.h == 0 || r.w == 0 {
        return S_OK;
    }
    if (stride as usize) < row || (size as usize) < stride as usize * (r.h as usize - 1) + row {
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

unsafe extern "win64" fn factory_qi(
    this: *mut Com<Factory>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_IMAGING_FACTORY, IID_IMAGING_FACTORY2],
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
        None => WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT,
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
    let Some(mut pix) = Pix::zeroed(w, h, *fmt) else {
        return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    };
    let row = (w * bytes_per_pixel(&*fmt).unwrap_or(4)) as usize;
    if h > 0
        && ((stride as usize) < row || (size as usize) < stride as usize * (h as usize - 1) + row)
    {
        return E_INVALIDARG;
    }
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

unsafe extern "win64" fn not_implemented() -> i32 {
    E_NOTIMPL
}

static FACTORY_VT: VTable<29> = VTable([
    factory_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    create_decoder_from_filename as *const (),
    create_decoder_from_stream as *const (),
    create_decoder_from_file_handle as *const (),
    not_implemented as *const (), // CreateComponentInfo
    create_decoder as *const (),
    not_implemented as *const (), // CreateEncoder
    not_implemented as *const (), // CreatePalette
    create_format_converter as *const (),
    create_bitmap_scaler as *const (),
    create_bitmap_clipper as *const (),
    create_bitmap_flip_rotator as *const (),
    create_stream as *const (),
    not_implemented as *const (), // CreateColorContext
    not_implemented as *const (), // CreateColorTransformer
    create_bitmap as *const (),
    create_bitmap_from_source as *const (),
    create_bitmap_from_source_rect as *const (),
    create_bitmap_from_memory as *const (),
    not_implemented as *const (), // CreateBitmapFromHBITMAP
    not_implemented as *const (), // CreateBitmapFromHICON
    not_implemented as *const (), // CreateComponentEnumerator
    not_implemented as *const (), // CreateFastMetadataEncoderFromDecoder
    not_implemented as *const (), // CreateFastMetadataEncoderFromFrameDecode
    not_implemented as *const (), // CreateQueryWriter
    not_implemented as *const (), // CreateQueryWriterFromReader
    not_implemented as *const (), // CreateImageEncoder (IWICImagingFactory2)
]);

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
    let pix = match &*crate::dlls::lock(Com::get(this)) {
        Some(d) => match d.frames.get(index as usize) {
            Some(p) => p.clone(),
            None => return WINCODEC_ERR_FRAMEMISSING,
        },
        None => return WINCODEC_ERR_NOTINITIALIZED,
    };
    new_source(Kind::Frame, Some(pix), out)
}

static DECODER_VT: VTable<14> = VTable([
    decoder_qi as *const (),
    add_ref::<Mutex<Option<Decoder>>> as *const (),
    release::<Mutex<Option<Decoder>>> as *const (),
    decoder_query_capability as *const (),
    decoder_initialize as *const (),
    decoder_container as *const (),
    not_implemented as *const (), // GetDecoderInfo
    palette_unavailable as *const (),
    unsupported_operation as *const (), // GetMetadataQueryReader
    unsupported_operation as *const (), // GetPreview
    no_color_contexts as *const (),
    no_thumbnail as *const (),
    decoder_frame_count as *const (),
    decoder_frame as *const (),
]);

// --- Sources : image décodée, bitmap, convertisseur, transformations ----------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Frame,
    Bitmap,
    Converter,
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
    if r.x < 0
        || r.y < 0
        || r.w <= 0
        || r.h <= 0
        || r.x + r.w > p.w as i32
        || r.y + r.h > p.h as i32
    {
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
    if r.x < 0
        || r.y < 0
        || r.w <= 0
        || r.h <= 0
        || r.x + r.w > pix.w as i32
        || r.y + r.h > pix.h as i32
    {
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
    not_implemented as *const (), // CopyTo
    stream_commit as *const (),
    not_implemented as *const (),  // Revert
    invalid_function as *const (), // LockRegion
    invalid_function as *const (), // UnlockRegion
    stream_stat as *const (),
    not_implemented as *const (), // Clone
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
    crate::exports! {
        "WICCreateImagingFactory_Proxy" => fn wic_create_imaging_factory_proxy,
        "WICConvertBitmapSource" => fn wic_convert_bitmap_source,
        "DllGetClassObject" => fn dll_get_class_object,
    }
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
}
