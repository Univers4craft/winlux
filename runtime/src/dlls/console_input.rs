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

// ── Pseudo-consoles (ConPTY) sur un vrai pseudo-terminal Linux ──────────────
//
// CreatePseudoConsole(taille, entrée, sortie) ouvre un pty (openpty). Deux
// fils relaient : ce que l'appelant écrit dans son tube d'entrée part vers le
// maître du pty ; ce que le programme lancé écrit (séquences VT, couleurs)
// revient dans le tube de sortie. CreateProcess avec
// PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE donne l'esclave au programme comme
// terminal de contrôle (entrée, sortie, erreur) : il voit une vraie console,
// avec couleurs, taille (ResizePseudoConsole → TIOCSWINSZ → SIGWINCH) et
// saisie interactive.

struct PseudoConsole {
    master: std::sync::Arc<std::os::fd::OwnedFd>,
    /// Gardé ouvert jusqu'à ClosePseudoConsole : comme sous Windows, la
    /// sortie reste ouverte même après la fin du programme.
    slave: Option<std::os::fd::OwnedFd>,
    closed: std::sync::Arc<AtomicBool>,
}

static CONSOLES: Mutex<Vec<(usize, PseudoConsole)>> = Mutex::new(Vec::new());
static NEXT_CONSOLE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0x7c00_0010);

fn winsize(size: u32) -> libc::winsize {
    let columns = (size & 0xffff) as u16;
    let rows = (size >> 16) as u16;
    libc::winsize {
        ws_row: if rows == 0 { 25 } else { rows },
        ws_col: if columns == 0 { 80 } else { columns },
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

/// Copie du descripteur Linux d'un handle (tube de CreatePipe, fichier).
fn dup_handle(h: usize) -> Option<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    let fd = super::kernel32::fd_of_handle(h)?;
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    (copy >= 0).then(|| unsafe { std::os::fd::OwnedFd::from_raw_fd(copy) })
}

fn write_all(fd: i32, mut data: &[u8]) -> bool {
    while !data.is_empty() {
        let n = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        data = &data[n as usize..];
    }
    true
}

pub unsafe extern "win64" fn create_pseudo_console(
    size: u32,
    input: usize,
    output: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    use std::os::fd::{AsRawFd, FromRawFd};
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_HANDLE: i32 = 0x8007_0006u32 as i32;
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let (Some(input), Some(output)) = (dup_handle(input), dup_handle(output)) else {
        return E_HANDLE;
    };
    let (mut master, mut slave) = (0, 0);
    let size = winsize(size);
    if libc::openpty(
        &mut master,
        &mut slave,
        std::ptr::null_mut(),
        std::ptr::null(),
        &size,
    ) != 0
    {
        return 0x8000_4005u32 as i32; // E_FAIL
    }
    for fd in [master, slave] {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    let master = std::sync::Arc::new(std::os::fd::OwnedFd::from_raw_fd(master));
    let slave = std::os::fd::OwnedFd::from_raw_fd(slave);
    let closed = std::sync::Arc::new(AtomicBool::new(false));
    // Entrée : tube de l'appelant → maître du pty (attente bornée pour voir
    // ClosePseudoConsole même si l'appelant garde son tube ouvert).
    let (to_master, stop) = (master.clone(), closed.clone());
    let _ = std::thread::Builder::new()
        .name("winlux-conpty-in".into())
        .spawn(move || {
            let mut buffer = [0u8; 4096];
            while !stop.load(Ordering::Acquire) {
                let mut poll = libc::pollfd {
                    fd: input.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                if unsafe { libc::poll(&mut poll, 1, 200) } <= 0 {
                    continue;
                }
                let n = unsafe {
                    libc::read(
                        input.as_raw_fd(),
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                    )
                };
                if n <= 0 || !write_all(to_master.as_raw_fd(), &buffer[..n as usize]) {
                    break;
                }
            }
        });
    // Sortie : maître du pty → tube de l'appelant, jusqu'à la fermeture.
    let from_master = master.clone();
    let _ = std::thread::Builder::new()
        .name("winlux-conpty-out".into())
        .spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                let n = unsafe {
                    libc::read(
                        from_master.as_raw_fd(),
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                    )
                };
                if n < 0
                    && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                {
                    continue;
                }
                if n <= 0 || !write_all(output.as_raw_fd(), &buffer[..n as usize]) {
                    break;
                }
            }
            // Fin : `output` se ferme ici, l'appelant lit la fin du tube.
        });
    let handle = NEXT_CONSOLE.fetch_add(4, Ordering::Relaxed);
    crate::dlls::lock(&CONSOLES).push((
        handle,
        PseudoConsole {
            master,
            slave: Some(slave),
            closed,
        },
    ));
    crate::trace!(
        "CreatePseudoConsole : {}x{} → {handle:#x}",
        size.ws_col,
        size.ws_row
    );
    *out = handle;
    0
}

pub extern "win64" fn resize_pseudo_console(h: usize, size: u32) -> i32 {
    use std::os::fd::AsRawFd;
    let consoles = crate::dlls::lock(&CONSOLES);
    let Some((_, console)) = consoles.iter().find(|(handle, _)| *handle == h) else {
        return 0x8007_0006u32 as i32; // E_HANDLE
    };
    let size = winsize(size);
    if unsafe { libc::ioctl(console.master.as_raw_fd(), libc::TIOCSWINSZ, &size) } != 0 {
        return 0x8000_4005u32 as i32;
    }
    0
}

/// Ferme la console : les programmes attachés reçoivent SIGHUP (comme
/// CTRL_CLOSE_EVENT), puis le tube de sortie se ferme.
pub extern "win64" fn close_pseudo_console(h: usize) {
    use std::os::fd::AsRawFd;
    let console = {
        let mut consoles = crate::dlls::lock(&CONSOLES);
        let Some(index) = consoles.iter().position(|(handle, _)| *handle == h) else {
            return;
        };
        consoles.remove(index).1
    };
    console.closed.store(true, Ordering::Release);
    let group = unsafe { libc::tcgetpgrp(console.master.as_raw_fd()) };
    if group > 0 {
        unsafe { libc::kill(-group, libc::SIGHUP) };
    }
    drop(console.slave);
}

/// Prépare `cmd` pour un programme attaché à la pseudo-console `h` :
/// l'esclave du pty devient son entrée, sa sortie, son erreur et son
/// terminal de contrôle (nouvelle session). Faux si `h` est inconnu.
pub(crate) fn attach_pseudo_console(cmd: &mut std::process::Command, h: usize) -> bool {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let consoles = crate::dlls::lock(&CONSOLES);
    let Some((_, console)) = consoles.iter().find(|(handle, _)| *handle == h) else {
        return false;
    };
    let Some(slave) = console.slave.as_ref() else {
        return false;
    };
    let copy = |fd: i32| {
        let c = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        (c >= 0).then(|| unsafe { std::os::fd::OwnedFd::from_raw_fd(c) })
    };
    let (Some(i), Some(o), Some(e)) = (
        copy(slave.as_raw_fd()),
        copy(slave.as_raw_fd()),
        copy(slave.as_raw_fd()),
    ) else {
        return false;
    };
    cmd.stdin(i).stdout(o).stderr(e);
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // Le terminal de l'entrée standard devient celui de la session.
            libc::ioctl(0, libc::TIOCSCTTY, 0);
            Ok(())
        });
    }
    true
}

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

    /// Lit le tube de sortie jusqu'à voir `until` (5 s au plus).
    fn read_until(h: usize, until: &str) -> String {
        let fd = super::super::kernel32::fd_of_handle(h).unwrap();
        let mut text = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < 5 && !String::from_utf8_lossy(&text).contains(until) {
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut poll, 1, 100) } <= 0 {
                continue;
            }
            let mut buffer = [0u8; 1024];
            let n =
                unsafe { libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len()) };
            if n <= 0 {
                break;
            }
            text.extend_from_slice(&buffer[..n as usize]);
        }
        String::from_utf8_lossy(&text).into_owned()
    }

    #[test]
    fn pseudo_console_vraie() {
        use super::super::kernel32::{create_pipe, fd_of_handle};
        let (mut in_read, mut in_write, mut out_read, mut out_write) =
            (0usize, 0usize, 0usize, 0usize);
        unsafe {
            assert_eq!(
                create_pipe(&mut in_read, &mut in_write, std::ptr::null(), 0),
                1
            );
            assert_eq!(
                create_pipe(&mut out_read, &mut out_write, std::ptr::null(), 0),
                1
            );
        }
        let mut console = 0usize;
        // 100 colonnes, 30 lignes (COORD : X en bas, Y en haut).
        let hr =
            unsafe { create_pseudo_console(100 | (30 << 16), in_read, out_write, 0, &mut console) };
        assert_eq!(hr, 0);
        assert_ne!(console, 0);
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.arg("-c").arg("stty size; test -t 0 && echo TTY; read ligne; echo \"lu:$ligne\"; printf '\\033[31mrouge\\033[0m\\n'");
        assert!(attach_pseudo_console(&mut cmd, console));
        let mut child = cmd.spawn().unwrap();
        let text = read_until(out_read, "TTY");
        assert!(text.contains("30 100"), "taille transmise : {text:?}");
        // Saisie interactive : ce que l'appelant écrit arrive au programme.
        let fd = fd_of_handle(in_write).unwrap();
        assert!(write_all(fd, b"bonjour\r"));
        let text = read_until(out_read, "rouge");
        assert!(text.contains("lu:bonjour"), "saisie : {text:?}");
        assert!(text.contains("\u{1b}[31mrouge"), "couleurs VT : {text:?}");
        assert!(child.wait().unwrap().success());
        assert_eq!(resize_pseudo_console(console, 120 | (40 << 16)), 0);
        close_pseudo_console(console);
        assert_ne!(
            resize_pseudo_console(console, 80 | (25 << 16)),
            0,
            "console fermée"
        );
    }

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
