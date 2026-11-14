//! msvcp140.dll maison : la partie de la bibliothèque C++ de Visual Studio
//! que les programmes importent le plus, pour les machines où aucun
//! redistribuable n'a été installé.
//!
//! Couvert : threads (`_Thrd_*`), mutex et variables de condition
//! (`_Mtx_*`, `_Cnd_*`, compatibles avec les deux dispositions en mémoire
//! du STL de Visual Studio, l'ancienne de 80 octets et celle de 32 octets
//! de VS 2022 17.10), horloges (`_Query_perf_*`, `_Xtime_*`),
//! `random_device`, tables d'erreurs, et les fonctions `_X*` qui signalent
//! une erreur fatale de conteneur.
//!
//! Quand la vraie msvcp140.dll est présente (dossier de l'appli ou
//! `C:\Windows\System32`, déposée par vc_redist ou par l'installeur de
//! l'appli), c'est elle qui est chargée : voir `dlls::prefers_real_file`.

use super::Exports;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const THRD_SUCCESS: i32 = 0;
const THRD_TIMEDOUT: i32 = 2;
const THRD_BUSY: i32 = 3;
const MTX_RECURSIVE: i32 = 0x100;

// ---------- futex ----------

pub(crate) unsafe fn futex_wait(
    word: &AtomicU32,
    expected: u32,
    timeout: Option<Duration>,
) -> bool {
    let ts = timeout.map(|d| libc::timespec {
        tv_sec: d.as_secs() as libc::time_t,
        tv_nsec: d.subsec_nanos() as libc::c_long,
    });
    let r = libc::syscall(
        libc::SYS_futex,
        word as *const AtomicU32,
        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
        expected,
        ts.as_ref()
            .map_or(std::ptr::null(), |t| t as *const libc::timespec),
    );
    !(r == -1 && *libc::__errno_location() == libc::ETIMEDOUT)
}

pub(crate) unsafe fn futex_wake(word: &AtomicU32, n: i32) {
    libc::syscall(
        libc::SYS_futex,
        word as *const AtomicU32,
        libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
        n,
    );
}

fn tid() -> i32 {
    unsafe { libc::syscall(libc::SYS_gettid) as i32 }
}

// ---------- Mutex ----------
//
// Disposition commune aux deux versions du STL (champs utilisés ici) :
//   +0 type (int), +16 verrou (mot de 32 bits, dans l'emplacement du
//   SRWLOCK), +24 thread propriétaire, +28 compteur de récursion.

struct Mtx(usize);

impl Mtx {
    fn kind(&self) -> &AtomicI32 {
        unsafe { &*(self.0 as *const AtomicI32) }
    }
    fn word(&self) -> &AtomicU32 {
        unsafe { &*((self.0 + 16) as *const AtomicU32) }
    }
    fn owner(&self) -> &AtomicI32 {
        unsafe { &*((self.0 + 24) as *const AtomicI32) }
    }
    fn count(&self) -> &AtomicI32 {
        unsafe { &*((self.0 + 28) as *const AtomicI32) }
    }

    fn raw_lock(&self) {
        let w = self.word();
        if w.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
        // 2 : verrouillé avec des threads en attente.
        while w.swap(2, Ordering::Acquire) != 0 {
            unsafe { futex_wait(w, 2, None) };
        }
    }

    fn raw_unlock(&self) {
        if self.word().swap(0, Ordering::Release) == 2 {
            unsafe { futex_wake(self.word(), 1) };
        }
    }

    fn lock(&self) -> i32 {
        let me = tid();
        if self.owner().load(Ordering::Relaxed) == me {
            if self.kind().load(Ordering::Relaxed) & MTX_RECURSIVE != 0 {
                self.count().fetch_add(1, Ordering::Relaxed);
                return THRD_SUCCESS;
            }
            // Comme Windows : relocker un mutex simple est une faute du programme.
            eprintln!("winlux: msvcp140 : mutex non récursif verrouillé deux fois");
        }
        self.raw_lock();
        self.owner().store(me, Ordering::Relaxed);
        self.count().store(1, Ordering::Relaxed);
        THRD_SUCCESS
    }

    fn try_lock(&self) -> i32 {
        let me = tid();
        if self.owner().load(Ordering::Relaxed) == me
            && self.kind().load(Ordering::Relaxed) & MTX_RECURSIVE != 0
        {
            self.count().fetch_add(1, Ordering::Relaxed);
            return THRD_SUCCESS;
        }
        if self
            .word()
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return THRD_BUSY;
        }
        self.owner().store(me, Ordering::Relaxed);
        self.count().store(1, Ordering::Relaxed);
        THRD_SUCCESS
    }

    fn unlock(&self) -> i32 {
        if self.count().fetch_sub(1, Ordering::Relaxed) <= 1 {
            self.count().store(0, Ordering::Relaxed);
            self.owner().store(0, Ordering::Relaxed);
            self.raw_unlock();
        }
        THRD_SUCCESS
    }
}

extern "win64" fn mtx_init_in_situ(m: usize, kind: i32) {
    unsafe { std::ptr::write_bytes(m as *mut u8, 0, 32) };
    Mtx(m).kind().store(kind, Ordering::Relaxed);
}

extern "win64" fn mtx_destroy_in_situ(_m: usize) {}

extern "win64" fn mtx_init(out: *mut usize, kind: i32) -> i32 {
    // Taille de l'ancienne disposition, la plus grande.
    let m = Box::into_raw(Box::new([0u64; 10])) as usize;
    mtx_init_in_situ(m, kind);
    unsafe { *out = m };
    THRD_SUCCESS
}

extern "win64" fn mtx_destroy(m: usize) {
    if m != 0 {
        drop(unsafe { Box::from_raw(m as *mut [u64; 10]) });
    }
}

extern "win64" fn mtx_lock(m: usize) -> i32 {
    Mtx(m).lock()
}

extern "win64" fn mtx_trylock(m: usize) -> i32 {
    Mtx(m).try_lock()
}

extern "win64" fn mtx_timedlock(m: usize, _xt: usize) -> i32 {
    Mtx(m).lock()
}

extern "win64" fn mtx_unlock(m: usize) -> i32 {
    Mtx(m).unlock()
}

extern "win64" fn mtx_current_owns(m: usize) -> i32 {
    let x = Mtx(m);
    (x.count().load(Ordering::Relaxed) != 0 && x.owner().load(Ordering::Relaxed) == tid()) as i32
}

extern "win64" fn mtx_clear_owner(m: usize) {
    let x = Mtx(m);
    x.owner().store(0, Ordering::Relaxed);
    x.count().fetch_sub(1, Ordering::Relaxed);
}

extern "win64" fn mtx_reset_owner(m: usize) {
    let x = Mtx(m);
    x.owner().store(tid(), Ordering::Relaxed);
    x.count().fetch_add(1, Ordering::Relaxed);
}

// ---------- Variables de condition ----------
//
// Un compteur de génération en +8 (emplacement du CONDITION_VARIABLE dans
// les deux dispositions).

fn seq(c: usize) -> &'static AtomicU32 {
    unsafe { &*((c + 8) as *const AtomicU32) }
}

extern "win64" fn cnd_init_in_situ(c: usize) {
    unsafe { std::ptr::write_bytes(c as *mut u8, 0, 16) };
}

extern "win64" fn cnd_destroy_in_situ(_c: usize) {}

extern "win64" fn cnd_init(out: *mut usize) -> i32 {
    let c = Box::into_raw(Box::new([0u64; 9])) as usize;
    unsafe { *out = c };
    THRD_SUCCESS
}

extern "win64" fn cnd_destroy(c: usize) {
    if c != 0 {
        drop(unsafe { Box::from_raw(c as *mut [u64; 9]) });
    }
}

fn cnd_wait_inner(c: usize, m: usize, timeout: Option<Duration>) -> i32 {
    let s = seq(c);
    let gen = s.load(Ordering::Acquire);
    let x = Mtx(m);
    let count = x.count().swap(0, Ordering::Relaxed);
    x.owner().store(0, Ordering::Relaxed);
    x.raw_unlock();
    let woke = unsafe { futex_wait(s, gen, timeout) };
    x.raw_lock();
    x.owner().store(tid(), Ordering::Relaxed);
    x.count().store(count.max(1), Ordering::Relaxed);
    if woke {
        THRD_SUCCESS
    } else {
        THRD_TIMEDOUT
    }
}

extern "win64" fn cnd_wait(c: usize, m: usize) -> i32 {
    cnd_wait_inner(c, m, None)
}

extern "win64" fn cnd_timedwait(c: usize, m: usize, xt: *const XTime) -> i32 {
    let left = unsafe { xt.as_ref() }
        .map(|t| t.until())
        .unwrap_or_default();
    cnd_wait_inner(c, m, Some(left))
}

extern "win64" fn cnd_timedwait_for(c: usize, m: usize, ms: u32) -> i32 {
    cnd_wait_inner(c, m, Some(Duration::from_millis(ms as u64)))
}

extern "win64" fn cnd_signal(c: usize) -> i32 {
    seq(c).fetch_add(1, Ordering::Release);
    unsafe { futex_wake(seq(c), 1) };
    THRD_SUCCESS
}

extern "win64" fn cnd_broadcast(c: usize) -> i32 {
    seq(c).fetch_add(1, Ordering::Release);
    unsafe { futex_wake(seq(c), i32::MAX) };
    THRD_SUCCESS
}

/// notify_all_at_thread_exit : appliqué tout de suite (approximation sûre).
extern "win64" fn cnd_broadcast_at_thread_exit(c: usize, m: usize, done: *mut i32) {
    if !done.is_null() {
        unsafe { *done = 1 };
    }
    Mtx(m).unlock();
    cnd_broadcast(c);
}

extern "win64" fn noop1(_a: usize) {}
extern "win64" fn noop3(_a: usize, _b: usize, _c: usize) {}

// ---------- Temps ----------

#[repr(C)]
pub struct XTime {
    sec: i64,
    nsec: i32,
}

impl XTime {
    /// Durée restante jusqu'à cette date absolue (horloge système).
    fn until(&self) -> Duration {
        let target = Duration::new(
            self.sec.max(0) as u64,
            self.nsec.clamp(0, 999_999_999) as u32,
        );
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        target.saturating_sub(now)
    }
}

fn start() -> Instant {
    static S: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *S.get_or_init(Instant::now)
}

/// Compteur haute résolution en nanosecondes (fréquence 1 GHz).
extern "win64" fn query_perf_counter() -> i64 {
    start().elapsed().as_nanos() as i64 + 1
}

extern "win64" fn query_perf_frequency() -> i64 {
    1_000_000_000
}

/// Centaines de nanosecondes depuis 1970.
extern "win64" fn xtime_get_ticks() -> i64 {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        / 100) as i64
}

extern "win64" fn xtime_get(xt: *mut XTime, base: i32) -> i32 {
    if xt.is_null() || base != 1 {
        return 0;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    unsafe {
        (*xt).sec = now.as_secs() as i64;
        (*xt).nsec = now.subsec_nanos() as i32;
    }
    base
}

extern "win64" fn xtime_diff_to_millis2(a: *const XTime, b: *const XTime) -> u32 {
    let (Some(a), Some(b)) = (unsafe { a.as_ref() }, unsafe { b.as_ref() }) else {
        return 0;
    };
    let d = (a.sec - b.sec) as i128 * 1000 + (a.nsec - b.nsec) as i128 / 1_000_000;
    d.clamp(0, u32::MAX as i128) as u32
}

// ---------- Threads ----------

extern "win64" fn thrd_hardware_concurrency() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

extern "win64" fn thrd_id() -> u32 {
    match super::lookup("kernel32", "GetCurrentThreadId") {
        Some(f) => {
            let f: extern "win64" fn() -> u32 = unsafe { std::mem::transmute(f) };
            f()
        }
        None => tid() as u32,
    }
}

extern "win64" fn thrd_yield() {
    std::thread::yield_now();
}

extern "win64" fn thrd_sleep(xt: *const XTime) {
    if let Some(t) = unsafe { xt.as_ref() } {
        std::thread::sleep(t.until());
    }
}

extern "win64" fn thrd_sleep_for(ms: u32) {
    std::thread::sleep(Duration::from_millis(ms as u64));
}

/// `_Thrd_t` : { HANDLE, id } (16 octets, passé par adresse en Windows x64).
#[repr(C)]
struct Thrd {
    handle: usize,
    id: u32,
}

fn k32<F: Copy>(name: &str) -> Option<F> {
    super::lookup("kernel32", name).map(|p| unsafe { std::mem::transmute_copy::<usize, F>(&p) })
}

extern "win64" fn thrd_detach(t: *const Thrd) -> i32 {
    if let (Some(t), Some(close)) = (
        unsafe { t.as_ref() },
        k32::<extern "win64" fn(usize) -> i32>("CloseHandle"),
    ) {
        close(t.handle);
    }
    THRD_SUCCESS
}

extern "win64" fn thrd_join(t: *const Thrd, res: *mut i32) -> i32 {
    let Some(t) = (unsafe { t.as_ref() }) else {
        return 1;
    };
    let wait = k32::<extern "win64" fn(usize, u32) -> u32>("WaitForSingleObject");
    let code = k32::<extern "win64" fn(usize, *mut u32) -> i32>("GetExitCodeThread");
    let close = k32::<extern "win64" fn(usize) -> i32>("CloseHandle");
    if let (Some(wait), Some(code), Some(close)) = (wait, code, close) {
        wait(t.handle, u32::MAX);
        let mut c = 0u32;
        code(t.handle, &mut c);
        if !res.is_null() {
            unsafe { *res = c as i32 };
        }
        close(t.handle);
        return THRD_SUCCESS;
    }
    1
}

// ---------- Erreurs ----------

pub(crate) fn fatal(what: &str, msg: *const u8) -> ! {
    let detail = if msg.is_null() {
        String::new()
    } else {
        unsafe { super::cstr(msg) }
    };
    eprintln!("winlux: exception C++ std::{what} non rattrapable ({detail}) : arrêt du programme");
    crate::crash::record_failure("CXX_EXCEPTION", &format!("std::{what} {detail}"));
    crate::dlls::kernel32::flush_and_exit(3)
}

/// std::bad_alloc non rattrapable, aussi pour msvcp140_1 (#2178).
pub(crate) fn bad_alloc_fatal() -> ! {
    fatal("bad_alloc", std::ptr::null())
}
extern "win64" fn x_bad_alloc() {
    fatal("bad_alloc", std::ptr::null())
}
extern "win64" fn x_length_error(m: *const u8) {
    fatal("length_error", m)
}
extern "win64" fn x_out_of_range(m: *const u8) {
    fatal("out_of_range", m)
}
extern "win64" fn x_invalid_argument(m: *const u8) {
    fatal("invalid_argument", m)
}
extern "win64" fn x_runtime_error(m: *const u8) {
    fatal("runtime_error", m)
}
extern "win64" fn x_overflow_error(m: *const u8) {
    fatal("overflow_error", m)
}
extern "win64" fn x_bad_function_call() {
    fatal("bad_function_call", std::ptr::null())
}
extern "win64" fn x_bad_array_new_length() {
    fatal("bad_array_new_length", std::ptr::null())
}
extern "win64" fn throw_cpp_error(code: i32) {
    fatal("system_error", format!("code {code}\0").as_ptr())
}

extern "win64" fn random_device() -> u32 {
    let mut v = 0u32;
    unsafe { libc::getrandom((&mut v as *mut u32).cast(), 4, 0) };
    v
}

/// Messages des codes errno, comme les renvoie la CRT de Microsoft.
pub fn syserror_text(code: i32) -> Option<&'static [u8]> {
    Some(match code {
        1 => b"operation not permitted\0",
        2 => b"no such file or directory\0",
        4 => b"interrupted\0",
        5 => b"io error\0",
        9 => b"bad file descriptor\0",
        11 | 140 => b"resource unavailable try again\0",
        12 => b"not enough memory\0",
        13 => b"permission denied\0",
        16 => b"device or resource busy\0",
        17 => b"file exists\0",
        22 => b"invalid argument\0",
        28 => b"no space on device\0",
        36 => b"resource deadlock would occur\0",
        38 => b"filename too long\0",
        41 => b"directory not empty\0",
        138 => b"timed out\0",
        _ => return None,
    })
}

extern "win64" fn syserror_map(code: i32) -> *const u8 {
    syserror_text(code).map_or(std::ptr::null(), |t| t.as_ptr())
}

/// Erreur Windows → errno (les plus courantes).
pub fn winerror_errno(code: i32) -> i32 {
    match code {
        2 | 3 => 2,
        5 => 13,
        8 | 14 => 12,
        80 | 183 => 17,
        87 => 22,
        112 => 28,
        145 => 41,
        170 => 16,
        1460 => 138,
        _ => 0,
    }
}

extern "win64" fn winerror_map(code: i32) -> i32 {
    winerror_errno(code)
}

extern "win64" fn ret_false() -> i32 {
    0
}

extern "win64" fn uncaught_exceptions() -> i32 {
    0
}

/// `Concurrency::task_continuation_context::task_continuation_context()`
/// (#2308), ici sur le bureau.
///
/// Corps d'après la MS STL publique (`stl/src/ppltasks.cpp`, branche hors
/// `_CRT_APP`) : `_ContextCallback(false), _M_RunInline(false)`.
///
/// Disposition x64, notre choix, dit dans la PR : la MS STL ne publie pas
/// `ppltasks.h`. Seul le premier mot est certain (la base, construite sans
/// contexte ; `pplxtasks.h` du C++ REST SDK de Microsoft, sous licence MIT,
/// y range le contexte COM). Nous le mettons à zéro et n'écrivons rien
/// au-delà : le décalage de `_M_RunInline` n'est qu'une inférence, et une
/// écriture de trop écraserait la pile de l'appelant. `_M_RunInline` garde
/// donc la valeur que l'appelant y a laissée.
unsafe extern "win64" fn task_continuation_context_ctor(this: *mut u8) -> *mut u8 {
    (this as *mut usize).write_unaligned(0);
    this
}

pub fn exports() -> Exports {
    crate::exports! {
        "_Mtx_init_in_situ" => fn mtx_init_in_situ,
        "_Mtx_destroy_in_situ" => fn mtx_destroy_in_situ,
        "_Mtx_init" => fn mtx_init,
        "_Mtx_destroy" => fn mtx_destroy,
        "_Mtx_lock" => fn mtx_lock,
        "_Mtx_trylock" => fn mtx_trylock,
        "_Mtx_timedlock" => fn mtx_timedlock,
        "_Mtx_unlock" => fn mtx_unlock,
        "_Mtx_current_owns" => fn mtx_current_owns,
        "_Mtx_clear_owner" => fn mtx_clear_owner,
        "_Mtx_reset_owner" => fn mtx_reset_owner,
        "_Cnd_init_in_situ" => fn cnd_init_in_situ,
        "_Cnd_destroy_in_situ" => fn cnd_destroy_in_situ,
        "_Cnd_init" => fn cnd_init,
        "_Cnd_destroy" => fn cnd_destroy,
        "_Cnd_wait" => fn cnd_wait,
        "_Cnd_timedwait" => fn cnd_timedwait,
        "_Cnd_timedwait_for" => fn cnd_timedwait_for,
        "_Cnd_signal" => fn cnd_signal,
        "_Cnd_broadcast" => fn cnd_broadcast,
        "_Cnd_do_broadcast_at_thread_exit" => fn cnd_broadcast_at_thread_exit,
        "_Cnd_register_at_thread_exit" => fn noop3,
        "_Cnd_unregister_at_thread_exit" => fn noop1,
        "_Query_perf_counter" => fn query_perf_counter,
        "_Query_perf_frequency" => fn query_perf_frequency,
        "_Xtime_get_ticks" => fn xtime_get_ticks,
        "xtime_get" => fn xtime_get,
        "_Xtime_diff_to_millis2" => fn xtime_diff_to_millis2,
        "_Thrd_hardware_concurrency" => fn thrd_hardware_concurrency,
        "_Thrd_id" => fn thrd_id,
        "_Thrd_yield" => fn thrd_yield,
        "_Thrd_sleep" => fn thrd_sleep,
        "_Thrd_sleep_for" => fn thrd_sleep_for,
        "_Thrd_detach" => fn thrd_detach,
        "_Thrd_join" => fn thrd_join,
        "?_Xbad_alloc@std@@YAXXZ" => fn x_bad_alloc,
        "?_Xlength_error@std@@YAXPEBD@Z" => fn x_length_error,
        "?_Xout_of_range@std@@YAXPEBD@Z" => fn x_out_of_range,
        "?_Xinvalid_argument@std@@YAXPEBD@Z" => fn x_invalid_argument,
        "?_Xruntime_error@std@@YAXPEBD@Z" => fn x_runtime_error,
        "?_Xoverflow_error@std@@YAXPEBD@Z" => fn x_overflow_error,
        "?_Xbad_function_call@std@@YAXXZ" => fn x_bad_function_call,
        "?_Throw_bad_array_new_length@std@@YAXXZ" => fn x_bad_array_new_length,
        "?_Throw_Cpp_error@std@@YAXH@Z" => fn throw_cpp_error,
        "?_Throw_C_error@std@@YAXH@Z" => fn throw_cpp_error,
        "?_Random_device@std@@YAIXZ" => fn random_device,
        "?_Syserror_map@std@@YAPEBDH@Z" => fn syserror_map,
        "?_Winerror_map@std@@YAHH@Z" => fn winerror_map,
        "?uncaught_exception@std@@YA_NXZ" => fn ret_false,
        "?uncaught_exceptions@std@@YAHXZ" => fn uncaught_exceptions,
        "??0task_continuation_context@Concurrency@@AEAA@XZ" => fn task_continuation_context_ctor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn task_continuation_context_par_defaut() {
        let mut o = [0xA5u8; 24];
        let p = o.as_mut_ptr();
        assert_eq!(unsafe { task_continuation_context_ctor(p) }, p);
        assert_eq!(&o[..8], &[0; 8], "contexte nul");
        assert_eq!(&o[8..], &[0xA5; 16], "rien écrit au-delà du premier mot");
        assert!(exports().contains_key("??0task_continuation_context@Concurrency@@AEAA@XZ"));
    }

    #[test]
    fn mutex_recursif_et_simple() {
        let mut m = [0u64; 10];
        let p = m.as_mut_ptr() as usize;
        mtx_init_in_situ(p, MTX_RECURSIVE | 1);
        assert_eq!(mtx_lock(p), 0);
        assert_eq!(mtx_lock(p), 0);
        assert_eq!(mtx_current_owns(p), 1);
        mtx_unlock(p);
        assert_eq!(mtx_current_owns(p), 1);
        mtx_unlock(p);
        assert_eq!(mtx_current_owns(p), 0);
        mtx_init_in_situ(p, 1);
        assert_eq!(mtx_trylock(p), 0);
        let q = p;
        let busy = std::thread::spawn(move || mtx_trylock(q)).join().unwrap();
        assert_eq!(busy, THRD_BUSY);
        mtx_unlock(p);
    }

    #[test]
    fn mutex_protege_un_compteur() {
        let mut out = 0usize;
        mtx_init(&mut out, 1);
        let total = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let shared = Arc::new(std::cell::UnsafeCell::new(0u64));
        struct S(Arc<std::cell::UnsafeCell<u64>>);
        unsafe impl Send for S {}
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let s = S(shared.clone());
                let t = total.clone();
                std::thread::spawn(move || {
                    let s = s;
                    for _ in 0..10_000 {
                        mtx_lock(out);
                        unsafe { *s.0.get() += 1 };
                        mtx_unlock(out);
                    }
                    t.fetch_add(1, Ordering::Relaxed);
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(unsafe { *shared.get() }, 40_000);
        mtx_destroy(out);
    }

    #[test]
    fn condition_reveille_et_expire() {
        let mut m = Box::new([0u64; 10]);
        let mut c = Box::new([0u64; 9]);
        let (mp, cp) = (m.as_mut_ptr() as usize, c.as_mut_ptr() as usize);
        mtx_init_in_situ(mp, 1);
        cnd_init_in_situ(cp);
        mtx_lock(mp);
        assert_eq!(cnd_timedwait_for(cp, mp, 20), THRD_TIMEDOUT);
        assert_eq!(mtx_current_owns(mp), 1);
        let waker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            mtx_lock(mp);
            cnd_signal(cp);
            mtx_unlock(mp);
        });
        assert_eq!(cnd_timedwait_for(cp, mp, 5000), THRD_SUCCESS);
        mtx_unlock(mp);
        waker.join().unwrap();
        drop((m, c));
    }

    #[test]
    fn horloges_et_erreurs() {
        assert!(query_perf_counter() > 0);
        assert_eq!(query_perf_frequency(), 1_000_000_000);
        assert!(xtime_get_ticks() > 16_000_000_000_000_000);
        assert!(thrd_hardware_concurrency() >= 1);
        assert_eq!(syserror_text(2), Some(&b"no such file or directory\0"[..]));
        assert_eq!(winerror_errno(5), 13);
        assert!(exports().contains_key("?_Xlength_error@std@@YAXPEBD@Z"));
    }
}
