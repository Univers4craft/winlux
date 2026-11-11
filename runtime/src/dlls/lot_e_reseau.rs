//! Lot E de #1410 (#1614) : iphlpapi, dnsapi, ws2_32, winhttp et
//! bthprops.cpl, importés par jdk4py, panda3d, nodejs, sharp, Qt et
//! Qt WebEngine.
//!
//! Les tables d'interfaces et d'adresses viennent des interfaces Linux
//! (`iphlpapi::adapters`, compteurs de /sys/class/net) ; l'écho ICMP passe
//! par un socket ICMP « datagramme » (celui de ping, ouvert aux comptes par
//! net.ipv4.ping_group_range), sans privilège.
//!
//! Sources : documentation Microsoft Learn des fonctions et des structures
//! MIB_IF_ROW2, MIB_IF_TABLE2, MIB_UNICASTIPADDRESS_ROW,
//! MIB_ANYCASTIPADDRESS_ROW, SOCKADDR_INET, ICMP_ECHO_REPLY,
//! IP_OPTION_INFORMATION, ICMPV6_ECHO_REPLY, IPV6_ADDRESS_EX (aligné sur
//! 1 octet), DNS_QUERY_REQUEST, DNS_QUERY_RESULT, DNS_MESSAGE_BUFFER,
//! PROTOENT (netioapi.h, ipexport.h, windns.h, winsock2.h).
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use crate::dlls::iphlpapi::{adapters, Adapter};
use crate::process::set_last_error;
use crate::wow32::{pointer_size, write_ptr};
use std::ffi::c_void;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

const ERROR_SUCCESS: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_IO_PENDING: u32 = 997;
const ERROR_NOT_FOUND: u32 = 1168;
const AF_UNSPEC: u16 = 0;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;

/// LUID d'interface de Winlux : type IF_TYPE en bits 48 à 63, index en
/// bits 24 à 47 (comme ConvertInterfaceIndexToLuid).
fn luid(a: &Adapter) -> u64 {
    (if_type(a) as u64) << 48 | (a.index as u64 & 0xFF_FFFF) << 24
}
fn luid_index(l: u64) -> u32 {
    ((l >> 24) & 0xFF_FFFF) as u32
}
/// IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_IEEE80211 ou IF_TYPE_ETHERNET_CSMACD.
fn if_type(a: &Adapter) -> u32 {
    if a.loopback {
        24
    } else if a.wireless {
        71
    } else {
        6
    }
}

fn guid_bytes(text: &str) -> [u8; 16] {
    let hex: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut b = [0u8; 16];
    if hex.len() != 32 {
        return b;
    }
    let n = |i: usize, len: usize| u64::from_str_radix(&hex[i..i + len], 16).unwrap_or(0);
    b[0..4].copy_from_slice(&(n(0, 8) as u32).to_le_bytes());
    b[4..6].copy_from_slice(&(n(8, 4) as u16).to_le_bytes());
    b[6..8].copy_from_slice(&(n(12, 4) as u16).to_le_bytes());
    for i in 0..8 {
        b[8 + i] = n(16 + 2 * i, 2) as u8;
    }
    b
}

fn stat(a: &Adapter, name: &str) -> u64 {
    std::fs::read_to_string(format!("/sys/class/net/{}/statistics/{name}", a.name))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}
fn put_wide(b: &mut [u8], at: usize, text: &str, max: usize) {
    for (i, c) in text.encode_utf16().take(max - 1).enumerate() {
        put(b, at + 2 * i, &c.to_le_bytes());
    }
}

// ============================================================
//  iphlpapi : tables d'interfaces (MIB_IF_ROW2, 1 352 octets)
// ============================================================

const IF_ROW2: usize = 1352;

fn if_row2(a: &Adapter) -> Vec<u8> {
    let mut r = vec![0u8; IF_ROW2];
    put(&mut r, 0, &luid(a).to_le_bytes());
    put(&mut r, 8, &a.index.to_le_bytes());
    put(
        &mut r,
        12,
        &guid_bytes(&crate::dlls::iphlpapi::adapter_guid(a)),
    );
    put_wide(&mut r, 28, &a.name, 257);
    put_wide(&mut r, 542, &a.name, 257);
    let mac = &a.mac[..a.mac.len().min(32)];
    put(&mut r, 1056, &(mac.len() as u32).to_le_bytes());
    put(&mut r, 1060, mac);
    put(&mut r, 1092, mac);
    put(&mut r, 1124, &a.mtu.to_le_bytes());
    put(&mut r, 1128, &if_type(a).to_le_bytes());
    // MediaType (NdisMedium802_3, NdisMediumNative802_11, NdisMediumLoopback),
    // PhysicalMediumType (802_3, Native802_11, Unspecified), AccessType
    // (NET_IF_ACCESS_BROADCAST ou LOOPBACK), DirectionType (émission et réception).
    let (media, physical, access) = if a.loopback {
        (9u32, 0u32, 1u32)
    } else if a.wireless {
        (16, 9, 2)
    } else {
        (0, 14, 2)
    };
    put(&mut r, 1136, &media.to_le_bytes());
    put(&mut r, 1140, &physical.to_le_bytes());
    put(&mut r, 1144, &access.to_le_bytes());
    // HardwareInterface, ConnectorPresent.
    r[1152] = if a.loopback { 0 } else { 0b101 };
    let (oper, connect) = if a.up { (1u32, 1u32) } else { (2, 2) };
    put(&mut r, 1156, &oper.to_le_bytes());
    put(&mut r, 1160, &1u32.to_le_bytes()); // AdminStatus : activée
    put(&mut r, 1164, &connect.to_le_bytes());
    put(&mut r, 1184, &1u32.to_le_bytes()); // NET_IF_CONNECTION_DEDICATED
    put(&mut r, 1192, &a.speed.to_le_bytes());
    put(&mut r, 1200, &a.speed.to_le_bytes());
    let counters = [
        stat(a, "rx_bytes"),
        stat(a, "rx_packets").saturating_sub(stat(a, "multicast")),
        stat(a, "multicast"),
        stat(a, "rx_dropped"),
        stat(a, "rx_errors"),
        0,
        stat(a, "rx_bytes"),
        0,
        0,
        stat(a, "tx_bytes"),
        stat(a, "tx_packets"),
        0,
        stat(a, "tx_dropped"),
        stat(a, "tx_errors"),
        stat(a, "tx_bytes"),
        0,
        0,
        0,
    ];
    for (i, c) in counters.iter().enumerate() {
        put(&mut r, 1208 + 8 * i, &c.to_le_bytes());
    }
    r
}

/// Table MIB_*_TABLE : NumEntries puis les lignes alignées sur 8 octets,
/// allouée comme les autres tables que FreeMibTable rend (malloc).
unsafe fn mib_table(rows: &[Vec<u8>], row_size: usize, out: *mut usize) -> u32 {
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let p = libc::calloc(1, 8 + rows.len() * row_size) as *mut u8;
    if p.is_null() {
        return 8;
    }
    (p as *mut u32).write(rows.len() as u32);
    for (i, r) in rows.iter().enumerate() {
        std::ptr::copy_nonoverlapping(r.as_ptr(), p.add(8 + i * row_size), row_size);
    }
    *out = p as usize;
    ERROR_SUCCESS
}

unsafe extern "win64" fn GetIfTable2(out: *mut usize) -> u32 {
    let rows: Vec<Vec<u8>> = adapters().iter().map(if_row2).collect();
    mib_table(&rows, IF_ROW2, out)
}
/// GetIfTable2Ex : MibIfTableNormal (0) et MibIfTableRaw (1) donnent la
/// même table, sans statistiques plus fraîches à rafraîchir.
unsafe extern "win64" fn GetIfTable2Ex(level: u32, out: *mut usize) -> u32 {
    if level > 2 {
        return ERROR_INVALID_PARAMETER;
    }
    GetIfTable2(out)
}

/// GetIfEntry2 : ligne choisie par InterfaceLuid, sinon InterfaceIndex.
unsafe extern "win64" fn GetIfEntry2(row: *mut u8) -> u32 {
    if row.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let l = (row as *const u64).read_unaligned();
    let index = if l != 0 {
        luid_index(l)
    } else {
        (row.add(8) as *const u32).read_unaligned()
    };
    let Some(a) = adapters().into_iter().find(|a| a.index == index) else {
        return ERROR_FILE_NOT_FOUND;
    };
    let r = if_row2(&a);
    std::ptr::copy_nonoverlapping(r.as_ptr(), row, IF_ROW2);
    ERROR_SUCCESS
}

// ============================================================
//  iphlpapi : adresses (MIB_UNICASTIPADDRESS_ROW, 80 octets)
// ============================================================

fn family_matches(family: u16, ip: &IpAddr) -> bool {
    match family {
        AF_UNSPEC => true,
        AF_INET => ip.is_ipv4(),
        AF_INET6 => ip.is_ipv6(),
        _ => false,
    }
}

/// SOCKADDR_INET (28 octets) : sockaddr_in ou sockaddr_in6.
fn sockaddr_inet(ip: &IpAddr, scope: u32) -> [u8; 28] {
    let mut s = [0u8; 28];
    match ip {
        IpAddr::V4(v4) => {
            s[0..2].copy_from_slice(&AF_INET.to_le_bytes());
            s[4..8].copy_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            s[0..2].copy_from_slice(&AF_INET6.to_le_bytes());
            s[8..24].copy_from_slice(&v6.octets());
            s[24..28].copy_from_slice(&scope.to_le_bytes());
        }
    }
    s
}

fn unicast_row(a: &Adapter, ip: &IpAddr, prefix: u8) -> Vec<u8> {
    let link_local = matches!(ip, IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80);
    let scope = if link_local { a.index } else { 0 };
    let mut r = vec![0u8; 80];
    put(&mut r, 0, &sockaddr_inet(ip, scope));
    put(&mut r, 32, &luid(a).to_le_bytes());
    put(&mut r, 40, &a.index.to_le_bytes());
    // IpPrefixOriginWellKnown / Dhcp ; IpSuffixOriginWellKnown / Dhcp / LinkLayerAddress.
    let (prefix_origin, suffix_origin) = if a.loopback {
        (2u32, 2u32)
    } else if link_local {
        (2, 4)
    } else {
        (3, 3)
    };
    put(&mut r, 44, &prefix_origin.to_le_bytes());
    put(&mut r, 48, &suffix_origin.to_le_bytes());
    put(&mut r, 52, &u32::MAX.to_le_bytes()); // ValidLifetime : infini
    put(&mut r, 56, &u32::MAX.to_le_bytes()); // PreferredLifetime
    r[60] = prefix;
    put(&mut r, 64, &4u32.to_le_bytes()); // IpDadStatePreferred
    put(&mut r, 68, &scope.to_le_bytes());
    r
}

unsafe extern "win64" fn GetUnicastIpAddressTable(family: u16, out: *mut usize) -> u32 {
    if !matches!(family, AF_UNSPEC | AF_INET | AF_INET6) {
        return ERROR_INVALID_PARAMETER;
    }
    let rows: Vec<Vec<u8>> = adapters()
        .iter()
        .flat_map(|a| {
            a.addrs
                .iter()
                .filter(|(ip, _)| family_matches(family, ip))
                .map(|(ip, p)| unicast_row(a, ip, *p))
                .collect::<Vec<_>>()
        })
        .collect();
    let r = mib_table(&rows, 80, out);
    if r == ERROR_SUCCESS && rows.is_empty() {
        return ERROR_NOT_FOUND;
    }
    r
}

/// GetAnycastIpAddressTable : aucune adresse anycast configurée.
unsafe extern "win64" fn GetAnycastIpAddressTable(family: u16, out: *mut usize) -> u32 {
    if !matches!(family, AF_UNSPEC | AF_INET | AF_INET6) {
        return ERROR_INVALID_PARAMETER;
    }
    let r = mib_table(&[], 48, out);
    if r == ERROR_SUCCESS {
        ERROR_NOT_FOUND
    } else {
        r
    }
}

/// ConvertLengthToIpv4Mask : masque dans l'ordre du réseau.
unsafe extern "win64" fn ConvertLengthToIpv4Mask(len: u32, mask: *mut u32) -> u32 {
    if mask.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    if len > 32 {
        *mask = u32::MAX; // INADDR_NONE
        return ERROR_INVALID_PARAMETER;
    }
    let bits: u32 = if len == 0 { 0 } else { u32::MAX << (32 - len) };
    *mask = u32::from_ne_bytes(bits.to_be_bytes());
    ERROR_SUCCESS
}

/// ConvertInterfaceLuidToNameA : même nom que la variante W (#1606).
unsafe extern "win64" fn ConvertInterfaceLuidToNameA(
    luid: *const u64,
    name: *mut u8,
    len: usize,
) -> u32 {
    if luid.is_null() || name.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let text = format!("ethernet_{}", luid_index(luid.read_unaligned()));
    if text.len() + 1 > len {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), name, text.len());
    *name.add(text.len()) = 0;
    ERROR_SUCCESS
}

static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x7C00_0000);
fn new_handle() -> usize {
    NEXT_HANDLE.fetch_add(4, Ordering::Relaxed)
}

/// NotifyRouteChange2 : la table de routage exposée ne change pas
/// (GetIpForwardTable2 la rend vide) ; la poignée se rend à
/// CancelMibChangeNotify2.
unsafe extern "win64" fn NotifyRouteChange2(
    family: u16,
    _callback: usize,
    _context: usize,
    _initial: u8,
    handle: *mut usize,
) -> u32 {
    if handle.is_null() || !matches!(family, AF_UNSPEC | AF_INET | AF_INET6) {
        return ERROR_INVALID_PARAMETER;
    }
    write_ptr(handle as usize, new_handle());
    ERROR_SUCCESS
}

// ============================================================
//  iphlpapi : écho ICMP
// ============================================================

const IP_SUCCESS: u32 = 0;
const IP_BUF_TOO_SMALL: u32 = 11001;
const IP_REQ_TIMED_OUT: u32 = 11010;
const IP_GENERAL_FAILURE: u32 = 11050;

/// Poignées ICMP : famille de chaque poignée ouverte.
fn icmp_handles() -> &'static std::sync::Mutex<std::collections::HashMap<usize, u16>> {
    static H: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, u16>>> =
        std::sync::OnceLock::new();
    H.get_or_init(Default::default)
}

unsafe extern "win64" fn IcmpCreateFile() -> usize {
    let h = new_handle();
    crate::dlls::lock(icmp_handles()).insert(h, AF_INET);
    h
}
unsafe extern "win64" fn Icmp6CreateFile() -> usize {
    let h = new_handle();
    crate::dlls::lock(icmp_handles()).insert(h, AF_INET6);
    h
}
unsafe extern "win64" fn IcmpCloseHandle(h: usize) -> i32 {
    if crate::dlls::lock(icmp_handles()).remove(&h).is_some() {
        1
    } else {
        set_last_error(6);
        0
    }
}

/// Un écho : (adresse qui répond, durée en ms, données reçues) ou le code
/// IP_* de l'échec.
fn ping(
    dest: IpAddr,
    data: &[u8],
    timeout_ms: u32,
    ttl: Option<u8>,
) -> Result<(IpAddr, u32, Vec<u8>), u32> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let (domain, proto, request) = match dest {
        IpAddr::V4(_) => (libc::AF_INET, libc::IPPROTO_ICMP, 8u8),
        IpAddr::V6(_) => (libc::AF_INET6, libc::IPPROTO_ICMPV6, 128u8),
    };
    let fd = unsafe { libc::socket(domain, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, proto) };
    if fd < 0 {
        return Err(IP_GENERAL_FAILURE);
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    if let Some(ttl) = ttl.filter(|&t| t > 0) {
        let (level, opt) = match dest {
            IpAddr::V4(_) => (libc::IPPROTO_IP, libc::IP_TTL),
            IpAddr::V6(_) => (libc::IPPROTO_IPV6, libc::IPV6_UNICAST_HOPS),
        };
        let v = ttl as i32;
        unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                level,
                opt,
                &v as *const i32 as *const c_void,
                4,
            );
        }
    }
    // En-tête ICMP : type, code, somme (calculée par le noyau pour ce
    // type de socket), identifiant (remplacé par le noyau), numéro.
    let mut packet = vec![request, 0, 0, 0, 0, 0, 0, 1];
    packet.extend_from_slice(data);
    let sock = socket2_addr(dest);
    let start = std::time::Instant::now();
    let sent = unsafe {
        libc::sendto(
            fd.as_raw_fd(),
            packet.as_ptr() as *const c_void,
            packet.len(),
            0,
            sock.0.as_ptr() as *const libc::sockaddr,
            sock.1,
        )
    };
    if sent < 0 {
        return Err(IP_GENERAL_FAILURE);
    }
    let mut pfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let wait = timeout_ms.min(i32::MAX as u32) as i32;
    if unsafe { libc::poll(&mut pfd, 1, wait) } <= 0 {
        return Err(IP_REQ_TIMED_OUT);
    }
    let mut buf = vec![0u8; 65536];
    let mut from = [0u8; 128];
    let mut from_len = from.len() as libc::socklen_t;
    let n = unsafe {
        libc::recvfrom(
            fd.as_raw_fd(),
            buf.as_mut_ptr() as *mut c_void,
            buf.len(),
            0,
            from.as_mut_ptr() as *mut libc::sockaddr,
            &mut from_len,
        )
    };
    if n < 8 {
        return Err(IP_GENERAL_FAILURE);
    }
    let rtt = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
    let who = match dest {
        IpAddr::V4(_) => IpAddr::from([from[4], from[5], from[6], from[7]]),
        IpAddr::V6(_) => IpAddr::from(<[u8; 16]>::try_from(&from[8..24]).unwrap_or([0; 16])),
    };
    Ok((who, rtt, buf[8..n as usize].to_vec()))
}

fn socket2_addr(ip: IpAddr) -> (Vec<u8>, libc::socklen_t) {
    match ip {
        IpAddr::V4(v4) => {
            let mut s = vec![0u8; 16];
            s[0..2].copy_from_slice(&(libc::AF_INET as u16).to_ne_bytes());
            s[4..8].copy_from_slice(&v4.octets());
            (s, 16)
        }
        IpAddr::V6(v6) => {
            let mut s = vec![0u8; 28];
            s[0..2].copy_from_slice(&(libc::AF_INET6 as u16).to_ne_bytes());
            s[8..24].copy_from_slice(&v6.octets());
            (s, 28)
        }
    }
}

/// Taille d'ICMP_ECHO_REPLY chez l'appelant.
fn echo_reply_size() -> usize {
    if pointer_size() == 4 {
        28
    } else {
        40
    }
}

/// Écho IPv4 : ICMP_ECHO_REPLY puis les données renvoyées ; rend le
/// nombre de réponses (0 en cas d'échec, code dans GetLastError).
unsafe fn echo4(
    handle: usize,
    dest: u32,
    data: *const u8,
    size: u16,
    options: *const u8,
    reply: *mut u8,
    reply_size: u32,
    timeout: u32,
) -> u32 {
    if crate::dlls::lock(icmp_handles()).get(&handle) != Some(&AF_INET) {
        set_last_error(6);
        return 0;
    }
    let ps = pointer_size();
    let rs = echo_reply_size();
    if reply.is_null() || (reply_size as usize) < rs + size as usize + 8 {
        set_last_error(IP_BUF_TOO_SMALL);
        return 0;
    }
    let payload = if data.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, size as usize).to_vec()
    };
    let ttl = (!options.is_null()).then(|| *options);
    let dest_ip = IpAddr::from(dest.to_ne_bytes());
    std::ptr::write_bytes(reply, 0, rs);
    let r = ping(dest_ip, &payload, timeout, ttl);
    let (status, count) = match &r {
        Ok((who, rtt, back)) => {
            if let IpAddr::V4(v4) = who {
                std::ptr::copy_nonoverlapping(v4.octets().as_ptr(), reply, 4);
            }
            (reply.add(8) as *mut u32).write_unaligned(*rtt);
            let n = back.len().min(size as usize);
            (reply.add(12) as *mut u16).write_unaligned(n as u16);
            let data_at = reply.add(rs);
            std::ptr::copy_nonoverlapping(back.as_ptr(), data_at, n);
            write_ptr(reply as usize + 16, data_at as usize);
            // IP_OPTION_INFORMATION : Ttl (celui demandé, sinon 128).
            *reply.add(16 + ps) = ttl.filter(|&t| t > 0).unwrap_or(128);
            (IP_SUCCESS, 1)
        }
        Err(code) => (*code, 0),
    };
    (reply.add(4) as *mut u32).write_unaligned(status);
    if count == 0 {
        set_last_error(status);
    }
    count
}

unsafe extern "win64" fn IcmpSendEcho(
    handle: usize,
    dest: u32,
    data: *const u8,
    size: u16,
    options: *const u8,
    reply: *mut u8,
    reply_size: u32,
    timeout: u32,
) -> u32 {
    echo4(
        handle, dest, data, size, options, reply, reply_size, timeout,
    )
}

/// Fin d'un appel « 2 » : avec un événement, l'écho a déjà eu lieu,
/// l'événement est signalé et l'appel rend ERROR_IO_PENDING comme
/// l'appel asynchrone documenté ; sinon le résultat direct.
fn finish_async(event: usize, count: u32) -> u32 {
    if event != 0 {
        crate::dlls::kernel32::set_event(event);
        set_last_error(ERROR_IO_PENDING);
        return 0;
    }
    count
}

unsafe extern "win64" fn IcmpSendEcho2Ex(
    handle: usize,
    event: usize,
    _apc: usize,
    _apc_ctx: usize,
    _source: u32,
    dest: u32,
    data: *const u8,
    size: u16,
    options: *const u8,
    reply: *mut u8,
    reply_size: u32,
    timeout: u32,
) -> u32 {
    let n = echo4(
        handle, dest, data, size, options, reply, reply_size, timeout,
    );
    finish_async(event, n)
}

/// Écho IPv6 : ICMPV6_ECHO_REPLY (IPV6_ADDRESS_EX sur 26 octets, Status,
/// RoundTripTime ; 36 octets) puis les données.
unsafe extern "win64" fn Icmp6SendEcho2(
    handle: usize,
    event: usize,
    _apc: usize,
    _apc_ctx: usize,
    _source: *const u8,
    dest: *const u8,
    data: *const u8,
    size: u16,
    options: *const u8,
    reply: *mut u8,
    reply_size: u32,
    timeout: u32,
) -> u32 {
    if crate::dlls::lock(icmp_handles()).get(&handle) != Some(&AF_INET6) {
        set_last_error(6);
        return 0;
    }
    if dest.is_null() || reply.is_null() || (reply_size as usize) < 36 + size as usize + 8 {
        set_last_error(IP_BUF_TOO_SMALL);
        return 0;
    }
    let addr: [u8; 16] = std::slice::from_raw_parts(dest.add(8), 16)
        .try_into()
        .unwrap_or([0; 16]);
    let payload = if data.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, size as usize).to_vec()
    };
    let ttl = (!options.is_null()).then(|| *options);
    std::ptr::write_bytes(reply, 0, 36);
    let count = match ping(IpAddr::from(addr), &payload, timeout, ttl) {
        Ok((who, rtt, back)) => {
            if let IpAddr::V6(v6) = who {
                std::ptr::copy_nonoverlapping(v6.octets().as_ptr(), reply.add(6), 16);
            }
            (reply.add(32) as *mut u32).write_unaligned(rtt);
            let n = back.len().min(size as usize);
            std::ptr::copy_nonoverlapping(back.as_ptr(), reply.add(36), n);
            1
        }
        Err(code) => {
            (reply.add(28) as *mut u32).write_unaligned(code);
            set_last_error(code);
            0
        }
    };
    finish_async(event, count)
}

pub fn iphlpapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetIfTable2" => fn GetIfTable2,
        "GetIfTable2Ex" => fn GetIfTable2Ex,
        "GetIfEntry2" => fn GetIfEntry2,
        "GetUnicastIpAddressTable" => fn GetUnicastIpAddressTable,
        "GetAnycastIpAddressTable" => fn GetAnycastIpAddressTable,
        "ConvertLengthToIpv4Mask" => fn ConvertLengthToIpv4Mask,
        "ConvertInterfaceLuidToNameA" => fn ConvertInterfaceLuidToNameA,
        "NotifyRouteChange2" => fn NotifyRouteChange2,
        "IcmpCreateFile" => fn IcmpCreateFile,
        "Icmp6CreateFile" => fn Icmp6CreateFile,
        "IcmpCloseHandle" => fn IcmpCloseHandle,
        "IcmpSendEcho" => fn IcmpSendEcho,
        "IcmpSendEcho2Ex" => fn IcmpSendEcho2Ex,
        "Icmp6SendEcho2" => fn Icmp6SendEcho2,
    }
}

// ============================================================
//  dnsapi
// ============================================================

/// DnsQueryEx : résolution faite tout de suite (le résultat est dans
/// DNS_QUERY_RESULT, sans rappel), comme un appel achevé de façon
/// synchrone.
unsafe extern "win64" fn DnsQueryEx(request: *const u8, result: *mut u8, _cancel: usize) -> i32 {
    if request.is_null() || result.is_null() {
        return ERROR_INVALID_PARAMETER as i32;
    }
    let ps = pointer_size();
    let read = |at: usize| crate::wow32::read_ptr(request as usize + at);
    let name = read(ps) as *const u16;
    let ty = (request.add(2 * ps) as *const u16).read_unaligned();
    if name.is_null() {
        return ERROR_INVALID_PARAMETER as i32;
    }
    let mut records = 0usize;
    let status = crate::dlls::games_win32::dns_query(&crate::dlls::wstr(name), ty, &mut records);
    // DNS_QUERY_RESULT : Version, QueryStatus, QueryOptions, pQueryRecords.
    (result.add(4) as *mut i32).write_unaligned(status);
    write_ptr(result as usize + 16, records);
    status
}

/// DnsExtractRecordsFromMessage_W : réponse au format réseau dont
/// l'appelant a remis les compteurs d'en-tête dans l'ordre de l'hôte
/// (DNS_BYTE_FLIP_HEADER_COUNTS, comme le veut windns.h).
unsafe extern "win64" fn DnsExtractRecordsFromMessage_W(
    msg: *const u8,
    len: u16,
    results: *mut usize,
) -> i32 {
    if msg.is_null() || results.is_null() || len < 12 {
        return ERROR_INVALID_PARAMETER as i32;
    }
    let mut m = std::slice::from_raw_parts(msg, len as usize).to_vec();
    for at in [4, 6, 8, 10] {
        let v = u16::from_le_bytes([m[at], m[at + 1]]);
        m[at..at + 2].copy_from_slice(&v.to_be_bytes());
    }
    crate::dlls::games_win32::records_from_message(&m, 255, results)
}

/// DnsWriteQuestionToBuffer_W : en-tête DNS (Xid, récursion demandée,
/// une question) et la question au format réseau.
unsafe extern "win64" fn DnsWriteQuestionToBuffer_W(
    buf: *mut u8,
    size: *mut u32,
    name: *const u16,
    ty: u16,
    xid: u16,
    recursion: i32,
) -> i32 {
    if size.is_null() || name.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut m = Vec::with_capacity(64);
    m.extend(xid.to_ne_bytes());
    m.extend(if recursion != 0 { [0x01u8, 0] } else { [0, 0] });
    m.extend([0, 1, 0, 0, 0, 0, 0, 0]);
    for label in crate::dlls::wstr(name).trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            set_last_error(123);
            return 0;
        }
        m.push(label.len() as u8);
        m.extend(label.as_bytes());
    }
    m.push(0);
    m.extend(ty.to_be_bytes());
    m.extend(1u16.to_be_bytes()); // classe IN
    if buf.is_null() || (*size as usize) < m.len() {
        *size = m.len() as u32;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(m.as_ptr(), buf, m.len());
    *size = m.len() as u32;
    1
}

pub fn dnsapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DnsQueryEx" => fn DnsQueryEx,
        "DnsExtractRecordsFromMessage_W" => fn DnsExtractRecordsFromMessage_W,
        "DnsWriteQuestionToBuffer_W" => fn DnsWriteQuestionToBuffer_W,
    }
}

// ============================================================
//  ws2_32
// ============================================================

unsafe extern "win64" fn WSAStringToAddressA(
    text: *const u8,
    family: i32,
    info: usize,
    addr: *mut u8,
    len: *mut i32,
) -> i32 {
    let wide: Vec<u16> = if text.is_null() {
        Vec::new()
    } else {
        crate::dlls::cstr(text)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    };
    let p = if text.is_null() {
        std::ptr::null()
    } else {
        wide.as_ptr()
    };
    crate::dlls::ws2_32::wsa_string_to_address_w(p, family, info, addr, len)
}

/// getprotobyname : protocoles de base (fichier « protocol » de Windows).
/// PROTOENT vit dans un bloc par protocole, sous 4 Gio pour un appelant
/// x86, et reste valable pour le fil comme le veut Winsock.
unsafe extern "win64" fn getprotobyname(name: *const u8) -> usize {
    const TABLE: [(&str, i16); 6] = [
        ("ip", 0),
        ("icmp", 1),
        ("tcp", 6),
        ("udp", 17),
        ("ipv6", 41),
        ("ipv6-icmp", 58),
    ];
    if name.is_null() {
        crate::dlls::ws2_32::set_error(10014); // WSAEFAULT
        return 0;
    }
    let wanted = crate::dlls::cstr(name).to_ascii_lowercase();
    let Some(&(proto_name, number)) = TABLE.iter().find(|(n, _)| *n == wanted) else {
        crate::dlls::ws2_32::set_error(11001); // WSAHOST_NOT_FOUND
        return 0;
    };
    static CACHE: std::sync::Mutex<Vec<(&'static str, usize)>> = std::sync::Mutex::new(Vec::new());
    let mut cache = crate::dlls::lock(&CACHE);
    if let Some(&(_, p)) = cache.iter().find(|(n, _)| *n == proto_name) {
        return p;
    }
    let ps = pointer_size();
    // p_name, p_aliases (liste vide), p_proto ; puis le pointeur nul de
    // la liste d'alias et le nom.
    let entry = 3 * ps;
    let block =
        crate::dlls::kernel32_misc::local_alloc(0x40, entry + ps + proto_name.len() + 1) as usize;
    if block == 0 {
        crate::dlls::ws2_32::set_error(10055);
        return 0;
    }
    let text = block + entry + ps;
    std::ptr::copy_nonoverlapping(proto_name.as_ptr(), text as *mut u8, proto_name.len());
    write_ptr(block, text);
    write_ptr(block + ps, block + entry);
    ((block + 2 * ps) as *mut i16).write_unaligned(number);
    cache.push((proto_name, block));
    block
}

pub fn ws2_32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WSAStringToAddressA" => fn WSAStringToAddressA,
        "getprotobyname" => fn getprotobyname,
    }
}

// ============================================================
//  winhttp : résolveur de proxy
// ============================================================

/// WinHttpCreateProxyResolver : pas de moteur PAC ni de découverte WPAD
/// (comme WinHttpGetProxyForUrl, #1439) ; l'appelant garde la connexion
/// directe.
unsafe extern "win64" fn WinHttpCreateProxyResolver(_session: usize, resolver: *mut usize) -> u32 {
    if !resolver.is_null() {
        write_ptr(resolver as usize, 0);
    }
    ERROR_NOT_SUPPORTED
}
unsafe extern "win64" fn WinHttpGetProxyForUrlEx(
    _r: usize,
    _url: usize,
    _opt: usize,
    _ctx: usize,
) -> u32 {
    12180 // ERROR_WINHTTP_AUTODETECTION_FAILED
}
unsafe extern "win64" fn WinHttpGetProxyResult(_r: usize, _result: usize) -> u32 {
    12019 // ERROR_WINHTTP_INCORRECT_HANDLE_STATE
}
unsafe extern "win64" fn WinHttpFreeProxyResult(_result: usize) {}

pub fn winhttp_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinHttpCreateProxyResolver" => fn WinHttpCreateProxyResolver,
        "WinHttpGetProxyForUrlEx" => fn WinHttpGetProxyForUrlEx,
        "WinHttpGetProxyResult" => fn WinHttpGetProxyResult,
        "WinHttpFreeProxyResult" => fn WinHttpFreeProxyResult,
    }
}

// ============================================================
//  bthprops.cpl : enregistrements SDP
// ============================================================

/// Élément SDP lu à `at` : (type, taille de l'en-tête, taille des données).
fn sdp_element(s: &[u8], at: usize) -> Option<(u8, usize, usize)> {
    let d = *s.get(at)?;
    let (kind, size) = (d >> 3, d & 7);
    let (head, len) = match size {
        0 if kind == 0 => (1, 0),
        0..=4 => (1, 1usize << size),
        5 => (2, *s.get(at + 1)? as usize),
        6 => (
            3,
            u16::from_be_bytes([*s.get(at + 1)?, *s.get(at + 2)?]) as usize,
        ),
        _ => (
            5,
            u32::from_be_bytes(s.get(at + 1..at + 5)?.try_into().ok()?) as usize,
        ),
    };
    (at + head + len <= s.len()).then_some((kind, head, len))
}

/// BluetoothSdpGetAttributeValue : l'enregistrement est une séquence
/// d'identifiants (UINT16) suivis de leur valeur ; rend l'élément brut de
/// l'attribut dans SDP_ELEMENT_DATA (type, spécificateur, puis sa valeur
/// ou ses octets). Winlux n'a pas encore de pile Bluetooth : ces
/// enregistrements ne viennent que de l'appelant.
unsafe extern "win64" fn BluetoothSdpGetAttributeValue(
    record: *const u8,
    len: u32,
    attribute: u16,
    out: *mut u8,
) -> u32 {
    if record.is_null() || out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let s = std::slice::from_raw_parts(record, len as usize);
    let Some((6, head, body)) = sdp_element(s, 0) else {
        return ERROR_INVALID_PARAMETER;
    };
    let mut at = head;
    while at < head + body {
        let Some((1, ih, il)) = sdp_element(s, at) else {
            return ERROR_INVALID_PARAMETER;
        };
        let id = if il == 2 {
            u16::from_be_bytes([s[at + ih], s[at + ih + 1]])
        } else {
            0
        };
        let value = at + ih + il;
        let Some((vk, vh, vl)) = sdp_element(s, value) else {
            return ERROR_INVALID_PARAMETER;
        };
        if id == attribute {
            sdp_element_data(s, value, vk, vh, vl, out);
            return ERROR_SUCCESS;
        }
        at = value + vh + vl;
    }
    ERROR_FILE_NOT_FOUND
}

/// SDP_ELEMENT_DATA : type (SDP_TYPE), spécificateur, puis l'union ; les
/// séquences, alternatives, chaînes et URL y sont rendues par (pointeur
/// sur les octets, longueur).
unsafe fn sdp_element_data(s: &[u8], at: usize, kind: u8, head: usize, len: usize, out: *mut u8) {
    let ps = pointer_size();
    std::ptr::write_bytes(out, 0, 8 + 2 * ps.max(8));
    let data = &s[at + head..at + head + len];
    // SDP_TYPE : NIL 0, UINT 1, INT 2, UUID 3, STRING 4, BOOLEAN 5,
    // SEQUENCE 6, ALTERNATIVE 7, URL 8 (même numérotation que le flux).
    (out as *mut u32).write_unaligned(kind as u32);
    let union = out.add(8);
    match kind {
        4 | 6 | 7 | 8 => {
            write_ptr(union as usize, data.as_ptr() as usize);
            (union.add(ps) as *mut u32).write_unaligned(len as u32);
        }
        _ => {
            for (i, b) in data.iter().rev().take(16).enumerate() {
                *union.add(i) = *b;
            }
        }
    }
}

/// BluetoothSdpGetContainerElementData : parcours d'une séquence ; la
/// poignée d'élément est la position suivante dans le conteneur.
unsafe extern "win64" fn BluetoothSdpGetContainerElementData(
    container: *const u8,
    len: u32,
    element: *mut usize,
    out: *mut u8,
) -> u32 {
    if container.is_null() || element.is_null() || out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let s = std::slice::from_raw_parts(container, len as usize);
    let Some((6 | 7, head, body)) = sdp_element(s, 0) else {
        return ERROR_INVALID_PARAMETER;
    };
    let at = match crate::wow32::read_ptr(element as usize) {
        0 => head,
        p => p.saturating_sub(container as usize),
    };
    if at >= head + body {
        return ERROR_NO_MORE_ITEMS;
    }
    let Some((k, h, l)) = sdp_element(s, at) else {
        return ERROR_INVALID_PARAMETER;
    };
    sdp_element_data(s, at, k, h, l, out);
    write_ptr(element as usize, container as usize + at + h + l);
    ERROR_SUCCESS
}

pub fn bthprops_exports() -> crate::dlls::Exports {
    crate::exports! {
        "BluetoothSdpGetAttributeValue" => fn BluetoothSdpGetAttributeValue,
        "BluetoothSdpGetContainerElementData" => fn BluetoothSdpGetContainerElementData,
    }
}

/// Exports du lot E pour les DLL réseau.
pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        ("iphlpapi", iphlpapi_exports()),
        ("dnsapi", dnsapi_exports()),
        ("ws2_32", ws2_32_exports()),
        ("winhttp", winhttp_exports()),
        ("bthprops.cpl", bthprops_exports()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_d_interfaces_et_adresses() {
        unsafe {
            let mut t = 0usize;
            assert_eq!(GetIfTable2(&mut t), 0);
            let n = *(t as *const u32) as usize;
            assert_eq!(n, adapters().len());
            if n > 0 {
                let row = (t + 8) as *const u8;
                let idx = (row.add(8) as *const u32).read();
                let mut probe = vec![0u8; IF_ROW2];
                probe[8..12].copy_from_slice(&idx.to_le_bytes());
                assert_eq!(GetIfEntry2(probe.as_mut_ptr()), 0);
                assert_eq!(&probe[..1124], std::slice::from_raw_parts(row, 1124));
            }
            libc::free(t as *mut c_void);
            let r = GetUnicastIpAddressTable(AF_INET, &mut t);
            assert!(r == 0 || r == ERROR_NOT_FOUND);
            libc::free(t as *mut c_void);
            assert_eq!(GetAnycastIpAddressTable(AF_UNSPEC, &mut t), ERROR_NOT_FOUND);
            libc::free(t as *mut c_void);

            let mut m = 0u32;
            assert_eq!(ConvertLengthToIpv4Mask(24, &mut m), 0);
            assert_eq!(m.to_ne_bytes(), [255, 255, 255, 0]);
            assert_eq!(ConvertLengthToIpv4Mask(33, &mut m), ERROR_INVALID_PARAMETER);
            assert_eq!(m, u32::MAX);
        }
    }

    #[test]
    fn echo_icmp_et_poignees() {
        unsafe {
            let h = IcmpCreateFile();
            let mut reply = vec![0u8; 128];
            let data = *b"winlux";
            let n = IcmpSendEcho(
                h,
                u32::from_ne_bytes([127, 0, 0, 1]),
                data.as_ptr(),
                6,
                std::ptr::null(),
                reply.as_mut_ptr(),
                128,
                1000,
            );
            if n == 1 {
                assert_eq!(&reply[0..4], &[127, 0, 0, 1]);
                assert_eq!(u16::from_le_bytes([reply[12], reply[13]]), 6);
                assert_eq!(&reply[40..46], b"winlux");
            } else {
                // Banc sans socket ICMP pour ce compte (ping_group_range).
                let status = u32::from_le_bytes(reply[4..8].try_into().unwrap());
                assert!(matches!(status, IP_GENERAL_FAILURE | IP_REQ_TIMED_OUT));
            }
            assert_eq!(
                IcmpSendEcho(
                    h,
                    0,
                    data.as_ptr(),
                    6,
                    std::ptr::null(),
                    reply.as_mut_ptr(),
                    20,
                    10
                ),
                0
            );
            assert_eq!(IcmpCloseHandle(h), 1);
            assert_eq!(IcmpCloseHandle(h), 0);
        }
    }

    #[test]
    fn dns_et_protocoles() {
        unsafe {
            let name: Vec<u16> = "exemple.org\0".encode_utf16().collect();
            let mut size = 0u32;
            assert_eq!(
                DnsWriteQuestionToBuffer_W(std::ptr::null_mut(), &mut size, name.as_ptr(), 1, 7, 1),
                0
            );
            assert_eq!(size, 12 + 13 + 4);
            let mut buf = vec![0u8; size as usize];
            assert_eq!(
                DnsWriteQuestionToBuffer_W(buf.as_mut_ptr(), &mut size, name.as_ptr(), 1, 7, 1),
                1
            );
            assert_eq!(&buf[2..6], &[1, 0, 0, 1]);
            assert_eq!(&buf[12..20], b"\x07exemple");

            let p = getprotobyname(b"TCP\0".as_ptr());
            assert_ne!(p, 0);
            assert_eq!(crate::dlls::cstr(*(p as *const *const u8)), "tcp");
            assert_eq!(*((p + 16) as *const i16), 6);
            assert_eq!(p, getprotobyname(b"tcp\0".as_ptr()));
            assert_eq!(getprotobyname(b"inconnu\0".as_ptr()), 0);
        }
    }

    #[test]
    fn attributs_sdp() {
        unsafe {
            // Séquence : attribut 0x0100 = chaîne « ab », attribut 0x0004 = UINT8 3.
            let rec = [
                0x35, 10, 0x09, 0x01, 0x00, 0x25, 2, b'a', b'b', 0x09, 0x00, 0x04,
            ];
            let rec = [&rec[..], &[0x08, 3]].concat();
            let mut rec = rec;
            rec[1] = (rec.len() - 2) as u8;
            let mut out = [0u8; 32];
            assert_eq!(
                BluetoothSdpGetAttributeValue(
                    rec.as_ptr(),
                    rec.len() as u32,
                    0x0004,
                    out.as_mut_ptr()
                ),
                0
            );
            assert_eq!((out[0], out[8]), (1, 3));
            assert_eq!(
                BluetoothSdpGetAttributeValue(
                    rec.as_ptr(),
                    rec.len() as u32,
                    0x0100,
                    out.as_mut_ptr()
                ),
                0
            );
            assert_eq!(out[0], 4);
            assert_eq!(
                BluetoothSdpGetAttributeValue(
                    rec.as_ptr(),
                    rec.len() as u32,
                    0x0200,
                    out.as_mut_ptr()
                ),
                ERROR_FILE_NOT_FOUND
            );
            let mut el = 0usize;
            let mut count = 0;
            while BluetoothSdpGetContainerElementData(
                rec.as_ptr(),
                rec.len() as u32,
                &mut el,
                out.as_mut_ptr(),
            ) == 0
            {
                count += 1;
            }
            assert_eq!(count, 4);
        }
    }
}
