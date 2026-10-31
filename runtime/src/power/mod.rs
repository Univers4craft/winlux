//! Gestion de l'alimentation Winlux, écrite de zéro.
//!
//! Tout passe par les interfaces du noyau Linux : `/sys/class/power_supply`
//! pour la batterie et le secteur, `cpufreq` pour le gouverneur, la classe
//! `backlight` pour la luminosité et `/sys/power` pour la veille et
//! l'hibernation. Le même module sert :
//! - aux API Win32 (`GetSystemPowerStatus`, `SetThreadExecutionState`,
//!   `PowerSetActiveScheme`...) appelées par les `.exe` ;
//! - à la commande `winlux --power ...` utilisée par `winlux-control`, l'init
//!   maison et le démon de veille.
//!
//! La racine du système de fichiers peut être déplacée avec
//! `WINLUX_POWER_ROOT` pour les tests (faux sysfs).

pub mod cli;
pub mod daemon;
pub mod sleep;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Racine des chemins système (« / » hors tests).
pub fn root() -> PathBuf {
    std::env::var_os("WINLUX_POWER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub(crate) fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub(crate) fn read_u64(path: &Path) -> Option<u64> {
    read_trim(path)?.parse::<i64>().ok().map(i64::unsigned_abs)
}

/// Écrit une valeur dans un fichier sysfs existant. On ne crée jamais un
/// attribut absent : sur une VM sans rétroéclairage, rien n'est inventé.
pub(crate) fn write_existing(path: &Path, value: &str) -> bool {
    path.exists() && fs::write(path, value).is_ok()
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ───────────────────────────── Batterie ─────────────────────────────

/// État de charge rapporté par le noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Charging,
    Discharging,
    Full,
    NotCharging,
    Unknown,
}

impl Charge {
    fn parse(s: &str) -> Self {
        match s {
            "Charging" => Self::Charging,
            "Discharging" => Self::Discharging,
            "Full" => Self::Full,
            "Not charging" => Self::NotCharging,
            _ => Self::Unknown,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Charging => "charging",
            Self::Discharging => "discharging",
            Self::Full => "full",
            Self::NotCharging => "not-charging",
            Self::Unknown => "unknown",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "En charge",
            Self::Discharging => "Sur batterie",
            Self::Full => "Charg\u{e9}e",
            Self::NotCharging => "Branch\u{e9}e, pas en charge",
            Self::Unknown => "\u{c9}tat inconnu",
        }
    }
}

/// Batterie (ou ensemble de batteries agrégées).
#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub state: Charge,
    /// Autonomie restante estimée (décharge).
    pub seconds_left: Option<u32>,
    /// Temps avant charge complète (charge).
    pub seconds_to_full: Option<u32>,
    /// Autonomie totale sur une charge complète au débit actuel.
    pub seconds_full_life: Option<u32>,
    /// Capacité actuelle par rapport à la capacité d'origine.
    pub health_percent: Option<u8>,
    /// Débit instantané en mW (si connu).
    pub rate_mw: Option<u32>,
    pub count: usize,
}

/// État complet de l'alimentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub ac_online: bool,
    pub battery: Option<Battery>,
}

/// Mesures brutes d'une batterie, toutes en µWh/µW (ou µAh/µA si la tension
/// est inconnue ; l'agrégat reste cohérent tant que l'unité est la même).
#[derive(Debug, Default, Clone, Copy)]
struct Raw {
    now: Option<u64>,
    full: Option<u64>,
    design: Option<u64>,
    rate: Option<u64>,
    capacity: Option<u64>,
}

fn read_raw(dir: &Path) -> Raw {
    let get = |name: &str| read_u64(&dir.join(name));
    let voltage = get("voltage_now").or_else(|| get("voltage_min_design"));
    // Préférer les énergies ; à défaut convertir les charges avec la tension.
    let convert = |charge: Option<u64>| match (charge, voltage) {
        (Some(c), Some(v)) if v > 0 => Some(c.saturating_mul(v) / 1_000_000),
        (c, _) => c,
    };
    let energy = get("energy_now").is_some();
    Raw {
        now: get("energy_now").or_else(|| convert(get("charge_now"))),
        full: get("energy_full").or_else(|| convert(get("charge_full"))),
        design: get("energy_full_design").or_else(|| convert(get("charge_full_design"))),
        rate: if energy {
            get("power_now")
        } else {
            get("power_now").or_else(|| convert(get("current_now")))
        },
        capacity: get("capacity").map(|c| c.min(100)),
    }
}

/// Lit `/sys/class/power_supply` sous `root`.
pub fn read_status_at(root: &Path) -> Status {
    let base = root.join("sys/class/power_supply");
    let mut mains_seen = false;
    let mut mains_online = false;
    let mut states = Vec::new();
    let mut raws = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&base)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    for dir in entries {
        let kind = read_trim(&dir.join("type")).unwrap_or_default();
        // Souris, manettes : batteries de périphériques, pas du système.
        if read_trim(&dir.join("scope")).as_deref() == Some("Device") {
            continue;
        }
        match kind.as_str() {
            "Battery" => {
                if read_trim(&dir.join("present")).as_deref() == Some("0") {
                    continue;
                }
                states.push(Charge::parse(
                    &read_trim(&dir.join("status")).unwrap_or_default(),
                ));
                raws.push(read_raw(&dir));
            }
            "Mains" | "USB" | "USB_C" | "USB_PD" | "USB_PD_DRP" => {
                mains_seen = true;
                if read_trim(&dir.join("online")).as_deref() == Some("1") {
                    mains_online = true;
                }
            }
            _ => {}
        }
    }
    let battery = aggregate(&states, &raws);
    let ac_online = if mains_seen {
        mains_online
    } else {
        // Sans adaptateur exposé : un poste fixe (ou une VM) est sur secteur,
        // une batterie en décharge dit le contraire.
        battery
            .as_ref()
            .is_none_or(|b| b.state != Charge::Discharging)
    };
    Status { ac_online, battery }
}

fn aggregate(states: &[Charge], raws: &[Raw]) -> Option<Battery> {
    if raws.is_empty() {
        return None;
    }
    let state = if states.contains(&Charge::Charging) {
        Charge::Charging
    } else if states.contains(&Charge::Discharging) {
        Charge::Discharging
    } else if states.iter().all(|s| *s == Charge::Full) {
        Charge::Full
    } else if states.contains(&Charge::NotCharging) {
        Charge::NotCharging
    } else {
        Charge::Unknown
    };
    let sum = |f: fn(&Raw) -> Option<u64>| -> Option<u64> {
        raws.iter().map(f).try_fold(0u64, |a, v| v.map(|v| a + v))
    };
    let now = sum(|r| r.now);
    let full = sum(|r| r.full);
    let design = sum(|r| r.design);
    let rate = sum(|r| r.rate).filter(|r| *r > 0);
    let percent = match (now, full) {
        (Some(n), Some(f)) if f > 0 && raws.len() > 1 => (n * 100 / f).min(100),
        _ => {
            let caps: Vec<u64> = raws.iter().filter_map(|r| r.capacity).collect();
            if caps.is_empty() {
                match (now, full) {
                    (Some(n), Some(f)) if f > 0 => (n * 100 / f).min(100),
                    _ => 0,
                }
            } else {
                caps.iter().sum::<u64>() / caps.len() as u64
            }
        }
    } as u8;
    let hours = |energy: u64, rate: u64| -> u32 { (energy.saturating_mul(3600) / rate) as u32 };
    let seconds_left = match (state, now, rate) {
        (Charge::Discharging, Some(n), Some(r)) => Some(hours(n, r)),
        _ => None,
    };
    let seconds_to_full = match (state, now, full, rate) {
        (Charge::Charging, Some(n), Some(f), Some(r)) => Some(hours(f.saturating_sub(n), r)),
        _ => None,
    };
    let seconds_full_life = match (state, full, rate) {
        (Charge::Discharging, Some(f), Some(r)) => Some(hours(f, r)),
        _ => None,
    };
    let health_percent = match (full, design) {
        (Some(f), Some(d)) if d > 0 => Some((f * 100 / d).min(100) as u8),
        _ => None,
    };
    Some(Battery {
        percent,
        state,
        seconds_left,
        seconds_to_full,
        seconds_full_life,
        health_percent,
        rate_mw: rate.map(|r| (r / 1000) as u32),
        count: raws.len(),
    })
}

/// Vrai sous un hyperviseur (drapeau CPU « hypervisor »).
pub fn is_virtual_at(root: &Path) -> bool {
    fs::read_to_string(root.join("proc/cpuinfo")).is_ok_and(|c| {
        c.lines()
            .filter(|l| l.starts_with("flags"))
            .any(|l| l.split_whitespace().any(|f| f == "hypervisor"))
    })
}

pub fn read_status() -> Status {
    read_status_at(&root())
}

/// Durée lisible : « 2 h 05 min ».
pub fn format_duration(secs: u32) -> String {
    let h = secs / 3600;
    let m = secs / 60 % 60;
    if h > 0 {
        format!("{h} h {m:02} min")
    } else {
        format!("{m} min")
    }
}

// ──────────────────────── SYSTEM_POWER_STATUS ────────────────────────

/// Disposition mémoire de `SYSTEM_POWER_STATUS` (12 octets).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemPowerStatus {
    pub ac_line_status: u8,
    pub battery_flag: u8,
    pub battery_life_percent: u8,
    pub system_status_flag: u8,
    pub battery_life_time: u32,
    pub battery_full_life_time: u32,
}

pub const UNKNOWN_TIME: u32 = u32::MAX;

/// Traduction fidèle aux règles documentées de Windows.
pub fn to_system_power_status(s: &Status, saver: bool) -> SystemPowerStatus {
    let mut out = SystemPowerStatus {
        ac_line_status: u8::from(s.ac_online),
        battery_flag: 128, // pas de batterie système
        battery_life_percent: 255,
        system_status_flag: u8::from(saver),
        battery_life_time: UNKNOWN_TIME,
        battery_full_life_time: UNKNOWN_TIME,
    };
    if let Some(b) = &s.battery {
        let mut flag = if b.percent > 66 {
            1
        } else if b.percent < 33 {
            2
        } else {
            0
        };
        if b.percent < 5 {
            flag |= 4;
        }
        if b.state == Charge::Charging {
            flag |= 8;
        }
        out.battery_flag = flag;
        out.battery_life_percent = b.percent.min(100);
        if !s.ac_online {
            out.battery_life_time = b.seconds_left.unwrap_or(UNKNOWN_TIME);
            out.battery_full_life_time = b.seconds_full_life.unwrap_or(UNKNOWN_TIME);
        }
    }
    out
}

// ───────────────────────────── Profils ─────────────────────────────

/// Les trois modes d'alimentation, avec les GUID des schémas Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Performance,
    Balanced,
    Saver,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Self::Performance, Self::Balanced, Self::Saver];

    pub fn key(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Balanced => "balanced",
            Self::Saver => "saver",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Performance => "Performances maximales",
            Self::Balanced => "\u{c9}quilibr\u{e9}",
            Self::Saver => "\u{c9}conomie d'\u{e9}nergie",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == s)
    }
    /// GUID au format mémoire Windows (Data1/2/3 en petit-boutiste).
    pub fn guid(self) -> [u8; 16] {
        let (d1, d2, d3, d4): (u32, u16, u16, [u8; 8]) = match self {
            // SCHEME_MIN : 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
            Self::Performance => (
                0x8c5e_7fda,
                0xe8bf,
                0x4a96,
                [0x9a, 0x85, 0xa6, 0xe2, 0x3a, 0x8c, 0x63, 0x5c],
            ),
            // SCHEME_BALANCED : 381b4222-f694-41f0-9685-ff5bb260df2e
            Self::Balanced => (
                0x381b_4222,
                0xf694,
                0x41f0,
                [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
            ),
            // SCHEME_MAX : a1841308-3541-4fab-bc81-f71556f20b4a
            Self::Saver => (
                0xa184_1308,
                0x3541,
                0x4fab,
                [0xbc, 0x81, 0xf7, 0x15, 0x56, 0xf2, 0x0b, 0x4a],
            ),
        };
        let mut g = [0u8; 16];
        g[..4].copy_from_slice(&d1.to_le_bytes());
        g[4..6].copy_from_slice(&d2.to_le_bytes());
        g[6..8].copy_from_slice(&d3.to_le_bytes());
        g[8..].copy_from_slice(&d4);
        g
    }
    pub fn from_guid(g: &[u8; 16]) -> Option<Self> {
        Self::ALL.into_iter().find(|p| &p.guid() == g)
    }
    /// Gouverneurs cpufreq acceptables, du préféré au moins bon.
    pub fn governors(self) -> &'static [&'static str] {
        match self {
            Self::Performance => &["performance", "schedutil", "ondemand"],
            Self::Balanced => &["schedutil", "ondemand", "conservative", "powersave"],
            Self::Saver => &["powersave", "conservative", "schedutil"],
        }
    }
    /// Préférence énergie/performance des pilotes intel_pstate/amd_pstate.
    pub fn epp(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Balanced => "balance_performance",
            Self::Saver => "power",
        }
    }
    /// Pourcentage appliqué à la luminosité choisie par l'utilisateur.
    pub fn brightness_factor(self) -> u32 {
        match self {
            Self::Saver => 70,
            _ => 100,
        }
    }
    /// Gestion d'énergie des périphériques : (autosuspend USB, ASPM PCIe,
    /// lien SATA, turbo autorisé).
    pub fn devices(self) -> (&'static str, &'static str, &'static str, bool) {
        match self {
            Self::Performance => ("on", "performance", "max_performance", true),
            Self::Balanced => ("auto", "default", "med_power_with_dipm", true),
            Self::Saver => ("auto", "powersupersave", "min_power", false),
        }
    }
}

/// Choisit le premier gouverneur du profil proposé par le pilote.
pub fn pick_governor(profile: Profile, available: &str) -> Option<&'static str> {
    let avail: Vec<&str> = available.split_whitespace().collect();
    profile
        .governors()
        .iter()
        .copied()
        .find(|g| avail.contains(g))
}

// ───────────────────────────── Réglages ─────────────────────────────

/// Action déclenchée par le capot ou le bouton d'alimentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Sleep,
    Hibernate,
    Shutdown,
    DisplayOff,
}

impl Action {
    pub const ALL: [Action; 5] = [
        Self::Nothing,
        Self::Sleep,
        Self::Hibernate,
        Self::Shutdown,
        Self::DisplayOff,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
            Self::Shutdown => "shutdown",
            Self::DisplayOff => "display-off",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Nothing => "Ne rien faire",
            Self::Sleep => "Mettre en veille",
            Self::Hibernate => "Mettre en veille prolong\u{e9}e",
            Self::Shutdown => "Arr\u{ea}ter",
            Self::DisplayOff => "\u{c9}teindre l'\u{e9}cran",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == s)
    }
}

/// Délais proposés dans les Paramètres (secondes, 0 = jamais).
pub const DELAYS: [u32; 9] = [60, 120, 300, 600, 900, 1800, 3600, 7200, 0];

pub fn next_delay(current: u32) -> u32 {
    let i = DELAYS.iter().position(|d| *d == current).unwrap_or(0);
    DELAYS[(i + 1) % DELAYS.len()]
}

pub fn delay_label(secs: u32) -> String {
    match secs {
        0 => "Jamais".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}

/// Contenu de `/etc/winlux/power.conf`.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub profile: Profile,
    pub screen_ac: u32,
    pub screen_dc: u32,
    pub sleep_ac: u32,
    pub sleep_dc: u32,
    pub lid: Action,
    /// `None` : veille sur portable, arrêt sur poste fixe (comme Windows).
    pub button: Option<Action>,
    pub brightness: u32,
    /// Fichier d'échange dédié à l'hibernation, s'il a été préparé.
    pub hibernate_file: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Balanced,
            screen_ac: 600,
            screen_dc: 300,
            sleep_ac: 1800,
            sleep_dc: 900,
            lid: Action::Sleep,
            button: None,
            brightness: 80,
            hibernate_file: None,
        }
    }
}

impl Config {
    pub fn path_at(root: &Path) -> PathBuf {
        root.join("etc/winlux/power.conf")
    }
    pub fn parse(text: &str) -> Self {
        let mut c = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            let delay = || v.parse::<u32>().ok().map(|d| d.min(86_400));
            match k.trim() {
                "PROFILE" => c.profile = Profile::parse(v).unwrap_or(c.profile),
                "SCREEN_AC" => c.screen_ac = delay().unwrap_or(c.screen_ac),
                "SCREEN_DC" => c.screen_dc = delay().unwrap_or(c.screen_dc),
                "SLEEP_AC" => c.sleep_ac = delay().unwrap_or(c.sleep_ac),
                "SLEEP_DC" => c.sleep_dc = delay().unwrap_or(c.sleep_dc),
                "LID" => c.lid = Action::parse(v).unwrap_or(c.lid),
                "BUTTON" => c.button = Action::parse(v).or(c.button),
                "BRIGHTNESS" => {
                    c.brightness = v.parse::<u32>().map_or(c.brightness, |b| b.clamp(5, 100))
                }
                "HIBERNATE_FILE" if !v.is_empty() => c.hibernate_file = Some(v.to_string()),
                _ => {}
            }
        }
        c
    }
    pub fn serialize(&self) -> String {
        let mut s = format!(
            "PROFILE={}\nSCREEN_AC={}\nSCREEN_DC={}\nSLEEP_AC={}\nSLEEP_DC={}\nLID={}\n",
            self.profile.key(),
            self.screen_ac,
            self.screen_dc,
            self.sleep_ac,
            self.sleep_dc,
            self.lid.key(),
        );
        if let Some(b) = self.button {
            s.push_str(&format!("BUTTON={}\n", b.key()));
        }
        s.push_str(&format!("BRIGHTNESS={}\n", self.brightness));
        if let Some(f) = &self.hibernate_file {
            s.push_str(&format!("HIBERNATE_FILE={f}\n"));
        }
        s
    }
    pub fn load_at(root: &Path) -> Self {
        fs::read_to_string(Self::path_at(root))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn load() -> Self {
        Self::load_at(&root())
    }
    pub fn save_at(&self, root: &Path) -> std::io::Result<()> {
        let path = Self::path_at(root);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("conf.{}", std::process::id()));
        fs::write(&tmp, self.serialize())?;
        fs::rename(&tmp, &path)
    }
    pub fn button_action(&self, has_battery: bool) -> Action {
        self.button.unwrap_or(if has_battery {
            Action::Sleep
        } else {
            Action::Shutdown
        })
    }
    /// Délais réellement appliqués (écran, veille) selon la source
    /// d'alimentation ; le mode économie plafonne les délais.
    pub fn effective_timeouts(&self, on_ac: bool) -> (u32, u32) {
        let (screen, sleep) = if on_ac {
            (self.screen_ac, self.sleep_ac)
        } else {
            (self.screen_dc, self.sleep_dc)
        };
        if self.profile != Profile::Saver {
            return (screen, sleep);
        }
        let (cap_screen, cap_sleep) = if on_ac { (300, 900) } else { (120, 300) };
        let cap = |v: u32, c: u32| if v == 0 { c } else { v.min(c) };
        (cap(screen, cap_screen), cap(sleep, cap_sleep))
    }
}

// ───────────────────────────── Application ─────────────────────────────

/// Résultat de l'application d'un profil (pour le journal de Paramètres).
#[derive(Debug, Default)]
pub struct Applied {
    pub governor: Option<&'static str>,
    pub cpus: usize,
    pub brightness: Option<u32>,
    pub devices: usize,
}

fn dir_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Premier rétroéclairage : (dossier, maximum).
pub fn backlight_at(root: &Path) -> Option<(PathBuf, u64)> {
    let dirs = dir_entries(&root.join("sys/class/backlight"));
    // Les pilotes firmware/plateforme sont plus fiables que « raw ».
    let rank = |d: &PathBuf| match read_trim(&d.join("type")).as_deref() {
        Some("firmware") => 0,
        Some("platform") => 1,
        _ => 2,
    };
    let dir = dirs.into_iter().min_by_key(rank)?;
    let max = read_u64(&dir.join("max_brightness")).filter(|m| *m > 0)?;
    Some((dir, max))
}

/// Applique une luminosité en pourcentage ; renvoie la valeur écrite.
pub fn set_brightness_at(root: &Path, percent: u32) -> Option<u32> {
    let (dir, max) = backlight_at(root)?;
    let percent = percent.clamp(1, 100);
    let raw = (max * percent as u64).div_ceil(100).max(1);
    write_existing(&dir.join("brightness"), &raw.to_string()).then_some(percent)
}

pub fn brightness_at(root: &Path) -> Option<u32> {
    let (dir, max) = backlight_at(root)?;
    let cur = read_u64(&dir.join("brightness"))?;
    Some(((cur * 100 + max / 2) / max) as u32)
}

/// Allume ou éteint le rétroéclairage (bl_power, 0 = allumé, 4 = éteint).
pub fn set_backlight_power_at(root: &Path, on: bool) -> bool {
    backlight_at(root)
        .is_some_and(|(dir, _)| write_existing(&dir.join("bl_power"), if on { "0" } else { "4" }))
}

/// Applique le profil : gouverneur, luminosité et périphériques.
pub fn apply_profile_at(root: &Path, config: &Config) -> Applied {
    let profile = config.profile;
    let mut applied = Applied::default();
    let cpu = root.join("sys/devices/system/cpu");
    for dir in dir_entries(&cpu.join("cpufreq")) {
        let available = read_trim(&dir.join("scaling_available_governors")).unwrap_or_default();
        if let Some(g) = pick_governor(profile, &available) {
            if write_existing(&dir.join("scaling_governor"), g) {
                applied.governor = Some(g);
                applied.cpus += 1;
            }
        }
        let epps =
            read_trim(&dir.join("energy_performance_available_preferences")).unwrap_or_default();
        if epps.split_whitespace().any(|e| e == profile.epp()) {
            write_existing(&dir.join("energy_performance_preference"), profile.epp());
        }
    }
    let (usb, aspm, sata, turbo) = profile.devices();
    write_existing(
        &cpu.join("intel_pstate/no_turbo"),
        if turbo { "0" } else { "1" },
    );
    write_existing(&cpu.join("cpufreq/boost"), if turbo { "1" } else { "0" });
    if write_existing(&root.join("sys/module/pcie_aspm/parameters/policy"), aspm) {
        applied.devices += 1;
    }
    for dev in dir_entries(&root.join("sys/bus/usb/devices")) {
        // Les claviers et souris restent actifs pour ne pas perdre de frappe.
        let hid = dir_entries(&dev)
            .iter()
            .any(|i| read_trim(&i.join("bInterfaceClass")).as_deref() == Some("03"));
        let value = if hid { "on" } else { usb };
        if write_existing(&dev.join("power/control"), value) {
            applied.devices += 1;
        }
    }
    for host in dir_entries(&root.join("sys/class/scsi_host")) {
        if write_existing(&host.join("link_power_management_policy"), sata) {
            applied.devices += 1;
        }
    }
    let target = config.brightness * profile.brightness_factor() / 100;
    applied.brightness = set_brightness_at(root, target);
    applied
}

// ───────────────────── Inhibitions (SetThreadExecutionState) ─────────────────────

pub const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
pub const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
pub const ES_AWAYMODE_REQUIRED: u32 = 0x0000_0040;
pub const ES_CONTINUOUS: u32 = 0x8000_0000;

pub fn run_dir_at(root: &Path) -> PathBuf {
    root.join("run/winlux")
}

fn inhibit_dir_at(root: &Path) -> PathBuf {
    run_dir_at(root).join("power-inhibit")
}

/// Enregistre (ou retire si `flags == 0`) une demande d'éveil.
pub fn set_inhibit_at(root: &Path, id: &str, flags: u32) {
    let dir = inhibit_dir_at(root);
    let path = dir.join(id);
    if flags & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED | ES_AWAYMODE_REQUIRED) == 0 {
        let _ = fs::remove_file(path);
        return;
    }
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(path, format!("{flags:#x}\n"));
    }
}

/// Union des demandes d'éveil des processus encore vivants. Les entrées
/// des processus terminés sont supprimées.
pub fn active_inhibits_at(root: &Path) -> u32 {
    let mut flags = 0;
    for path in dir_entries(&inhibit_dir_at(root)) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let pid = name.split('-').next().unwrap_or_default();
        let alive = !pid.is_empty() && root.join("proc").join(pid).exists();
        if !alive {
            let _ = fs::remove_file(&path);
            continue;
        }
        let text = read_trim(&path).unwrap_or_default();
        flags |= u32::from_str_radix(text.trim_start_matches("0x"), 16).unwrap_or(0);
    }
    flags
}

fn activity_path_at(root: &Path) -> PathBuf {
    run_dir_at(root).join("last-input")
}

/// Signale une activité de l'utilisateur (remet à zéro le délai d'inactivité).
pub fn touch_activity_at(root: &Path) {
    let dir = run_dir_at(root);
    if !dir.is_dir() {
        return;
    }
    if let Ok(f) = fs::File::create(activity_path_at(root)) {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// Dernière activité connue (secondes Unix).
pub fn last_activity_at(root: &Path) -> Option<u64> {
    let m = fs::metadata(activity_path_at(root)).ok()?.modified().ok()?;
    m.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

static LAST_NOTE: AtomicU64 = AtomicU64::new(0);

/// Appelé par la boucle X11 à chaque entrée clavier/souris ; limité à une
/// écriture toutes les 5 s.
pub fn note_activity() {
    let now = now_secs();
    let last = LAST_NOTE.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= 5
        && LAST_NOTE
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        touch_activity_at(&root());
    }
}

/// Lignes `power_*` du fichier d'état lu par le bureau. Uniquement de
/// l'ASCII : le bureau affiche ces valeurs avec TextOutA (Latin-1).
pub fn status_lines(status: &Status, config: &Config, root: &Path) -> String {
    let mut s = String::new();
    s.push_str(&format!("power_ac={}\n", u8::from(status.ac_online)));
    match &status.battery {
        Some(b) => {
            s.push_str(&format!(
                "power_battery={}\npower_percent={}\npower_state={}\n",
                b.count,
                b.percent,
                b.state.key()
            ));
            if let Some(t) = b.seconds_left {
                s.push_str(&format!("power_seconds_left={t}\n"));
            }
            if let Some(t) = b.seconds_to_full {
                s.push_str(&format!("power_seconds_to_full={t}\n"));
            }
            if let Some(h) = b.health_percent {
                s.push_str(&format!("power_health={h}\n"));
            }
        }
        None => s.push_str("power_battery=0\n"),
    }
    let (screen, sleep_delay) = config.effective_timeouts(status.ac_online);
    s.push_str(&format!(
        "power_profile={}\npower_screen_ac={}\npower_screen_dc={}\npower_sleep_ac={}\npower_sleep_dc={}\n",
        config.profile.key(),
        config.screen_ac,
        config.screen_dc,
        config.sleep_ac,
        config.sleep_dc
    ));
    let vm_no_sleep = status.battery.is_none() && is_virtual_at(root);
    s.push_str(&format!(
        "power_screen_now={}\npower_sleep_now={}\n",
        delay_label(screen),
        if vm_no_sleep {
            "Jamais (machine virtuelle)".to_string()
        } else {
            delay_label(sleep_delay)
        }
    ));
    s.push_str(&format!(
        "power_lid={}\npower_button={}\n",
        config.lid.key(),
        config.button_action(status.battery.is_some()).key()
    ));
    match brightness_at(root) {
        Some(b) => s.push_str(&format!("power_brightness={b}\n")),
        None => s.push_str("power_brightness=\n"),
    }
    s.push_str(&format!("power_brightness_pref={}\n", config.brightness));
    let caps = sleep::capabilities_at(root);
    s.push_str(&format!(
        "power_can_sleep={}\npower_can_hibernate={}\n",
        u8::from(caps.suspend),
        u8::from(caps.hibernate)
    ));
    s
}

#[cfg(test)]
mod tests;
