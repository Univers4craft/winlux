//! Le « hello.exe » du palier P1 : console, printf, code de sortie.
#![no_std]
#![no_main]

use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Bonjour depuis un .exe Windows, execute par Winlux !\n");
    unsafe {
        printf(c"printf : %d + %d = %d, pi ~ %.4f, hex %#x, chaine \"%s\"\n".as_ptr(),
               2, 40, 42, 3.14159265f64, 255u32, c"ok".as_ptr());
        ExitProcess(0)
    }
}
