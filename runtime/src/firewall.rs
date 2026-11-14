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
//!   `/sys/fs/cgroup/winlux-fw/r<id>`, filtré par `socket cgroupv2`. Le
//!   noyau suspend l'`exec` d'un programme filtré (fanotify
//!   `FAN_OPEN_EXEC_PERM`) jusqu'à ce que le service l'ait rattaché : aucune
//!   socket n'est créée hors du filtre (#2157). Sans fanotify, le connecteur
//!   « proc » rattache juste après l'exec, sans garantie. Un .exe filtré
//!   attend en plus, au lancement, d'être rattaché avant d'exécuter son
//!   propre code. Le balayage toutes les deux secondes reste en secours.

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
    let controllers = cgroup_root().join("cgroup.controllers");
    if !cfg!(test) && !controllers.exists() && admin::root() == Path::new("/") {
        mount_cgroup2();
    }
    controllers.exists()
}

/// Monte cgroup v2 s'il manque (#2142) : un /init antérieur au correctif
/// laissait /sys/fs/cgroup vide, et le pare-feu, qui tourne en root,
/// répare lui-même plutôt que d'abandonner ses règles par application.
fn mount_cgroup2() {
    let mounted = admin::read_text(Path::new("/proc/mounts"))
        .unwrap_or_default()
        .lines()
        .any(|l| l.split(' ').nth(1) == Some("/sys/fs/cgroup"));
    if mounted || unsafe { libc::geteuid() } != 0 || !Path::new("/sys/fs/cgroup").is_dir() {
        return;
    }
    let source = c"cgroup2";
    let target = c"/sys/fs/cgroup";
    // SAFETY : chaînes C terminées par NUL, aucune donnée de montage.
    let rc = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            source.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    if rc != 0 {
        eprintln!(
            "winlux-firewall : montage de cgroup2 impossible : {}",
            std::io::Error::last_os_error()
        );
    }
}

fn write_status(cfg: &Config, active: bool, error: &str, rules: usize) {
    let backend = if error.starts_with("nft absent") {
        "absent"
    } else {
        "nftables"
    };
    let _ = admin::write_text(
        &admin::path(RUN_DIR).join("status.txt"),
        &format!(
            "backend={backend}\nprofile={}\nactive={}\nrules={rules}\nerror={}\napplied={}\n",
            PROFILES[cfg.profile],
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
            write_status(cfg, false, &msg, 0);
            return Err(msg);
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(script.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let count = cfg.active_rules().count();
    let dropped = !cgroups
        && cfg
            .active_rules()
            .any(|r| r.dir == Dir::Out && !r.program.is_empty());
    if out.status.success() {
        // Les règles sortantes par application sont omises sans cgroup v2 :
        // le dire dans l'état plutôt que d'afficher « actif » sans elles.
        let note = if dropped {
            "règles sortantes par application ignorées : cgroup v2 indisponible"
        } else {
            ""
        };
        write_status(cfg, rules.is_some(), note, count);
        publish_targets(cfg, cgroups && rules.is_some());
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        write_status(cfg, false, &msg, count);
        publish_targets(cfg, false);
        Err(msg)
    }
}

/// Programmes filtrés en sortie et leur cgroup.
fn out_targets(cfg: &Config) -> Vec<(PathBuf, String)> {
    cfg.active_rules()
        .filter(|r| r.dir == Dir::Out)
        .filter_map(|r| r.program_path().map(|p| (p, cgroup_of(r))))
        .collect()
}

/// Liste publiée des programmes filtrés en sortie (#2157) : un chemin par
/// ligne, rien d'autre (ni règle, ni profil, ni port, ni adresse). Un .exe y
/// lit seulement s'il doit attendre son rattachement, car firewall.conf
/// n'est lisible que par root (umask 077 de /init). Écrite par le service
/// root, 0644 dans un dossier root 0755 : aucun compte ne peut la modifier.
/// Vide quand le filtre n'est pas en place : personne n'attend alors.
pub const TARGETS: &str = "run/winlux/firewall/programmes.txt";

pub fn targets_text(targets: &[(PathBuf, String)]) -> String {
    let mut paths: Vec<String> = targets
        .iter()
        .map(|(p, _)| p.display().to_string())
        .filter(|p| !p.contains('\n'))
        .collect();
    paths.sort();
    paths.dedup();
    paths.into_iter().map(|p| p + "\n").collect()
}

pub fn parse_targets(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn publish_targets(cfg: &Config, filtered: bool) {
    use std::os::unix::fs::PermissionsExt;
    let text = if filtered {
        targets_text(&out_targets(cfg))
    } else {
        String::new()
    };
    let path = admin::path(TARGETS);
    if admin::read_text(&path).as_deref() == Some(text.as_str()) {
        return;
    }
    if admin::write_text(&path, &text).is_ok() {
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
    }
}

/// Place un processus dans le cgroup de sa règle s'il n'y est pas déjà.
fn assign_one(pid_dir: &Path, targets: &[(PathBuf, String)]) {
    let Some(prog) = program_of(pid_dir) else {
        return;
    };
    let Some((_, group)) = targets.iter().find(|(t, _)| *t == prog) else {
        return;
    };
    let current = admin::read_text(&pid_dir.join("cgroup")).unwrap_or_default();
    if !current.trim_end().ends_with(&format!("/{group}")) {
        if let Some(pid) = pid_dir.file_name() {
            let _ = std::fs::write(
                cgroup_root().join(group).join("cgroup.procs"),
                pid.to_string_lossy().as_bytes(),
            );
        }
    }
}

/// Rattache les processus des applications filtrées en sortie à leur cgroup.
fn assign_cgroups(cfg: &Config) {
    let targets = out_targets(cfg);
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
        assign_one(&p.path(), &targets);
    }
}

// ---------- Rattachement dès l'exec (#2157) ----------

const PROC_EVENT_EXEC: u32 = 0x0000_0002;

/// Processus (tgid) des événements `exec` d'un datagramme du connecteur
/// « proc » : nlmsghdr (16 octets), cn_msg (20), puis proc_event
/// (what, cpu, horodatage, puis pour exec : pid, tgid).
pub fn exec_tgids(buf: &[u8]) -> Vec<i32> {
    let u32_at = |b: &[u8], o: usize| {
        b.get(o..o + 4)
            .map(|s| u32::from_ne_bytes([s[0], s[1], s[2], s[3]]))
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(len) = u32_at(buf, at).map(|l| l as usize) {
        if len < 16 || at + len > buf.len() {
            break;
        }
        let msg = &buf[at..at + len];
        if u32_at(msg, 36) == Some(PROC_EVENT_EXEC) {
            if let Some(tgid) = u32_at(msg, 36 + 20) {
                out.push(tgid as i32);
            }
        }
        at += (len + 3) & !3;
    }
    out
}

/// Socket netlink abonnée aux événements de processus du noyau (root seul).
fn proc_connector() -> Option<i32> {
    const NETLINK_CONNECTOR: i32 = 11;
    const CN_IDX_PROC: u32 = 1;
    const CN_VAL_PROC: u32 = 1;
    const PROC_CN_MCAST_LISTEN: u32 = 1;
    // SAFETY : appels système sur un descripteur que l'on possède, tampons
    // locaux de la taille annoncée.
    unsafe {
        let fd = libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
            NETLINK_CONNECTOR,
        );
        if fd < 0 {
            return None;
        }
        let mut addr: libc::sockaddr_nl = std::mem::zeroed();
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = CN_IDX_PROC;
        let addr_len = std::mem::size_of::<libc::sockaddr_nl>() as u32;
        if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, addr_len) != 0 {
            libc::close(fd);
            return None;
        }
        let mut msg = Vec::with_capacity(40);
        msg.extend_from_slice(&40u32.to_ne_bytes()); // nlmsg_len
        msg.extend_from_slice(&(libc::NLMSG_DONE as u16).to_ne_bytes());
        msg.extend_from_slice(&0u16.to_ne_bytes()); // flags
        msg.extend_from_slice(&0u32.to_ne_bytes()); // seq
        msg.extend_from_slice(&libc::getpid().to_ne_bytes());
        msg.extend_from_slice(&CN_IDX_PROC.to_ne_bytes());
        msg.extend_from_slice(&CN_VAL_PROC.to_ne_bytes());
        msg.extend_from_slice(&0u32.to_ne_bytes()); // seq
        msg.extend_from_slice(&0u32.to_ne_bytes()); // ack
        msg.extend_from_slice(&4u16.to_ne_bytes()); // len
        msg.extend_from_slice(&0u16.to_ne_bytes()); // flags
        msg.extend_from_slice(&PROC_CN_MCAST_LISTEN.to_ne_bytes());
        if libc::send(fd, msg.as_ptr().cast(), msg.len(), 0) != msg.len() as isize {
            libc::close(fd);
            return None;
        }
        Some(fd)
    }
}

/// Règles publiées, relues seulement quand `programmes.txt` change
/// (configuration ou profil).
#[derive(Default)]
struct PublishedTargets {
    stamp: Option<std::time::SystemTime>,
    loaded: bool,
    targets: Vec<(PathBuf, String)>,
}

impl PublishedTargets {
    /// Relit les règles si la liste publiée a changé ; vrai si relu.
    fn refresh(&mut self) -> bool {
        let path = admin::path(TARGETS);
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if self.loaded && stamp == self.stamp {
            return false;
        }
        let published = !admin::read_text(&path).unwrap_or_default().is_empty();
        self.targets = if stamp.is_some() && published {
            out_targets(&Config::load())
        } else {
            Vec::new()
        };
        self.stamp = stamp;
        self.loaded = true;
        true
    }
}

/// Fil du service : rattache chaque programme filtré dès son `exec`. Avec
/// fanotify, l'exec attend le rattachement (garantie stricte) ; sinon, le
/// connecteur « proc » le fait juste après, sans garantie.
fn watch_execs() {
    match exec_guard() {
        Some(fd) => guard_execs(fd),
        None => {
            eprintln!(
                "winlux-firewall : fanotify indisponible ({}), rattachement après l'exec seulement",
                std::io::Error::last_os_error()
            );
            follow_execs();
        }
    }
}

/// Connecteur « proc » : rattachement juste après l'exec. Le programme tourne
/// déjà ; une socket ouverte avant le rattachement échappe à la règle.
fn follow_execs() {
    let Some(fd) = proc_connector() else {
        eprintln!(
            "winlux-firewall : connecteur « proc » indisponible ({}), rattachement par balayage seulement",
            std::io::Error::last_os_error()
        );
        return;
    };
    let mut buf = vec![0u8; 16 * 1024];
    let mut published = PublishedTargets::default();
    loop {
        // SAFETY : `buf` est un tampon local de la taille annoncée.
        let n = unsafe { libc::recv(fd, buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n <= 0 {
            // ENOBUFS : des événements ont été perdus, le balayage les rattrape.
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EBADF) {
                return;
            }
            continue;
        }
        let tgids = exec_tgids(&buf[..n as usize]);
        if tgids.is_empty() {
            continue;
        }
        published.refresh();
        if published.targets.is_empty() {
            continue;
        }
        for tgid in tgids {
            assign_one(&admin::path(&format!("proc/{tgid}")), &published.targets);
        }
    }
}

// ---------- Exec suspendu jusqu'au rattachement (#2157) ----------
//
// fanotify `FAN_OPEN_EXEC_PERM`, posé sur les seuls programmes filtrés : le
// noyau suspend l'`execve` du programme jusqu'à la réponse du service, qui
// place d'abord le processus dans le cgroup de sa règle. Le programme ne
// s'exécute donc jamais hors du filtre, et toute socket qu'il crée est
// filtrée. La réponse est toujours « autoriser » : le pare-feu ne bloque
// pas le lancement, seulement le réseau. Si le service s'arrête, le noyau
// autorise les exec en attente à la fermeture du descripteur.

/// Groupe fanotify de classe « contenu » (événements de permission).
fn exec_guard() -> Option<i32> {
    // SAFETY : appel système sans pointeur.
    let fd = unsafe {
        libc::fanotify_init(
            libc::FAN_CLASS_CONTENT | libc::FAN_CLOEXEC,
            (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_LARGEFILE) as u32,
        )
    };
    (fd >= 0).then_some(fd)
}

/// Identité d'un fichier (périphérique, inode), liens symboliques suivis
/// comme par `fanotify_mark`.
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// Pose les marques sur les programmes filtrés et rend leur identité avec
/// leur cgroup. Un programme remplacé (mise à jour) change d'inode : la
/// boucle compare les identités et repose les marques.
fn mark_targets(fd: i32, targets: &[(PathBuf, String)]) -> Vec<(Option<(u64, u64)>, String)> {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY : descripteur fanotify possédé ; chemins terminés par NUL.
    unsafe {
        libc::fanotify_mark(
            fd,
            libc::FAN_MARK_FLUSH,
            0,
            libc::AT_FDCWD,
            std::ptr::null(),
        );
    }
    let mut marked = Vec::new();
    for (path, group) in targets {
        let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            continue;
        };
        // SAFETY : voir plus haut.
        let rc = unsafe {
            libc::fanotify_mark(
                fd,
                libc::FAN_MARK_ADD,
                libc::FAN_OPEN_EXEC_PERM,
                libc::AT_FDCWD,
                c.as_ptr(),
            )
        };
        if rc == 0 {
            marked.push((file_identity(path), group.clone()));
        } else {
            // Programme absent pour l'instant : réessayé au prochain tour.
            marked.push((None, group.clone()));
        }
    }
    marked
}

/// Événement fanotify : (masque, descripteur du fichier, processus).
pub fn fanotify_events(buf: &[u8]) -> Vec<(u64, i32, i32)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 24 <= buf.len() {
        let m = &buf[at..];
        let len = u32::from_ne_bytes([m[0], m[1], m[2], m[3]]) as usize;
        if len < 24 || at + len > buf.len() || m[4] != libc::FANOTIFY_METADATA_VERSION {
            break;
        }
        let mut mask = [0u8; 8];
        mask.copy_from_slice(&m[8..16]);
        let fd = i32::from_ne_bytes([m[16], m[17], m[18], m[19]]);
        let pid = i32::from_ne_bytes([m[20], m[21], m[22], m[23]]);
        out.push((u64::from_ne_bytes(mask), fd, pid));
        at += len;
    }
    out
}

/// Répond à un événement de permission : toujours « autoriser ».
fn allow(group_fd: i32, event_fd: i32) {
    let response = libc::fanotify_response {
        fd: event_fd,
        response: libc::FAN_ALLOW,
    };
    // SAFETY : structure locale de la taille annoncée.
    unsafe {
        libc::write(
            group_fd,
            (&response as *const libc::fanotify_response).cast(),
            std::mem::size_of::<libc::fanotify_response>(),
        );
    }
}

fn guard_execs(fd: i32) {
    let mut published = PublishedTargets::default();
    let mut marked: Vec<(Option<(u64, u64)>, String)> = Vec::new();
    let mut buf = vec![0u8; 8 * 1024];
    loop {
        let stale = marked
            .iter()
            .zip(&published.targets)
            .any(|((id, _), (path, _))| *id != file_identity(path));
        if published.refresh() || stale {
            marked = mark_targets(fd, &published.targets);
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY : une seule structure locale.
        if unsafe { libc::poll(&mut poll, 1, 1000) } <= 0 {
            continue;
        }
        // SAFETY : `buf` est un tampon local de la taille annoncée.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EBADF) {
                return;
            }
            continue;
        }
        for (mask, event_fd, pid) in fanotify_events(&buf[..n as usize]) {
            if event_fd < 0 {
                continue;
            }
            if mask & libc::FAN_OPEN_EXEC_PERM != 0 {
                let id = std::fs::read_link(format!("/proc/self/fd/{event_fd}"))
                    .ok()
                    .and_then(|p| file_identity(&p));
                if let Some((_, group)) = marked.iter().find(|(m, _)| m.is_some() && *m == id) {
                    let _ = std::fs::write(
                        cgroup_root().join(group).join("cgroup.procs"),
                        pid.to_string(),
                    );
                }
                allow(fd, event_fd);
            }
            // SAFETY : descripteur reçu avec l'événement, fermé une fois.
            unsafe { libc::close(event_fd) };
        }
    }
}

/// Au lancement d'un .exe filtré en sortie : son propre code ne s'exécute
/// qu'une fois le processus dans le cgroup de sa règle, donc aucun octet ne
/// part avant le filtre (#2157). Rien à attendre pour un programme non visé.
pub fn wait_for_cgroup() {
    let Some(text) = admin::read_text(&admin::path(TARGETS)) else {
        return;
    };
    let targets = parse_targets(&text);
    if targets.is_empty() {
        return;
    }
    let me = admin::path("proc/self");
    let Some(prog) = program_of(&me) else {
        return;
    };
    if !targets.contains(&prog) {
        return;
    }
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(3) {
        let current = admin::read_text(&me.join("cgroup")).unwrap_or_default();
        if current.contains("/winlux-fw/r") {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    eprintln!(
        "winlux : {} n'a pas été rattaché au filtre du pare-feu en 3 s",
        prog.display()
    );
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

/// Port déjà ouvert à tous les programmes par une règle entrante active
/// (Téléphone connecté, partage de fichiers…) : rien à demander.
fn port_opened(cfg: &Config, l: &Listen) -> bool {
    cfg.rules.iter().any(|r| {
        r.enabled
            && r.dir == Dir::In
            && r.verdict == Verdict::Allow
            && r.program.is_empty()
            && match r.proto {
                Proto::Tcp => !l.udp,
                Proto::Udp => l.udp,
                Proto::Any => true,
                Proto::Icmp => false,
            }
            && parse_ports(&r.ports).is_some_and(|ports| {
                !ports.is_empty() && ports.iter().any(|&(a, b)| (a..=b).contains(&l.port))
            })
    })
}

/// Programmes à signaler : en écoute, sans règle entrante, jamais signalés.
/// Un programme dont tous les ports sont déjà ouverts par une règle
/// système (Téléphone connecté) n'est pas signalé non plus.
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
        .filter(|p| !apps[*p].iter().all(|l| port_opened(cfg, l)))
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
    {
        // Les .exe lisent la liste des programmes filtrés (#2157).
        use std::os::unix::fs::PermissionsExt;
        let _ =
            std::fs::set_permissions(admin::path(RUN_DIR), std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::thread::Builder::new()
        .name("winlux-fw-exec".into())
        .spawn(watch_execs);
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
    let mut published = None;
    publish_profile(&cfg, &mut published);
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
        publish_profile(&cfg, &mut published);
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

/// Le profil vient de changer (Paramètres, `winlux-control` ou
/// `firewall.conf` modifié à la main) : `/run/winlux/shares.txt` est
/// republié pour que tout ce qui le lit voie le profil réellement appliqué.
fn publish_profile(cfg: &Config, published: &mut Option<usize>) {
    if *published == Some(cfg.profile) {
        return;
    }
    *published = Some(cfg.profile);
    let share = std::env::var("WINLUX_SHARE").unwrap_or_else(|_| "/usr/bin/winlux-share".into());
    if std::path::Path::new(&share).exists() {
        let _ = std::process::Command::new(share)
            .arg("status")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
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

/// Sortie d'une sous-commande : un lecteur qui ferme le tube (`| head`)
/// arrête la commande proprement, sans la panique de `println!` (#1105).
fn print_out(text: &str) -> i32 {
    write_out(&mut std::io::stdout().lock(), text)
}

fn write_out(out: &mut impl std::io::Write, text: &str) -> i32 {
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
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
            let mut text = format!("profil={}\n", PROFILES[cfg.profile]);
            if let Some(s) = admin::read_text(&admin::path(RUN_DIR).join("status.txt")) {
                text.push_str(&s);
            }
            match ruleset(&cfg, &listening_programs(), cgroups_available()) {
                Some(r) => text.push_str(&r),
                None => text.push_str("pare-feu désactivé pour ce profil\n"),
            }
            print_out(&text)
        }
        "ports" => {
            let mut text = String::new();
            for p in crate::netports::open_ports() {
                text.push_str(&crate::netports::port_line(&p));
                text.push('\n');
            }
            print_out(&text)
        }
        "listening" => {
            let mut text = String::new();
            for (prog, ports) in listening_programs() {
                let list: Vec<String> = ports
                    .iter()
                    .map(|l| format!("{}/{}", l.port, if l.udp { "udp" } else { "tcp" }))
                    .collect();
                text.push_str(&format!("{}\t{}\n", prog.display(), list.join(" ")));
            }
            print_out(&text)
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
    /// Datagramme fanotify : deux événements à la suite, version vérifiée.
    #[test]
    fn analyse_des_evenements_fanotify() {
        let event = |mask: u64, fd: i32, pid: i32| {
            let mut e = Vec::new();
            e.extend_from_slice(&24u32.to_ne_bytes());
            e.push(libc::FANOTIFY_METADATA_VERSION);
            e.push(0);
            e.extend_from_slice(&24u16.to_ne_bytes());
            e.extend_from_slice(&mask.to_ne_bytes());
            e.extend_from_slice(&fd.to_ne_bytes());
            e.extend_from_slice(&pid.to_ne_bytes());
            e
        };
        let mut buf = event(libc::FAN_OPEN_EXEC_PERM, 7, 4242);
        buf.extend(event(libc::FAN_OPEN_EXEC_PERM, 8, 4343));
        assert_eq!(
            super::fanotify_events(&buf),
            vec![
                (libc::FAN_OPEN_EXEC_PERM, 7, 4242),
                (libc::FAN_OPEN_EXEC_PERM, 8, 4343)
            ]
        );
        let mut wrong = event(libc::FAN_OPEN_EXEC_PERM, 7, 1);
        wrong[4] = 99;
        assert!(super::fanotify_events(&wrong).is_empty());
        assert!(super::fanotify_events(&buf[..20]).is_empty());
    }

    /// Réel (root, fanotify) : l'exec d'un programme marqué reste suspendu
    /// jusqu'à la réponse, et l'événement donne le processus qui démarre.
    /// `cargo test --release --bin winlux exec_suspendu -- --ignored`
    #[test]
    #[ignore]
    fn exec_suspendu_jusqu_a_la_reponse() {
        let dir = std::env::temp_dir().join(format!("winlux-fan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prog = dir.join("vrai");
        std::fs::copy("/bin/true", &prog).unwrap();
        let fd = super::exec_guard().expect("fanotify_init");
        let marked = super::mark_targets(fd, &[(prog.clone(), "winlux-fw/r1".into())]);
        assert!(marked[0].0.is_some(), "marque posée");
        let p = prog.clone();
        let child = std::thread::spawn(move || {
            let mut c = std::process::Command::new(&p).spawn().unwrap();
            let id = c.id() as i32;
            c.wait().unwrap();
            id
        });
        let mut buf = vec![0u8; 4096];
        // SAFETY : tampon local de la taille annoncée.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        let events = super::fanotify_events(&buf[..n as usize]);
        assert_eq!(events.len(), 1);
        let (mask, event_fd, pid) = events[0];
        assert!(mask & libc::FAN_OPEN_EXEC_PERM != 0);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!child.is_finished(), "l'exec attend la réponse");
        let id = std::fs::read_link(format!("/proc/self/fd/{event_fd}"))
            .ok()
            .and_then(|p| super::file_identity(&p));
        assert_eq!(
            id, marked[0].0,
            "le fichier de l'événement est le programme marqué"
        );
        super::allow(fd, event_fd);
        // SAFETY : descripteurs possédés.
        unsafe {
            libc::close(event_fd);
            libc::close(fd);
        }
        assert_eq!(
            child.join().unwrap(),
            pid,
            "processus de l'événement = programme lancé"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use crate::admin::test_root::Root;

    /// Lecteur qui a fermé le tube, comme `| head` (#1105).
    struct ClosedPipe;
    impl std::io::Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn closed_pipe_ends_output_quietly() {
        assert_eq!(write_out(&mut ClosedPipe, "profil=public\n"), 0);
        let mut text = Vec::new();
        assert_eq!(write_out(&mut text, "profil=private\n"), 0);
        assert_eq!(text, b"profil=private\n");
    }

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
        // Téléphone connecté : python3 écoute sur des ports déjà ouverts par
        // la règle « Téléphone connecté » (réseaux privés), pas d'alerte.
        let mut phone_cfg = Config::default();
        phone_cfg.rules.push(Rule::parse("9447\tTelephone connecte Winlux (TCP)\tin\tallow\ttcp\t1716,1739-1764\t\tprivate,domain\t1").unwrap());
        phone_cfg.rules.push(
            Rule::parse(
                "9448\tTelephone connecte Winlux (UDP)\tin\tallow\tudp\t1716\t\tprivate,domain\t1",
            )
            .unwrap(),
        );
        let python = PathBuf::from("/usr/bin/python3.13");
        let mut phone_apps = apps.clone();
        phone_apps.insert(
            python.clone(),
            BTreeSet::from([
                Listen {
                    udp: false,
                    port: 1716,
                },
                Listen {
                    udp: true,
                    port: 1716,
                },
                Listen {
                    udp: false,
                    port: 1740,
                },
            ]),
        );
        assert_eq!(
            new_listeners(&phone_cfg, &phone_apps, &BTreeSet::new()),
            vec![prog.clone()]
        );
        phone_apps.get_mut(&python).unwrap().insert(Listen {
            udp: false,
            port: 8080,
        });
        assert!(new_listeners(&phone_cfg, &phone_apps, &BTreeSet::new()).contains(&python));
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

    #[test]
    fn profile_change_republishes_shares() {
        use std::os::unix::fs::PermissionsExt;
        let r = Root::new("firewall-profile");
        let calls = r.dir.join("calls.txt");
        let tool = r.dir.join("winlux-share");
        std::fs::write(
            &tool,
            format!("#!/bin/sh\necho \"$1\" >> '{}'\n", calls.display()),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("WINLUX_SHARE", &tool);
        let mut cfg = Config::default();
        let mut published = None;
        publish_profile(&cfg, &mut published);
        publish_profile(&cfg, &mut published);
        cfg.profile = 2;
        publish_profile(&cfg, &mut published);
        std::env::remove_var("WINLUX_SHARE");
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            "status\nstatus\n",
            "un seul relevé par changement de profil"
        );
        write_status(&cfg, true, "", 0);
        let status = admin::read_text(&r.dir.join(RUN_DIR).join("status.txt")).unwrap();
        assert!(status.contains("profile=public\n"));
    }

    /// Datagramme du connecteur « proc » : un fork ignoré, puis un exec (#2157).
    #[test]
    fn exec_events_give_the_new_program_tgid() {
        fn event(what: u32, pid: i32, tgid: i32) -> Vec<u8> {
            let mut m = Vec::new();
            m.extend_from_slice(&60u32.to_ne_bytes());
            m.extend_from_slice(&[0u8; 12]);
            m.extend_from_slice(&[0u8; 16]);
            m.extend_from_slice(&24u16.to_ne_bytes());
            m.extend_from_slice(&0u16.to_ne_bytes());
            m.extend_from_slice(&what.to_ne_bytes());
            m.extend_from_slice(&0u32.to_ne_bytes());
            m.extend_from_slice(&0u64.to_ne_bytes());
            m.extend_from_slice(&pid.to_ne_bytes());
            m.extend_from_slice(&tgid.to_ne_bytes());
            assert_eq!(m.len(), 60);
            m
        }
        let mut buf = event(1, 41, 41);
        buf.extend(event(PROC_EVENT_EXEC, 4243, 4242));
        assert_eq!(exec_tgids(&buf), vec![4242]);
        assert!(exec_tgids(&buf[..30]).is_empty());
    }

    #[test]
    fn published_targets_round_trip() {
        let targets = vec![
            (
                PathBuf::from("/usr/bin/wget"),
                "winlux-fw/r9900".to_string(),
            ),
            (
                PathBuf::from("/home/d/Program Files/App\tx/app.exe"),
                "winlux-fw/r3".into(),
            ),
        ];
        let text = targets_text(&targets);
        // Chemins seuls : ni numéro de règle ni cgroup.
        assert!(!text.contains("winlux-fw") && !text.contains("r9900"));
        let paths: Vec<PathBuf> = targets.iter().map(|(p, _)| p.clone()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(parse_targets(&text), sorted);
        assert!(parse_targets("").is_empty());
    }
}
