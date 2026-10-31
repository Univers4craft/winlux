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
pub unsafe extern "C" fn cxa_allocate_exception(size: u64) -> *mut c_void {
    let total = ALLOC_SIZE + size as usize;
    let layout = std::alloc::Layout::from_size_align(total, 16).unwrap();
    let ptr = std::alloc::alloc_zeroed(layout);
    if ptr.is_null() {
        std::process::abort();
    }
    ptr.add(ALLOC_SIZE) as *mut c_void
}

// ── __cxa_free_exception ─────────────────────────────────────────────
pub unsafe extern "C" fn cxa_free_exception(obj: *mut c_void) {
    if obj.is_null() {
        return;
    }
    let hdr = (obj as *mut u8).sub(ALLOC_SIZE);
    let size_guess = 256;
    let total = ALLOC_SIZE + size_guess;
    let layout = std::alloc::Layout::from_size_align(total, 16)
        .unwrap_or_else(|_| std::alloc::Layout::from_size_align(ALLOC_SIZE, 16).unwrap());
    std::alloc::dealloc(hdr, layout);
}

// ── __cxa_throw ──────────────────────────────────────────────────────
pub unsafe extern "C" fn cxa_throw(obj: *mut c_void, tinfo: *mut c_void, dtor: *mut c_void) -> ! {
    let obj_ptr = obj as *mut u8;
    let hdr = obj_ptr.sub(ALLOC_SIZE);

    // Fill header
    (hdr as *mut u64).write(tinfo as u64);
    (hdr.add(0x08) as *mut u64).write(dtor as u64);

    // Fill _Unwind_Exception (at hdr + CXA_HDR_SIZE)
    let ue = hdr.add(CXA_HDR_SIZE);
    (ue as *mut u64).write(GXX_EXCEPTION_CLASS);
    (ue.add(8) as *mut u64).write(cxa_exception_cleanup as *const () as u64);

    super::libgcc::unwind_raise_exception(ue as *mut c_void);
    std::process::abort();
}

unsafe extern "C" fn cxa_exception_cleanup(_reason: i32, exc: *mut c_void) {
    if exc.is_null() {
        return;
    }
    let ue = exc as *mut u8;
    let hdr = ue.sub(CXA_HDR_SIZE);
    let dtor_ptr = (hdr.add(0x08) as *const u64).read();
    let obj = ue.add(UE_SIZE);
    if dtor_ptr != 0 {
        type DtorFn = unsafe extern "C" fn(*mut c_void);
        let dtor: DtorFn = std::mem::transmute(dtor_ptr);
        dtor(obj as *mut c_void);
    }
    cxa_free_exception(obj as *mut c_void);
}

// ── __cxa_begin_catch ────────────────────────────────────────────────
pub unsafe extern "C" fn cxa_begin_catch(exc: *mut c_void) -> *mut c_void {
    if exc.is_null() {
        return std::ptr::null_mut();
    }
    let ue = exc as *mut u8;
    CAUGHT_STACK.with(|s| s.borrow_mut().push(ue));
    let hdr = ue.sub(CXA_HDR_SIZE);
    let obj = hdr.add(ALLOC_SIZE);
    obj as *mut c_void
}

// ── __cxa_end_catch ──────────────────────────────────────────────────
pub unsafe extern "C" fn cxa_end_catch() {
    CAUGHT_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(ue) = stack.pop() {
            let hdr = ue.sub(CXA_HDR_SIZE);
            let handler_count = hdr.add(0x28) as *mut i64;
            *handler_count -= 1;
            if *handler_count <= 0 {
                let dtor_ptr = (hdr.add(0x08) as *const u64).read();
                let obj = hdr.add(ALLOC_SIZE);
                if dtor_ptr != 0 {
                    type DtorFn = unsafe extern "C" fn(*mut c_void);
                    let dtor: DtorFn = std::mem::transmute(dtor_ptr);
                    dtor(obj as *mut c_void);
                }
            }
        }
    });
}

// ── __cxa_rethrow ────────────────────────────────────────────────────
pub unsafe extern "C" fn cxa_rethrow() -> ! {
    let ue = CAUGHT_STACK.with(|s| {
        let stack = s.borrow();
        stack.last().copied()
    });
    match ue {
        Some(ue) => {
            super::libgcc::unwind_resume(ue as *mut c_void);
        }
        None => std::process::abort(),
    }
}

// ── __cxa_current_exception_type ─────────────────────────────────────
pub unsafe extern "C" fn cxa_current_exception_type() -> *mut c_void {
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
pub unsafe extern "C" fn cxa_pure_virtual() -> ! {
    eprintln!("winlux: pure virtual function called");
    std::process::abort();
}

// ── __cxa_atexit ─────────────────────────────────────────────────────
pub unsafe extern "C" fn cxa_atexit(
    _func: *mut c_void,
    _arg: *mut c_void,
    _dso: *mut c_void,
) -> i32 {
    0
}

// ── __cxa_guard_acquire / release / abort ────────────────────────────
use std::sync::atomic::{AtomicU8, Ordering};

pub unsafe extern "C" fn cxa_guard_acquire(guard: *mut i64) -> i32 {
    let byte = guard as *const AtomicU8;
    if (*byte).load(Ordering::Acquire) != 0 {
        return 0;
    }
    1
}

pub unsafe extern "C" fn cxa_guard_release(guard: *mut i64) {
    let byte = guard as *mut AtomicU8;
    (*byte).store(1, Ordering::Release);
}

pub unsafe extern "C" fn cxa_guard_abort(_guard: *mut i64) {}

// ── Exports ─────────────────────────────────────────────────────────

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__cxa_allocate_exception" => fn cxa_allocate_exception,
        "__cxa_free_exception" => fn cxa_free_exception,
        "__cxa_throw" => fn cxa_throw,
        "__cxa_begin_catch" => fn cxa_begin_catch,
        "__cxa_end_catch" => fn cxa_end_catch,
        "__cxa_rethrow" => fn cxa_rethrow,
        "__cxa_current_exception_type" => fn cxa_current_exception_type,
        "__cxa_pure_virtual" => fn cxa_pure_virtual,
        "__cxa_atexit" => fn cxa_atexit,
        "__cxa_guard_acquire" => fn cxa_guard_acquire,
        "__cxa_guard_release" => fn cxa_guard_release,
        "__cxa_guard_abort" => fn cxa_guard_abort,
    }
}
