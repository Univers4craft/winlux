//! Vérifie que le code de sortie Windows est transmis au shell Linux.
#![no_std]
#![no_main]

use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe { exit(42) }
}
