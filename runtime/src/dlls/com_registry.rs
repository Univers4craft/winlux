//! Activation COM depuis le registre (#16).
//!
//! Quand aucune fabrique n'est inscrite par `CoRegisterClassObject` et
//! qu'aucune classe intégrée ne répond, `CoGetClassObject` et
//! `CoCreateInstance` lisent `HKEY_CLASSES_ROOT\CLSID\{…}\InprocServer32`,
//! chargent la DLL indiquée et appellent son `DllGetClassObject`, comme le
//! décrit la documentation publique de COM (« InprocServer32 », « COM
//! Server Responsibilities »). Les DLL chargées restent en mémoire jusqu'à
//! ce que `CoFreeUnusedLibraries` les trouve libérables (`DllCanUnloadNow`
//! rend S_OK).

use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::Mutex;

const S_OK: i32 = 0;
const CO_E_DLLNOTFOUND: i32 = 0x8004_01f8u32 as i32;
const CO_E_ERRORINDLL: i32 = 0x8004_01f9u32 as i32;
const CLASS_E_CLASSNOTAVAILABLE: i32 = 0x8004_0111u32 as i32;

/// CLSCTX_INPROC_SERVER.
pub const CLSCTX_INPROC_SERVER: u32 = 0x1;

type DllGetClassObject =
    unsafe extern "win64" fn(*const Guid, *const Guid, *mut *mut c_void) -> i32;
type DllCanUnloadNow = unsafe extern "win64" fn() -> i32;

/// Serveurs en processus chargés par l'activation : (chemin, module).
static LOADED: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());

/// Texte `{XXXXXXXX-XXXX-…}` d'un GUID, en minuscules pour la clé.
fn key_of(guid: &Guid) -> String {
    format!(
        "hkcr\\clsid\\{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

/// Chemin du serveur en processus inscrit pour cette classe, variables
/// d'environnement développées et guillemets retirés.
pub fn inproc_server(guid: &Guid) -> Option<String> {
    let raw = crate::registry::get_string(&format!("{}\\InprocServer32", key_of(guid)), "")?;
    let path = super::kernel32_fs::expand_environment(raw.trim());
    let path = path.trim().trim_matches('"').to_string();
    (!path.is_empty()).then_some(path)
}

/// Charge (une seule fois) la DLL et rend son module.
unsafe fn load(path: &str) -> Option<usize> {
    let mut loaded = LOADED.lock().unwrap();
    if let Some((_, module)) = loaded.iter().find(|(p, _)| p.eq_ignore_ascii_case(path)) {
        return Some(*module);
    }
    // LoadLibraryW peut réentrer dans COM (DllMain) : pas de verrou tenu.
    drop(loaded);
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let module = super::kernel32::load_library_w(wide.as_ptr());
    if module == 0 {
        return None;
    }
    loaded = LOADED.lock().unwrap();
    if !loaded.iter().any(|(p, _)| p.eq_ignore_ascii_case(path)) {
        loaded.push((path.to_string(), module));
    } else {
        // Un autre thread l'a chargée entre-temps : rendre la référence en trop.
        super::kernel32::free_library(module);
    }
    Some(module)
}

/// Fabrique de la classe obtenue du serveur en processus inscrit.
/// `None` : rien d'inscrit, l'appelant garde son propre code d'erreur.
pub unsafe fn class_object(guid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    // Le code x86 d'une DLL 32 bits ne s'appelle pas d'ici.
    if crate::wow32::active() {
        return None;
    }
    let path = inproc_server(guid)?;
    let Some(module) = load(&path) else {
        crate::log_warn!(
            "com",
            "InprocServer32 {path} introuvable (CO_E_DLLNOTFOUND)"
        );
        return Some(CO_E_DLLNOTFOUND);
    };
    let entry = super::kernel32::get_proc_address(module, c"DllGetClassObject".as_ptr().cast());
    if entry == 0 {
        crate::log_warn!(
            "com",
            "{path} n'exporte pas DllGetClassObject (CO_E_ERRORINDLL)"
        );
        return Some(CO_E_ERRORINDLL);
    }
    let get: DllGetClassObject = std::mem::transmute(entry);
    let hr = get(guid, iid, out);
    if hr < 0 && hr != CLASS_E_CLASSNOTAVAILABLE {
        crate::trace!("DllGetClassObject de {path} : 0x{:08x}", hr as u32);
    }
    Some(hr)
}

/// CoFreeUnusedLibraries : décharge les serveurs dont `DllCanUnloadNow`
/// rend S_OK. Rend le nombre de DLL libérées.
pub unsafe fn free_unused() -> usize {
    let candidates: Vec<(String, usize)> = LOADED.lock().unwrap().clone();
    let mut freed = 0;
    for (path, module) in candidates {
        let entry = super::kernel32::get_proc_address(module, c"DllCanUnloadNow".as_ptr().cast());
        if entry == 0 {
            continue;
        }
        let can: DllCanUnloadNow = std::mem::transmute(entry);
        if can() != S_OK {
            continue;
        }
        LOADED.lock().unwrap().retain(|(p, _)| p != &path);
        super::kernel32::free_library(module);
        freed += 1;
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cle_du_clsid_en_minuscules_avec_accolades() {
        let g = Guid {
            data1: 0x0002_DF01,
            data2: 0,
            data3: 0,
            data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
        };
        assert_eq!(
            key_of(&g),
            "hkcr\\clsid\\{0002df01-0000-0000-c000-000000000046}"
        );
    }

    #[test]
    fn classe_absente_du_registre_laisse_la_main() {
        let _guard = crate::dlls::advapi32::REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("winlux-com-reg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let old = std::env::var_os("WINLUX_REGISTRY_PATH");
        std::env::set_var("WINLUX_REGISTRY_PATH", dir.join("registry.tsv"));
        let g = Guid {
            data1: 0x776c_c0de,
            ..Guid::default()
        };
        let mut out = std::ptr::null_mut();
        assert!(unsafe { class_object(&g, &g, &mut out) }.is_none());
        assert!(inproc_server(&g).is_none());
        match old {
            Some(v) => std::env::set_var("WINLUX_REGISTRY_PATH", v),
            None => std::env::remove_var("WINLUX_REGISTRY_PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
