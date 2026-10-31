//! Diagnostic des plantages : quand le programme Windows fait une faute
//! (accès mémoire invalide, instruction illégale…), on affiche un écran
//! bleu (BSOD) style Windows, on écrit un dump dans le journal, puis on
//! quitte avec le code d'exception Windows.

const CRASH_EXE_CAP: usize = 256;
static mut CRASH_EXE_NAME: [u8; CRASH_EXE_CAP] = [0; CRASH_EXE_CAP];
static mut CRASH_EXE_LEN: usize = 0;

pub fn set_exe_name(name: &str) {
    let n = name.len().min(CRASH_EXE_CAP - 1);
    unsafe {
        let dst = std::ptr::addr_of_mut!(CRASH_EXE_NAME) as *mut u8;
        std::ptr::copy_nonoverlapping(name.as_ptr(), dst, n);
        CRASH_EXE_LEN = n;
    }
}

unsafe fn crash_exe_name() -> &'static [u8] {
    let base = std::ptr::addr_of!(CRASH_EXE_NAME) as *const u8;
    std::slice::from_raw_parts(base, CRASH_EXE_LEN)
}

extern "C" fn on_signal(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
        let rbp = (*uc).uc_mcontext.gregs[libc::REG_RBP as usize] as usize;
        let rax = (*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as usize;
        let rcx = (*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as usize;
        let rdx = (*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as usize;
        let addr = (*info).si_addr() as usize;
        let (what, stop_code, code) = match sig {
            libc::SIGSEGV | libc::SIGBUS => (
                b"EXCEPTION_ACCESS_VIOLATION".as_slice(),
                b"IRQL_NOT_LESS_OR_EQUAL".as_slice(),
                0xC000_0005u32,
            ),
            libc::SIGILL => (
                b"EXCEPTION_ILLEGAL_INSTRUCTION".as_slice(),
                b"KMODE_EXCEPTION_NOT_HANDLED".as_slice(),
                0xC000_001D,
            ),
            libc::SIGTRAP => (
                b"EXCEPTION_BREAKPOINT".as_slice(),
                b"CRITICAL_PROCESS_DIED".as_slice(),
                0x8000_0003,
            ),
            libc::SIGFPE => (
                b"EXCEPTION_INT_DIVIDE_BY_ZERO".as_slice(),
                b"DIVIDE_BY_ZERO_ERROR".as_slice(),
                0xC000_0094,
            ),
            _ => (
                b"UNEXPECTED_SIGNAL".as_slice(),
                b"UNEXPECTED_KERNEL_MODE_TRAP".as_slice(),
                0xC000_0001,
            ),
        };

        // Écrire le dump dans le journal
        write_crash_log(
            sig, what, stop_code, code, rip, rsp, rbp, rax, rcx, rdx, addr,
        );

        // Afficher le diagnostic sur stderr
        write_literal(b"\nwinlux: le programme a plante : ");
        write_literal(what);
        write_literal(b"\nwinlux: stop code: ");
        write_literal(stop_code);
        write_literal(b"\nwinlux: rip=0x");
        write_hex(rip);
        write_literal(b" addr=0x");
        write_hex(addr);
        write_literal(b" rsp=0x");
        write_hex(rsp);
        write_literal(b"\nwinlux: rbp=0x");
        write_hex(rbp);
        write_literal(b" rax=0x");
        write_hex(rax);
        write_literal(b" rcx=0x");
        write_hex(rcx);
        write_literal(b" rdx=0x");
        write_hex(rdx);
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

        libc::_exit(code as i32);
    }
}

unsafe fn write_crash_log(
    sig: i32,
    what: &[u8],
    stop_code: &[u8],
    code: u32,
    rip: usize,
    rsp: usize,
    rbp: usize,
    rax: usize,
    rcx: usize,
    rdx: usize,
    addr: usize,
) {
    let dir = b"/run/winlux/journal\0";
    libc::mkdir(dir.as_ptr() as *const i8, 0o755);
    let path = b"/run/winlux/journal/crash.log\0";
    let fd = libc::open(
        path.as_ptr() as *const i8,
        libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
        0o644,
    );
    if fd < 0 {
        return;
    }

    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);

    write_fd(fd, b"[CRASH] ts=");
    write_fd_dec(fd, ts.tv_sec as u64);
    write_fd(fd, b" sig=");
    write_fd_dec(fd, sig as u64);
    write_fd(fd, b" stop=");
    write_fd(fd, stop_code);
    write_fd(fd, b" exception=");
    write_fd(fd, what);
    write_fd(fd, b" code=0x");
    write_fd_hex(fd, code as usize);
    write_fd(fd, b" exe=");
    if CRASH_EXE_LEN > 0 {
        write_fd(fd, crash_exe_name());
    } else {
        write_fd(fd, b"unknown");
    }
    write_fd(fd, b"\n  rip=0x");
    write_fd_hex(fd, rip);
    write_fd(fd, b" rsp=0x");
    write_fd_hex(fd, rsp);
    write_fd(fd, b" rbp=0x");
    write_fd_hex(fd, rbp);
    write_fd(fd, b"\n  rax=0x");
    write_fd_hex(fd, rax);
    write_fd(fd, b" rcx=0x");
    write_fd_hex(fd, rcx);
    write_fd(fd, b" rdx=0x");
    write_fd_hex(fd, rdx);
    write_fd(fd, b" addr=0x");
    write_fd_hex(fd, addr);
    write_fd(fd, b"\n");

    libc::close(fd);

    // Écrire aussi un fichier BSOD que le shell peut lire pour afficher l'écran bleu
    let bsod_path = b"/run/winlux/crash-bsod\0";
    let bfd = libc::open(
        bsod_path.as_ptr() as *const i8,
        libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        0o644,
    );
    if bfd >= 0 {
        write_fd(bfd, stop_code);
        write_fd(bfd, b"\n");
        write_fd(bfd, what);
        write_fd(bfd, b"\n0x");
        write_fd_hex(bfd, code as usize);
        write_fd(bfd, b"\n0x");
        write_fd_hex(bfd, rip);
        write_fd(bfd, b"\n");
        if CRASH_EXE_LEN > 0 {
            write_fd(bfd, crash_exe_name());
        } else {
            write_fd(bfd, b"unknown.exe");
        }
        write_fd(bfd, b"\n");
        libc::close(bfd);
    }
}

unsafe fn write_fd(fd: i32, s: &[u8]) {
    let _ = libc::write(fd, s.as_ptr() as *const libc::c_void, s.len());
}

unsafe fn write_fd_hex(fd: i32, mut n: usize) {
    let mut b = [0u8; 16];
    for i in (0..16).rev() {
        b[i] = b"0123456789abcdef"[n & 15];
        n >>= 4;
    }
    write_fd(fd, &b);
}

unsafe fn write_fd_dec(fd: i32, mut n: u64) {
    let mut b = [0u8; 20];
    let mut i = 20;
    if n == 0 {
        write_fd(fd, b"0");
        return;
    }
    while n > 0 {
        i -= 1;
        b[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    write_fd(fd, &b[i..]);
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
