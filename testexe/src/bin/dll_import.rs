#![no_std]
#![no_main]

use winlux_testexe::*;

#[link(name = "testdll", kind = "raw-dylib")]
extern "system" {
    fn winlux_add(a: u32, b: u32) -> u32;
    fn winlux_attach_kind() -> u32;
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(winlux_add(20, 22) == 42, "appel d'une DLL réelle");
        check!(
            winlux_attach_kind() == 1,
            "DllMain distingue le chargement statique"
        );
    }
    finish()
}
