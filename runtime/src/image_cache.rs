//! Sections d'image PE recopiées alignées sur la page (#1388).
//!
//! Une section n'est projetable depuis le .exe que si sa position dans le
//! fichier est un multiple de la page. Ce n'est pas le cas du code de
//! claude.exe (161 Mo, alignement de fichier 0x200) : chaque processus en
//! gardait une copie anonyme, et un Claude en bac à sable compte neuf
//! processus. La section est donc écrite une fois, alignée, dans
//! `~/.cache/winlux/images/`, puis projetée en copie privée depuis ce
//! fichier : tous les processus partagent les mêmes pages du cache du noyau,
//! qui peut les rendre sous pression mémoire.

use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

fn dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            // Renderer de Chromium en bac à sable : environnement vidé, sans
            // HOME ; le dossier personnel vient alors du compte.
            .or_else(|| account_home().map(|h| h.join(".cache")))?;
        let d = base.join("winlux").join("images");
        let _ = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&d);
        d.is_dir().then_some(d)
    })
    .clone()
}

/// Dossier personnel du compte courant, d'après la base des comptes.
fn account_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut out = std::ptr::null_mut();
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut pw,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        )
    };
    if rc != 0 || out.is_null() || pw.pw_dir.is_null() {
        return None;
    }
    let dir = unsafe { std::ffi::CStr::from_ptr(pw.pw_dir) };
    let dir = PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()));
    dir.is_absolute().then_some(dir)
}

/// Empreinte FNV-1a 64 bits (nom de fichier, pas une signature).
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Nom du fichier de cache : le chemin du .exe, son identité (inode, taille,
/// dates de modification et de changement) et la place de la section. Toute
/// réécriture du .exe change le nom ; les versions précédentes du même
/// chemin sont effacées à la création d'une nouvelle.
fn names(image: &Path, meta: &std::fs::Metadata, offset: usize, len: usize) -> (String, String) {
    use std::os::unix::ffi::OsStrExt;
    let prefix = format!("{:016x}", fnv(image.as_os_str().as_bytes()));
    let name = format!(
        "{prefix}-{:x}-{:x}-{:x}.{:x}-{:x}.{:x}-{offset:x}-{len:x}.sec",
        meta.ino(),
        meta.size(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec()
    );
    (prefix, name)
}

/// Fichier de cache de `len` octets de `image` à partir de `offset`,
/// identiques à `bytes` ; créé au premier appel. `None` si le cache est
/// indisponible (l'appelant copie alors la section).
pub fn section_file(
    image: &Path,
    meta: &std::fs::Metadata,
    offset: usize,
    bytes: &[u8],
) -> Option<std::fs::File> {
    section_file_in(&dir()?, image, meta, offset, bytes)
}

fn section_file_in(
    dir: &Path,
    image: &Path,
    meta: &std::fs::Metadata,
    offset: usize,
    bytes: &[u8],
) -> Option<std::fs::File> {
    let (prefix, name) = names(image, meta, offset, bytes.len());
    let path = dir.join(&name);
    if let Ok(f) = std::fs::File::open(&path) {
        if f.metadata().ok()?.len() == bytes.len() as u64 {
            return Some(f);
        }
    }
    // Écriture dans un fichier temporaire puis renommage : un autre
    // processus ne voit jamais une section à moitié écrite.
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    let written = (|| {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .ok()?;
        f.write_all(bytes).ok()?;
        f.sync_data().ok()?;
        std::fs::rename(&tmp, &path).ok()
    })();
    if written.is_none() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    // Versions précédentes du même .exe : plus jamais lues.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name();
            let n = n.to_string_lossy();
            if n.starts_with(&prefix) && n.ends_with(".sec") && n != name.as_str() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    crate::trace!(
        "section de {} mise en cache alignée ({} octets)",
        image.display(),
        bytes.len()
    );
    let f = std::fs::File::open(&path).ok()?;
    (f.metadata().ok()?.len() == bytes.len() as u64).then_some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_change_avec_le_fichier_et_la_section() {
        let dir = std::env::temp_dir().join(format!("winlux-image-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app.exe");
        std::fs::write(&exe, b"MZ un").unwrap();
        let m1 = std::fs::metadata(&exe).unwrap();
        let (p1, a) = names(&exe, &m1, 0x400, 0x1000);
        let (_, b) = names(&exe, &m1, 0x600, 0x1000);
        assert_ne!(a, b);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&exe, b"MZ deux, plus long").unwrap();
        let m2 = std::fs::metadata(&exe).unwrap();
        let (p2, c) = names(&exe, &m2, 0x400, 0x1000);
        assert_eq!(p1, p2, "même chemin, même préfixe");
        assert_ne!(a, c, "le .exe réécrit change le nom");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn section_alignee_creee_puis_reprise_et_ancienne_version_effacee() {
        use std::io::Read;
        let dir = std::env::temp_dir().join(format!("winlux-image-sec-{}", std::process::id()));
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let exe = dir.join("app.exe");
        std::fs::write(&exe, vec![7u8; 0x3000]).unwrap();
        let m1 = std::fs::metadata(&exe).unwrap();
        let bytes: Vec<u8> = (0..0x2000u32).map(|i| i as u8).collect();
        let mut f = section_file_in(&cache, &exe, &m1, 0x200, &bytes).expect("cache");
        let mut back = Vec::new();
        f.read_to_end(&mut back).unwrap();
        assert_eq!(back, bytes);
        // Deuxième processus : même fichier, rien de réécrit.
        let again = section_file_in(&cache, &exe, &m1, 0x200, &bytes).unwrap();
        assert_eq!(again.metadata().unwrap().len(), 0x2000);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        // Nouvelle version du .exe : l'ancienne section disparaît.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&exe, vec![8u8; 0x3400]).unwrap();
        let m2 = std::fs::metadata(&exe).unwrap();
        section_file_in(&cache, &exe, &m2, 0x200, &bytes).unwrap();
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
