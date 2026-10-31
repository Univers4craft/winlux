//! `libgcc_s_seh-1.dll` — GCC _Unwind_* API backed by SEH.
//!
//! MinGW-w64 C++ exceptions use the GCC unwinder API.  On Windows x64
//! the real libgcc_s_seh-1.dll delegates to RtlUnwindEx / RtlVirtualUnwind.
//! We do the same, calling our ntdll implementations.

use std::ffi::c_void;

// ── _Unwind_Reason_Code ──────────────────────────────────────────────
#[repr(i32)]
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum UnwindReason {
    NoReason = 0,
    ForeignExceptionCaught = 1,
    FatalPhase2Error = 2,
    FatalPhase1Error = 3,
    NormalStop = 4,
    EndOfStack = 5,
    HandlerFound = 6,
    InstallContext = 7,
    ContinueUnwind = 8,
}

// ── _Unwind_Action (bitmask) ─────────────────────────────────────────
const _UA_SEARCH_PHASE: i32 = 1;
const _UA_CLEANUP_PHASE: i32 = 2;
const _UA_HANDLER_FRAME: i32 = 4;
const _UA_FORCE_UNWIND: i32 = 8;

// ── _Unwind_Exception header (GCC ABI) ───────────────────────────────
// 8 bytes exception_class + 8 bytes cleanup fn + 2×8 private fields = 32 bytes
const UE_CLASS: usize = 0;
const UE_CLEANUP: usize = 8;
const UE_PRIVATE1: usize = 16;
const UE_PRIVATE2: usize = 24;
#[allow(dead_code)]
const UE_SIZE: usize = 32;

// ── CONTEXT offsets (must match ntdll.rs) ────────────────────────────
const CTX_RAX: usize = 0x78;
const CTX_RDX: usize = 0x88;
#[allow(dead_code)]
const CTX_RBX: usize = 0x90;
const CTX_RSP: usize = 0x98;
#[allow(dead_code)]
const CTX_RBP: usize = 0xA0;
#[allow(dead_code)]
const CTX_RSI: usize = 0xA8;
#[allow(dead_code)]
const CTX_RDI: usize = 0xB0;
#[allow(dead_code)]
const CTX_R12: usize = 0xD8;
#[allow(dead_code)]
const CTX_R13: usize = 0xE0;
#[allow(dead_code)]
const CTX_R14: usize = 0xE8;
#[allow(dead_code)]
const CTX_R15: usize = 0xF0;
const CTX_RIP: usize = 0xF8;
const CTX_SIZE: usize = 1232;

// ── PE caller context (set by naked cxa_throw / _Unwind_Resume) ─────
use std::sync::atomic::{AtomicU64, Ordering};
pub static THROW_CALLER_RIP: AtomicU64 = AtomicU64::new(0);
pub static THROW_CALLER_RSP: AtomicU64 = AtomicU64::new(0);

#[inline(never)]
pub unsafe extern "win64" fn store_throw_ctx(rip: u64, rsp: u64) {
    THROW_CALLER_RIP.store(rip, Ordering::Relaxed);
    THROW_CALLER_RSP.store(rsp, Ordering::Relaxed);
}

// ── _Unwind_Context ──────────────────────────────────────────────────
// We piggyback on the DISPATCHER_CONTEXT + CONTEXT pair that SEH gives us.
// During the search phase (phase 1) we walk frames manually.
// During cleanup (phase 2) we call RtlUnwindEx.
//
// Our _Unwind_Context wraps a CONTEXT (register snapshot).
// We also carry the DISPATCHER_CONTEXT fields we need (image_base,
// function_entry, handler_data).
#[repr(C)]
struct UnwindContext {
    ctx: [u8; CTX_SIZE], // Windows CONTEXT
    image_base: u64,
    function_entry: *const c_void,
    handler_data: *const c_void,
    establisher_frame: u64,
}

// ── Helpers ──────────────────────────────────────────────────────────
unsafe fn ctx_get(c: *const u8, off: usize) -> u64 {
    (c.add(off) as *const u64).read_unaligned()
}
unsafe fn ctx_set(c: *mut u8, off: usize, v: u64) {
    (c.add(off) as *mut u64).write_unaligned(v);
}

// ── Phase 1 : search for a handler ──────────────────────────────────
// Walk frames using RtlVirtualUnwind, call personality routines to find
// who can catch the exception.
unsafe fn phase1(exc: *mut u8) -> UnwindReason {
    let mut uc = std::mem::zeroed::<UnwindContext>();
    // Start directly from the PE frame that threw (set by naked cxa_throw).
    let throw_rip = THROW_CALLER_RIP.load(Ordering::Relaxed);
    let throw_rsp = THROW_CALLER_RSP.load(Ordering::Relaxed);
    if throw_rip != 0 {
        ctx_set(uc.ctx.as_mut_ptr(), CTX_RIP, throw_rip);
        ctx_set(uc.ctx.as_mut_ptr(), CTX_RSP, throw_rsp);
    } else {
        super::ntdll::rtl_capture_context(uc.ctx.as_mut_ptr() as *mut c_void);
    }

    for _ in 0..256u32 {
        let rip = ctx_get(uc.ctx.as_ptr(), CTX_RIP);
        if rip == 0 {
            return UnwindReason::EndOfStack;
        }

        let mut image_base: u64 = 0;
        let func_entry =
            super::ntdll::rtl_lookup_function_entry(rip, &mut image_base, std::ptr::null_mut());
        if func_entry.is_null() {
            // Leaf function — pop return address
            let rsp = ctx_get(uc.ctx.as_ptr(), CTX_RSP);
            ctx_set(
                uc.ctx.as_mut_ptr(),
                CTX_RIP,
                (rsp as *const u64).read_unaligned(),
            );
            ctx_set(uc.ctx.as_mut_ptr(), CTX_RSP, rsp + 8);
            continue;
        }

        let mut handler_data: *mut c_void = std::ptr::null_mut();
        let mut frame: u64 = 0;
        let handler = super::ntdll::rtl_virtual_unwind(
            1, // UNW_FLAG_EHANDLER
            image_base,
            rip,
            func_entry,
            uc.ctx.as_mut_ptr() as *mut c_void,
            &mut handler_data,
            &mut frame,
            std::ptr::null_mut(),
        );

        if !handler.is_null() {
            uc.image_base = image_base;
            uc.function_entry = func_entry;
            uc.handler_data = handler_data;
            uc.establisher_frame = frame;

            // Call the personality routine (search phase)
            type PersonalityFn =
                unsafe extern "win64" fn(i32, i32, u64, *mut u8, *mut UnwindContext) -> i32;
            let personality: PersonalityFn = std::mem::transmute(handler);
            let exc_class = (exc.add(UE_CLASS) as *const u64).read_unaligned();
            let reason = personality(
                1, // version
                _UA_SEARCH_PHASE,
                exc_class,
                exc,
                &mut uc,
            );
            if reason == UnwindReason::HandlerFound as i32 {
                // Store the target frame in the exception's private data
                (exc.add(UE_PRIVATE1) as *mut u64).write_unaligned(frame);
                (exc.add(UE_PRIVATE2) as *mut u64)
                    .write_unaligned(ctx_get(uc.ctx.as_ptr(), CTX_RIP));
                return UnwindReason::HandlerFound;
            }
        }
    }
    UnwindReason::EndOfStack
}

// ── Phase 2 : cleanup and jump to handler ───────────────────────────
unsafe fn phase2(exc: *mut u8) -> UnwindReason {
    let target_frame = (exc.add(UE_PRIVATE1) as *const u64).read_unaligned();

    let mut uc = std::mem::zeroed::<UnwindContext>();
    let throw_rip = THROW_CALLER_RIP.load(Ordering::Relaxed);
    let throw_rsp = THROW_CALLER_RSP.load(Ordering::Relaxed);
    if throw_rip != 0 {
        ctx_set(uc.ctx.as_mut_ptr(), CTX_RIP, throw_rip);
        ctx_set(uc.ctx.as_mut_ptr(), CTX_RSP, throw_rsp);
    } else {
        super::ntdll::rtl_capture_context(uc.ctx.as_mut_ptr() as *mut c_void);
    }

    for _ in 0..256u32 {
        let rip = ctx_get(uc.ctx.as_ptr(), CTX_RIP);
        if rip == 0 {
            return UnwindReason::EndOfStack;
        }

        let mut image_base: u64 = 0;
        let func_entry =
            super::ntdll::rtl_lookup_function_entry(rip, &mut image_base, std::ptr::null_mut());
        if func_entry.is_null() {
            let rsp = ctx_get(uc.ctx.as_ptr(), CTX_RSP);
            ctx_set(
                uc.ctx.as_mut_ptr(),
                CTX_RIP,
                (rsp as *const u64).read_unaligned(),
            );
            ctx_set(uc.ctx.as_mut_ptr(), CTX_RSP, rsp + 8);
            continue;
        }

        let mut handler_data: *mut c_void = std::ptr::null_mut();
        let mut frame: u64 = 0;
        let handler = super::ntdll::rtl_virtual_unwind(
            2, // UNW_FLAG_UHANDLER
            image_base,
            rip,
            func_entry,
            uc.ctx.as_mut_ptr() as *mut c_void,
            &mut handler_data,
            &mut frame,
            std::ptr::null_mut(),
        );

        if !handler.is_null() {
            uc.image_base = image_base;
            uc.function_entry = func_entry;
            uc.handler_data = handler_data;
            uc.establisher_frame = frame;

            let actions = if frame == target_frame {
                _UA_CLEANUP_PHASE | _UA_HANDLER_FRAME
            } else {
                _UA_CLEANUP_PHASE
            };

            type PersonalityFn =
                unsafe extern "win64" fn(i32, i32, u64, *mut u8, *mut UnwindContext) -> i32;
            let personality: PersonalityFn = std::mem::transmute(handler);
            let exc_class = (exc.add(UE_CLASS) as *const u64).read_unaligned();
            let reason = personality(1, actions, exc_class, exc, &mut uc);

            if reason == UnwindReason::InstallContext as i32 {
                restore_context(&uc);
            }
        }

        if frame == target_frame {
            restore_context(&uc);
        }
    }
    UnwindReason::FatalPhase2Error
}

unsafe fn restore_context(uc: &UnwindContext) -> ! {
    let ctx_ptr = uc.ctx.as_ptr();
    std::arch::asm!(
        "mov rbx, [r11 + 0x90]",
        "mov rbp, [r11 + 0xA0]",
        "mov rsi, [r11 + 0xA8]",
        "mov rdi, [r11 + 0xB0]",
        "mov r12, [r11 + 0xD8]",
        "mov r13, [r11 + 0xE0]",
        "mov r14, [r11 + 0xE8]",
        "mov r15, [r11 + 0xF0]",
        "mov rsp, [r11 + 0x98]",
        "mov rax, [r11 + 0x78]",
        "mov rdx, [r11 + 0x88]",
        "mov rcx, [r11 + 0xF8]",
        "jmp rcx",
        in("r11") ctx_ptr,
        options(noreturn),
    );
}

// ====================================================================
//  Public API (_Unwind_*)
// ====================================================================

pub unsafe extern "win64" fn unwind_raise_exception(exc: *mut c_void) -> i32 {
    let exc = exc as *mut u8;
    let search = phase1(exc);
    if search != UnwindReason::HandlerFound {
        return search as i32;
    }
    let _cleanup = phase2(exc);
    // If we reach here, phase2 didn't jump (shouldn't happen in normal flow).
    // The personality is supposed to longjmp/restore context.
    UnwindReason::FatalPhase2Error as i32
}

core::arch::global_asm!(
    ".globl winlux_unwind_resume",
    ".p2align 4",
    "winlux_unwind_resume:",
    // RCX = exc (one param to save)
    "sub rsp, 40",
    "mov [rsp+32], rcx",
    "mov rcx, [rsp+40]",   // caller RIP
    "lea rdx, [rsp+48]",   // caller RSP
    "call {store}",
    "mov rcx, [rsp+32]",
    "add rsp, 40",
    "jmp {inner}",
    store = sym store_throw_ctx,
    inner = sym unwind_resume_inner,
);
extern "win64" {
    pub fn winlux_unwind_resume(exc: *mut c_void) -> !;
}

pub unsafe extern "win64" fn unwind_resume_inner(exc: *mut c_void) -> ! {
    let exc = exc as *mut u8;
    phase2(exc);
    std::process::abort();
}

pub unsafe extern "win64" fn unwind_delete_exception(exc: *mut c_void) {
    if exc.is_null() {
        return;
    }
    let cleanup_ptr = (exc as *const u8).add(UE_CLEANUP) as *const u64;
    let cleanup_fn = (*cleanup_ptr) as usize;
    if cleanup_fn != 0 {
        type CleanupFn = unsafe extern "win64" fn(i32, *mut c_void);
        let cleanup: CleanupFn = std::mem::transmute(cleanup_fn);
        cleanup(UnwindReason::ForeignExceptionCaught as i32, exc);
    }
}

pub unsafe extern "win64" fn unwind_get_language_specific_data(ctx: *mut c_void) -> *mut c_void {
    let uc = ctx as *const UnwindContext;
    (*uc).handler_data as *mut c_void
}

pub unsafe extern "win64" fn unwind_get_region_start(ctx: *mut c_void) -> u64 {
    let uc = ctx as *const UnwindContext;
    if (*uc).function_entry.is_null() {
        return 0;
    }
    let rf = (*uc).function_entry as *const [u32; 3];
    (*uc).image_base + (*rf)[0] as u64
}

pub unsafe extern "win64" fn unwind_get_ip(ctx: *mut c_void) -> u64 {
    let uc = ctx as *const UnwindContext;
    ctx_get((*uc).ctx.as_ptr(), CTX_RIP)
}

pub unsafe extern "win64" fn unwind_set_ip(ctx: *mut c_void, ip: u64) {
    let uc = ctx as *mut UnwindContext;
    ctx_set((*uc).ctx.as_mut_ptr(), CTX_RIP, ip);
}

pub unsafe extern "win64" fn unwind_get_gr(ctx: *mut c_void, reg: i32) -> u64 {
    let uc = ctx as *const UnwindContext;
    let off = CTX_RAX + (reg as usize) * 8;
    ctx_get((*uc).ctx.as_ptr(), off)
}

pub unsafe extern "win64" fn unwind_set_gr(ctx: *mut c_void, reg: i32, val: u64) {
    let uc = ctx as *mut UnwindContext;
    let off = CTX_RAX + (reg as usize) * 8;
    ctx_set((*uc).ctx.as_mut_ptr(), off, val);
}

pub unsafe extern "win64" fn unwind_get_data_rel_base(ctx: *mut c_void) -> u64 {
    let uc = ctx as *const UnwindContext;
    (*uc).image_base
}

pub unsafe extern "win64" fn unwind_get_text_rel_base(ctx: *mut c_void) -> u64 {
    let uc = ctx as *const UnwindContext;
    (*uc).image_base
}

// __gcc_personality_seh0 / __gxx_personality_seh0 — the personality
// routine that MinGW places in each frame's .xdata.
// On real Windows this is in libgcc; the linker emits a reference to it.
// For now we provide a stub that behaves like the C personality:
//   search phase  → CONTINUE_UNWIND (no handler)
//   cleanup phase → CONTINUE_UNWIND (nothing to clean)
// Actual C++ personality is __gxx_personality_seh0 in libstdc++ and
// references LSDA (Language-Specific Data Area).
// We'll need to implement LSDA parsing for real C++ catch.

pub unsafe extern "win64" fn gcc_personality_seh0(
    _version: i32,
    _actions: i32,
    _exception_class: u64,
    _exc: *mut c_void,
    _ctx: *mut c_void,
) -> i32 {
    UnwindReason::ContinueUnwind as i32
}

// __gxx_personality_seh0 — C++ personality with LSDA parsing.
// This reads the DWARF-encoded LSDA (Language-Specific Data Area) from
// .gcc_except_table to determine which catch/cleanup landing pads match.
pub unsafe extern "win64" fn gxx_personality_seh0(
    _version: i32,
    actions: i32,
    _exception_class: u64,
    exc: *mut c_void,
    ctx: *mut c_void,
) -> i32 {
    let uc = ctx as *mut UnwindContext;
    let lsda = (*uc).handler_data as *const u8;
    if lsda.is_null() {
        return UnwindReason::ContinueUnwind as i32;
    }

    let ip = ctx_get((*uc).ctx.as_ptr(), CTX_RIP);
    let func_start = unwind_get_region_start(ctx);
    let ip_offset = ip - func_start;

    // Parse LSDA header
    let mut p = lsda;
    let lp_start_encoding = p.read();
    p = p.add(1);
    let lp_start = if lp_start_encoding != 0xFF {
        let (val, adv) = read_encoded(p, lp_start_encoding);
        p = p.add(adv);
        val
    } else {
        func_start
    };

    let tt_encoding = p.read();
    p = p.add(1);
    let _tt_base = if tt_encoding != 0xFF {
        let (val, adv) = read_uleb128(p);
        p = p.add(adv);
        p as u64 + val
    } else {
        0
    };

    let cs_encoding = p.read();
    p = p.add(1);
    let (cs_table_len, adv) = read_uleb128(p);
    p = p.add(adv);
    let cs_table_end = p as u64 + cs_table_len;

    // Walk the call-site table
    while (p as u64) < cs_table_end {
        let (cs_start, adv) = read_encoded(p, cs_encoding);
        p = p.add(adv);
        let (cs_len, adv) = read_encoded(p, cs_encoding);
        p = p.add(adv);
        let (cs_lp, adv) = read_encoded(p, cs_encoding);
        p = p.add(adv);
        let (_cs_action, adv) = read_uleb128(p);
        p = p.add(adv);

        if ip_offset >= cs_start && ip_offset < cs_start + cs_len {
            if cs_lp == 0 {
                return UnwindReason::ContinueUnwind as i32;
            }
            let landing_pad = lp_start + cs_lp;

            if actions & _UA_SEARCH_PHASE != 0 {
                return UnwindReason::HandlerFound as i32;
            }
            if actions & _UA_CLEANUP_PHASE != 0 {
                // Set up landing pad
                // GR 0 = RAX = exception object pointer
                // GR 1 = RDX = type selector
                ctx_set((*uc).ctx.as_mut_ptr(), CTX_RAX, exc as u64);
                ctx_set((*uc).ctx.as_mut_ptr(), CTX_RDX, _cs_action);
                ctx_set((*uc).ctx.as_mut_ptr(), CTX_RIP, landing_pad);
                return UnwindReason::InstallContext as i32;
            }
        }
    }
    UnwindReason::ContinueUnwind as i32
}

// ── DWARF encoding helpers ──────────────────────────────────────────

unsafe fn read_uleb128(mut p: *const u8) -> (u64, usize) {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    let mut count = 0usize;
    loop {
        let byte = p.read();
        p = p.add(1);
        count += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (result, count)
}

#[allow(dead_code)]
unsafe fn read_sleb128(mut p: *const u8) -> (i64, usize) {
    let mut result: i64 = 0;
    let mut shift = 0u32;
    let mut count = 0usize;
    let mut byte;
    loop {
        byte = p.read();
        p = p.add(1);
        count += 1;
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    if shift < 64 && (byte & 0x40) != 0 {
        result |= -(1i64 << shift);
    }
    (result, count)
}

unsafe fn read_encoded(p: *const u8, encoding: u8) -> (u64, usize) {
    if encoding == 0xFF {
        return (0, 0);
    }
    let base_enc = encoding & 0x0F;
    match base_enc {
        // DW_EH_PE_uleb128
        0x01 => read_uleb128(p),
        // DW_EH_PE_udata2
        0x02 => ((p as *const u16).read_unaligned() as u64, 2),
        // DW_EH_PE_udata4
        0x03 => ((p as *const u32).read_unaligned() as u64, 4),
        // DW_EH_PE_udata8
        0x04 => ((p as *const u64).read_unaligned(), 8),
        // DW_EH_PE_sleb128
        0x09 => {
            let (v, n) = read_sleb128(p);
            (v as u64, n)
        }
        // DW_EH_PE_sdata4
        0x0B => ((p as *const i32).read_unaligned() as u64, 4),
        // DW_EH_PE_absptr / DW_EH_PE_udata4 (common in MinGW)
        0x00 => ((p as *const u64).read_unaligned(), 8),
        _ => (0, 0),
    }
}

// ── RtlUnwindEx wrapper for phase2 (used by personality) ─────────────
pub unsafe extern "win64" fn unwind_force_unwind(
    exc: *mut c_void,
    _stop: *mut c_void,
    _stop_arg: *mut c_void,
) -> i32 {
    let _ = exc;
    UnwindReason::FatalPhase2Error as i32
}

pub unsafe extern "win64" fn unwind_backtrace(_ip: *mut c_void, _arg: *mut c_void) -> i32 {
    UnwindReason::EndOfStack as i32
}

// ── Exports ─────────────────────────────────────────────────────────

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "_Unwind_RaiseException" => fn unwind_raise_exception,
        "_Unwind_Resume" => fn winlux_unwind_resume,
        "_Unwind_DeleteException" => fn unwind_delete_exception,
        "_Unwind_GetLanguageSpecificData" => fn unwind_get_language_specific_data,
        "_Unwind_GetRegionStart" => fn unwind_get_region_start,
        "_Unwind_GetIPInfo" => fn unwind_get_ip,
        "_Unwind_GetIP" => fn unwind_get_ip,
        "_Unwind_SetIP" => fn unwind_set_ip,
        "_Unwind_GetGR" => fn unwind_get_gr,
        "_Unwind_SetGR" => fn unwind_set_gr,
        "_Unwind_GetDataRelBase" => fn unwind_get_data_rel_base,
        "_Unwind_GetTextRelBase" => fn unwind_get_text_rel_base,
        "_Unwind_ForcedUnwind" => fn unwind_force_unwind,
        "_Unwind_Backtrace" => fn unwind_backtrace,
        "__gcc_personality_seh0" => fn gcc_personality_seh0,
        "__gxx_personality_seh0" => fn gxx_personality_seh0,
    }
}
