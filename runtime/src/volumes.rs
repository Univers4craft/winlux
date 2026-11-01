//! Disques branchés (clés USB, disques externes, disque interne d'un Windows
//! existant) montés automatiquement avec une lettre de lecteur, comme sous
//! Windows : NTFS en lecture et écriture, FAT32, exFAT et ext4.
//!
//! Le service `winlux --volumes daemon` (root) surveille `/sys/class/block`
//! toutes les deux secondes :
//! - reconnaît le système de fichiers lui-même (secteur d'amorçage NTFS,
//!   FAT, exFAT, superbloc ext, signature BitLocker) et la table de partitions
//!   (GPT ou MBR) pour ignorer EFI, partitions réservées, récupération, swap
//!   et les partitions marquées « pas de lettre » par Windows ;
//! - lit l'état NTFS (`$Volume` : indicateur « sale ») et, après un montage en
//!   lecture seule, `hiberfil.sys` (veille prolongée ou démarrage rapide) :
//!   un volume que Windows n'a pas fermé reste en lecture seule et
//!   l'utilisateur reçoit un message clair ;
//! - monte sous `/media/<utilisateur>/<nom>` avec le propriétaire de la
//!   session active (`uid`/`gid`, `umask=022`, noms compatibles Windows) et
//!   publie la lettre dans le registre partagé `mounts.tsv` (type `disk`) ;
//! - redonne la même lettre au même volume (numéro de série), comme Windows ;
//! - retire la lettre quand le disque est débranché.
//!
//! Le pilote NTFS est `ntfs3` (noyau) s'il existe, sinon `ntfs-3g` ; sans
//! aucun des deux, les volumes NTFS ne sont pas proposés.

use crate::admin;
use crate::mounts::{self, Mount};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CONFIG: &str = "etc/winlux/volumes.conf";
const LETTERS: &str = "var/lib/winlux/volumes/letters.tsv";
pub const RUN_DIR: &str = "run/winlux/volumes";

// ---------- Systèmes de fichiers ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fs {
    Ntfs,
    Fat,
    Exfat,
    Ext,
    /// Volume chiffré BitLocker : reconnu pour l'expliquer, jamais monté.
    BitLocker,
}

/// Ce que le début d'un volume nous apprend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub fs: Fs,
    /// Numéro de série du volume (sert à garder la même lettre).
    pub serial: String,
    pub label: String,
}

fn le16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .unwrap_or(0)
}

fn le32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0)
}

fn le64(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8])))
        .unwrap_or(0)
}

fn ascii_label(b: &[u8]) -> String {
    let s: String = b
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| {
            if c.is_ascii_graphic() || c == b' ' {
                c as char
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.eq_ignore_ascii_case("NO NAME") {
        String::new()
    } else {
        s
    }
}

/// Reconnaît le système de fichiers à partir des 4 premiers Kio du volume.
pub fn probe(head: &[u8]) -> Option<Probe> {
    if head.len() < 2048 {
        return None;
    }
    let oem = &head[3..11];
    if oem == b"-FVE-FS-" {
        return Some(Probe {
            fs: Fs::BitLocker,
            serial: String::new(),
            label: String::new(),
        });
    }
    if oem == b"NTFS    " {
        return Some(Probe {
            fs: Fs::Ntfs,
            serial: format!("{:016X}", le64(head, 0x48)),
            label: String::new(),
        });
    }
    if oem == b"EXFAT   " {
        return Some(Probe {
            fs: Fs::Exfat,
            serial: format!("{:08X}", le32(head, 0x64)),
            label: String::new(),
        });
    }
    let boot_sig = head[510] == 0x55 && head[511] == 0xAA;
    if boot_sig && &head[82..90] == b"FAT32   " {
        return Some(Probe {
            fs: Fs::Fat,
            serial: format!("{:08X}", le32(head, 0x43)),
            label: ascii_label(&head[0x47..0x52]),
        });
    }
    if boot_sig && (&head[54..62] == b"FAT16   " || &head[54..62] == b"FAT12   ") {
        return Some(Probe {
            fs: Fs::Fat,
            serial: format!("{:08X}", le32(head, 0x27)),
            label: ascii_label(&head[0x2B..0x36]),
        });
    }
    if le16(head, 1024 + 0x38) == 0xEF53 {
        let uuid = &head[1024 + 0x68..1024 + 0x78];
        return Some(Probe {
            fs: Fs::Ext,
            serial: uuid.iter().map(|b| format!("{b:02X}")).collect(),
            label: ascii_label(&head[1024 + 0x78..1024 + 0x88]),
        });
    }
    None
}

// ---------- État d'un volume NTFS ($Volume) ----------

/// Géométrie NTFS lue dans le secteur d'amorçage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtfsGeometry {
    pub bytes_per_sector: u64,
    pub cluster: u64,
    pub mft_offset: u64,
    pub record_size: u64,
}

pub fn ntfs_geometry(boot: &[u8]) -> Option<NtfsGeometry> {
    let bps = le16(boot, 0x0B) as u64;
    if !(256..=4096).contains(&bps) || !bps.is_power_of_two() {
        return None;
    }
    let spc = boot[0x0D];
    let spc = if spc > 0x80 {
        1u64 << (256 - spc as u32).min(31)
    } else {
        spc as u64
    };
    let cluster = bps * spc;
    if cluster == 0 {
        return None;
    }
    let cpr = boot[0x40] as i8;
    let record_size = if cpr < 0 {
        1u64 << (-(cpr as i32)).min(20)
    } else {
        cpr as u64 * cluster
    };
    if !(512..=65536).contains(&record_size) {
        return None;
    }
    Some(NtfsGeometry {
        bytes_per_sector: bps,
        cluster,
        mft_offset: le64(boot, 0x30).checked_mul(cluster)?,
        record_size,
    })
}

/// Informations du fichier `$Volume` (enregistrement 3 de la MFT).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NtfsVolume {
    pub label: String,
    pub dirty: bool,
}

/// Décode un enregistrement MFT de `$Volume` (corrige les « fixups »).
pub fn parse_volume_record(raw: &[u8], bytes_per_sector: u64) -> Option<NtfsVolume> {
    if raw.len() < 512 || &raw[0..4] != b"FILE" {
        return None;
    }
    let mut rec = raw.to_vec();
    let usa = le16(&rec, 4) as usize;
    let count = le16(&rec, 6) as usize;
    let bps = bytes_per_sector as usize;
    for i in 1..count {
        let end = i * bps;
        let fix = usa + 2 * i;
        if end > rec.len() || fix + 2 > rec.len() {
            return None;
        }
        rec[end - 2] = rec[fix];
        rec[end - 1] = rec[fix + 1];
    }
    let mut vol = NtfsVolume::default();
    let mut off = le16(&rec, 0x14) as usize;
    let mut seen = false;
    while off + 16 <= rec.len() {
        let kind = le32(&rec, off);
        let len = le32(&rec, off + 4) as usize;
        if kind == 0xFFFF_FFFF || len < 16 || off + len > rec.len() {
            break;
        }
        // Attributs résidents seulement (c'est toujours le cas ici).
        if rec[off + 8] == 0 {
            let vlen = le32(&rec, off + 0x10) as usize;
            let voff = off + le16(&rec, off + 0x14) as usize;
            if let Some(value) = rec.get(voff..voff + vlen) {
                match kind {
                    0x60 => {
                        let units: Vec<u16> = value
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|c| u16::from_le_bytes(*c))
                            .collect();
                        vol.label = String::from_utf16_lossy(&units);
                    }
                    0x70 if value.len() >= 12 => {
                        vol.dirty = le16(value, 10) & 1 != 0;
                        seen = true;
                    }
                    _ => {}
                }
            }
        }
        off += len;
    }
    seen.then_some(vol)
}

/// `hiberfil.sys` commence par `hibr`/`HIBR` tant qu'une session Windows
/// est en veille prolongée ou fermée par le démarrage rapide.
pub fn hibernated(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4].eq_ignore_ascii_case(b"hibr")
}

// ---------- Tables de partitions ----------

/// Rôle d'une partition, d'après son type GPT ou MBR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Partition de données : reçoit une lettre.
    Data,
    /// Partition Linux : lettre seulement sur un disque amovible.
    Linux,
    /// EFI, réservée Microsoft, récupération, swap, « sans lettre »…
    Hidden,
}

/// GUID GPT sous sa forme texte → octets sur disque (moitié little-endian).
fn guid(s: &str) -> [u8; 16] {
    let hex: Vec<u8> = s
        .bytes()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("0"), 16).unwrap_or(0))
        .collect();
    let mut g = [0u8; 16];
    if hex.len() == 16 {
        g[0..4].copy_from_slice(&[hex[3], hex[2], hex[1], hex[0]]);
        g[4..6].copy_from_slice(&[hex[5], hex[4]]);
        g[6..8].copy_from_slice(&[hex[7], hex[6]]);
        g[8..16].copy_from_slice(&hex[8..16]);
    }
    g
}

const GPT_BASIC_DATA: &str = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7";
const GPT_LINUX_DATA: &str = "0FC63DAF-8483-4772-8E79-3D69D8477DE4";

/// Rôle de la partition qui commence au secteur `start`, à partir des
/// premiers secteurs du disque (MBR, en-tête et entrées GPT).
pub fn partition_role(disk: &[u8], sector: usize, start: u64) -> Role {
    if disk.len() < 512 || disk[510] != 0x55 || disk[511] != 0xAA {
        // Disque sans table (« superfloppy ») : le volume couvre tout le disque.
        return Role::Data;
    }
    let mbr_type = |i: usize| disk[446 + 16 * i + 4];
    let protective = (0..4).any(|i| mbr_type(i) == 0xEE);
    if protective {
        let h = sector;
        if disk.len() < h + 92 || &disk[h..h + 8] != b"EFI PART" {
            return Role::Hidden;
        }
        let entries = le64(disk, h + 72) as usize * sector;
        let count = le32(disk, h + 80) as usize;
        let size = le32(disk, h + 84) as usize;
        if size < 128 {
            return Role::Hidden;
        }
        for i in 0..count.min(256) {
            let e = entries + i * size;
            let Some(entry) = disk.get(e..e + size) else {
                break;
            };
            if le64(entry, 32) != start {
                continue;
            }
            let attrs = le64(entry, 48);
            // Bits 0 (plateforme), 62 (cachée), 63 (pas de lettre de lecteur).
            if attrs & (1 | 1 << 62 | 1 << 63) != 0 {
                return Role::Hidden;
            }
            let t = &entry[0..16];
            return if t == guid(GPT_BASIC_DATA) {
                Role::Data
            } else if t == guid(GPT_LINUX_DATA) {
                Role::Linux
            } else {
                Role::Hidden
            };
        }
        return Role::Hidden;
    }
    for i in 0..4 {
        let e = 446 + 16 * i;
        let kind = disk[e + 4];
        let lba = le32(disk, e + 8) as u64;
        if kind == 0 || lba != start {
            continue;
        }
        return match kind {
            0x01 | 0x04 | 0x06 | 0x07 | 0x0B | 0x0C | 0x0E => Role::Data,
            0x83 => Role::Linux,
            _ => Role::Hidden,
        };
    }
    // Partition logique d'une partition étendue : on se fie au contenu.
    Role::Data
}

// ---------- Périphériques ----------

/// Volume vu dans `/sys/class/block`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// `sdb1`, `nvme0n1p3`…
    pub name: String,
    /// Disque parent (`sdb`) ; le volume lui-même s'il n'a pas de table.
    pub disk: String,
    /// `maj:min`.
    pub dev: String,
    pub removable: bool,
    pub start: u64,
    pub model: String,
}

fn sys_text(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn wanted_disk(name: &str) -> bool {
    // `WINLUX_VOLUMES_EXTRA=loop` : périphériques en plus, pour les tests en VM.
    let extra = std::env::var("WINLUX_VOLUMES_EXTRA").unwrap_or_default();
    ["sd", "nvme", "mmcblk", "vd", "hd", "xvd"]
        .iter()
        .copied()
        .chain(extra.split(',').filter(|p| !p.is_empty()))
        .any(|p| name.starts_with(p))
}

/// Volumes candidats : partitions des disques physiques, ou disque entier
/// sans partition, qui ne servent pas déjà (montés, LVM, chiffrés…).
pub fn devices() -> Vec<Device> {
    let class = admin::path("sys/class/block");
    let Ok(dir) = std::fs::read_dir(&class) else {
        return Vec::new();
    };
    let mut names: Vec<String> = dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| wanted_disk(n))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in &names {
        let base = class.join(name);
        let is_part = base.join("partition").exists();
        let disk = if is_part {
            std::fs::canonicalize(&base)
                .ok()
                .and_then(|p| {
                    p.parent()
                        .and_then(|d| d.file_name())
                        .map(|d| d.to_string_lossy().into_owned())
                })
                .unwrap_or_default()
        } else {
            // Un disque qui a des partitions n'est pas un volume.
            if has_partitions(&base) {
                continue;
            }
            name.clone()
        };
        if disk.is_empty() || sys_text(&base.join("size")) == "0" {
            continue;
        }
        let holders = std::fs::read_dir(base.join("holders"))
            .map(|d| d.count())
            .unwrap_or(0);
        if holders > 0 {
            continue;
        }
        let disk_dir = class.join(&disk);
        let path = std::fs::canonicalize(&disk_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let removable = sys_text(&disk_dir.join("removable")) == "1" || path.contains("/usb");
        let vendor = sys_text(&disk_dir.join("device/vendor"));
        let model = sys_text(&disk_dir.join("device/model"));
        let model = if vendor.is_empty() || model.starts_with(&vendor) {
            model
        } else {
            format!("{vendor} {model}")
        };
        out.push(Device {
            name: name.clone(),
            disk,
            dev: sys_text(&base.join("dev")),
            removable,
            start: if is_part {
                sys_text(&base.join("start")).parse().unwrap_or(0)
            } else {
                0
            },
            model,
        });
    }
    out
}

fn has_partitions(disk: &Path) -> bool {
    std::fs::read_dir(disk)
        .map(|d| d.flatten().any(|e| e.path().join("partition").exists()))
        .unwrap_or(false)
}

/// `maj:min` de tous les périphériques montés.
fn mounted_devs() -> BTreeSet<String> {
    admin::read_text(&admin::path("proc/self/mountinfo"))
        .map(|t| {
            t.lines()
                .filter_map(|l| l.split(' ').nth(2).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn read_at(path: &Path, offset: u64, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        match f.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(_) => return None,
        }
    }
    buf.truncate(got);
    Some(buf)
}

fn dev_node(name: &str) -> PathBuf {
    admin::path(&format!("dev/{name}"))
}

fn sector_size(disk: &str) -> usize {
    sys_text(&admin::path(&format!(
        "sys/class/block/{disk}/queue/logical_block_size"
    )))
    .parse::<usize>()
    .ok()
    .filter(|s| (512..=4096).contains(s))
    .unwrap_or(512)
}

/// Lit l'état NTFS (`$Volume`) directement sur le périphérique.
pub fn ntfs_volume(node: &Path, boot: &[u8]) -> Option<NtfsVolume> {
    let g = ntfs_geometry(boot)?;
    let raw = read_at(
        node,
        g.mft_offset + 3 * g.record_size,
        g.record_size as usize,
    )?;
    parse_volume_record(&raw, g.bytes_per_sector)
}

// ---------- Pilotes et montage ----------

fn find_tool(names: &[&str]) -> Option<PathBuf> {
    crate::archive::tools::find(names)
}

fn kernel_fs(name: &str) -> bool {
    let listed = |t: &str| t.lines().any(|l| l.split_whitespace().last() == Some(name));
    if admin::read_text(&admin::path("proc/filesystems")).is_some_and(|t| listed(&t)) {
        return true;
    }
    if unsafe { libc::geteuid() } != 0 {
        return false;
    }
    let ok = find_tool(&["modprobe"]).is_some_and(|m| {
        Command::new(m)
            .args(["-q", name])
            .status()
            .is_ok_and(|s| s.success())
    });
    ok && admin::read_text(&admin::path("proc/filesystems")).is_some_and(|t| listed(&t))
}

/// Pilote utilisé pour un système de fichiers ; `None` = pas proposé.
pub fn driver(fs: Fs) -> Option<&'static str> {
    match fs {
        Fs::Ntfs => {
            if kernel_fs("ntfs3") {
                Some("ntfs3")
            } else if find_tool(&["ntfs-3g", "mount.ntfs-3g"]).is_some() {
                Some("ntfs-3g")
            } else {
                None
            }
        }
        Fs::Fat => kernel_fs("vfat").then_some("vfat"),
        Fs::Exfat => kernel_fs("exfat").then_some("exfat"),
        Fs::Ext => kernel_fs("ext4").then_some("ext4"),
        Fs::BitLocker => None,
    }
}

/// Options de montage : propriétaire = utilisateur de la session, les
/// autres comptes lisent ; noms de fichiers interdits par Windows refusés.
pub fn mount_options(
    driver: &str,
    uid: u32,
    gid: u32,
    read_only: bool,
    windows_names: bool,
) -> String {
    let mut o = vec![if read_only { "ro" } else { "rw" }.to_string()];
    o.push("nosuid".into());
    o.push("nodev".into());
    match driver {
        "ntfs3" | "ntfs-3g" => {
            o.push(format!("uid={uid}"));
            o.push(format!("gid={gid}"));
            o.push("umask=022".into());
            if windows_names {
                o.push("windows_names".into());
            }
            if driver == "ntfs-3g" && !read_only {
                o.push("big_writes".into());
            }
        }
        "vfat" => {
            o.push(format!("uid={uid}"));
            o.push(format!("gid={gid}"));
            o.push("umask=022".into());
            o.push("iocharset=utf8".into());
            o.push("shortname=mixed".into());
            o.push("flush".into());
        }
        "exfat" => {
            o.push(format!("uid={uid}"));
            o.push(format!("gid={gid}"));
            o.push("umask=022".into());
            o.push("iocharset=utf8".into());
        }
        _ => {}
    }
    o.join(",")
}

fn run_mount(driver: &str, node: &Path, mp: &Path, opts: &str) -> Result<(), String> {
    let out = if driver == "ntfs-3g" {
        let tool = find_tool(&["ntfs-3g", "mount.ntfs-3g"]).ok_or("ntfs-3g absent")?;
        Command::new(tool)
            .arg(node)
            .arg(mp)
            .args(["-o", opts])
            .output()
    } else {
        let tool = find_tool(&["mount"]).ok_or("mount absent")?;
        Command::new(tool)
            .args(["-t", driver, "-o", opts])
            .arg(node)
            .arg(mp)
            .output()
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Monte en essayant d'abord avec `windows_names` (refusé par les vieux
/// noyaux), puis sans.
fn mount_volume(
    driver: &str,
    node: &Path,
    mp: &Path,
    uid: u32,
    gid: u32,
    ro: bool,
) -> Result<(), String> {
    let first = run_mount(driver, node, mp, &mount_options(driver, uid, gid, ro, true));
    if first.is_ok() || !matches!(driver, "ntfs3" | "ntfs-3g") {
        return first;
    }
    run_mount(
        driver,
        node,
        mp,
        &mount_options(driver, uid, gid, ro, false),
    )
}

fn unmount(mp: &Path, lazy: bool) -> bool {
    let Some(tool) = find_tool(&["umount"]) else {
        return false;
    };
    let mut c = Command::new(tool);
    if lazy {
        c.arg("-l");
    }
    c.arg(mp)
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Décode les échappements octaux de /proc/self/mounts (`\040` espace,
/// `\011` tabulation, `\012` retour à la ligne, `\134` barre oblique).
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_mounted(mp: &Path) -> bool {
    let target = mp.to_string_lossy();
    admin::read_text(&admin::path("proc/self/mounts"))
        .map(|t| {
            t.lines().any(|l| {
                l.split(' ')
                    .nth(1)
                    .is_some_and(|m| unescape_mount(m) == target)
            })
        })
        .unwrap_or(false)
}

// ---------- Session, lettres et messages ----------

/// Utilisateur de la session active (`/run/winlux/sessions.txt`), sinon root.
pub fn session_owner() -> (String, u32, u32) {
    let active = admin::read_text(&admin::path("run/winlux/sessions.txt")).and_then(|t| {
        t.lines().find_map(|l| {
            let f: Vec<&str> = l.strip_prefix("session=")?.split('|').collect();
            (f.get(3) == Some(&"active")).then(|| f[0].to_string())
        })
    });
    if let Some(user) = active {
        if let Some(passwd) = admin::read_text(&admin::path("etc/passwd")) {
            for l in passwd.lines() {
                let f: Vec<&str> = l.split(':').collect();
                if f.len() > 3 && f[0] == user {
                    if let (Ok(u), Ok(g)) = (f[2].parse(), f[3].parse()) {
                        return (user, u, g);
                    }
                }
            }
        }
    }
    ("root".into(), 0, 0)
}

/// Lettre déjà donnée à ce volume (même numéro de série).
fn remembered_letter(serial: &str) -> Option<char> {
    if serial.is_empty() {
        return None;
    }
    admin::read_text(&admin::path(LETTERS))?
        .lines()
        .find_map(|l| {
            let (s, letter) = l.split_once('\t')?;
            (s == serial).then(|| letter.chars().next())?
        })
}

fn remember_letter(serial: &str, letter: char) {
    if serial.is_empty() {
        return;
    }
    let path = admin::path(LETTERS);
    let mut lines: Vec<String> = admin::read_text(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with(&format!("{serial}\t")))
        .map(str::to_string)
        .collect();
    lines.push(format!("{serial}\t{letter}"));
    // Les 64 volumes les plus récents suffisent.
    let skip = lines.len().saturating_sub(64);
    let _ = admin::write_text(&path, &(lines[skip..].join("\n") + "\n"));
}

/// Nom de dossier sûr pour `/media/<utilisateur>/<nom>`.
pub fn folder_name(label: &str, fallback: &str) -> String {
    let clean: String = label
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() {
        fallback.to_string()
    } else {
        clean
    }
}

/// Raisons d'un montage en lecture seule, gardées dans la colonne `backend`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Mounted,
    Hibernated,
    Dirty,
    BitLocker,
    Repaired,
    /// Le système de fichiers est reconnu mais refuse de se monter.
    Failed,
}

impl Notice {
    pub fn code(self) -> &'static str {
        match self {
            Notice::Mounted => "mounted",
            Notice::Hibernated => "hibernated",
            Notice::Dirty => "dirty",
            Notice::BitLocker => "bitlocker",
            Notice::Repaired => "repaired",
            Notice::Failed => "failed",
        }
    }
}

/// Message pour `notify-send`, dans la langue du système.
pub fn notice_text(n: Notice, name: &str, lang: &str) -> String {
    let l = match lang.get(..2) {
        Some("en") => 1,
        Some("es") => 2,
        Some("de") => 3,
        _ => 0,
    };
    let t: [&str; 4] = match n {
        Notice::Mounted => [
            "{} est prêt.",
            "{} is ready.",
            "{} está listo.",
            "{} ist bereit.",
        ],
        Notice::Hibernated => [
            "Windows n'a pas été arrêté complètement (démarrage rapide ou veille prolongée) : {} est ouvert en lecture seule. Arrêtez Windows en maintenant Maj pour pouvoir y écrire.",
            "Windows was not fully shut down (Fast Startup or hibernation): {} is open read-only. Shut down Windows while holding Shift to write to it.",
            "Windows no se apagó por completo (inicio rápido o hibernación): {} se abre en solo lectura. Apague Windows manteniendo Mayús para poder escribir.",
            "Windows wurde nicht vollständig heruntergefahren (Schnellstart oder Ruhezustand): {} ist schreibgeschützt geöffnet. Fahren Sie Windows mit gedrückter Umschalttaste herunter, um darauf zu schreiben.",
        ],
        Notice::Dirty => [
            "{} n'a pas été fermé proprement : ouvert en lecture seule pour protéger vos fichiers. Réparez-le dans Gestion des disques.",
            "{} was not closed properly: opened read-only to protect your files. Repair it in Disk Management.",
            "{} no se cerró correctamente: se abre en solo lectura para proteger sus archivos. Repárelo en Administración de discos.",
            "{} wurde nicht ordnungsgemäß getrennt: zum Schutz Ihrer Dateien schreibgeschützt geöffnet. Reparieren Sie es in der Datenträgerverwaltung.",
        ],
        Notice::BitLocker => [
            "{} est chiffré avec BitLocker : Winlux ne peut pas l'ouvrir.",
            "{} is encrypted with BitLocker: Winlux cannot open it.",
            "{} está cifrado con BitLocker: Winlux no puede abrirlo.",
            "{} ist mit BitLocker verschlüsselt: Winlux kann es nicht öffnen.",
        ],
        Notice::Failed => [
            "{} n'a pas pu être ouvert : il est peut-être endommagé. Vérifiez-le depuis Windows (chkdsk /f).",
            "{} could not be opened: it may be damaged. Check it from Windows (chkdsk /f).",
            "No se pudo abrir {}: puede estar dañado. Compruébelo desde Windows (chkdsk /f).",
            "{} konnte nicht geöffnet werden: Es ist möglicherweise beschädigt. Prüfen Sie es unter Windows (chkdsk /f).",
        ],
        Notice::Repaired => [
            "{} a été réparé et peut de nouveau être modifié.",
            "{} was repaired and can be written to again.",
            "{} se reparó y se puede volver a modificar.",
            "{} wurde repariert und ist wieder beschreibbar.",
        ],
    };
    t[l].replace("{}", name)
}

fn system_lang() -> String {
    if let Ok(l) = std::env::var("LANG") {
        if !l.is_empty() && l != "C" && !l.starts_with("C.") {
            return l;
        }
    }
    admin::read_text(&admin::path("etc/default/locale"))
        .and_then(|t| admin::field(&t, "LANG").map(|v| v.trim_matches('"').to_string()))
        .unwrap_or_else(|| "fr_FR.UTF-8".into())
}

/// Publie un message : fichier lu par le bureau Winlux (texte traduit par
/// lui) et `notify-send` pour les bureaux Linux.
pub fn post_notice(n: Notice, letter: Option<char>, label: &str) {
    let path = admin::path(RUN_DIR).join("notices.txt");
    let seq = admin::read_text(&path)
        .and_then(|t| {
            t.lines()
                .last()
                .and_then(|l| l.split('\t').next()?.parse::<u64>().ok())
        })
        .unwrap_or(0)
        + 1;
    let l = letter.map(|c| c.to_string()).unwrap_or_default();
    admin::append_line(
        &path,
        &format!(
            "{seq}\t{}\t{l}\t{}",
            n.code(),
            label.replace(['\t', '\n'], " ")
        ),
        8 * 1024,
    );
    let name = match letter {
        Some(c) if !label.is_empty() => format!("{label} ({c}:)"),
        Some(c) => format!("{c}:"),
        None => label.to_string(),
    };
    if let Ok(mut child) = Command::new("notify-send")
        .args(["Winlux", &notice_text(n, &name, &system_lang())])
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

// ---------- Service ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub automount: bool,
    /// Disques internes (partition Windows d'un double démarrage…).
    pub internal: bool,
}

impl Config {
    pub fn load() -> Config {
        let t = admin::read_text(&admin::path(CONFIG)).unwrap_or_default();
        let on = |k: &str| admin::field(&t, k) != Some("0");
        Config {
            automount: on("automount"),
            internal: on("internal"),
        }
    }
}

/// Volumes que l'utilisateur a éjectés : pas remontés tant qu'ils restent
/// branchés.
fn ejected_file() -> PathBuf {
    admin::path(RUN_DIR).join("ejected")
}

fn ejected() -> BTreeSet<String> {
    admin::read_text(&ejected_file())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn set_ejected(set: &BTreeSet<String>) {
    let text: String = set.iter().map(|s| format!("{s}\n")).collect();
    let _ = admin::write_text(&ejected_file(), &text);
}

/// Monte un volume et publie sa lettre. Renvoie la lettre attribuée.
fn attach(d: &Device, cfg: &Config, owner: &(String, u32, u32)) -> Option<char> {
    if !d.removable && !cfg.internal {
        return None;
    }
    let node = dev_node(&d.name);
    let head = read_at(&node, 0, 4096)?;
    let probe = probe(&head)?;
    if d.disk != d.name {
        let sector = sector_size(&d.disk);
        let disk = read_at(&dev_node(&d.disk), 0, sector * 34)?;
        // `start` est toujours en secteurs de 512 octets dans sysfs.
        let start = d.start * 512 / sector as u64;
        match partition_role(&disk, sector, start) {
            Role::Data => {}
            Role::Linux if d.removable => {}
            _ => return None,
        }
    }
    if probe.fs == Fs::BitLocker {
        post_notice(Notice::BitLocker, None, &d.model);
        return None;
    }
    let driver = driver(probe.fs)?;
    let mut label = probe.label.clone();
    let mut dirty = false;
    if probe.fs == Fs::Ntfs {
        let vol = ntfs_volume(&node, &head);
        if let Some(v) = &vol {
            label = v.label.clone();
            dirty = v.dirty;
        }
    }
    if label.is_empty() {
        label = d.model.clone();
    }
    let letter_hint = remembered_letter(&probe.serial);
    let base = admin::path(&format!("media/{}", owner.0));
    let mut mp = base.join(folder_name(&label, &d.name));
    let mut n = 2;
    while mp.exists()
        && (is_mounted(&mp)
            || std::fs::read_dir(&mp)
                .map(|mut r| r.next().is_some())
                .unwrap_or(true))
    {
        mp = base.join(format!("{} ({n})", folder_name(&label, &d.name)));
        n += 1;
    }
    std::fs::create_dir_all(&mp).ok()?;
    let (_, uid, gid) = *owner;
    // Toujours d'abord en lecture seule : rien n'est écrit avant d'avoir
    // vérifié que Windows a bien fermé le volume.
    if let Err(e) = mount_volume(driver, &node, &mp, uid, gid, true) {
        crate::log_warn!("volumes", "montage impossible de {} : {e}", d.name);
        let _ = std::fs::remove_dir(&mp);
        post_notice(Notice::Failed, None, &label);
        return None;
    }
    let mut reason = "";
    if probe.fs == Fs::Ntfs {
        let header = read_at(&mp.join("hiberfil.sys"), 0, 4).unwrap_or_default();
        if hibernated(&header) {
            reason = "hibernated";
        } else if dirty {
            reason = "dirty";
        }
    }
    let mut read_only = true;
    if reason.is_empty() && unmount(&mp, false) {
        match mount_volume(driver, &node, &mp, uid, gid, false) {
            Ok(()) => read_only = false,
            Err(e) => {
                crate::log_warn!("volumes", "écriture refusée sur {} : {e}", d.name);
                reason = "readonly";
                mount_volume(driver, &node, &mp, uid, gid, true).ok()?;
            }
        }
    }
    let bus = if d.removable { "usb" } else { "fixed" };
    let backend = if reason.is_empty() {
        format!("{driver}:{bus}")
    } else {
        format!("{driver}:{bus}:{reason}")
    };
    let mut m = Mount {
        letter: 'D',
        kind: "disk".into(),
        mountpoint: mp.clone(),
        label: label.clone(),
        read_only,
        source: probe.serial.clone(),
        device: format!("/dev/{}", d.name),
        backend,
        state: String::new(),
    };
    let letter = match letter_hint {
        Some(l) => {
            m.letter = l;
            match mounts::add(m.clone()) {
                Ok(()) => l,
                Err(_) => mounts::add_auto(m).ok()?,
            }
        }
        None => mounts::add_auto(m).ok()?,
    };
    remember_letter(&probe.serial, letter);
    let notice = match reason {
        "hibernated" => Notice::Hibernated,
        "dirty" => Notice::Dirty,
        _ => Notice::Mounted,
    };
    // Disque interne : pas de bulle à chaque démarrage, sauf problème.
    if d.removable || notice != Notice::Mounted {
        post_notice(notice, Some(letter), &label);
    }
    crate::log_info!(
        "volumes",
        "{} monté en {letter}: ({})",
        d.name,
        if read_only {
            "lecture seule"
        } else {
            "lecture et écriture"
        }
    );
    Some(letter)
}

/// Retire un lecteur : démonte (paresseusement si le disque a disparu),
/// supprime le dossier et la lettre.
fn detach(m: &Mount, gone: bool) {
    if is_mounted(&m.mountpoint) {
        unmount(&m.mountpoint, gone);
    }
    let _ = std::fs::remove_dir(&m.mountpoint);
    let _ = mounts::remove(m.letter);
}

/// Un passage de surveillance.
pub fn scan(cfg: &Config, owner: &(String, u32, u32), seen: &mut BTreeSet<String>) {
    let devs = devices();
    let present: BTreeSet<String> = devs.iter().map(|d| format!("/dev/{}", d.name)).collect();
    let mut ej = ejected();
    let before = ej.len();
    ej.retain(|d| present.contains(d));
    // Disques débranchés ou démontés hors de Winlux.
    for m in mounts::list().into_iter().filter(|m| m.kind == "disk") {
        let gone = !present.contains(&m.device);
        if gone || !is_mounted(&m.mountpoint) {
            detach(&m, gone);
            if !gone {
                ej.insert(m.device.clone());
            }
        }
    }
    if ej.len() != before {
        set_ejected(&ej);
    }
    seen.retain(|d| present.contains(d));
    if !cfg.automount {
        return;
    }
    let busy = mounted_devs();
    let ours: BTreeSet<String> = mounts::list()
        .into_iter()
        .filter(|m| m.kind == "disk")
        .map(|m| m.device)
        .collect();
    for d in devs {
        let node = format!("/dev/{}", d.name);
        if busy.contains(&d.dev)
            || ours.contains(&node)
            || ej.contains(&node)
            || seen.contains(&node)
        {
            continue;
        }
        // Une seule tentative par branchement, réussie ou non.
        seen.insert(node);
        attach(&d, cfg, owner);
    }
}

/// Commandes du Gestionnaire de disques (`Z:\run\winlux\disk-action.txt`) :
/// `mount /dev/X`, `umount /dev/X`, `repair /dev/X`. Seuls les volumes
/// physiques vus par le service sont acceptés.
fn handle_actions(cfg: &Config, owner: &(String, u32, u32)) {
    let path = admin::path("run/winlux/disk-action.txt");
    let Some(text) = admin::read_text(&path) else {
        return;
    };
    let _ = std::fs::remove_file(&path);
    let devs = devices();
    for line in text.lines() {
        let Some((verb, node)) = line.trim().split_once(' ') else {
            continue;
        };
        let Some(d) = devs
            .iter()
            .find(|d| format!("/dev/{}", d.name) == node.trim())
        else {
            continue;
        };
        let node = format!("/dev/{}", d.name);
        let ours = mounts::list()
            .into_iter()
            .find(|m| m.kind == "disk" && m.device == node);
        let r = match (verb, ours) {
            ("mount", None) => {
                let mut ej = ejected();
                ej.remove(&node);
                set_ejected(&ej);
                // Demande explicite : disque interne accepté.
                let manual = Config {
                    internal: true,
                    ..*cfg
                };
                attach(d, &manual, owner)
                    .map(|l| format!("{l}:"))
                    .ok_or_else(|| "montage impossible".to_string())
            }
            ("umount", Some(m)) => eject(m.letter),
            ("repair", Some(m)) => repair(m.letter),
            _ => continue,
        };
        if let Err(e) = r {
            crate::log_warn!("volumes", "{verb} {node} : {e}");
        }
    }
}

/// État lisible par le bureau : `Z:\run\winlux\volumes\list.txt`.
fn write_list() {
    let text: String = mounts::list()
        .iter()
        .filter(|m| m.kind == "disk")
        .map(|m| describe(m) + "\n")
        .collect();
    let path = admin::path(RUN_DIR).join("list.txt");
    if admin::read_text(&path).as_deref() != Some(text.as_str()) {
        let _ = admin::write_text(&path, &text);
    }
}

pub fn daemon() -> ! {
    let _ = std::fs::create_dir_all(admin::path(RUN_DIR));
    let _ = admin::write_text(
        &admin::path(RUN_DIR).join("daemon.pid"),
        &std::process::id().to_string(),
    );
    let mut seen = BTreeSet::new();
    loop {
        let cfg = Config::load();
        let owner = session_owner();
        handle_actions(&cfg, &owner);
        scan(&cfg, &owner, &mut seen);
        write_list();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// Éjection demandée par l'Explorateur (bouton Éjecter de « Ce PC »).
pub fn eject(letter: char) -> Result<String, String> {
    let m = mounts::get(letter).ok_or("lecteur inconnu")?;
    if m.kind != "disk" {
        return Err("ce lecteur n'est pas un disque".into());
    }
    if let Some(r) = ask_daemon("umount", &m) {
        return r;
    }
    // `sync` d'abord : une clé retirée juste après garde ses fichiers.
    unsafe { libc::sync() };
    if is_mounted(&m.mountpoint) && !unmount(&m.mountpoint, false) {
        return Err(
            "Le lecteur est en cours d'utilisation. Fermez les fichiers ouverts et réessayez."
                .into(),
        );
    }
    let mut ej = ejected();
    ej.insert(m.device.clone());
    set_ejected(&ej);
    detach(&m, false);
    Ok(format!("{}:", m.letter))
}

/// Bureau sous le compte de l'utilisateur : démonter ou réparer demande root.
/// La demande passe par le service des volumes (root), qui lit
/// `run/winlux/disk-action.txt` toutes les deux secondes ; on attend son
/// résultat (lecteur retiré, ou remonté) au plus 20 s. `None` en root.
fn ask_daemon(verb: &str, m: &Mount) -> Option<Result<String, String>> {
    if unsafe { libc::geteuid() } == 0 {
        return None;
    }
    let daemon_pid = admin::read_text(&admin::path(RUN_DIR).join("daemon.pid"))?;
    let alive = daemon_pid
        .trim()
        .parse::<i32>()
        .is_ok_and(|pid| unsafe { libc::kill(pid, 0) } == 0 || errno_is_eperm());
    if !alive {
        return None;
    }
    let path = admin::path("run/winlux/disk-action.txt");
    let mut text = admin::read_text(&path).unwrap_or_default();
    text.push_str(&format!("{verb} {}\n", m.device));
    if let Err(e) = admin::write_text(&path, &text) {
        return Some(Err(format!("service des volumes injoignable : {e}")));
    }
    let mut processed = 0;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if admin::read_text(&path).is_some_and(|t| t.contains(&m.device)) {
            continue;
        }
        processed += 1;
        let gone = mounts::get(m.letter)
            .filter(|x| x.device == m.device)
            .is_none();
        if verb == "umount" && gone || verb != "umount" && processed > 10 {
            return Some(Ok(format!("{}:", m.letter)));
        }
        // Le service a lu la demande : quelques secondes pour la mener à bien.
        if processed > 25 {
            return Some(Err(
                "Le lecteur est en cours d'utilisation. Fermez les fichiers ouverts et réessayez."
                    .into(),
            ));
        }
    }
    Some(Err("le service des volumes n'a pas répondu".into()))
}

fn errno_is_eperm() -> bool {
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Réparation légère d'un volume NTFS « sale » (`ntfsfix -d`), puis
/// remontage en écriture. Refusée si Windows est en veille prolongée.
pub fn repair(letter: char) -> Result<String, String> {
    let m = mounts::get(letter).ok_or("lecteur inconnu")?;
    if m.kind != "disk" || !m.backend.starts_with("ntfs") {
        return Err("seuls les disques NTFS se réparent ici".into());
    }
    if let Some(r) = ask_daemon("repair", &m) {
        return r;
    }
    if m.backend.ends_with(":hibernated") {
        return Err(
            "Windows est en veille prolongée : arrêtez-le complètement pour libérer le disque."
                .into(),
        );
    }
    let tool = find_tool(&["ntfsfix"]).ok_or("ntfsfix absent")?;
    if is_mounted(&m.mountpoint) && !unmount(&m.mountpoint, false) {
        return Err("Le lecteur est en cours d'utilisation.".into());
    }
    let ok = Command::new(tool)
        .args(["-d", &m.device])
        .stdout(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    detach(&m, false);
    if !ok {
        return Err("La réparation a échoué : lancez « chkdsk /f » depuis Windows.".into());
    }
    // Remonté tout de suite, en écriture cette fois.
    let mut ej = ejected();
    ej.remove(&m.device);
    set_ejected(&ej);
    let cfg = Config {
        internal: true,
        ..Config::load()
    };
    let letter = devices()
        .iter()
        .find(|d| format!("/dev/{}", d.name) == m.device)
        .and_then(|d| attach(d, &cfg, &session_owner()))
        .unwrap_or(m.letter);
    post_notice(Notice::Repaired, Some(letter), &m.label);
    Ok(format!("{letter}:"))
}

/// Ligne de `winlux --volumes list` / `Z:\run\winlux\volumes\list.txt`.
pub fn describe(m: &Mount) -> String {
    let reason = m.backend.split(':').nth(2).unwrap_or("");
    format!(
        "{}:\t{}\t{}\t{}\t{}\t{}",
        m.letter,
        m.label,
        m.device,
        m.backend.split(':').next().unwrap_or(""),
        if m.read_only { "ro" } else { "rw" },
        reason
    )
}

fn usage() -> i32 {
    eprintln!("usage : winlux --volumes daemon | scan | list | eject X: | repair X:");
    2
}

fn letter_arg(a: Option<&String>) -> Option<char> {
    a.and_then(|s| s.chars().next())
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
}

/// Point d'entrée de `winlux --volumes …`.
pub fn cli(args: &[String]) -> i32 {
    let report = |r: Result<String, String>| match r {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    };
    match args.first().map(String::as_str).unwrap_or("") {
        "daemon" => daemon(),
        "scan" => {
            scan(&Config::load(), &session_owner(), &mut BTreeSet::new());
            0
        }
        "list" => {
            for m in mounts::list().iter().filter(|m| m.kind == "disk") {
                println!("{}", describe(m));
            }
            0
        }
        "eject" => match letter_arg(args.get(1)) {
            Some(l) => report(eject(l)),
            None => usage(),
        },
        "repair" => match letter_arg(args.get(1)) {
            Some(l) => report(repair(l)),
            None => usage(),
        },
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_mount_decode_tous_les_echappements() {
        assert_eq!(unescape_mount("/mnt/Mon\\040disque"), "/mnt/Mon disque");
        assert_eq!(unescape_mount("/a\\011b\\012c\\134d"), "/a\tb\nc\\d");
        assert_eq!(unescape_mount("/sans\\x"), "/sans\\x");
    }

    fn boot(oem: &[u8; 8]) -> Vec<u8> {
        let mut b = vec![0u8; 4096];
        b[3..11].copy_from_slice(oem);
        b[510] = 0x55;
        b[511] = 0xAA;
        b
    }

    #[test]
    fn probes_filesystems() {
        let mut ntfs = boot(b"NTFS    ");
        ntfs[0x48..0x50].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        let p = probe(&ntfs).unwrap();
        assert_eq!(p.fs, Fs::Ntfs);
        assert_eq!(p.serial, "1122334455667788");

        let mut fat = boot(b"MSDOS5.0");
        fat[82..90].copy_from_slice(b"FAT32   ");
        fat[0x43..0x47].copy_from_slice(&0xABCD_1234u32.to_le_bytes());
        fat[0x47..0x52].copy_from_slice(b"CLE USB    ");
        let p = probe(&fat).unwrap();
        assert_eq!(
            (p.fs, p.serial.as_str(), p.label.as_str()),
            (Fs::Fat, "ABCD1234", "CLE USB")
        );
        fat[0x47..0x52].copy_from_slice(b"NO NAME    ");
        assert_eq!(probe(&fat).unwrap().label, "");

        assert_eq!(probe(&boot(b"EXFAT   ")).unwrap().fs, Fs::Exfat);
        assert_eq!(probe(&boot(b"-FVE-FS-")).unwrap().fs, Fs::BitLocker);

        let mut ext = vec![0u8; 4096];
        ext[1024 + 0x38..1024 + 0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());
        ext[1024 + 0x78..1024 + 0x7F].copy_from_slice(b"Donnees");
        let p = probe(&ext).unwrap();
        assert_eq!((p.fs, p.label.as_str()), (Fs::Ext, "Donnees"));
        assert!(probe(&vec![0u8; 4096]).is_none());
    }

    /// Enregistrement `$Volume` minimal : nom, informations, fixups.
    fn volume_record(label: &str, flags: u16) -> Vec<u8> {
        let mut r = vec![0u8; 1024];
        r[0..4].copy_from_slice(b"FILE");
        r[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // tableau de fixups
        r[6..8].copy_from_slice(&3u16.to_le_bytes()); // 1 + 2 secteurs
        r[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        let mut off = 0x38;
        let name: Vec<u8> = label.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        for (kind, value) in [
            (0x60u32, name),
            (0x70u32, {
                let mut v = vec![0u8; 12];
                v[8] = 3;
                v[9] = 1;
                v[10..12].copy_from_slice(&flags.to_le_bytes());
                v
            }),
        ] {
            let len = (0x18 + value.len()).div_ceil(8) * 8;
            r[off..off + 4].copy_from_slice(&kind.to_le_bytes());
            r[off + 4..off + 8].copy_from_slice(&(len as u32).to_le_bytes());
            r[off + 0x10..off + 0x14].copy_from_slice(&(value.len() as u32).to_le_bytes());
            r[off + 0x14..off + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
            r[off + 0x18..off + 0x18 + value.len()].copy_from_slice(&value);
            off += len;
        }
        r[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        // Fixups : la fin de chaque secteur reçoit le numéro de séquence.
        r[0x30..0x32].copy_from_slice(&[0x07, 0x00]);
        for i in 1..3 {
            let end = i * 512;
            let saved = [r[end - 2], r[end - 1]];
            r[0x30 + 2 * i..0x32 + 2 * i].copy_from_slice(&saved);
            r[end - 2..end].copy_from_slice(&[0x07, 0x00]);
        }
        r
    }

    #[test]
    fn reads_ntfs_volume_state() {
        let v = parse_volume_record(&volume_record("Windows", 0), 512).unwrap();
        assert_eq!(
            v,
            NtfsVolume {
                label: "Windows".into(),
                dirty: false
            }
        );
        let v = parse_volume_record(&volume_record("Données", 1), 512).unwrap();
        assert!(v.dirty);
        assert_eq!(v.label, "Données");
        assert!(parse_volume_record(&[0u8; 1024], 512).is_none());
    }

    #[test]
    fn ntfs_geometry_from_boot_sector() {
        let mut b = boot(b"NTFS    ");
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 8;
        b[0x30..0x38].copy_from_slice(&786_432u64.to_le_bytes());
        b[0x40] = 0xF6; // -10 → 1024 octets
        let g = ntfs_geometry(&b).unwrap();
        assert_eq!(g.cluster, 4096);
        assert_eq!(g.record_size, 1024);
        assert_eq!(g.mft_offset, 786_432 * 4096);
    }

    #[test]
    fn hibernation_header() {
        assert!(hibernated(b"HIBR\0\0"));
        assert!(hibernated(b"hibr"));
        assert!(!hibernated(b"wake"));
        assert!(!hibernated(b"\0\0\0\0"));
        assert!(!hibernated(b""));
    }

    fn gpt_disk(entries: &[(&str, u64, u64)]) -> Vec<u8> {
        let mut d = vec![0u8; 512 * 34];
        d[446 + 4] = 0xEE;
        d[510] = 0x55;
        d[511] = 0xAA;
        d[512..520].copy_from_slice(b"EFI PART");
        d[512 + 72..512 + 80].copy_from_slice(&2u64.to_le_bytes());
        d[512 + 80..512 + 84].copy_from_slice(&128u32.to_le_bytes());
        d[512 + 84..512 + 88].copy_from_slice(&128u32.to_le_bytes());
        for (i, (t, start, attrs)) in entries.iter().enumerate() {
            let e = 1024 + i * 128;
            d[e..e + 16].copy_from_slice(&guid(t));
            d[e + 32..e + 40].copy_from_slice(&start.to_le_bytes());
            d[e + 48..e + 56].copy_from_slice(&attrs.to_le_bytes());
        }
        d
    }

    #[test]
    fn partition_roles() {
        let disk = gpt_disk(&[
            ("C12A7328-F81F-11D2-BA4B-00A0C93EC93B", 2048, 0),
            ("E3C9E316-0B5C-4DB8-817D-F92DF00215AE", 206_848, 0),
            (GPT_BASIC_DATA, 239_616, 0),
            ("DE94BBA4-06D1-4D40-A16A-BFD50179D6AC", 900_000, 1 << 63),
            (GPT_BASIC_DATA, 950_000, 1 << 63),
            (GPT_LINUX_DATA, 990_000, 0),
        ]);
        assert_eq!(guid(GPT_BASIC_DATA)[..4], [0xA2, 0xA0, 0xD0, 0xEB]);
        assert_eq!(partition_role(&disk, 512, 2048), Role::Hidden);
        assert_eq!(partition_role(&disk, 512, 206_848), Role::Hidden);
        assert_eq!(partition_role(&disk, 512, 239_616), Role::Data);
        assert_eq!(partition_role(&disk, 512, 900_000), Role::Hidden);
        assert_eq!(partition_role(&disk, 512, 950_000), Role::Hidden);
        assert_eq!(partition_role(&disk, 512, 990_000), Role::Linux);
        assert_eq!(partition_role(&disk, 512, 12345), Role::Hidden);

        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        for (i, (kind, start)) in [(0x0Cu8, 2048u32), (0x27, 100_000), (0x83, 200_000)]
            .iter()
            .enumerate()
        {
            mbr[446 + 16 * i + 4] = *kind;
            mbr[446 + 16 * i + 8..446 + 16 * i + 12].copy_from_slice(&start.to_le_bytes());
        }
        assert_eq!(partition_role(&mbr, 512, 2048), Role::Data);
        assert_eq!(partition_role(&mbr, 512, 100_000), Role::Hidden);
        assert_eq!(partition_role(&mbr, 512, 200_000), Role::Linux);
        assert_eq!(partition_role(&[0u8; 512], 512, 0), Role::Data);
    }

    #[test]
    fn mount_options_keep_the_owner() {
        let o = mount_options("ntfs3", 1000, 1000, false, true);
        assert_eq!(
            o,
            "rw,nosuid,nodev,uid=1000,gid=1000,umask=022,windows_names"
        );
        let o = mount_options("ntfs-3g", 1000, 100, true, false);
        assert_eq!(o, "ro,nosuid,nodev,uid=1000,gid=100,umask=022");
        assert!(mount_options("vfat", 0, 0, false, false).contains("shortname=mixed"));
        assert_eq!(
            mount_options("ext4", 1000, 1000, true, true),
            "ro,nosuid,nodev"
        );
    }

    #[test]
    fn folder_names_and_messages() {
        assert_eq!(folder_name("Mes/Docs", "sdb1"), "Mes_Docs");
        assert_eq!(folder_name("  ", "sdb1"), "sdb1");
        assert_eq!(folder_name("..", "sdb1"), "sdb1");
        let fr = notice_text(Notice::Hibernated, "Windows (E:)", "fr_FR.UTF-8");
        assert!(fr.contains("Windows (E:)") && fr.contains("lecture seule"));
        assert!(notice_text(Notice::Dirty, "X", "de_DE").contains("schreibgeschützt"));
        assert!(notice_text(Notice::Mounted, "X", "en_US").ends_with("is ready."));
    }

    #[test]
    fn session_owner_and_letters() {
        let root = crate::admin::test_root::Root::new("volumes");
        std::fs::create_dir_all(root.dir.join("run/winlux")).unwrap();
        std::fs::create_dir_all(root.dir.join("etc")).unwrap();
        std::fs::write(
            root.dir.join("run/winlux/sessions.txt"),
            "session=alice|:1|8|background\nsession=bob|:0|7|active\n",
        )
        .unwrap();
        std::fs::write(
            root.dir.join("etc/passwd"),
            "root:x:0:0::/root:/bin/sh\nbob:x:1001:1002::/home/bob:/bin/sh\n",
        )
        .unwrap();
        assert_eq!(session_owner(), ("bob".into(), 1001, 1002));
        assert_eq!(remembered_letter("AB12"), None);
        remember_letter("AB12", 'F');
        remember_letter("CD34", 'G');
        remember_letter("AB12", 'H');
        assert_eq!(remembered_letter("AB12"), Some('H'));
        assert_eq!(remembered_letter("CD34"), Some('G'));
        assert_eq!(remembered_letter(""), None);
    }
}
