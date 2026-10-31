//! Stubs pour crypt32.dll — certificats et protection de données.
//! Les apps Electron (Claude, ChatGPT) interrogent le magasin système ;
//! on renvoie « vide » pour qu'elles retombent sur leur propre TLS.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn CertOpenSystemStoreW(_hProv: usize, _proto: *const u16) -> usize {
    0x7000_0001 // faux handle
}
pub unsafe extern "win64" fn CertOpenStore(
    _provider: usize, _enc: u32, _prov: usize, _flags: u32, _para: usize,
) -> usize {
    0x7000_0001
}
pub extern "win64" fn CertCloseStore(_h: usize, _flags: u32) -> i32 { 1 }
pub unsafe extern "win64" fn CertEnumCertificatesInStore(_h: usize, _prev: usize) -> usize { 0 }
pub extern "win64" fn CertFreeCertificateContext(_ctx: usize) -> i32 { 1 }
pub extern "win64" fn CertDuplicateCertificateContext(ctx: usize) -> usize { ctx }
pub unsafe extern "win64" fn CertFindCertificateInStore(
    _h: usize, _enc: u32, _flags: u32, _type: u32, _para: usize, _prev: usize,
) -> usize { 0 }
pub unsafe extern "win64" fn CertGetCertificateChain(
    _engine: usize, _ctx: usize, _time: usize, _store: usize,
    _para: usize, _flags: u32, _reserved: usize, _chain: *mut usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn CertVerifyCertificateChainPolicy(
    _policy: usize, _chain: usize, _para: usize, _status: usize,
) -> i32 { 1 }
pub unsafe extern "win64" fn CertGetNameStringW(
    _ctx: usize, _type: u32, _flags: u32, _para: usize, _buf: *mut u16, _size: u32,
) -> u32 { 0 }
pub unsafe extern "win64" fn CertGetNameStringA(
    _ctx: usize, _type: u32, _flags: u32, _para: usize, _buf: *mut u8, _size: u32,
) -> u32 { 0 }
pub unsafe extern "win64" fn CryptProtectData(
    _in: usize, _desc: *const u16, _entropy: usize, _reserved: usize,
    _prompt: usize, _flags: u32, _out: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn CryptUnprotectData(
    _in: usize, _desc: *mut *mut u16, _entropy: usize, _reserved: usize,
    _prompt: usize, _flags: u32, _out: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn CryptStringToBinaryW(
    _sz: *const u16, _cch: u32, _flags: u32, _bin: *mut u8,
    _pcb: *mut u32, _skip: *mut u32, _pflags: *mut u32,
) -> i32 { 0 }
pub unsafe extern "win64" fn CryptBinaryToStringW(
    _bin: *const u8, _cb: u32, _flags: u32, _sz: *mut u16, _pcch: *mut u32,
) -> i32 { 0 }
pub unsafe extern "win64" fn PFXImportCertStore(
    _pfx: usize, _pass: *const u16, _flags: u32,
) -> usize { 0 }

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CertOpenSystemStoreW" => fn CertOpenSystemStoreW,
        "CertOpenStore" => fn CertOpenStore,
        "CertCloseStore" => fn CertCloseStore,
        "CertEnumCertificatesInStore" => fn CertEnumCertificatesInStore,
        "CertFreeCertificateContext" => fn CertFreeCertificateContext,
        "CertDuplicateCertificateContext" => fn CertDuplicateCertificateContext,
        "CertFindCertificateInStore" => fn CertFindCertificateInStore,
        "CertGetCertificateChain" => fn CertGetCertificateChain,
        "CertVerifyCertificateChainPolicy" => fn CertVerifyCertificateChainPolicy,
        "CertGetNameStringW" => fn CertGetNameStringW,
        "CertGetNameStringA" => fn CertGetNameStringA,
        "CryptProtectData" => fn CryptProtectData,
        "CryptUnprotectData" => fn CryptUnprotectData,
        "CryptStringToBinaryW" => fn CryptStringToBinaryW,
        "CryptBinaryToStringW" => fn CryptBinaryToStringW,
        "PFXImportCertStore" => fn PFXImportCertStore,
    }
}
