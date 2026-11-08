//! IDWriteTextAnalyzer et IDWriteNumberSubstitution maison (#268).
//!
//! WPF découpe et met en forme tout son texte par l'analyseur de DirectWrite
//! (`TextAnalyzer.Itemize`, puis `GetGlyphs` et `GetGlyphPlacements`). Cet
//! analyseur reste simple, comme le reste de `dwrite.rs` :
//! - une seule écriture pour tout le texte (forme par défaut) ;
//! - un niveau bidirectionnel uniforme, celui du paragraphe ;
//! - coupures de ligne après les espaces et les tirets, obligatoires après
//!   un saut de ligne ;
//! - un glyphe par caractère (paires de substitution comprises), placé à son
//!   avance de dessin, sans crénage ni ligatures.
#![allow(clippy::too_many_arguments)]

use super::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use super::d2d1::{add_ref, not_implemented, qi_list, release};
use super::ole32::Guid;
use std::ffi::c_void;

const E_NOT_SUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;

const IID_TEXT_ANALYZER: Guid = guid(
    0xb7e6163e,
    0x7f46,
    0x43b4,
    [0x84, 0xb3, 0xe4, 0xe6, 0x24, 0x9c, 0x36, 0x5d],
);
const IID_NUMBER_SUBSTITUTION: Guid = guid(
    0x14885cc9,
    0xbab0,
    0x4f90,
    [0xb6, 0xed, 0x5c, 0x36, 0x6a, 0x2c, 0xd0, 0x3d],
);

/// Écriture unique rendue par AnalyzeScript (identifiant propre à Winlux,
/// seulement relu par GetGlyphs).
const SCRIPT_LATIN: u16 = 1;

type AnalyzerObj = Com<()>;
type SubstitutionObj = Com<u32>;

/// Appel d'une méthode d'un objet COM par son rang dans la vtable.
unsafe fn slot(obj: *mut c_void, index: usize) -> *const () {
    *(*(obj as *const *const *const ())).add(index)
}

// --- Lecture de la source (IDWriteTextAnalysisSource) ----------------------------------

/// Texte complet [pos, pos + len) lu par GetTextAtPosition, par morceaux.
unsafe fn source_text(source: *mut c_void, pos: u32, len: u32) -> Option<Vec<u16>> {
    type GetText = unsafe extern "win64" fn(*mut c_void, u32, *mut *const u16, *mut u32) -> i32;
    let get: GetText = std::mem::transmute(slot(source, 3));
    let mut out = Vec::with_capacity(len as usize);
    while (out.len() as u32) < len {
        let at = pos + out.len() as u32;
        let mut text: *const u16 = std::ptr::null();
        let mut n = 0u32;
        if get(source, at, &mut text, &mut n) < 0 || text.is_null() || n == 0 {
            break;
        }
        let take = n.min(len - out.len() as u32) as usize;
        out.extend_from_slice(std::slice::from_raw_parts(text, take));
    }
    (out.len() as u32 == len).then_some(out)
}

/// 0 de gauche à droite, 1 de droite à gauche.
unsafe fn paragraph_direction(source: *mut c_void) -> u32 {
    type GetDir = unsafe extern "win64" fn(*mut c_void) -> u32;
    let get: GetDir = std::mem::transmute(slot(source, 5));
    get(source)
}

// --- Analyses ---------------------------------------------------------------------------

unsafe extern "win64" fn analyzer_qi(
    this: *mut AnalyzerObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_TEXT_ANALYZER])
}

/// DWRITE_SCRIPT_ANALYSIS { UINT16 script; DWRITE_SCRIPT_SHAPES shapes; }.
#[repr(C)]
struct ScriptAnalysis {
    script: u16,
    shapes: u32,
}

unsafe extern "win64" fn analyze_script(
    _this: *mut AnalyzerObj,
    source: *mut c_void,
    pos: u32,
    len: u32,
    sink: *mut c_void,
) -> i32 {
    if source.is_null() || sink.is_null() {
        return E_INVALIDARG;
    }
    if len == 0 {
        return S_OK;
    }
    type SetScript = unsafe extern "win64" fn(*mut c_void, u32, u32, *const ScriptAnalysis) -> i32;
    let set: SetScript = std::mem::transmute(slot(sink, 3));
    let analysis = ScriptAnalysis {
        script: SCRIPT_LATIN,
        shapes: 0,
    };
    set(sink, pos, len, &analysis)
}

unsafe extern "win64" fn analyze_bidi(
    _this: *mut AnalyzerObj,
    source: *mut c_void,
    pos: u32,
    len: u32,
    sink: *mut c_void,
) -> i32 {
    if source.is_null() || sink.is_null() {
        return E_INVALIDARG;
    }
    if len == 0 {
        return S_OK;
    }
    let level = u8::from(paragraph_direction(source) == 1);
    type SetBidi = unsafe extern "win64" fn(*mut c_void, u32, u32, u8, u8) -> i32;
    let set: SetBidi = std::mem::transmute(slot(sink, 5));
    set(sink, pos, len, level, level)
}

/// Pas de substitution de chiffres : rien à signaler au récepteur.
unsafe extern "win64" fn analyze_number_substitution(
    _this: *mut AnalyzerObj,
    source: *mut c_void,
    _pos: u32,
    _len: u32,
    sink: *mut c_void,
) -> i32 {
    if source.is_null() || sink.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

/// DWRITE_LINE_BREAKPOINT : breakConditionBefore:2, breakConditionAfter:2,
/// isWhitespace:1, isSoftHyphen:1. Conditions : 0 neutre, 1 possible,
/// 2 interdite, 3 obligatoire.
fn breakpoints(text: &[u16]) -> Vec<u8> {
    const CAN: u8 = 1;
    const MAY_NOT: u8 = 2;
    const MUST: u8 = 3;
    let mut after: Vec<u8> = text
        .iter()
        .map(|&c| match c {
            0x0A | 0x0B | 0x0C | 0x0D | 0x85 | 0x2028 | 0x2029 => MUST,
            0x20 | 0x09 | 0x2D | 0x2010 | 0x2013 | 0x2014 | 0x200B => CAN,
            0xA0 | 0x202F | 0x2060 => MAY_NOT,
            _ => 0,
        })
        .collect();
    // CR LF : pas de coupure entre les deux.
    for i in 0..text.len().saturating_sub(1) {
        if text[i] == 0x0D && text[i + 1] == 0x0A {
            after[i] = MAY_NOT;
        }
    }
    // Pas de coupure au milieu d'une paire de substitution.
    for i in 0..text.len() {
        if (0xD800..0xDC00).contains(&text[i]) {
            after[i] = MAY_NOT;
        }
    }
    (0..text.len())
        .map(|i| {
            let before = if i == 0 {
                MAY_NOT
            } else if after[i - 1] == MUST || after[i - 1] == CAN {
                after[i - 1]
            } else if text[i] == 0xA0 {
                MAY_NOT
            } else {
                0
            };
            let white = matches!(text[i], 0x20 | 0x09 | 0x0A | 0x0D | 0x0B | 0x0C | 0x3000);
            before | (after[i] << 2) | (u8::from(white) << 4) | (u8::from(text[i] == 0xAD) << 5)
        })
        .collect()
}

unsafe extern "win64" fn analyze_line_breakpoints(
    _this: *mut AnalyzerObj,
    source: *mut c_void,
    pos: u32,
    len: u32,
    sink: *mut c_void,
) -> i32 {
    if source.is_null() || sink.is_null() {
        return E_INVALIDARG;
    }
    if len == 0 {
        return S_OK;
    }
    let Some(text) = source_text(source, pos, len) else {
        return E_INVALIDARG;
    };
    let points = breakpoints(&text);
    type SetBreaks = unsafe extern "win64" fn(*mut c_void, u32, u32, *const u8) -> i32;
    let set: SetBreaks = std::mem::transmute(slot(sink, 4));
    set(sink, pos, len, points.as_ptr())
}

// --- Glyphes ------------------------------------------------------------------------------

/// Points de code et indice du premier code UTF-16 de chacun.
fn code_points(text: &[u16]) -> Vec<(u32, usize)> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        if (0xD800..0xDC00).contains(&c)
            && i + 1 < text.len()
            && (0xDC00..0xE000).contains(&text[i + 1])
        {
            let cp = 0x10000 + (((c as u32) - 0xD800) << 10) + (text[i + 1] as u32 - 0xDC00);
            out.push((cp, i));
            i += 2;
        } else {
            out.push((c as u32, i));
            i += 1;
        }
    }
    out
}

unsafe extern "win64" fn get_glyphs(
    _this: *mut AnalyzerObj,
    text: *const u16,
    len: u32,
    face: *mut c_void,
    _sideways: i32,
    rtl: i32,
    _script: *const ScriptAnalysis,
    _locale: *const u16,
    _substitution: *mut c_void,
    _features: *const *const c_void,
    _feature_lengths: *const u32,
    _feature_ranges: u32,
    max_glyphs: u32,
    cluster_map: *mut u16,
    text_props: *mut u16,
    glyph_indices: *mut u16,
    glyph_props: *mut u16,
    actual: *mut u32,
) -> i32 {
    if text.is_null()
        || face.is_null()
        || cluster_map.is_null()
        || text_props.is_null()
        || glyph_indices.is_null()
        || glyph_props.is_null()
        || actual.is_null()
    {
        return E_POINTER;
    }
    let text = std::slice::from_raw_parts(text, len as usize);
    let mut points = code_points(text);
    if points.len() as u32 > max_glyphs {
        *actual = points.len() as u32;
        return E_NOT_SUFFICIENT_BUFFER;
    }
    // Miroir élémentaire des parenthèses en écriture de droite à gauche.
    if rtl != 0 {
        for p in points.iter_mut() {
            p.0 = match p.0 {
                0x28 => 0x29,
                0x29 => 0x28,
                0x5B => 0x5D,
                0x5D => 0x5B,
                0x7B => 0x7D,
                0x7D => 0x7B,
                0x3C => 0x3E,
                0x3E => 0x3C,
                c => c,
            };
        }
    }
    let cps: Vec<u32> = points.iter().map(|p| p.0).collect();
    type GlyphIndices = unsafe extern "win64" fn(*mut c_void, *const u32, u32, *mut u16) -> i32;
    let indices: GlyphIndices = std::mem::transmute(slot(face, 11));
    let hr = indices(face, cps.as_ptr(), cps.len() as u32, glyph_indices);
    if hr < 0 {
        return hr;
    }
    for (g, &(cp, at)) in points.iter().enumerate() {
        *cluster_map.add(at) = g as u16;
        *text_props.add(at) = 0;
        if at + 1 < text.len() && points.get(g + 1).map(|n| n.1) != Some(at + 1) {
            // Seconde moitié d'une paire de substitution : même grappe.
            *cluster_map.add(at + 1) = g as u16;
            *text_props.add(at + 1) = 0;
        }
        // justification:4 (4 = espace), isClusterStart:1, isDiacritic:1,
        // isZeroWidthSpace:1.
        let justification: u16 = if cp == 0x20 || cp == 0x3000 { 4 } else { 0 };
        let zero_width = matches!(cp, 0x200B | 0x200C | 0x200D | 0x2060 | 0xFEFF);
        let diacritic = (0x300..0x370).contains(&cp);
        *glyph_props.add(g) =
            justification | (1 << 4) | (u16::from(diacritic) << 5) | (u16::from(zero_width) << 6);
        if zero_width {
            *glyph_indices.add(g) = 0;
        }
    }
    *actual = points.len() as u32;
    S_OK
}

/// Avances de dessin (en unités de la police) des glyphes, et unités par em.
unsafe fn design_advances(
    face: *mut c_void,
    indices: &[u16],
    sideways: i32,
) -> Option<(Vec<u32>, u16)> {
    // DWRITE_FONT_METRICS : designUnitsPerEm en tête (UINT16), 20 octets.
    let mut metrics = [0u16; 10];
    type Metrics = unsafe extern "win64" fn(*mut c_void, *mut u16);
    let get_metrics: Metrics = std::mem::transmute(slot(face, 8));
    get_metrics(face, metrics.as_mut_ptr());
    let upem = metrics[0];
    if upem == 0 {
        return None;
    }
    // DWRITE_GLYPH_METRICS : 7 champs de 4 octets, advanceWidth en second.
    let mut out = vec![[0u32; 7]; indices.len()];
    type Design = unsafe extern "win64" fn(*mut c_void, *const u16, u32, *mut [u32; 7], i32) -> i32;
    let design: Design = std::mem::transmute(slot(face, 10));
    if design(
        face,
        indices.as_ptr(),
        indices.len() as u32,
        out.as_mut_ptr(),
        sideways,
    ) < 0
    {
        return None;
    }
    Some((out.iter().map(|m| m[1]).collect(), upem))
}

unsafe fn place(
    glyph_indices: *const u16,
    glyph_props: *const u16,
    count: u32,
    face: *mut c_void,
    em: f32,
    sideways: i32,
    advances: *mut f32,
    offsets: *mut [f32; 2],
    pixel_scale: Option<f32>,
) -> i32 {
    if count == 0 {
        return S_OK;
    }
    if glyph_indices.is_null() || face.is_null() || advances.is_null() || offsets.is_null() {
        return E_POINTER;
    }
    let indices = std::slice::from_raw_parts(glyph_indices, count as usize);
    let Some((design, upem)) = design_advances(face, indices, sideways) else {
        return E_INVALIDARG;
    };
    for (i, adv) in design.iter().enumerate() {
        let zero_width = !glyph_props.is_null() && *glyph_props.add(i) & (1 << 6) != 0;
        let mut a = if zero_width {
            0.0
        } else {
            *adv as f32 * em / upem as f32
        };
        if let Some(scale) = pixel_scale.filter(|s| *s > 0.0) {
            // Mode compatible GDI : avances arrondies au pixel.
            a = (a * scale).round() / scale;
        }
        *advances.add(i) = a;
        *offsets.add(i) = [0.0, 0.0];
    }
    S_OK
}

unsafe extern "win64" fn get_glyph_placements(
    _this: *mut AnalyzerObj,
    _text: *const u16,
    _cluster_map: *const u16,
    _text_props: *const u16,
    _len: u32,
    glyph_indices: *const u16,
    glyph_props: *const u16,
    count: u32,
    face: *mut c_void,
    em: f32,
    sideways: i32,
    _rtl: i32,
    _script: *const ScriptAnalysis,
    _locale: *const u16,
    _features: *const *const c_void,
    _feature_lengths: *const u32,
    _feature_ranges: u32,
    advances: *mut f32,
    offsets: *mut [f32; 2],
) -> i32 {
    place(
        glyph_indices,
        glyph_props,
        count,
        face,
        em,
        sideways,
        advances,
        offsets,
        None,
    )
}

unsafe extern "win64" fn get_gdi_compatible_glyph_placements(
    _this: *mut AnalyzerObj,
    _text: *const u16,
    _cluster_map: *const u16,
    _text_props: *const u16,
    _len: u32,
    glyph_indices: *const u16,
    glyph_props: *const u16,
    count: u32,
    face: *mut c_void,
    em: f32,
    pixels_per_dip: f32,
    transform: *const [f32; 6],
    _natural: i32,
    sideways: i32,
    _rtl: i32,
    _script: *const ScriptAnalysis,
    _locale: *const u16,
    _features: *const *const c_void,
    _feature_lengths: *const u32,
    _feature_ranges: u32,
    advances: *mut f32,
    offsets: *mut [f32; 2],
) -> i32 {
    let mut scale = pixels_per_dip;
    if !transform.is_null() {
        let m = *transform;
        scale *= (m[0] * m[0] + m[1] * m[1]).sqrt();
    }
    place(
        glyph_indices,
        glyph_props,
        count,
        face,
        em,
        sideways,
        advances,
        offsets,
        Some(scale),
    )
}

static ANALYZER_VT: VTable<10> = VTable([
    analyzer_qi as *const (),
    add_ref::<()> as *const (),
    release::<()> as *const (),
    analyze_script as *const (),
    analyze_bidi as *const (),
    analyze_number_substitution as *const (),
    analyze_line_breakpoints as *const (),
    get_glyphs as *const (),
    get_glyph_placements as *const (),
    get_gdi_compatible_glyph_placements as *const (),
]);

/// IDWriteFactory::CreateTextAnalyzer.
pub unsafe extern "win64" fn create_text_analyzer(
    _factory: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, Com::create(ANALYZER_VT.ptr(), ()))
}

unsafe extern "win64" fn substitution_qi(
    this: *mut SubstitutionObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_NUMBER_SUBSTITUTION])
}

static SUBSTITUTION_VT: VTable<3> = VTable([
    substitution_qi as *const (),
    add_ref::<u32> as *const (),
    release::<u32> as *const (),
]);

/// IDWriteFactory::CreateNumberSubstitution : objet opaque qui garde la
/// méthode demandée (0 à 4) ; l'analyseur ne substitue pas les chiffres.
pub unsafe extern "win64" fn create_number_substitution(
    _factory: *mut c_void,
    method: u32,
    locale: *const u16,
    _ignore_user_override: i32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if method > 4 || locale.is_null() {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    put(out, Com::create(SUBSTITUTION_VT.ptr(), method))
}

/// Garde l'import de `not_implemented` utile si une méthode y retombe.
#[allow(dead_code)]
fn _keep() -> *const () {
    not_implemented as *const ()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupures_de_ligne() {
        let text: Vec<u16> = "un deux\r\ntrois".encode_utf16().collect();
        let b = breakpoints(&text);
        // Après l'espace (indice 2) : coupure possible, et l'espace est blanc.
        assert_eq!((b[2] >> 2) & 3, 1);
        assert_eq!((b[2] >> 4) & 1, 1);
        assert_eq!(b[3] & 3, 1);
        // Entre CR et LF : interdite ; après LF : obligatoire.
        assert_eq!((b[7] >> 2) & 3, 2);
        assert_eq!((b[8] >> 2) & 3, 3);
        assert_eq!(b[9] & 3, 3);
        // Au début du texte : pas de coupure.
        assert_eq!(b[0] & 3, 2);
    }

    #[test]
    fn points_de_code_et_paires() {
        let text: Vec<u16> = "a😀b".encode_utf16().collect();
        let p = code_points(&text);
        assert_eq!(p, vec![(0x61, 0), (0x1F600, 1), (0x62, 3)]);
    }
}
