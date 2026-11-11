//! Fichiers mappés en mémoire (CreateFileMapping / MapViewOfFile).
//!
//! Cœur de Notepad++ pour l'édition de gros fichiers. On s'appuie sur
//! mmap(2) de Linux pour le mappage réel.

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
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
    /// Droits du handle (SECTION_*) : un handle en lecture seule ne donne
    /// ni vue inscriptible ni copie inscriptible (Chromium le vérifie).
    access: u32,
}

/// Une vue créée par MapViewOfFile.
struct MappedView {
    addr: usize,
    len: usize,
    /// Protection Windows (PAGE_*) de chaque page de 4 Kio, pour
    /// VirtualProtect et VirtualAlloc(MEM_COMMIT) sur la vue (#665).
    protection: Vec<u32>,
    /// Nom de la section, pour la trace de VirtualQuery (#1650).
    name: Option<String>,
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

/// Droits d'un handle de section.
pub const SECTION_QUERY: u32 = 0x0001;
pub const SECTION_MAP_WRITE: u32 = 0x0002;
pub const SECTION_MAP_READ: u32 = 0x0004;
pub const SECTION_ALL_ACCESS: u32 = 0x000F_001F;
const DUPLICATE_SAME_ACCESS: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;

/// Droits génériques traduits pour une section (GENERIC_MAPPING de Windows).
pub fn section_rights(access: u32) -> u32 {
    crate::dlls::electron_win32::map_generic_mask(
        access,
        [
            0x0002_0000 | SECTION_QUERY | SECTION_MAP_READ,
            0x0002_0000 | SECTION_MAP_WRITE,
            0x0002_0000 | 0x0008,
            SECTION_ALL_ACCESS,
        ],
    )
}

/// Les droits demandés sont-ils tous contenus dans ceux du handle ?
/// (SYNCHRONIZE et ACCESS_SYSTEM_SECURITY mis à part.)
pub fn rights_allowed(have: u32, want: u32) -> bool {
    let want = section_rights(want) & !0x0010_0000 & !0x0100_0000;
    want & !have == 0
}

const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;

const FILE_MAP_WRITE: u32 = 0x0002;
const _FILE_MAP_READ: u32 = 0x0004;
const FILE_MAP_COPY: u32 = 0x0001;
const FILE_MAP_EXECUTE: u32 = 0x0020;
const PAGE: usize = 4096;
const ERROR_INVALID_ADDRESS: u32 = 487;

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
    crate::dlls::named_objects::name_key(name)
}

fn insert_mapping(
    fd: i32,
    max_size: u64,
    owned_fd: bool,
    name: Option<String>,
    access: u32,
) -> usize {
    let handle = next_handle();
    if let Some(k) = name.as_deref() {
        crate::dlls::named_objects::retain(crate::dlls::named_objects::SECTION, k);
    }
    mappings().as_mut().unwrap().insert(
        handle,
        FileMapping {
            fd,
            max_size,
            owned_fd,
            name,
            access,
        },
    );
    handle
}

/// Mapping nommé créé par un autre processus du même compte (#20) : la
/// mémoire elle-même (section anonyme) ou le fichier qu'il mappe.
fn open_shared(key: &str, access: u32) -> Option<usize> {
    use crate::dlls::named_objects as named;
    use std::os::fd::IntoRawFd;
    let o = named::open(named::SECTION, key, None)?;
    let target = match std::fs::read_to_string(named::sidecar(&o.path)) {
        Ok(text) => text.split_once('\n').and_then(|(size, path)| {
            let size = size.parse::<u64>().ok()?;
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .or_else(|_| std::fs::File::open(path))
                .ok()?;
            Some((f.into_raw_fd(), size))
        }),
        Err(_) => {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            (unsafe { libc::fstat(o.fd.as_raw_fd(), &mut st) } == 0 && st.st_size > 0)
                .then(|| (o.fd.into_raw_fd(), st.st_size as u64))
        }
    };
    let Some((fd, size)) = target else {
        named::release(named::SECTION, key);
        return None;
    };
    Some(insert_mapping(
        fd,
        size,
        true,
        Some(key.to_string()),
        access,
    ))
}

/// Crée la section nommée `key` visible des autres processus du compte.
/// `Ok((handle, existait))`, `Err(code)` ; None : dossier des objets
/// indisponible, la section reste propre au processus.
fn create_shared(
    key: &str,
    file: usize,
    protect: u32,
    max_size: u64,
) -> Option<Result<(usize, bool), u32>> {
    use crate::dlls::named_objects as named;
    use std::io::Write;
    use std::os::fd::IntoRawFd;
    if let Some(h) = open_shared(key, SECTION_ALL_ACCESS) {
        return Some(Ok((h, true)));
    }
    named::dir()?;
    if file == usize::MAX || file == 0 {
        if max_size == 0 {
            return Some(Err(ERROR_INVALID_PARAMETER));
        }
        let mut init = |f: &std::fs::File, _: &std::path::Path| f.set_len(max_size).is_ok();
        let o = named::open(named::SECTION, key, Some(&mut init))?;
        let size = if o.created {
            max_size
        } else {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            unsafe { libc::fstat(o.fd.as_raw_fd(), &mut st) };
            st.st_size as u64
        };
        let fd = o.fd.into_raw_fd();
        let h = insert_mapping(fd, size, true, Some(key.to_string()), SECTION_ALL_ACCESS);
        return Some(Ok((h, !o.created)));
    }
    let (fd, size) = match unsafe { file_source(file, protect, max_size) } {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    // Section adossée à un fichier : les autres processus rouvrent le même
    // fichier (chemin et taille dans `<objet>.chemin`). Un fichier sans
    // chemin (supprimé, anonyme) reste une section de ce seul processus.
    let path = std::fs::read_link(format!("/proc/self/fd/{fd}"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| p.starts_with('/') && !p.ends_with(" (deleted)"));
    if let Some(path) = path {
        let mut write = |_: &std::fs::File, object: &std::path::Path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(named::sidecar(object))
                .and_then(|mut s| write!(s, "{size}\n{path}"))
                .is_ok()
        };
        let _ = named::open(named::SECTION, key, Some(&mut write));
    }
    let h = insert_mapping(fd, size, true, Some(key.to_string()), SECTION_ALL_ACCESS);
    Some(Ok((h, false)))
}

/// Nouveau handle vers un mapping nommé existant de ce processus.
fn open_named(key: &str, access: u32) -> Option<usize> {
    let (fd, size) = {
        let g = mappings();
        let fm = g
            .as_ref()?
            .values()
            .find(|m| m.name.as_deref() == Some(key))?;
        (fm.fd, fm.max_size)
    };
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    (copy >= 0).then(|| insert_mapping(copy, size, true, Some(key.to_string()), access))
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

/// Fichier source d'une section adossée à un fichier : (copie de son
/// descripteur, taille de la section).
unsafe fn file_source(file: usize, protect: u32, max_size: u64) -> Result<(i32, u64), u32> {
    let src = win_handle_to_fd(file);
    if src < 0 {
        return Err(ERROR_INVALID_HANDLE);
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
        return Err(1006);
    }
    // Mapping inscriptible plus grand que le fichier : Windows l'agrandit.
    let writable = matches!(protect & 0xFF, PAGE_READWRITE | PAGE_EXECUTE_READWRITE);
    if writable && size > current {
        libc::ftruncate(src, size as libc::off_t);
    }
    let dup_fd = libc::fcntl(src, libc::F_DUPFD_CLOEXEC, 3);
    if dup_fd < 0 {
        return Err(ERROR_NOT_ENOUGH_MEMORY);
    }
    Ok((dup_fd, size))
}

unsafe fn create_mapping(
    file: usize,
    security: usize,
    protect: u32,
    max_size: u64,
    name: Option<String>,
) -> usize {
    let shown = name.clone();
    let h = create_mapping_inner(file, security, protect, max_size, name);
    // WINLUX_TRACE : nom, taille demandée, handle, taille réelle et
    // ERROR_ALREADY_EXISTS, pour suivre une mémoire partagée nommée entre
    // processus (flux de Steam, #1650).
    if let Some(n) = shown.filter(|n| !n.is_empty()) {
        crate::trace!(
            "CreateFileMapping({n}, fichier 0x{file:x}, taille {max_size}) = 0x{h:x}, taille réelle {:?} (erreur {})",
            mapping_size(h),
            crate::dlls::kernel32::get_last_error()
        );
    }
    h
}

/// Taille d'une section (pour la trace).
fn mapping_size(h: usize) -> Option<u64> {
    Some(mappings().as_ref()?.get(&h)?.max_size)
}

unsafe fn create_mapping_inner(
    file: usize,
    security: usize,
    protect: u32,
    max_size: u64,
    name: Option<String>,
) -> usize {
    let key = name.as_deref().filter(|n| !n.is_empty()).map(name_key);
    if let Some(k) = key.as_deref() {
        if let Some(h) = open_named(k, SECTION_ALL_ACCESS) {
            crate::dlls::inherit::from_security_attributes(h, security);
            crate::process::set_last_error(ERROR_ALREADY_EXISTS);
            return h;
        }
        // Même nom dans un autre processus du compte (#20).
        match create_shared(k, file, protect, max_size) {
            Some(Ok((h, existed))) => {
                crate::dlls::inherit::from_security_attributes(h, security);
                crate::process::set_last_error(if existed { ERROR_ALREADY_EXISTS } else { 0 });
                return h;
            }
            Some(Err(e)) => {
                crate::process::set_last_error(e);
                return 0;
            }
            None => {}
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
        match file_source(file, protect, max_size) {
            Ok(v) => v,
            Err(e) => {
                crate::process::set_last_error(e);
                return 0;
            }
        }
    };
    let handle = insert_mapping(fd, size, true, key, SECTION_ALL_ACCESS);
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

fn open_mapping(name: Option<String>, access: u32, inherit: i32) -> usize {
    let Some(key) = name.as_deref().filter(|n| !n.is_empty()).map(name_key) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let found = open_named(&key, section_rights(access))
        .or_else(|| open_shared(&key, section_rights(access)));
    crate::trace!(
        "OpenFileMapping({}, accès 0x{access:x}) = {:?}, taille {:?}",
        name.as_deref().unwrap_or(""),
        found,
        found.and_then(mapping_size)
    );
    match found {
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
    access: u32,
    inherit: i32,
    name: *const u16,
) -> usize {
    open_mapping(
        (!name.is_null()).then(|| crate::dlls::wstr(name)),
        access,
        inherit,
    )
}

pub unsafe extern "win64" fn open_file_mapping_a(
    access: u32,
    inherit: i32,
    name: *const u8,
) -> usize {
    open_mapping(
        (!name.is_null()).then(|| crate::dlls::cstr(name)),
        access,
        inherit,
    )
}

/// DuplicateHandle d'un mapping : nouveau handle, même mémoire. `None` si
/// ce n'est pas un mapping ; `Err(5)` si les droits demandés dépassent
/// ceux du handle (Chromium vérifie ainsi qu'une région en lecture seule ne
/// peut pas redevenir inscriptible).
pub fn duplicate_mapping(h: usize, access: u32, options: u32) -> Option<Result<usize, u32>> {
    let (fd, size, name, have) = {
        let g = mappings();
        let fm = g.as_ref()?.get(&h)?;
        (fm.fd, fm.max_size, fm.name.clone(), fm.access)
    };
    let access = if options & DUPLICATE_SAME_ACCESS != 0 {
        have
    } else if rights_allowed(have, access) {
        section_rights(access) & SECTION_ALL_ACCESS
    } else {
        return Some(Err(ERROR_ACCESS_DENIED));
    };
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    Some(if copy >= 0 {
        Ok(insert_mapping(copy, size, true, name, access))
    } else {
        Err(ERROR_NOT_ENOUGH_MEMORY)
    })
}

/// Droits d'un handle de mapping (NtQueryObject).
pub fn mapping_access(h: usize) -> Option<u32> {
    Some(mappings().as_ref()?.get(&h)?.access)
}

/// Mapping transmissible à un enfant : (descripteur, taille, droits).
pub fn export(h: usize) -> Option<(i32, u64, u32)> {
    let g = mappings();
    let fm = g.as_ref()?.get(&h)?;
    Some((fm.fd, fm.max_size, fm.access))
}

/// L'enfant reprend un mapping hérité, sous le même numéro de handle.
pub fn adopt(h: usize, fd: std::os::fd::OwnedFd, size: u64, access: u32) {
    use std::os::fd::IntoRawFd;
    mappings().as_mut().unwrap().insert(
        h,
        FileMapping {
            fd: fd.into_raw_fd(),
            max_size: size,
            owned_fd: true,
            name: None,
            access,
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
    map_view_at(mapping, access, offset_high, offset_low, bytes_to_map, 0)
}

unsafe fn map_view_at(
    mapping: usize,
    access: u32,
    offset_high: u32,
    offset_low: u32,
    bytes_to_map: usize,
    base: usize,
) -> usize {
    // Adresse imposée : alignée sur 64 Kio et libre, sinon
    // ERROR_INVALID_ADDRESS, comme Windows. coreclr essaie ainsi plusieurs
    // adresses proches de son code pour ses vues exécutables (#665).
    if base != 0 && !base.is_multiple_of(65536) {
        crate::process::set_last_error(ERROR_INVALID_ADDRESS);
        return 0;
    }
    let guard = mappings();
    let fm = match guard.as_ref().and_then(|m| m.get(&mapping)) {
        Some(m) => m,
        None => {
            crate::process::set_last_error(6);
            return 0;
        }
    };

    // Vue inscriptible (ou lisible) que le handle n'autorise pas :
    // ERROR_ACCESS_DENIED, comme Windows.
    // FILE_MAP_WRITE suffit pour une vue lecture-écriture.
    let need = if access & FILE_MAP_WRITE != 0 {
        SECTION_MAP_WRITE
    } else if access & (_FILE_MAP_READ | FILE_MAP_COPY) != 0 {
        SECTION_MAP_READ
    } else {
        0
    };
    if need & !fm.access != 0 {
        crate::process::set_last_error(ERROR_ACCESS_DENIED);
        return 0;
    }

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

    // FILE_MAP_COPY (0x1) est aussi le bit SECTION_QUERY, présent dans
    // FILE_MAP_ALL_ACCESS (0xF001F) : seule une demande de copie sans
    // lecture ni écriture explicites donne une vue privée (#1707).
    let copy_on_write =
        access & FILE_MAP_COPY != 0 && access & (FILE_MAP_WRITE | _FILE_MAP_READ) == 0;
    let mut prot = if access & FILE_MAP_WRITE != 0 || copy_on_write {
        libc::PROT_READ | libc::PROT_WRITE
    } else {
        libc::PROT_READ
    };
    // FILE_MAP_EXECUTE : vue exécutable (code du JIT de coreclr, #665).
    if access & FILE_MAP_EXECUTE != 0 {
        prot |= libc::PROT_EXEC;
    }
    let win_prot = match (prot & libc::PROT_WRITE != 0, prot & libc::PROT_EXEC != 0) {
        (true, true) => 0x40,
        (true, false) => 0x04,
        (false, true) => 0x20,
        (false, false) => 0x02,
    };

    // Vue partagée (les écritures des autres vues y apparaissent), sauf
    // copie sur écriture demandée.
    let flags = if copy_on_write {
        libc::MAP_PRIVATE
    } else {
        libc::MAP_SHARED
    };

    // Sans adresse imposée, la vue est posée sur 64 Kio comme sous Windows :
    // on réserve un peu plus large, puis on remplace la partie alignée.
    let target = if base != 0 {
        base
    } else {
        let span = len.saturating_add(65536);
        let raw = libc::mmap(
            std::ptr::null_mut(),
            span,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        );
        if raw == libc::MAP_FAILED {
            crate::process::set_last_error(8);
            return 0;
        }
        let aligned = (raw as usize).next_multiple_of(65536);
        let prefix = aligned - raw as usize;
        if prefix > 0 {
            libc::munmap(raw, prefix);
        }
        let tail = span - prefix - len;
        if tail > 0 {
            libc::munmap((aligned + len) as *mut libc::c_void, tail);
        }
        aligned
    };
    let flags = if base != 0 {
        flags | libc::MAP_FIXED_NOREPLACE
    } else {
        flags | libc::MAP_FIXED
    };
    let p = libc::mmap(
        target as *mut libc::c_void,
        len,
        prot,
        flags,
        fm.fd,
        offset as libc::off_t,
    );

    if p == libc::MAP_FAILED {
        let busy = std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST);
        if base == 0 {
            libc::munmap(target as *mut libc::c_void, len);
        }
        crate::process::set_last_error(if busy { ERROR_INVALID_ADDRESS } else { 8 });
        return 0;
    }
    // Ancien noyau sans MAP_FIXED_NOREPLACE : l'adresse n'est qu'un indice.
    if p as usize != target {
        libc::munmap(p, len);
        crate::process::set_last_error(ERROR_INVALID_ADDRESS);
        return 0;
    }

    let addr = p as usize;
    let view_name = fm.name.clone();
    if let Some(n) = fm.name.as_deref() {
        // Une vue garde la section nommée vivante, et son nom trouvable par
        // OpenFileMapping, même après CloseHandle, comme sous Windows.
        crate::dlls::named_objects::retain(crate::dlls::named_objects::SECTION, n);
        crate::trace!(
            "MapViewOfFile({n}, accès 0x{access:x}, décalage {offset}, demandé {bytes_to_map}) = 0x{addr:x}, {len} octets"
        );
    }
    drop(guard);
    views().as_mut().unwrap().insert(
        addr,
        MappedView {
            addr,
            len,
            protection: vec![win_prot; len.div_ceil(PAGE)],
            name: view_name,
        },
    );
    addr
}

/// Nom de la section d'une vue qui contient `addr` (trace de VirtualQuery).
pub fn named_view_at(addr: usize) -> Option<String> {
    views()
        .as_ref()?
        .values()
        .find(|v| addr >= v.addr && addr < v.addr + v.len)
        .and_then(|v| v.name.clone())
}

/// Change la protection des pages [start, end) si elles sont toutes dans
/// une même vue. `None` : hors de toute vue. `Some(Err(code))` : échec.
/// `Some(Ok(ancienne))` : protection Windows de la première page avant.
/// Sert à VirtualAlloc(MEM_COMMIT) et VirtualProtect sur une vue, que
/// coreclr utilise pour son code double-mappé (W^X, #665).
pub fn protect_view_pages(
    start: usize,
    end: usize,
    prot: u32,
    unix_prot: i32,
) -> Option<Result<u32, u32>> {
    let mut guard = views();
    let v = guard
        .as_mut()?
        .values_mut()
        .find(|v| start >= v.addr && end <= v.addr + v.len.div_ceil(PAGE) * PAGE && start < end)?;
    if unsafe { libc::mprotect(start as *mut libc::c_void, end - start, unix_prot) } != 0 {
        return Some(Err(8));
    }
    let first = (start - v.addr) / PAGE;
    let count = (end - start) / PAGE;
    let old = v.protection[first];
    v.protection[first..first + count].fill(prot);
    Some(Ok(old))
}

/// MapViewOfFileEx : à l'adresse demandée si elle est libre, sinon échec.
pub unsafe extern "win64" fn map_view_of_file_ex(
    mapping: usize,
    access: u32,
    offset_high: u32,
    offset_low: u32,
    bytes_to_map: usize,
    base: usize,
) -> usize {
    map_view_at(mapping, access, offset_high, offset_low, bytes_to_map, base)
}

/// UnmapViewOfFile
pub unsafe extern "win64" fn unmap_view_of_file(addr: usize) -> i32 {
    let view = views().as_mut().and_then(|m| m.remove(&addr));
    match view {
        Some(v) => {
            libc::munmap(v.addr as *mut _, v.len);
            if let Some(k) = v.name.as_deref() {
                crate::dlls::named_objects::release(crate::dlls::named_objects::SECTION, k);
            }
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

// ============================================================
//  Sections de ntdll (NtCreateSection, NtOpenSection, NtMapViewOfSection)
// ============================================================

/// NtCreateSection : même moteur que CreateFileMapping. Ok((handle,
/// existait)) ou le code d'erreur Win32.
pub(crate) unsafe fn nt_create(
    file: usize,
    protect: u32,
    max_size: u64,
    name: Option<String>,
) -> Result<(usize, bool), u32> {
    let h = create_mapping(file, 0, protect, max_size, name);
    let err = crate::dlls::kernel32::get_last_error();
    if h == 0 {
        Err(if err == 0 {
            ERROR_INVALID_PARAMETER
        } else {
            err
        })
    } else {
        Ok((h, err == ERROR_ALREADY_EXISTS))
    }
}

/// NtOpenSection : handle vers une section nommée, ou 0.
pub(crate) fn nt_open(name: String, access: u32) -> usize {
    open_mapping(Some(name), access, 0)
}

/// Droits de vue (FILE_MAP_*) pour une protection de page de ntdll.
pub(crate) fn view_access(protect: u32) -> u32 {
    match protect & 0xFF {
        0x02 => _FILE_MAP_READ,
        0x08 => FILE_MAP_COPY,
        0x10 | 0x20 => _FILE_MAP_READ | FILE_MAP_EXECUTE,
        0x40 => FILE_MAP_WRITE | FILE_MAP_EXECUTE,
        0x80 => FILE_MAP_COPY | FILE_MAP_EXECUTE,
        _ => FILE_MAP_WRITE,
    }
}

/// NtMapViewOfSection dans le processus courant : (adresse, taille de la
/// vue) ou le code d'erreur Win32.
pub(crate) unsafe fn nt_map(
    section: usize,
    protect: u32,
    offset: u64,
    size: usize,
    base: usize,
) -> Result<(usize, usize), u32> {
    let addr = map_view_at(
        section,
        view_access(protect),
        (offset >> 32) as u32,
        offset as u32,
        size,
        base,
    );
    if addr == 0 {
        let err = crate::dlls::kernel32::get_last_error();
        return Err(if err == 0 {
            ERROR_INVALID_PARAMETER
        } else {
            err
        });
    }
    let len = views()
        .as_ref()
        .and_then(|m| m.get(&addr).map(|v| v.len))
        .unwrap_or(size);
    Ok((addr, len))
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
        if let Some(k) = fm.name.as_deref() {
            crate::dlls::named_objects::release(crate::dlls::named_objects::SECTION, k);
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
    fn region_en_lecture_seule_comme_chromium() {
        let _teb = crate::process::test_teb();
        unsafe {
            use crate::dlls::kernel32::{close_handle, duplicate_handle, get_current_process};
            let me = get_current_process();
            let h = create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 4096, std::ptr::null());
            assert_ne!(h, 0);
            // Create : FILE_MAP_READ | FILE_MAP_WRITE | SECTION_QUERY.
            let mut rw = 0usize;
            assert_eq!(duplicate_handle(me, h, me, &mut rw, 0x7, 0, 0), 1);
            close_handle(h);
            assert_eq!(mapping_access(rw), Some(0x7));
            // ConvertToReadOnly : FILE_MAP_READ | SECTION_QUERY, source fermée.
            let mut ro = 0usize;
            assert_eq!(duplicate_handle(me, rw, me, &mut ro, 0x5, 0, 1), 1);
            assert_eq!(mapping_access(rw), None);
            // Le contrôle de Chromium : FILE_MAP_WRITE doit être refusé.
            let mut w = 0usize;
            assert_eq!(
                duplicate_handle(me, ro, me, &mut w, FILE_MAP_WRITE, 0, 0),
                0
            );
            assert_eq!(crate::process::last_error(), 5);
            assert_eq!(w, 0);
            // Ni vue inscriptible, mais une vue en lecture.
            assert_eq!(map_view_of_file(ro, FILE_MAP_WRITE, 0, 0, 0), 0);
            let v = map_view_of_file(ro, _FILE_MAP_READ, 0, 0, 0);
            assert_ne!(v, 0);
            unmap_view_of_file(v);
            // DUPLICATE_SAME_ACCESS garde les droits.
            let mut same = 0usize;
            assert_eq!(duplicate_handle(me, ro, me, &mut same, 0, 0, 2), 1);
            assert_eq!(mapping_access(same), Some(0x5));
            // GENERIC_READ se traduit en lecture + interrogation.
            assert!(rights_allowed(0x2_0005, 0x8000_0000));
            assert!(!rights_allowed(0x5, 0x4000_0000));
            close_handle(same);
            close_handle(ro);
        }
    }

    #[test]
    fn double_mappage_w_xor_x_comme_coreclr() {
        let _teb = crate::process::test_teb();
        unsafe {
            use crate::dlls::kernel32::{close_handle, virtual_alloc, virtual_protect};
            const SEC_RESERVE: u32 = 0x0400_0000;
            // CreateFileMapping(INVALID_HANDLE_VALUE, PAGE_EXECUTE_READWRITE |
            // SEC_RESERVE, 64 Gio) : le mappeur de coreclr.
            let h = create_file_mapping_w(
                usize::MAX,
                0,
                PAGE_EXECUTE_READWRITE | SEC_RESERVE,
                0x10,
                0,
                std::ptr::null(),
            );
            assert_ne!(h, 0);
            // Vue exécutable, puis VirtualAlloc(MEM_COMMIT, PAGE_EXECUTE_READ).
            let rx = map_view_of_file_ex(h, FILE_MAP_EXECUTE | _FILE_MAP_READ, 0, 0, 0x10000, 0);
            assert_ne!(rx, 0);
            let c = virtual_alloc(rx as *mut _, 0x2000, 0x1000, 0x20);
            assert_eq!(c as usize, rx);
            // Vue inscriptible des mêmes pages : ce qu'on y écrit se lit dans
            // la vue exécutable.
            let rw = map_view_of_file(h, FILE_MAP_WRITE | _FILE_MAP_READ, 0, 0, 0x10000);
            assert_ne!(rw, 0);
            assert_eq!(
                virtual_alloc(rw as *mut _, 0x2000, 0x1000, 0x04) as usize,
                rw
            );
            *(rw as *mut u8) = 0xC3;
            assert_eq!(*(rx as *const u8), 0xC3);
            // VirtualProtect sur la vue rend l'ancienne protection.
            let mut old = 0u32;
            assert_eq!(virtual_protect(rx as *mut _, 0x1000, 0x02, &mut old), 1);
            assert_eq!(old, 0x20);
            // Adresse imposée : non alignée ou occupée, ERROR_INVALID_ADDRESS.
            assert_eq!(
                map_view_of_file_ex(h, _FILE_MAP_READ, 0, 0, 0x10000, rx + 0x1000),
                0
            );
            assert_eq!(crate::process::last_error(), 487);
            assert_eq!(map_view_of_file_ex(h, _FILE_MAP_READ, 0, 0, 0x10000, rx), 0);
            assert_eq!(crate::process::last_error(), 487);
            // Adresse libre : la vue y est posée.
            unmap_view_of_file(rx);
            assert_eq!(
                map_view_of_file_ex(h, _FILE_MAP_READ, 0, 0, 0x10000, rx),
                rx
            );
            assert_eq!(*(rx as *const u8), 0xC3);
            unmap_view_of_file(rx);
            unmap_view_of_file(rw);
            close_handle(h);
        }
    }

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

    /// Steam (#1650, #1707) : deux vues FILE_MAP_ALL_ACCESS d'une section nommée
    /// partagent leurs pages, et la section reste ouvrable par son nom
    /// tant qu'une vue existe, même après CloseHandle du créateur.
    #[test]
    fn section_nommee_vues_all_access_partagees_et_vivantes() {
        let _teb = crate::process::test_teb();
        unsafe {
            let name: Vec<u16> = format!("Local\\winlux-test-1707-{}", std::process::id())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let h = create_file_mapping_w(usize::MAX, 0, PAGE_READWRITE, 0, 8208, name.as_ptr());
            assert_ne!(h, 0);
            let a = map_view_of_file(h, 0xF001F, 0, 0, 0) as *mut u8;
            let o = open_file_mapping_w(0xF001F, 0, name.as_ptr());
            assert_ne!(o, 0);
            let b = map_view_of_file(o, 0xF001F, 0, 0, 0) as *mut u8;
            assert!(!a.is_null() && !b.is_null());
            *a.add(16) = 42;
            assert_eq!(*b.add(16), 42);
            crate::dlls::kernel32::close_handle(o);
            unmap_view_of_file(b as usize);
            // Le créateur ferme son handle mais garde sa vue.
            crate::dlls::kernel32::close_handle(h);
            let again = open_file_mapping_w(0xF001F, 0, name.as_ptr());
            assert_ne!(again, 0);
            assert_eq!(mapping_size(again), Some(8208));
            let c = map_view_of_file(again, 0xF001F, 0, 0, 0) as *mut u8;
            assert_eq!(*c.add(16), 42);
            *c = 9;
            assert_eq!(*a, 9);
            // FILE_MAP_COPY seul : copie privée.
            let d = map_view_of_file(again, FILE_MAP_COPY, 0, 0, 0) as *mut u8;
            *d = 1;
            assert_eq!(*a, 9);
            for v in [a, c, d] {
                unmap_view_of_file(v as usize);
            }
            crate::dlls::kernel32::close_handle(again);
            assert_eq!(open_file_mapping_w(4, 0, name.as_ptr()), 0);
        }
    }
}
