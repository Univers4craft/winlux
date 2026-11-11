//! crypt32, lot F de #1410 (#1615).
//!
//! Servis pour de bon : la table des OID (CryptFindOIDInfo,
//! CryptEnumOIDInfo, CertAlgIdToOID, CertOIDToAlgId), la comparaison et le
//! condensat des clés publiques, l'énumération des magasins système et de
//! leurs emplacements, l'export d'un magasin en PKCS #7, la lecture des
//! certificats et du nombre de signataires d'un message PKCS #7, les
//! usages valides (EKU), la vérification d'un sujet par son émetteur
//! (dates, nom), la sérialisation d'un certificat et sa relecture, les
//! enveloppes CryptEncodeObject / CryptDecodeObject et CryptFormatObject
//! (vidage hexadécimal, comme Windows sans formateur dédié).
//!
//! Winlux ne gère pas de listes de confiance (CTL) : les magasins n'en
//! contiennent aucune. Signer, chiffrer et vérifier des messages PKCS #7
//! et exporter un PFX répondent NTE_NOT_SUPPORTED avec une trace, tant que
//! l'OpenSSL de l'hôte n'est pas branché pour cela.
//!
//! Comme le reste de crypt32 dans Winlux, les structures suivent la
//! disposition x64 de wincrypt.h (documentation publique).

#![allow(non_snake_case)]

use super::crypt32::{
    cert_der, children, der_wrap, new_store, oid_str, prop_ids, store_add_der, store_certs, tlv,
    AlgId, BitBlob, Blob,
};
use crate::process::set_last_error;
use std::ffi::CStr;
use std::sync::OnceLock;

const CRYPT_E_NOT_FOUND: u32 = 0x8009_2004;
const CRYPT_E_ASN1_BADTAG: u32 = 0x8009_3102;
const NTE_NOT_SUPPORTED: u32 = 0x8009_0029;
const E_INVALIDARG: u32 = 0x8007_0057;
const ERROR_MORE_DATA: u32 = 234;

// ── Table des OID ───────────────────────────────────────────────────────

const GROUP_HASH: u32 = 1;
const GROUP_ENCRYPT: u32 = 2;
const GROUP_PUBKEY: u32 = 3;
const GROUP_SIGN: u32 = 4;
const GROUP_RDN_ATTR: u32 = 5;
const GROUP_EXT: u32 = 6;
const GROUP_ENHKEY_USAGE: u32 = 7;

/// (OID, nom, groupe, ALG_ID ou valeur, ALG_ID du hachage pour SIGN, nom CNG).
const OIDS: &[(&str, &str, u32, u32, u32, &str)] = &[
    ("1.2.840.113549.2.5", "md5", GROUP_HASH, 0x8003, 0, "MD5"),
    ("1.3.14.3.2.26", "sha1", GROUP_HASH, 0x8004, 0, "SHA1"),
    (
        "2.16.840.1.101.3.4.2.1",
        "sha256",
        GROUP_HASH,
        0x800c,
        0,
        "SHA256",
    ),
    (
        "2.16.840.1.101.3.4.2.2",
        "sha384",
        GROUP_HASH,
        0x800d,
        0,
        "SHA384",
    ),
    (
        "2.16.840.1.101.3.4.2.3",
        "sha512",
        GROUP_HASH,
        0x800e,
        0,
        "SHA512",
    ),
    (
        "1.2.840.113549.3.7",
        "3des",
        GROUP_ENCRYPT,
        0x6603,
        0,
        "3DES",
    ),
    (
        "2.16.840.1.101.3.4.1.2",
        "aes128",
        GROUP_ENCRYPT,
        0x660e,
        0,
        "AES",
    ),
    (
        "2.16.840.1.101.3.4.1.22",
        "aes192",
        GROUP_ENCRYPT,
        0x660f,
        0,
        "AES",
    ),
    (
        "2.16.840.1.101.3.4.1.42",
        "aes256",
        GROUP_ENCRYPT,
        0x6610,
        0,
        "AES",
    ),
    (
        "1.2.840.113549.1.1.1",
        "RSA",
        GROUP_PUBKEY,
        0xa400,
        0,
        "RSA",
    ),
    ("1.2.840.10045.2.1", "ECC", GROUP_PUBKEY, 0, 0, "ECC"),
    (
        "1.2.840.113549.1.1.4",
        "md5RSA",
        GROUP_SIGN,
        0x2400,
        0x8003,
        "RSA",
    ),
    (
        "1.2.840.113549.1.1.5",
        "sha1RSA",
        GROUP_SIGN,
        0x2400,
        0x8004,
        "RSA",
    ),
    (
        "1.2.840.113549.1.1.11",
        "sha256RSA",
        GROUP_SIGN,
        0x2400,
        0x800c,
        "RSA",
    ),
    (
        "1.2.840.113549.1.1.12",
        "sha384RSA",
        GROUP_SIGN,
        0x2400,
        0x800d,
        "RSA",
    ),
    (
        "1.2.840.113549.1.1.13",
        "sha512RSA",
        GROUP_SIGN,
        0x2400,
        0x800e,
        "RSA",
    ),
    (
        "1.2.840.10045.4.1",
        "sha1ECDSA",
        GROUP_SIGN,
        0x2203,
        0x8004,
        "ECDSA",
    ),
    (
        "1.2.840.10045.4.3.2",
        "sha256ECDSA",
        GROUP_SIGN,
        0x2203,
        0x800c,
        "ECDSA",
    ),
    (
        "1.2.840.10045.4.3.3",
        "sha384ECDSA",
        GROUP_SIGN,
        0x2203,
        0x800d,
        "ECDSA",
    ),
    (
        "1.2.840.10045.4.3.4",
        "sha512ECDSA",
        GROUP_SIGN,
        0x2203,
        0x800e,
        "ECDSA",
    ),
    ("2.5.4.3", "CN", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.4.6", "C", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.4.7", "L", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.4.8", "S", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.4.10", "O", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.4.11", "OU", GROUP_RDN_ATTR, 0, 0, ""),
    ("1.2.840.113549.1.9.1", "E", GROUP_RDN_ATTR, 0, 0, ""),
    ("2.5.29.14", "Subject Key Identifier", GROUP_EXT, 0, 0, ""),
    ("2.5.29.15", "Key Usage", GROUP_EXT, 0, 0, ""),
    ("2.5.29.17", "Subject Alternative Name", GROUP_EXT, 0, 0, ""),
    ("2.5.29.19", "Basic Constraints", GROUP_EXT, 0, 0, ""),
    ("2.5.29.35", "Authority Key Identifier", GROUP_EXT, 0, 0, ""),
    ("2.5.29.37", "Enhanced Key Usage", GROUP_EXT, 0, 0, ""),
    (
        "1.3.6.1.5.5.7.3.1",
        "Server Authentication",
        GROUP_ENHKEY_USAGE,
        0,
        0,
        "",
    ),
    (
        "1.3.6.1.5.5.7.3.2",
        "Client Authentication",
        GROUP_ENHKEY_USAGE,
        0,
        0,
        "",
    ),
    (
        "1.3.6.1.5.5.7.3.3",
        "Code Signing",
        GROUP_ENHKEY_USAGE,
        0,
        0,
        "",
    ),
    (
        "1.3.6.1.5.5.7.3.4",
        "Secure Email",
        GROUP_ENHKEY_USAGE,
        0,
        0,
        "",
    ),
    (
        "1.3.6.1.5.5.7.3.8",
        "Time Stamping",
        GROUP_ENHKEY_USAGE,
        0,
        0,
        "",
    ),
];

/// CRYPT_OID_INFO (x64, 72 octets) : cbSize, pszOID, pwszName, dwGroupId,
/// dwValue/Algid, ExtraInfo { cbData, pbData }, pwszCNGAlgid,
/// pwszCNGExtraAlgid.
#[repr(C)]
struct OidInfo {
    size: u32,
    oid: *const u8,
    name: *const u16,
    group: u32,
    value: u32,
    extra: Blob,
    cng: *const u16,
    cng_extra: *const u16,
}

unsafe impl Send for OidInfo {}
unsafe impl Sync for OidInfo {}

fn leak_c(s: &str) -> *const u8 {
    Box::leak(format!("{s}\0").into_boxed_str()).as_ptr()
}
fn leak_w(s: &str) -> *const u16 {
    Box::leak(
        s.encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
    .as_ptr()
}

/// Les structures sont bâties une fois et ne bougent plus : les
/// appelants gardent les pointeurs rendus.
fn table() -> &'static [OidInfo] {
    static T: OnceLock<Vec<OidInfo>> = OnceLock::new();
    T.get_or_init(|| {
        OIDS.iter()
            .map(|&(oid, name, group, value, hash, cng)| {
                let extra: &'static [u8] = if group == GROUP_SIGN {
                    // ExtraInfo d'une signature : ALG_ID du hachage puis de la clé.
                    let v = [hash.to_le_bytes(), value.to_le_bytes()].concat();
                    Box::leak(v.into_boxed_slice())
                } else {
                    &[]
                };
                OidInfo {
                    size: std::mem::size_of::<OidInfo>() as u32,
                    oid: leak_c(oid),
                    name: leak_w(name),
                    group,
                    value: if group == GROUP_SIGN { hash } else { value },
                    extra: Blob {
                        cb: extra.len() as u32,
                        pb: if extra.is_empty() {
                            std::ptr::null()
                        } else {
                            extra.as_ptr()
                        },
                    },
                    cng: if cng.is_empty() {
                        std::ptr::null()
                    } else {
                        leak_w(cng)
                    },
                    cng_extra: std::ptr::null(),
                }
            })
            .collect()
    })
}

const OID_KEY: u32 = 1;
const NAME_KEY: u32 = 2;
const ALGID_KEY: u32 = 3;
const SIGN_KEY: u32 = 4;
const CNG_ALGID_KEY: u32 = 5;

/// CryptFindOIDInfo(dwKeyType, pvKey, dwGroupId).
unsafe extern "win64" fn CryptFindOIDInfo(key_type: u32, key: usize, group: u32) -> usize {
    if key == 0 {
        return 0;
    }
    let group = group & 0xFFFF;
    let fits = |i: &&OidInfo| group == 0 || i.group == group;
    let found = match key_type & 0xFFFF {
        OID_KEY => {
            let s = CStr::from_ptr(key as *const _)
                .to_string_lossy()
                .into_owned();
            table().iter().filter(fits).find(|i| oid_of(i) == s)
        }
        NAME_KEY => {
            let s = crate::dlls::wstr(key as *const u16);
            table()
                .iter()
                .filter(fits)
                .find(|i| crate::dlls::wstr(i.name).eq_ignore_ascii_case(&s))
        }
        ALGID_KEY => {
            let a = *(key as *const u32);
            table()
                .iter()
                .filter(fits)
                .find(|i| i.group != GROUP_SIGN && i.value == a && a != 0)
        }
        SIGN_KEY => {
            let (h, k) = (*(key as *const u32), *(key as *const u32).add(1));
            table().iter().find(|i| {
                i.group == GROUP_SIGN && i.value == h && {
                    let e = std::slice::from_raw_parts(i.extra.pb, 8);
                    u32::from_le_bytes(e[4..8].try_into().unwrap()) == k
                }
            })
        }
        CNG_ALGID_KEY => {
            let s = crate::dlls::wstr(key as *const u16);
            table()
                .iter()
                .filter(fits)
                .find(|i| !i.cng.is_null() && crate::dlls::wstr(i.cng) == s)
        }
        _ => None,
    };
    found.map_or(0, |i| i as *const OidInfo as usize)
}

fn oid_of(i: &OidInfo) -> String {
    unsafe {
        CStr::from_ptr(i.oid as *const _)
            .to_string_lossy()
            .into_owned()
    }
}

type OidCallback = unsafe extern "win64" fn(*const OidInfo, usize) -> i32;

/// CryptEnumOIDInfo(dwGroupId, dwFlags, pvArg, pfnEnumOIDInfo).
unsafe extern "win64" fn CryptEnumOIDInfo(group: u32, _flags: u32, arg: usize, cb: usize) -> i32 {
    if cb == 0 {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let f: OidCallback = std::mem::transmute(cb);
    for i in table().iter().filter(|i| group == 0 || i.group == group) {
        if f(i, arg) == 0 {
            return 0;
        }
    }
    1
}

/// CertAlgIdToOID : OID du hachage, du chiffrement ou de la clé publique.
extern "win64" fn CertAlgIdToOID(algid: u32) -> usize {
    table()
        .iter()
        .find(|i| i.group != GROUP_SIGN && i.value == algid && algid != 0)
        .map_or(0, |i| i.oid as usize)
}

/// CertOIDToAlgId : pour une signature, l'ALG_ID du hachage.
unsafe extern "win64" fn CertOIDToAlgId(oid: *const u8) -> u32 {
    if oid.is_null() {
        return 0;
    }
    let s = CStr::from_ptr(oid as *const _)
        .to_string_lossy()
        .into_owned();
    table()
        .iter()
        .find(|i| oid_of(i) == s)
        .map_or(0, |i| i.value)
}

/// CryptFindLocalizedName : noms des magasins système.
unsafe extern "win64" fn CryptFindLocalizedName(name: *const u16) -> usize {
    static NAMES: OnceLock<Vec<(&'static str, usize)>> = OnceLock::new();
    if name.is_null() {
        return 0;
    }
    let names = NAMES.get_or_init(|| {
        [
            ("Root", "Autorités de certification racines de confiance"),
            ("AuthRoot", "Autorités de certification racines tierces"),
            ("CA", "Autorités de certification intermédiaires"),
            ("My", "Personnel"),
            ("Trust", "Confiance de l'entreprise"),
            ("Disallowed", "Certificats non autorisés"),
            ("AddressBook", "Autres personnes"),
            ("TrustedPeople", "Personnes autorisées"),
            ("TrustedPublisher", "Éditeurs approuvés"),
            (".Default", "Registre"),
        ]
        .iter()
        .map(|&(k, v)| (k, leak_w(v) as usize))
        .collect()
    });
    let n = crate::dlls::wstr(name);
    names
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&n))
        .map_or(0, |&(_, v)| v)
}

// ── Clés publiques ──────────────────────────────────────────────────────

/// CERT_PUBLIC_KEY_INFO { Algorithm, PublicKey }.
#[repr(C)]
struct PublicKeyInfo {
    alg: AlgId,
    key: BitBlob,
}

unsafe fn spki_der(info: &PublicKeyInfo) -> Option<Vec<u8>> {
    let oid = CStr::from_ptr(info.alg.oid).to_string_lossy().into_owned();
    let mut alg = super::crypt32_encode::oid_der(&oid)?;
    if info.alg.params.cb > 0 && !info.alg.params.pb.is_null() {
        alg.extend_from_slice(std::slice::from_raw_parts(
            info.alg.params.pb,
            info.alg.params.cb as usize,
        ));
    }
    let mut bits = vec![info.key.unused as u8];
    if info.key.cb > 0 {
        bits.extend_from_slice(std::slice::from_raw_parts(
            info.key.pb,
            info.key.cb as usize,
        ));
    }
    let body = [der_wrap(0x30, &alg), der_wrap(0x03, &bits)].concat();
    Some(der_wrap(0x30, &body))
}

/// CertComparePublicKeyInfo : même algorithme et mêmes octets de clé.
unsafe extern "win64" fn CertComparePublicKeyInfo(
    _enc: u32,
    a: *const PublicKeyInfo,
    b: *const PublicKeyInfo,
) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    let key = |p: &PublicKeyInfo| {
        if p.key.cb == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(p.key.pb, p.key.cb as usize)
        }
    };
    let oid = |p: &PublicKeyInfo| CStr::from_ptr(p.alg.oid).to_bytes().to_vec();
    (oid(&*a) == oid(&*b) && key(&*a) == key(&*b)) as i32
}

fn digest_name(algid: u32) -> Option<&'static CStr> {
    Some(match algid {
        0 | 0x8004 => c"SHA1",
        0x8003 => c"MD5",
        0x800c => c"SHA256",
        0x800d => c"SHA384",
        0x800e => c"SHA512",
        _ => return None,
    })
}

unsafe fn put_out(v: &[u8], out: *mut u8, size: *mut u32) -> i32 {
    if size.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    if out.is_null() {
        *size = v.len() as u32;
        return 1;
    }
    if (*size as usize) < v.len() {
        *size = v.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len());
    *size = v.len() as u32;
    1
}

/// CryptHashPublicKeyInfo : condensat (SHA-1 par défaut) de la
/// SubjectPublicKeyInfo encodée.
unsafe extern "win64" fn CryptHashPublicKeyInfo(
    _prov: usize,
    algid: u32,
    _flags: u32,
    _enc: u32,
    info: *const PublicKeyInfo,
    out: *mut u8,
    size: *mut u32,
) -> i32 {
    let (Some(name), false) = (digest_name(algid), info.is_null()) else {
        set_last_error(E_INVALIDARG);
        return 0;
    };
    let Some(der) = spki_der(&*info) else {
        set_last_error(E_INVALIDARG);
        return 0;
    };
    match super::hostssl::digest(name, &der) {
        Some(d) => put_out(&d, out, size),
        None => {
            set_last_error(NTE_NOT_SUPPORTED);
            0
        }
    }
}

/// CryptExportPublicKeyInfo(Ex) : les fournisseurs CSP et CNG de Winlux ne
/// rendent pas encore leurs clés publiques sous cette forme.
extern "win64" fn CryptExportPublicKeyInfo() -> i32 {
    crate::trace!("CryptExportPublicKeyInfo : non servi");
    set_last_error(NTE_NOT_SUPPORTED);
    0
}

// ── Magasins système ────────────────────────────────────────────────────

const SYSTEM_STORES: [&str; 5] = ["My", "Root", "Trust", "CA", "Disallowed"];
/// CERT_SYSTEM_STORE_CURRENT_USER et LOCAL_MACHINE (emplacement << 16).
const LOCATIONS: [(&str, u32); 2] = [("CurrentUser", 1 << 16), ("LocalMachine", 2 << 16)];

type LocationCallback = unsafe extern "win64" fn(*const u16, u32, usize, usize) -> i32;
type StoreCallback = unsafe extern "win64" fn(*const u16, u32, *const u32, usize, usize) -> i32;
type PhysicalCallback =
    unsafe extern "win64" fn(*const u16, u32, *const u16, *const u8, usize, usize) -> i32;

unsafe extern "win64" fn CertEnumSystemStoreLocation(_flags: u32, arg: usize, cb: usize) -> i32 {
    if cb == 0 {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let f: LocationCallback = std::mem::transmute(cb);
    for (name, flag) in LOCATIONS {
        let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), flag, 0, arg) == 0 {
            return 0;
        }
    }
    1
}

unsafe extern "win64" fn CertEnumSystemStore(
    flags: u32,
    _para: usize,
    arg: usize,
    cb: usize,
) -> i32 {
    if cb == 0 {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let f: StoreCallback = std::mem::transmute(cb);
    let info = [4u32]; // CERT_SYSTEM_STORE_INFO { cbSize }
    for name in SYSTEM_STORES {
        let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), flags & 0xFFFF_0000, info.as_ptr(), 0, arg) == 0 {
            return 0;
        }
    }
    1
}

/// Un seul magasin physique par magasin système : « .Default ».
unsafe extern "win64" fn CertEnumPhysicalStore(
    store: *const u16,
    flags: u32,
    arg: usize,
    cb: usize,
) -> i32 {
    if cb == 0 || store.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let f: PhysicalCallback = std::mem::transmute(cb);
    let name: Vec<u16> = ".Default".encode_utf16().chain(Some(0)).collect();
    // CERT_PHYSICAL_STORE_INFO : cbSize, pszOpenStoreProvider, dwOpenEncodingType,
    // dwOpenFlags, OpenParameters, dwFlags, dwPriority (56 octets en x64).
    let mut info = [0u8; 56];
    info[..4].copy_from_slice(&56u32.to_le_bytes());
    f(
        store,
        flags & 0xFFFF_0000,
        name.as_ptr(),
        info.as_ptr(),
        0,
        arg,
    );
    1
}

/// Les magasins système de Winlux sont fixes : inscrire ou retirer un
/// magasin réussit sans en créer de nouveau.
extern "win64" fn CertRegisterSystemStore(
    _store: usize,
    _flags: u32,
    _info: usize,
    _r: usize,
) -> i32 {
    1
}
extern "win64" fn CertUnregisterSystemStore(_store: usize, _flags: u32) -> i32 {
    1
}

/// CertRemoveStoreFromCollection : la collection perd les certificats que
/// le frère lui avait apportés.
unsafe extern "win64" fn CertRemoveStoreFromCollection(coll: usize, sibling: usize) {
    let gone: Vec<Vec<u8>> = store_certs(sibling)
        .iter()
        .map(|&c| cert_der(c).to_vec())
        .collect();
    for c in store_certs(coll) {
        if gone.iter().any(|g| g.as_slice() == cert_der(c)) {
            let dup = super::crypt32::CertDuplicateCertificateContext(c);
            super::crypt32::CertDeleteCertificateFromStore(dup);
        }
    }
}

// ── Propriétés, usages, vérification ────────────────────────────────────

unsafe extern "win64" fn CertEnumCertificateContextProperties(ctx: usize, prev: u32) -> u32 {
    prop_ids(ctx).into_iter().find(|&id| id > prev).unwrap_or(0)
}

/// Usages étendus (EKU, 2.5.29.37) d'un certificat ; None sans extension.
unsafe fn ekus(ctx: usize) -> Option<Vec<String>> {
    let c = super::crypt32::context(ctx);
    let info = &*c.info;
    let exts = std::slice::from_raw_parts(info.exts, info.ext_count as usize);
    let ext = exts
        .iter()
        .find(|e| CStr::from_ptr(e.oid).to_bytes() == b"2.5.29.37")?;
    let value = std::slice::from_raw_parts(ext.value.pb, ext.value.cb as usize);
    let (seq, _) = tlv(value)?;
    Some(
        children(seq.body)
            .iter()
            .filter(|t| t.tag == 6)
            .map(|t| oid_str(t.body))
            .collect(),
    )
}

/// CertGetValidUsages : intersection des EKU ; -1 si aucun certificat
/// n'en restreint les usages.
unsafe extern "win64" fn CertGetValidUsages(
    count: u32,
    certs: *const usize,
    num: *mut i32,
    oids: *mut usize,
    size: *mut u32,
) -> i32 {
    if num.is_null() || size.is_null() || (count > 0 && certs.is_null()) {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let mut common: Option<Vec<String>> = None;
    for i in 0..count as usize {
        if let Some(e) = ekus(*certs.add(i)) {
            common = Some(match common {
                None => e,
                Some(c) => c.into_iter().filter(|o| e.contains(o)).collect(),
            });
        }
    }
    let Some(list) = common else {
        *num = -1;
        *size = 0;
        return 1;
    };
    let ptrs = list.len() * 8;
    let total = ptrs + list.iter().map(|s| s.len() + 1).sum::<usize>();
    *num = list.len() as i32;
    if oids.is_null() {
        *size = total as u32;
        return 1;
    }
    if (*size as usize) < total {
        *size = total as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    let base = oids as *mut u8;
    let mut at = ptrs;
    for (i, s) in list.iter().enumerate() {
        let p = base.add(at);
        std::ptr::copy_nonoverlapping(s.as_ptr(), p, s.len());
        *p.add(s.len()) = 0;
        *oids.add(i) = p as usize;
        at += s.len() + 1;
    }
    *size = total as u32;
    1
}

const CERT_STORE_SIGNATURE_FLAG: u32 = 1;
const CERT_STORE_TIME_VALIDITY_FLAG: u32 = 2;
const CERT_STORE_REVOCATION_FLAG: u32 = 4;
const CERT_STORE_NO_CRL_FLAG: u32 = 0x10000;
const CERT_STORE_NO_ISSUER_FLAG: u32 = 0x20000;

/// CertVerifySubjectCertificateContext : chaque contrôle réussi efface son
/// drapeau. La signature est vérifiée par l'OpenSSL de l'hôte ; sans liste
/// de révocation, NO_CRL est posé et REVOCATION effacé, comme le décrit la
/// documentation.
unsafe extern "win64" fn CertVerifySubjectCertificateContext(
    subject: usize,
    issuer: usize,
    flags: *mut u32,
) -> i32 {
    if subject == 0 || flags.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let mut f = *flags;
    if f & CERT_STORE_TIME_VALIDITY_FLAG != 0 {
        let info = super::crypt32::context(subject).info;
        if super::crypt32::CertVerifyTimeValidity(std::ptr::null(), info) == 0 {
            f &= !CERT_STORE_TIME_VALIDITY_FLAG;
        }
    }
    if f & CERT_STORE_SIGNATURE_FLAG != 0 {
        if issuer == 0 {
            f |= CERT_STORE_NO_ISSUER_FLAG;
        } else if super::hostssl::signed_by(cert_der(subject), cert_der(issuer)) {
            f &= !CERT_STORE_SIGNATURE_FLAG;
        }
    }
    if f & CERT_STORE_REVOCATION_FLAG != 0 {
        f = (f & !CERT_STORE_REVOCATION_FLAG) | CERT_STORE_NO_CRL_FLAG;
    }
    *flags = f;
    1
}

// ── PKCS #7 ─────────────────────────────────────────────────────────────

const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
const OID_DATA: &str = "1.2.840.113549.1.7.1";

/// SignedData « dégénéré » (sans signataire) portant des certificats.
fn degenerate_pkcs7(certs: &[Vec<u8>]) -> Vec<u8> {
    let oid = |s: &str| super::crypt32_encode::oid_der(s).unwrap_or_default();
    let content_info = der_wrap(0x30, &oid(OID_DATA));
    let signed = [
        der_wrap(0x02, &[1]),
        der_wrap(0x31, &[]),
        content_info,
        der_wrap(0xA0, &certs.concat()),
        der_wrap(0x31, &[]),
    ]
    .concat();
    der_wrap(
        0x30,
        &[
            oid(OID_SIGNED_DATA),
            der_wrap(0xA0, &der_wrap(0x30, &signed)),
        ]
        .concat(),
    )
}

/// Corps de SignedData d'un ContentInfo PKCS #7 : (certificats, signataires).
fn signed_data(blob: &[u8]) -> Option<(Vec<Vec<u8>>, usize)> {
    let (ci, _) = tlv(blob)?;
    let parts = children(ci.body);
    if parts.first()?.tag != 6 || oid_str(parts[0].body) != OID_SIGNED_DATA {
        return None;
    }
    let (sd, _) = tlv(parts.get(1)?.body)?;
    let items = children(sd.body);
    let certs = items
        .iter()
        .find(|t| t.tag == 0xA0)
        .map(|t| children(t.body).iter().map(|c| c.all.to_vec()).collect())
        .unwrap_or_default();
    let signers = items
        .iter()
        .rev()
        .find(|t| t.tag == 0x31)
        .map_or(0, |t| children(t.body).len());
    Some((certs, signers))
}

unsafe fn blob_slice<'a>(p: *const u8, n: u32) -> &'a [u8] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}

unsafe extern "win64" fn CryptGetMessageCertificates(
    _enc: u32,
    _prov: usize,
    _flags: u32,
    data: *const u8,
    len: u32,
) -> usize {
    let Some((certs, _)) = signed_data(blob_slice(data, len)) else {
        set_last_error(CRYPT_E_ASN1_BADTAG);
        return 0;
    };
    let h = new_store();
    for c in &certs {
        store_add_der(h, c);
    }
    h
}

unsafe extern "win64" fn CryptGetMessageSignerCount(_enc: u32, data: *const u8, len: u32) -> i32 {
    match signed_data(blob_slice(data, len)) {
        Some((_, n)) => n as i32,
        None => {
            set_last_error(CRYPT_E_ASN1_BADTAG);
            -1
        }
    }
}

const SAVE_AS_PKCS7: u32 = 2;
const SAVE_TO_FILE: u32 = 1;
const SAVE_TO_MEMORY: u32 = 2;
const SAVE_TO_FILENAME_A: u32 = 3;
const SAVE_TO_FILENAME_W: u32 = 4;

/// CertSaveStore : en PKCS #7, vers un fichier ouvert, un nom de fichier
/// ou la mémoire. Le format « magasin sérialisé » n'est pas servi.
unsafe extern "win64" fn CertSaveStore(
    store: usize,
    _enc: u32,
    save_as: u32,
    save_to: u32,
    para: usize,
    _flags: u32,
) -> i32 {
    if save_as != SAVE_AS_PKCS7 {
        crate::trace!("CertSaveStore : format {save_as} non servi");
        set_last_error(NTE_NOT_SUPPORTED);
        return 0;
    }
    let certs: Vec<Vec<u8>> = store_certs(store)
        .iter()
        .map(|&c| cert_der(c).to_vec())
        .collect();
    let data = degenerate_pkcs7(&certs);
    match save_to {
        SAVE_TO_MEMORY => {
            let b = para as *mut Blob;
            if b.is_null() {
                set_last_error(E_INVALIDARG);
                return 0;
            }
            let mut size = (*b).cb;
            let r = put_out(&data, (*b).pb as *mut u8, &mut size);
            (*b).cb = size;
            r
        }
        SAVE_TO_FILENAME_A | SAVE_TO_FILENAME_W => {
            let name = if save_to == SAVE_TO_FILENAME_A {
                crate::dlls::cstr(para as *const u8)
            } else {
                crate::dlls::wstr(para as *const u16)
            };
            match std::fs::write(crate::paths::to_linux(&name), &data) {
                Ok(()) => 1,
                Err(e) => {
                    set_last_error(crate::dlls::kernel32::io_error_code(&e));
                    0
                }
            }
        }
        SAVE_TO_FILE => {
            let mut written = 0u32;
            crate::dlls::kernel32::write_file(
                para,
                data.as_ptr(),
                data.len() as u32,
                &mut written,
                0,
            )
        }
        _ => {
            set_last_error(E_INVALIDARG);
            0
        }
    }
}

/// Messages signés ou chiffrés et export PFX : pas encore servis.
extern "win64" fn message_not_supported() -> i32 {
    crate::trace!("crypt32 : message PKCS #7 signé ou chiffré, ou export PFX, non servi");
    set_last_error(NTE_NOT_SUPPORTED);
    0
}

// ── Listes de confiance (CTL) : aucune dans Winlux ──────────────────────

extern "win64" fn CertEnumCTLsInStore(_store: usize, _prev: usize) -> usize {
    set_last_error(CRYPT_E_NOT_FOUND);
    0
}
extern "win64" fn CertDuplicateCTLContext(ctx: usize) -> usize {
    ctx
}
extern "win64" fn CertEnumCTLContextProperties(_ctx: usize, _prev: u32) -> u32 {
    0
}
extern "win64" fn CertEnumSubjectInSortedCTL(
    _ctx: usize,
    _next: usize,
    _id: usize,
    _attr: usize,
) -> i32 {
    0
}
/// Ajouter, supprimer ou sérialiser une CTL : aucune n'existe ni n'est
/// conservée.
extern "win64" fn ctl_not_supported() -> i32 {
    set_last_error(CRYPT_E_NOT_FOUND);
    0
}

// ── Sérialisation d'un certificat ───────────────────────────────────────

/// En-tête d'un élément : dwPropId, dwEncodingType, cbData, puis les
/// octets ; le certificat lui-même porte CERT_CERT_PROP_ID (32).
const CERT_CERT_PROP_ID: u32 = 32;

fn element(id: u32, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(12 + data.len());
    v.extend_from_slice(&id.to_le_bytes());
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    v
}

unsafe extern "win64" fn CertSerializeCertificateStoreElement(
    ctx: usize,
    _flags: u32,
    out: *mut u8,
    size: *mut u32,
) -> i32 {
    if ctx == 0 {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let mut v = Vec::new();
    for id in prop_ids(ctx) {
        if [3, 4, 107].contains(&id) {
            continue; // recalculés à la relecture
        }
        let mut n = 0u32;
        if super::crypt32::CertGetCertificateContextProperty(ctx, id, std::ptr::null_mut(), &mut n)
            == 0
        {
            continue;
        }
        let mut buf = vec![0u8; n as usize];
        if super::crypt32::CertGetCertificateContextProperty(ctx, id, buf.as_mut_ptr(), &mut n) != 0
        {
            v.extend(element(id, &buf[..n as usize]));
        }
    }
    v.extend(element(CERT_CERT_PROP_ID, cert_der(ctx)));
    put_out(&v, out, size)
}

/// CertAddSerializedElementToStore : relit un élément sérialisé et
/// l'ajoute au magasin (ou crée seulement le contexte si hCertStore est
/// nul).
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn CertAddSerializedElementToStore(
    store: usize,
    data: *const u8,
    len: u32,
    _disposition: u32,
    _flags: u32,
    _types: u32,
    out_type: *mut u32,
    out_ctx: *mut usize,
) -> i32 {
    let mut b = blob_slice(data, len);
    let mut props = Vec::new();
    let mut der = None;
    while b.len() >= 12 {
        let id = u32::from_le_bytes(b[0..4].try_into().unwrap());
        let n = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
        if b.len() < 12 + n {
            break;
        }
        let v = &b[12..12 + n];
        if id == CERT_CERT_PROP_ID {
            der = Some(v.to_vec());
        } else {
            props.push((id, v.to_vec()));
        }
        b = &b[12 + n..];
    }
    let Some(der) = der else {
        set_last_error(CRYPT_E_NOT_FOUND);
        return 0;
    };
    let target = if store == 0 { new_store() } else { store };
    let Some(ctx) = store_add_der(target, &der) else {
        set_last_error(CRYPT_E_ASN1_BADTAG);
        return 0;
    };
    for (id, v) in &props {
        let blob = Blob {
            cb: v.len() as u32,
            pb: v.as_ptr(),
        };
        super::crypt32::CertSetCertificateContextProperty(ctx, *id, 0, &blob);
    }
    if !out_type.is_null() {
        *out_type = 1; // CERT_STORE_CERTIFICATE_CONTEXT
    }
    if !out_ctx.is_null() {
        *out_ctx = super::crypt32::CertDuplicateCertificateContext(ctx);
    }
    1
}

// ── Identifiants de clé : aucune propriété conservée ────────────────────

extern "win64" fn CryptGetKeyIdentifierProperty() -> i32 {
    set_last_error(CRYPT_E_NOT_FOUND);
    0
}
extern "win64" fn CryptEnumKeyIdentifierProperties() -> i32 {
    1
}

// ── Encodage, décodage, mise en forme ───────────────────────────────────

unsafe extern "win64" fn CryptDecodeObject(
    enc: u32,
    ty: usize,
    data: *const u8,
    len: u32,
    flags: u32,
    out: *mut u8,
    size: *mut u32,
) -> i32 {
    // Sans CRYPT_DECODE_ALLOC_FLAG, la variante Ex a le même contrat.
    super::crypt32::CryptDecodeObjectEx(enc, ty, data, len, flags & !0x8000, 0, out, size)
}

unsafe extern "win64" fn CryptEncodeObject(
    enc: u32,
    ty: usize,
    info: usize,
    out: *mut u8,
    size: *mut u32,
) -> i32 {
    super::crypt32_encode::CryptEncodeObjectEx(enc, ty, info, 0, 0, out, size)
}

/// CryptFormatObject : vidage hexadécimal « 30 82 01 0a … », la mise en
/// forme par défaut de Windows pour un type sans formateur.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn CryptFormatObject(
    _enc: u32,
    _type: u32,
    _strtype: u32,
    _format: usize,
    _struct_type: usize,
    data: *const u8,
    len: u32,
    out: *mut u16,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let text: String = blob_slice(data, len)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let w: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes: Vec<u8> = w.iter().flat_map(|c| c.to_le_bytes()).collect();
    put_out(&bytes, out as *mut u8, size)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CryptFindOIDInfo" => fn CryptFindOIDInfo, "CryptEnumOIDInfo" => fn CryptEnumOIDInfo,
        "CertAlgIdToOID" => fn CertAlgIdToOID, "CertOIDToAlgId" => fn CertOIDToAlgId,
        "CryptFindLocalizedName" => fn CryptFindLocalizedName,
        "CertComparePublicKeyInfo" => fn CertComparePublicKeyInfo,
        "CryptHashPublicKeyInfo" => fn CryptHashPublicKeyInfo,
        "CryptExportPublicKeyInfo" => fn CryptExportPublicKeyInfo,
        "CryptExportPublicKeyInfoEx" => fn CryptExportPublicKeyInfo,
        "CertEnumSystemStoreLocation" => fn CertEnumSystemStoreLocation,
        "CertEnumSystemStore" => fn CertEnumSystemStore,
        "CertEnumPhysicalStore" => fn CertEnumPhysicalStore,
        "CertRegisterSystemStore" => fn CertRegisterSystemStore,
        "CertUnregisterSystemStore" => fn CertUnregisterSystemStore,
        "CertRemoveStoreFromCollection" => fn CertRemoveStoreFromCollection,
        "CertEnumCertificateContextProperties" => fn CertEnumCertificateContextProperties,
        "CertGetValidUsages" => fn CertGetValidUsages,
        "CertVerifySubjectCertificateContext" => fn CertVerifySubjectCertificateContext,
        "CryptGetMessageCertificates" => fn CryptGetMessageCertificates,
        "CryptGetMessageSignerCount" => fn CryptGetMessageSignerCount,
        "CertSaveStore" => fn CertSaveStore,
        "CryptDecodeMessage" => fn message_not_supported,
        "CryptDecryptMessage" => fn message_not_supported,
        "CryptEncryptMessage" => fn message_not_supported,
        "CryptSignMessage" => fn message_not_supported,
        "CryptSignAndEncryptMessage" => fn message_not_supported,
        "CryptDecryptAndVerifyMessageSignature" => fn message_not_supported,
        "CryptVerifyMessageSignature" => fn message_not_supported,
        "CryptVerifyDetachedMessageSignature" => fn message_not_supported,
        "PFXExportCertStoreEx" => fn message_not_supported,
        "CertEnumCTLsInStore" => fn CertEnumCTLsInStore,
        "CertDuplicateCTLContext" => fn CertDuplicateCTLContext,
        "CertEnumCTLContextProperties" => fn CertEnumCTLContextProperties,
        "CertEnumSubjectInSortedCTL" => fn CertEnumSubjectInSortedCTL,
        "CertAddCTLContextToStore" => fn ctl_not_supported,
        "CertAddCTLLinkToStore" => fn ctl_not_supported,
        "CertDeleteCTLFromStore" => fn ctl_not_supported,
        "CertSerializeCTLStoreElement" => fn ctl_not_supported,
        "CertSerializeCertificateStoreElement" => fn CertSerializeCertificateStoreElement,
        "CertAddSerializedElementToStore" => fn CertAddSerializedElementToStore,
        "CryptGetKeyIdentifierProperty" => fn CryptGetKeyIdentifierProperty,
        "CryptEnumKeyIdentifierProperties" => fn CryptEnumKeyIdentifierProperties,
        "CryptDecodeObject" => fn CryptDecodeObject,
        "CryptEncodeObject" => fn CryptEncodeObject,
        "CryptFormatObject" => fn CryptFormatObject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_des_oid_et_algorithmes() {
        unsafe {
            let p = CryptFindOIDInfo(OID_KEY, c"1.2.840.113549.1.1.11".as_ptr() as usize, 0);
            assert_ne!(p, 0);
            let i = &*(p as *const OidInfo);
            assert_eq!(crate::dlls::wstr(i.name), "sha256RSA");
            assert_eq!(i.group, GROUP_SIGN);
            let alg = 0x800cu32;
            let h = CryptFindOIDInfo(ALGID_KEY, &alg as *const u32 as usize, GROUP_HASH);
            assert_eq!(oid_of(&*(h as *const OidInfo)), "2.16.840.1.101.3.4.2.1");
            let sign = [0x8004u32, 0x2400];
            let s = CryptFindOIDInfo(SIGN_KEY, sign.as_ptr() as usize, 0);
            assert_eq!(oid_of(&*(s as *const OidInfo)), "1.2.840.113549.1.1.5");
            assert_eq!(
                CertOIDToAlgId(c"1.3.14.3.2.26".as_ptr() as *const u8),
                0x8004
            );
            let o = CertAlgIdToOID(0x8003);
            assert_eq!(
                CStr::from_ptr(o as *const _).to_bytes(),
                b"1.2.840.113549.2.5"
            );
            let root: Vec<u16> = "root".encode_utf16().chain(Some(0)).collect();
            assert_ne!(CryptFindLocalizedName(root.as_ptr()), 0);
        }
    }

    #[test]
    fn pkcs7_degenere_aller_retour() {
        let a = der_wrap(0x30, &der_wrap(0x02, &[1]));
        let b = der_wrap(0x30, &der_wrap(0x02, &[2]));
        let p7 = degenerate_pkcs7(&[a.clone(), b.clone()]);
        let (certs, signers) = signed_data(&p7).unwrap();
        assert_eq!(certs, vec![a, b]);
        assert_eq!(signers, 0);
        unsafe {
            assert_eq!(
                CryptGetMessageSignerCount(1, p7.as_ptr(), p7.len() as u32),
                0
            );
            assert_eq!(CryptGetMessageSignerCount(1, [5u8, 0].as_ptr(), 2), -1);
        }
        let mut n = 0u32;
        unsafe {
            assert_eq!(
                CryptFormatObject(
                    1,
                    0,
                    0,
                    0,
                    0,
                    [0x30u8, 0x0a].as_ptr(),
                    2,
                    std::ptr::null_mut(),
                    &mut n
                ),
                1
            );
        }
        assert_eq!(n, 12); // « 30 0a » + zéro, en UTF-16
    }

    #[test]
    fn condensat_et_comparaison_de_cles() {
        if super::super::hostssl::ssl().is_none() {
            return;
        }
        let key = [0x30u8, 0x03, 0x02, 0x01, 0x05];
        let params = [0x05u8, 0x00];
        let info = PublicKeyInfo {
            alg: AlgId {
                oid: c"1.2.840.113549.1.1.1".as_ptr(),
                params: Blob {
                    cb: 2,
                    pb: params.as_ptr(),
                },
            },
            key: BitBlob {
                cb: 5,
                pb: key.as_ptr(),
                unused: 0,
            },
        };
        unsafe {
            assert_eq!(CertComparePublicKeyInfo(1, &info, &info), 1);
            let mut n = 0u32;
            assert_eq!(
                CryptHashPublicKeyInfo(0, 0, 0, 1, &info, std::ptr::null_mut(), &mut n),
                1
            );
            assert_eq!(n, 20);
            let der = spki_der(&info).unwrap();
            assert_eq!(der[0], 0x30);
            assert_eq!(&der[der.len() - 6..], &[0, 0x30, 0x03, 0x02, 0x01, 0x05]);
        }
    }
}
