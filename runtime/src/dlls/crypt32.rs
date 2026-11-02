//! crypt32.dll : certificats X.509, magasins et vérification de chaînes.
//!
//! Les certificats sont décodés par un petit lecteur DER maison, qui remplit
//! les vraies structures CERT_CONTEXT et CERT_INFO lues par les applications.
//! Le magasin « ROOT » contient les autorités de l'hôte
//! (/etc/ssl/certs/ca-certificates.crt) ; « CA », « MY » et les magasins en
//! mémoire démarrent vides. La vérification des chaînes passe par l'OpenSSL
//! de l'hôte ([`super::hostssl`]), avec les mêmes autorités.
#![allow(non_snake_case)]

use super::hostssl;
use crate::process::set_last_error;
use std::collections::HashMap;
use std::ffi::{c_char, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const X509_ASN_ENCODING: u32 = 1;
const PKCS_7_ASN_ENCODING: u32 = 0x10000;
const CRYPT_E_NOT_FOUND: u32 = 0x8009_2004;
const CRYPT_E_ASN1_BADTAG: u32 = 0x8009_3102;
const ERROR_MORE_DATA: u32 = 234;
const E_INVALIDARG: u32 = 0x8007_0057;

// ---------- Lecteur DER ----------

#[derive(Clone, Copy)]
pub(crate) struct Tlv<'a> {
    pub tag: u8,
    pub body: &'a [u8],
    /// En-tête compris.
    pub all: &'a [u8],
}

/// Premier élément DER de `b`, et ce qui suit.
pub(crate) fn tlv(b: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    let tag = *b.first()?;
    let l0 = *b.get(1)? as usize;
    let (len, hdr) = if l0 < 0x80 {
        (l0, 2)
    } else {
        let n = l0 & 0x7f;
        if n == 0 || n > 4 || b.len() < 2 + n {
            return None;
        }
        (
            b[2..2 + n]
                .iter()
                .fold(0usize, |a, &x| (a << 8) | x as usize),
            2 + n,
        )
    };
    let end = hdr.checked_add(len)?;
    if end > b.len() {
        return None;
    }
    Some((
        Tlv {
            tag,
            body: &b[hdr..end],
            all: &b[..end],
        },
        &b[end..],
    ))
}

/// Éléments contenus dans le corps d'une SEQUENCE ou d'un SET.
pub(crate) fn children(mut b: &[u8]) -> Vec<Tlv<'_>> {
    let mut v = Vec::new();
    while let Some((t, rest)) = tlv(b) {
        v.push(t);
        b = rest;
    }
    v
}

/// Identifiant d'objet en notation pointée.
pub(crate) fn oid_str(b: &[u8]) -> String {
    let mut parts = Vec::new();
    let mut acc: u64 = 0;
    for (i, &c) in b.iter().enumerate() {
        acc = (acc << 7) | (c & 0x7f) as u64;
        if c & 0x80 == 0 {
            if parts.is_empty() && i < b.len() {
                let first = (acc / 40).min(2);
                parts.push(first);
                parts.push(acc - first * 40);
            } else {
                parts.push(acc);
            }
            acc = 0;
        }
    }
    parts
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

/// UTCTime / GeneralizedTime → FILETIME (centaines de ns depuis 1601).
fn filetime_of(t: &Tlv) -> u64 {
    let s = std::str::from_utf8(t.body).unwrap_or("");
    let digits: Vec<u32> = s
        .bytes()
        .take_while(|c| c.is_ascii_digit())
        .map(|c| (c - b'0') as u32)
        .collect();
    let num = |i: usize, n: usize| {
        digits
            .get(i..i + n)
            .map(|d| d.iter().fold(0, |a, &x| a * 10 + x))
    };
    let (year, off) = if t.tag == 0x17 {
        let y = num(0, 2).unwrap_or(0);
        (if y >= 50 { 1900 + y } else { 2000 + y }, 2)
    } else {
        (num(0, 4).unwrap_or(1970), 4)
    };
    let mon = num(off, 2).unwrap_or(1);
    let day = num(off + 2, 2).unwrap_or(1);
    let h = num(off + 4, 2).unwrap_or(0);
    let mi = num(off + 6, 2).unwrap_or(0);
    let se = num(off + 8, 2).unwrap_or(0);
    // Jours depuis le 1er janvier 1970 (algorithme civil de H. Hinnant).
    let (y, m) = if mon <= 2 {
        (year as i64 - 1, mon + 9)
    } else {
        (year as i64, mon - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m as i64 + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + (h * 3600 + mi * 60 + se) as i64;
    ((secs + 11_644_473_600) as u64).saturating_mul(10_000_000)
}

fn now_filetime() -> u64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs() + 11_644_473_600) * 10_000_000
}

/// Chaîne d'un attribut de nom (UTF8String, PrintableString, BMPString…).
fn text_of(t: &Tlv) -> String {
    match t.tag {
        0x1e => {
            let w: Vec<u16> = t
                .body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            String::from_utf16_lossy(&w)
        }
        0x1c => t
            .body
            .as_chunks::<4>()
            .0
            .iter()
            .filter_map(|c| char::from_u32(u32::from_be_bytes(*c)))
            .collect(),
        0x14 | 0x16 | 0x13 | 0x0c => String::from_utf8_lossy(t.body).into_owned(),
        _ => String::from_utf8_lossy(t.body).into_owned(),
    }
}

/// Attributs (OID, valeur) d'un Name X.501, dans l'ordre d'encodage.
pub(crate) fn name_attrs(name_der: &[u8]) -> Vec<(String, String)> {
    let Some((seq, _)) = tlv(name_der) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    for set in children(seq.body) {
        for atv in children(set.body) {
            let c = children(atv.body);
            if c.len() >= 2 && c[0].tag == 0x06 {
                v.push((oid_str(c[0].body), text_of(&c[1])));
            }
        }
    }
    v
}

fn short_name(oid: &str) -> Option<&'static str> {
    Some(match oid {
        "2.5.4.3" => "CN",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "S",
        "2.5.4.9" => "STREET",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        "2.5.4.12" => "T",
        "2.5.4.42" => "G",
        "2.5.4.4" => "SN",
        "2.5.4.5" => "SERIALNUMBER",
        "1.2.840.113549.1.9.1" => "E",
        "0.9.2342.19200300.100.1.25" => "DC",
        _ => return None,
    })
}

/// Nom lisible façon CertNameToStr (CERT_X500_NAME_STR).
pub(crate) fn name_to_str(name_der: &[u8], reverse: bool) -> String {
    let mut parts: Vec<String> = name_attrs(name_der)
        .into_iter()
        .map(|(oid, val)| {
            let key = short_name(&oid).map(str::to_string).unwrap_or(oid);
            if val.contains([',', '+', '=', '"', '\n', '<', '>', '#', ';']) {
                format!("{key}=\"{}\"", val.replace('"', "\"\""))
            } else {
                format!("{key}={val}")
            }
        })
        .collect();
    if reverse {
        parts.reverse();
    }
    parts.join(", ")
}

// ---------- Contextes de certificat ----------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Blob {
    pub cb: u32,
    pub pb: *const u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AlgId {
    pub oid: *const c_char,
    pub params: Blob,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BitBlob {
    pub cb: u32,
    pub pb: *const u8,
    pub unused: u32,
}

#[repr(C)]
pub struct CertInfo {
    pub version: u32,
    pub serial: Blob,
    pub sig_alg: AlgId,
    pub issuer: Blob,
    pub not_before: [u32; 2],
    pub not_after: [u32; 2],
    pub subject: Blob,
    pub spki_alg: AlgId,
    pub spki_key: BitBlob,
    pub issuer_uid: BitBlob,
    pub subject_uid: BitBlob,
    pub ext_count: u32,
    pub exts: *const Extension,
}

#[repr(C)]
pub struct Extension {
    pub oid: *const c_char,
    pub critical: i32,
    pub value: Blob,
}

/// CERT_CONTEXT tel que vu par l'application.
#[repr(C)]
pub struct CertContext {
    pub encoding: u32,
    pub der: *const u8,
    pub der_len: u32,
    pub info: *const CertInfo,
    pub store: usize,
}

/// Contexte complet : la partie publique en tête, puis les données possédées.
#[repr(C)]
pub struct Cert {
    pub ctx: CertContext,
    info: CertInfo,
    refs: AtomicUsize,
    der: Vec<u8>,
    serial_le: Vec<u8>,
    _oids: Vec<CString>,
    _exts: Vec<Extension>,
    props: Mutex<HashMap<u32, Vec<u8>>>,
}

unsafe impl Send for Cert {}
unsafe impl Sync for Cert {}

const NULL_BLOB: Blob = Blob {
    cb: 0,
    pb: std::ptr::null(),
};
const NULL_BIT: BitBlob = BitBlob {
    cb: 0,
    pb: std::ptr::null(),
    unused: 0,
};

fn blob(b: &[u8]) -> Blob {
    Blob {
        cb: b.len() as u32,
        pb: b.as_ptr(),
    }
}

/// Décode un certificat DER en contexte (référence 1), rattaché à `store`.
pub fn new_cert(der: &[u8], store: usize) -> Option<*mut Cert> {
    let mut c = Box::new(Cert {
        ctx: CertContext {
            encoding: X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            der: std::ptr::null(),
            der_len: 0,
            info: std::ptr::null(),
            store,
        },
        info: CertInfo {
            version: 0,
            serial: NULL_BLOB,
            sig_alg: AlgId {
                oid: std::ptr::null(),
                params: NULL_BLOB,
            },
            issuer: NULL_BLOB,
            not_before: [0; 2],
            not_after: [0; 2],
            subject: NULL_BLOB,
            spki_alg: AlgId {
                oid: std::ptr::null(),
                params: NULL_BLOB,
            },
            spki_key: NULL_BIT,
            issuer_uid: NULL_BIT,
            subject_uid: NULL_BIT,
            ext_count: 0,
            exts: std::ptr::null(),
        },
        refs: AtomicUsize::new(1),
        der: der.to_vec(),
        serial_le: Vec::new(),
        _oids: Vec::new(),
        _exts: Vec::new(),
        props: Mutex::new(HashMap::new()),
    });
    // Les tranches pointent dans c.der, qui ne bouge plus (Vec dans une Box).
    let der: &[u8] = unsafe { std::slice::from_raw_parts(c.der.as_ptr(), c.der.len()) };
    let (cert, _) = tlv(der)?;
    let top = children(cert.body);
    let tbs = top.first().filter(|t| t.tag == 0x30)?;
    let mut f = children(tbs.body).into_iter().peekable();
    let mut oids = Vec::new();
    let alg = |t: &Tlv, oids: &mut Vec<CString>| -> AlgId {
        let c = children(t.body);
        let oid = CString::new(c.first().map(|o| oid_str(o.body)).unwrap_or_default())
            .unwrap_or_default();
        let p = oid.as_ptr();
        oids.push(oid);
        AlgId {
            oid: p,
            params: c.get(1).map(|x| blob(x.all)).unwrap_or(NULL_BLOB),
        }
    };
    if f.peek().map(|t| t.tag) == Some(0xa0) {
        let v = f.next()?;
        c.info.version = tlv(v.body)
            .and_then(|(i, _)| i.body.last().copied())
            .unwrap_or(0) as u32;
    }
    let serial = f.next()?;
    c.serial_le = serial.body.iter().rev().copied().collect();
    let sig = f.next()?;
    c.info.sig_alg = alg(&sig, &mut oids);
    let issuer = f.next()?;
    c.info.issuer = blob(issuer.all);
    let validity = children(f.next()?.body);
    let nb = validity.first().map(filetime_of).unwrap_or(0);
    let na = validity.get(1).map(filetime_of).unwrap_or(0);
    c.info.not_before = [nb as u32, (nb >> 32) as u32];
    c.info.not_after = [na as u32, (na >> 32) as u32];
    let subject = f.next()?;
    c.info.subject = blob(subject.all);
    let spki = children(f.next()?.body);
    if let Some(a) = spki.first() {
        c.info.spki_alg = alg(a, &mut oids);
    }
    if let Some(k) = spki.get(1).filter(|k| k.tag == 0x03 && !k.body.is_empty()) {
        c.info.spki_key = BitBlob {
            cb: k.body.len() as u32 - 1,
            pb: k.body[1..].as_ptr(),
            unused: k.body[0] as u32,
        };
    }
    let mut exts = Vec::new();
    for t in f {
        match t.tag {
            0x81 | 0x82 if !t.body.is_empty() => {
                let b = BitBlob {
                    cb: t.body.len() as u32 - 1,
                    pb: t.body[1..].as_ptr(),
                    unused: t.body[0] as u32,
                };
                if t.tag == 0x81 {
                    c.info.issuer_uid = b;
                } else {
                    c.info.subject_uid = b;
                }
            }
            0xa3 => {
                let Some((seq, _)) = tlv(t.body) else {
                    continue;
                };
                for e in children(seq.body) {
                    let parts = children(e.body);
                    let Some(oid) = parts.first() else { continue };
                    let critical =
                        parts.len() == 3 && parts[1].body.first().copied().unwrap_or(0) != 0;
                    let Some(val) = parts.last().filter(|v| v.tag == 0x04) else {
                        continue;
                    };
                    let s = CString::new(oid_str(oid.body)).unwrap_or_default();
                    exts.push(Extension {
                        oid: s.as_ptr(),
                        critical: critical as i32,
                        value: blob(val.body),
                    });
                    oids.push(s);
                }
            }
            _ => {}
        }
    }
    c.info.serial = Blob {
        cb: c.serial_le.len() as u32,
        pb: c.serial_le.as_ptr(),
    };
    c.info.ext_count = exts.len() as u32;
    c.info.exts = if exts.is_empty() {
        std::ptr::null()
    } else {
        exts.as_ptr()
    };
    c._exts = exts;
    c._oids = oids;
    c.ctx.der = c.der.as_ptr();
    c.ctx.der_len = c.der.len() as u32;
    let p = Box::into_raw(c);
    unsafe { (*p).ctx.info = &(*p).info };
    Some(p)
}

/// Le contexte derrière un PCCERT_CONTEXT créé par Winlux.
unsafe fn cert<'a>(ctx: usize) -> Option<&'a Cert> {
    (ctx != 0).then(|| &*(ctx as *const Cert))
}

pub fn cert_der<'a>(ctx: usize) -> &'a [u8] {
    unsafe { cert(ctx).map(|c| c.der.as_slice()).unwrap_or(&[]) }
}

fn add_ref(ctx: usize) -> usize {
    if let Some(c) = unsafe { cert(ctx) } {
        c.refs.fetch_add(1, Ordering::AcqRel);
    }
    ctx
}

fn release(ctx: usize) {
    if let Some(c) = unsafe { cert(ctx) } {
        if c.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            drop(unsafe { Box::from_raw(ctx as *mut Cert) });
        }
    }
}

// ---------- Magasins ----------

pub struct Store {
    certs: Mutex<Vec<usize>>,
    refs: AtomicUsize,
    /// Le magasin ROOT partagé n'est jamais libéré.
    permanent: bool,
}

/// Nouveau magasin en mémoire. La structure elle-même n'est jamais libérée,
/// car des contextes encore vivants peuvent la désigner (hCertStore).
pub fn new_store() -> usize {
    Box::into_raw(Box::new(Store {
        certs: Mutex::new(Vec::new()),
        refs: AtomicUsize::new(1),
        permanent: false,
    })) as usize
}

unsafe fn store<'a>(h: usize) -> Option<&'a Store> {
    (h > 0x1_0000).then(|| &*(h as *const Store))
}

/// Ajoute un certificat DER à un magasin ; renvoie le contexte (sans référence en plus).
pub fn store_add_der(h: usize, der: &[u8]) -> Option<usize> {
    let s = unsafe { store(h) }?;
    let mut certs = super::lock(&s.certs);
    if let Some(&c) = certs.iter().find(|&&c| cert_der(c) == der) {
        return Some(c);
    }
    let c = new_cert(der, h)? as usize;
    certs.push(c);
    Some(c)
}

pub fn store_certs(h: usize) -> Vec<usize> {
    unsafe { store(h) }
        .map(|s| super::lock(&s.certs).clone())
        .unwrap_or_default()
}

/// Certificats PEM d'un fichier (blocs BEGIN CERTIFICATE).
pub fn pem_certs(text: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(b) = rest.find("-----BEGIN CERTIFICATE-----") {
        let body = &rest[b + 27..];
        let Some(e) = body.find("-----END CERTIFICATE-----") else {
            break;
        };
        if let Some(der) = crate::imaging::base64(&body[..e]) {
            out.push(der);
        }
        rest = &body[e..];
        rest = &rest[25..];
    }
    out
}

fn root_store() -> usize {
    static ROOT: OnceLock<usize> = OnceLock::new();
    *ROOT.get_or_init(|| {
        let h = Box::into_raw(Box::new(Store {
            certs: Mutex::new(Vec::new()),
            refs: AtomicUsize::new(1),
            permanent: true,
        })) as usize;
        for path in [
            "/etc/ssl/certs/ca-certificates.crt",
            "/etc/pki/tls/certs/ca-bundle.crt",
        ] {
            if let Ok(t) = std::fs::read_to_string(path) {
                for der in pem_certs(&t) {
                    store_add_der(h, &der);
                }
                break;
            }
        }
        h
    })
}

fn open_system(name: &str) -> usize {
    if name.eq_ignore_ascii_case("ROOT") || name.eq_ignore_ascii_case("AuthRoot") {
        root_store()
    } else {
        new_store()
    }
}

pub unsafe extern "win64" fn CertOpenSystemStoreW(_prov: usize, name: *const u16) -> usize {
    open_system(&super::wstr(name))
}

pub unsafe extern "win64" fn CertOpenSystemStoreA(_prov: usize, name: *const u8) -> usize {
    open_system(&super::cstr(name))
}

pub unsafe extern "win64" fn CertOpenStore(
    provider: usize,
    _enc: u32,
    _prov: usize,
    _flags: u32,
    para: usize,
) -> usize {
    let id = if provider < 0x1_0000 {
        provider
    } else {
        match super::cstr(provider as *const u8)
            .to_ascii_lowercase()
            .as_str()
        {
            "system" => 10,
            "memory" => 2,
            "collection" => 11,
            _ => 0,
        }
    };
    match id {
        9 => open_system(&super::cstr(para as *const u8)),
        10 => open_system(&super::wstr(para as *const u16)),
        // Mémoire, collection, PKCS #7 et fichiers : magasin en mémoire.
        _ => new_store(),
    }
}

pub extern "win64" fn CertDuplicateStore(h: usize) -> usize {
    if let Some(s) = unsafe { store(h) } {
        s.refs.fetch_add(1, Ordering::AcqRel);
    }
    h
}

pub extern "win64" fn CertCloseStore(h: usize, _flags: u32) -> i32 {
    if let Some(s) = unsafe { store(h) } {
        if !s.permanent && s.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            for c in std::mem::take(&mut *super::lock(&s.certs)) {
                release(c);
            }
        }
    }
    1
}

pub extern "win64" fn CertAddStoreToCollection(
    coll: usize,
    sibling: usize,
    _flags: u32,
    _prio: u32,
) -> i32 {
    for c in store_certs(sibling) {
        store_add_der(coll, cert_der(c));
    }
    1
}

pub unsafe extern "win64" fn CertEnumCertificatesInStore(h: usize, prev: usize) -> usize {
    let certs = store_certs(h);
    let next = match certs.iter().position(|&c| c == prev) {
        _ if prev == 0 => certs.first().copied(),
        Some(i) => certs.get(i + 1).copied(),
        None => None,
    };
    if prev != 0 {
        release(prev);
    }
    match next {
        Some(c) => add_ref(c),
        None => {
            set_last_error(CRYPT_E_NOT_FOUND);
            0
        }
    }
}

pub extern "win64" fn CertFreeCertificateContext(ctx: usize) -> i32 {
    release(ctx);
    1
}

pub extern "win64" fn CertDuplicateCertificateContext(ctx: usize) -> usize {
    add_ref(ctx)
}

pub unsafe extern "win64" fn CertCreateCertificateContext(
    _enc: u32,
    der: *const u8,
    len: u32,
) -> usize {
    if der.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let bytes = std::slice::from_raw_parts(der, len as usize);
    match store_add_der(new_store(), bytes) {
        Some(c) => add_ref(c),
        None => {
            set_last_error(CRYPT_E_ASN1_BADTAG);
            0
        }
    }
}

pub unsafe extern "win64" fn CertAddEncodedCertificateToStore(
    h: usize,
    _enc: u32,
    der: *const u8,
    len: u32,
    _disp: u32,
    out: *mut usize,
) -> i32 {
    if der.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let target = if store(h).is_some() { h } else { new_store() };
    match store_add_der(target, std::slice::from_raw_parts(der, len as usize)) {
        Some(c) => {
            if !out.is_null() {
                *out = add_ref(c);
            }
            1
        }
        None => {
            set_last_error(CRYPT_E_ASN1_BADTAG);
            0
        }
    }
}

pub unsafe extern "win64" fn CertAddCertificateContextToStore(
    h: usize,
    ctx: usize,
    _disp: u32,
    out: *mut usize,
) -> i32 {
    let der = cert_der(ctx).to_vec();
    CertAddEncodedCertificateToStore(h, X509_ASN_ENCODING, der.as_ptr(), der.len() as u32, 0, out)
}

pub unsafe extern "win64" fn CertCompareCertificate(
    _enc: u32,
    a: *const CertInfo,
    b: *const CertInfo,
) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    let s = |x: &Blob| {
        if x.pb.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(x.pb, x.cb as usize)
        }
    };
    (s(&(*a).serial) == s(&(*b).serial) && s(&(*a).issuer) == s(&(*b).issuer)) as i32
}

fn hash_prop(der: &[u8], id: u32) -> Option<Vec<u8>> {
    match id {
        3 => hostssl::digest(c"SHA1", der),
        4 => hostssl::digest(c"MD5", der),
        107 => hostssl::digest(c"SHA256", der),
        _ => None,
    }
}

pub unsafe extern "win64" fn CertFindCertificateInStore(
    h: usize,
    _enc: u32,
    _flags: u32,
    find: u32,
    para: usize,
    prev: usize,
) -> usize {
    let certs = store_certs(h);
    let start = if prev == 0 {
        0
    } else {
        certs
            .iter()
            .position(|&c| c == prev)
            .map_or(certs.len(), |i| i + 1)
    };
    if prev != 0 {
        release(prev);
    }
    let para_blob = || -> &[u8] {
        let b = para as *const Blob;
        if b.is_null() || (*b).pb.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts((*b).pb, (*b).cb as usize)
        }
    };
    let wanted_str = match find {
        0x80007 | 0x80004 => super::wstr(para as *const u16).to_lowercase(),
        0x70007 | 0x70004 => super::cstr(para as *const u8).to_lowercase(),
        _ => String::new(),
    };
    for &c in &certs[start.min(certs.len())..] {
        let k = cert(c).unwrap();
        let name = |issuer: bool| {
            let b = if issuer {
                &k.info.issuer
            } else {
                &k.info.subject
            };
            std::slice::from_raw_parts(b.pb, b.cb as usize)
        };
        let hit = match find {
            0 => true,
            0x10000 => hash_prop(&k.der, 3).as_deref() == Some(para_blob()),
            0x40000 => hash_prop(&k.der, 4).as_deref() == Some(para_blob()),
            0x20007 => name(false) == para_blob(),
            0x20004 => name(true) == para_blob(),
            0x80007 | 0x70007 => name_to_str(name(false), false)
                .to_lowercase()
                .contains(&wanted_str),
            0x80004 | 0x70004 => name_to_str(name(true), false)
                .to_lowercase()
                .contains(&wanted_str),
            0xd0000 => k.der.as_slice() == cert_der(para),
            _ => false,
        };
        if hit {
            return add_ref(c);
        }
    }
    set_last_error(CRYPT_E_NOT_FOUND);
    0
}

pub unsafe extern "win64" fn CertGetCertificateContextProperty(
    ctx: usize,
    id: u32,
    data: *mut u8,
    size: *mut u32,
) -> i32 {
    let Some(c) = cert(ctx) else {
        set_last_error(E_INVALIDARG);
        return 0;
    };
    let value = super::lock(&c.props)
        .get(&id)
        .cloned()
        .or_else(|| hash_prop(&c.der, id));
    let Some(v) = value else {
        set_last_error(CRYPT_E_NOT_FOUND);
        return 0;
    };
    if size.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    if data.is_null() {
        *size = v.len() as u32;
        return 1;
    }
    if (*size as usize) < v.len() {
        *size = v.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), data, v.len());
    *size = v.len() as u32;
    1
}

pub unsafe extern "win64" fn CertSetCertificateContextProperty(
    ctx: usize,
    id: u32,
    _flags: u32,
    data: *const Blob,
) -> i32 {
    let Some(c) = cert(ctx) else { return 0 };
    let mut props = super::lock(&c.props);
    if data.is_null() {
        props.remove(&id);
    } else if !(*data).pb.is_null() {
        // Propriétés de type blob (nom convivial, etc.) ; les autres sont gardées telles quelles.
        props.insert(
            id,
            std::slice::from_raw_parts((*data).pb, (*data).cb as usize).to_vec(),
        );
    }
    1
}

pub unsafe extern "win64" fn CertFindExtension(
    oid: *const u8,
    count: u32,
    exts: *const Extension,
) -> usize {
    let want = super::cstr(oid);
    for i in 0..count as usize {
        let e = exts.add(i);
        if super::cstr((*e).oid as *const u8) == want {
            return e as usize;
        }
    }
    0
}

pub unsafe extern "win64" fn CertVerifyTimeValidity(
    time: *const u64,
    info: *const CertInfo,
) -> i32 {
    if info.is_null() {
        return 0;
    }
    let t = if time.is_null() {
        now_filetime()
    } else {
        time.read_unaligned()
    };
    let ft = |x: [u32; 2]| x[0] as u64 | (x[1] as u64) << 32;
    if t < ft((*info).not_before) {
        -1
    } else if t > ft((*info).not_after) {
        1
    } else {
        0
    }
}

// ---------- Noms ----------

/// Noms DNS de l'extension subjectAltName.
fn dns_names(c: &Cert) -> Vec<String> {
    let mut v = Vec::new();
    for e in &c._exts {
        if unsafe { super::cstr(e.oid as *const u8) } != "2.5.29.17" {
            continue;
        }
        let val = unsafe { std::slice::from_raw_parts(e.value.pb, e.value.cb as usize) };
        if let Some((seq, _)) = tlv(val) {
            for g in children(seq.body) {
                if g.tag == 0x82 {
                    v.push(String::from_utf8_lossy(g.body).into_owned());
                }
            }
        }
    }
    v
}

fn attr(name: &[u8], oid: &str) -> Option<String> {
    name_attrs(name)
        .into_iter()
        .find(|(o, _)| o == oid)
        .map(|(_, v)| v)
}

unsafe fn name_string(ctx: usize, kind: u32, flags: u32, para: usize) -> String {
    let Some(c) = cert(ctx) else {
        return String::new();
    };
    let b = if flags & 1 != 0 {
        &c.info.issuer
    } else {
        &c.info.subject
    };
    let name = std::slice::from_raw_parts(b.pb, b.cb as usize);
    match kind {
        1 => attr(name, "1.2.840.113549.1.9.1").unwrap_or_default(),
        2 => name_to_str(name, false),
        3 => attr(name, &super::cstr(para as *const u8)).unwrap_or_default(),
        6 if flags & 1 == 0 => dns_names(c)
            .into_iter()
            .next()
            .or_else(|| attr(name, "2.5.4.3"))
            .unwrap_or_default(),
        6 => attr(name, "2.5.4.3").unwrap_or_default(),
        _ => ["2.5.4.3", "2.5.4.11", "2.5.4.10", "1.2.840.113549.1.9.1"]
            .iter()
            .find_map(|o| attr(name, o))
            .unwrap_or_default(),
    }
}

unsafe fn put_w(s: &str, buf: *mut u16, cap: u32) -> u32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() || cap == 0 {
        return w.len() as u32 + 1;
    }
    let n = w.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32 + 1
}

unsafe fn put_a(s: &str, buf: *mut u8, cap: u32) -> u32 {
    let a = super::encode_ansi(s);
    if buf.is_null() || cap == 0 {
        return a.len() as u32 + 1;
    }
    let n = a.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32 + 1
}

pub unsafe extern "win64" fn CertGetNameStringW(
    ctx: usize,
    kind: u32,
    flags: u32,
    para: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    put_w(&name_string(ctx, kind, flags, para), buf, size)
}

pub unsafe extern "win64" fn CertGetNameStringA(
    ctx: usize,
    kind: u32,
    flags: u32,
    para: usize,
    buf: *mut u8,
    size: u32,
) -> u32 {
    put_a(&name_string(ctx, kind, flags, para), buf, size)
}

unsafe fn blob_slice<'a>(b: *const Blob) -> &'a [u8] {
    if b.is_null() || (*b).pb.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts((*b).pb, (*b).cb as usize)
    }
}

/// CERT_NAME_STR_REVERSE_FLAG = 0x0200_0000.
pub unsafe extern "win64" fn CertNameToStrW(
    _enc: u32,
    name: *const Blob,
    kind: u32,
    buf: *mut u16,
    size: u32,
) -> u32 {
    put_w(
        &name_to_str(blob_slice(name), kind & 0x0200_0000 != 0),
        buf,
        size,
    )
}

pub unsafe extern "win64" fn CertNameToStrA(
    _enc: u32,
    name: *const Blob,
    kind: u32,
    buf: *mut u8,
    size: u32,
) -> u32 {
    put_a(
        &name_to_str(blob_slice(name), kind & 0x0200_0000 != 0),
        buf,
        size,
    )
}

// ---------- Chaînes ----------

#[repr(C)]
struct TrustStatus {
    error: u32,
    info: u32,
}

#[repr(C)]
struct ChainElement {
    cb: u32,
    cert: usize,
    trust: TrustStatus,
    revocation: usize,
    issuance: usize,
    application: usize,
    extended_error: usize,
}

#[repr(C)]
struct SimpleChain {
    cb: u32,
    trust: TrustStatus,
    count: u32,
    elements: *const *const ChainElement,
    trust_list: usize,
    has_freshness: i32,
    freshness: u32,
}

#[repr(C)]
struct ChainContext {
    cb: u32,
    trust: TrustStatus,
    count: u32,
    chains: *const *const SimpleChain,
    lower_count: u32,
    lower: usize,
    has_freshness: i32,
    freshness: u32,
    create_flags: u32,
    id: [u8; 16],
}

/// Chaîne complète : la structure publique en tête.
#[repr(C)]
struct Chain {
    ctx: ChainContext,
    refs: AtomicUsize,
    simple: Box<SimpleChain>,
    simple_ptrs: Vec<*const SimpleChain>,
    elements: Vec<ChainElement>,
    element_ptrs: Vec<*const ChainElement>,
    /// Erreur X509_V_* d'OpenSSL.
    x509_error: i32,
    store: usize,
}

const CERT_TRUST_IS_NOT_TIME_VALID: u32 = 0x1;
const CERT_TRUST_IS_REVOKED: u32 = 0x4;
const CERT_TRUST_IS_NOT_SIGNATURE_VALID: u32 = 0x8;
const CERT_TRUST_IS_UNTRUSTED_ROOT: u32 = 0x20;
const CERT_TRUST_IS_PARTIAL_CHAIN: u32 = 0x1_0000;

fn trust_error(x509: i32) -> u32 {
    match x509 {
        0 => 0,
        9 | 10 | 13 | 14 => CERT_TRUST_IS_NOT_TIME_VALID,
        18 | 19 => CERT_TRUST_IS_UNTRUSTED_ROOT,
        7 | 6 | 4 => CERT_TRUST_IS_NOT_SIGNATURE_VALID,
        23 => CERT_TRUST_IS_REVOKED,
        _ => CERT_TRUST_IS_PARTIAL_CHAIN,
    }
}

pub unsafe extern "win64" fn CertGetCertificateChain(
    _engine: usize,
    ctx: usize,
    _time: usize,
    extra_store: usize,
    _para: usize,
    _flags: u32,
    _reserved: usize,
    out: *mut usize,
) -> i32 {
    let Some(leaf) = cert(ctx) else {
        set_last_error(E_INVALIDARG);
        return 0;
    };
    if out.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let mut extra: Vec<Vec<u8>> = Vec::new();
    for h in [leaf.ctx.store, extra_store] {
        for c in store_certs(h) {
            let d = cert_der(c);
            if d != leaf.der.as_slice() && !extra.iter().any(|e| e == d) {
                extra.push(d.to_vec());
            }
        }
    }
    let (x509_error, ders) = match hostssl::verify(&leaf.der, &extra, None) {
        Some(v) => (v.error, v.chain),
        // Sans OpenSSL : chaîne d'un seul élément, racine inconnue.
        None => (20, vec![leaf.der.clone()]),
    };
    let err = trust_error(x509_error);
    let store = new_store();
    let mut elements = Vec::new();
    for (i, d) in ders.iter().enumerate() {
        let Some(c) = store_add_der(store, d) else {
            continue;
        };
        let last = i + 1 == ders.len();
        elements.push(ChainElement {
            cb: std::mem::size_of::<ChainElement>() as u32,
            cert: add_ref(c),
            trust: TrustStatus {
                error: if last || i == 0 { err } else { 0 },
                // CERT_TRUST_IS_SELF_SIGNED pour la racine, HAS_NAME_MATCH_ISSUER sinon.
                info: if last { 0x8 } else { 0x4 },
            },
            revocation: 0,
            issuance: 0,
            application: 0,
            extended_error: 0,
        });
    }
    let element_ptrs: Vec<*const ChainElement> =
        elements.iter().map(|e| e as *const ChainElement).collect();
    let simple = Box::new(SimpleChain {
        cb: std::mem::size_of::<SimpleChain>() as u32,
        trust: TrustStatus {
            error: err,
            info: 0,
        },
        count: element_ptrs.len() as u32,
        elements: element_ptrs.as_ptr(),
        trust_list: 0,
        has_freshness: 0,
        freshness: 0,
    });
    let simple_ptrs = vec![&*simple as *const SimpleChain];
    let chain = Box::new(Chain {
        ctx: ChainContext {
            cb: std::mem::size_of::<ChainContext>() as u32,
            trust: TrustStatus {
                error: err,
                info: 0,
            },
            count: 1,
            chains: simple_ptrs.as_ptr(),
            lower_count: 0,
            lower: 0,
            has_freshness: 0,
            freshness: 0,
            create_flags: 0,
            id: [0; 16],
        },
        refs: AtomicUsize::new(1),
        simple,
        simple_ptrs,
        elements,
        element_ptrs,
        x509_error,
        store,
    });
    *out = Box::into_raw(chain) as usize;
    1
}

pub unsafe extern "win64" fn CertFreeCertificateChain(chain: usize) {
    if chain == 0 {
        return;
    }
    let c = &*(chain as *const Chain);
    if c.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
        let c = Box::from_raw(chain as *mut Chain);
        for e in &c.elements {
            release(e.cert);
        }
        CertCloseStore(c.store, 0);
        let _ = (&c.simple, &c.simple_ptrs, &c.element_ptrs);
    }
}

pub unsafe extern "win64" fn CertDuplicateCertificateChain(chain: usize) -> usize {
    if chain != 0 {
        (*(chain as *const Chain))
            .refs
            .fetch_add(1, Ordering::AcqRel);
    }
    chain
}

#[repr(C)]
struct PolicyPara {
    cb: u32,
    flags: u32,
    extra: usize,
}

#[repr(C)]
struct SslExtraPara {
    cb: u32,
    auth_type: u32,
    checks: u32,
    server_name: *const u16,
}

#[repr(C)]
struct PolicyStatus {
    cb: u32,
    error: u32,
    chain_index: i32,
    element_index: i32,
    extra: usize,
}

const CERT_E_EXPIRED: u32 = 0x800B_0101;
const CERT_E_UNTRUSTEDROOT: u32 = 0x800B_0109;
const CERT_E_CHAINING: u32 = 0x800B_010A;
const CERT_E_CN_NO_MATCH: u32 = 0x800B_010F;
const TRUST_E_CERT_SIGNATURE: u32 = 0x8009_6004;
const CRYPT_E_REVOKED: u32 = 0x8009_2010;

/// Erreur de stratégie d'une chaîne, selon les drapeaux d'indulgence.
fn policy_error(trust: u32, flags: u32, ssl_checks: u32) -> u32 {
    let ignore_time = flags & 0x7 != 0 || ssl_checks & 0x2000 != 0;
    let ignore_ca = flags & 0x10 != 0 || ssl_checks & 0x100 != 0;
    if trust & CERT_TRUST_IS_NOT_SIGNATURE_VALID != 0 {
        TRUST_E_CERT_SIGNATURE
    } else if trust & CERT_TRUST_IS_REVOKED != 0 {
        CRYPT_E_REVOKED
    } else if trust & CERT_TRUST_IS_NOT_TIME_VALID != 0 && !ignore_time {
        CERT_E_EXPIRED
    } else if trust & CERT_TRUST_IS_UNTRUSTED_ROOT != 0 && !ignore_ca {
        CERT_E_UNTRUSTEDROOT
    } else if trust & CERT_TRUST_IS_PARTIAL_CHAIN != 0 && !ignore_ca {
        CERT_E_CHAINING
    } else {
        0
    }
}

pub unsafe extern "win64" fn CertVerifyCertificateChainPolicy(
    policy: usize,
    chain: usize,
    para: usize,
    status: usize,
) -> i32 {
    let st = status as *mut PolicyStatus;
    if chain == 0 || st.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let c = &*(chain as *const Chain);
    let p = para as *const PolicyPara;
    let flags = if p.is_null() { 0 } else { (*p).flags };
    let ssl = if policy == 4 && !p.is_null() && (*p).extra != 0 {
        (*p).extra as *const SslExtraPara
    } else {
        std::ptr::null()
    };
    let checks = if ssl.is_null() { 0 } else { (*ssl).checks };
    let mut error = policy_error(trust_error(c.x509_error), flags, checks);
    // Nom du serveur (stratégie SSL, côté client : dwAuthType = 2).
    if error == 0 && !ssl.is_null() && !(*ssl).server_name.is_null() && checks & 0x1000 == 0 {
        let host = super::wstr((*ssl).server_name);
        let leaf = c.elements.first().map(|e| cert_der(e.cert)).unwrap_or(&[]);
        if !host.is_empty() && !hostssl::check_host(leaf, &host) {
            error = CERT_E_CN_NO_MATCH;
        }
    }
    (*st).error = error;
    (*st).chain_index = if error == 0 { -1 } else { 0 };
    (*st).element_index = if error == 0 { -1 } else { 0 };
    1
}

pub extern "win64" fn CertCreateCertificateChainEngine(_config: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        unsafe { *out = 0x7000_0002 };
    }
    1
}

pub extern "win64" fn CertFreeCertificateChainEngine(_engine: usize) {}

// ---------- Conversions texte ----------

const CRYPT_STRING_BASE64HEADER: u32 = 0;
const CRYPT_STRING_BASE64: u32 = 1;
const CRYPT_STRING_BINARY: u32 = 2;
const CRYPT_STRING_BASE64REQUESTHEADER: u32 = 3;
const CRYPT_STRING_HEX: u32 = 4;
const CRYPT_STRING_BASE64_ANY: u32 = 6;
const CRYPT_STRING_ANY: u32 = 7;
const CRYPT_STRING_HEX_ANY: u32 = 8;
const CRYPT_STRING_BASE64X509CRLHEADER: u32 = 9;
const CRYPT_STRING_HEXRAW: u32 = 12;
const CRYPT_STRING_NOCRLF: u32 = 0x4000_0000;
const CRYPT_STRING_NOCR: u32 = 0x8000_0000;

fn unhex(s: &str) -> Option<Vec<u8>> {
    let d: Vec<u8> = s.bytes().filter(|c| c.is_ascii_hexdigit()).collect();
    if !d.len().is_multiple_of(2) {
        return None;
    }
    d.chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

/// Décode `text` selon le format CRYPT_STRING_* ; renvoie (octets, format reconnu).
fn string_to_binary(text: &str, format: u32) -> Option<(Vec<u8>, u32)> {
    let header_body = |t: &str| -> Option<String> {
        let b = t.find("-----BEGIN")?;
        let after = t[b..].find('\n').map(|i| b + i + 1)?;
        let e = t[after..].find("-----END")? + after;
        Some(t[after..e].to_string())
    };
    match format & 0xffff {
        CRYPT_STRING_BASE64HEADER
        | CRYPT_STRING_BASE64REQUESTHEADER
        | CRYPT_STRING_BASE64X509CRLHEADER => {
            Some((crate::imaging::base64(&header_body(text)?)?, format))
        }
        CRYPT_STRING_BASE64 => Some((crate::imaging::base64(text)?, format)),
        CRYPT_STRING_BINARY => Some((text.as_bytes().to_vec(), format)),
        CRYPT_STRING_HEX | CRYPT_STRING_HEXRAW | CRYPT_STRING_HEX_ANY => {
            Some((unhex(text)?, CRYPT_STRING_HEX))
        }
        CRYPT_STRING_BASE64_ANY => string_to_binary(text, CRYPT_STRING_BASE64HEADER)
            .or_else(|| string_to_binary(text, CRYPT_STRING_BASE64)),
        CRYPT_STRING_ANY => string_to_binary(text, CRYPT_STRING_BASE64_ANY)
            .or_else(|| Some((text.as_bytes().to_vec(), CRYPT_STRING_BINARY))),
        _ => None,
    }
}

unsafe fn finish_binary(
    r: Option<(Vec<u8>, u32)>,
    bin: *mut u8,
    pcb: *mut u32,
    skip: *mut u32,
    pflags: *mut u32,
) -> i32 {
    let Some((bytes, fmt)) = r else {
        set_last_error(0x8009_0006 /* NTE_BAD_SIGNATURE, comme Windows */);
        return 0;
    };
    if pcb.is_null() {
        set_last_error(87);
        return 0;
    }
    if !skip.is_null() {
        *skip = 0;
    }
    if !pflags.is_null() {
        *pflags = fmt & 0xffff;
    }
    if bin.is_null() {
        *pcb = bytes.len() as u32;
        return 1;
    }
    if (*pcb as usize) < bytes.len() {
        *pcb = bytes.len() as u32;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), bin, bytes.len());
    *pcb = bytes.len() as u32;
    1
}

pub unsafe extern "win64" fn CryptStringToBinaryW(
    sz: *const u16,
    cch: u32,
    flags: u32,
    bin: *mut u8,
    pcb: *mut u32,
    skip: *mut u32,
    pflags: *mut u32,
) -> i32 {
    let text = if cch == 0 {
        super::wstr(sz)
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(sz, cch as usize))
    };
    finish_binary(string_to_binary(&text, flags), bin, pcb, skip, pflags)
}

pub unsafe extern "win64" fn CryptStringToBinaryA(
    sz: *const u8,
    cch: u32,
    flags: u32,
    bin: *mut u8,
    pcb: *mut u32,
    skip: *mut u32,
    pflags: *mut u32,
) -> i32 {
    if flags & 0xffff == CRYPT_STRING_BINARY && cch != 0 {
        let bytes = std::slice::from_raw_parts(sz, cch as usize).to_vec();
        return finish_binary(Some((bytes, CRYPT_STRING_BINARY)), bin, pcb, skip, pflags);
    }
    let text = if cch == 0 {
        super::cstr(sz)
    } else {
        String::from_utf8_lossy(std::slice::from_raw_parts(sz, cch as usize)).into_owned()
    };
    finish_binary(string_to_binary(&text, flags), bin, pcb, skip, pflags)
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn binary_to_string(data: &[u8], flags: u32) -> Option<String> {
    let eol = if flags & CRYPT_STRING_NOCRLF != 0 {
        ""
    } else if flags & CRYPT_STRING_NOCR != 0 {
        "\n"
    } else {
        "\r\n"
    };
    let lines = |s: String| -> String {
        if eol.is_empty() {
            return s;
        }
        s.as_bytes()
            .chunks(64)
            .map(|l| format!("{}{eol}", std::str::from_utf8(l).unwrap_or("")))
            .collect()
    };
    Some(match flags & 0xffff {
        CRYPT_STRING_BASE64 => lines(b64(data)),
        CRYPT_STRING_BASE64HEADER => format!(
            "-----BEGIN CERTIFICATE-----{eol}{}-----END CERTIFICATE-----{eol}",
            lines(b64(data))
        ),
        CRYPT_STRING_HEXRAW => {
            let mut s: String = data.iter().map(|b| format!("{b:02x}")).collect();
            s.push_str(eol);
            s
        }
        CRYPT_STRING_HEX => {
            let mut s = data
                .chunks(16)
                .map(|l| {
                    l.iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(eol);
            s.push_str(eol);
            s
        }
        CRYPT_STRING_BINARY => String::from_utf8_lossy(data).into_owned(),
        _ => return None,
    })
}

pub unsafe extern "win64" fn CryptBinaryToStringW(
    bin: *const u8,
    cb: u32,
    flags: u32,
    sz: *mut u16,
    pcch: *mut u32,
) -> i32 {
    let data = if bin.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(bin, cb as usize)
    };
    let (Some(s), false) = (binary_to_string(data, flags), pcch.is_null()) else {
        set_last_error(87);
        return 0;
    };
    let w: Vec<u16> = s.encode_utf16().collect();
    if sz.is_null() {
        *pcch = w.len() as u32 + 1;
        return 1;
    }
    if (*pcch as usize) <= w.len() {
        *pcch = w.len() as u32 + 1;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), sz, w.len());
    *sz.add(w.len()) = 0;
    *pcch = w.len() as u32;
    1
}

pub unsafe extern "win64" fn CryptBinaryToStringA(
    bin: *const u8,
    cb: u32,
    flags: u32,
    sz: *mut u8,
    pcch: *mut u32,
) -> i32 {
    let data = if bin.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(bin, cb as usize)
    };
    let (Some(s), false) = (binary_to_string(data, flags), pcch.is_null()) else {
        set_last_error(87);
        return 0;
    };
    let a = s.into_bytes();
    if sz.is_null() {
        *pcch = a.len() as u32 + 1;
        return 1;
    }
    if (*pcch as usize) <= a.len() {
        *pcch = a.len() as u32 + 1;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), sz, a.len());
    *sz.add(a.len()) = 0;
    *pcch = a.len() as u32;
    1
}

// ---------- Protection de données (non fournie) ----------

pub unsafe extern "win64" fn CryptProtectData(
    _in: usize,
    _desc: *const u16,
    _entropy: usize,
    _reserved: usize,
    _prompt: usize,
    _flags: u32,
    _out: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn CryptUnprotectData(
    _in: usize,
    _desc: *mut *mut u16,
    _entropy: usize,
    _reserved: usize,
    _prompt: usize,
    _flags: u32,
    _out: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn PFXImportCertStore(
    _pfx: usize,
    _pass: *const u16,
    _flags: u32,
) -> usize {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CertOpenSystemStoreW" => fn CertOpenSystemStoreW,
        "CertOpenSystemStoreA" => fn CertOpenSystemStoreA,
        "CertOpenStore" => fn CertOpenStore,
        "CertDuplicateStore" => fn CertDuplicateStore,
        "CertCloseStore" => fn CertCloseStore,
        "CertAddStoreToCollection" => fn CertAddStoreToCollection,
        "CertEnumCertificatesInStore" => fn CertEnumCertificatesInStore,
        "CertFreeCertificateContext" => fn CertFreeCertificateContext,
        "CertDuplicateCertificateContext" => fn CertDuplicateCertificateContext,
        "CertCreateCertificateContext" => fn CertCreateCertificateContext,
        "CertAddEncodedCertificateToStore" => fn CertAddEncodedCertificateToStore,
        "CertAddCertificateContextToStore" => fn CertAddCertificateContextToStore,
        "CertCompareCertificate" => fn CertCompareCertificate,
        "CertFindCertificateInStore" => fn CertFindCertificateInStore,
        "CertGetCertificateContextProperty" => fn CertGetCertificateContextProperty,
        "CertSetCertificateContextProperty" => fn CertSetCertificateContextProperty,
        "CertFindExtension" => fn CertFindExtension,
        "CertVerifyTimeValidity" => fn CertVerifyTimeValidity,
        "CertGetCertificateChain" => fn CertGetCertificateChain,
        "CertFreeCertificateChain" => fn CertFreeCertificateChain,
        "CertDuplicateCertificateChain" => fn CertDuplicateCertificateChain,
        "CertVerifyCertificateChainPolicy" => fn CertVerifyCertificateChainPolicy,
        "CertCreateCertificateChainEngine" => fn CertCreateCertificateChainEngine,
        "CertFreeCertificateChainEngine" => fn CertFreeCertificateChainEngine,
        "CertGetNameStringW" => fn CertGetNameStringW,
        "CertGetNameStringA" => fn CertGetNameStringA,
        "CertNameToStrW" => fn CertNameToStrW,
        "CertNameToStrA" => fn CertNameToStrA,
        "CryptProtectData" => fn CryptProtectData,
        "CryptUnprotectData" => fn CryptUnprotectData,
        "CryptStringToBinaryW" => fn CryptStringToBinaryW,
        "CryptStringToBinaryA" => fn CryptStringToBinaryA,
        "CryptBinaryToStringW" => fn CryptBinaryToStringW,
        "CryptBinaryToStringA" => fn CryptBinaryToStringA,
        "PFXImportCertStore" => fn PFXImportCertStore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oid_et_dates() {
        assert_eq!(
            oid_str(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b]),
            "1.2.840.113549.1.1.11"
        );
        assert_eq!(oid_str(&[0x55, 0x04, 0x03]), "2.5.4.3");
        let t = Tlv {
            tag: 0x17,
            body: b"700101000000Z",
            all: &[],
        };
        assert_eq!(filetime_of(&t), 116_444_736_000_000_000);
        let g = Tlv {
            tag: 0x18,
            body: b"20000301000000Z",
            all: &[],
        };
        // 1er mars 2000 = 951868800 s depuis 1970.
        assert_eq!(filetime_of(&g), (951_868_800 + 11_644_473_600) * 10_000_000);
    }

    #[test]
    fn base64_et_hex() {
        let s = binary_to_string(b"Winlux!", CRYPT_STRING_BASE64 | CRYPT_STRING_NOCRLF).unwrap();
        assert_eq!(s, "V2lubHV4IQ==");
        assert_eq!(
            string_to_binary(&s, CRYPT_STRING_BASE64).unwrap().0,
            b"Winlux!"
        );
        assert_eq!(
            string_to_binary("de ad BE EF", CRYPT_STRING_HEX).unwrap().0,
            [0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn magasin_racine_et_chaine() {
        let root = root_store();
        let certs = store_certs(root);
        if certs.is_empty() || hostssl::ssl().is_none() {
            return;
        }
        // Chaque autorité du système se décode et se vérifie seule.
        let c = certs[0];
        let k = unsafe { cert(c) }.unwrap();
        assert!(k.info.subject.cb > 0 && k.info.spki_key.cb > 0);
        let name = unsafe { name_string(c, 4, 0, 0) };
        assert!(!name.is_empty());
        let mut chain = 0usize;
        assert_eq!(
            unsafe { CertGetCertificateChain(0, c, 0, 0, 0, 0, 0, &mut chain) },
            1
        );
        let ch = unsafe { &*(chain as *const ChainContext) };
        assert_eq!(
            ch.trust.error & !CERT_TRUST_IS_NOT_TIME_VALID,
            0,
            "racine du système de confiance"
        );
        let mut st = PolicyStatus {
            cb: 24,
            error: 1,
            chain_index: 0,
            element_index: 0,
            extra: 0,
        };
        let para = PolicyPara {
            cb: 16,
            flags: 0x7,
            extra: 0,
        };
        unsafe {
            CertVerifyCertificateChainPolicy(
                1,
                chain,
                &para as *const _ as usize,
                &mut st as *mut _ as usize,
            )
        };
        assert_eq!(st.error, 0);
        unsafe { CertFreeCertificateChain(chain) };
        // Recherche par condensat SHA-1.
        let sha1 = hash_prop(&k.der, 3).unwrap();
        let b = Blob {
            cb: 20,
            pb: sha1.as_ptr(),
        };
        let found =
            unsafe { CertFindCertificateInStore(root, 1, 0, 0x10000, &b as *const _ as usize, 0) };
        assert_eq!(found, c);
        CertFreeCertificateContext(found);
    }
}
