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
    crate::log::journal_event("session", &format!("démarrage de {path}"));
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
    if addr < rsp && rsp.saturating_sub(addr) <= margin {
        return true;
    }
    // Faute dans la dernière page de la pile d'un thread Windows, même
    // au-dessus de RSP (`sub rsp, N` puis écriture dans la nouvelle trame) :
    // limites lues dans le TEB (#15). arch_prctl est async-signal-safe.
    let teb = crate::process::teb();
    if teb == 0 {
        return false;
    }
    let limit = unsafe { safe_read(teb + crate::process::TEB_STACK_LIMIT) }.unwrap_or(0);
    limit != 0 && addr < limit + page && addr + 64 * page >= limit && rsp < limit + 64 * page
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
        // Faute du programme Windows : exception SEH qu'il peut rattraper.
        if crate::dlls::seh_hw::on_fault(sig, info, ctx) {
            return;
        }
        let code = report(sig, info, ctx);
        crate::dlls::kernel32::record_exit_code(code);
        // Comme sous Windows, la mort du processus ferme ses jobs (#1966).
        crate::dlls::jobs::on_crash();
        // Ses fichiers « effacés à la fermeture » aussi (#2040).
        crate::dlls::doomed::on_exit();
        libc::_exit(code as i32);
    }
}

/// Écrit le diagnostic d'une faute (journal, écran bleu, stderr) et renvoie
/// le code d'exception Windows. Appelé aussi par le gestionnaire de
/// `quality`, qui s'installe après celui-ci, avant son propre dump.
pub(crate) unsafe fn report(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) -> u32 {
    // Processus figés par celui-ci (Crashpad en plein vidage) : relancés
    // avant toute autre chose, sinon ils restent figés pour toujours (#1787).
    crate::dlls::electron_win32::resume_suspended();
    {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
        let rbp = (*uc).uc_mcontext.gregs[libc::REG_RBP as usize] as usize;
        let rax = (*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as usize;
        let rcx = (*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as usize;
        let rdx = (*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as usize;
        let addr = (*info).si_addr() as usize;
        // Faute d'un programme en espace utilisateur : aucun arrêt du noyau
        // n'a lieu, d'où un seul « stop code » APPLICATION_FAILURE ; le type
        // de faute est dans `what` (#1308).
        let stop_code = APPLICATION_STOP_CODE;
        let (what, code) = match sig {
            // __fastfail : code d'échec dans RCX, noté avec les registres (#2287).
            libc::SIGSEGV if crate::dlls::seh_hw::is_fastfail(uc) => {
                (b"STATUS_STACK_BUFFER_OVERRUN".as_slice(), 0xC000_0409u32)
            }
            libc::SIGSEGV | libc::SIGBUS if is_stack_overflow(addr, rsp) => {
                (b"EXCEPTION_STACK_OVERFLOW".as_slice(), 0xC000_00FDu32)
            }
            libc::SIGSEGV | libc::SIGBUS => {
                (b"EXCEPTION_ACCESS_VIOLATION".as_slice(), 0xC000_0005u32)
            }
            libc::SIGILL => (b"EXCEPTION_ILLEGAL_INSTRUCTION".as_slice(), 0xC000_001D),
            libc::SIGTRAP => (b"EXCEPTION_BREAKPOINT".as_slice(), 0x8000_0003),
            libc::SIGFPE => (b"EXCEPTION_INT_DIVIDE_BY_ZERO".as_slice(), 0xC000_0094),
            _ => (b"UNEXPECTED_SIGNAL".as_slice(), 0xC000_0001),
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
        write_literal(CRASH_HEADLINE.as_bytes());
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
        // Fil fautif (#2405) : tid Linux, celui des lignes de tas et de fils,
        // et heure monotone comparable à WINLUX_THREAD_TRACE.
        write_fault_thread();

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

        // Mode WINLUX_HEAP_DIAG=page (#2158) : bloc de tas qui contient
        // l'adresse fautive ou la valeur d'un registre.
        if crate::dlls::tas_page::enabled() {
            let g = |r: i32| (*uc).uc_mcontext.gregs[r as usize] as usize;
            let values: [(&[u8], usize); 16] = [
                (b"addr", addr),
                (b"rax", g(libc::REG_RAX)),
                (b"rbx", g(libc::REG_RBX)),
                (b"rcx", g(libc::REG_RCX)),
                (b"rdx", g(libc::REG_RDX)),
                (b"rsi", g(libc::REG_RSI)),
                (b"rdi", g(libc::REG_RDI)),
                (b"r8", g(libc::REG_R8)),
                (b"r9", g(libc::REG_R9)),
                (b"r10", g(libc::REG_R10)),
                (b"r11", g(libc::REG_R11)),
                (b"r12", g(libc::REG_R12)),
                (b"r13", g(libc::REG_R13)),
                (b"r14", g(libc::REG_R14)),
                (b"r15", g(libc::REG_R15)),
                (b"rbp", rbp),
            ];
            for (name, value) in values {
                write_heap_page(name, value);
            }
        }

        // Identification du module (try_lock pour rester async-signal-safe)
        if module_len > 0 {
            write_literal(b"winlux: module=");
            write_literal(&module[..module_len]);
            write_literal(b"\n");
        }
        // Mot au sommet de la pile : l'adresse de retour de l'appelant quand
        // la faute suit un appel vers une adresse invalide (rip = addr), même
        // hors de tout module connu (code produit par un JIT) ; puis la pile
        // d'appels, aussi écrite dans le journal (#1308).
        if !is_stack_overflow(addr, rsp) {
            if let Some(top) = safe_read(rsp & !7) {
                write_literal(b"winlux: [rsp]=0x");
                write_hex(top);
                write_place(top);
                write_literal(b"\n");
            }
        }
        for (i, &(frame, sure)) in frames[..frame_count].iter().enumerate().skip(1) {
            write_literal(b"winlux: cadre ");
            write_fd_dec(2, i as u64);
            write_literal(if sure { b" 0x" } else { b" ?0x" });
            write_hex(frame);
            write_place(frame);
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
    // Dossier du compte, préparé par install() (#633) : rien à créer ici.
    let Some(paths) = crate::journal::signal_paths() else {
        return;
    };
    let fd = libc::open(
        paths.crash_log.as_ptr(),
        libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0o600,
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
    let bfd = libc::open(
        paths.crash_bsod.as_ptr(),
        libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0o600,
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

/// Stop code d'une faute en espace utilisateur (#1308).
const APPLICATION_STOP_CODE: &[u8] = b"APPLICATION_FAILURE";

/// Première ligne du diagnostic sur stderr, accentuée (#1307).
const CRASH_HEADLINE: &str = "\nwinlux: le programme a planté : ";

/// « module+décalage » d'une adresse, précédé d'une espace, si connu.
unsafe fn write_place(addr: usize) {
    let mut place = [0u8; 96];
    let n = locate(addr, &mut place);
    if n > 0 {
        write_literal(b" ");
        write_literal(&place[..n]);
    }
}

/// Une ligne `winlux: tas page <nom>=0x… bloc 0x… taille … servie … +décalage
/// alloué n°… fil … [libéré n°… fil …|vivant]` si la valeur tombe dans un bloc
/// du mode page (#2158) ; rien sinon. Sans allocation.
unsafe fn write_heap_page(name: &[u8], value: usize) {
    use crate::dlls::tas_page::{locate, Where};
    match locate(value) {
        Where::None => {}
        Where::Busy => {
            write_literal(b"winlux: tas page ");
            write_literal(name);
            write_literal(b" : registre du tas pris, bloc inconnu\n");
        }
        Where::Block {
            ptr,
            size,
            served,
            alloc_seq,
            alloc_tid,
            free_seq,
            free_tid,
            alloc_frames,
            free_frames,
            offset,
        } => {
            write_literal(b"winlux: tas page ");
            write_literal(name);
            write_literal(b"=0x");
            write_hex(value);
            write_literal(b" bloc 0x");
            write_hex(ptr);
            write_literal(b" taille ");
            write_fd_dec(2, size as u64);
            write_literal(b" servie ");
            write_fd_dec(2, served as u64);
            write_literal(if offset < 0 { b" -" } else { b" +" });
            write_fd_dec(2, offset.unsigned_abs() as u64);
            write_literal(if offset < 0 {
                b" (avant le bloc)".as_slice()
            } else if offset as usize >= size {
                b" (au-dela de la taille)".as_slice()
            } else {
                b" (dans le bloc)".as_slice()
            });
            write_literal(b" alloue n");
            write_fd_dec(2, alloc_seq);
            write_literal(b" fil ");
            write_fd_dec(2, alloc_tid as u64);
            if free_seq == 0 {
                write_literal(b" vivant\n");
            } else {
                write_literal(b" libere n");
                write_fd_dec(2, free_seq);
                write_literal(b" fil ");
                write_fd_dec(2, free_tid as u64);
                write_literal(b"\n");
            }
            write_heap_frames(name, b"allocation", &alloc_frames);
            if free_seq != 0 {
                write_heap_frames(name, b"liberation", &free_frames);
            }
        }
    }
}

/// `winlux: fil <tid> @<secondes>.<microsecondes>` (CLOCK_MONOTONIC).
/// Async-signal-safe : gettid et clock_gettime sont des appels système.
unsafe fn write_fault_thread() {
    let tid = libc::syscall(libc::SYS_gettid) as u64;
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t);
    write_literal(b"winlux: fil ");
    write_fd_dec(2, tid);
    write_literal(b" @");
    write_fd_dec(2, t.tv_sec as u64);
    write_literal(b".");
    let us = (t.tv_nsec / 1000) as u64;
    // Six chiffres, zéros en tête compris.
    let mut d = [b'0'; 6];
    let mut v = us;
    for i in (0..6).rev() {
        d[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    write_literal(&d);
    write_literal(b"\n");
}

/// `winlux: tas page <nom> pile <quoi> : ?0x… module+0x… …` (#2379) :
/// adresses de retour probables relevées sur la pile à l'allocation ou à
/// la libération du bloc.
unsafe fn write_heap_frames(name: &[u8], what: &[u8], frames: &[usize]) {
    write_literal(b"winlux: tas page ");
    write_literal(name);
    write_literal(b" pile ");
    write_literal(what);
    write_literal(b" :");
    for &f in frames.iter().take_while(|&&f| f != 0) {
        write_literal(b" ?0x");
        write_hex(f);
        write_place(f);
    }
    write_literal(b"\n");
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
    let Some(dir) = crate::journal::dir() else {
        return;
    };
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
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("crash.log"))
    {
        let _ = f.write_all(entry.as_bytes());
    }
    // Proposition de signalement : dans le dossier du compte, pour que le
    // bureau d'un autre compte ne la voie pas (#633).
    let _ = std::fs::write(dir.join("crash-offer"), format!("{ts}\n{exe}\n"));
}

/// Limites qui font refuser la création d'un thread (EAGAIN) : threads du
/// processus, mémoire, nombre de zones mappées (une pile mmap refusée rend
/// aussi EAGAIN), RLIMIT_NPROC/AS/STACK, cgroup pids, réglages du
/// noyau (#767). Une ligne, pour le journal.
pub fn thread_limits_report() -> String {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let field = |text: &str, key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|| "?".into())
    };
    let limit = |r: libc::__rlimit_resource_t| {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        let show = |v: libc::rlim_t| {
            if v == libc::RLIM_INFINITY {
                "illimité".to_string()
            } else {
                v.to_string()
            }
        };
        if unsafe { libc::getrlimit(r, &mut rl) } == 0 {
            format!("{}/{}", show(rl.rlim_cur), show(rl.rlim_max))
        } else {
            "?".into()
        }
    };
    let status = read("/proc/self/status");
    let meminfo = read("/proc/meminfo");
    // cgroup v2 : « 0::/chemin ».
    let cgroup = read("/proc/self/cgroup")
        .lines()
        .find_map(|l| l.strip_prefix("0::").map(str::to_string))
        .unwrap_or_default();
    let pids = |f: &str| {
        let v = read(&format!("/sys/fs/cgroup{cgroup}/{f}"));
        if v.is_empty() {
            "?".into()
        } else {
            v.trim().to_string()
        }
    };
    format!(
        "limites des threads : threads {} ; VmSize {} ; VmRSS {} ; \
         zones {} sur max_map_count {} ; RLIMIT_NPROC {} ; RLIMIT_AS {} ; RLIMIT_STACK {} ; threads-max {} ; pid_max {} ; \
         overcommit {} ; MemAvailable {} ; Committed_AS {} ; CommitLimit {} ; \
         cgroup {cgroup} pids {}/{}",
        field(&status, "Threads:"),
        field(&status, "VmSize:"),
        field(&status, "VmRSS:"),
        read("/proc/self/maps").lines().count(),
        read("/proc/sys/vm/max_map_count").trim(),
        limit(libc::RLIMIT_NPROC),
        limit(libc::RLIMIT_AS),
        limit(libc::RLIMIT_STACK),
        read("/proc/sys/kernel/threads-max").trim(),
        read("/proc/sys/kernel/pid_max").trim(),
        read("/proc/sys/vm/overcommit_memory").trim(),
        field(&meminfo, "MemAvailable:"),
        field(&meminfo, "Committed_AS:"),
        field(&meminfo, "CommitLimit:"),
        pids("pids.current"),
        pids("pids.max"),
    )
}

pub fn install() {
    crate::dlls::seh_hw::init();
    crate::journal::prepare_signal_paths();
    unsafe {
        let mut fds = [-1i32; 2];
        if libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) == 0 {
            PROBE = fds;
        }
        // Erreur interne du runtime (panique Rust) : même chemin de signalement.
        std::panic::set_hook(Box::new(|info| {
            let mut detail = format!("{info}");
            // Thread refusé par le noyau (#767) : dire quelle limite et où.
            if detail.contains("failed to spawn thread") {
                detail.push('\n');
                detail.push_str(&thread_limits_report());
                detail.push_str(&format!(
                    "\nappelant : {}",
                    std::backtrace::Backtrace::force_capture()
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Variable qui fait de `fastfail_enfant` le processus enfant (#2287).
    const FASTFAIL_ENFANT: &str = "WINLUX_TEST_FASTFAIL_ENFANT";

    /// Partie enfant : pose le gestionnaire de plantage puis exécute un vrai
    /// `int 0x29` (code d'échec 7, FAST_FAIL_FATAL_APP_EXIT). Ne fait rien
    /// dans l'exécuteur ordinaire.
    #[test]
    fn fastfail_enfant() {
        if std::env::var_os(FASTFAIL_ENFANT).is_none() {
            return;
        }
        install();
        unsafe { std::arch::asm!("int 0x29", in("rcx") 7usize, options(noreturn)) };
    }

    #[test]
    fn fastfail_rapporte_stack_buffer_overrun() {
        // Processus neuf, relancé sur le seul test enfant : jamais de fork
        // dans l'exécuteur multithread.
        let dir = std::env::temp_dir().join(format!("winlux-fastfail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash::tests::fastfail_enfant",
                "--test-threads=1",
            ])
            .env(FASTFAIL_ENFANT, "1")
            .env("WINLUX_JOURNAL_DIR", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break st;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("processus enfant bloqué plus de 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let mut err = String::new();
        use std::io::Read;
        let _ = child.stderr.take().unwrap().read_to_string(&mut err);
        let log = std::fs::read_to_string(dir.join("crash.log")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        // Code de sortie 0xC0000409 : Linux n'en garde que l'octet bas.
        assert_eq!(status.code(), Some(0x09), "{status:?}\n{err}");
        assert!(err.contains("STATUS_STACK_BUFFER_OVERRUN"), "{err}");
        assert!(!err.contains("EXCEPTION_ACCESS_VIOLATION"), "{err}");
        // #2405 : le rapport nomme le fil fautif, avec l'heure monotone.
        let fil = err
            .lines()
            .find(|l| l.starts_with("winlux: fil "))
            .unwrap_or_else(|| panic!("{err}"));
        let (tid, at) = fil["winlux: fil ".len()..].split_once(" @").unwrap();
        assert!(tid.parse::<u64>().unwrap() > 0, "{fil}");
        let (s, us) = at.split_once('.').unwrap();
        assert!(s.parse::<u64>().is_ok() && us.len() == 6, "{fil}");
        assert!(log.is_empty() || log.contains("c0000409"), "{log}");
    }

    /// Variable qui fait de `tas_page_enfant` le processus enfant (#2158) :
    /// `apres` lit un bloc après HeapFree, `depasse` lit au-delà de sa fin.
    const TAS_PAGE_ENFANT: &str = "WINLUX_TEST_TAS_PAGE_ENFANT";

    #[test]
    fn tas_page_enfant() {
        let Some(mode) = std::env::var_os(TAS_PAGE_ENFANT) else {
            return;
        };
        install();
        unsafe {
            let p = crate::dlls::kernel32::heap_alloc(0, 8, 40) as *const u64;
            if mode == "apres" {
                crate::dlls::kernel32::heap_free(0, 0, p as *mut _);
                std::ptr::read_volatile(p.add(1));
            } else {
                // 40 demandés, 48 servis : l'octet 48 est sur la page de garde.
                std::ptr::read_volatile(p.add(6));
            }
        }
        unreachable!("la lecture devait fauter");
    }

    fn tas_page_lancer(mode: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("winlux-taspage-{mode}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash::tests::tas_page_enfant",
                "--test-threads=1",
            ])
            .env(TAS_PAGE_ENFANT, mode)
            .env("WINLUX_HEAP_DIAG", "page")
            .env("WINLUX_JOURNAL_DIR", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("processus enfant bloqué plus de 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut err = String::new();
        use std::io::Read;
        let _ = child.stderr.take().unwrap().read_to_string(&mut err);
        let _ = std::fs::remove_dir_all(&dir);
        err
    }

    /// #2158 : une lecture après HeapFree faute au moment même, et la faute
    /// nomme le bloc libéré ; un dépassement nomme un bloc vivant.
    #[test]
    fn tas_page_nomme_le_bloc_a_la_faute() {
        let err = tas_page_lancer("apres");
        assert!(err.contains("EXCEPTION_ACCESS_VIOLATION"), "{err}");
        let line = err
            .lines()
            .find(|l| l.starts_with("winlux: tas page addr="))
            .unwrap_or_else(|| panic!("{err}"));
        assert!(
            line.contains("taille 40 servie 48 +8 (dans le bloc)"),
            "{line}"
        );
        assert!(line.contains(" libere n"), "{line}");
        // Piles d'allocation et de libération : au moins une adresse de
        // retour dans le code du binaire (ici, le test lui-même).
        for what in ["allocation", "liberation"] {
            let head = format!("winlux: tas page addr pile {what} :");
            let pile = err
                .lines()
                .find(|l| l.starts_with(&head))
                .unwrap_or_else(|| panic!("{err}"));
            assert!(pile.contains(" winlux+0x"), "{pile}");
        }

        let err = tas_page_lancer("depasse");
        let line = err
            .lines()
            .find(|l| l.starts_with("winlux: tas page addr="))
            .unwrap_or_else(|| panic!("{err}"));
        assert!(
            line.contains("taille 40 servie 48 +48 (au-dela de la taille)"),
            "{line}"
        );
        assert!(line.ends_with(" vivant"), "{line}");
    }

    #[test]
    fn plantage_d_application_bien_nomme() {
        assert_eq!(APPLICATION_STOP_CODE, b"APPLICATION_FAILURE");
        assert!(CRASH_HEADLINE.contains("a planté"));
        assert!(!CRASH_HEADLINE.contains("a plante "));
    }
}
