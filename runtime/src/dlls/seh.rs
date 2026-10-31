//! Gestion structurée des exceptions Windows x64 (SEH) : capture et
//! restauration de CONTEXT, déroulement par les tables `.pdata`/`.xdata`,
//! répartition d'une exception (gestionnaires vectorisés puis gestionnaires
//! de chaque cadre), `RtlUnwindEx` avec consolidation (utilisée par les
//! exceptions C++ MSVC) et tables de fonctions dynamiques (code généré par
//! un JIT, comme celui de coreclr).
//!
//! Le code de Winlux lui-même (Rust) n'a pas de tables Windows. Deux
//! trampolines en assembleur relient donc les cadres invités entre eux :
//! quand le déroulement atteint l'un d'eux, il reprend avec le CONTEXT
//! mémorisé pour ce trampoline (le point où l'exception a été levée, ou le
//! cadre cible d'une consolidation), comme le fait la trame machine de
//! `KiUserExceptionDispatcher` sous Windows.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::Mutex;

pub const CONTEXT_SIZE: usize = 1232;

pub const CTX_RAX: usize = 0x78;
pub const CTX_RSP: usize = 0x98;
pub const CTX_RIP: usize = 0xF8;
pub const CTX_XMM0: usize = 0x1A0;

/// Ordre des registres entiers dans les codes de déroulement (RAX=0…R15=15),
/// qui est aussi leur ordre dans CONTEXT.
fn gpr(reg: usize) -> usize {
    CTX_RAX + reg * 8
}

pub const EXCEPTION_NONCONTINUABLE: u32 = 0x1;
pub const EXCEPTION_UNWINDING: u32 = 0x2;
pub const EXCEPTION_EXIT_UNWIND: u32 = 0x4;
pub const EXCEPTION_TARGET_UNWIND: u32 = 0x20;
pub const EXCEPTION_COLLIDED_UNWIND: u32 = 0x40;

const STATUS_UNWIND: u32 = 0xC000_0027;
const STATUS_UNWIND_CONSOLIDATE: u32 = 0x8000_0029;
const STATUS_NONCONTINUABLE_EXCEPTION: u32 = 0xC000_0025;
const STATUS_INVALID_DISPOSITION: u32 = 0xC000_0026;

const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;

const DISPOSITION_CONTINUE_EXECUTION: u32 = 0;
const DISPOSITION_CONTINUE_SEARCH: u32 = 1;
const DISPOSITION_NESTED: u32 = 2;
const DISPOSITION_COLLIDED: u32 = 3;

const UNW_FLAG_NHANDLER: u32 = 0;
const UNW_FLAG_EHANDLER: u32 = 1;
const UNW_FLAG_UHANDLER: u32 = 2;
const UNW_FLAG_CHAININFO: u8 = 4;

/// CONTEXT x64 aligné sur 16 octets, comme l'exige Windows.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Context(pub [u8; CONTEXT_SIZE]);

impl Context {
    pub fn zeroed() -> Self {
        Context([0; CONTEXT_SIZE])
    }
    pub fn ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
    pub fn get(&self, off: usize) -> u64 {
        u64::from_le_bytes(self.0[off..off + 8].try_into().unwrap())
    }
    pub fn set(&mut self, off: usize, v: u64) {
        self.0[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExceptionRecord {
    pub code: u32,
    pub flags: u32,
    pub record: *mut ExceptionRecord,
    pub address: usize,
    pub nparams: u32,
    pub info: [usize; 15],
}

impl ExceptionRecord {
    pub fn new(code: u32, flags: u32, address: usize) -> Self {
        ExceptionRecord {
            code,
            flags,
            record: std::ptr::null_mut(),
            address,
            nparams: 0,
            info: [0; 15],
        }
    }
}

#[repr(C)]
pub struct DispatcherContext {
    pub control_pc: u64,
    pub image_base: u64,
    pub function_entry: *const c_void,
    pub establisher_frame: u64,
    pub target_ip: u64,
    pub context_record: *mut u8,
    pub language_handler: usize,
    pub handler_data: *mut c_void,
    pub history_table: *mut c_void,
    pub scope_index: u32,
    pub fill0: u32,
}

impl DispatcherContext {
    fn new(ctx: *mut u8) -> Self {
        DispatcherContext {
            control_pc: 0,
            image_base: 0,
            function_entry: std::ptr::null(),
            establisher_frame: 0,
            target_ip: 0,
            context_record: ctx,
            language_handler: 0,
            handler_data: std::ptr::null_mut(),
            history_table: std::ptr::null_mut(),
            scope_index: 0,
            fill0: 0,
        }
    }
}

// ============================================================
//  Capture et restauration du CONTEXT
// ============================================================

/// `RtlCaptureContext` : registres du code appelant, tels qu'ils sont au
/// moment de l'appel (RSP et RIP désignent le point de retour).
#[unsafe(naked)]
pub unsafe extern "win64" fn rtl_capture_context(_ctx: *mut c_void) {
    core::arch::naked_asm!(
        "mov [rcx + 0x78], rax",
        "mov [rcx + 0x80], rcx",
        "mov [rcx + 0x88], rdx",
        "mov [rcx + 0x90], rbx",
        "mov [rcx + 0xA0], rbp",
        "mov [rcx + 0xA8], rsi",
        "mov [rcx + 0xB0], rdi",
        "mov [rcx + 0xB8], r8",
        "mov [rcx + 0xC0], r9",
        "mov [rcx + 0xC8], r10",
        "mov [rcx + 0xD0], r11",
        "mov [rcx + 0xD8], r12",
        "mov [rcx + 0xE0], r13",
        "mov [rcx + 0xE8], r14",
        "mov [rcx + 0xF0], r15",
        "lea rax, [rsp + 8]",
        "mov [rcx + 0x98], rax",
        "mov rax, [rsp]",
        "mov [rcx + 0xF8], rax",
        "pushfq",
        "pop rax",
        "mov [rcx + 0x44], eax",
        "mov word ptr [rcx + 0x38], cs",
        "mov word ptr [rcx + 0x3A], ds",
        "mov word ptr [rcx + 0x3C], es",
        "mov word ptr [rcx + 0x3E], fs",
        "mov word ptr [rcx + 0x40], gs",
        "mov word ptr [rcx + 0x42], ss",
        "stmxcsr [rcx + 0x34]",
        "stmxcsr [rcx + 0x118]",
        "fnstcw [rcx + 0x100]",
        "movdqu [rcx + 0x1A0], xmm0",
        "movdqu [rcx + 0x1B0], xmm1",
        "movdqu [rcx + 0x1C0], xmm2",
        "movdqu [rcx + 0x1D0], xmm3",
        "movdqu [rcx + 0x1E0], xmm4",
        "movdqu [rcx + 0x1F0], xmm5",
        "movdqu [rcx + 0x200], xmm6",
        "movdqu [rcx + 0x210], xmm7",
        "movdqu [rcx + 0x220], xmm8",
        "movdqu [rcx + 0x230], xmm9",
        "movdqu [rcx + 0x240], xmm10",
        "movdqu [rcx + 0x250], xmm11",
        "movdqu [rcx + 0x260], xmm12",
        "movdqu [rcx + 0x270], xmm13",
        "movdqu [rcx + 0x280], xmm14",
        "movdqu [rcx + 0x290], xmm15",
        // CONTEXT_AMD64 | CONTROL | INTEGER | SEGMENTS | FLOATING_POINT
        "mov dword ptr [rcx + 0x30], 0x10000F",
        "mov rax, [rcx + 0x78]",
        "ret",
    )
}

/// Charge tous les registres depuis `ctx` et saute à son RIP.
#[unsafe(naked)]
unsafe extern "win64" fn load_context(_ctx: *const u8) -> ! {
    core::arch::naked_asm!(
        "movdqu xmm0, [rcx + 0x1A0]",
        "movdqu xmm1, [rcx + 0x1B0]",
        "movdqu xmm2, [rcx + 0x1C0]",
        "movdqu xmm3, [rcx + 0x1D0]",
        "movdqu xmm4, [rcx + 0x1E0]",
        "movdqu xmm5, [rcx + 0x1F0]",
        "movdqu xmm6, [rcx + 0x200]",
        "movdqu xmm7, [rcx + 0x210]",
        "movdqu xmm8, [rcx + 0x220]",
        "movdqu xmm9, [rcx + 0x230]",
        "movdqu xmm10, [rcx + 0x240]",
        "movdqu xmm11, [rcx + 0x250]",
        "movdqu xmm12, [rcx + 0x260]",
        "movdqu xmm13, [rcx + 0x270]",
        "movdqu xmm14, [rcx + 0x280]",
        "movdqu xmm15, [rcx + 0x290]",
        "ldmxcsr [rcx + 0x34]",
        // RIP et EFLAGS sont posés juste sous la pile cible.
        "mov rax, [rcx + 0x98]",
        "sub rax, 16",
        "mov rdx, [rcx + 0xF8]",
        "mov [rax + 8], rdx",
        "mov edx, [rcx + 0x44]",
        "mov [rax], rdx",
        "mov rdx, [rcx + 0x88]",
        "mov rbx, [rcx + 0x90]",
        "mov rbp, [rcx + 0xA0]",
        "mov rsi, [rcx + 0xA8]",
        "mov rdi, [rcx + 0xB0]",
        "mov r8, [rcx + 0xB8]",
        "mov r9, [rcx + 0xC0]",
        "mov r10, [rcx + 0xC8]",
        "mov r11, [rcx + 0xD0]",
        "mov r12, [rcx + 0xD8]",
        "mov r13, [rcx + 0xE0]",
        "mov r14, [rcx + 0xE8]",
        "mov r15, [rcx + 0xF0]",
        "mov rsp, rax",
        "mov rax, [rcx + 0x78]",
        "mov rcx, [rcx + 0x80]",
        "popfq",
        "ret",
    )
}

/// `RtlRestoreContext` : reprend l'exécution dans `ctx`. Pour une
/// consolidation (STATUS_UNWIND_CONSOLIDATE), la fonction de rappel du
/// record s'exécute d'abord et fournit l'adresse de reprise.
pub unsafe extern "win64" fn rtl_restore_context(ctx: *mut u8, rec: *mut ExceptionRecord) -> ! {
    let mut target = Context::zeroed();
    std::ptr::copy_nonoverlapping(ctx, target.ptr(), CONTEXT_SIZE);
    if !rec.is_null() && (*rec).code == STATUS_UNWIND_CONSOLIDATE && (*rec).nparams >= 1 {
        let callback = (*rec).info[0];
        let mut slot = 0u64;
        let index = push_frame(FrameKind::Consolidate, target);
        let rip = call_consolidate(callback, rec, frame_slot_ptr(index, &mut slot));
        pop_frames_from(index);
        target.set(CTX_RIP, rip);
    }
    resume(&target)
}

unsafe fn resume(ctx: &Context) -> ! {
    // Les cadres de Winlux abandonnés par ce saut ne sont plus déroulables.
    let rsp = ctx.get(CTX_RSP);
    FRAMES.with(|f| f.borrow_mut().retain(|e| e.rsp >= rsp));
    load_context(ctx.0.as_ptr())
}

// ============================================================
//  Trampolines reliant les cadres invités
// ============================================================

#[derive(Clone, Copy, PartialEq)]
enum FrameKind {
    /// Appel d'un gestionnaire pendant la répartition : le déroulement
    /// reprend au point où l'exception a été levée.
    Handler,
    /// Rappel de consolidation : le déroulement reprend dans le cadre cible.
    Consolidate,
}

struct Frame {
    kind: FrameKind,
    /// Valeur de RSP juste avant l'appel, renseignée par le trampoline.
    rsp: u64,
    resume: Context,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

fn push_frame(kind: FrameKind, resume: Context) -> usize {
    FRAMES.with(|f| {
        let mut f = f.borrow_mut();
        f.push(Frame {
            kind,
            rsp: 0,
            resume,
        });
        f.len() - 1
    })
}
fn pop_frames_from(index: usize) {
    FRAMES.with(|f| f.borrow_mut().truncate(index));
}

/// Appelle `handler(rec, frame, ctx, dispatch)` et écrit dans `*rsp_slot`
/// la valeur de RSP au moment de l'appel.
#[unsafe(naked)]
unsafe extern "win64" fn call_handler_tramp(
    _rec: *mut ExceptionRecord,
    _frame: u64,
    _ctx: *mut u8,
    _dispatch: *mut DispatcherContext,
    _handler: usize,
    _rsp_slot: *mut u64,
) -> u32 {
    core::arch::naked_asm!(
        "push rbp",
        "mov rbp, rsp",
        "sub rsp, 0x20",
        "mov rax, [rbp + 0x38]",
        "mov [rax], rsp",
        "mov rax, [rbp + 0x30]",
        "call rax",
        ".globl winlux_seh_handler_ret",
        "winlux_seh_handler_ret:",
        "mov rsp, rbp",
        "pop rbp",
        "ret",
    )
}

/// Appelle le rappel de consolidation `callback(rec)` ; il renvoie l'adresse
/// où reprendre dans le cadre cible.
#[unsafe(naked)]
unsafe extern "win64" fn call_consolidate(
    _callback: usize,
    _rec: *mut ExceptionRecord,
    _rsp_slot: *mut u64,
) -> u64 {
    core::arch::naked_asm!(
        "push rbp",
        "mov rbp, rsp",
        "sub rsp, 0x20",
        "mov [r8], rsp",
        "mov rax, rcx",
        "mov rcx, rdx",
        "call rax",
        ".globl winlux_seh_consolidate_ret",
        "winlux_seh_consolidate_ret:",
        "mov rsp, rbp",
        "pop rbp",
        "ret",
    )
}

extern "C" {
    static winlux_seh_handler_ret: u8;
    static winlux_seh_consolidate_ret: u8;
}

/// Si `ctx` est arrêté au retour d'un trampoline, renvoie le CONTEXT à
/// partir duquel poursuivre le déroulement.
fn trampoline_resume(ctx: &Context) -> Option<Context> {
    let rip = ctx.get(CTX_RIP) as usize;
    let rsp = ctx.get(CTX_RSP);
    let kind = {
        if rip == std::ptr::addr_of!(winlux_seh_handler_ret) as usize {
            FrameKind::Handler
        } else if rip == std::ptr::addr_of!(winlux_seh_consolidate_ret) as usize {
            FrameKind::Consolidate
        } else {
            return None;
        }
    };
    FRAMES.with(|f| {
        f.borrow()
            .iter()
            .rev()
            .find(|e| e.kind == kind && e.rsp == rsp)
            .map(|e| e.resume)
    })
}

// ============================================================
//  Tables de fonctions dynamiques (code JIT)
// ============================================================

enum DynamicTable {
    Static {
        entries: usize,
        count: u32,
        base: u64,
    },
    Callback {
        id: u64,
        base: u64,
        length: u32,
        callback: usize,
        context: usize,
    },
}

static DYNAMIC: Mutex<Vec<DynamicTable>> = Mutex::new(Vec::new());

/// `RtlAddFunctionTable` : RUNTIME_FUNCTION d'un code généré à l'exécution.
pub unsafe extern "win64" fn rtl_add_function_table(entries: usize, count: u32, base: u64) -> u8 {
    if entries == 0 {
        return 0;
    }
    crate::dlls::lock(&DYNAMIC).push(DynamicTable::Static {
        entries,
        count,
        base,
    });
    1
}
pub unsafe extern "win64" fn rtl_delete_function_table(entries: usize) -> u8 {
    let mut t = crate::dlls::lock(&DYNAMIC);
    let before = t.len();
    t.retain(|e| match e {
        DynamicTable::Static { entries: x, .. } => *x != entries,
        DynamicTable::Callback { id, .. } => *id as usize != entries,
    });
    (t.len() != before) as u8
}
/// `RtlInstallFunctionTableCallback` : les entrées de [base, base+length)
/// sont fournies à la demande par `callback(pc, context)`.
pub unsafe extern "win64" fn rtl_install_function_table_callback(
    id: u64,
    base: u64,
    length: u32,
    callback: usize,
    context: usize,
    _dll: *const u16,
) -> u8 {
    // L'identifiant doit avoir ses deux bits de poids faible à 1.
    if id & 3 != 3 || callback == 0 {
        return 0;
    }
    crate::dlls::lock(&DYNAMIC).push(DynamicTable::Callback {
        id,
        base,
        length,
        callback,
        context,
    });
    1
}

/// Cherche `pc` dans les tables dynamiques. Renvoie (entrée, base) ; une
/// entrée nulle signifie « code dynamique connu, fonction feuille ».
unsafe fn lookup_dynamic(pc: u64) -> Option<(*const c_void, u64)> {
    enum Hit {
        Found(*const c_void, u64),
        Ask(usize, usize, u64),
    }
    let hit = {
        let tables = crate::dlls::lock(&DYNAMIC);
        let mut hit = None;
        for t in tables.iter().rev() {
            match *t {
                DynamicTable::Static {
                    entries,
                    count,
                    base,
                } => {
                    for i in 0..count as usize {
                        let e = (entries + i * 12) as *const u32;
                        let begin = base + e.read_unaligned() as u64;
                        let end = base + e.add(1).read_unaligned() as u64;
                        if pc >= begin && pc < end {
                            hit = Some(Hit::Found(e.cast(), base));
                            break;
                        }
                    }
                }
                DynamicTable::Callback {
                    base,
                    length,
                    callback,
                    context,
                    ..
                } => {
                    if pc >= base && pc < base + length as u64 {
                        hit = Some(Hit::Ask(callback, context, base));
                    }
                }
            }
            if hit.is_some() {
                break;
            }
        }
        hit
    };
    match hit? {
        Hit::Found(e, base) => Some((e, base)),
        Hit::Ask(callback, context, base) => {
            // Rappel hors du verrou : il peut lui-même consulter les tables.
            let f: unsafe extern "win64" fn(u64, usize) -> *const c_void =
                std::mem::transmute(callback);
            Some((f(pc, context), base))
        }
    }
}

// ============================================================
//  Recherche de RUNTIME_FUNCTION et déroulement virtuel
// ============================================================

/// `RtlLookupFunctionEntry` : cherche dans le `.pdata` du module qui contient
/// `pc`, puis dans les tables dynamiques.
pub unsafe extern "win64" fn rtl_lookup_function_entry(
    pc: u64,
    image_base: *mut u64,
    _history: *mut c_void,
) -> *mut c_void {
    match lookup_entry(pc) {
        Some((entry, base)) if !entry.is_null() => {
            if !image_base.is_null() {
                *image_base = base;
            }
            entry as *mut c_void
        }
        _ => std::ptr::null_mut(),
    }
}

/// Résultat de recherche : None si `pc` n'appartient à aucun code invité
/// connu ; Some((nul, base)) pour une fonction feuille sans entrée.
unsafe fn lookup_entry(pc: u64) -> Option<(*const c_void, u64)> {
    let addr = pc as usize;
    {
        let modules = crate::dlls::lock(&crate::loader::MODULES);
        if let Some(module) = modules.iter().find(|m| m.contains(addr)) {
            let base = module.base;
            let rva = (addr - base) as u32;
            let Some(dir) = module.pe.dirs.get(3).filter(|d| d.rva != 0 && d.size >= 12) else {
                return Some((std::ptr::null(), base as u64));
            };
            let table = base + dir.rva as usize;
            let (mut lo, mut hi) = (0usize, (dir.size / 12) as usize);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let e = (table + mid * 12) as *const u32;
                if rva < e.read_unaligned() {
                    hi = mid;
                } else if rva >= e.add(1).read_unaligned() {
                    lo = mid + 1;
                } else {
                    return Some((e.cast(), base as u64));
                }
            }
            return Some((std::ptr::null(), base as u64));
        }
    }
    lookup_dynamic(pc)
}

fn opcode_slots(op: u8, info: u8) -> usize {
    // Codes UWOP_* : 0 PUSH_NONVOL, 1 ALLOC_LARGE, 2 ALLOC_SMALL, 3 SET_FPREG,
    // 4 SAVE_NONVOL, 5 SAVE_NONVOL_FAR, 6 EPILOG (version 2), 7 SPARE,
    // 8 SAVE_XMM128, 9 SAVE_XMM128_FAR, 10 PUSH_MACHFRAME.
    match op {
        1 => {
            if info == 0 {
                2
            } else {
                3
            }
        }
        4 | 6 | 8 => 2,
        5 | 9 => 3,
        _ => 1,
    }
}

/// Repère un épilogue standard à `pc` (add/lea rsp, pop…, ret ou jmp) ; le
/// cas échéant, l'exécute virtuellement sur `ctx`.
unsafe fn unwind_epilog(pc: u64, base: u64, entry: *const u32, ctx: &mut Context) -> bool {
    let begin = base + entry.read_unaligned() as u64;
    let end = base + entry.add(1).read_unaligned() as u64;
    let mut p = pc as *const u8;
    let at = |p: *const u8, i: usize| *p.add(i);
    let mut rsp_adjust: Option<u64> = None;
    // add rsp, imm8 / imm32
    if at(p, 0) == 0x48 && at(p, 1) == 0x83 && at(p, 2) == 0xC4 {
        rsp_adjust = Some(ctx.get(CTX_RSP) + at(p, 3) as u64);
        p = p.add(4);
    } else if at(p, 0) == 0x48 && at(p, 1) == 0x81 && at(p, 2) == 0xC4 {
        rsp_adjust = Some(ctx.get(CTX_RSP) + (p.add(3) as *const u32).read_unaligned() as u64);
        p = p.add(7);
    } else if (at(p, 0) & 0xFE) == 0x48 && at(p, 1) == 0x8D && (at(p, 2) & 0xF8) == 0x60 {
        // lea rsp, [reg + disp8]
        let reg = (at(p, 2) & 7) as usize + if at(p, 0) & 1 != 0 { 8 } else { 0 };
        rsp_adjust = Some(ctx.get(gpr(reg)).wrapping_add(at(p, 3) as i8 as i64 as u64));
        p = p.add(4);
    } else if (at(p, 0) & 0xFE) == 0x48 && at(p, 1) == 0x8D && (at(p, 2) & 0xF8) == 0xA0 {
        let reg = (at(p, 2) & 7) as usize + if at(p, 0) & 1 != 0 { 8 } else { 0 };
        let d = (p.add(3) as *const i32).read_unaligned() as i64 as u64;
        rsp_adjust = Some(ctx.get(gpr(reg)).wrapping_add(d));
        p = p.add(7);
    }
    // Vérifier que la suite n'est que des pop puis ret/jmp avant de modifier.
    let mut q = p;
    loop {
        let b = at(q, 0);
        if (0x58..=0x5F).contains(&b) {
            q = q.add(1);
        } else if b == 0x41 && (0x58..=0x5F).contains(&at(q, 1)) {
            q = q.add(2);
        } else {
            break;
        }
    }
    let b = at(q, 0);
    let is_ret = b == 0xC3 || b == 0xC2 || (b == 0xF3 && at(q, 1) == 0xC3);
    let is_tail = b == 0xE9 || b == 0xEB || (b == 0xFF && at(q, 1) == 0x25);
    if !is_ret && !(is_tail && rsp_adjust.is_some() || is_tail && q != p) {
        return false;
    }
    if is_tail {
        // Un saut sortant de la fonction est un appel terminal ; à l'intérieur
        // ce n'est pas un épilogue.
        let target = match b {
            0xE9 => {
                (q as u64 + 5).wrapping_add((q.add(1) as *const i32).read_unaligned() as i64 as u64)
            }
            0xEB => (q as u64 + 2).wrapping_add(at(q, 1) as i8 as i64 as u64),
            _ => 0,
        };
        if b != 0xFF && target >= begin && target < end {
            return false;
        }
    }
    if let Some(rsp) = rsp_adjust {
        ctx.set(CTX_RSP, rsp);
    }
    let mut q = p;
    loop {
        let b = at(q, 0);
        let reg = if (0x58..=0x5F).contains(&b) {
            q = q.add(1);
            (b - 0x58) as usize
        } else if b == 0x41 && (0x58..=0x5F).contains(&at(q, 1)) {
            let r = (at(q, 1) - 0x58) as usize + 8;
            q = q.add(2);
            r
        } else {
            break;
        };
        let rsp = ctx.get(CTX_RSP);
        ctx.set(gpr(reg), (rsp as *const u64).read_unaligned());
        ctx.set(CTX_RSP, rsp + 8);
    }
    let rsp = ctx.get(CTX_RSP);
    ctx.set(CTX_RIP, (rsp as *const u64).read_unaligned());
    ctx.set(CTX_RSP, rsp + 8);
    true
}

/// `RtlVirtualUnwind` : déroule un cadre de `ctx` vers celui de l'appelant
/// selon les codes de déroulement (mêmes règles que Windows : cadre
/// d'établissement, prologue partiel, épilogue, informations chaînées).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn rtl_virtual_unwind(
    handler_type: u32,
    base: u64,
    pc: u64,
    entry: *const c_void,
    ctx_raw: *mut c_void,
    handler_data: *mut *mut c_void,
    establisher_frame: *mut u64,
    _ctx_pointers: *mut c_void,
) -> *mut c_void {
    let mut ctx = Context::zeroed();
    std::ptr::copy_nonoverlapping(ctx_raw as *const u8, ctx.ptr(), CONTEXT_SIZE);
    let (handler, data, frame) =
        virtual_unwind(handler_type, base, pc, entry as *const u32, &mut ctx);
    std::ptr::copy_nonoverlapping(ctx.0.as_ptr(), ctx_raw as *mut u8, CONTEXT_SIZE);
    if !handler_data.is_null() {
        *handler_data = data;
    }
    if !establisher_frame.is_null() {
        *establisher_frame = frame;
    }
    handler as *mut c_void
}

unsafe fn virtual_unwind(
    handler_type: u32,
    base: u64,
    pc: u64,
    mut entry: *const u32,
    ctx: &mut Context,
) -> (usize, *mut c_void, u64) {
    let first = entry;
    let mut info = (base + entry.add(2).read_unaligned() as u64) as *const u8;
    let mut prolog_offset = pc.wrapping_sub(base + entry.read_unaligned() as u64);
    let flags0 = *info >> 3;
    let prolog_size = *info.add(1) as u64;
    let frame_reg = (*info.add(3) & 0xF) as usize;
    let frame_off = ((*info.add(3) >> 4) as u64) * 16;

    // Cadre d'établissement : RSP, ou le registre de cadre une fois posé.
    let mut frame = ctx.get(CTX_RSP);
    if frame_reg != 0 {
        let count = *info.add(2) as usize;
        let codes = info.add(4);
        let mut fp_set = prolog_offset >= prolog_size;
        let mut i = 0;
        while i < count {
            let (off, op) = (*codes.add(i * 2), *codes.add(i * 2 + 1) & 0xF);
            if op == 3 && prolog_offset >= off as u64 {
                fp_set = true;
            }
            i += opcode_slots(op, *codes.add(i * 2 + 1) >> 4);
        }
        if fp_set {
            frame = ctx.get(gpr(frame_reg)).wrapping_sub(frame_off);
        }
    }

    if prolog_offset >= prolog_size && unwind_epilog(pc, base, first, ctx) {
        return (0, std::ptr::null_mut(), frame);
    }

    let mut mach_frame = false;
    loop {
        let count = *info.add(2) as usize;
        let codes = info.add(4);
        let mut i = 0;
        while i < count {
            let off = *codes.add(i * 2) as u64;
            let op = *codes.add(i * 2 + 1) & 0xF;
            let reg = (*codes.add(i * 2 + 1) >> 4) as usize;
            let slots = opcode_slots(op, reg as u8);
            // Les codes d'épilogue (version 2) ne décrivent pas le prologue.
            if op == 6 || op == 7 || prolog_offset < off {
                i += slots;
                continue;
            }
            let u16_at = |k: usize| (codes.add((i + k) * 2) as *const u16).read_unaligned() as u64;
            match op {
                0 => {
                    // UWOP_PUSH_NONVOL
                    let rsp = ctx.get(CTX_RSP);
                    ctx.set(gpr(reg), (rsp as *const u64).read_unaligned());
                    ctx.set(CTX_RSP, rsp + 8);
                }
                1 => {
                    // UWOP_ALLOC_LARGE
                    let size = if reg == 0 {
                        u16_at(1) * 8
                    } else {
                        u16_at(1) | (u16_at(2) << 16)
                    };
                    ctx.set(CTX_RSP, ctx.get(CTX_RSP) + size);
                }
                2 => ctx.set(CTX_RSP, ctx.get(CTX_RSP) + (reg as u64 + 1) * 8),
                3 => ctx.set(CTX_RSP, frame),
                4 => {
                    let v = ((frame + u16_at(1) * 8) as *const u64).read_unaligned();
                    ctx.set(gpr(reg), v);
                }
                5 => {
                    let v =
                        ((frame + (u16_at(1) | (u16_at(2) << 16))) as *const u64).read_unaligned();
                    ctx.set(gpr(reg), v);
                }
                8 | 9 => {
                    // UWOP_SAVE_XMM128 / UWOP_SAVE_XMM128_FAR
                    let addr = if op == 8 {
                        frame + u16_at(1) * 16
                    } else {
                        frame + (u16_at(1) | (u16_at(2) << 16))
                    };
                    let src = std::slice::from_raw_parts(addr as *const u8, 16);
                    let at = CTX_XMM0 + reg * 16;
                    ctx.0[at..at + 16].copy_from_slice(src);
                }
                10 => {
                    // UWOP_PUSH_MACHFRAME
                    let mut rsp = ctx.get(CTX_RSP);
                    if reg != 0 {
                        rsp += 8;
                    }
                    ctx.set(CTX_RIP, (rsp as *const u64).read_unaligned());
                    ctx.set(CTX_RSP, ((rsp + 24) as *const u64).read_unaligned());
                    mach_frame = true;
                }
                _ => {}
            }
            i += slots;
        }
        if (*info >> 3) & UNW_FLAG_CHAININFO == 0 {
            break;
        }
        let aligned = (count + 1) & !1;
        entry = codes.add(aligned * 2) as *const u32;
        info = (base + entry.add(2).read_unaligned() as u64) as *const u8;
        // Les informations chaînées s'appliquent en entier.
        prolog_offset = u64::MAX;
    }

    if !mach_frame {
        let rsp = ctx.get(CTX_RSP);
        ctx.set(CTX_RIP, (rsp as *const u64).read_unaligned());
        ctx.set(CTX_RSP, rsp + 8);
    }

    let first_info = (base + first.add(2).read_unaligned() as u64) as *const u8;
    let wanted = (handler_type & (flags0 as u32)) != 0;
    let in_prolog = pc.wrapping_sub(base + first.read_unaligned() as u64) < prolog_size;
    if !wanted || in_prolog || flags0 & UNW_FLAG_CHAININFO != 0 {
        return (0, std::ptr::null_mut(), frame);
    }
    let count = *first_info.add(2) as usize;
    let handler_rva = first_info.add(4 + ((count + 1) & !1) * 2) as *const u32;
    (
        (base + handler_rva.read_unaligned() as u64) as usize,
        handler_rva.add(1) as *mut c_void,
        frame,
    )
}

/// Un pas de déroulement pour le répartiteur : renseigne `dispatch` et
/// remonte `ctx` d'un cadre. Renvoie false en fin de pile.
unsafe fn step(handler_type: u32, dispatch: &mut DispatcherContext, ctx: &mut Context) -> bool {
    dispatch.image_base = 0;
    dispatch.scope_index = 0;
    dispatch.control_pc = ctx.get(CTX_RIP);
    dispatch.language_handler = 0;
    dispatch.handler_data = std::ptr::null_mut();
    let pc = ctx.get(CTX_RIP);
    if pc == 0 {
        dispatch.establisher_frame = 0;
        return false;
    }
    match lookup_entry(pc) {
        Some((entry, base)) if !entry.is_null() => {
            dispatch.image_base = base;
            dispatch.function_entry = entry;
            let (handler, data, frame) = virtual_unwind(handler_type, base, pc, entry.cast(), ctx);
            dispatch.language_handler = handler;
            dispatch.handler_data = data;
            dispatch.establisher_frame = frame;
            true
        }
        Some(_) => {
            // Fonction feuille d'un module invité.
            dispatch.function_entry = std::ptr::null();
            let rsp = ctx.get(CTX_RSP);
            dispatch.establisher_frame = rsp;
            ctx.set(CTX_RIP, (rsp as *const u64).read_unaligned());
            ctx.set(CTX_RSP, rsp + 8);
            true
        }
        None => match trampoline_resume(ctx) {
            Some(next) => {
                dispatch.function_entry = std::ptr::null();
                dispatch.establisher_frame = ctx.get(CTX_RSP);
                *ctx = next;
                true
            }
            None => {
                dispatch.establisher_frame = 0;
                false
            }
        },
    }
}

// ============================================================
//  Gestionnaires vectorisés
// ============================================================

static VEH: Mutex<Vec<(usize, usize, bool)>> = Mutex::new(Vec::new());
static NEXT_VEH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x7E00_0001);

fn add_vectored(first: u32, handler: usize, continue_handler: bool) -> usize {
    if handler == 0 {
        return 0;
    }
    let id = NEXT_VEH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut list = crate::dlls::lock(&VEH);
    let entry = (id, handler, continue_handler);
    if first != 0 {
        list.insert(0, entry);
    } else {
        list.push(entry);
    }
    id
}
fn remove_vectored(handle: usize, continue_handler: bool) -> u32 {
    let mut list = crate::dlls::lock(&VEH);
    match list
        .iter()
        .position(|&(id, _, c)| id == handle && c == continue_handler)
    {
        Some(i) => {
            list.remove(i);
            1
        }
        None => 0,
    }
}
pub extern "win64" fn rtl_add_vectored_exception_handler(first: u32, handler: usize) -> usize {
    add_vectored(first, handler, false)
}
pub extern "win64" fn rtl_remove_vectored_exception_handler(handle: usize) -> u32 {
    remove_vectored(handle, false)
}
pub extern "win64" fn rtl_add_vectored_continue_handler(first: u32, handler: usize) -> usize {
    add_vectored(first, handler, true)
}
pub extern "win64" fn rtl_remove_vectored_continue_handler(handle: usize) -> u32 {
    remove_vectored(handle, true)
}

#[repr(C)]
struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut u8,
}

unsafe fn call_vectored(rec: *mut ExceptionRecord, ctx: *mut u8, continue_handlers: bool) -> bool {
    let handlers: Vec<usize> = crate::dlls::lock(&VEH)
        .iter()
        .filter(|e| e.2 == continue_handlers)
        .map(|e| e.1)
        .collect();
    let mut ptrs = ExceptionPointers {
        record: rec,
        context: ctx,
    };
    for h in handlers {
        let f: unsafe extern "win64" fn(*mut ExceptionPointers) -> i32 = std::mem::transmute(h);
        if f(&mut ptrs) == EXCEPTION_CONTINUE_EXECUTION {
            return true;
        }
    }
    false
}

// ============================================================
//  Répartition
// ============================================================

/// Résultat de `dispatch` quand aucun gestionnaire n'a repris la main.
pub enum Unhandled {
    NotHandled,
    Noncontinuable,
    InvalidDisposition,
}

/// `RtlDispatchException` : gestionnaires vectorisés, puis gestionnaires de
/// chaque cadre. Ne revient que si l'exception n'est pas gérée ; sinon
/// l'exécution reprend dans le CONTEXT (éventuellement modifié).
pub unsafe fn dispatch(rec: *mut ExceptionRecord, orig: &mut Context) -> Unhandled {
    if call_vectored(rec, orig.ptr(), false) {
        call_vectored(rec, orig.ptr(), true);
        resume(orig);
    }
    let mut ctx = *orig;
    let mut dc = DispatcherContext::new(ctx.ptr());
    loop {
        dc.context_record = ctx.ptr();
        if !step(UNW_FLAG_EHANDLER, &mut dc, &mut ctx) {
            break;
        }
        'retry: loop {
            if dc.establisher_frame == 0 {
                return Unhandled::NotHandled;
            }
            if dc.language_handler == 0 {
                break 'retry;
            }
            let index = push_frame(FrameKind::Handler, *orig);
            let mut slot = 0u64;
            let (handler, frame) = (dc.language_handler, dc.establisher_frame);
            let r = call_handler_tramp(
                rec,
                frame,
                orig.ptr(),
                &mut dc,
                handler,
                frame_slot_ptr(index, &mut slot),
            );
            pop_frames_from(index);
            match r {
                DISPOSITION_CONTINUE_EXECUTION => {
                    if (*rec).flags & EXCEPTION_NONCONTINUABLE != 0 {
                        return Unhandled::Noncontinuable;
                    }
                    call_vectored(rec, orig.ptr(), true);
                    resume(orig);
                }
                DISPOSITION_CONTINUE_SEARCH | DISPOSITION_NESTED => break 'retry,
                DISPOSITION_COLLIDED => {
                    // Le gestionnaire a fourni, dans dc, le cadre où reprendre.
                    let mut next = Context::zeroed();
                    std::ptr::copy_nonoverlapping(dc.context_record, next.ptr(), CONTEXT_SIZE);
                    ctx = next;
                    let mut frame = 0;
                    virtual_unwind_raw(UNW_FLAG_NHANDLER, &dc, &mut ctx, &mut frame);
                    dc.establisher_frame = frame;
                    dc.language_handler = 0;
                    continue 'retry;
                }
                _ => return Unhandled::InvalidDisposition,
            }
        }
    }
    Unhandled::NotHandled
}

/// Pointeur où le trampoline écrit RSP : directement dans l'entrée de la
/// pile de cadres (stable tant qu'aucune autre entrée n'est ajoutée avant
/// l'appel, ce qui est le cas).
fn frame_slot_ptr(index: usize, fallback: &mut u64) -> *mut u64 {
    FRAMES.with(|f| {
        f.borrow_mut()
            .get_mut(index)
            .map(|e| &mut e.rsp as *mut u64)
            .unwrap_or(fallback as *mut u64)
    })
}

unsafe fn virtual_unwind_raw(
    handler_type: u32,
    dc: &DispatcherContext,
    ctx: &mut Context,
    frame: &mut u64,
) {
    if dc.function_entry.is_null() {
        return;
    }
    let (_, _, f) = virtual_unwind(
        handler_type,
        dc.image_base,
        dc.control_pc,
        dc.function_entry.cast(),
        ctx,
    );
    *frame = f;
}

/// Lève une exception logicielle depuis le CONTEXT de l'appelant.
pub unsafe fn raise(rec: &mut ExceptionRecord, ctx: &mut Context) -> ! {
    crate::trace!(
        "exception {:#010x} levée à {} (arguments {:x?})",
        rec.code,
        crate::thunks::describe_site(rec.address),
        &rec.info[..(rec.nparams as usize).min(4)]
    );
    match dispatch(rec, ctx) {
        Unhandled::Noncontinuable => {
            let mut nested = ExceptionRecord::new(
                STATUS_NONCONTINUABLE_EXCEPTION,
                EXCEPTION_NONCONTINUABLE,
                rec.address,
            );
            nested.record = rec;
            unhandled(&nested, ctx)
        }
        Unhandled::InvalidDisposition => {
            let mut nested = ExceptionRecord::new(
                STATUS_INVALID_DISPOSITION,
                EXCEPTION_NONCONTINUABLE,
                rec.address,
            );
            nested.record = rec;
            unhandled(&nested, ctx)
        }
        Unhandled::NotHandled => unhandled(rec, ctx),
    }
}

/// Exception non gérée : filtre de dernier recours du programme, puis arrêt
/// avec un diagnostic.
unsafe fn unhandled(rec: &ExceptionRecord, ctx: &mut Context) -> ! {
    let filter = crate::dlls::kernel32_misc::current_unhandled_filter();
    if filter != 0 {
        let mut copy = *rec;
        let mut ptrs = ExceptionPointers {
            record: &mut copy,
            context: ctx.ptr(),
        };
        let f: unsafe extern "win64" fn(*mut ExceptionPointers) -> i32 =
            std::mem::transmute(filter);
        if f(&mut ptrs) == EXCEPTION_CONTINUE_EXECUTION {
            resume(ctx);
        }
    }
    crate::dlls::kernel32_misc::report_unhandled(
        rec.code,
        rec.flags,
        &rec.info[..rec.nparams.min(15) as usize],
    )
}

/// Point d'entrée Rust de `RaiseException`, appelé par le trampoline avec le
/// CONTEXT de l'appelant.
unsafe extern "win64" fn raise_exception_impl(
    code: u32,
    flags: u32,
    nargs: u32,
    args: *const usize,
    ctx: *mut u8,
) -> ! {
    let mut context = Context::zeroed();
    std::ptr::copy_nonoverlapping(ctx, context.ptr(), CONTEXT_SIZE);
    let mut rec = ExceptionRecord::new(
        code,
        flags & EXCEPTION_NONCONTINUABLE,
        context.get(CTX_RIP) as usize,
    );
    let n = nargs.min(15);
    rec.nparams = n;
    if !args.is_null() {
        for i in 0..n as usize {
            rec.info[i] = *args.add(i);
        }
    }
    // 0x406D1388 : convention MSVC pour nommer un thread dans le débogueur ;
    // sans débogueur, Windows la transmet aussi aux gestionnaires.
    raise(&mut rec, &mut context)
}

/// `RaiseException(code, flags, nargs, args)` : capture le CONTEXT de
/// l'appelant puis répartit l'exception.
#[unsafe(naked)]
pub unsafe extern "win64" fn raise_exception(
    _code: u32,
    _flags: u32,
    _nargs: u32,
    _args: *const usize,
) {
    core::arch::naked_asm!(
        "mov [rsp + 8], rcx",
        "mov [rsp + 0x10], rdx",
        "mov [rsp + 0x18], r8",
        "mov [rsp + 0x20], r9",
        "sub rsp, 0x518",
        "lea rcx, [rsp + 0x30]",
        "call {capture}",
        // L'appelant : RSP après retour, RIP = adresse de retour.
        "lea rax, [rsp + 0x520]",
        "mov [rsp + 0x30 + 0x98], rax",
        "mov rax, [rsp + 0x518]",
        "mov [rsp + 0x30 + 0xF8], rax",
        "mov rax, [rsp + 0x520]",
        "mov [rsp + 0x30 + 0x80], rax",
        "mov rax, [rsp + 0x528]",
        "mov [rsp + 0x30 + 0x88], rax",
        "lea rax, [rsp + 0x30]",
        "mov [rsp + 0x20], rax",
        "mov rcx, [rsp + 0x520]",
        "mov rdx, [rsp + 0x528]",
        "mov r8, [rsp + 0x530]",
        "mov r9, [rsp + 0x538]",
        "call {raise}",
        "ud2",
        capture = sym rtl_capture_context,
        raise = sym raise_exception_impl,
    )
}

/// `RtlRaiseException(record)` : comme RaiseException avec un record fourni.
unsafe extern "win64" fn rtl_raise_exception_impl(rec: *mut ExceptionRecord, ctx: *mut u8) -> ! {
    let mut context = Context::zeroed();
    std::ptr::copy_nonoverlapping(ctx, context.ptr(), CONTEXT_SIZE);
    let mut r = *rec;
    r.address = context.get(CTX_RIP) as usize;
    raise(&mut r, &mut context)
}

#[unsafe(naked)]
pub unsafe extern "win64" fn rtl_raise_exception(_rec: *mut ExceptionRecord) {
    core::arch::naked_asm!(
        "mov [rsp + 8], rcx",
        "sub rsp, 0x518",
        "lea rcx, [rsp + 0x30]",
        "call {capture}",
        "lea rax, [rsp + 0x520]",
        "mov [rsp + 0x30 + 0x98], rax",
        "mov rax, [rsp + 0x518]",
        "mov [rsp + 0x30 + 0xF8], rax",
        "mov rcx, [rsp + 0x520]",
        "lea rdx, [rsp + 0x30]",
        "call {raise}",
        "ud2",
        capture = sym rtl_capture_context,
        raise = sym rtl_raise_exception_impl,
    )
}

// ============================================================
//  Déroulement
// ============================================================

/// Cœur de `RtlUnwindEx`, à partir du CONTEXT `start` de l'appelant.
unsafe fn unwind_ex(
    target_frame: u64,
    target_ip: u64,
    rec_in: *mut ExceptionRecord,
    retval: u64,
    start: &Context,
    out_ctx: *mut u8,
) -> ! {
    let mut local_rec;
    let rec: *mut ExceptionRecord = if rec_in.is_null() {
        local_rec = ExceptionRecord::new(STATUS_UNWIND, 0, start.get(CTX_RIP) as usize);
        &mut local_rec
    } else {
        rec_in
    };
    (*rec).flags |= EXCEPTION_UNWINDING
        | if target_frame == 0 {
            EXCEPTION_EXIT_UNWIND
        } else {
            0
        };

    let mut context = *start;
    let mut new_context = *start;
    let mut dc = DispatcherContext::new(context.ptr());
    dc.target_ip = target_ip;
    loop {
        dc.context_record = context.ptr();
        if !step(UNW_FLAG_UHANDLER, &mut dc, &mut new_context) || dc.establisher_frame == 0 {
            break;
        }
        if target_frame != 0 && dc.establisher_frame > target_frame {
            crate::log_error!("seh", "cible de déroulement invalide ({target_frame:#x})");
            break;
        }
        if dc.language_handler != 0 {
            if target_frame != 0 && dc.establisher_frame == target_frame {
                (*rec).flags |= EXCEPTION_TARGET_UNWIND;
            }
            let index = push_frame(FrameKind::Handler, context);
            let mut slot = 0u64;
            let handler = dc.language_handler;
            let frame = dc.establisher_frame;
            let r = call_handler_tramp(
                rec,
                frame,
                context.ptr(),
                &mut dc,
                handler,
                frame_slot_ptr(index, &mut slot),
            );
            pop_frames_from(index);
            (*rec).flags &= !(EXCEPTION_TARGET_UNWIND | EXCEPTION_COLLIDED_UNWIND);
            match r {
                DISPOSITION_CONTINUE_SEARCH => {}
                DISPOSITION_COLLIDED => {
                    // Reprendre le déroulement interrompu là où il en était.
                    std::ptr::copy_nonoverlapping(
                        dc.context_record,
                        new_context.ptr(),
                        CONTEXT_SIZE,
                    );
                    context = new_context;
                    let mut f = 0;
                    virtual_unwind_raw(UNW_FLAG_NHANDLER, &dc, &mut new_context, &mut f);
                    (*rec).flags |= EXCEPTION_COLLIDED_UNWIND;
                }
                _ => {
                    crate::log_error!("seh", "disposition de déroulement invalide ({r})");
                    break;
                }
            }
        }
        if dc.establisher_frame == target_frame {
            break;
        }
        context = new_context;
    }
    context.set(CTX_RAX, retval);
    context.set(CTX_RIP, target_ip);
    if !out_ctx.is_null() {
        std::ptr::copy_nonoverlapping(context.0.as_ptr(), out_ctx, CONTEXT_SIZE);
    }
    rtl_restore_context(context.ptr(), rec)
}

unsafe extern "win64" fn rtl_unwind_ex_impl(
    target_frame: u64,
    target_ip: u64,
    rec: *mut ExceptionRecord,
    retval: u64,
    out_ctx: *mut u8,
    start: *const u8,
) -> ! {
    let mut c = Context::zeroed();
    std::ptr::copy_nonoverlapping(start, c.ptr(), CONTEXT_SIZE);
    unwind_ex(target_frame, target_ip, rec, retval, &c, out_ctx)
}

/// `RtlUnwindEx(frame, ip, record, retval, context, history)`.
#[unsafe(naked)]
pub unsafe extern "win64" fn rtl_unwind_ex(
    _target_frame: u64,
    _target_ip: u64,
    _rec: *mut ExceptionRecord,
    _retval: u64,
    _ctx: *mut u8,
    _history: *mut c_void,
) {
    core::arch::naked_asm!(
        "mov [rsp + 8], rcx",
        "mov [rsp + 0x10], rdx",
        "mov [rsp + 0x18], r8",
        "mov [rsp + 0x20], r9",
        "sub rsp, 0x528",
        "lea rcx, [rsp + 0x40]",
        "call {capture}",
        "lea rax, [rsp + 0x530]",
        "mov [rsp + 0x40 + 0x98], rax",
        "mov rax, [rsp + 0x528]",
        "mov [rsp + 0x40 + 0xF8], rax",
        // 5e argument : CONTEXT de sortie ; 6e : CONTEXT de départ.
        "mov rax, [rsp + 0x528 + 0x28]",
        "mov [rsp + 0x20], rax",
        "lea rax, [rsp + 0x40]",
        "mov [rsp + 0x28], rax",
        "mov rcx, [rsp + 0x530]",
        "mov rdx, [rsp + 0x538]",
        "mov r8, [rsp + 0x540]",
        "mov r9, [rsp + 0x548]",
        "call {unwind}",
        "ud2",
        capture = sym rtl_capture_context,
        unwind = sym rtl_unwind_ex_impl,
    )
}

/// `RtlUnwind(frame, ip, record, retval)` : RtlUnwindEx sans CONTEXT fourni.
#[unsafe(naked)]
pub unsafe extern "win64" fn rtl_unwind(
    _target_frame: u64,
    _target_ip: u64,
    _rec: *mut ExceptionRecord,
    _retval: u64,
) {
    core::arch::naked_asm!(
        "mov [rsp + 8], rcx",
        "mov [rsp + 0x10], rdx",
        "mov [rsp + 0x18], r8",
        "mov [rsp + 0x20], r9",
        "sub rsp, 0x528",
        "lea rcx, [rsp + 0x40]",
        "call {capture}",
        "lea rax, [rsp + 0x530]",
        "mov [rsp + 0x40 + 0x98], rax",
        "mov rax, [rsp + 0x528]",
        "mov [rsp + 0x40 + 0xF8], rax",
        "mov qword ptr [rsp + 0x20], 0",
        "lea rax, [rsp + 0x40]",
        "mov [rsp + 0x28], rax",
        "mov rcx, [rsp + 0x530]",
        "mov rdx, [rsp + 0x538]",
        "mov r8, [rsp + 0x540]",
        "mov r9, [rsp + 0x548]",
        "call {unwind}",
        "ud2",
        capture = sym rtl_capture_context,
        unwind = sym rtl_unwind_ex_impl,
    )
}

pub extern "win64" fn raise_fail_fast_exception(
    rec: *const ExceptionRecord,
    _ctx: *const u8,
    _flags: u32,
) -> ! {
    let code = if rec.is_null() {
        0xC000_0409
    } else {
        unsafe { (*rec).code }
    };
    crate::log_error!("seh", "RaiseFailFastException (code {code:#010x})");
    eprintln!("winlux: le programme s'est arrêté volontairement (fail-fast, code 0x{code:08X}).");
    crate::dlls::kernel32::flush_and_exit(code as i32)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "RtlCaptureContext" => fn rtl_capture_context,
        "RtlRestoreContext" => fn rtl_restore_context,
        "RtlLookupFunctionEntry" => fn rtl_lookup_function_entry,
        "RtlVirtualUnwind" => fn rtl_virtual_unwind,
        "RtlUnwindEx" => fn rtl_unwind_ex,
        "RtlUnwind" => fn rtl_unwind,
        "RtlRaiseException" => fn rtl_raise_exception,
        "RtlAddFunctionTable" => fn rtl_add_function_table,
        "RtlDeleteFunctionTable" => fn rtl_delete_function_table,
        "RtlInstallFunctionTableCallback" => fn rtl_install_function_table_callback,
        "RtlAddVectoredExceptionHandler" => fn rtl_add_vectored_exception_handler,
        "RtlRemoveVectoredExceptionHandler" => fn rtl_remove_vectored_exception_handler,
        "RtlAddVectoredContinueHandler" => fn rtl_add_vectored_continue_handler,
        "RtlRemoveVectoredContinueHandler" => fn rtl_remove_vectored_continue_handler,
    }
}

/// Noms kernel32 des mêmes services.
pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RaiseException" => fn raise_exception,
        "RaiseFailFastException" => fn raise_fail_fast_exception,
        "AddVectoredExceptionHandler" => fn rtl_add_vectored_exception_handler,
        "RemoveVectoredExceptionHandler" => fn rtl_remove_vectored_exception_handler,
        "AddVectoredContinueHandler" => fn rtl_add_vectored_continue_handler,
        "RemoveVectoredContinueHandler" => fn rtl_remove_vectored_continue_handler,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_de_deroulement_et_tailles() {
        assert_eq!(opcode_slots(0, 3), 1); // PUSH_NONVOL
        assert_eq!(opcode_slots(1, 0), 2); // ALLOC_LARGE, taille / 8
        assert_eq!(opcode_slots(1, 1), 3); // ALLOC_LARGE, taille 32 bits
        assert_eq!(opcode_slots(4, 0), 2); // SAVE_NONVOL
        assert_eq!(opcode_slots(5, 0), 3); // SAVE_NONVOL_FAR
        assert_eq!(opcode_slots(8, 6), 2); // SAVE_XMM128
        assert_eq!(opcode_slots(9, 6), 3); // SAVE_XMM128_FAR
        assert_eq!(opcode_slots(10, 0), 1); // PUSH_MACHFRAME
    }

    /// Prologue `sub rsp, 0x28 ; movaps [rsp+0x10], xmm6` : le déroulage
    /// restaure XMM6 et retrouve l'adresse de retour (cas des stubs de
    /// coreclr qui sauvegardent les registres d'arguments flottants).
    #[test]
    fn deroule_une_sauvegarde_xmm128() {
        let mut image = vec![0u8; 0x400];
        let base = image.as_mut_ptr() as u64;
        // RUNTIME_FUNCTION { begin 0x100, end 0x200, unwind 0x40 }
        image[0..4].copy_from_slice(&0x100u32.to_le_bytes());
        image[4..8].copy_from_slice(&0x200u32.to_le_bytes());
        image[8..12].copy_from_slice(&0x40u32.to_le_bytes());
        // UNWIND_INFO version 1, prologue de 8 octets, 3 emplacements.
        image[0x40..0x4A].copy_from_slice(&[1, 8, 3, 0, 8, 0x68, 1, 0, 4, 0x42]);
        let mut stack = [0u64; 8];
        stack[2] = 0x1111_2222_3333_4444;
        stack[3] = 0x5555_6666_7777_8888;
        stack[5] = 0xDEAD_BEEF;
        let rsp = stack.as_ptr() as u64;
        let mut ctx = Context::zeroed();
        ctx.set(CTX_RSP, rsp);
        let pc = base + 0x150;
        ctx.set(CTX_RIP, pc);
        let entry = image.as_ptr() as *const u32;
        unsafe { virtual_unwind(0, base, pc, entry, &mut ctx) };
        assert_eq!(ctx.get(CTX_RIP), 0xDEAD_BEEF);
        assert_eq!(ctx.get(CTX_RSP), rsp + 48);
        let xmm6 = CTX_XMM0 + 6 * 16;
        assert_eq!(ctx.get(xmm6), 0x1111_2222_3333_4444);
        assert_eq!(ctx.get(xmm6 + 8), 0x5555_6666_7777_8888);
    }
}
