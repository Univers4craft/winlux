//! Tubes nommés Windows (`\\.\pipe\nom`) sur des paires de sockets Unix.
//!
//! libuv (Node.js, Bun, donc Claude Code) crée l'entrée et la sortie de
//! chaque processus enfant ainsi : CreateNamedPipeW côté serveur, puis
//! CreateFileW sur le même nom côté client, et ConnectNamedPipe. Chaque
//! instance du tube est une paire `socketpair(AF_UNIX, SOCK_STREAM)` :
//! le serveur garde un bout, le premier client qui ouvre le nom reçoit
//! l'autre. Les deux bouts sont des descripteurs Linux ordinaires, que les
//! programmes Linux lancés par CreateProcess (bash, git…) héritent tels
//! quels comme entrée/sortie standard.

use std::collections::HashMap;
use std::os::unix::io::{FromRawFd, OwnedFd};
use std::sync::{Condvar, Mutex};

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_NAME: u32 = 123;
const ERROR_PIPE_BUSY: u32 = 231;
const ERROR_PIPE_CONNECTED: u32 = 535;
const ERROR_IO_PENDING: u32 = 997;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;

struct Instance {
    /// Handle Winlux du bout serveur.
    server: usize,
    /// Bout client en attente d'un CreateFileW (None une fois pris).
    client: Option<OwnedFd>,
    connected: bool,
}

struct State {
    pipes: HashMap<String, Vec<Instance>>,
}

struct Registry {
    state: Mutex<State>,
    changed: Condvar,
}

fn registry() -> &'static Registry {
    static R: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    R.get_or_init(|| Registry {
        state: Mutex::new(State {
            pipes: HashMap::new(),
        }),
        changed: Condvar::new(),
    })
}

/// Nom normalisé d'un chemin de tube (`\\.\pipe\x`, `\\?\pipe\x`,
/// `\??\pipe\x`, `\Device\NamedPipe\x`), ou None si ce n'en est pas un.
pub(crate) fn pipe_name(path: &str) -> Option<String> {
    let p = path.replace('/', "\\");
    let lower = p.to_ascii_lowercase();
    for prefix in [
        "\\\\.\\pipe\\",
        "\\\\?\\pipe\\",
        "\\??\\pipe\\",
        "\\device\\namedpipe\\",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.is_empty() {
                return None;
            }
            return Some(rest.to_string());
        }
    }
    None
}

fn socket_pair() -> Option<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];
    let rc = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return None;
    }
    unsafe { Some((OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))) }
}

/// `CreateNamedPipe` : nouvelle instance du tube `name`.
pub(crate) fn create(path: &str, open_mode: u32, _pipe_mode: u32, max_instances: u32) -> usize {
    let Some(name) = pipe_name(path) else {
        crate::process::set_last_error(ERROR_INVALID_NAME);
        return usize::MAX;
    };
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let list = st.pipes.entry(name.clone()).or_default();
    if open_mode & FILE_FLAG_FIRST_PIPE_INSTANCE != 0 && !list.is_empty() {
        crate::process::set_last_error(ERROR_ACCESS_DENIED);
        return usize::MAX;
    }
    let limit = if max_instances == 0 || max_instances >= 255 {
        usize::MAX
    } else {
        max_instances as usize
    };
    if list.len() >= limit {
        crate::process::set_last_error(ERROR_PIPE_BUSY);
        return usize::MAX;
    }
    let Some((server, client)) = socket_pair() else {
        crate::process::set_last_error(8);
        return usize::MAX;
    };
    let h = crate::dlls::kernel32::insert_file(std::fs::File::from(server));
    if open_mode & FILE_FLAG_OVERLAPPED != 0 {
        crate::dlls::kernel32::mark_overlapped_handle(h);
    }
    list.push(Instance {
        server: h,
        client: Some(client),
        connected: false,
    });
    crate::trace!("CreateNamedPipe({name}) = {h:#x}");
    h
}

/// `CreateFile` sur un nom de tube : connexion à la première instance libre.
pub(crate) fn open_client(name: &str, flags: u32) -> usize {
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let Some(list) = st.pipes.get_mut(name) else {
        crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
        return usize::MAX;
    };
    let Some(inst) = list.iter_mut().find(|i| i.client.is_some()) else {
        crate::process::set_last_error(ERROR_PIPE_BUSY);
        return usize::MAX;
    };
    let fd = inst.client.take().expect("bout client");
    inst.connected = true;
    let h = crate::dlls::kernel32::insert_file(std::fs::File::from(fd));
    if flags & FILE_FLAG_OVERLAPPED != 0 {
        crate::dlls::kernel32::mark_overlapped_handle(h);
    }
    reg.changed.notify_all();
    crate::trace!("CreateFile(tube {name}) = {h:#x}");
    crate::process::set_last_error(0);
    h
}

fn find_server(st: &mut State, h: usize) -> Option<&mut Instance> {
    st.pipes
        .values_mut()
        .flat_map(|l| l.iter_mut())
        .find(|i| i.server == h)
}

/// `ConnectNamedPipe(h, overlapped)` : attend qu'un client ouvre le tube.
pub(crate) fn connect(h: usize, overlapped: usize) -> i32 {
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let Some(inst) = find_server(&mut st, h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if inst.connected {
        crate::process::set_last_error(ERROR_PIPE_CONNECTED);
        return 0;
    }
    if overlapped == 0 {
        loop {
            match find_server(&mut st, h) {
                Some(i) if i.connected => return 1,
                Some(_) => {}
                None => {
                    crate::process::set_last_error(ERROR_INVALID_HANDLE);
                    return 0;
                }
            }
            st = reg
                .changed
                .wait(st)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    drop(st);
    unsafe {
        // OVERLAPPED.Internal = STATUS_PENDING jusqu'à la connexion.
        (overlapped as *mut usize).write(0x103);
    }
    let spawned = std::thread::Builder::new()
        .name("winlux-pipe-connect".into())
        .spawn(move || {
            let reg = registry();
            let mut st = crate::dlls::lock(&reg.state);
            loop {
                match find_server(&mut st, h) {
                    Some(i) if i.connected => break,
                    Some(_) => {}
                    None => {
                        drop(st);
                        crate::dlls::kernel32::complete_overlapped(h, overlapped, 0, 995);
                        return;
                    }
                }
                let (g, _) = reg
                    .changed
                    .wait_timeout(st, std::time::Duration::from_millis(200))
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                st = g;
            }
            drop(st);
            crate::dlls::kernel32::complete_overlapped(h, overlapped, 0, 0);
        });
    if spawned.is_err() {
        crate::process::set_last_error(8);
        return 0;
    }
    crate::process::set_last_error(ERROR_IO_PENDING);
    0
}

/// Fermeture d'un bout serveur : l'instance disparaît du nom.
pub(crate) fn close_server(h: usize) {
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let mut found = false;
    st.pipes.retain(|_, list| {
        let before = list.len();
        list.retain(|i| i.server != h);
        found |= list.len() != before;
        !list.is_empty()
    });
    if found {
        reg.changed.notify_all();
    }
}

/// `WaitNamedPipe` : une instance attend-elle un client ?
pub(crate) fn wait(path: &str, timeout_ms: u32) -> bool {
    let Some(name) = pipe_name(path) else {
        return false;
    };
    let reg = registry();
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(if timeout_ms == 0xFFFF_FFFF {
            u64::MAX / 4
        } else if timeout_ms == 0 {
            50
        } else {
            timeout_ms as u64
        });
    let mut st = crate::dlls::lock(&reg.state);
    loop {
        if st
            .pipes
            .get(&name)
            .is_some_and(|l| l.iter().any(|i| i.client.is_some()))
        {
            return true;
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return false;
        }
        let (g, _) = reg
            .changed
            .wait_timeout(
                st,
                (deadline - now).min(std::time::Duration::from_millis(200)),
            )
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        st = g;
    }
}

pub unsafe extern "win64" fn wait_named_pipe_w(name: *const u16, timeout: u32) -> i32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if wait(&crate::dlls::wstr(name), timeout) {
        1
    } else {
        crate::process::set_last_error(if timeout == 0 {
            121
        } else {
            ERROR_FILE_NOT_FOUND
        });
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_de_tubes() {
        assert_eq!(
            pipe_name("\\\\.\\pipe\\uv\\abc-12").as_deref(),
            Some("uv\\abc-12")
        );
        assert_eq!(pipe_name("\\\\?\\PIPE\\Truc").as_deref(), Some("truc"));
        assert_eq!(pipe_name("C:\\pipe\\x"), None);
    }
}
