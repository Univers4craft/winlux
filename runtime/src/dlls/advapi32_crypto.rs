//! CryptoAPI « historique » d'advapi32 : condensats (CryptCreateHash…).
//!
//! Le fournisseur (HCRYPTPROV) est fictif ; les condensats MD5, SHA-1 et
//! SHA-2 sont calculés par le module bcrypt (maison ou OpenSSL de l'hôte).
//! Les clés (CryptGenKey, CryptEncrypt…) ne sont pas fournies.

use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const NTE_BAD_ALGID: u32 = 0x8009_0008;
const NTE_BAD_HASH: u32 = 0x8009_0002;
const NTE_BAD_TYPE: u32 = 0x8009_000A;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_INVALID_PARAMETER: u32 = 87;

struct Hash {
    algid: u32,
    data: Vec<u8>,
}

fn hashes() -> &'static Mutex<HashMap<usize, Hash>> {
    static H: OnceLock<Mutex<HashMap<usize, Hash>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicUsize = AtomicUsize::new(0x4841_0000);

/// ALG_ID → algorithme bcrypt (2 SHA256, 3 SHA1, 4 MD5, 5 SHA384, 6 SHA512).
fn bcrypt_alg(algid: u32) -> Option<usize> {
    Some(match algid {
        0x8003 => 4,
        0x8004 => 3,
        0x800c => 2,
        0x800d => 5,
        0x800e => 6,
        _ => return None,
    })
}

fn digest(h: &Hash) -> Option<Vec<u8>> {
    let alg = bcrypt_alg(h.algid)?;
    let len = match alg {
        2 => 32,
        3 => 20,
        4 => 16,
        5 => 48,
        _ => 64,
    };
    let mut out = vec![0u8; len];
    let r = unsafe {
        super::bcrypt::bcrypt_hash(
            alg,
            std::ptr::null(),
            0,
            h.data.as_ptr(),
            h.data.len() as u32,
            out.as_mut_ptr(),
            len as u32,
        )
    };
    (r == 0).then_some(out)
}

pub unsafe extern "win64" fn crypt_acquire_context_w(
    ph: *mut usize,
    _container: *const u16,
    _provider: *const u16,
    _type: u32,
    _flags: u32,
) -> i32 {
    if ph.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *ph = 1;
    1
}

pub unsafe extern "win64" fn crypt_create_hash(
    _prov: usize,
    algid: u32,
    key: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if key != 0 || bcrypt_alg(algid).is_none() {
        // HMAC et MAC (avec clé) ne sont pas fournis.
        set_last_error(NTE_BAD_ALGID);
        return 0;
    }
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(hashes()).insert(
        id,
        Hash {
            algid,
            data: Vec::new(),
        },
    );
    *out = id;
    1
}

pub unsafe extern "win64" fn crypt_hash_data(
    h: usize,
    data: *const u8,
    len: u32,
    _flags: u32,
) -> i32 {
    let mut map = super::lock(hashes());
    let Some(st) = map.get_mut(&h) else {
        set_last_error(NTE_BAD_HASH);
        return 0;
    };
    if len > 0 {
        if data.is_null() {
            set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        st.data
            .extend_from_slice(std::slice::from_raw_parts(data, len as usize));
    }
    1
}

pub unsafe extern "win64" fn crypt_get_hash_param(
    h: usize,
    param: u32,
    out: *mut u8,
    len: *mut u32,
    _flags: u32,
) -> i32 {
    let map = super::lock(hashes());
    let Some(st) = map.get(&h) else {
        set_last_error(NTE_BAD_HASH);
        return 0;
    };
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let value: Vec<u8> = match param {
        // HP_ALGID
        1 => st.algid.to_le_bytes().to_vec(),
        // HP_HASHVAL
        2 => match digest(st) {
            Some(d) => d,
            None => {
                set_last_error(NTE_BAD_ALGID);
                return 0;
            }
        },
        // HP_HASHSIZE
        4 => (digest(&Hash {
            algid: st.algid,
            data: Vec::new(),
        })
        .map_or(0, |d| d.len()) as u32)
            .to_le_bytes()
            .to_vec(),
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    };
    if out.is_null() {
        *len = value.len() as u32;
        return 1;
    }
    if (*len as usize) < value.len() {
        *len = value.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(value.as_ptr(), out, value.len());
    *len = value.len() as u32;
    1
}

pub unsafe extern "win64" fn crypt_duplicate_hash(
    h: usize,
    _reserved: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    let mut map = super::lock(hashes());
    let Some(st) = map.get(&h) else {
        set_last_error(NTE_BAD_HASH);
        return 0;
    };
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let copy = Hash {
        algid: st.algid,
        data: st.data.clone(),
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    map.insert(id, copy);
    *out = id;
    1
}

pub extern "win64" fn crypt_destroy_hash(h: usize) -> i32 {
    if super::lock(hashes()).remove(&h).is_some() {
        1
    } else {
        set_last_error(NTE_BAD_HASH);
        0
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CryptAcquireContextW" => fn crypt_acquire_context_w,
        "CryptCreateHash" => fn crypt_create_hash,
        "CryptHashData" => fn crypt_hash_data,
        "CryptGetHashParam" => fn crypt_get_hash_param,
        "CryptDuplicateHash" => fn crypt_duplicate_hash,
        "CryptDestroyHash" => fn crypt_destroy_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_par_morceaux() {
        let _teb = crate::process::test_teb();
        let mut h = 0usize;
        unsafe {
            assert_eq!(crypt_create_hash(1, 0x800c, 0, 0, &mut h), 1);
            assert_eq!(crypt_hash_data(h, b"a".as_ptr(), 1, 0), 1);
            let mut dup = 0usize;
            assert_eq!(crypt_duplicate_hash(h, 0, 0, &mut dup), 1);
            assert_eq!(crypt_hash_data(h, b"bc".as_ptr(), 2, 0), 1);
            let mut out = [0u8; 32];
            let mut n = 32u32;
            assert_eq!(crypt_get_hash_param(h, 2, out.as_mut_ptr(), &mut n, 0), 1);
            assert_eq!(out[..4], [0xba, 0x78, 0x16, 0xbf]);
            let mut size = [0u8; 4];
            let mut n = 4u32;
            assert_eq!(
                crypt_get_hash_param(dup, 4, size.as_mut_ptr(), &mut n, 0),
                1
            );
            assert_eq!(u32::from_le_bytes(size), 32);
            assert_eq!(crypt_destroy_hash(h), 1);
            assert_eq!(crypt_destroy_hash(dup), 1);
            assert_eq!(crypt_create_hash(1, 0x6610, 0, 0, &mut h), 0);
        }
    }
}
