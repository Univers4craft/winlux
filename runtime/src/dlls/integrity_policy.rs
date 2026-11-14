//! Classe Windows Runtime `Windows.System.Profile.WindowsIntegrityPolicy`
//! (#2217) : état du mode « 10 S » (Windows Lockdown). Teams lève
//! `winrt::hresult_class_not_registered` sans elle, et le `catch` qui la
//! rattrape arrête le programme avant sa fenêtre.
//!
//! Winlux n'a pas de mode S : les quatre propriétés valent `false` et
//! l'événement PolicyChanged n'est jamais déclenché (choix de Winlux, la
//! politique ne change pas pendant la session).
//!
//! Sources : documentation publique de Microsoft (WindowsIntegrityPolicy :
//! classe statique, propriétés booléennes CanDisable, IsDisableSupported,
//! IsEnabled, IsEnabledForTrial, événement PolicyChanged ; Windows 10 1809,
//! UniversalApiContract v7) ; identifiant d'interface et ordre des méthodes
//! de IWindowsIntegrityPolicyStatics tels que les publie windows-rs
//! (licence MIT).

use std::sync::atomic::{AtomicI64, Ordering};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
/// IWindowsIntegrityPolicyStatics.
const IID_STATICS: Guid = g(0x7D1D_81DB, 0x8D63, 0x4789, 0x9EA5_DDCF_65A9_4F3C);

const CLASS: &str = "Windows.System.Profile.WindowsIntegrityPolicy";

/// Jetons rendus par PolicyChanged.
static NEXT_TOKEN: AtomicI64 = AtomicI64::new(1);

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<12>,
}

static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};

unsafe extern "win64" fn st_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_STATICS,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = std::ptr::addr_of!(STATICS) as usize;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
/// Fabrique statique : jamais libérée.
extern "win64" fn st_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn st_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn st_class_name(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}

/// IsEnabled, IsEnabledForTrial, CanDisable, IsDisableSupported : pas de
/// mode S sous Winlux.
unsafe extern "win64" fn st_false(_this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn st_policy_changed(_this: usize, handler: usize, token: *mut i64) -> i32 {
    if handler == 0 || token.is_null() {
        return E_POINTER;
    }
    // La politique ne change jamais : le gestionnaire n'est pas gardé.
    *token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    S_OK
}
extern "win64" fn st_remove_policy_changed(_this: usize, _token: i64) -> i32 {
    S_OK
}

static STATICS_VTBL: VTable<12> = VTable([
    st_qi as *const (),
    st_add_ref as *const (),
    st_release as *const (),
    get_iids as *const (),
    st_class_name as *const (),
    trust_level as *const (),
    st_false as *const (), // IsEnabled
    st_false as *const (), // IsEnabledForTrial
    st_false as *const (), // CanDisable
    st_false as *const (), // IsDisableSupported
    st_policy_changed as *const (),
    st_remove_policy_changed as *const (),
]);

/// `RoGetActivationFactory` pour WindowsIntegrityPolicy, `None` pour une
/// autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(st_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn slot(obj: usize, n: usize) -> *const () {
        *(*(obj as *const *const *const ())).add(n)
    }

    /// Teams (#2217) : la fabrique répond, pas de mode S, l'abonnement à
    /// PolicyChanged rend un jeton.
    #[test]
    fn politique_d_integrite_2217() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert_ne!(f, 0);
            let mut other = 0usize;
            assert!(
                activation_factory("Windows.Other", IID_STATICS.as_ptr(), &mut other).is_none()
            );
            let mut none = 1usize;
            let bad = g(1, 2, 3, 4);
            assert_eq!(
                activation_factory(CLASS, bad.as_ptr(), &mut none),
                Some(E_NOINTERFACE)
            );
            assert_eq!(none, 0);
            for n in 6..=9 {
                let get: unsafe extern "win64" fn(usize, *mut u8) -> i32 =
                    std::mem::transmute(slot(f, n));
                let mut v = 7u8;
                assert_eq!(get(f, &mut v), S_OK, "emplacement {n}");
                assert_eq!(v, 0, "emplacement {n}");
                assert_eq!(get(f, std::ptr::null_mut()), E_POINTER);
            }
            let add: unsafe extern "win64" fn(usize, usize, *mut i64) -> i32 =
                std::mem::transmute(slot(f, 10));
            let mut t1 = 0i64;
            let mut t2 = 0i64;
            assert_eq!(add(f, 0x1000, &mut t1), S_OK);
            assert_eq!(add(f, 0x1000, &mut t2), S_OK);
            assert_ne!(t1, 0);
            assert_ne!(t1, t2);
            assert_eq!(add(f, 0, &mut t1), E_POINTER);
            let remove: extern "win64" fn(usize, i64) -> i32 = std::mem::transmute(slot(f, 11));
            assert_eq!(remove(f, t1), S_OK);
            let mut h = 0usize;
            assert_eq!(st_class_name(f, &mut h), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h)),
                CLASS
            );
        }
    }
}
