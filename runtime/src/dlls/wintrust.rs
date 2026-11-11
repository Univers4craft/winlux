//! wintrust.dll : WinVerifyTrust et les WTHelper* qui lisent son état
//! (#1728).
//!
//! WinVerifyTrust vérifie la signature Authenticode d'un fichier avec le
//! même vérificateur que la fenêtre d'élévation (`appx_signature`, #1382) :
//! fichier non signé → TRUST_E_NOSIGNATURE, signature fausse ou fichier
//! modifié → TRUST_E_BAD_DIGEST, autorité non reconnue → CERT_E_UNTRUSTEDROOT.
//! Avec WTD_STATEACTION_VERIFY, il garde comme Windows un état dont
//! `hWVTStateData` est le CRYPT_PROVIDER_DATA, avec un signataire
//! (CRYPT_PROVIDER_SGNR) et ses certificats (CRYPT_PROVIDER_CERT, pCert
//! étant un vrai CERT_CONTEXT de crypt32) ; WTD_STATEACTION_CLOSE le libère.
//! Les dispositions suivent wintrust.h (en-têtes mingw-w64) pour x86 et x64.
//! Il n'y a pas de catalogue de signatures : une demande WTD_CHOICE_CATALOG
//! rend TRUST_E_NOSIGNATURE.
#![allow(non_snake_case)]

use std::collections::BTreeMap;
use std::sync::Mutex;

const TRUST_E_NOSIGNATURE: u32 = 0x800B_0100;
const TRUST_E_SUBJECT_FORM_UNKNOWN: u32 = 0x800B_0003;
const TRUST_E_BAD_DIGEST: u32 = 0x8009_6010;
const CERT_E_UNTRUSTEDROOT: u32 = 0x800B_0109;
const E_INVALIDARG: u32 = 0x8007_0057;

const WTD_CHOICE_FILE: u32 = 1;
const WTD_CHOICE_CATALOG: u32 = 2;
const WTD_STATEACTION_VERIFY: u32 = 1;
const WTD_STATEACTION_CLOSE: u32 = 2;
const WSS_VERIFY_SPECIFIC: u32 = 1;
const WSS_GET_SECONDARY_SIG_COUNT: u32 = 2;
/// TRUSTERROR_MAX_STEPS : taille de padwTrustStepErrors.
const TRUST_STEPS: usize = 38;

/// Champ d'une structure C : DWORD, pointeur ou FILETIME.
#[derive(Clone, Copy)]
enum F {
    D,
    P,
    Ft,
}

/// Décalages des champs et taille de la structure, pour une taille de
/// pointeur `ps`.
fn layout(fields: &[F], ps: usize) -> (Vec<usize>, usize) {
    let mut offsets = Vec::with_capacity(fields.len());
    let (mut off, mut align) = (0usize, 4usize);
    for f in fields {
        let (size, a) = match f {
            F::D => (4, 4),
            F::P => (ps, ps),
            F::Ft => (8, 4),
        };
        off = off.next_multiple_of(a);
        offsets.push(off);
        off += size;
        align = align.max(a);
    }
    (offsets, off.next_multiple_of(align))
}

use F::{Ft, D, P};

/// WINTRUST_DATA : cbStruct, pPolicyCallbackData, pSIPClientData,
/// dwUIChoice, fdwRevocationChecks, dwUnionChoice, pFile…, dwStateAction,
/// hWVTStateData, pwszURLReference, dwProvFlags, dwUIContext,
/// pSignatureSettings.
const WINTRUST_DATA: &[F] = &[D, P, P, D, D, D, P, D, P, P, D, D, P];
/// WINTRUST_FILE_INFO : cbStruct, pcwszFilePath, hFile, pgKnownSubject.
const WINTRUST_FILE_INFO: &[F] = &[D, P, P, P];
/// CRYPT_PROVIDER_DATA (wintrust.h).
const PROVIDER_DATA: &[F] = &[
    D, P, D, P, P, P, D, D, D, P, D, P, D, P, D, P, D, P, D, P, D, P, P, D, Ft, P, D, D, P, D, D,
];
/// CRYPT_PROVIDER_SGNR : cbStruct, sftVerifyAsOf, csCertChain,
/// pasCertChain, dwSignerType, psSigner, dwError, csCounterSigners,
/// pasCounterSigners, pChainContext.
const PROVIDER_SGNR: &[F] = &[D, Ft, D, P, D, P, D, D, P, P];
/// CRYPT_PROVIDER_CERT : cbStruct, pCert, fCommercial, fTrustedRoot,
/// fSelfSigned, fTestCert, dwRevokedReason, dwConfidence, dwError,
/// pTrustListContext, fTrustListSignerCert, pCtlContext, dwCtlError,
/// fIsCyclic, pChainElement.
const PROVIDER_CERT: &[F] = &[D, P, D, D, D, D, D, D, D, P, D, P, D, D, P];

// Indices des champs utilisés.
const WD_UNION_CHOICE: usize = 5;
const WD_FILE: usize = 6;
const WD_STATE_ACTION: usize = 7;
const WD_STATE: usize = 8;
const WD_PROV_FLAGS: usize = 10;
const WD_SIGNATURE_SETTINGS: usize = 12;

/// État gardé entre VERIFY et CLOSE : les blocs vus par le programme et les
/// contextes de certificats à libérer.
struct State {
    _data: Vec<u64>,
    _signer: Vec<u64>,
    _certs: Vec<u64>,
    _steps: Vec<u32>,
    contexts: Vec<usize>,
    signer_at: usize,
    certs_at: usize,
    cert_count: usize,
}

static STATES: Mutex<BTreeMap<usize, State>> = Mutex::new(BTreeMap::new());

fn states() -> std::sync::MutexGuard<'static, BTreeMap<usize, State>> {
    STATES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bloc zéro d'au moins `size` octets, aligné sur 8.
fn block(size: usize) -> Vec<u64> {
    vec![0u64; size.div_ceil(8)]
}

unsafe fn put32(base: usize, off: usize, v: u32) {
    ((base + off) as *mut u32).write_unaligned(v);
}

unsafe fn put_ptr(base: usize, off: usize, v: usize) {
    crate::wow32::write_ptr(base + off, v);
}

fn now_filetime() -> u64 {
    let mut ft = 0u64;
    unsafe { crate::dlls::kernel32::get_system_time_as_file_time(&mut ft) };
    ft
}

static USAGE_OID: &[u8] = b"1.3.6.1.5.5.7.3.3\0";

/// Bâtit l'état d'une vérification : CRYPT_PROVIDER_DATA → SGNR → CERT.
unsafe fn build_state(
    data: usize,
    action: usize,
    prov_flags: u32,
    status: u32,
    certs: &[Vec<u8>],
) -> usize {
    let ps = crate::wow32::pointer_size();
    let (pd, pd_size) = layout(PROVIDER_DATA, ps);
    let (sg, sg_size) = layout(PROVIDER_SGNR, ps);
    let (pc, pc_size) = layout(PROVIDER_CERT, ps);
    let now = now_filetime();
    let mut contexts = Vec::new();
    for der in certs {
        let ctx = crate::dlls::crypt32::CertCreateCertificateContext(
            0x0001_0001, // X509_ASN_ENCODING | PKCS_7_ASN_ENCODING
            der.as_ptr(),
            der.len() as u32,
        );
        if ctx != 0 {
            contexts.push(ctx);
        }
    }
    let mut cert_block = block(pc_size * contexts.len().max(1));
    let cb = cert_block.as_mut_ptr() as usize;
    for (i, &ctx) in contexts.iter().enumerate() {
        let c = cb + i * pc_size;
        put32(c, pc[0], pc_size as u32);
        put_ptr(c, pc[1], ctx);
        let last = i + 1 == contexts.len();
        put32(c, pc[3], u32::from(last && status == 0)); // fTrustedRoot
                                                         // dwConfidence : signature et dates vérifiées.
        put32(c, pc[7], 0x1000_0000 | 0x0100_0000);
        put32(c, pc[8], if i == 0 { status } else { 0 });
    }
    let mut signer_block = block(sg_size);
    let sb = signer_block.as_mut_ptr() as usize;
    put32(sb, sg[0], sg_size as u32);
    ((sb + sg[1]) as *mut u64).write_unaligned(now);
    put32(sb, sg[2], contexts.len() as u32);
    put_ptr(sb, sg[3], cb);
    put32(sb, sg[6], status);
    let mut steps = vec![0u32; TRUST_STEPS];
    let mut data_block = block(pd_size);
    let db = data_block.as_mut_ptr() as usize;
    put32(db, pd[0], pd_size as u32);
    put_ptr(db, pd[1], data);
    put_ptr(db, pd[3], usize::MAX); // hWndParent : INVALID_HANDLE_VALUE
    put_ptr(db, pd[4], action);
    put32(db, pd[10], TRUST_STEPS as u32);
    put_ptr(db, pd[11], steps.as_mut_ptr() as usize);
    put32(db, pd[14], 0x0001_0001);
    put32(db, pd[16], 1); // csSigners
    put_ptr(db, pd[17], sb);
    put_ptr(db, pd[22], USAGE_OID.as_ptr() as usize);
    ((db + pd[24]) as *mut u64).write_unaligned(now);
    put32(db, pd[26], prov_flags);
    put32(db, pd[27], status); // dwFinalError
    states().insert(
        db,
        State {
            _data: data_block,
            _signer: signer_block,
            _certs: cert_block,
            _steps: steps,
            cert_count: contexts.len(),
            contexts,
            signer_at: sb,
            certs_at: cb,
        },
    );
    db
}

fn close_state(h: usize) {
    if let Some(s) = states().remove(&h) {
        for c in s.contexts {
            crate::dlls::crypt32::CertFreeCertificateContext(c);
        }
    }
}

/// Chemin Linux du fichier désigné par WINTRUST_FILE_INFO.
unsafe fn file_path(info: usize) -> Option<std::path::PathBuf> {
    let ps = crate::wow32::pointer_size();
    let (f, _) = layout(WINTRUST_FILE_INFO, ps);
    let path = crate::wow32::read_ptr(info + f[1]);
    if path != 0 {
        let w = crate::dlls::wstr(path as *const u16);
        return Some(crate::paths::to_linux(&w));
    }
    let handle = crate::wow32::read_ptr(info + f[2]);
    let fd = crate::dlls::kernel32::fd_of_handle(handle)?;
    std::fs::read_link(format!("/proc/self/fd/{fd}")).ok()
}

fn fail(status: u32) -> i32 {
    crate::process::set_last_error(status);
    status as i32
}

/// WinVerifyTrust(hwnd, pgActionID, pWVTData).
/// WinVerifyTrustEx : même vérification que WinVerifyTrust (la
/// documentation la décrit comme sa forme typée, WINTRUST_DATA en
/// troisième argument) ; Teams l'importe (#1791).
pub unsafe extern "win64" fn WinVerifyTrustEx(hwnd: usize, action: usize, data: usize) -> i32 {
    WinVerifyTrust(hwnd, action, data)
}

pub unsafe extern "win64" fn WinVerifyTrust(_hwnd: usize, action: usize, data: usize) -> i32 {
    if data == 0 {
        return fail(E_INVALIDARG);
    }
    let ps = crate::wow32::pointer_size();
    let (w, _) = layout(WINTRUST_DATA, ps);
    let cb_struct = *(data as *const u32) as usize;
    let state_action = *((data + w[WD_STATE_ACTION]) as *const u32);
    if state_action == WTD_STATEACTION_CLOSE {
        close_state(crate::wow32::read_ptr(data + w[WD_STATE]));
        put_ptr(data, w[WD_STATE], 0);
        return 0;
    }
    if state_action == WTD_STATEACTION_VERIFY {
        put_ptr(data, w[WD_STATE], 0);
    }
    let settings = if cb_struct >= w[WD_SIGNATURE_SETTINGS] + ps {
        crate::wow32::read_ptr(data + w[WD_SIGNATURE_SETTINGS])
    } else {
        0
    };
    // WINTRUST_SIGNATURE_SETTINGS : cbStruct, dwIndex, dwFlags,
    // cSecondarySigs, dwVerifiedSigIndex, pCryptoPolicy. Seule la signature
    // principale est examinée : aucune signature secondaire annoncée.
    if settings != 0 {
        let index = *((settings + 4) as *const u32);
        let flags = *((settings + 8) as *const u32);
        if flags & WSS_GET_SECONDARY_SIG_COUNT != 0 {
            put32(settings, 12, 0);
        }
        if flags & WSS_VERIFY_SPECIFIC != 0 && index != 0 {
            return fail(TRUST_E_NOSIGNATURE);
        }
        put32(settings, 16, 0);
    }
    match *((data + w[WD_UNION_CHOICE]) as *const u32) {
        WTD_CHOICE_FILE => {}
        WTD_CHOICE_CATALOG => return fail(TRUST_E_NOSIGNATURE),
        _ => return fail(TRUST_E_SUBJECT_FORM_UNKNOWN),
    }
    let info = crate::wow32::read_ptr(data + w[WD_FILE]);
    let Some(path) = (if info == 0 { None } else { file_path(info) }) else {
        return fail(E_INVALIDARG);
    };
    use crate::setup::appx_signature::{examine_pe_with_certs, Signature};
    let (signature, certs) = examine_pe_with_certs(&path);
    let status = match &signature {
        Signature::Verified { .. } => 0,
        Signature::Absent => TRUST_E_NOSIGNATURE,
        Signature::Unverified { .. } => CERT_E_UNTRUSTEDROOT,
        Signature::Invalid(_) => TRUST_E_BAD_DIGEST,
    };
    crate::trace!(
        "WinVerifyTrust({}) : {signature:?} → {status:#x}",
        path.display()
    );
    if state_action == WTD_STATEACTION_VERIFY && !certs.is_empty() {
        let prov_flags = *((data + w[WD_PROV_FLAGS]) as *const u32);
        let h = build_state(data, action, prov_flags, status, &certs);
        put_ptr(data, w[WD_STATE], h);
    }
    if status == 0 {
        0
    } else {
        fail(status)
    }
}

/// WTHelperProvDataFromStateData : l'état est le CRYPT_PROVIDER_DATA.
pub extern "win64" fn WTHelperProvDataFromStateData(state: usize) -> usize {
    if states().contains_key(&state) {
        state
    } else {
        0
    }
}

/// WTHelperGetProvSignerFromChain(données, signataire, contre-signataire ?,
/// indice du contre-signataire) : un seul signataire, sans contre-signature.
pub extern "win64" fn WTHelperGetProvSignerFromChain(
    data: usize,
    signer: u32,
    counter: i32,
    _counter_index: u32,
) -> usize {
    match states().get(&data) {
        Some(s) if signer == 0 && counter == 0 => s.signer_at,
        _ => 0,
    }
}

/// WTHelperGetProvCertFromChain(signataire, indice) : certificat `indice`
/// de la chaîne, le signataire en tête.
pub extern "win64" fn WTHelperGetProvCertFromChain(signer: usize, index: u32) -> usize {
    let ps = crate::wow32::pointer_size();
    let (_, pc_size) = layout(PROVIDER_CERT, ps);
    states()
        .values()
        .find(|s| s.signer_at == signer && (index as usize) < s.cert_count)
        .map_or(0, |s| s.certs_at + index as usize * pc_size)
}

pub unsafe extern "win64" fn CryptCATAdminAcquireContext(
    admin: *mut usize,
    _subsystem: usize,
    _flags: u32,
) -> i32 {
    if !admin.is_null() {
        *admin = 0;
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinVerifyTrust" => fn WinVerifyTrust,
        "WinVerifyTrustEx" => fn WinVerifyTrustEx,
        "WTHelperProvDataFromStateData" => fn WTHelperProvDataFromStateData,
        "WTHelperGetProvSignerFromChain" => fn WTHelperGetProvSignerFromChain,
        "WTHelperGetProvCertFromChain" => fn WTHelperGetProvCertFromChain,
        "CryptCATAdminAcquireContext" => fn CryptCATAdminAcquireContext,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispositions_x86_et_x64() {
        assert_eq!(layout(WINTRUST_DATA, 8).1, 88);
        assert_eq!(layout(WINTRUST_DATA, 4).1, 52);
        assert_eq!(layout(PROVIDER_CERT, 8).0[1], 8);
        assert_eq!(layout(PROVIDER_CERT, 4).0[1], 4);
        let (sg, _) = layout(PROVIDER_SGNR, 8);
        assert_eq!((sg[1], sg[2], sg[3]), (4, 12, 16));
    }

    /// Vérification avec état d'un .exe signé, puis d'un .exe non signé.
    #[test]
    fn verification_avec_etat() {
        let _teb = crate::process::test_teb();
        let Some(signed) = crate::setup::appx_signature::tests::signed_test_pe() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("winlux-wvt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("signe.exe");
        std::fs::write(&exe, &signed).unwrap();
        let wide: Vec<u16> = format!("Z:{}", exe.display().to_string().replace('/', "\\"))
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            let mut file = [0u64; 4];
            *(file.as_mut_ptr() as *mut u32) = 32;
            file[1] = wide.as_ptr() as u64;
            let mut settings = [0u32; 8];
            settings[0] = 32;
            settings[2] = WSS_GET_SECONDARY_SIG_COUNT | WSS_VERIFY_SPECIFIC;
            settings[3] = 7;
            let (w, size) = layout(WINTRUST_DATA, 8);
            let mut wd = vec![0u64; size / 8];
            let d = wd.as_mut_ptr() as usize;
            put32(d, 0, size as u32);
            put32(d, w[3], 2); // WTD_UI_NONE
            put32(d, w[WD_UNION_CHOICE], WTD_CHOICE_FILE);
            put_ptr(d, w[WD_FILE], file.as_ptr() as usize);
            put32(d, w[WD_STATE_ACTION], WTD_STATEACTION_VERIFY);
            put_ptr(d, w[WD_SIGNATURE_SETTINGS], settings.as_ptr() as usize);
            let guid = [0u8; 16];
            // Certificat de test auto-signé : autorité non reconnue, mais
            // l'état est là, comme sous Windows.
            let status = WinVerifyTrust(usize::MAX, guid.as_ptr() as usize, d);
            assert_eq!(status as u32, CERT_E_UNTRUSTEDROOT);
            assert_eq!(settings[3], 0, "aucune signature secondaire");
            let state = crate::wow32::read_ptr(d + w[WD_STATE]);
            assert_ne!(state, 0);
            let prov = WTHelperProvDataFromStateData(state);
            assert_eq!(prov, state);
            let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
            assert_ne!(sgnr, 0);
            assert_eq!(WTHelperGetProvSignerFromChain(prov, 1, 0, 0), 0);
            let cert = WTHelperGetProvCertFromChain(sgnr, 0);
            assert_ne!(cert, 0);
            assert_eq!(WTHelperGetProvCertFromChain(sgnr, 1), 0);
            let ctx = crate::wow32::read_ptr(cert + 8);
            assert_ne!(ctx, 0);
            // Empreinte SHA-1 du certificat, comme la calcule CEF.
            let mut sha1 = [0u8; 20];
            let mut n = 20u32;
            assert_eq!(
                crate::dlls::crypt32::CertGetCertificateContextProperty(
                    ctx,
                    3, // CERT_SHA1_HASH_PROP_ID
                    sha1.as_mut_ptr().cast(),
                    &mut n
                ),
                1
            );
            assert_eq!(n, 20);
            put32(d, w[WD_STATE_ACTION], WTD_STATEACTION_CLOSE);
            assert_eq!(WinVerifyTrust(usize::MAX, guid.as_ptr() as usize, d), 0);
            assert_eq!(crate::wow32::read_ptr(d + w[WD_STATE]), 0);
            assert_eq!(WTHelperProvDataFromStateData(state), 0);
            // Fichier non signé : TRUST_E_NOSIGNATURE, sans état.
            let plain = dir.join("nu.exe");
            std::fs::write(&plain, b"MZ pas un PE").unwrap();
            let wide2: Vec<u16> = format!("Z:{}", plain.display().to_string().replace('/', "\\"))
                .encode_utf16()
                .chain([0])
                .collect();
            file[1] = wide2.as_ptr() as u64;
            put32(d, w[WD_STATE_ACTION], WTD_STATEACTION_VERIFY);
            let status = WinVerifyTrust(usize::MAX, guid.as_ptr() as usize, d);
            assert_eq!(status as u32, TRUST_E_NOSIGNATURE);
            assert_eq!(crate::dlls::kernel32::get_last_error(), TRUST_E_NOSIGNATURE);
            assert_eq!(crate::wow32::read_ptr(d + w[WD_STATE]), 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
