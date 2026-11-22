//! Inscriptions du rapport d'erreurs Windows (WER) dans kernel32 (#2898).
//!
//! Winlux n'envoie aucun rapport WER, mais un programme inscrit puis
//! désinscrit des fichiers, blocs de mémoire, métadonnées, processus et
//! modules d'exception, et la désinscription d'un élément jamais inscrit
//! est une erreur documentée (WER_E_NOT_FOUND). Les inscriptions sont donc
//! tenues dans le processus, sans autre effet.
//!
//! OneDriveSetup.exe appelle WerUnregisterFile (#2898) ; la même famille
//! est servie ici en entier.
//!
//! Provenance : documentation publique de werapi.h (sdk-api :
//! WerRegisterFile, WerUnregisterFile, WerUnregisterCustomMetadata,
//! WerUnregisterAdditionalProcess, WerUnregisterRuntimeExceptionModule,
//! WerRegisterExcludedMemoryBlock, WerUnregisterExcludedMemoryBlock,
//! WerRegisterAppLocalDump, WerUnregisterAppLocalDump) ; valeurs de
//! WER_E_NOT_FOUND (HRESULT_FROM_WIN32(ERROR_NOT_FOUND)) et de
//! WER_MAX_REGISTERED_ENTRIES (512) d'après werapi.h de MinGW-w64 (domaine
//! public). Le nom de fichier est comparé sans tenir compte de la casse,
//! comme les chemins Windows : c'est une inférence, la page ne le dit pas.

use std::sync::Mutex;

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_NOT_FOUND).
const WER_E_NOT_FOUND: i32 = 0x8007_0490u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER).
const WER_E_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
/// Fichiers et blocs de mémoire inscrits, au plus.
const WER_MAX_REGISTERED_ENTRIES: usize = 512;
/// Longueur maximale du chemin de WerRegisterAppLocalDump.
const APP_LOCAL_DUMP_MAX: usize = 64;

#[derive(Default)]
struct Registre {
    files: Vec<String>,
    memory: Vec<usize>,
    excluded: Vec<usize>,
    metadata: Vec<String>,
    processes: Vec<u32>,
    modules: Vec<(String, usize)>,
    app_local_dump: Option<String>,
}

fn registre() -> std::sync::MutexGuard<'static, Registre> {
    static R: std::sync::OnceLock<Mutex<Registre>> = std::sync::OnceLock::new();
    crate::dlls::lock(R.get_or_init(Default::default))
}

fn remove<T: PartialEq>(list: &mut Vec<T>, item: &T) -> i32 {
    match list.iter().position(|x| x == item) {
        Some(i) => {
            list.remove(i);
            S_OK
        }
        None => WER_E_NOT_FOUND,
    }
}

unsafe fn text(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| super::wstr(p))
}

pub unsafe extern "win64" fn wer_register_file(path: *const u16, _kind: u32, _flags: u32) -> i32 {
    let Some(path) = text(path) else {
        return E_INVALIDARG;
    };
    let mut r = registre();
    if r.files.len() + r.memory.len() >= WER_MAX_REGISTERED_ENTRIES {
        return WER_E_INSUFFICIENT_BUFFER;
    }
    r.files.push(path.to_lowercase());
    S_OK
}

pub unsafe extern "win64" fn wer_unregister_file(path: *const u16) -> i32 {
    let Some(path) = text(path) else {
        return E_INVALIDARG;
    };
    remove(&mut registre().files, &path.to_lowercase())
}

pub extern "win64" fn wer_register_memory_block(addr: usize, _size: u32) -> i32 {
    let mut r = registre();
    if r.files.len() + r.memory.len() >= WER_MAX_REGISTERED_ENTRIES {
        return WER_E_INSUFFICIENT_BUFFER;
    }
    r.memory.push(addr);
    S_OK
}

/// La page de WerUnregisterMemoryBlock ne prévoit pas d'erreur « absent » :
/// S_OK dans tous les cas, comme avant.
pub extern "win64" fn wer_unregister_memory_block(addr: usize) -> i32 {
    let _ = remove(&mut registre().memory, &addr);
    S_OK
}

pub extern "win64" fn wer_register_excluded_memory_block(addr: usize, size: u32) -> i32 {
    if addr == 0 || size == 0 {
        return E_INVALIDARG;
    }
    let mut r = registre();
    if r.excluded.len() >= WER_MAX_REGISTERED_ENTRIES {
        return WER_E_INSUFFICIENT_BUFFER;
    }
    r.excluded.push(addr);
    S_OK
}

/// La page ne prévoit que WER_E_INVALID_STATE comme erreur : S_OK.
pub extern "win64" fn wer_unregister_excluded_memory_block(addr: usize) -> i32 {
    let _ = remove(&mut registre().excluded, &addr);
    S_OK
}

pub unsafe extern "win64" fn wer_register_custom_metadata(
    key: *const u16,
    value: *const u16,
) -> i32 {
    let Some(key) = text(key) else {
        return E_INVALIDARG;
    };
    if value.is_null() {
        return E_INVALIDARG;
    }
    let mut r = registre();
    if !r.metadata.contains(&key) {
        r.metadata.push(key);
    }
    S_OK
}

pub unsafe extern "win64" fn wer_unregister_custom_metadata(key: *const u16) -> i32 {
    let Some(key) = text(key) else {
        return E_INVALIDARG;
    };
    remove(&mut registre().metadata, &key)
}

pub extern "win64" fn wer_register_additional_process(pid: u32, _thread: u32) -> i32 {
    let mut r = registre();
    if !r.processes.contains(&pid) {
        r.processes.push(pid);
    }
    S_OK
}

pub extern "win64" fn wer_unregister_additional_process(pid: u32) -> i32 {
    remove(&mut registre().processes, &pid)
}

pub unsafe extern "win64" fn wer_register_runtime_exception_module(
    dll: *const u16,
    ctx: usize,
) -> i32 {
    let Some(dll) = text(dll) else {
        return E_INVALIDARG;
    };
    registre().modules.push((dll.to_lowercase(), ctx));
    S_OK
}

pub unsafe extern "win64" fn wer_unregister_runtime_exception_module(
    dll: *const u16,
    ctx: usize,
) -> i32 {
    let Some(dll) = text(dll) else {
        return E_INVALIDARG;
    };
    remove(&mut registre().modules, &(dll.to_lowercase(), ctx))
}

pub unsafe extern "win64" fn wer_register_app_local_dump(path: *const u16) -> i32 {
    match text(path) {
        Some(p) if p.encode_utf16().count() <= APP_LOCAL_DUMP_MAX => {
            registre().app_local_dump = Some(p);
            S_OK
        }
        _ => E_INVALIDARG,
    }
}

pub extern "win64" fn wer_unregister_app_local_dump() -> i32 {
    registre().app_local_dump = None;
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WerRegisterFile" => fn wer_register_file,
        "WerUnregisterFile" => fn wer_unregister_file,
        "WerRegisterMemoryBlock" => fn wer_register_memory_block,
        "WerUnregisterMemoryBlock" => fn wer_unregister_memory_block,
        "WerRegisterExcludedMemoryBlock" => fn wer_register_excluded_memory_block,
        "WerUnregisterExcludedMemoryBlock" => fn wer_unregister_excluded_memory_block,
        "WerRegisterCustomMetadata" => fn wer_register_custom_metadata,
        "WerUnregisterCustomMetadata" => fn wer_unregister_custom_metadata,
        "WerRegisterAdditionalProcess" => fn wer_register_additional_process,
        "WerUnregisterAdditionalProcess" => fn wer_unregister_additional_process,
        "WerRegisterRuntimeExceptionModule" => fn wer_register_runtime_exception_module,
        "WerUnregisterRuntimeExceptionModule" => fn wer_unregister_runtime_exception_module,
        "WerRegisterAppLocalDump" => fn wer_register_app_local_dump,
        "WerUnregisterAppLocalDump" => fn wer_unregister_app_local_dump,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// #2898 : un fichier inscrit se désinscrit une fois (casse ignorée),
    /// puis WER_E_NOT_FOUND ; chemin nul : E_INVALIDARG.
    #[test]
    fn fichier_inscrit_puis_desinscrit_2898() {
        let nom = format!(
            "C:\\Users\\a\\AppData\\Local\\Temp\\wer-{}.log",
            std::process::id()
        );
        let p = wide(&nom);
        let maj = wide(&nom.to_uppercase());
        unsafe {
            assert_eq!(wer_unregister_file(p.as_ptr()), WER_E_NOT_FOUND);
            assert_eq!(wer_register_file(p.as_ptr(), 2, 0), S_OK);
            assert_eq!(wer_unregister_file(maj.as_ptr()), S_OK);
            assert_eq!(wer_unregister_file(p.as_ptr()), WER_E_NOT_FOUND);
            assert_eq!(wer_unregister_file(std::ptr::null()), E_INVALIDARG);
        }
    }

    /// #2898 : métadonnées, processus et modules d'exception suivent la même
    /// règle ; blocs exclus et vidage local acceptés selon leurs pages.
    #[test]
    fn autres_inscriptions_wer_2898() {
        let cle = wide(&format!("cle-{}", std::process::id()));
        let val = wide("v");
        let dll = wide(&format!("gestionnaire-{}.dll", std::process::id()));
        let pid = 0x7fff_0000 | (std::process::id() & 0xffff);
        unsafe {
            assert_eq!(
                wer_unregister_custom_metadata(cle.as_ptr()),
                WER_E_NOT_FOUND
            );
            assert_eq!(
                wer_register_custom_metadata(cle.as_ptr(), val.as_ptr()),
                S_OK
            );
            assert_eq!(wer_unregister_custom_metadata(cle.as_ptr()), S_OK);

            assert_eq!(wer_unregister_additional_process(pid), WER_E_NOT_FOUND);
            assert_eq!(wer_register_additional_process(pid, 0), S_OK);
            assert_eq!(wer_unregister_additional_process(pid), S_OK);

            assert_eq!(
                wer_unregister_runtime_exception_module(dll.as_ptr(), 7),
                WER_E_NOT_FOUND
            );
            assert_eq!(wer_register_runtime_exception_module(dll.as_ptr(), 7), S_OK);
            assert_eq!(
                wer_unregister_runtime_exception_module(dll.as_ptr(), 8),
                WER_E_NOT_FOUND
            );
            assert_eq!(
                wer_unregister_runtime_exception_module(dll.as_ptr(), 7),
                S_OK
            );

            assert_eq!(wer_register_excluded_memory_block(0, 16), E_INVALIDARG);
            assert_eq!(wer_register_excluded_memory_block(0x1000, 0), E_INVALIDARG);
            let bloc = [0u8; 16];
            assert_eq!(
                wer_register_excluded_memory_block(bloc.as_ptr() as usize, 16),
                S_OK
            );
            assert_eq!(
                wer_unregister_excluded_memory_block(bloc.as_ptr() as usize),
                S_OK
            );

            let long = wide(&"x".repeat(65));
            assert_eq!(wer_register_app_local_dump(long.as_ptr()), E_INVALIDARG);
            assert_eq!(wer_register_app_local_dump(std::ptr::null()), E_INVALIDARG);
            assert_eq!(wer_register_app_local_dump(wide("dumps").as_ptr()), S_OK);
            assert_eq!(wer_unregister_app_local_dump(), S_OK);
        }
    }
}
