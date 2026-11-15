//! taskschd.dll intégrée (#2577) : le serveur en processus du planificateur
//! de tâches 2.0.
//!
//! Sous Windows, CoCreateInstance(CLSID_TaskScheduler) charge taskschd.dll
//! dans le processus (documentation Microsoft Learn, « Configuration
//! requise » des interfaces du planificateur : DLL taskschd.dll). Google
//! Update l'épingle ensuite par GetModuleHandleExW(PIN, « taskschd.dll ») ;
//! sans module de ce nom, l'appel rendait ERROR_MOD_NOT_FOUND. Les objets
//! sont ceux de `taskschd.rs` ; cette DLL n'ajoute que ses deux points
//! d'entrée COM, DllGetClassObject (fabrique commune de `audio_com.rs`) et
//! DllCanUnloadNow.

use super::ole32::Guid;
use std::ffi::c_void;

const E_POINTER: i32 = 0x8000_4003u32 as i32;

/// Crée l'objet ITaskService demandé par la fabrique de DllGetClassObject.
///
/// # Safety
/// `riid` et `out` doivent être valides.
pub unsafe fn create_service(riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if riid.is_null() || out.is_null() {
        return E_POINTER;
    }
    super::taskschd::create_builtin(&super::taskschd::CLSID_TASK_SCHEDULER, &*riid, out)
        .unwrap_or(E_POINTER)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DllGetClassObject"=>fn super::audio_com::dll_get_class_object,"DllCanUnloadNow"=>fn super::audio_com::dll_can_unload_now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IID_ICLASSFACTORY: Guid = Guid {
        data1: 1,
        data2: 0,
        data3: 0,
        data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
    };
    /// ITaskService {2FABA4C7-4DA9-4013-9697-20CC3FD40F85} (taskschd.idl).
    const IID_ITASKSERVICE: Guid = Guid {
        data1: 0x2FAB_A4C7,
        data2: 0x4DA9,
        data3: 0x4013,
        data4: [0x96, 0x97, 0x20, 0xCC, 0x3F, 0xD4, 0x0F, 0x85],
    };

    /// #2577 : GetModuleHandleExW(PIN, « taskschd.dll ») trouve le module,
    /// et son DllGetClassObject rend la fabrique d'ITaskService.
    #[test]
    fn taskschd_dll_epinglable_et_fabrique() {
        unsafe {
            let name: Vec<u16> = "taskschd.dll".encode_utf16().chain([0]).collect();
            let mut h = 0usize;
            // GET_MODULE_HANDLE_EX_FLAG_PIN = 1.
            assert_eq!(
                super::super::kernel32::get_module_handle_ex_w(1, name.as_ptr(), &mut h),
                1
            );
            assert_ne!(h, 0);
            let entry = crate::dlls::lookup("taskschd", "DllGetClassObject").expect("export");
            let get: unsafe extern "win64" fn(*const Guid, *const Guid, *mut *mut c_void) -> i32 =
                std::mem::transmute(entry);
            let mut cf: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                get(
                    &super::super::taskschd::CLSID_TASK_SCHEDULER,
                    &IID_ICLASSFACTORY,
                    &mut cf
                ),
                0
            );
            assert!(!cf.is_null());
            let vt = *(cf as *const *const usize);
            let create: unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *const Guid,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(*vt.add(3));
            let mut svc: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create(cf, std::ptr::null_mut(), &IID_ITASKSERVICE, &mut svc),
                0
            );
            assert!(!svc.is_null());
            for p in [svc, cf] {
                let rel: unsafe extern "win64" fn(*mut c_void) -> u32 =
                    std::mem::transmute(*(*(p as *const *const usize)).add(2));
                rel(p);
            }
        }
    }
}
