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

/// CREDENTIALW d'un programme 64 bits (wincred.h de mingw-w64, domaine
/// public). FILETIME n'est aligné que sur 4 octets, d'où `[u32; 2]`.
#[repr(C)]
#[allow(dead_code)]
struct CredentialW64 {
    flags: u32,
    kind: u32,
    target: *const u16,
    comment: *const u16,
    written: [u32; 2],
    blob_size: u32,
    blob: *const u8,
    persist: u32,
    attribute_count: u32,
    attributes: *const u8,
    target_alias: *const u16,
    user: *const u16,
}

/// La même structure vue par un programme 32 bits : pointeurs de 4 octets
/// (#2955).
#[repr(C)]
#[allow(dead_code)]
struct CredentialW32 {
    flags: u32,
    kind: u32,
    target: u32,
    comment: u32,
    written: [u32; 2],
    blob_size: u32,
    blob: u32,
    persist: u32,
    attribute_count: u32,
    attributes: u32,
    target_alias: u32,
    user: u32,
}

/// Décalages de CREDENTIALW pour une taille de pointeur donnée.
struct Layout {
    ptr: usize,
    kind: usize,
    target: usize,
    comment: usize,
    written: usize,
    blob_size: usize,
    blob: usize,
    persist: usize,
    user: usize,
    size: usize,
}

macro_rules! layout_of {
    ($t:ty, $ptr:expr) => {
        Layout {
            ptr: $ptr,
            kind: std::mem::offset_of!($t, kind),
            target: std::mem::offset_of!($t, target),
            comment: std::mem::offset_of!($t, comment),
            written: std::mem::offset_of!($t, written),
            blob_size: std::mem::offset_of!($t, blob_size),
            blob: std::mem::offset_of!($t, blob),
            persist: std::mem::offset_of!($t, persist),
            user: std::mem::offset_of!($t, user),
            size: std::mem::size_of::<$t>(),
        }
    };
}

/// Disposition pour des pointeurs de `ptr` octets (4 en x86, 8 sinon).
fn layout(ptr: usize) -> Layout {
    if ptr == 4 {
        layout_of!(CredentialW32, 4)
    } else {
        layout_of!(CredentialW64, 8)
    }
}

/// Disposition du programme appelant.
fn caller_layout() -> Layout {
    layout(crate::wow32::pointer_size())
}

/// Lit un pointeur du programme sur `ptr` octets.
unsafe fn get_ptr(at: *const u8, ptr: usize) -> usize {
    if ptr == 4 {
        (at as *const u32).read_unaligned() as usize
    } else {
        (at as *const u64).read_unaligned() as usize
    }
}

/// Écrit un pointeur du programme sur `ptr` octets.
unsafe fn put_ptr(at: *mut u8, v: usize, ptr: usize) {
    if ptr == 4 {
        (at as *mut u32).write_unaligned(v as u32)
    } else {
        (at as *mut u64).write_unaligned(v as u64)
    }
}

/// Type, cible et contenu d'un CREDENTIALW ; `None` si la cible manque.
unsafe fn read_credential(cred: *const u8, lay: &Layout) -> Option<(u32, String, Stored)> {
    let kind = (cred.add(lay.kind) as *const u32).read_unaligned();
    let text = |at: usize| wide_opt(get_ptr(cred.add(at), lay.ptr) as *const u16);
    let target = text(lay.target);
    if target.is_empty() {
        return None;
    }
    let size = (cred.add(lay.blob_size) as *const u32).read_unaligned() as usize;
    let blob_ptr = get_ptr(cred.add(lay.blob), lay.ptr) as *const u8;
    let blob = if blob_ptr.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(blob_ptr, size).to_vec()
    };
    let stored = Stored {
        comment: text(lay.comment),
        persist: (cred.add(lay.persist) as *const u32).read_unaligned(),
        user: text(lay.user),
        blob,
        written: 0,
    };
    Some((kind, target, stored))
}

/// Octets d'un CREDENTIALW et de ses chaînes, arrondis à 8.
fn credential_len(lay: &Layout, target: &str, s: &Stored) -> usize {
    let n = lay.size
        + wide_bytes(target).len()
        + wide_bytes(&s.comment).len()
        + wide_bytes(&s.user).len()
        + s.blob.len();
    (n + 7) & !7
}

/// Remplit en `c` (mis à zéro, `credential_len` octets) un CREDENTIALW
/// suivi de ses chaînes et de son blob.
unsafe fn fill_credential(c: *mut u8, lay: &Layout, target: &str, kind: u32, s: &Stored) {
    let mut cur = c.add(lay.size);
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
    (c.add(lay.kind) as *mut u32).write_unaligned(kind);
    put_ptr(c.add(lay.target), pt as usize, lay.ptr);
    put_ptr(c.add(lay.comment), pc as usize, lay.ptr);
    (c.add(lay.written) as *mut u64).write_unaligned(s.written);
    (c.add(lay.blob_size) as *mut u32).write_unaligned(s.blob.len() as u32);
    let blob = if s.blob.is_empty() { 0 } else { pb as usize };
    put_ptr(c.add(lay.blob), blob, lay.ptr);
    (c.add(lay.persist) as *mut u32).write_unaligned(s.persist);
    put_ptr(c.add(lay.user), pu as usize, lay.ptr);
}

/// `CredWriteW(PCREDENTIALW, flags)`.
pub unsafe extern "win64" fn CredWriteW(cred: *const u8, _flags: u32) -> i32 {
    if cred.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some((kind, target, stored)) = read_credential(cred, &caller_layout()) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
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
    let lay = caller_layout();
    let block = libc::calloc(1, credential_len(&lay, &name, &s) + 8) as *mut u8;
    if block.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    fill_credential(block, &lay, &name, kind, &s);
    put_ptr(out as *mut u8, block as usize, lay.ptr);
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
unsafe fn credential_array(list: &[(String, u32, Stored)], lay: &Layout) -> Option<*mut u8> {
    let head = (list.len() * lay.ptr + 7) & !7;
    let sizes: Vec<usize> = list
        .iter()
        .map(|(t, _, s)| credential_len(lay, t, s))
        .collect();
    let block = libc::calloc(1, head + sizes.iter().sum::<usize>() + 8) as *mut u8;
    if block.is_null() {
        return None;
    }
    let mut at = block.add(head);
    for (i, ((target, kind, s), size)) in list.iter().zip(&sizes).enumerate() {
        put_ptr(block.add(i * lay.ptr), at as usize, lay.ptr);
        fill_credential(at, lay, target, *kind, s);
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
    let lay = caller_layout();
    let Some(block) = credential_array(&list, &lay) else {
        crate::process::set_last_error(8);
        return 0;
    };
    *count = list.len() as u32;
    put_ptr(out as *mut u8, block as usize, lay.ptr);
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
    let lay = caller_layout();
    // TargetName est le premier champ de CREDENTIAL_TARGET_INFORMATIONW.
    let target = wide_opt(get_ptr(info, lay.ptr) as *const u16);
    let list: Vec<_> = stored_all()
        .into_iter()
        .filter(|(t, k, _)| (2..=4).contains(k) && t.eq_ignore_ascii_case(&target))
        .collect();
    if list.is_empty() {
        crate::process::set_last_error(ERROR_NOT_FOUND);
        return 0;
    }
    let Some(block) = credential_array(&list, &lay) else {
        crate::process::set_last_error(8);
        return 0;
    };
    *count = list.len() as u32;
    put_ptr(out as *mut u8, block as usize, lay.ptr);
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

/// Octets d'une structure à marshaler, lue avec des pointeurs de `ptr`
/// octets. CertCredential (1) : CERT_CREDENTIAL_INFO, empreinte de 20
/// octets ; UsernameTargetCredential (2) : USERNAME_TARGET_CREDENTIAL_INFO,
/// un pointeur vers le nom ; BinaryBlobCredential (3) : un bloc d'octets.
/// La structure du type 3 n'est décrite ni par la page sdk-api de
/// CRED_MARSHAL_TYPE ni par wincred.h de mingw-w64 : elle n'est lue qu'en
/// x64, où Winlux l'a toujours servie ainsi, et refusée en x86 plutôt que
/// devinée (#2959).
unsafe fn marshal_input(kind: u32, cred: *const u8, ptr: usize) -> Option<Vec<u8>> {
    match kind {
        1 => Some(std::slice::from_raw_parts(cred.add(4), 20).to_vec()),
        2 => Some(wide_opt(get_ptr(cred, ptr) as *const u16).into_bytes()),
        3 if ptr == 8 => {
            let n = (cred as *const u32).read_unaligned() as usize;
            let p = get_ptr(cred.add(8), 8) as *const u8;
            Some(if p.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p, n).to_vec()
            })
        }
        _ => None,
    }
}

pub unsafe extern "win64" fn CredMarshalCredentialW(
    kind: u32,
    cred: *const u8,
    out: *mut *mut u16,
) -> i32 {
    if cred.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let ptr = crate::wow32::pointer_size();
    let Some(data) = marshal_input(kind, cred, ptr) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let w = wide_bytes(&marshal(kind, &data));
    let p = libc::calloc(1, w.len()) as *mut u8;
    if p.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    put_ptr(out as *mut u8, p as usize, ptr);
    1
}

/// Structure rendue par CredUnmarshalCredentialW pour le type `k`, avec
/// des pointeurs de `ptr` octets, en un bloc libéré par CredFree. Le type 3
/// est refusé en x86 (voir `marshal_input`).
unsafe fn unmarshal_output(k: u32, data: &[u8], ptr: usize) -> Option<*mut u8> {
    let block: Vec<u8> = match k {
        1 if data.len() == 20 => {
            let mut b = 24u32.to_le_bytes().to_vec();
            b.extend_from_slice(data);
            b
        }
        2 => {
            // Pointeur vers le nom placé juste après.
            let mut b = vec![0u8; ptr];
            b.extend(wide_bytes(&String::from_utf8_lossy(data)));
            b
        }
        3 if ptr == 8 => {
            let mut b = (data.len() as u32).to_le_bytes().to_vec();
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&[0; 8]);
            b.extend_from_slice(data);
            b
        }
        _ => return None,
    };
    let p = libc::calloc(1, block.len().max(8)) as *mut u8;
    if p.is_null() {
        return None;
    }
    std::ptr::copy_nonoverlapping(block.as_ptr(), p, block.len());
    match k {
        2 => put_ptr(p, p.add(ptr) as usize, ptr),
        3 => put_ptr(p.add(8), p.add(16) as usize, 8),
        _ => {}
    }
    Some(p)
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
    let ptr = crate::wow32::pointer_size();
    let Some((k, data)) = unmarshal(&wide_opt(text)) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let Some(p) = unmarshal_output(k, &data, ptr) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    *kind = k;
    put_ptr(out as *mut u8, p as usize, ptr);
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

    /// Décalages de wincred.h (mingw-w64) : x86 à gauche, x64 à droite.
    #[test]
    fn dispositions_credentialw_2955() {
        let (a, b) = (layout(4), layout(8));
        let x86 = [a.kind, a.target, a.comment, a.written, a.blob_size, a.blob];
        let x64 = [b.kind, b.target, b.comment, b.written, b.blob_size, b.blob];
        assert_eq!(x86, [4, 8, 12, 16, 24, 28]);
        assert_eq!(x64, [4, 8, 16, 24, 32, 40]);
        assert_eq!([a.persist, a.user, a.size], [32, 48, 52]);
        assert_eq!([b.persist, b.user, b.size], [48, 72, 80]);
    }

    fn sample() -> Stored {
        Stored {
            user: "damien".into(),
            comment: "note".into(),
            blob: b"jeton secret".to_vec(),
            persist: 2,
            written: 0x0123_4567_89ab_cdef,
        }
    }

    /// Écrit un CREDENTIALW dans `mem` puis le relit, avec la disposition
    /// `ptr` : le programme doit retrouver chaque champ à sa place.
    unsafe fn aller_retour(mem: *mut u8, ptr: usize) {
        let lay = layout(ptr);
        let s = sample();
        fill_credential(mem, &lay, "winlux:test", 1, &s);
        let blob_size = (mem.add(lay.blob_size) as *const u32).read_unaligned();
        let persist = (mem.add(lay.persist) as *const u32).read_unaligned();
        let written = (mem.add(lay.written) as *const u64).read_unaligned();
        assert_eq!(blob_size, 12);
        assert_eq!(persist, 2);
        assert_eq!(written, s.written);
        let user = get_ptr(mem.add(lay.user), ptr) as *const u16;
        assert_eq!(crate::dlls::wstr(user), "damien");
        let (kind, target, back) = read_credential(mem, &lay).unwrap();
        assert_eq!((kind, target.as_str()), (1, "winlux:test"));
        assert_eq!(
            (back.user.as_str(), back.comment.as_str()),
            ("damien", "note")
        );
        assert_eq!((back.blob, back.persist), (s.blob, 2));
    }

    #[test]
    fn credentialw_x64_2955() {
        let lay = layout(8);
        let mut mem = vec![0u64; credential_len(&lay, "winlux:test", &sample()) / 8 + 1];
        unsafe { aller_retour(mem.as_mut_ptr() as *mut u8, 8) };
    }

    /// Un programme 32 bits n'a que des adresses sous 4 Gio : la mémoire du
    /// test est prise sous 2 Gio (MAP_32BIT) pour que ses pointeurs tiennent
    /// sur 4 octets.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn credentialw_x86_2955() {
        unsafe {
            let mem = libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            );
            assert_ne!(mem, libc::MAP_FAILED);
            assert!((mem as usize) < 1 << 32);
            aller_retour(mem as *mut u8, 4);
            // Rien n'est écrit au-delà des 52 octets de la structure x86
            // avant ses chaînes : la cible commence juste après.
            let target = get_ptr((mem as *const u8).add(8), 4);
            assert_eq!(target, mem as usize + 52);
            libc::munmap(mem, 4096);
        }
    }

    /// Le tableau de CredEnumerateW : des pointeurs de 4 octets en x86.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn tableau_credentialw_x86_2955() {
        unsafe {
            let list = vec![
                ("a".to_string(), 1, sample()),
                ("b".to_string(), 1, sample()),
            ];
            let lay = layout(4);
            // calloc rend ici une adresse 64 bits : on ne vérifie que le pas
            // du tableau et la place de chaque structure.
            let block = credential_array(&list, &lay).unwrap();
            let size = credential_len(&lay, "a", &list[0].2);
            let first = (block as *const u32).read_unaligned() as usize;
            let second = (block.add(4) as *const u32).read_unaligned() as usize;
            assert_eq!(first, (block as usize + 8) & 0xffff_ffff);
            assert_eq!(second, (block as usize + 8 + size) & 0xffff_ffff);
            let kind = (block.add(8 + size + lay.kind) as *const u32).read_unaligned();
            assert_eq!(kind, 1);
            libc::free(block as *mut libc::c_void);
        }
    }

    /// USERNAME_TARGET_CREDENTIAL_INFO d'un programme 32 bits : un seul
    /// pointeur de 4 octets, suivi ici d'octets parasites qui ne doivent
    /// pas être lus (#2959).
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn marshal_nom_x86_2959() {
        unsafe {
            let mem = libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            ) as *mut u8;
            assert_ne!(mem as *mut libc::c_void, libc::MAP_FAILED);
            let name = wide_bytes("damien");
            std::ptr::copy_nonoverlapping(name.as_ptr(), mem.add(64), name.len());
            (mem as *mut u32).write_unaligned(mem.add(64) as usize as u32);
            (mem.add(4) as *mut u32).write_unaligned(0xdead_beef);
            let data = marshal_input(2, mem, 4).unwrap();
            assert_eq!(data, b"damien");
            libc::munmap(mem as *mut libc::c_void, 4096);
        }
    }

    /// Le bloc rendu en x86 : le nom commence à +4, désigné par un pointeur
    /// de 4 octets.
    #[test]
    fn unmarshal_nom_x86_2959() {
        unsafe {
            let p = unmarshal_output(2, b"damien", 4).unwrap();
            let at = (p as *const u32).read_unaligned() as usize;
            assert_eq!(at, (p as usize + 4) & 0xffff_ffff);
            assert_eq!(crate::dlls::wstr(p.add(4) as *const u16), "damien");
            libc::free(p as *mut libc::c_void);
            let p = unmarshal_output(2, b"damien", 8).unwrap();
            assert_eq!(get_ptr(p, 8), p as usize + 8);
            libc::free(p as *mut libc::c_void);
        }
    }

    /// Type 3 non documenté : refusé en x86 plutôt que deviné, servi en x64.
    #[test]
    fn blob_refuse_en_x86_2959() {
        unsafe {
            assert!(unmarshal_output(3, b"abc", 4).is_none());
            let blob = *b"abc";
            let mut info = [0u8; 16];
            info[..4].copy_from_slice(&3u32.to_le_bytes());
            info[8..].copy_from_slice(&(blob.as_ptr() as u64).to_le_bytes());
            assert!(marshal_input(3, info.as_ptr(), 4).is_none());
            assert_eq!(marshal_input(3, info.as_ptr(), 8).unwrap(), b"abc");
            let p = unmarshal_output(3, b"abc", 8).unwrap();
            assert_eq!(
                std::slice::from_raw_parts(get_ptr(p.add(8), 8) as *const u8, 3),
                b"abc"
            );
            libc::free(p as *mut libc::c_void);
        }
    }
}
