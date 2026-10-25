//! Fichiers mappés en mémoire (CreateFileMapping / MapViewOfFile).
//!
//! Cœur de Notepad++ pour l'édition de gros fichiers. On s'appuie sur
//! mmap(2) de Linux pour le mappage réel.

use std::collections::HashMap;
use std::sync::Mutex;

/// Un mapping créé par CreateFileMappingA/W.
struct FileMapping {
    fd: i32,
    /// -1 = mapping anonyme (pagefile-backed).
    prot: i32,
    max_size: u64,
    /// Descripteur dupliqué qu'on ferme nous-mêmes.
    owned_fd: bool,
}

/// Une vue créée par MapViewOfFile.
struct MappedView {
    addr: usize,
    len: usize,
}

static MAPPINGS: Mutex<Option<HashMap<usize, FileMapping>>> = Mutex::new(None);
static VIEWS: Mutex<Option<HashMap<usize, MappedView>>> = Mutex::new(None);

fn mappings() -> std::sync::MutexGuard<'static, Option<HashMap<usize, FileMapping>>> {
    let mut g = MAPPINGS.lock().unwrap();
    if g.is_none() { *g = Some(HashMap::new()); }
    g
}

fn views() -> std::sync::MutexGuard<'static, Option<HashMap<usize, MappedView>>> {
    let mut g = VIEWS.lock().unwrap();
    if g.is_none() { *g = Some(HashMap::new()); }
    g
}

fn next_handle() -> usize {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0xF000_0000);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

const PAGE_READONLY: u32 = 0x02;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_WRITECOPY: u32 = 0x08;

const FILE_MAP_WRITE: u32 = 0x0002;
const FILE_MAP_READ: u32 = 0x0004;
const FILE_MAP_COPY: u32 = 0x0001;

fn win_handle_to_fd(handle: usize) -> i32 {
    crate::dlls::kernel32::handle_to_fd(handle)
}

/// CreateFileMappingA
pub unsafe extern "win64" fn create_file_mapping_a(
    file: usize,
    _security: usize,
    protect: u32,
    max_high: u32,
    max_low: u32,
    _name: *const u8,
) -> usize {
    let max_size = ((max_high as u64) << 32) | max_low as u64;
    let fd = if file == usize::MAX || file == 0 { -1 } else { win_handle_to_fd(file) };

    // Vérifier la taille du fichier si pas de taille max spécifiée.
    let actual_max = if max_size == 0 && fd >= 0 {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut st) == 0 { st.st_size as u64 } else { 0 }
    } else {
        max_size
    };

    if actual_max == 0 { crate::process::set_last_error(87); return 0; }

    let prot = match protect & 0xFF {
        PAGE_READONLY => libc::PROT_READ,
        PAGE_READWRITE => libc::PROT_READ | libc::PROT_WRITE,
        PAGE_WRITECOPY => libc::PROT_READ | libc::PROT_WRITE,
        _ => libc::PROT_READ,
    };

    let dup_fd = if fd >= 0 { libc::dup(fd) } else { -1 };

    let handle = next_handle();
    mappings().as_mut().unwrap().insert(handle, FileMapping {
        fd: if dup_fd >= 0 { dup_fd } else { fd },
        prot,
        max_size: actual_max,
        owned_fd: dup_fd >= 0,
    });
    handle
}

pub unsafe extern "win64" fn create_file_mapping_w(
    file: usize,
    security: usize,
    protect: u32,
    max_high: u32,
    max_low: u32,
    _name: *const u16,
) -> usize {
    create_file_mapping_a(file, security, protect, max_high, max_low, std::ptr::null())
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
        None => { crate::process::set_last_error(6); return 0; }
    };

    let offset = ((offset_high as u64) << 32) | offset_low as u64;
    let len = if bytes_to_map == 0 {
        (fm.max_size - offset) as usize
    } else {
        bytes_to_map
    };

    let prot = if access & FILE_MAP_WRITE != 0 {
        libc::PROT_READ | libc::PROT_WRITE
    } else if access & FILE_MAP_COPY != 0 {
        libc::PROT_READ | libc::PROT_WRITE
    } else {
        libc::PROT_READ
    };

    let flags = if fm.fd < 0 {
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS
    } else if access & FILE_MAP_COPY != 0 {
        libc::MAP_PRIVATE
    } else if access & FILE_MAP_WRITE != 0 {
        libc::MAP_SHARED
    } else {
        libc::MAP_PRIVATE
    };

    let p = libc::mmap(
        std::ptr::null_mut(),
        len,
        prot,
        flags,
        fm.fd,
        offset as libc::off_t,
    );

    if p == libc::MAP_FAILED { crate::process::set_last_error(8); return 0; }

    let addr = p as usize;
    drop(guard);
    views().as_mut().unwrap().insert(addr, MappedView { addr, len });
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
        None => { crate::process::set_last_error(6); 0 }
    }
}

/// FlushViewOfFile
pub unsafe extern "win64" fn flush_view_of_file(addr: usize, size: usize) -> i32 {
    let guard = views();
    let view = guard.as_ref().and_then(|m| m.get(&addr));
    let len = match view {
        Some(v) => if size == 0 { v.len } else { size },
        None => { crate::process::set_last_error(6); return 0; }
    };
    libc::msync(addr as *mut _, len, libc::MS_SYNC);
    1
}

/// CloseHandle pour les mappings de fichiers.
pub fn close_mapping(handle: usize) -> bool {
    let fm = mappings().as_mut().and_then(|m| m.remove(&handle));
    if let Some(fm) = fm {
        if fm.owned_fd && fm.fd >= 0 { unsafe { libc::close(fm.fd); } }
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
    }
}
