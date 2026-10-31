//! Correspondance entre chemins Windows et chemins Linux.
//!
//! - `C:\` correspond au dossier personnel (modifiable avec WINLUX_DRIVE_C) ;
//! - `Z:\` correspond à la racine `/` du système Linux.
//!
//! Windows ignore la casse des noms de fichiers : si le chemin exact
//! n'existe pas, on cherche chaque composant sans tenir compte de la casse.

use std::path::{Path, PathBuf};

pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(d).join("winlux"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/winlux"))
}

pub fn drive_c() -> PathBuf {
    std::env::var_os("WINLUX_DRIVE_C")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Chemin Linux absolu → chemin Windows (`C:\...` ou `Z:\...`).
pub fn to_windows(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    for m in crate::mounts::list() {
        if let Ok(r) = abs.strip_prefix(&m.mountpoint) {
            let mut s = format!("{}:\\", m.letter);
            s.push_str(&r.to_string_lossy().replace('/', "\\"));
            return s;
        }
    }
    let c = std::fs::canonicalize(drive_c()).unwrap_or_else(|_| drive_c());
    let (drive, rest) = match abs.strip_prefix(&c) {
        Ok(r) => ("C:", r.to_path_buf()),
        Err(_) => (
            "Z:",
            abs.strip_prefix("/")
                .map(|r| r.to_path_buf())
                .unwrap_or(abs.clone()),
        ),
    };
    let mut s = format!("{drive}\\");
    s.push_str(&rest.to_string_lossy().replace('/', "\\"));
    s
}

/// Chemin Windows (absolu ou relatif) → chemin Linux.
pub fn to_linux(w: &str) -> PathBuf {
    guard(to_linux_inner(w, true))
}

/// Dossiers personnels : l'application doit en avoir reçu l'autorisation.
fn guard(p: PathBuf) -> PathBuf {
    if crate::permissions::path_allowed(&p) {
        p
    } else {
        crate::permissions::denied_path(&p)
    }
}

/// Comme `to_linux`, sans traverser les archives ouvertes : le chemin reste
/// `.../photos.zip/dossier` (sert à retrouver l'archive elle-même).
pub fn to_linux_raw(w: &str) -> PathBuf {
    guard(to_linux_inner(w, false))
}

fn to_linux_inner(w: &str, archives: bool) -> PathBuf {
    let mut w = w.trim_end_matches('\0');
    for pre in ["\\\\?\\", "\\??\\", "\\\\.\\"] {
        if let Some(r) = w.strip_prefix(pre) {
            w = r;
        }
    }
    let b = w.as_bytes();
    let (root, rest): (Option<PathBuf>, &str) =
        if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
            let letter = b[0].to_ascii_uppercase() as char;
            let root = match letter {
                'C' => drive_c(),
                'Z' => PathBuf::from("/"),
                l => std::env::var_os(format!("WINLUX_DRIVE_{l}"))
                    .map(PathBuf::from)
                    .or_else(|| crate::mounts::root_of(l))
                    .unwrap_or_else(drive_c),
            };
            (Some(root), &w[2..])
        } else {
            (None, w)
        };
    let rel = rest.replace('\\', "/");
    let joined = match root {
        Some(r) => r.join(rel.trim_start_matches('/')),
        None => PathBuf::from(rel),
    };
    if joined.exists() {
        return joined;
    }
    // Chemin à l'intérieur d'une archive ouverte (navigation façon dossier).
    if archives {
        if let Some(v) = crate::archive::resolve(&joined) {
            return v;
        }
    }
    fix_case(&joined).unwrap_or(joined)
}

fn fix_case(p: &Path) -> Option<PathBuf> {
    let mut cur = if p.is_absolute() {
        PathBuf::from("/")
    } else {
        PathBuf::from(".")
    };
    let comps: Vec<_> = p.components().collect();
    for (i, comp) in comps.iter().enumerate() {
        let name = match comp {
            std::path::Component::Normal(n) => n.to_string_lossy().into_owned(),
            std::path::Component::RootDir => continue,
            other => {
                cur.push(other.as_os_str());
                continue;
            }
        };
        let exact = cur.join(&name);
        if exact.exists() {
            cur = exact;
            continue;
        }
        let lower = name.to_lowercase();
        let found = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower);
        match found {
            Some(e) => cur = e.path(),
            // Dernier composant absent (création de fichier) : on garde le nom tel quel.
            None if i == comps.len() - 1 => cur = exact,
            None => return None,
        }
    }
    Some(if p.is_absolute() {
        cur
    } else {
        cur.strip_prefix(".")
            .map(|r| r.to_path_buf())
            .unwrap_or(cur)
    })
}
