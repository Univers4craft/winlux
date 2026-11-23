//! #2024 : OpenThread ouvre le fil d'un autre processus du même compte, et
//! NtQueryInformationThread(ThreadBasicInformation) donne son TEB, que
//! ReadProcessMemory lit ensuite. C'est ce que fait crashpad (Chromium,
//! Electron) sur chaque fil d'un processus planté.
//!
//! « fil_etranger.exe --enfant » écrit l'identifiant de son fil dans
//! fil_etranger.tid (dossier courant), puis attend ; le parent l'inspecte,
//! puis le termine.

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::null_mut;
use winlux_testexe::*;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn GetModuleFileNameA(module: Handle, name: *mut u8, size: u32) -> u32;
    fn OpenThread(access: u32, inherit: i32, tid: u32) -> Handle;
    fn GetThreadId(thread: Handle) -> u32;
    fn GetExitCodeThread(thread: Handle, code: *mut u32) -> i32;
    fn SuspendThread(thread: Handle) -> u32;
    fn ResumeThread(thread: Handle) -> u32;
    fn GetThreadContext(thread: Handle, context: *mut u8) -> i32;
    fn SetThreadContext(thread: Handle, context: *const u8) -> i32;
    fn ReadProcessMemory(
        process: Handle,
        address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        read: *mut usize,
    ) -> i32;
}

#[link(name = "ntdll", kind = "raw-dylib")]
extern "system" {
    fn NtQueryInformationThread(
        thread: Handle,
        class: u32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
    fn NtSuspendProcess(process: Handle) -> i32;
    fn NtResumeProcess(process: Handle) -> i32;
}

/// CONTEXT x64 aligné sur 16 octets (1232 octets).
#[repr(C, align(16))]
struct Contexte([u8; 1232]);

const THREAD_ALL_ACCESS: u32 = 0x1F_FFFF;
const FICHIER: &core::ffi::CStr = c"fil_etranger.tid";

fn out_hex(v: u64) {
    let mut b = *b"0x0000000000000000";
    for i in 0..16 {
        let d = ((v >> (60 - 4 * i)) & 0xF) as u8;
        b[2 + i] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
    }
    out(&b);
}

unsafe fn word(process: Handle, at: usize) -> Option<u64> {
    let mut v = 0u64;
    let mut n = 0usize;
    let ok = ReadProcessMemory(
        process,
        at as *const c_void,
        &mut v as *mut u64 as *mut c_void,
        8,
        &mut n,
    );
    (ok != 0 && n == 8).then_some(v)
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        if !strstr(GetCommandLineA(), c"--enfant".as_ptr()).is_null() {
            let tid = GetCurrentThreadId();
            let mut n = 0u32;
            let f = CreateFileA(
                FICHIER.as_ptr(),
                0x4000_0000,
                0,
                null_mut(),
                2,
                0x80,
                null_mut(),
            );
            WriteFile(f, &tid as *const u32 as *const u8, 4, &mut n, null_mut());
            CloseHandle(f);
            Sleep(20_000);
            ExitProcess(0)
        }
        let mut exe = [0u8; 600];
        let n = GetModuleFileNameA(null_mut(), exe.as_mut_ptr(), 500) as usize;
        let mut cmd = [0u8; 700];
        cmd[0] = b'"';
        cmd[1..1 + n].copy_from_slice(&exe[..n]);
        let tail = b"\" --enfant\0";
        cmd[1 + n..1 + n + tail.len()].copy_from_slice(tail);
        DeleteFileA(FICHIER.as_ptr());
        let mut startup: StartupInfoA = core::mem::zeroed();
        startup.cb = core::mem::size_of::<StartupInfoA>() as u32;
        let mut child: ProcessInformation = core::mem::zeroed();
        let launched = CreateProcessA(
            exe.as_ptr() as *const _,
            cmd.as_mut_ptr() as *mut _,
            null_mut(),
            null_mut(),
            0,
            0,
            null_mut(),
            null_mut(),
            &startup,
            &mut child,
        ) != 0;
        out(b"  programme : ");
        out(&exe[..n]);
        out(b"\n");
        if !launched {
            out(b"  erreur : ");
            out_hex(GetLastError() as u64);
            out(b"\n");
        }
        check!(launched, "CreateProcessA lance l'enfant");
        if !launched {
            finish()
        }
        let mut octets = [0u8; 4];
        let mut total = 0u32;
        for _ in 0..200 {
            let f = CreateFileA(
                FICHIER.as_ptr(),
                0x8000_0000,
                1,
                null_mut(),
                3,
                0x80,
                null_mut(),
            );
            if f as isize != -1 {
                ReadFile(f, octets.as_mut_ptr(), 4, &mut total, null_mut());
                CloseHandle(f);
                if total == 4 {
                    break;
                }
            }
            Sleep(50);
        }
        DeleteFileA(FICHIER.as_ptr());
        let tid = u32::from_le_bytes(octets);
        check!(
            total == 4 && tid != 0,
            "l'enfant donne l'identifiant de son fil"
        );
        out(b"  fil de l'enfant : ");
        out_hex(tid as u64);
        out(b" (dwThreadId de CreateProcess : ");
        out_hex(child.thread_id as u64);
        out(b")\n");

        // L'enfant publie son TEB au démarrage : on réessaie quelques secondes.
        let mut info = [0u64; 6];
        let mut thread: Handle = null_mut();
        for _ in 0..100 {
            if thread.is_null() {
                thread = OpenThread(THREAD_ALL_ACCESS, 0, tid);
            }
            if !thread.is_null() {
                let mut got = 0u32;
                let st = NtQueryInformationThread(
                    thread,
                    0,
                    info.as_mut_ptr() as *mut c_void,
                    48,
                    &mut got,
                );
                if st == 0 && info[1] != 0 {
                    break;
                }
            }
            Sleep(50);
        }
        check!(
            !thread.is_null(),
            "OpenThread ouvre le fil d'un autre processus"
        );
        check!(GetThreadId(thread) == tid, "GetThreadId sur ce fil");
        out(b"  TEB lu : ");
        out_hex(info[1]);
        out(b"\n");
        check!(
            info[0] as u32 == 0x103,
            "ThreadBasicInformation : STATUS_PENDING"
        );
        check!(
            info[2] == child.process_id as u64 && info[3] == tid as u64,
            "ThreadBasicInformation : ClientId de l'enfant"
        );
        let teb = info[1] as usize;
        check!(teb != 0, "ThreadBasicInformation : TEB de l'enfant");
        check!(
            teb != 0 && word(child.process, teb + 0x30) == Some(teb as u64),
            "ReadProcessMemory : NtTib.Self du TEB lu"
        );
        check!(
            teb != 0 && word(child.process, teb + 0x48) == Some(tid as u64),
            "ReadProcessMemory : ClientId.UniqueThread du TEB lu"
        );

        // #3320 : processus en marche, ni suspension ni registres.
        check!(
            SuspendThread(thread) == u32::MAX,
            "SuspendThread refusé tant que l'enfant tourne"
        );
        // Processus suspendu (comme crashpad avant son instantané) : les
        // appels réussissent sans rien faire, Rip et Rsp seulement.
        NtSuspendProcess(child.process);
        let mut compte = u32::MAX;
        for _ in 0..100 {
            compte = SuspendThread(thread);
            if compte != u32::MAX {
                break;
            }
            Sleep(10);
        }
        check!(compte == 1, "SuspendThread : 1 sur l'enfant suspendu");
        let mut ctx = Contexte([0; 1232]);
        (ctx.0.as_mut_ptr().add(0x30) as *mut u32).write(0x0010_001F);
        let lu = GetThreadContext(thread, ctx.0.as_mut_ptr()) != 0;
        let drapeaux = (ctx.0.as_ptr().add(0x30) as *const u32).read();
        let rsp = (ctx.0.as_ptr().add(0x98) as *const u64).read();
        out(b"  Rsp lu : ");
        out_hex(rsp);
        out(b"\n");
        check!(
            lu && drapeaux == 0x0010_0000 && rsp != 0,
            "GetThreadContext : Rsp, sans CONTEXT_CONTROL annoncé"
        );
        check!(
            SetThreadContext(thread, ctx.0.as_ptr()) == 0,
            "SetThreadContext toujours refusé"
        );
        check!(ResumeThread(thread) == 1, "ResumeThread : 1, sans effet");
        NtResumeProcess(child.process);

        TerminateProcess(child.process, 9);
        check!(
            WaitForSingleObject(thread, 5_000) == 0,
            "attente du fil : finie avec l'enfant"
        );
        let mut code = 259u32;
        check!(
            GetExitCodeThread(thread, &mut code) != 0 && code != 259,
            "GetExitCodeThread : plus STILL_ACTIVE"
        );
        CloseHandle(thread);
        CloseHandle(child.thread);
        CloseHandle(child.process);
        finish()
    }
}
