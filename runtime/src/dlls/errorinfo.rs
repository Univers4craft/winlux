//! Objet d'erreur d'OLE Automation : CreateErrorInfo, SetErrorInfo,
//! GetErrorInfo (oleaut32, aussi exportées par ole32).
//!
//! Un objet implémente ICreateErrorInfo (pour le remplir) et IErrorInfo
//! (pour le lire). Chaque thread garde au plus un objet d'erreur : SetErrorInfo
//! le remplace, GetErrorInfo le rend à l'appelant et vide la case, comme le
//! décrit la documentation de Microsoft.

use super::comobj::{
    read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_FALSE: i32 = 1;

pub const IID_IERRORINFO: Guid = super::comobj::guid(
    0x1CF2_B120,
    0x547D,
    0x101B,
    [0x8E, 0x65, 0x08, 0x00, 0x2B, 0x2B, 0xD1, 0x19],
);
pub const IID_ICREATEERRORINFO: Guid = super::comobj::guid(
    0x22F0_3340,
    0x547D,
    0x101B,
    [0x8E, 0x65, 0x08, 0x00, 0x2B, 0x2B, 0xD1, 0x19],
);

#[derive(Default)]
struct Fields {
    guid: Guid,
    source: Vec<u16>,
    description: Vec<u16>,
    help_file: Vec<u16>,
    help_context: u32,
}

/// Deux tables : IErrorInfo à l'adresse de l'objet, ICreateErrorInfo 8
/// octets plus loin.
#[repr(C)]
struct ErrorObject {
    read: &'static VTable<8>,
    write: &'static VTable<8>,
    refs: AtomicU32,
    fields: Mutex<Fields>,
}

unsafe fn object(this: usize, from_write: bool) -> &'static ErrorObject {
    &*((if from_write { this - 8 } else { this }) as *const ErrorObject)
}

unsafe fn qi(obj: &ErrorObject, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = obj as *const ErrorObject as usize;
    let Some(g) = read_guid(iid as *const u8) else {
        return E_INVALIDARG;
    };
    *out = if g == IID_IUNKNOWN || g == IID_IERRORINFO {
        base
    } else if g == IID_ICREATEERRORINFO {
        base + 8
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    obj.refs.fetch_add(1, Ordering::AcqRel);
    S_OK
}

unsafe fn release(obj: &ErrorObject) -> u32 {
    let n = obj.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(obj as *const ErrorObject as *mut ErrorObject));
    }
    n
}

unsafe extern "win64" fn r_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    qi(object(this, false), iid, out)
}
unsafe extern "win64" fn r_add_ref(this: usize) -> u32 {
    object(this, false).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn r_release(this: usize) -> u32 {
    release(object(this, false))
}
unsafe extern "win64" fn w_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    qi(object(this, true), iid, out)
}
unsafe extern "win64" fn w_add_ref(this: usize) -> u32 {
    object(this, true).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn w_release(this: usize) -> u32 {
    release(object(this, true))
}

unsafe fn bstr_out(out: *mut usize, text: &[u16]) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = if text.is_empty() {
        0
    } else {
        super::oleaut32::sys_alloc_string_len(text.as_ptr(), text.len() as u32) as usize
    };
    S_OK
}

unsafe extern "win64" fn get_guid(this: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = object(this, false).fields.lock().unwrap().guid;
    S_OK
}
unsafe extern "win64" fn get_source(this: usize, out: *mut usize) -> i32 {
    bstr_out(out, &object(this, false).fields.lock().unwrap().source)
}
unsafe extern "win64" fn get_description(this: usize, out: *mut usize) -> i32 {
    bstr_out(out, &object(this, false).fields.lock().unwrap().description)
}
unsafe extern "win64" fn get_help_file(this: usize, out: *mut usize) -> i32 {
    bstr_out(out, &object(this, false).fields.lock().unwrap().help_file)
}
unsafe extern "win64" fn get_help_context(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = object(this, false).fields.lock().unwrap().help_context;
    S_OK
}

unsafe fn wide(p: *const u16) -> Vec<u16> {
    if p.is_null() {
        return Vec::new();
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    std::slice::from_raw_parts(p, n).to_vec()
}

unsafe extern "win64" fn set_guid(this: usize, g: *const Guid) -> i32 {
    object(this, true).fields.lock().unwrap().guid = read_guid(g as *const u8).unwrap_or_default();
    S_OK
}
unsafe extern "win64" fn set_source(this: usize, s: *const u16) -> i32 {
    object(this, true).fields.lock().unwrap().source = wide(s);
    S_OK
}
unsafe extern "win64" fn set_description(this: usize, s: *const u16) -> i32 {
    object(this, true).fields.lock().unwrap().description = wide(s);
    S_OK
}
unsafe extern "win64" fn set_help_file(this: usize, s: *const u16) -> i32 {
    object(this, true).fields.lock().unwrap().help_file = wide(s);
    S_OK
}
unsafe extern "win64" fn set_help_context(this: usize, c: u32) -> i32 {
    object(this, true).fields.lock().unwrap().help_context = c;
    S_OK
}

static READ_VTBL: VTable<8> = VTable([
    r_qi as *const (),
    r_add_ref as *const (),
    r_release as *const (),
    get_guid as *const (),
    get_source as *const (),
    get_description as *const (),
    get_help_file as *const (),
    get_help_context as *const (),
]);
static WRITE_VTBL: VTable<8> = VTable([
    w_qi as *const (),
    w_add_ref as *const (),
    w_release as *const (),
    set_guid as *const (),
    set_source as *const (),
    set_description as *const (),
    set_help_file as *const (),
    set_help_context as *const (),
]);

thread_local! {
    /// Objet d'erreur du thread (référence détenue), 0 si aucun.
    static CURRENT: Cell<usize> = const { Cell::new(0) };
}

unsafe fn com_release(p: usize) {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(2));
    f(p);
}

unsafe fn com_add_ref(p: usize) {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(1));
    f(p);
}

pub unsafe extern "win64" fn create_error_info(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let obj = Box::new(ErrorObject {
        read: &READ_VTBL,
        write: &WRITE_VTBL,
        refs: AtomicU32::new(1),
        fields: Mutex::new(Fields::default()),
    });
    *out = Box::into_raw(obj) as usize + 8;
    S_OK
}

pub unsafe extern "win64" fn set_error_info(_reserved: u32, info: *mut c_void) -> i32 {
    let info = info as usize;
    if info != 0 {
        com_add_ref(info);
    }
    let old = CURRENT.with(|c| c.replace(info));
    if old != 0 {
        com_release(old);
    }
    S_OK
}

pub unsafe extern "win64" fn get_error_info(_reserved: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let cur = CURRENT.with(|c| c.replace(0));
    *out = cur;
    if cur == 0 {
        S_FALSE
    } else {
        S_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erreur_creee_posee_puis_relue() {
        unsafe {
            let mut w = 0usize;
            assert_eq!(create_error_info(&mut w), S_OK);
            let text: Vec<u16> = "Disque plein\0".encode_utf16().collect();
            assert_eq!(set_description(w, text.as_ptr()), S_OK);
            let mut r = 0usize;
            assert_eq!(w_qi(w, &IID_IERRORINFO, &mut r), S_OK);
            assert_eq!(r + 8, w);
            assert_eq!(set_error_info(0, r as *mut c_void), S_OK);
            com_release(r);
            com_release(w);
            let mut out = 0usize;
            assert_eq!(get_error_info(0, &mut out), S_OK);
            assert_eq!(out, r);
            let mut desc = 0usize;
            assert_eq!(get_description(out, &mut desc), S_OK);
            assert_eq!(
                super::super::oleaut32::sys_string_len(desc as *const u16),
                12
            );
            super::super::oleaut32::sys_free_string(desc as *mut u16);
            assert_eq!(get_error_info(0, &mut out), S_FALSE);
            assert_eq!(out, 0);
            assert_eq!(r_release(r), 0);
        }
    }
}
