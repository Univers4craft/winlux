//! Stubs pour dbghelp.dll — symboles de débogage et minidumps.
//! Electron et Chromium les utilisent pour leurs crash reporters.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn SymInitialize(_proc: usize, _path: *const u8, _invade: i32) -> i32 { 1 }
pub unsafe extern "win64" fn SymCleanup(_proc: usize) -> i32 { 1 }
pub extern "win64" fn SymGetOptions() -> u32 { 0 }
pub extern "win64" fn SymSetOptions(opts: u32) -> u32 { opts }
pub unsafe extern "win64" fn StackWalk64(
    _machine: u32, _proc: usize, _thread: usize, _frame: usize,
    _ctx: usize, _read_mem: usize, _fn_table: usize, _get_mod: usize, _translate: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn SymFromAddr(
    _proc: usize, _addr: u64, _disp: *mut u64, _info: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn SymGetLineFromAddr64(
    _proc: usize, _addr: u64, _disp: *mut u32, _line: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn UnDecorateSymbolName(
    _name: *const u8, _output: *mut u8, _max: u32, _flags: u32,
) -> u32 { 0 }
pub unsafe extern "win64" fn MiniDumpWriteDump(
    _proc: usize, _pid: u32, _file: usize, _type: u32,
    _exception: usize, _user_stream: usize, _callback: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn ImageNtHeader(_base: usize) -> usize { 0 }
pub unsafe extern "win64" fn SymInitializeW(_proc: usize, _path: *const u16, _invade: i32) -> i32 { 1 }

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SymInitialize" => fn SymInitialize,
        "SymInitializeW" => fn SymInitializeW,
        "SymCleanup" => fn SymCleanup,
        "SymGetOptions" => fn SymGetOptions,
        "SymSetOptions" => fn SymSetOptions,
        "StackWalk64" => fn StackWalk64,
        "SymFromAddr" => fn SymFromAddr,
        "SymGetLineFromAddr64" => fn SymGetLineFromAddr64,
        "UnDecorateSymbolName" => fn UnDecorateSymbolName,
        "MiniDumpWriteDump" => fn MiniDumpWriteDump,
        "ImageNtHeader" => fn ImageNtHeader,
    }
}
