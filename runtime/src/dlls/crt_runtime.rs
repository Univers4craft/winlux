//! Démarrage et services de base de la CRT tels que les attend le code de
//! démarrage des compilateurs Windows (MinGW-w64, MSVC) : `__getmainargs`,
//! `_initterm`, `atexit`/`_onexit`, `_errno`, verrous `_lock`, environnement,
//! temps, nombres aléatoires, E/S bas niveau `_open`/`_read`/`_write`…
//!
//! Ces fonctions sont écrites d'après la documentation publique de la CRT
//! Microsoft; elles s'appuient sur la libc Linux quand la sémantique coïncide.

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

// ---------- Arguments et environnement ----------

static mut WINLUX_ARGC: i32 = 0;
static mut ARGV: *mut *mut u8 = std::ptr::null_mut();
static mut WARGV: *mut *mut u16 = std::ptr::null_mut();
static mut ENVIRON: *mut *mut u8 = std::ptr::null_mut();
static mut WENVIRON: *mut *mut u16 = std::ptr::null_mut();
static mut INITENV: *mut *mut u8 = std::ptr::null_mut();
static mut WINITENV: *mut *mut u16 = std::ptr::null_mut();
static mut FMODE: i32 = 0x4000; // _O_TEXT
static mut COMMODE: i32 = 0;
static mut MB_CUR_MAX: i32 = 1;
static mut APP_TYPE: i32 = 0;

fn leak_narrow(items: &[String]) -> *mut *mut u8 {
    let mut ptrs: Vec<*mut u8> = items.iter().map(|s| {
        let mut b = crate::dlls::encode_ansi(s);
        b.push(0);
        Box::leak(b.into_boxed_slice()).as_mut_ptr()
    }).collect();
    ptrs.push(std::ptr::null_mut());
    Box::leak(ptrs.into_boxed_slice()).as_mut_ptr()
}

fn leak_wide(items: &[String]) -> *mut *mut u16 {
    let mut ptrs: Vec<*mut u16> = items.iter().map(|s| {
        let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
        Box::leak(w.into_boxed_slice()).as_mut_ptr()
    }).collect();
    ptrs.push(std::ptr::null_mut());
    Box::leak(ptrs.into_boxed_slice()).as_mut_ptr()
}

fn arguments() -> Vec<String> {
    let line = crate::process::PROCESS.get()
        .map(|p| String::from_utf16_lossy(&p.command_line_w[..p.command_line_w.len().saturating_sub(1)]))
        .unwrap_or_default();
    crate::dlls::kernel32::split_windows_command_line(&line)
}

fn environment() -> Vec<String> {
    std::env::vars_os().map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy())).collect()
}

/// Prépare une seule fois argc/argv/environ (versions étroites et larges).
fn init_args() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        let args = arguments();
        let env = environment();
        WINLUX_ARGC = args.len() as i32;
        ARGV = leak_narrow(&args);
        WARGV = leak_wide(&args);
        ENVIRON = leak_narrow(&env);
        WENVIRON = leak_wide(&env);
        INITENV = ENVIRON;
        WINITENV = WENVIRON;
    });
}

pub unsafe extern "win64" fn getmainargs(argc: *mut i32, argv: *mut *mut *mut u8, env: *mut *mut *mut u8, _glob: i32, _si: *mut c_void) -> i32 {
    init_args();
    if !argc.is_null() { *argc = WINLUX_ARGC; }
    if !argv.is_null() { *argv = ARGV; }
    if !env.is_null() { *env = ENVIRON; }
    0
}
pub unsafe extern "win64" fn wgetmainargs(argc: *mut i32, argv: *mut *mut *mut u16, env: *mut *mut *mut u16, _glob: i32, _si: *mut c_void) -> i32 {
    init_args();
    if !argc.is_null() { *argc = WINLUX_ARGC; }
    if !argv.is_null() { *argv = WARGV; }
    if !env.is_null() { *env = WENVIRON; }
    0
}
pub unsafe extern "win64" fn p_argc() -> *mut i32 { init_args(); std::ptr::addr_of_mut!(WINLUX_ARGC) }
pub unsafe extern "win64" fn p_argv() -> *mut *mut *mut u8 { init_args(); std::ptr::addr_of_mut!(ARGV) }
pub unsafe extern "win64" fn p_wargv() -> *mut *mut *mut u16 { init_args(); std::ptr::addr_of_mut!(WARGV) }
pub unsafe extern "win64" fn p_environ() -> *mut *mut *mut u8 { init_args(); std::ptr::addr_of_mut!(ENVIRON) }
pub unsafe extern "win64" fn p_wenviron() -> *mut *mut *mut u16 { init_args(); std::ptr::addr_of_mut!(WENVIRON) }
pub unsafe extern "win64" fn p_fmode() -> *mut i32 { std::ptr::addr_of_mut!(FMODE) }
pub unsafe extern "win64" fn p_commode() -> *mut i32 { std::ptr::addr_of_mut!(COMMODE) }
pub unsafe extern "win64" fn set_fmode(mode: i32) -> i32 { FMODE = mode; 0 }
pub unsafe extern "win64" fn get_fmode(mode: *mut i32) -> i32 { if mode.is_null() { return 22; } *mode = FMODE; 0 }
pub unsafe extern "win64" fn set_app_type(t: i32) { APP_TYPE = t; }
pub extern "win64" fn configure_narrow_argv(_mode: i32) -> i32 { init_args(); 0 }
pub extern "win64" fn initialize_narrow_environment() -> i32 { init_args(); 0 }
pub extern "win64" fn set_user_math_err(_handler: usize) {}
pub extern "win64" fn setusermatherr(_handler: usize) {}

/// Appelle les pointeurs de fonction non nuls d'une table `[début, fin[`.
pub unsafe extern "win64" fn initterm(start: *const usize, end: *const usize) {
    let mut p = start;
    while p < end {
        if *p != 0 {
            let f: extern "win64" fn() = std::mem::transmute(*p);
            f();
        }
        p = p.add(1);
    }
}
/// Variante qui s'arrête au premier initialiseur renvoyant une erreur.
pub unsafe extern "win64" fn initterm_e(start: *const usize, end: *const usize) -> i32 {
    let mut p = start;
    while p < end {
        if *p != 0 {
            let f: extern "win64" fn() -> i32 = std::mem::transmute(*p);
            let r = f();
            if r != 0 { return r; }
        }
        p = p.add(1);
    }
    0
}

// ---------- Fin de programme ----------

fn atexit_list() -> &'static Mutex<Vec<usize>> {
    static L: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    &L
}

/// Exécute les fonctions enregistrées par `atexit`/`_onexit`, la dernière
/// enregistrée en premier, une seule fois.
pub fn run_atexit() {
    loop {
        let next = atexit_list().lock().unwrap_or_else(std::sync::PoisonError::into_inner).pop();
        match next {
            Some(f) => unsafe {
                let f: extern "win64" fn() = std::mem::transmute(f);
                f();
            },
            None => break,
        }
    }
}

pub extern "win64" fn atexit(f: usize) -> i32 {
    if f == 0 { return -1; }
    atexit_list().lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f);
    0
}
pub extern "win64" fn onexit(f: usize) -> usize { if atexit(f) == 0 { f } else { 0 } }
pub extern "win64" fn dllonexit(f: usize, _begin: usize, _end: usize) -> usize { onexit(f) }
pub extern "win64" fn crt_atexit(f: usize) -> i32 { atexit(f) }

/// Table `_onexit_table_t` de la CRT universelle : trois pointeurs
/// (premier, dernier, fin de capacité) vers un tableau de fonctions.
#[repr(C)]
pub struct OnexitTable { first: *mut usize, last: *mut usize, end: *mut usize }

pub unsafe extern "win64" fn initialize_onexit_table(t: *mut OnexitTable) -> i32 {
    if t.is_null() { return -1; }
    (*t).first = std::ptr::null_mut();
    (*t).last = std::ptr::null_mut();
    (*t).end = std::ptr::null_mut();
    0
}
pub unsafe extern "win64" fn register_onexit_function(t: *mut OnexitTable, f: usize) -> i32 {
    if t.is_null() { return -1; }
    let t = &mut *t;
    if t.last == t.end {
        let len = if t.first.is_null() { 0 } else { t.last.offset_from(t.first) as usize };
        let cap = (len * 2).max(32);
        let p = libc::realloc(t.first as *mut c_void, cap * 8) as *mut usize;
        if p.is_null() { return -1; }
        t.first = p;
        t.last = p.add(len);
        t.end = p.add(cap);
    }
    *t.last = f;
    t.last = t.last.add(1);
    0
}
pub unsafe extern "win64" fn execute_onexit_table(t: *mut OnexitTable) -> i32 {
    if t.is_null() { return -1; }
    let (first, mut last) = ((*t).first, (*t).last);
    initialize_onexit_table(t);
    while !first.is_null() && last > first {
        last = last.sub(1);
        if *last != 0 {
            let f: extern "win64" fn() = std::mem::transmute(*last);
            f();
        }
    }
    libc::free(first as *mut c_void);
    0
}

pub extern "win64" fn cexit() { run_atexit(); let _ = std::io::Write::flush(&mut std::io::stdout()); }
pub extern "win64" fn c_exit() { let _ = std::io::Write::flush(&mut std::io::stdout()); }
pub extern "win64" fn exit(code: i32) -> ! { run_atexit(); crate::dlls::kernel32::flush_and_exit(code) }
pub extern "win64" fn quick_exit(code: i32) -> ! { crate::dlls::kernel32::flush_and_exit(code) }
pub extern "win64" fn amsg_exit(code: i32) -> ! {
    eprintln!("winlux: erreur d'exécution de la CRT R60{code:02}");
    crate::dlls::kernel32::flush_and_exit(255)
}
/// `abort` : code de sortie 3 comme sous Windows, sans exécuter `atexit`.
pub extern "win64" fn abort() -> ! {
    if let Some(h) = signal_handler(SIGABRT) {
        unsafe { let f: extern "win64" fn(i32) = std::mem::transmute(h); f(SIGABRT); }
    }
    eprintln!("\nabnormal program termination");
    crate::dlls::kernel32::flush_and_exit(3)
}

// ---------- Signaux (sous-ensemble Windows) ----------

const SIGABRT: i32 = 22;
const SIG_DFL: usize = 0;
const SIG_IGN: usize = 1;
const SIG_ERR: usize = usize::MAX;

fn signals() -> &'static Mutex<HashMap<i32, usize>> {
    static S: OnceLock<Mutex<HashMap<i32, usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}
fn signal_handler(sig: i32) -> Option<usize> {
    signals().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&sig).copied().filter(|&h| h != SIG_DFL && h != SIG_IGN)
}
pub extern "win64" fn signal(sig: i32, handler: usize) -> usize {
    // SIGINT 2, SIGILL 4, SIGFPE 8, SIGSEGV 11, SIGTERM 15, SIGBREAK 21, SIGABRT 22/6.
    if ![2, 4, 6, 8, 11, 15, 21, 22].contains(&sig) {
        unsafe { *errno() = 22; }
        return SIG_ERR;
    }
    let sig = if sig == 6 { SIGABRT } else { sig };
    signals().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(sig, handler).unwrap_or(SIG_DFL)
}
pub extern "win64" fn raise(sig: i32) -> i32 {
    let sig = if sig == 6 { SIGABRT } else { sig };
    let handler = signals().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&sig).copied().unwrap_or(SIG_DFL);
    match handler {
        SIG_IGN => 0,
        SIG_DFL => {
            if sig == SIGABRT { abort() }
            crate::dlls::kernel32::flush_and_exit(3)
        }
        h => {
            signals().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(sig, SIG_DFL);
            unsafe { let f: extern "win64" fn(i32) = std::mem::transmute(h); f(sig); }
            0
        }
    }
}

// ---------- errno et verrous internes ----------

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
    static DOSERRNO: Cell<u32> = const { Cell::new(0) };
}

pub extern "win64" fn errno() -> *mut i32 { ERRNO.with(|e| e.as_ptr()) }
pub extern "win64" fn doserrno() -> *mut u32 { DOSERRNO.with(|e| e.as_ptr()) }
pub unsafe extern "win64" fn get_errno(out: *mut i32) -> i32 { if out.is_null() { return 22; } *out = *errno(); 0 }
pub unsafe extern "win64" fn set_errno(v: i32) -> i32 { *errno() = v; 0 }

/// Convertit un errno Linux en errno de la CRT Windows (mêmes valeurs pour
/// les codes POSIX classiques, différentes au-delà de ERANGE).
pub fn crt_errno(e: i32) -> i32 {
    match e {
        libc::EDEADLK => 36,
        libc::ENAMETOOLONG => 38,
        libc::ENOLCK => 39,
        libc::ENOSYS => 40,
        libc::ENOTEMPTY => 41,
        libc::EILSEQ => 42,
        e if e <= 34 => e,
        _ => 22,
    }
}
fn set_errno_from_os() { unsafe { *errno() = crt_errno(*libc::__errno_location()); } }

fn crt_locks() -> &'static [libc::pthread_mutex_t; 64] {
    static LOCKS: OnceLock<usize> = OnceLock::new();
    let p = *LOCKS.get_or_init(|| unsafe {
        let locks: &'static mut [libc::pthread_mutex_t; 64] = Box::leak(Box::new(std::mem::zeroed()));
        let mut attr: libc::pthread_mutexattr_t = std::mem::zeroed();
        libc::pthread_mutexattr_init(&mut attr);
        libc::pthread_mutexattr_settype(&mut attr, libc::PTHREAD_MUTEX_RECURSIVE);
        for l in locks.iter_mut() { libc::pthread_mutex_init(l, &attr); }
        locks as *mut _ as usize
    });
    unsafe { &*(p as *const [libc::pthread_mutex_t; 64]) }
}
pub unsafe extern "win64" fn lock(n: i32) {
    if let Some(l) = crt_locks().get(n as usize) { libc::pthread_mutex_lock(l as *const _ as *mut _); }
}
pub unsafe extern "win64" fn unlock(n: i32) {
    if let Some(l) = crt_locks().get(n as usize) { libc::pthread_mutex_unlock(l as *const _ as *mut _); }
}

// ---------- Mémoire ----------

pub unsafe extern "win64" fn calloc(n: usize, size: usize) -> *mut c_void { libc::calloc(n, size) }
pub unsafe extern "win64" fn realloc(p: *mut c_void, size: usize) -> *mut c_void { libc::realloc(p, size) }
pub unsafe extern "win64" fn msize(p: *mut c_void) -> usize { if p.is_null() { usize::MAX } else { libc::malloc_usable_size(p) } }
pub unsafe extern "win64" fn aligned_malloc(size: usize, align: usize) -> *mut c_void {
    if align == 0 || !align.is_power_of_two() { *errno() = 22; return std::ptr::null_mut(); }
    let mut p = std::ptr::null_mut();
    if libc::posix_memalign(&mut p, align.max(8), size.max(1)) != 0 { *errno() = 12; return std::ptr::null_mut(); }
    p
}
pub unsafe extern "win64" fn aligned_free(p: *mut c_void) { libc::free(p) }
pub unsafe extern "win64" fn aligned_realloc(p: *mut c_void, size: usize, align: usize) -> *mut c_void {
    let q = aligned_malloc(size, align);
    if !q.is_null() && !p.is_null() {
        std::ptr::copy_nonoverlapping(p as *const u8, q as *mut u8, libc::malloc_usable_size(p).min(size));
        libc::free(p);
    }
    q
}

// ---------- Locale « C » ----------

#[repr(C)]
struct Lconv { text: [*const u8; 10], chars: [u8; 8], wide: [*const u16; 8] }
unsafe impl Sync for Lconv {}
static DOT: [u8; 2] = *b".\0";
static EMPTY: [u8; 1] = [0];
static WDOT: [u16; 2] = [b'.' as u16, 0];
static WEMPTY: [u16; 1] = [0];
static LCONV: Lconv = Lconv {
    text: [DOT.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr(),
           EMPTY.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr(), EMPTY.as_ptr()],
    chars: [127; 8],
    wide: [WDOT.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr(), WEMPTY.as_ptr()],
};
static LOCALE_C: [u8; 2] = *b"C\0";
static WLOCALE_C: [u16; 2] = [b'C' as u16, 0];
static LC_HANDLES: [u32; 6] = [0; 6];

pub extern "win64" fn localeconv() -> *const c_void { &LCONV as *const Lconv as *const c_void }
/// Seule la locale « C » est disponible : toute demande la renvoie.
pub unsafe extern "win64" fn setlocale(_cat: i32, locale: *const u8) -> *const u8 {
    if locale.is_null() { return LOCALE_C.as_ptr(); }
    let name = crate::dlls::cstr(locale);
    if name.is_empty() || name == "C" || name == "POSIX" || name.eq_ignore_ascii_case(".utf8") || name.eq_ignore_ascii_case(".ACP") {
        LOCALE_C.as_ptr()
    } else {
        std::ptr::null()
    }
}
pub unsafe extern "win64" fn wsetlocale(_cat: i32, locale: *const u16) -> *const u16 {
    if locale.is_null() { return WLOCALE_C.as_ptr(); }
    let s = crate::dlls::wstr(locale);
    if s.is_empty() || s == "C" { WLOCALE_C.as_ptr() } else { std::ptr::null() }
}
pub extern "win64" fn lc_codepage_func() -> u32 { 0 }
pub extern "win64" fn mb_cur_max_func() -> i32 { 1 }
pub extern "win64" fn lc_handle_func() -> *const u32 { LC_HANDLES.as_ptr() }
pub extern "win64" fn lc_collate_cp_func() -> u32 { 0 }

// ---------- Gestion des exceptions C ----------

/// Gestionnaire de langage des blocs `__try/__except`. Winlux ne distribue
/// pas encore d'exceptions structurées vers le programme : il n'est donc
/// appelé que par un futur dispatcher, et laisse chercher plus loin.
pub extern "win64" fn c_specific_handler(_rec: usize, _frame: usize, _ctx: usize, _disp: usize) -> i32 { 1 }

// ---------- Environnement ----------

fn getenv_cache() -> &'static Mutex<HashMap<String, Box<[u8]>>> {
    static C: OnceLock<Mutex<HashMap<String, Box<[u8]>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}
fn wgetenv_cache() -> &'static Mutex<HashMap<String, Box<[u16]>>> {
    static C: OnceLock<Mutex<HashMap<String, Box<[u16]>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Le pointeur renvoyé reste valide jusqu'au prochain changement de la même
/// variable, comme le garantit la CRT Microsoft.
pub unsafe extern "win64" fn getenv(name: *const u8) -> *const u8 {
    if name.is_null() { return std::ptr::null(); }
    let n = crate::dlls::cstr(name);
    let Some(v) = crate::dlls::kernel32::env_value(&n) else { return std::ptr::null(); };
    let mut cache = getenv_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = n.to_ascii_uppercase();
    let mut bytes = crate::dlls::encode_ansi(&v);
    bytes.push(0);
    if cache.get(&key).map(|b| &b[..]) != Some(&bytes[..]) {
        cache.insert(key.clone(), bytes.into_boxed_slice());
    }
    cache[&key].as_ptr()
}
pub unsafe extern "win64" fn wgetenv(name: *const u16) -> *const u16 {
    if name.is_null() { return std::ptr::null(); }
    let n = crate::dlls::wstr(name);
    let Some(v) = crate::dlls::kernel32::env_value(&n) else { return std::ptr::null(); };
    let mut cache = wgetenv_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = n.to_ascii_uppercase();
    let w: Vec<u16> = v.encode_utf16().chain([0]).collect();
    if cache.get(&key).map(|b| &b[..]) != Some(&w[..]) {
        cache.insert(key.clone(), w.into_boxed_slice());
    }
    cache[&key].as_ptr()
}
fn putenv_str(s: &str) -> i32 {
    let Some((k, v)) = s.split_once('=') else { return -1; };
    if k.is_empty() { return -1; }
    if v.is_empty() { std::env::remove_var(k); } else { std::env::set_var(k, v); }
    0
}
pub unsafe extern "win64" fn putenv(s: *const u8) -> i32 { if s.is_null() { -1 } else { putenv_str(&crate::dlls::cstr(s)) } }
pub unsafe extern "win64" fn wputenv(s: *const u16) -> i32 { if s.is_null() { -1 } else { putenv_str(&crate::dlls::wstr(s)) } }
pub unsafe extern "win64" fn putenv_s(k: *const u8, v: *const u8) -> i32 {
    if k.is_null() || v.is_null() { return 22; }
    if putenv_str(&format!("{}={}", crate::dlls::cstr(k), crate::dlls::cstr(v))) == 0 { 0 } else { 22 }
}

// ---------- Temps ----------

/// `struct tm` Windows : neuf `int`, sans les champs supplémentaires de glibc.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Tm { sec: i32, min: i32, hour: i32, mday: i32, mon: i32, year: i32, wday: i32, yday: i32, isdst: i32 }

fn to_libc(t: &Tm) -> libc::tm {
    let mut out: libc::tm = unsafe { std::mem::zeroed() };
    out.tm_sec = t.sec; out.tm_min = t.min; out.tm_hour = t.hour; out.tm_mday = t.mday;
    out.tm_mon = t.mon; out.tm_year = t.year; out.tm_wday = t.wday; out.tm_yday = t.yday; out.tm_isdst = t.isdst;
    out
}
fn from_libc(t: &libc::tm) -> Tm {
    Tm { sec: t.tm_sec, min: t.tm_min, hour: t.tm_hour, mday: t.tm_mday, mon: t.tm_mon, year: t.tm_year, wday: t.tm_wday, yday: t.tm_yday, isdst: t.tm_isdst }
}
thread_local! { static TM_BUF: Cell<Tm> = Cell::new(Tm::default()); }

pub unsafe extern "win64" fn time64(out: *mut i64) -> i64 {
    let t = libc::time(std::ptr::null_mut()) as i64;
    if !out.is_null() { *out = t; }
    t
}
pub unsafe extern "win64" fn time32(out: *mut i32) -> i32 {
    let t = libc::time(std::ptr::null_mut()) as i32;
    if !out.is_null() { *out = t; }
    t
}
unsafe fn convert(t: *const i64, local: bool) -> *mut Tm {
    if t.is_null() || *t < 0 { *errno() = 22; return std::ptr::null_mut(); }
    let secs = *t as libc::time_t;
    let mut out: libc::tm = std::mem::zeroed();
    let ok = if local { !libc::localtime_r(&secs, &mut out).is_null() } else { !libc::gmtime_r(&secs, &mut out).is_null() };
    if !ok { *errno() = 22; return std::ptr::null_mut(); }
    TM_BUF.with(|b| { b.set(from_libc(&out)); b.as_ptr() })
}
pub unsafe extern "win64" fn localtime64(t: *const i64) -> *mut Tm { convert(t, true) }
pub unsafe extern "win64" fn gmtime64(t: *const i64) -> *mut Tm { convert(t, false) }
pub unsafe extern "win64" fn localtime32(t: *const i32) -> *mut Tm { if t.is_null() { return std::ptr::null_mut(); } let v = *t as i64; convert(&v, true) }
pub unsafe extern "win64" fn gmtime32(t: *const i32) -> *mut Tm { if t.is_null() { return std::ptr::null_mut(); } let v = *t as i64; convert(&v, false) }
pub unsafe extern "win64" fn localtime64_s(out: *mut Tm, t: *const i64) -> i32 {
    if out.is_null() { return 22; }
    let r = convert(t, true);
    if r.is_null() { return 22; }
    *out = *r;
    0
}
pub unsafe extern "win64" fn gmtime64_s(out: *mut Tm, t: *const i64) -> i32 {
    if out.is_null() { return 22; }
    let r = convert(t, false);
    if r.is_null() { return 22; }
    *out = *r;
    0
}
pub unsafe extern "win64" fn mktime64(t: *mut Tm) -> i64 {
    if t.is_null() { return -1; }
    let mut l = to_libc(&*t);
    let r = libc::mktime(&mut l);
    if r != -1 { *t = from_libc(&l); }
    r as i64
}
pub unsafe extern "win64" fn mkgmtime64(t: *mut Tm) -> i64 {
    if t.is_null() { return -1; }
    let mut l = to_libc(&*t);
    let r = libc::timegm(&mut l);
    if r != -1 { *t = from_libc(&l); }
    r as i64
}
pub extern "win64" fn difftime64(a: i64, b: i64) -> f64 { (a - b) as f64 }
/// `clock` : temps écoulé depuis le démarrage du processus, en millisecondes
/// (CLOCKS_PER_SEC vaut 1000 sous Windows).
pub extern "win64" fn clock() -> i32 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as i32
}
pub unsafe extern "win64" fn strftime(buf: *mut u8, max: usize, fmt: *const u8, t: *const Tm) -> usize {
    if buf.is_null() || fmt.is_null() || t.is_null() { *errno() = 22; return 0; }
    let l = to_libc(&*t);
    libc::strftime(buf as *mut libc::c_char, max, fmt as *const libc::c_char, &l)
}
pub unsafe extern "win64" fn asctime(t: *const Tm) -> *const u8 {
    thread_local! { static BUF: Cell<[u8; 32]> = const { Cell::new([0; 32]) }; }
    if t.is_null() { return std::ptr::null(); }
    let l = to_libc(&*t);
    BUF.with(|b| { libc::asctime_r(&l, b.as_ptr() as *mut libc::c_char); b.as_ptr() as *const u8 })
}
pub unsafe extern "win64" fn ctime64(t: *const i64) -> *const u8 { let tm = localtime64(t); if tm.is_null() { std::ptr::null() } else { asctime(tm) } }

// ---------- Nombres pseudo-aléatoires (générateur de la CRT Microsoft) ----------

thread_local! { static SEED: Cell<u32> = const { Cell::new(1) }; }
pub extern "win64" fn srand(seed: u32) { SEED.with(|s| s.set(seed)); }
pub extern "win64" fn rand() -> i32 {
    SEED.with(|s| {
        let v = s.get().wrapping_mul(214_013).wrapping_add(2_531_011);
        s.set(v);
        ((v >> 16) & 0x7fff) as i32
    })
}
pub unsafe extern "win64" fn rand_s(out: *mut u32) -> i32 {
    if out.is_null() { return 22; }
    let mut v = 0u32;
    if libc::getrandom(&mut v as *mut u32 as *mut c_void, 4, 0) != 4 { return 22; }
    *out = v;
    0
}

// ---------- Chaînes et messages d'erreur ----------

static ERROR_TEXT: [&str; 43] = [
    "No error", "Operation not permitted", "No such file or directory", "No such process",
    "Interrupted function call", "Input/output error", "No such device or address", "Arg list too long",
    "Exec format error", "Bad file descriptor", "No child processes", "Resource temporarily unavailable",
    "Not enough space", "Permission denied", "Bad address", "Unknown error", "Resource device",
    "File exists", "Improper link", "No such device", "Not a directory", "Is a directory",
    "Invalid argument", "Too many open files in system", "Too many open files",
    "Inappropriate I/O control operation", "Unknown error", "File too large", "No space left on device",
    "Invalid seek", "Read-only file system", "Too many links", "Broken pipe", "Domain error",
    "Result too large", "Unknown error", "Resource deadlock avoided", "Unknown error", "Filename too long",
    "No locks available", "Function not implemented", "Directory not empty", "Illegal byte sequence",
];
fn error_text(n: i32) -> &'static str { ERROR_TEXT.get(n as usize).copied().unwrap_or("Unknown error") }

pub extern "win64" fn strerror(n: i32) -> *const u8 {
    static TABLE: OnceLock<Vec<Box<[u8]>>> = OnceLock::new();
    let t = TABLE.get_or_init(|| (0..44).map(|i| {
        let mut b = if i < 43 { error_text(i).as_bytes().to_vec() } else { b"Unknown error".to_vec() };
        b.push(0);
        b.into_boxed_slice()
    }).collect());
    t[(n.clamp(0, 43)) as usize].as_ptr()
}
pub unsafe extern "win64" fn strerror_s(buf: *mut u8, size: usize, n: i32) -> i32 {
    if buf.is_null() || size == 0 { return 22; }
    let text = error_text(n).as_bytes();
    let len = text.len().min(size - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, len);
    *buf.add(len) = 0;
    0
}
pub unsafe extern "win64" fn perror(prefix: *const u8) {
    let msg = error_text(*errno());
    let p = if prefix.is_null() { String::new() } else { crate::dlls::cstr(prefix) };
    let _ = std::io::Write::flush(&mut std::io::stdout());
    if p.is_empty() { eprintln!("{msg}"); } else { eprintln!("{p}: {msg}"); }
}

pub unsafe extern "win64" fn strspn(s: *const u8, set: *const u8) -> usize { libc::strspn(s as _, set as _) }
pub unsafe extern "win64" fn strcspn(s: *const u8, set: *const u8) -> usize { libc::strcspn(s as _, set as _) }
pub unsafe extern "win64" fn strpbrk(s: *const u8, set: *const u8) -> *const u8 { libc::strpbrk(s as _, set as _) as *const u8 }
pub unsafe extern "win64" fn memchr(s: *const c_void, c: i32, n: usize) -> *const c_void { libc::memchr(s, c, n) }
pub unsafe extern "win64" fn strcoll(a: *const u8, b: *const u8) -> i32 { libc::strcmp(a as _, b as _) }
pub unsafe extern "win64" fn strtok(s: *mut u8, delim: *const u8) -> *mut u8 {
    thread_local! { static NEXT: Cell<*mut u8> = const { Cell::new(std::ptr::null_mut()) }; }
    NEXT.with(|next| {
        let mut save = next.get();
        let r = libc::strtok_r(s as _, delim as _, &mut save as *mut *mut u8 as *mut *mut libc::c_char);
        next.set(save);
        r as *mut u8
    })
}
pub unsafe extern "win64" fn atof(s: *const u8) -> f64 { libc::strtod(s as _, std::ptr::null_mut()) }
pub extern "win64" fn ldexp(x: f64, e: i32) -> f64 { unsafe { libc_ldexp(x, e) } }
pub unsafe extern "win64" fn frexp(x: f64, e: *mut i32) -> f64 { libc_frexp(x, e) }
pub unsafe extern "win64" fn modf(x: f64, ip: *mut f64) -> f64 { let i = x.trunc(); if !ip.is_null() { *ip = i; } x - i }
pub extern "win64" fn isnan(x: f64) -> i32 { x.is_nan() as i32 }
pub extern "win64" fn finite(x: f64) -> i32 { x.is_finite() as i32 }
extern "C" {
    #[link_name = "ldexp"] fn libc_ldexp(x: f64, e: i32) -> f64;
    #[link_name = "frexp"] fn libc_frexp(x: f64, e: *mut i32) -> f64;
}

// ---------- Conversions multioctets (locale « C » : un octet par caractère) ----------

pub unsafe extern "win64" fn mbstowcs(dst: *mut u16, src: *const u8, n: usize) -> usize {
    if src.is_null() { return usize::MAX; }
    let len = crate::printf::narrow_len(src);
    if dst.is_null() { return len; }
    let m = len.min(n);
    for i in 0..m { *dst.add(i) = *src.add(i) as u16; }
    if m < n { *dst.add(m) = 0; }
    m
}
pub unsafe extern "win64" fn wcstombs(dst: *mut u8, src: *const u16, n: usize) -> usize {
    if src.is_null() { return usize::MAX; }
    let len = crate::printf::wide_len(src);
    for i in 0..len { if *src.add(i) > 0xff { *errno() = 42; return usize::MAX; } }
    if dst.is_null() { return len; }
    let m = len.min(n);
    for i in 0..m { *dst.add(i) = *src.add(i) as u8; }
    if m < n { *dst.add(m) = 0; }
    m
}
pub unsafe extern "win64" fn mbtowc(dst: *mut u16, src: *const u8, n: usize) -> i32 {
    if src.is_null() { return 0; }
    if n == 0 { return -1; }
    if !dst.is_null() { *dst = *src as u16; }
    (*src != 0) as i32
}
pub unsafe extern "win64" fn wctomb(dst: *mut u8, wc: u16) -> i32 {
    if dst.is_null() { return 0; }
    if wc > 0xff { *errno() = 42; return -1; }
    *dst = wc as u8;
    1
}
pub unsafe extern "win64" fn mblen(s: *const u8, n: usize) -> i32 {
    if s.is_null() { return 0; }
    if n == 0 { return -1; }
    (*s != 0) as i32
}

// ---------- E/S bas niveau (descripteurs de la CRT) ----------

const O_WRONLY: i32 = 1;
const O_RDWR: i32 = 2;
const O_APPEND: i32 = 8;
const O_CREAT: i32 = 0x100;
const O_TRUNC: i32 = 0x200;
const O_EXCL: i32 = 0x400;

fn linux_flags(flags: i32) -> i32 {
    let mut f = match flags & 3 { O_WRONLY => libc::O_WRONLY, O_RDWR => libc::O_RDWR, _ => libc::O_RDONLY };
    if flags & O_APPEND != 0 { f |= libc::O_APPEND; }
    if flags & O_CREAT != 0 { f |= libc::O_CREAT; }
    if flags & O_TRUNC != 0 { f |= libc::O_TRUNC; }
    if flags & O_EXCL != 0 { f |= libc::O_EXCL; }
    f | libc::O_CLOEXEC
}
fn open_path(path: &str, flags: i32, mode: u32) -> i32 {
    let linux = crate::paths::to_linux(path);
    let c = match std::ffi::CString::new(linux.to_string_lossy().as_bytes()) { Ok(c) => c, Err(_) => { unsafe { *errno() = 22; } return -1; } };
    // _S_IWRITE (0x80) absent : fichier en lecture seule.
    let perm = if mode & 0x80 != 0 || flags & O_CREAT == 0 { 0o666 } else { 0o444 };
    let fd = unsafe { libc::open(c.as_ptr(), linux_flags(flags), perm) };
    if fd < 0 { set_errno_from_os(); }
    fd
}
pub unsafe extern "win64" fn open(path: *const u8, flags: i32, mode: u32) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    open_path(&crate::dlls::cstr(path), flags, mode)
}
pub unsafe extern "win64" fn wopen(path: *const u16, flags: i32, mode: u32) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    open_path(&crate::dlls::wstr(path), flags, mode)
}
pub unsafe extern "win64" fn sopen(path: *const u8, flags: i32, _share: i32, mode: u32) -> i32 { open(path, flags, mode) }
pub unsafe extern "win64" fn sopen_s(fd: *mut i32, path: *const u8, flags: i32, _share: i32, mode: u32) -> i32 {
    if fd.is_null() { return 22; }
    *fd = open(path, flags, mode);
    if *fd < 0 { *errno() } else { 0 }
}
pub unsafe extern "win64" fn close(fd: i32) -> i32 {
    if fd <= 2 { return 0; }
    let r = libc::close(fd);
    if r < 0 { set_errno_from_os(); }
    r
}
pub unsafe extern "win64" fn read(fd: i32, buf: *mut c_void, n: u32) -> i32 {
    if fd == 0 { let _ = std::io::Write::flush(&mut std::io::stdout()); }
    let r = libc::read(fd, buf, n as usize);
    if r < 0 { set_errno_from_os(); }
    r as i32
}
pub unsafe extern "win64" fn write(fd: i32, buf: *const c_void, n: u32) -> i32 {
    // Les sorties standard passent par les mêmes tampons que printf.
    if fd == 1 || fd == 2 {
        let b = std::slice::from_raw_parts(buf as *const u8, n as usize);
        let ok = if fd == 1 { std::io::Write::write_all(&mut std::io::stdout(), b) } else { std::io::Write::write_all(&mut std::io::stderr(), b) };
        return if ok.is_ok() { n as i32 } else { *errno() = 28; -1 };
    }
    let r = libc::write(fd, buf, n as usize);
    if r < 0 { set_errno_from_os(); }
    r as i32
}
pub unsafe extern "win64" fn lseek(fd: i32, off: i32, whence: i32) -> i32 { lseeki64(fd, off as i64, whence) as i32 }
pub unsafe extern "win64" fn lseeki64(fd: i32, off: i64, whence: i32) -> i64 {
    let r = libc::lseek(fd, off, whence);
    if r < 0 { set_errno_from_os(); }
    r
}
pub unsafe extern "win64" fn isatty(fd: i32) -> i32 { (libc::isatty(fd) == 1) as i32 }
pub unsafe extern "win64" fn dup(fd: i32) -> i32 { let r = libc::dup(fd); if r < 0 { set_errno_from_os(); } r }
pub unsafe extern "win64" fn dup2(a: i32, b: i32) -> i32 { let r = libc::dup2(a, b); if r < 0 { set_errno_from_os(); -1 } else { 0 } }
pub unsafe extern "win64" fn setmode(_fd: i32, _mode: i32) -> i32 { 0x4000 }
pub unsafe extern "win64" fn access(path: *const u8, mode: i32) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    let linux = crate::paths::to_linux(&crate::dlls::cstr(path));
    let c = match std::ffi::CString::new(linux.to_string_lossy().as_bytes()) { Ok(c) => c, Err(_) => return -1 };
    let m = (if mode & 2 != 0 { libc::W_OK } else { 0 }) | (if mode & 4 != 0 { libc::R_OK } else { 0 });
    let r = libc::access(c.as_ptr(), if m == 0 { libc::F_OK } else { m });
    if r < 0 { set_errno_from_os(); }
    r
}
pub unsafe extern "win64" fn unlink(path: *const u8) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    match std::fs::remove_file(crate::paths::to_linux(&crate::dlls::cstr(path))) { Ok(()) => 0, Err(e) => { *errno() = crt_errno(e.raw_os_error().unwrap_or(22)); -1 } }
}
pub unsafe extern "win64" fn mkdir(path: *const u8) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    match std::fs::create_dir(crate::paths::to_linux(&crate::dlls::cstr(path))) { Ok(()) => 0, Err(e) => { *errno() = crt_errno(e.raw_os_error().unwrap_or(22)); -1 } }
}
pub unsafe extern "win64" fn rmdir(path: *const u8) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    match std::fs::remove_dir(crate::paths::to_linux(&crate::dlls::cstr(path))) { Ok(()) => 0, Err(e) => { *errno() = crt_errno(e.raw_os_error().unwrap_or(22)); -1 } }
}
pub unsafe extern "win64" fn chdir(path: *const u8) -> i32 {
    if path.is_null() { *errno() = 22; return -1; }
    match std::env::set_current_dir(crate::paths::to_linux(&crate::dlls::cstr(path))) { Ok(()) => 0, Err(e) => { *errno() = crt_errno(e.raw_os_error().unwrap_or(22)); -1 } }
}
pub unsafe extern "win64" fn getcwd(buf: *mut u8, size: i32) -> *mut u8 {
    let cwd = match std::env::current_dir() { Ok(p) => crate::paths::to_windows(&p), Err(_) => { *errno() = 2; return std::ptr::null_mut(); } };
    let mut b = crate::dlls::encode_ansi(&cwd);
    b.push(0);
    if buf.is_null() {
        let p = libc::malloc(b.len().max(size.max(0) as usize)) as *mut u8;
        if p.is_null() { *errno() = 12; return p; }
        std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
        return p;
    }
    if (size as usize) < b.len() { *errno() = 34; return std::ptr::null_mut(); }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    buf
}
pub extern "win64" fn get_osfhandle(fd: i32) -> isize {
    match fd { 0 => -10, 1 => -11, 2 => -12, _ => { unsafe { *errno() = 9; } -1 } }
}
pub extern "win64" fn getpid() -> i32 { std::process::id() as i32 }

// ---------- Threads ----------

pub unsafe extern "win64" fn beginthreadex(attrs: usize, stack: u32, start: usize, arg: usize, flags: u32, id: *mut u32) -> usize {
    let h = crate::dlls::kernel32::create_thread(attrs, stack as usize, start, arg, flags, id);
    if h == 0 { *errno() = 11; }
    h
}
pub unsafe extern "win64" fn beginthread(start: usize, stack: u32, arg: usize) -> usize {
    let h = crate::dlls::kernel32::create_thread(0, stack as usize, start, arg, 0, std::ptr::null_mut());
    if h == 0 { *errno() = 11; usize::MAX } else { h }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__getmainargs" => fn getmainargs, "__wgetmainargs" => fn wgetmainargs,
        "__argc" => data WINLUX_ARGC, "__argv" => data ARGV, "__wargv" => data WARGV,
        "_environ" => data ENVIRON, "_wenviron" => data WENVIRON,
        "__initenv" => data INITENV, "__winitenv" => data WINITENV,
        "_fmode" => data FMODE, "_commode" => data COMMODE, "__mb_cur_max" => data MB_CUR_MAX,
        "__p___argc" => fn p_argc, "__p___argv" => fn p_argv, "__p___wargv" => fn p_wargv,
        "__p__environ" => fn p_environ, "__p__wenviron" => fn p_wenviron,
        "__p__fmode" => fn p_fmode, "__p__commode" => fn p_commode, "_set_fmode" => fn set_fmode, "_get_fmode" => fn get_fmode,
        "__set_app_type" => fn set_app_type, "_set_app_type" => fn set_app_type,
        "_configure_narrow_argv" => fn configure_narrow_argv, "_initialize_narrow_environment" => fn initialize_narrow_environment,
        "_configure_wide_argv" => fn configure_narrow_argv, "_initialize_wide_environment" => fn initialize_narrow_environment,
        "__setusermatherr" => fn setusermatherr, "__p__setusermatherr" => fn set_user_math_err,
        "_initterm" => fn initterm, "_initterm_e" => fn initterm_e,
        "atexit" => fn atexit, "_onexit" => fn onexit, "__dllonexit" => fn dllonexit, "_crt_atexit" => fn crt_atexit,
        "_initialize_onexit_table" => fn initialize_onexit_table, "_register_onexit_function" => fn register_onexit_function,
        "_execute_onexit_table" => fn execute_onexit_table,
        "exit" => fn exit, "_exit" => fn quick_exit, "_Exit" => fn quick_exit, "quick_exit" => fn quick_exit,
        "_cexit" => fn cexit, "_c_exit" => fn c_exit, "_amsg_exit" => fn amsg_exit, "abort" => fn abort,
        "signal" => fn signal, "raise" => fn raise,
        "_errno" => fn errno, "__doserrno" => fn doserrno, "_get_errno" => fn get_errno, "_set_errno" => fn set_errno,
        "_lock" => fn lock, "_unlock" => fn unlock,
        "calloc" => fn calloc, "realloc" => fn realloc, "_msize" => fn msize,
        "_aligned_malloc" => fn aligned_malloc, "_aligned_free" => fn aligned_free, "_aligned_realloc" => fn aligned_realloc,
        "localeconv" => fn localeconv, "setlocale" => fn setlocale, "_wsetlocale" => fn wsetlocale,
        "___lc_codepage_func" => fn lc_codepage_func, "___mb_cur_max_func" => fn mb_cur_max_func,
        "___lc_handle_func" => fn lc_handle_func, "___lc_collate_cp_func" => fn lc_collate_cp_func,
        "__C_specific_handler" => fn c_specific_handler,
        "getenv" => fn getenv, "_wgetenv" => fn wgetenv, "_putenv" => fn putenv, "_wputenv" => fn wputenv, "_putenv_s" => fn putenv_s,
        "time" => fn time64, "_time64" => fn time64, "_time32" => fn time32,
        "localtime" => fn localtime64, "_localtime64" => fn localtime64, "_localtime32" => fn localtime32,
        "gmtime" => fn gmtime64, "_gmtime64" => fn gmtime64, "_gmtime32" => fn gmtime32,
        "_localtime64_s" => fn localtime64_s, "_gmtime64_s" => fn gmtime64_s,
        "mktime" => fn mktime64, "_mktime64" => fn mktime64, "_mkgmtime64" => fn mkgmtime64, "_mkgmtime" => fn mkgmtime64,
        "difftime" => fn difftime64, "_difftime64" => fn difftime64, "clock" => fn clock, "strftime" => fn strftime,
        "asctime" => fn asctime, "ctime" => fn ctime64, "_ctime64" => fn ctime64,
        "srand" => fn srand, "rand" => fn rand, "rand_s" => fn rand_s,
        "strerror" => fn strerror, "strerror_s" => fn strerror_s, "perror" => fn perror,
        "strspn" => fn strspn, "strcspn" => fn strcspn, "strpbrk" => fn strpbrk, "memchr" => fn memchr,
        "strcoll" => fn strcoll, "strtok" => fn strtok,
        "atof" => fn atof, "ldexp" => fn ldexp, "frexp" => fn frexp, "modf" => fn modf, "_isnan" => fn isnan, "_finite" => fn finite,
        "mbstowcs" => fn mbstowcs, "wcstombs" => fn wcstombs, "mbtowc" => fn mbtowc, "wctomb" => fn wctomb, "mblen" => fn mblen,
        "_open" => fn open, "_wopen" => fn wopen, "_sopen" => fn sopen, "_sopen_s" => fn sopen_s, "_close" => fn close,
        "_read" => fn read, "_write" => fn write, "_lseek" => fn lseek, "_lseeki64" => fn lseeki64,
        "_isatty" => fn isatty, "_dup" => fn dup, "_dup2" => fn dup2, "_setmode" => fn setmode, "_access" => fn access,
        "_unlink" => fn unlink, "_mkdir" => fn mkdir, "_rmdir" => fn rmdir, "_chdir" => fn chdir, "_getcwd" => fn getcwd,
        "_get_osfhandle" => fn get_osfhandle, "_getpid" => fn getpid,
        "_beginthreadex" => fn beginthreadex, "_beginthread" => fn beginthread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CALLS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    extern "win64" fn premier() { CALLS.lock().unwrap().push(1); }
    extern "win64" fn second() { CALLS.lock().unwrap().push(2); }
    extern "win64" fn echec() -> i32 { 7 }

    #[test]
    fn initterm_appelle_les_initialiseurs_non_nuls() {
        CALLS.lock().unwrap().clear();
        let table = [premier as usize, 0, second as usize];
        unsafe { initterm(table.as_ptr(), table.as_ptr().add(3)); }
        assert_eq!(*CALLS.lock().unwrap(), vec![1, 2]);
        let table = [echec as usize];
        assert_eq!(unsafe { initterm_e(table.as_ptr(), table.as_ptr().add(1)) }, 7);
    }

    #[test]
    fn table_onexit_execute_en_ordre_inverse() {
        CALLS.lock().unwrap().clear();
        let mut t = OnexitTable { first: std::ptr::null_mut(), last: std::ptr::null_mut(), end: std::ptr::null_mut() };
        unsafe {
            assert_eq!(initialize_onexit_table(&mut t), 0);
            for _ in 0..20 { assert_eq!(register_onexit_function(&mut t, premier as usize), 0); }
            assert_eq!(register_onexit_function(&mut t, second as usize), 0);
            assert_eq!(execute_onexit_table(&mut t), 0);
        }
        let calls = CALLS.lock().unwrap();
        assert_eq!(calls.len(), 21);
        assert_eq!(calls[0], 2);
        assert!(t.first.is_null());
    }

    #[test]
    fn rand_suit_le_generateur_microsoft() {
        srand(1);
        assert_eq!([rand(), rand(), rand()], [41, 18467, 6334]);
    }

    #[test]
    fn temps_et_strftime() {
        unsafe {
            let t: i64 = 86_400 * 365;
            let tm = gmtime64(&t);
            assert_eq!(((*tm).year, (*tm).mon, (*tm).mday), (71, 0, 1));
            let mut buf = [0u8; 32];
            let n = strftime(buf.as_mut_ptr(), buf.len(), b"%Y-%m-%d\0".as_ptr(), tm);
            assert_eq!(&buf[..n], b"1971-01-01");
            let mut copy = *tm;
            assert_eq!(mkgmtime64(&mut copy), t);
        }
    }

    #[test]
    fn errno_strerror_et_signaux() {
        unsafe {
            *errno() = 2;
            assert_eq!(std::ffi::CStr::from_ptr(strerror(2) as *const i8).to_str().unwrap(), "No such file or directory");
            assert_eq!(crt_errno(libc::ENOTEMPTY), 41);
        }
        assert_eq!(signal(999, 0), SIG_ERR);
        assert_eq!(signal(2, SIG_IGN), SIG_DFL);
        assert_eq!(raise(2), 0);
        assert_eq!(signal(2, SIG_DFL), SIG_IGN);
    }

    #[test]
    fn locale_c_et_conversions() {
        unsafe {
            assert!(!setlocale(0, b"\0".as_ptr()).is_null());
            assert!(setlocale(0, b"fr_FR\0".as_ptr()).is_null());
            let mut w = [0u16; 8];
            assert_eq!(mbstowcs(w.as_mut_ptr(), b"abc\0".as_ptr(), 8), 3);
            assert_eq!(&w[..4], &[97, 98, 99, 0]);
            let mut b = [0u8; 8];
            assert_eq!(wcstombs(b.as_mut_ptr(), w.as_ptr(), 8), 3);
            assert_eq!(&b[..4], b"abc\0");
        }
    }

    #[test]
    fn descripteurs_bas_niveau() {
        unsafe {
            let path = std::env::temp_dir().join(format!("winlux-lowio-{}", std::process::id()));
            let c = format!("{}\0", path.display());
            let fd = open(c.as_ptr(), O_RDWR | O_CREAT | O_TRUNC | 0x8000, 0x180);
            assert!(fd > 2);
            assert_eq!(write(fd, b"bonjour".as_ptr() as *const c_void, 7), 7);
            assert_eq!(lseek(fd, 0, 0), 0);
            let mut buf = [0u8; 8];
            assert_eq!(read(fd, buf.as_mut_ptr() as *mut c_void, 8), 7);
            assert_eq!(&buf[..7], b"bonjour");
            assert_eq!(close(fd), 0);
            assert_eq!(access(c.as_ptr(), 0), 0);
            assert_eq!(unlink(c.as_ptr()), 0);
            assert_eq!(open(c.as_ptr(), 0, 0), -1);
            assert_eq!(*errno(), 2);
        }
    }
}
