//! Options globales du runtime COM : CLSID_GlobalOptions et IGlobalOptions
//! (Set, Query), #2208.
//!
//! Teams crée cet objet au démarrage pour régler la gestion des exceptions
//! de COM ; sans lui, CoCreateInstance rendait REGDB_E_CLASSNOTREG et Teams
//! s'arrêtait sur la wil::ResultException qui en découle.
//!
//! Sources : documentation Microsoft Learn (IGlobalOptions, Set, Query,
//! GLOBALOPT_PROPERTIES : « S_OK if the property was set successfully »),
//! et pour les identifiants, l'ordre des méthodes et les valeurs des
//! énumérations, objidlbase.idl et uuid.c de MinGW-w64 (domaine public).
//!
//! Les valeurs sont retenues pour tout le processus et rendues telles quelles
//! par Query ; chaque propriété vaut 0 tant qu'elle n'a pas été réglée, ce qui
//! correspond aux défauts documentés (COMGLB_EXCEPTION_HANDLE,
//! COMGLB_RPC_THREADPOOL_SETTING_DEFAULT_POOL,
//! COMGLB_UNMARSHALING_POLICY_NORMAL hors conteneur d'application). Elles ne
//! changent rien au comportement de notre COM : il n'intercepte jamais les
//! exceptions des méthodes appelées et n'a pas de canal RPC. COMGLB_APPID
//! reçoit un pointeur vers un GUID, dont Winlux garde une copie. Une
//! propriété hors de 1..=5 rend E_INVALIDARG : la documentation ne dit rien
//! de ce cas, c'est notre choix.

use super::comobj::{guid, Guid, VTable};
use std::ffi::c_void;
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const CLSID_GLOBAL_OPTIONS: Guid = guid(0x0000_034B, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IGLOBAL_OPTIONS: Guid = guid(0x0000_015B, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// GLOBALOPT_PROPERTIES.
const COMGLB_APPID: u32 = 2;
const LAST_PROPERTY: u32 = 5; // COMGLB_UNMARSHALING_POLICY

/// Valeurs des propriétés 1..=5 (index 0 inutilisé).
static VALUES: Mutex<[usize; 6]> = Mutex::new([0; 6]);
/// Copie du GUID d'AppID passé à Set(COMGLB_APPID).
static APPID: Mutex<Option<&'static [u8; 16]>> = Mutex::new(None);

/// Objet sans état propre : un seul exemplaire, compteur fixe.
#[repr(C)]
struct Obj {
    vtbl: *const (),
}
unsafe impl Sync for Obj {}

static OPTIONS: Obj = Obj {
    vtbl: &VT as *const _ as *const (),
};

static VT: VTable<5> = VTable([
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    set as *const (),
    query as *const (),
]);

unsafe extern "win64" fn qi(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if iid.is_null() {
        *out = std::ptr::null_mut();
        return E_INVALIDARG;
    }
    if *iid == IID_IGLOBAL_OPTIONS || *iid == IID_IUNKNOWN {
        *out = std::ptr::addr_of!(OPTIONS) as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
extern "win64" fn add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn release(_this: usize) -> u32 {
    1
}

/// IGlobalOptions::Set(dwProperty, dwValue).
unsafe extern "win64" fn set(_this: usize, property: u32, value: usize) -> i32 {
    if property == 0 || property > LAST_PROPERTY {
        return E_INVALIDARG;
    }
    if property == COMGLB_APPID {
        if value == 0 {
            return E_INVALIDARG;
        }
        let copy: &'static [u8; 16] = Box::leak(Box::new(*(value as *const [u8; 16])));
        *APPID.lock().unwrap_or_else(|e| e.into_inner()) = Some(copy);
    }
    VALUES.lock().unwrap_or_else(|e| e.into_inner())[property as usize] = value;
    S_OK
}

/// IGlobalOptions::Query(dwProperty, pdwValue).
unsafe extern "win64" fn query(_this: usize, property: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if property == 0 || property > LAST_PROPERTY {
        *out = 0;
        return E_INVALIDARG;
    }
    *out = if property == COMGLB_APPID {
        APPID
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map_or(0, |g| g.as_ptr() as usize)
    } else {
        VALUES.lock().unwrap_or_else(|e| e.into_inner())[property as usize]
    };
    S_OK
}

/// CoCreateInstance(CLSID_GlobalOptions) ; `None` pour une autre classe.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    if *clsid != CLSID_GLOBAL_OPTIONS {
        return None;
    }
    Some(qi(0, iid as *const _ as *const Guid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(x: &Guid) -> &super::super::ole32::Guid {
        unsafe { &*(x as *const Guid as *const super::super::ole32::Guid) }
    }

    /// Teams (#2208) : création, Set puis Query, propriété inconnue.
    #[test]
    fn options_globales_2208() {
        unsafe {
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(g(&CLSID_GLOBAL_OPTIONS), g(&IID_IGLOBAL_OPTIONS), &mut p),
                Some(S_OK)
            );
            assert!(!p.is_null());
            let iid_other = guid(1, 2, 3, [4; 8]);
            assert!(create_builtin(g(&iid_other), g(&IID_IGLOBAL_OPTIONS), &mut p).is_none());
            let mut q: *mut c_void = std::ptr::null_mut();
            assert_eq!(qi(0, &iid_other, &mut q), E_NOINTERFACE);
            assert!(q.is_null());

            // COMGLB_EXCEPTION_HANDLING = 1, COMGLB_EXCEPTION_DONOT_HANDLE_ANY = 2.
            let mut v = 99usize;
            assert_eq!(set(0, 1, 2), S_OK);
            assert_eq!(query(0, 1, &mut v), S_OK);
            assert_eq!(v, 2);
            // COMGLB_APPID : pointeur vers une copie du GUID.
            let appid = [7u8; 16];
            assert_eq!(set(0, 2, appid.as_ptr() as usize), S_OK);
            assert_eq!(query(0, 2, &mut v), S_OK);
            assert_eq!(*(v as *const [u8; 16]), appid);
            assert_eq!(set(0, 2, 0), E_INVALIDARG);
            // Propriétés hors de l'énumération.
            assert_eq!(set(0, 0, 1), E_INVALIDARG);
            assert_eq!(set(0, 6, 1), E_INVALIDARG);
            assert_eq!(query(0, 9, &mut v), E_INVALIDARG);
            assert_eq!(query(0, 1, std::ptr::null_mut()), E_POINTER);
        }
    }
}
