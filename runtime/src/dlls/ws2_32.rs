//! Passerelle Winsock vers les sockets POSIX/Linux, avec codes d'erreur WSA.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

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
struct WsaNetworkEvents {
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

static EVENT_ASSOC: OnceLock<Mutex<HashMap<usize, (usize, i32)>>> = OnceLock::new();
fn event_assoc() -> &'static Mutex<HashMap<usize, (usize, i32)>> {
    EVENT_ASSOC.get_or_init(|| Mutex::new(HashMap::new()))
}

#[repr(C)]
struct WsaBuf {
    len: u32,
    buf: *mut u8,
}

#[repr(C)]
struct WinLinger {
    onoff: u16,
    linger: u16,
}

#[repr(C)]
struct WinFdSet {
    count: u32,
    // followed by count SOCKET values (usize each)
}

#[repr(C)]
struct WinTimeval {
    sec: i32,
    usec: i32,
}

// hostent for gethostbyname/gethostbyaddr
#[repr(C)]
struct WinHostent {
    h_name: *mut c_char,
    h_aliases: *mut *mut c_char,
    h_addrtype: i16,
    h_length: i16,
    h_addr_list: *mut *mut c_char,
}

static HOSTENT_BUF: OnceLock<Mutex<HostentBuf>> = OnceLock::new();
struct HostentBuf {
    hostent: WinHostent,
    name: Vec<u8>,
    aliases_ptr: Vec<*mut c_char>,
    addrs: Vec<[u8; 4]>,
    addr_ptrs: Vec<*mut c_char>,
}
unsafe impl Send for HostentBuf {}

static INET_NTOA_BUF: OnceLock<Mutex<[u8; 16]>> = OnceLock::new();

#[repr(C)]
struct WinServent {
    s_name: *mut c_char,
    s_aliases: *mut *mut c_char,
    s_port: i16,
    s_proto: *mut c_char,
}
static SERVENT_BUF: OnceLock<Mutex<ServentBuf>> = OnceLock::new();
struct ServentBuf {
    servent: WinServent,
    name: Vec<u8>,
    proto: Vec<u8>,
    aliases_ptr: Vec<*mut c_char>,
}
unsafe impl Send for ServentBuf {}
fn sockets() -> &'static Mutex<HashSet<usize>> {
    SOCKETS.get_or_init(|| Mutex::new(HashSet::new()))
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn set_error(e: i32) {
    LAST_WSA_ERROR.with(|x| x.set(e));
}
fn error_from_errno(e: i32) -> i32 {
    match e {
        libc::EACCES | libc::EPERM => 10013,
        libc::EADDRINUSE => 10048,
        libc::EADDRNOTAVAIL => 10049,
        libc::EAFNOSUPPORT => 10047,
        libc::EINPROGRESS => 10036,
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
        _ => 10022,
    }
}
fn last_error() -> i32 {
    unsafe { error_from_errno(*libc::__errno_location()) }
}
fn valid_socket(s: usize) -> bool {
    lock(sockets()).contains(&s)
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
    let h = std::ptr::read_unaligned(hints);
    let mut out: libc::addrinfo = std::mem::zeroed();
    out.ai_flags = h.flags;
    out.ai_family = h.family;
    out.ai_socktype = h.socktype;
    out.ai_protocol = h.protocol;
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

pub unsafe extern "win64" fn wsa_startup(requested: u16, data: *mut u8) -> i32 {
    if data.is_null() {
        return 10014;
    }
    let major = (requested & 0xff) as u8;
    let minor = (requested >> 8) as u8;
    let version = if major > 2 || (major == 2 && minor >= 2) {
        0x0202
    } else if major == 1 {
        0x0101
    } else {
        return 10092;
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
    std::ptr::write_unaligned(data as *mut WsaData, out);
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
    LAST_WSA_ERROR.with(|x| x.get())
}
pub extern "win64" fn wsa_set_last_error(e: i32) {
    set_error(e)
}
pub extern "win64" fn socket(af: i32, kind: i32, protocol: i32) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 {
        set_error(WSA_NOT_INITIALISED);
        return usize::MAX;
    }
    let s = unsafe { libc::socket(af, kind, protocol) };
    if s < 0 {
        set_error(last_error());
        usize::MAX
    } else {
        lock(sockets()).insert(s as usize);
        s as usize
    }
}
pub extern "win64" fn closesocket(s: usize) -> i32 {
    if !valid_socket(s) {
        set_error(10038);
        return SOCKET_ERROR;
    }
    let r = unsafe { libc::close(s as i32) };
    if r < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
        lock(sockets()).remove(&s);
        0
    }
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
    let rc = libc::connect(
        s as i32,
        name as *const libc::sockaddr,
        len as libc::socklen_t,
    );
    if rc < 0 {
        set_error(last_error());
        SOCKET_ERROR
    } else {
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
    let rc = libc::bind(
        s as i32,
        name as *const libc::sockaddr,
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
    let r = libc::accept(s as i32, ptr, &mut n);
    if r < 0 {
        set_error(last_error());
        return usize::MAX;
    }
    if !len.is_null() {
        *len = n as i32;
    }
    lock(sockets()).insert(r as usize);
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
    if r < 0 {
        set_error(last_error());
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
    if n < 0 || (n > 0 && b.is_null()) {
        set_error(if n < 0 { WSA_INVALID_PARAMETER } else { 10014 });
        return SOCKET_ERROR;
    }
    let r = libc::recv(s as i32, b as *mut c_void, n as usize, flags);
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
    let r = libc::sendto(
        s as i32,
        b as *const c_void,
        n as usize,
        flags | libc::MSG_NOSIGNAL,
        name as *const libc::sockaddr,
        len as libc::socklen_t,
    );
    if r < 0 {
        set_error(last_error());
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
            let p = Box::into_raw(storage);
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
            family: ai.ai_family,
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
            let p = Box::into_raw(storage);
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
            family: ai.ai_family,
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
        let p = hints as *const WinAddrInfoA;
        let hin = std::ptr::read_unaligned(p);
        h.ai_flags = hin.flags;
        h.ai_family = hin.family;
        h.ai_socktype = hin.socktype;
        h.ai_protocol = hin.protocol;
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
        (level, opt) if level != WIN_SOL_SOCKET => Some((level, opt)),
        _ => None,
    }
}

pub unsafe extern "win64" fn setsockopt(
    s: usize, level: i32, optname: i32, optval: *const u8, optlen: i32,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if optlen < 0 { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let Some((l, o)) = translate_sockopt(level, optname) else {
        set_error(WSAENOPROTOOPT); return SOCKET_ERROR;
    };
    // Windows SO_LINGER uses u16 pairs, Linux uses c_int pairs
    if level == WIN_SOL_SOCKET && optname == WIN_SO_LINGER {
        if optlen < 4 || optval.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let win = std::ptr::read_unaligned(optval as *const WinLinger);
        let lin = libc::linger { l_onoff: win.onoff as i32, l_linger: win.linger as i32 };
        let rc = libc::setsockopt(s as i32, l, o,
            &lin as *const _ as *const c_void, std::mem::size_of::<libc::linger>() as u32);
        if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }
        return 0;
    }
    // Windows timeout is a DWORD (ms), Linux is timeval
    if level == WIN_SOL_SOCKET && (optname == WIN_SO_SNDTIMEO || optname == WIN_SO_RCVTIMEO) {
        if optlen < 4 || optval.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let ms = std::ptr::read_unaligned(optval as *const u32);
        let tv = libc::timeval { tv_sec: (ms / 1000) as i64, tv_usec: ((ms % 1000) * 1000) as i64 };
        let rc = libc::setsockopt(s as i32, l, o,
            &tv as *const _ as *const c_void, std::mem::size_of::<libc::timeval>() as u32);
        if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }
        return 0;
    }
    let rc = libc::setsockopt(s as i32, l, o,
        if optval.is_null() { std::ptr::null() } else { optval as *const c_void },
        optlen as u32);
    if rc < 0 { set_error(last_error()); SOCKET_ERROR } else { 0 }
}

pub unsafe extern "win64" fn getsockopt(
    s: usize, level: i32, optname: i32, optval: *mut u8, optlen: *mut i32,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if optval.is_null() || optlen.is_null() || *optlen < 0 {
        set_error(WSAEFAULT); return SOCKET_ERROR;
    }
    let Some((l, o)) = translate_sockopt(level, optname) else {
        set_error(WSAENOPROTOOPT); return SOCKET_ERROR;
    };
    if level == WIN_SOL_SOCKET && optname == WIN_SO_LINGER {
        let mut lin: libc::linger = std::mem::zeroed();
        let mut sz = std::mem::size_of::<libc::linger>() as u32;
        let rc = libc::getsockopt(s as i32, l, o, &mut lin as *mut _ as *mut c_void, &mut sz);
        if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }
        if *optlen >= 4 {
            let win = WinLinger { onoff: lin.l_onoff as u16, linger: lin.l_linger as u16 };
            std::ptr::write_unaligned(optval as *mut WinLinger, win);
            *optlen = 4;
        }
        return 0;
    }
    if level == WIN_SOL_SOCKET && (optname == WIN_SO_SNDTIMEO || optname == WIN_SO_RCVTIMEO) {
        let mut tv: libc::timeval = std::mem::zeroed();
        let mut sz = std::mem::size_of::<libc::timeval>() as u32;
        let rc = libc::getsockopt(s as i32, l, o, &mut tv as *mut _ as *mut c_void, &mut sz);
        if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }
        if *optlen >= 4 {
            let ms = (tv.tv_sec as u32) * 1000 + (tv.tv_usec as u32) / 1000;
            std::ptr::write_unaligned(optval as *mut u32, ms);
            *optlen = 4;
        }
        return 0;
    }
    let mut sz = (*optlen) as u32;
    let rc = libc::getsockopt(s as i32, l, o, optval as *mut c_void, &mut sz);
    if rc < 0 { set_error(last_error()); SOCKET_ERROR } else { *optlen = sz as i32; 0 }
}

// ── ioctlsocket ─────────────────────────────────────────────────────────
pub unsafe extern "win64" fn ioctlsocket(s: usize, cmd: u32, argp: *mut u32) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if argp.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
    match cmd {
        FIONBIO => {
            let val = *argp;
            let flags = libc::fcntl(s as i32, libc::F_GETFL);
            if flags < 0 { set_error(last_error()); return SOCKET_ERROR; }
            let new = if val != 0 { flags | libc::O_NONBLOCK } else { flags & !libc::O_NONBLOCK };
            if libc::fcntl(s as i32, libc::F_SETFL, new) < 0 {
                set_error(last_error()); SOCKET_ERROR
            } else { 0 }
        }
        FIONREAD => {
            let mut count: i32 = 0;
            if libc::ioctl(s as i32, libc::FIONREAD, &mut count) < 0 {
                set_error(last_error()); SOCKET_ERROR
            } else { *argp = count as u32; 0 }
        }
        SIOCATMARK => {
            let mut at: i32 = 0;
            if libc::ioctl(s as i32, 0x8905 /* SIOCATMARK */, &mut at) < 0 {
                set_error(last_error()); SOCKET_ERROR
            } else { *argp = at as u32; 0 }
        }
        _ => { set_error(WSAEINVAL); SOCKET_ERROR }
    }
}

// ── select ──────────────────────────────────────────────────────────────
unsafe fn read_fd_set(win: *const WinFdSet) -> Vec<usize> {
    if win.is_null() { return Vec::new(); }
    let count = std::ptr::read_unaligned(win as *const u32) as usize;
    let base = (win as *const u8).add(4) as *const usize;
    (0..count).map(|i| std::ptr::read_unaligned(base.add(i))).collect()
}

unsafe fn write_fd_set(win: *mut WinFdSet, socks: &[usize]) {
    if win.is_null() { return; }
    std::ptr::write_unaligned(win as *mut u32, socks.len() as u32);
    let base = (win as *mut u8).add(4) as *mut usize;
    for (i, &s) in socks.iter().enumerate() {
        std::ptr::write_unaligned(base.add(i), s);
    }
}

pub unsafe extern "win64" fn select(
    _nfds: i32, readfds: *mut WinFdSet, writefds: *mut WinFdSet,
    exceptfds: *mut WinFdSet, timeout: *const WinTimeval,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 { set_error(WSA_NOT_INITIALISED); return SOCKET_ERROR; }
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

    for &s in &rfds { libc::FD_SET(s as i32, &mut rd); nfds = nfds.max(s as i32 + 1); }
    for &s in &wfds { libc::FD_SET(s as i32, &mut wr); nfds = nfds.max(s as i32 + 1); }
    for &s in &efds { libc::FD_SET(s as i32, &mut ex); nfds = nfds.max(s as i32 + 1); }

    let mut tv = if timeout.is_null() {
        None
    } else {
        let t = std::ptr::read_unaligned(timeout);
        Some(libc::timeval { tv_sec: t.sec as i64, tv_usec: t.usec as i64 })
    };

    let rc = libc::select(
        nfds,
        if readfds.is_null() { std::ptr::null_mut() } else { &mut rd },
        if writefds.is_null() { std::ptr::null_mut() } else { &mut wr },
        if exceptfds.is_null() { std::ptr::null_mut() } else { &mut ex },
        tv.as_mut().map_or(std::ptr::null_mut(), |p| p),
    );
    if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }

    if !readfds.is_null() {
        let v: Vec<usize> = rfds.iter().copied().filter(|&s| libc::FD_ISSET(s as i32, &rd)).collect();
        write_fd_set(readfds, &v);
    }
    if !writefds.is_null() {
        let v: Vec<usize> = wfds.iter().copied().filter(|&s| libc::FD_ISSET(s as i32, &wr)).collect();
        write_fd_set(writefds, &v);
    }
    if !exceptfds.is_null() {
        let v: Vec<usize> = efds.iter().copied().filter(|&s| libc::FD_ISSET(s as i32, &ex)).collect();
        write_fd_set(exceptfds, &v);
    }
    rc
}

// ── WSAPoll (= poll) ────────────────────────────────────────────────────
#[repr(C)]
struct WsaPollFd {
    fd: usize,
    events: i16,
    revents: i16,
}

pub unsafe extern "win64" fn wsa_poll(fds: *mut WsaPollFd, nfds: u32, timeout: i32) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 { set_error(WSA_NOT_INITIALISED); return SOCKET_ERROR; }
    if fds.is_null() && nfds > 0 { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let n = nfds as usize;
    let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(n);
    for i in 0..n {
        let w = std::ptr::read_unaligned(fds.add(i));
        pfds.push(libc::pollfd { fd: w.fd as i32, events: w.events, revents: 0 });
    }
    let rc = libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, timeout);
    if rc < 0 { set_error(last_error()); return SOCKET_ERROR; }
    for i in 0..n {
        (*fds.add(i)).revents = pfds[i].revents;
    }
    rc
}

// ── inet_ntoa / inet_ntop / inet_pton ───────────────────────────────────
pub unsafe extern "win64" fn inet_ntoa(addr: u32) -> *const u8 {
    let octets = addr.to_ne_bytes();
    let s = format!("{}.{}.{}.{}\0", octets[0], octets[1], octets[2], octets[3]);
    let buf = INET_NTOA_BUF.get_or_init(|| Mutex::new([0u8; 16]));
    let mut g = lock(buf);
    let b = s.as_bytes();
    let n = b.len().min(16);
    g[..n].copy_from_slice(&b[..n]);
    g.as_ptr()
}

pub unsafe extern "win64" fn inet_pton(af: i32, src: *const u8, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let text = crate::dlls::cstr(src);
    let c = match CString::new(text) { Ok(c) => c, Err(_) => return 0 };
    let rc = posix_inet_pton(af, c.as_ptr(), dst as *mut c_void);
    if rc < 0 { set_error(last_error()); SOCKET_ERROR } else { rc }
}

pub unsafe extern "win64" fn inet_ntop(
    af: i32, src: *const u8, dst: *mut u8, size: usize,
) -> *const u8 {
    if src.is_null() || dst.is_null() { set_error(WSAEFAULT); return std::ptr::null(); }
    let r = posix_inet_ntop(af, src as *const c_void, dst as *mut c_char, size as u32);
    if r.is_null() { set_error(last_error()); std::ptr::null() } else { dst }
}

pub unsafe extern "win64" fn inet_ntop_w(
    af: i32, src: *const u8, dst: *mut u16, size: usize,
) -> *const u16 {
    if src.is_null() || dst.is_null() { set_error(WSAEFAULT); return std::ptr::null(); }
    let mut buf = [0u8; 64];
    let r = posix_inet_ntop(af, src as *const c_void, buf.as_mut_ptr() as *mut c_char, 64);
    if r.is_null() { set_error(last_error()); return std::ptr::null(); }
    let len = buf.iter().position(|b| *b == 0).unwrap_or(63);
    if len + 1 > size { set_error(WSAEFAULT); return std::ptr::null(); }
    for i in 0..len { *dst.add(i) = buf[i] as u16; }
    *dst.add(len) = 0;
    dst
}

pub unsafe extern "win64" fn inet_pton_w(af: i32, src: *const u16, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let n = crate::printf::wide_len(src);
    let s = match String::from_utf16(std::slice::from_raw_parts(src, n)) {
        Ok(s) => s, Err(_) => return 0,
    };
    let c = match CString::new(s) { Ok(c) => c, Err(_) => return 0 };
    let rc = posix_inet_pton(af, c.as_ptr(), dst as *mut c_void);
    if rc < 0 { set_error(last_error()); SOCKET_ERROR } else { rc }
}

// ── gethostbyname / gethostbyaddr ───────────────────────────────────────
pub unsafe extern "win64" fn gethostbyname(name: *const u8) -> *const WinHostent {
    if STARTUPS.load(Ordering::Acquire) == 0 { set_error(WSA_NOT_INITIALISED); return std::ptr::null(); }
    if name.is_null() { set_error(WSAEFAULT); return std::ptr::null(); }
    let text = crate::dlls::cstr(name);
    let c = match CString::new(text) { Ok(c) => c, Err(_) => { set_error(WSAHOST_NOT_FOUND); return std::ptr::null(); } };

    let mut hints: libc::addrinfo = std::mem::zeroed();
    hints.ai_family = libc::AF_INET;
    hints.ai_socktype = libc::SOCK_STREAM;
    let mut raw = std::ptr::null_mut();
    let rc = libc::getaddrinfo(c.as_ptr(), std::ptr::null(), &hints, &mut raw);
    if rc != 0 { set_error(gai_error(rc)); return std::ptr::null(); }

    let mut addrs: Vec<[u8; 4]> = Vec::new();
    let mut hostname = Vec::new();
    let mut cur = raw;
    while !cur.is_null() {
        let ai = &*cur;
        if ai.ai_family == libc::AF_INET && ai.ai_addrlen >= 16 {
            let sin = &*(ai.ai_addr as *const libc::sockaddr_in);
            addrs.push(sin.sin_addr.s_addr.to_ne_bytes());
        }
        if hostname.is_empty() && !ai.ai_canonname.is_null() {
            let cn = std::ffi::CStr::from_ptr(ai.ai_canonname);
            hostname = cn.to_bytes().to_vec();
            hostname.push(0);
        }
        cur = ai.ai_next;
    }
    libc::freeaddrinfo(raw);
    if addrs.is_empty() { set_error(WSAHOST_NOT_FOUND); return std::ptr::null(); }
    if hostname.is_empty() {
        hostname = c.as_bytes_with_nul().to_vec();
    }

    let buf = HOSTENT_BUF.get_or_init(|| Mutex::new(HostentBuf {
        hostent: std::mem::zeroed(),
        name: Vec::new(),
        aliases_ptr: Vec::new(),
        addrs: Vec::new(),
        addr_ptrs: Vec::new(),
    }));
    let mut g = lock(buf);
    g.name = hostname;
    g.addrs = addrs;
    g.aliases_ptr = vec![std::ptr::null_mut()];
    g.addr_ptrs = g.addrs.iter_mut().map(|a| a.as_mut_ptr() as *mut c_char).collect();
    g.addr_ptrs.push(std::ptr::null_mut());
    g.hostent.h_name = g.name.as_mut_ptr() as *mut c_char;
    g.hostent.h_aliases = g.aliases_ptr.as_mut_ptr();
    g.hostent.h_addrtype = libc::AF_INET as i16;
    g.hostent.h_length = 4;
    g.hostent.h_addr_list = g.addr_ptrs.as_mut_ptr();
    &g.hostent as *const WinHostent
}

pub unsafe extern "win64" fn gethostbyaddr(
    addr: *const u8, len: i32, addrtype: i32,
) -> *const WinHostent {
    if STARTUPS.load(Ordering::Acquire) == 0 { set_error(WSA_NOT_INITIALISED); return std::ptr::null(); }
    if addr.is_null() || len < 4 || addrtype != libc::AF_INET {
        set_error(WSAEFAULT); return std::ptr::null();
    }
    let ip_bytes: [u8; 4] = [*addr, *addr.add(1), *addr.add(2), *addr.add(3)];
    let ip = std::net::Ipv4Addr::from(ip_bytes);
    let text = format!("{}", ip);
    let c = match CString::new(text) { Ok(c) => c, Err(_) => { set_error(WSAHOST_NOT_FOUND); return std::ptr::null(); } };

    let mut hints: libc::addrinfo = std::mem::zeroed();
    hints.ai_family = libc::AF_INET;
    hints.ai_flags = libc::AI_NUMERICHOST;
    let mut raw = std::ptr::null_mut();
    let rc = libc::getaddrinfo(c.as_ptr(), std::ptr::null(), &hints, &mut raw);
    if rc != 0 { set_error(gai_error(rc)); return std::ptr::null(); }

    let mut hostname = c.as_bytes_with_nul().to_vec();

    // Try reverse DNS
    if !raw.is_null() && !(*raw).ai_addr.is_null() {
        let mut host = [0i8; 256];
        let ni = libc::getnameinfo(
            (*raw).ai_addr, (*raw).ai_addrlen as u32,
            host.as_mut_ptr(), 256, std::ptr::null_mut(), 0, 0);
        if ni == 0 {
            let hn = std::ffi::CStr::from_ptr(host.as_ptr());
            hostname = hn.to_bytes().to_vec();
            hostname.push(0);
        }
    }
    libc::freeaddrinfo(raw);

    let buf = HOSTENT_BUF.get_or_init(|| Mutex::new(HostentBuf {
        hostent: std::mem::zeroed(),
        name: Vec::new(),
        aliases_ptr: Vec::new(),
        addrs: Vec::new(),
        addr_ptrs: Vec::new(),
    }));
    let mut g = lock(buf);
    g.name = hostname;
    g.addrs = vec![ip_bytes];
    g.aliases_ptr = vec![std::ptr::null_mut()];
    g.addr_ptrs = g.addrs.iter_mut().map(|a| a.as_mut_ptr() as *mut c_char).collect();
    g.addr_ptrs.push(std::ptr::null_mut());
    g.hostent.h_name = g.name.as_mut_ptr() as *mut c_char;
    g.hostent.h_aliases = g.aliases_ptr.as_mut_ptr();
    g.hostent.h_addrtype = libc::AF_INET as i16;
    g.hostent.h_length = 4;
    g.hostent.h_addr_list = g.addr_ptrs.as_mut_ptr();
    &g.hostent as *const WinHostent
}

// ── getservbyname / getservbyport ───────────────────────────────────────
pub unsafe extern "win64" fn getservbyname(
    name: *const u8, proto: *const u8,
) -> *const WinServent {
    if name.is_null() { set_error(WSAEFAULT); return std::ptr::null(); }
    let nm = crate::dlls::cstr(name);
    let pr = if proto.is_null() { String::new() } else { crate::dlls::cstr(proto) };
    let nm_c = match CString::new(nm) { Ok(c) => c, Err(_) => return std::ptr::null() };
    let pr_c = if pr.is_empty() { None } else { CString::new(pr).ok() };
    let r = libc::getservbyname(
        nm_c.as_ptr(),
        pr_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()));
    if r.is_null() { set_error(WSAHOST_NOT_FOUND); return std::ptr::null(); }
    fill_servent(&*r)
}

pub unsafe extern "win64" fn getservbyport(
    port: i32, proto: *const u8,
) -> *const WinServent {
    let pr = if proto.is_null() { String::new() } else { crate::dlls::cstr(proto) };
    let pr_c = if pr.is_empty() { None } else { CString::new(pr).ok() };
    let r = libc::getservbyport(
        port, pr_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()));
    if r.is_null() { set_error(WSAHOST_NOT_FOUND); return std::ptr::null(); }
    fill_servent(&*r)
}

unsafe fn fill_servent(se: &libc::servent) -> *const WinServent {
    let buf = SERVENT_BUF.get_or_init(|| Mutex::new(ServentBuf {
        servent: std::mem::zeroed(),
        name: Vec::new(), proto: Vec::new(), aliases_ptr: Vec::new(),
    }));
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
    &g.servent as *const WinServent
}

// ── getnameinfo / GetNameInfoW ──────────────────────────────────────────
pub unsafe extern "win64" fn getnameinfo(
    sa: *const u8, sa_len: i32,
    host: *mut u8, host_len: u32,
    serv: *mut u8, serv_len: u32,
    flags: i32,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 { return WSA_NOT_INITIALISED; }
    if sa.is_null() { return WSAEFAULT; }
    // translate Windows NI_ flags to Linux NI_ flags
    let mut lflags = 0i32;
    if flags & NI_NUMERICHOST != 0 { lflags |= libc::NI_NUMERICHOST; }
    if flags & NI_NUMERICSERV != 0 { lflags |= libc::NI_NUMERICSERV; }
    if flags & NI_NOFQDN != 0 { lflags |= libc::NI_NOFQDN; }
    if flags & NI_NAMEREQD != 0 { lflags |= libc::NI_NAMEREQD; }
    if flags & NI_DGRAM != 0 { lflags |= libc::NI_DGRAM; }
    let rc = libc::getnameinfo(
        sa as *const libc::sockaddr, sa_len as u32,
        if host.is_null() { std::ptr::null_mut() } else { host as *mut c_char },
        host_len,
        if serv.is_null() { std::ptr::null_mut() } else { serv as *mut c_char },
        serv_len, lflags);
    if rc != 0 { gai_error(rc) } else { 0 }
}

pub unsafe extern "win64" fn get_name_info_w(
    sa: *const u8, sa_len: i32,
    host: *mut u16, host_len: u32,
    serv: *mut u16, serv_len: u32,
    flags: i32,
) -> i32 {
    if STARTUPS.load(Ordering::Acquire) == 0 { return WSA_NOT_INITIALISED; }
    if sa.is_null() { return WSAEFAULT; }
    let mut hbuf = vec![0u8; host_len.max(1) as usize];
    let mut sbuf = vec![0u8; serv_len.max(1) as usize];
    let rc = getnameinfo(
        sa, sa_len,
        if host.is_null() { std::ptr::null_mut() } else { hbuf.as_mut_ptr() },
        host_len,
        if serv.is_null() { std::ptr::null_mut() } else { sbuf.as_mut_ptr() },
        serv_len, flags);
    if rc != 0 { return rc; }
    if !host.is_null() {
        let len = hbuf.iter().position(|b| *b == 0).unwrap_or(hbuf.len());
        for i in 0..len.min(host_len as usize - 1) { *host.add(i) = hbuf[i] as u16; }
        *host.add(len.min(host_len as usize - 1)) = 0;
    }
    if !serv.is_null() {
        let len = sbuf.iter().position(|b| *b == 0).unwrap_or(sbuf.len());
        for i in 0..len.min(serv_len as usize - 1) { *serv.add(i) = sbuf[i] as u16; }
        *serv.add(len.min(serv_len as usize - 1)) = 0;
    }
    0
}

// ── WSASocketW ──────────────────────────────────────────────────────────
pub extern "win64" fn wsa_socket_w(
    af: i32, kind: i32, protocol: i32,
    _info: usize, _group: u32, flags: u32,
) -> usize {
    if STARTUPS.load(Ordering::Acquire) == 0 { set_error(WSA_NOT_INITIALISED); return INVALID_SOCKET; }
    let mut sock_flags = 0i32;
    if flags & 0x01 != 0 { sock_flags |= libc::SOCK_NONBLOCK; } // WSA_FLAG_OVERLAPPED
    let s = unsafe { libc::socket(af, kind | sock_flags, protocol) };
    if s < 0 { set_error(last_error()); INVALID_SOCKET } else { lock(sockets()).insert(s as usize); s as usize }
}

// ── WSASend / WSARecv ───────────────────────────────────────────────────
pub unsafe extern "win64" fn wsa_send(
    s: usize, bufs: *const WsaBuf, buf_count: u32,
    bytes_sent: *mut u32, flags: u32,
    _overlapped: usize, _completion: usize,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if bufs.is_null() || buf_count == 0 { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let n = buf_count as usize;
    let mut iovs: Vec<libc::iovec> = Vec::with_capacity(n);
    for i in 0..n {
        let b = std::ptr::read_unaligned(bufs.add(i));
        iovs.push(libc::iovec { iov_base: b.buf as *mut c_void, iov_len: b.len as usize });
    }
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = iovs.as_mut_ptr();
    msg.msg_iovlen = n;
    let r = libc::sendmsg(s as i32, &msg, flags as i32 | libc::MSG_NOSIGNAL);
    if r < 0 { set_error(last_error()); SOCKET_ERROR }
    else { if !bytes_sent.is_null() { *bytes_sent = r as u32; } 0 }
}

pub unsafe extern "win64" fn wsa_recv(
    s: usize, bufs: *mut WsaBuf, buf_count: u32,
    bytes_recv: *mut u32, flags: *mut u32,
    _overlapped: usize, _completion: usize,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if bufs.is_null() || buf_count == 0 { set_error(WSAEFAULT); return SOCKET_ERROR; }
    let n = buf_count as usize;
    let mut iovs: Vec<libc::iovec> = Vec::with_capacity(n);
    for i in 0..n {
        let b = std::ptr::read_unaligned(bufs.add(i));
        iovs.push(libc::iovec { iov_base: b.buf as *mut c_void, iov_len: b.len as usize });
    }
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = iovs.as_mut_ptr();
    msg.msg_iovlen = n;
    let fl = if flags.is_null() { 0 } else { *flags as i32 };
    let r = libc::recvmsg(s as i32, &mut msg, fl);
    if r < 0 { set_error(last_error()); SOCKET_ERROR }
    else {
        if !bytes_recv.is_null() { *bytes_recv = r as u32; }
        if !flags.is_null() { *flags = msg.msg_flags as u32; }
        0
    }
}

// ── WSAConnect ──────────────────────────────────────────────────────────
pub unsafe extern "win64" fn wsa_connect(
    s: usize, name: *const u8, namelen: i32,
    _caller_data: usize, _callee_data: usize,
    _sqos: usize, _gqos: usize,
) -> i32 {
    connect(s, name, namelen)
}

// ── WSA Event API ───────────────────────────────────────────────────────
pub extern "win64" fn wsa_create_event() -> usize {
    let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK) };
    if fd < 0 { set_error(last_error()); return WSA_INVALID_EVENT; }
    let handle = fd as usize;
    lock(events()).insert(handle, EventState { fd, triggered: false });
    handle
}

pub extern "win64" fn wsa_close_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.remove(&h) {
        unsafe { libc::close(st.fd); }
        lock(event_assoc()).retain(|_, (evt, _)| *evt != h);
        1 // TRUE
    } else {
        set_error(WSAEINVAL); 0
    }
}

pub extern "win64" fn wsa_set_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.get_mut(&h) {
        st.triggered = true;
        let val: u64 = 1;
        unsafe { libc::write(st.fd, &val as *const _ as *const c_void, 8); }
        1
    } else { set_error(WSAEINVAL); 0 }
}

pub extern "win64" fn wsa_reset_event(h: usize) -> i32 {
    let mut ev = lock(events());
    if let Some(st) = ev.get_mut(&h) {
        st.triggered = false;
        let mut val: u64 = 0;
        unsafe { libc::read(st.fd, &mut val as *mut _ as *mut c_void, 8); }
        1
    } else { set_error(WSAEINVAL); 0 }
}

pub unsafe extern "win64" fn wsa_wait_for_multiple_events(
    count: u32, events_arr: *const usize, wait_all: i32,
    timeout: u32, _alertable: i32,
) -> u32 {
    if count == 0 || count > WSA_MAXIMUM_WAIT_EVENTS || events_arr.is_null() {
        set_error(WSAEINVAL); return WSA_WAIT_FAILED;
    }
    let n = count as usize;
    let handles: Vec<usize> = (0..n).map(|i| std::ptr::read_unaligned(events_arr.add(i))).collect();
    let ev = events();
    let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(n);
    {
        let g = lock(ev);
        for &h in &handles {
            if let Some(st) = g.get(&h) {
                pfds.push(libc::pollfd { fd: st.fd, events: libc::POLLIN, revents: 0 });
            } else {
                set_error(WSAEINVAL); return WSA_WAIT_FAILED;
            }
        }
    }
    let ms = if timeout == u32::MAX { -1i32 } else { timeout as i32 };
    let rc = libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, ms);
    if rc < 0 { set_error(last_error()); return WSA_WAIT_FAILED; }
    if rc == 0 { return WSA_WAIT_TIMEOUT; }
    if wait_all != 0 {
        let ready = pfds.iter().filter(|p| p.revents & libc::POLLIN != 0).count();
        if ready < n { return WSA_WAIT_TIMEOUT; }
        for &h in &handles { wsa_reset_event(h); }
        return WSA_WAIT_EVENT_0;
    }
    for (i, p) in pfds.iter().enumerate() {
        if p.revents & libc::POLLIN != 0 {
            wsa_reset_event(handles[i]);
            return WSA_WAIT_EVENT_0 + i as u32;
        }
    }
    WSA_WAIT_TIMEOUT
}

// ── WSAEventSelect / WSAEnumNetworkEvents ───────────────────────────────
pub extern "win64" fn wsa_event_select(s: usize, event: usize, mask: i32) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    // set socket non-blocking
    unsafe {
        let flags = libc::fcntl(s as i32, libc::F_GETFL);
        if flags >= 0 { libc::fcntl(s as i32, libc::F_SETFL, flags | libc::O_NONBLOCK); }
    }
    lock(event_assoc()).insert(s, (event, mask));
    0
}

pub unsafe extern "win64" fn wsa_enum_network_events(
    s: usize, event: usize, net_events: *mut WsaNetworkEvents,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if net_events.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
    if event != 0 { wsa_reset_event(event); }

    let mask = lock(event_assoc()).get(&s).map(|(_, m)| *m).unwrap_or(0);
    let mut result = WsaNetworkEvents::default();

    // poll the socket to determine which events are ready
    let mut pfd = libc::pollfd { fd: s as i32, events: libc::POLLIN | libc::POLLOUT | libc::POLLPRI | libc::POLLHUP | libc::POLLERR, revents: 0 };
    libc::poll(&mut pfd, 1, 0);

    if pfd.revents & libc::POLLIN != 0 && mask & FD_READ != 0 { result.network_events |= FD_READ; }
    if pfd.revents & libc::POLLOUT != 0 && mask & FD_WRITE != 0 { result.network_events |= FD_WRITE; }
    if pfd.revents & libc::POLLPRI != 0 && mask & FD_OOB != 0 { result.network_events |= FD_OOB; }
    if pfd.revents & libc::POLLIN != 0 && mask & FD_ACCEPT != 0 { result.network_events |= FD_ACCEPT; }
    if pfd.revents & libc::POLLOUT != 0 && mask & FD_CONNECT != 0 { result.network_events |= FD_CONNECT; }
    if pfd.revents & libc::POLLHUP != 0 && mask & FD_CLOSE != 0 { result.network_events |= FD_CLOSE; }

    std::ptr::write_unaligned(net_events, result);
    0
}

// ── WSAIoctl ────────────────────────────────────────────────────────────
const SIO_KEEPALIVE_VALS: u32 = 0x98000004;

pub unsafe extern "win64" fn wsa_ioctl(
    s: usize, ioctl_code: u32,
    in_buf: *const u8, in_len: u32,
    out_buf: *mut u8, out_len: u32,
    bytes_returned: *mut u32,
    _overlapped: usize, _completion: usize,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    match ioctl_code {
        FIONBIO => {
            if in_buf.is_null() || in_len < 4 { set_error(WSAEFAULT); return SOCKET_ERROR; }
            let val = std::ptr::read_unaligned(in_buf as *const u32);
            let mut v = val;
            ioctlsocket(s, FIONBIO, &mut v)
        }
        FIONREAD => {
            if out_buf.is_null() || out_len < 4 { set_error(WSAEFAULT); return SOCKET_ERROR; }
            let mut v = 0u32;
            let rc = ioctlsocket(s, FIONREAD, &mut v);
            if rc == 0 {
                std::ptr::write_unaligned(out_buf as *mut u32, v);
                if !bytes_returned.is_null() { *bytes_returned = 4; }
            }
            rc
        }
        SIO_KEEPALIVE_VALS => {
            if in_buf.is_null() || in_len < 12 { set_error(WSAEFAULT); return SOCKET_ERROR; }
            let onoff = std::ptr::read_unaligned(in_buf as *const u32);
            let keepalive_time = std::ptr::read_unaligned((in_buf.add(4)) as *const u32);
            let keepalive_interval = std::ptr::read_unaligned((in_buf.add(8)) as *const u32);
            let v: i32 = if onoff != 0 { 1 } else { 0 };
            libc::setsockopt(s as i32, libc::SOL_SOCKET, libc::SO_KEEPALIVE,
                &v as *const _ as *const c_void, 4);
            if onoff != 0 {
                let idle = (keepalive_time / 1000).max(1) as i32;
                let intvl = (keepalive_interval / 1000).max(1) as i32;
                libc::setsockopt(s as i32, libc::IPPROTO_TCP, libc::TCP_KEEPIDLE,
                    &idle as *const _ as *const c_void, 4);
                libc::setsockopt(s as i32, libc::IPPROTO_TCP, libc::TCP_KEEPINTVL,
                    &intvl as *const _ as *const c_void, 4);
            }
            if !bytes_returned.is_null() { *bytes_returned = 0; }
            0
        }
        _ => {
            eprintln!("[ws2_32] WSAIoctl: code non supporté {:#010x}", ioctl_code);
            set_error(WSAEOPNOTSUPP); SOCKET_ERROR
        }
    }
}

// ── WSADuplicateSocketW (stub — single-process) ─────────────────────────
pub unsafe extern "win64" fn wsa_duplicate_socket_w(
    s: usize, _pid: u32, info: *mut u8,
) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    if info.is_null() { set_error(WSAEFAULT); return SOCKET_ERROR; }
    // fill the WSAPROTOCOL_INFOW struct with minimal data
    std::ptr::write_bytes(info, 0, 632); // size of WSAPROTOCOL_INFOW
    0
}

// ── WSAStringToAddressW / WSAAddressToStringW ───────────────────────────
pub unsafe extern "win64" fn wsa_string_to_address_w(
    addr_string: *const u16, af: i32, _proto_info: usize,
    addr: *mut u8, addr_len: *mut i32,
) -> i32 {
    if addr_string.is_null() || addr.is_null() || addr_len.is_null() {
        set_error(WSAEFAULT); return SOCKET_ERROR;
    }
    let n = crate::printf::wide_len(addr_string);
    let s = match String::from_utf16(std::slice::from_raw_parts(addr_string, n)) {
        Ok(s) => s, Err(_) => { set_error(WSAEINVAL); return SOCKET_ERROR; }
    };
    // split off optional :port
    let (host, port) = if let Some(idx) = s.rfind(':') {
        let p = s[idx+1..].parse::<u16>().unwrap_or(0);
        (&s[..idx], p)
    } else { (s.as_str(), 0u16) };

    if af == libc::AF_INET {
        if *addr_len < 16 { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let ip: std::net::Ipv4Addr = match host.parse() {
            Ok(ip) => ip, Err(_) => { set_error(WSAEINVAL); return SOCKET_ERROR; }
        };
        let mut sin: libc::sockaddr_in = std::mem::zeroed();
        sin.sin_family = libc::AF_INET as u16;
        sin.sin_port = port.to_be();
        sin.sin_addr.s_addr = u32::from_ne_bytes(ip.octets());
        std::ptr::copy_nonoverlapping(&sin as *const _ as *const u8, addr, 16);
        *addr_len = 16;
        0
    } else if af == libc::AF_INET6 {
        if *addr_len < 28 { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let c = match CString::new(host) { Ok(c) => c, Err(_) => { set_error(WSAEINVAL); return SOCKET_ERROR; } };
        let mut sin6: libc::sockaddr_in6 = std::mem::zeroed();
        sin6.sin6_family = libc::AF_INET6 as u16;
        sin6.sin6_port = port.to_be();
        if posix_inet_pton(libc::AF_INET6, c.as_ptr(), &mut sin6.sin6_addr as *mut _ as *mut c_void) != 1 {
            set_error(WSAEINVAL); return SOCKET_ERROR;
        }
        std::ptr::copy_nonoverlapping(&sin6 as *const _ as *const u8, addr, 28);
        *addr_len = 28;
        0
    } else {
        set_error(10047); SOCKET_ERROR
    }
}

pub unsafe extern "win64" fn wsa_address_to_string_w(
    addr: *const u8, addr_len: u32, _proto_info: usize,
    string: *mut u16, string_len: *mut u32,
) -> i32 {
    if addr.is_null() || string.is_null() || string_len.is_null() {
        set_error(WSAEFAULT); return SOCKET_ERROR;
    }
    let family = std::ptr::read_unaligned(addr as *const u16);
    let text = if family == libc::AF_INET as u16 {
        if addr_len < 16 { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let sin = std::ptr::read_unaligned(addr as *const libc::sockaddr_in);
        let ip = std::net::Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
        let port = u16::from_be(sin.sin_port);
        if port != 0 { format!("{}:{}", ip, port) } else { format!("{}", ip) }
    } else if family == libc::AF_INET6 as u16 {
        if addr_len < 28 { set_error(WSAEFAULT); return SOCKET_ERROR; }
        let sin6 = std::ptr::read_unaligned(addr as *const libc::sockaddr_in6);
        let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
        let port = u16::from_be(sin6.sin6_port);
        if port != 0 { format!("[{}]:{}", ip, port) } else { format!("{}", ip) }
    } else {
        set_error(10047); return SOCKET_ERROR;
    };
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    if (*string_len as usize) < wide.len() {
        *string_len = wide.len() as u32;
        set_error(WSAEFAULT); return SOCKET_ERROR;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), string, wide.len());
    *string_len = wide.len() as u32;
    0
}

// ── WSAAsyncSelect (stub: sets non-blocking, tracks mask) ───────────────
pub extern "win64" fn wsa_async_select(s: usize, _hwnd: usize, _msg: u32, mask: i32) -> i32 {
    if !valid_socket(s) { set_error(WSAENOTSOCK); return SOCKET_ERROR; }
    unsafe {
        let flags = libc::fcntl(s as i32, libc::F_GETFL);
        if flags >= 0 {
            if mask != 0 {
                libc::fcntl(s as i32, libc::F_SETFL, flags | libc::O_NONBLOCK);
            } else {
                libc::fcntl(s as i32, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            }
        }
    }
    0
}

// ── __WSAFDIsSet (FD_ISSET helper) ──────────────────────────────────────
pub unsafe extern "win64" fn wsa_fd_is_set(s: usize, set: *const WinFdSet) -> i32 {
    if set.is_null() { return 0; }
    let count = std::ptr::read_unaligned(set as *const u32) as usize;
    let base = (set as *const u8).add(4) as *const usize;
    for i in 0..count {
        if std::ptr::read_unaligned(base.add(i)) == s { return 1; }
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
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
     "gethostname"=>fn get_host_name,
     "gethostbyname"=>fn gethostbyname,"gethostbyaddr"=>fn gethostbyaddr,
     "getservbyname"=>fn getservbyname,"getservbyport"=>fn getservbyport,
     "getaddrinfo"=>fn get_addr_info_a,
     "GetAddrInfoA"=>fn get_addr_info_a,"GetAddrInfoW"=>fn get_addr_info_w,
     "freeaddrinfo"=>fn free_addr_info_a,
     "FreeAddrInfoA"=>fn free_addr_info_a,"FreeAddrInfoW"=>fn free_addr_info_w,
     "getnameinfo"=>fn getnameinfo,"GetNameInfoA"=>fn getnameinfo,"GetNameInfoW"=>fn get_name_info_w,
     // select / poll
     "select"=>fn select,"WSAPoll"=>fn wsa_poll,
     "__WSAFDIsSet"=>fn wsa_fd_is_set,
     // WSA extended
     "WSASocketW"=>fn wsa_socket_w,"WSASocketA"=>fn wsa_socket_w,
     "WSASend"=>fn wsa_send,"WSARecv"=>fn wsa_recv,
     "WSAConnect"=>fn wsa_connect,
     "WSAIoctl"=>fn wsa_ioctl,
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
    fn init() { unsafe { let mut d = [0u8; 408]; wsa_startup(0x0202, d.as_mut_ptr()); } }
    fn cleanup() { wsa_cleanup(); }

    #[test]
    fn winsock_startup_ouvre_ferme_et_valide_un_socket() {
        unsafe {
            // drain any leftover startups from other tests
            while STARTUPS.load(Ordering::Acquire) > 0 { wsa_cleanup(); }
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
    fn setsockopt_reuseaddr() {
        init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            assert_ne!(s, INVALID_SOCKET);
            let val: i32 = 1;
            assert_eq!(setsockopt(s, WIN_SOL_SOCKET, WIN_SO_REUSEADDR,
                &val as *const _ as *const u8, 4), 0);
            let mut out: i32 = 0;
            let mut len: i32 = 4;
            assert_eq!(getsockopt(s, WIN_SOL_SOCKET, WIN_SO_REUSEADDR,
                &mut out as *mut _ as *mut u8, &mut len), 0);
            assert_ne!(out, 0);
            closesocket(s);
        }
        cleanup();
    }

    #[test]
    fn ioctlsocket_nonblock() {
        init();
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
            assert_eq!(inet_pton(libc::AF_INET, b"10.0.0.1\0".as_ptr(), dst.as_mut_ptr()), 1);
            assert_eq!(dst, [10, 0, 0, 1]);

            let mut buf = [0u8; 32];
            let r = inet_ntop(libc::AF_INET, dst.as_ptr(), buf.as_mut_ptr(), 32);
            assert!(!r.is_null());
            let s = crate::dlls::cstr(buf.as_ptr());
            assert_eq!(s, "10.0.0.1");
        }
    }

    #[test]
    fn select_non_blocking() {
        init();
        unsafe {
            let s = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            // unconnected socket should be writable
            let mut wset = [0u8; 4 + 8]; // count + 1 SOCKET
            std::ptr::write_unaligned(wset.as_mut_ptr() as *mut u32, 1);
            std::ptr::write_unaligned(wset.as_mut_ptr().add(4) as *mut usize, s);
            let tv = WinTimeval { sec: 0, usec: 0 };
            let rc = select(0, std::ptr::null_mut(), wset.as_mut_ptr() as *mut WinFdSet,
                std::ptr::null_mut(), &tv);
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
        init();
        unsafe {
            let srv = socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            let val: i32 = 1;
            setsockopt(srv, WIN_SOL_SOCKET, WIN_SO_REUSEADDR, &val as *const _ as *const u8, 4);
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
            let mut buf = WsaBuf { len: 5, buf: msg.as_ptr() as *mut u8 };
            let mut sent = 0u32;
            assert_eq!(wsa_send(cli, &buf, 1, &mut sent, 0, 0, 0), 0);
            assert_eq!(sent, 5);

            let mut rbuf = [0u8; 16];
            let mut rbufv = WsaBuf { len: 16, buf: rbuf.as_mut_ptr() };
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
}
