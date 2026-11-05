//! ncrypt.dll : le fournisseur de stockage de clés de CNG (fonctions
//! `NCrypt*`, d'après Microsoft Learn).
//!
//! Winlux n'a pas de clés CNG persistantes : les clés privées des
//! certificats passent par CryptoAPI (`CryptAcquireCertificatePrivateKey`
//! rend un HCRYPTPROV quand CNG est seulement autorisé). Ce fournisseur
//! s'ouvre, s'énumère et se décrit, pour que GnuTLS (FileZilla) et les
//! autres programmes qui le lient puissent démarrer ; les opérations sur
//! clé répondent « non pris en charge ».

use super::{lock, wstr};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const S_OK: u32 = 0;
const NTE_BAD_FLAGS: u32 = 0x8009_0009;
const NTE_BAD_KEYSET: u32 = 0x8009_0016;
const NTE_INVALID_HANDLE: u32 = 0x8009_0026;
const NTE_INVALID_PARAMETER: u32 = 0x8009_0027;
const NTE_BUFFER_TOO_SMALL: u32 = 0x8009_0028;
const NTE_NOT_SUPPORTED: u32 = 0x8009_0029;
const NTE_NO_MORE_ITEMS: u32 = 0x8009_002A;
const NTE_PROV_DLL_NOT_FOUND: u32 = 0x8009_001E;

const KSP_NAME: &str = "Microsoft Software Key Storage Provider";
const KSP_COMMENT: &str = "Fournisseur de clés logiciel de Winlux";
#[cfg(test)]
const SMART_CARD_KSP: &str = "Microsoft Smart Card Key Storage Provider";
#[cfg(test)]
const PLATFORM_KSP: &str = "Microsoft Platform Crypto Provider";

#[derive(Clone)]
enum Obj {
    Provider(String),
}

fn objects() -> &'static Mutex<(usize, HashMap<usize, Obj>)> {
    static O: OnceLock<Mutex<(usize, HashMap<usize, Obj>)>> = OnceLock::new();
    O.get_or_init(|| Mutex::new((0x6e63_0000, HashMap::new())))
}

fn add(o: Obj) -> usize {
    let mut t = lock(objects());
    t.0 += 0x10;
    let h = t.0;
    t.1.insert(h, o);
    h
}

fn get(h: usize) -> Option<Obj> {
    lock(objects()).1.get(&h).cloned()
}

/// Tampons rendus par NCryptEnum* : libérés par NCryptFreeBuffer.
fn buffers() -> &'static Mutex<HashMap<usize, Box<[u8]>>> {
    static B: OnceLock<Mutex<HashMap<usize, Box<[u8]>>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

fn keep(b: Box<[u8]>) -> usize {
    let p = b.as_ptr() as usize;
    lock(buffers()).insert(p, b);
    p
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe extern "win64" fn open_storage_provider(
    out: *mut usize,
    name: *const u16,
    flags: u32,
) -> u32 {
    if out.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    if flags & !0x40 != 0 {
        // Seul NCRYPT_SILENT_FLAG est admis.
        return NTE_BAD_FLAGS;
    }
    let n = if name.is_null() {
        KSP_NAME.to_string()
    } else {
        wstr(name)
    };
    if !n.eq_ignore_ascii_case(KSP_NAME) {
        // Carte à puce et TPM : absents.
        *out = 0;
        return NTE_PROV_DLL_NOT_FOUND;
    }
    *out = add(Obj::Provider(KSP_NAME.into()));
    S_OK
}

/// NCryptEnumStorageProviders : un seul fournisseur, le logiciel.
unsafe extern "win64" fn enum_storage_providers(
    count: *mut u32,
    list: *mut usize,
    _flags: u32,
) -> u32 {
    if count.is_null() || list.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    // NCryptProviderName { LPWSTR pszName; LPWSTR pszComment; } puis les chaînes.
    let (n, c) = (wide(KSP_NAME), wide(KSP_COMMENT));
    let size = 16 + (n.len() + c.len()) * 2;
    let mut b = vec![0u8; size].into_boxed_slice();
    let base = b.as_mut_ptr();
    let pn = base.add(16) as *mut u16;
    let pc = pn.add(n.len());
    std::ptr::copy_nonoverlapping(n.as_ptr(), pn, n.len());
    std::ptr::copy_nonoverlapping(c.as_ptr(), pc, c.len());
    (base as *mut usize).write_unaligned(pn as usize);
    (base.add(8) as *mut usize).write_unaligned(pc as usize);
    *list = keep(b);
    *count = 1;
    S_OK
}

unsafe extern "win64" fn free_buffer(p: usize) -> u32 {
    match lock(buffers()).remove(&p) {
        Some(_) => S_OK,
        None if p == 0 => S_OK,
        None => NTE_INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn free_object(h: usize) -> u32 {
    match lock(objects()).1.remove(&h) {
        Some(_) => S_OK,
        None => NTE_INVALID_HANDLE,
    }
}

unsafe extern "win64" fn is_key_handle(_h: usize) -> i32 {
    0
}

/// Algorithmes que le fournisseur annoncerait : seuls ceux qu'OpenSSL de
/// l'hôte sait faire par CryptoAPI.
fn supported(alg: &str) -> bool {
    ["RSA", "ECDSA_P256", "ECDSA_P384", "ECDH_P256", "ECDH_P384"]
        .iter()
        .any(|a| a.eq_ignore_ascii_case(alg))
}

unsafe extern "win64" fn is_alg_supported(prov: usize, alg: *const u16, _flags: u32) -> u32 {
    if !matches!(get(prov), Some(Obj::Provider(_))) {
        return NTE_INVALID_HANDLE;
    }
    if alg.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    if supported(&wstr(alg)) {
        S_OK
    } else {
        NTE_NOT_SUPPORTED
    }
}

unsafe extern "win64" fn enum_keys(
    prov: usize,
    _scope: *const u16,
    out: *mut usize,
    _state: *mut usize,
    _flags: u32,
) -> u32 {
    if !matches!(get(prov), Some(Obj::Provider(_))) {
        return NTE_INVALID_HANDLE;
    }
    if !out.is_null() {
        *out = 0;
    }
    NTE_NO_MORE_ITEMS
}

unsafe extern "win64" fn enum_algorithms(
    prov: usize,
    _ops: u32,
    count: *mut u32,
    list: *mut usize,
    _flags: u32,
) -> u32 {
    if !matches!(get(prov), Some(Obj::Provider(_))) {
        return NTE_INVALID_HANDLE;
    }
    if count.is_null() || list.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    *count = 0;
    *list = 0;
    S_OK
}

unsafe extern "win64" fn open_key(
    prov: usize,
    key: *mut usize,
    _name: *const u16,
    _spec: u32,
    _flags: u32,
) -> u32 {
    if !matches!(get(prov), Some(Obj::Provider(_))) {
        return NTE_INVALID_HANDLE;
    }
    if !key.is_null() {
        *key = 0;
    }
    NTE_BAD_KEYSET
}

/// Écrit une propriété : taille d'abord, puis les octets si le tampon suffit.
unsafe fn property(bytes: &[u8], out: *mut u8, cb: u32, result: *mut u32) -> u32 {
    if result.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    *result = bytes.len() as u32;
    if out.is_null() {
        return S_OK;
    }
    if (cb as usize) < bytes.len() {
        return NTE_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    S_OK
}

unsafe extern "win64" fn get_property(
    h: usize,
    name: *const u16,
    out: *mut u8,
    cb: u32,
    result: *mut u32,
    _flags: u32,
) -> u32 {
    let Some(Obj::Provider(prov)) = get(h) else {
        return NTE_INVALID_HANDLE;
    };
    if name.is_null() {
        return NTE_INVALID_PARAMETER;
    }
    let bytes: Vec<u8> = match wstr(name).as_str() {
        "Name" => wide(&prov).iter().flat_map(|u| u.to_le_bytes()).collect(),
        // NCRYPT_IMPL_SOFTWARE_FLAG.
        "Impl Type" => 2u32.to_le_bytes().to_vec(),
        "Version" => 0x0001_0000u32.to_le_bytes().to_vec(),
        "Max Name Length" => 260u32.to_le_bytes().to_vec(),
        _ => return NTE_NOT_SUPPORTED,
    };
    property(&bytes, out, cb, result)
}

unsafe extern "win64" fn not_supported() -> u32 {
    NTE_NOT_SUPPORTED
}

/// NCryptTranslateHandle : les clés CryptoAPI de Winlux ne passent pas à CNG.
unsafe extern "win64" fn translate_handle(
    prov: *mut usize,
    key: *mut usize,
    _lp: usize,
    _lk: usize,
    _spec: u32,
    _flags: u32,
) -> u32 {
    if !prov.is_null() {
        *prov = 0;
    }
    if !key.is_null() {
        *key = 0;
    }
    NTE_NOT_SUPPORTED
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "NCryptOpenStorageProvider" => fn open_storage_provider,
        "NCryptEnumStorageProviders" => fn enum_storage_providers,
        "NCryptFreeBuffer" => fn free_buffer,
        "NCryptFreeObject" => fn free_object,
        "NCryptIsKeyHandle" => fn is_key_handle,
        "NCryptIsAlgSupported" => fn is_alg_supported,
        "NCryptEnumKeys" => fn enum_keys,
        "NCryptEnumAlgorithms" => fn enum_algorithms,
        "NCryptOpenKey" => fn open_key,
        "NCryptGetProperty" => fn get_property,
        "NCryptSetProperty" => fn not_supported,
        "NCryptCreatePersistedKey" => fn not_supported,
        "NCryptFinalizeKey" => fn not_supported,
        "NCryptDeleteKey" => fn not_supported,
        "NCryptImportKey" => fn not_supported,
        "NCryptExportKey" => fn not_supported,
        "NCryptSignHash" => fn not_supported,
        "NCryptVerifySignature" => fn not_supported,
        "NCryptEncrypt" => fn not_supported,
        "NCryptDecrypt" => fn not_supported,
        "NCryptSecretAgreement" => fn not_supported,
        "NCryptDeriveKey" => fn not_supported,
        "NCryptKeyDerivation" => fn not_supported,
        "NCryptCreateClaim" => fn not_supported,
        "NCryptVerifyClaim" => fn not_supported,
        "NCryptNotifyChangeKey" => fn not_supported,
        "NCryptTranslateHandle" => fn translate_handle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fournisseur_logiciel() {
        unsafe {
            let mut p = 0usize;
            assert_eq!(open_storage_provider(&mut p, std::ptr::null(), 0), S_OK);
            let name = wide("Name");
            let mut n = 0u32;
            assert_eq!(
                get_property(p, name.as_ptr(), std::ptr::null_mut(), 0, &mut n, 0),
                S_OK
            );
            let mut buf = vec![0u16; n as usize / 2];
            assert_eq!(
                get_property(p, name.as_ptr(), buf.as_mut_ptr() as *mut u8, n, &mut n, 0),
                S_OK
            );
            assert_eq!(wstr(buf.as_ptr()), KSP_NAME);
            let rsa = wide("RSA");
            assert_eq!(is_alg_supported(p, rsa.as_ptr(), 0), S_OK);
            let mut k = 1usize;
            assert_eq!(open_key(p, &mut k, rsa.as_ptr(), 0, 0), NTE_BAD_KEYSET);
            assert_eq!(k, 0);
            assert_eq!(free_object(p), S_OK);
            assert_eq!(free_object(p), NTE_INVALID_HANDLE);
            let card = wide(SMART_CARD_KSP);
            assert_eq!(
                open_storage_provider(&mut p, card.as_ptr(), 0),
                NTE_PROV_DLL_NOT_FOUND
            );
            let tpm = wide(PLATFORM_KSP);
            assert_eq!(
                open_storage_provider(&mut p, tpm.as_ptr(), 0),
                NTE_PROV_DLL_NOT_FOUND
            );
        }
    }

    #[test]
    fn enumeration_des_fournisseurs() {
        unsafe {
            let (mut n, mut list) = (0u32, 0usize);
            assert_eq!(enum_storage_providers(&mut n, &mut list, 0), S_OK);
            assert_eq!(n, 1);
            let name = (list as *const usize).read_unaligned() as *const u16;
            assert_eq!(wstr(name), KSP_NAME);
            assert_eq!(free_buffer(list), S_OK);
        }
    }
}
