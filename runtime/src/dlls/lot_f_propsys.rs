//! propsys, lot F de #1410 (#1615) : PROPVARIANT et magasin de propriétés.
//!
//! D'après la documentation Microsoft Learn de propvarutil.h et
//! propsys.h, et [MS-OLEPS] § 2.15 (TypedPropertyValue) pour la forme
//! sérialisée. Un PROPVARIANT a son type sur 2 octets en tête et sa valeur
//! à +8, en x64 comme en x86 ; les types communs avec VARIANT passent par
//! la conversion d'oleaut32.

#![allow(non_snake_case)]

use super::ole32::{co_task_mem_alloc, Guid};
use std::ffi::c_void;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
/// TYPE_E_ELEMENTNOTFOUND : clé ou propriété inconnue du schéma.
const TYPE_E_ELEMENTNOTFOUND: i32 = 0x8002_802Bu32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND).
const HR_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
/// STG_E_INVALIDPARAMETER, rendu pour une forme sérialisée illisible.
const STG_E_INVALIDPARAMETER: i32 = 0x8003_0057u32 as i32;
/// DISP_E_TYPEMISMATCH.
const DISP_E_TYPEMISMATCH: i32 = 0x8002_0005u32 as i32;

const VT_EMPTY: u16 = 0;
const VT_NULL: u16 = 1;
const VT_I2: u16 = 2;
const VT_I4: u16 = 3;
const VT_R4: u16 = 4;
const VT_R8: u16 = 5;
const VT_CY: u16 = 6;
const VT_DATE: u16 = 7;
const VT_BSTR: u16 = 8;
const VT_ERROR: u16 = 10;
const VT_BOOL: u16 = 11;
const VT_I1: u16 = 16;
const VT_UI1: u16 = 17;
const VT_UI2: u16 = 18;
const VT_UI4: u16 = 19;
const VT_I8: u16 = 20;
const VT_UI8: u16 = 21;
const VT_INT: u16 = 22;
const VT_UINT: u16 = 23;
const VT_LPWSTR: u16 = 31;
const VT_FILETIME: u16 = 64;
const VT_CLSID: u16 = 72;

unsafe fn vt(pv: *const u8) -> u16 {
    std::ptr::read_unaligned(pv as *const u16)
}

unsafe fn value_ptr(pv: *const u8) -> usize {
    crate::wow32::read_ptr(pv as usize + 8)
}

unsafe fn clear(pv: *mut u8) {
    std::ptr::write_bytes(pv, 0, 16);
}

/// Types dont la valeur tient en place dans le PROPVARIANT et que la
/// conversion de VARIANT sait traiter.
fn variant_like(t: u16) -> bool {
    matches!(
        t,
        VT_EMPTY
            | VT_NULL
            | VT_I2
            | VT_I4
            | VT_R4
            | VT_R8
            | VT_CY
            | VT_DATE
            | VT_BSTR
            | VT_ERROR
            | VT_BOOL
            | VT_I1
            | VT_UI1
            | VT_UI2
            | VT_UI4
            | VT_I8
            | VT_UI8
            | VT_INT
            | VT_UINT
    )
}

unsafe fn wide_len(p: *const u16) -> usize {
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    n
}

unsafe fn alloc_wide(text: &[u16]) -> *mut u16 {
    let p = co_task_mem_alloc((text.len() + 1) * 2) as *mut u16;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
        *p.add(text.len()) = 0;
    }
    p
}

/// Texte d'un PROPVARIANT : VT_LPWSTR tel quel, VT_CLSID sous sa forme
/// « {…} », le reste par conversion en BSTR.
unsafe fn text_of(pv: *const u8) -> Result<Vec<u16>, i32> {
    match vt(pv) {
        VT_LPWSTR => {
            let p = value_ptr(pv) as *const u16;
            if p.is_null() {
                return Ok(Vec::new());
            }
            Ok(std::slice::from_raw_parts(p, wide_len(p)).to_vec())
        }
        VT_CLSID => {
            let g = value_ptr(pv) as *const Guid;
            if g.is_null() {
                return Err(E_INVALIDARG);
            }
            let mut buf = [0u16; 39];
            let n = super::ole32::string_from_guid2(g, buf.as_mut_ptr(), 39);
            Ok(buf[..(n.max(1) - 1) as usize].to_vec())
        }
        t if variant_like(t) => {
            let mut tmp = [0u64; 3];
            let dest = tmp.as_mut_ptr().cast::<c_void>();
            let hr = super::oleaut32_var::change_type(dest, pv.cast(), 0x400, 0, VT_BSTR);
            if hr < 0 {
                return Err(hr);
            }
            let b = tmp[1] as usize as *const u16;
            let text = if b.is_null() {
                Vec::new()
            } else {
                let len = super::oleaut32::sys_string_len(b) as usize;
                std::slice::from_raw_parts(b, len).to_vec()
            };
            super::oleaut32::variant_clear(dest);
            Ok(text)
        }
        _ => Err(DISP_E_TYPEMISMATCH),
    }
}

/// InitPropVariantFromFileTime : VT_FILETIME, valeur copiée.
pub unsafe extern "win64" fn InitPropVariantFromFileTime(ft: *const u64, pv: *mut u8) -> i32 {
    if ft.is_null() || pv.is_null() {
        return E_INVALIDARG;
    }
    clear(pv);
    *pv.cast::<u16>() = VT_FILETIME;
    std::ptr::write_unaligned(pv.add(8) as *mut u64, std::ptr::read_unaligned(ft));
    S_OK
}

/// PropVariantChangeType(dest, src, flags, vt) : conversions de VARIANT,
/// plus VT_LPWSTR dans les deux sens (passage par une BSTR).
pub unsafe extern "win64" fn PropVariantChangeType(
    dest: *mut u8,
    src: *const u8,
    _flags: u32,
    target: u16,
) -> i32 {
    if dest.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    let from = vt(src);
    if target == VT_LPWSTR {
        let text = match text_of(src) {
            Ok(t) => t,
            Err(hr) => return hr,
        };
        let p = alloc_wide(&text);
        if p.is_null() {
            return E_OUTOFMEMORY;
        }
        if !std::ptr::eq(dest, src) {
            clear(dest);
        } else {
            super::ole32::prop_variant_clear(dest.cast());
        }
        *dest.cast::<u16>() = VT_LPWSTR;
        crate::wow32::write_ptr(dest as usize + 8, p as usize);
        return S_OK;
    }
    if !variant_like(target) {
        return DISP_E_TYPEMISMATCH;
    }
    if from == VT_LPWSTR {
        // Chaîne large : BSTR temporaire, puis conversion de VARIANT.
        let text = match text_of(src) {
            Ok(t) => t,
            Err(hr) => return hr,
        };
        let b = super::oleaut32::sys_alloc_string_len(text.as_ptr(), text.len() as u32);
        let mut tmp = [0u64; 3];
        tmp[0] = VT_BSTR as u64;
        tmp[1] = b as usize as u64;
        let mut out = [0u64; 3];
        let hr = super::oleaut32_var::change_type(
            out.as_mut_ptr().cast(),
            tmp.as_ptr().cast(),
            0x400,
            0,
            target,
        );
        super::oleaut32::sys_free_string(b);
        if hr < 0 {
            return hr;
        }
        if std::ptr::eq(dest, src) {
            super::ole32::prop_variant_clear(dest.cast());
        }
        std::ptr::copy_nonoverlapping(out.as_ptr().cast::<u8>(), dest, 16);
        return S_OK;
    }
    if !variant_like(from) {
        return DISP_E_TYPEMISMATCH;
    }
    super::oleaut32_var::change_type(dest.cast(), src.cast(), 0x400, 0, target)
}

/// PropVariantToGUID : VT_CLSID, ou une chaîne « {…} ».
pub unsafe extern "win64" fn PropVariantToGUID(pv: *const u8, out: *mut Guid) -> i32 {
    if pv.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if vt(pv) == VT_CLSID {
        let g = value_ptr(pv) as *const Guid;
        if g.is_null() {
            return E_INVALIDARG;
        }
        std::ptr::write_unaligned(out, std::ptr::read_unaligned(g));
        return S_OK;
    }
    if !matches!(vt(pv), VT_LPWSTR | VT_BSTR) {
        return DISP_E_TYPEMISMATCH;
    }
    let text = match text_of(pv) {
        Ok(t) => String::from_utf16_lossy(&t),
        Err(hr) => return hr,
    };
    match super::ole32::parse_guid(text.trim()) {
        Some(g) => {
            std::ptr::write_unaligned(out, g);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

/// PropVariantToStringAlloc : texte alloué par CoTaskMemAlloc ; VT_EMPTY
/// donne la chaîne vide.
pub unsafe extern "win64" fn PropVariantToStringAlloc(pv: *const u8, out: usize) -> i32 {
    if pv.is_null() || out == 0 {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out, 0);
    let text = match text_of(pv) {
        Ok(t) => t,
        Err(hr) => return hr,
    };
    let p = alloc_wide(&text);
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    crate::wow32::write_ptr(out, p as usize);
    S_OK
}

/// PropVariantToUInt32 : conversion en VT_UI4 (VT_EMPTY donne 0).
pub unsafe extern "win64" fn PropVariantToUInt32(pv: *const u8, out: *mut u32) -> i32 {
    if pv.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    if vt(pv) == VT_EMPTY {
        return S_OK;
    }
    let mut tmp = [0u64; 3];
    let hr = PropVariantChangeType(tmp.as_mut_ptr().cast(), pv, 0, VT_UI4);
    if hr < 0 {
        return hr;
    }
    *out = tmp[1] as u32;
    S_OK
}

// ── Forme sérialisée ([MS-OLEPS] TypedPropertyValue) ───────────────────

/// Taille de la valeur en place pour les types de longueur fixe.
fn fixed_size(t: u16) -> Option<usize> {
    Some(match t {
        VT_EMPTY | VT_NULL => 0,
        VT_I1 | VT_UI1 => 1,
        VT_I2 | VT_UI2 | VT_BOOL => 2,
        VT_I4 | VT_UI4 | VT_INT | VT_UINT | VT_R4 | VT_ERROR => 4,
        VT_I8 | VT_UI8 | VT_R8 | VT_CY | VT_DATE | VT_FILETIME => 8,
        _ => return None,
    })
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

unsafe fn serialize(pv: *const u8) -> Result<Vec<u8>, i32> {
    let t = vt(pv);
    let mut v = Vec::with_capacity(16);
    v.extend_from_slice(&(t as u32).to_le_bytes());
    if let Some(n) = fixed_size(t) {
        v.extend_from_slice(std::slice::from_raw_parts(pv.add(8), n));
    } else {
        match t {
            VT_CLSID => {
                let g = value_ptr(pv) as *const u8;
                if g.is_null() {
                    return Err(E_INVALIDARG);
                }
                v.extend_from_slice(std::slice::from_raw_parts(g, 16));
            }
            // UnicodeString : longueur en caractères, zéro final compris.
            VT_LPWSTR => {
                let text = text_of(pv)?;
                v.extend_from_slice(&(text.len() as u32 + 1).to_le_bytes());
                for c in text.iter().chain(std::iter::once(&0)) {
                    v.extend_from_slice(&c.to_le_bytes());
                }
            }
            // CodePageString en UTF-16 : taille en octets, zéro final compris.
            VT_BSTR => {
                let text = text_of(pv)?;
                v.extend_from_slice(&((text.len() as u32 + 1) * 2).to_le_bytes());
                for c in text.iter().chain(std::iter::once(&0)) {
                    v.extend_from_slice(&c.to_le_bytes());
                }
            }
            _ => return Err(STG_E_INVALIDPARAMETER),
        }
    }
    pad4(&mut v);
    Ok(v)
}

/// StgSerializePropVariant(pv, &SERIALIZEDPROPERTYVALUE*, &cb) : forme
/// TypedPropertyValue allouée par CoTaskMemAlloc.
pub unsafe extern "win64" fn StgSerializePropVariant(
    pv: *const u8,
    out: usize,
    cb: *mut u32,
) -> i32 {
    if pv.is_null() || out == 0 || cb.is_null() {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out, 0);
    *cb = 0;
    let data = match serialize(pv) {
        Ok(d) => d,
        Err(hr) => return hr,
    };
    let p = co_task_mem_alloc(data.len()) as *mut u8;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
    crate::wow32::write_ptr(out, p as usize);
    *cb = data.len() as u32;
    S_OK
}

/// StgDeserializePropVariant(prop, cbMax, pv).
pub unsafe extern "win64" fn StgDeserializePropVariant(
    prop: *const u8,
    cb_max: u32,
    pv: *mut u8,
) -> i32 {
    if prop.is_null() || pv.is_null() || cb_max < 4 {
        return E_INVALIDARG;
    }
    clear(pv);
    let data = std::slice::from_raw_parts(prop, cb_max as usize);
    let t = u16::from_le_bytes([data[0], data[1]]);
    let body = &data[4..];
    let u32_at = |o: usize| -> Option<u32> {
        body.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if let Some(n) = fixed_size(t) {
        let Some(bytes) = body.get(..n) else {
            return STG_E_INVALIDPARAMETER;
        };
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pv.add(8), n);
        *pv.cast::<u16>() = t;
        return S_OK;
    }
    let units = |count: usize| -> Option<Vec<u16>> {
        let raw = body.get(4..4 + count * 2)?;
        let mut w: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        if w.last() == Some(&0) {
            w.pop();
        }
        Some(w)
    };
    match t {
        VT_CLSID => {
            let Some(bytes) = body.get(..16) else {
                return STG_E_INVALIDPARAMETER;
            };
            let p = co_task_mem_alloc(16) as *mut u8;
            if p.is_null() {
                return E_OUTOFMEMORY;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, 16);
            *pv.cast::<u16>() = VT_CLSID;
            crate::wow32::write_ptr(pv as usize + 8, p as usize);
            S_OK
        }
        VT_LPWSTR | VT_BSTR => {
            let Some(n) = u32_at(0) else {
                return STG_E_INVALIDPARAMETER;
            };
            let count = if t == VT_LPWSTR { n } else { n / 2 } as usize;
            let Some(w) = units(count) else {
                return STG_E_INVALIDPARAMETER;
            };
            let p = if t == VT_LPWSTR {
                alloc_wide(&w)
            } else {
                super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32)
            };
            if p.is_null() {
                return E_OUTOFMEMORY;
            }
            *pv.cast::<u16>() = t;
            crate::wow32::write_ptr(pv as usize + 8, p as usize);
            S_OK
        }
        _ => STG_E_INVALIDPARAMETER,
    }
}

// ── Système de propriétés ───────────────────────────────────────────────

/// Quelques clés de propkey.h (en-têtes publics) et leur nom canonique.
const NAMES: &[(&str, u32, &str)] = &[
    ("F29F85E0-4FF9-1068-AB91-08002B27B3D9", 2, "System.Title"),
    ("F29F85E0-4FF9-1068-AB91-08002B27B3D9", 4, "System.Author"),
    (
        "B725F130-47EF-101A-A5F1-02608C9EEBAC",
        10,
        "System.ItemNameDisplay",
    ),
    ("B725F130-47EF-101A-A5F1-02608C9EEBAC", 12, "System.Size"),
    (
        "B725F130-47EF-101A-A5F1-02608C9EEBAC",
        14,
        "System.DateModified",
    ),
    (
        "9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3",
        5,
        "System.AppUserModel.ID",
    ),
    (
        "9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3",
        2,
        "System.AppUserModel.RelaunchCommand",
    ),
];

/// PSGetNameFromPropertyKey : nom canonique alloué par CoTaskMemAlloc.
pub unsafe extern "win64" fn PSGetNameFromPropertyKey(key: *const u8, out: usize) -> i32 {
    if key.is_null() || out == 0 {
        return E_POINTER;
    }
    crate::wow32::write_ptr(out, 0);
    let fmtid = std::ptr::read_unaligned(key as *const Guid);
    let pid = std::ptr::read_unaligned(key.add(16) as *const u32);
    for (g, p, name) in NAMES {
        if *p == pid && super::ole32::parse_guid(g) == Some(fmtid) {
            let w: Vec<u16> = name.encode_utf16().collect();
            let s = alloc_wide(&w);
            if s.is_null() {
                return E_OUTOFMEMORY;
            }
            crate::wow32::write_ptr(out, s as usize);
            return S_OK;
        }
    }
    TYPE_E_ELEMENTNOTFOUND
}

/// PSCreateMemoryPropertyStore : magasin vide, modifiable.
pub unsafe extern "win64" fn PSCreateMemoryPropertyStore(
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    super::shell32_releve::memory_property_store(riid, out)
}

/// PSCoerceToCanonicalValue : sans schéma, la valeur est gardée telle quelle.
pub unsafe extern "win64" fn PSCoerceToCanonicalValue(_key: usize, _pv: usize) -> i32 {
    S_OK
}

/// PSGetPropertyDescription : aucune description de schéma n'est servie.
pub unsafe extern "win64" fn PSGetPropertyDescription(
    _key: usize,
    _riid: usize,
    out: usize,
) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    crate::wow32::write_ptr(out, 0);
    TYPE_E_ELEMENTNOTFOUND
}

/// PSLookupPropertyHandlerCLSID : aucun gestionnaire de propriétés inscrit.
pub unsafe extern "win64" fn PSLookupPropertyHandlerCLSID(_path: usize, clsid: *mut u8) -> i32 {
    if !clsid.is_null() {
        std::ptr::write_bytes(clsid, 0, 16);
    }
    HR_FILE_NOT_FOUND
}

/// PSRegisterPropertySchema / PSUnregisterPropertySchema : les schémas
/// .propdesc ne sont pas lus ; l'inscription réussit sans effet.
pub unsafe extern "win64" fn PSRegisterPropertySchema(_path: usize) -> i32 {
    S_OK
}
pub unsafe extern "win64" fn PSUnregisterPropertySchema(_path: usize) -> i32 {
    S_OK
}

/// Fonctions dont l'objet COM n'existe pas encore : E_NOTIMPL tracé, la
/// sortie à NULL.
macro_rules! absent {
    ($($name:ident($($a:ident),*) -> $out:ident;)*) => {$(
        #[allow(unused_variables)]
        pub unsafe extern "win64" fn $name($($a: usize),*) -> i32 {
            crate::trace!(concat!("propsys!", stringify!($name), " : pas encore servi (E_NOTIMPL)"));
            if $out != 0 {
                crate::wow32::write_ptr($out, 0);
            }
            E_NOTIMPL
        }
    )*};
}

absent! {
    PSCreatePropertyChangeArray(keys, flags, values, count, riid, out) -> out;
    PSCreateSimplePropertyChange(flags, key, value, riid, out) -> out;
    PSGetItemPropertyHandler(item, rw, riid, out) -> out;
    PSGetPropertySystem(riid, out) -> out;
}

/// PSGetPropertyFromPropertyStorage / PSGetNamedPropertyFromPropertyStorage :
/// pas de lecture de la forme sérialisée d'un magasin ; PROPVARIANT vide.
pub unsafe extern "win64" fn PSGetPropertyFromPropertyStorage(
    _store: usize,
    _cb: u32,
    _key: usize,
    pv: *mut u8,
) -> i32 {
    if !pv.is_null() {
        clear(pv);
    }
    E_NOTIMPL
}
pub unsafe extern "win64" fn PSGetNamedPropertyFromPropertyStorage(
    _store: usize,
    _cb: u32,
    _name: usize,
    pv: *mut u8,
) -> i32 {
    if !pv.is_null() {
        clear(pv);
    }
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "InitPropVariantFromFileTime" => fn InitPropVariantFromFileTime,
        "PropVariantChangeType" => fn PropVariantChangeType,
        "PropVariantToGUID" => fn PropVariantToGUID,
        "PropVariantToStringAlloc" => fn PropVariantToStringAlloc,
        "PropVariantToUInt32" => fn PropVariantToUInt32,
        "StgSerializePropVariant" => fn StgSerializePropVariant,
        "StgDeserializePropVariant" => fn StgDeserializePropVariant,
        "PSGetNameFromPropertyKey" => fn PSGetNameFromPropertyKey,
        "PSCreateMemoryPropertyStore" => fn PSCreateMemoryPropertyStore,
        "PSCoerceToCanonicalValue" => fn PSCoerceToCanonicalValue,
        "PSGetPropertyDescription" => fn PSGetPropertyDescription,
        "PSLookupPropertyHandlerCLSID" => fn PSLookupPropertyHandlerCLSID,
        "PSRegisterPropertySchema" => fn PSRegisterPropertySchema,
        "PSUnregisterPropertySchema" => fn PSUnregisterPropertySchema,
        "PSCreatePropertyChangeArray" => fn PSCreatePropertyChangeArray,
        "PSCreateSimplePropertyChange" => fn PSCreateSimplePropertyChange,
        "PSGetItemPropertyHandler" => fn PSGetItemPropertyHandler,
        "PSGetPropertySystem" => fn PSGetPropertySystem,
        "PSGetPropertyFromPropertyStorage" => fn PSGetPropertyFromPropertyStorage,
        "PSGetNamedPropertyFromPropertyStorage" => fn PSGetNamedPropertyFromPropertyStorage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propvariant_conversions_et_serialisation() {
        unsafe {
            // VT_I4 = 42 → texte, entier, aller-retour sérialisé.
            let v = [VT_I4 as u64, 42, 0];
            let pv = v.as_ptr().cast::<u8>();
            let mut n = 0u32;
            assert_eq!(PropVariantToUInt32(pv, &mut n), 0);
            assert_eq!(n, 42);
            let mut s = 0usize;
            assert_eq!(
                PropVariantToStringAlloc(pv, &mut s as *mut usize as usize),
                0
            );
            assert_eq!(crate::dlls::wstr(s as *const u16), "42");
            crate::dlls::ole32::co_task_mem_free(s as *mut _);

            // VT_LPWSTR → VT_UI4 et retour.
            let w: Vec<u16> = "17\0".encode_utf16().collect();
            let lp = [VT_LPWSTR as u64, w.as_ptr() as u64, 0];
            let mut d = [0u64; 3];
            assert_eq!(
                PropVariantChangeType(d.as_mut_ptr().cast(), lp.as_ptr().cast(), 0, VT_UI4),
                0
            );
            assert_eq!((d[0] as u16, d[1] as u32), (VT_UI4, 17));
            let mut back = [0u64; 3];
            assert_eq!(
                PropVariantChangeType(back.as_mut_ptr().cast(), d.as_ptr().cast(), 0, VT_LPWSTR),
                0
            );
            assert_eq!(crate::dlls::wstr(back[1] as usize as *const u16), "17");

            // Sérialisation de la chaîne : type, longueur 3, « 17\0 », bourrage.
            let mut p = 0usize;
            let mut cb = 0u32;
            assert_eq!(
                StgSerializePropVariant(
                    back.as_ptr().cast(),
                    &mut p as *mut usize as usize,
                    &mut cb
                ),
                0
            );
            let bytes = std::slice::from_raw_parts(p as *const u8, cb as usize);
            assert_eq!(&bytes[..8], &[31, 0, 0, 0, 3, 0, 0, 0]);
            assert_eq!(cb, 16);
            let mut r = [0u64; 3];
            assert_eq!(
                StgDeserializePropVariant(bytes.as_ptr(), cb, r.as_mut_ptr().cast()),
                0
            );
            assert_eq!(r[0] as u16, VT_LPWSTR);
            assert_eq!(crate::dlls::wstr(r[1] as usize as *const u16), "17");
            crate::dlls::ole32::prop_variant_clear(back.as_mut_ptr().cast());

            // FILETIME et GUID.
            let ft = 0x01D9_0000_1234_5678u64;
            let mut f = [0u64; 3];
            assert_eq!(InitPropVariantFromFileTime(&ft, f.as_mut_ptr().cast()), 0);
            assert_eq!((f[0] as u16, f[1]), (VT_FILETIME, ft));
            let gs: Vec<u16> = "{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}\0"
                .encode_utf16()
                .collect();
            let gv = [VT_LPWSTR as u64, gs.as_ptr() as u64, 0];
            let mut g = Guid::default();
            assert_eq!(PropVariantToGUID(gv.as_ptr().cast(), &mut g), 0);
            assert_eq!(g.data1, 0x9F4C2855);

            // Nom canonique d'une clé : System.AppUserModel.ID (pid 5).
            let mut key = [0u8; 20];
            std::ptr::write_unaligned(key.as_mut_ptr() as *mut Guid, g);
            key[16] = 5;
            let mut name = 0usize;
            assert_eq!(
                PSGetNameFromPropertyKey(key.as_ptr(), &mut name as *mut usize as usize),
                0
            );
            assert_eq!(
                crate::dlls::wstr(name as *const u16),
                "System.AppUserModel.ID"
            );
            key[16] = 99;
            assert_eq!(
                PSGetNameFromPropertyKey(key.as_ptr(), &mut name as *mut usize as usize),
                TYPE_E_ELEMENTNOTFOUND
            );
        }
    }
}
