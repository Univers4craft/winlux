//! Encodeur WIC PNG (#1868) : IWICImagingFactory::CreateEncoder,
//! IWICBitmapEncoder et IWICBitmapFrameEncode.
//!
//! Paint.NET copie une sélection dans le Presse-papiers au format PNG par
//! cet encodeur : Initialize sur un IStream, CreateNewFrame, SetSize,
//! SetPixelFormat, WritePixels ou WriteSource, Commit de l'image puis de
//! l'encodeur. L'ordre des méthodes suit `wincodec.h` (mingw-w64, domaine
//! public). Le PNG est produit par la caisse `png` en RVBA 8 bits.

use super::{
    add_ref, clip, method, qi_list, read_source, release, Pix, WicRect, CONTAINER_PNG, FMT_BGR24,
    FMT_BGRA32, FMT_GRAY8, FMT_RGB24, FMT_RGBA32, WINCODEC_ERR_COMPONENTNOTFOUND,
    WINCODEC_ERR_NOTINITIALIZED, WINCODEC_ERR_UNSUPPORTEDOPERATION,
};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::ole32::Guid;
use std::ffi::c_void;
use std::sync::Mutex;

/// WINCODEC_ERR_WRONGSTATE (winerror.h).
const WINCODEC_ERR_WRONGSTATE: i32 = 0x8898_2F04u32 as i32;
/// WINCODEC_ERR_CODECTOOMANYSCANLINES (winerror.h).
const WINCODEC_ERR_TOOMANYSCANLINES: i32 = 0x8898_2F46u32 as i32;

/// IWICBitmapEncoder {00000103-A8F2-4877-BA0A-FD2B6645FB94}.
const IID_BITMAP_ENCODER: Guid = guid(
    0x0000_0103,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
/// IWICBitmapFrameEncode {00000105-A8F2-4877-BA0A-FD2B6645FB94}.
const IID_FRAME_ENCODE: Guid = guid(
    0x0000_0105,
    0xa8f2,
    0x4877,
    [0xba, 0x0a, 0xfd, 0x2b, 0x66, 0x45, 0xfb, 0x94],
);
/// IPropertyBag2 {22F55882-280B-11D0-A8A9-00A0C90C2004} (ocidl.h).
const IID_PROPERTY_BAG2: Guid = guid(
    0x22f5_5882,
    0x280b,
    0x11d0,
    [0xa8, 0xa9, 0x00, 0xa0, 0xc9, 0x0c, 0x20, 0x04],
);

#[derive(Default)]
struct EncoderState {
    /// IStream de destination, tenu jusqu'à la fin de l'encodeur.
    stream: Option<usize>,
    /// PNG de l'image validée par son Commit.
    png: Option<Vec<u8>>,
    frame_created: bool,
    committed: bool,
}

struct Encoder {
    state: Mutex<EncoderState>,
}

impl Drop for Encoder {
    fn drop(&mut self) {
        if let Some(s) = crate::dlls::lock(&self.state).stream.take() {
            unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(s as _, 2)(s as _) };
        }
    }
}

type EncoderObj = Com<Encoder>;

#[derive(Default)]
struct FrameState {
    initialized: bool,
    w: u32,
    h: u32,
    fmt: Option<Guid>,
    dpi: (f64, f64),
    /// Lignes reçues, en RVBA 8 bits non prémultiplié.
    rgba: Vec<u8>,
    rows: u32,
    committed: bool,
}

struct Frame {
    encoder: *mut EncoderObj,
    state: Mutex<FrameState>,
}
unsafe impl Send for Frame {}
unsafe impl Sync for Frame {}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe { Com::release(self.encoder) };
    }
}

type FrameObj = Com<Frame>;

/// Formats que l'encodeur garde tels quels ; les autres deviennent
/// 32bppBGRA, comme l'encodeur PNG de WIC le fait pour un format qu'il
/// n'écrit pas.
fn native(fmt: &Guid) -> bool {
    [FMT_BGRA32, FMT_RGBA32, FMT_BGR24, FMT_RGB24, FMT_GRAY8].contains(fmt)
}

/// CreateEncoder : seul le conteneur PNG est pris en charge.
pub(super) unsafe extern "win64" fn create_encoder(
    _this: *mut c_void,
    container: *const Guid,
    _vendor: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if container.is_null() {
        return E_INVALIDARG;
    }
    if *container != CONTAINER_PNG {
        crate::log_warn!(
            "windowscodecs",
            "CreateEncoder : conteneur {:?} non pris en charge (PNG seulement)",
            *container
        );
        return WINCODEC_ERR_COMPONENTNOTFOUND;
    }
    put(
        out,
        Com::create(
            ENCODER_VT.ptr(),
            Encoder {
                state: Mutex::new(EncoderState::default()),
            },
        ),
    )
}

unsafe extern "win64" fn encoder_qi(
    this: *mut EncoderObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_BITMAP_ENCODER])
}

unsafe extern "win64" fn encoder_initialize(
    this: *mut EncoderObj,
    stream: *mut c_void,
    _cache: u32,
) -> i32 {
    if stream.is_null() {
        return E_INVALIDARG;
    }
    let mut s = crate::dlls::lock(&Com::get(this).state);
    if s.stream.is_some() {
        return WINCODEC_ERR_WRONGSTATE;
    }
    method::<unsafe extern "win64" fn(*mut c_void) -> u32>(stream, 1)(stream);
    s.stream = Some(stream as usize);
    S_OK
}

unsafe extern "win64" fn encoder_container(_this: *mut EncoderObj, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = CONTAINER_PNG;
    S_OK
}

unsafe extern "win64" fn encoder_info(_this: *mut EncoderObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // Des informations d'encodeur (IWICBitmapEncoderInfo), pas celles du
    // décodeur : Paint.NET les exige pour copier (#1868).
    super::encoder_info_for_container(&CONTAINER_PNG, out)
}

/// SetColorContexts, SetPalette : sans effet sur un PNG RVBA.
unsafe extern "win64" fn encoder_ignored2(_this: *mut c_void, _a: usize, _b: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn encoder_ignored1(_this: *mut c_void, _a: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn encoder_unsupported1(_this: *mut c_void, _a: usize) -> i32 {
    WINCODEC_ERR_UNSUPPORTEDOPERATION
}

unsafe extern "win64" fn encoder_new_frame(
    this: *mut EncoderObj,
    frame: *mut *mut c_void,
    bag: *mut *mut c_void,
) -> i32 {
    if frame.is_null() {
        return E_INVALIDARG;
    }
    *frame = std::ptr::null_mut();
    if !bag.is_null() {
        *bag = std::ptr::null_mut();
    }
    {
        let mut s = crate::dlls::lock(&Com::get(this).state);
        if s.stream.is_none() {
            return WINCODEC_ERR_NOTINITIALIZED;
        }
        // Un PNG n'a qu'une image.
        if s.frame_created || s.committed {
            return WINCODEC_ERR_WRONGSTATE;
        }
        s.frame_created = true;
    }
    Com::add_ref(this);
    let f = Com::create(
        FRAME_VT.ptr(),
        Frame {
            encoder: this,
            state: Mutex::new(FrameState {
                dpi: (96.0, 96.0),
                ..FrameState::default()
            }),
        },
    );
    if !bag.is_null() {
        put(bag, Com::create(BAG_VT.ptr(), PropertyBag));
    }
    put(frame, f)
}

unsafe extern "win64" fn encoder_commit(this: *mut EncoderObj) -> i32 {
    let mut s = crate::dlls::lock(&Com::get(this).state);
    if s.committed {
        return WINCODEC_ERR_WRONGSTATE;
    }
    let (Some(stream), Some(png)) = (s.stream, s.png.as_ref()) else {
        return WINCODEC_ERR_WRONGSTATE;
    };
    let hr = write_stream(stream as _, png);
    if hr < 0 {
        return hr;
    }
    s.committed = true;
    S_OK
}

/// ISequentialStream::Write, par morceaux, jusqu'au bout.
unsafe fn write_stream(stream: *mut c_void, mut data: &[u8]) -> i32 {
    type Write = unsafe extern "win64" fn(*mut c_void, *const u8, u32, *mut u32) -> i32;
    while !data.is_empty() {
        let n = data.len().min(1 << 30) as u32;
        let mut written = 0u32;
        let hr = method::<Write>(stream, 4)(stream, data.as_ptr(), n, &mut written);
        if hr < 0 {
            return hr;
        }
        if written == 0 {
            return 0x8003_0070u32 as i32; // STG_E_MEDIUMFULL
        }
        data = &data[(written as usize).min(data.len())..];
    }
    S_OK
}

unsafe extern "win64" fn encoder_query_writer(
    _this: *mut EncoderObj,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    WINCODEC_ERR_UNSUPPORTEDOPERATION
}

static ENCODER_VT: VTable<13> = VTable([
    encoder_qi as *const (),
    add_ref::<Encoder> as *const (),
    release::<Encoder> as *const (),
    encoder_initialize as *const (),
    encoder_container as *const (),
    encoder_info as *const (),
    encoder_ignored2 as *const (),     // SetColorContexts
    encoder_ignored1 as *const (),     // SetPalette
    encoder_unsupported1 as *const (), // SetThumbnail
    encoder_unsupported1 as *const (), // SetPreview
    encoder_new_frame as *const (),
    encoder_commit as *const (),
    encoder_query_writer as *const (),
]);

// --- Image ----------------------------------------------------------------------

unsafe extern "win64" fn frame_qi(
    this: *mut FrameObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FRAME_ENCODE])
}

unsafe fn frame(this: *mut FrameObj) -> std::sync::MutexGuard<'static, FrameState> {
    crate::dlls::lock(&Com::get(this).state)
}

unsafe extern "win64" fn frame_initialize(this: *mut FrameObj, _bag: *mut c_void) -> i32 {
    let mut f = frame(this);
    if f.initialized {
        return WINCODEC_ERR_WRONGSTATE;
    }
    f.initialized = true;
    S_OK
}

unsafe extern "win64" fn frame_set_size(this: *mut FrameObj, w: u32, h: u32) -> i32 {
    let mut f = frame(this);
    if !f.initialized || f.rows > 0 {
        return WINCODEC_ERR_WRONGSTATE;
    }
    if w == 0 || h == 0 || crate::imaging::check_size(w, h).is_err() {
        return E_INVALIDARG;
    }
    (f.w, f.h) = (w, h);
    S_OK
}

unsafe extern "win64" fn frame_set_resolution(this: *mut FrameObj, x: f64, y: f64) -> i32 {
    frame(this).dpi = (x, y);
    S_OK
}

unsafe extern "win64" fn frame_set_format(this: *mut FrameObj, fmt: *mut Guid) -> i32 {
    if fmt.is_null() {
        return E_INVALIDARG;
    }
    let mut f = frame(this);
    if !f.initialized || f.rows > 0 {
        return WINCODEC_ERR_WRONGSTATE;
    }
    if !native(&*fmt) {
        *fmt = FMT_BGRA32;
    }
    f.fmt = Some(*fmt);
    S_OK
}

/// Ajoute des lignes RVBA à l'image.
fn append_rows(f: &mut FrameState, rgba: &[u8], rows: u32) -> i32 {
    if f.rows + rows > f.h {
        return WINCODEC_ERR_TOOMANYSCANLINES;
    }
    f.rgba.extend_from_slice(rgba);
    f.rows += rows;
    S_OK
}

unsafe extern "win64" fn frame_write_pixels(
    this: *mut FrameObj,
    lines: u32,
    stride: u32,
    size: u32,
    buf: *const u8,
) -> i32 {
    let mut f = frame(this);
    let Some(fmt) = f.fmt else {
        return WINCODEC_ERR_WRONGSTATE;
    };
    if f.w == 0 {
        return WINCODEC_ERR_WRONGSTATE;
    }
    if lines == 0 {
        return S_OK;
    }
    if buf.is_null() {
        return E_INVALIDARG;
    }
    let Some(mut pix) = Pix::zeroed(f.w, lines, fmt) else {
        return E_INVALIDARG;
    };
    let row = (pix.data.len() / lines as usize).min(pix.stride as usize);
    let bpp = super::bytes_per_pixel(&fmt).unwrap_or(4) as usize;
    let used = f.w as usize * bpp;
    if (stride as usize) < used || (size as u64) < stride as u64 * (lines as u64 - 1) + used as u64
    {
        return E_INVALIDARG;
    }
    for y in 0..lines as usize {
        std::ptr::copy_nonoverlapping(
            buf.add(y * stride as usize),
            pix.data[y * row..].as_mut_ptr(),
            used,
        );
    }
    let rgba = pix.to_rgba();
    append_rows(&mut f, &rgba, lines)
}

unsafe extern "win64" fn frame_write_source(
    this: *mut FrameObj,
    src: *mut c_void,
    rect: *const WicRect,
) -> i32 {
    let mut pix = match read_source(src) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    if let Some(r) = rect.as_ref() {
        match clip(&pix, *r) {
            Some(p) => pix = p,
            None => return E_INVALIDARG,
        }
    }
    let mut f = frame(this);
    if !f.initialized {
        return WINCODEC_ERR_WRONGSTATE;
    }
    // Taille et format tirés de la source s'ils n'ont pas été donnés.
    if f.w == 0 {
        (f.w, f.h) = (pix.w, pix.h);
    }
    if f.fmt.is_none() {
        f.fmt = Some(if native(&pix.fmt) {
            pix.fmt
        } else {
            FMT_BGRA32
        });
    }
    if pix.w != f.w {
        return E_INVALIDARG;
    }
    let rgba = pix.to_rgba();
    append_rows(&mut f, &rgba, pix.h)
}

unsafe extern "win64" fn frame_commit(this: *mut FrameObj) -> i32 {
    let png = {
        let mut f = frame(this);
        if f.committed || f.w == 0 || f.rows < f.h {
            return WINCODEC_ERR_WRONGSTATE;
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, f.w, f.h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            // Résolution : pHYs en pixels par mètre.
            let ppm = |d: f64| (d.max(1.0) / 0.0254).round() as u32;
            enc.set_pixel_dims(Some(png::PixelDimensions {
                xppu: ppm(f.dpi.0),
                yppu: ppm(f.dpi.1),
                unit: png::Unit::Meter,
            }));
            let Ok(mut w) = enc.write_header() else {
                return E_INVALIDARG;
            };
            if w.write_image_data(&f.rgba).is_err() {
                return E_INVALIDARG;
            }
        }
        f.committed = true;
        f.rgba = Vec::new();
        out
    };
    let enc = Com::get(this).encoder;
    crate::dlls::lock(&Com::get(enc).state).png = Some(png);
    S_OK
}

static FRAME_VT: VTable<14> = VTable([
    frame_qi as *const (),
    add_ref::<Frame> as *const (),
    release::<Frame> as *const (),
    frame_initialize as *const (),
    frame_set_size as *const (),
    frame_set_resolution as *const (),
    frame_set_format as *const (),
    encoder_ignored2 as *const (),     // SetColorContexts
    encoder_ignored1 as *const (),     // SetPalette
    encoder_unsupported1 as *const (), // SetThumbnail
    frame_write_pixels as *const (),
    frame_write_source as *const (),
    frame_commit as *const (),
    encoder_query_writer as *const (), // GetMetadataQueryWriter
]);

// --- Options d'encodage ------------------------------------------------------------

/// IPropertyBag2 des options : accepte tout, n'en retient rien (PNG RVBA
/// sans entrelacement).
struct PropertyBag;

unsafe extern "win64" fn bag_qi(
    this: *mut Com<PropertyBag>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_PROPERTY_BAG2])
}
unsafe extern "win64" fn bag_read(
    _this: *mut c_void,
    _n: u32,
    _props: *const c_void,
    _log: *mut c_void,
    _values: *mut c_void,
    _errors: *mut i32,
) -> i32 {
    0x8000_4005u32 as i32 // E_FAIL : aucune option lisible
}
unsafe extern "win64" fn bag_write(
    _this: *mut c_void,
    _n: u32,
    _props: *const c_void,
    _values: *const c_void,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn bag_count(_this: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn bag_info(
    _this: *mut c_void,
    _first: u32,
    _n: u32,
    _props: *mut c_void,
    out: *mut u32,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_INVALIDARG
}
unsafe extern "win64" fn bag_load(
    _this: *mut c_void,
    _name: *const u16,
    _hint: u32,
    _obj: *mut c_void,
    _log: *mut c_void,
) -> i32 {
    WINCODEC_ERR_UNSUPPORTEDOPERATION
}

static BAG_VT: VTable<8> = VTable([
    bag_qi as *const (),
    add_ref::<PropertyBag> as *const (),
    release::<PropertyBag> as *const (),
    bag_read as *const (),
    bag_write as *const (),
    bag_count as *const (),
    bag_info as *const (),
    bag_load as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    /// IStream minimal qui garde ce qu'on lui écrit.
    mod flux {
        use super::*;
        pub struct Flux {
            pub data: Mutex<Vec<u8>>,
        }
        unsafe extern "win64" fn qi(_: *mut c_void, _: *const Guid, _: *mut *mut c_void) -> i32 {
            0x8000_4002u32 as i32
        }
        unsafe extern "win64" fn write(
            this: *mut Com<Flux>,
            p: *const u8,
            n: u32,
            w: *mut u32,
        ) -> i32 {
            crate::dlls::lock(&Com::get(this).data)
                .extend_from_slice(std::slice::from_raw_parts(p, n as usize));
            if !w.is_null() {
                *w = n;
            }
            S_OK
        }
        pub static VT: VTable<5> = VTable([
            qi as *const (),
            add_ref::<Flux> as *const (),
            release::<Flux> as *const (),
            qi as *const (), // Read, inutilisé
            write as *const (),
        ]);
    }

    /// #1868 : CreateEncoder(PNG), une image 2×1 en 32bppPBGRA par
    /// WritePixels (rendue en BGRA), Commit : le flux reçoit un PNG que le
    /// décodeur relit avec les mêmes couleurs.
    #[test]
    fn encodeur_png_par_write_pixels() {
        unsafe {
            let flux = Com::create(
                flux::VT.ptr(),
                flux::Flux {
                    data: Mutex::new(Vec::new()),
                },
            );
            let mut enc = std::ptr::null_mut();
            assert_eq!(
                create_encoder(
                    std::ptr::null_mut(),
                    &CONTAINER_PNG,
                    std::ptr::null(),
                    &mut enc
                ),
                S_OK
            );
            let enc = enc as *mut EncoderObj;
            assert_eq!(encoder_initialize(enc, flux.cast(), 2), S_OK);
            let (mut fr, mut bag) = (std::ptr::null_mut(), std::ptr::null_mut());
            assert_eq!(encoder_new_frame(enc, &mut fr, &mut bag), S_OK);
            assert!(!bag.is_null());
            assert_eq!(bag_write(bag, 0, std::ptr::null(), std::ptr::null()), S_OK);
            let fr = fr as *mut FrameObj;
            // Deuxième image refusée : un PNG n'en a qu'une.
            let mut autre = std::ptr::null_mut();
            assert_eq!(
                encoder_new_frame(enc, &mut autre, std::ptr::null_mut()),
                WINCODEC_ERR_WRONGSTATE
            );
            assert_eq!(frame_initialize(fr, bag), S_OK);
            assert_eq!(frame_set_size(fr, 2, 1), S_OK);
            let mut fmt = super::super::FMT_PBGRA32;
            assert_eq!(frame_set_format(fr, &mut fmt), S_OK);
            assert_eq!(fmt, FMT_BGRA32, "PBGRA ramené à BGRA");
            // Rouge opaque, puis bleu à moitié transparent.
            let px: [u8; 8] = [0, 0, 255, 255, 255, 0, 0, 128];
            assert_eq!(frame_write_pixels(fr, 1, 8, 8, px.as_ptr()), S_OK);
            assert_eq!(
                frame_write_pixels(fr, 1, 8, 8, px.as_ptr()),
                WINCODEC_ERR_TOOMANYSCANLINES
            );
            assert_eq!(frame_commit(fr), S_OK);
            assert_eq!(encoder_commit(enc), S_OK);
            let data = crate::dlls::lock(&Com::get(flux).data).clone();
            assert!(data.starts_with(b"\x89PNG\r\n\x1a\n"));
            let dec = png::Decoder::new(std::io::Cursor::new(data));
            let mut r = dec.read_info().unwrap();
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            let info = r.next_frame(&mut buf).unwrap();
            assert_eq!((info.width, info.height), (2, 1));
            assert_eq!(&buf[..8], &[255, 0, 0, 255, 0, 0, 255, 128]);
            Com::release(fr);
            Com::release(bag as *mut Com<PropertyBag>);
            Com::release(enc);
            Com::release(flux);
        }
    }

    /// Un autre conteneur que PNG est refusé proprement.
    #[test]
    fn encodeur_autre_conteneur_refuse() {
        unsafe {
            let mut enc = std::ptr::null_mut();
            assert_eq!(
                create_encoder(
                    std::ptr::null_mut(),
                    &super::super::CONTAINER_JPEG,
                    std::ptr::null(),
                    &mut enc
                ),
                WINCODEC_ERR_COMPONENTNOTFOUND
            );
            assert!(enc.is_null());
        }
    }

    /// Appel d'une méthode de vtable COM à un argument pointeur.
    unsafe fn call1<T>(obj: *mut c_void, slot: usize, a: *mut T) -> i32 {
        let vt = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void, *mut T) -> i32 =
            std::mem::transmute(*vt.add(slot));
        f(obj, a)
    }

    /// #1868 : GetEncoderInfo rend un IWICBitmapEncoderInfo (type
    /// WICEncoder, CLSID de l'encodeur PNG) dont CreateInstance crée un
    /// encodeur ; CreateComponentInfo(CLSID) rend le même. Paint.NET
    /// l'exige avant de copier dans le presse-papiers.
    #[test]
    fn informations_encodeur_png() {
        unsafe {
            let mut enc = std::ptr::null_mut();
            assert_eq!(
                create_encoder(
                    std::ptr::null_mut(),
                    &CONTAINER_PNG,
                    std::ptr::null(),
                    &mut enc
                ),
                S_OK
            );
            let mut info: *mut c_void = std::ptr::null_mut();
            assert_eq!(encoder_info(enc.cast(), &mut info), S_OK);
            let iid = super::super::IID_BITMAP_ENCODER_INFO;
            let mut ei: *mut c_void = std::ptr::null_mut();
            let vt = *(info as *const *const usize);
            let qi: unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
                std::mem::transmute(*vt);
            assert_eq!(qi(info, &iid, &mut ei), S_OK, "IWICBitmapEncoderInfo");
            let decoder_iid = super::super::IID_BITMAP_DECODER_INFO;
            let mut di: *mut c_void = std::ptr::null_mut();
            assert_ne!(qi(info, &decoder_iid, &mut di), S_OK);
            let mut kind = 0u32;
            assert_eq!(call1(ei, 3, &mut kind), S_OK);
            assert_eq!(kind, 2, "WICEncoder");
            let mut clsid = Guid::default();
            assert_eq!(call1(ei, 4, &mut clsid), S_OK);
            assert_eq!(clsid, super::super::ENCODER_CODECS[0].clsid);
            let mut cont = Guid::default();
            assert_eq!(call1(ei, 11, &mut cont), S_OK);
            assert_eq!(cont, CONTAINER_PNG);
            let mut autre: *mut c_void = std::ptr::null_mut();
            assert_eq!(call1(ei, 23, &mut autre), S_OK, "CreateInstance");
            assert!(!autre.is_null());
            let mut cont2 = Guid::default();
            assert_eq!(call1(autre, 4, &mut cont2), S_OK);
            assert_eq!(cont2, CONTAINER_PNG);
            let mut ci: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                super::super::create_component_info(std::ptr::null_mut(), &clsid, &mut ci),
                S_OK
            );
            let mut kind2 = 0u32;
            assert_eq!(call1(ci, 3, &mut kind2), S_OK);
            assert_eq!(kind2, 2);
        }
    }
}
