//! Passerelle Winsock vers les sockets POSIX/Linux, avec codes d'erreur WSA.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

mod overlapped;

#[link(name = "c")]
unsafe extern "C" {
    #[link_name = "inet_pton"]
    fn posix_inet_pton(af: i32, src: *const c_char, dst: *mut c_void) -> i32;
    #[link_name = "inet_ntop"]
    fn posix_inet_ntop(af: i32, src: *const c_void, dst: *mut c_char, size: u32) -> *const c_char;
}

const WSA_NOT_INITIALISED: i32 = 10093;
const WSA_INVALID_PARAMETER: i32 = 10022;
const WSAENOTSOCK: i32 = 10038;
const WSAEFAULT: i32 = 10014;
const WSAEINVAL: i32 = 10022;
const WSAENOBUFS: i32 = 10055;
const WSAEOPNOTSUPP: i32 = 10045;
const WSAEWOULDBLOCK: i32 = 10035;
const WSAENOPROTOOPT: i32 = 10042;
const SOCKET_ERROR: i32 = -1;
const WSAHOST_NOT_FOUND: i32 = 11001;
const WSATRY_AGAIN: i32 = 11002;
const WSANO_RECOVERY: i32 = 11003;
const WSATYPE_NOT_FOUND: i32 = 10109;
const INVALID_SOCKET: usize = usize::MAX;

// ioctlsocket commands
const FIONBIO: u32 = 0x8004667E;
const FIONREAD: u32 = 0x4004667F;
const SIOCATMARK: u32 = 0x40047307;

// SOL_SOCKET option translations
const WIN_SOL_SOCKET: i32 = 0xFFFF;
const WIN_SO_DEBUG: i32 = 0x0001;
const WIN_SO_REUSEADDR: i32 = 0x0004;
const WIN_SO_KEEPALIVE: i32 = 0x0008;
const WIN_SO_DONTROUTE: i32 = 0x0010;
const WIN_SO_BROADCAST: i32 = 0x0020;
const WIN_SO_LINGER: i32 = 0x0080;
const WIN_SO_OOBINLINE: i32 = 0x0100;
const WIN_SO_SNDBUF: i32 = 0x1001;
const WIN_SO_RCVBUF: i32 = 0x1002;
const WIN_SO_SNDTIMEO: i32 = 0x1005;
const WIN_SO_RCVTIMEO: i32 = 0x1006;
const WIN_SO_ERROR: i32 = 0x1007;
const WIN_SO_TYPE: i32 = 0x1008;

const WIN_TCP_NODELAY: i32 = 0x0001;
const WIN_IPPROTO_TCP: i32 = 6;
const WIN_IPPROTO_IP: i32 = 0;
/// `AF_INET6` vaut 23 sous Windows et 10 sous Linux ; les structures
/// `sockaddr_in6` ont la même forme, seule la famille change.
const WIN_AF_INET6: i32 = 23;

fn af_in(af: i32) -> i32 {
    if af == WIN_AF_INET6 {
        libc::AF_INET6
    } else {
        af
    }
}

fn af_out(af: i32) -> i32 {
    if af == libc::AF_INET6 {
        WIN_AF_INET6
    } else {
        af
    }
}

/// Copie d'une adresse donnée par l'appli, avec la famille Linux.
unsafe fn addr_in(name: *const u8, len: i32) -> libc::sockaddr_storage {
    let mut st: libc::sockaddr_storage = std::mem::zeroed();
    let n = (len.max(0) as usize).min(std::mem::size_of::<libc::sockaddr_storage>());
    if !name.is_null() && n >= 2 {
        std::ptr::copy_nonoverlapping(name, &mut st as *mut _ as *mut u8, n);
        st.ss_family = af_in(st.ss_family as i32) as u16;
    }
    st
}

/// Remet la famille Windows dans une adresse rendue à l'appli.
unsafe fn addr_out(name: *mut u8, len: usize) {
    if !name.is_null() && len >= 2 {
        let f = std::ptr::read_unaligned(name as *const u16);
        std::ptr::write_unaligned(name as *mut u16, af_out(f as i32) as u16);
    }
}
const WIN_IPPROTO_IPV6: i32 = 41;
/// `SO_EXCLUSIVEADDRUSE` (`~SO_REUSEADDR`) : un port lié sous Linux est déjà
/// exclusif tant que personne ne demande `SO_REUSEADDR`.
const WIN_SO_EXCLUSIVEADDRUSE: i32 = !WIN_SO_REUSEADDR;
// Drapeaux WSAPoll (Windows) ↔ poll (Linux) : les valeurs diffèrent.
const WIN_POLLRDNORM: i16 = 0x0100;
const WIN_POLLRDBAND: i16 = 0x0200;
const WIN_POLLPRI: i16 = 0x0400;
const WIN_POLLWRNORM: i16 = 0x0010;
const WIN_POLLWRBAND: i16 = 0x0020;
const WIN_POLLERR: i16 = 0x0001;
const WIN_POLLHUP: i16 = 0x0002;
const WIN_POLLNVAL: i16 = 0x0004;
fn poll_events_to_linux(w: i16) -> i16 {
    let mut e = 0i16;
    if w & WIN_POLLRDNORM != 0 {
        e |= libc::POLLIN | libc::POLLRDNORM;
    }
    if w & WIN_POLLRDBAND != 0 {
        e |= libc::POLLRDBAND;
    }
    if w & WIN_POLLPRI != 0 {
        e |= libc::POLLPRI;
    }
    if w & WIN_POLLWRNORM != 0 {
        e |= libc::POLLOUT | libc::POLLWRNORM;
    }
    if w & WIN_POLLWRBAND != 0 {
        e |= libc::POLLWRBAND;
    }
    e
}
fn poll_events_to_win(l: i16) -> i16 {
    let mut e = 0i16;
    if l & (libc::POLLIN | libc::POLLRDNORM) != 0 {
        e |= WIN_POLLRDNORM;
    }
    if l & libc::POLLRDBAND != 0 {
        e |= WIN_POLLRDBAND;
    }
    if l & libc::POLLPRI != 0 {
        e |= WIN_POLLPRI;
    }
    if l & (libc::POLLOUT | libc::POLLWRNORM) != 0 {
        e |= WIN_POLLWRNORM;
    }
    if l & libc::POLLWRBAND != 0 {
        e |= WIN_POLLWRBAND;
    }
    if l & libc::POLLERR != 0 {
        e |= WIN_POLLERR;
    }
    if l & libc::POLLHUP != 0 {
        e |= WIN_POLLHUP;
    }
    if l & libc::POLLNVAL != 0 {
        e |= WIN_POLLNVAL;
    }
    e
}

// WSA event constants
const WSA_INVALID_EVENT: usize = 0;
const WSA_MAXIMUM_WAIT_EVENTS: u32 = 64;
const WSA_WAIT_TIMEOUT: u32 = 0x102;
const WSA_WAIT_FAILED: u32 = u32::MAX;
const WSA_WAIT_EVENT_0: u32 = 0;

// Network event bits
const FD_READ: i32 = 0x01;
const FD_WRITE: i32 = 0x02;
const FD_OOB: i32 = 0x04;
const FD_ACCEPT: i32 = 0x08;
const FD_CONNECT: i32 = 0x10;
const FD_CLOSE: i32 = 0x20;
// Indices de WSANETWORKEVENTS.iErrorCode : FD_READ_BIT 0, FD_WRITE_BIT 1,
// FD_OOB_BIT 2, FD_ACCEPT_BIT 3, FD_CONNECT_BIT 4, FD_CLOSE_BIT 5.
const FD_CONNECT_BIT: usize = 4;
const FD_CLOSE_BIT: usize = 5;

/// La socket écoute-t-elle (listen) ?
fn socket_listening(s: usize) -> bool {
    let mut on = 0i32;
    let mut len = 4 as libc::socklen_t;
    // SAFETY : tampon de 4 octets à nous.
    let rc = unsafe {
        libc::getsockopt(
            s as i32,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            &mut on as *mut i32 as *mut libc::c_void,
            &mut len,
        )
    };
    rc == 0 && on != 0
}

// NI flags
const NI_NUMERICHOST: i32 = 1;
const NI_NUMERICSERV: i32 = 2;
const NI_NOFQDN: i32 = 4;
const NI_NAMEREQD: i32 = 8;
const NI_DGRAM: i32 = 16;
static STARTUPS: AtomicUsize = AtomicUsize::new(0);
static SOCKETS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
#[repr(C)]
struct WinAddrInfoA {
    flags: i32,
    family: i32,
    socktype: i32,
    protocol: i32,
    addrlen: usize,
    canonname: *mut c_char,
    addr: *mut u8,
    next: *mut WinAddrInfoA,
}
#[repr(C)]
struct WinAddrInfoW {
    flags: i32,
    family: i32,
    socktype: i32,
    protocol: i32,
    addrlen: usize,
    canonname: *mut u16,
    addr: *mut u8,
    next: *mut WinAddrInfoW,
}
enum AddrInfoAllocation {
    A {
        nodes: Vec<usize>,
        addrs: Vec<usize>,
        names: Vec<usize>,
    },
    W {
        nodes: Vec<usize>,
        addrs: Vec<usize>,
        names: Vec<(usize, usize)>,
    },
}
static ADDRINFO_ALLOCS: OnceLock<Mutex<HashMap<usize, AddrInfoAllocation>>> = OnceLock::new();
thread_local! {static LAST_WSA_ERROR: Cell<i32> = const { Cell::new(0) };}

#[repr(C)]
#[derive(Default, Clone)]
pub(crate) struct WsaNetworkEvents {
    network_events: i32,
    error_code: [i32; 10],
}

struct EventState {
    fd: i32,
    triggered: bool,
}

static EVENTS: OnceLock<Mutex<HashMap<usize, EventState>>> = OnceLock::new();
fn events() -> &'static Mutex<HashMap<usize, EventState>> {
    EVENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Socket → (événement, masque FD_*, FD_WRITE/FD_CONNECT armés). Sous
/// Windows FD_WRITE et FD_CONNECT sont signalés une seule fois (après la
/// connexion ou après un envoi en WSAEWOULDBLOCK), pas à chaque fois que la
/// socket est écrivable : sinon l'attente ne bloquerait jamais.
#[derive(Clone, Copy)]
struct EventAssoc {
    event: usize,
    mask: i32,
    write_armed: bool,
}
static EVENT_ASSOC: OnceLock<Mutex<HashMap<usize, EventAssoc>>> = OnceLock::new();
fn event_assoc() -> &'static Mutex<HashMap<usize, EventAssoc>> {
    EVENT_ASSOC.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Réarme FD_WRITE après un envoi refusé (EWOULDBLOCK) sur une socket suivie.
fn rearm_write(s: usize) {
    if let Some(a) = lock(event_assoc()).get_mut(&s) {
        a.write_armed = true;
    }
    async_arm(s, FD_WRITE);
}

// ── WSAAsyncSelect : notifications par message de fenêtre ───────────────
//
// PuTTY (et beaucoup de programmes anciens) demande à être prévenu par un
// message posté à sa fenêtre (wParam = socket, lParam = événement | erreur
// << 16). Comme sous Windows, chaque événement est envoyé une fois puis
// réactivé par l'appel qui le consomme : recv pour FD_READ, accept pour
// FD_ACCEPT, un envoi refusé (WSAEWOULDBLOCK) pour FD_WRITE. FD_CONNECT et
// FD_CLOSE ne partent qu'une fois. Les files de messages interrogent les
// sockets (`pump_async`) à chaque PeekMessage / GetMessage.

#[derive(Clone, Copy)]
struct AsyncSel {
    hwnd: usize,
    msg: u32,
    mask: i32,
    /// Événements qui peuvent encore être postés.
    armed: i32,
    /// connect() appelé : FD_CONNECT attendu.
    connecting: bool,
}

fn async_sel() -> &'static Mutex<HashMap<usize, AsyncSel>> {
    static A: OnceLock<Mutex<HashMap<usize, AsyncSel>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(HashMap::new()))
}

fn async_arm(s: usize, bits: i32) {
    if let Some(a) = lock(async_sel()).get_mut(&s) {
        a.armed |= bits & a.mask;
    }
}

fn async_connecting(s: usize) {
    if let Some(a) = lock(async_sel()).get_mut(&s) {
        a.connecting = true;
        a.armed |= a.mask & (FD_CONNECT | FD_WRITE);
    }
}

/// Des sockets attendent-elles une notification par message ?
pub(crate) fn async_active() -> bool {
    !lock(async_sel()).is_empty()
}

fn sock_flag(s: i32, opt: i32) -> i32 {
    let mut v: i32 = 0;
    let mut len = 4u32;
    // SAFETY : tampon de 4 octets valide.
    unsafe {
        libc::getsockopt(
            s,
            libc::SOL_SOCKET,
            opt,
            &mut v as *mut i32 as *mut c_void,
            &mut len,
        );
    }
    v
}

fn is_connected(s: i32) -> bool {
    // SAFETY : adresse de taille suffisante pour IPv4 et IPv6.
    unsafe {
        let mut addr: libc::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        libc::getpeername(s, &mut addr as *mut _ as *mut libc::sockaddr, &mut len) == 0
    }
}

/// Événements prêts pour une socket enregistrée par WSAAsyncSelect, dans
/// l'ordre de Windows : (événement, erreur Winsock).
fn async_ready(s: usize, a: &mut AsyncSel) -> Vec<(i32, i32)> {
    let fd = s as i32;
    let listening = sock_flag(fd, libc::SO_ACCEPTCONN) != 0;
    if !listening && !a.connecting && !is_connected(fd) {
        // Socket pas encore connectée : Linux la dit « prête » à tort.
        return Vec::new();
    }
    let mut events = 0i16;
    if a.armed & (FD_READ | FD_ACCEPT | FD_CLOSE | FD_OOB) != 0 {
        events |= libc::POLLIN | libc::POLLPRI;
    }
    if a.armed & (FD_WRITE | FD_CONNECT) != 0 {
        events |= libc::POLLOUT;
    }
    if events == 0 {
        return Vec::new();
    }
    let mut p = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    // SAFETY : un seul pollfd, attente nulle.
    if unsafe { libc::poll(&mut p, 1, 0) } <= 0 {
        return Vec::new();
    }
    let r = p.revents;
    let mut out = Vec::new();
    let take = |a: &mut AsyncSel, bit: i32| {
        let ok = a.armed & bit != 0;
        a.armed &= !bit;
        ok
    };
    if a.connecting && r & (libc::POLLOUT | libc::POLLERR | libc::POLLHUP) != 0 {
        a.connecting = false;
        let err = sock_flag(fd, libc::SO_ERROR);
        if take(a, FD_CONNECT) {
            out.push((FD_CONNECT, if err != 0 { error_from_errno(err) } else { 0 }));
        }
        if err != 0 {
            a.armed = 0;
            return out;
        }
    }
    if listening {
        if r & libc::POLLIN != 0 && take(a, FD_ACCEPT) {
            out.push((FD_ACCEPT, 0));
        }
        return out;
    }
    if r & libc::POLLPRI != 0 && take(a, FD_OOB) {
        out.push((FD_OOB, 0));
    }
    if r & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
        let mut b = 0u8;
        // SAFETY : un octet lu sans le retirer de la file.
        let n = unsafe {
            libc::recv(
                fd,
                &mut b as *mut u8 as *mut c_void,
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        if n > 0 {
            if take(a, FD_READ) {
                out.push((FD_READ, 0));
            }
        } else if n == 0 || r & (libc::POLLHUP | libc::POLLERR) != 0 {
            let err = if n < 0 {
                error_from_errno(unsafe { *libc::__errno_location() })
            } else {
                0
            };
            let err = if err == WSAEWOULDBLOCK { 0 } else { err };
            if take(a, FD_CLOSE) {
                out.push((FD_CLOSE, err));
                a.armed &= !(FD_READ | FD_WRITE);
            }
        }
    }
    if r & libc::POLLOUT != 0 && r & libc::POLLHUP == 0 && take(a, FD_WRITE) {
        out.push((FD_WRITE, 0));
    }
    out
}

/// Poste les notifications WSAAsyncSelect prêtes. Renvoie vrai si au moins
/// un message est parti.
pub(crate) fn pump_async() -> bool {
    let mut posts = Vec::new();
    {
        let mut map = lock(async_sel());
        if map.is_empty() {
            return false;
        }
        for (&s, a) in map.iter_mut() {
            for (ev, err) in async_ready(s, a) {
                posts.push((a.hwnd, a.msg, s, ((err as u32) << 16 | ev as u32) as isize));
            }
        }
    }
    let any = !posts.is_empty();
    for (hwnd, msg, s, lp) in posts {
        crate::dlls::gui::post_to_window(hwnd, crate::dlls::gui::Msg::new(hwnd, msg, s, lp));
    }
    any
}
/// Événements poll à surveiller pour une socket associée par WSAEventSelect.
fn assoc_poll_events(a: &EventAssoc) -> i16 {
    let mut e = 0i16;
    if a.mask & (FD_READ | FD_ACCEPT | FD_CLOSE) != 0 {
        e |= libc::POLLIN;
    }
    if a.write_armed && a.mask & (FD_WRITE | FD_CONNECT) != 0 {
        e |= libc::POLLOUT;
    }
    if a.mask & FD_OOB != 0 {
        e |= libc::POLLPRI;
    }
    e
}

/// Le handle est-il un événement créé par WSACreateEvent ?
pub(crate) fn is_wsa_event(h: usize) -> bool {
    lock(events()).contains_key(&h)
}

/// État d'un handle attendu par MsgWaitForMultipleObjects : un événement
/// WSACreateEvent signalé, ou une socket associée au handle par
/// WSAEventSelect (même un événement CreateEvent, comme PuTTY) qui est prête.
/// Sous Windows la pile réseau signale l'événement ; ici on interroge les
/// sockets au moment de l'attente.
/// Le handle est-il l'événement d'au moins une socket (WSAEventSelect) ?
/// Les attentes de kernel32 l'interrogent alors elles-mêmes (#1340).
pub(crate) fn has_socket_assoc(h: usize) -> bool {
    h != 0 && lock(event_assoc()).values().any(|a| a.event == h)
}

pub(crate) fn handle_ready(h: usize) -> bool {
    {
        let ev = lock(events());
        if let Some(st) = ev.get(&h) {
            if st.triggered {
                return true;
            }
            let mut p = libc::pollfd {
                fd: st.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY : un seul pollfd valide, attente nulle.
            if unsafe { libc::poll(&mut p, 1, 0) } > 0 && p.revents & libc::POLLIN != 0 {
                return true;
            }
        }
    }
    let mut pfds: Vec<libc::pollfd> = lock(event_assoc())
        .iter()
        .filter(|(_, a)| a.event == h)
        .map(|(&sock, a)| libc::pollfd {
            fd: sock as i32,
            events: assoc_poll_events(a),
            revents: 0,
        })
        .filter(|p| p.events != 0)
        .collect();
    if pfds.is_empty() {
        return false;
    }
    // SAFETY : `pfds` est un tableau valide pour la durée de l'appel.
    let rc = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, 0) };
    rc > 0
        && pfds
            .iter()
            .any(|p| p.revents & (p.events | libc::POLLHUP | libc::POLLERR) != 0)
}

#[repr(C)]
pub(crate) struct WsaBuf {
    len: u32,
    buf: *mut u8,
}

/// WSABUF n° `i` : `{ ULONG len; CHAR *buf; }`, 8 octets en x86 (16 en
/// x64, pointeur aligné sur 8).
unsafe fn read_wsabuf(bufs: usize, i: usize) -> (usize, usize) {
    if crate::wow32::pointer_size() == 4 {
        let at = bufs + i * 8;
        (
            std::ptr::read_unaligned(at as *const u32) as usize,
            std::ptr::read_unaligned((at + 4) as *const u32) as usize,
        )
    } else {
        let b = std::ptr::read_unaligned((bufs as *const WsaBuf).add(i));
        (b.len as usize, b.buf as usize)
    }
}

#[repr(C)]
struct WinLinger {
    onoff: u16,
    linger: u16,
}

#[repr(C)]
pub(crate) struct WinFdSet {
    count: u32,
    // followed by count SOCKET values (usize each)
}

#[repr(C)]
pub(crate) struct WinTimeval {
    sec: i32,
    usec: i32,
}

// hostent pour gethostbyname/gethostbyaddr et WSAAsyncGetHostBy*.
/// Type opaque : le HOSTENT est écrit octet par octet par `pack_hostent`,
/// à la disposition du programme (16 octets en x86, 32 en x64).
pub(crate) struct WinHostent {
    _opaque: u8,
}

/// HOSTENT emballé à `base` : struct, h_aliases (vide), h_addr_list, les
/// adresses puis le nom. Rend la taille nécessaire ; n'écrit rien si
/// `out` est trop petit.
fn pack_hostent(out: &mut [u8], base: usize, name: &[u8], addrs: &[[u8; 4]], ps: usize) -> usize {
    let head = if ps == 4 { 16 } else { 32 };
    let aliases = head;
    let list = aliases + ps;
    let data = list + ps * (addrs.len() + 1);
    let name_at = data + 4 * addrs.len();
    let mut name = name.to_vec();
    if name.last() != Some(&0) {
        name.push(0);
    }
    let total = name_at + name.len();
    if out.len() < total {
        return total;
    }
    out[..total].fill(0);
    let mut put = |at: usize, v: usize| {
        out[at..at + ps].copy_from_slice(&v.to_le_bytes()[..ps]);
    };
    put(0, base + name_at);
    put(ps, base + aliases);
    put(head - ps, base + list);
    for i in 0..addrs.len() {
        put(list + ps * i, base + data + 4 * i);
    }
    out[2 * ps..2 * ps + 2].copy_from_slice(&(libc::AF_INET as i16).to_le_bytes());
    out[2 * ps + 2..2 * ps + 4].copy_from_slice(&4i16.to_le_bytes());
    for (i, a) in addrs.iter().enumerate() {
        out[data + 4 * i..data + 4 * i + 4].copy_from_slice(a);
    }
    out[name_at..total].copy_from_slice(&name);
    total
}

thread_local! {
    /// Tampon par thread de gethostbyname/gethostbyaddr, comme Windows.
    static HOSTENT_TLS: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Publie un HOSTENT dans le tampon du thread.
fn publish_hostent(name: &[u8], addrs: &[[u8; 4]]) -> *const WinHostent {
    let ps = crate::wow32::pointer_size();
    HOSTENT_TLS.with(|b| {
        let mut b = b.borrow_mut();
        let need = pack_hostent(&mut [], 0, name, addrs, ps);
        if b.len() < need {
            *b = vec![0; need.max(1024)];
        }
        let base = b.as_ptr() as usize;
        pack_hostent(&mut b, base, name, addrs, ps);
        base as *const WinHostent
    })
}

/// Résolution IPv4 d'un nom : (nom canonique, adresses) ou code WSA.
unsafe fn resolve_v4(text: &str) -> Result<(Vec<u8>, Vec<[u8; 4]>), i32> {
    let c = CString::new(text).map_err(|_| WSAHOST_NOT_FOUND)?;
    let mut hints: libc::addrinfo = std::mem::zeroed();
    hints.ai_family = libc::AF_INET;
    hints.ai_socktype = libc::SOCK_STREAM;
    hints.ai_flags = libc::AI_CANONNAME;
    let mut raw = std::ptr::null_mut();
    let rc = libc::getaddrinfo(c.as_ptr(), std::ptr::null(), &hints, &mut raw);
    if rc != 0 {
        return Err(gai_error(rc));
    }
    let mut addrs: Vec<[u8; 4]> = Vec::new();
    let mut hostname = Vec::new();
    let mut cur = raw;
    while !cur.is_null() {
        let ai = &*cur;
        if ai.ai_family == libc::AF_INET && ai.ai_addrlen >= 16 {
            let sin = &*(ai.ai_addr as *const libc::sockaddr_in);
            let a = sin.sin_addr.s_addr.to_ne_bytes();
            if !addrs.contains(&a) {
                addrs.push(a);
            }
        }
        if hostname.is_empty() && !ai.ai_canonname.is_null() {
            hostname = std::ffi::CStr::from_ptr(ai.ai_canonname)
                .to_bytes()
                .to_vec();
        }
        cur = ai.ai_next;
    }
    libc::freeaddrinfo(raw);
    if addrs.is_empty() {
        return Err(WSAHOST_NOT_FOUND);
    }
    if hostname.is_empty() {
        hostname = text.as_bytes().to_vec();
    }
    Ok((hostname, addrs))
}

/// Résolution inverse d'une adresse IPv4 (nom numérique si pas de PTR).
unsafe fn reverse_v4(ip: [u8; 4]) -> Vec<u8> {
    let mut sin: libc::sockaddr_in = std::mem::zeroed();
    sin.sin_family = libc::AF_INET as libc::sa_family_t;
    sin.sin_addr.s_addr = u32::from_ne_bytes(ip);
    let mut host = [0 as c_char; 256];
    let ni = libc::getnameinfo(
        &sin as *const libc::sockaddr_in as *const libc::sockaddr,
        std::mem::size_of::<libc::sockaddr_in>() as u32,
        host.as_mut_ptr(),
        256,
        std::ptr::null_mut(),
        0,
        0,
    );
    if ni == 0 {
        std::ffi::CStr::from_ptr(host.as_ptr()).to_bytes().to_vec()
    } else {
        std::net::Ipv4Addr::from(ip).to_string().into_bytes()
    }
}

#[repr(C)]
pub(crate) struct WinServent {
    s_name: *mut c_char,
    s_aliases: *mut *mut c_char,
    // Disposition _WIN64 : s_proto précède s_port.
    s_proto: *mut c_char,
    s_port: i16,
}
static SERVENT_BUF: OnceLock<Mutex<ServentBuf>> = OnceLock::new();
struct ServentBuf {
    servent: WinServent,
    name: Vec<u8>,
    proto: Vec<u8>,
    aliases_ptr: Vec<*mut c_char>,
    /// Forme x86 : { s_name; s_aliases; SHORT s_port; s_proto }, 16 octets,
    /// s_port avant s_proto (l'inverse de _WIN64), suivie du tableau
    /// s_aliases vide. Alloué sur le tas à chaque appel : le tas est sous
    /// 4 Go en mode x86, alors que ce static vit au-dessus.
    servent32: Vec<u32>,
}
unsafe impl Send for ServentBuf {}
fn sockets() -> &'static Mutex<HashSet<usize>> {
    SOCKETS.get_or_init(|| Mutex::new(HashSet::new()))
}
use super::lock;
/// Sous Windows, WSAGetLastError et GetLastError lisent la même valeur
/// (celle du TEB) : .NET relit l'erreur d'une socket par GetLastError
/// (Marshal.GetLastPInvokeError). La copie par fil sert hors TEB (tests).
fn set_error(e: i32) {
    LAST_WSA_ERROR.with(|x| x.set(e));
    crate::process::set_last_error(e as u32);
}
fn error_from_errno(e: i32) -> i32 {
    match e {
        libc::EACCES | libc::EPERM => 10013,
        libc::EADDRINUSE => 10048,
        libc::EADDRNOTAVAIL => 10049,
        libc::EAFNOSUPPORT => 10047,
        // Windows signale un connect non bloquant en cours par WSAEWOULDBLOCK
        // (10035), jamais WSAEINPROGRESS (10036) que PuTTY traite comme fatal.
        libc::EINPROGRESS => WSAEWOULDBLOCK,
        libc::EINTR => 10004,
        libc::EINVAL => 10022,
        libc::EMFILE => 10024,
        libc::ENFILE => 10023,
        libc::ENOMEM => 10055,
        libc::ENOTSOCK => 10038,
        libc::EPIPE | libc::ECONNRESET => 10054,
        libc::ECONNREFUSED => 10061,
        libc::ETIMEDOUT => 10060,
        libc::ENETUNREACH => 10051,
        libc::EHOSTUNREACH => 10065,
        libc::EWOULDBLOCK => 10035,
        libc::ENOTCONN => 10057,
        libc::EISCONN => 10056,
        libc::EALREADY => 10037,
        libc::EMSGSIZE => 10040,
        libc::ECONNABORTED => 10053,
        libc::ENOPROTOOPT => 10042,
        libc::EBADF => 10038,
        libc::ENETDOWN => 10050,
        libc::ESHUTDOWN => 10058,
        libc::EPROTONOSUPPORT => 10043,
        libc::ESOCKTNOSUPPORT => 10044,
        libc::EOPNOTSUPP => 10045,
        _ => 10022,
    }
}
/// Traduit les drapeaux Winsock de `recv` (MSG_WAITALL = 0x8 sous Windows,
/// 0x100 sous Linux ; MSG_PEEK et MSG_OOB coïncident).
fn recv_flags(win: i32) -> i32 {
    let mut f = win & (libc::MSG_PEEK | libc::MSG_OOB);
    if win & 0x8 != 0 {
        f |= libc::MSG_WAITALL;
    }
    f
}
fn last_error() -> i32 {
    unsafe { error_from_errno(*libc::__errno_location()) }
}
/// Enregistre une socket neuve. Son numéro de descripteur a pu servir à une
/// socket fermée sans closesocket : ses modes de complétion ne la suivent
/// pas (FILE_SKIP_SET_EVENT_ON_HANDLE empêchait de signaler hEvent, #1360).
fn register_socket(s: usize) {
    overlapped::forget_modes(s);
    lock(sockets()).insert(s);
}
fn valid_socket(s: usize) -> bool {
    lock(sockets()).contains(&s)
}
/// Reprend une socket transmise par un autre processus (héritage,
/// DuplicateHandle). Le handle garde la valeur `want` quand ce numéro de
/// descripteur est libre ; sinon il prend celui du descripteur reçu.
pub(crate) fn adopt_socket(fd: std::os::fd::OwnedFd, want: usize) -> usize {
    use std::os::fd::{AsRawFd, IntoRawFd};
    let n = fd.as_raw_fd();
    let free = want != 0
        && want <= i32::MAX as usize
        && unsafe { libc::fcntl(want as i32, libc::F_GETFD) } < 0;
    let h = if free && unsafe { libc::dup3(n, want as i32, libc::O_CLOEXEC) } == want as i32 {
        want
    } else {
        fd.into_raw_fd() as usize
    };
    register_socket(h);
    h
}
fn gai_error(e: i32) -> i32 {
    if e == libc::EAI_AGAIN {
        WSATRY_AGAIN
    } else if e == libc::EAI_NONAME {
        WSAHOST_NOT_FOUND
    } else if e == libc::EAI_FAMILY {
        10047
    } else if e == libc::EAI_SOCKTYPE {
        10044
    } else if e == libc::EAI_SERVICE {
        WSATYPE_NOT_FOUND
    } else if e == libc::EAI_MEMORY {
        10055
    } else if e == libc::EAI_BADFLAGS {
        10022
    } else if e == libc::EAI_FAIL {
        WSANO_RECOVERY
    } else {
        10022
    }
}
unsafe fn make_libc_hints(hints: *const WinAddrInfoA) -> Option<libc::addrinfo> {
    if hints.is_null() {
        return None;
    }
    // Seuls les quatre premiers champs (16 octets, communs à x86 et x64)
    // sont lus : la forme x86 ne fait que 32 octets.
    let h = std::ptr::read_unaligned(hints as *const [i32; 4]);
    let mut out: libc::addrinfo = std::mem::zeroed();
    out.ai_flags = h[0];
    out.ai_family = af_in(h[1]);
    out.ai_socktype = h[2];
    out.ai_protocol = h[3];
    Some(out)
}

#[repr(C)]
struct WsaData {
    version: u16,
    high_version: u16,
    max_sockets: u16,
    max_udp: u16,
    vendor: *mut c_char,
    description: [c_char; 257],
    system_status: [c_char; 129],
}

/// Taille de WSADATA pour un programme x86 : 2 + 2 + 257 + 129 + 2 + 2,
/// aligné sur 4, puis lpVendorInfo (4 octets).
const WSADATA32_SIZE: usize = 400;

pub unsafe extern "win64" fn wsa_startup(requested: u16, data: *mut u8) -> i32 {
    if data.is_null() {
        return 10014;
    }
    let major = (requested & 0xff) as u8;
    let minor = (requested >> 8) as u8;
    // Windows accepte toute version de 1.0 à 2.2 et répond la plus proche
    // (OpenTTD demande 2.0) ; au-delà de 2.2, il propose 2.2.
    let version = if major > 2 || (major == 2 && minor >= 2) {
        0x0202
    } else if major >= 1 {
        requested
    } else {
        return 10092; // WSAVERNOTSUPPORTED
    };
    let mut out: WsaData = std::mem::zeroed();
    out.version = version;
    out.high_version = 0x0202;
    let desc = b"Winlux Winsock/POSIX bridge\0";
    std::ptr::copy_nonoverlapping(
        desc.as_ptr(),
        out.description.as_mut_ptr() as *mut u8,
        desc.len(),
    );
    let status = b"Running on Linux\0";
    std::ptr::copy_nonoverlapping(
        status.as_ptr(),
        out.system_status.as_mut_ptr() as *mut u8,
        status.len(),
    );
    if crate::wow32::pointer_size() == 4 {
        // Programme x86 : WSADATA y fait 400 octets et range les chaînes
        // avant iMaxSockets/iMaxUdpDg/lpVendorInfo. La forme 64 bits
        // (408 octets) débordait de 8 octets sur la pile de l'appelant :
        // WinSCP x86 y perdait son adresse de retour (#779).
        let b = std::slice::from_raw_parts_mut(data, WSADATA32_SIZE);
        b.fill(0);
        b[0..2].copy_from_slice(&out.version.to_le_bytes());
        b[2..4].copy_from_slice(&out.high_version.to_le_bytes());
        b[4..4 + desc.len()].copy_from_slice(desc);
        b[261..261 + status.len()].copy_from_slice(status);
        // iMaxSockets (390), iMaxUdpDg (392), lpVendorInfo (396) : 0.
    } else {
        std::ptr::write_unaligned(data as *mut WsaData, out);
    }
    STARTUPS.fetch_add(1, Ordering::AcqRel);
    0
}
pub extern "win64" fn wsa_cleanup() -> i32 {
    let old = STARTUPS.try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
        if n > 0 {
            Some(n - 1)
        } else {
            None
        }
    });
    match old {
        Ok(1) => {
            let mut s = lock(sockets());
            for h in s.drain() {
                unsafe {
                    libc::close(h as i32);
                }
            }
            0
        }
        Ok(_) => 0,
        Err(_) => {
            set_error(WSA_NOT_INITIALISED);
            SOCKET_ERROR
        }
    }
}
pub extern "win64" fn wsa_get_last_error() -> i32 {
    if crate::process::teb() != 0 {
        crate::process::last_error() as i32
    } else {
        LAST_WSA_ERROR.with(|x| x.get())
    }
}
/// SetFileCompletionNotificationModes sur une socket ; faux si `s` n'en
/// est pas une.
pub(crate) fn set_completion_modes(s: usize, flags: u8) -> bool {
    if !valid_socket(s) {
        return false;
    }
    overlapped::set_modes(s, flags);
    true
}

/// Le handle est-il une socket ouverte ? (CreateIoCompletionPort, CancelIoEx)
pub(crate) fn is_socket(s: usize) -> bool {
    valid_socket(s)
}
/// CancelIo / CancelIoEx sur une socket.
pub(crate) fn cancel_overlapped(s: usize, overlapped: usize) -> bool {
    overlapped::cancel(s, overlapped)
}
pub extern "win64" fn wsa_set_last_error(e: i32) {
    set_error(e)
}
pub extern "win64" fn socket(af: i32, kind: i32, protocol: i32) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return usize::MAX;
    }
    // SOCK_CLOEXEC : un enfant n'hérite d'une socket que si elle est
    // marquée héritable, et `inherit` la lui passe alors (#1347).
    let s = unsafe { libc::socket(af_in(af), kind | libc::SOCK_CLOEXEC, protocol) };
    if s < 0 {
        set_error(last_error());
        usize::MAX
    } else {
        register_socket(s as usize);
        s as usize
    }
}
pub extern "win64" fn closesocket(s: usize) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    // Les opérations en cours se terminent en WSA_OPERATION_ABORTED, leur
    // paquet parti, avant que le numéro de socket ne puisse resservir.
    overlapped::close(s);
    let r = unsafe { libc::close(s as i32) };
    if r < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        lock(sockets()).remove(&s);
        crate::dlls::kernel32::forget_completion_port(s);
        // Un fd réutilisé ne doit pas hériter du masque WSAEventSelect.
        lock(event_assoc()).remove(&s);
        lock(async_sel()).remove(&s);
        0
    }
}
/// Réseau : bloqué seulement si l'utilisateur l'a refusé à cette application
/// (Paramètres > Confidentialité et sécurité) ; la boucle locale reste libre.
unsafe fn network_allowed(name: *const u8, len: i32) -> bool {
    crate::permissions::sockaddr_is_local(name, len)
        || crate::permissions::allowed(crate::permissions::Capability::Network)
}
pub unsafe extern "win64" fn connect(s: usize, name: *const u8, len: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    if name.is_null() || len < 0 {
        set_error(10014);
        return SOCKET_ERROR;
    }
    if !network_allowed(name, len) {
        set_error(10013); // WSAEACCES : bloqué dans Paramètres
        return SOCKET_ERROR;
    }
    let addr = addr_in(name, len);
    let rc = libc::connect(
        s as i32,
        &addr as *const _ as *const libc::sockaddr,
        len as libc::socklen_t,
    );
    if rc < 0 {
        let e = last_error();
        if e == WSAEWOULDBLOCK {
            rearm_write(s);
            async_connecting(s);
        }
        set_error(e);
        SOCKET_ERROR
    } else {
        rearm_write(s);
        async_connecting(s);
        0
    }
}
pub unsafe extern "win64" fn bind(s: usize, name: *const u8, len: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    if name.is_null() || len < 0 {
        set_error(10014);
        return SOCKET_ERROR;
    }
    let addr = addr_in(name, len);
    let rc = libc::bind(
        s as i32,
        &addr as *const _ as *const libc::sockaddr,
        len as libc::socklen_t,
    );
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        0
    }
}
pub extern "win64" fn listen(s: usize, backlog: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    let rc = unsafe { libc::listen(s as i32, backlog) };
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        0
    }
}
pub unsafe extern "win64" fn accept(s: usize, name: *mut u8, len: *mut i32) -> usize {
    if !valid_socket(s) {
        set_error(10038);
        return usize::MAX;
    }
    async_arm(s, FD_ACCEPT);
    let mut n = if len.is_null() {
        0
    } else {
        (*len).max(0) as libc::socklen_t
    };
    let ptr = if name.is_null() {
        std::ptr::null_mut()
    } else {
        name as *mut libc::sockaddr
    };
    let r = libc::accept4(s as i32, ptr, &mut n, libc::SOCK_CLOEXEC);
    if r < 0 {
        set_error(last_error());
        return usize::MAX;
    }
    addr_out(name, n as usize);
    if !len.is_null() {
        *len = n as i32;
    }
    register_socket(r as usize);
    // La socket acceptée garde l'association WSAEventSelect de la socket
    // d'écoute, comme sous Windows (#1360).
    {
        let mut assoc = lock(event_assoc());
        if let Some(a) = assoc.get(&s).copied() {
            // WSAEventSelect rend aussi la socket non bloquante.
            libc::fcntl(
                r,
                libc::F_SETFL,
                libc::fcntl(r, libc::F_GETFL) | libc::O_NONBLOCK,
            );
            assoc.insert(
                r as usize,
                EventAssoc {
                    write_armed: true,
                    ..a
                },
            );
        }
    }
    r as usize
}
pub unsafe extern "win64" fn send(s: usize, b: *const u8, n: i32, flags: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    if n < 0 || (n > 0 && b.is_null()) {
        set_error(if n < 0 { WSA_INVALID_PARAMETER } else { 10014 });
        return SOCKET_ERROR;
    }
    let r = libc::send(
        s as i32,
        b as *const c_void,
        n as usize,
        flags | libc::MSG_NOSIGNAL,
    );
    crate::trace!("send({s}, {n}) = {r}");
    if r < 0 {
        let e = last_error();
        if e == WSAEWOULDBLOCK {
            rearm_write(s);
        }
        set_error(e);
        SOCKET_ERROR
    } else {
        r as i32
    }
}
pub unsafe extern "win64" fn recv(s: usize, b: *mut u8, n: i32, flags: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    async_arm(s, FD_READ | FD_OOB);
    if n < 0 || (n > 0 && b.is_null()) {
        set_error(if n < 0 { WSA_INVALID_PARAMETER } else { 10014 });
        return SOCKET_ERROR;
    }
    if n == 0 {
        // Winsock : une réception de longueur nulle réussit tout de suite
        // (Bun s'en sert pour sonder une socket après « prête en
        // écriture »). Sous Linux, elle bloquerait jusqu'aux premières
        // données ; on ne fait remonter que l'erreur en attente.
        let mut err: i32 = 0;
        let mut len = 4u32;
        libc::getsockopt(
            s as i32,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            &mut err as *mut i32 as *mut c_void,
            &mut len,
        );
        if err != 0 {
            set_error(error_from_errno(err));
            return SOCKET_ERROR;
        }
        return 0;
    }
    let r = libc::recv(s as i32, b as *mut c_void, n as usize, recv_flags(flags));
    crate::trace!("recv({s}, {n}, {flags:#x}) = {r}");
    if r < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        r as i32
    }
}
pub unsafe extern "win64" fn send_to(
    s: usize,
    b: *const u8,
    n: i32,
    flags: i32,
    name: *const u8,
    len: i32,
) -> i32 {
    if n < 0 || len < 0 || (n > 0 && b.is_null()) || name.is_null() {
        set_error(WSA_INVALID_PARAMETER);
        return SOCKET_ERROR;
    }
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    if !network_allowed(name, len) {
        set_error(10013);
        return SOCKET_ERROR;
    }
    let addr = addr_in(name, len);
    let r = libc::sendto(
        s as i32,
        b as *const c_void,
        n as usize,
        flags | libc::MSG_NOSIGNAL,
        if name.is_null() {
            std::ptr::null()
        } else {
            &addr as *const _ as *const libc::sockaddr
        },
        len as libc::socklen_t,
    );
    if r < 0 {
        let e = last_error();
        if e == WSAEWOULDBLOCK {
            rearm_write(s);
        }
        set_error(e);
        SOCKET_ERROR
    } else {
        r as i32
    }
}
pub unsafe extern "win64" fn recv_from(
    s: usize,
    b: *mut u8,
    n: i32,
    flags: i32,
    name: *mut u8,
    len: *mut i32,
) -> i32 {
    if n < 0 || (n > 0 && b.is_null()) {
        set_error(WSA_INVALID_PARAMETER);
        return SOCKET_ERROR;
    }
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    async_arm(s, FD_READ | FD_OOB);
    let mut addrlen = if len.is_null() {
        0
    } else {
        (*len).max(0) as libc::socklen_t
    };
    let addr = if name.is_null() {
        std::ptr::null_mut()
    } else {
        name as *mut libc::sockaddr
    };
    let r = libc::recvfrom(
        s as i32,
        b as *mut c_void,
        n as usize,
        flags,
        addr,
        &mut addrlen,
    );
    if r < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        addr_out(name, addrlen as usize);
        if !len.is_null() {
            *len = addrlen as i32;
        }
        r as i32
    }
}
pub unsafe extern "win64" fn shutdown(s: usize, how: i32) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    let rc = libc::shutdown(s as i32, how);
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        0
    }
}
pub unsafe extern "win64" fn get_socket_name(s: usize, name: *mut u8, len: *mut i32) -> i32 {
    socket_name_common(s, name, len, false)
}
pub unsafe extern "win64" fn get_peer_name(s: usize, name: *mut u8, len: *mut i32) -> i32 {
    socket_name_common(s, name, len, true)
}
unsafe fn socket_name_common(s: usize, name: *mut u8, len: *mut i32, peer: bool) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    if name.is_null() || len.is_null() || *len < 0 {
        set_error(10014);
        return SOCKET_ERROR;
    }
    let mut n = *len as libc::socklen_t;
    let rc = if peer {
        libc::getpeername(s as i32, name as *mut libc::sockaddr, &mut n)
    } else {
        libc::getsockname(s as i32, name as *mut libc::sockaddr, &mut n)
    };
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        addr_out(name, *len as usize);
        *len = n as i32;
        0
    }
}
pub extern "win64" fn htons(v: u16) -> u16 {
    v.to_be()
}
pub extern "win64" fn ntohs(v: u16) -> u16 {
    u16::from_be(v)
}
pub extern "win64" fn htonl(v: u32) -> u32 {
    v.to_be()
}
pub extern "win64" fn ntohl(v: u32) -> u32 {
    u32::from_be(v)
}
pub unsafe extern "win64" fn inet_addr(v: *const u8) -> u32 {
    if v.is_null() {
        set_error(10014);
        return u32::MAX;
    }
    let text = crate::dlls::cstr(v);
    match text.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => u32::from_ne_bytes(ip.octets()),
        Err(_) => u32::MAX,
    }
}
pub unsafe extern "win64" fn get_host_name(buf: *mut u8, len: i32) -> i32 {
    if buf.is_null() || len <= 0 {
        set_error(10014);
        return SOCKET_ERROR;
    }
    let mut b = vec![0i8; len as usize];
    if libc::gethostname(b.as_mut_ptr(), b.len()) < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    let end = b
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(b.len().saturating_sub(1));
    std::ptr::copy_nonoverlapping(b.as_ptr() as *const u8, buf, end);
    *buf.add(end) = 0;
    0
}

/// `GetHostNameW(name, namelen)` : nom d'hôte en UTF-16.
pub unsafe extern "win64" fn get_host_name_w(buf: *mut u16, len: i32) -> i32 {
    if buf.is_null() || len <= 0 {
        set_error(10014);
        return SOCKET_ERROR;
    }
    let mut b = [0i8; 256];
    if libc::gethostname(b.as_mut_ptr(), b.len()) < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    let name = std::ffi::CStr::from_ptr(b.as_ptr())
        .to_string_lossy()
        .into_owned();
    let w: Vec<u16> = name.encode_utf16().collect();
    if w.len() >= len as usize {
        set_error(10014); // WSAEFAULT : tampon trop petit
        return SOCKET_ERROR;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    0
}

unsafe fn make_addr_a(raw: *mut libc::addrinfo) -> Result<(usize, AddrInfoAllocation), i32> {
    let (mut nodes, mut addrs, mut names) = (Vec::new(), Vec::new(), Vec::new());
    let (mut head, mut prev) = (
        std::ptr::null_mut::<WinAddrInfoA>(),
        std::ptr::null_mut::<WinAddrInfoA>(),
    );
    let mut cur = raw;
    while !cur.is_null() {
        let ai = &*cur;
        let length = ai.ai_addrlen as usize;
        if length > std::mem::size_of::<libc::sockaddr_storage>()
            || (length != 0 && ai.ai_addr.is_null())
        {
            libc::freeaddrinfo(raw);
            drop_addr_a(nodes, addrs, names);
            return Err(10014);
        }
        let addr = if length == 0 {
            std::ptr::null_mut()
        } else {
            let mut storage: Box<libc::sockaddr_storage> = Box::new(std::mem::zeroed());
            std::ptr::copy_nonoverlapping(
                ai.ai_addr as *const u8,
                &mut *storage as *mut _ as *mut u8,
                length,
            );
            storage.ss_family = af_out(storage.ss_family as i32) as u16;
            let p = Box::into_raw(storage);
            addr_out(p as *mut u8, length);
            addrs.push(p as usize);
            p as *mut u8
        };
        let canon = if ai.ai_canonname.is_null() {
            std::ptr::null_mut()
        } else {
            let s = crate::dlls::decode_ansi(std::ffi::CStr::from_ptr(ai.ai_canonname).to_bytes());
            let c = CString::new(crate::dlls::encode_ansi(&s)).map_err(|_| 10022)?;
            let p = c.into_raw();
            names.push(p as usize);
            p
        };
        let p = Box::into_raw(Box::new(WinAddrInfoA {
            flags: ai.ai_flags,
            family: af_out(ai.ai_family),
            socktype: ai.ai_socktype,
            protocol: ai.ai_protocol,
            addrlen: length,
            canonname: canon,
            addr,
            next: std::ptr::null_mut(),
        }));
        nodes.push(p as usize);
        if prev.is_null() {
            head = p
        } else {
            (*prev).next = p;
        }
        prev = p;
        cur = ai.ai_next;
    }
    libc::freeaddrinfo(raw);
    Ok((
        head as usize,
        AddrInfoAllocation::A {
            nodes,
            addrs,
            names,
        },
    ))
}
unsafe fn drop_addr_a(nodes: Vec<usize>, addrs: Vec<usize>, names: Vec<usize>) {
    for p in nodes {
        drop(Box::from_raw(p as *mut WinAddrInfoA));
    }
    for p in addrs {
        drop(Box::from_raw(p as *mut libc::sockaddr_storage));
    }
    for p in names {
        drop(CString::from_raw(p as *mut c_char));
    }
}
unsafe fn drop_addr_w(nodes: Vec<usize>, addrs: Vec<usize>, names: Vec<(usize, usize)>) {
    for p in nodes {
        drop(Box::from_raw(p as *mut WinAddrInfoW));
    }
    for p in addrs {
        drop(Box::from_raw(p as *mut libc::sockaddr_storage));
    }
    for (p, n) in names {
        let slice = std::ptr::slice_from_raw_parts_mut(p as *mut u16, n);
        drop(Box::from_raw(slice));
    }
}
unsafe fn make_addr_w(raw: *mut libc::addrinfo) -> Result<(usize, AddrInfoAllocation), i32> {
    let (mut nodes, mut addrs, mut names) = (Vec::new(), Vec::new(), Vec::new());
    let (mut head, mut prev) = (
        std::ptr::null_mut::<WinAddrInfoW>(),
        std::ptr::null_mut::<WinAddrInfoW>(),
    );
    let mut cur = raw;
    while !cur.is_null() {
        let ai = &*cur;
        let length = ai.ai_addrlen as usize;
        if length > std::mem::size_of::<libc::sockaddr_storage>()
            || (length != 0 && ai.ai_addr.is_null())
        {
            libc::freeaddrinfo(raw);
            drop_addr_w(nodes, addrs, names);
            return Err(10014);
        }
        let addr = if length == 0 {
            std::ptr::null_mut()
        } else {
            let mut storage: Box<libc::sockaddr_storage> = Box::new(std::mem::zeroed());
            std::ptr::copy_nonoverlapping(
                ai.ai_addr as *const u8,
                &mut *storage as *mut _ as *mut u8,
                length,
            );
            storage.ss_family = af_out(storage.ss_family as i32) as u16;
            let p = Box::into_raw(storage);
            addr_out(p as *mut u8, length);
            addrs.push(p as usize);
            p as *mut u8
        };
        let canon = if ai.ai_canonname.is_null() {
            std::ptr::null_mut()
        } else {
            let s = crate::dlls::decode_ansi(std::ffi::CStr::from_ptr(ai.ai_canonname).to_bytes());
            let v: Vec<u16> = s.encode_utf16().chain([0]).collect();
            let n = v.len();
            let p = Box::into_raw(v.into_boxed_slice());
            names.push((p as *mut u16 as usize, n));
            p as *mut u16
        };
        let p = Box::into_raw(Box::new(WinAddrInfoW {
            flags: ai.ai_flags,
            family: af_out(ai.ai_family),
            socktype: ai.ai_socktype,
            protocol: ai.ai_protocol,
            addrlen: length,
            canonname: canon,
            addr,
            next: std::ptr::null_mut(),
        }));
        nodes.push(p as usize);
        if prev.is_null() {
            head = p
        } else {
            (*prev).next = p;
        }
        prev = p;
        cur = ai.ai_next;
    }
    libc::freeaddrinfo(raw);
    Ok((
        head as usize,
        AddrInfoAllocation::W {
            nodes,
            addrs,
            names,
        },
    ))
}
fn register_addrinfo(head: usize, allocation: AddrInfoAllocation) {
    if head != 0 {
        lock(ADDRINFO_ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))).insert(head, allocation);
    }
}
/// Programme x86 (wow32) : ADDRINFOA/W y fait 32 octets (size_t et
/// pointeurs sur 4 octets) au lieu de 48. Chaque nœud est réécrit sur place
/// (sa place de 48 octets suffit) ; toute la mémoire d'un processus x86
/// est sous 4 Go, les pointeurs tiennent donc sur 4 octets. Sans cela,
/// PuTTY 32 bits lisait le port et l'adresse au mauvais endroit et se
/// connectait au port 22 au lieu de celui demandé.
unsafe fn compact_addrinfo_x86(head: usize) {
    if crate::wow32::pointer_size() != 4 {
        return;
    }
    let mut nodes = Vec::new();
    let mut cur = head as *const WinAddrInfoA;
    while !cur.is_null() {
        let n = std::ptr::read_unaligned(cur);
        nodes.push((
            cur as usize,
            n.flags,
            n.family,
            n.socktype,
            n.protocol,
            n.addrlen,
            n.canonname as usize,
            n.addr as usize,
            n.next as usize,
        ));
        cur = n.next;
    }
    for &(at, flags, family, socktype, protocol, addrlen, canon, addr, next) in &nodes {
        let words: [u32; 8] = [
            flags as u32,
            family as u32,
            socktype as u32,
            protocol as u32,
            addrlen as u32,
            canon as u32,
            addr as u32,
            next as u32,
        ];
        std::ptr::copy_nonoverlapping(words.as_ptr() as *const u8, at as *mut u8, 32);
    }
}

unsafe extern "win64" fn get_addr_info_a(
    node: *const u8,
    service: *const u8,
    hints: *const WinAddrInfoA,
    result: *mut *mut WinAddrInfoA,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        return WSA_NOT_INITIALISED;
    }
    if result.is_null() {
        return 10014;
    }
    *result = std::ptr::null_mut();
    let node_text = if node.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(node))
    };
    let service_text = if service.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(service))
    };
    let node_c = match node_text {
        Some(s) => match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return 10022,
        },
        None => None,
    };
    let service_c = match service_text {
        Some(s) => match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return 10022,
        },
        None => None,
    };
    let mut h = match make_libc_hints(hints) {
        Some(h) => h,
        None => std::mem::zeroed(),
    };
    let mut raw = std::ptr::null_mut();
    let rc = libc::getaddrinfo(
        node_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
        service_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
        if hints.is_null() {
            std::ptr::null()
        } else {
            &mut h
        },
        &mut raw,
    );
    if rc != 0 {
        return gai_error(rc);
    }
    match make_addr_a(raw) {
        Ok((head, alloc)) => {
            compact_addrinfo_x86(head);
            *result = head as *mut WinAddrInfoA;
            register_addrinfo(head, alloc);
            0
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn get_addr_info_w(
    node: *const u16,
    service: *const u16,
    hints: *const WinAddrInfoW,
    result: *mut *mut WinAddrInfoW,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        return WSA_NOT_INITIALISED;
    }
    if result.is_null() {
        return 10014;
    }
    *result = std::ptr::null_mut();
    let node_c = if node.is_null() {
        None
    } else {
        let n = crate::printf::wide_len(node);
        let s = match String::from_utf16(std::slice::from_raw_parts(node, n)) {
            Ok(s) => s,
            Err(_) => return 10022,
        };
        match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return 10022,
        }
    };
    let service_c = if service.is_null() {
        None
    } else {
        let n = crate::printf::wide_len(service);
        let s = match String::from_utf16(std::slice::from_raw_parts(service, n)) {
            Ok(s) => s,
            Err(_) => return 10022,
        };
        match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return 10022,
        }
    };
    let mut h: libc::addrinfo = std::mem::zeroed();
    if !hints.is_null() {
        if let Some(hin) = make_libc_hints(hints as *const WinAddrInfoA) {
            h = hin;
        }
    }
    let mut raw = std::ptr::null_mut();
    let rc = libc::getaddrinfo(
        node_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
        service_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
        if hints.is_null() {
            std::ptr::null()
        } else {
            &mut h
        },
        &mut raw,
    );
    if rc != 0 {
        return gai_error(rc);
    }
    match make_addr_w(raw) {
        Ok((head, alloc)) => {
            compact_addrinfo_x86(head);
            *result = head as *mut WinAddrInfoW;
            register_addrinfo(head, alloc);
            0
        }
        Err(e) => e,
    }
}
unsafe extern "win64" fn free_addr_info_a(head: *mut WinAddrInfoA) {
    let mut m = lock(ADDRINFO_ALLOCS.get_or_init(|| Mutex::new(HashMap::new())));
    if !matches!(m.get(&(head as usize)), Some(AddrInfoAllocation::A { .. })) {
        set_error(WSA_INVALID_PARAMETER);
        return;
    }
    let Some(AddrInfoAllocation::A {
        nodes,
        addrs,
        names,
    }) = m.remove(&(head as usize))
    else {
        unreachable!()
    };
    drop(m);
    drop_addr_a(nodes, addrs, names)
}
unsafe extern "win64" fn free_addr_info_w(head: *mut WinAddrInfoW) {
    let mut m = lock(ADDRINFO_ALLOCS.get_or_init(|| Mutex::new(HashMap::new())));
    if !matches!(m.get(&(head as usize)), Some(AddrInfoAllocation::W { .. })) {
        set_error(WSA_INVALID_PARAMETER);
        return;
    }
    let Some(AddrInfoAllocation::W {
        nodes,
        addrs,
        names,
    }) = m.remove(&(head as usize))
    else {
        unreachable!()
    };
    drop(m);
    drop_addr_w(nodes, addrs, names)
}

// ── setsockopt / getsockopt ──────────────────────────────────────────────
fn translate_sockopt(win_level: i32, win_opt: i32) -> Option<(i32, i32)> {
    match (win_level, win_opt) {
        (WIN_SOL_SOCKET, WIN_SO_DEBUG) => Some((libc::SOL_SOCKET, libc::SO_DEBUG)),
        (WIN_SOL_SOCKET, WIN_SO_REUSEADDR) => Some((libc::SOL_SOCKET, libc::SO_REUSEADDR)),
        (WIN_SOL_SOCKET, WIN_SO_KEEPALIVE) => Some((libc::SOL_SOCKET, libc::SO_KEEPALIVE)),
        (WIN_SOL_SOCKET, WIN_SO_DONTROUTE) => Some((libc::SOL_SOCKET, libc::SO_DONTROUTE)),
        (WIN_SOL_SOCKET, WIN_SO_BROADCAST) => Some((libc::SOL_SOCKET, libc::SO_BROADCAST)),
        (WIN_SOL_SOCKET, WIN_SO_LINGER) => Some((libc::SOL_SOCKET, libc::SO_LINGER)),
        (WIN_SOL_SOCKET, WIN_SO_OOBINLINE) => Some((libc::SOL_SOCKET, libc::SO_OOBINLINE)),
        (WIN_SOL_SOCKET, WIN_SO_SNDBUF) => Some((libc::SOL_SOCKET, libc::SO_SNDBUF)),
        (WIN_SOL_SOCKET, WIN_SO_RCVBUF) => Some((libc::SOL_SOCKET, libc::SO_RCVBUF)),
        (WIN_SOL_SOCKET, WIN_SO_SNDTIMEO) => Some((libc::SOL_SOCKET, libc::SO_SNDTIMEO)),
        (WIN_SOL_SOCKET, WIN_SO_RCVTIMEO) => Some((libc::SOL_SOCKET, libc::SO_RCVTIMEO)),
        (WIN_SOL_SOCKET, WIN_SO_ERROR) => Some((libc::SOL_SOCKET, libc::SO_ERROR)),
        (WIN_SOL_SOCKET, WIN_SO_TYPE) => Some((libc::SOL_SOCKET, libc::SO_TYPE)),
        (WIN_IPPROTO_TCP, WIN_TCP_NODELAY) => Some((libc::IPPROTO_TCP, libc::TCP_NODELAY)),
        (WIN_IPPROTO_TCP, 3) => Some((libc::IPPROTO_TCP, libc::TCP_KEEPIDLE)),
        (WIN_IPPROTO_TCP, 4) => Some((libc::IPPROTO_TCP, libc::TCP_MAXSEG)),
        (WIN_IPPROTO_TCP, 16) => Some((libc::IPPROTO_TCP, libc::TCP_KEEPCNT)),
        (WIN_IPPROTO_TCP, 17) => Some((libc::IPPROTO_TCP, libc::TCP_KEEPINTVL)),
        // IPPROTO_IP : les numéros Windows diffèrent de ceux de Linux
        // (multidiffusion des jeux en réseau local, découverte d'appareils…).
        (WIN_IPPROTO_IP, opt) => Some((
            libc::IPPROTO_IP,
            match opt {
                1 => libc::IP_OPTIONS,
                2 => libc::IP_HDRINCL,
                3 => libc::IP_TOS,
                4 => libc::IP_TTL,
                9 => libc::IP_MULTICAST_IF,
                10 => libc::IP_MULTICAST_TTL,
                11 => libc::IP_MULTICAST_LOOP,
                12 => libc::IP_ADD_MEMBERSHIP,
                13 => libc::IP_DROP_MEMBERSHIP,
                19 => libc::IP_PKTINFO,
                _ => return None,
            },
        )),
        // IPPROTO_IPV6 : IPV6_V6ONLY vaut 27 sous Windows, 26 sous Linux.
        (WIN_IPPROTO_IPV6, opt) => Some((
            libc::IPPROTO_IPV6,
            match opt {
                4 => libc::IPV6_UNICAST_HOPS,
                9 => libc::IPV6_MULTICAST_IF,
                10 => libc::IPV6_MULTICAST_HOPS,
                11 => libc::IPV6_MULTICAST_LOOP,
                12 => libc::IPV6_ADD_MEMBERSHIP,
                13 => libc::IPV6_DROP_MEMBERSHIP,
                19 => libc::IPV6_RECVPKTINFO,
                27 => libc::IPV6_V6ONLY,
                _ => return None,
            },
        )),
        (level, opt) if level != WIN_SOL_SOCKET => Some((level, opt)),
        _ => None,
    }
}

pub unsafe extern "win64" fn setsockopt(
    s: usize,
    level: i32,
    optname: i32,
    optval: *const u8,
    optlen: i32,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if optlen < 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    if level == WIN_SOL_SOCKET && optname == WIN_SO_EXCLUSIVEADDRUSE {
        return 0;
    }
    // SO_UPDATE_ACCEPT_CONTEXT / SO_UPDATE_CONNECT_CONTEXT : après AcceptEx
    // ou ConnectEx, Windows met la socket à jour ; Linux l'est déjà. .NET
    // tient la connexion pour ratée si l'appel échoue.
    if level == WIN_SOL_SOCKET && (optname == 0x700B || optname == 0x7010) {
        return 0;
    }
    let Some((l, o)) = translate_sockopt(level, optname) else {
        set_error(WSAENOPROTOOPT);
        return SOCKET_ERROR;
    };
    // Windows SO_LINGER uses u16 pairs, Linux uses c_int pairs
    if level == WIN_SOL_SOCKET && optname == WIN_SO_LINGER {
        if optlen < 4 || optval.is_null() {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let win = std::ptr::read_unaligned(optval as *const WinLinger);
        let lin = libc::linger {
            l_onoff: win.onoff as i32,
            l_linger: win.linger as i32,
        };
        let rc = libc::setsockopt(
            s as i32,
            l,
            o,
            &lin as *const _ as *const c_void,
            std::mem::size_of::<libc::linger>() as u32,
        );
        if rc < 0 {
            set_error(last_error());
            return SOCKET_ERROR;
        }
        return 0;
    }
    // Windows timeout is a DWORD (ms), Linux is timeval
    if level == WIN_SOL_SOCKET && (optname == WIN_SO_SNDTIMEO || optname == WIN_SO_RCVTIMEO) {
        if optlen < 4 || optval.is_null() {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let ms = std::ptr::read_unaligned(optval as *const u32);
        let tv = libc::timeval {
            tv_sec: (ms / 1000) as i64,
            tv_usec: ((ms % 1000) * 1000) as i64,
        };
        let rc = libc::setsockopt(
            s as i32,
            l,
            o,
            &tv as *const _ as *const c_void,
            std::mem::size_of::<libc::timeval>() as u32,
        );
        if rc < 0 {
            set_error(last_error());
            return SOCKET_ERROR;
        }
        return 0;
    }
    let rc = libc::setsockopt(
        s as i32,
        l,
        o,
        if optval.is_null() {
            std::ptr::null()
        } else {
            optval as *const c_void
        },
        optlen as u32,
    );
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        0
    }
}

pub unsafe extern "win64" fn getsockopt(
    s: usize,
    level: i32,
    optname: i32,
    optval: *mut u8,
    optlen: *mut i32,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if optval.is_null() || optlen.is_null() || *optlen < 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    // SO_PROTOCOL_INFOW / SO_PROTOCOL_INFOA : description du fournisseur.
    // libuv la lit avant de surveiller une socket (uv_poll) ; un
    // ProviderId nul l'oriente vers sa surveillance par select().
    if level == WIN_SOL_SOCKET && (optname == 0x2005 || optname == 0x2004) {
        let wide = optname == 0x2005;
        let need = if wide { 628 } else { 372 };
        if *optlen < need {
            *optlen = need;
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let get = |o: i32| -> i32 {
            let mut v: i32 = 0;
            let mut l = 4u32;
            libc::getsockopt(
                s as i32,
                libc::SOL_SOCKET,
                o,
                &mut v as *mut i32 as *mut c_void,
                &mut l,
            );
            v
        };
        let (domain, kind, proto) = (
            get(libc::SO_DOMAIN),
            get(libc::SO_TYPE),
            get(libc::SO_PROTOCOL),
        );
        std::ptr::write_bytes(optval, 0, need as usize);
        let put = |off: usize, v: i32| std::ptr::write_unaligned(optval.add(off) as *mut i32, v);
        // XP1_CONNECTIONLESS (datagrammes) ou XP1_GUARANTEED_DELIVERY|ORDER.
        put(
            0,
            if kind == libc::SOCK_DGRAM {
                0x1 | 0x200
            } else {
                0x2 | 0x4 | 0x20000
            },
        );
        put(36, 1001); // dwCatalogEntryId
        put(40, 1); // ProtocolChain.ChainLen : BASE_PROTOCOL
        put(72, 2); // iVersion
        let af = match domain {
            libc::AF_INET6 => 23,
            libc::AF_UNIX => 1,
            d => d,
        };
        put(76, af);
        put(80, if af == 23 { 28 } else { 16 });
        put(84, if af == 23 { 28 } else { 16 });
        put(88, kind);
        put(92, proto);
        put(100, 1); // BIGENDIAN
        put(108, if kind == libc::SOCK_DGRAM { 65467 } else { 0 });
        let name = if kind == libc::SOCK_DGRAM {
            "Winlux UDP"
        } else {
            "Winlux TCP"
        };
        if wide {
            for (i, c) in name.encode_utf16().enumerate() {
                std::ptr::write_unaligned(optval.add(116 + i * 2) as *mut u16, c);
            }
        } else {
            std::ptr::copy_nonoverlapping(name.as_ptr(), optval.add(116), name.len());
        }
        *optlen = need;
        return 0;
    }
    if level == WIN_SOL_SOCKET && optname == WIN_SO_EXCLUSIVEADDRUSE && *optlen >= 4 {
        std::ptr::write_unaligned(optval as *mut i32, 0);
        *optlen = 4;
        return 0;
    }
    let Some((l, o)) = translate_sockopt(level, optname) else {
        set_error(WSAENOPROTOOPT);
        return SOCKET_ERROR;
    };
    if level == WIN_SOL_SOCKET && optname == WIN_SO_LINGER {
        let mut lin: libc::linger = std::mem::zeroed();
        let mut sz = std::mem::size_of::<libc::linger>() as u32;
        let rc = libc::getsockopt(s as i32, l, o, &mut lin as *mut _ as *mut c_void, &mut sz);
        if rc < 0 {
            set_error(last_error());
            return SOCKET_ERROR;
        }
        if *optlen >= 4 {
            let win = WinLinger {
                onoff: lin.l_onoff as u16,
                linger: lin.l_linger as u16,
            };
            std::ptr::write_unaligned(optval as *mut WinLinger, win);
            *optlen = 4;
        }
        return 0;
    }
    if level == WIN_SOL_SOCKET && (optname == WIN_SO_SNDTIMEO || optname == WIN_SO_RCVTIMEO) {
        let mut tv: libc::timeval = std::mem::zeroed();
        let mut sz = std::mem::size_of::<libc::timeval>() as u32;
        let rc = libc::getsockopt(s as i32, l, o, &mut tv as *mut _ as *mut c_void, &mut sz);
        if rc < 0 {
            set_error(last_error());
            return SOCKET_ERROR;
        }
        if *optlen >= 4 {
            let ms = (tv.tv_sec as u32) * 1000 + (tv.tv_usec as u32) / 1000;
            std::ptr::write_unaligned(optval as *mut u32, ms);
            *optlen = 4;
        }
        return 0;
    }
    let mut sz = (*optlen) as u32;
    let rc = libc::getsockopt(s as i32, l, o, optval as *mut c_void, &mut sz);
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        // SO_ERROR : l'application attend un code WSAE*, pas un errno Linux.
        if l == libc::SOL_SOCKET && o == libc::SO_ERROR && sz >= 4 && !optval.is_null() {
            let e = std::ptr::read_unaligned(optval as *const i32);
            if e != 0 {
                std::ptr::write_unaligned(optval as *mut i32, error_from_errno(e));
            }
        }
        *optlen = sz as i32;
        0
    }
}

// ── ioctlsocket ─────────────────────────────────────────────────────────
pub unsafe extern "win64" fn ioctlsocket(s: usize, cmd: u32, argp: *mut u32) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if argp.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    match cmd {
        FIONBIO => {
            let val = *argp;
            let flags = libc::fcntl(s as i32, libc::F_GETFL);
            if flags < 0 {
                set_error(last_error());
                return SOCKET_ERROR;
            }
            let new = if val != 0 {
                flags | libc::O_NONBLOCK
            } else {
                flags & !libc::O_NONBLOCK
            };
            if libc::fcntl(s as i32, libc::F_SETFL, new) < 0 {
                set_error(last_error());
                SOCKET_ERROR
            } else {
                0
            }
        }
        FIONREAD => {
            let mut count: i32 = 0;
            if libc::ioctl(s as i32, libc::FIONREAD, &mut count) < 0 {
                set_error(last_error());
                SOCKET_ERROR
            } else {
                *argp = count as u32;
                0
            }
        }
        SIOCATMARK => {
            let mut at: i32 = 0;
            if libc::ioctl(s as i32, 0x8905 /* SIOCATMARK */, &mut at) < 0 {
                set_error(last_error());
                SOCKET_ERROR
            } else {
                *argp = at as u32;
                0
            }
        }
        _ => {
            set_error(WSAEINVAL);
            SOCKET_ERROR
        }
    }
}

// ── select ──────────────────────────────────────────────────────────────
/// Décalage de `fd_array` dans le `fd_set` Winsock x64 et taille maximale.
const FD_ARRAY_OFFSET: usize = 8;
const FD_SETSIZE_WIN: usize = 64;
/// Un descripteur utilisable par `libc::FD_SET` (sinon indexation hors bornes).
fn fd_set_ok(s: usize) -> bool {
    valid_socket(s) && s < libc::FD_SETSIZE
}
unsafe fn read_fd_set(win: *const WinFdSet) -> Vec<usize> {
    if win.is_null() {
        return Vec::new();
    }
    // `struct fd_set { u_int fd_count; SOCKET fd_array[64]; }` : sur x64 le
    // tableau commence à l'offset 8 (alignement de SOCKET), pas 4 ; en x86,
    // SOCKET fait 4 octets et le tableau commence à 4 (#779).
    let count = (std::ptr::read_unaligned(win as *const u32) as usize).min(FD_SETSIZE_WIN);
    (0..count).map(|i| fd_slot_read(win as usize, i)).collect()
}

unsafe fn write_fd_set(win: *mut WinFdSet, socks: &[usize]) {
    if win.is_null() {
        return;
    }
    let socks = &socks[..socks.len().min(FD_SETSIZE_WIN)];
    std::ptr::write_unaligned(win as *mut u32, socks.len() as u32);
    for (i, &s) in socks.iter().enumerate() {
        fd_slot_write(win as usize, i, s);
    }
}

/// Case `i` de fd_array : SOCKET de la taille d'un pointeur du programme.
unsafe fn fd_slot_read(set: usize, i: usize) -> usize {
    let ps = crate::wow32::pointer_size();
    let at = set + if ps == 4 { 4 } else { FD_ARRAY_OFFSET } + i * ps;
    if ps == 4 {
        std::ptr::read_unaligned(at as *const u32) as usize
    } else {
        std::ptr::read_unaligned(at as *const usize)
    }
}

unsafe fn fd_slot_write(set: usize, i: usize, v: usize) {
    let ps = crate::wow32::pointer_size();
    let at = set + if ps == 4 { 4 } else { FD_ARRAY_OFFSET } + i * ps;
    if ps == 4 {
        std::ptr::write_unaligned(at as *mut u32, v as u32)
    } else {
        std::ptr::write_unaligned(at as *mut usize, v)
    }
}

pub unsafe extern "win64" fn select(
    _nfds: i32,
    readfds: *mut WinFdSet,
    writefds: *mut WinFdSet,
    exceptfds: *mut WinFdSet,
    timeout: *const WinTimeval,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return SOCKET_ERROR;
    }
    let rfds = read_fd_set(readfds);
    let wfds = read_fd_set(writefds);
    let efds = read_fd_set(exceptfds);

    let mut nfds = 0i32;
    let mut rd: libc::fd_set = std::mem::zeroed();
    let mut wr: libc::fd_set = std::mem::zeroed();
    let mut ex: libc::fd_set = std::mem::zeroed();
    libc::FD_ZERO(&mut rd);
    libc::FD_ZERO(&mut wr);
    libc::FD_ZERO(&mut ex);

    // Une valeur qui n'est pas une socket (INVALID_SOCKET, fd fermé) fait
    // échouer l'appel avec WSAENOTSOCK, comme sous Windows, au lieu de
    // déborder le tableau de bits de `libc::FD_SET`.
    if rfds
        .iter()
        .chain(&wfds)
        .chain(&efds)
        .any(|&s| !fd_set_ok(s))
    {
        set_error(10038);
        return SOCKET_ERROR;
    }
    for &s in &rfds {
        libc::FD_SET(s as i32, &mut rd);
        nfds = nfds.max(s as i32 + 1);
    }
    for &s in &wfds {
        libc::FD_SET(s as i32, &mut wr);
        nfds = nfds.max(s as i32 + 1);
    }
    for &s in &efds {
        libc::FD_SET(s as i32, &mut ex);
        nfds = nfds.max(s as i32 + 1);
    }

    let mut tv = if timeout.is_null() {
        None
    } else {
        let t = std::ptr::read_unaligned(timeout);
        Some(libc::timeval {
            tv_sec: t.sec as i64,
            tv_usec: t.usec as i64,
        })
    };

    let rc = libc::select(
        nfds,
        if readfds.is_null() {
            std::ptr::null_mut()
        } else {
            &mut rd
        },
        if writefds.is_null() {
            std::ptr::null_mut()
        } else {
            &mut wr
        },
        if exceptfds.is_null() {
            std::ptr::null_mut()
        } else {
            &mut ex
        },
        tv.as_mut().map_or(std::ptr::null_mut(), |p| p),
    );
    crate::trace!("select(r={rfds:?}, w={wfds:?}, e={efds:?}) = {rc}");
    if rc < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }

    if !readfds.is_null() {
        let v: Vec<usize> = rfds
            .iter()
            .copied()
            .filter(|&s| libc::FD_ISSET(s as i32, &rd))
            .collect();
        write_fd_set(readfds, &v);
    }
    if !writefds.is_null() {
        let v: Vec<usize> = wfds
            .iter()
            .copied()
            .filter(|&s| libc::FD_ISSET(s as i32, &wr))
            .collect();
        write_fd_set(writefds, &v);
    }
    if !exceptfds.is_null() {
        let v: Vec<usize> = efds
            .iter()
            .copied()
            .filter(|&s| libc::FD_ISSET(s as i32, &ex))
            .collect();
        write_fd_set(exceptfds, &v);
    }
    rc
}

// ── WSAPoll (= poll) ────────────────────────────────────────────────────
#[repr(C)]
pub(crate) struct WsaPollFd {
    fd: usize,
    events: i16,
    revents: i16,
}

pub unsafe extern "win64" fn wsa_poll(fds: *mut WsaPollFd, nfds: u32, timeout: i32) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return SOCKET_ERROR;
    }
    if fds.is_null() && nfds > 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let n = nfds as usize;
    // WSAPOLLFD : { SOCKET fd; SHORT events; SHORT revents; }, 8 octets en
    // x86 (SOCKET sur 4), 16 en x64.
    let stride = if crate::wow32::pointer_size() == 4 {
        8
    } else {
        16
    };
    let ps = crate::wow32::pointer_size();
    let base = fds as usize;
    let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(n);
    for i in 0..n {
        let at = base + i * stride;
        let fd = crate::wow32::read_ptr(at);
        let events = std::ptr::read_unaligned((at + ps) as *const i16);
        pfds.push(libc::pollfd {
            fd: fd as i32,
            events: poll_events_to_linux(events),
            revents: 0,
        });
    }
    let rc = libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, timeout);
    if rc < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    for (i, pfd) in pfds.iter().enumerate().take(n) {
        let at = base + i * stride + ps + 2;
        std::ptr::write_unaligned(at as *mut i16, poll_events_to_win(pfd.revents));
    }
    rc
}

// ── inet_ntoa / inet_ntop / inet_pton ───────────────────────────────────
pub unsafe extern "win64" fn inet_ntoa(addr: u32) -> *const u8 {
    let octets = addr.to_ne_bytes();
    let s = format!("{}.{}.{}.{}\0", octets[0], octets[1], octets[2], octets[3]);
    // Tampon par thread sur le tas, comme Windows : une statique du binaire
    // est au-dessus de 4 Go, hors de portée d'un programme x86 (PuTTY 32
    // bits plantait en lisant l'adresse qu'il venait de résoudre).
    thread_local! {
        static NTOA: std::cell::RefCell<Box<[u8; 16]>> = std::cell::RefCell::new(Box::new([0u8; 16]));
    }
    NTOA.with(|buf| {
        let mut g = buf.borrow_mut();
        let b = s.as_bytes();
        let n = b.len().min(16);
        g[..n].copy_from_slice(&b[..n]);
        g.as_ptr()
    })
}

pub unsafe extern "win64" fn inet_pton(af: i32, src: *const u8, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let text = crate::dlls::cstr(src);
    let c = match CString::new(text) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let rc = posix_inet_pton(af_in(af), c.as_ptr(), dst as *mut c_void);
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        rc
    }
}

pub unsafe extern "win64" fn inet_ntop(
    af: i32,
    src: *const u8,
    dst: *mut u8,
    size: usize,
) -> *const u8 {
    if src.is_null() || dst.is_null() {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    let r = posix_inet_ntop(
        af_in(af),
        src as *const c_void,
        dst as *mut c_char,
        size as u32,
    );
    if r.is_null() {
        set_error(last_error());
        std::ptr::null()
    } else {
        dst
    }
}

pub unsafe extern "win64" fn inet_ntop_w(
    af: i32,
    src: *const u8,
    dst: *mut u16,
    size: usize,
) -> *const u16 {
    if src.is_null() || dst.is_null() {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    let mut buf = [0u8; 64];
    let r = posix_inet_ntop(
        af_in(af),
        src as *const c_void,
        buf.as_mut_ptr() as *mut c_char,
        64,
    );
    if r.is_null() {
        set_error(last_error());
        return std::ptr::null();
    }
    let len = buf.iter().position(|b| *b == 0).unwrap_or(63);
    if len + 1 > size {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    for (i, &b) in buf[..len].iter().enumerate() {
        *dst.add(i) = b as u16;
    }
    *dst.add(len) = 0;
    dst
}

pub unsafe extern "win64" fn inet_pton_w(af: i32, src: *const u16, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let n = crate::printf::wide_len(src);
    let s = match String::from_utf16(std::slice::from_raw_parts(src, n)) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let c = match CString::new(s) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let rc = posix_inet_pton(af_in(af), c.as_ptr(), dst as *mut c_void);
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        rc
    }
}

// ── gethostbyname / gethostbyaddr ───────────────────────────────────────
pub unsafe extern "win64" fn gethostbyname(name: *const u8) -> *const WinHostent {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return std::ptr::null();
    }
    if name.is_null() {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    match resolve_v4(&crate::dlls::cstr(name)) {
        Ok((host, addrs)) => publish_hostent(&host, &addrs),
        Err(e) => {
            set_error(e);
            std::ptr::null()
        }
    }
}

pub unsafe extern "win64" fn gethostbyaddr(
    addr: *const u8,
    len: i32,
    addrtype: i32,
) -> *const WinHostent {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return std::ptr::null();
    }
    if addr.is_null() || len < 4 || addrtype != libc::AF_INET {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    let ip: [u8; 4] = [*addr, *addr.add(1), *addr.add(2), *addr.add(3)];
    publish_hostent(&reverse_v4(ip), &[ip])
}

// ── WSAAsyncGetHostByName / WSAAsyncGetHostByAddr ───────────────────────

fn async_requests() -> &'static Mutex<std::collections::HashSet<usize>> {
    static R: OnceLock<Mutex<std::collections::HashSet<usize>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Lance la résolution dans un thread ; à la fin, le HOSTENT est écrit
/// dans `buf` et `msg` est posté à `hwnd` (wParam = poignée, lParam =
/// WSAMAKEASYNCREPLY(taille, erreur)), sauf si la demande a été annulée.
fn start_async(
    hwnd: usize,
    msg: u32,
    buf: usize,
    buflen: usize,
    job: impl FnOnce() -> Result<(Vec<u8>, Vec<[u8; 4]>), i32> + Send + 'static,
) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
    let handle = NEXT.fetch_add(1, Ordering::Relaxed);
    let ps = crate::wow32::pointer_size();
    lock(async_requests()).insert(handle);
    std::thread::spawn(move || {
        let (size, err) = match job() {
            Ok((host, addrs)) => {
                let out = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, buflen) };
                let need = pack_hostent(out, buf, &host, &addrs, ps);
                (need, if need > buflen { WSAENOBUFS } else { 0 })
            }
            Err(e) => (0, e),
        };
        if lock(async_requests()).remove(&handle) {
            let lp = (((err as u32) << 16) | (size as u32 & 0xFFFF)) as isize;
            crate::dlls::gui::post_to_window(
                hwnd,
                crate::dlls::gui::Msg::new(hwnd, msg, handle, lp),
            );
        }
    });
    handle
}

pub unsafe extern "win64" fn wsa_async_get_host_by_name(
    hwnd: usize,
    msg: u32,
    name: *const u8,
    buf: *mut u8,
    buflen: i32,
) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return 0;
    }
    if name.is_null() || buf.is_null() || buflen <= 0 {
        set_error(WSAEFAULT);
        return 0;
    }
    let text = crate::dlls::cstr(name).to_string();
    start_async(hwnd, msg, buf as usize, buflen as usize, move || unsafe {
        resolve_v4(&text)
    })
}

pub unsafe extern "win64" fn wsa_async_get_host_by_addr(
    hwnd: usize,
    msg: u32,
    addr: *const u8,
    len: i32,
    addrtype: i32,
    buf: *mut u8,
    buflen: i32,
) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return 0;
    }
    if addr.is_null() || len < 4 || addrtype != libc::AF_INET || buf.is_null() || buflen <= 0 {
        set_error(WSAEFAULT);
        return 0;
    }
    let ip: [u8; 4] = [*addr, *addr.add(1), *addr.add(2), *addr.add(3)];
    start_async(hwnd, msg, buf as usize, buflen as usize, move || unsafe {
        Ok((reverse_v4(ip), vec![ip]))
    })
}

/// WSACancelAsyncRequest : la réponse ne sera pas postée ; WSAEINVAL si la
/// demande est déjà terminée (ou inconnue).
pub extern "win64" fn wsa_cancel_async_request(handle: usize) -> i32 {
    if lock(async_requests()).remove(&handle) {
        0
    } else {
        set_error(WSAEINVAL);
        SOCKET_ERROR
    }
}

// ── getservbyname / getservbyport ───────────────────────────────────────
pub unsafe extern "win64" fn getservbyname(name: *const u8, proto: *const u8) -> *const WinServent {
    if name.is_null() {
        set_error(WSAEFAULT);
        return std::ptr::null();
    }
    let nm = crate::dlls::cstr(name);
    let pr = if proto.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(proto)
    };
    let nm_c = match CString::new(nm) {
        Ok(c) => c,
        Err(_) => return std::ptr::null(),
    };
    let pr_c = if pr.is_empty() {
        None
    } else {
        CString::new(pr).ok()
    };
    let r = libc::getservbyname(
        nm_c.as_ptr(),
        pr_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
    );
    if r.is_null() {
        set_error(WSAHOST_NOT_FOUND);
        return std::ptr::null();
    }
    fill_servent(&*r)
}

pub unsafe extern "win64" fn getservbyport(port: i32, proto: *const u8) -> *const WinServent {
    let pr = if proto.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(proto)
    };
    let pr_c = if pr.is_empty() {
        None
    } else {
        CString::new(pr).ok()
    };
    let r = libc::getservbyport(port, pr_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()));
    if r.is_null() {
        set_error(WSAHOST_NOT_FOUND);
        return std::ptr::null();
    }
    fill_servent(&*r)
}

unsafe fn fill_servent(se: &libc::servent) -> *const WinServent {
    let buf = SERVENT_BUF.get_or_init(|| {
        Mutex::new(ServentBuf {
            servent: std::mem::zeroed(),
            name: Vec::new(),
            proto: Vec::new(),
            aliases_ptr: Vec::new(),
            servent32: Vec::new(),
        })
    });
    let mut g = lock(buf);
    g.name = std::ffi::CStr::from_ptr(se.s_name).to_bytes().to_vec();
    g.name.push(0);
    g.proto = std::ffi::CStr::from_ptr(se.s_proto).to_bytes().to_vec();
    g.proto.push(0);
    g.aliases_ptr = vec![std::ptr::null_mut()];
    g.servent.s_name = g.name.as_mut_ptr() as *mut c_char;
    g.servent.s_aliases = g.aliases_ptr.as_mut_ptr();
    g.servent.s_port = se.s_port as i16;
    g.servent.s_proto = g.proto.as_mut_ptr() as *mut c_char;
    if crate::wow32::pointer_size() == 4 {
        let name = g.name.as_ptr() as usize as u32;
        let proto = g.proto.as_ptr() as usize as u32;
        g.servent32 = vec![name, 0, (se.s_port as u16) as u32, proto, 0];
        let base = g.servent32.as_mut_ptr();
        *base.add(1) = base.add(4) as usize as u32;
        return base as *const WinServent;
    }
    &g.servent as *const WinServent
}

// ── getnameinfo / GetNameInfoW ──────────────────────────────────────────
pub unsafe extern "win64" fn getnameinfo(
    sa: *const u8,
    sa_len: i32,
    host: *mut u8,
    host_len: u32,
    serv: *mut u8,
    serv_len: u32,
    flags: i32,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        return WSA_NOT_INITIALISED;
    }
    if sa.is_null() {
        return WSAEFAULT;
    }
    // translate Windows NI_ flags to Linux NI_ flags
    let mut lflags = 0i32;
    if flags & NI_NUMERICHOST != 0 {
        lflags |= libc::NI_NUMERICHOST;
    }
    if flags & NI_NUMERICSERV != 0 {
        lflags |= libc::NI_NUMERICSERV;
    }
    if flags & NI_NOFQDN != 0 {
        lflags |= libc::NI_NOFQDN;
    }
    if flags & NI_NAMEREQD != 0 {
        lflags |= libc::NI_NAMEREQD;
    }
    if flags & NI_DGRAM != 0 {
        lflags |= libc::NI_DGRAM;
    }
    let sa_linux = addr_in(sa, sa_len);
    let rc = libc::getnameinfo(
        &sa_linux as *const _ as *const libc::sockaddr,
        sa_len as u32,
        if host.is_null() {
            std::ptr::null_mut()
        } else {
            host as *mut c_char
        },
        host_len,
        if serv.is_null() {
            std::ptr::null_mut()
        } else {
            serv as *mut c_char
        },
        serv_len,
        lflags,
    );
    if rc != 0 {
        gai_error(rc)
    } else {
        0
    }
}

pub unsafe extern "win64" fn get_name_info_w(
    sa: *const u8,
    sa_len: i32,
    host: *mut u16,
    host_len: u32,
    serv: *mut u16,
    serv_len: u32,
    flags: i32,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        return WSA_NOT_INITIALISED;
    }
    if sa.is_null() || (!host.is_null() && host_len == 0) || (!serv.is_null() && serv_len == 0) {
        return WSAEFAULT;
    }
    let mut hbuf = vec![0u8; host_len.max(1) as usize];
    let mut sbuf = vec![0u8; serv_len.max(1) as usize];
    let rc = getnameinfo(
        sa,
        sa_len,
        if host.is_null() {
            std::ptr::null_mut()
        } else {
            hbuf.as_mut_ptr()
        },
        host_len,
        if serv.is_null() {
            std::ptr::null_mut()
        } else {
            sbuf.as_mut_ptr()
        },
        serv_len,
        flags,
    );
    if rc != 0 {
        return rc;
    }
    if !host.is_null() {
        let len = hbuf.iter().position(|b| *b == 0).unwrap_or(hbuf.len());
        let n = len.min(host_len as usize - 1);
        for (i, &b) in hbuf[..n].iter().enumerate() {
            *host.add(i) = b as u16;
        }
        *host.add(n) = 0;
    }
    if !serv.is_null() {
        let len = sbuf.iter().position(|b| *b == 0).unwrap_or(sbuf.len());
        let n = len.min(serv_len as usize - 1);
        for (i, &b) in sbuf[..n].iter().enumerate() {
            *serv.add(i) = b as u16;
        }
        *serv.add(n) = 0;
    }
    0
}

// ── WSASocketW ──────────────────────────────────────────────────────────
pub extern "win64" fn wsa_socket_w(
    af: i32,
    kind: i32,
    protocol: i32,
    _info: usize,
    _group: u32,
    flags: u32,
) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return INVALID_SOCKET;
    }
    // WSA_FLAG_OVERLAPPED (0x01) est le drapeau par défaut de presque tous
    // les appels : il ne rend pas la socket non bloquante (ioctlsocket FIONBIO
    // s'en charge).
    let _ = flags;
    // Non héritable par défaut, comme sous Windows (#1347).
    let s = unsafe { libc::socket(af_in(af), kind | libc::SOCK_CLOEXEC, protocol) };
    if s < 0 {
        set_error(last_error());
        INVALID_SOCKET
    } else {
        register_socket(s as usize);
        s as usize
    }
}

// ── WSASend / WSARecv ───────────────────────────────────────────────────
pub unsafe extern "win64" fn wsa_send(
    s: usize,
    bufs: *const WsaBuf,
    buf_count: u32,
    bytes_sent: *mut u32,
    flags: u32,
    overlapped: usize,
    completion: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if bufs.is_null() || buf_count == 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    if overlapped::wanted(overlapped, completion) {
        return overlapped::send(s, bufs, buf_count, bytes_sent, flags, overlapped);
    }
    let n = buf_count as usize;
    let mut iovs: Vec<libc::iovec> = Vec::with_capacity(n);
    for i in 0..n {
        let (len, buf) = read_wsabuf(bufs as usize, i);
        iovs.push(libc::iovec {
            iov_base: buf as *mut c_void,
            iov_len: len,
        });
    }
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = iovs.as_mut_ptr();
    msg.msg_iovlen = n;
    let r = libc::sendmsg(s as i32, &msg, flags as i32 | libc::MSG_NOSIGNAL);
    if r < 0 {
        let e = last_error();
        if e == WSAEWOULDBLOCK {
            rearm_write(s);
        }
        set_error(e);
        SOCKET_ERROR
    } else {
        if !bytes_sent.is_null() {
            *bytes_sent = r as u32;
        }
        0
    }
}

pub unsafe extern "win64" fn wsa_recv(
    s: usize,
    bufs: *mut WsaBuf,
    buf_count: u32,
    bytes_recv: *mut u32,
    flags: *mut u32,
    overlapped: usize,
    completion: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    async_arm(s, FD_READ | FD_OOB);
    if bufs.is_null() || buf_count == 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    if overlapped::wanted(overlapped, completion) {
        return overlapped::recv(s, bufs, buf_count, bytes_recv, flags, overlapped);
    }
    let n = buf_count as usize;
    let mut iovs: Vec<libc::iovec> = Vec::with_capacity(n);
    for i in 0..n {
        let (len, buf) = read_wsabuf(bufs as usize, i);
        iovs.push(libc::iovec {
            iov_base: buf as *mut c_void,
            iov_len: len,
        });
    }
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = iovs.as_mut_ptr();
    msg.msg_iovlen = n;
    let fl = if flags.is_null() { 0 } else { *flags as i32 };
    let r = libc::recvmsg(s as i32, &mut msg, fl);
    if r < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        if !bytes_recv.is_null() {
            *bytes_recv = r as u32;
        }
        if !flags.is_null() {
            *flags = msg.msg_flags as u32;
        }
        0
    }
}

// ── WSAConnect ──────────────────────────────────────────────────────────
pub unsafe extern "win64" fn wsa_connect(
    s: usize,
    name: *const u8,
    namelen: i32,
    _caller_data: usize,
    _callee_data: usize,
    _sqos: usize,
    _gqos: usize,
) -> i32 {
    connect(s, name, namelen)
}

// ── WSA Event API ───────────────────────────────────────────────────────
pub extern "win64" fn wsa_create_event() -> usize {
    let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    if fd < 0 {
        set_error(last_error());
        return WSA_INVALID_EVENT;
    }
    // Numéro de handle de kernel32, multiple de 4, et non le descripteur :
    // un numéro impair passé en OVERLAPPED.hEvent perdait son bit 0 (#1360).
    let handle = crate::dlls::kernel32::new_handle_number();
    lock(events()).insert(
        handle,
        EventState {
            fd,
            triggered: false,
        },
    );
    handle
}

pub extern "win64" fn wsa_close_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.remove(&h) {
        unsafe {
            libc::close(st.fd);
        }
        lock(event_assoc()).retain(|_, a| a.event != h);
        1 // TRUE
    } else {
        set_error(WSAEINVAL);
        0
    }
}

pub extern "win64" fn wsa_set_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.get_mut(&h) {
        st.triggered = true;
        let val: u64 = 1;
        unsafe {
            libc::write(st.fd, &val as *const _ as *const c_void, 8);
        }
        1
    } else {
        drop(ev);
        // Événement de CreateEvent : WSASetEvent agit aussi sur lui (#1360).
        if crate::dlls::kernel32::is_event_handle(h) {
            return crate::dlls::kernel32::set_event(h);
        }
        set_error(WSAEINVAL);
        0
    }
}

pub extern "win64" fn wsa_reset_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.get_mut(&h) {
        st.triggered = false;
        let mut val: u64 = 0;
        unsafe {
            libc::read(st.fd, &mut val as *mut _ as *mut c_void, 8);
        }
        1
    } else {
        drop(ev);
        // WSAEnumNetworkEvents de PuTTY passe un événement de CreateEvent.
        if crate::dlls::kernel32::is_event_handle(h) {
            return crate::dlls::kernel32::reset_event(h);
        }
        set_error(WSAEINVAL);
        0
    }
}

pub unsafe extern "win64" fn wsa_wait_for_multiple_events(
    count: u32,
    events_arr: *const usize,
    wait_all: i32,
    timeout: u32,
    _alertable: i32,
) -> u32 {
    if count == 0 || count > WSA_MAXIMUM_WAIT_EVENTS || events_arr.is_null() {
        set_error(WSAEINVAL);
        return WSA_WAIT_FAILED;
    }
    let n = count as usize;
    let handles: Vec<usize> = (0..n)
        .map(|i| std::ptr::read_unaligned(events_arr.add(i)))
        .collect();
    let ev = events();
    // Les n premières entrées sont les eventfd ; ensuite les sockets
    // associées par WSAEventSelect (elles n'étaient jamais surveillées, donc
    // l'attente ne se terminait jamais sur l'activité réseau).
    let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(n);
    let mut owners: Vec<usize> = Vec::new();
    {
        let g = lock(ev);
        for &h in &handles {
            if let Some(st) = g.get(&h) {
                pfds.push(libc::pollfd {
                    fd: st.fd,
                    events: libc::POLLIN,
                    revents: 0,
                });
            } else {
                set_error(WSAEINVAL);
                return WSA_WAIT_FAILED;
            }
        }
        let assoc = lock(event_assoc());
        for (i, &h) in handles.iter().enumerate() {
            for (&sock, a) in assoc.iter() {
                if a.event == h {
                    let e = assoc_poll_events(a);
                    if e != 0 {
                        pfds.push(libc::pollfd {
                            fd: sock as i32,
                            events: e,
                            revents: 0,
                        });
                        owners.push(i);
                    }
                }
            }
        }
    }
    let ms = if timeout == u32::MAX {
        -1i32
    } else {
        timeout as i32
    };
    let rc = libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, ms);
    if rc < 0 {
        set_error(last_error());
        return WSA_WAIT_FAILED;
    }
    if rc == 0 {
        return WSA_WAIT_TIMEOUT;
    }
    let mut ready = vec![false; n];
    let mut by_event = vec![false; n];
    for (i, p) in pfds.iter().enumerate() {
        if p.revents == 0 {
            continue;
        }
        if i < n {
            if p.revents & libc::POLLIN != 0 {
                ready[i] = true;
                by_event[i] = true;
            }
        } else {
            let o = owners[i - n];
            if p.revents & (p.events | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                ready[o] = true;
            }
        }
    }
    if wait_all != 0 {
        if ready.iter().any(|r| !r) {
            return WSA_WAIT_TIMEOUT;
        }
        for (i, &h) in handles.iter().enumerate() {
            if by_event[i] {
                wsa_reset_event(h);
            }
        }
        return WSA_WAIT_EVENT_0;
    }
    for i in 0..n {
        if ready[i] {
            if by_event[i] {
                wsa_reset_event(handles[i]);
            }
            return WSA_WAIT_EVENT_0 + i as u32;
        }
    }
    WSA_WAIT_TIMEOUT
}

// ── WSAEventSelect / WSAEnumNetworkEvents ───────────────────────────────
pub extern "win64" fn wsa_event_select(s: usize, event: usize, mask: i32) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    // set socket non-blocking
    unsafe {
        let flags = libc::fcntl(s as i32, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(s as i32, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    lock(async_sel()).remove(&s);
    if event == 0 || mask == 0 {
        lock(event_assoc()).remove(&s);
    } else {
        lock(event_assoc()).insert(
            s,
            EventAssoc {
                event,
                mask,
                write_armed: true,
            },
        );
    }
    0
}

pub unsafe extern "win64" fn wsa_enum_network_events(
    s: usize,
    event: usize,
    net_events: *mut WsaNetworkEvents,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if net_events.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    if event != 0 {
        wsa_reset_event(event);
    }

    let assoc = lock(event_assoc()).get(&s).copied();
    let mask = assoc.map(|a| a.mask).unwrap_or(0);
    let write_armed = assoc.map(|a| a.write_armed).unwrap_or(false);
    let mut result = WsaNetworkEvents::default();

    // État de la socket, sans attente.
    let mut pfd = libc::pollfd {
        fd: s as i32,
        events: libc::POLLIN
            | libc::POLLOUT
            | libc::POLLPRI
            | libc::POLLRDHUP
            | libc::POLLHUP
            | libc::POLLERR,
        revents: 0,
    };
    libc::poll(&mut pfd, 1, 0);
    let r = pfd.revents;

    // FD_ACCEPT ne concerne qu'une socket d'écoute, FD_READ, FD_WRITE,
    // FD_CONNECT et FD_CLOSE qu'une socket connectée. PuTTY (WinSCP) demande
    // les six sur chaque socket : annoncer FD_ACCEPT sur sa connexion SSH
    // lui faisait appeler accept() puis suivre un pointeur nul (#1350).
    if socket_listening(s) {
        if r & libc::POLLIN != 0 && mask & FD_ACCEPT != 0 {
            result.network_events |= FD_ACCEPT;
        }
    } else {
        // Erreur en attente (connexion refusée…) : rendue dans iErrorCode,
        // indice FD_CONNECT_BIT (4) tant que la connexion n'a pas abouti.
        let pending_error = if r & libc::POLLERR != 0 {
            let mut e = 0i32;
            let mut len = 4 as libc::socklen_t;
            libc::getsockopt(
                s as i32,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                &mut e as *mut i32 as *mut c_void,
                &mut len,
            );
            (e != 0).then(|| error_from_errno(e))
        } else {
            None
        };
        if r & libc::POLLIN != 0 && mask & FD_READ != 0 {
            result.network_events |= FD_READ;
        }
        if r & libc::POLLPRI != 0 && mask & FD_OOB != 0 {
            result.network_events |= FD_OOB;
        }
        if write_armed && mask & FD_CONNECT != 0 {
            if let Some(e) = pending_error {
                result.network_events |= FD_CONNECT;
                result.error_code[FD_CONNECT_BIT] = e;
            } else if r & libc::POLLOUT != 0 {
                result.network_events |= FD_CONNECT;
            }
        }
        if r & libc::POLLOUT != 0 && mask & FD_WRITE != 0 && write_armed {
            result.network_events |= FD_WRITE;
        }
        if result.network_events & (FD_WRITE | FD_CONNECT) != 0 {
            // Signalés une fois : réarmés par le prochain envoi en WSAEWOULDBLOCK.
            if let Some(a) = lock(event_assoc()).get_mut(&s) {
                a.write_armed = false;
            }
        }
        if r & (libc::POLLRDHUP | libc::POLLHUP) != 0
            && mask & FD_CLOSE != 0
            && result.network_events & FD_CONNECT == 0
        {
            result.network_events |= FD_CLOSE;
            if let Some(e) = pending_error {
                result.error_code[FD_CLOSE_BIT] = e;
            }
        }
    }

    std::ptr::write_unaligned(net_events, result);
    0
}

// ── WSAIoctl ────────────────────────────────────────────────────────────
const SIO_KEEPALIVE_VALS: u32 = 0x98000004;
const SIO_BASE_HANDLE: u32 = 0x48000022;
const SIO_BSP_HANDLE: u32 = 0x4800001B;
const SIO_BSP_HANDLE_SELECT: u32 = 0x4800001C;
const SIO_BSP_HANDLE_POLL: u32 = 0x4800001D;
const SIO_LOOPBACK_FAST_PATH: u32 = 0x98000010;
const SIO_IDEAL_SEND_BACKLOG_QUERY: u32 = 0x4004747B;
const SIO_TCP_INITIAL_RTO: u32 = 0x98000011;

pub unsafe extern "win64" fn wsa_ioctl(
    s: usize,
    ioctl_code: u32,
    in_buf: *const u8,
    in_len: u32,
    out_buf: *mut u8,
    out_len: u32,
    bytes_returned: *mut u32,
    _overlapped: usize,
    _completion: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    crate::trace!("WSAIoctl({s}, {ioctl_code:#010x}, entrée {in_len} o, sortie {out_len} o)");
    match ioctl_code {
        FIONBIO => {
            if in_buf.is_null() || in_len < 4 {
                set_error(WSAEFAULT);
                return SOCKET_ERROR;
            }
            let val = std::ptr::read_unaligned(in_buf as *const u32);
            let mut v = val;
            ioctlsocket(s, FIONBIO, &mut v)
        }
        FIONREAD => {
            if out_buf.is_null() || out_len < 4 {
                set_error(WSAEFAULT);
                return SOCKET_ERROR;
            }
            let mut v = 0u32;
            let rc = ioctlsocket(s, FIONREAD, &mut v);
            if rc == 0 {
                std::ptr::write_unaligned(out_buf as *mut u32, v);
                if !bytes_returned.is_null() {
                    *bytes_returned = 4;
                }
            }
            rc
        }
        SIO_KEEPALIVE_VALS => {
            if in_buf.is_null() || in_len < 12 {
                set_error(WSAEFAULT);
                return SOCKET_ERROR;
            }
            let onoff = std::ptr::read_unaligned(in_buf as *const u32);
            let keepalive_time = std::ptr::read_unaligned((in_buf.add(4)) as *const u32);
            let keepalive_interval = std::ptr::read_unaligned((in_buf.add(8)) as *const u32);
            let v: i32 = if onoff != 0 { 1 } else { 0 };
            libc::setsockopt(
                s as i32,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &v as *const _ as *const c_void,
                4,
            );
            if onoff != 0 {
                let idle = (keepalive_time / 1000).max(1) as i32;
                let intvl = (keepalive_interval / 1000).max(1) as i32;
                libc::setsockopt(
                    s as i32,
                    libc::IPPROTO_TCP,
                    libc::TCP_KEEPIDLE,
                    &idle as *const _ as *const c_void,
                    4,
                );
                libc::setsockopt(
                    s as i32,
                    libc::IPPROTO_TCP,
                    libc::TCP_KEEPINTVL,
                    &intvl as *const _ as *const c_void,
                    4,
                );
            }
            if !bytes_returned.is_null() {
                *bytes_returned = 0;
            }
            0
        }
        // SIO_BASE_HANDLE et SIO_BSP_HANDLE(_SELECT/_POLL) : pas de
        // fournisseur en couches, la socket est sa propre base.
        SIO_BASE_HANDLE | SIO_BSP_HANDLE | SIO_BSP_HANDLE_SELECT | SIO_BSP_HANDLE_POLL => {
            if out_buf.is_null() || out_len < 8 {
                set_error(WSAEFAULT);
                return SOCKET_ERROR;
            }
            // Seules les boucles d'événements demandent la socket de base,
            // juste avant de la surveiller (uv_poll_init_socket de libuv,
            // dont le contrat est de passer la socket en mode non bloquant).
            // Bun (Claude Code) lit ensuite jusqu'à WSAEWOULDBLOCK : en mode
            // bloquant, la deuxième lecture d'une connexion HTTP gardée
            // ouverte figeait le programme.
            if ioctl_code == SIO_BASE_HANDLE {
                let flags = libc::fcntl(s as i32, libc::F_GETFL);
                if flags >= 0 && flags & libc::O_NONBLOCK == 0 {
                    libc::fcntl(s as i32, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
            std::ptr::write_unaligned(out_buf as *mut usize, s);
            if !bytes_returned.is_null() {
                *bytes_returned = 8;
            }
            0
        }
        // Réglages de performance sans équivalent Linux (le noyau gère déjà
        // le délai initial de retransmission et la boucle locale rapide).
        overlapped::SIO_GET_EXTENSION_FUNCTION_POINTER => {
            overlapped::extension_function(in_buf, in_len, out_buf, out_len, bytes_returned)
        }
        // SIO_IDEAL_SEND_BACKLOG_QUERY : quantité idéale de données en
        // attente d'envoi ; le tampon d'envoi du noyau en tient lieu.
        // PuTTY et WinSCP la demandent à chaque tour de boucle.
        SIO_IDEAL_SEND_BACKLOG_QUERY => {
            if out_buf.is_null() || out_len < 4 {
                set_error(WSAEFAULT);
                return SOCKET_ERROR;
            }
            let v = sock_flag(s as i32, libc::SO_SNDBUF).max(0) as u32;
            std::ptr::write_unaligned(out_buf as *mut u32, v);
            if !bytes_returned.is_null() {
                *bytes_returned = 4;
            }
            0
        }
        SIO_LOOPBACK_FAST_PATH | SIO_TCP_INITIAL_RTO => {
            if !bytes_returned.is_null() {
                *bytes_returned = 0;
            }
            0
        }
        _ => {
            eprintln!("[ws2_32] WSAIoctl: code non supporté {:#010x}", ioctl_code);
            set_error(WSAEOPNOTSUPP);
            SOCKET_ERROR
        }
    }
}

// ── WSADuplicateSocketW (stub — single-process) ─────────────────────────
pub unsafe extern "win64" fn wsa_duplicate_socket_w(s: usize, _pid: u32, info: *mut u8) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if info.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    // fill the WSAPROTOCOL_INFOW struct with minimal data
    std::ptr::write_bytes(info, 0, 628); // sizeof(WSAPROTOCOL_INFOW) sur x64
    0
}

// ── WSAStringToAddressW / WSAAddressToStringW ───────────────────────────
pub unsafe extern "win64" fn wsa_string_to_address_w(
    addr_string: *const u16,
    af: i32,
    _proto_info: usize,
    addr: *mut u8,
    addr_len: *mut i32,
) -> i32 {
    if addr_string.is_null() || addr.is_null() || addr_len.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let n = crate::printf::wide_len(addr_string);
    let s = match String::from_utf16(std::slice::from_raw_parts(addr_string, n)) {
        Ok(s) => s,
        Err(_) => {
            set_error(WSAEINVAL);
            return SOCKET_ERROR;
        }
    };
    // split off optional :port
    let (host, port) = if let Some(idx) = s.rfind(':') {
        let p = s[idx + 1..].parse::<u16>().unwrap_or(0);
        (&s[..idx], p)
    } else {
        (s.as_str(), 0u16)
    };

    if af == libc::AF_INET {
        if *addr_len < 16 {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let ip: std::net::Ipv4Addr = match host.parse() {
            Ok(ip) => ip,
            Err(_) => {
                set_error(WSAEINVAL);
                return SOCKET_ERROR;
            }
        };
        let mut sin: libc::sockaddr_in = std::mem::zeroed();
        sin.sin_family = libc::AF_INET as u16;
        sin.sin_port = port.to_be();
        sin.sin_addr.s_addr = u32::from_ne_bytes(ip.octets());
        std::ptr::copy_nonoverlapping(&sin as *const _ as *const u8, addr, 16);
        *addr_len = 16;
        0
    } else if af == libc::AF_INET6 || af == WIN_AF_INET6 {
        if *addr_len < 28 {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let c = match CString::new(host) {
            Ok(c) => c,
            Err(_) => {
                set_error(WSAEINVAL);
                return SOCKET_ERROR;
            }
        };
        let mut sin6: libc::sockaddr_in6 = std::mem::zeroed();
        sin6.sin6_family = WIN_AF_INET6 as u16;
        sin6.sin6_port = port.to_be();
        if posix_inet_pton(
            libc::AF_INET6,
            c.as_ptr(),
            &mut sin6.sin6_addr as *mut _ as *mut c_void,
        ) != 1
        {
            set_error(WSAEINVAL);
            return SOCKET_ERROR;
        }
        std::ptr::copy_nonoverlapping(&sin6 as *const _ as *const u8, addr, 28);
        *addr_len = 28;
        0
    } else {
        set_error(10047);
        SOCKET_ERROR
    }
}

pub unsafe extern "win64" fn wsa_address_to_string_w(
    addr: *const u8,
    addr_len: u32,
    _proto_info: usize,
    string: *mut u16,
    string_len: *mut u32,
) -> i32 {
    if addr.is_null() || string.is_null() || string_len.is_null() {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let family = std::ptr::read_unaligned(addr as *const u16);
    let text = if family == libc::AF_INET as u16 {
        if addr_len < 16 {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let sin = std::ptr::read_unaligned(addr as *const libc::sockaddr_in);
        let ip = std::net::Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
        let port = u16::from_be(sin.sin_port);
        if port != 0 {
            format!("{}:{}", ip, port)
        } else {
            format!("{}", ip)
        }
    } else if family == libc::AF_INET6 as u16 || family == WIN_AF_INET6 as u16 {
        if addr_len < 28 {
            set_error(WSAEFAULT);
            return SOCKET_ERROR;
        }
        let sin6 = std::ptr::read_unaligned(addr as *const libc::sockaddr_in6);
        let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
        let port = u16::from_be(sin6.sin6_port);
        if port != 0 {
            format!("[{}]:{}", ip, port)
        } else {
            format!("{}", ip)
        }
    } else {
        set_error(10047);
        return SOCKET_ERROR;
    };
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    if (*string_len as usize) < wide.len() {
        *string_len = wide.len() as u32;
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), string, wide.len());
    *string_len = wide.len() as u32;
    0
}

// ── WSAAsyncSelect (stub: sets non-blocking, tracks mask) ───────────────
pub extern "win64" fn wsa_async_select(s: usize, hwnd: usize, msg: u32, mask: i32) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    // Comme sous Windows : la socket passe en non bloquant (et y reste
    // même si le masque est vidé) et WSAEventSelect est annulé.
    unsafe {
        let flags = libc::fcntl(s as i32, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(s as i32, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    lock(event_assoc()).remove(&s);
    let mut map = lock(async_sel());
    if hwnd == 0 || mask == 0 {
        map.remove(&s);
    } else {
        let connecting = map.get(&s).is_some_and(|a| a.connecting);
        map.insert(
            s,
            AsyncSel {
                hwnd,
                msg,
                mask,
                // FD_WRITE est signalé dès que la socket est connectée.
                armed: mask & (FD_READ | FD_OOB | FD_ACCEPT | FD_CLOSE | FD_WRITE),
                connecting,
            },
        );
    }
    0
}

/// Longueur d'une adresse rangée par AcceptEx, d'après sa famille.
fn accept_ex_sockaddr_len(block: &[u8]) -> usize {
    let len = match block.get(..2).map(|f| u16::from_le_bytes([f[0], f[1]])) {
        Some(2) => 16,  // AF_INET : sockaddr_in
        Some(23) => 28, // AF_INET6 : sockaddr_in6
        _ => 0,
    };
    len.min(block.len())
}

/// `GetAcceptExSockaddrs` (mswsock, wsock32 ordinal 1142) : après les
/// `data_len` octets reçus, AcceptEx range l'adresse locale puis
/// l'adresse distante, chacune dans un bloc de `local_len` /
/// `remote_len` octets. Rend des pointeurs vers ces adresses et leurs
/// longueurs (#1380 : Steam.exe l'importe par ordinal).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn get_accept_ex_sockaddrs(
    buf: *const u8,
    data_len: u32,
    local_len: u32,
    remote_len: u32,
    local: usize,
    local_size: *mut i32,
    remote: usize,
    remote_size: *mut i32,
) {
    if buf.is_null() {
        return;
    }
    let local_at = buf.add(data_len as usize);
    let remote_at = local_at.add(local_len as usize);
    let blocks = [
        (local_at, local_len, local, local_size),
        (remote_at, remote_len, remote, remote_size),
    ];
    for (at, len, out, size) in blocks {
        let block = std::slice::from_raw_parts(at, len as usize);
        let n = accept_ex_sockaddr_len(block);
        if out != 0 {
            crate::wow32::write_ptr(out, if n == 0 { 0 } else { at as usize });
        }
        if !size.is_null() {
            std::ptr::write_unaligned(size, n as i32);
        }
    }
}

/// `WSARecvEx` (mswsock, wsock32 ordinal 1107) : `recv` qui signale par
/// MSG_PARTIAL (0x8000) dans `*flags` un datagramme plus long que le
/// tampon ; le reste du datagramme est perdu, comme sous Windows.
pub unsafe extern "win64" fn wsa_recv_ex(s: usize, b: *mut u8, n: i32, flags: *mut i32) -> i32 {
    const MSG_PARTIAL: i32 = 0x8000;
    let asked = if flags.is_null() {
        0
    } else {
        std::ptr::read_unaligned(flags) & !MSG_PARTIAL
    };
    if !valid_socket(s) || n <= 0 || b.is_null() {
        return recv(s, b, n, asked);
    }
    async_arm(s, FD_READ | FD_OOB);
    let mut iov = libc::iovec {
        iov_base: b as *mut c_void,
        iov_len: n as usize,
    };
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    let r = libc::recvmsg(s as i32, &mut msg, recv_flags(asked));
    if r < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    if !flags.is_null() {
        let partial = if msg.msg_flags & libc::MSG_TRUNC != 0 {
            MSG_PARTIAL
        } else {
            0
        };
        std::ptr::write_unaligned(flags, partial);
    }
    set_error(0);
    r as i32
}

// ── __WSAFDIsSet (FD_ISSET helper) ──────────────────────────────────────
pub unsafe extern "win64" fn wsa_fd_is_set(s: usize, set: *const WinFdSet) -> i32 {
    if set.is_null() {
        return 0;
    }
    let count = (std::ptr::read_unaligned(set as *const u32) as usize).min(FD_SETSIZE_WIN);
    // En x86, SOCKET arrive sur 32 bits : comparer sur cette largeur.
    let s = if crate::wow32::pointer_size() == 4 {
        s as u32 as usize
    } else {
        s
    };
    for i in 0..count {
        if fd_slot_read(set as usize, i) == s {
            return 1;
        }
    }
    0
}

/// Ordinaux publics communs à ws2_32 et wsock32 (Winsock 1.1) : les
/// programmes liés à wsock32 importent souvent par numéro.
const WINSOCK_ORDINALS: &[(u16, &str)] = &[
    (1, "accept"),
    (2, "bind"),
    (3, "closesocket"),
    (4, "connect"),
    (5, "getpeername"),
    (6, "getsockname"),
    (7, "getsockopt"),
    (8, "htonl"),
    (9, "htons"),
    (10, "ioctlsocket"),
    (11, "inet_addr"),
    (12, "inet_ntoa"),
    (13, "listen"),
    (14, "ntohl"),
    (15, "ntohs"),
    (16, "recv"),
    (17, "recvfrom"),
    (18, "select"),
    (19, "send"),
    (20, "sendto"),
    (21, "setsockopt"),
    (22, "shutdown"),
    (23, "socket"),
    (51, "gethostbyaddr"),
    (52, "gethostbyname"),
    (53, "getprotobyname"),
    (54, "getprotobynumber"),
    (55, "getservbyname"),
    (56, "getservbyport"),
    (57, "gethostname"),
    (101, "WSAAsyncSelect"),
    (102, "WSAAsyncGetHostByAddr"),
    (103, "WSAAsyncGetHostByName"),
    (108, "WSACancelAsyncRequest"),
    (111, "WSAGetLastError"),
    (112, "WSASetLastError"),
    (113, "WSACancelBlockingCall"),
    (114, "WSAIsBlocking"),
    (115, "WSAStartup"),
    (116, "WSACleanup"),
    (151, "__WSAFDIsSet"),
    // wsock32 : fonctions de mswsock exportées par ordinal (#1380, #1390).
    (1107, "WSARecvEx"),
    (1140, "TransmitFile"),
    (1141, "AcceptEx"),
    (1142, "GetAcceptExSockaddrs"),
];

pub fn exports() -> crate::dlls::Exports {
    let mut e = named_exports();
    for (ordinal, name) in WINSOCK_ORDINALS {
        if let Some(&addr) = e.get(name) {
            e.insert(Box::leak(format!("#{ordinal}").into_boxed_str()), addr);
        }
    }
    e
}

fn named_exports() -> crate::dlls::Exports {
    crate::exports! {
     // init/cleanup
     "WSAStartup"=>fn wsa_startup,"WSACleanup"=>fn wsa_cleanup,
     "WSAGetLastError"=>fn wsa_get_last_error,"WSASetLastError"=>fn wsa_set_last_error,
     // core socket ops
     "socket"=>fn socket,"closesocket"=>fn closesocket,"connect"=>fn connect,
     "bind"=>fn bind,"listen"=>fn listen,"accept"=>fn accept,
     "send"=>fn send,"recv"=>fn recv,"sendto"=>fn send_to,"recvfrom"=>fn recv_from,
     "shutdown"=>fn shutdown,
     // socket options
     "setsockopt"=>fn setsockopt,"getsockopt"=>fn getsockopt,
     "ioctlsocket"=>fn ioctlsocket,
     // name queries
     "getsockname"=>fn get_socket_name,"getpeername"=>fn get_peer_name,
     // byte order
     "htons"=>fn htons,"ntohs"=>fn ntohs,"htonl"=>fn htonl,"ntohl"=>fn ntohl,
     // address conversion
     "inet_addr"=>fn inet_addr,"inet_ntoa"=>fn inet_ntoa,
     "inet_pton"=>fn inet_pton,"InetPtonA"=>fn inet_pton,
     "InetPtonW"=>fn inet_pton_w,
     "inet_ntop"=>fn inet_ntop,"InetNtopA"=>fn inet_ntop,
     "InetNtopW"=>fn inet_ntop_w,
     // DNS / host resolution
     "gethostname"=>fn get_host_name,"GetHostNameW"=>fn get_host_name_w,
     "gethostbyname"=>fn gethostbyname,"gethostbyaddr"=>fn gethostbyaddr,
     "WSAAsyncGetHostByName"=>fn wsa_async_get_host_by_name,
     "WSAAsyncGetHostByAddr"=>fn wsa_async_get_host_by_addr,
     "WSACancelAsyncRequest"=>fn wsa_cancel_async_request,
     "getservbyname"=>fn getservbyname,"getservbyport"=>fn getservbyport,
     "getaddrinfo"=>fn get_addr_info_a,
     "GetAddrInfoA"=>fn get_addr_info_a,"GetAddrInfoW"=>fn get_addr_info_w,
     "freeaddrinfo"=>fn free_addr_info_a,
     "FreeAddrInfoA"=>fn free_addr_info_a,"FreeAddrInfoW"=>fn free_addr_info_w,
     "getnameinfo"=>fn getnameinfo,"GetNameInfoA"=>fn getnameinfo,"GetNameInfoW"=>fn get_name_info_w,
     // select / poll
     "select"=>fn select,"WSAPoll"=>fn wsa_poll,
     "__WSAFDIsSet"=>fn wsa_fd_is_set,
     // mswsock (#1390) : aussi rendues par WSAIoctl et par wsock32.
     "GetAcceptExSockaddrs"=>fn get_accept_ex_sockaddrs,
     "AcceptEx"=>fn overlapped::accept_ex,"TransmitFile"=>fn overlapped::transmit_file,
     "WSARecvEx"=>fn wsa_recv_ex,
     // WSA extended
     "WSASocketW"=>fn wsa_socket_w,"WSASocketA"=>fn wsa_socket_w,
     "WSASend"=>fn wsa_send,"WSARecv"=>fn wsa_recv,
     "WSAConnect"=>fn wsa_connect,
     "WSAIoctl"=>fn wsa_ioctl,
     "WSAGetOverlappedResult"=>fn overlapped::get_result,
     "WSADuplicateSocketW"=>fn wsa_duplicate_socket_w,
     "WSAStringToAddressW"=>fn wsa_string_to_address_w,
     "WSAAddressToStringW"=>fn wsa_address_to_string_w,
     "WSAAsyncSelect"=>fn wsa_async_select,
     // WSA events
     "WSACreateEvent"=>fn wsa_create_event,"WSACloseEvent"=>fn wsa_close_event,
     "WSASetEvent"=>fn wsa_set_event,"WSAResetEvent"=>fn wsa_reset_event,
     "WSAWaitForMultipleEvents"=>fn wsa_wait_for_multiple_events,
     "WSAEventSelect"=>fn wsa_event_select,
     "WSAEnumNetworkEvents"=>fn wsa_enum_network_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresses_rangees_par_accept_ex() {
        // 4 octets de données, puis deux blocs de 32 octets (sockaddr_in + 16).
        let mut buf = [0u8; 4 + 32 + 32];
        buf[4..6].copy_from_slice(&2u16.to_le_bytes());
        buf[6..8].copy_from_slice(&8080u16.to_be_bytes());
        buf[36..38].copy_from_slice(&2u16.to_le_bytes());
        buf[38..40].copy_from_slice(&443u16.to_be_bytes());
        let (mut l, mut r) = (0usize, 0usize);
        let (mut ln, mut rn) = (0i32, 0i32);
        unsafe {
            get_accept_ex_sockaddrs(
                buf.as_ptr(),
                4,
                32,
                32,
                &mut l as *mut usize as usize,
                &mut ln,
                &mut r as *mut usize as usize,
                &mut rn,
            );
        }
        assert_eq!((l, ln), (buf.as_ptr() as usize + 4, 16));
        assert_eq!((r, rn), (buf.as_ptr() as usize + 36, 16));
        let e = exports();
        assert_eq!(e.get("#1142"), e.get("GetAcceptExSockaddrs"));
        assert!(e.contains_key("#1142"));
    }

    #[test]
    fn hostent_aux_deux_dispositions() {
        let addrs = [[10, 0, 0, 1], [192, 168, 1, 2]];
        for ps in [4usize, 8] {
            let need = pack_hostent(&mut [], 0, b"hote.local", &addrs, ps);
            let mut buf = vec![0u8; need];
            let base = 0x1000;
            assert_eq!(
                pack_hostent(&mut buf, base, b"hote.local", &addrs, ps),
                need
            );
            let rd = |at: usize| {
                let mut v = [0u8; 8];
                v[..ps].copy_from_slice(&buf[at..at + ps]);
                usize::from_le_bytes(v)
            };
            let head = if ps == 4 { 16 } else { 32 };
            let name = rd(0) - base;
            assert_eq!(&buf[name..name + 11], b"hote.local\0");
            assert_eq!(rd(rd(ps) - base), 0, "h_aliases vide");
            assert_eq!(i16::from_le_bytes([buf[2 * ps], buf[2 * ps + 1]]), 2);
            assert_eq!(i16::from_le_bytes([buf[2 * ps + 2], buf[2 * ps + 3]]), 4);
            let list = rd(head - ps) - base;
            for (i, a) in addrs.iter().enumerate() {
                let p = rd(list + ps * i) - base;
                assert_eq!(&buf[p..p + 4], a);
            }
            assert_eq!(rd(list + ps * 2), 0);
            // Tampon trop petit : rien d'écrit, taille rendue.
            let mut small = vec![0xAAu8; need - 1];
            assert_eq!(
                pack_hostent(&mut small, base, b"hote.local", &addrs, ps),
                need
            );
            assert!(small.iter().all(|b| *b == 0xAA));
        }
    }

    #[test]
    fn annulation_de_demande_inconnue() {
        assert_eq!(wsa_cancel_async_request(usize::MAX), SOCKET_ERROR);
    }
    /// Les tests partagent le compteur global WSAStartup/WSACleanup : un
    /// test qui vide ce compteur pendant qu'un autre utilise ses sockets le
    /// faisait échouer au hasard. Ils passent donc l'un après l'autre.
    static WSA_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serialise() -> std::sync::MutexGuard<'static, ()> {
        WSA_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn init() -> std::sync::MutexGuard<'static, ()> {
        let guard = serialise();
        unsafe {
            let mut d = [0u8; 408];
            wsa_startup(0x0202, d.as_mut_ptr());
        }
        guard
    }
    fn cleanup() {
        wsa_cleanup();
    }

    #[test]
    fn accept_ex_par_wsaioctl_range_la_connexion_et_les_adresses() {
        // #1408 : Steam demande AcceptEx et ses voisines à WSAIoctl.
        let _g = init();
        unsafe {
            let l = socket(2, 1, 6);
            let mut a = [0u8; 16];
            a[0] = 2;
            a[4..8].copy_from_slice(&[127, 0, 0, 1]);
            assert_eq!(bind(l, a.as_ptr(), 16), 0);
            assert_eq!(listen(l, 4), 0);
            let mut alen = 16i32;
            assert_eq!(get_socket_name(l, a.as_mut_ptr(), &mut alen), 0);
            let mut f = 0usize;
            let mut got = 0u32;
            let guid: [u8; 16] = [
                0xF1, 0x7D, 0x36, 0xB5, 0xAC, 0xCB, 0xCF, 0x11, 0x95, 0xCA, 0x00, 0x80, 0x5F, 0x48,
                0xA1, 0x92,
            ];
            assert_eq!(
                overlapped::extension_function(
                    guid.as_ptr(),
                    16,
                    &mut f as *mut usize as *mut u8,
                    8,
                    &mut got
                ),
                0
            );
            assert_eq!((f, got), (overlapped::accept_ex as *const () as usize, 8));
            let c = socket(2, 1, 6);
            assert_eq!(connect(c, a.as_ptr(), 16), 0);
            let inc = socket(2, 1, 6);
            let mut buf = [0u8; 64];
            let mut n = 7u32;
            assert_eq!(
                overlapped::accept_ex(l, inc, buf.as_mut_ptr(), 0, 32, 32, &mut n, 0),
                1
            );
            assert_eq!(n, 0);
            let (mut lp, mut rp) = (0usize, 0usize);
            let (mut ll, mut rl) = (0i32, 0i32);
            get_accept_ex_sockaddrs(
                buf.as_ptr(),
                0,
                32,
                32,
                &mut lp as *mut usize as usize,
                &mut ll,
                &mut rp as *mut usize as usize,
                &mut rl,
            );
            assert_eq!((ll, rl), (16, 16));
            let local = std::slice::from_raw_parts(lp as *const u8, 16);
            assert_eq!(&local[2..4], &a[2..4]);
            assert_eq!(send(c, b"ok".as_ptr(), 2, 0), 2);
            let mut r = [0u8; 2];
            assert_eq!(recv(inc, r.as_mut_ptr(), 2, 0), 2);
            assert_eq!(&r, b"ok");
            for s in [c, inc, l] {
                closesocket(s);
            }
        }
    }

    #[test]
    fn mswsock_transmit_file_et_wsa_recv_ex() {
        // #1390 : jdk4py et pywin32 importent mswsock.dll en direct.
        let _g = init();
        assert_eq!(crate::dlls::normalize("MSWSOCK.dll"), "ws2_32");
        let e = exports();
        for (o, n) in [
            ("#1107", "WSARecvEx"),
            ("#1140", "TransmitFile"),
            ("#1141", "AcceptEx"),
        ] {
            assert!(e.contains_key(n), "{n}");
            assert_eq!(e.get(o), e.get(n));
        }
        let dir = std::env::temp_dir().join(format!("winlux-1390-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corps.txt");
        std::fs::write(&path, b"0123456789").unwrap();
        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        unsafe {
            let l = socket(2, 1, 6);
            let mut a = [0u8; 16];
            a[0] = 2;
            a[4..8].copy_from_slice(&[127, 0, 0, 1]);
            assert_eq!(bind(l, a.as_ptr(), 16), 0);
            assert_eq!(listen(l, 4), 0);
            let mut alen = 16i32;
            assert_eq!(get_socket_name(l, a.as_mut_ptr(), &mut alen), 0);
            let c = socket(2, 1, 6);
            assert_eq!(connect(c, a.as_ptr(), 16), 0);
            let inc = accept(l, std::ptr::null_mut(), std::ptr::null_mut());
            let h = crate::dlls::kernel32::create_file_a(
                cpath.as_ptr() as *const u8,
                0x8000_0000,
                1,
                0,
                3,
                0,
                0,
            );
            assert_ne!(h, usize::MAX);
            // Position courante à 2 : le fichier part de là, 5 octets.
            crate::dlls::kernel32::set_file_pointer(h, 2, std::ptr::null_mut(), 0);
            let (head, tail) = (*b"<", *b">");
            let tfb: [usize; 4] = [head.as_ptr() as usize, 1, tail.as_ptr() as usize, 1];
            assert_eq!(
                overlapped::transmit_file(inc, h, 5, 0, 0, tfb.as_ptr() as usize, 1),
                1
            );
            let mut r = [0u8; 16];
            let mut got = 0usize;
            loop {
                let mut fl = 0i32;
                let n = wsa_recv_ex(c, r.as_mut_ptr().add(got), (16 - got) as i32, &mut fl);
                assert!(n >= 0);
                assert_eq!(fl, 0);
                if n == 0 {
                    break;
                }
                got += n as usize;
            }
            // TF_DISCONNECT : la fin d'envoi arrive après « <23456> ».
            assert_eq!(&r[..got], b"<23456>");
            crate::dlls::kernel32::close_handle(h);
            for s in [c, inc, l] {
                closesocket(s);
            }
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn connect_non_bloquant_signale_wsaewouldblock_comme_windows() {
        // PuTTY abandonne sur WSAEINPROGRESS (10036) : Windows renvoie
        // WSAEWOULDBLOCK (10035) pour un connect non bloquant en cours.
        assert_eq!(error_from_errno(libc::EINPROGRESS), WSAEWOULDBLOCK);
        assert_eq!(error_from_errno(libc::EWOULDBLOCK), WSAEWOULDBLOCK);
    }

    #[test]
    fn handle_associe_par_wsaeventselect_est_pret_quand_la_socket_l_est() {
        let mut fds = [0i32; 2];
        // SAFETY : tableau de deux descripteurs.
        unsafe {
            assert_eq!(
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()),
                0
            );
        }
        let h = 0x7777_0001usize;
        lock(event_assoc()).insert(
            fds[0] as usize,
            EventAssoc {
                event: h,
                mask: FD_READ,
                write_armed: false,
            },
        );
        assert!(!handle_ready(h));
        // SAFETY : écrit un octet sur le pair.
        unsafe { libc::write(fds[1], b"x".as_ptr() as *const c_void, 1) };
        assert!(handle_ready(h));
        lock(event_assoc()).remove(&(fds[0] as usize));
        assert!(!handle_ready(h));
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    /// WaitForSingleObject / WaitForMultipleObjects de kernel32 sur un
    /// événement CreateEvent associé par WSAEventSelect : prêt quand la
    /// socket a des données (WinSCP x86 en SFTP, #1340).
    #[test]
    fn attente_kernel32_sur_l_evenement_d_une_socket() {
        let _g = serialise();
        use crate::dlls::kernel32;
        let mut fds = [0i32; 2];
        unsafe {
            assert_eq!(
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()),
                0
            );
            lock(sockets()).insert(fds[0] as usize);
            let ev = kernel32::create_event_w(0, 0, 0, std::ptr::null());
            assert_ne!(ev, 0);
            assert_eq!(wsa_event_select(fds[0] as usize, ev, FD_READ | FD_CLOSE), 0);
            assert_eq!(kernel32::wait_for_single_object(ev, 0), 258);
            assert_eq!(kernel32::wait_for_multiple_objects(1, &ev, 0, 20), 258);
            libc::write(fds[1], b"SSH-2.0-x\r\n".as_ptr() as *const c_void, 11);
            assert_eq!(kernel32::wait_for_single_object(ev, 1000), 0);
            assert_eq!(kernel32::wait_for_multiple_objects(1, &ev, 0, 1000), 0);
            assert_eq!(kernel32::wait_for_multiple_objects(1, &ev, 1, 1000), 0);
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(fds[0] as usize, ev, &mut ne), 0);
            assert_eq!(ne.network_events & FD_READ, FD_READ);
            let mut buf = [0u8; 32];
            libc::read(fds[0], buf.as_mut_ptr() as *mut c_void, 32);
            assert_eq!(kernel32::wait_for_single_object(ev, 0), 258);
            assert_eq!(wsa_event_select(fds[0] as usize, 0, 0), 0);
            lock(sockets()).remove(&(fds[0] as usize));
            kernel32::close_handle(ev);
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[test]
    fn sockets_non_heritables_par_defaut_et_fd_accept_vu_par_kernel32() {
        use crate::dlls::kernel32;
        let _g = init();
        unsafe {
            let (srv, addr) = listener();
            // Ni socket() ni accept() ne laissent un descripteur à l'enfant
            // (#1347).
            assert_ne!(libc::fcntl(srv as i32, libc::F_GETFD) & libc::FD_CLOEXEC, 0);
            let ev = kernel32::create_event_w(0, 0, 0, std::ptr::null());
            assert_eq!(wsa_event_select(srv, ev, FD_ACCEPT), 0);
            assert_eq!(kernel32::wait_for_single_object(ev, 0), 258);
            let cli = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_eq!(
                libc::connect(cli, &addr as *const _ as *const libc::sockaddr, 16),
                0
            );
            // DevTools de Chromium : FD_ACCEPT attendu par WaitForSingleObject.
            assert_eq!(kernel32::wait_for_single_object(ev, 1000), 0);
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(srv, ev, &mut ne), 0);
            assert_eq!(ne.network_events & FD_ACCEPT, FD_ACCEPT);
            assert_eq!(wsa_event_select(srv, 0, 0), 0);
            let peer = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(peer, usize::MAX);
            assert_ne!(
                libc::fcntl(peer as i32, libc::F_GETFD) & libc::FD_CLOEXEC,
                0
            );

            // Marquée héritable, la socket est transmise et reprise.
            let (text, fds) = crate::dlls::inherit::describe(srv, false).expect("socket");
            assert!(text.starts_with("k,"), "{text}");
            let mut fds = fds.into_iter();
            let h = crate::dlls::inherit::adopt_one(&text, 0, |_| fds.next()).expect("reprise");
            assert_ne!(h, srv, "numéro déjà pris ici : un autre descripteur");
            assert!(is_socket(h));
            let mut on = 0i32;
            let mut len = 4u32;
            libc::getsockopt(
                h as i32,
                libc::SOL_SOCKET,
                libc::SO_ACCEPTCONN,
                &mut on as *mut i32 as *mut c_void,
                &mut len,
            );
            assert_eq!(on, 1, "la copie est bien la socket d'écoute");

            closesocket(h);
            closesocket(peer);
            closesocket(srv);
            libc::close(cli);
            kernel32::close_handle(ev);
        }
        cleanup();
    }

    #[test]
    fn evenements_reseau_selon_le_role_de_la_socket() {
        use crate::dlls::kernel32;
        let _g = init();
        const TOUS: i32 = FD_READ | FD_WRITE | FD_OOB | FD_ACCEPT | FD_CONNECT | FD_CLOSE;
        unsafe {
            let (srv, addr) = listener();
            // Comme PuTTY : les six événements demandés sur chaque socket.
            let ev_srv = kernel32::create_event_w(0, 0, 0, std::ptr::null());
            assert_eq!(wsa_event_select(srv, ev_srv, TOUS), 0);
            let cli = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let ev_cli = kernel32::create_event_w(0, 0, 0, std::ptr::null());
            assert_eq!(wsa_event_select(cli, ev_cli, TOUS), 0);
            let rc = connect(cli, &addr as *const _ as *const u8, 16);
            assert!(rc == 0 || wsa_get_last_error() == 10035);
            assert_eq!(kernel32::wait_for_single_object(ev_srv, 1000), 0);
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(srv, ev_srv, &mut ne), 0);
            assert_eq!(ne.network_events, FD_ACCEPT, "écoute : FD_ACCEPT seul");
            let peer = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(peer, usize::MAX);
            libc::send(
                peer as i32,
                b"SSH-2.0-x\r\n".as_ptr() as *const c_void,
                11,
                0,
            );
            assert_eq!(kernel32::wait_for_single_object(ev_cli, 1000), 0);
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(cli, ev_cli, &mut ne), 0);
            assert_eq!(
                ne.network_events & FD_ACCEPT,
                0,
                "connectée : jamais FD_ACCEPT (#1350)"
            );
            assert_eq!(ne.network_events & FD_READ, FD_READ);
            assert_eq!(ne.error_code, [0; 10]);
            // Fermeture par le pair : FD_CLOSE.
            closesocket(peer);
            let mut buf = [0u8; 32];
            libc::recv(cli as i32, buf.as_mut_ptr() as *mut c_void, 32, 0);
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(cli, ev_cli, &mut ne), 0);
            assert_eq!(ne.network_events & FD_CLOSE, FD_CLOSE);
            assert_eq!(ne.network_events & FD_ACCEPT, 0);
            closesocket(cli);

            // Connexion refusée : FD_CONNECT avec WSAECONNREFUSED à
            // l'indice FD_CONNECT_BIT (4).
            let mut libre = addr;
            closesocket(srv);
            libre.sin_port = addr.sin_port;
            let c2 = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_eq!(wsa_event_select(c2, ev_cli, TOUS), 0);
            let rc = connect(c2, &libre as *const _ as *const u8, 16);
            // Le refus peut aussi être immédiat sur la boucle locale.
            if rc != 0 && wsa_get_last_error() == 10035 {
                assert_eq!(kernel32::wait_for_single_object(ev_cli, 1000), 0);
                let mut ne = WsaNetworkEvents::default();
                assert_eq!(wsa_enum_network_events(c2, ev_cli, &mut ne), 0);
                assert_eq!(ne.network_events & FD_CONNECT, FD_CONNECT);
                assert_eq!(ne.error_code[4], 10061);
                assert_eq!(ne.network_events & FD_ACCEPT, 0);
            }
            closesocket(c2);
            kernel32::close_handle(ev_srv);
            kernel32::close_handle(ev_cli);
        }
        cleanup();
    }

    #[test]
    fn evenement_wsa_attendu_par_kernel32() {
        use crate::dlls::kernel32;
        let _g = init();
        unsafe {
            let (srv, addr) = listener();
            // Serveur DevTools de Chromium : WSACreateEvent, FD_ACCEPT, puis
            // WaitForSingleObject par ObjectWatcher (#1351).
            let ev = wsa_create_event();
            assert_eq!(wsa_event_select(srv, ev, FD_ACCEPT), 0);
            assert_eq!(kernel32::wait_for_single_object(ev, 0), 258);
            let other = kernel32::create_event_w(0, 0, 0, std::ptr::null());
            let both = [other, ev];
            assert_eq!(
                kernel32::wait_for_multiple_objects(2, both.as_ptr(), 0, 10),
                258
            );
            let cli = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            libc::connect(cli, &addr as *const _ as *const libc::sockaddr, 16);
            assert_eq!(kernel32::wait_for_single_object(ev, 1000), 0);
            assert_eq!(
                kernel32::wait_for_multiple_objects(2, both.as_ptr(), 0, 1000),
                1
            );
            let peer = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(peer, usize::MAX);
            closesocket(peer);
            libc::close(cli);
            closesocket(srv);
            wsa_close_event(ev);
            kernel32::close_handle(other);
        }
        cleanup();
    }

    #[test]
    fn evenements_wsa_et_kernel32_interchangeables() {
        use crate::dlls::kernel32;
        let _g = init();
        unsafe {
            // Serveur DevTools de Chromium (#1360) : écoute, accept, lecture
            // par WSAEventSelect et écriture WSASend avec un OVERLAPPED dont
            // hEvent vient de WSACreateEvent.
            let (srv, addr) = listener();
            let accept_ev = wsa_create_event();
            assert_eq!(wsa_event_select(srv, accept_ev, FD_ACCEPT), 0);
            let cli = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            libc::connect(cli, &addr as *const _ as *const libc::sockaddr, 16);
            assert_eq!(kernel32::wait_for_single_object(accept_ev, 1000), 0);
            let peer = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(peer, usize::MAX);
            // La socket acceptée garde l'association de la socket d'écoute.
            assert!(lock(event_assoc())
                .get(&peer)
                .is_some_and(|a| a.event == accept_ev));
            let read_ev = wsa_create_event();
            assert_eq!(wsa_event_select(peer, read_ev, FD_READ | FD_CLOSE), 0);
            libc::send(cli, b"GET".as_ptr() as *const c_void, 3, 0);
            assert_eq!(kernel32::wait_for_single_object(read_ev, 1000), 0);
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(peer, read_ev, &mut ne), 0);
            assert_eq!(ne.network_events & FD_READ, FD_READ);

            let write_ev = wsa_create_event();
            let mut ov = new_overlapped();
            ov.event = write_ev;
            let mut data = *b"HTTP/1.1 101";
            let buf = WsaBuf {
                len: data.len() as u32,
                buf: data.as_mut_ptr(),
            };
            let mut sent = 0u32;
            let rc = wsa_send(peer, &buf, 1, &mut sent, 0, &mut *ov as *mut _ as usize, 0);
            if rc != 0 {
                assert_eq!(wsa_get_last_error(), 997);
                assert_eq!(kernel32::wait_for_single_object(write_ev, 1000), 0);
            }
            // Ce que fait Chromium : l'événement est signalé, ResetEvent
            // le remet à zéro.
            assert_eq!(kernel32::wait_for_single_object(write_ev, 0), 0);
            assert_eq!(kernel32::reset_event(write_ev), 1);
            assert_eq!(kernel32::wait_for_single_object(write_ev, 0), 258);
            assert_eq!(kernel32::set_event(write_ev), 1);
            assert_eq!(kernel32::wait_for_single_object(write_ev, 0), 0);
            assert_eq!(kernel32::close_handle(write_ev), 1);
            assert!(!is_wsa_event(write_ev));

            // Et dans l'autre sens : WSASetEvent/WSAResetEvent sur CreateEvent.
            let k = kernel32::create_event_w(0, 1, 0, std::ptr::null());
            assert_eq!(wsa_set_event(k), 1);
            assert_eq!(kernel32::wait_for_single_object(k, 0), 0);
            assert_eq!(wsa_reset_event(k), 1);
            assert_eq!(kernel32::wait_for_single_object(k, 0), 258);
            kernel32::close_handle(k);

            libc::close(cli);
            closesocket(peer);
            closesocket(srv);
            wsa_close_event(accept_ev);
            wsa_close_event(read_ev);
        }
        cleanup();
    }

    #[test]
    fn ideal_send_backlog_rend_une_taille() {
        let _g = serialise();
        let mut fds = [0i32; 2];
        unsafe {
            libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr());
            lock(sockets()).insert(fds[0] as usize);
            let mut v = 0u32;
            let mut n = 0u32;
            let rc = wsa_ioctl(
                fds[0] as usize,
                SIO_IDEAL_SEND_BACKLOG_QUERY,
                std::ptr::null(),
                0,
                &mut v as *mut u32 as *mut u8,
                4,
                &mut n,
                0,
                0,
            );
            assert_eq!((rc, n), (0, 4));
            assert!(v > 0);
            lock(sockets()).remove(&(fds[0] as usize));
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[test]
    fn wsaasyncselect_annonce_connexion_donnees_et_fermeture_une_fois() {
        unsafe {
            let srv = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let mut a: libc::sockaddr_in = std::mem::zeroed();
            a.sin_family = libc::AF_INET as libc::sa_family_t;
            a.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
            let sz = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            assert_eq!(
                libc::bind(srv, &a as *const _ as *const libc::sockaddr, sz),
                0
            );
            assert_eq!(libc::listen(srv, 1), 0);
            let mut len = sz;
            libc::getsockname(srv, &mut a as *mut _ as *mut libc::sockaddr, &mut len);
            let cli = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
            let mask = FD_READ | FD_WRITE | FD_CONNECT | FD_CLOSE;
            let mut sel = AsyncSel {
                hwnd: 1,
                msg: 0x8005,
                mask,
                armed: mask & !FD_CONNECT,
                connecting: false,
            };
            // Pas encore connectée : rien, malgré le « prêt » de Linux.
            assert!(async_ready(cli as usize, &mut sel).is_empty());
            libc::connect(cli, &a as *const _ as *const libc::sockaddr, sz);
            sel.connecting = true;
            sel.armed |= FD_CONNECT | FD_WRITE;
            let wait = |sel: &mut AsyncSel| {
                for _ in 0..200 {
                    let r = async_ready(cli as usize, sel);
                    if !r.is_empty() {
                        return r;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Vec::new()
            };
            assert_eq!(wait(&mut sel), vec![(FD_CONNECT, 0), (FD_WRITE, 0)]);
            assert!(
                async_ready(cli as usize, &mut sel).is_empty(),
                "une seule fois"
            );
            let peer = libc::accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            libc::write(peer, b"SSH-2.0-x\r\n".as_ptr() as *const c_void, 11);
            assert_eq!(wait(&mut sel), vec![(FD_READ, 0)]);
            assert!(
                async_ready(cli as usize, &mut sel).is_empty(),
                "réarmé par recv"
            );
            sel.armed |= FD_READ; // recv()
            assert_eq!(wait(&mut sel), vec![(FD_READ, 0)]);
            let mut buf = [0u8; 32];
            libc::read(cli, buf.as_mut_ptr() as *mut c_void, 32);
            libc::close(peer);
            sel.armed |= FD_READ;
            assert_eq!(wait(&mut sel), vec![(FD_CLOSE, 0)]);
            assert!(async_ready(cli as usize, &mut sel).is_empty());
            libc::close(cli);
            libc::close(srv);
        }
    }

    #[test]
    fn wsaasyncselect_signale_un_refus_de_connexion() {
        unsafe {
            // Port fermé : on prend un port libre puis on le referme.
            let tmp = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let mut a: libc::sockaddr_in = std::mem::zeroed();
            a.sin_family = libc::AF_INET as libc::sa_family_t;
            a.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
            let sz = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            libc::bind(tmp, &a as *const _ as *const libc::sockaddr, sz);
            let mut len = sz;
            libc::getsockname(tmp, &mut a as *mut _ as *mut libc::sockaddr, &mut len);
            libc::close(tmp);
            let cli = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
            libc::connect(cli, &a as *const _ as *const libc::sockaddr, sz);
            let mask = FD_READ | FD_WRITE | FD_CONNECT | FD_CLOSE;
            let mut sel = AsyncSel {
                hwnd: 1,
                msg: 0x8005,
                mask,
                armed: mask,
                connecting: true,
            };
            let mut got = Vec::new();
            for _ in 0..200 {
                got = async_ready(cli as usize, &mut sel);
                if !got.is_empty() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(got, vec![(FD_CONNECT, 10061)]);
            libc::close(cli);
        }
    }

    #[test]
    fn winsock_startup_accepte_les_versions_1_0_a_2_2() {
        let _wsa = serialise();
        unsafe {
            let mut data = [0u8; 408];
            // OpenTTD : MAKEWORD(2, 0)
            for (asked, got) in [
                (0x0002u16, 0x0002u16),
                (0x0102, 0x0102),
                (0x0101, 0x0101),
                (0x0303, 0x0202),
            ] {
                assert_eq!(wsa_startup(asked, data.as_mut_ptr()), 0, "{asked:#06x}");
                let version = std::ptr::read_unaligned(data.as_ptr() as *const u16);
                assert_eq!(version, got);
                wsa_cleanup();
            }
            assert_eq!(wsa_startup(0x0000, data.as_mut_ptr()), 10092);
        }
    }

    #[test]
    fn winsock_startup_ouvre_ferme_et_valide_un_socket() {
        let _wsa = serialise();
        unsafe {
            // drain any leftover startups from other tests
            while STARTUPS.load(Ordering::Acquire) > 0 {
                wsa_cleanup();
            }
            let mut data = [0u8; 408];
            assert_eq!(wsa_startup(0x0202, data.as_mut_ptr()), 0);
            let version = std::ptr::read_unaligned(data.as_ptr() as *const u16);
            assert_eq!(version, 0x0202);
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_ne!(s, usize::MAX);
            assert_eq!(closesocket(s), 0);
            assert_eq!(wsa_cleanup(), 0);
            assert_eq!(socket(libc::AF_INET, libc::SOCK_STREAM, 0), usize::MAX);
            assert_eq!(wsa_get_last_error(), WSA_NOT_INITIALISED);
        }
    }

    #[test]
    fn options_ip_et_ipv6_traduites() {
        assert_eq!(
            translate_sockopt(WIN_IPPROTO_IPV6, 27),
            Some((libc::IPPROTO_IPV6, libc::IPV6_V6ONLY))
        );
        assert_eq!(
            translate_sockopt(WIN_IPPROTO_IP, 12),
            Some((libc::IPPROTO_IP, libc::IP_ADD_MEMBERSHIP))
        );
        assert_eq!(translate_sockopt(WIN_IPPROTO_IP, 99), None);
        assert_eq!((af_in(23), af_out(libc::AF_INET6)), (libc::AF_INET6, 23));
        unsafe {
            let mut sa = [0u8; 28];
            sa[0] = 23;
            let st = addr_in(sa.as_ptr(), 28);
            assert_eq!(st.ss_family as i32, libc::AF_INET6);
            sa[0] = libc::AF_INET6 as u8;
            addr_out(sa.as_mut_ptr(), 28);
            assert_eq!(sa[0], 23);
        }
        let _wsa = init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_ne!(s, INVALID_SOCKET);
            let ttl: i32 = 32;
            assert_eq!(
                setsockopt(s, WIN_IPPROTO_IP, 4, &ttl as *const _ as *const u8, 4),
                0
            );
            let mut out: i32 = 0;
            let mut len: i32 = 4;
            assert_eq!(
                getsockopt(
                    s,
                    WIN_IPPROTO_IP,
                    4,
                    &mut out as *mut _ as *mut u8,
                    &mut len
                ),
                0
            );
            assert_eq!(out, 32);
            let on: i32 = 1;
            assert_eq!(
                setsockopt(
                    s,
                    WIN_SOL_SOCKET,
                    WIN_SO_EXCLUSIVEADDRUSE,
                    &on as *const _ as *const u8,
                    4
                ),
                0
            );
            closesocket(s);
        }
    }

    #[test]
    fn setsockopt_reuseaddr() {
        let _wsa = init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_ne!(s, INVALID_SOCKET);
            let val: i32 = 1;
            assert_eq!(
                setsockopt(
                    s,
                    WIN_SOL_SOCKET,
                    WIN_SO_REUSEADDR,
                    &val as *const _ as *const u8,
                    4
                ),
                0
            );
            let mut out: i32 = 0;
            let mut len: i32 = 4;
            assert_eq!(
                getsockopt(
                    s,
                    WIN_SOL_SOCKET,
                    WIN_SO_REUSEADDR,
                    &mut out as *mut _ as *mut u8,
                    &mut len
                ),
                0
            );
            assert_ne!(out, 0);
            closesocket(s);
        }
        cleanup();
    }

    #[test]
    fn ioctlsocket_nonblock() {
        let _wsa = init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let mut val: u32 = 1;
            assert_eq!(ioctlsocket(s, FIONBIO, &mut val), 0);
            let flags = libc::fcntl(s as i32, libc::F_GETFL);
            assert!(flags & libc::O_NONBLOCK != 0);
            val = 0;
            assert_eq!(ioctlsocket(s, FIONBIO, &mut val), 0);
            let flags = libc::fcntl(s as i32, libc::F_GETFL);
            assert!(flags & libc::O_NONBLOCK == 0);
            closesocket(s);
        }
        cleanup();
    }

    #[test]
    fn inet_conversions() {
        unsafe {
            let ip = inet_addr(b"127.0.0.1\0".as_ptr());
            assert_ne!(ip, u32::MAX);
            let text = inet_ntoa(ip);
            assert!(!text.is_null());
            let s = crate::dlls::cstr(text);
            assert_eq!(s, "127.0.0.1");

            let mut dst = [0u8; 4];
            assert_eq!(
                inet_pton(libc::AF_INET, b"10.0.0.1\0".as_ptr(), dst.as_mut_ptr()),
                1
            );
            assert_eq!(dst, [10, 0, 0, 1]);

            let mut buf = [0u8; 32];
            let r = inet_ntop(libc::AF_INET, dst.as_ptr(), buf.as_mut_ptr(), 32);
            assert!(!r.is_null());
            let s = crate::dlls::cstr(buf.as_ptr());
            assert_eq!(s, "10.0.0.1");
        }
    }

    #[test]
    fn af_inet6_windows_est_traduit() {
        let _wsa = init();
        unsafe {
            // Sans IPv6 sur la machine de test, rien à vérifier.
            let probe = libc::socket(libc::AF_INET6, libc::SOCK_STREAM, 0);
            if probe < 0 {
                cleanup();
                return;
            }
            libc::close(probe);
            // 23 = AF_INET6 côté Windows ; sans traduction, Linux refuse la famille.
            let s = socket(WIN_AF_INET6, libc::SOCK_STREAM, 0);
            assert_ne!(
                s,
                usize::MAX,
                "socket IPv6 refusée : {}",
                wsa_get_last_error()
            );
            let mut sin6 = [0u8; 28];
            sin6[0] = WIN_AF_INET6 as u8; // famille Windows
            sin6[23] = 1; // ::1
            assert_eq!(
                bind(s, sin6.as_ptr(), 28),
                0,
                "bind ::1 : {}",
                wsa_get_last_error()
            );
            let mut out = [0u8; 28];
            let mut len = 28i32;
            assert_eq!(get_socket_name(s, out.as_mut_ptr(), &mut len), 0);
            // La famille rendue au programme est bien celle de Windows.
            assert_eq!(u16::from_ne_bytes([out[0], out[1]]), WIN_AF_INET6 as u16);
            closesocket(s);
            let mut dst = [0u8; 16];
            assert_eq!(
                inet_pton(WIN_AF_INET6, b"::1\0".as_ptr(), dst.as_mut_ptr()),
                1
            );
            assert_eq!(dst[15], 1);
        }
        cleanup();
    }

    #[test]
    fn drapeaux_wsapoll_traduits() {
        assert_eq!(
            poll_events_to_linux(WIN_POLLRDNORM) & libc::POLLIN,
            libc::POLLIN
        );
        assert_eq!(
            poll_events_to_linux(WIN_POLLWRNORM) & libc::POLLOUT,
            libc::POLLOUT
        );
        assert_eq!(poll_events_to_win(libc::POLLIN), WIN_POLLRDNORM);
        assert_eq!(
            poll_events_to_win(libc::POLLHUP | libc::POLLERR),
            WIN_POLLHUP | WIN_POLLERR
        );
    }

    #[test]
    fn wsaeventselect_reveille_sur_connexion_entrante() {
        let _wsa = init();
        unsafe {
            let srv = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let mut addr: libc::sockaddr_in = std::mem::zeroed();
            addr.sin_family = libc::AF_INET as u16;
            addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
            assert_eq!(
                bind(srv, &addr as *const _ as *const u8, 16),
                0,
                "bind : {}",
                wsa_get_last_error()
            );
            assert_eq!(listen(srv, 4), 0);
            let mut len = 16i32;
            assert_eq!(
                get_socket_name(srv, &mut addr as *mut _ as *mut u8, &mut len),
                0
            );
            let ev = wsa_create_event();
            assert_ne!(ev, WSA_INVALID_EVENT);
            assert_eq!(wsa_event_select(srv, ev, FD_ACCEPT | FD_CLOSE), 0);
            // Rien en attente : l'attente expire.
            assert_eq!(
                wsa_wait_for_multiple_events(1, &ev, 0, 10, 0),
                WSA_WAIT_TIMEOUT
            );
            let cli = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_eq!(connect(cli, &addr as *const _ as *const u8, 16), 0);
            // La socket associée signale l'événement sans WSASetEvent.
            assert_eq!(
                wsa_wait_for_multiple_events(1, &ev, 0, 2000, 0),
                WSA_WAIT_EVENT_0
            );
            let mut ne = WsaNetworkEvents::default();
            assert_eq!(wsa_enum_network_events(srv, ev, &mut ne), 0);
            assert_ne!(ne.network_events & FD_ACCEPT, 0);
            closesocket(cli);
            closesocket(srv);
            wsa_close_event(ev);
        }
        cleanup();
    }

    #[test]
    fn select_non_blocking() {
        let _wsa = init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            // unconnected socket should be writable
            let mut wset = [0u8; 8 + 8]; // count (+ padding) + 1 SOCKET
            std::ptr::write_unaligned(wset.as_mut_ptr() as *mut u32, 1);
            std::ptr::write_unaligned(wset.as_mut_ptr().add(8) as *mut usize, s);
            let tv = WinTimeval { sec: 0, usec: 0 };
            let rc = select(
                0,
                std::ptr::null_mut(),
                wset.as_mut_ptr() as *mut WinFdSet,
                std::ptr::null_mut(),
                &tv,
            );
            assert!(rc >= 0);
            closesocket(s);
        }
        cleanup();
    }

    #[test]
    fn wsa_events_lifecycle() {
        let evt = wsa_create_event();
        assert_ne!(evt, WSA_INVALID_EVENT);
        assert_eq!(wsa_set_event(evt), 1);
        assert_eq!(wsa_reset_event(evt), 1);
        assert_eq!(wsa_close_event(evt), 1);
        assert_eq!(wsa_close_event(evt), 0); // already closed
    }

    #[test]
    fn wsa_send_recv_tcp() {
        let _wsa = init();
        unsafe {
            let srv = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let val: i32 = 1;
            setsockopt(
                srv,
                WIN_SOL_SOCKET,
                WIN_SO_REUSEADDR,
                &val as *const _ as *const u8,
                4,
            );
            let mut addr: libc::sockaddr_in = std::mem::zeroed();
            addr.sin_family = libc::AF_INET as u16;
            addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
            bind(srv, &addr as *const _ as *const u8, 16);
            let mut len: i32 = 16;
            get_socket_name(srv, &mut addr as *mut _ as *mut u8, &mut len);
            listen(srv, 1);

            let cli = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            connect(cli, &addr as *const _ as *const u8, 16);
            let acc = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());

            let msg = b"hello";
            let buf = WsaBuf {
                len: 5,
                buf: msg.as_ptr() as *mut u8,
            };
            let mut sent = 0u32;
            assert_eq!(wsa_send(cli, &buf, 1, &mut sent, 0, 0, 0), 0);
            assert_eq!(sent, 5);

            let mut rbuf = [0u8; 16];
            let mut rbufv = WsaBuf {
                len: 16,
                buf: rbuf.as_mut_ptr(),
            };
            let mut recvd = 0u32;
            let mut fl = 0u32;
            assert_eq!(wsa_recv(acc, &mut rbufv, 1, &mut recvd, &mut fl, 0, 0), 0);
            assert_eq!(&rbuf[..5], b"hello");

            closesocket(acc);
            closesocket(cli);
            closesocket(srv);
        }
        cleanup();
    }

    // ── E/S avec OVERLAPPED et port de complétion (#1146) ───────────────

    type ConnectEx =
        unsafe extern "win64" fn(usize, *const u8, i32, *const u8, u32, *mut u32, usize) -> i32;

    fn new_overlapped() -> Box<crate::dlls::kernel32::Overlapped> {
        Box::new(crate::dlls::kernel32::Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: 0,
        })
    }

    /// Socket d'écoute sur 127.0.0.1, port libre.
    unsafe fn listener() -> (usize, libc::sockaddr_in) {
        let srv = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
        let mut addr: libc::sockaddr_in = std::mem::zeroed();
        addr.sin_family = libc::AF_INET as u16;
        addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
        assert_eq!(bind(srv, &addr as *const _ as *const u8, 16), 0);
        let mut len: i32 = 16;
        get_socket_name(srv, &mut addr as *mut _ as *mut u8, &mut len);
        assert_eq!(listen(srv, 4), 0);
        (srv, addr)
    }

    /// Socket comme celle de .NET : WSASocketW avec WSA_FLAG_OVERLAPPED,
    /// liée à 127.0.0.1 et au port de complétion `port` (clé 7).
    unsafe fn overlapped_client(port: usize) -> usize {
        let cli = wsa_socket_w(libc::AF_INET, libc::SOCK_STREAM, 6, 0, 0, 1);
        assert_ne!(cli, INVALID_SOCKET);
        let mut any: libc::sockaddr_in = std::mem::zeroed();
        any.sin_family = libc::AF_INET as u16;
        any.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
        assert_eq!(bind(cli, &any as *const _ as *const u8, 16), 0);
        assert_eq!(
            crate::dlls::kernel32::create_io_completion_port(cli, port, 7, 0),
            port,
            "une socket doit pouvoir se lier à un port de complétion"
        );
        cli
    }

    unsafe fn connect_ex_of(s: usize) -> ConnectEx {
        let guid: [u8; 16] = [
            0xB9, 0x07, 0xA2, 0x25, 0xF3, 0xDD, 0x60, 0x46, 0x8E, 0xE9, 0x76, 0xE5, 0x8C, 0x74,
            0x06, 0x3E,
        ];
        let mut ptr = 0usize;
        let mut returned = 0u32;
        assert_eq!(
            wsa_ioctl(
                s,
                0xC800_0006,
                guid.as_ptr(),
                16,
                &mut ptr as *mut usize as *mut u8,
                8,
                &mut returned,
                0,
                0
            ),
            0
        );
        assert_eq!(returned, 8);
        std::mem::transmute::<usize, ConnectEx>(ptr)
    }

    /// Paquet suivant du port (au plus deux secondes) :
    /// (succès, octets, clé, OVERLAPPED).
    unsafe fn next_packet(port: usize) -> (i32, u32, usize, usize) {
        let mut bytes = 0u32;
        let mut key = 0usize;
        let mut ov: *mut c_void = std::ptr::null_mut();
        let ok = crate::dlls::kernel32::get_queued_completion_status(
            port, &mut bytes, &mut key, &mut ov, 2000,
        );
        (ok, bytes, key, ov as usize)
    }

    #[test]
    fn mode_sans_paquet_si_fin_immediate_pour_libuv() {
        // #1155 : libuv (Node d'Electron) pose FILE_SKIP_COMPLETION_PORT_ON_SUCCESS
        // et FILE_SKIP_SET_EVENT_ON_HANDLE sur chaque socket TCP, et échouait
        // en « connect EISDIR » (ERROR_INVALID_FUNCTION) quand on refusait.
        let _wsa = init();
        unsafe {
            let (srv, addr) = listener();
            let port = crate::dlls::kernel32::create_io_completion_port(usize::MAX, 0, 0, 0);
            let cli = overlapped_client(port);
            assert_eq!(
                crate::dlls::kernel32_node::SetFileCompletionNotificationModes(cli, 3),
                1
            );
            assert_eq!(
                connect(cli, &addr as *const _ as *const u8, 16),
                0,
                "connexion bloquante"
            );
            let acc = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(acc, INVALID_SOCKET);

            // Envoi fini tout de suite : OVERLAPPED rempli, aucun paquet.
            let msg = b"salut";
            let sb = WsaBuf {
                len: 5,
                buf: msg.as_ptr() as *mut u8,
            };
            let sov = new_overlapped();
            let sovp = &*sov as *const _ as usize;
            let mut sent = 0u32;
            assert_eq!(wsa_send(cli, &sb, 1, &mut sent, 0, sovp, 0), 0);
            assert_eq!(sent, 5);
            assert_eq!((sov.internal, sov.internal_high), (0, 5));
            assert_eq!(
                next_packet(port).0,
                0,
                "aucun paquet après une fin immédiate"
            );

            // Une réception qui attend poste toujours son paquet.
            let mut data = [0u8; 8];
            let mut wb = WsaBuf {
                len: 8,
                buf: data.as_mut_ptr(),
            };
            let rov = new_overlapped();
            let rovp = &*rov as *const _ as usize;
            let mut flags = 0u32;
            assert_eq!(
                wsa_recv(cli, &mut wb, 1, std::ptr::null_mut(), &mut flags, rovp, 0),
                SOCKET_ERROR
            );
            assert_eq!(wsa_get_last_error(), 997);
            assert_eq!(
                libc::send(acc as i32, b"ok".as_ptr() as *const c_void, 2, 0),
                2
            );
            assert_eq!(next_packet(port), (1, 2, 7, rovp));

            // Un handle qui n'est pas une socket : refus, ERROR_INVALID_FUNCTION.
            assert_eq!(
                crate::dlls::kernel32_node::SetFileCompletionNotificationModes(0x7777_0000, 3),
                0
            );
            closesocket(acc);
            closesocket(cli);
            closesocket(srv);
        }
    }

    #[test]
    fn connectex_wsasend_wsarecv_terminent_par_le_port_de_completion() {
        let _wsa = init();
        unsafe {
            let (srv, addr) = listener();
            let port = crate::dlls::kernel32::create_io_completion_port(usize::MAX, 0, 0, 0);
            let cli = overlapped_client(port);
            let connect_ex = connect_ex_of(cli);

            let ov = new_overlapped();
            let ovp = &*ov as *const _ as usize;
            let rc = connect_ex(
                cli,
                &addr as *const _ as *const u8,
                16,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                ovp,
            );
            assert!(
                rc == 1 || wsa_get_last_error() == 997,
                "ConnectEx : {rc}, {}",
                wsa_get_last_error()
            );
            // Terminée tout de suite ou non, la connexion poste un paquet.
            assert_eq!(next_packet(port), (1, 0, 7, ovp));
            let (mut n, mut fl) = (99u32, 0u32);
            assert_eq!(
                overlapped::get_result(cli, ovp as *const u8, &mut n, 0, &mut fl),
                1
            );
            assert_eq!(n, 0);
            assert_eq!(
                setsockopt(cli, WIN_SOL_SOCKET, 0x7010, std::ptr::null(), 0),
                0
            );
            let acc = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            assert_ne!(acc, INVALID_SOCKET);

            // Réception sans données : en attente, puis paquet à l'arrivée.
            let mut data = [0u8; 16];
            let mut wb = WsaBuf {
                len: 16,
                buf: data.as_mut_ptr(),
            };
            let rov = new_overlapped();
            let rovp = &*rov as *const _ as usize;
            let mut flags = 0u32;
            assert_eq!(
                wsa_recv(cli, &mut wb, 1, std::ptr::null_mut(), &mut flags, rovp, 0),
                SOCKET_ERROR
            );
            assert_eq!(wsa_get_last_error(), 997, "WSA_IO_PENDING attendu");
            assert_eq!(next_packet(port).0, 0, "rien avant l'envoi : délai dépassé");
            assert_eq!(
                libc::send(acc as i32, b"bonjour".as_ptr() as *const c_void, 7, 0),
                7
            );
            assert_eq!(next_packet(port), (1, 7, 7, rovp));
            assert_eq!(&data[..7], b"bonjour");

            // Envoi immédiat : rendu 0, et un paquet quand même.
            let msg = b"salut";
            let sb = WsaBuf {
                len: 5,
                buf: msg.as_ptr() as *mut u8,
            };
            let sov = new_overlapped();
            let sovp = &*sov as *const _ as usize;
            let mut sent = 0u32;
            assert_eq!(wsa_send(cli, &sb, 1, &mut sent, 0, sovp, 0), 0);
            assert_eq!(sent, 5);
            assert_eq!(next_packet(port), (1, 5, 7, sovp));
            let mut back = [0u8; 8];
            assert_eq!(
                libc::recv(acc as i32, back.as_mut_ptr() as *mut c_void, 8, 0),
                5
            );

            // Réception de zéro octet (.NET) : attend que des données soient là.
            let mut nothing = WsaBuf {
                len: 0,
                buf: data.as_mut_ptr(),
            };
            assert_eq!(
                wsa_recv(
                    cli,
                    &mut nothing,
                    1,
                    std::ptr::null_mut(),
                    &mut flags,
                    rovp,
                    0
                ),
                SOCKET_ERROR
            );
            assert_eq!(wsa_get_last_error(), 997);
            assert_eq!(
                libc::send(acc as i32, b"x".as_ptr() as *const c_void, 1, 0),
                1
            );
            assert_eq!(next_packet(port), (1, 0, 7, rovp));

            closesocket(acc);
            closesocket(cli);
            closesocket(srv);
            crate::dlls::kernel32::close_handle(port);
        }
        cleanup();
    }

    #[test]
    fn connexion_refusee_rend_wsaeconnrefused_et_jamais_moins_un() {
        let _wsa = init();
        unsafe {
            // Port libéré : personne n'écoute plus.
            let (srv, addr) = listener();
            closesocket(srv);
            let port = crate::dlls::kernel32::create_io_completion_port(usize::MAX, 0, 0, 0);
            let cli = overlapped_client(port);
            let connect_ex = connect_ex_of(cli);
            let ov = new_overlapped();
            let ovp = &*ov as *const _ as usize;
            let rc = connect_ex(
                cli,
                &addr as *const _ as *const u8,
                16,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                ovp,
            );
            if rc == 0 && wsa_get_last_error() == 997 {
                let (ok, _, key, at) = next_packet(port);
                assert_eq!((ok, key, at), (0, 7, ovp), "paquet d'échec attendu");
                let (mut n, mut fl) = (0u32, 0u32);
                assert_eq!(
                    overlapped::get_result(cli, ovp as *const u8, &mut n, 0, &mut fl),
                    0
                );
            } else {
                assert_eq!(rc, 0, "la connexion ne peut pas réussir");
            }
            assert_eq!(wsa_get_last_error(), 10061, "WSAECONNREFUSED attendu");
            closesocket(cli);
            crate::dlls::kernel32::close_handle(port);
        }
        cleanup();
    }

    #[test]
    fn fermeture_et_cancelioex_annulent_les_receptions_en_attente() {
        let _wsa = init();
        unsafe {
            let (srv, addr) = listener();
            let port = crate::dlls::kernel32::create_io_completion_port(usize::MAX, 0, 0, 0);
            let cli = overlapped_client(port);
            assert_eq!(connect(cli, &addr as *const _ as *const u8, 16), 0);
            let acc = accept(srv, std::ptr::null_mut(), std::ptr::null_mut());
            let mut data = [0u8; 8];
            let mut wb = WsaBuf {
                len: 8,
                buf: data.as_mut_ptr(),
            };
            let mut flags = 0u32;

            // CancelIoEx sur cette opération.
            let ov = new_overlapped();
            let ovp = &*ov as *const _ as usize;
            assert_eq!(
                wsa_recv(cli, &mut wb, 1, std::ptr::null_mut(), &mut flags, ovp, 0),
                SOCKET_ERROR
            );
            assert_eq!(wsa_get_last_error(), 997);
            assert_eq!(crate::dlls::kernel32::cancel_io_ex(cli, ovp as *mut _), 1);
            let (ok, _, _, at) = next_packet(port);
            assert_eq!((ok, at), (0, ovp));
            let (mut n, mut fl) = (0u32, 0u32);
            assert_eq!(
                overlapped::get_result(cli, ovp as *const u8, &mut n, 0, &mut fl),
                0
            );
            assert_eq!(wsa_get_last_error(), 995, "WSA_OPERATION_ABORTED attendu");

            // closesocket : le paquet d'annulation part avant le retour.
            assert_eq!(
                wsa_recv(cli, &mut wb, 1, std::ptr::null_mut(), &mut flags, ovp, 0),
                SOCKET_ERROR
            );
            assert_eq!(closesocket(cli), 0);
            let mut bytes = 0u32;
            let mut key = 0usize;
            let mut got: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                crate::dlls::kernel32::get_queued_completion_status(
                    port, &mut bytes, &mut key, &mut got, 0
                ),
                0
            );
            assert_eq!(got as usize, ovp, "paquet d'annulation déjà posté");
            assert!(!is_socket(cli));

            closesocket(acc);
            closesocket(srv);
            crate::dlls::kernel32::close_handle(port);
        }
        cleanup();
    }

    #[test]
    fn wsagetlasterror_et_getlasterror_partagent_la_meme_valeur() {
        // Sans TEB (tests), la copie par fil suffit ; avec un TEB, les deux
        // lisent la même case.
        set_error(10061);
        assert_eq!(wsa_get_last_error(), 10061);
        if crate::process::teb() != 0 {
            assert_eq!(crate::process::last_error(), 10061);
        }
    }
}
