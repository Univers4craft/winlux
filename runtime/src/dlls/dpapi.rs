//! Protection de données (DPAPI) : CryptProtectData, CryptUnprotectData,
//! CryptProtectMemory, CryptUnprotectMemory.
//!
//! Comme sous Windows, un blob protégé ne se déchiffre que pour le même
//! utilisateur : la clé maîtresse est un secret aléatoire de 32 octets dans
//! `~/.config/winlux/dpapi/master.key` (droits 0600), créé au premier usage.
//! Avec CRYPTPROTECT_LOCAL_MACHINE, la clé vient de /etc/machine-id : tout
//! compte de la machine peut alors déchiffrer, ce qui est le sens de ce
//! drapeau. Le format du blob est propre à Winlux (un blob venu d'un vrai
//! Windows ne se déchiffre pas, il dépend des secrets du compte Windows) :
//!
//! `WLXDPAPI` | version (u32) | drapeaux (u32) | longueur de la description
//! en octets (u32) | description UTF-16 | IV (16) | AES-256-CBC (PKCS#7) |
//! HMAC-SHA256 de tout ce qui précède (32).
//!
//! Les clés de chiffrement et d'authentification sont tirées de la clé
//! maîtresse et de l'entropie facultative de l'appelant par HMAC-SHA256.
//! AES et HMAC passent par l'OpenSSL de l'hôte ([`super::hostssl`]).

#![allow(non_snake_case)]

use crate::process::set_last_error;
use std::path::PathBuf;
use std::sync::OnceLock;

const MAGIC: &[u8; 8] = b"WLXDPAPI";
const VERSION: u32 = 1;
const CRYPTPROTECT_LOCAL_MACHINE: u32 = 0x4;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_DATA: u32 = 13;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const NTE_BAD_DATA: u32 = 0x8009_0005;
const CRYPTPROTECTMEMORY_BLOCK_SIZE: usize = 16;

#[repr(C)]
pub struct DataBlob {
    pub cb: u32,
    pub pb: *mut u8,
}

fn config_dir() -> PathBuf {
    // Dossier imposé (tests).
    if let Some(d) = std::env::var_os("WINLUX_DPAPI_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(d).join("winlux");
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config/winlux"))
        .unwrap_or_else(|| std::env::temp_dir().join("winlux"))
}

fn random(n: usize) -> Option<Vec<u8>> {
    let mut v = vec![0u8; n];
    let r = unsafe { super::bcrypt::bcrypt_gen_random(0, v.as_mut_ptr(), n as u32, 2) };
    (r == 0).then_some(v)
}

/// Clé maîtresse de l'utilisateur, créée à la première demande.
fn user_master() -> Option<Vec<u8>> {
    let dir = config_dir().join("dpapi");
    let path = dir.join("master.key");
    if let Ok(k) = std::fs::read(&path) {
        if k.len() == 32 {
            return Some(k);
        }
    }
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .ok()?;
    let key = random(32)?;
    // Écriture atomique : un fichier temporaire, puis renommage. Si un autre
    // processus a gagné la course, sa clé est relue.
    let tmp = dir.join(format!("master.key.{}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .ok()?;
    f.write_all(&key).ok()?;
    drop(f);
    if std::fs::hard_link(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return std::fs::read(&path).ok().filter(|k| k.len() == 32);
    }
    let _ = std::fs::remove_file(&tmp);
    Some(key)
}

/// Clé de la machine (CRYPTPROTECT_LOCAL_MACHINE).
fn machine_master() -> Option<Vec<u8>> {
    let id = std::fs::read("/etc/machine-id")
        .or_else(|_| std::fs::read("/var/lib/dbus/machine-id"))
        .ok()?;
    super::hostssl::hmac(c"SHA256", b"winlux-dpapi-machine", id.trim_ascii())
}

/// (clé AES-256, clé HMAC) pour ces drapeaux et cette entropie.
fn keys(flags: u32, entropy: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let master = if flags & CRYPTPROTECT_LOCAL_MACHINE != 0 {
        machine_master()?
    } else {
        user_master()?
    };
    let derive =
        |label: &[u8]| super::hostssl::hmac(c"SHA256", &master, &[label, entropy].concat());
    Some((derive(b"enc")?, derive(b"mac")?))
}

/// Chiffre `data` ; `desc` est gardé en clair dans le blob, comme sous Windows.
pub fn protect(data: &[u8], desc: &[u16], entropy: &[u8], flags: u32) -> Option<Vec<u8>> {
    let (enc, mac) = keys(flags, entropy)?;
    let iv = random(16)?;
    let pad = 16 - data.len() % 16;
    let mut body = data.to_vec();
    body.resize(data.len() + pad, pad as u8);
    if !super::hostssl::cipher(c"AES-256-CBC", &enc, &iv, true, &mut body) {
        return None;
    }
    let mut out = Vec::with_capacity(28 + desc.len() * 2 + 16 + body.len() + 32);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(flags & CRYPTPROTECT_LOCAL_MACHINE).to_le_bytes());
    out.extend_from_slice(&((desc.len() * 2) as u32).to_le_bytes());
    for w in desc {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&iv);
    out.extend_from_slice(&body);
    let tag = super::hostssl::hmac(c"SHA256", &mac, &out)?;
    out.extend_from_slice(&tag);
    Some(out)
}

/// Déchiffre un blob de `protect` : (données, description), ou None si le
/// blob est abîmé, d'un autre compte ou d'une autre entropie.
pub fn unprotect(blob: &[u8], entropy: &[u8]) -> Option<(Vec<u8>, Vec<u16>)> {
    let u32_at = |o: usize| {
        blob.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if blob.len() < 20 + 16 + 16 + 32 || &blob[..8] != MAGIC || u32_at(8)? != VERSION {
        return None;
    }
    let flags = u32_at(12)?;
    let desc_len = u32_at(16)? as usize;
    let body_start = 20 + desc_len + 16;
    let tag_start = blob.len() - 32;
    if !desc_len.is_multiple_of(2) || body_start > tag_start || !(tag_start - body_start).is_multiple_of(16) {
        return None;
    }
    let (enc, mac) = keys(flags, entropy)?;
    let tag = super::hostssl::hmac(c"SHA256", &mac, &blob[..tag_start])?;
    // Comparaison en temps constant.
    let diff = tag
        .iter()
        .zip(&blob[tag_start..])
        .fold(0u8, |d, (a, b)| d | (a ^ b));
    if diff != 0 || tag_start == body_start {
        return None;
    }
    let iv = &blob[20 + desc_len..body_start];
    let mut body = blob[body_start..tag_start].to_vec();
    if !super::hostssl::cipher(c"AES-256-CBC", &enc, iv, false, &mut body) {
        return None;
    }
    let pad = *body.last()? as usize;
    if pad == 0 || pad > 16 || body[body.len() - pad..].iter().any(|&b| b as usize != pad) {
        return None;
    }
    body.truncate(body.len() - pad);
    let desc = blob[20..20 + desc_len]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some((body, desc))
}

unsafe fn blob_bytes<'a>(b: *const DataBlob) -> &'a [u8] {
    if b.is_null() || (*b).pb.is_null() || (*b).cb == 0 {
        &[]
    } else {
        std::slice::from_raw_parts((*b).pb, (*b).cb as usize)
    }
}

unsafe fn local_copy(bytes: &[u8]) -> *mut u8 {
    let p = super::kernel32_misc::local_alloc(0, bytes.len().max(1)) as *mut u8;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    }
    p
}

pub unsafe extern "win64" fn CryptProtectData(
    input: *const DataBlob,
    desc: *const u16,
    entropy: *const DataBlob,
    _reserved: usize,
    _prompt: usize,
    flags: u32,
    output: *mut DataBlob,
) -> i32 {
    if input.is_null() || output.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let desc: Vec<u16> = if desc.is_null() {
        Vec::new()
    } else {
        super::wstr(desc).encode_utf16().collect()
    };
    let Some(blob) = protect(blob_bytes(input), &desc, blob_bytes(entropy), flags) else {
        set_last_error(NTE_BAD_DATA);
        return 0;
    };
    let p = local_copy(&blob);
    if p.is_null() {
        set_last_error(ERROR_NOT_ENOUGH_MEMORY);
        return 0;
    }
    (*output).pb = p;
    (*output).cb = blob.len() as u32;
    1
}

pub unsafe extern "win64" fn CryptUnprotectData(
    input: *const DataBlob,
    desc_out: *mut *mut u16,
    entropy: *const DataBlob,
    _reserved: usize,
    _prompt: usize,
    _flags: u32,
    output: *mut DataBlob,
) -> i32 {
    if input.is_null() || output.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some((data, desc)) = unprotect(blob_bytes(input), blob_bytes(entropy)) else {
        set_last_error(ERROR_INVALID_DATA);
        return 0;
    };
    let p = local_copy(&data);
    if p.is_null() {
        set_last_error(ERROR_NOT_ENOUGH_MEMORY);
        return 0;
    }
    if !desc_out.is_null() {
        let mut w = desc;
        w.push(0);
        let bytes: Vec<u8> = w.iter().flat_map(|c| c.to_le_bytes()).collect();
        *desc_out = local_copy(&bytes) as *mut u16;
    }
    (*output).pb = p;
    (*output).cb = data.len() as u32;
    1
}

/// Clé de la session pour CryptProtectMemory (tirée une fois par processus ;
/// CRYPTPROTECTMEMORY_CROSS_PROCESS et SAME_LOGON reprennent la clé
/// de l'utilisateur).
fn memory_key(flags: u32) -> Option<Vec<u8>> {
    static PROCESS: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    if flags & 0x3 != 0 {
        let master = user_master()?;
        return super::hostssl::hmac(c"SHA256", &master, b"memory");
    }
    PROCESS.get_or_init(|| random(32)).clone()
}

unsafe fn protect_memory(data: *mut u8, len: u32, flags: u32, enc: bool) -> i32 {
    let n = len as usize;
    if data.is_null() || !n.is_multiple_of(CRYPTPROTECTMEMORY_BLOCK_SIZE) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(key) = memory_key(flags) else {
        set_last_error(NTE_BAD_DATA);
        return 0;
    };
    let buf = std::slice::from_raw_parts_mut(data, n);
    if super::hostssl::cipher(c"AES-256-CBC", &key, &[0u8; 16], enc, buf) {
        1
    } else {
        set_last_error(NTE_BAD_DATA);
        0
    }
}

pub unsafe extern "win64" fn CryptProtectMemory(data: *mut u8, len: u32, flags: u32) -> i32 {
    protect_memory(data, len, flags, true)
}

pub unsafe extern "win64" fn CryptUnprotectMemory(data: *mut u8, len: u32, flags: u32) -> i32 {
    protect_memory(data, len, flags, false)
}

/// RtlEncryptMemory / RtlDecryptMemory (advapi32 SystemFunction040/041) :
/// rendent un NTSTATUS.
pub unsafe extern "win64" fn rtl_encrypt_memory(data: *mut u8, len: u32, flags: u32) -> u32 {
    if protect_memory(data, len, flags, true) != 0 {
        0
    } else {
        0xC000_000D
    }
}

pub unsafe extern "win64" fn rtl_decrypt_memory(data: *mut u8, len: u32, flags: u32) -> u32 {
    if protect_memory(data, len, flags, false) != 0 {
        0
    } else {
        0xC000_000D
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CryptProtectData" => fn CryptProtectData,
        "CryptUnprotectData" => fn CryptUnprotectData,
        "CryptProtectMemory" => fn CryptProtectMemory,
        "CryptUnprotectMemory" => fn CryptUnprotectMemory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_entropie_et_alteration() {
        if super::super::hostssl::ssl().is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("winlux-dpapi-{}", std::process::id()));
        std::env::set_var("WINLUX_DPAPI_DIR", &dir);
        let _teb = crate::process::test_teb();
        unsafe {
            let secret = b"mot de passe enregistre".to_vec();
            let input = DataBlob {
                cb: secret.len() as u32,
                pb: secret.as_ptr() as *mut u8,
            };
            let ent = b"sel";
            let entropy = DataBlob {
                cb: 3,
                pb: ent.as_ptr() as *mut u8,
            };
            let desc: Vec<u16> = "Compte\0".encode_utf16().collect();
            let mut out = DataBlob {
                cb: 0,
                pb: std::ptr::null_mut(),
            };
            assert_eq!(
                CryptProtectData(&input, desc.as_ptr(), &entropy, 0, 0, 0, &mut out),
                1
            );
            assert!(out.cb as usize > secret.len());
            let key = dir.join("dpapi/master.key");
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&key).unwrap().permissions().mode() & 0o777,
                0o600
            );

            let mut back = DataBlob {
                cb: 0,
                pb: std::ptr::null_mut(),
            };
            let mut d: *mut u16 = std::ptr::null_mut();
            assert_eq!(
                CryptUnprotectData(&out, &mut d, &entropy, 0, 0, 0, &mut back),
                1
            );
            assert_eq!(blob_bytes(&back), secret.as_slice());
            assert_eq!(super::super::wstr(d), "Compte");
            super::super::kernel32_misc::local_free(d.cast());
            super::super::kernel32_misc::local_free(back.pb.cast());

            // Sans la bonne entropie, ou blob modifié : refusé.
            assert_eq!(
                CryptUnprotectData(
                    &out,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    &mut back
                ),
                0
            );
            *out.pb.add(40) ^= 1;
            assert_eq!(
                CryptUnprotectData(&out, std::ptr::null_mut(), &entropy, 0, 0, 0, &mut back),
                0
            );
            super::super::kernel32_misc::local_free(out.pb.cast());

            // Clé de la machine, données vides.
            let empty = DataBlob {
                cb: 0,
                pb: std::ptr::null_mut(),
            };
            if machine_master().is_some() {
                assert_eq!(
                    CryptProtectData(
                        &empty,
                        std::ptr::null(),
                        std::ptr::null(),
                        0,
                        0,
                        4,
                        &mut out
                    ),
                    1
                );
                assert_eq!(
                    CryptUnprotectData(
                        &out,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                        0,
                        0,
                        0,
                        &mut back
                    ),
                    1
                );
                assert_eq!(back.cb, 0);
            }

            // CryptProtectMemory : multiple de 16, aller-retour en place.
            let mut mem = *b"0123456789abcdef0123456789abcdef";
            assert_eq!(CryptProtectMemory(mem.as_mut_ptr(), 32, 0), 1);
            assert_ne!(&mem, b"0123456789abcdef0123456789abcdef");
            assert_eq!(CryptUnprotectMemory(mem.as_mut_ptr(), 32, 0), 1);
            assert_eq!(&mem, b"0123456789abcdef0123456789abcdef");
            assert_eq!(CryptProtectMemory(mem.as_mut_ptr(), 10, 0), 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
