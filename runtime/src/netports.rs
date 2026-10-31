//! Ports réseau de la machine, vus de la même façon par les applis Windows
//! et Linux : il n'y a qu'un seul noyau, donc une appli Windows qui écoute
//! sur `127.0.0.1:8080` est joignable par Firefox ou `curl`, et une appli
//! Windows peut joindre un serveur Linux local. Ce module lit les tables du
//! noyau (`/proc/net/{tcp,udp}{,6}`) pour :
//! - `GetExtendedTcpTable`, `GetExtendedUdpTable`, `GetTcpTable`,
//!   `GetUdpTable` d'iphlpapi (netstat, TCPView, jeux qui cherchent un port
//!   libre…), avec le PID propriétaire ;
//! - la page « Ports ouverts » du pare-feu (`winlux --firewall ports`), qui
//!   donne le nom de l'appli (description du `.exe`) et dit si le port est
//!   réservé à la machine ou ouvert au réseau.

use crate::admin;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Un socket du noyau.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Socket {
    pub udp: bool,
    pub v6: bool,
    /// Adresse locale, octets dans l'ordre réseau (4 ou 16 utiles).
    pub local: [u8; 16],
    pub local_port: u16,
    pub remote: [u8; 16],
    pub remote_port: u16,
    /// État Linux (`0A` = écoute pour TCP, `07` pour un UDP lié).
    pub state: u8,
    pub inode: u64,
}

impl Socket {
    pub fn listening(&self) -> bool {
        if self.udp {
            self.state == 0x07 && self.remote_port == 0
        } else {
            self.state == 0x0A
        }
    }

    /// Joignable seulement depuis la machine (boucle locale).
    pub fn local_only(&self) -> bool {
        if self.v6 {
            (self.local[..15].iter().all(|&b| b == 0) && self.local[15] == 1)
                || (self.local[..12] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF]
                    && self.local[12] == 127)
        } else {
            self.local[0] == 127
        }
    }

    pub fn ipv4(&self, remote: bool) -> u32 {
        let a = if remote { &self.remote } else { &self.local };
        u32::from_ne_bytes([a[0], a[1], a[2], a[3]])
    }
}

/// Adresse de `/proc/net` (`0100007F` ou 32 chiffres pour IPv6) : le noyau
/// affiche chaque mot de 32 bits tel qu'il est en mémoire.
fn parse_addr(hex: &str) -> Option<[u8; 16]> {
    let mut out = [0u8; 16];
    if hex.len() != 8 && hex.len() != 32 {
        return None;
    }
    for (i, chunk) in hex.as_bytes().chunks(8).enumerate() {
        let w = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    Some(out)
}

fn parse_endpoint(s: &str) -> Option<([u8; 16], u16)> {
    let (a, p) = s.split_once(':')?;
    Some((parse_addr(a)?, u16::from_str_radix(p, 16).ok()?))
}

pub fn parse_proc_net(text: &str, udp: bool, v6: bool) -> Vec<Socket> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let (Some((local, local_port)), Some((remote, remote_port))) =
            (parse_endpoint(f[1]), parse_endpoint(f[2]))
        else {
            continue;
        };
        let (Ok(state), Ok(inode)) = (u8::from_str_radix(f[3], 16), f[9].parse::<u64>()) else {
            continue;
        };
        out.push(Socket {
            udp,
            v6,
            local,
            local_port,
            remote,
            remote_port,
            state,
            inode,
        });
    }
    out
}

/// Sockets de la machine (`tcp` et/ou `udp`, IPv4 et/ou IPv6).
pub fn sockets(tcp: bool, udp: bool, v4: bool, v6: bool) -> Vec<Socket> {
    let mut out = Vec::new();
    for (file, is_udp, is_v6) in [
        ("tcp", false, false),
        ("tcp6", false, true),
        ("udp", true, false),
        ("udp6", true, true),
    ] {
        if (is_udp && !udp) || (!is_udp && !tcp) || (is_v6 && !v6) || (!is_v6 && !v4) {
            continue;
        }
        if let Some(text) = admin::read_text(&admin::path(&format!("proc/net/{file}"))) {
            out.extend(parse_proc_net(&text, is_udp, is_v6));
        }
    }
    out
}

/// inode de socket → PID (les processus d'autres comptes ne sont visibles
/// qu'en root, comme sous Windows sans élévation).
pub fn socket_owners() -> BTreeMap<u64, u32> {
    let mut out = BTreeMap::new();
    let Ok(procs) = std::fs::read_dir(admin::path("proc")) else {
        return out;
    };
    for p in procs.flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Some(inode) = std::fs::read_link(fd.path()).ok().and_then(|t| {
                t.to_string_lossy()
                    .strip_prefix("socket:[")
                    .and_then(|r| r.strip_suffix(']'))
                    .and_then(|n| n.parse::<u64>().ok())
            }) {
                out.entry(inode).or_insert(pid);
            }
        }
    }
    out
}

// ---------- Nom de l'appli ----------

/// Valeur d'une chaîne `StringFileInfo` (`FileDescription`, `ProductName`)
/// dans les ressources de version d'un PE.
pub fn version_string(pe: &[u8], key: &str) -> Option<String> {
    let needle: Vec<u8> = key
        .encode_utf16()
        .chain([0])
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut from = 0;
    while let Some(pos) = pe[from..]
        .windows(needle.len())
        .position(|w| w == needle.as_slice())
    {
        let key_at = from + pos;
        from = key_at + 2;
        // Structure String : wLength, wValueLength, wType, szKey, remplissage, Value.
        if key_at < 6 || key_at % 2 != 0 {
            continue;
        }
        let start = key_at - 6;
        let value_len = u16::from_le_bytes([pe[start + 2], pe[start + 3]]) as usize;
        let kind = u16::from_le_bytes([pe[start + 4], pe[start + 5]]);
        if kind != 1 || value_len == 0 || value_len > 260 {
            continue;
        }
        let value_at = (key_at + needle.len()).div_ceil(4) * 4;
        let Some(raw) = pe.get(value_at..value_at + value_len * 2) else {
            continue;
        };
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&u| u != 0)
            .collect();
        let s = String::from_utf16_lossy(&units).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Nom affichable d'un programme : description du `.exe` (« Minecraft
/// Launcher »), sinon le nom du fichier.
pub fn app_name(program: &Path) -> String {
    let is_pe = program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("dll"));
    if is_pe {
        let bytes = std::fs::metadata(program)
            .ok()
            .filter(|m| m.len() < 512 * 1024 * 1024)
            .and_then(|_| std::fs::read(program).ok());
        if let Some(b) = bytes {
            for key in ["FileDescription", "ProductName"] {
                if let Some(s) = version_string(&b, key) {
                    return s;
                }
            }
        }
    }
    program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Port en écoute, pour la page « Ports ouverts » du pare-feu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPort {
    pub udp: bool,
    pub port: u16,
    pub local_only: bool,
    pub pid: u32,
    pub program: PathBuf,
    pub name: String,
}

/// Ports en écoute de la machine, un par (protocole, port, portée, PID).
pub fn open_ports() -> Vec<OpenPort> {
    let owners = socket_owners();
    let mut names: BTreeMap<u32, (PathBuf, String)> = BTreeMap::new();
    let mut out: Vec<OpenPort> = Vec::new();
    for s in sockets(true, true, true, true)
        .iter()
        .filter(|s| s.listening())
    {
        let pid = owners.get(&s.inode).copied().unwrap_or(0);
        let (program, name) = names
            .entry(pid)
            .or_insert_with(|| {
                if pid == 0 {
                    return (PathBuf::new(), String::new());
                }
                let dir = admin::path(&format!("proc/{pid}"));
                let prog = crate::firewall::program_of(&dir).unwrap_or_default();
                let name = app_name(&windows_to_linux(&prog));
                (prog, name)
            })
            .clone();
        let p = OpenPort {
            udp: s.udp,
            port: s.local_port,
            local_only: s.local_only(),
            pid,
            program,
            name,
        };
        if !out.iter().any(|o| {
            o.udp == p.udp && o.port == p.port && o.pid == p.pid && o.local_only == p.local_only
        }) {
            out.push(p);
        }
    }
    out.sort_by(|a, b| {
        (a.name.to_lowercase(), a.port, a.udp).cmp(&(b.name.to_lowercase(), b.port, b.udp))
    });
    out
}

/// Chemin de programme relevé dans `cmdline` : il peut être au format Windows.
fn windows_to_linux(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s.len() > 2 && s.as_bytes()[1] == b':' {
        crate::paths::to_linux(&s)
    } else {
        p.to_path_buf()
    }
}

/// Ligne de `ports.txt` : `proto  port  portée  pid  nom  programme`.
pub fn port_line(p: &OpenPort) -> String {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        if p.udp { "UDP" } else { "TCP" },
        p.port,
        if p.local_only { "local" } else { "network" },
        p.pid,
        clean(&p.name),
        clean(&p.program.to_string_lossy())
    )
}

// ---------- iphlpapi ----------

const NO_ERROR: u32 = 0;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;

/// État Linux → `MIB_TCP_STATE`.
pub fn mib_state(linux: u8) -> u32 {
    match linux {
        0x01 => 5,  // ESTABLISHED
        0x02 => 3,  // SYN_SENT
        0x03 => 4,  // SYN_RCVD
        0x04 => 6,  // FIN_WAIT1
        0x05 => 7,  // FIN_WAIT2
        0x06 => 11, // TIME_WAIT
        0x07 => 1,  // CLOSED
        0x08 => 8,  // CLOSE_WAIT
        0x09 => 10, // LAST_ACK
        0x0A => 2,  // LISTEN
        0x0B => 9,  // CLOSING
        _ => 12,    // DELETE_TCB
    }
}

fn port_dword(p: u16) -> u32 {
    p.to_be() as u32
}

/// Table `MIB_TCPTABLE[_OWNER_PID]` / `MIB_TCP6TABLE_OWNER_PID` /
/// `MIB_UDPTABLE[_OWNER_PID]` / `MIB_UDP6TABLE_OWNER_PID` sérialisée.
pub fn build_table(
    udp: bool,
    v6: bool,
    class: u32,
    socks: &[Socket],
    owners: &BTreeMap<u64, u32>,
    sort: bool,
) -> Result<Vec<u8>, u32> {
    // Classes TCP : 0-2 de base, 3-5 avec PID ; UDP : 0 de base, 1 avec PID.
    let (with_pid, filter): (bool, fn(&Socket) -> bool) = if udp {
        match class {
            0 => (false, |_| true),
            1 => (true, |_| true),
            _ => return Err(ERROR_NOT_SUPPORTED),
        }
    } else {
        match class {
            0 => (false, |s| s.state == 0x0A),
            1 => (false, |s| s.state != 0x0A),
            2 => (false, |_| true),
            3 => (true, |s| s.state == 0x0A),
            4 => (true, |s| s.state != 0x0A),
            5 => (true, |_| true),
            _ => return Err(ERROR_NOT_SUPPORTED),
        }
    };
    if v6 && !with_pid {
        return Err(ERROR_INVALID_PARAMETER);
    }
    let mut rows: Vec<&Socket> = socks
        .iter()
        .filter(|s| s.udp == udp && s.v6 == v6 && filter(s))
        .filter(|s| !udp || s.remote_port == 0)
        .collect();
    if sort {
        rows.sort_by_key(|s| (s.local, s.local_port, s.remote, s.remote_port));
    }
    let mut out = (rows.len() as u32).to_le_bytes().to_vec();
    for s in rows {
        let pid = owners.get(&s.inode).copied().unwrap_or(0);
        let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
        match (udp, v6) {
            (false, false) => {
                put(mib_state(s.state));
                put(s.ipv4(false));
                put(port_dword(s.local_port));
                put(s.ipv4(true));
                put(port_dword(s.remote_port));
                if with_pid {
                    put(pid);
                }
            }
            (false, true) => {
                out.extend_from_slice(&s.local);
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&port_dword(s.local_port).to_le_bytes());
                out.extend_from_slice(&s.remote);
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&port_dword(s.remote_port).to_le_bytes());
                out.extend_from_slice(&mib_state(s.state).to_le_bytes());
                out.extend_from_slice(&pid.to_le_bytes());
            }
            (true, false) => {
                put(s.ipv4(false));
                put(port_dword(s.local_port));
                if with_pid {
                    put(pid);
                }
            }
            (true, true) => {
                out.extend_from_slice(&s.local);
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&port_dword(s.local_port).to_le_bytes());
                out.extend_from_slice(&pid.to_le_bytes());
            }
        }
    }
    Ok(out)
}

unsafe fn copy_table(table: *mut u8, size: *mut u32, data: &[u8]) -> u32 {
    if size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    if table.is_null() || (*size as usize) < data.len() {
        *size = data.len() as u32;
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), table, data.len());
    *size = data.len() as u32;
    NO_ERROR
}

unsafe fn extended(
    table: *mut u8,
    size: *mut u32,
    sort: i32,
    af: u32,
    class: u32,
    udp: bool,
) -> u32 {
    let v6 = match af {
        AF_INET => false,
        AF_INET6 => true,
        _ => return ERROR_INVALID_PARAMETER,
    };
    let socks = sockets(!udp, udp, !v6, v6);
    let owners = socket_owners();
    match build_table(udp, v6, class, &socks, &owners, sort != 0) {
        Ok(data) => copy_table(table, size, &data),
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn get_extended_tcp_table(
    table: *mut u8,
    size: *mut u32,
    sort: i32,
    af: u32,
    class: u32,
    _reserved: u32,
) -> u32 {
    extended(table, size, sort, af, class, false)
}

pub unsafe extern "win64" fn get_extended_udp_table(
    table: *mut u8,
    size: *mut u32,
    sort: i32,
    af: u32,
    class: u32,
    _reserved: u32,
) -> u32 {
    extended(table, size, sort, af, class, true)
}

pub unsafe extern "win64" fn get_tcp_table(table: *mut u8, size: *mut u32, sort: i32) -> u32 {
    extended(table, size, sort, AF_INET, 2, false)
}

pub unsafe extern "win64" fn get_udp_table(table: *mut u8, size: *mut u32, sort: i32) -> u32 {
    extended(table, size, sort, AF_INET, 0, true)
}

pub unsafe extern "win64" fn get_tcp_table2(table: *mut u8, size: *mut u32, sort: i32) -> u32 {
    // MIB_TCPROW2 = ligne avec PID + état de délestage (0).
    let socks = sockets(true, false, true, false);
    let owners = socket_owners();
    let Ok(data) = build_table(false, false, 5, &socks, &owners, sort != 0) else {
        return ERROR_NOT_SUPPORTED;
    };
    let mut out = data[..4].to_vec();
    for row in data[4..].chunks(24) {
        out.extend_from_slice(row);
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    copy_table(table, size, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0000000000000000 100 0 0 10 0
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 17 1 0000000000000000 100 0 0 10 0
   2: 0F02000A:D2B4 22D8B85D:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 99 1 0000000000000000 20 4 30 10 -1
";
    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:0BB8 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 555 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn parses_kernel_tables() {
        let s = parse_proc_net(TCP, false, false);
        assert_eq!(s.len(), 3);
        assert_eq!(
            (s[0].local_port, s[0].local[..4].to_vec()),
            (8080, vec![127, 0, 0, 1])
        );
        assert!(s[0].listening() && s[0].local_only());
        assert!(s[1].listening() && !s[1].local_only());
        assert!(!s[2].listening());
        assert_eq!(s[2].remote[..4], [93, 184, 216, 34]);
        assert_eq!(s[2].remote_port, 443);
        let v6 = parse_proc_net(TCP6, false, true);
        assert!(v6[0].local_only());
        assert_eq!(v6[0].local_port, 3000);
    }

    #[test]
    fn mib_tables() {
        let s = parse_proc_net(TCP, false, false);
        let owners: BTreeMap<u64, u32> = [(4242, 1234u32)].into_iter().collect();
        // Écoute avec PID : deux lignes de 24 octets.
        let t = build_table(false, false, 3, &s, &owners, true).unwrap();
        assert_eq!(t.len(), 4 + 2 * 24);
        assert_eq!(u32::from_le_bytes(t[0..4].try_into().unwrap()), 2);
        // Tri par adresse : 0.0.0.0:22 puis 127.0.0.1:8080.
        let row = &t[4 + 24..4 + 48];
        assert_eq!(u32::from_le_bytes(row[0..4].try_into().unwrap()), 2); // LISTEN
        assert_eq!(row[4..8], [127, 0, 0, 1]);
        assert_eq!(row[8..10], [0x1F, 0x90]); // port en ordre réseau
        assert_eq!(u32::from_le_bytes(row[20..24].try_into().unwrap()), 1234);
        // Toutes les connexions, sans PID : 3 × 20 octets.
        assert_eq!(
            build_table(false, false, 2, &s, &owners, false)
                .unwrap()
                .len(),
            4 + 60
        );
        assert_eq!(
            build_table(false, true, 0, &s, &owners, false),
            Err(ERROR_INVALID_PARAMETER)
        );
        assert_eq!(
            build_table(false, false, 8, &s, &owners, false),
            Err(ERROR_NOT_SUPPORTED)
        );
        let v6 = parse_proc_net(TCP6, false, true);
        assert_eq!(
            build_table(false, true, 5, &v6, &owners, false)
                .unwrap()
                .len(),
            4 + 56
        );
        assert_eq!(mib_state(0x01), 5);
    }

    #[test]
    fn reads_version_description() {
        // Structure String : wLength, wValueLength (caractères), wType = 1.
        let mut pe = vec![0u8; 40];
        let key: Vec<u8> = "FileDescription\0"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let value: Vec<u8> = "Serveur de jeu\0"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let start = pe.len();
        pe.extend_from_slice(&0u16.to_le_bytes());
        pe.extend_from_slice(&15u16.to_le_bytes());
        pe.extend_from_slice(&1u16.to_le_bytes());
        pe.extend_from_slice(&key);
        while pe.len() % 4 != 0 {
            pe.push(0);
        }
        pe.extend_from_slice(&value);
        let _ = start;
        assert_eq!(
            version_string(&pe, "FileDescription").as_deref(),
            Some("Serveur de jeu")
        );
        assert_eq!(version_string(&pe, "ProductName"), None);
        assert_eq!(app_name(Path::new("/nonexistent/jeu.exe")), "jeu.exe");
    }
}
