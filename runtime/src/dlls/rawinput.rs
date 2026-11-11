//! Entrée brute (#271) : `RegisterRawInputDevices`, `GetRawInputData`,
//! `GetRawInputBuffer`, `GetRawInputDeviceList`, `GetRawInputDeviceInfoW`.
//!
//! Les jeux de tir lisent la souris en déplacements relatifs (WM_INPUT),
//! sans accélération et sans être bloqués au bord de l'écran. Winlux lit
//! pour cela les souris et claviers evdev de l'hôte (/dev/input/event*, que
//! le groupe input ouvre à chaque compte) dans un fil, et poste WM_INPUT à
//! la fenêtre inscrite tant que l'application a le focus (ou toujours avec
//! RIDEV_INPUTSINK).
#![allow(non_snake_case)]

use std::collections::VecDeque;
use std::os::unix::io::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

pub const WM_INPUT: u32 = 0x00FF;
const RIM_TYPEMOUSE: u32 = 0;
const RIM_TYPEKEYBOARD: u32 = 1;
const RIM_TYPEHID: u32 = 2;
const RID_INPUT: u32 = 0x1000_0003;
const RID_HEADER: u32 = 0x1000_0005;
const RIDI_PREPARSEDDATA: u32 = 0x2000_0005;
const RIDI_DEVICENAME: u32 = 0x2000_0007;
const RIDI_DEVICEINFO: u32 = 0x2000_000B;
const RIDEV_REMOVE: u32 = 0x0000_0001;
const RIDEV_INPUTSINK: u32 = 0x0000_0100;
const HEADER: usize = 24;

// Un programme x86 (wow32) passe les tailles de ses propres structures :
// RAWINPUTDEVICE 12 octets, RAWINPUTDEVICELIST 8, RAWINPUTHEADER 16, et
// des paquets alignés sur 4 octets (RAWINPUT_ALIGN). Tout refuser en
// ERROR_INVALID_PARAMETER faisait boucler l'initialisation des manettes de
// SDL 2 dans pygame 32 bits (#1616).

/// Taille d'un pointeur ou d'un handle chez l'appelant (4 en x86, 8 en x64).
fn ptr() -> usize {
    crate::wow32::pointer_size()
}
/// RAWINPUTDEVICE : usUsagePage, usUsage, dwFlags, hwndTarget.
fn device_size() -> usize {
    8 + ptr()
}
/// RAWINPUTDEVICELIST : hDevice, dwType (complété à l'alignement).
fn list_size() -> usize {
    2 * ptr()
}
/// RAWINPUTHEADER : dwType, dwSize, hDevice, wParam.
fn header_size() -> usize {
    8 + 2 * ptr()
}
unsafe fn read_handle(p: *const u8) -> usize {
    if ptr() == 4 {
        (p as *const u32).read_unaligned() as usize
    } else {
        (p as *const usize).read_unaligned()
    }
}
unsafe fn write_handle(p: *mut u8, v: usize) {
    if ptr() == 4 {
        (p as *mut u32).write_unaligned(v as u32);
    } else {
        (p as *mut usize).write_unaligned(v);
    }
}

/// Paquet x64 (en-tête de 24 octets) remis à la forme de l'appelant : en
/// x86, en-tête de 16 octets aux handles tronqués, dwSize corrigé.
fn for_caller(packet: &[u8], pointer: usize) -> Vec<u8> {
    if pointer == 8 || packet.len() < HEADER {
        return packet.to_vec();
    }
    let size = (packet.len() - 8) as u32;
    let mut v = Vec::with_capacity(packet.len() - 8);
    v.extend(&packet[0..4]);
    v.extend(size.to_le_bytes());
    v.extend(&packet[8..12]); // hDevice (handles < 4 Gio)
    v.extend(&packet[16..20]); // wParam
    v.extend(&packet[HEADER..]);
    v
}
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_HANDLE: u32 = 6;

// ============================================================
//  Focus de l'application (posé par gui_x11 sur FocusIn/FocusOut)
// ============================================================

static APP_FOCUSED: AtomicBool = AtomicBool::new(true);
pub fn set_app_focus(focused: bool) {
    APP_FOCUSED.store(focused, Ordering::Relaxed);
}

// ============================================================
//  Inscriptions
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq)]
struct Registration {
    page: u16,
    usage: u16,
    flags: u32,
    hwnd: usize,
}
static REGISTERED: Mutex<Vec<Registration>> = Mutex::new(Vec::new());
static READER: std::sync::Once = std::sync::Once::new();

fn wants(page: u16, usage: u16) -> Option<Registration> {
    crate::dlls::lock(&REGISTERED)
        .iter()
        .find(|r| r.page == page && r.usage == usage)
        .copied()
}

/// RAWINPUTDEVICE : usUsagePage, usUsage, dwFlags, hwndTarget (16 octets).
pub unsafe extern "win64" fn RegisterRawInputDevices(
    devs: *const u8,
    count: u32,
    size: u32,
) -> i32 {
    if devs.is_null() || count == 0 || size as usize != device_size() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut regs = crate::dlls::lock(&REGISTERED);
    for i in 0..count as usize {
        let p = devs.add(i * device_size());
        let r = Registration {
            page: (p as *const u16).read_unaligned(),
            usage: (p.add(2) as *const u16).read_unaligned(),
            flags: (p.add(4) as *const u32).read_unaligned(),
            hwnd: read_handle(p.add(8)),
        };
        regs.retain(|x| !(x.page == r.page && x.usage == r.usage));
        if r.flags & RIDEV_REMOVE == 0 {
            regs.push(r);
        }
    }
    drop(regs);
    READER.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("winlux-rawinput".into())
            .spawn(reader);
    });
    1
}

pub unsafe extern "win64" fn GetRegisteredRawInputDevices(
    devs: *mut u8,
    count: *mut u32,
    size: u32,
) -> u32 {
    if count.is_null() || size as usize != device_size() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let regs = crate::dlls::lock(&REGISTERED).clone();
    if devs.is_null() || (*count as usize) < regs.len() {
        *count = regs.len() as u32;
        if devs.is_null() {
            return 0;
        }
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    for (i, r) in regs.iter().enumerate() {
        let p = devs.add(i * device_size());
        (p as *mut u16).write_unaligned(r.page);
        (p.add(2) as *mut u16).write_unaligned(r.usage);
        (p.add(4) as *mut u32).write_unaligned(r.flags);
        write_handle(p.add(8), r.hwnd);
    }
    regs.len() as u32
}

// ============================================================
//  Paquets RAWINPUT
// ============================================================

/// File des paquets : HRAWINPUT = numéro du paquet (jamais zéro).
struct Queue {
    packets: VecDeque<(usize, Vec<u8>, bool)>, // (handle, octets, déjà lu par GetRawInputBuffer)
}
static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    packets: VecDeque::new(),
});
static NEXT_PACKET: AtomicUsize = AtomicUsize::new(1);

fn push_packet(data: Vec<u8>) -> usize {
    let h = NEXT_PACKET.fetch_add(1, Ordering::Relaxed);
    let mut q = crate::dlls::lock(&QUEUE);
    q.packets.push_back((h, data, false));
    while q.packets.len() > 1024 {
        q.packets.pop_front();
    }
    h
}

fn header(kind: u32, size: usize, device: usize, wparam: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(size);
    v.extend(kind.to_le_bytes());
    v.extend((size as u32).to_le_bytes());
    v.extend(device.to_le_bytes());
    v.extend(wparam.to_le_bytes());
    v
}

/// RAWMOUSE (24 octets après l'en-tête).
pub fn mouse_packet(
    device: usize,
    sink: bool,
    buttons: u16,
    wheel: i16,
    dx: i32,
    dy: i32,
) -> Vec<u8> {
    let mut v = header(RIM_TYPEMOUSE, HEADER + 24, device, sink as usize);
    v.extend(0u16.to_le_bytes()); // MOUSE_MOVE_RELATIVE
    v.extend(0u16.to_le_bytes());
    v.extend(buttons.to_le_bytes());
    v.extend(wheel.to_le_bytes());
    v.extend(0u32.to_le_bytes()); // ulRawButtons
    v.extend(dx.to_le_bytes());
    v.extend(dy.to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v
}

/// RAWKEYBOARD (16 octets après l'en-tête) ; paquet de 40 octets.
pub fn keyboard_packet(device: usize, sink: bool, linux_code: u16, down: bool) -> Option<Vec<u8>> {
    let (make, e0) = scancode(linux_code)?;
    let vk = vk_of(make, e0);
    let mut flags = if down { 0u16 } else { 1 }; // RI_KEY_BREAK
    if e0 {
        flags |= 2; // RI_KEY_E0
    }
    let alt = matches!(linux_code, 56 | 100);
    let message: u32 = match (down, alt) {
        (true, false) => 0x100,
        (false, false) => 0x101,
        (true, true) => 0x104,
        (false, true) => 0x105,
    };
    let mut v = header(RIM_TYPEKEYBOARD, HEADER + 16, device, sink as usize);
    v.extend(make.to_le_bytes());
    v.extend(flags.to_le_bytes());
    v.extend(0u16.to_le_bytes());
    v.extend(vk.to_le_bytes());
    v.extend(message.to_le_bytes());
    v.extend(0u32.to_le_bytes());
    Some(v)
}

/// Code evdev → scancode PS/2 (jeu 1) et préfixe E0.
pub fn scancode(code: u16) -> Option<(u16, bool)> {
    Some(match code {
        1..=83 | 87 | 88 => (code, false),
        86 => (0x56, false),
        96 => (0x1C, true),  // Entrée du pavé
        97 => (0x1D, true),  // Ctrl droit
        98 => (0x35, true),  // / du pavé
        99 => (0x37, true),  // Impr. écran
        100 => (0x38, true), // Alt Gr
        102 => (0x47, true),
        103 => (0x48, true),
        104 => (0x49, true),
        105 => (0x4B, true),
        106 => (0x4D, true),
        107 => (0x4F, true),
        108 => (0x50, true),
        109 => (0x51, true),
        110 => (0x52, true),
        111 => (0x53, true),
        119 => (0x45, false), // Pause (simplifié)
        125 => (0x5B, true),
        126 => (0x5C, true),
        127 => (0x5D, true),
        _ => return None,
    })
}

/// Touche virtuelle d'un scancode, disposition QWERTY positionnelle (les jeux
/// lisent surtout MakeCode ; VKey sert aux raccourcis).
pub fn vk_of(make: u16, e0: bool) -> u16 {
    const ROW: &[(u16, &[u8])] = &[
        (0x10, b"QWERTYUIOP"),
        (0x1E, b"ASDFGHJKL"),
        (0x2C, b"ZXCVBNM"),
    ];
    for (start, keys) in ROW {
        if (*start..*start + keys.len() as u16).contains(&make) {
            return keys[(make - start) as usize] as u16;
        }
    }
    if e0 {
        return match make {
            0x1C => 0x0D,
            0x1D => 0xA3,
            0x35 => 0x6F,
            0x37 => 0x2C,
            0x38 => 0xA5,
            0x47 => 0x24,
            0x48 => 0x26,
            0x49 => 0x21,
            0x4B => 0x25,
            0x4D => 0x27,
            0x4F => 0x23,
            0x50 => 0x28,
            0x51 => 0x22,
            0x52 => 0x2D,
            0x53 => 0x2E,
            0x5B => 0x5B,
            0x5C => 0x5C,
            0x5D => 0x5D,
            _ => 0,
        };
    }
    match make {
        0x01 => 0x1B,
        0x02..=0x0A => 0x31 + (make - 0x02), // 1..9
        0x0B => 0x30,
        0x0C => 0xBD,
        0x0D => 0xBB,
        0x0E => 0x08,
        0x0F => 0x09,
        0x1A => 0xDB,
        0x1B => 0xDD,
        0x1C => 0x0D,
        0x1D => 0xA2,
        0x27 => 0xBA,
        0x28 => 0xDE,
        0x29 => 0xC0,
        0x2A => 0xA0,
        0x2B => 0xDC,
        0x33 => 0xBC,
        0x34 => 0xBE,
        0x35 => 0xBF,
        0x36 => 0xA1,
        0x37 => 0x6A,
        0x38 => 0xA4,
        0x39 => 0x20,
        0x3A => 0x14,
        0x3B..=0x44 => 0x70 + (make - 0x3B), // F1..F10
        0x45 => 0x90,
        0x46 => 0x91,
        0x47 => 0x67,
        0x48 => 0x68,
        0x49 => 0x69,
        0x4A => 0x6D,
        0x4B => 0x64,
        0x4C => 0x65,
        0x4D => 0x66,
        0x4E => 0x6B,
        0x4F => 0x61,
        0x50 => 0x62,
        0x51 => 0x63,
        0x52 => 0x60,
        0x53 => 0x6E,
        0x56 => 0xE2,
        0x57 => 0x7A,
        0x58 => 0x7B,
        _ => 0,
    }
}

/// Paquet lu par handle (GetRawInputData).
pub unsafe extern "win64" fn GetRawInputData(
    handle: usize,
    command: u32,
    data: *mut u8,
    size: *mut u32,
    header_size: u32,
) -> u32 {
    if size.is_null()
        || header_size as usize != self::header_size()
        || !(command == RID_INPUT || command == RID_HEADER)
    {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let q = crate::dlls::lock(&QUEUE);
    let Some((_, packet, _)) = q.packets.iter().find(|(h, _, _)| *h == handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return u32::MAX;
    };
    let packet = for_caller(packet, ptr());
    let want = if command == RID_HEADER {
        self::header_size()
    } else {
        packet.len()
    };
    if data.is_null() {
        *size = want as u32;
        return 0;
    }
    if (*size as usize) < want {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    std::ptr::copy_nonoverlapping(packet.as_ptr(), data, want);
    want as u32
}

/// Paquets en attente, alignés sur 8 octets (4 en x86) (GetRawInputBuffer).
pub unsafe extern "win64" fn GetRawInputBuffer(
    data: *mut u8,
    size: *mut u32,
    header_size: u32,
) -> u32 {
    if size.is_null() || header_size as usize != self::header_size() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    // RAWINPUT_ALIGN : 8 octets en x64, 4 en x86.
    let (pointer, align) = (ptr(), ptr());
    let mut q = crate::dlls::lock(&QUEUE);
    let pending = q.packets.iter().filter(|(_, _, read)| !read);
    if data.is_null() {
        *size = pending
            .map(|(_, p, _)| for_caller(p, pointer).len().next_multiple_of(align))
            .max()
            .unwrap_or(0) as u32;
        return 0;
    }
    let room = *size as usize;
    let mut at = 0usize;
    let mut count = 0u32;
    for (_, p, read) in q.packets.iter_mut().filter(|(_, _, read)| !*read) {
        let p = for_caller(p, pointer);
        let step = p.len().next_multiple_of(align);
        if at + step > room {
            break;
        }
        std::ptr::copy_nonoverlapping(p.as_ptr(), data.add(at), p.len());
        *read = true;
        at += step;
        count += 1;
    }
    if count == 0 && q.packets.iter().any(|(_, _, read)| !read) {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    count
}

pub extern "win64" fn DefRawInputProc(_raw: usize, _count: i32, _header: u32) -> isize {
    0
}

// ============================================================
//  Périphériques
// ============================================================

#[derive(Clone, Debug, PartialEq)]
pub struct RawDevice {
    pub handle: usize,
    pub kind: u32,
    pub path: PathBuf,
    pub vendor: u16,
    pub product: u16,
}

const MOUSE_BASE: usize = 0x1_0000;
const KEYBOARD_BASE: usize = 0x2_0000;
const HID_BASE: usize = 0x3_0000;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;

fn ioc_read(nr: u32, size: u32) -> u64 {
    (2u64 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr as u64
}
fn bits(fd: i32, ev: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    let req = ioc_read(0x20 + ev, len as u32);
    if unsafe { libc::ioctl(fd, req as _, b.as_mut_ptr()) } < 0 {
        b.clear();
    }
    b
}
fn has(b: &[u8], n: usize) -> bool {
    b.get(n / 8).is_some_and(|x| x & (1 << (n % 8)) != 0)
}

/// Souris (REL_X + REL_Y + BTN_LEFT) ou clavier (touches A, Z, Espace).
fn classify(fd: i32) -> Option<u32> {
    let rel = bits(fd, EV_REL as u32, 2);
    let keys = bits(fd, EV_KEY as u32, 96);
    if has(&rel, 0) && has(&rel, 1) && has(&keys, 0x110) {
        Some(RIM_TYPEMOUSE)
    } else if has(&keys, 30) && has(&keys, 44) && has(&keys, 57) {
        Some(RIM_TYPEKEYBOARD)
    } else {
        None
    }
}

fn input_ids(fd: i32) -> (u16, u16) {
    let mut id = [0u16; 4];
    let req = ioc_read(0x02, 8);
    if unsafe { libc::ioctl(fd, req as _, id.as_mut_ptr()) } < 0 {
        return (0, 0);
    }
    (id[1], id[2])
}

fn open_event(path: &std::path::Path) -> Option<OwnedFd> {
    use std::os::unix::fs::OpenOptionsExt;
    // Faux appareil de test (FIFO) : ouvert aussi en écriture pour ne pas
    // voir POLLHUP quand l'écrivain referme.
    let fake = std::env::var_os("WINLUX_RAWINPUT_DEVICES").is_some();
    std::fs::OpenOptions::new()
        .read(true)
        .write(fake)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()
        .map(OwnedFd::from)
}

/// Souris et claviers evdev lisibles, puis appareils HID (manettes).
pub fn devices() -> Vec<RawDevice> {
    let mut out = Vec::new();
    // Tests : « mouse=/chemin;keyboard=/chemin » remplace evdev et HID ; les
    // fichiers reçoivent des struct input_event de 24 octets.
    if let Some(spec) = std::env::var_os("WINLUX_RAWINPUT_DEVICES") {
        for (i, item) in spec.to_string_lossy().split(';').enumerate() {
            let Some((kind, path)) = item.split_once('=') else {
                continue;
            };
            let (kind, base) = match kind {
                "mouse" => (RIM_TYPEMOUSE, MOUSE_BASE),
                "keyboard" => (RIM_TYPEKEYBOARD, KEYBOARD_BASE),
                _ => continue,
            };
            out.push(RawDevice {
                handle: base + 900 + i,
                kind,
                path: PathBuf::from(path),
                vendor: 0x1234,
                product: 0x5678,
            });
        }
        return out;
    }
    for path in crate::dlls::evdev::event_paths() {
        let Some(fd) = open_event(&path) else {
            continue;
        };
        let n: usize = path
            .file_name()
            .and_then(|n| n.to_str()?.strip_prefix("event")?.parse().ok())
            .unwrap_or(0);
        if let Some(kind) = classify(fd.as_raw_fd()) {
            let (vendor, product) = input_ids(fd.as_raw_fd());
            let base = if kind == RIM_TYPEMOUSE {
                MOUSE_BASE
            } else {
                KEYBOARD_BASE
            };
            out.push(RawDevice {
                handle: base + n,
                kind,
                path,
                vendor,
                product,
            });
        }
    }
    for d in crate::dlls::hid::devices() {
        out.push(RawDevice {
            handle: HID_BASE + d.index as usize,
            kind: RIM_TYPEHID,
            path: PathBuf::from(d.interface_path()),
            vendor: d.vendor,
            product: d.product,
        });
    }
    out
}

/// RAWINPUTDEVICELIST : hDevice, dwType (16 octets).
pub unsafe extern "win64" fn GetRawInputDeviceList(
    list: *mut u8,
    count: *mut u32,
    size: u32,
) -> u32 {
    if count.is_null() || size as usize != list_size() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let devs = devices();
    if list.is_null() {
        *count = devs.len() as u32;
        return 0;
    }
    if (*count as usize) < devs.len() {
        *count = devs.len() as u32;
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    for (i, d) in devs.iter().enumerate() {
        let p = list.add(i * list_size());
        write_handle(p, d.handle);
        // dwType, puis le bourrage d'alignement à zéro en x64.
        write_handle(p.add(ptr()), d.kind as usize);
    }
    devs.len() as u32
}

/// Nom d'interface Windows d'un appareil evdev.
fn device_name(d: &RawDevice) -> String {
    match d.kind {
        RIM_TYPEHID => d.path.to_string_lossy().into_owned(),
        kind => {
            let class = if kind == RIM_TYPEMOUSE {
                "{378de44c-56ef-11d1-bc8c-00a0c91405dd}"
            } else {
                "{884b96c3-56ef-11d1-bc8c-00a0c91405dd}"
            };
            format!(
                "\\\\?\\HID#VID_{:04X}&PID_{:04X}#{}#{}",
                d.vendor,
                d.product,
                d.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("event"),
                class
            )
        }
    }
}

/// RID_DEVICE_INFO (32 octets) : souris, clavier ou HID.
fn device_info(d: &RawDevice) -> Vec<u8> {
    let mut v = vec![0u8; 32];
    v[0..4].copy_from_slice(&32u32.to_le_bytes());
    v[4..8].copy_from_slice(&d.kind.to_le_bytes());
    match d.kind {
        RIM_TYPEMOUSE => {
            v[8..12].copy_from_slice(&256u32.to_le_bytes()); // dwId
            v[12..16].copy_from_slice(&5u32.to_le_bytes()); // boutons
            v[16..20].copy_from_slice(&1000u32.to_le_bytes()); // échantillonnage
            v[20] = 1; // fHasHorizontalWheel
        }
        RIM_TYPEKEYBOARD => {
            for (i, x) in [4u32, 0, 1, 12, 3, 101].iter().enumerate() {
                v[8 + i * 4..12 + i * 4].copy_from_slice(&x.to_le_bytes());
            }
        }
        _ => {
            v[8..12].copy_from_slice(&(d.vendor as u32).to_le_bytes());
            v[12..16].copy_from_slice(&(d.product as u32).to_le_bytes());
            let caps = crate::dlls::hid::devices()
                .into_iter()
                .find(|h| HID_BASE + h.index as usize == d.handle)
                .map(|h| crate::dlls::hid::parse_descriptor(&h.descriptor))
                .unwrap_or_default();
            v[20..22].copy_from_slice(&caps.usage_page.to_le_bytes());
            v[22..24].copy_from_slice(&caps.usage.to_le_bytes());
        }
    }
    v
}

pub unsafe extern "win64" fn GetRawInputDeviceInfoW(
    handle: usize,
    command: u32,
    data: *mut u8,
    size: *mut u32,
) -> u32 {
    if size.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let Some(d) = devices().into_iter().find(|d| d.handle == handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return u32::MAX;
    };
    // RIDI_DEVICENAME compte en caractères, le reste en octets.
    let (bytes, unit): (Vec<u8>, usize) = match command {
        RIDI_DEVICENAME => (
            device_name(&d)
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|c| c.to_le_bytes())
                .collect(),
            2,
        ),
        RIDI_DEVICEINFO => (device_info(&d), 1),
        RIDI_PREPARSEDDATA => return 0, // *size inchangé : pas de données
        _ => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return u32::MAX;
        }
    };
    let need = (bytes.len() / unit) as u32;
    if data.is_null() {
        *size = need;
        return 0;
    }
    if *size < need {
        *size = need;
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
    need
}
pub unsafe extern "win64" fn GetRawInputDeviceInfoA(
    handle: usize,
    command: u32,
    data: *mut u8,
    size: *mut u32,
) -> u32 {
    if command != RIDI_DEVICENAME {
        return GetRawInputDeviceInfoW(handle, command, data, size);
    }
    let Some(d) = devices().into_iter().find(|d| d.handle == handle) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return u32::MAX;
    };
    let mut name = device_name(&d).into_bytes();
    name.push(0);
    if data.is_null() || (*size as usize) < name.len() {
        *size = name.len() as u32;
        if data.is_null() {
            return 0;
        }
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return u32::MAX;
    }
    std::ptr::copy_nonoverlapping(name.as_ptr(), data, name.len());
    name.len() as u32
}

// ============================================================
//  Lecture evdev et envoi de WM_INPUT
// ============================================================

/// Destination d'un paquet : la fenêtre inscrite, sinon la fenêtre du
/// premier plan ; rien quand l'application n'a pas le focus (sauf
/// RIDEV_INPUTSINK).
fn target(page: u16, usage: u16) -> Option<(usize, bool)> {
    let r = wants(page, usage)?;
    let focused = APP_FOCUSED.load(Ordering::Relaxed);
    if !focused && r.flags & RIDEV_INPUTSINK == 0 {
        return None;
    }
    let hwnd = if r.hwnd != 0 {
        r.hwnd
    } else {
        crate::dlls::gui::first_window()?
    };
    Some((hwnd, !focused))
}

fn deliver(hwnd: usize, sink: bool, packet: Vec<u8>) {
    let h = push_packet(packet);
    unsafe {
        crate::dlls::user32::post_message_w(hwnd, WM_INPUT, sink as usize, h as isize);
    }
}

#[derive(Default)]
struct MouseAcc {
    dx: i32,
    dy: i32,
    buttons: u16,
    wheel: i32,
    hwheel: i32,
    hires: bool,
}

fn mouse_button(code: u16, down: bool) -> u16 {
    let bit = match code {
        0x110 => 0,
        0x111 => 2,
        0x112 => 4,
        0x113 => 6,
        0x114 => 8,
        _ => return 0,
    };
    1 << (bit + if down { 0 } else { 1 })
}

fn reader() {
    let mut open: Vec<(RawDevice, OwnedFd, MouseAcc)> = Vec::new();
    let mut rescan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let mut ev = [0u8; 24 * 64];
    loop {
        if rescan.elapsed() > std::time::Duration::from_secs(2) {
            rescan = std::time::Instant::now();
            for d in devices().into_iter().filter(|d| d.kind != RIM_TYPEHID) {
                if open.iter().any(|(o, _, _)| o.handle == d.handle) {
                    continue;
                }
                if let Some(fd) = open_event(&d.path) {
                    open.push((d, fd, MouseAcc::default()));
                }
            }
        }
        if crate::dlls::lock(&REGISTERED).is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }
        let mut fds: Vec<libc::pollfd> = open
            .iter()
            .map(|(_, fd, _)| libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        if fds.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 500) } <= 0 {
            continue;
        }
        let mut gone = Vec::new();
        for (i, p) in fds.iter().enumerate() {
            if p.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                gone.push(i);
                continue;
            }
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            let n = unsafe { libc::read(p.fd, ev.as_mut_ptr() as *mut _, ev.len()) };
            if n <= 0 {
                continue;
            }
            let (dev, _, acc) = &mut open[i];
            for e in ev[..n as usize].as_chunks::<24>().0 {
                let ty = u16::from_le_bytes([e[16], e[17]]);
                let code = u16::from_le_bytes([e[18], e[19]]);
                let value = i32::from_le_bytes([e[20], e[21], e[22], e[23]]);
                handle_event(dev, acc, ty, code, value);
            }
        }
        for i in gone.into_iter().rev() {
            open.remove(i);
        }
    }
}

fn handle_event(dev: &RawDevice, acc: &mut MouseAcc, ty: u16, code: u16, value: i32) {
    match (dev.kind, ty) {
        (RIM_TYPEMOUSE, EV_REL) => match code {
            0 => acc.dx += value,
            1 => acc.dy += value,
            8 if !acc.hires => acc.wheel += value * 120,
            6 if !acc.hires => acc.hwheel += value * 120,
            11 => {
                acc.hires = true;
                acc.wheel += value;
            }
            12 => {
                acc.hires = true;
                acc.hwheel += value;
            }
            _ => {}
        },
        (RIM_TYPEMOUSE, EV_KEY) if value != 2 => acc.buttons |= mouse_button(code, value != 0),
        (RIM_TYPEMOUSE, EV_SYN) if code == 0 => {
            let Some((hwnd, sink)) = target(1, 2) else {
                *acc = MouseAcc {
                    hires: acc.hires,
                    ..Default::default()
                };
                return;
            };
            if acc.dx != 0 || acc.dy != 0 || acc.buttons != 0 {
                deliver(
                    hwnd,
                    sink,
                    mouse_packet(dev.handle, sink, acc.buttons, 0, acc.dx, acc.dy),
                );
            }
            if acc.wheel != 0 {
                deliver(
                    hwnd,
                    sink,
                    mouse_packet(dev.handle, sink, 0x400, acc.wheel as i16, 0, 0),
                );
            }
            if acc.hwheel != 0 {
                deliver(
                    hwnd,
                    sink,
                    mouse_packet(dev.handle, sink, 0x800, acc.hwheel as i16, 0, 0),
                );
            }
            *acc = MouseAcc {
                hires: acc.hires,
                ..Default::default()
            };
        }
        (RIM_TYPEKEYBOARD, EV_KEY) => {
            // 0 relâchée, 1 enfoncée, 2 répétition (Windows la répète aussi).
            if let Some((hwnd, sink)) = target(1, 6) {
                if let Some(p) = keyboard_packet(dev.handle, sink, code, value != 0) {
                    deliver(hwnd, sink, p);
                }
            }
        }
        _ => {}
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterRawInputDevices" => fn RegisterRawInputDevices,
        "GetRegisteredRawInputDevices" => fn GetRegisteredRawInputDevices,
        "GetRawInputData" => fn GetRawInputData,
        "GetRawInputBuffer" => fn GetRawInputBuffer,
        "GetRawInputDeviceList" => fn GetRawInputDeviceList,
        "GetRawInputDeviceInfoW" => fn GetRawInputDeviceInfoW,
        "GetRawInputDeviceInfoA" => fn GetRawInputDeviceInfoA,
        "DefRawInputProc" => fn DefRawInputProc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paquet_souris() {
        let p = mouse_packet(MOUSE_BASE + 3, false, 0x1, 0, -5, 7);
        assert_eq!(p.len(), 48);
        assert_eq!(u32::from_le_bytes(p[4..8].try_into().unwrap()), 48);
        assert_eq!(u16::from_le_bytes(p[28..30].try_into().unwrap()), 0x1);
        assert_eq!(i32::from_le_bytes(p[36..40].try_into().unwrap()), -5);
        assert_eq!(i32::from_le_bytes(p[40..44].try_into().unwrap()), 7);
    }

    #[test]
    fn paquet_remis_en_forme_x86() {
        // #1616 : en-tête RAWINPUTHEADER de 16 octets pour un appelant x86.
        let p = mouse_packet(MOUSE_BASE + 3, true, 0x1, 0, -5, 7);
        assert_eq!(for_caller(&p, 8), p);
        let x86 = for_caller(&p, 4);
        assert_eq!(x86.len(), 40);
        assert_eq!(
            u32::from_le_bytes(x86[0..4].try_into().unwrap()),
            RIM_TYPEMOUSE
        );
        assert_eq!(u32::from_le_bytes(x86[4..8].try_into().unwrap()), 40);
        assert_eq!(
            u32::from_le_bytes(x86[8..12].try_into().unwrap()) as usize,
            MOUSE_BASE + 3
        );
        assert_eq!(u32::from_le_bytes(x86[12..16].try_into().unwrap()), 1);
        assert_eq!(x86[16..], p[HEADER..]);
        // Tailles x64 attendues hors wow32.
        assert_eq!((device_size(), list_size(), header_size()), (16, 16, 24));
    }

    #[test]
    fn paquet_clavier_et_touches() {
        let p = keyboard_packet(KEYBOARD_BASE, false, 17, true).unwrap(); // W
        assert_eq!(p.len(), 40);
        assert_eq!(u16::from_le_bytes(p[24..26].try_into().unwrap()), 0x11);
        assert_eq!(
            u16::from_le_bytes(p[30..32].try_into().unwrap()),
            b'W' as u16
        );
        assert_eq!(u32::from_le_bytes(p[32..36].try_into().unwrap()), 0x100);
        let up = keyboard_packet(KEYBOARD_BASE, false, 103, false).unwrap(); // Haut
        assert_eq!(u16::from_le_bytes(up[26..28].try_into().unwrap()), 3); // BREAK | E0
        assert_eq!(u16::from_le_bytes(up[30..32].try_into().unwrap()), 0x26);
        assert_eq!(vk_of(0x3B, false), 0x70);
        assert_eq!(vk_of(0x02, false), b'1' as u16);
        assert_eq!(mouse_button(0x111, false), 0x8);
    }
}
