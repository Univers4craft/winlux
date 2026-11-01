//! oleaut32.dll — OLE Automation (BSTR, VARIANT, SafeArray, stubs IDispatch).
//!
//! paint.net et d'autres apps .NET importent SysAllocString, SysStringLen,
//! VariantInit, SafeArrayCreate, etc.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
#[allow(dead_code)]
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const DISP_E_TYPEMISMATCH: i32 = 0x8002_0012u32 as i32;
const DISP_E_BADINDEX: i32 = 0x8002_000bu32 as i32;

const VT_EMPTY: u16 = 0;
const VT_I2: u16 = 2;
const VT_I4: u16 = 3;
const VT_BSTR: u16 = 8;
const VT_BOOL: u16 = 11;
const VT_UI1: u16 = 17;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── BSTR ────────────────────────────────────────────────────────────

pub unsafe extern "win64" fn sys_alloc_string(source: *const u16) -> *mut u16 {
    if source.is_null() {
        return std::ptr::null_mut();
    }
    let len = crate::printf::wide_len(source);
    sys_alloc_string_len(source, len as u32)
}

pub unsafe extern "win64" fn sys_alloc_string_len(source: *const u16, len: u32) -> *mut u16 {
    let byte_len = (len as usize) * 2;
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

pub unsafe extern "win64" fn variant_init(v: *mut c_void) {
    if v.is_null() {
        return;
    }
    std::ptr::write_bytes(v as *mut u8, 0, 16);
}

pub unsafe extern "win64" fn variant_clear(v: *mut c_void) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    let vt = *(v as *const u16);
    if vt == VT_BSTR {
        let bstr = *((v as *const u8).add(8) as *const *mut u16);
        sys_free_string(bstr);
    }
    std::ptr::write_bytes(v as *mut u8, 0, 16);
    0
}

pub unsafe extern "win64" fn variant_copy(dest: *mut c_void, src: *const c_void) -> i32 {
    if dest.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    variant_clear(dest);
    std::ptr::copy_nonoverlapping(src as *const u8, dest as *mut u8, 16);
    let vt = *(src as *const u16);
    if vt == VT_BSTR {
        let bstr = *((src as *const u8).add(8) as *const *const u16);
        let dup = sys_alloc_string(bstr);
        *((dest as *mut u8).add(8) as *mut *mut u16) = dup;
    }
    0
}

pub unsafe extern "win64" fn variant_change_type(
    dest: *mut c_void,
    src: *const c_void,
    _flags: u16,
    target_vt: u16,
) -> i32 {
    if dest.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    let src_vt = *(src as *const u16);
    if src_vt == target_vt {
        if !std::ptr::eq(dest, src) {
            return variant_copy(dest, src);
        }
        return 0;
    }

    match (src_vt, target_vt) {
        (VT_EMPTY, VT_BSTR) => {
            variant_clear(dest);
            let empty: [u16; 1] = [0];
            let bstr = sys_alloc_string(empty.as_ptr());
            *(dest as *mut u16) = VT_BSTR;
            *((dest as *mut u8).add(8) as *mut *mut u16) = bstr;
            0
        }
        (VT_I4, VT_BSTR) | (VT_I2, VT_BSTR) => {
            let val = if src_vt == VT_I4 {
                *((src as *const u8).add(8) as *const i32)
            } else {
                *((src as *const u8).add(8) as *const i16) as i32
            };
            let text = val.to_string();
            let wide: Vec<u16> = text.encode_utf16().collect();
            variant_clear(dest);
            let bstr = sys_alloc_string_len(wide.as_ptr(), wide.len() as u32);
            *(dest as *mut u16) = VT_BSTR;
            *((dest as *mut u8).add(8) as *mut *mut u16) = bstr;
            0
        }
        (VT_BSTR, VT_I4) => {
            let bstr = *((src as *const u8).add(8) as *const *const u16);
            let s = if bstr.is_null() {
                String::new()
            } else {
                let len = sys_string_len(bstr) as usize;
                String::from_utf16_lossy(std::slice::from_raw_parts(bstr, len))
            };
            let val: i32 = match s.trim().parse() {
                Ok(v) => v,
                Err(_) => return DISP_E_TYPEMISMATCH,
            };
            variant_clear(dest);
            *(dest as *mut u16) = VT_I4;
            *((dest as *mut u8).add(8) as *mut i32) = val;
            0
        }
        (VT_BOOL, VT_I4) => {
            let val = *((src as *const u8).add(8) as *const i16);
            variant_clear(dest);
            *(dest as *mut u16) = VT_I4;
            *((dest as *mut u8).add(8) as *mut i32) = if val != 0 { 1 } else { 0 };
            0
        }
        (VT_I4, VT_BOOL) | (VT_I2, VT_BOOL) => {
            let val = if src_vt == VT_I4 {
                *((src as *const u8).add(8) as *const i32)
            } else {
                *((src as *const u8).add(8) as *const i16) as i32
            };
            variant_clear(dest);
            *(dest as *mut u16) = VT_BOOL;
            // VARIANT_TRUE = -1 (0xFFFF), VARIANT_FALSE = 0
            *((dest as *mut u8).add(8) as *mut i16) = if val != 0 { -1 } else { 0 };
            0
        }
        (VT_UI1, VT_I4) => {
            let val = *((src as *const u8).add(8));
            variant_clear(dest);
            *(dest as *mut u16) = VT_I4;
            *((dest as *mut u8).add(8) as *mut i32) = val as i32;
            0
        }
        _ => DISP_E_TYPEMISMATCH,
    }
}

// ── SafeArray ──────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
struct SafeArrayBound {
    elements: u32,
    lower_bound: i32,
}

struct SafeArrayInner {
    dims: u16,
    vt: u16,
    element_size: u32,
    locks: u32,
    data: Vec<u8>,
    bounds: Vec<SafeArrayBound>,
}

static SAFE_ARRAYS: OnceLock<Mutex<HashMap<usize, Box<SafeArrayInner>>>> = OnceLock::new();

fn safe_arrays() -> &'static Mutex<HashMap<usize, Box<SafeArrayInner>>> {
    SAFE_ARRAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn element_size_for_vt(vt: u16) -> u32 {
    match vt {
        VT_UI1 => 1,           // VT_UI1
        VT_I2 | VT_BOOL => 2,  // VT_I2, VT_BOOL
        VT_I4 | 4 => 4,        // VT_I4, VT_R4
        5 | 7 => 8,            // VT_R8, VT_DATE
        VT_BSTR | 9 | 13 => 8, // VT_BSTR, VT_DISPATCH, VT_UNKNOWN (pointers)
        12 => 16,              // VT_VARIANT
        _ => 16,
    }
}

/// Taille maximale des données d'un SAFEARRAY (1 Gio).
const MAX_SAFEARRAY_BYTES: usize = 1 << 30;

pub unsafe extern "win64" fn safe_array_create(vt: u16, dims: u32, bounds_ptr: usize) -> usize {
    // `cDims` est un USHORT côté Windows ; le produit des dimensions est
    // calculé sans débordement et plafonné à 1 Gio (échec = NULL, comme
    // Windows en cas de mémoire insuffisante).
    if dims == 0 || dims > 0xFFFF || bounds_ptr == 0 {
        return 0;
    }
    let bounds_slice =
        std::slice::from_raw_parts(bounds_ptr as *const SafeArrayBound, dims as usize);
    let bounds: Vec<SafeArrayBound> = bounds_slice.to_vec();
    let elem_size = element_size_for_vt(vt);
    let total_elements = bounds
        .iter()
        .try_fold(1usize, |acc, b| acc.checked_mul(b.elements as usize));
    let Some(data_size) = total_elements
        .and_then(|n| n.checked_mul(elem_size as usize))
        .filter(|&n| n <= MAX_SAFEARRAY_BYTES)
    else {
        return 0;
    };
    let data = vec![0u8; data_size];

    let inner = Box::new(SafeArrayInner {
        dims: dims as u16,
        vt,
        element_size: elem_size,
        locks: 0,
        data,
        bounds,
    });
    let key = &*inner as *const SafeArrayInner as usize;
    lock(safe_arrays()).insert(key, inner);
    key
}

pub unsafe extern "win64" fn safe_array_create_vector(vt: u16, lower: i32, count: u32) -> usize {
    let bound = SafeArrayBound {
        elements: count,
        lower_bound: lower,
    };
    let elem_size = element_size_for_vt(vt);
    let Some(data_size) = (count as usize)
        .checked_mul(elem_size as usize)
        .filter(|&n| n <= MAX_SAFEARRAY_BYTES)
    else {
        return 0;
    };
    let data = vec![0u8; data_size];

    let inner = Box::new(SafeArrayInner {
        dims: 1,
        vt,
        element_size: elem_size,
        locks: 0,
        data,
        bounds: vec![bound],
    });
    let key = &*inner as *const SafeArrayInner as usize;
    lock(safe_arrays()).insert(key, inner);
    key
}

pub extern "win64" fn safe_array_destroy(sa: usize) -> i32 {
    if sa == 0 {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    if let Some(inner) = map.get(&sa) {
        if inner.locks > 0 {
            return E_INVALIDARG;
        }
    }
    if map.remove(&sa).is_some() {
        0
    } else {
        E_INVALIDARG
    }
}

pub unsafe extern "win64" fn safe_array_access_data(sa: usize, data: *mut *mut c_void) -> i32 {
    if sa == 0 || data.is_null() {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    inner.locks = inner.locks.saturating_add(1);
    *data = inner.data.as_mut_ptr() as *mut c_void;
    0
}

pub extern "win64" fn safe_array_unaccess_data(sa: usize) -> i32 {
    if sa == 0 {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    inner.locks = inner.locks.saturating_sub(1);
    0
}

pub extern "win64" fn safe_array_get_dim(sa: usize) -> u32 {
    if sa == 0 {
        return 0;
    }
    let map = lock(safe_arrays());
    map.get(&sa).map(|i| i.dims as u32).unwrap_or(0)
}

pub unsafe extern "win64" fn safe_array_get_l_bound(sa: usize, dim: u32, bound: *mut i32) -> i32 {
    if sa == 0 || bound.is_null() || dim == 0 {
        return E_INVALIDARG;
    }
    let map = lock(safe_arrays());
    let Some(inner) = map.get(&sa) else {
        return E_INVALIDARG;
    };
    let idx = (dim - 1) as usize;
    if idx >= inner.bounds.len() {
        return E_INVALIDARG;
    }
    *bound = inner.bounds[idx].lower_bound;
    0
}

pub unsafe extern "win64" fn safe_array_get_u_bound(sa: usize, dim: u32, bound: *mut i32) -> i32 {
    if sa == 0 || bound.is_null() || dim == 0 {
        return E_INVALIDARG;
    }
    let map = lock(safe_arrays());
    let Some(inner) = map.get(&sa) else {
        return E_INVALIDARG;
    };
    let idx = (dim - 1) as usize;
    if idx >= inner.bounds.len() {
        return E_INVALIDARG;
    }
    let b = &inner.bounds[idx];
    *bound = b.lower_bound + b.elements as i32 - 1;
    0
}

pub unsafe extern "win64" fn safe_array_get_element(
    sa: usize,
    indices: *const i32,
    out: *mut c_void,
) -> i32 {
    if sa == 0 || indices.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let map = lock(safe_arrays());
    let Some(inner) = map.get(&sa) else {
        return E_INVALIDARG;
    };
    let offset = match compute_offset(inner, indices) {
        Some(o) => o,
        None => return DISP_E_BADINDEX,
    };
    let elem_size = inner.element_size as usize;
    if offset + elem_size > inner.data.len() {
        return DISP_E_BADINDEX;
    }
    std::ptr::copy_nonoverlapping(inner.data.as_ptr().add(offset), out as *mut u8, elem_size);
    0
}

pub unsafe extern "win64" fn safe_array_put_element(
    sa: usize,
    indices: *const i32,
    val: *const c_void,
) -> i32 {
    if sa == 0 || indices.is_null() || val.is_null() {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    let offset = match compute_offset(inner, indices) {
        Some(o) => o,
        None => return DISP_E_BADINDEX,
    };
    let elem_size = inner.element_size as usize;
    if offset + elem_size > inner.data.len() {
        return DISP_E_BADINDEX;
    }
    std::ptr::copy_nonoverlapping(
        val as *const u8,
        inner.data.as_mut_ptr().add(offset),
        elem_size,
    );
    0
}

unsafe fn compute_offset(inner: &SafeArrayInner, indices: *const i32) -> Option<usize> {
    let mut offset = 0usize;
    let mut multiplier = 1usize;
    for dim_idx in (0..inner.dims as usize).rev() {
        let b = &inner.bounds[dim_idx];
        let idx = *indices.add(dim_idx) - b.lower_bound;
        if idx < 0 || idx >= b.elements as i32 {
            return None;
        }
        offset += idx as usize * multiplier;
        multiplier *= b.elements as usize;
    }
    Some(offset * inner.element_size as usize)
}

pub unsafe extern "win64" fn safe_array_get_elem_size(sa: usize) -> u32 {
    if sa == 0 {
        return 0;
    }
    let map = lock(safe_arrays());
    map.get(&sa).map(|i| i.element_size).unwrap_or(0)
}

pub unsafe extern "win64" fn safe_array_get_vartype(sa: usize, pvt: *mut u16) -> i32 {
    if sa == 0 || pvt.is_null() {
        return E_INVALIDARG;
    }
    let map = lock(safe_arrays());
    let Some(inner) = map.get(&sa) else {
        return E_INVALIDARG;
    };
    *pvt = inner.vt;
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
        "SafeArrayGetElement" => fn safe_array_get_element,
        "SafeArrayPutElement" => fn safe_array_put_element,
        "SafeArrayGetElemsize" => fn safe_array_get_elem_size,
        "SafeArrayGetVartype" => fn safe_array_get_vartype,
        "GetErrorInfo" => fn get_error_info,
        "SetErrorInfo" => fn set_error_info,
        "CreateErrorInfo" => fn create_error_info,
        // Ordinaux publics d'oleaut32 : l'apphost de Paint.NET et beaucoup
        // de programmes Visual C++ importent BSTR et VARIANT par numéro.
        "#2" => fn sys_alloc_string, "#3" => fn sys_realloc_string,
        "#4" => fn sys_alloc_string_len, "#5" => fn sys_realloc_string_len,
        "#6" => fn sys_free_string, "#7" => fn sys_string_len,
        "#8" => fn variant_init, "#9" => fn variant_clear, "#10" => fn variant_copy,
        "#12" => fn variant_change_type,
        "#15" => fn safe_array_create, "#16" => fn safe_array_destroy,
        "#17" => fn safe_array_get_dim, "#18" => fn safe_array_get_elem_size,
        "#19" => fn safe_array_get_u_bound, "#20" => fn safe_array_get_l_bound,
        "#23" => fn safe_array_access_data, "#24" => fn safe_array_unaccess_data,
        "#25" => fn safe_array_get_element, "#26" => fn safe_array_put_element,
        "#77" => fn safe_array_get_vartype,
        "#149" => fn sys_string_byte_len, "#150" => fn sys_alloc_string_byte_len,
        "#200" => fn get_error_info, "#201" => fn set_error_info, "#202" => fn create_error_info,
        "#411" => fn safe_array_create_vector,
    }
}
