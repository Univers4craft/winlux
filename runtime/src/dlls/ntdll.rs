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

/// `RtlLookupFunctionEntry` : cherche l'entrée RUNTIME_FUNCTION dans le
/// répertoire d'exceptions (.pdata) du module contenant `pc`.
pub unsafe extern "win64" fn rtl_lookup_function_entry(
    pc: u64,
    image_base: *mut u64,
    _history: *mut c_void,
) -> *mut c_void {
    let Some(m) = crate::loader::find_module_by_addr(pc as usize) else {
        return std::ptr::null_mut();
    };
    let base = m.base;
    if !image_base.is_null() {
        *image_base = base as u64;
    }
    // Data directory index 3 = IMAGE_DIRECTORY_ENTRY_EXCEPTION
    let Some(dir) = m.pe.dirs.get(3) else {
        return std::ptr::null_mut();
    };
    if dir.rva == 0 || dir.size == 0 {
        return std::ptr::null_mut();
    }
    let rva = (pc as usize).wrapping_sub(base) as u32;
    let table = (base + dir.rva as usize) as *const [u32; 3];
    let count = dir.size as usize / 12;
    // Binary search: the table is sorted by BeginAddress.
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
    crate::dlls::kernel32::close_handle(handle);
    STATUS_SUCCESS
}

/// `NtCreateFile` : stub, non implémenté.
pub unsafe extern "win64" fn nt_create_file(
    _handle: *mut usize,
    _access: u32,
    _attrs: *const c_void,
    _io: *mut c_void,
    _alloc: *const c_void,
    _file_attrs: u32,
    _share: u32,
    _disposition: u32,
    _options: u32,
    _ea: *const c_void,
    _ea_len: u32,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// `NtOpenFile` : stub, non implémenté.
pub unsafe extern "win64" fn nt_open_file(
    _handle: *mut usize,
    _access: u32,
    _attrs: *const c_void,
    _io: *mut c_void,
    _share: u32,
    _options: u32,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
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

/// `NtQueryVirtualMemory` : stub.
pub unsafe extern "win64" fn nt_query_virtual_memory(
    _handle: usize,
    _addr: *const c_void,
    _info_class: u32,
    _info: *mut c_void,
    _length: usize,
    _return_length: *mut usize,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
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

/// `NtReadVirtualMemory` : stub.
pub unsafe extern "win64" fn nt_read_virtual_memory(
    _process: usize,
    _base: *const c_void,
    _buffer: *mut c_void,
    _size: usize,
    _bytes_read: *mut usize,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// `NtWriteVirtualMemory` : stub.
pub unsafe extern "win64" fn nt_write_virtual_memory(
    _process: usize,
    _base: *mut c_void,
    _buffer: *const c_void,
    _size: usize,
    _bytes_written: *mut usize,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
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

/// `NtDuplicateObject` : stub.
pub unsafe extern "win64" fn nt_duplicate_object(
    _src_process: usize,
    _src_handle: usize,
    _tgt_process: usize,
    _tgt_handle: *mut usize,
    _access: u32,
    _attrs: u32,
    _options: u32,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
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

/// `NtQueryAttributesFile` : stub.
pub unsafe extern "win64" fn nt_query_attributes_file(
    _attrs: *const c_void,
    _info: *mut c_void,
) -> i32 {
    STATUS_NOT_IMPLEMENTED
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
