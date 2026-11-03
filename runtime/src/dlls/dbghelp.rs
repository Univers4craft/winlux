//! Stubs pour dbghelp.dll — symboles de débogage et minidumps.
//! Electron et Chromium les utilisent pour leurs crash reporters.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn SymInitialize(_proc: usize, _path: *const u8, _invade: i32) -> i32 {
    1
}
pub unsafe extern "win64" fn SymCleanup(_proc: usize) -> i32 {
    1
}
pub extern "win64" fn SymGetOptions() -> u32 {
    0
}
pub extern "win64" fn SymSetOptions(opts: u32) -> u32 {
    opts
}
pub unsafe extern "win64" fn StackWalk64(
    _machine: u32,
    _proc: usize,
    _thread: usize,
    _frame: usize,
    _ctx: usize,
    _read_mem: usize,
    _fn_table: usize,
    _get_mod: usize,
    _translate: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn SymFromAddr(
    _proc: usize,
    _addr: u64,
    _disp: *mut u64,
    _info: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn SymGetLineFromAddr64(
    _proc: usize,
    _addr: u64,
    _disp: *mut u32,
    _line: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn UnDecorateSymbolName(
    _name: *const u8,
    _output: *mut u8,
    _max: u32,
    _flags: u32,
) -> u32 {
    0
}
pub unsafe extern "win64" fn MiniDumpWriteDump(
    _proc: usize,
    _pid: u32,
    _file: usize,
    _type: u32,
    _exception: usize,
    _user_stream: usize,
    _callback: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn ImageNtHeader(_base: usize) -> usize {
    0
}
pub unsafe extern "win64" fn SymInitializeW(_proc: usize, _path: *const u16, _invade: i32) -> i32 {
    1
}

/// Module qui contient `addr` : (base, taille, horodatage, nom court, chemin Windows).
fn module_at(addr: u64) -> Option<(u64, u32, u32, String, String)> {
    let m = crate::loader::find_module_by_addr(addr as usize)?;
    // SAFETY: l'image est chargée ; e_lfanew puis FileHeader.TimeDateStamp.
    let stamp = unsafe {
        let pe = std::ptr::read_unaligned((m.base + 0x3C) as *const u32) as usize;
        std::ptr::read_unaligned((m.base + pe + 8) as *const u32)
    };
    let short = m
        .name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&m.name)
        .rsplit_once('.')
        .map_or(m.name.clone(), |(n, _)| n.to_string());
    let path = crate::paths::to_windows(&m.path);
    Some((m.base as u64, m.size as u32, stamp, short, path))
}

/// IMAGEHLP_MODULE64 : champs remplis dans la limite de SizeOfStruct.
/// Sans fichier de symboles, SymType vaut SymExport (les exports du PE).
unsafe fn module_info(addr: u64, out: *mut u8, wide: bool) -> i32 {
    let Some((base, size, stamp, short, path)) = module_at(addr) else {
        crate::process::set_last_error(126); // ERROR_MOD_NOT_FOUND
        return 0;
    };
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let cap = (std::ptr::read_unaligned(out as *const u32) as usize).min(0x1000);
    if cap < 36 {
        crate::process::set_last_error(87);
        return 0;
    }
    std::ptr::write_bytes(out.add(4), 0, cap - 4);
    std::ptr::write_unaligned(out.add(8) as *mut u64, base);
    std::ptr::write_unaligned(out.add(16) as *mut u32, size);
    std::ptr::write_unaligned(out.add(20) as *mut u32, stamp);
    std::ptr::write_unaligned(out.add(32) as *mut u32, 4); // SymExport
    let put = |at: usize, chars: usize, text: &str| {
        let unit = if wide { 2 } else { 1 };
        if at + chars * unit > cap {
            return;
        }
        if wide {
            for (i, c) in text.encode_utf16().take(chars - 1).enumerate() {
                std::ptr::write_unaligned(out.add(at + i * 2) as *mut u16, c);
            }
        } else {
            let b = crate::dlls::encode_ansi(text);
            for (i, c) in b.iter().take(chars - 1).enumerate() {
                *out.add(at + i) = *c;
            }
        }
    };
    let u = if wide { 2 } else { 1 };
    put(36, 32, &short);
    put(36 + 32 * u, 256, &path);
    put(36 + 32 * u + 256 * u, 256, &path);
    1
}

pub unsafe extern "win64" fn SymGetModuleInfo64(_proc: usize, addr: u64, out: *mut u8) -> i32 {
    module_info(addr, out, false)
}

pub unsafe extern "win64" fn SymGetModuleInfoW64(_proc: usize, addr: u64, out: *mut u8) -> i32 {
    module_info(addr, out, true)
}

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
        "SymGetModuleInfo64" => fn SymGetModuleInfo64,
        "SymGetModuleInfoW64" => fn SymGetModuleInfoW64,
    }
}
