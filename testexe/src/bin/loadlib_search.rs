//! LoadLibraryExW d'un simple nom de DLL avec LOAD_LIBRARY_SEARCH_SYSTEM32,
//! comme la CRT de MSVC au démarrage de claude.exe (#1019). Lancé depuis un
//! dossier personnel, il ne doit déclencher aucune demande d'autorisation.
#![no_std]
#![no_main]

use winlux_testexe::*;

type LoadLibraryExW = unsafe extern "system" fn(*const u16, Handle, u32) -> Handle;

fn wide<const N: usize>(text: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    let mut i = 0;
    while i < N {
        out[i] = text[i] as u16;
        i += 1;
    }
    out
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let proc = GetProcAddress(kernel32, c"LoadLibraryExW".as_ptr());
        if proc.is_null() {
            out(b"ECHEC : LoadLibraryExW absente\n");
            ExitProcess(1)
        }
        let load: LoadLibraryExW = core::mem::transmute(proc);
        let synch = wide(b"api-ms-win-core-synch-l1-2-0\0");
        let kernel = wide(b"kernel32.dll\0");
        // LOAD_LIBRARY_SEARCH_SYSTEM32, puis LOAD_WITH_ALTERED_SEARCH_PATH.
        for (name, flags) in [(&synch[..], 0x800u32), (&kernel[..], 0x800), (&kernel[..], 0x8)] {
            if load(name.as_ptr(), core::ptr::null_mut(), flags).is_null() {
                out(b"ECHEC : DLL systeme introuvable\n");
                ExitProcess(1)
            }
        }
        out(b"OK : DLL systeme chargees sans toucher au dossier courant\n");
        ExitProcess(0)
    }
}
