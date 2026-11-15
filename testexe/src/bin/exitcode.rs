//! Vérifie que le code de sortie Windows est transmis au shell Linux.
#![no_std]
#![no_main]

use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    // « exitcode.exe 3010 » (lancé par p1_suite, #2267) : un code de plus de
    // 8 bits, que le parent doit lire entier avec GetExitCodeProcess.
    unsafe {
        if !strstr(GetCommandLineA(), c" 3010".as_ptr()).is_null() {
            exit(3010)
        }
        exit(42)
    }
}
