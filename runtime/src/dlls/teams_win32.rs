//! Imports de Microsoft Teams (ms-teams.exe et ses DLL, paquet MSIX)
//! que Winlux ne servait pas (#2110), relevés dans le journal du banc :
//! kernel32 (paquets, rapport d'erreurs, récupération, port d'achèvement
//! du pool), user32 (ShowWindowAsync, IsGUIThread,
//! CalculatePopupWindowPosition), shlwapi (flux et URL), ucrt
//! (set_terminate, _byteswap_*), advapi32, iphlpapi, propsys et shell32.
//!
//! Les groupes de nettoyage et SetThreadpoolTimerEx vivent dans
//! threadpool.rs, SafeArrayCopy dans oleaut32.rs.
//!
//! Sources : documentation publique de Microsoft pour chaque fonction
//! (MicrosoftDocs/sdk-api) ; numéros d'ordre lus dans OleAut32.Lib et
//! ShLwApi.Lib du SDK Windows public (NuGet Microsoft.Windows.SDK.CPP.x64
//! 10.0.26100.9457), voir common_apps::ORDINALS.

#![allow(non_snake_case)]

use crate::process::set_last_error;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::OnceLock;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NOT_FOUND: u32 = 1168;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

unsafe fn wstr(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(p)
    }
}

/// Copie `text` (sans zéro final) dans `out` de `*len` caractères, à la
/// manière des fonctions d'URL de shlwapi : en cas de succès `*len` reçoit
/// le nombre de caractères écrits sans le zéro, sinon E_POINTER et la
/// taille nécessaire, zéro compris.
unsafe fn copy_out_hr(text: &[u16], out: *mut u16, len: *mut u32, ok: i32) -> i32 {
    if len.is_null() {
        return E_INVALIDARG;
    }
    if out.is_null() || (*len as usize) < text.len() + 1 {
        *len = text.len() as u32 + 1;
        return E_POINTER;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
    *out.add(text.len()) = 0;
    *len = text.len() as u32;
    ok
}

// ============================================================
//  kernel32
// ============================================================

/// GetProcessIdOfThread : un thread de ce processus (Winlux ne rend pas de
/// poignée sur le thread d'un autre programme).
unsafe extern "win64" fn GetProcessIdOfThread(thread: usize) -> u32 {
    let current = thread == usize::MAX - 1;
    if current || crate::dlls::kernel32::thread_id_of_handle(thread).is_some() {
        return crate::dlls::kernel32::get_current_process_id();
    }
    set_last_error(ERROR_INVALID_HANDLE);
    0
}

/// RECOVERY_MAX_PING_INTERVAL : 5 minutes.
const RECOVERY_MAX_PING_INTERVAL: u32 = 5 * 60 * 1000;

/// RegisterApplicationRecoveryCallback : Winlux n'a pas de rapport
/// d'erreurs Windows qui rappellerait le programme après un plantage ;
/// l'inscription est acceptée (le journal des plantages de Winlux reste
/// la trace), comme sous Windows quand le service ne s'en sert pas.
unsafe extern "win64" fn RegisterApplicationRecoveryCallback(
    callback: usize,
    _parameter: usize,
    ping_interval: u32,
    _flags: u32,
) -> i32 {
    if callback == 0 || ping_interval > RECOVERY_MAX_PING_INTERVAL {
        return E_INVALIDARG;
    }
    S_OK
}

extern "win64" fn UnregisterApplicationRecoveryCallback() -> i32 {
    S_OK
}

/// WerRegisterFile, WerRegisterCustomMetadata,
/// WerRegisterAdditionalProcess : données à joindre à un rapport d'erreurs
/// Windows, que Winlux n'envoie pas. Les paramètres sont vérifiés comme la
/// documentation l'indique.
unsafe extern "win64" fn WerRegisterFile(path: *const u16, _kind: u32, _flags: u32) -> i32 {
    if path.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

unsafe extern "win64" fn WerRegisterCustomMetadata(key: *const u16, value: *const u16) -> i32 {
    if key.is_null() || value.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

extern "win64" fn WerRegisterAdditionalProcess(_pid: u32, _thread: u32) -> i32 {
    S_OK
}

/// Dossier des paquets MSIX installés (`C:\Program Files\WindowsApps`).
fn windows_apps() -> std::path::PathBuf {
    crate::paths::to_linux("C:\\Program Files\\WindowsApps")
}

/// Paquets installés : (nom complet, nom de famille, dossier Windows).
fn installed_packages() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(windows_apps()) else {
        return out;
    };
    for entry in dir.flatten() {
        let full = entry.file_name().to_string_lossy().into_owned();
        let Ok(xml) = std::fs::read_to_string(entry.path().join("AppxManifest.xml")) else {
            continue;
        };
        let Ok(pkg) = crate::setup::msix::Package::parse(&xml) else {
            continue;
        };
        if pkg.full_name().eq_ignore_ascii_case(&full) {
            out.push((
                pkg.full_name(),
                pkg.family_name(),
                format!("C:\\Program Files\\WindowsApps\\{full}"),
            ));
        }
    }
    out.sort();
    out
}

/// GetPackagePathByFullName : dossier d'un paquet installé.
unsafe extern "win64" fn GetPackagePathByFullName(
    name: *const u16,
    len: *mut u32,
    path: *mut u16,
) -> u32 {
    if name.is_null() || len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let name = wstr(name);
    let Some((_, _, dir)) = installed_packages()
        .into_iter()
        .find(|(full, _, _)| full.eq_ignore_ascii_case(&name))
    else {
        return ERROR_NOT_FOUND;
    };
    let w: Vec<u16> = dir.encode_utf16().chain(std::iter::once(0)).collect();
    if path.is_null() || (*len as usize) < w.len() {
        *len = w.len() as u32;
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), path, w.len());
    *len = w.len() as u32;
    0
}

/// FindPackagesByPackageFamily : noms complets des paquets installés d'une
/// famille. Deux appels : le premier donne le nombre de noms et de
/// caractères, le second remplit le tableau de pointeurs et le tampon.
unsafe extern "win64" fn FindPackagesByPackageFamily(
    family: *const u16,
    _filters: u32,
    count: *mut u32,
    full_names: *mut *mut u16,
    buffer_len: *mut u32,
    buffer: *mut u16,
    properties: *mut u32,
) -> u32 {
    if family.is_null() || count.is_null() || buffer_len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let family = wstr(family);
    let names: Vec<Vec<u16>> = installed_packages()
        .into_iter()
        .filter(|(_, fam, _)| fam.eq_ignore_ascii_case(&family))
        .map(|(full, _, _)| full.encode_utf16().chain(std::iter::once(0)).collect())
        .collect();
    let chars: usize = names.iter().map(Vec::len).sum();
    let enough = (*count as usize) >= names.len() && (*buffer_len as usize) >= chars;
    let want = !full_names.is_null() || !buffer.is_null();
    *count = names.len() as u32;
    *buffer_len = chars as u32;
    if names.is_empty() {
        return 0;
    }
    if !want || !enough || full_names.is_null() || buffer.is_null() {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    let mut at = buffer;
    for (i, n) in names.iter().enumerate() {
        std::ptr::copy_nonoverlapping(n.as_ptr(), at, n.len());
        *full_names.add(i) = at;
        if !properties.is_null() {
            *properties.add(i) = 0;
        }
        at = at.add(n.len());
    }
    0
}

/// LPOVERLAPPED_COMPLETION_ROUTINE(dwErrorCode, dwBytes, lpOverlapped).
type IoCompletion = unsafe extern "win64" fn(u32, u32, *mut c_void);

/// Port d'achèvement du pool de BindIoCompletionCallback et ses deux
/// threads Windows, créés au premier appel.
fn bind_port() -> usize {
    static PORT: OnceLock<usize> = OnceLock::new();
    *PORT.get_or_init(|| {
        let port = crate::dlls::kernel32::create_io_completion_port(usize::MAX, 0, 0, 0);
        if port != 0 {
            for n in 0..2 {
                let _ = std::thread::Builder::new()
                    .name(format!("win-pool-io-{n}"))
                    .spawn(move || io_worker(port));
            }
        }
        port
    })
}

fn io_worker(port: usize) {
    crate::process::thread_attach_current();
    loop {
        let mut bytes = 0u32;
        let mut key = 0usize;
        let mut overlapped: *mut c_void = std::ptr::null_mut();
        let ok = unsafe {
            crate::dlls::kernel32::get_queued_completion_status(
                port,
                &mut bytes,
                &mut key,
                &mut overlapped,
                u32::MAX,
            )
        };
        if overlapped.is_null() || key == 0 {
            if ok == 0 {
                // Port fermé ou erreur sans opération : on s'arrête.
                return;
            }
            continue;
        }
        let error = if ok != 0 {
            0
        } else {
            unsafe { api!("kernel32", "GetLastError", fn() -> u32).unwrap_or(0) }
        };
        let callback: IoCompletion = unsafe { std::mem::transmute(key) };
        unsafe { callback(error, bytes, overlapped) };
    }
}

/// BindIoCompletionCallback : la fin de chaque opération asynchrone sur
/// `file` appelle `function` sur un thread du pool.
unsafe extern "win64" fn BindIoCompletionCallback(file: usize, function: usize, flags: u32) -> i32 {
    if function == 0 || flags != 0 || crate::wow32::active() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let port = bind_port();
    if port == 0 {
        return 0;
    }
    i32::from(crate::dlls::kernel32::create_io_completion_port(file, port, function, 0) != 0)
}

// ============================================================
//  user32
// ============================================================

/// Message privé : ShowWindowAsync rejoué sur le thread de la fenêtre.
pub const WM_WINLUX_SHOW_ASYNC: u32 = 0x0001_0D0E;

/// ShowWindowAsync : sur le thread de la fenêtre, comme ShowWindow ;
/// depuis un autre thread, la demande part dans la file de la fenêtre et
/// la fonction rend la main sans attendre.
unsafe extern "win64" fn ShowWindowAsync(hwnd: usize, cmd: i32) -> i32 {
    let mine = crate::dlls::gui::window_thread_id(hwnd)
        .is_none_or(|t| t == crate::dlls::kernel32::get_current_thread_id());
    if mine {
        crate::dlls::user32::show_window(hwnd, cmd);
        return 1;
    }
    let msg = crate::dlls::gui::Msg::new(hwnd, WM_WINLUX_SHOW_ASYNC, cmd as u32 as usize, 0);
    i32::from(crate::dlls::gui::post_to_window(hwnd, msg))
}

/// Appelé par DispatchMessage : vrai si le message était privé.
pub fn dispatch_private(hwnd: usize, message: u32, wparam: usize) -> bool {
    if message != WM_WINLUX_SHOW_ASYNC {
        return false;
    }
    unsafe { crate::dlls::user32::show_window(hwnd, wparam as u32 as i32) };
    true
}

/// IsGUIThread : vrai si le thread a une file de messages ; `convert`
/// lui en donne une.
extern "win64" fn IsGUIThread(convert: i32) -> i32 {
    if convert != 0 {
        let _ = crate::dlls::gui::current_queue();
        return 1;
    }
    i32::from(crate::dlls::gui::current_thread_has_queue())
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

const TPM_CENTERALIGN: u32 = 0x0004;
const TPM_RIGHTALIGN: u32 = 0x0008;
const TPM_VCENTERALIGN: u32 = 0x0010;
const TPM_BOTTOMALIGN: u32 = 0x0020;
const TPM_VERTICAL: u32 = 0x0040;
const TPM_WORKAREA: u32 = 0x10000;

/// Écran de l'ancre (zone de travail avec TPM_WORKAREA).
unsafe fn monitor_area(x: i32, y: i32, work: bool) -> Rect {
    let pt = (x as u32 as u64) | ((y as u32 as u64) << 32);
    // MONITOR_DEFAULTTONEAREST
    if let Some(mon) = api!("user32", "MonitorFromPoint", fn(u64, u32) -> usize, pt, 2) {
        let mut info = [0u32; 10];
        info[0] = 40;
        if mon != 0
            && api!(
                "user32",
                "GetMonitorInfoW",
                fn(usize, *mut u32) -> i32,
                mon,
                info.as_mut_ptr()
            ) == Some(1)
        {
            let at = if work { 5 } else { 1 };
            return Rect {
                left: info[at] as i32,
                top: info[at + 1] as i32,
                right: info[at + 2] as i32,
                bottom: info[at + 3] as i32,
            };
        }
    }
    let r = crate::dlls::gui::work_area();
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// Position d'une fenêtre surgissante (règles de TrackPopupMenuEx) :
/// alignée sur l'ancre, gardée entière sur l'écran, et poussée hors du
/// rectangle exclu, verticalement avec TPM_VERTICAL, sinon horizontalement.
fn popup_position(
    anchor: (i32, i32),
    size: (i32, i32),
    flags: u32,
    exclude: Option<Rect>,
    screen: Rect,
) -> Rect {
    let (w, h) = size;
    let mut x = if flags & TPM_RIGHTALIGN != 0 {
        anchor.0 - w
    } else if flags & TPM_CENTERALIGN != 0 {
        anchor.0 - w / 2
    } else {
        anchor.0
    };
    let mut y = if flags & TPM_BOTTOMALIGN != 0 {
        anchor.1 - h
    } else if flags & TPM_VCENTERALIGN != 0 {
        anchor.1 - h / 2
    } else {
        anchor.1
    };
    let clamp = |v: i32, lo: i32, hi: i32, len: i32| v.min(hi - len).max(lo);
    x = clamp(x, screen.left, screen.right, w);
    y = clamp(y, screen.top, screen.bottom, h);
    if let Some(ex) = exclude {
        let r = Rect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        if overlaps(&r, &ex) {
            if flags & TPM_VERTICAL != 0 {
                // Dessous si la place le permet, sinon dessus.
                y = if ex.bottom + h <= screen.bottom || ex.top - h < screen.top {
                    ex.bottom
                } else {
                    ex.top - h
                };
                y = clamp(y, screen.top, screen.bottom, h);
            } else {
                x = if ex.right + w <= screen.right || ex.left - w < screen.left {
                    ex.right
                } else {
                    ex.left - w
                };
                x = clamp(x, screen.left, screen.right, w);
            }
        }
    }
    Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

unsafe extern "win64" fn CalculatePopupWindowPosition(
    anchor: *const i32,
    size: *const i32,
    flags: u32,
    exclude: *const Rect,
    out: *mut Rect,
) -> i32 {
    if anchor.is_null() || size.is_null() || out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let a = (*anchor, *anchor.add(1));
    let s = (*size, *size.add(1));
    let screen = monitor_area(a.0, a.1, flags & TPM_WORKAREA != 0);
    *out = popup_position(a, s, flags, exclude.as_ref().copied(), screen);
    1
}

// ============================================================
//  shlwapi : flux
// ============================================================

/// Appel d'une méthode de la table virtuelle d'un objet COM 64 bits.
unsafe fn vcall(obj: usize, slot: usize) -> usize {
    *(*(obj as *const *const usize)).add(slot)
}

/// SHCreateMemStream (#12) : flux en mémoire initialisé par une copie de
/// `init` ; NULL en cas d'échec.
unsafe extern "win64" fn SHCreateMemStream(init: *const u8, len: u32) -> usize {
    let size = (len as usize).max(1);
    // GMEM_MOVEABLE
    let Some(h) = api!("kernel32", "GlobalAlloc", fn(u32, usize) -> usize, 2, size) else {
        return 0;
    };
    if h == 0 {
        return 0;
    }
    if !init.is_null() && len > 0 {
        let p =
            api!("kernel32", "GlobalLock", fn(usize) -> *mut u8, h).unwrap_or(std::ptr::null_mut());
        if p.is_null() {
            api!("kernel32", "GlobalFree", fn(usize) -> usize, h);
            return 0;
        }
        std::ptr::copy_nonoverlapping(init, p, len as usize);
        api!("kernel32", "GlobalUnlock", fn(usize) -> i32, h);
    }
    let mut stream = 0usize;
    let hr = api!(
        "ole32",
        "CreateStreamOnHGlobal",
        fn(usize, i32, *mut usize) -> i32,
        h,
        1,
        &mut stream
    )
    .unwrap_or(E_FAIL);
    if hr < 0 || stream == 0 {
        api!("kernel32", "GlobalFree", fn(usize) -> usize, h);
        return 0;
    }
    // La taille du flux est celle des données, pas celle du bloc.
    let set_size: unsafe extern "win64" fn(usize, u64) -> i32 =
        std::mem::transmute(vcall(stream, 6));
    set_size(stream, u64::from(len));
    stream
}

/// IStream_Read (#184) : E_FAIL si moins de `cb` octets ont été lus.
unsafe extern "win64" fn IStream_Read(stream: usize, buf: *mut u8, cb: u32) -> i32 {
    if stream == 0 {
        return E_INVALIDARG;
    }
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(vcall(stream, 3));
    let mut got = 0u32;
    let hr = read(stream, buf, cb, &mut got);
    if hr < 0 {
        return hr;
    }
    if got == cb {
        S_OK
    } else {
        E_FAIL
    }
}

/// IStream_Reset (#213) : retour au début du flux.
unsafe extern "win64" fn IStream_Reset(stream: usize) -> i32 {
    if stream == 0 {
        return E_INVALIDARG;
    }
    let seek: unsafe extern "win64" fn(usize, i64, u32, *mut u64) -> i32 =
        std::mem::transmute(vcall(stream, 5));
    seek(stream, 0, 0, std::ptr::null_mut())
}

/// IStream_Size (#214) : taille par IStream::Stat (STATFLAG_NONAME).
unsafe extern "win64" fn IStream_Size(stream: usize, size: *mut u64) -> i32 {
    if stream == 0 || size.is_null() {
        return E_INVALIDARG;
    }
    let stat: unsafe extern "win64" fn(usize, *mut u8, u32) -> i32 =
        std::mem::transmute(vcall(stream, 12));
    let mut st = [0u8; 80];
    let hr = stat(stream, st.as_mut_ptr(), 1);
    if hr >= 0 {
        *size = std::ptr::read_unaligned(st.as_ptr().add(16) as *const u64);
    }
    hr
}

/// SHGlobalCounterIncrement (#633) : compteurs SHGLOBALCOUNTER, propres
/// à la session de ce processus.
unsafe extern "win64" fn SHGlobalCounterIncrement(id: u32) -> i32 {
    static COUNTERS: [AtomicI32; 64] = [const { AtomicI32::new(0) }; 64];
    match COUNTERS.get(id as usize) {
        Some(c) => c.fetch_add(1, Ordering::SeqCst) + 1,
        None => 0,
    }
}

// ============================================================
//  shlwapi : URL
// ============================================================

const URL_ESCAPE_PERCENT: u32 = 0x0000_1000;
const URL_ESCAPE_SEGMENT_ONLY: u32 = 0x0000_2000;
const URL_ESCAPE_AS_UTF8: u32 = 0x0004_0000;
const URL_ESCAPE_ASCII_URI_COMPONENT: u32 = 0x0008_0000;
const URL_DONT_ESCAPE_EXTRA_INFO: u32 = 0x0200_0000;
const URL_ESCAPE_SPACES_ONLY: u32 = 0x0400_0000;
const URL_DONT_UNESCAPE_EXTRA_INFO: u32 = 0x0200_0000;
const URL_UNESCAPE_INPLACE: u32 = 0x0010_0000;
const URL_UNESCAPE_AS_UTF8: u32 = 0x0004_0000;

/// Caractères « dangereux » de la documentation d'UrlEscape, plus
/// l'espace et les caractères de contrôle.
fn unsafe_char(c: u16) -> bool {
    c <= 0x20
        || c == 0x7F
        || matches!(
            c as u8 as char,
            '^' | '&' | '`' | '{' | '}' | '|' | ']' | '[' | '"' | '<' | '>' | '\\'
        ) && c < 0x80
}

fn push_escaped(out: &mut Vec<u16>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(u16::from(b'%'));
    out.push(u16::from(HEX[(byte >> 4) as usize]));
    out.push(u16::from(HEX[(byte & 15) as usize]));
}

/// Fin de la partie « serveur » d'une URL complète (`schéma://hôte/`,
/// barre finale comprise), sinon 0.
fn server_end(s: &[u16]) -> usize {
    let text = String::from_utf16_lossy(s);
    let Some(sep) = text.find("://") else {
        return 0;
    };
    if !text[..sep]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return 0;
    }
    let after = sep + 3;
    let end = text[after..]
        .find('/')
        .map_or(text.len(), |i| after + i + 1);
    // Positions en caractères UTF-16.
    text[..end].encode_utf16().count()
}

pub(crate) fn url_escape(src: &[u16], flags: u32) -> Vec<u16> {
    let mut out = Vec::with_capacity(src.len());
    let spaces_only = flags & URL_ESCAPE_SPACES_ONLY != 0;
    let segment_only = flags & URL_ESCAPE_SEGMENT_ONLY != 0;
    let component = flags & URL_ESCAPE_ASCII_URI_COMPONENT != 0;
    let start = if spaces_only || segment_only || component {
        0
    } else {
        server_end(src)
    };
    out.extend_from_slice(&src[..start]);
    let mut in_query = false;
    let mut i = start;
    while i < src.len() {
        let c = src[i];
        if !segment_only && !component && (c == u16::from(b'?') || c == u16::from(b'#')) {
            in_query = true;
        }
        let escape = if spaces_only {
            c == 0x20 && !(in_query && flags & URL_DONT_ESCAPE_EXTRA_INFO != 0)
        } else if in_query {
            false
        } else if component {
            c < 0x80 && !(c as u8).is_ascii_alphanumeric() && !b"-.~_".contains(&(c as u8))
        } else if c == u16::from(b'%') {
            flags & URL_ESCAPE_PERCENT != 0
        } else if segment_only
            && (c == u16::from(b'#') || c == u16::from(b'?') || c == u16::from(b'/'))
        {
            true
        } else {
            unsafe_char(c)
        };
        if escape {
            push_escaped(&mut out, c as u8);
            i += 1;
            continue;
        }
        if c >= 0x80 && !in_query && !spaces_only && flags & URL_ESCAPE_AS_UTF8 != 0 {
            // Caractère (ou paire de substitution) en UTF-8, octet par octet.
            let n = if (0xD800..0xDC00).contains(&c) && i + 1 < src.len() {
                2
            } else {
                1
            };
            let s = String::from_utf16_lossy(&src[i..i + n]);
            for b in s.bytes() {
                push_escaped(&mut out, b);
            }
            i += n;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

unsafe extern "win64" fn UrlEscapeW(
    url: *const u16,
    out: *mut u16,
    len: *mut u32,
    flags: u32,
) -> i32 {
    if url.is_null() || len.is_null() {
        return E_INVALIDARG;
    }
    let src = wide(&wstr(url));
    copy_out_hr(&url_escape(&src, flags), out, len, S_OK)
}

fn hex(c: u16) -> Option<u8> {
    char::from_u32(u32::from(c))?.to_digit(16).map(|d| d as u8)
}

pub(crate) fn url_unescape(src: &[u16], flags: u32) -> Vec<u16> {
    let stop = if flags & URL_DONT_UNESCAPE_EXTRA_INFO != 0 {
        src.iter()
            .position(|&c| c == u16::from(b'?') || c == u16::from(b'#'))
            .unwrap_or(src.len())
    } else {
        src.len()
    };
    let mut out = Vec::with_capacity(src.len());
    let mut bytes: Vec<u8> = Vec::new();
    let flush = |bytes: &mut Vec<u8>, out: &mut Vec<u16>| {
        if bytes.is_empty() {
            return;
        }
        if flags & URL_UNESCAPE_AS_UTF8 != 0 {
            out.extend(String::from_utf8_lossy(bytes).encode_utf16());
        } else {
            out.extend(bytes.iter().map(|&b| u16::from(b)));
        }
        bytes.clear();
    };
    let mut i = 0;
    while i < stop {
        if src[i] == u16::from(b'%') && i + 2 < stop + 1 && i + 2 < src.len() {
            if let (Some(h), Some(l)) = (hex(src[i + 1]), hex(src[i + 2])) {
                if i + 2 < stop {
                    bytes.push(h << 4 | l);
                    i += 3;
                    continue;
                }
            }
        }
        flush(&mut bytes, &mut out);
        out.push(src[i]);
        i += 1;
    }
    flush(&mut bytes, &mut out);
    out.extend_from_slice(&src[stop..]);
    out
}

unsafe extern "win64" fn UrlUnescapeW(
    url: *mut u16,
    out: *mut u16,
    len: *mut u32,
    flags: u32,
) -> i32 {
    if url.is_null() {
        return E_INVALIDARG;
    }
    let src = wide(&wstr(url));
    let text = url_unescape(&src, flags);
    if flags & URL_UNESCAPE_INPLACE != 0 {
        // Le résultat n'est jamais plus long que l'entrée.
        std::ptr::copy_nonoverlapping(text.as_ptr(), url, text.len());
        *url.add(text.len()) = 0;
        return S_OK;
    }
    copy_out_hr(&text, out, len, S_OK)
}

const URL_PART_SCHEME: u32 = 1;
const URL_PART_HOSTNAME: u32 = 2;
const URL_PART_USERNAME: u32 = 3;
const URL_PART_PASSWORD: u32 = 4;
const URL_PART_PORT: u32 = 5;
const URL_PART_QUERY: u32 = 6;
const URL_PARTFLAG_KEEPSCHEME: u32 = 1;

/// Partie d'une URL `schéma://utilisateur:mot@hôte:port/chemin?requête#ancre`.
/// None : l'URL n'a pas de schéma ; Some("") : la partie est absente.
pub(crate) fn url_part(url: &str, part: u32) -> Option<(String, String)> {
    let colon = url.find(':')?;
    let scheme = &url[..colon];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    let rest = &url[colon + 1..];
    let query = rest
        .split_once('?')
        .map(|(_, q)| q.split('#').next().unwrap_or("").to_string())
        .unwrap_or_default();
    let authority = rest
        .strip_prefix("//")
        .map(|a| a.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (u, h),
        None => ("", authority),
    };
    let (user, pass) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let (host, port) = if hostport.starts_with('[') {
        match hostport.find(']') {
            Some(e) => (
                &hostport[..=e],
                hostport[e + 1..].strip_prefix(':').unwrap_or(""),
            ),
            None => (hostport, ""),
        }
    } else {
        hostport.rsplit_once(':').unwrap_or((hostport, ""))
    };
    let value = match part {
        URL_PART_SCHEME => scheme,
        URL_PART_HOSTNAME => host,
        URL_PART_USERNAME => user,
        URL_PART_PASSWORD => pass,
        URL_PART_PORT => port,
        URL_PART_QUERY => &query,
        _ => return None,
    };
    Some((scheme.to_string(), value.to_string()))
}

unsafe extern "win64" fn UrlGetPartW(
    url: *const u16,
    out: *mut u16,
    len: *mut u32,
    part: u32,
    flags: u32,
) -> i32 {
    if url.is_null() || len.is_null() || !(URL_PART_SCHEME..=URL_PART_QUERY).contains(&part) {
        return E_INVALIDARG;
    }
    let Some((scheme, value)) = url_part(&wstr(url), part) else {
        return E_FAIL;
    };
    if value.is_empty() {
        if !out.is_null() && *len > 0 {
            *out = 0;
        }
        *len = 0;
        return S_FALSE;
    }
    let text = if flags & URL_PARTFLAG_KEEPSCHEME != 0 && part != URL_PART_SCHEME {
        format!("{scheme}:{value}")
    } else {
        value
    };
    copy_out_hr(&wide(&text), out, len, S_OK)
}

/// URL `file:` d'un chemin MS-DOS : `C:\a b\c.txt` → `file:///C:/a%20b/c.txt`,
/// `\\serveur\partage\x` → `file://serveur/partage/x`. None si le chemin
/// est déjà une URL.
pub(crate) fn url_from_path(path: &str) -> Option<String> {
    if path.contains("://") || path.to_ascii_lowercase().starts_with("file:") {
        return None;
    }
    let (prefix, body) = match path.strip_prefix("\\\\") {
        Some(unc) => ("file://", unc),
        None => ("file:///", path),
    };
    let mut out = String::from(prefix);
    for c in body.chars() {
        match c {
            '\\' => out.push('/'),
            ' ' | '%' | '#' | '^' | '`' | '{' | '}' | '|' | '[' | ']' | '"' | '<' | '>' => {
                out.push_str(&format!("%{:02X}", c as u32))
            }
            c => out.push(c),
        }
    }
    Some(out)
}

unsafe extern "win64" fn UrlCreateFromPathW(
    path: *const u16,
    url: *mut u16,
    len: *mut u32,
    _reserved: u32,
) -> i32 {
    if path.is_null() || len.is_null() {
        return E_INVALIDARG;
    }
    let p = wstr(path);
    if p.starts_with("\\\\?\\") {
        return E_INVALIDARG;
    }
    match url_from_path(&p) {
        Some(u) => copy_out_hr(&wide(&u), url, len, S_OK),
        None => copy_out_hr(&wide(&p), url, len, S_FALSE),
    }
}

// ============================================================
//  oleaut32
// ============================================================

/// RegisterTypeLibForUser (#442) : inscription de la bibliothèque de types,
/// dans le même registre que RegisterTypeLib.
unsafe extern "win64" fn RegisterTypeLibForUser(
    lib: usize,
    path: *const u16,
    help: *const u16,
) -> i32 {
    crate::dlls::oleaut32::register_type_lib(lib, path, help as usize)
}

// ============================================================
//  ucrt
// ============================================================

static TERMINATE_HANDLER: AtomicUsize = AtomicUsize::new(0);

/// Fonction installée par set_terminate, appelée par terminate().
pub fn terminate_handler() -> Option<unsafe extern "win64" fn()> {
    match TERMINATE_HANDLER.load(Ordering::SeqCst) {
        0 => None,
        f => Some(unsafe { std::mem::transmute::<usize, unsafe extern "win64" fn()>(f) }),
    }
}

/// set_terminate : installe la fonction appelée par terminate() et rend
/// la précédente.
extern "win64" fn set_terminate(handler: usize) -> usize {
    TERMINATE_HANDLER.swap(handler, Ordering::SeqCst)
}

extern "win64" fn _get_terminate() -> usize {
    TERMINATE_HANDLER.load(Ordering::SeqCst)
}

extern "win64" fn _byteswap_ushort(v: u16) -> u16 {
    v.swap_bytes()
}

extern "win64" fn _byteswap_ulong(v: u32) -> u32 {
    v.swap_bytes()
}

extern "win64" fn _byteswap_uint64(v: u64) -> u64 {
    v.swap_bytes()
}

// ============================================================
//  advapi32
// ============================================================

/// AddConditionalAce : une ACE conditionnelle porte une expression SDDL
/// compilée (MS-DTYP 2.4.4.17) que Winlux ne sait pas encore produire.
/// Échec documenté plutôt qu'une ACE fausse.
unsafe extern "win64" fn AddConditionalAce(
    _acl: usize,
    _revision: u32,
    _flags: u32,
    _ace_type: u8,
    _mask: u32,
    _sid: usize,
    _condition: *const u16,
    _return_length: *mut u32,
) -> i32 {
    crate::log_warn!(
        "advapi32",
        "AddConditionalAce : ACE conditionnelle non prise en charge"
    );
    set_last_error(ERROR_NOT_SUPPORTED);
    0
}

// ============================================================
//  iphlpapi
// ============================================================

/// GetIpNetTable2 : Winlux n'expose pas la table des voisins (ARP/NDP) ;
/// la documentation prévoit ERROR_NOT_FOUND quand elle est vide.
unsafe extern "win64" fn GetIpNetTable2(_family: u16, table: *mut *mut c_void) -> u32 {
    if table.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *table = std::ptr::null_mut();
    ERROR_NOT_FOUND
}

/// IcmpSendEcho2 : l'envoi d'IcmpSendEcho ; avec un évènement, celui-ci
/// est signalé à la fin et la fonction rend 0 avec ERROR_IO_PENDING, comme
/// un appel asynchrone. Une routine APC sans évènement n'est pas prise en
/// charge.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn IcmpSendEcho2(
    icmp: usize,
    event: usize,
    apc: usize,
    _apc_context: usize,
    destination: u32,
    data: *const u8,
    data_len: u16,
    options: usize,
    reply: *mut u8,
    reply_len: u32,
    timeout: u32,
) -> u32 {
    if event == 0 && apc != 0 {
        set_last_error(ERROR_NOT_SUPPORTED);
        return 0;
    }
    let n = api!(
        "iphlpapi",
        "IcmpSendEcho",
        fn(usize, u32, *const u8, u16, usize, *mut u8, u32, u32) -> u32,
        icmp,
        destination,
        data,
        data_len,
        options,
        reply,
        reply_len,
        timeout
    )
    .unwrap_or(0);
    if event != 0 {
        api!("kernel32", "SetEvent", fn(usize) -> i32, event);
        set_last_error(997); // ERROR_IO_PENDING
        return 0;
    }
    n
}

// ============================================================
//  propsys
// ============================================================

/// InitPropVariantFromCLSID : PROPVARIANT VT_CLSID, GUID copié dans un bloc
/// CoTaskMemAlloc que PropVariantClear libère.
unsafe extern "win64" fn InitPropVariantFromCLSID(clsid: *const u8, pv: *mut u8) -> i32 {
    if clsid.is_null() || pv.is_null() {
        return E_INVALIDARG;
    }
    let mem =
        api!("ole32", "CoTaskMemAlloc", fn(usize) -> *mut u8, 16).unwrap_or(std::ptr::null_mut());
    if mem.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(clsid, mem, 16);
    std::ptr::write_bytes(pv, 0, 24);
    std::ptr::write_unaligned(pv as *mut u16, 72); // VT_CLSID
    std::ptr::write_unaligned(pv.add(8) as *mut *mut u8, mem);
    S_OK
}

// ============================================================
//  shell32 : IApplicationAssociationRegistration
// ============================================================

use super::comobj::{guid, Guid, VTable, IID_IUNKNOWN};

const IID_IAPPLICATION_ASSOCIATION_REGISTRATION: Guid = guid(
    0x4e53_0b0a,
    0xe611,
    0x4c77,
    [0xa3, 0xac, 0x90, 0x31, 0xd0, 0x22, 0x28, 0x1b],
);
/// HRESULT_FROM_WIN32(ERROR_NO_ASSOCIATION).
const E_NO_ASSOCIATION: i32 = 0x8007_0483u32 as i32;

#[repr(C)]
struct AssocReg {
    vtbl: *const VTable<9>,
}
unsafe impl Sync for AssocReg {}

static ASSOC_REG: AssocReg = AssocReg {
    vtbl: &ASSOC_REG_VTBL,
};
static ASSOC_REG_VTBL: VTable<9> = VTable([
    ar_query_interface as *const (),
    ar_add_ref as *const (),
    ar_release as *const (),
    ar_query_current_default as *const (),
    ar_query_app_is_default as *const (),
    ar_query_app_is_default_all as *const (),
    ar_set_app_as_default as *const (),
    ar_set_app_as_default_all as *const (),
    ar_clear_user_associations as *const (),
]);

unsafe extern "win64" fn ar_query_interface(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match super::comobj::read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IAPPLICATION_ASSOCIATION_REGISTRATION => {
            *out = this;
            S_OK
        }
        _ => {
            *out = 0;
            E_NOINTERFACE
        }
    }
}
extern "win64" fn ar_add_ref(_this: usize) -> u32 {
    1
}
extern "win64" fn ar_release(_this: usize) -> u32 {
    1
}
/// QueryCurrentDefault : Winlux ne tient pas de choix d'application par
/// défaut à la Windows pour les extensions et les protocoles.
unsafe extern "win64" fn ar_query_current_default(
    _this: usize,
    _query: *const u16,
    _kind: u32,
    _level: u32,
    out: *mut *mut u16,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NO_ASSOCIATION
}
unsafe extern "win64" fn ar_query_app_is_default(
    _this: usize,
    _query: *const u16,
    _kind: u32,
    _level: u32,
    _app: *const u16,
    is_default: *mut i32,
) -> i32 {
    if is_default.is_null() {
        return E_POINTER;
    }
    *is_default = 0;
    S_OK
}
unsafe extern "win64" fn ar_query_app_is_default_all(
    _this: usize,
    _level: u32,
    _app: *const u16,
    is_default: *mut i32,
) -> i32 {
    if is_default.is_null() {
        return E_POINTER;
    }
    *is_default = 0;
    S_OK
}
/// SetAppAsDefault / SetAppAsDefaultAll : Windows 8 et suivants ne laissent
/// plus un programme se déclarer par défaut lui-même ; refusé.
extern "win64" fn ar_set_app_as_default(_this: usize) -> i32 {
    E_NOTIMPL
}
extern "win64" fn ar_set_app_as_default_all(_this: usize) -> i32 {
    E_NOTIMPL
}
extern "win64" fn ar_clear_user_associations(_this: usize) -> i32 {
    S_OK
}

unsafe extern "win64" fn SHCreateAssociationRegistration(iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    ar_query_interface(&ASSOC_REG as *const AssocReg as usize, iid, out)
}

// ============================================================
//  Tables d'exports
// ============================================================

pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        (
            "kernel32",
            crate::exports! {
                "GetProcessIdOfThread" => fn GetProcessIdOfThread,
                "RegisterApplicationRecoveryCallback" => fn RegisterApplicationRecoveryCallback,
                "UnregisterApplicationRecoveryCallback" => fn UnregisterApplicationRecoveryCallback,
                "WerRegisterFile" => fn WerRegisterFile,
                "WerRegisterCustomMetadata" => fn WerRegisterCustomMetadata,
                "WerRegisterAdditionalProcess" => fn WerRegisterAdditionalProcess,
                "GetPackagePathByFullName" => fn GetPackagePathByFullName,
                "FindPackagesByPackageFamily" => fn FindPackagesByPackageFamily,
                "BindIoCompletionCallback" => fn BindIoCompletionCallback,
            },
        ),
        (
            "user32",
            crate::exports! {
                "ShowWindowAsync" => fn ShowWindowAsync,
                "IsGUIThread" => fn IsGUIThread,
                "CalculatePopupWindowPosition" => fn CalculatePopupWindowPosition,
            },
        ),
        (
            "shlwapi",
            crate::exports! {
                "SHCreateMemStream" => fn SHCreateMemStream,
                "IStream_Read" => fn IStream_Read,
                "IStream_Reset" => fn IStream_Reset,
                "IStream_Size" => fn IStream_Size,
                "SHGlobalCounterIncrement" => fn SHGlobalCounterIncrement,
                "UrlEscapeW" => fn UrlEscapeW,
                "UrlUnescapeW" => fn UrlUnescapeW,
                "UrlGetPartW" => fn UrlGetPartW,
                "UrlCreateFromPathW" => fn UrlCreateFromPathW,
            },
        ),
        (
            "oleaut32",
            crate::exports! {
                "RegisterTypeLibForUser" => fn RegisterTypeLibForUser,
            },
        ),
        (
            "ucrtbase",
            crate::exports! {
                "set_terminate" => fn set_terminate,
                "_get_terminate" => fn _get_terminate,
                "_byteswap_ushort" => fn _byteswap_ushort,
                "_byteswap_ulong" => fn _byteswap_ulong,
                "_byteswap_uint64" => fn _byteswap_uint64,
            },
        ),
        (
            "advapi32",
            crate::exports! {
                "AddConditionalAce" => fn AddConditionalAce,
                // Contrat api-ms-win-eventing-provider, servi par advapi32.
                "EventActivityIdControl" => fn crate::dlls::sechost::EventActivityIdControl,
            },
        ),
        (
            "iphlpapi",
            crate::exports! {
                "GetIpNetTable2" => fn GetIpNetTable2,
                "IcmpSendEcho2" => fn IcmpSendEcho2,
            },
        ),
        (
            "propsys",
            crate::exports! {
                "InitPropVariantFromCLSID" => fn InitPropVariantFromCLSID,
            },
        ),
        (
            "shell32",
            crate::exports! {
                "SHCreateAssociationRegistration" => fn SHCreateAssociationRegistration,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        wide(s)
    }
    fn s(v: &[u16]) -> String {
        String::from_utf16_lossy(v)
    }

    /// Exemples de la documentation d'UrlEscape.
    #[test]
    fn url_escape_exemples_documentes() {
        let src = "test/t%e<s t.asp?url=/{ex% ample</abc.asp?frame=true#fr%agment";
        assert_eq!(
            s(&url_escape(&w(src), URL_ESCAPE_SPACES_ONLY)),
            "test/t%e<s%20t.asp?url=/{ex%%20ample</abc.asp?frame=true#fr%agment"
        );
        assert_eq!(
            s(&url_escape(
                &w(src),
                URL_ESCAPE_SPACES_ONLY | URL_DONT_ESCAPE_EXTRA_INFO
            )),
            "test/t%e<s%20t.asp?url=/{ex% ample</abc.asp?frame=true#fr%agment"
        );
        assert_eq!(
            s(&url_escape(&w(src), URL_ESCAPE_PERCENT)),
            "test/t%25e%3Cs%20t.asp?url=/{ex% ample</abc.asp?frame=true#fr%agment"
        );
        assert_eq!(
            s(&url_escape(&w("test/t%e<s t.asp"), URL_ESCAPE_SEGMENT_ONLY)),
            "test%2Ft%e%3Cs%20t.asp"
        );
        // La partie serveur d'une URL complète reste telle quelle.
        assert_eq!(
            s(&url_escape(&w("http://exemple.fr/a b/c{d}.txt?x y"), 0)),
            "http://exemple.fr/a%20b/c%7Bd%7D.txt?x y"
        );
        assert_eq!(s(&url_escape(&w("é/a"), URL_ESCAPE_AS_UTF8)), "%C3%A9/a");
        assert_eq!(
            s(&url_escape(&w("a b/c~d"), URL_ESCAPE_ASCII_URI_COMPONENT)),
            "a%20b%2Fc~d"
        );
    }

    #[test]
    fn url_unescape_et_utf8() {
        assert_eq!(s(&url_unescape(&w("a%20b%2Fc"), 0)), "a b/c");
        assert_eq!(
            s(&url_unescape(&w("%C3%A9t%C3%A9"), URL_UNESCAPE_AS_UTF8)),
            "été"
        );
        assert_eq!(
            s(&url_unescape(
                &w("a%20b?q=%20"),
                URL_DONT_UNESCAPE_EXTRA_INFO
            )),
            "a b?q=%20"
        );
        // Séquence incomplète : gardée telle quelle.
        assert_eq!(s(&url_unescape(&w("100%"), 0)), "100%");
        assert_eq!(s(&url_unescape(&w("%zz"), 0)), "%zz");
    }

    #[test]
    fn url_unescape_en_place_et_tampon() {
        unsafe {
            let mut buf = w("a%20b");
            buf.push(0);
            assert_eq!(
                UrlUnescapeW(
                    buf.as_mut_ptr(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    URL_UNESCAPE_INPLACE
                ),
                S_OK
            );
            assert_eq!(wstr(buf.as_ptr()), "a b");
            let mut src = w("x%41y");
            src.push(0);
            let mut out = [0u16; 2];
            let mut len = 2u32;
            assert_eq!(
                UrlUnescapeW(src.as_mut_ptr(), out.as_mut_ptr(), &mut len, 0),
                E_POINTER
            );
            assert_eq!(len, 4);
            let mut out = [0u16; 8];
            let mut len = 8u32;
            assert_eq!(
                UrlUnescapeW(src.as_mut_ptr(), out.as_mut_ptr(), &mut len, 0),
                S_OK
            );
            assert_eq!(len, 3);
            assert_eq!(wstr(out.as_ptr()), "xAy");
        }
    }

    #[test]
    fn url_parties() {
        let u = "https://moi:secret@teams.example.com:8443/v2/a?x=1&y=2#ancre";
        let part = |p| url_part(u, p).unwrap().1;
        assert_eq!(part(URL_PART_SCHEME), "https");
        assert_eq!(part(URL_PART_HOSTNAME), "teams.example.com");
        assert_eq!(part(URL_PART_USERNAME), "moi");
        assert_eq!(part(URL_PART_PASSWORD), "secret");
        assert_eq!(part(URL_PART_PORT), "8443");
        assert_eq!(part(URL_PART_QUERY), "x=1&y=2");
        assert_eq!(
            url_part("http://[::1]:80/", URL_PART_HOSTNAME).unwrap().1,
            "[::1]"
        );
        assert_eq!(url_part("pas une url", URL_PART_SCHEME), None);
        unsafe {
            let mut src = w(u);
            src.push(0);
            let mut out = [0u16; 64];
            let mut len = 64u32;
            assert_eq!(
                UrlGetPartW(
                    src.as_ptr(),
                    out.as_mut_ptr(),
                    &mut len,
                    URL_PART_HOSTNAME,
                    URL_PARTFLAG_KEEPSCHEME
                ),
                S_OK
            );
            assert_eq!(wstr(out.as_ptr()), "https:teams.example.com");
            let mut src = w("http://hote/");
            src.push(0);
            let mut len = 64u32;
            assert_eq!(
                UrlGetPartW(src.as_ptr(), out.as_mut_ptr(), &mut len, URL_PART_PORT, 0),
                S_FALSE
            );
            assert_eq!(len, 0);
        }
    }

    #[test]
    fn url_depuis_un_chemin() {
        assert_eq!(
            url_from_path("C:\\a b\\c#d.txt").unwrap(),
            "file:///C:/a%20b/c%23d.txt"
        );
        assert_eq!(
            url_from_path("\\\\serveur\\partage\\x").unwrap(),
            "file://serveur/partage/x"
        );
        assert_eq!(url_from_path("http://x/"), None);
        unsafe {
            let mut src = w("file:///C:/x");
            src.push(0);
            let mut out = [0u16; 32];
            let mut len = 32u32;
            assert_eq!(
                UrlCreateFromPathW(src.as_ptr(), out.as_mut_ptr(), &mut len, 0),
                S_FALSE
            );
            assert_eq!(wstr(out.as_ptr()), "file:///C:/x");
        }
    }

    #[test]
    fn position_des_surgissantes() {
        let screen = Rect {
            left: 0,
            top: 0,
            right: 1366,
            bottom: 768,
        };
        // Alignée à gauche et en haut sur l'ancre.
        assert_eq!(
            popup_position((100, 100), (200, 50), 0, None, screen),
            Rect {
                left: 100,
                top: 100,
                right: 300,
                bottom: 150
            }
        );
        // Gardée entière sur l'écran.
        assert_eq!(
            popup_position((1300, 750), (200, 50), 0, None, screen),
            Rect {
                left: 1166,
                top: 718,
                right: 1366,
                bottom: 768
            }
        );
        // Alignée à droite et en bas.
        assert_eq!(
            popup_position(
                (500, 500),
                (200, 50),
                TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
                None,
                screen
            ),
            Rect {
                left: 300,
                top: 450,
                right: 500,
                bottom: 500
            }
        );
        // Poussée sous le rectangle exclu (TPM_VERTICAL).
        let ex = Rect {
            left: 90,
            top: 90,
            right: 160,
            bottom: 120,
        };
        assert_eq!(
            popup_position((100, 100), (200, 50), TPM_VERTICAL, Some(ex), screen),
            Rect {
                left: 100,
                top: 120,
                right: 300,
                bottom: 170
            }
        );
        // Sinon à sa droite.
        assert_eq!(
            popup_position((100, 100), (200, 50), 0, Some(ex), screen),
            Rect {
                left: 160,
                top: 100,
                right: 360,
                bottom: 150
            }
        );
    }

    #[test]
    fn echanges_d_octets_et_terminate() {
        assert_eq!(_byteswap_ulong(0x1122_3344), 0x4433_2211);
        assert_eq!(
            _byteswap_uint64(0x0102_0304_0506_0708),
            0x0807_0605_0403_0201
        );
        assert_eq!(_byteswap_ushort(0xABCD), 0xCDAB);
        let old = set_terminate(0x1234);
        assert_eq!(_get_terminate(), 0x1234);
        assert_eq!(set_terminate(old), 0x1234);
    }

    #[test]
    fn compteurs_globaux() {
        unsafe {
            let a = SHGlobalCounterIncrement(7);
            assert_eq!(SHGlobalCounterIncrement(7), a + 1);
            assert_eq!(SHGlobalCounterIncrement(10_000), 0);
        }
    }

    #[test]
    fn paquets_installes_par_famille() {
        let _teb = crate::process::test_teb();
        let dir = std::env::temp_dir().join(format!("winlux-2110-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old_c = std::env::var_os("WINLUX_DRIVE_C");
        std::env::set_var("WINLUX_DRIVE_C", &dir);
        let manifest = r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
  <Identity Name="Winlux.Essai" Publisher="CN=Winlux Test" Version="1.2.3.0" ProcessorArchitecture="x64" />
  <Properties><DisplayName>Essai</DisplayName><PublisherDisplayName>Winlux</PublisherDisplayName></Properties>
  <Applications><Application Id="Essai" Executable="essai.exe" /></Applications>
</Package>"#;
        let pkg = crate::setup::msix::Package::parse(manifest).unwrap();
        let full = pkg.full_name();
        let family = pkg.family_name();
        let pkg_dir = windows_apps().join(&full);
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("AppxManifest.xml"), manifest).unwrap();
        unsafe {
            let mut fam = w(&family);
            fam.push(0);
            let (mut count, mut chars) = (0u32, 0u32);
            let r = FindPackagesByPackageFamily(
                fam.as_ptr(),
                0x10,
                &mut count,
                std::ptr::null_mut(),
                &mut chars,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            assert_eq!(r, ERROR_INSUFFICIENT_BUFFER);
            assert_eq!(count, 1);
            assert_eq!(chars as usize, full.len() + 1);
            let mut names = vec![std::ptr::null_mut::<u16>(); count as usize];
            let mut buf = vec![0u16; chars as usize];
            let mut props = vec![9u32; count as usize];
            let r = FindPackagesByPackageFamily(
                fam.as_ptr(),
                0x10,
                &mut count,
                names.as_mut_ptr(),
                &mut chars,
                buf.as_mut_ptr(),
                props.as_mut_ptr(),
            );
            assert_eq!(r, 0);
            assert_eq!(wstr(names[0]), full);
            assert_eq!(props[0], 0);

            let mut name = w(&full);
            name.push(0);
            let mut len = 0u32;
            assert_eq!(
                GetPackagePathByFullName(name.as_ptr(), &mut len, std::ptr::null_mut()),
                ERROR_INSUFFICIENT_BUFFER
            );
            let mut path = vec![0u16; len as usize];
            assert_eq!(
                GetPackagePathByFullName(name.as_ptr(), &mut len, path.as_mut_ptr()),
                0
            );
            assert_eq!(
                wstr(path.as_ptr()),
                format!("C:\\Program Files\\WindowsApps\\{full}")
            );
            let mut other = w("Autre_1.0.0.0_x64__abc");
            other.push(0);
            assert_eq!(
                GetPackagePathByFullName(other.as_ptr(), &mut len, path.as_mut_ptr()),
                ERROR_NOT_FOUND
            );
        }
        match old_c {
            Some(v) => std::env::set_var("WINLUX_DRIVE_C", v),
            None => std::env::remove_var("WINLUX_DRIVE_C"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn association_par_defaut_sans_choix() {
        unsafe {
            let mut obj = 0usize;
            let iid = IID_IAPPLICATION_ASSOCIATION_REGISTRATION;
            assert_eq!(
                SHCreateAssociationRegistration(iid.as_ptr(), &mut obj),
                S_OK
            );
            assert_ne!(obj, 0);
            let mut def = 1i32;
            assert_eq!(
                ar_query_app_is_default_all(obj, 1, std::ptr::null(), &mut def),
                S_OK
            );
            assert_eq!(def, 0);
            let other = guid(1, 2, 3, [0; 8]);
            assert_eq!(
                SHCreateAssociationRegistration(other.as_ptr(), &mut obj),
                E_NOINTERFACE
            );
        }
    }

    #[test]
    fn propvariant_clsid() {
        unsafe {
            let g = guid(0x1122_3344, 0x5566, 0x7788, [1, 2, 3, 4, 5, 6, 7, 8]);
            let mut pv = [0u8; 24];
            assert_eq!(InitPropVariantFromCLSID(g.as_ptr(), pv.as_mut_ptr()), S_OK);
            assert_eq!(u16::from_le_bytes([pv[0], pv[1]]), 72);
            let p = std::ptr::read_unaligned(pv.as_ptr().add(8) as *const *const u8);
            assert_eq!(std::slice::from_raw_parts(p, 16), &g);
        }
    }
}

#[cfg(test)]
mod tests_2110 {
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Chaque import relevé dans le journal de Teams est servi (#2110).
    #[test]
    fn imports_de_teams_servis() {
        let list = [
            ("kernel32", "BindIoCompletionCallback"),
            ("kernel32", "CreateThreadpoolCleanupGroup"),
            ("kernel32", "CloseThreadpoolCleanupGroupMembers"),
            ("kernel32", "CloseThreadpoolCleanupGroup"),
            ("kernel32", "SetThreadpoolTimerEx"),
            ("kernel32", "GetProcessIdOfThread"),
            ("kernel32", "FindPackagesByPackageFamily"),
            ("kernel32", "GetPackagePathByFullName"),
            ("kernel32", "RegisterApplicationRecoveryCallback"),
            ("kernel32", "UnregisterApplicationRecoveryCallback"),
            ("kernel32", "WerRegisterFile"),
            ("kernel32", "WerRegisterCustomMetadata"),
            ("kernel32", "WerRegisterAdditionalProcess"),
            ("user32", "ShowWindowAsync"),
            ("user32", "IsGUIThread"),
            ("user32", "CalculatePopupWindowPosition"),
            ("shlwapi", "#12"),
            ("shlwapi", "#184"),
            ("shlwapi", "#213"),
            ("shlwapi", "#214"),
            ("shlwapi", "#633"),
            ("shlwapi", "UrlEscapeW"),
            ("shlwapi", "UrlUnescapeW"),
            ("shlwapi", "UrlGetPartW"),
            ("shlwapi", "UrlCreateFromPathW"),
            ("oleaut32", "#27"),
            ("oleaut32", "#146"),
            ("oleaut32", "#184"),
            ("oleaut32", "#442"),
            ("ucrtbase", "set_terminate"),
            ("ucrtbase", "_byteswap_ulong"),
            ("ucrtbase", "_byteswap_uint64"),
            ("advapi32", "EventActivityIdControl"),
            ("advapi32", "AddConditionalAce"),
            ("iphlpapi", "GetIpNetTable2"),
            ("iphlpapi", "IcmpSendEcho2"),
            ("propsys", "InitPropVariantFromCLSID"),
            ("shell32", "SHCreateAssociationRegistration"),
        ];
        assert_eq!(list.len(), 38);
        for (dll, f) in list {
            assert!(crate::dlls::lookup(dll, f).is_some(), "{dll}!{f}");
        }
        // Les numéros pointent sur les fonctions nommées.
        for (dll, ord, name) in [
            ("oleaut32", "#27", "SafeArrayCopy"),
            ("oleaut32", "#146", "DispCallFunc"),
            ("shlwapi", "#12", "SHCreateMemStream"),
            ("shlwapi", "#633", "SHGlobalCounterIncrement"),
        ] {
            assert_eq!(
                crate::dlls::lookup(dll, ord),
                crate::dlls::lookup(dll, name),
                "{dll}!{ord}"
            );
        }
    }

    unsafe extern "win64" fn compte(
        _i: *mut crate::dlls::threadpool::Instance,
        ctx: usize,
        _w: *const crate::dlls::threadpool::Work,
    ) {
        std::thread::sleep(std::time::Duration::from_millis(5));
        (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "win64" fn annule(obj: usize, cleanup: usize) {
        (*(cleanup as *const AtomicU32)).fetch_add(1, Ordering::SeqCst);
        assert_ne!(obj, 0);
    }

    /// Groupe de nettoyage : les travaux de l'environnement en sont
    /// membres ; la fermeture attend leurs rappels puis les libère.
    #[test]
    fn groupe_de_nettoyage() {
        use crate::dlls::threadpool::*;
        let _teb = crate::process::test_teb();
        let hits = Box::leak(Box::new(AtomicU32::new(0)));
        let cancels = Box::leak(Box::new(AtomicU32::new(0)));
        unsafe {
            let group = create_threadpool_cleanup_group();
            assert_ne!(group, 0);
            // TP_CALLBACK_ENVIRON v3 (x64) : Version, Pool, CleanupGroup,
            // CleanupGroupCancelCallback…
            let mut env = [0usize; 9];
            env[0] = 3;
            env[2] = group;
            env[3] = annule as usize;
            let env_ptr = env.as_ptr() as usize;
            let ctx = hits as *const AtomicU32 as usize;
            let w1 = create_threadpool_work(Some(compte), ctx, env_ptr);
            let w2 = create_threadpool_work(Some(compte), ctx, env_ptr);
            let alone = create_threadpool_work(Some(compte), ctx, 0);
            for w in [w1, w2, alone] {
                submit_threadpool_work(w);
            }
            // Fermé seul, un membre quitte le groupe.
            wait_for_threadpool_work_callbacks(w2, 0);
            close_threadpool_work(w2);
            close_threadpool_cleanup_group_members(group, 0, cancels as *const AtomicU32 as usize);
            assert!(hits.load(Ordering::SeqCst) >= 2);
            assert_eq!(cancels.load(Ordering::SeqCst), 0);
            // Avec annulation, la fonction de l'environnement est appelée
            // pour chaque membre.
            let t = create_threadpool_timer(None, 0, env_ptr);
            assert!(t.is_null());
            let w3 = create_threadpool_work(Some(compte), ctx, env_ptr);
            assert!(!w3.is_null());
            close_threadpool_cleanup_group_members(group, 1, cancels as *const AtomicU32 as usize);
            assert_eq!(cancels.load(Ordering::SeqCst), 1);
            close_threadpool_cleanup_group(group);
            wait_for_threadpool_work_callbacks(alone, 0);
            close_threadpool_work(alone);
            assert_eq!(hits.load(Ordering::SeqCst), 3);
        }
    }

    /// SetThreadpoolTimerEx dit si le minuteur était armé.
    #[test]
    fn minuteur_ex() {
        use crate::dlls::threadpool::*;
        unsafe extern "win64" fn rien(_i: *mut Instance, _c: usize, _t: *const Timer) {}
        unsafe {
            let t = create_threadpool_timer(Some(rien), 0, 0);
            let loin = -10_000_000i64 * 60;
            assert_eq!(set_threadpool_timer_ex(t, &loin, 0, 0), 0);
            assert_eq!(set_threadpool_timer_ex(t, &loin, 0, 0), 1);
            assert_eq!(set_threadpool_timer_ex(t, std::ptr::null(), 0, 0), 1);
            assert_eq!(set_threadpool_timer_ex(t, std::ptr::null(), 0, 0), 0);
            close_threadpool_timer(t);
        }
    }

    /// SafeArrayCopy recopie les BSTR : la copie survit à la destruction
    /// de l'original.
    #[test]
    fn copie_de_safearray_bstr() {
        use crate::dlls::oleaut32::*;
        unsafe {
            let bound = [4u32, 0u32]; // 4 éléments, borne basse 0
            let sa = safe_array_create(8, 1, bound.as_ptr() as usize);
            assert_ne!(sa, 0);
            let w: Vec<u16> = "Teams".encode_utf16().chain(std::iter::once(0)).collect();
            let b = sys_alloc_string(w.as_ptr());
            let idx = 2i32;
            assert_eq!(
                safe_array_put_element(sa, &idx, b as *const std::ffi::c_void),
                0
            );
            let mut copy = 0usize;
            assert_eq!(safe_array_copy(sa, &mut copy), 0);
            assert_ne!(copy, 0);
            assert_ne!(copy, sa);
            assert_eq!(safe_array_destroy(sa), 0);
            let mut got: *mut u16 = std::ptr::null_mut();
            assert_eq!(
                safe_array_get_element(copy, &idx, &mut got as *mut _ as *mut std::ffi::c_void),
                0
            );
            assert_eq!(crate::dlls::wstr(got), "Teams");
            assert_eq!(safe_array_destroy(copy), 0);
            let mut none = 1usize;
            assert_eq!(safe_array_copy(0, &mut none), 0);
            assert_eq!(none, 0);
        }
    }
}
