//! CoInternetCreateSecurityManager (urlmon.dll) : gestionnaire de zones (#1062).
//!
//! WPF demande la zone de sécurité de chaque URI d'image avant de la décoder
//! (IInternetSecurityManager::MapUrlToZone). Winlux n'a pas de réglage de
//! zones : un fichier local est en zone « Ordinateur local » (0), une
//! adresse de la machine elle-même en « Intranet local » (1), tout le reste
//! en « Internet » (3), comme le classement par défaut décrit par la
//! documentation publique d'urlmon. Les actions sont autorisées
//! (URLPOLICY_ALLOW) ; le mappage de zones n'est pas modifiable.

use std::ffi::c_void;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

const URLZONE_LOCAL_MACHINE: u32 = 0;
const URLZONE_INTRANET: u32 = 1;
const URLZONE_INTERNET: u32 = 3;

type Guid = [u8; 16];
const IID_IUNKNOWN: Guid = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
/// IID_IInternetSecurityManager {79EAC9EE-BAF9-11CE-8C82-00AA004BA90B}.
const IID_SECURITY_MANAGER: Guid = [
    0xEE, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];

#[repr(C)]
struct Manager {
    vtbl: *const [usize; 11],
}

fn vtbl() -> &'static [usize; 11] {
    static VTBL: std::sync::OnceLock<[usize; 11]> = std::sync::OnceLock::new();
    VTBL.get_or_init(|| {
        [
            query_interface as *const () as usize,
            add_ref as *const () as usize,
            release as *const () as usize,
            set_security_site as *const () as usize,
            get_security_site as *const () as usize,
            map_url_to_zone as *const () as usize,
            get_security_id as *const () as usize,
            process_url_action as *const () as usize,
            query_custom_policy as *const () as usize,
            not_implemented3 as *const () as usize,
            not_implemented3 as *const () as usize,
        ]
    })
}

/// Objet unique, sans état : le compteur de références est fictif.
fn manager() -> *mut c_void {
    static MANAGER: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MANAGER.get_or_init(|| Box::into_raw(Box::new(Manager { vtbl: vtbl() })) as usize)
        as *mut c_void
}

unsafe extern "win64" fn query_interface(
    this: *mut c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    if *iid == IID_IUNKNOWN || *iid == IID_SECURITY_MANAGER {
        *out = this;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "win64" fn add_ref(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "win64" fn release(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "win64" fn set_security_site(_this: *mut c_void, _site: *mut c_void) -> i32 {
    S_OK
}

unsafe extern "win64" fn get_security_site(_this: *mut c_void, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    E_NOTIMPL
}

/// Zone d'une URL, d'après son schéma et son hôte.
fn zone_of(url: &str) -> u32 {
    let lower = url.trim().to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if lower.starts_with("file:") || drive || lower.starts_with('\\') || lower.starts_with('/') {
        return URLZONE_LOCAL_MACHINE;
    }
    let Some((_, rest)) = lower.split_once("://") else {
        return URLZONE_INTERNET;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.split_once(']').map(|(h, _)| h))
        .unwrap_or_else(|| host.split(':').next().unwrap_or(""));
    if host == "localhost" || host == "::1" || host.starts_with("127.") {
        return URLZONE_INTRANET;
    }
    URLZONE_INTERNET
}

unsafe extern "win64" fn map_url_to_zone(
    _this: *mut c_void,
    url: *const u16,
    zone: *mut u32,
    _flags: u32,
) -> i32 {
    if url.is_null() || zone.is_null() {
        return E_INVALIDARG;
    }
    *zone = zone_of(&crate::dlls::wstr(url));
    S_OK
}

unsafe extern "win64" fn get_security_id(
    _this: *mut c_void,
    _url: *const u16,
    _id: *mut u8,
    _len: *mut u32,
    _reserved: usize,
) -> i32 {
    E_NOTIMPL
}

/// ProcessUrlAction(url, action, policy, cbPolicy, context, cbContext, flags, reserved).
unsafe extern "win64" fn process_url_action(
    _this: *mut c_void,
    _url: *const u16,
    _action: u32,
    policy: *mut u8,
    cb_policy: u32,
    _context: *mut u8,
    _cb_context: u32,
    _flags: u32,
    _reserved: u32,
) -> i32 {
    if !policy.is_null() && cb_policy >= 4 {
        // URLPOLICY_ALLOW.
        (policy as *mut u32).write_unaligned(0);
    }
    S_OK
}

unsafe extern "win64" fn query_custom_policy(
    _this: *mut c_void,
    _url: *const u16,
    _key: *const Guid,
    policy: *mut *mut u8,
    cb: *mut u32,
    _context: *mut u8,
    _cb_context: u32,
    _reserved: u32,
) -> i32 {
    if !policy.is_null() {
        *policy = std::ptr::null_mut();
    }
    if !cb.is_null() {
        *cb = 0;
    }
    E_NOTIMPL
}

/// SetZoneMapping / GetZoneMappings : le mappage n'est pas modifiable.
unsafe extern "win64" fn not_implemented3(
    _this: *mut c_void,
    _a: usize,
    _b: usize,
    _c: usize,
) -> i32 {
    E_NOTIMPL
}

/// CoInternetCreateSecurityManager(pSP, ppSM, dwReserved).
pub unsafe extern "win64" fn co_internet_create_security_manager(
    _sp: *mut c_void,
    out: *mut *mut c_void,
    _reserved: u32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = manager();
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_par_defaut() {
        assert_eq!(zone_of(r"C:\Users\a\image.png"), URLZONE_LOCAL_MACHINE);
        assert_eq!(zone_of("file:///C:/a.png"), URLZONE_LOCAL_MACHINE);
        assert_eq!(zone_of("http://localhost:8080/a.png"), URLZONE_INTRANET);
        assert_eq!(zone_of("https://127.0.0.1/a"), URLZONE_INTRANET);
        assert_eq!(zone_of("http://[::1]/a"), URLZONE_INTRANET);
        assert_eq!(zone_of("https://exemple.org/a.png"), URLZONE_INTERNET);
    }

    #[test]
    fn gestionnaire_cree_et_interroge() {
        unsafe {
            let mut m = std::ptr::null_mut();
            assert_eq!(
                co_internet_create_security_manager(std::ptr::null_mut(), &mut m, 0),
                S_OK
            );
            let mut q = std::ptr::null_mut();
            assert_eq!(query_interface(m, &IID_SECURITY_MANAGER, &mut q), S_OK);
            assert_eq!(q, m);
            let vt = *(m as *const *const usize);
            type Map = unsafe extern "win64" fn(*mut c_void, *const u16, *mut u32, u32) -> i32;
            let map: Map = std::mem::transmute(*vt.add(5));
            let url: Vec<u16> = "file:///C:/a.png".encode_utf16().chain(Some(0)).collect();
            let mut z = 99;
            assert_eq!(map(m, url.as_ptr(), &mut z, 0), S_OK);
            assert_eq!(z, URLZONE_LOCAL_MACHINE);
        }
    }
}
