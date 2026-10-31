//! Diagnostic des plantages : quand le programme Windows fait une faute
//! (accès mémoire invalide, instruction illégale…), on affiche un écran
//! bleu (BSOD) style Windows, on écrit un dump dans le journal, puis on
//! quitte avec le code d'exception Windows.
//!
//! Le dump contient aussi le module fautif et une pile d'appels lue sans
//! risque (chaque mot de pile passe par un tube : une adresse invalide
//! renvoie EFAULT au lieu de refaire planter le gestionnaire). Les échecs
//! hors signal (fonction Windows absente, erreur interne du runtime) sont
//! enregistrés par [`record_failure`]. `winlux-report` lit ce journal pour
//! proposer à l'utilisateur de signaler le problème.

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

const CRASH_PATH_CAP: usize = 1024;
static mut CRASH_EXE_PATH: [u8; CRASH_PATH_CAP] = [0; CRASH_PATH_CAP];
static mut CRASH_EXE_PATH_LEN: usize = 0;
/// Tube servant à tester la lisibilité d'une adresse sans faute.
static mut PROBE: [i32; 2] = [-1, -1];
const MAX_FRAMES: usize = 16;

/// Chemin Linux complet du programme, recopié dans le dump (masqué ensuite
/// par `winlux-report` avant tout envoi).
pub fn set_exe_path(path: &str) {
    let n = path.len().min(CRASH_PATH_CAP - 1);
    unsafe {
        let dst = std::ptr::addr_of_mut!(CRASH_EXE_PATH) as *mut u8;
        std::ptr::copy_nonoverlapping(path.as_ptr(), dst, n);
        CRASH_EXE_PATH_LEN = n;
    }
}

unsafe fn crash_exe_path() -> &'static [u8] {
    let base = std::ptr::addr_of!(CRASH_EXE_PATH) as *const u8;
    std::slice::from_raw_parts(base, CRASH_EXE_PATH_LEN)
}

unsafe fn crash_exe_name() -> &'static [u8] {
    let base = std::ptr::addr_of!(CRASH_EXE_NAME) as *const u8;
    std::slice::from_raw_parts(base, CRASH_EXE_LEN)
}

fn is_stack_overflow(addr: usize, rsp: usize) -> bool {
    // A faulting address within 64 KiB below the stack pointer strongly
    // suggests the thread hit its guard page.  On Linux the guard page
    // sits right below the mapped stack, so after a deep recursion RSP
    // passes the last valid page and the next touch faults.
    let page = 4096usize;
    let margin = 16 * page;
    addr < rsp && rsp.saturating_sub(addr) <= margin
}

/// Lit un mot mémoire sans risquer une nouvelle faute : `write` sur un tube
/// renvoie EFAULT pour une adresse non lisible. Async-signal-safe.
unsafe fn safe_read(addr: usize) -> Option<usize> {
    if PROBE[0] < 0 || addr == 0 || !addr.is_multiple_of(std::mem::size_of::<usize>()) {
        return None;
    }
    let n = libc::write(PROBE[1], addr as *const libc::c_void, 8);
    if n != 8 {
        return None;
    }
    let mut out = 0usize;
    let r = libc::read(PROBE[0], (&mut out as *mut usize).cast(), 8);
    (r == 8).then_some(out)
}

extern "C" {
    static __executable_start: u8;
    static etext: u8;
}

/// Situe une adresse : module PE chargé, ou code du runtime Winlux lui-même.
/// `try_lock` : on ne bloque jamais dans le gestionnaire de signal.
unsafe fn locate(addr: usize, out: &mut [u8; 96]) -> usize {
    let mut n = 0;
    if let Ok(modules) = crate::loader::MODULES.try_lock() {
        for m in modules.iter() {
            if m.contains(addr) {
                n = push(out, n, m.name.as_bytes());
                n = push(out, n, b"+0x");
                return push_hex(out, n, addr - m.base);
            }
        }
    }
    let start = std::ptr::addr_of!(__executable_start) as usize;
    let end = std::ptr::addr_of!(etext) as usize;
    if addr >= start && addr < end {
        n = push(out, n, b"winlux+0x");
        return push_hex(out, n, addr - start);
    }
    n
}

fn push(out: &mut [u8], n: usize, s: &[u8]) -> usize {
    let k = s.len().min(out.len() - n);
    out[n..n + k].copy_from_slice(&s[..k]);
    n + k
}

fn push_hex(out: &mut [u8], mut n: usize, v: usize) -> usize {
    let mut started = false;
    for i in (0..16).rev() {
        let d = (v >> (i * 4)) & 15;
        if d != 0 || started || i == 0 {
            started = true;
            n = push(out, n, &[b"0123456789abcdef"[d]]);
        }
    }
    n
}

unsafe fn likely_code(addr: usize) -> bool {
    if let Ok(modules) = crate::loader::MODULES.try_lock() {
        if let Some(m) = modules.iter().find(|m| m.contains(addr)) {
            return addr - m.base >= 0x1000;
        }
    }
    true
}

/// Pile d'appels : chaîne des pointeurs de cadre si elle est exploitable,
/// sinon balayage de la pile à la recherche d'adresses de retour situées dans
/// un module connu (cadres « probables », marqués `?`).
unsafe fn collect_frames(
    rip: usize,
    rsp: usize,
    rbp: usize,
    frames: &mut [(usize, bool); MAX_FRAMES],
) -> usize {
    let mut count = 0;
    frames[count] = (rip, true);
    count += 1;
    let mut fp = rbp;
    while count < MAX_FRAMES && fp >= rsp && fp - rsp < 8 * 1024 * 1024 {
        let (Some(next), Some(ret)) = (safe_read(fp), safe_read(fp + 8)) else {
            break;
        };
        let mut tmp = [0u8; 96];
        if ret == 0 || locate(ret, &mut tmp) == 0 {
            break;
        }
        frames[count] = (ret, true);
        count += 1;
        if next <= fp {
            break;
        }
        fp = next;
    }
    if count < 3 {
        let mut at = rsp & !7;
        let limit = at + 4096 * 2;
        while count < MAX_FRAMES && at < limit {
            let Some(word) = safe_read(at) else {
                break;
            };
            let mut tmp = [0u8; 96];
            // Une adresse de retour vise du code : on écarte la base d'un
            // module (en-têtes PE) et les doublons consécutifs.
            if word != frames[count - 1].0 && likely_code(word) && locate(word, &mut tmp) != 0 {
                frames[count] = (word, false);
                count += 1;
            }
            at += 8;
        }
    }
    count
}

extern "C" fn on_signal(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let code = report(sig, info, ctx);
        libc::_exit(code as i32);
    }
}

/// Écrit le diagnostic d'une faute (journal, écran bleu, stderr) et renvoie
/// le code d'exception Windows. Appelé aussi par le gestionnaire de
/// `quality`, qui s'installe après celui-ci, avant son propre dump.
pub(crate) unsafe fn report(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) -> u32 {
    {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
        let rbp = (*uc).uc_mcontext.gregs[libc::REG_RBP as usize] as usize;
        let rax = (*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as usize;
        let rcx = (*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as usize;
        let rdx = (*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as usize;
        let addr = (*info).si_addr() as usize;
        let (what, stop_code, code) = match sig {
            libc::SIGSEGV | libc::SIGBUS if is_stack_overflow(addr, rsp) => (
                b"EXCEPTION_STACK_OVERFLOW".as_slice(),
                b"KERNEL_STACK_INPAGE_ERROR".as_slice(),
                0xC000_00FDu32,
            ),
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

        let mut module = [0u8; 96];
        let module_len = locate(rip, &mut module);
        let mut frames = [(0usize, false); MAX_FRAMES];
        let frame_count = collect_frames(rip, rsp, rbp, &mut frames);

        // Écrire le dump dans le journal
        write_crash_log(
            sig,
            what,
            stop_code,
            code,
            [rip, rsp, rbp, rax, rcx, rdx, addr],
            &module[..module_len],
            &frames[..frame_count],
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
        if module_len > 0 {
            write_literal(b"winlux: module=");
            write_literal(&module[..module_len]);
            write_literal(b"\n");
        }
        code
    }
}

unsafe fn write_crash_log(
    sig: i32,
    what: &[u8],
    stop_code: &[u8],
    code: u32,
    regs: [usize; 7],
    module: &[u8],
    frames: &[(usize, bool)],
) {
    let [rip, rsp, rbp, rax, rcx, rdx, addr] = regs;
    libc::mkdir(c"/run/winlux".as_ptr(), 0o755);
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
    write_fd(fd, b" module=");
    write_fd(
        fd,
        if module.is_empty() {
            b"inconnu"
        } else {
            module
        },
    );
    write_fd(fd, b" pid=");
    write_fd_dec(fd, libc::getpid() as u64);
    if CRASH_EXE_PATH_LEN > 0 {
        write_fd(fd, b"\n  path=");
        write_fd(fd, crash_exe_path());
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
    for (i, &(frame, sure)) in frames.iter().enumerate() {
        write_fd(fd, b"  frame#");
        write_fd_dec(fd, i as u64);
        write_fd(fd, if sure { b" 0x" } else { b" ?0x" });
        write_fd_hex(fd, frame);
        let mut place = [0u8; 96];
        let n = locate(frame, &mut place);
        if n > 0 {
            write_fd(fd, b" ");
            write_fd(fd, &place[..n]);
        }
        write_fd(fd, b"\n");
    }

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
        write_fd(bfd, module);
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

/// Échec sans signal (fonction Windows absente, erreur interne du runtime) :
/// même format de journal que les fautes, plus un fichier `crash-offer` que
/// le bureau surveille pour proposer le signalement sans écran bleu.
pub fn record_failure(exception: &str, detail: &str) {
    use std::io::Write;
    let dir = std::path::Path::new("/run/winlux/journal");
    let _ = std::fs::create_dir_all(dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let exe = unsafe { String::from_utf8_lossy(crash_exe_name()).into_owned() };
    let path = unsafe { String::from_utf8_lossy(crash_exe_path()).into_owned() };
    let exe = if exe.is_empty() {
        "unknown".into()
    } else {
        exe
    };
    let mut entry = format!(
        "[CRASH] ts={ts} sig=0 stop=APPLICATION_FAILURE exception={exception} code=0x0 exe={exe} module=inconnu pid={}\n",
        std::process::id()
    );
    if !path.is_empty() {
        entry.push_str(&format!("  path={path}\n"));
    }
    for line in detail.lines().take(40) {
        entry.push_str(&format!("  detail={line}\n"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("crash.log"))
    {
        let _ = f.write_all(entry.as_bytes());
    }
    let _ = std::fs::write("/run/winlux/crash-offer", format!("{ts}\n{exe}\n"));
}

pub fn install() {
    unsafe {
        let mut fds = [-1i32; 2];
        if libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) == 0 {
            PROBE = fds;
        }
        // Erreur interne du runtime (panique Rust) : même chemin de signalement.
        std::panic::set_hook(Box::new(|info| {
            let detail = format!("{info}");
            eprintln!("winlux: erreur interne : {detail}");
            record_failure("WINLUX_INTERNAL_ERROR", &detail);
        }));
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
