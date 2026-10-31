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

use super::lock;

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
    crate::dlls::kernel32::process_heap()
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
//  SEH stubs (x64 table-based unwinding)
// ============================================================

/// `RtlCaptureContext` : remplit un CONTEXT pour le thread courant.
/// Stub : on met tout à zéro sauf ContextFlags.
pub unsafe extern "win64" fn rtl_capture_context(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    std::ptr::write_bytes(ctx as *mut u8, 0, 1232); // CONTEXT size on x64
                                                    // CONTEXT_AMD64 | CONTEXT_FULL
    std::ptr::write_unaligned(ctx as *mut u32, 0x0010_001F);
}

/// `RtlLookupFunctionEntry` : stub, renvoie NULL (pas de table d'exception).
pub unsafe extern "win64" fn rtl_lookup_function_entry(
    _pc: u64,
    _image_base: *mut u64,
    _history: *mut c_void,
) -> *mut c_void {
    std::ptr::null_mut()
}

/// `RtlVirtualUnwind` : stub SEH.
pub unsafe extern "win64" fn rtl_virtual_unwind(
    _type: u32,
    _base: u64,
    _pc: u64,
    _entry: *const c_void,
    _ctx: *mut c_void,
    _handler_data: *mut *mut c_void,
    establisher_frame: *mut u64,
    _ctx_pointers: *mut c_void,
) -> *mut c_void {
    if !establisher_frame.is_null() {
        *establisher_frame = 0;
    }
    std::ptr::null_mut()
}

/// `RtlUnwindEx` : stub.
pub unsafe extern "win64" fn rtl_unwind_ex(
    _target_frame: *mut c_void,
    _target_ip: *mut c_void,
    _exception_record: *mut c_void,
    _return_value: *mut c_void,
    _ctx: *mut c_void,
    _history: *mut c_void,
) {
    // Les programmes qui arrivent ici sans SEH réel vont probablement planter,
    // mais c'est le mieux qu'on puisse faire sans déroulement complet.
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
    if crate::dlls::advapi32::close_native_key(handle)
        || crate::dlls::kernel32::close_handle(handle) != 0
    {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_HANDLE
    }
}

/// Allocate the current user's registry path, released with RtlFreeUnicodeString.
pub unsafe extern "win64" fn rtl_format_current_user_key_path(out: *mut u8) -> i32 {
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let text: Vec<u16> = crate::dlls::advapi32::native_current_user_key_path()
        .encode_utf16()
        .collect();
    let size = (text.len() + 1) * 2;
    let buffer = crate::dlls::kernel32::heap_alloc(0, 0, size) as *mut u16;
    if buffer.is_null() {
        return 0xC000_0017u32 as i32;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len());
    *buffer.add(text.len()) = 0;
    ustr_set_len(out, (text.len() * 2) as u16);
    ustr_set_max(out, size as u16);
    ustr_set_buf(out, buffer.cast());
    STATUS_SUCCESS
}

/// Open an existing persistent registry key; never create missing settings.
pub unsafe extern "win64" fn nt_open_key(out: *mut usize, access: u32, attrs: *const u8) -> i32 {
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let (root, text) = match registry_object_name(attrs) {
        Ok(value) => value,
        Err(status) => return status,
    };
    registry_status(crate::dlls::advapi32::open_native_key(
        root, &text, access, out,
    ))
}

unsafe fn registry_object_name(attrs: *const u8) -> Result<(usize, String), i32> {
    if attrs.is_null() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    if std::ptr::read_unaligned(attrs.cast::<u32>()) < 48 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let root = std::ptr::read_unaligned(attrs.add(8).cast::<usize>());
    let name = std::ptr::read_unaligned(attrs.add(16).cast::<*const u8>());
    if name.is_null() || !ustr_len(name).is_multiple_of(2) || ustr_len(name) > ustr_max(name) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let length = ustr_len(name) as usize / 2;
    if length != 0 && ustr_buf_w(name).is_null() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let text = if length == 0 {
        String::new()
    } else {
        match String::from_utf16(std::slice::from_raw_parts(ustr_buf_w(name), length)) {
            Ok(text) => text,
            Err(_) => return Err(STATUS_INVALID_PARAMETER),
        }
    };
    Ok((root, text))
}

fn registry_status(error: i32) -> i32 {
    match error {
        0 => STATUS_SUCCESS,
        2 => 0xC000_0034u32 as i32,
        5 => 0xC000_0022u32 as i32,
        6 => STATUS_INVALID_HANDLE,
        _ => STATUS_INVALID_PARAMETER,
    }
}

pub unsafe extern "win64" fn nt_create_key(
    out: *mut usize,
    _access: u32,
    attrs: *const u8,
    _title: u32,
    class: *const u8,
    options: u32,
    disposition: *mut u32,
) -> i32 {
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // Volatile keys, backup/restore semantics and class metadata need separate support.
    if options != 0 || !class.is_null() && ustr_len(class) != 0 {
        return STATUS_NOT_IMPLEMENTED;
    }
    let (root, text) = match registry_object_name(attrs) {
        Ok(value) => value,
        Err(status) => return status,
    };
    registry_status(crate::dlls::advapi32::create_native_key(
        root,
        &text,
        out,
        disposition,
    ))
}

pub unsafe extern "win64" fn nt_open_key_ex(
    out: *mut usize,
    access: u32,
    attrs: *const u8,
    options: u32,
) -> i32 {
    if options != 0 {
        return STATUS_NOT_IMPLEMENTED;
    }
    nt_open_key(out, access, attrs)
}

pub unsafe extern "win64" fn nt_query_value_key(
    handle: usize,
    name: *const u8,
    info_class: u32,
    info: *mut u8,
    length: u32,
    result_length: *mut u32,
) -> i32 {
    if name.is_null()
        || result_length.is_null()
        || !ustr_len(name).is_multiple_of(2)
        || ustr_len(name) > ustr_max(name)
    {
        return STATUS_INVALID_PARAMETER;
    }
    let count = ustr_len(name) as usize / 2;
    if count != 0 && ustr_buf_w(name).is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let wide = if count == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(ustr_buf_w(name), count)
    };
    let text = match String::from_utf16(wide) {
        Ok(text) => text,
        Err(_) => return STATUS_INVALID_PARAMETER,
    };
    if info_class > 2 {
        return 0xC000_0003u32 as i32;
    }
    let (kind, data) = match crate::dlls::advapi32::query_native_value(handle, &text) {
        Ok(value) => value,
        Err(error) => return registry_status(error),
    };
    let header = if info_class == 1 { 20 } else { 12 };
    let name_size = wide.len() * 2;
    let data_offset = (header + name_size + 3) & !3;
    let required = match info_class {
        0 => header + name_size,
        1 => data_offset + data.len(),
        _ => header + data.len(),
    };
    let Ok(required_u32) = u32::try_from(required) else {
        return 0xC000_0017u32 as i32;
    };
    *result_length = required_u32;
    if (length as usize) < header {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let mut result = vec![0u8; required];
    result[4..8].copy_from_slice(&kind.to_le_bytes());
    match info_class {
        0 => {
            result[8..12].copy_from_slice(&(name_size as u32).to_le_bytes());
        }
        1 => {
            result[8..12].copy_from_slice(&(data_offset as u32).to_le_bytes());
            result[12..16].copy_from_slice(&(data.len() as u32).to_le_bytes());
            result[16..20].copy_from_slice(&(name_size as u32).to_le_bytes());
            result[data_offset..].copy_from_slice(&data);
        }
        _ => {
            result[8..12].copy_from_slice(&(data.len() as u32).to_le_bytes());
            result[12..].copy_from_slice(&data);
        }
    }
    if info_class < 2 {
        for (index, unit) in wide.iter().enumerate() {
            result[header + index * 2..header + index * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
    }
    std::ptr::copy_nonoverlapping(result.as_ptr(), info, required.min(length as usize));
    if (length as usize) < required {
        0x8000_0005u32 as i32
    } else {
        STATUS_SUCCESS
    }
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
    let s = s.strip_prefix("\\??\\").unwrap_or(&s);
    let s = s.strip_prefix("\\DosDevices\\").unwrap_or(s);
    let s = s.strip_prefix("\\\\?\\").unwrap_or(s);
    Some(s.to_string())
}

/// Map NT disposition to Rust OpenOptions.
unsafe fn apply_disposition(o: &mut std::fs::OpenOptions, disp: u32) {
    match disp {
        0 => {
            o.create(true).truncate(true);
        }
        1 => {}
        2 => {
            o.create_new(true);
        }
        3 => {
            o.create(true);
        }
        4 => {
            o.truncate(true);
        }
        5 => {
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
        0,
        0,
        1, // FILE_OPEN
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

#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn nt_memory_queries_and_protection_use_real_pages_and_validate_handles() {
        let _teb = crate::process::test_teb();
        unsafe {
            let process = crate::dlls::kernel32::get_current_process();
            let allocation =
                crate::dlls::kernel32::virtual_alloc(std::ptr::null_mut(), 8192, 0x3000, 4);
            assert!(!allocation.is_null());
            crate::process::set_last_error(1234);
            let mut base = (allocation as *mut u8).add(7).cast();
            let mut size = 2;
            let mut old = 0;
            assert_eq!(
                nt_protect_virtual_memory(0, &mut base, &mut size, 2, &mut old),
                STATUS_INVALID_HANDLE
            );
            assert_eq!(size, 2);
            assert_eq!(
                nt_protect_virtual_memory(process, &mut base, &mut size, 2, &mut old),
                0
            );
            assert_eq!(base, allocation);
            assert_eq!(size, 4096);
            assert_eq!(old, 4);
            assert_eq!(crate::dlls::kernel32::get_last_error(), 1234);
            let mut info: crate::dlls::kernel32::MemoryBasicInformation = std::mem::zeroed();
            let mut length = 0;
            assert_eq!(
                nt_query_virtual_memory(
                    process,
                    allocation,
                    0,
                    (&mut info as *mut crate::dlls::kernel32::MemoryBasicInformation).cast(),
                    std::mem::size_of_val(&info),
                    &mut length
                ),
                0
            );
            assert_eq!(info.protect, 2);
            assert_eq!(info.region_size, 4096);
            assert_eq!(length, std::mem::size_of_val(&info));
            assert_eq!(
                nt_query_virtual_memory(
                    process,
                    allocation,
                    99,
                    std::ptr::null_mut(),
                    0,
                    &mut length
                ),
                0xC000_0003u32 as i32
            );
            assert_ne!(
                nt_protect_virtual_memory(process, &mut base, &mut size, 0, &mut old),
                0
            );
            assert_eq!(
                nt_protect_virtual_memory(process, &mut base, &mut size, 4, &mut old),
                0
            );
            *(allocation as *mut u8) = 42;
            assert_eq!(old, 2);
            assert_eq!(
                crate::dlls::kernel32::virtual_free(allocation, 0, 0x8000),
                1
            );
        }
    }
}

/// Query the current process' real Windows memory map (MemoryBasicInformation).
pub unsafe extern "win64" fn nt_query_virtual_memory(
    handle: usize,
    addr: *const c_void,
    info_class: u32,
    info: *mut c_void,
    length: usize,
    return_length: *mut usize,
) -> i32 {
    if handle != crate::dlls::kernel32::get_current_process() {
        return STATUS_INVALID_HANDLE;
    }
    if info_class != 0 {
        return 0xC000_0003u32 as i32; // STATUS_INVALID_INFO_CLASS
    }
    let required = std::mem::size_of::<crate::dlls::kernel32::MemoryBasicInformation>();
    if !return_length.is_null() {
        *return_length = required;
    }
    if length < required {
        return 0xC000_0004u32 as i32;
    }
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let previous = crate::dlls::kernel32::get_last_error();
    let result = crate::dlls::kernel32::virtual_query(addr, info.cast(), length);
    crate::process::set_last_error(previous);
    if result == 0 {
        STATUS_INVALID_PARAMETER
    } else {
        STATUS_SUCCESS
    }
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

/// Apply page protections, returning the aligned range only on success.
pub unsafe extern "win64" fn nt_protect_virtual_memory(
    process: usize,
    base: *mut *mut c_void,
    size: *mut usize,
    new_protect: u32,
    old_protect: *mut u32,
) -> i32 {
    if process != crate::dlls::kernel32::get_current_process() {
        return STATUS_INVALID_HANDLE;
    }
    if base.is_null() || size.is_null() || old_protect.is_null() || *size == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let start = *base as usize & !4095;
    let Some(end) = (*base as usize)
        .checked_add(*size)
        .and_then(|end| end.checked_add(4095))
        .map(|end| end & !4095)
    else {
        return STATUS_INVALID_PARAMETER;
    };
    let previous = crate::dlls::kernel32::get_last_error();
    let result = crate::dlls::kernel32::virtual_protect(*base, *size, new_protect, old_protect);
    let error = crate::dlls::kernel32::get_last_error();
    crate::process::set_last_error(previous);
    if result == 0 {
        return match error {
            487 => 0xC000_0018u32 as i32,
            8 => 0xC000_0017u32 as i32,
            _ => STATUS_INVALID_PARAMETER,
        };
    }
    *base = start as *mut c_void;
    *size = end - start;
    STATUS_SUCCESS
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

/// `NtCreateSection` : stub.
pub unsafe extern "win64" fn nt_create_section(
    _handle: *mut usize,
    _access: u32,
    _attrs: *const c_void,
    _max_size: *const c_void,
    _protect: u32,
    _alloc: u32,
    _file: usize,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// `NtMapViewOfSection` : stub.
pub unsafe extern "win64" fn nt_map_view_of_section(
    _section: usize,
    _process: usize,
    _base: *mut *mut c_void,
    _zero_bits: usize,
    _commit: usize,
    _offset: *mut c_void,
    _size: *mut usize,
    _inherit: u32,
    _alloc: u32,
    _protect: u32,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// `NtUnmapViewOfSection` : stub.
pub unsafe extern "win64" fn nt_unmap_view_of_section(_process: usize, _base: *mut c_void) -> i32 {
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
    if let Some(obj) = super::kernel32::lookup_sync(src_handle) {
        if !tgt_handle.is_null() {
            let new_h = super::kernel32::add_sync(obj, None);
            *tgt_handle = new_h;
        }
        if options & DUPLICATE_CLOSE_SOURCE != 0 {
            super::kernel32::close_handle(src_handle);
        }
        return STATUS_SUCCESS;
    }
    let dup_result = super::kernel32::duplicate_handle_impl(src_handle);
    if let Some(new_h) = dup_result {
        if !tgt_handle.is_null() {
            *tgt_handle = new_h;
        }
        if options & DUPLICATE_CLOSE_SOURCE != 0 {
            super::kernel32::close_handle(src_handle);
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
        let buf = info as *mut u8;
        std::ptr::write_bytes(buf, 0, 40);
        let file_attrs: u32 = if meta.is_dir() { 0x10 } else { 0x80 };
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
        "RtlFormatCurrentUserKeyPath" => fn rtl_format_current_user_key_path,
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
        "NtOpenKey" => fn nt_open_key,
        "NtOpenKeyEx" => fn nt_open_key_ex,
        "NtCreateKey" => fn nt_create_key,
        "NtQueryValueKey" => fn nt_query_value_key,
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
