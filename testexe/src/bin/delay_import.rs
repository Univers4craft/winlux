#![no_std]
#![no_main]

use winlux_testexe::*;

#[link(name = "testdll", kind = "raw-dylib")]
extern "system" {
    fn winlux_add(a: u32, b: u32) -> u32;
    fn winlux_attach_kind() -> u32;
}

#[link(name = "winlux_optional_never_installed", kind = "raw-dylib")]
extern "system" {
    fn winlux_optional_feature() -> u32;
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(
            GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "DLL différée absente avant le premier appel"
        );
        check!(
            delay_load_count() == 0,
            "helper différé non appelé au démarrage"
        );
        check!(winlux_add(19, 23) == 42, "appel via import différé");
        check!(
            !GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "DLL chargée au premier appel"
        );
        check!(delay_load_count() == 1, "helper différé appelé une fois");
        check!(
            winlux_add(20, 22) == 42 && delay_load_count() == 1,
            "IAT mise à jour pour les appels suivants"
        );
        check!(
            winlux_attach_kind() == 2,
            "DllMain reçoit NULL pour un chargement dynamique"
        );
        let mut value = [0u8; 2];
        if GetEnvironmentVariableA(
            c"WINLUX_CALL_MISSING_DELAY".as_ptr(),
            value.as_mut_ptr().cast(),
            2,
        ) != 0
        {
            let _ = winlux_optional_feature();
        }
        check!(
            GetModuleHandleA(c"winlux_optional_never_installed.dll".as_ptr()).is_null(),
            "DLL optionnelle inutilisée non requise"
        );
    }
    finish()
}
