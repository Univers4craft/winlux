//! Liens symboliques et physiques, nom d'utilisateur et développement des
//! variables d'environnement (`%USERPROFILE%`…) pour les applis Windows.

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}

fn io_fail(e: &std::io::Error) -> i32 {
    fail(crate::dlls::kernel32::io_error_code(e))
}

/// `CreateSymbolicLink` : un lien Linux ordinaire. Les drapeaux (dossier,
/// création sans privilège) n'ont pas d'équivalent nécessaire sous Linux.
pub fn create_symbolic_link(link: &str, target: &str) -> i32 {
    if link.is_empty() || target.is_empty() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let link = crate::paths::to_linux_raw(link);
    let target = crate::paths::link_target(target);
    match std::os::unix::fs::symlink(&target, &link) {
        Ok(()) => 1,
        Err(e) => io_fail(&e),
    }
}

pub fn create_hard_link(link: &str, existing: &str) -> i32 {
    if link.is_empty() || existing.is_empty() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match std::fs::hard_link(
        crate::paths::to_linux(existing),
        crate::paths::to_linux_raw(link),
    ) {
        Ok(()) => 1,
        Err(e) => io_fail(&e),
    }
}

pub unsafe extern "win64" fn create_symbolic_link_w(
    link: *const u16,
    target: *const u16,
    _flags: u32,
) -> u8 {
    if link.is_null() || target.is_null() {
        return fail(ERROR_INVALID_PARAMETER) as u8;
    }
    create_symbolic_link(&crate::dlls::wstr(link), &crate::dlls::wstr(target)) as u8
}

pub unsafe extern "win64" fn create_symbolic_link_a(
    link: *const u8,
    target: *const u8,
    _flags: u32,
) -> u8 {
    if link.is_null() || target.is_null() {
        return fail(ERROR_INVALID_PARAMETER) as u8;
    }
    create_symbolic_link(&crate::dlls::cstr(link), &crate::dlls::cstr(target)) as u8
}

pub unsafe extern "win64" fn create_hard_link_w(
    link: *const u16,
    existing: *const u16,
    _sec: usize,
) -> i32 {
    if link.is_null() || existing.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    create_hard_link(&crate::dlls::wstr(link), &crate::dlls::wstr(existing))
}

pub unsafe extern "win64" fn create_hard_link_a(
    link: *const u8,
    existing: *const u8,
    _sec: usize,
) -> i32 {
    if link.is_null() || existing.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    create_hard_link(&crate::dlls::cstr(link), &crate::dlls::cstr(existing))
}

/// `GetUserNameW` : taille en caractères, terminateur compris.
pub unsafe extern "win64" fn get_user_name_w(buf: *mut u16, size: *mut u32) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let w: Vec<u16> = crate::paths::user_name()
        .encode_utf16()
        .chain([0])
        .collect();
    if buf.is_null() || (*size as usize) < w.len() {
        *size = w.len() as u32;
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *size = w.len() as u32;
    1
}

/// Remplace chaque `%NOM%` défini par sa valeur ; un nom inconnu reste
/// tel quel, `%%` aussi, comme sous Windows.
pub fn expand_environment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match crate::dlls::kernel32::env_value(name) {
                    Some(v) => {
                        out.push_str(&v);
                        rest = &after[end + 1..];
                    }
                    None => {
                        // Le second % peut ouvrir la variable suivante.
                        out.push('%');
                        out.push_str(name);
                        rest = &after[end..];
                    }
                }
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub unsafe extern "win64" fn expand_environment_strings_w(
    src: *const u16,
    dst: *mut u16,
    size: u32,
) -> u32 {
    if src.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let w: Vec<u16> = expand_environment(&crate::dlls::wstr(src))
        .encode_utf16()
        .chain([0])
        .collect();
    if !dst.is_null() && (size as usize) >= w.len() {
        std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    }
    w.len() as u32
}

pub unsafe extern "win64" fn expand_environment_strings_a(
    src: *const u8,
    dst: *mut u8,
    size: u32,
) -> u32 {
    if src.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut b = crate::dlls::encode_ansi(&expand_environment(&crate::dlls::cstr(src)));
    b.push(0);
    if !dst.is_null() && (size as usize) >= b.len() {
        std::ptr::copy_nonoverlapping(b.as_ptr(), dst, b.len());
    }
    b.len() as u32
}

/// Winlux n'impose pas la limite MAX_PATH : les applis « longPathAware »
/// peuvent utiliser des chemins longs sans préfixe `\\?\`.
pub extern "win64" fn rtl_are_long_paths_enabled() -> u8 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateSymbolicLinkW" => fn create_symbolic_link_w,
        "CreateSymbolicLinkA" => fn create_symbolic_link_a,
        "CreateHardLinkW" => fn create_hard_link_w,
        "CreateHardLinkA" => fn create_hard_link_a,
        "GetUserNameW" => fn get_user_name_w,
        "ExpandEnvironmentStringsW" => fn expand_environment_strings_w,
        "ExpandEnvironmentStringsA" => fn expand_environment_strings_a,
    }
}

/// GetUserName est exporté par advapi32 sous Windows.
pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetUserNameW" => fn get_user_name_w,
        "GetUserNameA" => fn crate::dlls::kernel32::get_user_name_a,
    }
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RtlAreLongPathsEnabled" => fn rtl_are_long_paths_enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_developpees_comme_windows() {
        std::env::set_var("WINLUX_TEST_EXPAND", "C:\\Users\\moi");
        assert_eq!(
            expand_environment("%winlux_test_expand%\\AppData"),
            "C:\\Users\\moi\\AppData"
        );
        assert_eq!(expand_environment("100%"), "100%");
        assert_eq!(expand_environment("%%"), "%%");
        assert_eq!(
            expand_environment("%INCONNU_WINLUX%%WINLUX_TEST_EXPAND%"),
            "%INCONNU_WINLUX%C:\\Users\\moi"
        );
        assert_eq!(
            expand_environment("a %INCONNU_WINLUX b"),
            "a %INCONNU_WINLUX b"
        );
        std::env::remove_var("WINLUX_TEST_EXPAND");
    }

    #[test]
    fn liens_crees_par_une_appli() {
        let dir = std::env::temp_dir().join(format!("winlux-liens-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cible")).unwrap();
        std::fs::write(dir.join("cible/f.txt"), b"x").unwrap();
        let w = crate::paths::to_windows(&dir);
        assert_eq!(create_symbolic_link(&format!("{w}\\lien"), "cible"), 1);
        assert_eq!(
            std::fs::read_link(dir.join("lien")).unwrap(),
            std::path::PathBuf::from("cible")
        );
        assert_eq!(std::fs::read(dir.join("lien/f.txt")).unwrap(), b"x");
        assert_eq!(
            create_symbolic_link(&format!("{w}\\lien"), "cible"),
            0,
            "déjà là"
        );
        assert_eq!(
            create_hard_link(&format!("{w}\\dur.txt"), &format!("{w}\\cible\\F.TXT")),
            1
        );
        assert_eq!(std::fs::read(dir.join("dur.txt")).unwrap(), b"x");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
