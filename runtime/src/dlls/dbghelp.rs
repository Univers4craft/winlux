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
/// ImageNtHeader : adresse des en-têtes NT (« PE\0\0 ») d'une image ou
/// d'un fichier projeté en mémoire, ou NULL. Notepad++ y lit le type de
/// machine de chaque extension avant de la charger : NULL donnait « Cannot
/// load 64-bit plugin » pour chacune (#1152).
pub unsafe extern "win64" fn ImageNtHeader(base: usize) -> usize {
    if base == 0 || std::ptr::read_unaligned(base as *const u16) != 0x5A4D {
        return 0;
    }
    let lfanew = std::ptr::read_unaligned((base + 0x3C) as *const u32) as usize;
    if lfanew == 0 || lfanew > 0x1000_0000 {
        return 0;
    }
    let nt = base + lfanew;
    if std::ptr::read_unaligned(nt as *const u32) != 0x0000_4550 {
        return 0;
    }
    nt
}
/// Chemin de recherche des symboles : Winlux ne charge pas de .pdb.
pub unsafe extern "win64" fn SymSetSearchPath(_proc: usize, _path: *const u8) -> i32 {
    1
}
pub unsafe extern "win64" fn SymRefreshModuleList(_proc: usize) -> i32 {
    1
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

/// EnumerateLoadedModules64(process, callback, context) : rappel
/// (nom ANSI, base, taille, contexte) pour chaque module PE chargé.
pub unsafe extern "win64" fn EnumerateLoadedModules64(
    _proc: usize,
    callback: usize,
    context: usize,
) -> i32 {
    if callback == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    type Cb = unsafe extern "win64" fn(*const u8, u64, u32, usize) -> i32;
    let cb: Cb = std::mem::transmute(callback);
    for m in crate::loader::list_modules() {
        let mut name = crate::dlls::encode_ansi(&m.name);
        name.push(0);
        if cb(name.as_ptr(), m.base as u64, m.size as u32, context) == 0 {
            break;
        }
    }
    1
}

/// SymGetSymFromAddr64 : sans fichier de symboles, aucun nom à donner.
pub unsafe extern "win64" fn SymGetSymFromAddr64(
    _proc: usize,
    _addr: u64,
    _disp: *mut u64,
    _symbol: usize,
) -> i32 {
    crate::process::set_last_error(487); // ERROR_INVALID_ADDRESS
    0
}

/// Somme de contrôle d'une image PE (champ CheckSum de l'en-tête optionnel) :
/// somme des mots de 16 bits avec retenue, sans le champ lui-même, plus la
/// longueur du fichier.
pub fn pe_checksum(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 0x40 || data[0] != b'M' || data[1] != b'Z' {
        return None;
    }
    let pe = u32::from_le_bytes(data[0x3C..0x40].try_into().ok()?) as usize;
    let field = pe.checked_add(24 + 64)?;
    if data.get(pe..pe + 4)? != b"PE\0\0" || field + 4 > data.len() {
        return None;
    }
    let header = u32::from_le_bytes(data[field..field + 4].try_into().ok()?);
    let mut sum: u64 = 0;
    for (i, c) in data.chunks(2).enumerate() {
        if i * 2 == field || i * 2 == field + 2 {
            continue;
        }
        let w = u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]) as u64;
        sum += w;
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    sum = (sum & 0xFFFF) + (sum >> 16);
    Some((header, sum as u32 + data.len() as u32))
}

/// CheckSumMappedFile(base, length, &ancienne, &nouvelle) -> en-têtes NT.
pub unsafe extern "win64" fn CheckSumMappedFile(
    base: usize,
    len: u32,
    header_sum: *mut u32,
    check_sum: *mut u32,
) -> usize {
    if base == 0 || header_sum.is_null() || check_sum.is_null() {
        return 0;
    }
    let data = std::slice::from_raw_parts(base as *const u8, len as usize);
    let Some((old, new)) = pe_checksum(data) else {
        return 0;
    };
    *header_sum = old;
    *check_sum = new;
    ImageNtHeader(base)
}

unsafe fn map_file_and_checksum(path: std::path::PathBuf, header: *mut u32, sum: *mut u32) -> u32 {
    const CHECKSUM_OPEN_FAILURE: u32 = 1;
    const CHECKSUM_MAP_FAILURE: u32 = 2;
    if header.is_null() || sum.is_null() {
        return CHECKSUM_MAP_FAILURE;
    }
    let Ok(data) = std::fs::read(path) else {
        return CHECKSUM_OPEN_FAILURE;
    };
    match pe_checksum(&data) {
        Some((old, new)) => {
            *header = old;
            *sum = new;
            0
        }
        None => CHECKSUM_MAP_FAILURE,
    }
}

pub unsafe extern "win64" fn MapFileAndCheckSumW(
    path: *const u16,
    header: *mut u32,
    sum: *mut u32,
) -> u32 {
    if path.is_null() {
        return 1;
    }
    map_file_and_checksum(
        crate::paths::to_linux(&crate::dlls::wstr(path)),
        header,
        sum,
    )
}

pub unsafe extern "win64" fn MapFileAndCheckSumA(
    path: *const u8,
    header: *mut u32,
    sum: *mut u32,
) -> u32 {
    if path.is_null() {
        return 1;
    }
    map_file_and_checksum(
        crate::paths::to_linux(&crate::dlls::cstr(path)),
        header,
        sum,
    )
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SymInitialize" => fn SymInitialize,
        "SymInitializeW" => fn SymInitializeW,
        "SymCleanup" => fn SymCleanup,
        "SymSetSearchPath" => fn SymSetSearchPath,
        "SymRefreshModuleList" => fn SymRefreshModuleList,
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
        "EnumerateLoadedModules64" => fn EnumerateLoadedModules64,
        "SymGetSymFromAddr64" => fn SymGetSymFromAddr64,
        "CheckSumMappedFile" => fn CheckSumMappedFile,
        "MapFileAndCheckSumW" => fn MapFileAndCheckSumW,
        "MapFileAndCheckSumA" => fn MapFileAndCheckSumA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_tetes_nt_d_une_image_projetee() {
        // #1152 : Notepad++ lit Machine dans les en-têtes rendus ici.
        let mut img = vec![0u8; 0x200];
        img[0] = b'M';
        img[1] = b'Z';
        img[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        img[0x80..0x84].copy_from_slice(b"PE\0\0");
        img[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        let base = img.as_ptr() as usize;
        let nt = unsafe { ImageNtHeader(base) };
        assert_eq!(nt, base + 0x80);
        let machine = unsafe { std::ptr::read_unaligned((nt + 4) as *const u16) };
        assert_eq!(machine, 0x8664);
        img[0x80] = b'X';
        assert_eq!(unsafe { ImageNtHeader(img.as_ptr() as usize) }, 0);
        img[0] = 0;
        assert_eq!(unsafe { ImageNtHeader(img.as_ptr() as usize) }, 0);
        assert_eq!(unsafe { ImageNtHeader(0) }, 0);
    }
}
