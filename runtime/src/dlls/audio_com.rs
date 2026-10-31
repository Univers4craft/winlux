//! Briques COM partagées par les DLL audio (DirectSound, WASAPI, XAudio2) :
//! tables virtuelles statiques, objets à compteur de références et
//! interfaces secondaires (« tear-off ») qui délèguent à leur objet.

use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

pub const S_OK: i32 = 0;
pub const S_FALSE: i32 = 1;
pub const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
pub const E_POINTER: i32 = 0x8000_4003u32 as i32;
pub const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
pub const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
pub const CLASS_E_NOAGGREGATION: i32 = 0x8004_0110u32 as i32;

/// Table virtuelle statique : pointeurs de fonctions `extern "win64"`.
#[repr(transparent)]
pub struct VTable<const N: usize>(pub [*const (); N]);
unsafe impl<const N: usize> Sync for VTable<N> {}

impl<const N: usize> VTable<N> {
    pub fn ptr(&'static self) -> *const () {
        self.0.as_ptr() as *const ()
    }
}

/// GUID écrit comme dans les en-têtes Windows.
pub const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    }
}

pub const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_ICLASSFACTORY: Guid = guid(1, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);

/// Objet COM : la table virtuelle en tête, puis le compteur et les données.
#[repr(C)]
pub struct Com<T> {
    vtbl: *const (),
    refs: AtomicU32,
    pub inner: T,
}

impl<T> Com<T> {
    pub fn create(vtbl: *const (), inner: T) -> *mut Com<T> {
        Box::into_raw(Box::new(Com {
            vtbl,
            refs: AtomicU32::new(1),
            inner,
        }))
    }

    /// # Safety
    /// `this` doit venir de [`Com::create`] et être encore vivant.
    pub unsafe fn add_ref(this: *mut Com<T>) -> u32 {
        (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// # Safety
    /// `this` doit venir de [`Com::create`] ; il est libéré à zéro.
    pub unsafe fn release(this: *mut Com<T>) -> u32 {
        let left = (*this).refs.fetch_sub(1, Ordering::AcqRel).wrapping_sub(1);
        if left == 0 {
            drop(Box::from_raw(this));
        }
        left
    }

    /// # Safety
    /// `this` doit être un objet vivant.
    pub unsafe fn get<'a>(this: *mut Com<T>) -> &'a T {
        &(*this).inner
    }
}

/// Interface secondaire d'un objet : délègue la durée de vie à `owner`.
#[repr(C)]
pub struct Iface {
    vtbl: *const (),
    pub owner: *mut c_void,
}

impl Iface {
    pub fn new(vtbl: *const ()) -> Iface {
        Iface {
            vtbl,
            owner: std::ptr::null_mut(),
        }
    }
}

/// Écrit `obj` dans `out` (si non nul) et renvoie S_OK.
///
/// # Safety
/// `out` doit être nul ou inscriptible.
pub unsafe fn put<T>(out: *mut *mut c_void, obj: *mut T) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = obj as *mut c_void;
    S_OK
}

/// Lit le GUID pointé (ou `None`).
///
/// # Safety
/// `p` doit être nul ou lisible.
pub unsafe fn read_guid(p: *const Guid) -> Option<Guid> {
    if p.is_null() {
        None
    } else {
        Some(std::ptr::read_unaligned(p))
    }
}

/// Copie un WAVEFORMATEX(TENSIBLE) dans un bloc CoTaskMemAlloc.
pub fn alloc_format(f: &crate::audio::format::Format) -> *mut u8 {
    let bytes = f.to_bytes();
    unsafe {
        let p = super::ole32::co_task_mem_alloc(bytes.len()) as *mut u8;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        }
        p
    }
}

/// Appelle la méthode `index` d'une interface du programme (rappel) avec
/// `this` seul.
///
/// # Safety
/// `obj` doit être une interface valide dont la méthode `index` a la
/// signature `fn(this)`.
pub unsafe fn call0(obj: *mut c_void, index: usize) {
    let vt = *(obj as *const *const usize);
    let f: unsafe extern "win64" fn(*mut c_void) = std::mem::transmute(*vt.add(index));
    f(obj)
}

/// Appelle la méthode `index` avec un argument entier/pointeur.
///
/// # Safety
/// Voir [`call0`] ; signature `fn(this, usize)`.
pub unsafe fn call1(obj: *mut c_void, index: usize, a: usize) {
    let vt = *(obj as *const *const usize);
    let f: unsafe extern "win64" fn(*mut c_void, usize) = std::mem::transmute(*vt.add(index));
    f(obj, a)
}

/// AddRef / Release d'une interface du programme.
///
/// # Safety
/// `obj` doit être nul ou un IUnknown valide.
pub unsafe fn unknown_add_ref(obj: *mut c_void) {
    if !obj.is_null() {
        let vt = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(1));
        f(obj);
    }
}

/// # Safety
/// `obj` doit être nul ou un IUnknown valide.
pub unsafe fn unknown_release(obj: *mut c_void) {
    if !obj.is_null() {
        let vt = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(2));
        f(obj);
    }
}

/// Signale un événement Win32 (s'il existe).
pub fn signal(event: usize) {
    if event != 0 {
        super::kernel32::set_event(event);
    }
}

/// Fabrique de classes générique pour les objets intégrés (CoGetClassObject /
/// DllGetClassObject) : `create(riid, out)`.
pub type Creator = unsafe fn(*const Guid, *mut *mut c_void) -> i32;

struct Factory {
    create: Creator,
}

unsafe extern "win64" fn cf_qi(
    this: *mut Com<Factory>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_ICLASSFACTORY => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn cf_add_ref(this: *mut Com<Factory>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn cf_release(this: *mut Com<Factory>) -> u32 {
    Com::release(this)
}
unsafe extern "win64" fn cf_create(
    this: *mut Com<Factory>,
    outer: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if !outer.is_null() {
        return CLASS_E_NOAGGREGATION;
    }
    (Com::get(this).create)(riid, out)
}
unsafe extern "win64" fn cf_lock(_this: *mut Com<Factory>, _lock: i32) -> i32 {
    S_OK
}
static CF_VT: VTable<5> = VTable([
    cf_qi as *const (),
    cf_add_ref as *const (),
    cf_release as *const (),
    cf_create as *const (),
    cf_lock as *const (),
]);

/// Classes audio intégrées, créées sans enregistrement préalable.
fn builtin_creator(clsid: &Guid) -> Option<Creator> {
    super::dsound::creator(clsid)
        .or_else(|| super::mmdevapi::creator(clsid))
        .or_else(|| super::xaudio2::creator(clsid))
}

/// Crée une instance d'une classe audio intégrée (appelé par CoCreateInstance
/// quand aucune fabrique n'a été enregistrée par le programme).
///
/// # Safety
/// `riid` et `out` doivent être valides.
pub unsafe fn create_builtin(
    clsid: &Guid,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    let c = builtin_creator(clsid)?;
    Some(c(riid, out))
}

/// Renvoie une fabrique de classes pour une classe audio intégrée.
///
/// # Safety
/// `riid` et `out` doivent être valides.
pub unsafe fn class_object(clsid: &Guid, riid: *const Guid, out: *mut *mut c_void) -> Option<i32> {
    let create = builtin_creator(clsid)?;
    let f = Com::create(CF_VT.ptr(), Factory { create });
    let hr = cf_qi(f, riid, out);
    Com::release(f);
    Some(hr)
}

/// DllGetClassObject commun aux DLL audio intégrées.
pub unsafe extern "win64" fn dll_get_class_object(
    clsid: *const Guid,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(c) = read_guid(clsid) else {
        return E_INVALIDARG;
    };
    class_object(&c, riid, out).unwrap_or(0x8004_0111u32 as i32) // CLASS_E_CLASSNOTAVAILABLE
}

pub extern "win64" fn dll_can_unload_now() -> i32 {
    S_FALSE
}
