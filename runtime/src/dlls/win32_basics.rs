//! Fonctions Win32 de base qui manquaient aux logiciels réels (mesure du
//! 2 novembre 2026, docs/COMPATIBILITE.md) : petits calculs (MulDiv,
//! PtInRect…), dessin des bords 3D, saisie clavier ANSI, DPI, police (texte
//! tabulé, mesures par caractère), jeux de caractères, ports série,
//! minuteur multimédia, MIDI (sans périphérique) et quelques fonctions du
//! runtime Visual C++.
//!
//! Chaque fonction s'appuie sur celles déjà fournies par Winlux, retrouvées
//! par leur nom d'export : c'est la version retenue par la table de la DLL
//! (user32 et user32_ext ont des doublons) qui est appelée.

use crate::dlls::{cstr, lookup, wstr};
use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_HANDLE: u32 = 6;

/// Appelle une fonction déjà exportée par Winlux. `None` si elle n'existe pas.
macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                // SAFETY: l'adresse vient de la table d'exports de Winlux et la
                // signature est celle de l'API Windows documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

fn point(packed: u64) -> (i32, i32) {
    (packed as u32 as i32, (packed >> 32) as u32 as i32)
}

// ── kernel32 ────────────────────────────────────────────────────────────

/// MulDiv : (a × b) / c arrondi au plus proche, -1 si c = 0 ou débordement.
pub extern "win64" fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return -1;
    }
    let num = a as i64 * b as i64;
    let den = c as i64;
    // Arrondi à la moitié en s'éloignant de zéro, comme Windows.
    let negative = (num < 0) != (den < 0);
    let (n, d) = (num.abs(), den.abs());
    let q = (n + d / 2) / d;
    let q = if negative { -q } else { q };
    if q > i32::MAX as i64 || q < i32::MIN as i64 {
        return -1;
    }
    q as i32
}

/// Beep : le haut-parleur du PC n'existe plus ; la durée est respectée.
pub extern "win64" fn beep(_freq: u32, duration_ms: u32) -> i32 {
    std::thread::sleep(std::time::Duration::from_millis(
        duration_ms.min(5000) as u64
    ));
    1
}

#[repr(C)]
pub struct MemoryStatus {
    length: u32,
    memory_load: u32,
    total_phys: usize,
    avail_phys: usize,
    total_page_file: usize,
    avail_page_file: usize,
    total_virtual: usize,
    avail_virtual: usize,
}

/// GlobalMemoryStatus : version ancienne de GlobalMemoryStatusEx.
pub unsafe extern "win64" fn global_memory_status(out: *mut MemoryStatus) {
    if out.is_null() {
        return;
    }
    let mut ex = [0u64; 8];
    ex[0] = 64; // dwLength de MEMORYSTATUSEX
    if api!(
        "kernel32",
        "GlobalMemoryStatusEx",
        fn(*mut u64) -> i32,
        ex.as_mut_ptr()
    ) != Some(1)
    {
        return;
    }
    let load = ex[0] >> 32;
    let o = &mut *out;
    o.length = std::mem::size_of::<MemoryStatus>() as u32;
    o.memory_load = load as u32;
    o.total_phys = ex[1] as usize;
    o.avail_phys = ex[2] as usize;
    o.total_page_file = ex[3] as usize;
    o.avail_page_file = ex[4] as usize;
    o.total_virtual = ex[5] as usize;
    o.avail_virtual = ex[6] as usize;
}

/// OpenFileMapping : Winlux ne crée pas de projections nommées, aucune ne
/// peut donc être ouverte (Pageant absent pour PuTTY, par exemple).
pub extern "win64" fn open_file_mapping(_access: u32, _inherit: i32, _name: *const u8) -> usize {
    set_last_error(2); // ERROR_FILE_NOT_FOUND
    0
}

pub unsafe extern "win64" fn wait_named_pipe_a(name: *const u8, timeout: u32) -> i32 {
    let w: Vec<u16> = cstr(name).encode_utf16().chain([0]).collect();
    api!(
        "kernel32",
        "WaitNamedPipeW",
        fn(*const u16, u32) -> i32,
        w.as_ptr(),
        timeout
    )
    .unwrap_or(0)
}

/// AddDllDirectory / RemoveDllDirectory / SetDefaultDllDirectories /
/// SetDllDirectory : dossiers que le chargeur ajoute à sa recherche.
pub unsafe extern "win64" fn add_dll_directory(dir: *const u16) -> usize {
    if dir.is_null() {
        set_last_error(87);
        return 0;
    }
    let c = crate::loader::add_dll_directory(&wstr(dir));
    if c == 0 {
        set_last_error(87);
    }
    c
}
pub extern "win64" fn remove_dll_directory(cookie: usize) -> i32 {
    if crate::loader::remove_dll_directory(cookie) {
        1
    } else {
        set_last_error(87);
        0
    }
}
pub extern "win64" fn set_default_dll_directories(flags: u32) -> i32 {
    // LOAD_LIBRARY_SEARCH_APPLICATION_DIR | USER_DIRS | SYSTEM32 | DEFAULT_DIRS
    // (et SYSTEM32_NO_FORWARDER) : rien d'autre n'est admis.
    if flags == 0 || flags & !(0x200 | 0x400 | 0x800 | 0x1000 | 0x4000) != 0 {
        set_last_error(87);
        return 0;
    }
    crate::loader::set_default_dll_directories(flags);
    1
}
pub unsafe extern "win64" fn set_dll_directory_w(dir: *const u16) -> i32 {
    let d = (!dir.is_null()).then(|| wstr(dir));
    crate::loader::set_dll_directory(d.as_deref());
    1
}
pub unsafe extern "win64" fn set_dll_directory_a(dir: *const u8) -> i32 {
    let d = (!dir.is_null()).then(|| cstr(dir));
    crate::loader::set_dll_directory(d.as_deref());
    1
}

/// GetLogicalDriveStringsW : « C:\\ », « D:\\ »… d'après GetLogicalDrives.
pub unsafe extern "win64" fn get_logical_drive_strings_w(len: u32, buf: *mut u16) -> u32 {
    let mask = api!("kernel32", "GetLogicalDrives", fn() -> u32).unwrap_or(1 << 2);
    let mut out: Vec<u16> = Vec::new();
    for i in 0..26u16 {
        if mask & (1 << i) != 0 {
            out.extend_from_slice(&[b'A' as u16 + i, b':' as u16, b'\\' as u16, 0]);
        }
    }
    if buf.is_null() || (len as usize) < out.len() + 1 {
        return out.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    *buf.add(out.len()) = 0;
    out.len() as u32
}

pub unsafe extern "win64" fn get_logical_drive_strings_a(len: u32, buf: *mut u8) -> u32 {
    let mut w = vec![0u16; 26 * 4 + 1];
    let n = get_logical_drive_strings_w(w.len() as u32, w.as_mut_ptr()) as usize;
    if buf.is_null() || (len as usize) < n + 1 {
        return n as u32 + 1;
    }
    for (i, c) in w[..=n].iter().enumerate() {
        *buf.add(i) = *c as u8;
    }
    n as u32
}

/// GetVolumePathNameW : racine du lecteur qui contient le chemin.
pub unsafe extern "win64" fn get_volume_path_name_w(
    path: *const u16,
    out: *mut u16,
    len: u32,
) -> i32 {
    let p = wstr(path);
    let full = crate::paths::to_windows(&crate::paths::to_linux(&p));
    let root = match full.get(..3) {
        Some(r) if r.as_bytes()[1] == b':' => r.to_string(),
        _ => "C:\\".to_string(),
    };
    if out.is_null() || (len as usize) <= root.len() {
        set_last_error(206); // ERROR_FILENAME_EXCED_RANGE
        return 0;
    }
    crate::dlls::put_w(&root, out, len);
    1
}

/// Nom de volume `\\?\Volume{GUID}\` de la racine d'un lecteur : GUID
/// stable, tiré de la lettre et du périphérique Linux qui porte le lecteur.
fn volume_name_for_root(root: &str) -> Result<String, u32> {
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const ERROR_INVALID_NAME: u32 = 123;
    const ERROR_NOT_A_REPARSE_POINT: u32 = 4390;
    if !root.ends_with('\\') {
        return Err(ERROR_INVALID_NAME);
    }
    let up = root.to_ascii_uppercase();
    if up.starts_with("\\\\?\\VOLUME{") && up.ends_with("}\\") && root.len() == 49 {
        return Ok(root.to_string());
    }
    let b = up.as_bytes();
    let target = crate::paths::to_linux(root);
    let meta = std::fs::metadata(&target).map_err(|_| ERROR_FILE_NOT_FOUND)?;
    // Seules les racines de lecteur sont des points de montage ici.
    if b.len() != 3 || !b[0].is_ascii_uppercase() || b[1] != b':' {
        return Err(ERROR_NOT_A_REPARSE_POINT);
    }
    use std::os::unix::fs::MetadataExt;
    // FNV-1a 64 bits de la lettre et du numéro de périphérique.
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in [b[0]].into_iter().chain(meta.dev().to_le_bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    Ok(format!(
        "\\\\?\\Volume{{{:08x}-{:04x}-{:04x}-{:04x}-{:012x}}}\\",
        0x5749_4E4Cu32, // « WINL »
        (h >> 48) as u16,
        0x4000 | ((h >> 32) as u16 & 0x0FFF),
        0x8000 | ((h >> 16) as u16 & 0x3FFF),
        (h & 0xFFFF) << 32 | u64::from(b[0]),
    ))
}

/// GetVolumeNameForVolumeMountPointW (#1112, diagnostic de Paint.NET).
pub unsafe extern "win64" fn get_volume_name_for_volume_mount_point_w(
    mount_point: *const u16,
    out: *mut u16,
    len: u32,
) -> i32 {
    if mount_point.is_null() || out.is_null() {
        set_last_error(87);
        return 0;
    }
    match volume_name_for_root(&wstr(mount_point)) {
        Ok(name) if (len as usize) > name.len() => {
            crate::dlls::put_w(&name, out, len);
            1
        }
        Ok(_) => {
            set_last_error(206); // ERROR_FILENAME_EXCED_RANGE
            0
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

pub unsafe extern "win64" fn get_volume_name_for_volume_mount_point_a(
    mount_point: *const u8,
    out: *mut u8,
    len: u32,
) -> i32 {
    if mount_point.is_null() || out.is_null() {
        set_last_error(87);
        return 0;
    }
    match volume_name_for_root(&cstr(mount_point)) {
        Ok(name) if (len as usize) > name.len() => {
            std::ptr::copy_nonoverlapping(name.as_ptr(), out, name.len());
            *out.add(name.len()) = 0;
            1
        }
        Ok(_) => {
            set_last_error(206);
            0
        }
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

/// Chemin Windows du programme d'un autre processus (#2041), pour
/// QueryFullProcessImageNameW. Seulement pour un processus du même compte :
/// celui d'un autre compte reste refusé (ERROR_ACCESS_DENIED), ce qui
/// n'expose rien de plus que Windows. Un programme Windows lancé par
/// Winlux a pour image l'exécutable Winlux et son .exe en premier argument ;
/// un programme Linux a pour chemin celui de son exécutable, vu en `Z:\`.
pub(crate) fn foreign_image_path(pid: i32) -> Result<String, u32> {
    const ERROR_ACCESS_DENIED: u32 = 5;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .map_err(|_| ERROR_INVALID_PARAMETER)?;
    let real_uid = status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|u| u.parse::<u32>().ok());
    if real_uid != Some(unsafe { libc::getuid() }) {
        return Err(ERROR_ACCESS_DENIED);
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|_| ERROR_ACCESS_DENIED)?;
    let runtime = std::fs::read_link("/proc/self/exe").ok();
    if runtime.as_deref() != Some(exe.as_path()) {
        return Ok(crate::paths::to_windows(&exe));
    }
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).map_err(|_| ERROR_ACCESS_DENIED)?;
    let arg = cmdline
        .split(|b| *b == 0)
        .nth(1)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .filter(|a| !a.is_empty() && !a.starts_with("--"))
        .ok_or(ERROR_ACCESS_DENIED)?;
    let windows_form = arg.contains('\\') || arg.as_bytes().get(1) == Some(&b':');
    if windows_form {
        return Ok(crate::paths::full_windows(&arg));
    }
    let mut path = std::path::PathBuf::from(&arg);
    if path.is_relative() {
        let cwd =
            std::fs::read_link(format!("/proc/{pid}/cwd")).map_err(|_| ERROR_ACCESS_DENIED)?;
        path = cwd.join(path);
    }
    Ok(crate::paths::to_windows(&path))
}

/// Chemin Windows du programme désigné par `process` : le nôtre, ou celui
/// d'un autre processus du même compte (#2041).
unsafe fn process_image_path(process: usize) -> Result<String, u32> {
    if let Some(pid) = crate::dlls::kernel32::process_os_pid(process) {
        if pid != std::process::id() as i32 {
            return foreign_image_path(pid);
        }
    }
    let mut buf = vec![0u16; 1024];
    let got = api!(
        "kernel32",
        "K32GetModuleFileNameExW",
        fn(usize, usize, *mut u16, u32) -> u32,
        process,
        0,
        buf.as_mut_ptr(),
        buf.len() as u32
    )
    .unwrap_or(0) as usize;
    if got == 0 {
        return Err(crate::process::last_error());
    }
    Ok(String::from_utf16_lossy(&buf[..got]))
}

/// QueryFullProcessImageNameW : chemin Windows du programme d'un processus.
pub unsafe extern "win64" fn query_full_process_image_name_w(
    process: usize,
    flags: u32,
    out: *mut u16,
    size: *mut u32,
) -> i32 {
    if out.is_null() || size.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let found = process_image_path(process);
    crate::trace!(
        "QueryFullProcessImageNameW(pid {:?}, flags {flags:#x}) -> {:?}",
        crate::dlls::kernel32::process_os_pid(process),
        found
    );
    let mut text = match found {
        Ok(t) => t,
        Err(e) => {
            set_last_error(e);
            return 0;
        }
    };
    if flags & 1 != 0 {
        // PROCESS_NAME_NATIVE : forme « \Device\HarddiskVolume1\… ».
        text = format!("\\Device\\HarddiskVolume1{}", text.get(2..).unwrap_or(""));
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    if (*size as usize) <= w.len() {
        set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    *out.add(w.len()) = 0;
    *size = w.len() as u32;
    1
}

pub unsafe extern "win64" fn query_full_process_image_name_a(
    process: usize,
    flags: u32,
    out: *mut u8,
    size: *mut u32,
) -> i32 {
    if out.is_null() || size.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut w = vec![0u16; 1024];
    let mut n = w.len() as u32;
    if query_full_process_image_name_w(process, flags, w.as_mut_ptr(), &mut n) == 0 {
        return 0;
    }
    let bytes = crate::dlls::encode_ansi(&String::from_utf16_lossy(&w[..n as usize]));
    if (*size as usize) <= bytes.len() {
        set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out.add(bytes.len()) = 0;
    *size = bytes.len() as u32;
    1
}

// ── Ports série (DCB, COMMTIMEOUTS) sur termios ────────────────────────

fn serial_fd(handle: usize) -> Option<i32> {
    let fd = crate::dlls::kernel32::handle_to_fd(handle);
    if fd < 0 {
        set_last_error(ERROR_INVALID_HANDLE);
        return None;
    }
    Some(fd)
}

const BAUDS: [(u32, libc::speed_t); 18] = [
    (110, libc::B110),
    (300, libc::B300),
    (600, libc::B600),
    (1200, libc::B1200),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
    (57600, libc::B57600),
    (115200, libc::B115200),
    (230400, libc::B230400),
    (460800, libc::B460800),
    (500000, libc::B500000),
    (921600, libc::B921600),
    (1000000, libc::B1000000),
    (2000000, libc::B2000000),
    (4000000, libc::B4000000),
];

/// DCB (28 octets) : DCBlength, BaudRate, bits (fBinary…), wReserved,
/// XonLim, XoffLim, ByteSize, Parity, StopBits, XonChar, XoffChar…
pub unsafe extern "win64" fn get_comm_state(handle: usize, dcb: *mut u8) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    if dcb.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut t: libc::termios = std::mem::zeroed();
    if libc::tcgetattr(fd, &mut t) != 0 {
        set_last_error(1); // ERROR_INVALID_FUNCTION : pas un port série
        return 0;
    }
    let speed = libc::cfgetospeed(&t);
    let baud = BAUDS
        .iter()
        .find(|(_, s)| *s == speed)
        .map_or(9600, |(b, _)| *b);
    std::ptr::write_bytes(dcb, 0, 28);
    std::ptr::write_unaligned(dcb as *mut u32, 28);
    std::ptr::write_unaligned(dcb.add(4) as *mut u32, baud);
    let mut bits: u32 = 1; // fBinary
    if t.c_cflag & libc::PARENB != 0 {
        bits |= 1 << 1; // fParity
    }
    if t.c_cflag & libc::CRTSCTS != 0 {
        bits |= 1 << 2; // fOutxCtsFlow
        bits |= 2 << 12; // fRtsControl = RTS_CONTROL_HANDSHAKE
    } else {
        bits |= 1 << 12; // RTS_CONTROL_ENABLE
    }
    bits |= 1 << 4; // fDtrControl = DTR_CONTROL_ENABLE
    if t.c_iflag & libc::IXON != 0 {
        bits |= 1 << 8; // fOutX
    }
    if t.c_iflag & libc::IXOFF != 0 {
        bits |= 1 << 9; // fInX
    }
    std::ptr::write_unaligned(dcb.add(8) as *mut u32, bits);
    std::ptr::write_unaligned(dcb.add(14) as *mut u16, 2048);
    std::ptr::write_unaligned(dcb.add(16) as *mut u16, 512);
    *dcb.add(18) = match t.c_cflag & libc::CSIZE {
        libc::CS5 => 5,
        libc::CS6 => 6,
        libc::CS7 => 7,
        _ => 8,
    };
    *dcb.add(19) = if t.c_cflag & libc::PARENB == 0 {
        0
    } else if t.c_cflag & libc::PARODD != 0 {
        1
    } else {
        2
    };
    *dcb.add(20) = if t.c_cflag & libc::CSTOPB != 0 { 2 } else { 0 };
    *dcb.add(21) = 0x11; // XonChar
    *dcb.add(22) = 0x13; // XoffChar
    1
}

pub unsafe extern "win64" fn set_comm_state(handle: usize, dcb: *const u8) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    if dcb.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut t: libc::termios = std::mem::zeroed();
    if libc::tcgetattr(fd, &mut t) != 0 {
        set_last_error(1);
        return 0;
    }
    libc::cfmakeraw(&mut t);
    let baud = std::ptr::read_unaligned(dcb.add(4) as *const u32);
    let Some((_, speed)) = BAUDS.iter().find(|(b, _)| *b == baud) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    libc::cfsetispeed(&mut t, *speed);
    libc::cfsetospeed(&mut t, *speed);
    let bits = std::ptr::read_unaligned(dcb.add(8) as *const u32);
    t.c_cflag |= libc::CLOCAL | libc::CREAD;
    t.c_cflag &= !libc::CSIZE;
    t.c_cflag |= match *dcb.add(18) {
        5 => libc::CS5,
        6 => libc::CS6,
        7 => libc::CS7,
        _ => libc::CS8,
    };
    t.c_cflag &= !(libc::PARENB | libc::PARODD | libc::CSTOPB | libc::CRTSCTS);
    match *dcb.add(19) {
        1 => t.c_cflag |= libc::PARENB | libc::PARODD,
        2 => t.c_cflag |= libc::PARENB,
        _ => {}
    }
    if *dcb.add(20) != 0 {
        t.c_cflag |= libc::CSTOPB;
    }
    if bits & (1 << 2) != 0 || (bits >> 12) & 3 == 2 {
        t.c_cflag |= libc::CRTSCTS;
    }
    t.c_iflag &= !(libc::IXON | libc::IXOFF);
    if bits & (1 << 8) != 0 {
        t.c_iflag |= libc::IXON;
    }
    if bits & (1 << 9) != 0 {
        t.c_iflag |= libc::IXOFF;
    }
    if libc::tcsetattr(fd, libc::TCSANOW, &t) != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    1
}

/// Délais de lecture : Winlux les applique par VMIN/VTIME (au dixième de
/// seconde) ; une lecture « immédiate » (MAXDWORD, 0, 0) devient non bloquante.
pub unsafe extern "win64" fn set_comm_timeouts(handle: usize, timeouts: *const u32) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    if timeouts.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let interval = *timeouts;
    let total_mult = *timeouts.add(1);
    let total_const = *timeouts.add(2);
    let mut t: libc::termios = std::mem::zeroed();
    if libc::tcgetattr(fd, &mut t) != 0 {
        set_last_error(1);
        return 0;
    }
    if interval == u32::MAX && total_mult == 0 && total_const == 0 {
        t.c_cc[libc::VMIN] = 0;
        t.c_cc[libc::VTIME] = 0;
    } else if total_const > 0 {
        t.c_cc[libc::VMIN] = 0;
        t.c_cc[libc::VTIME] = (total_const.div_ceil(100)).min(255) as u8;
    } else {
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
    }
    libc::tcsetattr(fd, libc::TCSANOW, &t);
    *COMM_TIMEOUTS.lock().unwrap_or_else(|e| e.into_inner()) = [
        interval,
        total_mult,
        total_const,
        *timeouts.add(3),
        *timeouts.add(4),
    ];
    1
}

static COMM_TIMEOUTS: std::sync::Mutex<[u32; 5]> = std::sync::Mutex::new([0; 5]);

pub unsafe extern "win64" fn get_comm_timeouts(handle: usize, timeouts: *mut u32) -> i32 {
    if serial_fd(handle).is_none() || timeouts.is_null() {
        return 0;
    }
    let t = *COMM_TIMEOUTS.lock().unwrap_or_else(|e| e.into_inner());
    std::ptr::copy_nonoverlapping(t.as_ptr(), timeouts, 5);
    1
}

pub unsafe extern "win64" fn set_comm_break(handle: usize) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    (libc::ioctl(fd, libc::TIOCSBRK) == 0) as i32
}

pub unsafe extern "win64" fn clear_comm_break(handle: usize) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    (libc::ioctl(fd, libc::TIOCCBRK) == 0) as i32
}

pub extern "win64" fn setup_comm(handle: usize, _in_queue: u32, _out_queue: u32) -> i32 {
    serial_fd(handle).is_some() as i32
}

/// PurgeComm : PURGE_TXCLEAR (4) et PURGE_RXCLEAR (8).
pub unsafe extern "win64" fn purge_comm(handle: usize, flags: u32) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    let queue = match (flags & 4 != 0, flags & 8 != 0) {
        (true, true) => libc::TCIOFLUSH,
        (true, false) => libc::TCOFLUSH,
        (false, true) => libc::TCIFLUSH,
        (false, false) => return 1,
    };
    (libc::tcflush(fd, queue) == 0) as i32
}

/// EscapeCommFunction : SETRTS 3, CLRRTS 4, SETDTR 5, CLRDTR 6, SETBREAK 8, CLRBREAK 9.
pub unsafe extern "win64" fn escape_comm_function(handle: usize, func: u32) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    let r = match func {
        3..=6 => {
            let bit: libc::c_int = if func <= 4 {
                libc::TIOCM_RTS
            } else {
                libc::TIOCM_DTR
            };
            let req = if func % 2 == 1 {
                libc::TIOCMBIS
            } else {
                libc::TIOCMBIC
            };
            libc::ioctl(fd, req, &bit)
        }
        8 => libc::ioctl(fd, libc::TIOCSBRK),
        9 => libc::ioctl(fd, libc::TIOCCBRK),
        _ => 0,
    };
    (r == 0) as i32
}

/// GetCommModemStatus : MS_CTS_ON 0x10, MS_DSR_ON 0x20, MS_RING_ON 0x40, MS_RLSD_ON 0x80.
pub unsafe extern "win64" fn get_comm_modem_status(handle: usize, status: *mut u32) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    let mut bits: libc::c_int = 0;
    if libc::ioctl(fd, libc::TIOCMGET, &mut bits) != 0 {
        return 0;
    }
    if !status.is_null() {
        let mut s = 0;
        if bits & libc::TIOCM_CTS != 0 {
            s |= 0x10;
        }
        if bits & libc::TIOCM_DSR != 0 {
            s |= 0x20;
        }
        if bits & libc::TIOCM_RNG != 0 {
            s |= 0x40;
        }
        if bits & libc::TIOCM_CAR != 0 {
            s |= 0x80;
        }
        *status = s;
    }
    1
}

/// ClearCommError : aucune erreur mémorisée ; COMSTAT.cbInQue d'après FIONREAD.
pub unsafe extern "win64" fn clear_comm_error(
    handle: usize,
    errors: *mut u32,
    stat: *mut u32,
) -> i32 {
    let Some(fd) = serial_fd(handle) else {
        return 0;
    };
    if !errors.is_null() {
        *errors = 0;
    }
    if !stat.is_null() {
        let mut pending: libc::c_int = 0;
        libc::ioctl(fd, libc::FIONREAD, &mut pending);
        *stat = 0;
        *stat.add(1) = pending.max(0) as u32;
        *stat.add(2) = 0;
    }
    1
}

// ── user32 ──────────────────────────────────────────────────────────────

pub unsafe extern "win64" fn pt_in_rect(rect: *const Rect, pt: u64) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let (x, y) = point(pt);
    let r = &*rect;
    (x >= r.left && x < r.right && y >= r.top && y < r.bottom) as i32
}

pub unsafe extern "win64" fn offset_rect(rect: *mut Rect, dx: i32, dy: i32) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = &mut *rect;
    r.left += dx;
    r.right += dx;
    r.top += dy;
    r.bottom += dy;
    1
}

pub unsafe extern "win64" fn get_queue_status(flags: u32) -> u32 {
    // Haut mot : types présents dans la file ; bas mot : arrivés depuis le
    // dernier appel. Winlux répond « rien en attente » si la file est vide.
    let _ = flags;
    0
}

pub extern "win64" fn set_keyboard_state(_state: *const u8) -> i32 {
    1
}

/// FlashWindow(Ex) : attire l'attention ; renvoie l'état précédent (aucun).
pub extern "win64" fn flash_window(_hwnd: usize, _invert: i32) -> i32 {
    0
}
pub extern "win64" fn flash_window_ex(_info: *const u8) -> i32 {
    0
}

unsafe fn to_ascii_common(vk: u32, scan: u32, state: *const u8, out: *mut u16, flags: u32) -> i32 {
    let mut buf = [0u16; 4];
    let n = api!(
        "user32",
        "ToUnicode",
        fn(u32, u32, *const u8, *mut u16, i32, u32) -> i32,
        vk,
        scan,
        state,
        buf.as_mut_ptr(),
        4,
        flags
    )
    .unwrap_or(0);
    if n <= 0 {
        return n;
    }
    let text = String::from_utf16_lossy(&buf[..n as usize]);
    let bytes = crate::dlls::encode_ansi(&text);
    if out.is_null() || bytes.is_empty() {
        return 0;
    }
    let b0 = bytes[0] as u16;
    let b1 = bytes.get(1).copied().unwrap_or(0) as u16;
    *out = b0 | (b1 << 8);
    bytes.len().min(2) as i32
}

pub unsafe extern "win64" fn to_ascii(
    vk: u32,
    scan: u32,
    state: *const u8,
    out: *mut u16,
    flags: u32,
) -> i32 {
    to_ascii_common(vk, scan, state, out, flags)
}

pub unsafe extern "win64" fn to_ascii_ex(
    vk: u32,
    scan: u32,
    state: *const u8,
    out: *mut u16,
    flags: u32,
    _layout: usize,
) -> i32 {
    to_ascii_common(vk, scan, state, out, flags)
}

pub extern "win64" fn get_menu_default_item(_menu: usize, _by_pos: u32, _flags: u32) -> u32 {
    u32::MAX
}
pub extern "win64" fn set_menu_default_item(_menu: usize, _item: u32, _by_pos: u32) -> i32 {
    1
}

pub unsafe extern "win64" fn set_class_long_a(hwnd: usize, index: i32, value: i32) -> u32 {
    api!(
        "user32",
        "SetClassLongPtrA",
        fn(usize, i32, isize) -> usize,
        hwnd,
        index,
        value as isize
    )
    .unwrap_or(0) as u32
}
pub unsafe extern "win64" fn set_class_long_w(hwnd: usize, index: i32, value: i32) -> u32 {
    api!(
        "user32",
        "SetClassLongPtrW",
        fn(usize, i32, isize) -> usize,
        hwnd,
        index,
        value as isize
    )
    .unwrap_or(0) as u32
}

/// MessageBoxIndirect : MSGBOXPARAMS (texte, titre, style) → MessageBox.
pub unsafe extern "win64" fn message_box_indirect_a(p: *const u8) -> i32 {
    if p.is_null() {
        return 0;
    }
    let owner = std::ptr::read_unaligned(p.add(8) as *const usize);
    let text = std::ptr::read_unaligned(p.add(24) as *const *const u8);
    let caption = std::ptr::read_unaligned(p.add(32) as *const *const u8);
    let style = std::ptr::read_unaligned(p.add(40) as *const u32) & !0x80; // MB_USERICON
    api!(
        "user32",
        "MessageBoxA",
        fn(usize, *const u8, *const u8, u32) -> i32,
        owner,
        text,
        caption,
        style
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn message_box_indirect_w(p: *const u8) -> i32 {
    if p.is_null() {
        return 0;
    }
    let owner = std::ptr::read_unaligned(p.add(8) as *const usize);
    let text = std::ptr::read_unaligned(p.add(24) as *const *const u16);
    let caption = std::ptr::read_unaligned(p.add(32) as *const *const u16);
    let style = std::ptr::read_unaligned(p.add(40) as *const u32) & !0x80;
    api!(
        "user32",
        "MessageBoxW",
        fn(usize, *const u16, *const u16, u32) -> i32,
        owner,
        text,
        caption,
        style
    )
    .unwrap_or(0)
}

pub unsafe extern "win64" fn check_radio_button(
    dlg: usize,
    first: i32,
    last: i32,
    check: i32,
) -> i32 {
    for id in first..=last {
        api!(
            "user32",
            "CheckDlgButton",
            fn(usize, i32, u32) -> i32,
            dlg,
            id,
            (id == check) as u32
        );
    }
    1
}

/// GetDlgItemInt : texte du contrôle lu comme un entier (signé ou non).
pub unsafe extern "win64" fn get_dlg_item_int(
    dlg: usize,
    id: i32,
    ok: *mut i32,
    signed: i32,
) -> u32 {
    let mut buf = [0u8; 64];
    let n = api!(
        "user32",
        "GetDlgItemTextA",
        fn(usize, i32, *mut u8, i32) -> u32,
        dlg,
        id,
        buf.as_mut_ptr(),
        64
    )
    .unwrap_or(0) as usize;
    let text = String::from_utf8_lossy(&buf[..n.min(64)]);
    let text = text.trim();
    let value: Option<i64> = text.parse().ok().filter(|v: &i64| {
        if signed != 0 {
            *v >= i32::MIN as i64 && *v <= i32::MAX as i64
        } else {
            *v >= 0 && *v <= u32::MAX as i64
        }
    });
    if !ok.is_null() {
        *ok = value.is_some() as i32;
    }
    value.unwrap_or(0) as u32
}

pub unsafe extern "win64" fn set_dlg_item_int(dlg: usize, id: i32, value: u32, signed: i32) -> i32 {
    let text = if signed != 0 {
        format!("{}\0", value as i32)
    } else {
        format!("{value}\0")
    };
    api!(
        "user32",
        "SetDlgItemTextA",
        fn(usize, i32, *const u8) -> i32,
        dlg,
        id,
        text.as_ptr()
    )
    .unwrap_or(0)
}

/// Unités de dialogue de la police par défaut (MS Shell Dlg 8 pt à 96 ppp) :
/// 4 unités horizontales = 6 pixels, 8 unités verticales = 13 pixels.
pub const DIALOG_BASE_X: i32 = 6;
pub const DIALOG_BASE_Y: i32 = 13;

pub extern "win64" fn get_dialog_base_units() -> i32 {
    // Police système : 8 × 16.
    (16 << 16) | 8
}

pub unsafe extern "win64" fn map_dialog_rect(_dlg: usize, rect: *mut Rect) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = &mut *rect;
    r.left = mul_div(r.left, DIALOG_BASE_X, 4);
    r.right = mul_div(r.right, DIALOG_BASE_X, 4);
    r.top = mul_div(r.top, DIALOG_BASE_Y, 8);
    r.bottom = mul_div(r.bottom, DIALOG_BASE_Y, 8);
    1
}

/// DefDlgProc : procédure des classes de dialogue privées (PuTTY déclare la
/// sienne avec lpfnWndProc = DefDlgProc). Sans gestionnaire de dialogues,
/// elle se comporte comme DefWindowProc.
pub unsafe extern "win64" fn def_dlg_proc_a(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    api!(
        "user32",
        "DefWindowProcA",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}
pub unsafe extern "win64" fn def_dlg_proc_w(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    api!(
        "user32",
        "DefWindowProcW",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}

unsafe fn screen_dpi() -> u32 {
    let hdc = api!("user32", "GetDC", fn(usize) -> usize, 0).unwrap_or(0);
    let dpi = api!("gdi32", "GetDeviceCaps", fn(usize, i32) -> i32, hdc, 88).unwrap_or(96); // LOGPIXELSX
    api!("user32", "ReleaseDC", fn(usize, usize) -> i32, 0, hdc);
    if dpi > 0 {
        dpi as u32
    } else {
        96
    }
}

pub unsafe extern "win64" fn get_dpi_for_window(_hwnd: usize) -> u32 {
    screen_dpi()
}
pub unsafe extern "win64" fn get_dpi_for_system() -> u32 {
    screen_dpi()
}

/// Mesures de GetSystemMetrics qui ne sont pas des tailles en pixels
/// (nombres, booléens) : jamais mises à l'échelle.
fn metric_is_size(index: i32) -> bool {
    !matches!(index, 19 | 20 | 22 | 23 | 40..=43 | 63 | 67 | 73 | 74 | 75 | 80 | 86 | 87 | 89 | 90 | 91 | 94 | 0x1000..=0x2005)
}

pub unsafe extern "win64" fn get_system_metrics_for_dpi(index: i32, dpi: u32) -> i32 {
    let v = api!("user32", "GetSystemMetrics", fn(i32) -> i32, index).unwrap_or(0);
    let system = screen_dpi();
    if dpi == 0 || dpi == system || !metric_is_size(index) {
        return v;
    }
    mul_div(v, dpi as i32, system as i32)
}

pub unsafe extern "win64" fn adjust_window_rect_ex_for_dpi(
    rect: *mut Rect,
    style: u32,
    menu: i32,
    ex_style: u32,
    _dpi: u32,
) -> i32 {
    api!(
        "user32",
        "AdjustWindowRectEx",
        fn(*mut Rect, u32, i32, u32) -> i32,
        rect,
        style,
        menu,
        ex_style
    )
    .unwrap_or(0)
}

/// SetProcessDPIAware : le processus devient « conscient du DPI système ».
pub extern "win64" fn set_process_dpi_aware() -> i32 {
    crate::dlls::chromium_win32::SetProcessDpiAwareness(1);
    1
}

/// DEVMODEW des réglages d'écran actuels (ENUM_CURRENT_SETTINGS = -1,
/// ENUM_REGISTRY_SETTINGS = -2, ou le mode 0 : le seul que Winlux énumère).
unsafe fn fill_devmode(dm: *mut u8, wide: bool) -> bool {
    let (w, h) = crate::dlls::gui::screen_size();
    let after_name = if wide { 0 } else { 32 };
    let base = |off: usize| off - after_name; // les champs suivent le nom
    let declared = std::ptr::read_unaligned(dm.add(base(68)) as *const u16) as usize;
    let full = if wide { 220 } else { 156 };
    let rest = if wide { 166 } else { 102 };
    let size = if declared > 0 {
        declared.min(full)
    } else {
        full
    };
    if size < rest + 22 {
        return false;
    }
    std::ptr::write_bytes(dm, 0, size);
    let name = "\\\\.\\DISPLAY1";
    for (i, c) in name.encode_utf16().enumerate() {
        if wide {
            std::ptr::write_unaligned(dm.add(i * 2) as *mut u16, c);
        } else {
            *dm.add(i) = c as u8;
        }
    }
    std::ptr::write_unaligned(dm.add(base(64)) as *mut u16, 0x0401); // dmSpecVersion
    std::ptr::write_unaligned(dm.add(base(68)) as *mut u16, size as u16);
    // DM_POSITION | DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_DISPLAYFLAGS
    std::ptr::write_unaligned(
        dm.add(base(72)) as *mut u32,
        0x20 | 0x4_0000 | 0x8_0000 | 0x10_0000 | 0x40_0000 | 0x20_0000,
    );
    std::ptr::write_unaligned(dm.add(rest) as *mut u16, 96); // dmLogPixels
    std::ptr::write_unaligned(dm.add(rest + 2) as *mut u32, 32);
    std::ptr::write_unaligned(dm.add(rest + 6) as *mut u32, w.max(1) as u32);
    std::ptr::write_unaligned(dm.add(rest + 10) as *mut u32, h.max(1) as u32);
    std::ptr::write_unaligned(dm.add(rest + 18) as *mut u32, 60);
    true
}

pub unsafe extern "win64" fn enum_display_settings_w(
    _device: *const u16,
    mode: u32,
    dm: *mut u8,
) -> i32 {
    if dm.is_null() || !(mode == u32::MAX || mode == u32::MAX - 1 || mode == 0) {
        return 0;
    }
    fill_devmode(dm, true) as i32
}

pub unsafe extern "win64" fn enum_display_settings_a(
    _device: *const u8,
    mode: u32,
    dm: *mut u8,
) -> i32 {
    if dm.is_null() || !(mode == u32::MAX || mode == u32::MAX - 1 || mode == 0) {
        return 0;
    }
    fill_devmode(dm, false) as i32
}

pub unsafe extern "win64" fn enum_display_settings_ex_w(
    device: *const u16,
    mode: u32,
    dm: *mut u8,
    _flags: u32,
) -> i32 {
    enum_display_settings_w(device, mode, dm)
}

/// EnumDisplayDevicesW : un seul adaptateur, « \\.\DISPLAY1 », actif et principal.
pub unsafe extern "win64" fn enum_display_devices_w(
    device: *const u16,
    index: u32,
    out: *mut u8,
    _flags: u32,
) -> i32 {
    if out.is_null() || index != 0 {
        return 0;
    }
    // DISPLAY_DEVICEW : cb, DeviceName[32], DeviceString[128], StateFlags,
    // DeviceID[128], DeviceKey[128] (840 octets).
    std::ptr::write_bytes(out.add(4), 0, 836);
    let put = |off: usize, s: &str| {
        for (i, c) in s.encode_utf16().take(127).enumerate() {
            std::ptr::write_unaligned(out.add(off + i * 2) as *mut u16, c);
        }
    };
    if device.is_null() {
        put(4, "\\\\.\\DISPLAY1");
        put(68, "Winlux Display Adapter");
        // DISPLAY_DEVICE_ATTACHED_TO_DESKTOP | DISPLAY_DEVICE_PRIMARY_DEVICE
        std::ptr::write_unaligned(out.add(324) as *mut u32, 0x1 | 0x4);
    } else {
        put(4, "\\\\.\\DISPLAY1\\Monitor0");
        put(68, "Generic PnP Monitor");
        std::ptr::write_unaligned(out.add(324) as *mut u32, 0x1 | 0x2);
    }
    1
}

/// DrawEdge : bords 3D (BDR_RAISED*, BDR_SUNKEN*) sur les côtés BF_* ;
/// BF_MIDDLE remplit, BF_ADJUST rétrécit le rectangle.
pub unsafe extern "win64" fn draw_edge(hdc: usize, rect: *mut Rect, edge: u32, flags: u32) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = *rect;
    let sys = |i: i32| api!("user32", "GetSysColor", fn(i32) -> u32, i).unwrap_or(0);
    // (haut-gauche, bas-droite) pour l'anneau extérieur puis intérieur.
    let outer = match edge & 0x3 {
        1 => Some((sys(22), sys(21))), // RAISEDOUTER : 3DLIGHT, 3DDKSHADOW
        2 => Some((sys(16), sys(20))), // SUNKENOUTER : 3DSHADOW, 3DHILIGHT
        _ => None,
    };
    let inner = match edge & 0xC {
        4 => Some((sys(20), sys(16))), // RAISEDINNER : 3DHILIGHT, 3DSHADOW
        8 => Some((sys(21), sys(22))), // SUNKENINNER : 3DDKSHADOW, 3DLIGHT
        _ => None,
    };
    let mut cur = r;
    for ring in [outer, inner].into_iter().flatten() {
        line_ring(hdc, cur, ring.0, ring.1, flags);
        if flags & 0x1 != 0 {
            cur.left += 1;
        }
        if flags & 0x2 != 0 {
            cur.top += 1;
        }
        if flags & 0x4 != 0 {
            cur.right -= 1;
        }
        if flags & 0x8 != 0 {
            cur.bottom -= 1;
        }
    }
    if flags & 0x800 != 0 {
        let brush = api!("user32", "GetSysColorBrush", fn(i32) -> usize, 15).unwrap_or(0); // COLOR_3DFACE
        api!(
            "user32",
            "FillRect",
            fn(usize, *const Rect, usize) -> i32,
            hdc,
            &cur,
            brush
        );
    }
    if flags & 0x2000 != 0 {
        *rect = cur;
    }
    1
}

unsafe fn line_ring(hdc: usize, r: Rect, light: u32, dark: u32, sides: u32) {
    let line = |color: u32, x0: i32, y0: i32, x1: i32, y1: i32| {
        let pen = api!(
            "gdi32",
            "CreatePen",
            fn(i32, i32, u32) -> usize,
            0,
            1,
            color
        )
        .unwrap_or(0);
        let old = api!("gdi32", "SelectObject", fn(usize, usize) -> usize, hdc, pen).unwrap_or(0);
        api!(
            "gdi32",
            "MoveToEx",
            fn(usize, i32, i32, *mut i32) -> i32,
            hdc,
            x0,
            y0,
            std::ptr::null_mut()
        );
        api!("gdi32", "LineTo", fn(usize, i32, i32) -> i32, hdc, x1, y1);
        api!("gdi32", "SelectObject", fn(usize, usize) -> usize, hdc, old);
        api!("gdi32", "DeleteObject", fn(usize) -> i32, pen);
    };
    if sides & 0x1 != 0 {
        line(light, r.left, r.bottom - 1, r.left, r.top - 1);
    }
    if sides & 0x2 != 0 {
        line(light, r.left, r.top, r.right, r.top);
    }
    if sides & 0x4 != 0 {
        line(dark, r.right - 1, r.top, r.right - 1, r.bottom);
    }
    if sides & 0x8 != 0 {
        line(dark, r.left, r.bottom - 1, r.right, r.bottom - 1);
    }
}

/// DrawFocusRect : rectangle pointillé gris autour de l'élément actif.
pub unsafe extern "win64" fn draw_focus_rect(hdc: usize, rect: *const Rect) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let r = *rect;
    let pen = api!(
        "gdi32",
        "CreatePen",
        fn(i32, i32, u32) -> usize,
        2,
        1,
        0x0050_5050
    )
    .unwrap_or(0); // PS_DOT
    let old = api!("gdi32", "SelectObject", fn(usize, usize) -> usize, hdc, pen).unwrap_or(0);
    for (x0, y0, x1, y1) in [
        (r.left, r.top, r.right - 1, r.top),
        (r.right - 1, r.top, r.right - 1, r.bottom - 1),
        (r.right - 1, r.bottom - 1, r.left, r.bottom - 1),
        (r.left, r.bottom - 1, r.left, r.top),
    ] {
        api!(
            "gdi32",
            "MoveToEx",
            fn(usize, i32, i32, *mut i32) -> i32,
            hdc,
            x0,
            y0,
            std::ptr::null_mut()
        );
        api!("gdi32", "LineTo", fn(usize, i32, i32) -> i32, hdc, x1, y1);
    }
    api!("gdi32", "SelectObject", fn(usize, usize) -> usize, hdc, old);
    api!("gdi32", "DeleteObject", fn(usize) -> i32, pen);
    1
}

/// DrawIconEx : les icônes chargées ne sont pas encore dessinées par Winlux.
pub extern "win64" fn draw_icon_ex(
    _hdc: usize,
    _x: i32,
    _y: i32,
    icon: usize,
    _cx: i32,
    _cy: i32,
    _step: u32,
    _brush: usize,
    _flags: u32,
) -> i32 {
    (icon != 0) as i32
}

pub extern "win64" fn draw_icon(_hdc: usize, _x: i32, _y: i32, icon: usize) -> i32 {
    (icon != 0) as i32
}

/// Largeur d'un texte dans la police du DC (GetTextExtentPoint32W).
unsafe fn extent_w(hdc: usize, text: &[u16]) -> (i32, i32) {
    let mut size = [0i32; 2];
    api!(
        "gdi32",
        "GetTextExtentPoint32W",
        fn(usize, *const u16, i32, *mut i32) -> i32,
        hdc,
        text.as_ptr(),
        text.len() as i32,
        size.as_mut_ptr()
    );
    (size[0], size[1])
}

/// TabbedTextOut : TextOut segment par segment, chaque tabulation allant au
/// taquet suivant (liste donnée, ou tous les 8 caractères moyens).
unsafe fn tabbed_text_out(
    hdc: usize,
    x: i32,
    y: i32,
    text: &[u16],
    tabs: &[i32],
    origin: i32,
) -> i32 {
    let mut tm = [0u8; 64];
    api!(
        "gdi32",
        "GetTextMetricsW",
        fn(usize, *mut u8) -> i32,
        hdc,
        tm.as_mut_ptr()
    );
    let avg = i32::from_le_bytes(tm[20..24].try_into().unwrap_or([0; 4])).max(1);
    let height = i32::from_le_bytes(tm[0..4].try_into().unwrap_or([0; 4]));
    let default_tab = match tabs {
        [] => avg * 8,
        [one] => (*one).max(1),
        _ => avg * 8,
    };
    let mut pen = x;
    for (i, seg) in text.split(|c| *c == 9).enumerate() {
        if i > 0 {
            let rel = pen - origin;
            let next = if tabs.len() > 1 {
                tabs.iter()
                    .map(|t| t.abs())
                    .find(|t| *t > rel)
                    .unwrap_or(((rel / default_tab) + 1) * default_tab)
            } else {
                ((rel / default_tab) + 1) * default_tab
            };
            pen = origin + next;
        }
        if !seg.is_empty() {
            api!(
                "gdi32",
                "TextOutW",
                fn(usize, i32, i32, *const u16, i32) -> i32,
                hdc,
                pen,
                y,
                seg.as_ptr(),
                seg.len() as i32
            );
            pen += extent_w(hdc, seg).0;
        }
    }
    let width = (pen - x).clamp(0, 0xFFFF);
    (height.clamp(0, 0xFFFF) << 16) | width
}

pub unsafe extern "win64" fn tabbed_text_out_w(
    hdc: usize,
    x: i32,
    y: i32,
    text: *const u16,
    count: i32,
    ntabs: i32,
    tabs: *const i32,
    origin: i32,
) -> i32 {
    if text.is_null() || count < 0 {
        return 0;
    }
    let t = std::slice::from_raw_parts(text, count as usize);
    let stops = if tabs.is_null() || ntabs <= 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(tabs, ntabs as usize)
    };
    tabbed_text_out(hdc, x, y, t, stops, origin)
}

pub unsafe extern "win64" fn tabbed_text_out_a(
    hdc: usize,
    x: i32,
    y: i32,
    text: *const u8,
    count: i32,
    ntabs: i32,
    tabs: *const i32,
    origin: i32,
) -> i32 {
    if text.is_null() || count < 0 {
        return 0;
    }
    let s = crate::dlls::decode_ansi(std::slice::from_raw_parts(text, count as usize));
    let w: Vec<u16> = s.encode_utf16().collect();
    let stops = if tabs.is_null() || ntabs <= 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(tabs, ntabs as usize)
    };
    tabbed_text_out(hdc, x, y, &w, stops, origin)
}

/// Attributs posés par SetLayeredWindowAttributes, relus par
/// GetLayeredWindowAttributes : (couleur clé, alpha, options).
fn layered_attributes() -> &'static Mutex<HashMap<usize, (u32, u8, u32)>> {
    static M: OnceLock<Mutex<HashMap<usize, (u32, u8, u32)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// SetLayeredWindowAttributes : la transparence par fenêtre n'est pas encore
/// appliquée ; la fenêtre reste opaque et visible. Les valeurs sont gardées
/// pour GetLayeredWindowAttributes.
pub extern "win64" fn set_layered_window_attributes(
    hwnd: usize,
    key: u32,
    alpha: u8,
    flags: u32,
) -> i32 {
    if hwnd == 0 {
        set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    layered_attributes()
        .lock()
        .unwrap()
        .insert(hwnd, (key, alpha, flags));
    1
}

/// GetLayeredWindowAttributes : rend les valeurs de SetLayeredWindowAttributes.
/// Chromium (Claude, Electron) l'importe en chargement différé et s'arrête
/// net si la fonction manque (#852). Comme sous Windows, l'appel échoue
/// pour une fenêtre dont les attributs n'ont jamais été posés.
pub unsafe extern "win64" fn get_layered_window_attributes(
    hwnd: usize,
    key: *mut u32,
    alpha: *mut u8,
    flags: *mut u32,
) -> i32 {
    let Some((k, a, f)) = layered_attributes().lock().unwrap().get(&hwnd).copied() else {
        set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    };
    if !key.is_null() {
        *key = k;
    }
    if !alpha.is_null() {
        *alpha = a;
    }
    if !flags.is_null() {
        *flags = f;
    }
    1
}

const ULW_COLORKEY: u32 = 1;
const ULW_ALPHA: u32 = 2;
const AC_SRC_ALPHA: u8 = 1;
/// Au-delà, la forme coûterait plus cher que le rendu : fenêtre rectangulaire.
const MAX_SHAPE_RECTS: usize = 16_384;

/// Forme d'une image de fenêtre en couches : bandes horizontales des pixels
/// visibles (alpha d'au moins 50 %, ou couleur différente de la clé), lignes
/// identiques fusionnées. `None` : tout est visible, ou trop de morceaux.
/// `pixels` : COLORREF avec l'alpha dans l'octet haut, ligne du haut d'abord.
pub(crate) fn layered_shape(
    width: usize,
    height: usize,
    pixels: &[u32],
    alpha: bool,
    key: Option<u32>,
) -> Option<Vec<super::gui::Rect>> {
    use super::gui::Rect;
    let visible = |c: u32| {
        (!alpha || c >> 24 >= 0x80) && key.is_none_or(|k| c & 0x00FF_FFFF != k & 0x00FF_FFFF)
    };
    let mut rects: Vec<Rect> = Vec::new();
    let mut previous: Vec<(i32, i32)> = Vec::new();
    let mut band_start = 0usize; // index dans `rects` des bandes de la ligne précédente
    let mut all_visible = true;
    for y in 0..height {
        let row = pixels.get(y * width..(y + 1) * width)?;
        let mut runs = Vec::new();
        let mut x = 0;
        while x < width {
            if !visible(row[x]) {
                all_visible = false;
                x += 1;
                continue;
            }
            let start = x;
            while x < width && visible(row[x]) {
                x += 1;
            }
            runs.push((start as i32, x as i32));
        }
        if y > 0 && runs == previous {
            for r in &mut rects[band_start..] {
                r.bottom += 1;
            }
            continue;
        }
        band_start = rects.len();
        rects.extend(runs.iter().map(|&(l, r)| Rect {
            left: l,
            top: y as i32,
            right: r,
            bottom: y as i32 + 1,
        }));
        if rects.len() > MAX_SHAPE_RECTS {
            return None;
        }
        previous = runs;
    }
    (!all_visible).then_some(rects)
}

/// UpdateLayeredWindow : place la fenêtre (pptDst, psize) et y copie le
/// contenu de hdcSrc à partir de pptSrc. Le serveur X ne compose pas l'alpha
/// d'une fenêtre ordinaire : avec ULW_ALPHA (et l'alpha par pixel) ou
/// ULW_COLORKEY, les pixels transparents sont découpés par une forme X
/// (SHAPE) au lieu d'apparaître en noir (fond de Flow Launcher, #1220).
pub unsafe extern "win64" fn update_layered_window(
    hwnd: usize,
    _hdc_dst: usize,
    pt_dst: *const [i32; 2],
    size: *const [i32; 2],
    hdc_src: usize,
    pt_src: *const [i32; 2],
    key: u32,
    blend: *const u32,
    flags: u32,
) -> i32 {
    let Some(rect) = super::gui::window_rect(hwnd) else {
        set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    };
    let (x, y) = pt_dst
        .as_ref()
        .map_or((rect.left, rect.top), |p| (p[0], p[1]));
    let (w, h) = size
        .as_ref()
        .map_or((rect.width(), rect.height()), |s| (s[0], s[1]));
    const SWP_NOZORDER: u32 = 0x4;
    const SWP_NOACTIVATE: u32 = 0x10;
    if (x, y, w, h) != (rect.left, rect.top, rect.width(), rect.height()) {
        super::user32::set_window_pos(hwnd, 0, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    if hdc_src != 0 {
        let (sx, sy) = pt_src.as_ref().map_or((0, 0), |p| (p[0], p[1]));
        let dc = super::user32::get_dc(hwnd);
        super::gdi32::bit_blt(dc, 0, 0, w, h, hdc_src, sx, sy, 0x00CC_0020);
        super::user32::release_dc(hwnd, dc);
        // BLENDFUNCTION : BlendOp, BlendFlags, SourceConstantAlpha, AlphaFormat.
        let per_pixel = flags & ULW_ALPHA != 0
            && !blend.is_null()
            && (std::ptr::read_unaligned(blend) >> 24) as u8 & AC_SRC_ALPHA != 0;
        let key = (flags & ULW_COLORKEY != 0).then_some(key);
        if per_pixel || key.is_some() {
            let bitmap = super::gui::dc_bitmap(hdc_src);
            if let Some((bw, bh, pixels, has_alpha)) = super::gui::bitmap_pixels(bitmap) {
                let (cw, ch) = (w.max(0) as usize, h.max(0) as usize);
                let mut area = vec![0u32; cw * ch];
                for y in 0..ch {
                    for x in 0..cw {
                        let (px, py) = (sx as i64 + x as i64, sy as i64 + y as i64);
                        if px >= 0 && py >= 0 && (px as usize) < bw && (py as usize) < bh {
                            area[y * cw + x] = pixels[py as usize * bw + px as usize];
                        }
                    }
                }
                let shape = layered_shape(cw, ch, &area, per_pixel && has_alpha, key);
                super::gui::with_backend_public(|b| b.set_shape(hwnd, shape.as_deref()));
            }
        }
    }
    1
}

/// UpdateLayeredWindowIndirect : mêmes champs dans UPDATELAYEREDWINDOWINFO.
pub unsafe extern "win64" fn update_layered_window_indirect(hwnd: usize, info: *const u8) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let at = |o: usize| std::ptr::read_unaligned(info.add(o) as *const usize);
    update_layered_window(
        hwnd,
        at(8),
        at(16) as *const [i32; 2],
        at(24) as *const [i32; 2],
        at(32),
        at(40) as *const [i32; 2],
        std::ptr::read_unaligned(info.add(48) as *const u32),
        at(56) as *const u32,
        std::ptr::read_unaligned(info.add(64) as *const u32),
    )
}

// ── gdi32 ───────────────────────────────────────────────────────────────

/// GetTextExtentExPoint : largeurs cumulées caractère par caractère et
/// nombre de caractères qui tiennent dans `max_extent`.
unsafe fn text_extent_ex(
    hdc: usize,
    text: &[u16],
    max_extent: i32,
    fit: *mut i32,
    dx: *mut i32,
    size: *mut i32,
) -> i32 {
    let mut total = 0;
    let mut fits = 0;
    for (i, c) in text.iter().enumerate() {
        total += extent_w(hdc, std::slice::from_ref(c)).0;
        if !dx.is_null() {
            *dx.add(i) = total;
        }
        if total <= max_extent {
            fits = i as i32 + 1;
        }
    }
    let (w, h) = extent_w(hdc, text);
    if !size.is_null() {
        *size = w.max(total);
        *size.add(1) = h;
    }
    if !fit.is_null() {
        *fit = fits;
    }
    1
}

pub unsafe extern "win64" fn get_text_extent_ex_point_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    max_extent: i32,
    fit: *mut i32,
    dx: *mut i32,
    size: *mut i32,
) -> i32 {
    if text.is_null() || count < 0 {
        return 0;
    }
    text_extent_ex(
        hdc,
        std::slice::from_raw_parts(text, count as usize),
        max_extent,
        fit,
        dx,
        size,
    )
}

pub unsafe extern "win64" fn get_text_extent_ex_point_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    max_extent: i32,
    fit: *mut i32,
    dx: *mut i32,
    size: *mut i32,
) -> i32 {
    if text.is_null() || count < 0 {
        return 0;
    }
    let bytes = std::slice::from_raw_parts(text, count as usize);
    // Une case de `dx` par octet ANSI : la page de code occidentale a un
    // caractère par octet.
    let w: Vec<u16> = bytes
        .iter()
        .map(|b| {
            crate::dlls::decode_ansi(&[*b])
                .encode_utf16()
                .next()
                .unwrap_or(*b as u16)
        })
        .collect();
    text_extent_ex(hdc, &w, max_extent, fit, dx, size)
}

/// GetCharABCWidthsFloat : les largeurs ABC entières converties en flottants.
unsafe fn abc_float(hdc: usize, first: u32, last: u32, out: *mut f32) -> i32 {
    if out.is_null() || last < first {
        return 0;
    }
    let n = (last - first + 1) as usize;
    let mut abc = vec![0i32; n * 3];
    let ok = api!(
        "gdi32",
        "GetCharABCWidthsW",
        fn(usize, u32, u32, *mut i32) -> i32,
        hdc,
        first,
        last,
        abc.as_mut_ptr()
    )
    .unwrap_or(0);
    if ok == 0 {
        return 0;
    }
    for (i, v) in abc.iter().enumerate() {
        // ABC = (int A, UINT B, int C)
        *out.add(i) = if i % 3 == 1 {
            *v as u32 as f32
        } else {
            *v as f32
        };
    }
    1
}

pub unsafe extern "win64" fn get_char_abc_widths_float_w(
    hdc: usize,
    first: u32,
    last: u32,
    out: *mut f32,
) -> i32 {
    abc_float(hdc, first, last, out)
}
pub unsafe extern "win64" fn get_char_abc_widths_float_a(
    hdc: usize,
    first: u32,
    last: u32,
    out: *mut f32,
) -> i32 {
    abc_float(hdc, first, last, out)
}

/// GetCharacterPlacementW : placement simple de gauche à droite (pas de
/// réordonnancement bidirectionnel) ; largeurs, ordre et glyphes = caractères.
pub unsafe extern "win64" fn get_character_placement_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    _max_extent: i32,
    results: *mut u8,
    _flags: u32,
) -> u32 {
    if text.is_null() || count <= 0 {
        return 0;
    }
    let t = std::slice::from_raw_parts(text, count as usize);
    let (w, h) = extent_w(hdc, t);
    if !results.is_null() {
        let rd = |off: usize| std::ptr::read_unaligned(results.add(off) as *const usize);
        let out_string = rd(8) as *mut u16;
        let order = rd(16) as *mut u32;
        let dx = rd(24) as *mut i32;
        let caret = rd(32) as *mut i32;
        let glyphs = rd(48) as *mut u16;
        let max = (std::ptr::read_unaligned(results.add(56) as *const u32) as usize).min(t.len());
        let mut pos = 0;
        for (i, c) in t.iter().take(max).enumerate() {
            let cw = extent_w(hdc, std::slice::from_ref(c)).0;
            if !out_string.is_null() {
                *out_string.add(i) = *c;
            }
            if !order.is_null() {
                *order.add(i) = i as u32;
            }
            if !dx.is_null() {
                *dx.add(i) = cw;
            }
            if !caret.is_null() {
                *caret.add(i) = pos;
            }
            if !glyphs.is_null() {
                *glyphs.add(i) = *c;
            }
            pos += cw;
        }
        std::ptr::write_unaligned(results.add(56) as *mut u32, max as u32);
        std::ptr::write_unaligned(results.add(60) as *mut i32, max as i32);
    }
    ((h.clamp(0, 0xFFFF) as u32) << 16) | (w.clamp(0, 0xFFFF) as u32)
}

/// GetOutlineTextMetrics : Winlux ne lit pas encore les tables des polices
/// vectorielles ; « échec » fait prendre aux logiciels leur repli TEXTMETRIC.
pub extern "win64" fn get_outline_text_metrics(_hdc: usize, _size: u32, _out: *mut u8) -> u32 {
    0
}

/// Jeux de caractères Windows ↔ pages de code ↔ bit de FONTSIGNATURE.fsCsb[0].
const CHARSETS: [(u32, u32, u32); 15] = [
    (0, 1252, 0),
    (238, 1250, 1),
    (204, 1251, 2),
    (161, 1253, 3),
    (162, 1254, 4),
    (177, 1255, 5),
    (178, 1256, 6),
    (186, 1257, 7),
    (163, 1258, 8),
    (222, 874, 16),
    (128, 932, 17),
    (134, 936, 18),
    (129, 949, 19),
    (136, 950, 20),
    (2, 42, 31),
];

/// TranslateCharsetInfo : TCI_SRCCHARSET (1), TCI_SRCCODEPAGE (2) ou
/// TCI_SRCFONTSIG (3) → CHARSETINFO { ciCharset, ciACP, FONTSIGNATURE }.
pub unsafe extern "win64" fn translate_charset_info(
    src: *const u32,
    out: *mut u32,
    flags: u32,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    let key = src as usize as u32;
    let found = match flags {
        1 => CHARSETS.iter().find(|c| c.0 == key),
        2 => CHARSETS.iter().find(|c| c.1 == key),
        3 if !src.is_null() => {
            let csb = *src.add(4);
            CHARSETS.iter().find(|c| csb & (1 << c.2) != 0)
        }
        _ => None,
    };
    let Some(&(charset, acp, bit)) = found else {
        return 0;
    };
    std::ptr::write_bytes(out, 0, 8);
    *out = charset;
    *out.add(1) = acp;
    *out.add(6) = 1 << bit;
    1
}

/// Palettes : l'affichage est en couleurs vraies, elles n'ont pas d'effet.
pub extern "win64" fn set_palette_entries(
    _pal: usize,
    _start: u32,
    count: u32,
    _entries: *const u8,
) -> u32 {
    count
}
pub extern "win64" fn unrealize_object(_obj: usize) -> i32 {
    1
}
pub extern "win64" fn update_colors(_hdc: usize) -> i32 {
    1
}
/// GetPath : aucun tracé n'est en cours (BeginPath n'est pas encore fourni).
pub extern "win64" fn get_path(_hdc: usize, _points: *mut u8, _types: *mut u8, _count: i32) -> i32 {
    set_last_error(1); // ERROR_CAN_NOT_COMPLETE
    -1
}

// ── shell32 ─────────────────────────────────────────────────────────────

/// SetCurrentProcessExplicitAppUserModelID : identifiant de regroupement
/// des fenêtres dans la barre des tâches, conservé pour le processus.
pub unsafe extern "win64" fn set_current_process_explicit_app_user_model_id(id: *const u16) -> i32 {
    if id.is_null() {
        return 0x8007_0057u32 as i32; // E_INVALIDARG
    }
    *APP_ID.lock().unwrap_or_else(|e| e.into_inner()) = Some(wstr(id));
    0
}

static APP_ID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub unsafe extern "win64" fn get_current_process_explicit_app_user_model_id(
    out: *mut *mut u16,
) -> i32 {
    let Some(id) = APP_ID.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return 0x8000_4005u32 as i32; // E_FAIL : aucun identifiant posé
    };
    if out.is_null() {
        return 0x8007_0057u32 as i32;
    }
    let w: Vec<u16> = id.encode_utf16().chain([0]).collect();
    let p = api!(
        "ole32",
        "CoTaskMemAlloc",
        fn(usize) -> *mut u16,
        w.len() * 2
    )
    .unwrap_or(std::ptr::null_mut());
    if p.is_null() {
        return 0x8007_000Eu32 as i32; // E_OUTOFMEMORY
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    0
}

// ── winmm ───────────────────────────────────────────────────────────────

/// timeBeginPeriod / timeEndPeriod : les minuteurs Linux ont déjà la
/// milliseconde ; la demande est acceptée (TIMERR_NOERROR).
pub extern "win64" fn time_begin_period(period: u32) -> u32 {
    if period == 0 {
        97 // TIMERR_NOCANDO
    } else {
        0
    }
}
pub extern "win64" fn time_end_period(period: u32) -> u32 {
    time_begin_period(period)
}

// ── Runtime Visual C++ (vcruntime140 → ucrtbase) ───────────────────────

/// __std_type_info_destroy_list : libère les noms décorés mis en cache par
/// type_info::name. Winlux ne les met pas en cache : rien à libérer.
pub extern "win64" fn std_type_info_destroy_list(_head: *mut u8) {}

/// __std_exception_copy(from, to) : `{ const char* what; bool do_free; }`.
pub unsafe extern "win64" fn std_exception_copy(from: *const usize, to: *mut usize) {
    if from.is_null() || to.is_null() {
        return;
    }
    let what = *from as *const u8;
    let do_free = *(from.add(1) as *const u8) != 0;
    if do_free && !what.is_null() {
        let n = libc::strlen(what as *const libc::c_char) + 1;
        let copy = libc::malloc(n) as *mut u8;
        if !copy.is_null() {
            std::ptr::copy_nonoverlapping(what, copy, n);
            *to = copy as usize;
            *(to.add(1) as *mut u8) = 1;
            return;
        }
    }
    *to = what as usize;
    *(to.add(1) as *mut u8) = 0;
}

pub unsafe extern "win64" fn std_exception_destroy(data: *mut usize) {
    if data.is_null() {
        return;
    }
    if *(data.add(1) as *const u8) != 0 {
        libc::free(*data as *mut libc::c_void);
    }
    *data = 0;
    *(data.add(1) as *mut u8) = 0;
}

/// _purecall : appel d'une méthode virtuelle pure (objet C++ à moitié
/// détruit). Windows arrête le programme ; Winlux aussi, avec un message.
pub extern "win64" fn purecall() -> i32 {
    eprintln!("winlux: appel de fonction virtuelle pure (_purecall) : arrêt du programme");
    std::process::abort();
}

// ── ws2_32 et comctl32 ──────────────────────────────────────────────────

/// WSAAddressToStringA : version ANSI de WSAAddressToStringW.
pub unsafe extern "win64" fn wsa_address_to_string_a(
    addr: *const u8,
    len: u32,
    info: *const u8,
    out: *mut u8,
    out_len: *mut u32,
) -> i32 {
    if out_len.is_null() {
        set_last_error(10014); // WSAEFAULT
        return -1;
    }
    let mut w = vec![0u16; 128];
    let mut n = w.len() as u32;
    let r = api!(
        "ws2_32",
        "WSAAddressToStringW",
        fn(*const u8, u32, *const u8, *mut u16, *mut u32) -> i32,
        addr,
        len,
        info,
        w.as_mut_ptr(),
        &mut n
    )
    .unwrap_or(-1);
    if r != 0 {
        return r;
    }
    let text = String::from_utf16_lossy(&w[..(n as usize).saturating_sub(1).min(w.len())]);
    let text = text.trim_end_matches('\0');
    if out.is_null() || (*out_len as usize) <= text.len() {
        *out_len = text.len() as u32 + 1;
        set_last_error(10014);
        return -1;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
    *out.add(text.len()) = 0;
    *out_len = text.len() as u32 + 1;
    0
}

/// Listes réordonnables par glisser (MakeDragList) : les zones de liste ne
/// sont pas encore des contrôles Winlux ; le glisser n'est pas proposé
/// (LBItemFromPt : aucun élément sous le pointeur).
pub extern "win64" fn make_drag_list(_listbox: usize) -> i32 {
    0
}
pub extern "win64" fn lb_item_from_pt(_listbox: usize, _pt: u64, _auto_scroll: i32) -> i32 {
    -1
}
pub extern "win64" fn draw_insert(_parent: usize, _listbox: usize, _item: i32) {}

// ── Tables d'exports ────────────────────────────────────────────────────

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "MulDiv" => fn mul_div,
        "Beep" => fn beep,
        "GlobalMemoryStatus" => fn global_memory_status,
        "OpenFileMappingA" => fn open_file_mapping,
        "OpenFileMappingW" => fn open_file_mapping,
        "WaitNamedPipeA" => fn wait_named_pipe_a,
        "AddDllDirectory" => fn add_dll_directory,
        "RemoveDllDirectory" => fn remove_dll_directory,
        "SetDefaultDllDirectories" => fn set_default_dll_directories,
        "SetDllDirectoryA" => fn set_dll_directory_a,
        "SetDllDirectoryW" => fn set_dll_directory_w,
        "GetLogicalDriveStringsA" => fn get_logical_drive_strings_a,
        "GetLogicalDriveStringsW" => fn get_logical_drive_strings_w,
        "GetVolumePathNameW" => fn get_volume_path_name_w,
        "GetVolumeNameForVolumeMountPointW" => fn get_volume_name_for_volume_mount_point_w,
        "GetVolumeNameForVolumeMountPointA" => fn get_volume_name_for_volume_mount_point_a,
        "QueryFullProcessImageNameA" => fn query_full_process_image_name_a,
        "QueryFullProcessImageNameW" => fn query_full_process_image_name_w,
        "GetCommState" => fn get_comm_state,
        "SetCommState" => fn set_comm_state,
        "GetCommTimeouts" => fn get_comm_timeouts,
        "SetCommTimeouts" => fn set_comm_timeouts,
        "SetCommBreak" => fn set_comm_break,
        "ClearCommBreak" => fn clear_comm_break,
        "SetupComm" => fn setup_comm,
        "PurgeComm" => fn purge_comm,
        "EscapeCommFunction" => fn escape_comm_function,
        "GetCommModemStatus" => fn get_comm_modem_status,
        "ClearCommError" => fn clear_comm_error,
    }
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "PtInRect" => fn pt_in_rect,
        "OffsetRect" => fn offset_rect,
        "GetQueueStatus" => fn get_queue_status,
        "SetKeyboardState" => fn set_keyboard_state,
        "FlashWindow" => fn flash_window,
        "FlashWindowEx" => fn flash_window_ex,
        "ToAscii" => fn to_ascii,
        "ToAsciiEx" => fn to_ascii_ex,
        "GetMenuDefaultItem" => fn get_menu_default_item,
        "SetMenuDefaultItem" => fn set_menu_default_item,
        "SetClassLongA" => fn set_class_long_a,
        "SetClassLongW" => fn set_class_long_w,
        "MessageBoxIndirectA" => fn message_box_indirect_a,
        "MessageBoxIndirectW" => fn message_box_indirect_w,
        "CheckRadioButton" => fn check_radio_button,
        "GetDlgItemInt" => fn get_dlg_item_int,
        "SetDlgItemInt" => fn set_dlg_item_int,
        "GetDialogBaseUnits" => fn get_dialog_base_units,
        "MapDialogRect" => fn map_dialog_rect,
        "DefDlgProcA" => fn def_dlg_proc_a,
        "DefDlgProcW" => fn def_dlg_proc_w,
        "GetDpiForWindow" => fn get_dpi_for_window,
        "GetDpiForSystem" => fn get_dpi_for_system,
        "GetSystemMetricsForDpi" => fn get_system_metrics_for_dpi,
        "AdjustWindowRectExForDpi" => fn adjust_window_rect_ex_for_dpi,
        "SetProcessDPIAware" => fn set_process_dpi_aware,
        "EnumDisplaySettingsA" => fn enum_display_settings_a,
        "EnumDisplaySettingsW" => fn enum_display_settings_w,
        "EnumDisplaySettingsExW" => fn enum_display_settings_ex_w,
        "EnumDisplayDevicesW" => fn enum_display_devices_w,
        "DrawEdge" => fn draw_edge,
        "DrawFocusRect" => fn draw_focus_rect,
        "DrawIcon" => fn draw_icon,
        "DrawIconEx" => fn draw_icon_ex,
        "TabbedTextOutA" => fn tabbed_text_out_a,
        "TabbedTextOutW" => fn tabbed_text_out_w,
        "SetLayeredWindowAttributes" => fn set_layered_window_attributes,
        "GetLayeredWindowAttributes" => fn get_layered_window_attributes,
        "UpdateLayeredWindow" => fn update_layered_window,
        "UpdateLayeredWindowIndirect" => fn update_layered_window_indirect,
    }
}

pub fn gdi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetTextExtentExPointA" => fn get_text_extent_ex_point_a,
        "GetTextExtentExPointW" => fn get_text_extent_ex_point_w,
        "GetCharABCWidthsFloatA" => fn get_char_abc_widths_float_a,
        "GetCharABCWidthsFloatW" => fn get_char_abc_widths_float_w,
        "GetCharacterPlacementW" => fn get_character_placement_w,
        "GetOutlineTextMetricsA" => fn get_outline_text_metrics,
        "GetOutlineTextMetricsW" => fn get_outline_text_metrics,
        "TranslateCharsetInfo" => fn translate_charset_info,
        "SetPaletteEntries" => fn set_palette_entries,
        "UnrealizeObject" => fn unrealize_object,
        "UpdateColors" => fn update_colors,
        "GetPath" => fn get_path,
    }
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetCurrentProcessExplicitAppUserModelID" => fn set_current_process_explicit_app_user_model_id,
        "GetCurrentProcessExplicitAppUserModelID" => fn get_current_process_explicit_app_user_model_id,
    }
}

pub fn winmm_exports() -> crate::dlls::Exports {
    crate::exports! {
        "timeBeginPeriod" => fn time_begin_period,
        "timeEndPeriod" => fn time_end_period,
    }
}

pub fn ws2_32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WSAAddressToStringA" => fn wsa_address_to_string_a,
    }
}

pub fn comctl32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "MakeDragList" => fn make_drag_list,
        "LBItemFromPt" => fn lb_item_from_pt,
        "DrawInsert" => fn draw_insert,
    }
}

pub fn crt_exports() -> crate::dlls::Exports {
    crate::exports! {
        "__std_type_info_destroy_list" => fn std_type_info_destroy_list,
        "__std_exception_copy" => fn std_exception_copy,
        "__std_exception_destroy" => fn std_exception_destroy,
        "_purecall" => fn purecall,
    }
}

#[cfg(test)]
mod tests {
    /// Discord (#2041) : chemin du programme d'un autre processus du même
    /// compte ; celui d'un autre compte reste refusé.
    #[test]
    fn chemin_d_un_autre_processus() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        // Juste après spawn(), l'enfant peut ne pas avoir fait son exec :
        // /proc/<pid>/exe désigne encore le binaire de test (#2109). On
        // attend, au plus 10 s, qu'il soit devenu « sleep ».
        let me = std::fs::read_link("/proc/self/exe").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let exe = loop {
            let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap();
            if exe != me {
                break exe;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "sleep n'a pas été exécuté en 10 s"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let got = super::foreign_image_path(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(got, Ok(crate::paths::to_windows(&exe)));
        if unsafe { libc::getuid() } != 0 {
            assert_eq!(super::foreign_image_path(1), Err(5));
        }
    }

    use super::*;

    /// #1220 : fond transparent d'une fenêtre WPF en couches (Flow Launcher)
    /// découpé, coins arrondis compris ; image opaque : pas de forme.
    #[test]
    fn forme_d_une_fenetre_en_couches() {
        let (o, t) = (0xFF20_2020u32, 0x0000_0000u32);
        // 4x3 : coins transparents sur la première ligne.
        #[rustfmt::skip]
        let px = [
            t, o, o, t,
            o, o, o, o,
            o, o, o, o,
        ];
        let r = layered_shape(4, 3, &px, true, None).expect("forme");
        let rr: Vec<_> = r
            .iter()
            .map(|r| (r.left, r.top, r.right, r.bottom))
            .collect();
        assert_eq!(rr, vec![(1, 0, 3, 1), (0, 1, 4, 3)]);
        assert!(layered_shape(4, 3, &[o; 12], true, None).is_none());
        // Ombre à 25 % d'opacité : découpée ; sans alpha par pixel : gardée.
        let ombre = 0x4000_0000u32;
        assert_eq!(
            layered_shape(2, 1, &[ombre, o], true, None).unwrap().len(),
            1
        );
        assert!(layered_shape(2, 1, &[ombre, o], false, None).is_none());
        // ULW_COLORKEY : la couleur clé (magenta) est découpée.
        let r = layered_shape(
            3,
            1,
            &[0xFF_00FF, 0x12_3456, 0xFF_00FF],
            false,
            Some(0xFF_00FF),
        );
        let rr: Vec<_> = r.unwrap().iter().map(|r| (r.left, r.right)).collect();
        assert_eq!(rr, vec![(1, 2)]);
    }

    #[test]
    fn attributs_de_fenetre_superposee_relus() {
        let hwnd = 0x7e57_1a7e;
        let (mut k, mut a, mut f) = (0u32, 0u8, 0u32);
        unsafe {
            assert_eq!(
                get_layered_window_attributes(hwnd, &mut k, &mut a, &mut f),
                0
            );
            assert_eq!(set_layered_window_attributes(hwnd, 0x00ff00, 200, 3), 1);
            assert_eq!(
                get_layered_window_attributes(hwnd, &mut k, &mut a, &mut f),
                1
            );
            assert_eq!((k, a, f), (0x00ff00, 200, 3));
            let p = std::ptr::null_mut();
            assert_eq!(get_layered_window_attributes(hwnd, p, &mut a, p), 1);
        }
        assert!(lookup("user32", "GetLayeredWindowAttributes").is_some());
    }

    #[test]
    fn nom_de_volume_d_une_racine_de_lecteur() {
        let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut out = [0u16; 50];
        let ok = unsafe {
            get_volume_name_for_volume_mount_point_w(w("C:\\").as_ptr(), out.as_mut_ptr(), 50)
        };
        assert_eq!(ok, 1);
        let n = out.iter().position(|&c| c == 0).unwrap();
        let name = String::from_utf16_lossy(&out[..n]);
        assert!(name.starts_with("\\\\?\\Volume{5749"), "{name}");
        assert!(name.ends_with("}\\") && name.len() == 49, "{name}");
        // Stable, et rendu tel quel quand on le repasse.
        assert_eq!(volume_name_for_root("C:\\"), Ok(name.clone()));
        assert_eq!(volume_name_for_root(&name), Ok(name.clone()));
        // Barre oblique finale exigée ; un dossier n'est pas un point de montage.
        assert_eq!(volume_name_for_root("C:"), Err(123));
        let dir = crate::paths::to_linux("C:\\volume_1112");
        let _ = std::fs::create_dir_all(&dir);
        assert_eq!(volume_name_for_root("C:\\volume_1112\\"), Err(4390));
        let _ = std::fs::remove_dir(&dir);
        assert_eq!(volume_name_for_root("C:\\absent_1112\\"), Err(2));
        // Tampon trop court.
        let r = unsafe {
            get_volume_name_for_volume_mount_point_w(w("C:\\").as_ptr(), out.as_mut_ptr(), 20)
        };
        assert_eq!(r, 0);
        let mut a = [0u8; 50];
        let r = unsafe {
            get_volume_name_for_volume_mount_point_a(c"C:\\".as_ptr().cast(), a.as_mut_ptr(), 50)
        };
        assert_eq!(r, 1);
        assert_eq!(&a[..49], name.as_bytes());
    }

    #[test]
    fn mul_div_rounds_like_windows() {
        assert_eq!(mul_div(10, 3, 4), 8); // 7,5 → 8
        assert_eq!(mul_div(-10, 3, 4), -8);
        assert_eq!(mul_div(10, -3, 4), -8);
        assert_eq!(mul_div(9, 96, 72), 12);
        assert_eq!(mul_div(1, 1, 0), -1);
        assert_eq!(mul_div(i32::MAX, 2, 1), -1);
        assert_eq!(mul_div(7, 1, 2), 4);
        assert_eq!(mul_div(5, 1, 3), 2);
    }

    #[test]
    fn rect_helpers() {
        let mut r = Rect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 5,
        };
        let inside = (3u64 << 32) | 2;
        let edge = (5u64 << 32) | 10;
        unsafe {
            assert_eq!(pt_in_rect(&r, inside), 1);
            assert_eq!(pt_in_rect(&r, edge), 0);
            offset_rect(&mut r, 2, -1);
        }
        assert_eq!(
            r,
            Rect {
                left: 2,
                top: -1,
                right: 12,
                bottom: 4
            }
        );
    }

    #[test]
    fn charset_round_trip() {
        let mut out = [0u32; 8];
        unsafe {
            assert_eq!(
                translate_charset_info(204 as *const u32, out.as_mut_ptr(), 1),
                1
            );
        }
        assert_eq!(out[0..2], [204, 1251]);
        assert_eq!(out[6], 1 << 2);
        unsafe {
            assert_eq!(
                translate_charset_info(932 as *const u32, out.as_mut_ptr(), 2),
                1
            );
        }
        assert_eq!(out[0], 128);
        let sig = [0u32, 0, 0, 0, 1 << 1, 0];
        unsafe {
            assert_eq!(translate_charset_info(sig.as_ptr(), out.as_mut_ptr(), 3), 1);
        }
        assert_eq!(out[1], 1250);
    }

    #[test]
    fn exception_copy_duplicates_owned_text() {
        let text = unsafe { libc::strdup(c"erreur".as_ptr()) } as usize;
        let from = [text, 1usize];
        let mut to = [0usize; 2];
        unsafe { std_exception_copy(from.as_ptr(), to.as_mut_ptr()) };
        assert_ne!(to[0], text);
        assert_eq!(to[1] & 0xFF, 1);
        unsafe {
            std_exception_destroy(to.as_mut_ptr());
            libc::free(text as *mut libc::c_void);
        }
        assert_eq!(to[0], 0);
    }

    #[test]
    fn com_ports_map_to_linux_serial_devices() {
        assert_eq!(
            crate::paths::to_linux("COM1").to_string_lossy(),
            "/dev/ttyS0"
        );
        assert_eq!(
            crate::paths::to_linux("\\\\.\\COM4").to_string_lossy(),
            "/dev/ttyS3"
        );
    }

    #[test]
    fn update_layered_window_refuse_une_fenetre_inconnue() {
        let r = unsafe {
            update_layered_window(
                0xDEAD_0000,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(r, 0);
        assert_eq!(
            unsafe { update_layered_window_indirect(0xDEAD_0000, std::ptr::null()) },
            0
        );
    }
}
