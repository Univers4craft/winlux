//! Gestionnaire d'identifiants Windows (advapi32 Cred*W).
//!
//! Les programmes y rangent leurs jetons de connexion : Claude Code y garde
//! son jeton OAuth, d'autres clients leurs mots de passe. Winlux les range
//! dans `~/.local/share/winlux/identifiants/`, un fichier par cible, lisible
//! par le seul compte (0600, dossier 0700), comme le coffre de session
//! Windows qui est lié au compte de l'utilisateur.
#![allow(non_snake_case)]

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

const ERROR_NOT_FOUND: u32 = 1168;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_SUCH_LOGON_SESSION: u32 = 1312;

fn store_dir() -> Option<std::path::PathBuf> {
    let d = crate::paths::data_dir()?.join("identifiants");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&d)
        .ok()?;
    Some(d)
}

/// Nom de fichier sûr pour une cible : type + nom en hexadécimal.
fn file_for(target: &str, kind: u32) -> Option<std::path::PathBuf> {
    let hex: String = target
        .to_lowercase()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    Some(store_dir()?.join(format!("{kind}-{hex}")))
}

struct Stored {
    user: String,
    comment: String,
    blob: Vec<u8>,
    persist: u32,
    written: u64,
}

fn encode(s: &Stored) -> Vec<u8> {
    let mut out = Vec::new();
    for part in [s.user.as_bytes(), s.comment.as_bytes(), &s.blob[..]] {
        out.extend_from_slice(&(part.len() as u32).to_le_bytes());
        out.extend_from_slice(part);
    }
    out.extend_from_slice(&s.persist.to_le_bytes());
    out
}

fn decode(raw: &[u8], written: u64) -> Option<Stored> {
    let mut pos = 0usize;
    let mut take = || -> Option<Vec<u8>> {
        let n = u32::from_le_bytes(raw.get(pos..pos + 4)?.try_into().ok()?) as usize;
        let v = raw.get(pos + 4..pos + 4 + n)?.to_vec();
        pos += 4 + n;
        Some(v)
    };
    let user = String::from_utf8(take()?).ok()?;
    let comment = String::from_utf8(take()?).ok()?;
    let blob = take()?;
    let persist = raw
        .get(pos..pos + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(2);
    Some(Stored {
        user,
        comment,
        blob,
        persist,
        written,
    })
}

unsafe fn wide_opt(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(p)
    }
}

/// `CredWriteW(PCREDENTIALW, flags)`.
pub unsafe extern "win64" fn CredWriteW(cred: *const u8, _flags: u32) -> i32 {
    if cred.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let kind = (cred.add(4) as *const u32).read_unaligned();
    let target = wide_opt((cred.add(8) as *const *const u16).read_unaligned());
    if target.is_empty() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let size = (cred.add(32) as *const u32).read_unaligned() as usize;
    let blob_ptr = (cred.add(40) as *const *const u8).read_unaligned();
    let blob = if blob_ptr.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(blob_ptr, size).to_vec()
    };
    let stored = Stored {
        comment: wide_opt((cred.add(16) as *const *const u16).read_unaligned()),
        persist: (cred.add(48) as *const u32).read_unaligned(),
        user: wide_opt((cred.add(72) as *const *const u16).read_unaligned()),
        blob,
        written: 0,
    };
    let Some(path) = file_for(&target, kind) else {
        crate::process::set_last_error(ERROR_NO_SUCH_LOGON_SESSION);
        return 0;
    };
    let tmp = path.with_extension("tmp");
    let ok = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(&encode(&stored)).and_then(|_| f.sync_all()))
        .and_then(|_| std::fs::rename(&tmp, &path))
        .is_ok();
    if !ok {
        crate::process::set_last_error(ERROR_NO_SUCH_LOGON_SESSION);
        return 0;
    }
    // Nom de la cible conservé à côté pour CredEnumerate et le diagnostic.
    let _ = std::fs::write(path.with_extension("cible"), target.as_bytes());
    1
}

/// `CredReadW(target, type, flags, PCREDENTIALW*)` : un seul bloc alloué,
/// rendu par CredFree.
pub unsafe extern "win64" fn CredReadW(
    target: *const u16,
    kind: u32,
    _flags: u32,
    out: *mut *mut u8,
) -> i32 {
    if target.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let name = crate::dlls::wstr(target);
    let Some(path) = file_for(&name, kind) else {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    };
    let written = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs() + 11_644_473_600) * 10_000_000)
        .unwrap_or(0);
    let Some(s) = std::fs::read(&path)
        .ok()
        .and_then(|raw| decode(&raw, written))
    else {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    };
    let wide = |t: &str| -> Vec<u16> { t.encode_utf16().chain(std::iter::once(0)).collect() };
    let (wt, wc, wu) = (wide(&name), wide(&s.comment), wide(&s.user));
    let total = 80 + (wt.len() + wc.len() + wu.len()) * 2 + s.blob.len() + 8;
    let block = libc::calloc(1, total) as *mut u8;
    if block.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    let mut cur = block.add(80);
    let mut place = |data: &[u8]| -> *mut u8 {
        let p = cur;
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        cur = cur.add(data.len());
        p
    };
    let as_bytes =
        |w: &[u16]| std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2).to_vec();
    let pt = place(&as_bytes(&wt));
    let pc = place(&as_bytes(&wc));
    let pu = place(&as_bytes(&wu));
    let pb = place(&s.blob);
    (block.add(4) as *mut u32).write_unaligned(kind);
    (block.add(8) as *mut *mut u8).write_unaligned(pt);
    (block.add(16) as *mut *mut u8).write_unaligned(pc);
    (block.add(24) as *mut u64).write_unaligned(s.written);
    (block.add(32) as *mut u32).write_unaligned(s.blob.len() as u32);
    (block.add(40) as *mut *mut u8).write_unaligned(if s.blob.is_empty() {
        std::ptr::null_mut()
    } else {
        pb
    });
    (block.add(48) as *mut u32).write_unaligned(s.persist);
    (block.add(72) as *mut *mut u8).write_unaligned(pu);
    *out = block;
    1
}

pub unsafe extern "win64" fn CredDeleteW(target: *const u16, kind: u32, _flags: u32) -> i32 {
    if target.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(path) = file_for(&crate::dlls::wstr(target), kind) else {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    };
    if std::fs::remove_file(&path).is_err() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    let _ = std::fs::remove_file(path.with_extension("cible"));
    1
}

pub unsafe extern "win64" fn CredFree(p: *mut u8) {
    if !p.is_null() {
        libc::free(p as *mut libc::c_void);
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CredWriteW" => fn CredWriteW,
        "CredReadW" => fn CredReadW,
        "CredDeleteW" => fn CredDeleteW,
        "CredFree" => fn CredFree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_du_coffre() {
        let s = Stored {
            user: "damien".into(),
            comment: "".into(),
            blob: vec![1, 2, 3],
            persist: 2,
            written: 0,
        };
        let d = decode(&encode(&s), 5).unwrap();
        assert_eq!(d.user, "damien");
        assert_eq!(d.blob, vec![1, 2, 3]);
        assert_eq!(d.persist, 2);
    }
}
