//! #852 : RegisterWaitForSingleObject et UnregisterWaitEx suivent les règles
//! de Windows. Un rappel qui n'a pas commencé ne part plus après
//! l'annulation ; l'événement de fin n'est signalé qu'à la fin du rappel en
//! cours ; l'annulation bloquante depuis le rappel lui-même ne se bloque pas.
//! Claude pour Windows (Chromium) libère l'objet surveillé juste après
//! UnregisterWaitEx : un rappel tardif lisait un pointeur nul.

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use winlux_testexe::*;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn RegisterWaitForSingleObject(
        out: *mut Handle,
        object: Handle,
        callback: extern "system" fn(*mut c_void, u8),
        context: *mut c_void,
        ms: u32,
        flags: u32,
    ) -> i32;
    fn UnregisterWaitEx(wait: Handle, completion: Handle) -> i32;
}

const INVALID: Handle = usize::MAX as Handle;
const WT_EXECUTEONLYONCE: u32 = 8;

static SLOW_CALLS: AtomicU32 = AtomicU32::new(0);
static SLOW_DONE: AtomicU32 = AtomicU32::new(0);
static LATE_CALLS: AtomicU32 = AtomicU32::new(0);
static SELF_WAIT: AtomicUsize = AtomicUsize::new(0);
static SELF_RESULT: AtomicU32 = AtomicU32::new(0);

extern "system" fn slow(_: *mut c_void, _: u8) {
    SLOW_CALLS.fetch_add(1, Ordering::SeqCst);
    unsafe { Sleep(300) };
    SLOW_DONE.store(1, Ordering::SeqCst);
}

extern "system" fn late(_: *mut c_void, _: u8) {
    LATE_CALLS.fetch_add(1, Ordering::SeqCst);
}

extern "system" fn unregisters_itself(_: *mut c_void, _: u8) {
    let w = SELF_WAIT.load(Ordering::SeqCst) as Handle;
    let r = unsafe { UnregisterWaitEx(w, INVALID) };
    SELF_RESULT.store(1 + r as u32, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let null = core::ptr::null_mut();
        // 1. Rappel en cours : l'événement de fin attend sa fin, et l'objet
        //    resté signalé (réinitialisation manuelle) ne relance rien.
        let e = CreateEventA(null, 1, 0, core::ptr::null());
        let done = CreateEventA(null, 1, 0, core::ptr::null());
        let mut w: Handle = core::ptr::null_mut();
        check!(
            RegisterWaitForSingleObject(&mut w, e, slow, null, u32::MAX, 0) != 0,
            "RegisterWaitForSingleObject"
        );
        SetEvent(e);
        for _ in 0..100 {
            if SLOW_CALLS.load(Ordering::SeqCst) != 0 {
                break;
            }
            Sleep(10);
        }
        check!(
            SLOW_CALLS.load(Ordering::SeqCst) == 1,
            "le rappel a commencé"
        );
        check!(
            UnregisterWaitEx(w, done) != 0,
            "UnregisterWaitEx(événement)"
        );
        check!(
            WaitForSingleObject(done, 0) == 0x102,
            "événement de fin pas encore signalé pendant le rappel"
        );
        check!(
            WaitForSingleObject(done, 3000) == 0,
            "événement de fin signalé"
        );
        check!(
            SLOW_DONE.load(Ordering::SeqCst) == 1,
            "le rappel était fini quand l'événement de fin est arrivé"
        );
        Sleep(400);
        check!(
            SLOW_CALLS.load(Ordering::SeqCst) == 1,
            "aucun rappel après l'annulation"
        );

        // 2. Annulation avant tout signal : le rappel ne part jamais.
        let e2 = CreateEventA(null, 1, 0, core::ptr::null());
        let mut w2: Handle = core::ptr::null_mut();
        RegisterWaitForSingleObject(&mut w2, e2, late, null, u32::MAX, 0);
        check!(
            UnregisterWaitEx(w2, INVALID) != 0,
            "UnregisterWaitEx(bloquant)"
        );
        SetEvent(e2);
        Sleep(300);
        check!(
            LATE_CALLS.load(Ordering::SeqCst) == 0,
            "pas de rappel tardif après l'annulation"
        );

        // 3. Annulation bloquante depuis le rappel : pas d'interblocage.
        let e3 = CreateEventA(null, 1, 0, core::ptr::null());
        let mut w3: Handle = core::ptr::null_mut();
        RegisterWaitForSingleObject(
            &mut w3,
            e3,
            unregisters_itself,
            null,
            u32::MAX,
            WT_EXECUTEONLYONCE,
        );
        SELF_WAIT.store(w3 as usize, Ordering::SeqCst);
        SetEvent(e3);
        for _ in 0..200 {
            if SELF_RESULT.load(Ordering::SeqCst) != 0 {
                break;
            }
            Sleep(10);
        }
        check!(
            SELF_RESULT.load(Ordering::SeqCst) != 0,
            "UnregisterWaitEx depuis le rappel rend la main"
        );
        CloseHandle(e);
        CloseHandle(e2);
        CloseHandle(e3);
        CloseHandle(done);
    }
    finish()
}
