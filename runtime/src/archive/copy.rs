//! Copier et déplacer des fichiers en tâche de fond pour l'Explorer (#11).
//!
//! L'Explorer prépare les paires source → destination (noms « (2) »,
//! garde-fous contre les cycles), puis confie le travail à ce module : la
//! fenêtre reste réactive, la progression est suivie octet par octet, la
//! tâche peut être mise en pause ou annulée.
//!
//! Règles, comme Windows :
//! - déplacement sur le même disque : simple renommage, instantané ;
//! - entre deux disques : copie, puis suppression de chaque source copiée ;
//! - chaque fichier est écrit sous un nom temporaire caché puis renommé :
//!   une copie annulée ou interrompue ne laisse jamais de fichier tronqué
//!   sous le vrai nom, et un fichier remplacé n'est perdu qu'une fois la
//!   nouvelle version complète ;
//! - annuler garde ce qui est déjà copié (Windows fait de même) ; un échec
//!   retire l'arborescence que la tâche venait de créer pour cet élément.

use super::util::*;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// Déplacer : la source disparaît une fois copiée.
pub const MOVE: u32 = 1;
/// Remplacer les fichiers existants (les dossiers sont fusionnés).
pub const REPLACE: u32 = 2;
/// Ignorer les fichiers déjà présents (les dossiers sont fusionnés).
pub const SKIP: u32 = 4;

/// Démarrer en pause (tests de l'Explorateur).
pub const START_PAUSED: u32 = 8;

const CHUNK: usize = 1 << 20;
const MAX_DEPTH: u32 = 256;

/// Taille et nombre d'éléments d'une arborescence (liens non suivis).
fn measure(p: &Path, depth: u32, pr: &Progress) -> Res<(u64, u64)> {
    pr.check()?;
    let m = fs::symlink_metadata(p)?;
    if !m.is_dir() || depth > MAX_DEPTH {
        return Ok((if m.is_file() { m.len() } else { 0 }, 1));
    }
    let (mut bytes, mut items) = (0, 1);
    for e in fs::read_dir(p)? {
        let (b, i) = measure(&e?.path(), depth + 1, pr)?;
        bytes += b;
        items += i;
    }
    Ok((bytes, items))
}

fn named(e: io::Error, p: &Path) -> ArcError {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string());
    ArcError::Io(io::Error::new(e.kind(), format!("{name} : {e}")))
}

fn temp_name(dst: &Path) -> PathBuf {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dst.with_file_name(format!(".~{name}.winlux-copie"))
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn copy_file(src: &Path, dst: &Path, m: &fs::Metadata, pr: &Progress) -> Res<()> {
    let tmp = temp_name(dst);
    let r = (|| -> Res<()> {
        let mut input = fs::File::open(src).map_err(|e| named(e, src))?;
        let mut out = fs::File::create(&tmp).map_err(|e| named(e, dst))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = match input.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(named(e, src)),
            };
            out.write_all(&buf[..n]).map_err(|e| named(e, dst))?;
            pr.add(n as u64)?;
        }
        out.flush().map_err(|e| named(e, dst))?;
        let _ = out.set_permissions(m.permissions());
        if let Ok(t) = m.modified() {
            let _ = out.set_modified(t);
        }
        drop(out);
        fs::rename(&tmp, dst).map_err(|e| named(e, dst))?;
        Ok(())
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r
}

/// Copie (ou déplace, avec `MOVE`) un élément et son contenu.
fn copy_item(src: &Path, dst: &Path, flags: u32, depth: u32, pr: &Progress) -> Res<()> {
    pr.check()?;
    if depth > MAX_DEPTH {
        return Err(named(io::Error::other("arborescence trop profonde"), src));
    }
    let m = fs::symlink_metadata(src).map_err(|e| named(e, src))?;
    if let Some(n) = src.file_name() {
        pr.set_current(&n.to_string_lossy());
    }
    let present = exists(dst);
    if m.is_dir() {
        if present && !fs::metadata(dst).map(|d| d.is_dir()).unwrap_or(false) {
            if flags & REPLACE == 0 {
                return Err(named(io::ErrorKind::AlreadyExists.into(), dst));
            }
            fs::remove_file(dst).map_err(|e| named(e, dst))?;
        }
        if !exists(dst) {
            fs::create_dir(dst).map_err(|e| named(e, dst))?;
        }
        for e in fs::read_dir(src).map_err(|e| named(e, src))? {
            let e = e.map_err(|e| named(e, src))?;
            copy_item(&e.path(), &dst.join(e.file_name()), flags, depth + 1, pr)?;
        }
        let _ = fs::set_permissions(dst, m.permissions());
        pr.items_done.fetch_add(1, Ordering::Relaxed);
        if flags & MOVE != 0 {
            // Un fichier ignoré y reste : le dossier source n'est alors pas vide.
            let _ = fs::remove_dir(src);
        }
        return Ok(());
    }
    if present {
        if same_file(src, dst) || flags & SKIP != 0 {
            pr.add(if m.is_file() { m.len() } else { 0 })?;
            pr.items_done.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        if flags & REPLACE == 0 {
            return Err(named(io::ErrorKind::AlreadyExists.into(), dst));
        }
        if fs::symlink_metadata(dst)?.is_dir() {
            fs::remove_dir_all(dst).map_err(|e| named(e, dst))?;
        }
    }
    if m.file_type().is_symlink() {
        let target = fs::read_link(src).map_err(|e| named(e, src))?;
        if exists(dst) {
            fs::remove_file(dst).map_err(|e| named(e, dst))?;
        }
        std::os::unix::fs::symlink(target, dst).map_err(|e| named(e, dst))?;
    } else if m.is_file() {
        copy_file(src, dst, &m, pr)?;
    } else {
        // Tubes, sockets, périphériques : Windows ne les connaît pas.
        pr.items_done.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    pr.items_done.fetch_add(1, Ordering::Relaxed);
    if flags & MOVE != 0 {
        fs::remove_file(src).map_err(|e| named(e, src))?;
    }
    Ok(())
}

/// Copie ou déplace chaque paire ; message de fin vide.
pub fn run(pairs: &[(PathBuf, PathBuf)], flags: u32, pr: &Progress) -> Res<String> {
    if flags & START_PAUSED != 0 {
        pr.pause.store(true, Ordering::Relaxed);
    }
    let (mut bytes, mut items) = (0u64, 0u64);
    for (src, _) in pairs {
        let (b, i) = measure(src, 0, pr).map_err(|e| match e {
            ArcError::Io(e) => named(e, src),
            e => e,
        })?;
        bytes += b;
        items += i;
    }
    pr.set_total(bytes.max(1));
    pr.items_total.store(items, Ordering::Relaxed);
    for (src, dst) in pairs {
        pr.check()?;
        if let Some(n) = src.file_name() {
            pr.set_current(&n.to_string_lossy());
        }
        if flags & MOVE != 0 && !exists(dst) && fs::rename(src, dst).is_ok() {
            // Même disque : renommage instantané.
            let (b, i) = measure(dst, 0, pr).unwrap_or((0, 1));
            pr.add(b)?;
            pr.items_done.fetch_add(i, Ordering::Relaxed);
            continue;
        }
        let created = !exists(dst);
        match copy_item(src, dst, flags, 0, pr) {
            Ok(()) => {}
            Err(ArcError::Cancelled) => return Err(ArcError::Cancelled),
            Err(e) => {
                if created && flags & MOVE == 0 {
                    let _ = if fs::symlink_metadata(dst)
                        .map(|m| m.is_dir())
                        .unwrap_or(false)
                    {
                        fs::remove_dir_all(dst)
                    } else {
                        fs::remove_file(dst)
                    };
                }
                return Err(e);
            }
        }
    }
    Ok(String::new())
}

/// Ne supprimer que le contenu des dossiers donnés (vider la Corbeille).
pub const CONTENTS: u32 = 1;

fn remove_tree(p: &Path, depth: u32, pr: &Progress) -> Res<()> {
    pr.check()?;
    let m = fs::symlink_metadata(p).map_err(|e| named(e, p))?;
    if m.is_dir() && depth < MAX_DEPTH {
        for e in fs::read_dir(p).map_err(|e| named(e, p))? {
            remove_tree(&e.map_err(|e| named(e, p))?.path(), depth + 1, pr)?;
        }
        fs::remove_dir(p).map_err(|e| named(e, p))?;
    } else {
        fs::remove_file(p).map_err(|e| named(e, p))?;
    }
    pr.items_done.fetch_add(1, Ordering::Relaxed);
    pr.add(1)
}

/// Suppression définitive, élément par élément : l'avancement compte les
/// éléments (octets = éléments). Avec `CONTENTS`, les dossiers donnés
/// restent mais sont vidés.
pub fn delete(paths: &[PathBuf], flags: u32, pr: &Progress) -> Res<String> {
    let mut targets = Vec::new();
    for p in paths {
        if flags & CONTENTS != 0 {
            match fs::read_dir(p) {
                Ok(rd) => targets.extend(rd.filter_map(|e| e.ok()).map(|e| e.path())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(named(e, p)),
            }
        } else {
            targets.push(p.clone());
        }
    }
    let mut items = 0;
    for t in &targets {
        items += measure(t, 0, pr).map(|(_, i)| i).unwrap_or(1);
    }
    pr.set_total(items.max(1));
    pr.items_total.store(items, Ordering::Relaxed);
    for t in &targets {
        if let Some(n) = t.file_name() {
            pr.set_current(&n.to_string_lossy());
        }
        match remove_tree(t, 0, pr) {
            Err(ArcError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {}
            r => r?,
        }
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-copie-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn tree(root: &Path) -> PathBuf {
        let s = root.join("source");
        fs::create_dir_all(s.join("sous")).unwrap();
        fs::write(s.join("a.txt"), vec![7u8; 3 * CHUNK + 5]).unwrap();
        fs::write(s.join("sous").join("b.txt"), b"winlux").unwrap();
        s
    }

    #[test]
    fn copie_un_dossier_avec_progression() {
        let r = scratch("dossier");
        let s = tree(&r);
        let d = r.join("cible");
        let pr = Progress::default();
        run(&[(s.clone(), d.clone())], 0, &pr).unwrap();
        assert_eq!(fs::read(d.join("sous").join("b.txt")).unwrap(), b"winlux");
        assert_eq!(
            fs::metadata(d.join("a.txt")).unwrap().len(),
            3 * CHUNK as u64 + 5
        );
        assert_eq!(pr.permille(), 1000);
        assert_eq!(pr.items_done.load(Ordering::Relaxed), 4);
        assert_eq!(pr.items_total.load(Ordering::Relaxed), 4);
        assert!(s.join("a.txt").exists());
        assert!(!d.join(".~a.txt.winlux-copie").exists());
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn deplacer_supprime_la_source() {
        let r = scratch("deplacer");
        let s = tree(&r);
        let d = r.join("ailleurs");
        run(&[(s.clone(), d.clone())], MOVE, &Progress::default()).unwrap();
        assert!(!s.exists());
        assert_eq!(fs::read(d.join("sous").join("b.txt")).unwrap(), b"winlux");
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn remplacer_ignorer_et_refuser() {
        let r = scratch("conflits");
        let s = tree(&r);
        let d = r.join("cible");
        fs::create_dir_all(d.join("sous")).unwrap();
        fs::write(d.join("sous").join("b.txt"), b"ancien").unwrap();
        fs::write(d.join("garde.txt"), b"x").unwrap();
        // Sans consigne, un fichier existant n'est jamais écrasé.
        assert!(run(&[(s.clone(), d.clone())], 0, &Progress::default()).is_err());
        assert_eq!(fs::read(d.join("sous").join("b.txt")).unwrap(), b"ancien");
        assert!(
            d.join("garde.txt").exists(),
            "un échec ne touche pas un dossier existant"
        );
        run(&[(s.clone(), d.clone())], SKIP, &Progress::default()).unwrap();
        assert_eq!(fs::read(d.join("sous").join("b.txt")).unwrap(), b"ancien");
        assert!(d.join("a.txt").exists());
        run(&[(s.clone(), d.clone())], REPLACE, &Progress::default()).unwrap();
        assert_eq!(fs::read(d.join("sous").join("b.txt")).unwrap(), b"winlux");
        assert!(d.join("garde.txt").exists(), "les dossiers sont fusionnés");
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn deplacer_en_ignorant_garde_la_source_ignoree() {
        let r = scratch("ignorer");
        let s = tree(&r);
        let d = r.join("cible");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("a.txt"), b"deja la").unwrap();
        run(&[(s.clone(), d.clone())], MOVE | SKIP, &Progress::default()).unwrap();
        assert!(
            s.join("a.txt").exists(),
            "le fichier ignoré reste à sa place"
        );
        assert!(!s.join("sous").exists(), "le reste est déplacé");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"deja la");
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn annuler_ne_laisse_pas_de_fichier_tronque() {
        let r = scratch("annuler");
        let s = tree(&r);
        let d = r.join("cible");
        let pr = Progress::default();
        pr.cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            run(&[(s.join("a.txt"), d.clone())], 0, &pr),
            Err(ArcError::Cancelled)
        ));
        assert!(!d.exists());
        assert!(!r.join(".~cible.winlux-copie").exists());
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn vider_garde_les_dossiers() {
        let r = scratch("vider");
        tree(&r);
        let pr = Progress::default();
        delete(&[r.clone(), r.join("absent")], CONTENTS, &pr).unwrap();
        assert!(r.is_dir());
        assert_eq!(fs::read_dir(&r).unwrap().count(), 0);
        assert_eq!(pr.items_done.load(Ordering::Relaxed), 4);
        assert_eq!(pr.permille(), 1000);
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn pause_puis_reprise() {
        let r = scratch("pause");
        let s = tree(&r);
        let d = r.join("cible");
        let pr = Progress::default();
        pr.pause.store(true, Ordering::Relaxed);
        let p2 = pr.clone();
        let (s2, d2) = (s.clone(), d.clone());
        let t = std::thread::spawn(move || run(&[(s2, d2)], 0, &p2).map(|_| ()));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            pr.items_done.load(Ordering::Relaxed),
            0,
            "rien ne bouge en pause"
        );
        pr.pause.store(false, Ordering::Relaxed);
        t.join().unwrap().unwrap();
        assert!(d.join("a.txt").exists());
        let _ = fs::remove_dir_all(&r);
    }
}
