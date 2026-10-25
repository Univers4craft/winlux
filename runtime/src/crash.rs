//! Diagnostic des plantages : quand le programme Windows fait une faute
//! (accès mémoire invalide, instruction illégale…), on affiche où, dans
//! quel module, puis on quitte avec le code d'exception Windows.

fn is_stack_overflow(addr: usize, rsp: usize) -> bool {
    // A faulting address within 64 KiB below the stack pointer strongly
    // suggests the thread hit its guard page.  On Linux the guard page
    // sits right below the mapped stack, so after a deep recursion RSP
    // passes the last valid page and the next touch faults.
    let page = 4096usize;
    let margin = 16 * page;
    addr < rsp && rsp.saturating_sub(addr) <= margin
}

extern "C" fn on_signal(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
        let addr = (*info).si_addr() as usize;
        let (what, code) = match sig {
            libc::SIGSEGV | libc::SIGBUS if is_stack_overflow(addr, rsp) => (
                b"debordement de pile (EXCEPTION_STACK_OVERFLOW)\n".as_slice(),
                0xC000_00FDu32,
            ),
            libc::SIGSEGV | libc::SIGBUS => (
                b"acces memoire invalide (EXCEPTION_ACCESS_VIOLATION)\n".as_slice(),
                0xC000_0005u32,
            ),
            libc::SIGILL => (
                b"instruction illegale (EXCEPTION_ILLEGAL_INSTRUCTION)\n".as_slice(),
                0xC000_001D,
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
        libc::_exit(code as i32);
    }
}

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

pub fn install() {
    unsafe {
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
        if stack == libc::MAP_FAILED {
            return;
        }
        let ss = libc::stack_t {
            ss_sp: stack,
            ss_flags: 0,
            ss_size: size,
        };
        libc::sigaltstack(&ss, std::ptr::null_mut());
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        for s in [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGFPE] {
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
}
