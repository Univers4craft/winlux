//! Exceptions C++ MSVC et blocs `__try` maison (vcruntime140) :
//! `_CxxThrowException`, `__CxxFrameHandler3`, `__C_specific_handler`,
//! `__current_exception(_context)`, `__std_terminate`.
//!
//! Les tables lues sont celles que le compilateur MSVC (ou clang en mode
//! MSVC) place dans le programme : FuncInfo (magie 0x19930520 à 0x19930522),
//! table des états, blocs `try`, gestionnaires `catch`, ThrowInfo et types
//! capturables. Un `catch` s'exécute pendant la consolidation du
//! déroulement (`RtlUnwindEx` avec STATUS_UNWIND_CONSOLIDATE), comme sous
//! Windows, à travers le SEH de `seh.rs`.
//!
//! L'état du cadre est tenu dans la zone « UnwindHelp » que le compilateur
//! réserve et initialise à -2 : mot bas = état courant une fois un `catch`
//! engagé (-2 sinon). Une exception relancée depuis ce `catch` ne peut donc
//! plus être reprise par le même bloc `try`.

use super::seh::{
    self, Context, DispatcherContext, ExceptionRecord, EXCEPTION_EXIT_UNWIND,
    EXCEPTION_NONCONTINUABLE, EXCEPTION_TARGET_UNWIND, EXCEPTION_UNWINDING,
};
use std::cell::RefCell;
use std::ffi::c_void;

pub const CXX_EXCEPTION: u32 = 0xE06D_7363;
const CXX_MAGIC: usize = 0x1993_0520;
const STATUS_UNWIND_CONSOLIDATE: u32 = 0x8000_0029;
const CONTINUE_SEARCH: i32 = 1;
const CONTINUE_EXECUTION: i32 = 0;

const HT_REFERENCE: u32 = 8;
const HT_ALL: u32 = 0x40;
const CT_SIMPLE: u32 = 1;
const CT_BY_REF_ONLY: u32 = 2;
const CT_VIRTUAL_BASE: u32 = 4;

unsafe fn rd32(p: usize) -> u32 {
    (p as *const u32).read_unaligned()
}
unsafe fn rdi(p: usize) -> i32 {
    (p as *const i32).read_unaligned()
}

/// FuncInfo (FH3, x64 : déplacements relatifs à l'image).
#[derive(Clone, Copy, Debug)]
struct FuncInfo {
    base: usize,
    magic: u32,
    unwind_map: usize,
    try_count: u32,
    try_map: usize,
    ip_count: u32,
    ip_map: usize,
    unwind_help: i32,
}

impl FuncInfo {
    unsafe fn read(base: usize, p: usize) -> Option<FuncInfo> {
        let magic = rd32(p) & 0x1FFF_FFFF;
        if !(0x1993_0520..=0x1993_0522).contains(&magic) {
            return None;
        }
        Some(FuncInfo {
            base,
            magic,
            unwind_map: base + rd32(p + 8) as usize,
            try_count: rd32(p + 12),
            try_map: base + rd32(p + 16) as usize,
            ip_count: rd32(p + 20),
            ip_map: base + rd32(p + 24) as usize,
            unwind_help: rdi(p + 28),
        })
    }

    /// État du programme à l'adresse `pc`.
    unsafe fn state_at(&self, pc: u64) -> i32 {
        let rva = (pc as usize).wrapping_sub(self.base) as u32;
        let mut state = -1;
        for i in 0..self.ip_count as usize {
            let e = self.ip_map + i * 8;
            if rd32(e) > rva {
                break;
            }
            state = rdi(e + 4);
        }
        state
    }

    unsafe fn try_block(&self, i: u32) -> TryBlock {
        let e = self.try_map + i as usize * 20;
        TryBlock {
            low: rdi(e),
            high: rdi(e + 4),
            catch_high: rdi(e + 8),
            count: rdi(e + 12).max(0) as u32,
            handlers: self.base + rd32(e + 16) as usize,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TryBlock {
    low: i32,
    high: i32,
    catch_high: i32,
    count: u32,
    handlers: usize,
}

#[derive(Clone, Copy, Debug)]
struct Handler {
    adjectives: u32,
    type_desc: usize,
    catch_obj: i32,
    funclet: usize,
    frame_disp: u32,
}

unsafe fn handler(base: usize, t: &TryBlock, j: u32) -> Handler {
    let e = t.handlers + j as usize * 20;
    let ty = rd32(e + 4);
    Handler {
        adjectives: rd32(e),
        type_desc: if ty == 0 { 0 } else { base + ty as usize },
        catch_obj: rdi(e + 8),
        funclet: base + rd32(e + 12) as usize,
        frame_disp: rd32(e + 16),
    }
}

/// Cadre parent : pour un funclet de `catch`, le cadre de la fonction qui
/// le contient (sauvegardé par le funclet), sinon le cadre lui-même.
unsafe fn parent_frame(
    fi: &FuncInfo,
    dc: &DispatcherContext,
    frame: u64,
) -> (u64, Option<(i32, i32)>) {
    if dc.function_entry.is_null() {
        return (frame, None);
    }
    let begin = fi.base + rd32(dc.function_entry as usize) as usize;
    for i in 0..fi.try_count {
        let t = fi.try_block(i);
        for j in 0..t.count {
            let h = handler(fi.base, &t, j);
            if h.funclet == begin {
                let parent =
                    ((frame as usize + h.frame_disp as usize) as *const u64).read_unaligned();
                return (parent, Some((t.high, t.catch_high)));
            }
        }
    }
    (frame, None)
}

unsafe fn help_slot(fi: &FuncInfo, frame: u64) -> Option<*mut i32> {
    (fi.magic >= 0x1993_0521 && fi.unwind_help != 0)
        .then(|| (frame as i64 + fi.unwind_help as i64) as *mut i32)
}

/// Exécute les destructeurs des états de `from` jusqu'à `to` (exclu).
unsafe fn local_unwind(fi: &FuncInfo, frame: u64, from: i32, to: i32) {
    let mut s = from;
    while s > to && s >= 0 {
        let e = fi.unwind_map + s as usize * 8;
        let next = rdi(e);
        let action = rd32(e + 4);
        if action != 0 {
            let f: unsafe extern "win64" fn(u64, u64) =
                std::mem::transmute(fi.base + action as usize);
            f(0, frame);
        }
        s = next;
    }
}

// --- Types et objets lancés -------------------------------------------------------

/// Paramètres d'une exception C++ : (objet, ThrowInfo, base de ThrowInfo).
unsafe fn cxx_params(rec: &ExceptionRecord) -> Option<(usize, usize, usize)> {
    if rec.code != CXX_EXCEPTION
        || rec.nparams < 3
        || !(0x1993_0520..=0x1993_0522).contains(&rec.info[0])
    {
        return None;
    }
    let ti = rec.info[2];
    if ti == 0 {
        return None;
    }
    let base = if rec.nparams >= 4 && rec.info[3] != 0 {
        rec.info[3]
    } else {
        seh::module_base(ti).unwrap_or(0)
    };
    Some((rec.info[1], ti, base))
}

unsafe fn type_name(td: usize) -> &'static [u8] {
    std::ffi::CStr::from_ptr((td + 16) as *const std::ffi::c_char).to_bytes()
}

/// Type capturable du ThrowInfo qui correspond au gestionnaire `h`.
unsafe fn find_catchable(ti: usize, base: usize, h: &Handler) -> Option<usize> {
    let cta = base + rd32(ti + 12) as usize;
    let n = rdi(cta).max(0) as usize;
    for i in 0..n {
        let ct = base + rd32(cta + 4 + i * 4) as usize;
        let props = rd32(ct);
        let td = base + rd32(ct + 4) as usize;
        if props & CT_BY_REF_ONLY != 0 && h.adjectives & HT_REFERENCE == 0 {
            continue;
        }
        if td == h.type_desc || type_name(td) == type_name(h.type_desc) {
            return Some(ct);
        }
    }
    None
}

/// Applique le déplacement `this` d'un type capturable (PMD).
unsafe fn adjust(obj: usize, ct: usize) -> usize {
    let mdisp = rdi(ct + 8) as isize;
    let pdisp = rdi(ct + 12);
    let vdisp = rdi(ct + 16) as isize;
    let mut p = obj as isize;
    if pdisp >= 0 {
        let vbtable = ((p + pdisp as isize) as *const isize).read_unaligned();
        p += pdisp as isize + ((vbtable + vdisp) as *const i32).read_unaligned() as isize;
    }
    (p + mdisp) as usize
}

/// Copie l'objet lancé dans la variable du `catch`.
unsafe fn copy_object(obj: usize, ct: usize, base: usize, h: &Handler, frame: u64) {
    if h.catch_obj == 0 || h.type_desc == 0 {
        return;
    }
    let dest = (frame as i64 + h.catch_obj as i64) as usize;
    let src = adjust(obj, ct);
    if h.adjectives & HT_REFERENCE != 0 {
        (dest as *mut usize).write_unaligned(src);
        return;
    }
    let props = rd32(ct);
    let size = rdi(ct + 20).max(0) as usize;
    let copy = rd32(ct + 24);
    if props & CT_SIMPLE != 0 || copy == 0 {
        std::ptr::copy_nonoverlapping(src as *const u8, dest as *mut u8, size);
    } else if props & CT_VIRTUAL_BASE != 0 {
        let f: unsafe extern "win64" fn(usize, usize, i32) =
            std::mem::transmute(base + copy as usize);
        f(dest, src, 1);
    } else {
        let f: unsafe extern "win64" fn(usize, usize) = std::mem::transmute(base + copy as usize);
        f(dest, src);
    }
}

unsafe fn destroy_object(obj: usize, ti: usize, base: usize) {
    let dtor = rd32(ti + 4);
    if obj != 0 && dtor != 0 {
        let f: unsafe extern "win64" fn(usize) = std::mem::transmute(base + dtor as usize);
        f(obj);
    }
}

// --- Exceptions en cours de traitement (par thread) ------------------------------

struct Active {
    rec: Box<ExceptionRecord>,
    ctx: Box<Context>,
    /// Position de pile de l'appel du funclet : un déroulement qui remonte
    /// au-dessus abandonne ce `catch`.
    mark: u64,
}

thread_local! {
    static ACTIVE: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
    static CURRENT: RefCell<(usize, usize)> = const { RefCell::new((0, 0)) };
    static CURRENT_SLOTS: RefCell<Box<[usize; 2]>> = RefCell::new(Box::new([0, 0]));
}

fn refresh_current() {
    let (r, c) = ACTIVE.with(|a| {
        a.borrow().last().map_or((0, 0), |e| {
            (&*e.rec as *const _ as usize, &*e.ctx as *const _ as usize)
        })
    });
    CURRENT_SLOTS.with(|s| {
        let mut s = s.borrow_mut();
        s[0] = r;
        s[1] = c;
    });
    CURRENT.with(|c| *c.borrow_mut() = (r, 0));
}

/// Les `catch` actifs appelés sous le cadre `frame`, qui capture une
/// nouvelle exception, sont abandonnés : leur objet est détruit, sauf s'il
/// est celui qui est relancé.
unsafe fn abandon_below(frame: u64, keep_obj: usize) {
    let gone: Vec<Active> = ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        let k = a.iter().position(|e| e.mark < frame).unwrap_or(a.len());
        a.split_off(k)
    });
    for e in gone.iter().rev() {
        if let Some((obj, ti, base)) = cxx_params(&e.rec) {
            if obj != keep_obj {
                destroy_object(obj, ti, base);
            }
        }
    }
    if !gone.is_empty() {
        refresh_current();
    }
}

// --- Lancer --------------------------------------------------------------------

thread_local! {
    static THROW_ARGS: RefCell<[usize; 4]> = const { RefCell::new([0; 4]) };
}

/// Prépare les arguments de RaiseException ; `throw;` reprend l'exception
/// en cours de traitement.
unsafe extern "win64" fn prepare_throw(obj: usize, ti: usize) -> *const usize {
    let args = if obj == 0 && ti == 0 {
        ACTIVE
            .with(|a| {
                a.borrow()
                    .last()
                    .map(|e| [e.rec.info[0], e.rec.info[1], e.rec.info[2], e.rec.info[3]])
            })
            .unwrap_or([CXX_MAGIC, 0, 0, 0])
    } else {
        [CXX_MAGIC, obj, ti, seh::module_base(ti).unwrap_or(0)]
    };
    THROW_ARGS.with(|a| {
        *a.borrow_mut() = args;
        a.as_ptr() as *const usize
    })
}

/// `_CxxThrowException(objet, ThrowInfo)` : lève 0xE06D7363 depuis le
/// cadre de l'appelant.
#[unsafe(naked)]
pub unsafe extern "win64" fn cxx_throw_exception(_obj: usize, _ti: usize) {
    core::arch::naked_asm!(
        "sub rsp, 0x28",
        "call {prepare}",
        "add rsp, 0x28",
        "mov ecx, 0xE06D7363",
        "mov edx, 1",
        "mov r8d, 4",
        "mov r9, rax",
        "jmp {raise}",
        prepare = sym prepare_throw,
        raise = sym seh::raise_exception,
    )
}

// --- __CxxFrameHandler3 ------------------------------------------------------------

/// Rappel de consolidation : exécute le `catch`, renvoie l'adresse de reprise.
/// info : [1] cadre parent, [2] funclet, [3] état cible, [4] record d'origine,
/// [5] CONTEXT d'origine, [6] type capturable, [7] FuncInfo (adresse),
/// [8] base de l'image, [9] 1 si le `try` est dans un funclet de `catch`.
unsafe extern "win64" fn call_catch(rec: *mut ExceptionRecord) -> u64 {
    let r = &*rec;
    let frame = r.info[1] as u64;
    let funclet = r.info[2];
    let orig = *(r.info[4] as *const ExceptionRecord);
    let mut octx = Context::zeroed();
    std::ptr::copy_nonoverlapping(r.info[5] as *const u8, octx.ptr(), seh::CONTEXT_SIZE);
    let target = seh::consolidate_target().unwrap_or(octx);
    let here = 0u8;
    let mark = &here as *const u8 as u64;
    ACTIVE.with(|a| {
        a.borrow_mut().push(Active {
            rec: Box::new(orig),
            ctx: Box::new(octx),
            mark,
        })
    });
    refresh_current();
    let cont = seh::call_funclet(funclet, frame, target);
    // Fin normale du `catch` : l'objet n'est plus utilisé.
    let done = ACTIVE.with(|a| a.borrow_mut().pop());
    refresh_current();
    if let Some(e) = done {
        if let Some((obj, ti, base)) = cxx_params(&e.rec) {
            let still_used = ACTIVE.with(|a| a.borrow().iter().any(|x| x.rec.info[1] == obj));
            if !still_used {
                destroy_object(obj, ti, base);
            }
        }
    }
    // Un `catch` du cadre parent (pas d'un `try` imbriqué dans un `catch`)
    // est terminé : le cadre retrouve son état d'après l'adresse.
    if let (Some(fi), 0) = (FuncInfo::read(r.info[8], r.info[7]), r.info[9]) {
        if let Some(slot) = help_slot(&fi, frame) {
            *slot = -2;
        }
    }
    cont
}

pub unsafe extern "win64" fn cxx_frame_handler3(
    rec: *mut ExceptionRecord,
    frame: u64,
    ctx: *mut u8,
    dc: *mut DispatcherContext,
) -> i32 {
    let (Some(r), Some(d)) = (rec.as_mut(), dc.as_mut()) else {
        return CONTINUE_SEARCH;
    };
    if d.handler_data.is_null() {
        return CONTINUE_SEARCH;
    }
    let base = d.image_base as usize;
    let Some(fi) = FuncInfo::read(base, base + rd32(d.handler_data as usize) as usize) else {
        return CONTINUE_SEARCH;
    };
    let (orig_frame, funclet) = parent_frame(&fi, d, frame);
    let slot = help_slot(&fi, orig_frame);
    let ip_state = fi.state_at(d.control_pc);

    if r.flags & (EXCEPTION_UNWINDING | EXCEPTION_EXIT_UNWIND) != 0 {
        let target = r.flags & EXCEPTION_TARGET_UNWIND != 0
            && r.code == STATUS_UNWIND_CONSOLIDATE
            && r.info[0] == call_catch as *const () as usize;
        if let Some((high, _)) = funclet {
            // Funclet de `catch` : objets locaux du corps du `catch`.
            let to = if target { r.info[3] as i32 } else { high };
            local_unwind(&fi, orig_frame, ip_state, to);
            return CONTINUE_SEARCH;
        }
        let cur = match slot {
            Some(s) if *s != -2 => (*s).min(ip_state),
            _ => ip_state,
        };
        let to = if target { r.info[3] as i32 } else { -1 };
        local_unwind(&fi, frame, cur, to);
        if let Some(s) = slot {
            *s = to;
        }
        return CONTINUE_SEARCH;
    }
    // FH3 /EHs : les exceptions non C++ ne sont pas capturées par catch(...).
    let Some((obj, ti, tbase)) = cxx_params(r) else {
        return CONTINUE_SEARCH;
    };
    if fi.try_count == 0 {
        return CONTINUE_SEARCH;
    }
    let level = match slot {
        Some(s) if funclet.is_none() && *s != -2 => (*s).min(ip_state),
        _ => ip_state,
    };
    for i in 0..fi.try_count {
        let t = fi.try_block(i);
        if level < t.low || level > t.high {
            continue;
        }
        // Dans un funclet de `catch`, seuls les `try` écrits dans ce `catch`
        // le concernent ; les autres sont traités dans le cadre parent.
        if let Some((high, catch_high)) = funclet {
            if t.low <= high || t.high > catch_high {
                continue;
            }
        }
        for j in 0..t.count {
            let h = handler(base, &t, j);
            let ct = if h.type_desc == 0 || h.adjectives & HT_ALL != 0 {
                Some(0)
            } else {
                find_catchable(ti, tbase, &h)
            };
            let Some(ct) = ct else { continue };
            if ct != 0 {
                copy_object(obj, ct, tbase, &h, orig_frame);
            }
            abandon_below(frame, obj);
            let mut cr =
                ExceptionRecord::new(STATUS_UNWIND_CONSOLIDATE, EXCEPTION_NONCONTINUABLE, 0);
            cr.nparams = 10;
            cr.info[0] = call_catch as *const () as usize;
            cr.info[1] = orig_frame as usize;
            cr.info[2] = h.funclet;
            cr.info[3] = (t.low - 1) as usize;
            cr.info[4] = rec as usize;
            cr.info[5] = ctx as usize;
            cr.info[6] = ct;
            cr.info[7] = base + rd32(d.handler_data as usize) as usize;
            cr.info[8] = base;
            cr.info[9] = funclet.is_some() as usize;
            // Le cadre cible est celui qui capture (un funclet pour un `try`
            // imbriqué dans un `catch`) ; le funclet reçoit le cadre parent.
            seh::unwind_from(frame, d.control_pc, &mut cr, 0, ctx);
        }
    }
    CONTINUE_SEARCH
}

// --- __C_specific_handler ------------------------------------------------------------

#[repr(C)]
struct ExceptionPointers {
    rec: *mut ExceptionRecord,
    ctx: *mut u8,
}

/// `__C_specific_handler` : blocs `__try/__except` et `__try/__finally`.
pub unsafe extern "win64" fn c_specific_handler(
    rec: *mut ExceptionRecord,
    frame: u64,
    ctx: *mut u8,
    dc: *mut DispatcherContext,
) -> i32 {
    let (Some(r), Some(d)) = (rec.as_mut(), dc.as_mut()) else {
        return CONTINUE_SEARCH;
    };
    if d.handler_data.is_null() {
        return CONTINUE_SEARCH;
    }
    let base = d.image_base;
    let table = d.handler_data as usize;
    let count = rd32(table);
    let pc = d.control_pc.wrapping_sub(base) as u32;
    let entry = |i: u32| {
        let e = table + 4 + i as usize * 16;
        (rd32(e), rd32(e + 4), rd32(e + 8), rd32(e + 12))
    };
    if r.flags & (EXCEPTION_UNWINDING | EXCEPTION_EXIT_UNWIND) == 0 {
        for i in d.scope_index..count {
            let (begin, end, filter, target) = entry(i);
            if pc < begin || pc >= end || target == 0 {
                continue;
            }
            let verdict = if filter == 1 {
                1
            } else {
                let mut ep = ExceptionPointers { rec, ctx };
                let f: unsafe extern "win64" fn(*mut ExceptionPointers, u64) -> i32 =
                    std::mem::transmute(base as usize + filter as usize);
                f(&mut ep, frame)
            };
            if verdict < 0 {
                return CONTINUE_EXECUTION;
            }
            if verdict > 0 {
                abandon_below(frame, 0);
                seh::unwind_from(frame, base + target as u64, rec, r.code as u64, ctx);
            }
        }
    } else {
        let target_pc = d.target_ip.wrapping_sub(base) as u32;
        for i in d.scope_index..count {
            let (begin, end, handler, target) = entry(i);
            if pc < begin || pc >= end {
                continue;
            }
            if r.flags & EXCEPTION_TARGET_UNWIND != 0 && target == target_pc {
                return CONTINUE_SEARCH;
            }
            if target == 0 {
                d.scope_index = i + 1;
                let f: unsafe extern "win64" fn(u8, u64) =
                    std::mem::transmute(base as usize + handler as usize);
                f(1, frame);
            }
        }
    }
    CONTINUE_SEARCH
}

// --- Divers vcruntime ------------------------------------------------------------------

/// `__current_exception()` : adresse du pointeur vers le record en cours.
pub unsafe extern "win64" fn current_exception() -> *mut usize {
    CURRENT_SLOTS.with(|s| s.borrow_mut().as_mut_ptr())
}
/// `__current_exception_context()` : adresse du pointeur vers son CONTEXT.
pub unsafe extern "win64" fn current_exception_context() -> *mut usize {
    CURRENT_SLOTS.with(|s| s.borrow_mut().as_mut_ptr().add(1))
}
pub unsafe extern "win64" fn uncaught_exceptions() -> i32 {
    0
}
pub unsafe extern "win64" fn std_terminate() {
    eprintln!("winlux: std::terminate() appelé par le programme (exception C++ non capturée)");
    std::process::exit(3);
}
pub unsafe extern "win64" fn is_exception_typeof(
    _type_desc: *const c_void,
    _ep: *const c_void,
) -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "_CxxThrowException" => fn cxx_throw_exception,
        "__CxxFrameHandler3" => fn cxx_frame_handler3,
        "__CxxFrameHandler" => fn cxx_frame_handler3,
        "__C_specific_handler" => fn c_specific_handler,
        "__current_exception" => fn current_exception,
        "__current_exception_context" => fn current_exception_context,
        "__uncaught_exceptions" => fn uncaught_exceptions,
        "__uncaught_exception" => fn uncaught_exceptions,
        "__std_terminate" => fn std_terminate,
        "_is_exception_typeof" => fn is_exception_typeof,
    }
}
