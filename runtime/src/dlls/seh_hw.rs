//! Fautes matérielles en exceptions Windows (#269).
//!
//! Sous Windows, un accès mémoire invalide, une division entière par zéro,
//! une instruction illégale ou un `int3` dans un programme deviennent des
//! exceptions SEH : les gestionnaires vectorisés (VEH) puis les blocs
//! `__try/__except` de l'appli peuvent les rattraper et reprendre. La JVM,
//! coreclr (`NullReferenceException`), les moteurs JavaScript et les
//! programmes Go s'en servent en fonctionnement normal.
//!
//! Sous Linux, ces fautes arrivent en SIGSEGV, SIGBUS, SIGFPE, SIGILL ou
//! SIGTRAP. Le gestionnaire de signal (contexte asynchrone, pile de secours)
//! fait seulement le tri et la traduction : si la faute vient du code invité,
//! il construit `EXCEPTION_RECORD` et `CONTEXT` depuis le `ucontext` sur la
//! pile du thread, sous le point de faute, puis modifie le `ucontext` pour
//! qu'au retour du signal le thread exécute `hw_dispatch` sur cette pile,
//! hors contexte de signal, comme `KiUserExceptionDispatcher`. La
//! répartition elle-même est celle de `seh.rs` (VEH, puis cadres).
//!
//! Si personne ne traite l'exception (ni gestionnaire, ni filtre de dernier
//! recours), le thread rejoue l'instruction fautive avec la répartition
//! coupée : la faute revient au signal et part au rapport de plantage
//! habituel (`crash.rs`), avec les vrais registres.
//!
//! Une faute dans le code de Winlux lui-même (Rust, bibliothèques Linux)
//! n'est jamais répartie : elle peut survenir verrou tenu, et ce code n'a
//! pas de tables de déroulement Windows.

use super::seh::{self, Context, ExceptionRecord, Unhandled};
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

pub const STATUS_GUARD_PAGE_VIOLATION: u32 = 0x8000_0001;
pub const STATUS_BREAKPOINT: u32 = 0x8000_0003;
pub const STATUS_SINGLE_STEP: u32 = 0x8000_0004;
pub const STATUS_ACCESS_VIOLATION: u32 = 0xC000_0005;
pub const STATUS_IN_PAGE_ERROR: u32 = 0xC000_0006;
pub const STATUS_ILLEGAL_INSTRUCTION: u32 = 0xC000_001D;
pub const STATUS_FLOAT_DENORMAL_OPERAND: u32 = 0xC000_008D;
pub const STATUS_FLOAT_DIVIDE_BY_ZERO: u32 = 0xC000_008E;
pub const STATUS_FLOAT_INEXACT_RESULT: u32 = 0xC000_008F;
pub const STATUS_FLOAT_INVALID_OPERATION: u32 = 0xC000_0090;
pub const STATUS_FLOAT_OVERFLOW: u32 = 0xC000_0091;
pub const STATUS_FLOAT_UNDERFLOW: u32 = 0xC000_0093;
pub const STATUS_INTEGER_DIVIDE_BY_ZERO: u32 = 0xC000_0094;
pub const STATUS_INTEGER_OVERFLOW: u32 = 0xC000_0095;
pub const STATUS_PRIVILEGED_INSTRUCTION: u32 = 0xC000_0096;
pub const STATUS_STACK_OVERFLOW: u32 = 0xC000_00FD;

const PAGE: usize = 4096;
/// Espace laissé libre sous le RSP fautif avant la trame d'exception.
const STACK_GAP: usize = 512;
/// Pile qu'il faut encore sous la trame pour répartir (code Rust, puis
/// gestionnaires et filtres de l'appli).
const DISPATCH_HEADROOM: usize = 48 * 1024;
/// Pile de secours d'un thread dont la pile est épuisée.
const EMERGENCY_STACK: usize = 512 * 1024;

/// Coupure globale : `WINLUX_SEH_HW=0` rend l'ancien comportement (toute
/// faute matérielle va directement au rapport de plantage).
static ENABLED: AtomicBool = AtomicBool::new(true);

thread_local! {
    /// RIP d'une faute à ne pas répartir une seconde fois : l'exception n'a
    /// été traitée par personne et l'instruction est rejouée pour le
    /// rapport de plantage.
    static REFIRE: Cell<usize> = const { Cell::new(0) };
    /// Pile de secours (base, 0 tant qu'elle n'existe pas).
    static EMERGENCY: Cell<usize> = const { Cell::new(0) };
}

/// Lu une fois au démarrage, hors contexte de signal.
pub fn init() {
    let off = std::env::var_os("WINLUX_SEH_HW").is_some_and(|v| v == "0");
    ENABLED.store(!off, Ordering::Relaxed);
}

/// Trame posée sur la pile du thread fautif.
#[repr(C, align(16))]
struct HwFrame {
    /// CONTEXT remis aux gestionnaires (qu'ils peuvent modifier).
    ctx: Context,
    /// CONTEXT d'origine, pour rejouer la faute si personne ne la traite.
    orig: Context,
    rec: ExceptionRecord,
}

// ============================================================
//  CONTEXT <-> ucontext
// ============================================================

const GREGS: [(i32, usize); 17] = [
    (libc::REG_RAX, 0x78),
    (libc::REG_RCX, 0x80),
    (libc::REG_RDX, 0x88),
    (libc::REG_RBX, 0x90),
    (libc::REG_RSP, 0x98),
    (libc::REG_RBP, 0xA0),
    (libc::REG_RSI, 0xA8),
    (libc::REG_RDI, 0xB0),
    (libc::REG_R8, 0xB8),
    (libc::REG_R9, 0xC0),
    (libc::REG_R10, 0xC8),
    (libc::REG_R11, 0xD0),
    (libc::REG_R12, 0xD8),
    (libc::REG_R13, 0xE0),
    (libc::REG_R14, 0xE8),
    (libc::REG_R15, 0xF0),
    (libc::REG_RIP, 0xF8),
];

/// CONTEXT_AMD64 | CONTROL | INTEGER | SEGMENTS | FLOATING_POINT.
pub const CONTEXT_FULL_SEGMENTS: u32 = 0x0010_000F;
const CONTEXT_CONTROL: u32 = 0x1;
const CONTEXT_INTEGER: u32 = 0x2;
const CONTEXT_FLOATING_POINT: u32 = 0x8;

/// Remplit un CONTEXT Windows depuis les registres sauvés par le noyau.
/// Async-signal-safe (copies mémoire seulement).
pub(crate) unsafe fn context_from_ucontext(uc: *const libc::ucontext_t, ctx: &mut Context) {
    let g = &(*uc).uc_mcontext.gregs;
    for (reg, off) in GREGS {
        ctx.set(off, g[reg as usize] as u64);
    }
    let p = ctx.ptr();
    (p.add(0x30) as *mut u32).write_unaligned(CONTEXT_FULL_SEGMENTS);
    (p.add(seh::CTX_EFLAGS) as *mut u32).write_unaligned(g[libc::REG_EFL as usize] as u32);
    // Sélecteurs d'un processus 64 bits sous Windows.
    for (off, sel) in [
        (0x38, 0x33u16),
        (0x3A, 0x2B),
        (0x3C, 0x2B),
        (0x3E, 0x53),
        (0x40, 0x2B),
        (0x42, 0x2B),
    ] {
        (p.add(off) as *mut u16).write_unaligned(sel);
    }
    let fp = (*uc).uc_mcontext.fpregs as *const u8;
    if !fp.is_null() {
        // Zone FXSAVE de Linux = XMM_SAVE_AREA32 (FltSave, à 0x100) ; les
        // registres XMM y sont à +0xA0, soit Xmm0 à 0x1A0 dans CONTEXT.
        std::ptr::copy_nonoverlapping(fp, p.add(0x100), 512);
        let mxcsr = (fp.add(24) as *const u32).read_unaligned();
        (p.add(0x34) as *mut u32).write_unaligned(mxcsr);
    }
}

/// Écrit dans le `ucontext` d'un thread arrêté les registres d'un CONTEXT,
/// selon ses drapeaux (CONTROL, INTEGER, FLOATING_POINT).
#[allow(dead_code)] // SetThreadContext (#269, deuxième partie)
pub(crate) unsafe fn context_to_ucontext(ctx: &Context, uc: *mut libc::ucontext_t) {
    let p = ctx.0.as_ptr();
    let flags = (p.add(0x30) as *const u32).read_unaligned();
    let g = &mut (*uc).uc_mcontext.gregs;
    for (reg, off) in GREGS {
        let control = matches!(reg, libc::REG_RSP | libc::REG_RIP);
        let wanted = if control {
            flags & CONTEXT_CONTROL != 0
        } else {
            flags & CONTEXT_INTEGER != 0
        };
        if wanted {
            g[reg as usize] = ctx.get(off) as i64;
        }
    }
    if flags & CONTEXT_CONTROL != 0 {
        let eflags = (p.add(seh::CTX_EFLAGS) as *const u32).read_unaligned() as i64;
        // Seuls les drapeaux modifiables en mode utilisateur.
        const USER: i64 = 0x0000_0CD5 | 0x100 | 0x0004_0000 | 0x0020_0000;
        let efl = &mut g[libc::REG_EFL as usize];
        *efl = (*efl & !USER) | (eflags & USER);
    }
    let fp = (*uc).uc_mcontext.fpregs as *mut u8;
    if flags & CONTEXT_FLOATING_POINT != 0 && !fp.is_null() {
        // Pas MXCSR_MASK (+28) : il décrit le processeur, pas le thread.
        let mask = (fp.add(28) as *const u32).read_unaligned();
        std::ptr::copy_nonoverlapping(p.add(0x100), fp, 512);
        (fp.add(28) as *mut u32).write_unaligned(mask);
        let mxcsr = (p.add(0x34) as *const u32).read_unaligned();
        (fp.add(24) as *mut u32).write_unaligned(mxcsr & if mask != 0 { mask } else { 0xFFBF });
    }
}

// ============================================================
//  Tri dans le gestionnaire de signal
// ============================================================

/// Le code à `rip` appartient-il au programme Windows (module PE, code JIT
/// déclaré ou mémoire VirtualAlloc exécutable) ? Les tables sont protégées
/// par des verrous : on ne fait que des essais sans attente (le thread
/// fautif pourrait lui-même les tenir). Faute de réponse, « non ».
unsafe fn is_guest_code(rip: usize) -> bool {
    for _ in 0..64 {
        let mut busy = false;
        match crate::loader::MODULES.try_lock() {
            Ok(m) => {
                if m.iter().any(|m| m.contains(rip)) {
                    return true;
                }
            }
            Err(_) => busy = true,
        }
        match seh::dynamic_contains_try(rip as u64) {
            Some(true) => return true,
            Some(false) => {}
            None => busy = true,
        }
        match super::kernel32::exec_region_try(rip) {
            Some(true) => return true,
            Some(false) => {}
            None => busy = true,
        }
        if !busy {
            return false;
        }
        libc::sched_yield();
    }
    false
}

/// Octet de code à `addr` (le code invité est lisible : il vient d'être
/// exécuté ou décodé par le processeur).
unsafe fn code_byte(addr: usize) -> u8 {
    *(addr as *const u8)
}

/// Instructions privilégiées en mode utilisateur (faute #GP) : `hlt`, `cli`,
/// `sti`, `in`/`out`, `clts`, `wbinvd`, `invd`, accès aux registres de
/// contrôle et de débogage.
unsafe fn is_privileged(rip: usize) -> bool {
    let mut at = rip;
    // Préfixes de taille et de segment.
    while matches!(
        code_byte(at),
        0x66 | 0x67 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 | 0xF2 | 0xF3
    ) {
        at += 1;
    }
    match code_byte(at) {
        0xF4 | 0xFA | 0xFB | 0xE4..=0xE7 | 0xEC..=0xEF | 0x6C..=0x6F => true,
        0x0F => matches!(
            code_byte(at + 1),
            0x06 | 0x08 | 0x09 | 0x20..=0x23 | 0x30 | 0x32
        ),
        _ => false,
    }
}

struct Fault {
    code: u32,
    address: usize,
    params: [usize; 2],
    nparams: u32,
    /// RIP à placer dans le CONTEXT (int3 : l'instruction elle-même).
    ctx_rip: usize,
    /// Pile épuisée : répartir sur la pile de secours.
    overflow: bool,
}

unsafe fn classify(
    sig: i32,
    info: *const libc::siginfo_t,
    uc: *const libc::ucontext_t,
) -> Option<Fault> {
    let g = &(*uc).uc_mcontext.gregs;
    let rip = g[libc::REG_RIP as usize] as usize;
    let rsp = g[libc::REG_RSP as usize] as usize;
    let addr = (*info).si_addr() as usize;
    let si_code = (*info).si_code;
    let mut f = Fault {
        code: 0,
        address: rip,
        params: [0; 2],
        nparams: 0,
        ctx_rip: rip,
        overflow: false,
    };
    match sig {
        libc::SIGSEGV | libc::SIGBUS => {
            let trapno = g[libc::REG_TRAPNO as usize];
            let err = g[libc::REG_ERR as usize];
            if trapno == 13 {
                // __fastfail (int 0x29) : arrêt immédiat, jamais réparti.
                if code_byte(rip) == 0xCD && code_byte(rip + 1) == 0x29 {
                    return None;
                }
                if is_privileged(rip) {
                    f.code = STATUS_PRIVILEGED_INSTRUCTION;
                    return Some(f);
                }
                // Adresse non canonique : Windows signale une lecture de -1.
                f.code = STATUS_ACCESS_VIOLATION;
                f.params = [0, usize::MAX];
                f.nparams = 2;
                return Some(f);
            }
            let rw = if err & 0x10 != 0 {
                8
            } else if err & 0x2 != 0 {
                1
            } else {
                0
            };
            f.params = [rw, addr];
            f.nparams = 2;
            f.code = if sig == libc::SIGBUS && si_code == libc::BUS_ADRERR {
                f.nparams = 3;
                STATUS_IN_PAGE_ERROR
            } else {
                STATUS_ACCESS_VIOLATION
            };
            if super::kernel32::is_guard_page_try(addr) == Some(true) {
                f.code = STATUS_GUARD_PAGE_VIOLATION;
                f.nparams = 2;
            } else if is_stack_overflow(addr, rsp) {
                f.code = STATUS_STACK_OVERFLOW;
                f.nparams = 2;
                f.overflow = true;
            }
        }
        libc::SIGILL => f.code = STATUS_ILLEGAL_INSTRUCTION,
        libc::SIGFPE => {
            // si_code de SIGFPE : FPE_INTDIV 1, INTOVF 2, FLTDIV 3, FLTOVF 4,
            // FLTUND 5, FLTRES 6, FLTINV 7, FLTSUB 8.
            f.code = match si_code {
                2 => STATUS_INTEGER_OVERFLOW,
                3 => STATUS_FLOAT_DIVIDE_BY_ZERO,
                4 => STATUS_FLOAT_OVERFLOW,
                5 => STATUS_FLOAT_UNDERFLOW,
                6 => STATUS_FLOAT_INEXACT_RESULT,
                7 => STATUS_FLOAT_INVALID_OPERATION,
                8 => STATUS_FLOAT_DENORMAL_OPERAND,
                _ => STATUS_INTEGER_DIVIDE_BY_ZERO,
            }
        }
        libc::SIGTRAP => {
            // TRAP_TRACE (2) et TRAP_HWBKPT (4) : pas à pas ou point d'arrêt
            // matériel ; sinon int3 (SI_KERNEL ou TRAP_BRKPT).
            if si_code == 2 || si_code == 4 {
                f.code = STATUS_SINGLE_STEP;
            } else {
                f.code = STATUS_BREAKPOINT;
                f.nparams = 1;
                if rip > 0 && code_byte(rip - 1) == 0xCC {
                    f.address = rip - 1;
                    f.ctx_rip = rip - 1;
                }
            }
        }
        _ => return None,
    }
    Some(f)
}

/// Faute juste sous le pointeur de pile ou dans la dernière page de la pile
/// du thread : pile épuisée.
unsafe fn is_stack_overflow(addr: usize, rsp: usize) -> bool {
    let teb = crate::process::teb();
    if teb != 0 {
        let limit = *((teb + crate::process::TEB_STACK_LIMIT) as *const usize);
        if limit != 0 && addr < limit + PAGE && addr + 64 * PAGE >= limit && rsp < limit + 64 * PAGE
        {
            return true;
        }
    }
    // Heuristique de crash.rs : faute juste sous RSP (pile non projetée).
    addr < rsp && rsp - addr <= 16 * PAGE
}

/// Pile de secours du thread courant (créée au premier débordement).
unsafe fn emergency_stack() -> Option<usize> {
    let base = EMERGENCY.with(|e| e.get());
    if base != 0 {
        return Some(base);
    }
    let p = libc::mmap(
        std::ptr::null_mut(),
        EMERGENCY_STACK,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );
    if p == libc::MAP_FAILED {
        return None;
    }
    // Page de garde au bas : un débordement sur la pile de secours n'écrase rien.
    libc::mprotect(p, PAGE, libc::PROT_NONE);
    EMERGENCY.with(|e| e.set(p as usize));
    Some(p as usize)
}

/// Appelé en tête des gestionnaires de signal de plantage. Vrai si la faute
/// a été transformée en exception Windows : le gestionnaire doit alors
/// simplement revenir, le thread reprend dans `hw_dispatch`.
///
/// # Safety
/// Contexte de signal (SA_SIGINFO), `info` et `ctx` fournis par le noyau.
pub unsafe fn on_fault(sig: i32, info: *mut libc::siginfo_t, ctx: *mut c_void) -> bool {
    if !ENABLED.load(Ordering::Relaxed) || info.is_null() || ctx.is_null() {
        return false;
    }
    let uc = ctx as *mut libc::ucontext_t;
    let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
    let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as usize;
    // Faute rejouée après une exception non traitée : rapport de plantage.
    let refire = REFIRE.with(|r| r.replace(0));
    if refire != 0 && (refire == rip || refire + 1 == rip) {
        return false;
    }
    let teb = crate::process::teb();
    if teb == 0 || !is_guest_code(rip) {
        return false;
    }
    let Some(fault) = classify(sig, info, uc) else {
        return false;
    };
    // Où poser la trame : sous le RSP fautif si la pile le permet, sinon
    // (pile épuisée) sur la pile de secours du thread.
    let size = std::mem::size_of::<HwFrame>();
    let limit = *((teb + crate::process::TEB_STACK_LIMIT) as *const usize);
    let base = *((teb + crate::process::TEB_STACK_BASE) as *const usize);
    let candidate = rsp.wrapping_sub(STACK_GAP + size) & !63;
    let fits = limit != 0
        && rsp <= base
        && rsp > limit
        && candidate > limit
        && candidate - limit >= DISPATCH_HEADROOM;
    let frame = if fits && !fault.overflow {
        candidate
    } else {
        let Some(stack) = emergency_stack() else {
            return false;
        };
        // Déjà sur la pile de secours : faute pendant la répartition d'un
        // débordement, on abandonne au rapport de plantage.
        if rsp >= stack && rsp < stack + EMERGENCY_STACK {
            return false;
        }
        (stack + EMERGENCY_STACK - STACK_GAP - size) & !63
    };
    let f = &mut *(frame as *mut HwFrame);
    context_from_ucontext(uc, &mut f.ctx);
    f.ctx.set(seh::CTX_RIP, fault.ctx_rip as u64);
    if fault.code == STATUS_SINGLE_STEP {
        // Windows efface TF avant de remettre le contexte aux gestionnaires.
        let flags = (f.ctx.get(seh::CTX_EFLAGS) as u32) & !0x100;
        f.ctx.0[seh::CTX_EFLAGS..seh::CTX_EFLAGS + 4].copy_from_slice(&flags.to_le_bytes());
    }
    f.orig = f.ctx;
    f.rec = ExceptionRecord::new(fault.code, 0, fault.address);
    f.rec.nparams = fault.nparams;
    f.rec.info[0] = fault.params[0];
    f.rec.info[1] = fault.params[1];
    if fault.code == STATUS_IN_PAGE_ERROR {
        f.rec.info[2] = STATUS_IN_PAGE_ERROR as usize;
    }
    // Au retour du signal : hw_dispatch(frame) sur la pile choisie, DF et
    // TF effacés comme à l'entrée d'une fonction.
    let g = &mut (*uc).uc_mcontext.gregs;
    g[libc::REG_RSP as usize] = frame as i64;
    g[libc::REG_RDI as usize] = frame as i64;
    g[libc::REG_RIP as usize] = winlux_hw_entry as *const () as usize as i64;
    g[libc::REG_EFL as usize] &= !(0x400 | 0x100);
    true
}

/// Point de reprise après le signal : RSP = trame (alignée sur 16).
#[unsafe(naked)]
unsafe extern "C" fn winlux_hw_entry() -> ! {
    core::arch::naked_asm!(
        "and rsp, -16",
        "call {dispatch}",
        "ud2",
        dispatch = sym hw_dispatch,
    )
}

/// Répartition d'une faute matérielle, sur la pile du thread fautif et hors
/// contexte de signal.
unsafe extern "C" fn hw_dispatch(frame: *mut HwFrame) -> ! {
    let f = &mut *frame;
    let code = f.rec.code;
    if code == STATUS_GUARD_PAGE_VIOLATION {
        // Une seule levée par page de garde, comme Windows.
        super::kernel32::clear_guard_page(f.rec.info[1]);
    }
    crate::trace!(
        "faute matérielle {:#010x} à {} (arguments {:x?})",
        code,
        crate::thunks::describe_site(f.rec.address),
        &f.rec.info[..f.rec.nparams as usize]
    );
    match seh::dispatch(&mut f.rec, &mut f.ctx) {
        Unhandled::InvalidDisposition => {
            super::kernel32_misc::report_unhandled(0xC000_0026, 1, &[])
        }
        Unhandled::NotHandled | Unhandled::Noncontinuable => {}
    }
    // Filtre de dernier recours du programme (SetUnhandledExceptionFilter).
    let filter = super::kernel32_misc::current_unhandled_filter();
    if filter != 0 {
        eprintln!("{}", seh::describe_for_app_filter(&f.rec, &f.ctx));
        let mut ptrs = seh::ExceptionPointers {
            record: &mut f.rec,
            context: f.ctx.ptr(),
        };
        let call: unsafe extern "win64" fn(*mut seh::ExceptionPointers) -> i32 =
            std::mem::transmute(filter);
        if call(&mut ptrs) == seh::EXCEPTION_CONTINUE_EXECUTION {
            seh::resume(&f.ctx);
        }
    }
    match code {
        // Pas de faute à rejouer : arrêt avec le code, comme Windows.
        STATUS_GUARD_PAGE_VIOLATION | STATUS_SINGLE_STEP => {
            super::kernel32_misc::report_unhandled(code, 0, &f.rec.info[..f.rec.nparams as usize])
        }
        _ => {
            // Rejoue l'instruction fautive sans répartition : rapport de
            // plantage complet par crash.rs, avec les registres d'origine.
            let rip = f.orig.get(seh::CTX_RIP) as usize;
            REFIRE.with(|r| r.set(rip));
            seh::resume(&f.orig)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trame_alignee() {
        assert_eq!(std::mem::align_of::<HwFrame>(), 16);
        assert_eq!(std::mem::size_of::<HwFrame>() % 16, 0);
    }

    #[test]
    fn contexte_aller_retour() {
        unsafe {
            let mut uc: libc::ucontext_t = std::mem::zeroed();
            let mut fp = vec![0u8; 512];
            uc.uc_mcontext.fpregs = fp.as_mut_ptr().cast();
            for (i, (reg, _)) in GREGS.iter().enumerate() {
                uc.uc_mcontext.gregs[*reg as usize] = 0x1000 + i as i64;
            }
            uc.uc_mcontext.gregs[libc::REG_EFL as usize] = 0x246;
            fp[24..28].copy_from_slice(&0x1F80u32.to_le_bytes());
            fp[0xA0] = 0x5A; // XMM0, premier octet
            let mut ctx = Context::zeroed();
            context_from_ucontext(&uc, &mut ctx);
            assert_eq!(ctx.get(0x78), 0x1000); // RAX
            assert_eq!(ctx.get(0xF8), 0x1000 + 16); // RIP
            assert_eq!(ctx.0[seh::CTX_XMM0], 0x5A);
            assert_eq!(
                u32::from_le_bytes(ctx.0[0x34..0x38].try_into().unwrap()),
                0x1F80
            );
            ctx.set(0x78, 0xABCD);
            ctx.set(seh::CTX_RIP, 0x4242);
            let mut out: libc::ucontext_t = std::mem::zeroed();
            let mut fp2 = vec![0u8; 512];
            out.uc_mcontext.fpregs = fp2.as_mut_ptr().cast();
            context_to_ucontext(&ctx, &mut out);
            assert_eq!(out.uc_mcontext.gregs[libc::REG_RAX as usize], 0xABCD);
            assert_eq!(out.uc_mcontext.gregs[libc::REG_RIP as usize], 0x4242);
            // IF et le bit réservé restent ceux du noyau.
            assert_eq!(out.uc_mcontext.gregs[libc::REG_EFL as usize] & 0xFF, 0x44);
            assert_eq!(fp2[0xA0], 0x5A);
        }
    }
}
