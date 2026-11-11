//! `\Device\Afd` : sondage des sockets par IOCTL_AFD_POLL (#1766).
//!
//! Sous Windows, mio (la boucle d'événements de tokio, donc de presque tout
//! programme réseau écrit en Rust) n'utilise ni select ni WSAPoll : il
//! ouvre `\Device\Afd\Mio` par NtCreateFile, lie ce handle à un port de
//! complétion, puis demande l'état de chaque socket par NtDeviceIoControlFile
//! (IOCTL_AFD_POLL), une demande asynchrone qui se termine par un paquet au
//! port. Sans ce périphérique, toute connexion échouait (« Failed to open
//! \Device\Afd\Mio », Discord Updater).
//!
//! Ici, l'ouverture rend un handle synthétique ; chaque demande est confiée
//! à un fil qui attend les descripteurs Linux des sockets (poll) et un
//! eventfd d'annulation, puis écrit le résultat dans AFD_POLL_INFO et dans
//! l'IO_STATUS_BLOCK avant de poster le paquet (ApcContext en guise
//! d'OVERLAPPED), comme le fait le pilote. NtCancelIoFileEx termine la
//! demande avec STATUS_CANCELLED ; une socket fermée pendant l'attente est
//! rendue avec AFD_POLL_LOCAL_CLOSE.
//!
//! Sources : code source public de mio (src/sys/windows/afd.rs, licence
//! MIT) pour le code de l'IOCTL, les bits d'événements et la disposition
//! d'AFD_POLL_INFO, et de wepoll (licence BSD) qui décrit le même usage.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const IOCTL_AFD_POLL: u32 = 0x0001_2024;

pub const AFD_POLL_RECEIVE: u32 = 0x0001;
pub const AFD_POLL_RECEIVE_EXPEDITED: u32 = 0x0002;
pub const AFD_POLL_SEND: u32 = 0x0004;
pub const AFD_POLL_DISCONNECT: u32 = 0x0008;
pub const AFD_POLL_ABORT: u32 = 0x0010;
pub const AFD_POLL_LOCAL_CLOSE: u32 = 0x0020;
pub const AFD_POLL_ACCEPT: u32 = 0x0080;
pub const AFD_POLL_CONNECT_FAIL: u32 = 0x0100;

const STATUS_SUCCESS: u32 = 0;
const STATUS_PENDING: u32 = 0x103;
const STATUS_CANCELLED: u32 = 0xC000_0120;
const STATUS_NOT_FOUND: u32 = 0xC000_0225;
const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
const STATUS_NOT_SUPPORTED: u32 = 0xC000_00BB;
const STATUS_INSUFFICIENT_RESOURCES: u32 = 0xC000_009A;
const ERROR_OPERATION_ABORTED: u32 = 995;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Handles de `\Device\Afd` ouverts.
fn handles() -> &'static Mutex<HashSet<usize>> {
    static H: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashSet::new()))
}

struct Op {
    afd: usize,
    iosb: usize,
    sockets: Vec<usize>,
    wake: i32,
    cancelled: Arc<AtomicBool>,
    closed: Arc<Mutex<Vec<usize>>>,
}

fn ops() -> &'static Mutex<HashMap<u64, Op>> {
    static O: OnceLock<Mutex<HashMap<u64, Op>>> = OnceLock::new();
    O.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Le chemin NT désigne-t-il le pilote AFD (`\Device\Afd`, `\Device\Afd\…`) ?
pub fn is_afd_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p == "\\device\\afd" || p.starts_with("\\device\\afd\\")
}

pub fn is_afd(h: usize) -> bool {
    lock(handles()).contains(&h)
}

/// Écrit l'IO_STATUS_BLOCK à la disposition de l'appelant.
unsafe fn write_iosb(io: usize, status: u32, info: usize) {
    if io == 0 {
        return;
    }
    if crate::wow32::pointer_size() == 4 {
        (io as *mut u32).write_unaligned(status);
        (io as *mut u32).add(1).write_unaligned(info as u32);
    } else {
        (io as *mut u64).write_unaligned(u64::from(status));
        (io as *mut u64).add(1).write_unaligned(info as u64);
    }
}

/// NtCreateFile de `\Device\Afd…` : handle synthétique (adossé à
/// /dev/null pour avoir un vrai handle de fichier, que CloseHandle et
/// CreateIoCompletionPort connaissent).
pub unsafe fn open(handle: *mut usize, io: *mut c_void) -> i32 {
    let Ok(f) = std::fs::File::open("/dev/null") else {
        return STATUS_INSUFFICIENT_RESOURCES as i32;
    };
    let h = crate::dlls::kernel32::insert_file(f);
    lock(handles()).insert(h);
    *handle = h;
    write_iosb(io as usize, STATUS_SUCCESS, 1); // FILE_OPENED
    STATUS_SUCCESS as i32
}

/// Fermeture du handle : les demandes en cours sont annulées.
pub fn closed(h: usize) {
    if lock(handles()).remove(&h) {
        cancel_matching(|op| op.afd == h);
    }
}

/// Disposition d'AFD_POLL_INFO : en-tête (Timeout i64, NumberOfHandles,
/// Exclusive) de 16 octets, puis { Handle, Events, Status } par socket.
fn entry_size(ps: usize) -> usize {
    ps + 8
}

/// Bits AFD demandés → événements poll(2).
fn poll_events(requested: u32) -> i16 {
    let mut e = 0;
    if requested & (AFD_POLL_RECEIVE | AFD_POLL_ACCEPT) != 0 {
        e |= libc::POLLIN;
    }
    if requested & AFD_POLL_RECEIVE_EXPEDITED != 0 {
        e |= libc::POLLPRI;
    }
    if requested & AFD_POLL_SEND != 0 {
        e |= libc::POLLOUT;
    }
    if requested & AFD_POLL_DISCONNECT != 0 {
        e |= libc::POLLRDHUP;
    }
    e
}

/// Réponse poll(2) → bits AFD, limités à ceux demandés. `listening` : la
/// socket attend des connexions ; `connected` : elle a un pair (une erreur
/// avant toute connexion est un échec de connexion, après, un abandon).
fn afd_events(revents: i16, requested: u32, listening: bool, connected: bool) -> u32 {
    if revents & libc::POLLNVAL != 0 {
        return AFD_POLL_LOCAL_CLOSE & requested;
    }
    let mut e = 0;
    if revents & libc::POLLIN != 0 {
        e |= if listening {
            AFD_POLL_ACCEPT
        } else {
            AFD_POLL_RECEIVE
        };
    }
    if revents & libc::POLLPRI != 0 {
        e |= AFD_POLL_RECEIVE_EXPEDITED;
    }
    if revents & libc::POLLERR != 0 {
        e |= if connected {
            AFD_POLL_ABORT
        } else {
            AFD_POLL_CONNECT_FAIL
        };
    } else {
        if revents & libc::POLLOUT != 0 && (connected || listening) {
            e |= AFD_POLL_SEND;
        }
        if revents & libc::POLLRDHUP != 0 || (revents & libc::POLLHUP != 0 && connected) {
            e |= AFD_POLL_DISCONNECT;
        }
    }
    e & requested
}

fn is_listening(fd: i32) -> bool {
    let mut v: i32 = 0;
    let mut len = 4 as libc::socklen_t;
    // SAFETY : tampon de 4 octets.
    let r = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut v as *mut i32).cast(),
            &mut len,
        )
    };
    r == 0 && v != 0
}

fn is_connected(fd: i32) -> bool {
    // SAFETY : sockaddr_storage assez grand pour toute adresse.
    unsafe {
        let mut a: libc::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        libc::getpeername(fd, (&mut a as *mut libc::sockaddr_storage).cast(), &mut len) == 0
    }
}

/// Délai d'AFD_POLL_INFO.Timeout en millisecondes (-1 : sans limite). Un
/// délai négatif est relatif (unités de 100 ns) ; mio passe i64::MAX.
fn timeout_ms(timeout: i64) -> i64 {
    if timeout < 0 {
        timeout.unsigned_abs().div_ceil(10_000).min(i64::MAX as u64) as i64
    } else if timeout == 0 {
        0
    } else {
        -1
    }
}

/// NtDeviceIoControlFile sur un handle AFD ; `None` pour un autre handle.
#[allow(clippy::too_many_arguments)]
pub unsafe fn device_io_control(
    h: usize,
    event: usize,
    apc_ctx: usize,
    io: *mut c_void,
    code: u32,
    inb: *const u8,
    inl: u32,
    outb: *mut u8,
    outl: u32,
) -> Option<u32> {
    if !is_afd(h) {
        return None;
    }
    let io = io as usize;
    if code != IOCTL_AFD_POLL {
        write_iosb(io, STATUS_NOT_SUPPORTED, 0);
        return Some(STATUS_NOT_SUPPORTED);
    }
    let ps = crate::wow32::pointer_size();
    let stride = entry_size(ps);
    if inb.is_null() || outb.is_null() || (inl as usize) < 16 {
        write_iosb(io, STATUS_INVALID_PARAMETER, 0);
        return Some(STATUS_INVALID_PARAMETER);
    }
    let timeout = (inb as *const i64).read_unaligned();
    let count = (inb.add(8) as *const u32).read_unaligned() as usize;
    let exclusive = (inb.add(12) as *const u32).read_unaligned();
    let need = 16 + count * stride;
    if count == 0 || (inl as usize) < need || (outl as usize) < need {
        write_iosb(io, STATUS_INVALID_PARAMETER, 0);
        return Some(STATUS_INVALID_PARAMETER);
    }
    let mut wanted = Vec::with_capacity(count);
    for i in 0..count {
        let at = inb.add(16 + i * stride);
        let socket = if ps == 4 {
            (at as *const u32).read_unaligned() as usize
        } else {
            (at as *const u64).read_unaligned() as usize
        };
        let events = (at.add(ps) as *const u32).read_unaligned();
        wanted.push((socket, events));
    }
    // SAFETY : appel système sans pointeur.
    let wake = libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK);
    if wake < 0 {
        write_iosb(io, STATUS_INSUFFICIENT_RESOURCES, 0);
        return Some(STATUS_INSUFFICIENT_RESOURCES);
    }
    write_iosb(io, STATUS_PENDING, 0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(Mutex::new(Vec::new()));
    lock(ops()).insert(
        id,
        Op {
            afd: h,
            iosb: io,
            sockets: wanted.iter().map(|w| w.0).collect(),
            wake,
            cancelled: cancelled.clone(),
            closed: closed.clone(),
        },
    );
    let out = outb as usize;
    let spawned = std::thread::Builder::new()
        .name("winlux-afd-poll".into())
        .spawn(move || {
            let ready = wait(&wanted, timeout_ms(timeout), wake, &cancelled, &closed);
            // Retirée de la liste avant d'être rendue : une fois la
            // complétion postée, NtCancelIoFileEx doit répondre
            // STATUS_NOT_FOUND (#1836). L'eventfd n'est fermé qu'après.
            lock(ops()).remove(&id);
            let (status, n) = match &ready {
                None => (STATUS_CANCELLED, 0),
                Some(r) => (STATUS_SUCCESS, r.len()),
            };
            // SAFETY : tampons de l'appelant, valables jusqu'à la fin de la
            // demande (contrat de NtDeviceIoControlFile).
            unsafe {
                let o = out as *mut u8;
                (o as *mut i64).write_unaligned(timeout);
                (o.add(8) as *mut u32).write_unaligned(n as u32);
                (o.add(12) as *mut u32).write_unaligned(exclusive);
                for (i, &(socket, events)) in ready.iter().flatten().enumerate() {
                    let at = o.add(16 + i * stride);
                    if ps == 4 {
                        (at as *mut u32).write_unaligned(socket as u32);
                    } else {
                        (at as *mut u64).write_unaligned(socket as u64);
                    }
                    (at.add(ps) as *mut u32).write_unaligned(events);
                    (at.add(ps + 4) as *mut u32).write_unaligned(0);
                }
                let bytes = if status == STATUS_SUCCESS {
                    16 + n * stride
                } else {
                    0
                };
                write_iosb(io, status, bytes);
                if event != 0 {
                    crate::dlls::kernel32::set_event(event);
                }
                let error = if status == STATUS_SUCCESS {
                    0
                } else {
                    ERROR_OPERATION_ABORTED
                };
                crate::dlls::kernel32::post_completion(h, apc_ctx, bytes as u32, error);
            }
            // SAFETY : eventfd propre à cette demande.
            unsafe { libc::close(wake) };
        });
    if spawned.is_err() {
        lock(ops()).remove(&id);
        libc::close(wake);
        write_iosb(io, STATUS_INSUFFICIENT_RESOURCES, 0);
        return Some(STATUS_INSUFFICIENT_RESOURCES);
    }
    Some(STATUS_PENDING)
}

/// Attend qu'au moins une socket ait un événement demandé. `None` : demande
/// annulée ; liste vide : délai écoulé.
fn wait(
    wanted: &[(usize, u32)],
    timeout_ms: i64,
    wake: i32,
    cancelled: &AtomicBool,
    closed: &Mutex<Vec<usize>>,
) -> Option<Vec<(usize, u32)>> {
    let deadline = (timeout_ms >= 0)
        .then(|| std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64));
    let mut idle = false;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return None;
        }
        let gone = lock(closed).clone();
        let mut ready: Vec<(usize, u32)> = wanted
            .iter()
            .filter(|(s, ev)| gone.contains(s) && ev & AFD_POLL_LOCAL_CLOSE != 0)
            .map(|&(s, _)| (s, AFD_POLL_LOCAL_CLOSE))
            .collect();
        if !ready.is_empty() {
            return Some(ready);
        }
        let mut fds: Vec<libc::pollfd> = wanted
            .iter()
            .map(|&(s, ev)| libc::pollfd {
                fd: if gone.contains(&s) { -1 } else { s as i32 },
                events: poll_events(ev),
                revents: 0,
            })
            .collect();
        fds.push(libc::pollfd {
            fd: wake,
            events: libc::POLLIN,
            revents: 0,
        });
        let left = deadline.map(|d| {
            d.saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32
        });
        // Un état qui ne correspond à rien de demandé (pair parti alors que
        // seule l'écriture est surveillée…) reste vrai : on réessaie après
        // un court repos au lieu de tourner à vide.
        let slice = if idle { 20 } else { -1 };
        let t = match left {
            Some(l) if slice >= 0 => l.min(slice),
            Some(l) => l,
            None => slice,
        };
        if idle {
            std::thread::sleep(std::time::Duration::from_millis(t.max(0) as u64));
        }
        // SAFETY : tableau de pollfd local.
        let r = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, t) };
        if r < 0 {
            // SAFETY : errno du fil courant.
            if unsafe { *libc::__errno_location() } == libc::EINTR {
                continue;
            }
            return Some(Vec::new());
        }
        if cancelled.load(Ordering::Acquire) {
            return None;
        }
        idle = false;
        for (i, &(s, requested)) in wanted.iter().enumerate() {
            let rev = fds[i].revents;
            if rev == 0 {
                continue;
            }
            let fd = s as i32;
            let e = afd_events(rev, requested, is_listening(fd), is_connected(fd));
            if e != 0 {
                ready.push((s, e));
            } else {
                idle = true;
            }
        }
        if !ready.is_empty() {
            return Some(ready);
        }
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            return Some(Vec::new());
        }
    }
}

fn cancel_matching(f: impl Fn(&Op) -> bool) -> bool {
    let list = lock(ops());
    let mut found = false;
    for op in list.values().filter(|op| f(op)) {
        op.cancelled.store(true, Ordering::Release);
        let one: u64 = 1;
        // SAFETY : eventfd ouvert tant que l'entrée est listée.
        unsafe { libc::write(op.wake, (&one as *const u64).cast(), 8) };
        found = true;
    }
    found
}

/// NtCancelIoFileEx / NtCancelIoFile sur un handle AFD (`iosb` nul : toutes
/// ses demandes) ; `None` pour un autre handle.
pub unsafe fn cancel(h: usize, iosb: usize, out: *mut c_void) -> Option<u32> {
    if !is_afd(h) {
        return None;
    }
    let found = cancel_matching(|op| op.afd == h && (iosb == 0 || op.iosb == iosb));
    let status = if found {
        STATUS_SUCCESS
    } else {
        STATUS_NOT_FOUND
    };
    write_iosb(out as usize, status, 0);
    Some(status)
}

/// closesocket : les demandes qui surveillent `s` la rendent avec
/// AFD_POLL_LOCAL_CLOSE, avant que son descripteur ne soit réutilisé.
pub fn socket_closed(s: usize) {
    let list = lock(ops());
    for op in list.values().filter(|op| op.sockets.contains(&s)) {
        lock(&op.closed).push(s);
        let one: u64 = 1;
        // SAFETY : eventfd ouvert tant que l'entrée est listée.
        unsafe { libc::write(op.wake, (&one as *const u64).cast(), 8) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evenements_afd() {
        let all = 0x1ff;
        assert_eq!(afd_events(libc::POLLIN, all, false, true), AFD_POLL_RECEIVE);
        assert_eq!(afd_events(libc::POLLIN, all, true, false), AFD_POLL_ACCEPT);
        assert_eq!(afd_events(libc::POLLOUT, all, false, true), AFD_POLL_SEND);
        assert_eq!(
            afd_events(
                libc::POLLERR | libc::POLLHUP | libc::POLLOUT,
                all,
                false,
                false
            ),
            AFD_POLL_CONNECT_FAIL
        );
        assert_eq!(
            afd_events(libc::POLLERR | libc::POLLHUP, all, false, true),
            AFD_POLL_ABORT
        );
        assert_eq!(
            afd_events(libc::POLLIN | libc::POLLRDHUP, all, false, true),
            AFD_POLL_RECEIVE | AFD_POLL_DISCONNECT
        );
        // Seuls les bits demandés reviennent.
        assert_eq!(afd_events(libc::POLLIN, AFD_POLL_SEND, false, true), 0);
        assert_eq!(
            poll_events(AFD_POLL_RECEIVE | AFD_POLL_SEND),
            libc::POLLIN | libc::POLLOUT
        );
        assert_eq!(timeout_ms(i64::MAX), -1);
        assert_eq!(timeout_ms(-10_000_000), 1000);
        assert!(is_afd_path("\\Device\\Afd\\Mio"));
        assert!(is_afd_path("\\device\\afd"));
        assert!(!is_afd_path("\\Device\\Afdx"));
    }

    /// Le parcours de mio : ouverture, port, sondage d'une socket qui
    /// devient lisible, paquet au port, puis annulation d'un second sondage.
    #[test]
    fn sondage_comme_mio() {
        use crate::dlls::kernel32::{create_io_completion_port, get_queued_completion_status_ex};
        unsafe {
            let mut afd = 0usize;
            let mut iosb = [0u64; 2];
            assert_eq!(open(&mut afd, iosb.as_mut_ptr().cast()), 0);
            let port = create_io_completion_port(afd, 0, 7, 0);
            assert_ne!(port, 0);
            let mut pair = [0i32; 2];
            assert_eq!(
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, pair.as_mut_ptr()),
                0
            );
            let mut info = [0u8; 32];
            (info.as_mut_ptr() as *mut i64).write_unaligned(i64::MAX);
            (info.as_mut_ptr().add(8) as *mut u32).write_unaligned(1);
            (info.as_mut_ptr().add(16) as *mut u64).write_unaligned(pair[0] as u64);
            (info.as_mut_ptr().add(24) as *mut u32)
                .write_unaligned(AFD_POLL_RECEIVE | AFD_POLL_LOCAL_CLOSE);
            let ctx = 0x1234usize;
            let p = info.as_mut_ptr();
            assert_eq!(
                device_io_control(
                    afd,
                    0,
                    ctx,
                    iosb.as_mut_ptr().cast(),
                    IOCTL_AFD_POLL,
                    p,
                    32,
                    p,
                    32
                ),
                Some(STATUS_PENDING)
            );
            assert_eq!(iosb[0], u64::from(STATUS_PENDING));
            assert_eq!(libc::write(pair[1], b"x".as_ptr().cast(), 1), 1);
            let mut entries: [crate::dlls::kernel32::OverlappedEntry; 1] = std::mem::zeroed();
            let mut removed = 0u32;
            assert_eq!(
                get_queued_completion_status_ex(
                    port,
                    entries.as_mut_ptr(),
                    1,
                    &mut removed,
                    5000,
                    0
                ),
                1
            );
            assert_eq!(removed, 1);
            assert_eq!(entries[0].completion_key, 7);
            assert_eq!(entries[0].overlapped as usize, ctx);
            assert_eq!(iosb[0], 0);
            assert_eq!((info.as_ptr().add(8) as *const u32).read_unaligned(), 1);
            assert_eq!(
                (info.as_ptr().add(24) as *const u32).read_unaligned(),
                AFD_POLL_RECEIVE
            );
            // Second sondage, sur l'écriture seule d'une socket vide côté
            // lecture : annulé.
            let mut info2 = info;
            (info2.as_mut_ptr().add(24) as *mut u32).write_unaligned(AFD_POLL_ACCEPT);
            let p2 = info2.as_mut_ptr();
            let mut iosb2 = [0u64; 2];
            assert_eq!(
                device_io_control(
                    afd,
                    0,
                    ctx + 1,
                    iosb2.as_mut_ptr().cast(),
                    IOCTL_AFD_POLL,
                    p2,
                    32,
                    p2,
                    32
                ),
                Some(STATUS_PENDING)
            );
            let mut c = [0u64; 2];
            assert_eq!(
                cancel(afd, iosb2.as_mut_ptr() as usize, c.as_mut_ptr().cast()),
                Some(STATUS_SUCCESS)
            );
            assert_eq!(
                get_queued_completion_status_ex(
                    port,
                    entries.as_mut_ptr(),
                    1,
                    &mut removed,
                    5000,
                    0
                ),
                1
            );
            assert_eq!(entries[0].overlapped as usize, ctx + 1);
            assert_eq!(iosb2[0], u64::from(STATUS_CANCELLED));
            assert_eq!(
                cancel(afd, iosb2.as_mut_ptr() as usize, c.as_mut_ptr().cast()),
                Some(STATUS_NOT_FOUND)
            );
            libc::close(pair[0]);
            libc::close(pair[1]);
            closed(afd);
        }
    }
}
