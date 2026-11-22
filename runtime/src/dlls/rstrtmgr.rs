//! rstrtmgr.dll : Gestionnaire de redémarrage (#2851).
//!
//! OneDriveSetup.exe l'importe statiquement (RmStartSession,
//! RmRegisterResources, RmGetList, RmEndSession) : sans elle, l'installeur
//! ne démarrait pas.
//!
//! Sessions réelles, tenues dans ce processus : une poignée et une clé par
//! session, 64 sessions au plus, ressources enregistrées comptées. Limite :
//! `RmGetList` ne cherche pas encore quels processus tiennent les fichiers
//! enregistrés ; elle rend une liste vide et RmRebootReasonNone.
//!
//! Provenance : signatures, codes de retour et limite de 64 sessions
//! d'après la documentation publique de restartmanager.h (sdk-api :
//! RmStartSession, RmRegisterResources, RmGetList, RmEndSession) ;
//! CCH_RM_SESSION_KEY (= 2 × sizeof(GUID)) d'après restartmanager.h de
//! MinGW-w64 (domaine public).

use std::collections::HashMap;
use std::sync::Mutex;

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_BAD_ARGUMENTS: u32 = 160;
const ERROR_MAX_SESSIONS_REACHED: u32 = 353;
/// Caractères de la clé de session, sans le 0 final.
const CCH_RM_SESSION_KEY: usize = 32;
/// « A maximum of 64 Restart Manager sessions per user session ».
const MAX_SESSIONS: usize = 64;
/// RmRebootReasonNone.
const REBOOT_REASON_NONE: u32 = 0;

#[derive(Default)]
struct Session {
    files: usize,
    applications: usize,
    services: usize,
}

struct Sessions {
    next: u32,
    open: HashMap<u32, Session>,
}

fn sessions() -> &'static Mutex<Sessions> {
    static S: std::sync::OnceLock<Mutex<Sessions>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Sessions {
            next: 0,
            open: HashMap::new(),
        })
    })
}

/// Clé de session : 16 octets aléatoires en hexadécimal.
fn session_key() -> [u16; CCH_RM_SESSION_KEY] {
    let mut raw = [0u8; CCH_RM_SESSION_KEY / 2];
    unsafe {
        crate::dlls::bcrypt::process_prng(raw.as_mut_ptr(), raw.len());
    }
    let mut key = [0u16; CCH_RM_SESSION_KEY];
    for (i, b) in raw.iter().enumerate() {
        let hex = format!("{b:02x}");
        for (j, c) in hex.bytes().enumerate() {
            key[2 * i + j] = c as u16;
        }
    }
    key
}

pub unsafe extern "win64" fn rm_start_session(handle: *mut u32, _flags: u32, key: *mut u16) -> u32 {
    if handle.is_null() || key.is_null() {
        return ERROR_BAD_ARGUMENTS;
    }
    let mut s = crate::dlls::lock(sessions());
    if s.open.len() >= MAX_SESSIONS {
        return ERROR_MAX_SESSIONS_REACHED;
    }
    let mut h = s.next;
    while s.open.contains_key(&h) {
        h = h.wrapping_add(1);
    }
    s.next = h.wrapping_add(1);
    s.open.insert(h, Session::default());
    drop(s);
    let k = session_key();
    std::ptr::copy_nonoverlapping(k.as_ptr(), key, k.len());
    *key.add(k.len()) = 0;
    *handle = h;
    crate::trace!("RmStartSession = {h}");
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn rm_register_resources(
    handle: u32,
    n_files: u32,
    files: usize,
    n_applications: u32,
    applications: usize,
    n_services: u32,
    services: usize,
) -> u32 {
    if (n_files != 0 && files == 0)
        || (n_applications != 0 && applications == 0)
        || (n_services != 0 && services == 0)
    {
        return ERROR_BAD_ARGUMENTS;
    }
    let mut s = crate::dlls::lock(sessions());
    let Some(session) = s.open.get_mut(&handle) else {
        return ERROR_INVALID_HANDLE;
    };
    session.files += n_files as usize;
    session.applications += n_applications as usize;
    session.services += n_services as usize;
    crate::trace!(
        "RmRegisterResources({handle}) : {n_files} fichier(s), {n_applications} processus, {n_services} service(s)"
    );
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn rm_get_list(
    handle: u32,
    needed: *mut u32,
    count: *mut u32,
    _affected: usize,
    reasons: *mut u32,
) -> u32 {
    if needed.is_null() || count.is_null() || reasons.is_null() {
        return ERROR_BAD_ARGUMENTS;
    }
    let s = crate::dlls::lock(sessions());
    let Some(session) = s.open.get(&handle) else {
        return ERROR_INVALID_HANDLE;
    };
    crate::trace!(
        "RmGetList({handle}) : {} ressource(s) enregistrée(s), aucun processus recherché",
        session.files + session.applications + session.services
    );
    *needed = 0;
    *count = 0;
    *reasons = REBOOT_REASON_NONE;
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn rm_end_session(handle: u32) -> u32 {
    if crate::dlls::lock(sessions()).open.remove(&handle).is_none() {
        return ERROR_INVALID_HANDLE;
    }
    crate::trace!("RmEndSession({handle})");
    ERROR_SUCCESS
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "RmStartSession" => fn rm_start_session,
        "RmRegisterResources" => fn rm_register_resources,
        "RmGetList" => fn rm_get_list,
        "RmEndSession" => fn rm_end_session,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// #2851 : le parcours de OneDriveSetup.exe. Session ouverte avec sa
    /// clé, fichier enregistré, liste vide sans redémarrage, session close.
    #[test]
    fn session_du_gestionnaire_de_redemarrage_2851() {
        let mut h = u32::MAX;
        let mut key = [0xFFFFu16; CCH_RM_SESSION_KEY + 1];
        assert_eq!(
            unsafe { rm_start_session(&mut h, 0, key.as_mut_ptr()) },
            ERROR_SUCCESS
        );
        assert_ne!(h, u32::MAX);
        assert_eq!(key[CCH_RM_SESSION_KEY], 0);
        assert!(key[..CCH_RM_SESSION_KEY]
            .iter()
            .all(|&c| (c as u8 as char).is_ascii_hexdigit() && c < 0x80));
        let file = wide("C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe");
        let files = [file.as_ptr() as usize];
        assert_eq!(
            unsafe { rm_register_resources(h, 1, files.as_ptr() as usize, 0, 0, 0, 0) },
            ERROR_SUCCESS
        );
        let (mut needed, mut count, mut reasons) = (7u32, 5u32, 9u32);
        assert_eq!(
            unsafe { rm_get_list(h, &mut needed, &mut count, 0, &mut reasons) },
            ERROR_SUCCESS
        );
        assert_eq!((needed, count, reasons), (0, 0, REBOOT_REASON_NONE));
        assert_eq!(unsafe { rm_end_session(h) }, ERROR_SUCCESS);
        // Session fermée : poignée inconnue.
        assert_eq!(unsafe { rm_end_session(h) }, ERROR_INVALID_HANDLE);
        assert_eq!(
            unsafe { rm_get_list(h, &mut needed, &mut count, 0, &mut reasons) },
            ERROR_INVALID_HANDLE
        );
        assert_eq!(
            unsafe { rm_register_resources(h, 0, 0, 0, 0, 0, 0) },
            ERROR_INVALID_HANDLE
        );
    }

    /// #2851 : pointeurs obligatoires nuls, tableau nul avec un compte non
    /// nul : ERROR_BAD_ARGUMENTS.
    #[test]
    fn arguments_refuses_2851() {
        let mut key = [0u16; CCH_RM_SESSION_KEY + 1];
        let mut h = 0u32;
        assert_eq!(
            unsafe { rm_start_session(std::ptr::null_mut(), 0, key.as_mut_ptr()) },
            ERROR_BAD_ARGUMENTS
        );
        assert_eq!(
            unsafe { rm_start_session(&mut h, 0, std::ptr::null_mut()) },
            ERROR_BAD_ARGUMENTS
        );
        assert_eq!(
            unsafe { rm_start_session(&mut h, 0, key.as_mut_ptr()) },
            ERROR_SUCCESS
        );
        assert_eq!(
            unsafe { rm_register_resources(h, 1, 0, 0, 0, 0, 0) },
            ERROR_BAD_ARGUMENTS
        );
        let mut n = 0u32;
        assert_eq!(
            unsafe { rm_get_list(h, std::ptr::null_mut(), &mut n, 0, &mut n) },
            ERROR_BAD_ARGUMENTS
        );
        assert_eq!(unsafe { rm_end_session(h) }, ERROR_SUCCESS);
    }
}
