//! Diagnostic des plantages : quand le programme Windows fait une faute
//! (accès mémoire invalide, instruction illégale…), on affiche où, dans
//! quel module, puis on quitte avec le code d'exception Windows.
//!
//! Toutes les opérations dans le gestionnaire de signal sont
//! async-signal-safe : pas d'allocation, pas de verrou bloquant.

use std::sync::atomic::{AtomicUsize, Ordering};

static mut CRASH_LOG_PATH: [u8; 256] = [0; 256];
static CRASH_LOG_LEN: AtomicUsize = AtomicUsize::new(0);

extern "C" fn on_signal(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
        let addr = (*info).si_addr() as usize;
        let (what, code) = match sig {
            libc::SIGSEGV | libc::SIGBUS => (
                b"acces memoire invalide (EXCEPTION_ACCESS_VIOLATION)\n".as_slice(),
                0xC000_0005u32,
            ),
            libc::SIGILL => (
                b"instruction illegale (EXCEPTION_ILLEGAL_INSTRUCTION)\n".as_slice(),
                0xC000_001D,
            ),
            libc::SIGTRAP => (
                b"point d'arret logiciel (EXCEPTION_BREAKPOINT)\n".as_slice(),
                0x8000_0003,
            ),
            libc::SIGFPE => (
                b"division par zero (EXCEPTION_INT_DIVIDE_BY_ZERO)\n".as_slice(),
                0xC000_0094,
            ),
            _ => (b"signal inattendu\n".as_slice(), 0xC000_0001),
        };
        // Un gestionnaire de signal ne doit ni allouer ni prendre de verrou :
        // le processus peut avoir été interrompu au milieu de malloc ou d'un
        // verrou du chargeur. write/_exit sont async-signal-safe.
        write_literal(b"\nwinlux: le programme a plante : ");
        write_literal(what);
        write_literal(b"winlux: rip=0x");
        write_hex(rip);
        write_literal(b" addr=0x");
        write_hex(addr);
        write_literal(b" rsp=0x");
        write_hex(rsp);
        write_literal(b"\n");

        // Dump complet des registres x86-64
        write_literal(b"winlux: rax=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as usize);
        write_literal(b" rbx=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RBX as usize] as usize);
        write_literal(b" rcx=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as usize);
        write_literal(b" rdx=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as usize);
        write_literal(b"\nwinlux: rsi=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RSI as usize] as usize);
        write_literal(b" rdi=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RDI as usize] as usize);
        write_literal(b" rbp=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_RBP as usize] as usize);
        write_literal(b"\nwinlux: r8=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R8 as usize] as usize);
        write_literal(b" r9=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R9 as usize] as usize);
        write_literal(b" r10=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R10 as usize] as usize);
        write_literal(b" r11=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R11 as usize] as usize);
        write_literal(b"\nwinlux: r12=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R12 as usize] as usize);
        write_literal(b" r13=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R13 as usize] as usize);
        write_literal(b" r14=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R14 as usize] as usize);
        write_literal(b" r15=0x");
        write_hex((*uc).uc_mcontext.gregs[libc::REG_R15 as usize] as usize);
        write_literal(b"\n");

        // Identification du module (try_lock pour rester async-signal-safe)
        if let Ok(modules) = crate::loader::MODULES.try_lock() {
            for m in modules.iter() {
                if m.contains(rip) {
                    write_literal(b"winlux: module=");
                    write_literal(m.name.as_bytes());
                    write_literal(b"+0x");
                    write_hex(rip - m.base);
                    write_literal(b"\n");
                    break;
                }
            }
        }

        // Écriture du crash dans un fichier de log (async-signal-safe)
        let log_len = CRASH_LOG_LEN.load(Ordering::Relaxed);
        if log_len > 0 {
            let fd = libc::open(
                std::ptr::addr_of!(CRASH_LOG_PATH) as *const libc::c_char,
                libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
                0o644,
            );
            if fd >= 0 {
                let saved_fd = CRASH_LOG_FD;
                CRASH_LOG_FD = fd;
                write_to_log(b"--- crash ");
                write_hex_to_log(code as usize);
                write_to_log(b" ---\nrip=0x");
                write_hex_to_log(rip);
                write_to_log(b" addr=0x");
                write_hex_to_log(addr);
                write_to_log(b" rsp=0x");
                write_hex_to_log(rsp);
                write_to_log(b"\nrax=0x");
                write_hex_to_log((*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as usize);
                write_to_log(b" rbx=0x");
                write_hex_to_log((*uc).uc_mcontext.gregs[libc::REG_RBX as usize] as usize);
                write_to_log(b" rcx=0x");
                write_hex_to_log((*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as usize);
                write_to_log(b" rdx=0x");
                write_hex_to_log((*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as usize);
                write_to_log(b"\n");
                libc::close(fd);
                CRASH_LOG_FD = saved_fd;
            }
        }

        libc::_exit(code as i32);
    }
}

static mut CRASH_LOG_FD: i32 = -1;

unsafe fn write_literal(s: &[u8]) {
    let _ = libc::write(2, s.as_ptr() as *const libc::c_void, s.len());
}

unsafe fn write_hex(mut n: usize) {
    let mut b = [0u8; 16];
    for i in (0..16).rev() {
        b[i] = b"0123456789abcdef"[n & 15];
        n >>= 4;
    }
    write_literal(&b);
}

unsafe fn write_to_log(s: &[u8]) {
    if CRASH_LOG_FD >= 0 {
        let _ = libc::write(CRASH_LOG_FD, s.as_ptr() as *const libc::c_void, s.len());
    }
}

unsafe fn write_hex_to_log(mut n: usize) {
    let mut b = [0u8; 16];
    for i in (0..16).rev() {
        b[i] = b"0123456789abcdef"[n & 15];
        n >>= 4;
    }
    write_to_log(&b);
}

pub fn install() {
    unsafe {
        // Pré-calcule le chemin du fichier crash.log (async-signal-safe)
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.as_encoded_bytes();
            let suffix = b"/.local/share/winlux/crash.log";
            let total = home.len() + suffix.len();
            if total < 256 {
                let base = std::ptr::addr_of_mut!(CRASH_LOG_PATH) as *mut u8;
                std::ptr::copy_nonoverlapping(home.as_ptr(), base, home.len());
                std::ptr::copy_nonoverlapping(suffix.as_ptr(), base.add(home.len()), suffix.len());
                *base.add(total) = 0;
                CRASH_LOG_LEN.store(total, Ordering::Relaxed);
            }
        }

        // Pile de secours pour pouvoir diagnostiquer un débordement de pile.
        let size = 64 * 1024;
        let stack = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        let ss = libc::stack_t {
            ss_sp: stack,
            ss_flags: 0,
            ss_size: size,
        };
        libc::sigaltstack(&ss, std::ptr::null_mut());
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        for s in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGTRAP,
        ] {
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
}
