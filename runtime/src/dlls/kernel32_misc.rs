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
    let h = super::kernel32::canon_std(h);
    h == STD_OUTPUT || h == STD_ERROR || h == STD_INPUT
}

fn write_std(h: usize, bytes: &[u8]) -> bool {
    let h = super::kernel32::canon_std(h);
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
    if !is_console(h) || super::kernel32::canon_std(h) == STD_INPUT {
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
    // Les octets sont dans la page de code de la console (OEM de la locale,
    // 850 en Europe de l'Ouest, sauf SetConsoleOutputCP) : on les convertit
    // pour le terminal Linux, qui attend de l'UTF-8.
    let cp = get_console_output_cp();
    let text = if cp == 65001 {
        bytes.to_vec()
    } else {
        crate::intl::codepage::decode(cp, bytes, false)
            .unwrap_or_else(|_| crate::dlls::decode_ansi(bytes))
            .into_bytes()
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
/// 0 : page OEM de la locale (valeur par défaut de la console Windows).
static CONSOLE_CP: AtomicU32 = AtomicU32::new(0);
static CONSOLE_OUTPUT_CP: AtomicU32 = AtomicU32::new(0);

pub unsafe extern "win64" fn get_console_mode(h: usize, mode: *mut u32) -> i32 {
    let h = super::kernel32::canon_std(h);
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
    let h = super::kernel32::canon_std(h);
    match h {
        STD_INPUT => {
            CONSOLE_MODE_IN.store(mode, Ordering::Relaxed);
            super::console_input::apply_input_mode(mode);
        }
        STD_OUTPUT | STD_ERROR => CONSOLE_MODE_OUT.store(mode, Ordering::Relaxed),
        _ => {
            crate::process::set_last_error(6);
            return 0;
        }
    }
    1
}
pub extern "win64" fn get_console_cp() -> u32 {
    match CONSOLE_CP.load(Ordering::Relaxed) {
        0 => crate::intl::codepage::resolve(crate::intl::codepage::CP_OEMCP),
        cp => cp,
    }
}
pub extern "win64" fn get_console_output_cp() -> u32 {
    match CONSOLE_OUTPUT_CP.load(Ordering::Relaxed) {
        0 => crate::intl::codepage::resolve(crate::intl::codepage::CP_OEMCP),
        cp => cp,
    }
}
pub extern "win64" fn set_console_cp(cp: u32) -> i32 {
    if !crate::intl::codepage::is_valid(cp) {
        return 0;
    }
    CONSOLE_CP.store(cp, Ordering::Relaxed);
    1
}
pub extern "win64" fn set_console_output_cp(cp: u32) -> i32 {
    if !crate::intl::codepage::is_valid(cp) {
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
    let h = super::kernel32::canon_std(h);
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

// GetACP, GetOEMCP, IsValidCodePage, GetCPInfo : voir crate::intl::api.
pub extern "win64" fn is_dbcs_lead_byte(b: u8) -> i32 {
    is_dbcs_lead_byte_ex(0, b)
}
pub extern "win64" fn is_dbcs_lead_byte_ex(cp: u32, b: u8) -> i32 {
    crate::intl::dbcs::is_lead_byte(crate::intl::codepage::resolve(cp), b) as i32
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
        if let Some(text) = unsafe { cxx_thrown_value(args) } {
            eprintln!("winlux: objet lancé : {text}");
        }
    }
    print_debug_tail();
    crate::log_error!("seh", "exception 0x{code:08X} non gérée");
    crate::dlls::kernel32::flush_and_exit(code as i32)
}
/// Type (nom décoré) et valeur d'une exception C++ MSVC x64 : `throw hr;` de
/// 7-Zip ou `throw "message";` deviennent lisibles dans le journal.
/// args = [magie, objet, ThrowInfo, base de l'image].
unsafe fn cxx_thrown_value(args: &[usize]) -> Option<String> {
    if args.len() < 4 || args[0] != 0x1993_0520 || args[2] == 0 || args[3] == 0 {
        return None;
    }
    let (object, info, base) = (args[1], args[2], args[3]);
    let rva = |at: usize| unsafe { std::ptr::read_unaligned(at as *const i32) } as usize;
    let array = base + rva(info + 12);
    if unsafe { std::ptr::read_unaligned(array as *const i32) } < 1 {
        return None;
    }
    let catchable = base + rva(array + 4);
    let name_at = (base + rva(catchable + 4) + 16) as *const std::ffi::c_char;
    let name = unsafe { std::ffi::CStr::from_ptr(name_at) }
        .to_string_lossy()
        .into_owned();
    if object == 0 {
        return Some(name);
    }
    let value = match name.as_str() {
        ".H" | ".J" | ".K" => {
            let v = unsafe { std::ptr::read_unaligned(object as *const u32) };
            format!(" = 0x{v:08X} ({})", v as i32)
        }
        ".PEBD" | ".PEAD" => {
            let text = unsafe { std::ptr::read_unaligned(object as *const usize) };
            if text == 0 {
                String::new()
            } else {
                let c = unsafe { std::ffi::CStr::from_ptr(text as *const std::ffi::c_char) };
                format!(" = {:?}", c.to_string_lossy())
            }
        }
        _ => String::new(),
    };
    Some(format!("{name}{value}"))
}
pub extern "win64" fn is_debugger_present() -> i32 {
    0
}
/// Derniers messages OutputDebugString, rappelés sur stderr quand le
/// programme s'arrête sur abort() ou une exception non gérée : Qt y écrit la
/// raison de son qFatal (VLC), d'autres bibliothèques leur dernière erreur.
static DEBUG_TAIL: std::sync::Mutex<std::collections::VecDeque<String>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
const DEBUG_TAIL_LEN: usize = 20;

fn debug_string(text: String) {
    let text = text.trim_end().to_string();
    if text.is_empty() {
        return;
    }
    let line = format!("winlux[debug]: {text}");
    if std::env::var_os("WINLUX_TRACE").is_some() {
        eprintln!("{line}");
    } else {
        crate::log_info!("debug", "OutputDebugString : {text}");
    }
    // Journal d'API (WINLUX_APITRACE) : Crashpad y écrit pourquoi il arrête
    // un processus.
    crate::thunks::api_note(&line);
    let mut tail = crate::dlls::lock(&DEBUG_TAIL);
    if tail.len() == DEBUG_TAIL_LEN {
        tail.pop_front();
    }
    tail.push_back(text);
}

/// Affiche (une fois) les derniers messages OutputDebugString du programme.
pub fn print_debug_tail() {
    let tail: Vec<String> = crate::dlls::lock(&DEBUG_TAIL).drain(..).collect();
    if tail.is_empty() {
        return;
    }
    eprintln!("winlux: derniers messages de débogage du programme (OutputDebugString) :");
    for line in tail {
        eprintln!("winlux:   {line}");
    }
}

pub unsafe extern "win64" fn output_debug_string_a(s: *const u8) {
    if !s.is_null() {
        debug_string(crate::dlls::cstr(s));
    }
}
pub unsafe extern "win64" fn output_debug_string_w(s: *const u16) {
    if !s.is_null() {
        debug_string(crate::dlls::wstr(s));
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
            "ssse3" => std::arch::is_x86_feature_detected!("ssse3"),
            "sse4.1" => std::arch::is_x86_feature_detected!("sse4.1"),
            "sse4.2" => std::arch::is_x86_feature_detected!("sse4.2"),
            "avx512f" => std::arch::is_x86_feature_detected!("avx512f"),
            _ => false,
        };
        return match feature {
            3 | 6 | 7 | 8 | 10 | 12 | 13 | 17 | 23 => 1, // MMX, SSE, SSE2, RDTSC, NX, cmpxchg64, fastfail…
            14 => f("cmpxchg16b") as i32,
            28 => f("rdrand") as i32,
            // PF_SSSE3, PF_SSE4_1, PF_SSE4_2 (Photoshop l'exige), PF_AVX512F (#1195).
            36 => f("ssse3") as i32,
            37 => f("sse4.1") as i32,
            38 => f("sse4.2") as i32,
            41 => f("avx512f") as i32,
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
    // Le handle du thread est signalé et son code enregistré, pour que
    // WaitForSingleObject(hThread) rende la main et que GetExitCodeThread
    // lise `code` ; puis retour au pont de thread (voir kernel32).
    crate::dlls::kernel32::exit_current_thread(code)
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

/// Textes rédigés par Winlux (salle blanche, voir AGENTS.md) : seul le
/// code d'erreur vient de l'API Windows, jamais le libellé.
fn system_message(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "Opération terminée sans erreur.",
        2 => "Fichier introuvable.",
        3 => "Dossier introuvable.",
        5 => "Accès non autorisé.",
        6 => "Handle invalide.",
        8 => "Mémoire insuffisante pour terminer l’opération.",
        32 => "Fichier déjà ouvert par un autre programme.",
        80 => "Ce fichier existe déjà.",
        87 => "Paramètre invalide.",
        122 => "Tampon trop petit pour le résultat.",
        126 => "Bibliothèque (DLL) introuvable.",
        127 => "Fonction absente de la bibliothèque.",
        183 => "Un élément de ce nom existe déjà.",
        267 => "Ce nom ne désigne pas un dossier.",
        // HRESULT de COM et d'OLE Automation les plus courants (#1838).
        0x8000_4001 => "Fonction non implémentée.",
        0x8000_4002 => "Interface non prise en charge.",
        0x8000_4003 => "Pointeur non valide.",
        0x8000_4005 => "Erreur non précisée.",
        0x8002_0003 => "Membre introuvable.",
        0x8002_0006 => "Nom inconnu.",
        0x8002_801D => "Bibliothèque de types non inscrite.",
        0x8002_802B => "Élément introuvable.",
        0x8002_9C4A => "Impossible de charger la bibliothèque de types.",
        0x8004_0154 => "Classe non inscrite.",
        // HRESULT_FROM_WIN32 (0x8007xxxx) : le texte du code Win32 contenu,
        // comme sous Windows (7-Zip passe ainsi ses erreurs, #483).
        c if c & 0xFFFF_0000 == 0x8007_0000 => return system_message(c & 0xFFFF),
        _ => return None,
    })
}

/// Les mêmes messages en anglais, pour un appel qui demande explicitement
/// LANG_ENGLISH (libuv, donc Node et Electron, le fait pour ses erreurs de
/// chargement de module : un texte français en ANSI devenait
/// « Biblioth?que » dans le journal de Discord, #1820). Textes de Winlux.
fn system_message_english(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "The operation completed without error.",
        2 => "File not found.",
        3 => "Folder not found.",
        5 => "Access is not allowed.",
        6 => "Invalid handle.",
        8 => "Not enough memory to complete the operation.",
        32 => "The file is already open in another program.",
        80 => "This file already exists.",
        87 => "Invalid parameter.",
        122 => "The buffer is too small for the result.",
        126 => "Library (DLL) not found.",
        127 => "Function missing from the library.",
        183 => "An item with this name already exists.",
        267 => "This name is not a folder.",
        // Les mêmes HRESULT de COM qu'en français (#1842).
        0x8000_4001 => "Function not implemented.",
        0x8000_4002 => "Interface not supported.",
        0x8000_4003 => "Invalid pointer.",
        0x8000_4005 => "Unspecified error.",
        0x8002_0003 => "Member not found.",
        0x8002_0006 => "Unknown name.",
        0x8002_801D => "Type library not registered.",
        0x8002_802B => "Element not found.",
        0x8002_9C4A => "Cannot load the type library.",
        0x8004_0154 => "Class not registered.",
        c if c & 0xFFFF_0000 == 0x8007_0000 => return system_message_english(c & 0xFFFF),
        _ => return None,
    })
}

/// LANG_ENGLISH (0x09) demandé explicitement dans dwLanguageId.
fn wants_english(lang: u32) -> bool {
    lang & 0x3FF == 0x09
}

const FORMAT_MESSAGE_ALLOCATE_BUFFER: u32 = 0x100;
const FORMAT_MESSAGE_IGNORE_INSERTS: u32 = 0x200;
const FORMAT_MESSAGE_FROM_STRING: u32 = 0x400;
const FORMAT_MESSAGE_FROM_SYSTEM: u32 = 0x1000;

/// `FormatMessage` : messages système (`FROM_SYSTEM`) ou chaîne de format
/// applicative (`FROM_STRING`) dont les insertions `%1`…`%99` (avec un
/// format optionnel `!s!`, `!d!`, `!x!`…) sont lues dans `args`. Sans `args`
/// (pointeur nul) ou avec `IGNORE_INSERTS`, les insertions sont copiées
/// telles quelles au lieu de déréférencer un pointeur nul ; `%n`, `%r`,
/// `%t`, `%%`, `%!`, `%.` et `%0` sont toujours interprétés.
unsafe fn format_message(
    flags: u32,
    source: usize,
    code: u32,
    lang: u32,
    args: usize,
    wide: bool,
) -> Option<String> {
    if flags & FORMAT_MESSAGE_FROM_STRING != 0 {
        if source == 0 {
            return None;
        }
        let template = if wide {
            crate::dlls::wstr(source as *const u16)
        } else {
            crate::dlls::cstr(source as *const u8)
        };
        return Some(expand_inserts(&template, flags, args, wide));
    }
    if flags & FORMAT_MESSAGE_FROM_SYSTEM == 0 {
        return None;
    }
    if wants_english(lang) {
        return Some(format!(
            "{}\r\n",
            system_message_english(code)
                .map(str::to_string)
                .unwrap_or_else(|| if code >= 0x8000_0000 {
                    format!("Windows error 0x{code:08X}.")
                } else {
                    format!("Windows error {code}.")
                })
        ));
    }
    Some(format!(
        "{}\r\n",
        system_message(code)
            .map(str::to_string)
            .unwrap_or_else(|| if code >= 0x8000_0000 {
                // HRESULT : en hexadécimal, comme on le cherche.
                format!("Erreur Windows 0x{code:08X}.")
            } else {
                format!("Erreur Windows {code}.")
            })
    ))
}

/// Valeur du n-ième argument d'insertion (tableau de pointeurs ou va_list,
/// identiques sur x64 : des mots de 8 octets).
unsafe fn insert_arg(args: usize, index: usize) -> Option<usize> {
    if args == 0 || index == 0 {
        return None;
    }
    Some(*(args as *const usize).add(index - 1))
}

unsafe fn expand_inserts(template: &str, flags: u32, args: usize, wide: bool) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&next) = chars.get(i) else {
            break;
        };
        match next {
            '0' => break,
            'n' => {
                out.push_str("\r\n");
                i += 1;
            }
            'r' => {
                out.push('\r');
                i += 1;
            }
            't' => {
                out.push('\t');
                i += 1;
            }
            '%' | '!' | '.' => {
                out.push(next);
                i += 1;
            }
            '1'..='9' => {
                let start = i;
                let mut n = 0usize;
                while i < chars.len() && i - start < 2 && chars[i].is_ascii_digit() {
                    n = n * 10 + chars[i].to_digit(10).unwrap_or(0) as usize;
                    i += 1;
                }
                let mut spec = String::new();
                if chars.get(i) == Some(&'!') {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] != '!' {
                        spec.push(chars[j]);
                        j += 1;
                    }
                    if j < chars.len() {
                        i = j + 1;
                    } else {
                        spec.clear();
                    }
                }
                let verbatim = flags & FORMAT_MESSAGE_IGNORE_INSERTS != 0 || args == 0;
                match insert_arg(args, n).filter(|_| !verbatim) {
                    Some(value) => out.push_str(&format_insert(value, &spec, wide)),
                    None => {
                        out.push('%');
                        out.extend(chars[start..start + n.to_string().len()].iter());
                        if !spec.is_empty() {
                            out.push('!');
                            out.push_str(&spec);
                            out.push('!');
                        }
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Une insertion formatée : chaîne (`s`, par défaut), ou entier selon le
/// format printf réduit (`d`, `i`, `u`, `x`, `X`, `c`, modificateurs `l`/`ll`
/// et largeur ignorée pour l'essentiel).
unsafe fn format_insert(value: usize, spec: &str, wide: bool) -> String {
    let conv = spec
        .chars()
        .rev()
        .find(|c| c.is_ascii_alphabetic())
        .unwrap_or('s');
    match conv {
        's' | 'S' if value == 0 => "(null)".to_string(),
        's' => {
            if wide != (conv == 'S') {
                crate::dlls::wstr(value as *const u16)
            } else {
                crate::dlls::cstr(value as *const u8)
            }
        }
        'S' => {
            if wide {
                crate::dlls::cstr(value as *const u8)
            } else {
                crate::dlls::wstr(value as *const u16)
            }
        }
        'c' | 'C' => char::from_u32(value as u32).unwrap_or('?').to_string(),
        'd' | 'i' => {
            if spec.contains("ll") || spec.contains("I64") {
                (value as i64).to_string()
            } else {
                (value as i32).to_string()
            }
        }
        'u' => {
            if spec.contains("ll") || spec.contains("I64") {
                (value as u64).to_string()
            } else {
                (value as u32).to_string()
            }
        }
        'x' => {
            if spec.contains("ll") || spec.contains("I64") {
                format!("{:x}", value as u64)
            } else {
                format!("{:x}", value as u32)
            }
        }
        'X' => {
            if spec.contains("ll") || spec.contains("I64") {
                format!("{:X}", value as u64)
            } else {
                format!("{:X}", value as u32)
            }
        }
        'p' => format!("{value:016X}"),
        _ => crate::dlls::cstr(value as *const u8),
    }
}
pub unsafe extern "win64" fn format_message_a(
    flags: u32,
    src: usize,
    code: u32,
    lang: u32,
    buf: *mut u8,
    size: u32,
    args: usize,
) -> u32 {
    let Some(text) = format_message(flags, src, code, lang, args, false) else {
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
        crate::wow32::write_ptr(buf as usize, p as usize);
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
    src: usize,
    code: u32,
    lang: u32,
    buf: *mut u16,
    size: u32,
    args: usize,
) -> u32 {
    let Some(text) = format_message(flags, src, code, lang, args, true) else {
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
        crate::wow32::write_ptr(buf as usize, p as usize);
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

// GetUserDefaultLCID, GetUserDefaultUILanguage… : voir crate::intl::api.

/// Winlux n'exécute pas d'applications empaquetées (MSIX/AppX) : les API de
/// paquet répondent `APPMODEL_ERROR_NO_PACKAGE`, ce que les programmes
/// classiques reçoivent aussi sous Windows.
const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
/// Identité de paquet MSIX du programme courant (lancé depuis
/// `C:\Program Files\WindowsApps\<PackageFullName>`), lue une fois.
fn current_package() -> Option<&'static (crate::setup::msix::Package, String)> {
    static PACKAGE: std::sync::OnceLock<Option<(crate::setup::msix::Package, String)>> =
        std::sync::OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            let exe = crate::loader::main_module()?.path;
            let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
            crate::setup::msix::package_of(&exe)
        })
        .as_ref()
}

/// Copie `text` (UTF-16, terminé par zéro) dans un tampon de `*len`
/// caractères, à la manière des fonctions de l'identité de paquet.
unsafe fn package_string(len: *mut u32, buf: *mut u16, text: Option<String>) -> u32 {
    if len.is_null() {
        return 87; // ERROR_INVALID_PARAMETER
    }
    let Some(text) = text else {
        return APPMODEL_ERROR_NO_PACKAGE;
    };
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let needed = wide.len() as u32;
    if buf.is_null() || *len < needed {
        *len = needed;
        return 122; // ERROR_INSUFFICIENT_BUFFER
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    *len = needed;
    0
}

/// Valeur Windows de l'architecture d'un paquet.
fn package_arch(arch: &str) -> u32 {
    match arch {
        "x86" => 0,
        "arm" => 5,
        "x64" => 9,
        "arm64" => 12,
        _ => 11, // neutral
    }
}

/// `GetCurrentPackageId` : structure PACKAGE_ID (48 octets en 64 bits)
/// suivie de ses chaînes, dans le même tampon.
pub unsafe extern "win64" fn get_current_package_id(len: *mut u32, buf: *mut u8) -> u32 {
    if len.is_null() {
        return 87; // ERROR_INVALID_PARAMETER
    }
    let Some((pkg, _)) = current_package() else {
        return APPMODEL_ERROR_NO_PACKAGE;
    };
    let strings = [
        pkg.name.clone(),
        pkg.publisher.clone(),
        pkg.resource_id.clone(),
        pkg.publisher_id(),
    ];
    let wide: Vec<Vec<u16>> = strings
        .iter()
        .map(|s| s.encode_utf16().chain(std::iter::once(0)).collect())
        .collect();
    let needed = 48 + wide.iter().map(|w| w.len() * 2).sum::<usize>();
    if buf.is_null() || (*len as usize) < needed {
        *len = needed as u32;
        return 122; // ERROR_INSUFFICIENT_BUFFER
    }
    let mut v = pkg
        .version
        .split('.')
        .map(|n| n.parse::<u16>().unwrap_or(0));
    let (major, minor, build, revision) = (
        v.next().unwrap_or(0) as u64,
        v.next().unwrap_or(0) as u64,
        v.next().unwrap_or(0) as u64,
        v.next().unwrap_or(0) as u64,
    );
    std::ptr::write_bytes(buf, 0, 48);
    (buf.add(4) as *mut u32).write_unaligned(package_arch(&pkg.arch));
    (buf.add(8) as *mut u64).write_unaligned(major << 48 | minor << 32 | build << 16 | revision);
    let mut at = 48usize;
    for (i, w) in wide.iter().enumerate() {
        let dst = buf.add(at);
        std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, dst, w.len() * 2);
        (buf.add(16 + i * 8) as *mut usize).write_unaligned(dst as usize);
        at += w.len() * 2;
    }
    *len = needed as u32;
    0
}
pub unsafe extern "win64" fn get_current_package_full_name(len: *mut u32, buf: *mut u16) -> u32 {
    package_string(len, buf, current_package().map(|(p, _)| p.full_name()))
}
pub unsafe extern "win64" fn get_current_package_family_name(len: *mut u32, buf: *mut u16) -> u32 {
    package_string(len, buf, current_package().map(|(p, _)| p.family_name()))
}
/// `GetCurrentApplicationUserModelId` : `Famille!IdApplication`.
pub unsafe extern "win64" fn get_current_application_user_model_id(
    len: *mut u32,
    buf: *mut u16,
) -> u32 {
    let id = current_package().map(|(p, _)| {
        let exe = crate::loader::main_module()
            .map(|m| m.name.to_ascii_lowercase())
            .unwrap_or_default();
        let app = p
            .apps
            .iter()
            .find(|a| a.executable.to_ascii_lowercase().ends_with(&exe))
            .or(p.apps.first())
            .map(|a| a.id.clone())
            .unwrap_or_else(|| "App".into());
        format!("{}!{app}", p.family_name())
    });
    package_string(len, buf, id)
}
pub unsafe extern "win64" fn get_current_package_path(len: *mut u32, buf: *mut u16) -> u32 {
    package_string(len, buf, current_package().map(|(_, dir)| dir.clone()))
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
/// Version ANSI : même chemin, converti dans la page de codes du système.
pub unsafe extern "win64" fn get_final_path_name_by_handle_a(
    handle: usize,
    buf: *mut u8,
    len: u32,
    flags: u32,
) -> u32 {
    let mut wide = vec![0u16; 32768];
    let n = get_final_path_name_by_handle_w(handle, wide.as_mut_ptr(), wide.len() as u32, flags);
    if n == 0 {
        return 0;
    }
    let bytes = crate::dlls::encode_ansi(&String::from_utf16_lossy(&wide[..n as usize]));
    if buf.is_null() || (len as usize) <= bytes.len() {
        return bytes.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    *buf.add(bytes.len()) = 0;
    bytes.len() as u32
}

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

/// SetProcessDEPPolicy : la DEP est toujours active sous Linux (NX).
pub extern "win64" fn set_process_dep_policy(_flags: u32) -> i32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetProcessDEPPolicy" => fn set_process_dep_policy,
        "GetErrorMode" => fn get_error_mode, "SetErrorMode" => fn set_error_mode,
        "GetThreadErrorMode" => fn get_thread_error_mode, "SetThreadErrorMode" => fn set_thread_error_mode,
        "WerGetFlags" => fn wer_get_flags, "WerSetFlags" => fn wer_set_flags,
        "WriteConsoleA" => fn write_console_a, "GetConsoleMode" => fn get_console_mode, "SetConsoleMode" => fn set_console_mode,
        "GetConsoleCP" => fn get_console_cp, "GetConsoleOutputCP" => fn get_console_output_cp,
        "SetConsoleCP" => fn set_console_cp, "SetConsoleOutputCP" => fn set_console_output_cp,
        "SetConsoleCtrlHandler" => fn set_console_ctrl_handler, "SetConsoleTitleA" => fn set_console_title_a, "SetConsoleTitleW" => fn set_console_title_w,
        "GetConsoleWindow" => fn get_console_window, "AllocConsole" => fn alloc_console, "FreeConsole" => fn free_console, "AttachConsole" => fn attach_console,
        "GetConsoleScreenBufferInfo" => fn get_console_screen_buffer_info, "SetConsoleTextAttribute" => fn set_console_text_attribute,
        "IsDBCSLeadByte" => fn is_dbcs_lead_byte, "IsDBCSLeadByteEx" => fn is_dbcs_lead_byte_ex,
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
        "GetCurrentPackageId" => fn get_current_package_id,
        "GetCurrentPackageFullName" => fn get_current_package_full_name,
        "GetCurrentPackageFamilyName" => fn get_current_package_family_name,
        "GetCurrentApplicationUserModelId" => fn get_current_application_user_model_id,
        "GetCurrentPackagePath" => fn get_current_package_path,
        "IsWow64Process" => fn is_wow64_process, "LCMapStringEx" => fn lc_map_string_ex,
        "GetFinalPathNameByHandleW" => fn get_final_path_name_by_handle_w,
        "GetFinalPathNameByHandleA" => fn get_final_path_name_by_handle_a,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hresult_win32_donne_le_texte_du_code_contenu() {
        assert_eq!(system_message(0x8007_0005), system_message(5));
        assert!(system_message(0x8007_010B).is_some());
        assert_eq!(system_message(0x8000_1234), None);
    }

    use super::*;

    #[test]
    fn instructions_sse4_et_avx512_suivent_cpuid() {
        // #1195 : chaque constante répond comme CPUID, présente ou non.
        for (pf, name) in [
            (36u32, "ssse3"),
            (37, "sse4.1"),
            (38, "sse4.2"),
            (41, "avx512f"),
        ] {
            let cpu = match name {
                "ssse3" => std::arch::is_x86_feature_detected!("ssse3"),
                "sse4.1" => std::arch::is_x86_feature_detected!("sse4.1"),
                "sse4.2" => std::arch::is_x86_feature_detected!("sse4.2"),
                _ => std::arch::is_x86_feature_detected!("avx512f"),
            };
            assert_eq!(is_processor_feature_present(pf), cpu as i32, "{name}");
        }
        // Numéro inconnu : absent.
        assert_eq!(is_processor_feature_present(999), 0);
    }

    #[test]
    fn texte_de_debogage_accepte() {
        unsafe {
            output_debug_string_a(std::ptr::null());
            output_debug_string_w(std::ptr::null());
            output_debug_string_a(b"crashpad: dump ok\n\0".as_ptr());
            let w: Vec<u16> = "Claude\n".encode_utf16().chain(Some(0)).collect();
            output_debug_string_w(w.as_ptr());
        }
    }

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
                get_current_package_full_name(&mut len, std::ptr::null_mut()),
                15700
            );
        }
    }

    #[test]
    fn filtre_d_exception_et_pages_de_code() {
        let old = set_unhandled_exception_filter(0x1234);
        assert_eq!(set_unhandled_exception_filter(old), 0x1234);
        assert_eq!(set_console_output_cp(12345), 0);
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
            assert!(crate::dlls::wstr(msg.as_ptr()).starts_with("Fichier introuvable"));
            // #1820 : LANG_ENGLISH demandé (libuv) → texte anglais, en ASCII.
            let mut ansi = [0u8; 128];
            let n = format_message_a(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                0,
                126,
                0x0409,
                ansi.as_mut_ptr(),
                128,
                0,
            );
            assert!(n > 0);
            let text = crate::dlls::cstr(ansi.as_ptr());
            assert!(text.starts_with("Library (DLL) not found"), "{text}");
            assert!(ansi[..n as usize].is_ascii());
        }
    }

    /// #1838 : un HRESULT de COM a un texte, sinon il est écrit en
    /// hexadécimal (et non « Erreur Windows 2147647517 »).
    #[test]
    fn messages_des_hresult() {
        unsafe {
            for (code, lang, debut) in [
                (0x8002_801Du32, 0u32, "Bibliothèque de types non inscrite"),
                (0x8004_0154, 0, "Classe non inscrite"),
                (0x8000_1234, 0, "Erreur Windows 0x80001234"),
                // #1842 : en anglais aussi (LANG_ENGLISH demandé).
                (0x8002_801D, 0x0409, "Type library not registered"),
                (0x8004_0154, 0x0409, "Class not registered"),
                (0x8000_1234, 0x0409, "Windows error 0x80001234"),
            ] {
                let mut msg = [0u16; 128];
                let n = format_message_w(
                    FORMAT_MESSAGE_FROM_SYSTEM,
                    0,
                    code,
                    lang,
                    msg.as_mut_ptr(),
                    128,
                    0,
                );
                assert!(n > 0);
                let texte = crate::dlls::wstr(msg.as_ptr());
                assert!(texte.starts_with(debut), "{code:#x} : {texte}");
            }
        }
    }

    /// #1838 : un HRESULT de COM a un texte, sinon il est écrit en
    /// hexadécimal (et non « Erreur Windows 2147647517 »).
    #[test]
    fn messages_des_hresult() {
        unsafe {
            for (code, debut) in [
                (0x8002_801Du32, "Bibliothèque de types non inscrite"),
                (0x8004_0154, "Classe non inscrite"),
                (0x8000_1234, "Erreur Windows 0x80001234"),
            ] {
                let mut msg = [0u16; 128];
                let n = format_message_w(
                    FORMAT_MESSAGE_FROM_SYSTEM,
                    0,
                    code,
                    0,
                    msg.as_mut_ptr(),
                    128,
                    0,
                );
                assert!(n > 0);
                let texte = crate::dlls::wstr(msg.as_ptr());
                assert!(texte.starts_with(debut), "{code:#x} : {texte}");
            }
        }
    }

    #[test]
    fn output_debug_string_garde_les_derniers_messages() {
        let w: Vec<u16> = "qt.qpa.plugin: Could not load\r\n"
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe { output_debug_string_w(w.as_ptr()) };
        unsafe { output_debug_string_a(b"deuxieme\0".as_ptr()) };
        let tail: Vec<String> = crate::dlls::lock(&DEBUG_TAIL).iter().cloned().collect();
        assert!(tail.contains(&"qt.qpa.plugin: Could not load".to_string()));
        assert_eq!(tail.last().map(String::as_str), Some("deuxieme"));
        for n in 0..DEBUG_TAIL_LEN + 5 {
            debug_string(format!("m{n}"));
        }
        assert_eq!(crate::dlls::lock(&DEBUG_TAIL).len(), DEBUG_TAIL_LEN);
        print_debug_tail();
        assert!(crate::dlls::lock(&DEBUG_TAIL).is_empty());
    }
}
