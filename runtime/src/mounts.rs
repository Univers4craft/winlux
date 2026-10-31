//! Registre des lecteurs dynamiques (images disque montées, lecteurs réseau…).
//!
//! Le registre est partagé entre tous les processus Winlux : c'est un fichier
//! texte à colonnes séparées par des tabulations, une ligne par lecteur :
//!
//! `lettre  type  point_de_montage  libellé  ro  source  périphérique  backend  [état]`
//!
//! - `type` : `image` (image disque montée), `network` (FTP, SFTP, SMB…),
//!   `disk` (clé USB, disque NTFS… monté par `winlux --volumes`) ;
//! - `ro` : `1` si le lecteur est en lecture seule ;
//! - `état` (optionnel) : `ok`, `offline` ou `connecting`. Un lecteur hors
//!   ligne est résolu vers un chemin inexistant pour échouer vite.
//!
//! C: (dossier personnel) et Z: (racine Linux) ne passent jamais par ici.

#![allow(dead_code)] // Une partie de l'API sert aux lecteurs réseau.

use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Mount {
    pub letter: char,
    pub kind: String,
    pub mountpoint: PathBuf,
    pub label: String,
    pub read_only: bool,
    pub source: String,
    pub device: String,
    pub backend: String,
    pub state: String,
}

impl Mount {
    pub fn online(&self) -> bool {
        self.state.is_empty() || self.state == "ok"
    }

    fn to_line(&self) -> String {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let mut l = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.letter,
            clean(&self.kind),
            clean(&self.mountpoint.to_string_lossy()),
            clean(&self.label),
            if self.read_only { 1 } else { 0 },
            clean(&self.source),
            clean(&self.device),
            clean(&self.backend),
        );
        if !self.state.is_empty() {
            l.push('\t');
            l.push_str(&clean(&self.state));
        }
        l
    }

    fn parse(line: &str) -> Option<Mount> {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 8 {
            return None;
        }
        let letter = c[0].chars().next()?.to_ascii_uppercase();
        if !letter.is_ascii_uppercase() || c[0].len() != 1 {
            return None;
        }
        Some(Mount {
            letter,
            kind: c[1].to_string(),
            mountpoint: PathBuf::from(c[2]),
            label: c[3].to_string(),
            read_only: c[4] == "1",
            source: c[5].to_string(),
            device: c[6].to_string(),
            backend: c[7].to_string(),
            state: c.get(8).map(|s| s.to_string()).unwrap_or_default(),
        })
    }
}

/// Dossier d'état partagé : `/run/winlux` si inscriptible, sinon
/// `$XDG_RUNTIME_DIR/winlux`, sinon le dossier de données Winlux.
pub fn state_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WINLUX_STATE_DIR") {
        return PathBuf::from(d);
    }
    let run = PathBuf::from("/run/winlux");
    if std::fs::create_dir_all(&run).is_ok() && writable(&run) {
        return run;
    }
    if let Some(x) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(x).join("winlux");
    }
    crate::paths::data_dir().unwrap_or_else(|| std::env::temp_dir().join("winlux"))
}

fn writable(p: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap_or_default();
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

pub fn registry_file() -> PathBuf {
    state_dir().join("mounts.tsv")
}

fn parse_all(text: &str) -> Vec<Mount> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(Mount::parse)
        .collect()
}

/// Tous les lecteurs dynamiques déclarés.
pub fn list() -> Vec<Mount> {
    std::fs::read_to_string(registry_file())
        .map(|t| parse_all(&t))
        .unwrap_or_default()
}

pub fn get(letter: char) -> Option<Mount> {
    let l = letter.to_ascii_uppercase();
    list().into_iter().find(|m| m.letter == l)
}

/// Lettres réservées hors registre : C:, Z: et les `WINLUX_DRIVE_X`.
fn reserved(letter: char) -> bool {
    letter == 'C' || letter == 'Z' || std::env::var_os(format!("WINLUX_DRIVE_{letter}")).is_some()
}

fn first_free(used: &[Mount]) -> Option<char> {
    ('D'..='Y').find(|&l| !reserved(l) && !used.iter().any(|m| m.letter == l))
}

/// Première lettre libre de D: à Y: (les lecteurs réseau partent de Y: vers le bas).
pub fn alloc_letter() -> Option<char> {
    first_free(&list())
}

fn with_lock<T>(f: impl FnOnce(&mut Vec<Mount>) -> std::io::Result<T>) -> std::io::Result<T> {
    use std::os::unix::io::AsRawFd;
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("mounts.lock"))?;
    unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) };
    let file = dir.join("mounts.tsv");
    let mut all = std::fs::read_to_string(&file)
        .map(|t| parse_all(&t))
        .unwrap_or_default();
    let r = f(&mut all)?;
    let tmp = dir.join(format!("mounts.tsv.{}", std::process::id()));
    {
        let mut out = std::fs::File::create(&tmp)?;
        for m in all.iter() {
            writeln!(out, "{}", m.to_line())?;
        }
    }
    std::fs::rename(&tmp, &file)?;
    unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) };
    Ok(r)
}

/// Ajoute un lecteur ; refuse une lettre déjà occupée ou réservée.
pub fn add(m: Mount) -> std::io::Result<()> {
    with_lock(|all| {
        if reserved(m.letter) || all.iter().any(|x| x.letter == m.letter) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("lettre {}: déjà utilisée", m.letter),
            ));
        }
        all.push(m);
        all.sort_by_key(|x| x.letter);
        Ok(())
    })
}

/// Réserve la première lettre libre et ajoute le lecteur en une opération.
pub fn add_auto(mut m: Mount) -> std::io::Result<char> {
    with_lock(|all| {
        let l = first_free(all)
            .ok_or_else(|| std::io::Error::other("aucune lettre de lecteur libre"))?;
        m.letter = l;
        all.push(m);
        all.sort_by_key(|x| x.letter);
        Ok(l)
    })
}

pub fn remove(letter: char) -> std::io::Result<Option<Mount>> {
    let l = letter.to_ascii_uppercase();
    with_lock(|all| {
        let pos = all.iter().position(|m| m.letter == l);
        Ok(pos.map(|i| all.remove(i)))
    })
}

/// Change l'état (`ok`, `offline`, `connecting`) d'un lecteur.
#[allow(dead_code)]
pub fn set_state(letter: char, state: &str) -> std::io::Result<()> {
    let l = letter.to_ascii_uppercase();
    with_lock(|all| {
        for m in all.iter_mut().filter(|m| m.letter == l) {
            m.state = state.to_string();
        }
        Ok(())
    })
}

/// Racine Linux d'une lettre dynamique, pour `paths::to_linux`.
pub fn root_of(letter: char) -> Option<PathBuf> {
    let m = get(letter)?;
    Some(if m.online() {
        m.mountpoint
    } else {
        PathBuf::from(format!("/nonexistent/winlux-offline/{}", m.letter))
    })
}

/// Masque de bits `GetLogicalDrives` des lecteurs dynamiques.
pub fn drive_mask() -> u32 {
    list()
        .iter()
        .fold(0, |acc, m| acc | 1 << (m.letter as u32 - 'A' as u32))
}

/// Valeur `GetDriveType` : 2 amovible, 3 fixe, 4 réseau, 5 CD, `None` sinon.
pub fn drive_type(letter: char) -> Option<u32> {
    get(letter).map(|m| match m.kind.as_str() {
        "network" => 4,
        // Disque branché : fixe (3) s'il est interne, amovible sinon.
        "disk" if m.backend.split(':').nth(1) == Some("fixed") => 3,
        "image" if m.source.to_ascii_lowercase().ends_with(".iso") => 5,
        _ => 2,
    })
}

/// `WinluxMountList(buf, cap)` : écrit en UTF-16 une ligne par lecteur
/// dynamique, `lettre\ttype\tlibellé\tro\tétat\tsource\n`, terminée par un
/// zéro. Renvoie le nombre de lecteurs (ou la taille requise en caractères,
/// négative, si `cap` est insuffisant).
pub unsafe extern "win64" fn winlux_mount_list(buf: *mut u16, cap: u32) -> i32 {
    // Images démontées hors de Winlux (redémarrage, umount manuel) : oubliées.
    crate::diskimage::prune_stale();
    let all = list();
    let mut text = String::new();
    for m in &all {
        let state = if m.state.is_empty() { "ok" } else { &m.state };
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            m.letter,
            m.kind,
            m.label,
            if m.read_only { 1 } else { 0 },
            state,
            m.source
        ));
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    if buf.is_null() || (w.len() + 1) > cap as usize {
        return -((w.len() + 1) as i32);
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    all.len() as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxMountList" => fn winlux_mount_list,
    }
}

/// Sérialise les tests qui changent les variables d'environnement d'état.
#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(letter: char) -> Mount {
        Mount {
            letter,
            kind: "image".into(),
            mountpoint: PathBuf::from("/run/winlux/media/x"),
            label: "Ubuntu 26.04".into(),
            read_only: true,
            source: "/home/u/ubuntu.iso".into(),
            device: "/dev/loop3".into(),
            backend: "loop".into(),
            state: String::new(),
        }
    }

    #[test]
    fn line_roundtrip() {
        let m = sample('E');
        assert_eq!(Mount::parse(&m.to_line()), Some(m.clone()));
        let mut n = m;
        n.state = "offline".into();
        let p = Mount::parse(&n.to_line()).unwrap();
        assert!(!p.online());
        assert!(Mount::parse("garbage").is_none());
    }

    #[test]
    fn registry_add_remove() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("winlux-mounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("WINLUX_STATE_DIR", &dir);
        let l = add_auto(sample('A')).unwrap();
        assert_eq!(l, 'D');
        assert_eq!(add_auto(sample('A')).unwrap(), 'E');
        assert!(add(sample('D')).is_err());
        assert!(add(sample('C')).is_err());
        add(sample('Y')).unwrap();
        assert_eq!(drive_mask(), 1 << 3 | 1 << 4 | 1 << 24);
        assert_eq!(drive_type('Y'), Some(5));
        set_state('Y', "offline").unwrap();
        assert!(root_of('Y').unwrap().starts_with("/nonexistent"));
        assert_eq!(remove('d').unwrap().map(|m| m.letter), Some('D'));
        assert_eq!(alloc_letter(), Some('D'));
        std::env::remove_var("WINLUX_STATE_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
