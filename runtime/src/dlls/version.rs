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
    // Un nom nu (« msvcp140.dll ») suit l'ordre de recherche des DLL, comme
    // LoadLibraryEx sous Windows : dossier de l'appli, puis System32 (#2062).
    let on_disk = if path.contains(['\\', '/']) || path.as_bytes().get(1) == Some(&b':') {
        crate::paths::to_linux(path)
    } else {
        let importer = crate::loader::main_module()
            .map(|m| m.path)
            .unwrap_or_default();
        match crate::loader::find_dll_file(path, &importer) {
            Some(found) => found,
            None => return builtin_version(path).ok_or(ERROR_FILE_NOT_FOUND),
        }
    };
    match std::fs::read(on_disk) {
        Ok(file) => version_resource(&file).ok_or(ERROR_RESOURCE_TYPE_NOT_FOUND),
        Err(_) => builtin_version(path).ok_or(ERROR_FILE_NOT_FOUND),
    }
}

/// Nœud VS_VERSIONINFO : longueur, taille de la valeur, type, clé, valeur,
/// puis enfants, chacun aligné sur 4 octets.
fn version_node(
    key: &str,
    value: &[u8],
    value_len: u16,
    text: bool,
    children: &[Vec<u8>],
) -> Vec<u8> {
    let mut b = vec![0u8; 6];
    for c in key.encode_utf16().chain(Some(0)) {
        b.extend_from_slice(&c.to_le_bytes());
    }
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    b.extend_from_slice(value);
    for c in children {
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        b.extend_from_slice(c);
    }
    let len = b.len() as u16;
    b[0..2].copy_from_slice(&len.to_le_bytes());
    b[2..4].copy_from_slice(&value_len.to_le_bytes());
    b[4..6].copy_from_slice(&u16::from(text).to_le_bytes());
    b
}

fn text_node(key: &str, text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    version_node(key, &bytes, units.len() as u16, true, &[])
}

/// Version des DLL système de Winlux (Windows 10 22H2, 10.0.19045).
pub const SYSTEM_VERSION: (u16, u16, u16, u16) = (10, 0, 19045, 1);

/// Version du runtime Visual C++ servi par les DLL intégrées msvcp140,
/// vcruntime140, concrt140… : celle du redistribuable de Visual Studio 2022
/// 17.14 (14.44.35211.0). Les applis vérifient le numéro mineur : OBS
/// Studio exige 14.40 ou plus et refuse de démarrer sinon (#2062).
pub const VC_RUNTIME_VERSION: (u16, u16, u16, u16) = (14, 44, 35211, 0);

/// DLL du runtime Visual C++ (redistribuable 2015-2022), d'après son nom.
fn is_vc_runtime(file: &str) -> bool {
    let base = file.strip_suffix(".dll").unwrap_or(file);
    [
        "msvcp140",
        "vcruntime140",
        "concrt140",
        "vccorlib140",
        "vcomp140",
    ]
    .iter()
    .any(|p| base == *p || base.strip_prefix(p).is_some_and(|r| r.starts_with('_')))
}

/// DLL système intégrée (kernel32.dll, kernelbase.dll…), sans fichier sur
/// disque : Windows y trouve une ressource de version, que Chromium exige
/// (base::win::OSInfo lit celle de kernelbase.dll, puis de kernel32.dll,
/// et s'arrête sur un CHECK si les deux manquent).
fn builtin_version(path: &str) -> Option<Vec<u8>> {
    let file = path.rsplit(['\\', '/']).next()?;
    let lower = file.to_ascii_lowercase();
    if !(lower.ends_with(".dll") || lower.ends_with(".drv") || !lower.contains('.')) {
        return None;
    }
    let dir = path[..path.len() - file.len()]
        .to_ascii_lowercase()
        .replace('/', "\\");
    let system_dir = dir.is_empty()
        || dir.ends_with("\\windows\\system32\\")
        || dir.ends_with("\\windows\\syswow64\\")
        || dir.ends_with("\\windows\\");
    if !system_dir || !crate::dlls::is_builtin(&crate::dlls::normalize(file)) {
        return None;
    }
    let (a, b, c, d) = if is_vc_runtime(&lower) {
        VC_RUNTIME_VERSION
    } else {
        SYSTEM_VERSION
    };
    let ms = (u32::from(a) << 16) | u32::from(b);
    let ls = (u32::from(c) << 16) | u32::from(d);
    let mut fixed = Vec::with_capacity(52);
    for v in [
        0xFEEF_04BDu32, // signature
        0x0001_0000,    // version de la structure
        ms,
        ls,
        ms,
        ls,
        0x3F,                                        // masque des indicateurs
        0,                                           // indicateurs
        0x0004_0004,                                 // VOS_NT_WINDOWS32
        if lower.ends_with(".drv") { 3 } else { 2 }, // VFT_DRV / VFT_DLL
        0,
        0,
        0,
    ] {
        fixed.extend_from_slice(&v.to_le_bytes());
    }
    let version = format!("{a}.{b}.{c}.{d}");
    let table = version_node(
        "040904B0",
        &[],
        0,
        true,
        &[
            text_node("CompanyName", "Winlux"),
            text_node("FileDescription", file),
            text_node("FileVersion", &version),
            text_node("InternalName", file),
            text_node("OriginalFilename", file),
            text_node("ProductName", "Winlux"),
            text_node("ProductVersion", &version),
        ],
    );
    let strings = version_node("StringFileInfo", &[], 0, true, &[table]);
    let translation = version_node("Translation", &[0x09, 0x04, 0xB0, 0x04], 4, false, &[]);
    let vars = version_node("VarFileInfo", &[], 0, true, &[translation]);
    Some(version_node(
        "VS_VERSION_INFO",
        &fixed,
        52,
        false,
        &[strings, vars],
    ))
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

    #[test]
    fn version_des_dll_systeme() {
        let _teb = crate::process::test_teb();
        unsafe {
            for name in [
                "C:\\Windows\\System32\\kernelbase.dll",
                "kernel32.dll",
                "c:/windows/system32/USER32.DLL",
            ] {
                let path = wide(name);
                let mut h = 7u32;
                let size = get_file_version_info_size_w(path.as_ptr(), &mut h);
                assert!(size > 52, "{name}");
                assert_eq!(h, 0);
                let mut buf = vec![0u8; size as usize];
                assert_eq!(
                    get_file_version_info_w(path.as_ptr(), 0, size, buf.as_mut_ptr()),
                    1
                );
                let mut p: *mut u8 = std::ptr::null_mut();
                let mut n = 0u32;
                let root = wide("\\");
                assert_eq!(
                    ver_query_value_w(buf.as_ptr(), root.as_ptr(), &mut p, &mut n),
                    1
                );
                assert_eq!(n, 52);
                let fixed = std::slice::from_raw_parts(p as *const u32, 13);
                assert_eq!(fixed[0], 0xFEEF_04BD);
                assert_eq!(fixed[2], 10 << 16);
                assert_eq!(fixed[3] >> 16, 19045);
                let key = wide("\\StringFileInfo\\040904B0\\FileVersion");
                assert_eq!(
                    ver_query_value_w(buf.as_ptr(), key.as_ptr(), &mut p, &mut n),
                    1
                );
                let text: Vec<u16> =
                    std::slice::from_raw_parts(p as *const u16, n as usize - 1).to_vec();
                assert_eq!(String::from_utf16_lossy(&text), "10.0.19045.1");
            }
            // Ni une DLL inconnue, ni une DLL intégrée hors du dossier système.
            let other = wide("C:\\Program Files\\x\\kernel32.dll");
            assert_eq!(
                get_file_version_info_size_w(other.as_ptr(), std::ptr::null_mut()),
                0
            );
            let unknown = wide("C:\\Windows\\System32\\pas-une-dll-winlux.dll");
            assert_eq!(
                get_file_version_info_size_w(unknown.as_ptr(), std::ptr::null_mut()),
                0
            );
        }
    }

    #[test]
    fn runtime_visual_cpp_en_version_14() {
        // OBS Studio lit la version de « msvcp140.dll » et exige 14.40 ou plus (#2062).
        let _teb = crate::process::test_teb();
        assert!(is_vc_runtime("msvcp140.dll"));
        assert!(is_vc_runtime("vcruntime140_1.dll"));
        assert!(!is_vc_runtime("msvcp140x.dll"));
        assert!(!is_vc_runtime("kernel32.dll"));
        unsafe {
            for name in ["msvcp140.dll", "C:\\Windows\\System32\\vcruntime140.dll"] {
                let path = wide(name);
                let size = get_file_version_info_size_w(path.as_ptr(), std::ptr::null_mut());
                assert!(size > 52, "{name}");
                let mut buf = vec![0u8; size as usize];
                assert_eq!(
                    get_file_version_info_w(path.as_ptr(), 0, size, buf.as_mut_ptr()),
                    1
                );
                let mut p: *mut u8 = std::ptr::null_mut();
                let mut n = 0u32;
                let root = wide("\\");
                assert_eq!(
                    ver_query_value_w(buf.as_ptr(), root.as_ptr(), &mut p, &mut n),
                    1
                );
                let fixed = std::slice::from_raw_parts(p as *const u32, 13);
                assert_eq!(fixed[2] >> 16, 14, "{name}");
                assert!(fixed[2] & 0xFFFF >= 40, "{name}");
            }
        }
    }

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
