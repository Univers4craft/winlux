//! IRecordInfo d'oleaut32 (lot C du relevé, #1511) : GetRecordInfoFromTypeInfo
//! et GetRecordInfoFromGuids décrivent une structure (TKIND_RECORD) d'une
//! bibliothèque de types pour les VARIANT VT_RECORD et les SAFEARRAY de
//! structures (pywin32, .NET).
//!
//! Sources : documentation Microsoft Learn d'IRecordInfo, de TYPEATTR et
//! de VARDESC ; ordre des méthodes d'oaidl.h de MinGW-w64 (domaine
//! public). Noms et ordinaux (322, 323) : bibliothèque d'importation
//! OleAut32.Lib du SDK Windows public.
//!
//! Le type est lu une fois, à la création : champs, décalages et VARTYPE.
//! Les champs d'un type composé (structure imbriquée, tableau C) sont
//! copiés tels quels et ne sont pas lisibles par GetField. Réservé aux
//! programmes 64 bits (dispositions x64 de TYPEATTR et VARDESC).

use super::comobj::{guid, Guid, VTable};
use super::ole32_lot_c::{add_ref, read_guid, release, slot, IID_IUNKNOWN};
use super::oleaut32::{
    sys_alloc_string_len, sys_free_string, sys_string_len, variant_change_type, variant_clear,
    variant_copy, variant_copy_ind, variant_init,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const DISP_E_BADVARTYPE: i32 = 0x8002_0008u32 as i32;
const TYPE_E_FIELDNOTFOUND: i32 = 0x8002_8017u32 as i32;

const IID_IRECORD_INFO: Guid = guid(0x0000_002F, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const TKIND_RECORD: u32 = 1;
const MEMBERID_NIL: i32 = -1;
const VARIANT_SIZE: usize = 24;
const VT_BYREF: u16 = 0x4000;

struct Field {
    name: String,
    offset: usize,
    vt: u16,
}

#[repr(C)]
struct RecordInfo {
    vtbl: *const (),
    refs: AtomicU32,
    tinfo: usize,
    guid: Guid,
    name: String,
    size: u32,
    fields: Vec<Field>,
}

static RECORD_VT: VTable<19> = VTable([
    ri_qi as *const (),
    ri_add_ref as *const (),
    ri_release as *const (),
    ri_record_init as *const (),
    ri_record_clear as *const (),
    ri_record_copy as *const (),
    ri_get_guid as *const (),
    ri_get_name as *const (),
    ri_get_size as *const (),
    ri_get_type_info as *const (),
    ri_get_field as *const (),
    ri_get_field_no_copy as *const (),
    ri_put_field as *const (),
    ri_put_field as *const (), // PutFieldNoCopy : même effet ici.
    ri_get_field_names as *const (),
    ri_is_matching_type as *const (),
    ri_record_create as *const (),
    ri_record_create_copy as *const (),
    ri_record_destroy as *const (),
]);

/// Octets qu'occupe un champ de ce VARTYPE ; None pour un type composé.
fn vt_size(vt: u16) -> Option<usize> {
    Some(match vt {
        16 | 17 => 1,                        // I1, UI1
        2 | 18 | 11 => 2,                    // I2, UI2, BOOL
        3 | 4 | 10 | 19 | 22 | 23 | 25 => 4, // I4, R4, ERROR, UI4, INT, UINT, HRESULT
        5..=7 | 20 | 21 => 8,                // R8, CY, DATE, I8, UI8
        8 | 9 | 13 | 26 => 8,                // BSTR, DISPATCH, UNKNOWN, PTR
        12 => VARIANT_SIZE,                  // VARIANT
        14 => 16,                            // DECIMAL
        _ => return None,
    })
}

unsafe fn ri(p: usize) -> &'static RecordInfo {
    &*(p as *const RecordInfo)
}

unsafe fn wide(p: *const u16) -> String {
    super::oleaut32_typelib::wide(p)
}

unsafe fn bstr(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().collect();
    sys_alloc_string_len(w.as_ptr(), w.len() as u32)
}

/// ITypeInfo::GetDocumentation (emplacement 12) : nom seul.
unsafe fn doc_name(ti: usize, memid: i32) -> String {
    let f: unsafe extern "win64" fn(usize, i32, *mut *mut u16, usize, usize, usize) -> i32 =
        std::mem::transmute(slot(ti, 12));
    let mut name: *mut u16 = std::ptr::null_mut();
    if f(ti, memid, &mut name, 0, 0, 0) < 0 || name.is_null() {
        return String::new();
    }
    let s = wide(name);
    sys_free_string(name);
    s
}

/// Lit TYPEATTR et les VARDESC du type (dispositions x64).
unsafe fn describe(ti: usize) -> Result<(Guid, u32, Vec<Field>), i32> {
    let get_attr: unsafe extern "win64" fn(usize, *mut *const u8) -> i32 =
        std::mem::transmute(slot(ti, 3));
    let release_attr: unsafe extern "win64" fn(usize, *const u8) =
        std::mem::transmute(slot(ti, 19));
    let mut attr: *const u8 = std::ptr::null();
    let hr = get_attr(ti, &mut attr);
    if hr < 0 || attr.is_null() {
        return Err(if hr < 0 { hr } else { E_INVALIDARG });
    }
    let g: Guid = std::ptr::read_unaligned(attr as *const Guid);
    let size = std::ptr::read_unaligned(attr.add(40) as *const u32);
    let kind = std::ptr::read_unaligned(attr.add(44) as *const u32);
    let vars = std::ptr::read_unaligned(attr.add(50) as *const u16);
    release_attr(ti, attr);
    if kind != TKIND_RECORD {
        return Err(E_INVALIDARG);
    }
    let get_var: unsafe extern "win64" fn(usize, u32, *mut *const u8) -> i32 =
        std::mem::transmute(slot(ti, 6));
    let release_var: unsafe extern "win64" fn(usize, *const u8) = std::mem::transmute(slot(ti, 21));
    let mut fields = Vec::new();
    for i in 0..u32::from(vars) {
        let mut vd: *const u8 = std::ptr::null();
        if get_var(ti, i, &mut vd) < 0 || vd.is_null() {
            continue;
        }
        let memid = std::ptr::read_unaligned(vd as *const i32);
        let offset = std::ptr::read_unaligned(vd.add(16) as *const u32) as usize;
        let vt = std::ptr::read_unaligned(vd.add(32) as *const u16);
        // VAR_PERINSTANCE seulement : une constante n'occupe aucun octet.
        let per_instance = std::ptr::read_unaligned(vd.add(60) as *const u32) == 0;
        release_var(ti, vd);
        if per_instance {
            fields.push(Field {
                name: doc_name(ti, memid),
                offset,
                vt,
            });
        }
    }
    // Taille au moins jusqu'au bout du dernier champ connu.
    let end = fields
        .iter()
        .map(|f| f.offset + vt_size(f.vt).unwrap_or(0))
        .max()
        .unwrap_or(0) as u32;
    Ok((g, size.max(end), fields))
}

pub unsafe extern "win64" fn get_record_info_from_type_info(ti: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if ti == 0 {
        return E_INVALIDARG;
    }
    if crate::wow32::pointer_size() != 8 {
        return E_NOTIMPL;
    }
    let (g, size, fields) = match describe(ti) {
        Ok(d) => d,
        Err(hr) => return hr,
    };
    add_ref(ti);
    let obj = Box::new(RecordInfo {
        vtbl: &RECORD_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        tinfo: ti,
        guid: g,
        name: doc_name(ti, MEMBERID_NIL),
        size,
        fields,
    });
    *out = Box::into_raw(obj) as usize;
    S_OK
}

pub unsafe extern "win64" fn get_record_info_from_guids(
    lib: *const Guid,
    major: u32,
    minor: u32,
    lcid: u32,
    type_guid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if lib.is_null() || type_guid.is_null() {
        return E_INVALIDARG;
    }
    let mut tl = 0usize;
    let hr = super::oleaut32::load_reg_type_lib(
        lib as *const u8,
        major as u16,
        minor as u16,
        lcid,
        &mut tl,
    );
    if hr < 0 || tl == 0 {
        return if hr < 0 { hr } else { E_INVALIDARG };
    }
    let of_guid: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(slot(tl, 6));
    let mut ti = 0usize;
    let mut hr = of_guid(tl, type_guid, &mut ti);
    if hr >= 0 && ti != 0 {
        hr = get_record_info_from_type_info(ti, out);
        release(ti);
    }
    release(tl);
    hr
}

unsafe extern "win64" fn ri_qi(p: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IRECORD_INFO => {
            ri_add_ref(p);
            *out = p;
            S_OK
        }
        Some(_) => E_NOINTERFACE,
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn ri_add_ref(p: usize) -> u32 {
    ri(p).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn ri_release(p: usize) -> u32 {
    let left = ri(p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        let b = Box::from_raw(p as *mut RecordInfo);
        release(b.tinfo);
    }
    left
}

unsafe extern "win64" fn ri_record_init(p: usize, data: *mut u8) -> i32 {
    if data.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(data, 0, ri(p).size as usize);
    S_OK
}

/// Libère ce que tient un champ (chaîne, interface, VARIANT) et le remet à zéro.
unsafe fn clear_field(at: *mut u8, vt: u16) {
    match vt {
        8 => {
            let b = std::ptr::read_unaligned(at as *const *mut u16);
            if !b.is_null() {
                sys_free_string(b);
            }
            std::ptr::write_unaligned(at as *mut usize, 0);
        }
        9 | 13 => {
            let o = std::ptr::read_unaligned(at as *const usize);
            if o != 0 {
                release(o);
            }
            std::ptr::write_unaligned(at as *mut usize, 0);
        }
        12 => {
            variant_clear(at.cast());
        }
        _ => {}
    }
}

unsafe extern "win64" fn ri_record_clear(p: usize, data: *mut u8) -> i32 {
    if data.is_null() {
        return E_INVALIDARG;
    }
    for f in &ri(p).fields {
        clear_field(data.add(f.offset), f.vt);
    }
    S_OK
}

unsafe extern "win64" fn ri_record_copy(p: usize, src: *const u8, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() {
        return E_INVALIDARG;
    }
    if std::ptr::eq(src, dst) {
        return S_OK;
    }
    ri_record_clear(p, dst);
    std::ptr::copy_nonoverlapping(src, dst, ri(p).size as usize);
    for f in &ri(p).fields {
        let (s, d) = (src.add(f.offset), dst.add(f.offset));
        match f.vt {
            8 => {
                let b = std::ptr::read_unaligned(s as *const *const u16);
                let copy = if b.is_null() {
                    std::ptr::null_mut()
                } else {
                    sys_alloc_string_len(b, sys_string_len(b))
                };
                std::ptr::write_unaligned(d as *mut *mut u16, copy);
            }
            9 | 13 => {
                let o = std::ptr::read_unaligned(s as *const usize);
                if o != 0 {
                    add_ref(o);
                }
            }
            12 => {
                variant_init(d.cast());
                variant_copy(d.cast(), s.cast());
            }
            _ => {}
        }
    }
    S_OK
}

unsafe extern "win64" fn ri_get_guid(p: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_unaligned(out, ri(p).guid);
    S_OK
}

unsafe extern "win64" fn ri_get_name(p: usize, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = bstr(&ri(p).name);
    S_OK
}

unsafe extern "win64" fn ri_get_size(p: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = ri(p).size;
    S_OK
}

unsafe extern "win64" fn ri_get_type_info(p: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    add_ref(ri(p).tinfo);
    *out = ri(p).tinfo;
    S_OK
}

unsafe fn field<'a>(p: usize, name: *const u16) -> Result<&'a Field, i32> {
    if name.is_null() {
        return Err(E_INVALIDARG);
    }
    let name = wide(name);
    let f = ri(p)
        .fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(&name))
        .ok_or(TYPE_E_FIELDNOTFOUND)?;
    if vt_size(f.vt).is_none() {
        return Err(DISP_E_BADVARTYPE);
    }
    Ok(f)
}

unsafe extern "win64" fn ri_get_field(
    p: usize,
    data: *const u8,
    name: *const u16,
    out: *mut u8,
) -> i32 {
    if data.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let f = match field(p, name) {
        Ok(f) => f,
        Err(hr) => return hr,
    };
    variant_clear(out.cast());
    let mut by_ref = [0u8; VARIANT_SIZE];
    std::ptr::write_unaligned(by_ref.as_mut_ptr() as *mut u16, f.vt | VT_BYREF);
    std::ptr::write_unaligned(
        by_ref.as_mut_ptr().add(8) as *mut usize,
        data.add(f.offset) as usize,
    );
    variant_copy_ind(out.cast(), by_ref.as_ptr().cast())
}

unsafe extern "win64" fn ri_get_field_no_copy(
    p: usize,
    data: *mut u8,
    name: *const u16,
    out: *mut u8,
    field_ptr: *mut *mut c_void,
) -> i32 {
    if data.is_null() || out.is_null() || field_ptr.is_null() {
        return E_INVALIDARG;
    }
    let f = match field(p, name) {
        Ok(f) => f,
        Err(hr) => return hr,
    };
    variant_clear(out.cast());
    let at = data.add(f.offset);
    std::ptr::write_unaligned(out as *mut u16, f.vt | VT_BYREF);
    std::ptr::write_unaligned(out.add(8) as *mut usize, at as usize);
    *field_ptr = at.cast();
    S_OK
}

/// PutField : la valeur est convertie au type du champ, puis remplace
/// l'ancienne (libérée).
unsafe extern "win64" fn ri_put_field(
    p: usize,
    _flags: u32,
    data: *mut u8,
    name: *const u16,
    value: *const u8,
) -> i32 {
    if data.is_null() || value.is_null() {
        return E_INVALIDARG;
    }
    let f = match field(p, name) {
        Ok(f) => f,
        Err(hr) => return hr,
    };
    let mut tmp = [0u8; VARIANT_SIZE];
    variant_init(tmp.as_mut_ptr().cast());
    let hr = if f.vt == 12 {
        variant_copy_ind(tmp.as_mut_ptr().cast(), value.cast())
    } else {
        variant_change_type(tmp.as_mut_ptr().cast(), value.cast(), 0, f.vt)
    };
    if hr < 0 {
        return hr;
    }
    let at = data.add(f.offset);
    clear_field(at, f.vt);
    match f.vt {
        12 => std::ptr::copy_nonoverlapping(tmp.as_ptr(), at, VARIANT_SIZE),
        14 => {
            // DECIMAL occupe tout le VARIANT ; son wReserved repart à zéro.
            std::ptr::copy_nonoverlapping(tmp.as_ptr(), at, 16);
            std::ptr::write_unaligned(at as *mut u16, 0);
        }
        vt => {
            let n = vt_size(vt).unwrap_or(0);
            std::ptr::copy_nonoverlapping(tmp.as_ptr().add(8), at, n);
        }
    }
    S_OK
}

unsafe extern "win64" fn ri_get_field_names(
    p: usize,
    count: *mut u32,
    names: *mut *mut u16,
) -> i32 {
    if count.is_null() {
        return E_INVALIDARG;
    }
    let fields = &ri(p).fields;
    if names.is_null() {
        *count = fields.len() as u32;
        return S_OK;
    }
    let n = (*count as usize).min(fields.len());
    for (i, f) in fields.iter().take(n).enumerate() {
        *names.add(i) = bstr(&f.name);
    }
    *count = n as u32;
    S_OK
}

unsafe extern "win64" fn ri_is_matching_type(p: usize, other: usize) -> i32 {
    if other == 0 {
        return 0;
    }
    let get: unsafe extern "win64" fn(usize, *mut Guid) -> i32 =
        std::mem::transmute(slot(other, 6));
    let mut g = [0u8; 16];
    (get(other, &mut g) >= 0 && g == ri(p).guid) as i32
}

unsafe extern "win64" fn ri_record_create(p: usize) -> *mut u8 {
    let size = ri(p).size as usize;
    let m = super::ole32::co_task_mem_alloc(size) as *mut u8;
    if !m.is_null() {
        std::ptr::write_bytes(m, 0, size);
    }
    m
}

unsafe extern "win64" fn ri_record_create_copy(p: usize, src: *const u8, out: *mut *mut u8) -> i32 {
    if src.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = ri_record_create(p);
    if (*out).is_null() {
        return E_OUTOFMEMORY;
    }
    ri_record_copy(p, src, *out)
}

unsafe extern "win64" fn ri_record_destroy(p: usize, data: *mut u8) -> i32 {
    if data.is_null() {
        return S_OK;
    }
    ri_record_clear(p, data);
    super::ole32::co_task_mem_free(data.cast());
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetRecordInfoFromTypeInfo" => fn get_record_info_from_type_info,
        "GetRecordInfoFromGuids" => fn get_record_info_from_guids,
        "#322" => fn get_record_info_from_guids,
        "#323" => fn get_record_info_from_type_info,
    }
}

#[cfg(test)]
#[path = "oleaut32_record/tests.rs"]
mod tests;
