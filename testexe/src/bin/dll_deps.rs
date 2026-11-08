//! #691 : une DLL amenée seulement comme dépendance d'un LoadLibrary reçoit
//! son DllMain, avant la DLL qui l'importe. Audacity plantait au premier
//! lancement parce que les objets globaux de lib-cloud-audiocom.dll, chargée
//! par mod-cloud-audiocom.dll, n'étaient jamais construits.

#![no_std]
#![no_main]

use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(
            GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "testdll.dll pas encore chargée"
        );
        let dll = LoadLibraryA(c"testdll2.dll".as_ptr());
        check!(!dll.is_null(), "LoadLibrary d'une DLL qui en importe une autre");
        let f = if dll.is_null() {
            core::ptr::null()
        } else {
            GetProcAddress(dll, c"winlux_dep_attach_kind".as_ptr())
        };
        check!(!f.is_null(), "GetProcAddress(winlux_dep_attach_kind)");
        if !f.is_null() {
            let kind: extern "system" fn() -> u32 = core::mem::transmute(f);
            check!(
                kind() == 2,
                "DllMain de la dépendance appelé avant celui de la DLL chargée"
            );
        }
        check!(
            !GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "la dépendance est dans la liste des modules"
        );
    }
    finish()
}
