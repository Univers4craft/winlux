//! version.dll : informations de version des fichiers (ressource RT_VERSION).
//!
//! La ressource est lue dans le fichier sur disque (PE32 ou PE32+), sans le
//! charger : GetFileVersionInfoSize/GetFileVersionInfo copient le bloc
//! VS_VERSIONINFO, VerQueryValue le parcourt (`\`, `\VarFileInfo\Translation`,
//! `\StringFileInfo\<langue>\<nom>`).

use crate::pe::{rd16, rd32};

const RT_VERSION: u32 = 16;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_RESOURCE_TYPE_NOT_FOUND: u32 = 1813;

/// Bloc VS_VERSIONINFO brut d'un fichier PE.
fn version_resource(file: &[u8]) -> Option<Vec<u8>> {
    let pe = rd32(file, 0x3c)? as usize;
    if file.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let nsec = rd16(file, pe + 6)? as usize;
    let opt_size = rd16(file, pe + 20)? as usize;
    let opt = pe + 24;
    let dirs = match rd16(file, opt)? {
        0x10b => opt + 96,
        0x20b => opt + 112,
        _ => return None,
    };
    let rsrc_rva = rd32(file, dirs + 2 * 8)?;
    if rsrc_rva == 0 {
        return None;
    }
    let sections = opt + opt_size;
    let to_file = |rva: u32| -> Option<usize> {
        (0..nsec).find_map(|i| {
            let s = sections + i * 40;
            let vsize = rd32(file, s + 8)?;
            let va = rd32(file, s + 12)?;
            let raw_size = rd32(file, s + 16)?;
            let raw = rd32(file, s + 20)?;
            (rva >= va && rva < va + vsize.max(raw_size)).then(|| (raw + (rva - va)) as usize)
        })
    };
    let root = to_file(rsrc_rva)?;
    // Type → nom (premier) → langue (première), comme Windows sans préférence.
    let first_child = |dir: usize, wanted: Option<u32>| -> Option<u32> {
        let named = rd16(file, dir + 12)? as usize;
        let ids = rd16(file, dir + 14)? as usize;
        (0..named + ids).find_map(|i| {
            let e = dir + 16 + i * 8;
            let id = rd32(file, e)?;
            (wanted.is_none() || (i >= named && Some(id) == wanted)).then(|| rd32(file, e + 4))?
        })
    };
    let ty = first_child(root, Some(RT_VERSION))?;
    let names = root + (ty & 0x7FFF_FFFF) as usize;
    let name = first_child(names, None)?;
    let langs = root + (name & 0x7FFF_FFFF) as usize;
    let data = first_child(langs, None)?;
    if data & 0x8000_0000 != 0 {
        return None;
    }
    let entry = root + data as usize;
    let at = to_file(rd32(file, entry)?)?;
    let size = rd32(file, entry + 4)? as usize;
    file.get(at..at.checked_add(size)?).map(<[u8]>::to_vec)
}

fn read_version(path: &str) -> Result<Vec<u8>, u32> {
    let file = std::fs::read(crate::paths::to_linux(path)).map_err(|_| ERROR_FILE_NOT_FOUND)?;
    version_resource(&file).ok_or(ERROR_RESOURCE_TYPE_NOT_FOUND)
}

unsafe fn size_of(path: String, handle: *mut u32) -> u32 {
    if !handle.is_null() {
        *handle = 0;
    }
    match read_version(&path) {
        // Comme Windows, de la place en plus pour une conversion ANSI éventuelle.
        Ok(v) => (v.len() * 2) as u32,
        Err(e) => {
            crate::process::set_last_error(e);
            0
        }
    }
}

unsafe fn copy_into(path: String, len: u32, data: *mut u8) -> i32 {
    if data.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    match read_version(&path) {
        Ok(v) => {
            let n = v.len().min(len as usize);
            std::ptr::copy_nonoverlapping(v.as_ptr(), data, n);
            if (len as usize) > n {
                std::ptr::write_bytes(data.add(n), 0, len as usize - n);
            }
            1
        }
        Err(e) => {
            crate::process::set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn get_file_version_info_size_a(
    file: *const u8,
    handle: *mut u32,
) -> u32 {
    size_of(crate::dlls::cstr(file), handle)
}
pub unsafe extern "win64" fn get_file_version_info_size_w(
    file: *const u16,
    handle: *mut u32,
) -> u32 {
    size_of(crate::dlls::wstr(file), handle)
}
pub unsafe extern "win64" fn get_file_version_info_size_ex_w(
    _flags: u32,
    file: *const u16,
    handle: *mut u32,
) -> u32 {
    size_of(crate::dlls::wstr(file), handle)
}
pub unsafe extern "win64" fn get_file_version_info_size_ex_a(
    _flags: u32,
    file: *const u8,
    handle: *mut u32,
) -> u32 {
    size_of(crate::dlls::cstr(file), handle)
}
pub unsafe extern "win64" fn get_file_version_info_a(
    file: *const u8,
    _handle: u32,
    len: u32,
    data: *mut u8,
) -> i32 {
    copy_into(crate::dlls::cstr(file), len, data)
}
pub unsafe extern "win64" fn get_file_version_info_w(
    file: *const u16,
    _handle: u32,
    len: u32,
    data: *mut u8,
) -> i32 {
    copy_into(crate::dlls::wstr(file), len, data)
}
pub unsafe extern "win64" fn get_file_version_info_ex_w(
    _flags: u32,
    file: *const u16,
    _handle: u32,
    len: u32,
    data: *mut u8,
) -> i32 {
    copy_into(crate::dlls::wstr(file), len, data)
}
pub unsafe extern "win64" fn get_file_version_info_ex_a(
    _flags: u32,
    file: *const u8,
    _handle: u32,
    len: u32,
    data: *mut u8,
) -> i32 {
    copy_into(crate::dlls::cstr(file), len, data)
}

// --- Parcours du bloc VS_VERSIONINFO ---------------------------------------------

/// Nœud du bloc : (clé, décalage de la valeur, taille de la valeur en
/// octets, texte ?, début des enfants, fin du nœud).
struct Node {
    key: String,
    value: usize,
    value_len: usize,
    text: bool,
    children: usize,
    end: usize,
}

fn align4(x: usize) -> usize {
    (x + 3) & !3
}

fn node(b: &[u8], at: usize, limit: usize) -> Option<Node> {
    let len = rd16(b, at)? as usize;
    let vlen = rd16(b, at + 2)? as usize;
    let ty = rd16(b, at + 4)?;
    if len < 6 || at + len > limit {
        return None;
    }
    let mut key = Vec::new();
    let mut p = at + 6;
    loop {
        let c = rd16(b, p)?;
        p += 2;
        if c == 0 {
            break;
        }
        key.push(c);
    }
    let value = align4(p);
    let value_len = if ty == 1 { vlen * 2 } else { vlen };
    let end = at + len;
    Some(Node {
        key: String::from_utf16_lossy(&key),
        value,
        value_len,
        text: ty == 1,
        children: align4(value + value_len).min(end),
        end,
    })
}

/// Valeur désignée par `path` : (décalage, taille en octets, texte ?).
fn query(b: &[u8], path: &str) -> Option<(usize, usize, bool)> {
    let mut n = node(b, 0, b.len())?;
    for part in path.split('\\').filter(|p| !p.is_empty()) {
        let mut at = n.children;
        let mut found = None;
        while at + 6 <= n.end {
            let c = node(b, at, n.end)?;
            if c.key.eq_ignore_ascii_case(part) {
                found = Some(c);
                break;
            }
            at = align4(c.end);
        }
        n = found?;
    }
    if n.value_len == 0 {
        return None;
    }
    Some((n.value, n.value_len, n.text))
}

pub unsafe extern "win64" fn ver_query_value_w(
    block: *const u8,
    sub: *const u16,
    value: *mut *mut u8,
    len: *mut u32,
) -> i32 {
    if block.is_null() || value.is_null() {
        return 0;
    }
    let total = rd16(std::slice::from_raw_parts(block, 2), 0).unwrap_or(0) as usize;
    let b = std::slice::from_raw_parts(block, total);
    let path = if sub.is_null() {
        String::from("\\")
    } else {
        crate::dlls::wstr(sub)
    };
    match query(b, &path) {
        Some((at, n, text)) => {
            *value = block.add(at) as *mut u8;
            if !len.is_null() {
                // Texte : longueur en caractères, zéro final compris.
                *len = if text { (n / 2) as u32 } else { n as u32 };
            }
            1
        }
        None => {
            *value = std::ptr::null_mut();
            if !len.is_null() {
                *len = 0;
            }
            0
        }
    }
}

/// VerQueryValueA : valeurs binaires (`\`, traductions) seulement ; les
/// chaînes sont converties dans la place libre après le bloc.
pub unsafe extern "win64" fn ver_query_value_a(
    block: *const u8,
    sub: *const u8,
    value: *mut *mut u8,
    len: *mut u32,
) -> i32 {
    if block.is_null() || value.is_null() {
        return 0;
    }
    let total = rd16(std::slice::from_raw_parts(block, 2), 0).unwrap_or(0) as usize;
    let b = std::slice::from_raw_parts(block, total);
    let path = if sub.is_null() {
        String::from("\\")
    } else {
        crate::dlls::cstr(sub)
    };
    let Some((at, n, text)) = query(b, &path) else {
        *value = std::ptr::null_mut();
        if !len.is_null() {
            *len = 0;
        }
        return 0;
    };
    if !text {
        *value = block.add(at) as *mut u8;
        if !len.is_null() {
            *len = n as u32;
        }
        return 1;
    }
    // GetFileVersionInfo réserve autant de place que le bloc lui-même : la
    // chaîne ANSI y est écrite au même décalage, dans la seconde moitié.
    let wide = std::slice::from_raw_parts(block.add(at) as *const u16, n / 2);
    let s = String::from_utf16_lossy(wide);
    let s = s.trim_end_matches('\0');
    let dst = (block as *mut u8).add(total + at / 2);
    let written = crate::dlls::put_a(s, dst, (n / 2) as u32);
    *value = dst;
    if !len.is_null() {
        *len = written + 1;
    }
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "GetFileVersionInfoSizeA"=>fn get_file_version_info_size_a,"GetFileVersionInfoA"=>fn get_file_version_info_a,
     "GetFileVersionInfoSizeW"=>fn get_file_version_info_size_w,"GetFileVersionInfoW"=>fn get_file_version_info_w,
     "GetFileVersionInfoSizeExW"=>fn get_file_version_info_size_ex_w,"GetFileVersionInfoExW"=>fn get_file_version_info_ex_w,
     "GetFileVersionInfoSizeExA"=>fn get_file_version_info_size_ex_a,"GetFileVersionInfoExA"=>fn get_file_version_info_ex_a,
     "VerQueryValueA"=>fn ver_query_value_a,"VerQueryValueW"=>fn ver_query_value_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Nœud VS_VERSIONINFO sérialisé (alignements compris).
    fn put_node(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 6];
        for c in key.encode_utf16().chain(Some(0)) {
            b.extend_from_slice(&c.to_le_bytes());
        }
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b.extend_from_slice(value);
        for c in children {
            while b.len() % 4 != 0 {
                b.push(0);
            }
            b.extend_from_slice(c);
        }
        let vlen = if text { value.len() / 2 } else { value.len() };
        let total = b.len() as u16;
        b[0..2].copy_from_slice(&total.to_le_bytes());
        b[2..4].copy_from_slice(&(vlen as u16).to_le_bytes());
        b[4..6].copy_from_slice(&(text as u16).to_le_bytes());
        b
    }

    fn sample() -> Vec<u8> {
        let mut fixed = vec![0u8; 52];
        fixed[0..4].copy_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        fixed[8..12].copy_from_slice(&0x0005_0001u32.to_le_bytes());
        let ver: Vec<u8> = wide("5.1.12")
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let s = put_node("ProductVersion", &ver, true, &[]);
        let table = put_node("040904b0", &[], true, &[s]);
        let sfi = put_node("StringFileInfo", &[], true, &[table]);
        let tr = put_node("Translation", &[0x09, 0x04, 0xb0, 0x04], false, &[]);
        let vfi = put_node("VarFileInfo", &[], true, &[tr]);
        put_node("VS_VERSION_INFO", &fixed, false, &[sfi, vfi])
    }

    #[test]
    fn interroge_un_bloc_de_version() {
        let mut b = sample();
        let total = b.len();
        b.resize(total * 2, 0);
        unsafe {
            let mut v = std::ptr::null_mut();
            let mut n = 0u32;
            let root = wide("\\");
            assert_eq!(
                ver_query_value_w(b.as_ptr(), root.as_ptr(), &mut v, &mut n),
                1
            );
            assert_eq!(n, 52);
            assert_eq!((v as *const u32).read_unaligned(), 0xFEEF_04BD);
            let tr = wide("\\VarFileInfo\\Translation");
            assert_eq!(
                ver_query_value_w(b.as_ptr(), tr.as_ptr(), &mut v, &mut n),
                1
            );
            assert_eq!((n, (v as *const u32).read_unaligned()), (4, 0x04b0_0409));
            let pv = wide("\\StringFileInfo\\040904B0\\ProductVersion");
            assert_eq!(
                ver_query_value_w(b.as_ptr(), pv.as_ptr(), &mut v, &mut n),
                1
            );
            let text = std::slice::from_raw_parts(v as *const u16, n as usize - 1);
            assert_eq!(String::from_utf16_lossy(text), "5.1.12");
            let pa = b"\\StringFileInfo\\040904b0\\ProductVersion\0";
            assert_eq!(
                ver_query_value_a(b.as_ptr(), pa.as_ptr(), &mut v, &mut n),
                1
            );
            assert_eq!(
                std::ffi::CStr::from_ptr(v as *const _).to_bytes(),
                b"5.1.12"
            );
            let missing = wide("\\StringFileInfo\\040904b0\\Absent");
            assert_eq!(
                ver_query_value_w(b.as_ptr(), missing.as_ptr(), &mut v, &mut n),
                0
            );
        }
    }

    #[test]
    fn trouve_la_ressource_dans_un_fichier_pe() {
        // PE32+ minimal : une section .rsrc avec type 16 → id 1 → langue 0x409.
        let block = sample();
        let mut f = vec![0u8; 0x400];
        f[..2].copy_from_slice(b"MZ");
        f[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        f[0x80..0x84].copy_from_slice(b"PE\0\0");
        f[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        f[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        f[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        f[opt + 112 + 16..opt + 116 + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        let sec = opt + 240;
        let put = |f: &mut Vec<u8>, o: usize, v: u32| f[o..o + 4].copy_from_slice(&v.to_le_bytes());
        f[sec..sec + 5].copy_from_slice(b".rsrc");
        put(&mut f, sec + 8, 0x1000);
        put(&mut f, sec + 12, 0x1000);
        put(&mut f, sec + 16, 0x200);
        put(&mut f, sec + 20, 0x200);
        let r = 0x200;
        f.resize(0x400 + block.len(), 0);
        // Racine : une entrée id 16 → sous-répertoire 0x18.
        f[r + 14] = 1;
        put(&mut f, r + 16, 16);
        put(&mut f, r + 20, 0x8000_0018);
        f[r + 0x18 + 14] = 1;
        put(&mut f, r + 0x18 + 16, 1);
        put(&mut f, r + 0x18 + 20, 0x8000_0030);
        f[r + 0x30 + 14] = 1;
        put(&mut f, r + 0x30 + 16, 0x409);
        put(&mut f, r + 0x30 + 20, 0x48);
        put(&mut f, r + 0x48, 0x1100);
        put(&mut f, r + 0x48 + 4, block.len() as u32);
        f[0x300..0x300 + block.len()].copy_from_slice(&block);
        assert_eq!(version_resource(&f).as_deref(), Some(&block[..]));
        assert!(version_resource(&f[..0x200]).is_none());
    }
}
