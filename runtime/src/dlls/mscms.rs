//! mscms.dll : profils de couleur ICM et WCS (#1363).
//!
//! D'après icm.h (mingw-w64) et la documentation publique de l'API ICM.
//! Un profil ouvert garde ses octets ICC en mémoire ; Paint.NET l'ouvre
//! depuis la mémoire ou depuis le profil sRGB du système, qui est servi par
//! le profil construit d'après la spécification ICC (`icc::srgb`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const PROFILE_FILENAME: u32 = 1;
const PROFILE_MEMBUFFER: u32 = 2;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
/// Nom du profil sRGB standard du dossier des couleurs.
const SRGB_NAME: &str = "sRGB Color Space Profile.icm";
/// LCS_sRGB, LCS_WINDOWS_COLOR_SPACE (GetStandardColorSpaceProfile).
const LCS_SRGB: u32 = 0x7352_4742;
const LCS_WINDOWS_COLOR_SPACE: u32 = 0x5769_6E20;
const COLOR_DIRECTORY: &str = "C:\\Windows\\System32\\spool\\drivers\\color";

fn profiles() -> &'static Mutex<HashMap<usize, Vec<u8>>> {
    static P: OnceLock<Mutex<HashMap<usize, Vec<u8>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}
static NEXT: AtomicUsize = AtomicUsize::new(0x1C3_0000);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}

fn register(bytes: Vec<u8>) -> usize {
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    lock(profiles()).insert(h, bytes);
    h
}

fn with_profile<T>(h: usize, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    lock(profiles()).get(&h).map(|p| f(p))
}

/// Profil ICC plausible : taille déclarée cohérente et signature « acsp ».
fn valid(p: &[u8]) -> bool {
    p.len() >= 132
        && u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize <= p.len()
        && &p[36..40] == b"acsp"
}

/// Octets d'un profil désigné par un nom de fichier Windows (chemin complet
/// ou nom dans le dossier des couleurs).
fn read_named(name: &str) -> Option<Vec<u8>> {
    let path = if name.contains('\\') || name.contains('/') {
        name.to_string()
    } else {
        format!("{COLOR_DIRECTORY}\\{name}")
    };
    if let Ok(bytes) = std::fs::read(crate::paths::to_linux(&path)) {
        return Some(bytes);
    }
    let file = path.rsplit(['\\', '/']).next().unwrap_or("");
    file.eq_ignore_ascii_case(SRGB_NAME)
        .then(crate::dlls::icc::srgb)
}

/// PROFILE { DWORD dwType; PVOID pProfileData; DWORD cbDataSize; }.
unsafe fn open_profile(profile: *const u8, wide: bool) -> usize {
    if profile.is_null() {
        fail(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let kind = std::ptr::read_unaligned(profile as *const u32);
    let data = std::ptr::read_unaligned(profile.add(8) as *const *const u8);
    let size = std::ptr::read_unaligned(profile.add(16) as *const u32) as usize;
    if data.is_null() {
        fail(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let bytes = match kind {
        PROFILE_MEMBUFFER => std::slice::from_raw_parts(data, size).to_vec(),
        PROFILE_FILENAME => {
            let name = if wide {
                crate::dlls::wstr(data as *const u16)
            } else {
                std::ffi::CStr::from_ptr(data.cast())
                    .to_string_lossy()
                    .into_owned()
            };
            match read_named(&name) {
                Some(b) => b,
                None => {
                    fail(ERROR_FILE_NOT_FOUND);
                    return 0;
                }
            }
        }
        _ => {
            fail(ERROR_INVALID_PARAMETER);
            return 0;
        }
    };
    if !valid(&bytes) {
        fail(ERROR_INVALID_PARAMETER);
        return 0;
    }
    register(bytes)
}

pub unsafe extern "win64" fn open_color_profile_w(
    profile: *const u8,
    _access: u32,
    _share: u32,
    _creation: u32,
) -> usize {
    open_profile(profile, true)
}

pub unsafe extern "win64" fn open_color_profile_a(
    profile: *const u8,
    _access: u32,
    _share: u32,
    _creation: u32,
) -> usize {
    open_profile(profile, false)
}

/// WcsOpenColorProfileW : seul le profil ICC (CDMP) compte ; les profils
/// de modèle d'apparence et de gamme WCS sont ignorés.
pub unsafe extern "win64" fn wcs_open_color_profile_w(
    cdmp: *const u8,
    _camp: *const u8,
    _gmmp: *const u8,
    _access: u32,
    _share: u32,
    _creation: u32,
    _flags: u32,
) -> usize {
    open_profile(cdmp, true)
}

/// WcsCreateIccProfile : un profil déjà ICC est rendu dans un nouveau handle.
pub extern "win64" fn wcs_create_icc_profile(h: usize, _options: u32) -> usize {
    match with_profile(h, |p| p.to_vec()) {
        Some(bytes) => register(bytes),
        None => {
            fail(ERROR_INVALID_HANDLE);
            0
        }
    }
}

pub extern "win64" fn close_color_profile(h: usize) -> i32 {
    if lock(profiles()).remove(&h).is_some() {
        1
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

pub unsafe extern "win64" fn get_color_profile_from_handle(
    h: usize,
    buffer: *mut u8,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let Some(bytes) = with_profile(h, |p| p.to_vec()) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let room = *size as usize;
    *size = bytes.len() as u32;
    if buffer.is_null() || room < bytes.len() {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    1
}

/// PROFILEHEADER : les champs de l'en-tête ICC en ordre machine (icm.h).
/// L'illuminant passe de s15Fixed16 à FXPT2DOT30.
fn native_header(p: &[u8]) -> [u8; 128] {
    let mut out = [0u8; 128];
    for at in (0..84).step_by(4) {
        let v = u32::from_be_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]]);
        let v = if (68..80).contains(&at) {
            ((v as i32) << 14) as u32
        } else {
            v
        };
        out[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    out[84..128].copy_from_slice(&p[84..128]);
    out
}

pub unsafe extern "win64" fn get_color_profile_header(h: usize, header: *mut u8) -> i32 {
    if header.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match with_profile(h, native_header) {
        Some(bytes) => {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), header, 128);
            1
        }
        None => fail(ERROR_INVALID_HANDLE),
    }
}

pub unsafe extern "win64" fn is_color_profile_valid(h: usize, result: *mut i32) -> i32 {
    if result.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match with_profile(h, valid) {
        Some(v) => {
            *result = v as i32;
            1
        }
        None => fail(ERROR_INVALID_HANDLE),
    }
}

/// Table des balises : (signature, décalage, taille).
fn tags(p: &[u8]) -> Vec<(u32, usize, usize)> {
    let be = |at: usize| {
        p.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let count = be(128).unwrap_or(0) as usize;
    (0..count.min(1024))
        .filter_map(|i| {
            let e = 132 + 12 * i;
            Some((be(e)?, be(e + 4)? as usize, be(e + 8)? as usize))
        })
        .filter(|&(_, off, len)| off.checked_add(len).is_some_and(|end| end <= p.len()))
        .collect()
}

pub unsafe extern "win64" fn get_count_color_profile_elements(h: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match with_profile(h, |p| tags(p).len()) {
        Some(n) => {
            *count = n as u32;
            1
        }
        None => fail(ERROR_INVALID_HANDLE),
    }
}

/// GetColorProfileElementTag : index à partir de 1.
pub unsafe extern "win64" fn get_color_profile_element_tag(
    h: usize,
    index: u32,
    tag: *mut u32,
) -> i32 {
    if tag.is_null() || index == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match with_profile(h, |p| tags(p).get(index as usize - 1).map(|t| t.0)) {
        Some(Some(t)) => {
            *tag = t;
            1
        }
        Some(None) => fail(ERROR_INVALID_PARAMETER),
        None => fail(ERROR_INVALID_HANDLE),
    }
}

pub unsafe extern "win64" fn is_color_profile_tag_present(
    h: usize,
    tag: u32,
    present: *mut i32,
) -> i32 {
    if present.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match with_profile(h, |p| tags(p).iter().any(|t| t.0 == tag)) {
        Some(v) => {
            *present = v as i32;
            1
        }
        None => fail(ERROR_INVALID_HANDLE),
    }
}

pub unsafe extern "win64" fn get_color_profile_element(
    h: usize,
    tag: u32,
    offset: u32,
    size: *mut u32,
    buffer: *mut u8,
    reference: *mut i32,
) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let element = with_profile(h, |p| {
        tags(p)
            .iter()
            .find(|t| t.0 == tag)
            .map(|&(_, off, len)| p[off..off + len].to_vec())
    });
    let data = match element {
        Some(Some(d)) => d,
        Some(None) => return fail(ERROR_INVALID_PARAMETER),
        None => return fail(ERROR_INVALID_HANDLE),
    };
    if !reference.is_null() {
        *reference = 0;
    }
    let start = (offset as usize).min(data.len());
    let rest = &data[start..];
    if buffer.is_null() {
        *size = rest.len() as u32;
        return 1;
    }
    let n = rest.len().min(*size as usize);
    std::ptr::copy_nonoverlapping(rest.as_ptr(), buffer, n);
    *size = n as u32;
    1
}

/// GetStandardColorSpaceProfileW : le profil sRGB du dossier des couleurs.
pub unsafe extern "win64" fn get_standard_color_space_profile_w(
    _machine: *const u16,
    space: u32,
    name: *mut u16,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if space != LCS_SRGB && space != LCS_WINDOWS_COLOR_SPACE {
        return fail(ERROR_FILE_NOT_FOUND);
    }
    let path: Vec<u16> = format!("{COLOR_DIRECTORY}\\{SRGB_NAME}")
        .encode_utf16()
        .chain([0])
        .collect();
    let bytes = (path.len() * 2) as u32;
    let room = *size;
    *size = bytes;
    if name.is_null() || room < bytes {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(path.as_ptr(), name, path.len());
    1
}

/// GetColorDirectoryW : taille en octets, zéro final compris.
pub unsafe extern "win64" fn get_color_directory_w(
    _machine: *const u16,
    buffer: *mut u16,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let dir: Vec<u16> = COLOR_DIRECTORY.encode_utf16().chain([0]).collect();
    let bytes = (dir.len() * 2) as u32;
    let room = *size;
    *size = bytes;
    if buffer.is_null() || room < bytes {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(dir.as_ptr(), buffer, dir.len());
    1
}

// --- Transformations de couleurs (#3103) ---------------------------------------------
//
// D'après les pages Learn d'icm.h (CreateMultiProfileTransform, TranslateColors,
// TranslateBitmapBits, CheckColors, DeleteColorTransform, SelectCMM,
// EnumColorProfiles, BMFORMAT, ENUMTYPEW) et la spécification ICC.1. Seuls les
// profils RVB matrice + courbes sont transformés (`icc::shaper`) : un profil à
// tables (CMJN, gris, lien de périphériques) est refusé, jamais approché.

const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_CANCELLED: u32 = 1223;
const ERROR_INVALID_CMM: u32 = 2010;
const ERROR_INVALID_TRANSFORM: u32 = 2020;
/// Profil dont la matrice n'est pas inversible.
const ERROR_INVALID_PROFILE: u32 = 2011;
const INTENT_RELATIVE_COLORIMETRIC: u32 = 1;
const INTENT_ABSOLUTE_COLORIMETRIC: u32 = 3;
/// COLORTYPE COLOR_RGB.
const COLOR_RGB: u32 = 2;
/// Taille d'un COLOR en x64 : union de 16 octets (DWORD + pointeur).
const COLOR_SIZE: usize = 16;
/// Signature du module de correspondance par défaut, « Win ».
const CMM_WINDOWS_DEFAULT: u32 = 0x5769_6E20;
/// Points de la table inverse d'une courbe de destination, indexée par √y.
const INVERSE_POINTS: usize = 4096;

/// Une étape entre deux profils : matrice + courbes, et courbes de
/// destination inversées une fois pour toutes.
struct Step {
    t: crate::dlls::icc::Transform,
    inverse: [Vec<f32>; 3],
}

impl Step {
    fn new(t: crate::dlls::icc::Transform) -> Step {
        let inverse = [0, 1, 2].map(|c| {
            (0..=INVERSE_POINTS)
                .map(|i| {
                    let s = i as f32 / INVERSE_POINTS as f32;
                    t.dest_curves()[c].invert(s * s)
                })
                .collect()
        });
        Step { t, inverse }
    }

    fn encode(&self, c: usize, y: f32) -> f32 {
        let pos = y.clamp(0.0, 1.0).sqrt() * INVERSE_POINTS as f32;
        let i = (pos as usize).min(INVERSE_POINTS - 1);
        let f = pos - i as f32;
        let t = &self.inverse[c];
        t[i] + (t[i + 1] - t[i]) * f
    }
}

/// Transformation : étapes successives, et, pour les entrées sur 8 bits, la
/// courbe source de la première étape échantillonnée.
struct Xform {
    steps: Vec<Step>,
    first8: [[f32; 256]; 3],
}

impl Xform {
    /// Couleur encodée (0..1) vers la destination, et l'écart le plus grand
    /// hors de la gamme rencontré en chemin (0 : dans la gamme).
    fn apply_linear(&self, mut lin: [f32; 3]) -> ([f32; 3], f32) {
        let mut out = [0f32; 3];
        let mut beyond = 0f32;
        for (k, step) in self.steps.iter().enumerate() {
            if k > 0 {
                lin = [0, 1, 2].map(|c| step.t.source_curves()[c].eval(out[c]));
            }
            let v = step.t.linear(lin);
            for c in 0..3 {
                beyond = beyond.max(-v[c]).max(v[c] - 1.0);
                out[c] = step.encode(c, v[c]);
            }
        }
        (out, beyond)
    }

    fn apply(&self, rgb: [f32; 3]) -> ([f32; 3], f32) {
        let first = &self.steps[0].t;
        self.apply_linear([0, 1, 2].map(|c| first.source_curves()[c].eval(rgb[c])))
    }

    fn apply8(&self, rgb: [u8; 3]) -> [f32; 3] {
        self.apply_linear([0, 1, 2].map(|c| self.first8[c][rgb[c] as usize]))
            .0
    }
}

fn transforms() -> &'static Mutex<HashMap<usize, std::sync::Arc<Xform>>> {
    static T: OnceLock<Mutex<HashMap<usize, std::sync::Arc<Xform>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn transform(h: usize) -> Option<std::sync::Arc<Xform>> {
    lock(transforms()).get(&h).cloned()
}

/// Intention de l'étape `k` (du profil k au profil k+1), d'après la remarque
/// de CreateMultiProfileTransform : une seule intention vaut pour la première
/// étape, les suivantes en colorimétrie relative ; n-1 intentions, une par
/// étape ; n intentions, la première est ignorée. DWORD_MAX : l'intention de
/// l'en-tête du second profil.
fn step_intent(k: usize, intents: &[u32], profiles: &[Vec<u8>]) -> Option<u32> {
    let n = profiles.len();
    let i = match intents.len() {
        1 if k == 0 => intents[0],
        1 => INTENT_RELATIVE_COLORIMETRIC,
        m if m == n - 1 => intents[k],
        m if m == n => intents[k + 1],
        _ => return None,
    };
    if i == u32::MAX {
        let p = &profiles[k + 1];
        return Some(u32::from_be_bytes(p.get(64..68)?.try_into().ok()?) & 3);
    }
    (i <= INTENT_ABSOLUTE_COLORIMETRIC).then_some(i)
}

/// CreateMultiProfileTransform(pahProfiles, nProfiles, padwIntent, nIntents,
/// dwFlags, indexPreferredCMM). Les drapeaux et le CMM préféré sont des
/// indications (Learn) : un profil matrice + courbes n'a qu'une
/// transformation colorimétrique (ICC.1 §F.3), perceptuelle et saturation y
/// reviennent ; l'absolue applique le rapport des blancs du support.
pub unsafe extern "win64" fn create_multi_profile_transform(
    handles: *const usize,
    count: u32,
    intents: *const u32,
    intent_count: u32,
    _flags: u32,
    _preferred_cmm: u32,
) -> usize {
    let n = count as usize;
    let m = intent_count as usize;
    if handles.is_null() || !(1..=10).contains(&n) || intents.is_null() || !(1..=10).contains(&m) {
        fail(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut bytes = Vec::with_capacity(n);
    for i in 0..n {
        match with_profile(*handles.add(i), |p| p.to_vec()) {
            Some(b) => bytes.push(b),
            None => {
                fail(ERROR_INVALID_HANDLE);
                return 0;
            }
        }
    }
    if n == 1 {
        // Un seul profil n'est admis que s'il lie deux périphériques ;
        // les profils de lien (tables) ne sont pas transformés ici.
        crate::log_warn!(
            "mscms",
            "profil de lien de périphériques non pris en charge ({}:{})",
            file!(),
            line!()
        );
        fail(ERROR_NOT_SUPPORTED);
        return 0;
    }
    let intents = std::slice::from_raw_parts(intents, m);
    let mut steps = Vec::with_capacity(n - 1);
    for k in 0..n - 1 {
        let Some(intent) = step_intent(k, intents, &bytes) else {
            fail(ERROR_INVALID_PARAMETER);
            return 0;
        };
        let (Some(src), Some(dst)) = (
            crate::dlls::icc::shaper(&bytes[k]),
            crate::dlls::icc::shaper(&bytes[k + 1]),
        ) else {
            crate::log_warn!(
                "mscms",
                "seuls les profils RVB matrice + courbes sont transformés ({}:{})",
                file!(),
                line!()
            );
            fail(ERROR_NOT_SUPPORTED);
            return 0;
        };
        let scale = if intent == INTENT_ABSOLUTE_COLORIMETRIC {
            let d50 = [0.9642f32, 1.0, 0.8249];
            let ws = crate::dlls::icc::media_white(&bytes[k]).unwrap_or(d50);
            let wd = crate::dlls::icc::media_white(&bytes[k + 1]).unwrap_or(d50);
            [0, 1, 2].map(|c| if wd[c] > 0.0 { ws[c] / wd[c] } else { 1.0 })
        } else {
            [1.0; 3]
        };
        match crate::dlls::icc::Transform::new_scaled(src, dst, scale) {
            Some(t) => steps.push(Step::new(t)),
            None => {
                fail(ERROR_INVALID_PROFILE);
                return 0;
            }
        }
    }
    let curves = steps[0].t.source_curves();
    let first8 = [0, 1, 2].map(|c| {
        let mut t = [0f32; 256];
        for (v, o) in t.iter_mut().enumerate() {
            *o = curves[c].eval(v as f32 / 255.0);
        }
        t
    });
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    lock(transforms()).insert(h, std::sync::Arc::new(Xform { steps, first8 }));
    h
}

pub extern "win64" fn delete_color_transform(h: usize) -> i32 {
    if lock(transforms()).remove(&h).is_some() {
        1
    } else {
        fail(ERROR_INVALID_TRANSFORM)
    }
}

fn unit(v: u16) -> f32 {
    v as f32 / 65535.0
}

fn word(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// RGBCOLOR en tête de chaque COLOR : trois WORD.
unsafe fn read_rgb(colors: *const u8, i: usize) -> [f32; 3] {
    let p = colors.add(i * COLOR_SIZE) as *const u16;
    [0, 1, 2].map(|c| unit(std::ptr::read_unaligned(p.add(c))))
}

/// TranslateColors : couleurs RVB seulement, en entrée comme en sortie.
pub unsafe extern "win64" fn translate_colors(
    h: usize,
    input: *const u8,
    count: u32,
    input_type: u32,
    output: *mut u8,
    output_type: u32,
) -> i32 {
    let Some(x) = transform(h) else {
        return fail(ERROR_INVALID_TRANSFORM);
    };
    if input.is_null() || output.is_null() || input_type != COLOR_RGB || output_type != COLOR_RGB {
        return fail(ERROR_INVALID_PARAMETER);
    }
    for i in 0..count as usize {
        let (out, _) = x.apply(read_rgb(input, i));
        let p = output.add(i * COLOR_SIZE) as *mut u16;
        for (c, v) in out.iter().enumerate() {
            std::ptr::write_unaligned(p.add(c), word(*v));
        }
    }
    1
}

/// CheckColors : 0 dans la gamme ; sinon 1 à 255, croissant avec l'écart
/// (valeurs linéaires de destination hors de 0..1).
pub unsafe extern "win64" fn check_colors(
    h: usize,
    input: *const u8,
    count: u32,
    input_type: u32,
    result: *mut u8,
) -> i32 {
    let Some(x) = transform(h) else {
        return fail(ERROR_INVALID_TRANSFORM);
    };
    if input.is_null() || result.is_null() || input_type != COLOR_RGB {
        return fail(ERROR_INVALID_PARAMETER);
    }
    for i in 0..count as usize {
        let (_, beyond) = x.apply(read_rgb(input, i));
        *result.add(i) = if beyond <= 1.0 / 65535.0 {
            0
        } else {
            1 + (beyond * 254.0).min(254.0) as u8
        };
    }
    1
}

/// Disposition d'un pixel RVB d'un BMFORMAT : taille, et pour R, G, B le
/// décalage de chaque canal et sa taille en octets. Déduit, pas lu : Learn
/// décrit BM_xRGBQUADS comme 32 bits dont l'octet de poids fort est ignoré ;
/// le nom se lit donc du poids fort au poids faible (x, R, G, B), soit
/// B, G, R, x en mémoire petit-boutiste. La même lecture vaut pour les
/// triplets (24 bits R, G, B → B, G, R en mémoire) et pour 16b_RGB.
fn pixel_layout(format: i32) -> Option<(usize, [usize; 3], usize)> {
    Some(match format {
        2 => (3, [2, 1, 0], 1),  // BM_RGBTRIPLETS
        4 => (3, [0, 1, 2], 1),  // BM_BGRTRIPLETS
        8 => (4, [2, 1, 0], 1),  // BM_xRGBQUADS
        16 => (4, [0, 1, 2], 1), // BM_xBGRQUADS
        10 => (6, [4, 2, 0], 2), // BM_16b_RGB
        _ => return None,
    })
}

/// Pas d'une ligne : celui donné, sinon aligné sur un DWORD.
fn stride(given: u32, width: usize, size: usize) -> usize {
    if given != 0 {
        given as usize
    } else {
        (width * size).div_ceil(4) * 4
    }
}

/// TranslateBitmapBits : formats RVB 8 et 16 bits ; le rappel de progrès est
/// appelé après chaque ligne (maximum : la hauteur), et l'arrête s'il rend
/// FALSE (ICMProgressProcCallback).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn translate_bitmap_bits(
    h: usize,
    src: *const u8,
    input_format: i32,
    width: u32,
    height: u32,
    input_stride: u32,
    dst: *mut u8,
    output_format: i32,
    output_stride: u32,
    callback: usize,
    callback_data: isize,
) -> i32 {
    let Some(x) = transform(h) else {
        return fail(ERROR_INVALID_TRANSFORM);
    };
    let (Some((isz, ioff, iw)), Some((osz, ooff, ow))) =
        (pixel_layout(input_format), pixel_layout(output_format))
    else {
        crate::log_warn!("mscms", "TranslateBitmapBits, format {input_format:#x} → {output_format:#x} non pris en charge ({}:{})", file!(), line!());
        return fail(ERROR_NOT_SUPPORTED);
    };
    if src.is_null() || dst.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (w, rows) = (width as usize, height as usize);
    let (is, os) = (stride(input_stride, w, isz), stride(output_stride, w, osz));
    let progress: Option<unsafe extern "win64" fn(u32, u32, isize) -> i32> =
        (callback != 0).then(|| std::mem::transmute(callback));
    for y in 0..rows {
        let (ir, or) = (src.add(y * is), dst.add(y * os));
        for i in 0..w {
            let (ip, op) = (ir.add(i * isz), or.add(i * osz));
            let out = if iw == 1 {
                x.apply8([0, 1, 2].map(|c| *ip.add(ioff[c])))
            } else {
                x.apply(
                    [0, 1, 2]
                        .map(|c| unit(std::ptr::read_unaligned(ip.add(ioff[c]) as *const u16))),
                )
                .0
            };
            // L'octet ignoré d'un quadruplet est recopié quand l'entrée en a un.
            if osz == 4 {
                *op.add(3) = if isz == 4 { *ip.add(3) } else { 0 };
            }
            for c in 0..3 {
                if ow == 1 {
                    *op.add(ooff[c]) = (out[c].clamp(0.0, 1.0) * 255.0).round() as u8;
                } else {
                    std::ptr::write_unaligned(op.add(ooff[c]) as *mut u16, word(out[c]));
                }
            }
        }
        if let Some(f) = progress {
            if f(height, y as u32 + 1, callback_data) == 0 {
                return fail(ERROR_CANCELLED);
            }
        }
    }
    1
}

/// SelectCMM : seul le module par défaut existe ; 0 le désigne aussi
/// (Learn, Windows 2000). Un autre doit être enregistré : refusé.
pub extern "win64" fn select_cmm(cmm: u32) -> i32 {
    if cmm == 0 || cmm == CMM_WINDOWS_DEFAULT {
        1
    } else {
        fail(ERROR_INVALID_CMM)
    }
}

// ENUMTYPE (x64) : dwFields à 8, pDeviceName à 16, puis les DWORD.
const ET_DEVICENAME: u32 = 0x1;
const ET_MEDIATYPE: u32 = 0x2;
const ET_DITHERMODE: u32 = 0x4;
const ET_RESOLUTION: u32 = 0x8;
const ET_DEVICECLASS: u32 = 0x10000;

/// Champs d'ENUMTYPE comparés à l'en-tête du profil (ordre machine) :
/// (bit, décalage dans ENUMTYPE x64, décalage dans l'en-tête, nombre de DWORD).
const ENUM_HEADER_FIELDS: [(u32, usize, usize, usize); 11] = [
    (0x10, 40, 4, 1),    // ET_CMMTYPE
    (0x20, 44, 12, 1),   // ET_CLASS
    (0x40, 48, 16, 1),   // ET_DATACOLORSPACE
    (0x80, 52, 20, 1),   // ET_CONNECTIONSPACE
    (0x100, 56, 36, 1),  // ET_SIGNATURE
    (0x200, 60, 40, 1),  // ET_PLATFORM
    (0x400, 64, 44, 1),  // ET_PROFILEFLAGS
    (0x800, 68, 48, 1),  // ET_MANUFACTURER
    (0x1000, 72, 52, 1), // ET_MODEL
    (0x2000, 76, 56, 2), // ET_ATTRIBUTES
    (0x4000, 84, 64, 1), // ET_RENDERINGINTENT
];
const ET_CREATOR: (u32, usize, usize) = (0x8000, 88, 80);

/// Profils du dossier des couleurs (et le profil sRGB standard, toujours
/// servi), triés par nom.
fn color_directory_profiles() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(dir) = std::fs::read_dir(crate::paths::to_linux(COLOR_DIRECTORY)) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".icm") || lower.ends_with(".icc")) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(e.path()) {
                if valid(&bytes) {
                    out.push((name, bytes));
                }
            }
        }
    }
    if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(SRGB_NAME)) {
        out.push((SRGB_NAME.to_string(), crate::dlls::icc::srgb()));
    }
    out.sort_by_key(|(n, _)| n.to_ascii_lowercase());
    out
}

/// Le profil répond-il aux critères ? Les critères de périphérique (nom,
/// support, tramage, résolution) ne retiennent aucun profil : aucun profil
/// n'est associé à un périphérique ici.
unsafe fn matches(record: *const u8, profile: &[u8]) -> bool {
    let dw = |at: usize| std::ptr::read_unaligned(record.add(at) as *const u32);
    let fields = dw(8);
    if fields & (ET_DEVICENAME | ET_MEDIATYPE | ET_DITHERMODE | ET_RESOLUTION) != 0 {
        return false;
    }
    let header = native_header(profile);
    let hd = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
    if fields & ET_DEVICECLASS != 0 && dw(92) != hd(12) {
        return false;
    }
    for (bit, rec, head, n) in ENUM_HEADER_FIELDS {
        if fields & bit != 0 && (0..n).any(|k| dw(rec + 4 * k) != hd(head + 4 * k)) {
            return false;
        }
    }
    let (bit, rec, head) = ET_CREATOR;
    !(fields & bit != 0 && dw(rec) != hd(head))
}

/// EnumColorProfiles : noms de fichier en MULTI_SZ. Tampon nul ou trop petit :
/// la taille nécessaire (en octets) et ERROR_INSUFFICIENT_BUFFER.
unsafe fn enum_color_profiles(
    machine: *const u8,
    record: *const u8,
    buffer: *mut u8,
    size: *mut u32,
    count: *mut u32,
    wide: bool,
) -> i32 {
    if !machine.is_null() || record.is_null() || size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let names: Vec<String> = color_directory_profiles()
        .into_iter()
        .filter(|(_, p)| matches(record, p))
        .map(|(n, _)| n)
        .collect();
    let mut out: Vec<u8> = Vec::new();
    for n in names.iter().map(String::as_str).chain(std::iter::once("")) {
        if wide {
            for u in n.encode_utf16().chain(std::iter::once(0)) {
                out.extend_from_slice(&u.to_le_bytes());
            }
        } else {
            out.extend_from_slice(n.as_bytes());
            out.push(0);
        }
    }
    if names.is_empty() {
        // MULTI_SZ vide : deux terminateurs.
        out.extend_from_slice(if wide { &[0, 0] } else { &[0] });
    }
    let room = *size as usize;
    *size = out.len() as u32;
    if buffer.is_null() || room < out.len() {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buffer, out.len());
    if !count.is_null() {
        *count = names.len() as u32;
    }
    1
}

pub unsafe extern "win64" fn enum_color_profiles_w(
    machine: *const u16,
    record: *const u8,
    buffer: *mut u8,
    size: *mut u32,
    count: *mut u32,
) -> i32 {
    enum_color_profiles(machine.cast(), record, buffer, size, count, true)
}

pub unsafe extern "win64" fn enum_color_profiles_a(
    machine: *const u8,
    record: *const u8,
    buffer: *mut u8,
    size: *mut u32,
    count: *mut u32,
) -> i32 {
    enum_color_profiles(machine, record, buffer, size, count, false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OpenColorProfileW" => fn open_color_profile_w,
        "OpenColorProfileA" => fn open_color_profile_a,
        "WcsOpenColorProfileW" => fn wcs_open_color_profile_w,
        "WcsCreateIccProfile" => fn wcs_create_icc_profile,
        "CloseColorProfile" => fn close_color_profile,
        "GetColorProfileFromHandle" => fn get_color_profile_from_handle,
        "GetColorProfileHeader" => fn get_color_profile_header,
        "IsColorProfileValid" => fn is_color_profile_valid,
        "GetCountColorProfileElements" => fn get_count_color_profile_elements,
        "GetColorProfileElementTag" => fn get_color_profile_element_tag,
        "IsColorProfileTagPresent" => fn is_color_profile_tag_present,
        "GetColorProfileElement" => fn get_color_profile_element,
        "GetStandardColorSpaceProfileW" => fn get_standard_color_space_profile_w,
        "GetColorDirectoryW" => fn get_color_directory_w,
        "CreateMultiProfileTransform" => fn create_multi_profile_transform,
        "DeleteColorTransform" => fn delete_color_transform,
        "TranslateColors" => fn translate_colors,
        "CheckColors" => fn check_colors,
        "TranslateBitmapBits" => fn translate_bitmap_bits,
        "SelectCMM" => fn select_cmm,
        "EnumColorProfilesW" => fn enum_color_profiles_w,
        "EnumColorProfilesA" => fn enum_color_profiles_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct Profile {
        kind: u32,
        data: *const u8,
        size: u32,
    }

    /// #1363 : Paint.NET ouvre le profil sRGB par mscms (SafeWcsProfileHandle).
    #[test]
    fn profil_ouvert_lu_puis_ferme() {
        unsafe {
            let srgb = crate::dlls::icc::srgb();
            let p = Profile {
                kind: PROFILE_MEMBUFFER,
                data: srgb.as_ptr(),
                size: srgb.len() as u32,
            };
            let h = wcs_open_color_profile_w(
                (&p as *const Profile).cast(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                1,
                3,
                0,
            );
            assert_ne!(h, 0);
            let mut size = 0u32;
            assert_eq!(
                get_color_profile_from_handle(h, std::ptr::null_mut(), &mut size),
                0
            );
            assert_eq!(size as usize, srgb.len());
            let mut buf = vec![0u8; size as usize];
            assert_eq!(
                get_color_profile_from_handle(h, buf.as_mut_ptr(), &mut size),
                1
            );
            assert_eq!(buf, srgb);
            let mut header = [0u8; 128];
            assert_eq!(get_color_profile_header(h, header.as_mut_ptr()), 1);
            let field = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
            assert_eq!(field(0) as usize, srgb.len());
            assert_eq!(field(12), u32::from_be_bytes(*b"mntr"));
            assert_eq!(field(36), u32::from_be_bytes(*b"acsp"));
            // Illuminant D50 en FXPT2DOT30 : Y = 1.0.
            assert_eq!(field(72), 1 << 30);
            let mut present = 0;
            assert_eq!(
                is_color_profile_tag_present(h, u32::from_be_bytes(*b"rXYZ"), &mut present),
                1
            );
            assert_eq!(present, 1);
            let mut n = 0u32;
            assert_eq!(
                get_color_profile_element(
                    h,
                    u32::from_be_bytes(*b"wtpt"),
                    0,
                    &mut n,
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                1
            );
            assert_eq!(n, 20);
            let icc = wcs_create_icc_profile(h, 0);
            assert_ne!(icc, 0);
            assert_eq!(close_color_profile(icc), 1);
            assert_eq!(close_color_profile(h), 1);
            assert_eq!(close_color_profile(h), 0);

            // Profil sRGB standard par son nom, et données invalides refusées.
            let name: Vec<u16> = SRGB_NAME.encode_utf16().chain([0]).collect();
            let f = Profile {
                kind: PROFILE_FILENAME,
                data: name.as_ptr().cast(),
                size: 0,
            };
            let h = open_color_profile_w((&f as *const Profile).cast(), 1, 1, 3);
            assert_ne!(h, 0);
            close_color_profile(h);
            let junk = [0u8; 64];
            let j = Profile {
                kind: PROFILE_MEMBUFFER,
                data: junk.as_ptr(),
                size: 64,
            };
            assert_eq!(
                open_color_profile_w((&j as *const Profile).cast(), 1, 1, 3),
                0
            );
        }
    }

    /// Profil ouvert depuis la mémoire.
    unsafe fn open_bytes(bytes: &[u8]) -> usize {
        let p = Profile {
            kind: PROFILE_MEMBUFFER,
            data: bytes.as_ptr(),
            size: bytes.len() as u32,
        };
        open_color_profile_w((&p as *const Profile).cast(), 1, 1, 3)
    }

    /// sRGB dont les colorants sont ceux d'Adobe RGB (1998), gamme plus large.
    fn wide_profile() -> Vec<u8> {
        let mut p = crate::dlls::icc::srgb();
        let wide = crate::dlls::icc::adobe_rgb_shaper().matrix;
        let count = u32::from_be_bytes(p[128..132].try_into().unwrap()) as usize;
        for i in 0..count {
            let e = 132 + 12 * i;
            let col = match &p[e..e + 4] {
                b"rXYZ" => 0,
                b"gXYZ" => 1,
                b"bXYZ" => 2,
                _ => continue,
            };
            let off = u32::from_be_bytes(p[e + 4..e + 8].try_into().unwrap()) as usize;
            for row in 0..3 {
                let v = (wide[row][col] * 65536.0).round() as i32;
                p[off + 8 + 4 * row..off + 12 + 4 * row].copy_from_slice(&v.to_be_bytes());
            }
        }
        p
    }

    fn colors(rgb: &[[u16; 3]]) -> Vec<[u8; COLOR_SIZE]> {
        rgb.iter()
            .map(|c| {
                let mut b = [0u8; COLOR_SIZE];
                for (k, v) in c.iter().enumerate() {
                    b[2 * k..2 * k + 2].copy_from_slice(&v.to_le_bytes());
                }
                b
            })
            .collect()
    }

    fn rgb_of(c: &[u8; COLOR_SIZE]) -> [u16; 3] {
        [0, 1, 2].map(|k| u16::from_le_bytes([c[2 * k], c[2 * k + 1]]))
    }

    static mut CALLS: u32 = 0;
    unsafe extern "win64" fn stop_after_first(_max: u32, _cur: u32, data: isize) -> i32 {
        CALLS += 1;
        (data != 7) as i32
    }

    /// #3103 : transformations de couleurs demandées par Acrobat.
    #[test]
    fn transformations_de_couleurs_3103() {
        let _teb = crate::process::test_teb();
        unsafe {
            let srgb = crate::dlls::icc::srgb();
            let scrgb = crate::dlls::icc::scrgb();
            let wide = wide_profile();
            let (a, b, l, w) = (
                open_bytes(&srgb),
                open_bytes(&srgb),
                open_bytes(&scrgb),
                open_bytes(&wide),
            );
            assert!(a != 0 && b != 0 && l != 0 && w != 0);
            let intent = [INTENT_RELATIVE_COLORIMETRIC];

            // sRGB → sRGB : les couleurs ne bougent pas (à l'arrondi près).
            let same = create_multi_profile_transform([a, b].as_ptr(), 2, intent.as_ptr(), 1, 0, 0);
            assert_ne!(same, 0);
            let input = colors(&[[0, 0, 0], [65535, 65535, 65535], [65535, 32768, 4096]]);
            let mut out = colors(&[[1, 1, 1]; 3]);
            assert_eq!(
                translate_colors(
                    same,
                    input.as_ptr().cast(),
                    3,
                    COLOR_RGB,
                    out.as_mut_ptr().cast(),
                    COLOR_RGB
                ),
                1
            );
            for (i, o) in input.iter().zip(&out) {
                for (x, y) in rgb_of(i).iter().zip(rgb_of(o)) {
                    assert!(
                        (*x as i32 - y as i32).abs() <= 64,
                        "{:?} → {:?}",
                        rgb_of(i),
                        rgb_of(o)
                    );
                }
            }
            let mut res = [9u8; 3];
            assert_eq!(
                check_colors(same, input.as_ptr().cast(), 3, COLOR_RGB, res.as_mut_ptr()),
                1
            );
            assert_eq!(res, [0, 0, 0]);

            // sRGB → scRGB (linéaire) : un gris à 50 % vaut 21,4 % de lumière.
            let lin = create_multi_profile_transform([a, l].as_ptr(), 2, intent.as_ptr(), 1, 0, 0);
            let gray = colors(&[[32768, 32768, 32768]]);
            let mut g = colors(&[[0, 0, 0]]);
            assert_eq!(
                translate_colors(
                    lin,
                    gray.as_ptr().cast(),
                    1,
                    COLOR_RGB,
                    g.as_mut_ptr().cast(),
                    COLOR_RGB
                ),
                1
            );
            for v in rgb_of(&g[0]) {
                assert!((v as f32 / 65535.0 - 0.2140).abs() < 0.002, "{v}");
            }

            // Rouge pur d'une gamme large : hors de la gamme sRGB, pas le gris.
            let narrow =
                create_multi_profile_transform([w, a].as_ptr(), 2, intent.as_ptr(), 1, 0, 0);
            let probe = colors(&[[65535, 0, 0], [32768, 32768, 32768]]);
            let mut res = [0u8; 2];
            assert_eq!(
                check_colors(
                    narrow,
                    probe.as_ptr().cast(),
                    2,
                    COLOR_RGB,
                    res.as_mut_ptr()
                ),
                1
            );
            assert!(res[0] > 0, "{res:?}");
            assert_eq!(res[1], 0);

            // Bitmaps : xRGB (B, G, R, x en mémoire) vers triplets BGR (R, G, B),
            // pas de ligne aligné sur un DWORD, octet ignoré recopié.
            let src = [10u8, 20, 30, 0xAA, 40, 50, 60, 0xBB];
            let mut dst = [0u8; 8];
            assert_eq!(
                translate_bitmap_bits(same, src.as_ptr(), 8, 2, 1, 0, dst.as_mut_ptr(), 4, 0, 0, 0),
                1
            );
            let expect = [30u8, 20, 10, 60, 50, 40];
            for (x, y) in expect.iter().zip(&dst[..6]) {
                assert!((*x as i32 - *y as i32).abs() <= 1, "{dst:?}");
            }
            let mut quads = [0u8; 8];
            assert_eq!(
                translate_bitmap_bits(
                    same,
                    src.as_ptr(),
                    8,
                    2,
                    1,
                    0,
                    quads.as_mut_ptr(),
                    8,
                    0,
                    0,
                    0
                ),
                1
            );
            assert_eq!((quads[3], quads[7]), (0xAA, 0xBB));
            // Le rappel de progrès arrête la traduction.
            let rows = [0u8; 16];
            let mut out4 = [0u8; 16];
            CALLS = 0;
            assert_eq!(
                translate_bitmap_bits(
                    same,
                    rows.as_ptr(),
                    8,
                    1,
                    4,
                    4,
                    out4.as_mut_ptr(),
                    8,
                    4,
                    stop_after_first as usize,
                    7
                ),
                0
            );
            assert_eq!(CALLS, 1);
            assert_eq!(crate::process::last_error(), ERROR_CANCELLED);
            assert_eq!(
                translate_bitmap_bits(
                    same,
                    rows.as_ptr(),
                    8,
                    1,
                    4,
                    4,
                    out4.as_mut_ptr(),
                    8,
                    4,
                    stop_after_first as usize,
                    1
                ),
                1
            );
            // Format non RVB : refusé.
            assert_eq!(
                translate_bitmap_bits(
                    same,
                    rows.as_ptr(),
                    32,
                    1,
                    1,
                    0,
                    out4.as_mut_ptr(),
                    8,
                    0,
                    0,
                    0
                ),
                0
            );
            // Types de couleur autres que RVB : refusés.
            assert_eq!(
                translate_colors(
                    same,
                    gray.as_ptr().cast(),
                    1,
                    7,
                    g.as_mut_ptr().cast(),
                    COLOR_RGB
                ),
                0
            );

            // Refus : un seul profil, intentions en nombre invalide, profil gris.
            assert_eq!(
                create_multi_profile_transform([a].as_ptr(), 1, intent.as_ptr(), 1, 0, 0),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_NOT_SUPPORTED);
            let three = [0u32; 3];
            assert_eq!(
                create_multi_profile_transform([a, b].as_ptr(), 2, three.as_ptr(), 3, 0, 0),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INVALID_PARAMETER);
            let mut grayp = srgb.clone();
            grayp[16..20].copy_from_slice(b"GRAY");
            let gp = open_bytes(&grayp);
            assert_eq!(
                create_multi_profile_transform([gp, a].as_ptr(), 2, intent.as_ptr(), 1, 0, 0),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_NOT_SUPPORTED);

            // Absolue : blancs du support identiques, même résultat que relative.
            let abs = create_multi_profile_transform(
                [a, l].as_ptr(),
                2,
                [INTENT_ABSOLUTE_COLORIMETRIC].as_ptr(),
                1,
                0,
                0,
            );
            assert_ne!(abs, 0);

            for t in [same, lin, narrow, abs] {
                assert_eq!(delete_color_transform(t), 1);
            }
            assert_eq!(delete_color_transform(same), 0);
            assert_eq!(
                translate_colors(
                    same,
                    gray.as_ptr().cast(),
                    1,
                    COLOR_RGB,
                    g.as_mut_ptr().cast(),
                    COLOR_RGB
                ),
                0
            );
            for h in [a, b, l, w, gp] {
                close_color_profile(h);
            }

            assert_eq!(select_cmm(CMM_WINDOWS_DEFAULT), 1);
            assert_eq!(select_cmm(u32::from_be_bytes(*b"ADBE")), 0);
        }
    }

    /// #3103 : EnumColorProfiles, critères de l'en-tête et de périphérique.
    #[test]
    fn enumeration_des_profils_3103() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut record = [0u8; 96];
            let set = |r: &mut [u8; 96], at: usize, v: u32| {
                r[at..at + 4].copy_from_slice(&v.to_le_bytes())
            };
            set(&mut record, 0, 96);
            set(&mut record, 4, 0x300);
            set(&mut record, 8, 0x20); // ET_CLASS
            set(&mut record, 44, u32::from_be_bytes(*b"mntr"));
            let mut size = 0u32;
            let mut n = 0u32;
            assert_eq!(
                enum_color_profiles_w(
                    std::ptr::null(),
                    record.as_ptr(),
                    std::ptr::null_mut(),
                    &mut size,
                    &mut n
                ),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INSUFFICIENT_BUFFER);
            let mut buf = vec![0u8; size as usize];
            assert_eq!(
                enum_color_profiles_w(
                    std::ptr::null(),
                    record.as_ptr(),
                    buf.as_mut_ptr(),
                    &mut size,
                    &mut n
                ),
                1
            );
            assert!(n >= 1);
            let wide: Vec<u16> = buf
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let names: Vec<String> = wide
                .split(|&c| c == 0)
                .filter(|s| !s.is_empty())
                .map(String::from_utf16_lossy)
                .collect();
            assert!(names.iter().any(|s| s == SRGB_NAME), "{names:?}");
            assert_eq!(&wide[wide.len() - 2..], &[0, 0]);

            // Classe imprimante : pas le profil d'écran sRGB.
            set(&mut record, 44, u32::from_be_bytes(*b"prtr"));
            let mut size = 0u32;
            enum_color_profiles_a(
                std::ptr::null(),
                record.as_ptr(),
                std::ptr::null_mut(),
                &mut size,
                &mut n,
            );
            let mut buf = vec![0u8; size as usize];
            assert_eq!(
                enum_color_profiles_a(
                    std::ptr::null(),
                    record.as_ptr(),
                    buf.as_mut_ptr(),
                    &mut size,
                    &mut n
                ),
                1
            );
            assert!(!String::from_utf8_lossy(&buf).contains(SRGB_NAME));

            // Aucun profil associé à un périphérique.
            set(&mut record, 8, 0x1);
            let mut size = 16u32;
            let mut buf = [0xFFu8; 16];
            assert_eq!(
                enum_color_profiles_w(
                    std::ptr::null(),
                    record.as_ptr(),
                    buf.as_mut_ptr(),
                    &mut size,
                    &mut n
                ),
                1
            );
            assert_eq!(n, 0);
            assert_eq!(&buf[..4], &[0, 0, 0, 0]);
        }
    }
}
