//! Fichiers « effacés à la fermeture » (FILE_FLAG_DELETE_ON_CLOSE,
//! FILE_DELETE_ON_CLOSE) : sous Windows, la fin du processus ferme tous ses
//! handles, donc efface ces fichiers, qu'il sorte par ExitProcess, plante ou
//! soit tué. Chez nous l'effacement n'était fait que dans CloseHandle : le
//! `lockfile` de Chromium survivait et empêchait le prochain démarrage
//! (« Lock file exists but is writable »), #2040.
//!
//! - fin propre ou plantage : `on_exit` efface ce qui reste ouvert ;
//! - SIGKILL : rien ne tourne dans le processus. Chaque fichier est donc
//!   noté dans un registre par utilisateur (`$XDG_RUNTIME_DIR/winlux-doomed`,
//!   0700), une entrée par handle : PID, date de démarrage du processus,
//!   identité (st_dev, st_ino) et chemin. Au démarrage, `sweep` efface les
//!   fichiers des processus morts, si le nom désigne toujours le même
//!   fichier et qu'aucun processus vivant ne l'a ouvert.
//!
//! Sûreté : seuls les chemins notés par Winlux dans ce registre sont
//! examinés (jamais un nom deviné), et un fichier n'est effacé que s'il
//! appartient au compte courant et a toujours la même identité.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn ledger_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/winlux-{}", unsafe { libc::getuid() })));
    let d = base.join("winlux-doomed");
    std::fs::create_dir_all(&d).ok()?;
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    Some(d)
}

/// Champ 22 de `/proc/PID/stat` : distingue un processus d'un autre qui
/// reprendrait son numéro.
fn start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn my_tag() -> Option<String> {
    static TAG: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    TAG.get_or_init(|| {
        let pid = std::process::id();
        Some(format!("{pid}-{}", start_time(pid)?))
    })
    .clone()
}

fn record_path(h: usize) -> Option<PathBuf> {
    Some(ledger_dir()?.join(format!("{}-{h:x}", my_tag()?)))
}

/// Handle `h` ouvert avec effacement à la fermeture sur `path`.
pub(crate) fn register(h: usize, path: PathBuf) {
    if let (Ok(m), Some(rec)) = (std::fs::symlink_metadata(&path), record_path(h)) {
        let _ = std::fs::write(
            rec,
            format!("{} {}\n{}", m.dev(), m.ino(), path.to_string_lossy()),
        );
    }
    crate::dlls::lock(super::kernel32::delete_on_close()).insert(h, path);
    let fd = super::kernel32::handle_to_fd(h);
    crate::dlls::lock(&FDS).push((h, fd));
}

/// Descripteur de chaque handle à effacer : la fin du processus retrouve
/// ainsi le nom actuel du fichier (il a pu être renommé, #1885) sans
/// prendre le verrou de la table des handles.
static FDS: std::sync::Mutex<Vec<(usize, i32)>> = std::sync::Mutex::new(Vec::new());

/// Handle fermé : son entrée du registre n'a plus lieu d'être.
pub(crate) fn forget(h: usize) {
    crate::dlls::lock(&FDS).retain(|&(k, _)| k != h);
    forget_record(h);
}

fn forget_record(h: usize) {
    if let Some(rec) = record_path(h) {
        let _ = std::fs::remove_file(rec);
    }
}

/// Efface `path` s'il désigne toujours le fichier (dev, ino) et qu'il
/// appartient au compte courant : jamais le fichier d'un autre compte, ni
/// un fichier qui aurait pris le nom entre-temps.
fn remove_if_same(path: &Path, dev: u64, ino: u64) -> bool {
    let uid = unsafe { libc::getuid() };
    let same = std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.dev() == dev && m.ino() == ino && m.uid() == uid);
    same && if path.is_dir() {
        std::fs::remove_dir(path).is_ok()
    } else {
        std::fs::remove_file(path).is_ok()
    }
}

/// Fin du processus (ExitProcess, TerminateProcess, plantage) : efface les
/// fichiers dont le handle est encore ouvert. `try_lock` : le gestionnaire
/// de plantage peut couper un thread qui tenait le verrou.
pub(crate) fn on_exit() {
    let Ok(mut fds) = FDS.try_lock() else {
        return;
    };
    for (h, fd) in fds.drain(..) {
        if let Some((path, dev, ino)) = super::kernel32::doomed_on_close(fd) {
            let ok = remove_if_same(&path, dev, ino);
            crate::trace!(
                "fin du processus : effacement à la fermeture de {} = {}",
                path.display(),
                if ok { "ok" } else { "échec" }
            );
        }
        forget_record(h);
    }
}

/// Un processus vivant a-t-il ouvert le fichier (dev, ino) ?
fn open_elsewhere(dev: u64, ino: u64) -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return false;
    };
    for p in procs.flatten() {
        let name = p.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if std::fs::metadata(fd.path()).is_ok_and(|m| m.dev() == dev && m.ino() == ino) {
                return true;
            }
        }
    }
    false
}

/// Entrée du registre : (pid, date de démarrage) du détenteur, et le
/// fichier (dev, ino, chemin).
fn parse(name: &str, text: &str) -> Option<(u32, u64, u64, u64, PathBuf)> {
    let mut parts = name.split('-');
    let pid = parts.next()?.parse().ok()?;
    let start = parts.next()?.parse().ok()?;
    let (ids, path) = text.split_once('\n')?;
    let (dev, ino) = ids.split_once(' ')?;
    Some((
        pid,
        start,
        dev.parse().ok()?,
        ino.parse().ok()?,
        PathBuf::from(path),
    ))
}

/// Au démarrage : efface les fichiers laissés par un processus mort sans
/// fermer ses handles (SIGKILL).
pub(crate) fn sweep() {
    if let Some(dir) = ledger_dir() {
        sweep_in(&dir);
    }
}

fn sweep_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Some((pid, start, dev, ino, path)) = parse(&name, &text) else {
            let _ = std::fs::remove_file(e.path());
            continue;
        };
        if start_time(pid) == Some(start) {
            continue; // détenteur vivant
        }
        let removed = !open_elsewhere(dev, ino) && remove_if_same(&path, dev, ino);
        crate::trace!(
            "effacement à la fermeture laissé par le processus {pid} : {} = {}",
            path.display(),
            if removed { "effacé" } else { "gardé" }
        );
        let _ = std::fs::remove_file(e.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_d_un_processus_mort_efface_au_demarrage() {
        let root = std::env::temp_dir().join(format!("winlux-doomed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ledger = root.join("registre");
        std::fs::create_dir_all(&ledger).unwrap();
        let lock = root.join("lockfile");
        let kept = root.join("garde");
        std::fs::write(&lock, b"").unwrap();
        std::fs::write(&kept, b"").unwrap();
        let id = |p: &Path| {
            let m = std::fs::metadata(p).unwrap();
            format!("{} {}\n{}", m.dev(), m.ino(), p.display())
        };
        // Processus mort : aucun PID ne vaut 0x7fffffff, ou sa date diffère.
        std::fs::write(ledger.join("2147483646-1-10"), id(&lock)).unwrap();
        // Détenteur vivant (nous) : gardé.
        let me = std::process::id();
        let mine = format!("{me}-{}-20", start_time(me).unwrap());
        std::fs::write(ledger.join(&mine), id(&kept)).unwrap();
        // Nom repris par un autre fichier : jamais effacé.
        let other = root.join("autre");
        std::fs::write(&other, b"").unwrap();
        std::fs::write(
            ledger.join("2147483646-1-30"),
            format!("1 1\n{}", other.display()),
        )
        .unwrap();

        sweep_in(&ledger);
        assert!(!lock.exists(), "fichier du processus mort effacé");
        assert!(kept.exists(), "fichier d'un processus vivant gardé");
        assert!(other.exists(), "autre fichier sous le même nom gardé");
        assert!(ledger.join(&mine).exists());
        assert!(!ledger.join("2147483646-1-10").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nom_repris_par_un_nouveau_fichier_garde() {
        let root = std::env::temp_dir().join(format!("winlux-doomed3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ledger = root.join("registre");
        std::fs::create_dir_all(&ledger).unwrap();
        let lock = root.join("lockfile");
        std::fs::write(&lock, b"ancien").unwrap();
        let m = std::fs::metadata(&lock).unwrap();
        std::fs::write(
            ledger.join("2147483646-1-10"),
            format!("{} {}\n{}", m.dev(), m.ino(), lock.display()),
        )
        .unwrap();
        // Le fichier noté disparaît, un autre prend son nom (nouvel inode,
        // l'ancien est gardé vivant pour que le numéro ne soit pas réutilisé).
        let old = root.join("ancien");
        std::fs::rename(&lock, &old).unwrap();
        std::fs::write(&lock, b"nouveau").unwrap();
        assert_ne!(std::fs::metadata(&lock).unwrap().ino(), m.ino());
        sweep_in(&ledger);
        assert_eq!(
            std::fs::read(&lock).unwrap(),
            b"nouveau",
            "nouveau fichier gardé"
        );
        assert!(old.exists(), "l'ancien n'est pas cherché sous un autre nom");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fichier_ouvert_ailleurs_garde() {
        let root = std::env::temp_dir().join(format!("winlux-doomed2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("ouvert");
        std::fs::write(&f, b"").unwrap();
        let held = std::fs::File::open(&f).unwrap();
        let m = held.metadata().unwrap();
        assert!(open_elsewhere(m.dev(), m.ino()));
        drop(held);
        let _ = std::fs::remove_dir_all(&root);
    }
}
