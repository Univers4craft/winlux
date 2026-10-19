//! Suite auto-vérifiante du palier P1. Code de sortie = nombre d'échecs.
#![no_std]
#![no_main]

use core::ffi::c_char;
use core::ptr::null_mut;
use winlux_testexe::*;

extern "system" fn thread_test_entry(param: *mut core::ffi::c_void) -> u32 {
    (param as usize as u32).wrapping_add(1)
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Winlux P1 - suite de tests\n");
    unsafe {
        // --- printf / sprintf ---
        let mut buf = [0 as c_char; 256];
        let b = buf.as_mut_ptr();
        sprintf(b, c"%d|%5d|%-5d|%05d|%+d".as_ptr(), -7, 42, 42, 42, 3);
        check!(cstr_eq(b, b"-7|   42|42   |00042|+3"), "sprintf entiers et largeurs");
        sprintf(b, c"%x|%X|%#x|%o|%u".as_ptr(), 0xbeefu32, 0xbeefu32, 255u32, 8u32, 4000000000u32);
        check!(cstr_eq(b, b"beef|BEEF|0xff|10|4000000000"), "sprintf hexa, octal, non signe");
        sprintf(b, c"%lld|%I64d|%llu".as_ptr(), -9000000000i64, 123456789012i64, u64::MAX);
        check!(cstr_eq(b, b"-9000000000|123456789012|18446744073709551615"), "sprintf 64 bits");
        sprintf(b, c"%.2f|%8.3f|%e|%g|%g".as_ptr(), 3.14159f64, -2.5f64, 12345.678f64, 0.0001f64, 1e20f64);
        check!(cstr_eq(b, b"3.14|  -2.500|1.234568e+004|0.0001|1e+020"), "sprintf flottants (format msvcrt)");
        sprintf(b, c"[%s|%10s|%-4s|%.2s|%c%c]".as_ptr(), c"abc".as_ptr(), c"droite".as_ptr(), c"g".as_ptr(), c"tronque".as_ptr(), b'W' as i32, b'L' as i32);
        check!(cstr_eq(b, b"[abc|    droite|g   |tr|WL]"), "sprintf chaines et caracteres");
        sprintf(b, c"%*d|%-*d|%%|%.*s".as_ptr(), 6, 1, 3, 2, 3, c"abcdef".as_ptr());
        check!(cstr_eq(b, b"     1|2  |%|abc"), "sprintf largeur et precision par *");
        // Beaucoup d'arguments : vérifie le passage par la pile (convention Windows x64).
        sprintf(b, c"%d %d %d %d %d %d %d %d %.1f %s".as_ptr(), 1, 2, 3, 4, 5, 6, 7, 8, 9.5f64, c"fin".as_ptr());
        check!(cstr_eq(b, b"1 2 3 4 5 6 7 8 9.5 fin"), "sprintf 10 arguments (pile)");

        // --- mémoire ---
        let p = malloc(1000) as *mut u8;
        check!(!p.is_null(), "malloc");
        memset(p as _, 0xAB, 1000);
        check!(*p.add(999) == 0xAB, "memset");
        free(p as _);
        let heap = GetProcessHeap();
        let h = HeapAlloc(heap, 8 /* HEAP_ZERO_MEMORY */, 4096) as *mut u8;
        check!(!h.is_null() && *h.add(4095) == 0, "HeapAlloc avec HEAP_ZERO_MEMORY");
        check!(HeapFree(heap, 0, h as _) != 0, "HeapFree");
        let h = HeapAlloc(heap, 0, 4) as *mut u8;
        check!(!h.is_null(), "HeapAlloc avant HeapReAlloc");
        if !h.is_null() {
            *h = 0x5a;
            let grown = HeapReAlloc(heap, 8, h as _, 8) as *mut u8;
            check!(!grown.is_null() && *grown == 0x5a && *grown.add(7) == 0, "HeapReAlloc conserve et zero-initialise l'extension");
            if !grown.is_null() { check!(HeapFree(heap, 0, grown as _) != 0, "HeapFree après HeapReAlloc"); }
        }
        let v = VirtualAlloc(null_mut(), 65536, 0x3000, 0x04) as *mut u32;
        check!(!v.is_null() && (v as usize) % 65536 == 0, "VirtualAlloc aligne sur 64 Ko");
        *v.add(100) = 0xdead;
        check!(*v.add(100) == 0xdead, "VirtualAlloc memoire inscriptible");
        let mut mbi: MemoryBasicInformation = core::mem::zeroed();
        let query_size = VirtualQuery(v as _, &mut mbi, core::mem::size_of::<MemoryBasicInformation>());
        check!(query_size == core::mem::size_of::<MemoryBasicInformation>() && mbi.allocation_base == v as _ && mbi.state == 0x1000 && mbi.protect == 0x04, "VirtualQuery page committée");
        let mut old_prot = 0u32;
        let read_only = VirtualProtect(v as _, 4096, 0x02, &mut old_prot) != 0 && old_prot == 0x04;
        let restore = VirtualProtect(v as _, 4096, 0x04, &mut old_prot) != 0 && old_prot == 0x02;
        check!(read_only && restore, "VirtualProtect et restauration des protections");
        check!(VirtualFree(v as _, 4096, 0x4000) != 0, "VirtualFree MEM_DECOMMIT");
        let mut mbi: MemoryBasicInformation = core::mem::zeroed();
        let _ = VirtualQuery(v as _, &mut mbi, core::mem::size_of::<MemoryBasicInformation>());
        check!(mbi.state == 0x2000 && mbi.protect == 0x01, "VirtualQuery après décommit");
        let recommitted = VirtualAlloc(v as _, 4096, 0x1000, 0x04) as *mut u32;
        check!(recommitted == v && *v.add(100) == 0, "VirtualAlloc recommit zeroed");
        check!(VirtualFree(v as _, 0, 0x8000) != 0, "VirtualFree");

        // --- fichiers Linux/Windows (handle Win32, accès binaire, seek) ---
        let mut file_path = [0 as c_char; 128];
        sprintf(file_path.as_mut_ptr(), c"Z:\\tmp\\winlux-p1-%u.bin".as_ptr(), GetCurrentProcessId());
        let fh = CreateFileA(file_path.as_ptr(), 0xC000_0000, 7, null_mut(), 2, 0, null_mut());
        let data = [0u8, 0x7f, 0xff, b'W'];
        let mut n = 0u32;
        let write_ok = fh as usize != usize::MAX && WriteFile(fh, data.as_ptr(), data.len() as u32, &mut n, null_mut()) != 0;
        check!(write_ok && n == data.len() as u32 && FlushFileBuffers(fh) != 0, "CreateFile/WriteFile/FlushFileBuffers Linux");
        check!(GetFileSize(fh, null_mut()) == data.len() as u32, "GetFileSize");
        let seek_ok = SetFilePointer(fh, 0, null_mut(), 0) == 0;
        let mut read_back = [0u8; 4];
        let read_ok = seek_ok && ReadFile(fh, read_back.as_mut_ptr(), read_back.len() as u32, &mut n, null_mut()) != 0;
        check!(read_ok && n == data.len() as u32 && read_back == data, "SetFilePointer/ReadFile binaire");
        check!(CloseHandle(fh) != 0 && DeleteFileA(file_path.as_ptr()) != 0, "CloseHandle/DeleteFileA");
        let mut full_path = [0 as c_char; 256];
        let mut file_part = null_mut();
        let full_len = GetFullPathNameA(file_path.as_ptr(), full_path.len() as u32, full_path.as_mut_ptr(), &mut file_part);
        check!(full_len > 0 && full_len < full_path.len() as u32 && !file_part.is_null() && *file_part != 0, "GetFullPathNameA et nom de fichier final");
        let mut source_path = [0 as c_char; 128];
        let mut copy_path = [0 as c_char; 128];
        let mut moved_path = [0 as c_char; 128];
        sprintf(source_path.as_mut_ptr(), c"Z:\\tmp\\winlux-copy-src-%u.bin".as_ptr(), GetCurrentProcessId());
        sprintf(copy_path.as_mut_ptr(), c"Z:\\tmp\\winlux-copy-dst-%u.bin".as_ptr(), GetCurrentProcessId());
        sprintf(moved_path.as_mut_ptr(), c"Z:\\tmp\\winlux-move-dst-%u.bin".as_ptr(), GetCurrentProcessId());
        let src_handle = CreateFileA(source_path.as_ptr(), 0xC000_0000, 7, null_mut(), 2, 0, null_mut());
        let source_ok = src_handle as usize != usize::MAX && WriteFile(src_handle, b"data".as_ptr(), 4, &mut n, null_mut()) != 0 && CloseHandle(src_handle) != 0;
        let copy_ok = source_ok && CopyFileA(source_path.as_ptr(), copy_path.as_ptr(), 1) != 0 && CopyFileA(source_path.as_ptr(), copy_path.as_ptr(), 1) == 0;
        let move_ok = copy_ok && MoveFileA(copy_path.as_ptr(), moved_path.as_ptr()) != 0;
        let cleanup_files = DeleteFileA(source_path.as_ptr()) != 0 && DeleteFileA(moved_path.as_ptr()) != 0;
        check!(source_ok && copy_ok && move_ok && cleanup_files, "CopyFileA/MoveFileA/DeleteFileA");
        let mut dir_path = [0 as c_char; 128];
        sprintf(dir_path.as_mut_ptr(), c"Z:\\tmp\\winlux-dir-%u".as_ptr(), GetCurrentProcessId());
        check!(CreateDirectoryA(dir_path.as_ptr(), null_mut()) != 0 && RemoveDirectoryA(dir_path.as_ptr()) != 0, "CreateDirectoryA/RemoveDirectoryA");
        let wide_file_name = c"Z:\\tmp\\winlux-p1-wide.bin".to_bytes();
        let mut wide_file_path = [0u16; 64];
        for (i, byte) in wide_file_name.iter().enumerate() { wide_file_path[i] = *byte as u16; }
        let mut wide_full = [0u16; 256];
        let mut wide_part = null_mut();
        check!(GetFullPathNameW(wide_file_path.as_ptr(), wide_full.len() as u32, wide_full.as_mut_ptr(), &mut wide_part) > 0 && !wide_part.is_null(), "GetFullPathNameW");
        let wh = CreateFileW(wide_file_path.as_ptr(), 0xC000_0000, 7, null_mut(), 2, 0, null_mut());
        check!(!wh.is_null() && CloseHandle(wh) != 0 && DeleteFileW(wide_file_path.as_ptr()) != 0, "CreateFileW/DeleteFileW chemin Unicode");

        // --- registre persistant partagé ---
        let mut key_path = [0 as c_char; 128];
        sprintf(key_path.as_mut_ptr(), c"Software\\Winlux\\P1_%u".as_ptr(), GetCurrentProcessId());
        let mut key = null_mut();
        let mut disposition = 0u32;
        let created = RegCreateKeyExA(0x8000_0001usize as Handle, key_path.as_ptr(), 0, null_mut(), 0, 0xF003F, null_mut(), &mut key, &mut disposition) == 0;
        let value_data = [0x00u8, 0xff, 0x42, 0x00];
        let set_ok = created && RegSetValueExA(key, c"Binary".as_ptr(), 0, 3, value_data.as_ptr(), value_data.len() as u32) == 0;
        let mut value_type = 0u32;
        let mut value_size = 0u32;
        let size_ok = set_ok && RegQueryValueExA(key, c"binary".as_ptr(), null_mut(), &mut value_type, null_mut(), &mut value_size) == 0;
        let mut value_out = [0u8; 4];
        let query_ok = size_ok && value_size == value_out.len() as u32 && RegQueryValueExA(key, c"Binary".as_ptr(), null_mut(), &mut value_type, value_out.as_mut_ptr(), &mut value_size) == 0;
        check!(query_ok && value_type == 3 && value_out == value_data, "RegCreate/Set/Query registre binaire persistant");
        let cleanup = RegDeleteValueA(key, c"Binary".as_ptr()) == 0 && RegCloseKey(key) == 0 && RegDeleteKeyA(0x8000_0001usize as Handle, key_path.as_ptr()) == 0;
        check!(cleanup, "nettoyage du registre de test");

        // --- événements, mutex récursif et thread Win32 ---
        let event = CreateEventA(null_mut(), 0, 0, null_mut());
        let event_ok = !event.is_null() && WaitForSingleObject(event, 0) == 258 && SetEvent(event) != 0
            && WaitForSingleObject(event, 0) == 0 && WaitForSingleObject(event, 0) == 258;
        check!(event_ok && CloseHandle(event) != 0, "événement auto-reset");
        let event_a = CreateEventA(null_mut(), 0, 0, null_mut());
        let event_b = CreateEventA(null_mut(), 0, 0, null_mut());
        SetEvent(event_b);
        let events = [event_a, event_b];
        check!(WaitForMultipleObjects(2, events.as_ptr(), 0, 0) == 1, "WaitForMultipleObjects (wait-any)");
        CloseHandle(event_a); CloseHandle(event_b);
        let all_a = CreateEventA(null_mut(), 0, 0, null_mut());
        let all_b = CreateEventA(null_mut(), 0, 0, null_mut());
        let all_events = [all_a, all_b];
        SetEvent(all_b);
        let not_partial = WaitForMultipleObjects(2, all_events.as_ptr(), 1, 0) == 258 && WaitForSingleObject(all_b, 0) == 0;
        SetEvent(all_b); SetEvent(all_a);
        check!(not_partial && WaitForMultipleObjects(2, all_events.as_ptr(), 1, 0) == 0 && WaitForSingleObject(all_a, 0) == 258, "WaitForMultipleObjects wait-all atomique");
        CloseHandle(all_a); CloseHandle(all_b);
        let mutex = CreateMutexA(null_mut(), 0, null_mut());
        let mutex_ok = WaitForSingleObject(mutex, 0) == 0 && WaitForSingleObject(mutex, 0) == 0
            && ReleaseMutex(mutex) != 0 && ReleaseMutex(mutex) != 0 && CloseHandle(mutex) != 0;
        check!(mutex_ok, "mutex Win32 récursif");
        let mut cs = [0usize; 5];
        let cs_ptr = cs.as_mut_ptr() as *mut core::ffi::c_void;
        InitializeCriticalSection(cs_ptr);
        EnterCriticalSection(cs_ptr); EnterCriticalSection(cs_ptr);
        let cs_ok = TryEnterCriticalSection(cs_ptr) != 0;
        LeaveCriticalSection(cs_ptr); LeaveCriticalSection(cs_ptr); LeaveCriticalSection(cs_ptr);
        DeleteCriticalSection(cs_ptr);
        check!(cs_ok, "section critique récursive");
        let mut thread_id = 0u32;
        let thread = CreateThread(null_mut(), 0, thread_test_entry, 41usize as _, 4 /* CREATE_SUSPENDED */, &mut thread_id);
        let mut thread_code = 259u32;
        let thread_ok = !thread.is_null() && thread_id != 0 && WaitForSingleObject(thread, 0) == 258
            && ResumeThread(thread) == 1 && WaitForSingleObject(thread, 5000) == 0
            && GetExitCodeThread(thread, &mut thread_code) != 0 && thread_code == 42;
        check!(thread_ok && CloseHandle(thread) != 0, "CreateThread/WaitForSingleObject/GetExitCodeThread");
        let mut startup: StartupInfoA = core::mem::zeroed();
        startup.cb = core::mem::size_of::<StartupInfoA>() as u32;
        let mut child: ProcessInformation = core::mem::zeroed();
        let launched = CreateProcessA(c"Z:\\bin\\sh".as_ptr(), c"sh -c \"exit 42\"".as_ptr(), null_mut(), null_mut(), 0, 0, null_mut(), null_mut(), &startup, &mut child) != 0;
        let child_done = launched && WaitForSingleObject(child.process, 5_000) == 0;
        let mut child_exit = 0u32;
        let child_status = child_done && GetExitCodeProcess(child.process, &mut child_exit) != 0 && child_exit == 42;
        let mut child_thread_exit = 0u32;
        let child_thread_status = GetExitCodeThread(child.thread, &mut child_thread_exit) != 0 && child_thread_exit == 42;
        check!(child_status && child_thread_status, "CreateProcessA/WaitForSingleObject/GetExitCodeProcess");
        if launched { CloseHandle(child.thread); CloseHandle(child.process); }
        let app_w = [b'/' as u16,b'b' as u16,b'i' as u16,b'n' as u16,b'/' as u16,b't' as u16,b'r' as u16,b'u' as u16,b'e' as u16,0];
        let mut cmd_w = [b't' as u16,b'r' as u16,b'u' as u16,b'e' as u16,0];
        let mut child_w: ProcessInformation = core::mem::zeroed();
        let launched_w = CreateProcessW(app_w.as_ptr(), cmd_w.as_mut_ptr(), null_mut(), null_mut(), 0, 0, null_mut(), null_mut(), &startup, &mut child_w) != 0;
        let child_w_done = launched_w && WaitForSingleObject(child_w.process, 5_000) == 0;
        let mut child_w_exit = 999u32;
        let child_w_ok = child_w_done && GetExitCodeProcess(child_w.process, &mut child_w_exit) != 0 && child_w_exit == 0;
        check!(child_w_ok, "CreateProcessW lance un binaire Linux via le pont");
        if launched_w { CloseHandle(child_w.thread); CloseHandle(child_w.process); }
        // --- module, imports dynamiques ---
        let base = GetModuleHandleA(core::ptr::null());
        check!(!base.is_null() && *(base as *const u16) == 0x5A4D, "GetModuleHandle(NULL) pointe sur l'en-tete MZ");
        let k32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let f = GetProcAddress(k32, c"GetTickCount".as_ptr());
        check!(!f.is_null(), "GetProcAddress(kernel32, GetTickCount)");
        let g: extern "system" fn() -> u32 = core::mem::transmute(f);
        let t = g();
        check!(t.wrapping_sub(GetTickCount()) < 1000 || GetTickCount().wrapping_sub(t) < 1000, "appel via GetProcAddress");
        let dyn_dll = LoadLibraryA(c"testdll.dll".as_ptr());
        let dyn_fn = GetProcAddress(dyn_dll, c"winlux_add".as_ptr());
        let dyn_ok = if dyn_dll.is_null() || dyn_fn.is_null() { false } else {
            let dyn_add: extern "system" fn(u32, u32) -> u32 = core::mem::transmute(dyn_fn);
            dyn_add(19, 23) == 42
        };
        check!(dyn_ok, "LoadLibrary/GetProcAddress DLL dynamique");
        check!(FreeLibrary(dyn_dll) != 0, "FreeLibrary conserve la DLL jusqu'à la fin du processus");
        check!(GetCurrentProcess() as usize == usize::MAX && GetCurrentThread() as usize == (-2isize) as usize && GetCurrentThreadId() != 0, "pseudo-handles processus/thread et TID");
        let mut sysinfo: SystemInfo = core::mem::zeroed();
        GetSystemInfo(&mut sysinfo);
        check!(sysinfo.processor_architecture == 9 && sysinfo.page_size == 4096 && sysinfo.allocation_granularity == 65536 && sysinfo.number_of_processors > 0, "GetSystemInfo cohérent avec le runtime");
        let mut ver: OsVersionInfoA = core::mem::zeroed();
        ver.size = core::mem::size_of::<OsVersionInfoA>() as u32;
        check!(GetVersionExA(&mut ver) != 0 && ver.major == 10 && ver.build == 19045 && GetVersion() & 0xff == 10 && GetVersion() >> 16 == 19045, "version Windows simulée");
        let mut filetime = 0u64;
        GetSystemTimeAsFileTime(&mut filetime);
        check!(filetime > 116_444_736_000_000_000, "GetSystemTimeAsFileTime");
        let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr());
        let rtl_get_version = GetProcAddress(ntdll, c"RtlGetVersion".as_ptr());
        let rtl_ok = if rtl_get_version.is_null() { false } else {
            let f: extern "system" fn(*mut OsVersionInfoW) -> i32 = core::mem::transmute(rtl_get_version);
            let mut vw: OsVersionInfoW = core::mem::zeroed();
            vw.size = core::mem::size_of::<OsVersionInfoW>() as u32;
            f(&mut vw) == 0 && vw.major == 10 && vw.build == 19045
        };
        check!(rtl_ok, "ntdll!RtlGetVersion");

        // --- TEB / PEB via le registre GS ---
        let teb: usize;
        core::arch::asm!("mov {}, gs:[0x30]", out(reg) teb);
        check!(teb != 0, "TEB accessible par gs:[0x30]");
        let peb: usize;
        core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb);
        check!(peb != 0 && *((peb + 0x10) as *const usize) == base as usize, "PEB->ImageBaseAddress");
        let pid: u32;
        core::arch::asm!("mov {:e}, gs:[0x40]", out(reg) pid);
        check!(pid == GetCurrentProcessId(), "TEB->ClientId.UniqueProcess");

        // --- erreurs, TLS, environnement, ligne de commande ---
        SetLastError(1234);
        check!(GetLastError() == 1234, "SetLastError / GetLastError");
        let last: u32;
        core::arch::asm!("mov {:e}, gs:[0x68]", out(reg) last);
        check!(last == 1234, "LastErrorValue stocke dans le TEB");
        let idx = TlsAlloc();
        TlsSetValue(idx, 0x1234 as _);
        check!(TlsGetValue(idx) as usize == 0x1234, "TlsAlloc / TlsSetValue / TlsGetValue");
        check!(*((teb + 0x1480 + idx as usize * 8) as *const usize) == 0x1234, "valeur TLS visible dans les slots du TEB");
        check!(TlsFree(idx) != 0 && TlsGetValue(idx).is_null() && *((teb + 0x1480 + idx as usize * 8) as *const usize) == 0, "TlsFree invalide les anciennes valeurs");
        let n = GetEnvironmentVariableA(c"WINLUX_TEST".as_ptr(), b, 256);
        check!(n == 5 && cstr_eq(b, b"salut"), "GetEnvironmentVariableA");
        let env_name_w = [b'W' as u16,b'I' as u16,b'N' as u16,b'L' as u16,b'U' as u16,b'X' as u16,b'_' as u16,b'W' as u16,b'_' as u16,b'T' as u16,b'E' as u16,b'S' as u16,b'T' as u16,0];
        let env_value_w = [b'o' as u16,b'k' as u16,0];
        let mut env_out_w = [0u16;8];
        SetEnvironmentVariableW(env_name_w.as_ptr(), env_value_w.as_ptr());
        check!(GetEnvironmentVariableW(env_name_w.as_ptr(), env_out_w.as_mut_ptr(), 8) == 2 && env_out_w[0] == b'o' as u16 && env_out_w[1] == b'k' as u16, "variables d'environnement W");
        SetEnvironmentVariableW(env_name_w.as_ptr(), null_mut());
        let cl = GetCommandLineA();
        check!(strlen(cl) > 0, "GetCommandLineA");
        let clw = GetCommandLineW();
        let mut clw_len = 0usize;
        while *clw.add(clw_len) != 0 { clw_len += 1; }
        check!(clw_len > 0, "GetCommandLineW UTF-16 terminée");

        // --- temps ---
        let (mut c1, mut freq) = (0i64, 0i64);
        QueryPerformanceFrequency(&mut freq);
        QueryPerformanceCounter(&mut c1);
        check!(freq > 0 && c1 > 0, "QueryPerformanceCounter / Frequency");

        // --- Winsock relié aux sockets POSIX ---
        let mut wsa_data = [0u8; 408];
        let wsa_started = WSAStartup(0x0202, wsa_data.as_mut_ptr()) == 0;
        let sock = if wsa_started { socket(2, 2, 17) } else { usize::MAX };
        let ip = inet_addr(c"127.0.0.1".as_ptr());
        check!(wsa_started && u16::from_le_bytes([wsa_data[0], wsa_data[1]]) == 0x0202 && sock != usize::MAX && ip.to_ne_bytes() == [127, 0, 0, 1], "WSAStartup/socket/inet_addr");
        let socket_closed = sock != usize::MAX && closesocket(sock) == 0;
        let mut addr_hints: AddrInfoA = core::mem::zeroed();
        addr_hints.family = 2;
        addr_hints.socktype = 1;
        let mut addresses = null_mut();
        let resolved = getaddrinfo(c"127.0.0.1".as_ptr(), c"80".as_ptr(), &addr_hints, &mut addresses);
        let addr_ok = resolved == 0 && !addresses.is_null() && (*addresses).family == 2 && (*addresses).addrlen >= 16;
        if !addresses.is_null() { freeaddrinfo(addresses); }
        check!(addr_ok, "getaddrinfo/freeaddrinfo vers le résolveur Linux");
        let node_w = [b'1' as u16,b'2' as u16,b'7' as u16,b'.' as u16,b'0' as u16,b'.' as u16,b'0' as u16,b'.' as u16,b'1' as u16,0];
        let service_w = [b'8' as u16,b'0' as u16,0];
        let mut hints_w: AddrInfoW = core::mem::zeroed();hints_w.family=2;hints_w.socktype=1;
        let mut result_w = null_mut();
        let resolved_w = GetAddrInfoW(node_w.as_ptr(),service_w.as_ptr(),&hints_w,&mut result_w)==0&&!result_w.is_null()&&(*result_w).family==2;
        if !result_w.is_null(){FreeAddrInfoW(result_w);}
        check!(resolved_w,"GetAddrInfoW/FreeAddrInfoW résout une adresse numérique");
        let server=socket(2,1,6);
        let mut server_addr=SockAddrIn{family:2,port:0,address:ip,zero:[0;8]};
        let mut server_len=16;
        let mut tcp_ok=server!=usize::MAX&&bind(server,&server_addr as *const _ as *const u8,16)==0&&listen(server,1)==0
            &&getsockname(server,&mut server_addr as *mut _ as *mut u8,&mut server_len)==0;
        let client=if tcp_ok{socket(2,1,6)}else{usize::MAX};
        tcp_ok &= client!=usize::MAX&&connect(client,&server_addr as *const _ as *const u8,16)==0;
        let accepted=if tcp_ok{accept(server,null_mut(),null_mut())}else{usize::MAX};
        let payload=*b"ping";tcp_ok &= accepted!=usize::MAX&&send(client,payload.as_ptr(),4,0)==4;
        let mut received=[0u8;4];tcp_ok &= accepted!=usize::MAX&&recv(accepted,received.as_mut_ptr(),4,0)==4&&received==payload;
        if accepted!=usize::MAX{closesocket(accepted);}if client!=usize::MAX{closesocket(client);}if server!=usize::MAX{closesocket(server);}
        check!(tcp_ok,"bind/listen/connect/accept/send/recv TCP loopback");
        check!(socket_closed && WSACleanup() == 0, "closesocket/WSACleanup");
        let invalid_socket = socket(2, 2, 17) == usize::MAX && WSAGetLastError() == 10093;
        check!(invalid_socket, "WSAGetLastError après WSACleanup");

        // --- Unicode ---
        let mut w = [0u16; 32];
        let src = "Été à Noël\n".as_bytes();
        let wn = MultiByteToWideChar(65001, 0, src.as_ptr(), src.len() as i32, w.as_mut_ptr(), 32);
        check!(wn == 11 && w[0] == 0xC9, "MultiByteToWideChar UTF-8");
        let cp1252 = [0x80u8];
        let mut cp_wide = [0u16; 4];
        check!(MultiByteToWideChar(1252, 0, cp1252.as_ptr(), 1, cp_wide.as_mut_ptr(), 4) == 1 && cp_wide[0] == 0x20ac, "conversion de codepage Windows-1252");
        let mut back = [0u8; 4];
        check!(WideCharToMultiByte(1252, 0, cp_wide.as_ptr(), 1, back.as_mut_ptr(), 4, null_mut(), null_mut()) == 1 && back[0] == 0x80, "conversion Unicode vers Windows-1252");
        let mut wr = 0u32;
        out(b"  (texte UTF-16 attendu : Ete accentue) ");
        WriteConsoleW(GetStdHandle(STD_OUTPUT_HANDLE), w.as_ptr(), wn as u32, &mut wr, null_mut());
        check!(wr == 11, "WriteConsoleW");

        if FAILURES == 0 { puts(c"Tous les tests P1 passent.".as_ptr()); }
        else { printf(c"%u test(s) en echec.\n".as_ptr(), FAILURES); }
    }
    finish()
}
