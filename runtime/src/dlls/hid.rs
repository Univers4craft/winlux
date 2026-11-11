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
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_GEN_FAILURE: u32 = 31;
const ERROR_INVALID_PARAMETER: u32 = 87;
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

/// Un périphérique hidraw, ou une de ses collections de tête : sous Windows,
/// chaque collection de tête (clavier, contrôles multimédia, interface
/// fabricant…) est un périphérique HID à part, avec son propre chemin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HidDevice {
    pub index: u32,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    /// Nom complet du noyau (HID_NAME, souvent « Fabricant Produit »).
    pub name: String,
    pub serial: String,
    /// Chaînes de l'appareil USB parent, quand il y en a un.
    pub manufacturer: String,
    pub product_name: String,
    /// Numéro d'interface USB (`&mi_NN` du chemin Windows).
    pub interface: Option<u8>,
    /// Indices des chaînes USB (iManufacturer, iProduct, iSerialNumber).
    pub string_indices: [u8; 3],
    /// Collection de tête, à partir de 1 ; 0 quand l'appareil n'en a qu'une.
    pub collection: u16,
    pub descriptor: Vec<u8>,
}

impl HidDevice {
    /// Chemin d'interface Windows.
    pub fn interface_path(&self) -> String {
        let mut first = format!("vid_{:04x}&pid_{:04x}", self.vendor, self.product);
        if let Some(mi) = self.interface {
            first.push_str(&format!("&mi_{mi:02x}"));
        }
        if self.collection > 0 {
            first.push_str(&format!("&col{:02x}", self.collection));
        }
        format!(
            "\\\\?\\hid#{first}#hidraw{}#{{4d1e55b2-f16f-11cf-88cb-001111000030}}",
            self.index
        )
    }
    /// Identifiant d'instance Plug and Play (`HID\VID_…&PID_…&MI_…&COL…\HIDRAWn`).
    pub fn instance_id(&self) -> String {
        let mut first = format!("VID_{:04X}&PID_{:04X}", self.vendor, self.product);
        if let Some(mi) = self.interface {
            first.push_str(&format!("&MI_{mi:02X}"));
        }
        if self.collection > 0 {
            first.push_str(&format!("&COL{:02X}", self.collection));
        }
        format!("HID\\{first}\\HIDRAW{}", self.index)
    }
    /// Fabricant : chaîne USB, sinon premier mot du nom du noyau.
    pub fn manufacturer_string(&self) -> String {
        if !self.manufacturer.is_empty() {
            return self.manufacturer.clone();
        }
        self.name
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }
    /// Produit : chaîne USB, sinon le nom du noyau.
    pub fn product_string(&self) -> String {
        if !self.product_name.is_empty() {
            return self.product_name.clone();
        }
        self.name.clone()
    }
    /// Capacités de la collection de tête de ce périphérique.
    pub fn tlc(&self) -> crate::dlls::hid_report::Tlc {
        let mut all =
            crate::dlls::hid_report::split(&crate::dlls::hid_report::parse(&self.descriptor));
        let i = (self.collection.max(1) - 1) as usize;
        if i < all.len() {
            all.swap_remove(i)
        } else {
            all.swap_remove(0)
        }
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
        // Appareil USB : …/1-2/1-2:1.0/0003:VVVV:PPPP.NNNN. Le dossier parent
        // est l'interface USB, le grand-parent l'appareil.
        if let Ok(real) = std::fs::canonicalize(&dir) {
            let read = |p: PathBuf| {
                std::fs::read_to_string(p)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            };
            if let Some(intf) = real.parent() {
                if let Ok(n) = u8::from_str_radix(&read(intf.join("bInterfaceNumber")), 16) {
                    d.interface = Some(n);
                }
                if let Some(usb) = intf.parent().filter(|_| d.interface.is_some()) {
                    d.manufacturer = read(usb.join("manufacturer"));
                    d.product_name = read(usb.join("product"));
                    if d.serial.is_empty() {
                        d.serial = read(usb.join("serial"));
                    }
                    d.version = u16::from_str_radix(&read(usb.join("bcdDevice")), 16).unwrap_or(0);
                    // Descripteur d'appareil USB : iManufacturer, iProduct,
                    // iSerialNumber aux octets 14, 15 et 16.
                    if let Ok(raw) = std::fs::read(usb.join("descriptors")) {
                        if raw.len() >= 18 {
                            d.string_indices = [raw[14], raw[15], raw[16]];
                        }
                    }
                }
            }
        }
        Some(d)
    }
}

/// Périphériques hidraw présents, triés par numéro (un par nœud hidraw).
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

/// Interfaces HID vues par setupapi : une par collection de tête.
pub fn interfaces() -> Vec<HidDevice> {
    let mut out = Vec::new();
    for d in devices() {
        out.extend(split_collections(d));
    }
    out
}

fn split_collections(d: HidDevice) -> Vec<HidDevice> {
    let n = crate::dlls::hid_report::parse(&d.descriptor).tlcs.len();
    if n <= 1 {
        return vec![d];
    }
    (1..=n as u16)
        .map(|c| HidDevice {
            collection: c,
            ..d.clone()
        })
        .collect()
}

/// Numéro hidraw et collection de tête d'un chemin d'interface HID.
pub fn parse_path(path: &str) -> Option<(u32, u16)> {
    let lower = path.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("\\\\?\\hid#")
        .or_else(|| lower.strip_prefix("\\\\.\\hid#"))?;
    let mut parts = rest.split('#');
    let first = parts.next()?;
    let index: u32 = parts.next()?.strip_prefix("hidraw")?.parse().ok()?;
    let collection = first
        .split('&')
        .find_map(|p| u16::from_str_radix(p.strip_prefix("col")?, 16).ok())
        .unwrap_or(0);
    Some((index, collection))
}

/// Chemin Linux d'un chemin d'interface HID passé à CreateFile.
#[cfg(test)]
pub fn device_path(path: &str) -> Option<PathBuf> {
    let (index, _) = parse_path(path)?;
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
//
// Quand l'appareil a plusieurs collections de tête, le relais ne transmet
// que les rapports d'entrée de la collection ouverte, comme Windows.
//
// Une ouverture sans droit d'accès (dwDesiredAccess = 0), que hidapi et les
// logiciels de matériel font pour lire les attributs d'un clavier ou d'une
// souris, réussit même quand /dev/hidrawN est fermé à l'utilisateur : le
// handle sert alors aux fonctions HidD_Get*/HidP_*, pas aux rapports.

struct Opened {
    inode: u64,
    device: HidDevice,
    /// Descripteur du vrai hidraw, -1 pour un handle d'attributs seulement.
    raw: i32,
}
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());

fn inode_of(fd: i32) -> Option<u64> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some(st.st_ino as u64)
}

fn register(inode: u64, device: HidDevice, raw: i32) {
    let mut opened = crate::dlls::lock(&OPENED);
    // Un inode libéré peut être réutilisé : l'ancienne entrée est morte.
    opened.retain(|o| o.inode != inode);
    opened.push(Opened { inode, device, raw });
}

/// GENERIC_READ, GENERIC_WRITE, GENERIC_ALL, FILE_READ_DATA, FILE_WRITE_DATA.
fn wants_data(access: u32) -> bool {
    access & (0x8000_0000 | 0x4000_0000 | 0x1000_0000 | 0x1 | 0x2) != 0
}

/// Ouvre le périphérique désigné par un chemin d'interface Windows.
pub fn open_path(path: &str, access: u32) -> std::io::Result<std::fs::File> {
    let (index, collection) =
        parse_path(path).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let base =
        HidDevice::load(index).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let device = split_collections(base)
        .into_iter()
        .find(|d| d.collection == collection)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let dev = dev_root().join(format!("hidraw{index}"));
    match open_device(&dev, device.clone()) {
        Ok(f) => Ok(f),
        Err(e) if !wants_data(access) => attributes_only(device).map_err(|_| e),
        Err(e) => Err(e),
    }
}

/// Handle sans accès aux rapports : un memfd vide, propre à ce handle.
fn attributes_only(device: HidDevice) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::FromRawFd;
    let fd = unsafe { libc::memfd_create(c"winlux-hid".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    register(inode_of(fd).unwrap_or(0), device, -1);
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn open_device(dev: &std::path::Path, device: HidDevice) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::{FromRawFd, IntoRawFd};
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
    let desc = crate::dlls::hid_report::parse(&device.descriptor);
    let prefix = !desc.report_ids;
    // Filtre des rapports d'entrée : seulement s'il y a plusieurs collections.
    let filter: Option<Vec<u8>> = (device.collection > 0).then(|| device.tlc().input_ids);
    register(inode_of(app).unwrap_or(0), device, raw);
    let spawned = std::thread::Builder::new()
        .name("winlux-hid".into())
        .spawn(move || {
            relay(raw, pump, prefix, filter.as_deref());
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
fn relay(raw: i32, pump: i32, prefix: bool, filter: Option<&[u8]>) {
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
            if filter.is_some_and(|ids| !ids.contains(&buf[0])) {
                continue; // rapport d'une autre collection de tête
            }
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

pub unsafe extern "win64" fn HidD_GetProductString(handle: usize, buf: *mut u16, bytes: u32) -> u8 {
    match device_of_handle(handle) {
        Some((d, _)) => put_string(buf, bytes, &d.product_string()),
        None => 0,
    }
}
pub unsafe extern "win64" fn HidD_GetManufacturerString(
    handle: usize,
    buf: *mut u16,
    bytes: u32,
) -> u8 {
    match device_of_handle(handle) {
        Some((d, _)) => put_string(buf, bytes, &d.manufacturer_string()),
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
/// Chaîne USB par son indice. Linux expose les trois chaînes de
/// l'appareil (fabricant, produit, numéro de série) ; les autres indices
/// ne sont pas lisibles sans accès usbfs.
pub unsafe extern "win64" fn HidD_GetIndexedString(
    handle: usize,
    index: u32,
    buf: *mut u16,
    bytes: u32,
) -> u8 {
    let Some((d, _)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    let text = match d
        .string_indices
        .iter()
        .position(|&i| i != 0 && u32::from(i) == index)
    {
        Some(0) => d.manufacturer_string(),
        Some(1) => d.product_string(),
        Some(2) => d.serial.clone(),
        _ => String::new(),
    };
    if text.is_empty() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    put_string(buf, bytes, &text)
}
/// Descripteur physique (rarissime) : absent, comme sur la plupart des
/// appareils.
pub extern "win64" fn HidD_GetPhysicalDescriptor(_handle: usize, _buf: usize, _len: u32) -> u8 {
    crate::process::set_last_error(1); // ERROR_INVALID_FUNCTION
    0
}

pub unsafe extern "win64" fn HidD_GetPreparsedData(handle: usize, out: *mut usize) -> u8 {
    let Some((d, _)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let p = crate::dlls::hid_report::preparsed_into_raw(d.tlc());
    if crate::wow32::active() {
        // Programme 32 bits : la valeur est un pointeur de 4 octets.
        (out as *mut u32).write_unaligned(p as u32);
    } else {
        *out = p;
    }
    1
}

pub unsafe extern "win64" fn HidD_FreePreparsedData(data: usize) -> u8 {
    u8::from(crate::dlls::hid_report::preparsed_free(data))
}

/// Rapports par ioctl hidraw (HIDIOCSFEATURE, HIDIOCGFEATURE, HIDIOCGINPUT,
/// HIDIOCSOUTPUT) : _IOC(lecture|écriture, 'H', nr, taille).
unsafe fn report_ioctl(handle: usize, nr: u64, buf: *mut u8, len: u32) -> u8 {
    let Some((_, fd)) = device_of_handle(handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if fd < 0 {
        crate::process::set_last_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    if buf.is_null() || len == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let req = (3u64 << 30) | ((len as u64 & 0x3FFF) << 16) | ((b'H' as u64) << 8) | nr;
    if libc::ioctl(fd, req as _, buf) >= 0 {
        1
    } else {
        crate::process::set_last_error(ERROR_GEN_FAILURE);
        0
    }
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
/// HIDD_CONFIGURATION : rien de configurable côté Winlux.
pub extern "win64" fn HidD_GetConfiguration(handle: usize, _cfg: usize, _len: u32) -> u8 {
    u8::from(device_of_handle(handle).is_some())
}
pub extern "win64" fn HidD_SetConfiguration(handle: usize, _cfg: usize, _len: u32) -> u8 {
    u8::from(device_of_handle(handle).is_some())
}

pub fn exports() -> crate::dlls::Exports {
    let mut e = crate::exports! {
        "HidD_GetHidGuid" => fn HidD_GetHidGuid,
        "HidD_GetAttributes" => fn HidD_GetAttributes,
        "HidD_GetProductString" => fn HidD_GetProductString,
        "HidD_GetManufacturerString" => fn HidD_GetManufacturerString,
        "HidD_GetSerialNumberString" => fn HidD_GetSerialNumberString,
        "HidD_GetIndexedString" => fn HidD_GetIndexedString,
        "HidD_GetPhysicalDescriptor" => fn HidD_GetPhysicalDescriptor,
        "HidD_GetPreparsedData" => fn HidD_GetPreparsedData,
        "HidD_FreePreparsedData" => fn HidD_FreePreparsedData,
        "HidD_SetFeature" => fn HidD_SetFeature,
        "HidD_GetFeature" => fn HidD_GetFeature,
        "HidD_GetInputReport" => fn HidD_GetInputReport,
        "HidD_SetOutputReport" => fn HidD_SetOutputReport,
        "HidD_SetNumInputBuffers" => fn HidD_SetNumInputBuffers,
        "HidD_GetNumInputBuffers" => fn HidD_GetNumInputBuffers,
        "HidD_FlushQueue" => fn HidD_FlushQueue,
        "HidD_GetConfiguration" => fn HidD_GetConfiguration,
        "HidD_SetConfiguration" => fn HidD_SetConfiguration,
    };
    e.extend(crate::dlls::hid_report::exports());
    e
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
