//! Exceptions des programmes x86 (#232, étape 2) : SEH par chaîne de cadres.
//!
//! En 32 bits, Windows ne lit pas de tables de déroulement : chaque
//! fonction protégée inscrit un cadre {précédent, gestionnaire} en tête de
//! la liste chaînée `fs:[0]` (TEB32.ExceptionList). Pour une exception, on
//! appelle les gestionnaires de la tête vers la queue ; celui qui la prend
//! déroule (RtlUnwind) puis saute lui-même dans le bloc `__except`.
//!
//! - Fautes matérielles : le gestionnaire de signal (`super::fault32`)
//!   construit EXCEPTION_RECORD et CONTEXT x86 sous la pile du programme
//!   et revient du signal dans `hw_dispatch` (64 bits, même pile).
//! - `RaiseException`, `RtlUnwind`, `SetUnhandledExceptionFilter`,
//!   `UnhandledExceptionFilter` : versions x86.
//! - `_except_handler3` (msvcrt, VC6 et clang) et `_except_handler4_common`
//!   (vcruntime140, MSVC récent) : gestionnaires des blocs `__try` du C,
//!   d'après leur disposition publique (cadre à ebp-0x10, table des
//!   portées, niveau courant).
//! - Les blocs `__except` sont atteints en rechargeant un CONTEXT x86
//!   (`resume`) : les cadres 64 bits de la répartition sont abandonnés,
//!   comme les cadres x86 dépassés.

use super::{call32, rd, wr};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

pub const CONTEXT32_SIZE: usize = 0x2cc;
pub const EXCEPTION_RECORD32_SIZE: usize = 0x50;
pub const CTX_FLAGS: usize = 0x00;
pub const CTX_EDI: usize = 0x9c;
pub const CTX_ESI: usize = 0xa0;
pub const CTX_EBX: usize = 0xa4;
pub const CTX_EDX: usize = 0xa8;
pub const CTX_ECX: usize = 0xac;
pub const CTX_EAX: usize = 0xb0;
pub const CTX_EBP: usize = 0xb4;
pub const CTX_EIP: usize = 0xb8;
pub const CTX_SEGCS: usize = 0xbc;
pub const CTX_EFLAGS: usize = 0xc0;
pub const CTX_ESP: usize = 0xc4;
pub const CTX_SEGSS: usize = 0xc8;
pub const CTX_EXTENDED: usize = 0xcc;
/// CONTEXT_i386 | CONTROL | INTEGER | SEGMENTS.
const CONTEXT_FULL: u32 = 0x0001_0007;
const CONTEXT_EXTENDED_REGISTERS: u32 = 0x0001_0020;

const EXCEPTION_NONCONTINUABLE: u32 = 1;
const EXCEPTION_UNWINDING: u32 = 2;
const EXCEPTION_EXIT_UNWIND: u32 = 4;
const STATUS_UNWIND: u32 = 0xC000_0027;
const STATUS_NONCONTINUABLE_EXCEPTION: u32 = 0xC000_0025;
const END_OF_CHAIN: u32 = 0xffff_ffff;

/// Filtre de dernier recours x86 (SetUnhandledExceptionFilter).
static FILTER32: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Valeur de la chaîne des appels 64 → 32 au début de chaque
    /// répartition en cours : un saut vers un bloc `__except` la rétablit.
    static DISPATCHES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

extern "win64" {
    /// rcx = CONTEXT x86, rdx = chaîne des appels à rétablir, r8 = zone
    /// FXSAVE alignée sur 16 (ou 0). Ne revient pas.
    fn winlux_wow_resume32(ctx: usize, chain: u64, fx: usize) -> !;
}

std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_resume32
winlux_wow_resume32:
    mov qword ptr gs:[{rsp_chain}], rdx
    test r8, r8
    jz 1f
    fxrstor [r8]
1:
    mov r9, rcx
    movzx ecx, word ptr gs:[{fs_selector}]
    mov fs, ecx
    mov eax, 0x2b
    mov ds, eax
    mov es, eax
    push 0x2b
    mov eax, dword ptr [r9 + {esp}]
    push rax
    mov eax, dword ptr [r9 + {eflags}]
    push rax
    push {cs32}
    mov eax, dword ptr [r9 + {eip}]
    push rax
    mov edi, dword ptr [r9 + {edi}]
    mov esi, dword ptr [r9 + {esi}]
    mov ebx, dword ptr [r9 + {ebx}]
    mov edx, dword ptr [r9 + {edx}]
    mov ecx, dword ptr [r9 + {ecx}]
    mov ebp, dword ptr [r9 + {ebp}]
    mov eax, dword ptr [r9 + {eax}]
    iretq

// Retour du signal d'une faute x86 : RSP = RDI = trame (sous la pile x86).
    .globl winlux_wow_fault64
winlux_wow_fault64:
    and rsp, -16
    call {hw_dispatch}
    ud2
"#,
    rsp_chain = const super::TEB64_RSP_CHAIN,
    fs_selector = const super::TEB64_FS_SELECTOR,
    cs32 = const super::CS32,
    esp = const CTX_ESP,
    eflags = const CTX_EFLAGS,
    eip = const CTX_EIP,
    edi = const CTX_EDI,
    esi = const CTX_ESI,
    ebx = const CTX_EBX,
    edx = const CTX_EDX,
    ecx = const CTX_ECX,
    ebp = const CTX_EBP,
    eax = const CTX_EAX,
    hw_dispatch = sym hw_dispatch,
);

extern "C" {
    pub fn winlux_wow_fault64();
}

pub(super) fn chain_now() -> u64 {
    let v: u64;
    unsafe {
        std::arch::asm!("mov {}, qword ptr gs:[{off}]", out(reg) v, off = const super::TEB64_RSP_CHAIN);
    }
    v
}

fn teb32() -> u32 {
    let v: u64;
    unsafe {
        std::arch::asm!("mov {}, qword ptr gs:[{off}]", out(reg) v, off = const super::TEB64_TEB32);
    }
    v as u32
}

/// Recharge un CONTEXT x86, avec la chaîne des appels 64 → 32 qui était
/// en place quand ce code x86 tournait.
pub fn resume(ctx: u32, chain: u64, fx: Option<&Fx>) -> ! {
    unsafe { winlux_wow_resume32(ctx as usize, chain, fx.map_or(0, |f| f.0.as_ptr() as usize)) }
}

/// Saut vers un bloc `__except` depuis un gestionnaire : la répartition qui
/// l'a appelé (et ses cadres 64 bits) est abandonnée.
pub(super) fn transfer(ctx: u32) -> ! {
    let chain = DISPATCHES
        .with(|d| d.borrow_mut().pop())
        .unwrap_or_else(chain_now);
    resume(ctx, chain, None)
}

/// Zone FXSAVE alignée (état x87/SSE d'une faute).
#[repr(C, align(16))]
pub struct Fx(pub [u8; 512]);

/// Trame posée sous la pile x86 par le gestionnaire de signal.
#[repr(C, align(64))]
pub struct HwFrame {
    pub rec: [u8; EXCEPTION_RECORD32_SIZE],
    pub ctx: [u8; CONTEXT32_SIZE],
    pub orig: [u8; CONTEXT32_SIZE],
    pub fx: Fx,
}

fn put(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

/// Remplit l'EXCEPTION_RECORD x86 `rec`.
fn fill_record(rec: &mut [u8], code: u32, flags: u32, address: u32, params: &[u32]) {
    rec.fill(0);
    put(rec, 0, code);
    put(rec, 4, flags);
    put(rec, 12, address);
    let n = params.len().min(15);
    put(rec, 16, n as u32);
    for (i, p) in params.iter().take(n).enumerate() {
        put(rec, 20 + i * 4, *p);
    }
}

/// CONTEXT x86 depuis le contexte d'un signal reçu en mode 32 bits.
pub unsafe fn context_from_ucontext(uc: *const libc::ucontext_t, ctx: &mut [u8], fx: &mut Fx) {
    let g = &(*uc).uc_mcontext.gregs;
    let r = |i: i32| g[i as usize] as u32;
    ctx.fill(0);
    put(ctx, CTX_FLAGS, CONTEXT_FULL | CONTEXT_EXTENDED_REGISTERS);
    put(ctx, CTX_EDI, r(libc::REG_RDI));
    put(ctx, CTX_ESI, r(libc::REG_RSI));
    put(ctx, CTX_EBX, r(libc::REG_RBX));
    put(ctx, CTX_EDX, r(libc::REG_RDX));
    put(ctx, CTX_ECX, r(libc::REG_RCX));
    put(ctx, CTX_EAX, r(libc::REG_RAX));
    put(ctx, CTX_EBP, r(libc::REG_RBP));
    put(ctx, CTX_EIP, r(libc::REG_RIP));
    put(ctx, CTX_SEGCS, super::CS32 as u32);
    put(ctx, CTX_EFLAGS, r(libc::REG_EFL));
    put(ctx, CTX_ESP, r(libc::REG_RSP));
    put(ctx, CTX_SEGSS, 0x2b);
    // SegFs, SegDs, SegEs.
    put(ctx, 0x90, rd_selector() as u32);
    put(ctx, 0x94, 0x2b);
    put(ctx, 0x98, 0x2b);
    let fp = (*uc).uc_mcontext.fpregs as *const u8;
    if !fp.is_null() {
        std::ptr::copy_nonoverlapping(fp, fx.0.as_mut_ptr(), 512);
        ctx[CTX_EXTENDED..CTX_EXTENDED + 512].copy_from_slice(&fx.0);
    }
}

fn rd_selector() -> u16 {
    let v: u64;
    unsafe {
        std::arch::asm!("mov {}, qword ptr gs:[{off}]", out(reg) v, off = const super::TEB64_FS_SELECTOR);
    }
    v as u16
}

/// Le cadre `f` est-il plausible (aligné, dans la pile du thread) ?
fn frame_ok(f: u32) -> bool {
    let teb = teb32();
    if teb == 0 || f & 3 != 0 {
        return false;
    }
    let base = rd(teb + 4);
    let limit = rd(teb + 8);
    f >= limit && f.checked_add(8).is_some_and(|e| e <= base)
}

/// Répartition d'une exception x86. Rend si personne ne l'a prise ou si un
/// gestionnaire demande de continuer (`true`).
fn dispatch(rec: u32, ctx: u32) -> bool {
    // Une répartition abandonnée par un saut direct (Delphi, C++Builder)
    // laisse sa valeur : la longueur notée ici la retire au retour.
    let depth = DISPATCHES.with(|d| {
        let mut d = d.borrow_mut();
        d.push(chain_now());
        d.len() - 1
    });
    let result = walk(rec, ctx);
    DISPATCHES.with(|d| d.borrow_mut().truncate(depth));
    result
}

fn walk(rec: u32, ctx: u32) -> bool {
    let teb = teb32();
    let mut frame = rd(teb);
    let mut dispatcher_context = 0u32;
    let dc = &mut dispatcher_context as *mut u32 as usize as u32;
    while frame != END_OF_CHAIN && frame != 0 {
        if !frame_ok(frame) {
            crate::trace!("wow32 : cadre SEH invalide 0x{frame:x}, répartition arrêtée");
            break;
        }
        let handler = rd(frame + 4);
        crate::trace!("wow32 : gestionnaire SEH 0x{handler:x} (cadre 0x{frame:x})");
        let r = call32(handler as usize, &[rec, frame, ctx, dc]) as u32;
        crate::trace!("wow32 : gestionnaire SEH 0x{handler:x} → {r}");
        match r {
            // ExceptionContinueExecution
            0 => {
                if rd(rec + 4) & EXCEPTION_NONCONTINUABLE != 0 {
                    let mut nested = [0u8; EXCEPTION_RECORD32_SIZE];
                    fill_record(
                        &mut nested,
                        STATUS_NONCONTINUABLE_EXCEPTION,
                        EXCEPTION_NONCONTINUABLE,
                        0,
                        &[],
                    );
                    put(&mut nested, 8, rec);
                    unhandled(&nested, ctx)
                }
                return true;
            }
            // ExceptionContinueSearch, ou réponse inattendue.
            _ => frame = rd(frame),
        }
    }
    // Filtre de dernier recours du programme.
    let filter = FILTER32.load(Ordering::Relaxed);
    if filter != 0 {
        let ptrs = [rec, ctx];
        let p = ptrs.as_ptr() as usize as u32;
        if call32(filter as usize, &[p]) as u32 as i32 == -1 {
            return true;
        }
    }
    false
}

/// Trace d'une faute x86 : registres et premiers mots de la pile, pour
/// retrouver l'appelant d'un saut vers une adresse nulle (#779).
fn trace_fault_state(ctx: &[u8]) {
    if !crate::log::enabled(crate::log::Level::Trace) {
        return;
    }
    let esp = get(ctx, CTX_ESP);
    let words: Vec<String> = (0..24u32)
        .map(|i| {
            let w = rd(esp.wrapping_add(i * 4));
            format!("{w:08x}")
        })
        .collect();
    crate::trace!(
        "wow32 :   eax={:08x} ebx={:08x} ecx={:08x} edx={:08x} esi={:08x} edi={:08x} ebp={:08x} esp={esp:08x}",
        get(ctx, CTX_EAX),
        get(ctx, CTX_EBX),
        get(ctx, CTX_ECX),
        get(ctx, CTX_EDX),
        get(ctx, CTX_ESI),
        get(ctx, CTX_EDI),
        get(ctx, CTX_EBP)
    );
    crate::trace!("wow32 :   pile : {}", words.join(" "));
    crate::trace!(
        "wow32 :   [esp] = {}",
        crate::thunks::describe_site(rd(esp) as usize)
    );
    // Adresses de retour le long de la chaîne des ebp, tant qu'elle reste
    // dans la pile du thread.
    let mut ebp = get(ctx, CTX_EBP);
    let mut sites = Vec::new();
    while sites.len() < 10 && frame_ok(ebp) && ebp >= esp {
        sites.push(crate::thunks::describe_site(rd(ebp + 4) as usize));
        let next = rd(ebp);
        if next <= ebp {
            break;
        }
        ebp = next;
    }
    crate::trace!("wow32 :   appelants (ebp) : {}", sites.join(" ← "));
}

fn unhandled(rec: &[u8], _ctx: u32) -> ! {
    let n = get(rec, 16).min(15) as usize;
    let params: Vec<usize> = (0..n).map(|i| get(rec, 20 + i * 4) as usize).collect();
    crate::dlls::kernel32_misc::report_unhandled(get(rec, 0), get(rec, 4), &params)
}

/// Faute matérielle x86, hors contexte de signal, sur la pile du thread.
unsafe extern "C" fn hw_dispatch(frame: *mut HwFrame) -> ! {
    let f = &mut *frame;
    let rec = f.rec.as_ptr() as usize as u32;
    let ctx = f.ctx.as_ptr() as usize as u32;
    let code = get(&f.rec, 0);
    crate::trace!("wow32 : faute 0x{code:08x} à 0x{:x}", get(&f.rec, 12));
    trace_fault_state(&f.ctx);
    if dispatch(rec, ctx) {
        // Un gestionnaire a pu changer le contexte, y compris l'état SSE.
        let ext: [u8; 512] = f.ctx[CTX_EXTENDED..CTX_EXTENDED + 512].try_into().unwrap();
        f.fx.0 = ext;
        resume(ctx, chain_now(), Some(&f.fx))
    }
    // Personne : l'instruction est rejouée sans répartition, pour le
    // rapport de plantage complet (registres d'origine).
    super::set_refire(get(&f.orig, CTX_EIP));
    let orig = f.orig.as_ptr() as usize as u32;
    let ext: [u8; 512] = f.orig[CTX_EXTENDED..CTX_EXTENDED + 512].try_into().unwrap();
    f.fx.0 = ext;
    resume(orig, chain_now(), Some(&f.fx))
}

// ---------------------------------------------------------------------
// API x86
// ---------------------------------------------------------------------

/// `RaiseException(code, flags, nargs, args)`.
pub extern "win64" fn raise_exception(code: u32, flags: u32, nargs: u32, args: u32) {
    let mut rec = [0u8; EXCEPTION_RECORD32_SIZE];
    let n = if args == 0 { 0 } else { nargs.min(15) };
    let params: Vec<u32> = (0..n).map(|i| rd(args + i * 4)).collect();
    let site = super::caller_regs();
    fill_record(
        &mut rec,
        code,
        flags & EXCEPTION_NONCONTINUABLE,
        site.eip,
        &params,
    );
    let mut ctx = [0u8; CONTEXT32_SIZE];
    put(&mut ctx, CTX_FLAGS, CONTEXT_FULL);
    put(&mut ctx, CTX_EIP, site.eip);
    put(&mut ctx, CTX_ESP, site.esp);
    put(&mut ctx, CTX_EBP, site.ebp);
    put(&mut ctx, CTX_EBX, site.ebx);
    put(&mut ctx, CTX_ESI, site.esi);
    put(&mut ctx, CTX_EDI, site.edi);
    put(&mut ctx, CTX_SEGCS, super::CS32 as u32);
    put(&mut ctx, CTX_SEGSS, 0x2b);
    put(&mut ctx, CTX_EFLAGS, 0x202);
    crate::trace!(
        "wow32 : RaiseException 0x{code:08x} depuis 0x{:x}",
        site.eip
    );
    if code == DELPHI_EXCEPTION
        && params.len() >= 2
        && crate::log::enabled(crate::log::Level::Trace)
    {
        if let Some((class, message)) = delphi_exception(params[1]) {
            crate::trace!("wow32 :   exception Delphi {class} : « {message} »");
        }
    }
    let r = rec.as_ptr() as usize as u32;
    let c = ctx.as_ptr() as usize as u32;
    if !dispatch(r, c) {
        unhandled(&rec, c)
    }
    // Continuer : retour à l'appelant de RaiseException.
}

/// Code des exceptions du langage Delphi (et de la VCL de C++Builder).
const DELPHI_EXCEPTION: u32 = 0x0EED_FADE;

/// Lit `len` octets du processus sans risquer de faute (adresse invalide :
/// `None`).
fn read_bytes(addr: u32, len: usize) -> Option<Vec<u8>> {
    if addr < 0x1_0000 || len == 0 || len > 4096 {
        return None;
    }
    let mut out = vec![0u8; len];
    let local = libc::iovec {
        iov_base: out.as_mut_ptr().cast(),
        iov_len: len,
    };
    let remote = libc::iovec {
        iov_base: addr as usize as *mut libc::c_void,
        iov_len: len,
    };
    // SAFETY: process_vm_readv vérifie l'adresse et échoue proprement.
    let n = unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0) };
    (n == len as isize).then_some(out)
}

fn read_u32(addr: u32) -> Option<u32> {
    read_bytes(addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Nom de classe et message d'un objet exception Delphi (paramètre 1 de
/// l'exception 0x0EEDFADE), pour la trace : le nom est la ShortString
/// pointée par l'entrée vmtClassName de la VMT (-56 depuis Delphi 2009,
/// -44 avant), le message le champ FMessage juste après la VMT (chaîne
/// UTF-16 depuis Delphi 2009, octets avant). Documentation publique de
/// l'unité System de Delphi.
fn delphi_exception(object: u32) -> Option<(String, String)> {
    let vmt = read_u32(object)?;
    let class = [56u32, 44].iter().find_map(|off| {
        let name = read_u32(vmt.wrapping_sub(*off))?;
        let len = *read_bytes(name, 1)?.first()? as usize;
        let text = read_bytes(name.wrapping_add(1), len)?;
        let ok = len > 1
            && matches!(text[0], b'E' | b'T')
            && text.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_');
        ok.then(|| String::from_utf8_lossy(&text).into_owned())
    })?;
    let text = read_u32(object.wrapping_add(4)).filter(|p| *p != 0);
    let message = text
        .and_then(|p| {
            let len = (read_u32(p.wrapping_sub(4))? as usize).min(1024);
            let wide = read_bytes(p.wrapping_sub(10), 2).is_some_and(|b| b == [2, 0]);
            if wide {
                let b = read_bytes(p, len * 2)?;
                let units: Vec<u16> = b
                    .chunks(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else {
                Some(String::from_utf8_lossy(&read_bytes(p, len)?).into_owned())
            }
        })
        .unwrap_or_default();
    Some((class, message))
}

/// `RtlUnwind(TargetFrame, TargetIp, ExceptionRecord, ReturnValue)` : appelle
/// les gestionnaires des cadres au-dessus de `target` avec le drapeau de
/// déroulement et les retire de la chaîne. Rend `ret` à l'appelant.
pub extern "win64" fn rtl_unwind(target: u32, ip: u32, record: u32, ret: u32) -> u32 {
    crate::trace!(
        "wow32 : RtlUnwind(cadre 0x{target:x}, ip 0x{ip:x}, record 0x{record:x}) depuis 0x{:x}",
        super::caller_regs().eip
    );
    unwind_to(target, record);
    crate::trace!("wow32 : RtlUnwind vers 0x{target:x} terminé");
    ret
}

fn unwind_to(target: u32, record: u32) {
    let mut own = [0u8; EXCEPTION_RECORD32_SIZE];
    let flag = if target == 0 {
        EXCEPTION_UNWINDING | EXCEPTION_EXIT_UNWIND
    } else {
        EXCEPTION_UNWINDING
    };
    let rec = if record != 0 {
        wr(record + 4, rd(record + 4) | flag);
        record
    } else {
        fill_record(&mut own, STATUS_UNWIND, flag, 0, &[]);
        own.as_ptr() as usize as u32
    };
    let ctx = [0u8; CONTEXT32_SIZE];
    let c = ctx.as_ptr() as usize as u32;
    let mut dispatcher_context = 0u32;
    let dc = &mut dispatcher_context as *mut u32 as usize as u32;
    let teb = teb32();
    let mut frame = rd(teb);
    while frame != END_OF_CHAIN && frame != 0 && frame != target {
        if !frame_ok(frame) {
            break;
        }
        let handler = rd(frame + 4);
        crate::trace!("wow32 : déroulement, gestionnaire 0x{handler:x} (cadre 0x{frame:x})");
        call32(handler as usize, &[rec, frame, c, dc]);
        frame = rd(frame);
        wr(teb, frame);
    }
}

/// Déroule les cadres posés au-dessus de `frame` (longjmp).
pub(super) fn unwind_to_frame(frame: u32) {
    unwind_to(frame, 0);
}

/// Registres entiers d'une reprise x86 (longjmp).
pub(super) struct Regs32 {
    pub eip: u32,
    pub esp: u32,
    pub ebp: u32,
    pub ebx: u32,
    pub edi: u32,
    pub esi: u32,
    pub eax: u32,
}

/// Reprend le code x86 avec ces registres et la chaîne des appels 64 → 32
/// `chain`.
pub(super) fn resume_regs(r: &Regs32, chain: u64) -> ! {
    let mut ctx = [0u8; CONTEXT32_SIZE];
    put(&mut ctx, CTX_FLAGS, CONTEXT_FULL);
    put(&mut ctx, CTX_EIP, r.eip);
    put(&mut ctx, CTX_ESP, r.esp);
    put(&mut ctx, CTX_EBP, r.ebp);
    put(&mut ctx, CTX_EBX, r.ebx);
    put(&mut ctx, CTX_ESI, r.esi);
    put(&mut ctx, CTX_EDI, r.edi);
    put(&mut ctx, CTX_EAX, r.eax);
    put(&mut ctx, CTX_SEGCS, super::CS32 as u32);
    put(&mut ctx, CTX_SEGSS, 0x2b);
    put(&mut ctx, CTX_EFLAGS, 0x202);
    resume(ctx.as_ptr() as usize as u32, chain, None)
}

/// `SetUnhandledExceptionFilter(filter)` : rend le précédent.
pub extern "win64" fn set_unhandled_exception_filter(filter: u32) -> u32 {
    FILTER32.swap(filter, Ordering::Relaxed)
}

/// `UnhandledExceptionFilter(pointers)`.
pub extern "win64" fn unhandled_exception_filter(pointers: u32) -> i32 {
    let filter = FILTER32.load(Ordering::Relaxed);
    if filter != 0 {
        return call32(filter as usize, &[pointers]) as u32 as i32;
    }
    // EXCEPTION_EXECUTE_HANDLER : l'appelant (la CRT) termine le programme.
    1
}

// ---------------------------------------------------------------------
// Gestionnaires des blocs __try du C (MSVC et clang)
// ---------------------------------------------------------------------

/// Disposition d'une table de portées : EH3 (msvcrt, niveau « aucun » -1)
/// ou EH4 (vcruntime140, table chiffrée par le cookie, niveau aucun -2).
#[derive(Clone, Copy)]
struct Scopes {
    table: u32,
    none: u32,
}

impl Scopes {
    fn entry(&self, level: u32) -> (u32, u32, u32) {
        let e = self.table + level * 12;
        (rd(e), rd(e + 4), rd(e + 8))
    }
}

/// Appelle un filtre ou un bloc `__finally` : code de la fonction, qui
/// attend son ebp.
pub(super) fn call_with_ebp(code: u32, ebp: u32) -> u32 {
    call32(super::call_ebp_helper(), &[code, ebp]) as u32
}

/// Exécute les blocs `__finally` du cadre jusqu'au niveau `stop`.
fn local_unwind(frame: u32, scopes: Scopes, stop: u32) {
    let ebp = frame + 16;
    loop {
        let level = rd(frame + 12);
        if level == scopes.none || level == stop {
            break;
        }
        let (enclosing, filter, handler) = scopes.entry(level);
        wr(frame + 12, enclosing);
        if filter == 0 {
            call_with_ebp(handler, ebp);
        }
    }
}

fn frame_handler(rec: u32, frame: u32, ctx: u32, scopes: Scopes) -> u32 {
    if rd(rec + 4) & (EXCEPTION_UNWINDING | EXCEPTION_EXIT_UNWIND) != 0 {
        local_unwind(frame, scopes, scopes.none);
        return 1;
    }
    // EXCEPTION_POINTERS à ebp-0x14, lus par GetExceptionInformation().
    let pointers = [rec, ctx];
    wr(frame - 4, pointers.as_ptr() as usize as u32);
    let ebp = frame + 16;
    let mut level = rd(frame + 12);
    while level != scopes.none {
        let (enclosing, filter, handler) = scopes.entry(level);
        if filter != 0 {
            let r = call_with_ebp(filter, ebp) as i32;
            if r < 0 {
                // EXCEPTION_CONTINUE_EXECUTION
                return 0;
            }
            if r > 0 {
                // EXCEPTION_EXECUTE_HANDLER : déroulement global puis local,
                // et saut dans le bloc __except avec la pile de la fonction.
                unwind_to(frame, rec);
                local_unwind(frame, scopes, level);
                wr(frame + 12, enclosing);
                let mut target = [0u8; CONTEXT32_SIZE];
                put(&mut target, CTX_FLAGS, CONTEXT_FULL);
                put(&mut target, CTX_EIP, handler);
                put(&mut target, CTX_EBP, ebp);
                put(&mut target, CTX_ESP, rd(frame - 8));
                put(&mut target, CTX_EFLAGS, 0x202);
                transfer(target.as_ptr() as usize as u32)
            }
        }
        level = enclosing;
    }
    1
}

/// `_except_handler3(rec, frame, ctx, dispatcher)` (cdecl).
pub extern "win64" fn except_handler3(rec: u32, frame: u32, ctx: u32, _dc: u32) -> u32 {
    let scopes = Scopes {
        table: rd(frame + 8),
        none: u32::MAX,
    };
    frame_handler(rec, frame, ctx, scopes)
}

/// `_except_handler4_common(cookie, check, rec, frame, ctx, dispatcher)`
/// (cdecl) : la table des portées est chiffrée par le cookie et commence
/// par 16 octets de décalages de cookies (non vérifiés ici).
pub extern "win64" fn except_handler4_common(
    cookie: u32,
    _check: u32,
    rec: u32,
    frame: u32,
    ctx: u32,
    _dc: u32,
) -> u32 {
    let table = rd(frame + 8) ^ rd(cookie);
    let scopes = Scopes {
        table: table + 16,
        none: 0xffff_fffe,
    };
    frame_handler(rec, frame, ctx, scopes)
}

/// `_local_unwind2(frame, level)` (cdecl, msvcrt).
pub extern "win64" fn local_unwind2(frame: u32, stop: u32) {
    let scopes = Scopes {
        table: rd(frame + 8),
        none: u32::MAX,
    };
    local_unwind(frame, scopes, stop);
}

/// `_global_unwind2(frame)` (cdecl, msvcrt).
pub extern "win64" fn global_unwind2(frame: u32) {
    unwind_to(frame, 0);
}

/// `_local_unwind4(cookie, frame, level)` (cdecl, vcruntime140).
pub extern "win64" fn local_unwind4(cookie: u32, frame: u32, stop: u32) {
    let scopes = Scopes {
        table: (rd(frame + 8) ^ rd(cookie)) + 16,
        none: 0xffff_fffe,
    };
    local_unwind(frame, scopes, stop);
}

#[cfg(test)]
mod tests {
    /// Trace des exceptions Delphi : classe et message lus dans l'objet.
    #[test]
    fn nom_et_message_d_une_exception_delphi() {
        unsafe {
            let low = libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            ) as usize;
            assert_ne!(low as isize, -1);
            let base = low as u32;
            let put = |at: u32, v: u32| ((at as usize) as *mut u32).write_unaligned(v);
            // ShortString « EOSError » à +0x10.
            let name = b"\x08EOSError";
            std::ptr::copy_nonoverlapping(name.as_ptr(), (low + 0x10) as *mut u8, name.len());
            // VMT à +0x100, vmtClassName à -56.
            put(base + 0x100 - 56, base + 0x10);
            // Chaîne UTF-16 « Accès refusé » à +0x210 : élément 2 à -10, longueur à -4.
            let text: Vec<u16> = "Accès refusé".encode_utf16().collect();
            ((low + 0x210 - 10) as *mut u16).write_unaligned(2);
            put(base + 0x210 - 4, text.len() as u32);
            std::ptr::copy_nonoverlapping(text.as_ptr(), (low + 0x210) as *mut u16, text.len());
            // Objet à +0x300 : VMT puis FMessage.
            put(base + 0x300, base + 0x100);
            put(base + 0x304, base + 0x210);
            assert_eq!(
                delphi_exception(base + 0x300),
                Some(("EOSError".to_string(), "Accès refusé".to_string()))
            );
            assert_eq!(delphi_exception(0), None);
            libc::munmap(low as *mut libc::c_void, 4096);
        }
    }

    use super::*;

    #[test]
    fn dispositions_x86() {
        assert_eq!(CTX_EXTENDED + 512, CONTEXT32_SIZE);
        assert_eq!(std::mem::size_of::<HwFrame>() % 64, 0);
        let mut rec = [0u8; EXCEPTION_RECORD32_SIZE];
        fill_record(&mut rec, 0xE000_0001, 1, 0x40_1000, &[7, 8]);
        assert_eq!(get(&rec, 0), 0xE000_0001);
        assert_eq!(get(&rec, 12), 0x40_1000);
        assert_eq!(get(&rec, 16), 2);
        assert_eq!(get(&rec, 24), 8);
    }
}
