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
//! rendu personnalisés, aucune police en couleur (`DWRITE_E_NOCOLOR`), et
//! pas de constructeur de repli personnalisé (`E_NOTIMPL`, sortie nulle).

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

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
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

/// IDWriteFactory2::CreateFontFallbackBuilder : non pris en charge.
pub unsafe extern "win64" fn fallback_builder(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
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

/// IDWriteFactory2::CreateGlyphRunAnalysis : pas encore de rendu en
/// niveaux de gris par DirectWrite (la version d'IDWriteFactory non plus).
pub unsafe extern "win64" fn glyph_run_analysis2(
    _this: *mut c_void,
    _run: *const c_void,
    _transform: *const c_void,
    _mode: u32,
    _measuring: u32,
    _grid_fit: u32,
    _antialias: u32,
    _x: f32,
    _y: f32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
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
    if out.is_null() {
        return E_POINTER;
    }
    // Plages de la documentation : gamma > 0, contrastes et ClearType >= 0.
    if !(gamma > 0.0 && gamma <= 256.0)
        || contrast < 0.0
        || gray_contrast < 0.0
        || !(0.0..=1.0).contains(&cleartype)
        || geometry > 2
        || mode > 6
        || grid_fit > 3
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
    Com::get(this).mode
}
unsafe extern "win64" fn params_gray_contrast(this: *mut ParamsObj) -> f32 {
    Com::get(this).gray_contrast
}
unsafe extern "win64" fn params_grid_fit(this: *mut ParamsObj) -> u32 {
    Com::get(this).grid_fit
}
static PARAMS_VT: VTable<10> = VTable([
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
