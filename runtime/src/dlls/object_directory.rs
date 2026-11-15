//! Répertoires du gestionnaire d'objets (`\BaseNamedObjects`,
//! `\Sessions\N\AppContainerNamedObjects\<SID>`…) ouverts par
//! NtOpenDirectoryObject (#2561).
//!
//! Microsoft Learn (ZwOpenDirectoryObject, ntifs.h) : la fonction « opens an
//! existing directory object and returns a handle to the object », et ce
//! handle se libère par ZwClose. Winlux n'a pas d'espace de noms d'objets :
//! les objets nommés vivent dans ses propres tables. Il reconnaît donc les
//! répertoires que Windows crée de lui-même pour une session et pour un
//! AppContainer, et rend pour chacun un handle que CloseHandle et NtClose
//! libèrent. Un autre nom est refusé avec STATUS_OBJECT_NAME_NOT_FOUND, et
//! le refus est journalisé avec le nom demandé.

use std::collections::HashMap;
use std::sync::Mutex;

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;
const STATUS_OBJECT_PATH_SYNTAX_BAD: i32 = 0xC000_003Bu32 as i32;

/// Handle → chemin absolu du répertoire.
static DIRECTORIES: Mutex<Option<HashMap<usize, String>>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<HashMap<usize, String>>> {
    DIRECTORIES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Le répertoire existe-t-il dans l'espace de noms qu'imite Winlux ?
fn known(path: &str) -> bool {
    let parts: Vec<&str> = path.split('\\').collect();
    let session = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let package = |s: &str| s.len() > 9 && s[..9].eq_ignore_ascii_case("S-1-15-2-");
    let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    match parts.as_slice() {
        ["", ""] => true,
        ["", d] => eq(d, "BaseNamedObjects") || eq(d, "Sessions"),
        ["", s, n] => eq(s, "Sessions") && session(n),
        ["", s, n, d] => {
            eq(s, "Sessions")
                && session(n)
                && (eq(d, "BaseNamedObjects") || eq(d, "AppContainerNamedObjects"))
        }
        ["", s, n, d, sid] => {
            eq(s, "Sessions") && session(n) && eq(d, "AppContainerNamedObjects") && package(sid)
        }
        _ => false,
    }
}

/// Chemin d'un répertoire ouvert par ce module.
fn path_of(h: usize) -> Option<String> {
    lock().as_ref()?.get(&h).cloned()
}

/// CloseHandle et NtClose : vrai si le handle venait de ce module.
pub(crate) fn close(h: usize) -> bool {
    lock().as_mut().is_some_and(|m| m.remove(&h).is_some())
}

/// Le handle est-il un répertoire d'objets (NtQueryObject) ?
pub(crate) fn is_directory(h: usize) -> bool {
    path_of(h).is_some()
}

/// Nom lu dans OBJECT_ATTRIBUTES (x64 : RootDirectory +0x8, ObjectName +0x10).
unsafe fn requested(attrs: *const u8) -> Result<String, i32> {
    if attrs.is_null() {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let root = std::ptr::read_unaligned(attrs.add(0x8) as *const usize);
    let name = std::ptr::read_unaligned(attrs.add(0x10) as *const *const u8);
    let text = if name.is_null() {
        String::new()
    } else {
        let len = std::ptr::read_unaligned(name as *const u16) as usize / 2;
        let buf = std::ptr::read_unaligned(name.add(8) as *const *const u16);
        if buf.is_null() || len == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(buf, len))
        }
    };
    if root != 0 {
        let base = path_of(root).ok_or(STATUS_INVALID_PARAMETER)?;
        if text.is_empty() {
            return Ok(base);
        }
        let base = base.trim_end_matches('\\');
        return Ok(format!("{base}\\{text}"));
    }
    if !text.starts_with('\\') {
        return Err(STATUS_OBJECT_PATH_SYNTAX_BAD);
    }
    Ok(text)
}

/// ntdll!NtOpenDirectoryObject(DirectoryHandle, DesiredAccess, ObjectAttributes).
pub unsafe extern "win64" fn nt_open_directory_object(
    handle: *mut usize,
    _access: u32,
    attrs: *const u8,
) -> i32 {
    if handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let path = match requested(attrs) {
        Ok(p) => p,
        Err(status) => return status,
    };
    let trimmed = if path.len() > 1 {
        path.trim_end_matches('\\')
    } else {
        path.as_str()
    };
    if !known(trimmed) {
        crate::log_warn!(
            "process",
            "NtOpenDirectoryObject : répertoire « {path} » inconnu de Winlux (#2561)"
        );
        return STATUS_OBJECT_NAME_NOT_FOUND;
    }
    let h = crate::dlls::kernel32::new_handle_number();
    lock()
        .get_or_insert_with(HashMap::new)
        .insert(h, trimmed.to_string());
    *handle = h;
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// UNICODE_STRING (16 octets) puis OBJECT_ATTRIBUTES (48 octets), x64.
    fn attributes(root: usize, name: &[u16], us: &mut [u8; 16], oa: &mut [u8; 48]) {
        let bytes = (name.len() * 2) as u16;
        us[0..2].copy_from_slice(&bytes.to_le_bytes());
        us[2..4].copy_from_slice(&bytes.to_le_bytes());
        us[8..16].copy_from_slice(&(name.as_ptr() as usize).to_le_bytes());
        oa[0..4].copy_from_slice(&48u32.to_le_bytes());
        oa[8..16].copy_from_slice(&root.to_le_bytes());
        oa[16..24].copy_from_slice(&(us.as_ptr() as usize).to_le_bytes());
    }

    #[test]
    fn repertoire_des_objets_d_un_conteneur() {
        let name = wide("\\Sessions\\1\\AppContainerNamedObjects\\S-1-15-2-1-2-3-4-5-6-7");
        let (mut us, mut oa) = ([0u8; 16], [0u8; 48]);
        attributes(0, &name, &mut us, &mut oa);
        let mut h = 0usize;
        let st = unsafe { nt_open_directory_object(&mut h, 0xF, oa.as_ptr()) };
        assert_eq!(st, STATUS_SUCCESS);
        assert!(is_directory(h));

        // Nom relatif au répertoire ouvert : le même répertoire.
        let empty: Vec<u16> = Vec::new();
        let (mut us2, mut oa2) = ([0u8; 16], [0u8; 48]);
        attributes(h, &empty, &mut us2, &mut oa2);
        let mut h2 = 0usize;
        let st = unsafe { nt_open_directory_object(&mut h2, 0x3, oa2.as_ptr()) };
        assert_eq!(st, STATUS_SUCCESS);
        assert_ne!(h2, h);

        assert_eq!(unsafe { crate::dlls::ntdll::nt_close(h) }, STATUS_SUCCESS);
        assert!(!is_directory(h));
        assert_eq!(crate::dlls::kernel32::close_handle(h2), 1);
        assert!(!is_directory(h2));
    }

    #[test]
    fn repertoire_inconnu_ou_mal_forme() {
        let mut h = 0usize;
        let name = wide("\\Sessions\\1\\Inexistant");
        let (mut us, mut oa) = ([0u8; 16], [0u8; 48]);
        attributes(0, &name, &mut us, &mut oa);
        let st = unsafe { nt_open_directory_object(&mut h, 0xF, oa.as_ptr()) };
        assert_eq!(st, STATUS_OBJECT_NAME_NOT_FOUND);

        let name = wide("BaseNamedObjects");
        attributes(0, &name, &mut us, &mut oa);
        let st = unsafe { nt_open_directory_object(&mut h, 0xF, oa.as_ptr()) };
        assert_eq!(st, STATUS_OBJECT_PATH_SYNTAX_BAD);

        let st = unsafe { nt_open_directory_object(&mut h, 0xF, std::ptr::null()) };
        assert_eq!(st, STATUS_INVALID_PARAMETER);

        let name = wide("\\BaseNamedObjects");
        attributes(0, &name, &mut us, &mut oa);
        let st = unsafe { nt_open_directory_object(&mut h, 0xF, oa.as_ptr()) };
        assert_eq!(st, STATUS_SUCCESS);
        assert!(close(h));
    }
}
