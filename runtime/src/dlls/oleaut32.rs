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
const DISP_E_BADINDEX: i32 = 0x8002_000bu32 as i32;

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

/// BSTR d'un VARIANT (union à +8). Un programme x86 n'y range que 4 octets
/// ; les 4 suivants sont quelconques (#232, 7-Zip 32 bits).
unsafe fn variant_bstr(v: *const c_void) -> *mut u16 {
    crate::wow32::read_ptr(v as usize + 8) as *mut u16
}

unsafe fn set_variant_bstr(v: *mut c_void, bstr: *mut u16) {
    crate::wow32::write_ptr(v as usize + 8, bstr as usize)
}

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
        sys_free_string(variant_bstr(v));
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
        let dup = sys_alloc_string(variant_bstr(src));
        set_variant_bstr(dest, dup);
    }
    0
}

/// VariantChangeType : toutes les conversions de base (oleaut32_var.rs),
/// avec la langue de l'utilisateur.
pub unsafe extern "win64" fn variant_change_type(
    dest: *mut c_void,
    src: *const c_void,
    flags: u16,
    target_vt: u16,
) -> i32 {
    super::oleaut32_var::change_type(dest, src, 0x400, flags, target_vt)
}

// ── SafeArray ──────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct SafeArrayBound {
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

/// SafeArrayPtrOfIndex : adresse d'un élément (le tableau doit être
/// verrouillé par l'appelant, comme le veut la documentation).
pub unsafe extern "win64" fn safe_array_ptr_of_index(
    sa: usize,
    indices: *const i32,
    out: *mut usize,
) -> i32 {
    if sa == 0 || indices.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    let Some(offset) = compute_offset(inner, indices) else {
        return DISP_E_BADINDEX;
    };
    if offset + inner.element_size as usize > inner.data.len() {
        return DISP_E_BADINDEX;
    }
    *out = inner.data.as_mut_ptr().add(offset) as usize;
    0
}

/// SafeArrayRedim : nouvelle borne de la dimension la moins significative
/// (la dernière de rgsabound) ; les éléments gardés restent en place, les
/// nouveaux sont à zéro. Refusé tant que le tableau est verrouillé.
pub unsafe extern "win64" fn safe_array_redim(sa: usize, bound: *const SafeArrayBound) -> i32 {
    const DISP_E_ARRAYISLOCKED: i32 = 0x8002_000Du32 as i32;
    if sa == 0 || bound.is_null() {
        return E_INVALIDARG;
    }
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    if inner.locks != 0 {
        return DISP_E_ARRAYISLOCKED;
    }
    let Some(last) = inner.bounds.last_mut() else {
        return E_INVALIDARG;
    };
    *last = std::ptr::read_unaligned(bound);
    let total = inner
        .bounds
        .iter()
        .try_fold(inner.element_size as usize, |n, b| {
            n.checked_mul(b.elements as usize)
        });
    match total {
        Some(n) if n <= 1 << 30 => {
            inner.data.resize(n, 0);
            0
        }
        _ => E_OUTOFMEMORY,
    }
}

/// VariantCopyInd : comme VariantCopy, mais une référence (VT_BYREF) est
/// suivie et la valeur pointée est copiée.
pub unsafe extern "win64" fn variant_copy_ind(dest: *mut c_void, src: *const c_void) -> i32 {
    const VT_BYREF: u16 = 0x4000;
    if dest.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    let vt = *(src as *const u16);
    if vt & VT_BYREF == 0 {
        return variant_copy(dest, src);
    }
    let base = vt & !VT_BYREF;
    let p = crate::wow32::read_ptr(src as usize + 8);
    if p == 0 {
        return E_INVALIDARG;
    }
    if base == 12 {
        // VT_VARIANT | VT_BYREF : le VARIANT pointé.
        return variant_copy(dest, p as *const c_void);
    }
    let mut tmp = [0u8; 24];
    *(tmp.as_mut_ptr() as *mut u16) = base;
    let size = match base {
        VT_UI1 => 1,
        VT_I2 | VT_BOOL => 2,
        VT_BSTR | 9 | 13 => crate::wow32::pointer_size(),
        5 | 6 | 7 | 20 | 21 => 8, // R8, CY, DATE, I8, UI8
        _ => 4,
    };
    std::ptr::copy_nonoverlapping(p as *const u8, tmp.as_mut_ptr().add(8), size);
    variant_copy(dest, tmp.as_ptr() as *const c_void)
}

/// GetActiveObject : aucun objet n'est inscrit dans la table des objets
/// actifs (pas de RegisterActiveObject) : MK_E_UNAVAILABLE.
pub unsafe extern "win64" fn get_active_object(
    _clsid: *const u8,
    _reserved: usize,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    0x8004_01E3u32 as i32
}

// GetErrorInfo, SetErrorInfo, CreateErrorInfo : voir errorinfo.rs.
use super::errorinfo::{create_error_info, get_error_info, set_error_info};

// ── Bibliothèques de types ──────────────────────────────────────────
//
// Aucune bibliothèque de types n'est enregistrée : les appelants (Chromium,
// accessibilité, automatisation) reçoivent l'erreur documentée et continuent
// sans elle.

const TYPE_E_LIBNOTREGISTERED: i32 = 0x8002_801Du32 as i32;
const TYPE_E_CANTLOADLIBRARY: i32 = 0x8002_9C4Au32 as i32;

pub unsafe extern "win64" fn load_type_lib(_file: *const u16, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    TYPE_E_CANTLOADLIBRARY
}
pub unsafe extern "win64" fn load_type_lib_ex(
    _file: *const u16,
    _kind: u32,
    out: *mut usize,
) -> i32 {
    load_type_lib(_file, out)
}
pub unsafe extern "win64" fn load_reg_type_lib(
    _guid: *const u8,
    _major: u16,
    _minor: u16,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    TYPE_E_LIBNOTREGISTERED
}
pub unsafe extern "win64" fn query_path_of_reg_type_lib(
    _guid: *const u8,
    _major: u16,
    _minor: u16,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    TYPE_E_LIBNOTREGISTERED
}
pub extern "win64" fn register_type_lib(_lib: usize, _path: usize, _help: usize) -> i32 {
    TYPE_E_CANTLOADLIBRARY
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "LoadTypeLib" => fn load_type_lib,
        "LoadTypeLibEx" => fn load_type_lib_ex,
        "LoadRegTypeLib" => fn load_reg_type_lib,
        "QueryPathOfRegTypeLib" => fn query_path_of_reg_type_lib,
        "RegisterTypeLib" => fn register_type_lib,
        "#161" => fn load_type_lib, "#162" => fn load_reg_type_lib,
        "#163" => fn register_type_lib, "#164" => fn query_path_of_reg_type_lib,
        "#183" => fn load_type_lib_ex,
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
        "SafeArrayPtrOfIndex" => fn safe_array_ptr_of_index,
        "SafeArrayRedim" => fn safe_array_redim,
        "VariantCopyInd" => fn variant_copy_ind,
        "GetActiveObject" => fn get_active_object,
        "GetErrorInfo" => fn get_error_info,
        "SetErrorInfo" => fn set_error_info,
        "CreateErrorInfo" => fn create_error_info,
        // Ordinaux publics d'oleaut32 : l'apphost de Paint.NET et beaucoup
        // de programmes Visual C++ importent BSTR et VARIANT par numéro.
        "#2" => fn sys_alloc_string, "#3" => fn sys_realloc_string,
        "#4" => fn sys_alloc_string_len, "#5" => fn sys_realloc_string_len,
        "#6" => fn sys_free_string, "#7" => fn sys_string_len,
        "#8" => fn variant_init, "#9" => fn variant_clear, "#10" => fn variant_copy,
        "#11" => fn variant_copy_ind, "#12" => fn variant_change_type,
        "#35" => fn get_active_object, "#40" => fn safe_array_redim,
        "#148" => fn safe_array_ptr_of_index,
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
