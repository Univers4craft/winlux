//! `winusb.dll` sur usbfs (#271, partie 5) : l'accès USB direct des outils
//! de flashage (cartes, claviers programmables, microcontrôleurs), des
//! programmes de radio logicielle et de libusb sous Windows.
//!
//! Le programme trouve l'appareil par setupapi (`GUID_DEVINTERFACE_USB_DEVICE`,
//! voir `devnodes.rs`), l'ouvre avec `CreateFileW`, puis `WinUsb_Initialize`.
//! Le handle de fichier est `/dev/bus/usb/BBB/DDD` (usbfs). Sans droit sur
//! usbfs, c'est le fichier `descriptors` de sysfs, toujours lisible : les
//! descripteurs, la vitesse, les interfaces et les points de terminaison
//! restent consultables, et les transferts échouent proprement
//! (ERROR_ACCESS_DENIED).
//!
//! Transferts : `WinUsb_ControlTransfer` (USBDEVFS_CONTROL),
//! `WinUsb_ReadPipe`/`WritePipe` (USBDEVFS_BULK, aussi pour l'interruption),
//! délai par `PIPE_TRANSFER_TIMEOUT`, `ResetPipe` (CLEAR_HALT), réglage
//! alternatif (SETINTERFACE). Un OVERLAPPED est terminé tout de suite.
//! Un pilote du noyau déjà lié à l'interface n'est jamais détaché :
//! `WinUsb_Initialize` échoue alors avec ERROR_BUSY, comme quand un autre
//! pilote tient l'appareil sous Windows.
#![allow(non_snake_case)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_GEN_FAILURE: u32 = 31;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_SEM_TIMEOUT: u32 = 121;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_BUSY: u32 = 170;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;

// ioctl usbfs (linux/usbdevice_fs.h), en 64 bits.
const USBDEVFS_CONTROL: u64 = 0xC018_5500;
const USBDEVFS_BULK: u64 = 0xC018_5502;
const USBDEVFS_SETINTERFACE: u64 = 0x8008_5504;
const USBDEVFS_CLAIMINTERFACE: u64 = 0x8004_550F;
const USBDEVFS_RELEASEINTERFACE: u64 = 0x8004_5510;
const USBDEVFS_CLEAR_HALT: u64 = 0x8004_5515;

fn dev_root() -> PathBuf {
    std::env::var_os("WINLUX_DEV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev"))
}

/// Appareil ouvert par CreateFileW (repéré par l'inode du fichier).
struct Opened {
    inode: u64,
    sysfs: PathBuf,
    usbfs: bool,
}
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());

fn inode_of(fd: i32) -> Option<u64> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some(st.st_ino as u64)
}

/// Ouvre l'appareil USB dont `sysfs` est le dossier : usbfs si possible,
/// sinon le fichier `descriptors`.
pub fn open(sysfs: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let num = |f: &str| {
        std::fs::read_to_string(sysfs.join(f))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    };
    let node = match (num("busnum"), num("devnum")) {
        (Some(b), Some(d)) => Some(dev_root().join(format!("bus/usb/{b:03}/{d:03}"))),
        _ => None,
    };
    let (file, usbfs) = match node.and_then(|n| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(n)
            .ok()
    }) {
        Some(f) => (f, true),
        None => (std::fs::File::open(sysfs.join("descriptors"))?, false),
    };
    if let Some(inode) = inode_of(file.as_raw_fd()) {
        let mut opened = crate::dlls::lock(&OPENED);
        opened.retain(|o| o.inode != inode);
        opened.push(Opened {
            inode,
            sysfs: sysfs.to_path_buf(),
            usbfs,
        });
    }
    Ok(file)
}

/// Interface WinUSB (un handle par interface réclamée).
#[derive(Clone)]
struct Iface {
    device: usize,
    fd: i32,
    sysfs: PathBuf,
    usbfs: bool,
    number: u8,
    alt: u8,
    timeouts: Vec<(u8, u32)>,
}
static IFACES: Mutex<Vec<(usize, Iface)>> = Mutex::new(Vec::new());
static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x75B0_0001);

fn iface(h: usize) -> Option<Iface> {
    crate::dlls::lock(&IFACES)
        .iter()
        .find(|(k, _)| *k == h)
        .map(|(_, i)| i.clone())
}

fn fail(e: u32) -> i32 {
    crate::process::set_last_error(e);
    0
}

fn errno_to_win32(e: i32) -> u32 {
    match e {
        libc::ETIMEDOUT => ERROR_SEM_TIMEOUT,
        libc::EBUSY => ERROR_BUSY,
        libc::EACCES | libc::EPERM => ERROR_ACCESS_DENIED,
        libc::ENODEV | libc::ESHUTDOWN => ERROR_DEVICE_NOT_CONNECTED,
        libc::EINVAL => ERROR_INVALID_PARAMETER,
        _ => ERROR_GEN_FAILURE,
    }
}

unsafe fn claim(fd: i32, n: u8) -> Result<(), u32> {
    let mut v = n as u32;
    if libc::ioctl(fd, USBDEVFS_CLAIMINTERFACE as _, &mut v) < 0 {
        return Err(errno_to_win32(*libc::__errno_location()));
    }
    Ok(())
}

fn new_iface(i: Iface) -> usize {
    let h = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::dlls::lock(&IFACES).push((h, i));
    h
}

pub unsafe extern "win64" fn WinUsb_Initialize(device: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let fd = crate::dlls::kernel32::handle_to_fd(device);
    let Some(inode) = (fd >= 0).then(|| inode_of(fd)).flatten() else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let Some((sysfs, usbfs)) = crate::dlls::lock(&OPENED)
        .iter()
        .find(|o| o.inode == inode)
        .map(|o| (o.sysfs.clone(), o.usbfs))
    else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let first = descriptors(&sysfs)
        .and_then(|d| interfaces(&d).first().map(|i| i.number))
        .unwrap_or(0);
    if usbfs {
        if let Err(e) = claim(fd, first) {
            return fail(e);
        }
    }
    *out = new_iface(Iface {
        device,
        fd,
        sysfs,
        usbfs,
        number: first,
        alt: 0,
        timeouts: Vec::new(),
    });
    1
}

pub unsafe extern "win64" fn WinUsb_Free(h: usize) -> i32 {
    let mut list = crate::dlls::lock(&IFACES);
    let Some(pos) = list.iter().position(|(k, _)| *k == h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let (_, i) = list.remove(pos);
    if i.usbfs {
        let mut v = i.number as u32;
        libc::ioctl(i.fd, USBDEVFS_RELEASEINTERFACE as _, &mut v);
    }
    1
}

/// `index` 0 : l'interface qui suit celle de `h`, et ainsi de suite.
pub unsafe extern "win64" fn WinUsb_GetAssociatedInterface(
    h: usize,
    index: u8,
    out: *mut usize,
) -> i32 {
    let Some(base) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let list: Vec<u8> = descriptors(&base.sysfs)
        .map(|d| {
            let mut v: Vec<u8> = interfaces(&d).iter().map(|i| i.number).collect();
            v.dedup();
            v
        })
        .unwrap_or_default();
    let Some(n) = list
        .iter()
        .position(|&n| n == base.number)
        .and_then(|p| list.get(p + 1 + index as usize))
        .copied()
    else {
        return fail(ERROR_NO_MORE_ITEMS);
    };
    if base.usbfs {
        if let Err(e) = claim(base.fd, n) {
            return fail(e);
        }
    }
    *out = new_iface(Iface {
        number: n,
        alt: 0,
        timeouts: Vec::new(),
        ..base
    });
    1
}

// ============================================================
//  Descripteurs (fichier `descriptors` de sysfs)
// ============================================================

fn descriptors(sysfs: &Path) -> Option<Vec<u8>> {
    std::fs::read(sysfs.join("descriptors"))
        .ok()
        .filter(|d| d.len() >= 18)
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfaceDesc {
    pub raw: [u8; 9],
    pub number: u8,
    pub alt: u8,
    pub endpoints: Vec<[u8; 7]>,
}

/// Interfaces (et leurs points de terminaison) de la première configuration.
pub fn interfaces(d: &[u8]) -> Vec<IfaceDesc> {
    let mut out: Vec<IfaceDesc> = Vec::new();
    let mut p = d.first().map(|&l| l as usize).unwrap_or(18);
    while p + 2 <= d.len() {
        let len = d[p] as usize;
        if len < 2 || p + len > d.len() {
            break;
        }
        match d[p + 1] {
            2 if !out.is_empty() => break, // configuration suivante
            4 if len >= 9 => {
                let mut raw = [0u8; 9];
                raw.copy_from_slice(&d[p..p + 9]);
                out.push(IfaceDesc {
                    raw,
                    number: raw[2],
                    alt: raw[3],
                    endpoints: Vec::new(),
                });
            }
            5 if len >= 7 => {
                if let Some(last) = out.last_mut() {
                    let mut e = [0u8; 7];
                    e.copy_from_slice(&d[p..p + 7]);
                    last.endpoints.push(e);
                }
            }
            _ => {}
        }
        p += len;
    }
    out
}

/// Première configuration complète (wTotalLength).
fn config(d: &[u8]) -> Option<&[u8]> {
    let start = *d.first()? as usize;
    let c = d.get(start..start + 4)?;
    let total = u16::from_le_bytes([c[2], c[3]]) as usize;
    d.get(start..start + total.max(9))
}

fn string_descriptor(i: &Iface, d: &[u8], index: u8) -> Option<Vec<u8>> {
    if index == 0 {
        return Some(vec![4, 3, 0x09, 0x04]);
    }
    let file = if index == d[14] {
        "manufacturer"
    } else if index == d[15] {
        "product"
    } else if index == d[16] {
        "serial"
    } else {
        return None;
    };
    let text = std::fs::read_to_string(i.sysfs.join(file)).ok()?;
    let units: Vec<u8> = text
        .trim()
        .encode_utf16()
        .take(126)
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut v = vec![2 + units.len() as u8, 3];
    v.extend(units);
    Some(v)
}

/// Descripteur servi sans usbfs : appareil, configuration, chaînes.
fn cached_descriptor(i: &Iface, kind: u8, index: u8) -> Option<Vec<u8>> {
    let d = descriptors(&i.sysfs)?;
    match kind {
        1 => Some(d[..18].to_vec()),
        2 if index == 0 => config(&d).map(|c| c.to_vec()),
        3 => string_descriptor(i, &d, index),
        _ => None,
    }
}

unsafe fn put(data: &[u8], buf: *mut u8, len: u32, transferred: *mut u32) -> i32 {
    let n = data.len().min(len as usize);
    if buf.is_null() && n > 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if n > 0 {
        std::ptr::copy_nonoverlapping(data.as_ptr(), buf, n);
    }
    if !transferred.is_null() {
        *transferred = n as u32;
    }
    1
}

pub unsafe extern "win64" fn WinUsb_GetDescriptor(
    h: usize,
    kind: u8,
    index: u8,
    lang: u16,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if let Some(d) = cached_descriptor(&i, kind, index) {
        return put(&d, buf, len, transferred);
    }
    let setup = 0x80
        | (6u64 << 8)
        | (((kind as u64) << 8 | index as u64) << 16)
        | ((lang as u64) << 32)
        | ((len.min(0xFFFF) as u64) << 48);
    control(&i, setup, buf, len, transferred)
}

pub unsafe extern "win64" fn WinUsb_QueryDeviceInformation(
    h: usize,
    kind: u32,
    len: *mut u32,
    buf: *mut u8,
) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if kind != 1 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if len.is_null() || buf.is_null() || *len < 1 {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    // DEVICE_SPEED : LowSpeed 1, FullSpeed 2, HighSpeed 3 (et au-delà).
    let mbps: f64 = std::fs::read_to_string(i.sysfs.join("speed"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(12.0);
    *buf = if mbps < 2.0 {
        1
    } else if mbps < 100.0 {
        2
    } else {
        3
    };
    *len = 1;
    1
}

fn current(i: &Iface, alt: u8) -> Option<IfaceDesc> {
    interfaces(&descriptors(&i.sysfs)?)
        .into_iter()
        .find(|d| d.number == i.number && d.alt == alt)
}

pub unsafe extern "win64" fn WinUsb_QueryInterfaceSettings(h: usize, alt: u8, out: *mut u8) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match current(&i, alt) {
        Some(d) => {
            std::ptr::copy_nonoverlapping(d.raw.as_ptr(), out, 9);
            1
        }
        None => fail(ERROR_NO_MORE_ITEMS),
    }
}

/// WINUSB_PIPE_INFORMATION : PipeType (u32), PipeId, MaximumPacketSize, Interval.
pub unsafe extern "win64" fn WinUsb_QueryPipe(h: usize, alt: u8, index: u8, out: *mut u8) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let Some(e) = current(&i, alt).and_then(|d| d.endpoints.get(index as usize).copied()) else {
        return fail(ERROR_NO_MORE_ITEMS);
    };
    std::ptr::write_bytes(out, 0, 12);
    (out as *mut u32).write_unaligned((e[3] & 3) as u32);
    *out.add(4) = e[2];
    (out.add(6) as *mut u16).write_unaligned(u16::from_le_bytes([e[4], e[5]]) & 0x7FF);
    *out.add(8) = e[6];
    1
}

pub unsafe extern "win64" fn WinUsb_GetCurrentAlternateSetting(h: usize, out: *mut u8) -> i32 {
    match iface(h) {
        Some(i) if !out.is_null() => {
            *out = i.alt;
            1
        }
        Some(_) => fail(ERROR_INVALID_PARAMETER),
        None => fail(ERROR_INVALID_HANDLE),
    }
}

pub unsafe extern "win64" fn WinUsb_SetCurrentAlternateSetting(h: usize, alt: u8) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if current(&i, alt).is_none() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if i.usbfs {
        let mut s = [i.number as u32, alt as u32];
        if libc::ioctl(i.fd, USBDEVFS_SETINTERFACE as _, s.as_mut_ptr()) < 0 {
            return fail(errno_to_win32(*libc::__errno_location()));
        }
    }
    if let Some((_, x)) = crate::dlls::lock(&IFACES).iter_mut().find(|(k, _)| *k == h) {
        x.alt = alt;
    }
    1
}

// ============================================================
//  Politiques et transferts
// ============================================================

const PIPE_TRANSFER_TIMEOUT: u32 = 3;

pub unsafe extern "win64" fn WinUsb_SetPipePolicy(
    h: usize,
    pipe: u8,
    kind: u32,
    len: u32,
    value: *const u8,
) -> i32 {
    let mut list = crate::dlls::lock(&IFACES);
    let Some((_, i)) = list.iter_mut().find(|(k, _)| *k == h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if kind == PIPE_TRANSFER_TIMEOUT {
        if len < 4 || value.is_null() {
            return fail(ERROR_INVALID_PARAMETER);
        }
        let ms = (value as *const u32).read_unaligned();
        i.timeouts.retain(|(p, _)| *p != pipe);
        i.timeouts.push((pipe, ms));
    }
    1
}

pub unsafe extern "win64" fn WinUsb_GetPipePolicy(
    h: usize,
    pipe: u8,
    kind: u32,
    len: *mut u32,
    value: *mut u8,
) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if len.is_null() || value.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if kind == PIPE_TRANSFER_TIMEOUT {
        if *len < 4 {
            return fail(ERROR_INSUFFICIENT_BUFFER);
        }
        (value as *mut u32).write_unaligned(timeout(&i, pipe));
        *len = 4;
    } else {
        if *len < 1 {
            return fail(ERROR_INSUFFICIENT_BUFFER);
        }
        *value = 0;
        *len = 1;
    }
    1
}

pub unsafe extern "win64" fn WinUsb_SetPowerPolicy(
    h: usize,
    _kind: u32,
    _len: u32,
    _v: usize,
) -> i32 {
    if iface(h).is_none() {
        return fail(ERROR_INVALID_HANDLE);
    }
    1
}

fn timeout(i: &Iface, pipe: u8) -> u32 {
    i.timeouts
        .iter()
        .find(|(p, _)| *p == pipe)
        .map(|(_, t)| *t)
        .unwrap_or(0)
}

unsafe fn finish(
    i: &Iface,
    overlapped: usize,
    result: Result<u32, u32>,
    transferred: *mut u32,
) -> i32 {
    match result {
        Ok(n) => {
            if !transferred.is_null() {
                *transferred = n;
            }
            if overlapped != 0 {
                crate::dlls::kernel32::complete_overlapped(i.device, overlapped, n, 0);
            }
            crate::process::set_last_error(0);
            1
        }
        Err(e) => fail(e),
    }
}

/// WINUSB_SETUP_PACKET (8 octets) passé par valeur, comme un u64.
unsafe fn control(i: &Iface, setup: u64, buf: *mut u8, len: u32, transferred: *mut u32) -> i32 {
    control_overlapped(i, setup, buf, len, transferred, 0)
}

unsafe fn control_overlapped(
    i: &Iface,
    setup: u64,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
    overlapped: usize,
) -> i32 {
    let b = setup.to_le_bytes();
    let length = u16::from_le_bytes([b[6], b[7]]).min(len.min(0xFFFF) as u16);
    // Sans usbfs : GET_DESCRIPTOR standard servi depuis sysfs.
    if !i.usbfs {
        if b[0] == 0x80 && b[1] == 6 {
            if let Some(d) = cached_descriptor(i, b[3], b[2]) {
                let n = d.len().min(length as usize);
                if n > 0 && !buf.is_null() {
                    std::ptr::copy_nonoverlapping(d.as_ptr(), buf, n);
                }
                return finish(i, overlapped, Ok(n as u32), transferred);
            }
        }
        return fail(ERROR_ACCESS_DENIED);
    }
    #[repr(C)]
    struct Ctrl {
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
        timeout: u32,
        data: *mut u8,
    }
    let mut c = Ctrl {
        request_type: b[0],
        request: b[1],
        value: u16::from_le_bytes([b[2], b[3]]),
        index: u16::from_le_bytes([b[4], b[5]]),
        length,
        timeout: timeout(i, 0).max(5000),
        data: buf,
    };
    let r = libc::ioctl(i.fd, USBDEVFS_CONTROL as _, &mut c);
    let result = if r < 0 {
        Err(errno_to_win32(*libc::__errno_location()))
    } else {
        Ok(r as u32)
    };
    finish(i, overlapped, result, transferred)
}

pub unsafe extern "win64" fn WinUsb_ControlTransfer(
    h: usize,
    setup: u64,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
    overlapped: usize,
) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    control_overlapped(&i, setup, buf, len, transferred, overlapped)
}

unsafe fn bulk(
    h: usize,
    pipe: u8,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
    overlapped: usize,
) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if !i.usbfs {
        return fail(ERROR_ACCESS_DENIED);
    }
    if buf.is_null() && len > 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    #[repr(C)]
    struct Bulk {
        ep: u32,
        len: u32,
        timeout: u32,
        data: *mut u8,
    }
    let mut b = Bulk {
        ep: pipe as u32,
        len,
        timeout: timeout(&i, pipe),
        data: buf,
    };
    let r = libc::ioctl(i.fd, USBDEVFS_BULK as _, &mut b);
    let result = if r < 0 {
        Err(errno_to_win32(*libc::__errno_location()))
    } else {
        Ok(r as u32)
    };
    finish(&i, overlapped, result, transferred)
}

pub unsafe extern "win64" fn WinUsb_ReadPipe(
    h: usize,
    pipe: u8,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
    overlapped: usize,
) -> i32 {
    if pipe & 0x80 == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    bulk(h, pipe, buf, len, transferred, overlapped)
}

pub unsafe extern "win64" fn WinUsb_WritePipe(
    h: usize,
    pipe: u8,
    buf: *mut u8,
    len: u32,
    transferred: *mut u32,
    overlapped: usize,
) -> i32 {
    if pipe & 0x80 != 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    bulk(h, pipe, buf, len, transferred, overlapped)
}

pub unsafe extern "win64" fn WinUsb_ResetPipe(h: usize, pipe: u8) -> i32 {
    let Some(i) = iface(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if i.usbfs {
        let mut ep = pipe as u32;
        if libc::ioctl(i.fd, USBDEVFS_CLEAR_HALT as _, &mut ep) < 0 {
            return fail(errno_to_win32(*libc::__errno_location()));
        }
    }
    1
}

/// AbortPipe et FlushPipe : les transferts sont synchrones, rien en attente.
pub extern "win64" fn WinUsb_AbortPipe(h: usize, _pipe: u8) -> i32 {
    if iface(h).is_none() {
        return fail(ERROR_INVALID_HANDLE);
    }
    1
}

pub extern "win64" fn WinUsb_GetOverlappedResult(
    h: usize,
    overlapped: *const usize,
    transferred: *mut u32,
    _wait: i32,
) -> i32 {
    if iface(h).is_none() || overlapped.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    unsafe {
        if !transferred.is_null() {
            *transferred = *overlapped.add(1) as u32;
        }
    }
    1
}

pub extern "win64" fn not_supported() -> i32 {
    fail(ERROR_NOT_SUPPORTED)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinUsb_Initialize" => fn WinUsb_Initialize,
        "WinUsb_Free" => fn WinUsb_Free,
        "WinUsb_GetAssociatedInterface" => fn WinUsb_GetAssociatedInterface,
        "WinUsb_GetDescriptor" => fn WinUsb_GetDescriptor,
        "WinUsb_QueryDeviceInformation" => fn WinUsb_QueryDeviceInformation,
        "WinUsb_QueryInterfaceSettings" => fn WinUsb_QueryInterfaceSettings,
        "WinUsb_QueryPipe" => fn WinUsb_QueryPipe,
        "WinUsb_GetCurrentAlternateSetting" => fn WinUsb_GetCurrentAlternateSetting,
        "WinUsb_SetCurrentAlternateSetting" => fn WinUsb_SetCurrentAlternateSetting,
        "WinUsb_SetPipePolicy" => fn WinUsb_SetPipePolicy,
        "WinUsb_GetPipePolicy" => fn WinUsb_GetPipePolicy,
        "WinUsb_SetPowerPolicy" => fn WinUsb_SetPowerPolicy,
        "WinUsb_GetPowerPolicy" => fn WinUsb_SetPowerPolicy,
        "WinUsb_ControlTransfer" => fn WinUsb_ControlTransfer,
        "WinUsb_ReadPipe" => fn WinUsb_ReadPipe,
        "WinUsb_WritePipe" => fn WinUsb_WritePipe,
        "WinUsb_ResetPipe" => fn WinUsb_ResetPipe,
        "WinUsb_AbortPipe" => fn WinUsb_AbortPipe,
        "WinUsb_FlushPipe" => fn WinUsb_AbortPipe,
        "WinUsb_GetOverlappedResult" => fn WinUsb_GetOverlappedResult,
        "WinUsb_ReadIsochPipe" => fn not_supported,
        "WinUsb_WriteIsochPipe" => fn not_supported,
        "WinUsb_RegisterIsochBuffer" => fn not_supported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appareil + configuration : une interface, deux points bulk de 512 octets.
    pub(crate) const DESC: &[u8] = &[
        18, 1, 0x00, 0x02, 0xFF, 0, 0, 64, 0xD0, 0x16, 0x34, 0x12, 0x00, 0x01, 1, 2, 3,
        1, // appareil
        9, 2, 32, 0, 1, 1, 0, 0x80, 50, // configuration, 32 octets
        9, 4, 0, 0, 2, 0xFF, 0, 0, 0, // interface 0, 2 points
        7, 5, 0x81, 2, 0x00, 0x02, 0, // bulk IN 512
        7, 5, 0x02, 2, 0x00, 0x02, 0, // bulk OUT 512
    ];

    #[test]
    fn lit_interfaces_et_points() {
        let i = interfaces(DESC);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].number, 0);
        assert_eq!(i[0].endpoints.len(), 2);
        assert_eq!(i[0].endpoints[0][2], 0x81);
        assert_eq!(config(DESC).unwrap().len(), 32);
    }

    #[test]
    fn descripteur_tronque_sans_panique() {
        assert!(interfaces(&DESC[..30]).is_empty() || interfaces(&DESC[..30]).len() == 1);
        assert!(config(&DESC[..19]).is_none());
    }
}
