//! Variantes ANSI de shell32 que Winlux ne servait qu'en Unicode (#1841).
//!
//! L'outil de déploiement d'Office (x86) importe `SHFileOperationA` en
//! chargement différé : sans elle, `__delayLoadHelper2` lève 0xC06D007F et
//! l'extraction s'arrête. Chaque variante convertit ses chaînes depuis la
//! page ANSI et appelle la fonction `W` existante. Prototypes et structures :
//! documentation publique de Microsoft et shellapi.h de MinGW-w64.

use super::{cstr, encode_ansi, Exports};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Chaîne ANSI de l'appelant en UTF-16 terminé par zéro ; `None` si nulle.
unsafe fn wide_of(p: *const u8) -> Option<Vec<u16>> {
    (!p.is_null()).then(|| wide(&cstr(p)))
}

fn as_ptr(v: &Option<Vec<u16>>) -> *const u16 {
    v.as_ref().map_or(std::ptr::null(), |v| v.as_ptr())
}

/// SHFileOperationA : SHFILEOPSTRUCTA, chemins en listes double-zéro ANSI.
unsafe extern "win64" fn sh_file_operation_a(op: *mut u8) -> i32 {
    super::apps_win32::sh_file_operation(op, false)
}

/// ExtractIconExA : comme la variante W, aucune icône extraite.
unsafe extern "win64" fn extract_icon_ex_a(
    file: *const u8,
    index: i32,
    large: *mut usize,
    small: *mut usize,
    n: u32,
) -> u32 {
    let f = wide_of(file);
    super::apps_win32::extract_icon_ex_w(as_ptr(&f), index, large, small, n)
}

/// ILCreateFromPathA.
unsafe extern "win64" fn il_create_from_path_a(path: *const u8) -> usize {
    let p = wide_of(path);
    super::apps_win32::il_create_from_path_w(as_ptr(&p))
}

/// FindExecutableA : `out` reçoit au plus MAX_PATH octets ANSI.
unsafe extern "win64" fn find_executable_a(file: *const u8, dir: *const u8, out: *mut u8) -> usize {
    if out.is_null() {
        return super::apps_win32::find_executable_w(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
        );
    }
    let f = wide_of(file);
    let d = wide_of(dir);
    let mut buf = [0u16; 260];
    let r = super::apps_win32::find_executable_w(as_ptr(&f), as_ptr(&d), buf.as_mut_ptr());
    let mut bytes = encode_ansi(&super::wstr(buf.as_ptr()));
    bytes.truncate(259);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out.add(bytes.len()) = 0;
    r
}

/// SHQueryRecycleBinA : la racine n'est pas lue, comme en W.
unsafe extern "win64" fn sh_query_recycle_bin_a(root: *const u8, info: *mut u8) -> i32 {
    let r = wide_of(root);
    super::shell32_releve::sh_query_recycle_bin_w(as_ptr(&r), info)
}

/// SHEmptyRecycleBinA.
unsafe extern "win64" fn sh_empty_recycle_bin_a(hwnd: usize, root: *const u8, flags: u32) -> i32 {
    let r = wide_of(root);
    super::shell32_releve::sh_empty_recycle_bin_w(hwnd, as_ptr(&r), flags)
}

/// ExtractAssociatedIconA : la variante W ne lit ni n'écrit le chemin.
unsafe extern "win64" fn extract_associated_icon_a(
    inst: usize,
    _path: *mut u8,
    index: *mut u16,
) -> usize {
    super::icue_win32::ExtractAssociatedIconW(inst, std::ptr::null_mut(), index)
}

pub fn exports() -> Exports {
    crate::exports! {
        "SHFileOperationA" => fn sh_file_operation_a,
        "ExtractIconExA" => fn extract_icon_ex_a,
        "ILCreateFromPathA" => fn il_create_from_path_a,
        "FindExecutableA" => fn find_executable_a,
        "SHQueryRecycleBinA" => fn sh_query_recycle_bin_a,
        "SHEmptyRecycleBinA" => fn sh_empty_recycle_bin_a,
        "ExtractAssociatedIconA" => fn extract_associated_icon_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_file_operation_a_copie_un_fichier() {
        let dir = std::env::temp_dir().join(format!("winlux-shfop-a-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.txt");
        let dst = dir.join("b.txt");
        std::fs::write(&src, b"odt").unwrap();
        // Chemins Linux absolus : paths::to_linux les garde tels quels.
        let mut from = src.to_str().unwrap().as_bytes().to_vec();
        from.extend([0, 0]);
        let mut to = dst.to_str().unwrap().as_bytes().to_vec();
        to.extend([0, 0]);
        // SHFILEOPSTRUCTA 64 bits : wFunc 8, pFrom 16, pTo 24, fFlags 32,
        // fAnyOperationsAborted 36.
        let mut op = [0u8; 56];
        op[8..12].copy_from_slice(&2u32.to_le_bytes()); // FO_COPY
        op[16..24].copy_from_slice(&(from.as_ptr() as usize).to_le_bytes());
        op[24..32].copy_from_slice(&(to.as_ptr() as usize).to_le_bytes());
        op[36..40].copy_from_slice(&7i32.to_le_bytes());
        assert_eq!(unsafe { sh_file_operation_a(op.as_mut_ptr()) }, 0);
        assert_eq!(std::fs::read(&dst).unwrap(), b"odt");
        assert_eq!(i32::from_le_bytes(op[36..40].try_into().unwrap()), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn variantes_ansi_servies() {
        let m = exports();
        for n in [
            "SHFileOperationA",
            "ExtractIconExA",
            "ILCreateFromPathA",
            "FindExecutableA",
            "SHQueryRecycleBinA",
            "SHEmptyRecycleBinA",
            "ExtractAssociatedIconA",
        ] {
            assert!(m.contains_key(n), "{n}");
        }
    }
}
