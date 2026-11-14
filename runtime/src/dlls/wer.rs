//! wer.dll : création de rapports d'erreurs Windows (werapi.h), importée par
//! l'installateur de OneDrive (#2391).
//!
//! Un programme prépare un rapport (WerReportCreate, paramètres P0 à P9,
//! dump, fichiers) puis le soumet. **Écart voulu avec Windows** : Winlux ne
//! crée aucun dump et ne transmet aucun rapport, ni à Microsoft ni ailleurs.
//! WerReportSubmit le dit au programme par le résultat `WerDisabled`
//! (« error reporting was disabled »), avec S_OK.
//!
//! Source : documentation publique de Microsoft (werapi.h, pages
//! WerReportCreate, WerReportSetParameter, WerReportAddDump,
//! WerReportSubmit, WerReportCloseHandle) ; valeurs des énumérations :
//! werapi.h de mingw-w64 (domaine public). La documentation ne nomme pas
//! l'erreur rendue pour un argument hors domaine : E_INVALIDARG est notre
//! choix ; E_HANDLE et WER_E_LENGTH_EXCEEDED sont documentés.

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const E_HANDLE: i32 = 0x8007_0006u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_PARAMETER_QUOTA_EXCEEDED).
const WER_E_LENGTH_EXCEEDED: i32 = 0x8007_0503u32 as i32;

/// WER_MAX_EVENT_NAME_LENGTH et WER_MAX_PARAM_LENGTH (MAX_PATH).
const MAX_EVENT_NAME: usize = 64;
const MAX_PARAM: usize = 260;
/// WerReportInvalid : premier type de rapport hors domaine.
const REPORT_TYPE_INVALID: u32 = 5;
/// WerDumpTypeMicroDump … WerDumpTypeTriageDump.
const DUMP_TYPES: std::ops::RangeInclusive<u32> = 1..=4;
/// WerConsentNotAsked … WerConsentAlwaysPrompt.
const CONSENTS: std::ops::RangeInclusive<u32> = 1..=4;
/// WER_SUBMIT_RESULT : WerDisabled.
const WER_DISABLED: u32 = 5;

#[derive(Default)]
struct Report {
    params: [Option<String>; 10],
}

fn reports() -> &'static Mutex<HashMap<usize, Report>> {
    static R: OnceLock<Mutex<HashMap<usize, Report>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Longueur d'une chaîne large terminée par zéro, bornée à `max + 1`.
unsafe fn wlen(p: *const u16, max: usize) -> usize {
    let mut n = 0;
    while n <= max && *p.add(n) != 0 {
        n += 1;
    }
    n
}

fn known(h: usize) -> bool {
    lock(reports()).contains_key(&h)
}

/// `WerReportCreate(type d'événement, type, informations, &handle)`.
unsafe extern "win64" fn WerReportCreate(
    event: *const u16,
    rep_type: u32,
    _info: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() || event.is_null() || rep_type >= REPORT_TYPE_INVALID {
        return E_INVALIDARG;
    }
    let n = wlen(event, MAX_EVENT_NAME);
    if n == 0 {
        return E_INVALIDARG;
    }
    if n > MAX_EVENT_NAME {
        return WER_E_LENGTH_EXCEEDED;
    }
    // Handle opaque, jamais une adresse : impair et hors des petits entiers.
    static NEXT: AtomicUsize = AtomicUsize::new(0x5745_0001);
    let h = NEXT.fetch_add(2, Ordering::Relaxed);
    lock(reports()).insert(h, Report::default());
    *out = h;
    S_OK
}

/// `WerReportSetParameter(handle, WER_P0…WER_P9, nom facultatif, valeur)`.
unsafe extern "win64" fn WerReportSetParameter(
    h: usize,
    id: u32,
    name: *const u16,
    value: *const u16,
) -> i32 {
    let mut all = lock(reports());
    let Some(report) = all.get_mut(&h) else {
        return E_HANDLE;
    };
    if id > 9 || value.is_null() {
        return E_INVALIDARG;
    }
    if (!name.is_null() && wlen(name, MAX_PARAM) > MAX_PARAM) || wlen(value, MAX_PARAM) > MAX_PARAM
    {
        return WER_E_LENGTH_EXCEEDED;
    }
    report.params[id as usize] = Some(crate::dlls::wstr(value));
    S_OK
}

/// `WerReportAddDump(handle, processus, fil, type, exception, options,
/// drapeaux)` : accepté, aucun dump n'est écrit.
unsafe extern "win64" fn WerReportAddDump(
    h: usize,
    _process: usize,
    _thread: usize,
    dump_type: u32,
    _exception: usize,
    _options: usize,
    _flags: u32,
) -> i32 {
    if !known(h) {
        return E_HANDLE;
    }
    if !DUMP_TYPES.contains(&dump_type) {
        return E_INVALIDARG;
    }
    S_OK
}

/// `WerReportAddFile(handle, chemin, type, drapeaux)` : accepté, le fichier
/// n'est ni lu ni copié.
unsafe extern "win64" fn WerReportAddFile(
    h: usize,
    path: *const u16,
    kind: u32,
    _flags: u32,
) -> i32 {
    if !known(h) {
        return E_HANDLE;
    }
    if path.is_null() || kind == 0 {
        return E_INVALIDARG;
    }
    S_OK
}

/// `WerReportSetUIOption(handle, champ, valeur)` : texte de l'interface du
/// rapport, qui ne s'affiche jamais ici.
unsafe extern "win64" fn WerReportSetUIOption(h: usize, _field: u32, value: *const u16) -> i32 {
    if !known(h) {
        return E_HANDLE;
    }
    if value.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

/// `WerReportSubmit(handle, consentement, drapeaux, &résultat)` : rien n'est
/// transmis ; résultat `WerDisabled`.
unsafe extern "win64" fn WerReportSubmit(
    h: usize,
    consent: u32,
    _flags: u32,
    result: *mut u32,
) -> i32 {
    if !known(h) {
        return E_HANDLE;
    }
    if !CONSENTS.contains(&consent) {
        return E_INVALIDARG;
    }
    crate::log_debug!("wer", "rapport d'erreurs non transmis (WerReportSubmit)");
    if !result.is_null() {
        *result = WER_DISABLED;
    }
    S_OK
}

/// `WerReportCloseHandle(handle)`.
extern "win64" fn WerReportCloseHandle(h: usize) -> i32 {
    if lock(reports()).remove(&h).is_some() {
        S_OK
    } else {
        E_HANDLE
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WerReportCreate" => fn WerReportCreate,
        "WerReportSetParameter" => fn WerReportSetParameter,
        "WerReportAddDump" => fn WerReportAddDump,
        "WerReportAddFile" => fn WerReportAddFile,
        "WerReportSetUIOption" => fn WerReportSetUIOption,
        "WerReportSubmit" => fn WerReportSubmit,
        "WerReportCloseHandle" => fn WerReportCloseHandle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// #2391 : le parcours de l'installateur de OneDrive (créer, paramètres,
    /// dump, soumettre, fermer) réussit et ne transmet rien (WerDisabled).
    #[test]
    fn rapport_cree_soumis_sans_envoi() {
        unsafe {
            let event = w("OneDriveSetup");
            let mut h = 0usize;
            assert_eq!(WerReportCreate(event.as_ptr(), 1, 0, &mut h), S_OK);
            assert_ne!(h, 0);
            let v = w("26.198");
            assert_eq!(
                WerReportSetParameter(h, 0, std::ptr::null(), v.as_ptr()),
                S_OK
            );
            assert_eq!(
                WerReportSetParameter(h, 10, std::ptr::null(), v.as_ptr()),
                E_INVALIDARG
            );
            assert_eq!(
                WerReportSetParameter(h, 1, std::ptr::null(), std::ptr::null()),
                E_INVALIDARG
            );
            let long = w(&"x".repeat(MAX_PARAM + 1));
            assert_eq!(
                WerReportSetParameter(h, 1, std::ptr::null(), long.as_ptr()),
                WER_E_LENGTH_EXCEEDED
            );
            assert_eq!(WerReportAddDump(h, usize::MAX, 0, 2, 0, 0, 0), S_OK);
            assert_eq!(WerReportAddDump(h, usize::MAX, 0, 9, 0, 0, 0), E_INVALIDARG);
            let mut result = 0u32;
            assert_eq!(WerReportSubmit(h, 1, 0, &mut result), S_OK);
            assert_eq!(result, WER_DISABLED);
            assert_eq!(WerReportSubmit(h, 0, 0, &mut result), E_INVALIDARG);
            assert_eq!(WerReportCloseHandle(h), S_OK);
            assert_eq!(WerReportCloseHandle(h), E_HANDLE);
            assert_eq!(WerReportSubmit(h, 1, 0, &mut result), E_HANDLE);
        }
    }

    #[test]
    fn creation_refusee_hors_domaine() {
        unsafe {
            let event = w("APPCRASH");
            let mut h = 0usize;
            assert_eq!(
                WerReportCreate(std::ptr::null(), 0, 0, &mut h),
                E_INVALIDARG
            );
            assert_eq!(WerReportCreate(event.as_ptr(), 5, 0, &mut h), E_INVALIDARG);
            assert_eq!(
                WerReportCreate(event.as_ptr(), 0, 0, std::ptr::null_mut()),
                E_INVALIDARG
            );
            let long = w(&"e".repeat(MAX_EVENT_NAME + 1));
            assert_eq!(
                WerReportCreate(long.as_ptr(), 0, 0, &mut h),
                WER_E_LENGTH_EXCEEDED
            );
        }
    }
}
