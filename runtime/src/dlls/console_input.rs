//! Entrée de la console Windows sur le terminal Linux.
//!
//! Les programmes en mode texte plein écran (Claude Code, Node, Bun…)
//! passent la console en mode brut avec SetConsoleMode puis lisent les
//! touches avec ReadConsoleInputW. Ici la console est le terminal Linux :
//! SetConsoleMode règle termios (mode canonique, écho, Ctrl+C), et chaque
//! caractère reçu devient un enregistrement KEY_EVENT dont le caractère
//! est celui que le terminal a envoyé. Les séquences d'échappement (flèches,
//! touches de fonction) passent donc telles quelles, ce que libuv relaie
//! sans les modifier. Un changement de taille du terminal (SIGWINCH)
//! devient un WINDOW_BUFFER_SIZE_EVENT, que libuv transforme en
//! événement « resize ».

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const ENABLE_PROCESSED_INPUT: u32 = 0x1;
const ENABLE_LINE_INPUT: u32 = 0x2;
const ENABLE_ECHO_INPUT: u32 = 0x4;
const KEY_EVENT: u16 = 1;
const WINDOW_BUFFER_SIZE_EVENT: u16 = 4;
const RECORD_SIZE: usize = 20;

#[derive(Clone, Copy)]
enum Record {
    Key(u16),
    Resize(u16, u16),
}

struct InputState {
    queue: VecDeque<Record>,
    /// Octets UTF-8 incomplets en attente de la suite.
    pending: Vec<u8>,
}

static STATE: Mutex<InputState> = Mutex::new(InputState {
    queue: VecDeque::new(),
    pending: Vec::new(),
});
static RESIZED: AtomicBool = AtomicBool::new(false);
static ORIGINAL: Mutex<Option<libc::termios>> = Mutex::new(None);

fn is_input(h: usize) -> bool {
    super::kernel32::canon_std(h) == (-10isize) as usize && unsafe { libc::isatty(0) } == 1
}

extern "C" fn on_winch(_: libc::c_int) {
    RESIZED.store(true, Ordering::Release);
}

extern "C" fn restore_at_exit() {
    restore();
}

/// Remet le terminal tel qu'on l'a trouvé (fin du programme).
pub(crate) fn restore() {
    if let Some(t) = *crate::dlls::lock(&ORIGINAL) {
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &t);
        }
    }
}

/// SetConsoleMode sur l'entrée : règle le terminal Linux en conséquence.
pub(crate) fn apply_input_mode(mode: u32) {
    if unsafe { libc::isatty(0) } != 1 {
        return;
    }
    let mut original = crate::dlls::lock(&ORIGINAL);
    let base = match *original {
        Some(t) => t,
        None => {
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(0, &mut t) } != 0 {
                return;
            }
            *original = Some(t);
            unsafe {
                libc::atexit(restore_at_exit);
                libc::signal(libc::SIGWINCH, on_winch as *const () as libc::sighandler_t);
            }
            t
        }
    };
    drop(original);
    let mut t = base;
    if mode & ENABLE_LINE_INPUT == 0 {
        t.c_lflag &= !libc::ICANON;
        t.c_iflag &= !(libc::ICRNL | libc::INLCR | libc::IXON);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
    }
    if mode & ENABLE_ECHO_INPUT == 0 {
        t.c_lflag &= !(libc::ECHO | libc::ECHONL);
    }
    if mode & ENABLE_PROCESSED_INPUT == 0 {
        // Ctrl+C arrive comme caractère 0x03 au lieu d'un signal.
        t.c_lflag &= !(libc::ISIG | libc::IEXTEN);
    }
    unsafe {
        libc::tcsetattr(0, libc::TCSANOW, &t);
    }
}

fn terminal_size() -> (u16, u16) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    for fd in [1, 2, 0] {
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_col > 0 {
            return (ws.ws_col, ws.ws_row.max(1));
        }
    }
    (80, 25)
}

/// Lit ce que le terminal a déjà envoyé, sans attendre.
fn fill(state: &mut InputState) {
    if RESIZED.swap(false, Ordering::AcqRel) {
        let (c, r) = terminal_size();
        state.queue.push_back(Record::Resize(c, r));
    }
    loop {
        let mut p = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut p, 1, 0) } <= 0 || p.revents & libc::POLLIN == 0 {
            break;
        }
        let mut buf = [0u8; 4096];
        let n = unsafe { libc::read(0, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            break;
        }
        state.pending.extend_from_slice(&buf[..n as usize]);
        // Décode ce qui est complet, garde la fin d'un caractère coupé.
        let (text, rest) = match std::str::from_utf8(&state.pending) {
            Ok(s) => (s.to_string(), Vec::new()),
            Err(e) if e.error_len().is_none() => {
                let cut = e.valid_up_to();
                (
                    String::from_utf8_lossy(&state.pending[..cut]).into_owned(),
                    state.pending[cut..].to_vec(),
                )
            }
            Err(_) => (
                String::from_utf8_lossy(&state.pending).into_owned(),
                Vec::new(),
            ),
        };
        state.pending = rest;
        for unit in text.encode_utf16() {
            state.queue.push_back(Record::Key(unit));
        }
        if (n as usize) < buf.len() {
            break;
        }
    }
}

/// Attend une entrée (ou un changement de taille) pendant `timeout` ms.
/// Renvoie true si au moins un enregistrement est disponible.
pub(crate) fn wait(timeout: u32) -> bool {
    let start = std::time::Instant::now();
    loop {
        {
            let mut st = crate::dlls::lock(&STATE);
            fill(&mut st);
            if !st.queue.is_empty() {
                return true;
            }
        }
        let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
        if timeout != u32::MAX && elapsed >= timeout {
            return false;
        }
        let slice = if timeout == u32::MAX {
            100
        } else {
            (timeout - elapsed).min(100)
        };
        let mut p = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut p, 1, slice as i32);
        }
    }
}

/// L'entrée de la console a-t-elle quelque chose à lire ? (WaitForSingleObject)
pub(crate) fn is_console_input(h: usize) -> bool {
    is_input(h)
}

unsafe fn write_record(dst: *mut u8, r: Record, wide: bool) {
    std::ptr::write_bytes(dst, 0, RECORD_SIZE);
    match r {
        Record::Key(unit) => {
            (dst as *mut u16).write_unaligned(KEY_EVENT);
            (dst.add(4) as *mut i32).write_unaligned(1); // bKeyDown
            (dst.add(8) as *mut u16).write_unaligned(1); // wRepeatCount
            let (vk, ctrl) = match unit {
                0x0D => (0x0D, 0),
                0x1B => (0x1B, 0),
                0x08 | 0x7F => (0x08, 0),
                0x09 => (0x09, 0),
                0x01..=0x1A => (0x40 + unit, 0x0008), // Ctrl+lettre : LEFT_CTRL_PRESSED
                _ => (0, 0),
            };
            (dst.add(10) as *mut u16).write_unaligned(vk);
            if wide {
                (dst.add(14) as *mut u16).write_unaligned(unit);
            } else {
                *dst.add(14) = if unit < 0x80 { unit as u8 } else { b'?' };
            }
            (dst.add(16) as *mut u32).write_unaligned(ctrl);
        }
        Record::Resize(c, r) => {
            (dst as *mut u16).write_unaligned(WINDOW_BUFFER_SIZE_EVENT);
            (dst.add(4) as *mut u16).write_unaligned(c);
            (dst.add(6) as *mut u16).write_unaligned(r);
        }
    }
}

unsafe fn read_records(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
    wide: bool,
    remove: bool,
    block: bool,
) -> i32 {
    if !is_input(h) {
        crate::process::set_last_error(6);
        return 0;
    }
    if buf.is_null() || read.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if block {
        wait(u32::MAX);
    }
    let mut st = crate::dlls::lock(&STATE);
    fill(&mut st);
    let n = (len as usize).min(st.queue.len());
    for (i, r) in st.queue.iter().take(n).enumerate() {
        write_record(buf.add(i * RECORD_SIZE), *r, wide);
    }
    if remove {
        st.queue.drain(..n);
    }
    *read = n as u32;
    1
}

pub unsafe extern "win64" fn read_console_input_w(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
) -> i32 {
    read_records(h, buf, len, read, true, true, true)
}
pub unsafe extern "win64" fn read_console_input_a(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
) -> i32 {
    read_records(h, buf, len, read, false, true, true)
}
pub unsafe extern "win64" fn peek_console_input_w(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
) -> i32 {
    read_records(h, buf, len, read, true, false, false)
}
pub unsafe extern "win64" fn peek_console_input_a(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
) -> i32 {
    read_records(h, buf, len, read, false, false, false)
}

/// `WriteConsoleInputW` : ajoute des touches dans la file d'entrée.
pub unsafe extern "win64" fn write_console_input_w(
    h: usize,
    buf: *const u8,
    len: u32,
    written: *mut u32,
) -> i32 {
    if !is_input(h) {
        crate::process::set_last_error(6);
        return 0;
    }
    if buf.is_null() && len != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut st = crate::dlls::lock(&STATE);
    for i in 0..len as usize {
        let r = buf.add(i * RECORD_SIZE);
        let kind = (r as *const u16).read_unaligned();
        let down = (r.add(4) as *const i32).read_unaligned();
        let unit = (r.add(14) as *const u16).read_unaligned();
        if kind == KEY_EVENT && down != 0 && unit != 0 {
            st.queue.push_back(Record::Key(unit));
        }
    }
    if !written.is_null() {
        *written = len;
    }
    1
}

pub unsafe extern "win64" fn get_number_of_console_input_events(h: usize, n: *mut u32) -> i32 {
    if !is_input(h) {
        crate::process::set_last_error(6);
        return 0;
    }
    if n.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut st = crate::dlls::lock(&STATE);
    fill(&mut st);
    *n = st.queue.len() as u32;
    1
}

pub unsafe extern "win64" fn flush_console_input_buffer(h: usize) -> i32 {
    if !is_input(h) {
        crate::process::set_last_error(6);
        return 0;
    }
    let mut st = crate::dlls::lock(&STATE);
    st.queue.clear();
    st.pending.clear();
    libc::tcflush(0, libc::TCIFLUSH);
    1
}

/// `ReadConsoleW` : caractères tapés (une ligne en mode canonique, ce qui
/// est disponible en mode brut).
pub unsafe extern "win64" fn read_console_w(
    h: usize,
    buf: *mut u16,
    len: u32,
    read: *mut u32,
    _control: usize,
) -> i32 {
    if !is_input(h) {
        crate::process::set_last_error(6);
        return 0;
    }
    if buf.is_null() || read.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut out = Vec::new();
    loop {
        wait(u32::MAX);
        let mut st = crate::dlls::lock(&STATE);
        while out.len() < len as usize {
            match st.queue.pop_front() {
                Some(Record::Key(u)) => out.push(if u == 0x0A { 0x0D } else { u }),
                Some(Record::Resize(..)) => {}
                None => break,
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    // Le terminal envoie « \n » en fin de ligne ; Windows rend « \r\n ».
    if out.last() == Some(&0x0D) && out.len() < len as usize {
        out.push(0x0A);
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    *read = out.len() as u32;
    1
}

pub unsafe extern "win64" fn read_console_a(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
    control: usize,
) -> i32 {
    let mut wide = vec![0u16; len as usize];
    let mut n = 0u32;
    if read_console_w(h, wide.as_mut_ptr(), len, &mut n, control) == 0 {
        return 0;
    }
    let text = String::from_utf16_lossy(&wide[..n as usize]);
    let bytes = text.as_bytes();
    let k = bytes.len().min(len as usize);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, k);
    if !read.is_null() {
        *read = k as u32;
    }
    1
}

/// Curseur et remplissage : traduits en séquences VT pour le terminal.
pub unsafe extern "win64" fn set_console_cursor_position(h: usize, pos: u32) -> i32 {
    let x = pos & 0xFFFF;
    let y = pos >> 16;
    write_vt(h, &format!("\x1b[{};{}H", y + 1, x + 1))
}

fn write_vt(h: usize, s: &str) -> i32 {
    use std::io::Write;
    let h = super::kernel32::canon_std(h);
    let ok = if h == (-12isize) as usize {
        std::io::stderr().write_all(s.as_bytes()).is_ok()
    } else if h == (-11isize) as usize {
        let mut o = std::io::stdout();
        o.write_all(s.as_bytes()).and_then(|_| o.flush()).is_ok()
    } else {
        crate::process::set_last_error(6);
        return 0;
    };
    ok as i32
}

#[repr(C)]
pub struct ConsoleCursorInfo {
    size: u32,
    visible: i32,
}

pub unsafe extern "win64" fn get_console_cursor_info(
    h: usize,
    info: *mut ConsoleCursorInfo,
) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    (*info).size = 25;
    (*info).visible = 1;
    let h = super::kernel32::canon_std(h);
    (h == (-11isize) as usize || h == (-12isize) as usize) as i32
}

pub unsafe extern "win64" fn set_console_cursor_info(
    h: usize,
    info: *const ConsoleCursorInfo,
) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    write_vt(
        h,
        if (*info).visible != 0 {
            "\x1b[?25h"
        } else {
            "\x1b[?25l"
        },
    )
}

/// FillConsoleOutputCharacterW(h, car, nombre, coord, écrits) : efface
/// (le cas courant : des espaces) à partir de la position donnée.
pub unsafe extern "win64" fn fill_console_output_character_w(
    h: usize,
    ch: u16,
    count: u32,
    pos: u32,
    written: *mut u32,
) -> i32 {
    let c = char::from_u32(ch as u32).unwrap_or(' ');
    let x = pos & 0xFFFF;
    let y = pos >> 16;
    let text: String = std::iter::repeat_n(c, count.min(1 << 20) as usize).collect();
    let r = write_vt(h, &format!("\x1b7\x1b[{};{}H{}\x1b8", y + 1, x + 1, text));
    if r != 0 && !written.is_null() {
        *written = count;
    }
    r
}

pub unsafe extern "win64" fn fill_console_output_character_a(
    h: usize,
    ch: u8,
    count: u32,
    pos: u32,
    written: *mut u32,
) -> i32 {
    fill_console_output_character_w(h, ch as u16, count, pos, written)
}

pub unsafe extern "win64" fn fill_console_output_attribute(
    h: usize,
    _attr: u16,
    count: u32,
    _pos: u32,
    written: *mut u32,
) -> i32 {
    if !written.is_null() {
        *written = count;
    }
    let h = super::kernel32::canon_std(h);
    (h == (-11isize) as usize || h == (-12isize) as usize) as i32
}

/// GetConsoleScreenBufferInfoEx : taille du terminal, 16 couleurs.
pub unsafe extern "win64" fn get_console_screen_buffer_info_ex(h: usize, info: *mut u8) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    // cbSize (4) puis les mêmes champs que CONSOLE_SCREEN_BUFFER_INFO.
    if super::kernel32_misc::get_console_screen_buffer_info(h, info.add(4) as *mut u16) == 0 {
        return 0;
    }
    1
}

pub unsafe extern "win64" fn set_console_screen_buffer_info_ex(h: usize, _info: *const u8) -> i32 {
    let h = super::kernel32::canon_std(h);
    (h == (-11isize) as usize || h == (-12isize) as usize) as i32
}

/// Pseudo-consoles (ConPTY) : non prises en charge, les programmes
/// retombent alors sur des tubes.
pub unsafe extern "win64" fn create_pseudo_console(
    _size: u32,
    _input: usize,
    _output: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    0x8000_4001u32 as i32 // E_NOTIMPL
}
pub extern "win64" fn resize_pseudo_console(_h: usize, _size: u32) -> i32 {
    0x8000_4001u32 as i32
}
pub extern "win64" fn close_pseudo_console(_h: usize) {}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "ReadConsoleInputW" => fn read_console_input_w,
        "ReadConsoleInputA" => fn read_console_input_a,
        "PeekConsoleInputW" => fn peek_console_input_w,
        "PeekConsoleInputA" => fn peek_console_input_a,
        "GetNumberOfConsoleInputEvents" => fn get_number_of_console_input_events,
        "WriteConsoleInputW" => fn write_console_input_w,
        "FlushConsoleInputBuffer" => fn flush_console_input_buffer,
        "ReadConsoleW" => fn read_console_w,
        "ReadConsoleA" => fn read_console_a,
        "SetConsoleCursorPosition" => fn set_console_cursor_position,
        "GetConsoleCursorInfo" => fn get_console_cursor_info,
        "SetConsoleCursorInfo" => fn set_console_cursor_info,
        "FillConsoleOutputCharacterW" => fn fill_console_output_character_w,
        "FillConsoleOutputCharacterA" => fn fill_console_output_character_a,
        "FillConsoleOutputAttribute" => fn fill_console_output_attribute,
        "GetConsoleScreenBufferInfoEx" => fn get_console_screen_buffer_info_ex,
        "SetConsoleScreenBufferInfoEx" => fn set_console_screen_buffer_info_ex,
        "CreatePseudoConsole" => fn create_pseudo_console,
        "ResizePseudoConsole" => fn resize_pseudo_console,
        "ClosePseudoConsole" => fn close_pseudo_console,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enregistrement_de_touche() {
        let mut buf = [0u8; RECORD_SIZE];
        unsafe { write_record(buf.as_mut_ptr(), Record::Key('é' as u16), true) };
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), KEY_EVENT);
        assert_eq!(i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]), 1);
        assert_eq!(u16::from_le_bytes([buf[14], buf[15]]), 'é' as u16);
        unsafe { write_record(buf.as_mut_ptr(), Record::Resize(120, 40), true) };
        assert_eq!(
            u16::from_le_bytes([buf[0], buf[1]]),
            WINDOW_BUFFER_SIZE_EVENT
        );
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 120);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 40);
    }
}
