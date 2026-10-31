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

/// `SYSTEM_POWER_STATUS` (GetSystemPowerStatus).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SystemPowerStatus {
    pub ac_line_status: u8,
    pub battery_flag: u8,
    pub battery_life_percent: u8,
    pub system_status_flag: u8,
    pub battery_life_time: u32,
    pub battery_full_life_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
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
    pub cb: u32,
    pub reserved: *mut c_char,
    pub desktop: *mut c_char,
    pub title: *mut c_char,
    pub x: u32,
    pub y: u32,
    pub x_size: u32,
    pub y_size: u32,
    pub x_chars: u32,
    pub y_chars: u32,
    pub fill_attribute: u32,
    pub flags: u32,
    pub show_window: u16,
    pub reserved2_size: u16,
    pub reserved2: *mut u8,
    pub std_input: Handle,
    pub std_output: Handle,
    pub std_error: Handle,
}
#[repr(C)]
pub struct ProcessInformation {
    pub process: Handle,
    pub thread: Handle,
    pub process_id: u32,
    pub thread_id: u32,
}
#[repr(C)]
pub struct Overlapped {
    pub internal: usize,
    pub internal_high: usize,
    pub offset: u32,
    pub offset_high: u32,
    pub event: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OverlappedEntry {
    pub completion_key: usize,
    pub overlapped: *mut c_void,
    pub internal: usize,
    pub bytes_transferred: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct Win32FindDataA {
    pub attributes: u32,
    pub creation_time: u64,
    pub access_time: u64,
    pub write_time: u64,
    pub size_high: u32,
    pub size_low: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub file_name: [c_char; 260],
    pub alternate_name: [c_char; 14],
}
#[repr(C)]
pub struct Win32FindDataW {
    pub attributes: u32,
    pub creation_time: u64,
    pub access_time: u64,
    pub write_time: u64,
    pub size_high: u32,
    pub size_low: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub file_name: [u16; 260],
    pub alternate_name: [u16; 14],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileTime {
    pub low: u32,
    pub high: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StatStg {
    pub name: *mut u16,
    pub kind: u32,
    pub size: u64,
    pub modified: FileTime,
    pub created: FileTime,
    pub accessed: FileTime,
    pub mode: u32,
    pub locks_supported: u32,
    pub clsid: Guid,
    pub state_bits: u32,
    pub reserved: u32,
}
#[repr(C)]
pub struct IStreamVTable {
    pub query_interface: unsafe extern "system" fn(Handle, *const Guid, *mut Handle) -> i32,
    pub add_ref: unsafe extern "system" fn(Handle) -> u32,
    pub release: unsafe extern "system" fn(Handle) -> u32,
    pub read: unsafe extern "system" fn(Handle, *mut c_void, u32, *mut u32) -> i32,
    pub write: unsafe extern "system" fn(Handle, *const c_void, u32, *mut u32) -> i32,
    pub seek: unsafe extern "system" fn(Handle, i64, u32, *mut u64) -> i32,
    pub set_size: unsafe extern "system" fn(Handle, u64) -> i32,
    pub copy_to: unsafe extern "system" fn(Handle, Handle, u64, *mut u64, *mut u64) -> i32,
    pub commit: unsafe extern "system" fn(Handle, u32) -> i32,
    pub revert: unsafe extern "system" fn(Handle) -> i32,
    pub lock_region: unsafe extern "system" fn(Handle, u64, u64, u32) -> i32,
    pub unlock_region: unsafe extern "system" fn(Handle, u64, u64, u32) -> i32,
    pub stat: unsafe extern "system" fn(Handle, *mut StatStg, u32) -> i32,
    pub clone_stream: unsafe extern "system" fn(Handle, *mut Handle) -> i32,
}
#[repr(C)]
pub struct Win32FileAttributeData {
    pub attributes: u32,
    pub creation_time: FileTime,
    pub access_time: FileTime,
    pub write_time: FileTime,
    pub size_high: u32,
    pub size_low: u32,
}

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct Msg {
    pub hwnd: Handle,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: Point,
    pub l_private: u32,
}

#[repr(C)]
pub struct WndClassExA {
    pub size: u32,
    pub style: u32,
    pub wnd_proc: usize,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: Handle,
    pub icon: Handle,
    pub cursor: Handle,
    pub background: Handle,
    pub menu_name: *const c_char,
    pub class_name: *const c_char,
    pub icon_sm: Handle,
}

#[repr(C)]
pub struct PaintStruct {
    pub hdc: Handle,
    pub erase: i32,
    pub paint: Rect,
    pub restore: i32,
    pub inc_update: i32,
    pub reserved: [u8; 32],
}

pub type MSG = Msg;
pub type PAINTSTRUCT = PaintStruct;

#[repr(C)]
#[allow(non_snake_case)]
pub struct WNDCLASSA {
    pub style: u32,
    pub lpfnWndProc: Option<unsafe extern "system" fn(Handle, u32, usize, isize) -> isize>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: Handle,
    pub hIcon: Handle,
    pub hCursor: Handle,
    pub hbrBackground: usize,
    pub lpszMenuName: *const c_char,
    pub lpszClassName: *const c_char,
}

#[allow(non_snake_case)]
pub unsafe fn RegisterClassA(wc: *const WNDCLASSA) -> u16 {
    let wc = &*wc;
    let ex = WndClassExA {
        size: core::mem::size_of::<WndClassExA>() as u32,
        style: wc.style,
        wnd_proc: match wc.lpfnWndProc {
            Some(f) => f as *const () as usize,
            None => 0,
        },
        cls_extra: wc.cbClsExtra,
        wnd_extra: wc.cbWndExtra,
        instance: wc.hInstance,
        icon: wc.hIcon,
        cursor: wc.hCursor,
        background: wc.hbrBackground as *mut c_void,
        menu_name: wc.lpszMenuName,
        class_name: wc.lpszClassName,
        icon_sm: core::ptr::null_mut(),
    };
    RegisterClassExA(&ex)
}

#[allow(non_snake_case)]
pub unsafe fn LoadCursorA(_instance: Handle, _id: usize) -> Handle {
    core::ptr::null_mut()
}

#[repr(C)]
pub struct WaveFormatEx {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub cb_size: u16,
}

#[repr(C)]
pub struct WaveHdr {
    pub data: *mut u8,
    pub buffer_length: u32,
    pub bytes_recorded: u32,
    pub user: usize,
    pub flags: u32,
    pub loops: u32,
    pub next: *mut WaveHdr,
    pub reserved: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BitmapInfoHeader {
    pub size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bit_count: u16,
    pub compression: u32,
    pub size_image: u32,
    pub x_pels_per_meter: i32,
    pub y_pels_per_meter: i32,
    pub colors_used: u32,
    pub colors_important: u32,
}

#[repr(C)]
pub struct BitmapObject {
    pub bm_type: i32,
    pub bm_width: i32,
    pub bm_height: i32,
    pub bm_width_bytes: i32,
    pub bm_planes: u16,
    pub bm_bits_pixel: u16,
    pub bm_bits: *mut c_void,
}

#[repr(C)]
pub struct LogFontW {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strikeout: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
    pub face: [u16; 32],
}

pub const WM_CREATE: u32 = 0x0001;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_MOVE: u32 = 0x0003;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_SETFOCUS: u32 = 0x0007;
pub const WM_KILLFOCUS: u32 = 0x0008;
pub const WM_CHAR: u32 = 0x0102;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONDOWN: u32 = 0x0204;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MBUTTONDOWN: u32 = 0x0207;
pub const WM_MOUSEWHEEL: u32 = 0x020A;
pub const VK_A: i32 = 0x41;
pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SW_SHOW: i32 = 5;
pub const SW_SHOWNORMAL: i32 = 1;

pub const IDC_ARROW: usize = 32512;
pub const IDC_IBEAM: usize = 32513;
pub const IDC_SIZENWSE: usize = 32642;
pub const IDC_SIZENESW: usize = 32643;
pub const IDC_SIZEWE: usize = 32644;
pub const IDC_SIZENS: usize = 32645;
pub const IDC_SIZEALL: usize = 32646;
pub const IDC_HAND: usize = 32649;

pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;

/// Informations d'une image ouverte par `WinluxImageOpen`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinluxImageInfoC {
    pub width: u32,
    pub height: u32,
    pub pages: u32,
    /// 1 animée, 2 calques, 4 RAW, 8 aperçu intégré, 16 HDR, 32 vectoriel, 64 modèle 3D.
    pub flags: u32,
    pub delay_ms: u32,
    pub meta_count: u32,
    pub format: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinluxPageInfoC {
    pub width: u32,
    pub height: u32,
    pub delay_ms: u32,
    pub kind: u32,
    pub name: [u8; 64],
}

/// Vue demandée à `WinluxImageRender` (centre en coordonnées image).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinluxViewC {
    pub zoom: f32,
    pub cx: f32,
    pub cy: f32,
    pub rot: u32,
    pub mirror: u32,
    pub yaw: f32,
    pub pitch: f32,
    pub exposure: f32,
    pub auto_levels: u32,
    pub split: f32,
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn GetStdHandle(which: u32) -> Handle;
    pub fn WriteFile(
        h: Handle,
        buf: *const u8,
        len: u32,
        written: *mut u32,
        ov: *mut c_void,
    ) -> i32;
    pub fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, ov: *mut c_void) -> i32;
    pub fn CreateFileA(
        path: *const c_char,
        access: u32,
        share: u32,
        sec: *mut c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    pub fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        sec: *mut c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    pub fn GetFileSize(h: Handle, high: *mut u32) -> u32;
    pub fn SetEndOfFile(h: Handle) -> i32;
    pub fn GetFileAttributesA(path: *const c_char) -> u32;
    pub fn GetFileAttributesW(path: *const u16) -> u32;
    pub fn GetFileAttributesExA(
        path: *const c_char,
        level: u32,
        data: *mut Win32FileAttributeData,
    ) -> i32;
    pub fn GetFileAttributesExW(
        path: *const u16,
        level: u32,
        data: *mut Win32FileAttributeData,
    ) -> i32;
    pub fn GetTempFileNameA(
        path: *const c_char,
        prefix: *const c_char,
        unique: u32,
        output: *mut c_char,
    ) -> u32;
    pub fn GetTempFileNameW(
        path: *const u16,
        prefix: *const u16,
        unique: u32,
        output: *mut u16,
    ) -> u32;
    pub fn SetFilePointer(h: Handle, distance: i32, high: *mut i32, method: u32) -> u32;
    pub fn FlushFileBuffers(h: Handle) -> i32;
    pub fn GetOverlappedResult(
        h: Handle,
        overlapped: *mut Overlapped,
        transferred: *mut u32,
        wait: i32,
    ) -> i32;
    pub fn CreateIoCompletionPort(
        file: Handle,
        existing_port: Handle,
        completion_key: usize,
        concurrent_threads: u32,
    ) -> Handle;
    pub fn GetQueuedCompletionStatus(
        port: Handle,
        bytes: *mut u32,
        key: *mut usize,
        overlapped: *mut *mut c_void,
        timeout: u32,
    ) -> i32;
    pub fn GetQueuedCompletionStatusEx(
        port: Handle,
        entries: *mut OverlappedEntry,
        count: u32,
        removed: *mut u32,
        timeout: u32,
        alertable: i32,
    ) -> i32;
    pub fn PostQueuedCompletionStatus(
        port: Handle,
        bytes: u32,
        key: usize,
        overlapped: *mut c_void,
    ) -> i32;
    pub fn CloseHandle(h: Handle) -> i32;
    pub fn CreateEventA(sec: *mut c_void, manual: i32, initial: i32, name: *const c_char)
        -> Handle;
    pub fn SetEvent(h: Handle) -> i32;
    pub fn ResetEvent(h: Handle) -> i32;
    pub fn CreateMutexA(sec: *mut c_void, initial_owner: i32, name: *const c_char) -> Handle;
    pub fn ReleaseMutex(h: Handle) -> i32;
    pub fn WaitForSingleObject(h: Handle, timeout: u32) -> u32;
    pub fn WaitForMultipleObjects(
        count: u32,
        handles: *const Handle,
        wait_all: i32,
        timeout: u32,
    ) -> u32;
    pub fn InitializeCriticalSection(cs: *mut c_void);
    pub fn InitializeCriticalSectionAndSpinCount(cs: *mut c_void, spin: u32) -> i32;
    pub fn EnterCriticalSection(cs: *mut c_void);
    pub fn TryEnterCriticalSection(cs: *mut c_void) -> i32;
    pub fn LeaveCriticalSection(cs: *mut c_void);
    pub fn DeleteCriticalSection(cs: *mut c_void);
    pub fn CreateThread(
        attrs: *mut c_void,
        stack: usize,
        start: extern "system" fn(*mut c_void) -> u32,
        param: *mut c_void,
        flags: u32,
        id: *mut u32,
    ) -> Handle;
    pub fn GetExitCodeThread(h: Handle, code: *mut u32) -> i32;
    pub fn ResumeThread(h: Handle) -> u32;
    pub fn DeleteFileA(path: *const c_char) -> i32;
    pub fn DeleteFileW(path: *const u16) -> i32;
    pub fn CopyFileA(src: *const c_char, dst: *const c_char, fail_if_exists: i32) -> i32;
    pub fn MoveFileA(src: *const c_char, dst: *const c_char) -> i32;
    pub fn MoveFileW(src: *const u16, dst: *const u16) -> i32;
    pub fn CreateDirectoryA(path: *const c_char, sec: *mut c_void) -> i32;
    pub fn CreateDirectoryW(path: *const u16, sec: *mut c_void) -> i32;
    pub fn RemoveDirectoryA(path: *const c_char) -> i32;
    pub fn RemoveDirectoryW(path: *const u16) -> i32;
    pub fn FindFirstFileA(path: *const c_char, data: *mut Win32FindDataA) -> Handle;
    pub fn FindNextFileA(search: Handle, data: *mut Win32FindDataA) -> i32;
    pub fn FindFirstFileW(path: *const u16, data: *mut Win32FindDataW) -> Handle;
    pub fn FindNextFileW(search: Handle, data: *mut Win32FindDataW) -> i32;
    pub fn FindClose(search: Handle) -> i32;
    pub fn GetFullPathNameA(
        path: *const c_char,
        size: u32,
        out: *mut c_char,
        file_part: *mut *mut c_char,
    ) -> u32;
    pub fn GetFullPathNameW(
        path: *const u16,
        size: u32,
        out: *mut u16,
        file_part: *mut *mut u16,
    ) -> u32;
    pub fn ExitProcess(code: u32) -> !;
    pub fn CreatePipe(
        read: *mut Handle,
        write: *mut Handle,
        attributes: *mut c_void,
        size: u32,
    ) -> i32;
    pub fn PeekNamedPipe(
        h: Handle,
        buf: *mut u8,
        size: u32,
        read: *mut u32,
        total: *mut u32,
        left: *mut u32,
    ) -> i32;
    pub fn CreateProcessA(
        application: *const c_char,
        command_line: *const c_char,
        process_attributes: *mut c_void,
        thread_attributes: *mut c_void,
        inherit: i32,
        flags: u32,
        environment: *const c_void,
        current_directory: *const c_char,
        startup: *const StartupInfoA,
        info: *mut ProcessInformation,
    ) -> i32;
    pub fn CreateProcessW(
        application: *const u16,
        command_line: *mut u16,
        process_attributes: *mut c_void,
        thread_attributes: *mut c_void,
        inherit: i32,
        flags: u32,
        environment: *const c_void,
        current_directory: *const u16,
        startup: *const StartupInfoA,
        info: *mut ProcessInformation,
    ) -> i32;
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
    pub fn GlobalAlloc(flags: u32, size: usize) -> Handle;
    pub fn GlobalReAlloc(memory: Handle, size: usize, flags: u32) -> Handle;
    pub fn GlobalLock(memory: Handle) -> *mut c_void;
    pub fn GlobalUnlock(memory: Handle) -> i32;
    pub fn GlobalSize(memory: Handle) -> usize;
    pub fn GlobalFree(memory: Handle) -> Handle;
    pub fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, prot: u32) -> *mut c_void;
    pub fn VirtualFree(addr: *mut c_void, size: usize, kind: u32) -> i32;
    pub fn VirtualProtect(addr: *mut c_void, size: usize, prot: u32, old: *mut u32) -> i32;
    pub fn VirtualQuery(
        addr: *const c_void,
        info: *mut MemoryBasicInformation,
        len: usize,
    ) -> usize;
    pub fn GetTickCount() -> u32;
    pub fn GetSystemPowerStatus(out: *mut SystemPowerStatus) -> i32;
    pub fn SetThreadExecutionState(flags: u32) -> u32;
    pub fn PowerCreateRequest(reason: *const c_void) -> Handle;
    pub fn PowerSetRequest(request: Handle, kind: u32) -> i32;
    pub fn PowerClearRequest(request: Handle, kind: u32) -> i32;
    pub fn LocalFree(p: *mut c_void) -> *mut c_void;
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
    pub fn GetLocalTime(out: *mut SystemTime);
    pub fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
    pub fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, size: u32) -> u32;
    pub fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    pub fn SetEnvironmentVariableA(name: *const c_char, value: *const c_char) -> i32;
    pub fn TlsAlloc() -> u32;
    pub fn TlsFree(idx: u32) -> i32;
    pub fn TlsSetValue(idx: u32, v: *mut c_void) -> i32;
    pub fn TlsGetValue(idx: u32) -> *mut c_void;
    pub fn MultiByteToWideChar(
        cp: u32,
        flags: u32,
        s: *const u8,
        n: i32,
        w: *mut u16,
        wn: i32,
    ) -> i32;
    pub fn WideCharToMultiByte(
        cp: u32,
        flags: u32,
        w: *const u16,
        n: i32,
        out: *mut u8,
        cap: i32,
        default: *const u8,
        used: *mut i32,
    ) -> i32;
    pub fn WriteConsoleW(
        h: Handle,
        buf: *const u16,
        n: u32,
        written: *mut u32,
        r: *mut c_void,
    ) -> i32;
    pub fn Sleep(ms: u32);
    pub fn WinluxCreatePty(cols: u32, rows: u32) -> usize;
    pub fn WinluxReadPty(handle: usize, buf: *mut u8, len: u32, read: *mut u32) -> i32;
    pub fn WinluxWritePty(handle: usize, buf: *const u8, len: u32, written: *mut u32) -> i32;
    pub fn WinluxResizePty(handle: usize, cols: u32, rows: u32) -> i32;
    pub fn WinluxPtyAlive(handle: usize) -> i32;
    pub fn WinluxClosePty(handle: usize) -> i32;
    // Images (décodeurs maison du runtime, voir runtime/src/imaging/api.rs).
    pub fn WinluxImageOpen(path: *const u16) -> usize;
    pub fn WinluxImageLastError(buf: *mut u8, cap: u32) -> i32;
    pub fn WinluxImageInfo(h: usize, out: *mut WinluxImageInfoC) -> i32;
    pub fn WinluxImagePage(h: usize, page: u32, out: *mut WinluxPageInfoC) -> i32;
    pub fn WinluxImageRender(
        h: usize,
        page: u32,
        view: *const WinluxViewC,
        out: *mut u8,
        w: u32,
        hh: u32,
        stride: u32,
        bg: u32,
        flags: u32,
    ) -> i32;
    pub fn WinluxImageMeta(
        h: usize,
        i: u32,
        key: *mut u8,
        kcap: u32,
        val: *mut u8,
        vcap: u32,
    ) -> i32;
    pub fn WinluxImageClose(h: usize) -> i32;
    pub fn WinluxImageSupported(path: *const u16) -> i32;
    pub fn WinluxImageFormats(buf: *mut u8, cap: u32) -> u32;
    pub fn WinluxImageThumbnail(path: *const u16, size: u32, out: *mut u8) -> i32;
    pub fn WinluxCaptureScreen(x: i32, y: i32, w: i32, h: i32, bits: *mut u8, stride: i32) -> i32;
    pub fn WinluxDrawStroke(
        bits: *mut u8,
        w: i32,
        h: i32,
        stride: i32,
        points: *const i32,
        count: i32,
        color: u32,
        pen: i32,
        mode: u32,
    ) -> i32;
    pub fn WinluxDrawText(
        bits: *mut u8,
        w: i32,
        h: i32,
        stride: i32,
        x: i32,
        y: i32,
        text: *const c_char,
        size: i32,
        color: u32,
    ) -> i32;
    pub fn WinluxSavePng(path: *const c_char, bits: *const u8, w: i32, h: i32, stride: i32) -> i32;
    pub fn WinluxRecordStart(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flags: u32,
        path: *const c_char,
    ) -> i32;
    pub fn WinluxRecordStop() -> i32;
    pub fn WinluxRecordState(elapsed_ms: *mut u32) -> i32;
    /// Rapport GPU `clé=valeur` du runtime Winlux ; renvoie la longueur complète.
    pub fn WinluxQueryGpu(buf: *mut u8, len: u32) -> u32;
    // Archives, images disque et lecteurs dynamiques (exports privés Winlux).
    pub fn WinluxMountList(buf: *mut u16, cap: u32) -> i32;
    pub fn WinluxArchiveSupport(path: *const u16) -> u32;
    pub fn WinluxArchiveFormats() -> u32;
    pub fn WinluxArchiveOpen(path: *const u16, password: *const u16) -> u32;
    pub fn WinluxArchiveExtract(path: *const u16, dest: *const u16, password: *const u16) -> u32;
    pub fn WinluxArchiveCreate(out: *const u16, sources: *const u16, password: *const u16) -> u32;
    pub fn WinluxArchiveCommit(path: *const u16) -> u32;
    pub fn WinluxImageMount(path: *const u16) -> u32;
    pub fn WinluxImageEject(letter: u32) -> u32;
    pub fn WinluxJobPoll(job: u32, progress: *mut u32, msg: *mut u16, cap: u32) -> u32;
    pub fn WinluxJobCancel(job: u32);
    pub fn WinluxJobClose(job: u32);
    // Moteur multimédia (runtime/src/dlls/winlux_media.rs).
    pub fn WinluxMediaProbe(path: *const u16, out: *mut u8, cap: u32) -> i32;
    pub fn WinluxMediaFormats(out: *mut u8, cap: u32) -> i32;
    pub fn WinluxMediaOpen(path: *const u16, opts: *const MediaOpenOptions) -> usize;
    pub fn WinluxMediaState(handle: usize, out: *mut MediaState) -> i32;
    pub fn WinluxMediaFrame(handle: usize, out: *mut u8, cap: u32) -> u32;
    pub fn WinluxMediaError(handle: usize, out: *mut u8, cap: u32) -> i32;
    pub fn WinluxMediaClose(handle: usize) -> i32;
    pub fn WinluxMediaThumbnail(path: *const u16, size: u32, out: *mut u8) -> i32;
    pub fn WinluxMediaPreview(path: *const u16, at_ms: u32, w: u32, h: u32, out: *mut u8) -> i32;
    pub fn WinluxMediaGetResume(path: *const u16) -> u32;
    pub fn WinluxMediaSetResume(path: *const u16, ms: u32, duration_ms: u32) -> i32;
    pub fn WinluxMediaKind(path: *const u16) -> u32;
    pub fn WinluxMediaCanonical(path: *const u16, out: *mut u16, cap: u32) -> i32;
    pub fn WinluxMediaSiblings(path: *const u16, out: *mut u16, cap: u32) -> i32;
    pub fn WinluxMediaSubtitles(path: *const u16, out: *mut u16, cap: u32) -> i32;
}

/// Options d'ouverture d'une session de lecture (`WinluxMediaOpen`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediaOpenOptions {
    pub start_ms: u32,
    pub width: u32,
    pub height: u32,
    pub volume: u32,
    pub speed: u32,
    pub audio_track: i32,
    pub sub_track: i32,
    pub flags: u32,
    pub sub_path: *const u16,
}

/// État d'une session de lecture (`WinluxMediaState`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MediaState {
    pub position_ms: u32,
    pub status: u32,
    pub frame_seq: u32,
    pub width: u32,
    pub height: u32,
    pub hw_active: u32,
    pub audio_active: u32,
    pub reserved: u32,
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    pub fn Shell_NotifyIconW(message: u32, data: *const u8) -> i32;
    pub fn ShellExecuteA(
        hwnd: Handle,
        verb: *const c_char,
        file: *const c_char,
        params: *const c_char,
        dir: *const c_char,
        show: i32,
    ) -> Handle;
    pub fn ShellExecuteW(
        hwnd: Handle,
        verb: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> Handle;
}

#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    pub fn CoInitializeEx(reserved: Handle, coinit: u32) -> i32;
    pub fn CoUninitialize();
    pub fn CoTaskMemAlloc(size: usize) -> *mut c_void;
    pub fn CoTaskMemRealloc(memory: *mut c_void, size: usize) -> *mut c_void;
    pub fn CoTaskMemFree(memory: *mut c_void);
    pub fn CoCreateGuid(out: *mut Guid) -> i32;
    pub fn CLSIDFromString(value: *const u16, out: *mut Guid) -> i32;
    pub fn StringFromGUID2(guid: *const Guid, out: *mut u16, capacity: i32) -> i32;
    pub fn CreateStreamOnHGlobal(memory: Handle, delete_on_release: i32, out: *mut Handle) -> i32;
    pub fn GetHGlobalFromStream(stream: Handle, out: *mut Handle) -> i32;
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
    pub fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const AddrInfoA,
        result: *mut *mut AddrInfoA,
    ) -> i32;
    pub fn freeaddrinfo(result: *mut AddrInfoA);
    pub fn GetAddrInfoW(
        node: *const u16,
        service: *const u16,
        hints: *const AddrInfoW,
        result: *mut *mut AddrInfoW,
    ) -> i32;
    pub fn FreeAddrInfoW(result: *mut AddrInfoW);
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    pub fn RegCreateKeyExA(
        root: Handle,
        sub: *const c_char,
        reserved: u32,
        class: *const c_char,
        options: u32,
        access: u32,
        sec: *mut c_void,
        result: *mut Handle,
        disposition: *mut u32,
    ) -> i32;
    pub fn RegSetValueExA(
        key: Handle,
        name: *const c_char,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    pub fn RegQueryValueExA(
        key: Handle,
        name: *const c_char,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    pub fn RegDeleteValueA(key: Handle, name: *const c_char) -> i32;
    pub fn RegDeleteKeyA(root: Handle, sub: *const c_char) -> i32;
    pub fn RegCloseKey(key: Handle) -> i32;
    pub fn RegOpenKeyExA(
        root: Handle,
        sub: *const c_char,
        options: u32,
        access: u32,
        result: *mut Handle,
    ) -> i32;
    pub fn RegEnumKeyExA(
        key: Handle,
        index: u32,
        name: *mut c_char,
        name_len: *mut u32,
        reserved: *mut u32,
        class: *mut c_char,
        class_len: *mut u32,
        last_write: *mut u64,
    ) -> i32;
    pub fn RegEnumValueA(
        key: Handle,
        index: u32,
        name: *mut c_char,
        name_len: *mut u32,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
    pub fn RegQueryInfoKeyA(
        key: Handle,
        class: *mut c_char,
        class_len: *mut u32,
        reserved: *mut u32,
        sub_keys: *mut u32,
        max_sub_key_len: *mut u32,
        max_class_len: *mut u32,
        values: *mut u32,
        max_value_name_len: *mut u32,
        max_value_len: *mut u32,
        security: *mut u32,
        last_write: *mut u64,
    ) -> i32;
    pub fn RegDeleteTreeA(key: Handle, sub: *const c_char) -> i32;
    // Fonctions privées du runtime pour l'Éditeur du Registre.
    pub fn WinluxRegImportA(path: *const c_char) -> i32;
    pub fn WinluxRegExportA(path: *const c_char, key: *const c_char) -> i32;
    pub fn WinluxRegRenameValueA(key: Handle, old: *const c_char, new: *const c_char) -> i32;
    pub fn WinluxRegRenameKeyA(root: Handle, sub: *const c_char, new: *const c_char) -> i32;
}

#[link(name = "user32", kind = "raw-dylib")]
extern "C" {
    pub fn OpenClipboard(owner: Handle) -> i32;
    pub fn CloseClipboard() -> i32;
    pub fn EmptyClipboard() -> i32;
    pub fn SetClipboardData(format: u32, mem: usize) -> usize;
    pub fn GetClipboardData(format: u32) -> usize;
    pub fn IsClipboardFormatAvailable(format: u32) -> i32;
    pub fn GetClipboardSequenceNumber() -> u32;
    pub fn RegisterClassExA(wc: *const WndClassExA) -> u16;
    pub fn CreateWindowExA(
        ex_style: u32,
        class: *const c_char,
        title: *const c_char,
        style: u32,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: *mut c_void,
    ) -> Handle;
    pub fn DefWindowProcA(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    pub fn GetMessageA(msg: *mut Msg, hwnd: Handle, min: u32, max: u32) -> i32;
    pub fn GetMessageW(msg: *mut Msg, hwnd: Handle, min: u32, max: u32) -> i32;
    pub fn PeekMessageA(msg: *mut Msg, hwnd: Handle, min: u32, max: u32, remove: u32) -> i32;
    pub fn PeekMessageW(msg: *mut Msg, hwnd: Handle, min: u32, max: u32, remove: u32) -> i32;
    pub fn DispatchMessageA(msg: *const Msg) -> isize;
    pub fn DispatchMessageW(msg: *const Msg) -> isize;
    pub fn TranslateMessage(msg: *const Msg) -> i32;
    pub fn PostMessageA(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> i32;
    pub fn PostMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> i32;
    pub fn SendMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    pub fn PostQuitMessage(code: i32);
    pub fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
    pub fn UpdateWindow(hwnd: Handle) -> i32;
    pub fn DestroyWindow(hwnd: Handle) -> i32;
    pub fn IsWindow(hwnd: Handle) -> i32;
    pub fn GetFocus() -> Handle;
    pub fn SetFocus(hwnd: Handle) -> Handle;
    pub fn GetKeyState(vk: i32) -> i16;
    pub fn SetCapture(hwnd: Handle) -> Handle;
    pub fn ScreenToClient(hwnd: Handle, p: *mut Point) -> i32;
    pub fn ReleaseCapture() -> i32;
    pub fn GetAsyncKeyState(vk: i32) -> i16;
    pub fn GetKeyboardState(state: *mut u8) -> i32;
    pub fn GetClientRect(hwnd: Handle, out: *mut Rect) -> i32;
    pub fn GetWindowRect(hwnd: Handle, out: *mut Rect) -> i32;
    pub fn SetWindowTextW(hwnd: Handle, text: *const u16) -> i32;
    pub fn GetWindowTextW(hwnd: Handle, text: *mut u16, cap: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: Handle) -> i32;
    pub fn SetWindowPos(
        hwnd: Handle,
        after: Handle,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    pub fn MoveWindow(hwnd: Handle, x: i32, y: i32, cx: i32, cy: i32, repaint: i32) -> i32;
    pub fn BeginPaint(hwnd: Handle, ps: *mut PaintStruct) -> Handle;
    pub fn EndPaint(hwnd: Handle, ps: *const PaintStruct) -> i32;
    pub fn GetDC(hwnd: Handle) -> Handle;
    pub fn ReleaseDC(hwnd: Handle, hdc: Handle) -> i32;
    pub fn FillRect(hdc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn FrameRect(hdc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn InvalidateRect(hwnd: Handle, rect: *const Rect, erase: i32) -> i32;
    pub fn GetSystemMetrics(index: i32) -> i32;
    pub fn GetDoubleClickTime() -> u32;
    pub fn GetSysColor(index: i32) -> u32;
    pub fn GetSysColorBrush(index: i32) -> Handle;
    pub fn SetCursor(cursor: Handle) -> Handle;
    pub fn LoadCursorW(instance: Handle, name: usize) -> Handle;
    pub fn SetTimer(hwnd: Handle, id: usize, elapse: u32, func: usize) -> usize;
    pub fn GetWindowLongA(hwnd: Handle, index: i32) -> i32;
    pub fn SetWindowLongA(hwnd: Handle, index: i32, value: i32) -> i32;
    pub fn KillTimer(hwnd: Handle, id: usize) -> i32;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "C" {
    pub fn GetStockObject(index: i32) -> Handle;
    pub fn GetObjectA(obj: Handle, size: i32, out: *mut c_void) -> i32;
    pub fn GetObjectW(obj: Handle, size: i32, out: *mut c_void) -> i32;
    pub fn CreateSolidBrush(color: u32) -> Handle;
    pub fn CreatePen(style: u32, width: i32, color: u32) -> Handle;
    pub fn MoveToEx(dc: Handle, x: i32, y: i32, previous: *mut Point) -> i32;
    pub fn LineTo(dc: Handle, x: i32, y: i32) -> i32;
    pub fn CreateCompatibleDC(dc: Handle) -> Handle;
    pub fn SetDIBitsToDevice(
        dc: Handle,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        src_x: i32,
        src_y: i32,
        start: u32,
        lines: u32,
        bits: *const c_void,
        info: *const BitmapInfoHeader,
        usage: u32,
    ) -> i32;
    pub fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    pub fn CreateDIBSection(
        dc: Handle,
        info: *const BitmapInfoHeader,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Handle;
    pub fn DeleteDC(dc: Handle) -> i32;
    pub fn GetDeviceCaps(dc: Handle, index: i32) -> i32;
    pub fn StretchDIBits(
        dc: Handle,
        xd: i32,
        yd: i32,
        wd: i32,
        hd: i32,
        xs: i32,
        ys: i32,
        ws: i32,
        hs: i32,
        bits: *const c_void,
        info: *const BitmapInfoHeader,
        usage: u32,
        rop: u32,
    ) -> i32;
    pub fn CreateFontA(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_family: u32,
        face: *const c_char,
    ) -> Handle;
    pub fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_family: u32,
        face: *const u16,
    ) -> Handle;
    pub fn SelectObject(hdc: Handle, obj: Handle) -> Handle;
    pub fn DeleteObject(obj: Handle) -> i32;
    pub fn SetTextColor(hdc: Handle, color: u32) -> u32;
    pub fn SetBkColor(hdc: Handle, color: u32) -> u32;
    pub fn SetBkMode(hdc: Handle, mode: i32) -> i32;
    pub fn PatBlt(hdc: Handle, x: i32, y: i32, width: i32, height: i32, rop: u32) -> i32;
    pub fn BitBlt(
        dst: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        src: Handle,
        sx: i32,
        sy: i32,
        rop: u32,
    ) -> i32;
    pub fn StretchBlt(
        dst: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        src: Handle,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        rop: u32,
    ) -> i32;
    pub fn SetStretchBltMode(hdc: Handle, mode: i32) -> i32;
    pub fn TextOutA(hdc: Handle, x: i32, y: i32, text: *const c_char, count: i32) -> i32;
    pub fn GetTextExtentPoint32A(
        hdc: Handle,
        text: *const c_char,
        count: i32,
        size: *mut [i32; 2],
    ) -> i32;
    pub fn WinluxDrawImageA(
        hdc: Handle,
        path: *const c_char,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        background: u32,
    ) -> i32;
    pub fn AddFontResourceA(name: *const c_char) -> i32;
    pub fn RemoveFontResourceA(name: *const c_char) -> i32;
    pub fn WinluxDrawIconA(
        hdc: Handle,
        request: *const c_char,
        x: i32,
        y: i32,
        size: i32,
        background: u32,
    ) -> i32;
    pub fn Rectangle(
        hdc: Handle,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        pen: Handle,
        brush: Handle,
    ) -> i32;
    pub fn Ellipse(hdc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn RoundRect(
        hdc: Handle,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> i32;
    pub fn SetPixelV(hdc: Handle, x: i32, y: i32, color: u32) -> i32;
}

// Spooler d'impression (`winspool.drv`) : API Win32 et extensions Winlux
// utilisées par la page « Imprimantes et scanners » des Paramètres.
#[link(name = "winspool.drv", kind = "raw-dylib", modifiers = "+verbatim")]
extern "system" {
    pub fn EnumPrintersW(
        flags: u32,
        name: *const u16,
        level: u32,
        buf: *mut u8,
        cb: u32,
        needed: *mut u32,
        returned: *mut u32,
    ) -> i32;
    pub fn GetDefaultPrinterW(buf: *mut u16, size: *mut u32) -> i32;
    pub fn SetDefaultPrinterW(name: *const u16) -> i32;
    pub fn OpenPrinterW(name: *const u16, printer: *mut Handle, defaults: *const c_void) -> i32;
    pub fn ClosePrinter(printer: Handle) -> i32;
    pub fn StartDocPrinterW(printer: Handle, level: u32, info: *const c_void) -> u32;
    pub fn StartPagePrinter(printer: Handle) -> i32;
    pub fn WritePrinter(printer: Handle, buf: *const u8, len: u32, written: *mut u32) -> i32;
    pub fn EndPagePrinter(printer: Handle) -> i32;
    pub fn EndDocPrinter(printer: Handle) -> i32;
    pub fn DeletePrinter(printer: Handle) -> i32;
    pub fn SetPrinterW(printer: Handle, level: u32, info: *const u8, command: u32) -> i32;
    pub fn EnumJobsW(
        printer: Handle,
        first: u32,
        count: u32,
        level: u32,
        buf: *mut u8,
        cb: u32,
        needed: *mut u32,
        returned: *mut u32,
    ) -> i32;
    pub fn SetJobW(printer: Handle, job: u32, level: u32, info: *const u8, command: u32) -> i32;
    pub fn DeviceCapabilitiesW(
        device: *const u16,
        port: *const u16,
        capability: u16,
        output: *mut u8,
        devmode: *const u8,
    ) -> i32;
    pub fn DocumentPropertiesW(
        hwnd: Handle,
        printer: Handle,
        device: *const u16,
        output: *mut u8,
        input: *const u8,
        mode: u32,
    ) -> i32;
    pub fn WinluxDiscoverPrintersW(buf: *mut u16, cap: u32) -> u32;
    pub fn WinluxAddPrinterW(name: *const u16, uri: *const u16) -> i32;
    pub fn WinluxPrintStatusW(buf: *mut u16, cap: u32) -> u32;
    pub fn WinluxPrintTestPageW(name: *const u16) -> i32;
}

/// `DOCINFOW` (gdi32 `StartDocW`).
#[repr(C)]
pub struct DocInfoW {
    pub size: i32,
    pub doc_name: *const u16,
    pub output: *const u16,
    pub datatype: *const u16,
    pub flags: u32,
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "system" {
    pub fn CreateDCW(
        driver: *const u16,
        device: *const u16,
        output: *const u16,
        devmode: *const u8,
    ) -> Handle;
    pub fn StartDocW(dc: Handle, info: *const DocInfoW) -> i32;
    pub fn StartPage(dc: Handle) -> i32;
    pub fn EndPage(dc: Handle) -> i32;
    pub fn EndDoc(dc: Handle) -> i32;
}

#[link(name = "comdlg32", kind = "raw-dylib")]
extern "system" {
    pub fn PrintDlgW(pd: *mut u8) -> i32;
    pub fn CommDlgExtendedError() -> u32;
}

#[link(name = "powrprof", kind = "raw-dylib")]
extern "system" {
    pub fn PowerGetActiveScheme(key: Handle, guid: *mut *mut [u8; 16]) -> u32;
    pub fn PowerSetActiveScheme(key: Handle, guid: *const [u8; 16]) -> u32;
    pub fn PowerReadFriendlyName(
        key: Handle,
        scheme: *const [u8; 16],
        sub: *const c_void,
        setting: *const c_void,
        buffer: *mut u16,
        size: *mut u32,
    ) -> u32;
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
    pub fn strcpy(d: *mut c_char, s: *const c_char) -> *mut c_char;
    pub fn strncpy(d: *mut c_char, s: *const c_char, n: usize) -> *mut c_char;
    pub fn strcat(d: *mut c_char, s: *const c_char) -> *mut c_char;
    pub fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> i32;
    pub fn strchr(s: *const c_char, c: i32) -> *mut c_char;
    pub fn strrchr(s: *const c_char, c: i32) -> *mut c_char;
    pub fn strstr(h: *const c_char, n: *const c_char) -> *mut c_char;
    pub fn strdup(s: *const c_char) -> *mut c_char;
    pub fn stricmp(a: *const c_char, b: *const c_char) -> i32;
    pub fn strlwr(s: *mut c_char) -> *mut c_char;
    pub fn strupr(s: *mut c_char) -> *mut c_char;
    pub fn strtok_s(s: *mut c_char, delim: *const c_char, context: *mut *mut c_char)
        -> *mut c_char;
    pub fn atol(s: *const c_char) -> i32;
    pub fn atoll(s: *const c_char) -> i64;
    pub fn strtol(s: *const c_char, end: *mut *mut c_char, base: i32) -> i32;
    pub fn strtoul(s: *const c_char, end: *mut *mut c_char, base: i32) -> u32;
    pub fn strtoll(s: *const c_char, end: *mut *mut c_char, base: i32) -> i64;
    pub fn strtod(s: *const c_char, end: *mut *mut c_char) -> f64;
    pub fn toupper(c: i32) -> i32;
    pub fn tolower(c: i32) -> i32;
    pub fn isalpha(c: i32) -> i32;
    pub fn isdigit(c: i32) -> i32;
    pub fn isspace(c: i32) -> i32;
    pub fn isxdigit(c: i32) -> i32;
    pub fn sqrt(x: f64) -> f64;
    pub fn pow(x: f64, y: f64) -> f64;
    pub fn fabs(x: f64) -> f64;
    pub fn floor(x: f64) -> f64;
    pub fn abs(v: i32) -> i32;
    pub fn qsort(base: *mut c_void, num: usize, width: usize, compare: usize);
    pub fn bsearch(
        key: *const c_void,
        base: *const c_void,
        num: usize,
        width: usize,
        compare: usize,
    ) -> *mut c_void;
    pub fn _itoa(value: i32, buf: *mut c_char, radix: i32) -> *mut c_char;
    pub fn _i64toa(value: i64, buf: *mut c_char, radix: i32) -> *mut c_char;
    pub fn _snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> i32;
    pub fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> i32;
    pub fn _snwprintf(buf: *mut u16, count: usize, fmt: *const u16, ...) -> i32;
    pub fn _itow(value: i32, buf: *mut u16, radix: i32) -> *mut u16;
    pub fn wcscmp(a: *const u16, b: *const u16) -> i32;
    pub fn wcslen(s: *const u16) -> usize;
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
    pub fn fclose(f: *mut c_void) -> i32;
    pub fn fread(ptr: *mut u8, size: usize, count: usize, f: *mut c_void) -> usize;
    pub fn fwrite(ptr: *const u8, size: usize, count: usize, f: *mut c_void) -> usize;
    pub fn fgets(buf: *mut c_char, n: i32, f: *mut c_void) -> *mut c_char;
    pub fn fseek(f: *mut c_void, offset: i64, origin: i32) -> i32;
    pub fn ftell(f: *mut c_void) -> i64;
    pub fn feof(f: *mut c_void) -> i32;
    pub fn fflush(f: *mut c_void) -> i32;
    pub fn fprintf(f: *mut c_void, fmt: *const c_char, ...) -> i32;
    pub fn sscanf(buf: *const c_char, fmt: *const c_char, ...) -> i32;
    pub fn fscanf(f: *mut c_void, fmt: *const c_char, ...) -> i32;
    pub fn remove(path: *const c_char) -> i32;
}

#[link(name = "winmm", kind = "raw-dylib")]
extern "system" {
    pub fn PlaySoundA(name: *const c_char, module: Handle, flags: u32) -> i32;
    pub fn waveInGetNumDevs() -> u32;
    pub fn waveInOpen(
        phwi: *mut Handle,
        device: u32,
        pwfx: *const WaveFormatEx,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    pub fn waveInPrepareHeader(hwi: Handle, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    pub fn waveInUnprepareHeader(hwi: Handle, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    pub fn waveInAddBuffer(hwi: Handle, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    pub fn waveInStart(hwi: Handle) -> u32;
    pub fn waveInStop(hwi: Handle) -> u32;
    pub fn waveInReset(hwi: Handle) -> u32;
    pub fn waveInClose(hwi: Handle) -> u32;
}

/// Requis par le compilateur dès qu'un programme Windows manipule des flottants.
#[no_mangle]
#[used]
pub static _fltused: i32 = 0;

// Minimal real delay-load helper for the PE regressions, independent of the
// Microsoft SDK's delayimp.lib. The loader must leave its linker thunks intact.
static DELAY_LOAD_CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
pub fn delay_load_count() -> usize {
    DELAY_LOAD_CALLS.load(core::sync::atomic::Ordering::SeqCst)
}
#[no_mangle]
pub unsafe extern "system" fn __delayLoadHelper2(
    descriptor: *const u32,
    slot: *mut usize,
) -> *const c_void {
    DELAY_LOAD_CALLS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    if descriptor.is_null() || slot.is_null() || *descriptor != 1 {
        ExitProcess(94);
    }
    let base = GetModuleHandleA(core::ptr::null()) as usize;
    let handle_slot = (base + *descriptor.add(2) as usize) as *mut Handle;
    let mut handle = *handle_slot;
    if handle.is_null() {
        handle = LoadLibraryA((base + *descriptor.add(1) as usize) as *const c_char);
        if handle.is_null() {
            ExitProcess(95);
        }
        *handle_slot = handle;
    }
    let index = (slot as usize - base - *descriptor.add(3) as usize) / 8;
    let lookup = *((base + *descriptor.add(4) as usize + index * 8) as *const u64);
    let name = if lookup & (1 << 63) != 0 {
        (lookup & 0xffff) as usize as *const c_char
    } else {
        (base + lookup as usize + 2) as *const c_char
    };
    let address = GetProcAddress(handle, name);
    if address.is_null() {
        ExitProcess(96);
    }
    *slot = address as usize;
    address
}

/// Écrit des octets bruts sur la sortie standard via WriteFile.
pub fn out(s: &[u8]) {
    unsafe {
        let mut n = 0u32;
        WriteFile(
            GetStdHandle(STD_OUTPUT_HANDLE),
            s.as_ptr(),
            s.len() as u32,
            &mut n,
            core::ptr::null_mut(),
        );
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

// Sonde de pile appelée par le code généré pour les fonctions dont les
// variables locales dépassent une page (4 Kio) : touche chaque page entre le
// pointeur de pile et la nouvelle limite, sans modifier aucun registre.
core::arch::global_asm!(
    ".globl __chkstk",
    "__chkstk:",
    "push rcx",
    "push rax",
    "lea rcx, [rsp + 24]",
    "cmp rax, 0x1000",
    "jb 2f",
    "1:",
    "sub rcx, 0x1000",
    "test [rcx], rcx",
    "sub rax, 0x1000",
    "cmp rax, 0x1000",
    "ja 1b",
    "2:",
    "sub rcx, rax",
    "test [rcx], rcx",
    "pop rax",
    "pop rcx",
    "ret",
);

/// Certaines fonctions précompilées de `core` référencent le gestionnaire
/// d'exceptions C++ de MSVC dans leurs tables de déroulement. Les tests sont
/// compilés avec `panic = "abort"` : aucun déroulement ne l'appellera.
#[no_mangle]
pub extern "C" fn __CxxFrameHandler3() -> i32 {
    1
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    out(b"panic\n");
    unsafe { ExitProcess(101) }
}

// ---------- Langue de l'interface ----------

/// Texte traduit : français, anglais, espagnol, allemand (Windows-1252).
pub type Tr4 = [&'static [u8]; 4];

static UI_LANG: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(u8::MAX);
static UI_LANG_TICK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

fn lang_index(code: &[u8]) -> Option<u8> {
    match code.get(..2) {
        Some(b"fr") | Some(b"FR") => Some(0),
        Some(b"en") | Some(b"EN") => Some(1),
        Some(b"es") | Some(b"ES") => Some(2),
        Some(b"de") | Some(b"DE") => Some(3),
        _ => None,
    }
}

/// Langue de l'interface (0 fr, 1 en, 2 es, 3 de), pour les applications
/// autonomes comme pour celles du bureau : `LANG=` de /etc/winlux/locale.conf
/// (Paramètres > Langue), sinon la variable `LANG`, sinon le français.
/// Relue au plus toutes les deux secondes pour suivre un changement.
/// Langue choisie dans cette session (page Langue des Paramètres) : prioritaire
/// sur locale.conf, que le service système n'a peut-être pas encore réécrit.
static UI_LANG_CHOSEN: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(u8::MAX);

/// Fixe la langue de l'interface pour ce processus (0 fr, 1 en, 2 es, 3 de).
pub fn set_ui_lang(index: usize) {
    if index < 4 {
        UI_LANG_CHOSEN.store(index as u8, core::sync::atomic::Ordering::Relaxed);
    }
}

pub fn ui_lang() -> usize {
    use core::sync::atomic::Ordering::Relaxed;
    let chosen = UI_LANG_CHOSEN.load(Relaxed);
    if chosen != u8::MAX {
        return chosen as usize;
    }
    let now = unsafe { GetTickCount() };
    let cached = UI_LANG.load(Relaxed);
    if cached != u8::MAX && now.wrapping_sub(UI_LANG_TICK.load(Relaxed)) < 2000 {
        return cached as usize;
    }
    let mut found = None;
    unsafe {
        let h = CreateFileA(
            c"Z:\\etc\\winlux\\locale.conf".as_ptr(),
            0x8000_0000,
            1,
            core::ptr::null_mut(),
            3,
            0,
            core::ptr::null_mut(),
        );
        if !h.is_null() && h as isize != -1 {
            let mut buf = [0u8; 256];
            let mut read = 0u32;
            ReadFile(h, buf.as_mut_ptr(), 256, &mut read, core::ptr::null_mut());
            CloseHandle(h);
            for line in buf[..(read as usize).min(256)].split(|&c| c == b'\n') {
                if let Some(v) = line.strip_prefix(b"LANG=") {
                    found = lang_index(v);
                }
            }
        }
        if found.is_none() {
            let mut v = [0u8; 16];
            let n = GetEnvironmentVariableA(c"LANG".as_ptr(), v.as_mut_ptr().cast(), 16);
            if (2..16).contains(&n) {
                found = lang_index(&v[..2]);
            }
        }
    }
    let lang = found.unwrap_or(0);
    UI_LANG.store(lang, Relaxed);
    UI_LANG_TICK.store(now, Relaxed);
    lang as usize
}

/// Texte dans la langue de l'interface.
pub fn tr4(s: &Tr4) -> &'static [u8] {
    s[ui_lang()]
}
