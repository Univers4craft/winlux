//! Diagnostic des plantages : quand le programme Windows fait une faute
//! (accès mémoire invalide, instruction illégale…), on affiche un écran
//! bleu (BSOD) style Windows, on écrit un dump dans le journal, puis on
//! quitte avec le code d'exception Windows.

static mut CRASH_EXE_NAME: [u8; 256] = [0; 256];
static mut CRASH_EXE_LEN: usize = 0;

pub fn set_exe_name(name: &str) {
    unsafe {
        let buf = &raw mut CRASH_EXE_NAME;
        let n = name.len().min((*buf).len() - 1);
        (&mut (*buf))[..n].copy_from_slice(&name.as_bytes()[..n]);
        CRASH_EXE_LEN = n;
    }
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
        write_fd(fd, &CRASH_EXE_NAME[..CRASH_EXE_LEN]);
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
            write_fd(bfd, &CRASH_EXE_NAME[..CRASH_EXE_LEN]);
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
