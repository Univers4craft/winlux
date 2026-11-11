//! E/S Winsock avec OVERLAPPED et port de complétion (#1146).
//!
//! Le `HttpClient` de .NET n'utilise que ce chemin : la socket est liée au
//! port de complétion du pool de threads (CreateIoCompletionPort), la
//! connexion passe par ConnectEx (obtenu avec WSAIoctl
//! SIO_GET_EXTENSION_FUNCTION_POINTER), puis WSASend / WSARecv avec un
//! OVERLAPPED. Chaque opération se termine par un paquet posté au port,
//! qu'elle aboutisse tout de suite ou plus tard ; en cas d'échec, le code
//! Winsock se relit par WSAGetOverlappedResult. Sans tout cela, .NET
//! échouait avant même de se connecter et rendait SocketError -1
//! (« Erreur Windows 4294967295 »).
//!
//! Une opération qui ne peut pas aboutir tout de suite est confiée à un fil
//! qui attend la socket (sur une copie du descripteur, pour que closesocket
//! reste immédiat) et un eventfd d'annulation (CancelIoEx, closesocket).
//! Seule la forme 64 bits de l'OVERLAPPED est prise en charge ; les
//! programmes 32 bits gardent le chemin synchrone d'avant.

use super::{
    error_from_errno, last_error, lock, network_allowed, read_wsabuf, recv_flags, set_error,
    valid_socket, WsaBuf, SOCKET_ERROR, WSAEFAULT, WSAEINVAL, WSAENOBUFS, WSAENOTSOCK,
};
use crate::dlls::kernel32::Overlapped;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const STATUS_PENDING: usize = 0x103;
pub(super) const WSA_IO_PENDING: i32 = 997;
const WSA_IO_INCOMPLETE: i32 = 996;
pub(super) const WSA_OPERATION_ABORTED: i32 = 995;

/// `SIO_GET_EXTENSION_FUNCTION_POINTER`.
pub(super) const SIO_GET_EXTENSION_FUNCTION_POINTER: u32 = 0xC800_0006;
/// WSAID_CONNECTEX {25A207B9-DDF3-4660-8EE9-76E58C74063E}, tel qu'en mémoire.
const WSAID_CONNECTEX: [u8; 16] = [
    0xB9, 0x07, 0xA2, 0x25, 0xF3, 0xDD, 0x60, 0x46, 0x8E, 0xE9, 0x76, 0xE5, 0x8C, 0x74, 0x06, 0x3E,
];

struct Pending {
    socket: usize,
    wake: i32,
    cancelled: Arc<AtomicBool>,
    overlapped: usize,
}

fn pending() -> &'static Mutex<HashMap<u64, Pending>> {
    static P: OnceLock<Mutex<HashMap<u64, Pending>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Code Winsock d'une opération terminée en échec, relu par
/// WSAGetOverlappedResult (l'OVERLAPPED ne garde qu'un NTSTATUS).
fn errors() -> &'static Mutex<HashMap<usize, i32>> {
    static E: OnceLock<Mutex<HashMap<usize, i32>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// SetFileCompletionNotificationModes d'une socket : bits
/// FILE_SKIP_COMPLETION_PORT_ON_SUCCESS (1) et FILE_SKIP_SET_EVENT_ON_HANDLE
/// (2). libuv (Node, Electron) les pose sur chaque socket TCP et traite
/// lui-même une opération finie tout de suite : un paquet au port en plus
/// la ferait traiter deux fois (#1155).
fn modes() -> &'static Mutex<HashMap<usize, u8>> {
    static M: OnceLock<Mutex<HashMap<usize, u8>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

const FILE_SKIP_COMPLETION_PORT_ON_SUCCESS: u8 = 1;

pub(super) fn forget_modes(s: usize) {
    lock(modes()).remove(&s);
}

pub(super) fn set_modes(s: usize, flags: u8) {
    lock(modes()).insert(s, flags);
}

/// L'appel demande-t-il une E/S avec OVERLAPPED que l'on sait terminer ?
/// Une routine de complétion (APC) garde l'ancien chemin synchrone.
pub(super) fn wanted(overlapped: usize, completion: usize) -> bool {
    // OVERLAPPED x86 : champs de 4 octets, alignement 4 (#1408).
    let align = if crate::wow32::pointer_size() == 4 {
        4
    } else {
        std::mem::align_of::<Overlapped>()
    };
    overlapped != 0 && completion == 0 && overlapped.is_multiple_of(align)
}

/// Termine l'opération : OVERLAPPED, événement et paquet au port associé.
fn finish(s: usize, overlapped: usize, bytes: u32, error: i32) {
    if error != 0 {
        lock(errors()).insert(overlapped, error);
    } else {
        lock(errors()).remove(&overlapped);
    }
    crate::dlls::kernel32::complete_overlapped(s, overlapped, bytes, error as u32);
}

/// Étape d'une opération : `None` tant qu'il faut attendre la socket.
type Step = Box<dyn FnMut(i32) -> Option<(u32, i32)> + Send>;

/// Confie l'opération à un fil qui attend `events` sur la socket.
fn start(s: usize, overlapped: usize, events: i16, mut step: Step) -> i32 {
    // SAFETY : descripteur valide (valid_socket vérifié par l'appelant).
    let fd = unsafe { libc::fcntl(s as i32, libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    // SAFETY : appel système sans pointeur.
    let wake = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    if wake < 0 {
        // SAFETY : descripteur ouvert juste au-dessus.
        unsafe { libc::close(fd) };
        set_error(WSAENOBUFS);
        return SOCKET_ERROR;
    }
    // SAFETY : OVERLAPPED aligné fourni par l'appelant, valable jusqu'à la fin
    // de l'opération (contrat Win32).
    unsafe {
        crate::dlls::kernel32::ov_set(overlapped, STATUS_PENDING, 0);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(AtomicBool::new(false));
    lock(pending()).insert(
        id,
        Pending {
            socket: s,
            wake,
            cancelled: cancelled.clone(),
            overlapped,
        },
    );
    let spawned = std::thread::Builder::new()
        .name("winlux-wsa-io".into())
        .spawn(move || {
            let (bytes, error) = loop {
                if cancelled.load(Ordering::Acquire) {
                    break (0, WSA_OPERATION_ABORTED);
                }
                let mut fds = [
                    libc::pollfd {
                        fd,
                        events,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: wake,
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                // SAFETY : tableau de deux pollfd.
                let r = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                if r < 0 {
                    // SAFETY : errno du fil courant.
                    if unsafe { *libc::__errno_location() } == libc::EINTR {
                        continue;
                    }
                    break (0, last_error());
                }
                if cancelled.load(Ordering::Acquire) {
                    break (0, WSA_OPERATION_ABORTED);
                }
                if fds[0].revents == 0 {
                    continue;
                }
                if let Some(done) = step(fd) {
                    break done;
                }
            };
            // Le paquet part avant le retrait de la liste : closesocket attend
            // ainsi que l'annulation soit livrée avant d'oublier le port.
            finish(s, overlapped, bytes, error);
            lock(pending()).remove(&id);
            // SAFETY : descripteurs propres à cette opération.
            unsafe {
                libc::close(wake);
                libc::close(fd);
            }
        });
    if spawned.is_err() {
        if let Some(p) = lock(pending()).remove(&id) {
            // SAFETY : descripteurs ouverts plus haut, jamais confiés au fil.
            unsafe {
                libc::close(p.wake);
                libc::close(fd);
            }
        }
        set_error(WSAENOBUFS);
        return SOCKET_ERROR;
    }
    set_error(WSA_IO_PENDING);
    SOCKET_ERROR
}

/// Annule les opérations en cours de `s` (toutes si `overlapped` vaut 0).
/// Rend vrai si au moins une a été trouvée.
pub(crate) fn cancel(s: usize, overlapped: usize) -> bool {
    let list = lock(pending());
    let mut found = false;
    for p in list.values() {
        if p.socket == s && (overlapped == 0 || p.overlapped == overlapped) {
            p.cancelled.store(true, Ordering::Release);
            let one: u64 = 1;
            // SAFETY : eventfd encore ouvert tant que l'entrée est listée.
            unsafe { libc::write(p.wake, &one as *const u64 as *const libc::c_void, 8) };
            found = true;
        }
    }
    found
}

/// closesocket : annule ce qui attend, puis laisse aux paquets
/// d'annulation le temps de partir (au plus une seconde).
pub(super) fn close(s: usize) {
    lock(modes()).remove(&s);
    if !cancel(s, 0) {
        return;
    }
    let start = std::time::Instant::now();
    while lock(pending()).values().any(|p| p.socket == s)
        && start.elapsed() < std::time::Duration::from_secs(1)
    {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Copie (longueur, adresse) des WSABUF : le tableau lui-même n'a pas à
/// survivre à l'appel, contrairement aux tampons.
unsafe fn buffers(bufs: usize, count: u32) -> Vec<(usize, usize)> {
    (0..count as usize).map(|i| read_wsabuf(bufs, i)).collect()
}

fn errno() -> i32 {
    // SAFETY : errno du fil courant.
    unsafe { *libc::__errno_location() }
}

fn would_block(e: i32) -> bool {
    e == libc::EAGAIN || e == libc::EWOULDBLOCK || e == libc::EINTR
}

unsafe fn recv_now(fd: i32, list: &[(usize, usize)], flags: i32) -> Option<(u32, i32)> {
    let mut iovs: Vec<libc::iovec> = list
        .iter()
        .map(|&(len, buf)| libc::iovec {
            iov_base: buf as *mut libc::c_void,
            iov_len: len,
        })
        .collect();
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = iovs.as_mut_ptr();
    msg.msg_iovlen = iovs.len();
    let r = libc::recvmsg(fd, &mut msg, flags | libc::MSG_DONTWAIT);
    if r >= 0 {
        return Some((r as u32, 0));
    }
    let e = errno();
    if would_block(e) {
        None
    } else {
        Some((0, error_from_errno(e)))
    }
}

/// Envoie depuis `done` octets ; rend `None` si la socket est pleine.
unsafe fn send_now(
    fd: i32,
    list: &[(usize, usize)],
    done: &mut usize,
    flags: i32,
) -> Option<(u32, i32)> {
    let total: usize = list.iter().map(|b| b.0).sum();
    loop {
        if *done >= total {
            return Some((total as u32, 0));
        }
        let mut skip = *done;
        let mut iovs = Vec::new();
        for &(len, buf) in list {
            if skip >= len {
                skip -= len;
                continue;
            }
            iovs.push(libc::iovec {
                iov_base: (buf + skip) as *mut libc::c_void,
                iov_len: len - skip,
            });
            skip = 0;
        }
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = iovs.as_mut_ptr();
        msg.msg_iovlen = iovs.len();
        let r = libc::sendmsg(fd, &msg, flags | libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL);
        if r >= 0 {
            *done += r as usize;
            continue;
        }
        let e = errno();
        if would_block(e) {
            return None;
        }
        return Some((*done as u32, error_from_errno(e)));
    }
}

/// Résultat d'un essai immédiat : succès posté au port, échec rendu tout de
/// suite (sans paquet, comme sous Windows), sinon attente.
fn conclude(s: usize, overlapped: usize, now: Option<(u32, i32)>, out: *mut u32) -> Option<i32> {
    match now {
        Some((n, 0)) => {
            if !out.is_null() {
                // SAFETY : pointeur de sortie fourni par l'appelant.
                unsafe { *out = n };
            }
            let skip = lock(modes()).get(&s).copied().unwrap_or(0);
            if skip & FILE_SKIP_COMPLETION_PORT_ON_SUCCESS != 0 {
                // Fin immédiate : OVERLAPPED rempli, aucun paquet au port.
                lock(errors()).remove(&overlapped);
                crate::dlls::kernel32::complete_overlapped_inline(overlapped, n, skip & 2 == 0);
            } else {
                finish(s, overlapped, n, 0);
            }
            set_error(0);
            Some(0)
        }
        Some((_, e)) => {
            set_error(e);
            Some(SOCKET_ERROR)
        }
        None => None,
    }
}

/// WSARecv avec OVERLAPPED. Une réception de zéro octet attend que des
/// données arrivent (.NET s'en sert pour ne pas immobiliser de tampon).
pub(super) unsafe fn recv(
    s: usize,
    bufs: *mut WsaBuf,
    count: u32,
    received: *mut u32,
    flags: *mut u32,
    overlapped: usize,
) -> i32 {
    let list = buffers(bufs as usize, count);
    let fl = if flags.is_null() {
        0
    } else {
        let f = recv_flags(*flags as i32);
        *flags = 0;
        f
    };
    let empty = list.iter().all(|b| b.0 == 0);
    let attempt = move |fd: i32| -> Option<(u32, i32)> {
        if empty {
            return Some((0, 0));
        }
        // SAFETY : tampons valables jusqu'à la fin de l'opération.
        unsafe { recv_now(fd, &list, fl) }
    };
    let now = if empty {
        let mut p = libc::pollfd {
            fd: s as i32,
            events: libc::POLLIN,
            revents: 0,
        };
        if libc::poll(&mut p, 1, 0) > 0 {
            Some((0, 0))
        } else {
            None
        }
    } else {
        attempt(s as i32)
    };
    if let Some(rc) = conclude(s, overlapped, now, received) {
        return rc;
    }
    start(s, overlapped, libc::POLLIN, Box::new(attempt))
}

/// WSASend avec OVERLAPPED : l'opération ne se termine qu'une fois tout
/// envoyé, comme sur une socket Windows.
pub(super) unsafe fn send(
    s: usize,
    bufs: *const WsaBuf,
    count: u32,
    sent: *mut u32,
    flags: u32,
    overlapped: usize,
) -> i32 {
    let list = buffers(bufs as usize, count);
    let fl = (flags as i32) & libc::MSG_OOB;
    let mut done = 0usize;
    let now = send_now(s as i32, &list, &mut done, fl);
    if let Some(rc) = conclude(s, overlapped, now, sent) {
        return rc;
    }
    start(
        s,
        overlapped,
        libc::POLLOUT,
        // SAFETY : tampons valables jusqu'à la fin de l'opération.
        Box::new(move |fd| unsafe { send_now(fd, &list, &mut done, fl) }),
    )
}

/// Erreur en attente sur une socket après un connect non bloquant.
fn socket_error(fd: i32) -> i32 {
    let mut v: i32 = 0;
    let mut len = 4u32;
    // SAFETY : tampon de quatre octets.
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            &mut v as *mut i32 as *mut libc::c_void,
            &mut len,
        )
    };
    if rc < 0 {
        errno()
    } else {
        v
    }
}

/// ConnectEx (mswsock), rendu par WSAIoctl : connexion asynchrone, puis
/// envoi facultatif d'un premier tampon. Rend TRUE si tout est fait.
pub(super) unsafe extern "win64" fn connect_ex(
    s: usize,
    name: *const u8,
    len: i32,
    send_buf: *const u8,
    send_len: u32,
    sent: *mut u32,
    overlapped: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return 0;
    }
    if name.is_null() || len <= 0 {
        set_error(WSAEFAULT);
        return 0;
    }
    if !wanted(overlapped, 0) {
        set_error(WSAEINVAL);
        return 0;
    }
    if !network_allowed(name, len) {
        set_error(10013); // WSAEACCES : bloqué dans Paramètres
        return 0;
    }
    let addr = super::addr_in(name, len);
    let fd = s as i32;
    let flags = libc::fcntl(fd, libc::F_GETFL);
    if flags >= 0 && flags & libc::O_NONBLOCK == 0 {
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
    let rc = libc::connect(
        fd,
        &addr as *const _ as *const libc::sockaddr,
        len as libc::socklen_t,
    );
    let e = errno();
    if flags >= 0 && flags & libc::O_NONBLOCK == 0 {
        libc::fcntl(fd, libc::F_SETFL, flags);
    }
    let list = if send_buf.is_null() || send_len == 0 {
        Vec::new()
    } else {
        vec![(send_len as usize, send_buf as usize)]
    };
    let mut done = 0usize;
    let mut connected = false;
    let step = move |fd: i32| -> Option<(u32, i32)> {
        if !connected {
            let err = socket_error(fd);
            if err != 0 {
                return Some((0, error_from_errno(err)));
            }
            connected = true;
        }
        // SAFETY : tampon valable jusqu'à la fin de l'opération.
        unsafe { send_now(fd, &list, &mut done, 0) }
    };
    let mut step: Step = Box::new(step);
    if rc < 0 && e != libc::EINPROGRESS && e != libc::EAGAIN {
        set_error(error_from_errno(e));
        return 0;
    }
    let now = if rc == 0 { step(fd) } else { None };
    if let Some(rc) = conclude(s, overlapped, now, sent) {
        return i32::from(rc == 0);
    }
    start(s, overlapped, libc::POLLOUT, step);
    0
}

/// AcceptEx (mswsock) : accepte une connexion sur `listen` et la range
/// dans la socket `accept` déjà créée. Après les `data_len` premiers octets
/// reçus (attendus s'il en faut), `out` reçoit l'adresse locale puis
/// l'adresse distante, chacune dans un bloc de `local_len` / `remote_len`
/// octets (relues par GetAcceptExSockaddrs). Rend TRUE si tout est fait.
#[allow(clippy::too_many_arguments)]
pub(super) unsafe extern "win64" fn accept_ex(
    listen: usize,
    accept: usize,
    out: *mut u8,
    data_len: u32,
    local_len: u32,
    remote_len: u32,
    received: *mut u32,
    overlapped: usize,
) -> i32 {
    if !valid_socket(listen) || !valid_socket(accept) {
        set_error(WSAENOTSOCK);
        return 0;
    }
    if out.is_null() || local_len < 16 || remote_len < 16 {
        set_error(WSAEFAULT);
        return 0;
    }
    let target = accept as i32;
    let out = out as usize;
    let step = move |fd: i32| -> Option<(u32, i32)> {
        // SAFETY : descripteurs et tampon valables jusqu'à la fin de
        // l'opération (contrat de AcceptEx).
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            let r = libc::accept4(
                fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            );
            let e = errno();
            libc::fcntl(fd, libc::F_SETFL, flags);
            if r < 0 {
                return if would_block(e) {
                    None
                } else {
                    Some((0, error_from_errno(e)))
                };
            }
            let moved = libc::dup3(r, target, libc::O_CLOEXEC);
            libc::close(r);
            if moved < 0 {
                return Some((0, error_from_errno(errno())));
            }
            let mut got = 0u32;
            if data_len > 0 {
                let mut p = libc::pollfd {
                    fd: target,
                    events: libc::POLLIN,
                    revents: 0,
                };
                libc::poll(&mut p, 1, -1);
                let n = libc::recv(target, out as *mut libc::c_void, data_len as usize, 0);
                if n < 0 {
                    return Some((0, error_from_errno(errno())));
                }
                got = n as u32;
            }
            for (at, len, peer) in [
                (out + data_len as usize, local_len, false),
                (out + (data_len + local_len) as usize, remote_len, true),
            ] {
                std::ptr::write_bytes(at as *mut u8, 0, len as usize);
                let mut n = len as libc::socklen_t;
                let p = at as *mut libc::sockaddr;
                if peer {
                    libc::getpeername(target, p, &mut n);
                } else {
                    libc::getsockname(target, p, &mut n);
                }
                super::addr_out(at as *mut u8, n as usize);
            }
            Some((got, 0))
        }
    };
    let mut step: Step = Box::new(step);
    if !wanted(overlapped, 0) {
        // Sans OVERLAPPED : attente bloquante de la connexion.
        let mut p = libc::pollfd {
            fd: listen as i32,
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            libc::poll(&mut p, 1, -1);
            match step(listen as i32) {
                Some((n, 0)) => {
                    if !received.is_null() {
                        *received = n;
                    }
                    set_error(0);
                    return 1;
                }
                Some((_, e)) => {
                    set_error(e);
                    return 0;
                }
                None => continue,
            }
        }
    }
    let now = step(listen as i32);
    if let Some(rc) = conclude(listen, overlapped, now, received) {
        return i32::from(rc == 0);
    }
    start(listen, overlapped, libc::POLLIN, step);
    0
}

/// DisconnectEx (mswsock) : ferme la connexion ; la socket ne peut pas
/// resservir sous Linux, TF_REUSE_SOCKET est donc sans effet.
pub(super) unsafe extern "win64" fn disconnect_ex(
    s: usize,
    overlapped: usize,
    _flags: u32,
    _reserved: u32,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return 0;
    }
    libc::shutdown(s as i32, libc::SHUT_RDWR);
    if wanted(overlapped, 0) {
        conclude(s, overlapped, Some((0, 0)), std::ptr::null_mut());
    }
    set_error(0);
    1
}

/// TransmitFile : TF_DISCONNECT ferme l'envoi une fois tout parti ;
/// TF_REUSE_SOCKET est sans effet sous Linux, comme pour DisconnectEx.
const TF_DISCONNECT: u32 = 1;
/// Plafond d'un envoi par TransmitFile (2 147 483 646 octets, documenté).
const TRANSMIT_FILE_MAX: u64 = 0x7FFF_FFFE;

/// Lit le contenu à transmettre : `n` octets (0 : jusqu'à la fin), à
/// l'Offset de l'OVERLAPPED s'il y en a un, sinon à la position courante
/// du fichier, qui avance alors comme sous Windows.
unsafe fn transmit_content(fd: i32, n: u32, overlapped: usize) -> Result<Vec<u8>, i32> {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) < 0 {
        return Err(error_from_errno(errno()));
    }
    let mut at = if overlapped != 0 {
        // OVERLAPPED : Offset/OffsetHigh après Internal et InternalHigh.
        let base = overlapped + 2 * crate::wow32::pointer_size();
        let lo = std::ptr::read_unaligned(base as *const u32) as u64;
        let hi = std::ptr::read_unaligned((base + 4) as *const u32) as u64;
        Some(lo | hi << 32)
    } else {
        None
    };
    let pos = match at {
        Some(o) => o,
        None => libc::lseek(fd, 0, libc::SEEK_CUR).max(0) as u64,
    };
    let left = (st.st_size.max(0) as u64).saturating_sub(pos);
    let want = if n == 0 { left } else { (n as u64).min(left) };
    if want > TRANSMIT_FILE_MAX {
        return Err(WSAEINVAL);
    }
    let mut data = vec![0u8; want as usize];
    let mut got = 0usize;
    while got < data.len() {
        let p = data.as_mut_ptr().add(got) as *mut libc::c_void;
        let r = match at {
            Some(o) => libc::pread(fd, p, data.len() - got, o as libc::off_t),
            None => libc::read(fd, p, data.len() - got),
        };
        if r < 0 {
            let e = errno();
            if e == libc::EINTR {
                continue;
            }
            return Err(error_from_errno(e));
        }
        if r == 0 {
            break;
        }
        got += r as usize;
        at = at.map(|o| o + r as u64);
    }
    data.truncate(got);
    Ok(data)
}

/// TransmitFile (mswsock) : envoie l'en-tête et la fin de
/// TRANSMIT_FILE_BUFFERS autour du contenu de `file` (aucun fichier si
/// `file` vaut 0). Rend TRUE si tout est parti ; avec OVERLAPPED, FALSE et
/// WSA_IO_PENDING tant que la socket est pleine.
pub(super) unsafe extern "win64" fn transmit_file(
    s: usize,
    file: usize,
    n_bytes: u32,
    _per_send: u32,
    overlapped: usize,
    buffers: usize,
    flags: u32,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return 0;
    }
    let ov = if wanted(overlapped, 0) { overlapped } else { 0 };
    let content = if file == 0 {
        Vec::new()
    } else {
        let Some(fd) = crate::dlls::kernel32::fd_of_handle(file) else {
            set_error(WSAEINVAL);
            return 0;
        };
        match transmit_content(fd, n_bytes, ov) {
            Ok(v) => v,
            Err(e) => {
                set_error(e);
                return 0;
            }
        }
    };
    // TRANSMIT_FILE_BUFFERS : Head, HeadLength, Tail, TailLength, chaque
    // champ aligné sur la taille d'un pointeur du programme.
    let mut list: Vec<(usize, usize)> = Vec::new();
    let ps = crate::wow32::pointer_size();
    let head = |i: usize| -> (usize, usize) {
        let ptr = crate::wow32::read_ptr(buffers + 2 * i * ps);
        let len = std::ptr::read_unaligned((buffers + (2 * i + 1) * ps) as *const u32) as usize;
        (len, ptr)
    };
    let (h, t) = if buffers != 0 {
        (head(0), head(1))
    } else {
        ((0, 0), (0, 0))
    };
    if h.0 > 0 && h.1 != 0 {
        list.push(h);
    }
    let body = (content.len(), content.as_ptr() as usize);
    if body.0 > 0 {
        list.push(body);
    }
    if t.0 > 0 && t.1 != 0 {
        list.push(t);
    }
    let disconnect = flags & TF_DISCONNECT != 0;
    let mut done = 0usize;
    // Le contenu lu vit dans l'étape : le tampon reste valable tant que
    // l'envoi n'est pas fini.
    let mut step: Step = Box::new(move |fd| {
        let _keep = &content;
        let r = send_now(fd, &list, &mut done, 0);
        if disconnect && matches!(r, Some((_, 0))) {
            libc::shutdown(fd, libc::SHUT_WR);
        }
        r
    });
    if ov == 0 {
        // Sans OVERLAPPED : envoi bloquant jusqu'au bout.
        let mut p = libc::pollfd {
            fd: s as i32,
            events: libc::POLLOUT,
            revents: 0,
        };
        loop {
            match step(s as i32) {
                Some((_, 0)) => {
                    set_error(0);
                    return 1;
                }
                Some((_, e)) => {
                    set_error(e);
                    return 0;
                }
                None => {
                    libc::poll(&mut p, 1, -1);
                }
            }
        }
    }
    let now = step(s as i32);
    if let Some(rc) = conclude(s, ov, now, std::ptr::null_mut()) {
        return i32::from(rc == 0);
    }
    start(s, ov, libc::POLLOUT, step);
    0
}

/// GUID des fonctions d'extension de mswsock, tels qu'en mémoire.
const WSAID_ACCEPTEX: [u8; 16] = [
    0xF1, 0x7D, 0x36, 0xB5, 0xAC, 0xCB, 0xCF, 0x11, 0x95, 0xCA, 0x00, 0x80, 0x5F, 0x48, 0xA1, 0x92,
];
const WSAID_GETACCEPTEXSOCKADDRS: [u8; 16] = [
    0xF2, 0x7D, 0x36, 0xB5, 0xAC, 0xCB, 0xCF, 0x11, 0x95, 0xCA, 0x00, 0x80, 0x5F, 0x48, 0xA1, 0x92,
];
const WSAID_DISCONNECTEX: [u8; 16] = [
    0x11, 0x2E, 0xDA, 0x7F, 0x30, 0x86, 0x6F, 0x43, 0xA0, 0x31, 0xF5, 0x36, 0xA6, 0xEE, 0xC1, 0x57,
];
/// WSAID_WSARECVMSG {F689D7C8-6F1F-436B-8A53-E54FE351C322} (mswsock.h).
const WSAID_WSARECVMSG: [u8; 16] = [
    0xC8, 0xD7, 0x89, 0xF6, 0x1F, 0x6F, 0x6B, 0x43, 0x8A, 0x53, 0xE5, 0x4F, 0xE3, 0x51, 0xC3, 0x22,
];
/// WSAID_WSASENDMSG {A441E712-754F-43CA-84A7-0DEE44CF606D} (mswsock.h).
const WSAID_WSASENDMSG: [u8; 16] = [
    0x12, 0xE7, 0x41, 0xA4, 0x4F, 0x75, 0xCA, 0x43, 0x84, 0xA7, 0x0D, 0xEE, 0x44, 0xCF, 0x60, 0x6D,
];
const WSAID_TRANSMITFILE: [u8; 16] = [
    0xF0, 0x7D, 0x36, 0xB5, 0xAC, 0xCB, 0xCF, 0x11, 0x95, 0xCA, 0x00, 0x80, 0x5F, 0x48, 0xA1, 0x92,
];

/// Adresse appelable par le programme : la fonction elle-même en x64, une
/// passerelle stdcall de `args` arguments en x86 (#1408).
fn extension_address(guid: &[u8; 16]) -> Option<usize> {
    let (name, f, args): (&str, usize, u8) = if *guid == WSAID_CONNECTEX {
        ("ConnectEx", connect_ex as *const () as usize, 7)
    } else if *guid == WSAID_ACCEPTEX {
        ("AcceptEx", accept_ex as *const () as usize, 8)
    } else if *guid == WSAID_GETACCEPTEXSOCKADDRS {
        (
            "GetAcceptExSockaddrs",
            super::get_accept_ex_sockaddrs as *const () as usize,
            8,
        )
    } else if *guid == WSAID_DISCONNECTEX {
        ("DisconnectEx", disconnect_ex as *const () as usize, 4)
    } else if *guid == WSAID_WSARECVMSG {
        (
            "WSARecvMsg",
            super::msg::wsa_recv_msg as *const () as usize,
            5,
        )
    } else if *guid == WSAID_WSASENDMSG {
        (
            "WSASendMsg",
            super::msg::wsa_send_msg as *const () as usize,
            6,
        )
    } else if *guid == WSAID_TRANSMITFILE {
        ("TransmitFile", transmit_file as *const () as usize, 7)
    } else {
        return None;
    };
    if crate::wow32::pointer_size() == 8 {
        return Some(f);
    }
    static STUBS: OnceLock<Mutex<HashMap<&'static str, usize>>> = OnceLock::new();
    let mut stubs = lock(STUBS.get_or_init(|| Mutex::new(HashMap::new())));
    Some(
        *stubs
            .entry(name)
            .or_insert_with(|| crate::wow32::x86_stdcall(name, f, args) as usize),
    )
}

/// WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER) : ConnectEx, AcceptEx,
/// GetAcceptExSockaddrs, DisconnectEx, WSARecvMsg et WSASendMsg (#1629) ; la sortie est un pointeur de la
/// taille de ceux du programme (4 octets en x86, #1408).
pub(super) unsafe fn extension_function(
    in_buf: *const u8,
    in_len: u32,
    out_buf: *mut u8,
    out_len: u32,
    returned: *mut u32,
) -> i32 {
    let ps = crate::wow32::pointer_size();
    if in_buf.is_null() || in_len < 16 || out_buf.is_null() || (out_len as usize) < ps {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let mut guid = [0u8; 16];
    std::ptr::copy_nonoverlapping(in_buf, guid.as_mut_ptr(), 16);
    let Some(f) = extension_address(&guid) else {
        eprintln!("[ws2_32] WSAIoctl: fonction d'extension non fournie {guid:02x?}");
        set_error(WSAEINVAL);
        return SOCKET_ERROR;
    };
    crate::wow32::write_ptr(out_buf as usize, f);
    if !returned.is_null() {
        *returned = ps as u32;
    }
    0
}

/// Code Winsock d'un NTSTATUS rangé dans l'OVERLAPPED par une autre voie.
fn wsa_of_status(status: u32) -> i32 {
    match crate::dlls::ntdll::rtl_nt_status_to_dos_error(status as i32) {
        995 => WSA_OPERATION_ABORTED,
        1225 => 10061, // ERROR_CONNECTION_REFUSED
        1231 => 10051, // ERROR_NETWORK_UNREACHABLE
        1232 => 10065, // ERROR_HOST_UNREACHABLE
        1236 => 10053, // ERROR_CONNECTION_ABORTED
        121 => 10060,  // ERROR_SEM_TIMEOUT
        e @ 10000..=11999 => e as i32,
        _ => WSAEINVAL,
    }
}

/// WSAGetOverlappedResult : octets transférés et, en cas d'échec, le code
/// Winsock de l'opération (jamais SOCKET_ERROR brut).
pub(super) unsafe extern "win64" fn get_result(
    _s: usize,
    overlapped: *const u8,
    bytes: *mut u32,
    wait: i32,
    flags: *mut u32,
) -> i32 {
    if overlapped.is_null() || bytes.is_null() {
        set_error(WSAEFAULT);
        return 0;
    }
    let wide = crate::wow32::pointer_size() == 8;
    let read = |at: usize| -> usize {
        // SAFETY : OVERLAPPED fourni par l'appelant.
        unsafe {
            if wide {
                std::ptr::read_volatile((overlapped as *const usize).add(at))
            } else {
                std::ptr::read_volatile((overlapped as *const u32).add(at)) as usize
            }
        }
    };
    let mut status = read(0);
    while status == STATUS_PENDING {
        if wait == 0 {
            set_error(WSA_IO_INCOMPLETE);
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        status = read(0);
    }
    *bytes = read(1) as u32;
    if !flags.is_null() {
        *flags = 0;
    }
    if status == 0 {
        return 1;
    }
    let code = lock(errors())
        .get(&(overlapped as usize))
        .copied()
        .unwrap_or_else(|| wsa_of_status(status as u32));
    set_error(code);
    0
}
