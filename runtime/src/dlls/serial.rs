//! Ports série, suite (#271) : ce dont ont besoin les logiciels d'Arduino,
//! d'imprimante 3D et de radio pour TROUVER un port et attendre ses données.
//! DCB, délais et lignes de modem sont déjà dans `win32_basics.rs` (termios).
//!
//! - `QueryDosDeviceW/A` : lettres de lecteur et ports COM présents ;
//! - `HKLM\HARDWARE\DEVICEMAP\SERIALCOMM`, rafraîchie à chaque ouverture ;
//! - `SetCommMask`, `GetCommMask`, `WaitCommEvent` (OVERLAPPED compris),
//!   `GetCommProperties`, `BuildCommDCBA/W`, `TransmitCommChar`,
//!   `GetCommConfig`, `GetDefaultCommConfigW/A`.
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_IO_PENDING: u32 = 997;

const EV_RXCHAR: u32 = 0x0001;
const EV_TXEMPTY: u32 = 0x0004;
const EV_CTS: u32 = 0x0008;
const EV_DSR: u32 = 0x0010;
const EV_RLSD: u32 = 0x0020;
const EV_RING: u32 = 0x0100;

/// Un port COM présent : numéro, fichier Linux, nom NT (`\Device\…`).
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub number: u32,
    pub linux: String,
    pub nt: String,
}

fn dev_root() -> std::path::PathBuf {
    std::env::var_os("WINLUX_DEV_ROOT")
        .map(Into::into)
        .unwrap_or_else(|| "/dev".into())
}
pub(crate) fn sys_tty() -> std::path::PathBuf {
    std::env::var_os("WINLUX_SYSFS")
        .map(|s| std::path::PathBuf::from(s).join("class/tty"))
        .unwrap_or_else(|| "/sys/class/tty".into())
}

/// Ports présents, numérotés comme `paths::com_port` : COM1 à COM4 = ttyS0 à
/// ttyS3 (seulement s'il y a un vrai UART derrière), COM5 et suivants = les
/// adaptateurs USB, et tout `WINLUX_COMn` défini.
pub fn ports() -> Vec<Port> {
    let mut out = Vec::new();
    for n in 1..=4u32 {
        let name = format!("ttyS{}", n - 1);
        let kind = std::fs::read_to_string(sys_tty().join(&name).join("type")).unwrap_or_default();
        if kind.trim().parse::<u32>().unwrap_or(0) != 0 {
            out.push(Port {
                number: n,
                linux: format!("/dev/{name}"),
                nt: format!("\\Device\\Serial{}", n - 1),
            });
        }
    }
    let mut usb: Vec<String> = std::fs::read_dir(dev_root())
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|f| f.starts_with("ttyUSB") || f.starts_with("ttyACM"))
                .collect()
        })
        .unwrap_or_default();
    usb.sort_by_key(|f| (f.starts_with("ttyACM"), f.len(), f.clone()));
    for (i, f) in usb.iter().enumerate() {
        let nt = if let Some(k) = f.strip_prefix("ttyACM") {
            format!("\\Device\\USBSER{:0>3}", k)
        } else {
            format!("\\Device\\VCP{}", f.trim_start_matches("ttyUSB"))
        };
        out.push(Port {
            number: 5 + i as u32,
            linux: format!("/dev/{f}"),
            nt,
        });
    }
    for (k, v) in std::env::vars() {
        let Some(n) = k
            .strip_prefix("WINLUX_COM")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if !(1..=256).contains(&n) || !std::path::Path::new(&v).exists() {
            continue;
        }
        out.retain(|p| p.number != n);
        out.push(Port {
            number: n,
            linux: v,
            nt: format!("\\Device\\Serial{}", n - 1),
        });
    }
    out.sort_by_key(|p| p.number);
    out
}

// ============================================================
//  QueryDosDevice
// ============================================================

fn dos_devices() -> Vec<(String, String)> {
    let mut v = Vec::new();
    // Les mêmes lettres que GetLogicalDrives : C:, Z: et les lecteurs
    // dynamiques (#2448).
    let mask = crate::dlls::kernel32::get_logical_drives();
    let mut volume = 1;
    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            v.push((
                format!("{letter}:"),
                format!("\\Device\\HarddiskVolume{volume}"),
            ));
            volume += 1;
        }
    }
    for p in ports() {
        v.push((format!("COM{}", p.number), p.nt));
    }
    v.push(("NUL".into(), "\\Device\\Null".into()));
    v.push(("PhysicalDrive0".into(), "\\Device\\Harddisk0\\DR0".into()));
    v
}

/// MULTI_SZ rendu par QueryDosDevice : liste des noms, ou cible d'un nom.
pub fn query_dos_device(name: Option<&str>) -> Option<Vec<String>> {
    let all = dos_devices();
    match name {
        None => Some(all.into_iter().map(|(n, _)| n).collect()),
        Some(n) => all
            .into_iter()
            .find(|(d, _)| d.eq_ignore_ascii_case(n.trim_end_matches('\\')))
            .map(|(_, t)| vec![t]),
    }
}

fn multi_sz(items: &[String]) -> Vec<u16> {
    let mut v: Vec<u16> = Vec::new();
    for s in items {
        v.extend(s.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

pub unsafe extern "win64" fn QueryDosDeviceW(name: *const u16, buf: *mut u16, max: u32) -> u32 {
    let name = (!name.is_null()).then(|| {
        let mut n = 0;
        while *name.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(name, n))
    });
    let Some(items) = query_dos_device(name.as_deref()) else {
        crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    };
    let wide = multi_sz(&items);
    if buf.is_null() || (max as usize) < wide.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    crate::process::set_last_error(0);
    wide.len() as u32
}

pub unsafe extern "win64" fn QueryDosDeviceA(name: *const u8, buf: *mut u8, max: u32) -> u32 {
    let name = (!name.is_null()).then(|| {
        std::ffi::CStr::from_ptr(name as *const libc::c_char)
            .to_string_lossy()
            .into_owned()
    });
    let Some(items) = query_dos_device(name.as_deref()) else {
        crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    };
    let mut bytes = Vec::new();
    for s in &items {
        bytes.extend(s.bytes());
        bytes.push(0);
    }
    bytes.push(0);
    if buf.is_null() || (max as usize) < bytes.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    bytes.len() as u32
}

// ============================================================
//  HKLM\HARDWARE\DEVICEMAP\SERIALCOMM
// ============================================================

/// Appelé à l'ouverture d'une clé : remet SERIALCOMM à jour (les ports USB
/// vont et viennent). Rien pour une autre clé.
pub fn refresh_registry(path: &str) {
    let lower = path.to_ascii_lowercase().replace('/', "\\");
    let lower = lower.trim_end_matches('\\');
    let key = "hklm\\hardware\\devicemap\\serialcomm";
    if !(lower == key
        || key.starts_with(&format!("{lower}\\"))
        || lower == "hkey_local_machine\\hardware\\devicemap\\serialcomm")
    {
        return;
    }
    let ports = ports();
    let _ = crate::registry::write(|d| {
        d.ensure(key);
        for v in d.values(key) {
            d.delete_value(key, &v.name);
        }
        for p in &ports {
            d.set(
                key,
                &p.nt,
                1,
                crate::registry::utf16z(&format!("COM{}", p.number)),
            );
        }
        Ok(())
    });
}

// ============================================================
//  Événements : SetCommMask / WaitCommEvent
// ============================================================

/// Masque et génération par handle (un SetCommMask réveille les attentes).
static MASKS: Mutex<Option<HashMap<usize, (u32, u64)>>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);

fn mask_of(h: usize) -> (u32, u64) {
    crate::dlls::lock(&MASKS)
        .get_or_insert_with(HashMap::new)
        .get(&h)
        .copied()
        .unwrap_or((0, 0))
}

pub unsafe extern "win64" fn SetCommMask(h: usize, mask: u32) -> i32 {
    if crate::dlls::kernel32::fd_of_handle(h).is_none() {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    crate::dlls::lock(&MASKS)
        .get_or_insert_with(HashMap::new)
        .insert(h, (mask, generation));
    1
}

pub unsafe extern "win64" fn GetCommMask(h: usize, mask: *mut u32) -> i32 {
    if mask.is_null() || crate::dlls::kernel32::fd_of_handle(h).is_none() {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    *mask = mask_of(h).0;
    1
}

fn modem_lines(fd: i32) -> i32 {
    let mut bits: libc::c_int = 0;
    unsafe { libc::ioctl(fd, libc::TIOCMGET, &mut bits) };
    bits
}

/// Attend un des événements du masque ; 0 si un SetCommMask l'interrompt.
fn wait_events(h: usize, fd: i32) -> u32 {
    let (mask, generation) = mask_of(h);
    let lines = |b: i32| {
        let mut ev = 0;
        if b & libc::TIOCM_CTS != 0 {
            ev |= EV_CTS;
        }
        if b & libc::TIOCM_DSR != 0 {
            ev |= EV_DSR;
        }
        if b & libc::TIOCM_CAR != 0 {
            ev |= EV_RLSD;
        }
        if b & libc::TIOCM_RNG != 0 {
            ev |= EV_RING;
        }
        ev
    };
    let start = lines(modem_lines(fd));
    if mask & EV_TXEMPTY != 0 {
        unsafe { libc::tcdrain(fd) };
        return EV_TXEMPTY;
    }
    loop {
        if mask_of(h).1 != generation {
            return 0;
        }
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut p, 1, 50) };
        if ready > 0 && p.revents & libc::POLLIN != 0 && mask & EV_RXCHAR != 0 {
            return EV_RXCHAR;
        }
        if ready > 0 && p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return 0;
        }
        let changed = (lines(modem_lines(fd)) ^ start) & mask;
        if changed != 0 {
            return changed;
        }
        if ready > 0 && mask & EV_RXCHAR == 0 {
            // Données présentes mais non demandées : ne pas tourner à vide.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

pub unsafe extern "win64" fn WaitCommEvent(h: usize, out: *mut u32, ov: *mut usize) -> i32 {
    let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if mask_of(h).0 == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if ov.is_null() || !crate::dlls::kernel32::is_overlapped_handle(h) {
        let ev = wait_events(h, fd);
        if !out.is_null() {
            *out = ev;
        }
        return 1;
    }
    // Asynchrone : STATUS_PENDING, puis un fil remplit le masque, termine
    // l'OVERLAPPED et signale son événement.
    *ov = 0x103;
    *ov.add(1) = 0;
    let (out, ov) = (out as usize, ov as usize);
    let spawned = std::thread::Builder::new()
        .name("winlux-commevent".into())
        .spawn(move || {
            let ev = wait_events(h, fd);
            unsafe {
                if out != 0 {
                    *(out as *mut u32) = ev;
                }
                let ov = ov as *mut usize;
                *ov.add(1) = 0;
                *ov = 0;
                let event = *ov.add(3);
                if event != 0 {
                    crate::dlls::kernel32::set_event(event & !1);
                }
            }
        });
    if spawned.is_err() {
        crate::process::set_last_error(8);
        return 0;
    }
    crate::process::set_last_error(ERROR_IO_PENDING);
    0
}

// ============================================================
//  Propriétés, DCB par texte, configuration
// ============================================================

/// COMMPROP (64 octets).
pub unsafe extern "win64" fn GetCommProperties(h: usize, prop: *mut u8) -> i32 {
    if prop.is_null() || crate::dlls::kernel32::fd_of_handle(h).is_none() {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    std::ptr::write_bytes(prop, 0, 64);
    let w16 = |o: usize, v: u16| (prop.add(o) as *mut u16).write_unaligned(v);
    let w32 = |o: usize, v: u32| (prop.add(o) as *mut u32).write_unaligned(v);
    w16(0, 64); // wPacketLength
    w16(2, 2); // wPacketVersion
    w32(4, 1); // SP_SERIALCOMM
    w32(12, 4096); // dwMaxTxQueue
    w32(16, 4096); // dwMaxRxQueue
    w32(20, 0x1000_0000); // BAUD_USER : débit libre
    w32(24, 1); // PST_RS232
    w32(28, 0xFF); // PCF_* : DTR/DSR, RTS/CTS, délais, XON/XOFF…
    w32(32, 0x7F); // SP_* : tout réglable
    w32(36, 0x1007_FFFF); // débits standard + BAUD_USER
    w16(40, 0x000F); // DATABITS_5 à 8
    w16(42, 0x1F07); // STOPBITS_10/15/20 + PARITY_NONE…SPACE
    w32(44, 4096);
    w32(48, 4096);
    1
}

/// Remplit un DCB depuis « 9600,n,8,1 » ou « baud=9600 parity=N data=8 stop=1 ».
pub fn parse_dcb(text: &str, dcb: &mut [u8; 28]) -> bool {
    let mut baud = u32::from_le_bytes(dcb[4..8].try_into().unwrap());
    let (mut parity, mut data, mut stop) = (dcb[19], dcb[18], dcb[20]);
    let parity_of = |s: &str| match s.to_ascii_lowercase().chars().next() {
        Some('n') => Some(0),
        Some('o') => Some(1),
        Some('e') => Some(2),
        Some('m') => Some(3),
        Some('s') => Some(4),
        _ => None,
    };
    let stop_of = |s: &str| match s {
        "1" => Some(0),
        "1.5" => Some(1),
        "2" => Some(2),
        _ => None,
    };
    let text = text.trim();
    let text = text
        .split_once(':')
        .filter(|(p, _)| p.to_ascii_uppercase().starts_with("COM"))
        .map(|(_, r)| r)
        .unwrap_or(text)
        .trim();
    if text.contains('=') {
        for item in text.split_whitespace() {
            let Some((k, v)) = item.split_once('=') else {
                return false;
            };
            match k.to_ascii_lowercase().as_str() {
                "baud" => match v.parse() {
                    Ok(b) => baud = b,
                    Err(_) => return false,
                },
                "parity" => match parity_of(v) {
                    Some(p) => parity = p,
                    None => return false,
                },
                "data" => match v.parse::<u8>() {
                    Ok(d @ 5..=8) => data = d,
                    _ => return false,
                },
                "stop" => match stop_of(v) {
                    Some(s) => stop = s,
                    None => return false,
                },
                "xon" | "odsr" | "octs" | "dtr" | "rts" | "idsr" | "to" => {}
                _ => return false,
            }
        }
    } else {
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        let Some(b) = parts.first().and_then(|b| b.parse::<u32>().ok()) else {
            return false;
        };
        // Anciens raccourcis MODE : 11 = 110, 30 = 300… 96 = 9600, 19 = 19200.
        baud = match b {
            11 => 110,
            15 => 150,
            30 => 300,
            60 => 600,
            12 => 1200,
            24 => 2400,
            48 => 4800,
            96 => 9600,
            19 => 19200,
            b => b,
        };
        if let Some(p) = parts.get(1) {
            match parity_of(p) {
                Some(x) => parity = x,
                None => return false,
            }
        }
        if let Some(d) = parts.get(2) {
            match d.parse::<u8>() {
                Ok(x @ 5..=8) => data = x,
                _ => return false,
            }
        }
        if let Some(s) = parts.get(3) {
            match stop_of(s) {
                Some(x) => stop = x,
                None => return false,
            }
        }
    }
    dcb[0..4].copy_from_slice(&28u32.to_le_bytes());
    dcb[4..8].copy_from_slice(&baud.to_le_bytes());
    let mut bits = u32::from_le_bytes(dcb[8..12].try_into().unwrap());
    bits |= 1; // fBinary
    if parity != 0 {
        bits |= 2; // fParity
    } else {
        bits &= !2;
    }
    dcb[8..12].copy_from_slice(&bits.to_le_bytes());
    dcb[18] = if data == 0 { 8 } else { data };
    dcb[19] = parity;
    dcb[20] = stop;
    true
}

unsafe fn build_dcb(text: &str, dcb: *mut u8) -> i32 {
    if dcb.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut d = [0u8; 28];
    std::ptr::copy_nonoverlapping(dcb, d.as_mut_ptr(), 28);
    if !parse_dcb(text, &mut d) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(d.as_ptr(), dcb, 28);
    1
}

pub unsafe extern "win64" fn BuildCommDCBA(def: *const u8, dcb: *mut u8) -> i32 {
    if def.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let text = std::ffi::CStr::from_ptr(def as *const libc::c_char).to_string_lossy();
    build_dcb(&text, dcb)
}
pub unsafe extern "win64" fn BuildCommDCBW(def: *const u16, dcb: *mut u8) -> i32 {
    if def.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut n = 0;
    while *def.add(n) != 0 {
        n += 1;
    }
    build_dcb(
        &String::from_utf16_lossy(std::slice::from_raw_parts(def, n)),
        dcb,
    )
}
pub unsafe extern "win64" fn BuildCommDCBAndTimeoutsA(
    def: *const u8,
    dcb: *mut u8,
    _t: *mut u32,
) -> i32 {
    BuildCommDCBA(def, dcb)
}
pub unsafe extern "win64" fn BuildCommDCBAndTimeoutsW(
    def: *const u16,
    dcb: *mut u8,
    _t: *mut u32,
) -> i32 {
    BuildCommDCBW(def, dcb)
}

pub unsafe extern "win64" fn TransmitCommChar(h: usize, c: u8) -> i32 {
    let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    (libc::write(fd, &c as *const u8 as *const _, 1) == 1) as i32
}

/// COMMCONFIG (52 octets) : dwSize, wVersion, wReserved, DCB, sous-type…
unsafe fn fill_config(cc: *mut u8, size: *mut u32, dcb: &[u8; 28]) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if cc.is_null() || *size < 52 {
        *size = 52;
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_bytes(cc, 0, 52);
    (cc as *mut u32).write_unaligned(52);
    (cc.add(4) as *mut u16).write_unaligned(1);
    std::ptr::copy_nonoverlapping(dcb.as_ptr(), cc.add(8), 28);
    (cc.add(36) as *mut u32).write_unaligned(1); // PST_RS232
    *size = 52;
    1
}

fn default_dcb() -> [u8; 28] {
    let mut d = [0u8; 28];
    d[0..4].copy_from_slice(&28u32.to_le_bytes());
    d[4..8].copy_from_slice(&9600u32.to_le_bytes());
    d[8..12].copy_from_slice(&0x1011u32.to_le_bytes()); // fBinary, DTR et RTS actifs
    d[14..16].copy_from_slice(&2048u16.to_le_bytes());
    d[16..18].copy_from_slice(&512u16.to_le_bytes());
    d[18] = 8;
    d[21] = 0x11;
    d[22] = 0x13;
    d
}

pub unsafe extern "win64" fn GetCommConfig(h: usize, cc: *mut u8, size: *mut u32) -> i32 {
    let mut dcb = default_dcb();
    if crate::dlls::win32_basics::get_comm_state(h, dcb.as_mut_ptr()) == 0 {
        return 0;
    }
    fill_config(cc, size, &dcb)
}
pub unsafe extern "win64" fn SetCommConfig(h: usize, cc: *const u8, size: u32) -> i32 {
    if cc.is_null() || size < 36 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    crate::dlls::win32_basics::set_comm_state(h, cc.add(8))
}
pub unsafe extern "win64" fn GetDefaultCommConfigW(
    _name: *const u16,
    cc: *mut u8,
    size: *mut u32,
) -> i32 {
    fill_config(cc, size, &default_dcb())
}
pub unsafe extern "win64" fn GetDefaultCommConfigA(
    _name: *const u8,
    cc: *mut u8,
    size: *mut u32,
) -> i32 {
    fill_config(cc, size, &default_dcb())
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "QueryDosDeviceW" => fn QueryDosDeviceW,
        "QueryDosDeviceA" => fn QueryDosDeviceA,
        "SetCommMask" => fn SetCommMask,
        "GetCommMask" => fn GetCommMask,
        "WaitCommEvent" => fn WaitCommEvent,
        "GetCommProperties" => fn GetCommProperties,
        "BuildCommDCBA" => fn BuildCommDCBA,
        "BuildCommDCBW" => fn BuildCommDCBW,
        "BuildCommDCBAndTimeoutsA" => fn BuildCommDCBAndTimeoutsA,
        "BuildCommDCBAndTimeoutsW" => fn BuildCommDCBAndTimeoutsW,
        "TransmitCommChar" => fn TransmitCommChar,
        "GetCommConfig" => fn GetCommConfig,
        "SetCommConfig" => fn SetCommConfig,
        "GetDefaultCommConfigW" => fn GetDefaultCommConfigW,
        "GetDefaultCommConfigA" => fn GetDefaultCommConfigA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dcb_par_texte() {
        let mut d = default_dcb();
        assert!(parse_dcb("115200,e,7,2", &mut d));
        assert_eq!(u32::from_le_bytes(d[4..8].try_into().unwrap()), 115200);
        assert_eq!((d[18], d[19], d[20]), (7, 2, 2));
        let mut d = default_dcb();
        assert!(parse_dcb("COM3: baud=57600 parity=N data=8 stop=1", &mut d));
        assert_eq!(u32::from_le_bytes(d[4..8].try_into().unwrap()), 57600);
        assert_eq!((d[18], d[19], d[20]), (8, 0, 0));
        assert!(parse_dcb("96,n,8,1", &mut d));
        assert_eq!(u32::from_le_bytes(d[4..8].try_into().unwrap()), 9600);
        assert!(!parse_dcb("baud=x", &mut d));
        assert!(!parse_dcb("9600,q,8,1", &mut d));
    }

    /// Chaque lettre de GetLogicalDrives a une cible \\Device\\… (#2448).
    #[test]
    fn chaque_lecteur_annonce_a_une_cible_dos() {
        let mask = crate::dlls::kernel32::get_logical_drives();
        assert_ne!(mask & 0x0200_0000, 0, "Z: annoncé");
        let all = query_dos_device(None).unwrap();
        for i in 0..26u32 {
            if mask & (1 << i) != 0 {
                let letter = format!("{}:", (b'A' + i as u8) as char);
                assert!(all.contains(&letter), "{letter} listé");
                assert!(
                    query_dos_device(Some(&letter)).unwrap()[0].starts_with("\\Device\\"),
                    "{letter}"
                );
            }
        }
    }

    #[test]
    fn noms_dos_et_multi_sz() {
        let all = query_dos_device(None).unwrap();
        assert!(all.iter().any(|n| n == "C:"));
        assert!(query_dos_device(Some("c:")).unwrap()[0].starts_with("\\Device\\HarddiskVolume"));
        assert!(query_dos_device(Some("COM250")).is_none());
        assert_eq!(multi_sz(&["A".into(), "B".into()]), vec![65, 0, 66, 0, 0]);
    }
}
