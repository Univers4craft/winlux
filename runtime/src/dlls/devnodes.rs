//! Arbre des périphériques Plug and Play pour `setupapi` et `cfgmgr32`
//! (#271, partie 4).
//!
//! Les logiciels de cartes Arduino, d'imprimantes 3D, de radio et de
//! flashage cherchent leur port COM comme pyserial : `SetupDiGetClassDevs`
//! sur la classe « Ports », `SetupDiOpenDevRegKey` → valeur `PortName`,
//! identifiant d'instance, propriétés (`FRIENDLYNAME`, `HARDWAREID`…), puis
//! `CM_Get_Parent` + `CM_Get_Device_ID` pour lire le VID, le PID et le
//! numéro de série de l'appareil USB parent. Les manettes passent par
//! l'interface HID (`hid.rs`).
//!
//! Nœuds :
//! - `HTREE\ROOT\0`, la racine ;
//! - un port USB (`ttyUSB*`, `ttyACM*`) : `USB\VID_v&PID_p&MI_nn\…`, enfant
//!   de l'appareil `USB\VID_v&PID_p\<numéro de série>` lu dans sysfs ;
//! - un port intégré (`ttyS*`) : `ACPI\PNP0501\n` ; un `WINLUX_COMn` :
//!   `ROOT\PORTS\000n` ;
//! - un périphérique hidraw : `HID\VID_v&PID_p\HIDRAWn`.
//!
//! Un DEVINST est l'indice (plus un) de l'identifiant dans une table qui ne
//! fait que grandir pendant la vie du processus, comme sous Windows.
#![allow(non_snake_case)]

use crate::dlls::hid::{self, HidDevice, HID_GUID};
use crate::dlls::serial;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_INVALID_HANDLE: u32 = 6;

const CR_SUCCESS: u32 = 0;
const CR_INVALID_POINTER: u32 = 3;
const CR_NO_SUCH_DEVNODE: u32 = 0x0D;
const CR_BUFFER_SMALL: u32 = 0x1A;
const CR_NO_SUCH_VALUE: u32 = 0x25;

const DIGCF_ALLCLASSES: u32 = 0x4;
const DIGCF_DEVICEINTERFACE: u32 = 0x10;

/// {4D36E978-E325-11CE-BFC1-08002BE10318} : classe « Ports (COM et LPT) ».
pub const PORTS_CLASS: [u8; 16] = guid(
    0x4D36E978,
    0xE325,
    0x11CE,
    [0xBF, 0xC1, 0x08, 0x00, 0x2B, 0xE1, 0x03, 0x18],
);
/// {86E0D1E0-8089-11D0-9CE4-08003E301F73} : GUID_DEVINTERFACE_COMPORT.
pub const COMPORT_INTERFACE: [u8; 16] = guid(
    0x86E0D1E0,
    0x8089,
    0x11D0,
    [0x9C, 0xE4, 0x08, 0x00, 0x3E, 0x30, 0x1F, 0x73],
);
/// {745A17A0-74D3-11D0-B6FE-00A0C90F57DA} : classe HIDClass.
pub const HID_CLASS: [u8; 16] = guid(
    0x745A17A0,
    0x74D3,
    0x11D0,
    [0xB6, 0xFE, 0x00, 0xA0, 0xC9, 0x0F, 0x57, 0xDA],
);
/// {A5DCBF10-6530-11D2-901F-00C04FB951ED} : GUID_DEVINTERFACE_USB_DEVICE
/// (libusb, outils de flashage, winusb).
pub const USB_DEVICE_INTERFACE: [u8; 16] = guid(
    0xA5DCBF10,
    0x6530,
    0x11D2,
    [0x90, 0x1F, 0x00, 0xC0, 0x4F, 0xB9, 0x51, 0xED],
);
/// {36FC9E60-C465-11CF-8056-444553540000} : classe USB.
pub const USB_CLASS: [u8; 16] = guid(
    0x36FC9E60,
    0xC465,
    0x11CF,
    [0x80, 0x56, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00],
);

/// {50127DC3-0F36-415E-A6CC-4CB3BE910B65} : classe Processor (#1370).
pub const PROCESSOR_CLASS: [u8; 16] = guid(
    0x50127DC3,
    0x0F36,
    0x415E,
    [0xA6, 0xCC, 0x4C, 0xB3, 0xBE, 0x91, 0x0B, 0x65],
);

const fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let a = a.to_le_bytes();
    let b = b.to_le_bytes();
    let c = c.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6],
        d[7],
    ]
}

fn guid_text(g: &[u8; 16]) -> String {
    let a = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
    let b = u16::from_le_bytes([g[4], g[5]]);
    let c = u16::from_le_bytes([g[6], g[7]]);
    format!(
        "{{{a:08x}-{b:04x}-{c:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
    )
}

// ============================================================
//  Nœuds
// ============================================================

/// Appareil USB derrière un port série, lu dans sysfs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usb {
    pub vendor: u16,
    pub product: u16,
    pub serial: String,
    pub name: String,
    pub manufacturer: String,
    pub interface: u8,
    /// Chemin sysfs de l'appareil (sert d'identité quand il n'a pas de
    /// numéro de série).
    pub sysfs: String,
}

impl Usb {
    fn device_id(&self) -> String {
        let instance = if self.serial.is_empty() {
            // Comme Windows : identifiant tiré de l'emplacement.
            let tail = self.sysfs.rsplit('/').next().unwrap_or("0");
            format!("5&{}&0&0", tail.replace(['-', '.', ':'], ""))
        } else {
            self.serial.clone()
        };
        format!(
            "USB\\VID_{:04X}&PID_{:04X}\\{instance}",
            self.vendor, self.product
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Root,
    Hid(HidDevice),
    Port {
        port: serial::Port,
        usb: Option<Usb>,
    },
    UsbDevice(Usb),
    /// Processeur logique, tiré de /proc/cpuinfo (#1370).
    Processor(Cpu),
}

/// Processeur logique : numéro, nom (« model name ») et fabricant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cpu {
    pub index: u32,
    pub name: String,
    pub vendor: String,
}

/// Processeurs logiques décrits par le texte de /proc/cpuinfo. Sur ARM,
/// sans « model name », le nom vient de « Hardware » ou reste générique.
pub fn parse_cpuinfo(text: &str) -> Vec<Cpu> {
    let mut out: Vec<Cpu> = Vec::new();
    let mut hardware = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "processor" => out.push(Cpu {
                index: value.parse().unwrap_or(out.len() as u32),
                ..Cpu::default()
            }),
            "model name" | "cpu model" => {
                if let Some(c) = out.last_mut() {
                    c.name = value.split_whitespace().collect::<Vec<_>>().join(" ");
                }
            }
            "vendor_id" | "CPU implementer" => {
                if let Some(c) = out.last_mut() {
                    c.vendor = value.to_string();
                }
            }
            "Hardware" => hardware = value.to_string(),
            _ => {}
        }
    }
    for c in &mut out {
        if c.name.is_empty() {
            c.name = if hardware.is_empty() {
                "Processeur".into()
            } else {
                hardware.clone()
            };
        }
    }
    out
}

fn processors() -> Vec<Cpu> {
    let path = std::env::var_os("WINLUX_CPUINFO").unwrap_or_else(|| "/proc/cpuinfo".into());
    std::fs::read_to_string(path)
        .map(|t| parse_cpuinfo(&t))
        .unwrap_or_default()
}

/// Fabricant affiché : « GenuineIntel » → Intel, « AuthenticAMD » → AMD.
fn cpu_manufacturer(vendor: &str) -> &str {
    match vendor {
        "GenuineIntel" => "Intel",
        "AuthenticAMD" | "HygonGenuine" => "AMD",
        "" => "(Standard)",
        v => v,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub kind: Kind,
}

const ROOT: &str = "HTREE\\ROOT\\0";

fn read_trim(p: &Path) -> String {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Appareil USB d'un tty (`ttyUSB0`, `ttyACM0`) : on remonte depuis
/// `/sys/class/tty/<nom>/device` jusqu'au dossier qui porte `idVendor`.
pub fn usb_of_tty(name: &str) -> Option<Usb> {
    let start = std::fs::canonicalize(serial::sys_tty().join(name).join("device")).ok()?;
    let mut interface = 0u8;
    let mut dir: Option<&Path> = Some(&start);
    for _ in 0..8 {
        let d = dir?;
        if let Ok(n) = u8::from_str_radix(&read_trim(&d.join("bInterfaceNumber")), 16) {
            interface = n;
        }
        if d.join("idVendor").is_file() {
            let hex = |f: &str| u16::from_str_radix(&read_trim(&d.join(f)), 16).unwrap_or(0);
            return Some(Usb {
                vendor: hex("idVendor"),
                product: hex("idProduct"),
                serial: read_trim(&d.join("serial")),
                name: read_trim(&d.join("product")),
                manufacturer: read_trim(&d.join("manufacturer")),
                interface,
                sysfs: d.to_string_lossy().into_owned(),
            });
        }
        dir = d.parent();
    }
    None
}

fn port_node(port: &serial::Port) -> (Node, Option<Node>) {
    let file = Path::new(&port.linux)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let usb = (file.starts_with("ttyUSB") || file.starts_with("ttyACM"))
        .then(|| usb_of_tty(&file))
        .flatten();
    let (id, parent) = match &usb {
        Some(u) => {
            let parent = u.device_id();
            let inst = parent.rsplit('\\').next().unwrap_or("0").to_string();
            (
                format!(
                    "USB\\VID_{:04X}&PID_{:04X}&MI_{:02X}\\{inst}&{:04}",
                    u.vendor, u.product, u.interface, port.number
                ),
                Some(parent),
            )
        }
        None if file.starts_with("ttyS") => (
            format!("ACPI\\PNP0501\\{}", port.number - 1),
            Some(ROOT.to_string()),
        ),
        None => (
            format!("ROOT\\PORTS\\{:04}", port.number),
            Some(ROOT.to_string()),
        ),
    };
    let parent_node = usb.clone().map(|u| Node {
        id: u.device_id(),
        parent: Some(ROOT.to_string()),
        kind: Kind::UsbDevice(u),
    });
    (
        Node {
            id,
            parent,
            kind: Kind::Port {
                port: port.clone(),
                usb,
            },
        },
        parent_node,
    )
}

/// Tous les nœuds présents, la racine en premier.
pub fn nodes() -> Vec<Node> {
    let mut out = vec![Node {
        id: ROOT.into(),
        parent: None,
        kind: Kind::Root,
    }];
    for d in hid::devices() {
        out.push(Node {
            id: format!(
                "HID\\VID_{:04X}&PID_{:04X}\\HIDRAW{}",
                d.vendor, d.product, d.index
            ),
            parent: Some(ROOT.into()),
            kind: Kind::Hid(d),
        });
    }
    for p in serial::ports() {
        let (node, parent) = port_node(&p);
        if let Some(parent) = parent {
            if !out.iter().any(|n| n.id.eq_ignore_ascii_case(&parent.id)) {
                out.push(parent);
            }
        }
        out.push(node);
    }
    for c in processors() {
        out.push(Node {
            id: format!("ACPI\\ACPI0007\\{}", c.index),
            parent: Some(ROOT.into()),
            kind: Kind::Processor(c),
        });
    }
    // Les autres appareils USB (ceux que winusb ou libusb ouvrent).
    for u in usb_devices() {
        let id = u.device_id();
        if !out.iter().any(|n| n.id.eq_ignore_ascii_case(&id)) {
            out.push(Node {
                id,
                parent: Some(ROOT.into()),
                kind: Kind::UsbDevice(u),
            });
        }
    }
    out
}

/// Racine de sysfs (`WINLUX_SYSFS` pour les tests).
fn sysfs() -> PathBuf {
    std::env::var_os("WINLUX_SYSFS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys"))
}

/// Appareils de `/sys/bus/usb/devices` (sans les concentrateurs racine
/// `usbN` ni les interfaces `1-1:1.0`).
pub fn usb_devices() -> Vec<Usb> {
    let mut names: Vec<String> = std::fs::read_dir(sysfs().join("bus/usb/devices"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.contains(':') && !n.starts_with("usb"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .filter_map(|n| {
            let d = std::fs::canonicalize(sysfs().join("bus/usb/devices").join(n)).ok()?;
            let hex = |f: &str| u16::from_str_radix(&read_trim(&d.join(f)), 16).ok();
            Some(Usb {
                vendor: hex("idVendor")?,
                product: hex("idProduct").unwrap_or(0),
                serial: read_trim(&d.join("serial")),
                name: read_trim(&d.join("product")),
                manufacturer: read_trim(&d.join("manufacturer")),
                interface: 0,
                sysfs: d.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

/// Chemin d'interface `{a5dcbf10-…}` passé à CreateFile : dossier sysfs de
/// l'appareil USB qu'il désigne.
pub fn usb_device_target(path: &str) -> Option<PathBuf> {
    let lower = path.to_ascii_lowercase().replace("\\\\.\\", "\\\\?\\");
    if !lower.ends_with(&guid_text(&USB_DEVICE_INTERFACE)) {
        return None;
    }
    nodes().into_iter().find_map(|n| match &n.kind {
        Kind::UsbDevice(u) if n.interface_path()?.eq_ignore_ascii_case(&lower) => {
            Some(PathBuf::from(&u.sysfs))
        }
        _ => None,
    })
}

impl Node {
    fn class(&self) -> [u8; 16] {
        match self.kind {
            Kind::Hid(_) => HID_CLASS,
            Kind::Port { .. } => PORTS_CLASS,
            Kind::UsbDevice(_) | Kind::Root => USB_CLASS,
            Kind::Processor(_) => PROCESSOR_CLASS,
        }
    }
    fn interface(&self) -> Option<[u8; 16]> {
        match self.kind {
            Kind::Hid(_) => Some(HID_GUID),
            Kind::Port { .. } => Some(COMPORT_INTERFACE),
            Kind::UsbDevice(_) => Some(USB_DEVICE_INTERFACE),
            Kind::Root | Kind::Processor(_) => None,
        }
    }
    /// Chemin rendu par `SetupDiGetDeviceInterfaceDetailW`.
    pub fn interface_path(&self) -> Option<String> {
        match &self.kind {
            Kind::Hid(d) => Some(d.interface_path()),
            Kind::Port { .. } => Some(format!(
                "\\\\?\\{}#{}",
                self.id.to_ascii_lowercase().replace('\\', "#"),
                guid_text(&COMPORT_INTERFACE)
            )),
            Kind::UsbDevice(_) => Some(format!(
                "\\\\?\\{}#{}",
                self.id.to_ascii_lowercase().replace('\\', "#"),
                guid_text(&USB_DEVICE_INTERFACE)
            )),
            Kind::Root | Kind::Processor(_) => None,
        }
    }
    fn description(&self) -> String {
        match &self.kind {
            Kind::Root => "Racine".into(),
            Kind::Hid(d) if d.name.is_empty() => "Périphérique HID".into(),
            Kind::Hid(d) => d.name.clone(),
            Kind::Port { usb: Some(u), .. } if !u.name.is_empty() => u.name.clone(),
            Kind::Port { usb: Some(_), .. } => "USB Serial Device".into(),
            Kind::Port { .. } => "Communications Port".into(),
            Kind::UsbDevice(u) if !u.name.is_empty() => u.name.clone(),
            Kind::UsbDevice(_) => "USB Composite Device".into(),
            Kind::Processor(c) => c.name.clone(),
        }
    }
    /// Propriété SPDRP_* : (type de registre, chaîne(s) UTF-16 terminées).
    pub fn property(&self, prop: u32) -> Option<(u32, Vec<u16>)> {
        let sz = |s: &str| (1, wide(s));
        let multi = |v: &[String]| {
            let mut out: Vec<u16> = v.iter().flat_map(|s| wide(s)).collect();
            out.push(0);
            (7, out)
        };
        let enumerator = self.id.split('\\').next().unwrap_or("").to_string();
        Some(match prop {
            0 => sz(&self.description()),
            0xC => match &self.kind {
                Kind::Port { port, .. } => {
                    sz(&format!("{} (COM{})", self.description(), port.number))
                }
                _ => sz(&self.description()),
            },
            1 => match &self.kind {
                Kind::Hid(d) => multi(&[
                    format!("HID\\VID_{:04X}&PID_{:04X}", d.vendor, d.product),
                    "HID_DEVICE".into(),
                ]),
                Kind::Port { usb: Some(u), .. } => multi(&[
                    format!(
                        "USB\\VID_{:04X}&PID_{:04X}&MI_{:02X}",
                        u.vendor, u.product, u.interface
                    ),
                    format!("USB\\VID_{:04X}&PID_{:04X}", u.vendor, u.product),
                ]),
                Kind::UsbDevice(u) => {
                    multi(&[format!("USB\\VID_{:04X}&PID_{:04X}", u.vendor, u.product)])
                }
                Kind::Port { .. } if enumerator == "ACPI" => {
                    multi(&["ACPI\\PNP0501".into(), "*PNP0501".into()])
                }
                Kind::Port { .. } => multi(&["ROOT\\PORTS".into()]),
                Kind::Processor(_) => multi(&["ACPI\\ACPI0007".into(), "*ACPI0007".into()]),
                Kind::Root => return None,
            },
            4 => sz(match &self.kind {
                Kind::Hid(_) => "HidUsb",
                Kind::Port { usb: Some(_), .. } => "usbser",
                Kind::Port { .. } => "Serial",
                Kind::UsbDevice(_) => "usbccgp",
                Kind::Processor(c) if c.vendor == "GenuineIntel" => "intelppm",
                Kind::Processor(c) if c.vendor == "AuthenticAMD" => "amdppm",
                Kind::Processor(_) => "Processor",
                Kind::Root => return None,
            }),
            7 => sz(match self.kind {
                Kind::Hid(_) => "HIDClass",
                Kind::Port { .. } => "Ports",
                Kind::Processor(_) => "Processor",
                _ => "USB",
            }),
            8 => sz(&guid_text(&self.class())),
            0xB => match &self.kind {
                Kind::Hid(d) => sz(d.name.split_whitespace().next().unwrap_or("(Standard)")),
                Kind::Port { usb: Some(u), .. } | Kind::UsbDevice(u)
                    if !u.manufacturer.is_empty() =>
                {
                    sz(&u.manufacturer)
                }
                Kind::Port { .. } => sz("(Types de ports standard)"),
                Kind::Processor(c) => sz(cpu_manufacturer(&c.vendor)),
                _ => sz("(Standard)"),
            },
            0xE => match &self.kind {
                Kind::Port { port, .. } => sz(&port.nt),
                _ => return None,
            },
            0x16 => sz(&enumerator),
            _ => return None,
        })
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `\\?\usb#…#{86e0d1e0-…}` passé à CreateFile : le port COM qu'il désigne.
pub fn comport_alias(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    if !lower.ends_with(&guid_text(&COMPORT_INTERFACE)) {
        return None;
    }
    let lower = lower.replace("\\\\.\\", "\\\\?\\");
    nodes().into_iter().find_map(|n| match &n.kind {
        Kind::Port { port, .. } if n.interface_path()?.eq_ignore_ascii_case(&lower) => {
            Some(format!("\\\\.\\COM{}", port.number))
        }
        _ => None,
    })
}

// ============================================================
//  DEVINST
// ============================================================

static IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// DEVINST d'un identifiant (créé au premier besoin ; la racine vaut 1).
pub fn devinst(id: &str) -> u32 {
    let mut ids = crate::dlls::lock(&IDS);
    if ids.is_empty() {
        ids.push(ROOT.into());
    }
    if let Some(i) = ids.iter().position(|s| s.eq_ignore_ascii_case(id)) {
        return i as u32 + 1;
    }
    ids.push(id.to_string());
    ids.len() as u32
}

fn node_of(dn: u32) -> Option<Node> {
    let id = {
        let ids = crate::dlls::lock(&IDS);
        if dn == 1 && ids.is_empty() {
            ROOT.to_string()
        } else {
            ids.get((dn as usize).checked_sub(1)?)?.clone()
        }
    };
    nodes().into_iter().find(|n| n.id.eq_ignore_ascii_case(&id))
}

// ============================================================
//  Ensembles setupapi
// ============================================================

static SETS: Mutex<Vec<(usize, Vec<Node>)>> = Mutex::new(Vec::new());
static NEXT_SET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5E70_0001);

fn set_nodes(set: usize) -> Option<Vec<Node>> {
    crate::dlls::lock(&SETS)
        .iter()
        .find(|(h, _)| *h == set)
        .map(|(_, d)| d.clone())
}

/// `SetupDiGetClassDevs` : comme sous Windows, une classe sans appareil
/// donne un ensemble vide, pas INVALID_HANDLE_VALUE (#1370). `None` reste
/// pour un appel invalide (ni classe ni DIGCF_ALLCLASSES).
pub fn get_class_devs(
    guid: Option<[u8; 16]>,
    enumerator: Option<&str>,
    flags: u32,
) -> Option<usize> {
    let all = flags & DIGCF_ALLCLASSES != 0;
    let interfaces = flags & DIGCF_DEVICEINTERFACE != 0;
    let wanted = |n: &Node| -> bool {
        if matches!(n.kind, Kind::Root) {
            return false;
        }
        if all {
            return !interfaces || n.interface().is_some();
        }
        let Some(g) = guid else { return false };
        // L'interface HID est acceptée dans les deux modes (SDL, hidapi).
        n.interface() == Some(g) || (!interfaces && n.class() == g)
    };
    if !all && guid.is_none() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return None;
    }
    let list: Vec<Node> = nodes()
        .into_iter()
        .filter(wanted)
        .filter(|n| {
            enumerator.is_none_or(|e| {
                n.id.split('\\')
                    .next()
                    .is_some_and(|first| first.eq_ignore_ascii_case(e.trim_end_matches('\\')))
            })
        })
        .collect();
    let h = NEXT_SET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::dlls::lock(&SETS).push((h, list));
    crate::process::set_last_error(0);
    Some(h)
}

pub fn destroy(set: usize) -> bool {
    let mut sets = crate::dlls::lock(&SETS);
    let before = sets.len();
    sets.retain(|(h, _)| *h != set);
    sets.len() != before
}

/// Indice d'un SP_DEVINFO_DATA ou SP_DEVICE_INTERFACE_DATA (champ Reserved).
unsafe fn reserved_index(data: *const u8) -> usize {
    (data.add(24) as *const usize)
        .read_unaligned()
        .wrapping_sub(1)
}

unsafe fn write_devinfo(n: &Node, index: usize, data: *mut u8) {
    std::ptr::copy_nonoverlapping(n.class().as_ptr(), data.add(4), 16);
    (data.add(20) as *mut u32).write_unaligned(devinst(&n.id));
    (data.add(24) as *mut usize).write_unaligned(index + 1);
}

/// SP_DEVINFO_DATA : cbSize, ClassGuid, DevInst, Reserved.
pub unsafe fn enum_device_info(set: usize, index: u32, data: *mut u8) -> Option<bool> {
    let nodes = set_nodes(set)?;
    let Some(n) = nodes.get(index as usize) else {
        crate::process::set_last_error(ERROR_NO_MORE_ITEMS);
        return Some(false);
    };
    if !data.is_null() {
        write_devinfo(n, index as usize, data);
    }
    Some(true)
}

/// SP_DEVICE_INTERFACE_DATA : cbSize, InterfaceClassGuid, Flags, Reserved.
/// Avec un `devinfo`, seules les interfaces de cet appareil.
pub unsafe fn enum_interfaces(
    set: usize,
    devinfo: *const u8,
    guid: Option<[u8; 16]>,
    index: u32,
    data: *mut u8,
) -> Option<bool> {
    let nodes = set_nodes(set)?;
    let only = (!devinfo.is_null()).then(|| reserved_index(devinfo));
    let found = nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| only.is_none_or(|o| o == *i) && n.interface().is_some())
        .filter(|(_, n)| guid.is_none_or(|g| n.interface() == Some(g)))
        .nth(index as usize);
    let Some((i, n)) = found else {
        crate::process::set_last_error(ERROR_NO_MORE_ITEMS);
        return Some(false);
    };
    if !data.is_null() {
        let g = n.interface().unwrap_or_default();
        std::ptr::copy_nonoverlapping(g.as_ptr(), data.add(4), 16);
        (data.add(20) as *mut u32).write_unaligned(1); // SPINT_ACTIVE
        (data.add(24) as *mut usize).write_unaligned(i + 1);
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
    let nodes = set_nodes(set)?;
    if iface.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return Some(false);
    }
    let index = reserved_index(iface);
    let Some((n, path)) = nodes
        .get(index)
        .and_then(|n| Some((n, n.interface_path()?)))
    else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return Some(false);
    };
    let w = wide(&path);
    let needed = 4 + w.len() as u32 * 2;
    if !required.is_null() {
        *required = needed;
    }
    if !devinfo.is_null() {
        write_devinfo(n, index, devinfo);
    }
    if detail.is_null() || size < needed {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return Some(false);
    }
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, detail.add(4), w.len() * 2);
    Some(true)
}

unsafe fn node_in_set(set: usize, devinfo: *const u8) -> Option<Node> {
    if devinfo.is_null() {
        return None;
    }
    set_nodes(set)?.get(reserved_index(devinfo)).cloned()
}

pub unsafe fn registry_property(
    set: usize,
    devinfo: *const u8,
    prop: u32,
) -> Option<(u32, Vec<u16>)> {
    node_in_set(set, devinfo)?.property(prop)
}

/// Copie une chaîne UTF-16 terminée ; `required` en caractères.
unsafe fn put_wide(s: &str, buf: *mut u16, chars: u32, required: *mut u32) -> bool {
    let w = wide(s);
    if !required.is_null() {
        *required = w.len() as u32;
    }
    if buf.is_null() || (chars as usize) < w.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return false;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    true
}
unsafe fn put_ansi(s: &str, buf: *mut u8, chars: u32, required: *mut u32) -> bool {
    let mut a = crate::dlls::encode_ansi(s);
    a.push(0);
    if !required.is_null() {
        *required = a.len() as u32;
    }
    if buf.is_null() || (chars as usize) < a.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return false;
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, a.len());
    true
}

pub unsafe extern "win64" fn SetupDiGetDeviceInstanceIdW(
    set: usize,
    devinfo: *const u8,
    buf: *mut u16,
    chars: u32,
    required: *mut u32,
) -> i32 {
    match node_in_set(set, devinfo) {
        Some(n) => put_wide(&n.id, buf, chars, required) as i32,
        None => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}
pub unsafe extern "win64" fn SetupDiGetDeviceInstanceIdA(
    set: usize,
    devinfo: *const u8,
    buf: *mut u8,
    chars: u32,
    required: *mut u32,
) -> i32 {
    match node_in_set(set, devinfo) {
        Some(n) => put_ansi(&n.id, buf, chars, required) as i32,
        None => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

/// Clé `HKLM\SYSTEM\CurrentControlSet\Enum\<instance>\Device Parameters`
/// (`PortName` pour un port), créée à la demande.
pub unsafe extern "win64" fn SetupDiOpenDevRegKey(
    set: usize,
    devinfo: *const u8,
    _scope: u32,
    _profile: u32,
    keytype: u32,
    _access: u32,
) -> usize {
    let Some(n) = node_in_set(set, devinfo) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return usize::MAX;
    };
    let key = if keytype == 2 {
        // DIREG_DRV : clé du pilote, sous la classe.
        format!(
            "hklm\\system\\currentcontrolset\\control\\class\\{}\\{:04}",
            guid_text(&n.class()),
            devinst(&n.id)
        )
    } else {
        format!(
            "hklm\\system\\currentcontrolset\\enum\\{}\\device parameters",
            n.id.to_ascii_lowercase()
        )
    };
    let port = match &n.kind {
        Kind::Port { port, .. } => Some(port.number),
        _ => None,
    };
    let ok = crate::registry::write(|d| {
        d.ensure(&key);
        if let Some(p) = port {
            d.set(
                &key,
                "PortName",
                1,
                crate::registry::utf16z(&format!("COM{p}")),
            );
        }
        Ok(())
    });
    match ok.and_then(|_| crate::registry::open(&key)) {
        Ok(h) => h,
        Err(e) => {
            crate::process::set_last_error(e as u32);
            usize::MAX
        }
    }
}

unsafe fn class_guids(name: &str, list: *mut u8, size: u32, required: *mut u32) -> i32 {
    let g = match name.to_ascii_lowercase().as_str() {
        "ports" => Some(PORTS_CLASS),
        "hidclass" => Some(HID_CLASS),
        "usb" => Some(USB_CLASS),
        "processor" => Some(PROCESSOR_CLASS),
        _ => None,
    };
    let count = g.is_some() as u32;
    if !required.is_null() {
        *required = count;
    }
    if let Some(g) = g {
        if list.is_null() || size < 1 {
            crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
            return 0;
        }
        std::ptr::copy_nonoverlapping(g.as_ptr(), list, 16);
    }
    1
}
pub unsafe extern "win64" fn SetupDiClassGuidsFromNameW(
    name: *const u16,
    list: *mut u8,
    size: u32,
    required: *mut u32,
) -> i32 {
    if name.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    class_guids(&crate::dlls::wstr(name), list, size, required)
}
pub unsafe extern "win64" fn SetupDiClassGuidsFromNameA(
    name: *const u8,
    list: *mut u8,
    size: u32,
    required: *mut u32,
) -> i32 {
    if name.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    class_guids(&crate::dlls::cstr(name), list, size, required)
}

// ============================================================
//  cfgmgr32
// ============================================================

unsafe fn locate(out: *mut u32, id: Option<String>) -> u32 {
    if out.is_null() {
        return CR_INVALID_POINTER;
    }
    let id = match id.filter(|s| !s.is_empty()) {
        None => ROOT.to_string(),
        Some(s) => match nodes().into_iter().find(|n| n.id.eq_ignore_ascii_case(&s)) {
            Some(n) => n.id,
            None => {
                *out = 0;
                return CR_NO_SUCH_DEVNODE;
            }
        },
    };
    *out = devinst(&id);
    CR_SUCCESS
}
pub unsafe extern "win64" fn CM_Locate_DevNodeW(out: *mut u32, id: *const u16, _flags: u32) -> u32 {
    locate(out, (!id.is_null()).then(|| crate::dlls::wstr(id)))
}
pub unsafe extern "win64" fn CM_Locate_DevNodeA(out: *mut u32, id: *const u8, _flags: u32) -> u32 {
    locate(out, (!id.is_null()).then(|| crate::dlls::cstr(id)))
}

pub unsafe extern "win64" fn CM_Get_Parent(out: *mut u32, dn: u32, _flags: u32) -> u32 {
    if out.is_null() {
        return CR_INVALID_POINTER;
    }
    match node_of(dn).and_then(|n| n.parent) {
        Some(p) => {
            *out = devinst(&p);
            CR_SUCCESS
        }
        None => CR_NO_SUCH_DEVNODE,
    }
}

fn children(id: &str) -> Vec<String> {
    nodes()
        .into_iter()
        .filter(|n| {
            n.parent
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(id))
        })
        .map(|n| n.id)
        .collect()
}

pub unsafe extern "win64" fn CM_Get_Child(out: *mut u32, dn: u32, _flags: u32) -> u32 {
    if out.is_null() {
        return CR_INVALID_POINTER;
    }
    let Some(n) = node_of(dn) else {
        return CR_NO_SUCH_DEVNODE;
    };
    match children(&n.id).first() {
        Some(c) => {
            *out = devinst(c);
            CR_SUCCESS
        }
        None => CR_NO_SUCH_DEVNODE,
    }
}

pub unsafe extern "win64" fn CM_Get_Sibling(out: *mut u32, dn: u32, _flags: u32) -> u32 {
    if out.is_null() {
        return CR_INVALID_POINTER;
    }
    let Some(n) = node_of(dn) else {
        return CR_NO_SUCH_DEVNODE;
    };
    let Some(parent) = n.parent else {
        return CR_NO_SUCH_DEVNODE;
    };
    let kids = children(&parent);
    let next = kids
        .iter()
        .position(|k| k.eq_ignore_ascii_case(&n.id))
        .and_then(|i| kids.get(i + 1));
    match next {
        Some(s) => {
            *out = devinst(s);
            CR_SUCCESS
        }
        None => CR_NO_SUCH_DEVNODE,
    }
}

pub unsafe extern "win64" fn CM_Get_Device_IDW(
    dn: u32,
    buf: *mut u16,
    chars: u32,
    _flags: u32,
) -> u32 {
    let Some(n) = node_of(dn) else {
        return CR_NO_SUCH_DEVNODE;
    };
    if buf.is_null() {
        return CR_INVALID_POINTER;
    }
    if put_wide(&n.id, buf, chars, std::ptr::null_mut()) {
        CR_SUCCESS
    } else {
        CR_BUFFER_SMALL
    }
}
pub unsafe extern "win64" fn CM_Get_Device_IDA(
    dn: u32,
    buf: *mut u8,
    chars: u32,
    _flags: u32,
) -> u32 {
    let Some(n) = node_of(dn) else {
        return CR_NO_SUCH_DEVNODE;
    };
    if buf.is_null() {
        return CR_INVALID_POINTER;
    }
    if put_ansi(&n.id, buf, chars, std::ptr::null_mut()) {
        CR_SUCCESS
    } else {
        CR_BUFFER_SMALL
    }
}
/// Longueur de l'identifiant, sans le zéro final.
pub unsafe extern "win64" fn CM_Get_Device_ID_Size(out: *mut u32, dn: u32, _flags: u32) -> u32 {
    if out.is_null() {
        return CR_INVALID_POINTER;
    }
    match node_of(dn) {
        Some(n) => {
            *out = n.id.encode_utf16().count() as u32;
            CR_SUCCESS
        }
        None => CR_NO_SUCH_DEVNODE,
    }
}

/// DN_DRIVER_LOADED | DN_STARTED | DN_DISABLEABLE | DN_NT_ENUMERATOR | DN_NT_DRIVER.
pub unsafe extern "win64" fn CM_Get_DevNode_Status(
    status: *mut u32,
    problem: *mut u32,
    dn: u32,
    _flags: u32,
) -> u32 {
    if status.is_null() || problem.is_null() {
        return CR_INVALID_POINTER;
    }
    if node_of(dn).is_none() {
        return CR_NO_SUCH_DEVNODE;
    }
    *status = 0x0180_200A;
    *problem = 0;
    CR_SUCCESS
}

/// `CM_DRP_*` = `SPDRP_*` + 1.
pub unsafe extern "win64" fn CM_Get_DevNode_Registry_PropertyW(
    dn: u32,
    prop: u32,
    kind: *mut u32,
    buf: *mut u8,
    len: *mut u32,
    _flags: u32,
) -> u32 {
    let Some(n) = node_of(dn) else {
        return CR_NO_SUCH_DEVNODE;
    };
    if len.is_null() {
        return CR_INVALID_POINTER;
    }
    let Some((t, value)) = prop.checked_sub(1).and_then(|p| n.property(p)) else {
        return CR_NO_SUCH_VALUE;
    };
    let bytes = value.len() as u32 * 2;
    if !kind.is_null() {
        *kind = t;
    }
    let room = *len;
    *len = bytes;
    if buf.is_null() || room < bytes {
        return CR_BUFFER_SMALL;
    }
    std::ptr::copy_nonoverlapping(value.as_ptr() as *const u8, buf, bytes as usize);
    CR_SUCCESS
}

/// MULTI_SZ des chemins d'interface d'une classe d'interface.
fn interface_list(guid: [u8; 16], device: Option<&str>) -> Vec<u16> {
    let mut out: Vec<u16> = nodes()
        .iter()
        .filter(|n| n.interface() == Some(guid))
        .filter(|n| device.is_none_or(|d| d.is_empty() || n.id.eq_ignore_ascii_case(d)))
        .filter_map(|n| n.interface_path())
        .flat_map(|p| wide(&p))
        .collect();
    out.push(0);
    out
}
unsafe fn read_guid(g: *const u8) -> Option<[u8; 16]> {
    (!g.is_null()).then(|| std::ptr::read_unaligned(g as *const [u8; 16]))
}
pub unsafe extern "win64" fn CM_Get_Device_Interface_List_SizeW(
    out: *mut u32,
    guid: *const u8,
    device: *const u16,
    _flags: u32,
) -> u32 {
    let (Some(g), false) = (read_guid(guid), out.is_null()) else {
        return CR_INVALID_POINTER;
    };
    let dev = (!device.is_null()).then(|| crate::dlls::wstr(device));
    *out = interface_list(g, dev.as_deref()).len() as u32;
    CR_SUCCESS
}
pub unsafe extern "win64" fn CM_Get_Device_Interface_ListW(
    guid: *const u8,
    device: *const u16,
    buf: *mut u16,
    chars: u32,
    _flags: u32,
) -> u32 {
    let (Some(g), false) = (read_guid(guid), buf.is_null()) else {
        return CR_INVALID_POINTER;
    };
    let dev = (!device.is_null()).then(|| crate::dlls::wstr(device));
    let list = interface_list(g, dev.as_deref());
    if (chars as usize) < list.len() {
        return CR_BUFFER_SMALL;
    }
    std::ptr::copy_nonoverlapping(list.as_ptr(), buf, list.len());
    CR_SUCCESS
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupDiGetDeviceInstanceIdW" => fn SetupDiGetDeviceInstanceIdW,
        "SetupDiGetDeviceInstanceIdA" => fn SetupDiGetDeviceInstanceIdA,
        "SetupDiOpenDevRegKey" => fn SetupDiOpenDevRegKey,
        "SetupDiClassGuidsFromNameW" => fn SetupDiClassGuidsFromNameW,
        "SetupDiClassGuidsFromNameA" => fn SetupDiClassGuidsFromNameA,
        "CM_Locate_DevNodeW" => fn CM_Locate_DevNodeW,
        "CM_Locate_DevNodeA" => fn CM_Locate_DevNodeA,
        "CM_Get_Parent" => fn CM_Get_Parent,
        "CM_Get_Child" => fn CM_Get_Child,
        "CM_Get_Sibling" => fn CM_Get_Sibling,
        "CM_Get_Device_IDW" => fn CM_Get_Device_IDW,
        "CM_Get_Device_IDA" => fn CM_Get_Device_IDA,
        "CM_Get_Device_ID_Size" => fn CM_Get_Device_ID_Size,
        "CM_Get_DevNode_Status" => fn CM_Get_DevNode_Status,
        "CM_Get_DevNode_Registry_PropertyW" => fn CM_Get_DevNode_Registry_PropertyW,
        "CM_Get_Device_Interface_List_SizeW" => fn CM_Get_Device_Interface_List_SizeW,
        "CM_Get_Device_Interface_ListW" => fn CM_Get_Device_Interface_ListW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classe_processor_depuis_cpuinfo() {
        let text = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM)  i7-1165G7 @ 2.80GHz\n\n\
                    processor\t: 1\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM)  i7-1165G7 @ 2.80GHz\n";
        let cpus = parse_cpuinfo(text);
        assert_eq!(cpus.len(), 2);
        assert_eq!(cpus[1].index, 1);
        assert_eq!(cpus[0].name, "Intel(R) Core(TM) i7-1165G7 @ 2.80GHz");
        assert_eq!(
            guid_text(&PROCESSOR_CLASS).to_ascii_lowercase(),
            "{50127dc3-0f36-415e-a6cc-4cb3be910b65}"
        );
        // ARM sans « model name » : nom générique.
        assert_eq!(
            parse_cpuinfo("processor : 0\nCPU implementer : 0x41\n")[0].name,
            "Processeur"
        );

        // Nœud Processor : nom lisible, classe et fabricant.
        let node = Node {
            id: "ACPI\\ACPI0007\\0".into(),
            parent: Some(ROOT.into()),
            kind: Kind::Processor(cpus[0].clone()),
        };
        let name = node.property(0xC).unwrap().1;
        assert_eq!(
            String::from_utf16_lossy(&name[..name.len() - 1]),
            cpus[0].name
        );
        assert_eq!(node.property(7).unwrap().1, wide("Processor"));
        assert_eq!(node.property(0xB).unwrap().1, wide("Intel"));
        assert_eq!(node.class(), PROCESSOR_CLASS);
        // Classe Processor : un appareil par processeur logique de la machine
        // (sans variable d'environnement : les tests tournent en parallèle).
        let expected = std::fs::read_to_string("/proc/cpuinfo")
            .map(|t| parse_cpuinfo(&t).len())
            .unwrap_or(0);
        let set = get_class_devs(Some(PROCESSOR_CLASS), None, 0x2).expect("ensemble");
        assert_eq!(set_nodes(set).unwrap().len(), expected);
        destroy(set);
        // Classe sans appareil : ensemble vide, pas INVALID_HANDLE_VALUE.
        let unknown = guid(
            0x4D36E968,
            0xE325,
            0x11CE,
            [0xBF, 0xC1, 0x08, 0x00, 0x2B, 0xE1, 0x03, 0x18],
        );
        let set = get_class_devs(Some(unknown), None, 0x2).expect("ensemble vide");
        assert!(set_nodes(set).unwrap().is_empty());
        destroy(set);
        assert!(get_class_devs(None, None, 0x2).is_none());
    }

    #[test]
    fn guids_au_format_windows() {
        assert_eq!(
            guid_text(&PORTS_CLASS),
            "{4d36e978-e325-11ce-bfc1-08002be10318}"
        );
        assert_eq!(
            guid_text(&COMPORT_INTERFACE),
            "{86e0d1e0-8089-11d0-9ce4-08003e301f73}"
        );
        assert_eq!(
            guid_text(&HID_GUID),
            "{4d1e55b2-f16f-11cf-88cb-001111000030}"
        );
    }

    #[test]
    fn port_usb_et_son_parent() {
        let usb = Usb {
            vendor: 0x2341,
            product: 0x0043,
            serial: "SN123".into(),
            name: "Arduino Uno".into(),
            manufacturer: "Arduino".into(),
            interface: 0,
            sysfs: "/sys/devices/usb1/1-1".into(),
        };
        let n = Node {
            id: "USB\\VID_2341&PID_0043&MI_00\\SN123&0005".into(),
            parent: Some(usb.device_id()),
            kind: Kind::Port {
                port: serial::Port {
                    number: 5,
                    linux: "/dev/ttyACM0".into(),
                    nt: "\\Device\\USBSER000".into(),
                },
                usb: Some(usb.clone()),
            },
        };
        assert_eq!(usb.device_id(), "USB\\VID_2341&PID_0043\\SN123");
        assert_eq!(n.property(0xC).unwrap().1, wide("Arduino Uno (COM5)"));
        assert_eq!(n.property(4).unwrap().1, wide("usbser"));
        assert_eq!(
            n.interface_path().unwrap(),
            "\\\\?\\usb#vid_2341&pid_0043&mi_00#sn123&0005#{86e0d1e0-8089-11d0-9ce4-08003e301f73}"
        );
        let ids = n.property(1).unwrap();
        assert_eq!(ids.0, 7);
        assert_eq!(*ids.1.last().unwrap(), 0);
    }

    #[test]
    fn devinst_stable_et_racine_a_un() {
        assert_eq!(devinst(ROOT), 1);
        let a = devinst("TEST\\DEVNODES\\A");
        assert_eq!(devinst("test\\devnodes\\a"), a);
        assert_ne!(devinst("TEST\\DEVNODES\\B"), a);
    }

    #[test]
    fn sans_numero_de_serie_l_emplacement_sert_d_identite() {
        let u = Usb {
            vendor: 0x1A86,
            product: 0x7523,
            sysfs: "/sys/devices/pci0000:00/usb1/1-2.3".into(),
            ..Default::default()
        };
        assert_eq!(u.device_id(), "USB\\VID_1A86&PID_7523\\5&123&0&0");
    }
}
