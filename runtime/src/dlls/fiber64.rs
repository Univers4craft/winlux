//! Fibres des programmes 64 bits (#654).
//!
//! Une fibre est un bloc dont le premier mot est le paramètre :
//! `GetCurrentFiber()` lit gs:[0x20] (NT_TIB.FiberData) et `GetFiberData()`
//! lit `*(gs:[0x20])`, deux fonctions en ligne des en-têtes publics.
//! SwitchToFiber est écrit en assembleur : il sauve les registres que la
//! convention win64 préserve (rbx, rbp, rdi, rsi, r12-r15, xmm6-xmm15, plus
//! MXCSR et le mot de contrôle x87), note la pile et les bornes de pile du
//! TEB, puis reprend celles de la fibre visée. Les données FLS restent
//! celles du thread.
//!
//! Disposition du bloc (la nôtre ; seul le premier mot est public) :
//! +0 paramètre, +8 pile sauvegardée, +0x10 base de pile, +0x18 limite de
//! pile, +0x20 routine, +0x28 début de la pile allouée, +0x30 sa taille,
//! +0x38 drapeaux.
//!
//! Référence : documentation publique de CreateFiberEx, SwitchToFiber,
//! ConvertThreadToFiberEx et de la convention d'appel x64.

const TEB_FIBER_DATA: usize = 0x20;
const PAGE: usize = 4096;
const DEFAULT_STACK: usize = 1024 * 1024;
/// Fibre née d'un thread (ConvertThreadToFiber) : pas de pile à elle.
const FLAG_THREAD: usize = 1;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const ERROR_ALREADY_FIBER: u32 = 1280;
const ERROR_ALREADY_THREAD: u32 = 1281;

#[repr(C)]
struct Fiber {
    param: usize,
    rsp: usize,
    base: usize,
    limit: usize,
    start: usize,
    stack: usize,
    stack_size: usize,
    flags: usize,
}

// Taille du cadre que SwitchToFiber laisse sur la pile quittée : 10 xmm
// (0xA0), MXCSR et mot de contrôle x87 (8), 8 registres, adresse de retour.
const FRAME_XMM: usize = 0xA8;
const FRAME: usize = FRAME_XMM + 8 * 8 + 8;

std::arch::global_asm!(
    ".p2align 4",
    ".globl winlux_switch_to_fiber64",
    "winlux_switch_to_fiber64:",
    "mov rax, qword ptr gs:[0x30]",
    "mov rdx, qword ptr [rax+0x20]",
    "test rdx, rdx",
    "jz 2f",
    "test rcx, rcx",
    "jz 2f",
    "push rbx",
    "push rbp",
    "push rdi",
    "push rsi",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "sub rsp, 0xA8",
    "movdqu xmmword ptr [rsp], xmm6",
    "movdqu xmmword ptr [rsp+0x10], xmm7",
    "movdqu xmmword ptr [rsp+0x20], xmm8",
    "movdqu xmmword ptr [rsp+0x30], xmm9",
    "movdqu xmmword ptr [rsp+0x40], xmm10",
    "movdqu xmmword ptr [rsp+0x50], xmm11",
    "movdqu xmmword ptr [rsp+0x60], xmm12",
    "movdqu xmmword ptr [rsp+0x70], xmm13",
    "movdqu xmmword ptr [rsp+0x80], xmm14",
    "movdqu xmmword ptr [rsp+0x90], xmm15",
    "stmxcsr dword ptr [rsp+0xA0]",
    "fnstcw word ptr [rsp+0xA4]",
    "mov r8, qword ptr [rax+0x08]",
    "mov qword ptr [rdx+0x10], r8",
    "mov r8, qword ptr [rax+0x10]",
    "mov qword ptr [rdx+0x18], r8",
    "mov qword ptr [rdx+0x08], rsp",
    "mov qword ptr [rax+0x20], rcx",
    "mov r8, qword ptr [rcx+0x10]",
    "mov qword ptr [rax+0x08], r8",
    "mov r8, qword ptr [rcx+0x18]",
    "mov qword ptr [rax+0x10], r8",
    "mov rsp, qword ptr [rcx+0x08]",
    "ldmxcsr dword ptr [rsp+0xA0]",
    "fldcw word ptr [rsp+0xA4]",
    "movdqu xmm6, xmmword ptr [rsp]",
    "movdqu xmm7, xmmword ptr [rsp+0x10]",
    "movdqu xmm8, xmmword ptr [rsp+0x20]",
    "movdqu xmm9, xmmword ptr [rsp+0x30]",
    "movdqu xmm10, xmmword ptr [rsp+0x40]",
    "movdqu xmm11, xmmword ptr [rsp+0x50]",
    "movdqu xmm12, xmmword ptr [rsp+0x60]",
    "movdqu xmm13, xmmword ptr [rsp+0x70]",
    "movdqu xmm14, xmmword ptr [rsp+0x80]",
    "movdqu xmm15, xmmword ptr [rsp+0x90]",
    "add rsp, 0xA8",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rsi",
    "pop rdi",
    "pop rbp",
    "pop rbx",
    "2:",
    "ret",
    // Première bascule vers une fibre : « retour » ici, avec rsp comme à
    // l'entrée d'une fonction. On appelle start(param), puis on termine le
    // thread, comme Windows quand la routine d'une fibre revient.
    ".p2align 4",
    ".globl winlux_fiber64_start",
    "winlux_fiber64_start:",
    "mov rax, qword ptr gs:[0x30]",
    "mov rax, qword ptr [rax+0x20]",
    "mov rcx, qword ptr [rax]",
    "sub rsp, 0x28",
    "call qword ptr [rax+0x20]",
    "xor ecx, ecx",
    "call {exit}",
    "int3",
    exit = sym fiber_returned,
);

extern "win64" {
    fn winlux_switch_to_fiber64(fiber: usize);
    fn winlux_fiber64_start();
}

/// Fin de la routine d'une fibre : le thread se termine.
extern "win64" fn fiber_returned(code: u32) {
    crate::dlls::kernel32_misc::exit_thread(code);
}

fn current() -> usize {
    let t = crate::process::teb();
    if t == 0 {
        return 0;
    }
    unsafe { *((t + TEB_FIBER_DATA) as *const usize) }
}

fn set_error(code: u32) {
    crate::process::set_last_error(code);
}

/// ConvertThreadToFiberEx : le thread devient une fibre.
pub extern "win64" fn convert_thread_to_fiber_ex(param: usize, _flags: u32) -> usize {
    let t = crate::process::teb();
    if t == 0 {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if current() != 0 {
        set_error(ERROR_ALREADY_FIBER);
        return 0;
    }
    let f = Box::into_raw(Box::new(Fiber {
        param,
        rsp: 0,
        base: unsafe { *((t + crate::process::TEB_STACK_BASE) as *const usize) },
        limit: unsafe { *((t + crate::process::TEB_STACK_LIMIT) as *const usize) },
        start: 0,
        stack: 0,
        stack_size: 0,
        flags: FLAG_THREAD,
    })) as usize;
    unsafe { *((t + TEB_FIBER_DATA) as *mut usize) = f };
    f
}

pub extern "win64" fn convert_thread_to_fiber(param: usize) -> usize {
    convert_thread_to_fiber_ex(param, 0)
}

/// ConvertFiberToThread : le thread n'est plus une fibre.
pub extern "win64" fn convert_fiber_to_thread() -> i32 {
    let f = current();
    if f == 0 {
        set_error(ERROR_ALREADY_THREAD);
        return 0;
    }
    let t = crate::process::teb();
    unsafe {
        *((t + TEB_FIBER_DATA) as *mut usize) = 0;
        if (*(f as *const Fiber)).flags & FLAG_THREAD != 0 {
            drop(Box::from_raw(f as *mut Fiber));
        }
    }
    1
}

/// CreateFiberEx : bloc et pile à part (page de garde en bas) ; la
/// première bascule vers la fibre « revient » dans le tremplin.
pub extern "win64" fn create_fiber_ex(
    commit: usize,
    reserve: usize,
    _flags: u32,
    start: usize,
    param: usize,
) -> usize {
    if start == 0 {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let stack = if reserve == 0 && commit == 0 {
        DEFAULT_STACK
    } else {
        (reserve.max(commit).max(64 * 1024) + 0xFFFF) & !0xFFFF
    };
    let size = stack + PAGE;
    let base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        set_error(ERROR_NOT_ENOUGH_MEMORY);
        return 0;
    }
    let base = base as usize;
    unsafe { libc::mprotect(base as *mut libc::c_void, PAGE, libc::PROT_NONE) };
    let top = base + size;
    // Après le `ret` vers le tremplin, rsp doit valoir 8 modulo 16, comme
    // à l'entrée d'une fonction ; on laisse 32 octets de marge au-dessus.
    let after_ret = top - 40;
    let rsp = after_ret - FRAME;
    unsafe {
        // Pile mappée à zéro : xmm et registres à zéro.
        *((rsp + 0xA0) as *mut u32) = 0x1F80; // MXCSR par défaut
        *((rsp + 0xA4) as *mut u16) = 0x027F; // x87 par défaut (Windows x64)
        *((rsp + FRAME - 8) as *mut usize) = winlux_fiber64_start as *const () as usize;
    }
    Box::into_raw(Box::new(Fiber {
        param,
        rsp,
        base: top,
        limit: base + PAGE,
        start,
        stack: base,
        stack_size: size,
        flags: 0,
    })) as usize
}

pub extern "win64" fn create_fiber(stack: usize, start: usize, param: usize) -> usize {
    create_fiber_ex(stack, stack, 0, start, param)
}

/// DeleteFiber : libère la pile ; supprimer la fibre courante termine le
/// thread, comme sous Windows.
pub extern "win64" fn delete_fiber(f: usize) {
    if f == 0 {
        return;
    }
    if current() == f {
        crate::dlls::kernel32_misc::exit_thread(0);
    }
    unsafe {
        let b = Box::from_raw(f as *mut Fiber);
        if b.stack != 0 {
            libc::munmap(b.stack as *mut libc::c_void, b.stack_size);
        }
    }
}

pub extern "win64" fn switch_to_fiber(f: usize) {
    unsafe { winlux_switch_to_fiber64(f) }
}

pub extern "win64" fn is_thread_a_fiber() -> i32 {
    (current() != 0) as i32
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "ConvertThreadToFiber" => fn convert_thread_to_fiber,
        "ConvertThreadToFiberEx" => fn convert_thread_to_fiber_ex,
        "ConvertFiberToThread" => fn convert_fiber_to_thread,
        "CreateFiber" => fn create_fiber,
        "CreateFiberEx" => fn create_fiber_ex,
        "DeleteFiber" => fn delete_fiber,
        "SwitchToFiber" => fn switch_to_fiber,
        "IsThreadAFiber" => fn is_thread_a_fiber,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static MAIN: AtomicUsize = AtomicUsize::new(0);
    static SEEN_PARAM: AtomicUsize = AtomicUsize::new(0);
    static SEEN_CURRENT: AtomicUsize = AtomicUsize::new(0);
    static SEEN_LIMIT: AtomicUsize = AtomicUsize::new(0);
    static ROUNDS: AtomicUsize = AtomicUsize::new(0);

    extern "win64" fn routine(param: usize) {
        // Pas d'assert ici : on est sur la pile de la fibre.
        let t = crate::process::teb();
        let f = unsafe { *((t + TEB_FIBER_DATA) as *const usize) };
        SEEN_CURRENT.store(f, Ordering::SeqCst);
        SEEN_PARAM.store(unsafe { *(f as *const usize) }, Ordering::SeqCst);
        SEEN_LIMIT.store(
            unsafe { *((t + crate::process::TEB_STACK_LIMIT) as *const usize) },
            Ordering::SeqCst,
        );
        // Calcul flottant sur la pile de la fibre (alignement 16 requis).
        let mut acc = 0.0f64;
        for i in 0..64 {
            acc += (i as f64).sqrt();
        }
        std::hint::black_box(acc);
        loop {
            ROUNDS.fetch_add(param, Ordering::SeqCst);
            switch_to_fiber(MAIN.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn bascule_entre_fibres() {
        let _teb = crate::process::test_teb();
        let t = crate::process::teb();
        unsafe {
            *((t + crate::process::TEB_STACK_BASE) as *mut usize) = 0x7000_0000;
            *((t + crate::process::TEB_STACK_LIMIT) as *mut usize) = 0x6000_0000;
        }
        assert_eq!(is_thread_a_fiber(), 0);
        assert_eq!(convert_fiber_to_thread(), 0);
        let main = convert_thread_to_fiber(0x1234);
        assert_ne!(main, 0);
        assert_eq!(convert_thread_to_fiber(0), 0);
        assert_eq!(crate::process::last_error(), ERROR_ALREADY_FIBER);
        assert_eq!(is_thread_a_fiber(), 1);
        MAIN.store(main, Ordering::SeqCst);
        let other = create_fiber(0, routine as *const () as usize, 7);
        assert_ne!(other, 0);
        // Valeurs dans des registres préservés : elles doivent survivre.
        let keep = std::hint::black_box(0.5f64);
        switch_to_fiber(other);
        assert_eq!(SEEN_CURRENT.load(Ordering::SeqCst), other);
        assert_eq!(SEEN_PARAM.load(Ordering::SeqCst), 7);
        assert_ne!(SEEN_LIMIT.load(Ordering::SeqCst), 0x6000_0000);
        assert_eq!(ROUNDS.load(Ordering::SeqCst), 7);
        switch_to_fiber(other);
        assert_eq!(ROUNDS.load(Ordering::SeqCst), 14);
        assert_eq!(keep, 0.5);
        unsafe {
            assert_eq!(*((t + TEB_FIBER_DATA) as *const usize), main);
            assert_eq!(*(main as *const usize), 0x1234);
            assert_eq!(
                *((t + crate::process::TEB_STACK_LIMIT) as *const usize),
                0x6000_0000
            );
        }
        delete_fiber(other);
        assert_eq!(convert_fiber_to_thread(), 1);
        assert_eq!(is_thread_a_fiber(), 0);
        assert_eq!(create_fiber(0, 0, 0), 0);
    }
}
