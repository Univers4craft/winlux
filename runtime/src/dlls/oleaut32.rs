//! oleaut32.dll — OLE Automation (BSTR, VARIANT, SafeArray, stubs IDispatch).
//!
//! paint.net et d'autres apps .NET importent SysAllocString, SysStringLen,
//! VariantInit, SafeArrayCreate, etc.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
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
    // VT_RECORD en x64 : pRecInfo occupe les octets 16 à 23 (#1902).
    if vt == VT_RECORD && variant_size() == 24 {
        std::ptr::copy_nonoverlapping((src as *const u8).add(16), (dest as *mut u8).add(16), 8);
    }
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
    /// IRecordInfo d'un tableau VT_RECORD (référence prise), sinon 0.
    record: usize,
    /// Structure SAFEARRAY publique (oaidl.h) que voit l'appelant, précédée
    /// de 16 octets dont les 4 derniers portent le VARTYPE
    /// (FADF_HAVEVARTYPE). Son adresse + 16 est le pointeur rendu (#1204).
    header: Vec<u64>,
}

// Le tampon d'en-tête et les données appartiennent au tableau, protégé
// par le verrou de la table.
unsafe impl Send for SafeArrayInner {}

const FADF_RECORD: u16 = 0x0020;
const FADF_HAVEVARTYPE: u16 = 0x0080;
const VT_RECORD: u16 = 36;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;
const FADF_BSTR: u16 = 0x0100;
const FADF_UNKNOWN: u16 = 0x0200;
const FADF_DISPATCH: u16 = 0x0400;
const FADF_VARIANT: u16 = 0x0800;

impl SafeArrayInner {
    fn new(vt: u16, bounds: Vec<SafeArrayBound>, data: Vec<u8>, element_size: u32) -> Box<Self> {
        let dims = bounds.len();
        // 16 octets de préfixe, puis 24 + 8 par dimension (x64).
        let words = (16 + 24 + 8 * dims).div_ceil(8);
        let mut inner = Box::new(SafeArrayInner {
            dims: dims as u16,
            vt,
            element_size,
            locks: 0,
            data,
            bounds,
            record: 0,
            header: vec![0u64; words],
        });
        inner.sync_header();
        inner
    }

    /// Adresse du SAFEARRAY public, clé de la table.
    fn key(&self) -> usize {
        self.header.as_ptr() as usize + 16
    }

    /// Recopie l'état dans la structure publique : cDims, fFeatures,
    /// cbElements, cLocks, pvData puis rgsabound, la dimension la plus à
    /// droite en premier comme sous Windows. Disposition x86 (pointeur de
    /// 4 octets, bornes à l'octet 16) pour un programme 32 bits.
    fn sync_header(&mut self) {
        let features = if self.vt == VT_RECORD {
            FADF_RECORD
        } else {
            FADF_HAVEVARTYPE
        } | match self.vt {
            VT_BSTR => FADF_BSTR,
            13 => FADF_UNKNOWN,
            9 => FADF_DISPATCH,
            12 => FADF_VARIANT,
            _ => 0,
        };
        let data = self.data.as_mut_ptr() as usize;
        let ptr32 = crate::wow32::pointer_size() == 4;
        let bounds: Vec<SafeArrayBound> = self.bounds.iter().rev().copied().collect();
        let (dims, size, locks, vt) = (self.dims, self.element_size, self.locks, self.vt);
        let record = self.record;
        let base = self.header.as_mut_ptr() as *mut u8;
        unsafe {
            // FADF_RECORD : l'IRecordInfo juste avant la structure, à la
            // place du VARTYPE.
            if vt == VT_RECORD {
                if ptr32 {
                    std::ptr::write_unaligned(base.add(12) as *mut u32, record as u32);
                } else {
                    std::ptr::write_unaligned(base.add(8) as *mut usize, record);
                }
            } else {
                std::ptr::write_unaligned(base.add(12) as *mut u32, u32::from(vt));
            }
            let sa = base.add(16);
            std::ptr::write_unaligned(sa as *mut u16, dims);
            std::ptr::write_unaligned(sa.add(2) as *mut u16, features);
            std::ptr::write_unaligned(sa.add(4) as *mut u32, size);
            std::ptr::write_unaligned(sa.add(8) as *mut u32, locks);
            let first = if ptr32 {
                std::ptr::write_unaligned(sa.add(12) as *mut u32, data as u32);
                16
            } else {
                std::ptr::write_unaligned(sa.add(16) as *mut usize, data);
                24
            };
            for (i, b) in bounds.iter().enumerate() {
                std::ptr::write_unaligned(sa.add(first + 8 * i) as *mut SafeArrayBound, *b);
            }
        }
    }
}

/// IUnknown::AddRef (1) ou Release (2) d'un élément VT_UNKNOWN/VT_DISPATCH.
/// Les objets d'un programme 32 bits suivent une autre convention d'appel :
/// leur compteur n'est alors pas touché.
unsafe fn unk_call(slot: usize, p: usize) {
    if p == 0 || crate::wow32::pointer_size() != 8 {
        return;
    }
    let vtbl = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtbl.add(slot));
    f(p);
}

fn register(inner: Box<SafeArrayInner>) -> usize {
    let key = inner.key();
    lock(safe_arrays()).insert(key, inner);
    key
}

static SAFE_ARRAYS: OnceLock<Mutex<HashMap<usize, Box<SafeArrayInner>>>> = OnceLock::new();

fn safe_arrays() -> &'static Mutex<HashMap<usize, Box<SafeArrayInner>>> {
    SAFE_ARRAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Taille d'une case de SAFEARRAY, selon le programme : les pointeurs
/// (BSTR, IDispatch, IUnknown) font 4 octets en x86, et un VARIANT 24 en
/// x64 (son union contient BRECORD, deux pointeurs) contre 16 en x86 (#1902).
fn element_size_for_vt(vt: u16) -> u32 {
    element_size(vt, crate::wow32::pointer_size() as u32)
}

/// Taille d'une case pour des pointeurs de `pointer` octets (4 ou 8).
fn element_size(vt: u16, pointer: u32) -> u32 {
    match vt {
        VT_UI1 | 16 => 1,                   // VT_UI1, VT_I1
        VT_I2 | VT_BOOL | 18 => 2,          // VT_I2, VT_BOOL, VT_UI2
        VT_I4 | 4 | 10 | 19 | 22 | 23 => 4, // VT_I4, VT_R4, VT_ERROR, VT_UI4, VT_INT, VT_UINT
        5 | 6 | 7 | 20 | 21 => 8,           // VT_R8, VT_CY, VT_DATE, VT_I8, VT_UI8
        VT_BSTR | 9 | 13 => pointer,        // VT_BSTR, VT_DISPATCH, VT_UNKNOWN
        14 => 16,                           // VT_DECIMAL
        12 => 8 + 2 * pointer,              // VT_VARIANT : 16 en x86, 24 en x64
        _ => 16,
    }
}

/// sizeof(VARIANT) pour le programme : 16 en x86, 24 en x64.
fn variant_size() -> u32 {
    element_size(12, crate::wow32::pointer_size() as u32)
}

/// Case de la taille d'un pointeur du programme, à une adresse quelconque.
unsafe fn read_cell(at: *const u8) -> usize {
    if crate::wow32::pointer_size() == 4 {
        std::ptr::read_unaligned(at as *const u32) as usize
    } else {
        std::ptr::read_unaligned(at as *const usize)
    }
}

unsafe fn write_cell(at: *mut u8, v: usize) {
    if crate::wow32::pointer_size() == 4 {
        std::ptr::write_unaligned(at as *mut u32, v as u32)
    } else {
        std::ptr::write_unaligned(at as *mut usize, v)
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
    register(SafeArrayInner::new(
        vt,
        bounds,
        vec![0u8; data_size],
        elem_size,
    ))
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
    register(SafeArrayInner::new(
        vt,
        vec![bound],
        vec![0u8; data_size],
        elem_size,
    ))
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
    match map.remove(&sa) {
        Some(inner) => {
            drop(map);
            // SAFETY : éléments écrits par les fonctions SafeArray* elles-mêmes.
            unsafe {
                clear_elements(&inner);
                unk_call(2, inner.record);
            }
            0
        }
        None => E_INVALIDARG,
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
    inner.sync_header();
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
    inner.sync_header();
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
    let at = inner.data.as_ptr().add(offset);
    match inner.vt {
        // Une copie de la chaîne, de l'interface (AddRef) ou du VARIANT,
        // que l'appelant libère (Microsoft Learn, SafeArrayGetElement).
        VT_BSTR => {
            let b = read_cell(at) as *const u16;
            let copy = if b.is_null() {
                std::ptr::null_mut()
            } else {
                sys_alloc_string_len(b, sys_string_len(b))
            };
            write_cell(out as *mut u8, copy as usize);
        }
        9 | 13 => {
            let p = read_cell(at);
            if p != 0 {
                unk_call(1, p);
            }
            write_cell(out as *mut u8, p);
        }
        12 => {
            std::ptr::write_bytes(out as *mut u8, 0, elem_size);
            return variant_copy(out, at as *const c_void);
        }
        _ => std::ptr::copy_nonoverlapping(at, out as *mut u8, elem_size),
    }
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
    let at = inner.data.as_mut_ptr().add(offset);
    match inner.vt {
        // `pv` est la chaîne, l'interface ou le VARIANT lui-même : le
        // tableau en garde une copie et libère l'ancienne valeur
        // (Microsoft Learn, SafeArrayPutElement).
        VT_BSTR => {
            let b = val as *const u16;
            let copy = sys_alloc_string_len(b, sys_string_len(b));
            if copy.is_null() {
                return E_OUTOFMEMORY;
            }
            let old = read_cell(at) as *mut u16;
            write_cell(at, copy as usize);
            if !old.is_null() {
                sys_free_string(old);
            }
        }
        9 | 13 => {
            let p = val as usize;
            unk_call(1, p);
            let old = read_cell(at);
            write_cell(at, p);
            if old != 0 {
                unk_call(2, old);
            }
        }
        12 => {
            variant_clear(at as *mut c_void);
            return variant_copy(at as *mut c_void, val);
        }
        _ => std::ptr::copy_nonoverlapping(val as *const u8, at, elem_size),
    }
    0
}

/// Libère ce que tient chaque élément (chaînes, interfaces, VARIANT), comme
/// SafeArrayDestroy sous Windows.
unsafe fn clear_elements(inner: &SafeArrayInner) {
    let size = inner.element_size as usize;
    if size == 0 {
        return;
    }
    for at in inner.data.chunks_exact(size).map(|c| c.as_ptr()) {
        match inner.vt {
            VT_BSTR => {
                let b = read_cell(at) as *mut u16;
                if !b.is_null() {
                    sys_free_string(b);
                }
            }
            9 | 13 => {
                let p = read_cell(at);
                if p != 0 {
                    unk_call(2, p);
                }
            }
            12 => {
                variant_clear(at as *mut c_void);
            }
            // IRecordInfo::RecordClear de chaque élément.
            VT_RECORD if inner.record != 0 && crate::wow32::pointer_size() == 8 => {
                let rec = inner.record;
                let clear: unsafe extern "win64" fn(usize, *const u8) -> i32 =
                    std::mem::transmute(*(*(rec as *const *const usize)).add(4));
                clear(rec, at);
            }
            _ => return,
        }
    }
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

// ── Verrous, descripteurs et tableaux de structures (#1511) ──────────

pub extern "win64" fn safe_array_lock(sa: usize) -> i32 {
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    inner.locks = inner.locks.saturating_add(1);
    inner.sync_header();
    0
}

pub extern "win64" fn safe_array_unlock(sa: usize) -> i32 {
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    if inner.locks == 0 {
        return E_UNEXPECTED;
    }
    inner.locks -= 1;
    inner.sync_header();
    0
}

/// Descripteur sans données : l'appelant remplit rgsabound dans la
/// structure publique, puis appelle SafeArrayAllocData.
pub unsafe extern "win64" fn safe_array_alloc_descriptor_ex(
    vt: u16,
    dims: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if dims == 0 || dims > 0xFFFF {
        return E_INVALIDARG;
    }
    let size = if vt == VT_RECORD {
        0
    } else {
        element_size_for_vt(vt)
    };
    let bounds = vec![
        SafeArrayBound {
            elements: 0,
            lower_bound: 0,
        };
        dims as usize
    ];
    *out = register(SafeArrayInner::new(vt, bounds, Vec::new(), size));
    0
}

/// Données d'un descripteur : bornes et taille d'élément lues dans la
/// structure publique (disposition x86 ou x64).
pub extern "win64" fn safe_array_alloc_data(sa: usize) -> i32 {
    let mut map = lock(safe_arrays());
    let Some(inner) = map.get_mut(&sa) else {
        return E_INVALIDARG;
    };
    let first = if crate::wow32::pointer_size() == 4 {
        16
    } else {
        24
    };
    // SAFETY : la structure publique appartient au tableau (`header`).
    let (public_size, mut bounds) = unsafe {
        let p = sa as *const u8;
        let size = std::ptr::read_unaligned(p.add(4) as *const u32);
        let bounds: Vec<SafeArrayBound> = (0..inner.dims as usize)
            .map(|i| std::ptr::read_unaligned(p.add(first + 8 * i) as *const SafeArrayBound))
            .collect();
        (size, bounds)
    };
    bounds.reverse();
    if public_size != 0 {
        inner.element_size = public_size;
    }
    let Some(data_size) = bounds
        .iter()
        .try_fold(inner.element_size as usize, |acc, b| {
            acc.checked_mul(b.elements as usize)
        })
        .filter(|&n| n <= MAX_SAFEARRAY_BYTES)
    else {
        return E_OUTOFMEMORY;
    };
    inner.bounds = bounds;
    inner.data = vec![0u8; data_size];
    inner.sync_header();
    0
}

/// IRecordInfo::GetSize (emplacement 8) ; 0 si l'objet le refuse.
unsafe fn record_size(rec: usize) -> u32 {
    if rec == 0 || crate::wow32::pointer_size() != 8 {
        return 0;
    }
    let get: unsafe extern "win64" fn(usize, *mut u32) -> i32 =
        std::mem::transmute(*(*(rec as *const *const usize)).add(8));
    let mut size = 0u32;
    if get(rec, &mut size) < 0 {
        0
    } else {
        size
    }
}

/// SafeArrayCreateEx : pour VT_RECORD, `extra` est l'IRecordInfo des
/// éléments ; pour VT_UNKNOWN/VT_DISPATCH, un IID qui ne change rien ici.
pub unsafe extern "win64" fn safe_array_create_ex(
    vt: u16,
    dims: u32,
    bounds_ptr: usize,
    extra: usize,
) -> usize {
    if vt != VT_RECORD {
        return safe_array_create(vt, dims, bounds_ptr);
    }
    let size = record_size(extra);
    if extra == 0 || size == 0 || dims == 0 || dims > 0xFFFF || bounds_ptr == 0 {
        return 0;
    }
    let bounds =
        std::slice::from_raw_parts(bounds_ptr as *const SafeArrayBound, dims as usize).to_vec();
    let Some(data_size) = bounds
        .iter()
        .try_fold(size as usize, |acc, b| acc.checked_mul(b.elements as usize))
        .filter(|&n| n <= MAX_SAFEARRAY_BYTES)
    else {
        return 0;
    };
    let mut inner = SafeArrayInner::new(vt, bounds, vec![0u8; data_size], size);
    unk_call(1, extra);
    inner.record = extra;
    inner.sync_header();
    register(inner)
}

pub unsafe extern "win64" fn safe_array_set_record_info(sa: usize, rec: usize) -> i32 {
    let size = record_size(rec);
    let old = {
        let mut map = lock(safe_arrays());
        let Some(inner) = map.get_mut(&sa) else {
            return E_INVALIDARG;
        };
        if inner.vt != VT_RECORD {
            return E_INVALIDARG;
        }
        unk_call(1, rec);
        if inner.data.is_empty() && size != 0 {
            inner.element_size = size;
        }
        let old = std::mem::replace(&mut inner.record, rec);
        inner.sync_header();
        old
    };
    unk_call(2, old);
    0
}

pub unsafe extern "win64" fn safe_array_get_record_info(sa: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let map = lock(safe_arrays());
    let Some(inner) = map.get(&sa) else {
        return E_INVALIDARG;
    };
    if inner.vt != VT_RECORD || inner.record == 0 {
        return E_INVALIDARG;
    }
    unk_call(1, inner.record);
    *out = inner.record;
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
            inner.sync_header();
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

// GetActiveObject, RegisterActiveObject, IDispatch standard : oleaut32_disp.rs.

// GetErrorInfo, SetErrorInfo, CreateErrorInfo : voir errorinfo.rs.
use super::errorinfo::{create_error_info, get_error_info, set_error_info};

// ── Bibliothèques de types ──────────────────────────────────────────
//
// Les bibliothèques créées par CreateTypeLib2 (oleaut32_typelib.rs) et
// inscrites par RegisterTypeLib sont retrouvées par LoadRegTypeLib,
// QueryPathOfRegTypeLib et LoadTypeLib dans le processus ; stdole2.tlb
// est intégrée. Le format binaire .tlb n'étant pas publié, un fichier
// .tlb inconnu reste TYPE_E_CANTLOADLIBRARY (#1108).

use super::oleaut32_typelib as typelib;

unsafe fn read_guid(p: *const u8) -> Option<super::ole32::Guid> {
    if p.is_null() {
        return None;
    }
    Some(std::ptr::read_unaligned(p as *const super::ole32::Guid))
}

pub unsafe extern "win64" fn load_type_lib(file: *const u16, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG_TL;
    }
    *out = 0;
    if crate::wow32::active() {
        return typelib::NOT_LOADED;
    }
    match typelib::load_by_path(&typelib::wide(file)) {
        Some(t) => {
            *out = t;
            0
        }
        None => typelib::NOT_LOADED,
    }
}
pub unsafe extern "win64" fn load_type_lib_ex(
    file: *const u16,
    _kind: u32,
    out: *mut usize,
) -> i32 {
    load_type_lib(file, out)
}
pub unsafe extern "win64" fn load_reg_type_lib(
    guid: *const u8,
    major: u16,
    _minor: u16,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG_TL;
    }
    *out = 0;
    let Some(g) = read_guid(guid) else {
        return E_INVALIDARG_TL;
    };
    if crate::wow32::active() {
        return typelib::NOT_REGISTERED;
    }
    match typelib::load_registered(&g, major) {
        Some(t) => {
            *out = t;
            0
        }
        None => typelib::NOT_REGISTERED,
    }
}
pub unsafe extern "win64" fn query_path_of_reg_type_lib(
    guid: *const u8,
    major: u16,
    _minor: u16,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG_TL;
    }
    *out = 0;
    let Some(g) = read_guid(guid) else {
        return E_INVALIDARG_TL;
    };
    match typelib::registered_path(&g, major) {
        Some(path) => {
            let w: Vec<u16> = path.encode_utf16().collect();
            *out = sys_alloc_string_len(w.as_ptr(), w.len() as u32) as usize;
            0
        }
        None => typelib::NOT_REGISTERED,
    }
}
pub unsafe extern "win64" fn register_type_lib(lib: usize, path: *const u16, _help: usize) -> i32 {
    if lib == 0 || crate::wow32::active() {
        return typelib::NOT_LOADED;
    }
    typelib::register(lib, &typelib::wide(path)).unwrap_or(typelib::NOT_LOADED)
}

const E_INVALIDARG_TL: i32 = 0x8007_0057u32 as i32;

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
        "SafeArrayLock" => fn safe_array_lock,
        "SafeArrayUnlock" => fn safe_array_unlock,
        "SafeArrayAllocDescriptorEx" => fn safe_array_alloc_descriptor_ex,
        "SafeArrayAllocData" => fn safe_array_alloc_data,
        "SafeArrayCreateEx" => fn safe_array_create_ex,
        "SafeArraySetRecordInfo" => fn safe_array_set_record_info,
        "SafeArrayGetRecordInfo" => fn safe_array_get_record_info,
        "VariantCopyInd" => fn variant_copy_ind,
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
        "#40" => fn safe_array_redim,
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
        // Lot C du relevé (#1511) : numéros de la bibliothèque d'importation
        // OleAut32.Lib du SDK Windows public.
        "#21" => fn safe_array_lock, "#22" => fn safe_array_unlock,
        "#37" => fn safe_array_alloc_data, "#41" => fn safe_array_alloc_descriptor_ex,
        "#42" => fn safe_array_create_ex, "#44" => fn safe_array_set_record_info,
        "#45" => fn safe_array_get_record_info,
    }
}

#[cfg(test)]
mod safearray_tests {
    use super::*;

    unsafe fn rd<T: Copy>(p: usize) -> T {
        std::ptr::read_unaligned(p as *const T)
    }

    #[test]
    fn safearray_a_la_disposition_publique_et_copie_les_chaines() {
        // #1204 : les champs de oaidl.h se lisent directement.
        unsafe {
            let sa = safe_array_create_vector(VT_BSTR, 2, 5);
            assert_ne!(sa, 0);
            assert_eq!(rd::<u16>(sa), 1, "cDims");
            let features = rd::<u16>(sa + 2);
            assert_eq!(features & 0x0180, 0x0180, "FADF_BSTR | FADF_HAVEVARTYPE");
            assert_eq!(rd::<u32>(sa + 4), 8, "cbElements");
            assert_eq!(rd::<u16>(sa - 4), VT_BSTR, "VARTYPE avant la structure");
            assert_eq!(rd::<u32>(sa + 24), 5, "rgsabound[0].cElements");
            assert_eq!(rd::<i32>(sa + 28), 2, "rgsabound[0].lLbound");
            let mut data: *mut c_void = std::ptr::null_mut();
            assert_eq!(safe_array_access_data(sa, &mut data), 0);
            assert_eq!(rd::<usize>(sa + 16), data as usize, "pvData");
            assert_eq!(rd::<u32>(sa + 8), 1, "cLocks");
            safe_array_unaccess_data(sa);
            assert_eq!(rd::<u32>(sa + 8), 0);

            // PutElement prend la chaîne elle-même et en garde une copie.
            let text: Vec<u16> = "bonjour".encode_utf16().collect();
            let b = sys_alloc_string_len(text.as_ptr(), text.len() as u32);
            let idx = [3i32];
            assert_eq!(
                safe_array_put_element(sa, idx.as_ptr(), b as *const c_void),
                0
            );
            let stored = rd::<*mut u16>(data as usize + 8);
            assert!(!stored.is_null() && stored != b, "copie, pas le pointeur");
            assert_eq!(sys_string_len(stored), 7);
            // GetElement rend une autre copie, à libérer par l'appelant.
            let mut got: *mut u16 = std::ptr::null_mut();
            assert_eq!(
                safe_array_get_element(sa, idx.as_ptr(), &mut got as *mut _ as *mut c_void),
                0
            );
            assert!(!got.is_null() && got != stored);
            assert_eq!(std::slice::from_raw_parts(got, 7), &text[..]);
            sys_free_string(got);
            sys_free_string(b);
            assert_eq!(safe_array_destroy(sa), 0);
        }
    }

    /// #1902 : taille des cases selon le programme. En x64, un VARIANT fait
    /// 24 octets ; en x86, 16, et les pointeurs 4.
    #[test]
    fn taille_des_cases_selon_le_programme() {
        assert_eq!(element_size(12, 8), 24);
        assert_eq!(element_size(12, 4), 16);
        for vt in [VT_BSTR, 9, 13] {
            assert_eq!(element_size(vt, 8), 8);
            assert_eq!(element_size(vt, 4), 4);
        }
        assert_eq!(element_size(14, 4), 16, "VT_DECIMAL");
        assert_eq!(element_size(20, 4), 8, "VT_I8");
        assert_eq!(element_size(16, 8), 1, "VT_I1");
        assert_eq!(element_size(18, 8), 2, "VT_UI2");
        assert_eq!(element_size(10, 8), 4, "VT_ERROR");
    }

    /// #1902 : un SAFEARRAY de VARIANT d'un programme x64 a des cases de
    /// 24 octets, et PutElement/GetElement tombent sur la bonne case.
    #[test]
    fn variants_espaces_de_24_octets_en_x64() {
        unsafe {
            let sa = safe_array_create_vector(12, 0, 3);
            assert_ne!(sa, 0);
            assert_eq!(safe_array_get_elem_size(sa), 24);
            assert_eq!(rd::<u32>(sa + 4), 24, "cbElements");
            let mut v = [0u8; 24];
            v[0] = VT_I4 as u8;
            v[8..12].copy_from_slice(&0x1234_5678u32.to_le_bytes());
            for i in 0..3i32 {
                v[8] = i as u8;
                assert_eq!(
                    safe_array_put_element(sa, [i].as_ptr(), v.as_ptr().cast()),
                    0
                );
            }
            let mut data: *mut c_void = std::ptr::null_mut();
            assert_eq!(safe_array_access_data(sa, &mut data), 0);
            for i in 0..3usize {
                let at = data as usize + 24 * i;
                assert_eq!(rd::<u16>(at), VT_I4);
                assert_eq!(rd::<u32>(at + 8), 0x1234_5600 | i as u32);
            }
            safe_array_unaccess_data(sa);
            let mut got = [0u8; 24];
            assert_eq!(
                safe_array_get_element(sa, [2].as_ptr(), got.as_mut_ptr().cast()),
                0
            );
            assert_eq!(rd::<u32>(got.as_ptr() as usize + 8), 0x1234_5602);
            assert_eq!(safe_array_destroy(sa), 0);
        }
    }

    /// #1902 : VariantCopy garde pRecInfo (octets 16 à 23) d'un VT_RECORD.
    #[test]
    fn copie_d_un_variant_record_garde_record_info() {
        unsafe {
            let mut src = [0u8; 24];
            src[0] = VT_RECORD as u8;
            src[8..16].copy_from_slice(&0x1111_2222_3333_4444u64.to_le_bytes());
            src[16..24].copy_from_slice(&0x5555_6666_7777_8888u64.to_le_bytes());
            let mut dst = [0u8; 24];
            assert_eq!(
                variant_copy(dst.as_mut_ptr().cast(), src.as_ptr().cast()),
                0
            );
            assert_eq!(dst, src);
        }
    }

    #[test]
    fn safearray_bornes_dans_l_ordre_de_windows() {
        // rgsabound garde la dimension la plus à droite en premier.
        unsafe {
            let bounds = [
                SafeArrayBound {
                    elements: 3,
                    lower_bound: 0,
                },
                SafeArrayBound {
                    elements: 7,
                    lower_bound: 1,
                },
            ];
            let sa = safe_array_create(VT_I4, 2, bounds.as_ptr() as usize);
            assert_ne!(sa, 0);
            assert_eq!(rd::<u16>(sa), 2);
            assert_eq!(rd::<u32>(sa + 4), 4);
            assert_eq!((rd::<u32>(sa + 24), rd::<i32>(sa + 28)), (7, 1));
            assert_eq!((rd::<u32>(sa + 32), rd::<i32>(sa + 36)), (3, 0));
            let mut ub = 0;
            assert_eq!(safe_array_get_u_bound(sa, 2, &mut ub), 0);
            assert_eq!(ub, 7);
            // Redim change la dernière dimension et pvData suit.
            let nb = SafeArrayBound {
                elements: 9,
                lower_bound: 1,
            };
            assert_eq!(safe_array_redim(sa, &nb), 0);
            assert_eq!(rd::<u32>(sa + 24), 9);
            let mut data: *mut c_void = std::ptr::null_mut();
            safe_array_access_data(sa, &mut data);
            assert_eq!(rd::<usize>(sa + 16), data as usize);
            safe_array_unaccess_data(sa);
            assert_eq!(safe_array_destroy(sa), 0);
        }
    }
}
