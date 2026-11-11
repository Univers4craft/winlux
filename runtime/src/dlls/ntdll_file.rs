//! Fonctions de fichiers natives de ntdll (Nt*File) sur les descripteurs
//! Linux de Winlux.
//!
//! libuv (Node.js) et Bun refusent de démarrer si NtQueryInformationFile,
//! NtSetInformationFile, NtQueryVolumeInformationFile, NtQueryDirectoryFile
//! ou NtDeviceIoControlFile manquent : c'est par elles que passent stat(),
//! la liste d'un dossier, rename/unlink et les informations de volume.
//! Les structures suivent la documentation publique (ntifs.h, wdm.h).
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::unix::ffi::OsStrExt;
use std::sync::Mutex;

const STATUS_SUCCESS: u32 = 0;
const STATUS_BUFFER_OVERFLOW: u32 = 0x8000_0005;
const STATUS_NO_MORE_FILES: u32 = 0x8000_0006;
const STATUS_INVALID_INFO_CLASS: u32 = 0xC000_0003;
const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
const STATUS_END_OF_FILE: u32 = 0xC000_0011;
const STATUS_ACCESS_DENIED: u32 = 0xC000_0022;
const STATUS_BUFFER_TOO_SMALL: u32 = 0xC000_0023;
const STATUS_OBJECT_NAME_NOT_FOUND: u32 = 0xC000_0034;
const STATUS_OBJECT_NAME_COLLISION: u32 = 0xC000_0035;
const STATUS_OBJECT_PATH_NOT_FOUND: u32 = 0xC000_003A;
const STATUS_DISK_FULL: u32 = 0xC000_007F;
const STATUS_NOT_SUPPORTED: u32 = 0xC000_00BB;
const STATUS_DIRECTORY_NOT_EMPTY: u32 = 0xC000_0101;
const STATUS_NOT_A_DIRECTORY: u32 = 0xC000_0103;
const STATUS_FILE_IS_A_DIRECTORY: u32 = 0xC000_00BA;
const STATUS_PIPE_BROKEN: u32 = 0xC000_014B;
const STATUS_NOT_A_REPARSE_POINT: u32 = 0xC000_0275;
const STATUS_UNSUCCESSFUL: u32 = 0xC000_0001;

const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

/// Écrit le bloc IO_STATUS_BLOCK { Status (union 8 octets), Information }.
unsafe fn iosb(io: *mut c_void, status: u32, info: usize) -> u32 {
    if !io.is_null() {
        (io as *mut u64).write_unaligned(status as u64);
        (io as *mut usize).add(1).write_unaligned(info);
    }
    status
}

pub(crate) fn status_of_errno(e: i32) -> u32 {
    match e {
        libc::ENOENT => STATUS_OBJECT_NAME_NOT_FOUND,
        libc::ENOTDIR => STATUS_OBJECT_PATH_NOT_FOUND,
        libc::EEXIST => STATUS_OBJECT_NAME_COLLISION,
        libc::EACCES | libc::EPERM | libc::EROFS => STATUS_ACCESS_DENIED,
        libc::ENOTEMPTY => STATUS_DIRECTORY_NOT_EMPTY,
        libc::EISDIR => STATUS_FILE_IS_A_DIRECTORY,
        libc::ENOSPC => STATUS_DISK_FULL,
        libc::EBADF => STATUS_INVALID_HANDLE,
        libc::EINVAL => STATUS_INVALID_PARAMETER,
        libc::EPIPE => STATUS_PIPE_BROKEN,
        _ => STATUS_UNSUCCESSFUL,
    }
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn fd_of(handle: usize) -> Option<i32> {
    crate::dlls::kernel32::fd_of_handle(handle)
}

/// Chemin Linux d'un descripteur ouvert (via /proc/self/fd).
pub(crate) fn path_of_fd(fd: i32) -> Option<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{fd}")).ok()
}

fn filetime(sec: i64, nsec: i64) -> i64 {
    (sec + 11_644_473_600) * 10_000_000 + nsec / 100
}

/// Les informations Windows d'un `stat` Linux.
pub(crate) struct WinStat {
    pub creation: i64,
    pub access: i64,
    pub write: i64,
    pub change: i64,
    pub size: i64,
    pub alloc: i64,
    pub attributes: u32,
    pub links: u32,
    pub index: i64,
    pub directory: bool,
    pub reparse_tag: u32,
    pub dev: u64,
}

pub(crate) fn win_stat(st: &libc::stat, name: Option<&[u8]>, is_link: bool) -> WinStat {
    let directory = st.st_mode & libc::S_IFMT == libc::S_IFDIR;
    let mut attributes = 0;
    if directory {
        attributes |= FILE_ATTRIBUTE_DIRECTORY;
    } else if st.st_mode & 0o222 == 0 {
        attributes |= FILE_ATTRIBUTE_READONLY;
    }
    if name.is_some_and(|n| n.len() > 1 && n[0] == b'.' && n != b"..") {
        attributes |= FILE_ATTRIBUTE_HIDDEN;
    }
    if is_link {
        attributes |= FILE_ATTRIBUTE_REPARSE_POINT;
    }
    if attributes == 0 {
        attributes = if directory {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_ARCHIVE
        };
    }
    let write = filetime(st.st_mtime, st.st_mtime_nsec);
    WinStat {
        creation: write.min(filetime(st.st_ctime, st.st_ctime_nsec)),
        access: filetime(st.st_atime, st.st_atime_nsec),
        write,
        change: filetime(st.st_ctime, st.st_ctime_nsec),
        size: if directory { 0 } else { st.st_size },
        alloc: if directory { 0 } else { st.st_blocks * 512 },
        attributes,
        links: st.st_nlink as u32,
        index: st.st_ino as i64,
        directory,
        reparse_tag: if is_link { IO_REPARSE_TAG_SYMLINK } else { 0 },
        dev: st.st_dev,
    }
}

unsafe fn fstat(fd: i32) -> Result<libc::stat, u32> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) != 0 {
        return Err(status_of_errno(errno()));
    }
    Ok(st)
}

/// Le handle désigne-t-il un lien symbolique ouvert sans le suivre
/// (FILE_FLAG_OPEN_REPARSE_POINT → O_PATH|O_NOFOLLOW) ?
unsafe fn fd_is_symlink(st: &libc::stat) -> bool {
    st.st_mode & libc::S_IFMT == libc::S_IFLNK
}

struct Out {
    p: *mut u8,
    len: usize,
}
impl Out {
    unsafe fn put<T: Copy>(&self, off: usize, v: T) {
        if off + std::mem::size_of::<T>() <= self.len {
            (self.p.add(off) as *mut T).write_unaligned(v);
        }
    }
    unsafe fn zero(&self, n: usize) {
        std::ptr::write_bytes(self.p, 0, n.min(self.len));
    }
}

fn windows_name_of(path: &std::path::Path) -> Vec<u16> {
    // FileNameInformation : chemin sans la lettre de lecteur (« \Users\… »).
    let w = crate::paths::to_windows(path);
    let rest = if w.as_bytes().get(1) == Some(&b':') {
        &w[2..]
    } else {
        &w[..]
    };
    rest.encode_utf16().collect()
}

unsafe fn put_basic(o: &Out, at: usize, s: &WinStat) {
    o.put(at, s.creation);
    o.put(at + 8, s.access);
    o.put(at + 16, s.write);
    o.put(at + 24, s.change);
    o.put(at + 32, s.attributes);
}
unsafe fn put_standard(o: &Out, at: usize, s: &WinStat) {
    o.put(at, s.alloc);
    o.put(at + 8, s.size);
    o.put(at + 16, s.links);
    o.put(at + 20, 0u8);
    o.put(at + 21, s.directory as u8);
}

/// `NtQueryInformationFile(handle, iosb, buffer, length, class)`.
pub unsafe extern "win64" fn NtQueryInformationFile(
    handle: usize,
    io: *mut c_void,
    buf: *mut u8,
    len: u32,
    class: u32,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    let o = Out {
        p: buf,
        len: len as usize,
    };
    if buf.is_null() {
        return iosb(io, STATUS_INVALID_PARAMETER, 0);
    }
    let st = match fstat(fd) {
        Ok(s) => s,
        Err(e) => return iosb(io, e, 0),
    };
    let path = path_of_fd(fd);
    let name = path
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.as_bytes().to_vec()));
    let s = win_stat(&st, name.as_deref(), fd_is_symlink(&st));
    let need = |n: usize| len as usize >= n;
    let position = || libc::lseek(fd, 0, libc::SEEK_CUR).max(0);
    let mode = if crate::dlls::kernel32::is_overlapped_handle(handle) {
        0u32
    } else {
        0x20u32 // FILE_SYNCHRONOUS_IO_NONALERT
    };
    match class {
        4 => {
            if !need(40) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(40);
            put_basic(&o, 0, &s);
            iosb(io, STATUS_SUCCESS, 40)
        }
        5 => {
            if !need(24) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(24);
            put_standard(&o, 0, &s);
            iosb(io, STATUS_SUCCESS, 24)
        }
        6 => {
            if !need(8) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.put(0, s.index);
            iosb(io, STATUS_SUCCESS, 8)
        }
        7 | 8 | 16 | 17 => {
            if !need(4) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            let v: u32 = match class {
                8 => 0x001F_01FF, // FILE_ALL_ACCESS
                16 => mode,
                _ => 0,
            };
            o.put(0, v);
            iosb(io, STATUS_SUCCESS, 4)
        }
        14 => {
            if !need(8) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.put(0, position());
            iosb(io, STATUS_SUCCESS, 8)
        }
        9 | 48 => {
            // FileNameInformation / FileNormalizedNameInformation.
            if !need(4) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            let w = crate::dlls::named_pipes::file_name(handle)
                .or_else(|| path.as_deref().map(windows_name_of))
                .unwrap_or_default();
            put_name(&o, 0, &w, io)
        }
        18 => {
            // FileAllInformation : Basic, Standard, Internal, Ea, Access,
            // Position, Mode, Alignment, puis Name.
            if !need(100) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(100);
            put_basic(&o, 0, &s);
            put_standard(&o, 40, &s);
            o.put(64, s.index);
            o.put(76, 0x001F_01FFu32);
            o.put(80, position());
            o.put(88, mode);
            let w = path.as_deref().map(windows_name_of).unwrap_or_default();
            let status = put_name(&o, 96, &w, std::ptr::null_mut());
            let written = (100 + w.len() * 2).min(len as usize);
            iosb(io, status, written)
        }
        23 => {
            // FilePipeLocalInformation : uniquement pour les tubes.
            if st.st_mode & libc::S_IFMT != libc::S_IFIFO {
                return iosb(io, STATUS_INVALID_PARAMETER, 0);
            }
            if !need(40) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(40);
            let mut avail: libc::c_int = 0;
            libc::ioctl(fd, libc::FIONREAD, &mut avail);
            o.put(8, 1u32); // NamedPipeState : FILE_PIPE_CONNECTED_STATE = 3
            o.put(8, 3u32);
            o.put(12, 1u32); // NamedPipeEnd
            o.put(16, 1u32); // MaximumInstances
            o.put(20, 1u32); // CurrentInstances
            o.put(24, 65536u32); // InboundQuota
            o.put(28, avail.max(0) as u32); // ReadDataAvailable
            o.put(32, 65536u32); // OutboundQuota
            o.put(36, 65536u32); // WriteQuotaAvailable
            iosb(io, STATUS_SUCCESS, 40)
        }
        34 => {
            // FileNetworkOpenInformation.
            if !need(56) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(56);
            o.put(0, s.creation);
            o.put(8, s.access);
            o.put(16, s.write);
            o.put(24, s.change);
            o.put(32, s.alloc);
            o.put(40, s.size);
            o.put(48, s.attributes);
            iosb(io, STATUS_SUCCESS, 56)
        }
        35 => {
            if !need(8) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.put(0, s.attributes);
            o.put(4, s.reparse_tag);
            iosb(io, STATUS_SUCCESS, 8)
        }
        59 => {
            // FileIdInformation : numéro de volume + identifiant 128 bits.
            if !need(24) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(24);
            o.put(0, s.dev);
            o.put(8, s.index);
            iosb(io, STATUS_SUCCESS, 24)
        }
        68 => {
            // FileStatInformation (Windows 10 1709+), lue en premier par libuv.
            if !need(72) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(72);
            o.put(0, s.index);
            o.put(8, s.creation);
            o.put(16, s.access);
            o.put(24, s.write);
            o.put(32, s.change);
            o.put(40, s.alloc);
            o.put(48, s.size);
            o.put(56, s.attributes);
            o.put(60, s.reparse_tag);
            o.put(64, s.links);
            o.put(68, 0x001F_01FFu32);
            iosb(io, STATUS_SUCCESS, 72)
        }
        71 => {
            // FileCaseSensitiveInformation : les dossiers Linux le sont.
            if !need(4) {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.put(0, 1u32);
            iosb(io, STATUS_SUCCESS, 4)
        }
        _ => iosb(io, STATUS_INVALID_INFO_CLASS, 0),
    }
}

/// { ULONG FileNameLength; WCHAR FileName[] } à l'offset `at`.
unsafe fn put_name(o: &Out, at: usize, w: &[u16], io: *mut c_void) -> u32 {
    let bytes = w.len() * 2;
    o.put(at, bytes as u32);
    let room = o.len.saturating_sub(at + 4);
    let n = bytes.min(room);
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, o.p.add(at + 4), n);
    let status = if n < bytes {
        STATUS_BUFFER_OVERFLOW
    } else {
        STATUS_SUCCESS
    };
    if !io.is_null() {
        iosb(io, status, at + 4 + n);
    }
    status
}

/// Chemin NT (« \??\C:\… ») éventuellement relatif à un handle de dossier.
unsafe fn nt_name_to_linux(
    root: usize,
    name: *const u16,
    bytes: u32,
) -> Option<std::path::PathBuf> {
    if name.is_null() {
        return None;
    }
    let w = std::slice::from_raw_parts(name, bytes as usize / 2);
    let s = String::from_utf16_lossy(w);
    if root != 0 {
        let base = path_of_fd(fd_of(root)?)?;
        return Some(base.join(s.replace('\\', "/")));
    }
    let s = s.strip_prefix("\\??\\").unwrap_or(&s);
    let s = s.strip_prefix("\\DosDevices\\").unwrap_or(s);
    let s = s.strip_prefix("\\\\?\\").unwrap_or(s);
    Some(crate::paths::to_linux(s))
}

fn cpath(p: &std::path::Path) -> std::ffi::CString {
    std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap_or_default()
}

/// `NtSetInformationFile(handle, iosb, buffer, length, class)`.
pub unsafe extern "win64" fn NtSetInformationFile(
    handle: usize,
    io: *mut c_void,
    buf: *const u8,
    len: u32,
    class: u32,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    if buf.is_null() {
        return iosb(io, STATUS_INVALID_PARAMETER, 0);
    }
    let rd64 = |off: usize| (buf.add(off) as *const i64).read_unaligned();
    let rd32 = |off: usize| (buf.add(off) as *const u32).read_unaligned();
    match class {
        4 if len >= 36 => {
            // FileBasicInformation : 0 = ne pas changer, -1 = figer.
            let ts = |ft: i64| -> libc::timespec {
                if ft <= 0 {
                    libc::timespec {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_OMIT,
                    }
                } else {
                    let t = ft - 116_444_736_000_000_000;
                    libc::timespec {
                        tv_sec: t.div_euclid(10_000_000) as libc::time_t,
                        tv_nsec: (t.rem_euclid(10_000_000) * 100) as libc::c_long,
                    }
                }
            };
            let times = [ts(rd64(8)), ts(rd64(16))];
            if (rd64(8) > 0 || rd64(16) > 0) && libc::futimens(fd, times.as_ptr()) != 0 {
                return iosb(io, status_of_errno(errno()), 0);
            }
            let attrs = rd32(32);
            if attrs != 0 {
                if let Ok(st) = fstat(fd) {
                    let mode = if attrs & FILE_ATTRIBUTE_READONLY != 0 {
                        st.st_mode & !0o222
                    } else if st.st_mode & 0o222 == 0 {
                        st.st_mode | 0o200
                    } else {
                        st.st_mode
                    };
                    if mode != st.st_mode {
                        libc::fchmod(fd, mode & 0o7777);
                    }
                }
            }
            iosb(io, STATUS_SUCCESS, 0)
        }
        10 | 11 | 65 | 72 if len >= 20 => {
            // FileRenameInformation(Ex) / FileLinkInformation(Ex) :
            // { ReplaceIfExists|Flags ; RootDirectory ; FileNameLength ; FileName }.
            let flags = rd32(0);
            let replace = if class == 10 || class == 11 {
                flags & 0xff != 0
            } else {
                flags & 1 != 0
            };
            let root = (buf.add(8) as *const usize).read_unaligned();
            let bytes = rd32(16);
            let Some(dst) = nt_name_to_linux(root, buf.add(20) as *const u16, bytes) else {
                return iosb(io, STATUS_INVALID_PARAMETER, 0);
            };
            let Some(src) = path_of_fd(fd) else {
                return iosb(io, STATUS_INVALID_HANDLE, 0);
            };
            crate::trace!(
                "NtSetInformationFile(classe {class}) : {} → {} (remplacer : {replace})",
                src.display(),
                dst.display()
            );
            let (cs, cd) = (cpath(&src), cpath(&dst));
            let rc = if class == 11 || class == 72 {
                if !replace && dst.symlink_metadata().is_ok() {
                    return iosb(io, STATUS_OBJECT_NAME_COLLISION, 0);
                }
                if replace {
                    libc::unlink(cd.as_ptr());
                }
                libc::link(cs.as_ptr(), cd.as_ptr())
            } else if replace {
                libc::rename(cs.as_ptr(), cd.as_ptr())
            } else {
                libc::renameat2(
                    libc::AT_FDCWD,
                    cs.as_ptr(),
                    libc::AT_FDCWD,
                    cd.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            if rc != 0 {
                let e = errno();
                // renameat2 absent (vieux noyaux, certains FS) : vérification manuelle.
                if e == libc::EINVAL && !replace && class != 11 && class != 72 {
                    if dst.symlink_metadata().is_ok() {
                        return iosb(io, STATUS_OBJECT_NAME_COLLISION, 0);
                    }
                    if libc::rename(cs.as_ptr(), cd.as_ptr()) == 0 {
                        return iosb(io, STATUS_SUCCESS, 0);
                    }
                    return iosb(io, status_of_errno(errno()), 0);
                }
                return iosb(io, status_of_errno(e), 0);
            }
            iosb(io, STATUS_SUCCESS, 0)
        }
        13 | 64 if len >= 1 => {
            // FileDispositionInformation(Ex) : suppression immédiate (le
            // fichier reste lisible par les handles ouverts, comme la
            // sémantique POSIX que Windows 10 applique par défaut).
            let delete = if class == 13 {
                *buf != 0
            } else {
                rd32(0) & 1 != 0
            };
            if !delete {
                return iosb(io, STATUS_SUCCESS, 0);
            }
            let Some(p) = path_of_fd(fd) else {
                return iosb(io, STATUS_INVALID_HANDLE, 0);
            };
            let c = cpath(&p);
            let is_dir = fstat(fd).is_ok_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR);
            let rc = if is_dir {
                libc::rmdir(c.as_ptr())
            } else {
                libc::unlink(c.as_ptr())
            };
            let e = (rc != 0).then(errno);
            crate::trace!(
                "NtSetInformationFile(classe {class}) : suppression de {} = {}",
                p.display(),
                e.map_or("ok".to_string(), |e| format!("errno {e}"))
            );
            if let Some(e) = e {
                return iosb(io, status_of_errno(e), 0);
            }
            iosb(io, STATUS_SUCCESS, 0)
        }
        14 if len >= 8 => {
            if libc::lseek(fd, rd64(0), libc::SEEK_SET) < 0 {
                return iosb(io, status_of_errno(errno()), 0);
            }
            iosb(io, STATUS_SUCCESS, 0)
        }
        19 => iosb(io, STATUS_SUCCESS, 0),
        20 if len >= 8 => {
            if libc::ftruncate(fd, rd64(0)) != 0 {
                return iosb(io, status_of_errno(errno()), 0);
            }
            iosb(io, STATUS_SUCCESS, 0)
        }
        16 | 23 => iosb(io, STATUS_SUCCESS, 0),
        _ => iosb(io, STATUS_INVALID_INFO_CLASS, 0),
    }
}

/// `NtQueryVolumeInformationFile(handle, iosb, buffer, length, class)`.
pub unsafe extern "win64" fn NtQueryVolumeInformationFile(
    handle: usize,
    io: *mut c_void,
    buf: *mut u8,
    len: u32,
    class: u32,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    if buf.is_null() {
        return iosb(io, STATUS_INVALID_PARAMETER, 0);
    }
    let o = Out {
        p: buf,
        len: len as usize,
    };
    let st = match fstat(fd) {
        Ok(s) => s,
        Err(e) => return iosb(io, e, 0),
    };
    let mut vfs: libc::statvfs = std::mem::zeroed();
    let have_vfs = libc::fstatvfs(fd, &mut vfs) == 0;
    let bsize = if have_vfs && vfs.f_frsize > 0 {
        vfs.f_frsize as u64
    } else {
        4096
    };
    match class {
        1 => {
            // FileFsVolumeInformation (sans nom de volume).
            if len < 18 {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(24);
            o.put(8, (st.st_dev as u32) ^ ((st.st_dev >> 32) as u32));
            o.put(12, 0u32);
            iosb(io, STATUS_SUCCESS, 18)
        }
        3 | 7 => {
            let sectors = (bsize / 512).max(1) as u32;
            let total = if have_vfs { vfs.f_blocks as i64 } else { 0 };
            let avail = if have_vfs { vfs.f_bavail as i64 } else { 0 };
            let free = if have_vfs { vfs.f_bfree as i64 } else { 0 };
            if class == 3 {
                if len < 24 {
                    return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
                }
                o.put(0, total);
                o.put(8, avail);
                o.put(16, sectors);
                o.put(20, 512u32);
                iosb(io, STATUS_SUCCESS, 24)
            } else {
                if len < 32 {
                    return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
                }
                o.put(0, total);
                o.put(8, avail);
                o.put(16, free);
                o.put(24, sectors);
                o.put(28, 512u32);
                iosb(io, STATUS_SUCCESS, 32)
            }
        }
        4 => {
            if len < 8 {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            let device: u32 = match st.st_mode & libc::S_IFMT {
                libc::S_IFIFO | libc::S_IFSOCK => 0x11, // FILE_DEVICE_NAMED_PIPE
                libc::S_IFCHR => {
                    if libc::isatty(fd) == 1 {
                        0x50 // FILE_DEVICE_CONSOLE
                    } else {
                        0x15 // FILE_DEVICE_NULL
                    }
                }
                _ => 0x07, // FILE_DEVICE_DISK
            };
            o.put(0, device);
            o.put(4, 0u32);
            iosb(io, STATUS_SUCCESS, 8)
        }
        5 => {
            // FileFsAttributeInformation : « NTFS », sensible à la casse,
            // noms Unicode, liens et identifiants de fichiers.
            if len < 12 {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            let name: Vec<u16> = "NTFS".encode_utf16().collect();
            // FILE_CASE_SENSITIVE_SEARCH | CASE_PRESERVED | UNICODE_ON_DISK |
            // PERSISTENT_ACLS | SUPPORTS_REPARSE_POINTS | SUPPORTS_HARD_LINKS.
            o.put(0, 0x0000_0001u32 | 0x2 | 0x4 | 0x8 | 0x80 | 0x0040_0000);
            o.put(4, 255i32);
            put_name(&o, 8, &name, io)
        }
        11 => {
            if len < 28 {
                return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
            }
            o.zero(28);
            for i in 0..4 {
                o.put(
                    i * 4,
                    if i == 0 || i == 3 {
                        512u32
                    } else {
                        bsize as u32
                    },
                );
            }
            iosb(io, STATUS_SUCCESS, 28)
        }
        _ => iosb(io, STATUS_INVALID_INFO_CLASS, 0),
    }
}

// ---- NtQueryDirectoryFile -------------------------------------------------

struct DirEntry {
    name: Vec<u16>,
    stat: WinStat,
}

struct DirCursor {
    entries: Vec<DirEntry>,
    pos: usize,
}

static DIR_CURSORS: Mutex<Option<HashMap<usize, DirCursor>>> = Mutex::new(None);

/// Oublie la position de lecture d'un dossier (à la fermeture du handle).
pub(crate) fn forget_dir_cursor(handle: usize) {
    if let Some(m) = crate::dlls::lock(&DIR_CURSORS).as_mut() {
        m.remove(&handle);
    }
}

fn wild_match(pattern: &[u16], name: &[u16]) -> bool {
    // * et ? (et leurs variantes DOS < > "), insensible à la casse.
    fn up(c: u16) -> u16 {
        char::from_u32(c as u32)
            .map(|ch| ch.to_uppercase().next().unwrap_or(ch) as u32 as u16)
            .unwrap_or(c)
    }
    let (mut p, mut n) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while n < name.len() {
        if p < pattern.len()
            && (pattern[p] == b'?' as u16
                || pattern[p] == b'>' as u16
                || up(pattern[p]) == up(name[n]))
        {
            p += 1;
            n += 1;
        } else if p < pattern.len() && (pattern[p] == b'*' as u16 || pattern[p] == b'<' as u16) {
            star = p;
            mark = n;
            p += 1;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && matches!(pattern[p], 0x2A | 0x3C | 0x22 | 0x3E) {
        p += 1;
    }
    p == pattern.len()
}

unsafe fn read_directory(fd: i32, filter: Option<&[u16]>) -> Result<Vec<DirEntry>, u32> {
    let dup = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3);
    if dup < 0 {
        return Err(status_of_errno(errno()));
    }
    let dir = libc::fdopendir(dup);
    if dir.is_null() {
        let e = errno();
        libc::close(dup);
        return Err(if e == libc::ENOTDIR {
            STATUS_NOT_A_DIRECTORY
        } else {
            status_of_errno(e)
        });
    }
    libc::rewinddir(dir);
    let mut out = Vec::new();
    loop {
        let ent = libc::readdir64(dir);
        if ent.is_null() {
            break;
        }
        let cname = std::ffi::CStr::from_ptr((*ent).d_name.as_ptr());
        let bytes = cname.to_bytes();
        let name: Vec<u16> = String::from_utf8_lossy(bytes).encode_utf16().collect();
        if let Some(f) = filter {
            if !wild_match(f, &name) {
                continue;
            }
        }
        let mut st: libc::stat = std::mem::zeroed();
        let mut is_link = false;
        if libc::fstatat(dup, cname.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) == 0
            && st.st_mode & libc::S_IFMT == libc::S_IFLNK
        {
            is_link = true;
            // Taille et type de la cible, comme un lien symbolique NTFS listé.
            let mut target: libc::stat = std::mem::zeroed();
            if libc::fstatat(dup, cname.as_ptr(), &mut target, 0) == 0 {
                st = target;
            }
        } else if libc::fstatat(dup, cname.as_ptr(), &mut st, 0) != 0 {
            continue;
        }
        out.push(DirEntry {
            name,
            stat: win_stat(&st, Some(bytes), is_link),
        });
    }
    libc::closedir(dir);
    // « . » et « .. » d'abord, comme NTFS ; le reste par ordre de nom.
    out.sort_by(|a, b| {
        let rank = |e: &DirEntry| match e.name.as_slice() {
            [0x2E] => 0,
            [0x2E, 0x2E] => 1,
            _ => 2,
        };
        rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// Taille de l'en-tête de chaque classe et offset du nom.
fn dir_class_layout(class: u32) -> Option<usize> {
    Some(match class {
        1 => 64,   // FileDirectoryInformation
        2 => 68,   // FileFullDirectoryInformation
        3 => 94,   // FileBothDirectoryInformation
        12 => 12,  // FileNamesInformation
        37 => 104, // FileIdBothDirectoryInformation
        38 => 80,  // FileIdFullDirectoryInformation
        _ => return None,
    })
}

unsafe fn put_dir_entry(o: &Out, class: u32, e: &DirEntry, header: usize) {
    let s = &e.stat;
    let name_bytes = (e.name.len() * 2) as u32;
    o.zero(header);
    if class == 12 {
        o.put(8, name_bytes);
    } else {
        o.put(8, s.creation);
        o.put(16, s.access);
        o.put(24, s.write);
        o.put(32, s.change);
        o.put(40, s.size);
        o.put(48, s.alloc);
        o.put(56, s.attributes);
        o.put(60, name_bytes);
        if class != 1 {
            // EaSize : pour un point d'analyse, c'est le ReparseTag.
            o.put(64, s.reparse_tag);
        }
        if class == 37 {
            o.put(96, s.index);
        }
        if class == 38 {
            o.put(72, s.index);
        }
    }
    std::ptr::copy_nonoverlapping(
        e.name.as_ptr() as *const u8,
        o.p.add(header),
        name_bytes as usize,
    );
}

/// `NtQueryDirectoryFile` : liste un dossier ouvert, synchrone.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn NtQueryDirectoryFile(
    handle: usize,
    event: usize,
    _apc: usize,
    _apc_ctx: usize,
    io: *mut c_void,
    buf: *mut u8,
    len: u32,
    class: u32,
    single: u8,
    file_name: *const c_void,
    restart: u8,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    let Some(header) = dir_class_layout(class) else {
        return iosb(io, STATUS_INVALID_INFO_CLASS, 0);
    };
    if buf.is_null() {
        return iosb(io, STATUS_INVALID_PARAMETER, 0);
    }
    let mut guard = crate::dlls::lock(&DIR_CURSORS);
    let map = guard.get_or_insert_with(HashMap::new);
    if restart != 0 || !map.contains_key(&handle) {
        let filter: Option<Vec<u16>> = if file_name.is_null() {
            None
        } else {
            let l = *(file_name as *const u16) as usize / 2;
            let p = *((file_name as *const u8).add(8) as *const *const u16);
            if p.is_null() || l == 0 {
                None
            } else {
                Some(std::slice::from_raw_parts(p, l).to_vec())
            }
        };
        let had = map.contains_key(&handle);
        match read_directory(fd, filter.as_deref()) {
            Ok(entries) => {
                if entries.is_empty() && !had && filter.is_some() {
                    return iosb(io, STATUS_NO_MORE_FILES, 0);
                }
                map.insert(handle, DirCursor { entries, pos: 0 });
            }
            Err(e) => return iosb(io, e, 0),
        }
    }
    let cur = map.get_mut(&handle).expect("curseur de dossier");
    if cur.pos >= cur.entries.len() {
        return iosb(io, STATUS_NO_MORE_FILES, 0);
    }
    let mut off = 0usize;
    let mut last: Option<usize> = None;
    let total = len as usize;
    while cur.pos < cur.entries.len() {
        let e = &cur.entries[cur.pos];
        let size = header + e.name.len() * 2;
        if off + size > total {
            if last.is_none() {
                // Même la première entrée ne tient pas.
                return iosb(io, STATUS_BUFFER_OVERFLOW, 0);
            }
            break;
        }
        let o = Out {
            p: buf.add(off),
            len: size,
        };
        put_dir_entry(&o, class, e, header);
        o.put(4, cur.pos as u32); // FileIndex
        if let Some(prev) = last {
            (buf.add(prev) as *mut u32).write_unaligned((off - prev) as u32);
        }
        last = Some(off);
        cur.pos += 1;
        off = (off + size + 7) & !7;
        if single != 0 {
            break;
        }
    }
    if let Some(prev) = last {
        (buf.add(prev) as *mut u32).write_unaligned(0);
    }
    let used = last
        .map(|l| l + header + cur.entries[cur.pos - 1].name.len() * 2)
        .unwrap_or(0);
    drop(guard);
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
    iosb(io, STATUS_SUCCESS, used)
}

// ---- NtReadFile / NtWriteFile ---------------------------------------------

/// FILE_USE_FILE_POINTER_POSITION / FILE_WRITE_TO_END_OF_FILE.
const USE_FILE_POINTER: i64 = -2;
const WRITE_TO_END: i64 = -1;

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn NtReadFile(
    handle: usize,
    event: usize,
    _apc: usize,
    _apc_ctx: usize,
    io: *mut c_void,
    buf: *mut u8,
    len: u32,
    offset: *const i64,
    _key: *const u32,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    let off = if offset.is_null() {
        USE_FILE_POINTER
    } else {
        *offset
    };
    let n = loop {
        let n = if off >= 0 {
            libc::pread(fd, buf as *mut c_void, len as usize, off)
        } else {
            libc::read(fd, buf as *mut c_void, len as usize)
        };
        if n < 0 && errno() == libc::EINTR {
            continue;
        }
        break n;
    };
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
    if n < 0 {
        return iosb(io, status_of_errno(errno()), 0);
    }
    if n == 0 && len > 0 {
        let is_pipe = fstat(fd).is_ok_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFIFO);
        return iosb(
            io,
            if is_pipe {
                STATUS_PIPE_BROKEN
            } else {
                STATUS_END_OF_FILE
            },
            0,
        );
    }
    iosb(io, STATUS_SUCCESS, n as usize)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn NtWriteFile(
    handle: usize,
    event: usize,
    _apc: usize,
    _apc_ctx: usize,
    io: *mut c_void,
    buf: *const u8,
    len: u32,
    offset: *const i64,
    _key: *const u32,
) -> u32 {
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    let off = if offset.is_null() {
        USE_FILE_POINTER
    } else {
        *offset
    };
    if off == WRITE_TO_END {
        libc::lseek(fd, 0, libc::SEEK_END);
    }
    let n = loop {
        let n = if off >= 0 {
            libc::pwrite(fd, buf as *const c_void, len as usize, off)
        } else {
            libc::write(fd, buf as *const c_void, len as usize)
        };
        if n < 0 && errno() == libc::EINTR {
            continue;
        }
        break n;
    };
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
    if n < 0 {
        return iosb(io, status_of_errno(errno()), 0);
    }
    iosb(io, STATUS_SUCCESS, n as usize)
}

// ---- NtDeviceIoControlFile / NtFsControlFile ------------------------------

const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;

/// `NtDeviceIoControlFile` : IOCTL_AFD_POLL sur un handle `\Device\Afd`
/// (mio/tokio, #1766). Les autres IOCTL de périphériques Windows (AFD sur
/// la socket elle-même, disques) n'ont pas d'équivalent ; libuv sait s'en
/// passer.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn NtDeviceIoControlFile(
    handle: usize,
    event: usize,
    _apc: usize,
    apc_ctx: usize,
    io: *mut c_void,
    code: u32,
    inb: *const u8,
    inl: u32,
    outb: *mut u8,
    outl: u32,
) -> u32 {
    if let Some(r) =
        super::afd::device_io_control(handle, event, apc_ctx, io, code, inb, inl, outb, outl)
    {
        return r;
    }
    iosb(io, STATUS_NOT_SUPPORTED, 0)
}

/// `NtCancelIoFileEx` : annule la demande dont l'IO_STATUS_BLOCK est
/// `request` (toutes celles du handle s'il est nul) ; STATUS_NOT_FOUND si
/// rien n'est en cours.
pub unsafe extern "win64" fn NtCancelIoFileEx(
    handle: usize,
    request: *mut c_void,
    io: *mut c_void,
) -> u32 {
    if let Some(r) = super::afd::cancel(handle, request as usize, io) {
        return r;
    }
    let ok = crate::dlls::kernel32::cancel_io_ex(handle, request.cast());
    let status = if ok != 0 { 0 } else { 0xC000_0225 };
    iosb(io, status, 0)
}

/// `NtCancelIoFile` : toutes les demandes du handle.
pub unsafe extern "win64" fn NtCancelIoFile(handle: usize, io: *mut c_void) -> u32 {
    NtCancelIoFileEx(handle, std::ptr::null_mut(), io)
}

/// `NtFsControlFile` : FSCTL_GET_REPARSE_POINT renvoie la cible d'un lien
/// symbolique Linux sous forme de REPARSE_DATA_BUFFER (lien symbolique
/// Windows), ce qui sert à readlink() et realpath().
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn NtFsControlFile(
    handle: usize,
    _event: usize,
    _apc: usize,
    _apc_ctx: usize,
    io: *mut c_void,
    code: u32,
    _inb: *const u8,
    _inl: u32,
    outb: *mut u8,
    outl: u32,
) -> u32 {
    if code != FSCTL_GET_REPARSE_POINT {
        return iosb(io, STATUS_NOT_SUPPORTED, 0);
    }
    let Some(fd) = fd_of(handle) else {
        return iosb(io, STATUS_INVALID_HANDLE, 0);
    };
    reparse_point_of_fd(fd, outb, outl, io)
}

pub(crate) unsafe fn reparse_point_of_fd(
    fd: i32,
    outb: *mut u8,
    outl: u32,
    io: *mut c_void,
) -> u32 {
    let mut target = [0u8; 4096];
    let n = libc::readlinkat(
        fd,
        c"".as_ptr(),
        target.as_mut_ptr() as *mut libc::c_char,
        target.len(),
    );
    if n <= 0 {
        return iosb(io, STATUS_NOT_A_REPARSE_POINT, 0);
    }
    let t = std::path::Path::new(std::ffi::OsStr::from_bytes(&target[..n as usize]));
    let (w, relative): (Vec<u16>, bool) = if t.is_absolute() {
        (crate::paths::to_windows(t).encode_utf16().collect(), false)
    } else {
        (
            t.to_string_lossy()
                .replace('/', "\\")
                .encode_utf16()
                .collect(),
            true,
        )
    };
    let subst: Vec<u16> = if relative {
        w.clone()
    } else {
        "\\??\\".encode_utf16().chain(w.iter().copied()).collect()
    };
    // REPARSE_DATA_BUFFER (SymbolicLinkReparseBuffer) : en-tête 8 octets,
    // 4 USHORT d'offsets/longueurs, Flags, puis Substitute + Print.
    let path_bytes = (subst.len() + w.len()) * 2;
    let total = 20 + path_bytes;
    if (outl as usize) < total || outb.is_null() {
        return iosb(io, STATUS_BUFFER_TOO_SMALL, 0);
    }
    let o = Out {
        p: outb,
        len: outl as usize,
    };
    o.zero(total);
    o.put(0, IO_REPARSE_TAG_SYMLINK);
    o.put(4, (total - 8) as u16);
    o.put(8, 0u16); // SubstituteNameOffset
    o.put(10, (subst.len() * 2) as u16);
    o.put(12, (subst.len() * 2) as u16); // PrintNameOffset
    o.put(14, (w.len() * 2) as u16);
    o.put(16, relative as u32); // SYMLINK_FLAG_RELATIVE
    std::ptr::copy_nonoverlapping(subst.as_ptr() as *const u8, outb.add(20), subst.len() * 2);
    std::ptr::copy_nonoverlapping(
        w.as_ptr() as *const u8,
        outb.add(20 + subst.len() * 2),
        w.len() * 2,
    );
    iosb(io, STATUS_SUCCESS, total)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "NtQueryInformationFile" => fn NtQueryInformationFile,
        "ZwQueryInformationFile" => fn NtQueryInformationFile,
        "NtSetInformationFile" => fn NtSetInformationFile,
        "ZwSetInformationFile" => fn NtSetInformationFile,
        "NtQueryVolumeInformationFile" => fn NtQueryVolumeInformationFile,
        "NtQueryDirectoryFile" => fn NtQueryDirectoryFile,
        "ZwQueryDirectoryFile" => fn NtQueryDirectoryFile,
        "NtReadFile" => fn NtReadFile,
        "NtWriteFile" => fn NtWriteFile,
        "NtDeviceIoControlFile" => fn NtDeviceIoControlFile,
        "NtCancelIoFileEx" => fn NtCancelIoFileEx,
        "NtCancelIoFile" => fn NtCancelIoFile,
        "NtFsControlFile" => fn NtFsControlFile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn motifs_dos() {
        assert!(wild_match(&w("*"), &w("abc")));
        assert!(wild_match(&w("*.TXT"), &w("notes.txt")));
        assert!(wild_match(&w("a?c"), &w("abc")));
        assert!(!wild_match(&w("*.exe"), &w("notes.txt")));
        assert!(wild_match(&w("<"), &w("x.y")));
    }

    /// Ouvre `name` relatif au dossier `root` par NtCreateFile.
    unsafe fn ouvrir_relatif(root: usize, name: &[u16], options: u32) -> usize {
        let us: [u64; 2] = [
            ((name.len() * 2) as u64) | (((name.len() * 2) as u64) << 16),
            name.as_ptr() as u64,
        ];
        let mut oa = [0u64; 6];
        oa[0] = 48;
        oa[1] = root as u64;
        oa[2] = us.as_ptr() as u64;
        let mut h = 0usize;
        let mut io = [0u64; 2];
        let st = crate::dlls::ntdll::nt_create_file(
            &mut h,
            0x0001_0000 | 0x0010_0000 | 1, // DELETE | SYNCHRONIZE | FILE_LIST_DIRECTORY
            oa.as_ptr() as *const c_void,
            io.as_mut_ptr() as *mut c_void,
            std::ptr::null(),
            0,
            7,
            1,
            options,
            std::ptr::null(),
            0,
        );
        assert_eq!(
            st,
            0,
            "ouverture relative de {}",
            String::from_utf16_lossy(name)
        );
        h
    }

    unsafe fn renommer(h: usize, dst: &std::path::Path) -> u32 {
        let name: Vec<u16> = dst.to_str().unwrap().encode_utf16().collect();
        let mut buf = vec![0u8; 20 + name.len() * 2];
        buf[16..20].copy_from_slice(&((name.len() * 2) as u32).to_le_bytes());
        std::ptr::copy_nonoverlapping(
            name.as_ptr() as *const u8,
            buf.as_mut_ptr().add(20),
            name.len() * 2,
        );
        let mut io = [0u64; 2];
        NtSetInformationFile(
            h,
            io.as_mut_ptr() as *mut c_void,
            buf.as_ptr(),
            buf.len() as u32,
            10,
        )
    }

    /// Le dossier renommé puis effacé comme `remove_dir_all` de Rust :
    /// liste par GetFileInformationByHandleEx(FileIdBothDirectoryRestartInfo),
    /// entrées ouvertes relativement au dossier avec FILE_DELETE_ON_CLOSE,
    /// puis le dossier lui-même (rm-* de Discord, #1885).
    #[test]
    fn dossier_renomme_puis_efface_comme_remove_dir_all() {
        let base = std::env::temp_dir().join(format!("winlux-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("download").join("paquet");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.bin"), b"a").unwrap();
        std::fs::write(src.join("b.bin"), b"b").unwrap();
        unsafe {
            let dir = crate::dlls::kernel32::insert_file(std::fs::File::open(&src).unwrap());
            let rm = base.join("rm-0");
            assert_eq!(renommer(dir, &rm), 0);
            assert!(rm.is_dir() && !src.exists());
            let mut buf = vec![0u8; 4096];
            assert_eq!(
                crate::dlls::kernel32::get_file_information_by_handle_ex(
                    dir,
                    11,
                    buf.as_mut_ptr(),
                    buf.len() as u32
                ),
                1
            );
            let mut noms = Vec::new();
            let mut off = 0usize;
            loop {
                let rd = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
                let len = rd(off + 60) as usize / 2;
                let p = buf.as_ptr().add(off + 104) as *const u16;
                noms.push(std::slice::from_raw_parts(p, len).to_vec());
                let next = rd(off) as usize;
                if next == 0 {
                    break;
                }
                off += next;
            }
            let lisibles: Vec<String> = noms.iter().map(|n| String::from_utf16_lossy(n)).collect();
            assert_eq!(lisibles, [".", "..", "a.bin", "b.bin"]);
            // Suite de la liste : plus rien.
            assert_eq!(
                crate::dlls::kernel32::get_file_information_by_handle_ex(
                    dir,
                    10,
                    buf.as_mut_ptr(),
                    buf.len() as u32
                ),
                0
            );
            for n in &noms[2..] {
                let h = ouvrir_relatif(dir, n, 0x1000);
                assert_eq!(crate::dlls::kernel32::close_handle(h), 1);
            }
            assert!(!rm.join("a.bin").exists() && !rm.join("b.bin").exists());
            let mut io = [0u64; 2];
            assert_eq!(
                NtSetInformationFile(dir, io.as_mut_ptr() as *mut c_void, [1u8].as_ptr(), 1, 13),
                0
            );
            crate::dlls::kernel32::close_handle(dir);
            assert!(!rm.exists(), "rm-0 doit disparaître");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// FILE_DELETE_ON_CLOSE efface le fichier sous son nom actuel, même
    /// renommé depuis l'ouverture (#1885).
    #[test]
    fn efface_a_la_fermeture_apres_renommage() {
        let base = std::env::temp_dir().join(format!("winlux-doc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("avant"), b"x").unwrap();
        unsafe {
            let dir = crate::dlls::kernel32::insert_file(std::fs::File::open(&base).unwrap());
            let h = ouvrir_relatif(dir, &w("avant"), 0x1000);
            assert_eq!(renommer(h, &base.join("apres")), 0);
            assert!(base.join("apres").exists());
            crate::dlls::kernel32::close_handle(h);
            assert!(!base.join("apres").exists() && !base.join("avant").exists());
            crate::dlls::kernel32::close_handle(dir);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Ramasse-miettes de la mise à jour de Discord : chaque entrée est
    /// ouverte par CreateFileW (DELETE, FILE_FLAG_BACKUP_SEMANTICS |
    /// FILE_FLAG_DELETE_ON_CLOSE), renommée en rm-N puis fermée ; fichiers et
    /// dossiers doivent disparaître sous leur nouveau nom (#1885).
    #[test]
    fn ramasse_miettes_rm_n_de_discord() {
        let base = std::env::temp_dir().join(format!("winlux-rmn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let download = base.join("download");
        std::fs::create_dir_all(download.join("incoming")).unwrap();
        std::fs::write(download.join("paquet"), b"p").unwrap();
        let ouvrir = |p: &std::path::Path| unsafe {
            let wp: Vec<u16> = crate::paths::to_windows(p)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let h = crate::dlls::kernel32::create_file_w(
                wp.as_ptr(),
                0x0001_0000,
                7,
                0,
                3,
                0x0200_0000 | 0x0020_0000 | 0x0400_0000,
                0,
            );
            assert!(h != 0 && h != usize::MAX, "ouverture de {}", p.display());
            h
        };
        let mut handles = Vec::new();
        for (i, p) in [
            download.join("paquet"),
            download.join("incoming"),
            download.clone(),
        ]
        .iter()
        .enumerate()
        {
            let h = ouvrir(p);
            assert_eq!(unsafe { renommer(h, &base.join(format!("rm-{i}"))) }, 0);
            handles.push(h);
        }
        for h in handles {
            assert_eq!(crate::dlls::kernel32::close_handle(h), 1);
        }
        let reste: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(reste.is_empty(), "restent : {reste:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn statut_des_erreurs_linux() {
        assert_eq!(status_of_errno(libc::ENOENT), STATUS_OBJECT_NAME_NOT_FOUND);
        assert_eq!(status_of_errno(libc::ENOTEMPTY), STATUS_DIRECTORY_NOT_EMPTY);
    }
}
