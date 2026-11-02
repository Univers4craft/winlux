//! Suite auto-vérifiante : dossier C:\Windows\System32 et msvcp140.
//!
//! 1. Une DLL copiée dans System32 (et nulle part ailleurs) se charge par
//!    son seul nom, comme un redistribuable installé par vc_redist.
//! 2. msvcp140.dll (la version maison, ou la vraie si elle est installée)
//!    fournit threads, mutex, variables de condition et horloges.
//! Code de sortie = nombre d'échecs.
#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use core::ptr::null_mut;
use winlux_testexe::*;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn GetSystemDirectoryA(buf: *mut u8, size: u32) -> u32;
    fn GetModuleFileNameA(module: Handle, buf: *mut u8, size: u32) -> u32;
}

/// Concatène deux chaînes C dans `buf` (terminée par un nul).
fn join(buf: &mut [u8; 512], a: &[u8], b: &[u8]) -> *const c_char {
    let mut n = 0;
    for &c in a.iter().chain(b) {
        if c == 0 || n + 1 >= buf.len() {
            break;
        }
        buf[n] = c;
        n += 1;
    }
    buf[n] = 0;
    buf.as_ptr() as *const c_char
}

unsafe fn system32() {
    out(b"\n-- System32 --\n");
    let mut sys = [0u8; 260];
    let n = GetSystemDirectoryA(sys.as_mut_ptr(), 260) as usize;
    check!(&sys[..n] == b"C:\\Windows\\System32", "GetSystemDirectoryA");
    // Crée C:\Windows puis C:\Windows\System32 (déjà présents : sans effet).
    CreateDirectoryA(c"C:\\Windows".as_ptr(), null_mut());
    CreateDirectoryA(c"C:\\Windows\\System32".as_ptr(), null_mut());

    // testdll.dll est à côté du programme : on la copie dans System32
    // sous un nom introuvable ailleurs.
    let mut me = [0u8; 512];
    let len = GetModuleFileNameA(null_mut(), me.as_mut_ptr(), 512) as usize;
    let dir_end = me[..len].iter().rposition(|&c| c == b'\\' || c == b'/').map_or(0, |p| p + 1);
    let mut src = [0u8; 512];
    let src = join(&mut src, &me[..dir_end], b"testdll.dll");
    let mut dst = [0u8; 512];
    let dst = join(&mut dst, b"C:\\Windows\\System32\\", b"winlux_sys32_probe.dll");
    check!(CopyFileA(src, dst, 0) != 0, "copie d'une DLL dans System32");
    let m = LoadLibraryA(c"winlux_sys32_probe.dll".as_ptr());
    check!(!m.is_null(), "DLL de System32 chargée par son nom");
    if !m.is_null() {
        let add = GetProcAddress(m, c"winlux_add".as_ptr());
        check!(!add.is_null(), "export de la DLL de System32");
        if !add.is_null() {
            let add: unsafe extern "system" fn(u32, u32) -> u32 = core::mem::transmute(add);
            check!(add(40, 2) == 42, "appel dans la DLL de System32");
        }
        FreeLibrary(m);
    }
    DeleteFileA(dst);
}

#[repr(C)]
struct Shared {
    mtx: [u64; 10],
    cnd: [u64; 9],
    ready: u32,
}

static mut SHARED: Shared = Shared { mtx: [0; 10], cnd: [0; 9], ready: 0 };

type MtxFn = unsafe extern "system" fn(*mut c_void) -> i32;
type CndWait = unsafe extern "system" fn(*mut c_void, *mut c_void, u32) -> i32;
type CndSignal = unsafe extern "system" fn(*mut c_void) -> i32;

static mut FNS: Option<(MtxFn, MtxFn, CndSignal)> = None;

extern "system" fn worker(_p: *mut c_void) -> u32 {
    unsafe { worker_body() }
}

unsafe fn worker_body() -> u32 {
    let Some((lock, unlock, signal)) = FNS else { return 1 };
    let s = &raw mut SHARED;
    lock((*s).mtx.as_mut_ptr().cast());
    (*s).ready = 1;
    signal((*s).cnd.as_mut_ptr().cast());
    unlock((*s).mtx.as_mut_ptr().cast());
    0
}

unsafe fn msvcp() {
    out(b"\n-- msvcp140 --\n");
    let m = LoadLibraryA(c"msvcp140.dll".as_ptr());
    check!(!m.is_null(), "msvcp140.dll chargée");
    if m.is_null() {
        return;
    }
    let p = |n: &core::ffi::CStr| GetProcAddress(m, n.as_ptr());
    let hc = p(c"_Thrd_hardware_concurrency");
    check!(!hc.is_null(), "_Thrd_hardware_concurrency");
    if !hc.is_null() {
        let hc: unsafe extern "system" fn() -> u32 = core::mem::transmute(hc);
        check!(hc() >= 1, "au moins un processeur");
    }
    let freq = p(c"_Query_perf_frequency");
    let ctr = p(c"_Query_perf_counter");
    check!(!freq.is_null() && !ctr.is_null(), "_Query_perf_*");
    if !freq.is_null() && !ctr.is_null() {
        let freq: unsafe extern "system" fn() -> i64 = core::mem::transmute(freq);
        let ctr: unsafe extern "system" fn() -> i64 = core::mem::transmute(ctr);
        let a = ctr();
        let b = ctr();
        check!(freq() > 0 && b >= a, "horloge haute résolution");
    }
    let names = [
        c"_Mtx_init_in_situ",
        c"_Mtx_lock",
        c"_Mtx_unlock",
        c"_Cnd_init_in_situ",
        c"_Cnd_timedwait_for",
        c"_Cnd_signal",
    ];
    let mut f = [core::ptr::null::<c_void>(); 6];
    for (i, n) in names.iter().enumerate() {
        f[i] = p(n);
    }
    // _Cnd_timedwait_for n'existe que dans les msvcp140 récentes.
    check!(f.iter().take(4).all(|x| !x.is_null()) && !f[5].is_null(), "_Mtx_* et _Cnd_*");
    if f.iter().any(|x| x.is_null()) {
        return;
    }
    let init: unsafe extern "system" fn(*mut c_void, i32) = core::mem::transmute(f[0]);
    let lock: MtxFn = core::mem::transmute(f[1]);
    let unlock: MtxFn = core::mem::transmute(f[2]);
    let cinit: unsafe extern "system" fn(*mut c_void) = core::mem::transmute(f[3]);
    let wait: CndWait = core::mem::transmute(f[4]);
    let signal: CndSignal = core::mem::transmute(f[5]);
    let s = &raw mut SHARED;
    init((*s).mtx.as_mut_ptr().cast(), 1);
    cinit((*s).cnd.as_mut_ptr().cast());
    FNS = Some((lock, unlock, signal));
    lock((*s).mtx.as_mut_ptr().cast());
    let t = CreateThread(null_mut(), 0, worker, null_mut(), 0, null_mut());
    check!(!t.is_null(), "thread créé");
    let mut rounds = 0;
    while (*s).ready == 0 && rounds < 50 {
        wait((*s).cnd.as_mut_ptr().cast(), (*s).mtx.as_mut_ptr().cast(), 100);
        rounds += 1;
    }
    unlock((*s).mtx.as_mut_ptr().cast());
    check!((*s).ready == 1, "variable de condition réveillée par un autre thread");
    if !t.is_null() {
        WaitForSingleObject(t, 5000);
        CloseHandle(t);
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Winlux - suite System32 / msvcp140\n");
    unsafe {
        system32();
        msvcp();
        if FAILURES == 0 {
            out(b"\nTous les tests System32 et msvcp140 passent.\n");
        }
    }
    finish()
}
