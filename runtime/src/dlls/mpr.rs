//! mpr.dll : routeur des fournisseurs réseau (famille WNet*).
//!
//! Winlux ne branche pas de fournisseur réseau Windows : les lecteurs
//! réseau passent par la page Lecteurs réseau des Paramètres (mount.cifs,
//! sshfs, rclone). Ces fonctions répondent donc comme un poste sans
//! connexion réseau mappée, ce qui suffit aux programmes qui ne font que
//! vérifier un chemin (Audacity 4).
#![allow(non_snake_case)]

const NO_ERROR: u32 = 0;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_NETWORK: u32 = 1222;
const ERROR_NOT_CONNECTED: u32 = 2250;

fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Utilisateur".into())
}

/// Écrit `text` (UTF-16 terminé par 0) dans `buf` de `*len` caractères ;
/// `*len` reçoit la taille nécessaire si elle manque.
unsafe fn put_w(text: &str, buf: *mut u16, len: *mut u32) -> u32 {
    if len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    if buf.is_null() || (*len as usize) < w.len() {
        *len = w.len() as u32;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    NO_ERROR
}

unsafe fn put_a(text: &str, buf: *mut u8, len: *mut u32) -> u32 {
    if len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let mut a = crate::dlls::encode_ansi(text);
    a.push(0);
    if buf.is_null() || (*len as usize) < a.len() {
        *len = a.len() as u32;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, a.len());
    NO_ERROR
}

pub unsafe extern "win64" fn WNetGetConnection(_local: usize, _remote: usize, _len: usize) -> u32 {
    ERROR_NOT_CONNECTED
}
pub unsafe extern "win64" fn WNetAddConnection2(
    _res: usize,
    _password: usize,
    _user: usize,
    _flags: u32,
) -> u32 {
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn WNetAddConnection3(
    _owner: usize,
    _res: usize,
    _password: usize,
    _user: usize,
    _flags: u32,
) -> u32 {
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn WNetCancelConnection2(_name: usize, _flags: u32, _force: i32) -> u32 {
    ERROR_NOT_CONNECTED
}
pub unsafe extern "win64" fn WNetOpenEnum(
    _scope: u32,
    _kind: u32,
    _usage: u32,
    _res: usize,
    handle: *mut usize,
) -> u32 {
    if !handle.is_null() {
        *handle = 0;
    }
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn WNetEnumResource(
    _handle: usize,
    count: *mut u32,
    _buf: usize,
    _size: usize,
) -> u32 {
    if !count.is_null() {
        *count = 0;
    }
    ERROR_NO_MORE_ITEMS
}
pub extern "win64" fn WNetCloseEnum(_handle: usize) -> u32 {
    NO_ERROR
}
pub unsafe extern "win64" fn WNetGetUniversalName(
    _local: usize,
    _level: u32,
    _buf: usize,
    _size: usize,
) -> u32 {
    ERROR_NOT_CONNECTED
}
pub unsafe extern "win64" fn WNetGetResourceInformation(
    _res: usize,
    _buf: usize,
    _size: usize,
    _system: usize,
) -> u32 {
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn WNetGetProviderName(_kind: u32, _buf: usize, _size: usize) -> u32 {
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn WNetGetUserW(_name: usize, user: *mut u16, len: *mut u32) -> u32 {
    put_w(&user_name(), user, len)
}
pub unsafe extern "win64" fn WNetGetUserA(_name: usize, user: *mut u8, len: *mut u32) -> u32 {
    put_a(&user_name(), user, len)
}
pub unsafe extern "win64" fn WNetGetLastErrorW(
    error: *mut u32,
    text: *mut u16,
    text_len: u32,
    provider: *mut u16,
    provider_len: u32,
) -> u32 {
    if !error.is_null() {
        *error = NO_ERROR;
    }
    if !text.is_null() && text_len > 0 {
        *text = 0;
    }
    if !provider.is_null() && provider_len > 0 {
        *provider = 0;
    }
    NO_ERROR
}
pub unsafe extern "win64" fn WNetGetLastErrorA(
    error: *mut u32,
    text: *mut u8,
    text_len: u32,
    provider: *mut u8,
    provider_len: u32,
) -> u32 {
    if !error.is_null() {
        *error = NO_ERROR;
    }
    if !text.is_null() && text_len > 0 {
        *text = 0;
    }
    if !provider.is_null() && provider_len > 0 {
        *provider = 0;
    }
    NO_ERROR
}
pub unsafe extern "win64" fn WNetConnectionDialog(_owner: usize, _kind: u32) -> u32 {
    ERROR_NO_NETWORK
}
pub unsafe extern "win64" fn MultinetGetConnectionPerformance(_res: usize, _info: usize) -> u32 {
    ERROR_NOT_CONNECTED
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WNetGetConnectionW" => fn WNetGetConnection,
        "WNetGetConnectionA" => fn WNetGetConnection,
        "WNetAddConnection2W" => fn WNetAddConnection2,
        "WNetAddConnection2A" => fn WNetAddConnection2,
        "WNetAddConnection3W" => fn WNetAddConnection3,
        "WNetAddConnection3A" => fn WNetAddConnection3,
        "WNetCancelConnection2W" => fn WNetCancelConnection2,
        "WNetCancelConnection2A" => fn WNetCancelConnection2,
        "WNetOpenEnumW" => fn WNetOpenEnum,
        "WNetOpenEnumA" => fn WNetOpenEnum,
        "WNetEnumResourceW" => fn WNetEnumResource,
        "WNetEnumResourceA" => fn WNetEnumResource,
        "WNetCloseEnum" => fn WNetCloseEnum,
        "WNetGetUniversalNameW" => fn WNetGetUniversalName,
        "WNetGetUniversalNameA" => fn WNetGetUniversalName,
        "WNetGetResourceInformationW" => fn WNetGetResourceInformation,
        "WNetGetResourceInformationA" => fn WNetGetResourceInformation,
        "WNetGetProviderNameW" => fn WNetGetProviderName,
        "WNetGetProviderNameA" => fn WNetGetProviderName,
        "WNetGetUserW" => fn WNetGetUserW,
        "WNetGetUserA" => fn WNetGetUserA,
        "WNetGetLastErrorW" => fn WNetGetLastErrorW,
        "WNetGetLastErrorA" => fn WNetGetLastErrorA,
        "WNetConnectionDialog" => fn WNetConnectionDialog,
        "MultinetGetConnectionPerformanceW" => fn MultinetGetConnectionPerformance,
        "MultinetGetConnectionPerformanceA" => fn MultinetGetConnectionPerformance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpr_sans_connexion_reseau() {
        assert!(crate::dlls::lookup("mpr", "WNetGetConnectionW").is_some());
        unsafe {
            assert_eq!(WNetGetConnection(0, 0, 0), ERROR_NOT_CONNECTED);
            let mut len = 1u32;
            let mut buf = [0u16; 64];
            assert_eq!(WNetGetUserW(0, buf.as_mut_ptr(), &mut len), ERROR_MORE_DATA);
            assert!(len > 1);
            assert_eq!(WNetGetUserW(0, buf.as_mut_ptr(), &mut len), NO_ERROR);
            assert_eq!(buf[len as usize - 1], 0);
            let mut h = 1usize;
            assert_eq!(WNetOpenEnum(2, 1, 0, 0, &mut h), ERROR_NO_NETWORK);
            assert_eq!(h, 0);
        }
    }
}
