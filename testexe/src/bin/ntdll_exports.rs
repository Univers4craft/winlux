//! #851 : ntdll a de vrais en-têtes PE et une table d'exports lisible sans
//! GetProcAddress. Le bac à sable de Chromium (Claude, Electron) parcourt
//! lui-même cette table ; sans elle, NtQueryInformationProcess restait nul et
//! le courtier sautait à l'adresse 0.

#![no_std]
#![no_main]

use winlux_testexe::*;

unsafe fn rd16(a: usize) -> usize {
    (a as *const u16).read_unaligned() as usize
}
unsafe fn rd32(a: usize) -> usize {
    (a as *const u32).read_unaligned() as usize
}

/// Recherche par nom dans la table d'exports, comme le fait Chromium.
unsafe fn export(base: usize, wanted: &[u8]) -> usize {
    let pe = base + rd32(base + 0x3c);
    let dir = base + rd32(pe + 24 + 112);
    let n = rd32(dir + 24);
    let names = base + rd32(dir + 32);
    let ords = base + rd32(dir + 36);
    let funcs = base + rd32(dir + 28);
    for i in 0..n {
        let p = (base + rd32(names + 4 * i)) as *const u8;
        let mut k = 0;
        while k < wanted.len() && *p.add(k) == wanted[k] {
            k += 1;
        }
        if k == wanted.len() && *p.add(k) == 0 {
            return base + rd32(funcs + 4 * rd16(ords + 2 * i));
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let base = GetModuleHandleA(c"ntdll.dll".as_ptr()) as usize;
        check!(base != 0, "GetModuleHandle(ntdll)");
        if base != 0 {
            check!(rd16(base) == 0x5A4D, "signature MZ");
            let pe = base + rd32(base + 0x3c);
            check!(rd32(pe) == 0x4550, "signature PE");
            check!(rd16(pe + 24) == 0x20b, "en-tête optionnel PE32+");
            check!(rd32(pe + 24 + 116) != 0, "répertoire d'exports présent");
            let f = export(base, b"NtQueryInformationProcess");
            check!(f != 0, "NtQueryInformationProcess dans la table");
            check!(
                export(base, b"RtlGetVersion") != 0,
                "RtlGetVersion dans la table"
            );
            check!(export(base, b"NePasTrouver") == 0, "nom absent introuvable");
            if f != 0 {
                // ProcessDebugPort (7) sur le processus courant : appel réel
                // à travers le relais.
                let q: extern "system" fn(isize, u32, *mut usize, u32, *mut u32) -> i32 =
                    core::mem::transmute(f);
                let mut port = 1usize;
                check!(
                    q(-1, 7, &mut port, 8, core::ptr::null_mut()) == 0 && port == 0,
                    "NtQueryInformationProcess appelé par le relais"
                );
            }
        }
    }
    finish()
}
