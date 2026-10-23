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
#[derive(Clone, Copy)]
pub struct OverlappedEntry { pub completion_key: usize, pub overlapped: *mut c_void, pub internal: usize, pub bytes_transferred: u32, pub reserved: u32 }

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
pub struct FileTime { pub low: u32, pub high: u32 }
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
pub const VK_A: i32 = 0x41;
pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SW_SHOW: i32 = 5;
pub const SW_SHOWNORMAL: i32 = 1;

pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn GetStdHandle(which: u32) -> Handle;
    pub fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, ov: *mut c_void) -> i32;
    pub fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, ov: *mut c_void) -> i32;
    pub fn CreateFileA(path: *const c_char, access: u32, share: u32, sec: *mut c_void, creation: u32, flags: u32, template: Handle) -> Handle;
    pub fn CreateFileW(path: *const u16, access: u32, share: u32, sec: *mut c_void, creation: u32, flags: u32, template: Handle) -> Handle;
    pub fn GetFileSize(h: Handle, high: *mut u32) -> u32;
    pub fn SetEndOfFile(h: Handle) -> i32;
    pub fn GetFileAttributesExA(path: *const c_char, level: u32, data: *mut Win32FileAttributeData) -> i32;
    pub fn GetFileAttributesExW(path: *const u16, level: u32, data: *mut Win32FileAttributeData) -> i32;
    pub fn GetTempFileNameA(path: *const c_char, prefix: *const c_char, unique: u32, output: *mut c_char) -> u32;
    pub fn GetTempFileNameW(path: *const u16, prefix: *const u16, unique: u32, output: *mut u16) -> u32;
    pub fn SetFilePointer(h: Handle, distance: i32, high: *mut i32, method: u32) -> u32;
    pub fn FlushFileBuffers(h: Handle) -> i32;
    pub fn GetOverlappedResult(h: Handle, overlapped: *mut Overlapped, transferred: *mut u32, wait: i32) -> i32;
    pub fn CreateIoCompletionPort(file: Handle, existing_port: Handle, completion_key: usize, concurrent_threads: u32) -> Handle;
    pub fn GetQueuedCompletionStatus(port: Handle, bytes: *mut u32, key: *mut usize, overlapped: *mut *mut c_void, timeout: u32) -> i32;
    pub fn GetQueuedCompletionStatusEx(port: Handle, entries: *mut OverlappedEntry, count: u32, removed: *mut u32, timeout: u32, alertable: i32) -> i32;
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
    pub fn FindFirstFileA(path: *const c_char, data: *mut Win32FindDataA) -> Handle;
    pub fn FindNextFileA(search: Handle, data: *mut Win32FindDataA) -> i32;
    pub fn FindFirstFileW(path: *const u16, data: *mut Win32FindDataW) -> Handle;
    pub fn FindNextFileW(search: Handle, data: *mut Win32FindDataW) -> i32;
    pub fn FindClose(search: Handle) -> i32;
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
    pub fn Sleep(ms: u32);
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

#[link(name = "user32", kind = "raw-dylib")]
extern "C" {
    pub fn RegisterClassExA(wc: *const WndClassExA) -> u16;
    pub fn CreateWindowExA(
        ex_style: u32, class: *const c_char, title: *const c_char, style: u32,
        x: i32, y: i32, cx: i32, cy: i32, parent: Handle, menu: Handle, instance: Handle, param: *mut c_void,
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
    pub fn GetAsyncKeyState(vk: i32) -> i16;
    pub fn GetKeyboardState(state: *mut u8) -> i32;
    pub fn GetClientRect(hwnd: Handle, out: *mut Rect) -> i32;
    pub fn GetWindowRect(hwnd: Handle, out: *mut Rect) -> i32;
    pub fn SetWindowTextW(hwnd: Handle, text: *const u16) -> i32;
    pub fn GetWindowTextW(hwnd: Handle, text: *mut u16, cap: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: Handle) -> i32;
    pub fn SetWindowPos(hwnd: Handle, after: Handle, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    pub fn MoveWindow(hwnd: Handle, x: i32, y: i32, cx: i32, cy: i32, repaint: i32) -> i32;
    pub fn BeginPaint(hwnd: Handle, ps: *mut PaintStruct) -> Handle;
    pub fn EndPaint(hwnd: Handle, ps: *const PaintStruct) -> i32;
    pub fn GetDC(hwnd: Handle) -> Handle;
    pub fn ReleaseDC(hwnd: Handle, hdc: Handle) -> i32;
    pub fn FillRect(hdc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn FrameRect(hdc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn InvalidateRect(hwnd: Handle, rect: *const Rect, erase: i32) -> i32;
    pub fn GetSystemMetrics(index: i32) -> i32;
    pub fn GetSysColor(index: i32) -> u32;
    pub fn GetSysColorBrush(index: i32) -> Handle;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "C" {
    pub fn GetStockObject(index: i32) -> Handle;
    pub fn GetObjectA(obj: Handle, size: i32, out: *mut c_void) -> i32;
    pub fn GetObjectW(obj: Handle, size: i32, out: *mut c_void) -> i32;
    pub fn CreateSolidBrush(color: u32) -> Handle;
    pub fn CreatePen(style: u32, width: i32, color: u32) -> Handle;
    pub fn CreateCompatibleDC(dc: Handle) -> Handle;
    pub fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    pub fn CreateDIBSection(dc: Handle, info: *const BitmapInfoHeader, usage: u32, bits: *mut *mut c_void, section: Handle, offset: u32) -> Handle;
    pub fn DeleteDC(dc: Handle) -> i32;
    pub fn CreateFontA(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strikeout: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_family: u32, face: *const c_char) -> Handle;
    pub fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strikeout: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_family: u32, face: *const u16) -> Handle;
    pub fn SelectObject(hdc: Handle, obj: Handle) -> Handle;
    pub fn DeleteObject(obj: Handle) -> i32;
    pub fn SetTextColor(hdc: Handle, color: u32) -> u32;
    pub fn SetBkColor(hdc: Handle, color: u32) -> u32;
    pub fn SetBkMode(hdc: Handle, mode: i32) -> i32;
    pub fn PatBlt(hdc: Handle, x: i32, y: i32, width: i32, height: i32, rop: u32) -> i32;
    pub fn BitBlt(dst: Handle, x: i32, y: i32, width: i32, height: i32, src: Handle, sx: i32, sy: i32, rop: u32) -> i32;
    pub fn StretchBlt(dst: Handle, x: i32, y: i32, width: i32, height: i32, src: Handle, sx: i32, sy: i32, sw: i32, sh: i32, rop: u32) -> i32;
    pub fn TextOutA(hdc: Handle, x: i32, y: i32, text: *const c_char, count: i32) -> i32;
    pub fn Rectangle(hdc: Handle, left: i32, top: i32, right: i32, bottom: i32, pen: Handle, brush: Handle) -> i32;
    pub fn Ellipse(hdc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
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
    pub fn strtok_s(s: *mut c_char, delim: *const c_char, context: *mut *mut c_char) -> *mut c_char;
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
    pub fn bsearch(key: *const c_void, base: *const c_void, num: usize, width: usize, compare: usize) -> *mut c_void;
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
