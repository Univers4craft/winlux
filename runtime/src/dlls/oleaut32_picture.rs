//! Images et polices OLE d'oleaut32 (lot C du relevé, #1511) :
//! OleCreatePictureIndirect, OleLoadPicture, OleLoadPicturePath
//! (IPicture, IPictureDisp) et OleCreateFontIndirect (IFont, IFontDisp).
//!
//! Sources : documentation Microsoft Learn d'IPicture, IFont, PICTDESC,
//! FONTDESC et des DISPID_PICT_* / DISPID_FONT_* ; ordre des méthodes
//! d'ocidl.h et olectl.h de MinGW-w64 (domaine public). Noms et ordinaux
//! (418, 419, 420, 424) : bibliothèque d'importation OleAut32.Lib du SDK
//! Windows public.
//!
//! Les images sont décodées par le module d'images de Winlux (BMP, JPEG,
//! GIF, PNG ; ICO devient une icône). Les métafichiers sont gardés mais
//! pas dessinés. Les tailles sont en HIMETRIC (0,01 mm) à 96 points par
//! pouce. Réservé aux programmes 64 bits.

use super::comobj::{guid, Guid, VTable};
use super::ole32_lot_c::{read_guid, release, slot, stream_write, IID_IUNKNOWN};
use super::oleaut32::{sys_alloc_string_len, variant_change_type, variant_clear, variant_init};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const DISP_E_MEMBERNOTFOUND: i32 = 0x8002_0003u32 as i32;
const DISP_E_UNKNOWNNAME: i32 = 0x8002_0006u32 as i32;
const DISP_E_BADPARAMCOUNT: i32 = 0x8002_000Eu32 as i32;
/// CTL_E_INVALIDPICTURE : données qui ne sont pas une image connue.
const CTL_E_INVALIDPICTURE: i32 = 0x800A_01E1u32 as i32;
/// CTL_E_PATHFILEACCESSERROR : fichier illisible.
const CTL_E_PATHFILEACCESSERROR: i32 = 0x800A_004Bu32 as i32;

const IID_IDISPATCH: Guid = guid(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPICTURE: Guid = guid(
    0x7BF8_0980,
    0xBF32,
    0x101A,
    [0x8B, 0xBB, 0x00, 0xAA, 0x00, 0x30, 0x0C, 0xAB],
);
const IID_IPICTURE_DISP: Guid = guid(
    0x7BF8_0981,
    0xBF32,
    0x101A,
    [0x8B, 0xBB, 0x00, 0xAA, 0x00, 0x30, 0x0C, 0xAB],
);
const IID_IFONT: Guid = guid(
    0xBEF6_E002,
    0xA874,
    0x101A,
    [0x8B, 0xBA, 0x00, 0xAA, 0x00, 0x30, 0x0C, 0xAB],
);
const IID_IFONT_DISP: Guid = guid(
    0xBEF6_E003,
    0xA874,
    0x101A,
    [0x8B, 0xBA, 0x00, 0xAA, 0x00, 0x30, 0x0C, 0xAB],
);

const PICTYPE_UNINITIALIZED: i16 = -1;
const PICTYPE_NONE: i16 = 0;
const PICTYPE_BITMAP: i16 = 1;
const PICTYPE_METAFILE: i16 = 2;
const PICTYPE_ICON: i16 = 3;
const PICTYPE_ENHMETAFILE: i16 = 4;
const PICTURE_TRANSPARENT: u32 = 2;

const VT_I2: u16 = 2;
const VT_I4: u16 = 3;
const VT_CY: u16 = 6;
const VT_BSTR: u16 = 8;
const VT_BOOL: u16 = 11;
const DISPATCH_PROPERTYGET: u16 = 2;
const DISPATCH_PROPERTYPUT: u16 = 4;
const VARIANT_SIZE: usize = 24;

/// Pixels → HIMETRIC à 96 points par pouce.
fn himetric(px: i32) -> i32 {
    ((i64::from(px) * 2540 + 48) / 96) as i32
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------- IDispatch minimal partagé ----------

/// Propriétés exposées par IDispatch : (nom, DISPID).
type Names = &'static [(&'static str, i32)];

unsafe fn get_ids_of_names(
    table: Names,
    names: *const *const u16,
    count: u32,
    ids: *mut i32,
) -> i32 {
    if names.is_null() || ids.is_null() {
        return E_INVALIDARG;
    }
    let mut hr = S_OK;
    for i in 0..count as usize {
        let n = super::oleaut32_typelib::wide(*names.add(i));
        *ids.add(i) = match table.iter().find(|(k, _)| k.eq_ignore_ascii_case(&n)) {
            Some(&(_, id)) if i == 0 => id,
            _ => {
                hr = DISP_E_UNKNOWNNAME;
                -1
            }
        };
    }
    hr
}

unsafe fn put_variant_i(out: *mut u8, vt: u16, value: i64) {
    if out.is_null() {
        return;
    }
    variant_clear(out.cast());
    std::ptr::write_unaligned(out as *mut u16, vt);
    match vt {
        VT_I2 | VT_BOOL => std::ptr::write_unaligned(out.add(8) as *mut i16, value as i16),
        VT_I4 => std::ptr::write_unaligned(out.add(8) as *mut i32, value as i32),
        _ => std::ptr::write_unaligned(out.add(8) as *mut i64, value),
    }
}

/// Argument d'un PROPERTYPUT converti en `vt` ; rend le VARIANT converti.
unsafe fn put_argument(
    params: *const u8,
    vt: u16,
) -> Result<crate::dlls::comobj::Aligned<VARIANT_SIZE>, i32> {
    if params.is_null() {
        return Err(E_INVALIDARG);
    }
    let args = std::ptr::read_unaligned(params as *const *const u8);
    let count = std::ptr::read_unaligned(params.add(16) as *const u32);
    if count < 1 || args.is_null() {
        return Err(DISP_E_BADPARAMCOUNT);
    }
    let mut v = crate::dlls::comobj::Aligned::<VARIANT_SIZE>::default();
    variant_init(v.as_mut_ptr().cast());
    let hr = variant_change_type(v.as_mut_ptr().cast(), args.cast(), 0, vt);
    if hr < 0 {
        return Err(hr);
    }
    Ok(v)
}

// ---------- IPicture ----------

struct PictureState {
    kind: i16,
    handle: usize,
    hpal: usize,
    /// Taille en HIMETRIC.
    width: i32,
    height: i32,
    own: bool,
    keep_original: bool,
    /// Octets d'origine (OleLoadPicture), pour SaveAsFile.
    original: Option<Vec<u8>>,
    cur_dc: usize,
}

#[repr(C)]
struct Picture {
    vtbl: *const (),
    disp: *const (),
    refs: AtomicU32,
    state: Mutex<PictureState>,
}

static PICTURE_VT: VTable<17> = VTable([
    pic_qi as *const (),
    pic_add_ref as *const (),
    pic_release as *const (),
    pic_get_handle as *const (),
    pic_get_hpal as *const (),
    pic_get_type as *const (),
    pic_get_width as *const (),
    pic_get_height as *const (),
    pic_render as *const (),
    pic_set_hpal as *const (),
    pic_get_cur_dc as *const (),
    pic_select_picture as *const (),
    pic_get_keep_original_format as *const (),
    pic_put_keep_original_format as *const (),
    pic_picture_changed as *const (),
    pic_save_as_file as *const (),
    pic_get_attributes as *const (),
]);

static PICTURE_DISP_VT: VTable<7> = VTable([
    picd_qi as *const (),
    picd_add_ref as *const (),
    picd_release as *const (),
    disp_type_info_count as *const (),
    disp_type_info as *const (),
    picd_ids_of_names as *const (),
    picd_invoke as *const (),
]);

const PICTURE_NAMES: Names = &[
    ("Handle", 0),
    ("hPal", 2),
    ("Type", 3),
    ("Width", 4),
    ("Height", 5),
];

unsafe fn pic(p: usize) -> &'static Picture {
    &*(p as *const Picture)
}

fn new_picture(state: PictureState) -> usize {
    Box::into_raw(Box::new(Picture {
        vtbl: &PICTURE_VT as *const _ as *const (),
        disp: &PICTURE_DISP_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        state: Mutex::new(state),
    })) as usize
}

/// Remet l'objet sous l'interface demandée et lâche la référence initiale.
unsafe fn hand_out(obj: usize, iid: *const Guid, out: *mut usize) -> i32 {
    let hr = super::ole32_lot_c::query(obj, iid, out);
    release(obj);
    hr
}

unsafe extern "win64" fn pic_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(g) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    let target = if g == IID_IUNKNOWN || g == IID_IPICTURE {
        p
    } else if g == IID_IDISPATCH || g == IID_IPICTURE_DISP {
        p + 8
    } else {
        return E_NOINTERFACE;
    };
    pic_add_ref(p);
    *out = target;
    S_OK
}

unsafe extern "win64" fn pic_add_ref(p: usize) -> u32 {
    pic(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn pic_release(p: usize) -> u32 {
    let left = pic(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut Picture);
        let st = b.state.into_inner().unwrap_or_else(|e| e.into_inner());
        if st.own && st.handle != 0 {
            match st.kind {
                PICTYPE_BITMAP => {
                    super::gdi32::delete_object(st.handle);
                }
                PICTYPE_ICON => {
                    super::icons::destroy_icon(st.handle);
                }
                _ => {}
            }
        }
    }
    left
}

unsafe extern "win64" fn pic_get_handle(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).handle as u32;
    S_OK
}

unsafe extern "win64" fn pic_get_hpal(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).hpal as u32;
    S_OK
}

unsafe extern "win64" fn pic_get_type(p: usize, out: *mut i16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).kind;
    S_OK
}

unsafe extern "win64" fn pic_get_width(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).width;
    S_OK
}

unsafe extern "win64" fn pic_get_height(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).height;
    S_OK
}

/// Render : la zone source est en HIMETRIC, mesurée depuis le bas de
/// l'image (cySrc négatif pour une image à l'endroit).
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn pic_render(
    p: usize,
    hdc: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    x_src: i32,
    y_src: i32,
    cx_src: i32,
    cy_src: i32,
    _bounds: usize,
) -> i32 {
    if hdc == 0 {
        return E_INVALIDARG;
    }
    let (kind, handle, w_hm, h_hm) = {
        let st = lock(&pic(p).state);
        (st.kind, st.handle, st.width, st.height)
    };
    match kind {
        PICTYPE_BITMAP if handle != 0 => {
            let (w, h) = bitmap_pixels(handle).unwrap_or((0, 0));
            if w == 0 || h == 0 || w_hm == 0 || h_hm == 0 {
                return E_FAIL;
            }
            let px = |v: i32, total_px: i32, total_hm: i32| {
                (i64::from(v) * i64::from(total_px) / i64::from(total_hm)) as i32
            };
            let sx = px(x_src, w, w_hm);
            let sw = px(cx_src, w, w_hm);
            let bottom = px(y_src, h, h_hm);
            let sh = px(cy_src, h, h_hm);
            // Y depuis le bas : haut de la zone = hauteur - y_src.
            let (sy, sh) = if sh < 0 {
                (h - bottom, -sh)
            } else {
                (h - bottom - sh, sh)
            };
            let mem = super::gdi32::create_compatible_dc_from(hdc);
            let old = super::gdi32::select_object(mem, handle);
            super::gdi32::stretch_blt(hdc, x, y, cx, cy, mem, sx, sy, sw, sh, 0x00CC_0020);
            super::gdi32::select_object(mem, old);
            super::gdi32::delete_dc(mem);
            S_OK
        }
        PICTYPE_ICON if handle != 0 => {
            super::icons::draw_icon_ex(hdc, x, y, handle, cx, cy, 0, 0, 3);
            S_OK
        }
        PICTYPE_NONE | PICTYPE_UNINITIALIZED => S_OK,
        _ => E_NOTIMPL,
    }
}

unsafe extern "win64" fn pic_set_hpal(p: usize, hpal: u32) -> i32 {
    lock(&pic(p).state).hpal = hpal as usize;
    S_OK
}

unsafe extern "win64" fn pic_get_cur_dc(p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).cur_dc;
    S_OK
}

unsafe extern "win64" fn pic_select_picture(
    p: usize,
    hdc: usize,
    old_dc: *mut usize,
    old_bitmap: *mut u32,
) -> i32 {
    let mut st = lock(&pic(p).state);
    if st.kind != PICTYPE_BITMAP {
        return E_FAIL;
    }
    let previous = super::gdi32::select_object(hdc, st.handle);
    if !old_dc.is_null() {
        *old_dc = st.cur_dc;
    }
    if !old_bitmap.is_null() {
        *old_bitmap = previous as u32;
    }
    st.cur_dc = hdc;
    S_OK
}

unsafe extern "win64" fn pic_get_keep_original_format(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&pic(p).state).keep_original as i32;
    S_OK
}

unsafe extern "win64" fn pic_put_keep_original_format(p: usize, keep: i32) -> i32 {
    lock(&pic(p).state).keep_original = keep != 0;
    S_OK
}

extern "win64" fn pic_picture_changed(_p: usize) -> i32 {
    S_OK
}

/// SaveAsFile : les octets d'origine d'une image chargée ; une image
/// créée depuis un handle n'a pas de format de fichier à écrire.
unsafe extern "win64" fn pic_save_as_file(
    p: usize,
    stm: usize,
    _mem_copy: i32,
    size: *mut i32,
) -> i32 {
    if stm == 0 {
        return E_POINTER;
    }
    let bytes = lock(&pic(p).state).original.clone();
    let Some(bytes) = bytes else {
        return E_NOTIMPL;
    };
    let hr = stream_write(stm, &bytes);
    if hr >= 0 && !size.is_null() {
        *size = bytes.len() as i32;
    }
    hr
}

unsafe extern "win64" fn pic_get_attributes(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if lock(&pic(p).state).kind == PICTYPE_ICON {
        PICTURE_TRANSPARENT
    } else {
        0
    };
    S_OK
}

unsafe extern "win64" fn picd_qi(d: usize, iid: *const Guid, out: *mut usize) -> i32 {
    pic_qi(d - 8, iid, out)
}

unsafe extern "win64" fn picd_add_ref(d: usize) -> u32 {
    pic_add_ref(d - 8)
}

unsafe extern "win64" fn picd_release(d: usize) -> u32 {
    pic_release(d - 8)
}

unsafe extern "win64" fn disp_type_info_count(_d: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

unsafe extern "win64" fn disp_type_info(
    _d: usize,
    _index: u32,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

unsafe extern "win64" fn picd_ids_of_names(
    _d: usize,
    _iid: *const Guid,
    names: *const *const u16,
    count: u32,
    _lcid: u32,
    ids: *mut i32,
) -> i32 {
    get_ids_of_names(PICTURE_NAMES, names, count, ids)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn picd_invoke(
    d: usize,
    id: i32,
    _iid: *const Guid,
    _lcid: u32,
    flags: u16,
    _params: *const u8,
    result: *mut u8,
    _excep: usize,
    _arg_err: *mut u32,
) -> i32 {
    if flags & DISPATCH_PROPERTYGET == 0 {
        return DISP_E_MEMBERNOTFOUND;
    }
    let st = lock(&pic(d - 8).state);
    let (vt, v) = match id {
        0 => (VT_I4, st.handle as i64),
        2 => (VT_I4, st.hpal as i64),
        3 => (VT_I2, i64::from(st.kind)),
        4 => (VT_I4, i64::from(st.width)),
        5 => (VT_I4, i64::from(st.height)),
        _ => return DISP_E_MEMBERNOTFOUND,
    };
    put_variant_i(result, vt, v);
    S_OK
}

/// Largeur et hauteur en pixels d'une bitmap GDI (GetObject → BITMAP).
fn bitmap_pixels(handle: usize) -> Option<(i32, i32)> {
    let mut bm = crate::dlls::comobj::Aligned::<32>::default();
    // SAFETY : BITMAP x64 fait 32 octets.
    let n = unsafe { super::gdi32::get_object_w(handle, 32, bm.as_mut_ptr().cast()) };
    if n <= 0 {
        return None;
    }
    let w = i32::from_le_bytes([bm[4], bm[5], bm[6], bm[7]]);
    let h = i32::from_le_bytes([bm[8], bm[9], bm[10], bm[11]]);
    Some((w, h.abs()))
}

fn icon_pixels(handle: usize) -> (i32, i32) {
    super::icons::icon_image(handle)
        .map(|i| (i.w as i32, i.h as i32))
        .unwrap_or((32, 32))
}

fn empty_picture(kind: i16) -> PictureState {
    PictureState {
        kind,
        handle: 0,
        hpal: 0,
        width: 0,
        height: 0,
        own: false,
        keep_original: true,
        original: None,
        cur_dc: 0,
    }
}

/// PICTDESC (x64) : cbSizeofstruct, picType, puis l'union à l'octet 8.
pub unsafe extern "win64" fn ole_create_picture_indirect(
    desc: *const u8,
    iid: *const Guid,
    own: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::pointer_size() != 8 {
        return E_NOTIMPL;
    }
    let mut st = empty_picture(PICTYPE_UNINITIALIZED);
    if !desc.is_null() {
        let kind = std::ptr::read_unaligned(desc.add(4) as *const u32) as i32 as i16;
        let handle = std::ptr::read_unaligned(desc.add(8) as *const usize);
        st.kind = kind;
        st.handle = handle;
        st.own = own != 0;
        match kind {
            PICTYPE_BITMAP => {
                st.hpal = std::ptr::read_unaligned(desc.add(16) as *const usize);
                let Some((w, h)) = bitmap_pixels(handle) else {
                    return E_INVALIDARG;
                };
                (st.width, st.height) = (himetric(w), himetric(h));
            }
            PICTYPE_ICON => {
                let (w, h) = icon_pixels(handle);
                (st.width, st.height) = (himetric(w), himetric(h));
            }
            PICTYPE_METAFILE => {
                st.width = std::ptr::read_unaligned(desc.add(16) as *const i32);
                st.height = std::ptr::read_unaligned(desc.add(20) as *const i32);
            }
            PICTYPE_ENHMETAFILE | PICTYPE_NONE | PICTYPE_UNINITIALIZED => {}
            _ => return E_INVALIDARG,
        }
    }
    hand_out(new_picture(st), iid, out)
}

/// Décode les octets d'une image en bitmap ou en icône.
fn picture_from_bytes(bytes: Vec<u8>) -> Result<PictureState, i32> {
    let is_ico = bytes.len() > 6 && bytes[0..4] == [0, 0, 1, 0];
    if is_ico {
        let img = super::icons::decode_ico_file(&bytes, 32).ok_or(CTL_E_INVALIDPICTURE)?;
        let (w, h) = (img.w as i32, img.h as i32);
        let handle = super::icons::new_icon(img);
        if handle == 0 {
            return Err(E_OUTOFMEMORY);
        }
        let mut st = empty_picture(PICTYPE_ICON);
        st.handle = handle;
        st.own = true;
        (st.width, st.height) = (himetric(w), himetric(h));
        st.original = Some(bytes);
        return Ok(st);
    }
    let mime = if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        return Err(CTL_E_INVALIDPICTURE);
    };
    let (w, h, rgba) = crate::clipbridge::decode_image(&bytes, mime).ok_or(CTL_E_INVALIDPICTURE)?;
    let px: Vec<u32> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let handle =
        super::gui::bitmap_from_pixels(w as usize, h as usize, &px).ok_or(E_OUTOFMEMORY)?;
    let mut st = empty_picture(PICTYPE_BITMAP);
    st.handle = handle;
    st.own = true;
    (st.width, st.height) = (himetric(w as i32), himetric(h as i32));
    st.original = Some(bytes);
    Ok(st)
}

/// Plafond d'une image lue d'un flux ou d'un fichier.
const MAX_PICTURE_BYTES: usize = 256 << 20;

pub unsafe extern "win64" fn ole_load_picture(
    stm: usize,
    size: i32,
    _run_mode: i32,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if stm == 0 || iid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::pointer_size() != 8 {
        return E_NOTIMPL;
    }
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stm, 3));
    let limit = if size > 0 {
        size as usize
    } else {
        MAX_PICTURE_BYTES
    };
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    while bytes.len() < limit {
        let want = chunk.len().min(limit - bytes.len());
        let mut got = 0u32;
        let hr = read(stm, chunk.as_mut_ptr(), want as u32, &mut got);
        if hr < 0 {
            return hr;
        }
        if got == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..got as usize]);
    }
    match picture_from_bytes(bytes) {
        Ok(st) => hand_out(new_picture(st), iid, out),
        Err(hr) => hr,
    }
}

/// Chemin local ou URL file:// ; les autres URL ne sont pas téléchargées.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn ole_load_picture_path(
    path: *const u16,
    _caller: usize,
    _reserved: u32,
    _color: u32,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if path.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::pointer_size() != 8 {
        return E_NOTIMPL;
    }
    let text = super::oleaut32_typelib::wide(path);
    let local = match text.get(..7) {
        Some(p) if p.eq_ignore_ascii_case("file://") => {
            text[7..].trim_start_matches('/').to_string()
        }
        _ if text.contains("://") => return E_NOTIMPL,
        _ => text,
    };
    let file = crate::paths::to_linux(&local);
    let bytes = match std::fs::metadata(&file) {
        Ok(m) if m.len() as usize <= MAX_PICTURE_BYTES => match std::fs::read(&file) {
            Ok(b) => b,
            Err(_) => return CTL_E_PATHFILEACCESSERROR,
        },
        _ => return CTL_E_PATHFILEACCESSERROR,
    };
    match picture_from_bytes(bytes) {
        Ok(st) => hand_out(new_picture(st), iid, out),
        Err(hr) => hr,
    }
}

// ---------- IFont ----------

#[derive(Clone)]
struct FontState {
    name: String,
    /// CY : points × 10 000.
    size: i64,
    weight: i16,
    charset: i16,
    italic: bool,
    underline: bool,
    strike: bool,
    ratio: (i32, i32),
    hfont: usize,
}

#[repr(C)]
struct Font {
    vtbl: *const (),
    disp: *const (),
    refs: AtomicU32,
    state: Mutex<FontState>,
}

static FONT_VT: VTable<27> = VTable([
    font_qi as *const (),
    font_add_ref as *const (),
    font_release as *const (),
    font_get_name as *const (),
    font_put_name as *const (),
    font_get_size as *const (),
    font_put_size as *const (),
    font_get_bold as *const (),
    font_put_bold as *const (),
    font_get_italic as *const (),
    font_put_italic as *const (),
    font_get_underline as *const (),
    font_put_underline as *const (),
    font_get_strike as *const (),
    font_put_strike as *const (),
    font_get_weight as *const (),
    font_put_weight as *const (),
    font_get_charset as *const (),
    font_put_charset as *const (),
    font_get_hfont as *const (),
    font_clone as *const (),
    font_is_equal as *const (),
    font_set_ratio as *const (),
    font_query_text_metrics as *const (),
    font_ok_handle as *const (), // AddRefHfont
    font_ok_handle as *const (), // ReleaseHfont
    font_ok_handle as *const (), // SetHdc
]);

static FONT_DISP_VT: VTable<7> = VTable([
    fontd_qi as *const (),
    fontd_add_ref as *const (),
    fontd_release as *const (),
    disp_type_info_count as *const (),
    disp_type_info as *const (),
    fontd_ids_of_names as *const (),
    fontd_invoke as *const (),
]);

const FONT_NAMES: Names = &[
    ("Name", 0),
    ("Size", 2),
    ("Bold", 3),
    ("Italic", 4),
    ("Underline", 5),
    ("Strikethrough", 6),
    ("Weight", 7),
    ("Charset", 8),
];

unsafe fn font(p: usize) -> &'static Font {
    &*(p as *const Font)
}

fn new_font(state: FontState) -> usize {
    Box::into_raw(Box::new(Font {
        vtbl: &FONT_VT as *const _ as *const (),
        disp: &FONT_DISP_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        state: Mutex::new(state),
    })) as usize
}

/// Modifie la police ; le HFONT déjà créé est détruit.
unsafe fn edit_font(p: usize, f: impl FnOnce(&mut FontState)) -> i32 {
    let old = {
        let mut st = lock(&font(p).state);
        f(&mut st);
        std::mem::replace(&mut st.hfont, 0)
    };
    if old != 0 {
        super::gdi32::delete_object(old);
    }
    S_OK
}

unsafe extern "win64" fn font_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(g) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    let target = if g == IID_IUNKNOWN || g == IID_IFONT {
        p
    } else if g == IID_IDISPATCH || g == IID_IFONT_DISP {
        p + 8
    } else {
        return E_NOINTERFACE;
    };
    font_add_ref(p);
    *out = target;
    S_OK
}

unsafe extern "win64" fn font_add_ref(p: usize) -> u32 {
    font(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn font_release(p: usize) -> u32 {
    let left = font(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut Font);
        let st = b.state.into_inner().unwrap_or_else(|e| e.into_inner());
        if st.hfont != 0 {
            super::gdi32::delete_object(st.hfont);
        }
    }
    left
}

unsafe fn bstr(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().collect();
    sys_alloc_string_len(w.as_ptr(), w.len() as u32)
}

unsafe extern "win64" fn font_get_name(p: usize, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = bstr(&lock(&font(p).state).name);
    S_OK
}

unsafe extern "win64" fn font_put_name(p: usize, name: *const u16) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    let name = super::oleaut32_typelib::wide(name);
    edit_font(p, |st| st.name = name)
}

unsafe extern "win64" fn font_get_size(p: usize, out: *mut i64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).size;
    S_OK
}

unsafe extern "win64" fn font_put_size(p: usize, size: i64) -> i32 {
    edit_font(p, |st| st.size = size)
}

/// Les booléens OLE (VARIANT_BOOL / BOOL) : vrai = non nul.
unsafe extern "win64" fn font_get_bold(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (lock(&font(p).state).weight >= 550) as i32;
    S_OK
}

unsafe extern "win64" fn font_put_bold(p: usize, bold: i32) -> i32 {
    edit_font(p, |st| st.weight = if bold != 0 { 700 } else { 400 })
}

unsafe extern "win64" fn font_get_italic(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).italic as i32;
    S_OK
}

unsafe extern "win64" fn font_put_italic(p: usize, v: i32) -> i32 {
    edit_font(p, |st| st.italic = v != 0)
}

unsafe extern "win64" fn font_get_underline(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).underline as i32;
    S_OK
}

unsafe extern "win64" fn font_put_underline(p: usize, v: i32) -> i32 {
    edit_font(p, |st| st.underline = v != 0)
}

unsafe extern "win64" fn font_get_strike(p: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).strike as i32;
    S_OK
}

unsafe extern "win64" fn font_put_strike(p: usize, v: i32) -> i32 {
    edit_font(p, |st| st.strike = v != 0)
}

unsafe extern "win64" fn font_get_weight(p: usize, out: *mut i16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).weight;
    S_OK
}

unsafe extern "win64" fn font_put_weight(p: usize, v: i16) -> i32 {
    edit_font(p, |st| st.weight = v)
}

unsafe extern "win64" fn font_get_charset(p: usize, out: *mut i16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&font(p).state).charset;
    S_OK
}

unsafe extern "win64" fn font_put_charset(p: usize, v: i16) -> i32 {
    edit_font(p, |st| st.charset = v)
}

/// Hauteur logique : points → pixels à 96 points par pouce, ou par le
/// rapport de SetRatio (unités logiques par HIMETRIC).
fn font_height(st: &FontState) -> i32 {
    let (logical, himetric) = st.ratio;
    let px = if logical != 0 && himetric != 0 {
        let hm = st.size as i128 * 2540 / 72 / 10_000;
        hm * i128::from(logical) / i128::from(himetric)
    } else {
        (st.size as i128 * 96 + 360_000) / 720_000
    };
    -(px.clamp(1, 16_384) as i32)
}

unsafe extern "win64" fn font_get_hfont(p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut st = lock(&font(p).state);
    if st.hfont == 0 {
        let mut face = [0u16; 32];
        for (d, s) in face.iter_mut().zip(st.name.encode_utf16().take(31)) {
            *d = s;
        }
        let lf = super::gdi32::LogFontW {
            height: font_height(&st),
            width: 0,
            escapement: 0,
            orientation: 0,
            weight: i32::from(st.weight),
            italic: st.italic as u8,
            underline: st.underline as u8,
            strikeout: st.strike as u8,
            charset: st.charset as u8,
            out_precision: 0,
            clip_precision: 0,
            quality: 0,
            pitch_and_family: 0,
            face,
        };
        st.hfont = super::gdi32_ext::create_font_indirect_w(&lf);
    }
    *out = st.hfont;
    if st.hfont == 0 {
        E_FAIL
    } else {
        S_OK
    }
}

unsafe extern "win64" fn font_clone(p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut st = lock(&font(p).state).clone();
    st.hfont = 0;
    *out = new_font(st);
    S_OK
}

unsafe extern "win64" fn font_is_equal(p: usize, other: usize) -> i32 {
    if other == 0 {
        return E_POINTER;
    }
    if *(other as *const *const ()) != &FONT_VT as *const _ as *const () {
        return S_FALSE;
    }
    if other == p {
        return S_OK;
    }
    let (a, b) = (
        lock(&font(p).state).clone(),
        lock(&font(other).state).clone(),
    );
    let same = a.name.eq_ignore_ascii_case(&b.name)
        && (a.size, a.weight, a.charset, a.italic, a.underline, a.strike)
            == (b.size, b.weight, b.charset, b.italic, b.underline, b.strike);
    if same {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn font_set_ratio(p: usize, logical: i32, himetric: i32) -> i32 {
    if logical == 0 || himetric == 0 {
        return E_FAIL;
    }
    edit_font(p, |st| st.ratio = (logical, himetric))
}

extern "win64" fn font_query_text_metrics(_p: usize, _tm: usize) -> i32 {
    E_NOTIMPL
}

extern "win64" fn font_ok_handle(_p: usize, _h: usize) -> i32 {
    S_OK
}

unsafe extern "win64" fn fontd_qi(d: usize, iid: *const Guid, out: *mut usize) -> i32 {
    font_qi(d - 8, iid, out)
}

unsafe extern "win64" fn fontd_add_ref(d: usize) -> u32 {
    font_add_ref(d - 8)
}

unsafe extern "win64" fn fontd_release(d: usize) -> u32 {
    font_release(d - 8)
}

unsafe extern "win64" fn fontd_ids_of_names(
    _d: usize,
    _iid: *const Guid,
    names: *const *const u16,
    count: u32,
    _lcid: u32,
    ids: *mut i32,
) -> i32 {
    get_ids_of_names(FONT_NAMES, names, count, ids)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn fontd_invoke(
    d: usize,
    id: i32,
    _iid: *const Guid,
    _lcid: u32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    _excep: usize,
    _arg_err: *mut u32,
) -> i32 {
    let p = d - 8;
    let vt = match id {
        0 => VT_BSTR,
        2 => VT_CY,
        3..=6 => VT_BOOL,
        7 | 8 => VT_I2,
        _ => return DISP_E_MEMBERNOTFOUND,
    };
    if flags & DISPATCH_PROPERTYPUT != 0 {
        let mut v = match put_argument(params, vt) {
            Ok(v) => v,
            Err(hr) => return hr,
        };
        let at = v.as_ptr().add(8);
        let hr = match id {
            0 => font_put_name(p, std::ptr::read_unaligned(at as *const *const u16)),
            2 => font_put_size(p, std::ptr::read_unaligned(at as *const i64)),
            3..=6 => {
                let b = i32::from(std::ptr::read_unaligned(at as *const i16) != 0);
                match id {
                    3 => font_put_bold(p, b),
                    4 => font_put_italic(p, b),
                    5 => font_put_underline(p, b),
                    _ => font_put_strike(p, b),
                }
            }
            7 => font_put_weight(p, std::ptr::read_unaligned(at as *const i16)),
            _ => font_put_charset(p, std::ptr::read_unaligned(at as *const i16)),
        };
        variant_clear(v.as_mut_ptr().cast());
        return hr;
    }
    if flags & DISPATCH_PROPERTYGET == 0 {
        return DISP_E_MEMBERNOTFOUND;
    }
    if result.is_null() {
        return S_OK;
    }
    let st = lock(&font(p).state).clone();
    match id {
        0 => {
            variant_clear(result.cast());
            std::ptr::write_unaligned(result as *mut u16, VT_BSTR);
            std::ptr::write_unaligned(result.add(8) as *mut *mut u16, bstr(&st.name));
        }
        2 => put_variant_i(result, VT_CY, st.size),
        3 => put_variant_i(result, VT_BOOL, -i64::from(st.weight >= 550)),
        4 => put_variant_i(result, VT_BOOL, -i64::from(st.italic)),
        5 => put_variant_i(result, VT_BOOL, -i64::from(st.underline)),
        6 => put_variant_i(result, VT_BOOL, -i64::from(st.strike)),
        7 => put_variant_i(result, VT_I2, i64::from(st.weight)),
        _ => put_variant_i(result, VT_I2, i64::from(st.charset)),
    }
    S_OK
}

/// FONTDESC (x64) : cbSizeofstruct, lpstrName (8), cySize (16), sWeight
/// (24), sCharset (26), fItalic (28), fUnderline (32), fStrikethrough (36).
pub unsafe extern "win64" fn ole_create_font_indirect(
    desc: *const u8,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if desc.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    if crate::wow32::pointer_size() != 8 {
        return E_NOTIMPL;
    }
    let name_ptr = std::ptr::read_unaligned(desc.add(8) as *const *const u16);
    let st = FontState {
        name: if name_ptr.is_null() {
            String::new()
        } else {
            super::oleaut32_typelib::wide(name_ptr)
        },
        size: std::ptr::read_unaligned(desc.add(16) as *const i64),
        weight: std::ptr::read_unaligned(desc.add(24) as *const i16),
        charset: std::ptr::read_unaligned(desc.add(26) as *const i16),
        italic: std::ptr::read_unaligned(desc.add(28) as *const i32) != 0,
        underline: std::ptr::read_unaligned(desc.add(32) as *const i32) != 0,
        strike: std::ptr::read_unaligned(desc.add(36) as *const i32) != 0,
        ratio: (0, 0),
        hfont: 0,
    };
    hand_out(new_font(st), iid, out)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OleCreatePictureIndirect" => fn ole_create_picture_indirect,
        "OleLoadPicture" => fn ole_load_picture,
        "OleLoadPicturePath" => fn ole_load_picture_path,
        "OleCreateFontIndirect" => fn ole_create_font_indirect,
        "#418" => fn ole_load_picture,
        "#419" => fn ole_create_picture_indirect,
        "#420" => fn ole_create_font_indirect,
        "#424" => fn ole_load_picture_path,
    }
}

#[cfg(test)]
#[path = "oleaut32_picture/tests.rs"]
mod tests;
