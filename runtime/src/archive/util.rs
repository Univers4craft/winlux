//! Outils communs : erreurs, progression, parcours d'arborescence, noms sûrs.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub enum ArcError {
    /// Archive chiffrée : un mot de passe est nécessaire.
    NeedPassword,
    /// Mot de passe incorrect.
    BadPassword,
    /// Format ou méthode non pris en charge (outil absent…).
    Unsupported(String),
    /// Format lisible mais pas modifiable (RAR…).
    ReadOnly,
    Cancelled,
    Io(io::Error),
}

impl From<io::Error> for ArcError {
    fn from(e: io::Error) -> Self {
        ArcError::Io(e)
    }
}

impl std::fmt::Display for ArcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArcError::NeedPassword => write!(f, "mot de passe requis"),
            ArcError::BadPassword => write!(f, "mot de passe incorrect"),
            ArcError::Unsupported(s) => write!(f, "non pris en charge : {s}"),
            ArcError::ReadOnly => write!(f, "archive en lecture seule"),
            ArcError::Cancelled => write!(f, "opération annulée"),
            ArcError::Io(e) => write!(f, "{e}"),
        }
    }
}

pub type Res<T> = Result<T, ArcError>;

/// Avancement partagé entre la tâche de fond et l'Explorer.
#[derive(Clone, Default)]
pub struct Progress {
    pub done: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

impl Progress {
    pub fn add(&self, n: u64) -> Res<()> {
        self.done.fetch_add(n, Ordering::Relaxed);
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ArcError::Cancelled);
        }
        Ok(())
    }
    pub fn set_total(&self, n: u64) {
        self.total.store(n, Ordering::Relaxed);
    }
    /// Avancement en millièmes.
    pub fn permille(&self) -> u32 {
        let t = self.total.load(Ordering::Relaxed);
        if t == 0 {
            return 0;
        }
        ((self.done.load(Ordering::Relaxed).min(t) * 1000) / t) as u32
    }
}

/// Nettoie un nom d'entrée d'archive : séparateurs `/` ou `\`, sans `..`,
/// sans racine. `None` si le nom tenterait de sortir du dossier cible.
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if p.len() == 2 && p.ends_with(':') => continue,
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Vérifie qu'aucun composant parent de `rel` sous `root` n'est un lien
/// symbolique (évite d'écrire hors du dossier via un lien de l'archive).
pub fn no_symlink_parent(root: &Path, rel: &Path) -> bool {
    let mut cur = root.to_path_buf();
    let comps: Vec<_> = rel.components().collect();
    for c in comps.iter().take(comps.len().saturating_sub(1)) {
        if let Component::Normal(n) = c {
            cur.push(n);
            if let Ok(m) = std::fs::symlink_metadata(&cur) {
                if m.file_type().is_symlink() {
                    return false;
                }
            }
        }
    }
    true
}

/// Une cible de lien symbolique reste-t-elle dans l'arborescence extraite ?
pub fn symlink_stays_inside(rel: &Path, target: &str) -> bool {
    if target.starts_with('/') {
        return false;
    }
    let mut depth = rel.components().count() as i64 - 1;
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

pub struct WalkEntry {
    pub rel: PathBuf,
    pub meta: std::fs::Metadata,
}

/// Parcours récursif trié (dossier avant son contenu), liens non suivis.
pub fn walk(root: &Path) -> io::Result<Vec<WalkEntry>> {
    let mut out = Vec::new();
    fn rec(root: &Path, rel: &Path, out: &mut Vec<WalkEntry>) -> io::Result<()> {
        let mut names: Vec<_> = std::fs::read_dir(root.join(rel))?
            .flatten()
            .map(|e| e.file_name())
            .collect();
        names.sort();
        for n in names {
            let r = rel.join(&n);
            let meta = std::fs::symlink_metadata(root.join(&r))?;
            let is_dir = meta.is_dir();
            out.push(WalkEntry {
                rel: r.clone(),
                meta,
            });
            if is_dir {
                rec(root, &r, out)?;
            }
        }
        Ok(())
    }
    rec(root, Path::new(""), &mut out)?;
    Ok(out)
}

/// Écrit `rel` en notation d'archive (séparateur `/`).
pub fn archive_name(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    meta.mtime()
}

pub fn set_mtime(path: &Path, secs: i64) {
    if secs <= 0 {
        return;
    }
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
    if let Ok(f) = std::fs::File::options().write(true).open(path) {
        let _ = f.set_modified(t);
    }
}

/// Lecteur qui compte les octets lus dans la progression.
pub struct Counted<'a, R> {
    pub inner: R,
    pub progress: &'a Progress,
}

impl<R: io::Read> io::Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if self.progress.add(n as u64).is_err() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "annulé"));
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_slip_rejected() {
        assert_eq!(safe_relative("a/b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_relative("\\a\\b"), Some(PathBuf::from("a/b")));
        assert_eq!(
            safe_relative("/etc/passwd"),
            Some(PathBuf::from("etc/passwd"))
        );
        assert_eq!(safe_relative("C:\\x"), Some(PathBuf::from("x")));
        assert!(safe_relative("../x").is_none());
        assert!(safe_relative("a/../../x").is_none());
        assert!(safe_relative("./").is_none());
        assert!(symlink_stays_inside(Path::new("a/l"), "../b"));
        assert!(!symlink_stays_inside(Path::new("l"), "../b"));
        assert!(!symlink_stays_inside(Path::new("l"), "/etc"));
    }
}
