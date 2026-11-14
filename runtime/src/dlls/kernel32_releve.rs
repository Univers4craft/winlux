//! kernel32 : les fonctions du lot B du relevé d'imports (#1510).
//!
//! Fonctions importées par le corpus de #26 (pywin32, panda3d, jdk4py,
//! dotnetcore2, Qt, nodejs, electron-winstaller) et absentes jusqu'ici.
//! Chacune fait ce que la documentation publique décrit quand Linux le
//! permet. Sinon, elle échoue avec le code que Windows rend dans le même
//! cas : un utilisateur sans privilège (ERROR_PRIVILEGE_NOT_HELD,
//! ERROR_ACCESS_DENIED) ou une fonction sans équivalent
//! (ERROR_NOT_SUPPORTED).

#![allow(clippy::missing_safety_doc)]

use crate::dlls::{cstr, lookup, wstr, Exports};
use crate::process::set_last_error;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),* $(,)?) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const INVALID_HANDLE_VALUE: usize = usize::MAX;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_HANDLE_EOF: u32 = 38;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_BAD_EXE_FORMAT: u32 = 193;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_PRIVILEGE_NOT_HELD: u32 = 1314;

fn fail(e: u32) -> i32 {
    set_last_error(e);
    0
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copie `text` (sans zéro) dans un tampon de `cap` caractères. Rend la
/// longueur écrite, ou la taille nécessaire, zéro compris, s'il est trop
/// petit (convention de GetWindowsDirectory, SearchPath…).
unsafe fn put_w(buf: *mut u16, cap: u32, text: &[u16]) -> u32 {
    if buf.is_null() || (cap as usize) <= text.len() {
        return text.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len());
    *buf.add(text.len()) = 0;
    text.len() as u32
}

// ---------------------------------------------------------------------
// Handles propres à ce module : tampons d'écran, recherches de noms de
// lien, contextes de sauvegarde.
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Obj {
    ScreenBuffer,
    FileNames,
}

static OBJECTS: Mutex<Option<HashMap<usize, Obj>>> = Mutex::new(None);

fn add_object(kind: Obj) -> usize {
    let h = crate::dlls::kernel32::new_handle_number();
    lock(&OBJECTS)
        .get_or_insert_with(HashMap::new)
        .insert(h, kind);
    h
}

fn object(h: usize) -> Option<Obj> {
    lock(&OBJECTS).as_ref()?.get(&h).copied()
}

/// Tampon d'écran de CreateConsoleScreenBuffer : il écrit dans le même
/// terminal que la sortie standard (voir `canon_std`).
pub(crate) fn is_screen_buffer(h: usize) -> bool {
    h > 0x10 && object(h) == Some(Obj::ScreenBuffer)
}

/// CloseHandle et FindClose : vrai si le handle venait de ce module.
pub(crate) fn close(h: usize) -> bool {
    lock(&OBJECTS)
        .as_mut()
        .is_some_and(|m| m.remove(&h).is_some())
}

// ---------------------------------------------------------------------
// Recherche de fichiers et lancement
// ---------------------------------------------------------------------

unsafe fn file_exists_w(path: &[u16]) -> bool {
    let attrs = api!(
        "kernel32",
        "GetFileAttributesW",
        fn(*const u16) -> u32,
        path.as_ptr()
    )
    .unwrap_or(u32::MAX);
    // Un dossier ne répond pas à SearchPath.
    attrs != u32::MAX && attrs & 0x10 == 0
}

unsafe fn dir_of(f: impl FnOnce(*mut u16, u32) -> u32) -> Option<String> {
    let mut b = vec![0u16; 1024];
    let n = f(b.as_mut_ptr(), b.len() as u32) as usize;
    (n > 0 && n < b.len()).then(|| String::from_utf16_lossy(&b[..n]))
}

/// Dossiers de SearchPath sans chemin donné : celui du programme, le
/// dossier système, le dossier Windows, le dossier courant, puis PATH.
/// Mode de recherche de SearchPath (SetSearchPathMode, documentation
/// Microsoft Learn) : `Some(false)` quand le programme a désactivé le mode
/// sûr (dossier courant juste après celui de l'appli), sinon l'ordre sûr
/// (dossier courant après les dossiers système) ; le booléen dit si le
/// réglage est devenu permanent (#2179).
static SEARCH_MODE: Mutex<(Option<bool>, bool)> = Mutex::new((None, false));

const BASE_SEARCH_PATH_ENABLE_SAFE_SEARCHMODE: u32 = 0x1;
const BASE_SEARCH_PATH_DISABLE_SAFE_SEARCHMODE: u32 = 0x1_0000;
const BASE_SEARCH_PATH_PERMANENT: u32 = 0x8000;

/// SetSearchPathMode(indicateurs) : un seul de ENABLE ou DISABLE, PERMANENT
/// seulement avec ENABLE ; une fois permanent, plus aucun changement
/// (ERROR_ACCESS_DENIED). OBS Studio l'appelle au démarrage.
pub unsafe extern "win64" fn set_search_path_mode(flags: u32) -> i32 {
    let enable = flags & BASE_SEARCH_PATH_ENABLE_SAFE_SEARCHMODE != 0;
    let disable = flags & BASE_SEARCH_PATH_DISABLE_SAFE_SEARCHMODE != 0;
    let permanent = flags & BASE_SEARCH_PATH_PERMANENT != 0;
    let known = BASE_SEARCH_PATH_ENABLE_SAFE_SEARCHMODE
        | BASE_SEARCH_PATH_DISABLE_SAFE_SEARCHMODE
        | BASE_SEARCH_PATH_PERMANENT;
    if flags & !known != 0 || enable == disable || (permanent && disable) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut mode = SEARCH_MODE.lock().unwrap_or_else(|e| e.into_inner());
    if mode.1 {
        return fail(ERROR_ACCESS_DENIED);
    }
    *mode = (Some(enable), permanent);
    1
}

fn safe_search_disabled() -> bool {
    SEARCH_MODE.lock().unwrap_or_else(|e| e.into_inner()).0 == Some(false)
}

unsafe fn default_search_dirs() -> Vec<String> {
    let mut dirs = Vec::new();
    if let Some(exe) = dir_of(|b, n| {
        api!(
            "kernel32",
            "GetModuleFileNameW",
            fn(usize, *mut u16, u32) -> u32,
            0,
            b,
            n
        )
        .unwrap_or(0)
    }) {
        if let Some(i) = exe.rfind('\\') {
            dirs.push(exe[..i].to_string());
        }
    }
    let after_app = dirs.len();
    for name in ["GetSystemDirectoryW", "GetWindowsDirectoryW"] {
        let addr = lookup("kernel32", name);
        if let Some(d) = dir_of(|b, n| match addr {
            Some(a) => {
                std::mem::transmute::<usize, unsafe extern "win64" fn(*mut u16, u32) -> u32>(a)(
                    b, n,
                )
            }
            None => 0,
        }) {
            dirs.push(d);
        }
    }
    if let Some(cwd) = dir_of(|b, n| {
        api!(
            "kernel32",
            "GetCurrentDirectoryW",
            fn(u32, *mut u16) -> u32,
            n,
            b
        )
        .unwrap_or(0)
    }) {
        // Mode sûr désactivé : le dossier courant vient juste après celui
        // de l'appli, avant les dossiers système.
        if safe_search_disabled() {
            dirs.insert(after_app, cwd);
        } else {
            dirs.push(cwd);
        }
    }
    let path = wide("PATH");
    if let Some(p) = dir_of(|b, n| {
        api!(
            "kernel32",
            "GetEnvironmentVariableW",
            fn(*const u16, *mut u16, u32) -> u32,
            path.as_ptr(),
            b,
            n
        )
        .unwrap_or(0)
    }) {
        dirs.extend(p.split(';').filter(|d| !d.is_empty()).map(str::to_string));
    }
    dirs
}

/// Nom complet d'un chemin Windows (GetFullPathNameW).
unsafe fn full_path_w(path: &[u16]) -> Option<Vec<u16>> {
    let mut b = vec![0u16; 1024];
    let mut part: *mut u16 = std::ptr::null_mut();
    let n = api!(
        "kernel32",
        "GetFullPathNameW",
        fn(*const u16, u32, *mut u16, *mut *mut u16) -> u32,
        path.as_ptr(),
        b.len() as u32,
        b.as_mut_ptr(),
        &mut part
    )
    .unwrap_or(0) as usize;
    (n > 0 && n < b.len()).then(|| b[..n].to_vec())
}

/// Candidats de SearchPath, dans l'ordre : `file` (plus `ext` s'il n'a
/// pas d'extension) dans chaque dossier, ou tel quel s'il a un chemin.
pub(crate) fn search_candidates(file: &str, ext: Option<&str>, dirs: &[String]) -> Vec<String> {
    let base = file.rsplit(['\\', '/']).next().unwrap_or(file);
    let name = match ext {
        Some(e) if !base.contains('.') => format!("{file}{e}"),
        _ => file.to_string(),
    };
    let has_path = name.contains(['\\', '/']) || name.as_bytes().get(1) == Some(&b':');
    if has_path {
        return vec![name];
    }
    dirs.iter()
        .map(|d| format!("{}\\{name}", d.trim_end_matches(['\\', '/'])))
        .collect()
}

/// SearchPathW(chemin, fichier, extension, taille, tampon, partie nom).
pub unsafe extern "win64" fn search_path_w(
    path: *const u16,
    file: *const u16,
    ext: *const u16,
    len: u32,
    buf: *mut u16,
    file_part: *mut *mut u16,
) -> u32 {
    if file.is_null() || *file == 0 {
        return fail(ERROR_INVALID_PARAMETER) as u32;
    }
    let file = wstr(file);
    let ext = (!ext.is_null()).then(|| wstr(ext));
    let dirs = if path.is_null() {
        default_search_dirs()
    } else {
        wstr(path)
            .split(';')
            .filter(|d| !d.is_empty())
            .map(str::to_string)
            .collect()
    };
    for cand in search_candidates(&file, ext.as_deref(), &dirs) {
        let w = wide(&cand);
        if !file_exists_w(&w) {
            continue;
        }
        let full = full_path_w(&w).unwrap_or_else(|| w[..w.len() - 1].to_vec());
        let n = put_w(buf, len, &full);
        if n as usize == full.len() && !file_part.is_null() {
            let i = full
                .iter()
                .rposition(|&c| c == b'\\' as u16)
                .map_or(0, |i| i + 1);
            *file_part = buf.add(i);
        }
        return n;
    }
    fail(ERROR_FILE_NOT_FOUND) as u32
}

/// WinExec(ligne de commande, affichage) : CreateProcessA ; plus de 31 en
/// cas de succès, sinon l'ancien code d'erreur (2, 3, 11 ou 0).
pub unsafe extern "win64" fn win_exec(cmd: *const u8, show: u32) -> u32 {
    if cmd.is_null() {
        return 0;
    }
    // STARTUPINFOA x64 : cb, …, dwFlags à 60, wShowWindow à 64.
    let mut si = [0u8; 104];
    si[..4].copy_from_slice(&104u32.to_le_bytes());
    si[60..64].copy_from_slice(&1u32.to_le_bytes()); // STARTF_USESHOWWINDOW
    si[64..66].copy_from_slice(&(show as u16).to_le_bytes());
    let mut pi = [0usize; 3];
    let mut line = cstr(cmd).into_bytes();
    line.push(0);
    let ok = api!(
        "kernel32",
        "CreateProcessA",
        fn(
            *const u8,
            *mut u8,
            usize,
            usize,
            i32,
            u32,
            usize,
            *const u8,
            *mut u8,
            *mut usize,
        ) -> i32,
        std::ptr::null(),
        line.as_mut_ptr(),
        0,
        0,
        0,
        0,
        0,
        std::ptr::null(),
        si.as_mut_ptr(),
        pi.as_mut_ptr()
    )
    .unwrap_or(0);
    if ok == 0 {
        return match crate::process::last_error() {
            2 => 2,
            3 => 3,
            193 => 11,
            _ => 0,
        };
    }
    for h in &pi[..2] {
        crate::dlls::kernel32::close_handle(*h);
    }
    33
}

/// OpenFile(nom, OFSTRUCT, style) : interface 16 bits de fichiers. Le
/// handle rendu est un HFILE (entier de 32 bits), -1 en cas d'échec.
pub unsafe extern "win64" fn open_file(name: *const u8, of: *mut u8, style: u32) -> i32 {
    const OF_PARSE: u32 = 0x100;
    const OF_DELETE: u32 = 0x200;
    const OF_CREATE: u32 = 0x1000;
    const OF_EXIST: u32 = 0x4000;
    if name.is_null() || of.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    // OFSTRUCT : cBytes, fFixedDisk, nErrCode (u16), 2 × u16, chemin[128].
    std::ptr::write_bytes(of, 0, 136);
    *of = 136;
    *of.add(1) = 1;
    let mut full = [0u8; 260];
    let n = api!(
        "kernel32",
        "GetFullPathNameA",
        fn(*const u8, u32, *mut u8, usize) -> u32,
        name,
        full.len() as u32,
        full.as_mut_ptr(),
        0
    )
    .unwrap_or(0) as usize;
    let n = n.min(127);
    std::ptr::copy_nonoverlapping(full.as_ptr(), of.add(8), n);
    let set_err = |of: *mut u8| {
        let e = crate::process::last_error() as u16;
        std::ptr::copy_nonoverlapping(e.to_le_bytes().as_ptr(), of.add(2), 2);
    };
    if style & OF_PARSE != 0 {
        return 0;
    }
    if style & OF_DELETE != 0 {
        if api!("kernel32", "DeleteFileA", fn(*const u8) -> i32, name).unwrap_or(0) != 0 {
            return 1;
        }
        set_err(of);
        return -1;
    }
    let access = match style & 3 {
        1 => 0x4000_0000u32,
        2 => 0xC000_0000,
        _ => 0x8000_0000,
    };
    let disposition = if style & OF_CREATE != 0 { 2 } else { 3 }; // CREATE_ALWAYS / OPEN_EXISTING
    let access = if style & OF_CREATE != 0 {
        access | 0x4000_0000
    } else {
        access
    };
    let h = api!(
        "kernel32",
        "CreateFileA",
        fn(*const u8, u32, u32, usize, u32, u32, usize) -> usize,
        name,
        access,
        3,
        0,
        disposition,
        0x80,
        0
    )
    .unwrap_or(INVALID_HANDLE_VALUE);
    if h == INVALID_HANDLE_VALUE {
        set_err(of);
        return -1;
    }
    if style & OF_EXIST != 0 {
        crate::dlls::kernel32::close_handle(h);
        return 1;
    }
    h as i32
}

/// CreateFile2(nom, accès, partage, disposition, paramètres étendus).
pub unsafe extern "win64" fn create_file_2(
    name: *const u16,
    access: u32,
    share: u32,
    disposition: u32,
    params: *const u8,
) -> usize {
    // CREATEFILE2_EXTENDED_PARAMETERS : dwSize, attributs, drapeaux, QoS,
    // lpSecurityAttributes (+16), hTemplateFile (+24).
    let (flags, sa, template) = if params.is_null() {
        (0x80, 0usize, 0usize)
    } else {
        let rd = |o: usize| (params.add(o) as *const u32).read_unaligned();
        (
            rd(4) | rd(8) | rd(12),
            (params.add(16) as *const usize).read_unaligned(),
            (params.add(24) as *const usize).read_unaligned(),
        )
    };
    api!(
        "kernel32",
        "CreateFileW",
        fn(*const u16, u32, u32, usize, u32, u32, usize) -> usize,
        name,
        access,
        share,
        sa,
        disposition,
        flags,
        template
    )
    .unwrap_or(INVALID_HANDLE_VALUE)
}

/// Type d'exécutable d'après son en-tête (GetBinaryType).
pub(crate) fn binary_type(head: &[u8], path: &str) -> Option<u32> {
    const SCS_32BIT: u32 = 0;
    const SCS_DOS: u32 = 1;
    const SCS_WOW: u32 = 2;
    const SCS_PIF: u32 = 3;
    const SCS_OS216: u32 = 5;
    const SCS_64BIT: u32 = 6;
    let lower = path.to_ascii_lowercase();
    if head.len() < 2 || &head[..2] != b"MZ" {
        return if lower.ends_with(".com") {
            Some(SCS_DOS)
        } else if lower.ends_with(".pif") {
            Some(SCS_PIF)
        } else {
            None
        };
    }
    let lfanew = head
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let sig = lfanew.and_then(|o| head.get(o..o + 6));
    Some(match sig {
        Some(s) if &s[..4] == b"PE\0\0" => match u16::from_le_bytes([s[4], s[5]]) {
            0x014c => SCS_32BIT,
            0x8664 | 0xaa64 => SCS_64BIT,
            _ => return None,
        },
        Some(s) if &s[..2] == b"NE" => SCS_WOW,
        Some(s) if &s[..2] == b"LE" || &s[..2] == b"LX" => SCS_OS216,
        _ => SCS_DOS,
    })
}

pub unsafe extern "win64" fn get_binary_type_w(path: *const u16, kind: *mut u32) -> i32 {
    if path.is_null() || kind.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let p = wstr(path);
    let Ok(data) = std::fs::read(crate::paths::to_linux(&p)).map(|mut d| {
        d.truncate(4096);
        d
    }) else {
        return fail(ERROR_FILE_NOT_FOUND);
    };
    match binary_type(&data, &p) {
        Some(t) => {
            *kind = t;
            1
        }
        None => fail(ERROR_BAD_EXE_FORMAT),
    }
}

// ---------------------------------------------------------------------
// Variantes « Transacted » : les transactions de ktmw32 n'ont pas
// d'isolation (voir MoveFileTransactedW, #1475) ; l'opération est faite
// tout de suite, comme sa version ordinaire.
// ---------------------------------------------------------------------

pub unsafe extern "win64" fn copy_file_transacted_w(
    src: *const u16,
    dst: *const u16,
    progress: usize,
    data: usize,
    cancel: *mut i32,
    flags: u32,
    _tx: usize,
) -> i32 {
    api!(
        "kernel32",
        "CopyFileExW",
        fn(*const u16, *const u16, usize, usize, *mut i32, u32) -> i32,
        src,
        dst,
        progress,
        data,
        cancel,
        flags
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn create_directory_transacted_w(
    template: *const u16,
    dir: *const u16,
    sa: usize,
    _tx: usize,
) -> i32 {
    if template.is_null() {
        api!(
            "kernel32",
            "CreateDirectoryW",
            fn(*const u16, usize) -> i32,
            dir,
            sa
        )
        .unwrap_or(0)
    } else {
        api!(
            "kernel32",
            "CreateDirectoryExW",
            fn(*const u16, *const u16, usize) -> i32,
            template,
            dir,
            sa
        )
        .unwrap_or(0)
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn create_file_transacted_w(
    name: *const u16,
    access: u32,
    share: u32,
    sa: usize,
    disposition: u32,
    flags: u32,
    template: usize,
    _tx: usize,
    _mini_version: *const u16,
    _extended: usize,
) -> usize {
    api!(
        "kernel32",
        "CreateFileW",
        fn(*const u16, u32, u32, usize, u32, u32, usize) -> usize,
        name,
        access,
        share,
        sa,
        disposition,
        flags,
        template
    )
    .unwrap_or(INVALID_HANDLE_VALUE)
}

pub unsafe extern "win64" fn create_hard_link_transacted_w(
    new: *const u16,
    existing: *const u16,
    sa: usize,
    _tx: usize,
) -> i32 {
    api!(
        "kernel32",
        "CreateHardLinkW",
        fn(*const u16, *const u16, usize) -> i32,
        new,
        existing,
        sa
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn create_symbolic_link_transacted_w(
    link: *const u16,
    target: *const u16,
    flags: u32,
    _tx: usize,
) -> u8 {
    api!(
        "kernel32",
        "CreateSymbolicLinkW",
        fn(*const u16, *const u16, u32) -> u8,
        link,
        target,
        flags
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn find_first_file_transacted_w(
    name: *const u16,
    level: u32,
    data: *mut c_void,
    op: u32,
    filter: usize,
    flags: u32,
    _tx: usize,
) -> usize {
    api!(
        "kernel32",
        "FindFirstFileExW",
        fn(*const u16, u32, *mut c_void, u32, usize, u32) -> usize,
        name,
        level,
        data,
        op,
        filter,
        flags
    )
    .unwrap_or(INVALID_HANDLE_VALUE)
}

pub unsafe extern "win64" fn find_first_stream_transacted_w(
    name: *const u16,
    level: u32,
    data: *mut c_void,
    flags: u32,
    _tx: usize,
) -> usize {
    api!(
        "kernel32",
        "FindFirstStreamW",
        fn(*const u16, u32, *mut c_void, u32) -> usize,
        name,
        level,
        data,
        flags
    )
    .unwrap_or(INVALID_HANDLE_VALUE)
}

pub unsafe extern "win64" fn find_first_file_name_transacted_w(
    name: *const u16,
    flags: u32,
    len: *mut u32,
    buf: *mut u16,
    _tx: usize,
) -> usize {
    find_first_file_name_w(name, flags, len, buf)
}

pub unsafe extern "win64" fn get_file_attributes_transacted_w(
    name: *const u16,
    level: u32,
    info: *mut c_void,
    _tx: usize,
) -> i32 {
    api!(
        "kernel32",
        "GetFileAttributesExW",
        fn(*const u16, u32, *mut c_void) -> i32,
        name,
        level,
        info
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_file_attributes_transacted_a(
    name: *const u8,
    level: u32,
    info: *mut c_void,
    _tx: usize,
) -> i32 {
    api!(
        "kernel32",
        "GetFileAttributesExA",
        fn(*const u8, u32, *mut c_void) -> i32,
        name,
        level,
        info
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_full_path_name_transacted_w(
    name: *const u16,
    len: u32,
    buf: *mut u16,
    part: *mut *mut u16,
    _tx: usize,
) -> u32 {
    api!(
        "kernel32",
        "GetFullPathNameW",
        fn(*const u16, u32, *mut u16, *mut *mut u16) -> u32,
        name,
        len,
        buf,
        part
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_full_path_name_transacted_a(
    name: *const u8,
    len: u32,
    buf: *mut u8,
    part: *mut *mut u8,
    _tx: usize,
) -> u32 {
    api!(
        "kernel32",
        "GetFullPathNameA",
        fn(*const u8, u32, *mut u8, *mut *mut u8) -> u32,
        name,
        len,
        buf,
        part
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_long_path_name_transacted_w(
    short: *const u16,
    buf: *mut u16,
    len: u32,
    _tx: usize,
) -> u32 {
    api!(
        "kernel32",
        "GetLongPathNameW",
        fn(*const u16, *mut u16, u32) -> u32,
        short,
        buf,
        len
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn remove_directory_transacted_w(path: *const u16, _tx: usize) -> i32 {
    api!("kernel32", "RemoveDirectoryW", fn(*const u16) -> i32, path).unwrap_or(0)
}

pub unsafe extern "win64" fn set_file_attributes_transacted_w(
    name: *const u16,
    attrs: u32,
    _tx: usize,
) -> i32 {
    api!(
        "kernel32",
        "SetFileAttributesW",
        fn(*const u16, u32) -> i32,
        name,
        attrs
    )
    .unwrap_or(0)
}

// ---------------------------------------------------------------------
// Noms de liens physiques (FindFirstFileNameW)
// ---------------------------------------------------------------------

/// Chemin d'un fichier depuis la racine de son volume (« \dossier\nom »),
/// la forme que rend FindFirstFileNameW.
pub(crate) fn volume_relative(full: &[u16]) -> Vec<u16> {
    if full.len() >= 2 && full[1] == b':' as u16 {
        full[2..].to_vec()
    } else {
        full.to_vec()
    }
}

/// FindFirstFileNameW : un seul nom est connu, celui demandé (Linux ne
/// liste pas les autres liens d'un inœud sans parcourir le volume). La
/// suite rend ERROR_HANDLE_EOF.
pub unsafe extern "win64" fn find_first_file_name_w(
    name: *const u16,
    _flags: u32,
    len: *mut u32,
    buf: *mut u16,
) -> usize {
    if name.is_null() || len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return INVALID_HANDLE_VALUE;
    }
    let w = wide(&wstr(name));
    if !crate::paths::to_linux(&wstr(name)).exists() {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return INVALID_HANDLE_VALUE;
    }
    let Some(full) = full_path_w(&w) else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return INVALID_HANDLE_VALUE;
    };
    let rel = volume_relative(&full);
    if buf.is_null() || (*len as usize) <= rel.len() {
        *len = rel.len() as u32 + 1;
        set_last_error(ERROR_MORE_DATA);
        return INVALID_HANDLE_VALUE;
    }
    put_w(buf, *len, &rel);
    *len = rel.len() as u32 + 1;
    add_object(Obj::FileNames)
}

pub unsafe extern "win64" fn find_next_file_name_w(
    h: usize,
    _len: *mut u32,
    _buf: *mut u16,
) -> i32 {
    if object(h) != Some(Obj::FileNames) {
        return fail(ERROR_INVALID_HANDLE);
    }
    fail(ERROR_HANDLE_EOF)
}

// ---------------------------------------------------------------------
// BackupRead / BackupWrite / BackupSeek : flux WIN32_STREAM_ID. Linux
// n'a ni flux alternatifs ni descripteurs NTFS : seul le flux de données
// (BACKUP_DATA) est produit ou restauré, les autres flux sont sautés.
// ---------------------------------------------------------------------

const BACKUP_DATA: u32 = 1;
const STREAM_HEADER: usize = 20;

#[derive(Default)]
struct BackupState {
    /// En-tête du flux en cours (lu ou à écrire).
    header: Vec<u8>,
    header_pos: usize,
    /// Octets du nom de flux restant à sauter (écriture).
    name_left: u64,
    /// Octets de données restant dans le flux en cours.
    remaining: u64,
    stream_id: u32,
    started: bool,
    done: bool,
}

unsafe fn backup_state(ctx: *mut usize) -> &'static mut BackupState {
    if *ctx == 0 {
        *ctx = Box::into_raw(Box::<BackupState>::default()) as usize;
    }
    &mut *(*ctx as *mut BackupState)
}

unsafe fn backup_free(ctx: *mut usize) {
    if !ctx.is_null() && *ctx != 0 {
        drop(Box::from_raw(*ctx as *mut BackupState));
        *ctx = 0;
    }
}

/// En-tête WIN32_STREAM_ID d'un flux de données sans nom.
pub(crate) fn data_stream_header(size: u64) -> Vec<u8> {
    let mut h = Vec::with_capacity(STREAM_HEADER);
    h.extend_from_slice(&BACKUP_DATA.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes()); // attributs
    h.extend_from_slice(&size.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes()); // taille du nom
    h
}

unsafe fn rw_file(name: &'static str, h: usize, buf: *mut u8, n: u32, done: &mut u32) -> bool {
    let f: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32, usize) -> i32 =
        match lookup("kernel32", name) {
            Some(a) => std::mem::transmute::<
                usize,
                unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32, usize) -> i32,
            >(a),
            None => return false,
        };
    f(h, buf, n, done, 0) != 0
}

pub unsafe extern "win64" fn backup_read(
    h: usize,
    buf: *mut u8,
    n: u32,
    read: *mut u32,
    abort: i32,
    _security: i32,
    ctx: *mut usize,
) -> i32 {
    if ctx.is_null() || read.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if abort != 0 {
        backup_free(ctx);
        return 1;
    }
    let st = backup_state(ctx);
    *read = 0;
    if !st.started {
        st.started = true;
        let mut size = 0i64;
        let is_file = api!(
            "kernel32",
            "GetFileSizeEx",
            fn(usize, *mut i64) -> i32,
            h,
            &mut size
        )
        .unwrap_or(0)
            != 0
            && api!("kernel32", "GetFileType", fn(usize) -> u32, h).unwrap_or(0) == 1;
        if is_file && size > 0 {
            st.header = data_stream_header(size as u64);
            st.remaining = size as u64;
        } else {
            st.done = true;
        }
    }
    let mut out = 0u32;
    while out < n && !st.done {
        if st.header_pos < st.header.len() {
            let k = (st.header.len() - st.header_pos).min((n - out) as usize);
            std::ptr::copy_nonoverlapping(
                st.header.as_ptr().add(st.header_pos),
                buf.add(out as usize),
                k,
            );
            st.header_pos += k;
            out += k as u32;
            continue;
        }
        let want = st.remaining.min((n - out) as u64) as u32;
        if want == 0 {
            st.done = true;
            break;
        }
        let mut got = 0u32;
        if !rw_file("ReadFile", h, buf.add(out as usize), want, &mut got) {
            *read = out;
            return 0;
        }
        if got == 0 {
            st.done = true;
            break;
        }
        st.remaining -= got as u64;
        out += got;
    }
    *read = out;
    1
}

pub unsafe extern "win64" fn backup_seek(
    h: usize,
    low: u32,
    high: u32,
    seeked_low: *mut u32,
    seeked_high: *mut u32,
    ctx: *mut usize,
) -> i32 {
    if ctx.is_null() || *ctx == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let st = &mut *(*ctx as *mut BackupState);
    let want = (high as u64) << 32 | low as u64;
    // On ne saute que dans les données du flux en cours, jamais au-delà.
    let can = if st.header_pos >= st.header.len() {
        st.remaining
    } else {
        0
    };
    let step = want.min(can);
    if step > 0
        && api!(
            "kernel32",
            "SetFilePointerEx",
            fn(usize, i64, *mut i64, u32) -> i32,
            h,
            step as i64,
            std::ptr::null_mut(),
            1
        )
        .unwrap_or(0)
            == 0
    {
        return 0;
    }
    st.remaining -= step;
    if !seeked_low.is_null() {
        *seeked_low = step as u32;
    }
    if !seeked_high.is_null() {
        *seeked_high = (step >> 32) as u32;
    }
    if step < want {
        return fail(ERROR_HANDLE_EOF);
    }
    1
}

pub unsafe extern "win64" fn backup_write(
    h: usize,
    buf: *const u8,
    n: u32,
    written: *mut u32,
    abort: i32,
    _security: i32,
    ctx: *mut usize,
) -> i32 {
    if ctx.is_null() || written.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if abort != 0 {
        backup_free(ctx);
        return 1;
    }
    let st = backup_state(ctx);
    let mut pos = 0usize;
    let n = n as usize;
    while pos < n {
        if st.header.len() < STREAM_HEADER {
            let k = (STREAM_HEADER - st.header.len()).min(n - pos);
            st.header
                .extend_from_slice(std::slice::from_raw_parts(buf.add(pos), k));
            pos += k;
            if st.header.len() == STREAM_HEADER {
                let h4 = |o: usize| u32::from_le_bytes(st.header[o..o + 4].try_into().unwrap());
                st.stream_id = h4(0);
                st.remaining = u64::from_le_bytes(st.header[8..16].try_into().unwrap());
                st.name_left = h4(16) as u64;
            }
            continue;
        }
        if st.name_left > 0 {
            let k = st.name_left.min((n - pos) as u64);
            st.name_left -= k;
            pos += k as usize;
            continue;
        }
        let k = st.remaining.min((n - pos) as u64) as usize;
        if k > 0 && st.stream_id == BACKUP_DATA {
            let mut done = 0u32;
            if !rw_file("WriteFile", h, buf.add(pos) as *mut u8, k as u32, &mut done) {
                *written = pos as u32;
                return 0;
            }
        }
        st.remaining -= k as u64;
        pos += k;
        if st.remaining == 0 {
            // Flux suivant.
            st.header.clear();
        }
    }
    *written = n as u32;
    1
}

// ---------------------------------------------------------------------
// Console : le terminal n'a qu'une surface ; les tampons d'écran
// supplémentaires écrivent au même endroit et l'écran alternatif du
// terminal sert de tampon actif. Le contenu n'est pas relisible.
// ---------------------------------------------------------------------

fn vt(s: &str) -> bool {
    use std::io::Write;
    let mut o = std::io::stdout();
    o.write_all(s.as_bytes()).and_then(|_| o.flush()).is_ok()
}

fn is_console_out(h: usize) -> bool {
    let h = crate::dlls::kernel32::canon_std(h);
    h == (-11isize) as usize || h == (-12isize) as usize
}

fn terminal_size() -> (u16, u16) {
    // SAFETY : ioctl de lecture sur la sortie standard.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            (ws.ws_col, ws.ws_row.max(1))
        } else {
            (80, 25)
        }
    }
}

pub unsafe extern "win64" fn create_console_screen_buffer(
    _access: u32,
    _share: u32,
    _sa: usize,
    flags: u32,
    _data: usize,
) -> usize {
    // CONSOLE_TEXTMODE_BUFFER = 1, seul mode documenté.
    if flags != 1 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return INVALID_HANDLE_VALUE;
    }
    add_object(Obj::ScreenBuffer)
}

pub extern "win64" fn set_console_active_screen_buffer(h: usize) -> i32 {
    if is_screen_buffer(h) {
        vt("\x1b[?1049h\x1b[H") as i32
    } else if is_console_out(h) {
        vt("\x1b[?1049l") as i32
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

/// SetConsoleWindowInfo : la fenêtre est tout le terminal ; un rectangle
/// qui en sort est refusé comme sous Windows.
pub unsafe extern "win64" fn set_console_window_info(
    h: usize,
    absolute: i32,
    rect: *const i16,
) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if rect.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let r: [i16; 4] = std::ptr::read_unaligned(rect as *const [i16; 4]);
    let (cols, rows) = terminal_size();
    let ok = if absolute != 0 {
        r[0] >= 0
            && r[1] >= 0
            && r[2] < cols as i16
            && r[3] < rows as i16
            && r[0] <= r[2]
            && r[1] <= r[3]
    } else {
        r == [0; 4]
    };
    if ok {
        1
    } else {
        fail(ERROR_INVALID_PARAMETER)
    }
}

/// Séquence SGR d'un attribut de console (FOREGROUND_* / BACKGROUND_*).
pub(crate) fn sgr(attr: u16) -> String {
    let idx = |a: u16| (a & 4) >> 2 | (a & 2) | (a & 1) << 2; // RGB → ANSI
    let fg = idx(attr & 7);
    let bg = idx((attr >> 4) & 7);
    let fgc = if attr & 8 != 0 { 90 + fg } else { 30 + fg };
    let bgc = if attr & 0x80 != 0 { 100 + bg } else { 40 + bg };
    format!("\x1b[{fgc};{bgc}m")
}

fn coord(c: u32) -> (i32, i32) {
    ((c & 0xFFFF) as i16 as i32, (c >> 16) as i16 as i32)
}

/// WriteConsoleOutputW(h, CHAR_INFO[], taille, origine, région).
pub unsafe extern "win64" fn write_console_output_w(
    h: usize,
    cells: *const u16,
    size: u32,
    origin: u32,
    region: *mut i16,
) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if cells.is_null() || region.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (w, hgt) = coord(size);
    let (ox, oy) = coord(origin);
    let mut r: [i16; 4] = std::ptr::read_unaligned(region as *const [i16; 4]);
    let (cols, rows) = terminal_size();
    r[2] = r[2].min(r[0] + (w - ox) as i16 - 1).min(cols as i16 - 1);
    r[3] = r[3].min(r[1] + (hgt - oy) as i16 - 1).min(rows as i16 - 1);
    let mut out = String::from("\x1b7");
    let mut last = u16::MAX;
    for y in r[1].max(0)..=r[3] {
        out.push_str(&format!("\x1b[{};{}H", y + 1, r[0].max(0) + 1));
        for x in r[0].max(0)..=r[2] {
            let sx = ox + (x - r[0]) as i32;
            let sy = oy + (y - r[1]) as i32;
            let i = (sy * w + sx) as usize * 2;
            let ch = *cells.add(i);
            let attr = *cells.add(i + 1);
            if attr != last {
                out.push_str(&sgr(attr));
                last = attr;
            }
            out.push(
                char::from_u32(ch as u32)
                    .filter(|c| !c.is_control())
                    .unwrap_or(' '),
            );
        }
    }
    out.push_str("\x1b[0m\x1b8");
    std::ptr::write_unaligned(region as *mut [i16; 4], r);
    vt(&out) as i32
}

pub unsafe extern "win64" fn write_console_output_character_w(
    h: usize,
    text: *const u16,
    len: u32,
    pos: u32,
    written: *mut u32,
) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if text.is_null() && len > 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (x, y) = coord(pos);
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, len as usize));
    let ok = vt(&format!("\x1b7\x1b[{};{}H{s}\x1b8", y + 1, x + 1));
    if ok && !written.is_null() {
        *written = len;
    }
    ok as i32
}

/// WriteConsoleOutputAttribute : le terminal ne recolore pas un texte déjà
/// écrit ; l'appel réussit, comme FillConsoleOutputAttribute.
pub unsafe extern "win64" fn write_console_output_attribute(
    h: usize,
    _attrs: *const u16,
    len: u32,
    _pos: u32,
    written: *mut u32,
) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if !written.is_null() {
        *written = len;
    }
    1
}

/// Lecture du contenu de l'écran : le terminal ne le rend pas.
pub unsafe extern "win64" fn read_console_output_unsupported(
    h: usize,
    _buf: *mut u16,
    _len: u32,
    _pos: u32,
    read: *mut u32,
) -> i32 {
    if !read.is_null() {
        *read = 0;
    }
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    fail(ERROR_NOT_SUPPORTED)
}

/// ScrollConsoleScreenBufferW : défilement vertical de lignes entières,
/// le cas des programmes en mode texte ; le reste n'est pas pris en charge.
pub unsafe extern "win64" fn scroll_console_screen_buffer_w(
    h: usize,
    scroll: *const i16,
    _clip: *const i16,
    dest: u32,
    _fill: *const u16,
) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if scroll.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let r: [i16; 4] = std::ptr::read_unaligned(scroll as *const [i16; 4]);
    let (cols, _) = terminal_size();
    let (dx, dy) = coord(dest);
    if r[0] != 0 || r[2] < cols as i16 - 1 || dx != 0 {
        return fail(ERROR_NOT_SUPPORTED);
    }
    let shift = r[1] as i32 - dy;
    let region = format!("\x1b7\x1b[{};{}r", r[1].min(dy as i16) + 1, r[3] + 1);
    let seq = match shift {
        0 => return 1,
        s if s > 0 => format!("{region}\x1b[{s}S\x1b[r\x1b8"),
        s => format!("{region}\x1b[{}T\x1b[r\x1b8", -s),
    };
    vt(&seq) as i32
}

pub unsafe extern "win64" fn get_console_display_mode(flags: *mut u32) -> i32 {
    if flags.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    *flags = 0; // fenêtre
    1
}

/// SetConsoleDisplayMode : seul le mode fenêtre (2) existe, comme sur
/// Windows depuis Vista.
pub unsafe extern "win64" fn set_console_display_mode(h: usize, mode: u32, size: *mut u32) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if mode != 2 {
        return fail(ERROR_NOT_SUPPORTED);
    }
    if !size.is_null() {
        let (c, r) = terminal_size();
        *size = c as u32 | (r as u32) << 16;
    }
    1
}

/// Cellule du terminal : 8 × 16 points, la police de la console.
const FONT_SIZE: u32 = 8 | 16 << 16;

pub extern "win64" fn get_console_font_size(h: usize, _font: u32) -> u32 {
    if !is_console_out(h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    FONT_SIZE
}

pub unsafe extern "win64" fn get_current_console_font(h: usize, _max: i32, info: *mut u32) -> i32 {
    if !is_console_out(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    *info = 0;
    *info.add(1) = FONT_SIZE;
    1
}

pub unsafe extern "win64" fn get_console_process_list(list: *mut u32, count: u32) -> u32 {
    if list.is_null() || count == 0 {
        return fail(ERROR_INVALID_PARAMETER) as u32;
    }
    *list = std::process::id();
    1
}

pub unsafe extern "win64" fn get_console_selection_info(info: *mut u8) -> i32 {
    if info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    // CONSOLE_SELECTION_INFO : drapeaux (aucune sélection), ancre, rectangle.
    std::ptr::write_bytes(info, 0, 16);
    1
}

pub extern "win64" fn get_largest_console_window_size(h: usize) -> u32 {
    if !is_console_out(h) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    let (c, r) = terminal_size();
    c as u32 | (r as u32) << 16
}

// Alias de console (doskey) : table du processus, par programme.
static ALIASES: Mutex<Vec<(String, Vec<(String, String)>)>> = Mutex::new(Vec::new());

pub unsafe extern "win64" fn add_console_alias_w(
    source: *const u16,
    target: *const u16,
    exe: *const u16,
) -> i32 {
    if source.is_null() || exe.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (src, exe) = (wstr(source), wstr(exe).to_lowercase());
    let mut all = lock(&ALIASES);
    let i = match all.iter().position(|(e, _)| *e == exe) {
        Some(i) => i,
        None => {
            all.push((exe, Vec::new()));
            all.len() - 1
        }
    };
    let list = &mut all[i].1;
    list.retain(|(s, _)| !s.eq_ignore_ascii_case(&src));
    if !target.is_null() && *target != 0 {
        list.push((src, wstr(target)));
    }
    1
}

fn aliases_text(exe: &str) -> Vec<u16> {
    let exe = exe.to_lowercase();
    let all = lock(&ALIASES);
    let mut out = Vec::new();
    if let Some((_, list)) = all.iter().find(|(e, _)| *e == exe) {
        for (s, t) in list {
            out.extend(format!("{s}={t}").encode_utf16());
            out.push(0);
        }
    }
    out
}

fn alias_exes_text() -> Vec<u16> {
    let mut out = Vec::new();
    for (e, list) in lock(&ALIASES).iter() {
        if !list.is_empty() {
            out.extend(e.encode_utf16());
            out.push(0);
        }
    }
    out
}

/// Copie une liste « a\0b\0 » dans un tampon dont la taille est en octets.
unsafe fn put_list(buf: *mut u16, bytes: u32, text: &[u16]) -> u32 {
    if buf.is_null() || (bytes as usize) < text.len() * 2 {
        return fail(ERROR_INSUFFICIENT_BUFFER) as u32;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len());
    (text.len() * 2) as u32
}

pub unsafe extern "win64" fn get_console_aliases_w(
    buf: *mut u16,
    bytes: u32,
    exe: *const u16,
) -> u32 {
    if exe.is_null() {
        return fail(ERROR_INVALID_PARAMETER) as u32;
    }
    put_list(buf, bytes, &aliases_text(&wstr(exe)))
}

pub unsafe extern "win64" fn get_console_aliases_length_w(exe: *const u16) -> u32 {
    if exe.is_null() {
        return 0;
    }
    (aliases_text(&wstr(exe)).len() * 2) as u32
}

pub unsafe extern "win64" fn get_console_alias_exes_w(buf: *mut u16, bytes: u32) -> u32 {
    put_list(buf, bytes, &alias_exes_text())
}

pub extern "win64" fn get_console_alias_exes_length_w() -> u32 {
    (alias_exes_text().len() * 2) as u32
}

// ---------------------------------------------------------------------
// Processus, mémoire, processeurs
// ---------------------------------------------------------------------

/// PROCESSENTRY32 (ANSI) depuis la version large : mêmes champs jusqu'au
/// nom (44 octets), nom de 260 caractères ANSI.
unsafe fn process_entry_a(snap: usize, entry: *mut u8, first: bool) -> i32 {
    if entry.is_null() || (entry as *const u32).read_unaligned() < 304 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut w = [0u8; 568];
    w[..4].copy_from_slice(&568u32.to_le_bytes());
    let ok = if first {
        crate::dlls::kernel32_node::Process32FirstW(snap, w.as_mut_ptr())
    } else {
        crate::dlls::kernel32_node::Process32NextW(snap, w.as_mut_ptr())
    };
    if ok == 0 {
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr().add(4), entry.add(4), 40);
    let name: Vec<u16> = w[44..44 + 520]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    let mut a = crate::dlls::encode_ansi(&String::from_utf16_lossy(&name));
    a.truncate(259);
    a.push(0);
    std::ptr::write_bytes(entry.add(44), 0, 260);
    std::ptr::copy_nonoverlapping(a.as_ptr(), entry.add(44), a.len());
    1
}

pub unsafe extern "win64" fn process32_first(snap: usize, entry: *mut u8) -> i32 {
    process_entry_a(snap, entry, true)
}

pub unsafe extern "win64" fn process32_next(snap: usize, entry: *mut u8) -> i32 {
    process_entry_a(snap, entry, false)
}

pub unsafe extern "win64" fn virtual_alloc_ex_numa(
    process: usize,
    addr: usize,
    size: usize,
    kind: u32,
    protect: u32,
    _node: u32,
) -> usize {
    api!(
        "kernel32",
        "VirtualAllocEx",
        fn(usize, usize, usize, u32, u32) -> usize,
        process,
        addr,
        size,
        kind,
        protect
    )
    .unwrap_or(0)
}

/// Pages physiques AWE : elles exigent SeLockMemoryPrivilege, qu'un
/// compte ordinaire n'a pas.
pub extern "win64" fn awe_privilege_not_held() -> i32 {
    fail(ERROR_PRIVILEGE_NOT_HELD)
}

fn cpu_count() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}

/// GetNumaProcessorNodeEx(PROCESSOR_NUMBER, nœud) : un seul nœud NUMA.
pub unsafe extern "win64" fn get_numa_processor_node_ex(pn: *const u8, node: *mut u16) -> i32 {
    if pn.is_null() || node.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let group = (pn as *const u16).read_unaligned();
    let number = *pn.add(2) as u32;
    if group != 0 || number >= cpu_count() {
        *node = u16::MAX;
        return fail(ERROR_INVALID_PARAMETER);
    }
    *node = 0;
    1
}

unsafe fn put_processor(p: *mut u8, number: u32) {
    if !p.is_null() {
        (p as *mut u16).write_unaligned(0);
        *p.add(2) = number as u8;
        *p.add(3) = 0;
    }
}

pub unsafe extern "win64" fn set_thread_ideal_processor_ex(
    thread: usize,
    ideal: *const u8,
    previous: *mut u8,
) -> i32 {
    if ideal.is_null() || (ideal as *const u16).read_unaligned() != 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let old = crate::dlls::kernel32_cpu::set_thread_ideal_processor(thread, *ideal.add(2) as u32);
    if old == u32::MAX {
        return 0;
    }
    put_processor(previous, old);
    1
}

pub unsafe extern "win64" fn get_thread_ideal_processor_ex(thread: usize, ideal: *mut u8) -> i32 {
    if ideal.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    // MAXIMUM_PROCESSORS (64) : lit le processeur idéal sans le changer.
    let cur = crate::dlls::kernel32_cpu::set_thread_ideal_processor(thread, 64);
    if cur == u32::MAX {
        return 0;
    }
    put_processor(ideal, cur);
    1
}

pub unsafe extern "win64" fn get_process_priority_boost(
    _process: usize,
    disabled: *mut i32,
) -> i32 {
    if disabled.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    *disabled = 0;
    1
}

/// GetProcessVersion : version de sous-système de l'image principale
/// (MajorSubsystemVersion << 16 | Minor) pour ce processus.
pub extern "win64" fn get_process_version(pid: u32) -> u32 {
    if pid != 0 && pid != std::process::id() {
        return 0x000A_0000;
    }
    let Some(m) = crate::loader::main_module() else {
        return 0x000A_0000;
    };
    // SAFETY : en-têtes de l'image principale, chargée par le chargeur.
    unsafe {
        let base = m.base as *const u8;
        let lfanew = (base.add(0x3c) as *const u32).read_unaligned() as usize;
        let opt = base.add(lfanew + 24);
        let major = (opt.add(48) as *const u16).read_unaligned() as u32;
        let minor = (opt.add(50) as *const u16).read_unaligned() as u32;
        major << 16 | minor
    }
}

/// Plage de l'ensemble de travail : retenue telle que le programme la
/// règle (Linux n'a pas de borne équivalente par processus).
static WORKING_SET: Mutex<(usize, usize)> = Mutex::new((204_800, 1_413_120));

pub unsafe extern "win64" fn get_process_working_set_size(
    _process: usize,
    min: *mut usize,
    max: *mut usize,
) -> i32 {
    if min.is_null() || max.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (a, b) = *lock(&WORKING_SET);
    *min = a;
    *max = b;
    1
}

pub extern "win64" fn set_process_working_set_size(_process: usize, min: usize, max: usize) -> i32 {
    // (-1, -1) vide l'ensemble de travail sans changer la plage.
    if min == usize::MAX && max == usize::MAX {
        return 1;
    }
    if min > max {
        return fail(ERROR_INVALID_PARAMETER);
    }
    *lock(&WORKING_SET) = (min, max);
    1
}

pub unsafe extern "win64" fn get_system_file_cache_size(
    min: *mut usize,
    max: *mut usize,
    flags: *mut u32,
) -> i32 {
    if min.is_null() || max.is_null() || flags.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let pages = libc::sysconf(libc::_SC_PHYS_PAGES).max(0) as usize;
    let page = libc::sysconf(libc::_SC_PAGESIZE).max(4096) as usize;
    *min = 1 << 20;
    *max = pages * page;
    *flags = 0;
    1
}

/// Réglages réservés aux administrateurs (cache système, fuseau horaire).
pub extern "win64" fn privilege_not_held() -> i32 {
    fail(ERROR_PRIVILEGE_NOT_HELD)
}

/// SetSystemPowerState(veille, forcer) : la même mise en veille que
/// SetSuspendState de powrprof (vrai = veille, faux = veille prolongée).
pub unsafe extern "win64" fn set_system_power_state(suspend: i32, force: i32) -> i32 {
    api!(
        "powrprof",
        "SetSuspendState",
        fn(u8, u8, u8) -> u8,
        (suspend == 0) as u8,
        (force != 0) as u8,
        0
    )
    .unwrap_or(0) as i32
}

/// Débogage d'un autre processus : Winlux n'a pas de débogueur Win32.
pub extern "win64" fn debug_not_supported() -> i32 {
    fail(ERROR_NOT_SUPPORTED)
}

/// Volumes et points de montage : réservés aux administrateurs.
pub extern "win64" fn access_denied() -> i32 {
    fail(ERROR_ACCESS_DENIED)
}

pub extern "win64" fn not_supported() -> i32 {
    fail(ERROR_NOT_SUPPORTED)
}

/// Fonctions qui rendent un handle : INVALID_HANDLE_VALUE et
/// ERROR_NOT_SUPPORTED (boîtes aux lettres).
pub extern "win64" fn not_supported_handle() -> usize {
    set_last_error(ERROR_NOT_SUPPORTED);
    INVALID_HANDLE_VALUE
}

/// Get/SetMailslotInfo : aucun handle de boîte aux lettres n'existe.
pub extern "win64" fn invalid_handle() -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

// ---------------------------------------------------------------------
// Tubes nommés
// ---------------------------------------------------------------------

/// CallNamedPipeW : ouvre le tube, passe en mode message, transaction,
/// ferme.
pub unsafe extern "win64" fn call_named_pipe_w(
    name: *const u16,
    input: *const u8,
    in_len: u32,
    output: *mut u8,
    out_len: u32,
    read: *mut u32,
    timeout: u32,
) -> i32 {
    if name.is_null() || read.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let open = || {
        api!(
            "kernel32",
            "CreateFileW",
            fn(*const u16, u32, u32, usize, u32, u32, usize) -> usize,
            name,
            0xC000_0000,
            0,
            0,
            3,
            0x80,
            0
        )
        .unwrap_or(INVALID_HANDLE_VALUE)
    };
    let mut h = open();
    if h == INVALID_HANDLE_VALUE {
        // NMPWAIT_WAIT_FOREVER = 0xFFFFFFFF, NMPWAIT_USE_DEFAULT_WAIT = 0.
        let wait = if timeout == 0 { 50 } else { timeout };
        if crate::dlls::named_pipes::wait_named_pipe_w(name, wait) == 0 {
            return 0;
        }
        h = open();
        if h == INVALID_HANDLE_VALUE {
            return 0;
        }
    }
    let mut mode = 2u32; // PIPE_READMODE_MESSAGE
    let _ = api!(
        "kernel32",
        "SetNamedPipeHandleState",
        fn(usize, *mut u32, usize, usize) -> i32,
        h,
        &mut mode,
        0,
        0
    );
    let ok = api!(
        "kernel32",
        "TransactNamedPipe",
        fn(usize, *const u8, u32, *mut u8, u32, *mut u32, usize) -> i32,
        h,
        input,
        in_len,
        output,
        out_len,
        read,
        0
    )
    .unwrap_or(0);
    let err = crate::process::last_error();
    crate::dlls::kernel32::close_handle(h);
    if ok == 0 {
        set_last_error(err);
    }
    ok
}

pub unsafe extern "win64" fn get_named_pipe_info(
    h: usize,
    flags: *mut u32,
    out_size: *mut u32,
    in_size: *mut u32,
    max_instances: *mut u32,
) -> i32 {
    let server = crate::dlls::named_pipes::export(h).is_some();
    // FILE_TYPE_PIPE = 3.
    if !server && api!("kernel32", "GetFileType", fn(usize) -> u32, h).unwrap_or(0) != 3 {
        return fail(ERROR_INVALID_HANDLE);
    }
    for (p, v) in [
        (flags, server as u32), // PIPE_SERVER_END = 1, octets
        (out_size, 4096),
        (in_size, 4096),
        (max_instances, 255), // PIPE_UNLIMITED_INSTANCES
    ] {
        if !p.is_null() {
            *p = v;
        }
    }
    1
}

/// Session du client ou du serveur d'un tube : même machine, même
/// session que ce processus.
pub unsafe extern "win64" fn get_named_pipe_session_id(_h: usize, id: *mut u32) -> i32 {
    crate::dlls::kernel32_node::ProcessIdToSessionId(std::process::id(), id)
}

// ---------------------------------------------------------------------
// Divers
// ---------------------------------------------------------------------

pub unsafe extern "win64" fn get_computer_name_ex_a(
    kind: u32,
    buf: *mut u8,
    size: *mut u32,
) -> i32 {
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut w = vec![0u16; 256];
    let mut n = w.len() as u32;
    if crate::dlls::kernel32_node::GetComputerNameExW(kind, w.as_mut_ptr(), &mut n) == 0 {
        return 0;
    }
    let a = crate::dlls::encode_ansi(&String::from_utf16_lossy(&w[..n as usize]));
    if buf.is_null() || (*size as usize) <= a.len() {
        *size = a.len() as u32 + 1;
        return fail(ERROR_MORE_DATA);
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, a.len());
    *buf.add(a.len()) = 0;
    *size = a.len() as u32;
    1
}

pub unsafe extern "win64" fn get_geo_info_a(
    geo: u32,
    kind: u32,
    buf: *mut u8,
    len: i32,
    lang: u16,
) -> i32 {
    let mut w = vec![0u16; 512];
    let n = api!(
        "kernel32",
        "GetGeoInfoW",
        fn(u32, u32, *mut u16, i32, u16) -> i32,
        geo,
        kind,
        w.as_mut_ptr(),
        w.len() as i32,
        lang
    )
    .unwrap_or(0);
    if n <= 0 {
        return 0;
    }
    let text = String::from_utf16_lossy(&w[..(n as usize).saturating_sub(1)]);
    let mut a = crate::dlls::encode_ansi(&text);
    a.push(0);
    if len == 0 {
        return a.len() as i32;
    }
    if buf.is_null() || (len as usize) < a.len() {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, a.len());
    a.len() as i32
}

pub unsafe extern "win64" fn get_currency_format_w(
    lcid: u32,
    flags: u32,
    value: *const u16,
    fmt: *const c_void,
    buf: *mut u16,
    len: i32,
) -> i32 {
    let mut name = [0u16; 85];
    let n = api!(
        "kernel32",
        "LCIDToLocaleName",
        fn(u32, *mut u16, i32, u32) -> i32,
        lcid,
        name.as_mut_ptr(),
        name.len() as i32,
        0
    )
    .unwrap_or(0);
    if n == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    api!(
        "kernel32",
        "GetCurrencyFormatEx",
        fn(*const u16, u32, *const u16, *const c_void, *mut u16, i32) -> i32,
        name.as_ptr(),
        flags,
        value,
        fmt,
        buf,
        len
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_dll_directory_w(len: u32, buf: *mut u16) -> u32 {
    let dir = crate::loader::dll_directory().unwrap_or_default();
    let w: Vec<u16> = dir.encode_utf16().collect();
    put_w(buf, len, &w)
}

pub unsafe extern "win64" fn get_system_windows_directory_w(buf: *mut u16, len: u32) -> u32 {
    api!(
        "kernel32",
        "GetWindowsDirectoryW",
        fn(*mut u16, u32) -> u32,
        buf,
        len
    )
    .unwrap_or(0)
}

/// PulseEvent : libère les attentes en cours puis remet l'événement à
/// zéro (fonction dite peu fiable par la documentation, gardée simple).
pub unsafe extern "win64" fn pulse_event(h: usize) -> i32 {
    if api!("kernel32", "SetEvent", fn(usize) -> i32, h).unwrap_or(0) == 0 {
        return 0;
    }
    std::thread::yield_now();
    api!("kernel32", "ResetEvent", fn(usize) -> i32, h).unwrap_or(0)
}

/// QueryUnbiasedInterruptTimePrecise (KernelBase) : même horloge que
/// QueryUnbiasedInterruptTime, déjà précise à 100 ns près.
pub unsafe extern "win64" fn query_unbiased_interrupt_time_precise(t: *mut u64) {
    let _ = api!(
        "kernel32",
        "QueryUnbiasedInterruptTime",
        fn(*mut u64) -> i32,
        t
    );
}

/// CompareObjectHandles (KernelBase) : deux handles du même objet.
pub extern "win64" fn compare_object_handles(a: usize, b: usize) -> i32 {
    let c = crate::dlls::kernel32::canon_std;
    (c(a) == c(b)) as i32
}

pub extern "win64" fn set_file_apis_to_ansi() {}

pub fn exports() -> Exports {
    crate::exports! {
        "SearchPathW" => fn search_path_w,
        "SetSearchPathMode" => fn set_search_path_mode,
        "WinExec" => fn win_exec,
        "OpenFile" => fn open_file,
        "CreateFile2" => fn create_file_2,
        "GetBinaryTypeW" => fn get_binary_type_w,
        "CopyFileTransactedW" => fn copy_file_transacted_w,
        "CreateDirectoryTransactedW" => fn create_directory_transacted_w,
        "CreateFileTransactedW" => fn create_file_transacted_w,
        "CreateHardLinkTransactedW" => fn create_hard_link_transacted_w,
        "CreateSymbolicLinkTransactedW" => fn create_symbolic_link_transacted_w,
        "FindFirstFileTransactedW" => fn find_first_file_transacted_w,
        "FindFirstStreamTransactedW" => fn find_first_stream_transacted_w,
        "FindFirstFileNameTransactedW" => fn find_first_file_name_transacted_w,
        "GetFileAttributesTransactedA" => fn get_file_attributes_transacted_a,
        "GetFileAttributesTransactedW" => fn get_file_attributes_transacted_w,
        "GetFullPathNameTransactedA" => fn get_full_path_name_transacted_a,
        "GetFullPathNameTransactedW" => fn get_full_path_name_transacted_w,
        "GetLongPathNameTransactedW" => fn get_long_path_name_transacted_w,
        "RemoveDirectoryTransactedW" => fn remove_directory_transacted_w,
        "SetFileAttributesTransactedW" => fn set_file_attributes_transacted_w,
        "FindFirstFileNameW" => fn find_first_file_name_w,
        "FindNextFileNameW" => fn find_next_file_name_w,
        "SetFileShortNameW" => fn not_supported,
        "BackupRead" => fn backup_read,
        "BackupWrite" => fn backup_write,
        "BackupSeek" => fn backup_seek,
        "DefineDosDeviceW" => fn not_supported,
        "DeleteVolumeMountPointW" => fn access_denied,
        "SetVolumeMountPointW" => fn access_denied,
        "SetVolumeLabelW" => fn access_denied,
        "CreateConsoleScreenBuffer" => fn create_console_screen_buffer,
        "SetConsoleActiveScreenBuffer" => fn set_console_active_screen_buffer,
        "SetConsoleWindowInfo" => fn set_console_window_info,
        "WriteConsoleOutputW" => fn write_console_output_w,
        "WriteConsoleOutputCharacterW" => fn write_console_output_character_w,
        "WriteConsoleOutputAttribute" => fn write_console_output_attribute,
        "ReadConsoleOutputAttribute" => fn read_console_output_unsupported,
        "ReadConsoleOutputCharacterW" => fn read_console_output_unsupported,
        "ScrollConsoleScreenBufferW" => fn scroll_console_screen_buffer_w,
        "GetConsoleDisplayMode" => fn get_console_display_mode,
        "SetConsoleDisplayMode" => fn set_console_display_mode,
        "GetConsoleFontSize" => fn get_console_font_size,
        "GetCurrentConsoleFont" => fn get_current_console_font,
        "GetConsoleProcessList" => fn get_console_process_list,
        "GetConsoleSelectionInfo" => fn get_console_selection_info,
        "GetLargestConsoleWindowSize" => fn get_largest_console_window_size,
        "AddConsoleAliasW" => fn add_console_alias_w,
        "GetConsoleAliasesW" => fn get_console_aliases_w,
        "GetConsoleAliasesLengthW" => fn get_console_aliases_length_w,
        "GetConsoleAliasExesW" => fn get_console_alias_exes_w,
        "GetConsoleAliasExesLengthW" => fn get_console_alias_exes_length_w,
        "Process32First" => fn process32_first,
        "Process32Next" => fn process32_next,
        "VirtualAllocExNuma" => fn virtual_alloc_ex_numa,
        "AllocateUserPhysicalPages" => fn awe_privilege_not_held,
        "FreeUserPhysicalPages" => fn awe_privilege_not_held,
        "MapUserPhysicalPages" => fn awe_privilege_not_held,
        "GetNumaProcessorNodeEx" => fn get_numa_processor_node_ex,
        "SetThreadIdealProcessorEx" => fn set_thread_ideal_processor_ex,
        "GetThreadIdealProcessorEx" => fn get_thread_ideal_processor_ex,
        "GetProcessPriorityBoost" => fn get_process_priority_boost,
        "GetProcessVersion" => fn get_process_version,
        "GetProcessWorkingSetSize" => fn get_process_working_set_size,
        "SetProcessWorkingSetSize" => fn set_process_working_set_size,
        "GetSystemFileCacheSize" => fn get_system_file_cache_size,
        "SetSystemFileCacheSize" => fn privilege_not_held,
        "SetTimeZoneInformation" => fn privilege_not_held,
        "SetSystemPowerState" => fn set_system_power_state,
        "DebugActiveProcess" => fn debug_not_supported,
        "DebugActiveProcessStop" => fn debug_not_supported,
        "WaitForDebugEvent" => fn debug_not_supported,
        "ContinueDebugEvent" => fn debug_not_supported,
        "CreateMailslotW" => fn not_supported_handle,
        "GetMailslotInfo" => fn invalid_handle,
        "SetMailslotInfo" => fn invalid_handle,
        "CallNamedPipeW" => fn call_named_pipe_w,
        "GetNamedPipeInfo" => fn get_named_pipe_info,
        "GetNamedPipeClientSessionId" => fn get_named_pipe_session_id,
        "GetNamedPipeServerSessionId" => fn get_named_pipe_session_id,
        "GetComputerNameExA" => fn get_computer_name_ex_a,
        "GetGeoInfoA" => fn get_geo_info_a,
        "GetCurrencyFormatW" => fn get_currency_format_w,
        "GetDllDirectoryW" => fn get_dll_directory_w,
        "GetSystemWindowsDirectoryW" => fn get_system_windows_directory_w,
        "PulseEvent" => fn pulse_event,
        "OpenWaitableTimerW" => fn crate::dlls::kernel32::open_waitable_timer_w,
        "QueryUnbiasedInterruptTimePrecise" => fn query_unbiased_interrupt_time_precise,
        "CompareObjectHandles" => fn compare_object_handles,
        "SetFileApisToANSI" => fn set_file_apis_to_ansi,
        "BeginUpdateResourceA" => fn not_supported,
        "UpdateResourceA" => fn not_supported,
        "EndUpdateResourceA" => fn not_supported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_search_path_mode_valide_les_indicateurs() {
        // Documentation de SetSearchPathMode ; OBS Studio l'appelle au
        // démarrage (#2179).
        let _teb = crate::process::test_teb();
        unsafe {
            assert_eq!(set_search_path_mode(0), 0);
            assert_eq!(set_search_path_mode(0x1 | 0x1_0000), 0);
            assert_eq!(set_search_path_mode(0x1_0000 | 0x8000), 0);
            assert_eq!(set_search_path_mode(0x2), 0);
            assert_eq!(set_search_path_mode(0x1_0000), 1);
            assert!(safe_search_disabled());
            assert_eq!(set_search_path_mode(0x1 | 0x8000), 1);
            assert!(!safe_search_disabled());
            // Permanent : plus aucun changement.
            assert_eq!(set_search_path_mode(0x1_0000), 0);
            assert_eq!(crate::process::last_error(), ERROR_ACCESS_DENIED);
            *SEARCH_MODE.lock().unwrap() = (None, false);
        }
    }

    #[test]
    fn searchpath_ajoute_l_extension_et_parcourt_les_dossiers() {
        let dirs = vec!["C:\\a".to_string(), "D:\\b\\".to_string()];
        assert_eq!(
            search_candidates("python", Some(".exe"), &dirs),
            vec!["C:\\a\\python.exe", "D:\\b\\python.exe"]
        );
        // Extension déjà là, ou chemin donné : tel quel.
        assert_eq!(
            search_candidates("x.dll", Some(".exe"), &dirs)[0],
            "C:\\a\\x.dll"
        );
        assert_eq!(
            search_candidates("C:\\y\\z", Some(".txt"), &dirs),
            vec!["C:\\y\\z.txt"]
        );
    }

    #[test]
    fn type_binaire_d_apres_l_en_tete() {
        let mut pe = vec![0u8; 0x100];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        assert_eq!(binary_type(&pe, "a.exe"), Some(6));
        pe[0x84..0x86].copy_from_slice(&0x014cu16.to_le_bytes());
        assert_eq!(binary_type(&pe, "a.exe"), Some(0));
        pe[0x80..0x82].copy_from_slice(b"NE");
        assert_eq!(binary_type(&pe, "a.exe"), Some(2));
        assert_eq!(binary_type(b"\x7fELF", "a.com"), Some(1));
        assert_eq!(binary_type(b"\x7fELF", "a.so"), None);
    }

    #[test]
    fn nom_depuis_la_racine_du_volume() {
        let w: Vec<u16> = "C:\\Users\\a.txt".encode_utf16().collect();
        assert_eq!(
            String::from_utf16_lossy(&volume_relative(&w)),
            "\\Users\\a.txt"
        );
    }

    #[test]
    fn couleurs_de_console_en_sgr() {
        // FOREGROUND_RED | FOREGROUND_INTENSITY sur BACKGROUND_BLUE.
        assert_eq!(sgr(0x04 | 0x08 | 0x10), "\x1b[91;44m");
        assert_eq!(sgr(0x07), "\x1b[37;40m");
    }

    #[test]
    fn sauvegarde_puis_restauration_du_flux_de_donnees() {
        let dir = std::env::temp_dir().join(format!("winlux-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.bin");
        std::fs::write(&src, b"contenu de test").unwrap();
        let w = |p: &std::path::Path| wide(&crate::paths::to_windows(p));
        unsafe {
            let open = |p: &[u16], access: u32, disp: u32| {
                api!(
                    "kernel32",
                    "CreateFileW",
                    fn(*const u16, u32, u32, usize, u32, u32, usize) -> usize,
                    p.as_ptr(),
                    access,
                    3,
                    0,
                    disp,
                    0x80,
                    0
                )
                .unwrap()
            };
            let h = open(&w(&src), 0x8000_0000, 3);
            assert_ne!(h, INVALID_HANDLE_VALUE);
            let mut ctx = 0usize;
            let mut all = Vec::new();
            loop {
                let mut buf = [0u8; 7];
                let mut n = 0u32;
                assert_eq!(
                    backup_read(h, buf.as_mut_ptr(), 7, &mut n, 0, 0, &mut ctx),
                    1
                );
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&buf[..n as usize]);
            }
            backup_read(h, std::ptr::null_mut(), 0, &mut 0, 1, 0, &mut ctx);
            crate::dlls::kernel32::close_handle(h);
            assert_eq!(&all[..STREAM_HEADER], &data_stream_header(15)[..]);
            assert_eq!(&all[STREAM_HEADER..], b"contenu de test");

            let dst = dir.join("dst.bin");
            let h = open(&w(&dst), 0x4000_0000, 2);
            let mut ctx = 0usize;
            for chunk in all.chunks(6) {
                let mut n = 0u32;
                assert_eq!(
                    backup_write(
                        h,
                        chunk.as_ptr(),
                        chunk.len() as u32,
                        &mut n,
                        0,
                        0,
                        &mut ctx
                    ),
                    1
                );
            }
            backup_write(h, std::ptr::null(), 0, &mut 0, 1, 0, &mut ctx);
            crate::dlls::kernel32::close_handle(h);
            assert_eq!(std::fs::read(&dst).unwrap(), b"contenu de test");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn alias_de_console_par_programme() {
        let p = |s: &str| wide(s);
        unsafe {
            assert_eq!(
                add_console_alias_w(
                    p("ll").as_ptr(),
                    p("dir /w").as_ptr(),
                    p("cmd.exe").as_ptr()
                ),
                1
            );
            let n = get_console_aliases_length_w(p("CMD.EXE").as_ptr());
            assert_eq!(n as usize, ("ll=dir /w".len() + 1) * 2);
            let mut buf = vec![0u16; 64];
            assert_eq!(
                get_console_aliases_w(buf.as_mut_ptr(), 128, p("cmd.exe").as_ptr()),
                n
            );
            assert_eq!(String::from_utf16_lossy(&buf[..9]), "ll=dir /w");
            add_console_alias_w(p("ll").as_ptr(), std::ptr::null(), p("cmd.exe").as_ptr());
            assert_eq!(get_console_aliases_length_w(p("cmd.exe").as_ptr()), 0);
        }
    }
}
