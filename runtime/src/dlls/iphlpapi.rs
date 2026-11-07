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
        // Nom d'adaptateur stable, au format GUID comme sous Windows.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in a.name.bytes() {
            hash = (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3);
        }
        let guid = format!(
            "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
            hash as u32,
            (hash >> 32) as u16,
            (hash >> 48) as u16 & 0x0FFF | 0x4000,
            a.index as u16 | 0x8000,
            hash & 0xFFFF_FFFF_FFFF
        );
        let name = b.ansi(&guid);
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
pub unsafe extern "win64" fn GetAdaptersInfo(_info: usize, size: *mut u32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_BUFFER_OVERFLOW
}
pub unsafe extern "win64" fn GetNetworkParams(_info: usize, size: *mut u32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_BUFFER_OVERFLOW
}
pub unsafe extern "win64" fn GetBestRoute(_dest: u32, _src: u32, _route: usize) -> u32 {
    0
}
pub unsafe extern "win64" fn GetIfTable(_table: usize, size: *mut u32, _order: i32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    122 // ERROR_INSUFFICIENT_BUFFER
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
