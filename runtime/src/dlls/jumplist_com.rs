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

// ── CLSID_EnumerableObjectCollection ────────────────────────────────
//
// IObjectCollection : tableau d'objets COM qu'un programme remplit avant
// AddUserTasks/AppendCategory (WinSCP x86, #905). Chaque objet ajouté est
// retenu (AddRef) et rendu par GetAt après QueryInterface.

pub const CLSID_ENUMERABLE_OBJECT_COLLECTION: Guid = guid(
    0x2D34_68C1,
    0x36A7,
    0x43B6,
    [0xAC, 0x24, 0xD3, 0xF0, 0x2F, 0xD9, 0x60, 0x7A],
);
pub const IID_IOBJECT_COLLECTION: Guid = guid(
    0x5632_B1A4,
    0xE38A,
    0x400A,
    [0x92, 0x8A, 0xD4, 0xCD, 0x63, 0x23, 0x02, 0x95],
);

#[repr(C)]
struct Coll {
    vtbl: *const (),
    refs: std::sync::atomic::AtomicU32,
    items: std::sync::Mutex<Vec<usize>>,
}

static COLL_VT: VTable<9> = VTable([
    coll_qi as *const (),
    coll_add_ref as *const (),
    coll_release as *const (),
    coll_get_count as *const (),
    coll_get_at as *const (),
    coll_add_object as *const (),
    coll_add_from_array as *const (),
    coll_remove_at as *const (),
    coll_clear as *const (),
]);

type Qi = unsafe extern "win64" fn(usize, *const Guid, *mut *mut c_void) -> i32;
type Ref = unsafe extern "win64" fn(usize) -> u32;

unsafe fn method(obj: usize, index: usize) -> usize {
    *(*(obj as *const *const usize)).add(index)
}

unsafe fn com_add_ref(obj: usize) {
    let f: Ref = std::mem::transmute(method(obj, 1));
    f(obj);
}

unsafe fn com_release(obj: usize) {
    let f: Ref = std::mem::transmute(method(obj, 2));
    f(obj);
}

fn new_collection() -> *mut Coll {
    Box::into_raw(Box::new(Coll {
        vtbl: &COLL_VT as *const _ as *const (),
        refs: std::sync::atomic::AtomicU32::new(1),
        items: std::sync::Mutex::new(Vec::new()),
    }))
}

fn items(c: &Coll) -> std::sync::MutexGuard<'_, Vec<usize>> {
    c.items.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "win64" fn coll_qi(this: *mut Coll, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    if *iid == IID_IUNKNOWN || *iid == IID_IOBJECT_ARRAY || *iid == IID_IOBJECT_COLLECTION {
        coll_add_ref(this);
        *out = this.cast();
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "win64" fn coll_add_ref(this: *mut Coll) -> u32 {
    (*this)
        .refs
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1
}

unsafe extern "win64" fn coll_release(this: *mut Coll) -> u32 {
    let n = (*this)
        .refs
        .fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
        - 1;
    if n == 0 {
        let b = Box::from_raw(this);
        let list = std::mem::take(&mut *items(&b));
        drop(b);
        for o in list {
            com_release(o);
        }
    }
    n
}

unsafe extern "win64" fn coll_get_count(this: *mut Coll, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    *count = items(&*this).len() as u32;
    S_OK
}

unsafe extern "win64" fn coll_get_at(
    this: *mut Coll,
    index: u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(obj) = items(&*this).get(index as usize).copied() else {
        return E_INVALIDARG;
    };
    let qi: Qi = std::mem::transmute(method(obj, 0));
    qi(obj, iid, out)
}

unsafe extern "win64" fn coll_add_object(this: *mut Coll, obj: usize) -> i32 {
    if obj == 0 {
        return E_INVALIDARG;
    }
    com_add_ref(obj);
    items(&*this).push(obj);
    S_OK
}

/// AddFromArray : chaque élément d'un autre IObjectArray, en IUnknown.
unsafe extern "win64" fn coll_add_from_array(this: *mut Coll, array: usize) -> i32 {
    if array == 0 {
        return E_INVALIDARG;
    }
    let count: unsafe extern "win64" fn(usize, *mut u32) -> i32 =
        std::mem::transmute(method(array, 3));
    let get_at: unsafe extern "win64" fn(usize, u32, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(method(array, 4));
    let mut n = 0u32;
    let hr = count(array, &mut n);
    if hr < 0 {
        return hr;
    }
    for i in 0..n {
        let mut o: *mut c_void = std::ptr::null_mut();
        let hr = get_at(array, i, &IID_IUNKNOWN, &mut o);
        if hr < 0 {
            return hr;
        }
        // GetAt a déjà compté la référence : elle passe à la collection.
        items(&*this).push(o as usize);
    }
    S_OK
}

unsafe extern "win64" fn coll_remove_at(this: *mut Coll, index: u32) -> i32 {
    let removed = {
        let mut l = items(&*this);
        if (index as usize) >= l.len() {
            return E_INVALIDARG;
        }
        l.remove(index as usize)
    };
    com_release(removed);
    S_OK
}

unsafe extern "win64" fn coll_clear(this: *mut Coll) -> i32 {
    let list = std::mem::take(&mut *items(&*this));
    for o in list {
        com_release(o);
    }
    S_OK
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
    if *clsid == CLSID_ENUMERABLE_OBJECT_COLLECTION {
        let c = new_collection();
        let hr = coll_qi(c, iid, out);
        coll_release(c);
        return Some(hr);
    }
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

            // Collection : la liste elle-même y entre (objet COM quelconque).
            let mut coll: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(
                    g(&CLSID_ENUMERABLE_OBJECT_COLLECTION),
                    g(&IID_IOBJECT_COLLECTION),
                    &mut coll
                ),
                Some(S_OK)
            );
            let c = coll as *mut Coll;
            let list = &LIST as *const Obj as *mut c_void;
            assert_eq!(coll_add_object(c, list as usize), S_OK);
            assert_eq!(coll_add_object(c, removed as usize), S_OK);
            let mut n = 0;
            coll_get_count(c, &mut n);
            assert_eq!(n, 2);
            let mut back: *mut c_void = std::ptr::null_mut();
            assert_eq!(coll_get_at(c, 1, &IID_IOBJECT_ARRAY, &mut back), S_OK);
            assert_eq!(back, removed);
            assert_eq!(coll_get_at(c, 5, &IID_IUNKNOWN, &mut back), E_INVALIDARG);
            // Une seconde collection reprend la première.
            let other_coll = new_collection();
            assert_eq!(coll_add_from_array(other_coll, coll as usize), S_OK);
            coll_get_count(other_coll, &mut n);
            assert_eq!(n, 2);
            assert_eq!(coll_remove_at(other_coll, 0), S_OK);
            assert_eq!(coll_clear(c), S_OK);
            coll_get_count(c, &mut n);
            assert_eq!(n, 0);
            assert_eq!(coll_release(other_coll), 0);
            assert_eq!(coll_release(c), 0);
        }
    }
}
