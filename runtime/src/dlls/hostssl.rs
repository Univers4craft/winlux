//! OpenSSL de l'hôte (libcrypto.so.3, libssl.so.3), chargé à la demande.
//!
//! Plomberie Linux standard, au même titre que le curl de WinHTTP : sert aux
//! condensats de la CryptoAPI (MD5, SHA-1, SHA-2), à la vérification des
//! chaînes de certificats (crypt32) et au TLS de Schannel (secur32). Si la
//! bibliothèque manque, chaque appelant renvoie une erreur propre.

#![allow(non_snake_case)]

use super::hostcall::sym;
use std::ffi::{c_char, c_int, c_long, c_void, CStr};
use std::sync::OnceLock;

pub type P = *mut c_void;

/// Quelques entrées (magasin, ALPN…) servent selon les appelants.
#[allow(dead_code)]
pub struct Ssl {
    // libcrypto
    pub EVP_get_digestbyname: unsafe extern "C" fn(*const c_char) -> P,
    pub EVP_Digest: unsafe extern "C" fn(*const c_void, usize, *mut u8, *mut u32, P, P) -> c_int,
    pub HMAC: unsafe extern "C" fn(
        P,
        *const c_void,
        c_int,
        *const u8,
        usize,
        *mut u8,
        *mut u32,
    ) -> *mut u8,
    pub d2i_X509: unsafe extern "C" fn(*mut P, *mut *const u8, c_long) -> P,
    pub i2d_X509: unsafe extern "C" fn(P, *mut *mut u8) -> c_int,
    pub X509_free: unsafe extern "C" fn(P),
    pub X509_STORE_new: unsafe extern "C" fn() -> P,
    pub X509_STORE_free: unsafe extern "C" fn(P),
    pub X509_STORE_set_default_paths: unsafe extern "C" fn(P) -> c_int,
    pub X509_STORE_add_cert: unsafe extern "C" fn(P, P) -> c_int,
    pub X509_STORE_set_flags: unsafe extern "C" fn(P, std::ffi::c_ulong) -> c_int,
    pub EVP_get_cipherbyname: unsafe extern "C" fn(*const c_char) -> P,
    pub d2i_PKCS12: unsafe extern "C" fn(*mut P, *mut *const u8, c_long) -> P,
    // clés RSA de la CryptoAPI
    pub EVP_PKEY_CTX_new_from_name: unsafe extern "C" fn(P, *const c_char, *const c_char) -> P,
    pub EVP_PKEY_CTX_new_from_pkey: unsafe extern "C" fn(P, P, *const c_char) -> P,
    pub EVP_PKEY_CTX_free: unsafe extern "C" fn(P),
    pub EVP_PKEY_keygen_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_CTX_set_rsa_keygen_bits: unsafe extern "C" fn(P, c_int) -> c_int,
    pub EVP_PKEY_generate: unsafe extern "C" fn(P, *mut P) -> c_int,
    pub EVP_PKEY_get_bn_param: unsafe extern "C" fn(P, *const c_char, *mut P) -> c_int,
    pub BN_bn2lebinpad: unsafe extern "C" fn(P, *mut u8, c_int) -> c_int,
    pub BN_lebin2bn: unsafe extern "C" fn(*const u8, c_int, P) -> P,
    pub BN_num_bits: unsafe extern "C" fn(P) -> c_int,
    pub BN_free: unsafe extern "C" fn(P),
    pub OSSL_PARAM_BLD_new: unsafe extern "C" fn() -> P,
    pub OSSL_PARAM_BLD_push_BN: unsafe extern "C" fn(P, *const c_char, P) -> c_int,
    pub OSSL_PARAM_BLD_to_param: unsafe extern "C" fn(P) -> P,
    pub OSSL_PARAM_BLD_free: unsafe extern "C" fn(P),
    pub OSSL_PARAM_free: unsafe extern "C" fn(P),
    pub EVP_PKEY_fromdata_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_fromdata: unsafe extern "C" fn(P, *mut P, c_int, P) -> c_int,
    pub EVP_PKEY_sign_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_sign: unsafe extern "C" fn(P, *mut u8, *mut usize, *const u8, usize) -> c_int,
    pub EVP_PKEY_verify_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_verify: unsafe extern "C" fn(P, *const u8, usize, *const u8, usize) -> c_int,
    pub EVP_PKEY_encrypt_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_encrypt: unsafe extern "C" fn(P, *mut u8, *mut usize, *const u8, usize) -> c_int,
    pub EVP_PKEY_decrypt_init: unsafe extern "C" fn(P) -> c_int,
    pub EVP_PKEY_decrypt: unsafe extern "C" fn(P, *mut u8, *mut usize, *const u8, usize) -> c_int,
    pub EVP_PKEY_CTX_set_rsa_padding: unsafe extern "C" fn(P, c_int) -> c_int,
    pub EVP_PKEY_CTX_set_signature_md: unsafe extern "C" fn(P, P) -> c_int,
    pub d2i_PUBKEY: unsafe extern "C" fn(*mut P, *mut *const u8, c_long) -> P,
    pub PKCS12_parse: unsafe extern "C" fn(P, *const c_char, *mut P, *mut P, *mut P) -> c_int,
    pub PKCS12_free: unsafe extern "C" fn(P),
    pub i2d_PrivateKey: unsafe extern "C" fn(P, *mut *mut u8) -> c_int,
    pub d2i_AutoPrivateKey: unsafe extern "C" fn(*mut P, *mut *const u8, c_long) -> P,
    pub EVP_CIPHER_CTX_new: unsafe extern "C" fn() -> P,
    pub EVP_CIPHER_CTX_free: unsafe extern "C" fn(P),
    pub EVP_CipherInit_ex: unsafe extern "C" fn(P, P, P, *const u8, *const u8, c_int) -> c_int,
    pub EVP_CIPHER_CTX_set_padding: unsafe extern "C" fn(P, c_int) -> c_int,
    pub EVP_CipherUpdate: unsafe extern "C" fn(P, *mut u8, *mut c_int, *const u8, c_int) -> c_int,
    pub X509_STORE_CTX_new: unsafe extern "C" fn() -> P,
    pub X509_STORE_CTX_free: unsafe extern "C" fn(P),
    pub X509_STORE_CTX_init: unsafe extern "C" fn(P, P, P, P) -> c_int,
    pub X509_STORE_CTX_get_error: unsafe extern "C" fn(P) -> c_int,
    pub X509_STORE_CTX_get1_chain: unsafe extern "C" fn(P) -> P,
    pub X509_STORE_CTX_set_time: unsafe extern "C" fn(P, std::ffi::c_ulong, libc::time_t),
    pub X509_verify_cert: unsafe extern "C" fn(P) -> c_int,
    pub X509_check_host:
        unsafe extern "C" fn(P, *const c_char, usize, u32, *mut *mut c_char) -> c_int,
    pub X509_check_ip_asc: unsafe extern "C" fn(P, *const c_char, u32) -> c_int,
    pub OPENSSL_sk_new_null: unsafe extern "C" fn() -> P,
    pub OPENSSL_sk_push: unsafe extern "C" fn(P, P) -> c_int,
    pub OPENSSL_sk_num: unsafe extern "C" fn(P) -> c_int,
    pub OPENSSL_sk_value: unsafe extern "C" fn(P, c_int) -> P,
    pub OPENSSL_sk_free: unsafe extern "C" fn(P),
    pub OPENSSL_sk_pop_free: unsafe extern "C" fn(P, unsafe extern "C" fn(P)),
    pub CRYPTO_free: unsafe extern "C" fn(*mut c_void, *const c_char, c_int),
    // condensats par morceaux et vérification de signature (MSIX)
    pub EVP_MD_CTX_new: unsafe extern "C" fn() -> P,
    pub EVP_MD_CTX_free: unsafe extern "C" fn(P),
    pub EVP_DigestInit_ex: unsafe extern "C" fn(P, P, P) -> c_int,
    pub EVP_DigestUpdate: unsafe extern "C" fn(P, *const c_void, usize) -> c_int,
    pub EVP_DigestFinal_ex: unsafe extern "C" fn(P, *mut u8, *mut u32) -> c_int,
    pub EVP_DigestVerifyInit: unsafe extern "C" fn(P, *mut P, P, P, P) -> c_int,
    pub EVP_DigestVerify: unsafe extern "C" fn(P, *const u8, usize, *const u8, usize) -> c_int,
    pub X509_get_pubkey: unsafe extern "C" fn(P) -> P,
    pub EVP_PKEY_free: unsafe extern "C" fn(P),
    // libssl
    pub TLS_client_method: unsafe extern "C" fn() -> P,
    pub SSL_CTX_new: unsafe extern "C" fn(P) -> P,
    pub SSL_CTX_free: unsafe extern "C" fn(P),
    pub SSL_CTX_set_default_verify_paths: unsafe extern "C" fn(P) -> c_int,
    pub SSL_CTX_set_verify: unsafe extern "C" fn(P, c_int, P),
    pub SSL_CTX_ctrl: unsafe extern "C" fn(P, c_int, c_long, P) -> c_long,
    pub SSL_new: unsafe extern "C" fn(P) -> P,
    pub SSL_free: unsafe extern "C" fn(P),
    pub SSL_set_bio: unsafe extern "C" fn(P, P, P),
    pub SSL_ctrl: unsafe extern "C" fn(P, c_int, c_long, P) -> c_long,
    pub SSL_set1_host: unsafe extern "C" fn(P, *const c_char) -> c_int,
    pub SSL_use_certificate: unsafe extern "C" fn(P, P) -> c_int,
    pub SSL_use_PrivateKey: unsafe extern "C" fn(P, P) -> c_int,
    pub SSL_set_connect_state: unsafe extern "C" fn(P),
    pub SSL_do_handshake: unsafe extern "C" fn(P) -> c_int,
    pub SSL_get_error: unsafe extern "C" fn(P, c_int) -> c_int,
    pub SSL_read: unsafe extern "C" fn(P, *mut c_void, c_int) -> c_int,
    pub SSL_write: unsafe extern "C" fn(P, *const c_void, c_int) -> c_int,
    pub SSL_shutdown: unsafe extern "C" fn(P) -> c_int,
    pub SSL_get1_peer_certificate: unsafe extern "C" fn(P) -> P,
    pub SSL_get_peer_cert_chain: unsafe extern "C" fn(P) -> P,
    pub SSL_get_verify_result: unsafe extern "C" fn(P) -> c_long,
    pub SSL_version: unsafe extern "C" fn(P) -> c_int,
    pub SSL_get_current_cipher: unsafe extern "C" fn(P) -> P,
    pub SSL_CIPHER_get_bits: unsafe extern "C" fn(P, *mut c_int) -> c_int,
    pub SSL_set_alpn_protos: unsafe extern "C" fn(P, *const u8, u32) -> c_int,
    pub SSL_get0_alpn_selected: unsafe extern "C" fn(P, *mut *const u8, *mut u32),
    pub BIO_s_mem: unsafe extern "C" fn() -> P,
    pub BIO_new: unsafe extern "C" fn(P) -> P,
    pub BIO_write: unsafe extern "C" fn(P, *const c_void, c_int) -> c_int,
    pub BIO_read: unsafe extern "C" fn(P, *mut c_void, c_int) -> c_int,
    pub BIO_ctrl: unsafe extern "C" fn(P, c_int, c_long, P) -> c_long,
}

unsafe impl Send for Ssl {}
unsafe impl Sync for Ssl {}

fn open(names: &[&CStr]) -> usize {
    names
        .iter()
        .map(|n| unsafe { libc::dlopen(n.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) as usize })
        .find(|&h| h != 0)
        .unwrap_or(0)
}

/// OpenSSL de l'hôte, ou None s'il manque (journalisé une fois).
#[allow(clippy::missing_transmute_annotations)]
pub fn ssl() -> Option<&'static Ssl> {
    static S: OnceLock<Option<Ssl>> = OnceLock::new();
    S.get_or_init(|| unsafe {
        let c = open(&[c"libcrypto.so.3", c"libcrypto.so"]);
        let s = open(&[c"libssl.so.3", c"libssl.so"]);
        if c == 0 || s == 0 {
            crate::log_warn!(
                "ssl",
                "OpenSSL 3 absent de l'hôte : TLS et CryptoAPI limités"
            );
            return None;
        }
        macro_rules! f {
            ($lib:expr, $n:literal) => {{
                let p = sym($lib, concat!($n, "\0").as_bytes());
                if p == 0 {
                    crate::log_warn!("ssl", "symbole OpenSSL absent : {}", $n);
                    return None;
                }
                std::mem::transmute(p)
            }};
        }
        Some(Ssl {
            EVP_get_digestbyname: f!(c, "EVP_get_digestbyname"),
            EVP_Digest: f!(c, "EVP_Digest"),
            HMAC: f!(c, "HMAC"),
            d2i_X509: f!(c, "d2i_X509"),
            i2d_X509: f!(c, "i2d_X509"),
            X509_free: f!(c, "X509_free"),
            X509_STORE_new: f!(c, "X509_STORE_new"),
            X509_STORE_free: f!(c, "X509_STORE_free"),
            X509_STORE_set_default_paths: f!(c, "X509_STORE_set_default_paths"),
            X509_STORE_add_cert: f!(c, "X509_STORE_add_cert"),
            X509_STORE_set_flags: f!(c, "X509_STORE_set_flags"),
            EVP_get_cipherbyname: f!(c, "EVP_get_cipherbyname"),
            d2i_PKCS12: f!(c, "d2i_PKCS12"),
            EVP_PKEY_CTX_new_from_name: f!(c, "EVP_PKEY_CTX_new_from_name"),
            EVP_PKEY_CTX_new_from_pkey: f!(c, "EVP_PKEY_CTX_new_from_pkey"),
            EVP_PKEY_CTX_free: f!(c, "EVP_PKEY_CTX_free"),
            EVP_PKEY_keygen_init: f!(c, "EVP_PKEY_keygen_init"),
            EVP_PKEY_CTX_set_rsa_keygen_bits: f!(c, "EVP_PKEY_CTX_set_rsa_keygen_bits"),
            EVP_PKEY_generate: f!(c, "EVP_PKEY_generate"),
            EVP_PKEY_get_bn_param: f!(c, "EVP_PKEY_get_bn_param"),
            BN_bn2lebinpad: f!(c, "BN_bn2lebinpad"),
            BN_lebin2bn: f!(c, "BN_lebin2bn"),
            BN_num_bits: f!(c, "BN_num_bits"),
            BN_free: f!(c, "BN_free"),
            OSSL_PARAM_BLD_new: f!(c, "OSSL_PARAM_BLD_new"),
            OSSL_PARAM_BLD_push_BN: f!(c, "OSSL_PARAM_BLD_push_BN"),
            OSSL_PARAM_BLD_to_param: f!(c, "OSSL_PARAM_BLD_to_param"),
            OSSL_PARAM_BLD_free: f!(c, "OSSL_PARAM_BLD_free"),
            OSSL_PARAM_free: f!(c, "OSSL_PARAM_free"),
            EVP_PKEY_fromdata_init: f!(c, "EVP_PKEY_fromdata_init"),
            EVP_PKEY_fromdata: f!(c, "EVP_PKEY_fromdata"),
            EVP_PKEY_sign_init: f!(c, "EVP_PKEY_sign_init"),
            EVP_PKEY_sign: f!(c, "EVP_PKEY_sign"),
            EVP_PKEY_verify_init: f!(c, "EVP_PKEY_verify_init"),
            EVP_PKEY_verify: f!(c, "EVP_PKEY_verify"),
            EVP_PKEY_encrypt_init: f!(c, "EVP_PKEY_encrypt_init"),
            EVP_PKEY_encrypt: f!(c, "EVP_PKEY_encrypt"),
            EVP_PKEY_decrypt_init: f!(c, "EVP_PKEY_decrypt_init"),
            EVP_PKEY_decrypt: f!(c, "EVP_PKEY_decrypt"),
            EVP_PKEY_CTX_set_rsa_padding: f!(c, "EVP_PKEY_CTX_set_rsa_padding"),
            EVP_PKEY_CTX_set_signature_md: f!(c, "EVP_PKEY_CTX_set_signature_md"),
            d2i_PUBKEY: f!(c, "d2i_PUBKEY"),
            PKCS12_parse: f!(c, "PKCS12_parse"),
            PKCS12_free: f!(c, "PKCS12_free"),
            i2d_PrivateKey: f!(c, "i2d_PrivateKey"),
            d2i_AutoPrivateKey: f!(c, "d2i_AutoPrivateKey"),
            EVP_CIPHER_CTX_new: f!(c, "EVP_CIPHER_CTX_new"),
            EVP_CIPHER_CTX_free: f!(c, "EVP_CIPHER_CTX_free"),
            EVP_CipherInit_ex: f!(c, "EVP_CipherInit_ex"),
            EVP_CIPHER_CTX_set_padding: f!(c, "EVP_CIPHER_CTX_set_padding"),
            EVP_CipherUpdate: f!(c, "EVP_CipherUpdate"),
            X509_STORE_CTX_new: f!(c, "X509_STORE_CTX_new"),
            X509_STORE_CTX_free: f!(c, "X509_STORE_CTX_free"),
            X509_STORE_CTX_init: f!(c, "X509_STORE_CTX_init"),
            X509_STORE_CTX_get_error: f!(c, "X509_STORE_CTX_get_error"),
            X509_STORE_CTX_get1_chain: f!(c, "X509_STORE_CTX_get1_chain"),
            X509_STORE_CTX_set_time: f!(c, "X509_STORE_CTX_set_time"),
            X509_verify_cert: f!(c, "X509_verify_cert"),
            X509_check_host: f!(c, "X509_check_host"),
            X509_check_ip_asc: f!(c, "X509_check_ip_asc"),
            OPENSSL_sk_new_null: f!(c, "OPENSSL_sk_new_null"),
            OPENSSL_sk_push: f!(c, "OPENSSL_sk_push"),
            OPENSSL_sk_num: f!(c, "OPENSSL_sk_num"),
            OPENSSL_sk_value: f!(c, "OPENSSL_sk_value"),
            OPENSSL_sk_free: f!(c, "OPENSSL_sk_free"),
            OPENSSL_sk_pop_free: f!(c, "OPENSSL_sk_pop_free"),
            CRYPTO_free: f!(c, "CRYPTO_free"),
            EVP_MD_CTX_new: f!(c, "EVP_MD_CTX_new"),
            EVP_MD_CTX_free: f!(c, "EVP_MD_CTX_free"),
            EVP_DigestInit_ex: f!(c, "EVP_DigestInit_ex"),
            EVP_DigestUpdate: f!(c, "EVP_DigestUpdate"),
            EVP_DigestFinal_ex: f!(c, "EVP_DigestFinal_ex"),
            EVP_DigestVerifyInit: f!(c, "EVP_DigestVerifyInit"),
            EVP_DigestVerify: f!(c, "EVP_DigestVerify"),
            X509_get_pubkey: f!(c, "X509_get_pubkey"),
            EVP_PKEY_free: f!(c, "EVP_PKEY_free"),
            TLS_client_method: f!(s, "TLS_client_method"),
            SSL_CTX_new: f!(s, "SSL_CTX_new"),
            SSL_CTX_free: f!(s, "SSL_CTX_free"),
            SSL_CTX_set_default_verify_paths: f!(s, "SSL_CTX_set_default_verify_paths"),
            SSL_CTX_set_verify: f!(s, "SSL_CTX_set_verify"),
            SSL_CTX_ctrl: f!(s, "SSL_CTX_ctrl"),
            SSL_new: f!(s, "SSL_new"),
            SSL_free: f!(s, "SSL_free"),
            SSL_set_bio: f!(s, "SSL_set_bio"),
            SSL_ctrl: f!(s, "SSL_ctrl"),
            SSL_set1_host: f!(s, "SSL_set1_host"),
            SSL_use_certificate: f!(s, "SSL_use_certificate"),
            SSL_use_PrivateKey: f!(s, "SSL_use_PrivateKey"),
            SSL_set_connect_state: f!(s, "SSL_set_connect_state"),
            SSL_do_handshake: f!(s, "SSL_do_handshake"),
            SSL_get_error: f!(s, "SSL_get_error"),
            SSL_read: f!(s, "SSL_read"),
            SSL_write: f!(s, "SSL_write"),
            SSL_shutdown: f!(s, "SSL_shutdown"),
            SSL_get1_peer_certificate: f!(s, "SSL_get1_peer_certificate"),
            SSL_get_peer_cert_chain: f!(s, "SSL_get_peer_cert_chain"),
            SSL_get_verify_result: f!(s, "SSL_get_verify_result"),
            SSL_version: f!(s, "SSL_version"),
            SSL_get_current_cipher: f!(s, "SSL_get_current_cipher"),
            SSL_CIPHER_get_bits: f!(s, "SSL_CIPHER_get_bits"),
            SSL_set_alpn_protos: f!(s, "SSL_set_alpn_protos"),
            SSL_get0_alpn_selected: f!(s, "SSL_get0_alpn_selected"),
            BIO_s_mem: f!(c, "BIO_s_mem"),
            BIO_new: f!(c, "BIO_new"),
            BIO_write: f!(c, "BIO_write"),
            BIO_read: f!(c, "BIO_read"),
            BIO_ctrl: f!(c, "BIO_ctrl"),
        })
    })
    .as_ref()
}

/// Condensat calculé par morceaux (gros fichiers).
pub struct Hasher {
    ctx: P,
}

impl Hasher {
    pub fn new(name: &CStr) -> Option<Hasher> {
        let s = ssl()?;
        unsafe {
            let md = (s.EVP_get_digestbyname)(name.as_ptr());
            if md.is_null() {
                return None;
            }
            let ctx = (s.EVP_MD_CTX_new)();
            if ctx.is_null() || (s.EVP_DigestInit_ex)(ctx, md, std::ptr::null_mut()) != 1 {
                (s.EVP_MD_CTX_free)(ctx);
                return None;
            }
            Some(Hasher { ctx })
        }
    }
    pub fn update(&mut self, data: &[u8]) {
        if let Some(s) = ssl() {
            unsafe {
                (s.EVP_DigestUpdate)(self.ctx, data.as_ptr() as *const c_void, data.len());
            }
        }
    }
    pub fn finish(self) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        let mut n = 0u32;
        if let Some(s) = ssl() {
            unsafe {
                (s.EVP_DigestFinal_ex)(self.ctx, out.as_mut_ptr(), &mut n);
            }
        }
        out.truncate(n as usize);
        out
    }
}

impl Drop for Hasher {
    fn drop(&mut self) {
        if let Some(s) = ssl() {
            unsafe { (s.EVP_MD_CTX_free)(self.ctx) }
        }
    }
}

/// La signature `sig` de `data` (condensat `digest`) est-elle celle de la clé
/// publique du certificat `cert` (RSA PKCS #1 v1.5 ou ECDSA) ? None : OpenSSL
/// absent ou certificat illisible.
pub fn verify_signature(cert: &[u8], digest: &CStr, sig: &[u8], data: &[u8]) -> Option<bool> {
    let s = ssl()?;
    unsafe {
        let x = x509_of(cert)?;
        let key = (s.X509_get_pubkey)(x);
        (s.X509_free)(x);
        if key.is_null() {
            return None;
        }
        let md = (s.EVP_get_digestbyname)(digest.as_ptr());
        let ctx = (s.EVP_MD_CTX_new)();
        let ok = !md.is_null()
            && !ctx.is_null()
            && (s.EVP_DigestVerifyInit)(ctx, std::ptr::null_mut(), md, std::ptr::null_mut(), key)
                == 1
            && (s.EVP_DigestVerify)(ctx, sig.as_ptr(), sig.len(), data.as_ptr(), data.len()) == 1;
        (s.EVP_MD_CTX_free)(ctx);
        (s.EVP_PKEY_free)(key);
        Some(ok)
    }
}

/// Condensat `name` (« MD5 », « SHA1 », « SHA256 »…) de `data`.
pub fn digest(name: &CStr, data: &[u8]) -> Option<Vec<u8>> {
    let s = ssl()?;
    unsafe {
        let md = (s.EVP_get_digestbyname)(name.as_ptr());
        if md.is_null() {
            return None;
        }
        let mut out = [0u8; 64];
        let mut n = 0u32;
        if (s.EVP_Digest)(
            data.as_ptr().cast(),
            data.len(),
            out.as_mut_ptr(),
            &mut n,
            md,
            std::ptr::null_mut(),
        ) != 1
        {
            return None;
        }
        Some(out[..n as usize].to_vec())
    }
}

/// HMAC `name` de `data` avec la clé `key`.
pub fn hmac(name: &CStr, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let s = ssl()?;
    unsafe {
        let md = (s.EVP_get_digestbyname)(name.as_ptr());
        if md.is_null() {
            return None;
        }
        let mut out = [0u8; 64];
        let mut n = 0u32;
        let r = (s.HMAC)(
            md,
            key.as_ptr().cast(),
            key.len() as c_int,
            data.as_ptr(),
            data.len(),
            out.as_mut_ptr(),
            &mut n,
        );
        (!r.is_null()).then(|| out[..n as usize].to_vec())
    }
}

/// Encodage DER d'un X509* d'OpenSSL.
pub fn der_of(x: P) -> Option<Vec<u8>> {
    let s = ssl()?;
    unsafe {
        let n = (s.i2d_X509)(x, std::ptr::null_mut());
        if n <= 0 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        let mut p = buf.as_mut_ptr();
        (s.i2d_X509)(x, &mut p);
        Some(buf)
    }
}

/// X509* d'OpenSSL à partir de DER (à libérer par X509_free).
pub fn x509_of(der: &[u8]) -> Option<P> {
    let s = ssl()?;
    let mut p = der.as_ptr();
    let x = unsafe { (s.d2i_X509)(std::ptr::null_mut(), &mut p, der.len() as c_long) };
    (!x.is_null()).then_some(x)
}

/// Résultat de la vérification d'une chaîne par OpenSSL.
pub struct Verified {
    /// Code X509_V_* (0 : succès).
    pub error: i32,
    /// Chaîne construite, du certificat final à la racine (DER).
    pub chain: Vec<Vec<u8>>,
}

/// Vérifie `leaf` avec les autorités du système, `extra` servant
/// d'intermédiaires. `host` : nom à contrôler (None : pas de contrôle).
pub fn verify(leaf: &[u8], extra: &[Vec<u8>], host: Option<&str>) -> Option<Verified> {
    verify_with(leaf, extra, None, false, host)
}

/// Comme `verify`, mais la validité des certificats est jugée à la date
/// `at` (secondes Unix) : signature horodatée d'un exécutable (#1387).
pub fn verify_at(leaf: &[u8], extra: &[Vec<u8>], at: i64) -> Option<Verified> {
    verify_inner(leaf, extra, None, false, None, Some(at))
}

/// Comme `verify`, mais avec des racines de confiance données (`roots`) à la
/// place de celles du système ; `partial` accepte une chaîne qui s'arrête sur
/// n'importe quel certificat de `roots` (X509_V_FLAG_PARTIAL_CHAIN).
pub fn verify_with(
    leaf: &[u8],
    extra: &[Vec<u8>],
    roots: Option<&[Vec<u8>]>,
    partial: bool,
    host: Option<&str>,
) -> Option<Verified> {
    verify_inner(leaf, extra, roots, partial, host, None)
}

fn verify_inner(
    leaf: &[u8],
    extra: &[Vec<u8>],
    roots: Option<&[Vec<u8>]>,
    partial: bool,
    host: Option<&str>,
    at: Option<i64>,
) -> Option<Verified> {
    let s = ssl()?;
    unsafe {
        let x = x509_of(leaf)?;
        let store = (s.X509_STORE_new)();
        match roots {
            None => {
                (s.X509_STORE_set_default_paths)(store);
            }
            Some(list) => {
                for r in list {
                    if let Some(rx) = x509_of(r) {
                        (s.X509_STORE_add_cert)(store, rx);
                        (s.X509_free)(rx);
                    }
                }
            }
        }
        if partial {
            (s.X509_STORE_set_flags)(store, 0x80000);
        }
        let untrusted = (s.OPENSSL_sk_new_null)();
        let mut extras = Vec::new();
        for e in extra {
            if let Some(ex) = x509_of(e) {
                (s.OPENSSL_sk_push)(untrusted, ex);
                extras.push(ex);
            }
        }
        let ctx = (s.X509_STORE_CTX_new)();
        (s.X509_STORE_CTX_init)(ctx, store, x, untrusted);
        if let Some(t) = at {
            (s.X509_STORE_CTX_set_time)(ctx, 0, t as libc::time_t);
        }
        let ok = (s.X509_verify_cert)(ctx);
        let mut error = if ok == 1 {
            0
        } else {
            (s.X509_STORE_CTX_get_error)(ctx)
        };
        let mut chain = Vec::new();
        let sk = (s.X509_STORE_CTX_get1_chain)(ctx);
        if !sk.is_null() {
            for i in 0..(s.OPENSSL_sk_num)(sk) {
                if let Some(d) = der_of((s.OPENSSL_sk_value)(sk, i)) {
                    chain.push(d);
                }
            }
            (s.OPENSSL_sk_pop_free)(sk, s.X509_free);
        }
        if chain.is_empty() {
            chain.push(leaf.to_vec());
        }
        if error == 0 {
            if let Some(h) = host.filter(|h| !h.is_empty()) {
                let c = std::ffi::CString::new(h).unwrap_or_default();
                let by_ip = (s.X509_check_ip_asc)(x, c.as_ptr(), 0) == 1;
                if !by_ip
                    && (s.X509_check_host)(x, c.as_ptr(), h.len(), 0, std::ptr::null_mut()) != 1
                {
                    error = 62; // X509_V_ERR_HOSTNAME_MISMATCH
                }
            }
        }
        (s.X509_STORE_CTX_free)(ctx);
        (s.OPENSSL_sk_free)(untrusted);
        for ex in extras {
            (s.X509_free)(ex);
        }
        (s.X509_free)(x);
        (s.X509_STORE_free)(store);
        Some(Verified { error, chain })
    }
}

/// Chiffre (`enc`) ou déchiffre des blocs entiers avec le chiffrement
/// OpenSSL `name` (« AES-128-CBC », « DES-EDE3-ECB »…), sans bourrage ;
/// `iv` sert aux modes chaînés.
pub fn cipher(name: &CStr, key: &[u8], iv: &[u8], enc: bool, data: &mut [u8]) -> bool {
    let Some(s) = ssl() else {
        return false;
    };
    unsafe {
        let cipher = (s.EVP_get_cipherbyname)(name.as_ptr());
        if cipher.is_null() {
            return false;
        }
        let ctx = (s.EVP_CIPHER_CTX_new)();
        if ctx.is_null() {
            return false;
        }
        let null = std::ptr::null_mut();
        let mut ok =
            (s.EVP_CipherInit_ex)(ctx, cipher, null, key.as_ptr(), iv.as_ptr(), enc as c_int) == 1;
        if ok {
            (s.EVP_CIPHER_CTX_set_padding)(ctx, 0);
            let input = data.to_vec();
            let mut n: c_int = 0;
            ok = (s.EVP_CipherUpdate)(
                ctx,
                data.as_mut_ptr(),
                &mut n,
                input.as_ptr(),
                input.len() as c_int,
            ) == 1
                && n as usize == input.len();
        }
        (s.EVP_CIPHER_CTX_free)(ctx);
        ok
    }
}

/// Contenu d'un fichier PKCS #12 (.pfx, .p12).
pub struct Pkcs12 {
    /// Clé privée (DER, format « traditionnel » ou PKCS #8 selon OpenSSL).
    pub key: Option<Vec<u8>>,
    pub cert: Option<Vec<u8>>,
    pub ca: Vec<Vec<u8>>,
}

/// Déchiffre un PKCS #12 avec `password` (UTF-8) ; None si le mot de passe
/// est faux ou le fichier illisible.
pub fn pkcs12(der: &[u8], password: &str) -> Option<Pkcs12> {
    let s = ssl()?;
    unsafe {
        let mut p = der.as_ptr();
        let p12 = (s.d2i_PKCS12)(std::ptr::null_mut(), &mut p, der.len() as c_long);
        if p12.is_null() {
            return None;
        }
        let pass = std::ffi::CString::new(password).unwrap_or_default();
        let (mut key, mut cert, mut ca) = (
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let mut ok = (s.PKCS12_parse)(p12, pass.as_ptr(), &mut key, &mut cert, &mut ca) == 1;
        if !ok && password.is_empty() {
            // Fichier sans mot de passe : OpenSSL distingue "" et NULL.
            ok = (s.PKCS12_parse)(p12, std::ptr::null(), &mut key, &mut cert, &mut ca) == 1;
        }
        (s.PKCS12_free)(p12);
        if !ok {
            return None;
        }
        let key_der = if key.is_null() {
            None
        } else {
            let mut out: *mut u8 = std::ptr::null_mut();
            let n = (s.i2d_PrivateKey)(key, &mut out);
            let v = (n > 0).then(|| std::slice::from_raw_parts(out, n as usize).to_vec());
            if !out.is_null() {
                (s.CRYPTO_free)(out.cast(), c"hostssl".as_ptr(), 0);
            }
            (s.EVP_PKEY_free)(key);
            v
        };
        let cert_der = if cert.is_null() {
            None
        } else {
            let d = der_of(cert);
            (s.X509_free)(cert);
            d
        };
        let mut cas = Vec::new();
        if !ca.is_null() {
            for i in 0..(s.OPENSSL_sk_num)(ca) {
                if let Some(d) = der_of((s.OPENSSL_sk_value)(ca, i)) {
                    cas.push(d);
                }
            }
            (s.OPENSSL_sk_pop_free)(ca, s.X509_free);
        }
        Some(Pkcs12 {
            key: key_der,
            cert: cert_der,
            ca: cas,
        })
    }
}

/// Installe un certificat client et sa clé sur une session TLS.
pub fn use_client_cert(ssl_: P, cert: &[u8], key: &[u8]) -> bool {
    let Some(s) = ssl() else {
        return false;
    };
    unsafe {
        let Some(x) = x509_of(cert) else {
            return false;
        };
        let mut p = key.as_ptr();
        let k = (s.d2i_AutoPrivateKey)(std::ptr::null_mut(), &mut p, key.len() as c_long);
        let ok = !k.is_null()
            && (s.SSL_use_certificate)(ssl_, x) == 1
            && (s.SSL_use_PrivateKey)(ssl_, k) == 1;
        (s.X509_free)(x);
        if !k.is_null() {
            (s.EVP_PKEY_free)(k);
        }
        ok
    }
}

/// Le certificat `der` couvre-t-il le nom d'hôte (ou l'adresse IP) `host` ?
pub fn check_host(der: &[u8], host: &str) -> bool {
    let (Some(s), Some(x)) = (ssl(), x509_of(der)) else {
        return false;
    };
    let c = std::ffi::CString::new(host).unwrap_or_default();
    unsafe {
        let ok = (s.X509_check_ip_asc)(x, c.as_ptr(), 0) == 1
            || (s.X509_check_host)(x, c.as_ptr(), host.len(), 0, std::ptr::null_mut()) == 1;
        (s.X509_free)(x);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condensats_et_hmac() {
        if ssl().is_none() {
            return;
        }
        let md5 = digest(c"MD5", b"abc").unwrap();
        assert_eq!(md5[..4], [0x90, 0x01, 0x50, 0x98]);
        let sha256 = digest(c"SHA256", b"abc").unwrap();
        assert_eq!(sha256[..4], [0xba, 0x78, 0x16, 0xbf]);
        let mac = hmac(
            c"SHA256",
            b"key",
            b"The quick brown fox jumps over the lazy dog",
        )
        .unwrap();
        assert_eq!(mac[..4], [0xf7, 0xbc, 0x83, 0xf4]);
    }
}
