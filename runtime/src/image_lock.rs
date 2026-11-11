//! Images en cours d'exécution protégées en écriture, comme sous Windows
//! (#1917).
//!
//! Sous Windows, un .exe ou une DLL projeté comme image par un processus
//! vivant ne peut pas être ouvert en écriture ni tronqué : `CreateFile` rend
//! ERROR_SHARING_VIOLATION (le renommage reste permis). Les programmes qui
//! se mettent à jour eux-mêmes en tiennent compte (copie temporaire, puis
//! remplacement une fois arrêtés). Sans ce verrou, SteamService.exe
//! réécrivait le fichier dont il exécutait le code, et la copie restait
//! tronquée.
//!
//! Chaque processus Winlux garde, pour chaque image chargée depuis le
//! disque, un descripteur verrouillé en partage (`flock(LOCK_SH)`). Une
//! ouverture en écriture d'un .exe ou d'une .dll essaie un verrou exclusif
//! non bloquant : s'il est refusé, l'image tourne quelque part. Le noyau
//! lève le verrou à la mort du processus, et les descripteurs sont fermés à
//! l'exec (O_CLOEXEC), donc un enfant n'en hérite pas.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Mutex;

static HELD: Mutex<Vec<std::fs::File>> = Mutex::new(Vec::new());

/// Extensions d'image concernées : le contrôle ne touche jamais un
/// document, un journal ou une base de données.
fn is_image_name(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "exe" | "dll" | "sys" | "ocx" | "cpl" | "drv" | "scr"
        )
    })
}

/// Note que `path` est chargé comme image par ce processus, pour toute sa
/// durée de vie.
pub fn hold(path: &Path) {
    if !is_image_name(path) {
        return;
    }
    let Ok(f) = std::fs::File::open(path) else {
        return;
    };
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
        return;
    }
    if let Ok(mut held) = HELD.lock() {
        held.push(f);
    }
}

/// `path` est-il chargé comme image par un processus vivant ?
pub fn in_use(path: &Path) -> bool {
    if !is_image_name(path) {
        return false;
    }
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    let fd = f.as_raw_fd();
    if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        return false;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK)
}

/// Refus d'écrire sur une image en cours d'exécution : `Some(32)`
/// (ERROR_SHARING_VIOLATION) avec une ligne de trace, sinon `None`.
pub fn refuse_write(path: &Path, api: &str) -> Option<u32> {
    if !in_use(path) {
        return None;
    }
    crate::trace!(
        "{api}({}) : image en cours d'exécution, écriture refusée (ERROR_SHARING_VIOLATION)",
        path.display()
    );
    Some(32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_chargee_refuse_l_ecriture() {
        let dir = std::env::temp_dir().join(format!("winlux-image-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Service.exe");
        let doc = dir.join("notes.txt");
        std::fs::write(&exe, b"MZ").unwrap();
        std::fs::write(&doc, b"texte").unwrap();
        assert!(!in_use(&exe), "pas encore chargée");
        hold(&exe);
        hold(&doc);
        assert!(in_use(&exe), "chargée : écriture refusée");
        assert_eq!(refuse_write(&exe, "CreateFileW"), Some(32));
        assert!(!in_use(&doc), "un document n'est jamais verrouillé");
        // Le renommage reste permis, comme sous Windows.
        let moved = dir.join("Service.old.exe");
        std::fs::rename(&exe, &moved).unwrap();
        assert!(!in_use(&exe));
        HELD.lock().unwrap().clear();
        assert!(!in_use(&moved), "verrou levé avec le descripteur");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
