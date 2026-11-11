//! crypt32 : encodage DER (CryptEncodeObjectEx), noms X.500 depuis une
//! chaîne (CertStrToNameA/W), condensat d'un objet encodé
//! (CryptHashCertificate2) et certificat auto-signé
//! (CertCreateSelfSignCertificate). Exports relevés par le banc sur Steam et
//! pygame32 (#1585).
//!
//! Sources : documentation Microsoft Learn de ces fonctions et des
//! structures CERT_NAME_INFO, CERT_RDN_ATTR, CERT_ALT_NAME_ENTRY,
//! CRYPT_BIT_BLOB, CERT_BASIC_CONSTRAINTS2_INFO, CTL_USAGE, CERT_EXTENSIONS,
//! CRYPT_ALGORITHM_IDENTIFIER, CRYPT_ENCODE_PARA, des constantes
//! X509_*/CERT_NAME_STR_* (wincrypt.h) ; RFC 5280 (X.509) et X.690 (DER).
//! Les structures sont lues avec la taille de pointeur du programme (x64 ou
//! x86). La clé d'un certificat auto-signé est une RSA 2048 neuve générée
//! par l'OpenSSL de l'hôte (le fournisseur passé n'est pas consulté) ; sans
//! OpenSSL, la fonction échoue proprement.

#![allow(non_snake_case)]

use super::crypt32::{der_wrap, short_name};
use crate::process::set_last_error;
use crate::wow32::{pointer_size, read_ptr};

const ERROR_MORE_DATA: u32 = 234;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const E_INVALIDARG: u32 = 0x8007_0057;
const NTE_BAD_ALGID: u32 = 0x8009_0008;
const NTE_FAIL: u32 = 0x8009_0020;
const CRYPT_E_INVALID_X500_STRING: u32 = 0x8009_2023;
const CRYPT_E_NOT_FOUND: u32 = 0x8009_2004;
const CRYPT_E_INVALID_NUMERIC_STRING: u32 = 0x8009_2020;

const X509_NAME: usize = 7;
const X509_ALTERNATE_NAME: usize = 12;
const X509_KEY_USAGE: usize = 14;
const X509_BASIC_CONSTRAINTS2: usize = 15;
const X509_OCTET_STRING: usize = 25;
const X509_INTEGER: usize = 27;
const X509_MULTI_BYTE_INTEGER: usize = 28;
const X509_ENHANCED_KEY_USAGE: usize = 36;
const CRYPT_ENCODE_ALLOC_FLAG: u32 = 0x8000;

const CERT_NAME_STR_SEMICOLON_FLAG: u32 = 0x4000_0000;
const CERT_NAME_STR_NO_PLUS_FLAG: u32 = 0x2000_0000;
const CERT_NAME_STR_COMMA_FLAG: u32 = 0x0400_0000;
const CERT_NAME_STR_CRLF_FLAG: u32 = 0x0800_0000;
const CERT_NAME_STR_REVERSE_FLAG: u32 = 0x0200_0000;
const CERT_NAME_STR_FORCE_UTF8_DIR_STR_FLAG: u32 = 0x0008_0000;

const CERT_CREATE_SELFSIGN_NO_SIGN: u32 = 1;

// ---------- Briques DER ----------

/// Identifiant d'objet en notation pointée vers son encodage DER complet.
pub(crate) fn oid_der(dotted: &str) -> Option<Vec<u8>> {
    let arcs: Vec<u64> = dotted
        .split('.')
        .map(|a| a.parse().ok())
        .collect::<Option<_>>()?;
    if arcs.len() < 2 || arcs[0] > 2 || (arcs[0] < 2 && arcs[1] > 39) {
        return None;
    }
    let mut body = Vec::new();
    let first = arcs[0].checked_mul(40)?.checked_add(arcs[1])?;
    for &a in std::iter::once(&first).chain(&arcs[2..]) {
        let mut groups = vec![(a & 0x7f) as u8];
        let mut v = a >> 7;
        while v != 0 {
            groups.push(0x80 | (v & 0x7f) as u8);
            v >>= 7;
        }
        groups.reverse();
        body.extend(groups);
    }
    Some(der_wrap(0x06, &body))
}

/// INTEGER d'un entier signé grand-boutiste, ramené à sa forme minimale.
fn int_signed_be(b: &[u8]) -> Vec<u8> {
    let mut s = if b.is_empty() { &[0u8][..] } else { b };
    while s.len() > 1 && ((s[0] == 0 && s[1] & 0x80 == 0) || (s[0] == 0xff && s[1] & 0x80 != 0)) {
        s = &s[1..];
    }
    der_wrap(0x02, s)
}

/// INTEGER d'un entier non signé grand-boutiste.
fn int_unsigned_be(b: &[u8]) -> Vec<u8> {
    let z = b.iter().take_while(|&&x| x == 0).count();
    let mut v = vec![0u8];
    v.extend_from_slice(&b[z..]);
    int_signed_be(&v)
}

fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
    der_wrap(0x30, &parts.concat())
}

// ---------- Lecture des structures du programme ----------

/// (cbData, pbData) d'un CRYPT_*_BLOB situé en `at`.
unsafe fn read_blob<'a>(at: usize) -> &'a [u8] {
    let cb = *(at as *const u32) as usize;
    let pb = read_ptr(at + pointer_size());
    if cb == 0 || pb == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(pb as *const u8, cb)
    }
}

unsafe fn read_u32(at: usize) -> u32 {
    std::ptr::read_unaligned(at as *const u32)
}

unsafe fn cstr_at(p: usize) -> Option<String> {
    (p != 0).then(|| super::cstr(p as *const u8))
}

unsafe fn wstr_at(p: usize) -> Option<String> {
    (p != 0).then(|| super::wstr(p as *const u16))
}

// ---------- Noms X.500 ----------

/// Valeur d'un attribut, au type ASN.1 choisi comme CertStrToName : chaîne
/// IA5 pour E et DC, PrintableString si possible, sinon BMPString (ou
/// UTF8String avec CERT_NAME_STR_FORCE_UTF8_DIR_STR_FLAG).
fn rdn_value(oid: &str, value: &str, utf8: bool) -> Vec<u8> {
    let printable = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " '()+,-./:=?".contains(c));
    if (oid == "1.2.840.113549.1.9.1" || oid == "0.9.2342.19200300.100.1.25") && value.is_ascii() {
        der_wrap(0x16, value.as_bytes())
    } else if printable {
        der_wrap(0x13, value.as_bytes())
    } else if utf8 {
        der_wrap(0x0c, value.as_bytes())
    } else {
        let be: Vec<u8> = value.encode_utf16().flat_map(u16::to_be_bytes).collect();
        der_wrap(0x1e, &be)
    }
}

/// OID d'une clé de CertStrToName : nom court (CN, O…), « OID.1.2.3 » ou
/// « 1.2.3 ».
fn key_oid(key: &str) -> Option<String> {
    let k = key.trim();
    let dotted = k
        .strip_prefix("OID.")
        .or_else(|| k.strip_prefix("oid."))
        .unwrap_or(k);
    if dotted.starts_with(|c: char| c.is_ascii_digit()) {
        return oid_der(dotted).map(|_| dotted.to_string());
    }
    const OIDS: &[&str] = &[
        "2.5.4.3",
        "2.5.4.6",
        "2.5.4.7",
        "2.5.4.8",
        "2.5.4.9",
        "2.5.4.10",
        "2.5.4.11",
        "2.5.4.12",
        "2.5.4.42",
        "2.5.4.4",
        "2.5.4.5",
        "1.2.840.113549.1.9.1",
        "0.9.2342.19200300.100.1.25",
    ];
    let upper = k.to_ascii_uppercase();
    let alias = match upper.as_str() {
        "EMAIL" | "EMAILADDRESS" => "E",
        "ST" => "S",
        "TITLE" => "T",
        "GIVENNAME" => "G",
        "SURNAME" => "SN",
        _ => upper.as_str(),
    };
    OIDS.iter()
        .find(|o| short_name(o) == Some(alias))
        .map(|o| o.to_string())
}

/// Analyse une chaîne X.500 (« CN=Nom, O=Société ») en Name DER. En cas
/// d'erreur, rend la position (en caractères) où l'analyse a échoué.
pub(crate) fn str_to_name(text: &str, flags: u32) -> Result<Vec<u8>, usize> {
    let seps: &[char] = if flags & CERT_NAME_STR_SEMICOLON_FLAG != 0 {
        &[';']
    } else if flags & CERT_NAME_STR_COMMA_FLAG != 0 {
        &[',']
    } else if flags & CERT_NAME_STR_CRLF_FLAG != 0 {
        &['\r', '\n']
    } else {
        &[',', ';']
    };
    let plus = flags & CERT_NAME_STR_NO_PLUS_FLAG == 0;
    let utf8 = flags & CERT_NAME_STR_FORCE_UTF8_DIR_STR_FLAG != 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    // Ensembles (RDN), chacun fait d'attributs (OID, valeur).
    let mut rdns: Vec<Vec<(String, String)>> = vec![Vec::new()];
    loop {
        while i < chars.len() && (chars[i].is_whitespace() || seps.contains(&chars[i])) {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let key_start = i;
        while i < chars.len() && chars[i] != '=' {
            if seps.contains(&chars[i]) || (plus && chars[i] == '+') {
                return Err(key_start);
            }
            i += 1;
        }
        if i >= chars.len() {
            return Err(key_start);
        }
        let key: String = chars[key_start..i].iter().collect();
        let oid = key_oid(&key).ok_or(key_start)?;
        i += 1;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        let mut value = String::new();
        if chars.get(i) == Some(&'"') {
            let open = i;
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(open),
                    Some('"') if chars.get(i + 1) == Some(&'"') => {
                        value.push('"');
                        i += 2;
                    }
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some(&c) => {
                        value.push(c);
                        i += 1;
                    }
                }
            }
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
        } else {
            let start = i;
            while i < chars.len() && !seps.contains(&chars[i]) && !(plus && chars[i] == '+') {
                i += 1;
            }
            value = chars[start..i]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_string();
        }
        rdns.last_mut().expect("au moins un RDN").push((oid, value));
        match chars.get(i) {
            None => break,
            Some('+') if plus => {
                i += 1;
            }
            Some(c) if seps.contains(c) => {
                i += 1;
                rdns.push(Vec::new());
            }
            Some(_) => return Err(i),
        }
    }
    rdns.retain(|r| !r.is_empty());
    if flags & CERT_NAME_STR_REVERSE_FLAG != 0 {
        rdns.reverse();
    }
    let sets: Vec<Vec<u8>> = rdns
        .iter()
        .map(|r| {
            let atvs: Vec<u8> = r
                .iter()
                .map(|(oid, v)| seq(&[oid_der(oid).expect("OID vérifié"), rdn_value(oid, v, utf8)]))
                .collect::<Vec<_>>()
                .concat();
            der_wrap(0x31, &atvs)
        })
        .collect();
    Ok(seq(&sets))
}

/// Copie `data` vers (out, len) selon la convention de taille de crypt32.
unsafe fn put(data: &[u8], out: *mut u8, len: *mut u32) -> i32 {
    if len.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let avail = *len as usize;
    *len = data.len() as u32;
    if out.is_null() {
        return 1;
    }
    if avail < data.len() {
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), out, data.len());
    1
}

unsafe fn str_to_name_common(
    text: Option<String>,
    flags: u32,
    out: *mut u8,
    len: *mut u32,
    error_at: impl FnOnce(Option<usize>),
) -> i32 {
    let Some(text) = text else {
        error_at(None);
        set_last_error(E_INVALIDARG);
        return 0;
    };
    match str_to_name(&text, flags) {
        Ok(der) => {
            error_at(None);
            put(&der, out, len)
        }
        Err(pos) => {
            error_at(Some(pos));
            if !len.is_null() {
                *len = 0;
            }
            set_last_error(CRYPT_E_INVALID_X500_STRING);
            0
        }
    }
}

/// Position en octets d'un caractère dans une chaîne ANSI ou large.
fn byte_offset(text: &str, ch: usize, wide: bool) -> usize {
    text.chars()
        .take(ch)
        .map(|c| {
            if wide {
                c.len_utf16() * 2
            } else {
                c.len_utf8()
            }
        })
        .sum()
}

pub unsafe extern "win64" fn CertStrToNameA(
    _enc: u32,
    x500: *const u8,
    str_type: u32,
    _reserved: usize,
    out: *mut u8,
    len: *mut u32,
    error: *mut usize,
) -> i32 {
    let text = cstr_at(x500 as usize);
    let t = text.clone().unwrap_or_default();
    str_to_name_common(text, str_type, out, len, |pos| {
        if !error.is_null() {
            let at = pos.map_or(0, |p| x500 as usize + byte_offset(&t, p, false));
            crate::wow32::write_ptr(error as usize, at);
        }
    })
}

pub unsafe extern "win64" fn CertStrToNameW(
    _enc: u32,
    x500: *const u16,
    str_type: u32,
    _reserved: usize,
    out: *mut u8,
    len: *mut u32,
    error: *mut usize,
) -> i32 {
    let text = wstr_at(x500 as usize);
    let t = text.clone().unwrap_or_default();
    str_to_name_common(text, str_type, out, len, |pos| {
        if !error.is_null() {
            let at = pos.map_or(0, |p| x500 as usize + byte_offset(&t, p, true));
            crate::wow32::write_ptr(error as usize, at);
        }
    })
}

// ---------- CryptEncodeObjectEx ----------

/// CERT_NAME_INFO → Name DER.
unsafe fn encode_name(info: usize) -> Option<Vec<u8>> {
    let ps = pointer_size();
    let rdn_count = read_u32(info) as usize;
    let rdns = read_ptr(info + ps);
    let mut sets = Vec::new();
    for r in 0..rdn_count {
        let rdn = rdns + r * 2 * ps;
        let n = read_u32(rdn) as usize;
        let attrs = read_ptr(rdn + ps);
        let mut atvs = Vec::new();
        for a in 0..n {
            // CERT_RDN_ATTR : pszObjId, dwValueType, Value (blob).
            let at = attrs + a * 4 * ps;
            let oid = oid_der(&cstr_at(read_ptr(at))?)?;
            let ty = read_u32(at + ps);
            let raw = read_blob(at + 2 * ps);
            let wide = || -> String {
                let w: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                String::from_utf16_lossy(&w)
            };
            let value = match ty {
                2 => raw.to_vec(),
                3 => der_wrap(0x04, raw),
                4 => der_wrap(0x12, raw),
                5 => der_wrap(0x13, raw),
                6 => der_wrap(0x14, raw),
                7 => der_wrap(0x15, raw),
                8 => der_wrap(0x16, raw),
                9 => der_wrap(0x19, raw),
                10 => der_wrap(0x1a, raw),
                11 => der_wrap(0x1b, raw),
                12 => {
                    let be: Vec<u8> = raw
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|c| u32::from_le_bytes(*c).to_be_bytes())
                        .collect();
                    der_wrap(0x1c, &be)
                }
                13 => {
                    let be: Vec<u8> = raw
                        .chunks(2)
                        .flat_map(|c| c.iter().rev().copied())
                        .collect();
                    der_wrap(0x1e, &be)
                }
                14 => der_wrap(0x0c, wide().as_bytes()),
                _ => return None,
            };
            atvs.push(seq(&[oid, value]));
        }
        sets.push(der_wrap(0x31, &atvs.concat()));
    }
    Some(seq(&sets))
}

/// CERT_ALT_NAME_INFO → GeneralNames.
unsafe fn encode_alt_names(info: usize) -> Option<Vec<u8>> {
    let ps = pointer_size();
    let n = read_u32(info) as usize;
    let entries = read_ptr(info + ps);
    let mut names = Vec::new();
    for e in 0..n {
        // CERT_ALT_NAME_ENTRY : dwAltNameChoice puis l'union, alignée.
        let at = entries + e * 3 * ps;
        let u = at + ps;
        let ia5 = |tag: u8| -> Option<Vec<u8>> {
            let s = wstr_at(read_ptr(u))?;
            s.is_ascii().then(|| der_wrap(tag, s.as_bytes()))
        };
        names.push(match read_u32(at) {
            2 => ia5(0x81)?,
            3 => ia5(0x82)?,
            7 => ia5(0x86)?,
            5 => der_wrap(0xa4, read_blob(u)),
            8 => der_wrap(0x87, read_blob(u)),
            9 => {
                let oid = oid_der(&cstr_at(read_ptr(u))?)?;
                der_wrap(0x88, &oid[2..])
            }
            _ => return None,
        });
    }
    Some(seq(&names))
}

/// CRYPT_BIT_BLOB → BIT STRING, les zéros de queue retirés (KeyUsage).
unsafe fn encode_key_usage(info: usize) -> Vec<u8> {
    let mut b = read_blob(info).to_vec();
    while b.last() == Some(&0) {
        b.pop();
    }
    let unused = b.last().map_or(0, |&x| x.trailing_zeros() as u8);
    let mut body = vec![unused];
    body.extend(b);
    der_wrap(0x03, &body)
}

unsafe fn encode_basic_constraints2(info: usize) -> Vec<u8> {
    let mut parts = Vec::new();
    if read_u32(info) != 0 {
        parts.push(vec![0x01, 0x01, 0xff]);
    }
    if read_u32(info + 4) != 0 {
        parts.push(int_unsigned_be(&read_u32(info + 8).to_be_bytes()));
    }
    seq(&parts)
}

/// CTL_USAGE → ExtKeyUsageSyntax.
unsafe fn encode_eku(info: usize) -> Option<Vec<u8>> {
    let ps = pointer_size();
    let n = read_u32(info) as usize;
    let list = read_ptr(info + ps);
    let oids = (0..n)
        .map(|i| oid_der(&cstr_at(read_ptr(list + i * ps))?))
        .collect::<Option<Vec<_>>>()?;
    Some(seq(&oids))
}

/// Encodage DER d'une structure ; `Err` porte le code d'erreur.
pub(crate) unsafe fn encode_object(struct_type: usize, info: usize) -> Result<Vec<u8>, u32> {
    let kind = if struct_type < 0x1_0000 {
        struct_type
    } else {
        match super::cstr(struct_type as *const u8).as_str() {
            "2.5.29.17" | "2.5.29.18" | "2.5.29.7" | "2.5.29.8" => X509_ALTERNATE_NAME,
            "2.5.29.19" => X509_BASIC_CONSTRAINTS2,
            "2.5.29.15" => X509_KEY_USAGE,
            "2.5.29.37" => X509_ENHANCED_KEY_USAGE,
            "2.5.29.14" => X509_OCTET_STRING,
            _ => 0,
        }
    };
    if info == 0 {
        return Err(E_INVALIDARG);
    }
    let bad = CRYPT_E_INVALID_NUMERIC_STRING;
    match kind {
        X509_NAME => encode_name(info).ok_or(bad),
        X509_ALTERNATE_NAME => encode_alt_names(info).ok_or(bad),
        X509_KEY_USAGE => Ok(encode_key_usage(info)),
        X509_BASIC_CONSTRAINTS2 => Ok(encode_basic_constraints2(info)),
        X509_OCTET_STRING => Ok(der_wrap(0x04, read_blob(info))),
        X509_INTEGER => Ok(int_signed_be(&(read_u32(info) as i32).to_be_bytes())),
        X509_MULTI_BYTE_INTEGER => {
            let mut be = read_blob(info).to_vec();
            be.reverse();
            Ok(int_signed_be(&be))
        }
        X509_ENHANCED_KEY_USAGE => encode_eku(info).ok_or(bad),
        _ => Err(CRYPT_E_NOT_FOUND),
    }
}

/// CryptEncodeObjectEx : X509_NAME, X509_ALTERNATE_NAME, X509_KEY_USAGE,
/// X509_BASIC_CONSTRAINTS2, X509_OCTET_STRING, X509_INTEGER,
/// X509_MULTI_BYTE_INTEGER, X509_ENHANCED_KEY_USAGE et les OID d'extension
/// correspondants. Les autres structures répondent CRYPT_E_NOT_FOUND.
pub unsafe extern "win64" fn CryptEncodeObjectEx(
    _enc: u32,
    struct_type: usize,
    info: usize,
    flags: u32,
    para: usize,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let der = match encode_object(struct_type, info) {
        Ok(d) => d,
        Err(e) => {
            *len = 0;
            set_last_error(e);
            return 0;
        }
    };
    if flags & CRYPT_ENCODE_ALLOC_FLAG == 0 {
        return put(&der, out, len);
    }
    if out.is_null() {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    // CRYPT_ENCODE_PARA { cbSize, pfnAlloc, pfnFree } ; sinon LocalAlloc.
    let ps = pointer_size();
    let alloc = if para != 0 && read_u32(para) as usize >= 3 * ps {
        read_ptr(para + ps)
    } else {
        0
    };
    let p = if alloc != 0 {
        let f = if crate::wow32::active() {
            crate::wow32::callback64(alloc as u32)
        } else {
            alloc
        };
        let f: unsafe extern "win64" fn(usize) -> *mut u8 = std::mem::transmute(f);
        f(der.len())
    } else {
        super::kernel32_misc::local_alloc(0x40, der.len()) as *mut u8
    };
    if p.is_null() {
        set_last_error(ERROR_NOT_ENOUGH_MEMORY);
        return 0;
    }
    std::ptr::copy_nonoverlapping(der.as_ptr(), p, der.len());
    crate::wow32::write_ptr(out as usize, p as usize);
    *len = der.len() as u32;
    1
}

// ---------- CryptHashCertificate2 ----------

/// Nom OpenSSL d'un identifiant d'algorithme CNG (BCRYPT_*_ALGORITHM).
fn cng_digest(name: &str) -> Option<&'static std::ffi::CStr> {
    Some(match name.to_ascii_uppercase().as_str() {
        "SHA1" => c"SHA1",
        "SHA256" => c"SHA256",
        "SHA384" => c"SHA384",
        "SHA512" => c"SHA512",
        "MD5" => c"MD5",
        "MD4" => c"MD4",
        _ => return None,
    })
}

pub unsafe extern "win64" fn CryptHashCertificate2(
    alg: *const u16,
    _flags: u32,
    _reserved: usize,
    data: *const u8,
    data_len: u32,
    out: *mut u8,
    len: *mut u32,
) -> i32 {
    let Some(md) = wstr_at(alg as usize).and_then(|n| cng_digest(&n)) else {
        set_last_error(NTE_BAD_ALGID);
        return 0;
    };
    let input = if data.is_null() || data_len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(data, data_len as usize)
    };
    let Some(h) = super::hostssl::digest(md, input) else {
        set_last_error(NTE_BAD_ALGID);
        return 0;
    };
    put(&h, out, len)
}

// ---------- CertCreateSelfSignCertificate ----------

/// (OID de signature, ALG_ID du condensat).
fn signature_alg(oid: Option<&str>) -> Option<(&'static str, u32)> {
    Some(match oid {
        None | Some("1.2.840.113549.1.1.5") => ("1.2.840.113549.1.1.5", 0x8004),
        Some("1.2.840.113549.1.1.11") => ("1.2.840.113549.1.1.11", 0x800c),
        Some("1.2.840.113549.1.1.12") => ("1.2.840.113549.1.1.12", 0x800d),
        Some("1.2.840.113549.1.1.13") => ("1.2.840.113549.1.1.13", 0x800e),
        Some("1.2.840.113549.1.1.4") => ("1.2.840.113549.1.1.4", 0x8003),
        _ => return None,
    })
}

/// Date (année, mois, jour, heure, minute, seconde) en UTCTime ou
/// GeneralizedTime selon RFC 5280 §4.1.2.5.
fn x509_time(t: [u32; 6]) -> Vec<u8> {
    let [y, mo, d, h, mi, s] = t;
    if (1950..2050).contains(&y) {
        let txt = format!("{:02}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z", y % 100);
        der_wrap(0x17, txt.as_bytes())
    } else {
        let txt = format!("{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z");
        der_wrap(0x18, txt.as_bytes())
    }
}

unsafe fn read_systemtime(p: usize) -> Option<[u32; 6]> {
    if p == 0 {
        return None;
    }
    let w = |i: usize| u32::from(std::ptr::read_unaligned((p + 2 * i) as *const u16));
    Some([w(0), w(1), w(3), w(4), w(5), w(6)])
}

fn now_utc() -> [u32; 6] {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::intl::tz::civil_from_days(secs.div_euclid(86_400));
    let r = secs.rem_euclid(86_400) as u32;
    [y as u32, m, d, r / 3600, r / 60 % 60, r % 60]
}

/// Un an plus tard, le 29 février ramené au 28.
fn plus_one_year(t: [u32; 6]) -> [u32; 6] {
    let mut n = t;
    n[0] += 1;
    let leap = |y: u32| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    if n[1] == 2 && n[2] == 29 && !leap(n[0]) {
        n[2] = 28;
    }
    n
}

/// Extensions [3] d'un CERT_EXTENSIONS (pszObjId, fCritical, Value).
unsafe fn read_extensions(p: usize) -> Option<Vec<u8>> {
    if p == 0 {
        return Some(Vec::new());
    }
    let ps = pointer_size();
    let n = read_u32(p) as usize;
    let list = read_ptr(p + ps);
    let mut exts = Vec::new();
    for i in 0..n {
        let at = list + i * 4 * ps;
        let mut parts = vec![oid_der(&cstr_at(read_ptr(at))?)?];
        if read_u32(at + ps) != 0 {
            parts.push(vec![0x01, 0x01, 0xff]);
        }
        parts.push(der_wrap(0x04, read_blob(at + 2 * ps)));
        exts.push(seq(&parts));
    }
    Some(if exts.is_empty() {
        Vec::new()
    } else {
        der_wrap(0xa3, &seq(&exts))
    })
}

/// Certificat auto-signé DER et clé privée DER associée.
pub(crate) fn self_signed(
    name: &[u8],
    flags: u32,
    sig_oid: Option<&str>,
    start: [u32; 6],
    end: [u32; 6],
    extensions: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), u32> {
    let (sig_oid, algid) = signature_alg(sig_oid).ok_or(NTE_BAD_ALGID)?;
    let key = super::advapi32_rsa::SelfSignKey::new(2048).ok_or(NTE_FAIL)?;
    let (n, e) = key.public().ok_or(NTE_FAIL)?;
    let private = key.private_der().ok_or(NTE_FAIL)?;
    let mut serial = [0u8; 16];
    let got = unsafe { libc::getrandom(serial.as_mut_ptr().cast(), serial.len(), 0) };
    if got != serial.len() as isize {
        return Err(NTE_FAIL);
    }
    serial[0] &= 0x7f;
    serial[0] |= 0x40;
    let alg = seq(&[oid_der(sig_oid).expect("OID fixe"), vec![0x05, 0x00]]);
    let rsa_pub = seq(&[int_unsigned_be(&n), int_unsigned_be(&e)]);
    let mut bits = vec![0u8];
    bits.extend(rsa_pub);
    let spki = seq(&[
        seq(&[
            oid_der("1.2.840.113549.1.1.1").expect("OID fixe"),
            vec![0x05, 0x00],
        ]),
        der_wrap(0x03, &bits),
    ]);
    let tbs = seq(&[
        vec![0xa0, 0x03, 0x02, 0x01, 0x02],
        int_unsigned_be(&serial),
        alg.clone(),
        name.to_vec(),
        seq(&[x509_time(start), x509_time(end)]),
        name.to_vec(),
        spki,
        extensions.to_vec(),
    ]);
    let signature = if flags & CERT_CREATE_SELFSIGN_NO_SIGN != 0 {
        vec![0u8; n.len()]
    } else {
        key.sign(algid, &tbs).ok_or(NTE_FAIL)?
    };
    let mut sig_bits = vec![0u8];
    sig_bits.extend(signature);
    Ok((seq(&[tbs, alg, der_wrap(0x03, &sig_bits)]), private))
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn CertCreateSelfSignCertificate(
    _prov: usize,
    subject: usize,
    flags: u32,
    _key_prov_info: usize,
    sig_alg: usize,
    start: usize,
    end: usize,
    extensions: usize,
) -> usize {
    if subject == 0 {
        set_last_error(E_INVALIDARG);
        return 0;
    }
    let name = read_blob(subject).to_vec();
    if super::crypt32::tlv(&name).is_none_or(|(t, _)| t.tag != 0x30) {
        set_last_error(CRYPT_E_INVALID_X500_STRING);
        return 0;
    }
    let sig_oid = if sig_alg == 0 {
        None
    } else {
        cstr_at(read_ptr(sig_alg))
    };
    let start = read_systemtime(start).unwrap_or_else(now_utc);
    let end = read_systemtime(end).unwrap_or_else(|| plus_one_year(start));
    let Some(exts) = read_extensions(extensions) else {
        set_last_error(E_INVALIDARG);
        return 0;
    };
    match self_signed(&name, flags, sig_oid.as_deref(), start, end, &exts) {
        Ok((der, private)) => match super::crypt32::new_cert(&der, 0) {
            Some(c) => {
                super::crypt32::attach_private_key(c, private);
                c as usize
            }
            None => {
                set_last_error(NTE_FAIL);
                0
            }
        },
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CertStrToNameA" => fn CertStrToNameA,
        "CertStrToNameW" => fn CertStrToNameW,
        "CryptEncodeObjectEx" => fn CryptEncodeObjectEx,
        "CryptHashCertificate2" => fn CryptHashCertificate2,
        "CertCreateSelfSignCertificate" => fn CertCreateSelfSignCertificate,
    }
}

#[cfg(test)]
mod tests;
