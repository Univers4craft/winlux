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

// ── Lot D du relevé (#1512) ────────────────────────────────────────────

/// Identifiants rangés : (cible, type, contenu).
fn stored_all() -> Vec<(String, u32, Stored)> {
    let Some(dir) = store_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some() {
            continue;
        }
        let file = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(kind) = file.split('-').next().and_then(|k| k.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(target) = std::fs::read_to_string(p.with_extension("cible")) else {
            continue;
        };
        let written = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| (d.as_secs() + 11_644_473_600) * 10_000_000)
            .unwrap_or(0);
        if let Some(s) = std::fs::read(&p).ok().and_then(|raw| decode(&raw, written)) {
            out.push((target, kind, s));
        }
    }
    out.sort_by_key(|a| a.0.to_lowercase());
    out
}

fn wide_bytes(t: &str) -> Vec<u8> {
    t.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

/// Un tableau de PCREDENTIALW et les CREDENTIALW qu'il désigne, en un
/// seul bloc libéré par CredFree.
unsafe fn credential_array(list: &[(String, u32, Stored)]) -> Option<*mut u8> {
    let head = list.len() * 8;
    let sizes: Vec<usize> = list
        .iter()
        .map(|(t, _, s)| {
            let n = 80
                + wide_bytes(t).len()
                + wide_bytes(&s.comment).len()
                + wide_bytes(&s.user).len()
                + s.blob.len();
            (n + 7) & !7
        })
        .collect();
    let block = libc::calloc(1, head + sizes.iter().sum::<usize>() + 8) as *mut u8;
    if block.is_null() {
        return None;
    }
    let mut at = block.add(head);
    for (i, ((target, kind, s), size)) in list.iter().zip(&sizes).enumerate() {
        let c = at;
        (block.add(i * 8) as *mut *mut u8).write_unaligned(c);
        let mut cur = c.add(80);
        let mut place = |data: &[u8]| -> *mut u8 {
            let p = cur;
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            cur = cur.add(data.len());
            p
        };
        let pt = place(&wide_bytes(target));
        let pc = place(&wide_bytes(&s.comment));
        let pu = place(&wide_bytes(&s.user));
        let pb = place(&s.blob);
        (c.add(4) as *mut u32).write_unaligned(*kind);
        (c.add(8) as *mut *mut u8).write_unaligned(pt);
        (c.add(16) as *mut *mut u8).write_unaligned(pc);
        (c.add(24) as *mut u64).write_unaligned(s.written);
        (c.add(32) as *mut u32).write_unaligned(s.blob.len() as u32);
        (c.add(40) as *mut *mut u8).write_unaligned(if s.blob.is_empty() {
            std::ptr::null_mut()
        } else {
            pb
        });
        (c.add(48) as *mut u32).write_unaligned(s.persist);
        (c.add(72) as *mut *mut u8).write_unaligned(pu);
        at = at.add(*size);
    }
    Some(block)
}

/// Filtre de CredEnumerate : un nom, éventuellement terminé par « * ».
fn filter_matches(filter: &str, target: &str) -> bool {
    let (f, t) = (filter.to_lowercase(), target.to_lowercase());
    match f.strip_suffix('*') {
        Some(prefix) => t.starts_with(prefix),
        None => f == t,
    }
}

/// `CredEnumerateW(filter, flags, count*, PCREDENTIALW**)`.
pub unsafe extern "win64" fn CredEnumerateW(
    filter: *const u16,
    flags: u32,
    count: *mut u32,
    out: *mut *mut u8,
) -> i32 {
    if count.is_null() || out.is_null() || (flags & 1 != 0 && !filter.is_null()) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let wanted = wide_opt(filter);
    let list: Vec<_> = stored_all()
        .into_iter()
        .filter(|(t, _, _)| wanted.is_empty() || filter_matches(&wanted, t))
        .collect();
    if list.is_empty() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    let Some(block) = credential_array(&list) else {
        crate::process::set_last_error(8);
        return 0;
    };
    *count = list.len() as u32;
    *out = block;
    1
}

/// Chaque type d'identifiant se garde sur la machine locale
/// (CRED_PERSIST_LOCAL_MACHINE).
pub unsafe extern "win64" fn CredGetSessionTypes(max: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    for i in 0..max as usize {
        *out.add(i) = if i == 0 { 0 } else { 2 };
    }
    1
}

/// Pas de domaine : aucune information de cible n'est connue.
pub extern "win64" fn CredGetTargetInfoW(_target: *const u16, _flags: u32, _out: usize) -> i32 {
    crate::process::set_last_error(ERROR_NOT_FOUND);
    0
}

/// CredReadDomainCredentialsW : les identifiants de domaine (types 2 à 4)
/// rangés pour TargetName.
pub unsafe extern "win64" fn CredReadDomainCredentialsW(
    info: *const u8,
    _flags: u32,
    count: *mut u32,
    out: *mut *mut u8,
) -> i32 {
    if info.is_null() || count.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let target = wide_opt((info as *const *const u16).read_unaligned());
    let list: Vec<_> = stored_all()
        .into_iter()
        .filter(|(t, k, _)| (2..=4).contains(k) && t.eq_ignore_ascii_case(&target))
        .collect();
    if list.is_empty() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    let Some(block) = credential_array(&list) else {
        crate::process::set_last_error(8);
        return 0;
    };
    *count = list.len() as u32;
    *out = block;
    1
}

pub unsafe extern "win64" fn CredWriteDomainCredentialsW(
    info: *const u8,
    cred: *const u8,
    flags: u32,
) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    CredWriteW(cred, flags)
}

/// CredRenameW : la même entrée sous un autre nom de cible.
pub unsafe extern "win64" fn CredRenameW(
    old: *const u16,
    new: *const u16,
    kind: u32,
    _flags: u32,
) -> i32 {
    let (from, to) = (wide_opt(old), wide_opt(new));
    if from.is_empty() || to.is_empty() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (Some(a), Some(b)) = (file_for(&from, kind), file_for(&to, kind)) else {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    };
    if !a.exists() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    if b.exists() {
        crate::process::set_last_error(183); // ERROR_ALREADY_EXISTS
        return 0;
    }
    if std::fs::rename(&a, &b).is_err() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    let _ = std::fs::remove_file(a.with_extension("cible"));
    let _ = std::fs::write(b.with_extension("cible"), to.as_bytes());
    1
}

/// Forme « marshalée » : « @@ », une lettre de type, puis les octets en
/// hexadécimal. Le format est propre à Winlux (la documentation le dit
/// opaque) ; CredUnmarshalCredential le relit.
pub fn marshal(kind: u32, data: &[u8]) -> String {
    let letter = (b'A' + kind as u8) as char;
    let mut s = format!("@@{letter}");
    for b in data {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

pub fn unmarshal(text: &str) -> Option<(u32, Vec<u8>)> {
    let rest = text.strip_prefix("@@")?;
    let mut chars = rest.chars();
    let kind = (chars.next()? as u32).checked_sub('A' as u32)?;
    let hex = chars.as_str();
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    Some((kind, bytes))
}

pub unsafe extern "win64" fn CredIsMarshaledCredentialW(text: *const u16) -> i32 {
    i32::from(unmarshal(&wide_opt(text)).is_some_and(|(k, _)| (1..=3).contains(&k)))
}

/// CertCredential (1) : CERT_CREDENTIAL_INFO, empreinte de 20 octets ;
/// UsernameTargetCredential (2) : un nom d'utilisateur ;
/// BinaryBlobCredential (3) : un bloc d'octets.
pub unsafe extern "win64" fn CredMarshalCredentialW(
    kind: u32,
    cred: *const u8,
    out: *mut *mut u16,
) -> i32 {
    if cred.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let data: Vec<u8> = match kind {
        1 => std::slice::from_raw_parts(cred.add(4), 20).to_vec(),
        2 => wide_opt((cred as *const *const u16).read_unaligned()).into_bytes(),
        3 => {
            let n = (cred as *const u32).read_unaligned() as usize;
            let p = (cred.add(8) as *const *const u8).read_unaligned();
            if p.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p, n).to_vec()
            }
        }
        _ => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
    };
    let w = wide_bytes(&marshal(kind, &data));
    let p = libc::calloc(1, w.len()) as *mut u8;
    if p.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p as *mut u16;
    1
}

pub unsafe extern "win64" fn CredUnmarshalCredentialW(
    text: *const u16,
    kind: *mut u32,
    out: *mut *mut u8,
) -> i32 {
    if kind.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some((k, data)) = unmarshal(&wide_opt(text)) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let block: Vec<u8> = match k {
        1 if data.len() == 20 => {
            let mut b = 24u32.to_le_bytes().to_vec();
            b.extend_from_slice(&data);
            b
        }
        2 => {
            // Pointeur vers le nom placé juste après.
            let mut b = vec![0u8; 8];
            b.extend(wide_bytes(&String::from_utf8_lossy(&data)));
            b
        }
        3 => {
            let mut b = (data.len() as u32).to_le_bytes().to_vec();
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&[0; 8]);
            b.extend_from_slice(&data);
            b
        }
        _ => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
    };
    let p = libc::calloc(1, block.len().max(8)) as *mut u8;
    if p.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    std::ptr::copy_nonoverlapping(block.as_ptr(), p, block.len());
    match k {
        2 => (p as *mut *mut u8).write_unaligned(p.add(8)),
        3 => (p.add(8) as *mut *mut u8).write_unaligned(p.add(16)),
        _ => {}
    }
    *kind = k;
    *out = p;
    1
}

pub fn releve_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CredEnumerateW" => fn CredEnumerateW,
        "CredGetSessionTypes" => fn CredGetSessionTypes,
        "CredGetTargetInfoW" => fn CredGetTargetInfoW,
        "CredReadDomainCredentialsW" => fn CredReadDomainCredentialsW,
        "CredWriteDomainCredentialsW" => fn CredWriteDomainCredentialsW,
        "CredRenameW" => fn CredRenameW,
        "CredIsMarshaledCredentialW" => fn CredIsMarshaledCredentialW,
        "CredMarshalCredentialW" => fn CredMarshalCredentialW,
        "CredUnmarshalCredentialW" => fn CredUnmarshalCredentialW,
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
