//! Contrôle des threads Windows depuis un autre thread (#269) :
//! `SuspendThread`, `ResumeThread`, `GetThreadContext`, `SetThreadContext`,
//! ainsi que la racine de pile de chaque thread invité.
//!
//! Le ramasse-miettes de coreclr (applis .NET autonomes, WinForms), la JVM
//! et les programmes Go arrêtent leurs threads pour lire ou déplacer leurs
//! registres. Sous Windows c'est le noyau qui le fait ; ici, le thread visé
//! reçoit un signal temps réel. Son gestionnaire publie le `ucontext` que
//! le noyau a sauvé et attend (futex) la reprise : `GetThreadContext` lit ces
//! registres, `SetThreadContext` les modifie, et au retour du signal le
//! noyau recharge le `ucontext` éventuellement modifié.
//!
//! Le thread ne s'arrête de lui-même que dans le code du programme (module
//! PE, code JIT) : arrêté dans du code de Winlux, il pourrait tenir un verrou
//! dont le thread qui suspend a besoin. Il répond alors « plus tard » et le
//! signal est renvoyé jusqu'à ce qu'il soit revenu au code invité. Un thread
//! qui reste dans Winlux est arrêté quand même s'il dort dans le noyau
//! (attente bloquante) ou au bout de 200 ms, sauf s'il tient une garde
//! [`NoPark`] (#2065), et `GetThreadContext` le signale par
//! CONTEXT_SERVICE_ACTIVE comme Windows pour un thread dans un appel système.

use super::seh::{self, Context, CONTEXT_SIZE};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

/// Suspension maximale d'un thread (MAXIMUM_SUSPEND_COUNT).
pub const MAXIMUM_SUSPEND_COUNT: u32 = 0x7F;
/// ERROR_SIGNAL_REFUSED : compte de suspension déjà au maximum.
pub const ERROR_SIGNAL_REFUSED: u32 = 156;

const IDLE: u32 = 0;
const REQUEST: u32 = 1;
const PARKED: u32 = 2;
const DEFERRED: u32 = 3;

/// Après ce délai passé dans le code de Winlux, un thread endormi dans le
/// noyau (attente bloquante) est arrêté dans cet appel système.
///
/// Un thread qui *tourne* dans le code de Winlux ou de la glibc n'est jamais
/// arrêté de force : il peut tenir un verrou d'arène de malloc, et le thread
/// qui suspend bloquerait à sa prochaine allocation sans jamais le reprendre
/// (#2165). Comme Windows pour un thread dans le noyau, l'arrêt attend qu'il
/// revienne au code du programme ou qu'il s'endorme.
const FORCE_ASLEEP_AFTER: Duration = Duration::from_millis(20);

thread_local! {
    /// Profondeur des sections où le thread courant tient un verrou de
    /// Winlux que le thread qui suspend peut demander (#2065).
    static HOLD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Le thread courant ne peut pas être arrêté tant que cette garde vit : il
/// tient un verrou de Winlux (table des réservations VirtualAlloc, journal
/// d'API) que le thread qui suspend demanderait ensuite, par exemple dans
/// `VirtualQuery` après `GetThreadContext`, et les deux resteraient bloqués.
pub struct NoPark(());

pub fn no_park() -> NoPark {
    let _ = HOLD.try_with(|h| h.set(h.get() + 1));
    NoPark(())
}

impl Drop for NoPark {
    fn drop(&mut self) {
        let _ = HOLD.try_with(|h| h.set(h.get().saturating_sub(1)));
    }
}

/// Contexte de signal : le thread courant tient-il une garde [`NoPark`] ?
fn held() -> bool {
    HOLD.try_with(|h| h.get() > 0).unwrap_or(false)
}

/// Le thread `tid` du processus dort-il dans le noyau (états S ou D de
/// `/proc/self/task/<tid>/stat`) ?
fn asleep(tid: i32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")) else {
        return false;
    };
    let Some(end) = stat.rfind(')') else {
        return false;
    };
    matches!(stat[end + 1..].trim_start().chars().next(), Some('S' | 'D'))
}

/// État de suspension d'un thread Windows, lu par son gestionnaire de
/// signal : uniquement des atomiques.
pub struct SuspendSlot {
    /// Compte de suspension d'un thread démarré (sous `op`).
    count: AtomicU32,
    /// Sérialise Suspend/Resume/Get/SetThreadContext sur ce thread. Jamais
    /// pris par le thread visé dans son gestionnaire.
    op: Mutex<()>,
    /// IDLE, REQUEST, PARKED ou DEFERRED (mot de futex).
    state: AtomicU32,
    /// 1 quand le thread peut repartir (mot de futex).
    resume: AtomicU32,
    /// S'arrêter dans le code de Winlux, mais seulement endormi dans un
    /// appel système bloquant.
    force: AtomicBool,
    /// S'arrêter n'importe où (le thread se suspend lui-même).
    anywhere: AtomicBool,
    /// `ucontext_t` publié tant que le thread est arrêté.
    uc: AtomicUsize,
    /// Arrêté hors du code invité.
    in_host: AtomicBool,
    /// Thread terminé ou en train de se terminer.
    gone: AtomicBool,
    /// Pointeur de pile du thread lors de `register` (appelé par le thread
    /// lui-même) : borne haute sûre de sa pile pour le vidage (#2147).
    stack_top: AtomicUsize,
}

impl Default for SuspendSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl SuspendSlot {
    pub const fn new() -> Self {
        SuspendSlot {
            count: AtomicU32::new(0),
            op: Mutex::new(()),
            state: AtomicU32::new(IDLE),
            resume: AtomicU32::new(0),
            force: AtomicBool::new(false),
            anywhere: AtomicBool::new(false),
            uc: AtomicUsize::new(0),
            in_host: AtomicBool::new(false),
            gone: AtomicBool::new(false),
            stack_top: AtomicUsize::new(0),
        }
    }

    fn lock_op(&self) -> std::sync::MutexGuard<'_, ()> {
        self.op
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// ============================================================
//  Table tid -> emplacement, lisible depuis le gestionnaire de signal
// ============================================================

const TABLE: usize = 4096;
struct Entry {
    tid: AtomicI32,
    slot: AtomicUsize,
}
#[allow(clippy::declare_interior_mutable_const)]
const EMPTY: Entry = Entry {
    tid: AtomicI32::new(0),
    slot: AtomicUsize::new(0),
};
static ENTRIES: [Entry; TABLE] = [EMPTY; TABLE];

/// Rend `slot` visible du gestionnaire de signal du thread `tid`. L'appelant
/// garde l'emplacement vivant jusqu'à `unregister`.
pub fn register(tid: i32, slot: &SuspendSlot) {
    install_handler();
    slot.gone.store(false, Ordering::Release);
    // Adresse d'une variable locale : dans la pile de ce thread, au-dessus
    // de tout ce qu'il empilera ensuite (une seule écriture).
    let here = 0u8;
    let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
    slot.stack_top.store(
        if me == tid {
            std::ptr::addr_of!(here) as usize
        } else {
            0
        },
        Ordering::Release,
    );
    let ptr = slot as *const SuspendSlot as usize;
    for e in ENTRIES.iter() {
        if e.tid.load(Ordering::Acquire) == tid {
            e.slot.store(ptr, Ordering::Release);
            return;
        }
    }
    for e in ENTRIES.iter() {
        if e.tid
            .compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            e.slot.store(ptr, Ordering::Release);
            e.tid.store(tid, Ordering::Release);
            return;
        }
    }
    // Plus de 4096 threads vivants : ce thread ne pourra pas être suspendu.
}

/// Le thread `tid` se termine : plus de suspension possible.
pub fn unregister(tid: i32, slot: &SuspendSlot) {
    slot.gone.store(true, Ordering::Release);
    futex_wake(&slot.state);
    let ptr = slot as *const SuspendSlot as usize;
    for e in ENTRIES.iter() {
        if e.tid.load(Ordering::Acquire) == tid && e.slot.load(Ordering::Acquire) == ptr {
            e.slot.store(0, Ordering::Release);
            e.tid.store(0, Ordering::Release);
        }
    }
}

fn find_slot(tid: i32) -> Option<&'static SuspendSlot> {
    ENTRIES.iter().find_map(|e| {
        if e.tid.load(Ordering::Acquire) != tid {
            return None;
        }
        let p = e.slot.load(Ordering::Acquire);
        (p != 0).then(|| unsafe { &*(p as *const SuspendSlot) })
    })
}

// ============================================================
//  Futex
// ============================================================

fn futex_wait(word: &AtomicU32, expected: u32, timeout: Option<Duration>) {
    let ts = timeout.map(|d| libc::timespec {
        tv_sec: d.as_secs() as libc::time_t,
        tv_nsec: d.subsec_nanos() as libc::c_long,
    });
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            word as *const AtomicU32,
            libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
            expected,
            ts.as_ref()
                .map_or(std::ptr::null(), |t| t as *const libc::timespec),
        );
    }
}

fn futex_wake(word: &AtomicU32) {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            word as *const AtomicU32,
            libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
            i32::MAX,
        );
    }
}

// ============================================================
//  Gestionnaire de signal
// ============================================================

fn suspend_signal() -> i32 {
    libc::SIGRTMIN() + 6
}

fn install_handler() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_suspend as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(suspend_signal(), &sa, std::ptr::null_mut());
    });
}

unsafe extern "C" fn on_suspend(_sig: i32, _info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    let errno = *libc::__errno_location();
    park_here(ctx as *mut libc::ucontext_t);
    *libc::__errno_location() = errno;
}

/// Contexte de signal : arrêt du thread courant si une suspension l'attend.
unsafe fn park_here(uc: *mut libc::ucontext_t) {
    if uc.is_null() {
        return;
    }
    let tid = libc::syscall(libc::SYS_gettid) as i32;
    let Some(slot) = find_slot(tid) else {
        return;
    };
    if slot.state.load(Ordering::Acquire) != REQUEST {
        return;
    }
    let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
    let guest = super::seh_hw::is_guest_code_pub(rip);
    // Jamais arrêté en tenant un verrou de Winlux ; hors du code invité,
    // seulement quand `park` le force et que le thread est dans un appel
    // système bloquant (jamais au milieu de malloc, #2144).
    let forced = slot.anywhere.load(Ordering::Acquire)
        || (slot.force.load(Ordering::Acquire) && in_blocking_syscall(uc));
    if held() || (!guest && !forced) {
        if slot
            .state
            .compare_exchange(REQUEST, DEFERRED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            futex_wake(&slot.state);
        }
        return;
    }
    slot.uc.store(uc as usize, Ordering::Release);
    slot.in_host.store(!guest, Ordering::Release);
    if slot
        .state
        .compare_exchange(REQUEST, PARKED, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        slot.uc.store(0, Ordering::Release);
        return;
    }
    futex_wake(&slot.state);
    while slot.resume.load(Ordering::Acquire) == 0 {
        futex_wait(&slot.resume, 0, None);
    }
    slot.uc.store(0, Ordering::Release);
    slot.state.store(IDLE, Ordering::Release);
    futex_wake(&slot.state);
}

/// Appels système où un thread attend sans tenir de verrou de la glibc :
/// futex, sommeils, attentes d'E/S.
const BLOCKING_SYSCALLS: &[i64] = &[
    libc::SYS_futex,
    libc::SYS_restart_syscall,
    libc::SYS_nanosleep,
    libc::SYS_clock_nanosleep,
    libc::SYS_read,
    libc::SYS_poll,
    libc::SYS_ppoll,
    libc::SYS_select,
    libc::SYS_pselect6,
    libc::SYS_epoll_wait,
    libc::SYS_epoll_pwait,
    libc::SYS_recvfrom,
    libc::SYS_recvmsg,
    libc::SYS_wait4,
    libc::SYS_waitid,
    libc::SYS_pause,
    libc::SYS_rt_sigtimedwait,
    libc::SYS_accept,
    libc::SYS_accept4,
];

/// Le signal a interrompu un appel système bloquant : soit le noyau va le
/// relancer (rip sur `syscall`, rax = numéro d'un appel bloquant), soit il
/// rend EINTR (rip juste après `syscall`). Un thread arrêté au milieu de
/// malloc n'est dans aucun de ces cas (#2165).
unsafe fn in_blocking_syscall(uc: *const libc::ucontext_t) -> bool {
    let g = &(*uc).uc_mcontext.gregs;
    let rip = g[libc::REG_RIP as usize] as usize;
    let rax = g[libc::REG_RAX as usize];
    let is_syscall = |a: usize| -> bool {
        // Le code autour de rip vient d'être exécuté : lisible.
        let mut b = [0u8; 2];
        read_own_memory(a, &mut b) == 2 && b == [0x0f, 0x05]
    };
    if is_syscall(rip) {
        return BLOCKING_SYSCALLS.contains(&rax);
    }
    rax == -(libc::EINTR as i64) && rip >= 2 && is_syscall(rip - 2)
}

// ============================================================
//  Arrêt et reprise
// ============================================================

/// Arrête le thread `tid` (autre que le thread courant) et attend qu'il
/// soit arrêté. Faux s'il s'est terminé entre-temps. Sous `op`.
fn park(slot: &SuspendSlot, tid: i32) -> bool {
    park_until(slot, tid, None)
}

/// Comme `park`, mais renonce à `deadline` (faux, thread laissé libre).
fn park_until(slot: &SuspendSlot, tid: i32, deadline: Option<Instant>) -> bool {
    install_handler();
    slot.resume.store(0, Ordering::Release);
    slot.force.store(false, Ordering::Release);
    slot.anywhere.store(false, Ordering::Release);
    slot.state.store(REQUEST, Ordering::Release);
    let start = Instant::now();
    let mut delay = Duration::from_micros(20);
    let pid = std::process::id() as i32;
    loop {
        if slot.gone.load(Ordering::Acquire) {
            slot.state.store(IDLE, Ordering::Release);
            return false;
        }
        if unsafe { libc::syscall(libc::SYS_tgkill, pid, tid, suspend_signal()) } != 0 {
            slot.state.store(IDLE, Ordering::Release);
            return false;
        }
        // Réponse du gestionnaire (arrêté ou « plus tard »).
        let waited = Instant::now();
        loop {
            let s = slot.state.load(Ordering::Acquire);
            if s == PARKED {
                return true;
            }
            if s != REQUEST || slot.gone.load(Ordering::Acquire) {
                break;
            }
            if waited.elapsed() > Duration::from_millis(100) {
                break;
            }
            futex_wait(&slot.state, REQUEST, Some(Duration::from_millis(5)));
        }
        if slot.gone.load(Ordering::Acquire) {
            slot.state.store(IDLE, Ordering::Release);
            return false;
        }
        if deadline.is_some_and(|d| Instant::now() > d) {
            // Abandon : seulement depuis REQUEST ou DEFERRED ; s'il vient
            // de s'arrêter, il est à nous.
            for from in [REQUEST, DEFERRED] {
                if slot
                    .state
                    .compare_exchange(from, IDLE, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return false;
                }
            }
            if slot.state.load(Ordering::Acquire) == PARKED {
                return true;
            }
        }
        if slot.state.load(Ordering::Acquire) == DEFERRED {
            std::thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(1));
            let elapsed = start.elapsed();
            if elapsed > FORCE_ASLEEP_AFTER && asleep(tid) {
                slot.force.store(true, Ordering::Release);
            }
            let _ =
                slot.state
                    .compare_exchange(DEFERRED, REQUEST, Ordering::AcqRel, Ordering::Acquire);
        }
        // Sinon (pas de réponse) : signal renvoyé tel quel.
    }
}

/// Laisse repartir un thread arrêté par `park` et attend qu'il ait quitté
/// son gestionnaire. Sous `op`.
fn unpark(slot: &SuspendSlot) {
    slot.resume.store(1, Ordering::Release);
    futex_wake(&slot.resume);
    while slot.state.load(Ordering::Acquire) == PARKED && !slot.gone.load(Ordering::Acquire) {
        futex_wait(&slot.state, PARKED, Some(Duration::from_millis(5)));
    }
}

/// `SuspendThread` sur un thread démarré : ancien compte, ou code d'erreur.
pub fn suspend(slot: &SuspendSlot, tid: i32) -> Result<u32, u32> {
    let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
    let op = slot.lock_op();
    let prev = slot.count.load(Ordering::Acquire);
    if prev >= MAXIMUM_SUSPEND_COUNT {
        return Err(ERROR_SIGNAL_REFUSED);
    }
    slot.count.store(prev + 1, Ordering::Release);
    if prev != 0 {
        return Ok(prev);
    }
    if tid == me {
        // Le thread se suspend lui-même : arrêt immédiat, où qu'il soit,
        // jusqu'à ce qu'un autre thread le reprenne.
        install_handler();
        slot.resume.store(0, Ordering::Release);
        slot.force.store(true, Ordering::Release);
        slot.anywhere.store(true, Ordering::Release);
        slot.state.store(REQUEST, Ordering::Release);
        drop(op);
        unsafe {
            libc::syscall(
                libc::SYS_tgkill,
                std::process::id() as i32,
                me,
                suspend_signal(),
            );
        }
        return Ok(prev);
    }
    if !park(slot, tid) {
        // Thread terminé : Windows rend quand même l'ancien compte.
        slot.count.store(prev, Ordering::Release);
    }
    Ok(prev)
}

/// `ResumeThread` sur un thread démarré : ancien compte.
pub fn resume(slot: &SuspendSlot) -> u32 {
    let _op = slot.lock_op();
    let prev = slot.count.load(Ordering::Acquire);
    if prev == 0 {
        return 0;
    }
    slot.count.store(prev - 1, Ordering::Release);
    if prev == 1 {
        unpark(slot);
    }
    prev
}

// ============================================================
//  CONTEXT
// ============================================================

const CONTEXT_AMD64: u32 = 0x0010_0000;
const CONTEXT_CONTROL: u32 = 0x1;
const CONTEXT_INTEGER: u32 = 0x2;
const CONTEXT_SEGMENTS: u32 = 0x4;
const CONTEXT_FLOATING_POINT: u32 = 0x8;
const CONTEXT_DEBUG_REGISTERS: u32 = 0x10;
const CONTEXT_EXCEPTION_ACTIVE: u32 = 0x0800_0000;
const CONTEXT_SERVICE_ACTIVE: u32 = 0x1000_0000;
const CONTEXT_EXCEPTION_REQUEST: u32 = 0x4000_0000;
const CONTEXT_EXCEPTION_REPORTING: u32 = 0x8000_0000;

/// Copie dans `dst` les parties de `src` demandées par `flags`, puis écrit
/// les drapeaux rendus (ContextFlags).
unsafe fn copy_parts(src: &Context, dst: *mut u8, flags: u32, in_host: bool) {
    let s = src.0.as_ptr();
    let copy =
        |off: usize, len: usize| std::ptr::copy_nonoverlapping(s.add(off), dst.add(off), len);
    if flags & CONTEXT_CONTROL != 0 {
        copy(0x38, 2); // SegCs
        copy(0x42, 2); // SegSs
        copy(0x44, 4); // EFlags
        copy(0x98, 8); // Rsp
        copy(0xF8, 8); // Rip
    }
    if flags & CONTEXT_INTEGER != 0 {
        copy(0x78, 0x20); // Rax, Rcx, Rdx, Rbx
        copy(0xA0, 0x58); // Rbp … R15
    }
    if flags & CONTEXT_SEGMENTS != 0 {
        copy(0x3A, 8); // SegDs, SegEs, SegFs, SegGs
    }
    if flags & CONTEXT_FLOATING_POINT != 0 {
        copy(0x34, 4); // MxCsr
        copy(0x100, 0x200); // FltSave (dont Xmm0-15)
    }
    if flags & CONTEXT_DEBUG_REGISTERS != 0 {
        std::ptr::write_bytes(dst.add(0x48), 0, 0x30); // Dr0-Dr7
    }
    let mut out = flags
        & (CONTEXT_AMD64
            | CONTEXT_CONTROL
            | CONTEXT_INTEGER
            | CONTEXT_SEGMENTS
            | CONTEXT_FLOATING_POINT
            | CONTEXT_DEBUG_REGISTERS)
        | CONTEXT_AMD64;
    if flags & CONTEXT_EXCEPTION_REQUEST != 0 {
        // Arrêté dans Winlux = dans un « appel système » : coreclr ne
        // redirige pas un tel thread.
        out |= CONTEXT_EXCEPTION_REPORTING;
        if in_host {
            out |= CONTEXT_SERVICE_ACTIVE;
        }
    }
    let _ = CONTEXT_EXCEPTION_ACTIVE;
    (dst.add(0x30) as *mut u32).write_unaligned(out);
}

/// `GetThreadContext` sur le thread courant.
pub unsafe fn get_own_context(dst: *mut u8) {
    let flags = (dst.add(0x30) as *const u32).read_unaligned();
    let mut ctx = Context::zeroed();
    seh::rtl_capture_context(ctx.ptr().cast());
    copy_parts(&ctx, dst, flags, false);
}

/// Exécute `f` sur le thread arrêté (le suspend un instant s'il tourne).
/// Faux si le thread s'est terminé.
unsafe fn with_parked(
    slot: &SuspendSlot,
    tid: i32,
    f: impl FnOnce(*mut libc::ucontext_t, bool),
) -> bool {
    let _op = slot.lock_op();
    let temporary = slot.count.load(Ordering::Acquire) == 0;
    if temporary && !park(slot, tid) {
        return false;
    }
    let uc = slot.uc.load(Ordering::Acquire) as *mut libc::ucontext_t;
    let ok = !uc.is_null() && slot.state.load(Ordering::Acquire) == PARKED;
    if ok {
        f(uc, slot.in_host.load(Ordering::Acquire));
    }
    if temporary {
        unpark(slot);
    }
    ok
}

/// `GetThreadContext` sur un autre thread démarré.
pub unsafe fn get_context(slot: &SuspendSlot, tid: i32, dst: *mut u8) -> bool {
    let flags = (dst.add(0x30) as *const u32).read_unaligned();
    with_parked(slot, tid, |uc, in_host| {
        let mut ctx = Context::zeroed();
        super::seh_hw::context_from_ucontext(uc, &mut ctx);
        copy_parts(&ctx, dst, flags, in_host);
    })
}

/// `SetThreadContext` sur un autre thread démarré.
pub unsafe fn set_context(slot: &SuspendSlot, tid: i32, src: *const u8) -> bool {
    let mut ctx = Context::zeroed();
    std::ptr::copy_nonoverlapping(src, ctx.ptr(), CONTEXT_SIZE);
    with_parked(slot, tid, |uc, _| {
        super::seh_hw::context_to_ucontext(&ctx, uc);
    })
}

// ============================================================
//  Racine de pile des threads invités
// ============================================================

/// Appelle `start(param)` depuis une petite racine dont le déroulement
/// Windows s'arrête net (adresse de retour nulle), comme
/// RtlUserThreadStart sous Windows. Sans elle, coreclr remonte la pile
/// d'un thread jusque dans le code de Winlux, qu'il prend pour une fonction
/// feuille, et reprend sur une adresse de retour fantaisiste.
pub fn root_call(start: usize, param: usize) -> u32 {
    let stub = root_stub();
    if stub == 0 {
        let f: extern "win64" fn(usize) -> u32 = unsafe { std::mem::transmute(start) };
        return f(param);
    }
    let f: unsafe extern "win64" fn(usize, usize) -> u32 = unsafe { std::mem::transmute(stub) };
    unsafe { f(start, param) }
}

fn root_stub() -> usize {
    static STUB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *STUB.get_or_init(|| {
        #[rustfmt::skip]
        let code: [u8; 21] = [
            0x6A, 0x00,             // push 0        ; adresse de retour nulle : fin de pile
            0x48, 0x83, 0xEC, 0x20, // sub rsp, 0x20 ; espace de rangement
            0x48, 0x89, 0xC8,       // mov rax, rcx  ; start
            0x48, 0x89, 0xD1,       // mov rcx, rdx  ; param
            0xFF, 0xD0,             // call rax
            0x89, 0xC0,             // mov eax, eax  ; pas un épilogue au retour
            0x48, 0x83, 0xC4, 0x28, // add rsp, 0x28
            0xC3,                   // ret
        ];
        // UNWIND_INFO : version 1, prologue de 6 octets, un code :
        // UWOP_ALLOC_SMALL de 0x20 à la fin du prologue. Le `push 0` n'est
        // pas décrit : le déroulement lit donc 0 comme adresse de retour.
        let unwind = [0x01, 0x06, 0x01, 0x00, 0x06, 0x32, 0x00, 0x00];
        seh::code_stub(&code, &unwind)
    })
}

// ============================================================
//  Vidage des piles (WINLUX_THREAD_DUMP, #2147)
// ============================================================

/// Demande de vidage posée par SIGUSR2 (mot de futex).
static DUMP_REQUEST: AtomicU32 = AtomicU32::new(0);

/// WINLUX_THREAD_DUMP=<dossier> (#2147) : sur SIGUSR2, l'état de chaque
/// thread Windows du processus est écrit dans `<dossier>/pile-<pid>-<n>.txt`
/// (rip, appel système en cours, adresses de retour vues sur la pile dans
/// les modules PE). Diagnostic des blocages sans gdb ni ptrace. Sans la
/// variable, rien n'est installé : ni gestionnaire de SIGUSR2, ni thread,
/// ni adoption du thread principal ; la variable n'est lue qu'ici, une fois.
pub fn install_dump() {
    install_dump_from(std::env::var_os("WINLUX_THREAD_DUMP"));
}

fn install_dump_from(value: Option<std::ffi::OsString>) -> bool {
    let Some(dir) = value
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
    else {
        return false;
    };
    // Le thread principal devient suspendable comme les autres.
    let _ = super::kernel32::current_thread_object();
    install_handler();
    let spawned = std::thread::Builder::new()
        .name("winlux-piles".into())
        .spawn(move || {
            let mut n = 0u32;
            loop {
                while DUMP_REQUEST.swap(0, Ordering::AcqRel) == 0 {
                    futex_wait(&DUMP_REQUEST, 0, None);
                }
                n += 1;
                let threads = super::kernel32::thread_slots();
                let refs: Vec<(i32, &SuspendSlot)> =
                    threads.iter().map(|(t, o)| (*t, &o.slot)).collect();
                let text = dump_threads(&refs);
                let path = dir.join(format!("pile-{}-{n}.txt", std::process::id()));
                let written = write_private(&path, &text);
                crate::log::emit(
                    crate::log::Level::Error,
                    "runtime",
                    &format_args!(
                        "WINLUX_THREAD_DUMP : {} threads écrits dans {} ({})",
                        refs.len(),
                        path.display(),
                        if written { "ok" } else { "échec" }
                    ),
                );
            }
        });
    if spawned.is_err() {
        return false;
    }
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_dump_request as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGUSR2, &sa, std::ptr::null_mut());
    }
    true
}

extern "C" fn on_dump_request(_sig: i32) {
    let errno = unsafe { *libc::__errno_location() };
    DUMP_REQUEST.store(1, Ordering::Release);
    futex_wake(&DUMP_REQUEST);
    unsafe { *libc::__errno_location() = errno };
}

fn write_private(path: &std::path::Path, text: &str) -> bool {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .is_ok()
}

/// Copie `dst.len()` octets depuis `addr` de ce processus sans jamais
/// fauter : process_vm_readv page par page, arrêt à la première page
/// illisible. Rend le nombre d'octets copiés (#2147). Aucune allocation.
fn read_own_memory(addr: usize, dst: &mut [u8]) -> usize {
    const PAGE: usize = 4096;
    let pid = unsafe { libc::getpid() };
    let mut done = 0;
    while done < dst.len() {
        let at = addr + done;
        // Jusqu'à la fin de la page courante.
        let chunk = (PAGE - at % PAGE).min(dst.len() - done);
        let local = libc::iovec {
            iov_base: dst[done..].as_mut_ptr().cast(),
            iov_len: chunk,
        };
        let remote = libc::iovec {
            iov_base: at as *mut libc::c_void,
            iov_len: chunk,
        };
        let n = unsafe { libc::process_vm_readv(pid, &local, 1, &remote, 1, 0) };
        if n <= 0 {
            break;
        }
        done += n as usize;
        if (n as usize) < chunk {
            break;
        }
    }
    done
}

/// Zone lisible de /proc/self/maps : (début, fin, chemin).
fn readable_ranges(maps: &str) -> Vec<(usize, usize, String)> {
    maps.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let range = it.next()?;
            let perms = it.next()?;
            if !perms.starts_with('r') {
                return None;
            }
            let path = it.nth(3).unwrap_or("").to_string();
            let (a, b) = range.split_once('-')?;
            Some((
                usize::from_str_radix(a, 16).ok()?,
                usize::from_str_radix(b, 16).ok()?,
                path,
            ))
        })
        .collect()
}

/// Adresse lisible : module PE + décalage, ou fichier projeté + décalage
/// (code de Winlux), ou valeur brute.
fn describe(addr: usize, ranges: &[(usize, usize, String)]) -> String {
    if let Some(m) = crate::loader::find_module_by_addr(addr) {
        return format!("{}+0x{:x}", m.name, addr - m.base);
    }
    match ranges.iter().find(|(a, b, _)| addr >= *a && addr < *b) {
        Some((a, _, p)) if !p.is_empty() => {
            // Base du fichier : première zone projetée de ce chemin.
            let base = ranges
                .iter()
                .filter(|(_, _, q)| q == p)
                .map(|(x, _, _)| *x)
                .min()
                .unwrap_or(*a);
            format!("{p}+0x{:x}", addr - base)
        }
        _ => format!("0x{addr:x}"),
    }
}

/// Texte du vidage de `threads` (tid, emplacement), threads arrêtés un par
/// un, le temps de copier leurs registres et le haut de leur pile. Rien
/// n'est alloué pendant qu'un thread est arrêté (il peut tenir le verrou de
/// malloc) : tampon et carte mémoire sont préparés avant.
fn dump_threads(threads: &[(i32, &SuspendSlot)]) -> String {
    use std::fmt::Write;
    const STACK: usize = 128 * 1024;
    let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap_or_default();
    let ranges = readable_ranges(&maps);
    let exec: Vec<(usize, usize)> = maps
        .lines()
        .filter(|l| l.split_whitespace().nth(1).is_some_and(|p| p.contains('x')))
        .filter_map(|l| {
            let (a, b) = l.split_whitespace().next()?.split_once('-')?;
            Some((
                usize::from_str_radix(a, 16).ok()?,
                usize::from_str_radix(b, 16).ok()?,
            ))
        })
        .collect();
    let x86 = crate::wow32::active();
    let own_exe = std::fs::read_link("/proc/self/exe")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut buf = vec![0u8; STACK];
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Winlux (commit {}) : vidage des piles du processus {} ({} threads)\n\
         Trames « winlux+0x… » : addr2line -f -C -e {own_exe} <décalage>",
        env!("WINLUX_BUILD_COMMIT"),
        std::process::id(),
        threads.len()
    );
    for &(tid, slot) in threads {
        if tid == me || tid <= 0 {
            continue;
        }
        let comm =
            std::fs::read_to_string(format!("/proc/self/task/{tid}/comm")).unwrap_or_default();
        let syscall =
            std::fs::read_to_string(format!("/proc/self/task/{tid}/syscall")).unwrap_or_default();
        let state = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat"))
            .ok()
            .and_then(|s| {
                let end = s.rfind(')')?;
                s[end + 1..].trim_start().chars().next()
            })
            .unwrap_or('?');
        let _ = writeln!(
            out,
            "\n== thread {tid} « {} », état {state}, appel système : {}",
            comm.trim(),
            syscall.trim()
        );
        // Arrêt, copie, reprise.
        let mut regs = (0usize, 0usize, false, 0usize);
        let parked = {
            let _op = slot.lock_op();
            let temporary = slot.count.load(Ordering::Acquire) == 0;
            let ok = if temporary {
                park_until(slot, tid, Some(Instant::now() + Duration::from_secs(2)))
            } else {
                slot.state.load(Ordering::Acquire) == PARKED
            };
            if ok {
                let uc = slot.uc.load(Ordering::Acquire) as *const libc::ucontext_t;
                if !uc.is_null() {
                    let rip = unsafe { (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] } as usize;
                    let rsp = unsafe { (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] } as usize;
                    // Borne : la pile connue du thread (rsp sous son sommet
                    // relevé à `register`), sinon 16 Kio (pile de fibre…).
                    // Lecture par process_vm_readv : une page illisible
                    // (garde MADV_GUARD_INSTALL, absente de maps) arrête la
                    // copie au lieu de faire planter le processus.
                    let top = slot.stack_top.load(Ordering::Acquire);
                    let want = if top > rsp && top - rsp <= 64 << 20 {
                        (top - rsp).min(STACK)
                    } else {
                        STACK.min(16 * 1024)
                    };
                    let len = read_own_memory(rsp, &mut buf[..want]);
                    regs = (rip, rsp, slot.in_host.load(Ordering::Acquire), len);
                }
                if temporary {
                    unpark(slot);
                }
            }
            ok
        };
        if !parked {
            let _ = writeln!(out, "   non arrêté en 2 s (tient un verrou de Winlux ?)");
            continue;
        }
        let (rip, rsp, in_host, len) = regs;
        let _ = writeln!(
            out,
            "   rip {} ({}), rsp 0x{rsp:x}, {len} octets de pile lus",
            describe(rip, &ranges),
            if in_host {
                "code de Winlux"
            } else {
                "code du programme"
            }
        );
        let step = if x86 { 4 } else { 8 };
        let mut last = String::new();
        let mut shown = 0;
        let mut host_shown = 0;
        let mut off = 0;
        while off + step <= len && shown < 48 {
            let v = if x86 {
                u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize
            } else {
                u64::from_le_bytes(buf[off..off + 8].try_into().unwrap()) as usize
            };
            // Adresse de retour plausible : dans une page exécutable d'un
            // module PE (pas ses données).
            let code = exec.iter().any(|&(a, b)| v >= a && v < b);
            let module = code
                .then(|| crate::loader::find_module_by_addr(v))
                .flatten();
            if module.is_none() && code && host_shown < 16 {
                // Trame de Winlux lui-même : à symboliser avec addr2line.
                if ranges
                    .iter()
                    .any(|(a, b, p)| v >= *a && v < *b && *p == own_exe)
                {
                    let d = describe(v, &ranges).replacen(own_exe.as_str(), "winlux", 1);
                    if d != last {
                        let _ = writeln!(out, "   [rsp+0x{off:x}] {d}");
                        last = d;
                        host_shown += 1;
                    }
                }
            }
            if let Some(m) = module {
                let d = format!("{}+0x{:x}", m.name, v - m.base);
                if d != last {
                    let _ = writeln!(out, "   [rsp+0x{off:x}] {d}");
                    last = d;
                    shown += 1;
                }
            }
            off += step;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn racine_rend_la_valeur_et_le_parametre() {
        extern "win64" fn double(x: usize) -> u32 {
            (x * 2) as u32
        }
        assert_eq!(root_call(double as usize, 21), 42);
    }

    #[test]
    fn suspendre_lire_et_modifier_un_thread_qui_tourne() {
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        static STOP: AtomicBool = AtomicBool::new(false);
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let (s2, t2) = (slot.clone(), tid.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            t2.store(me, Ordering::Release);
            // Calcul entrecoupé de courts sommeils : seuls points où un
            // thread de Winlux peut être arrêté (#2165).
            while !STOP.load(Ordering::Relaxed) {
                if COUNTER.fetch_add(1, Ordering::Relaxed) % 4096 == 0 {
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let tid = tid.load(Ordering::Acquire);
        // Code de Winlux (Rust) : arrêt forcé après le délai.
        assert_eq!(suspend(&slot, tid), Ok(0));
        let a = COUNTER.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            COUNTER.load(Ordering::Relaxed),
            a,
            "le thread tourne encore"
        );
        assert_eq!(suspend(&slot, tid), Ok(1));
        let mut ctx = Context::zeroed();
        unsafe {
            (ctx.ptr().add(0x30) as *mut u32)
                .write_unaligned(CONTEXT_AMD64 | CONTEXT_CONTROL | CONTEXT_EXCEPTION_REQUEST)
        };
        assert!(unsafe { get_context(&slot, tid, ctx.ptr()) });
        assert_ne!(ctx.get(seh::CTX_RIP), 0);
        let flags = unsafe { (ctx.ptr().add(0x30) as *const u32).read_unaligned() };
        assert_ne!(flags & CONTEXT_EXCEPTION_REPORTING, 0);
        assert_ne!(flags & CONTEXT_SERVICE_ACTIVE, 0, "arrêté dans Winlux");
        assert_eq!(resume(&slot), 2);
        assert_eq!(resume(&slot), 1);
        std::thread::sleep(Duration::from_millis(5));
        assert_ne!(
            COUNTER.load(Ordering::Relaxed),
            a,
            "le thread n'est pas reparti"
        );
        assert_eq!(resume(&slot), 0);
        STOP.store(true, Ordering::Relaxed);
        h.join().unwrap();
        // Thread terminé : plus rien à arrêter.
        assert!(!unsafe { get_context(&slot, tid, ctx.ptr()) });
    }

    /// Thread qui tourne en tenant une garde NoPark : jamais arrêté tant
    /// qu'il la tient (#2065).
    #[test]
    fn pas_d_arret_sous_garde_no_park() {
        use std::sync::Arc;
        static RELEASE: AtomicBool = AtomicBool::new(false);
        static RELEASED: AtomicBool = AtomicBool::new(false);
        static DONE: AtomicBool = AtomicBool::new(false);
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let (s2, t2) = (slot.clone(), tid.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            let hold = no_park();
            t2.store(me, Ordering::Release);
            while !RELEASE.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            RELEASED.store(true, Ordering::Release);
            drop(hold);
            // Garde rendue : arrêtable à son prochain sommeil (#2165).
            while !DONE.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_micros(50));
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let tid = tid.load(Ordering::Acquire);
        let releaser = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            RELEASE.store(true, Ordering::Release);
        });
        assert_eq!(suspend(&slot, tid), Ok(0));
        assert!(
            RELEASED.load(Ordering::Acquire),
            "arrêté en tenant la garde"
        );
        assert_eq!(resume(&slot), 1);
        DONE.store(true, Ordering::Release);
        releaser.join().unwrap();
        h.join().unwrap();
    }

    /// Thread endormi dans le noyau depuis Winlux : arrêté vite (#2065).
    #[test]
    fn arret_rapide_d_un_thread_endormi() {
        use std::sync::Arc;
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, t2, st2) = (slot.clone(), tid.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            t2.store(me, Ordering::Release);
            while !st2.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(50));
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let tid = tid.load(Ordering::Acquire);
        std::thread::sleep(Duration::from_millis(5));
        let t0 = Instant::now();
        assert_eq!(suspend(&slot, tid), Ok(0));
        let took = t0.elapsed();
        assert_eq!(resume(&slot), 1);
        stop.store(true, Ordering::Release);
        h.join().unwrap();
        assert!(took < Duration::from_millis(200), "arrêt en {took:?}");
    }

    /// Sans WINLUX_THREAD_DUMP : rien n'est installé, SIGUSR2 garde son
    /// action par défaut (#2147).
    #[test]
    fn vidage_absent_sans_la_variable() {
        assert!(!install_dump_from(None));
        assert!(!install_dump_from(Some(std::ffi::OsString::new())));
        let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
        unsafe { libc::sigaction(libc::SIGUSR2, std::ptr::null(), &mut old) };
        assert_eq!(old.sa_sigaction, libc::SIG_DFL);
        assert_eq!(DUMP_REQUEST.load(Ordering::Acquire), 0);
    }

    /// Le vidage arrête un thread qui tourne, lit son rip et sa pile, et
    /// le laisse repartir (#2147).
    #[test]
    fn vidage_d_un_thread_qui_tourne() {
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let stop = Arc::new(AtomicBool::new(false));
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let (s2, t2, st2) = (slot.clone(), tid.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            t2.store(me, Ordering::Release);
            while !st2.load(Ordering::Relaxed) {
                if COUNTER.fetch_add(1, Ordering::Relaxed) % 4096 == 0 {
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let t = tid.load(Ordering::Acquire);
        let text = dump_threads(&[(t, &*slot)]);
        assert!(text.contains(&format!("== thread {t}")), "{text}");
        assert!(text.contains("rip "), "{text}");
        assert!(text.contains("octets de pile lus"), "{text}");
        let a = COUNTER.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(20));
        assert_ne!(COUNTER.load(Ordering::Relaxed), a, "thread non repris");
        assert_eq!(slot.state.load(Ordering::Acquire), IDLE);
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
    }

    /// Thread sous garde NoPark : abandon à l'échéance, thread laissé libre.
    #[test]
    fn arret_borne_abandonne_sous_garde() {
        use std::sync::Arc;
        let stop = Arc::new(AtomicBool::new(false));
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let (s2, t2, st2) = (slot.clone(), tid.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            let hold = no_park();
            t2.store(me, Ordering::Release);
            while !st2.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
            drop(hold);
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let t = tid.load(Ordering::Acquire);
        let _op = slot.lock_op();
        assert!(!park_until(
            &slot,
            t,
            Some(Instant::now() + Duration::from_millis(300))
        ));
        assert_eq!(slot.state.load(Ordering::Acquire), IDLE);
        drop(_op);
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
    }

    /// Lecture qui franchit une page illisible : copie partielle, pas de
    /// faute (#2147, garde de pile invisible dans maps).
    #[test]
    fn lecture_bornee_par_une_page_illisible() {
        unsafe {
            let p = libc::mmap(
                std::ptr::null_mut(),
                8192,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            assert_ne!(p, libc::MAP_FAILED);
            std::ptr::write_bytes(p as *mut u8, 0xAB, 4096);
            assert_eq!(
                libc::mprotect((p as usize + 4096) as *mut _, 4096, libc::PROT_NONE),
                0
            );
            let mut buf = vec![0u8; 8192];
            let n = read_own_memory(p as usize + 100, &mut buf);
            assert_eq!(n, 4096 - 100);
            assert!(buf[..n].iter().all(|&b| b == 0xAB));
            libc::munmap(p, 8192);
            // Garde posée par MADV_GUARD_INSTALL (102, noyau ≥ 6.13) : la
            // plage reste « rw-p » dans maps mais la page faute.
            let q = libc::mmap(
                std::ptr::null_mut(),
                8192,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            assert_ne!(q, libc::MAP_FAILED);
            std::ptr::write_bytes(q as *mut u8, 0xCD, 4096);
            if libc::madvise((q as usize + 4096) as *mut _, 4096, 102) == 0 {
                let n = read_own_memory(q as usize, &mut buf);
                assert_eq!(n, 4096, "copie arrêtée à la garde");
            }
            libc::munmap(q, 8192);
        }
    }

    /// Vidage d'un programme à plusieurs threads (qui tournent, dorment,
    /// sont profonds dans leur pile) : tous décrits, le processus survit
    /// et les threads repartent (#2147).
    #[test]
    fn vidage_de_plusieurs_threads_sans_dommage() {
        use std::sync::Arc;
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        let mut slots = Vec::new();
        for kind in 0..6 {
            let slot = Arc::new(SuspendSlot::new());
            let tid = Arc::new(AtomicI32::new(0));
            let (s2, t2, st2) = (slot.clone(), tid.clone(), stop.clone());
            handles.push(std::thread::spawn(move || {
                fn deep(n: u32, f: &dyn Fn()) -> u32 {
                    let pad = [n as u8; 512];
                    if n == 0 {
                        f();
                        return pad[0] as u32;
                    }
                    deep(n - 1, f) + std::hint::black_box(pad)[1] as u32
                }
                let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
                register(me, &s2);
                t2.store(me, Ordering::Release);
                let run = || {
                    while !st2.load(Ordering::Relaxed) {
                        if kind % 2 == 0 {
                            std::thread::sleep(Duration::from_millis(2));
                        } else {
                            for _ in 0..10_000 {
                                std::hint::spin_loop();
                            }
                            std::thread::sleep(Duration::from_micros(50));
                        }
                    }
                };
                deep(if kind < 3 { 0 } else { 200 }, &run);
                unregister(me, &s2);
            }));
            while tid.load(Ordering::Acquire) == 0 {
                std::thread::yield_now();
            }
            slots.push((tid.load(Ordering::Acquire), slot));
        }
        for _ in 0..3 {
            let refs: Vec<(i32, &SuspendSlot)> = slots.iter().map(|(t, s)| (*t, &**s)).collect();
            let text = dump_threads(&refs);
            for (t, _) in &slots {
                assert!(text.contains(&format!("== thread {t} ")), "{text}");
            }
            assert!(!text.contains("non arrêté"), "{text}");
        }
        stop.store(true, Ordering::Relaxed);
        for h in handles {
            h.join().unwrap();
        }
        for (_, s) in &slots {
            assert_eq!(s.state.load(Ordering::Acquire), IDLE);
        }
    }
    /// Variable du processus enfant de `jamais_arrete_au_milieu_de_malloc`.
    const ENFANT_MALLOC: &str = "WINLUX_TEST_2165_ENFANT";

    /// Un thread qui tourne dans malloc/free n'est jamais arrêté là : le
    /// thread qui suspend alloue ensuite (comme notre GetThreadContext ou la
    /// trace) et resterait bloqué sur le verrou d'arène tenu par le thread
    /// arrêté, que plus personne ne reprendrait (#2165). Une seule arène
    /// (MALLOC_ARENA_MAX=1, lu au démarrage) : relancé dans un processus
    /// enfant, borné à 60 s.
    #[test]
    fn jamais_arrete_au_milieu_de_malloc() {
        if std::env::var_os(ENFANT_MALLOC).is_some() {
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "dlls::thread_ctl::tests::enfant_suspension_dans_malloc",
                "--test-threads=1",
                "--include-ignored",
            ])
            .env(ENFANT_MALLOC, "1")
            .env("MALLOC_ARENA_MAX", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let t0 = Instant::now();
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break Some(st);
            }
            if t0.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let status = status.expect("interblocage : thread arrêté dans malloc (#2165)");
        assert!(status.success(), "processus enfant : {status:?}");
    }

    #[test]
    #[ignore = "lancé par jamais_arrete_au_milieu_de_malloc"]
    fn enfant_suspension_dans_malloc() {
        use std::sync::Arc;
        if std::env::var_os(ENFANT_MALLOC).is_none() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let slot = Arc::new(SuspendSlot::new());
        let tid = Arc::new(AtomicI32::new(0));
        let (s2, t2, st2) = (slot.clone(), tid.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            let me = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            register(me, &s2);
            t2.store(me, Ordering::Release);
            let mut n = 0u32;
            while !st2.load(Ordering::Relaxed) {
                // Tailles hors du cache par thread : verrou d'arène pris.
                for size in [5000usize, 9000, 20000, 70000] {
                    let p = unsafe { libc::malloc(size) };
                    unsafe { libc::free(p) };
                }
                n = n.wrapping_add(1);
                if n % 64 == 0 {
                    std::thread::sleep(Duration::from_micros(50));
                }
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let tid = tid.load(Ordering::Acquire);
        for i in 0..300usize {
            assert_eq!(suspend(&slot, tid), Ok(0));
            let v: Vec<u8> = Vec::with_capacity(3000 + i * 211);
            drop(std::hint::black_box(v));
            assert_eq!(resume(&slot), 1);
        }
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
    }
}
