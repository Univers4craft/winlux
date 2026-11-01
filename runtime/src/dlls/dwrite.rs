//! DirectWrite maison (dwrite.dll) : polices du système, faces, formats et
//! mise en page du texte, contours de glyphes.
//!
//! Les polices viennent du catalogue de `gdi32_fonts` (dossiers de polices
//! Linux et `C:\Windows\Fonts`) et leurs contours de `sfnt` (TrueType). Les
//! noms de polices Windows absentes (Segoe UI, Arial, Times New Roman,
//! Consolas…) sont servis par une police libre équivalente. Le dessin passe
//! par le Direct2D logiciel de `d2d1.rs` (`DrawText`, `DrawTextLayout`,
//! `DrawGlyphRun`) ou par le `IDWriteTextRenderer` de l'appli.
//!
//! Limites connues : pas de mise en forme OpenType (ligatures, crénage
//! GPOS, écritures complexes et bidirectionnelles), pas de polices CFF, et
//! les attributs par plage (`SetFontSize(…, range)`) ne s'appliquent que
//! s'ils couvrent tout le texte.
#![allow(clippy::too_many_arguments)]

use super::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use super::d2d1::{add_ref, method, not_implemented, nothing, qi_list, release};
use super::d2d_raster::{Path, Pt};
use super::gdi32_fonts::FontFace;
use super::ole32::Guid;
use super::sfnt::{self, OutlineSink};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const E_NOT_SUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
const DWRITE_E_FILEFORMAT: i32 = 0x8898_5000u32 as i32;
const DWRITE_E_NOFONT: i32 = 0x8898_5002u32 as i32;

const fn iid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    guid(d1, d2, d3, d4)
}
pub const IID_DWRITE_FACTORY: Guid = iid(
    0xb859ee5a,
    0xd838,
    0x4b5b,
    [0xa2, 0xe8, 0x1a, 0xdc, 0x7d, 0x93, 0xdb, 0x48],
);
const IID_FONT_COLLECTION: Guid = iid(
    0xa84cee02,
    0x3eea,
    0x4eee,
    [0xa8, 0x27, 0x87, 0xc1, 0xa0, 0x2a, 0x0f, 0xcc],
);
const IID_FONT_LIST: Guid = iid(
    0x1a0d8438,
    0x1d97,
    0x4ec1,
    [0xae, 0xf9, 0xa2, 0xfb, 0x86, 0xed, 0x6a, 0xcb],
);
const IID_FONT_FAMILY: Guid = iid(
    0xda20d8ef,
    0x812a,
    0x4c43,
    [0x98, 0x02, 0x62, 0xec, 0x4a, 0xbd, 0x7a, 0xdd],
);
const IID_FONT: Guid = iid(
    0xacd16696,
    0x8c14,
    0x4f5d,
    [0x87, 0x7e, 0xfe, 0x3f, 0xc1, 0xd3, 0x27, 0x37],
);
const IID_FONT_FACE: Guid = iid(
    0x5f49804d,
    0x7024,
    0x4d43,
    [0xbf, 0xa9, 0xd2, 0x59, 0x84, 0xf5, 0x38, 0x49],
);
const IID_FONT_FILE: Guid = iid(
    0x739d886a,
    0xcef5,
    0x47dc,
    [0x87, 0x69, 0x1a, 0x8b, 0x41, 0xbe, 0xbb, 0xb0],
);
const IID_LOCALIZED_STRINGS: Guid = iid(
    0x08256209,
    0x099a,
    0x4b34,
    [0xb8, 0x6d, 0xc2, 0x2b, 0x11, 0x0e, 0x77, 0x71],
);
const IID_TEXT_FORMAT: Guid = iid(
    0x9c906818,
    0x31d7,
    0x4fd3,
    [0xa1, 0x51, 0x7c, 0x5e, 0x22, 0x5d, 0xb5, 0x5a],
);
const IID_TEXT_LAYOUT: Guid = iid(
    0x53737037,
    0x6d14,
    0x410b,
    [0x9b, 0xfe, 0x0b, 0x18, 0x2b, 0xb7, 0x09, 0x61],
);
const IID_RENDERING_PARAMS: Guid = iid(
    0x2f0da53a,
    0x2add,
    0x47cd,
    [0x82, 0xee, 0xd9, 0xec, 0x34, 0x68, 0x8e, 0x75],
);

// --- Catalogue des polices -----------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct FaceInfo {
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub italic: bool,
    pub path: PathBuf,
    pub index: u32,
}

#[derive(Clone, Debug)]
struct Family {
    name: String,
    faces: Vec<FaceInfo>,
}

fn group(faces: impl IntoIterator<Item = FaceInfo>) -> Vec<Family> {
    let mut out: Vec<Family> = Vec::new();
    for f in faces {
        match out
            .iter_mut()
            .find(|x| x.name.eq_ignore_ascii_case(&f.family))
        {
            Some(x) => {
                if !x.faces.iter().any(|o| o.style == f.style) {
                    x.faces.push(f)
                }
            }
            None => out.push(Family {
                name: f.family.clone(),
                faces: vec![f],
            }),
        }
    }
    out.sort_by_key(|f| f.name.to_lowercase());
    out
}

fn info(f: &FontFace) -> FaceInfo {
    FaceInfo {
        family: f.family.clone(),
        style: f.style.clone(),
        weight: f.weight,
        italic: f.italic,
        path: f.path.clone(),
        index: f.index,
    }
}

/// Faces ajoutées hors du catalogue système (tests, collections privées).
fn extra() -> &'static Mutex<Vec<FaceInfo>> {
    static E: OnceLock<Mutex<Vec<FaceInfo>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(Vec::new()))
}

fn system_faces() -> &'static Vec<FaceInfo> {
    static S: OnceLock<Vec<FaceInfo>> = OnceLock::new();
    S.get_or_init(|| {
        super::gdi32_fonts::all_faces()
            .iter()
            .filter(|f| !f.cff)
            .map(info)
            .collect()
    })
}

fn families() -> Vec<Family> {
    let extra = super::lock(extra()).clone();
    group(extra.into_iter().chain(system_faces().iter().cloned()))
}

/// Ajoute une face au catalogue de DirectWrite.
#[cfg(test)]
pub fn register_face(f: FaceInfo) {
    let mut e = super::lock(extra());
    if !e.contains(&f) {
        e.push(f);
    }
}

/// Polices libres qui remplacent les polices Windows absentes.
fn substitutes(name: &str) -> &'static [&'static str] {
    const SANS: &[&str] = &[
        "Selawik",
        "Noto Sans",
        "Open Sans",
        "Liberation Sans",
        "Arimo",
        "DejaVu Sans",
        "FreeSans",
    ];
    const ARIAL: &[&str] = &[
        "Liberation Sans",
        "Arimo",
        "Noto Sans",
        "DejaVu Sans",
        "FreeSans",
    ];
    const SERIF: &[&str] = &[
        "Liberation Serif",
        "Tinos",
        "Noto Serif",
        "DejaVu Serif",
        "FreeSerif",
    ];
    const MONO: &[&str] = &[
        "Liberation Mono",
        "Cousine",
        "Noto Sans Mono",
        "DejaVu Sans Mono",
        "FreeMono",
    ];
    match name.to_ascii_lowercase().as_str() {
        "arial" | "helvetica" | "microsoft sans serif" | "tahoma" | "verdana" | "calibri" => ARIAL,
        "times new roman" | "times" | "georgia" | "cambria" | "serif" => SERIF,
        "courier new" | "courier" | "consolas" | "lucida console" | "cascadia mono"
        | "cascadia code" | "monospace" => MONO,
        _ => SANS,
    }
}

fn pick(fam: &Family, weight: u32, italic: bool) -> FaceInfo {
    fam.faces
        .iter()
        .min_by_key(|f| (f.italic != italic) as u32 * 2000 + (f.weight as u32).abs_diff(weight))
        .cloned()
        .expect("famille non vide")
}

/// Choisit la face la plus proche ; se rabat sur une police équivalente.
fn match_face(fams: &[Family], name: &str, weight: u32, italic: bool) -> Option<FaceInfo> {
    let find = |n: &str| fams.iter().find(|f| f.name.eq_ignore_ascii_case(n));
    if let Some(f) = find(name) {
        return Some(pick(f, weight, italic));
    }
    for s in substitutes(name) {
        if let Some(f) = find(s) {
            return Some(pick(f, weight, italic));
        }
    }
    fams.first().map(|f| pick(f, weight, italic))
}

/// Police de secours pour un caractère absent de la police demandée.
fn fallback_for(
    fams: &[Family],
    ch: u32,
    weight: u32,
    italic: bool,
) -> Option<(FaceInfo, Arc<sfnt::Font>)> {
    const CHAIN: &[&str] = &[
        "Noto Sans",
        "DejaVu Sans",
        "FreeSans",
        "Noto Sans Symbols",
        "Noto Sans Symbols2",
        "Noto Color Emoji",
        "DejaVu Sans Mono",
        "FreeSerif",
    ];
    for name in CHAIN {
        if let Some(fam) = fams.iter().find(|f| f.name.eq_ignore_ascii_case(name)) {
            let fi = pick(fam, weight, italic);
            if let Some(font) = sfnt::load(&fi.path, fi.index) {
                if font.glyph_index(ch) != 0 {
                    return Some((fi, font));
                }
            }
        }
    }
    None
}

// --- Objets COM communs -----------------------------------------------------------

unsafe fn put_wide(s: &str, buf: *mut u16, size: u32) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() || (size as usize) < w.len() + 1 {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    S_OK
}

unsafe fn read_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

unsafe extern "win64" fn get_false(_this: *mut c_void) -> i32 {
    0
}
unsafe extern "win64" fn get_zero(_this: *mut c_void) -> u32 {
    0
}

// Chaînes localisées (une seule langue : en-us).
type StringsObj = Com<Vec<(String, String)>>;

fn new_strings(v: Vec<(String, String)>) -> *mut StringsObj {
    Com::create(STRINGS_VT.ptr(), v)
}
unsafe extern "win64" fn strings_qi(
    this: *mut StringsObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_LOCALIZED_STRINGS])
}
unsafe extern "win64" fn strings_count(this: *mut StringsObj) -> u32 {
    Com::get(this).len() as u32
}
unsafe extern "win64" fn strings_find(
    this: *mut StringsObj,
    name: *const u16,
    index: *mut u32,
    exists: *mut i32,
) -> i32 {
    let n = read_wide(name);
    let pos = Com::get(this)
        .iter()
        .position(|(l, _)| l.eq_ignore_ascii_case(&n));
    if !index.is_null() {
        *index = pos.map_or(u32::MAX, |p| p as u32);
    }
    if !exists.is_null() {
        *exists = pos.is_some() as i32;
    }
    S_OK
}
unsafe fn strings_item(this: *mut StringsObj, i: u32, locale: bool) -> Option<String> {
    Com::get(this)
        .get(i as usize)
        .map(|(l, s)| if locale { l.clone() } else { s.clone() })
}
unsafe extern "win64" fn strings_locale_len(this: *mut StringsObj, i: u32, len: *mut u32) -> i32 {
    strings_len(this, i, len, true)
}
unsafe extern "win64" fn strings_locale(
    this: *mut StringsObj,
    i: u32,
    buf: *mut u16,
    size: u32,
) -> i32 {
    match strings_item(this, i, true) {
        Some(s) => put_wide(&s, buf, size),
        None => E_INVALIDARG,
    }
}
unsafe fn strings_len(this: *mut StringsObj, i: u32, len: *mut u32, locale: bool) -> i32 {
    match (strings_item(this, i, locale), len.is_null()) {
        (Some(s), false) => {
            *len = s.encode_utf16().count() as u32;
            S_OK
        }
        _ => E_INVALIDARG,
    }
}
unsafe extern "win64" fn strings_string_len(this: *mut StringsObj, i: u32, len: *mut u32) -> i32 {
    strings_len(this, i, len, false)
}
unsafe extern "win64" fn strings_string(
    this: *mut StringsObj,
    i: u32,
    buf: *mut u16,
    size: u32,
) -> i32 {
    match strings_item(this, i, false) {
        Some(s) => put_wide(&s, buf, size),
        None => E_INVALIDARG,
    }
}
static STRINGS_VT: VTable<9> = VTable([
    strings_qi as *const (),
    add_ref::<Vec<(String, String)>> as *const (),
    release::<Vec<(String, String)>> as *const (),
    strings_count as *const (),
    strings_find as *const (),
    strings_locale_len as *const (),
    strings_locale as *const (),
    strings_string_len as *const (),
    strings_string as *const (),
]);

fn en(s: &str) -> Vec<(String, String)> {
    vec![("en-us".into(), s.into())]
}

// --- Fabrique -----------------------------------------------------------------------

type FactoryObj = Com<()>;

unsafe extern "win64" fn factory_qi(
    this: *mut FactoryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_DWRITE_FACTORY])
}

unsafe extern "win64" fn system_collection(
    _this: *mut FactoryObj,
    out: *mut *mut c_void,
    _check: i32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_collection())
}

unsafe extern "win64" fn font_file_reference(
    _this: *mut FactoryObj,
    path: *const u16,
    _time: *const u64,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || path.is_null() {
        return E_INVALIDARG;
    }
    let p = crate::paths::to_linux(&read_wide(path));
    if !p.exists() {
        *out = std::ptr::null_mut();
        return 0x8007_0002u32 as i32;
    }
    put(out, Com::create(FILE_VT.ptr(), p))
}

unsafe extern "win64" fn factory_font_face(
    _this: *mut FactoryObj,
    _kind: u32,
    count: u32,
    files: *const *mut c_void,
    index: u32,
    _sims: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || files.is_null() || count == 0 {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let file = *files;
    if file.is_null() || *(file as *const *const ()) != FILE_VT.ptr() {
        return E_INVALIDARG;
    }
    let path = Com::get(file as *mut FileObj).clone();
    let Some(font) = sfnt::load(&path, index) else {
        return DWRITE_E_FILEFORMAT;
    };
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fi = super::gdi32_fonts::parse_font_file(&path)
        .iter()
        .find(|f| f.index == index)
        .map(info)
        .unwrap_or(FaceInfo {
            family: name,
            style: "Regular".into(),
            weight: 400,
            italic: false,
            path,
            index,
        });
    put(out, new_face(fi, font))
}

unsafe extern "win64" fn rendering_params(_this: *mut FactoryObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(PARAMS_VT.ptr(), ()))
}
unsafe extern "win64" fn monitor_rendering_params(
    this: *mut FactoryObj,
    _monitor: usize,
    out: *mut *mut c_void,
) -> i32 {
    rendering_params(this, out)
}
unsafe extern "win64" fn custom_rendering_params(
    this: *mut FactoryObj,
    _g: f32,
    _c: f32,
    _l: f32,
    _geo: u32,
    _mode: u32,
    out: *mut *mut c_void,
) -> i32 {
    rendering_params(this, out)
}
unsafe extern "win64" fn register_loader(_this: *mut FactoryObj, _loader: *mut c_void) -> i32 {
    S_OK
}

unsafe extern "win64" fn create_text_format(
    _this: *mut FactoryObj,
    family: *const u16,
    _collection: *mut c_void,
    weight: u32,
    style: u32,
    stretch: u32,
    size: f32,
    locale: *const u16,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || family.is_null() {
        return E_INVALIDARG;
    }
    if size <= 0.0 {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let fmt = Format {
        family: read_wide(family),
        weight,
        style,
        stretch,
        size,
        locale: read_wide(locale),
        ..Format::default()
    };
    put(
        out,
        Com::create(FORMAT_VT.ptr(), Mutex::new(Text { fmt, layout: None })),
    )
}

unsafe extern "win64" fn create_text_layout(
    _this: *mut FactoryObj,
    text: *const u16,
    len: u32,
    format: *mut c_void,
    max_w: f32,
    max_h: f32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(fmt) = format_of(format) else {
        return E_INVALIDARG;
    };
    let text = if text.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(text, len as usize).to_vec()
    };
    let layout = LayoutState {
        text,
        max_w,
        max_h,
        underline: false,
        strike: false,
        lines: None,
    };
    put(
        out,
        Com::create(
            LAYOUT_VT.ptr(),
            Mutex::new(Text {
                fmt,
                layout: Some(layout),
            }),
        ),
    )
}

unsafe extern "win64" fn gdi_compatible_layout(
    this: *mut FactoryObj,
    text: *const u16,
    len: u32,
    format: *mut c_void,
    max_w: f32,
    max_h: f32,
    _ppd: f32,
    _transform: *const c_void,
    _natural: i32,
    out: *mut *mut c_void,
) -> i32 {
    create_text_layout(this, text, len, format, max_w, max_h, out)
}

static FACTORY_VT: VTable<24> = VTable([
    factory_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    system_collection as *const (),
    not_implemented as *const (), // CreateCustomFontCollection
    register_loader as *const (), // RegisterFontCollectionLoader
    register_loader as *const (), // UnregisterFontCollectionLoader
    font_file_reference as *const (),
    not_implemented as *const (), // CreateCustomFontFileReference
    factory_font_face as *const (),
    rendering_params as *const (),
    monitor_rendering_params as *const (),
    custom_rendering_params as *const (),
    register_loader as *const (), // RegisterFontFileLoader
    register_loader as *const (), // UnregisterFontFileLoader
    create_text_format as *const (),
    not_implemented as *const (), // CreateTypography
    not_implemented as *const (), // GetGdiInterop
    create_text_layout as *const (),
    gdi_compatible_layout as *const (),
    not_implemented as *const (), // CreateEllipsisTrimmingSign
    not_implemented as *const (), // CreateTextAnalyzer
    not_implemented as *const (), // CreateNumberSubstitution
    not_implemented as *const (), // CreateGlyphRunAnalysis
]);

// Paramètres de rendu : valeurs par défaut de Windows.
unsafe extern "win64" fn params_qi(
    this: *mut FactoryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_RENDERING_PARAMS])
}
unsafe extern "win64" fn gamma(_this: *mut c_void) -> f32 {
    2.2
}
unsafe extern "win64" fn one(_this: *mut c_void) -> f32 {
    1.0
}
unsafe extern "win64" fn pixel_geometry(_this: *mut c_void) -> u32 {
    1 // RGB
}
unsafe extern "win64" fn rendering_mode(_this: *mut c_void) -> u32 {
    5 // OUTLINE
}
static PARAMS_VT: VTable<8> = VTable([
    params_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    gamma as *const (),
    one as *const (),
    one as *const (),
    pixel_geometry as *const (),
    rendering_mode as *const (),
]);

// Fichier de police (référence à un chemin).
type FileObj = Com<PathBuf>;
unsafe extern "win64" fn file_qi(
    this: *mut FileObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_FILE])
}
unsafe extern "win64" fn file_key(
    this: *mut FileObj,
    key: *mut *const c_void,
    size: *mut u32,
) -> i32 {
    let p = Com::get(this);
    if !key.is_null() {
        *key = p.as_os_str().as_encoded_bytes().as_ptr() as *const c_void;
    }
    if !size.is_null() {
        *size = p.as_os_str().len() as u32;
    }
    S_OK
}
unsafe extern "win64" fn file_analyze(
    this: *mut FileObj,
    supported: *mut i32,
    file_type: *mut u32,
    face_type: *mut u32,
    faces: *mut u32,
) -> i32 {
    let data = std::fs::read(Com::get(this)).unwrap_or_default();
    let n = sfnt::face_count(&data);
    let ok = sfnt::Font::parse(data.clone(), 0).is_some();
    let ttc = data.get(0..4) == Some(b"ttcf");
    if !supported.is_null() {
        *supported = ok as i32;
    }
    if !file_type.is_null() {
        *file_type = if !ok {
            0
        } else if ttc {
            4
        } else {
            3
        }; // TRUETYPE_COLLECTION / TRUETYPE
    }
    if !face_type.is_null() {
        *face_type = if !ok {
            6
        } else if ttc {
            2
        } else {
            1
        };
    }
    if !faces.is_null() {
        *faces = n;
    }
    S_OK
}
static FILE_VT: VTable<6> = VTable([
    file_qi as *const (),
    add_ref::<PathBuf> as *const (),
    release::<PathBuf> as *const (),
    file_key as *const (),
    not_implemented as *const (), // GetLoader
    file_analyze as *const (),
]);

// --- Collection, familles, polices ---------------------------------------------------

type CollectionObj = Com<Vec<Family>>;

fn new_collection() -> *mut CollectionObj {
    Com::create(COLLECTION_VT.ptr(), families())
}
unsafe extern "win64" fn collection_qi(
    this: *mut CollectionObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_COLLECTION])
}
unsafe extern "win64" fn collection_count(this: *mut CollectionObj) -> u32 {
    Com::get(this).len() as u32
}
unsafe extern "win64" fn collection_family(
    this: *mut CollectionObj,
    i: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match Com::get(this).get(i as usize) {
        Some(f) => put(out, Com::create(FAMILY_VT.ptr(), f.clone())),
        None => {
            *out = std::ptr::null_mut();
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn collection_find(
    this: *mut CollectionObj,
    name: *const u16,
    index: *mut u32,
    exists: *mut i32,
) -> i32 {
    let n = read_wide(name);
    let pos = Com::get(this)
        .iter()
        .position(|f| f.name.eq_ignore_ascii_case(&n));
    if !index.is_null() {
        *index = pos.map_or(u32::MAX, |p| p as u32);
    }
    if !exists.is_null() {
        *exists = pos.is_some() as i32;
    }
    S_OK
}
unsafe extern "win64" fn collection_font_from_face(
    _this: *mut CollectionObj,
    face: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match face_of(face) {
        Some(f) => put(out, Com::create(FONT_VT.ptr(), f.info.clone())),
        None => {
            *out = std::ptr::null_mut();
            DWRITE_E_NOFONT
        }
    }
}
static COLLECTION_VT: VTable<7> = VTable([
    collection_qi as *const (),
    add_ref::<Vec<Family>> as *const (),
    release::<Vec<Family>> as *const (),
    collection_count as *const (),
    collection_family as *const (),
    collection_find as *const (),
    collection_font_from_face as *const (),
]);

type FamilyObj = Com<Family>;

unsafe extern "win64" fn family_qi(
    this: *mut FamilyObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_LIST, IID_FONT_FAMILY])
}
unsafe extern "win64" fn family_collection(_this: *mut FamilyObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_collection())
}
unsafe extern "win64" fn family_count(this: *mut FamilyObj) -> u32 {
    Com::get(this).faces.len() as u32
}
unsafe extern "win64" fn family_font(this: *mut FamilyObj, i: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match Com::get(this).faces.get(i as usize) {
        Some(f) => put(out, Com::create(FONT_VT.ptr(), f.clone())),
        None => {
            *out = std::ptr::null_mut();
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn family_names(this: *mut FamilyObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_strings(en(&Com::get(this).name)))
}
unsafe extern "win64" fn family_first_match(
    this: *mut FamilyObj,
    weight: u32,
    _stretch: u32,
    style: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(FONT_VT.ptr(), pick(Com::get(this), weight, style != 0)),
    )
}
unsafe extern "win64" fn family_matching(
    this: *mut FamilyObj,
    weight: u32,
    _stretch: u32,
    style: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut f = Com::get(this).clone();
    let italic = style != 0;
    f.faces
        .sort_by_key(|x| (x.italic != italic) as u32 * 2000 + (x.weight as u32).abs_diff(weight));
    put(out, Com::create(FAMILY_VT.ptr(), f))
}
static FAMILY_VT: VTable<9> = VTable([
    family_qi as *const (),
    add_ref::<Family> as *const (),
    release::<Family> as *const (),
    family_collection as *const (),
    family_count as *const (),
    family_font as *const (),
    family_names as *const (),
    family_first_match as *const (),
    family_matching as *const (),
]);

type FontObj = Com<FaceInfo>;

unsafe extern "win64" fn font_qi(
    this: *mut FontObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT])
}
unsafe extern "win64" fn font_family(this: *mut FontObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let name = &Com::get(this).family;
    let fam = families()
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
        .unwrap_or(Family {
            name: name.clone(),
            faces: vec![Com::get(this).clone()],
        });
    put(out, Com::create(FAMILY_VT.ptr(), fam))
}
unsafe extern "win64" fn font_weight(this: *mut FontObj) -> u32 {
    Com::get(this).weight as u32
}
unsafe extern "win64" fn font_stretch(_this: *mut FontObj) -> u32 {
    5
}
unsafe extern "win64" fn font_style(this: *mut FontObj) -> u32 {
    if Com::get(this).italic {
        2
    } else {
        0
    }
}
unsafe extern "win64" fn font_face_names(this: *mut FontObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_strings(en(&Com::get(this).style)))
}
unsafe extern "win64" fn font_info_strings(
    _this: *mut FontObj,
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
unsafe fn font_loaded(this: *mut FontObj) -> Option<Arc<sfnt::Font>> {
    let f = Com::get(this);
    sfnt::load(&f.path, f.index)
}
unsafe extern "win64" fn font_metrics(this: *mut FontObj, out: *mut FontMetrics) {
    if let (Some(f), false) = (font_loaded(this), out.is_null()) {
        *out = FontMetrics::from(&f.metrics);
    }
}
unsafe extern "win64" fn font_has_char(this: *mut FontObj, ch: u32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = font_loaded(this).is_some_and(|f| f.glyph_index(ch) != 0) as i32;
    S_OK
}
unsafe extern "win64" fn font_create_face(this: *mut FontObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match font_loaded(this) {
        Some(f) => put(out, new_face(Com::get(this).clone(), f)),
        None => {
            *out = std::ptr::null_mut();
            DWRITE_E_FILEFORMAT
        }
    }
}
static FONT_VT: VTable<14> = VTable([
    font_qi as *const (),
    add_ref::<FaceInfo> as *const (),
    release::<FaceInfo> as *const (),
    font_family as *const (),
    font_weight as *const (),
    font_stretch as *const (),
    font_style as *const (),
    get_false as *const (), // IsSymbolFont
    font_face_names as *const (),
    font_info_strings as *const (),
    get_zero as *const (), // GetSimulations
    font_metrics as *const (),
    font_has_char as *const (),
    font_create_face as *const (),
]);

// --- Faces ----------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontMetrics {
    pub units_per_em: u16,
    pub ascent: u16,
    pub descent: u16,
    pub line_gap: i16,
    pub cap_height: u16,
    pub x_height: u16,
    pub underline_position: i16,
    pub underline_thickness: u16,
    pub strikethrough_position: i16,
    pub strikethrough_thickness: u16,
}

impl From<&sfnt::Metrics> for FontMetrics {
    fn from(m: &sfnt::Metrics) -> Self {
        FontMetrics {
            units_per_em: m.units_per_em,
            ascent: m.ascent,
            descent: m.descent,
            line_gap: m.line_gap,
            cap_height: m.cap_height,
            x_height: m.x_height,
            underline_position: m.underline_position,
            underline_thickness: m.underline_thickness,
            strikethrough_position: m.strikeout_position,
            strikethrough_thickness: m.strikeout_thickness,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct GlyphMetrics {
    left_side_bearing: i32,
    advance_width: u32,
    right_side_bearing: i32,
    top_side_bearing: i32,
    advance_height: u32,
    bottom_side_bearing: i32,
    vertical_origin_y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct GlyphOffset {
    advance: f32,
    ascender: f32,
}

/// DWRITE_GLYPH_RUN.
#[repr(C)]
pub struct GlyphRun {
    pub face: *mut c_void,
    pub em_size: f32,
    pub count: u32,
    pub indices: *const u16,
    pub advances: *const f32,
    offsets: *const GlyphOffset,
    pub sideways: i32,
    pub bidi_level: u32,
}

pub struct Face {
    info: FaceInfo,
    font: Arc<sfnt::Font>,
}

type FaceObj = Com<Face>;

fn new_face(info: FaceInfo, font: Arc<sfnt::Font>) -> *mut FaceObj {
    Com::create(FACE_VT.ptr(), Face { info, font })
}

/// La face derrière un IDWriteFontFace, s'il vient de Winlux.
unsafe fn face_of<'a>(p: *mut c_void) -> Option<&'a Face> {
    if p.is_null() || *(p as *const *const ()) != FACE_VT.ptr() {
        return None;
    }
    Some(Com::get(p as *mut FaceObj))
}

unsafe extern "win64" fn face_qi(
    this: *mut FaceObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_FACE])
}
unsafe extern "win64" fn face_type(this: *mut FaceObj) -> u32 {
    if Com::get(this)
        .info
        .path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ttc"))
    {
        2
    } else {
        1
    }
}
unsafe extern "win64" fn face_files(
    this: *mut FaceObj,
    count: *mut u32,
    files: *mut *mut c_void,
) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    if !files.is_null() && *count >= 1 {
        *files = Com::create(FILE_VT.ptr(), Com::get(this).info.path.clone()) as *mut c_void;
    }
    *count = 1;
    S_OK
}
unsafe extern "win64" fn face_index(this: *mut FaceObj) -> u32 {
    Com::get(this).info.index
}
unsafe extern "win64" fn face_metrics(this: *mut FaceObj, out: *mut FontMetrics) {
    if !out.is_null() {
        *out = FontMetrics::from(&Com::get(this).font.metrics);
    }
}
unsafe extern "win64" fn face_glyph_count(this: *mut FaceObj) -> u16 {
    Com::get(this).font.glyph_count()
}
unsafe extern "win64" fn face_design_metrics(
    this: *mut FaceObj,
    indices: *const u16,
    count: u32,
    out: *mut GlyphMetrics,
    _sideways: i32,
) -> i32 {
    if indices.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let f = &Com::get(this).font;
    let m = &f.metrics;
    for i in 0..count as usize {
        let g = *indices.add(i);
        let adv = f.advance(g) as i32;
        let [x0, y0, x1, y1] = f.bbox(g).unwrap_or([0; 4]).map(|v| v as i32);
        let height = m.ascent as i32 + m.descent as i32;
        *out.add(i) = GlyphMetrics {
            left_side_bearing: f.left_bearing(g) as i32,
            advance_width: adv as u32,
            right_side_bearing: adv - x1,
            top_side_bearing: m.ascent as i32 - y1,
            advance_height: height as u32,
            bottom_side_bearing: y0 + m.descent as i32,
            vertical_origin_y: m.ascent as i32,
        };
        if x0 == 0 && x1 == 0 {
            (*out.add(i)).right_side_bearing = 0;
        }
    }
    S_OK
}
unsafe extern "win64" fn face_glyph_indices(
    this: *mut FaceObj,
    cps: *const u32,
    count: u32,
    out: *mut u16,
) -> i32 {
    if cps.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let f = &Com::get(this).font;
    for i in 0..count as usize {
        *out.add(i) = f.glyph_index(*cps.add(i));
    }
    S_OK
}
unsafe extern "win64" fn face_table(
    _this: *mut FaceObj,
    _tag: u32,
    data: *mut *const c_void,
    size: *mut u32,
    ctx: *mut *mut c_void,
    exists: *mut i32,
) -> i32 {
    for (p, v) in [(data as *mut usize, 0usize), (ctx as *mut usize, 0)] {
        if !p.is_null() {
            *p = v;
        }
    }
    if !size.is_null() {
        *size = 0;
    }
    if !exists.is_null() {
        *exists = 0;
    }
    S_OK
}

/// Contour des glyphes vers un ID2D1SimplifiedGeometrySink (repère y vers le bas).
struct ComSink {
    sink: *mut c_void,
    cur: Pt,
    open: bool,
}
impl ComSink {
    unsafe fn end(&mut self) {
        if self.open {
            method::<unsafe extern "win64" fn(*mut c_void, u32)>(self.sink, 8)(self.sink, 1);
            self.open = false;
        }
    }
}
fn pack(p: Pt) -> u64 {
    p.0.to_bits() as u64 | (p.1.to_bits() as u64) << 32
}
impl OutlineSink for ComSink {
    fn move_to(&mut self, p: Pt) {
        unsafe {
            self.end();
            method::<unsafe extern "win64" fn(*mut c_void, u64, u32)>(self.sink, 5)(
                self.sink,
                pack(p),
                0,
            );
        }
        self.cur = p;
        self.open = true;
    }
    fn line_to(&mut self, p: Pt) {
        unsafe {
            method::<unsafe extern "win64" fn(*mut c_void, *const Pt, u32)>(self.sink, 6)(
                self.sink, &p, 1,
            )
        };
        self.cur = p;
    }
    fn quad_to(&mut self, c: Pt, p: Pt) {
        let s = self.cur;
        let seg = [
            (s.0 + (c.0 - s.0) * 2.0 / 3.0, s.1 + (c.1 - s.1) * 2.0 / 3.0),
            (p.0 + (c.0 - p.0) * 2.0 / 3.0, p.1 + (c.1 - p.1) * 2.0 / 3.0),
            p,
        ];
        unsafe {
            method::<unsafe extern "win64" fn(*mut c_void, *const [Pt; 3], u32)>(self.sink, 7)(
                self.sink, &seg, 1,
            )
        };
        self.cur = p;
    }
    fn close(&mut self) {
        unsafe { self.end() };
    }
}

/// Place les points d'un glyphe : unités de police → DIP, y vers le bas.
struct Placed<'a> {
    inner: &'a mut dyn OutlineSink,
    scale: f32,
    origin: Pt,
}
impl Placed<'_> {
    fn map(&self, p: Pt) -> Pt {
        (
            self.origin.0 + p.0 * self.scale,
            self.origin.1 - p.1 * self.scale,
        )
    }
}
impl OutlineSink for Placed<'_> {
    fn move_to(&mut self, p: Pt) {
        let q = self.map(p);
        self.inner.move_to(q)
    }
    fn line_to(&mut self, p: Pt) {
        let q = self.map(p);
        self.inner.line_to(q)
    }
    fn quad_to(&mut self, c: Pt, p: Pt) {
        let (c, p) = (self.map(c), self.map(p));
        self.inner.quad_to(c, p)
    }
    fn close(&mut self) {
        self.inner.close()
    }
}

/// Envoie au puits les contours d'une suite de glyphes posée en `origin`.
fn outline_glyphs(
    font: &sfnt::Font,
    em: f32,
    glyphs: &[(u16, f32, Pt)],
    origin: Pt,
    sink: &mut dyn OutlineSink,
) {
    let scale = em / font.metrics.units_per_em as f32;
    let mut x = origin.0;
    for &(g, adv, off) in glyphs {
        let mut p = Placed {
            inner: sink,
            scale,
            origin: (x + off.0, origin.1 - off.1),
        };
        font.outline(g, &mut p);
        x += adv;
    }
}

unsafe fn run_glyphs(
    font: &sfnt::Font,
    em: f32,
    indices: *const u16,
    advances: *const f32,
    offsets: *const GlyphOffset,
    count: u32,
) -> Vec<(u16, f32, Pt)> {
    let scale = em / font.metrics.units_per_em as f32;
    (0..count as usize)
        .map(|i| {
            let g = *indices.add(i);
            let adv = if advances.is_null() {
                font.advance(g) as f32 * scale
            } else {
                *advances.add(i)
            };
            let off = if offsets.is_null() {
                (0.0, 0.0)
            } else {
                let o = *offsets.add(i);
                (o.advance, o.ascender)
            };
            (g, adv, off)
        })
        .collect()
}

unsafe extern "win64" fn face_run_outline(
    this: *mut FaceObj,
    em: f32,
    indices: *const u16,
    advances: *const f32,
    offsets: *const GlyphOffset,
    count: u32,
    _sideways: i32,
    _rtl: i32,
    sink: *mut c_void,
) -> i32 {
    if sink.is_null() || (indices.is_null() && count > 0) {
        return E_INVALIDARG;
    }
    let f = &Com::get(this).font;
    method::<unsafe extern "win64" fn(*mut c_void, u32)>(sink, 3)(sink, 1); // D2D1_FILL_MODE_WINDING
    let glyphs = run_glyphs(f, em, indices, advances, offsets, count);
    let mut s = ComSink {
        sink,
        cur: (0.0, 0.0),
        open: false,
    };
    outline_glyphs(f, em, &glyphs, (0.0, 0.0), &mut s);
    s.end();
    S_OK
}
unsafe extern "win64" fn face_recommended_mode(
    _this: *mut FaceObj,
    _em: f32,
    _ppd: f32,
    _measuring: u32,
    _params: *mut c_void,
    mode: *mut u32,
) -> i32 {
    if !mode.is_null() {
        *mode = 5; // OUTLINE : le rendu logiciel dessine les contours
    }
    S_OK
}
unsafe extern "win64" fn face_gdi_metrics(
    this: *mut FaceObj,
    _em: f32,
    _ppd: f32,
    _m: *const c_void,
    out: *mut FontMetrics,
) -> i32 {
    face_metrics(this, out);
    S_OK
}
unsafe extern "win64" fn face_gdi_glyph_metrics(
    this: *mut FaceObj,
    _em: f32,
    _ppd: f32,
    _m: *const c_void,
    _natural: i32,
    indices: *const u16,
    count: u32,
    out: *mut GlyphMetrics,
    sideways: i32,
) -> i32 {
    face_design_metrics(this, indices, count, out, sideways)
}
static FACE_VT: VTable<18> = VTable([
    face_qi as *const (),
    add_ref::<Face> as *const (),
    release::<Face> as *const (),
    face_type as *const (),
    face_files as *const (),
    face_index as *const (),
    get_zero as *const (),  // GetSimulations
    get_false as *const (), // IsSymbolFont
    face_metrics as *const (),
    face_glyph_count as *const (),
    face_design_metrics as *const (),
    face_glyph_indices as *const (),
    face_table as *const (),
    nothing as *const (), // ReleaseFontTable
    face_run_outline as *const (),
    face_recommended_mode as *const (),
    face_gdi_metrics as *const (),
    face_gdi_glyph_metrics as *const (),
]);

// --- Format et mise en page -------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Format {
    family: String,
    weight: u32,
    style: u32,
    stretch: u32,
    size: f32,
    locale: String,
    text_align: u32,
    para_align: u32,
    wrapping: u32,
    reading: u32,
    flow: u32,
    tab: Option<f32>,
    trimming: [u32; 3],
    line_spacing: (u32, f32, f32),
}

impl Default for Format {
    fn default() -> Self {
        Format {
            family: "Segoe UI".into(),
            weight: 400,
            style: 0,
            stretch: 5,
            size: 12.0,
            locale: "en-us".into(),
            text_align: 0,
            para_align: 0,
            wrapping: 0,
            reading: 0,
            flow: 0,
            tab: None,
            trimming: [0; 3],
            line_spacing: (0, 0.0, 0.0),
        }
    }
}

#[derive(Clone, Debug)]
struct Glyph {
    font: usize,
    gid: u16,
    x: f32,
    advance: f32,
    /// Position UTF-16 du caractère et nombre d'unités UTF-16.
    pos: u32,
    len: u32,
    space: bool,
}

#[derive(Clone, Debug, Default)]
struct Line {
    start: u32,
    len: u32,
    trailing_ws: u32,
    newline: u32,
    width: f32,
    width_ws: f32,
    left: f32,
    top: f32,
    height: f32,
    baseline: f32,
    glyphs: Vec<Glyph>,
}

struct Lines {
    fonts: Vec<(FaceInfo, Arc<sfnt::Font>)>,
    lines: Vec<Line>,
    /// Coin supérieur gauche du bloc de texte.
    top: f32,
    width: f32,
    width_ws: f32,
    height: f32,
}

struct LayoutState {
    text: Vec<u16>,
    max_w: f32,
    max_h: f32,
    underline: bool,
    strike: bool,
    lines: Option<Arc<Lines>>,
}

pub struct Text {
    fmt: Format,
    layout: Option<LayoutState>,
}

type TextObj = Com<Mutex<Text>>;

unsafe fn text<'a>(this: *mut TextObj) -> MutexGuard<'a, Text> {
    super::lock(Com::get(this))
}

unsafe fn is_text(p: *mut c_void) -> bool {
    !p.is_null() && {
        let vt = *(p as *const *const ());
        vt == FORMAT_VT.ptr() || vt == LAYOUT_VT.ptr()
    }
}

unsafe fn format_of(p: *mut c_void) -> Option<Format> {
    is_text(p).then(|| text(p as *mut TextObj).fmt.clone())
}

fn decode(text: &[u16]) -> Vec<(u32, u32, u32)> {
    // (caractère, position UTF-16, longueur UTF-16)
    let mut out = Vec::new();
    let mut pos = 0u32;
    for r in char::decode_utf16(text.iter().copied()) {
        let (c, n) = match r {
            Ok(c) => (c as u32, c.len_utf16() as u32),
            Err(_) => (0xFFFD, 1),
        };
        out.push((c, pos, n));
        pos += n;
    }
    out
}

fn is_break_space(c: u32) -> bool {
    matches!(c, 0x20 | 0x09 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x205F | 0x3000)
}

/// Met le texte en lignes : polices de secours, retours à la ligne,
/// coupure aux espaces, tabulations, alignements.
fn build(fmt: &Format, text: &[u16], max_w: f32, max_h: f32) -> Lines {
    let fams = families();
    let italic = fmt.style != 0;
    let mut fonts: Vec<(FaceInfo, Arc<sfnt::Font>)> = Vec::new();
    if let Some(fi) = match_face(&fams, &fmt.family, fmt.weight, italic) {
        if let Some(f) = sfnt::load(&fi.path, fi.index) {
            fonts.push((fi, f));
        }
    }
    let size = fmt.size;
    let (ascent, descent, gap) = match fonts.first() {
        Some((_, f)) => {
            let s = size / f.metrics.units_per_em as f32;
            (
                f.metrics.ascent as f32 * s,
                f.metrics.descent as f32 * s,
                f.metrics.line_gap.max(0) as f32 * s,
            )
        }
        None => (size * 0.9, size * 0.25, 0.0),
    };
    let (height, baseline) = match fmt.line_spacing {
        (1, h, b) if h > 0.0 => (h, b),
        _ => (ascent + descent + gap, ascent),
    };
    let tab = fmt.tab.unwrap_or(size * 4.0).max(1.0);
    let wrap = fmt.wrapping != 1 && max_w.is_finite() && max_w > 0.0;

    // Glyphes de chaque caractère (les sauts de ligne n'en ont pas).
    let chars = decode(text);
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = Line::default();
    let mut x = 0.0f32;
    let mut i = 0;
    let finish = |cur: &mut Line, lines: &mut Vec<Line>, next: u32| {
        let mut l = std::mem::take(cur);
        l.len = next - l.start;
        let ws: Vec<&Glyph> = l.glyphs.iter().rev().take_while(|g| g.space).collect();
        l.trailing_ws = ws.iter().map(|g| g.len).sum::<u32>() + l.newline;
        let full: f32 = l.glyphs.iter().map(|g| g.advance).sum();
        l.width_ws = full;
        l.width = full - ws.iter().map(|g| g.advance).sum::<f32>();
        cur.start = next;
        lines.push(l);
    };
    while i < chars.len() {
        let (c, pos, n) = chars[i];
        // Saut de ligne : CR, LF, CRLF, NEL, LS, PS.
        if matches!(c, 0x0A | 0x0B | 0x0C | 0x0D | 0x85 | 0x2028 | 0x2029) {
            let mut nl = n;
            if c == 0x0D && chars.get(i + 1).is_some_and(|x| x.0 == 0x0A) {
                nl += 1;
                i += 1;
            }
            cur.newline = nl;
            finish(&mut cur, &mut lines, pos + nl);
            x = 0.0;
            i += 1;
            continue;
        }
        let (font, gid) = resolve_glyph(&fams, &mut fonts, c, fmt.weight, italic);
        let space = is_break_space(c);
        let advance = if c == 0x09 {
            ((x / tab).floor() + 1.0) * tab - x
        } else {
            match fonts.get(font) {
                Some((_, f)) => f.advance(gid) as f32 * size / f.metrics.units_per_em as f32,
                None => size * 0.5,
            }
        };
        let g = Glyph {
            font,
            gid,
            x,
            advance,
            pos,
            len: n,
            space,
        };
        if wrap && !space && x + advance > max_w && !cur.glyphs.is_empty() {
            // Recule jusqu'au dernier espace de la ligne ; sinon coupe ici.
            let cut = cur.glyphs.iter().rposition(|g| g.space).map(|k| k + 1);
            match cut {
                Some(k) if k < cur.glyphs.len() => {
                    let rest: Vec<Glyph> = cur.glyphs.split_off(k);
                    let next = rest[0].pos;
                    finish(&mut cur, &mut lines, next);
                    let shift = rest[0].x;
                    cur.glyphs = rest
                        .into_iter()
                        .map(|mut g| {
                            g.x -= shift;
                            g
                        })
                        .collect();
                    x = cur.glyphs.iter().map(|g| g.advance).sum();
                }
                Some(_) => {
                    finish(&mut cur, &mut lines, pos);
                    x = 0.0;
                }
                None => {
                    finish(&mut cur, &mut lines, pos);
                    x = 0.0;
                }
            }
            let mut g = g;
            g.x = x;
            x += g.advance;
            cur.glyphs.push(g);
            i += 1;
            continue;
        }
        x += advance;
        cur.glyphs.push(g);
        i += 1;
    }
    let total = text.len() as u32;
    finish(&mut cur, &mut lines, total);

    let layout_w = if max_w.is_finite() { max_w } else { 0.0 };
    let block_h = height * lines.len() as f32;
    let top = match fmt.para_align {
        1 if max_h.is_finite() => max_h - block_h,
        2 if max_h.is_finite() => (max_h - block_h) / 2.0,
        _ => 0.0,
    };
    let mut width = 0.0f32;
    let mut width_ws = 0.0f32;
    let rtl = fmt.reading == 1;
    for (k, l) in lines.iter_mut().enumerate() {
        let free = layout_w - l.width;
        // Alignement : 0 début, 1 fin, 2 centre, 3 justifié (traité comme début).
        let align = match (fmt.text_align, rtl) {
            (0 | 3, true) => 1,
            (1, true) => 0,
            (a, _) => a,
        };
        l.left = match align {
            1 => free,
            2 => free / 2.0,
            _ => 0.0,
        };
        l.top = top + height * k as f32;
        l.height = height;
        l.baseline = baseline;
        width = width.max(l.width);
        width_ws = width_ws.max(l.width_ws);
    }
    Lines {
        fonts,
        lines,
        top,
        width,
        width_ws,
        height: block_h,
    }
}

fn resolve_glyph(
    fams: &[Family],
    fonts: &mut Vec<(FaceInfo, Arc<sfnt::Font>)>,
    c: u32,
    weight: u32,
    italic: bool,
) -> (usize, u16) {
    let c = if c == 0x09 { 0x20 } else { c };
    if let Some((_, f)) = fonts.first() {
        let g = f.glyph_index(c);
        // Un blanc absent garde la police principale : pas de secours.
        if g != 0 || c < 0x20 || is_break_space(c) || c == 0xA0 {
            return (0, g);
        }
    }
    for (k, (_, f)) in fonts.iter().enumerate().skip(1) {
        let g = f.glyph_index(c);
        if g != 0 {
            return (k, g);
        }
    }
    if let Some((fi, f)) = fallback_for(fams, c, weight, italic) {
        let g = f.glyph_index(c);
        fonts.push((fi, f));
        return (fonts.len() - 1, g);
    }
    (0, 0)
}

unsafe fn lines_of(this: *mut TextObj) -> Option<Arc<Lines>> {
    let mut t = text(this);
    let fmt = t.fmt.clone();
    let l = t.layout.as_mut()?;
    if l.lines.is_none() {
        l.lines = Some(Arc::new(build(&fmt, &l.text, l.max_w, l.max_h)));
    }
    l.lines.clone()
}

fn invalidate(t: &mut Text) {
    if let Some(l) = t.layout.as_mut() {
        l.lines = None;
    }
}

/// Contours de la mise en page `layout` (objet Winlux) posée en `origin`,
/// ajoutés à `path` pour le rendu Direct2D. Faux si l'objet est étranger.
pub unsafe fn layout_outline(layout: *mut c_void, origin: Pt, path: &mut Path, scale: f32) -> bool {
    if layout.is_null() || *(layout as *const *const ()) != LAYOUT_VT.ptr() {
        return false;
    }
    let this = layout as *mut TextObj;
    let Some(lines) = lines_of(this) else {
        return false;
    };
    let (underline, strike, size) = {
        let t = text(this);
        let l = t.layout.as_ref().unwrap();
        (l.underline, l.strike, t.fmt.size)
    };
    let mut sink = PathSink { path, scale };
    for line in &lines.lines {
        let base = (origin.0 + line.left, origin.1 + line.top + line.baseline);
        for g in &line.glyphs {
            if let Some((_, f)) = lines.fonts.get(g.font) {
                outline_glyphs(
                    f,
                    size,
                    &[(g.gid, g.advance, (0.0, 0.0))],
                    (base.0 + g.x, base.1),
                    &mut sink,
                );
            }
        }
        if let Some((_, f)) = lines.fonts.first() {
            let s = size / f.metrics.units_per_em as f32;
            let m = &f.metrics;
            let bar = |pos: i16, thick: u16| {
                let t = (thick as f32 * s).max(1.0);
                let y = base.1 - pos as f32 * s;
                Path::rect(base.0, y - t / 2.0, base.0 + line.width, y + t / 2.0)
            };
            if underline && line.width > 0.0 {
                sink.path
                    .append(&bar(m.underline_position, m.underline_thickness));
            }
            if strike && line.width > 0.0 {
                sink.path
                    .append(&bar(m.strikeout_position, m.strikeout_thickness));
            }
        }
    }
    true
}

/// Contours d'un DWRITE_GLYPH_RUN dont la face vient de Winlux.
pub unsafe fn glyph_run_outline(
    run: *const GlyphRun,
    origin: Pt,
    path: &mut Path,
    scale: f32,
) -> bool {
    let Some(run) = run.as_ref() else {
        return false;
    };
    let Some(face) = face_of(run.face) else {
        return false;
    };
    if run.indices.is_null() && run.count > 0 {
        return false;
    }
    let glyphs = run_glyphs(
        &face.font,
        run.em_size,
        run.indices,
        run.advances,
        run.offsets,
        run.count,
    );
    let mut sink = PathSink { path, scale };
    outline_glyphs(&face.font, run.em_size, &glyphs, origin, &mut sink);
    true
}

/// Contours d'un texte simple (ID2D1RenderTarget::DrawText).
pub unsafe fn text_outline(
    s: &[u16],
    format: *mut c_void,
    rect: (f32, f32, f32, f32),
    path: &mut Path,
    scale: f32,
) -> bool {
    let Some(fmt) = format_of(format) else {
        return false;
    };
    let (l, t, r, b) = rect;
    let state = LayoutState {
        text: s.to_vec(),
        max_w: (r - l).max(0.0),
        max_h: (b - t).max(0.0),
        underline: false,
        strike: false,
        lines: None,
    };
    let obj = Com::create(
        LAYOUT_VT.ptr(),
        Mutex::new(Text {
            fmt,
            layout: Some(state),
        }),
    );
    let ok = layout_outline(obj as *mut c_void, (l, t), path, scale);
    Com::release(obj);
    ok
}

struct PathSink<'a> {
    path: &'a mut Path,
    scale: f32,
}
impl OutlineSink for PathSink<'_> {
    fn move_to(&mut self, p: Pt) {
        self.path.begin(p)
    }
    fn line_to(&mut self, p: Pt) {
        self.path.line(p)
    }
    fn quad_to(&mut self, c: Pt, p: Pt) {
        self.path.quadratic(c, p, self.scale)
    }
    fn close(&mut self) {
        self.path.close()
    }
}

// Méthodes de IDWriteTextFormat (partagées par IDWriteTextLayout).

unsafe extern "win64" fn format_qi(
    this: *mut TextObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_TEXT_FORMAT])
}
unsafe extern "win64" fn layout_qi(
    this: *mut TextObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_TEXT_FORMAT, IID_TEXT_LAYOUT])
}

macro_rules! setter {
    ($name:ident, $field:ident, $max:expr) => {
        unsafe extern "win64" fn $name(this: *mut TextObj, v: u32) -> i32 {
            if v > $max {
                return E_INVALIDARG;
            }
            let mut t = text(this);
            t.fmt.$field = v;
            invalidate(&mut t);
            S_OK
        }
    };
}
macro_rules! getter {
    ($name:ident, $field:ident) => {
        unsafe extern "win64" fn $name(this: *mut TextObj) -> u32 {
            text(this).fmt.$field
        }
    };
}
setter!(set_text_align, text_align, 3);
setter!(set_para_align, para_align, 2);
setter!(set_wrapping, wrapping, 4);
setter!(set_reading, reading, 3);
setter!(set_flow, flow, 3);
getter!(get_text_align, text_align);
getter!(get_para_align, para_align);
getter!(get_wrapping, wrapping);
getter!(get_reading, reading);
getter!(get_flow, flow);
getter!(get_weight, weight);
getter!(get_style, style);
getter!(get_stretch, stretch);

unsafe extern "win64" fn set_tab(this: *mut TextObj, v: f32) -> i32 {
    if v <= 0.0 {
        return E_INVALIDARG;
    }
    let mut t = text(this);
    t.fmt.tab = Some(v);
    invalidate(&mut t);
    S_OK
}
unsafe extern "win64" fn get_tab(this: *mut TextObj) -> f32 {
    let t = text(this);
    t.fmt.tab.unwrap_or(t.fmt.size * 4.0)
}
unsafe extern "win64" fn set_trimming(
    this: *mut TextObj,
    trimming: *const [u32; 3],
    _sign: *mut c_void,
) -> i32 {
    if let Some(v) = trimming.as_ref() {
        text(this).fmt.trimming = *v;
    }
    S_OK
}
unsafe extern "win64" fn get_trimming(
    this: *mut TextObj,
    trimming: *mut [u32; 3],
    sign: *mut *mut c_void,
) -> i32 {
    if !trimming.is_null() {
        *trimming = text(this).fmt.trimming;
    }
    if !sign.is_null() {
        *sign = std::ptr::null_mut();
    }
    S_OK
}
unsafe extern "win64" fn set_line_spacing(
    this: *mut TextObj,
    method: u32,
    spacing: f32,
    baseline: f32,
) -> i32 {
    let mut t = text(this);
    t.fmt.line_spacing = (method, spacing, baseline);
    invalidate(&mut t);
    S_OK
}
unsafe extern "win64" fn get_line_spacing(
    this: *mut TextObj,
    method: *mut u32,
    spacing: *mut f32,
    baseline: *mut f32,
) -> i32 {
    let (m, s, b) = text(this).fmt.line_spacing;
    if !method.is_null() {
        *method = m;
    }
    if !spacing.is_null() {
        *spacing = s;
    }
    if !baseline.is_null() {
        *baseline = b;
    }
    S_OK
}
unsafe extern "win64" fn get_collection(_this: *mut TextObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_collection())
}
unsafe extern "win64" fn get_family_len(this: *mut TextObj) -> u32 {
    text(this).fmt.family.encode_utf16().count() as u32
}
unsafe extern "win64" fn get_family(this: *mut TextObj, buf: *mut u16, size: u32) -> i32 {
    let f = text(this).fmt.family.clone();
    put_wide(&f, buf, size)
}
unsafe extern "win64" fn get_size(this: *mut TextObj) -> f32 {
    text(this).fmt.size
}
unsafe extern "win64" fn get_locale_len(this: *mut TextObj) -> u32 {
    text(this).fmt.locale.encode_utf16().count() as u32
}
unsafe extern "win64" fn get_locale(this: *mut TextObj, buf: *mut u16, size: u32) -> i32 {
    let l = text(this).fmt.locale.clone();
    put_wide(&l, buf, size)
}

static FORMAT_VT: VTable<28> = VTable([
    format_qi as *const (),
    add_ref::<Mutex<Text>> as *const (),
    release::<Mutex<Text>> as *const (),
    set_text_align as *const (),
    set_para_align as *const (),
    set_wrapping as *const (),
    set_reading as *const (),
    set_flow as *const (),
    set_tab as *const (),
    set_trimming as *const (),
    set_line_spacing as *const (),
    get_text_align as *const (),
    get_para_align as *const (),
    get_wrapping as *const (),
    get_reading as *const (),
    get_flow as *const (),
    get_tab as *const (),
    get_trimming as *const (),
    get_line_spacing as *const (),
    get_collection as *const (),
    get_family_len as *const (),
    get_family as *const (),
    get_weight as *const (),
    get_style as *const (),
    get_stretch as *const (),
    get_size as *const (),
    get_locale_len as *const (),
    get_locale as *const (),
]);

// Méthodes propres à IDWriteTextLayout.

#[repr(C)]
#[derive(Clone, Copy)]
struct Range {
    start: u32,
    len: u32,
}

/// Une plage couvre-t-elle tout le texte ? (attributs par plage simplifiés)
unsafe fn whole(this: *mut TextObj, r: Range) -> bool {
    let t = text(this);
    let n = t.layout.as_ref().map_or(0, |l| l.text.len() as u32);
    r.start == 0 && r.len >= n
}

unsafe fn put_range(out: *mut Range, this: *mut TextObj) {
    if !out.is_null() {
        let n = text(this)
            .layout
            .as_ref()
            .map_or(0, |l| l.text.len() as u32);
        *out = Range { start: 0, len: n };
    }
}

unsafe extern "win64" fn set_max_width(this: *mut TextObj, v: f32) -> i32 {
    let mut t = text(this);
    if let Some(l) = t.layout.as_mut() {
        l.max_w = v;
        l.lines = None;
    }
    S_OK
}
unsafe extern "win64" fn set_max_height(this: *mut TextObj, v: f32) -> i32 {
    let mut t = text(this);
    if let Some(l) = t.layout.as_mut() {
        l.max_h = v;
        l.lines = None;
    }
    S_OK
}
unsafe extern "win64" fn get_max_width(this: *mut TextObj) -> f32 {
    text(this).layout.as_ref().map_or(0.0, |l| l.max_w)
}
unsafe extern "win64" fn get_max_height(this: *mut TextObj) -> f32 {
    text(this).layout.as_ref().map_or(0.0, |l| l.max_h)
}
unsafe extern "win64" fn set_range_ok(_this: *mut TextObj, _v: *mut c_void, _r: Range) -> i32 {
    S_OK
}
unsafe extern "win64" fn set_range_family(this: *mut TextObj, name: *const u16, r: Range) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    if whole(this, r) {
        let mut t = text(this);
        t.fmt.family = read_wide(name);
        invalidate(&mut t);
    }
    S_OK
}
macro_rules! range_setter {
    ($name:ident, $field:ident) => {
        unsafe extern "win64" fn $name(this: *mut TextObj, v: u32, r: Range) -> i32 {
            if whole(this, r) {
                let mut t = text(this);
                t.fmt.$field = v;
                invalidate(&mut t);
            }
            S_OK
        }
    };
}
range_setter!(set_range_weight, weight);
range_setter!(set_range_style, style);
range_setter!(set_range_stretch, stretch);
unsafe extern "win64" fn set_range_size(this: *mut TextObj, v: f32, r: Range) -> i32 {
    if v <= 0.0 {
        return E_INVALIDARG;
    }
    if whole(this, r) {
        let mut t = text(this);
        t.fmt.size = v;
        invalidate(&mut t);
    }
    S_OK
}
unsafe extern "win64" fn set_range_underline(this: *mut TextObj, v: i32, r: Range) -> i32 {
    if whole(this, r) {
        if let Some(l) = text(this).layout.as_mut() {
            l.underline = v != 0;
        }
    }
    S_OK
}
unsafe extern "win64" fn set_range_strike(this: *mut TextObj, v: i32, r: Range) -> i32 {
    if whole(this, r) {
        if let Some(l) = text(this).layout.as_mut() {
            l.strike = v != 0;
        }
    }
    S_OK
}
unsafe extern "win64" fn set_range_locale(this: *mut TextObj, name: *const u16, r: Range) -> i32 {
    if whole(this, r) && !name.is_null() {
        text(this).fmt.locale = read_wide(name);
    }
    S_OK
}
unsafe extern "win64" fn get_pos_collection(
    this: *mut TextObj,
    _pos: u32,
    out: *mut *mut c_void,
    r: *mut Range,
) -> i32 {
    put_range(r, this);
    get_collection(this, out)
}
unsafe extern "win64" fn get_pos_family_len(
    this: *mut TextObj,
    _pos: u32,
    len: *mut u32,
    r: *mut Range,
) -> i32 {
    put_range(r, this);
    if !len.is_null() {
        *len = get_family_len(this);
    }
    S_OK
}
unsafe extern "win64" fn get_pos_family(
    this: *mut TextObj,
    _pos: u32,
    buf: *mut u16,
    size: u32,
    r: *mut Range,
) -> i32 {
    put_range(r, this);
    get_family(this, buf, size)
}
macro_rules! pos_getter {
    ($name:ident, $ty:ty, $get:expr) => {
        unsafe extern "win64" fn $name(
            this: *mut TextObj,
            _pos: u32,
            out: *mut $ty,
            r: *mut Range,
        ) -> i32 {
            put_range(r, this);
            if !out.is_null() {
                *out = $get(this);
            }
            S_OK
        }
    };
}
pos_getter!(get_pos_weight, u32, |t| get_weight(t));
pos_getter!(get_pos_style, u32, |t| get_style(t));
pos_getter!(get_pos_stretch, u32, |t| get_stretch(t));
pos_getter!(get_pos_size, f32, |t| get_size(t));
pos_getter!(get_pos_underline, i32, |t: *mut TextObj| text(t)
    .layout
    .as_ref()
    .is_some_and(|l| l.underline)
    as i32);
pos_getter!(
    get_pos_strike,
    i32,
    |t: *mut TextObj| text(t).layout.as_ref().is_some_and(|l| l.strike) as i32
);
pos_getter!(get_pos_null, *mut c_void, |_t| std::ptr::null_mut());
unsafe extern "win64" fn get_pos_locale_len(
    this: *mut TextObj,
    _pos: u32,
    len: *mut u32,
    r: *mut Range,
) -> i32 {
    put_range(r, this);
    if !len.is_null() {
        *len = get_locale_len(this);
    }
    S_OK
}
unsafe extern "win64" fn get_pos_locale(
    this: *mut TextObj,
    _pos: u32,
    buf: *mut u16,
    size: u32,
    r: *mut Range,
) -> i32 {
    put_range(r, this);
    get_locale(this, buf, size)
}

/// IDWriteTextLayout::Draw : un DrawGlyphRun par suite de glyphes d'une
/// même police sur une ligne.
unsafe extern "win64" fn layout_draw(
    this: *mut TextObj,
    ctx: *mut c_void,
    renderer: *mut c_void,
    x: f32,
    y: f32,
) -> i32 {
    if renderer.is_null() {
        return E_INVALIDARG;
    }
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let size = text(this).fmt.size;
    type DrawRun = unsafe extern "win64" fn(
        *mut c_void,
        *mut c_void,
        f32,
        f32,
        u32,
        *const GlyphRun,
        *const c_void,
        *mut c_void,
    ) -> i32;
    let faces: Vec<*mut FaceObj> = lines
        .fonts
        .iter()
        .map(|(fi, f)| new_face(fi.clone(), f.clone()))
        .collect();
    let mut hr = S_OK;
    'lines: for line in &lines.lines {
        let mut k = 0;
        while k < line.glyphs.len() {
            let font = line.glyphs[k].font;
            let end = line.glyphs[k..]
                .iter()
                .position(|g| g.font != font)
                .map_or(line.glyphs.len(), |e| k + e);
            let part = &line.glyphs[k..end];
            let indices: Vec<u16> = part.iter().map(|g| g.gid).collect();
            let advances: Vec<f32> = part.iter().map(|g| g.advance).collect();
            if let Some(&face) = faces.get(font) {
                let run = GlyphRun {
                    face: face as *mut c_void,
                    em_size: size,
                    count: indices.len() as u32,
                    indices: indices.as_ptr(),
                    advances: advances.as_ptr(),
                    offsets: std::ptr::null(),
                    sideways: 0,
                    bidi_level: 0,
                };
                let bx = x + line.left + part[0].x;
                let by = y + line.top + line.baseline;
                hr = method::<DrawRun>(renderer, 6)(
                    renderer,
                    ctx,
                    bx,
                    by,
                    0,
                    &run,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                );
                if hr < 0 {
                    break 'lines;
                }
            }
            k = end;
        }
    }
    for f in faces {
        Com::release(f);
    }
    hr
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LineMetrics {
    length: u32,
    trailing_ws: u32,
    newline: u32,
    height: f32,
    baseline: f32,
    trimmed: i32,
}

unsafe extern "win64" fn layout_line_metrics(
    this: *mut TextObj,
    out: *mut LineMetrics,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    if !actual.is_null() {
        *actual = lines.lines.len() as u32;
    }
    if (max as usize) < lines.lines.len() || out.is_null() {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    for (i, l) in lines.lines.iter().enumerate() {
        *out.add(i) = LineMetrics {
            length: l.len,
            trailing_ws: l.trailing_ws,
            newline: l.newline,
            height: l.height,
            baseline: l.baseline,
            trimmed: 0,
        };
    }
    S_OK
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TextMetrics {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub width_ws: f32,
    pub height: f32,
    pub layout_width: f32,
    pub layout_height: f32,
    pub bidi_depth: u32,
    pub line_count: u32,
}

unsafe extern "win64" fn layout_metrics(this: *mut TextObj, out: *mut TextMetrics) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let (mw, mh) = text(this)
        .layout
        .as_ref()
        .map_or((0.0, 0.0), |l| (l.max_w, l.max_h));
    let left = lines
        .lines
        .iter()
        .map(|l| l.left)
        .fold(f32::INFINITY, f32::min);
    *out = TextMetrics {
        left: if left.is_finite() { left } else { 0.0 },
        top: lines.top,
        width: lines.width,
        width_ws: lines.width_ws,
        height: lines.height,
        layout_width: mw,
        layout_height: mh,
        bidi_depth: 1,
        line_count: lines.lines.len() as u32,
    };
    S_OK
}

unsafe extern "win64" fn layout_overhang(this: *mut TextObj, out: *mut [f32; 4]) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let (mw, mh) = text(this)
        .layout
        .as_ref()
        .map_or((0.0, 0.0), |l| (l.max_w, l.max_h));
    let left = lines.lines.iter().map(|l| l.left).fold(0.0f32, f32::min);
    *out = [
        -left,
        -lines.top,
        lines.width - mw,
        lines.top + lines.height - mh,
    ];
    S_OK
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ClusterMetrics {
    width: f32,
    length: u16,
    flags: u16,
}

unsafe extern "win64" fn layout_clusters(
    this: *mut TextObj,
    out: *mut ClusterMetrics,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let mut v = Vec::new();
    for l in &lines.lines {
        for g in &l.glyphs {
            // drapeaux : 1 coupure possible après, 2 espace
            let flags = if g.space { 3 } else { 0 };
            v.push(ClusterMetrics {
                width: g.advance,
                length: g.len as u16,
                flags,
            });
        }
        if l.newline > 0 {
            v.push(ClusterMetrics {
                width: 0.0,
                length: l.newline as u16,
                flags: 1 | 2 | 4, // coupure, espace, saut de ligne
            });
        }
    }
    if !actual.is_null() {
        *actual = v.len() as u32;
    }
    if (max as usize) < v.len() || out.is_null() {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len());
    S_OK
}

unsafe extern "win64" fn layout_min_width(this: *mut TextObj, out: *mut f32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let mut best = 0.0f32;
    for l in &lines.lines {
        let mut w = 0.0;
        for g in &l.glyphs {
            if g.space {
                w = 0.0;
            } else {
                w += g.advance;
                best = best.max(w);
            }
        }
    }
    *out = best;
    S_OK
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct HitMetrics {
    pos: u32,
    len: u32,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    bidi: u32,
    is_text: i32,
    trimmed: i32,
}

fn line_at(lines: &Lines, y: f32) -> usize {
    lines
        .lines
        .iter()
        .position(|l| y < l.top + l.height)
        .unwrap_or(lines.lines.len().saturating_sub(1))
}

unsafe extern "win64" fn layout_hit_point(
    this: *mut TextObj,
    x: f32,
    y: f32,
    trailing: *mut i32,
    inside: *mut i32,
    out: *mut HitMetrics,
) -> i32 {
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let mut hit = HitMetrics::default();
    let (mut tr, mut ins) = (0, 0);
    if let Some(l) = lines.lines.get(line_at(&lines, y)) {
        ins = (y >= l.top && y < l.top + l.height) as i32;
        hit.top = l.top;
        hit.height = l.height;
        let lx = x - l.left;
        match l.glyphs.iter().find(|g| lx < g.x + g.advance) {
            Some(g) if lx >= 0.0 => {
                tr = (lx >= g.x + g.advance / 2.0) as i32;
                hit.pos = g.pos;
                hit.len = g.len;
                hit.left = l.left + g.x;
                hit.width = g.advance;
                hit.is_text = 1;
                ins &= 1;
            }
            Some(g) => {
                hit.pos = g.pos;
                hit.len = g.len;
                hit.left = l.left + g.x;
                hit.width = g.advance;
                hit.is_text = 1;
                ins = 0;
            }
            None => {
                ins = 0;
                match l.glyphs.last() {
                    Some(g) => {
                        tr = 1;
                        hit.pos = g.pos;
                        hit.len = g.len;
                        hit.left = l.left + g.x;
                        hit.width = g.advance;
                        hit.is_text = 1;
                    }
                    None => {
                        hit.pos = l.start;
                        hit.left = l.left;
                    }
                }
            }
        }
    }
    if !trailing.is_null() {
        *trailing = tr;
    }
    if !inside.is_null() {
        *inside = ins;
    }
    if !out.is_null() {
        *out = hit;
    }
    S_OK
}

unsafe extern "win64" fn layout_hit_position(
    this: *mut TextObj,
    pos: u32,
    trailing: i32,
    x: *mut f32,
    y: *mut f32,
    out: *mut HitMetrics,
) -> i32 {
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let mut hit = HitMetrics::default();
    let (mut px, mut py) = (0.0, lines.top);
    let line = lines
        .lines
        .iter()
        .find(|l| pos < l.start + l.len)
        .or(lines.lines.last());
    if let Some(l) = line {
        py = l.top;
        hit.top = l.top;
        hit.height = l.height;
        hit.pos = pos;
        match l
            .glyphs
            .iter()
            .find(|g| pos >= g.pos && pos < g.pos + g.len)
        {
            Some(g) => {
                hit.len = g.len;
                hit.left = l.left + g.x;
                hit.width = g.advance;
                hit.is_text = 1;
                px = hit.left + if trailing != 0 { g.advance } else { 0.0 };
            }
            None => {
                px = l.left + l.width_ws;
                hit.left = px;
            }
        }
    }
    if !x.is_null() {
        *x = px;
    }
    if !y.is_null() {
        *y = py;
    }
    if !out.is_null() {
        *out = hit;
    }
    S_OK
}

unsafe extern "win64" fn layout_hit_range(
    this: *mut TextObj,
    pos: u32,
    len: u32,
    ox: f32,
    oy: f32,
    out: *mut HitMetrics,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let Some(lines) = lines_of(this) else {
        return E_INVALIDARG;
    };
    let end = pos.saturating_add(len);
    let mut v = Vec::new();
    for l in &lines.lines {
        let gs: Vec<&Glyph> = l
            .glyphs
            .iter()
            .filter(|g| g.pos < end && g.pos + g.len > pos)
            .collect();
        if let (Some(a), Some(b)) = (gs.first(), gs.last()) {
            v.push(HitMetrics {
                pos: a.pos,
                len: b.pos + b.len - a.pos,
                left: ox + l.left + a.x,
                top: oy + l.top,
                width: b.x + b.advance - a.x,
                height: l.height,
                bidi: 0,
                is_text: 1,
                trimmed: 0,
            });
        }
    }
    if !actual.is_null() {
        *actual = v.len() as u32;
    }
    if (max as usize) < v.len() || (out.is_null() && !v.is_empty()) {
        return E_NOT_SUFFICIENT_BUFFER;
    }
    if !v.is_empty() {
        std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len());
    }
    S_OK
}

static LAYOUT_VT: VTable<67> = VTable([
    layout_qi as *const (),
    add_ref::<Mutex<Text>> as *const (),
    release::<Mutex<Text>> as *const (),
    set_text_align as *const (),
    set_para_align as *const (),
    set_wrapping as *const (),
    set_reading as *const (),
    set_flow as *const (),
    set_tab as *const (),
    set_trimming as *const (),
    set_line_spacing as *const (),
    get_text_align as *const (),
    get_para_align as *const (),
    get_wrapping as *const (),
    get_reading as *const (),
    get_flow as *const (),
    get_tab as *const (),
    get_trimming as *const (),
    get_line_spacing as *const (),
    get_collection as *const (),
    get_family_len as *const (),
    get_family as *const (),
    get_weight as *const (),
    get_style as *const (),
    get_stretch as *const (),
    get_size as *const (),
    get_locale_len as *const (),
    get_locale as *const (),
    // IDWriteTextLayout
    set_max_width as *const (),
    set_max_height as *const (),
    set_range_ok as *const (), // SetFontCollection
    set_range_family as *const (),
    set_range_weight as *const (),
    set_range_style as *const (),
    set_range_stretch as *const (),
    set_range_size as *const (),
    set_range_underline as *const (),
    set_range_strike as *const (),
    set_range_ok as *const (), // SetDrawingEffect
    set_range_ok as *const (), // SetInlineObject
    set_range_ok as *const (), // SetTypography
    set_range_locale as *const (),
    get_max_width as *const (),
    get_max_height as *const (),
    get_pos_collection as *const (),
    get_pos_family_len as *const (),
    get_pos_family as *const (),
    get_pos_weight as *const (),
    get_pos_style as *const (),
    get_pos_stretch as *const (),
    get_pos_size as *const (),
    get_pos_underline as *const (),
    get_pos_strike as *const (),
    get_pos_null as *const (), // GetDrawingEffect
    get_pos_null as *const (), // GetInlineObject
    get_pos_null as *const (), // GetTypography
    get_pos_locale_len as *const (),
    get_pos_locale as *const (),
    layout_draw as *const (),
    layout_line_metrics as *const (),
    layout_metrics as *const (),
    layout_overhang as *const (),
    layout_clusters as *const (),
    layout_min_width as *const (),
    layout_hit_point as *const (),
    layout_hit_position as *const (),
    layout_hit_range as *const (),
]);

// --- Point d'entrée -------------------------------------------------------------------

/// DWriteCreateFactory(type, riid, out). La fabrique partagée et la
/// fabrique isolée sont équivalentes ici.
#[allow(non_snake_case)]
pub unsafe extern "win64" fn DWriteCreateFactory(
    _kind: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let f = Com::create(FACTORY_VT.ptr(), ());
    let hr = factory_qi(f, riid, out);
    Com::release(f);
    hr
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DWriteCreateFactory" => fn DWriteCreateFactory,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    type Unary = unsafe extern "win64" fn(*mut c_void) -> u32;

    /// Police de test « Winlux Test » (voir `sfnt::tests::tiny_font`),
    /// écrite une fois sur disque et ajoutée au catalogue.
    pub fn test_font() -> PathBuf {
        static P: OnceLock<PathBuf> = OnceLock::new();
        P.get_or_init(|| {
            let p =
                std::env::temp_dir().join(format!("winlux-dwrite-test-{}.ttf", std::process::id()));
            std::fs::write(&p, sfnt::tests::tiny_font()).unwrap();
            register_face(FaceInfo {
                family: "Winlux Test".into(),
                style: "Regular".into(),
                weight: 400,
                italic: false,
                path: p.clone(),
                index: 0,
            });
            p
        })
        .clone()
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub unsafe fn factory() -> *mut c_void {
        let mut f = std::ptr::null_mut();
        assert_eq!(DWriteCreateFactory(0, &IID_DWRITE_FACTORY, &mut f), S_OK);
        f
    }

    /// Mise en page de `s` en « Winlux Test » 10 DIP.
    pub unsafe fn layout(f: *mut c_void, s: &str, max_w: f32) -> *mut c_void {
        test_font();
        type Fmt = unsafe extern "win64" fn(
            *mut c_void,
            *const u16,
            *mut c_void,
            u32,
            u32,
            u32,
            f32,
            *const u16,
            *mut *mut c_void,
        ) -> i32;
        let mut fmt = std::ptr::null_mut();
        let fam = wide("winlux test");
        let loc = wide("fr-fr");
        assert_eq!(
            method::<Fmt>(f, 15)(
                f,
                fam.as_ptr(),
                std::ptr::null_mut(),
                400,
                0,
                5,
                10.0,
                loc.as_ptr(),
                &mut fmt
            ),
            S_OK
        );
        type Lay = unsafe extern "win64" fn(
            *mut c_void,
            *const u16,
            u32,
            *mut c_void,
            f32,
            f32,
            *mut *mut c_void,
        ) -> i32;
        let text: Vec<u16> = s.encode_utf16().collect();
        let mut l = std::ptr::null_mut();
        assert_eq!(
            method::<Lay>(f, 18)(
                f,
                text.as_ptr(),
                text.len() as u32,
                fmt,
                max_w,
                100.0,
                &mut l
            ),
            S_OK
        );
        method::<Unary>(fmt, 2)(fmt);
        l
    }

    #[test]
    fn face_depuis_un_fichier() {
        unsafe {
            let f = factory();
            let path = wide(test_font().to_str().unwrap());
            type FileRef = unsafe extern "win64" fn(
                *mut c_void,
                *const u16,
                *const u64,
                *mut *mut c_void,
            ) -> i32;
            let mut file = std::ptr::null_mut();
            assert_eq!(
                method::<FileRef>(f, 7)(f, path.as_ptr(), std::ptr::null(), &mut file),
                S_OK
            );
            type Analyze = unsafe extern "win64" fn(
                *mut c_void,
                *mut i32,
                *mut u32,
                *mut u32,
                *mut u32,
            ) -> i32;
            let (mut ok, mut ft, mut fc, mut n) = (0, 0, 0, 0);
            assert_eq!(
                method::<Analyze>(file, 5)(file, &mut ok, &mut ft, &mut fc, &mut n),
                S_OK
            );
            assert_eq!((ok, ft, fc, n), (1, 3, 1, 1));
            type MkFace = unsafe extern "win64" fn(
                *mut c_void,
                u32,
                u32,
                *const *mut c_void,
                u32,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let mut face = std::ptr::null_mut();
            assert_eq!(
                method::<MkFace>(f, 9)(f, 1, 1, &file, 0, 0, &mut face),
                S_OK
            );
            let cps = ['A' as u32, 'B' as u32, 'Z' as u32];
            let mut gi = [9u16; 3];
            type Indices = unsafe extern "win64" fn(*mut c_void, *const u32, u32, *mut u16) -> i32;
            assert_eq!(
                method::<Indices>(face, 11)(face, cps.as_ptr(), 3, gi.as_mut_ptr()),
                S_OK
            );
            assert_eq!(gi, [1, 2, 0]);
            let mut m = FontMetrics::default();
            method::<unsafe extern "win64" fn(*mut c_void, *mut FontMetrics)>(face, 8)(
                face, &mut m,
            );
            assert_eq!(
                (m.units_per_em, m.ascent, m.descent, m.line_gap),
                (1000, 800, 200, 90)
            );
            let mut gm = GlyphMetrics::default();
            type Design = unsafe extern "win64" fn(
                *mut c_void,
                *const u16,
                u32,
                *mut GlyphMetrics,
                i32,
            ) -> i32;
            assert_eq!(method::<Design>(face, 10)(face, &1, 1, &mut gm, 0), S_OK);
            assert_eq!(
                (
                    gm.left_side_bearing,
                    gm.advance_width,
                    gm.right_side_bearing,
                    gm.top_side_bearing
                ),
                (100, 700, 100, 200)
            );
            for o in [face, file, f] {
                method::<Unary>(o, 2)(o);
            }
        }
    }

    #[test]
    fn collection_et_polices_de_remplacement() {
        unsafe {
            test_font();
            let f = factory();
            let mut c = std::ptr::null_mut();
            type Sys = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void, i32) -> i32;
            assert_eq!(method::<Sys>(f, 3)(f, &mut c, 0), S_OK);
            let name = wide("WINLUX TEST");
            let (mut idx, mut exists) = (0u32, 0i32);
            type Find =
                unsafe extern "win64" fn(*mut c_void, *const u16, *mut u32, *mut i32) -> i32;
            assert_eq!(
                method::<Find>(c, 5)(c, name.as_ptr(), &mut idx, &mut exists),
                S_OK
            );
            assert_eq!(exists, 1);
            let mut fam = std::ptr::null_mut();
            type Get = unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
            assert_eq!(method::<Get>(c, 4)(c, idx, &mut fam), S_OK);
            let mut names = std::ptr::null_mut();
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32>(fam, 6)(
                    fam, &mut names
                ),
                S_OK
            );
            let mut buf = [0u16; 32];
            type Str = unsafe extern "win64" fn(*mut c_void, u32, *mut u16, u32) -> i32;
            assert_eq!(
                method::<Str>(names, 8)(names, 0, buf.as_mut_ptr(), 32),
                S_OK
            );
            assert_eq!(String::from_utf16_lossy(&buf[..11]), "Winlux Test");
            let mut font = std::ptr::null_mut();
            type First =
                unsafe extern "win64" fn(*mut c_void, u32, u32, u32, *mut *mut c_void) -> i32;
            assert_eq!(method::<First>(fam, 7)(fam, 700, 5, 0, &mut font), S_OK);
            let mut has = 0;
            type Has = unsafe extern "win64" fn(*mut c_void, u32, *mut i32) -> i32;
            assert_eq!(method::<Has>(font, 12)(font, 'C' as u32, &mut has), S_OK);
            assert_eq!(has, 1);
            for o in [font, names, fam, c, f] {
                method::<Unary>(o, 2)(o);
            }
            // Un nom Windows absent se rabat sur une police présente.
            let fams = families();
            assert!(match_face(&fams, "Segoe UI", 400, false).is_some());
        }
    }

    #[test]
    fn mise_en_page_coupure_et_metriques() {
        unsafe {
            let f = factory();
            let l = layout(f, "AB AB\nA", 15.0);
            let mut tm = TextMetrics::default();
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void, *mut TextMetrics) -> i32>(l, 60)(
                    l, &mut tm
                ),
                S_OK
            );
            assert_eq!(tm.line_count, 3);
            assert!((tm.width - 14.0).abs() < 1e-4, "{tm:?}");
            assert!((tm.height - 32.7).abs() < 1e-3, "{tm:?}");
            let mut lm = [LineMetrics::default(); 3];
            let mut n = 0u32;
            type Lm = unsafe extern "win64" fn(*mut c_void, *mut LineMetrics, u32, *mut u32) -> i32;
            assert_eq!(
                method::<Lm>(l, 59)(l, lm.as_mut_ptr(), 1, &mut n),
                E_NOT_SUFFICIENT_BUFFER
            );
            assert_eq!(n, 3);
            assert_eq!(method::<Lm>(l, 59)(l, lm.as_mut_ptr(), 3, &mut n), S_OK);
            let v: Vec<(u32, u32, u32)> = lm
                .iter()
                .map(|m| (m.length, m.trailing_ws, m.newline))
                .collect();
            assert_eq!(v, [(3, 1, 0), (3, 1, 1), (1, 0, 0)]);
            assert!((lm[0].baseline - 8.0).abs() < 1e-4);
            let (mut x, mut y) = (0.0f32, 0.0f32);
            let mut hm = HitMetrics::default();
            type Pos = unsafe extern "win64" fn(
                *mut c_void,
                u32,
                i32,
                *mut f32,
                *mut f32,
                *mut HitMetrics,
            ) -> i32;
            assert_eq!(method::<Pos>(l, 65)(l, 4, 1, &mut x, &mut y, &mut hm), S_OK);
            assert!(
                (x - 14.0).abs() < 1e-4 && (y - 10.9).abs() < 1e-4,
                "{x} {y}"
            );
            let (mut tr, mut inside) = (0, 0);
            type Pt_ = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                f32,
                *mut i32,
                *mut i32,
                *mut HitMetrics,
            ) -> i32;
            assert_eq!(
                method::<Pt_>(l, 64)(l, 12.0, 1.0, &mut tr, &mut inside, &mut hm),
                S_OK
            );
            assert_eq!((hm.pos, tr, inside), (1, 1, 1));
            let mut w = 0.0f32;
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void, *mut f32) -> i32>(l, 63)(l, &mut w),
                S_OK
            );
            assert!((w - 14.0).abs() < 1e-4);
            // Centrage : la ligne la plus courte est décalée.
            assert_eq!(
                method::<unsafe extern "win64" fn(*mut c_void, u32) -> i32>(l, 3)(l, 2),
                S_OK
            );
            assert_eq!(method::<Pos>(l, 65)(l, 6, 0, &mut x, &mut y, &mut hm), S_OK);
            assert!((x - 4.0).abs() < 1e-4, "{x}");
            for o in [l, f] {
                method::<Unary>(o, 2)(o);
            }
        }
    }

    #[test]
    fn draw_appelle_le_rendu_de_l_appli() {
        unsafe {
            use std::sync::atomic::{AtomicU32, Ordering};
            static RUNS: AtomicU32 = AtomicU32::new(0);
            static GLYPHS: AtomicU32 = AtomicU32::new(0);
            unsafe extern "win64" fn draw_run(
                _t: *mut c_void,
                _ctx: *mut c_void,
                _x: f32,
                y: f32,
                _m: u32,
                run: *const GlyphRun,
                _d: *const c_void,
                _e: *mut c_void,
            ) -> i32 {
                assert!(y > 0.0);
                RUNS.fetch_add(1, Ordering::SeqCst);
                GLYPHS.fetch_add((*run).count, Ordering::SeqCst);
                assert!(face_of((*run).face).is_some());
                S_OK
            }
            unsafe extern "win64" fn zero() -> u32 {
                0
            }
            static RENDERER: VTable<10> = VTable([
                zero as *const (),
                zero as *const (),
                zero as *const (),
                zero as *const (),
                zero as *const (),
                zero as *const (),
                draw_run as *const (),
                zero as *const (),
                zero as *const (),
                zero as *const (),
            ]);
            let mut obj: *const () = RENDERER.ptr();
            let renderer = &mut obj as *mut *const () as *mut c_void;
            let f = factory();
            let l = layout(f, "AB\nC", 100.0);
            type Draw =
                unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void, f32, f32) -> i32;
            assert_eq!(
                method::<Draw>(l, 58)(l, std::ptr::null_mut(), renderer, 0.0, 0.0),
                S_OK
            );
            assert_eq!(RUNS.load(Ordering::SeqCst), 2);
            assert_eq!(GLYPHS.load(Ordering::SeqCst), 3);
            for o in [l, f] {
                method::<Unary>(o, 2)(o);
            }
        }
    }
}
