//! `libstdc++-6.dll` — C++ runtime stubs (MinGW).
//!
//! Provides the __cxa_* ABI entry points that MinGW-compiled C++ programs
//! import from libstdc++-6.dll.  The throw/catch machinery delegates to
//! our _Unwind_* layer (libgcc.rs) which uses the SEH unwinder.

use std::ffi::c_void;

// ── __cxa_exception header (simplified) ──────────────────────────────
// Laid out before the user's exception object so the pointer given to
// `_Unwind_RaiseException` points to the _Unwind_Exception which is at
// the *end* of the header.
//
// Offsets (all 8-byte aligned on x64):
//   0x00  exception_type   (void* → typeinfo)
//   0x08  exception_dtor   (void (*)(void*))
//   0x10  unexpected_handler
//   0x18  terminate_handler
//   0x20  next_exception
//   0x28  handler_count
//   0x30  handler_switch_value
//   0x38  action_record
//   0x40  language_specific_data
//   0x48  caught_ptr
//   0x50  adjusted_ptr
//   0x58  _Unwind_Exception (32 bytes)
//   total = 0x78 (120 bytes)

const CXA_HDR_SIZE: usize = 0x58;
const UE_SIZE: usize = 32;
const ALLOC_SIZE: usize = CXA_HDR_SIZE + UE_SIZE;

// GCC exception class: "GNUCC++\0" encoded as u64
const GXX_EXCEPTION_CLASS: u64 = u64::from_le_bytes(*b"GNUCC++\0");

// ── Thread-local caught exception stack ──────────────────────────────
use std::cell::RefCell;
thread_local! {
    static CAUGHT_STACK: RefCell<Vec<*mut u8>> = const { RefCell::new(Vec::new()) };
}

// ── __cxa_allocate_exception ─────────────────────────────────────────
pub unsafe extern "win64" fn cxa_allocate_exception(size: u64) -> *mut c_void {
    let total = ALLOC_SIZE + size as usize;
    let layout = std::alloc::Layout::from_size_align(total, 16).unwrap();
    let ptr = std::alloc::alloc_zeroed(layout);
    if ptr.is_null() {
        std::process::abort();
    }
    // Return pointer to the user object (after header + _Unwind_Exception)
    ptr.add(ALLOC_SIZE) as *mut c_void
}

// ── __cxa_free_exception ─────────────────────────────────────────────
pub unsafe extern "win64" fn cxa_free_exception(obj: *mut c_void) {
    if obj.is_null() {
        return;
    }
    let hdr = (obj as *mut u8).sub(ALLOC_SIZE);
    let size_guess = 256; // We don't track size; use header-only dealloc
    let total = ALLOC_SIZE + size_guess;
    let layout = std::alloc::Layout::from_size_align(total, 16)
        .unwrap_or_else(|_| std::alloc::Layout::from_size_align(ALLOC_SIZE, 16).unwrap());
    std::alloc::dealloc(hdr, layout);
}

// ── __cxa_throw ──────────────────────────────────────────────────────
// Trampoline captures PE caller's RIP/RSP before the Rust prologue runs.
core::arch::global_asm!(
    ".globl winlux_cxa_throw",
    ".p2align 4",
    "winlux_cxa_throw:",
    // On entry: [rsp]=ret_addr, RCX=obj, RDX=tinfo, R8=dtor
    "sub rsp, 56",
    "mov [rsp+48], rcx",
    "mov [rsp+40], rdx",
    "mov [rsp+32], r8",
    "mov rcx, [rsp+56]",   // caller RIP (return address)
    "lea rdx, [rsp+64]",   // caller RSP
    "call {store}",
    "mov rcx, [rsp+48]",
    "mov rdx, [rsp+40]",
    "mov r8, [rsp+32]",
    "add rsp, 56",
    "jmp {inner}",
    store = sym super::libgcc::store_throw_ctx,
    inner = sym cxa_throw_inner,
);
extern "win64" {
    pub fn winlux_cxa_throw(obj: *mut c_void, tinfo: *mut c_void, dtor: *mut c_void) -> !;
}

unsafe extern "win64" fn cxa_throw_inner(
    obj: *mut c_void,
    tinfo: *mut c_void,
    dtor: *mut c_void,
) -> ! {
    let obj_ptr = obj as *mut u8;
    let hdr = obj_ptr.sub(ALLOC_SIZE);

    // Fill header
    (hdr as *mut u64).write(tinfo as u64); // exception_type
    (hdr.add(0x08) as *mut u64).write(dtor as u64); // exception_dtor

    // Fill _Unwind_Exception (at hdr + CXA_HDR_SIZE)
    let ue = hdr.add(CXA_HDR_SIZE);
    (ue as *mut u64).write(GXX_EXCEPTION_CLASS); // exception_class
    (ue.add(8) as *mut u64).write(cxa_exception_cleanup as *const () as u64);

    super::libgcc::unwind_raise_exception(ue as *mut c_void);
    std::process::abort();
}

unsafe extern "win64" fn cxa_exception_cleanup(_reason: i32, exc: *mut c_void) {
    if exc.is_null() {
        return;
    }
    let ue = exc as *mut u8;
    let hdr = ue.sub(CXA_HDR_SIZE);
    let dtor_ptr = (hdr.add(0x08) as *const u64).read();
    let obj = ue.add(UE_SIZE); // user object is right after UE
    if dtor_ptr != 0 {
        type DtorFn = unsafe extern "win64" fn(*mut c_void);
        let dtor: DtorFn = std::mem::transmute(dtor_ptr);
        dtor(obj as *mut c_void);
    }
    cxa_free_exception(obj as *mut c_void);
}

// ── __cxa_begin_catch ────────────────────────────────────────────────
pub unsafe extern "win64" fn cxa_begin_catch(exc: *mut c_void) -> *mut c_void {
    if exc.is_null() {
        return std::ptr::null_mut();
    }
    let ue = exc as *mut u8;
    CAUGHT_STACK.with(|s| s.borrow_mut().push(ue));
    // Return pointer to the thrown object (after _Unwind_Exception header)
    // Actually for GCC, the exc pointer points to _Unwind_Exception,
    // and the thrown object is at hdr + ALLOC_SIZE (which is ue + UE_SIZE for our layout).
    // But the C++ runtime passes exc as the value of RAX from the landing pad,
    // which is the _Unwind_Exception*. The actual thrown object is after the full header.
    let hdr = ue.sub(CXA_HDR_SIZE);
    let obj = hdr.add(ALLOC_SIZE);
    obj as *mut c_void
}

// ── __cxa_end_catch ──────────────────────────────────────────────────
pub unsafe extern "win64" fn cxa_end_catch() {
    CAUGHT_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(ue) = stack.pop() {
            let hdr = ue.sub(CXA_HDR_SIZE);
            // Decrement handler count or free
            let handler_count = hdr.add(0x28) as *mut i64;
            *handler_count -= 1;
            if *handler_count <= 0 {
                // Free the exception
                let dtor_ptr = (hdr.add(0x08) as *const u64).read();
                let obj = hdr.add(ALLOC_SIZE);
                if dtor_ptr != 0 {
                    type DtorFn = unsafe extern "win64" fn(*mut c_void);
                    let dtor: DtorFn = std::mem::transmute(dtor_ptr);
                    dtor(obj as *mut c_void);
                }
                // Don't free memory during rethrow scenarios
            }
        }
    });
}

// ── __cxa_rethrow ────────────────────────────────────────────────────
core::arch::global_asm!(
    ".globl winlux_cxa_rethrow",
    ".p2align 4",
    "winlux_cxa_rethrow:",
    // No params to save
    "sub rsp, 40",
    "mov rcx, [rsp+40]",   // caller RIP
    "lea rdx, [rsp+48]",   // caller RSP
    "call {store}",
    "add rsp, 40",
    "jmp {inner}",
    store = sym super::libgcc::store_throw_ctx,
    inner = sym cxa_rethrow_inner,
);
extern "win64" {
    pub fn winlux_cxa_rethrow() -> !;
}

unsafe extern "win64" fn cxa_rethrow_inner() -> ! {
    let ue = CAUGHT_STACK.with(|s| {
        let stack = s.borrow();
        stack.last().copied()
    });
    match ue {
        Some(ue) => {
            super::libgcc::unwind_resume_inner(ue as *mut c_void);
        }
        None => std::process::abort(),
    }
}

// ── __cxa_current_exception_type ─────────────────────────────────────
pub unsafe extern "win64" fn cxa_current_exception_type() -> *mut c_void {
    CAUGHT_STACK.with(|s| {
        let stack = s.borrow();
        match stack.last() {
            Some(&ue) => {
                let hdr = ue.sub(CXA_HDR_SIZE);
                (hdr as *const u64).read() as *mut c_void
            }
            None => std::ptr::null_mut(),
        }
    })
}

// ── __cxa_pure_virtual ───────────────────────────────────────────────
pub unsafe extern "win64" fn cxa_pure_virtual() -> ! {
    eprintln!("winlux: pure virtual function called");
    std::process::abort();
}

// ── __cxa_atexit ─────────────────────────────────────────────────────
pub unsafe extern "win64" fn cxa_atexit(
    _func: *mut c_void,
    _arg: *mut c_void,
    _dso: *mut c_void,
) -> i32 {
    0
}

// ── __cxa_guard_acquire / release / abort ────────────────────────────
// For thread-safe static initialization
use std::sync::atomic::{AtomicU8, Ordering};

pub unsafe extern "win64" fn cxa_guard_acquire(guard: *mut i64) -> i32 {
    let byte = guard as *const AtomicU8;
    if (*byte).load(Ordering::Acquire) != 0 {
        return 0; // Already initialized
    }
    // Simple: just claim it (no contention handling for now)
    1
}

pub unsafe extern "win64" fn cxa_guard_release(guard: *mut i64) {
    let byte = guard as *mut AtomicU8;
    (*byte).store(1, Ordering::Release);
}

pub unsafe extern "win64" fn cxa_guard_abort(_guard: *mut i64) {
    // Nothing to do for our simple implementation
}

// ── Exports ─────────────────────────────────────────────────────────

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__cxa_allocate_exception" => fn cxa_allocate_exception,
        "__cxa_free_exception" => fn cxa_free_exception,
        "__cxa_throw" => fn winlux_cxa_throw,
        "__cxa_begin_catch" => fn cxa_begin_catch,
        "__cxa_end_catch" => fn cxa_end_catch,
        "__cxa_rethrow" => fn winlux_cxa_rethrow,
        "__cxa_current_exception_type" => fn cxa_current_exception_type,
        "__cxa_pure_virtual" => fn cxa_pure_virtual,
        "__cxa_atexit" => fn cxa_atexit,
        "__cxa_guard_acquire" => fn cxa_guard_acquire,
        "__cxa_guard_release" => fn cxa_guard_release,
        "__cxa_guard_abort" => fn cxa_guard_abort,
    }
}
