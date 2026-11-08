//! IDWriteFactory1, IDWriteFactory2 et la police de repli du système
//! (IDWriteFontFallback), d'après la documentation publique de DirectWrite
//! (dwrite_1.h, dwrite_2.h) (#672).
//!
//! Chromium (Claude pour Windows, Electron) et Scintilla demandent
//! IDWriteFactory2 puis appellent `GetSystemFontFallback` et
//! `IDWriteFontFallback::MapCharacters` pour chaque bout de texte, sans
//! prévoir d'échec : la police de repli est donc réelle. Pour chaque
//! caractère, la famille demandée est gardée si elle a le glyphe ; sinon la
//! première police installée qui l'a est choisie (liste de secours de
//! DirectWrite, puis tout le catalogue), et la plage s'étend tant que cette
//! police convient. Un caractère qu'aucune police n'a rend `mappedFont`
//! nul, comme sous Windows.
//!
//! Le reste répond de façon cohérente : collection EUDC vide, paramètres de
//! rendu personnalisés, aucune police en couleur (`DWRITE_E_NOCOLOR`).
//! Le constructeur de repli personnalisé (IDWriteFontFallbackBuilder)
//! garde les correspondances plages Unicode → familles de l'appli, puis
//! le repli du système s'il a été ajouté (#984).

use super::{
    families, match_face, pick, read_wide, CollectionObj, FaceInfo, Family, COLLECTION_VT, FONT_VT,
};
use crate::dlls::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use crate::dlls::d2d1::{add_ref, qi_list, release};
use crate::dlls::ole32::Guid;
use crate::dlls::sfnt;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

/// La suite de glyphes n'a aucun glyphe en couleur (TranslateColorGlyphRun).
pub const DWRITE_E_NOCOLOR: i32 = 0x8898_500Cu32 as i32;

pub const IID_DWRITE_FACTORY1: Guid = guid(
    0x3057_2f99,
    0xdac6,
    0x41db,
    [0xa1, 0x6e, 0x04, 0x86, 0x30, 0x7e, 0x60, 0x6a],
);
pub const IID_DWRITE_FACTORY2: Guid = guid(
    0x0439_fc60,
    0xca44,
    0x4994,
    [0x8d, 0xee, 0x3a, 0x9a, 0xf7, 0xb7, 0x32, 0xec],
);
pub const IID_FONT_FALLBACK: Guid = guid(
    0xefa0_08f9,
    0xf7a1,
    0x48bf,
    [0xb0, 0x5c, 0xf2, 0x24, 0x71, 0x3c, 0xc0, 0xff],
);
pub const IID_RENDERING_PARAMS1: Guid = guid(
    0x9441_3cf4,
    0xa6fc,
    0x4248,
    [0x8b, 0x50, 0x66, 0x74, 0x34, 0x8f, 0xca, 0xd3],
);
pub const IID_RENDERING_PARAMS3: Guid = guid(
    0xb7924baa,
    0x391b,
    0x412a,
    [0x8c, 0x5c, 0xe4, 0x4c, 0xc2, 0xd8, 0x67, 0xdc],
);
pub const IID_RENDERING_PARAMS2: Guid = guid(
    0xf9d7_11c3,
    0x9777,
    0x40ae,
    [0x87, 0xe8, 0x3e, 0x5a, 0xf9, 0xbf, 0x09, 0x48],
);

// --- Méthodes ajoutées par IDWriteFactory1 et IDWriteFactory2 --------------------------

/// IDWriteFactory1::GetEudcFontCollection : aucun caractère défini par
/// l'utilisateur sous Winlux, la collection est vide.
pub unsafe extern "win64" fn eudc_collection(
    _this: *mut c_void,
    out: *mut *mut c_void,
    _check: i32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(COLLECTION_VT.ptr(), Vec::<Family>::new()))
}

/// IDWriteFactory1::CreateCustomRenderingParams (IDWriteRenderingParams1)
/// et IDWriteFactory2::CreateCustomRenderingParams (IDWriteRenderingParams2).
pub unsafe extern "win64" fn custom_params1(
    _this: *mut c_void,
    gamma: f32,
    contrast: f32,
    gray_contrast: f32,
    cleartype: f32,
    geometry: u32,
    mode: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_params(
        gamma,
        contrast,
        gray_contrast,
        cleartype,
        geometry,
        mode,
        0,
        out,
    )
}
pub unsafe extern "win64" fn custom_params2(
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
    new_params(
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

/// IDWriteFactory2::GetSystemFontFallback.
pub unsafe extern "win64" fn system_fallback(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(FALLBACK_VT.ptr(), ()))
}

/// IDWriteFactory2::CreateFontFallbackBuilder.
pub unsafe extern "win64" fn fallback_builder(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(
        out,
        Com::create(BUILDER_VT.ptr(), Mutex::new(Custom::default())),
    )
}

/// IDWriteFactory2::TranslateColorGlyphRun : aucune police en couleur n'est
/// dessinée en couleur, la suite se dessine telle quelle.
pub unsafe extern "win64" fn translate_color(
    _this: *mut c_void,
    _x: f32,
    _y: f32,
    _run: *const c_void,
    _description: *const c_void,
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

/// IDWriteFactory2::CreateGlyphRunAnalysis : même rendu que la version 3
/// (DWRITE_RENDERING_MODE et DWRITE_RENDERING_MODE1 ont les mêmes valeurs
/// jusqu'à OUTLINE).
pub unsafe extern "win64" fn glyph_run_analysis2(
    this: *mut c_void,
    run: *const super::GlyphRun,
    transform: *const crate::dlls::dwrite_glyphs::DwMatrix,
    mode: u32,
    measuring: u32,
    grid_fit: u32,
    antialias: u32,
    x: f32,
    y: f32,
    out: *mut *mut c_void,
) -> i32 {
    super::sets::glyph_run_analysis3(
        this, run, transform, mode, measuring, grid_fit, antialias, x, y, out,
    )
}

// --- IDWriteRenderingParams1 et 2 ------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub gamma: f32,
    pub contrast: f32,
    pub gray_contrast: f32,
    pub cleartype: f32,
    pub geometry: u32,
    pub mode: u32,
    pub grid_fit: u32,
}

type ParamsObj = Com<Params>;

unsafe fn new_params(
    gamma: f32,
    contrast: f32,
    gray_contrast: f32,
    cleartype: f32,
    geometry: u32,
    mode: u32,
    grid_fit: u32,
    out: *mut *mut c_void,
) -> i32 {
    // DWRITE_RENDERING_MODE s'arrête à OUTLINE (6).
    if mode > 6 {
        if !out.is_null() {
            *out = std::ptr::null_mut();
        }
        return E_INVALIDARG;
    }
    new_params3(
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

/// Paramètres par défaut de Windows (IDWriteFactory::CreateRenderingParams).
pub(super) unsafe fn default_params(out: *mut *mut c_void) -> i32 {
    new_params3(2.2, 1.0, 1.0, 1.0, 1, 5, 0, out)
}

/// IDWriteRenderingParams3 : DWRITE_RENDERING_MODE1 va jusqu'à
/// NATURAL_SYMMETRIC_DOWNSAMPLED (7).
pub(super) unsafe fn new_params3(
    gamma: f32,
    contrast: f32,
    gray_contrast: f32,
    cleartype: f32,
    geometry: u32,
    mode: u32,
    grid_fit: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // Plages de la documentation : gamma > 0, contrastes et ClearType >= 0.
    if !(gamma > 0.0 && gamma <= 256.0)
        || contrast < 0.0
        || gray_contrast < 0.0
        || !(0.0..=1.0).contains(&cleartype)
        || geometry > 2
        || mode > 7
        || grid_fit > 2
    {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    let p = Params {
        gamma,
        contrast,
        gray_contrast,
        cleartype,
        geometry,
        mode,
        grid_fit,
    };
    put(out, Com::create(PARAMS_VT.ptr(), p))
}
unsafe extern "win64" fn params_qi(
    this: *mut ParamsObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(
        this,
        riid,
        out,
        &[
            super::IID_RENDERING_PARAMS,
            IID_RENDERING_PARAMS1,
            IID_RENDERING_PARAMS2,
            IID_RENDERING_PARAMS3,
        ],
    )
}
unsafe extern "win64" fn params_gamma(this: *mut ParamsObj) -> f32 {
    Com::get(this).gamma
}
unsafe extern "win64" fn params_contrast(this: *mut ParamsObj) -> f32 {
    Com::get(this).contrast
}
unsafe extern "win64" fn params_cleartype(this: *mut ParamsObj) -> f32 {
    Com::get(this).cleartype
}
unsafe extern "win64" fn params_geometry(this: *mut ParamsObj) -> u32 {
    Com::get(this).geometry
}
unsafe extern "win64" fn params_mode(this: *mut ParamsObj) -> u32 {
    // NATURAL_SYMMETRIC_DOWNSAMPLED n'existe qu'en DWRITE_RENDERING_MODE1.
    match Com::get(this).mode {
        7 => 5,
        m => m,
    }
}
unsafe extern "win64" fn params_mode1(this: *mut ParamsObj) -> u32 {
    Com::get(this).mode
}
unsafe extern "win64" fn params_gray_contrast(this: *mut ParamsObj) -> f32 {
    Com::get(this).gray_contrast
}
unsafe extern "win64" fn params_grid_fit(this: *mut ParamsObj) -> u32 {
    Com::get(this).grid_fit
}
static PARAMS_VT: VTable<11> = VTable([
    params_qi as *const (),
    add_ref::<Params> as *const (),
    release::<Params> as *const (),
    params_gamma as *const (),
    params_contrast as *const (),
    params_cleartype as *const (),
    params_geometry as *const (),
    params_mode as *const (),
    params_gray_contrast as *const (), // IDWriteRenderingParams1
    params_grid_fit as *const (),      // IDWriteRenderingParams2
    params_mode1 as *const (),         // IDWriteRenderingParams3
]);

// --- IDWriteFontFallback ----------------------------------------------------------------

type FallbackObj = Com<()>;

unsafe extern "win64" fn fallback_qi(
    this: *mut FallbackObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_FALLBACK])
}

/// Familles de la collection de base : la nôtre telle quelle, sinon celles
/// du système (collection propre à l'appli, ou aucune).
unsafe fn base_families(collection: *mut c_void) -> Vec<Family> {
    if !collection.is_null() && *(collection as *const *const ()) == COLLECTION_VT.ptr() {
        let fams = Com::get(collection as *mut CollectionObj);
        if !fams.is_empty() {
            return fams.clone();
        }
    }
    families()
}

/// Polices essayées d'abord pour un caractère absent, avant tout le catalogue.
const CHAIN: &[&str] = &[
    "Noto Sans",
    "DejaVu Sans",
    "Noto Sans Symbols",
    "Noto Sans Symbols2",
    "Noto Sans Math",
    "Noto Sans Arabic",
    "Noto Sans Hebrew",
    "Noto Sans Devanagari",
    "Noto Sans Thai",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Droid Sans Fallback",
    "Noto Color Emoji",
    "DejaVu Sans Mono",
    "FreeSans",
    "FreeSerif",
    "Unifont",
];

fn has_glyph(face: &FaceInfo, ch: u32) -> bool {
    sfnt::load(&face.path, face.index).is_some_and(|f| f.glyph_index(ch) != 0)
}

/// Famille de secours qui a ce caractère (nom), mémorisée par caractère.
fn fallback_family(fams: &[Family], ch: u32) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = crate::dlls::lock(cache).get(&ch) {
        if found
            .as_ref()
            .is_none_or(|n| fams.iter().any(|f| f.name.eq_ignore_ascii_case(n)))
        {
            return found.clone();
        }
    }
    let regular = |f: &Family| pick(f, 400, false);
    let named = CHAIN
        .iter()
        .filter_map(|n| fams.iter().find(|f| f.name.eq_ignore_ascii_case(n)));
    let found = named
        .chain(fams.iter())
        .find(|f| has_glyph(&regular(f), ch))
        .map(|f| f.name.clone());
    crate::dlls::lock(cache).insert(ch, found.clone());
    found
}

/// Caractères qui suivent la police d'à côté sans en exiger une (espaces,
/// marques de mise en forme, sélecteurs de variante).
fn is_neutral(ch: u32) -> bool {
    matches!(ch, 0x20 | 0xA0 | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x2064 | 0xFE00..=0xFE0F | 0xFEFF)
        || (0xE0100..=0xE01EF).contains(&ch)
}

/// Plage rendue par une police : (longueur UTF-16, face retenue ou aucune).
pub(super) fn map_run(
    fams: &[Family],
    text: &[u16],
    base: &str,
    weight: u32,
    italic: bool,
) -> (u32, Option<FaceInfo>) {
    let chars = super::decode(text);
    let Some(&(first, _, _)) = chars.first() else {
        return (0, None);
    };
    let base_face = match_face(fams, base, weight, italic);
    let base_has = |c: u32| base_face.as_ref().is_some_and(|f| has_glyph(f, c));
    let face_for = |c: u32| -> Option<FaceInfo> {
        if base_has(c) {
            return base_face.clone();
        }
        let name = fallback_family(fams, c)?;
        fams.iter()
            .find(|f| f.name == name)
            .map(|f| pick(f, weight, italic))
    };
    // Un caractère neutre en tête prend la police du premier qui ne l'est pas.
    let lead = chars
        .iter()
        .find(|(c, _, _)| !is_neutral(*c))
        .map_or(first, |x| x.0);
    let chosen = face_for(lead);
    let mut length = 0u32;
    for &(c, _, n) in &chars {
        let fits = if is_neutral(c) {
            true
        } else {
            match &chosen {
                // La police demandée reprend dès qu'elle a le caractère.
                Some(face) if Some(face) == base_face.as_ref() => base_has(c),
                Some(face) => !base_has(c) && has_glyph(face, c),
                None => face_for(c).is_none(),
            }
        };
        if !fits {
            break;
        }
        length += n;
    }
    (length.max(1), chosen)
}

unsafe extern "win64" fn map_characters(
    _this: *mut FallbackObj,
    source: *mut c_void,
    position: u32,
    length: u32,
    collection: *mut c_void,
    family: *const u16,
    weight: u32,
    style: u32,
    _stretch: u32,
    mapped_length: *mut u32,
    mapped_font: *mut *mut c_void,
    scale: *mut f32,
) -> i32 {
    if mapped_length.is_null() || mapped_font.is_null() || scale.is_null() {
        return E_POINTER;
    }
    *mapped_font = std::ptr::null_mut();
    *mapped_length = 0;
    *scale = 1.0;
    if source.is_null() {
        return E_INVALIDARG;
    }
    if length == 0 {
        return S_OK;
    }
    let Some(text) = crate::dlls::dwrite_analyzer::source_text(source, position, length) else {
        return E_INVALIDARG;
    };
    let base = if family.is_null() {
        String::new()
    } else {
        read_wide(family)
    };
    let fams = base_families(collection);
    let (n, face) = map_run(&fams, &text, &base, weight, style != 0);
    *mapped_length = n.min(length);
    if let Some(face) = face {
        *mapped_font = Com::create(FONT_VT.ptr(), face).cast();
    }
    S_OK
}

static FALLBACK_VT: VTable<4> = VTable([
    fallback_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    map_characters as *const (),
]);

// --- IDWriteFontFallbackBuilder et repli personnalisé ------------------------------------

const IID_FALLBACK_BUILDER: Guid = guid(
    0xfd882d06,
    0x8aba,
    0x4fb8,
    [0xb8, 0x49, 0x8b, 0xe8, 0xb7, 0x3e, 0x14, 0xde],
);

/// Une correspondance AddMapping : plages, familles visées dans l'ordre,
/// collection (sinon celle du système) et échelle.
#[derive(Clone, Default)]
struct Mapping {
    ranges: Vec<(u32, u32)>,
    families: Vec<String>,
    collection: Option<Vec<Family>>,
    scale: f32,
}

/// Repli construit par l'appli : ses correspondances, puis celui du
/// système s'il a été ajouté par AddMappings.
#[derive(Clone, Default)]
struct Custom {
    mappings: Vec<Mapping>,
    system: bool,
}

type BuilderObj = Com<Mutex<Custom>>;
type CustomObj = Com<Custom>;

unsafe extern "win64" fn builder_qi(
    this: *mut BuilderObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FALLBACK_BUILDER])
}

unsafe extern "win64" fn builder_add_mapping(
    this: *mut BuilderObj,
    ranges: *const [u32; 2],
    n_ranges: u32,
    names: *const *const u16,
    n_names: u32,
    collection: *mut c_void,
    _locale: *const u16,
    _base: *const u16,
    scale: f32,
) -> i32 {
    if ranges.is_null() || n_ranges == 0 || names.is_null() || n_names == 0 || scale <= 0.0 {
        return E_INVALIDARG;
    }
    let ranges = std::slice::from_raw_parts(ranges, n_ranges as usize)
        .iter()
        .map(|r| (r[0], r[1]))
        .collect();
    let families = std::slice::from_raw_parts(names, n_names as usize)
        .iter()
        .map(|&n| read_wide(n))
        .collect();
    let collection = (!collection.is_null()
        && *(collection as *const *const ()) == COLLECTION_VT.ptr())
    .then(|| Com::get(collection as *mut CollectionObj).clone());
    crate::dlls::lock(Com::get(this)).mappings.push(Mapping {
        ranges,
        families,
        collection,
        scale,
    });
    S_OK
}

unsafe extern "win64" fn builder_add_mappings(this: *mut BuilderObj, other: *mut c_void) -> i32 {
    if other.is_null() {
        return E_INVALIDARG;
    }
    let vt = *(other as *const *const ());
    let mut me = crate::dlls::lock(Com::get(this));
    if vt == FALLBACK_VT.ptr() {
        me.system = true;
    } else if vt == CUSTOM_VT.ptr() {
        let o = Com::get(other as *mut CustomObj);
        me.mappings.extend(o.mappings.iter().cloned());
        me.system |= o.system;
    } else {
        return E_INVALIDARG;
    }
    S_OK
}

unsafe extern "win64" fn builder_create(this: *mut BuilderObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let c = crate::dlls::lock(Com::get(this)).clone();
    put(out, Com::create(CUSTOM_VT.ptr(), c))
}

static BUILDER_VT: VTable<6> = VTable([
    builder_qi as *const (),
    add_ref::<Mutex<Custom>> as *const (),
    release::<Mutex<Custom>> as *const (),
    builder_add_mapping as *const (),
    builder_add_mappings as *const (),
    builder_create as *const (),
]);

impl Mapping {
    fn covers(&self, ch: u32) -> bool {
        self.ranges.iter().any(|&(a, b)| (a..=b).contains(&ch))
    }
    /// Première famille visée qui a le caractère.
    fn face_for(&self, ch: u32, weight: u32, italic: bool) -> Option<FaceInfo> {
        let fams = self.collection.clone().unwrap_or_else(families);
        let named = |n: &str| fams.iter().find(|f| f.name.eq_ignore_ascii_case(n));
        self.families.iter().find_map(|n| {
            // Une famille absente est sautée ; une police Windows absente
            // est servie par sa remplaçante libre.
            let fam = named(n).or_else(|| {
                super::is_windows_font(n)
                    .then(|| super::substitutes(n).iter().find_map(|s| named(s)))
                    .flatten()
            })?;
            let f = pick(fam, weight, italic);
            has_glyph(&f, ch).then_some(f)
        })
    }
}

impl Custom {
    /// Plage rendue par une même police : (longueur UTF-16, face, échelle).
    fn map(
        &self,
        fams: &[Family],
        text: &[u16],
        base: &str,
        weight: u32,
        italic: bool,
    ) -> (u32, Option<FaceInfo>, f32) {
        let chars = super::decode(text);
        let Some(&(first, _, _)) = chars.first() else {
            return (0, None, 1.0);
        };
        let pick_for = |c: u32| -> Option<(usize, FaceInfo)> {
            self.mappings
                .iter()
                .enumerate()
                .filter(|(_, m)| m.covers(c))
                .find_map(|(i, m)| m.face_for(c, weight, italic).map(|f| (i, f)))
        };
        let lead = chars
            .iter()
            .find(|(c, _, _)| !is_neutral(*c))
            .map_or(first, |x| x.0);
        match pick_for(lead) {
            Some((i, face)) => {
                let mut length = 0u32;
                for &(c, _, n) in &chars {
                    let same =
                        is_neutral(c) || pick_for(c).is_some_and(|(j, f)| j == i && f == face);
                    if !same {
                        break;
                    }
                    length += n;
                }
                (length.max(1), Some(face), self.mappings[i].scale)
            }
            None if self.system => {
                let (n, f) = map_run(fams, text, base, weight, italic);
                (n, f, 1.0)
            }
            None => {
                // Rien ne couvre ce caractère : la plage reste sans police
                // jusqu'au prochain caractère couvert.
                let mut length = 0u32;
                for &(c, _, n) in &chars {
                    if length > 0 && !is_neutral(c) && pick_for(c).is_some() {
                        break;
                    }
                    length += n;
                }
                (length.max(1), None, 1.0)
            }
        }
    }
}

unsafe extern "win64" fn custom_qi(
    this: *mut CustomObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_FONT_FALLBACK])
}

unsafe extern "win64" fn custom_map(
    this: *mut CustomObj,
    source: *mut c_void,
    position: u32,
    length: u32,
    collection: *mut c_void,
    family: *const u16,
    weight: u32,
    style: u32,
    _stretch: u32,
    mapped_length: *mut u32,
    mapped_font: *mut *mut c_void,
    scale: *mut f32,
) -> i32 {
    if mapped_length.is_null() || mapped_font.is_null() || scale.is_null() {
        return E_POINTER;
    }
    *mapped_font = std::ptr::null_mut();
    *mapped_length = 0;
    *scale = 1.0;
    if source.is_null() {
        return E_INVALIDARG;
    }
    if length == 0 {
        return S_OK;
    }
    let Some(text) = crate::dlls::dwrite_analyzer::source_text(source, position, length) else {
        return E_INVALIDARG;
    };
    let base = read_wide(family);
    let fams = base_families(collection);
    let (n, face, s) = Com::get(this).map(&fams, &text, &base, weight, style != 0);
    *mapped_length = n.min(length);
    *scale = s;
    if let Some(face) = face {
        *mapped_font = Com::create(FONT_VT.ptr(), face).cast();
    }
    S_OK
}

static CUSTOM_VT: VTable<4> = VTable([
    custom_qi as *const (),
    add_ref::<Custom> as *const (),
    release::<Custom> as *const (),
    custom_map as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn system() -> Vec<Family> {
        families()
    }

    fn any_font_has(fams: &[Family], ch: u32) -> bool {
        fams.iter()
            .any(|f| f.faces.iter().any(|x| has_glyph(x, ch)))
    }

    #[test]
    fn latin_stays_in_base_family() {
        let fams = system();
        let Some(base) = fams
            .iter()
            .find(|f| has_glyph(&pick(f, 400, false), 'A' as u32))
        else {
            return; // pas de police sur cette machine
        };
        let (n, face) = map_run(&fams, &wide("Bonjour le monde"), &base.name, 400, false);
        assert_eq!(n, 16);
        assert_eq!(face.unwrap().family, base.name);
    }

    #[test]
    fn missing_character_moves_to_a_font_that_has_it() {
        let fams = system();
        // Une famille sans flèche ⟶ (U+27F6) et une autre qui l'a.
        let ch = 0x27F6;
        let Some(base) = fams.iter().find(|f| {
            let face = pick(f, 400, false);
            has_glyph(&face, 'a' as u32) && !has_glyph(&face, ch)
        }) else {
            return;
        };
        if !any_font_has(&fams, ch) {
            return;
        }
        let text = wide("ab\u{27F6}\u{27F6}cd");
        let (n, face) = map_run(&fams, &text, &base.name, 400, false);
        assert_eq!(
            (n, face.as_ref().map(|f| f.family.clone())),
            (2, Some(base.name.clone()))
        );
        let (n, face) = map_run(&fams, &text[2..], &base.name, 400, false);
        let face = face.expect("une police de repli");
        assert_ne!(face.family, base.name);
        assert!(has_glyph(&face, ch));
        assert_eq!(
            n, 2,
            "la plage de repli s'arrête où la police demandée reprend"
        );
    }

    #[test]
    fn unknown_character_has_no_font() {
        let fams = system();
        // Zone d'usage privé : aucune police libre n'y met de glyphe.
        let ch = 0x10FFFD;
        if any_font_has(&fams, ch) {
            return;
        }
        let text = wide("\u{10FFFD}\u{10FFFD}x");
        let (n, face) = map_run(&fams, &text, "Segoe UI", 400, false);
        assert!(face.is_none());
        assert_eq!(
            n, 4,
            "deux caractères hors BMP sans police = 4 unités UTF-16"
        );
    }

    #[test]
    fn custom_fallback_follows_the_app_mappings_then_the_system() {
        let fams = system();
        let Some(target) = fams
            .iter()
            .find(|f| has_glyph(&pick(f, 400, false), 'A' as u32))
        else {
            return;
        };
        let latin = Mapping {
            ranges: vec![(0x41, 0x5A)],
            families: vec!["Police absente".into(), target.name.clone()],
            collection: None,
            scale: 1.5,
        };
        let only_app = Custom {
            mappings: vec![latin.clone()],
            system: false,
        };
        // « AB » suit la correspondance, avec son échelle.
        let (n, face, scale) = only_app.map(&fams, &wide("ABc"), "", 400, false);
        assert_eq!((n, scale), (2, 1.5));
        assert_eq!(face.unwrap().family, target.name);
        // « c » n'est couvert par rien : pas de police sans le système.
        let (n, face, _) = only_app.map(&fams, &wide("cdA"), "", 400, false);
        assert_eq!((n, face), (2, None));
        let with_system = Custom {
            mappings: vec![latin],
            system: true,
        };
        let (_, face, scale) = with_system.map(&fams, &wide("cd"), &target.name, 400, false);
        assert_eq!(scale, 1.0);
        assert!(face.is_some());
    }

    #[test]
    fn rendering_params3_accept_the_downsampled_mode() {
        unsafe {
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(new_params3(2.2, 1.0, 1.0, 1.0, 1, 7, 1, &mut p), S_OK);
            let obj = p as *mut ParamsObj;
            assert_eq!((params_mode(obj), params_mode1(obj)), (5, 7));
            assert_eq!(release::<Params>(obj), 0);
            // IDWriteRenderingParams2 refuse ce mode.
            let mut q: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                new_params(2.2, 1.0, 1.0, 1.0, 1, 7, 1, &mut q),
                E_INVALIDARG
            );
            assert!(q.is_null());
            assert_eq!(std::mem::size_of_val(&PARAMS_VT), 11 * 8);
            assert_eq!(std::mem::size_of_val(&BUILDER_VT), 6 * 8);
        }
    }

    #[test]
    fn factory7_is_served_with_its_57_methods() {
        unsafe {
            let mut factory: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                super::super::DWriteCreateFactory(
                    0,
                    &super::super::sets::IID_DWRITE_FACTORY7,
                    &mut factory
                ),
                S_OK
            );
            assert_eq!(std::mem::size_of_val(&super::super::FACTORY_VT), 57 * 8);
            assert_eq!(std::mem::size_of_val(&super::super::FACE_VT), 60 * 8);
            assert_eq!(std::mem::size_of_val(&super::super::FONT_VT), 24 * 8);
            assert_eq!(std::mem::size_of_val(&super::super::FAMILY_VT), 14 * 8);
            assert_eq!(std::mem::size_of_val(&super::super::COLLECTION_VT), 14 * 8);
            let vt = *(factory as *const *const *const ());
            // IDWriteFactory7::GetSystemFontCollection (56) puis GetFontSet (7).
            type GetCol = unsafe extern "win64" fn(*mut c_void, i32, u32, *mut *mut c_void) -> i32;
            let get: GetCol = std::mem::transmute(*vt.add(56));
            let mut col: *mut c_void = std::ptr::null_mut();
            assert_eq!(get(factory, 0, 1, &mut col), S_OK);
            let cvt = *(col as *const *const *const ());
            type GetSet = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            let get_set: GetSet = std::mem::transmute(*cvt.add(7));
            let mut set: *mut c_void = std::ptr::null_mut();
            assert_eq!(get_set(col, &mut set), S_OK);
            type Count = unsafe extern "win64" fn(*mut c_void) -> u32;
            let families: Count = std::mem::transmute(*cvt.add(3));
            let fonts: Count = std::mem::transmute(*(*(set as *const *const *const ())).add(3));
            assert!(fonts(set) >= families(col));
            // Paramètres de rendu par défaut : IDWriteRenderingParams3.
            type Make = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            let make: Make = std::mem::transmute(*vt.add(10));
            let mut params: *mut c_void = std::ptr::null_mut();
            assert_eq!(make(factory, &mut params), S_OK);
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let qi: Qi = std::mem::transmute(**(params as *const *const *const ()));
            let mut p3: *mut c_void = std::ptr::null_mut();
            assert_eq!(qi(params, &IID_RENDERING_PARAMS3, &mut p3), S_OK);
            for o in [p3, params, set, col, factory] {
                let rel: unsafe extern "win64" fn(*mut c_void) -> u32 =
                    std::mem::transmute(*(*(o as *const *const *const ())).add(2));
                rel(o);
            }
        }
    }

    #[test]
    fn empty_text_maps_nothing() {
        assert_eq!(map_run(&system(), &[], "Arial", 400, false), (0, None));
    }

    // Source d'analyse minimale côté appli : GetTextAtPosition par morceaux de 3.
    #[repr(C)]
    struct Source {
        vtbl: *const *const (),
        text: Vec<u16>,
    }
    unsafe extern "win64" fn src_text(
        this: *mut Source,
        pos: u32,
        out: *mut *const u16,
        len: *mut u32,
    ) -> i32 {
        let t = &(*this).text;
        if pos as usize >= t.len() {
            *out = std::ptr::null();
            *len = 0;
        } else {
            *out = t.as_ptr().add(pos as usize);
            *len = (t.len() - pos as usize).min(3) as u32;
        }
        S_OK
    }

    #[test]
    fn factory2_gives_a_working_system_fallback() {
        unsafe {
            let mut factory: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                super::super::DWriteCreateFactory(0, &IID_DWRITE_FACTORY2, &mut factory),
                S_OK
            );
            let vt = *(factory as *const *const *const ());
            // QueryInterface vers IDWriteFactory1 et IDWriteFactory2.
            type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
            let qi: Qi = std::mem::transmute(*vt);
            for iid in [
                IID_DWRITE_FACTORY1,
                IID_DWRITE_FACTORY2,
                super::super::IID_DWRITE_FACTORY,
            ] {
                let mut other: *mut c_void = std::ptr::null_mut();
                assert_eq!(qi(factory, &iid, &mut other), S_OK);
                assert_eq!(other, factory);
            }
            type GetFallback = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
            let get: GetFallback = std::mem::transmute(*vt.add(26));
            let mut fallback: *mut c_void = std::ptr::null_mut();
            assert_eq!(get(factory, &mut fallback), S_OK);
            assert!(!fallback.is_null());
            // MapCharacters sur une source qui rend le texte par morceaux.
            let text = wide("Winlux");
            let vtable: [*const (); 4] = [
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                src_text as *const (),
            ];
            let mut source = Source {
                vtbl: vtable.as_ptr(),
                text: text.clone(),
            };
            type Map = unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                u32,
                u32,
                *mut c_void,
                *const u16,
                u32,
                u32,
                u32,
                *mut u32,
                *mut *mut c_void,
                *mut f32,
            ) -> i32;
            let map: Map = std::mem::transmute(*(*(fallback as *const *const *const ())).add(3));
            let family = super::super::tests::wide("Segoe UI");
            let (mut n, mut font, mut scale) = (0u32, std::ptr::null_mut(), 0f32);
            let hr = map(
                fallback,
                (&mut source as *mut Source).cast(),
                0,
                text.len() as u32,
                std::ptr::null_mut(),
                family.as_ptr(),
                400,
                0,
                5,
                &mut n,
                &mut font,
                &mut scale,
            );
            assert_eq!(hr, S_OK);
            assert_eq!(scale, 1.0);
            if !families().is_empty() {
                assert_eq!(n, 6);
                assert!(!font.is_null(), "une police installée doit rendre du latin");
            }
            // Couleur : aucune, réponse documentée.
            type Translate = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                f32,
                *const c_void,
                *const c_void,
                u32,
                *const c_void,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let translate: Translate = std::mem::transmute(*vt.add(28));
            let mut colors: *mut c_void = 1 as *mut c_void;
            let hr = translate(
                factory,
                0.0,
                0.0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut colors,
            );
            assert_eq!((hr, colors), (DWRITE_E_NOCOLOR, std::ptr::null_mut()));
            // Paramètres de rendu 2 : valeurs gardées, interface 1 et 2.
            type Custom2 = unsafe extern "win64" fn(
                *mut c_void,
                f32,
                f32,
                f32,
                f32,
                u32,
                u32,
                u32,
                *mut *mut c_void,
            ) -> i32;
            let custom: Custom2 = std::mem::transmute(*vt.add(29));
            let mut params: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                custom(factory, 1.8, 0.5, 0.25, 0.0, 0, 5, 1, &mut params),
                S_OK
            );
            let p = Com::get(params as *mut ParamsObj);
            assert_eq!((p.gamma, p.gray_contrast, p.grid_fit), (1.8, 0.25, 1));
            assert_eq!(
                custom(factory, -1.0, 0.5, 0.25, 0.0, 0, 5, 1, &mut params),
                E_INVALIDARG
            );
            // EUDC : collection vide mais valide.
            type Eudc = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void, i32) -> i32;
            let eudc: Eudc = std::mem::transmute(*vt.add(24));
            let mut collection: *mut c_void = std::ptr::null_mut();
            assert_eq!(eudc(factory, &mut collection, 0), S_OK);
            assert!(Com::get(collection as *mut CollectionObj).is_empty());
        }
    }
}
