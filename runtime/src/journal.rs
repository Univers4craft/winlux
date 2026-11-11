//! Dossier du journal Winlux, propre à chaque compte (#633).
//!
//! `winlux.log`, `crash.log`, `crash-offer`, `crash-bsod` et
//! `report-offered` vivaient tous dans `/run/winlux/journal` (1777, fichiers
//! 0666) ou à la racine de `/run/winlux` : n'importe quel compte pouvait
//! effacer ou falsifier les plantages d'un autre, que l'Observateur
//! d'événements affiche et que `winlux-report` envoie sur GitHub, et le
//! second compte ne pouvait pas réécrire `crash-bsod` du premier.
//!
//! Désormais chaque compte écrit dans `/run/winlux/journal-<uid>` (0700, à
//! lui) et root dans `/run/winlux/journal` (0755, à root). Un dossier qui
//! n'appartient pas au compte (créé à l'avance par un autre, lien
//! symbolique) n'est jamais utilisé : pas de journal plutôt qu'un journal
//! falsifiable. Le chemin retenu est exporté dans `WINLUX_JOURNAL_DIR` pour
//! les .exe (Observateur, écran bleu du bureau) et les scripts lancés.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Journal des démons root.
pub const SYSTEM_DIR: &str = "/run/winlux/journal";

static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Chemins lus par le gestionnaire de signal, préparés d'avance : il ne
/// doit ni allouer ni prendre de verrou.
pub struct SignalPaths {
    pub crash_log: CString,
    pub crash_bsod: CString,
}

static SIGNAL_PATHS: OnceLock<Option<SignalPaths>> = OnceLock::new();

/// Dossier par défaut du compte `euid`, sans rien créer.
pub fn default_dir(euid: u32) -> PathBuf {
    if euid == 0 {
        PathBuf::from(SYSTEM_DIR)
    } else {
        PathBuf::from(format!("/run/winlux/journal-{euid}"))
    }
}

/// Dossier du journal de ce processus, créé au besoin ; `None` si aucun
/// dossier sûr n'est disponible.
pub fn dir() -> Option<&'static Path> {
    DIR.get_or_init(|| {
        let euid = unsafe { libc::geteuid() };
        // WINLUX_JOURNAL_DIR (tests, ou hérité du processus parent) n'est
        // suivi que s'il appartient bien à ce compte : un compte lancé
        // depuis root (ou l'inverse) retombe sur son propre dossier.
        if let Some(wanted) = std::env::var_os("WINLUX_JOURNAL_DIR").filter(|v| !v.is_empty()) {
            let wanted = PathBuf::from(wanted);
            if ensure(&wanted, euid) {
                return Some(wanted);
            }
        }
        let own = default_dir(euid);
        ensure(&own, euid).then_some(own)
    })
    .as_deref()
}

/// Au tout début de `main`, quand le processus n'a encore qu'un fil :
/// fixe le dossier et l'exporte pour les programmes lancés ensuite.
pub fn init() {
    if let Some(dir) = dir() {
        std::env::set_var("WINLUX_JOURNAL_DIR", dir);
    }
}

/// Prépare les chemins du gestionnaire de signal (crash::install).
pub fn prepare_signal_paths() {
    SIGNAL_PATHS.get_or_init(|| {
        let dir = dir()?;
        let c = |name: &str| CString::new(dir.join(name).as_os_str().as_bytes()).ok();
        Some(SignalPaths {
            crash_log: c("crash.log")?,
            crash_bsod: c("crash-bsod")?,
        })
    });
}

/// Chemins du gestionnaire de signal, s'ils ont été préparés.
pub fn signal_paths() -> Option<&'static SignalPaths> {
    SIGNAL_PATHS.get().and_then(|p| p.as_ref())
}

/// Chemin d'un fichier du journal (hors gestionnaire de signal).
pub fn file(name: &str) -> Option<PathBuf> {
    dir().map(|d| d.join(name))
}

/// Crée `dir` pour le compte `euid` (0755 pour root, 0700 sinon) et vérifie
/// qu'il lui appartient : un dossier posé par un autre compte, ou un lien
/// symbolique, est refusé.
pub(crate) fn ensure(dir: &Path, euid: u32) -> bool {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let mode = if euid == 0 { 0o755 } else { 0o700 };
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::DirBuilder::new().mode(mode).create(dir);
    // symlink_metadata : un lien symbolique n'est pas un dossier.
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() && meta.uid() == euid => {
            // Ancien journal 1777 (avant #633) : on resserre.
            if meta.mode() & 0o022 != 0 {
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
            }
            true
        }
        _ => false,
    }
}

/// Utilisé par les tests : un dossier qui n'est pas au compte est refusé.
#[cfg(test)]
pub fn is_safe_for(dir: &Path, euid: u32) -> bool {
    ensure(dir, euid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossier_par_compte() {
        assert_eq!(default_dir(0), PathBuf::from("/run/winlux/journal"));
        assert_eq!(default_dir(1000), PathBuf::from("/run/winlux/journal-1000"));
    }

    #[test]
    fn refuse_un_dossier_d_un_autre_compte_ou_un_lien() {
        let euid = unsafe { libc::geteuid() };
        let base = std::env::temp_dir().join(format!("winlux-journal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let mine = base.join("mien");
        assert!(is_safe_for(&mine, euid));
        assert!(!is_safe_for(&mine, euid.wrapping_add(1)));
        let link = base.join("lien");
        std::os::unix::fs::symlink(&mine, &link).unwrap();
        assert!(!is_safe_for(&link, euid));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&mine).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode & 0o022,
            0,
            "journal écrivable par les autres : {mode:o}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
