//! AES de CNG (#1627) : poignées d'algorithme « AES », clés symétriques
//! (BCryptGenerateSymmetricKey, BCryptImportKey/ExportKey en KeyDataBlob),
//! modes CBC, ECB et CFB, bourrage PKCS #7 (BCRYPT_BLOCK_PADDING). Le
//! chiffrement passe par l'OpenSSL de l'hôte (hostssl::cipher).
//!
//! D'après la documentation publique de bcrypt.h (learn.microsoft.com) :
//! BCRYPT_KEY_DATA_BLOB_HEADER { dwMagic = 0x4d42444b, dwVersion = 1,
//! cbKeyData } suivi de la clé ; propriétés ChainingMode, BlockLength,
//! KeyLength, KeyLengths, ObjectLength, MessageBlockLength. GCM et CCM ne
//! sont pas encore servis et répondent STATUS_NOT_SUPPORTED.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_DATA_ERROR: i32 = 0xC000_003Eu32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
const STATUS_INVALID_BUFFER_SIZE: i32 = 0xC000_0206u32 as i32;
const BCRYPT_BLOCK_PADDING: u32 = 1;
const KEY_DATA_BLOB_MAGIC: u32 = 0x4d42_444b;
const BLOCK: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Cbc,
    Ecb,
    Cfb,
    Gcm,
    Ccm,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Cbc => "ChainingModeCBC",
            Mode::Ecb => "ChainingModeECB",
            Mode::Cfb => "ChainingModeCFB",
            Mode::Gcm => "ChainingModeGCM",
            Mode::Ccm => "ChainingModeCCM",
        }
    }
    fn parse(s: &str) -> Option<Mode> {
        Some(match s {
            "ChainingModeCBC" => Mode::Cbc,
            "ChainingModeECB" => Mode::Ecb,
            "ChainingModeCFB" => Mode::Cfb,
            "ChainingModeGCM" => Mode::Gcm,
            "ChainingModeCCM" => Mode::Ccm,
            _ => return None,
        })
    }
}

#[derive(Clone)]
struct Settings {
    mode: Mode,
    /// MessageBlockLength : 1 (CFB8, défaut de CNG) ou 16 (CFB128).
    message_block: u32,
}

#[derive(Clone)]
struct Key {
    settings: Settings,
    secret: Vec<u8>,
}

#[derive(Default)]
struct State {
    algs: HashMap<usize, Settings>,
    keys: HashMap<usize, Key>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

static SEQ: AtomicUsize = AtomicUsize::new(1);
/// Poignées d'algorithme et de clé AES : 0x4… et 0x2…, sous 4 Gio pour
/// les programmes 32 bits, distinctes des poignées de hachage (0x8…).
const ALG_TAG: usize = 0x4000_0000;
const KEY_TAG: usize = 0x2000_0000;

fn next(tag: usize) -> usize {
    tag | ((SEQ.fetch_add(1, Ordering::Relaxed) & 0xF_FFFF) << 4)
}

pub(super) fn open() -> usize {
    let h = next(ALG_TAG);
    state().algs.insert(
        h,
        Settings {
            mode: Mode::Cbc,
            message_block: 1,
        },
    );
    h
}

pub(super) fn close(h: usize) -> bool {
    state().algs.remove(&h).is_some()
}

unsafe fn wide(p: *const u8, len: u32) -> String {
    let n = (len as usize) / 2;
    let units: Vec<u16> = (0..n)
        .map(|i| std::ptr::read_unaligned((p as *const u16).add(i)))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// BCryptSetProperty sur une poignée d'algorithme AES ou une clé.
pub(super) unsafe extern "win64" fn set_property(
    h: usize,
    prop: *const u16,
    input: *const u8,
    len: u32,
    _flags: u32,
) -> i32 {
    if prop.is_null() || input.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let name = crate::dlls::wstr(prop);
    let mut st = state();
    let st = &mut *st;
    let settings = match (st.algs.get_mut(&h), st.keys.get_mut(&h)) {
        (Some(s), _) => s,
        (_, Some(k)) => &mut k.settings,
        _ => {
            crate::trace!("BCryptSetProperty({name}) : poignée 0x{h:x} inconnue");
            return STATUS_INVALID_HANDLE;
        }
    };
    match name.as_str() {
        "ChainingMode" => match Mode::parse(&wide(input, len)) {
            Some(m) => {
                settings.mode = m;
                0
            }
            None => STATUS_NOT_SUPPORTED,
        },
        "MessageBlockLength" if len >= 4 => {
            let v = std::ptr::read_unaligned(input as *const u32);
            if v == 1 || v == 16 {
                settings.message_block = v;
                0
            } else {
                STATUS_INVALID_PARAMETER
            }
        }
        _ => {
            crate::trace!("BCryptSetProperty : propriété « {name} » non servie");
            STATUS_NOT_SUPPORTED
        }
    }
}

unsafe fn put_bytes(bytes: &[u8], out: *mut u8, out_len: u32, result: *mut u32) -> i32 {
    *result = bytes.len() as u32;
    if out.is_null() {
        return 0;
    }
    if (out_len as usize) < bytes.len() {
        return STATUS_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    0
}

/// Propriétés d'une poignée AES ; `None` si la poignée n'est pas AES.
pub(super) unsafe fn get_property(
    h: usize,
    name: &str,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
) -> Option<i32> {
    let st = state();
    let (settings, key_bits) = match (st.algs.get(&h), st.keys.get(&h)) {
        (Some(s), _) => (s.clone(), None),
        (_, Some(k)) => (k.settings.clone(), Some(k.secret.len() as u32 * 8)),
        _ => return None,
    };
    drop(st);
    let u32_bytes = |v: u32| v.to_le_bytes().to_vec();
    let bytes: Vec<u8> = match name {
        "BlockLength" => u32_bytes(BLOCK as u32),
        "ObjectLength" | "KeyObjectLength" => u32_bytes(512),
        "MessageBlockLength" => u32_bytes(settings.message_block),
        "KeyLength" => match key_bits {
            Some(b) => u32_bytes(b),
            None => return Some(STATUS_NOT_SUPPORTED),
        },
        // BCRYPT_KEY_LENGTHS_STRUCT { dwMinLength, dwMaxLength, dwIncrement }.
        "KeyLengths" => [128u32, 256, 64]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect(),
        "ChainingMode" => settings
            .mode
            .name()
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect(),
        "AlgorithmName" => "AES"
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect(),
        _ => {
            crate::trace!("BCryptGetProperty : propriété AES « {name} » non servie");
            return Some(STATUS_NOT_SUPPORTED);
        }
    };
    Some(put_bytes(&bytes, out, out_len, result))
}

fn new_key(alg: usize, secret: Vec<u8>) -> Result<usize, i32> {
    if ![16, 24, 32].contains(&secret.len()) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut st = state();
    let Some(settings) = st.algs.get(&alg).cloned() else {
        return Err(STATUS_INVALID_HANDLE);
    };
    let h = next(KEY_TAG);
    st.keys.insert(h, Key { settings, secret });
    Ok(h)
}

/// BCryptGenerateSymmetricKey(hAlg, phKey, pbKeyObject, cbKeyObject,
/// pbSecret, cbSecret, dwFlags).
pub(super) unsafe extern "win64" fn generate_symmetric_key(
    alg: usize,
    out: *mut usize,
    _obj: *mut u8,
    _obj_len: u32,
    secret: *const u8,
    len: u32,
    _flags: u32,
) -> i32 {
    if out.is_null() || secret.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let secret = std::slice::from_raw_parts(secret, len as usize).to_vec();
    match new_key(alg, secret) {
        Ok(h) => {
            super::put_handle(out, h);
            0
        }
        Err(e) => e,
    }
}

/// Clé contenue dans un KeyDataBlob (ou l'OpaqueKeyBlob que rend
/// BCryptExportKey, de même forme).
fn parse_blob(kind: &str, blob: &[u8]) -> Result<Vec<u8>, i32> {
    if kind != "KeyDataBlob" && kind != "OpaqueKeyBlob" {
        crate::trace!("BCryptImportKey : type de blob « {kind} » non servi");
        return Err(STATUS_NOT_SUPPORTED);
    }
    if blob.len() < 12 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let word = |i: usize| u32::from_le_bytes(blob[i..i + 4].try_into().unwrap());
    let (magic, version, n) = (word(0), word(4), word(8) as usize);
    if magic != KEY_DATA_BLOB_MAGIC || version != 1 || blob.len() < 12 + n {
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok(blob[12..12 + n].to_vec())
}

/// BCryptImportKey(hAlg, hImportKey, pszBlobType, phKey, pbKeyObject,
/// cbKeyObject, pbInput, cbInput, dwFlags).
#[allow(clippy::too_many_arguments)]
pub(super) unsafe extern "win64" fn import_key(
    alg: usize,
    import_key: usize,
    kind: *const u16,
    out: *mut usize,
    _obj: *mut u8,
    _obj_len: u32,
    input: *const u8,
    len: u32,
    _flags: u32,
) -> i32 {
    if out.is_null() || kind.is_null() || input.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if import_key != 0 {
        crate::trace!("BCryptImportKey : clé d'emballage non servie");
        return STATUS_NOT_SUPPORTED;
    }
    let blob = std::slice::from_raw_parts(input, len as usize);
    let secret = match parse_blob(&crate::dlls::wstr(kind), blob) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match new_key(alg, secret) {
        Ok(h) => {
            super::put_handle(out, h);
            0
        }
        Err(e) => e,
    }
}

/// BCryptExportKey(hKey, hExportKey, pszBlobType, pbOutput, cbOutput,
/// pcbResult, dwFlags).
pub(super) unsafe extern "win64" fn export_key(
    key: usize,
    export_key: usize,
    kind: *const u16,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
    _flags: u32,
) -> i32 {
    if kind.is_null() || result.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let kind = crate::dlls::wstr(kind);
    if export_key != 0 || (kind != "KeyDataBlob" && kind != "OpaqueKeyBlob") {
        return STATUS_NOT_SUPPORTED;
    }
    let Some(k) = state().keys.get(&key).cloned() else {
        return STATUS_INVALID_HANDLE;
    };
    let mut blob = Vec::with_capacity(12 + k.secret.len());
    blob.extend_from_slice(&KEY_DATA_BLOB_MAGIC.to_le_bytes());
    blob.extend_from_slice(&1u32.to_le_bytes());
    blob.extend_from_slice(&(k.secret.len() as u32).to_le_bytes());
    blob.extend_from_slice(&k.secret);
    put_bytes(&blob, out, out_len, result)
}

pub(super) unsafe extern "win64" fn duplicate_key(
    key: usize,
    out: *mut usize,
    _obj: *mut u8,
    _obj_len: u32,
    _flags: u32,
) -> i32 {
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let mut st = state();
    let Some(k) = st.keys.get(&key).cloned() else {
        return STATUS_INVALID_HANDLE;
    };
    let h = next(KEY_TAG);
    st.keys.insert(h, k);
    super::put_handle(out, h);
    0
}

pub(super) unsafe extern "win64" fn destroy_key(key: usize) -> i32 {
    if state().keys.remove(&key).is_some() {
        0
    } else {
        STATUS_INVALID_HANDLE
    }
}

fn ossl_name(mode: Mode, message_block: u32, key_len: usize) -> Option<std::ffi::CString> {
    let suffix = match mode {
        Mode::Cbc => "CBC",
        Mode::Ecb => "ECB",
        Mode::Cfb if message_block == 16 => "CFB",
        Mode::Cfb => "CFB8",
        Mode::Gcm | Mode::Ccm => return None,
    };
    std::ffi::CString::new(format!("AES-{}-{suffix}", key_len * 8)).ok()
}

/// Chiffre ou déchiffre `data` ; rend le résultat et l'IV suivant.
fn run(key: &Key, iv: &[u8], enc: bool, padding: bool, data: &[u8]) -> Result<Vec<u8>, i32> {
    let Some(name) = ossl_name(
        key.settings.mode,
        key.settings.message_block,
        key.secret.len(),
    ) else {
        crate::trace!(
            "BCryptEncrypt/Decrypt : mode {:?} non servi",
            key.settings.mode
        );
        return Err(STATUS_NOT_SUPPORTED);
    };
    let block_mode = matches!(key.settings.mode, Mode::Cbc | Mode::Ecb);
    let mut buf = data.to_vec();
    if enc && padding {
        let pad = BLOCK - buf.len() % BLOCK;
        buf.extend(std::iter::repeat_n(pad as u8, pad));
    }
    if block_mode && !buf.len().is_multiple_of(BLOCK) {
        return Err(STATUS_INVALID_BUFFER_SIZE);
    }
    let zero = [0u8; BLOCK];
    let iv = if iv.is_empty() { &zero[..] } else { iv };
    if !buf.is_empty() && !crate::dlls::hostssl::cipher(&name, &key.secret, iv, enc, &mut buf) {
        return Err(STATUS_NOT_SUPPORTED);
    }
    if !enc && padding {
        let pad = *buf.last().ok_or(STATUS_DATA_ERROR)? as usize;
        if pad == 0
            || pad > BLOCK
            || pad > buf.len()
            || !buf[buf.len() - pad..].iter().all(|&b| b as usize == pad)
        {
            return Err(STATUS_DATA_ERROR);
        }
        buf.truncate(buf.len() - pad);
    }
    Ok(buf)
}

/// IV suivant en CBC et CFB128 : le dernier bloc chiffré.
fn next_iv(mode: Mode, message_block: u32, cipher_text: &[u8]) -> Option<Vec<u8>> {
    let chained = mode == Mode::Cbc || (mode == Mode::Cfb && message_block == 16);
    (chained && cipher_text.len() >= BLOCK)
        .then(|| cipher_text[cipher_text.len() - BLOCK..].to_vec())
}

#[allow(clippy::too_many_arguments)]
unsafe fn crypt(
    enc: bool,
    key: usize,
    input: *const u8,
    in_len: u32,
    padding_info: usize,
    iv: *mut u8,
    iv_len: u32,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
    flags: u32,
) -> i32 {
    if result.is_null() || (input.is_null() && in_len != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(k) = state().keys.get(&key).cloned() else {
        return STATUS_INVALID_HANDLE;
    };
    if padding_info != 0 && matches!(k.settings.mode, Mode::Cbc | Mode::Ecb | Mode::Cfb) {
        return STATUS_INVALID_PARAMETER;
    }
    let padding = flags & BCRYPT_BLOCK_PADDING != 0;
    let uses_iv = k.settings.mode != Mode::Ecb;
    if uses_iv && !iv.is_null() && iv_len as usize != BLOCK {
        return STATUS_INVALID_PARAMETER;
    }
    let data = if in_len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(input, in_len as usize)
    };
    // Taille demandée seulement : majorant pour le déchiffrement bourré.
    if out.is_null() {
        let n = if enc && padding {
            (data.len() / BLOCK + 1) * BLOCK
        } else {
            data.len()
        };
        *result = n as u32;
        return 0;
    }
    let iv_now = if uses_iv && !iv.is_null() {
        std::slice::from_raw_parts(iv, BLOCK).to_vec()
    } else {
        Vec::new()
    };
    let r = match run(&k, &iv_now, enc, padding, data) {
        Ok(r) => r,
        Err(e) => return e,
    };
    if (out_len as usize) < r.len() {
        *result = r.len() as u32;
        return STATUS_BUFFER_TOO_SMALL;
    }
    if uses_iv && !iv.is_null() {
        let ct = if enc { &r[..] } else { data };
        if let Some(n) = next_iv(k.settings.mode, k.settings.message_block, ct) {
            std::ptr::copy_nonoverlapping(n.as_ptr(), iv, BLOCK);
        }
    }
    std::ptr::copy(r.as_ptr(), out, r.len());
    *result = r.len() as u32;
    0
}

/// BCryptEncrypt(hKey, pbInput, cbInput, pPaddingInfo, pbIV, cbIV,
/// pbOutput, cbOutput, pcbResult, dwFlags).
#[allow(clippy::too_many_arguments)]
pub(super) unsafe extern "win64" fn encrypt(
    key: usize,
    input: *const u8,
    in_len: u32,
    padding_info: usize,
    iv: *mut u8,
    iv_len: u32,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
    flags: u32,
) -> i32 {
    crypt(
        true,
        key,
        input,
        in_len,
        padding_info,
        iv,
        iv_len,
        out,
        out_len,
        result,
        flags,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) unsafe extern "win64" fn decrypt(
    key: usize,
    input: *const u8,
    in_len: u32,
    padding_info: usize,
    iv: *mut u8,
    iv_len: u32,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
    flags: u32,
) -> i32 {
    crypt(
        false,
        key,
        input,
        in_len,
        padding_info,
        iv,
        iv_len,
        out,
        out_len,
        result,
        flags,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    #[test]
    fn aes_cbc_bourre_par_blob_et_duplication_de_hachage() {
        if crate::dlls::hostssl::ssl().is_none() {
            return;
        }
        unsafe {
            let mut alg = 0usize;
            let name = w("AES");
            assert_eq!(
                super::super::bcrypt_open_algorithm_provider(
                    &mut alg,
                    name.as_ptr(),
                    std::ptr::null(),
                    0
                ),
                0
            );
            let mode: Vec<u8> = w("ChainingModeCBC")
                .iter()
                .flat_map(|c| c.to_le_bytes())
                .collect();
            let prop = w("ChainingMode");
            assert_eq!(
                set_property(alg, prop.as_ptr(), mode.as_ptr(), mode.len() as u32, 0),
                0
            );
            // Clé de FIPS-197 / SP 800-38A F.2.1 (AES-128-CBC).
            let key: [u8; 16] = [
                0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
                0x4f, 0x3c,
            ];
            let mut blob = KEY_DATA_BLOB_MAGIC.to_le_bytes().to_vec();
            blob.extend_from_slice(&1u32.to_le_bytes());
            blob.extend_from_slice(&16u32.to_le_bytes());
            blob.extend_from_slice(&key);
            let kind = w("KeyDataBlob");
            let mut hk = 0usize;
            assert_eq!(
                import_key(
                    alg,
                    0,
                    kind.as_ptr(),
                    &mut hk,
                    std::ptr::null_mut(),
                    0,
                    blob.as_ptr(),
                    blob.len() as u32,
                    0
                ),
                0
            );
            let plain: [u8; 16] = [
                0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
                0x17, 0x2a,
            ];
            let mut iv: [u8; 16] = std::array::from_fn(|i| i as u8);
            let mut out = [0u8; 32];
            let mut n = 0u32;
            assert_eq!(
                encrypt(
                    hk,
                    plain.as_ptr(),
                    16,
                    0,
                    iv.as_mut_ptr(),
                    16,
                    out.as_mut_ptr(),
                    32,
                    &mut n,
                    0
                ),
                0
            );
            assert_eq!(n, 16);
            assert_eq!(out[..4], [0x76, 0x49, 0xab, 0xac]);
            assert_eq!(iv[..], out[..16]); // l'IV avance
                                           // Bourrage : 5 octets → un bloc, puis retour.
            let mut iv = [0u8; 16];
            let mut ct = [0u8; 16];
            assert_eq!(
                encrypt(
                    hk,
                    b"bonjo".as_ptr(),
                    5,
                    0,
                    iv.as_mut_ptr(),
                    16,
                    std::ptr::null_mut(),
                    0,
                    &mut n,
                    1
                ),
                0
            );
            assert_eq!(n, 16);
            assert_eq!(
                encrypt(
                    hk,
                    b"bonjo".as_ptr(),
                    5,
                    0,
                    iv.as_mut_ptr(),
                    16,
                    ct.as_mut_ptr(),
                    16,
                    &mut n,
                    1
                ),
                0
            );
            let mut iv = [0u8; 16];
            let mut back = [0u8; 16];
            assert_eq!(
                decrypt(
                    hk,
                    ct.as_ptr(),
                    16,
                    0,
                    iv.as_mut_ptr(),
                    16,
                    back.as_mut_ptr(),
                    16,
                    &mut n,
                    1
                ),
                0
            );
            assert_eq!(&back[..n as usize], b"bonjo");
            // Export puis destruction.
            let mut exported = [0u8; 28];
            assert_eq!(
                export_key(hk, 0, kind.as_ptr(), exported.as_mut_ptr(), 28, &mut n, 0),
                0
            );
            assert_eq!(exported[..], blob[..]);
            assert_eq!(destroy_key(hk), 0);
            assert_eq!(super::super::bcrypt_close_algorithm_provider(alg, 0), 0);

            // BCryptDuplicateHash : les deux copies continuent séparément.
            let mut h = 0usize;
            assert_eq!(
                super::super::bcrypt_create_hash(
                    0x41,
                    &mut h,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                    0,
                    0
                ),
                0
            );
            super::super::bcrypt_hash_data(h, b"ab".as_ptr(), 2, 0);
            let mut d = 0usize;
            assert_eq!(
                super::super::bcrypt_duplicate_hash(h, &mut d, std::ptr::null_mut(), 0, 0),
                0
            );
            super::super::bcrypt_hash_data(d, b"c".as_ptr(), 1, 0);
            let mut sum = [0u8; 32];
            assert_eq!(
                super::super::bcrypt_finish_hash(d, sum.as_mut_ptr(), 32, 0),
                0
            );
            assert_eq!(sum[..4], [0xba, 0x78, 0x16, 0xbf]); // SHA-256("abc")
            assert_eq!(
                super::super::bcrypt_finish_hash(h, sum.as_mut_ptr(), 32, 0),
                0
            );
            assert_ne!(sum[..4], [0xba, 0x78, 0x16, 0xbf]);
        }
    }
}
