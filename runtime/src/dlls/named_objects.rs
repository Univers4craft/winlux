//! Objets nommés visibles d'un processus Windows à l'autre (#20).
//!
//! Sous Windows, `CreateFileMapping`, `CreateEvent`, `CreateMutex` et
//! `CreateSemaphore` avec un nom désignent le même objet dans tous les
//! processus de la session : Chromium, Electron, les installeurs et les
//! applications « une seule instance » s'en servent pour se retrouver.
//!
//! Ici, chaque processus Windows est un processus Linux. Un objet nommé y
//! devient un fichier d'un dossier **propre au compte** (droits 0700, à son
//! propriétaire, vérifiés à chaque ouverture) :
//!
//! - `$XDG_RUNTIME_DIR/winlux-objets` quand la session en fournit un ;
//! - sinon `/dev/shm/winlux-objets-<uid>`, sinon `/tmp/winlux-objets-<uid>`.
//!
//! Les dossiers communs (`/dev/shm`, `/tmp`, `/run/winlux`) sont « sticky » et
//! partagés entre comptes (#578) : on n'y dépose jamais l'objet lui-même,
//! seulement le dossier privé d'un compte. Un même nom donné par deux comptes
//! désigne donc deux objets différents, et un compte ne peut ni lire ni
//! ouvrir les objets d'un autre.
//!
//! Le fichier porte l'objet : les pages d'une mémoire partagée anonyme, ou
//! la page d'état d'un événement, mutex ou sémaphore (attendue par futex
//! entre processus, voir `inherit::SharedWord`). Une section adossée à un
//! fichier ne garde que le chemin et la taille de celui-ci (fichier
//! `<objet>.chemin`).
//!
//! Durée de vie : comme sous Windows, l'objet disparaît quand le dernier
//! handle se ferme. Chaque processus qui le tient garde ouvert le fichier
//! avec un verrou `flock` partagé ; celui qui ferme le dernier handle de
//! son processus essaie de passer ce verrou en exclusif : s'il y arrive,
//! plus personne ne tient l'objet et le fichier est supprimé. Un processus
//! tué relâche ses verrous tout seul : son objet orphelin est reconnu (verrou
//! exclusif possible) et recréé à neuf par le suivant. Créations, ouvertures
//! et suppressions passent par le verrou exclusif du fichier `.verrou` du
//! dossier, ce qui évite toute course entre processus.

use std::collections::HashMap;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Mémoire partagée (section).
pub(crate) const SECTION: char = 'm';
/// Événement, mutex ou sémaphore.
pub(crate) const SYNC: char = 's';

/// Clé d'un nom : sans préfixe d'espace de noms (`Local\`, `Global\`,
/// `Session\`), sans casse. Les noms passés à ntdll
/// (`\Sessions\1\BaseNamedObjects\X`, `\BaseNamedObjects\X`) donnent la
/// même clé que `X`.
pub(crate) fn name_key(name: &str) -> String {
    let mut lower = name.to_lowercase();
    if let Some(i) = lower.find("basenamedobjects\\") {
        lower = lower[i + "basenamedobjects\\".len()..].to_string();
    }
    for prefix in ["local\\", "global\\", "session\\"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    lower
}

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

/// Le dossier est-il bien un dossier privé de ce compte (pas un lien, à lui,
/// sans aucun droit pour les autres) ? Un dossier trop ouvert est refusé tel
/// quel, jamais modifié : on passe alors à l'emplacement suivant.
fn private_dir(d: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(d) else {
        return false;
    };
    meta.is_dir() && meta.uid() == uid() && meta.mode() & 0o077 == 0
}

fn make_private_dir(d: &Path) -> bool {
    let _ = std::fs::DirBuilder::new().mode(0o700).create(d);
    private_dir(d)
}

/// Dossier des objets nommés de ce compte. `WINLUX_OBJETS_DIR` le remplace
/// (tests) ; il doit lui aussi être privé.
pub(crate) fn dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("WINLUX_OBJETS_DIR") {
            let d = PathBuf::from(d);
            return make_private_dir(&d).then_some(d);
        }
        candidates().into_iter().find(|d| make_private_dir(d))
    })
    .clone()
}

/// Emplacements possibles, dans l'ordre : le dossier de session du compte,
/// puis un dossier à son nom dans la mémoire partagée ou /tmp.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        if private_dir(&run) {
            out.push(run.join("winlux-objets"));
        }
    }
    for base in ["/dev/shm", "/tmp"] {
        if Path::new(base).is_dir() {
            out.push(Path::new(base).join(format!("winlux-objets-{}", uid())));
        }
    }
    out
}

/// Nom de fichier d'un objet : son type et sa clé en hexadécimal (aucun
/// caractère du nom ne peut sortir du dossier).
fn file_name(kind: char, key: &str) -> String {
    let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
    if hex.len() <= 200 {
        return format!("{kind}-{hex}");
    }
    // Nom très long : début lisible + empreinte FNV-1a 64 bits du tout.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{kind}-{}-{h:016x}", &hex[..160])
}

/// Objet tenu par ce processus : fichier ouvert (verrou partagé) et nombre
/// de handles qui le désignent ici.
struct Held {
    file: File,
    path: PathBuf,
    count: usize,
}

fn held() -> std::sync::MutexGuard<'static, HashMap<(char, String), Held>> {
    static H: OnceLock<Mutex<HashMap<(char, String), Held>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Verrou exclusif du dossier, rendu à la fin de la portée.
struct DirLock(File);

impl DirLock {
    fn take(dir: &Path) -> Option<DirLock> {
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(dir.join(".verrou"))
            .ok()?;
        loop {
            if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } == 0 {
                return Some(DirLock(f));
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                return None;
            }
        }
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

fn flock(f: &File, op: i32) -> bool {
    unsafe { libc::flock(f.as_raw_fd(), op) == 0 }
}

/// Copie du descripteur d'un fichier (fermée à l'exec).
pub(crate) fn dup(f: &File) -> Option<OwnedFd> {
    let c = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    (c >= 0).then(|| unsafe { OwnedFd::from_raw_fd(c) })
}

/// Résultat d'une ouverture : descripteur du fichier de l'objet, et vrai
/// s'il vient d'être créé (à neuf, ou à la place d'un objet orphelin).
pub(crate) struct Opened {
    pub(crate) fd: OwnedFd,
    pub(crate) created: bool,
    pub(crate) path: PathBuf,
}

/// Ouvre l'objet `kind`/`key` du compte, ou le crée si `create` est donné.
/// `create` prépare le fichier neuf (taille, état initial) pendant que le
/// dossier est verrouillé : aucun autre processus ne le voit à moitié prêt.
/// L'objet est tenu par ce processus jusqu'au dernier `release`, `retain`
/// comptant chaque handle. None : pas d'objet (ou dossier indisponible).
pub(crate) fn open(
    kind: char,
    key: &str,
    create: Option<&mut dyn FnMut(&File, &Path) -> bool>,
) -> Option<Opened> {
    let mut held = held();
    if let Some(h) = held.get(&(kind, key.to_string())) {
        return Some(Opened {
            fd: dup(&h.file)?,
            created: false,
            path: h.path.clone(),
        });
    }
    let dir = dir()?;
    let _lock = DirLock::take(&dir)?;
    static SWEPT: std::sync::Once = std::sync::Once::new();
    SWEPT.call_once(|| sweep_orphans(&dir));
    let path = dir.join(file_name(kind, key));
    let existing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&path)
        .ok();
    let (file, created) = match existing {
        // Personne ne le tient (processus disparu) : objet orphelin.
        Some(f) if flock(&f, libc::LOCK_EX | libc::LOCK_NB) => {
            let create = create?;
            let _ = std::fs::remove_file(sidecar(&path));
            if f.set_len(0).is_err() || !create(&f, &path) {
                let _ = std::fs::remove_file(&path);
                return None;
            }
            (f, true)
        }
        Some(f) => (f, false),
        None => {
            let create = create?;
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(&path)
                .ok()?;
            if !create(&f, &path) {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(sidecar(&path));
                return None;
            }
            (f, true)
        }
    };
    // Verrou partagé : « ce processus tient l'objet » (remplace l'exclusif
    // pris sur un orphelin).
    if !flock(&file, libc::LOCK_SH) {
        return None;
    }
    let fd = dup(&file)?;
    held.insert(
        (kind, key.to_string()),
        Held {
            file,
            path: path.clone(),
            count: 0,
        },
    );
    Some(Opened { fd, created, path })
}

/// Supprime les objets que plus aucun processus ne tient (#1911) : ceux
/// d'un processus tué (TerminateProcess, plantage, OOM), qui n'a lâché ni
/// `release` ni `on_exit`. Sans cela leurs pages restaient sur le tmpfs,
/// donc en RAM, jusqu'au redémarrage (Chromium et CEF donnent des noms
/// aléatoires à leurs mémoires partagées : le nom n'est jamais repris).
/// Appelé dossier verrouillé, une fois par processus.
fn sweep_orphans(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("m-") || name.starts_with("s-")) {
            continue;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Some(object) = name.strip_suffix(".chemin") {
            // `.chemin` dont l'objet a disparu.
            if !dir.join(object).exists() {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        let Ok(f) = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
        else {
            continue;
        };
        if flock(&f, libc::LOCK_EX | libc::LOCK_NB) {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(sidecar(&path));
        }
    }
}

/// Fichier `.chemin` d'une section adossée à un fichier.
pub(crate) fn sidecar(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".chemin");
    PathBuf::from(s)
}

/// Un handle de plus désigne l'objet dans ce processus.
pub(crate) fn retain(kind: char, key: &str) {
    if let Some(h) = held().get_mut(&(kind, key.to_string())) {
        h.count += 1;
    }
}

/// Un handle de moins ; au dernier, l'objet est lâché, et supprimé si plus
/// aucun processus ne le tient.
pub(crate) fn release(kind: char, key: &str) {
    let mut held = held();
    let k = (kind, key.to_string());
    let Some(h) = held.get_mut(&k) else {
        return;
    };
    h.count = h.count.saturating_sub(1);
    if h.count > 0 {
        return;
    }
    if let Some(h) = held.remove(&k) {
        drop_held(h);
    }
}

fn drop_held(h: Held) {
    let Some(dir) = dir() else {
        return;
    };
    let Some(_lock) = DirLock::take(&dir) else {
        return;
    };
    if flock(&h.file, libc::LOCK_EX | libc::LOCK_NB) {
        let _ = std::fs::remove_file(&h.path);
        let _ = std::fs::remove_file(sidecar(&h.path));
    }
}

/// Fin du processus : ses objets nommés sont lâchés (et supprimés s'il était
/// le dernier à les tenir).
pub(crate) fn on_exit() {
    let all: Vec<Held> = held().drain().map(|(_, h)| h).collect();
    for h in all {
        drop_held(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;

    fn unique(tag: &str) -> String {
        format!("winlux-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn cles_et_noms_de_fichier() {
        assert_eq!(name_key("Local\\Abc"), "abc");
        assert_eq!(name_key("Global\\Abc"), "abc");
        assert_eq!(name_key("\\Sessions\\1\\BaseNamedObjects\\Abc"), "abc");
        assert_eq!(name_key("\\BaseNamedObjects\\Local\\Abc"), "abc");
        assert_eq!(file_name(SYNC, "a/../b"), "s-612f2e2e2f62");
        let long = "x".repeat(300);
        let n = file_name(SECTION, &long);
        assert!(n.len() < 200, "{n}");
        assert_ne!(n, file_name(SECTION, &"x".repeat(301)));
    }

    #[test]
    fn dossier_prive_au_compte() {
        let d = dir().expect("dossier des objets nommés");
        let meta = std::fs::symlink_metadata(&d).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.uid(), uid());
        assert_eq!(meta.mode() & 0o077, 0, "{d:?} ouvert aux autres comptes");
        // Le nom du dossier commun porte l'uid : deux comptes, deux dossiers.
        if std::env::var_os("WINLUX_OBJETS_DIR").is_none() {
            let s = d.to_string_lossy();
            assert!(
                s.ends_with("winlux-objets") || s.ends_with(&format!("winlux-objets-{}", uid())),
                "{s}"
            );
        }
        // Un dossier trop ouvert est refusé, et laissé tel quel.
        let open = d.join(format!("ouvert-{}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&open)
            .unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!private_dir(&open));
        assert!(!make_private_dir(&open));
        let mode = std::fs::symlink_metadata(&open).unwrap().mode() & 0o777;
        std::fs::remove_dir(&open).unwrap();
        assert_eq!(mode, 0o755);
        // Un lien vers un dossier privé n'en est pas un.
        let link = d.join(format!("lien-{}", std::process::id()));
        std::os::unix::fs::symlink(&d, &link).unwrap();
        let refused = !private_dir(&link);
        std::fs::remove_file(&link).unwrap();
        assert!(refused);
    }

    #[test]
    fn creation_ouverture_et_suppression_au_dernier() {
        let key = unique("vie");
        assert!(open(SYNC, &key, None).is_none());
        let mut init = |f: &File, _: &Path| f.set_len(4096).is_ok();
        let first = open(SYNC, &key, Some(&mut init)).unwrap();
        assert!(first.created);
        retain(SYNC, &key);
        // Un autre « processus » : une ouverture distincte du même fichier.
        let other = File::open(&first.path).unwrap();
        assert!(flock(&other, libc::LOCK_SH | libc::LOCK_NB));
        let again = open(SYNC, &key, None).unwrap();
        assert!(!again.created);
        release(SYNC, &key);
        // L'autre le tient encore : le fichier reste.
        assert!(first.path.exists());
        let reopened = open(SYNC, &key, None).unwrap();
        assert!(!reopened.created);
        retain(SYNC, &key);
        drop(other);
        release(SYNC, &key);
        // Un test parallèle qui lance un processus (fork puis exec) peut
        // tenir, le temps de son exec, une copie du verrou partagé : le
        // fichier reste alors, mais en orphelin que plus personne ne tient
        // (#1140). Il ne doit jamais rester tenu.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while first.path.exists() {
            let left = File::open(&first.path);
            if left.is_ok_and(|f| flock(&f, libc::LOCK_EX | libc::LOCK_NB)) {
                let _ = std::fs::remove_file(&first.path);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "objet encore tenu après le dernier release"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(open(SYNC, &key, None).is_none());
    }

    #[test]
    fn orphelins_balayes_les_objets_tenus_gardes() {
        let d = dir().unwrap();
        let orphan = d.join(file_name(SECTION, &unique("balai-orphelin")));
        let kept = d.join(file_name(SYNC, &unique("balai-tenu")));
        let lone = d.join(format!(
            "{}.chemin",
            file_name(SECTION, &unique("balai-seul"))
        ));
        std::fs::write(&orphan, [0u8; 64]).unwrap();
        std::fs::write(sidecar(&orphan), "64\n/tmp/x").unwrap();
        std::fs::write(&kept, [0u8; 64]).unwrap();
        std::fs::write(&lone, "1\n/tmp/y").unwrap();
        // Un autre « processus » tient `kept`.
        let holder = File::open(&kept).unwrap();
        assert!(flock(&holder, libc::LOCK_SH | libc::LOCK_NB));
        {
            let _lock = DirLock::take(&d).unwrap();
            sweep_orphans(&d);
        }
        assert!(!orphan.exists() && !sidecar(&orphan).exists());
        assert!(!lone.exists());
        assert!(kept.exists());
        drop(holder);
        let _ = std::fs::remove_file(&kept);
    }

    #[test]
    fn objet_orphelin_recree_a_neuf() {
        let key = unique("orphelin");
        let path = dir().unwrap().join(file_name(SECTION, &key));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        f.write_all(b"reste d'un processus tue").unwrap();
        drop(f);
        // Sans création demandée, un orphelin n'est pas rendu.
        assert!(open(SECTION, &key, None).is_none());
        let mut init = |f: &File, _: &Path| {
            let mut f = f;
            f.write_all(b"neuf").is_ok()
        };
        let o = open(SECTION, &key, Some(&mut init)).unwrap();
        assert!(o.created);
        let mut s = String::new();
        File::open(&path).unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "neuf");
        retain(SECTION, &key);
        release(SECTION, &key);
        assert!(!path.exists());
    }
}

/// Objets nommés vus depuis un autre processus du compte : le test relance
/// l'exécutable de test lui-même comme second processus.
#[cfg(test)]
mod entre_processus {
    use crate::dlls::kernel32 as k;
    use crate::dlls::mmap;

    const ENFANT: &str = "WINLUX_TEST_OBJETS_ENFANT";

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Côté enfant : ne fait rien quand le test est lancé normalement.
    #[test]
    fn enfant_objets_nommes() {
        let Ok(base) = std::env::var(ENFANT) else {
            return;
        };
        let _teb = crate::process::test_teb();
        unsafe {
            let mem = mmap::open_file_mapping_w(2, 0, w(&format!("Local\\{base}-mem")).as_ptr());
            assert_ne!(mem, 0, "mémoire partagée introuvable");
            let p = mmap::map_view_of_file(mem, 2, 0, 0, 0) as *mut u8;
            assert_eq!(*p, 42);
            *p = 43;
            let sem = k::open_semaphore_w(0x1F_0003, 0, w(&format!("{base}-sem")).as_ptr());
            assert_ne!(sem, 0);
            assert_eq!(k::release_semaphore(sem, 1, std::ptr::null_mut()), 1);
            let mx = k::create_mutex_w(0, 1, w(&format!("{base}-mx")).as_ptr());
            assert_eq!(k::get_last_error(), 183, "le mutex du parent doit exister");
            assert_eq!(
                k::wait_for_single_object(mx, 0),
                258,
                "mutex tenu par le parent"
            );
            // Un nom d'un autre type : ERROR_INVALID_HANDLE.
            assert_eq!(
                k::open_semaphore_w(0, 0, w(&format!("{base}-mx")).as_ptr()),
                0
            );
            assert_eq!(k::get_last_error(), 6);
            let ev = k::open_event_a(
                0,
                0,
                std::ffi::CString::new(format!("{base}-ev"))
                    .unwrap()
                    .as_ptr()
                    .cast(),
            );
            assert_ne!(ev, 0);
            assert_eq!(k::set_event(ev), 1);
            let go = k::create_event_w(0, 0, 0, w(&format!("{base}-go")).as_ptr());
            assert_eq!(
                k::wait_for_single_object(go, 10_000),
                0,
                "le parent n'a pas relâché"
            );
            // Le parent a rendu le mutex : on le prend et on meurt avec.
            assert_eq!(k::wait_for_single_object(mx, 10_000), 0);
        }
    }

    #[test]
    fn objets_nommes_partages_entre_processus() {
        if std::env::var_os(ENFANT).is_some() || super::dir().is_none() {
            return;
        }
        let _teb = crate::process::test_teb();
        let base = format!("winlux-objets-{}", std::process::id());
        unsafe {
            let mem = mmap::create_file_mapping_w(
                usize::MAX,
                0,
                4,
                0,
                4096,
                w(&format!("Global\\{base}-mem")).as_ptr(),
            );
            assert_ne!(mem, 0);
            let p = mmap::map_view_of_file(mem, 2, 0, 0, 0) as *mut u8;
            *p = 42;
            let sem = k::create_semaphore_w(0, 0, 2, w(&format!("{base}-sem")).as_ptr());
            let mx = k::create_mutex_w(0, 1, w(&format!("{base}-mx")).as_ptr());
            let ev = k::create_event_w(0, 1, 0, w(&format!("{base}-ev")).as_ptr());
            let go = k::create_event_w(0, 0, 0, w(&format!("{base}-go")).as_ptr());
            for h in [sem, mx, ev, go] {
                assert_ne!(h, 0);
            }
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "dlls::named_objects::entre_processus::enfant_objets_nommes",
                    "--test-threads=1",
                ])
                .env(ENFANT, &base)
                .spawn()
                .unwrap();
            assert_eq!(
                k::wait_for_single_object(ev, 20_000),
                0,
                "l'enfant n'a pas signalé"
            );
            assert_eq!(*p, 43, "écriture de l'enfant invisible");
            assert_eq!(
                k::wait_for_single_object(sem, 0),
                0,
                "ReleaseSemaphore de l'enfant perdu"
            );
            assert_eq!(k::wait_for_single_object(sem, 0), 258);
            assert_eq!(k::release_mutex(mx), 1);
            assert_eq!(k::set_event(go), 1);
            assert!(
                child.wait().unwrap().success(),
                "le processus enfant a échoué"
            );
            // L'enfant est mort en tenant le mutex : abandonné.
            assert_eq!(k::wait_for_single_object(mx, 2_000), 0x80);
            assert_eq!(k::release_mutex(mx), 1);
            mmap::unmap_view_of_file(p as usize);
            for h in [mem, sem, mx, ev, go] {
                assert_eq!(k::close_handle(h), 1);
            }
            // Dernier handle fermé : les noms n'existent plus nulle part.
            assert_eq!(
                k::open_event_a(
                    0,
                    0,
                    std::ffi::CString::new(format!("{base}-ev"))
                        .unwrap()
                        .as_ptr()
                        .cast()
                ),
                0
            );
            assert_eq!(k::get_last_error(), 2);
            assert_eq!(
                mmap::open_file_mapping_w(2, 0, w(&format!("{base}-mem")).as_ptr()),
                0
            );
            let left: Vec<_> = std::fs::read_dir(super::dir().unwrap())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let hex: String = base.bytes().map(|b| format!("{b:02x}")).collect();
                    e.file_name().to_string_lossy().contains(&hex)
                })
                .collect();
            assert!(left.is_empty(), "fichiers d'objets restés : {left:?}");
        }
    }
}
