//! Références faibles des objets Windows Runtime de Winlux (#2207) :
//! IWeakReferenceSource (GetWeakReference) et IWeakReference (Resolve).
//! C++/WinRT en prend une sur la source de chaque événement abonné avec
//! `auto_revoke`, et l'appelle sans vérifier le QueryInterface.
//!
//! Sources : documentation publique de Microsoft (weakreference.h :
//! IWeakReferenceSource {00000038-0000-0000-C000-000000000046},
//! GetWeakReference ; IWeakReference {00000037-0000-0000-C000-000000000046},
//! Resolve(riid, objectReference) qui rend S_OK et un pointeur nul quand
//! l'objet n'existe plus).
//!
//! L'objet suivi garde un `Arc<WeakControl>`. Resolve et la dernière
//! libération passent par le même verrou : Resolve ne ressuscite jamais un
//! objet dont le compteur est tombé à zéro, et la libération ne détruit pas
//! un objet que Resolve vient de reprendre.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const IID_IWEAK_REFERENCE_SOURCE: Guid =
    guid(0x0000_0038, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IWEAK_REFERENCE: Guid = guid(0x0000_0037, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

type QueryInterface = unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32;

/// Cible d'une référence faible : l'objet, son QueryInterface et son
/// compteur, ou rien une fois l'objet détruit.
#[derive(Default)]
pub struct WeakControl(Mutex<Option<Target>>);

struct Target {
    object: usize,
    qi: QueryInterface,
    refs: *const AtomicU32,
}
unsafe impl Send for Target {}

impl WeakControl {
    pub fn new(object: usize, qi: QueryInterface, refs: *const AtomicU32) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(Target { object, qi, refs }))))
    }

    /// À appeler quand le compteur de l'objet vient de tomber à zéro :
    /// `true` si l'objet peut être détruit (la cible est alors effacée),
    /// `false` si une résolution l'a repris entre-temps.
    pub fn release_last(&self, refs: &AtomicU32) -> bool {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if refs.load(Ordering::SeqCst) != 0 {
            return false;
        }
        *t = None;
        true
    }

    unsafe fn resolve(&self, iid: *const u8, out: *mut usize) -> i32 {
        let t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *out = 0;
        match t.as_ref() {
            Some(t) if (*t.refs).load(Ordering::SeqCst) > 0 => (t.qi)(t.object, iid, out),
            // Objet détruit : S_OK et un pointeur nul.
            _ => S_OK,
        }
    }
}

#[repr(C)]
struct WeakRef {
    vtbl: &'static VTable<4>,
    refs: AtomicU32,
    control: Arc<WeakControl>,
}

/// GetWeakReference : un nouvel IWeakReference sur `control`.
pub unsafe fn get_weak_reference(control: &Arc<WeakControl>, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = Box::into_raw(Box::new(WeakRef {
        vtbl: &WEAK_VTBL,
        refs: AtomicU32::new(1),
        control: control.clone(),
    })) as usize;
    S_OK
}

unsafe fn weak_of(this: usize) -> &'static WeakRef {
    &*(this as *const WeakRef)
}

unsafe extern "win64" fn w_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if iid == IID_IWEAK_REFERENCE || iid == IID_IUNKNOWN {
        weak_of(this).refs.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn w_add_ref(this: usize) -> u32 {
    weak_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn w_release(this: usize) -> u32 {
    let n = weak_of(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut WeakRef));
    }
    n
}
unsafe extern "win64" fn w_resolve(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    weak_of(this).control.resolve(iid, out)
}

static WEAK_VTBL: VTable<4> = VTable([
    w_qi as *const (),
    w_add_ref as *const (),
    w_release as *const (),
    w_resolve as *const (),
]);
