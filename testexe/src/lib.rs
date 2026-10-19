//! Bibliothèque commune des .exe de test Winlux.
//!
//! Chaque test est un vrai programme Windows : il n'utilise que des
//! fonctions importées de kernel32.dll et msvcrt.dll, exactement comme un
//! programme compilé avec MinGW ou MSVC.
#![no_std]

use core::ffi::{c_char, c_void};

pub type Handle = *mut c_void;

#[repr(C)]
pub struct SystemInfo {
    pub processor_architecture: u16,
    pub reserved: u16,
    pub page_size: u32,
    pub minimum_application_address: *mut c_void,
    pub maximum_application_address: *mut c_void,
    pub active_processor_mask: usize,
    pub number_of_processors: u32,
    pub processor_type: u32,
    pub allocation_granularity: u32,
    pub processor_level: u16,
    pub processor_revision: u16,
}

#[repr(C)]
pub struct OsVersionInfoA {
    pub size: u32,
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub platform: u32,
    pub service_pack: [u8; 128],
}

#[repr(C)]
pub struct OsVersionInfoW {
    pub size: u32,
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub platform: u32,
    pub service_pack: [u16; 128],
}

#[repr(C)]
pub struct MemoryBasicInformation {
    pub base_address: *mut c_void,
    pub allocation_base: *mut c_void,
    pub allocation_protect: u32,
    pub partition_id: u16,
    pub region_size: usize,
    pub state: u32,
    pub protect: u32,
    pub kind: u32,
}

#[repr(C)]
pub struct StartupInfoA {
    pub cb: u32, pub reserved: *mut c_char, pub desktop: *mut c_char, pub title: *mut c_char,
    pub x: u32, pub y: u32, pub x_size: u32, pub y_size: u32, pub x_chars: u32, pub y_chars: u32,
    pub fill_attribute: u32, pub flags: u32, pub show_window: u16, pub reserved2_size: u16,
    pub reserved2: *mut u8, pub std_input: Handle, pub std_output: Handle, pub std_error: Handle,
}
#[repr(C)]
pub struct ProcessInformation { pub process: Handle, pub thread: Handle, pub process_id: u32, pub thread_id: u32 }
#[repr(C)]
pub struct Overlapped { pub internal: usize, pub internal_high: usize, pub offset: u32, pub offset_high: u32, pub event: usize }

#[repr(C)]
pub struct AddrInfoA {
    pub flags: i32,
    pub family: i32,
    pub socktype: i32,
    pub protocol: i32,
    pub addrlen: usize,
    pub canonname: *mut c_char,
    pub addr: *mut u8,
    pub next: *mut AddrInfoA,
}

#[repr(C)]
pub struct AddrInfoW {
    pub flags: i32,
    pub family: i32,
    pub socktype: i32,
    pub protocol: i32,
    pub addrlen: usize,
    pub canonname: *mut u16,
    pub addr: *mut u8,
    pub next: *mut AddrInfoW,
}

#[repr(C)]
pub struct SockAddrIn {
    pub family: u16,
    pub port: u16,
    pub address: u32,
    pub zero: [u8; 8],
}

pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn GetStdHandle(which: u32) -> Handle;
    pub fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, ov: *mut c_void) -> i32;
    pub fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, ov: *mut c_void) -> i32;
    pub fn CreateFileA(path: *const c_char, access: u32, share: u32, sec: *mut c_void, creation: u32, flags: u32, template: Handle) -> Handle;
    pub fn CreateFileW(path: *const u16, access: u32, share: u32, sec: *mut c_void, creation: u32, flags: u32, template: Handle) -> Handle;
    pub fn GetFileSize(h: Handle, high: *mut u32) -> u32;
    pub fn SetFilePointer(h: Handle, distance: i32, high: *mut i32, method: u32) -> u32;
    pub fn FlushFileBuffers(h: Handle) -> i32;
    pub fn GetOverlappedResult(h: Handle, overlapped: *mut Overlapped, transferred: *mut u32, wait: i32) -> i32;
    pub fn CreateIoCompletionPort(file: Handle, existing_port: Handle, completion_key: usize, concurrent_threads: u32) -> Handle;
    pub fn GetQueuedCompletionStatus(port: Handle, bytes: *mut u32, key: *mut usize, overlapped: *mut *mut c_void, timeout: u32) -> i32;
    pub fn PostQueuedCompletionStatus(port: Handle, bytes: u32, key: usize, overlapped: *mut c_void) -> i32;
    pub fn CloseHandle(h: Handle) -> i32;
    pub fn CreateEventA(sec: *mut c_void, manual: i32, initial: i32, name: *const c_char) -> Handle;
    pub fn SetEvent(h: Handle) -> i32;
    pub fn ResetEvent(h: Handle) -> i32;
    pub fn CreateMutexA(sec: *mut c_void, initial_owner: i32, name: *const c_char) -> Handle;
    pub fn ReleaseMutex(h: Handle) -> i32;
    pub fn WaitForSingleObject(h: Handle, timeout: u32) -> u32;
    pub fn WaitForMultipleObjects(count: u32, handles: *const Handle, wait_all: i32, timeout: u32) -> u32;
    pub fn InitializeCriticalSection(cs: *mut c_void);
    pub fn InitializeCriticalSectionAndSpinCount(cs: *mut c_void, spin: u32) -> i32;
    pub fn EnterCriticalSection(cs: *mut c_void);
    pub fn TryEnterCriticalSection(cs: *mut c_void) -> i32;
    pub fn LeaveCriticalSection(cs: *mut c_void);
    pub fn DeleteCriticalSection(cs: *mut c_void);
    pub fn CreateThread(attrs: *mut c_void, stack: usize, start: extern "system" fn(*mut c_void) -> u32, param: *mut c_void, flags: u32, id: *mut u32) -> Handle;
    pub fn GetExitCodeThread(h: Handle, code: *mut u32) -> i32;
    pub fn ResumeThread(h: Handle) -> u32;
    pub fn DeleteFileA(path: *const c_char) -> i32;
    pub fn DeleteFileW(path: *const u16) -> i32;
    pub fn CopyFileA(src: *const c_char, dst: *const c_char, fail_if_exists: i32) -> i32;
    pub fn MoveFileA(src: *const c_char, dst: *const c_char) -> i32;
    pub fn CreateDirectoryA(path: *const c_char, sec: *mut c_void) -> i32;
    pub fn RemoveDirectoryA(path: *const c_char) -> i32;
    pub fn GetFullPathNameA(path: *const c_char, size: u32, out: *mut c_char, file_part: *mut *mut c_char) -> u32;
    pub fn GetFullPathNameW(path: *const u16, size: u32, out: *mut u16, file_part: *mut *mut u16) -> u32;
    pub fn ExitProcess(code: u32) -> !;
    pub fn CreateProcessA(application: *const c_char, command_line: *const c_char, process_attributes: *mut c_void, thread_attributes: *mut c_void, inherit: i32, flags: u32, environment: *const c_void, current_directory: *const c_char, startup: *const StartupInfoA, info: *mut ProcessInformation) -> i32;
    pub fn CreateProcessW(application: *const u16, command_line: *mut u16, process_attributes: *mut c_void, thread_attributes: *mut c_void, inherit: i32, flags: u32, environment: *const c_void, current_directory: *const u16, startup: *const StartupInfoA, info: *mut ProcessInformation) -> i32;
    pub fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
    pub fn GetCommandLineA() -> *const c_char;
    pub fn GetCommandLineW() -> *const u16;
    pub fn GetModuleHandleA(name: *const c_char) -> Handle;
    pub fn GetProcAddress(module: Handle, name: *const c_char) -> *const c_void;
    pub fn LoadLibraryA(name: *const c_char) -> Handle;
    pub fn FreeLibrary(module: Handle) -> i32;
    pub fn GetLastError() -> u32;
    pub fn SetLastError(code: u32);
    pub fn GetProcessHeap() -> Handle;
    pub fn HeapAlloc(heap: Handle, flags: u32, size: usize) -> *mut c_void;
    pub fn HeapReAlloc(heap: Handle, flags: u32, p: *mut c_void, size: usize) -> *mut c_void;
    pub fn HeapFree(heap: Handle, flags: u32, p: *mut c_void) -> i32;
    pub fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, prot: u32) -> *mut c_void;
    pub fn VirtualFree(addr: *mut c_void, size: usize, kind: u32) -> i32;
    pub fn VirtualProtect(addr: *mut c_void, size: usize, prot: u32, old: *mut u32) -> i32;
    pub fn VirtualQuery(addr: *const c_void, info: *mut MemoryBasicInformation, len: usize) -> usize;
    pub fn GetTickCount() -> u32;
    pub fn QueryPerformanceCounter(out: *mut i64) -> i32;
    pub fn QueryPerformanceFrequency(out: *mut i64) -> i32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentProcess() -> Handle;
    pub fn GetCurrentThread() -> Handle;
    pub fn GetCurrentThreadId() -> u32;
    pub fn GetSystemInfo(out: *mut SystemInfo);
    pub fn GetVersion() -> u32;
    pub fn GetVersionExA(out: *mut OsVersionInfoA) -> i32;
    pub fn GetTickCount64() -> u64;
    pub fn GetSystemTimeAsFileTime(out: *mut u64);
    pub fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
    pub fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, size: u32) -> u32;
    pub fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    pub fn TlsAlloc() -> u32;
    pub fn TlsFree(idx: u32) -> i32;
    pub fn TlsSetValue(idx: u32, v: *mut c_void) -> i32;
    pub fn TlsGetValue(idx: u32) -> *mut c_void;
    pub fn MultiByteToWideChar(cp: u32, flags: u32, s: *const u8, n: i32, w: *mut u16, wn: i32) -> i32;
    pub fn WideCharToMultiByte(cp: u32, flags: u32, w: *const u16, n: i32, out: *mut u8, cap: i32, default: *const u8, used: *mut i32) -> i32;
    pub fn WriteConsoleW(h: Handle, buf: *const u16, n: u32, written: *mut u32, r: *mut c_void) -> i32;
}

#[link(name = "ws2_32", kind = "raw-dylib")]
extern "system" {
    pub fn WSAStartup(version: u16, data: *mut u8) -> i32;
    pub fn WSACleanup() -> i32;
    pub fn WSAGetLastError() -> i32;
    pub fn socket(af: i32, kind: i32, protocol: i32) -> usize;
    pub fn closesocket(s: usize) -> i32;
    pub fn inet_addr(address: *const c_char) -> u32;
    pub fn bind(s: usize, address: *const u8, len: i32) -> i32;
    pub fn listen(s: usize, backlog: i32) -> i32;
    pub fn accept(s: usize, address: *mut u8, len: *mut i32) -> usize;
    pub fn connect(s: usize, address: *const u8, len: i32) -> i32;
    pub fn getsockname(s: usize, address: *mut u8, len: *mut i32) -> i32;
    pub fn send(s: usize, data: *const u8, len: i32, flags: i32) -> i32;
    pub fn recv(s: usize, data: *mut u8, len: i32, flags: i32) -> i32;
    pub fn getaddrinfo(node: *const c_char, service: *const c_char, hints: *const AddrInfoA, result: *mut *mut AddrInfoA) -> i32;
    pub fn freeaddrinfo(result: *mut AddrInfoA);
    pub fn GetAddrInfoW(node: *const u16, service: *const u16, hints: *const AddrInfoW, result: *mut *mut AddrInfoW) -> i32;
    pub fn FreeAddrInfoW(result: *mut AddrInfoW);
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    pub fn RegCreateKeyExA(root: Handle, sub: *const c_char, reserved: u32, class: *const c_char, options: u32, access: u32, sec: *mut c_void, result: *mut Handle, disposition: *mut u32) -> i32;
    pub fn RegSetValueExA(key: Handle, name: *const c_char, reserved: u32, kind: u32, data: *const u8, size: u32) -> i32;
    pub fn RegQueryValueExA(key: Handle, name: *const c_char, reserved: *mut u32, kind: *mut u32, data: *mut u8, size: *mut u32) -> i32;
    pub fn RegDeleteValueA(key: Handle, name: *const c_char) -> i32;
    pub fn RegDeleteKeyA(root: Handle, sub: *const c_char) -> i32;
    pub fn RegCloseKey(key: Handle) -> i32;
}

#[link(name = "msvcrt", kind = "raw-dylib")]
extern "C" {
    pub fn printf(fmt: *const c_char, ...) -> i32;
    pub fn sprintf(buf: *mut c_char, fmt: *const c_char, ...) -> i32;
    pub fn puts(s: *const c_char) -> i32;
    pub fn malloc(n: usize) -> *mut c_void;
    pub fn free(p: *mut c_void);
    pub fn strlen(s: *const c_char) -> usize;
    pub fn strcmp(a: *const c_char, b: *const c_char) -> i32;
    pub fn memcpy(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void;
    pub fn memset(d: *mut c_void, c: i32, n: usize) -> *mut c_void;
    pub fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32;
    pub fn memmove(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void;
    pub fn atoi(s: *const c_char) -> i32;
    pub fn exit(code: i32) -> !;
}

/// Requis par le compilateur dès qu'un programme Windows manipule des flottants.
#[no_mangle]
#[used]
pub static _fltused: i32 = 0;

// Fourni uniquement pour permettre à l'éditeur de liens de produire un
// répertoire d'imports différés sans dépendre de delayimp.lib de Microsoft.
// Winlux résout ces imports avant le point d'entrée.
#[no_mangle]
pub extern "C" fn __delayLoadHelper2(_: *const c_void, _: *mut c_void) -> *mut c_void { core::ptr::null_mut() }

/// Écrit des octets bruts sur la sortie standard via WriteFile.
pub fn out(s: &[u8]) {
    unsafe {
        let mut n = 0u32;
        WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), s.as_ptr(), s.len() as u32, &mut n, core::ptr::null_mut());
    }
}

/// Compteur d'échecs pour les tests auto-vérifiants.
pub static mut FAILURES: u32 = 0;

/// Vérifie une condition et affiche OK/ECHEC. Termine avec le nombre d'échecs.
#[macro_export]
macro_rules! check {
    ($cond:expr, $name:expr) => {{
        if $cond {
            $crate::out(b"  OK     ");
        } else {
            $crate::out(b"  ECHEC  ");
            unsafe { $crate::FAILURES += 1 };
        }
        $crate::out($name.as_bytes());
        $crate::out(b"\n");
    }};
}

pub fn finish() -> ! {
    unsafe {
        let f = FAILURES;
        ExitProcess(f)
    }
}

/// Compare une chaîne C produite par le programme à une chaîne attendue.
pub fn cstr_eq(p: *const c_char, expected: &[u8]) -> bool {
    unsafe {
        let n = strlen(p);
        n == expected.len() && core::slice::from_raw_parts(p as *const u8, n) == expected
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    out(b"panic\n");
    unsafe { ExitProcess(101) }
}
