//! shlwapi.dll — Utilitaires Shell légers (chaînes et chemins).
//!
//! Notepad++ et paint.net importent PathFileExistsA, PathCombineA,
//! StrCmpIW et quelques autres. On implémente les plus courants ;
//! le reste est stubé.

/// PathFileExistsA
pub unsafe extern "win64" fn path_file_exists_a(path: *const u8) -> i32 {
    if path.is_null() { return 0; }
    let s = crate::dlls::cstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).exists() { 1 } else { 0 }
}

/// PathFileExistsW
pub unsafe extern "win64" fn path_file_exists_w(path: *const u16) -> i32 {
    if path.is_null() { return 0; }
    let s = crate::dlls::wstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).exists() { 1 } else { 0 }
}

/// PathIsDirectoryA
pub unsafe extern "win64" fn path_is_directory_a(path: *const u8) -> i32 {
    if path.is_null() { return 0; }
    let s = crate::dlls::cstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).is_dir() { 1 } else { 0 }
}

/// PathIsDirectoryW
pub unsafe extern "win64" fn path_is_directory_w(path: *const u16) -> i32 {
    if path.is_null() { return 0; }
    let s = crate::dlls::wstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).is_dir() { 1 } else { 0 }
}

/// PathCombineA : joint dir\file dans out (MAX_PATH).
pub unsafe extern "win64" fn path_combine_a(out: *mut u8, dir: *const u8, file: *const u8) -> *mut u8 {
    if out.is_null() { return std::ptr::null_mut(); }
    let d = if dir.is_null() { String::new() } else { crate::dlls::cstr(dir) };
    let f = if file.is_null() { String::new() } else { crate::dlls::cstr(file) };
    let combined = if d.is_empty() { f }
        else if f.is_empty() { d }
        else if f.starts_with('\\') || f.starts_with('/') || (f.len() >= 2 && f.as_bytes()[1] == b':') { f }
        else { format!("{}\\{}", d.trim_end_matches(['\\', '/']), f) };
    let b = combined.as_bytes();
    let n = b.len().min(259);
    std::ptr::copy_nonoverlapping(b.as_ptr(), out, n);
    *out.add(n) = 0;
    out
}

/// PathCombineW
pub unsafe extern "win64" fn path_combine_w(out: *mut u16, dir: *const u16, file: *const u16) -> *mut u16 {
    if out.is_null() { return std::ptr::null_mut(); }
    let d = if dir.is_null() { String::new() } else { crate::dlls::wstr(dir) };
    let f = if file.is_null() { String::new() } else { crate::dlls::wstr(file) };
    let combined = if d.is_empty() { f }
        else if f.is_empty() { d }
        else if f.starts_with('\\') || f.starts_with('/') || (f.len() >= 2 && f.as_bytes()[1] == b':') { f }
        else { format!("{}\\{}", d.trim_end_matches(['\\', '/']), f) };
    let w: Vec<u16> = combined.encode_utf16().collect();
    let n = w.len().min(259);
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, n);
    *out.add(n) = 0;
    out
}

/// PathFindExtensionA
pub unsafe extern "win64" fn path_find_extension_a(path: *const u8) -> *const u8 {
    if path.is_null() { return path; }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    // Chercher le dernier '.' après le dernier séparateur.
    let mut last_sep = 0;
    let mut last_dot = n;
    for (i, &c) in s.iter().enumerate() {
        if c == b'\\' || c == b'/' { last_sep = i + 1; }
        if c == b'.' && i >= last_sep { last_dot = i; }
    }
    if last_dot >= n { path.add(n) } else { path.add(last_dot) }
}

/// PathFindExtensionW
pub unsafe extern "win64" fn path_find_extension_w(path: *const u16) -> *const u16 {
    if path.is_null() { return path; }
    let n = crate::printf::wide_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let mut last_sep = 0;
    let mut last_dot = n;
    for (i, &c) in s.iter().enumerate() {
        if c == b'\\' as u16 || c == b'/' as u16 { last_sep = i + 1; }
        if c == b'.' as u16 && i >= last_sep { last_dot = i; }
    }
    if last_dot >= n { path.add(n) } else { path.add(last_dot) }
}

/// PathFindFileNameA
pub unsafe extern "win64" fn path_find_filename_a(path: *const u8) -> *const u8 {
    if path.is_null() { return path; }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let pos = s.iter().rposition(|&c| c == b'\\' || c == b'/').map(|i| i + 1).unwrap_or(0);
    path.add(pos)
}

/// PathFindFileNameW
pub unsafe extern "win64" fn path_find_filename_w(path: *const u16) -> *const u16 {
    if path.is_null() { return path; }
    let n = crate::printf::wide_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let pos = s.iter().rposition(|&c| c == b'\\' as u16 || c == b'/' as u16).map(|i| i + 1).unwrap_or(0);
    path.add(pos)
}

/// PathRemoveExtensionA
pub unsafe extern "win64" fn path_remove_extension_a(path: *mut u8) {
    if path.is_null() { return; }
    let ext = path_find_extension_a(path);
    if ext != path.add(crate::printf::narrow_len(path)) {
        *(ext as *mut u8) = 0;
    }
}

/// PathIsRelativeA
pub unsafe extern "win64" fn path_is_relative_a(path: *const u8) -> i32 {
    if path.is_null() { return 1; }
    let s = crate::dlls::cstr(path);
    if s.len() >= 2 && s.as_bytes()[1] == b':' { 0 } else if s.starts_with('\\') { 0 } else { 1 }
}

/// PathIsRelativeW
pub unsafe extern "win64" fn path_is_relative_w(path: *const u16) -> i32 {
    if path.is_null() { return 1; }
    let s = crate::dlls::wstr(path);
    if s.len() >= 2 && s.as_bytes()[1] == b':' { 0 } else if s.starts_with('\\') { 0 } else { 1 }
}

/// PathAppendA
pub unsafe extern "win64" fn path_append_a(base: *mut u8, more: *const u8) -> i32 {
    if base.is_null() || more.is_null() { return 0; }
    let mut buf = [0u8; 260];
    path_combine_a(buf.as_mut_ptr(), base, more);
    let n = crate::printf::narrow_len(buf.as_ptr()).min(259);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), base, n);
    *base.add(n) = 0;
    1
}

/// PathRemoveFileSpecA
pub unsafe extern "win64" fn path_remove_filespec_a(path: *mut u8) -> i32 {
    if path.is_null() { return 0; }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    if let Some(pos) = s.iter().rposition(|&c| c == b'\\' || c == b'/') {
        *path.add(pos) = 0; 1
    } else { *path = 0; 1 }
}

/// StrCmpIW (comparaison insensible à la casse)
pub unsafe extern "win64" fn str_cmp_iw(s1: *const u16, s2: *const u16) -> i32 {
    if s1.is_null() || s2.is_null() { return 0; }
    let a = crate::dlls::wstr(s1).to_lowercase();
    let b = crate::dlls::wstr(s2).to_lowercase();
    a.cmp(&b) as i32
}

/// StrCmpNIW
pub unsafe extern "win64" fn str_cmp_niw(s1: *const u16, s2: *const u16, n: i32) -> i32 {
    if s1.is_null() || s2.is_null() || n <= 0 { return 0; }
    let a: String = crate::dlls::wstr(s1).chars().take(n as usize).collect();
    let b: String = crate::dlls::wstr(s2).chars().take(n as usize).collect();
    a.to_lowercase().cmp(&b.to_lowercase()) as i32
}

/// StrStrIW (recherche insensible à la casse dans une chaîne large)
pub unsafe extern "win64" fn str_str_iw(haystack: *const u16, needle: *const u16) -> *const u16 {
    if haystack.is_null() || needle.is_null() { return std::ptr::null(); }
    let h = crate::dlls::wstr(haystack);
    let n = crate::dlls::wstr(needle);
    match h.to_lowercase().find(&n.to_lowercase()) {
        Some(pos) => {
            // Compter les caractères pour trouver l'offset en u16.
            let char_pos = h[..pos].chars().count();
            let u16_pos: usize = h.chars().take(char_pos).map(|c| c.len_utf16()).sum();
            haystack.add(u16_pos)
        }
        None => std::ptr::null(),
    }
}

/// SHCreateStreamOnFileA (IStream sur un fichier — stub)
pub unsafe extern "win64" fn sh_create_stream_on_file_a(_path: *const u8, _mode: u32, _out: *mut usize) -> i32 {
    0x8000_4005u32 as i32 // E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PathFileExistsA" => fn path_file_exists_a,
        "PathFileExistsW" => fn path_file_exists_w,
        "PathIsDirectoryA" => fn path_is_directory_a,
        "PathIsDirectoryW" => fn path_is_directory_w,
        "PathCombineA" => fn path_combine_a,
        "PathCombineW" => fn path_combine_w,
        "PathFindExtensionA" => fn path_find_extension_a,
        "PathFindExtensionW" => fn path_find_extension_w,
        "PathFindFileNameA" => fn path_find_filename_a,
        "PathFindFileNameW" => fn path_find_filename_w,
        "PathRemoveExtensionA" => fn path_remove_extension_a,
        "PathIsRelativeA" => fn path_is_relative_a,
        "PathIsRelativeW" => fn path_is_relative_w,
        "PathAppendA" => fn path_append_a,
        "PathRemoveFileSpecA" => fn path_remove_filespec_a,
        "StrCmpIW" => fn str_cmp_iw,
        "StrCmpNIW" => fn str_cmp_niw,
        "StrStrIW" => fn str_str_iw,
        "SHCreateStreamOnFileA" => fn sh_create_stream_on_file_a,
    }
}
