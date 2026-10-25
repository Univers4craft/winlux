//! oleaut32.dll — OLE Automation (BSTR, VARIANT, SafeArray, stubs IDispatch).
//!
//! paint.net et d'autres apps .NET importent SysAllocString, SysStringLen,
//! VariantInit, SafeArrayCreate, etc.

use std::ffi::c_void;

// ── BSTR ────────────────────────────────────────────────────────────

/// Alloue un BSTR (chaîne OLE préfixée par sa longueur en octets).
pub unsafe extern "win64" fn sys_alloc_string(source: *const u16) -> *mut u16 {
    if source.is_null() {
        return std::ptr::null_mut();
    }
    let len = crate::printf::wide_len(source);
    sys_alloc_string_len(source, len as u32)
}

pub unsafe extern "win64" fn sys_alloc_string_len(source: *const u16, len: u32) -> *mut u16 {
    let byte_len = (len as usize) * 2;
    // Layout : 4 octets de longueur, puis la chaîne, puis un u16 nul.
    let total = 4 + byte_len + 2;
    let ptr = libc::malloc(total) as *mut u8;
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    *(ptr as *mut u32) = byte_len as u32;
    let bstr = ptr.add(4) as *mut u16;
    if !source.is_null() && len > 0 {
        std::ptr::copy_nonoverlapping(source, bstr, len as usize);
    } else {
        std::ptr::write_bytes(bstr, 0, len as usize);
    }
    *bstr.add(len as usize) = 0;
    bstr
}

pub unsafe extern "win64" fn sys_alloc_string_byte_len(source: *const u8, len: u32) -> *mut u16 {
    let total = 4 + len as usize + 2;
    let ptr = libc::malloc(total) as *mut u8;
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    *(ptr as *mut u32) = len;
    let bstr = ptr.add(4);
    if !source.is_null() && len > 0 {
        std::ptr::copy_nonoverlapping(source, bstr, len as usize);
    }
    *bstr.add(len as usize) = 0;
    *(bstr.add(len as usize + 1)) = 0;
    (ptr.add(4)) as *mut u16
}

pub unsafe extern "win64" fn sys_free_string(bstr: *mut u16) {
    if bstr.is_null() {
        return;
    }
    let ptr = (bstr as *mut u8).sub(4);
    libc::free(ptr as *mut c_void);
}

pub unsafe extern "win64" fn sys_string_len(bstr: *const u16) -> u32 {
    if bstr.is_null() {
        return 0;
    }
    let ptr = (bstr as *const u8).sub(4);
    (*(ptr as *const u32)) / 2
}

pub unsafe extern "win64" fn sys_string_byte_len(bstr: *const u16) -> u32 {
    if bstr.is_null() {
        return 0;
    }
    let ptr = (bstr as *const u8).sub(4);
    *(ptr as *const u32)
}

pub unsafe extern "win64" fn sys_realloc_string(pbstr: *mut *mut u16, source: *const u16) -> i32 {
    if pbstr.is_null() {
        return 0;
    }
    let old = *pbstr;
    let new = sys_alloc_string(source);
    if source.is_null() && new.is_null() {
        sys_free_string(old);
        *pbstr = std::ptr::null_mut();
        return 1;
    }
    if new.is_null() {
        return 0;
    }
    sys_free_string(old);
    *pbstr = new;
    1
}

pub unsafe extern "win64" fn sys_realloc_string_len(
    pbstr: *mut *mut u16,
    source: *const u16,
    len: u32,
) -> i32 {
    if pbstr.is_null() {
        return 0;
    }
    let new = sys_alloc_string_len(source, len);
    if new.is_null() && len > 0 {
        return 0;
    }
    sys_free_string(*pbstr);
    *pbstr = new;
    1
}

// ── VARIANT ─────────────────────────────────────────────────────────

/// VariantInit : met le type à VT_EMPTY (0).
pub unsafe extern "win64" fn variant_init(v: *mut c_void) {
    if v.is_null() {
        return;
    }
    std::ptr::write_bytes(v as *mut u8, 0, 16);
}

pub unsafe extern "win64" fn variant_clear(v: *mut c_void) -> i32 {
    if v.is_null() {
        return 0x8007_0057u32 as i32;
    }
    let vt = *(v as *const u16);
    if vt == 8 {
        // VT_BSTR
        let bstr = *((v as *const u8).add(8) as *const *mut u16);
        sys_free_string(bstr);
    }
    std::ptr::write_bytes(v as *mut u8, 0, 16);
    0
}

pub unsafe extern "win64" fn variant_copy(dest: *mut c_void, src: *const c_void) -> i32 {
    if dest.is_null() || src.is_null() {
        return 0x8007_0057u32 as i32;
    }
    variant_clear(dest);
    std::ptr::copy_nonoverlapping(src as *const u8, dest as *mut u8, 16);
    let vt = *(src as *const u16);
    if vt == 8 {
        // VT_BSTR : dupliquer
        let bstr = *((src as *const u8).add(8) as *const *const u16);
        let dup = sys_alloc_string(bstr);
        *((dest as *mut u8).add(8) as *mut *mut u16) = dup;
    }
    0
}

pub unsafe extern "win64" fn variant_change_type(
    _dest: *mut c_void,
    _src: *const c_void,
    _flags: u16,
    _vt: u16,
) -> i32 {
    0x8002_0012u32 as i32 // DISP_E_TYPEMISMATCH (stub)
}

// ── SafeArray (stubs) ───────────────────────────────────────────────

pub extern "win64" fn safe_array_create(_vt: u16, _dims: u32, _bounds: usize) -> usize {
    0
}
pub extern "win64" fn safe_array_create_vector(_vt: u16, _lower: i32, _count: u32) -> usize {
    0
}
pub extern "win64" fn safe_array_destroy(_sa: usize) -> i32 {
    0
}
pub unsafe extern "win64" fn safe_array_access_data(_sa: usize, _data: *mut *mut c_void) -> i32 {
    0x8007_0057u32 as i32
}
pub extern "win64" fn safe_array_unaccess_data(_sa: usize) -> i32 {
    0
}
pub extern "win64" fn safe_array_get_dim(_sa: usize) -> u32 {
    0
}
pub unsafe extern "win64" fn safe_array_get_l_bound(
    _sa: usize,
    _dim: u32,
    _bound: *mut i32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn safe_array_get_u_bound(
    _sa: usize,
    _dim: u32,
    _bound: *mut i32,
) -> i32 {
    0
}

// ── Divers ──────────────────────────────────────────────────────────

pub extern "win64" fn get_error_info(_reserved: u32, _info: *mut usize) -> i32 {
    1
} // S_FALSE
pub extern "win64" fn set_error_info(_reserved: u32, _info: usize) -> i32 {
    0
}
pub extern "win64" fn create_error_info(_info: *mut usize) -> i32 {
    0x8000_4005u32 as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SysAllocString" => fn sys_alloc_string,
        "SysAllocStringLen" => fn sys_alloc_string_len,
        "SysAllocStringByteLen" => fn sys_alloc_string_byte_len,
        "SysFreeString" => fn sys_free_string,
        "SysStringLen" => fn sys_string_len,
        "SysStringByteLen" => fn sys_string_byte_len,
        "SysReAllocString" => fn sys_realloc_string,
        "SysReAllocStringLen" => fn sys_realloc_string_len,
        "VariantInit" => fn variant_init,
        "VariantClear" => fn variant_clear,
        "VariantCopy" => fn variant_copy,
        "VariantChangeType" => fn variant_change_type,
        "SafeArrayCreate" => fn safe_array_create,
        "SafeArrayCreateVector" => fn safe_array_create_vector,
        "SafeArrayDestroy" => fn safe_array_destroy,
        "SafeArrayAccessData" => fn safe_array_access_data,
        "SafeArrayUnaccessData" => fn safe_array_unaccess_data,
        "SafeArrayGetDim" => fn safe_array_get_dim,
        "SafeArrayGetLBound" => fn safe_array_get_l_bound,
        "SafeArrayGetUBound" => fn safe_array_get_u_bound,
        "GetErrorInfo" => fn get_error_info,
        "SetErrorInfo" => fn set_error_info,
        "CreateErrorInfo" => fn create_error_info,
    }
}
