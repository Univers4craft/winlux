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
}
