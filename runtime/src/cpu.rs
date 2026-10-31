//! Détection des capacités processeur et réglages CPU avancés.
//!
//! Tout part du matériel réellement exposé par Linux (`/sys`, `/proc/cpuinfo`
//! et CPUID). Chaque capacité absente reste absente du rapport : les
//! Paramètres n'affichent une section que si son `cap_*` vaut 1, si bien
//! qu'aucune commande n'est proposée sur une machine qui ne peut pas
//! l'exécuter (pas de P/E-cores, pas de cpufreq, pas de capteur, VM…).
//!
//! Le module lit une racine configurable pour être testé sur de faux arbres
//! `/sys` ; en production la racine est `/`.

use std::path::{Path, PathBuf};

/// Plafond de processeurs logiques acceptés depuis `/sys` (garde-fou contre
/// une liste corrompue comme `0-4294967295`).
pub const MAX_CPUS: usize = 4096;
/// Les masques Win32 (`DWORD_PTR`) couvrent le groupe 0 : 64 processeurs.
pub const MASK_BITS: usize = 64;

/// Accès au système de fichiers, enraciné pour les tests.
#[derive(Clone, Debug)]
pub struct Sys {
    root: PathBuf,
}

impl Sys {
    pub fn host() -> Self {
        Self::at("/")
    }
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel.trim_start_matches('/'))
    }
    /// Lit un petit fichier texte (borné à 64 Kio), sans espace final.
    pub fn read(&self, rel: &str) -> Option<String> {
        use std::io::Read;
        let file = std::fs::File::open(self.path(rel)).ok()?;
        let mut text = String::new();
        file.take(64 * 1024).read_to_string(&mut text).ok()?;
        Some(text.trim().to_owned())
    }
    pub fn read_num(&self, rel: &str) -> Option<i64> {
        self.read(rel)?.parse().ok()
    }
    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }
    /// Vrai seulement si le processus courant peut réellement écrire.
    pub fn writable(&self, rel: &str) -> bool {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(self.path(rel).as_os_str().as_bytes()) else {
            return false;
        };
        unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
    }
    pub fn write(&self, rel: &str, value: &str) -> Result<(), String> {
        std::fs::write(self.path(rel), value).map_err(|e| format!("{rel} : {e}"))
    }
    /// Entrées d'un dossier commençant par `prefix`, triées numériquement.
    pub fn list(&self, rel: &str, prefix: &str) -> Vec<String> {
        let Ok(dir) = std::fs::read_dir(self.path(rel)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = dir
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with(prefix))
            .collect();
        names.sort_by_key(|n| (trailing_number(n), n.clone()));
        names
    }
}

fn trailing_number(name: &str) -> u64 {
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().unwrap_or(u64::MAX)
}

/// Lit une liste Linux `0-3,8,10-11`. Une entrée invalide rend la liste vide :
/// mieux vaut masquer une section que montrer une topologie fausse.
pub fn parse_cpulist(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in text.trim().split(',').filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>(), b.trim().parse::<usize>()),
            None => (part.trim().parse::<usize>(), part.trim().parse::<usize>()),
        };
        let (Ok(a), Ok(b)) = (a, b) else {
            return Vec::new();
        };
        if a > b || b >= MAX_CPUS {
            return Vec::new();
        }
        out.extend(a..=b);
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn format_cpulist(cpus: &[usize]) -> String {
    let mut sorted = cpus.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = String::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        if !out.is_empty() {
            out.push(',');
        }
        if start == end {
            out.push_str(&start.to_string());
        } else {
            out.push_str(&format!("{start}-{end}"));
        }
        i += 1;
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreKind {
    /// Cœur performant d'un processeur hybride (Intel `cpu_core`).
    Performance,
    /// Cœur à haut rendement d'un processeur hybride (Intel `cpu_atom`).
    Efficiency,
    /// Processeur non hybride : tous les cœurs sont équivalents.
    Uniform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cpu {
    pub id: usize,
    pub kind: CoreKind,
    pub node: usize,
    pub package: usize,
    pub core: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fan {
    pub label: String,
    pub rpm: u32,
    /// Chemin relatif de `pwmN` si le ventilateur est pilotable par logiciel.
    pub pwm: Option<String>,
}

/// Droits d'ordonnancement du processus courant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Privileges {
    pub root: bool,
    pub rlimit_nice: u64,
    pub rlimit_rtprio: u64,
}

impl Privileges {
    pub fn current() -> Self {
        let mut nice = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        let mut rt = nice;
        unsafe {
            libc::getrlimit(libc::RLIMIT_NICE, &mut nice);
            libc::getrlimit(libc::RLIMIT_RTPRIO, &mut rt);
        }
        Self {
            root: unsafe { libc::geteuid() } == 0,
            rlimit_nice: nice.rlim_cur,
            rlimit_rtprio: rt.rlim_cur,
        }
    }
    /// Plus petite valeur `nice` atteignable (donc priorité maximale).
    pub fn nice_floor(&self) -> i32 {
        if self.root {
            -20
        } else {
            (20 - self.rlimit_nice.min(40) as i32).clamp(-20, 20)
        }
    }
    pub fn realtime(&self) -> bool {
        self.root || self.rlimit_rtprio > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Performance,
    Balanced,
    PowerSaver,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Performance, Profile::Balanced, Profile::PowerSaver];
    pub fn key(self) -> &'static str {
        match self {
            Profile::Performance => "performance",
            Profile::Balanced => "balanced",
            Profile::PowerSaver => "powersave",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == text)
    }
}

/// Ce qu'il faut écrire dans cpufreq pour obtenir un profil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernorPlan {
    pub governor: &'static str,
    /// Préférence énergétique `intel_pstate`/`amd-pstate` (HWP/CPPC).
    pub epp: Option<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Caps {
    pub model: String,
    pub cpus: Vec<Cpu>,
    /// Fréquence courante lisible (MHz) par processeur logique.
    pub freq_mhz: Vec<(usize, u32)>,
    pub max_mhz: u32,
    pub driver: String,
    pub governors: Vec<String>,
    pub governor: String,
    pub epp_values: Vec<String>,
    pub epp: String,
    /// Les fichiers de politique cpufreq sont modifiables par ce processus.
    pub governor_writable: bool,
    pub epp_writable: bool,
    pub cpuid_hybrid: bool,
    pub pcores: Vec<usize>,
    pub ecores: Vec<usize>,
    pub numa_nodes: Vec<(usize, Vec<usize>)>,
    pub temp_source: Option<String>,
    pub temp_c: Option<i32>,
    pub temp_crit_c: Option<i32>,
    pub throttle_events: Option<u64>,
    pub fans: Vec<Fan>,
    pub fan_control: bool,
    pub privileges: Option<Privileges>,
}

impl Caps {
    pub fn online(&self) -> Vec<usize> {
        self.cpus.iter().map(|c| c.id).collect()
    }
    pub fn has_freq(&self) -> bool {
        !self.freq_mhz.is_empty()
    }
    pub fn hybrid(&self) -> bool {
        !self.pcores.is_empty() && !self.ecores.is_empty()
    }
    pub fn numa(&self) -> bool {
        self.numa_nodes.len() > 1
    }
    pub fn affinity(&self) -> bool {
        self.cpus.len() > 1
    }
    pub fn plan(&self, profile: Profile) -> Option<GovernorPlan> {
        if !self.governor_writable {
            return None;
        }
        let has = |g: &str| self.governors.iter().any(|v| v == g);
        let epp = |value: &'static str| {
            (self.epp_writable && self.epp_values.iter().any(|v| v == value)).then_some(value)
        };
        match profile {
            Profile::Performance => has("performance").then_some(GovernorPlan {
                governor: "performance",
                epp: None,
            }),
            Profile::Balanced => {
                for governor in ["schedutil", "ondemand", "conservative"] {
                    if has(governor) {
                        return Some(GovernorPlan {
                            governor,
                            epp: None,
                        });
                    }
                }
                // intel_pstate/amd-pstate actifs : « powersave » est le
                // gouverneur dynamique, l'équilibre passe par l'EPP.
                let epp = epp("balance_performance").or(epp("default"))?;
                has("powersave").then_some(GovernorPlan {
                    governor: "powersave",
                    epp: Some(epp),
                })
            }
            Profile::PowerSaver => has("powersave").then_some(GovernorPlan {
                governor: "powersave",
                epp: epp("power"),
            }),
        }
    }
    /// Profils réellement applicables. Moins de deux : la section est masquée.
    pub fn profiles(&self) -> Vec<Profile> {
        let profiles: Vec<Profile> = Profile::ALL
            .into_iter()
            .filter(|p| self.plan(*p).is_some())
            .collect();
        if profiles.len() < 2 {
            return Vec::new();
        }
        profiles
    }
    pub fn current_profile(&self) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| {
            self.plan(*p).is_some_and(|plan| {
                plan.governor == self.governor && plan.epp.is_none_or(|e| e == self.epp)
            }) && (*p != Profile::PowerSaver
                || self.plan(Profile::Balanced).is_none_or(|b| {
                    b.governor != self.governor || b.epp.is_none_or(|e| e != self.epp)
                }))
        })
    }

    /// Rapport `clé=valeur` en ASCII lu par les Paramètres.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let mut put = |key: &str, value: &str| {
            let clean: String = value
                .chars()
                .map(|c| {
                    if c.is_ascii_graphic() || c == ' ' {
                        c
                    } else {
                        '?'
                    }
                })
                .take(160)
                .collect();
            out.push_str(key);
            out.push('=');
            out.push_str(&clean);
            out.push('\n');
        };
        let flag = |b: bool| if b { "1" } else { "0" };
        put("cpu_model", &self.model);
        put("cpu_count", &self.cpus.len().to_string());
        put("cpu_list", &format_cpulist(&self.online()));
        put("packages", &{
            let mut p: Vec<usize> = self.cpus.iter().map(|c| c.package).collect();
            p.sort_unstable();
            p.dedup();
            p.len().to_string()
        });
        put("cap_freq", flag(self.has_freq()));
        if self.has_freq() {
            let freqs: Vec<String> = self
                .freq_mhz
                .iter()
                .map(|(cpu, mhz)| format!("{cpu}:{mhz}"))
                .collect();
            put("freq", &freqs.join(","));
            put("freq_max", &self.max_mhz.to_string());
            put("freq_driver", &self.driver);
        }
        let profiles = self.profiles();
        put("cap_governor", flag(!profiles.is_empty()));
        if !profiles.is_empty() {
            let keys: Vec<&str> = profiles.iter().map(|p| p.key()).collect();
            put("profiles", &keys.join(","));
            put(
                "profile",
                self.current_profile().map(Profile::key).unwrap_or("custom"),
            );
            put("governor", &self.governor);
        }
        put("cap_hybrid", flag(self.hybrid()));
        // Information de diagnostic : le routage se fonde sur /sys, qui
        // reflète ce que le noyau sait vraiment ordonnancer.
        put("cpuid_hybrid", flag(self.cpuid_hybrid));
        if self.hybrid() {
            put("pcores", &format_cpulist(&self.pcores));
            put("ecores", &format_cpulist(&self.ecores));
        }
        put("cap_affinity", flag(self.affinity()));
        put("cap_numa", flag(self.numa()));
        if self.numa() {
            let nodes: Vec<String> = self
                .numa_nodes
                .iter()
                .map(|(n, cpus)| format!("{n}:{}", format_cpulist(cpus)))
                .collect();
            put("numa_nodes", &nodes.join(";"));
        }
        put("cap_thermal", flag(self.temp_c.is_some()));
        if let (Some(t), Some(src)) = (self.temp_c, &self.temp_source) {
            put("temp", &t.to_string());
            put("temp_path", src);
            if let Some(crit) = self.temp_crit_c {
                put("temp_crit", &crit.to_string());
            }
        }
        put("cap_throttle", flag(self.throttle_events.is_some()));
        if let Some(n) = self.throttle_events {
            put("throttle", &n.to_string());
        }
        put("cap_fans", flag(!self.fans.is_empty()));
        if !self.fans.is_empty() {
            let fans: Vec<String> = self
                .fans
                .iter()
                .map(|f| format!("{}:{}", f.label.replace([':', ','], " "), f.rpm))
                .collect();
            put("fans", &fans.join(","));
        }
        put(
            "cap_fan_control",
            flag(self.fan_control && self.temp_c.is_some()),
        );
        let privileges = self.privileges.unwrap_or(Privileges {
            root: false,
            rlimit_nice: 0,
            rlimit_rtprio: 0,
        });
        put("cap_priority", "1");
        put("cap_prio_high", flag(privileges.nice_floor() <= -10));
        put("cap_prio_realtime", flag(privileges.realtime()));
        out
    }
}

const CPU: &str = "sys/devices/system/cpu";

pub fn detect(sys: &Sys) -> Caps {
    let mut caps = Caps::default();
    let online = sys
        .read(&format!("{CPU}/online"))
        .map(|t| parse_cpulist(&t))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| {
            sys.list(CPU, "cpu")
                .iter()
                .filter_map(|n| n.strip_prefix("cpu")?.parse().ok())
                .collect()
        });
    caps.model = cpuinfo_model(sys).unwrap_or_default();
    caps.cpuid_hybrid = cpuid_hybrid();
    // P-cores / E-cores : uniquement si le noyau publie les deux PMU Intel
    // hybrides avec des listes disjointes et non vides.
    let pcores = sys
        .read("sys/devices/cpu_core/cpus")
        .map(|t| parse_cpulist(&t))
        .unwrap_or_default();
    let ecores = sys
        .read("sys/devices/cpu_atom/cpus")
        .map(|t| parse_cpulist(&t))
        .unwrap_or_default();
    let disjoint = pcores.iter().all(|c| !ecores.contains(c));
    if disjoint && !pcores.is_empty() && !ecores.is_empty() {
        caps.pcores = pcores.into_iter().filter(|c| online.contains(c)).collect();
        caps.ecores = ecores.into_iter().filter(|c| online.contains(c)).collect();
        if caps.pcores.is_empty() || caps.ecores.is_empty() {
            caps.pcores.clear();
            caps.ecores.clear();
        }
    }
    // NUMA : nœuds ayant au moins un processeur en ligne.
    for name in sys.list("sys/devices/system/node", "node") {
        let Some(id) = name.strip_prefix("node").and_then(|n| n.parse().ok()) else {
            continue;
        };
        let cpus: Vec<usize> = sys
            .read(&format!("sys/devices/system/node/{name}/cpulist"))
            .map(|t| parse_cpulist(&t))
            .unwrap_or_default()
            .into_iter()
            .filter(|c| online.contains(c))
            .collect();
        if !cpus.is_empty() {
            caps.numa_nodes.push((id, cpus));
        }
    }
    for &id in &online {
        let base = format!("{CPU}/cpu{id}");
        let kind = if caps.pcores.contains(&id) {
            CoreKind::Performance
        } else if caps.ecores.contains(&id) {
            CoreKind::Efficiency
        } else {
            CoreKind::Uniform
        };
        let node = caps
            .numa_nodes
            .iter()
            .find(|(_, c)| c.contains(&id))
            .map(|(n, _)| *n)
            .unwrap_or(0);
        let package = sys
            .read_num(&format!("{base}/topology/physical_package_id"))
            .unwrap_or(0)
            .max(0) as usize;
        let core = sys
            .read_num(&format!("{base}/topology/core_id"))
            .unwrap_or(id as i64)
            .max(0) as usize;
        caps.cpus.push(Cpu {
            id,
            kind,
            node,
            package,
            core,
        });
        if let Some(khz) = sys.read_num(&format!("{base}/cpufreq/scaling_cur_freq")) {
            if khz > 0 && khz < 20_000_000 {
                caps.freq_mhz.push((id, (khz / 1000) as u32));
            }
        }
        if let Some(khz) = sys.read_num(&format!("{base}/cpufreq/cpuinfo_max_freq")) {
            if khz > 0 && khz < 20_000_000 {
                caps.max_mhz = caps.max_mhz.max((khz / 1000) as u32);
            }
        }
    }
    detect_governor(sys, &mut caps);
    detect_thermal(sys, &mut caps);
    caps.throttle_events = detect_throttle(sys, &online);
    detect_fans(sys, &mut caps);
    caps
}

fn cpuinfo_model(sys: &Sys) -> Option<String> {
    let text = sys.read("proc/cpuinfo")?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        matches!(key.trim(), "model name" | "Model" | "Hardware")
            .then(|| value.split_whitespace().collect::<Vec<_>>().join(" "))
    })
}

/// CPUID.07H:EDX[15] annonce une architecture hybride (Alder Lake et après).
fn cpuid_hybrid() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid_count;
        let max = __cpuid_count(0, 0).eax;
        if max >= 7 {
            return __cpuid_count(7, 0).edx & (1 << 15) != 0;
        }
    }
    false
}

fn detect_governor(sys: &Sys, caps: &mut Caps) {
    let policies: Vec<String> = sys
        .list(&format!("{CPU}/cpufreq"), "policy")
        .into_iter()
        .map(|p| format!("{CPU}/cpufreq/{p}"))
        .collect();
    let Some(first) = policies.first() else {
        return;
    };
    caps.driver = sys
        .read(&format!("{first}/scaling_driver"))
        .unwrap_or_default();
    caps.governors = sys
        .read(&format!("{first}/scaling_available_governors"))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    caps.governor = sys
        .read(&format!("{first}/scaling_governor"))
        .unwrap_or_default();
    caps.epp_values = sys
        .read(&format!("{first}/energy_performance_available_preferences"))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    caps.epp = sys
        .read(&format!("{first}/energy_performance_preference"))
        .unwrap_or_default();
    caps.governor_writable = !caps.governor.is_empty()
        && policies
            .iter()
            .all(|p| sys.writable(&format!("{p}/scaling_governor")));
    caps.epp_writable = !caps.epp.is_empty()
        && policies
            .iter()
            .all(|p| sys.writable(&format!("{p}/energy_performance_preference")));
}

/// Températures plausibles seulement : les VM exposent souvent 0 °C ou des
/// valeurs fixes absurdes qu'il vaut mieux ne pas afficher.
fn plausible(milli: i64) -> Option<i32> {
    let c = milli / 1000;
    (1..=125).contains(&c).then_some(c as i32)
}

fn detect_thermal(sys: &Sys, caps: &mut Caps) {
    const CPU_SENSORS: [&str; 6] = [
        "coretemp",
        "k10temp",
        "zenpower",
        "cpu_thermal",
        "soc_thermal",
        "cpu-thermal",
    ];
    for hwmon in sys.list("sys/class/hwmon", "hwmon") {
        let base = format!("sys/class/hwmon/{hwmon}");
        let name = sys.read(&format!("{base}/name")).unwrap_or_default();
        if !CPU_SENSORS.contains(&name.as_str()) {
            continue;
        }
        let inputs: Vec<String> = sys
            .list(&base, "temp")
            .into_iter()
            .filter(|n| n.ends_with("_input"))
            .collect();
        let preferred = inputs.iter().find(|input| {
            let label = sys
                .read(&format!("{base}/{}", input.replace("_input", "_label")))
                .unwrap_or_default();
            label.starts_with("Package") || label == "Tdie" || label == "Tctl"
        });
        for input in preferred.into_iter().chain(inputs.iter()) {
            let rel = format!("{base}/{input}");
            if let Some(t) = sys.read_num(&rel).and_then(plausible) {
                caps.temp_c = Some(t);
                caps.temp_source = Some(format!("/{rel}"));
                caps.temp_crit_c = sys
                    .read_num(&rel.replace("_input", "_crit"))
                    .or_else(|| sys.read_num(&rel.replace("_input", "_max")))
                    .and_then(plausible);
                return;
            }
        }
    }
    for zone in sys.list("sys/class/thermal", "thermal_zone") {
        let base = format!("sys/class/thermal/{zone}");
        let kind = sys.read(&format!("{base}/type")).unwrap_or_default();
        if !matches!(
            kind.as_str(),
            "x86_pkg_temp" | "cpu-thermal" | "cpu_thermal"
        ) {
            continue;
        }
        if let Some(t) = sys.read_num(&format!("{base}/temp")).and_then(plausible) {
            caps.temp_c = Some(t);
            caps.temp_source = Some(format!("/{base}/temp"));
            return;
        }
    }
}

fn detect_throttle(sys: &Sys, online: &[usize]) -> Option<u64> {
    let mut found = false;
    let mut cores = 0u64;
    let mut package = 0u64;
    for id in online {
        let base = format!("{CPU}/cpu{id}/thermal_throttle");
        if let Some(n) = sys.read_num(&format!("{base}/core_throttle_count")) {
            found = true;
            cores = cores.saturating_add(n.max(0) as u64);
        }
        if let Some(n) = sys.read_num(&format!("{base}/package_throttle_count")) {
            found = true;
            package = package.max(n.max(0) as u64);
        }
    }
    found.then_some(cores.saturating_add(package))
}

fn detect_fans(sys: &Sys, caps: &mut Caps) {
    let mut controllable = false;
    for hwmon in sys.list("sys/class/hwmon", "hwmon") {
        let base = format!("sys/class/hwmon/{hwmon}");
        for input in sys
            .list(&base, "fan")
            .into_iter()
            .filter(|n| n.ends_with("_input"))
        {
            let n = input.trim_start_matches("fan").trim_end_matches("_input");
            let Some(rpm) = sys.read_num(&format!("{base}/{input}")) else {
                continue;
            };
            if !(0..=30_000).contains(&rpm) {
                continue;
            }
            let pwm = format!("{base}/pwm{n}");
            let pwm =
                (sys.exists(&pwm) && sys.writable(&pwm) && sys.writable(&format!("{pwm}_enable")))
                    .then_some(format!("/{pwm}"));
            // Un connecteur à 0 tr/min sans commande est un emplacement vide.
            if rpm == 0 && pwm.is_none() {
                continue;
            }
            controllable |= pwm.is_some();
            let label = sys
                .read(&format!("{base}/fan{n}_label"))
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| format!("Ventilateur {}", caps.fans.len() + 1));
            caps.fans.push(Fan {
                label,
                rpm: rpm as u32,
                pwm,
            });
        }
    }
    caps.fan_control = controllable;
}

/// Applique un profil d'alimentation sur toutes les politiques cpufreq.
pub fn apply_profile(sys: &Sys, caps: &Caps, profile: Profile) -> Result<(), String> {
    let plan = caps
        .plan(profile)
        .ok_or_else(|| format!("profil {} indisponible sur ce processeur", profile.key()))?;
    for policy in sys.list(&format!("{CPU}/cpufreq"), "policy") {
        let base = format!("{CPU}/cpufreq/{policy}");
        sys.write(&format!("{base}/scaling_governor"), plan.governor)?;
        if let Some(epp) = plan.epp {
            sys.write(&format!("{base}/energy_performance_preference"), epp)?;
        }
    }
    Ok(())
}

// --- Courbes de ventilateurs --------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanCurve {
    Auto,
    Quiet,
    Balanced,
    Performance,
}

impl FanCurve {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "auto" => FanCurve::Auto,
            "quiet" => FanCurve::Quiet,
            "balanced" => FanCurve::Balanced,
            "performance" => FanCurve::Performance,
            _ => return None,
        })
    }
    fn points(self) -> &'static [(i32, u32)] {
        match self {
            FanCurve::Auto => &[],
            FanCurve::Quiet => &[(40, 20), (60, 35), (75, 60), (85, 100)],
            FanCurve::Balanced => &[(40, 30), (60, 50), (75, 80), (85, 100)],
            FanCurve::Performance => &[(40, 50), (60, 70), (70, 100)],
        }
    }
    /// Vitesse en pourcentage. Garde-fous : jamais sous 20 %, 100 % dès
    /// 85 °C et 100 % si la température est illisible.
    pub fn duty(self, temp: Option<i32>) -> u32 {
        let Some(t) = temp else {
            return 100;
        };
        if t >= 85 {
            return 100;
        }
        let points = self.points();
        let Some(&(t0, d0)) = points.first() else {
            return 100;
        };
        if t <= t0 {
            return d0.max(20);
        }
        for pair in points.windows(2) {
            let ((a, da), (b, db)) = (pair[0], pair[1]);
            if t <= b {
                let span = (b - a).max(1) as u32;
                let pos = (t - a) as u32;
                return (da + (db.saturating_sub(da)) * pos / span).clamp(20, 100);
            }
        }
        100
    }
}

/// Boucle de régulation lancée par `winlux --fan-daemon`. Elle relit la
/// courbe choisie dans `/etc/winlux/cpu.conf` et rend la main au firmware
/// (mode automatique d'origine) quand la courbe repasse sur `auto` ou en cas
/// d'erreur.
pub fn fan_daemon(sys: &Sys) -> i32 {
    let caps = detect(sys);
    let fans: Vec<String> = caps.fans.iter().filter_map(|f| f.pwm.clone()).collect();
    let Some(temp_path) = caps.temp_source.clone() else {
        eprintln!("winlux: aucun capteur de température processeur : courbe refusée");
        return 1;
    };
    if fans.is_empty() {
        eprintln!("winlux: aucun ventilateur pilotable");
        return 1;
    }
    let original: Vec<(String, String)> = fans
        .iter()
        .filter_map(|pwm| Some((pwm.clone(), sys.read(&format!("{pwm}_enable"))?)))
        .collect();
    let restore = || {
        for (pwm, mode) in &original {
            let _ = sys.write(&format!("{pwm}_enable"), mode);
        }
    };
    loop {
        let curve = sys
            .read("etc/winlux/cpu.conf")
            .and_then(|t| config_value(&t, "FAN_CURVE").and_then(FanCurve::parse))
            .unwrap_or(FanCurve::Auto);
        if curve == FanCurve::Auto {
            restore();
            return 0;
        }
        let temp = sys.read_num(&temp_path).and_then(plausible);
        for pwm in &fans {
            let max = sys
                .read_num(&format!("{pwm}_max"))
                .filter(|m| *m > 0)
                .unwrap_or(255);
            let value = max * curve.duty(temp) as i64 / 100;
            if sys.write(&format!("{pwm}_enable"), "1").is_err()
                || sys.write(pwm, &value.to_string()).is_err()
            {
                eprintln!("winlux: écriture {pwm} refusée, retour au mode automatique");
                restore();
                return 1;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

// --- Lancement des .exe : affinité, priorité, P/E-cores, NUMA ------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoreChoice {
    #[default]
    Auto,
    Performance,
    Efficiency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HybridPolicy {
    /// Linux et Intel Thread Director répartissent seuls.
    #[default]
    Auto,
    /// Jeux et applications 3D sur les P-cores.
    Heavy,
    /// Tous les programmes Windows sur les P-cores.
    Performance,
    /// Tous les programmes Windows sur les E-cores (économie).
    Efficiency,
}

impl HybridPolicy {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "auto" => HybridPolicy::Auto,
            "heavy" => HybridPolicy::Heavy,
            "performance" => HybridPolicy::Performance,
            "efficiency" => HybridPolicy::Efficiency,
            _ => return None,
        })
    }
}

/// Priorités du Gestionnaire des tâches Windows et leur `nice` Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl Priority {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "low" => Priority::Idle,
            "below" => Priority::BelowNormal,
            "normal" => Priority::Normal,
            "above" => Priority::AboveNormal,
            "high" => Priority::High,
            "realtime" => Priority::Realtime,
            _ => return None,
        })
    }
    pub fn from_class(class: u32) -> Option<Self> {
        Some(match class {
            0x40 => Priority::Idle,
            0x4000 => Priority::BelowNormal,
            0x20 => Priority::Normal,
            0x8000 => Priority::AboveNormal,
            0x80 => Priority::High,
            0x100 => Priority::Realtime,
            _ => return None,
        })
    }
    pub fn class(self) -> u32 {
        match self {
            Priority::Idle => 0x40,
            Priority::BelowNormal => 0x4000,
            Priority::Normal => 0x20,
            Priority::AboveNormal => 0x8000,
            Priority::High => 0x80,
            Priority::Realtime => 0x100,
        }
    }
    pub fn nice(self) -> i32 {
        match self {
            Priority::Idle => 19,
            Priority::BelowNormal => 10,
            Priority::Normal => 0,
            Priority::AboveNormal => -5,
            Priority::High | Priority::Realtime => -10,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaunchSettings {
    pub cores: CoreChoice,
    pub affinity: Vec<usize>,
    pub priority: Option<Priority>,
    pub numa_node: Option<usize>,
}

/// Processeurs à utiliser pour un programme ; `None` = ne rien imposer.
pub fn launch_cpus(
    caps: &Caps,
    settings: &LaunchSettings,
    policy: HybridPolicy,
    heavy: bool,
) -> Option<Vec<usize>> {
    let online = caps.online();
    let mut cpus: Option<Vec<usize>> = None;
    if !settings.affinity.is_empty() {
        let chosen: Vec<usize> = settings
            .affinity
            .iter()
            .copied()
            .filter(|c| online.contains(c))
            .collect();
        if !chosen.is_empty() {
            cpus = Some(chosen);
        }
    }
    if cpus.is_none() && caps.hybrid() {
        let kind = match settings.cores {
            CoreChoice::Performance => Some(CoreKind::Performance),
            CoreChoice::Efficiency => Some(CoreKind::Efficiency),
            CoreChoice::Auto => match policy {
                HybridPolicy::Auto => None,
                HybridPolicy::Heavy => heavy.then_some(CoreKind::Performance),
                HybridPolicy::Performance => Some(CoreKind::Performance),
                HybridPolicy::Efficiency => Some(CoreKind::Efficiency),
            },
        };
        cpus = kind.map(|k| match k {
            CoreKind::Efficiency => caps.ecores.clone(),
            _ => caps.pcores.clone(),
        });
    }
    if let Some(node) = settings.numa_node.filter(|_| caps.numa()) {
        if let Some((_, node_cpus)) = caps.numa_nodes.iter().find(|(n, _)| *n == node) {
            let base = cpus.clone().unwrap_or_else(|| node_cpus.clone());
            let both: Vec<usize> = base
                .iter()
                .copied()
                .filter(|c| node_cpus.contains(c))
                .collect();
            cpus = Some(if both.is_empty() {
                node_cpus.clone()
            } else {
                both
            });
        }
    }
    // Tous les processeurs choisis : inutile d'imposer quoi que ce soit.
    cpus.filter(|c| !c.is_empty() && c.len() < online.len())
}

/// Les jeux et applications 3D importent Direct3D, OpenGL ou Vulkan.
pub fn is_heavy(imports: &[String]) -> bool {
    imports.iter().any(|name| {
        let n = name.to_ascii_lowercase();
        n.starts_with("d3d")
            || n.starts_with("dxgi")
            || n == "opengl32.dll"
            || n.starts_with("vulkan-1")
            || n.starts_with("xinput")
    })
}

pub fn config_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        (k == key).then_some(v.trim())
    })
}

/// Identifiants Linux des threads du processus courant.
pub fn task_ids() -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc/self/task") else {
        return vec![0];
    };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .collect()
}

pub fn set_affinity(tid: i32, cpus: &[usize]) -> bool {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        let mut any = false;
        for &cpu in cpus {
            if cpu < libc::CPU_SETSIZE as usize {
                libc::CPU_SET(cpu, &mut set);
                any = true;
            }
        }
        any && libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &set) == 0
    }
}

pub fn get_affinity(tid: i32) -> Vec<usize> {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &mut set) != 0 {
            return Vec::new();
        }
        (0..libc::CPU_SETSIZE as usize)
            .filter(|&c| libc::CPU_ISSET(c, &set))
            .collect()
    }
}

/// Valeur `nice` atteignable la plus proche de la demande.
pub fn reachable_nice(want: i32, privileges: &Privileges) -> i32 {
    // Linux autorise toujours à baisser sa priorité ; la relever demande
    // `nice >= 20 - RLIMIT_NICE` (ou root). On garde sinon la valeur normale.
    let floor = privileges.nice_floor();
    let nice = if want >= 0 || floor <= want {
        want
    } else if floor < 0 {
        floor
    } else {
        0
    };
    nice.clamp(-20, 19)
}

pub fn set_nice(tid: i32, nice: i32) -> bool {
    unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) == 0 }
}

/// Applique priorité et processeurs au démarrage d'un `.exe`.
pub fn apply_launch(settings: &LaunchSettings, imports: &[String]) {
    let sys = Sys::host();
    let caps = detect(&sys);
    let policy = sys
        .read("etc/winlux/cpu.conf")
        .and_then(|t| config_value(&t, "HYBRID_POLICY").and_then(HybridPolicy::parse))
        .unwrap_or_default();
    if let Some(cpus) = launch_cpus(&caps, settings, policy, is_heavy(imports)) {
        if set_affinity(0, &cpus) {
            crate::log_info!(
                "cpu",
                "programme placé sur les processeurs {}",
                format_cpulist(&cpus)
            );
        } else {
            crate::log_warn!(
                "cpu",
                "affinité {} refusée par Linux",
                format_cpulist(&cpus)
            );
        }
        if let Some(node) = settings.numa_node.filter(|_| caps.numa()) {
            prefer_numa_node(node);
        }
    }
    if let Some(priority) = settings.priority {
        apply_priority(priority, &Privileges::current());
    }
}

/// Priorité de processus ; « temps réel » utilise SCHED_RR si autorisé,
/// sinon la priorité haute atteignable. Retourne la priorité obtenue.
pub fn apply_priority(priority: Priority, privileges: &Privileges) -> Priority {
    if priority == Priority::Realtime && privileges.realtime() {
        let param = libc::sched_param {
            sched_priority: 1.max(privileges.rlimit_rtprio.min(10) as i32),
        };
        let ok = task_ids()
            .into_iter()
            .all(|tid| unsafe { libc::sched_setscheduler(tid, libc::SCHED_RR, &param) == 0 });
        if ok {
            return Priority::Realtime;
        }
    }
    let nice = reachable_nice(priority.nice(), privileges);
    for tid in task_ids() {
        set_nice(tid, nice);
    }
    if priority == Priority::Realtime {
        Priority::High
    } else {
        priority
    }
}

fn prefer_numa_node(node: usize) {
    if node >= 64 {
        return;
    }
    // MPOL_PREFERRED : la mémoire est allouée sur le nœud choisi, avec repli
    // automatique si ce nœud est plein.
    let mask: u64 = 1 << node;
    unsafe {
        libc::syscall(
            libc::SYS_set_mempolicy,
            1 as libc::c_int,
            &mask as *const u64,
            65 as libc::c_ulong,
        );
    }
}

/// Point d'entrée `winlux --cpu-report [RACINE]`.
pub fn report_command(root: Option<&Path>) -> i32 {
    let sys = root.map(Sys::at).unwrap_or_else(Sys::host);
    let mut caps = detect(&sys);
    caps.privileges = Some(Privileges::current());
    print!("{}", caps.report());
    0
}

/// Point d'entrée `winlux --cpu-profile PROFIL [RACINE]`.
pub fn profile_command(profile: &str, root: Option<&Path>) -> i32 {
    let sys = root.map(Sys::at).unwrap_or_else(Sys::host);
    let Some(profile) = Profile::parse(profile) else {
        eprintln!("winlux: profil inconnu {profile} (performance, balanced, powersave)");
        return 2;
    };
    match apply_profile(&sys, &detect(&sys), profile) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("winlux: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);
    impl Tree {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "winlux-cpu-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tree(dir)
        }
        fn put(&self, rel: &str, text: &str) -> &Self {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            self
        }
        fn sys(&self) -> Sys {
            Sys::at(&self.0)
        }
        fn cpus(&self, n: usize) -> &Self {
            self.put("sys/devices/system/cpu/online", &format!("0-{}\n", n - 1));
            for i in 0..n {
                self.put(
                    &format!("sys/devices/system/cpu/cpu{i}/topology/core_id"),
                    &format!("{}\n", i / 2),
                );
            }
            self
        }
        fn cpufreq(&self, n: usize, governors: &str, governor: &str) -> &Self {
            for i in 0..n {
                self.put(
                    &format!("sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"),
                    &format!("{}\n", 1_200_000 + i * 100_000),
                );
                self.put(
                    &format!("sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq"),
                    "4800000\n",
                );
                let p = format!("sys/devices/system/cpu/cpufreq/policy{i}");
                self.put(&format!("{p}/scaling_available_governors"), governors);
                self.put(&format!("{p}/scaling_governor"), governor);
                self.put(&format!("{p}/scaling_driver"), "acpi-cpufreq");
            }
            self
        }
        fn get(&self, rel: &str) -> String {
            std::fs::read_to_string(self.0.join(rel)).unwrap()
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn key<'a>(report: &'a str, k: &str) -> Option<&'a str> {
        config_value(report, k)
    }

    #[test]
    fn listes_de_processeurs_bornees() {
        assert_eq!(parse_cpulist("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(parse_cpulist(""), Vec::<usize>::new());
        assert_eq!(parse_cpulist("3-1"), Vec::<usize>::new());
        assert_eq!(parse_cpulist("0-4294967295"), Vec::<usize>::new());
        assert_eq!(parse_cpulist("0,x"), Vec::<usize>::new());
        assert_eq!(format_cpulist(&[0, 1, 2, 3, 8, 10, 11]), "0-3,8,10-11");
        assert_eq!(format_cpulist(&[5]), "5");
    }

    #[test]
    fn machine_minimale_sans_aucune_capacite_avancee() {
        // VM typique : un seul processeur, ni cpufreq, ni capteurs.
        let t = Tree::new("vm");
        t.cpus(1);
        let caps = detect(&t.sys());
        let r = caps.report();
        for cap in [
            "cap_freq",
            "cap_governor",
            "cap_hybrid",
            "cap_numa",
            "cap_thermal",
            "cap_throttle",
            "cap_fans",
            "cap_fan_control",
            "cap_affinity",
        ] {
            assert_eq!(key(&r, cap), Some("0"), "{cap} doit être masqué");
        }
        assert_eq!(key(&r, "cpu_count"), Some("1"));
        assert!(key(&r, "pcores").is_none() && key(&r, "freq").is_none());
    }

    #[test]
    fn processeur_classique_sans_p_cores_ni_e_cores() {
        let t = Tree::new("uniform");
        t.cpus(8)
            .cpufreq(8, "performance schedutil powersave\n", "schedutil\n");
        let caps = detect(&t.sys());
        assert!(!caps.hybrid());
        assert!(caps.cpus.iter().all(|c| c.kind == CoreKind::Uniform));
        let r = caps.report();
        assert_eq!(key(&r, "cap_hybrid"), Some("0"));
        assert_eq!(key(&r, "cap_affinity"), Some("1"));
        assert_eq!(key(&r, "cap_freq"), Some("1"));
        assert_eq!(key(&r, "freq_max"), Some("4800"));
        assert!(key(&r, "freq").unwrap().starts_with("0:1200,1:1300"));
    }

    #[test]
    fn hybride_intel_detecte_seulement_avec_deux_listes_disjointes() {
        let t = Tree::new("hybrid");
        t.cpus(16)
            .put("sys/devices/cpu_core/cpus", "0-7\n")
            .put("sys/devices/cpu_atom/cpus", "8-15\n");
        let caps = detect(&t.sys());
        assert!(caps.hybrid());
        assert_eq!(caps.cpus[0].kind, CoreKind::Performance);
        assert_eq!(caps.cpus[12].kind, CoreKind::Efficiency);
        let r = caps.report();
        assert_eq!(key(&r, "pcores"), Some("0-7"));
        assert_eq!(key(&r, "ecores"), Some("8-15"));

        // Une seule PMU (P-cores sans E-cores) : pas d'hybride.
        let t = Tree::new("pcore-only");
        t.cpus(8).put("sys/devices/cpu_core/cpus", "0-7\n");
        assert!(!detect(&t.sys()).hybrid());
        // Listes qui se chevauchent : incohérent, donc masqué.
        let t = Tree::new("overlap");
        t.cpus(8)
            .put("sys/devices/cpu_core/cpus", "0-5\n")
            .put("sys/devices/cpu_atom/cpus", "4-7\n");
        assert!(!detect(&t.sys()).hybrid());
        // E-cores tous hors ligne : pas de routage possible.
        let t = Tree::new("offline");
        t.cpus(8)
            .put("sys/devices/cpu_core/cpus", "0-7\n")
            .put("sys/devices/cpu_atom/cpus", "8-11\n");
        assert!(!detect(&t.sys()).hybrid());
    }

    #[test]
    fn numa_uniquement_avec_plusieurs_noeuds_peuples() {
        let t = Tree::new("numa1");
        t.cpus(4)
            .put("sys/devices/system/node/node0/cpulist", "0-3\n");
        assert!(!detect(&t.sys()).numa());
        let t = Tree::new("numa-empty");
        t.cpus(4)
            .put("sys/devices/system/node/node0/cpulist", "0-3\n")
            .put("sys/devices/system/node/node1/cpulist", "\n");
        assert!(
            !detect(&t.sys()).numa(),
            "un nœud mémoire sans CPU ne compte pas"
        );
        let t = Tree::new("numa2");
        t.cpus(8)
            .put("sys/devices/system/node/node0/cpulist", "0-3\n")
            .put("sys/devices/system/node/node1/cpulist", "4-7\n");
        let caps = detect(&t.sys());
        assert!(caps.numa());
        assert_eq!(caps.cpus[5].node, 1);
        assert_eq!(key(&caps.report(), "numa_nodes"), Some("0:0-3;1:4-7"));
    }

    #[test]
    fn profils_selon_les_gouverneurs_reellement_disponibles() {
        let t = Tree::new("gov");
        t.cpus(2)
            .cpufreq(2, "performance ondemand powersave\n", "ondemand\n");
        let caps = detect(&t.sys());
        // Les fichiers du test appartiennent à l'utilisateur : modifiables.
        assert!(caps.governor_writable);
        assert_eq!(caps.profiles(), Profile::ALL.to_vec());
        assert_eq!(caps.current_profile(), Some(Profile::Balanced));
        assert_eq!(apply_profile(&t.sys(), &caps, Profile::Performance), Ok(()));
        assert_eq!(
            t.get("sys/devices/system/cpu/cpufreq/policy1/scaling_governor"),
            "performance"
        );

        // intel_pstate sans EPP : seulement performance/powersave.
        let t = Tree::new("pstate");
        t.cpus(2)
            .cpufreq(2, "performance powersave\n", "powersave\n");
        let caps = detect(&t.sys());
        assert_eq!(
            caps.profiles(),
            vec![Profile::Performance, Profile::PowerSaver]
        );
        assert!(apply_profile(&t.sys(), &caps, Profile::Balanced).is_err());

        // intel_pstate avec EPP : l'équilibre passe par la préférence.
        for i in 0..2 {
            let p = format!("sys/devices/system/cpu/cpufreq/policy{i}");
            t.put(
                &format!("{p}/energy_performance_available_preferences"),
                "default performance balance_performance balance_power power\n",
            );
            t.put(
                &format!("{p}/energy_performance_preference"),
                "balance_performance\n",
            );
        }
        let caps = detect(&t.sys());
        assert_eq!(caps.profiles().len(), 3);
        assert_eq!(caps.current_profile(), Some(Profile::Balanced));
        apply_profile(&t.sys(), &caps, Profile::PowerSaver).unwrap();
        assert_eq!(
            t.get("sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference"),
            "power"
        );
        assert_eq!(
            detect(&t.sys()).current_profile(),
            Some(Profile::PowerSaver)
        );

        // Un seul gouverneur : rien à choisir, section masquée.
        let t = Tree::new("single");
        t.cpus(2).cpufreq(2, "performance\n", "performance\n");
        let r = detect(&t.sys()).report();
        assert_eq!(key(&r, "cap_governor"), Some("0"));
        assert_eq!(key(&r, "cap_freq"), Some("1"), "la fréquence reste visible");
    }

    #[test]
    fn capteurs_plausibles_uniquement() {
        let t = Tree::new("thermal");
        t.cpus(2)
            .put("sys/class/hwmon/hwmon0/name", "acpitz\n")
            .put("sys/class/hwmon/hwmon0/temp1_input", "27800\n")
            .put("sys/class/hwmon/hwmon1/name", "coretemp\n")
            .put("sys/class/hwmon/hwmon1/temp1_label", "Package id 0\n")
            .put("sys/class/hwmon/hwmon1/temp1_input", "54000\n")
            .put("sys/class/hwmon/hwmon1/temp1_crit", "100000\n")
            .put(
                "sys/devices/system/cpu/cpu0/thermal_throttle/core_throttle_count",
                "3\n",
            )
            .put(
                "sys/devices/system/cpu/cpu1/thermal_throttle/core_throttle_count",
                "2\n",
            )
            .put(
                "sys/devices/system/cpu/cpu0/thermal_throttle/package_throttle_count",
                "7\n",
            );
        let r = detect(&t.sys()).report();
        assert_eq!(key(&r, "cap_thermal"), Some("1"));
        assert_eq!(key(&r, "temp"), Some("54"));
        assert_eq!(key(&r, "temp_crit"), Some("100"));
        assert_eq!(
            key(&r, "temp_path"),
            Some("/sys/class/hwmon/hwmon1/temp1_input")
        );
        assert_eq!(key(&r, "throttle"), Some("12"));

        // Capteur à 0 °C (VM) : masqué.
        let t = Tree::new("thermal-vm");
        t.cpus(1)
            .put("sys/class/hwmon/hwmon0/name", "coretemp\n")
            .put("sys/class/hwmon/hwmon0/temp1_input", "0\n");
        assert_eq!(key(&detect(&t.sys()).report(), "cap_thermal"), Some("0"));
    }

    #[test]
    fn ventilateurs_et_commande_seulement_si_exposes() {
        let t = Tree::new("fans");
        t.cpus(2)
            .put("sys/class/hwmon/hwmon2/name", "nct6775\n")
            .put("sys/class/hwmon/hwmon2/fan1_input", "1180\n")
            .put("sys/class/hwmon/hwmon2/fan2_input", "0\n");
        let caps = detect(&t.sys());
        assert_eq!(caps.fans.len(), 1, "le connecteur vide est ignoré");
        let r = caps.report();
        assert_eq!(key(&r, "fans"), Some("Ventilateur 1:1180"));
        assert_eq!(key(&r, "cap_fan_control"), Some("0"));
        // pwm présent mais aucun capteur CPU : pas de courbe possible.
        t.put("sys/class/hwmon/hwmon2/pwm1", "128\n")
            .put("sys/class/hwmon/hwmon2/pwm1_enable", "2\n");
        let caps = detect(&t.sys());
        assert!(caps.fan_control);
        assert_eq!(key(&caps.report(), "cap_fan_control"), Some("0"));
        t.put("sys/class/hwmon/hwmon1/name", "k10temp\n")
            .put("sys/class/hwmon/hwmon1/temp1_label", "Tctl\n")
            .put("sys/class/hwmon/hwmon1/temp1_input", "61000\n");
        assert_eq!(
            key(&detect(&t.sys()).report(), "cap_fan_control"),
            Some("1")
        );
    }

    #[test]
    fn courbes_de_ventilateur_avec_garde_fous() {
        assert_eq!(FanCurve::Quiet.duty(None), 100);
        assert_eq!(FanCurve::Quiet.duty(Some(20)), 20);
        assert_eq!(FanCurve::Quiet.duty(Some(90)), 100);
        assert_eq!(FanCurve::Balanced.duty(Some(50)), 40);
        assert_eq!(FanCurve::Performance.duty(Some(70)), 100);
        assert!(FanCurve::Quiet.duty(Some(65)) < FanCurve::Performance.duty(Some(65)));
        for t in 0..=100 {
            assert!((20..=100).contains(&FanCurve::Quiet.duty(Some(t))));
        }
    }

    #[test]
    fn placement_des_programmes() {
        let t = Tree::new("launch");
        t.cpus(16)
            .put("sys/devices/cpu_core/cpus", "0-7\n")
            .put("sys/devices/cpu_atom/cpus", "8-15\n");
        let caps = detect(&t.sys());
        let auto = LaunchSettings::default();
        assert_eq!(launch_cpus(&caps, &auto, HybridPolicy::Auto, true), None);
        assert_eq!(
            launch_cpus(&caps, &auto, HybridPolicy::Heavy, true),
            Some((0..8).collect())
        );
        assert_eq!(launch_cpus(&caps, &auto, HybridPolicy::Heavy, false), None);
        let eco = LaunchSettings {
            cores: CoreChoice::Efficiency,
            ..Default::default()
        };
        assert_eq!(
            launch_cpus(&caps, &eco, HybridPolicy::Performance, true),
            Some((8..16).collect())
        );
        let pinned = LaunchSettings {
            affinity: vec![2, 3, 99],
            ..Default::default()
        };
        assert_eq!(
            launch_cpus(&caps, &pinned, HybridPolicy::Efficiency, true),
            Some(vec![2, 3]),
            "l'affinité manuelle prime et ignore les cœurs inexistants"
        );
        let missing = LaunchSettings {
            affinity: vec![99],
            ..Default::default()
        };
        assert_eq!(launch_cpus(&caps, &missing, HybridPolicy::Auto, true), None);

        // Sans hybride, les choix P/E sont ignorés.
        let t = Tree::new("launch-uniform");
        t.cpus(8);
        let caps = detect(&t.sys());
        let p = LaunchSettings {
            cores: CoreChoice::Performance,
            ..Default::default()
        };
        assert_eq!(
            launch_cpus(&caps, &p, HybridPolicy::Performance, true),
            None
        );
        // NUMA absent : le nœud demandé est ignoré.
        let n = LaunchSettings {
            numa_node: Some(1),
            ..Default::default()
        };
        assert_eq!(launch_cpus(&caps, &n, HybridPolicy::Auto, true), None);

        let t = Tree::new("launch-numa");
        t.cpus(8)
            .put("sys/devices/system/node/node0/cpulist", "0-3\n")
            .put("sys/devices/system/node/node1/cpulist", "4-7\n");
        let caps = detect(&t.sys());
        assert_eq!(
            launch_cpus(&caps, &n, HybridPolicy::Auto, false),
            Some(vec![4, 5, 6, 7])
        );
        let both = LaunchSettings {
            affinity: vec![1, 5],
            numa_node: Some(1),
            ..Default::default()
        };
        assert_eq!(
            launch_cpus(&caps, &both, HybridPolicy::Auto, false),
            Some(vec![5])
        );
    }

    #[test]
    fn priorites_et_droits() {
        let user = Privileges {
            root: false,
            rlimit_nice: 0,
            rlimit_rtprio: 0,
        };
        assert_eq!(user.nice_floor(), 20);
        assert_eq!(reachable_nice(Priority::High.nice(), &user), 0);
        assert_eq!(reachable_nice(Priority::Idle.nice(), &user), 19);
        assert!(!user.realtime());
        let gamer = Privileges {
            root: false,
            rlimit_nice: 40,
            rlimit_rtprio: 0,
        };
        assert_eq!(reachable_nice(Priority::High.nice(), &gamer), -10);
        let root = Privileges {
            root: true,
            rlimit_nice: 0,
            rlimit_rtprio: 0,
        };
        assert!(root.realtime());
        let mut caps = Caps::default();
        caps.privileges = Some(user);
        assert_eq!(key(&caps.report(), "cap_prio_high"), Some("0"));
        assert_eq!(key(&caps.report(), "cap_prio_realtime"), Some("0"));
        caps.privileges = Some(root);
        assert_eq!(key(&caps.report(), "cap_prio_realtime"), Some("1"));
        for p in [
            Priority::Idle,
            Priority::BelowNormal,
            Priority::Normal,
            Priority::AboveNormal,
            Priority::High,
            Priority::Realtime,
        ] {
            assert_eq!(Priority::from_class(p.class()), Some(p));
        }
        assert!(is_heavy(&["KERNEL32.dll".into(), "d3d11.dll".into()]));
        assert!(!is_heavy(&["kernel32.dll".into(), "user32.dll".into()]));
    }

    #[test]
    fn rapport_ascii_sans_injection_de_lignes() {
        let caps = Caps {
            model: "Intel(R) Core\u{2122} i9\nprofile=performance".into(),
            ..Default::default()
        };
        let r = caps.report();
        assert!(r
            .lines()
            .next()
            .unwrap()
            .starts_with("cpu_model=Intel(R) Core? i9?profile"));
        assert!(r.is_ascii());
    }
}
