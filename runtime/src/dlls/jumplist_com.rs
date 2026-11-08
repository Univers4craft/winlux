//! Listes de raccourcis de la barre des tâches : CLSID_DestinationList
//! (ICustomDestinationList) et le tableau vide (IObjectArray) que rend
//! BeginList.
//!
//! La barre des tâches de Winlux n'affiche pas encore de listes de
//! raccourcis : la liste est acceptée puis oubliée, et aucun élément n'est
//! jamais « retiré par l'utilisateur ». Cette classe existe toujours sous
//! Windows 7 et suivants, et WinSCP x86 se servait de l'objet sans
//! vérifier le HRESULT de CoCreateInstance (#905). Méthodes et ordre :
//! documentation Microsoft Learn d'ICustomDestinationList et
//! shobjidl.h de MinGW-w64 (domaine public).

use super::comobj::{guid, Guid, VTable};
use std::ffi::c_void;

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const CLSID_DESTINATION_LIST: Guid = guid(
    0x77F1_0CF0,
    0x3DB5,
    0x4966,
    [0xB5, 0x20, 0xB7, 0xC5, 0x4F, 0xD3, 0x5E, 0xD6],
);
pub const IID_ICUSTOM_DESTINATION_LIST: Guid = guid(
    0x6332_DEBF,
    0x87B5,
    0x4670,
    [0x90, 0xC0, 0x5E, 0x57, 0xB4, 0x08, 0xA4, 0x9E],
);
pub const IID_IOBJECT_ARRAY: Guid = guid(
    0x92CA_9DCD,
    0x5622,
    0x4BBA,
    [0xA8, 0x05, 0x5E, 0x9F, 0x54, 0x1B, 0xD8, 0xC9],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// Objet sans état : un seul exemplaire de chaque, compteur fixe.
#[repr(C)]
struct Obj {
    vtbl: *const (),
}
unsafe impl Sync for Obj {}

static LIST: Obj = Obj {
    vtbl: &LIST_VT as *const _ as *const (),
};
static EMPTY: Obj = Obj {
    vtbl: &EMPTY_VT as *const _ as *const (),
};

static LIST_VT: VTable<12> = VTable([
    list_qi as *const (),
    add_ref as *const (),
    release as *const (),
    set_app_id as *const (),
    begin_list as *const (),
    append_category as *const (),
    append_known_category as *const (),
    add_user_tasks as *const (),
    ok0 as *const (), // CommitList
    get_removed as *const (),
    delete_list as *const (),
    ok0 as *const (), // AbortList
]);

static EMPTY_VT: VTable<5> = VTable([
    empty_qi as *const (),
    add_ref as *const (),
    release as *const (),
    get_count as *const (),
    get_at as *const (),
]);

unsafe fn give(obj: &'static Obj, out: *mut *mut c_void) -> i32 {
    *out = obj as *const Obj as *mut c_void;
    S_OK
}

unsafe fn qi(obj: &'static Obj, own: &Guid, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    if *iid == IID_IUNKNOWN || *iid == *own {
        return give(obj, out);
    }
    E_NOINTERFACE
}

unsafe extern "win64" fn list_qi(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    qi(&LIST, &IID_ICUSTOM_DESTINATION_LIST, iid, out)
}

unsafe extern "win64" fn empty_qi(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    qi(&EMPTY, &IID_IOBJECT_ARRAY, iid, out)
}

extern "win64" fn add_ref(_this: usize) -> u32 {
    2
}

extern "win64" fn release(_this: usize) -> u32 {
    1
}

extern "win64" fn ok0(_this: usize) -> i32 {
    S_OK
}

extern "win64" fn set_app_id(_this: usize, _id: *const u16) -> i32 {
    S_OK
}

/// BeginList : nombre de places affichées (10, la valeur par défaut de
/// Windows) et liste des éléments retirés par l'utilisateur, ici vide.
unsafe extern "win64" fn begin_list(
    _this: usize,
    slots: *mut u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if slots.is_null() {
        return E_POINTER;
    }
    *slots = 10;
    qi(&EMPTY, &IID_IOBJECT_ARRAY, iid, out)
}

extern "win64" fn append_category(_this: usize, _name: *const u16, _items: usize) -> i32 {
    S_OK
}

extern "win64" fn append_known_category(_this: usize, _category: u32) -> i32 {
    S_OK
}

extern "win64" fn add_user_tasks(_this: usize, _items: usize) -> i32 {
    S_OK
}

unsafe extern "win64" fn get_removed(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    qi(&EMPTY, &IID_IOBJECT_ARRAY, iid, out)
}

extern "win64" fn delete_list(_this: usize, _id: *const u16) -> i32 {
    S_OK
}

unsafe extern "win64" fn get_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    *count = 0;
    S_OK
}

unsafe extern "win64" fn get_at(
    _this: usize,
    _index: u32,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_INVALIDARG
}

/// Classe intégrée pour CoCreateInstance.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    let iid = &*(iid as *const _ as *const Guid);
    if *clsid != CLSID_DESTINATION_LIST {
        return None;
    }
    Some(qi(&LIST, &IID_ICUSTOM_DESTINATION_LIST, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(x: &Guid) -> &super::super::ole32::Guid {
        unsafe { &*(x as *const Guid as *const super::super::ole32::Guid) }
    }

    #[test]
    fn liste_de_raccourcis_acceptee() {
        unsafe {
            let mut list: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(
                    g(&CLSID_DESTINATION_LIST),
                    g(&IID_ICUSTOM_DESTINATION_LIST),
                    &mut list
                ),
                Some(S_OK)
            );
            assert!(!list.is_null());
            let vt = *(list as *const *const usize);
            let begin: unsafe extern "win64" fn(
                *mut c_void,
                *mut u32,
                *const Guid,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(*vt.add(4));
            let mut slots = 0;
            let mut removed: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                begin(list, &mut slots, &IID_IOBJECT_ARRAY, &mut removed),
                S_OK
            );
            assert_eq!(slots, 10);
            let avt = *(removed as *const *const usize);
            let count: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 =
                std::mem::transmute(*avt.add(3));
            let mut n = 7;
            assert_eq!(count(removed, &mut n), S_OK);
            assert_eq!(n, 0);
            let commit: unsafe extern "win64" fn(*mut c_void) -> i32 =
                std::mem::transmute(*vt.add(8));
            assert_eq!(commit(list), S_OK);
            let other = guid(1, 2, 3, [0; 8]);
            assert_eq!(
                create_builtin(g(&CLSID_DESTINATION_LIST), g(&other), &mut list),
                Some(E_NOINTERFACE)
            );
            assert_eq!(create_builtin(g(&other), g(&IID_IUNKNOWN), &mut list), None);
        }
    }
}
