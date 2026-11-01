//! Fonctions de kernel32/ntdll demandées par les runtimes JavaScript
//! compilés pour Windows (Bun, Node.js/libuv), et en premier lieu par
//! l'exécutable Windows de Claude Code (`claude.exe`, un binaire Bun).
//!
//! Chaque fonction suit la documentation publique de Microsoft et
//! s'appuie sur l'équivalent Linux : futex pour WaitOnAddress, la pile
//! décrite dans le TEB pour GetCurrentThreadStackLimits, etc.
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU32, Ordering};

/// TEB->DeallocationStack : bas de la réservation de la pile.
const TEB_DEALLOCATION_STACK: usize = 0x1478;

/// `GetCurrentThreadStackLimits(PULONG_PTR low, PULONG_PTR high)`.
/// JavaScriptCore borne sa pile avec ces valeurs : des zéros lui font
/// croire à un débordement de pile immédiat.
pub unsafe extern "win64" fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize) {
    let teb = crate::process::teb();
    let (mut lo, mut hi) = (0usize, 0usize);
    if teb != 0 {
        hi = *((teb + crate::process::TEB_STACK_BASE) as *const usize);
        lo = *((teb + TEB_DEALLOCATION_STACK) as *const usize);
        if lo == 0 {
            lo = *((teb + crate::process::TEB_STACK_LIMIT) as *const usize);
        }
    }
    if hi == 0 {
        // Pas de TEB : on décrit la pile pthread courante.
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        let (mut addr, mut size) = (std::ptr::null_mut(), 0usize);
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) == 0 {
            libc::pthread_attr_getstack(&attr, &mut addr, &mut size);
            libc::pthread_attr_destroy(&mut attr);
        }
        lo = addr as usize;
        hi = lo + size;
    }
    if !low.is_null() {
        *low = lo;
    }
    if !high.is_null() {
        *high = hi;
    }
}

/// Winlux n'implémente pas les fibres en mode « thread converti » : aucun
/// thread n'est une fibre tant que ConvertThreadToFiber n'a pas réussi.
pub unsafe extern "win64" fn IsThreadAFiber() -> i32 {
    0
}

/// `DebugBreak` : lève EXCEPTION_BREAKPOINT comme sous Windows. Sans
/// débogueur, le gestionnaire de plantage de Winlux la rapporte.
pub unsafe extern "win64" fn DebugBreak() {
    eprintln!("winlux: le programme a appelé DebugBreak (point d'arrêt explicite)");
    std::arch::asm!("int3");
}

// ---- WaitOnAddress / WakeByAddress* (synch-l1-2-0) -----------------------

fn futex_word(addr: usize) -> (usize, u32, u32) {
    // Le futex Linux travaille sur des mots de 32 bits alignés ; on attend
    // sur le mot qui contient l'adresse, et on compare la valeur exacte
    // (1, 2, 4 ou 8 octets) avant chaque attente.
    let word = addr & !3;
    (word, (addr - word) as u32, 0)
}

unsafe fn same_value(addr: usize, compare: usize, size: usize) -> bool {
    match size {
        1 => *(addr as *const u8) == *(compare as *const u8),
        2 => (addr as *const u16).read_unaligned() == (compare as *const u16).read_unaligned(),
        4 => (addr as *const u32).read_unaligned() == (compare as *const u32).read_unaligned(),
        8 => (addr as *const u64).read_unaligned() == (compare as *const u64).read_unaligned(),
        _ => false,
    }
}

/// `WaitOnAddress(addr, compare, size, ms)` : TRUE au réveil, FALSE avec
/// ERROR_TIMEOUT à l'expiration. Les réveils parasites sont permis par la
/// documentation (l'appelant revérifie toujours sa condition).
pub unsafe extern "win64" fn WaitOnAddress(addr: usize, compare: usize, size: usize, ms: u32) -> i32 {
    if addr == 0 || compare == 0 || !matches!(size, 1 | 2 | 4 | 8) {
        crate::process::set_last_error(87);
        return 0;
    }
    if !same_value(addr, compare, size) {
        return 1;
    }
    let (word, _, _) = futex_word(addr);
    let current = (*(word as *const AtomicU32)).load(Ordering::SeqCst);
    // Revérifie après la lecture du mot pour ne pas rater un changement.
    if !same_value(addr, compare, size) {
        return 1;
    }
    let ts;
    let tsp = if ms == u32::MAX {
        std::ptr::null()
    } else {
        ts = libc::timespec {
            tv_sec: (ms / 1000) as libc::time_t,
            tv_nsec: ((ms % 1000) as libc::c_long) * 1_000_000,
        };
        &ts as *const libc::timespec
    };
    let rc = libc::syscall(
        libc::SYS_futex,
        word as *const u32,
        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
        current,
        tsp,
    );
    if rc != 0 && *libc::__errno_location() == libc::ETIMEDOUT {
        crate::process::set_last_error(1460); // ERROR_TIMEOUT
        return 0;
    }
    1
}

unsafe fn wake(addr: usize, count: i32) {
    if addr == 0 {
        return;
    }
    let (word, _, _) = futex_word(addr);
    libc::syscall(
        libc::SYS_futex,
        word as *const u32,
        libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
        count,
    );
}

pub unsafe extern "win64" fn WakeByAddressSingle(addr: usize) {
    // Plusieurs adresses peuvent partager un mot de 32 bits : réveiller
    // tous les attentifs de ce mot reste correct (réveils parasites admis).
    wake(addr, i32::MAX);
}
pub unsafe extern "win64" fn WakeByAddressAll(addr: usize) {
    wake(addr, i32::MAX);
}

/// `RtlWaitOnAddress(addr, compare, size, PLARGE_INTEGER timeout)` :
/// NTSTATUS, délai NT (négatif = relatif en unités de 100 ns, NULL = infini).
pub unsafe extern "win64" fn RtlWaitOnAddress(addr: usize, compare: usize, size: usize, timeout: *const i64) -> u32 {
    let ms = if timeout.is_null() {
        u32::MAX
    } else {
        let t = *timeout;
        if t < 0 {
            ((-t) / 10_000).min(u32::MAX as i64 - 1) as u32
        } else {
            0
        }
    };
    if WaitOnAddress(addr, compare, size, ms) != 0 {
        0
    } else {
        0x102 // STATUS_TIMEOUT
    }
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetCurrentThreadStackLimits" => fn GetCurrentThreadStackLimits,
        "IsThreadAFiber" => fn IsThreadAFiber,
        "DebugBreak" => fn DebugBreak,
        "WaitOnAddress" => fn WaitOnAddress,
        "WakeByAddressSingle" => fn WakeByAddressSingle,
        "WakeByAddressAll" => fn WakeByAddressAll,
    }
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RtlWaitOnAddress" => fn RtlWaitOnAddress,
        "RtlWakeAddressSingle" => fn WakeByAddressSingle,
        "RtlWakeAddressAll" => fn WakeByAddressAll,
        "DbgBreakPoint" => fn DebugBreak,
    }
}
