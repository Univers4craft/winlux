//! Imports de discord_utils.node, le module natif de Discord (#1823),
//! relevés au banc : msi (installation d'un paquet, niveau d'interface),
//! shell32 (état des notifications), shlwapi (wnsprintf), gdi32 (priorité
//! d'ordonnancement GPU) et crypt32 (décodage d'un message PKCS #7).
//!
//! Sources : documentation publique de Microsoft pour chaque fonction ;
//! numéros d'ordre de msi lus dans Msi.Lib du SDK 10.0.26100.4654.

#![allow(non_snake_case)]

use crate::process::set_last_error;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

// --- msi ----------------------------------------------------------------

const INSTALLUILEVEL_NOCHANGE: u32 = 0;
const INSTALLUILEVEL_DEFAULT: u32 = 1;
const INSTALLUILEVEL_NONE: u32 = 2;
const INSTALLUILEVEL_BASIC: u32 = 3;
const INSTALLUILEVEL_REDUCED: u32 = 4;
const INSTALLUILEVEL_FULL: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSTALL_SERVICE_FAILURE: u32 = 1601;

static UI_LEVEL: AtomicU32 = AtomicU32::new(INSTALLUILEVEL_BASIC);
static UI_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// MsiSetInternalUI(dwUILevel, phWnd) : rend le niveau précédent ;
/// INSTALLUILEVEL_NOCHANGE ne fait que le lire. `phWnd` (entrée-sortie)
/// échange la fenêtre parente.
unsafe extern "win64" fn MsiSetInternalUI(level: u32, window: *mut usize) -> u32 {
    let old = UI_LEVEL.load(Ordering::Relaxed);
    // Bits de modification (INSTALLUILEVEL_ENDDIALOG…) au-dessus de 0xFF.
    let base = level & 0xff;
    if !matches!(
        base,
        INSTALLUILEVEL_NOCHANGE
            | INSTALLUILEVEL_DEFAULT
            | INSTALLUILEVEL_NONE
            | INSTALLUILEVEL_BASIC
            | INSTALLUILEVEL_REDUCED
            | INSTALLUILEVEL_FULL
    ) {
        return 0; // INSTALLUILEVEL_NOCHANGE : niveau refusé
    }
    if base != INSTALLUILEVEL_NOCHANGE && base != INSTALLUILEVEL_DEFAULT {
        UI_LEVEL.store(level, Ordering::Relaxed);
    }
    if !window.is_null() {
        let previous = UI_WINDOW.swap(*window, Ordering::Relaxed);
        *window = previous;
    }
    old
}

/// Découpe une ligne de propriétés (« A=1 B="x y" ») en mots.
fn split_command_line(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(cur);
    }
    words
}

/// Arguments du msiexec de Winlux pour MsiInstallProduct.
fn msiexec_args(package: &str, command_line: &str, level: u32) -> Vec<String> {
    let mut args = vec!["/i".to_string(), package.to_string()];
    match level & 0xff {
        INSTALLUILEVEL_NONE => args.push("/qn".into()),
        INSTALLUILEVEL_BASIC => args.push("/qb".into()),
        INSTALLUILEVEL_REDUCED => args.push("/qr".into()),
        _ => {}
    }
    args.extend(split_command_line(command_line));
    args
}

/// MsiInstallProductW(szPackagePath, szCommandLine) : installation par le
/// msiexec de Winlux (même moteur que le double-clic), au niveau
/// d'interface choisi par MsiSetInternalUI ; rend son code de sortie.
unsafe extern "win64" fn MsiInstallProductW(package: *const u16, command_line: *const u16) -> u32 {
    if package.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let package = crate::dlls::wstr(package);
    let line = if command_line.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(command_line)
    };
    let args = msiexec_args(&package, &line, UI_LEVEL.load(Ordering::Relaxed));
    let Ok(exe) = std::env::current_exe() else {
        return ERROR_INSTALL_SERVICE_FAILURE;
    };
    match std::process::Command::new(exe)
        .arg("--msiexec")
        .args(&args)
        .status()
    {
        Ok(s) => s.code().map_or(ERROR_INSTALL_SERVICE_FAILURE, |c| c as u32),
        Err(_) => ERROR_INSTALL_SERVICE_FAILURE,
    }
}

// --- shell32 ------------------------------------------------------------

/// QUERY_USER_NOTIFICATION_STATE : QUNS_ACCEPTS_NOTIFICATIONS.
const QUNS_ACCEPTS_NOTIFICATIONS: u32 = 5;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

/// SHQueryUserNotificationState : le bureau Winlux n'a ni mode
/// présentation ni « ne pas déranger » ; il accepte les notifications.
unsafe extern "win64" fn SHQueryUserNotificationState(state: *mut u32) -> i32 {
    if state.is_null() {
        return E_INVALIDARG;
    }
    *state = QUNS_ACCEPTS_NOTIFICATIONS;
    0
}

// --- shlwapi ------------------------------------------------------------

/// Copie bornée, toujours terminée : -1 si le texte a été tronqué.
unsafe fn write_bounded<T: Copy + Default>(dst: *mut T, size: usize, text: &[T]) -> i32 {
    if dst.is_null() || size == 0 {
        return -1;
    }
    let n = text.len().min(size - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), dst, n);
    *dst.add(n) = T::default();
    if n < text.len() {
        -1
    } else {
        n as i32
    }
}

/// wnsprintfW(pszDest, cchDest, pszFmt, ...).
unsafe extern "win64" fn imp_wnsprintf_w(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let dst = v.next_u64() as *mut u16;
    let size = v.next_u64() as usize;
    let fmt = v.next_ptr::<u16>();
    let w = crate::dlls::crt::format_wide(fmt, &mut v);
    write_bounded(dst, size, &w)
}

/// wnsprintfA(pszDest, cchDest, pszFmt, ...).
unsafe extern "win64" fn imp_wnsprintf_a(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let dst = v.next_u64() as *mut u8;
    let size = v.next_u64() as usize;
    let fmt = v.next_ptr::<u8>();
    let b = crate::dlls::crt_stdio::format_narrow(fmt, &mut v);
    write_bounded(dst, size, &b)
}

/// wvnsprintfW(pszDest, cchDest, pszFmt, va_list).
unsafe extern "win64" fn wvnsprintf_w(
    dst: *mut u16,
    size: i32,
    fmt: *const u16,
    va: *const u64,
) -> i32 {
    let mut v = crate::thunks::VaList::new(va);
    let w = crate::dlls::crt::format_wide(fmt, &mut v);
    write_bounded(dst, size.max(0) as usize, &w)
}

/// wvnsprintfA(pszDest, cchDest, pszFmt, va_list).
unsafe extern "win64" fn wvnsprintf_a(
    dst: *mut u8,
    size: i32,
    fmt: *const u8,
    va: *const u64,
) -> i32 {
    let mut v = crate::thunks::VaList::new(va);
    let b = crate::dlls::crt_stdio::format_narrow(fmt, &mut v);
    write_bounded(dst, size.max(0) as usize, &b)
}

crate::varargs_thunk!(wnsprintf_w_thunk => imp_wnsprintf_w);
crate::varargs_thunk!(wnsprintf_a_thunk => imp_wnsprintf_a);

// --- gdi32 --------------------------------------------------------------

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
/// D3DKMT_SCHEDULINGPRIORITYCLASS_NORMAL.
const PRIORITY_NORMAL: i32 = 2;

/// D3DKMTSetProcessSchedulingPriorityClass : le pilote graphique Linux
/// n'expose pas de classe de priorité par processus.
unsafe extern "win64" fn D3DKMTSetProcessSchedulingPriorityClass(
    _process: usize,
    class: i32,
) -> i32 {
    if !(0..=5).contains(&class) {
        return STATUS_INVALID_PARAMETER;
    }
    STATUS_NOT_SUPPORTED
}

/// D3DKMTGetProcessSchedulingPriorityClass : toujours la classe normale,
/// puisqu'elle ne peut pas être changée.
unsafe extern "win64" fn D3DKMTGetProcessSchedulingPriorityClass(
    _process: usize,
    class: *mut i32,
) -> i32 {
    if class.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    *class = PRIORITY_NORMAL;
    STATUS_SUCCESS
}

/// Ouverture d'un adaptateur du noyau graphique Windows (par LUID, HDC ou
/// nom GDI) : sous Winlux, le pilote est celui de Linux et il n'y a aucun
/// adaptateur de noyau Windows à ouvrir. Chromium (processus GPU et
/// utilitaires de Discord) s'arrêtait en 0xC0000139 sur l'entrée absente ;
/// un échec NTSTATUS est son chemin prévu (#1920).
unsafe extern "win64" fn D3DKMTOpenAdapter(desc: *mut u8) -> i32 {
    if desc.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    STATUS_NOT_SUPPORTED
}

/// D3DKMTEnumAdapters2 : aucun adaptateur (NumAdapters = 0).
unsafe extern "win64" fn D3DKMTEnumAdapters2(desc: *mut u32) -> i32 {
    if desc.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    *desc = 0;
    STATUS_SUCCESS
}

/// Opération sur un handle d'adaptateur (D3DKMT_HANDLE en tête de la
/// structure) : aucun n'a pu être ouvert, tous sont inconnus.
unsafe extern "win64" fn D3DKMTUnknownAdapter(_desc: *mut u8) -> i32 {
    STATUS_INVALID_PARAMETER
}

/// D3DKMTQueryStatistics : pas de statistiques du noyau graphique Windows.
unsafe extern "win64" fn D3DKMTQueryStatistics(desc: *mut u8) -> i32 {
    if desc.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    STATUS_NOT_SUPPORTED
}

// --- crypt32 ------------------------------------------------------------

const CMSG_SIGNED: u32 = 2;
const CMSG_TYPE_PARAM: u32 = 1;
const CMSG_CONTENT_PARAM: u32 = 2;
const CMSG_SIGNER_COUNT_PARAM: u32 = 5;
const CMSG_CERT_COUNT_PARAM: u32 = 11;
const CMSG_CERT_PARAM: u32 = 12;
const CRYPT_E_INVALID_MSG_TYPE: u32 = 0x8009_1010;
const CRYPT_E_STREAM_MSG_NOT_READY: u32 = 0x8009_1011;
const CRYPT_E_ASN1_BADTAG: u32 = 0x8009_3104;
const CRYPT_E_INVALID_INDEX: u32 = 0x8009_1008;
const E_NOTIMPL: u32 = 0x8000_4001;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_INVALID_HANDLE: u32 = 6;

#[derive(Default)]
struct Msg {
    data: Vec<u8>,
    decoded: Option<(Vec<Vec<u8>>, Vec<u8>)>,
}

fn messages() -> &'static Mutex<HashMap<usize, Msg>> {
    static M: OnceLock<Mutex<HashMap<usize, Msg>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_MSG: AtomicUsize = AtomicUsize::new(0x4d53_0000);

fn lock() -> std::sync::MutexGuard<'static, HashMap<usize, Msg>> {
    messages().lock().unwrap_or_else(|e| e.into_inner())
}

/// CryptMsgOpenToDecode(encoding, flags, msgType, prov, recipient,
/// stream) : message à remplir par CryptMsgUpdate. Le mode flux (rappels)
/// n'est pas pris en charge.
unsafe extern "win64" fn CryptMsgOpenToDecode(
    _encoding: u32,
    _flags: u32,
    msg_type: u32,
    _prov: usize,
    _recipient: usize,
    stream: *const c_void,
) -> usize {
    if !stream.is_null() {
        set_last_error(E_NOTIMPL);
        return 0;
    }
    if msg_type != 0 && msg_type != CMSG_SIGNED {
        set_last_error(CRYPT_E_INVALID_MSG_TYPE);
        return 0;
    }
    let h = NEXT_MSG.fetch_add(8, Ordering::Relaxed);
    lock().insert(h, Msg::default());
    h
}

/// CryptMsgUpdate(msg, data, len, final) : le message est décodé quand le
/// dernier morceau arrive.
unsafe extern "win64" fn CryptMsgUpdate(msg: usize, data: *const u8, len: u32, last: i32) -> i32 {
    let mut map = lock();
    let Some(m) = map.get_mut(&msg) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if !data.is_null() && len > 0 {
        m.data
            .extend_from_slice(std::slice::from_raw_parts(data, len as usize));
    }
    if last != 0 {
        m.decoded = crate::setup::appx_signature::signed_data_parts(&m.data);
        if m.decoded.is_none() {
            set_last_error(CRYPT_E_ASN1_BADTAG);
            return 0;
        }
    }
    1
}

/// Le handle vient-il de CryptMsgOpenToDecode ?
pub(crate) fn is_decode_msg(msg: usize) -> bool {
    lock().contains_key(&msg)
}

pub(crate) fn close_msg(msg: usize) -> bool {
    lock().remove(&msg).is_some()
}

unsafe fn give(bytes: &[u8], data: *mut c_void, len: *mut u32) -> i32 {
    let need = bytes.len() as u32;
    if data.is_null() {
        *len = need;
        return 1;
    }
    if *len < need {
        *len = need;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len());
    *len = need;
    1
}

/// CryptMsgGetParam d'un message décodé : type, contenu, nombre de
/// signataires et de certificats, certificat n.
pub(crate) unsafe fn get_param(
    msg: usize,
    ty: u32,
    index: u32,
    data: *mut c_void,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let map = lock();
    let Some(m) = map.get(&msg) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    let Some((certs, content)) = &m.decoded else {
        set_last_error(CRYPT_E_STREAM_MSG_NOT_READY);
        return 0;
    };
    match ty {
        CMSG_TYPE_PARAM => give(&CMSG_SIGNED.to_le_bytes(), data, len),
        CMSG_CONTENT_PARAM => give(content, data, len),
        CMSG_SIGNER_COUNT_PARAM => give(&1u32.to_le_bytes(), data, len),
        CMSG_CERT_COUNT_PARAM => give(&(certs.len() as u32).to_le_bytes(), data, len),
        CMSG_CERT_PARAM => match certs.get(index as usize) {
            Some(c) => give(c, data, len),
            None => {
                set_last_error(CRYPT_E_INVALID_INDEX);
                0
            }
        },
        _ => {
            set_last_error(CRYPT_E_INVALID_MSG_TYPE);
            0
        }
    }
}

/// msi : ajoutée à la DLL elle-même, créée après le complément général.
pub fn msi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "MsiSetInternalUI" => fn MsiSetInternalUI,
        "MsiInstallProductW" => fn MsiInstallProductW,
        // Numéros d'ordre de Msi.Lib (SDK 10.0.26100.4654).
        "#88" => fn MsiInstallProductW,
        "#141" => fn MsiSetInternalUI,
    }
}

pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        (
            "shell32",
            crate::exports! {
                "SHQueryUserNotificationState" => fn SHQueryUserNotificationState,
            },
        ),
        (
            "shlwapi",
            crate::exports! {
                "wnsprintfW" => fn wnsprintf_w_thunk,
                "wnsprintfA" => fn wnsprintf_a_thunk,
                "wvnsprintfW" => fn wvnsprintf_w,
                "wvnsprintfA" => fn wvnsprintf_a,
            },
        ),
        (
            "gdi32",
            crate::exports! {
                "D3DKMTSetProcessSchedulingPriorityClass" => fn D3DKMTSetProcessSchedulingPriorityClass,
                "D3DKMTGetProcessSchedulingPriorityClass" => fn D3DKMTGetProcessSchedulingPriorityClass,
                "D3DKMTOpenAdapterFromLuid" => fn D3DKMTOpenAdapter,
                "D3DKMTOpenAdapterFromHdc" => fn D3DKMTOpenAdapter,
                "D3DKMTOpenAdapterFromGdiDisplayName" => fn D3DKMTOpenAdapter,
                "D3DKMTEnumAdapters2" => fn D3DKMTEnumAdapters2,
                "D3DKMTCloseAdapter" => fn D3DKMTUnknownAdapter,
                "D3DKMTQueryAdapterInfo" => fn D3DKMTUnknownAdapter,
                "D3DKMTQueryVideoMemoryInfo" => fn D3DKMTUnknownAdapter,
                "D3DKMTQueryStatistics" => fn D3DKMTQueryStatistics,
            },
        ),
        (
            "crypt32",
            crate::exports! {
                "CryptMsgOpenToDecode" => fn CryptMsgOpenToDecode,
                "CryptMsgUpdate" => fn CryptMsgUpdate,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_de_discord_utils() {
        unsafe {
            // MsiSetInternalUI : niveau précédent rendu, NOCHANGE ne change rien.
            let before = MsiSetInternalUI(INSTALLUILEVEL_NONE, std::ptr::null_mut());
            assert_eq!(
                MsiSetInternalUI(INSTALLUILEVEL_NOCHANGE, std::ptr::null_mut()),
                INSTALLUILEVEL_NONE
            );
            MsiSetInternalUI(before, std::ptr::null_mut());
            assert_eq!(
                msiexec_args("C:\\a.msi", "A=1 B=\"x y\"", INSTALLUILEVEL_NONE),
                ["/i", "C:\\a.msi", "/qn", "A=1", "B=x y"]
            );
            // msi importée par numéro d'ordre.
            assert!(crate::dlls::lookup("msi", "#88").is_some());
            assert!(crate::dlls::lookup("msi", "#141").is_some());
            let mut st = 0;
            assert_eq!(SHQueryUserNotificationState(&mut st), 0);
            assert_eq!(st, QUNS_ACCEPTS_NOTIFICATIONS);
            assert_eq!(
                D3DKMTSetProcessSchedulingPriorityClass(0, 9),
                STATUS_INVALID_PARAMETER
            );
            let mut c = 0;
            assert_eq!(D3DKMTGetProcessSchedulingPriorityClass(0, &mut c), 0);
            assert_eq!(c, PRIORITY_NORMAL);
            // #1920 : pas d'adaptateur de noyau Windows, échecs NTSTATUS.
            for name in [
                "D3DKMTOpenAdapterFromLuid",
                "D3DKMTOpenAdapterFromHdc",
                "D3DKMTCloseAdapter",
                "D3DKMTQueryAdapterInfo",
                "D3DKMTEnumAdapters2",
            ] {
                assert!(crate::dlls::lookup("gdi32", name).is_some(), "{name}");
            }
            let mut luid = [0u8; 12];
            assert_eq!(D3DKMTOpenAdapter(luid.as_mut_ptr()), STATUS_NOT_SUPPORTED);
            let mut enum2 = [7u32, 0, 0, 0];
            assert_eq!(D3DKMTEnumAdapters2(enum2.as_mut_ptr()), STATUS_SUCCESS);
            assert_eq!(enum2[0], 0);
            assert_eq!(
                D3DKMTUnknownAdapter(luid.as_mut_ptr()),
                STATUS_INVALID_PARAMETER
            );
            // Copie bornée de wnsprintf.
            let mut buf = [0xffu16; 4];
            assert_eq!(write_bounded(buf.as_mut_ptr(), 4, &[1, 2]), 2);
            assert_eq!(&buf[..3], &[1, 2, 0]);
            assert_eq!(write_bounded(buf.as_mut_ptr(), 2, &[1, 2, 3]), -1);
            assert_eq!(&buf[..2], &[1, 0]);
        }
    }

    #[test]
    fn message_pkcs7_decode() {
        let Some(pe) = crate::setup::appx_signature::tests::signed_test_pe() else {
            return; // OpenSSL absent : rien à signer
        };
        // Table des certificats : WIN_CERTIFICATE (8 octets) + PKCS #7.
        let sig = {
            let e = u32::from_le_bytes(pe[0x3c..0x40].try_into().unwrap()) as usize;
            // IMAGE_DIRECTORY_ENTRY_SECURITY : 5e entrée du répertoire, qui
            // commence à 96 (PE32) ou 112 (PE32+) dans l'en-tête optionnel.
            let pe32_plus = pe[e + 24..e + 26] == [0x0b, 0x02];
            let dir = e + 24 + if pe32_plus { 112 } else { 96 } + 4 * 8;
            let at = u32::from_le_bytes(pe[dir..dir + 4].try_into().unwrap()) as usize;
            let len = u32::from_le_bytes(pe[at..at + 4].try_into().unwrap()) as usize;
            pe[at + 8..at + len].to_vec()
        };
        unsafe {
            let m = CryptMsgOpenToDecode(0x10001, 0, 0, 0, 0, std::ptr::null());
            assert_ne!(m, 0);
            let mut n = 0u32;
            assert_eq!(
                get_param(m, CMSG_TYPE_PARAM, 0, std::ptr::null_mut(), &mut n),
                0
            );
            let half = sig.len() / 2;
            assert_eq!(CryptMsgUpdate(m, sig.as_ptr(), half as u32, 0), 1);
            assert_eq!(
                CryptMsgUpdate(m, sig[half..].as_ptr(), (sig.len() - half) as u32, 1),
                1
            );
            let mut ty = 0u32;
            let mut len = 4u32;
            assert_eq!(
                get_param(
                    m,
                    CMSG_TYPE_PARAM,
                    0,
                    (&mut ty as *mut u32).cast(),
                    &mut len
                ),
                1
            );
            assert_eq!(ty, CMSG_SIGNED);
            let mut count = 0u32;
            assert_eq!(
                get_param(
                    m,
                    CMSG_CERT_COUNT_PARAM,
                    0,
                    (&mut count as *mut u32).cast(),
                    &mut len
                ),
                1
            );
            assert!(count >= 1);
            let mut need = 0u32;
            assert_eq!(
                get_param(m, CMSG_CERT_PARAM, 0, std::ptr::null_mut(), &mut need),
                1
            );
            assert!(need > 100);
            assert!(close_msg(m));
            assert!(!is_decode_msg(m));
        }
    }
}
