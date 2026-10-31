//! PTY natif Winlux : alloue un pseudo-terminal Linux, lance un shell et
//! expose des poignées lisibles/inscriptibles au code PE invité.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

struct PtyState {
    master_fd: libc::c_int,
    child_pid: libc::pid_t,
}

static PTY_MAP: OnceLock<Mutex<HashMap<usize, PtyState>>> = OnceLock::new();
static NEXT_HANDLE: OnceLock<Mutex<usize>> = OnceLock::new();

fn pty_map() -> &'static Mutex<HashMap<usize, PtyState>> {
    PTY_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_handle() -> usize {
    let mut h = NEXT_HANDLE
        .get_or_init(|| Mutex::new(0xA000_0000))
        .lock()
        .unwrap();
    *h += 1;
    *h
}

/// Crée un PTY + processus shell fils. Renvoie un handle opaque (0 = échec).
pub unsafe extern "win64" fn winlux_create_pty(cols: u32, rows: u32) -> usize {
    let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
    if master < 0 {
        return 0;
    }
    if libc::grantpt(master) != 0 || libc::unlockpt(master) != 0 {
        libc::close(master);
        return 0;
    }
    let mut slave_name = [0u8; 256];
    if libc::ptsname_r(master, slave_name.as_mut_ptr() as *mut _, slave_name.len()) != 0 {
        libc::close(master);
        return 0;
    }
    let slave = libc::open(
        slave_name.as_ptr() as *const _,
        libc::O_RDWR | libc::O_NOCTTY,
    );
    if slave < 0 {
        libc::close(master);
        return 0;
    }
    let ws = libc::winsize {
        ws_row: rows.min(500) as u16,
        ws_col: cols.min(500) as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    libc::ioctl(slave, libc::TIOCSWINSZ, &ws);
    let flags = libc::fcntl(master, libc::F_GETFL);
    libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);

    let pid = libc::fork();
    if pid < 0 {
        libc::close(master);
        libc::close(slave);
        return 0;
    }
    if pid == 0 {
        libc::close(master);
        libc::setsid();
        libc::ioctl(slave, libc::TIOCSCTTY as libc::c_ulong, 0i32);
        libc::dup2(slave, 0);
        libc::dup2(slave, 1);
        libc::dup2(slave, 2);
        if slave > 2 {
            libc::close(slave);
        }
        std::env::set_var("TERM", "xterm-256color");
        std::env::set_var("COLORTERM", "truecolor");
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/bash".into());
        let c_shell = std::ffi::CString::new(shell).unwrap_or_else(|_| c"/bin/bash".into());
        let argv: [*const libc::c_char; 3] =
            [c_shell.as_ptr(), c"--login".as_ptr(), std::ptr::null()];
        libc::execvp(c_shell.as_ptr(), argv.as_ptr());
        libc::_exit(127);
    }
    libc::close(slave);

    let handle = next_handle();
    pty_map().lock().unwrap().insert(
        handle,
        PtyState {
            master_fd: master,
            child_pid: pid,
        },
    );
    handle
}

/// Lecture non-bloquante depuis le PTY. Renvoie 1 si des octets ont été lus.
pub unsafe extern "win64" fn winlux_read_pty(
    handle: usize,
    buf: *mut u8,
    len: u32,
    out_read: *mut u32,
) -> i32 {
    if !out_read.is_null() {
        *out_read = 0;
    }
    if buf.is_null() || len == 0 {
        return 0;
    }
    let fd = {
        let map = pty_map().lock().unwrap();
        match map.get(&handle) {
            Some(p) => p.master_fd,
            None => return 0,
        }
    };
    let n = libc::read(fd, buf as *mut libc::c_void, len as usize);
    if n <= 0 {
        return 0;
    }
    if !out_read.is_null() {
        *out_read = n as u32;
    }
    1
}

/// Écriture vers le PTY (envoi de saisie clavier au shell).
pub unsafe extern "win64" fn winlux_write_pty(
    handle: usize,
    buf: *const u8,
    len: u32,
    out_written: *mut u32,
) -> i32 {
    if !out_written.is_null() {
        *out_written = 0;
    }
    if buf.is_null() || len == 0 {
        return 0;
    }
    let fd = {
        let map = pty_map().lock().unwrap();
        match map.get(&handle) {
            Some(p) => p.master_fd,
            None => return 0,
        }
    };
    let n = libc::write(fd, buf as *const libc::c_void, len as usize);
    if n <= 0 {
        return 0;
    }
    if !out_written.is_null() {
        *out_written = n as u32;
    }
    1
}

/// Redimensionne le PTY (TIOCSWINSZ).
pub unsafe extern "win64" fn winlux_resize_pty(handle: usize, cols: u32, rows: u32) -> i32 {
    let map = pty_map().lock().unwrap();
    let Some(pty) = map.get(&handle) else {
        return 0;
    };
    let ws = libc::winsize {
        ws_row: rows.min(500) as u16,
        ws_col: cols.min(500) as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    (libc::ioctl(pty.master_fd, libc::TIOCSWINSZ, &ws) == 0) as i32
}

/// Vérifie si le processus fils est encore vivant.
pub unsafe extern "win64" fn winlux_pty_alive(handle: usize) -> i32 {
    let map = pty_map().lock().unwrap();
    let Some(pty) = map.get(&handle) else {
        return 0;
    };
    let mut status: libc::c_int = 0;
    let r = libc::waitpid(pty.child_pid, &mut status, libc::WNOHANG);
    if r == 0 {
        1
    } else {
        0
    }
}

/// Ferme le PTY et termine le processus fils.
pub unsafe extern "win64" fn winlux_close_pty(handle: usize) -> i32 {
    let pty = {
        let mut map = pty_map().lock().unwrap();
        match map.remove(&handle) {
            Some(p) => p,
            None => return 0,
        }
    };
    libc::close(pty.master_fd);
    libc::kill(pty.child_pid, libc::SIGHUP);
    let mut status: libc::c_int = 0;
    for _ in 0..10 {
        let r = libc::waitpid(pty.child_pid, &mut status, libc::WNOHANG);
        if r != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    libc::kill(pty.child_pid, libc::SIGKILL);
    libc::waitpid(pty.child_pid, &mut status, 0);
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxCreatePty" => fn winlux_create_pty,
        "WinluxReadPty" => fn winlux_read_pty,
        "WinluxWritePty" => fn winlux_write_pty,
        "WinluxResizePty" => fn winlux_resize_pty,
        "WinluxPtyAlive" => fn winlux_pty_alive,
        "WinluxClosePty" => fn winlux_close_pty,
    }
}
