//! Pare-feu Winlux : règles entrantes et sortantes, profils réseau (domaine,
//! privé, public), traduction en ruleset nftables, journal des connexions
//! bloquées et alerte à la première écoute d'une application.
//!
//! Configuration : `/etc/winlux/firewall.conf` (Latin-1, `clé=valeur`), écrite
//! par l'application Pare-feu du bureau ou par `winlux --firewall answer`.
//! Le service `winlux --firewall daemon` applique la configuration, suit le
//! journal du noyau (`/dev/kmsg`) et surveille les sockets en écoute.
//!
//! Règles par application :
//! - entrantes : le service relève les ports où l'application écoute
//!   (`/proc/net/{tcp,udp}{,6}`) et ouvre ou ferme exactement ces ports ;
//! - sortantes : les processus de l'application sont placés dans le cgroup
//!   `/sys/fs/cgroup/winlux-fw/r<id>`, filtré par `socket cgroupv2`.
//!   Un processus est rattaché au plus deux secondes après son démarrage.

use crate::admin;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const CONFIG: &str = "etc/winlux/firewall.conf";
pub const STATE_DIR: &str = "var/lib/winlux/firewall";
pub const RUN_DIR: &str = "run/winlux/firewall";
const LOG_MAX: u64 = 128 * 1024;
const TABLE: &str = "winlux_fw";
pub const PROFILES: [&str; 3] = ["domain", "private", "public"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block,
}

impl Verdict {
    fn parse(s: &str) -> Option<Verdict> {
        match s {
            "allow" => Some(Verdict::Allow),
            "block" => Some(Verdict::Block),
            _ => None,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Verdict::Allow => "allow",
            Verdict::Block => "block",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    Any,
    Tcp,
    Udp,
    Icmp,
}

impl Proto {
    fn parse(s: &str) -> Option<Proto> {
        match s {
            "any" | "" => Some(Proto::Any),
            "tcp" => Some(Proto::Tcp),
            "udp" => Some(Proto::Udp),
            "icmp" => Some(Proto::Icmp),
            _ => None,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Proto::Any => "any",
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
            Proto::Icmp => "icmp",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub id: u32,
    pub name: String,
    pub dir: Dir,
    pub verdict: Verdict,
    pub proto: Proto,
    /// `80`, `80,443`, `8000-8100` ; vide = tous les ports.
    pub ports: String,
    /// Programme concerné (chemin Linux ou Windows) ; vide = tous.
    pub program: String,
    /// Bit 0 = domaine, bit 1 = privé, bit 2 = public.
    pub profiles: u8,
    pub enabled: bool,
}

/// Liste de ports validée, prête pour un ensemble nftables.
pub fn parse_ports(s: &str) -> Option<Vec<(u16, u16)>> {
    let mut out = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<u16>().ok()?, b.trim().parse::<u16>().ok()?),
            None => {
                let p = part.parse::<u16>().ok()?;
                (p, p)
            }
        };
        if a == 0 || b < a {
            return None;
        }
        out.push((a, b));
    }
    Some(out)
}

fn port_set(ports: &[(u16, u16)]) -> String {
    let items: Vec<String> = ports
        .iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            }
        })
        .collect();
    format!("{{ {} }}", items.join(", "))
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

impl Rule {
    pub fn parse(line: &str) -> Option<Rule> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 9 {
            return None;
        }
        let mut profiles = 0u8;
        for p in f[7].split(',') {
            match p.trim() {
                "any" => profiles = 7,
                name => {
                    if let Some(i) = PROFILES.iter().position(|&x| x == name) {
                        profiles |= 1 << i;
                    }
                }
            }
        }
        let rule = Rule {
            id: f[0].parse().ok()?,
            name: f[1].to_string(),
            dir: match f[2] {
                "in" => Dir::In,
                "out" => Dir::Out,
                _ => return None,
            },
            verdict: Verdict::parse(f[3])?,
            proto: Proto::parse(f[4])?,
            ports: f[5].trim().to_string(),
            program: f[6].trim().to_string(),
            profiles: if profiles == 0 { 7 } else { profiles },
            enabled: f[8].trim() != "0",
        };
        parse_ports(&rule.ports)?;
        Some(rule)
    }

    pub fn to_line(&self) -> String {
        let profiles: Vec<&str> = (0..3)
            .filter(|i| self.profiles & (1 << i) != 0)
            .map(|i| PROFILES[i])
            .collect();
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.id,
            clean(&self.name),
            if self.dir == Dir::In { "in" } else { "out" },
            self.verdict.word(),
            self.proto.word(),
            clean(&self.ports),
            clean(&self.program),
            profiles.join(","),
            self.enabled as u8
        )
    }

    /// Programme sous forme de chemin Linux comparable à `/proc`.
    pub fn program_path(&self) -> Option<PathBuf> {
        let p = self.program.trim().trim_matches('"');
        if p.is_empty() {
            None
        } else if p.contains('\\') || p.as_bytes().get(1) == Some(&b':') {
            Some(crate::paths::to_linux(p))
        } else {
            Some(PathBuf::from(p))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: [bool; 3],
    pub inbound: [Verdict; 3],
    pub outbound: [Verdict; 3],
    /// Profil du réseau actuel (index dans [`PROFILES`]).
    pub profile: usize,
    /// Alerter quand une application écoute pour la première fois.
    pub notify: bool,
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            enabled: [true; 3],
            inbound: [Verdict::Block; 3],
            outbound: [Verdict::Allow; 3],
            profile: 1,
            notify: true,
            rules: Vec::new(),
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim_end_matches('\r');
            let profile = |k: &str| {
                k.split_once('.')
                    .and_then(|(_, p)| PROFILES.iter().position(|&x| x == p))
            };
            match key.trim() {
                "profile" => {
                    if let Some(i) = PROFILES.iter().position(|&x| x == value.trim()) {
                        c.profile = i;
                    }
                }
                "notify" => c.notify = value.trim() != "0",
                "rule" => c.rules.extend(Rule::parse(value)),
                k if k.starts_with("enabled.") => {
                    if let Some(i) = profile(k) {
                        c.enabled[i] = value.trim() != "0";
                    }
                }
                k if k.starts_with("inbound.") => {
                    if let (Some(i), Some(v)) = (profile(k), Verdict::parse(value.trim())) {
                        c.inbound[i] = v;
                    }
                }
                k if k.starts_with("outbound.") => {
                    if let (Some(i), Some(v)) = (profile(k), Verdict::parse(value.trim())) {
                        c.outbound[i] = v;
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        let mut s = format!(
            "profile={}\nnotify={}\n",
            PROFILES[self.profile], self.notify as u8
        );
        for (i, p) in PROFILES.iter().enumerate() {
            s += &format!(
                "enabled.{p}={}\ninbound.{p}={}\noutbound.{p}={}\n",
                self.enabled[i] as u8,
                self.inbound[i].word(),
                self.outbound[i].word()
            );
        }
        for r in &self.rules {
            s += &format!("rule={}\n", r.to_line());
        }
        s
    }

    pub fn load() -> Config {
        admin::read_text(&admin::path(CONFIG))
            .map(|t| Config::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        admin::write_text(&admin::path(CONFIG), &self.to_text())
    }

    pub fn next_id(&self) -> u32 {
        self.rules.iter().map(|r| r.id).max().unwrap_or(0) + 1
    }

    fn active_rules(&self) -> impl Iterator<Item = &Rule> {
        let bit = 1u8 << self.profile;
        self.rules
            .iter()
            .filter(move |r| r.enabled && r.profiles & bit != 0)
    }
}

// ---------- Sockets en écoute ----------

/// Port en écoute d'un programme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listen {
    pub udp: bool,
    pub port: u16,
}

/// Sockets en écoute non limitées à la boucle locale : inode → port.
pub fn parse_proc_net(text: &str, udp: bool) -> Vec<(u64, Listen)> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let Some((addr, port)) = f[1].split_once(':') else {
            continue;
        };
        let state = f[3];
        let listening = if udp {
            state == "07" && f[2].ends_with(":0000")
        } else {
            state == "0A"
        };
        let loopback = addr == "0100007F" || addr == "00000000000000000000000001000000";
        let Ok(port) = u16::from_str_radix(port, 16) else {
            continue;
        };
        if listening && !loopback && port != 0 {
            if let Ok(inode) = f[9].parse::<u64>() {
                out.push((inode, Listen { udp, port }));
            }
        }
    }
    out
}

/// Programme d'un processus : pour une application Windows lancée par le
/// runtime, le `.exe` (premier argument) plutôt que `winlux` lui-même.
pub fn program_of(pid_dir: &Path) -> Option<PathBuf> {
    let exe = std::fs::read_link(pid_dir.join("exe")).ok()?;
    if exe.file_name().is_some_and(|n| n == "winlux") {
        let cmdline = std::fs::read(pid_dir.join("cmdline")).ok()?;
        let arg = cmdline.split(|&b| b == 0).nth(1)?;
        let arg = String::from_utf8_lossy(arg).into_owned();
        if !arg.is_empty() && !arg.starts_with("--") {
            return Some(PathBuf::from(arg));
        }
    }
    Some(exe)
}

/// Programmes en écoute et leurs ports.
pub fn listening_programs() -> BTreeMap<PathBuf, BTreeSet<Listen>> {
    let mut by_inode = BTreeMap::new();
    for (file, udp) in [
        ("tcp", false),
        ("tcp6", false),
        ("udp", true),
        ("udp6", true),
    ] {
        if let Some(text) = admin::read_text(&admin::path(&format!("proc/net/{file}"))) {
            by_inode.extend(parse_proc_net(&text, udp));
        }
    }
    let mut out: BTreeMap<PathBuf, BTreeSet<Listen>> = BTreeMap::new();
    if by_inode.is_empty() {
        return out;
    }
    let Ok(procs) = std::fs::read_dir(admin::path("proc")) else {
        return out;
    };
    for p in procs.flatten() {
        if !p
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
            continue;
        };
        let mut found = Vec::new();
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            let t = target.to_string_lossy();
            if let Some(inode) = t
                .strip_prefix("socket:[")
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|n| n.parse::<u64>().ok())
            {
                if let Some(l) = by_inode.get(&inode) {
                    found.push(*l);
                }
            }
        }
        if !found.is_empty() {
            if let Some(prog) = program_of(&p.path()) {
                out.entry(prog).or_default().extend(found);
            }
        }
    }
    out
}

/// Les services système ne déclenchent pas d'alerte (comme sous Windows,
/// seules les applications de l'utilisateur sont concernées).
fn is_system_program(p: &Path) -> bool {
    [
        "/usr/sbin/",
        "/sbin/",
        "/usr/lib/",
        "/lib/",
        "/usr/libexec/",
    ]
    .iter()
    .any(|d| p.starts_with(d))
}

// ---------- Ruleset nftables ----------

fn proto_match(proto: Proto, ports: &[(u16, u16)], dport: bool) -> Option<String> {
    let key = if dport { "dport" } else { "sport" };
    Some(match (proto, ports.is_empty()) {
        (Proto::Icmp, _) => "meta l4proto { icmp, ipv6-icmp }".into(),
        (Proto::Any, true) => String::new(),
        (Proto::Any, false) => format!("meta l4proto {{ tcp, udp }} th {key} {}", port_set(ports)),
        (Proto::Tcp, true) => "meta l4proto tcp".into(),
        (Proto::Udp, true) => "meta l4proto udp".into(),
        (Proto::Tcp, false) => format!("tcp {key} {}", port_set(ports)),
        (Proto::Udp, false) => format!("udp {key} {}", port_set(ports)),
    })
}

fn verdict_lines(out: &mut Vec<String>, matcher: &str, verdict: Verdict, prefix: &str) {
    let m = if matcher.is_empty() {
        String::new()
    } else {
        format!("{matcher} ")
    };
    match verdict {
        Verdict::Allow => out.push(format!("{m}accept")),
        Verdict::Block => {
            out.push(format!(
                "{m}limit rate 10/second burst 20 packets log prefix \"{prefix}\" level info"
            ));
            out.push(format!("{m}drop"));
        }
    }
}

/// Chemin du cgroup des processus d'une règle sortante par application.
pub fn cgroup_of(rule: &Rule) -> String {
    format!("winlux-fw/r{}", rule.id)
}

/// Ruleset complet pour le profil actif, ou `None` si le pare-feu est
/// désactivé pour ce profil.
pub fn ruleset(
    cfg: &Config,
    apps: &BTreeMap<PathBuf, BTreeSet<Listen>>,
    cgroups: bool,
) -> Option<String> {
    let p = cfg.profile;
    if !cfg.enabled[p] {
        return None;
    }
    let mut input = vec![
        "ct state established,related accept".to_string(),
        "ct state invalid drop".into(),
        "iif \"lo\" accept".into(),
        "icmp type { destination-unreachable, time-exceeded, parameter-problem } accept".into(),
        "icmpv6 type { destination-unreachable, packet-too-big, time-exceeded, parameter-problem, nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert } accept".into(),
        "udp sport 67 udp dport 68 accept".into(),
        "udp sport 547 udp dport 546 accept".into(),
    ];
    let mut output = vec![
        "ct state established,related accept".to_string(),
        "oif \"lo\" accept".into(),
    ];
    if cfg.outbound[p] == Verdict::Block {
        output.push("udp dport { 67, 547 } accept".into());
        output.push(
            "icmpv6 type { nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert } accept"
                .into(),
        );
    }
    // Les blocages priment, comme sous Windows.
    for pass in [Verdict::Block, Verdict::Allow] {
        for r in cfg.active_rules().filter(|r| r.verdict == pass) {
            let ports = parse_ports(&r.ports).unwrap_or_default();
            let chain = if r.dir == Dir::In {
                &mut input
            } else {
                &mut output
            };
            let prefix = if r.dir == Dir::In {
                "winlux-fw-in: "
            } else {
                "winlux-fw-out: "
            };
            match (r.program_path(), r.dir) {
                (None, _) => {
                    if let Some(m) = proto_match(r.proto, &ports, true) {
                        verdict_lines(chain, &m, r.verdict, prefix);
                    }
                }
                (Some(prog), Dir::In) => {
                    // Ports où l'application écoute, filtrés par la règle.
                    for l in apps.get(&prog).into_iter().flatten() {
                        let proto_ok = match r.proto {
                            Proto::Any => true,
                            Proto::Tcp => !l.udp,
                            Proto::Udp => l.udp,
                            Proto::Icmp => false,
                        };
                        let port_ok = ports.is_empty()
                            || ports.iter().any(|&(a, b)| (a..=b).contains(&l.port));
                        if proto_ok && port_ok {
                            let m =
                                format!("{} dport {}", if l.udp { "udp" } else { "tcp" }, l.port);
                            verdict_lines(chain, &m, r.verdict, prefix);
                        }
                    }
                }
                (Some(_), Dir::Out) => {
                    if !cgroups {
                        continue;
                    }
                    let base = proto_match(r.proto, &ports, true).unwrap_or_default();
                    let m = format!("socket cgroupv2 level 2 \"{}\" {base}", cgroup_of(r));
                    verdict_lines(chain, m.trim_end(), r.verdict, prefix);
                }
            }
        }
    }
    let policy_in = if cfg.inbound[p] == Verdict::Block {
        input.push(
            "limit rate 10/second burst 20 packets log prefix \"winlux-fw-in: \" level info".into(),
        );
        "drop"
    } else {
        "accept"
    };
    let policy_out = if cfg.outbound[p] == Verdict::Block {
        output.push(
            "limit rate 10/second burst 20 packets log prefix \"winlux-fw-out: \" level info"
                .into(),
        );
        "drop"
    } else {
        "accept"
    };
    let body = |lines: &[String]| {
        lines
            .iter()
            .map(|l| format!("    {l}\n"))
            .collect::<String>()
    };
    Some(format!(
        "table inet {TABLE} {{\n  chain input {{\n    type filter hook input priority filter; policy {policy_in};\n{}  }}\n  chain output {{\n    type filter hook output priority filter; policy {policy_out};\n{}  }}\n}}\n",
        body(&input),
        body(&output)
    ))
}

fn nft_program() -> String {
    std::env::var("WINLUX_NFT").unwrap_or_else(|_| "nft".into())
}

fn cgroup_root() -> PathBuf {
    admin::path("sys/fs/cgroup")
}

fn cgroups_available() -> bool {
    cgroup_root().join("cgroup.controllers").exists()
}

fn write_status(active: bool, error: &str, rules: usize) {
    let backend = if error.starts_with("nft absent") {
        "absent"
    } else {
        "nftables"
    };
    let _ = admin::write_text(
        &admin::path(RUN_DIR).join("status.txt"),
        &format!(
            "backend={backend}\nactive={}\nrules={rules}\nerror={}\napplied={}\n",
            active as u8,
            clean(error.lines().next().unwrap_or("")),
            admin::format_time(admin::now())
        ),
    );
}

/// Applique la configuration ; renvoie une erreur lisible en cas d'échec.
pub fn apply(cfg: &Config, apps: &BTreeMap<PathBuf, BTreeSet<Listen>>) -> Result<(), String> {
    use std::io::Write;
    let mut cgroups = cgroups_available();
    if cgroups {
        // Sans ses cgroups, nft refuserait tout le ruleset : on omet alors
        // les règles sortantes par application plutôt que de tout perdre.
        for r in cfg
            .active_rules()
            .filter(|r| r.dir == Dir::Out && !r.program.is_empty())
        {
            cgroups &= std::fs::create_dir_all(cgroup_root().join(cgroup_of(r))).is_ok();
        }
    }
    let rules = ruleset(cfg, apps, cgroups);
    let mut script = format!("table inet {TABLE}\ndelete table inet {TABLE}\n");
    if let Some(r) = &rules {
        script += r;
    }
    let child = std::process::Command::new(nft_program())
        .args(["-f", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("nft absent : {e}");
            write_status(false, &msg, 0);
            return Err(msg);
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(script.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let count = cfg.active_rules().count();
    if out.status.success() {
        write_status(rules.is_some(), "", count);
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        write_status(false, &msg, count);
        Err(msg)
    }
}

/// Rattache les processus des applications filtrées en sortie à leur cgroup.
fn assign_cgroups(cfg: &Config) {
    let targets: Vec<(PathBuf, String)> = cfg
        .active_rules()
        .filter(|r| r.dir == Dir::Out)
        .filter_map(|r| r.program_path().map(|p| (p, cgroup_of(r))))
        .collect();
    if targets.is_empty() || !cgroups_available() {
        return;
    }
    let Ok(procs) = std::fs::read_dir(admin::path("proc")) else {
        return;
    };
    for p in procs.flatten() {
        let name = p.file_name().to_string_lossy().into_owned();
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Some(prog) = program_of(&p.path()) else {
            continue;
        };
        if let Some((_, group)) = targets.iter().find(|(t, _)| *t == prog) {
            let current = admin::read_text(&p.path().join("cgroup")).unwrap_or_default();
            if !current.trim_end().ends_with(&format!("/{group}")) {
                let _ = std::fs::write(cgroup_root().join(group).join("cgroup.procs"), &name);
            }
        }
    }
}

// ---------- Journal des connexions bloquées ----------

/// Ligne de journal à partir d'un message noyau `winlux-fw-…`.
pub fn parse_kernel_log(record: &str, epoch: i64) -> Option<String> {
    let msg = record.split_once(';').map_or(record, |(_, m)| m);
    let (dir, rest) = match msg.split_once("winlux-fw-in: ") {
        Some((_, r)) => ("in", r),
        None => ("out", msg.split_once("winlux-fw-out: ")?.1),
    };
    let get = |k: &str| {
        rest.split_whitespace()
            .find_map(|w| w.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
            .unwrap_or("")
            .to_string()
    };
    Some(format!(
        "{epoch}\t{}\t{dir}\t{}\t{}\t{}\t{}\t{}",
        admin::format_time(epoch),
        get("PROTO"),
        get("SRC"),
        get("SPT"),
        get("DST"),
        get("DPT")
    ))
}

fn log_blocked(line: &str) {
    admin::append_line(&admin::path(STATE_DIR).join("blocked.log"), line, LOG_MAX);
}

struct Kmsg(Option<std::fs::File>);

impl Kmsg {
    fn open() -> Kmsg {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;
        let f = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(admin::path("dev/kmsg"))
            .ok();
        if let Some(f) = &f {
            // SAFETY: descripteur valide détenu par `f`.
            unsafe { libc::lseek(f.as_raw_fd(), 0, libc::SEEK_END) };
        }
        Kmsg(f)
    }

    fn drain(&mut self) {
        use std::io::Read;
        let Some(f) = self.0.as_mut() else {
            return;
        };
        let mut buf = [0u8; 8192];
        for _ in 0..256 {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let rec = String::from_utf8_lossy(&buf[..n]);
                    if let Some(line) = parse_kernel_log(&rec, admin::now()) {
                        log_blocked(&line);
                    }
                }
                Err(e) if e.raw_os_error() == Some(libc::EPIPE) => continue,
                Err(_) => break,
            }
        }
    }
}

// ---------- Alerte de première écoute ----------

fn known_file() -> PathBuf {
    admin::path(STATE_DIR).join("known-apps")
}

fn prompt_file() -> PathBuf {
    admin::path(RUN_DIR).join("prompt.txt")
}

/// Programmes à signaler : en écoute, sans règle entrante, jamais signalés.
pub fn new_listeners(
    cfg: &Config,
    apps: &BTreeMap<PathBuf, BTreeSet<Listen>>,
    known: &BTreeSet<String>,
) -> Vec<PathBuf> {
    apps.keys()
        .filter(|p| !is_system_program(p))
        .filter(|p| !known.contains(&p.to_string_lossy().into_owned()))
        .filter(|p| {
            !cfg.rules
                .iter()
                .any(|r| r.dir == Dir::In && r.program_path().as_deref() == Some(p.as_path()))
        })
        .cloned()
        .collect()
}

fn load_known() -> BTreeSet<String> {
    admin::read_text(&known_file())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect()
}

fn write_prompt(prog: &Path, ports: &BTreeSet<Listen>, cfg: &Config) {
    let list: Vec<String> = ports
        .iter()
        .map(|l| format!("{} {}", if l.udp { "UDP" } else { "TCP" }, l.port))
        .collect();
    let name = prog
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = admin::write_text(
        &prompt_file(),
        &format!(
            "program={}\nname={name}\nports={}\nprofile={}\n",
            prog.display(),
            list.join(", "),
            PROFILES[cfg.profile]
        ),
    );
    admin::append_line(&known_file(), &prog.to_string_lossy(), 64 * 1024);
}

/// Notification pour la session native (le bureau Winlux lit l'alerte
/// lui-même) ; sans serveur de notifications, rien ne s'affiche.
fn notify_desktop(prog: &Path) {
    let name = prog
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Ok(mut child) = std::process::Command::new("notify-send")
        .args([
            "Pare-feu Winlux",
            &format!("{name} souhaite recevoir des connexions entrantes. Ouvrez le Pare-feu pour l'autoriser ou la bloquer."),
        ])
        .env("DISPLAY", std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Réponse de l'utilisateur à l'alerte : crée la règle entrante.
pub fn answer(allow: bool) -> Result<(), String> {
    let text = admin::read_text(&prompt_file()).ok_or("aucune alerte en attente")?;
    let program = admin::field(&text, "program").unwrap_or("").to_string();
    let name = admin::field(&text, "name").unwrap_or("").to_string();
    if program.is_empty() {
        return Err("alerte invalide".into());
    }
    let mut cfg = Config::load();
    cfg.rules
        .retain(|r| !(r.dir == Dir::In && r.program == program));
    cfg.rules.push(Rule {
        id: cfg.next_id(),
        name,
        dir: Dir::In,
        verdict: if allow {
            Verdict::Allow
        } else {
            Verdict::Block
        },
        proto: Proto::Any,
        ports: String::new(),
        program,
        profiles: 1 << cfg.profile,
        enabled: true,
    });
    cfg.save().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(prompt_file());
    apply(&cfg, &listening_programs())
}

// ---------- Service ----------

fn config_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(admin::path(CONFIG))
        .and_then(|m| m.modified())
        .ok()
}

pub fn daemon() -> ! {
    let _ = std::fs::create_dir_all(admin::path(RUN_DIR));
    let _ = admin::write_text(
        &admin::path(RUN_DIR).join("daemon.pid"),
        &std::process::id().to_string(),
    );
    let mut kmsg = Kmsg::open();
    let mut cfg = Config::load();
    let mut stamp = config_stamp();
    let mut apps = listening_programs();
    if let Err(e) = apply(&cfg, &apps) {
        eprintln!("winlux-firewall : {e}");
    }
    let mut tick = 0u64;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        tick += 1;
        kmsg.drain();
        if !tick.is_multiple_of(2) {
            continue;
        }
        let new_stamp = config_stamp();
        let new_apps = listening_programs();
        let reload = new_stamp != stamp;
        if reload {
            cfg = Config::load();
            stamp = new_stamp;
        }
        let program_rules = cfg
            .rules
            .iter()
            .any(|r| r.dir == Dir::In && !r.program.is_empty());
        if reload || (program_rules && new_apps != apps) {
            let _ = apply(&cfg, &new_apps);
        }
        assign_cgroups(&cfg);
        write_ports();
        if cfg.notify && cfg.enabled[cfg.profile] && !prompt_file().exists() {
            let known = load_known();
            if let Some(prog) = new_listeners(&cfg, &new_apps, &known).into_iter().next() {
                write_prompt(&prog, &new_apps[&prog], &cfg);
                notify_desktop(&prog);
            }
        }
        apps = new_apps;
    }
}

/// Page « Ports ouverts » : tous les ports en écoute, y compris ceux
/// réservés à la machine, avec le nom de l'appli (Windows ou Linux).
fn write_ports() {
    let text: String = crate::netports::open_ports()
        .iter()
        .map(|p| crate::netports::port_line(p) + "\n")
        .collect();
    let path = admin::path(RUN_DIR).join("ports.txt");
    if admin::read_text(&path).as_deref() != Some(text.as_str()) {
        let _ = admin::write_text(&path, &text);
    }
}

fn usage() -> i32 {
    eprintln!("usage : winlux --firewall daemon | apply | status | listening | ports | answer allow|block | clear-log");
    2
}

/// Point d'entrée de `winlux --firewall …`.
pub fn cli(args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("") {
        "daemon" => daemon(),
        "apply" => match apply(&Config::load(), &listening_programs()) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        "status" => {
            let cfg = Config::load();
            println!("profil={}", PROFILES[cfg.profile]);
            if let Some(s) = admin::read_text(&admin::path(RUN_DIR).join("status.txt")) {
                print!("{s}");
            }
            match ruleset(&cfg, &listening_programs(), cgroups_available()) {
                Some(r) => print!("{r}"),
                None => println!("pare-feu désactivé pour ce profil"),
            }
            0
        }
        "ports" => {
            for p in crate::netports::open_ports() {
                println!("{}", crate::netports::port_line(&p));
            }
            0
        }
        "listening" => {
            for (prog, ports) in listening_programs() {
                let list: Vec<String> = ports
                    .iter()
                    .map(|l| format!("{}/{}", l.port, if l.udp { "udp" } else { "tcp" }))
                    .collect();
                println!("{}\t{}", prog.display(), list.join(" "));
            }
            0
        }
        "answer" => match answer(args.get(1).map(String::as_str) == Some("allow")) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        "clear-log" => {
            let _ = std::fs::write(admin::path(STATE_DIR).join("blocked.log"), b"");
            0
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::test_root::Root;

    fn rule(id: u32, dir: Dir, verdict: Verdict, proto: Proto, ports: &str, program: &str) -> Rule {
        Rule {
            id,
            name: format!("Règle {id}"),
            dir,
            verdict,
            proto,
            ports: ports.into(),
            program: program.into(),
            profiles: 7,
            enabled: true,
        }
    }

    #[test]
    fn ports_are_validated() {
        assert_eq!(
            parse_ports("80, 443,8000-8100"),
            Some(vec![(80, 80), (443, 443), (8000, 8100)])
        );
        assert_eq!(parse_ports(""), Some(vec![]));
        assert!(parse_ports("0").is_none());
        assert!(parse_ports("90-80").is_none());
        assert!(parse_ports("http").is_none());
        assert!(parse_ports("70000").is_none());
    }

    #[test]
    fn config_round_trip() {
        let mut cfg = Config {
            profile: 2,
            ..Config::default()
        };
        cfg.enabled[0] = false;
        cfg.outbound[2] = Verdict::Block;
        cfg.rules
            .push(rule(1, Dir::In, Verdict::Allow, Proto::Tcp, "22", ""));
        let mut r = rule(
            2,
            Dir::Out,
            Verdict::Block,
            Proto::Any,
            "",
            "C:\\Jeux\\jeu.exe",
        );
        r.profiles = 0b110;
        r.enabled = false;
        cfg.rules.push(r);
        assert_eq!(Config::parse(&cfg.to_text()), cfg);
        assert_eq!(cfg.next_id(), 3);
        // Une règle aux ports invalides est ignorée plutôt que d'échouer.
        let bad = Config::parse("rule=1\tx\tin\tallow\ttcp\tabc\t\tany\t1\n");
        assert!(bad.rules.is_empty());
    }

    #[test]
    fn ruleset_orders_blocks_first_and_follows_profile() {
        let mut cfg = Config::default();
        cfg.rules
            .push(rule(1, Dir::In, Verdict::Allow, Proto::Tcp, "22,80-81", ""));
        cfg.rules
            .push(rule(2, Dir::In, Verdict::Block, Proto::Udp, "", ""));
        cfg.rules.push(rule(
            3,
            Dir::Out,
            Verdict::Block,
            Proto::Any,
            "",
            "/opt/app/bin",
        ));
        let mut public_only = rule(4, Dir::In, Verdict::Allow, Proto::Any, "9000", "");
        public_only.profiles = 0b100;
        cfg.rules.push(public_only);
        let rs = ruleset(&cfg, &BTreeMap::new(), true).unwrap();
        let allow = rs.find("tcp dport { 22, 80-81 } accept").unwrap();
        let block = rs.find("meta l4proto udp drop").unwrap();
        assert!(block < allow, "{rs}");
        assert!(rs.contains("policy drop;"));
        assert!(rs.contains("socket cgroupv2 level 2 \"winlux-fw/r3\" drop"));
        assert!(
            !rs.contains("9000"),
            "règle du profil public absente en privé"
        );
        assert!(rs.contains("log prefix \"winlux-fw-in: \""));
        cfg.profile = 2;
        assert!(ruleset(&cfg, &BTreeMap::new(), false)
            .unwrap()
            .contains("th dport { 9000 } accept"));
        cfg.enabled[2] = false;
        assert!(ruleset(&cfg, &BTreeMap::new(), false).is_none());
    }

    #[test]
    fn application_inbound_rules_follow_listening_ports() {
        let mut cfg = Config::default();
        cfg.rules.push(rule(
            1,
            Dir::In,
            Verdict::Allow,
            Proto::Any,
            "",
            "/home/u/serveur.exe",
        ));
        let mut apps = BTreeMap::new();
        apps.insert(
            PathBuf::from("/home/u/serveur.exe"),
            BTreeSet::from([
                Listen {
                    udp: false,
                    port: 8080,
                },
                Listen {
                    udp: true,
                    port: 5353,
                },
            ]),
        );
        let rs = ruleset(&cfg, &apps, false).unwrap();
        assert!(rs.contains("tcp dport 8080 accept"));
        assert!(rs.contains("udp dport 5353 accept"));
    }

    #[test]
    fn proc_net_and_kernel_log_parsing() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
           0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1\n\
           1: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4243 1\n\
           2: 0F02000A:D2C4 22D8B85D:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 4244 1\n";
        assert_eq!(
            parse_proc_net(tcp, false),
            vec![(
                4242,
                Listen {
                    udp: false,
                    port: 8080
                }
            )]
        );
        let rec = "4,1234,99999,-;winlux-fw-in: IN=eth0 OUT= MAC=52:54 SRC=10.0.2.2 DST=10.0.2.15 LEN=60 PROTO=TCP SPT=40000 DPT=22 WINDOW=64240 SYN";
        let line = parse_kernel_log(rec, 0).unwrap();
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            &f[2..],
            ["in", "TCP", "10.0.2.2", "40000", "10.0.2.15", "22"]
        );
        assert!(parse_kernel_log("6,1,2,-;eth0: link up", 0).is_none());
    }

    #[test]
    fn first_listen_prompt_and_answer_create_a_rule() {
        let r = Root::new("firewall");
        // Faux nft : enregistre le script reçu.
        let nft = r.dir.join("nft");
        std::fs::write(
            &nft,
            format!("#!/bin/sh\ncat > {}/nft.in\n", r.dir.display()),
        )
        .unwrap();
        std::fs::set_permissions(&nft, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        std::env::set_var("WINLUX_NFT", &nft);
        let cfg = Config::default();
        let prog = PathBuf::from("/home/u/chat.exe");
        let mut apps = BTreeMap::new();
        apps.insert(
            prog.clone(),
            BTreeSet::from([Listen {
                udp: false,
                port: 7000,
            }]),
        );
        apps.insert(
            PathBuf::from("/usr/sbin/sshd"),
            BTreeSet::from([Listen {
                udp: false,
                port: 22,
            }]),
        );
        let fresh = new_listeners(&cfg, &apps, &BTreeSet::new());
        assert_eq!(
            fresh,
            vec![prog.clone()],
            "les services système ne sont pas signalés"
        );
        write_prompt(&prog, &apps[&prog], &cfg);
        assert!(new_listeners(&cfg, &apps, &load_known()).is_empty());
        let prompt = admin::read_text(&prompt_file()).unwrap();
        assert!(prompt.contains("ports=TCP 7000"));
        answer(true).unwrap();
        std::env::remove_var("WINLUX_NFT");
        let saved = Config::load();
        assert_eq!(saved.rules.len(), 1);
        assert_eq!(saved.rules[0].verdict, Verdict::Allow);
        assert_eq!(saved.rules[0].profiles, 0b010);
        assert!(!prompt_file().exists());
        let script = std::fs::read_to_string(r.dir.join("nft.in")).unwrap();
        assert!(script.starts_with("table inet winlux_fw\ndelete table inet winlux_fw\n"));
        let status = admin::read_text(&r.dir.join(RUN_DIR).join("status.txt")).unwrap();
        assert!(status.contains("active=1"));
    }
}
