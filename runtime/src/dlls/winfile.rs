//! Énumération Win32 des fichiers, reliée au système de fichiers Linux.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_FILE_NOT_FOUND: u32 = 2;
/// Fin d'une énumération : FindNextFile renvoie 0 avec ce code (std::filesystem
/// le compare exactement, tout autre code est une erreur).
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_INVALID_PARAMETER: u32 = 87;
#[cfg(test)]
const INVALID_HANDLE_VALUE: usize = usize::MAX;
const ATTR_READONLY: u32 = 0x1;
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_ARCHIVE: u32 = 0x20;
const ATTR_NORMAL: u32 = 0x80;
pub(crate) const ATTR_REPARSE_POINT: u32 = 0x400;
/// Étiquette de point d'analyse d'un lien symbolique (`dwReserved0`).
pub(crate) const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

/// Vrai si l'utilisateur courant peut écrire dans `path` (droits Linux réels,
/// propriétaire, groupe, ACL POSIX et montage en lecture seule compris).
pub(crate) fn writable(path: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: chaîne C valide, access ne garde pas le pointeur.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Aucun bit d'écriture : c'est l'attribut « lecture seule » posé par
/// `SetFileAttributes`, respecté même par l'administrateur comme sous Windows.
pub(crate) fn no_write_bits(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o222 == 0
}

/// Attributs Windows d'un fichier Linux, et ses métadonnées (cible du lien).
///
/// - Un lien symbolique porte FILE_ATTRIBUTE_REPARSE_POINT, comme sous
///   Windows : les applis prudentes (sauvegarde, désinstallation récursive)
///   ne descendent pas dedans. Ses autres attributs sont ceux de la cible.
/// - Lecture seule = l'utilisateur ne peut pas écrire le fichier (droits
///   Linux réels), pas seulement « aucun bit w ». Jamais sur un dossier :
///   Windows ignore cet attribut pour les dossiers.
/// - Caché = nom commençant par un point.
pub(crate) fn attributes(path: &std::path::Path) -> std::io::Result<(u32, std::fs::Metadata)> {
    let link = std::fs::symlink_metadata(path)?;
    let is_link = link.file_type().is_symlink();
    let meta = if is_link {
        std::fs::metadata(path).unwrap_or(link)
    } else {
        link
    };
    let mut a = 0;
    if meta.is_dir() {
        a |= ATTR_DIRECTORY;
    } else if !meta.file_type().is_symlink() && (no_write_bits(&meta) || !writable(path)) {
        a |= ATTR_READONLY;
    }
    if is_link {
        a |= ATTR_REPARSE_POINT;
    }
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
    {
        a |= ATTR_HIDDEN;
    }
    if meta.is_file() {
        a |= ATTR_ARCHIVE;
    }
    if a == 0 {
        a = ATTR_NORMAL;
    }
    Ok((a, meta))
}

// FILETIME n'est aligné que sur 4 octets : cFileName est à l'offset 44.
#[repr(C, packed(4))]
pub struct Win32FindDataA {
    pub attributes: u32,
    pub creation_time: u64,
    pub access_time: u64,
    pub write_time: u64,
    pub size_high: u32,
    pub size_low: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub file_name: [c_char; 260],
    pub alternate_name: [c_char; 14],
}

// FILETIME n'est aligné que sur 4 octets : cFileName est à l'offset 44.
#[repr(C, packed(4))]
pub struct Win32FindDataW {
    pub attributes: u32,
    pub creation_time: u64,
    pub access_time: u64,
    pub write_time: u64,
    pub size_high: u32,
    pub size_low: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub file_name: [u16; 260],
    pub alternate_name: [u16; 14],
}

#[derive(Clone)]
struct Entry {
    name: String,
    attributes: u32,
    creation: u64,
    access: u64,
    write: u64,
    size: u64,
    reparse_tag: u32,
}
struct Search {
    entries: Vec<Entry>,
    next: usize,
}
static SEARCHES: OnceLock<Mutex<HashMap<usize, Search>>> = OnceLock::new();
static NEXT_SEARCH: AtomicUsize = AtomicUsize::new(0x6000_0000);

use super::lock;
fn searches() -> &'static Mutex<HashMap<usize, Search>> {
    SEARCHES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn error(code: u32) {
    crate::process::set_last_error(code);
}

fn filetime(t: std::time::SystemTime) -> u64 {
    const WINDOWS_EPOCH: u64 = 116_444_736_000_000_000;
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    WINDOWS_EPOCH
        .saturating_add(d.as_secs().saturating_mul(10_000_000))
        .saturating_add((d.subsec_nanos() / 100) as u64)
}

/// Motif de FindFirstFile, sans casse. Comme sous Windows, un « . » suivi
/// seulement de jokers en fin de motif correspond aussi à un nom sans point :
/// « *.* » désigne tous les fichiers, « *. » ceux sans extension et « nom.* »
/// aussi « nom ». wxWidgets énumère « dossier\*.* » : sans cela, il ne voyait
/// aucun sous-dossier « fr », « de »… et FileZilla restait en anglais (#1125).
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let s: Vec<char> = text.to_lowercase().chars().collect();
    if plain_wildcard_matches(&p, &s) {
        return true;
    }
    if s.contains(&'.') {
        return false;
    }
    match p.iter().rposition(|&c| c == '.') {
        Some(dot) if p[dot + 1..].iter().all(|&c| c == '*' || c == '?') => {
            plain_wildcard_matches(&p[..dot], &s)
        }
        _ => false,
    }
}

fn plain_wildcard_matches(p: &[char], s: &[char]) -> bool {
    let (mut i, mut j, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while j < s.len() {
        if i < p.len() && (p[i] == '?' || p[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            retry = j;
        } else if let Some(k) = star {
            retry += 1;
            j = retry;
            i = k + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

fn split_search_path(path: &str) -> Result<(std::path::PathBuf, String), u32> {
    let separator = path.rfind(['\\', '/']);
    let (directory, pattern) = match separator {
        Some(0) => (&path[..1], &path[1..]),
        // « C:\* » : la racine du lecteur, pas « C: » (le dossier courant du
        // lecteur, soit celui du programme) — #707.
        Some(i) if path[..i].ends_with(':') => (&path[..=i], &path[i + 1..]),
        Some(i) => (&path[..i], &path[i + 1..]),
        None => (".", path),
    };
    // Le « ? » des préfixes \\?\ et \??\ n'est pas un joker : Steam
    // cherche « \\?\C:\…\package\tmp\* » (#1475).
    let body = ["\\\\?\\", "\\??\\"]
        .iter()
        .find_map(|pre| directory.strip_prefix(pre))
        .unwrap_or(directory);
    if pattern.is_empty() || body.contains('*') || body.contains('?') {
        return Err(ERROR_INVALID_PARAMETER);
    }
    let directory = crate::paths::to_linux(directory);
    // « photos.zip\* » : on liste le contenu de l'archive ouverte.
    let directory = crate::archive::opened_root(&directory).unwrap_or(directory);
    Ok((directory, pattern.to_string()))
}

fn enumerate(path: &str) -> Result<Vec<Entry>, u32> {
    let (directory, pattern) = split_search_path(path)?;
    let entries = std::fs::read_dir(&directory).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ERROR_PATH_NOT_FOUND
        } else {
            crate::dlls::kernel32::io_error_code(&e)
        }
    })?;
    let mut found = Vec::new();
    for item in entries {
        let item = item.map_err(|e| crate::dlls::kernel32::io_error_code(&e))?;
        // Nom présenté à l'appli : caractères interdits sous Windows codés.
        let name = crate::paths::win_name(&item.file_name().to_string_lossy());
        if !wildcard_matches(&pattern, &name) {
            continue;
        }
        let (attributes, metadata) = match attributes(&item.path()) {
            Ok(x) => x,
            // Fichier disparu entre la lecture du dossier et maintenant.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(crate::dlls::kernel32::io_error_code(&e)),
        };
        let reparse_tag = if attributes & ATTR_REPARSE_POINT != 0 {
            IO_REPARSE_TAG_SYMLINK
        } else {
            0
        };
        let zero = std::time::UNIX_EPOCH;
        found.push(Entry {
            name,
            attributes,
            creation: filetime(
                metadata
                    .created()
                    .unwrap_or_else(|_| metadata.modified().unwrap_or(zero)),
            ),
            access: filetime(metadata.accessed().unwrap_or(zero)),
            write: filetime(metadata.modified().unwrap_or(zero)),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            reparse_tag,
        });
    }
    found.sort_by_key(|a| a.name.to_lowercase());
    if found.is_empty() {
        Err(ERROR_FILE_NOT_FOUND)
    } else {
        Ok(found)
    }
}

unsafe fn copy_a(e: &Entry, out: *mut Win32FindDataA) {
    std::ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<Win32FindDataA>());
    (*out).attributes = e.attributes;
    (*out).creation_time = e.creation;
    (*out).access_time = e.access;
    (*out).write_time = e.write;
    (*out).size_high = (e.size >> 32) as u32;
    (*out).size_low = e.size as u32;
    (*out).reserved0 = e.reparse_tag;
    let bytes = crate::dlls::encode_ansi(&e.name);
    let n = bytes.len().min((*out).file_name.len() - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), (*out).file_name.as_mut_ptr() as *mut u8, n);
}
unsafe fn copy_w(e: &Entry, out: *mut Win32FindDataW) {
    std::ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<Win32FindDataW>());
    (*out).attributes = e.attributes;
    (*out).creation_time = e.creation;
    (*out).access_time = e.access;
    (*out).write_time = e.write;
    (*out).size_high = (e.size >> 32) as u32;
    (*out).size_low = e.size as u32;
    (*out).reserved0 = e.reparse_tag;
    let mut wide: Vec<u16> = e.name.encode_utf16().collect();
    wide.truncate((*out).file_name.len() - 1);
    if wide.last().is_some_and(|c| (0xD800..=0xDBFF).contains(c)) {
        wide.pop();
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), (*out).file_name.as_mut_ptr(), wide.len());
}

unsafe fn first_common(path: &str, data: *mut c_void, wide: bool) -> usize {
    if data.is_null() {
        error(ERROR_INVALID_PARAMETER);
        return usize::MAX;
    }
    let mut entries = match enumerate(path) {
        Ok(x) => x,
        Err(e) => {
            crate::trace!("FindFirstFile({path}) = échec (erreur {e})");
            error(e);
            return usize::MAX;
        }
    };
    crate::trace!("FindFirstFile({path}) = {} entrée(s)", entries.len());
    let first = entries.remove(0);
    if wide {
        copy_w(&first, data as *mut Win32FindDataW);
    } else {
        copy_a(&first, data as *mut Win32FindDataA);
    }
    let handle = NEXT_SEARCH.fetch_add(1, Ordering::Relaxed);
    lock(searches()).insert(handle, Search { entries, next: 0 });
    handle
}

pub unsafe extern "win64" fn find_first_file_a(
    path: *const u8,
    data: *mut Win32FindDataA,
) -> usize {
    if path.is_null() {
        error(ERROR_INVALID_PARAMETER);
        return usize::MAX;
    }
    first_common(&crate::dlls::cstr(path), data as *mut c_void, false)
}
/// FindFirstFileExW : les niveaux d'information (standard ou « basique ») et
/// le filtre « répertoires seulement » (simple indication sous Windows) se
/// ramènent à FindFirstFileW.
pub unsafe extern "win64" fn find_first_file_ex_w(
    path: *const u16,
    info_level: u32,
    data: *mut Win32FindDataW,
    _search_op: u32,
    _filter: *mut c_void,
    _flags: u32,
) -> usize {
    if info_level > 1 {
        error(ERROR_INVALID_PARAMETER);
        return usize::MAX;
    }
    find_first_file_w(path, data)
}

pub unsafe extern "win64" fn find_first_file_w(
    path: *const u16,
    data: *mut Win32FindDataW,
) -> usize {
    if path.is_null() {
        error(ERROR_INVALID_PARAMETER);
        return usize::MAX;
    }
    let len = crate::printf::wide_len(path);
    let text = match String::from_utf16(std::slice::from_raw_parts(path, len)) {
        Ok(x) => x,
        Err(_) => {
            error(1113);
            return usize::MAX;
        }
    };
    first_common(&text, data as *mut c_void, true)
}
pub unsafe extern "win64" fn find_next_file_a(handle: usize, data: *mut Win32FindDataA) -> i32 {
    if data.is_null() {
        error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut map = lock(searches());
    let Some(search) = map.get_mut(&handle) else {
        error(6);
        return 0;
    };
    let Some(entry) = search.entries.get(search.next).cloned() else {
        error(ERROR_NO_MORE_FILES);
        return 0;
    };
    search.next += 1;
    copy_a(&entry, data);
    1
}
pub unsafe extern "win64" fn find_next_file_w(handle: usize, data: *mut Win32FindDataW) -> i32 {
    if data.is_null() {
        error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut map = lock(searches());
    let Some(search) = map.get_mut(&handle) else {
        error(6);
        return 0;
    };
    let Some(entry) = search.entries.get(search.next).cloned() else {
        error(ERROR_NO_MORE_FILES);
        return 0;
    };
    search.next += 1;
    copy_w(&entry, data);
    1
}
pub extern "win64" fn find_close(handle: usize) -> i32 {
    if lock(searches()).remove(&handle).is_some() {
        1
    } else {
        error(6);
        0
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "FindFirstFileA"=>fn find_first_file_a,"FindFirstFileW"=>fn find_first_file_w,
        "FindFirstFileExW"=>fn find_first_file_ex_w,
        "FindNextFileA"=>fn find_next_file_a,"FindNextFileW"=>fn find_next_file_w,"FindClose"=>fn find_close
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    #[test]
    fn racine_d_un_lecteur_et_non_dossier_courant() {
        // « Z:\* » liste la racine de Z:, jamais le dossier courant du
        // processus (7-Zip, boîte Ouvrir de Notepad++ sur « C:\ »).
        let (dir, pat) = split_search_path("Z:\\*").unwrap();
        assert_eq!(dir, std::path::PathBuf::from("/"));
        assert_eq!(pat, "*");
        let (dir, _) = split_search_path("Z:/*.txt").unwrap();
        assert_eq!(dir, std::path::PathBuf::from("/"));
        let (dir, pat) = split_search_path("Z:\\tmp\\*").unwrap();
        assert_eq!(dir, std::path::PathBuf::from("/tmp"));
        assert_eq!(pat, "*");
    }
    #[test]
    fn prefixe_chemin_long_avec_joker() {
        // #1475 : Steam liste « \\?\C:\…\package\tmp\* » ; le « ? » du
        // préfixe n'est pas un joker (avant : erreur 87 à chaque fois).
        let (dir, pat) = split_search_path("\\\\?\\Z:\\tmp\\*").unwrap();
        assert_eq!(dir, std::path::PathBuf::from("/tmp"));
        assert_eq!(pat, "*");
        let (dir, _) = split_search_path("\\??\\Z:\\tmp\\*.zip").unwrap();
        assert_eq!(dir, std::path::PathBuf::from("/tmp"));
        assert_eq!(
            split_search_path("\\\\?\\Z:\\t?p\\*").unwrap_err(),
            ERROR_INVALID_PARAMETER
        );
        let base = std::env::temp_dir().join(format!("winlux-find-1475-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("libcef.dll_"), b"x").unwrap();
        let win = format!(
            "\\\\?\\Z:{}\\*",
            base.display().to_string().replace('/', "\\")
        );
        let found = enumerate(&win).unwrap();
        assert!(found.iter().any(|e| e.name == "libcef.dll_"), "{win}");
        std::fs::remove_dir_all(&base).unwrap();
    }
    #[test]
    fn etoile_point_etoile_liste_aussi_les_noms_sans_point() {
        // #1125 : wxWidgets énumère « locales\*.* » pour trouver « fr », « de »…
        assert!(wildcard_matches("*.*", "fr"));
        assert!(wildcard_matches("*.*", "LISEZMOI"));
        assert!(wildcard_matches("*.*", "notes.txt"));
        assert!(wildcard_matches("*.", "fr"));
        assert!(!wildcard_matches("*.", "notes.txt"));
        assert!(wildcard_matches("fichier.*", "fichier"));
        assert!(wildcard_matches("fichier.?", "fichier"));
        assert!(!wildcard_matches("*.txt", "fr"));
        assert!(!wildcard_matches("fichier.*", "autre"));
        let dir = std::env::temp_dir().join(format!(
            "winlux-find-1125-{}-{}",
            std::process::id(),
            NEXT_SEARCH.fetch_add(1, Ordering::Relaxed)
        ));
        for lang in ["de", "es", "fr"] {
            std::fs::create_dir_all(dir.join(lang)).unwrap();
            std::fs::write(dir.join(lang).join("filezilla.mo"), b"x").unwrap();
        }
        let names: Vec<String> = enumerate(&format!("{}\\*.*", crate::paths::to_windows(&dir)))
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["de", "es", "fr"]);
    }
    #[test]
    fn enumeration_wildcard_est_insensible_a_la_casse_et_retourne_tailles() {
        let dir = std::env::temp_dir().join(format!(
            "winlux-find-{}-{}",
            std::process::id(),
            NEXT_SEARCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("Alpha.TXT"), b"abc").unwrap();
        std::fs::write(dir.join("beta.txt"), b"12345").unwrap();
        std::fs::write(dir.join("ignore.bin"), b"x").unwrap();
        let win_dir = crate::paths::to_windows(&dir);
        let pattern = format!("{}\\*.txt", win_dir);
        let c = CString::new(crate::dlls::encode_ansi(&pattern)).unwrap();
        let mut data: Win32FindDataA = unsafe { std::mem::zeroed() };
        let h = unsafe { find_first_file_a(c.as_ptr() as *const u8, &mut data) };
        assert_ne!(h, INVALID_HANDLE_VALUE);
        let mut names = vec![unsafe {
            std::ffi::CStr::from_ptr(data.file_name.as_ptr())
                .to_string_lossy()
                .into_owned()
        }];
        while unsafe { find_next_file_a(h, &mut data) } != 0 {
            names.push(unsafe {
                std::ffi::CStr::from_ptr(data.file_name.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            });
        }
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("alpha.txt")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("beta.txt")));
        assert_eq!(find_close(h), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
