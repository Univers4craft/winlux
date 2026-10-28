//! Passerelle Winsock vers les sockets POSIX/Linux, avec codes d'erreur WSA.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const WSA_NOT_INITIALISED: i32 = 10093;
const WSA_INVALID_PARAMETER: i32 = 10022;
const SOCKET_ERROR: i32 = -1;
const WSAHOST_NOT_FOUND: i32 = 11001;
const WSATRY_AGAIN: i32 = 11002;
const WSANO_RECOVERY: i32 = 11003;
const WSATYPE_NOT_FOUND: i32 = 10109;
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
thread_local! {static LAST_WSA_ERROR:std::cell::Cell<i32>=const{std::cell::Cell::new(0)};}
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

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "WSAStartup"=>fn wsa_startup,"WSACleanup"=>fn wsa_cleanup,"WSAGetLastError"=>fn wsa_get_last_error,"WSASetLastError"=>fn wsa_set_last_error,
     "socket"=>fn socket,"closesocket"=>fn closesocket,"connect"=>fn connect,"bind"=>fn bind,"listen"=>fn listen,"accept"=>fn accept,
     "send"=>fn send,"recv"=>fn recv,"sendto"=>fn send_to,"recvfrom"=>fn recv_from,"shutdown"=>fn shutdown,
     "getsockname"=>fn get_socket_name,"getpeername"=>fn get_peer_name,"htons"=>fn htons,"ntohs"=>fn ntohs,"htonl"=>fn htonl,"ntohl"=>fn ntohl,"inet_addr"=>fn inet_addr,"gethostname"=>fn get_host_name,
     "getaddrinfo"=>fn get_addr_info_a,"GetAddrInfoA"=>fn get_addr_info_a,"GetAddrInfoW"=>fn get_addr_info_w,"freeaddrinfo"=>fn free_addr_info_a,"FreeAddrInfoA"=>fn free_addr_info_a,"FreeAddrInfoW"=>fn free_addr_info_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn winsock_startup_ouvre_ferme_et_valide_un_socket() {
        unsafe {
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
}
