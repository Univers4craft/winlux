//! `DeviceIoControl` sur les disques, les volumes et les fichiers (#271) :
//! les codes `IOCTL_STORAGE_*`, `IOCTL_DISK_*`, `IOCTL_VOLUME_*` et `FSCTL_*`
//! que demandent les jeux, lanceurs, outils de sauvegarde et de licence
//! (modèle et numéro de série du disque, SSD ou non, taille, géométrie,
//! fichiers creux…), lus dans /sys/block. Les codes qui modifieraient un
//! volume (verrouillage, démontage, formatage) sont refusés proprement.
//!
//! `CreateFileW("\\\\.\\C:")` et `CreateFileW("\\\\.\\PhysicalDrive0")`
//! rendent un handle sur le dossier racine du lecteur ou sur l'entrée
//! /sys/block du disque : rien n'ouvre le vrai périphérique bloc.

use std::path::{Path, PathBuf};

pub const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
pub const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;
pub const IOCTL_STORAGE_GET_HOTPLUG_INFO: u32 = 0x002D_0C14;
pub const IOCTL_STORAGE_CHECK_VERIFY: u32 = 0x002D_4800;
pub const IOCTL_STORAGE_CHECK_VERIFY2: u32 = 0x002D_0800;
pub const IOCTL_DISK_GET_DRIVE_GEOMETRY: u32 = 0x0007_0000;
pub const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
pub const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405C;
pub const IOCTL_DISK_GET_PARTITION_INFO_EX: u32 = 0x0007_0048;
pub const IOCTL_DISK_IS_WRITABLE: u32 = 0x0007_0024;
pub const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
pub const FSCTL_LOCK_VOLUME: u32 = 0x0009_0018;
pub const FSCTL_UNLOCK_VOLUME: u32 = 0x0009_001C;
pub const FSCTL_DISMOUNT_VOLUME: u32 = 0x0009_0020;
pub const FSCTL_GET_COMPRESSION: u32 = 0x0009_003C;
pub const FSCTL_SET_COMPRESSION: u32 = 0x0009_C040;
pub const FSCTL_GET_OBJECT_ID: u32 = 0x0009_009C;
pub const FSCTL_CREATE_OR_GET_OBJECT_ID: u32 = 0x0009_00C0;
pub const FSCTL_SET_SPARSE: u32 = 0x0009_00C4;
pub const FSCTL_SET_ZERO_DATA: u32 = 0x0009_80C8;
pub const FSCTL_QUERY_ALLOCATED_RANGES: u32 = 0x0009_40CF;
pub const IOCTL_HID_SET_FEATURE: u32 = 0x000B_0191;
pub const IOCTL_HID_GET_FEATURE: u32 = 0x000B_0192;
pub const IOCTL_HID_SET_OUTPUT_REPORT: u32 = 0x000B_0195;
pub const IOCTL_HID_GET_INPUT_REPORT: u32 = 0x000B_01A2;

const ERROR_INVALID_FUNCTION: u32 = 1;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_WRITE_PROTECT: u32 = 19;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_MORE_DATA: u32 = 234;

fn sys_root() -> PathBuf {
    std::env::var_os("WINLUX_SYSFS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys"))
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
fn read_u64(p: &Path) -> u64 {
    read(p).parse().unwrap_or(0)
}

/// Disques physiques, dans l'ordre de PhysicalDrive0, 1… (les disques
/// virtuels du système, loop, ram, zram, et les lecteurs optiques exclus).
pub fn disks() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(sys_root().join("block"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| {
                    !["loop", "ram", "zram", "sr", "fd", "dm-", "md"]
                        .iter()
                        .any(|p| n.starts_with(p))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|n| {
        let rank = if n.starts_with("nvme") {
            0
        } else if n.starts_with("sd")
            || n.starts_with("vd")
            || n.starts_with("hd")
            || n.starts_with("xvd")
        {
            1
        } else {
            2
        };
        (rank, n.len(), n.clone())
    });
    v
}

/// Ce que désigne un handle ouvert : un disque entier, une partition, ou un
/// volume sans périphérique bloc (tmpfs, overlay de l'ISO live…).
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Disk(String),
    Partition { disk: String, part: String },
    Virtual { bytes: u64 },
}

/// Cible de `CreateFileW` pour `\\.\X:` (racine du lecteur) et
/// `\\.\PhysicalDriveN` (entrée /sys/block du disque).
pub fn device_target(path: &str) -> Option<PathBuf> {
    let rest = path
        .strip_prefix("\\\\.\\")
        .or_else(|| path.strip_prefix("\\\\?\\"))
        .or_else(|| path.strip_prefix("//./"))?;
    let upper = rest.trim_end_matches(['\\', '/']).to_ascii_uppercase();
    if upper.len() == 2
        && upper.ends_with(':')
        && upper.as_bytes()[0].is_ascii_alphabetic()
        && !rest.ends_with('\\')
    {
        let root = crate::paths::to_linux_raw(&format!("{upper}\\"));
        return root.is_dir().then_some(root);
    }
    let n: usize = upper.strip_prefix("PHYSICALDRIVE")?.parse().ok()?;
    let disk = disks().into_iter().nth(n)?;
    Some(sys_root().join("block").join(disk))
}

pub fn target_of_fd(fd: i32) -> Target {
    // Handle de \\.\PhysicalDriveN : le dossier sysfs du disque (le lien
    // /sys/block/X résolu vers /sys/devices/…/block/X).
    if let Ok(p) = std::fs::read_link(format!("/proc/self/fd/{fd}")) {
        let root = std::fs::canonicalize(sys_root()).unwrap_or_else(|_| sys_root());
        if p.starts_with(&root) && p.join("queue").is_dir() && p.join("size").is_file() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                return Target::Disk(name.to_string());
            }
        }
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } == 0 {
        let (major, minor) = (libc::major(st.st_dev), libc::minor(st.st_dev));
        if let Some(t) = target_of_dev(major, minor) {
            return t;
        }
    }
    let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
    let bytes = if unsafe { libc::fstatvfs(fd, &mut vfs) } == 0 {
        vfs.f_blocks as u64 * vfs.f_frsize as u64
    } else {
        0
    };
    Target::Virtual { bytes }
}

pub fn target_of_dev(major: u32, minor: u32) -> Option<Target> {
    let root = sys_root();
    let link = std::fs::canonicalize(root.join(format!("dev/block/{major}:{minor}"))).ok()?;
    let name = link.file_name()?.to_str()?.to_string();
    let class = root.join("class/block").join(&name);
    if class.join("partition").exists() || link.join("partition").exists() {
        let disk = link.parent()?.file_name()?.to_str()?.to_string();
        Some(Target::Partition { disk, part: name })
    } else {
        Some(Target::Disk(name))
    }
}

fn disk_dir(disk: &str) -> PathBuf {
    sys_root().join("block").join(disk)
}
fn part_dir(disk: &str, part: &str) -> PathBuf {
    let d = disk_dir(disk).join(part);
    if d.exists() {
        d
    } else {
        sys_root().join("class/block").join(part)
    }
}

/// Taille en octets d'un disque ou d'une partition.
fn size_of(t: &Target) -> u64 {
    match t {
        Target::Disk(d) => read_u64(&disk_dir(d).join("size")) * 512,
        Target::Partition { disk, part } => read_u64(&part_dir(disk, part).join("size")) * 512,
        Target::Virtual { bytes } => *bytes,
    }
}
fn disk_of(t: &Target) -> Option<&str> {
    match t {
        Target::Disk(d) | Target::Partition { disk: d, .. } => Some(d),
        Target::Virtual { .. } => None,
    }
}
fn sector_size(t: &Target) -> u32 {
    disk_of(t)
        .map(|d| read_u64(&disk_dir(d).join("queue/logical_block_size")) as u32)
        .filter(|s| *s >= 512)
        .unwrap_or(512)
}
fn disk_number(t: &Target) -> u32 {
    disk_of(t)
        .and_then(|d| disks().iter().position(|x| x == d))
        .unwrap_or(0) as u32
}

/// Fiche d'un disque : fabricant, modèle, révision, numéro de série, bus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiskInfo {
    pub vendor: String,
    pub model: String,
    pub revision: String,
    pub serial: String,
    pub bus: u32,
    pub removable: bool,
    pub rotational: bool,
    pub trim: bool,
}

pub fn disk_info(t: &Target) -> DiskInfo {
    let Some(d) = disk_of(t) else {
        return DiskInfo {
            vendor: "Winlux".into(),
            model: "Disque virtuel".into(),
            revision: "1.0".into(),
            bus: 0x0E, // BusTypeVirtual
            ..Default::default()
        };
    };
    let dir = disk_dir(d);
    let dev = dir.join("device");
    let first = |names: &[&str]| {
        names
            .iter()
            .map(|n| read(&dev.join(n)))
            .find(|s| !s.is_empty())
            .unwrap_or_default()
    };
    let real = std::fs::canonicalize(&dir)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let bus = if real.contains("/usb") {
        0x07
    } else if d.starts_with("nvme") {
        0x11
    } else if d.starts_with("mmcblk") {
        0x0C
    } else if real.contains("/ata") {
        0x0B
    } else if d.starts_with("vd") || real.contains("virtio") {
        0x0E
    } else {
        0x01
    };
    DiskInfo {
        vendor: first(&["vendor"]),
        model: first(&["model", "name"]),
        revision: first(&["rev", "firmware_rev", "fwrev"]),
        serial: first(&["serial", "wwid", "vpd_pg80"])
            .trim_matches(char::from(0))
            .trim()
            .to_string(),
        bus,
        removable: read(&dir.join("removable")) == "1",
        rotational: read(&dir.join("queue/rotational")) == "1",
        trim: read_u64(&dir.join("queue/discard_max_bytes")) > 0,
    }
}

/// STORAGE_DEVICE_DESCRIPTOR : en-tête de 40 octets puis les chaînes ASCII.
pub fn device_descriptor(info: &DiskInfo) -> Vec<u8> {
    let mut v = vec![0u8; 40];
    let put = |v: &mut Vec<u8>, off: usize, s: &str| {
        if s.is_empty() {
            return;
        }
        let at = v.len() as u32;
        v[off..off + 4].copy_from_slice(&at.to_le_bytes());
        v.extend(s.bytes().filter(|b| b.is_ascii() && *b != 0));
        v.push(0);
    };
    v[8] = 0; // DIRECT_ACCESS_DEVICE
    v[10] = info.removable as u8;
    v[11] = 1; // CommandQueueing
    put(&mut v, 12, &info.vendor);
    put(&mut v, 16, &info.model);
    put(&mut v, 20, &info.revision);
    put(&mut v, 24, &info.serial);
    v[28..32].copy_from_slice(&info.bus.to_le_bytes());
    let size = v.len() as u32;
    v[0..4].copy_from_slice(&40u32.to_le_bytes()); // Version = sizeof
    v[4..8].copy_from_slice(&size.to_le_bytes());
    v
}

fn descriptor_u8(value: u8) -> Vec<u8> {
    let mut v = vec![0u8; 12];
    v[0..4].copy_from_slice(&12u32.to_le_bytes());
    v[4..8].copy_from_slice(&12u32.to_le_bytes());
    v[8] = value;
    v
}

/// Réponse d'un code de contrôle : les octets à copier, ou une erreur Win32.
pub enum Reply {
    Data(Vec<u8>),
    /// Réponse partielle acceptée (en-tête seulement, ERROR_MORE_DATA évité).
    Header(Vec<u8>),
    Error(u32),
}

pub fn control(fd: i32, code: u32, input: &[u8], out_len: usize) -> Reply {
    let le64 = |b: &[u8], at: usize| {
        b.get(at..at + 8)
            .map(|s| i64::from_le_bytes(s.try_into().unwrap()))
    };
    match code {
        IOCTL_STORAGE_QUERY_PROPERTY => {
            let (Some(id), Some(kind)) = (
                input
                    .get(0..4)
                    .map(|s| u32::from_le_bytes(s.try_into().unwrap())),
                input
                    .get(4..8)
                    .map(|s| u32::from_le_bytes(s.try_into().unwrap())),
            ) else {
                return Reply::Error(ERROR_INVALID_PARAMETER);
            };
            let t = target_of_fd(fd);
            let info = disk_info(&t);
            let data = match id {
                0 => device_descriptor(&info),
                7 => descriptor_u8(info.rotational as u8), // IncursSeekPenalty
                8 => descriptor_u8(info.trim as u8),       // TrimEnabled
                6 => {
                    // STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR (28 octets).
                    let sector = sector_size(&t);
                    let phys = disk_of(&t)
                        .map(|d| read_u64(&disk_dir(d).join("queue/physical_block_size")) as u32)
                        .filter(|s| *s >= sector)
                        .unwrap_or(sector);
                    let mut v = vec![0u8; 28];
                    for (i, x) in [28u32, 28, 1, sector, 1, phys, 0].iter().enumerate() {
                        v[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
                    }
                    v
                }
                _ => return Reply::Error(ERROR_NOT_SUPPORTED),
            };
            if kind == 1 {
                // PropertyExistsQuery
                return Reply::Data(Vec::new());
            }
            if out_len < data.len() && out_len >= 8 {
                return Reply::Header(data[..8].to_vec());
            }
            Reply::Data(data)
        }
        IOCTL_STORAGE_GET_DEVICE_NUMBER => {
            let t = target_of_fd(fd);
            let part = match &t {
                Target::Partition { disk, part } => {
                    read_u64(&part_dir(disk, part).join("partition")) as u32
                }
                _ => 0,
            };
            let mut v = Vec::with_capacity(12);
            v.extend(7u32.to_le_bytes()); // FILE_DEVICE_DISK
            v.extend(disk_number(&t).to_le_bytes());
            v.extend(part.to_le_bytes());
            Reply::Data(v)
        }
        IOCTL_STORAGE_GET_HOTPLUG_INFO => {
            let removable = disk_info(&target_of_fd(fd)).removable as u8;
            let mut v = vec![0u8; 8];
            v[0..4].copy_from_slice(&8u32.to_le_bytes());
            v[4] = removable;
            v[5] = removable;
            v[6] = removable;
            Reply::Data(v)
        }
        IOCTL_STORAGE_CHECK_VERIFY | IOCTL_STORAGE_CHECK_VERIFY2 => Reply::Data(Vec::new()),
        IOCTL_DISK_IS_WRITABLE => {
            let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstatvfs(fd, &mut vfs) } == 0 && vfs.f_flag & libc::ST_RDONLY != 0 {
                Reply::Error(ERROR_WRITE_PROTECT)
            } else {
                Reply::Data(Vec::new())
            }
        }
        IOCTL_DISK_GET_DRIVE_GEOMETRY | IOCTL_DISK_GET_DRIVE_GEOMETRY_EX => {
            let t = target_of_fd(fd);
            let disk = match &t {
                Target::Partition { disk, .. } => Target::Disk(disk.clone()),
                other => other.clone(),
            };
            let bytes = size_of(&disk);
            let sector = sector_size(&disk) as u64;
            let removable = disk_info(&disk).removable;
            let cylinders = bytes / (255 * 63 * sector);
            let mut v = Vec::with_capacity(40);
            v.extend((cylinders as i64).to_le_bytes());
            v.extend((if removable { 11u32 } else { 12u32 }).to_le_bytes());
            v.extend(255u32.to_le_bytes());
            v.extend(63u32.to_le_bytes());
            v.extend((sector as u32).to_le_bytes());
            if code == IOCTL_DISK_GET_DRIVE_GEOMETRY_EX {
                v.extend((bytes as i64).to_le_bytes());
                if out_len >= 40 {
                    // DISK_PARTITION_INFO : taille de la structure, style GPT.
                    v.extend(8u32.to_le_bytes());
                }
            }
            Reply::Data(v)
        }
        IOCTL_DISK_GET_LENGTH_INFO => {
            Reply::Data((size_of(&target_of_fd(fd)) as i64).to_le_bytes().to_vec())
        }
        IOCTL_DISK_GET_PARTITION_INFO_EX => {
            let t = target_of_fd(fd);
            let (start, number) = match &t {
                Target::Partition { disk, part } => {
                    let d = part_dir(disk, part);
                    (
                        read_u64(&d.join("start")) * 512,
                        read_u64(&d.join("partition")) as u32,
                    )
                }
                _ => (0, 0),
            };
            // PARTITION_INFORMATION_EX (144 octets), style GPT, données de base.
            let mut v = vec![0u8; 144];
            v[0..4].copy_from_slice(&1u32.to_le_bytes());
            v[8..16].copy_from_slice(&(start as i64).to_le_bytes());
            v[16..24].copy_from_slice(&(size_of(&t) as i64).to_le_bytes());
            v[24..28].copy_from_slice(&number.to_le_bytes());
            // {EBD0A0A2-B9E5-4433-87C0-68B6B72699C7} : Basic data partition.
            v[32..48].copy_from_slice(&[
                0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26,
                0x99, 0xC7,
            ]);
            Reply::Data(v)
        }
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS => {
            let t = target_of_fd(fd);
            let start = match &t {
                Target::Partition { disk, part } => {
                    read_u64(&part_dir(disk, part).join("start")) * 512
                }
                _ => 0,
            };
            let mut v = vec![0u8; 32];
            v[0..4].copy_from_slice(&1u32.to_le_bytes());
            v[8..12].copy_from_slice(&disk_number(&t).to_le_bytes());
            v[16..24].copy_from_slice(&(start as i64).to_le_bytes());
            v[24..32].copy_from_slice(&(size_of(&t) as i64).to_le_bytes());
            Reply::Data(v)
        }
        // Verrouiller, démonter : Winlux ne prête jamais un volume monté.
        FSCTL_LOCK_VOLUME | FSCTL_UNLOCK_VOLUME | FSCTL_DISMOUNT_VOLUME => {
            Reply::Error(ERROR_ACCESS_DENIED)
        }
        FSCTL_GET_COMPRESSION => Reply::Data(0u16.to_le_bytes().to_vec()),
        // Accepté, le fichier reste tel quel (pas de compression NTFS).
        FSCTL_SET_COMPRESSION if input.len() >= 2 => Reply::Data(Vec::new()),
        FSCTL_SET_COMPRESSION => Reply::Error(ERROR_INVALID_PARAMETER),
        FSCTL_SET_SPARSE => Reply::Data(Vec::new()),
        FSCTL_GET_OBJECT_ID | FSCTL_CREATE_OR_GET_OBJECT_ID => {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstat(fd, &mut st) } != 0 {
                return Reply::Error(ERROR_INVALID_FUNCTION);
            }
            // FILE_OBJECTID_BUFFER : identifiant stable tiré de (st_dev, st_ino).
            let mut v = vec![0u8; 64];
            v[0..8].copy_from_slice(&(st.st_ino as u64).to_le_bytes());
            v[8..16].copy_from_slice(&(st.st_dev as u64).to_le_bytes());
            v[16..32].copy_from_slice(&crate::dlls::bcrypt::sha256(b"winlux-volume")[..16]);
            v.copy_within(0..16, 32); // BirthObjectId = ObjectId
            Reply::Data(v)
        }
        FSCTL_SET_ZERO_DATA => {
            let (Some(from), Some(to)) = (le64(input, 0), le64(input, 8)) else {
                return Reply::Error(ERROR_INVALID_PARAMETER);
            };
            if from < 0 || to < from {
                return Reply::Error(ERROR_INVALID_PARAMETER);
            }
            if to > from && !zero_range(fd, from, to - from) {
                return Reply::Error(ERROR_NOT_SUPPORTED);
            }
            Reply::Data(Vec::new())
        }
        FSCTL_QUERY_ALLOCATED_RANGES => {
            let (Some(from), Some(len)) = (le64(input, 0), le64(input, 8)) else {
                return Reply::Error(ERROR_INVALID_PARAMETER);
            };
            if from < 0 || len < 0 {
                return Reply::Error(ERROR_INVALID_PARAMETER);
            }
            let ranges = allocated_ranges(fd, from, from.saturating_add(len));
            let mut v = Vec::with_capacity(ranges.len() * 16);
            for (a, b) in ranges {
                v.extend(a.to_le_bytes());
                v.extend((b - a).to_le_bytes());
            }
            if v.len() > out_len {
                let fit = out_len / 16 * 16;
                v.truncate(fit);
                return Reply::Header(v); // ERROR_MORE_DATA, comme Windows
            }
            Reply::Data(v)
        }
        _ => Reply::Error(ERROR_NOT_SUPPORTED),
    }
}

/// Une partie de la réponse : `ERROR_MORE_DATA` pour les plages,
/// succès pour l'en-tête de STORAGE_QUERY_PROPERTY.
pub fn header_error(code: u32) -> u32 {
    if code == FSCTL_QUERY_ALLOCATED_RANGES {
        ERROR_MORE_DATA
    } else {
        0
    }
}
pub const ERROR_BUFFER: u32 = ERROR_INSUFFICIENT_BUFFER;

fn zero_range(fd: i32, from: i64, len: i64) -> bool {
    let keep = libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE;
    if unsafe { libc::fallocate(fd, keep, from, len) } == 0 {
        return true;
    }
    // Système de fichiers sans trous : on écrit des zéros jusqu'à la fin
    // actuelle du fichier (Windows ne l'agrandit pas non plus).
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return false;
    }
    let end = (from + len).min(st.st_size);
    let zeros = [0u8; 65536];
    let mut at = from;
    while at < end {
        let n = ((end - at) as usize).min(zeros.len());
        let w = unsafe { libc::pwrite(fd, zeros.as_ptr() as *const _, n, at) };
        if w <= 0 {
            return false;
        }
        at += w as i64;
    }
    true
}

/// Plages contenant des données entre `from` et `to` (SEEK_DATA/SEEK_HOLE).
fn allocated_ranges(fd: i32, from: i64, to: i64) -> Vec<(i64, i64)> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Vec::new();
    }
    let to = to.min(st.st_size);
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        let data = unsafe { libc::lseek(fd, at, libc::SEEK_DATA) };
        if data < 0 {
            if out.is_empty()
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL)
            {
                // SEEK_DATA non géré : tout est alloué.
                out.push((from, to));
            }
            break;
        }
        if data >= to {
            break;
        }
        let hole = unsafe { libc::lseek(fd, data, libc::SEEK_HOLE) }.max(data);
        out.push((data, hole.min(to)));
        at = hole;
        if hole <= data {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descripteur_de_disque() {
        let info = DiskInfo {
            vendor: "ATA".into(),
            model: "Samsung SSD 870".into(),
            revision: "SVT02B6Q".into(),
            serial: "S5Y1NX0T".into(),
            bus: 0x0B,
            ..Default::default()
        };
        let d = device_descriptor(&info);
        let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize;
        assert_eq!(u32_at(4), d.len());
        let s = |o: usize| {
            let at = u32_at(o);
            let end = d[at..].iter().position(|b| *b == 0).unwrap() + at;
            String::from_utf8(d[at..end].to_vec()).unwrap()
        };
        assert_eq!(s(12), "ATA");
        assert_eq!(s(16), "Samsung SSD 870");
        assert_eq!(s(24), "S5Y1NX0T");
        assert_eq!(u32_at(28), 0x0B);
    }

    #[test]
    fn plages_allouees_d_un_fichier_creux() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::unix::io::AsRawFd;
        let dir = std::env::temp_dir().join(format!("winlux-devio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("creux.bin");
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&p)
            .unwrap();
        f.write_all(&[1u8; 4096]).unwrap();
        f.seek(SeekFrom::Start(8 << 20)).unwrap();
        f.write_all(&[2u8; 4096]).unwrap();
        let r = allocated_ranges(f.as_raw_fd(), 0, 16 << 20);
        assert!(!r.is_empty());
        assert_eq!(r[0].0, 0);
        assert_eq!(r.last().unwrap().1, (8 << 20) + 4096);
        assert!(zero_range(f.as_raw_fd(), 0, 4096));
        let mut data = vec![9u8; 4096];
        use std::os::unix::fs::FileExt;
        f.read_at(&mut data, 0).unwrap();
        assert!(data.iter().all(|b| *b == 0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
