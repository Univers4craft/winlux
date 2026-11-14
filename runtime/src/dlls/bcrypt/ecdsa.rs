//! ECDSA de CNG (#2173) : poignées d'algorithme « ECDSA_P256 »,
//! « ECDSA_P384 », « ECDSA_P521 » et « ECDSA » (courbe donnée par la clé),
//! import de clé publique (BCryptImportKeyPair, ECCPUBLICBLOB) et
//! vérification de signature (BCryptVerifySignature). ISCC d'Inno Setup 6.7
//! vérifie ainsi la signature ISSig de ses DLL avant de les charger.
//!
//! Sources publiques : documentation de bcrypt.h (BCRYPT_ECCKEY_BLOB
//! { dwMagic, cbKey } suivi de X puis Y, gros-boutistes ; signature r‖s ;
//! constantes des en-têtes mingw-w64), RFC 5480 (SubjectPublicKeyInfo et
//! OID des courbes), RFC 3279 (signature ECDSA en DER). Le calcul passe par
//! l'OpenSSL de l'hôte (hostssl), comme les clés RSA de la CryptoAPI.

use super::super::hostssl;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
const STATUS_INVALID_SIGNATURE: i32 = 0xC000_A000u32 as i32;

/// Courbes NIST : taille d'une coordonnée (cbKey), magie de clé publique,
/// OID DER de la courbe (RFC 5480 §2.1.1.1).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Curve {
    P256,
    P384,
    P521,
}

impl Curve {
    fn size(self) -> usize {
        match self {
            Curve::P256 => 32,
            Curve::P384 => 48,
            Curve::P521 => 66,
        }
    }
    fn bits(self) -> u32 {
        match self {
            Curve::P256 => 256,
            Curve::P384 => 384,
            Curve::P521 => 521,
        }
    }
    /// BCRYPT_ECDSA_PUBLIC_P256_MAGIC (« ECS1 »), P384 (« ECS3 »), P521 (« ECS5 »).
    fn public_magic(self) -> u32 {
        match self {
            Curve::P256 => 0x3153_4345,
            Curve::P384 => 0x3353_4345,
            Curve::P521 => 0x3553_4345,
        }
    }
    fn oid(self) -> &'static [u8] {
        match self {
            // 1.2.840.10045.3.1.7 (secp256r1)
            Curve::P256 => &[0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07],
            // 1.3.132.0.34 (secp384r1)
            Curve::P384 => &[0x06, 0x05, 0x2b, 0x81, 0x04, 0x00, 0x22],
            // 1.3.132.0.35 (secp521r1)
            Curve::P521 => &[0x06, 0x05, 0x2b, 0x81, 0x04, 0x00, 0x23],
        }
    }
    fn from_size(n: usize) -> Option<Curve> {
        [Curve::P256, Curve::P384, Curve::P521]
            .into_iter()
            .find(|c| c.size() == n)
    }
    fn name(self) -> &'static str {
        match self {
            Curve::P256 => "ECDSA_P256",
            Curve::P384 => "ECDSA_P384",
            Curve::P521 => "ECDSA_P521",
        }
    }
}

/// BCRYPT_ECDSA_PUBLIC_GENERIC_MAGIC (« ECDP »).
const PUBLIC_GENERIC_MAGIC: u32 = 0x5044_4345;

struct Key {
    curve: Curve,
    /// EVP_PKEY de l'OpenSSL de l'hôte.
    pkey: usize,
}

#[derive(Default)]
struct State {
    /// Poignée d'algorithme → courbe imposée (None : « ECDSA », générique).
    algs: HashMap<usize, Option<Curve>>,
    keys: HashMap<usize, Key>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

static SEQ: AtomicUsize = AtomicUsize::new(1);
/// Poignées ECDSA sous 4 Gio (programmes 32 bits), distinctes de celles
/// d'AES (0x4…, 0x2…) et des condensats.
const ALG_TAG: usize = 0x1000_0000;
const KEY_TAG: usize = 0x3000_0000;

fn next(tag: usize) -> usize {
    tag | ((SEQ.fetch_add(1, Ordering::Relaxed) & 0xF_FFFF) << 4)
}

/// BCryptOpenAlgorithmProvider : noms ECDSA reconnus.
pub(super) fn open(name: &str) -> Option<usize> {
    let curve = match name {
        "ECDSA_P256" => Some(Curve::P256),
        "ECDSA_P384" => Some(Curve::P384),
        "ECDSA_P521" => Some(Curve::P521),
        "ECDSA" => None,
        _ => return None,
    };
    let h = next(ALG_TAG);
    state().algs.insert(h, curve);
    Some(h)
}

pub(super) fn close(h: usize) -> bool {
    state().algs.remove(&h).is_some()
}

/// Destruction d'une clé ECDSA ; `None` si la poignée n'en est pas une.
pub(super) fn destroy_key(h: usize) -> Option<i32> {
    let key = state().keys.remove(&h)?;
    if let Some(s) = hostssl::ssl() {
        unsafe { (s.EVP_PKEY_free)(key.pkey as hostssl::P) };
    }
    Some(0)
}

fn der_len(n: usize, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.extend([0x81, n as u8]);
    } else {
        out.extend([0x82, (n >> 8) as u8, n as u8]);
    }
}

fn der(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    der_len(body.len(), &mut v);
    v.extend_from_slice(body);
    v
}

/// SubjectPublicKeyInfo d'une clé EC (RFC 5480) : id-ecPublicKey, courbe,
/// point non compressé 04‖X‖Y.
fn spki(curve: Curve, x: &[u8], y: &[u8]) -> Vec<u8> {
    // 1.2.840.10045.2.1 (id-ecPublicKey)
    let mut alg = vec![0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
    alg.extend_from_slice(curve.oid());
    let mut bits = vec![0x00, 0x04];
    bits.extend_from_slice(x);
    bits.extend_from_slice(y);
    let mut body = der(0x30, &alg);
    body.extend(der(0x03, &bits));
    der(0x30, &body)
}

/// Entier DER positif à partir d'octets gros-boutistes.
fn der_uint(b: &[u8]) -> Vec<u8> {
    let start = b
        .iter()
        .position(|&c| c != 0)
        .unwrap_or(b.len().saturating_sub(1));
    let mut body = Vec::new();
    if b.get(start).is_some_and(|&c| c & 0x80 != 0) {
        body.push(0);
    }
    body.extend_from_slice(&b[start..]);
    der(0x02, &body)
}

/// Signature CNG r‖s → Ecdsa-Sig-Value DER (RFC 3279 §2.2.3).
fn sig_der(sig: &[u8]) -> Vec<u8> {
    let (r, s) = sig.split_at(sig.len() / 2);
    let mut body = der_uint(r);
    body.extend(der_uint(s));
    der(0x30, &body)
}

/// Clé publique d'un BCRYPT_ECCKEY_BLOB : courbe, X, Y.
fn parse_public_blob(alg_curve: Option<Curve>, blob: &[u8]) -> Result<(Curve, &[u8], &[u8]), i32> {
    if blob.len() < 8 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let magic = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
    let size = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]) as usize;
    let curve = Curve::from_size(size).ok_or(STATUS_NOT_SUPPORTED)?;
    if magic != curve.public_magic() && magic != PUBLIC_GENERIC_MAGIC {
        // Clé privée ou magie d'une autre famille (ECDH…).
        return Err(STATUS_NOT_SUPPORTED);
    }
    if alg_curve.is_some_and(|c| c != curve) || blob.len() < 8 + 2 * size {
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok((curve, &blob[8..8 + size], &blob[8 + size..8 + 2 * size]))
}

/// BCryptImportKeyPair pour une poignée ECDSA ; `None` si `alg` n'en est
/// pas une.
pub(super) unsafe fn import_key_pair(
    alg: usize,
    blob_type: &str,
    out: *mut usize,
    input: *const u8,
    len: u32,
) -> Option<i32> {
    let alg_curve = *state().algs.get(&alg)?;
    if out.is_null() || input.is_null() {
        return Some(STATUS_INVALID_PARAMETER);
    }
    // BCRYPT_ECCPUBLIC_BLOB, ou BCRYPT_PUBLIC_KEY_BLOB (type déduit de la magie).
    if blob_type != "ECCPUBLICBLOB" && blob_type != "PUBLICBLOB" {
        crate::trace!("BCryptImportKeyPair ECDSA : type {blob_type} non pris en charge");
        return Some(STATUS_NOT_SUPPORTED);
    }
    let blob = std::slice::from_raw_parts(input, len as usize);
    let (curve, x, y) = match parse_public_blob(alg_curve, blob) {
        Ok(k) => k,
        Err(e) => return Some(e),
    };
    let Some(s) = hostssl::ssl() else {
        return Some(STATUS_NOT_SUPPORTED);
    };
    let der = spki(curve, x, y);
    let mut p = der.as_ptr();
    let pkey = (s.d2i_PUBKEY)(std::ptr::null_mut(), &mut p, der.len() as std::ffi::c_long);
    if pkey.is_null() {
        // Point hors de la courbe.
        return Some(STATUS_INVALID_PARAMETER);
    }
    let h = next(KEY_TAG);
    state().keys.insert(
        h,
        Key {
            curve,
            pkey: pkey as usize,
        },
    );
    super::put_handle(out, h);
    Some(0)
}

/// BCryptVerifySignature pour une clé ECDSA ; `None` si `key` n'en est pas une.
pub(super) unsafe fn verify_signature(
    key: usize,
    hash: *const u8,
    hash_len: u32,
    sig: *const u8,
    sig_len: u32,
) -> Option<i32> {
    let st = state();
    let k = st.keys.get(&key)?;
    if hash.is_null() || sig.is_null() || hash_len == 0 {
        return Some(STATUS_INVALID_PARAMETER);
    }
    if sig_len as usize != 2 * k.curve.size() {
        return Some(STATUS_INVALID_SIGNATURE);
    }
    let Some(s) = hostssl::ssl() else {
        return Some(STATUS_NOT_SUPPORTED);
    };
    let der = sig_der(std::slice::from_raw_parts(sig, sig_len as usize));
    let ctx = (s.EVP_PKEY_CTX_new_from_pkey)(
        std::ptr::null_mut(),
        k.pkey as hostssl::P,
        std::ptr::null(),
    );
    if ctx.is_null() {
        return Some(STATUS_NOT_SUPPORTED);
    }
    let ok = (s.EVP_PKEY_verify_init)(ctx) == 1
        && (s.EVP_PKEY_verify)(ctx, der.as_ptr(), der.len(), hash, hash_len as usize) == 1;
    (s.EVP_PKEY_CTX_free)(ctx);
    Some(if ok { 0 } else { STATUS_INVALID_SIGNATURE })
}

/// BCryptGetProperty sur une poignée ou une clé ECDSA : AlgorithmName,
/// KeyLength (bits), KeyStrength, SignatureLength. `None` si la poignée
/// n'est pas ECDSA.
pub(super) unsafe fn get_property(
    h: usize,
    name: &str,
    out: *mut u8,
    out_len: u32,
    result: *mut u32,
) -> Option<i32> {
    let curve = {
        let st = state();
        match (st.algs.get(&h), st.keys.get(&h)) {
            (Some(c), _) => *c,
            (_, Some(k)) => Some(k.curve),
            _ => return None,
        }
    };
    let put_u32 = |v: u32| {
        *result = 4;
        if out.is_null() {
            return 0;
        }
        if out_len < 4 {
            return STATUS_BUFFER_TOO_SMALL;
        }
        std::ptr::write_unaligned(out as *mut u32, v);
        0
    };
    Some(match (name, curve) {
        ("KeyLength" | "KeyStrength", Some(c)) => put_u32(c.bits()),
        ("SignatureLength", Some(c)) => put_u32(2 * c.size() as u32),
        ("AlgorithmName", c) => {
            let text = c.map(Curve::name).unwrap_or("ECDSA");
            let w: Vec<u8> = text
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            *result = w.len() as u32;
            if out.is_null() {
                0
            } else if (out_len as usize) < w.len() {
                STATUS_BUFFER_TOO_SMALL
            } else {
                std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
                0
            }
        }
        _ => STATUS_NOT_SUPPORTED,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn der_des_entiers_et_de_la_signature() {
        assert_eq!(der_uint(&[0x00, 0x00, 0x7f]), vec![0x02, 0x01, 0x7f]);
        assert_eq!(der_uint(&[0x00, 0x80]), vec![0x02, 0x02, 0x00, 0x80]);
        assert_eq!(der_uint(&[0, 0]), vec![0x02, 0x01, 0x00]);
        // P-521 : contenu de plus de 127 octets, longueur en forme longue.
        let sig = sig_der(&[0xff; 132]);
        assert_eq!(&sig[..3], &[0x30, 0x81, 2 * (2 + 67)]);
        let k = spki(Curve::P256, &[1; 32], &[2; 32]);
        assert_eq!(k.len(), 91);
        assert_eq!(&k[..4], &[0x30, 0x59, 0x30, 0x13]);
    }

    #[test]
    fn verification_p256() {
        if hostssl::ssl().is_none() {
            return; // pas d'OpenSSL sur l'hôte de test
        }
        // Clé, condensat SHA-256 de « winlux #2173 » et signature produits
        // ici avec l'outil openssl (ecparam -genkey, pkeyutl -sign).
        let xy = hex(
            "329e0918ad401aa415e7b3538410597efaf3d03a5756ee1dab5fada27fc3bc66\
                      6aae6ef412e400e4dd0520da4d2cfb948ed33bfbe6dff118420542ef43445763",
        );
        let hash = hex("b3ba70516e5306ec98d77f022a31a02aa21fd8376cc31d9de95725d92cd1d514");
        let sig = hex(
            "a23993fbe33fe118cff135424c9dbde1ecc69a55ebce0ee88a4e4264d2e8a097\
                       8a8f006de66163723439c76dcd8796e37bbe07d28ef8e4b50fdbf1c1cac862a1",
        );
        let mut blob = Vec::new();
        blob.extend(0x3153_4345u32.to_le_bytes());
        blob.extend(32u32.to_le_bytes());
        blob.extend(&xy);
        unsafe {
            let alg = open("ECDSA_P256").unwrap();
            let mut key = 0usize;
            assert_eq!(
                import_key_pair(
                    alg,
                    "ECCPUBLICBLOB",
                    &mut key,
                    blob.as_ptr(),
                    blob.len() as u32
                ),
                Some(0)
            );
            assert_ne!(key, 0);
            let v = |h: &[u8], s: &[u8]| {
                verify_signature(key, h.as_ptr(), h.len() as u32, s.as_ptr(), s.len() as u32)
            };
            assert_eq!(v(&hash, &sig), Some(0));
            let mut bad = sig.clone();
            bad[10] ^= 1;
            assert_eq!(v(&hash, &bad), Some(STATUS_INVALID_SIGNATURE));
            let mut other = hash.clone();
            other[0] ^= 1;
            assert_eq!(v(&other, &sig), Some(STATUS_INVALID_SIGNATURE));
            assert_eq!(v(&hash, &sig[..63]), Some(STATUS_INVALID_SIGNATURE));
            let (mut bits, mut n) = (0u32, 0u32);
            assert_eq!(
                get_property(key, "KeyLength", (&mut bits as *mut u32).cast(), 4, &mut n),
                Some(0)
            );
            assert_eq!(bits, 256);
            // Clé P-384 refusée par un fournisseur P-256 ; générique accepté.
            let mut big = blob.clone();
            big[3] = 0x33; // « ECS3 » : clé publique P-384
            big[4] = 48;
            assert_eq!(
                import_key_pair(
                    alg,
                    "ECCPUBLICBLOB",
                    &mut key,
                    big.as_ptr(),
                    big.len() as u32
                ),
                Some(STATUS_INVALID_PARAMETER)
            );
            let generic = open("ECDSA").unwrap();
            let mut key2 = 0usize;
            assert_eq!(
                import_key_pair(
                    generic,
                    "ECCPUBLICBLOB",
                    &mut key2,
                    blob.as_ptr(),
                    blob.len() as u32
                ),
                Some(0)
            );
            assert_eq!(destroy_key(key2), Some(0));
            assert_eq!(destroy_key(key), Some(0));
            assert_eq!(destroy_key(key), None);
            assert!(close(alg) && close(generic));
            assert_eq!(open("ECDH_P256"), None);
        }
    }
}
