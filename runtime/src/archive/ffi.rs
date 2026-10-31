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
    let d = linux(dest).unwrap_or_else(|| p.with_file_name(super::default_folder_name(&p)));
    let pw = opt_password(pw);
    jobs::spawn(move |pr| {
        let target = super::unique_path(&d);
        let r = super::extract_to(&p, &target, pw.as_deref(), pr);
        if r.is_err() {
            let _ = std::fs::remove_dir_all(&target);
        }
        r?;
        Ok(crate::paths::to_windows(&target))
    })
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
    }
}
