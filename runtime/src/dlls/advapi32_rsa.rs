//! CryptoAPI : fournisseurs (HCRYPTPROV), conteneurs de clés et clés RSA.
//!
//! Un fournisseur porte un nom de conteneur. Les clés RSA d'un conteneur
//! (AT_KEYEXCHANGE, AT_SIGNATURE) sont gardées par compte dans
//! `~/.config/winlux/crypto/<conteneur>/`, chiffrées par la DPAPI de Winlux
//! ([`super::dpapi`]). CRYPT_VERIFYCONTEXT donne un fournisseur sans
//! conteneur, aux clés éphémères.
//!
//! Les opérations RSA (génération, signature PKCS #1 v1.5, chiffrement
//! PKCS #1 v1.5 ou OAEP) passent par l'OpenSSL de l'hôte. Comme sous
//! Windows, signatures et textes chiffrés sont rendus petit-boutistes, et les
//! blobs PUBLICKEYBLOB/PRIVATEKEYBLOB suivent le format « RSA1 »/« RSA2 ».

use super::hostssl::{self, P};
use crate::process::set_last_error;
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_MORE_DATA: u32 = 234;
const NTE_BAD_KEY: u32 = 0x8009_0003;
const NTE_BAD_LEN: u32 = 0x8009_0004;
const NTE_BAD_DATA: u32 = 0x8009_0005;
const NTE_BAD_SIGNATURE: u32 = 0x8009_0006;
const NTE_BAD_ALGID: u32 = 0x8009_0008;
const NTE_BAD_FLAGS: u32 = 0x8009_0009;
const NTE_BAD_TYPE: u32 = 0x8009_000A;
const NTE_BAD_KEY_STATE: u32 = 0x8009_000B;
const NTE_BAD_UID: u32 = 0x8009_0001;
const NTE_NO_KEY: u32 = 0x8009_000D;
const NTE_EXISTS: u32 = 0x8009_000F;
const NTE_BAD_KEYSET: u32 = 0x8009_0016;
const NTE_FAIL: u32 = 0x8009_0020;

const AT_KEYEXCHANGE: u32 = 1;
const AT_SIGNATURE: u32 = 2;
const CALG_RSA_SIGN: u32 = 0x2400;
const CALG_RSA_KEYX: u32 = 0xa400;
const PUBLICKEYBLOB: u8 = 6;
const PRIVATEKEYBLOB: u8 = 7;
const CRYPT_EXPORTABLE: u32 = 1;
const CRYPT_VERIFYCONTEXT: u32 = 0xF000_0000;
const CRYPT_NEWKEYSET: u32 = 0x8;
const CRYPT_DELETEKEYSET: u32 = 0x10;
const CRYPT_OAEP: u32 = 0x40;
const RSA1: u32 = 0x3141_5352;
const RSA2: u32 = 0x3241_5352;

// ---------- Fournisseurs ----------

struct Provider {
    /// None : CRYPT_VERIFYCONTEXT (aucun conteneur).
    container: Option<String>,
    prov_type: u32,
    refs: usize,
    /// Conteneur jetable (clé d'un certificat), effacé à la libération.
    temporary: bool,
}

fn providers() -> &'static Mutex<HashMap<usize, Provider>> {
    static M: OnceLock<Mutex<HashMap<usize, Provider>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicUsize = AtomicUsize::new(0x5250_0000);

/// Le fournisseur existe-t-il ? (1 reste accepté : ancien fournisseur fictif.)
pub fn is_provider(h: usize) -> bool {
    h == 1 || super::lock(providers()).contains_key(&h)
}

fn crypto_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WINLUX_CRYPTO_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(d).join("winlux/crypto");
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config/winlux/crypto"))
        .unwrap_or_else(|| std::env::temp_dir().join("winlux-crypto"))
}

/// Dossier d'un conteneur : son nom en hexadécimal, pour n'avoir aucun
/// caractère interdit ni de chemin détourné (« ../ »).
fn container_dir(name: &str) -> PathBuf {
    let hex: String = name.bytes().map(|b| format!("{b:02x}")).collect();
    crypto_dir().join(if hex.is_empty() { "00".into() } else { hex })
}

fn default_container() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".into())
}

unsafe fn acquire(ph: *mut usize, container: Option<String>, prov_type: u32, flags: u32) -> i32 {
    if ph.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if flags & CRYPT_VERIFYCONTEXT == CRYPT_VERIFYCONTEXT {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        super::lock(providers()).insert(
            id,
            Provider {
                container: None,
                prov_type,
                refs: 1,
                temporary: false,
            },
        );
        *ph = id;
        return 1;
    }
    let name = container
        .filter(|c| !c.is_empty())
        .unwrap_or_else(default_container);
    let dir = container_dir(&name);
    if flags & CRYPT_DELETEKEYSET != 0 {
        *ph = 0;
        if std::fs::remove_dir_all(&dir).is_err() {
            set_last_error(NTE_BAD_KEYSET);
            return 0;
        }
        return 1;
    }
    let exists = dir.is_dir();
    if flags & CRYPT_NEWKEYSET != 0 {
        if exists {
            set_last_error(NTE_EXISTS);
            return 0;
        }
        use std::os::unix::fs::DirBuilderExt;
        if std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .is_err()
        {
            set_last_error(NTE_FAIL);
            return 0;
        }
        let _ = std::fs::write(dir.join("name"), name.as_bytes());
    } else if !exists {
        set_last_error(NTE_BAD_KEYSET);
        return 0;
    }
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(providers()).insert(
        id,
        Provider {
            container: Some(name),
            prov_type,
            refs: 1,
            temporary: false,
        },
    );
    *ph = id;
    1
}

pub unsafe extern "win64" fn crypt_acquire_context_w(
    ph: *mut usize,
    container: *const u16,
    _provider: *const u16,
    prov_type: u32,
    flags: u32,
) -> i32 {
    let name = (!container.is_null()).then(|| super::wstr(container));
    acquire(ph, name, prov_type, flags)
}

pub unsafe extern "win64" fn crypt_acquire_context_a(
    ph: *mut usize,
    container: *const u8,
    _provider: *const u8,
    prov_type: u32,
    flags: u32,
) -> i32 {
    let name = (!container.is_null()).then(|| super::cstr(container));
    acquire(ph, name, prov_type, flags)
}

pub extern "win64" fn crypt_release_context(h: usize, _flags: u32) -> i32 {
    if h == 1 {
        return 1;
    }
    let mut map = super::lock(providers());
    match map.get_mut(&h) {
        Some(p) => {
            p.refs -= 1;
            if p.refs == 0 {
                if let Some(Provider {
                    container: Some(c),
                    temporary: true,
                    ..
                }) = map.remove(&h)
                {
                    let _ = std::fs::remove_dir_all(container_dir(&c));
                }
            }
            1
        }
        None => {
            set_last_error(NTE_BAD_UID);
            0
        }
    }
}

pub extern "win64" fn crypt_context_add_ref(h: usize, _reserved: usize, _flags: u32) -> i32 {
    match super::lock(providers()).get_mut(&h) {
        Some(p) => {
            p.refs += 1;
            1
        }
        None => {
            set_last_error(NTE_BAD_UID);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_gen_random(prov: usize, size: u32, buf: *mut u8) -> i32 {
    if !is_provider(prov) || (buf.is_null() && size != 0) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if size == 0 || super::bcrypt::bcrypt_gen_random(0, buf, size, 2) == 0 {
        1
    } else {
        set_last_error(NTE_FAIL);
        0
    }
}

unsafe fn put_bytes(v: &[u8], out: *mut u8, len: *mut u32) -> i32 {
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if out.is_null() {
        *len = v.len() as u32;
        return 1;
    }
    if (*len as usize) < v.len() {
        *len = v.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len());
    *len = v.len() as u32;
    1
}

pub unsafe extern "win64" fn crypt_get_prov_param(
    prov: usize,
    param: u32,
    out: *mut u8,
    len: *mut u32,
    _flags: u32,
) -> i32 {
    let (container, prov_type) = match super::lock(providers()).get(&prov) {
        Some(p) => (p.container.clone(), p.prov_type),
        None if prov == 1 => (None, 1),
        None => {
            set_last_error(NTE_BAD_UID);
            return 0;
        }
    };
    let cstr = |s: &str| [s.as_bytes(), &[0]].concat();
    let v = match param {
        // PP_IMPTYPE : CRYPT_IMPL_SOFTWARE
        3 => 2u32.to_le_bytes().to_vec(),
        // PP_NAME
        4 => cstr("Winlux Software Cryptographic Provider"),
        // PP_VERSION : 2.0
        5 => 0x200u32.to_le_bytes().to_vec(),
        // PP_CONTAINER, PP_UNIQUE_CONTAINER
        6 | 36 => match container {
            Some(c) => cstr(&c),
            None => {
                set_last_error(NTE_BAD_KEYSET);
                return 0;
            }
        },
        // PP_PROVTYPE
        16 => prov_type.to_le_bytes().to_vec(),
        // PP_KEYSPEC
        39 => (AT_KEYEXCHANGE | AT_SIGNATURE).to_le_bytes().to_vec(),
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    };
    put_bytes(&v, out, len)
}

pub extern "win64" fn crypt_set_prov_param(
    _prov: usize,
    _param: u32,
    _data: *const u8,
    _flags: u32,
) -> i32 {
    1
}

// ---------- Clés RSA ----------

struct RsaKey {
    pkey: usize,
    spec: u32,
    private: bool,
    exportable: bool,
}

unsafe impl Send for RsaKey {}

fn keys() -> &'static Mutex<HashMap<usize, RsaKey>> {
    static K: OnceLock<Mutex<HashMap<usize, RsaKey>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn is_rsa(key: usize) -> bool {
    super::lock(keys()).contains_key(&key)
}

fn new_handle(pkey: P, spec: u32, private: bool, exportable: bool) -> usize {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(keys()).insert(
        id,
        RsaKey {
            pkey: pkey as usize,
            spec,
            private,
            exportable,
        },
    );
    id
}

fn spec_of(algid: u32) -> Option<u32> {
    match algid {
        AT_KEYEXCHANGE | CALG_RSA_KEYX => Some(AT_KEYEXCHANGE),
        AT_SIGNATURE | CALG_RSA_SIGN => Some(AT_SIGNATURE),
        _ => None,
    }
}

fn generate(bits: u32) -> Option<P> {
    let s = hostssl::ssl()?;
    unsafe {
        let ctx =
            (s.EVP_PKEY_CTX_new_from_name)(std::ptr::null_mut(), c"RSA".as_ptr(), std::ptr::null());
        if ctx.is_null() {
            return None;
        }
        let mut pkey: P = std::ptr::null_mut();
        let ok = (s.EVP_PKEY_keygen_init)(ctx) == 1
            && (s.EVP_PKEY_CTX_set_rsa_keygen_bits)(ctx, bits as i32) == 1
            && (s.EVP_PKEY_generate)(ctx, &mut pkey) == 1;
        (s.EVP_PKEY_CTX_free)(ctx);
        (ok && !pkey.is_null()).then_some(pkey)
    }
}

/// Paramètre RSA en petit-boutiste sur `len` octets.
unsafe fn param_le(pkey: P, name: &CStr, len: usize) -> Option<Vec<u8>> {
    let s = hostssl::ssl()?;
    let mut bn: P = std::ptr::null_mut();
    if (s.EVP_PKEY_get_bn_param)(pkey, name.as_ptr(), &mut bn) != 1 || bn.is_null() {
        return None;
    }
    let mut v = vec![0u8; len];
    let r = (s.BN_bn2lebinpad)(bn, v.as_mut_ptr(), len as i32);
    (s.BN_free)(bn);
    (r >= 0).then_some(v)
}

unsafe fn bits_of(pkey: P) -> Option<u32> {
    let s = hostssl::ssl()?;
    let mut bn: P = std::ptr::null_mut();
    if (s.EVP_PKEY_get_bn_param)(pkey, c"n".as_ptr(), &mut bn) != 1 || bn.is_null() {
        return None;
    }
    let b = (s.BN_num_bits)(bn) as u32;
    (s.BN_free)(bn);
    Some(b.div_ceil(8) * 8)
}

/// PUBLICKEYBLOB ou PRIVATEKEYBLOB d'une clé RSA.
unsafe fn export_blob(pkey: P, algid: u32, private: bool) -> Option<Vec<u8>> {
    let bits = bits_of(pkey)?;
    let n = bits as usize / 8;
    let h = n / 2;
    let e = param_le(pkey, c"e", 4)?;
    let mut out = vec![
        if private {
            PRIVATEKEYBLOB
        } else {
            PUBLICKEYBLOB
        },
        2,
        0,
        0,
    ];
    out.extend_from_slice(&algid.to_le_bytes());
    out.extend_from_slice(&(if private { RSA2 } else { RSA1 }).to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(&e);
    out.extend(param_le(pkey, c"n", n)?);
    if private {
        for (name, len) in [
            (c"rsa-factor1", h),
            (c"rsa-factor2", h),
            (c"rsa-exponent1", h),
            (c"rsa-exponent2", h),
            (c"rsa-coefficient1", h),
            (c"d", n),
        ] {
            out.extend(param_le(pkey, name, len)?);
        }
    }
    Some(out)
}

/// Clé RSA depuis un PUBLICKEYBLOB ou un PRIVATEKEYBLOB : (clé, privée ?, algorithme).
unsafe fn import_blob(blob: &[u8]) -> Option<(P, bool, u32)> {
    let s = hostssl::ssl()?;
    if blob.len() < 20 {
        return None;
    }
    let private = match blob[0] {
        PUBLICKEYBLOB => false,
        PRIVATEKEYBLOB => true,
        _ => return None,
    };
    let algid = u32::from_le_bytes(blob[4..8].try_into().ok()?);
    let magic = u32::from_le_bytes(blob[8..12].try_into().ok()?);
    let bits = u32::from_le_bytes(blob[12..16].try_into().ok()?) as usize;
    if magic != if private { RSA2 } else { RSA1 } || bits == 0 || !bits.is_multiple_of(16) {
        return None;
    }
    let (n, h) = (bits / 8, bits / 16);
    let mut fields: Vec<(&CStr, &[u8])> = vec![(c"e", &blob[16..20])];
    let mut at = 20;
    let mut take = |len: usize| -> Option<&[u8]> {
        let v = blob.get(at..at + len)?;
        at += len;
        Some(v)
    };
    fields.push((c"n", take(n)?));
    if private {
        for name in [
            c"rsa-factor1",
            c"rsa-factor2",
            c"rsa-exponent1",
            c"rsa-exponent2",
            c"rsa-coefficient1",
        ] {
            fields.push((name, take(h)?));
        }
        fields.push((c"d", take(n)?));
    }
    let bld = (s.OSSL_PARAM_BLD_new)();
    let mut bns = Vec::new();
    for (name, le) in &fields {
        let bn = (s.BN_lebin2bn)(le.as_ptr(), le.len() as i32, std::ptr::null_mut());
        (s.OSSL_PARAM_BLD_push_BN)(bld, name.as_ptr(), bn);
        bns.push(bn);
    }
    let params = (s.OSSL_PARAM_BLD_to_param)(bld);
    let ctx =
        (s.EVP_PKEY_CTX_new_from_name)(std::ptr::null_mut(), c"RSA".as_ptr(), std::ptr::null());
    let mut pkey: P = std::ptr::null_mut();
    // EVP_PKEY_KEYPAIR (0x87) ou EVP_PKEY_PUBLIC_KEY (0x86).
    let selection = if private { 0x87 } else { 0x86 };
    let ok = !ctx.is_null()
        && !params.is_null()
        && (s.EVP_PKEY_fromdata_init)(ctx) == 1
        && (s.EVP_PKEY_fromdata)(ctx, &mut pkey, selection, params) == 1;
    if !ctx.is_null() {
        (s.EVP_PKEY_CTX_free)(ctx);
    }
    if !params.is_null() {
        (s.OSSL_PARAM_free)(params);
    }
    (s.OSSL_PARAM_BLD_free)(bld);
    for bn in bns {
        (s.BN_free)(bn);
    }
    (ok && !pkey.is_null()).then_some((pkey, private, algid))
}

fn key_file(container: &str, spec: u32) -> PathBuf {
    container_dir(container).join(if spec == AT_SIGNATURE {
        "signature.key"
    } else {
        "exchange.key"
    })
}

/// Garde la clé privée dans le conteneur, chiffrée par la DPAPI du compte.
unsafe fn persist(container: &str, spec: u32, pkey: P, exportable: bool) {
    let algid = if spec == AT_SIGNATURE {
        CALG_RSA_SIGN
    } else {
        CALG_RSA_KEYX
    };
    let Some(mut blob) = export_blob(pkey, algid, true) else {
        return;
    };
    // L'octet réservé du BLOBHEADER retient « exportable ».
    blob[2] = exportable as u8;
    if let Some(sealed) = super::dpapi::protect(&blob, &[], b"winlux-crypto-container", 0) {
        use std::os::unix::fs::OpenOptionsExt;
        let path = key_file(container, spec);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
        {
            use std::io::Write;
            let _ = f.write_all(&sealed);
        }
    }
}

unsafe fn load(container: &str, spec: u32) -> Option<usize> {
    let sealed = std::fs::read(key_file(container, spec)).ok()?;
    let (blob, _) = super::dpapi::unprotect(&sealed, b"winlux-crypto-container")?;
    let exportable = blob.get(2) == Some(&1);
    let (pkey, _, _) = import_blob(&blob)?;
    Some(new_handle(pkey, spec, true, exportable))
}

fn container_of(prov: usize) -> Result<Option<String>, ()> {
    match super::lock(providers()).get(&prov) {
        Some(p) => Ok(p.container.clone()),
        None if prov == 1 => Ok(None),
        None => Err(()),
    }
}

/// CryptGenKey pour AT_KEYEXCHANGE, AT_SIGNATURE et CALG_RSA_* ; None si
/// l'algorithme n'est pas RSA (les clés symétriques sont ailleurs).
pub unsafe fn gen_key(prov: usize, algid: u32, flags: u32, out: *mut usize) -> Option<i32> {
    let spec = spec_of(algid)?;
    let Ok(container) = container_of(prov) else {
        set_last_error(NTE_BAD_UID);
        return Some(0);
    };
    let bits = match flags >> 16 {
        0 => 2048,
        b if (512..=16384).contains(&b) && b.is_multiple_of(8) => b,
        _ => {
            set_last_error(NTE_BAD_FLAGS);
            return Some(0);
        }
    };
    let Some(pkey) = generate(bits) else {
        set_last_error(NTE_FAIL);
        return Some(0);
    };
    let exportable = flags & CRYPT_EXPORTABLE != 0;
    if let Some(c) = &container {
        persist(c, spec, pkey, exportable);
    }
    *out = new_handle(pkey, spec, true, exportable);
    Some(1)
}

/// CryptImportKey pour PUBLICKEYBLOB et PRIVATEKEYBLOB.
pub unsafe fn import_key(prov: usize, blob: &[u8], flags: u32, out: *mut usize) -> Option<i32> {
    if !matches!(blob.first(), Some(&PUBLICKEYBLOB) | Some(&PRIVATEKEYBLOB)) {
        return None;
    }
    let Some((pkey, private, algid)) = import_blob(blob) else {
        set_last_error(NTE_BAD_DATA);
        return Some(0);
    };
    let spec = spec_of(algid).unwrap_or(AT_KEYEXCHANGE);
    let exportable = flags & CRYPT_EXPORTABLE != 0;
    if private {
        if let Ok(Some(c)) = container_of(prov) {
            persist(&c, spec, pkey, exportable);
        }
    }
    *out = new_handle(pkey, spec, private, exportable);
    Some(1)
}

pub unsafe extern "win64" fn crypt_get_user_key(prov: usize, spec: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let container = match container_of(prov) {
        Ok(Some(c)) => c,
        Ok(None) => {
            set_last_error(NTE_NO_KEY);
            return 0;
        }
        Err(()) => {
            set_last_error(NTE_BAD_UID);
            return 0;
        }
    };
    match load(&container, spec) {
        Some(h) => {
            *out = h;
            1
        }
        None => {
            set_last_error(NTE_NO_KEY);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_export_key(
    key: usize,
    exp_key: usize,
    blob_type: u32,
    _flags: u32,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    let (pkey, spec, private, exportable) = match super::lock(keys()).get(&key) {
        Some(k) => (k.pkey as P, k.spec, k.private, k.exportable),
        None => return super::advapi32_crypto::export_symmetric(key, exp_key, blob_type, out, len),
    };
    if exp_key != 0 {
        set_last_error(NTE_BAD_KEY);
        return 0;
    }
    let algid = if spec == AT_SIGNATURE {
        CALG_RSA_SIGN
    } else {
        CALG_RSA_KEYX
    };
    let blob = match blob_type as u8 {
        PUBLICKEYBLOB => export_blob(pkey, algid, false),
        PRIVATEKEYBLOB if !private => {
            set_last_error(NTE_BAD_KEY_STATE);
            return 0;
        }
        PRIVATEKEYBLOB if !exportable => {
            set_last_error(NTE_BAD_KEY_STATE);
            return 0;
        }
        PRIVATEKEYBLOB => export_blob(pkey, algid, true),
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    };
    match blob {
        Some(b) => put_bytes(&b, out, len),
        None => {
            set_last_error(NTE_FAIL);
            0
        }
    }
}

/// Nom OpenSSL du condensat d'un ALG_ID de la CryptoAPI.
fn md_name(algid: u32) -> Option<&'static CStr> {
    Some(match algid {
        0x8003 => c"MD5",
        0x8004 => c"SHA1",
        0x800c => c"SHA256",
        0x800d => c"SHA384",
        0x800e => c"SHA512",
        _ => return None,
    })
}

/// Contexte d'opération sur la clé, rembourrage PKCS #1 ou OAEP.
unsafe fn op_ctx(pkey: P) -> Option<P> {
    let s = hostssl::ssl()?;
    let ctx = (s.EVP_PKEY_CTX_new_from_pkey)(std::ptr::null_mut(), pkey, std::ptr::null());
    (!ctx.is_null()).then_some(ctx)
}

unsafe fn sign_hash(prov_key: (P, bool), algid: u32, digest: &[u8]) -> Result<Vec<u8>, u32> {
    let s = hostssl::ssl().ok_or(NTE_FAIL)?;
    let (pkey, private) = prov_key;
    if !private {
        return Err(NTE_BAD_KEY);
    }
    let md = md_name(algid).ok_or(NTE_BAD_ALGID)?;
    let ctx = op_ctx(pkey).ok_or(NTE_FAIL)?;
    let mut sig = vec![0u8; 1024];
    let mut n = sig.len();
    let ok = (s.EVP_PKEY_sign_init)(ctx) == 1
        && (s.EVP_PKEY_CTX_set_rsa_padding)(ctx, 1) == 1
        && (s.EVP_PKEY_CTX_set_signature_md)(ctx, (s.EVP_get_digestbyname)(md.as_ptr())) == 1
        && (s.EVP_PKEY_sign)(ctx, sig.as_mut_ptr(), &mut n, digest.as_ptr(), digest.len()) == 1;
    (s.EVP_PKEY_CTX_free)(ctx);
    if !ok {
        return Err(NTE_BAD_DATA);
    }
    sig.truncate(n);
    sig.reverse();
    Ok(sig)
}

unsafe fn sign_common(hash: usize, spec: u32, flags: u32, out: *mut u8, len: *mut u32) -> i32 {
    if len.is_null() || flags != 0 {
        set_last_error(if flags != 0 {
            NTE_BAD_FLAGS
        } else {
            ERROR_INVALID_PARAMETER
        });
        return 0;
    }
    let Some((algid, digest, prov)) = super::advapi32_crypto::hash_value(hash) else {
        set_last_error(0x8009_0002); // NTE_BAD_HASH
        return 0;
    };
    let container = match container_of(prov) {
        Ok(Some(c)) => c,
        _ => {
            set_last_error(NTE_NO_KEY);
            return 0;
        }
    };
    let Some(h) = load(&container, spec) else {
        set_last_error(NTE_NO_KEY);
        return 0;
    };
    let pkey = super::lock(keys()).get(&h).map(|k| k.pkey as P);
    let result = sign_hash((pkey.unwrap_or(std::ptr::null_mut()), true), algid, &digest);
    crypt_destroy_key(h);
    match result {
        Ok(sig) => put_bytes(&sig, out, len),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn crypt_sign_hash_w(
    hash: usize,
    spec: u32,
    _desc: *const u16,
    flags: u32,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    sign_common(hash, spec, flags, out, len)
}

pub unsafe extern "win64" fn crypt_sign_hash_a(
    hash: usize,
    spec: u32,
    _desc: *const u8,
    flags: u32,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    sign_common(hash, spec, flags, out, len)
}

unsafe fn verify_common(hash: usize, sig: *const u8, sig_len: u32, key: usize) -> i32 {
    let Some(s) = hostssl::ssl() else {
        set_last_error(NTE_FAIL);
        return 0;
    };
    let Some(pkey) = super::lock(keys()).get(&key).map(|k| k.pkey as P) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    let Some((algid, digest, _)) = super::advapi32_crypto::hash_value(hash) else {
        set_last_error(0x8009_0002);
        return 0;
    };
    let Some(md) = md_name(algid) else {
        set_last_error(NTE_BAD_ALGID);
        return 0;
    };
    if sig.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut be = std::slice::from_raw_parts(sig, sig_len as usize).to_vec();
    be.reverse();
    let Some(ctx) = op_ctx(pkey) else {
        set_last_error(NTE_FAIL);
        return 0;
    };
    let ok = (s.EVP_PKEY_verify_init)(ctx) == 1
        && (s.EVP_PKEY_CTX_set_rsa_padding)(ctx, 1) == 1
        && (s.EVP_PKEY_CTX_set_signature_md)(ctx, (s.EVP_get_digestbyname)(md.as_ptr())) == 1
        && (s.EVP_PKEY_verify)(ctx, be.as_ptr(), be.len(), digest.as_ptr(), digest.len()) == 1;
    (s.EVP_PKEY_CTX_free)(ctx);
    if ok {
        1
    } else {
        set_last_error(NTE_BAD_SIGNATURE);
        0
    }
}

pub unsafe extern "win64" fn crypt_verify_signature_w(
    hash: usize,
    sig: *const u8,
    sig_len: u32,
    key: usize,
    _desc: *const u16,
    _flags: u32,
) -> i32 {
    verify_common(hash, sig, sig_len, key)
}

pub unsafe extern "win64" fn crypt_verify_signature_a(
    hash: usize,
    sig: *const u8,
    sig_len: u32,
    key: usize,
    _desc: *const u8,
    _flags: u32,
) -> i32 {
    verify_common(hash, sig, sig_len, key)
}

/// CryptEncrypt avec une clé RSA (PKCS #1 v1.5, ou OAEP avec CRYPT_OAEP).
pub unsafe fn encrypt(
    key: usize,
    final_: i32,
    flags: u32,
    data: *mut u8,
    len: *mut u32,
    cap: u32,
) -> i32 {
    let s = hostssl::ssl().unwrap();
    let pkey = super::lock(keys()).get(&key).map(|k| k.pkey as P).unwrap();
    let Some(bits) = bits_of(pkey) else {
        set_last_error(NTE_BAD_KEY);
        return 0;
    };
    let k = bits as usize / 8;
    if final_ == 0 {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    let n = *len as usize;
    let max = if flags & CRYPT_OAEP != 0 {
        k - 42
    } else {
        k - 11
    };
    if n > max {
        set_last_error(NTE_BAD_LEN);
        return 0;
    }
    if data.is_null() {
        *len = k as u32;
        return 1;
    }
    if (cap as usize) < k {
        *len = k as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    let Some(ctx) = op_ctx(pkey) else {
        set_last_error(NTE_FAIL);
        return 0;
    };
    let input = std::slice::from_raw_parts(data, n).to_vec();
    let mut outv = vec![0u8; k];
    let mut outn = k;
    let pad = if flags & CRYPT_OAEP != 0 { 4 } else { 1 };
    let ok = (s.EVP_PKEY_encrypt_init)(ctx) == 1
        && (s.EVP_PKEY_CTX_set_rsa_padding)(ctx, pad) == 1
        && (s.EVP_PKEY_encrypt)(
            ctx,
            outv.as_mut_ptr(),
            &mut outn,
            input.as_ptr(),
            input.len(),
        ) == 1;
    (s.EVP_PKEY_CTX_free)(ctx);
    if !ok {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    outv.truncate(outn);
    outv.reverse();
    std::ptr::copy_nonoverlapping(outv.as_ptr(), data, outv.len());
    *len = outv.len() as u32;
    1
}

pub unsafe fn decrypt(key: usize, final_: i32, flags: u32, data: *mut u8, len: *mut u32) -> i32 {
    let s = hostssl::ssl().unwrap();
    let (pkey, private) = super::lock(keys())
        .get(&key)
        .map(|k| (k.pkey as P, k.private))
        .unwrap();
    if !private {
        set_last_error(NTE_BAD_KEY);
        return 0;
    }
    if final_ == 0 || data.is_null() {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    let mut be = std::slice::from_raw_parts(data, *len as usize).to_vec();
    be.reverse();
    let Some(ctx) = op_ctx(pkey) else {
        set_last_error(NTE_FAIL);
        return 0;
    };
    let mut outv = vec![0u8; be.len().max(1)];
    let mut outn = outv.len();
    let pad = if flags & CRYPT_OAEP != 0 { 4 } else { 1 };
    let ok = (s.EVP_PKEY_decrypt_init)(ctx) == 1
        && (s.EVP_PKEY_CTX_set_rsa_padding)(ctx, pad) == 1
        && (s.EVP_PKEY_decrypt)(ctx, outv.as_mut_ptr(), &mut outn, be.as_ptr(), be.len()) == 1;
    (s.EVP_PKEY_CTX_free)(ctx);
    if !ok {
        set_last_error(NTE_BAD_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(outv.as_ptr(), data, outn);
    *len = outn as u32;
    1
}

/// Paramètres d'une clé RSA (KP_ALGID, KP_KEYLEN, KP_BLOCKLEN, KP_PERMISSIONS).
pub unsafe fn get_param(key: usize, param: u32, out: *mut u8, len: *mut u32) -> i32 {
    let (pkey, spec, exportable) = super::lock(keys())
        .get(&key)
        .map(|k| (k.pkey as P, k.spec, k.exportable))
        .unwrap();
    let bits = bits_of(pkey).unwrap_or(0);
    let v = match param {
        7 => (if spec == AT_SIGNATURE {
            CALG_RSA_SIGN
        } else {
            CALG_RSA_KEYX
        })
        .to_le_bytes(),
        8 | 9 => bits.to_le_bytes(),
        // KP_PERMISSIONS : tout, sauf l'export si la clé ne l'autorise pas.
        6 => (0x3fu32 & !(if exportable { 0 } else { 0x4 })).to_le_bytes(),
        _ => {
            set_last_error(NTE_BAD_TYPE);
            return 0;
        }
    };
    put_bytes(&v, out, len)
}

pub fn duplicate(key: usize) -> usize {
    let Some(s) = hostssl::ssl() else {
        return 0;
    };
    let k = super::lock(keys())
        .get(&key)
        .map(|k| (k.pkey, k.spec, k.private, k.exportable));
    match k {
        Some((pkey, spec, private, exportable)) => unsafe {
            // Une nouvelle référence sur la même EVP_PKEY.
            let blob = export_blob(pkey as P, CALG_RSA_KEYX, private);
            let _ = s;
            match blob.and_then(|b| import_blob(&b)) {
                Some((p, _, _)) => new_handle(p, spec, private, exportable),
                None => 0,
            }
        },
        None => 0,
    }
}

pub extern "win64" fn crypt_destroy_key(key: usize) -> i32 {
    match super::lock(keys()).remove(&key) {
        Some(k) => {
            if let Some(s) = hostssl::ssl() {
                unsafe { (s.EVP_PKEY_free)(k.pkey as P) };
            }
            1
        }
        None => {
            set_last_error(NTE_BAD_KEY);
            0
        }
    }
}

/// Fournisseur éphémère qui tient la clé privée `der` (DER d'OpenSSL) comme
/// AT_KEYEXCHANGE : sert à CryptAcquireCertificatePrivateKey.
pub fn provider_for_private_key(der: &[u8]) -> Option<usize> {
    let s = hostssl::ssl()?;
    unsafe {
        let mut p = der.as_ptr();
        let pkey =
            (s.d2i_AutoPrivateKey)(std::ptr::null_mut(), &mut p, der.len() as std::ffi::c_long);
        if pkey.is_null() {
            return None;
        }
        // Le conteneur temporaire vit le temps du fournisseur.
        let name = format!("cert-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let dir = container_dir(&name);
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .ok()?;
        persist(&name, AT_KEYEXCHANGE, pkey, false);
        (s.EVP_PKEY_free)(pkey);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        super::lock(providers()).insert(
            id,
            Provider {
                container: Some(name),
                prov_type: 24,
                refs: 1,
                temporary: true,
            },
        );
        Some(id)
    }
}

/// CryptImportPublicKeyInfo : clé publique d'un CERT_PUBLIC_KEY_INFO (SPKI).
pub unsafe fn import_spki(spki_der: &[u8], out: *mut usize) -> i32 {
    let Some(s) = hostssl::ssl() else {
        set_last_error(NTE_FAIL);
        return 0;
    };
    let mut p = spki_der.as_ptr();
    let pkey = (s.d2i_PUBKEY)(
        std::ptr::null_mut(),
        &mut p,
        spki_der.len() as std::ffi::c_long,
    );
    if pkey.is_null() || bits_of(pkey).is_none() {
        if !pkey.is_null() {
            (s.EVP_PKEY_free)(pkey);
        }
        set_last_error(NTE_BAD_ALGID);
        return 0;
    }
    *out = new_handle(pkey, AT_KEYEXCHANGE, false, true);
    1
}

#[cfg(test)]
mod tests {
    use super::super::advapi32_crypto as capi;
    use super::*;

    /// Dossiers de test : le même dossier DPAPI que les tests de `dpapi`
    /// (les variables d'environnement sont partagées entre fils).
    fn setup() -> bool {
        if hostssl::ssl().is_none() {
            return false;
        }
        let pid = std::process::id();
        std::env::set_var(
            "WINLUX_DPAPI_DIR",
            std::env::temp_dir().join(format!("winlux-dpapi-{pid}")),
        );
        std::env::set_var(
            "WINLUX_CRYPTO_DIR",
            std::env::temp_dir().join(format!("winlux-crypto-{pid}")),
        );
        true
    }

    unsafe fn verify_prov() -> usize {
        let mut p = 0;
        assert_eq!(
            crypt_acquire_context_w(
                &mut p,
                std::ptr::null(),
                std::ptr::null(),
                24,
                CRYPT_VERIFYCONTEXT
            ),
            1
        );
        p
    }

    unsafe fn export(key: usize, kind: u8) -> Vec<u8> {
        let mut n = 0u32;
        assert_eq!(
            crypt_export_key(key, 0, kind as u32, 0, std::ptr::null_mut(), &mut n),
            1
        );
        let mut v = vec![0u8; n as usize];
        assert_eq!(
            crypt_export_key(key, 0, kind as u32, 0, v.as_mut_ptr(), &mut n),
            1
        );
        v.truncate(n as usize);
        v
    }

    unsafe fn sha256(prov: usize, data: &[u8]) -> usize {
        let mut h = 0;
        assert_eq!(capi::crypt_create_hash(prov, 0x800c, 0, 0, &mut h), 1);
        assert_eq!(
            capi::crypt_hash_data(h, data.as_ptr(), data.len() as u32, 0),
            1
        );
        h
    }

    unsafe fn sign(hash: usize, spec: u32) -> Option<Vec<u8>> {
        let mut n = 0u32;
        if crypt_sign_hash_w(
            hash,
            spec,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            &mut n,
        ) != 1
        {
            return None;
        }
        let mut v = vec![0u8; n as usize];
        assert_eq!(
            crypt_sign_hash_w(hash, spec, std::ptr::null(), 0, v.as_mut_ptr(), &mut n),
            1
        );
        Some(v)
    }

    #[test]
    fn export_import_et_chiffrement() {
        if !setup() {
            return;
        }
        let _teb = crate::process::test_teb();
        unsafe {
            let prov = verify_prov();
            let mut key = 0;
            assert_eq!(
                capi::crypt_gen_key(
                    prov,
                    AT_KEYEXCHANGE,
                    (1024 << 16) | CRYPT_EXPORTABLE,
                    &mut key
                ),
                1
            );
            assert!(is_rsa(key));
            let mut bits = 0u32;
            let mut n = 4u32;
            assert_eq!(
                capi::crypt_get_key_param(key, 9, (&mut bits as *mut u32).cast(), &mut n, 0),
                1
            );
            assert_eq!(bits, 1024);

            let private = export(key, PRIVATEKEYBLOB);
            assert_eq!(&private[8..12], b"RSA2");
            assert_eq!(private.len(), 20 + 9 * 1024 / 16);
            let public = export(key, PUBLICKEYBLOB);
            assert_eq!(&public[8..12], b"RSA1");
            assert_eq!(public.len(), 20 + 1024 / 8);

            // Clé publique seule chez « l'expéditeur », clé privée importée ailleurs.
            let mut pubk = 0;
            assert_eq!(
                capi::crypt_import_key(prov, public.as_ptr(), public.len() as u32, 0, 0, &mut pubk),
                1
            );
            let mut privk = 0;
            assert_eq!(
                capi::crypt_import_key(
                    prov,
                    private.as_ptr(),
                    private.len() as u32,
                    0,
                    0,
                    &mut privk
                ),
                1
            );
            assert_eq!(export(privk, PUBLICKEYBLOB), public);

            for flags in [0, CRYPT_OAEP] {
                let msg = b"secret de session";
                let mut buf = vec![0u8; 128];
                buf[..msg.len()].copy_from_slice(msg);
                let mut len = msg.len() as u32;
                assert_eq!(
                    capi::crypt_encrypt(pubk, 0, 1, flags, buf.as_mut_ptr(), &mut len, 128),
                    1
                );
                assert_eq!(len, 128);
                // Une clé publique ne déchiffre pas.
                let mut copy = buf.clone();
                let mut l2 = len;
                assert_eq!(
                    capi::crypt_decrypt(pubk, 0, 1, flags, copy.as_mut_ptr(), &mut l2),
                    0
                );
                assert_eq!(
                    capi::crypt_decrypt(privk, 0, 1, flags, buf.as_mut_ptr(), &mut len),
                    1
                );
                assert_eq!(&buf[..len as usize], msg);
            }

            // Clé non exportable : pas de PRIVATEKEYBLOB.
            let mut locked = 0;
            assert_eq!(
                capi::crypt_gen_key(prov, CALG_RSA_KEYX, 1024 << 16, &mut locked),
                1
            );
            let mut n = 0u32;
            assert_eq!(
                crypt_export_key(
                    locked,
                    0,
                    PRIVATEKEYBLOB as u32,
                    0,
                    std::ptr::null_mut(),
                    &mut n
                ),
                0
            );

            let mut dup = 0;
            assert_eq!(capi::crypt_duplicate_key(privk, 0, 0, &mut dup), 1);
            assert_eq!(export(dup, PUBLICKEYBLOB), public);
            for k in [key, pubk, privk, locked, dup] {
                assert_eq!(capi::crypt_destroy_key(k), 1);
            }
            assert!(!is_rsa(key));
            assert_eq!(crypt_release_context(prov, 0), 1);
        }
    }

    #[test]
    fn conteneur_persistant_et_signature() {
        if !setup() {
            return;
        }
        let _teb = crate::process::test_teb();
        unsafe {
            let name: Vec<u16> = "Conteneur de test\0".encode_utf16().collect();
            let mut prov = 0;
            // Le conteneur n'existe pas encore.
            let _ = crypt_acquire_context_w(
                &mut prov,
                name.as_ptr(),
                std::ptr::null(),
                1,
                CRYPT_DELETEKEYSET,
            );
            assert_eq!(
                crypt_acquire_context_w(&mut prov, name.as_ptr(), std::ptr::null(), 1, 0),
                0
            );
            assert_eq!(
                crypt_acquire_context_w(
                    &mut prov,
                    name.as_ptr(),
                    std::ptr::null(),
                    1,
                    CRYPT_NEWKEYSET
                ),
                1
            );
            let mut none = 0;
            assert_eq!(crypt_get_user_key(prov, AT_SIGNATURE, &mut none), 0);
            let mut key = 0;
            assert_eq!(
                capi::crypt_gen_key(prov, AT_SIGNATURE, 1024 << 16, &mut key),
                1
            );
            let public = export(key, PUBLICKEYBLOB);
            assert_eq!(
                u32::from_le_bytes(public[4..8].try_into().unwrap()),
                CALG_RSA_SIGN
            );

            let h = sha256(prov, b"abc");
            let sig = sign(h, AT_SIGNATURE).unwrap();
            assert_eq!(sig.len(), 128);
            // Pas de clé d'échange dans ce conteneur.
            assert!(sign(h, AT_KEYEXCHANGE).is_none());

            // Vérification par un tiers qui n'a que la clé publique.
            let vp = verify_prov();
            let mut pubk = 0;
            assert_eq!(
                capi::crypt_import_key(vp, public.as_ptr(), public.len() as u32, 0, 0, &mut pubk),
                1
            );
            let hv = sha256(vp, b"abc");
            assert_eq!(
                crypt_verify_signature_w(
                    hv,
                    sig.as_ptr(),
                    sig.len() as u32,
                    pubk,
                    std::ptr::null(),
                    0
                ),
                1
            );
            let mut bad = sig.clone();
            bad[0] ^= 1;
            assert_eq!(
                crypt_verify_signature_w(
                    hv,
                    bad.as_ptr(),
                    bad.len() as u32,
                    pubk,
                    std::ptr::null(),
                    0
                ),
                0
            );
            let other = sha256(vp, b"abd");
            assert_eq!(
                crypt_verify_signature_w(
                    other,
                    sig.as_ptr(),
                    sig.len() as u32,
                    pubk,
                    std::ptr::null(),
                    0
                ),
                0
            );

            // La clé est gardée, chiffrée, et retrouvée par un nouveau fournisseur.
            assert_eq!(crypt_release_context(prov, 0), 1);
            let dir = container_dir("Conteneur de test");
            let stored = std::fs::read(dir.join("signature.key")).unwrap();
            assert!(!stored.windows(4).any(|w| w == b"RSA2"));
            let mut again = 0;
            assert_eq!(
                crypt_acquire_context_w(&mut again, name.as_ptr(), std::ptr::null(), 1, 0),
                1
            );
            let mut k2 = 0;
            assert_eq!(crypt_get_user_key(again, AT_SIGNATURE, &mut k2), 1);
            assert_eq!(export(k2, PUBLICKEYBLOB), public);
            let h2 = sha256(again, b"abc");
            // PKCS #1 v1.5 est déterministe : même signature.
            assert_eq!(sign(h2, AT_SIGNATURE).unwrap(), sig);
            assert_eq!(crypt_release_context(again, 0), 1);

            assert_eq!(
                crypt_acquire_context_w(
                    &mut again,
                    name.as_ptr(),
                    std::ptr::null(),
                    1,
                    CRYPT_DELETEKEYSET
                ),
                1
            );
            assert!(!dir.exists());
            assert_eq!(
                crypt_acquire_context_w(&mut again, name.as_ptr(), std::ptr::null(), 1, 0),
                0
            );
            for k in [key, pubk, k2] {
                capi::crypt_destroy_key(k);
            }
            for x in [h, hv, other, h2] {
                capi::crypt_destroy_hash(x);
            }
            crypt_release_context(vp, 0);
        }
    }

    #[test]
    fn cle_privee_d_un_certificat() {
        if !setup() {
            return;
        }
        let _teb = crate::process::test_teb();
        let s = hostssl::ssl().unwrap();
        unsafe {
            let pkey = generate(1024).unwrap();
            let mut out: *mut u8 = std::ptr::null_mut();
            let n = (s.i2d_PrivateKey)(pkey, &mut out);
            assert!(n > 0);
            let der = std::slice::from_raw_parts(out, n as usize).to_vec();
            (s.CRYPTO_free)(out.cast(), c"test".as_ptr(), 0);
            let expected = export_blob(pkey, CALG_RSA_KEYX, false).unwrap();
            (s.EVP_PKEY_free)(pkey);

            let prov = provider_for_private_key(&der).unwrap();
            let dir = match super::super::lock(providers()).get(&prov) {
                Some(p) => container_dir(p.container.as_ref().unwrap()),
                None => unreachable!(),
            };
            assert!(dir.is_dir());
            let mut key = 0;
            assert_eq!(crypt_get_user_key(prov, AT_KEYEXCHANGE, &mut key), 1);
            assert_eq!(export(key, PUBLICKEYBLOB), expected);
            // Non exportable, comme une clé de certificat importée sans option.
            let mut n = 0u32;
            assert_eq!(
                crypt_export_key(
                    key,
                    0,
                    PRIVATEKEYBLOB as u32,
                    0,
                    std::ptr::null_mut(),
                    &mut n
                ),
                0
            );
            let h = sha256(prov, b"poignee de main");
            assert!(sign(h, AT_KEYEXCHANGE).is_some());
            capi::crypt_destroy_key(key);
            capi::crypt_destroy_hash(h);
            assert_eq!(crypt_release_context(prov, 0), 1);
            assert!(!dir.exists());
        }
    }
}
