#![no_std]
#![no_main]

use winlux_testexe::*;

#[link(name = "testdll", kind = "raw-dylib")]
extern "system" {
    fn winlux_add(a: u32, b: u32) -> u32;
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(winlux_add(19, 23) == 42, "appel via import différé");
    }
    finish()
}
