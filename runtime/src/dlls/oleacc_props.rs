//! Service d'annotations d'accessibilité : CLSID_AccPropServices
//! (IAccPropServices, oleacc.h, #1114).
//!
//! Un programme y dépose des propriétés MSAA (nom, rôle, état…) pour une
//! fenêtre, un élément de menu ou une chaîne d'identité, sous forme de
//! valeur ou de serveur (IAccPropServer) qui les rendra à la demande. La
//! VCL de Delphi s'en sert pour tous ses contrôles. Les annotations sont
//! retenues ici (valeurs copiées, serveurs retenus par AddRef) jusqu'à
//! leur effacement ; aucun lecteur d'écran ne les lit encore.
//!
//! Les chaînes d'identité sont opaques pour le programme (documentation
//! Microsoft Learn de ComposeHwndIdentityString) : Winlux les code sur
//! 20 octets, `W`, genre (1 fenêtre, 2 menu), deux octets nuls, idObject,
//! idChild puis la poignée sur 8 octets. Méthodes et ordre : documentation
//! Microsoft Learn d'IAccPropServices et oleacc.h de MinGW-w64.

use super::comobj::{guid, Guid, VTable};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const CLSID_ACC_PROP_SERVICES: Guid = guid(
    0xB5F8_350B,
    0x0548,
    0x48B1,
    [0xA6, 0xEE, 0x88, 0xBD, 0x00, 0xB4, 0xA5, 0xE7],
);
pub const IID_IACC_PROP_SERVICES: Guid = guid(
    0x6E26_E776,
    0x04F0,
    0x495D,
    [0x80, 0xE4, 0x33, 0x30, 0x35, 0x2E, 0x31, 0x69],
);
pub const IID_IACC_PROP_SERVER: Guid = guid(
    0x76C0_DBBB,
    0x15E0,
    0x4E7B,
    [0xB6, 0x1B, 0x20, 0xEE, 0xEA, 0x20, 0x01, 0xE0],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

const KIND_HWND: u8 = 1;
const KIND_HMENU: u8 = 2;
const ID_LEN: usize = 20;
const VT_BSTR: u16 = 8;

/// Annotation retenue pour une propriété.
enum Entry {
    /// VARIANT copié (24 octets, la taille x64).
    Value([u64; 3]),
    /// Serveur retenu (AddRef) ; la portée (ANNO_THIS, ANNO_CONTAINER)
    /// n'est vérifiée qu'à la pose, faute de lecteur d'écran.
    Server(usize),
}

type Store = HashMap<Vec<u8>, Vec<(Guid, Entry)>>;

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut g = STORE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

unsafe fn method(obj: usize, index: usize) -> usize {
    *(*(obj as *const *const usize)).add(index)
}

unsafe fn com_add_ref(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(method(obj, 1));
    f(obj);
}

unsafe fn com_release(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(method(obj, 2));
    f(obj);
}

unsafe fn drop_entry(e: Entry) {
    match e {
        Entry::Value(mut v) => {
            super::oleaut32::variant_clear(v.as_mut_ptr().cast());
        }
        Entry::Server(s) => com_release(s),
    }
}

/// Remplace (ou ajoute) l'annotation `prop` de l'identité `id`.
unsafe fn put(id: Vec<u8>, prop: Guid, e: Entry) {
    let old = with_store(|s| {
        let list = s.entry(id).or_default();
        match list.iter_mut().find(|(p, _)| *p == prop) {
            Some(slot) => Some(std::mem::replace(&mut slot.1, e)),
            None => {
                list.push((prop, e));
                None
            }
        }
    });
    if let Some(o) = old {
        drop_entry(o);
    }
}

/// Retire les annotations `props` de l'identité `id`.
unsafe fn clear(id: &[u8], props: &[Guid]) {
    let gone = with_store(|s| {
        let Some(list) = s.get_mut(id) else {
            return Vec::new();
        };
        let mut gone = Vec::new();
        let mut i = 0;
        while i < list.len() {
            if props.contains(&list[i].0) {
                gone.push(list.swap_remove(i).1);
            } else {
                i += 1;
            }
        }
        if list.is_empty() {
            s.remove(id);
        }
        gone
    });
    for e in gone {
        drop_entry(e);
    }
}

/// Nombre d'annotations retenues pour une identité.
#[cfg(test)]
fn annotation_count(id: &[u8]) -> usize {
    with_store(|s| s.get(id).map_or(0, Vec::len))
}

/// Valeur annotée d'une propriété, copiée dans `out` (VARIANT x64).
#[cfg(test)]
unsafe fn annotation_value(id: &[u8], prop: &Guid, out: *mut c_void) -> bool {
    let v = with_store(|s| {
        s.get(id)?.iter().find_map(|(p, e)| match e {
            Entry::Value(v) if p == prop => Some(*v),
            _ => None,
        })
    });
    match v {
        Some(v) => super::oleaut32::variant_copy(out, v.as_ptr().cast()) >= 0,
        None => false,
    }
}

fn compose(kind: u8, handle: u64, id_object: u32, id_child: u32) -> [u8; ID_LEN] {
    let mut b = [0u8; ID_LEN];
    b[0] = b'W';
    b[1] = kind;
    b[4..8].copy_from_slice(&id_object.to_le_bytes());
    b[8..12].copy_from_slice(&id_child.to_le_bytes());
    b[12..20].copy_from_slice(&handle.to_le_bytes());
    b
}

/// (poignée, idObject, idChild) d'une chaîne d'identité du genre voulu.
fn decompose(id: &[u8], kind: u8) -> Option<(u64, u32, u32)> {
    if id.len() != ID_LEN || id[0] != b'W' || id[1] != kind {
        return None;
    }
    let d = |r: std::ops::Range<usize>| u32::from_le_bytes(id[r].try_into().unwrap());
    let h = u64::from_le_bytes(id[12..20].try_into().unwrap());
    Some((h, d(4..8), d(8..12)))
}

unsafe fn id_bytes(p: *const u8, len: u32) -> Option<Vec<u8>> {
    if p.is_null() || len == 0 {
        return None;
    }
    Some(std::slice::from_raw_parts(p, len as usize).to_vec())
}

unsafe fn props(p: *const Guid, n: i32) -> Option<Vec<Guid>> {
    if n < 0 || (n > 0 && p.is_null()) {
        return None;
    }
    Some(
        (0..n as usize)
            .map(|i| std::ptr::read_unaligned(p.add(i)))
            .collect(),
    )
}

unsafe fn give_id(id: [u8; ID_LEN], out: *mut *mut u8, len: *mut u32) -> i32 {
    if out.is_null() || len.is_null() {
        return E_POINTER;
    }
    let p = super::ole32::co_task_mem_alloc(ID_LEN) as *mut u8;
    if p.is_null() {
        *out = std::ptr::null_mut();
        *len = 0;
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(id.as_ptr(), p, ID_LEN);
    *out = p;
    *len = ID_LEN as u32;
    S_OK
}

// ── Méthodes ────────────────────────────────────────────────────────
//
// En x64, MSAAPROPID et VARIANT passés par valeur (plus de 8 octets)
// arrivent par l'adresse d'une copie.

#[repr(C)]
struct Obj {
    vtbl: *const (),
}
unsafe impl Sync for Obj {}

static SERVICES: Obj = Obj {
    vtbl: &VT as *const _ as *const (),
};

static VT: VTable<18> = VTable([
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    set_prop_value as *const (),
    set_prop_server as *const (),
    clear_props as *const (),
    set_hwnd_prop as *const (),
    set_hwnd_prop_str as *const (),
    set_hwnd_prop_server as *const (),
    clear_hwnd_props as *const (),
    compose_hwnd as *const (),
    decompose_hwnd as *const (),
    set_hmenu_prop as *const (),
    set_hmenu_prop_str as *const (),
    set_hmenu_prop_server as *const (),
    clear_hmenu_props as *const (),
    compose_hmenu as *const (),
    decompose_hmenu as *const (),
]);

unsafe extern "win64" fn qi(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    if *iid == IID_IUNKNOWN || *iid == IID_IACC_PROP_SERVICES {
        *out = &SERVICES as *const Obj as *mut c_void;
        return S_OK;
    }
    super::comobj::log_no_interface("AccPropServices", iid);
    E_NOINTERFACE
}

// Objet unique sans état propre : compteur fixe.
extern "win64" fn add_ref(_this: usize) -> u32 {
    2
}

extern "win64" fn release(_this: usize) -> u32 {
    1
}

unsafe fn set_value(id: Option<Vec<u8>>, prop: *const Guid, var: *const c_void) -> i32 {
    let (Some(id), false, false) = (id, prop.is_null(), var.is_null()) else {
        return E_INVALIDARG;
    };
    let mut v = [0u64; 3];
    let hr = super::oleaut32::variant_copy(v.as_mut_ptr().cast(), var);
    if hr < 0 {
        return hr;
    }
    put(id, std::ptr::read_unaligned(prop), Entry::Value(v));
    S_OK
}

unsafe fn set_str(id: Vec<u8>, prop: *const Guid, s: *const u16) -> i32 {
    if prop.is_null() || s.is_null() {
        return E_INVALIDARG;
    }
    let b = super::oleaut32::sys_alloc_string(s);
    if b.is_null() {
        return E_OUTOFMEMORY;
    }
    let mut v = [0u64; 3];
    *(v.as_mut_ptr() as *mut u16) = VT_BSTR;
    v[1] = b as u64;
    put(id, std::ptr::read_unaligned(prop), Entry::Value(v));
    S_OK
}

unsafe fn set_server(
    id: Option<Vec<u8>>,
    p: *const Guid,
    n: i32,
    server: usize,
    scope: u32,
) -> i32 {
    let (Some(id), Some(list)) = (id, props(p, n)) else {
        return E_INVALIDARG;
    };
    if server == 0 || scope > 1 {
        return E_INVALIDARG;
    }
    for prop in list {
        com_add_ref(server);
        put(id.clone(), prop, Entry::Server(server));
    }
    S_OK
}

unsafe fn clear_list(id: Option<Vec<u8>>, p: *const Guid, n: i32) -> i32 {
    let (Some(id), Some(list)) = (id, props(p, n)) else {
        return E_INVALIDARG;
    };
    clear(&id, &list);
    S_OK
}

unsafe extern "win64" fn set_prop_value(
    _this: usize,
    id: *const u8,
    len: u32,
    prop: *const Guid,
    var: *const c_void,
) -> i32 {
    set_value(id_bytes(id, len), prop, var)
}

unsafe extern "win64" fn set_prop_server(
    _this: usize,
    id: *const u8,
    len: u32,
    p: *const Guid,
    n: i32,
    server: usize,
    scope: u32,
) -> i32 {
    set_server(id_bytes(id, len), p, n, server, scope)
}

unsafe extern "win64" fn clear_props(
    _this: usize,
    id: *const u8,
    len: u32,
    p: *const Guid,
    n: i32,
) -> i32 {
    clear_list(id_bytes(id, len), p, n)
}

fn hwnd_id(hwnd: usize, obj: u32, child: u32) -> Vec<u8> {
    compose(KIND_HWND, hwnd as u64, obj, child).to_vec()
}

fn hmenu_id(hmenu: usize, child: u32) -> Vec<u8> {
    compose(KIND_HMENU, hmenu as u64, 0, child).to_vec()
}

unsafe extern "win64" fn set_hwnd_prop(
    _this: usize,
    hwnd: usize,
    obj: u32,
    child: u32,
    prop: *const Guid,
    var: *const c_void,
) -> i32 {
    set_value(Some(hwnd_id(hwnd, obj, child)), prop, var)
}

unsafe extern "win64" fn set_hwnd_prop_str(
    _this: usize,
    hwnd: usize,
    obj: u32,
    child: u32,
    prop: *const Guid,
    s: *const u16,
) -> i32 {
    set_str(hwnd_id(hwnd, obj, child), prop, s)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn set_hwnd_prop_server(
    _this: usize,
    hwnd: usize,
    obj: u32,
    child: u32,
    p: *const Guid,
    n: i32,
    server: usize,
    scope: u32,
) -> i32 {
    set_server(Some(hwnd_id(hwnd, obj, child)), p, n, server, scope)
}

unsafe extern "win64" fn clear_hwnd_props(
    _this: usize,
    hwnd: usize,
    obj: u32,
    child: u32,
    p: *const Guid,
    n: i32,
) -> i32 {
    clear_list(Some(hwnd_id(hwnd, obj, child)), p, n)
}

unsafe extern "win64" fn compose_hwnd(
    _this: usize,
    hwnd: usize,
    obj: u32,
    child: u32,
    out: *mut *mut u8,
    len: *mut u32,
) -> i32 {
    give_id(compose(KIND_HWND, hwnd as u64, obj, child), out, len)
}

unsafe extern "win64" fn decompose_hwnd(
    _this: usize,
    id: *const u8,
    len: u32,
    hwnd: *mut usize,
    obj: *mut u32,
    child: *mut u32,
) -> i32 {
    if hwnd.is_null() || obj.is_null() || child.is_null() {
        return E_POINTER;
    }
    let Some((h, o, c)) = id_bytes(id, len).and_then(|b| decompose(&b, KIND_HWND)) else {
        return E_INVALIDARG;
    };
    *hwnd = h as usize;
    *obj = o;
    *child = c;
    S_OK
}

unsafe extern "win64" fn set_hmenu_prop(
    _this: usize,
    hmenu: usize,
    child: u32,
    prop: *const Guid,
    var: *const c_void,
) -> i32 {
    set_value(Some(hmenu_id(hmenu, child)), prop, var)
}

unsafe extern "win64" fn set_hmenu_prop_str(
    _this: usize,
    hmenu: usize,
    child: u32,
    prop: *const Guid,
    s: *const u16,
) -> i32 {
    set_str(hmenu_id(hmenu, child), prop, s)
}

unsafe extern "win64" fn set_hmenu_prop_server(
    _this: usize,
    hmenu: usize,
    child: u32,
    p: *const Guid,
    n: i32,
    server: usize,
    scope: u32,
) -> i32 {
    set_server(Some(hmenu_id(hmenu, child)), p, n, server, scope)
}

unsafe extern "win64" fn clear_hmenu_props(
    _this: usize,
    hmenu: usize,
    child: u32,
    p: *const Guid,
    n: i32,
) -> i32 {
    clear_list(Some(hmenu_id(hmenu, child)), p, n)
}

unsafe extern "win64" fn compose_hmenu(
    _this: usize,
    hmenu: usize,
    child: u32,
    out: *mut *mut u8,
    len: *mut u32,
) -> i32 {
    give_id(compose(KIND_HMENU, hmenu as u64, 0, child), out, len)
}

unsafe extern "win64" fn decompose_hmenu(
    _this: usize,
    id: *const u8,
    len: u32,
    hmenu: *mut usize,
    child: *mut u32,
) -> i32 {
    if hmenu.is_null() || child.is_null() {
        return E_POINTER;
    }
    let Some((h, _, c)) = id_bytes(id, len).and_then(|b| decompose(&b, KIND_HMENU)) else {
        return E_INVALIDARG;
    };
    *hmenu = h as usize;
    *child = c;
    S_OK
}

/// Classe intégrée pour CoCreateInstance.
///
/// # Safety
/// `out` doit pouvoir recevoir un pointeur.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    let iid = &*(iid as *const _ as *const Guid);
    if *clsid != CLSID_ACC_PROP_SERVICES {
        return None;
    }
    Some(qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROP: Guid = guid(
        0x608D_3DF8,
        0x8128,
        0x4AA7,
        [0xA4, 0x28, 0xF5, 0x5E, 0x49, 0x26, 0x72, 0x91],
    );

    #[test]
    fn identites_aller_retour() {
        let id = compose(KIND_HWND, 0x1234_5678_9abc, 0xFFFF_FFFC, 7);
        assert_eq!(
            decompose(&id, KIND_HWND),
            Some((0x1234_5678_9abc, 0xFFFF_FFFC, 7))
        );
        assert_eq!(decompose(&id, KIND_HMENU), None);
        assert_eq!(decompose(&id[..19], KIND_HWND), None);
    }

    #[test]
    fn annotation_posee_puis_effacee() {
        unsafe {
            let name: Vec<u16> = "Valider\0".encode_utf16().collect();
            let id = hwnd_id(0x4242, 0xFFFF_FFFC, 0);
            assert_eq!(
                set_hwnd_prop_str(0, 0x4242, 0xFFFF_FFFC, 0, &PROP, name.as_ptr()),
                S_OK
            );
            assert_eq!(annotation_count(&id), 1);
            let mut v = [0u64; 3];
            assert!(annotation_value(&id, &PROP, v.as_mut_ptr().cast()));
            assert_eq!(*(v.as_ptr() as *const u16), VT_BSTR);
            super::super::oleaut32::variant_clear(v.as_mut_ptr().cast());
            assert_eq!(clear_hwnd_props(0, 0x4242, 0xFFFF_FFFC, 0, &PROP, 1), S_OK);
            assert_eq!(annotation_count(&id), 0);
            assert_eq!(
                set_prop_value(0, std::ptr::null(), 0, &PROP, v.as_ptr().cast()),
                E_INVALIDARG
            );
        }
    }
}
