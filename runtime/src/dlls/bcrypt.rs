/// BCryptPrimitives uses a SIZE_T length and a BOOL result, unlike BCrypt's
/// ULONG length / NTSTATUS API. Never report success with uninitialized bytes.
pub unsafe extern "win64" fn process_prng(buffer: *mut u8, size: usize) -> i32 {
    if size == 0 {
        return 1;
    }
    if buffer.is_null() || size > isize::MAX as usize {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut offset = 0;
    while offset < size {
        let count = libc::getrandom(
            buffer.add(offset).cast(),
            (size - offset).min(256 * 1024),
            0,
        );
        if count > 0 {
            offset += count as usize;
            continue;
        }
        if count < 0 && *libc::__errno_location() == libc::EINTR {
            continue;
        }
        // Older Linux kernels may not have getrandom. /dev/urandom is also a
        // system CSPRNG; there is no predictable application RNG fallback.
        let remaining = std::slice::from_raw_parts_mut(buffer.add(offset), size - offset);
        if std::fs::File::open("/dev/urandom")
            .and_then(|mut file| {
                use std::io::Read;
                file.read_exact(remaining)
            })
            .is_ok()
        {
            return 1;
        }
        crate::process::set_last_error(31);
        return 0;
    }
    1
}

pub fn primitives_exports() -> crate::dlls::Exports {
    crate::exports! { "ProcessPrng" => fn process_prng }
}

pub unsafe extern "win64" fn bcrypt_gen_random(
    _alg: usize,
    buf: *mut u8,
    len: u32,
    _flags: u32,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
    if buf.is_null() || _flags & !2 != 0 || (_flags & 2 != 0 && _alg != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    if _flags & 2 == 0 && resolve(_alg) != ALG_RNG {
        return STATUS_INVALID_PARAMETER;
    }
    let slice = std::slice::from_raw_parts_mut(buf, len as usize);
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if f.read_exact(slice).is_ok() {
            return 0;
        }
    }
    STATUS_NOT_SUPPORTED
}
// Algorithm handle values: 1=RNG, 2=SHA256, 3=SHA1, 4=MD5, 5=SHA384, 6=SHA512
const ALG_RNG: usize = 1;
const ALG_SHA256: usize = 2;
const ALG_SHA1: usize = 3;
const ALG_MD5: usize = 4;
const ALG_SHA384: usize = 5;
const ALG_SHA512: usize = 6;

fn alg_from_name(name: &str) -> Option<usize> {
    match name {
        "RNG" => Some(ALG_RNG),
        "SHA256" => Some(ALG_SHA256),
        "SHA1" => Some(ALG_SHA1),
        "MD5" => Some(ALG_MD5),
        "SHA384" => Some(ALG_SHA384),
        "SHA512" => Some(ALG_SHA512),
        _ => None,
    }
}

/// Les poignées HMAC valent l'algorithme + 0x10 (BCRYPT_ALG_HANDLE_HMAC_FLAG).
const HMAC_BIT: usize = 0x10;

/// Pseudo-handles d'algorithme (documentation CNG, « CNG Algorithm
/// Pseudo-handles ») : utilisables sans BCryptOpenAlgorithmProvider.
/// .NET hache en un coup avec eux (SHA1.HashData, IID des délégués
/// CsWinRT dans Paint.NET, #1364).
fn resolve(alg: usize) -> usize {
    match alg {
        0x21 => ALG_MD5,
        0x31 => ALG_SHA1,
        0x41 => ALG_SHA256,
        0x51 => ALG_SHA384,
        0x61 => ALG_SHA512,
        0x81 => ALG_RNG,
        0x91 => ALG_MD5 | HMAC_BIT,
        0xa1 => ALG_SHA1 | HMAC_BIT,
        0xb1 => ALG_SHA256 | HMAC_BIT,
        0xc1 => ALG_SHA384 | HMAC_BIT,
        0xd1 => ALG_SHA512 | HMAC_BIT,
        other => other,
    }
}

fn hash_length(alg: usize) -> Option<u32> {
    match alg & !HMAC_BIT {
        ALG_SHA256 => Some(32),
        ALG_SHA1 => Some(20),
        ALG_MD5 => Some(16),
        ALG_SHA384 => Some(48),
        ALG_SHA512 => Some(64),
        _ => None,
    }
}

pub unsafe extern "win64" fn bcrypt_open_algorithm_provider(
    out: *mut usize,
    id: *const u16,
    _impl_name: *const u16,
    flags: u32,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_FOUND: i32 = 0xC000_0002u32 as i32;
    if out.is_null() || id.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // BCRYPT_ALG_HANDLE_HMAC_FLAG = 0x8
    let hmac = flags & 0x8 != 0;
    let name = crate::dlls::wstr(id);
    if !hmac {
        if let Some(h) = ecdsa::open(&name) {
            put_handle(out, h);
            return 0;
        }
    }
    match alg_from_name(&name) {
        Some(h) if hmac && h == ALG_RNG => {
            put_handle(out, 0);
            STATUS_NOT_FOUND
        }
        Some(h) => {
            put_handle(out, if hmac { h | HMAC_BIT } else { h });
            0
        }
        None if name == "AES" && !hmac => {
            put_handle(out, aes::open());
            0
        }
        None => {
            put_handle(out, 0);
            STATUS_NOT_FOUND
        }
    }
}
/// Poignée rendue à la taille des pointeurs du programme (4 octets en x86).
unsafe fn put_handle(out: *mut usize, h: usize) {
    crate::wow32::write_ptr(out as usize, h);
}

pub extern "win64" fn bcrypt_close_algorithm_provider(h: usize, _flags: u32) -> i32 {
    if aes::close(h) || ecdsa::close(h) {
        return 0;
    }
    if (1..=6).contains(&(h & !HMAC_BIT)) {
        0
    } else {
        0xC000_0008u32 as i32
    }
}

pub unsafe extern "win64" fn bcrypt_get_property(
    h: usize,
    prop: *const u16,
    output: *mut u8,
    output_len: u32,
    result_len: *mut u32,
    _flags: u32,
) -> i32 {
    if prop.is_null() || result_len.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let name = crate::dlls::wstr(prop);
    if let Some(r) = aes::get_property(h, &name, output, output_len, result_len) {
        return r;
    }
    if let Some(r) = ecdsa::get_property(h, &name, output, output_len, result_len) {
        return r;
    }
    let h = resolve(h);
    if name == "HashDigestLength" {
        if let Some(len) = hash_length(h) {
            *result_len = 4;
            if !output.is_null() && output_len >= 4 {
                *(output as *mut u32) = len;
            }
            return 0;
        }
    }
    if name == "ObjectLength" {
        *result_len = 4;
        if !output.is_null() && output_len >= 4 {
            *(output as *mut u32) = 256;
        }
        return 0;
    }
    0xC000_000Du32 as i32
}

pub unsafe extern "win64" fn bcrypt_create_hash(
    alg: usize,
    hash_handle: *mut usize,
    _obj: *mut u8,
    _obj_len: u32,
    secret: *const u8,
    secret_len: u32,
    _flags: u32,
) -> i32 {
    let alg = resolve(alg);
    if hash_handle.is_null() || hash_length(alg).is_none() {
        return 0xC000_000Du32 as i32;
    }
    let key = (alg & HMAC_BIT != 0).then(|| {
        if secret.is_null() || secret_len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(secret, secret_len as usize).to_vec()
        }
    });
    if compute(alg, key.as_deref(), b"").is_none() {
        return STATUS_NOT_SUPPORTED;
    }
    // Handle = algorithme (16 bits bas) | numéro de séquence (bits hauts).
    let seq = HASH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xFFFF;
    let h = alg | (seq << 16) | 0x8000_0000;
    hash_states()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h, (key, Vec::new()));
    put_handle(hash_handle, h);
    0
}

/// BCryptDuplicateHash : copie l'état (clé HMAC et données déjà reçues).
pub unsafe extern "win64" fn bcrypt_duplicate_hash(
    hash: usize,
    out: *mut usize,
    _obj: *mut u8,
    _obj_len: u32,
    _flags: u32,
) -> i32 {
    if out.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let mut states = hash_states().lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = states.get(&hash).cloned() else {
        return 0xC000_0008u32 as i32;
    };
    let seq = HASH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xFFFF;
    let h = (hash & 0xFFFF) | (seq << 16) | 0x8000_0000;
    states.insert(h, state);
    put_handle(out, h);
    0
}

/// BCryptDeriveKeyPBKDF2 (lot F, #1615) : PBKDF2 de la RFC 8018 avec le
/// HMAC de l'algorithme `prf` (ouvert avec ou sans BCRYPT_ALG_HANDLE_HMAC_FLAG).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn bcrypt_derive_key_pbkdf2(
    prf: usize,
    password: *const u8,
    password_len: u32,
    salt: *const u8,
    salt_len: u32,
    iterations: u64,
    out: *mut u8,
    out_len: u32,
    _flags: u32,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    let alg = resolve(prf) | HMAC_BIT;
    if hash_length(alg).is_none() {
        return 0xC000_0008u32 as i32;
    }
    if iterations == 0 || (out.is_null() && out_len > 0) {
        return STATUS_INVALID_PARAMETER;
    }
    let slice = |p: *const u8, n: u32| {
        if p.is_null() || n == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(p, n as usize)
        }
    };
    match pbkdf2(
        alg,
        slice(password, password_len),
        slice(salt, salt_len),
        iterations,
        out_len as usize,
    ) {
        Some(key) => {
            std::ptr::copy_nonoverlapping(key.as_ptr(), out, key.len());
            0
        }
        None => STATUS_NOT_SUPPORTED,
    }
}

fn pbkdf2(
    alg: usize,
    password: &[u8],
    salt: &[u8],
    iterations: u64,
    len: usize,
) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    let mut block = 1u32;
    while out.len() < len {
        let mut msg = salt.to_vec();
        msg.extend_from_slice(&block.to_be_bytes());
        let mut u = compute(alg, Some(password), &msg)?;
        let mut t = u.clone();
        for _ in 1..iterations {
            u = compute(alg, Some(password), &u)?;
            t.iter_mut().zip(&u).for_each(|(a, b)| *a ^= b);
        }
        let take = (len - out.len()).min(t.len());
        out.extend_from_slice(&t[..take]);
        block += 1;
    }
    Some(out)
}

/// BCryptEnumContextFunctions (lot F, #1615) : fonctions servies pour
/// l'interface demandée, dans une CRYPT_CONTEXT_FUNCTIONS (nombre, tableau
/// de chaînes) allouée ici et rendue par BCryptFreeBuffer, ou copiée dans
/// le tampon de l'appelant.
pub unsafe extern "win64" fn bcrypt_enum_context_functions(
    _table: u32,
    _context: *const u16,
    interface: u32,
    cb: *mut u32,
    buffer: *mut usize,
) -> i32 {
    if cb.is_null() || buffer.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let names: &[&str] = match interface {
        1 => &["AES"],
        2 => &["SHA256", "SHA1", "SHA384", "SHA512", "MD5"],
        6 => &["RNG"],
        _ => &[],
    };
    let ps = crate::wow32::pointer_size();
    // CRYPT_CONTEXT_FUNCTIONS : ULONG cFunctions, puis PWSTR* à +ps.
    let table_at = 2 * ps;
    let mut strings_at = table_at + ps * names.len();
    let wide: Vec<Vec<u16>> = names
        .iter()
        .map(|n| n.encode_utf16().chain(std::iter::once(0)).collect())
        .collect();
    let size = strings_at + wide.iter().map(|w| w.len() * 2).sum::<usize>();
    let existing = crate::wow32::read_ptr(buffer as usize);
    let base = if existing == 0 {
        let p = crate::dlls::ole32::co_task_mem_alloc(size) as usize;
        if p == 0 {
            return 0xC000_0017u32 as i32; // STATUS_NO_MEMORY
        }
        p
    } else {
        if (*cb as usize) < size {
            *cb = size as u32;
            return 0xC000_0023u32 as i32; // STATUS_BUFFER_TOO_SMALL
        }
        existing
    };
    std::ptr::write_bytes(base as *mut u8, 0, size);
    *(base as *mut u32) = names.len() as u32;
    crate::wow32::write_ptr(
        base + ps,
        if names.is_empty() { 0 } else { base + table_at },
    );
    for (i, w) in wide.iter().enumerate() {
        std::ptr::copy_nonoverlapping(w.as_ptr(), (base + strings_at) as *mut u16, w.len());
        crate::wow32::write_ptr(base + table_at + i * ps, base + strings_at);
        strings_at += w.len() * 2;
    }
    *cb = size as u32;
    crate::wow32::write_ptr(buffer as usize, base);
    0
}

/// BCryptFreeBuffer : mémoire rendue par les fonctions d'énumération.
pub unsafe extern "win64" fn bcrypt_free_buffer(p: *mut std::ffi::c_void) {
    if !p.is_null() {
        crate::dlls::ole32::co_task_mem_free(p);
    }
}

// SHA-1/SHA-256 maison, partagés avec winlux-remote.
#[allow(dead_code)]
#[path = "../remote/crypto.rs"]
pub(crate) mod crypto;

/// SHA-256 pour le reste du runtime (block map des paquets MSIX).
pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    crypto::sha256(&[data])
}

const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;

/// Clé HMAC éventuelle et données accumulées par handle de hachage
/// (BCryptHashData), jusqu'à BCryptFinishHash.
type HashState = (Option<Vec<u8>>, Vec<u8>);
fn hash_states() -> &'static std::sync::Mutex<std::collections::HashMap<usize, HashState>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, HashState>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}
static HASH_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Condensat réel, ou `None` (plutôt que des zéros présentés comme un
/// succès). SHA-1, SHA-256 et MD5 sont maison ; SHA-384, SHA-512 et les
/// HMAC passent par l'OpenSSL de l'hôte.
fn digest(alg: usize, data: &[u8]) -> Option<Vec<u8>> {
    match alg {
        ALG_SHA256 => Some(crypto::sha256(&[data]).to_vec()),
        ALG_SHA1 => Some(crypto::sha1(&[data]).to_vec()),
        ALG_MD5 => Some(crate::dlls::games_win32::md5(data).to_vec()),
        _ => crate::dlls::hostssl::digest(ossl_name(alg)?, data),
    }
}

fn ossl_name(alg: usize) -> Option<&'static std::ffi::CStr> {
    Some(match alg & !HMAC_BIT {
        ALG_SHA256 => c"SHA256",
        ALG_SHA1 => c"SHA1",
        ALG_MD5 => c"MD5",
        ALG_SHA384 => c"SHA384",
        ALG_SHA512 => c"SHA512",
        _ => return None,
    })
}

fn compute(alg: usize, key: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    match key {
        Some(k) => crate::dlls::hostssl::hmac(ossl_name(alg)?, k, data),
        None => digest(alg, data),
    }
}

pub unsafe extern "win64" fn bcrypt_hash_data(
    hash: usize,
    input: *const u8,
    input_len: u32,
    _flags: u32,
) -> i32 {
    let mut states = hash_states().lock().unwrap_or_else(|e| e.into_inner());
    let Some((_, buf)) = states.get_mut(&hash) else {
        return 0xC000_0008u32 as i32; // STATUS_INVALID_HANDLE
    };
    if input_len > 0 {
        if input.is_null() {
            return 0xC000_000Du32 as i32;
        }
        buf.extend_from_slice(std::slice::from_raw_parts(input, input_len as usize));
    }
    0
}

pub unsafe extern "win64" fn bcrypt_finish_hash(
    hash: usize,
    output: *mut u8,
    output_len: u32,
    _flags: u32,
) -> i32 {
    if output.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let alg = hash & 0xFFFF;
    let expected = hash_length(alg).unwrap_or(0);
    if output_len != expected {
        return 0xC000_000Du32 as i32;
    }
    let data = hash_states()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&hash);
    let Some((key, data)) = data else {
        return 0xC000_0008u32 as i32;
    };
    let Some(d) = compute(alg, key.as_deref(), &data) else {
        return STATUS_NOT_SUPPORTED;
    };
    std::ptr::copy_nonoverlapping(d.as_ptr(), output, d.len());
    0
}

pub unsafe extern "win64" fn bcrypt_destroy_hash(hash: usize) -> i32 {
    hash_states()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&hash);
    0
}

pub unsafe extern "win64" fn bcrypt_hash(
    alg: usize,
    secret: *const u8,
    secret_len: u32,
    _input: *const u8,
    _input_len: u32,
    output: *mut u8,
    output_len: u32,
) -> i32 {
    let alg = resolve(alg);
    if output.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let expected = hash_length(alg).unwrap_or(0);
    if output_len != expected {
        return 0xC000_000Du32 as i32;
    }
    let key = (alg & HMAC_BIT != 0).then(|| {
        if secret.is_null() || secret_len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(secret, secret_len as usize).to_vec()
        }
    });
    let data: &[u8] = if _input.is_null() || _input_len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(_input, _input_len as usize)
    };
    let Some(d) = compute(alg, key.as_deref(), data) else {
        return STATUS_NOT_SUPPORTED;
    };
    std::ptr::copy_nonoverlapping(d.as_ptr(), output, d.len());
    0
}

mod aes;
mod ecdsa;

/// BCryptDestroyKey : clé ECDSA ou AES.
unsafe extern "win64" fn bcrypt_destroy_key(key: usize) -> i32 {
    match ecdsa::destroy_key(key) {
        Some(r) => r,
        None => aes::destroy_key(key),
    }
}

// --- Clés asymétriques, secrets, énumérations (ISCC, #1860) -----------------
// ISCmplr.dll (Inno Setup 6.7) résout ces fonctions au chargement et refuse
// de se charger s'il en manque une ; il ne s'en sert que pour les signatures
// ISSig. La vérification ECDSA (clé publique importée, BCryptVerifySignature)
// est servie par bcrypt/ecdsa.rs (#2173) ; le reste répond
// STATUS_NOT_SUPPORTED, comme un fournisseur qui ne connaît pas l'opération.
// Source : documentation publique de bcrypt.h.

const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;

pub extern "win64" fn bcrypt_generate_key_pair(
    _alg: usize,
    _key: usize,
    _bits: u32,
    _flags: u32,
) -> i32 {
    STATUS_NOT_SUPPORTED
}
/// Aucune paire n'a pu être créée, donc aucun handle de paire n'est valide.
pub extern "win64" fn bcrypt_finalize_key_pair(_key: usize, _flags: u32) -> i32 {
    STATUS_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn bcrypt_import_key_pair(
    alg: usize,
    _import: usize,
    blob_type: usize,
    key: usize,
    input: usize,
    len: u32,
    _flags: u32,
) -> i32 {
    if blob_type == 0 {
        return 0xC000_000Du32 as i32; // STATUS_INVALID_PARAMETER
    }
    let kind = crate::dlls::wstr(blob_type as *const u16);
    ecdsa::import_key_pair(alg, &kind, key as *mut usize, input as *const u8, len)
        .unwrap_or(STATUS_NOT_SUPPORTED)
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn bcrypt_sign_hash(
    _key: usize,
    _padding: usize,
    _input: usize,
    _len: u32,
    _output: usize,
    _out_len: u32,
    _result: usize,
    _flags: u32,
) -> i32 {
    STATUS_NOT_SUPPORTED
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn bcrypt_verify_signature(
    key: usize,
    _padding: usize,
    hash: usize,
    hash_len: u32,
    sig: usize,
    sig_len: u32,
    _flags: u32,
) -> i32 {
    ecdsa::verify_signature(key, hash as *const u8, hash_len, sig as *const u8, sig_len)
        .unwrap_or(STATUS_NOT_SUPPORTED)
}
pub extern "win64" fn bcrypt_secret_agreement(
    _private: usize,
    _public: usize,
    _secret: usize,
    _flags: u32,
) -> i32 {
    STATUS_NOT_SUPPORTED
}
pub extern "win64" fn bcrypt_destroy_secret(_secret: usize) -> i32 {
    STATUS_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn bcrypt_derive_key(
    _secret: usize,
    _kdf: usize,
    _params: usize,
    _out: usize,
    _len: u32,
    _result: usize,
    _flags: u32,
) -> i32 {
    STATUS_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn bcrypt_key_derivation(
    _key: usize,
    _params: usize,
    _out: usize,
    _len: u32,
    _result: usize,
    _flags: u32,
) -> i32 {
    STATUS_NOT_SUPPORTED
}
/// BCryptGetFipsAlgorithmMode : le mode FIPS n'est pas imposé.
pub unsafe extern "win64" fn bcrypt_get_fips_algorithm_mode(enabled: *mut u8) -> i32 {
    if enabled.is_null() {
        return 0xC000_000Du32 as i32; // STATUS_INVALID_PARAMETER
    }
    *enabled = 0;
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "BCryptGenRandom"=>fn bcrypt_gen_random,"BCryptOpenAlgorithmProvider"=>fn bcrypt_open_algorithm_provider,
     "BCryptCloseAlgorithmProvider"=>fn bcrypt_close_algorithm_provider,
     "BCryptDestroyKey"=>fn bcrypt_destroy_key,"BCryptEncrypt"=>fn aes::encrypt,"BCryptDecrypt"=>fn aes::decrypt,
     "BCryptGenerateSymmetricKey"=>fn aes::generate_symmetric_key,"BCryptImportKey"=>fn aes::import_key,
     "BCryptExportKey"=>fn aes::export_key,"BCryptDuplicateKey"=>fn aes::duplicate_key,
     "BCryptSetProperty"=>fn aes::set_property,"BCryptDuplicateHash"=>fn bcrypt_duplicate_hash,
     "BCryptGetProperty"=>fn bcrypt_get_property,
     "BCryptCreateHash"=>fn bcrypt_create_hash,"BCryptHashData"=>fn bcrypt_hash_data,
     "BCryptFinishHash"=>fn bcrypt_finish_hash,"BCryptDestroyHash"=>fn bcrypt_destroy_hash,
     "BCryptDeriveKeyPBKDF2"=>fn bcrypt_derive_key_pbkdf2,
     "BCryptEnumContextFunctions"=>fn bcrypt_enum_context_functions,
     "BCryptFreeBuffer"=>fn bcrypt_free_buffer,
     "BCryptHash"=>fn bcrypt_hash,
     "BCryptGenerateKeyPair"=>fn bcrypt_generate_key_pair,"BCryptFinalizeKeyPair"=>fn bcrypt_finalize_key_pair,
     "BCryptImportKeyPair"=>fn bcrypt_import_key_pair,"BCryptSignHash"=>fn bcrypt_sign_hash,
     "BCryptVerifySignature"=>fn bcrypt_verify_signature,"BCryptSecretAgreement"=>fn bcrypt_secret_agreement,
     "BCryptDestroySecret"=>fn bcrypt_destroy_secret,"BCryptDeriveKey"=>fn bcrypt_derive_key,
     "BCryptKeyDerivation"=>fn bcrypt_key_derivation,
     "BCryptGetFipsAlgorithmMode"=>fn bcrypt_get_fips_algorithm_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_rfc6070_et_enumeration() {
        unsafe {
            // RFC 6070, PBKDF2-HMAC-SHA1, « password » / « salt », 2 tours.
            let mut key = [0u8; 20];
            let st = bcrypt_derive_key_pbkdf2(
                0xa1,
                b"password".as_ptr(),
                8,
                b"salt".as_ptr(),
                4,
                2,
                key.as_mut_ptr(),
                20,
                0,
            );
            if st == STATUS_NOT_SUPPORTED {
                return; // pas d'OpenSSL sur l'hôte de test
            }
            assert_eq!(st, 0);
            assert_eq!(
                key[..4],
                [0xea, 0x6c, 0x01, 0x4d],
                "premier bloc du vecteur RFC 6070"
            );
            assert_eq!(key[16..], [0xd8, 0xde, 0x89, 0x57]);
            let (mut cb, mut buf) = (0u32, 0usize);
            assert_eq!(
                bcrypt_enum_context_functions(1, std::ptr::null(), 2, &mut cb, &mut buf),
                0
            );
            assert_eq!(*(buf as *const u32), 5);
            let first = *((buf + 8) as *const *const *const u16);
            assert_eq!(crate::dlls::wstr(*first), "SHA256");
            bcrypt_free_buffer(buf as *mut _);
        }
    }

    #[test]
    fn process_prng_uses_size_t_and_rejects_invalid_buffers() {
        let _teb = crate::process::test_teb();
        let mut first = [0u8; 64];
        let mut second = [0u8; 64];
        unsafe {
            assert_eq!(process_prng(first.as_mut_ptr(), first.len()), 1);
            assert_eq!(process_prng(second.as_mut_ptr(), second.len()), 1);
            assert_eq!(process_prng(std::ptr::null_mut(), 0), 1);
            assert_eq!(process_prng(std::ptr::null_mut(), 1), 0);
            assert_eq!(crate::process::last_error(), 87);
            assert_eq!(process_prng(first.as_mut_ptr(), usize::MAX), 0);
            assert_eq!(crate::process::last_error(), 87);
        }
        assert_ne!(first, second);
    }

    #[test]
    fn provider_rng_lit_une_source_systeme_et_refuse_les_buffers_invalides() {
        let algorithm: [u16; 4] = [b'R' as u16, b'N' as u16, b'G' as u16, 0];
        let mut h = 0usize;
        assert_eq!(
            unsafe {
                bcrypt_open_algorithm_provider(&mut h, algorithm.as_ptr(), std::ptr::null(), 0)
            },
            0
        );
        let mut bytes = [0u8; 32];
        assert_eq!(
            unsafe { bcrypt_gen_random(h, bytes.as_mut_ptr(), bytes.len() as u32, 0) },
            0
        );
        assert_eq!(
            unsafe { bcrypt_gen_random(0, std::ptr::null_mut(), 16, 2) },
            0xC000000Du32 as i32
        );
        assert_eq!(bcrypt_close_algorithm_provider(h, 0), 0);
    }

    /// #1364 : hachage en un coup avec un pseudo-handle (SHA1.HashData de
    /// .NET, utilisé par CsWinRT dans Paint.NET).
    #[test]
    fn pseudo_handle_sha1_en_un_coup() {
        let mut out = [0u8; 20];
        let r = unsafe {
            bcrypt_hash(
                0x31,
                std::ptr::null(),
                0,
                b"abc".as_ptr(),
                3,
                out.as_mut_ptr(),
                20,
            )
        };
        assert_eq!(r, 0);
        assert_eq!(out[..4], [0xa9, 0x99, 0x3e, 0x36]);
        assert_eq!(out[16..], [0x9c, 0xd0, 0xd8, 0x9d]);
        // SHA-256 par pseudo-handle, et longueur du condensat.
        let mut out = [0u8; 32];
        let r = unsafe {
            bcrypt_hash(
                0x41,
                std::ptr::null(),
                0,
                b"abc".as_ptr(),
                3,
                out.as_mut_ptr(),
                32,
            )
        };
        assert_eq!(r, 0);
        assert_eq!(out[..4], [0xba, 0x78, 0x16, 0xbf]);
        let prop: Vec<u16> = "HashDigestLength".encode_utf16().chain([0]).collect();
        let (mut len, mut got) = (0u32, 0u32);
        let r = unsafe {
            bcrypt_get_property(
                0x31,
                prop.as_ptr(),
                (&mut len as *mut u32).cast(),
                4,
                &mut got,
                0,
            )
        };
        assert_eq!((r, len), (0, 20));
        // Générateur par pseudo-handle.
        let mut bytes = [0u8; 16];
        assert_eq!(
            unsafe { bcrypt_gen_random(0x81, bytes.as_mut_ptr(), 16, 0) },
            0
        );
    }

    #[test]
    fn md5_sha512_et_hmac() {
        if crate::dlls::hostssl::ssl().is_none() {
            return;
        }
        let name: Vec<u16> = "SHA256".encode_utf16().chain([0]).collect();
        let mut alg = 0usize;
        assert_eq!(
            unsafe { bcrypt_open_algorithm_provider(&mut alg, name.as_ptr(), std::ptr::null(), 8) },
            0
        );
        let mut mac = [0u8; 32];
        let msg = b"The quick brown fox jumps over the lazy dog";
        let r = unsafe {
            bcrypt_hash(
                alg,
                b"key".as_ptr(),
                3,
                msg.as_ptr(),
                msg.len() as u32,
                mac.as_mut_ptr(),
                32,
            )
        };
        assert_eq!(r, 0);
        assert_eq!(mac[..4], [0xf7, 0xbc, 0x83, 0xf4]);
        assert_eq!(bcrypt_close_algorithm_provider(alg, 0), 0);
        let mut md5 = [0u8; 16];
        let r = unsafe {
            bcrypt_hash(
                ALG_MD5,
                std::ptr::null(),
                0,
                b"abc".as_ptr(),
                3,
                md5.as_mut_ptr(),
                16,
            )
        };
        assert_eq!(r, 0);
        assert_eq!(md5[..4], [0x90, 0x01, 0x50, 0x98]);
    }
}
