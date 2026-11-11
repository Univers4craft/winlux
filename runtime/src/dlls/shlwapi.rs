//! shlwapi.dll — Utilitaires Shell légers (chaînes et chemins).
//!
//! Notepad++ et paint.net importent PathFileExistsA, PathCombineA,
//! StrCmpIW et quelques autres. On implémente les plus courants ;
//! le reste est stubé.

/// PathFileExistsA
pub unsafe extern "win64" fn path_file_exists_a(path: *const u8) -> i32 {
    if path.is_null() {
        return 0;
    }
    let s = crate::dlls::cstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).exists() {
        1
    } else {
        0
    }
}

/// PathFileExistsW
pub unsafe extern "win64" fn path_file_exists_w(path: *const u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let s = crate::dlls::wstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).exists() {
        1
    } else {
        0
    }
}

/// PathIsDirectoryA
pub unsafe extern "win64" fn path_is_directory_a(path: *const u8) -> i32 {
    if path.is_null() {
        return 0;
    }
    let s = crate::dlls::cstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).is_dir() {
        1
    } else {
        0
    }
}

/// PathIsDirectoryW
pub unsafe extern "win64" fn path_is_directory_w(path: *const u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let s = crate::dlls::wstr(path);
    let host = crate::paths::to_linux(&s);
    if std::path::Path::new(&host).is_dir() {
        1
    } else {
        0
    }
}

/// PathCombineA : joint dir\file dans out (MAX_PATH).
pub unsafe extern "win64" fn path_combine_a(
    out: *mut u8,
    dir: *const u8,
    file: *const u8,
) -> *mut u8 {
    if out.is_null() {
        return std::ptr::null_mut();
    }
    let d = if dir.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(dir)
    };
    let f = if file.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(file)
    };
    let combined = if d.is_empty() {
        f
    } else if f.is_empty() {
        d
    } else if f.starts_with('\\') || f.starts_with('/') || (f.len() >= 2 && f.as_bytes()[1] == b':')
    {
        f
    } else {
        format!("{}\\{}", d.trim_end_matches(['\\', '/']), f)
    };
    let b = combined.as_bytes();
    let n = b.len().min(259);
    std::ptr::copy_nonoverlapping(b.as_ptr(), out, n);
    *out.add(n) = 0;
    out
}

/// Résultat de PathCombine (canonique), sans limite de taille ; sert aussi
/// à PathCchCombineEx (#1383).
pub(crate) unsafe fn path_combine_units(dir: *const u16, file: *const u16) -> Vec<u16> {
    let d = if dir.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(dir)
    };
    let f = if file.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(file)
    };
    let combined = if d.is_empty() {
        f
    } else if f.is_empty() {
        d
    } else if f.starts_with('\\') || f.starts_with('/') || (f.len() >= 2 && f.as_bytes()[1] == b':')
    {
        f
    } else {
        format!("{}\\{}", d.trim_end_matches(['\\', '/']), f)
    };
    // Le résultat est canonique (documentation de PathCombine).
    let units: Vec<u32> = combined.encode_utf16().map(u32::from).collect();
    canonicalize_units(&units)
        .iter()
        .map(|&c| c as u16)
        .collect()
}

/// PathCombineW
pub unsafe extern "win64" fn path_combine_w(
    out: *mut u16,
    dir: *const u16,
    file: *const u16,
) -> *mut u16 {
    if out.is_null() {
        return std::ptr::null_mut();
    }
    let w = path_combine_units(dir, file);
    let n = w.len().min(259);
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, n);
    *out.add(n) = 0;
    out
}

/// PathFindExtensionA
pub unsafe extern "win64" fn path_find_extension_a(path: *const u8) -> *const u8 {
    if path.is_null() {
        return path;
    }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    // Chercher le dernier '.' après le dernier séparateur.
    let mut last_sep = 0;
    let mut last_dot = n;
    for (i, &c) in s.iter().enumerate() {
        if c == b'\\' || c == b'/' {
            last_sep = i + 1;
        }
        if c == b'.' && i >= last_sep {
            last_dot = i;
        }
    }
    if last_dot >= n {
        path.add(n)
    } else {
        path.add(last_dot)
    }
}

/// PathFindExtensionW
pub unsafe extern "win64" fn path_find_extension_w(path: *const u16) -> *const u16 {
    if path.is_null() {
        return path;
    }
    let n = crate::printf::wide_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let mut last_sep = 0;
    let mut last_dot = n;
    for (i, &c) in s.iter().enumerate() {
        if c == b'\\' as u16 || c == b'/' as u16 {
            last_sep = i + 1;
        }
        if c == b'.' as u16 && i >= last_sep {
            last_dot = i;
        }
    }
    if last_dot >= n {
        path.add(n)
    } else {
        path.add(last_dot)
    }
}

/// PathFindFileNameA
pub unsafe extern "win64" fn path_find_filename_a(path: *const u8) -> *const u8 {
    if path.is_null() {
        return path;
    }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let pos = s
        .iter()
        .rposition(|&c| c == b'\\' || c == b'/')
        .map(|i| i + 1)
        .unwrap_or(0);
    path.add(pos)
}

/// PathFindFileNameW
pub unsafe extern "win64" fn path_find_filename_w(path: *const u16) -> *const u16 {
    if path.is_null() {
        return path;
    }
    let n = crate::printf::wide_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let pos = s
        .iter()
        .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16)
        .map(|i| i + 1)
        .unwrap_or(0);
    path.add(pos)
}

/// PathRemoveExtensionA
pub unsafe extern "win64" fn path_remove_extension_a(path: *mut u8) {
    if path.is_null() {
        return;
    }
    let ext = path_find_extension_a(path);
    if ext != path.add(crate::printf::narrow_len(path)) {
        *(ext as *mut u8) = 0;
    }
}

/// PathIsRelativeA
pub unsafe extern "win64" fn path_is_relative_a(path: *const u8) -> i32 {
    if path.is_null() {
        return 1;
    }
    let s = crate::dlls::cstr(path);
    if (s.len() >= 2 && s.as_bytes()[1] == b':') || s.starts_with('\\') {
        0
    } else {
        1
    }
}

/// PathIsRelativeW
pub unsafe extern "win64" fn path_is_relative_w(path: *const u16) -> i32 {
    if path.is_null() {
        return 1;
    }
    let s = crate::dlls::wstr(path);
    if (s.len() >= 2 && s.as_bytes()[1] == b':') || s.starts_with('\\') {
        0
    } else {
        1
    }
}

/// PathAppendA
pub unsafe extern "win64" fn path_append_a(base: *mut u8, more: *const u8) -> i32 {
    if base.is_null() || more.is_null() {
        return 0;
    }
    let mut buf = [0u8; 260];
    path_combine_a(buf.as_mut_ptr(), base, more);
    let n = crate::printf::narrow_len(buf.as_ptr()).min(259);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), base, n);
    *base.add(n) = 0;
    1
}

/// PathRemoveFileSpecA
pub unsafe extern "win64" fn path_remove_filespec_a(path: *mut u8) -> i32 {
    if path.is_null() {
        return 0;
    }
    let n = crate::printf::narrow_len(path);
    let s = std::slice::from_raw_parts(path, n);
    if let Some(pos) = s.iter().rposition(|&c| c == b'\\' || c == b'/') {
        *path.add(pos) = 0;
        1
    } else {
        *path = 0;
        1
    }
}

/// PathRemoveFileSpecW : retire le dernier composant et son séparateur, en
/// gardant la racine (`C:\\`, `\\`). Renvoie 0 si rien n'a été retiré.
pub unsafe extern "win64" fn path_remove_filespec_w(path: *mut u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let n = crate::printf::wide_len(path);
    let s = std::slice::from_raw_parts(path, n);
    let sep = |c: u16| c == b'\\' as u16 || c == b'/' as u16;
    let keep = match s.iter().rposition(|&c| sep(c)) {
        None => 0,
        Some(0) => 1,
        Some(2) if s[1] == b':' as u16 => 3,
        Some(pos) => pos,
    };
    if keep >= n {
        return 0;
    }
    *path.add(keep) = 0;
    1
}

/// PathRemoveExtensionW
pub unsafe extern "win64" fn path_remove_extension_w(path: *mut u16) {
    if path.is_null() {
        return;
    }
    let ext = path_find_extension_w(path);
    if ext != path.add(crate::printf::wide_len(path)) {
        *(ext as *mut u16) = 0;
    }
}

/// StrCmpIW (comparaison insensible à la casse)
pub unsafe extern "win64" fn str_cmp_iw(s1: *const u16, s2: *const u16) -> i32 {
    if s1.is_null() || s2.is_null() {
        return 0;
    }
    let a = crate::dlls::wstr(s1).to_lowercase();
    let b = crate::dlls::wstr(s2).to_lowercase();
    a.cmp(&b) as i32
}

/// StrCmpNIW
pub unsafe extern "win64" fn str_cmp_niw(s1: *const u16, s2: *const u16, n: i32) -> i32 {
    if s1.is_null() || s2.is_null() || n <= 0 {
        return 0;
    }
    let a: String = crate::dlls::wstr(s1).chars().take(n as usize).collect();
    let b: String = crate::dlls::wstr(s2).chars().take(n as usize).collect();
    a.to_lowercase().cmp(&b.to_lowercase()) as i32
}

/// StrStrIW (recherche insensible à la casse dans une chaîne large)
pub unsafe extern "win64" fn str_str_iw(haystack: *const u16, needle: *const u16) -> *const u16 {
    if haystack.is_null() || needle.is_null() {
        return std::ptr::null();
    }
    let h = crate::dlls::wstr(haystack);
    let n = crate::dlls::wstr(needle);
    // `to_lowercase()` peut changer la longueur (« İ » devient deux
    // caractères) : l'index trouvé ne correspondait plus à `h` et
    // `h[..pos]` pouvait couper un caractère (panique). Comparaison
    // caractère par caractère, chacun réduit à sa première minuscule.
    fn low(c: char) -> char {
        c.to_lowercase().next().unwrap_or(c)
    }
    let hc: Vec<char> = h.chars().map(low).collect();
    let nc: Vec<char> = n.chars().map(low).collect();
    if nc.is_empty() {
        return haystack;
    }
    if nc.len() > hc.len() {
        return std::ptr::null();
    }
    let mut u16_pos = 0usize;
    for (i, &c) in hc.iter().enumerate() {
        if i + nc.len() <= hc.len() && hc[i..i + nc.len()] == nc[..] {
            return haystack.add(u16_pos);
        }
        u16_pos += c.len_utf16();
    }
    std::ptr::null()
}

/// StrCmpLogicalW : ordre de l'Explorateur, sans casse, les suites de
/// chiffres comparées comme des nombres (« 2 » avant « 10 »).
pub unsafe extern "win64" fn str_cmp_logical_w(s1: *const u16, s2: *const u16) -> i32 {
    match (s1.is_null(), s2.is_null()) {
        (true, true) => return 0,
        (true, false) => return -1,
        (false, true) => return 1,
        _ => {}
    }
    logical_cmp(&crate::dlls::wstr(s1), &crate::dlls::wstr(s2)) as i32
}

fn logical_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i]
                .iter()
                .collect::<String>()
                .trim_start_matches('0')
                .into();
            let nb: String = b[sj..j]
                .iter()
                .collect::<String>()
                .trim_start_matches('0')
                .into();
            let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        let ca: String = a[i].to_lowercase().collect();
        let cb: String = b[j].to_lowercase().collect();
        if ca != cb {
            return ca.cmp(&cb);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// PathSkipRootW : pointe juste après la racine (« C:\\ », « \\\\serveur\\partage\\ »,
/// « \\ ») ; NULL si le chemin n'a pas de racine.
pub unsafe extern "win64" fn path_skip_root_w(path: *const u16) -> *const u16 {
    if path.is_null() {
        return std::ptr::null();
    }
    let n = crate::printf::wide_len(path);
    let p = std::slice::from_raw_parts(path, n);
    let sep = |c: u16| c == b'\\' as u16 || c == b'/' as u16;
    let at = if n >= 2 && sep(p[0]) && sep(p[1]) {
        // UNC : sauter serveur et partage.
        let mut i = 2;
        let mut parts = 0;
        while i < n && parts < 2 {
            if sep(p[i]) {
                parts += 1;
            }
            i += 1;
        }
        Some(i)
    } else if n >= 1 && sep(p[0]) {
        Some(1)
    } else if n >= 3 && p[1] == b':' as u16 && sep(p[2]) {
        Some(3)
    } else {
        None
    };
    at.map_or(std::ptr::null(), |i| path.add(i))
}

// --- PathCanonicalize, PathCreateFromUrl, barres obliques finales -----------
// D'après la documentation publique de shlwapi : « . » et « .. » sont retirés
// sans toucher au disque, la racine (« C:\ », « \\serveur ») est gardée et un
// chemin vide devient « \ ». Travail en unités de code pour couvrir A et W.

const BS: u32 = b'\\' as u32;

fn units_w(p: *const u16) -> Vec<u32> {
    if p.is_null() {
        return Vec::new();
    }
    let mut v = Vec::new();
    unsafe {
        let mut i = 0;
        while *p.add(i) != 0 {
            v.push(*p.add(i) as u32);
            i += 1;
        }
    }
    v
}

fn units_a(p: *const u8) -> Vec<u32> {
    if p.is_null() {
        return Vec::new();
    }
    let n = unsafe { crate::printf::narrow_len(p) };
    (0..n).map(|i| unsafe { *p.add(i) } as u32).collect()
}

/// Longueur de la racine : « C:\ » (3), « C: » (2), « \\serveur\partage\ »,
/// « \ » (1).
pub(crate) fn root_len(p: &[u32]) -> usize {
    if p.len() >= 2 && p[1] == b':' as u32 {
        return if p.get(2) == Some(&BS) { 3 } else { 2 };
    }
    if p.len() >= 2 && p[0] == BS && p[1] == BS {
        // \serveur\partage : la racine va jusqu'après le partage.
        let mut seps = 0;
        for (i, &c) in p.iter().enumerate().skip(2) {
            if c == BS {
                seps += 1;
                if seps == 2 {
                    return i + 1;
                }
            }
        }
        return p.len();
    }
    usize::from(p.first() == Some(&BS))
}

pub(crate) fn canonicalize_units(p: &[u32]) -> Vec<u32> {
    let root_n = root_len(p);
    let mut root = p[..root_n].to_vec();
    if root.len() == 2 && root[1] == b':' as u32 {
        root.push(BS); // « C: » devient « C:\\ »
    }
    let mut parts: Vec<&[u32]> = Vec::new();
    for seg in p[root_n..].split(|&c| c == BS) {
        if seg == [b'.' as u32] {
            continue;
        }
        if seg == [b'.' as u32, b'.' as u32] {
            parts.pop();
            continue;
        }
        if !seg.is_empty() {
            parts.push(seg);
        }
    }
    let mut out = root;
    for (i, seg) in parts.iter().enumerate() {
        if i > 0 {
            out.push(BS);
        }
        out.extend_from_slice(seg);
    }
    // Une barre finale du chemin d'origine est gardée (« C:\a\ »).
    if !parts.is_empty() && p.last() == Some(&BS) {
        out.push(BS);
    }
    if out.is_empty() {
        out.push(BS);
    }
    out
}

unsafe fn put_w(out: *mut u16, v: &[u32], cap: usize) -> bool {
    if v.len() >= cap {
        *out = 0;
        return false;
    }
    for (i, &c) in v.iter().enumerate() {
        *out.add(i) = c as u16;
    }
    *out.add(v.len()) = 0;
    true
}

unsafe fn put_a(out: *mut u8, v: &[u32], cap: usize) -> bool {
    if v.len() >= cap {
        *out = 0;
        return false;
    }
    for (i, &c) in v.iter().enumerate() {
        *out.add(i) = c as u8;
    }
    *out.add(v.len()) = 0;
    true
}

/// PathCanonicalizeW (tampon de MAX_PATH caractères).
pub unsafe extern "win64" fn path_canonicalize_w(out: *mut u16, path: *const u16) -> i32 {
    if out.is_null() || path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    put_w(out, &canonicalize_units(&units_w(path)), 260) as i32
}

/// PathCanonicalizeA
pub unsafe extern "win64" fn path_canonicalize_a(out: *mut u8, path: *const u8) -> i32 {
    if out.is_null() || path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    put_a(out, &canonicalize_units(&units_a(path)), 260) as i32
}

/// « file:///C:/a%20b/c.txt » → « C:\a b\c.txt » ; « file://serveur/x » →
/// « \\serveur\x ».
fn path_from_url(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("file:") {
        return None;
    }
    let rest = &url[5..];
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (unc, rest) = if let Some(local) = rest.strip_prefix('/') {
        (false, local)
    } else if rest.len() >= 2 && rest.as_bytes()[1] == b':' || rest.as_bytes().get(1) == Some(&b'|')
    {
        (false, rest)
    } else if let Some(local) = rest.strip_prefix("localhost/") {
        (false, local)
    } else {
        (true, rest)
    };
    let mut bytes = Vec::new();
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(if b[i] == b'/' { b'\\' } else { b[i] });
        i += 1;
    }
    let mut path = String::from_utf8_lossy(&bytes).into_owned();
    if path.as_bytes().get(1) == Some(&b'|') {
        path.replace_range(1..2, ":");
    }
    Some(if unc { format!("\\\\{path}") } else { path })
}

/// PathCreateFromUrlW : S_OK, E_INVALIDARG si ce n'est pas une URL file:.
pub unsafe extern "win64" fn path_create_from_url_w(
    url: *const u16,
    out: *mut u16,
    len: *mut u32,
    _reserved: u32,
) -> i32 {
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_POINTER: i32 = 0x8000_4003u32 as i32;
    if url.is_null() || out.is_null() || len.is_null() {
        return E_INVALIDARG;
    }
    let Some(path) = path_from_url(&crate::dlls::wstr(url)) else {
        return E_INVALIDARG;
    };
    let w: Vec<u16> = path.encode_utf16().collect();
    if w.len() >= *len as usize {
        *len = w.len() as u32 + 1;
        return E_POINTER;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    *out.add(w.len()) = 0;
    *len = w.len() as u32;
    0
}

/// PathCreateFromUrlA
pub unsafe extern "win64" fn path_create_from_url_a(
    url: *const u8,
    out: *mut u8,
    len: *mut u32,
    _reserved: u32,
) -> i32 {
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_POINTER: i32 = 0x8000_4003u32 as i32;
    if url.is_null() || out.is_null() || len.is_null() {
        return E_INVALIDARG;
    }
    let text: String = units_a(url).iter().map(|&c| c as u8 as char).collect();
    let Some(path) = path_from_url(&text) else {
        return E_INVALIDARG;
    };
    let b: Vec<u8> = path
        .chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect();
    if b.len() >= *len as usize {
        *len = b.len() as u32 + 1;
        return E_POINTER;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), out, b.len());
    *out.add(b.len()) = 0;
    *len = b.len() as u32;
    0
}

/// PathAddBackslashW : renvoie la fin de la chaîne, ou NULL si elle déborde.
pub unsafe extern "win64" fn path_add_backslash_w(path: *mut u16) -> *mut u16 {
    if path.is_null() {
        return path;
    }
    let mut v = units_w(path);
    if !v.is_empty() && v.last() != Some(&BS) {
        v.push(BS);
        if !put_w(path, &v, 260) {
            return std::ptr::null_mut();
        }
    }
    path.add(v.len())
}

/// PathAddBackslashA
pub unsafe extern "win64" fn path_add_backslash_a(path: *mut u8) -> *mut u8 {
    if path.is_null() {
        return path;
    }
    let mut v = units_a(path);
    if !v.is_empty() && v.last() != Some(&BS) {
        v.push(BS);
        if !put_a(path, &v, 260) {
            return std::ptr::null_mut();
        }
    }
    path.add(v.len())
}

/// PathRemoveBackslashW : retire la barre finale sauf sur une racine
/// (« C:\ ») ; renvoie la fin de la chaîne.
pub unsafe extern "win64" fn path_remove_backslash_w(path: *mut u16) -> *mut u16 {
    if path.is_null() {
        return path;
    }
    let mut v = units_w(path);
    if v.last() == Some(&BS) && v.len() > root_len(&v).max(1) {
        v.pop();
        *path.add(v.len()) = 0;
        return path.add(v.len());
    }
    path.add(v.len().saturating_sub(1))
}

/// PathRemoveBackslashA
pub unsafe extern "win64" fn path_remove_backslash_a(path: *mut u8) -> *mut u8 {
    if path.is_null() {
        return path;
    }
    let mut v = units_a(path);
    if v.last() == Some(&BS) && v.len() > root_len(&v).max(1) {
        v.pop();
        *path.add(v.len()) = 0;
        return path.add(v.len());
    }
    path.add(v.len().saturating_sub(1))
}

/// PathIsRootW : « \ », « C:\ », « \\serveur », « \\serveur\partage ».
pub unsafe extern "win64" fn path_is_root_w(path: *const u16) -> i32 {
    is_root(&units_w(path)) as i32
}

/// PathIsRootA
pub unsafe extern "win64" fn path_is_root_a(path: *const u8) -> i32 {
    is_root(&units_a(path)) as i32
}

fn is_root(p: &[u32]) -> bool {
    if p == [BS] || (p.len() == 3 && p[1] == b':' as u32 && p[2] == BS) {
        return true;
    }
    if p.len() > 2 && p[0] == BS && p[1] == BS {
        let inner = p[2..].strip_suffix(&[BS]).unwrap_or(&p[2..]);
        return inner.iter().filter(|&&c| c == BS).count() <= 1 && !inner.is_empty();
    }
    false
}

pub fn exports() -> crate::dlls::Exports {
    let mut m = crate::exports! {
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
        "PathRemoveFileSpecW" => fn path_remove_filespec_w,
        "PathRemoveExtensionW" => fn path_remove_extension_w,
        "PathCanonicalizeA" => fn path_canonicalize_a,
        "PathCanonicalizeW" => fn path_canonicalize_w,
        "PathCreateFromUrlA" => fn path_create_from_url_a,
        "PathCreateFromUrlW" => fn path_create_from_url_w,
        "PathAddBackslashA" => fn path_add_backslash_a,
        "PathAddBackslashW" => fn path_add_backslash_w,
        "PathRemoveBackslashA" => fn path_remove_backslash_a,
        "PathRemoveBackslashW" => fn path_remove_backslash_w,
        "PathIsRootA" => fn path_is_root_a,
        "PathIsRootW" => fn path_is_root_w,
        "StrCmpIW" => fn str_cmp_iw,
        "StrCmpLogicalW" => fn str_cmp_logical_w,
        "PathSkipRootW" => fn path_skip_root_w,
        "StrCmpNIW" => fn str_cmp_niw,
        "StrStrIW" => fn str_str_iw,
    };
    // SHGetValue, SHRegGetUSValue, AssocQueryString… : voir shlwapi_reg.rs.
    m.extend(super::shlwapi_reg::exports());
    // Le reste de la famille Path* (#1631), sans masquer une copie existante.
    for (k, v) in super::shlwapi_path::exports() {
        m.entry(k).or_insert(v);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordre_logique_et_racine() {
        use std::cmp::Ordering::*;
        assert_eq!(logical_cmp("fichier2.txt", "Fichier10.txt"), Less);
        assert_eq!(logical_cmp("a010", "a10"), Equal);
        assert_eq!(logical_cmp("abc", "abd"), Less);
        assert_eq!(logical_cmp("abc", "ab"), Greater);
        let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        unsafe {
            for (p, rest) in [
                ("C:\\Windows\\x", Some("Windows\\x")),
                ("\\\\srv\\part\\d", Some("d")),
                ("\\a", Some("a")),
                ("rel\\x", None),
            ] {
                let v = w(p);
                let r = path_skip_root_w(v.as_ptr());
                assert_eq!(
                    (!r.is_null()).then(|| crate::dlls::wstr(r)).as_deref(),
                    rest,
                    "{p}"
                );
            }
        }
    }

    fn remove_filespec(p: &str) -> (i32, String) {
        let mut buf: Vec<u16> = p.encode_utf16().chain([0]).collect();
        let r = unsafe { path_remove_filespec_w(buf.as_mut_ptr()) };
        (r, unsafe { crate::dlls::wstr(buf.as_ptr()) })
    }

    #[test]
    fn str_str_iw_ignore_la_casse_sans_decaler() {
        let h: Vec<u16> = "Ağaç İstanbul MERHABA".encode_utf16().chain([0]).collect();
        let n: Vec<u16> = "merhaba".encode_utf16().chain([0]).collect();
        let r = unsafe { str_str_iw(h.as_ptr(), n.as_ptr()) };
        assert!(!r.is_null());
        assert_eq!(unsafe { crate::dlls::wstr(r) }, "MERHABA");
        let absent: Vec<u16> = "xyz".encode_utf16().chain([0]).collect();
        assert!(unsafe { str_str_iw(h.as_ptr(), absent.as_ptr()) }.is_null());
    }

    #[test]
    fn path_remove_filespec_w_garde_la_racine() {
        assert_eq!(
            remove_filespec("C:\\Apps\\paint.net\\paintdotnet.exe"),
            (1, "C:\\Apps\\paint.net".into())
        );
        assert_eq!(remove_filespec("C:\\paintdotnet.exe"), (1, "C:\\".into()));
        assert_eq!(remove_filespec("C:\\"), (0, "C:\\".into()));
        assert_eq!(remove_filespec("\\fichier"), (1, "\\".into()));
        assert_eq!(remove_filespec("fichier"), (1, String::new()));
        assert_eq!(remove_filespec(""), (0, String::new()));
    }

    #[test]
    fn path_remove_extension_w_retire_le_suffixe() {
        let mut buf: Vec<u16> = "C:\\a.b\\app.exe".encode_utf16().chain([0]).collect();
        unsafe { path_remove_extension_w(buf.as_mut_ptr()) };
        assert_eq!(unsafe { crate::dlls::wstr(buf.as_ptr()) }, "C:\\a.b\\app");
    }

    #[test]
    fn path_canonicalize_et_url_de_fichier() {
        let canon = |p: &str| {
            let w: Vec<u16> = p.encode_utf16().chain([0]).collect();
            let mut out = [0u16; 260];
            assert_eq!(
                unsafe { path_canonicalize_w(out.as_mut_ptr(), w.as_ptr()) },
                1
            );
            unsafe { crate::dlls::wstr(out.as_ptr()) }
        };
        assert_eq!(canon(r"C:\a\.\b\..\c.txt"), r"C:\a\c.txt");
        assert_eq!(canon(r"C:\.."), r"C:\");
        assert_eq!(canon("C:"), r"C:\");
        assert_eq!(canon(r"\\srv\part\x\..\y"), r"\\srv\part\y");
        assert_eq!(canon(r"a\b\..\"), r"a\");
        assert_eq!(canon(""), r"\");
        let mut a = [0u8; 260];
        assert_eq!(
            unsafe { path_canonicalize_a(a.as_mut_ptr(), b"D:\\x\\..\\y\0".as_ptr()) },
            1
        );
        assert_eq!(&a[..5], b"D:\\y\0");

        let url: Vec<u16> = "file:///C:/Mes%20docs/a.txt"
            .encode_utf16()
            .chain([0])
            .collect();
        let mut out = [0u16; 64];
        let mut len = 64u32;
        assert_eq!(
            unsafe { path_create_from_url_w(url.as_ptr(), out.as_mut_ptr(), &mut len, 0) },
            0
        );
        assert_eq!(
            unsafe { crate::dlls::wstr(out.as_ptr()) },
            r"C:\Mes docs\a.txt"
        );
        let http: Vec<u16> = "http://x/".encode_utf16().chain([0]).collect();
        assert!(
            unsafe { path_create_from_url_w(http.as_ptr(), out.as_mut_ptr(), &mut len, 0) } < 0
        );

        let mut p: Vec<u16> = r"C:\a".encode_utf16().chain([0; 8]).collect();
        unsafe { path_add_backslash_w(p.as_mut_ptr()) };
        assert_eq!(unsafe { crate::dlls::wstr(p.as_ptr()) }, r"C:\a\");
        unsafe { path_remove_backslash_w(p.as_mut_ptr()) };
        assert_eq!(unsafe { crate::dlls::wstr(p.as_ptr()) }, r"C:\a");
        let mut root: Vec<u16> = r"C:\".encode_utf16().chain([0]).collect();
        unsafe { path_remove_backslash_w(root.as_mut_ptr()) };
        assert_eq!(unsafe { crate::dlls::wstr(root.as_ptr()) }, r"C:\");
        assert_eq!(unsafe { path_is_root_w(root.as_ptr()) }, 1);
        assert_eq!(unsafe { path_is_root_w(p.as_ptr()) }, 0);
    }
}
