use std::ffi::c_void;

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
    if _flags & 2 == 0 && _alg != 1 {
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

fn hash_length(alg: usize) -> Option<u32> {
    match alg {
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
    let _hmac = flags & 0x8 != 0;
    let name = crate::dlls::wstr(id);
    match alg_from_name(&name) {
        Some(h) => {
            *out = h;
            0
        }
        None => {
            *out = 0;
            STATUS_NOT_FOUND
        }
    }
}
pub extern "win64" fn bcrypt_close_algorithm_provider(h: usize, _flags: u32) -> i32 {
    if (1..=6).contains(&h) {
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
    _secret: *const u8,
    _secret_len: u32,
    _flags: u32,
) -> i32 {
    if hash_handle.is_null() || hash_length(alg).is_none() {
        return 0xC000_000Du32 as i32;
    }
    if digest(alg, b"").is_none() {
        return STATUS_NOT_SUPPORTED;
    }
    // Handle = algorithme (16 bits bas) | numéro de séquence (bits hauts).
    let seq = HASH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xFFFF;
    let h = alg | (seq << 16) | 0x8000_0000;
    hash_states()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h, Vec::new());
    *hash_handle = h;
    0
}

// SHA-1/SHA-256 maison, partagés avec winlux-remote.
#[allow(dead_code)]
#[path = "../remote/crypto.rs"]
mod crypto;

const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;

/// Données accumulées par handle de hachage (BCryptHashData), jusqu'à
/// BCryptFinishHash.
fn hash_states() -> &'static std::sync::Mutex<std::collections::HashMap<usize, Vec<u8>>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, Vec<u8>>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}
static HASH_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Condensat réel pour les algorithmes implémentés en interne ; `None` pour
/// les autres (plutôt que des zéros présentés comme un succès).
fn digest(alg: usize, data: &[u8]) -> Option<Vec<u8>> {
    match alg {
        ALG_SHA256 => Some(crypto::sha256(&[data]).to_vec()),
        ALG_SHA1 => Some(crypto::sha1(&[data]).to_vec()),
        _ => None,
    }
}

pub unsafe extern "win64" fn bcrypt_hash_data(
    hash: usize,
    input: *const u8,
    input_len: u32,
    _flags: u32,
) -> i32 {
    let mut states = hash_states().lock().unwrap_or_else(|e| e.into_inner());
    let Some(buf) = states.get_mut(&hash) else {
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
    let Some(data) = data else {
        return 0xC000_0008u32 as i32;
    };
    let Some(d) = digest(alg, &data) else {
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
    _secret: *const u8,
    _secret_len: u32,
    _input: *const u8,
    _input_len: u32,
    output: *mut u8,
    output_len: u32,
) -> i32 {
    if output.is_null() {
        return 0xC000_000Du32 as i32;
    }
    let expected = hash_length(alg).unwrap_or(0);
    if output_len != expected {
        return 0xC000_000Du32 as i32;
    }
    if _secret_len != 0 {
        return STATUS_NOT_SUPPORTED; // HMAC non pris en charge
    }
    let data: &[u8] = if _input.is_null() || _input_len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(_input, _input_len as usize)
    };
    let Some(d) = digest(alg, data) else {
        return STATUS_NOT_SUPPORTED;
    };
    std::ptr::copy_nonoverlapping(d.as_ptr(), output, d.len());
    0
}

pub unsafe extern "win64" fn bcrypt_destroy_key(_h: usize) -> i32 {
    0xC0000002u32 as i32
}
pub unsafe extern "win64" fn bcrypt_stub(_p: *mut c_void) -> i32 {
    0xC0000002u32 as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "BCryptGenRandom"=>fn bcrypt_gen_random,"BCryptOpenAlgorithmProvider"=>fn bcrypt_open_algorithm_provider,
     "BCryptCloseAlgorithmProvider"=>fn bcrypt_close_algorithm_provider,"BCryptDestroyKey"=>fn bcrypt_destroy_key,
     "BCryptEncrypt"=>fn bcrypt_stub,"BCryptDecrypt"=>fn bcrypt_stub,
     "BCryptGetProperty"=>fn bcrypt_get_property,
     "BCryptCreateHash"=>fn bcrypt_create_hash,"BCryptHashData"=>fn bcrypt_hash_data,
     "BCryptFinishHash"=>fn bcrypt_finish_hash,"BCryptDestroyHash"=>fn bcrypt_destroy_hash,
     "BCryptHash"=>fn bcrypt_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
