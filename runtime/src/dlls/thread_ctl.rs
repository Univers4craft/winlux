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
/// noyau (attente bloquante) est arrêté où il se trouve.
const FORCE_ASLEEP_AFTER: Duration = Duration::from_millis(20);

/// Après ce délai, le thread est arrêté où il se trouve même s'il tourne
/// encore dans le code de Winlux (#2065).
const FORCE_AFTER: Duration = Duration::from_millis(200);

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
    /// S'arrêter même dans le code de Winlux.
    force: AtomicBool,
    /// `ucontext_t` publié tant que le thread est arrêté.
    uc: AtomicUsize,
    /// Arrêté hors du code invité.
    in_host: AtomicBool,
    /// Thread terminé ou en train de se terminer.
    gone: AtomicBool,
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
            uc: AtomicUsize::new(0),
            in_host: AtomicBool::new(false),
            gone: AtomicBool::new(false),
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
    // seulement quand `park` le force.
    if held() || (!guest && !slot.force.load(Ordering::Acquire)) {
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

// ============================================================
//  Arrêt et reprise
// ============================================================

/// Arrête le thread `tid` (autre que le thread courant) et attend qu'il
/// soit arrêté. Faux s'il s'est terminé entre-temps. Sous `op`.
fn park(slot: &SuspendSlot, tid: i32) -> bool {
    install_handler();
    slot.resume.store(0, Ordering::Release);
    slot.force.store(false, Ordering::Release);
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
        if slot.state.load(Ordering::Acquire) == DEFERRED {
            std::thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(1));
            let elapsed = start.elapsed();
            if elapsed > FORCE_AFTER || (elapsed > FORCE_ASLEEP_AFTER && asleep(tid)) {
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
            while !STOP.load(Ordering::Relaxed) {
                COUNTER.fetch_add(1, Ordering::Relaxed);
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
    /// qu'il la tient, même au-delà de FORCE_AFTER (#2065).
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
            while !DONE.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            unregister(me, &s2);
        });
        while tid.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        let tid = tid.load(Ordering::Acquire);
        let releaser = std::thread::spawn(|| {
            std::thread::sleep(FORCE_AFTER * 2);
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

    /// Thread endormi dans le noyau depuis Winlux : arrêté sans attendre
    /// FORCE_AFTER (#2065).
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
        assert!(took < FORCE_AFTER, "arrêt en {took:?}");
    }
}
