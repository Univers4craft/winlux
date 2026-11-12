//! IEnumVARIANT du runtime : énumération d'objets IDispatch, rendue par les
//! propriétés `_NewEnum` des collections (pare-feu, #2036).
//!
//! L'énumérateur tient une référence sur chaque élément ; Next en rend une
//! nouvelle dans un VARIANT de type VT_DISPATCH, que l'appelant libère par
//! VariantClear. Méthodes, ordre et codes : oaidl.h de MinGW-w64 (domaine
//! public) et documentation Microsoft Learn d'IEnumVARIANT.

use super::comobj::{guid, log_no_interface, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const VT_DISPATCH: u16 = 9;
/// Taille d'un VARIANT x64.
const VARIANT_SIZE: usize = 24;

pub const IID_IENUM_VARIANT: Guid = guid(0x0002_0404, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

#[repr(C)]
struct EnumVariant {
    vtbl: *const (),
    refs: AtomicU32,
    items: Vec<usize>,
    pos: Mutex<usize>,
}

static ENUM_VT: VTable<7> = VTable([
    enum_qi as *const (),
    enum_add_ref as *const (),
    enum_release as *const (),
    enum_next as *const (),
    enum_skip as *const (),
    enum_reset as *const (),
    enum_clone as *const (),
]);

unsafe fn call_ref(obj: usize, slot: usize) -> u32 {
    let vt = *(obj as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(slot));
    f(obj)
}

/// Énumérateur sur `items`, des IDispatch dont il prend la référence.
pub fn new_enum(items: Vec<usize>) -> *mut c_void {
    new_at(items, 0) as *mut c_void
}

fn new_at(items: Vec<usize>, pos: usize) -> *mut EnumVariant {
    Box::into_raw(Box::new(EnumVariant {
        vtbl: ENUM_VT.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        items,
        pos: Mutex::new(pos),
    }))
}

unsafe fn pos(this: *mut EnumVariant) -> std::sync::MutexGuard<'static, usize> {
    (*this).pos.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "win64" fn enum_qi(
    this: *mut EnumVariant,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let want = *iid;
    if want == IID_IUNKNOWN || want == IID_IENUM_VARIANT {
        enum_add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    log_no_interface("IEnumVARIANT", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn enum_add_ref(this: *mut EnumVariant) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn enum_release(this: *mut EnumVariant) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let e = Box::from_raw(this);
        for &item in &e.items {
            call_ref(item, 2);
        }
    }
    n
}

unsafe extern "win64" fn enum_next(
    this: *mut EnumVariant,
    celt: u32,
    out: *mut u8,
    fetched: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // Un programme x86 passe par un mandataire qui ne convertit qu'un
    // VARIANT à la fois (wow32/com_netfw.rs) : on en rend un au plus,
    // S_FALSE dit alors « moins que demandé ».
    let want = if crate::wow32::active() {
        celt.min(1)
    } else {
        celt
    };
    let items = &(*this).items;
    let mut p = pos(this);
    let mut n = 0u32;
    while n < want && *p < items.len() {
        let item = items[*p];
        call_ref(item, 1);
        let v = out.add(n as usize * VARIANT_SIZE);
        std::ptr::write_bytes(v, 0, VARIANT_SIZE);
        *(v as *mut u16) = VT_DISPATCH;
        *(v.add(8) as *mut usize) = item;
        *p += 1;
        n += 1;
    }
    if !fetched.is_null() {
        *fetched = n;
    }
    if n == celt {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn enum_skip(this: *mut EnumVariant, celt: u32) -> i32 {
    let len = (*this).items.len();
    let mut p = pos(this);
    let target = p.saturating_add(celt as usize);
    *p = target.min(len);
    if target <= len {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn enum_reset(this: *mut EnumVariant) -> i32 {
    *pos(this) = 0;
    S_OK
}

unsafe extern "win64" fn enum_clone(this: *mut EnumVariant, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items = (*this).items.clone();
    for &item in &items {
        call_ref(item, 1);
    }
    *out = new_at(items, *pos(this)) as *mut c_void;
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Élément de test : un objet COM minimal qui compte ses références.
    #[repr(C)]
    struct Item {
        vtbl: *const (),
        refs: AtomicU32,
    }
    unsafe extern "win64" fn item_add_ref(this: *mut Item) -> u32 {
        (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
    }
    unsafe extern "win64" fn item_release(this: *mut Item) -> u32 {
        (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1
    }
    static ITEM_VT: VTable<3> = VTable([
        std::ptr::null(),
        item_add_ref as *const (),
        item_release as *const (),
    ]);

    unsafe fn method<F>(obj: *mut c_void, index: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(index))
    }

    #[test]
    fn enumeration_rend_chaque_element_puis_s_false() {
        let a = Box::into_raw(Box::new(Item {
            vtbl: ITEM_VT.0.as_ptr() as *const (),
            refs: AtomicU32::new(1),
        }));
        let b = Box::into_raw(Box::new(Item {
            vtbl: ITEM_VT.0.as_ptr() as *const (),
            refs: AtomicU32::new(1),
        }));
        let e = new_enum(vec![a as usize, b as usize]);
        unsafe {
            let next: unsafe extern "win64" fn(*mut c_void, u32, *mut u8, *mut u32) -> i32 =
                method(e, 3);
            let reset: unsafe extern "win64" fn(*mut c_void) -> i32 = method(e, 5);
            let release: unsafe extern "win64" fn(*mut c_void) -> u32 = method(e, 2);
            let mut v = [0u8; VARIANT_SIZE * 3];
            let mut got = 0u32;
            assert_eq!(next(e, 1, v.as_mut_ptr(), &mut got), S_OK);
            assert_eq!(got, 1);
            assert_eq!(*(v.as_ptr() as *const u16), VT_DISPATCH);
            assert_eq!(*(v.as_ptr().add(8) as *const usize), a as usize);
            assert_eq!((*a).refs.load(Ordering::Acquire), 2, "référence donnée");
            if !crate::wow32::active() {
                reset(e);
                assert_eq!(next(e, 3, v.as_mut_ptr(), &mut got), S_FALSE);
                assert_eq!(got, 2);
                assert_eq!(
                    *(v.as_ptr().add(VARIANT_SIZE + 8) as *const usize),
                    b as usize
                );
                assert_eq!(next(e, 1, v.as_mut_ptr(), &mut got), S_FALSE);
                assert_eq!(got, 0);
            }
            release(e);
            // L'énumérateur rend ses références ; restent celles données
            // par Next.
            let (ra, rb) = if crate::wow32::active() {
                (1, 0)
            } else {
                (2, 1)
            };
            assert_eq!((*a).refs.load(Ordering::Acquire), ra);
            assert_eq!((*b).refs.load(Ordering::Acquire), rb);
            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
        }
    }
}
