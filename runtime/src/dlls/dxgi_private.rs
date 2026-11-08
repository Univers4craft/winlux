//! Données privées des objets DXGI et Direct3D 11 (Get/SetPrivateData,
//! SetPrivateDataInterface), rangées par objet et par GUID.
//!
//! Les applications y posent un nom de débogage (WKPDID_D3DDebugObjectName)
//! ou leurs propres liens entre objets, et relisent ce qu'elles ont posé :
//! une lecture sans écriture préalable rend DXGI_ERROR_NOT_FOUND.

use crate::dlls::ole32::Guid;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const DXGI_ERROR_NOT_FOUND: i32 = 0x887a_0002u32 as i32;
const DXGI_ERROR_MORE_DATA: i32 = 0x887a_0003u32 as i32;

enum Value {
    Bytes(Vec<u8>),
    /// Interface tenue (AddRef à l'écriture, Release au remplacement).
    Iface(usize),
}

type Key = (usize, [u8; 16]);

static STORE: Mutex<Option<HashMap<Key, Value>>> = Mutex::new(None);

fn key(obj: *const c_void, g: &Guid) -> Key {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&g.data1.to_le_bytes());
    b[4..6].copy_from_slice(&g.data2.to_le_bytes());
    b[6..8].copy_from_slice(&g.data3.to_le_bytes());
    b[8..16].copy_from_slice(&g.data4);
    (obj as usize, b)
}

/// AddRef (1) ou Release (2) d'une interface tenue, objet du runtime ou
/// objet du programme, x86 compris (#1090).
unsafe fn com_call(p: usize, slot: usize) {
    crate::dlls::legacy_com::unknown_call(p, slot);
}

fn drop_value(v: Value) {
    if let Value::Iface(p) = v {
        if p != 0 {
            unsafe { com_call(p, 2) };
        }
    }
}

/// GetPrivateData.
pub(crate) unsafe fn get(
    obj: *const c_void,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    if g.is_null() || size.is_null() {
        return E_INVALIDARG;
    }
    let store = crate::dlls::lock(&STORE);
    let Some(v) = store.as_ref().and_then(|m| m.get(&key(obj, &*g))) else {
        *size = 0;
        return DXGI_ERROR_NOT_FOUND;
    };
    let bytes: Vec<u8> = match v {
        Value::Bytes(b) => b.clone(),
        Value::Iface(p) => p.to_le_bytes()[..crate::dlls::legacy_com::ptr_size()].to_vec(),
    };
    if data.is_null() {
        *size = bytes.len() as u32;
        return S_OK;
    }
    if (*size as usize) < bytes.len() {
        *size = bytes.len() as u32;
        return DXGI_ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data as *mut u8, bytes.len());
    *size = bytes.len() as u32;
    if let Value::Iface(p) = v {
        if *p != 0 {
            com_call(*p, 1);
        }
    }
    S_OK
}

fn put(obj: *const c_void, g: &Guid, v: Option<Value>) {
    let old = {
        let mut store = crate::dlls::lock(&STORE);
        let m = store.get_or_insert_with(HashMap::new);
        match v {
            Some(v) => m.insert(key(obj, g), v),
            None => m.remove(&key(obj, g)),
        }
    };
    if let Some(o) = old {
        drop_value(o);
    }
}

/// SetPrivateData : taille 0 ou données nulles effacent l'entrée.
pub(crate) unsafe fn set(
    obj: *const c_void,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    let Some(g) = g.as_ref() else {
        return E_INVALIDARG;
    };
    if size == 0 || data.is_null() {
        put(obj, g, None);
    } else {
        let b = std::slice::from_raw_parts(data as *const u8, size as usize).to_vec();
        put(obj, g, Some(Value::Bytes(b)));
    }
    S_OK
}

/// SetPrivateDataInterface : l'interface est tenue jusqu'au remplacement.
pub(crate) unsafe fn set_iface(obj: *const c_void, g: *const Guid, iface: *const c_void) -> i32 {
    let Some(g) = g.as_ref() else {
        return E_INVALIDARG;
    };
    if iface.is_null() {
        put(obj, g, None);
    } else {
        com_call(iface as usize, 1);
        put(obj, g, Some(Value::Iface(iface as usize)));
    }
    S_OK
}

/// Oublie les données d'un objet détruit (son adresse peut resservir).
pub(crate) fn forget(obj: *const c_void) {
    let old: Vec<Value> = {
        let mut store = crate::dlls::lock(&STORE);
        let Some(m) = store.as_mut() else { return };
        let keys: Vec<Key> = m.keys().filter(|k| k.0 == obj as usize).copied().collect();
        keys.into_iter().filter_map(|k| m.remove(&k)).collect()
    };
    old.into_iter().for_each(drop_value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_and_sizes() {
        let obj = 0x1234usize as *const c_void;
        let g = Guid {
            data1: 0x429b_8c22,
            data2: 0x9188,
            data3: 0x4b0c,
            data4: [0x87, 0x42, 0xac, 0xb0, 0xbf, 0x85, 0xc2, 0x00],
        };
        unsafe {
            let mut n = 0u32;
            assert_eq!(
                get(obj, &g, &mut n, std::ptr::null_mut()),
                DXGI_ERROR_NOT_FOUND
            );
            let name = b"toile";
            assert_eq!(set(obj, &g, 5, name.as_ptr().cast()), S_OK);
            assert_eq!(get(obj, &g, &mut n, std::ptr::null_mut()), S_OK);
            assert_eq!(n, 5);
            let mut small = [0u8; 2];
            n = 2;
            assert_eq!(
                get(obj, &g, &mut n, small.as_mut_ptr().cast()),
                DXGI_ERROR_MORE_DATA
            );
            let mut buf = [0u8; 8];
            n = 8;
            assert_eq!(get(obj, &g, &mut n, buf.as_mut_ptr().cast()), S_OK);
            assert_eq!(&buf[..5], name);
            forget(obj);
            assert_eq!(
                get(obj, &g, &mut n, buf.as_mut_ptr().cast()),
                DXGI_ERROR_NOT_FOUND
            );
        }
    }
}
