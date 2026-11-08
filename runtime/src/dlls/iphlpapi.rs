//! Stubs pour iphlpapi.dll — informations réseau.
//! Electron interroge les adaptateurs pour détecter la connectivité.
#![allow(non_snake_case)]

const ERROR_BUFFER_OVERFLOW: u32 = 111;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_DATA: u32 = 232;

const AF_INET_WIN: u16 = 2;
const AF_INET6_WIN: u16 = 23;
const GAA_FLAG_SKIP_UNICAST: u32 = 0x1;
const GAA_FLAG_SKIP_DNS_SERVER: u32 = 0x8;
const GAA_FLAG_SKIP_FRIENDLY_NAME: u32 = 0x20;
const GAA_FLAG_INCLUDE_GATEWAYS: u32 = 0x80;

/// Taille d'IP_ADAPTER_ADDRESSES_LH (x64, Windows 10).
const ADAPTER_SIZE: usize = 448;
const UNICAST_SIZE: usize = 64;
/// IP_ADAPTER_DNS_SERVER_ADDRESS et IP_ADAPTER_GATEWAY_ADDRESS_LH.
const LINKED_ADDR_SIZE: usize = 32;

/// Une interface Linux, telle que Windows la décrit.
#[derive(Debug, Default, Clone)]
pub struct Adapter {
    pub name: String,
    pub index: u32,
    pub up: bool,
    pub loopback: bool,
    pub wireless: bool,
    pub mac: Vec<u8>,
    pub mtu: u32,
    pub speed: u64,
    /// (adresse, longueur de préfixe)
    pub addrs: Vec<(std::net::IpAddr, u8)>,
    pub gateways: Vec<std::net::IpAddr>,
}

fn sys_net(name: &str, file: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/class/net/{name}/{file}"))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Passerelles IPv4 par défaut, par interface (/proc/net/route).
fn ipv4_gateways() -> Vec<(String, std::net::Ipv4Addr)> {
    let text = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 3 || f[1] != "00000000" {
                return None;
            }
            let gw = u32::from_str_radix(f[2], 16).ok()?;
            (gw != 0).then(|| (f[0].to_string(), std::net::Ipv4Addr::from(gw.to_le_bytes())))
        })
        .collect()
}

/// Serveurs DNS de /etc/resolv.conf.
pub fn dns_servers(text: &str) -> Vec<std::net::IpAddr> {
    text.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            (w.next() == Some("nameserver"))
                .then(|| w.next())
                .flatten()
                .and_then(|a| a.split('%').next())
                .and_then(|a| a.parse().ok())
        })
        .collect()
}

fn prefix_len(mask: *const libc::sockaddr) -> u8 {
    if mask.is_null() {
        return 0;
    }
    unsafe {
        match (*mask).sa_family as i32 {
            libc::AF_INET => {
                let m = &*(mask as *const libc::sockaddr_in);
                u32::from_be(m.sin_addr.s_addr).count_ones() as u8
            }
            libc::AF_INET6 => {
                let m = &*(mask as *const libc::sockaddr_in6);
                m.sin6_addr
                    .s6_addr
                    .iter()
                    .map(|b| b.count_ones())
                    .sum::<u32>() as u8
            }
            _ => 0,
        }
    }
}

/// Interfaces de la machine (getifaddrs + /sys/class/net).
pub fn adapters() -> Vec<Adapter> {
    let mut out: Vec<Adapter> = Vec::new();
    unsafe {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut at = list;
        while !at.is_null() {
            let i = &*at;
            at = i.ifa_next;
            let name = std::ffi::CStr::from_ptr(i.ifa_name)
                .to_string_lossy()
                .into_owned();
            let pos = match out.iter().position(|a| a.name == name) {
                Some(p) => p,
                None => {
                    out.push(Adapter {
                        index: libc::if_nametoindex(i.ifa_name),
                        up: i.ifa_flags & libc::IFF_UP as u32 != 0
                            && i.ifa_flags & libc::IFF_RUNNING as u32 != 0,
                        loopback: i.ifa_flags & libc::IFF_LOOPBACK as u32 != 0,
                        name: name.clone(),
                        ..Default::default()
                    });
                    out.len() - 1
                }
            };
            let a = &mut out[pos];
            if i.ifa_addr.is_null() {
                continue;
            }
            match (*i.ifa_addr).sa_family as i32 {
                libc::AF_INET => {
                    let s = &*(i.ifa_addr as *const libc::sockaddr_in);
                    let ip = std::net::Ipv4Addr::from(u32::from_be(s.sin_addr.s_addr));
                    a.addrs.push((ip.into(), prefix_len(i.ifa_netmask)));
                }
                libc::AF_INET6 => {
                    let s = &*(i.ifa_addr as *const libc::sockaddr_in6);
                    let ip = std::net::Ipv6Addr::from(s.sin6_addr.s6_addr);
                    a.addrs.push((ip.into(), prefix_len(i.ifa_netmask)));
                }
                libc::AF_PACKET => {
                    let s = &*(i.ifa_addr as *const libc::sockaddr_ll);
                    let n = (s.sll_halen as usize).min(8);
                    a.mac = s.sll_addr[..n].to_vec();
                }
                _ => {}
            }
        }
        libc::freeifaddrs(list);
    }
    let gateways = ipv4_gateways();
    for a in &mut out {
        a.mtu = sys_net(&a.name, "mtu")
            .and_then(|m| m.parse().ok())
            .unwrap_or(if a.loopback { 65536 } else { 1500 });
        a.wireless = std::path::Path::new(&format!("/sys/class/net/{}/wireless", a.name)).exists();
        a.speed = sys_net(&a.name, "speed")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|&s| s > 0)
            .map(|s| s as u64 * 1_000_000)
            .unwrap_or(1_000_000_000);
        a.gateways = gateways
            .iter()
            .filter(|(n, _)| *n == a.name)
            .map(|(_, g)| (*g).into())
            .collect();
    }
    out
}

/// Tampon en construction : les pointeurs sont des décalages, corrigés une
/// fois l'adresse finale connue.
struct Builder {
    buf: Vec<u8>,
    fixups: Vec<(usize, usize)>,
}

impl Builder {
    fn alloc(&mut self, size: usize) -> usize {
        let at = self.buf.len().next_multiple_of(8);
        self.buf.resize(at + size, 0);
        at
    }
    fn u32_at(&mut self, at: usize, v: u32) {
        self.buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn u64_at(&mut self, at: usize, v: u64) {
        self.buf[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn ptr_at(&mut self, at: usize, target: usize) {
        self.fixups.push((at, target));
    }
    fn wide(&mut self, s: &str) -> usize {
        let units: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
        let at = self.alloc(units.len() * 2);
        for (k, u) in units.iter().enumerate() {
            self.buf[at + 2 * k..at + 2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
        at
    }
    fn ansi(&mut self, s: &str) -> usize {
        let at = self.alloc(s.len() + 1);
        self.buf[at..at + s.len()].copy_from_slice(s.as_bytes());
        at
    }
    /// sockaddr Windows ; renvoie (décalage, longueur).
    fn sockaddr(&mut self, ip: std::net::IpAddr, scope: u32) -> (usize, i32) {
        match ip {
            std::net::IpAddr::V4(v4) => {
                let at = self.alloc(16);
                self.buf[at..at + 2].copy_from_slice(&AF_INET_WIN.to_le_bytes());
                self.buf[at + 4..at + 8].copy_from_slice(&v4.octets());
                (at, 16)
            }
            std::net::IpAddr::V6(v6) => {
                let at = self.alloc(28);
                self.buf[at..at + 2].copy_from_slice(&AF_INET6_WIN.to_le_bytes());
                self.buf[at + 8..at + 24].copy_from_slice(&v6.octets());
                let link_local = v6.segments()[0] & 0xffc0 == 0xfe80;
                if link_local {
                    self.u32_at(at + 24, scope);
                }
                (at, 28)
            }
        }
    }
    /// Liste chaînée d'éléments {Length, Flags/Reserved, Next, SOCKET_ADDRESS}.
    fn linked(&mut self, head_ptr: usize, items: &[std::net::IpAddr], scope: u32) {
        let mut prev = head_ptr;
        for ip in items {
            let at = self.alloc(LINKED_ADDR_SIZE);
            self.u32_at(at, LINKED_ADDR_SIZE as u32);
            let (sa, len) = self.sockaddr(*ip, scope);
            self.ptr_at(at + 16, sa);
            self.u32_at(at + 24, len as u32);
            self.ptr_at(prev, at);
            prev = at + 8;
        }
    }
}

/// Nom d'adaptateur stable, au format GUID comme sous Windows.
pub(crate) fn adapter_guid(a: &Adapter) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in a.name.bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        hash as u32,
        (hash >> 32) as u16,
        (hash >> 48) as u16 & 0x0FFF | 0x4000,
        a.index as u16 | 0x8000,
        hash & 0xFFFF_FFFF_FFFF
    )
}

fn family_ok(family: u32, ip: &std::net::IpAddr) -> bool {
    match family {
        2 => ip.is_ipv4(),
        23 => ip.is_ipv6(),
        _ => true,
    }
}

/// Construit la liste IP_ADAPTER_ADDRESSES_LH (pointeurs relatifs au début).
fn build(list: &[Adapter], family: u32, flags: u32, dns: &[std::net::IpAddr]) -> Builder {
    let mut b = Builder {
        buf: Vec::new(),
        fixups: Vec::new(),
    };
    // Les en-têtes d'abord, pour que la liste commence au début du tampon.
    let heads: Vec<usize> = list.iter().map(|_| b.alloc(ADAPTER_SIZE)).collect();
    for (k, a) in list.iter().enumerate() {
        let h = heads[k];
        b.u32_at(h, ADAPTER_SIZE as u32);
        b.u32_at(h + 4, a.index);
        if let Some(&next) = heads.get(k + 1) {
            b.ptr_at(h + 8, next);
        }
        let name = b.ansi(&adapter_guid(a));
        b.ptr_at(h + 16, name);
        if flags & GAA_FLAG_SKIP_UNICAST == 0 {
            let mut prev = h + 24;
            for (ip, plen) in a.addrs.iter().filter(|(ip, _)| family_ok(family, ip)) {
                let u = b.alloc(UNICAST_SIZE);
                b.u32_at(u, UNICAST_SIZE as u32);
                let (sa, len) = b.sockaddr(*ip, a.index);
                b.ptr_at(u + 16, sa);
                b.u32_at(u + 24, len as u32);
                b.u32_at(u + 32, 1); // IpPrefixOriginManual
                b.u32_at(u + 36, 1); // IpSuffixOriginManual
                b.u32_at(u + 40, 4); // IpDadStatePreferred
                b.u32_at(u + 44, u32::MAX);
                b.u32_at(u + 48, u32::MAX);
                b.u32_at(u + 52, u32::MAX);
                b.buf[u + 56] = *plen;
                b.ptr_at(prev, u);
                prev = u + 8;
            }
        }
        if flags & GAA_FLAG_SKIP_DNS_SERVER == 0 && !a.loopback && a.up {
            let servers: Vec<_> = dns
                .iter()
                .copied()
                .filter(|ip| family_ok(family, ip))
                .collect();
            b.linked(h + 48, &servers, a.index);
        }
        let empty = b.wide("");
        b.ptr_at(h + 56, empty);
        let desc = b.wide(&a.name);
        b.ptr_at(h + 64, desc);
        let friendly = if flags & GAA_FLAG_SKIP_FRIENDLY_NAME == 0 {
            b.wide(&a.name)
        } else {
            empty
        };
        b.ptr_at(h + 72, friendly);
        let n = a.mac.len().min(8);
        b.buf[h + 80..h + 80 + n].copy_from_slice(&a.mac[..n]);
        b.u32_at(h + 88, n as u32);
        // IP_ADAPTER_DDNS_ENABLED | IPV4_ENABLED | IPV6_ENABLED
        b.u32_at(h + 92, 0x1 | 0x80 | 0x100);
        b.u32_at(h + 96, a.mtu);
        let if_type = if a.loopback {
            24 // IF_TYPE_SOFTWARE_LOOPBACK
        } else if a.wireless {
            71 // IF_TYPE_IEEE80211
        } else {
            6 // IF_TYPE_ETHERNET_CSMACD
        };
        b.u32_at(h + 100, if_type);
        b.u32_at(h + 104, if a.up { 1 } else { 2 });
        b.u32_at(h + 108, a.index);
        for z in 0..16 {
            b.u32_at(h + 112 + 4 * z, if z <= 2 { a.index } else { 0 });
        }
        b.u64_at(h + 184, a.speed);
        b.u64_at(h + 192, a.speed);
        if flags & GAA_FLAG_INCLUDE_GATEWAYS != 0 {
            let gws: Vec<_> = a
                .gateways
                .iter()
                .copied()
                .filter(|ip| family_ok(family, ip))
                .collect();
            b.linked(h + 208, &gws, a.index);
        }
        b.u32_at(h + 216, 25);
        b.u32_at(h + 220, 25);
        // NET_LUID : index sur les bits 24..47, type sur 48..63.
        b.u64_at(h + 224, ((a.index as u64) << 24) | ((if_type as u64) << 48));
        b.u32_at(h + 248, 1); // CompartmentId
        b.u32_at(h + 268, 1); // NET_IF_CONNECTION_DEDICATED
    }
    b
}

pub unsafe extern "win64" fn GetAdaptersAddresses(
    family: u32,
    flags: u32,
    _reserved: usize,
    addrs: *mut u8,
    size: *mut u32,
) -> u32 {
    if size.is_null() || !matches!(family, 0 | 2 | 23) {
        return ERROR_INVALID_PARAMETER;
    }
    let list = adapters();
    if list.is_empty() {
        return ERROR_NO_DATA;
    }
    let dns = dns_servers(&std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default());
    let b = build(&list, family, flags, &dns);
    if addrs.is_null() || (*size as usize) < b.buf.len() {
        *size = b.buf.len() as u32;
        return ERROR_BUFFER_OVERFLOW;
    }
    std::ptr::copy_nonoverlapping(b.buf.as_ptr(), addrs, b.buf.len());
    for (at, target) in b.fixups {
        (addrs.add(at) as *mut usize).write_unaligned(addrs as usize + target);
    }
    *size = b.buf.len() as u32;
    0
}
// Structures d'iptypes.h et d'ifmib.h (x64), d'après leur documentation
// publique. Les listes chaînées (IP_ADDR_STRING) suivent la structure
// principale dans le tampon de l'appelant, qui annonce sa taille : trop
// petit, la fonction rend la taille exacte, jamais 0 (sinon .NET réalloue
// à 0 et recommence sans fin, #1048).

/// IP_ADDR_STRING : Next, IpAddress[16], IpMask[16], Context.
const ADDR_STRING_SIZE: usize = 48;
/// FIXED_INFO_W2KSP1.
const FIXED_INFO_SIZE: usize = 600;
const FI_DOMAIN: usize = 132;
const FI_CURRENT_DNS: usize = 264;
const FI_DNS_LIST: usize = 272;
const FI_NODE_TYPE: usize = 320;
const FI_ENABLE_DNS: usize = 592;
/// IP_ADAPTER_INFO.
const ADAPTER_INFO_SIZE: usize = 704;
/// MIB_IFROW.
const IFROW_SIZE: usize = 860;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const BROADCAST_NODETYPE: u32 = 1;

/// Chaîne ANSI terminée par un zéro, tronquée à `cap` octets zéro compris.
unsafe fn put_str(at: *mut u8, cap: usize, s: &str) {
    let n = s.len().min(cap - 1);
    std::ptr::copy_nonoverlapping(s.as_ptr(), at, n);
    *at.add(n) = 0;
}

unsafe fn put_u32(at: *mut u8, v: u32) {
    (at as *mut u32).write_unaligned(v);
}

unsafe fn put_ptr(at: *mut u8, v: usize) {
    (at as *mut usize).write_unaligned(v);
}

/// Écrit une IP_ADDR_STRING en `at`.
unsafe fn put_addr_string(at: *mut u8, next: usize, ip: &str, mask: &str) {
    put_ptr(at, next);
    put_str(at.add(8), 16, ip);
    put_str(at.add(24), 16, mask);
    put_u32(at.add(40), 0);
}

fn ipv4_mask(prefix: u8) -> std::net::Ipv4Addr {
    let bits = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32) as u32)
    };
    std::net::Ipv4Addr::from(bits)
}

/// Domaine DNS : `domain`, sinon le premier `search` de resolv.conf.
pub fn dns_domain(text: &str) -> String {
    let mut search = None;
    for l in text.lines() {
        let mut w = l.split_whitespace();
        match (w.next(), w.next()) {
            (Some("domain"), Some(d)) => return d.to_string(),
            (Some("search"), Some(d)) if search.is_none() => search = Some(d.to_string()),
            _ => {}
        }
    }
    search.unwrap_or_default()
}

/// `GetAdaptersInfo` : adaptateurs IPv4, boucle locale exclue comme sous
/// Windows.
pub unsafe extern "win64" fn GetAdaptersInfo(info: *mut u8, size: *mut u32) -> u32 {
    if size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let list: Vec<Adapter> = adapters().into_iter().filter(|a| !a.loopback).collect();
    if list.is_empty() {
        return ERROR_NO_DATA;
    }
    let v4 = |a: &Adapter| -> Vec<(std::net::Ipv4Addr, u8)> {
        a.addrs
            .iter()
            .filter_map(|(ip, p)| match ip {
                std::net::IpAddr::V4(v) => Some((*v, *p)),
                _ => None,
            })
            .collect()
    };
    // Adresses et passerelles au-delà de la première : IP_ADDR_STRING en plus.
    let extra = |a: &Adapter| {
        let gws = a.gateways.iter().filter(|g| g.is_ipv4()).count();
        v4(a).len().saturating_sub(1) + gws.saturating_sub(1)
    };
    let need: usize = list
        .iter()
        .map(|a| ADAPTER_INFO_SIZE + extra(a) * ADDR_STRING_SIZE)
        .sum();
    if info.is_null() || (*size as usize) < need {
        *size = need as u32;
        return ERROR_BUFFER_OVERFLOW;
    }
    std::ptr::write_bytes(info, 0, need);
    // Les structures principales d'abord, les IP_ADDR_STRING en plus ensuite.
    let mut tail = list.len() * ADAPTER_INFO_SIZE;
    for (i, a) in list.iter().enumerate() {
        let p = info.add(i * ADAPTER_INFO_SIZE);
        let next = if i + 1 < list.len() {
            p as usize + ADAPTER_INFO_SIZE
        } else {
            0
        };
        put_ptr(p, next);
        put_u32(p.add(8), a.index);
        put_str(p.add(12), 260, &adapter_guid(a));
        put_str(p.add(272), 132, &a.name);
        let mac = &a.mac[..a.mac.len().min(8)];
        put_u32(p.add(404), mac.len() as u32);
        std::ptr::copy_nonoverlapping(mac.as_ptr(), p.add(408), mac.len());
        put_u32(p.add(416), a.index);
        put_u32(p.add(420), if a.wireless { 71 } else { 6 });
        let mut write_list = |at: usize, items: Vec<(String, String)>| {
            let items = if items.is_empty() {
                vec![(String::new(), String::new())]
            } else {
                items
            };
            let mut cur = at;
            for (k, (ip, mask)) in items.iter().enumerate() {
                let next = if k + 1 < items.len() {
                    info as usize + tail
                } else {
                    0
                };
                put_addr_string(info.add(cur), next, ip, mask);
                if next != 0 {
                    cur = tail;
                    tail += ADDR_STRING_SIZE;
                }
            }
        };
        let base = i * ADAPTER_INFO_SIZE;
        write_list(
            base + 440,
            v4(a)
                .into_iter()
                .map(|(ip, p)| (ip.to_string(), ipv4_mask(p).to_string()))
                .collect(),
        );
        write_list(
            base + 488,
            a.gateways
                .iter()
                .filter(|g| g.is_ipv4())
                .map(|g| (g.to_string(), "255.255.255.255".to_string()))
                .collect(),
        );
        write_list(base + 536, Vec::new());
        write_list(base + 592, Vec::new());
        write_list(base + 640, Vec::new());
    }
    *size = need as u32;
    0
}

/// `GetNetworkParams` : FIXED_INFO, nom d'hôte, domaine et serveurs DNS.
pub unsafe extern "win64" fn GetNetworkParams(info: *mut u8, size: *mut u32) -> u32 {
    if size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let conf = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let dns: Vec<String> = dns_servers(&conf)
        .into_iter()
        .filter(|ip| ip.is_ipv4())
        .map(|ip| ip.to_string())
        .collect();
    let need = FIXED_INFO_SIZE + dns.len().saturating_sub(1) * ADDR_STRING_SIZE;
    if info.is_null() || (*size as usize) < need {
        *size = need as u32;
        return ERROR_BUFFER_OVERFLOW;
    }
    std::ptr::write_bytes(info, 0, need);
    let host = crate::dlls::kernel32::get_hostname_string();
    let host = host.split('.').next().unwrap_or_default();
    put_str(info, 132, host);
    put_str(info.add(FI_DOMAIN), 132, &dns_domain(&conf));
    if !dns.is_empty() {
        put_ptr(info.add(FI_CURRENT_DNS), info as usize + FI_DNS_LIST);
    }
    for (k, ip) in dns.iter().enumerate() {
        let at = if k == 0 {
            FI_DNS_LIST
        } else {
            FIXED_INFO_SIZE + (k - 1) * ADDR_STRING_SIZE
        };
        let next = if k + 1 < dns.len() {
            info as usize + FIXED_INFO_SIZE + k * ADDR_STRING_SIZE
        } else {
            0
        };
        put_addr_string(info.add(at), next, ip, "");
    }
    put_u32(info.add(FI_NODE_TYPE), BROADCAST_NODETYPE);
    put_u32(info.add(FI_ENABLE_DNS), 1);
    *size = need as u32;
    0
}
pub unsafe extern "win64" fn GetBestRoute(_dest: u32, _src: u32, _route: usize) -> u32 {
    0
}
fn stat(name: &str, file: &str) -> u32 {
    sys_net(name, &format!("statistics/{file}"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0) as u32
}

/// `GetIfTable` : MIB_IFTABLE, une MIB_IFROW par interface.
pub unsafe extern "win64" fn GetIfTable(table: *mut u8, size: *mut u32, order: i32) -> u32 {
    if size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let mut list = adapters();
    if order != 0 {
        list.sort_by_key(|a| a.index);
    }
    let need = 4 + list.len() * IFROW_SIZE;
    if table.is_null() || (*size as usize) < need {
        *size = need as u32;
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::write_bytes(table, 0, need);
    put_u32(table, list.len() as u32);
    for (i, a) in list.iter().enumerate() {
        let r = table.add(4 + i * IFROW_SIZE);
        let wname: Vec<u16> = format!("\\DEVICE\\TCPIP_{}", adapter_guid(a))
            .encode_utf16()
            .take(255)
            .collect();
        std::ptr::copy_nonoverlapping(wname.as_ptr() as *const u8, r, wname.len() * 2);
        put_u32(r.add(512), a.index);
        let if_type = if a.loopback {
            24
        } else if a.wireless {
            71
        } else {
            6
        };
        put_u32(r.add(516), if_type);
        put_u32(r.add(520), a.mtu);
        put_u32(r.add(524), a.speed.min(u32::MAX as u64) as u32);
        let mac = &a.mac[..a.mac.len().min(8)];
        put_u32(r.add(528), mac.len() as u32);
        std::ptr::copy_nonoverlapping(mac.as_ptr(), r.add(532), mac.len());
        put_u32(r.add(540), if a.up { 1 } else { 2 });
        put_u32(r.add(544), if a.up { 5 } else { 2 });
        for (off, file) in [
            (552, "rx_bytes"),
            (556, "rx_packets"),
            (560, "multicast"),
            (564, "rx_dropped"),
            (568, "rx_errors"),
            (576, "tx_bytes"),
            (580, "tx_packets"),
            (588, "tx_dropped"),
            (592, "tx_errors"),
        ] {
            put_u32(r.add(off), stat(&a.name, file));
        }
        let descr = &a.name.as_bytes()[..a.name.len().min(255)];
        put_u32(r.add(600), descr.len() as u32 + 1);
        std::ptr::copy_nonoverlapping(descr.as_ptr(), r.add(604), descr.len());
    }
    *size = need as u32;
    0
}
pub unsafe extern "win64" fn GetBestInterfaceEx(_addr: usize, _idx: *mut u32) -> u32 {
    0
}
pub unsafe extern "win64" fn GetNumberOfInterfaces(num: *mut u32) -> u32 {
    if !num.is_null() {
        *num = 1;
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetAdaptersAddresses" => fn GetAdaptersAddresses,
        "GetAdaptersInfo" => fn GetAdaptersInfo,
        "GetNetworkParams" => fn GetNetworkParams,
        "GetBestRoute" => fn GetBestRoute,
        "GetIfTable" => fn GetIfTable,
        "GetBestInterfaceEx" => fn GetBestInterfaceEx,
        "GetNumberOfInterfaces" => fn GetNumberOfInterfaces,
        "GetExtendedTcpTable" => fn crate::netports::get_extended_tcp_table,
        "GetExtendedUdpTable" => fn crate::netports::get_extended_udp_table,
        "GetTcpTable" => fn crate::netports::get_tcp_table,
        "GetTcpTable2" => fn crate::netports::get_tcp_table2,
        "GetUdpTable" => fn crate::netports::get_udp_table,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parametres_reseau_taille_puis_contenu() {
        unsafe {
            // Tampon trop petit : la taille exacte, jamais 0 (#1048).
            let mut size = 0u32;
            assert_eq!(
                GetNetworkParams(std::ptr::null_mut(), &mut size),
                ERROR_BUFFER_OVERFLOW
            );
            assert!(size as usize >= FIXED_INFO_SIZE);
            let mut buf = vec![0u64; size as usize / 8 + 1];
            let p = buf.as_mut_ptr() as *mut u8;
            assert_eq!(GetNetworkParams(p, &mut size), 0);
            let host = std::ffi::CStr::from_ptr(p as *const libc::c_char);
            assert!(!host.to_bytes().is_empty(), "nom d'hôte rempli");
            assert_eq!((p.add(FI_ENABLE_DNS) as *const u32).read_unaligned(), 1);
            // La liste DNS est chaînée dans le tampon.
            let mut next = (p.add(FI_DNS_LIST) as *const usize).read_unaligned();
            while next != 0 {
                assert!(next >= p as usize && next < p as usize + size as usize);
                next = (next as *const usize).read_unaligned();
            }
        }
    }

    #[test]
    fn table_des_interfaces() {
        unsafe {
            let mut size = 0u32;
            assert_eq!(
                GetIfTable(std::ptr::null_mut(), &mut size, 1),
                ERROR_INSUFFICIENT_BUFFER
            );
            assert!(size as usize >= 4 + IFROW_SIZE, "au moins la boucle locale");
            let mut buf = vec![0u32; size as usize / 4 + 1];
            let t = buf.as_mut_ptr() as *mut u8;
            assert_eq!(GetIfTable(t, &mut size, 1), 0);
            let n = (t as *const u32).read_unaligned() as usize;
            assert_eq!(size as usize, 4 + n * IFROW_SIZE);
            let rows: Vec<*const u8> = (0..n)
                .map(|i| t.add(4 + i * IFROW_SIZE) as *const u8)
                .collect();
            assert!(rows
                .iter()
                .any(|r| (r.add(516) as *const u32).read_unaligned() == 24));
            let idx: Vec<u32> = rows
                .iter()
                .map(|r| (r.add(512) as *const u32).read_unaligned())
                .collect();
            assert!(idx.windows(2).all(|w| w[0] <= w[1]), "triée par index");
        }
    }

    #[test]
    fn adaptateurs_ipv4_et_domaine() {
        assert_eq!(
            dns_domain("search lan maison\ndomain exemple.fr\n"),
            "exemple.fr"
        );
        assert_eq!(dns_domain("nameserver 1.1.1.1\nsearch lan\n"), "lan");
        assert_eq!(ipv4_mask(24).to_string(), "255.255.255.0");
        assert_eq!(ipv4_mask(0).to_string(), "0.0.0.0");
        unsafe {
            let mut size = 0u32;
            let r = GetAdaptersInfo(std::ptr::null_mut(), &mut size);
            if r == ERROR_NO_DATA {
                return; // seulement la boucle locale ici
            }
            assert_eq!(r, ERROR_BUFFER_OVERFLOW);
            assert!(size as usize >= ADAPTER_INFO_SIZE);
            let mut buf = vec![0u64; size as usize / 8 + 1];
            let p = buf.as_mut_ptr() as *mut u8;
            assert_eq!(GetAdaptersInfo(p, &mut size), 0);
            let name = std::ffi::CStr::from_ptr(p.add(12) as *const libc::c_char);
            assert!(name.to_bytes().starts_with(b"{"));
        }
    }

    #[test]
    fn adaptateurs_reseau() {
        unsafe {
            let mut size = 0u32;
            assert_eq!(
                GetAdaptersAddresses(0, 0x80, 0, std::ptr::null_mut(), &mut size),
                ERROR_BUFFER_OVERFLOW
            );
            assert!(size as usize >= ADAPTER_SIZE);
            let mut buf = vec![0u64; size as usize / 8 + 1];
            let base = buf.as_mut_ptr() as *mut u8;
            assert_eq!(GetAdaptersAddresses(0, 0x80, 0, base, &mut size), 0);
            // La boucle locale est toujours là, avec 127.0.0.1.
            let mut a = base as *const u8;
            let mut found = false;
            while !a.is_null() {
                assert_eq!((a as *const u32).read_unaligned(), ADAPTER_SIZE as u32);
                let if_type = (a.add(100) as *const u32).read_unaligned();
                let mut u = (a.add(24) as *const usize).read_unaligned() as *const u8;
                while !u.is_null() {
                    let sa = (u.add(16) as *const usize).read_unaligned() as *const u8;
                    let fam = (sa as *const u16).read_unaligned();
                    if if_type == 24 && fam == AF_INET_WIN && *sa.add(4) == 127 {
                        found = true;
                        assert_eq!(*u.add(56), 8);
                    }
                    u = (u.add(8) as *const usize).read_unaligned() as *const u8;
                }
                a = (a.add(8) as *const usize).read_unaligned() as *const u8;
            }
            assert!(found);
            // Tampon trop court d'un octet : la taille nécessaire revient.
            let need = size;
            let mut short = need - 1;
            assert_eq!(
                GetAdaptersAddresses(0, 0x80, 0, base, &mut short),
                ERROR_BUFFER_OVERFLOW
            );
            assert_eq!(short, need);
            assert_eq!(
                GetAdaptersAddresses(99, 0, 0, base, &mut short),
                ERROR_INVALID_PARAMETER
            );
        }
    }

    #[test]
    fn liste_construite_hors_ligne() {
        let a = Adapter {
            name: "wlan0".into(),
            index: 3,
            up: true,
            wireless: true,
            mac: vec![2, 0, 0, 0, 0, 1],
            mtu: 1500,
            speed: 1,
            addrs: vec![
                ("192.168.1.20".parse().unwrap(), 24),
                ("fe80::1".parse().unwrap(), 64),
            ],
            gateways: vec!["192.168.1.1".parse().unwrap()],
            ..Default::default()
        };
        let dns = dns_servers("# c\nnameserver 192.168.1.1\nnameserver fe80::1%wlan0\nsearch x\n");
        assert_eq!(dns.len(), 2);
        let b = build(std::slice::from_ref(&a), 2, 0x80, &dns);
        // IPv4 seulement : 1 unicast, 1 DNS, 1 passerelle.
        let u = b.fixups.iter().filter(|(at, _)| *at == 24).count();
        assert_eq!(u, 1);
        assert_eq!(u32::from_le_bytes(b.buf[100..104].try_into().unwrap()), 71);
        assert_eq!(u32::from_le_bytes(b.buf[104..108].try_into().unwrap()), 1);
        assert!(b.fixups.iter().any(|(at, _)| *at == 48));
        assert!(b.fixups.iter().any(|(at, _)| *at == 208));
    }
}
