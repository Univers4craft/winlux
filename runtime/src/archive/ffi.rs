//! Exports `kernel32` privés de Winlux pour l'Explorer : archives, images
//! disque et suivi des tâches de fond. Toutes les chaînes sont en UTF-16.

use super::jobs;
use super::Format;
use crate::dlls::wstr;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

unsafe fn opt_password(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = wstr(p);
    (!s.is_empty()).then_some(s)
}

unsafe fn linux(p: *const u16) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let s = wstr(p);
    (!s.is_empty()).then(|| crate::paths::to_linux(&s))
}

pub const CAN_READ: u32 = 1;
pub const CAN_WRITE: u32 = 2;
pub const CAN_MOUNT: u32 = 4;
pub const CAN_MOUNT_RW: u32 = 8;
pub const IS_OPEN: u32 = 16;

/// `WinluxArchiveSupport(chemin)` : ce que l'on peut faire de ce fichier.
pub unsafe extern "win64" fn winlux_archive_support(path: *const u16) -> u32 {
    let Some(p) = linux(path) else {
        return 0;
    };
    let mut flags = 0;
    if let Some(f) = Format::from_name(&p.to_string_lossy()) {
        let (r, w) = f.support();
        if r {
            flags |= CAN_READ;
        }
        if w {
            flags |= CAN_WRITE;
        }
        if r && super::cache_valid(&std::fs::canonicalize(&p).unwrap_or(p.clone())) {
            flags |= IS_OPEN;
        }
    } else if let Some(rw) = crate::diskimage::support(&p) {
        flags |= CAN_MOUNT;
        if rw {
            flags |= CAN_MOUNT_RW;
        }
    }
    flags
}

/// Formats que l'on peut créer : bit 0 zip, 1 7z, 2 tar, 3 tar.gz,
/// 4 tar.xz, 5 tar.bz2, 6 tar.zst.
pub extern "win64" fn winlux_archive_formats() -> u32 {
    let list = [
        Format::Zip,
        Format::SevenZ,
        Format::Tar,
        Format::TarGz,
        Format::TarXz,
        Format::TarBz2,
        Format::TarZst,
    ];
    list.iter()
        .enumerate()
        .filter(|(_, f)| f.support().1)
        .fold(0, |acc, (i, _)| acc | 1 << i)
}

/// Déploie l'archive pour y naviguer. Message de fin : chemin Windows.
pub unsafe extern "win64" fn winlux_archive_open(path: *const u16, pw: *const u16) -> u32 {
    let Some(p) = linux(path) else {
        return 0;
    };
    let win = wstr(path);
    let pw = opt_password(pw);
    jobs::spawn(move |pr| {
        super::open(&p, pw.as_deref(), pr)?;
        Ok(win)
    })
}

/// Extrait l'archive dans `dest` (rendu unique ; NULL = dossier du nom de
/// l'archive à côté d'elle). Message : dossier créé.
pub unsafe extern "win64" fn winlux_archive_extract(
    path: *const u16,
    dest: *const u16,
    pw: *const u16,
) -> u32 {
    let Some(p) = linux(path) else {
        return 0;
    };
    // Sans destination : dossier portant le nom de l'archive, à côté d'elle.
    // Avec destination (« Extraire ici ») : dans ce dossier même. Avant, la
    // destination passait par unique_path et l'archive partait dans un
    // nouveau « Téléchargements (2) » à côté, invisible depuis l'Explorateur.
    let here = linux(dest);
    let pw = opt_password(pw);
    jobs::spawn(move |pr| {
        let Some(dir) = here else {
            let target = super::unique_path(&p.with_file_name(super::default_folder_name(&p)));
            let r = super::extract_to(&p, &target, pw.as_deref(), pr);
            if r.is_err() {
                let _ = std::fs::remove_dir_all(&target);
            }
            r?;
            return Ok(crate::paths::to_windows(&target));
        };
        extract_here(&p, &dir, pw.as_deref(), pr)?;
        Ok(crate::paths::to_windows(&dir))
    })
}

/// « Extraire ici » : dossier de travail caché, puis chaque élément est
/// déplacé dans `dir`. Un fichier déjà présent n'est jamais écrasé (« nom
/// (2) » comme Windows) et un échec ne touche pas au dossier de
/// l'utilisateur.
fn extract_here(
    archive: &std::path::Path,
    dir: &std::path::Path,
    pw: Option<&str>,
    pr: &super::Progress,
) -> super::Res<()> {
    let staging = super::unique_path(&dir.join(".winlux-extraction"));
    let r = super::extract_to(archive, &staging, pw, pr).and_then(|()| {
        for entry in std::fs::read_dir(&staging)? {
            let entry = entry?;
            std::fs::rename(
                entry.path(),
                free_name(dir, &entry.file_name().to_string_lossy()),
            )?;
        }
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&staging);
    r
}

/// `dossier/nom.ext`, ou `nom (2).ext`, `nom (3).ext`… si la place est prise.
fn free_name(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = dir.join(name);
    if std::fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    (2..10_000)
        .map(|k| dir.join(format!("{stem} ({k}){ext}")))
        .find(|p| std::fs::symlink_metadata(p).is_err())
        .unwrap_or(first)
}

/// Crée `out` (format selon l'extension) depuis une liste de sources
/// séparées par des zéros et terminée par un double zéro.
pub unsafe extern "win64" fn winlux_archive_create(
    out: *const u16,
    sources: *const u16,
    pw: *const u16,
) -> u32 {
    let Some(o) = linux(out) else {
        return 0;
    };
    if sources.is_null() {
        return 0;
    }
    let mut list = Vec::new();
    let mut p = sources;
    loop {
        let s = wstr(p);
        if s.is_empty() {
            break;
        }
        p = p.add(s.encode_utf16().count() + 1);
        list.push(crate::paths::to_linux(&s));
    }
    let pw = opt_password(pw);
    jobs::spawn(move |pr| {
        let target = super::unique_path(&o);
        let r = super::create(&target, &list, pw.as_deref(), pr);
        if r.is_err() {
            let _ = std::fs::remove_file(&target);
        }
        r?;
        Ok(crate::paths::to_windows(&target))
    })
}

/// Reconstruit les archives traversées par `path` si leur contenu a changé
/// (de l'intérieur vers l'extérieur). Renvoie 0 s'il n'y a rien à faire.
pub unsafe extern "win64" fn winlux_archive_commit(path: *const u16) -> u32 {
    if path.is_null() {
        return 0;
    }
    let p = crate::paths::to_linux_raw(&wstr(path));
    let chain = super::chain(&p);
    if chain.is_empty() {
        return 0;
    }
    jobs::spawn(move |pr| {
        let mut changed = false;
        for a in chain.iter().rev() {
            changed |= super::commit(a, pr)?;
        }
        Ok(if changed { "1".into() } else { String::new() })
    })
}

/// Monte une image disque. Message : lettres attribuées (« E: F: »).
pub unsafe extern "win64" fn winlux_image_mount(path: *const u16) -> u32 {
    let Some(p) = linux(path) else {
        return 0;
    };
    jobs::spawn(move |pr| crate::diskimage::mount(&p, pr))
}

/// Éjecte le lecteur `letter` (et les autres partitions de la même image).
pub extern "win64" fn winlux_image_eject(letter: u32) -> u32 {
    let Some(c) = char::from_u32(letter) else {
        return 0;
    };
    jobs::spawn(move |_| crate::diskimage::eject(c.to_ascii_uppercase()))
}

/// État d'une tâche : 0 en cours, 1 réussie, 2 échec, 3 mot de passe requis,
/// 4 mot de passe incorrect, 5 lecture seule, 6 non pris en charge,
/// 7 annulée, 0xFFFFFFFF inconnue. `progress` reçoit l'avancement en ‰ et
/// `msg` le message (chemin, lettres ou erreur).
pub unsafe extern "win64" fn winlux_job_poll(
    id: u32,
    progress: *mut u32,
    msg: *mut u16,
    cap: u32,
) -> u32 {
    let Some(j) = jobs::get(id) else {
        return u32::MAX;
    };
    let st = j.status.load(Ordering::Acquire);
    if !progress.is_null() {
        *progress = if st == jobs::DONE {
            1000
        } else {
            j.progress.permille()
        };
    }
    if !msg.is_null() && cap > 0 {
        let m = j.message.lock().unwrap_or_else(|e| e.into_inner());
        let w: Vec<u16> = m.encode_utf16().take(cap as usize - 1).collect();
        std::ptr::copy_nonoverlapping(w.as_ptr(), msg, w.len());
        *msg.add(w.len()) = 0;
    }
    st
}

/// `WinluxFileCopy(liste, options)` : copie ou déplace en tâche de fond.
/// `liste` alterne source et destination (chemins Windows), chacune
/// terminée par NUL, la liste par un NUL de plus. Options : 1 déplacer,
/// 2 remplacer les fichiers existants, 4 les ignorer, 8 démarrer en pause.
pub unsafe extern "win64" fn winlux_file_copy(list: *const u16, flags: u32) -> u32 {
    if list.is_null() {
        return 0;
    }
    let mut paths = Vec::new();
    let mut p = list;
    while *p != 0 {
        let Some(l) = linux(p) else {
            return 0;
        };
        paths.push(l);
        p = p.add(crate::printf::wide_len(p) + 1);
    }
    if paths.is_empty() || paths.len() % 2 != 0 {
        return 0;
    }
    let pairs: Vec<(PathBuf, PathBuf)> = paths
        .chunks(2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect();
    jobs::spawn(move |pr| super::copy::run(&pairs, flags, pr))
}

/// `WinluxFileDelete(liste, options)` : suppression définitive en tâche de
/// fond (liste de chemins Windows terminés par NUL, plus un NUL final).
/// Option 1 : ne vider que le contenu des dossiers (Vider la Corbeille).
pub unsafe extern "win64" fn winlux_file_delete(list: *const u16, flags: u32) -> u32 {
    if list.is_null() {
        return 0;
    }
    let mut paths = Vec::new();
    let mut p = list;
    while *p != 0 {
        let Some(l) = linux(p) else {
            return 0;
        };
        paths.push(l);
        p = p.add(crate::printf::wide_len(p) + 1);
    }
    if paths.is_empty() {
        return 0;
    }
    jobs::spawn(move |pr| super::copy::delete(&paths, flags, pr))
}

/// Met en pause (`on` ≠ 0) ou relance une tâche.
pub extern "win64" fn winlux_job_pause(id: u32, on: u32) {
    if let Some(j) = jobs::get(id) {
        j.progress.pause.store(on != 0, Ordering::Relaxed);
    }
}

/// Détails d'une tâche : `out[0..4]` reçoit octets faits, octets à faire,
/// éléments faits, éléments à faire ; `name` l'élément en cours. Renvoie 1
/// si la tâche est en pause, 0 sinon, 0xFFFFFFFF si elle est inconnue.
pub unsafe extern "win64" fn winlux_job_stats(
    id: u32,
    out: *mut u64,
    name: *mut u16,
    cap: u32,
) -> u32 {
    let Some(j) = jobs::get(id) else {
        return u32::MAX;
    };
    let pr = &j.progress;
    if !out.is_null() {
        *out = pr.done.load(Ordering::Relaxed);
        *out.add(1) = pr.total.load(Ordering::Relaxed);
        *out.add(2) = pr.items_done.load(Ordering::Relaxed);
        *out.add(3) = pr.items_total.load(Ordering::Relaxed);
    }
    if !name.is_null() && cap > 0 {
        let m = pr.current.lock().unwrap_or_else(|e| e.into_inner());
        let w: Vec<u16> = m.encode_utf16().take(cap as usize - 1).collect();
        std::ptr::copy_nonoverlapping(w.as_ptr(), name, w.len());
        *name.add(w.len()) = 0;
    }
    pr.pause.load(Ordering::Relaxed) as u32
}

pub extern "win64" fn winlux_job_cancel(id: u32) {
    if let Some(j) = jobs::get(id) {
        j.progress.cancel.store(true, Ordering::Relaxed);
    }
}

pub extern "win64" fn winlux_job_close(id: u32) {
    jobs::close(id);
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxArchiveSupport" => fn winlux_archive_support,
        "WinluxArchiveFormats" => fn winlux_archive_formats,
        "WinluxArchiveOpen" => fn winlux_archive_open,
        "WinluxArchiveExtract" => fn winlux_archive_extract,
        "WinluxArchiveCreate" => fn winlux_archive_create,
        "WinluxArchiveCommit" => fn winlux_archive_commit,
        "WinluxImageMount" => fn winlux_image_mount,
        "WinluxImageEject" => fn winlux_image_eject,
        "WinluxJobPoll" => fn winlux_job_poll,
        "WinluxJobCancel" => fn winlux_job_cancel,
        "WinluxJobClose" => fn winlux_job_close,
        "WinluxJobPause" => fn winlux_job_pause,
        "WinluxJobStats" => fn winlux_job_stats,
        "WinluxFileCopy" => fn winlux_file_copy,
        "WinluxFileDelete" => fn winlux_file_delete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-ffi-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn free_name_keeps_the_extension_like_windows() {
        let d = tmp("free");
        assert_eq!(free_name(&d, "Everything.lng"), d.join("Everything.lng"));
        std::fs::write(d.join("Everything.lng"), "x").unwrap();
        assert_eq!(
            free_name(&d, "Everything.lng"),
            d.join("Everything (2).lng")
        );
        std::fs::write(d.join("Everything (2).lng"), "x").unwrap();
        assert_eq!(
            free_name(&d, "Everything.lng"),
            d.join("Everything (3).lng")
        );
        std::fs::create_dir(d.join("Projet")).unwrap();
        assert_eq!(free_name(&d, "Projet"), d.join("Projet (2)"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Campagne v0.2.13 : « Extraire ici » partait dans un dossier
    /// « Téléchargements (2) » voisin et pouvait écraser un fichier.
    #[test]
    fn extract_here_goes_into_the_folder_without_overwriting() {
        let d = tmp("here");
        let src = d.join("src");
        std::fs::create_dir_all(src.join("sous")).unwrap();
        std::fs::write(src.join("été.txt"), "archive").unwrap();
        std::fs::write(src.join("sous/b.txt"), "b").unwrap();
        let arc = d.join("a.zip");
        let entries = [src.join("été.txt"), src.join("sous")];
        super::super::create(&arc, &entries, None, &super::super::Progress::default()).unwrap();
        let dest = d.join("Téléchargements");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("été.txt"), "à moi").unwrap();
        extract_here(&arc, &dest, None, &super::super::Progress::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("été.txt")).unwrap(),
            "à moi"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("été (2).txt")).unwrap(),
            "archive"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("sous/b.txt")).unwrap(),
            "b"
        );
        assert!(!d.join("Téléchargements (2)").exists());
        let hidden = std::fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(".winlux"));
        assert!(!hidden, "dossier de travail laissé en place");
        // Archive illisible : le dossier de l'utilisateur reste intact.
        std::fs::write(d.join("abime.zip"), "pas une archive").unwrap();
        assert!(extract_here(
            &d.join("abime.zip"),
            &dest,
            None,
            &super::super::Progress::default()
        )
        .is_err());
        assert!(dest.join("été.txt").exists() && Path::new(&dest).is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }
}
