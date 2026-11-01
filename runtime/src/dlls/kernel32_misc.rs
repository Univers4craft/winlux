//! Compléments de kernel32 : console, pages de code, environnement en bloc,
//! répertoires système, exceptions non gérées, chaînes `lstr*` et
//! diagnostics. Ce sont les appels qu'émettent le code de démarrage de
//! MinGW-w64 et les programmes console courants.

use std::ffi::c_void;
use std::io::Write;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const STD_OUTPUT: usize = (-11isize) as usize;
const STD_ERROR: usize = (-12isize) as usize;
const STD_INPUT: usize = (-10isize) as usize;

static ERROR_MODE: AtomicU32 = AtomicU32::new(0);
thread_local! { static THREAD_ERROR_MODE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) }; }
pub extern "win64" fn get_error_mode() -> u32 {
    ERROR_MODE.load(Ordering::SeqCst)
}
pub extern "win64" fn set_error_mode(mode: u32) -> u32 {
    // SEM_NOALIGNMENTFAULTEXCEPT is sticky for the lifetime of the process.
    ERROR_MODE
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |previous| {
            Some(mode | (previous & 4))
        })
        .unwrap()
}
pub extern "win64" fn get_thread_error_mode() -> u32 {
    THREAD_ERROR_MODE.with(|value| value.get().unwrap_or_else(|| get_error_mode()))
}
pub unsafe extern "win64" fn set_thread_error_mode(mode: u32, previous: *mut u32) -> i32 {
    if mode & !(1 | 2 | 0x8000) != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if !previous.is_null() {
        *previous = get_thread_error_mode();
    }
    THREAD_ERROR_MODE.with(|value| value.set(Some(mode)));
    1
}
pub unsafe extern "win64" fn wer_get_flags(_process: usize, flags: *mut u32) -> i32 {
    if flags.is_null() {
        return 0x80004003u32 as i32;
    }
    *flags = 0;
    // There is no Windows Error Reporting service. Let optional callers handle
    // an explicit unsupported HRESULT instead of pretending reports are sent.
    crate::trace!("WerGetFlags: service WER non disponible");
    0x80004001u32 as i32
}
pub extern "win64" fn wer_set_flags(_flags: u32) -> i32 {
    crate::trace!("WerSetFlags: service WER non disponible, journal local conservé");
    0x80004001u32 as i32
}

fn is_console(h: usize) -> bool {
    h == STD_OUTPUT || h == STD_ERROR || h == STD_INPUT
}

fn write_std(h: usize, bytes: &[u8]) -> bool {
    if h == STD_ERROR {
        let _ = std::io::stdout().flush();
        std::io::stderr().write_all(bytes).is_ok()
    } else {
        std::io::stdout()
            .write_all(bytes)
            .and_then(|_| std::io::stdout().flush())
            .is_ok()
    }
}

// ---------- Console ----------

pub unsafe extern "win64" fn write_console_a(
    h: usize,
    buf: *const u8,
    n: u32,
    written: *mut u32,
    _reserved: usize,
) -> i32 {
    if !is_console(h) || h == STD_INPUT {
        crate::process::set_last_error(6);
        return 0;
    }
    if n != 0 && buf.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let bytes = if n == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(buf, n as usize)
    };
    // Les octets sont dans la page de code de la console (Windows-1252) :
    // on les convertit pour le terminal Linux, qui attend de l'UTF-8.
    let text = if CONSOLE_OUTPUT_CP.load(Ordering::Relaxed) == 65001 {
        bytes.to_vec()
    } else {
        crate::dlls::decode_ansi(bytes).into_bytes()
    };
    if !write_std(h, &text) {
        crate::process::set_last_error(232);
        return 0;
    }
    if !written.is_null() {
        *written = n;
    }
    1
}

static CONSOLE_MODE_IN: AtomicU32 = AtomicU32::new(0x1f7);
static CONSOLE_MODE_OUT: AtomicU32 = AtomicU32::new(0x3);
static CONSOLE_CP: AtomicU32 = AtomicU32::new(1252);
static CONSOLE_OUTPUT_CP: AtomicU32 = AtomicU32::new(1252);

pub unsafe extern "win64" fn get_console_mode(h: usize, mode: *mut u32) -> i32 {
    if mode.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let fd = match h {
        STD_INPUT => 0,
        STD_OUTPUT => 1,
        STD_ERROR => 2,
        _ => {
            crate::process::set_last_error(6);
            return 0;
        }
    };
    // Une sortie redirigée vers un fichier n'est pas une console.
    if libc::isatty(fd) != 1 {
        crate::process::set_last_error(6);
        return 0;
    }
    *mode = if h == STD_INPUT {
        CONSOLE_MODE_IN.load(Ordering::Relaxed)
    } else {
        CONSOLE_MODE_OUT.load(Ordering::Relaxed)
    };
    1
}
pub extern "win64" fn set_console_mode(h: usize, mode: u32) -> i32 {
    match h {
        STD_INPUT => CONSOLE_MODE_IN.store(mode, Ordering::Relaxed),
        STD_OUTPUT | STD_ERROR => CONSOLE_MODE_OUT.store(mode, Ordering::Relaxed),
        _ => {
            crate::process::set_last_error(6);
            return 0;
        }
    }
    1
}
pub extern "win64" fn get_console_cp() -> u32 {
    CONSOLE_CP.load(Ordering::Relaxed)
}
pub extern "win64" fn get_console_output_cp() -> u32 {
    CONSOLE_OUTPUT_CP.load(Ordering::Relaxed)
}
pub extern "win64" fn set_console_cp(cp: u32) -> i32 {
    if is_valid_code_page(cp) == 0 {
        return 0;
    }
    CONSOLE_CP.store(cp, Ordering::Relaxed);
    1
}
pub extern "win64" fn set_console_output_cp(cp: u32) -> i32 {
    if is_valid_code_page(cp) == 0 {
        return 0;
    }
    CONSOLE_OUTPUT_CP.store(cp, Ordering::Relaxed);
    1
}
pub extern "win64" fn set_console_ctrl_handler(_handler: usize, _add: i32) -> i32 {
    1
}
pub unsafe extern "win64" fn set_console_title_a(_title: *const u8) -> i32 {
    1
}
pub unsafe extern "win64" fn set_console_title_w(_title: *const u16) -> i32 {
    1
}
pub extern "win64" fn get_console_window() -> usize {
    0
}
pub extern "win64" fn alloc_console() -> i32 {
    0
}
pub extern "win64" fn free_console() -> i32 {
    1
}
pub extern "win64" fn attach_console(_pid: u32) -> i32 {
    0
}

/// `CONSOLE_SCREEN_BUFFER_INFO` : taille de la fenêtre du terminal Linux.
pub unsafe extern "win64" fn get_console_screen_buffer_info(h: usize, info: *mut u16) -> i32 {
    if info.is_null() || !(h == STD_OUTPUT || h == STD_ERROR) {
        crate::process::set_last_error(6);
        return 0;
    }
    let fd = if h == STD_ERROR { 2 } else { 1 };
    let mut ws: libc::winsize = std::mem::zeroed();
    if libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) != 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    let (cols, rows) = (ws.ws_col.max(1), ws.ws_row.max(1));
    let fields: [u16; 11] = [cols, rows, 0, 0, 0x7, 0, 0, cols - 1, rows - 1, cols, rows];
    std::ptr::copy_nonoverlapping(fields.as_ptr(), info, fields.len());
    1
}
pub extern "win64" fn set_console_text_attribute(h: usize, _attr: u16) -> i32 {
    is_console(h) as i32
}

// ---------- Pages de code ----------

pub extern "win64" fn get_acp() -> u32 {
    1252
}
pub extern "win64" fn get_oemcp() -> u32 {
    850
}
pub extern "win64" fn is_valid_code_page(cp: u32) -> i32 {
    matches!(cp, 437 | 850 | 1252 | 20127 | 28591 | 65000 | 65001) as i32
}
pub extern "win64" fn is_dbcs_lead_byte(_b: u8) -> i32 {
    0
}
pub extern "win64" fn is_dbcs_lead_byte_ex(_cp: u32, _b: u8) -> i32 {
    0
}
/// `CPINFO` : taille maximale d'un caractère, caractère par défaut, octets de tête.
pub unsafe extern "win64" fn get_cp_info(cp: u32, info: *mut u8) -> i32 {
    if info.is_null() || is_valid_code_page(cp) == 0 && cp != 0 && cp != 1 {
        crate::process::set_last_error(87);
        return 0;
    }
    std::ptr::write_bytes(info, 0, 20);
    *(info as *mut u32) = if cp == 65001 { 4 } else { 1 };
    *info.add(4) = b'?';
    1
}

// ---------- Environnement en un bloc ----------

fn environment_block() -> Vec<String> {
    let mut vars: Vec<String> = std::env::vars_os()
        .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
        .collect();
    vars.sort_by_key(|s| s.to_ascii_uppercase());
    vars
}
pub extern "win64" fn get_environment_strings_w() -> *mut u16 {
    let mut w: Vec<u16> = Vec::new();
    for s in environment_block() {
        w.extend(s.encode_utf16());
        w.push(0);
    }
    w.push(0);
    if w.len() == 1 {
        w.push(0);
    }
    Box::leak(w.into_boxed_slice()).as_mut_ptr()
}
pub extern "win64" fn get_environment_strings_a() -> *mut u8 {
    let mut b: Vec<u8> = Vec::new();
    for s in environment_block() {
        b.extend(crate::dlls::encode_ansi(&s));
        b.push(0);
    }
    b.push(0);
    if b.len() == 1 {
        b.push(0);
    }
    Box::leak(b.into_boxed_slice()).as_mut_ptr()
}
pub unsafe extern "win64" fn free_environment_strings_w(p: *mut u16) -> i32 {
    if p.is_null() {
        return 0;
    }
    let mut n = 0;
    while *p.add(n) != 0 || *p.add(n + 1) != 0 {
        n += 1;
    }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, n + 2)));
    1
}
pub unsafe extern "win64" fn free_environment_strings_a(p: *mut u8) -> i32 {
    if p.is_null() {
        return 0;
    }
    let mut n = 0;
    while *p.add(n) != 0 || *p.add(n + 1) != 0 {
        n += 1;
    }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, n + 2)));
    1
}

// ---------- Répertoires système ----------

pub unsafe extern "win64" fn get_system_directory_a(buf: *mut u8, size: u32) -> u32 {
    crate::dlls::put_a("C:\\Windows\\System32", buf, size)
}
pub unsafe extern "win64" fn get_system_directory_w(buf: *mut u16, size: u32) -> u32 {
    crate::dlls::put_w("C:\\Windows\\System32", buf, size)
}
pub unsafe extern "win64" fn get_windows_directory_a(buf: *mut u8, size: u32) -> u32 {
    crate::dlls::put_a("C:\\Windows", buf, size)
}
pub unsafe extern "win64" fn get_windows_directory_w(buf: *mut u16, size: u32) -> u32 {
    crate::dlls::put_w("C:\\Windows", buf, size)
}

// ---------- Exceptions non gérées et diagnostics ----------

static UNHANDLED_FILTER: AtomicUsize = AtomicUsize::new(0);

/// Mémorise le filtre de dernier recours et renvoie le précédent.
pub extern "win64" fn set_unhandled_exception_filter(filter: usize) -> usize {
    UNHANDLED_FILTER.swap(filter, Ordering::AcqRel)
}
pub extern "win64" fn unhandled_exception_filter(_info: usize) -> i32 {
    1
} // EXCEPTION_EXECUTE_HANDLER

pub fn current_unhandled_filter() -> usize {
    UNHANDLED_FILTER.load(Ordering::Acquire)
}

/// Exception que ni les gestionnaires du programme ni son filtre de dernier
/// recours n'ont prise en charge : diagnostic puis arrêt avec son code, comme
/// le ferait Windows.
pub fn report_unhandled(code: u32, flags: u32, args: &[usize]) -> ! {
    let detail = if args.is_empty() {
        String::new()
    } else {
        format!(
            " (arguments : {})",
            args.iter()
                .map(|v| format!("0x{v:x}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let _ = std::io::stdout().flush();
    eprintln!(
        "\nwinlux: exception Windows 0x{code:08X} non gérée par le programme{}{detail}",
        if flags & 1 != 0 {
            ", non continuable"
        } else {
            ""
        }
    );
    if code == 0xE06D_7363 {
        eprintln!("winlux: il s'agit d'une exception C++ MSVC qu'aucun bloc catch n'a rattrapée");
    }
    crate::log_error!("seh", "exception 0x{code:08X} non gérée");
    crate::dlls::kernel32::flush_and_exit(code as i32)
}
pub extern "win64" fn is_debugger_present() -> i32 {
    0
}
pub unsafe extern "win64" fn output_debug_string_a(s: *const u8) {
    if std::env::var_os("WINLUX_TRACE").is_some() && !s.is_null() {
        eprintln!("winlux[debug]: {}", crate::dlls::cstr(s).trim_end());
    }
}
pub unsafe extern "win64" fn output_debug_string_w(s: *const u16) {
    if std::env::var_os("WINLUX_TRACE").is_some() && !s.is_null() {
        eprintln!("winlux[debug]: {}", crate::dlls::wstr(s).trim_end());
    }
}
/// Fonctionnalités processeur (PF_*) : on répond pour celles qu'impose x86-64
/// et on interroge CPUID pour les autres.
pub extern "win64" fn is_processor_feature_present(feature: u32) -> i32 {
    #[cfg(target_arch = "x86_64")]
    {
        let f = |name: &str| match name {
            "sse3" => std::arch::is_x86_feature_detected!("sse3"),
            "avx" => std::arch::is_x86_feature_detected!("avx"),
            "avx2" => std::arch::is_x86_feature_detected!("avx2"),
            "rdrand" => std::arch::is_x86_feature_detected!("rdrand"),
            "cmpxchg16b" => std::arch::is_x86_feature_detected!("cmpxchg16b"),
            _ => false,
        };
        return match feature {
            3 | 6 | 7 | 8 | 10 | 12 | 13 | 17 | 23 => 1, // MMX, SSE, SSE2, RDTSC, NX, cmpxchg64, fastfail…
            14 => f("cmpxchg16b") as i32,
            28 => f("rdrand") as i32,
            39 => f("avx") as i32,
            40 => f("avx2") as i32,
            _ => 0,
        };
    }
    #[allow(unreachable_code)]
    0
}
pub extern "win64" fn switch_to_thread() -> i32 {
    std::thread::yield_now();
    1
}
pub extern "win64" fn exit_thread(code: u32) -> ! {
    // Sans pile de retour vers le pont de thread, on termine le thread
    // courant via pthread_exit : les destructeurs Rust ne sont pas exécutés.
    crate::process::thread_detach_current();
    unsafe { libc::pthread_exit(code as usize as *mut c_void) }
}

// ---------- Chaînes « lstr » ----------

pub unsafe extern "win64" fn lstrlen_a(s: *const u8) -> i32 {
    if s.is_null() {
        0
    } else {
        crate::printf::narrow_len(s) as i32
    }
}
pub unsafe extern "win64" fn lstrlen_w(s: *const u16) -> i32 {
    if s.is_null() {
        0
    } else {
        crate::printf::wide_len(s) as i32
    }
}
pub unsafe extern "win64" fn lstrcpy_a(d: *mut u8, s: *const u8) -> *mut u8 {
    if d.is_null() || s.is_null() {
        return std::ptr::null_mut();
    }
    let n = crate::printf::narrow_len(s);
    std::ptr::copy(s, d, n + 1);
    d
}
pub unsafe extern "win64" fn lstrcpy_w(d: *mut u16, s: *const u16) -> *mut u16 {
    if d.is_null() || s.is_null() {
        return std::ptr::null_mut();
    }
    let n = crate::printf::wide_len(s);
    std::ptr::copy(s, d, n + 1);
    d
}
pub unsafe extern "win64" fn lstrcat_a(d: *mut u8, s: *const u8) -> *mut u8 {
    if d.is_null() || s.is_null() {
        return std::ptr::null_mut();
    }
    lstrcpy_a(d.add(crate::printf::narrow_len(d)), s);
    d
}
pub unsafe extern "win64" fn lstrcat_w(d: *mut u16, s: *const u16) -> *mut u16 {
    if d.is_null() || s.is_null() {
        return std::ptr::null_mut();
    }
    lstrcpy_w(d.add(crate::printf::wide_len(d)), s);
    d
}
fn compare(a: String, b: String, fold: bool) -> i32 {
    let (a, b) = if fold {
        (a.to_lowercase(), b.to_lowercase())
    } else {
        (a, b)
    };
    a.cmp(&b) as i32
}
pub unsafe extern "win64" fn lstrcmp_a(a: *const u8, b: *const u8) -> i32 {
    compare(crate::dlls::cstr(a), crate::dlls::cstr(b), false)
}
pub unsafe extern "win64" fn lstrcmpi_a(a: *const u8, b: *const u8) -> i32 {
    compare(crate::dlls::cstr(a), crate::dlls::cstr(b), true)
}
pub unsafe extern "win64" fn lstrcmp_w(a: *const u16, b: *const u16) -> i32 {
    compare(crate::dlls::wstr(a), crate::dlls::wstr(b), false)
}
pub unsafe extern "win64" fn lstrcmpi_w(a: *const u16, b: *const u16) -> i32 {
    compare(crate::dlls::wstr(a), crate::dlls::wstr(b), true)
}

// ---------- Messages d'erreur système ----------

fn system_message(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "L’opération a réussi.",
        2 => "Le fichier spécifié est introuvable.",
        3 => "Le chemin d’accès spécifié est introuvable.",
        5 => "Accès refusé.",
        6 => "Descripteur non valide.",
        8 => "Espace de stockage insuffisant pour traiter cette commande.",
        32 => "Le processus ne peut pas accéder au fichier car ce fichier est utilisé par un autre processus.",
        80 => "Le fichier existe.",
        87 => "Paramètre incorrect.",
        122 => "La zone de données passée à un appel système est insuffisante.",
        126 => "Le module spécifié est introuvable.",
        127 => "La procédure spécifiée est introuvable.",
        183 => "Impossible de créer un fichier déjà existant.",
        _ => return None,
    })
}

const FORMAT_MESSAGE_ALLOCATE_BUFFER: u32 = 0x100;
const FORMAT_MESSAGE_FROM_SYSTEM: u32 = 0x1000;

/// Sous-ensemble de `FormatMessage` : messages système (`FROM_SYSTEM`) avec
/// ou sans allocation du tampon. Les chaînes de format applicatives ne sont
/// pas encore interprétées.
unsafe fn format_message(flags: u32, code: u32) -> Option<String> {
    if flags & FORMAT_MESSAGE_FROM_SYSTEM == 0 {
        return None;
    }
    Some(format!(
        "{}\r\n",
        system_message(code)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Erreur Windows {code}."))
    ))
}
pub unsafe extern "win64" fn format_message_a(
    flags: u32,
    _src: usize,
    code: u32,
    _lang: u32,
    buf: *mut u8,
    size: u32,
    _args: usize,
) -> u32 {
    let Some(text) = format_message(flags, code) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let mut bytes = crate::dlls::encode_ansi(&text);
    let n = bytes.len() as u32;
    bytes.push(0);
    if flags & FORMAT_MESSAGE_ALLOCATE_BUFFER != 0 {
        if buf.is_null() {
            return 0;
        }
        let p = crate::dlls::kernel32::heap_alloc(0, 0, bytes.len()) as *mut u8;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        *(buf as *mut *mut u8) = p;
        return n;
    }
    if buf.is_null() || size < n + 1 {
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    n
}
pub unsafe extern "win64" fn format_message_w(
    flags: u32,
    _src: usize,
    code: u32,
    _lang: u32,
    buf: *mut u16,
    size: u32,
    _args: usize,
) -> u32 {
    let Some(text) = format_message(flags, code) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let n = w.len() as u32 - 1;
    if flags & FORMAT_MESSAGE_ALLOCATE_BUFFER != 0 {
        if buf.is_null() {
            return 0;
        }
        let p = crate::dlls::kernel32::heap_alloc(0, 0, w.len() * 2) as *mut u16;
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        *(buf as *mut *mut u16) = p;
        return n;
    }
    if buf.is_null() || size < n + 1 {
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    n
}
/// LocalAlloc : mémoire du tas du processus ; LMEM_ZEROINIT (0x40) la met à
/// zéro. LMEM_MOVEABLE renvoie aussi un pointeur, que LocalLock rend tel quel.
pub unsafe extern "win64" fn local_alloc(flags: u32, size: usize) -> *mut c_void {
    let heap_flags = if flags & 0x40 != 0 { 8 } else { 0 };
    crate::dlls::kernel32::heap_alloc(0, heap_flags, size.max(1))
}
pub unsafe extern "win64" fn local_realloc(p: *mut c_void, size: usize, flags: u32) -> *mut c_void {
    if p.is_null() {
        return local_alloc(flags, size);
    }
    let heap_flags = if flags & 0x40 != 0 { 8 } else { 0 };
    crate::dlls::kernel32::heap_realloc(0, heap_flags, p, size.max(1))
}
pub extern "win64" fn local_lock(p: *mut c_void) -> *mut c_void {
    p
}
pub extern "win64" fn local_unlock(_p: *mut c_void) -> i32 {
    0
}
pub unsafe extern "win64" fn local_size(p: *mut c_void) -> usize {
    if p.is_null() {
        return 0;
    }
    crate::dlls::kernel32::heap_size(0, 0, p)
}
pub unsafe extern "win64" fn local_free(p: *mut c_void) -> *mut c_void {
    if !p.is_null() {
        crate::dlls::kernel32::heap_free(0, 0, p);
    }
    std::ptr::null_mut()
}

// ---------- Locale ----------

pub extern "win64" fn get_user_default_lcid() -> u32 {
    0x040C
} // fr-FR
pub extern "win64" fn get_thread_locale() -> u32 {
    0x040C
}
pub extern "win64" fn get_user_default_lang_id() -> u16 {
    0x040C
}
pub extern "win64" fn get_user_default_ui_language() -> u16 {
    0x040C
}

/// Winlux n'exécute pas d'applications empaquetées (MSIX/AppX) : les API de
/// paquet répondent `APPMODEL_ERROR_NO_PACKAGE`, ce que les programmes
/// classiques reçoivent aussi sous Windows.
const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
pub unsafe extern "win64" fn get_current_package_id(len: *mut u32, _buf: *mut u8) -> u32 {
    if len.is_null() {
        return 87; // ERROR_INVALID_PARAMETER
    }
    APPMODEL_ERROR_NO_PACKAGE
}
pub unsafe extern "win64" fn get_current_package_name(len: *mut u32, _buf: *mut u16) -> u32 {
    get_current_package_id(len, std::ptr::null_mut())
}
pub unsafe extern "win64" fn get_current_package_path(len: *mut u32, _buf: *mut u16) -> u32 {
    get_current_package_id(len, std::ptr::null_mut())
}

/// IsWow64Process : Winlux exécute les programmes 64 bits nativement, sans
/// couche WOW64.
pub unsafe extern "win64" fn is_wow64_process(_process: usize, wow64: *mut i32) -> i32 {
    if wow64.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *wow64 = 0;
    1
}

/// LCMapStringEx : variante de LCMapStringW nommant la locale par son nom.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn lc_map_string_ex(
    _locale: *const u16,
    flags: u32,
    src: *const u16,
    src_count: i32,
    dst: *mut u16,
    dst_count: i32,
    _version: *mut c_void,
    _reserved: *mut c_void,
    _sort: isize,
) -> i32 {
    crate::dlls::kernel32::lc_map_string_w(0x0400, flags, src, src_count, dst, dst_count)
}

/// GetFinalPathNameByHandleW : chemin Windows du fichier ouvert, au format
/// `\\?\C:\…` par défaut ou sans préfixe avec VOLUME_NAME_NONE (4).
pub unsafe extern "win64" fn get_final_path_name_by_handle_w(
    handle: usize,
    buf: *mut u16,
    len: u32,
    flags: u32,
) -> u32 {
    let fd = crate::dlls::kernel32::handle_to_fd(handle);
    let target = if fd < 0 {
        None
    } else {
        std::fs::read_link(format!("/proc/self/fd/{fd}")).ok()
    };
    let Some(target) = target else {
        crate::process::set_last_error(6); // ERROR_INVALID_HANDLE
        return 0;
    };
    let win = crate::paths::to_windows(&target);
    let text = if flags & 4 != 0 {
        win.get(2..).unwrap_or(&win).to_string()
    } else {
        format!("\\\\?\\{win}")
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    if buf.is_null() || (len as usize) <= wide.len() {
        return wide.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    *buf.add(wide.len()) = 0;
    wide.len() as u32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetErrorMode" => fn get_error_mode, "SetErrorMode" => fn set_error_mode,
        "GetThreadErrorMode" => fn get_thread_error_mode, "SetThreadErrorMode" => fn set_thread_error_mode,
        "WerGetFlags" => fn wer_get_flags, "WerSetFlags" => fn wer_set_flags,
        "WriteConsoleA" => fn write_console_a, "GetConsoleMode" => fn get_console_mode, "SetConsoleMode" => fn set_console_mode,
        "GetConsoleCP" => fn get_console_cp, "GetConsoleOutputCP" => fn get_console_output_cp,
        "SetConsoleCP" => fn set_console_cp, "SetConsoleOutputCP" => fn set_console_output_cp,
        "SetConsoleCtrlHandler" => fn set_console_ctrl_handler, "SetConsoleTitleA" => fn set_console_title_a, "SetConsoleTitleW" => fn set_console_title_w,
        "GetConsoleWindow" => fn get_console_window, "AllocConsole" => fn alloc_console, "FreeConsole" => fn free_console, "AttachConsole" => fn attach_console,
        "GetConsoleScreenBufferInfo" => fn get_console_screen_buffer_info, "SetConsoleTextAttribute" => fn set_console_text_attribute,
        "GetACP" => fn get_acp, "GetOEMCP" => fn get_oemcp, "IsValidCodePage" => fn is_valid_code_page,
        "IsDBCSLeadByte" => fn is_dbcs_lead_byte, "IsDBCSLeadByteEx" => fn is_dbcs_lead_byte_ex, "GetCPInfo" => fn get_cp_info,
        "GetEnvironmentStringsW" => fn get_environment_strings_w, "GetEnvironmentStrings" => fn get_environment_strings_a,
        "GetEnvironmentStringsA" => fn get_environment_strings_a,
        "FreeEnvironmentStringsW" => fn free_environment_strings_w, "FreeEnvironmentStringsA" => fn free_environment_strings_a,
        "GetSystemDirectoryA" => fn get_system_directory_a, "GetSystemDirectoryW" => fn get_system_directory_w,
        "GetWindowsDirectoryA" => fn get_windows_directory_a, "GetWindowsDirectoryW" => fn get_windows_directory_w,
        "SetUnhandledExceptionFilter" => fn set_unhandled_exception_filter, "UnhandledExceptionFilter" => fn unhandled_exception_filter,
        "IsDebuggerPresent" => fn is_debugger_present,
        "OutputDebugStringA" => fn output_debug_string_a, "OutputDebugStringW" => fn output_debug_string_w,
        "IsProcessorFeaturePresent" => fn is_processor_feature_present, "SwitchToThread" => fn switch_to_thread, "ExitThread" => fn exit_thread,
        "lstrlenA" => fn lstrlen_a, "lstrlenW" => fn lstrlen_w, "lstrcpyA" => fn lstrcpy_a, "lstrcpyW" => fn lstrcpy_w,
        "lstrcatA" => fn lstrcat_a, "lstrcatW" => fn lstrcat_w, "lstrcmpA" => fn lstrcmp_a, "lstrcmpiA" => fn lstrcmpi_a,
        "lstrcmpW" => fn lstrcmp_w, "lstrcmpiW" => fn lstrcmpi_w,
        "FormatMessageA" => fn format_message_a, "FormatMessageW" => fn format_message_w, "LocalFree" => fn local_free,
        "LocalAlloc" => fn local_alloc, "LocalReAlloc" => fn local_realloc, "LocalLock" => fn local_lock,
        "LocalUnlock" => fn local_unlock, "LocalSize" => fn local_size,
        "GetUserDefaultLCID" => fn get_user_default_lcid, "GetThreadLocale" => fn get_thread_locale,
        "GetUserDefaultLangID" => fn get_user_default_lang_id, "GetUserDefaultUILanguage" => fn get_user_default_ui_language,
        "GetCurrentPackageId" => fn get_current_package_id,
        "GetCurrentPackageFullName" => fn get_current_package_name,
        "GetCurrentPackageFamilyName" => fn get_current_package_name,
        "GetCurrentApplicationUserModelId" => fn get_current_package_name,
        "GetCurrentPackagePath" => fn get_current_package_path,
        "IsWow64Process" => fn is_wow64_process, "LCMapStringEx" => fn lc_map_string_ex,
        "GetFinalPathNameByHandleW" => fn get_final_path_name_by_handle_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_mode_has_process_state_sticky_alignment_and_thread_isolation() {
        struct Restore(u32);
        impl Drop for Restore {
            fn drop(&mut self) {
                ERROR_MODE.store(self.0, Ordering::SeqCst);
            }
        }
        let _restore = Restore(get_error_mode());
        set_error_mode(1);
        assert_eq!(set_error_mode(5), 1);
        assert_eq!(set_error_mode(0), 5);
        assert_eq!(get_error_mode(), 4);
        let previous_thread = THREAD_ERROR_MODE.with(|mode| mode.replace(None));
        let mut previous = 99;
        assert_eq!(unsafe { set_thread_error_mode(2, &mut previous) }, 1);
        assert_eq!(previous, 4);
        assert_eq!(get_thread_error_mode(), 2);
        assert_eq!(
            std::thread::spawn(|| get_thread_error_mode())
                .join()
                .unwrap(),
            4
        );
        assert_eq!(unsafe { set_thread_error_mode(4, std::ptr::null_mut()) }, 0);
        assert_eq!(get_thread_error_mode(), 2);
        THREAD_ERROR_MODE.with(|mode| mode.set(previous_thread));
    }

    #[test]
    fn bloc_d_environnement_double_zero() {
        std::env::set_var("WINLUX_BLOC_TEST", "oui");
        unsafe {
            let p = get_environment_strings_w();
            let mut found = false;
            let mut at = 0;
            loop {
                let n = crate::printf::wide_len(p.add(at));
                if n == 0 {
                    break;
                }
                if crate::dlls::wstr(p.add(at)) == "WINLUX_BLOC_TEST=oui" {
                    found = true;
                }
                at += n + 1;
            }
            assert!(found);
            assert_eq!(free_environment_strings_w(p), 1);
        }
    }

    #[test]
    fn chemin_final_d_un_handle_et_wow64() {
        let dir = std::env::temp_dir().join(format!("winlux-final-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hostfxr.dll");
        std::fs::write(&file, b"MZ").unwrap();
        let win = crate::paths::to_windows(&file);
        let w: Vec<u16> = win.encode_utf16().chain([0]).collect();
        unsafe {
            let h = crate::dlls::kernel32::create_file_w(w.as_ptr(), 0x8000_0000, 1, 0, 3, 0x80, 0);
            assert_ne!(h, usize::MAX);
            let need = get_final_path_name_by_handle_w(h, std::ptr::null_mut(), 0, 0);
            let mut buf = vec![0u16; need as usize];
            let n = get_final_path_name_by_handle_w(h, buf.as_mut_ptr(), need, 0);
            assert_eq!(n + 1, need);
            let got = crate::dlls::wstr(buf.as_ptr());
            assert!(got.starts_with("\\\\?\\"), "{got}");
            assert!(got.ends_with("hostfxr.dll"), "{got}");
            crate::dlls::kernel32::close_handle(h);
            let mut wow = 7;
            assert_eq!(is_wow64_process(0, &mut wow), 1);
            assert_eq!(wow, 0);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pas_de_paquet_msix() {
        let mut len = 0u32;
        unsafe {
            assert_eq!(
                get_current_package_id(&mut len, std::ptr::null_mut()),
                15700
            );
            assert_eq!(
                get_current_package_id(std::ptr::null_mut(), std::ptr::null_mut()),
                87
            );
            assert_eq!(
                get_current_package_name(&mut len, std::ptr::null_mut()),
                15700
            );
        }
    }

    #[test]
    fn filtre_d_exception_et_pages_de_code() {
        let old = set_unhandled_exception_filter(0x1234);
        assert_eq!(set_unhandled_exception_filter(old), 0x1234);
        assert_eq!(get_acp(), 1252);
        assert_eq!(is_valid_code_page(65001), 1);
        assert_eq!(is_valid_code_page(12345), 0);
        let mut info = [0u8; 20];
        assert_eq!(unsafe { get_cp_info(1252, info.as_mut_ptr()) }, 1);
        assert_eq!(info[0], 1);
    }

    #[test]
    fn chaines_lstr_et_messages() {
        unsafe {
            let mut buf = [0u8; 16];
            lstrcpy_a(buf.as_mut_ptr(), b"Win\0".as_ptr());
            lstrcat_a(buf.as_mut_ptr(), b"lux\0".as_ptr());
            assert_eq!(lstrlen_a(buf.as_ptr()), 6);
            assert_eq!(lstrcmpi_a(buf.as_ptr(), b"WINLUX\0".as_ptr()), 0);
            let mut msg = [0u16; 128];
            let n = format_message_w(
                FORMAT_MESSAGE_FROM_SYSTEM,
                0,
                2,
                0,
                msg.as_mut_ptr(),
                128,
                0,
            );
            assert!(n > 0);
            assert!(crate::dlls::wstr(msg.as_ptr()).starts_with("Le fichier spécifié"));
        }
    }
}
