//! `hid.dll` et énumération des périphériques HID par `setupapi` (#271),
//! sur l'interface hidraw de Linux.
//!
//! Les jeux, SDL et les logiciels de matériel (manettes, volants, souris
//! de jeu, claviers RVB) énumèrent les périphériques de l'interface
//! `GUID_DEVINTERFACE_HID` avec setupapi, ouvrent le chemin rendu par
//! `SetupDiGetDeviceInterfaceDetailW` avec `CreateFileW`, puis lisent les
//! rapports par `ReadFile` et décrivent l'appareil avec `HidD_*`/`HidP_*`.
//!
//! Chaque `/dev/hidrawN` devient le chemin
//! `\\?\hid#vid_XXXX&pid_YYYY#hidrawN#{4d1e55b2-f16f-11cf-88cb-001111000030}`.
//! L'identité (vendeur, produit, nom, numéro de série) vient de
//! `/sys/class/hidraw/hidrawN/device/uevent`, le descripteur de rapports de
//! `…/device/report_descriptor` (lisible sans droits particuliers).
//! `WINLUX_HIDRAW_SYSFS` et `WINLUX_HIDRAW_DEV` désignent d'autres dossiers
//! pour les tests.
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// {4D1E55B2-F16F-11CF-88CB-001111000030}
pub const HID_GUID: [u8; 16] = [
    0xB2, 0x55, 0x1E, 0x4D, 0x6F, 0xF1, 0xCF, 0x11, 0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30,
];
const HIDP_STATUS_SUCCESS: u32 = 0x0011_0000;
const HIDP_STATUS_INVALID_PREPARSED_DATA: u32 = 0xC011_0001;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_INVALID_HANDLE: u32 = 6;

fn sysfs_root() -> PathBuf {
    std::env::var_os("WINLUX_HIDRAW_SYSFS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys/class/hidraw"))
}
fn dev_root() -> PathBuf {
    std::env::var_os("WINLUX_HIDRAW_DEV")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev"))
}

/// Un périphérique hidraw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HidDevice {
    pub index: u32,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub name: String,
    pub serial: String,
    pub descriptor: Vec<u8>,
}

impl HidDevice {
    /// Chemin d'interface Windows.
    pub fn interface_path(&self) -> String {
        format!(
            "\\\\?\\hid#vid_{:04x}&pid_{:04x}#hidraw{}#{{4d1e55b2-f16f-11cf-88cb-001111000030}}",
            self.vendor, self.product, self.index
        )
    }
    fn load(index: u32) -> Option<HidDevice> {
        let dir = sysfs_root().join(format!("hidraw{index}")).join("device");
        let uevent = std::fs::read_to_string(dir.join("uevent")).ok()?;
        let mut d = HidDevice {
            index,
            ..Default::default()
        };
        for line in uevent.lines() {
            if let Some(id) = line.strip_prefix("HID_ID=") {
                // bus:vendor:produit, en hexadécimal sur 4 et 8 chiffres.
                let mut parts = id.split(':').skip(1);
                d.vendor = parts
                    .next()
                    .and_then(|v| u32::from_str_radix(v, 16).ok())
                    .unwrap_or(0) as u16;
                d.product = parts
                    .next()
                    .and_then(|v| u32::from_str_radix(v, 16).ok())
                    .unwrap_or(0) as u16;
            } else if let Some(n) = line.strip_prefix("HID_NAME=") {
                d.name = n.to_string();
            } else if let Some(s) = line.strip_prefix("HID_UNIQ=") {
                d.serial = s.to_string();
            }
        }
        d.descriptor = std::fs::read(dir.join("report_descriptor")).unwrap_or_default();
        Some(d)
    }
}

/// Périphériques hidraw présents, triés par numéro.
pub fn devices() -> Vec<HidDevice> {
    let mut idx: Vec<u32> = std::fs::read_dir(sysfs_root())
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_str()?.strip_prefix("hidraw")?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    idx.sort_unstable();
    idx.into_iter().filter_map(HidDevice::load).collect()
}

/// Chemin Linux d'un chemin d'interface HID passé à CreateFile.
pub fn device_path(path: &str) -> Option<PathBuf> {
    let lower = path.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("\\\\?\\hid#")
        .or_else(|| lower.strip_prefix("\\\\.\\hid#"))?;
    let index: u32 = rest
        .split('#')
        .nth(1)?
        .strip_prefix("hidraw")?
        .parse()
        .ok()?;
    Some(dev_root().join(format!("hidraw{index}")))
}

// ============================================================
//  Ouverture : un relais par périphérique
// ============================================================
//
// Sous Windows, chaque rapport lu ou écrit commence par son identifiant,
// 0 quand le descripteur n'en a pas ; hidraw l'omet dans ce cas. Le handle
// rendu à CreateFile est donc un socketpair SOCK_SEQPACKET (une lecture =
// un rapport) relié au vrai /dev/hidrawN par un fil qui ajoute ou retire
// cet octet. Les lectures asynchrones (OVERLAPPED), l'annulation et la
// fermeture passent ainsi par le chemin existant des sockets.

struct Opened {
    inode: u64,
    device: HidDevice,
    raw: i32,
}
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());

fn inode_of(fd: i32) -> Option<u64> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some(st.st_ino as u64)
}

/// Ouvre `/dev/hidrawN` et rend le bout « programme » du relais.
pub fn open(dev: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::{FromRawFd, IntoRawFd};
    let index: u32 = dev
        .file_name()
        .and_then(|n| n.to_str()?.strip_prefix("hidraw")?.parse().ok())
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let device =
        HidDevice::load(index).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let raw = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dev)
        .or_else(|_| std::fs::File::open(dev))?
        .into_raw_fd();
    let mut pair = [0i32; 2];
    if unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            pair.as_mut_ptr(),
        )
    } != 0
    {
        let e = std::io::Error::last_os_error();
        unsafe { libc::close(raw) };
        return Err(e);
    }
    let (app, pump) = (pair[0], pair[1]);
    let prefix = !parse_descriptor(&device.descriptor).report_ids;
    let inode = inode_of(app).unwrap_or(0);
    crate::dlls::lock(&OPENED).push(Opened { inode, device, raw });
    let spawned = std::thread::Builder::new()
        .name("winlux-hid".into())
        .spawn(move || {
            relay(raw, pump, prefix);
            crate::dlls::lock(&OPENED).retain(|o| o.raw != raw);
            unsafe {
                libc::close(pump);
                libc::close(raw);
            }
        });
    if spawned.is_err() {
        crate::dlls::lock(&OPENED).retain(|o| o.raw != raw);
        unsafe {
            libc::close(app);
            libc::close(pump);
            libc::close(raw);
        }
        return Err(std::io::Error::other("fil du relais HID"));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(app) })
}

/// Recopie les rapports dans les deux sens jusqu'à la fermeture du handle
/// (ou le débranchement de l'appareil).
fn relay(raw: i32, pump: i32, prefix: bool) {
    let mut buf = [0u8; 4097];
    loop {
        let mut fds = [
            libc::pollfd {
                fd: raw,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: pump,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents & (libc::POLLHUP | libc::POLLERR) != 0
            && fds[1].revents & libc::POLLIN == 0
        {
            return; // CloseHandle
        }
        if fds[0].revents & libc::POLLIN != 0 {
            let at = usize::from(prefix);
            let n = unsafe { libc::read(raw, buf.as_mut_ptr().add(at) as *mut _, buf.len() - at) };
            if n <= 0 {
                return; // appareil débranché
            }
            buf[0] = if prefix { 0 } else { buf[0] };
            unsafe {
                libc::send(
                    pump,
                    buf.as_ptr() as *const _,
                    n as usize + at,
                    libc::MSG_NOSIGNAL,
                )
            };
        } else if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            return;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let n = unsafe { libc::recv(pump, buf.as_mut_ptr() as *mut _, buf.len(), 0) };
            if n <= 0 {
                return;
            }
            let at = usize::from(prefix && buf[0] == 0);
            unsafe { libc::write(raw, buf.as_ptr().add(at) as *const _, n as usize - at) };
        }
    }
}

/// Périphérique ouvert derrière un handle, et le descripteur du vrai hidraw.
fn device_of_handle(handle: usize) -> Option<(HidDevice, i32)> {
    let fd = crate::dlls::kernel32::handle_to_fd(handle);
    if fd < 0 {
        return None;
    }
    let inode = inode_of(fd)?;
    crate::dlls::lock(&OPENED)
        .iter()
        .find(|o| o.inode == inode)
        .map(|o| (o.device.clone(), o.raw))
}

// ============================================================
//  Descripteur de rapports
// ============================================================

/// Résumé d'un descripteur : collection de tête et taille maximale des
/// rapports (octets, identifiant de rapport compris comme sous Windows).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Caps {
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub input_buttons: u16,
    pub input_values: u16,
    /// Le descripteur numérote ses rapports (élément Report ID).
    pub report_ids: bool,
}

pub fn parse_descriptor(d: &[u8]) -> Caps {
    let mut caps = Caps::default();
    let (mut usage_page, mut report_size, mut report_count, mut report_id) =
        (0u32, 0u32, 0u32, 0u32);
    let mut local_usage: Option<u32> = None;
    let mut depth = 0;
    // Bits par (type, identifiant de rapport).
    let mut bits: HashMap<(u8, u32), u32> = HashMap::new();
    let mut i = 0;
    while i < d.len() {
        let prefix = d[i];
        if prefix == 0xFE {
            // Élément long : taille dans l'octet suivant.
            let n = d.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + n;
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            s => s as usize,
        };
        let mut value = 0u32;
        for k in 0..size {
            value |= (d.get(i + 1 + k).copied().unwrap_or(0) as u32) << (8 * k);
        }
        let (kind, tag) = ((prefix >> 2) & 3, prefix >> 4);
        match (kind, tag) {
            // Principal : Input, Output, Feature, Collection, End Collection.
            (0, 8) | (0, 9) | (0, 11) => {
                let t = match tag {
                    8 => 0,
                    9 => 1,
                    _ => 2,
                };
                *bits.entry((t, report_id)).or_default() += report_size * report_count;
                if t == 0 && value & 1 == 0 {
                    // Donnée (pas constante) : bouton si 1 bit, sinon valeur.
                    if report_size == 1 {
                        caps.input_buttons += 1;
                    } else {
                        caps.input_values += 1;
                    }
                }
                local_usage = None;
            }
            (0, 10) => {
                if depth == 0 && caps.usage_page == 0 {
                    caps.usage_page = usage_page as u16;
                    caps.usage = local_usage.unwrap_or(0) as u16;
                }
                depth += 1;
                local_usage = None;
            }
            (0, 12) => depth -= 1,
            // Global : Usage Page, Report Size, Report ID, Report Count.
            (1, 0) => usage_page = value,
            (1, 7) => report_size = value,
            (1, 8) => {
                report_id = value;
                caps.report_ids = true;
            }
            (1, 9) => report_count = value,
            // Local : Usage (16 bits haut = page si élément sur 4 octets).
            (2, 0) if local_usage.is_none() => {
                local_usage = Some(if size == 4 { value & 0xFFFF } else { value });
            }
            _ => {}
        }
        i += 1 + size;
    }
    let max = |t: u8| {
        bits.iter()
            .filter(|((k, _), _)| *k == t)
            .map(|(_, b)| b.div_ceil(8) + 1)
            .max()
            .unwrap_or(0) as u16
    };
    caps.input_len = max(0);
    caps.output_len = max(1);
    caps.feature_len = max(2);
    caps
}

// ============================================================
//  hid.dll
// ============================================================

pub unsafe extern "win64" fn HidD_GetHidGuid(guid: *mut u8) {
    if !guid.is_null() {
        std::ptr::copy_nonoverlapping(HID_GUID.as_ptr(), guid, 16);
    }
}

/// HIDD_ATTRIBUTES : Size, VendorID, ProductID, VersionNumber.
pub unsafe extern "win64" fn HidD_GetAttributes(handle: usize, attrs: *mut u8) -> u8 {
    let Some((d, _)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if attrs.is_null() {
        return 0;
    }
    (attrs as *mut u32).write_unaligned(12);
    (attrs.add(4) as *mut u16).write_unaligned(d.vendor);
    (attrs.add(6) as *mut u16).write_unaligned(d.product);
    (attrs.add(8) as *mut u16).write_unaligned(d.version);
    1
}

unsafe fn put_string(buf: *mut u16, bytes: u32, s: &str) -> u8 {
    if buf.is_null() || s.is_empty() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let max = (bytes / 2) as usize;
    if max == 0 {
        return 0;
    }
    let wide: Vec<u16> = s
        .encode_utf16()
        .take(max - 1)
        .chain(std::iter::once(0))
        .collect();
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    1
}

/// Nom du produit (HID_NAME contient souvent « Fabricant Produit »).
pub unsafe extern "win64" fn HidD_GetProductString(handle: usize, buf: *mut u16, bytes: u32) -> u8 {
    match device_of_handle(handle) {
        Some((d, _)) => put_string(buf, bytes, &d.name),
        None => 0,
    }
}
pub unsafe extern "win64" fn HidD_GetManufacturerString(
    handle: usize,
    buf: *mut u16,
    bytes: u32,
) -> u8 {
    match device_of_handle(handle) {
        Some((d, _)) => {
            let maker = d.name.split_whitespace().next().unwrap_or("").to_string();
            put_string(buf, bytes, &maker)
        }
        None => 0,
    }
}
pub unsafe extern "win64" fn HidD_GetSerialNumberString(
    handle: usize,
    buf: *mut u16,
    bytes: u32,
) -> u8 {
    match device_of_handle(handle) {
        Some((d, _)) => put_string(buf, bytes, &d.serial),
        None => 0,
    }
}

/// Données « préanalysées » : une copie des capacités, rendue par pointeur.
#[repr(C)]
struct Preparsed {
    magic: u64,
    caps: Caps,
}
const PREPARSED_MAGIC: u64 = 0x5749_4E4C_5848_4944; // « WINLXHID »

pub unsafe extern "win64" fn HidD_GetPreparsedData(handle: usize, out: *mut usize) -> u8 {
    let Some((d, _)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let p = Box::new(Preparsed {
        magic: PREPARSED_MAGIC,
        caps: parse_descriptor(&d.descriptor),
    });
    *out = Box::into_raw(p) as usize;
    1
}

pub unsafe extern "win64" fn HidD_FreePreparsedData(data: usize) -> u8 {
    let p = data as *mut Preparsed;
    if p.is_null() || (*p).magic != PREPARSED_MAGIC {
        return 0;
    }
    (*p).magic = 0;
    drop(Box::from_raw(p));
    1
}

/// HIDP_CAPS (64 octets).
pub unsafe extern "win64" fn HidP_GetCaps(data: usize, out: *mut u16) -> u32 {
    let p = data as *const Preparsed;
    if p.is_null() || (*p).magic != PREPARSED_MAGIC || out.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let c = (*p).caps;
    std::ptr::write_bytes(out, 0, 32);
    *out = c.usage;
    *out.add(1) = c.usage_page;
    *out.add(2) = c.input_len;
    *out.add(3) = c.output_len;
    *out.add(4) = c.feature_len;
    // Après Reserved[17] : NumberLinkCollectionNodes, puis les compteurs.
    *out.add(22) = 1;
    *out.add(23) = u16::from(c.input_buttons > 0);
    *out.add(24) = c.input_values;
    *out.add(25) = c.input_buttons + c.input_values;
    HIDP_STATUS_SUCCESS
}

/// Rapports par ioctl hidraw (HIDIOCSFEATURE, HIDIOCGFEATURE, HIDIOCGINPUT,
/// HIDIOCSOUTPUT) : _IOC(lecture|écriture, 'H', nr, taille).
unsafe fn report_ioctl(handle: usize, nr: u64, buf: *mut u8, len: u32) -> u8 {
    let Some((_, fd)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if buf.is_null() || len == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let req = (3u64 << 30) | ((len as u64 & 0x3FFF) << 16) | ((b'H' as u64) << 8) | nr;
    u8::from(libc::ioctl(fd, req as _, buf) >= 0)
}
pub unsafe extern "win64" fn HidD_SetFeature(handle: usize, buf: *mut u8, len: u32) -> u8 {
    report_ioctl(handle, 0x06, buf, len)
}
pub unsafe extern "win64" fn HidD_GetFeature(handle: usize, buf: *mut u8, len: u32) -> u8 {
    report_ioctl(handle, 0x07, buf, len)
}
pub unsafe extern "win64" fn HidD_GetInputReport(handle: usize, buf: *mut u8, len: u32) -> u8 {
    report_ioctl(handle, 0x0A, buf, len)
}
pub unsafe extern "win64" fn HidD_SetOutputReport(handle: usize, buf: *mut u8, len: u32) -> u8 {
    report_ioctl(handle, 0x0B, buf, len)
}
/// File des rapports d'entrée : gérée par le noyau (hidraw), rien à régler.
pub extern "win64" fn HidD_SetNumInputBuffers(_handle: usize, count: u32) -> u8 {
    u8::from((2..=512).contains(&count))
}
pub unsafe extern "win64" fn HidD_GetNumInputBuffers(_handle: usize, count: *mut u32) -> u8 {
    if count.is_null() {
        return 0;
    }
    *count = 64;
    1
}
pub extern "win64" fn HidD_FlushQueue(handle: usize) -> u8 {
    u8::from(device_of_handle(handle).is_some())
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "HidD_GetHidGuid" => fn HidD_GetHidGuid,
        "HidD_GetAttributes" => fn HidD_GetAttributes,
        "HidD_GetProductString" => fn HidD_GetProductString,
        "HidD_GetManufacturerString" => fn HidD_GetManufacturerString,
        "HidD_GetSerialNumberString" => fn HidD_GetSerialNumberString,
        "HidD_GetPreparsedData" => fn HidD_GetPreparsedData,
        "HidD_FreePreparsedData" => fn HidD_FreePreparsedData,
        "HidP_GetCaps" => fn HidP_GetCaps,
        "HidD_SetFeature" => fn HidD_SetFeature,
        "HidD_GetFeature" => fn HidD_GetFeature,
        "HidD_GetInputReport" => fn HidD_GetInputReport,
        "HidD_SetOutputReport" => fn HidD_SetOutputReport,
        "HidD_SetNumInputBuffers" => fn HidD_SetNumInputBuffers,
        "HidD_GetNumInputBuffers" => fn HidD_GetNumInputBuffers,
        "HidD_FlushQueue" => fn HidD_FlushQueue,
    }
}

// ============================================================
//  setupapi : interface GUID_DEVINTERFACE_HID
// ============================================================

static SETS: Mutex<Vec<(usize, Vec<HidDevice>)>> = Mutex::new(Vec::new());
static NEXT_SET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5E70_0001);

fn set_devices(set: usize) -> Option<Vec<HidDevice>> {
    crate::dlls::lock(&SETS)
        .iter()
        .find(|(h, _)| *h == set)
        .map(|(_, d)| d.clone())
}

/// `SetupDiGetClassDevsW` pour l'interface HID (ou toutes les classes) ;
/// `None` pour une autre classe.
pub unsafe fn get_class_devs(guid: *const u8, flags: u32) -> Option<usize> {
    const DIGCF_ALLCLASSES: u32 = 0x4;
    let hid = !guid.is_null() && std::slice::from_raw_parts(guid, 16) == HID_GUID;
    if !hid && flags & DIGCF_ALLCLASSES == 0 {
        return None;
    }
    let h = NEXT_SET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::dlls::lock(&SETS).push((h, devices()));
    Some(h)
}

pub fn destroy(set: usize) -> bool {
    let mut sets = crate::dlls::lock(&SETS);
    let before = sets.len();
    sets.retain(|(h, _)| *h != set);
    sets.len() != before
}

/// SP_DEVINFO_DATA : cbSize, ClassGuid, DevInst, Reserved.
pub unsafe fn enum_device_info(set: usize, index: u32, data: *mut u8) -> Option<bool> {
    let devs = set_devices(set)?;
    let Some(d) = devs.get(index as usize) else {
        crate::process::set_last_error(ERROR_NO_MORE_ITEMS);
        return Some(false);
    };
    if !data.is_null() {
        std::ptr::copy_nonoverlapping(HID_GUID.as_ptr(), data.add(4), 16);
        (data.add(20) as *mut u32).write_unaligned(0x1000 + d.index);
        (data.add(24) as *mut usize).write_unaligned(index as usize + 1);
    }
    Some(true)
}

/// SP_DEVICE_INTERFACE_DATA : cbSize, InterfaceClassGuid, Flags, Reserved.
pub unsafe fn enum_interfaces(set: usize, index: u32, data: *mut u8) -> Option<bool> {
    let devs = set_devices(set)?;
    if devs.get(index as usize).is_none() {
        crate::process::set_last_error(ERROR_NO_MORE_ITEMS);
        return Some(false);
    }
    if !data.is_null() {
        std::ptr::copy_nonoverlapping(HID_GUID.as_ptr(), data.add(4), 16);
        (data.add(20) as *mut u32).write_unaligned(1); // SPINT_ACTIVE
        (data.add(24) as *mut usize).write_unaligned(index as usize + 1);
    }
    Some(true)
}

/// SP_DEVICE_INTERFACE_DETAIL_DATA_W : cbSize (8 en 64 bits), DevicePath.
pub unsafe fn interface_detail(
    set: usize,
    iface: *const u8,
    detail: *mut u8,
    size: u32,
    required: *mut u32,
    devinfo: *mut u8,
) -> Option<bool> {
    let devs = set_devices(set)?;
    if iface.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return Some(false);
    }
    let index = (iface.add(24) as *const usize)
        .read_unaligned()
        .wrapping_sub(1);
    let Some(d) = devs.get(index) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return Some(false);
    };
    let wide: Vec<u16> = d
        .interface_path()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let needed = 4 + wide.len() as u32 * 2;
    if !required.is_null() {
        *required = needed;
    }
    if !devinfo.is_null() {
        enum_device_info(set, index as u32, devinfo);
    }
    if detail.is_null() || size < needed {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return Some(false);
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, detail.add(4), wide.len() * 2);
    Some(true)
}

/// Propriétés lisibles d'un appareil : description, nom convivial,
/// identifiants matériels (MULTI_SZ), fabricant.
pub unsafe fn registry_property(
    set: usize,
    devinfo: *const u8,
    prop: u32,
) -> Option<(u32, Vec<u16>)> {
    let devs = set_devices(set)?;
    if devinfo.is_null() {
        return None;
    }
    let index = (devinfo.add(24) as *const usize)
        .read_unaligned()
        .wrapping_sub(1);
    let d = devs.get(index)?;
    let text = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    Some(match prop {
        0 | 0xC => (
            1,
            text(if d.name.is_empty() {
                "Périphérique HID"
            } else {
                &d.name
            }),
        ),
        1 => {
            let mut v = text(&format!("HID\\VID_{:04X}&PID_{:04X}", d.vendor, d.product));
            v.extend(text("HID_DEVICE"));
            v.push(0);
            (7, v)
        }
        0xB => (
            1,
            text(d.name.split_whitespace().next().unwrap_or("(Standard)")),
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manette de jeu : 16 boutons, 4 axes de 8 bits, un rapport de sortie
    /// de 2 octets (vibrations), sans identifiant de rapport.
    const GAMEPAD: &[u8] = &[
        0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, // Generic Desktop, Game Pad, Application
        0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x10, 0x81,
        0x02, // 16 boutons
        0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35, 0x15, 0x00, 0x26, 0xFF, 0x00,
        0x75, 0x08, 0x95, 0x04, 0x81, 0x02, // 4 axes
        0x06, 0x00, 0xFF, 0x09, 0x01, 0x75, 0x08, 0x95, 0x02, 0x91, 0x02, // sortie
        0xC0,
    ];

    #[test]
    fn analyse_un_descripteur_de_manette() {
        let c = parse_descriptor(GAMEPAD);
        assert_eq!((c.usage_page, c.usage), (1, 5));
        assert_eq!(c.input_len, 2 + 4 + 1);
        assert_eq!(c.output_len, 2 + 1);
        assert_eq!(c.feature_len, 0);
        assert_eq!(c.input_buttons, 1);
        assert_eq!(c.input_values, 1);
    }

    #[test]
    fn chemin_d_interface_et_retour() {
        let d = HidDevice {
            index: 3,
            vendor: 0x045E,
            product: 0x028E,
            ..Default::default()
        };
        let p = d.interface_path();
        assert_eq!(
            p,
            "\\\\?\\hid#vid_045e&pid_028e#hidraw3#{4d1e55b2-f16f-11cf-88cb-001111000030}"
        );
        assert_eq!(device_path(&p).unwrap().file_name().unwrap(), "hidraw3");
        assert_eq!(
            device_path(&p.to_uppercase()).unwrap().file_name().unwrap(),
            "hidraw3"
        );
        assert!(device_path("C:\\hid#x").is_none());
    }
}
