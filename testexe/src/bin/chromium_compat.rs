//! #996 : deux conditions du processus GPU de Chromium (Claude, Electron).
//! - `WindowFromDC(GetDC(NULL))` rend le bureau : ANGLE refuse sinon
//!   l'affichage EGL et le processus GPU échoue en boucle.
//! - Un enfant Windows lancé avec un bloc d'environnement explicite garde
//!   les variables de l'hôte (affichage, configuration de Winlux) : Chromium
//!   en donne un sans DISPLAY à son processus GPU.
//! - #767 : un enfant qui exécute le même .exe le charge à la même adresse,
//!   comme sous Windows ; le bac à sable de Chromium tue sinon l'enfant.
//! - #767 : VirtualAllocEx, WriteProcessMemory, VirtualProtectEx et
//!   VirtualFreeEx dans un enfant créé suspendu (préparation du bac à sable).
//! - #996 : NtQueryObject rend les droits réels d'un fichier ouvert en
//!   lecture (le mandataire de polices de Chromium les vérifie), et
//!   CreateDIBSection accepte une section : les pixels vivent dans la
//!   section, partagée avec ses autres vues (sortie logicielle).

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
    fn VirtualAllocEx(p: Handle, addr: *mut u8, size: usize, kind: u32, prot: u32) -> *mut u8;
    fn VirtualProtectEx(p: Handle, addr: *mut u8, size: usize, prot: u32, old: *mut u32) -> i32;
    fn VirtualFreeEx(p: Handle, addr: *mut u8, size: usize, kind: u32) -> i32;
    fn WriteProcessMemory(
        p: Handle,
        addr: *mut u8,
        buf: *const u8,
        n: usize,
        done: *mut usize,
    ) -> i32;
    fn CreateFileMappingW(
        file: Handle,
        sa: *mut core::ffi::c_void,
        protect: u32,
        high: u32,
        low: u32,
        name: *const u16,
    ) -> Handle;
    fn MapViewOfFile(map: Handle, access: u32, high: u32, low: u32, size: usize) -> *mut u8;
    fn UnmapViewOfFile(addr: *mut u8) -> i32;
    fn ReadProcessMemory(
        p: Handle,
        addr: *const u8,
        buf: *mut u8,
        n: usize,
        done: *mut usize,
    ) -> i32;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "system" {
    fn GetPixel(dc: Handle, x: i32, y: i32) -> u32;
}
#[link(name = "ntdll", kind = "raw-dylib")]
extern "system" {
    fn NtQueryObject(h: Handle, class: u32, buf: *mut u8, len: u32, ret: *mut u32) -> i32;
    fn NtQueryInformationProcess(
        p: Handle,
        class: u32,
        buf: *mut u8,
        len: u32,
        ret: *mut u32,
    ) -> i32;
}

/// Lit un mot de 64 bits dans un autre processus, ou 0.
unsafe fn read_u64(p: Handle, addr: usize) -> usize {
    let mut v = 0usize;
    let mut n = 0usize;
    if ReadProcessMemory(
        p,
        addr as *const u8,
        &mut v as *mut usize as *mut u8,
        8,
        &mut n,
    ) == 0
        || n != 8
    {
        return 0;
    }
    v
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

        // Fichier ouvert en lecture : aucun droit d'écriture annoncé
        // (OBJECT_BASIC_INFORMATION.GrantedAccess, à l'octet 4).
        let mut path = [0 as c_char; 512];
        GetModuleFileNameA(null, path.as_mut_ptr(), 512);
        let file = CreateFileA(path.as_ptr(), 0x8000_0000, 1, null, 3, 0, null);
        let mut info = [0u8; 56];
        let mut got = 0u32;
        let status = NtQueryObject(file, 0, info.as_mut_ptr(), 56, &mut got);
        let access = u32::from_le_bytes([info[4], info[5], info[6], info[7]]);
        check!(
            status == 0 && access & 0x1 != 0 && access & 0xd0116 == 0,
            "NtQueryObject : fichier en lecture seule, droits de lecture seulement"
        );
        CloseHandle(file);

        // DIB dans une section : les deux vues voient les mêmes pixels.
        let section = CreateFileMappingW(
            -1isize as Handle,
            null,
            0x04,
            0,
            0x2_0000,
            core::ptr::null(),
        );
        check!(
            !section.is_null(),
            "CreateFileMappingW d'une section anonyme"
        );
        let view = MapViewOfFile(section, 0x6, 0, 0, 0x2_0000);
        check!(!view.is_null(), "MapViewOfFile de la section");
        if !view.is_null() {
            let offset = 0x1_0010usize;
            *view.add(offset) = 0x5A;
            let header = BitmapInfoHeader {
                size: 40,
                width: 16,
                height: -8,
                planes: 1,
                bit_count: 32,
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = null;
            let dib = CreateDIBSection(null, &header, 0, &mut bits, section, offset as u32);
            check!(
                !dib.is_null() && !bits.is_null() && *(bits as *const u8) == 0x5A,
                "CreateDIBSection avec une section : pixels pris dans la section"
            );
            if !dib.is_null() {
                let dc = CreateCompatibleDC(null);
                let old = SelectObject(dc, dib);
                // Pixel (1, 0) écrit par l'autre vue, en BGRX, comme le
                // processus de rendu de Chromium.
                let px = view.add(offset + 4);
                (*px, *px.add(1), *px.add(2)) = (0x11, 0x22, 0x33);
                check!(
                    GetPixel(dc, 1, 0) == 0x00_11_22_33,
                    "un pixel écrit par l'autre vue se lit dans la DIB"
                );
                SelectObject(dc, old);
                DeleteDC(dc);
                check!(DeleteObject(dib) != 0, "DeleteObject de la DIB en section");
            }
            UnmapViewOfFile(view);
        }
        CloseHandle(section);

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

        // Enfant suspendu : mémoire allouée, écrite, relue, protégée, libérée.
        let mut child: ProcessInformation = core::mem::zeroed();
        let launched = CreateProcessA(
            path.as_ptr(),
            c"chromium_compat.exe @enfant".as_ptr(),
            null,
            null,
            0,
            4, // CREATE_SUSPENDED
            core::ptr::null(),
            core::ptr::null(),
            &startup,
            &mut child,
        ) != 0;
        check!(launched, "CreateProcessA suspendu");
        if launched {
            let p = child.process;
            let mem = VirtualAllocEx(p, core::ptr::null_mut(), 8192, 0x3000, 0x04);
            check!(!mem.is_null(), "VirtualAllocEx dans l'enfant suspendu");
            if !mem.is_null() {
                let data = *b"bac a sable";
                let mut n = 0usize;
                let wrote = WriteProcessMemory(p, mem, data.as_ptr(), data.len(), &mut n) != 0
                    && n == data.len();
                let mut back = [0u8; 11];
                let read = ReadProcessMemory(p, mem, back.as_mut_ptr(), 11, &mut n) != 0;
                check!(
                    wrote && read && back == data,
                    "WriteProcessMemory puis ReadProcessMemory dans cette mémoire"
                );
                let mut old = 0u32;
                check!(
                    VirtualProtectEx(p, mem, 4096, 0x20, &mut old) != 0 && old == 0x04,
                    "VirtualProtectEx rend l'ancienne protection (lecture-écriture)"
                );
                check!(
                    VirtualFreeEx(p, mem, 0, 0x8000) != 0,
                    "VirtualFreeEx(MEM_RELEASE) dans l'enfant"
                );
            }
            // #1314 : comme le crashpad de Chromium, PEB de l'enfant par
            // NtQueryInformationProcess, puis PEB->Ldr et la première entrée
            // de InLoadOrderModuleList : son DllBase est l'image du programme.
            let mut pbi = [0usize; 6];
            let mut ret = 0u32;
            let st = NtQueryInformationProcess(p, 0, pbi.as_mut_ptr() as *mut u8, 48, &mut ret);
            let ldr = if st == 0 {
                read_u64(p, pbi[1] + 0x18)
            } else {
                0
            };
            check!(ldr != 0, "PEB->Ldr de l'enfant lisible (#1314)");
            if ldr != 0 {
                let first = read_u64(p, ldr + 0x10);
                check!(
                    first != 0
                        && first != ldr + 0x10
                        && read_u64(p, first + 0x30)
                            == GetModuleHandleA(core::ptr::null()) as usize,
                    "InLoadOrderModuleList commence par l'image du programme (#1314)"
                );
            }
            // L'enfant n'a pas souffert : relancé, il se termine normalement.
            ResumeThread(child.thread);
            let mut code = 99u32;
            let done = WaitForSingleObject(p, 20_000) == 0 && GetExitCodeProcess(p, &mut code) != 0;
            check!(
                done && code != 99,
                "l'enfant relancé se termine normalement"
            );
            CloseHandle(child.thread);
            CloseHandle(p);
        }
    }
    finish()
}
