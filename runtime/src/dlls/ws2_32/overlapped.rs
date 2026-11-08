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

/// L'appel demande-t-il une E/S avec OVERLAPPED que l'on sait terminer ?
/// Une routine de complétion (APC) garde l'ancien chemin synchrone.
pub(super) fn wanted(overlapped: usize, completion: usize) -> bool {
    overlapped != 0
        && completion == 0
        && crate::wow32::pointer_size() == 8
        && overlapped.is_multiple_of(std::mem::align_of::<Overlapped>())
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
        let ov = overlapped as *mut Overlapped;
        std::ptr::write_volatile(&mut (*ov).internal, STATUS_PENDING);
        std::ptr::write_volatile(&mut (*ov).internal_high, 0);
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
            finish(s, overlapped, n, 0);
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

/// WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER) : seul ConnectEx est fourni.
pub(super) unsafe fn extension_function(
    in_buf: *const u8,
    in_len: u32,
    out_buf: *mut u8,
    out_len: u32,
    returned: *mut u32,
) -> i32 {
    if in_buf.is_null() || in_len < 16 || out_buf.is_null() || (out_len as usize) < 8 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let mut guid = [0u8; 16];
    std::ptr::copy_nonoverlapping(in_buf, guid.as_mut_ptr(), 16);
    if guid != WSAID_CONNECTEX || crate::wow32::pointer_size() != 8 {
        eprintln!("[ws2_32] WSAIoctl: fonction d'extension non fournie {guid:02x?}");
        set_error(WSAEINVAL);
        return SOCKET_ERROR;
    }
    std::ptr::write_unaligned(out_buf as *mut usize, connect_ex as *const () as usize);
    if !returned.is_null() {
        *returned = 8;
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
