//! Fichiers mappés en mémoire (CreateFileMapping / MapViewOfFile).
//!
//! Cœur de Notepad++ pour l'édition de gros fichiers. On s'appuie sur
//! mmap(2) de Linux pour le mappage réel.

use std::collections::HashMap;
use std::sync::Mutex;

/// Un mapping créé par CreateFileMappingA/W.
struct FileMapping {
    /// Fichier mappé, ou memfd d'un mapping anonyme (adossé au « fichier
    /// d'échange » sous Windows) : toutes les vues d'un même mapping, et
    /// celles des processus qui en ont reçu une copie, partagent ainsi les
    /// mêmes pages.
    fd: i32,
    max_size: u64,
    /// Descripteur dupliqué qu'on ferme nous-mêmes.
    owned_fd: bool,
    /// Nom (Local\…, Global\… ou nu), pour OpenFileMapping.
    name: Option<String>,
}

/// Une vue créée par MapViewOfFile.
struct MappedView {
    addr: usize,
    len: usize,
}

static MAPPINGS: Mutex<Option<HashMap<usize, FileMapping>>> = Mutex::new(None);
static VIEWS: Mutex<Option<HashMap<usize, MappedView>>> = Mutex::new(None);

fn mappings() -> std::sync::MutexGuard<'static, Option<HashMap<usize, FileMapping>>> {
    let mut g = MAPPINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    g
}

fn views() -> std::sync::MutexGuard<'static, Option<HashMap<usize, MappedView>>> {
    let mut g = VIEWS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    g
}

/// Les handles de mapping viennent du même compteur que les autres handles
/// de kernel32 : un handle hérité ne peut ainsi en écraser aucun.
fn next_handle() -> usize {
    crate::dlls::kernel32::new_handle_number()
}

const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;

const FILE_MAP_WRITE: u32 = 0x0002;
const _FILE_MAP_READ: u32 = 0x0004;
const FILE_MAP_COPY: u32 = 0x0001;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_ALREADY_EXISTS: u32 = 183;

fn win_handle_to_fd(handle: usize) -> i32 {
    crate::dlls::kernel32::handle_to_fd(handle)
}

/// Clé d'un nom de mapping : sans préfixe d'espace de noms, sans casse.
fn name_key(name: &str) -> String {
    let lower = name.to_lowercase();
    for prefix in ["local\\", "global\\", "session\\"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    lower
}

fn insert_mapping(fd: i32, max_size: u64, owned_fd: bool, name: Option<String>) -> usize {
    let handle = next_handle();
    mappings().as_mut().unwrap().insert(
        handle,
        FileMapping {
            fd,
            max_size,
            owned_fd,
            name,
        },
    );
    handle
}

/// Nouveau handle vers un mapping nommé existant de ce processus.
fn open_named(key: &str) -> Option<usize> {
    let (fd, size) = {
        let g = mappings();
        let fm = g
            .as_ref()?
            .values()
            .find(|m| m.name.as_deref() == Some(key))?;
        (fm.fd, fm.max_size)
    };
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    (copy >= 0).then(|| insert_mapping(copy, size, true, Some(key.to_string())))
}

/// Mapping anonyme : memfd de la taille demandée (les pages ne sont
/// réservées qu'au premier accès, comme SEC_COMMIT sous Windows).
fn anonymous_fd(size: u64) -> Option<i32> {
    let fd = unsafe { libc::memfd_create(c"winlux-section".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    if unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
        unsafe { libc::close(fd) };
        return None;
    }
    Some(fd)
}

unsafe fn create_mapping(
    file: usize,
    security: usize,
    protect: u32,
    max_size: u64,
    name: Option<String>,
) -> usize {
    let key = name.as_deref().filter(|n| !n.is_empty()).map(name_key);
    if let Some(k) = key.as_deref() {
        if let Some(h) = open_named(k) {
            crate::dlls::inherit::from_security_attributes(h, security);
            crate::process::set_last_error(ERROR_ALREADY_EXISTS);
            return h;
        }
    }
    let anonymous = file == usize::MAX || file == 0;
    let (fd, size) = if anonymous {
        if max_size == 0 {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        let Some(fd) = anonymous_fd(max_size) else {
            crate::process::set_last_error(ERROR_NOT_ENOUGH_MEMORY);
            return 0;
        };
        (fd, max_size)
    } else {
        let src = win_handle_to_fd(file);
        if src < 0 {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            return 0;
        }
        let mut st: libc::stat = std::mem::zeroed();
        let current = if libc::fstat(src, &mut st) == 0 {
            st.st_size as u64
        } else {
            0
        };
        let size = if max_size == 0 { current } else { max_size };
        if size == 0 {
            // Fichier vide sans taille : ERROR_FILE_INVALID sous Windows.
            crate::process::set_last_error(1006);
            return 0;
        }
        // Mapping inscriptible plus grand que le fichier : Windows l'agrandit.
        let writable = matches!(protect & 0xFF, PAGE_READWRITE | PAGE_EXECUTE_READWRITE);
        if writable && size > current {
            libc::ftruncate(src, size as libc::off_t);
        }
        let dup_fd = libc::fcntl(src, libc::F_DUPFD_CLOEXEC, 3);
        if dup_fd < 0 {
            crate::process::set_last_error(ERROR_NOT_ENOUGH_MEMORY);
            return 0;
        }
        (dup_fd, size)
    };
    let handle = insert_mapping(fd, size, true, key);
    crate::dlls::inherit::from_security_attributes(handle, security);
    crate::process::set_last_error(0);
    handle
}

/// CreateFileMappingA
pub unsafe extern "win64" fn create_file_mapping_a(
    file: usize,
    security: usize,
    protect: u32,
    max_high: u32,
    max_low: u32,
    name: *const u8,
) -> usize {
    let max_size = ((max_high as u64) << 32) | max_low as u64;
    let name = (!name.is_null()).then(|| crate::dlls::cstr(name));
    create_mapping(file, security, protect, max_size, name)
}

pub unsafe extern "win64" fn create_file_mapping_w(
    file: usize,
    security: usize,
    protect: u32,
    max_high: u32,
    max_low: u32,
    name: *const u16,
) -> usize {
    let max_size = ((max_high as u64) << 32) | max_low as u64;
    let name = (!name.is_null()).then(|| crate::dlls::wstr(name));
    create_mapping(file, security, protect, max_size, name)
}

fn open_mapping(name: Option<String>, inherit: i32) -> usize {
    let Some(key) = name.as_deref().filter(|n| !n.is_empty()).map(name_key) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    match open_named(&key) {
        Some(h) => {
            crate::dlls::inherit::set(h, inherit != 0);
            crate::process::set_last_error(0);
            h
        }
        None => {
            crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
            0
        }
    }
}

pub unsafe extern "win64" fn open_file_mapping_w(
    _access: u32,
    inherit: i32,
    name: *const u16,
) -> usize {
    open_mapping((!name.is_null()).then(|| crate::dlls::wstr(name)), inherit)
}

pub unsafe extern "win64" fn open_file_mapping_a(
    _access: u32,
    inherit: i32,
    name: *const u8,
) -> usize {
    open_mapping((!name.is_null()).then(|| crate::dlls::cstr(name)), inherit)
}

/// DuplicateHandle d'un mapping : nouveau handle, même mémoire.
pub fn duplicate_mapping(h: usize) -> Option<usize> {
    let (fd, size, name) = {
        let g = mappings();
        let fm = g.as_ref()?.get(&h)?;
        (fm.fd, fm.max_size, fm.name.clone())
    };
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    (copy >= 0).then(|| insert_mapping(copy, size, true, name))
}

/// Mapping transmissible à un enfant : (descripteur, taille).
pub fn export(h: usize) -> Option<(i32, u64)> {
    let g = mappings();
    let fm = g.as_ref()?.get(&h)?;
    Some((fm.fd, fm.max_size))
}

/// L'enfant reprend un mapping hérité, sous le même numéro de handle.
pub fn adopt(h: usize, fd: std::os::fd::OwnedFd, size: u64) {
    use std::os::fd::IntoRawFd;
    mappings().as_mut().unwrap().insert(
        h,
        FileMapping {
            fd: fd.into_raw_fd(),
            max_size: size,
            owned_fd: true,
            name: None,
        },
    );
}

/// MapViewOfFile
pub unsafe extern "win64" fn map_view_of_file(
    mapping: usize,
    access: u32,
    offset_high: u32,
    offset_low: u32,
    bytes_to_map: usize,
) -> usize {
    let guard = mappings();
    let fm = match guard.as_ref().and_then(|m| m.get(&mapping)) {
        Some(m) => m,
        None => {
            crate::process::set_last_error(6);
            return 0;
        }
    };

    let offset = ((offset_high as u64) << 32) | offset_low as u64;
    // Décalage au-delà de la taille du mapping : ERROR_ACCESS_DENIED, comme
    // Windows (et non un sous-dépassement de la longueur restante).
    let Some(remaining) = fm.max_size.checked_sub(offset) else {
        crate::process::set_last_error(5);
        return 0;
    };
    let len = if bytes_to_map == 0 {
        remaining as usize
    } else {
        bytes_to_map
    };
    if len == 0 || (fm.max_size != 0 && len as u64 > remaining) {
        crate::process::set_last_error(5);
        return 0;
    }

    let prot = if access & (FILE_MAP_WRITE | FILE_MAP_COPY) != 0 {
        libc::PROT_READ | libc::PROT_WRITE
    } else {
        libc::PROT_READ
    };

    // Vue partagée (les écritures des autres vues y apparaissent), sauf
    // copie sur écriture demandée.
    let flags = if access & FILE_MAP_COPY != 0 {
        libc::MAP_PRIVATE
    } else {
        libc::MAP_SHARED
    };

    let p = libc::mmap(
        std::ptr::null_mut(),
        len,
        prot,
        flags,
        fm.fd,
        offset as libc::off_t,
    );

    if p == libc::MAP_FAILED {
        crate::process::set_last_error(8);
        return 0;
    }

    let addr = p as usize;
    drop(guard);
    views()
        .as_mut()
        .unwrap()
        .insert(addr, MappedView { addr, len });
    addr
}

/// MapViewOfFileEx (avec adresse suggérée, qu'on ignore).
pub unsafe extern "win64" fn map_view_of_file_ex(
    mapping: usize,
    access: u32,
    offset_high: u32,
    offset_low: u32,
    bytes_to_map: usize,
    _base: usize,
) -> usize {
    map_view_of_file(mapping, access, offset_high, offset_low, bytes_to_map)
}

/// UnmapViewOfFile
pub unsafe extern "win64" fn unmap_view_of_file(addr: usize) -> i32 {
    let view = views().as_mut().and_then(|m| m.remove(&addr));
    match view {
        Some(v) => {
            libc::munmap(v.addr as *mut _, v.len);
            1
        }
        None => {
            crate::process::set_last_error(6);
            0
        }
    }
}

/// FlushViewOfFile
pub unsafe extern "win64" fn flush_view_of_file(addr: usize, size: usize) -> i32 {
    let guard = views();
    let view = guard.as_ref().and_then(|m| m.get(&addr));
    let len = match view {
        Some(v) => {
            if size == 0 {
                v.len
            } else {
                size
            }
        }
        None => {
            crate::process::set_last_error(6);
            return 0;
        }
    };
    libc::msync(addr as *mut _, len, libc::MS_SYNC);
    1
}

/// CloseHandle pour les mappings de fichiers.
pub fn close_mapping(handle: usize) -> bool {
    let fm = mappings().as_mut().and_then(|m| m.remove(&handle));
    if let Some(fm) = fm {
        if fm.owned_fd && fm.fd >= 0 {
            unsafe {
                libc::close(fm.fd);
            }
        }
        true
    } else {
        false
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateFileMappingA" => fn create_file_mapping_a,
        "CreateFileMappingW" => fn create_file_mapping_w,
        "MapViewOfFile" => fn map_view_of_file,
        "MapViewOfFileEx" => fn map_view_of_file_ex,
        "UnmapViewOfFile" => fn unmap_view_of_file,
        "FlushViewOfFile" => fn flush_view_of_file,
        "OpenFileMappingW" => fn open_file_mapping_w,
        "OpenFileMappingA" => fn open_file_mapping_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoire_partagee_anonyme() {
        let _teb = crate::process::test_teb();
        unsafe {
            // Comme base::UnsafeSharedMemoryRegion de Chromium : mapping
            // anonyme, copie par DuplicateHandle, puis deux vues.
            let h =
                create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 0x40000, std::ptr::null());
            assert_ne!(h, 0);
            let mut dup = 0usize;
            let me = crate::dlls::kernel32::get_current_process();
            assert_eq!(
                crate::dlls::kernel32::duplicate_handle(me, h, me, &mut dup, 0x6, 0, 0),
                1
            );
            assert_ne!(dup, 0);
            assert!(crate::dlls::kernel32::close_handle(h) != 0);
            let a = map_view_of_file(dup, FILE_MAP_WRITE, 0, 0, 0) as *mut u8;
            let b = map_view_of_file(dup, FILE_MAP_WRITE, 0, 0, 0x1000) as *mut u8;
            assert!(!a.is_null() && !b.is_null() && a != b);
            *a.add(10) = 0x5A;
            assert_eq!(*b.add(10), 0x5A, "les deux vues voient la même mémoire");
            // Le mapping fermé, les vues restent valides.
            assert!(crate::dlls::kernel32::close_handle(dup) != 0);
            assert_eq!(*a.add(10), 0x5A);
            assert_eq!(unmap_view_of_file(a as usize), 1);
            assert_eq!(unmap_view_of_file(b as usize), 1);
            // Taille nulle sans fichier : refusé.
            assert_eq!(
                create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 0, std::ptr::null()),
                0
            );
        }
    }

    #[test]
    fn mapping_nomme() {
        let _teb = crate::process::test_teb();
        unsafe {
            let name: Vec<u16> = format!("Local\\winlux-test-{}", std::process::id())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let h = create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 4096, name.as_ptr());
            assert_ne!(h, 0);
            let again =
                create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 4096, name.as_ptr());
            assert_eq!(
                crate::dlls::kernel32::get_last_error(),
                ERROR_ALREADY_EXISTS
            );
            let bare: Vec<u16> = format!("winlux-test-{}", std::process::id())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let opened = open_file_mapping_w(FILE_MAP_WRITE, 0, bare.as_ptr());
            assert_ne!(opened, 0);
            let a = map_view_of_file(h, FILE_MAP_WRITE, 0, 0, 0) as *mut u8;
            let b = map_view_of_file(opened, FILE_MAP_WRITE, 0, 0, 0) as *mut u8;
            *a = 7;
            assert_eq!(*b, 7);
            unmap_view_of_file(a as usize);
            unmap_view_of_file(b as usize);
            for x in [h, again, opened] {
                crate::dlls::kernel32::close_handle(x);
            }
            let missing: Vec<u16> = "winlux-absent".encode_utf16().chain(Some(0)).collect();
            assert_eq!(open_file_mapping_w(4, 0, missing.as_ptr()), 0);
            assert_eq!(
                crate::dlls::kernel32::get_last_error(),
                ERROR_FILE_NOT_FOUND
            );
        }
    }
}
