//! Personnalité WOW64 d'un programme x86 (#232, étape 7) : ce que voit un
//! programme 32 bits sous un Windows 64 bits.
//!
//! - `IsWow64Process` vrai, `IsWow64Process2` rend IMAGE_FILE_MACHINE_I386 ;
//! - redirection de fichiers `C:\Windows\System32` → `SysWOW64` (voir
//!   `paths::wow64_redirect`), suspendue par `Wow64DisableWow64FsRedirection` ;
//! - vue 32 bits du registre (`HKLM\Software\Wow6432Node`, voir
//!   `registry_view`) ;
//! - variables d'environnement d'un processus 32 bits (`PROCESSOR_ARCHITECTURE`
//!   = x86, `ProgramFiles` = Program Files (x86)…) ;
//! - structures de taille « pointeur » (SRWLOCK, INIT_ONCE…) lues et écrites
//!   sur 4 octets.

use std::cell::Cell;

thread_local! {
    /// Redirection de fichiers du thread (Wow64DisableWow64FsRedirection).
    static FS_REDIRECT: Cell<bool> = const { Cell::new(true) };
}

/// Taille d'un pointeur pour le programme : 4 octets en x86, 8 sinon.
pub fn pointer_size() -> usize {
    if super::active() {
        4
    } else {
        8
    }
}

/// Lit une case de la taille d'un pointeur du programme.
///
/// # Safety
/// `at` doit être lisible sur `pointer_size()` octets.
pub unsafe fn read_ptr(at: usize) -> usize {
    if super::active() {
        std::ptr::read_volatile(at as *const u32) as usize
    } else {
        std::ptr::read_volatile(at as *const usize)
    }
}

/// Écrit une case de la taille d'un pointeur du programme.
///
/// # Safety
/// `at` doit être inscriptible sur `pointer_size()` octets.
pub unsafe fn write_ptr(at: usize, v: usize) {
    if super::active() {
        std::ptr::write_volatile(at as *mut u32, v as u32)
    } else {
        std::ptr::write_volatile(at as *mut usize, v)
    }
}

/// La redirection System32 → SysWOW64 s'applique-t-elle à ce thread ?
pub fn fs_redirection() -> bool {
    super::active() && FS_REDIRECT.with(Cell::get)
}

/// Active ou suspend la redirection du thread ; rend l'état précédent.
pub fn set_fs_redirection(on: bool) -> bool {
    FS_REDIRECT.with(|c| c.replace(on))
}

/// Variables d'environnement propres à un processus 32 bits (ou rétablies
/// pour un processus 64 bits lancé par un programme 32 bits), comme le fait
/// Windows à la création de chaque processus.
pub fn apply_env(x86: bool) {
    let get = |k: &str| std::env::var_os(k);
    if x86 {
        if get("PROCESSOR_ARCHITEW6432").is_none() {
            std::env::set_var("PROCESSOR_ARCHITEW6432", "AMD64");
        }
        std::env::set_var("PROCESSOR_ARCHITECTURE", "x86");
        if get("ProgramW6432").is_none() {
            std::env::set_var("ProgramW6432", "C:\\Program Files");
        }
        if get("CommonProgramW6432").is_none() {
            std::env::set_var("CommonProgramW6432", "C:\\Program Files\\Common Files");
        }
        let pf86 = get("ProgramFiles(x86)").unwrap_or_else(|| "C:\\Program Files (x86)".into());
        std::env::set_var("ProgramFiles", pf86);
        let cpf86 = get("CommonProgramFiles(x86)")
            .unwrap_or_else(|| "C:\\Program Files (x86)\\Common Files".into());
        std::env::set_var("CommonProgramFiles", cpf86);
    } else if let Some(arch) = get("PROCESSOR_ARCHITEW6432") {
        // Processus 64 bits lancé par un programme 32 bits.
        std::env::set_var("PROCESSOR_ARCHITECTURE", arch);
        std::env::remove_var("PROCESSOR_ARCHITEW6432");
        if let Some(pf) = get("ProgramW6432") {
            std::env::set_var("ProgramFiles", pf);
        }
        if let Some(cpf) = get("CommonProgramW6432") {
            std::env::set_var("CommonProgramFiles", cpf);
        }
    }
    if !x86 && get("ProgramW6432").is_none() {
        std::env::set_var("ProgramW6432", "C:\\Program Files");
    }
    if !x86 && get("CommonProgramW6432").is_none() {
        std::env::set_var("CommonProgramW6432", "C:\\Program Files\\Common Files");
    }
}

const KEY_WOW64_64KEY: u32 = 0x0100;
const KEY_WOW64_32KEY: u32 = 0x0200;

/// Clés de `HKLM\Software\Classes` que Windows 7 et suivants redirigent
/// encore ; le reste de Classes est partagé entre les deux vues.
const CLASSES_REDIRECTED: [&str; 5] = [
    "clsid",
    "directshow",
    "interface",
    "media type",
    "mediafoundation",
];

/// Chemin de la vue 32 bits d'une clé `hklm\Software\…`, ou `None` si la clé
/// est partagée entre les vues.
fn wow_path(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let rest = lower.strip_prefix("hklm\\software\\")?;
    let first = rest.split('\\').next().unwrap_or("");
    if first == "wow6432node" {
        return None;
    }
    if first == "classes" {
        let second = rest.split('\\').nth(1).unwrap_or("");
        if !CLASSES_REDIRECTED.contains(&second) || second.is_empty() {
            return None;
        }
        let keep = "hklm\\software\\classes\\".len();
        return Some(format!("{}Wow6432Node\\{}", &path[..keep], &path[keep..]));
    }
    let keep = "hklm\\software\\".len();
    Some(format!("{}Wow6432Node\\{}", &path[..keep], &path[keep..]))
}

/// Vue du registre pour `path` selon le programme et `access`
/// (KEY_WOW64_32KEY / KEY_WOW64_64KEY). `exists` dit si une clé existe.
///
/// Comme Windows, un programme x86 lit et crée ses clés sous Wow6432Node.
/// Écart volontaire : si la clé n'existe que dans la vue 64 bits (clés
/// posées par Winlux lui-même, qui n'ont pas de double 32 bits), c'est
/// elle qui est rendue plutôt que « introuvable ».
pub fn registry_view(path: &str, access: u32, exists: &dyn Fn(&str) -> bool) -> String {
    let want32 = if access & KEY_WOW64_64KEY != 0 {
        false
    } else {
        access & KEY_WOW64_32KEY != 0 || super::active()
    };
    if !want32 {
        return path.to_string();
    }
    let Some(wow) = wow_path(path) else {
        return path.to_string();
    };
    if exists(&wow) || !exists(path) {
        wow
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_wow6432node() {
        assert_eq!(
            wow_path("hklm\\SOFTWARE\\Martin Prikryl\\WinSCP 2").as_deref(),
            Some("hklm\\SOFTWARE\\Wow6432Node\\Martin Prikryl\\WinSCP 2")
        );
        assert_eq!(wow_path("hklm\\Software\\Wow6432Node\\X"), None);
        assert_eq!(wow_path("hklm\\Software\\Classes\\.txt"), None);
        assert_eq!(
            wow_path("hklm\\Software\\Classes\\CLSID\\{1}").as_deref(),
            Some("hklm\\Software\\Classes\\Wow6432Node\\CLSID\\{1}")
        );
        assert_eq!(wow_path("hkcu\\Software\\X"), None);
        assert_eq!(wow_path("hklm\\System\\X"), None);
    }

    #[test]
    fn vue_32_bits_demandee() {
        let only64 = |p: &str| p == "hklm\\Software\\Winlux";
        assert_eq!(
            registry_view("hklm\\Software\\Winlux", KEY_WOW64_32KEY, &only64),
            "hklm\\Software\\Winlux"
        );
        assert_eq!(
            registry_view("hklm\\Software\\Neuve", KEY_WOW64_32KEY, &only64),
            "hklm\\Software\\Wow6432Node\\Neuve"
        );
        assert_eq!(
            registry_view("hklm\\Software\\Neuve", 0, &only64),
            "hklm\\Software\\Neuve"
        );
    }
}
