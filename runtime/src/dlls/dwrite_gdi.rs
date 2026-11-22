//! IDWriteGdiInterop (GetGdiInterop) et IDWriteFontResource
//! (IDWriteFontFace5::GetFontResource) (#3078).
//!
//! Provenance : ordre des vtables et IID tirés de windows-rs (MIT, crate
//! `windows` 0.58, `Win32/Graphics/DirectWrite`) ; comportement d'après les
//! pages Learn de IDWriteGdiInterop et IDWriteFontResource. Learn ne dit pas
//! ce que rend CreateFontFromLOGFONT pour un nom inconnu : nous rendons
//! DWRITE_E_NOFONT (inférence), sans prendre une famille au hasard.
//!
//! Les polices de Winlux n'ont pas d'axes de variation (GetFontAxisValueCount
//! vaut 0), d'où une ressource sans axe.

use super::sets::{put_face, put_ref};
use super::{
    is_windows_font, pick, read_wide, substitutes, FaceInfo, Family, DWRITE_E_NOFONT, FILE_VT,
    FONT_VT,
};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::d2d1::{add_ref, qi_list, release};
use crate::dlls::ole32::Guid;
use std::ffi::c_void;

pub(super) const IID_GDI_INTEROP: Guid = guid(
    0x1edd_9491,
    0x9853,
    0x4299,
    [0x89, 0x8f, 0x64, 0x32, 0x98, 0x3b, 0x6f, 0x3a],
);
pub(super) const IID_FONT_RESOURCE: Guid = guid(
    0x1f80_3a76,
    0x6871,
    0x48e8,
    [0x98, 0x7f, 0xb9, 0x75, 0x55, 0x1c, 0x50, 0xf2],
);

/// LOGFONTW (92 octets).
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct LogFontW {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strike_out: u8,
    pub char_set: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
    pub face_name: [u16; 32],
}

const DEFAULT_CHARSET: u8 = 1;

/// Familles que GDI connaît, polices ajoutées par AddFontResource(Ex)
/// comprises, comme pour CreateFont.
fn gdi_families() -> Vec<Family> {
    let all: Vec<FaceInfo> = crate::dlls::gdi32_fonts::all_faces()
        .iter()
        .filter(|f| !f.cff)
        .map(super::info)
        .collect();
    super::group(
        all.into_iter()
            .chain(crate::dlls::lock(super::extra()).clone()),
    )
}

/// Face d'un nom de police GDI : la famille elle-même, ou la remplaçante
/// d'une police Windows absente ; jamais une famille prise au hasard.
pub(super) fn logfont_face(name: &str, weight: u32, italic: bool) -> Option<FaceInfo> {
    let fams = gdi_families();
    let find = |n: &str| fams.iter().find(|f| f.name.eq_ignore_ascii_case(n));
    let name = match name {
        "" | "System" | "MS Shell Dlg" | "MS Shell Dlg 2" => "Segoe UI",
        n => n,
    };
    if let Some(f) = find(name) {
        return Some(pick(f, weight, italic));
    }
    if is_windows_font(name) {
        return substitutes(name)
            .iter()
            .find_map(|s| find(s))
            .map(|f| pick(f, weight, italic));
    }
    None
}

fn fill_logfont(f: &FaceInfo, out: &mut LogFontW) {
    *out = LogFontW {
        height: 0,
        width: 0,
        escapement: 0,
        orientation: 0,
        weight: f.weight as i32,
        italic: f.italic as u8,
        underline: 0,
        strike_out: 0,
        char_set: DEFAULT_CHARSET,
        out_precision: 0,
        clip_precision: 0,
        quality: 0,
        pitch_and_family: 0,
        face_name: [0; 32],
    };
    for (d, s) in out
        .face_name
        .iter_mut()
        .zip(f.family.encode_utf16().take(31))
    {
        *d = s;
    }
}

// --- IDWriteGdiInterop ------------------------------------------------------------------

type GdiObj = Com<()>;

pub(super) unsafe extern "win64" fn get_gdi_interop(
    _this: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(GDI_VT.ptr(), ()))
}

unsafe extern "win64" fn gdi_qi(
    this: *mut GdiObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_GDI_INTEROP])
}

unsafe extern "win64" fn from_logfont(
    _this: *mut GdiObj,
    lf: *const LogFontW,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if lf.is_null() {
        return E_INVALIDARG;
    }
    let lf = &*lf;
    let name = read_wide(lf.face_name.as_ptr());
    let weight = if lf.weight <= 0 {
        400
    } else {
        lf.weight as u32
    };
    match logfont_face(&name, weight, lf.italic != 0) {
        Some(fi) => put(out, Com::create(FONT_VT.ptr(), fi)),
        None => DWRITE_E_NOFONT,
    }
}

unsafe extern "win64" fn font_to_logfont(
    _this: *mut GdiObj,
    font: *mut c_void,
    lf: *mut LogFontW,
    system: *mut i32,
) -> i32 {
    if lf.is_null() || system.is_null() {
        return E_POINTER;
    }
    if font.is_null() || *(font as *const *const ()) != FONT_VT.ptr() {
        *system = 0;
        return E_INVALIDARG;
    }
    let fi = Com::get(font as *mut Com<FaceInfo>);
    fill_logfont(fi, &mut *lf);
    *system = 1;
    S_OK
}

unsafe extern "win64" fn face_to_logfont(
    _this: *mut GdiObj,
    face: *mut c_void,
    lf: *mut LogFontW,
) -> i32 {
    if lf.is_null() {
        return E_POINTER;
    }
    match super::face_of(face) {
        Some(f) => {
            fill_logfont(&f.info, &mut *lf);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn face_from_hdc(
    _this: *mut GdiObj,
    hdc: usize,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let spec = crate::dlls::gui::font_spec(crate::dlls::gui::dc_font(hdc)).unwrap_or_default();
    let weight = if spec.weight <= 0 {
        400
    } else {
        spec.weight as u32
    };
    // Le DC a déjà fait la correspondance de police (Learn) : même choix
    // que CreateFont, remplaçantes comprises.
    match super::gdi_face(&spec.face, weight, spec.italic) {
        Some(fi) => put_face(&fi, out),
        None => DWRITE_E_NOFONT,
    }
}

static GDI_VT: VTable<8> = VTable([
    gdi_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    from_logfont as *const (),
    font_to_logfont as *const (),
    face_to_logfont as *const (),
    face_from_hdc as *const (),
    // IDWriteBitmapRenderTarget n'existe pas encore dans Winlux : refus
    // explicite (E_NOTIMPL), journalisé (dwrite_gdi.rs, CreateBitmapRenderTarget).
    unsupported!("IDWriteGdiInterop::CreateBitmapRenderTarget"),
]);

// --- IDWriteFontResource ------------------------------------------------------------------

type ResObj = Com<FaceInfo>;

/// IDWriteFontFace5::GetFontResource.
pub(super) unsafe extern "win64" fn face_resource(
    this: *mut super::FaceObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(RES_VT.ptr(), Com::get(this).info.clone()))
}

unsafe extern "win64" fn res_qi(
    this: *mut ResObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_RESOURCE])
}
unsafe extern "win64" fn res_file(this: *mut ResObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(FILE_VT.ptr(), Com::get(this).path.clone()))
}
unsafe extern "win64" fn res_index(this: *mut ResObj) -> u32 {
    Com::get(this).index
}
unsafe extern "win64" fn res_zero(_this: *mut ResObj) -> u32 {
    0
}
unsafe extern "win64" fn res_axis_values(_this: *mut ResObj, _values: *mut c_void, n: u32) -> i32 {
    // Aucun axe : seul un tableau vide est valable.
    if n != 0 {
        return E_INVALIDARG;
    }
    S_OK
}
unsafe extern "win64" fn res_axis_attributes(_this: *mut ResObj, _axis: u32) -> u32 {
    0 // DWRITE_FONT_AXIS_ATTRIBUTES_NONE
}
unsafe extern "win64" fn res_axis_names(
    _this: *mut ResObj,
    _axis: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_INVALIDARG
}
unsafe extern "win64" fn res_value_name_count(_this: *mut ResObj, _axis: u32) -> u32 {
    0
}
unsafe extern "win64" fn res_value_names(
    _this: *mut ResObj,
    _axis: u32,
    _value: u32,
    _range: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_INVALIDARG
}
unsafe extern "win64" fn res_false(_this: *mut ResObj) -> i32 {
    0
}
unsafe extern "win64" fn res_face(
    this: *mut ResObj,
    _sims: u32,
    _values: *const c_void,
    _n: u32,
    out: *mut *mut c_void,
) -> i32 {
    put_face(Com::get(this), out)
}
unsafe extern "win64" fn res_reference(
    this: *mut ResObj,
    _sims: u32,
    _values: *const c_void,
    _n: u32,
    out: *mut *mut c_void,
) -> i32 {
    put_ref(Com::get(this), out)
}

static RES_VT: VTable<15> = VTable([
    res_qi as *const (),
    add_ref::<FaceInfo> as *const (),
    release::<FaceInfo> as *const (),
    res_file as *const (),
    res_index as *const (),
    res_zero as *const (),        // GetFontAxisCount
    res_axis_values as *const (), // GetDefaultFontAxisValues
    res_axis_values as *const (), // GetFontAxisRanges
    res_axis_attributes as *const (),
    res_axis_names as *const (),
    res_value_name_count as *const (),
    res_value_names as *const (),
    res_false as *const (), // HasVariations
    res_face as *const (),
    res_reference as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logfont_segoe_ui_servi_et_nom_inconnu_refuse_3078() {
        let fams = gdi_families();
        if fams.is_empty() {
            return; // aucune police dans le conteneur
        }
        let has_sub = substitutes("Segoe UI")
            .iter()
            .any(|s| fams.iter().any(|f| f.name.eq_ignore_ascii_case(s)));
        if has_sub {
            assert!(logfont_face("Segoe UI", 400, false).is_some());
        }
        assert!(logfont_face("Police qui n'existe pas 3078", 400, false).is_none());
        unsafe {
            let gdi: *mut c_void = std::ptr::null_mut();
            let mut gdi = gdi;
            assert_eq!(get_gdi_interop(std::ptr::null_mut(), &mut gdi), S_OK);
            let mut lf: LogFontW = std::mem::zeroed();
            for (d, s) in lf
                .face_name
                .iter_mut()
                .zip("Police qui n'existe pas 3078".encode_utf16())
            {
                *d = s;
            }
            let mut font: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                from_logfont(gdi as *mut GdiObj, &lf, &mut font),
                DWRITE_E_NOFONT
            );
            assert!(font.is_null());
            let f = &fams[0].faces[0];
            let mut back: LogFontW = std::mem::zeroed();
            let mut system = 0;
            let font = Com::create(FONT_VT.ptr(), f.clone()) as *mut c_void;
            assert_eq!(
                font_to_logfont(gdi as *mut GdiObj, font, &mut back, &mut system),
                S_OK
            );
            assert_eq!(system, 1);
            assert_eq!(
                read_wide(back.face_name.as_ptr()),
                f.family.chars().take(31).collect::<String>()
            );
            assert_eq!(back.weight, f.weight as i32);
            // Aller-retour : le LOGFONT rendu retrouve la même famille.
            let mut again: *mut c_void = std::ptr::null_mut();
            assert_eq!(from_logfont(gdi as *mut GdiObj, &back, &mut again), S_OK);
            assert_eq!(Com::get(again as *mut Com<FaceInfo>).family, f.family);
        }
    }

    #[test]
    fn ressource_de_face_sans_axe_3078() {
        let fams = gdi_families();
        let Some(f) = fams.first().map(|x| x.faces[0].clone()) else {
            return;
        };
        unsafe {
            let res = Com::create(RES_VT.ptr(), f.clone());
            assert_eq!(res_index(res), f.index);
            assert_eq!(res_zero(res), 0);
            assert_eq!(res_axis_values(res, std::ptr::null_mut(), 0), S_OK);
            assert_eq!(res_axis_values(res, std::ptr::null_mut(), 1), E_INVALIDARG);
            assert_eq!(res_false(res), 0);
            let mut face: *mut c_void = std::ptr::null_mut();
            if crate::dlls::sfnt::load(&f.path, f.index).is_some() {
                assert_eq!(res_face(res, 0, std::ptr::null(), 0, &mut face), S_OK);
                assert_eq!(super::super::face_of(face).unwrap().info, f);
            }
            let mut file: *mut c_void = std::ptr::null_mut();
            assert_eq!(res_file(res, &mut file), S_OK);
            assert!(!file.is_null());
        }
    }
}
