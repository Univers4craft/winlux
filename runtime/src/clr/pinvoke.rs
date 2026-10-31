//! Pont P/Invoke : appels managés vers les DLL natives Win32 du runtime Winlux.

use super::bcl::{alloc_managed_string, read_managed_string};
use super::il::Value;

pub fn dispatch_pinvoke(module: &str, name: &str, args: &[Value]) -> Option<Value> {
    let module_lower = module.to_lowercase();
    let module_base = module_lower
        .trim_end_matches(".dll")
        .trim_end_matches(".DLL");

    match module_base {
        "kernel32" => kernel32_dispatch(name, args),
        "user32" => user32_dispatch(name, args),
        "advapi32" => advapi32_dispatch(name, args),
        "msvcrt" | "ucrtbase" => crt_dispatch(name, args),
        "gdi32" => gdi32_dispatch(name, args),
        "shell32" => shell32_dispatch(name, args),
        "ole32" => ole32_dispatch(name, args),
        "ntdll" => ntdll_dispatch(name, args),
        _ => {
            crate::log_warn!(
                "clr-pinvoke",
                "module P/Invoke inconnu: {} (fonction {})",
                module,
                name
            );
            try_generic_dll_call(module_base, name, args)
        }
    }
}

fn try_generic_dll_call(module: &str, name: &str, args: &[Value]) -> Option<Value> {
    crate::log_debug!(
        "clr-pinvoke",
        "tentative d'appel générique: {}.{}",
        module,
        name
    );
    Some(Value::I32(0))
}

// ── kernel32.dll ──

fn kernel32_dispatch(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "GetLastError" => Some(Value::I32(0)),
        "SetLastError" => Some(Value::I32(0)),
        "GetCurrentProcess" => Some(Value::I64(-1)),
        "GetCurrentProcessId" => Some(Value::I32(std::process::id() as i32)),
        "GetCurrentThread" => Some(Value::I64(-2)),
        "GetCurrentThreadId" => Some(Value::I32(0)),
        "CloseHandle" => Some(Value::I32(1)),

        "GetStdHandle" => {
            let which = match args.last() {
                Some(Value::I32(v)) => *v,
                _ => -11,
            };
            Some(Value::I64(which as i64))
        }

        "WriteFile" | "WriteConsoleA" | "WriteConsoleW" => {
            if let Some(Value::Ref(buf)) = args.get(1) {
                if !buf.is_null() {
                    let len = match args.get(2) {
                        Some(Value::I32(n)) => *n as usize,
                        _ => 0,
                    };
                    if len > 0 {
                        let slice = unsafe { std::slice::from_raw_parts(*buf, len) };
                        let s = String::from_utf8_lossy(slice);
                        print!("{}", s);
                    }
                }
            }
            Some(Value::I32(1))
        }

        "ReadFile" | "ReadConsoleA" => Some(Value::I32(0)),

        "CreateFileA" | "CreateFileW" => Some(Value::I64(-1)),

        "GetModuleHandleA" | "GetModuleHandleW" => Some(Value::I64(0x400000)),
        "GetModuleFileNameA" | "GetModuleFileNameW" => Some(Value::I32(0)),
        "GetProcAddress" => Some(Value::I64(0)),
        "LoadLibraryA" | "LoadLibraryW" | "LoadLibraryExA" | "LoadLibraryExW" => {
            let lib_name = match args.last() {
                Some(Value::Ref(p)) if !p.is_null() => unsafe {
                    let c = std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char);
                    c.to_string_lossy().to_string()
                },
                _ => String::new(),
            };
            crate::log_debug!("clr-pinvoke", "LoadLibrary: {}", lib_name);
            Some(Value::I64(0x10000))
        }
        "FreeLibrary" => Some(Value::I32(1)),

        "GetTickCount" => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i32;
            Some(Value::I32(ms))
        }
        "GetTickCount64" => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            Some(Value::I64(ms))
        }
        "QueryPerformanceCounter" => {
            if let Some(Value::Ref(p)) = args.last() {
                if !p.is_null() {
                    let ns = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64;
                    unsafe {
                        *(*p as *mut u64) = ns;
                    }
                }
            }
            Some(Value::I32(1))
        }
        "QueryPerformanceFrequency" => {
            if let Some(Value::Ref(p)) = args.last() {
                if !p.is_null() {
                    unsafe {
                        *(*p as *mut u64) = 1_000_000_000;
                    }
                }
            }
            Some(Value::I32(1))
        }
        "Sleep" => {
            let ms = match args.last() {
                Some(Value::I32(v)) => *v as u64,
                _ => 0,
            };
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Some(Value::I32(0))
        }

        "GetSystemInfo" | "GetNativeSystemInfo" => Some(Value::I32(0)),
        "GetVersionExA" | "GetVersionExW" => Some(Value::I32(1)),

        "VirtualAlloc" => {
            let size = match args.get(1) {
                Some(Value::I32(s)) => *s as usize,
                Some(Value::I64(s)) => *s as usize,
                _ => 4096,
            };
            let layout = std::alloc::Layout::from_size_align(size.max(1), 4096).unwrap();
            let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
            Some(Value::Ref(ptr))
        }
        "VirtualFree" => Some(Value::I32(1)),
        "VirtualProtect" => Some(Value::I32(1)),

        "HeapAlloc" | "LocalAlloc" | "GlobalAlloc" => {
            let size = match args.last() {
                Some(Value::I32(s)) => *s as usize,
                Some(Value::I64(s)) => *s as usize,
                _ => 0,
            };
            if size > 0 {
                let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                Some(Value::Ref(ptr))
            } else {
                Some(Value::Ref(std::ptr::null_mut()))
            }
        }
        "HeapFree" | "LocalFree" | "GlobalFree" => Some(Value::I32(1)),

        "GetEnvironmentVariableA" | "GetEnvironmentVariableW" => Some(Value::I32(0)),
        "SetEnvironmentVariableA" | "SetEnvironmentVariableW" => Some(Value::I32(1)),
        "GetCommandLineA" | "GetCommandLineW" => Some(Value::Ref(std::ptr::null_mut())),

        "ExitProcess" => {
            let code = match args.last() {
                Some(Value::I32(c)) => *c,
                _ => 0,
            };
            std::process::exit(code);
        }

        "InitializeCriticalSection"
        | "InitializeCriticalSectionAndSpinCount"
        | "EnterCriticalSection"
        | "LeaveCriticalSection"
        | "DeleteCriticalSection"
        | "TryEnterCriticalSection" => Some(Value::I32(1)),

        "CreateMutexA" | "CreateMutexW" | "CreateEventA" | "CreateEventW" => {
            Some(Value::I64(0x1000))
        }
        "SetEvent" | "ResetEvent" | "ReleaseMutex" => Some(Value::I32(1)),
        "WaitForSingleObject" => Some(Value::I32(0)),
        "WaitForMultipleObjects" => Some(Value::I32(0)),

        "GetSystemDirectoryA"
        | "GetSystemDirectoryW"
        | "GetWindowsDirectoryA"
        | "GetWindowsDirectoryW" => Some(Value::I32(0)),
        "GetTempPathA" | "GetTempPathW" => Some(Value::I32(0)),

        "OutputDebugStringA" | "OutputDebugStringW" => Some(Value::I32(0)),
        "IsDebuggerPresent" => Some(Value::I32(0)),

        "MultiByteToWideChar" => Some(Value::I32(0)),
        "WideCharToMultiByte" => Some(Value::I32(0)),

        "GetACP" => Some(Value::I32(65001)),
        "IsProcessorFeaturePresent" => Some(Value::I32(0)),
        "FlsAlloc" | "TlsAlloc" => Some(Value::I32(0)),
        "FlsSetValue" | "TlsSetValue" | "FlsFree" | "TlsFree" => Some(Value::I32(1)),
        "FlsGetValue" | "TlsGetValue" => Some(Value::I64(0)),

        _ => {
            crate::log_debug!("clr-pinvoke", "kernel32.{} non implémenté", name);
            Some(Value::I32(0))
        }
    }
}

// ── user32.dll ──

fn user32_dispatch(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "MessageBoxA" | "MessageBoxW" => {
            let text = match args.get(1) {
                Some(Value::Ref(p)) if !p.is_null() => {
                    if name.ends_with('W') {
                        read_managed_string(*p)
                    } else {
                        unsafe {
                            std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char)
                                .to_string_lossy()
                                .to_string()
                        }
                    }
                }
                _ => String::new(),
            };
            crate::log_info!("clr-pinvoke", "MessageBox: {}", text);
            println!("[MessageBox] {}", text);
            Some(Value::I32(1))
        }
        "GetDesktopWindow" => Some(Value::I64(0)),
        "GetForegroundWindow" => Some(Value::I64(0)),
        "ShowWindow" => Some(Value::I32(1)),
        "UpdateWindow" => Some(Value::I32(1)),
        "PostQuitMessage" => Some(Value::I32(0)),
        "DestroyWindow" => Some(Value::I32(1)),
        "SetWindowTextA" | "SetWindowTextW" => Some(Value::I32(1)),
        "GetSystemMetrics" => Some(Value::I32(0)),
        "GetDC" | "ReleaseDC" => Some(Value::I64(0)),
        _ => {
            crate::log_debug!("clr-pinvoke", "user32.{} non implémenté", name);
            Some(Value::I32(0))
        }
    }
}

// ── advapi32.dll ──

fn advapi32_dispatch(name: &str, _args: &[Value]) -> Option<Value> {
    match name {
        "RegOpenKeyExA" | "RegOpenKeyExW" => Some(Value::I32(2)),
        "RegCloseKey" => Some(Value::I32(0)),
        "RegQueryValueExA" | "RegQueryValueExW" => Some(Value::I32(2)),
        "RegSetValueExA" | "RegSetValueExW" => Some(Value::I32(0)),
        "GetUserNameA" | "GetUserNameW" => Some(Value::I32(1)),
        _ => {
            crate::log_debug!("clr-pinvoke", "advapi32.{} non implémenté", name);
            Some(Value::I32(0))
        }
    }
}

// ── msvcrt / ucrtbase ──

fn crt_dispatch(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "printf" | "puts" => {
            if let Some(Value::Ref(p)) = args.first() {
                if !p.is_null() {
                    let s = unsafe {
                        std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char).to_string_lossy()
                    };
                    print!("{}", s);
                    if name == "puts" {
                        println!();
                    }
                }
            }
            Some(Value::I32(0))
        }
        "malloc" => {
            let size = match args.last() {
                Some(Value::I32(s)) => *s as usize,
                Some(Value::I64(s)) => *s as usize,
                _ => 0,
            };
            if size > 0 {
                let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                Some(Value::Ref(ptr))
            } else {
                Some(Value::Ref(std::ptr::null_mut()))
            }
        }
        "free" | "calloc" | "realloc" => Some(Value::I32(0)),
        "memcpy" | "memmove" => args.first().cloned().or(Some(Value::I32(0))),
        "memset" => args.first().cloned().or(Some(Value::I32(0))),
        "strlen" => match args.first() {
            Some(Value::Ref(p)) if !p.is_null() => {
                let s = unsafe { std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char) };
                Some(Value::I32(s.to_bytes().len() as i32))
            }
            _ => Some(Value::I32(0)),
        },
        "strcmp" | "stricmp" | "_stricmp" => Some(Value::I32(0)),
        "atoi" | "atol" => Some(Value::I32(0)),
        "exit" | "_exit" => {
            let code = match args.last() {
                Some(Value::I32(c)) => *c,
                _ => 0,
            };
            std::process::exit(code);
        }
        "getenv" => Some(Value::Ref(std::ptr::null_mut())),
        "_errno" | "__errno_location" => Some(Value::Ref(std::ptr::null_mut())),
        _ => {
            crate::log_debug!("clr-pinvoke", "crt.{} non implémenté", name);
            Some(Value::I32(0))
        }
    }
}

// ── gdi32.dll ──

fn gdi32_dispatch(name: &str, _args: &[Value]) -> Option<Value> {
    crate::log_debug!("clr-pinvoke", "gdi32.{} non implémenté", name);
    Some(Value::I32(0))
}

// ── shell32.dll ──

fn shell32_dispatch(name: &str, _args: &[Value]) -> Option<Value> {
    crate::log_debug!("clr-pinvoke", "shell32.{} non implémenté", name);
    Some(Value::I32(0))
}

// ── ole32.dll ──

fn ole32_dispatch(name: &str, _args: &[Value]) -> Option<Value> {
    match name {
        "CoInitialize" | "CoInitializeEx" => Some(Value::I32(0)),
        "CoUninitialize" => Some(Value::I32(0)),
        "CoCreateInstance" => Some(Value::I32(0x80004002_u32 as i32)),
        _ => {
            crate::log_debug!("clr-pinvoke", "ole32.{} non implémenté", name);
            Some(Value::I32(0))
        }
    }
}

// ── ntdll.dll ──

fn ntdll_dispatch(name: &str, _args: &[Value]) -> Option<Value> {
    crate::log_debug!("clr-pinvoke", "ntdll.{} non implémenté", name);
    Some(Value::I32(0))
}
