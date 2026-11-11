//! CryptoAPI « historique » d'advapi32 : condensats (CryptCreateHash…).
//!
//! Le fournisseur (HCRYPTPROV) est fictif ; les condensats MD5, SHA-1 et
//! SHA-2 sont calculés par le module bcrypt (maison ou OpenSSL de l'hôte),
//! MD4 ici même (NTLM de libcurl).
//!
//! Clés symétriques : CryptImportKey (PLAINTEXTKEYBLOB), CryptGenKey,
//! CryptDeriveKey, CryptEncrypt/CryptDecrypt en CBC (IV nul par défaut, comme
//! Windows) ou ECB, avec le bourrage PKCS#5 au dernier appel. DES, 3DES et AES
//! passent par l'OpenSSL de l'hôte (DES = DES-EDE3 à clé répétée, car DES
//! seul n'est que dans le fournisseur « legacy »), RC4 est maison. Les clés
//! RSA (échange, signature) et les conteneurs sont dans `advapi32_rsa`.

use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const NTE_BAD_ALGID: u32 = 0x8009_0008;
const NTE_BAD_HASH: u32 = 0x8009_0002;
const NTE_BAD_TYPE: u32 = 0x8009_000A;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_INVALID_PARAMETER: u32 = 87;
const NTE_BAD_DATA: u32 = 0x8009_0005;
const NTE_BAD_KEY: u32 = 0x8009_0003;
const NTE_BAD_LEN: u32 = 0x8009_0004;
const NTE_BAD_FLAGS: u32 = 0x8009_0009;

const CALG_MD4: u32 = 0x8002;
const CALG_HMAC: u32 = 0x8009;
const HP_HASHVAL: u32 = 2;
const HP_HMAC_INFO: u32 = 5;
const CRYPT_IPSEC_HMAC_KEY: u32 = 0x100;
const SIMPLEBLOB: u8 = 1;

#[derive(Default)]
struct Hash {
    algid: u32,
    data: Vec<u8>,
    /// Fournisseur qui a créé le condensat (CryptSignHash signe avec sa clé).
    prov: usize,
    /// Clé d'un HMAC (CALG_HMAC) ; l'algorithme interne vient de HP_HMAC_INFO.
    hmac: Option<Vec<u8>>,
    /// Valeur imposée par CryptSetHashParam(HP_HASHVAL) (signer un condensat
    /// calculé ailleurs).
    fixed: Option<Vec<u8>>,
}

fn hashes() -> &'static Mutex<HashMap<usize, Hash>> {
    static H: OnceLock<Mutex<HashMap<usize, Hash>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicUsize = AtomicUsize::new(0x4841_0000);

/// MD4 (RFC 1320), pour le condensat NT de NTLM.
pub fn md4(data: &[u8]) -> [u8; 16] {
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let mut h: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in msg.chunks(64) {
        let x: Vec<u32> = chunk
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let [mut a, mut b, mut c, mut d] = h;
        let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
        let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
        let hh = |x: u32, y: u32, z: u32| x ^ y ^ z;
        for &i in &[0usize, 4, 8, 12] {
            a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
            d = d
                .wrapping_add(f(a, b, c))
                .wrapping_add(x[i + 1])
                .rotate_left(7);
            c = c
                .wrapping_add(f(d, a, b))
                .wrapping_add(x[i + 2])
                .rotate_left(11);
            b = b
                .wrapping_add(f(c, d, a))
                .wrapping_add(x[i + 3])
                .rotate_left(19);
        }
        for &i in &[0usize, 1, 2, 3] {
            let k = 0x5a82_7999u32;
            a = a
                .wrapping_add(g(b, c, d))
                .wrapping_add(x[i])
                .wrapping_add(k)
                .rotate_left(3);
            d = d
                .wrapping_add(g(a, b, c))
                .wrapping_add(x[i + 4])
                .wrapping_add(k)
                .rotate_left(5);
            c = c
                .wrapping_add(g(d, a, b))
                .wrapping_add(x[i + 8])
                .wrapping_add(k)
                .rotate_left(9);
            b = b
                .wrapping_add(g(c, d, a))
                .wrapping_add(x[i + 12])
                .wrapping_add(k)
                .rotate_left(13);
        }
        for &i in &[0usize, 2, 1, 3] {
            let k = 0x6ed9_eba1u32;
            a = a
                .wrapping_add(hh(b, c, d))
                .wrapping_add(x[i])
                .wrapping_add(k)
                .rotate_left(3);
            d = d
                .wrapping_add(hh(a, b, c))
                .wrapping_add(x[i + 8])
                .wrapping_add(k)
                .rotate_left(9);
            c = c
                .wrapping_add(hh(d, a, b))
                .wrapping_add(x[i + 4])
                .wrapping_add(k)
                .rotate_left(11);
            b = b
                .wrapping_add(hh(c, d, a))
                .wrapping_add(x[i + 12])
                .wrapping_add(k)
                .rotate_left(15);
        }
        h = [
            h[0].wrapping_add(a),
            h[1].wrapping_add(b),
            h[2].wrapping_add(c),
            h[3].wrapping_add(d),
        ];
    }
    let mut out = [0u8; 16];
    for (o, v) in out.chunks_mut(4).zip(h) {
        o.copy_from_slice(&v.to_le_bytes());
    }
    out
}

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

/// Algorithme, valeur et fournisseur d'un condensat (CryptSignHash,
/// CryptVerifySignature).
pub(crate) fn hash_value(h: usize) -> Option<(u32, Vec<u8>, usize)> {
    let map = super::lock(hashes());
    let st = map.get(&h)?;
    Some((st.algid, digest(st)?, st.prov))
}

fn digest(h: &Hash) -> Option<Vec<u8>> {
    if let Some(v) = &h.fixed {
        return Some(v.clone());
    }
    if let Some(key) = &h.hmac {
        return hmac(h.algid, key, &h.data);
    }
    if h.algid == CALG_MD4 {
        return Some(md4(&h.data).to_vec());
    }
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

pub unsafe extern "win64" fn crypt_create_hash(
    prov: usize,
    algid: u32,
    key: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let hmac = if algid == CALG_HMAC {
        match super::lock(keys()).get(&key) {
            Some(k) => Some(k.key.clone()),
            None => {
                set_last_error(NTE_BAD_KEY);
                return 0;
            }
        }
    } else if key != 0 || (algid != CALG_MD4 && bcrypt_alg(algid).is_none()) {
        // Les MAC par chiffrement (CALG_MAC) ne sont pas fournis.
        set_last_error(NTE_BAD_ALGID);
        return 0;
    } else {
        None
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(hashes()).insert(
        id,
        Hash {
            algid,
            prov,
            hmac,
            ..Default::default()
        },
    );
    *out = id;
    1
}

/// HMAC (RFC 2104) sur le condensat `algid`.
fn hmac(algid: u32, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if algid == CALG_HMAC {
        // HP_HMAC_INFO pas encore donné.
        return None;
    }
    let plain = |d: Vec<u8>| {
        digest(&Hash {
            algid,
            data: d,
            ..Default::default()
        })
    };
    let block = if matches!(algid, 0x800d | 0x800e) {
        128
    } else {
        64
    };
    let mut k = if key.len() > block {
        plain(key.to_vec())?
    } else {
        key.to_vec()
    };
    k.resize(block, 0);
    let inner: Vec<u8> = k
        .iter()
        .map(|b| b ^ 0x36)
        .chain(data.iter().copied())
        .collect();
    let ih = plain(inner)?;
    plain(k.iter().map(|b| b ^ 0x5c).chain(ih).collect())
}

/// CryptSetHashParam : HP_HMAC_INFO (algorithme interne du HMAC, avec les
/// chaînes 0x36/0x5c par défaut) et HP_HASHVAL (valeur imposée).
pub unsafe extern "win64" fn crypt_set_hash_param(
    h: usize,
    param: u32,
    data: *const u8,
    _flags: u32,
) -> i32 {
    let mut map = super::lock(hashes());
    let Some(st) = map.get_mut(&h) else {
        set_last_error(NTE_BAD_HASH);
        return 0;
    };
    if data.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match param {
        HP_HMAC_INFO if st.hmac.is_some() => {
            // HMAC_INFO { HashAlgid, pbInnerString, cbInnerString,
            // pbOuterString, cbOuterString } (x64).
            let alg = (data as *const u32).read_unaligned();
            let inner = (data.add(16) as *const u32).read_unaligned();
            let outer = (data.add(32) as *const u32).read_unaligned();
            if inner != 0 || outer != 0 {
                set_last_error(NTE_BAD_DATA);
                return 0;
            }
            if alg != CALG_MD4 && bcrypt_alg(alg).is_none() {
                set_last_error(NTE_BAD_ALGID);
                return 0;
            }
            st.algid = alg;
        }
        HP_HASHVAL => {
            let Some(n) = digest(&Hash {
                algid: st.algid,
                ..Default::default()
            })
            .map(|d| d.len()) else {
                set_last_error(NTE_BAD_ALGID);
                return 0;
            };
            st.fixed = Some(std::slice::from_raw_parts(data, n).to_vec());
        }
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    }
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

/// CryptHashSessionKey : ajoute au condensat les octets de la clé de
/// session, dans l'ordre inverse sauf CRYPT_LITTLE_ENDIAN (1) — la
/// documentation dit que la clé est condensée en gros-boutiste par défaut,
/// et elle est gardée ici dans l'ordre où l'application l'a donnée (#1512).
pub unsafe extern "win64" fn crypt_hash_session_key(h: usize, key: usize, flags: u32) -> i32 {
    let Some(mut bytes) = super::lock(keys()).get(&key).map(|k| k.key.clone()) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    if flags & 1 == 0 {
        bytes.reverse();
    }
    let mut map = super::lock(hashes());
    let Some(st) = map.get_mut(&h) else {
        set_last_error(NTE_BAD_HASH);
        return 0;
    };
    st.data.extend_from_slice(&bytes);
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
            ..Default::default()
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
        prov: st.prov,
        hmac: st.hmac.clone(),
        fixed: st.fixed.clone(),
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

// ---------- Clés symétriques ----------

const CALG_DES: u32 = 0x6601;
const CALG_3DES: u32 = 0x6603;
const CALG_3DES_112: u32 = 0x6609;
const CALG_AES_128: u32 = 0x660e;
const CALG_AES_192: u32 = 0x660f;
const CALG_AES_256: u32 = 0x6610;
const CALG_AES: u32 = 0x6611;
const CALG_RC4: u32 = 0x6801;
const PLAINTEXTKEYBLOB: u8 = 8;
const CRYPT_MODE_CBC: u32 = 1;
const CRYPT_MODE_ECB: u32 = 2;

#[derive(Clone)]
struct Key {
    algid: u32,
    /// Clé telle que l'application l'a donnée (8, 16, 24 ou 32 octets ; RC4 : 1 à 256).
    key: Vec<u8>,
    mode: u32,
    iv: Vec<u8>,
    /// Bloc de chaînage courant (CBC) ; revient à `iv` après l'appel final.
    chain: Vec<u8>,
    /// État RC4 (S, i, j), recréé après l'appel final.
    rc4: Option<([u8; 256], u8, u8)>,
}

fn keys() -> &'static Mutex<HashMap<usize, Key>> {
    static K: OnceLock<Mutex<HashMap<usize, Key>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Taille de bloc (0 pour RC4) et longueurs de clé admises, en octets.
fn alg_shape(algid: u32) -> Option<(usize, &'static [usize])> {
    Some(match algid {
        CALG_DES => (8, &[8]),
        CALG_3DES => (8, &[24]),
        CALG_3DES_112 => (8, &[16]),
        CALG_AES_128 => (16, &[16]),
        CALG_AES_192 => (16, &[24]),
        CALG_AES_256 => (16, &[32]),
        CALG_AES => (16, &[16, 24, 32]),
        CALG_RC4 => (0, &[5, 7, 8, 10, 12, 13, 16, 24, 32]),
        _ => return None,
    })
}

fn rc4_init(key: &[u8]) -> ([u8; 256], u8, u8) {
    let mut s = [0u8; 256];
    for (i, v) in s.iter_mut().enumerate() {
        *v = i as u8;
    }
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    (s, 0, 0)
}

fn rc4_apply(st: &mut ([u8; 256], u8, u8), data: &mut [u8]) {
    let (s, i, j) = st;
    for b in data {
        *i = i.wrapping_add(1);
        *j = j.wrapping_add(s[*i as usize]);
        s.swap(*i as usize, *j as usize);
        *b ^= s[s[*i as usize].wrapping_add(s[*j as usize]) as usize];
    }
}

fn new_key(algid: u32, key: &[u8]) -> Result<usize, u32> {
    let (block, sizes) = alg_shape(algid).ok_or(NTE_BAD_ALGID)?;
    if algid == CALG_RC4 {
        if key.is_empty() || key.len() > 256 {
            return Err(NTE_BAD_LEN);
        }
    } else if !sizes.contains(&key.len()) {
        return Err(NTE_BAD_LEN);
    }
    let algid = match (algid, key.len()) {
        (CALG_AES, 16) => CALG_AES_128,
        (CALG_AES, 24) => CALG_AES_192,
        (CALG_AES, 32) => CALG_AES_256,
        (a, _) => a,
    };
    let k = Key {
        algid,
        key: key.to_vec(),
        mode: CRYPT_MODE_CBC,
        iv: vec![0; block],
        chain: vec![0; block],
        rc4: (algid == CALG_RC4).then(|| rc4_init(key)),
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(keys()).insert(id, k);
    Ok(id)
}

/// Nom OpenSSL et clé effective pour un chiffrement par blocs.
fn openssl_cipher(k: &Key) -> Option<(&'static std::ffi::CStr, Vec<u8>)> {
    let cbc = k.mode == CRYPT_MODE_CBC;
    let key = match k.algid {
        CALG_DES => [&k.key[..], &k.key[..], &k.key[..]].concat(),
        CALG_3DES_112 => [&k.key[..], &k.key[..8]].concat(),
        _ => k.key.clone(),
    };
    let name = match (k.algid, cbc) {
        (CALG_DES | CALG_3DES | CALG_3DES_112, true) => c"DES-EDE3-CBC",
        (CALG_DES | CALG_3DES | CALG_3DES_112, false) => c"DES-EDE3-ECB",
        (CALG_AES_128, true) => c"AES-128-CBC",
        (CALG_AES_128, false) => c"AES-128-ECB",
        (CALG_AES_192, true) => c"AES-192-CBC",
        (CALG_AES_192, false) => c"AES-192-ECB",
        (CALG_AES_256, true) => c"AES-256-CBC",
        (CALG_AES_256, false) => c"AES-256-ECB",
        _ => return None,
    };
    Some((name, key))
}

/// Chiffre ou déchiffre `data` (longueur multiple du bloc) en tenant le chaînage.
fn run_blocks(k: &mut Key, enc: bool, data: &mut [u8]) -> Result<(), u32> {
    if data.is_empty() {
        return Ok(());
    }
    if k.mode != CRYPT_MODE_CBC && k.mode != CRYPT_MODE_ECB {
        return Err(NTE_BAD_ALGID);
    }
    let (name, key) = openssl_cipher(k).ok_or(NTE_BAD_ALGID)?;
    let bs = k.chain.len();
    let last_cipher = if enc {
        None
    } else {
        Some(data[data.len() - bs..].to_vec())
    };
    if !super::hostssl::cipher(name, &key, &k.chain, enc, data) {
        return Err(NTE_BAD_KEY);
    }
    if k.mode == CRYPT_MODE_CBC {
        k.chain = last_cipher.unwrap_or_else(|| data[data.len() - bs..].to_vec());
    }
    Ok(())
}

fn reset(k: &mut Key) {
    k.chain = k.iv.clone();
    if k.algid == CALG_RC4 {
        k.rc4 = Some(rc4_init(&k.key));
    }
}

pub unsafe extern "win64" fn crypt_import_key(
    prov: usize,
    data: *const u8,
    len: u32,
    pub_key: usize,
    flags: u32,
    out: *mut usize,
) -> i32 {
    if data.is_null() || out.is_null() || len < 12 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let blob = std::slice::from_raw_parts(data, len as usize);
    if let Some(r) = super::advapi32_rsa::import_key(prov, blob, flags, out) {
        return r;
    }
    if blob[0] == SIMPLEBLOB {
        // BLOBHEADER, ALG_ID de la clé d'échange, puis la clé de session
        // chiffrée par RSA (petit-boutiste).
        let algid = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        let plain = match super::advapi32_rsa::unwrap_key(pub_key, &blob[12..]) {
            Ok(p) => p,
            Err(e) => {
                set_last_error(e);
                return 0;
            }
        };
        return match new_key(algid, &plain) {
            Ok(id) => {
                *out = id;
                1
            }
            Err(e) => {
                set_last_error(e);
                0
            }
        };
    }
    // BLOBHEADER { bType, bVersion, reserved, aiKeyAlg } puis, pour
    // PLAINTEXTKEYBLOB, dwKeySize et la clé en clair.
    if blob[0] != PLAINTEXTKEYBLOB || pub_key != 0 {
        set_last_error(NTE_BAD_TYPE);
        return 0;
    }
    let algid = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
    let size = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]) as usize;
    if 12 + size > blob.len() {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    if flags & CRYPT_IPSEC_HMAC_KEY != 0 {
        // Clé HMAC de longueur libre (souvent déclarée CALG_RC2) : elle ne
        // sert qu'à CryptCreateHash(CALG_HMAC).
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        super::lock(keys()).insert(
            id,
            Key {
                algid,
                key: blob[12..12 + size].to_vec(),
                mode: CRYPT_MODE_CBC,
                iv: vec![0; 8],
                chain: vec![0; 8],
                rc4: None,
            },
        );
        *out = id;
        return 1;
    }
    match new_key(algid, &blob[12..12 + size]) {
        Ok(id) => {
            *out = id;
            1
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_gen_key(
    prov: usize,
    algid: u32,
    flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if let Some(r) = super::advapi32_rsa::gen_key(prov, algid, flags, out) {
        return r;
    }
    let Some((_, sizes)) = alg_shape(algid) else {
        set_last_error(NTE_BAD_ALGID);
        return 0;
    };
    // Longueur en bits dans les 16 bits hauts de dwFlags, sinon la plus courante.
    let bits = (flags >> 16) as usize;
    let len = if bits == 0 {
        if algid == CALG_RC4 {
            16
        } else {
            sizes[sizes.len() - 1]
        }
    } else {
        bits / 8
    };
    let mut key = vec![0u8; len];
    if super::bcrypt::bcrypt_gen_random(0, key.as_mut_ptr(), len as u32, 2) != 0 {
        set_last_error(NTE_BAD_FLAGS);
        return 0;
    }
    match new_key(algid, &key) {
        Ok(id) => {
            *out = id;
            1
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

/// CryptDeriveKey : clé tirée d'un condensat (extension 0x36/0x5c de la
/// CryptoAPI quand le condensat est plus court que la clé).
pub unsafe extern "win64" fn crypt_derive_key(
    _prov: usize,
    algid: u32,
    hash: usize,
    flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (value, halg) = {
        let map = super::lock(hashes());
        let Some(h) = map.get(&hash) else {
            set_last_error(NTE_BAD_HASH);
            return 0;
        };
        (digest(h), h.algid)
    };
    let (Some(value), Some((_, sizes))) = (value, alg_shape(algid)) else {
        set_last_error(NTE_BAD_ALGID);
        return 0;
    };
    let bits = (flags >> 16) as usize;
    let len = if bits != 0 {
        bits / 8
    } else if algid == CALG_RC4 {
        16
    } else {
        sizes[sizes.len() - 1]
    };
    let material = if len <= value.len() {
        value[..len].to_vec()
    } else {
        let mut b1 = [0x36u8; 64];
        let mut b2 = [0x5cu8; 64];
        for (i, v) in value.iter().enumerate() {
            b1[i] ^= v;
            b2[i] ^= v;
        }
        let d = |b: &[u8]| {
            digest(&Hash {
                algid: halg,
                data: b.to_vec(),
                ..Default::default()
            })
            .unwrap_or_default()
        };
        [d(&b1), d(&b2)].concat()[..len.min(2 * value.len())].to_vec()
    };
    match new_key(algid, &material) {
        Ok(id) => {
            *out = id;
            1
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_encrypt(
    key: usize,
    hash: usize,
    final_: i32,
    flags: u32,
    data: *mut u8,
    len: *mut u32,
    buf_len: u32,
) -> i32 {
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if super::advapi32_rsa::is_rsa(key) {
        return super::advapi32_rsa::encrypt(key, final_, flags, data, len, buf_len);
    }
    let mut map = super::lock(keys());
    let Some(k) = map.get_mut(&key) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    let n = *len as usize;
    let bs = k.chain.len();
    let needed = if bs != 0 && final_ != 0 {
        n + bs - n % bs
    } else {
        n
    };
    if data.is_null() {
        *len = needed as u32;
        return 1;
    }
    if needed > buf_len as usize {
        *len = needed as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    if bs != 0 && final_ == 0 && !n.is_multiple_of(bs) {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    if hash != 0 {
        crypt_hash_data(hash, data, n as u32, 0);
    }
    let buf = std::slice::from_raw_parts_mut(data, needed);
    let result = if bs == 0 {
        if let Some(st) = k.rc4.as_mut() {
            rc4_apply(st, buf);
        }
        Ok(())
    } else {
        let pad = (needed - n) as u8;
        for b in &mut buf[n..] {
            *b = pad;
        }
        run_blocks(k, true, buf)
    };
    if final_ != 0 {
        reset(k);
    }
    match result {
        Ok(()) => {
            *len = needed as u32;
            1
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_decrypt(
    key: usize,
    hash: usize,
    final_: i32,
    flags: u32,
    data: *mut u8,
    len: *mut u32,
) -> i32 {
    if len.is_null() || (data.is_null() && *len != 0) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if super::advapi32_rsa::is_rsa(key) {
        return super::advapi32_rsa::decrypt(key, final_, flags, data, len);
    }
    let mut map = super::lock(keys());
    let Some(k) = map.get_mut(&key) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    let n = *len as usize;
    let bs = k.chain.len();
    if bs != 0 && !n.is_multiple_of(bs) {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    let buf: &mut [u8] = if n == 0 {
        &mut []
    } else {
        std::slice::from_raw_parts_mut(data, n)
    };
    let mut out = n;
    let result = if bs == 0 {
        if let Some(st) = k.rc4.as_mut() {
            rc4_apply(st, buf);
        }
        Ok(())
    } else {
        run_blocks(k, false, buf).and_then(|()| {
            if final_ == 0 {
                return Ok(());
            }
            let pad = *buf.last().ok_or(NTE_BAD_DATA)? as usize;
            if pad == 0 || pad > bs || buf[n - pad..].iter().any(|&b| b as usize != pad) {
                return Err(NTE_BAD_DATA);
            }
            out = n - pad;
            Ok(())
        })
    };
    if final_ != 0 {
        reset(k);
    }
    match result {
        Ok(()) => {
            if hash != 0 {
                crypt_hash_data(hash, data, out as u32, 0);
            }
            *len = out as u32;
            1
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_set_key_param(
    key: usize,
    param: u32,
    data: *const u8,
    _flags: u32,
) -> i32 {
    let mut map = super::lock(keys());
    let Some(k) = map.get_mut(&key) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    if data.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match param {
        // KP_IV
        1 => {
            let bs = k.iv.len();
            k.iv = std::slice::from_raw_parts(data, bs).to_vec();
            k.chain = k.iv.clone();
        }
        // KP_PADDING : seul PKCS5_PADDING existe.
        3 => {}
        // KP_MODE
        4 => k.mode = (data as *const u32).read_unaligned(),
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    }
    1
}

pub unsafe extern "win64" fn crypt_get_key_param(
    key: usize,
    param: u32,
    out: *mut u8,
    len: *mut u32,
    _flags: u32,
) -> i32 {
    if super::advapi32_rsa::is_rsa(key) {
        if len.is_null() {
            set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        return super::advapi32_rsa::get_param(key, param, out, len);
    }
    let map = super::lock(keys());
    let Some(k) = map.get(&key) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let value: Vec<u8> = match param {
        1 => k.iv.clone(),
        3 => 1u32.to_le_bytes().to_vec(),
        4 => k.mode.to_le_bytes().to_vec(),
        7 => k.algid.to_le_bytes().to_vec(),
        8 => ((k.iv.len() * 8) as u32).to_le_bytes().to_vec(),
        9 => ((k.key.len() * 8) as u32).to_le_bytes().to_vec(),
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

pub unsafe extern "win64" fn crypt_duplicate_key(
    key: usize,
    _reserved: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if super::advapi32_rsa::is_rsa(key) {
        if out.is_null() {
            set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        *out = super::advapi32_rsa::duplicate(key);
        return (*out != 0) as i32;
    }
    let mut map = super::lock(keys());
    let Some(k) = map.get(&key).cloned() else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    map.insert(id, k);
    *out = id;
    1
}

pub extern "win64" fn crypt_destroy_key(key: usize) -> i32 {
    if super::advapi32_rsa::is_rsa(key) {
        return super::advapi32_rsa::crypt_destroy_key(key);
    }
    if super::lock(keys()).remove(&key).is_some() {
        1
    } else {
        set_last_error(NTE_BAD_KEY);
        0
    }
}

/// CryptExportKey d'une clé symétrique : PLAINTEXTKEYBLOB, ou SIMPLEBLOB
/// (clé chiffrée par la clé RSA publique `exp_key`).
pub(crate) unsafe fn export_symmetric(
    key: usize,
    exp_key: usize,
    blob_type: u32,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(k) = super::lock(keys()).get(&key).cloned() else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    let blob = match (blob_type as u8, exp_key) {
        (PLAINTEXTKEYBLOB, 0) => {
            let mut b = vec![PLAINTEXTKEYBLOB, 2, 0, 0];
            b.extend_from_slice(&k.algid.to_le_bytes());
            b.extend_from_slice(&(k.key.len() as u32).to_le_bytes());
            b.extend_from_slice(&k.key);
            b
        }
        (SIMPLEBLOB, e) if e != 0 => match super::advapi32_rsa::wrap_key(e, &k.key) {
            Ok(wrapped) => {
                let mut b = vec![SIMPLEBLOB, 2, 0, 0];
                b.extend_from_slice(&k.algid.to_le_bytes());
                b.extend_from_slice(&0xa400u32.to_le_bytes());
                b.extend_from_slice(&wrapped);
                b
            }
            Err(e) => {
                set_last_error(e);
                return 0;
            }
        },
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    };
    if out.is_null() {
        *len = blob.len() as u32;
        return 1;
    }
    if (*len as usize) < blob.len() {
        *len = blob.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(blob.as_ptr(), out, blob.len());
    *len = blob.len() as u32;
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CryptAcquireContextW" => fn super::advapi32_rsa::crypt_acquire_context_w,
        "CryptAcquireContextA" => fn super::advapi32_rsa::crypt_acquire_context_a,
        "CryptReleaseContext" => fn super::advapi32_rsa::crypt_release_context,
        "CryptContextAddRef" => fn super::advapi32_rsa::crypt_context_add_ref,
        "CryptGenRandom" => fn super::advapi32_rsa::crypt_gen_random,
        "CryptGetProvParam" => fn super::advapi32_rsa::crypt_get_prov_param,
        "CryptSetProvParam" => fn super::advapi32_rsa::crypt_set_prov_param,
        "CryptGetUserKey" => fn super::advapi32_rsa::crypt_get_user_key,
        "CryptExportKey" => fn super::advapi32_rsa::crypt_export_key,
        "CryptSignHashW" => fn super::advapi32_rsa::crypt_sign_hash_w,
        "CryptSignHashA" => fn super::advapi32_rsa::crypt_sign_hash_a,
        "CryptVerifySignatureW" => fn super::advapi32_rsa::crypt_verify_signature_w,
        "CryptVerifySignatureA" => fn super::advapi32_rsa::crypt_verify_signature_a,
        "CryptCreateHash" => fn crypt_create_hash,
        "CryptHashData" => fn crypt_hash_data,
        "CryptHashSessionKey" => fn crypt_hash_session_key,
        "CryptGetHashParam" => fn crypt_get_hash_param,
        "CryptSetHashParam" => fn crypt_set_hash_param,
        "CryptDuplicateHash" => fn crypt_duplicate_hash,
        "CryptDestroyHash" => fn crypt_destroy_hash,
        "CryptImportKey" => fn crypt_import_key,
        "CryptGenKey" => fn crypt_gen_key,
        "CryptDeriveKey" => fn crypt_derive_key,
        "CryptEncrypt" => fn crypt_encrypt,
        "CryptDecrypt" => fn crypt_decrypt,
        "CryptSetKeyParam" => fn crypt_set_key_param,
        "CryptGetKeyParam" => fn crypt_get_key_param,
        "CryptDuplicateKey" => fn crypt_duplicate_key,
        "CryptDestroyKey" => fn crypt_destroy_key,
        "SystemFunction040" => fn super::dpapi::rtl_encrypt_memory,
        "SystemFunction041" => fn super::dpapi::rtl_decrypt_memory,
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

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    unsafe fn import(algid: u32, key: &[u8]) -> usize {
        let mut blob = vec![8u8, 2, 0, 0];
        blob.extend_from_slice(&algid.to_le_bytes());
        blob.extend_from_slice(&(key.len() as u32).to_le_bytes());
        blob.extend_from_slice(key);
        let mut k = 0usize;
        assert_eq!(
            crypt_import_key(1, blob.as_ptr(), blob.len() as u32, 0, 0, &mut k),
            1
        );
        k
    }

    #[test]
    fn md4_vecteurs() {
        assert_eq!(hex(&md4(b"")), "31d6cfe0d16ae931b73c59d7e0c089c0");
        assert_eq!(hex(&md4(b"abc")), "a448017aaf21d8525fc10ae87aa6729d");
        assert_eq!(
            hex(&md4(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "e33b4ddc9c38f2199c3e7b164fcc0536"
        );
    }

    #[test]
    fn des_comme_le_ntlm_de_curl() {
        if super::super::hostssl::ssl().is_none() {
            return;
        }
        let _teb = crate::process::test_teb();
        unsafe {
            let k = import(CALG_DES, &[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
            let mut block = *b"Now is t";
            let mut n = 8u32;
            assert_eq!(crypt_encrypt(k, 0, 0, 0, block.as_mut_ptr(), &mut n, 8), 1);
            assert_eq!(hex(&block), "3fa40e8a984d4815");
            assert_eq!(crypt_destroy_key(k), 1);
        }
    }

    #[test]
    fn aes_ecb_et_cbc_avec_bourrage() {
        if super::super::hostssl::ssl().is_none() {
            return;
        }
        let _teb = crate::process::test_teb();
        unsafe {
            let key: Vec<u8> = (0u8..16).collect();
            let k = import(CALG_AES_128, &key);
            let mode = CRYPT_MODE_ECB;
            assert_eq!(
                crypt_set_key_param(k, 4, &mode as *const u32 as *const u8, 0),
                1
            );
            let mut block = [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff,
            ];
            let mut n = 16u32;
            assert_eq!(crypt_encrypt(k, 0, 0, 0, block.as_mut_ptr(), &mut n, 16), 1);
            assert_eq!(hex(&block), "69c4e0d86a7b0430d8cdb78070b4c55a");

            let c = import(CALG_AES, &key);
            let mut buf = [0u8; 32];
            buf[..5].copy_from_slice(b"hello");
            let mut n = 5u32;
            // Taille demandée sans tampon, puis tampon trop court.
            assert_eq!(
                crypt_encrypt(c, 0, 1, 0, std::ptr::null_mut(), &mut n, 0),
                1
            );
            assert_eq!(n, 16);
            n = 5;
            assert_eq!(crypt_encrypt(c, 0, 1, 0, buf.as_mut_ptr(), &mut n, 8), 0);
            n = 5;
            assert_eq!(crypt_encrypt(c, 0, 1, 0, buf.as_mut_ptr(), &mut n, 32), 1);
            assert_eq!(n, 16);
            assert_ne!(&buf[..5], b"hello");
            assert_eq!(crypt_decrypt(c, 0, 1, 0, buf.as_mut_ptr(), &mut n), 1);
            assert_eq!(n, 5);
            assert_eq!(&buf[..5], b"hello");
            let mut bits = [0u8; 4];
            let mut n = 4u32;
            assert_eq!(crypt_get_key_param(c, 9, bits.as_mut_ptr(), &mut n, 0), 1);
            assert_eq!(u32::from_le_bytes(bits), 128);
            assert_eq!(crypt_destroy_key(k), 1);
            assert_eq!(crypt_destroy_key(c), 1);
            assert_eq!(crypt_destroy_key(c), 0);
        }
    }

    #[test]
    fn hmac_sha256_rfc4231() {
        let _teb = crate::process::test_teb();
        unsafe {
            // Clé HMAC de longueur libre, déclarée CALG_RC2 comme le font les applis.
            let key = [0x0bu8; 20];
            let mut blob = vec![8u8, 2, 0, 0];
            blob.extend_from_slice(&0x6602u32.to_le_bytes());
            blob.extend_from_slice(&20u32.to_le_bytes());
            blob.extend_from_slice(&key);
            let mut k = 0usize;
            assert_eq!(
                crypt_import_key(
                    1,
                    blob.as_ptr(),
                    blob.len() as u32,
                    0,
                    CRYPT_IPSEC_HMAC_KEY,
                    &mut k
                ),
                1
            );
            let mut h = 0usize;
            assert_eq!(crypt_create_hash(1, CALG_HMAC, k, 0, &mut h), 1);
            // HMAC_INFO { CALG_SHA_256, null, 0, null, 0 }
            let mut info = [0u8; 40];
            info[..4].copy_from_slice(&0x800cu32.to_le_bytes());
            assert_eq!(crypt_set_hash_param(h, HP_HMAC_INFO, info.as_ptr(), 0), 1);
            assert_eq!(crypt_hash_data(h, b"Hi There".as_ptr(), 8, 0), 1);
            let mut out = [0u8; 32];
            let mut n = 32u32;
            assert_eq!(crypt_get_hash_param(h, 2, out.as_mut_ptr(), &mut n, 0), 1);
            assert_eq!(
                hex(&out),
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
            );
            // Une clé plus longue que le bloc est d'abord condensée (cas 6).
            let long = [0xaau8; 131];
            assert_eq!(
                hex(&hmac(
                    0x800c,
                    &long,
                    b"Test Using Larger Than Block-Size Key - Hash Key First"
                )
                .unwrap()),
                "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
            );
            // Valeur imposée.
            let fixed = [7u8; 32];
            assert_eq!(crypt_set_hash_param(h, HP_HASHVAL, fixed.as_ptr(), 0), 1);
            assert_eq!(crypt_get_hash_param(h, 2, out.as_mut_ptr(), &mut n, 0), 1);
            assert_eq!(out, fixed);
            crypt_destroy_hash(h);
            crypt_destroy_key(k);
        }
    }

    #[test]
    fn rc4_et_cle_derivee() {
        let _teb = crate::process::test_teb();
        unsafe {
            let k = import(CALG_RC4, b"Key");
            let mut data = *b"Plaintext";
            let mut n = 9u32;
            assert_eq!(crypt_encrypt(k, 0, 1, 0, data.as_mut_ptr(), &mut n, 9), 1);
            assert_eq!(hex(&data), "bbf316e8d940af0ad3");
            let mut n = 9u32;
            assert_eq!(crypt_decrypt(k, 0, 1, 0, data.as_mut_ptr(), &mut n), 1);
            assert_eq!(&data, b"Plaintext");
            // Mauvais blob et algorithme inconnu.
            let bad = [6u8, 2, 0, 0, 1, 0x68, 0, 0, 3, 0, 0, 0, 1, 2, 3];
            let mut out = 0usize;
            assert_eq!(
                crypt_import_key(1, bad.as_ptr(), bad.len() as u32, 0, 0, &mut out),
                0
            );
            let mut h = 0usize;
            assert_eq!(crypt_create_hash(1, CALG_MD4, 0, 0, &mut h), 1);
            assert_eq!(crypt_hash_data(h, b"abc".as_ptr(), 3, 0), 1);
            let mut rc4 = 0usize;
            assert_eq!(crypt_derive_key(1, CALG_RC4, h, 0, &mut rc4), 1);
            let mut bits = [0u8; 4];
            let mut n = 4u32;
            assert_eq!(crypt_get_key_param(rc4, 9, bits.as_mut_ptr(), &mut n, 0), 1);
            assert_eq!(u32::from_le_bytes(bits), 128);
            assert_eq!(crypt_destroy_key(rc4), 1);
            assert_eq!(crypt_destroy_hash(h), 1);
        }
    }
}
