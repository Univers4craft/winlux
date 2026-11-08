//! #996 : deux conditions du processus GPU de Chromium (Claude, Electron).
//! - `WindowFromDC(GetDC(NULL))` rend le bureau : ANGLE refuse sinon
//!   l'affichage EGL et le processus GPU échoue en boucle.
//! - Un enfant Windows lancé avec un bloc d'environnement explicite garde
//!   les variables de l'hôte (affichage, configuration de Winlux) : Chromium
//!   en donne un sans DISPLAY à son processus GPU.
//! - #767 : un enfant qui exécute le même .exe le charge à la même adresse,
//!   comme sous Windows ; le bac à sable de Chromium tue sinon l'enfant.

#![no_std]
#![no_main]

use core::ffi::c_char;
use winlux_testexe::*;

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    fn WindowFromDC(hdc: Handle) -> Handle;
    fn GetDesktopWindow() -> Handle;
}
#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn GetModuleFileNameA(module: Handle, buf: *mut c_char, size: u32) -> u32;
}

unsafe fn env_is(name: &core::ffi::CStr, expected: &[u8]) -> bool {
    let mut buf = [0 as c_char; 64];
    let n = GetEnvironmentVariableA(name.as_ptr(), buf.as_mut_ptr(), 64) as usize;
    n == expected.len() && cstr_eq(buf.as_ptr(), expected)
}

/// Adresse en hexadécimal, terminée par un zéro.
fn hex(v: usize, out: &mut [u8; 20]) {
    let digits = b"0123456789abcdef";
    let mut n = 0;
    let mut started = false;
    for i in (0..16).rev() {
        let d = (v >> (i * 4)) & 0xf;
        if d != 0 || started || i == 0 {
            started = true;
            out[n] = digits[d];
            n += 1;
        }
    }
    out[n] = 0;
}

unsafe fn is_child() -> bool {
    let line = GetCommandLineA() as *const u8;
    let mut i = 0;
    while *line.add(i) != 0 {
        if *line.add(i) == b'@' {
            return true;
        }
        i += 1;
    }
    false
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        if is_child() {
            // Enfant : la valeur du bloc et celle de l'hôte sont là.
            let ok = env_is(c"A", b"1") && env_is(c"WINLUX_TEST_PLOMBERIE", b"oui");
            let mut base = [0u8; 20];
            hex(GetModuleHandleA(core::ptr::null()) as usize, &mut base);
            let n = base.iter().position(|&b| b == 0).unwrap_or(0);
            let same = env_is(c"WINLUX_TEST_BASE", &base[..n]);
            ExitProcess(if !ok {
                7
            } else if !same {
                8
            } else {
                0
            });
        }
        let null = core::ptr::null_mut();
        let screen = GetDC(null);
        check!(!screen.is_null(), "GetDC(NULL)");
        check!(
            WindowFromDC(screen) == GetDesktopWindow() && !GetDesktopWindow().is_null(),
            "WindowFromDC(GetDC(NULL)) rend le bureau"
        );
        ReleaseDC(null, screen);
        let mem = CreateCompatibleDC(null);
        check!(
            WindowFromDC(mem).is_null(),
            "WindowFromDC d'un DC mémoire : aucune fenêtre"
        );
        DeleteDC(mem);

        SetEnvironmentVariableA(c"WINLUX_TEST_PLOMBERIE".as_ptr(), c"oui".as_ptr());
        let mut base = [0u8; 20];
        hex(GetModuleHandleA(core::ptr::null()) as usize, &mut base);
        SetEnvironmentVariableA(c"WINLUX_TEST_BASE".as_ptr(), base.as_ptr() as *const c_char);
        let mut path = [0 as c_char; 512];
        GetModuleFileNameA(null, path.as_mut_ptr(), 512);
        let mut startup: StartupInfoA = core::mem::zeroed();
        startup.cb = core::mem::size_of::<StartupInfoA>() as u32;
        let mut child: ProcessInformation = core::mem::zeroed();
        let block = *b"A=1\0\0";
        let launched = CreateProcessA(
            path.as_ptr(),
            c"chromium_compat.exe @enfant".as_ptr(),
            null,
            null,
            0,
            0,
            block.as_ptr() as *const core::ffi::c_void,
            core::ptr::null(),
            &startup,
            &mut child,
        ) != 0;
        check!(launched, "CreateProcessA avec un bloc d'environnement");
        if launched {
            let mut code = 99u32;
            let done = WaitForSingleObject(child.process, 20_000) == 0
                && GetExitCodeProcess(child.process, &mut code) != 0;
            check!(
                done && code != 7 && code != 99,
                "l'enfant garde les variables de l'hôte et reçoit celles du bloc"
            );
            check!(
                done && code == 0,
                "même .exe dans l'enfant : chargé à la même adresse (#767)"
            );
            CloseHandle(child.thread);
            CloseHandle(child.process);
        }
    }
    finish()
}
