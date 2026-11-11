//! fwpuclnt.dll (pare-feu WFP), netapi32.dll (comptes locaux) et
//! wtsapi32.dll (sessions Terminal Server).
//!
//! Ces trois DLL sont importées statiquement par des programmes qui ne
//! s'en servent que pour des fonctions optionnelles : le bac à sable
//! Windows de Claude Code (comptes « sandbox », filtres réseau WFP), des
//! outils d'administration… Sans elles, le chargement échouait avant même
//! le point d'entrée. Winlux n'a pas de comptes Windows séparés ni de
//! moteur WFP : chaque fonction répond par un code d'erreur Windows
//! documenté, que ces programmes savent traiter (ils se replient sur un
//! fonctionnement sans bac à sable). Les vrais comptes et le vrai pare-feu
//! restent ceux de Linux, gérés par les Paramètres de Winlux.
#![allow(non_snake_case)]

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NOT_SUPPORTED: u32 = 50;
const NERR_USER_NOT_FOUND: u32 = 2221;
const NERR_GROUP_NOT_FOUND: u32 = 2220;
/// FWP_E_NOT_FOUND et compagnie sont des HRESULT-like ; on renvoie
/// ERROR_NOT_SUPPORTED, que Fwpm* documente comme un DWORD Win32.
const FWP_UNAVAILABLE: u32 = ERROR_NOT_SUPPORTED;

// ---- fwpuclnt.dll ---------------------------------------------------------

unsafe extern "win64" fn FwpmEngineOpen0(
    _server: usize,
    _authn: u32,
    _auth_identity: usize,
    _session: usize,
    engine: *mut usize,
) -> u32 {
    if !engine.is_null() {
        *engine = 0;
    }
    FWP_UNAVAILABLE
}
unsafe extern "win64" fn fwp_close(_h: usize) -> u32 {
    0
}
unsafe extern "win64" fn fwp_fail2(_a: usize, _b: usize) -> u32 {
    FWP_UNAVAILABLE
}
unsafe extern "win64" fn fwp_fail4(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    FWP_UNAVAILABLE
}
unsafe extern "win64" fn FwpmFilterEnum0(
    _engine: usize,
    _enum: usize,
    _requested: u32,
    entries: *mut usize,
    returned: *mut u32,
) -> u32 {
    if !entries.is_null() {
        *entries = 0;
    }
    if !returned.is_null() {
        *returned = 0;
    }
    FWP_UNAVAILABLE
}
unsafe extern "win64" fn FwpmFreeMemory0(p: *mut usize) {
    if !p.is_null() {
        *p = 0;
    }
}

pub fn fwpuclnt_exports() -> crate::dlls::Exports {
    crate::exports! {
        "FwpmEngineOpen0" => fn FwpmEngineOpen0,
        "FwpmEngineClose0" => fn fwp_close,
        "FwpmTransactionBegin0" => fn fwp_fail2,
        "FwpmTransactionCommit0" => fn fwp_close,
        "FwpmTransactionAbort0" => fn fwp_close,
        "FwpmSubLayerAdd0" => fn fwp_fail4,
        "FwpmSubLayerDeleteByKey0" => fn fwp_fail2,
        "FwpmFilterAdd0" => fn fwp_fail4,
        "FwpmFilterDeleteByKey0" => fn fwp_fail2,
        "FwpmFilterCreateEnumHandle0" => fn fwp_fail4,
        "FwpmFilterDestroyEnumHandle0" => fn fwp_fail2,
        "FwpmFilterEnum0" => fn FwpmFilterEnum0,
        "FwpmFreeMemory0" => fn FwpmFreeMemory0,
    }
}

// ---- netapi32.dll ---------------------------------------------------------

unsafe extern "win64" fn NetApiBufferFree(_buf: usize) -> u32 {
    0
}
unsafe extern "win64" fn net_denied4(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    ERROR_ACCESS_DENIED
}
unsafe extern "win64" fn NetUserGetInfo(
    _server: usize,
    _user: usize,
    _level: u32,
    buf: *mut usize,
) -> u32 {
    if !buf.is_null() {
        *buf = 0;
    }
    NERR_USER_NOT_FOUND
}
unsafe extern "win64" fn NetUserDel(_server: usize, _user: usize) -> u32 {
    NERR_USER_NOT_FOUND
}
unsafe extern "win64" fn NetLocalGroupDel(_server: usize, _group: usize) -> u32 {
    NERR_GROUP_NOT_FOUND
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn NetLocalGroupGetMembers(
    _server: usize,
    _group: usize,
    _level: u32,
    buf: *mut usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: usize,
) -> u32 {
    if !buf.is_null() {
        *buf = 0;
    }
    if !read.is_null() {
        *read = 0;
    }
    if !total.is_null() {
        *total = 0;
    }
    NERR_GROUP_NOT_FOUND
}
unsafe extern "win64" fn NetUserModalsGet(_server: usize, _level: u32, buf: *mut usize) -> u32 {
    if !buf.is_null() {
        *buf = 0;
    }
    ERROR_NOT_SUPPORTED
}

/// NetGetDCName : Winlux n'est jamais membre d'un domaine, il n'y a pas
/// de contrôleur de domaine (NERR_DCNotFound). Tk s'en sert pour le nom
/// de l'utilisateur (#1491).
unsafe extern "win64" fn NetGetDCName(_server: usize, _domain: usize, buf: *mut usize) -> u32 {
    if !buf.is_null() {
        *buf = 0;
    }
    2453
}

pub fn netapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "NetApiBufferFree" => fn NetApiBufferFree,
        "NetUserAdd" => fn net_denied4,
        "NetUserSetInfo" => fn net_denied4,
        "NetLocalGroupAdd" => fn net_denied4,
        "NetLocalGroupAddMembers" => fn net_denied4,
        "NetUserGetInfo" => fn NetUserGetInfo,
        "NetUserDel" => fn NetUserDel,
        "NetLocalGroupDel" => fn NetLocalGroupDel,
        "NetLocalGroupGetMembers" => fn NetLocalGroupGetMembers,
        "NetUserModalsGet" => fn NetUserModalsGet,
        "NetGetDCName" => fn NetGetDCName,
    }
}

// ---- wtsapi32.dll ---------------------------------------------------------

const WTS_CURRENT_SESSION: u32 = 0xFFFF_FFFF;
const WTS_SESSION_ID: u32 = 4;
const WTS_USER_NAME: u32 = 5;
const WTS_CONNECT_STATE: u32 = 8;
const WTS_CLIENT_PROTOCOL_TYPE: u32 = 16;

/// Poignée du serveur local rendue par WTSOpenServerA/W (#1606) ; elle
/// équivaut à WTS_CURRENT_SERVER_HANDLE pour les autres fonctions.
const WTS_LOCAL_SERVER: usize = 0x5754_5300;

/// WTSOpenServerW/A : seul le serveur local existe, quel que soit le nom
/// donné ; la poignée n'a rien à libérer.
unsafe extern "win64" fn WTSOpenServerW(_name: *const u16) -> usize {
    WTS_LOCAL_SERVER
}
unsafe extern "win64" fn WTSOpenServerA(_name: *const u8) -> usize {
    WTS_LOCAL_SERVER
}
unsafe extern "win64" fn WTSCloseServer(_server: usize) {}

/// Données d'une classe WTS_INFO_CLASS pour la session console unique de
/// Winlux (identifiant 1, état « active », protocole console, nom du
/// compte Linux) ; les chaînes en UTF-16 ou en ANSI selon `wide`.
fn session_info(class: u32, wide: bool) -> Vec<u8> {
    let text = |t: &str| -> Vec<u8> {
        if wide {
            t.encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|c| c.to_le_bytes())
                .collect()
        } else {
            t.bytes().chain(std::iter::once(0)).collect()
        }
    };
    match class {
        WTS_SESSION_ID => 1u32.to_le_bytes().to_vec(),
        WTS_CONNECT_STATE => 0u32.to_le_bytes().to_vec(), // WTSActive
        WTS_CLIENT_PROTOCOL_TYPE => 0u16.to_le_bytes().to_vec(), // console
        WTS_USER_NAME => text(&std::env::var("USER").unwrap_or_else(|_| "winlux".into())),
        // Classes textuelles inconnues : chaîne vide plutôt qu'un échec.
        _ => text(""),
    }
}

/// WTSQuerySessionInformationW/A : le tampon vient de LocalAlloc pour que
/// WTSFreeMemory puisse le rendre ; le pointeur est écrit à la taille de
/// pointeur du programme.
unsafe fn query_session(
    session: u32,
    class: u32,
    buffer: usize,
    bytes: *mut u32,
    wide: bool,
) -> i32 {
    if buffer == 0 || bytes.is_null() {
        crate::dlls::kernel32::set_last_error(87);
        return 0;
    }
    if session != WTS_CURRENT_SESSION && session != 1 {
        crate::dlls::kernel32::set_last_error(7022);
        return 0;
    }
    let data = session_info(class, wide);
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, data.len()) as *mut u8;
    if p.is_null() {
        crate::dlls::kernel32::set_last_error(8);
        return 0;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
    crate::wow32::write_ptr(buffer, p as usize);
    *bytes = data.len() as u32;
    1
}

unsafe extern "win64" fn WTSQuerySessionInformationW(
    _server: usize,
    session: u32,
    class: u32,
    buffer: usize,
    bytes: *mut u32,
) -> i32 {
    query_session(session, class, buffer, bytes, true)
}

unsafe extern "win64" fn WTSQuerySessionInformationA(
    _server: usize,
    session: u32,
    class: u32,
    buffer: usize,
    bytes: *mut u32,
) -> i32 {
    query_session(session, class, buffer, bytes, false)
}

unsafe extern "win64" fn WTSFreeMemory(p: usize) {
    if p != 0 {
        crate::dlls::kernel32_misc::local_free(p as *mut std::ffi::c_void);
    }
}

pub fn wtsapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WTSQuerySessionInformationW" => fn WTSQuerySessionInformationW,
        "WTSQuerySessionInformationA" => fn WTSQuerySessionInformationA,
        "WTSOpenServerW" => fn WTSOpenServerW,
        "WTSOpenServerA" => fn WTSOpenServerA,
        "WTSCloseServer" => fn WTSCloseServer,
        "WTSFreeMemory" => fn WTSFreeMemory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wtsapi32_serveur_local_et_session() {
        unsafe {
            let h = WTSOpenServerA(c"localhost".as_ptr().cast());
            assert_ne!(h, 0);
            let mut p = 0usize;
            let mut n = 0u32;
            assert_eq!(
                WTSQuerySessionInformationA(
                    h,
                    WTS_CURRENT_SESSION,
                    WTS_SESSION_ID,
                    (&mut p as *mut usize) as usize,
                    &mut n
                ),
                1
            );
            assert_eq!((n, *(p as *const u32)), (4, 1));
            WTSFreeMemory(p);
            assert_eq!(
                WTSQuerySessionInformationA(
                    h,
                    1,
                    WTS_USER_NAME,
                    (&mut p as *mut usize) as usize,
                    &mut n
                ),
                1
            );
            let name = std::ffi::CStr::from_ptr(p as *const std::ffi::c_char);
            assert_eq!(name.to_bytes().len() + 1, n as usize);
            WTSFreeMemory(p);
            assert_eq!(
                WTSQuerySessionInformationW(
                    h,
                    1,
                    WTS_USER_NAME,
                    (&mut p as *mut usize) as usize,
                    &mut n
                ),
                1
            );
            assert_eq!(n as usize, (name.to_bytes().len() + 1) * 2);
            WTSFreeMemory(p);
            WTSCloseServer(h);
        }
    }
}
