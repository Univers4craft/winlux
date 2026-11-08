//! IDWriteFactory3 à IDWriteFactory7 : jeux de polices (IDWriteFontSet 1 à
//! 4), constructeur de jeux, références de faces, collections, familles,
//! listes et polices de niveau 1 à 3, faces IDWriteFontFace1 à 6 et
//! paramètres de rendu IDWriteRenderingParams3 (#984).
//!
//! Ordre des méthodes : en-têtes mingw-w64 (dwrite_1.h, dwrite_2.h,
//! dwrite_3.h), recoupés avec TerraFX.Interop.Windows (MIT). Le
//! comportement suit la documentation publique de DirectWrite.
//!
//! Paint.NET 5.1, WinUI et les programmes .NET qui passent par TerraFX
//! demandent IDWriteFactory7 et énumèrent les polices par
//! `GetSystemFontCollection` ou `GetSystemFontSet`. Toutes les polices de
//! Winlux sont locales, statiques (sans axes de variation) et sans
//! simulation : les axes standard d'une police se déduisent de sa graisse
//! et de son style, et une police n'est jamais « distante ».
#![allow(clippy::missing_safety_doc, clippy::too_many_arguments)]

use super::{
    create_text_format, en, families, file_time, group, is_windows_font, new_face, new_strings,
    read_wide, strings_count, strings_locale, strings_locale_len, strings_string,
    strings_string_len, substitutes, CollectionObj, Face, FaceInfo, FaceObj, FamilyObj, FileObj,
    FontMetrics, FontObj, GlyphRun, COLLECTION_VT, E_NOT_SUFFICIENT_BUFFER, FACE_VT, FILE_VT,
    FONT_VT,
};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::d2d1::{add_ref, qi_list, release};
use crate::dlls::ole32::Guid;
use crate::dlls::sfnt;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

const S_FALSE: i32 = 1;
const DWRITE_E_NOCOLOR: i32 = super::fallback::DWRITE_E_NOCOLOR;
const DWRITE_E_FILEFORMAT: i32 = 0x8898_5000u32 as i32;

pub const IID_DWRITE_FACTORY3: Guid = guid(
    0x9a1b41c3,
    0xd3bb,
    0x466a,
    [0x87, 0xfc, 0xfe, 0x67, 0x55, 0x6a, 0x3b, 0x65],
);
pub const IID_DWRITE_FACTORY4: Guid = guid(
    0x4b0b5bd3,
    0x0797,
    0x4549,
    [0x8a, 0xc5, 0xfe, 0x91, 0x5c, 0xc5, 0x38, 0x56],
);
pub const IID_DWRITE_FACTORY5: Guid = guid(
    0x958db99a,
    0xbe2a,
    0x4f09,
    [0xaf, 0x7d, 0x65, 0x18, 0x98, 0x03, 0xd1, 0xd3],
);
pub const IID_DWRITE_FACTORY6: Guid = guid(
    0xf3744d80,
    0x21f7,
    0x42eb,
    [0xb3, 0x5d, 0x99, 0x5b, 0xc7, 0x2f, 0xc2, 0x23],
);
pub const IID_DWRITE_FACTORY7: Guid = guid(
    0x35d0e0b3,
    0x9076,
    0x4d2e,
    [0xa0, 0x16, 0xa9, 0x1b, 0x56, 0x8a, 0x06, 0xb4],
);

const IID_FONT_SET: Guid = guid(
    0x53585141,
    0xd9f8,
    0x4095,
    [0x83, 0x21, 0xd7, 0x3c, 0xf6, 0xbd, 0x11, 0x6b],
);
const IID_FONT_SET1: Guid = guid(
    0x7e9fda85,
    0x6c92,
    0x4053,
    [0xbc, 0x47, 0x7a, 0xe3, 0x53, 0x0d, 0xb4, 0xd3],
);
const IID_FONT_SET2: Guid = guid(
    0xdc7ead19,
    0xe54c,
    0x43af,
    [0xb2, 0xda, 0x4e, 0x2b, 0x79, 0xba, 0x3f, 0x7f],
);
const IID_FONT_SET3: Guid = guid(
    0x7c073ef2,
    0xa7f4,
    0x4045,
    [0x8c, 0x32, 0x8a, 0xb8, 0xae, 0x64, 0x0f, 0x90],
);
const IID_FONT_SET4: Guid = guid(
    0xeec175fc,
    0xbea9,
    0x4c86,
    [0x8b, 0x53, 0xcc, 0xbd, 0xd7, 0xdf, 0x0c, 0x82],
);
pub const IID_FONT_COLLECTION1: Guid = guid(
    0x53585141,
    0xd9f8,
    0x4095,
    [0x83, 0x21, 0xd7, 0x3c, 0xf6, 0xbd, 0x11, 0x6c],
);
pub const IID_FONT_COLLECTION2: Guid = guid(
    0x514039c6,
    0x4617,
    0x4064,
    [0xbf, 0x8b, 0x92, 0xea, 0x83, 0xe5, 0x06, 0xe0],
);
pub const IID_FONT_COLLECTION3: Guid = guid(
    0xa4d055a6,
    0xf9e3,
    0x4e25,
    [0x93, 0xb7, 0x9e, 0x30, 0x9f, 0x3a, 0xf8, 0xe9],
);
pub const IID_FONT_FAMILY1: Guid = guid(
    0xda20d8ef,
    0x812a,
    0x4c43,
    [0x98, 0x02, 0x62, 0xec, 0x4a, 0xbd, 0x7a, 0xdf],
);
pub const IID_FONT_FAMILY2: Guid = guid(
    0x3ed49e77,
    0xa398,
    0x4261,
    [0xb9, 0xcf, 0xc1, 0x26, 0xc2, 0x13, 0x1e, 0xf3],
);
const IID_FONT_LIST1: Guid = guid(
    0xda20d8ef,
    0x812a,
    0x4c43,
    [0x98, 0x02, 0x62, 0xec, 0x4a, 0xbd, 0x7a, 0xde],
);
const IID_FONT_LIST2: Guid = guid(
    0xc0763a34,
    0x77af,
    0x445a,
    [0xb7, 0x35, 0x08, 0xc3, 0x7b, 0x0a, 0x5b, 0xf5],
);
pub const IID_FONT1: Guid = guid(
    0xacd16696,
    0x8c14,
    0x4f5d,
    [0x87, 0x7e, 0xfe, 0x3f, 0xc1, 0xd3, 0x27, 0x38],
);
/// IDWriteFont2 et IDWriteFont3 portent le même identifiant dans les
/// en-têtes publics.
pub const IID_FONT2: Guid = guid(
    0x29748ed6,
    0x8c9c,
    0x4a6a,
    [0xbe, 0x0b, 0xd9, 0x12, 0xe8, 0x53, 0x89, 0x44],
);
const IID_FACE_REFERENCE: Guid = guid(
    0x5e7fa7ca,
    0xdde3,
    0x424c,
    [0x89, 0xf0, 0x9f, 0xcd, 0x6f, 0xed, 0x58, 0xcd],
);
const IID_FACE_REFERENCE1: Guid = guid(
    0xc081fe77,
    0x2fd1,
    0x41ac,
    [0xa5, 0xa3, 0x34, 0x98, 0x3c, 0x4b, 0xa6, 0x1a],
);
const IID_SET_BUILDER: Guid = guid(
    0x2f642afe,
    0x9c68,
    0x4f40,
    [0xb8, 0xbe, 0x45, 0x74, 0x01, 0xaf, 0xcb, 0x3d],
);
const IID_SET_BUILDER1: Guid = guid(
    0x3ff7715f,
    0x3cdc,
    0x4dc6,
    [0x9b, 0x72, 0xec, 0x56, 0x21, 0xdc, 0xca, 0xfd],
);
const IID_SET_BUILDER2: Guid = guid(
    0xee5ba612,
    0xb131,
    0x463c,
    [0x8f, 0x4f, 0x31, 0x89, 0xb9, 0x40, 0x1e, 0x45],
);
pub const IID_FONT_FACE_N: [Guid; 6] = [
    guid(
        0xa71efdb4,
        0x9fdb,
        0x4838,
        [0xad, 0x90, 0xcf, 0xc3, 0xbe, 0x8c, 0x3d, 0xaf],
    ),
    guid(
        0xd8b768ff,
        0x64bc,
        0x4e66,
        [0x98, 0x2b, 0xec, 0x8e, 0x87, 0xf6, 0x93, 0xf7],
    ),
    guid(
        0xd37d7598,
        0x09be,
        0x4222,
        [0xa2, 0x36, 0x20, 0x81, 0x34, 0x1c, 0xc1, 0xf2],
    ),
    guid(
        0x27f2a904,
        0x4eb8,
        0x441d,
        [0x96, 0x78, 0x05, 0x63, 0xf5, 0x3e, 0x3e, 0x2f],
    ),
    guid(
        0x98eff3a5,
        0xb667,
        0x479a,
        [0xb1, 0x45, 0xe2, 0xfa, 0x5b, 0x9f, 0xdc, 0x29],
    ),
    guid(
        0xc4b1fe1b,
        0x6e84,
        0x47d5,
        [0xb5, 0x4c, 0xa5, 0x97, 0x98, 0x1b, 0x06, 0xad],
    ),
];
const IID_DOWNLOAD_QUEUE: Guid = guid(
    0xb71e6052,
    0x5aea,
    0x4fa3,
    [0x83, 0x2e, 0xf6, 0x0d, 0x43, 0x1f, 0x7e, 0x91],
);
const IID_STRING_LIST: Guid = guid(
    0xcfee3140,
    0x1257,
    0x47ca,
    [0x8b, 0x85, 0x31, 0xbf, 0xcf, 0x3f, 0x2d, 0x0e],
);

/// DWRITE_LOCALITY_LOCAL : toutes les polices sont sur le disque.
const LOCALITY_LOCAL: u32 = 2;
/// DWRITE_FONT_FAMILY_MODEL_WEIGHT_STRETCH_STYLE.
const MODEL_WSS: u32 = 1;

// --- Structures publiques -----------------------------------------------------------

/// DWRITE_FONT_PROPERTY.
#[repr(C)]
pub struct FontProperty {
    id: u32,
    value: *const u16,
    locale: *const u16,
}

/// DWRITE_FONT_AXIS_VALUE.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisValue {
    tag: u32,
    value: f32,
}

/// DWRITE_FONT_AXIS_RANGE.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisRange {
    tag: u32,
    min: f32,
    max: f32,
}

/// DWRITE_FONT_METRICS1.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FontMetrics1 {
    base: FontMetrics,
    glyph_box: [i16; 4],
    subscript_position: [i16; 2],
    subscript_size: [i16; 2],
    superscript_position: [i16; 2],
    superscript_size: [i16; 2],
    has_typographic_metrics: i32,
}

/// DWRITE_CARET_METRICS.
#[repr(C)]
pub struct CaretMetrics {
    slope_rise: i16,
    slope_run: i16,
    offset: i16,
}

/// DWRITE_UNICODE_RANGE.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnicodeRange {
    first: u32,
    last: u32,
}

/// D2D1_POINT_2F (passé par valeur).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    x: f32,
    y: f32,
}

const fn tag(t: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*t)
}
const WGHT: u32 = tag(b"wght");
const WDTH: u32 = tag(b"wdth");
const ITAL: u32 = tag(b"ital");
const SLNT: u32 = tag(b"slnt");
const OPSZ: u32 = tag(b"opsz");

// --- Outils communs ---------------------------------------------------------------------

type SetObj = Com<Vec<FaceInfo>>;
type ListObj = Com<Vec<FaceInfo>>;
type RefObj = Com<FaceInfo>;
type BuilderObj = Com<Mutex<Vec<FaceInfo>>>;

/// Vrai si `p` est un objet de Winlux dont la table est `vt`.
unsafe fn is(p: *mut c_void, vt: *const ()) -> bool {
    !p.is_null() && *(p as *const *const ()) == vt
}

/// Toutes les faces du catalogue, famille par famille.
fn all_faces() -> Vec<FaceInfo> {
    families().into_iter().flat_map(|f| f.faces).collect()
}

/// Chargement d'une face pour IDWriteFontFace.
fn load(f: &FaceInfo) -> Option<Arc<sfnt::Font>> {
    sfnt::load(&f.path, f.index)
}

unsafe fn put_face(f: &FaceInfo, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match load(f) {
        Some(font) => put(out, new_face(f.clone(), font)),
        None => {
            *out = std::ptr::null_mut();
            DWRITE_E_FILEFORMAT
        }
    }
}

unsafe fn put_set(faces: Vec<FaceInfo>, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(SET_VT.ptr(), faces))
}

unsafe fn put_ref(f: &FaceInfo, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(REF_VT.ptr(), f.clone()))
}

unsafe fn put_font(f: &FaceInfo, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(FONT_VT.ptr(), f.clone()))
}

unsafe fn put_collection(faces: Vec<FaceInfo>, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(COLLECTION_VT.ptr(), group(faces)))
}

/// La face décrite par un objet DirectWrite de Winlux (référence, police,
/// face) ; `None` pour un objet venu d'ailleurs.
unsafe fn info_of(p: *mut c_void) -> Option<FaceInfo> {
    if is(p, REF_VT.ptr()) {
        return Some(Com::get(p as *mut RefObj).clone());
    }
    if is(p, FONT_VT.ptr()) {
        return Some(Com::get(p as *mut FontObj).clone());
    }
    if is(p, FACE_VT.ptr()) {
        return Some(Com::get(p as *mut FaceObj).info.clone());
    }
    None
}

unsafe fn faces_of_set(p: *mut c_void) -> Option<Vec<FaceInfo>> {
    is(p, SET_VT.ptr()).then(|| Com::get(p as *mut SetObj).clone())
}

fn same(a: &FaceInfo, b: &FaceInfo) -> bool {
    a.path == b.path && a.index == b.index
}

/// Nom de famille demandé, y compris une police Windows absente servie par
/// sa remplaçante libre (comme FindFamilyName).
fn family_matches(f: &FaceInfo, name: &str) -> bool {
    f.family.eq_ignore_ascii_case(name)
}
fn resolve_family(faces: &[FaceInfo], name: &str) -> Option<String> {
    if faces.iter().any(|f| family_matches(f, name)) {
        return Some(name.to_string());
    }
    if is_windows_font(name) {
        for s in substitutes(name) {
            if faces.iter().any(|f| family_matches(f, s)) {
                return Some((*s).to_string());
            }
        }
    }
    None
}

/// DWRITE_FONT_STYLE d'une face : NORMAL 0, ITALIC 2.
fn style(f: &FaceInfo) -> u32 {
    if f.italic {
        2
    } else {
        0
    }
}

/// Axes standard d'une police statique : wght, wdth, ital, slnt.
fn axes(f: &FaceInfo) -> [AxisValue; 4] {
    [
        AxisValue {
            tag: WGHT,
            value: f.weight as f32,
        },
        AxisValue {
            tag: WDTH,
            value: 100.0,
        },
        AxisValue {
            tag: ITAL,
            value: f.italic as u32 as f32,
        },
        AxisValue {
            tag: SLNT,
            value: if f.italic { -12.0 } else { 0.0 },
        },
    ]
}

/// Écart entre une face et des valeurs d'axes demandées (plus petit = mieux).
fn axis_distance(f: &FaceInfo, wanted: &[AxisValue]) -> u32 {
    let mut d = 0u32;
    for w in wanted {
        match w.tag {
            WGHT => d += (f.weight as u32).abs_diff(w.value.clamp(1.0, 1000.0) as u32),
            ITAL if (w.value >= 0.5) != f.italic => d += 2000,
            SLNT if (w.value != 0.0) != f.italic => d += 1000,
            _ => {}
        }
    }
    d
}

/// Tri de DirectWrite pour une graisse et un style demandés.
fn by_weight_style(faces: &mut [FaceInfo], weight: u32, style_wanted: u32) {
    let italic = style_wanted != 0;
    faces.sort_by_key(|x| (x.italic != italic) as u32 * 2000 + (x.weight as u32).abs_diff(weight));
}

unsafe fn read_axes<'a>(p: *const AxisValue, n: u32) -> &'a [AxisValue] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}

/// DWRITE_FONT_PROPERTY_ID → valeur de la propriété pour une face.
fn property(f: &FaceInfo, id: u32) -> Option<String> {
    let regular = f.style.eq_ignore_ascii_case("Regular") || f.style.is_empty();
    match id {
        1 | 2 | 5 => Some(f.family.clone()),
        3 | 13 => Some(if f.style.is_empty() {
            "Regular".into()
        } else {
            f.style.clone()
        }),
        4 => Some(if regular {
            f.family.clone()
        } else {
            format!("{} {}", f.family, f.style)
        }),
        6 => Some(
            format!(
                "{}-{}",
                f.family,
                if regular { "Regular" } else { &f.style }
            )
            .replace(' ', ""),
        ),
        10 => Some(f.weight.to_string()),
        11 => Some("5".into()),
        12 => Some(style(f).to_string()),
        _ => None,
    }
}

unsafe fn matches_props(f: &FaceInfo, props: &[FontProperty], any: bool) -> bool {
    if props.is_empty() {
        return true;
    }
    let ok = |p: &FontProperty| {
        let want = read_wide(p.value);
        property(f, p.id).is_some_and(|v| v.eq_ignore_ascii_case(&want))
    };
    if any {
        props.iter().any(ok)
    } else {
        props.iter().all(ok)
    }
}

unsafe fn read_props<'a>(p: *const FontProperty, n: u32) -> &'a [FontProperty] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}

fn in_ranges(f: &FaceInfo, ranges: &[AxisRange], any: bool) -> bool {
    if ranges.is_empty() {
        return true;
    }
    let values = axes(f);
    let ok = |r: &AxisRange| {
        values
            .iter()
            .find(|v| v.tag == r.tag)
            .is_some_and(|v| v.value >= r.min && v.value <= r.max)
    };
    if any {
        ranges.iter().any(ok)
    } else {
        ranges.iter().all(ok)
    }
}

// --- Méthodes ajoutées par IDWriteFactory3 à 7 -----------------------------------------

/// IDWriteFactory2/3::CreateGlyphRunAnalysis : même rendu que la version
/// d'IDWriteFactory, en niveaux de gris quand l'appli le demande.
pub unsafe extern "win64" fn glyph_run_analysis3(
    this: *mut c_void,
    run: *const GlyphRun,
    transform: *const super::super::dwrite_glyphs::DwMatrix,
    mode: u32,
    measuring: u32,
    _grid_fit: u32,
    antialias: u32,
    x: f32,
    y: f32,
    out: *mut *mut c_void,
) -> i32 {
    // DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC_DOWNSAMPLED (7) se dessine
    // comme NATURAL_SYMMETRIC (5).
    let mode = if mode == 7 { 5 } else { mode };
    super::super::dwrite_glyphs::create_glyph_run_analysis_aa(
        this,
        run,
        1.0,
        transform,
        mode,
        measuring,
        x,
        y,
        antialias == 1,
        out,
    )
}

/// IDWriteFactory3::CreateCustomRenderingParams (IDWriteRenderingParams3).
pub unsafe extern "win64" fn custom_params3(
    _this: *mut c_void,
    gamma: f32,
    contrast: f32,
    gray_contrast: f32,
    cleartype: f32,
    geometry: u32,
    mode: u32,
    grid_fit: u32,
    out: *mut *mut c_void,
) -> i32 {
    super::fallback::new_params3(
        gamma,
        contrast,
        gray_contrast,
        cleartype,
        geometry,
        mode,
        grid_fit,
        out,
    )
}

/// CreateFontFaceReference(IDWriteFontFile, index, simulations).
pub unsafe extern "win64" fn face_reference_from_file(
    _this: *mut c_void,
    file: *mut c_void,
    index: u32,
    _sims: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if !is(file, FILE_VT.ptr()) {
        return E_INVALIDARG;
    }
    let path = Com::get(file as *mut FileObj).clone();
    reference_for(path, index, out)
}

unsafe fn reference_for(path: std::path::PathBuf, index: u32, out: *mut *mut c_void) -> i32 {
    let found = super::super::gdi32_fonts::parse_font_file(&path)
        .iter()
        .find(|f| f.index == index)
        .map(super::info);
    let f = match found {
        Some(f) => f,
        None if sfnt::load(&path, index).is_some() => FaceInfo {
            family: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            style: "Regular".into(),
            weight: 400,
            italic: false,
            path,
            index,
        },
        None => return DWRITE_E_FILEFORMAT,
    };
    put_ref(&f, out)
}

/// CreateFontFaceReference(chemin, date, index, simulations).
pub unsafe extern "win64" fn face_reference_from_path(
    _this: *mut c_void,
    path: *const u16,
    _time: *const u64,
    index: u32,
    _sims: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if path.is_null() {
        return E_INVALIDARG;
    }
    let p = crate::paths::to_linux(&read_wide(path));
    if !p.exists() {
        return 0x8007_0002u32 as i32;
    }
    reference_for(p, index, out)
}

/// IDWriteFactory6::CreateFontFaceReference (avec valeurs d'axes, ignorées :
/// les polices sont statiques).
pub unsafe extern "win64" fn face_reference6(
    this: *mut c_void,
    file: *mut c_void,
    index: u32,
    sims: u32,
    _axes: *const AxisValue,
    _n: u32,
    out: *mut *mut c_void,
) -> i32 {
    face_reference_from_file(this, file, index, sims, out)
}

/// GetSystemFontSet (IDWriteFactory3, 6 et 7).
pub unsafe extern "win64" fn system_font_set(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    put_set(all_faces(), out)
}
pub unsafe extern "win64" fn system_font_set6(
    _this: *mut c_void,
    _downloadable: i32,
    out: *mut *mut c_void,
) -> i32 {
    put_set(all_faces(), out)
}

/// CreateFontSetBuilder (IDWriteFactory3, 5 et 6).
pub unsafe extern "win64" fn font_set_builder(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(BUILDER_VT.ptr(), Mutex::new(Vec::<FaceInfo>::new())),
    )
}

/// CreateFontCollectionFromFontSet (IDWriteFactory3 et 6).
pub unsafe extern "win64" fn collection_from_set(
    _this: *mut c_void,
    set: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match faces_of_set(set) {
        Some(faces) => put_collection(faces, out),
        None => {
            *out = std::ptr::null_mut();
            E_INVALIDARG
        }
    }
}
pub unsafe extern "win64" fn collection_from_set6(
    this: *mut c_void,
    set: *mut c_void,
    _model: u32,
    out: *mut *mut c_void,
) -> i32 {
    collection_from_set(this, set, out)
}

/// GetSystemFontCollection (IDWriteFactory3, 6 et 7).
pub unsafe extern "win64" fn system_collection3(
    _this: *mut c_void,
    _downloadable: i32,
    out: *mut *mut c_void,
    _check: i32,
) -> i32 {
    put_collection(all_faces(), out)
}
pub unsafe extern "win64" fn system_collection6(
    _this: *mut c_void,
    _downloadable: i32,
    _model: u32,
    out: *mut *mut c_void,
) -> i32 {
    put_collection(all_faces(), out)
}

/// GetFontDownloadQueue : file toujours vide (aucune police distante).
pub unsafe extern "win64" fn download_queue(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(QUEUE_VT.ptr(), ()))
}

/// IDWriteFactory4::TranslateColorGlyphRun : aucune police en couleur.
pub unsafe extern "win64" fn translate_color4(
    _this: *mut c_void,
    _origin: Point,
    _run: *const c_void,
    _description: *const c_void,
    _formats: u32,
    _measuring: u32,
    _transform: *const c_void,
    _palette: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DWRITE_E_NOCOLOR
}

/// Origine de chaque glyphe : avances cumulées et décalages, de droite à
/// gauche pour un niveau bidi impair.
unsafe fn origins_of(run: &GlyphRun, origin: Point, m: Option<[f32; 4]>, out: *mut Point) {
    let rtl = run.bidi_level & 1 == 1;
    let dir = if rtl { -1.0 } else { 1.0 };
    let apply = |dx: f32, dy: f32| match m {
        Some([a, b, c, d]) => (dx * a + dy * c, dx * b + dy * d),
        None => (dx, dy),
    };
    let mut pen = 0.0f32;
    for i in 0..run.count as usize {
        let adv = if run.advances.is_null() {
            0.0
        } else {
            *run.advances.add(i)
        };
        if rtl {
            pen -= adv;
        }
        let (ox, oy) = if run.offsets.is_null() {
            (0.0, 0.0)
        } else {
            let o = *run.offsets.add(i);
            (dir * o.advance, -o.ascender)
        };
        let (x, y) = apply(pen + ox, oy);
        *out.add(i) = Point {
            x: origin.x + x,
            y: origin.y + y,
        };
        if !rtl {
            pen += adv;
        }
    }
}

/// IDWriteFactory4::ComputeGlyphOrigins (sans mode de mesure).
pub unsafe extern "win64" fn glyph_origins(
    _this: *mut c_void,
    run: *const GlyphRun,
    origin: Point,
    out: *mut Point,
) -> i32 {
    if run.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    origins_of(&*run, origin, None, out);
    S_OK
}

/// IDWriteFactory4::ComputeGlyphOrigins (mode de mesure et transformation).
pub unsafe extern "win64" fn glyph_origins_transformed(
    _this: *mut c_void,
    run: *const GlyphRun,
    _measuring: u32,
    origin: Point,
    transform: *const [f32; 6],
    out: *mut Point,
) -> i32 {
    if run.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let m = transform.as_ref().map(|t| [t[0], t[1], t[2], t[3]]);
    origins_of(&*run, origin, m, out);
    S_OK
}

/// IDWriteFactory5::AnalyzeContainerType : DWRITE_CONTAINER_TYPE_WOFF (1),
/// WOFF2 (2), sinon UNKNOWN (0).
pub unsafe extern "win64" fn container_type(_this: *mut c_void, data: *const u8, size: u32) -> u32 {
    if data.is_null() || size < 4 {
        return 0;
    }
    match std::slice::from_raw_parts(data, 4) {
        b"wOFF" => 1,
        b"wOF2" => 2,
        _ => 0,
    }
}

/// IDWriteFactory6::CreateTextFormat : graisse, style et chasse lus dans
/// les axes demandés.
pub unsafe extern "win64" fn text_format6(
    this: *mut c_void,
    family: *const u16,
    collection: *mut c_void,
    axis: *const AxisValue,
    n: u32,
    size: f32,
    locale: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    let mut weight = 400u32;
    let mut style_v = 0u32;
    let mut stretch = 5u32;
    for a in read_axes(axis, n) {
        match a.tag {
            WGHT => weight = a.value.clamp(1.0, 1000.0) as u32,
            ITAL if a.value >= 0.5 => style_v = 2,
            SLNT if a.value != 0.0 && style_v == 0 => style_v = 1,
            WDTH => {
                const W: [f32; 9] = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];
                stretch = W
                    .iter()
                    .enumerate()
                    .min_by(|x, y| (x.1 - a.value).abs().total_cmp(&(y.1 - a.value).abs()))
                    .map_or(5, |(i, _)| i as u32 + 1);
            }
            _ => {}
        }
    }
    create_text_format(
        this as *mut _,
        family,
        collection,
        weight,
        style_v,
        stretch,
        size,
        locale,
        out,
    )
}

// --- IDWriteFontSet 1 à 4 ---------------------------------------------------------------

unsafe extern "win64" fn set_qi(
    this: *mut SetObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            IID_FONT_SET,
            IID_FONT_SET1,
            IID_FONT_SET2,
            IID_FONT_SET3,
            IID_FONT_SET4,
        ],
    )
}
unsafe extern "win64" fn set_count(this: *mut SetObj) -> u32 {
    Com::get(this).len() as u32
}
unsafe fn set_item<'a>(this: *mut SetObj, i: u32) -> Option<&'a FaceInfo> {
    Com::get(this).get(i as usize)
}
unsafe extern "win64" fn set_reference(this: *mut SetObj, i: u32, out: *mut *mut c_void) -> i32 {
    match set_item(this, i) {
        Some(f) => put_ref(f, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
unsafe fn set_find(
    this: *mut SetObj,
    f: Option<FaceInfo>,
    index: *mut u32,
    exists: *mut i32,
) -> i32 {
    if index.is_null() || exists.is_null() {
        return E_POINTER;
    }
    let pos = f.and_then(|f| Com::get(this).iter().position(|x| same(x, &f)));
    *index = pos.map_or(u32::MAX, |p| p as u32);
    *exists = pos.is_some() as i32;
    S_OK
}
unsafe extern "win64" fn set_find_reference(
    this: *mut SetObj,
    reference: *mut c_void,
    index: *mut u32,
    exists: *mut i32,
) -> i32 {
    set_find(this, info_of(reference), index, exists)
}
unsafe extern "win64" fn set_find_face(
    this: *mut SetObj,
    face: *mut c_void,
    index: *mut u32,
    exists: *mut i32,
) -> i32 {
    set_find(this, info_of(face), index, exists)
}
/// Valeurs distinctes d'une propriété, dans l'ordre du jeu.
unsafe fn distinct_values(this: *mut SetObj, id: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    for f in Com::get(this) {
        if let Some(v) = property(f, id) {
            if !seen.iter().any(|(_, s)| s.eq_ignore_ascii_case(&v)) {
                seen.push(("en-us".into(), v));
            }
        }
    }
    put(out, Com::create(STRING_LIST_VT.ptr(), seen))
}
unsafe extern "win64" fn set_values(this: *mut SetObj, id: u32, out: *mut *mut c_void) -> i32 {
    distinct_values(this, id, out)
}
unsafe extern "win64" fn set_values_locales(
    this: *mut SetObj,
    id: u32,
    _locales: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    distinct_values(this, id, out)
}
unsafe extern "win64" fn set_value_of(
    this: *mut SetObj,
    i: u32,
    id: u32,
    exists: *mut i32,
    out: *mut *mut c_void,
) -> i32 {
    if exists.is_null() || out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    *exists = 0;
    let Some(f) = set_item(this, i) else {
        return E_INVALIDARG;
    };
    if let Some(v) = property(f, id) {
        *exists = 1;
        return put(out, new_strings(en(&v)));
    }
    S_OK
}
unsafe extern "win64" fn set_occurrences(
    this: *mut SetObj,
    prop: *const FontProperty,
    count: *mut u32,
) -> i32 {
    if prop.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    let props = std::slice::from_raw_parts(prop, 1);
    *count = Com::get(this)
        .iter()
        .filter(|f| matches_props(f, props, false))
        .count() as u32;
    S_OK
}
unsafe extern "win64" fn set_matching_wss(
    this: *mut SetObj,
    family: *const u16,
    weight: u32,
    _stretch: u32,
    style_wanted: u32,
    out: *mut *mut c_void,
) -> i32 {
    let faces = Com::get(this);
    let name = read_wide(family);
    let mut m: Vec<FaceInfo> = match resolve_family(faces, &name) {
        Some(n) => faces
            .iter()
            .filter(|f| family_matches(f, &n))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    by_weight_style(&mut m, weight, style_wanted);
    put_set(m, out)
}
unsafe extern "win64" fn set_matching_props(
    this: *mut SetObj,
    props: *const FontProperty,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    let props = read_props(props, n);
    let m = Com::get(this)
        .iter()
        .filter(|f| matches_props(f, props, false))
        .cloned()
        .collect();
    put_set(m, out)
}
unsafe extern "win64" fn set_matching_axes(
    this: *mut SetObj,
    prop: *const FontProperty,
    axis: *const AxisValue,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    let props = read_props(prop, (!prop.is_null()) as u32);
    let wanted = read_axes(axis, n);
    let mut m: Vec<FaceInfo> = Com::get(this)
        .iter()
        .filter(|f| matches_props(f, props, false))
        .cloned()
        .collect();
    m.sort_by_key(|f| axis_distance(f, wanted));
    put_set(m, out)
}
unsafe extern "win64" fn set_first_resources(this: *mut SetObj, out: *mut *mut c_void) -> i32 {
    put_set(Com::get(this).clone(), out)
}
unsafe extern "win64" fn set_filtered_indices_list(
    this: *mut SetObj,
    indices: *const u32,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    if indices.is_null() && n > 0 {
        return E_INVALIDARG;
    }
    let faces = Com::get(this);
    let mut m = Vec::new();
    for i in 0..n as usize {
        match faces.get(*indices.add(i) as usize) {
            Some(f) => m.push(f.clone()),
            None => return E_INVALIDARG,
        }
    }
    put_set(m, out)
}
unsafe extern "win64" fn set_filtered_ranges(
    this: *mut SetObj,
    ranges: *const AxisRange,
    n: u32,
    any: i32,
    out: *mut *mut c_void,
) -> i32 {
    let r = if ranges.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(ranges, n as usize)
    };
    let m = Com::get(this)
        .iter()
        .filter(|f| in_ranges(f, r, any != 0))
        .cloned()
        .collect();
    put_set(m, out)
}
unsafe extern "win64" fn set_filtered_props(
    this: *mut SetObj,
    props: *const FontProperty,
    n: u32,
    any: i32,
    out: *mut *mut c_void,
) -> i32 {
    let props = read_props(props, n);
    let m = Com::get(this)
        .iter()
        .filter(|f| matches_props(f, props, any != 0))
        .cloned()
        .collect();
    put_set(m, out)
}
unsafe fn write_indices(found: Vec<u32>, out: *mut u32, max: u32, actual: *mut u32) -> i32 {
    if actual.is_null() {
        return E_POINTER;
    }
    *actual = found.len() as u32;
    if found.len() > max as usize || (out.is_null() && !found.is_empty()) {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    for (i, v) in found.into_iter().enumerate() {
        *out.add(i) = v;
    }
    S_OK
}
unsafe extern "win64" fn set_indices_ranges(
    this: *mut SetObj,
    ranges: *const AxisRange,
    n: u32,
    any: i32,
    out: *mut u32,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let r = if ranges.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(ranges, n as usize)
    };
    let found = Com::get(this)
        .iter()
        .enumerate()
        .filter(|(_, f)| in_ranges(f, r, any != 0))
        .map(|(i, _)| i as u32)
        .collect();
    write_indices(found, out, max, actual)
}
unsafe extern "win64" fn set_indices_props(
    this: *mut SetObj,
    props: *const FontProperty,
    n: u32,
    any: i32,
    out: *mut u32,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let props = read_props(props, n);
    let found = Com::get(this)
        .iter()
        .enumerate()
        .filter(|(_, f)| matches_props(f, props, any != 0))
        .map(|(i, _)| i as u32)
        .collect();
    write_indices(found, out, max, actual)
}
unsafe fn write_ranges(
    found: &[AxisRange],
    out: *mut AxisRange,
    max: u32,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() {
        return E_POINTER;
    }
    *actual = found.len() as u32;
    if found.len() > max as usize || out.is_null() {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(found.as_ptr(), out, found.len());
    S_OK
}
fn face_ranges(f: &FaceInfo) -> Vec<AxisRange> {
    axes(f)
        .iter()
        .map(|a| AxisRange {
            tag: a.tag,
            min: a.value,
            max: a.value,
        })
        .collect()
}
unsafe extern "win64" fn set_axis_ranges_of(
    this: *mut SetObj,
    i: u32,
    out: *mut AxisRange,
    max: u32,
    actual: *mut u32,
) -> i32 {
    match set_item(this, i) {
        Some(f) => write_ranges(&face_ranges(f), out, max, actual),
        None => E_INVALIDARG,
    }
}
unsafe extern "win64" fn set_axis_ranges(
    this: *mut SetObj,
    out: *mut AxisRange,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let mut all: Vec<AxisRange> = Vec::new();
    for f in Com::get(this) {
        for r in face_ranges(f) {
            match all.iter_mut().find(|a| a.tag == r.tag) {
                Some(a) => {
                    a.min = a.min.min(r.min);
                    a.max = a.max.max(r.max);
                }
                None => all.push(r),
            }
        }
    }
    write_ranges(&all, out, max, actual)
}
unsafe extern "win64" fn set_create_face(this: *mut SetObj, i: u32, out: *mut *mut c_void) -> i32 {
    match set_item(this, i) {
        Some(f) => put_face(f, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn set_locality(_this: *mut SetObj, _i: u32) -> u32 {
    LOCALITY_LOCAL
}
pub(super) unsafe extern "win64" fn no_event(_this: *mut c_void) -> usize {
    0
}
unsafe extern "win64" fn set_source_type(this: *mut SetObj, i: u32) -> u32 {
    // DWRITE_FONT_SOURCE_TYPE_PER_MACHINE pour les polices du système.
    match set_item(this, i) {
        Some(f) if f.path.starts_with("/usr/share/fonts") => 1,
        _ => 0,
    }
}
unsafe extern "win64" fn set_source_name_len(_this: *mut SetObj, _i: u32) -> u32 {
    0
}
unsafe extern "win64" fn set_source_name(
    _this: *mut SetObj,
    _i: u32,
    buf: *mut u16,
    size: u32,
) -> i32 {
    if buf.is_null() || size == 0 {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    *buf = 0;
    S_OK
}
/// IDWriteFontSet4::ConvertWeightStretchStyleToFontAxisValues : les cinq
/// axes standard (wght, wdth, slnt, opsz, ital), ceux fournis par l'appli
/// gardant leur valeur.
unsafe extern "win64" fn set_convert_wss(
    _this: *mut SetObj,
    input: *const AxisValue,
    n: u32,
    weight: u32,
    stretch: u32,
    style_v: u32,
    size: f32,
    out: *mut AxisValue,
) -> u32 {
    if out.is_null() {
        return 0;
    }
    const W: [f32; 9] = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];
    let mut v = [
        AxisValue {
            tag: WGHT,
            value: weight as f32,
        },
        AxisValue {
            tag: WDTH,
            value: W[(stretch.clamp(1, 9) - 1) as usize],
        },
        AxisValue {
            tag: SLNT,
            value: if style_v == 0 { 0.0 } else { -12.0 },
        },
        AxisValue {
            tag: OPSZ,
            value: size * 72.0 / 96.0,
        },
        AxisValue {
            tag: ITAL,
            value: (style_v == 2) as u32 as f32,
        },
    ];
    for a in read_axes(input, n) {
        if let Some(x) = v.iter_mut().find(|x| x.tag == a.tag) {
            x.value = a.value;
        }
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len());
    v.len() as u32
}
unsafe extern "win64" fn set_matching4(
    this: *mut SetObj,
    family: *const u16,
    axis: *const AxisValue,
    n: u32,
    _sims: u32,
    out: *mut *mut c_void,
) -> i32 {
    let faces = Com::get(this);
    let name = read_wide(family);
    let wanted = read_axes(axis, n);
    let mut m: Vec<FaceInfo> = match resolve_family(faces, &name) {
        Some(n) => faces
            .iter()
            .filter(|f| family_matches(f, &n))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    m.sort_by_key(|f| axis_distance(f, wanted));
    put_set(m, out)
}

pub static SET_VT: VTable<32> = VTable([
    set_qi as *const (),
    add_ref::<Vec<FaceInfo>> as *const (),
    release::<Vec<FaceInfo>> as *const (),
    set_count as *const (),
    set_reference as *const (),
    set_find_reference as *const (),
    set_find_face as *const (),
    set_values as *const (),
    set_values_locales as *const (),
    set_value_of as *const (),
    set_occurrences as *const (),
    set_matching_wss as *const (),
    set_matching_props as *const (),
    set_matching_axes as *const (), // IDWriteFontSet1
    set_first_resources as *const (),
    set_filtered_indices_list as *const (),
    set_filtered_ranges as *const (),
    set_filtered_props as *const (),
    set_indices_ranges as *const (),
    set_indices_props as *const (),
    set_axis_ranges_of as *const (),
    set_axis_ranges as *const (),
    set_reference as *const (), // GetFontFaceReference (IDWriteFontFaceReference1)
    unsupported!("IDWriteFontSet1::CreateFontResource"),
    set_create_face as *const (),
    set_locality as *const (),
    no_event as *const (),        // IDWriteFontSet2::GetExpirationEvent
    set_source_type as *const (), // IDWriteFontSet3
    set_source_name_len as *const (),
    set_source_name as *const (),
    set_convert_wss as *const (), // IDWriteFontSet4
    set_matching4 as *const (),
]);

// --- IDWriteStringList ---------------------------------------------------------------------

unsafe extern "win64" fn string_list_qi(
    this: *mut Com<Vec<(String, String)>>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_STRING_LIST])
}
static STRING_LIST_VT: VTable<8> = VTable([
    string_list_qi as *const (),
    add_ref::<Vec<(String, String)>> as *const (),
    release::<Vec<(String, String)>> as *const (),
    strings_count as *const (),
    strings_locale_len as *const (),
    strings_locale as *const (),
    strings_string_len as *const (),
    strings_string as *const (),
]);

// --- IDWriteFontFaceReference et IDWriteFontFaceReference1 ----------------------------

unsafe extern "win64" fn ref_qi(
    this: *mut RefObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FACE_REFERENCE, IID_FACE_REFERENCE1])
}
unsafe extern "win64" fn ref_face(this: *mut RefObj, out: *mut *mut c_void) -> i32 {
    put_face(Com::get(this), out)
}
unsafe extern "win64" fn ref_face_sims(
    this: *mut RefObj,
    _sims: u32,
    out: *mut *mut c_void,
) -> i32 {
    put_face(Com::get(this), out)
}
unsafe extern "win64" fn ref_equals(this: *mut RefObj, other: *mut c_void) -> i32 {
    info_of(other).is_some_and(|o| same(Com::get(this), &o)) as i32
}
unsafe extern "win64" fn ref_index(this: *mut RefObj) -> u32 {
    Com::get(this).index
}
unsafe extern "win64" fn ref_sims(_this: *mut RefObj) -> u32 {
    0
}
unsafe extern "win64" fn ref_file(this: *mut RefObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(FILE_VT.ptr(), Com::get(this).path.clone()))
}
unsafe extern "win64" fn ref_size(this: *mut RefObj) -> u64 {
    std::fs::metadata(&Com::get(this).path).map_or(0, |m| m.len())
}
unsafe extern "win64" fn ref_time(this: *mut RefObj, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = file_time(&Com::get(this).path);
    S_OK
}
unsafe extern "win64" fn ref_locality(_this: *mut RefObj) -> u32 {
    LOCALITY_LOCAL
}
unsafe extern "win64" fn ref_enqueue(_this: *mut RefObj) -> i32 {
    S_OK
}
unsafe extern "win64" fn ref_enqueue2(_this: *mut RefObj, _p: *const c_void, _n: u64) -> i32 {
    S_OK
}
unsafe extern "win64" fn ref_axis_count(_this: *mut RefObj) -> u32 {
    0
}
unsafe extern "win64" fn ref_axis_values(_this: *mut RefObj, _v: *mut AxisValue, _n: u32) -> i32 {
    S_OK
}

static REF_VT: VTable<20> = VTable([
    ref_qi as *const (),
    add_ref::<FaceInfo> as *const (),
    release::<FaceInfo> as *const (),
    ref_face as *const (),
    ref_face_sims as *const (),
    ref_equals as *const (),
    ref_index as *const (),
    ref_sims as *const (),
    ref_file as *const (),
    ref_size as *const (), // GetLocalFileSize
    ref_size as *const (), // GetFileSize
    ref_time as *const (),
    ref_locality as *const (),
    ref_enqueue as *const (),  // EnqueueFontDownloadRequest
    ref_enqueue2 as *const (), // EnqueueCharacterDownloadRequest
    ref_enqueue2 as *const (), // EnqueueGlyphDownloadRequest
    ref_enqueue2 as *const (), // EnqueueFileFragmentDownloadRequest
    ref_face as *const (),     // IDWriteFontFaceReference1::CreateFontFace
    ref_axis_count as *const (),
    ref_axis_values as *const (),
]);

// --- IDWriteFontSetBuilder 0 à 2 -----------------------------------------------------------

unsafe extern "win64" fn builder_qi(
    this: *mut BuilderObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[IID_SET_BUILDER, IID_SET_BUILDER1, IID_SET_BUILDER2],
    )
}
unsafe fn builder_push(this: *mut BuilderObj, faces: impl IntoIterator<Item = FaceInfo>) {
    let mut v = super::super::lock(Com::get(this));
    for f in faces {
        if !v.iter().any(|x| same(x, &f)) {
            v.push(f);
        }
    }
}
unsafe extern "win64" fn builder_add_ref_props(
    this: *mut BuilderObj,
    reference: *mut c_void,
    _props: *const FontProperty,
    _n: u32,
) -> i32 {
    builder_add_ref(this, reference)
}
unsafe extern "win64" fn builder_add_ref(this: *mut BuilderObj, reference: *mut c_void) -> i32 {
    match info_of(reference) {
        Some(f) => {
            builder_push(this, [f]);
            S_OK
        }
        None => E_INVALIDARG,
    }
}
unsafe extern "win64" fn builder_add_set(this: *mut BuilderObj, set: *mut c_void) -> i32 {
    match faces_of_set(set) {
        Some(faces) => {
            builder_push(this, faces);
            S_OK
        }
        None => E_INVALIDARG,
    }
}
unsafe extern "win64" fn builder_create(this: *mut BuilderObj, out: *mut *mut c_void) -> i32 {
    let faces = super::super::lock(Com::get(this)).clone();
    put_set(faces, out)
}
unsafe fn builder_add_path(this: *mut BuilderObj, path: std::path::PathBuf) -> i32 {
    let faces: Vec<FaceInfo> = super::super::gdi32_fonts::parse_font_file(&path)
        .iter()
        .filter(|f| !f.cff)
        .map(super::info)
        .collect();
    if faces.is_empty() {
        return DWRITE_E_FILEFORMAT;
    }
    builder_push(this, faces);
    S_OK
}
unsafe extern "win64" fn builder_add_file(this: *mut BuilderObj, file: *mut c_void) -> i32 {
    if !is(file, FILE_VT.ptr()) {
        return E_INVALIDARG;
    }
    builder_add_path(this, Com::get(file as *mut FileObj).clone())
}
unsafe extern "win64" fn builder_add_font(
    this: *mut BuilderObj,
    file: *mut c_void,
    index: u32,
    _sims: u32,
    _values: *const AxisValue,
    _nv: u32,
    _ranges: *const AxisRange,
    _nr: u32,
    _props: *const FontProperty,
    _np: u32,
) -> i32 {
    if !is(file, FILE_VT.ptr()) {
        return E_INVALIDARG;
    }
    let path = Com::get(file as *mut FileObj).clone();
    let mut tmp = std::ptr::null_mut();
    let hr = reference_for(path, index, &mut tmp);
    if hr != S_OK {
        return hr;
    }
    let f = Com::get(tmp as *mut RefObj).clone();
    release::<FaceInfo>(tmp as *mut RefObj);
    builder_push(this, [f]);
    S_OK
}
unsafe extern "win64" fn builder_add_file_path(this: *mut BuilderObj, path: *const u16) -> i32 {
    if path.is_null() {
        return E_INVALIDARG;
    }
    let p = crate::paths::to_linux(&read_wide(path));
    if !p.exists() {
        return 0x8007_0002u32 as i32;
    }
    builder_add_path(this, p)
}

static BUILDER_VT: VTable<10> = VTable([
    builder_qi as *const (),
    add_ref::<Mutex<Vec<FaceInfo>>> as *const (),
    release::<Mutex<Vec<FaceInfo>>> as *const (),
    builder_add_ref_props as *const (),
    builder_add_ref as *const (),
    builder_add_set as *const (),
    builder_create as *const (),
    builder_add_file as *const (), // IDWriteFontSetBuilder1
    builder_add_font as *const (), // IDWriteFontSetBuilder2
    builder_add_file_path as *const (),
]);

// --- IDWriteFontDownloadQueue (toujours vide) -------------------------------------------

unsafe extern "win64" fn queue_qi(
    this: *mut Com<()>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_DOWNLOAD_QUEUE])
}
unsafe extern "win64" fn queue_add_listener(
    _this: *mut c_void,
    _listener: *mut c_void,
    token: *mut u32,
) -> i32 {
    if token.is_null() {
        return E_INVALIDARG;
    }
    *token = 1;
    S_OK
}
unsafe extern "win64" fn queue_remove_listener(_this: *mut c_void, _token: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn queue_is_empty(_this: *mut c_void) -> i32 {
    1
}
unsafe extern "win64" fn queue_begin(_this: *mut c_void, _ctx: *mut c_void) -> i32 {
    S_FALSE
}
unsafe extern "win64" fn queue_cancel(_this: *mut c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn queue_generation(_this: *mut c_void) -> u64 {
    0
}
static QUEUE_VT: VTable<9> = VTable([
    queue_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    queue_add_listener as *const (),
    queue_remove_listener as *const (),
    queue_is_empty as *const (),
    queue_begin as *const (),
    queue_cancel as *const (),
    queue_generation as *const (),
]);

// --- IDWriteFontCollection1 à 3 (suite de COLLECTION_VT) --------------------------------

pub(super) const COLLECTION_IIDS: [Guid; 3] = [
    IID_FONT_COLLECTION1,
    IID_FONT_COLLECTION2,
    IID_FONT_COLLECTION3,
];

unsafe fn collection_faces(this: *mut CollectionObj) -> Vec<FaceInfo> {
    Com::get(this)
        .iter()
        .flat_map(|f| f.faces.iter().cloned())
        .collect()
}
pub(super) unsafe extern "win64" fn collection_font_set(
    this: *mut CollectionObj,
    out: *mut *mut c_void,
) -> i32 {
    put_set(collection_faces(this), out)
}
pub(super) unsafe extern "win64" fn collection_matching(
    this: *mut CollectionObj,
    family: *const u16,
    axis: *const AxisValue,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let faces = collection_faces(this);
    let name = read_wide(family);
    let wanted = read_axes(axis, n);
    let mut m: Vec<FaceInfo> = match resolve_family(&faces, &name) {
        Some(n) => faces
            .into_iter()
            .filter(|f| family_matches(f, &n))
            .collect(),
        None => Vec::new(),
    };
    m.sort_by_key(|f| axis_distance(f, wanted));
    put(out, Com::create(LIST_VT.ptr(), m))
}
pub(super) unsafe extern "win64" fn collection_model(_this: *mut CollectionObj) -> u32 {
    MODEL_WSS
}

// --- IDWriteFontFamily1 et 2 (suite de FAMILY_VT) ---------------------------------------

pub(super) const FAMILY_IIDS: [Guid; 2] = [IID_FONT_FAMILY1, IID_FONT_FAMILY2];

pub(super) unsafe extern "win64" fn family_locality(_this: *mut FamilyObj, _i: u32) -> u32 {
    LOCALITY_LOCAL
}
pub(super) unsafe extern "win64" fn family_reference(
    this: *mut FamilyObj,
    i: u32,
    out: *mut *mut c_void,
) -> i32 {
    match Com::get(this).faces.get(i as usize) {
        Some(f) => put_ref(f, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
pub(super) unsafe extern "win64" fn family_matching_axes(
    this: *mut FamilyObj,
    axis: *const AxisValue,
    n: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let wanted = read_axes(axis, n);
    let mut m = Com::get(this).faces.clone();
    m.sort_by_key(|f| axis_distance(f, wanted));
    put(out, Com::create(LIST_VT.ptr(), m))
}
pub(super) unsafe extern "win64" fn family_font_set(
    this: *mut FamilyObj,
    out: *mut *mut c_void,
) -> i32 {
    put_set(Com::get(this).faces.clone(), out)
}

// --- IDWriteFontList 0 à 2 ------------------------------------------------------------------

unsafe extern "win64" fn list_qi(
    this: *mut ListObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[super::IID_FONT_LIST, IID_FONT_LIST1, IID_FONT_LIST2],
    )
}
unsafe extern "win64" fn list_collection(_this: *mut ListObj, out: *mut *mut c_void) -> i32 {
    put_collection(all_faces(), out)
}
unsafe extern "win64" fn list_count(this: *mut ListObj) -> u32 {
    Com::get(this).len() as u32
}
unsafe extern "win64" fn list_font(this: *mut ListObj, i: u32, out: *mut *mut c_void) -> i32 {
    match Com::get(this).get(i as usize) {
        Some(f) => put_font(f, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn list_locality(_this: *mut ListObj, _i: u32) -> u32 {
    LOCALITY_LOCAL
}
unsafe extern "win64" fn list_reference(this: *mut ListObj, i: u32, out: *mut *mut c_void) -> i32 {
    match Com::get(this).get(i as usize) {
        Some(f) => put_ref(f, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn list_font_set(this: *mut ListObj, out: *mut *mut c_void) -> i32 {
    put_set(Com::get(this).clone(), out)
}
static LIST_VT: VTable<10> = VTable([
    list_qi as *const (),
    add_ref::<Vec<FaceInfo>> as *const (),
    release::<Vec<FaceInfo>> as *const (),
    list_collection as *const (),
    list_count as *const (),
    list_font as *const (),
    list_locality as *const (), // IDWriteFontList1
    list_font as *const (),
    list_reference as *const (),
    list_font_set as *const (), // IDWriteFontList2
]);

// --- IDWriteFont1 à 3 (suite de FONT_VT) ------------------------------------------------

pub(super) const FONT_IIDS: [Guid; 2] = [IID_FONT1, IID_FONT2];

/// DWRITE_FONT_METRICS1 d'une police : boîte des glyphes (table head),
/// indices et exposants (table OS/2), sinon des valeurs proportionnelles.
fn metrics1(font: &sfnt::Font) -> FontMetrics1 {
    let base = FontMetrics::from(&font.metrics);
    let upm = base.units_per_em as i32;
    let pc = |v: i32| (upm * v / 100) as i16;
    let mut m = FontMetrics1 {
        base,
        glyph_box: [0, -(base.descent as i16), upm as i16, base.ascent as i16],
        subscript_position: [0, pc(14)],
        subscript_size: [pc(65), pc(60)],
        superscript_position: [0, pc(48)],
        superscript_size: [pc(65), pc(60)],
        has_typographic_metrics: 0,
    };
    let be = |b: &[u8], at: usize| -> Option<i16> {
        Some(i16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
    };
    if let Some(head) = font.raw_table(b"head") {
        if let (Some(x0), Some(y0), Some(x1), Some(y1)) =
            (be(head, 36), be(head, 38), be(head, 40), be(head, 42))
        {
            m.glyph_box = [x0, y0, x1, y1];
        }
    }
    if let Some(os2) = font.raw_table(b"OS/2") {
        let g = |at| be(os2, at);
        if let (Some(sx), Some(sy), Some(_), Some(oy)) = (g(10), g(12), g(14), g(16)) {
            m.subscript_size = [sx, sy];
            m.subscript_position = [g(14).unwrap_or(0), oy];
        }
        if let (Some(sx), Some(sy), Some(ox), Some(oy)) = (g(18), g(20), g(22), g(24)) {
            m.superscript_size = [sx, sy];
            m.superscript_position = [ox, oy];
        }
        // fsSelection, bit 7 USE_TYPO_METRICS.
        if let Some(sel) = g(62) {
            m.has_typographic_metrics = (sel as u16 & 0x80 != 0) as i32;
        }
    }
    m
}

fn panose(font: &sfnt::Font) -> [u8; 10] {
    let mut p = [0u8; 10];
    if let Some(b) = font.raw_table(b"OS/2").and_then(|t| t.get(32..42)) {
        p.copy_from_slice(b);
    }
    p
}

fn monospaced(font: &sfnt::Font) -> bool {
    font.raw_table(b"post")
        .and_then(|t| t.get(12..16))
        .is_some_and(|b| b != [0, 0, 0, 0])
}

/// Plages Unicode couvertes (plan multilingue de base et premier plan
/// complémentaire).
fn unicode_ranges(font: &sfnt::Font) -> Vec<UnicodeRange> {
    let mut out: Vec<UnicodeRange> = Vec::new();
    for ch in 0..0x2_0000u32 {
        if (0xD800..0xE000).contains(&ch) || font.glyph_index(ch) == 0 {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.last + 1 == ch => r.last = ch,
            _ => out.push(UnicodeRange {
                first: ch,
                last: ch,
            }),
        }
    }
    out
}

unsafe fn write_unicode_ranges(
    font: Option<Arc<sfnt::Font>>,
    max: u32,
    out: *mut UnicodeRange,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() {
        return E_INVALIDARG;
    }
    let r = font.map(|f| unicode_ranges(&f)).unwrap_or_default();
    *actual = r.len() as u32;
    if r.len() > max as usize || (out.is_null() && !r.is_empty()) {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(r.as_ptr(), out, r.len());
    S_OK
}

pub(super) unsafe extern "win64" fn font_metrics1(this: *mut FontObj, out: *mut FontMetrics1) {
    if let (Some(f), false) = (load(Com::get(this)), out.is_null()) {
        *out = metrics1(&f);
    }
}
pub(super) unsafe extern "win64" fn font_panose(this: *mut FontObj, out: *mut [u8; 10]) {
    if !out.is_null() {
        *out = load(Com::get(this)).map(|f| panose(&f)).unwrap_or_default();
    }
}
pub(super) unsafe extern "win64" fn font_unicode_ranges(
    this: *mut FontObj,
    max: u32,
    out: *mut UnicodeRange,
    actual: *mut u32,
) -> i32 {
    write_unicode_ranges(load(Com::get(this)), max, out, actual)
}
pub(super) unsafe extern "win64" fn font_monospaced(this: *mut FontObj) -> i32 {
    load(Com::get(this)).is_some_and(|f| monospaced(&f)) as i32
}
pub(super) unsafe extern "win64" fn get_false(_this: *mut c_void) -> i32 {
    0
}
pub(super) unsafe extern "win64" fn font_create_face3(
    this: *mut FontObj,
    out: *mut *mut c_void,
) -> i32 {
    put_face(Com::get(this), out)
}
pub(super) unsafe extern "win64" fn font_equals(this: *mut FontObj, other: *mut c_void) -> i32 {
    info_of(other).is_some_and(|o| same(Com::get(this), &o)) as i32
}
pub(super) unsafe extern "win64" fn font_reference(
    this: *mut FontObj,
    out: *mut *mut c_void,
) -> i32 {
    put_ref(Com::get(this), out)
}
pub(super) unsafe extern "win64" fn font_has_char3(this: *mut FontObj, ch: u32) -> i32 {
    load(Com::get(this)).is_some_and(|f| f.glyph_index(ch) != 0) as i32
}
pub(super) unsafe extern "win64" fn font_locality(_this: *mut FontObj) -> u32 {
    LOCALITY_LOCAL
}

// --- IDWriteFontFace1 à 6 (suite de FACE_VT) --------------------------------------------

unsafe fn face<'a>(this: *mut FaceObj) -> &'a Face {
    Com::get(this)
}

pub(super) unsafe extern "win64" fn face_metrics1(this: *mut FaceObj, out: *mut FontMetrics1) {
    if !out.is_null() {
        *out = metrics1(&face(this).font);
    }
}
pub(super) unsafe extern "win64" fn face_gdi_metrics1(
    this: *mut FaceObj,
    _em: f32,
    _ppd: f32,
    _m: *const c_void,
    out: *mut FontMetrics1,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    face_metrics1(this, out);
    S_OK
}
pub(super) unsafe extern "win64" fn face_caret(this: *mut FaceObj, out: *mut CaretMetrics) {
    if out.is_null() {
        return;
    }
    let italic = face(this).info.italic;
    let upm = face(this).font.metrics.units_per_em as i16;
    // hhea : caretSlopeRise, caretSlopeRun, caretOffset.
    let be = |b: &[u8], at: usize| -> Option<i16> {
        Some(i16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
    };
    let hhea = face(this).font.raw_table(b"hhea");
    *out = match hhea.and_then(|h| Some((be(h, 18)?, be(h, 20)?, be(h, 22)?))) {
        Some((rise, run, offset)) if rise != 0 => CaretMetrics {
            slope_rise: rise,
            slope_run: run,
            offset,
        },
        _ => CaretMetrics {
            slope_rise: upm,
            slope_run: if italic { upm / 5 } else { 0 },
            offset: 0,
        },
    };
}
pub(super) unsafe extern "win64" fn face_unicode_ranges(
    this: *mut FaceObj,
    max: u32,
    out: *mut UnicodeRange,
    actual: *mut u32,
) -> i32 {
    write_unicode_ranges(Some(face(this).font.clone()), max, out, actual)
}
pub(super) unsafe extern "win64" fn face_monospaced(this: *mut FaceObj) -> i32 {
    monospaced(&face(this).font) as i32
}
pub(super) unsafe extern "win64" fn face_design_advances(
    this: *mut FaceObj,
    count: u32,
    indices: *const u16,
    out: *mut i32,
    _sideways: i32,
) -> i32 {
    if (indices.is_null() || out.is_null()) && count > 0 {
        return E_INVALIDARG;
    }
    let f = &face(this).font;
    for i in 0..count as usize {
        *out.add(i) = f.advance(*indices.add(i)) as i32;
    }
    S_OK
}
pub(super) unsafe extern "win64" fn face_gdi_advances(
    this: *mut FaceObj,
    _em: f32,
    _ppd: f32,
    _m: *const c_void,
    _natural: i32,
    sideways: i32,
    count: u32,
    indices: *const u16,
    out: *mut i32,
) -> i32 {
    face_design_advances(this, count, indices, out, sideways)
}
pub(super) unsafe extern "win64" fn face_kerning(
    _this: *mut FaceObj,
    count: u32,
    _indices: *const u16,
    out: *mut i32,
) -> i32 {
    if out.is_null() && count > 0 {
        return E_INVALIDARG;
    }
    for i in 0..count as usize {
        *out.add(i) = 0;
    }
    S_OK
}
pub(super) unsafe extern "win64" fn face_recommended1(
    _this: *mut FaceObj,
    _em: f32,
    _dpi_x: f32,
    _dpi_y: f32,
    _m: *const c_void,
    _sideways: i32,
    _threshold: u32,
    _measuring: u32,
    mode: *mut u32,
) -> i32 {
    if mode.is_null() {
        return E_INVALIDARG;
    }
    *mode = 5;
    S_OK
}
pub(super) unsafe extern "win64" fn face_vertical_variants(
    _this: *mut FaceObj,
    count: u32,
    nominal: *const u16,
    out: *mut u16,
) -> i32 {
    if (nominal.is_null() || out.is_null()) && count > 0 {
        return E_INVALIDARG;
    }
    std::ptr::copy(nominal, out, count as usize);
    S_OK
}
pub(super) unsafe extern "win64" fn get_zero(_this: *mut c_void) -> u32 {
    0
}
pub(super) unsafe extern "win64" fn face_palette_entries(
    _this: *mut FaceObj,
    _palette: u32,
    _first: u32,
    _count: u32,
    _out: *mut c_void,
) -> i32 {
    DWRITE_E_NOCOLOR
}
pub(super) unsafe extern "win64" fn face_recommended2(
    _this: *mut FaceObj,
    _em: f32,
    _dpi_x: f32,
    _dpi_y: f32,
    _m: *const c_void,
    _sideways: i32,
    _threshold: u32,
    _measuring: u32,
    _params: *mut c_void,
    mode: *mut u32,
    grid_fit: *mut u32,
) -> i32 {
    if mode.is_null() || grid_fit.is_null() {
        return E_INVALIDARG;
    }
    *mode = 5;
    *grid_fit = 1; // DWRITE_GRID_FIT_MODE_DISABLED
    S_OK
}
pub(super) unsafe extern "win64" fn face_reference(
    this: *mut FaceObj,
    out: *mut *mut c_void,
) -> i32 {
    put_ref(&face(this).info, out)
}
pub(super) unsafe extern "win64" fn face_panose(this: *mut FaceObj, out: *mut [u8; 10]) {
    if !out.is_null() {
        *out = panose(&face(this).font);
    }
}
pub(super) unsafe extern "win64" fn face_weight(this: *mut FaceObj) -> u32 {
    face(this).info.weight as u32
}
pub(super) unsafe extern "win64" fn face_stretch(_this: *mut FaceObj) -> u32 {
    5
}
pub(super) unsafe extern "win64" fn face_style(this: *mut FaceObj) -> u32 {
    style(&face(this).info)
}
pub(super) unsafe extern "win64" fn face_family_names(
    this: *mut FaceObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_strings(en(&face(this).info.family)))
}
pub(super) unsafe extern "win64" fn face_face_names(
    this: *mut FaceObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = property(&face(this).info, 3).unwrap_or_default();
    put(out, new_strings(en(&s)))
}
pub(super) unsafe extern "win64" fn face_info_strings(
    _this: *mut FaceObj,
    _id: u32,
    out: *mut *mut c_void,
    exists: *mut i32,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    if !exists.is_null() {
        *exists = 0;
    }
    S_OK
}
pub(super) unsafe extern "win64" fn face_has_char(this: *mut FaceObj, ch: u32) -> i32 {
    (face(this).font.glyph_index(ch) != 0) as i32
}
pub(super) unsafe extern "win64" fn face_char_local(_this: *mut FaceObj, _v: u32) -> i32 {
    1
}
pub(super) unsafe extern "win64" fn face_all_local(
    _this: *mut FaceObj,
    _items: *const c_void,
    _count: u32,
    _enqueue: i32,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 1;
    S_OK
}
/// DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE : contours TrueType seulement.
pub(super) unsafe extern "win64" fn face_image_formats_of(
    _this: *mut FaceObj,
    _glyph: u16,
    _first: u32,
    _last: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 1;
    S_OK
}
pub(super) unsafe extern "win64" fn face_image_formats(_this: *mut FaceObj) -> u32 {
    1
}
pub(super) unsafe extern "win64" fn nothing1(_this: *mut c_void, _p: *mut c_void) {}
pub(super) unsafe extern "win64" fn face_axis_values(
    _this: *mut FaceObj,
    _values: *mut AxisValue,
    _n: u32,
) -> i32 {
    S_OK
}
pub(super) unsafe extern "win64" fn face_equals(this: *mut FaceObj, other: *mut c_void) -> i32 {
    info_of(other).is_some_and(|o| same(&face(this).info, &o)) as i32
}
pub(super) unsafe extern "win64" fn face_family_names6(
    this: *mut FaceObj,
    _model: u32,
    out: *mut *mut c_void,
) -> i32 {
    face_family_names(this, out)
}
pub(super) unsafe extern "win64" fn face_face_names6(
    this: *mut FaceObj,
    _model: u32,
    out: *mut *mut c_void,
) -> i32 {
    face_face_names(this, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> FaceInfo {
        let dir = std::env::temp_dir().join(format!("winlux-dwsets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Tiny.ttf");
        std::fs::write(&path, sfnt::tests::tiny_font()).unwrap();
        FaceInfo {
            family: "Winlux Tiny".into(),
            style: "Bold".into(),
            weight: 700,
            italic: false,
            path,
            index: 0,
        }
    }

    #[test]
    fn tables_have_windows_sizes() {
        let ptr = std::mem::size_of::<usize>();
        assert_eq!(std::mem::size_of_val(&SET_VT), 32 * ptr);
        assert_eq!(std::mem::size_of_val(&REF_VT), 20 * ptr);
        assert_eq!(std::mem::size_of_val(&BUILDER_VT), 10 * ptr);
        assert_eq!(std::mem::size_of_val(&LIST_VT), 10 * ptr);
        assert_eq!(std::mem::size_of_val(&STRING_LIST_VT), 8 * ptr);
        assert_eq!(std::mem::size_of_val(&QUEUE_VT), 9 * ptr);
        assert_eq!(std::mem::size_of::<FontMetrics1>(), 48);
        assert_eq!(std::mem::size_of::<FontProperty>(), 24);
        assert_eq!(std::mem::size_of::<AxisRange>(), 12);
    }

    #[test]
    fn properties_and_axes_follow_the_face() {
        let f = tiny();
        assert_eq!(property(&f, 1).as_deref(), Some("Winlux Tiny"));
        assert_eq!(property(&f, 4).as_deref(), Some("Winlux Tiny Bold"));
        assert_eq!(property(&f, 6).as_deref(), Some("WinluxTiny-Bold"));
        assert_eq!(property(&f, 10).as_deref(), Some("700"));
        assert_eq!(axes(&f)[0].value, 700.0);
        let ranges = [AxisRange {
            tag: WGHT,
            min: 600.0,
            max: 800.0,
        }];
        assert!(in_ranges(&f, &ranges, false));
        assert!(
            axis_distance(
                &f,
                &[AxisValue {
                    tag: WGHT,
                    value: 700.0
                }]
            ) == 0
        );
    }

    #[test]
    fn set_builder_and_collection_round_trip() {
        unsafe {
            let f = tiny();
            let mut b = std::ptr::null_mut();
            assert_eq!(font_set_builder(std::ptr::null_mut(), &mut b), S_OK);
            let b = b as *mut BuilderObj;
            let mut r = std::ptr::null_mut();
            assert_eq!(put_ref(&f, &mut r), S_OK);
            assert_eq!(builder_add_ref(b, r), S_OK);
            assert_eq!(builder_add_ref(b, r), S_OK); // doublon ignoré
            let mut set = std::ptr::null_mut();
            assert_eq!(builder_create(b, &mut set), S_OK);
            let set = set as *mut SetObj;
            assert_eq!(set_count(set), 1);
            let (mut idx, mut exists) = (0u32, 0i32);
            assert_eq!(set_find_reference(set, r, &mut idx, &mut exists), S_OK);
            assert_eq!((idx, exists), (0, 1));
            // Valeur de propriété et face créée depuis le jeu.
            let mut face_obj = std::ptr::null_mut();
            assert_eq!(set_create_face(set, 0, &mut face_obj), S_OK);
            assert!(is(face_obj, FACE_VT.ptr()));
            assert_eq!(face_weight(face_obj as *mut FaceObj), 700);
            // Collection tirée du jeu : une famille.
            let mut col = std::ptr::null_mut();
            assert_eq!(
                collection_from_set(std::ptr::null_mut(), set as *mut c_void, &mut col),
                S_OK
            );
            assert_eq!(Com::get(col as *mut CollectionObj).len(), 1);
            // Recherche par famille, graisse et style.
            let name: Vec<u16> = "winlux tiny".encode_utf16().chain([0]).collect();
            let mut m = std::ptr::null_mut();
            assert_eq!(
                set_matching_wss(set, name.as_ptr(), 400, 5, 0, &mut m),
                S_OK
            );
            assert_eq!(set_count(m as *mut SetObj), 1);
            for p in [m, col, face_obj, set as *mut c_void, r, b as *mut c_void] {
                let rel: unsafe extern "win64" fn(*mut c_void) -> u32 =
                    std::mem::transmute(*(*(p as *const *const *const ())).add(2));
                rel(p);
            }
        }
    }

    #[test]
    fn glyph_origins_add_advances_and_offsets() {
        unsafe {
            let adv = [10.0f32, 20.0, 5.0];
            let idx = [1u16, 2, 3];
            let run = GlyphRun {
                face: std::ptr::null_mut(),
                em_size: 12.0,
                count: 3,
                indices: idx.as_ptr(),
                advances: adv.as_ptr(),
                offsets: std::ptr::null(),
                sideways: 0,
                bidi_level: 0,
            };
            let mut out = [Point::default(); 3];
            let o = Point { x: 100.0, y: 50.0 };
            assert_eq!(
                glyph_origins(std::ptr::null_mut(), &run, o, out.as_mut_ptr()),
                S_OK
            );
            assert_eq!(out.map(|p| p.x), [100.0, 110.0, 130.0]);
            let rtl = GlyphRun {
                bidi_level: 1,
                ..run
            };
            assert_eq!(
                glyph_origins(std::ptr::null_mut(), &rtl, o, out.as_mut_ptr()),
                S_OK
            );
            assert_eq!(out.map(|p| p.x), [90.0, 70.0, 65.0]);
        }
    }

    #[test]
    fn convert_wss_fills_the_five_standard_axes() {
        unsafe {
            let mut out = [AxisValue { tag: 0, value: 0.0 }; 5];
            let n = set_convert_wss(
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                700,
                5,
                2,
                16.0,
                out.as_mut_ptr(),
            );
            assert_eq!(n, 5);
            assert_eq!(
                out[0],
                AxisValue {
                    tag: WGHT,
                    value: 700.0
                }
            );
            assert_eq!(out[1].value, 100.0);
            assert_eq!(
                out[4],
                AxisValue {
                    tag: ITAL,
                    value: 1.0
                }
            );
        }
    }
}
