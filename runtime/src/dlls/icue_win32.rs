//! Imports relevés sur iCUE 4.18 (#1606) : iphlpapi, ws2_32, shlwapi,
//! shell32, crypt32, user32, winspool, CRT et le mélangeur de winmm.
//!
//! Sources : documentation Microsoft Learn de chaque fonction et des
//! structures MIXERCAPSW, MIXERLINEW, MIXERLINECONTROLSW, MIXERCONTROLW,
//! MIXERCONTROLDETAILS (mmeapi.h, alignées sur 1 octet), WSABUF,
//! IEnumAssocHandlers ; ordinal shlwapi #487 = SHLoadIndirectString d'après
//! ShLwApi.Lib du SDK Windows public (noms et numéros seulement).
#![allow(non_snake_case)]

use crate::process::set_last_error;
use crate::wow32::{pointer_size, write_ptr};
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const STRSAFE_E_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
const ERROR_SUCCESS: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_NAME: u32 = 123;

// ============================================================
//  iphlpapi : conversions de LUID
// ============================================================

/// Index d'interface porté par un LUID de Winlux (bits 24 à 47, comme
/// ConvertInterfaceIndexToLuid les écrit).
fn luid_index(luid: u64) -> u32 {
    ((luid >> 24) & 0xFF_FFFF) as u32
}

unsafe extern "win64" fn ConvertInterfaceLuidToIndex(luid: *const u64, index: *mut u32) -> u32 {
    if luid.is_null() || index.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *index = luid_index(std::ptr::read_unaligned(luid));
    ERROR_SUCCESS
}

/// GUID d'interface : celui que GetAdaptersAddresses donne en AdapterName.
unsafe extern "win64" fn ConvertInterfaceLuidToGuid(luid: *const u64, guid: *mut u8) -> u32 {
    if luid.is_null() || guid.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let index = luid_index(std::ptr::read_unaligned(luid));
    let Some(a) = super::iphlpapi::adapters()
        .into_iter()
        .find(|a| a.index == index)
    else {
        return ERROR_FILE_NOT_FOUND;
    };
    match parse_guid(&super::iphlpapi::adapter_guid(&a)) {
        Some(g) => {
            std::ptr::copy_nonoverlapping(g.as_ptr(), guid, 16);
            ERROR_SUCCESS
        }
        None => ERROR_FILE_NOT_FOUND,
    }
}

/// « {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX} » vers les 16 octets d'un GUID.
fn parse_guid(s: &str) -> Option<[u8; 16]> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok();
    let mut g = [0u8; 16];
    let d1 = u32::from_str_radix(&hex[0..8], 16).ok()?;
    let d2 = u16::from_str_radix(&hex[8..12], 16).ok()?;
    let d3 = u16::from_str_radix(&hex[12..16], 16).ok()?;
    g[0..4].copy_from_slice(&d1.to_le_bytes());
    g[4..6].copy_from_slice(&d2.to_le_bytes());
    g[6..8].copy_from_slice(&d3.to_le_bytes());
    for (i, b) in g[8..].iter_mut().enumerate() {
        *b = byte(8 + i)?;
    }
    Some(g)
}

/// Nom d'interface : « ethernet_N » (ConvertInterfaceLuidToNameW) ou le nom
/// Linux de l'interface.
unsafe extern "win64" fn ConvertInterfaceNameToLuidW(name: *const u16, luid: *mut u64) -> u32 {
    if name.is_null() || luid.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let text = super::wstr(name);
    let index = text
        .strip_prefix("ethernet_")
        .and_then(|n| n.parse::<u32>().ok())
        .or_else(|| {
            super::iphlpapi::adapters()
                .into_iter()
                .find(|a| a.name == text)
                .map(|a| a.index)
        });
    match index {
        Some(i) if i != 0 => {
            std::ptr::write_unaligned(luid, (6u64 << 48) | ((i as u64 & 0xFF_FFFF) << 24));
            ERROR_SUCCESS
        }
        _ => ERROR_INVALID_NAME,
    }
}

pub fn iphlpapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "ConvertInterfaceLuidToIndex" => fn ConvertInterfaceLuidToIndex,
        "ConvertInterfaceLuidToGuid" => fn ConvertInterfaceLuidToGuid,
        "ConvertInterfaceNameToLuidW" => fn ConvertInterfaceNameToLuidW,
    }
}

// ============================================================
//  ws2_32 : WSAAccept et conversions d'ordre des octets
// ============================================================

const WSAEFAULT: i32 = 10014;
const WSAECONNREFUSED: i32 = 10061;
const CF_ACCEPT: i32 = 0;

type Condition =
    unsafe extern "win64" fn(usize, usize, usize, usize, usize, usize, usize, usize) -> i32;

/// WSAAccept : accept, puis la fonction de condition si elle est donnée.
/// L'appelant reçoit l'adresse du pair (WSABUF) ; un refus ferme la
/// connexion acceptée (WSAECONNREFUSED). Pour un programme x86, la
/// condition n'est pas appelée et la connexion est acceptée.
unsafe extern "win64" fn WSAAccept(
    s: usize,
    addr: *mut u8,
    addrlen: *mut i32,
    condition: usize,
    data: usize,
) -> usize {
    let mut peer = [0u8; 128];
    let mut peer_len = peer.len() as i32;
    let a = super::ws2_32::accept(s, peer.as_mut_ptr(), &mut peer_len);
    if a == usize::MAX {
        return a;
    }
    if !addr.is_null() && !addrlen.is_null() {
        let n = (peer_len.max(0) as usize).min((*addrlen).max(0) as usize);
        std::ptr::copy_nonoverlapping(peer.as_ptr(), addr, n);
        *addrlen = peer_len;
    }
    if condition != 0 && !crate::wow32::active() {
        // WSABUF { ULONG len; CHAR *buf; }
        let caller: [usize; 2] = [peer_len as usize, peer.as_ptr() as usize];
        let f: Condition = std::mem::transmute(condition);
        let mut group = 0u32;
        let verdict = f(
            caller.as_ptr() as usize,
            0,
            0,
            0,
            0,
            0,
            &mut group as *mut u32 as usize,
            data,
        );
        if verdict != CF_ACCEPT {
            super::ws2_32::closesocket(a);
            super::ws2_32::set_error(WSAECONNREFUSED);
            return usize::MAX;
        }
    }
    a
}

unsafe fn put_order<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        super::ws2_32::set_error(WSAEFAULT);
        return -1;
    }
    std::ptr::write_unaligned(out, v);
    0
}

unsafe extern "win64" fn WSAHtonl(_s: usize, v: u32, out: *mut u32) -> i32 {
    put_order(out, v.to_be())
}
unsafe extern "win64" fn WSANtohl(_s: usize, v: u32, out: *mut u32) -> i32 {
    put_order(out, u32::from_be(v))
}
unsafe extern "win64" fn WSAHtons(_s: usize, v: u16, out: *mut u16) -> i32 {
    put_order(out, v.to_be())
}
unsafe extern "win64" fn WSANtohs(_s: usize, v: u16, out: *mut u16) -> i32 {
    put_order(out, u16::from_be(v))
}

pub fn ws2_32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WSAAccept" => fn WSAAccept,
        "WSAHtonl" => fn WSAHtonl,
        "WSANtohl" => fn WSANtohl,
        "WSAHtons" => fn WSAHtons,
        "WSANtohs" => fn WSANtohs,
    }
}

// ============================================================
//  shlwapi : StrStrIA, SHLoadIndirectString
// ============================================================

/// StrStrIA : première occurrence, sans tenir compte de la casse ASCII.
unsafe extern "win64" fn StrStrIA(text: *const u8, find: *const u8) -> *const u8 {
    if text.is_null() || find.is_null() {
        return std::ptr::null();
    }
    let hay = std::ffi::CStr::from_ptr(text.cast()).to_bytes();
    let needle = std::ffi::CStr::from_ptr(find.cast()).to_bytes();
    if needle.is_empty() {
        return std::ptr::null();
    }
    hay.windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map_or(std::ptr::null(), |i| text.add(i))
}

/// SHLoadIndirectString : « @fichier,-id » est lu dans la table de chaînes
/// du fichier (variables d'environnement développées) ; une chaîne sans
/// « @ » est copiée telle quelle. Les références de paquet
/// (« @{…} ») ne sont pas servies (E_FAIL).
unsafe extern "win64" fn SHLoadIndirectString(
    src: *const u16,
    out: *mut u16,
    cch: u32,
    _reserved: usize,
) -> i32 {
    if src.is_null() || out.is_null() || cch == 0 {
        return E_INVALIDARG;
    }
    let text = super::wstr(src);
    let result: Vec<u16> = match text.strip_prefix('@') {
        None => text.encode_utf16().collect(),
        Some(rest) => {
            let Some((file, id)) = rest.rsplit_once(',') else {
                *out = 0;
                return E_FAIL;
            };
            let id = id.split(';').next().unwrap_or("").trim();
            let Some(id) = id.strip_prefix('-').and_then(|n| n.parse::<u32>().ok()) else {
                *out = 0;
                return E_FAIL;
            };
            match load_resource_string(file, id) {
                Some(s) => s,
                None => {
                    *out = 0;
                    return E_FAIL;
                }
            }
        }
    };
    let n = result.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(result.as_ptr(), out, n);
    *out.add(n) = 0;
    if n < result.len() {
        STRSAFE_E_INSUFFICIENT_BUFFER
    } else {
        S_OK
    }
}

unsafe fn load_resource_string(file: &str, id: u32) -> Option<Vec<u16>> {
    let raw: Vec<u16> = file.encode_utf16().chain([0]).collect();
    let mut expanded = vec![0u16; 1024];
    let n = super::kernel32_fs::expand_environment_strings_w(
        raw.as_ptr(),
        expanded.as_mut_ptr(),
        expanded.len() as u32,
    );
    let path = if n == 0 || n as usize > expanded.len() {
        raw
    } else {
        expanded
    };
    // LOAD_LIBRARY_AS_DATAFILE
    let module = super::kernel32::load_library_ex_w(path.as_ptr(), 0, 2);
    if module == 0 {
        return None;
    }
    let mut buf = vec![0u16; 4096];
    let len = super::resource::load_string_w(module, id, buf.as_mut_ptr(), buf.len() as i32);
    super::kernel32::free_library(module);
    (len > 0).then(|| buf[..len as usize].to_vec())
}

pub fn shlwapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "StrStrIA" => fn StrStrIA,
        "SHLoadIndirectString" => fn SHLoadIndirectString,
        "#487" => fn SHLoadIndirectString,
    }
}

// ============================================================
//  shell32 : icônes, « Ouvrir avec », gestionnaires de protocole
// ============================================================

/// ExtractIconA : comme ExtractIconW de Winlux, aucune icône extraite.
extern "win64" fn ExtractIconA(_inst: usize, _file: *const u8, _index: u32) -> usize {
    0
}

/// ExtractAssociatedIconW : sans icône propre au fichier, l'icône
/// d'application par défaut (IDI_APPLICATION), comme le prévoit la
/// documentation ; l'index rendu est 0.
unsafe extern "win64" fn ExtractAssociatedIconW(
    _inst: usize,
    _path: *mut u16,
    index: *mut u16,
) -> usize {
    if !index.is_null() {
        *index = 0;
    }
    super::icons::load_icon_w(0, 32512 as *const u16)
}

/// SHOpenWithDialog : la boîte « Ouvrir avec » n'est pas servie aux
/// programmes ; ils retombent sur leur propre choix.
extern "win64" fn SHOpenWithDialog(_hwnd: usize, _info: usize) -> i32 {
    E_NOTIMPL
}

// IEnumAssocHandlers vide : QueryInterface, AddRef, Release, Next.
unsafe extern "win64" fn enum_qi(this: usize, _riid: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = this;
    S_OK
}
extern "win64" fn enum_ref(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn enum_next(
    _this: usize,
    _celt: u32,
    _items: usize,
    fetched: *mut u32,
) -> i32 {
    if !fetched.is_null() {
        *fetched = 0;
    }
    1 // S_FALSE : aucun gestionnaire
}

/// Objet unique et sans état : un pointeur de vtable.
fn empty_enum() -> usize {
    static OBJ: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *OBJ.get_or_init(|| {
        let vtbl: &'static [usize; 4] = Box::leak(Box::new([
            enum_qi as *const () as usize,
            enum_ref as *const () as usize,
            enum_ref as *const () as usize,
            enum_next as *const () as usize,
        ]));
        let obj: &'static usize = Box::leak(Box::new(vtbl.as_ptr() as usize));
        obj as *const usize as usize
    })
}

/// SHAssocEnumHandlersForProtocolByApplication : aucun gestionnaire de
/// protocole déclaré par application sous Winlux, d'où un énumérateur
/// vide (x64 ; les programmes x86 reçoivent E_NOTIMPL).
unsafe extern "win64" fn SHAssocEnumHandlersForProtocolByApplication(
    _protocol: *const u16,
    _riid: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if crate::wow32::active() {
        write_ptr(out as usize, 0);
        return E_NOTIMPL;
    }
    *out = empty_enum();
    S_OK
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "ExtractIconA" => fn ExtractIconA,
        "ExtractAssociatedIconW" => fn ExtractAssociatedIconW,
        "SHOpenWithDialog" => fn SHOpenWithDialog,
        "SHAssocEnumHandlersForProtocolByApplication" => fn SHAssocEnumHandlersForProtocolByApplication,
    }
}

// ============================================================
//  crypt32 : émetteur dans un magasin, fonctions d'OID
// ============================================================

const CERT_STORE_SIGNATURE_FLAG: u32 = 1;
const CERT_STORE_TIME_VALIDITY_FLAG: u32 = 2;
const CERT_STORE_REVOCATION_FLAG: u32 = 4;
const CERT_STORE_NO_CRL_FLAG: u32 = 0x1_0000;
const CERT_FIND_SUBJECT_NAME: u32 = 0x2_0007;
const CRYPT_E_SELF_SIGNED: u32 = 0x8009_2007;

/// CertGetIssuerCertificateFromStore : certificat du magasin dont le sujet
/// est l'émetteur de `subject`. Les vérifications demandées dans `flags`
/// sont effacées quand elles réussissent (signature par la clé de
/// l'émetteur, validité à cette heure) ; la révocation n'est pas vérifiée
/// (CERT_STORE_NO_CRL_FLAG).
unsafe extern "win64" fn CertGetIssuerCertificateFromStore(
    store: usize,
    subject: usize,
    prev: usize,
    flags: *mut u32,
) -> usize {
    use super::crypt32::{context, CertFindCertificateInStore, CertVerifyTimeValidity};
    if subject == 0 || flags.is_null() {
        set_last_error(E_INVALIDARG as u32);
        return 0;
    }
    let s = context(subject);
    let info = &*s.info;
    let blob = |b: &super::crypt32::Blob| std::slice::from_raw_parts(b.pb, b.cb as usize).to_vec();
    if blob(&info.issuer) == blob(&info.subject) {
        set_last_error(CRYPT_E_SELF_SIGNED);
        return 0;
    }
    // CERT_NAME_BLOB à la taille de pointeur du programme.
    let mut issuer_name = [0u64; 2];
    let at = issuer_name.as_mut_ptr() as usize;
    (at as *mut u32).write(info.issuer.cb);
    crate::wow32::write_ptr(at + crate::wow32::pointer_size(), info.issuer.pb as usize);
    let found = CertFindCertificateInStore(store, 1, 0, CERT_FIND_SUBJECT_NAME, at, prev);
    if found == 0 {
        return 0;
    }
    let wanted = *flags;
    let mut left = wanted;
    let issuer = context(found);
    if wanted & CERT_STORE_SIGNATURE_FLAG != 0 {
        let der = std::slice::from_raw_parts(s.der, s.der_len as usize);
        let issuer_der = std::slice::from_raw_parts(issuer.der, issuer.der_len as usize);
        if signature_ok(der, issuer_der) {
            left &= !CERT_STORE_SIGNATURE_FLAG;
        }
    }
    if wanted & CERT_STORE_TIME_VALIDITY_FLAG != 0
        && CertVerifyTimeValidity(std::ptr::null(), s.info) == 0
    {
        left &= !CERT_STORE_TIME_VALIDITY_FLAG;
    }
    if wanted & CERT_STORE_REVOCATION_FLAG != 0 {
        left |= CERT_STORE_NO_CRL_FLAG;
    }
    *flags = left;
    found
}

/// Signature RSA du certificat `der` vérifiée avec la clé de `issuer`.
fn signature_ok(der: &[u8], issuer: &[u8]) -> bool {
    use super::crypt32::{children, oid_str, tlv};
    let Some((outer, _)) = tlv(der) else {
        return false;
    };
    let parts = children(outer.body);
    if parts.len() != 3 || parts[2].body.is_empty() {
        return false;
    }
    let alg = children(parts[1].body)
        .first()
        .map(|o| oid_str(o.body))
        .unwrap_or_default();
    let md = match alg.as_str() {
        "1.2.840.113549.1.1.5" => c"SHA1",
        "1.2.840.113549.1.1.11" => c"SHA256",
        "1.2.840.113549.1.1.12" => c"SHA384",
        "1.2.840.113549.1.1.13" => c"SHA512",
        "1.2.840.113549.1.1.4" => c"MD5",
        _ => return false,
    };
    super::hostssl::verify_signature(issuer, md, &parts[2].body[1..], parts[0].all) == Some(true)
}

/// CryptInstallOIDFunctionAddress : Winlux n'a pas d'ensembles de
/// fonctions d'OID extensibles ; l'installation est acceptée et sans effet.
extern "win64" fn CryptInstallOIDFunctionAddress(
    _module: usize,
    _enc: u32,
    _func_name: usize,
    _count: u32,
    _entries: usize,
    _flags: u32,
) -> i32 {
    1
}

pub fn crypt32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CertGetIssuerCertificateFromStore" => fn CertGetIssuerCertificateFromStore,
        "CryptInstallOIDFunctionAddress" => fn CryptInstallOIDFunctionAddress,
    }
}

// ============================================================
//  user32, winspool, CRT
// ============================================================

/// CloseTouchInputHandle : les poignées de WM_TOUCH ne portent rien.
extern "win64" fn CloseTouchInputHandle(_h: usize) -> i32 {
    1
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CloseTouchInputHandle" => fn CloseTouchInputHandle,
    }
}

const ERROR_UNKNOWN_PRINTER_DRIVER: u32 = 1797;

/// GetPrinterDriverW : aucun pilote d'impression Windows n'est installé
/// (Winlux imprime par CUPS).
unsafe extern "win64" fn GetPrinterDriverW(
    _printer: usize,
    _env: *const u16,
    _level: u32,
    _buf: *mut u8,
    _size: u32,
    needed: *mut u32,
) -> i32 {
    if !needed.is_null() {
        *needed = 0;
    }
    set_last_error(ERROR_UNKNOWN_PRINTER_DRIVER);
    0
}

pub fn winspool_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetPrinterDriverW" => fn GetPrinterDriverW,
    }
}

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;
const STRUNCATE: i32 = 80;

/// mbstowcs_s : conversion bornée, zéro final compris dans `*converted`.
unsafe extern "win64" fn mbstowcs_s(
    converted: *mut usize,
    dst: *mut u16,
    dst_size: usize,
    src: *const u8,
    count: usize,
) -> i32 {
    let fail = |e: i32| {
        super::crt_runtime::set_errno_value(e);
        e
    };
    if (dst.is_null() && dst_size != 0) || (!dst.is_null() && dst_size == 0) {
        return fail(EINVAL);
    }
    if !dst.is_null() {
        *dst = 0;
    }
    if !converted.is_null() {
        *converted = 0;
    }
    if src.is_null() {
        return if count == 0 { 0 } else { fail(EINVAL) };
    }
    let len = crate::printf::narrow_len(src).min(count);
    if dst.is_null() {
        if !converted.is_null() {
            *converted = len + 1;
        }
        return 0;
    }
    let (n, code) = if len < dst_size {
        (len, 0)
    } else if count == usize::MAX {
        (dst_size - 1, STRUNCATE)
    } else {
        return fail(ERANGE);
    };
    for i in 0..n {
        *dst.add(i) = *src.add(i) as u16;
    }
    *dst.add(n) = 0;
    if !converted.is_null() {
        *converted = n + 1;
    }
    code
}

pub fn crt_exports() -> crate::dlls::Exports {
    crate::exports! {
        "mbstowcs_s" => fn mbstowcs_s,
    }
}

// ============================================================
//  winmm : mélangeur
// ============================================================
//
// Un seul mélangeur, une destination « haut-parleurs » à un canal et deux
// contrôles (volume, sourdine). Les valeurs sont gardées dans le processus
// et ne pilotent pas le volume du système.

const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_BADDEVICEID: u32 = 2;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_INVALFLAG: u32 = 10;
const MMSYSERR_INVALPARAM: u32 = 11;
const MIXERR_INVALLINE: u32 = 1024;
const MIXERR_INVALCONTROL: u32 = 1025;

const MIXER_HANDLE: usize = 0x4D58_0000;
const LINE_ID: u32 = 0xFFFF_0000;
const VOLUME_ID: u32 = 1;
const MUTE_ID: u32 = 2;
const CT_VOLUME: u32 = 0x5003_0001;
const CT_MUTE: u32 = 0x2001_0002;
const COMPONENT_SPEAKERS: u32 = 4;

static VOLUME: AtomicU32 = AtomicU32::new(0xFFFF);
static MUTE: AtomicU32 = AtomicU32::new(0);

/// Pointeur du programme dans une structure alignée sur 1 octet.
unsafe fn read_ptr_packed(at: usize) -> usize {
    if pointer_size() == 4 {
        std::ptr::read_unaligned(at as *const u32) as usize
    } else {
        std::ptr::read_unaligned(at as *const usize)
    }
}

unsafe fn put_u32(p: *mut u8, at: usize, v: u32) {
    std::ptr::write_unaligned(p.add(at) as *mut u32, v);
}

unsafe fn put_text(p: *mut u8, at: usize, chars: usize, text: &str) {
    let w: Vec<u16> = text.encode_utf16().take(chars - 1).collect();
    std::ptr::write_bytes(p.add(at), 0, chars * 2);
    for (i, c) in w.iter().enumerate() {
        std::ptr::write_unaligned(p.add(at + 2 * i) as *mut u16, *c);
    }
}

extern "win64" fn mixerGetNumDevs() -> u32 {
    1
}

/// MIXERCAPSW : 80 octets.
unsafe extern "win64" fn mixerGetDevCapsW(id: usize, caps: *mut u8, size: u32) -> u32 {
    if caps.is_null() || size < 80 {
        return MMSYSERR_INVALPARAM;
    }
    if id != 0 && id != MIXER_HANDLE {
        return MMSYSERR_BADDEVICEID;
    }
    std::ptr::write_bytes(caps, 0, 80);
    put_u32(caps, 4, 0x0100);
    put_text(caps, 8, 32, "Winlux");
    put_u32(caps, 76, 1); // cDestinations
    MMSYSERR_NOERROR
}

unsafe extern "win64" fn mixerOpen(
    out: *mut usize,
    id: u32,
    _cb: usize,
    _inst: usize,
    _flags: u32,
) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if id != 0 && id as usize != MIXER_HANDLE {
        return MMSYSERR_BADDEVICEID;
    }
    write_ptr(out as usize, MIXER_HANDLE);
    MMSYSERR_NOERROR
}

extern "win64" fn mixerClose(h: usize) -> u32 {
    if h == MIXER_HANDLE {
        MMSYSERR_NOERROR
    } else {
        MMSYSERR_INVALHANDLE
    }
}

unsafe extern "win64" fn mixerGetID(_obj: usize, id: *mut u32, _flags: u32) -> u32 {
    if id.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    *id = 0;
    MMSYSERR_NOERROR
}

/// MIXERLINEW : 284 octets en x64, 280 en x86 (dwUser de taille pointeur).
unsafe extern "win64" fn mixerGetLineInfoW(_obj: usize, line: *mut u8, flags: u32) -> u32 {
    if line.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let ps = pointer_size();
    let size = 276 + ps;
    let cb = std::ptr::read_unaligned(line as *const u32) as usize;
    if cb < size {
        return MMSYSERR_INVALPARAM;
    }
    let read = |at: usize| std::ptr::read_unaligned(line.add(at) as *const u32);
    let ok = match flags & 0xF {
        0 => read(4) == 0,
        2 => read(12) == LINE_ID,
        3 => read(20 + ps) == COMPONENT_SPEAKERS,
        4 => true,
        1 => false,
        _ => return MMSYSERR_INVALFLAG,
    };
    if !ok {
        return MIXERR_INVALLINE;
    }
    std::ptr::write_bytes(line.add(4), 0, size - 4);
    put_u32(line, 12, LINE_ID);
    put_u32(line, 16, 1); // MIXERLINE_LINEF_ACTIVE
    let o = 20 + ps; // après dwUser
    put_u32(line, o, COMPONENT_SPEAKERS);
    put_u32(line, o + 4, 1); // cChannels
    put_u32(line, o + 12, 2); // cControls
    put_text(line, o + 16, 16, "Haut-parleurs");
    put_text(line, o + 48, 64, "Haut-parleurs");
    put_u32(line, o + 176, 1); // Target.dwType : MIXERLINE_TARGETTYPE_WAVEOUT
    put_text(line, o + 192, 32, "Winlux");
    MMSYSERR_NOERROR
}

/// MIXERCONTROLW (228 octets) du contrôle `id`.
unsafe fn write_control(p: *mut u8, id: u32) {
    std::ptr::write_bytes(p, 0, 228);
    put_u32(p, 0, 228);
    put_u32(p, 4, id);
    let (ty, name, max) = if id == VOLUME_ID {
        (CT_VOLUME, "Volume", 0xFFFF)
    } else {
        (CT_MUTE, "Muet", 1)
    };
    put_u32(p, 8, ty);
    put_u32(p, 12, 1); // MIXERCONTROL_CONTROLF_UNIFORM
    put_text(p, 20, 16, name);
    put_text(p, 52, 64, name);
    put_u32(p, 184, max);
    put_u32(p, 204, if id == VOLUME_ID { 0x10000 } else { 0 });
}

/// MIXERLINECONTROLSW : cbStruct, dwLineID, dwControlID/Type, cControls,
/// cbmxctrl, pamxctrl.
unsafe extern "win64" fn mixerGetLineControlsW(_obj: usize, lc: *mut u8, flags: u32) -> u32 {
    if lc.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let read = |at: usize| std::ptr::read_unaligned(lc.add(at) as *const u32);
    let count = read(12);
    let cb = read(16) as usize;
    let arr = read_ptr_packed(lc as usize + 20) as *mut u8;
    if arr.is_null() || cb < 228 {
        return MMSYSERR_INVALPARAM;
    }
    let ids: Vec<u32> = match flags & 0xF {
        0 => {
            if read(4) != LINE_ID {
                return MIXERR_INVALLINE;
            }
            if count < 2 {
                return MMSYSERR_INVALPARAM;
            }
            vec![VOLUME_ID, MUTE_ID]
        }
        1 => match read(8) {
            id @ (VOLUME_ID | MUTE_ID) => vec![id],
            _ => return MIXERR_INVALCONTROL,
        },
        2 => {
            if read(4) != LINE_ID {
                return MIXERR_INVALLINE;
            }
            match read(8) {
                CT_VOLUME => vec![VOLUME_ID],
                CT_MUTE => vec![MUTE_ID],
                _ => return MIXERR_INVALCONTROL,
            }
        }
        _ => return MMSYSERR_INVALFLAG,
    };
    if flags & 0xF == 1 {
        put_u32(lc, 4, LINE_ID);
    }
    for (i, id) in ids.iter().enumerate() {
        write_control(arr.add(i * cb), *id);
    }
    MMSYSERR_NOERROR
}

/// MIXERCONTROLDETAILS : cbStruct, dwControlID, cChannels,
/// hwndOwner/cMultipleItems (taille pointeur), cbDetails, paDetails.
unsafe fn details(cd: *mut u8) -> Option<(u32, u32, usize, *mut u8)> {
    if cd.is_null() {
        return None;
    }
    let ps = pointer_size();
    let id = std::ptr::read_unaligned(cd.add(4) as *const u32);
    let channels = std::ptr::read_unaligned(cd.add(8) as *const u32).max(1);
    let cb = std::ptr::read_unaligned(cd.add(12 + ps) as *const u32) as usize;
    let pa = read_ptr_packed(cd as usize + 16 + ps) as *mut u8;
    (!pa.is_null() && cb >= 4).then_some((id, channels, cb, pa))
}

fn cell(id: u32) -> Option<&'static AtomicU32> {
    match id {
        VOLUME_ID => Some(&VOLUME),
        MUTE_ID => Some(&MUTE),
        _ => None,
    }
}

unsafe extern "win64" fn mixerGetControlDetailsW(_obj: usize, cd: *mut u8, _flags: u32) -> u32 {
    let Some((id, channels, cb, pa)) = details(cd) else {
        return MMSYSERR_INVALPARAM;
    };
    let Some(c) = cell(id) else {
        return MIXERR_INVALCONTROL;
    };
    for ch in 0..channels as usize {
        put_u32(pa, ch * cb, c.load(Ordering::Relaxed));
    }
    MMSYSERR_NOERROR
}

unsafe extern "win64" fn mixerSetControlDetails(_obj: usize, cd: *mut u8, _flags: u32) -> u32 {
    let Some((id, _, _, pa)) = details(cd) else {
        return MMSYSERR_INVALPARAM;
    };
    let Some(c) = cell(id) else {
        return MIXERR_INVALCONTROL;
    };
    let v = std::ptr::read_unaligned(pa as *const u32);
    c.store(
        if id == MUTE_ID {
            (v != 0) as u32
        } else {
            v.min(0xFFFF)
        },
        Ordering::Relaxed,
    );
    MMSYSERR_NOERROR
}

pub fn winmm_exports() -> crate::dlls::Exports {
    crate::exports! {
        "mixerGetNumDevs" => fn mixerGetNumDevs,
        "mixerGetDevCapsW" => fn mixerGetDevCapsW,
        "mixerOpen" => fn mixerOpen,
        "mixerClose" => fn mixerClose,
        "mixerGetID" => fn mixerGetID,
        "mixerGetLineInfoW" => fn mixerGetLineInfoW,
        "mixerGetLineControlsW" => fn mixerGetLineControlsW,
        "mixerGetControlDetailsW" => fn mixerGetControlDetailsW,
        "mixerSetControlDetails" => fn mixerSetControlDetails,
    }
}

/// Toutes les tables, par DLL, pour `add_basics`.
pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        ("iphlpapi", iphlpapi_exports()),
        ("ws2_32", ws2_32_exports()),
        ("shlwapi", shlwapi_exports()),
        ("shell32", shell32_exports()),
        ("crypt32", crypt32_exports()),
        ("user32", user32_exports()),
        ("winspool", winspool_exports()),
        ("msvcrt", crt_exports()),
        ("ucrtbase", crt_exports()),
        ("winmm", winmm_exports()),
    ]
}

#[cfg(test)]
mod tests;
