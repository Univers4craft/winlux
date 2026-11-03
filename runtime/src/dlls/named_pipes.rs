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
//!
//! Entre processus Windows (crashpad de Chromium et d'Electron, serveurs
//! d'applis), une instance qui attend dans ConnectNamedPipe écoute aussi
//! sur une socket Unix de rendez-vous, dans un dossier privé de
//! l'utilisateur (`$XDG_RUNTIME_DIR/winlux-tubes`, droits 0700). Un client
//! d'un autre processus qui ne trouve pas le nom chez lui s'y connecte ; le
//! serveur accepte et la connexion remplace (dup3) le bout serveur de
//! l'instance, qui garde ainsi son handle et son descripteur.

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
    /// Rendez-vous ouvert aux autres processus (ConnectNamedPipe).
    listening: bool,
    /// Change à chaque DisconnectNamedPipe : le fil d'écoute d'une
    /// connexion précédente s'arrête au lieu de servir la nouvelle.
    generation: u64,
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
        listening: false,
        generation: 0,
    });
    crate::trace!("CreateNamedPipe({name}) = {h:#x}");
    h
}

/// `CreateFile` sur un nom de tube : connexion à la première instance libre.
pub(crate) fn open_client(name: &str, flags: u32) -> usize {
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let local = st.pipes.get_mut(name);
    let exists = local.is_some();
    let Some(inst) = local.and_then(|list| list.iter_mut().find(|i| i.client.is_some())) else {
        drop(st);
        // Pas d'instance libre ici : peut-être dans un autre processus.
        return match remote::connect(name) {
            Ok(fd) => {
                let h = crate::dlls::kernel32::insert_file(std::fs::File::from(fd));
                if flags & FILE_FLAG_OVERLAPPED != 0 {
                    crate::dlls::kernel32::mark_overlapped_handle(h);
                }
                crate::trace!("CreateFile(tube {name}, autre processus) = {h:#x}");
                crate::process::set_last_error(0);
                h
            }
            Err(seen) => {
                crate::process::set_last_error(if exists || seen {
                    ERROR_PIPE_BUSY
                } else {
                    ERROR_FILE_NOT_FOUND
                });
                usize::MAX
            }
        };
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
    if !inst.listening {
        inst.listening = true;
        let name = st
            .pipes
            .iter()
            .find(|(_, l)| l.iter().any(|i| i.server == h))
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        let generation = inst_generation(&mut st, h);
        remote::listen(&name, h, generation);
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

fn inst_generation(st: &mut State, h: usize) -> u64 {
    find_server(st, h).map(|i| i.generation).unwrap_or(0)
}

/// `DisconnectNamedPipe` : le client courant perd sa connexion (il lit la
/// fin du tube) et l'instance redevient libre pour le prochain
/// ConnectNamedPipe, avec le même handle. Sans cela, le serveur de crashpad
/// retrouvait l'ancien client parti à chaque tour (ConnectNamedPipe
/// répondait ERROR_PIPE_CONNECTED, ReadFile la fin du tube) et bouclait.
pub(crate) fn disconnect(h: usize) -> i32 {
    use std::os::fd::AsRawFd;
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    let Some(inst) = find_server(&mut st, h) else {
        // Bout cédé à un enfant ou handle ordinaire : rien à libérer.
        return 1;
    };
    let (Some(server), Some((new_server, new_client))) =
        (crate::dlls::kernel32::fd_of_handle(h), socket_pair())
    else {
        crate::process::set_last_error(8);
        return 0;
    };
    // Le nouveau bout prend la place de l'ancien sous le même descripteur ;
    // l'ancienne connexion se ferme avec lui.
    if unsafe { libc::dup3(new_server.as_raw_fd(), server, libc::O_CLOEXEC) } < 0 {
        crate::process::set_last_error(8);
        return 0;
    }
    drop(new_server);
    inst.client = Some(new_client);
    inst.connected = false;
    inst.listening = false;
    inst.generation += 1;
    reg.changed.notify_all();
    crate::trace!("DisconnectNamedPipe({h:#x})");
    1
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
            || remote::available(&name)
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

/// Instance de tube transmise à un enfant (voir `inherit`) : nom, bout
/// client en attente, connectée ?
pub(crate) fn export(h: usize) -> Option<(String, Option<i32>, bool)> {
    use std::os::fd::AsRawFd;
    let st = crate::dlls::lock(&registry().state);
    st.pipes.iter().find_map(|(name, list)| {
        let i = list.iter().find(|i| i.server == h)?;
        Some((
            name.clone(),
            i.client.as_ref().map(|c| c.as_raw_fd()),
            i.connected,
        ))
    })
}

/// Le parent cède l'instance à l'enfant : elle quitte son registre (son
/// handle reste un simple descripteur).
pub(crate) fn release(h: usize) {
    close_server(h);
}

/// L'enfant reprend une instance héritée.
pub(crate) fn adopt(h: usize, name: &str, client: Option<OwnedFd>, connected: bool) {
    let reg = registry();
    let mut st = crate::dlls::lock(&reg.state);
    st.pipes
        .entry(name.to_string())
        .or_default()
        .push(Instance {
            server: h,
            client,
            connected,
            listening: false,
            generation: 0,
        });
    reg.changed.notify_all();
}

/// Rendez-vous entre processus : sockets Unix nommées
/// `<clé du nom>.<pid>.<handle>` dans un dossier privé.
mod remote {
    use super::{find_server, registry};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn dir() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| PathBuf::from(format!("/tmp/winlux-{}", unsafe { libc::getuid() })));
        let d = base.join("winlux-tubes");
        std::fs::create_dir_all(&d).ok()?;
        let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
        Some(d)
    }

    /// Clé courte et sans caractère spécial pour un nom de tube.
    fn key(name: &str) -> String {
        crate::dlls::bcrypt::sha256(name.as_bytes())[..12]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn address(path: &std::path::Path) -> Option<(libc::sockaddr_un, libc::socklen_t)> {
        use std::os::unix::ffi::OsStrExt;
        let mut a: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        a.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let b = path.as_os_str().as_bytes();
        if b.len() >= a.sun_path.len() {
            return None;
        }
        for (d, s) in a.sun_path.iter_mut().zip(b) {
            *d = *s as libc::c_char;
        }
        Some((
            a,
            std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
        ))
    }

    fn socket() -> Option<OwnedFd> {
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// Les instances des autres processus qui attendent un client.
    fn candidates(name: &str) -> Vec<PathBuf> {
        let Some(d) = dir() else {
            return Vec::new();
        };
        let prefix = format!("{}.", key(name));
        let mine = format!("{prefix}{}.", std::process::id());
        let mut v: Vec<PathBuf> = std::fs::read_dir(&d)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.starts_with(&prefix) && !n.starts_with(&mine)
            })
            .map(|e| e.path())
            .collect();
        v.sort();
        v
    }

    pub(super) fn available(name: &str) -> bool {
        !candidates(name).is_empty()
    }

    /// Connexion à une instance d'un autre processus. Err(true) : il y en
    /// avait, mais aucune n'a répondu.
    pub(super) fn connect(name: &str) -> Result<OwnedFd, bool> {
        let Some(d) = dir() else {
            return Err(false);
        };
        // Un verrou par nom : deux clients ne prennent pas la même instance.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(d.join(format!("{}.verrou", key(name))))
            .map_err(|_| false)?;
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) };
        let list = candidates(name);
        let seen = !list.is_empty();
        for path in list {
            let (Some(s), Some((a, len))) = (socket(), address(&path)) else {
                continue;
            };
            let rc = unsafe {
                libc::connect(s.as_raw_fd(), (&a as *const libc::sockaddr_un).cast(), len)
            };
            // Pris, ou abandonné par un processus disparu : on l'efface.
            let _ = std::fs::remove_file(&path);
            if rc == 0 {
                return Ok(s);
            }
        }
        Err(seen)
    }

    /// ConnectNamedPipe : ouvre le rendez-vous de l'instance `h` et attend,
    /// dans un fil, un client d'un autre processus.
    pub(super) fn listen(name: &str, h: usize, generation: u64) {
        let Some(d) = dir() else {
            return;
        };
        let path = d.join(format!(
            "{}.{}.{h:x}.{generation}",
            key(name),
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let (Some(l), Some((a, len))) = (socket(), address(&path)) else {
            return;
        };
        unsafe {
            if libc::bind(l.as_raw_fd(), (&a as *const libc::sockaddr_un).cast(), len) != 0
                || libc::listen(l.as_raw_fd(), 4) != 0
            {
                return;
            }
        }
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        let socket_path = path.clone();
        let spawned = std::thread::Builder::new()
            .name("winlux-pipe-accept".into())
            .spawn(move || {
                let path = socket_path;
                loop {
                    {
                        let mut st = crate::dlls::lock(&registry().state);
                        match find_server(&mut st, h) {
                            Some(i) if i.generation != generation => break,
                            Some(i) if !i.connected && i.client.is_some() => {}
                            Some(i) => {
                                i.listening = false;
                                break;
                            }
                            None => break,
                        }
                    }
                    let mut p = libc::pollfd {
                        fd: l.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if unsafe { libc::poll(&mut p, 1, 200) } <= 0 {
                        continue;
                    }
                    let a = unsafe {
                        libc::accept4(
                            l.as_raw_fd(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            libc::SOCK_CLOEXEC,
                        )
                    };
                    if a < 0 {
                        continue;
                    }
                    let accepted = unsafe { OwnedFd::from_raw_fd(a) };
                    let reg = registry();
                    let mut st = crate::dlls::lock(&reg.state);
                    let Some(i) = find_server(&mut st, h) else {
                        break;
                    };
                    let Some(server) = crate::dlls::kernel32::fd_of_handle(h) else {
                        break;
                    };
                    if i.connected
                        || i.generation != generation
                        || unsafe { libc::dup3(accepted.as_raw_fd(), server, libc::O_CLOEXEC) } < 0
                    {
                        continue;
                    }
                    i.client = None;
                    i.connected = true;
                    i.listening = false;
                    reg.changed.notify_all();
                    crate::trace!("tube {h:#x} : client d'un autre processus");
                    break;
                }
                let _ = std::fs::remove_file(&path);
            });
        if spawned.is_err() {
            let _ = std::fs::remove_file(&path);
        }
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
