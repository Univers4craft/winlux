//! ntdll : fonctions Nt* et Rtl* du noyau NT, couche basse utilisée par
//! kernel32, msvcrt et de nombreuses applications Windows. Le runtime Winlux
//! en fournit des implémentations natives Linux ou des stubs raisonnables.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

// ---------- Constantes NTSTATUS ----------

const STATUS_SUCCESS: i32 = 0;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
const STATUS_TIMEOUT: i32 = 0x0000_0102u32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_ACCESS_DENIED: i32 = 0xC000_0022u32 as i32;
const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;
const STATUS_OBJECT_NAME_COLLISION: i32 = 0xC000_0035u32 as i32;
const STATUS_UNSUCCESSFUL: i32 = 0xC000_0001u32 as i32;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ============================================================
//  Fonctions Rtl* existantes (inchangées)
// ============================================================

pub unsafe extern "win64" fn rtl_get_current_peb() -> usize {
    crate::process::peb()
}
pub extern "win64" fn rtl_get_current_teb() -> usize {
    crate::process::teb()
}
pub extern "win64" fn rtl_get_last_win32_error() -> u32 {
    crate::process::last_error()
}
pub extern "win64" fn rtl_set_last_win32_error(code: u32) {
    crate::process::set_last_error(code);
}

pub unsafe extern "win64" fn rtl_get_version(info: *mut u16) -> i32 {
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if crate::dlls::kernel32::get_version_ex_w(info) == 0 {
        STATUS_INVALID_PARAMETER
    } else {
        STATUS_SUCCESS
    }
}

pub extern "win64" fn rtl_nt_status_to_dos_error(status: i32) -> u32 {
    match status as u32 {
        0 => 0,
        0x0000_0102 => 258, // WAIT_TIMEOUT
        0xC000_0005 => 998, // STATUS_ACCESS_VIOLATION → ERROR_NOACCESS
        0xC000_0008 => 6,   // STATUS_INVALID_HANDLE
        0xC000_000D => 87,  // STATUS_INVALID_PARAMETER
        0xC000_0022 => 5,   // STATUS_ACCESS_DENIED
        0xC000_0023 => 122, // STATUS_BUFFER_TOO_SMALL
        0xC000_0034 => 2,   // STATUS_OBJECT_NAME_NOT_FOUND → ERROR_FILE_NOT_FOUND
        0xC000_0035 => 3,   // STATUS_OBJECT_PATH_NOT_FOUND → ERROR_PATH_NOT_FOUND
        0xC000_003A => 3,   // STATUS_OBJECT_PATH_NOT_FOUND
        0xC000_0094 => 534, // STATUS_FLOAT_MULTIPLE_TRAPS
        0xC000_00BA => 32,  // STATUS_SHARING_VIOLATION
        0xC000_0135 => 126, // STATUS_DLL_NOT_FOUND
        _ => 317,           // ERROR_MR_MID_NOT_FOUND
    }
}

pub unsafe extern "win64" fn rtl_allocate_heap(
    _heap: usize,
    flags: u32,
    size: usize,
) -> *mut c_void {
    crate::dlls::kernel32::heap_alloc(0, flags, size)
}
pub unsafe extern "win64" fn rtl_free_heap(_heap: usize, _flags: u32, p: *mut c_void) -> u8 {
    crate::dlls::kernel32::heap_free(0, 0, p) as u8
}
pub unsafe extern "win64" fn rtl_reallocate_heap(
    _heap: usize,
    flags: u32,
    p: *mut c_void,
    size: usize,
) -> *mut c_void {
    crate::dlls::kernel32::heap_realloc(0, flags, p, size)
}

pub unsafe extern "win64" fn rtl_init_unicode_string(dst: *mut u8, src: *const u16) {
    if dst.is_null() {
        return;
    }
    let (len, max, buf) = if src.is_null() {
        (0u16, 0u16, std::ptr::null())
    } else {
        let n = crate::printf::wide_len(src)
            .saturating_mul(2)
            .min((u16::MAX - 2) as usize) as u16;
        (n, n.saturating_add(2), src)
    };
    std::ptr::write_unaligned(dst as *mut u16, len);
    std::ptr::write_unaligned(dst.add(2) as *mut u16, max);
    std::ptr::write_unaligned(dst.add(8) as *mut *const u16, buf);
}

pub unsafe extern "win64" fn rtl_random_ex(seed: *mut u32) -> u32 {
    if seed.is_null() {
        return 0;
    }
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    if x == 0 {
        x = 0x9e37_79b9;
    }
    *seed = x;
    x & 0x7fff_ffff
}

// ============================================================
//  UNICODE_STRING / ANSI_STRING helpers
// ============================================================

/// Layout de UNICODE_STRING / ANSI_STRING (16 octets sur x64).
/// Offset 0: Length (u16), Offset 2: MaximumLength (u16),
/// Offset 4: padding (4 bytes on x64), Offset 8: Buffer (*const u8/u16).
unsafe fn ustr_len(p: *const u8) -> u16 {
    std::ptr::read_unaligned(p as *const u16)
}
unsafe fn ustr_max(p: *const u8) -> u16 {
    std::ptr::read_unaligned(p.add(2) as *const u16)
}
unsafe fn ustr_buf(p: *const u8) -> *const u8 {
    std::ptr::read_unaligned(p.add(8) as *const *const u8)
}
unsafe fn ustr_buf_w(p: *const u8) -> *const u16 {
    std::ptr::read_unaligned(p.add(8) as *const *const u16)
}
unsafe fn ustr_set_len(p: *mut u8, len: u16) {
    std::ptr::write_unaligned(p as *mut u16, len);
}
unsafe fn ustr_set_max(p: *mut u8, max: u16) {
    std::ptr::write_unaligned(p.add(2) as *mut u16, max);
}
unsafe fn ustr_set_buf(p: *mut u8, buf: *const u8) {
    std::ptr::write_unaligned(p.add(8) as *mut *const u8, buf);
}

/// `RtlInitAnsiString` : initialise une ANSI_STRING à partir d'un *const c_char.
pub unsafe extern "win64" fn rtl_init_ansi_string(dst: *mut u8, src: *const u8) {
    if dst.is_null() {
        return;
    }
    if src.is_null() {
        ustr_set_len(dst, 0);
        ustr_set_max(dst, 0);
        ustr_set_buf(dst, std::ptr::null());
    } else {
        let n = crate::printf::narrow_len(src).min((u16::MAX - 1) as usize) as u16;
        ustr_set_len(dst, n);
        ustr_set_max(dst, n + 1);
        ustr_set_buf(dst, src);
    }
}

/// `RtlFreeAnsiString` : libère le tampon alloué par RtlUnicodeStringToAnsiString.
pub unsafe extern "win64" fn rtl_free_ansi_string(s: *mut u8) {
    if s.is_null() {
        return;
    }
    let buf = ustr_buf(s);
    if !buf.is_null() {
        crate::dlls::kernel32::heap_free(0, 0, buf as *mut c_void);
        ustr_set_buf(s, std::ptr::null());
        ustr_set_len(s, 0);
        ustr_set_max(s, 0);
    }
}

/// `RtlFreeUnicodeString` : libère le tampon alloué par RtlAnsiStringToUnicodeString.
pub unsafe extern "win64" fn rtl_free_unicode_string(s: *mut u8) {
    if s.is_null() {
        return;
    }
    let buf = ustr_buf(s);
    if !buf.is_null() {
        crate::dlls::kernel32::heap_free(0, 0, buf as *mut c_void);
        ustr_set_buf(s, std::ptr::null());
        ustr_set_len(s, 0);
        ustr_set_max(s, 0);
    }
}

/// `RtlAnsiStringToUnicodeString` : convertit une ANSI_STRING en UNICODE_STRING.
pub unsafe extern "win64" fn rtl_ansi_string_to_unicode_string(
    dst: *mut u8,
    src: *const u8,
    alloc: u8,
) -> i32 {
    if dst.is_null() || src.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let ansi_len = ustr_len(src) as usize;
    let ansi_buf = ustr_buf(src);
    let text = if ansi_len == 0 || ansi_buf.is_null() {
        String::new()
    } else {
        crate::dlls::decode_ansi(std::slice::from_raw_parts(ansi_buf, ansi_len))
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    let byte_len = wide.len().saturating_mul(2);
    if byte_len > u16::MAX as usize - 2 {
        return STATUS_BUFFER_TOO_SMALL;
    }
    let uni_len = byte_len as u16;
    let uni_max = uni_len + 2;
    if alloc != 0 {
        let p = crate::dlls::kernel32::heap_alloc(0, 0, uni_max as usize) as *mut u16;
        if p.is_null() {
            return 0xC000_0017u32 as i32;
        } // STATUS_NO_MEMORY
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        *p.add(wide.len()) = 0;
        ustr_set_len(dst, uni_len);
        ustr_set_max(dst, uni_max);
        ustr_set_buf(dst, p as *const u8);
    } else {
        let existing_max = ustr_max(dst);
        if (existing_max as usize) < uni_max as usize {
            return STATUS_BUFFER_TOO_SMALL;
        }
        let p = ustr_buf(dst) as *mut u16;
        if p.is_null() {
            return STATUS_INVALID_PARAMETER;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        *p.add(wide.len()) = 0;
        ustr_set_len(dst, uni_len);
    }
    STATUS_SUCCESS
}

/// `RtlUnicodeStringToAnsiString` : convertit une UNICODE_STRING en ANSI_STRING.
pub unsafe extern "win64" fn rtl_unicode_string_to_ansi_string(
    dst: *mut u8,
    src: *const u8,
    alloc: u8,
) -> i32 {
    if dst.is_null() || src.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let uni_len = ustr_len(src) as usize / 2;
    let uni_buf = ustr_buf_w(src);
    let text = if uni_len == 0 || uni_buf.is_null() {
        String::new()
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(uni_buf, uni_len))
    };
    let ansi = crate::dlls::encode_ansi(&text);
    let byte_len = ansi.len();
    if byte_len > u16::MAX as usize - 1 {
        return STATUS_BUFFER_TOO_SMALL;
    }
    let ansi_len = byte_len as u16;
    let ansi_max = ansi_len + 1;
    if alloc != 0 {
        let p = crate::dlls::kernel32::heap_alloc(0, 0, ansi_max as usize) as *mut u8;
        if p.is_null() {
            return 0xC000_0017u32 as i32;
        }
        std::ptr::copy_nonoverlapping(ansi.as_ptr(), p, ansi.len());
        *p.add(ansi.len()) = 0;
        ustr_set_len(dst, ansi_len);
        ustr_set_max(dst, ansi_max);
        ustr_set_buf(dst, p);
    } else {
        let existing_max = ustr_max(dst);
        if (existing_max as usize) < ansi_max as usize {
            return STATUS_BUFFER_TOO_SMALL;
        }
        let p = ustr_buf(dst) as *mut u8;
        if p.is_null() {
            return STATUS_INVALID_PARAMETER;
        }
        std::ptr::copy_nonoverlapping(ansi.as_ptr(), p, ansi.len());
        *p.add(ansi.len()) = 0;
        ustr_set_len(dst, ansi_len);
    }
    STATUS_SUCCESS
}

/// `RtlUnicodeToMultiByteSize` : calcule la taille nécessaire pour la conversion.
pub unsafe extern "win64" fn rtl_unicode_to_multibyte_size(
    result_size: *mut u32,
    unicode: *const u16,
    byte_count: u32,
) -> i32 {
    if result_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let char_count = byte_count as usize / 2;
    if char_count == 0 || unicode.is_null() {
        *result_size = 0;
        return STATUS_SUCCESS;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(unicode, char_count));
    let ansi = crate::dlls::encode_ansi(&text);
    *result_size = ansi.len() as u32;
    STATUS_SUCCESS
}

/// `RtlMultiByteToUnicodeSize` : calcule la taille nécessaire pour la conversion.
pub unsafe extern "win64" fn rtl_multibyte_to_unicode_size(
    result_size: *mut u32,
    multibyte: *const u8,
    byte_count: u32,
) -> i32 {
    if result_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if byte_count == 0 || multibyte.is_null() {
        *result_size = 0;
        return STATUS_SUCCESS;
    }
    let text = crate::dlls::decode_ansi(std::slice::from_raw_parts(multibyte, byte_count as usize));
    let wide_count: usize = text.encode_utf16().count();
    *result_size = (wide_count * 2) as u32;
    STATUS_SUCCESS
}

/// `RtlCompareUnicodeString` : comparaison lexicographique.
pub unsafe extern "win64" fn rtl_compare_unicode_string(
    s1: *const u8,
    s2: *const u8,
    case_insensitive: u8,
) -> i32 {
    if s1.is_null() || s2.is_null() {
        return 0;
    }
    let get_str = |p: *const u8| -> String {
        let len = ustr_len(p) as usize / 2;
        let buf = ustr_buf_w(p);
        if len == 0 || buf.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(buf, len))
        }
    };
    let (a, b) = (get_str(s1), get_str(s2));
    if case_insensitive != 0 {
        a.to_uppercase().cmp(&b.to_uppercase()) as i32
    } else {
        a.cmp(&b) as i32
    }
}

/// `RtlEqualUnicodeString` : test d'égalité.
pub unsafe extern "win64" fn rtl_equal_unicode_string(
    s1: *const u8,
    s2: *const u8,
    case_insensitive: u8,
) -> u8 {
    (rtl_compare_unicode_string(s1, s2, case_insensitive) == 0) as u8
}

/// `RtlCopyUnicodeString` : copie src → dst.
pub unsafe extern "win64" fn rtl_copy_unicode_string(dst: *mut u8, src: *const u8) {
    if dst.is_null() {
        return;
    }
    if src.is_null() {
        ustr_set_len(dst, 0);
        return;
    }
    let src_len = ustr_len(src);
    let src_buf = ustr_buf_w(src);
    let dst_max = ustr_max(dst);
    let dst_buf = ustr_buf(dst) as *mut u16;
    if dst_buf.is_null() || dst_max == 0 {
        ustr_set_len(dst, 0);
        return;
    }
    let copy_bytes = src_len.min(dst_max.saturating_sub(2));
    let copy_chars = copy_bytes as usize / 2;
    if copy_chars > 0 && !src_buf.is_null() {
        std::ptr::copy_nonoverlapping(src_buf, dst_buf, copy_chars);
    }
    if (copy_bytes + 2) <= dst_max {
        *dst_buf.add(copy_chars) = 0;
    }
    ustr_set_len(dst, copy_bytes);
}

/// `RtlAppendUnicodeStringToString` : dst += src.
pub unsafe extern "win64" fn rtl_append_unicode_string_to_string(
    dst: *mut u8,
    src: *const u8,
) -> i32 {
    if dst.is_null() || src.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let src_len = ustr_len(src) as usize;
    let dst_len = ustr_len(dst) as usize;
    let dst_max = ustr_max(dst) as usize;
    let new_len = dst_len + src_len;
    if new_len + 2 > dst_max {
        return STATUS_BUFFER_TOO_SMALL;
    }
    let dst_buf = ustr_buf(dst) as *mut u8;
    let src_buf = ustr_buf(src);
    if dst_buf.is_null() || src_buf.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    std::ptr::copy_nonoverlapping(src_buf, dst_buf.add(dst_len), src_len);
    // Null-terminate
    std::ptr::write_bytes(dst_buf.add(new_len), 0, 2);
    ustr_set_len(dst, new_len as u16);
    STATUS_SUCCESS
}

/// `RtlHashUnicodeString` : hash d'une UNICODE_STRING.
pub unsafe extern "win64" fn rtl_hash_unicode_string(
    string: *const u8,
    case_insensitive: u8,
    _algorithm: u32,
    hash_value: *mut u32,
) -> i32 {
    if string.is_null() || hash_value.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let len = ustr_len(string) as usize / 2;
    let buf = ustr_buf_w(string);
    let mut hash = 0u32;
    if len > 0 && !buf.is_null() {
        for i in 0..len {
            let mut ch = *buf.add(i) as u32;
            if case_insensitive != 0 {
                ch = char::from_u32(ch)
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .to_uppercase()
                    .next()
                    .unwrap_or(char::REPLACEMENT_CHARACTER) as u32;
            }
            hash = hash.wrapping_mul(65599).wrapping_add(ch);
        }
    }
    *hash_value = hash;
    STATUS_SUCCESS
}

// ============================================================
//  Heap (Rtl)
// ============================================================

pub unsafe extern "win64" fn rtl_create_heap(
    _flags: u32,
    _base: *mut c_void,
    _reserve: usize,
    _commit: usize,
    _lock: *mut c_void,
    _params: *mut c_void,
) -> usize {
    // On renvoie le heap global du processus.
    crate::dlls::kernel32::PROCESS_HEAP
}

pub unsafe extern "win64" fn rtl_destroy_heap(_heap: usize) -> usize {
    // Pas de destruction réelle (heap global).
    0
}

pub unsafe extern "win64" fn rtl_size_heap(_heap: usize, _flags: u32, p: *const c_void) -> usize {
    if p.is_null() {
        return usize::MAX;
    }
    // Pas de suivi de taille individualisé ; renvoie 0 (le programme doit
    // connaître la taille allouée).
    0
}

// ============================================================
//  Critical sections (Rtl) — délèguent à kernel32
// ============================================================

pub unsafe extern "win64" fn rtl_initialize_critical_section(cs: *mut c_void) -> i32 {
    crate::dlls::kernel32::initialize_critical_section(cs);
    STATUS_SUCCESS
}

pub unsafe extern "win64" fn rtl_initialize_critical_section_ex(
    cs: *mut c_void,
    spin: u32,
    flags: u32,
) -> i32 {
    if crate::dlls::kernel32::initialize_critical_section_ex(cs, spin, flags) != 0 {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_PARAMETER
    }
}

pub unsafe extern "win64" fn rtl_enter_critical_section(cs: *mut c_void) -> i32 {
    crate::dlls::kernel32::enter_critical_section(cs);
    STATUS_SUCCESS
}

pub unsafe extern "win64" fn rtl_leave_critical_section(cs: *mut c_void) -> i32 {
    crate::dlls::kernel32::leave_critical_section(cs);
    STATUS_SUCCESS
}

pub unsafe extern "win64" fn rtl_try_enter_critical_section(cs: *mut c_void) -> i32 {
    crate::dlls::kernel32::try_enter_critical_section(cs)
}

pub unsafe extern "win64" fn rtl_delete_critical_section(cs: *mut c_void) -> i32 {
    crate::dlls::kernel32::delete_critical_section(cs);
    STATUS_SUCCESS
}

// ============================================================
//  Pointer encoding / decoding
// ============================================================

static POINTER_COOKIE: OnceLock<usize> = OnceLock::new();

fn pointer_cookie() -> usize {
    *POINTER_COOKIE.get_or_init(|| {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize)
            .unwrap_or(0x5A5A_5A5A_5A5A_5A5A);
        seed ^= seed >> 17;
        seed ^= seed << 31;
        seed ^= seed >> 8;
        if seed == 0 {
            seed = 0xDEAD_BEEF_CAFE_BABE;
        }
        seed
    })
}

pub extern "win64" fn rtl_encode_pointer(ptr: *mut c_void) -> *mut c_void {
    (ptr as usize ^ pointer_cookie()) as *mut c_void
}

pub extern "win64" fn rtl_decode_pointer(ptr: *mut c_void) -> *mut c_void {
    (ptr as usize ^ pointer_cookie()) as *mut c_void
}

// ============================================================
//  SEH : x64 table-based exception handling
// ============================================================

// CONTEXT offsets (x64 Windows) — used by global_asm! and RtlVirtualUnwind
#[allow(dead_code)]
const CTX_FLAGS: usize = 0x30;
#[allow(dead_code)]
const CTX_RAX: usize = 0x78;
#[allow(dead_code)]
const CTX_RCX: usize = 0x80;
#[allow(dead_code)]
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
const CTX_R8: usize = 0xB8;
#[allow(dead_code)]
const CTX_R9: usize = 0xC0;
#[allow(dead_code)]
const CTX_R10: usize = 0xC8;
#[allow(dead_code)]
const CTX_R11: usize = 0xD0;
#[allow(dead_code)]
const CTX_R12: usize = 0xD8;
#[allow(dead_code)]
const CTX_R13: usize = 0xE0;
#[allow(dead_code)]
const CTX_R14: usize = 0xE8;
#[allow(dead_code)]
const CTX_R15: usize = 0xF0;
const CTX_RIP: usize = 0xF8;
const CTX_XMM0: usize = 0x1A0;
pub(crate) const CTX_SIZE: usize = 1232;

#[allow(dead_code)]
const CONTEXT_AMD64: u32 = 0x0010_0000;
#[allow(dead_code)]
const CONTEXT_FULL: u32 = CONTEXT_AMD64 | 0x0F;

const UNW_FLAG_EHANDLER: u8 = 0x01;
const UNW_FLAG_UHANDLER: u8 = 0x02;
const UNW_FLAG_CHAININFO: u8 = 0x04;

fn ctx_gpr_offset(reg: u8) -> usize {
    CTX_RAX + (reg as usize) * 8
}

unsafe fn ctx_get_gpr(ctx: *const u8, reg: u8) -> u64 {
    (ctx.add(ctx_gpr_offset(reg)) as *const u64).read_unaligned()
}
unsafe fn ctx_set_gpr(ctx: *mut u8, reg: u8, val: u64) {
    (ctx.add(ctx_gpr_offset(reg)) as *mut u64).write_unaligned(val);
}
unsafe fn ctx_get_rsp(ctx: *const u8) -> u64 {
    (ctx.add(CTX_RSP) as *const u64).read_unaligned()
}
unsafe fn ctx_set_rsp(ctx: *mut u8, val: u64) {
    (ctx.add(CTX_RSP) as *mut u64).write_unaligned(val);
}
unsafe fn ctx_get_rip(ctx: *const u8) -> u64 {
    (ctx.add(CTX_RIP) as *const u64).read_unaligned()
}
unsafe fn ctx_set_rip(ctx: *mut u8, val: u64) {
    (ctx.add(CTX_RIP) as *mut u64).write_unaligned(val);
}

// `RtlCaptureContext` implemented in assembly to read the caller's registers.
core::arch::global_asm!(
    ".globl winlux_rtl_capture_context",
    ".p2align 4",
    "winlux_rtl_capture_context:",
    // rcx = pointer to CONTEXT (win64 first arg)
    // Zero the whole structure first
    "push rdi",
    "mov rdi, rcx",
    "push rcx",
    "xor eax, eax",
    "mov ecx, 154", // 1232 / 8 = 154
    "rep stosq",
    "pop rcx",
    "pop rdi",
    // ContextFlags = CONTEXT_AMD64 | CONTEXT_FULL
    "mov dword ptr [rcx + 0x30], 0x0010001F",
    // Integer registers
    "mov [rcx + 0x78], rax",
    "mov [rcx + 0x88], rdx",
    "mov [rcx + 0x90], rbx",
    // RSP of caller = our RSP + 8 (return address)
    "lea rax, [rsp + 8]",
    "mov [rcx + 0x98], rax",
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
    // RIP = return address = [rsp]
    "mov rax, [rsp]",
    "mov [rcx + 0xF8], rax",
    // RCX itself was clobbered by the call, store 0
    "mov qword ptr [rcx + 0x80], 0",
    "ret",
);
extern "win64" {
    fn winlux_rtl_capture_context(ctx: *mut c_void);
}
pub unsafe extern "win64" fn rtl_capture_context(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    winlux_rtl_capture_context(ctx);
}

// `RtlRestoreContext` — restore all registers from CONTEXT and jump to RIP.
core::arch::global_asm!(
    ".globl winlux_rtl_restore_context",
    ".p2align 4",
    "winlux_rtl_restore_context:",
    // rcx = pointer to CONTEXT
    // Restore RSP first, push target RIP for ret
    "mov rsp, [rcx + 0x98]",
    "push qword ptr [rcx + 0xF8]",
    // Restore all integer registers
    "mov rax, [rcx + 0x78]",
    "mov rdx, [rcx + 0x88]",
    "mov rbx, [rcx + 0x90]",
    "mov rbp, [rcx + 0xA0]",
    "mov rsi, [rcx + 0xA8]",
    "mov rdi, [rcx + 0xB0]",
    "mov r8,  [rcx + 0xB8]",
    "mov r9,  [rcx + 0xC0]",
    "mov r10, [rcx + 0xC8]",
    "mov r11, [rcx + 0xD0]",
    "mov r12, [rcx + 0xD8]",
    "mov r13, [rcx + 0xE0]",
    "mov r14, [rcx + 0xE8]",
    "mov r15, [rcx + 0xF0]",
    // RCX last (it was our pointer)
    "mov rcx, [rcx + 0x80]",
    "ret",
);
extern "win64" {
    pub(crate) fn winlux_rtl_restore_context(ctx: *mut c_void);
}
pub unsafe extern "win64" fn rtl_restore_context(ctx: *mut c_void, _er: *mut c_void) {
    winlux_rtl_restore_context(ctx);
}

/// `RtlLookupFunctionEntry` : cherche l'entrée RUNTIME_FUNCTION dans le
/// répertoire d'exceptions (.pdata) du module contenant `pc`.
pub unsafe extern "win64" fn rtl_lookup_function_entry(
    pc: u64,
    image_base: *mut u64,
    _history: *mut c_void,
) -> *mut c_void {
    lookup_function_entry_impl(pc, image_base)
}

unsafe fn lookup_function_entry_impl(pc: u64, image_base: *mut u64) -> *mut c_void {
    let Some(m) = crate::loader::find_module_by_addr(pc as usize) else {
        return std::ptr::null_mut();
    };
    let base = m.base;
    if !image_base.is_null() {
        *image_base = base as u64;
    }
    let Some(dir) = m.pe.dirs.get(3) else {
        return std::ptr::null_mut();
    };
    if dir.rva == 0 || dir.size == 0 {
        return std::ptr::null_mut();
    }
    let rva = (pc as usize).wrapping_sub(base) as u32;
    let table = (base + dir.rva as usize) as *const [u32; 3];
    let count = dir.size as usize / 12;
    let mut lo = 0usize;
    let mut hi = count;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let entry = &*table.add(mid);
        if rva < entry[0] {
            hi = mid;
        } else if rva >= entry[1] {
            lo = mid + 1;
        } else {
            return table.add(mid) as *mut c_void;
        }
    }
    std::ptr::null_mut()
}

/// `RtlVirtualUnwind` : déroulement d'une trame x64 à partir des
/// codes de déroulement (.xdata) référencés par la RUNTIME_FUNCTION.
pub unsafe extern "win64" fn rtl_virtual_unwind(
    handler_type: u32,
    image_base: u64,
    control_pc: u64,
    function_entry: *const c_void,
    context_record: *mut c_void,
    handler_data: *mut *mut c_void,
    establisher_frame: *mut u64,
    context_pointers: *mut c_void,
) -> *mut c_void {
    let ctx = context_record as *mut u8;
    let base = image_base as usize;
    let rf = function_entry as *const [u32; 3];
    let mut unwind_rva = (*rf)[2];
    let mut handler: *mut c_void = std::ptr::null_mut();
    let mut last_frame_register: u8 = 0;
    let mut last_frame_base: u64 = 0;

    // Follow CHAININFO chain
    loop {
        let info = (base + unwind_rva as usize) as *const u8;
        let version_flags = info.read();
        let _version = version_flags & 0x07;
        let flags = (version_flags >> 3) & 0x1F;
        let prolog_size = info.add(1).read();
        let code_count = info.add(2).read() as usize;
        let frame_reg_off = info.add(3).read();
        let frame_register = frame_reg_off & 0x0F;
        let frame_offset = ((frame_reg_off >> 4) & 0x0F) as u64 * 16;

        let codes = info.add(4) as *const u16;
        let rva = (control_pc as usize).wrapping_sub(base) as u32;
        let func_start = (*rf)[0];
        let in_prolog = rva < func_start + prolog_size as u32;

        // If there's a frame pointer register, use it as base for stack-relative
        // saves rather than RSP directly (handles variable-size stack frames).
        let frame_base = if frame_register != 0 && !in_prolog {
            ctx_get_gpr(ctx, frame_register).wrapping_sub(frame_offset)
        } else {
            ctx_get_rsp(ctx)
        };

        let mut i = 0usize;
        while i < code_count {
            let code = codes.add(i).read_unaligned();
            let code_offset = (code & 0xFF) as u8;
            let unwind_op = ((code >> 8) & 0x0F) as u8;
            let op_info = ((code >> 12) & 0x0F) as u8;
            i += 1;

            // In the prolog, skip codes past the current position
            if in_prolog && code_offset > (rva - func_start) as u8 {
                match unwind_op {
                    1 if op_info == 0 => {
                        i += 1;
                    }
                    1 if op_info == 1 => {
                        i += 2;
                    }
                    4 | 8 => {
                        i += 1;
                    }
                    5 | 9 => {
                        i += 2;
                    }
                    _ => {}
                }
                continue;
            }

            match unwind_op {
                // UWOP_PUSH_NONVOL
                0 => {
                    let rsp = ctx_get_rsp(ctx);
                    let val = (rsp as *const u64).read_unaligned();
                    ctx_set_gpr(ctx, op_info, val);
                    ctx_set_rsp(ctx, rsp + 8);
                    set_ctx_pointer(context_pointers, op_info, rsp);
                }
                // UWOP_ALLOC_LARGE
                1 => {
                    let size = if op_info == 0 {
                        let slot = codes.add(i).read_unaligned() as u64 * 8;
                        i += 1;
                        slot
                    } else {
                        let lo = codes.add(i).read_unaligned() as u64;
                        let hi = codes.add(i + 1).read_unaligned() as u64;
                        i += 2;
                        lo | (hi << 16)
                    };
                    ctx_set_rsp(ctx, ctx_get_rsp(ctx) + size);
                }
                // UWOP_ALLOC_SMALL
                2 => {
                    ctx_set_rsp(ctx, ctx_get_rsp(ctx) + op_info as u64 * 8 + 8);
                }
                // UWOP_SET_FPREG
                3 => {
                    ctx_set_rsp(
                        ctx,
                        ctx_get_gpr(ctx, frame_register).wrapping_sub(frame_offset),
                    );
                }
                // UWOP_SAVE_NONVOL
                4 => {
                    let offset = codes.add(i).read_unaligned() as u64 * 8;
                    i += 1;
                    let addr = frame_base + offset;
                    ctx_set_gpr(ctx, op_info, (addr as *const u64).read_unaligned());
                    set_ctx_pointer(context_pointers, op_info, addr);
                }
                // UWOP_SAVE_NONVOL_FAR
                5 => {
                    let lo = codes.add(i).read_unaligned() as u64;
                    let hi = codes.add(i + 1).read_unaligned() as u64;
                    i += 2;
                    let offset = lo | (hi << 16);
                    let addr = frame_base + offset;
                    ctx_set_gpr(ctx, op_info, (addr as *const u64).read_unaligned());
                    set_ctx_pointer(context_pointers, op_info, addr);
                }
                // UWOP_SAVE_XMM128
                8 => {
                    let offset = codes.add(i).read_unaligned() as u64 * 16;
                    i += 1;
                    let addr = frame_base + offset;
                    let xmm_off = CTX_XMM0 + op_info as usize * 16;
                    std::ptr::copy_nonoverlapping(addr as *const u8, ctx.add(xmm_off), 16);
                }
                // UWOP_SAVE_XMM128_FAR
                9 => {
                    let lo = codes.add(i).read_unaligned() as u64;
                    let hi = codes.add(i + 1).read_unaligned() as u64;
                    i += 2;
                    let offset = lo | (hi << 16);
                    let addr = frame_base + offset;
                    let xmm_off = CTX_XMM0 + op_info as usize * 16;
                    std::ptr::copy_nonoverlapping(addr as *const u8, ctx.add(xmm_off), 16);
                }
                // UWOP_PUSH_MACHFRAME
                10 => {
                    let rsp = ctx_get_rsp(ctx);
                    let base_off = if op_info != 0 { 8u64 } else { 0u64 };
                    ctx_set_rip(ctx, ((rsp + base_off) as *const u64).read_unaligned());
                    ctx_set_rsp(ctx, ((rsp + base_off + 24) as *const u64).read_unaligned());
                    if !establisher_frame.is_null() {
                        *establisher_frame = ctx_get_rsp(ctx);
                    }
                    return std::ptr::null_mut();
                }
                _ => {
                    // UWOP_EPILOG (6), UWOP_SPARE_CODE (7) — skip
                }
            }
        }

        // Remember frame info from this iteration for establisher_frame
        last_frame_register = frame_register;
        last_frame_base = frame_base;

        // Check for chained unwind info
        if flags & UNW_FLAG_CHAININFO != 0 {
            let aligned = (code_count + 1) & !1;
            let chain = codes.add(aligned) as *const [u32; 3];
            unwind_rva = (*chain)[2];
            continue;
        }

        // Extract exception handler if flags request it
        if (flags & UNW_FLAG_EHANDLER != 0 && handler_type == 1)
            || (flags & UNW_FLAG_UHANDLER != 0 && handler_type == 2)
            || (flags & (UNW_FLAG_EHANDLER | UNW_FLAG_UHANDLER) != 0 && handler_type == 0)
        {
            let aligned = (code_count + 1) & !1;
            let handler_rva = codes.add(aligned).cast::<u32>().read_unaligned();
            handler = (base + handler_rva as usize) as *mut c_void;
            if !handler_data.is_null() {
                *handler_data = codes.add(aligned + 2).cast::<u32>() as *mut c_void;
            }
        } else if !handler_data.is_null() {
            *handler_data = std::ptr::null_mut();
        }
        break;
    }

    // Pop return address → new RIP, adjust RSP
    let rsp = ctx_get_rsp(ctx);
    ctx_set_rip(ctx, (rsp as *const u64).read_unaligned());
    ctx_set_rsp(ctx, rsp + 8);

    if !establisher_frame.is_null() {
        if last_frame_register != 0 {
            // For functions with a frame pointer, the establisher frame is
            // frame_register_value - frame_offset (already computed as frame_base).
            *establisher_frame = last_frame_base;
        } else {
            // For functions without a frame pointer, the establisher frame is
            // RSP at function entry = RSP before we popped the return address.
            *establisher_frame = rsp;
        }
    }

    handler
}

unsafe fn set_ctx_pointer(ptrs: *mut c_void, reg: u8, addr: u64) {
    if ptrs.is_null() || reg > 15 {
        return;
    }
    // KNONVOLATILE_CONTEXT_POINTERS: 16 XMM pointers then 16 integer pointers
    let int_base = (ptrs as *mut u64).add(16);
    int_base.add(reg as usize).write(addr);
}

// EXCEPTION_RECORD layout offsets
pub(crate) const ER_CODE: usize = 0;
pub(crate) const ER_FLAGS: usize = 4;
pub(crate) const ER_RECORD: usize = 8;
pub(crate) const ER_ADDRESS: usize = 16;
pub(crate) const ER_NPARAMS: usize = 24;
pub(crate) const ER_INFO: usize = 32;
pub(crate) const ER_SIZE: usize = 152;

// DISPATCHER_CONTEXT layout (x64)
#[repr(C)]
struct DispatcherContext {
    control_pc: u64,
    image_base: u64,
    function_entry: *const c_void,
    establisher_frame: u64,
    target_ip: u64,
    context_record: *mut c_void,
    language_handler: *const c_void,
    handler_data: *const c_void,
    history_table: *const c_void,
    scope_index: u32,
    _pad: u32,
}

const EXCEPTION_UNWINDING: u32 = 0x02;
const EXCEPTION_TARGET_UNWIND: u32 = 0x20;

type ExceptionHandler = unsafe extern "win64" fn(
    exception_record: *mut c_void,
    establisher_frame: u64,
    context_record: *mut c_void,
    dispatcher_context: *mut DispatcherContext,
) -> i32;

#[allow(dead_code)]
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
const _EXCEPTION_EXECUTE_HANDLER: i32 = 1;

/// `RtlUnwindEx` : déroulement SEH complet.
pub unsafe extern "win64" fn rtl_unwind_ex(
    target_frame: *mut c_void,
    target_ip: *mut c_void,
    exception_record: *mut c_void,
    return_value: *mut c_void,
    original_context: *mut c_void,
    _history: *mut c_void,
) {
    let target_frame_val = target_frame as u64;
    let target_ip_val = target_ip as u64;
    let ctx = original_context as *mut u8;

    // Build or use the exception record
    let mut local_er = [0u8; ER_SIZE];
    let er = if exception_record.is_null() {
        (local_er.as_mut_ptr().add(ER_CODE) as *mut u32).write(0xC0000028); // STATUS_BAD_STACK
        (local_er.as_mut_ptr().add(ER_FLAGS) as *mut u32).write(EXCEPTION_UNWINDING);
        (local_er.as_mut_ptr().add(ER_NPARAMS) as *mut u32).write(0);
        local_er.as_mut_ptr() as *mut c_void
    } else {
        let flags = (exception_record as *mut u8).add(ER_FLAGS) as *mut u32;
        *flags |= EXCEPTION_UNWINDING;
        exception_record
    };

    // Walk frames and call unwind handlers
    let mut dispatch_ctx: DispatcherContext = std::mem::zeroed();
    let max_frames = 256u32;

    for _ in 0..max_frames {
        let rip = ctx_get_rip(ctx);
        let rsp = ctx_get_rsp(ctx);
        if rip == 0 {
            break;
        }

        let mut image_base: u64 = 0;
        let func_entry = lookup_function_entry_impl(rip, &mut image_base);
        if func_entry.is_null() {
            // Leaf function: RIP = [RSP], RSP += 8
            ctx_set_rip(ctx, (rsp as *const u64).read_unaligned());
            ctx_set_rsp(ctx, rsp + 8);
            continue;
        }

        let mut handler_data: *mut c_void = std::ptr::null_mut();
        let mut frame: u64 = 0;
        let handler = rtl_virtual_unwind(
            2, // UNW_FLAG_UHANDLER
            image_base,
            rip,
            func_entry,
            original_context,
            &mut handler_data,
            &mut frame,
            std::ptr::null_mut(),
        );

        // Call unwind handler if present
        if !handler.is_null() {
            // Set TARGET_UNWIND flag at the target frame
            if target_frame_val != 0 && frame == target_frame_val {
                let er_flags = (er as *mut u8).add(ER_FLAGS) as *mut u32;
                *er_flags |= EXCEPTION_TARGET_UNWIND;
            }

            dispatch_ctx.control_pc = rip;
            dispatch_ctx.image_base = image_base;
            dispatch_ctx.function_entry = func_entry;
            dispatch_ctx.establisher_frame = frame;
            dispatch_ctx.target_ip = target_ip_val;
            dispatch_ctx.context_record = original_context;
            dispatch_ctx.language_handler = handler;
            dispatch_ctx.handler_data = handler_data;
            dispatch_ctx.history_table = _history;
            dispatch_ctx.scope_index = 0;

            let handler_fn: ExceptionHandler = std::mem::transmute(handler);
            let _result = handler_fn(er, frame, original_context, &mut dispatch_ctx);
        }

        // At the target frame, restore context (handler set up RIP/RSP/RAX)
        if target_frame_val != 0 && frame == target_frame_val {
            ctx_set_gpr(ctx, 0, return_value as u64);
            winlux_rtl_restore_context(original_context);
        }
    }
}

/// Première passe SEH : cherche un gestionnaire pour l'exception.
/// Retourne `true` si un gestionnaire a été trouvé (et le contrôle a été
/// transféré via `RtlUnwindEx` + `RtlRestoreContext` — ne devrait pas revenir).
pub(crate) unsafe fn dispatch_exception(er: *mut u8, ctx: *mut u8) -> bool {
    let mut search_ctx = [0u8; CTX_SIZE];
    std::ptr::copy_nonoverlapping(ctx, search_ctx.as_mut_ptr(), CTX_SIZE);
    let sctx = search_ctx.as_mut_ptr();

    let mut dispatch_ctx: DispatcherContext = std::mem::zeroed();

    for _ in 0..256u32 {
        let rip = ctx_get_rip(sctx);
        if rip == 0 {
            break;
        }

        let mut image_base: u64 = 0;
        let func_entry = lookup_function_entry_impl(rip, &mut image_base);
        if func_entry.is_null() {
            let rsp = ctx_get_rsp(sctx);
            ctx_set_rip(sctx, (rsp as *const u64).read_unaligned());
            ctx_set_rsp(sctx, rsp + 8);
            continue;
        }

        let mut handler_data: *mut c_void = std::ptr::null_mut();
        let mut frame: u64 = 0;
        let handler = rtl_virtual_unwind(
            UNW_FLAG_EHANDLER as u32,
            image_base,
            rip,
            func_entry,
            sctx as *mut c_void,
            &mut handler_data,
            &mut frame,
            std::ptr::null_mut(),
        );

        if !handler.is_null() {
            dispatch_ctx.control_pc = rip;
            dispatch_ctx.image_base = image_base;
            dispatch_ctx.function_entry = func_entry;
            dispatch_ctx.establisher_frame = frame;
            dispatch_ctx.target_ip = 0;
            dispatch_ctx.context_record = ctx as *mut c_void;
            dispatch_ctx.language_handler = handler;
            dispatch_ctx.handler_data = handler_data;
            dispatch_ctx.history_table = std::ptr::null();
            dispatch_ctx.scope_index = 0;

            let handler_fn: ExceptionHandler = std::mem::transmute(handler);
            let result = handler_fn(
                er as *mut c_void,
                frame,
                ctx as *mut c_void,
                &mut dispatch_ctx,
            );

            if result == _EXCEPTION_EXECUTE_HANDLER {
                let nparams = *(er.add(ER_NPARAMS) as *const u32);
                let return_value = if nparams > 0 {
                    *(er.add(ER_INFO) as *const u64) as usize
                } else {
                    0
                };

                rtl_unwind_ex(
                    frame as *mut c_void,
                    rip as *mut c_void,
                    er as *mut c_void,
                    return_value as *mut c_void,
                    ctx as *mut c_void,
                    std::ptr::null_mut(),
                );
                return true;
            }
        }
    }

    false
}

/// `RtlPcToFileHeader` : trouve le module contenant l'adresse.
pub unsafe extern "win64" fn rtl_pc_to_file_header(
    pc: *const c_void,
    base: *mut *const c_void,
) -> *const c_void {
    if base.is_null() {
        return std::ptr::null();
    }
    let modules = crate::loader::MODULES.lock().unwrap();
    for module in modules.iter() {
        if module.contains(pc as usize) {
            *base = module.base as *const c_void;
            return *base;
        }
    }
    *base = std::ptr::null();
    std::ptr::null()
}

// ============================================================
//  Vectored Exception Handlers
// ============================================================

static VEH_HANDLERS: OnceLock<Mutex<Vec<(usize, usize)>>> = OnceLock::new();
static NEXT_VEH: AtomicUsize = AtomicUsize::new(0x7E00_0001);

pub unsafe extern "win64" fn rtl_add_vectored_exception_handler(
    first: u32,
    handler: usize,
) -> usize {
    if handler == 0 {
        return 0;
    }
    let id = NEXT_VEH.fetch_add(1, Ordering::Relaxed);
    let mut list = lock(VEH_HANDLERS.get_or_init(|| Mutex::new(Vec::new())));
    if first != 0 {
        list.insert(0, (id, handler));
    } else {
        list.push((id, handler));
    }
    id
}

pub unsafe extern "win64" fn rtl_remove_vectored_exception_handler(handle: usize) -> u32 {
    let mut list = lock(VEH_HANDLERS.get_or_init(|| Mutex::new(Vec::new())));
    if let Some(i) = list.iter().position(|&(id, _)| id == handle) {
        list.remove(i);
        1
    } else {
        0
    }
}

// ============================================================
//  Performance counter
// ============================================================

pub unsafe extern "win64" fn rtl_query_performance_counter(counter: *mut i64) -> u8 {
    if counter.is_null() {
        return 0;
    }
    let mut ts: libc::timespec = std::mem::zeroed();
    libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    *counter = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64;
    1
}

pub unsafe extern "win64" fn rtl_query_performance_frequency(freq: *mut i64) -> u8 {
    if freq.is_null() {
        return 0;
    }
    *freq = 1_000_000_000; // nanoseconds
    1
}

// ============================================================
//  SList (Singly-Linked Interlocked Lists) — lock-free stack
// ============================================================

/// SLIST_HEADER sur x64 : 16 octets, deux mots machine.
/// Mot 0 : Alignment (contient Next pointer et Depth)
/// Mot 1 : Region (séquence)
///
/// On simplifie : mot 0 = pointeur vers le premier nœud (ou 0),
/// mot 1 bits [0..15] = profondeur, bits [16..47] = séquence.
pub unsafe extern "win64" fn rtl_initialize_slist_head(head: *mut u8) {
    if head.is_null() {
        return;
    }
    std::ptr::write_bytes(head, 0, 16);
}

pub unsafe extern "win64" fn rtl_interlocked_push_entry_slist(
    head: *mut u8,
    entry: *mut u8,
) -> *mut u8 {
    if head.is_null() || entry.is_null() {
        return std::ptr::null_mut();
    }
    // On sérialise avec un verrou global (SList lock-free réel nécessiterait
    // CMPXCHG16B sur l'en-tête de 128 bits ; ici c'est suffisant).
    slist_push(head, entry)
}

pub unsafe extern "win64" fn rtl_interlocked_pop_entry_slist(head: *mut u8) -> *mut u8 {
    if head.is_null() {
        return std::ptr::null_mut();
    }
    slist_pop(head)
}

pub unsafe extern "win64" fn rtl_interlocked_flush_slist(head: *mut u8) -> *mut u8 {
    if head.is_null() {
        return std::ptr::null_mut();
    }
    slist_flush(head)
}

pub unsafe extern "win64" fn rtl_query_depth_slist(head: *const u8) -> u16 {
    if head.is_null() {
        return 0;
    }
    slist_depth(head)
}

static SLIST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

unsafe fn slist_push(head: *mut u8, entry: *mut u8) -> *mut u8 {
    let _guard = lock(SLIST_LOCK.get_or_init(|| Mutex::new(())));
    let old_next = std::ptr::read_unaligned(head as *const usize);
    // entry.Next = old_next
    std::ptr::write_unaligned(entry as *mut usize, old_next);
    // head.Next = entry
    std::ptr::write_unaligned(head as *mut usize, entry as usize);
    // depth++ (word 1, low 16 bits)
    let meta = std::ptr::read_unaligned(head.add(8) as *const u32);
    let depth = (meta & 0xFFFF) + 1;
    let seq = ((meta >> 16) + 1) & 0xFFFF;
    std::ptr::write_unaligned(head.add(8) as *mut u32, depth | (seq << 16));
    old_next as *mut u8
}

unsafe fn slist_pop(head: *mut u8) -> *mut u8 {
    let _guard = lock(SLIST_LOCK.get_or_init(|| Mutex::new(())));
    let first = std::ptr::read_unaligned(head as *const usize);
    if first == 0 {
        return std::ptr::null_mut();
    }
    let next = std::ptr::read_unaligned(first as *const usize);
    std::ptr::write_unaligned(head as *mut usize, next);
    let meta = std::ptr::read_unaligned(head.add(8) as *const u32);
    let depth = (meta & 0xFFFF).saturating_sub(1);
    let seq = ((meta >> 16) + 1) & 0xFFFF;
    std::ptr::write_unaligned(head.add(8) as *mut u32, depth | (seq << 16));
    first as *mut u8
}

unsafe fn slist_flush(head: *mut u8) -> *mut u8 {
    let _guard = lock(SLIST_LOCK.get_or_init(|| Mutex::new(())));
    let first = std::ptr::read_unaligned(head as *const usize);
    std::ptr::write_unaligned(head as *mut usize, 0usize);
    std::ptr::write_bytes(head.add(8), 0, 8);
    first as *mut u8
}

unsafe fn slist_depth(head: *const u8) -> u16 {
    let meta = std::ptr::read_unaligned(head.add(8) as *const u32);
    (meta & 0xFFFF) as u16
}

// ============================================================
//  SRW Locks (Slim Reader/Writer Locks)
// ============================================================

/// On utilise une table globale indexée par l'adresse du SRWLOCK guest.
/// Le SRWLOCK Windows est un seul pointeur (8 octets sur x64). On ne touche
/// pas à la mémoire guest sauf pour écrire 0 (init) ou un marqueur.
/// SRW Lock implémenté via pthread_rwlock (stable, pas de force_unlock).
/// Chaque adresse SRW lock Win32 est associée à un pthread_rwlock_t alloué en heap.
struct SrwState {
    rwlock: *mut libc::pthread_rwlock_t,
}

unsafe impl Send for SrwState {}
unsafe impl Sync for SrwState {}

impl Drop for SrwState {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_destroy(self.rwlock);
            let layout = std::alloc::Layout::new::<libc::pthread_rwlock_t>();
            std::alloc::dealloc(self.rwlock as *mut u8, layout);
        }
    }
}

impl SrwState {
    fn new() -> Self {
        unsafe {
            let layout = std::alloc::Layout::new::<libc::pthread_rwlock_t>();
            let ptr = std::alloc::alloc_zeroed(layout) as *mut libc::pthread_rwlock_t;
            libc::pthread_rwlock_init(ptr, std::ptr::null());
            SrwState { rwlock: ptr }
        }
    }
}

static SRW_TABLE: OnceLock<Mutex<HashMap<usize, Arc<SrwState>>>> = OnceLock::new();

fn srw_get_or_init(addr: usize) -> Arc<SrwState> {
    let mut table = lock(SRW_TABLE.get_or_init(|| Mutex::new(HashMap::new())));
    table
        .entry(addr)
        .or_insert_with(|| Arc::new(SrwState::new()))
        .clone()
}

pub unsafe extern "win64" fn rtl_initialize_srw_lock(lock_ptr: *mut usize) {
    if lock_ptr.is_null() {
        return;
    }
    *lock_ptr = 0;
    srw_get_or_init(lock_ptr as usize);
}

pub unsafe extern "win64" fn rtl_acquire_srw_lock_exclusive(lock_ptr: *mut usize) {
    if lock_ptr.is_null() {
        return;
    }
    let s = srw_get_or_init(lock_ptr as usize);
    libc::pthread_rwlock_wrlock(s.rwlock);
}

pub unsafe extern "win64" fn rtl_release_srw_lock_exclusive(lock_ptr: *mut usize) {
    if lock_ptr.is_null() {
        return;
    }
    let s = srw_get_or_init(lock_ptr as usize);
    libc::pthread_rwlock_unlock(s.rwlock);
}

pub unsafe extern "win64" fn rtl_acquire_srw_lock_shared(lock_ptr: *mut usize) {
    if lock_ptr.is_null() {
        return;
    }
    let s = srw_get_or_init(lock_ptr as usize);
    libc::pthread_rwlock_rdlock(s.rwlock);
}

pub unsafe extern "win64" fn rtl_release_srw_lock_shared(lock_ptr: *mut usize) {
    if lock_ptr.is_null() {
        return;
    }
    let s = srw_get_or_init(lock_ptr as usize);
    libc::pthread_rwlock_unlock(s.rwlock);
}

pub unsafe extern "win64" fn rtl_try_acquire_srw_lock_exclusive(lock_ptr: *mut usize) -> u8 {
    if lock_ptr.is_null() {
        return 0;
    }
    let s = srw_get_or_init(lock_ptr as usize);
    if libc::pthread_rwlock_trywrlock(s.rwlock) == 0 {
        1
    } else {
        0
    }
}

// ============================================================
//  Condition Variables
// ============================================================

struct CondState {
    condvar: Condvar,
    mutex: Mutex<()>,
}

static COND_TABLE: OnceLock<Mutex<HashMap<usize, Arc<CondState>>>> = OnceLock::new();

fn cond_get_or_init(addr: usize) -> Arc<CondState> {
    let mut table = lock(COND_TABLE.get_or_init(|| Mutex::new(HashMap::new())));
    table
        .entry(addr)
        .or_insert_with(|| {
            Arc::new(CondState {
                condvar: Condvar::new(),
                mutex: Mutex::new(()),
            })
        })
        .clone()
}

pub unsafe extern "win64" fn rtl_initialize_condition_variable(cv: *mut usize) {
    if cv.is_null() {
        return;
    }
    *cv = 0;
    cond_get_or_init(cv as usize);
}

/// `RtlSleepConditionVariableSRW` : attend la condition en relâchant le SRW lock.
pub unsafe extern "win64" fn rtl_sleep_condition_variable_srw(
    cv: *mut usize,
    srw: *mut usize,
    timeout: u32,
    _flags: u32,
) -> i32 {
    if cv.is_null() || srw.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // Relâcher le SRW lock (exclusif par défaut).
    rtl_release_srw_lock_exclusive(srw);
    let c = cond_get_or_init(cv as usize);
    let guard = lock(&c.mutex);
    let result = if timeout == u32::MAX {
        let _guard = c.condvar.wait(guard).unwrap_or_else(|e| e.into_inner());
        STATUS_SUCCESS
    } else {
        match c
            .condvar
            .wait_timeout(guard, std::time::Duration::from_millis(timeout as u64))
        {
            Ok((_, result)) => {
                if result.timed_out() {
                    STATUS_TIMEOUT
                } else {
                    STATUS_SUCCESS
                }
            }
            Err(_) => STATUS_SUCCESS,
        }
    };
    // Reprendre le SRW lock.
    rtl_acquire_srw_lock_exclusive(srw);
    result
}

pub unsafe extern "win64" fn rtl_wake_condition_variable(cv: *mut usize) {
    if cv.is_null() {
        return;
    }
    let c = cond_get_or_init(cv as usize);
    c.condvar.notify_one();
}

pub unsafe extern "win64" fn rtl_wake_all_condition_variable(cv: *mut usize) {
    if cv.is_null() {
        return;
    }
    let c = cond_get_or_init(cv as usize);
    c.condvar.notify_all();
}

// ============================================================
//  Fiber Local Storage (FLS)
// ============================================================

static FLS_NEXT: AtomicU32 = AtomicU32::new(0);
static FLS_DATA: OnceLock<Mutex<HashMap<(u64, u32), usize>>> = OnceLock::new();
// Thread-id + slot → value.

fn fls_thread_key() -> u64 {
    // Utilise l'id du thread comme clé.
    let id = std::thread::current().id();
    // Extraire un u64 de ThreadId via Debug (hack stable).
    let s = format!("{id:?}");
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

/// `RtlFlsAlloc` : alloue un slot FLS, renvoie son index.
pub unsafe extern "win64" fn rtl_fls_alloc(_callback: usize, index: *mut u32) -> i32 {
    if index.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let slot = FLS_NEXT.fetch_add(1, Ordering::Relaxed);
    if slot >= 4096 {
        return 0xC000_0017u32 as i32;
    } // STATUS_NO_MEMORY
    *index = slot;
    STATUS_SUCCESS
}

/// `RtlFlsFree` : libère un slot FLS.
pub unsafe extern "win64" fn rtl_fls_free(index: u32) -> i32 {
    let mut table = lock(FLS_DATA.get_or_init(|| Mutex::new(HashMap::new())));
    table.retain(|&(_, s), _| s != index);
    STATUS_SUCCESS
}

/// `RtlFlsGetValue` : lit la valeur FLS du thread courant.
pub unsafe extern "win64" fn rtl_fls_get_value(index: u32) -> usize {
    let key = (fls_thread_key(), index);
    let table = lock(FLS_DATA.get_or_init(|| Mutex::new(HashMap::new())));
    table.get(&key).copied().unwrap_or(0)
}

/// `RtlFlsSetValue` : écrit la valeur FLS du thread courant.
pub unsafe extern "win64" fn rtl_fls_set_value(index: u32, value: usize) -> i32 {
    let key = (fls_thread_key(), index);
    let mut table = lock(FLS_DATA.get_or_init(|| Mutex::new(HashMap::new())));
    table.insert(key, value);
    STATUS_SUCCESS
}

// ============================================================
//  Misc Rtl
// ============================================================

/// `RtlSetProcessIsCritical` : stub (marque le processus comme critique).
pub unsafe extern "win64" fn rtl_set_process_is_critical(
    _new: u8,
    _old: *mut u8,
    _need_scb: u8,
) -> i32 {
    STATUS_SUCCESS
}

// ============================================================
//  Nt* functions — noyau NT
// ============================================================

/// `NtClose` : ferme un handle du noyau.
pub unsafe extern "win64" fn nt_close(handle: usize) -> i32 {
    crate::dlls::kernel32::close_handle(handle);
    STATUS_SUCCESS
}

/// Extract a Windows path from OBJECT_ATTRIBUTES (offset 0x10 → UNICODE_STRING).
unsafe fn oa_path(attrs: *const c_void) -> Option<String> {
    if attrs.is_null() {
        return None;
    }
    let obj_name_ptr = *((attrs as *const u8).add(0x10) as *const *const u8);
    if obj_name_ptr.is_null() {
        return None;
    }
    let len = ustr_len(obj_name_ptr) as usize / 2;
    let buf = ustr_buf_w(obj_name_ptr);
    if buf.is_null() || len == 0 {
        return None;
    }
    let wchars = std::slice::from_raw_parts(buf, len);
    let s = String::from_utf16_lossy(wchars);
    // Strip NT prefixes: \??\ , \DosDevices\ , \\?\
    let s = s.strip_prefix("\\??\\").unwrap_or(&s);
    let s = s.strip_prefix("\\DosDevices\\").unwrap_or(s);
    let s = s.strip_prefix("\\\\?\\").unwrap_or(s);
    Some(s.to_string())
}

/// Map NT disposition to Rust OpenOptions.
unsafe fn apply_disposition(o: &mut std::fs::OpenOptions, disp: u32) {
    match disp {
        0 => {
            /* FILE_SUPERSEDE: create or overwrite */
            o.create(true).truncate(true);
        }
        1 => { /* FILE_OPEN: open existing */ }
        2 => {
            /* FILE_CREATE: create new, fail if exists */
            o.create_new(true);
        }
        3 => {
            /* FILE_OPEN_IF: open or create */
            o.create(true);
        }
        4 => {
            /* FILE_OVERWRITE: open existing, truncate */
            o.truncate(true);
        }
        5 => {
            /* FILE_OVERWRITE_IF: create or overwrite */
            o.create(true).truncate(true);
        }
        _ => {}
    }
}

/// `NtCreateFile` : ouvre ou crée un fichier via un chemin NT.
pub unsafe extern "win64" fn nt_create_file(
    handle: *mut usize,
    access: u32,
    attrs: *const c_void,
    io: *mut c_void,
    _alloc: *const c_void,
    _file_attrs: u32,
    _share: u32,
    disposition: u32,
    _options: u32,
    _ea: *const c_void,
    _ea_len: u32,
) -> i32 {
    if handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(win_path) = oa_path(attrs) else {
        return STATUS_INVALID_PARAMETER;
    };
    let linux_path = crate::paths::to_linux(&win_path);
    let want_read = access & 0x8000_0000 != 0 || access & 1 != 0 || access & 0x0008_0000 != 0;
    let want_write = access & 0x4000_0000 != 0 || access & 2 != 0 || access & 0x0004_0000 != 0;
    let mut o = std::fs::OpenOptions::new();
    o.read(want_read || !want_write).write(want_write);
    apply_disposition(&mut o, disposition);
    match o.open(&linux_path) {
        Ok(f) => {
            let h = super::kernel32::insert_file(f);
            *handle = h;
            if !io.is_null() {
                // IO_STATUS_BLOCK: Status = 0, Information = disposition action
                (io as *mut u64).write(0);
                (io as *mut u64).add(1).write(disposition as u64);
            }
            STATUS_SUCCESS
        }
        Err(e) => {
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
                ErrorKind::AlreadyExists => STATUS_OBJECT_NAME_COLLISION,
                ErrorKind::PermissionDenied => STATUS_ACCESS_DENIED,
                _ => STATUS_UNSUCCESSFUL,
            }
        }
    }
}

/// `NtOpenFile` : ouvre un fichier existant via un chemin NT.
pub unsafe extern "win64" fn nt_open_file(
    handle: *mut usize,
    access: u32,
    attrs: *const c_void,
    io: *mut c_void,
    _share: u32,
    _options: u32,
) -> i32 {
    nt_create_file(
        handle,
        access,
        attrs,
        io,
        std::ptr::null(),
        0, // file_attrs
        0, // share
        1, // FILE_OPEN (open existing only)
        _options,
        std::ptr::null(),
        0,
    )
}

/// `NtQueryInformationProcess` : stub minimal.
pub unsafe extern "win64" fn nt_query_information_process(
    _handle: usize,
    info_class: u32,
    info: *mut c_void,
    length: u32,
    return_length: *mut u32,
) -> i32 {
    match info_class {
        0 => {
            // ProcessBasicInformation : on renvoie le PEB et le PID.
            if length < 48 {
                return STATUS_BUFFER_TOO_SMALL;
            }
            if info.is_null() {
                return STATUS_INVALID_PARAMETER;
            }
            std::ptr::write_bytes(info as *mut u8, 0, 48);
            // Offset 8 : PebBaseAddress
            std::ptr::write_unaligned(
                (info as *mut u8).add(8) as *mut usize,
                crate::process::peb(),
            );
            // Offset 16 : UniqueProcessId
            std::ptr::write_unaligned(
                (info as *mut u8).add(16) as *mut usize,
                std::process::id() as usize,
            );
            if !return_length.is_null() {
                *return_length = 48;
            }
            STATUS_SUCCESS
        }
        7 => {
            // ProcessDebugPort : 0 = pas de débogueur.
            if length < 8 {
                return STATUS_BUFFER_TOO_SMALL;
            }
            if info.is_null() {
                return STATUS_INVALID_PARAMETER;
            }
            std::ptr::write_unaligned(info as *mut usize, 0);
            if !return_length.is_null() {
                *return_length = 8;
            }
            STATUS_SUCCESS
        }
        29 => {
            // ProcessImageFileName : stub avec un chemin factice.
            if info.is_null() || length < 16 {
                return STATUS_BUFFER_TOO_SMALL;
            }
            std::ptr::write_bytes(info as *mut u8, 0, length as usize);
            if !return_length.is_null() {
                *return_length = 0;
            }
            STATUS_SUCCESS
        }
        _ => STATUS_NOT_IMPLEMENTED,
    }
}

/// `NtQueryInformationThread` : stub.
pub unsafe extern "win64" fn nt_query_information_thread(
    _handle: usize,
    _info_class: u32,
    _info: *mut c_void,
    _length: u32,
    _return_length: *mut u32,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// `NtSetInformationThread` : stub (souvent appelé pour changer la priorité).
pub unsafe extern "win64" fn nt_set_information_thread(
    _handle: usize,
    _info_class: u32,
    _info: *const c_void,
    _length: u32,
) -> i32 {
    STATUS_SUCCESS // Silently succeed
}

/// `NtQuerySystemInformation` : stub.
pub unsafe extern "win64" fn nt_query_system_information(
    info_class: u32,
    info: *mut c_void,
    length: u32,
    return_length: *mut u32,
) -> i32 {
    match info_class {
        0 => {
            // SystemBasicInformation : taille de page et nombre de processeurs.
            if length < 44 {
                return STATUS_BUFFER_TOO_SMALL;
            }
            if info.is_null() {
                return STATUS_INVALID_PARAMETER;
            }
            std::ptr::write_bytes(info as *mut u8, 0, 44);
            // PageSize à offset 0 (pour certains layouts)
            std::ptr::write_unaligned(info as *mut u32, 4096);
            if !return_length.is_null() {
                *return_length = 44;
            }
            STATUS_SUCCESS
        }
        _ => {
            if !return_length.is_null() {
                *return_length = 0;
            }
            STATUS_NOT_IMPLEMENTED
        }
    }
}

/// `NtQueryVirtualMemory` : MemoryBasicInformation (classe 0) via /proc/self/maps.
pub unsafe extern "win64" fn nt_query_virtual_memory(
    _handle: usize,
    addr: *const c_void,
    info_class: u32,
    info: *mut c_void,
    length: usize,
    return_length: *mut usize,
) -> i32 {
    if info_class != 0 {
        return STATUS_NOT_IMPLEMENTED;
    }
    const MBI_SIZE: usize = 48;
    if info.is_null() || length < MBI_SIZE {
        return STATUS_BUFFER_TOO_SMALL;
    }
    let target = addr as usize;
    let buf = info as *mut u8;
    std::ptr::write_bytes(buf, 0, MBI_SIZE);
    if let Ok(maps) = std::fs::read_to_string("/proc/self/maps") {
        for line in maps.lines() {
            let Some((range, rest)) = line.split_once(' ') else {
                continue;
            };
            let Some((start_s, end_s)) = range.split_once('-') else {
                continue;
            };
            let Ok(start) = usize::from_str_radix(start_s, 16) else {
                continue;
            };
            let Ok(end) = usize::from_str_radix(end_s, 16) else {
                continue;
            };
            if target >= start && target < end {
                let perms = &rest[..4.min(rest.len())];
                let protect: u32 = match (
                    perms.contains('r'),
                    perms.contains('w'),
                    perms.contains('x'),
                ) {
                    (true, true, true) => 0x40,   // PAGE_EXECUTE_READWRITE
                    (true, true, false) => 0x04,  // PAGE_READWRITE
                    (true, false, true) => 0x20,  // PAGE_EXECUTE_READ
                    (true, false, false) => 0x02, // PAGE_READONLY
                    _ => 0x01,                    // PAGE_NOACCESS
                };
                // MEMORY_BASIC_INFORMATION layout (x64):
                // 0: BaseAddress (8), 8: AllocationBase (8), 16: AllocationProtect (4),
                // 20: PartitionId (2), 22: pad (2), 24: RegionSize (8),
                // 32: State (4), 36: Protect (4), 40: Type (4)
                std::ptr::write_unaligned(buf as *mut usize, start);
                std::ptr::write_unaligned(buf.add(8) as *mut usize, start);
                std::ptr::write_unaligned(buf.add(16) as *mut u32, protect);
                std::ptr::write_unaligned(buf.add(24) as *mut usize, end - start);
                std::ptr::write_unaligned(buf.add(32) as *mut u32, 0x1000); // MEM_COMMIT
                std::ptr::write_unaligned(buf.add(36) as *mut u32, protect);
                std::ptr::write_unaligned(buf.add(40) as *mut u32, 0x20000); // MEM_PRIVATE
                if !return_length.is_null() {
                    *return_length = MBI_SIZE;
                }
                return STATUS_SUCCESS;
            }
        }
    }
    // Address not in any mapping: free region
    std::ptr::write_unaligned(buf as *mut usize, target & !0xFFF);
    std::ptr::write_unaligned(buf.add(24) as *mut usize, 4096);
    std::ptr::write_unaligned(buf.add(32) as *mut u32, 0x10000); // MEM_FREE
    if !return_length.is_null() {
        *return_length = MBI_SIZE;
    }
    STATUS_SUCCESS
}

/// `NtDelayExecution` : dort pendant le délai spécifié.
/// Le délai est en unités de 100 ns ; un délai négatif est relatif.
pub unsafe extern "win64" fn nt_delay_execution(_alertable: u8, delay: *const i64) -> i32 {
    if delay.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let d = *delay;
    let ms = if d < 0 {
        ((-d) / 10_000) as u64
    } else {
        (d / 10_000) as u64
    };
    std::thread::sleep(std::time::Duration::from_millis(ms));
    STATUS_SUCCESS
}

/// `NtAllocateVirtualMemory` : délègue à VirtualAlloc.
pub unsafe extern "win64" fn nt_allocate_virtual_memory(
    _process: usize,
    base: *mut *mut c_void,
    _zero_bits: usize,
    size: *mut usize,
    alloc_type: u32,
    protect: u32,
) -> i32 {
    if base.is_null() || size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let result = crate::dlls::kernel32::virtual_alloc(*base, *size, alloc_type, protect);
    if result.is_null() {
        return 0xC000_0017u32 as i32;
    } // STATUS_NO_MEMORY
    *base = result;
    STATUS_SUCCESS
}

/// `NtFreeVirtualMemory` : délègue à VirtualFree.
pub unsafe extern "win64" fn nt_free_virtual_memory(
    _process: usize,
    base: *mut *mut c_void,
    size: *mut usize,
    free_type: u32,
) -> i32 {
    if base.is_null() || size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let result = crate::dlls::kernel32::virtual_free(*base, *size, free_type);
    if result != 0 {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_PARAMETER
    }
}

/// `NtProtectVirtualMemory` : stub.
pub unsafe extern "win64" fn nt_protect_virtual_memory(
    _process: usize,
    _base: *mut *mut c_void,
    _size: *mut usize,
    _new_protect: u32,
    _old_protect: *mut u32,
) -> i32 {
    STATUS_SUCCESS // Pretend it worked
}

/// `NtReadVirtualMemory` : copie de la mémoire du processus courant.
pub unsafe extern "win64" fn nt_read_virtual_memory(
    _process: usize,
    base: *const c_void,
    buffer: *mut c_void,
    size: usize,
    bytes_read: *mut usize,
) -> i32 {
    if base.is_null() || buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    std::ptr::copy_nonoverlapping(base as *const u8, buffer as *mut u8, size);
    if !bytes_read.is_null() {
        *bytes_read = size;
    }
    STATUS_SUCCESS
}

/// `NtWriteVirtualMemory` : écriture dans la mémoire du processus courant.
pub unsafe extern "win64" fn nt_write_virtual_memory(
    _process: usize,
    base: *mut c_void,
    buffer: *const c_void,
    size: usize,
    bytes_written: *mut usize,
) -> i32 {
    if base.is_null() || buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    std::ptr::copy_nonoverlapping(buffer as *const u8, base as *mut u8, size);
    if !bytes_written.is_null() {
        *bytes_written = size;
    }
    STATUS_SUCCESS
}

// ---------- Section views tracking ----------

static SECTION_VIEWS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();

/// `NtCreateSection` : crée un objet section (mémoire partagée ou fichier mappé).
pub unsafe extern "win64" fn nt_create_section(
    handle: *mut usize,
    _access: u32,
    _attrs: *const c_void,
    max_size: *const c_void,
    protect: u32,
    _alloc: u32,
    file: usize,
) -> i32 {
    if handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let size = if max_size.is_null() {
        0u64
    } else {
        std::ptr::read_unaligned(max_size as *const u64)
    };
    let fd = if file == 0 || file == usize::MAX {
        None
    } else {
        let raw = crate::dlls::kernel32::handle_to_fd(file);
        if raw < 0 {
            return STATUS_INVALID_HANDLE;
        }
        Some(raw)
    };
    let section = Arc::new(crate::dlls::kernel32::SectionObject {
        fd,
        max_size: size,
        protect,
    });
    let h =
        crate::dlls::kernel32::add_sync(crate::dlls::kernel32::SyncHandle::Section(section), None);
    *handle = h;
    STATUS_SUCCESS
}

/// `NtMapViewOfSection` : mappe une vue de la section dans l'espace d'adressage.
pub unsafe extern "win64" fn nt_map_view_of_section(
    section: usize,
    _process: usize,
    base: *mut *mut c_void,
    _zero_bits: usize,
    _commit: usize,
    offset_ptr: *mut c_void,
    size: *mut usize,
    _inherit: u32,
    _alloc: u32,
    protect: u32,
) -> i32 {
    if base.is_null() || size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(crate::dlls::kernel32::SyncHandle::Section(sec)) =
        crate::dlls::kernel32::lookup_sync(section)
    else {
        return STATUS_INVALID_HANDLE;
    };
    let offset = if offset_ptr.is_null() {
        0u64
    } else {
        std::ptr::read_unaligned(offset_ptr as *const u64)
    };
    let mut map_size = *size;
    if map_size == 0 {
        if sec.max_size > 0 {
            map_size = (sec.max_size - offset) as usize;
        } else if let Some(fd) = sec.fd {
            let mut stat: libc::stat = std::mem::zeroed();
            if libc::fstat(fd, &mut stat) == 0 && stat.st_size > offset as i64 {
                map_size = (stat.st_size as u64 - offset) as usize;
            }
        }
        if map_size == 0 {
            map_size = 4096;
        }
    }
    let prot = match protect & 0xFF {
        0x02 => libc::PROT_READ,                    // PAGE_READONLY
        0x04 => libc::PROT_READ | libc::PROT_WRITE, // PAGE_READWRITE
        0x08 => libc::PROT_READ | libc::PROT_WRITE, // PAGE_WRITECOPY
        0x20 => libc::PROT_READ | libc::PROT_EXEC,  // PAGE_EXECUTE_READ
        0x40 => libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
        _ => libc::PROT_READ | libc::PROT_WRITE,
    };
    let (flags, mfd) = if let Some(fd) = sec.fd {
        (libc::MAP_SHARED, fd)
    } else {
        (libc::MAP_SHARED | libc::MAP_ANONYMOUS, -1)
    };
    let hint = *base;
    let addr = libc::mmap(hint, map_size, prot, flags, mfd, offset as libc::off_t);
    if addr == libc::MAP_FAILED {
        return 0xC000_0017u32 as i32; // STATUS_NO_MEMORY
    }
    *base = addr;
    *size = map_size;
    let mut views = lock(SECTION_VIEWS.get_or_init(|| Mutex::new(HashMap::new())));
    views.insert(addr as usize, map_size);
    STATUS_SUCCESS
}

/// `NtUnmapViewOfSection` : libère la vue précédemment mappée.
pub unsafe extern "win64" fn nt_unmap_view_of_section(_process: usize, base: *mut c_void) -> i32 {
    if base.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let size = {
        let mut views = lock(SECTION_VIEWS.get_or_init(|| Mutex::new(HashMap::new())));
        views.remove(&(base as usize)).unwrap_or(0)
    };
    if size > 0 {
        libc::munmap(base, size);
    }
    STATUS_SUCCESS
}

/// `NtQueryPerformanceCounter` : horloge monotone haute résolution.
pub unsafe extern "win64" fn nt_query_performance_counter(
    counter: *mut i64,
    frequency: *mut i64,
) -> i32 {
    if counter.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let mut ts: libc::timespec = std::mem::zeroed();
    libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    *counter = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64;
    if !frequency.is_null() {
        *frequency = 1_000_000_000;
    }
    STATUS_SUCCESS
}

/// `NtTerminateProcess` : termine le processus courant.
pub unsafe extern "win64" fn nt_terminate_process(_handle: usize, code: u32) -> i32 {
    crate::dlls::kernel32::flush_and_exit(code as i32)
}

/// `NtDuplicateObject` : duplique un handle dans le même processus.
pub unsafe extern "win64" fn nt_duplicate_object(
    _src_process: usize,
    src_handle: usize,
    _tgt_process: usize,
    tgt_handle: *mut usize,
    _access: u32,
    _attrs: u32,
    options: u32,
) -> i32 {
    const DUPLICATE_CLOSE_SOURCE: u32 = 0x01;
    if tgt_handle.is_null() && options & DUPLICATE_CLOSE_SOURCE == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    if let Some(obj) = crate::dlls::kernel32::lookup_sync(src_handle) {
        if !tgt_handle.is_null() {
            let new_h = crate::dlls::kernel32::add_sync(obj, None);
            *tgt_handle = new_h;
        }
        if options & DUPLICATE_CLOSE_SOURCE != 0 {
            crate::dlls::kernel32::close_handle(src_handle);
        }
        return STATUS_SUCCESS;
    }
    let dup_result = crate::dlls::kernel32::duplicate_handle_impl(src_handle);
    if let Some(new_h) = dup_result {
        if !tgt_handle.is_null() {
            *tgt_handle = new_h;
        }
        if options & DUPLICATE_CLOSE_SOURCE != 0 {
            crate::dlls::kernel32::close_handle(src_handle);
        }
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_HANDLE
    }
}

/// `NtCreateEvent` : stub.
pub unsafe extern "win64" fn nt_create_event(
    handle: *mut usize,
    _access: u32,
    _attrs: *const c_void,
    event_type: u32,
    initial_state: u8,
) -> i32 {
    if handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let h = crate::dlls::kernel32::create_event_a(
        0,
        if event_type == 1 { 1 } else { 0 },
        initial_state as i32,
        std::ptr::null(),
    );
    if h == 0 {
        return STATUS_INVALID_HANDLE;
    }
    *handle = h;
    STATUS_SUCCESS
}

/// `NtSetEvent` : signale un événement.
pub unsafe extern "win64" fn nt_set_event(handle: usize, _previous: *mut i32) -> i32 {
    if crate::dlls::kernel32::set_event(handle) != 0 {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_HANDLE
    }
}

/// `NtResetEvent` : remet l'événement à non signalé.
pub unsafe extern "win64" fn nt_reset_event(handle: usize, _previous: *mut i32) -> i32 {
    if crate::dlls::kernel32::reset_event(handle) != 0 {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_HANDLE
    }
}

/// `NtWaitForSingleObject` : attend un objet du noyau.
pub unsafe extern "win64" fn nt_wait_for_single_object(
    handle: usize,
    _alertable: u8,
    timeout: *const i64,
) -> i32 {
    let ms = if timeout.is_null() {
        u32::MAX
    } else {
        let d = *timeout;
        if d < 0 {
            ((-d) / 10_000).min(u32::MAX as i64) as u32
        } else if d == 0 {
            0
        } else {
            (d / 10_000).min(u32::MAX as i64) as u32
        }
    };
    let result = crate::dlls::kernel32::wait_for_single_object(handle, ms);
    match result {
        0 => STATUS_SUCCESS,     // WAIT_OBJECT_0
        0x102 => STATUS_TIMEOUT, // WAIT_TIMEOUT
        _ => STATUS_INVALID_HANDLE,
    }
}

/// `NtFlushBuffersFile` : stub (flush un fichier par handle).
pub unsafe extern "win64" fn nt_flush_buffers_file(_handle: usize, _io: *mut c_void) -> i32 {
    STATUS_SUCCESS
}

/// `NtQueryAttributesFile` : récupère les attributs basiques d'un fichier.
pub unsafe extern "win64" fn nt_query_attributes_file(
    attrs: *const c_void,
    info: *mut c_void,
) -> i32 {
    let Some(win_path) = oa_path(attrs) else {
        return STATUS_INVALID_PARAMETER;
    };
    let linux_path = crate::paths::to_linux(&win_path);
    let meta = match std::fs::metadata(&linux_path) {
        Ok(m) => m,
        Err(_) => return STATUS_OBJECT_NAME_NOT_FOUND,
    };
    if !info.is_null() {
        // FILE_BASIC_INFORMATION: 40 bytes
        // CreationTime(8), LastAccessTime(8), LastWriteTime(8), ChangeTime(8), FileAttributes(4)
        let buf = info as *mut u8;
        std::ptr::write_bytes(buf, 0, 40);
        let file_attrs: u32 = if meta.is_dir() {
            0x10 // FILE_ATTRIBUTE_DIRECTORY
        } else {
            0x80 // FILE_ATTRIBUTE_NORMAL
        };
        std::ptr::write_unaligned(buf.add(32) as *mut u32, file_attrs);
    }
    STATUS_SUCCESS
}

/// `NtRaiseHardError` : affiche un message d'erreur fatal.
pub unsafe extern "win64" fn nt_raise_hard_error(
    status: i32,
    _num_params: u32,
    _unicode_mask: u32,
    _params: *const usize,
    _option: u32,
    _response: *mut u32,
) -> i32 {
    eprintln!("winlux: NtRaiseHardError status=0x{:08X}", status as u32);
    STATUS_SUCCESS
}

// ============================================================
//  Table d'exports
// ============================================================

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        // --- Rtl : existants ---
        "RtlGetCurrentPeb" => fn rtl_get_current_peb,
        "RtlGetCurrentTeb" => fn rtl_get_current_teb,
        "RtlGetLastWin32Error" => fn rtl_get_last_win32_error,
        "RtlSetLastWin32Error" => fn rtl_set_last_win32_error,
        "RtlGetVersion" => fn rtl_get_version,
        "RtlNtStatusToDosError" => fn rtl_nt_status_to_dos_error,
        "RtlAllocateHeap" => fn rtl_allocate_heap,
        "RtlFreeHeap" => fn rtl_free_heap,
        "RtlReAllocateHeap" => fn rtl_reallocate_heap,
        "RtlInitUnicodeString" => fn rtl_init_unicode_string,
        "RtlRandomEx" => fn rtl_random_ex,
        // --- Rtl : strings ---
        "RtlInitAnsiString" => fn rtl_init_ansi_string,
        "RtlFreeAnsiString" => fn rtl_free_ansi_string,
        "RtlFreeUnicodeString" => fn rtl_free_unicode_string,
        "RtlAnsiStringToUnicodeString" => fn rtl_ansi_string_to_unicode_string,
        "RtlUnicodeStringToAnsiString" => fn rtl_unicode_string_to_ansi_string,
        "RtlUnicodeToMultiByteSize" => fn rtl_unicode_to_multibyte_size,
        "RtlMultiByteToUnicodeSize" => fn rtl_multibyte_to_unicode_size,
        "RtlCompareUnicodeString" => fn rtl_compare_unicode_string,
        "RtlEqualUnicodeString" => fn rtl_equal_unicode_string,
        "RtlCopyUnicodeString" => fn rtl_copy_unicode_string,
        "RtlAppendUnicodeStringToString" => fn rtl_append_unicode_string_to_string,
        "RtlHashUnicodeString" => fn rtl_hash_unicode_string,
        // --- Rtl : heap ---
        "RtlCreateHeap" => fn rtl_create_heap,
        "RtlDestroyHeap" => fn rtl_destroy_heap,
        "RtlSizeHeap" => fn rtl_size_heap,
        // --- Rtl : critical sections ---
        "RtlInitializeCriticalSection" => fn rtl_initialize_critical_section,
        "RtlInitializeCriticalSectionEx" => fn rtl_initialize_critical_section_ex,
        "RtlEnterCriticalSection" => fn rtl_enter_critical_section,
        "RtlLeaveCriticalSection" => fn rtl_leave_critical_section,
        "RtlTryEnterCriticalSection" => fn rtl_try_enter_critical_section,
        "RtlDeleteCriticalSection" => fn rtl_delete_critical_section,
        // --- Rtl : pointer encoding ---
        "RtlEncodePointer" => fn rtl_encode_pointer,
        "RtlDecodePointer" => fn rtl_decode_pointer,
        // --- Rtl : SEH ---
        "RtlCaptureContext" => fn rtl_capture_context,
        "RtlLookupFunctionEntry" => fn rtl_lookup_function_entry,
        "RtlVirtualUnwind" => fn rtl_virtual_unwind,
        "RtlUnwindEx" => fn rtl_unwind_ex,
        "RtlRestoreContext" => fn rtl_restore_context,
        "RtlPcToFileHeader" => fn rtl_pc_to_file_header,
        // --- Rtl : VEH ---
        "RtlAddVectoredExceptionHandler" => fn rtl_add_vectored_exception_handler,
        "RtlRemoveVectoredExceptionHandler" => fn rtl_remove_vectored_exception_handler,
        // --- Rtl : performance counter ---
        "RtlQueryPerformanceCounter" => fn rtl_query_performance_counter,
        "RtlQueryPerformanceFrequency" => fn rtl_query_performance_frequency,
        // --- Rtl : SList ---
        "RtlInitializeSListHead" => fn rtl_initialize_slist_head,
        "RtlInterlockedPushEntrySList" => fn rtl_interlocked_push_entry_slist,
        "RtlInterlockedPopEntrySList" => fn rtl_interlocked_pop_entry_slist,
        "RtlInterlockedFlushSList" => fn rtl_interlocked_flush_slist,
        "RtlQueryDepthSList" => fn rtl_query_depth_slist,
        // --- Rtl : SRW Locks ---
        "RtlInitializeSRWLock" => fn rtl_initialize_srw_lock,
        "RtlAcquireSRWLockExclusive" => fn rtl_acquire_srw_lock_exclusive,
        "RtlReleaseSRWLockExclusive" => fn rtl_release_srw_lock_exclusive,
        "RtlAcquireSRWLockShared" => fn rtl_acquire_srw_lock_shared,
        "RtlReleaseSRWLockShared" => fn rtl_release_srw_lock_shared,
        "RtlTryAcquireSRWLockExclusive" => fn rtl_try_acquire_srw_lock_exclusive,
        // --- Rtl : Condition Variables ---
        "RtlInitializeConditionVariable" => fn rtl_initialize_condition_variable,
        "RtlSleepConditionVariableSRW" => fn rtl_sleep_condition_variable_srw,
        "RtlWakeConditionVariable" => fn rtl_wake_condition_variable,
        "RtlWakeAllConditionVariable" => fn rtl_wake_all_condition_variable,
        // --- Rtl : FLS ---
        "RtlFlsAlloc" => fn rtl_fls_alloc,
        "RtlFlsFree" => fn rtl_fls_free,
        "RtlFlsGetValue" => fn rtl_fls_get_value,
        "RtlFlsSetValue" => fn rtl_fls_set_value,
        // --- Rtl : misc ---
        "RtlSetProcessIsCritical" => fn rtl_set_process_is_critical,
        // --- Nt* ---
        "NtClose" => fn nt_close,
        "NtCreateFile" => fn nt_create_file,
        "NtOpenFile" => fn nt_open_file,
        "NtQueryInformationProcess" => fn nt_query_information_process,
        "NtQueryInformationThread" => fn nt_query_information_thread,
        "NtSetInformationThread" => fn nt_set_information_thread,
        "NtQuerySystemInformation" => fn nt_query_system_information,
        "NtQueryVirtualMemory" => fn nt_query_virtual_memory,
        "NtDelayExecution" => fn nt_delay_execution,
        "NtAllocateVirtualMemory" => fn nt_allocate_virtual_memory,
        "NtFreeVirtualMemory" => fn nt_free_virtual_memory,
        "NtProtectVirtualMemory" => fn nt_protect_virtual_memory,
        "NtReadVirtualMemory" => fn nt_read_virtual_memory,
        "NtWriteVirtualMemory" => fn nt_write_virtual_memory,
        "NtCreateSection" => fn nt_create_section,
        "NtMapViewOfSection" => fn nt_map_view_of_section,
        "NtUnmapViewOfSection" => fn nt_unmap_view_of_section,
        "NtQueryPerformanceCounter" => fn nt_query_performance_counter,
        "NtTerminateProcess" => fn nt_terminate_process,
        "NtDuplicateObject" => fn nt_duplicate_object,
        "NtCreateEvent" => fn nt_create_event,
        "NtSetEvent" => fn nt_set_event,
        "NtResetEvent" => fn nt_reset_event,
        "NtWaitForSingleObject" => fn nt_wait_for_single_object,
        "NtFlushBuffersFile" => fn nt_flush_buffers_file,
        "NtQueryAttributesFile" => fn nt_query_attributes_file,
        "NtRaiseHardError" => fn nt_raise_hard_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;

    #[test]
    fn section_anonyme_mappe_et_libere() {
        unsafe {
            let mut handle: usize = 0;
            let size: u64 = 4096;
            let r = nt_create_section(
                &mut handle,
                0xF001F,
                std::ptr::null(),
                &size as *const u64 as *const c_void,
                0x04, // PAGE_READWRITE
                0x08000000,
                0,
            );
            assert_eq!(r, STATUS_SUCCESS);
            assert_ne!(handle, 0);

            let mut base: *mut c_void = std::ptr::null_mut();
            let mut view_size: usize = 0;
            let r = nt_map_view_of_section(
                handle,
                usize::MAX,
                &mut base,
                0,
                0,
                std::ptr::null_mut(),
                &mut view_size,
                1,
                0,
                0x04,
            );
            assert_eq!(r, STATUS_SUCCESS);
            assert!(!base.is_null());
            assert!(view_size >= 4096);

            // Write and read back
            std::ptr::write(base as *mut u32, 0xDEAD_BEEF);
            assert_eq!(std::ptr::read(base as *const u32), 0xDEAD_BEEF);

            let r = nt_unmap_view_of_section(usize::MAX, base);
            assert_eq!(r, STATUS_SUCCESS);
        }
    }

    #[test]
    fn query_virtual_memory_region_committee() {
        unsafe {
            let page = libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            assert_ne!(page, libc::MAP_FAILED);
            let mut buf = [0u8; 48];
            let mut ret_len: usize = 0;
            let r = nt_query_virtual_memory(
                usize::MAX,
                page,
                0,
                buf.as_mut_ptr() as *mut c_void,
                48,
                &mut ret_len,
            );
            assert_eq!(r, STATUS_SUCCESS);
            assert_eq!(ret_len, 48);
            let state = std::ptr::read_unaligned(buf.as_ptr().add(32) as *const u32);
            assert_eq!(state, 0x1000); // MEM_COMMIT
            let protect = std::ptr::read_unaligned(buf.as_ptr().add(36) as *const u32);
            assert_eq!(protect, 0x04); // PAGE_READWRITE
            libc::munmap(page, 4096);
        }
    }

    #[test]
    fn duplicate_handle_duplique_un_evenement() {
        unsafe {
            let mut handle: usize = 0;
            let r = nt_create_event(&mut handle, 0x1F0003, std::ptr::null(), 1, 0);
            assert_eq!(r, STATUS_SUCCESS);
            assert_ne!(handle, 0);

            let mut dup: usize = 0;
            let r = nt_duplicate_object(usize::MAX, handle, usize::MAX, &mut dup, 0, 0, 0);
            assert_eq!(r, STATUS_SUCCESS);
            assert_ne!(dup, 0);
            assert_ne!(dup, handle);

            // Both handles should work
            let r = nt_set_event(handle, std::ptr::null_mut());
            assert_eq!(r, STATUS_SUCCESS);

            crate::dlls::kernel32::close_handle(handle);
            crate::dlls::kernel32::close_handle(dup);
        }
    }

    #[test]
    fn nt_create_file_ouvre_un_fichier_temporaire() {
        unsafe {
            let dir = std::env::temp_dir().join("winlux_nt_test");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("nt_create.txt");
            std::fs::write(&path, b"hello").unwrap();

            // Build a UNICODE_STRING + OBJECT_ATTRIBUTES for the path
            let win_path = format!("\\??\\{}", path.display());
            let wide: Vec<u16> = win_path.encode_utf16().collect();

            // UNICODE_STRING: Length(u16), MaxLen(u16), pad(4), Buffer(*const u16)
            let mut ustr = [0u8; 16];
            let byte_len = (wide.len() * 2) as u16;
            std::ptr::write_unaligned(ustr.as_mut_ptr() as *mut u16, byte_len);
            std::ptr::write_unaligned(ustr.as_mut_ptr().add(2) as *mut u16, byte_len);
            std::ptr::write_unaligned(ustr.as_mut_ptr().add(8) as *mut *const u16, wide.as_ptr());

            // OBJECT_ATTRIBUTES: Length(u32), pad, RootDir(usize), ObjectName(*USTR), Attrs(u32), ...
            let mut oa = [0u8; 48];
            std::ptr::write_unaligned(oa.as_mut_ptr() as *mut u32, 48);
            std::ptr::write_unaligned(oa.as_mut_ptr().add(0x10) as *mut *const u8, ustr.as_ptr());

            let mut handle: usize = 0;
            let mut io = [0u64; 2];
            let r = nt_create_file(
                &mut handle,
                0x8000_0000, // GENERIC_READ
                oa.as_ptr() as *const c_void,
                io.as_mut_ptr() as *mut c_void,
                std::ptr::null(),
                0x80, // FILE_ATTRIBUTE_NORMAL
                1,    // FILE_SHARE_READ
                1,    // FILE_OPEN
                0,
                std::ptr::null(),
                0,
            );
            assert_eq!(r, STATUS_SUCCESS, "NtCreateFile devrait réussir");
            assert_ne!(handle, 0);

            // Close and cleanup
            crate::dlls::kernel32::close_handle(handle);
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_dir(&dir);
        }
    }

    #[test]
    fn nt_read_write_virtual_memory_copie_les_donnees() {
        unsafe {
            let src: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
            let mut dst = [0u8; 8];
            let mut bytes_done: usize = 0;

            let r = nt_read_virtual_memory(
                usize::MAX,
                src.as_ptr() as *const c_void,
                dst.as_mut_ptr() as *mut c_void,
                8,
                &mut bytes_done,
            );
            assert_eq!(r, STATUS_SUCCESS);
            assert_eq!(bytes_done, 8);
            assert_eq!(dst, src);

            // Write
            let new_data: [u8; 4] = [0xAA, 0xBB, 0xCC, 0xDD];
            let mut target = [0u8; 4];
            let r = nt_write_virtual_memory(
                usize::MAX,
                target.as_mut_ptr() as *mut c_void,
                new_data.as_ptr() as *const c_void,
                4,
                &mut bytes_done,
            );
            assert_eq!(r, STATUS_SUCCESS);
            assert_eq!(bytes_done, 4);
            assert_eq!(target, new_data);
        }
    }
}
