//! Détection des cartes graphiques et de ce que leur pilote expose réellement.
//!
//! Garde-fou central : chaque information ou réglage n'est publié que si le
//! pilote le fournit sur cette machine. Les Paramètres n'affichent que les
//! clés présentes dans le rapport : jamais de section grisée ni de valeur
//! inventée. Sources : sysfs (`amdgpu`, `i915`, `xe`, `nouveau`, hwmon) et
//! NVML (`libnvidia-ml.so.1`) quand le pilote propriétaire NVIDIA est chargé.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    Amd,
    Nvidia,
    Other,
}

impl Vendor {
    fn from_id(id: u32) -> Self {
        match id {
            0x8086 => Vendor::Intel,
            0x1002 => Vendor::Amd,
            0x10de => Vendor::Nvidia,
            _ => Vendor::Other,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Vendor::Intel => "intel",
            Vendor::Amd => "amd",
            Vendor::Nvidia => "nvidia",
            Vendor::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// GPU intégré au processeur (économie d'énergie).
    Integrated,
    /// GPU dédié (hautes performances).
    Dedicated,
    /// Machine à un seul GPU, ou rôle impossible à établir.
    Single,
}

impl Role {
    fn key(self) -> &'static str {
        match self {
            Role::Integrated => "integrated",
            Role::Dedicated => "dedicated",
            Role::Single => "single",
        }
    }
}

/// Plage d'overclocking du cœur, au format `pp_od_clk_voltage` des puces
/// AMD RDNA1/RDNA2 (deux états, sans tension). Les autres formats (Vega,
/// Polaris, décalage RDNA3) ne sont pas reconnus et le réglage est masqué.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OdRange {
    pub current_max_mhz: u32,
    pub min_mhz: u32,
    pub max_mhz: u32,
}

/// Limite de puissance réglable, en watts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerLimit {
    pub current_w: u32,
    pub min_w: u32,
    pub max_w: u32,
    pub default_w: Option<u32>,
}

/// Relevé instantané : `None` signifie « non exposé par le pilote ».
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    pub load_pct: Option<u32>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
    pub temp_c: Option<u32>,
    pub freq_mhz: Option<u32>,
    pub freq_max_mhz: Option<u32>,
    pub power_w: Option<u32>,
    pub encoder_pct: Option<u32>,
    pub decoder_pct: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gpu {
    /// Nom sysfs (`card0`), identifiant stable pour les commandes.
    pub card: String,
    pub pci_slot: String,
    pub vendor: Vendor,
    pub vendor_id: u32,
    pub device_id: u32,
    pub driver: String,
    pub name: String,
    pub role: Role,
    pub boot_vga: bool,
    pub reading: Reading,
    pub power_limit: Option<PowerLimit>,
    /// Niveau `power_dpm_force_performance_level` courant, si modifiable.
    pub perf_level: Option<String>,
    pub overclock: Option<OdRange>,
    device: PathBuf,
}

fn read_trim(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read_u64(path: &Path) -> Option<u64> {
    read_trim(path)?.parse().ok()
}

fn parse_hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

/// Un réglage n'est proposé que si ce processus peut réellement l'écrire.
fn writable(path: &Path) -> bool {
    #[cfg(test)]
    if tests::ASSUME_WRITABLE.with(|w| w.get()) {
        return path.exists();
    }
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    // SAFETY: chaîne C valide pendant l'appel.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

fn uevent_field(device: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(device.join("uevent")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(str::to_owned)
}

fn hwmon_dir(device: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(device.join("hwmon"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("hwmon"))
        })
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

/// Nom commercial depuis la base `pci.ids`, sinon un libellé honnête.
fn device_name(vendor: Vendor, vendor_id: u32, device_id: u32, pci_ids: Option<&str>) -> String {
    if let Some(name) = pci_ids.and_then(|db| lookup_pci_ids(db, vendor_id, device_id)) {
        return name;
    }
    let brand = match vendor {
        Vendor::Intel => "Intel Graphics",
        Vendor::Amd => "AMD Radeon",
        Vendor::Nvidia => "NVIDIA GeForce",
        Vendor::Other => "Adaptateur graphique",
    };
    format!("{brand} ({vendor_id:04x}:{device_id:04x})")
}

/// Cherche `device` sous `vendor` dans le format texte de `pci.ids`.
pub fn lookup_pci_ids(db: &str, vendor_id: u32, device_id: u32) -> Option<String> {
    let vendor = format!("{vendor_id:04x}  ");
    let device = format!("\t{device_id:04x}  ");
    let mut in_vendor = false;
    for line in db.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if in_vendor {
                return None;
            }
            in_vendor = line.to_ascii_lowercase().starts_with(&vendor);
        } else if in_vendor && line.to_ascii_lowercase().starts_with(&device) {
            let name = line[device.len()..].trim();
            // Les noms longs « Navi 21 [Radeon RX 6800/6800 XT] » : garder
            // la partie commerciale entre crochets quand elle existe.
            let short = match (name.find('['), name.rfind(']')) {
                (Some(a), Some(b)) if b > a + 1 => &name[a + 1..b],
                _ => name,
            };
            return Some(ascii(short));
        }
    }
    None
}

/// Le rapport est lu par des applications `TextOutA` : uniquement de l'ASCII
/// imprimable, sans `=` ni retour à la ligne.
fn ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            ' '..='~' if c != '=' => c,
            _ => '?',
        })
        .take(80)
        .collect()
}

fn pci_ids(root: &Path) -> Option<String> {
    for rel in ["usr/share/misc/pci.ids", "usr/share/hwdata/pci.ids"] {
        if let Ok(text) = std::fs::read_to_string(root.join(rel)) {
            return Some(text);
        }
    }
    None
}

/// Analyse `pp_od_clk_voltage` (format RDNA1/RDNA2 uniquement).
pub fn parse_od(text: &str) -> Option<OdRange> {
    let mut section = "";
    let mut state1 = None;
    let mut range = None;
    let mhz = |s: &str| -> Option<u32> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        lower.strip_suffix("mhz")?.trim().parse().ok()
    };
    for line in text.lines() {
        let line = line.trim();
        if line.ends_with(':') {
            section = line;
            continue;
        }
        match section {
            "OD_SCLK:" => {
                let (index, value) = line.split_once(':')?;
                // Une tension (`mV`) signale l'ancien format à 8 états.
                if value.to_ascii_lowercase().contains("mv") {
                    return None;
                }
                if index.trim() == "1" {
                    state1 = Some(mhz(value)?);
                }
            }
            "OD_RANGE:" => {
                if let Some(rest) = line.strip_prefix("SCLK:") {
                    let mut parts = rest.split_whitespace();
                    let min = mhz(parts.next()?)?;
                    let max = mhz(parts.next()?)?;
                    range = Some((min, max));
                }
            }
            _ => {}
        }
    }
    let current = state1?;
    let (min, max) = range?;
    (min < max && (min..=max).contains(&current)).then_some(OdRange {
        current_max_mhz: current,
        min_mhz: min,
        max_mhz: max,
    })
}

fn micro_to_w(v: u64) -> u32 {
    (v / 1_000_000) as u32
}

fn sysfs_reading(card_dir: &Path, device: &Path, hwmon: Option<&Path>) -> Reading {
    let mut r = Reading {
        load_pct: read_u64(&device.join("gpu_busy_percent")).map(|v| v.min(100) as u32),
        ..Reading::default()
    };
    let used = read_u64(&device.join("mem_info_vram_used"));
    let total = read_u64(&device.join("mem_info_vram_total"));
    if let (Some(used), Some(total)) = (used, total) {
        if total > 0 {
            r.vram_used_mb = Some(used / (1024 * 1024));
            r.vram_total_mb = Some(total / (1024 * 1024));
        }
    }
    if let Some(h) = hwmon {
        r.temp_c = ["temp1_input", "temp2_input"]
            .iter()
            .find_map(|f| read_u64(&h.join(f)))
            .map(|m| (m / 1000) as u32);
        r.freq_mhz = read_u64(&h.join("freq1_input")).map(|hz| (hz / 1_000_000) as u32);
        r.power_w = read_u64(&h.join("power1_average"))
            .or_else(|| read_u64(&h.join("power1_input")))
            .map(micro_to_w);
    }
    // i915 (ancienne et nouvelle arborescence), puis xe.
    let intel_cur = [
        card_dir.join("gt_act_freq_mhz"),
        card_dir.join("gt_cur_freq_mhz"),
        card_dir.join("gt/gt0/rps_act_freq_mhz"),
        device.join("tile0/gt0/freq0/act_freq"),
        device.join("tile0/gt0/freq0/cur_freq"),
    ];
    if r.freq_mhz.is_none() {
        r.freq_mhz = intel_cur.iter().find_map(|p| read_u64(p)).map(|v| v as u32);
    }
    let intel_max = [
        card_dir.join("gt_RP0_freq_mhz"),
        card_dir.join("gt/gt0/rps_RP0_freq_mhz"),
        device.join("tile0/gt0/freq0/rp0_freq"),
    ];
    r.freq_max_mhz = intel_max.iter().find_map(|p| read_u64(p)).map(|v| v as u32);
    r
}

fn sysfs_power_limit(hwmon: &Path) -> Option<PowerLimit> {
    let cap = hwmon.join("power1_cap");
    if !writable(&cap) {
        return None;
    }
    let current = micro_to_w(read_u64(&cap)?);
    let min = micro_to_w(read_u64(&hwmon.join("power1_cap_min"))?);
    let max = micro_to_w(read_u64(&hwmon.join("power1_cap_max"))?);
    let default = read_u64(&hwmon.join("power1_cap_default")).map(micro_to_w);
    (min < max && max > 0).then_some(PowerLimit {
        current_w: current.clamp(min, max),
        min_w: min,
        max_w: max,
        default_w: default.filter(|d| (min..=max).contains(d)),
    })
}

const PERF_LEVELS: [&str; 3] = ["auto", "low", "high"];

fn sysfs_perf_level(device: &Path) -> Option<String> {
    let path = device.join("power_dpm_force_performance_level");
    if !writable(&path) {
        return None;
    }
    let level = read_trim(&path)?;
    // « manual » ou « profile_* » viennent d'un autre outil : on montre le
    // réglage comme « auto » non sélectionné plutôt que de le masquer.
    Some(level)
}

fn sysfs_overclock(device: &Path) -> Option<OdRange> {
    let path = device.join("pp_od_clk_voltage");
    if !writable(&path) {
        return None;
    }
    parse_od(&std::fs::read_to_string(path).ok()?)
}

fn is_card_name(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Énumère les GPU PCI sous `root` (normalement `/`).
pub fn probe(root: &Path) -> Vec<Gpu> {
    let drm = root.join("sys/class/drm");
    let mut cards: Vec<String> = std::fs::read_dir(&drm)
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                .filter(|n| is_card_name(n))
                .collect()
        })
        .unwrap_or_default();
    cards.sort_by_key(|c| c[4..].parse::<u32>().unwrap_or(u32::MAX));
    let ids = pci_ids(root);
    let mut gpus = Vec::new();
    for card in cards {
        let card_dir = drm.join(&card);
        let device = card_dir.join("device");
        // Les cartes sans identifiant PCI (simpledrm, framebuffer EFI) ne
        // sont pas de vrais GPU : rien à afficher ni à régler.
        let Some(vendor_id) = read_trim(&device.join("vendor")).and_then(|v| parse_hex(&v)) else {
            continue;
        };
        let device_id = read_trim(&device.join("device"))
            .and_then(|v| parse_hex(&v))
            .unwrap_or(0);
        let slot = uevent_field(&device, "PCI_SLOT_NAME").unwrap_or_default();
        // Une même puce peut exposer plusieurs nœuds `cardN` : un seul GPU.
        if !slot.is_empty() && gpus.iter().any(|g: &Gpu| g.pci_slot == slot) {
            continue;
        }
        let driver = uevent_field(&device, "DRIVER")
            .or_else(|| {
                std::fs::read_link(device.join("driver"))
                    .ok()
                    .and_then(|p| p.file_name()?.to_str().map(str::to_owned))
            })
            .unwrap_or_default();
        let vendor = Vendor::from_id(vendor_id);
        let hwmon = hwmon_dir(&device);
        let amd = driver == "amdgpu";
        let product = read_trim(&device.join("product_name")).map(|n| ascii(&n));
        gpus.push(Gpu {
            name: product
                .unwrap_or_else(|| device_name(vendor, vendor_id, device_id, ids.as_deref())),
            reading: sysfs_reading(&card_dir, &device, hwmon.as_deref()),
            power_limit: if amd {
                hwmon.as_deref().and_then(sysfs_power_limit)
            } else {
                None
            },
            perf_level: if amd { sysfs_perf_level(&device) } else { None },
            overclock: if amd { sysfs_overclock(&device) } else { None },
            boot_vga: read_trim(&device.join("boot_vga")).as_deref() == Some("1"),
            card,
            pci_slot: ascii(&slot),
            vendor,
            vendor_id,
            device_id,
            driver: ascii(&driver),
            role: Role::Single,
            device,
        });
    }
    if gpus.iter().any(|g| g.driver == "nvidia") {
        nvml::fill(&mut gpus);
    }
    classify(&mut gpus);
    gpus
}

/// Relit uniquement les capteurs (appelé périodiquement par l'overlay).
pub fn refresh(gpu: &mut Gpu) {
    if gpu.driver == "nvidia" {
        let mut one = [gpu.clone()];
        nvml::fill(&mut one);
        gpu.reading = one[0].reading;
        return;
    }
    let card_dir = gpu
        .device
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let hwmon = hwmon_dir(&gpu.device);
    gpu.reading = sysfs_reading(&card_dir, &gpu.device, hwmon.as_deref());
}

fn integrated_candidate(g: &Gpu) -> bool {
    // Les iGPU Intel sont toujours au 00:02.0 ; les Arc dédiées ailleurs.
    g.vendor == Vendor::Intel && g.pci_slot.ends_with("00:02.0")
}

/// Établit le rôle intégré/dédié. Sans couple clair, tout reste `Single` et
/// la sélection de GPU par application est masquée.
pub fn classify(gpus: &mut [Gpu]) {
    for g in gpus.iter_mut() {
        g.role = Role::Single;
    }
    let real: Vec<usize> = (0..gpus.len())
        .filter(|&i| gpus[i].vendor != Vendor::Other)
        .collect();
    if real.len() < 2 {
        return;
    }
    let integrated = real
        .iter()
        .copied()
        .find(|&i| integrated_candidate(&gpus[i]))
        .or_else(|| {
            // AMD + NVIDIA : l'APU AMD est l'intégré.
            let has_nvidia = real.iter().any(|&i| gpus[i].vendor == Vendor::Nvidia);
            real.iter()
                .copied()
                .find(|&i| has_nvidia && gpus[i].vendor == Vendor::Amd)
        })
        .or_else(|| {
            // Deux AMD : la plus petite mémoire vidéo est l'APU.
            let amds: Vec<usize> = real
                .iter()
                .copied()
                .filter(|&i| gpus[i].vendor == Vendor::Amd)
                .collect();
            if amds.len() != real.len() {
                return None;
            }
            let vram = |i: usize| gpus[i].reading.vram_total_mb;
            let smallest = amds.iter().copied().min_by_key(|&i| vram(i))?;
            let clearly_smaller = amds
                .iter()
                .all(|&i| {
                    i == smallest
                        || matches!((vram(i), vram(smallest)), (Some(a), Some(b)) if a > b.saturating_mul(2))
                });
            clearly_smaller.then_some(smallest)
        });
    let Some(integrated) = integrated else {
        return;
    };
    for &i in &real {
        gpus[i].role = if i == integrated {
            Role::Integrated
        } else {
            Role::Dedicated
        };
    }
}

/// Vrai si l'on peut choisir le GPU d'une application.
pub fn hybrid(gpus: &[Gpu]) -> bool {
    gpus.iter().any(|g| g.role == Role::Integrated)
        && gpus.iter().any(|g| g.role == Role::Dedicated)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Auto,
    PowerSaving,
    Performance,
}

impl Preference {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Preference::Auto),
            "integrated" | "power-saving" => Some(Preference::PowerSaving),
            "dedicated" | "performance" => Some(Preference::Performance),
            _ => None,
        }
    }
}

/// Variables d'environnement qui orientent Mesa/Vulkan/GLX vers le GPU
/// demandé. Vide sur une machine qui n'est pas hybride : rien à changer.
pub fn prime_env(gpus: &[Gpu], pref: Preference) -> Vec<(&'static str, String)> {
    if !hybrid(gpus) || pref == Preference::Auto {
        return Vec::new();
    }
    let wanted = if pref == Preference::Performance {
        Role::Dedicated
    } else {
        Role::Integrated
    };
    let Some(target) = gpus.iter().find(|g| g.role == wanted) else {
        return Vec::new();
    };
    let has_nvidia_blob = gpus.iter().any(|g| g.driver == "nvidia");
    let mut env = Vec::new();
    if target.driver == "nvidia" {
        env.push(("__NV_PRIME_RENDER_OFFLOAD", "1".to_owned()));
        env.push(("__NV_PRIME_RENDER_OFFLOAD_PROVIDER", "NVIDIA-G0".to_owned()));
        env.push(("__GLX_VENDOR_LIBRARY_NAME", "nvidia".to_owned()));
        env.push(("__VK_LAYER_NV_optimus", "NVIDIA_only".to_owned()));
    } else {
        // Mesa accepte l'adresse PCI : sans ambiguïté sur les machines à
        // plus de deux GPU, contrairement à `DRI_PRIME=1`.
        let slot = target.pci_slot.replace([':', '.'], "_");
        let value = if slot.is_empty() {
            if wanted == Role::Dedicated { "1" } else { "0" }.to_owned()
        } else {
            format!("pci-{slot}")
        };
        env.push(("DRI_PRIME", value));
        if has_nvidia_blob {
            env.push(("__VK_LAYER_NV_optimus", "non_NVIDIA_only".to_owned()));
            env.push(("__GLX_VENDOR_LIBRARY_NAME", "mesa".to_owned()));
        }
    }
    env
}

/// Rapport `clé=valeur` lu par le bureau. Une clé absente signifie
/// « non disponible sur cette machine » : la ligne correspondante est masquée.
pub fn report(gpus: &[Gpu]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "gpu_count={}", gpus.len());
    let _ = writeln!(out, "gpu_hybrid={}", u8::from(hybrid(gpus)));
    for (i, g) in gpus.iter().enumerate() {
        let mut kv = |k: &str, v: &dyn std::fmt::Display| {
            let _ = writeln!(out, "gpu{i}_{k}={v}");
        };
        kv("card", &g.card);
        kv("name", &g.name);
        kv("vendor", &g.vendor.key());
        kv("driver", &g.driver);
        kv("role", &g.role.key());
        kv("slot", &g.pci_slot);
        let r = &g.reading;
        if let Some(v) = r.load_pct {
            kv("load", &v);
        }
        if let (Some(u), Some(t)) = (r.vram_used_mb, r.vram_total_mb) {
            kv("vram_used_mb", &u);
            kv("vram_total_mb", &t);
        }
        if let Some(v) = r.temp_c {
            kv("temp_c", &v);
        }
        if let Some(v) = r.freq_mhz {
            kv("freq_mhz", &v);
        }
        if let Some(v) = r.freq_max_mhz {
            kv("freq_max_mhz", &v);
        }
        if let Some(v) = r.power_w {
            kv("power_w", &v);
        }
        if let Some(v) = r.encoder_pct {
            kv("encoder", &v);
        }
        if let Some(v) = r.decoder_pct {
            kv("decoder", &v);
        }
        if let Some(p) = g.power_limit {
            kv("power_limit_w", &p.current_w);
            kv("power_limit_min_w", &p.min_w);
            kv("power_limit_max_w", &p.max_w);
            if let Some(d) = p.default_w {
                kv("power_limit_default_w", &d);
            }
        }
        if let Some(level) = &g.perf_level {
            kv("perf_level", &ascii(level));
        }
        if let Some(od) = g.overclock {
            kv("oc_sclk_mhz", &od.current_max_mhz);
            kv("oc_sclk_min_mhz", &od.min_mhz);
            kv("oc_sclk_max_mhz", &od.max_mhz);
        }
    }
    out
}

/// Racine système, modifiable pour les tests.
pub fn system_root() -> PathBuf {
    std::env::var_os("WINLUX_SYSFS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Applique un réglage après l'avoir validé contre ce que le pilote expose.
/// Utilisé par `winlux --gpu-set` (appelé par `winlux-control`).
pub fn apply(root: &Path, card: &str, setting: &str, value: &str) -> Result<String, String> {
    let gpus = probe(root);
    let gpu = gpus
        .iter()
        .find(|g| g.card == card)
        .ok_or_else(|| format!("GPU {card} introuvable"))?;
    match setting {
        "power-limit" => {
            let limit = gpu
                .power_limit
                .ok_or("Ce GPU ne permet pas de modifier sa limite de puissance")?;
            let watts = if value == "default" {
                limit.default_w.ok_or("Aucune valeur par défaut exposée")?
            } else {
                value.parse::<u32>().map_err(|_| "Valeur invalide")?
            };
            if !(limit.min_w..=limit.max_w).contains(&watts) {
                return Err(format!(
                    "Limite hors plage ({}-{} W)",
                    limit.min_w, limit.max_w
                ));
            }
            if gpu.driver == "nvidia" {
                nvml::set_power_limit(&gpu.pci_slot, watts)?;
            } else {
                let hwmon = hwmon_dir(&gpu.device).ok_or("Capteurs absents")?;
                write(
                    &hwmon.join("power1_cap"),
                    &format!("{}", watts as u64 * 1_000_000),
                )?;
            }
            Ok(format!("Limite de puissance : {watts} W"))
        }
        "perf" => {
            gpu.perf_level
                .as_ref()
                .ok_or("Ce GPU ne propose pas de profil de performances")?;
            if !PERF_LEVELS.contains(&value) {
                return Err("Profil inconnu".into());
            }
            write(&gpu.device.join("power_dpm_force_performance_level"), value)?;
            Ok(format!("Profil de performances : {value}"))
        }
        "oc-sclk" => {
            let od = gpu.overclock.ok_or("Ce GPU ne permet pas l'overclocking")?;
            let mhz = value.parse::<u32>().map_err(|_| "Valeur invalide")?;
            if !(od.min_mhz..=od.max_mhz).contains(&mhz) {
                return Err(format!(
                    "Fréquence hors plage ({}-{} MHz)",
                    od.min_mhz, od.max_mhz
                ));
            }
            let path = gpu.device.join("pp_od_clk_voltage");
            write(&path, &format!("s 1 {mhz}"))?;
            write(&path, "c")?;
            Ok(format!("Fréquence maximale : {mhz} MHz"))
        }
        "oc-reset" => {
            gpu.overclock.ok_or("Ce GPU ne permet pas l'overclocking")?;
            let path = gpu.device.join("pp_od_clk_voltage");
            write(&path, "r")?;
            write(&path, "c")?;
            Ok("Fréquences d'usine rétablies".into())
        }
        _ => Err(format!("Réglage inconnu : {setting}")),
    }
}

fn write(path: &Path, value: &str) -> Result<(), String> {
    std::fs::write(path, format!("{value}\n"))
        .map_err(|e| format!("Écriture refusée par le pilote ({}) : {e}", path.display()))
}

/// NVML, chargé à la demande. Absent → aucune information NVIDIA, sans erreur.
mod nvml {
    use super::{Gpu, PowerLimit};
    use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
    use std::sync::OnceLock;

    type Device = *mut c_void;
    type GetU32 = unsafe extern "C" fn(Device, *mut c_uint) -> c_int;
    type GetU32Arg = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> c_int;
    type GetPair = unsafe extern "C" fn(Device, *mut c_uint, *mut c_uint) -> c_int;
    type ByBus = unsafe extern "C" fn(*const c_char, *mut Device) -> c_int;
    type GetName = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> c_int;
    type GetUtil = unsafe extern "C" fn(Device, *mut Utilization) -> c_int;
    type GetMem = unsafe extern "C" fn(Device, *mut Memory) -> c_int;
    type SetU32 = unsafe extern "C" fn(Device, c_uint) -> c_int;
    type Init = unsafe extern "C" fn() -> c_int;

    #[repr(C)]
    struct Utilization {
        gpu: c_uint,
        memory: c_uint,
    }
    #[repr(C)]
    struct Memory {
        total: u64,
        free: u64,
        used: u64,
    }

    struct Api {
        by_bus: ByBus,
        name: GetName,
        utilization: GetUtil,
        memory: GetMem,
        temperature: GetU32Arg,
        clock: GetU32Arg,
        max_clock: GetU32Arg,
        encoder: GetPair,
        decoder: GetPair,
        power: GetU32,
        limit: GetU32,
        default_limit: GetU32,
        constraints: GetPair,
        set_limit: SetU32,
    }
    // SAFETY: NVML est documentée comme sûre entre threads.
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| unsafe { load() }).as_ref()
    }

    unsafe fn load() -> Option<Api> {
        if std::env::var_os("WINLUX_NO_NVML").is_some() {
            return None;
        }
        let lib = libc::dlopen(
            c"libnvidia-ml.so.1".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        if lib.is_null() {
            return None;
        }
        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let p = libc::dlsym(lib, concat!($name, "\0").as_ptr().cast());
                if p.is_null() {
                    return None;
                }
                core::mem::transmute::<*mut c_void, $ty>(p)
            }};
        }
        let init = sym!("nvmlInit_v2", Init);
        if init() != 0 {
            return None;
        }
        Some(Api {
            by_bus: sym!("nvmlDeviceGetHandleByPciBusId_v2", ByBus),
            name: sym!("nvmlDeviceGetName", GetName),
            utilization: sym!("nvmlDeviceGetUtilizationRates", GetUtil),
            memory: sym!("nvmlDeviceGetMemoryInfo", GetMem),
            temperature: sym!("nvmlDeviceGetTemperature", GetU32Arg),
            clock: sym!("nvmlDeviceGetClockInfo", GetU32Arg),
            max_clock: sym!("nvmlDeviceGetMaxClockInfo", GetU32Arg),
            encoder: sym!("nvmlDeviceGetEncoderUtilization", GetPair),
            decoder: sym!("nvmlDeviceGetDecoderUtilization", GetPair),
            power: sym!("nvmlDeviceGetPowerUsage", GetU32),
            limit: sym!("nvmlDeviceGetPowerManagementLimit", GetU32),
            default_limit: sym!("nvmlDeviceGetPowerManagementDefaultLimit", GetU32),
            constraints: sym!("nvmlDeviceGetPowerManagementLimitConstraints", GetPair),
            set_limit: sym!("nvmlDeviceSetPowerManagementLimit", SetU32),
        })
    }

    fn device(api: &Api, slot: &str) -> Option<Device> {
        let bus = CString::new(slot).ok()?;
        let mut dev: Device = std::ptr::null_mut();
        // SAFETY: pointeurs valides ; NVML initialisée par `load`.
        (unsafe { (api.by_bus)(bus.as_ptr(), &mut dev) } == 0 && !dev.is_null()).then_some(dev)
    }

    fn one(f: GetU32, dev: Device) -> Option<u32> {
        let mut v = 0;
        // SAFETY: appel NVML avec un pointeur de sortie valide.
        (unsafe { f(dev, &mut v) } == 0).then_some(v)
    }
    fn arg(f: GetU32Arg, dev: Device, a: c_uint) -> Option<u32> {
        let mut v = 0;
        // SAFETY: idem.
        (unsafe { f(dev, a, &mut v) } == 0).then_some(v)
    }
    fn pair(f: GetPair, dev: Device) -> Option<(u32, u32)> {
        let (mut a, mut b) = (0, 0);
        // SAFETY: idem.
        (unsafe { f(dev, &mut a, &mut b) } == 0).then_some((a, b))
    }

    pub fn fill(gpus: &mut [Gpu]) {
        let Some(api) = api() else {
            return;
        };
        for g in gpus.iter_mut().filter(|g| g.driver == "nvidia") {
            let Some(dev) = device(api, &g.pci_slot) else {
                continue;
            };
            let mut name = [0 as c_char; 96];
            // SAFETY: tampon de 96 octets annoncé comme tel.
            if unsafe { (api.name)(dev, name.as_mut_ptr(), name.len() as c_uint) } == 0 {
                // SAFETY: NVML termine la chaîne par NUL.
                let text = unsafe { CStr::from_ptr(name.as_ptr()) }.to_string_lossy();
                if !text.is_empty() {
                    g.name = super::ascii(&text);
                }
            }
            let r = &mut g.reading;
            let mut u = Utilization { gpu: 0, memory: 0 };
            // SAFETY: structure `nvmlUtilization_t` de même disposition.
            if unsafe { (api.utilization)(dev, &mut u) } == 0 {
                r.load_pct = Some(u.gpu.min(100));
            }
            let mut m = Memory {
                total: 0,
                free: 0,
                used: 0,
            };
            // SAFETY: structure `nvmlMemory_t` (v1) de même disposition.
            if unsafe { (api.memory)(dev, &mut m) } == 0 && m.total > 0 {
                r.vram_used_mb = Some(m.used / (1024 * 1024));
                r.vram_total_mb = Some(m.total / (1024 * 1024));
            }
            r.temp_c = arg(api.temperature, dev, 0);
            r.freq_mhz = arg(api.clock, dev, 0);
            r.freq_max_mhz = arg(api.max_clock, dev, 0);
            r.encoder_pct = pair(api.encoder, dev).map(|(v, _)| v.min(100));
            r.decoder_pct = pair(api.decoder, dev).map(|(v, _)| v.min(100));
            r.power_w = one(api.power, dev).map(|mw| mw / 1000);
            // La modification exige root : sinon le réglage reste masqué.
            // SAFETY: geteuid n'a pas de précondition.
            let root = unsafe { libc::geteuid() } == 0;
            g.power_limit = match (root, one(api.limit, dev), pair(api.constraints, dev)) {
                (true, Some(cur), Some((min, max))) if min < max => Some(PowerLimit {
                    current_w: cur / 1000,
                    min_w: min / 1000,
                    max_w: max / 1000,
                    default_w: one(api.default_limit, dev).map(|d| d / 1000),
                }),
                _ => None,
            };
        }
    }

    pub fn set_power_limit(slot: &str, watts: u32) -> Result<(), String> {
        let api = api().ok_or("NVML indisponible")?;
        let dev = device(api, slot).ok_or("GPU NVIDIA introuvable via NVML")?;
        // SAFETY: handle obtenu de NVML.
        match unsafe { (api.set_limit)(dev, watts * 1000) } {
            0 => Ok(()),
            code => Err(format!("NVML a refusé la limite (code {code})")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(PathBuf);
    impl Fake {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "winlux-gpu-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("sys/class/drm")).unwrap();
            Fake(dir)
        }
        fn put(&self, rel: &str, text: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        fn card(&self, card: &str, vendor: &str, device: &str, driver: &str, slot: &str) {
            let d = format!("sys/class/drm/{card}/device");
            self.put(&format!("{d}/vendor"), &format!("{vendor}\n"));
            self.put(&format!("{d}/device"), &format!("{device}\n"));
            self.put(
                &format!("{d}/uevent"),
                &format!("DRIVER={driver}\nPCI_SLOT_NAME={slot}\n"),
            );
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    thread_local! {
        pub static ASSUME_WRITABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn with_writable<T>(f: impl FnOnce() -> T) -> T {
        // Les tests ne tournent pas forcément en root.
        ASSUME_WRITABLE.with(|w| w.set(true));
        std::env::set_var("WINLUX_NO_NVML", "1");
        f()
    }

    #[test]
    fn machine_sans_gpu_pci_ne_publie_rien() {
        let fs = Fake::new("vide");
        // simpledrm : pas d'identifiant PCI, ignoré.
        fs.put(
            "sys/class/drm/card0/device/uevent",
            "DRIVER=simple-framebuffer\n",
        );
        fs.put("sys/class/drm/card0-Virtual-1/status", "connected\n");
        let gpus = with_writable(|| probe(&fs.0));
        assert!(gpus.is_empty());
        assert_eq!(report(&gpus), "gpu_count=0\ngpu_hybrid=0\n");
    }

    #[test]
    fn gpu_virtuel_unique_sans_capteurs_ni_reglages() {
        let fs = Fake::new("vm");
        fs.card("card0", "0x1af4", "0x1050", "virtio-pci", "0000:00:01.0");
        let gpus = with_writable(|| probe(&fs.0));
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].role, Role::Single);
        let text = report(&gpus);
        assert!(text.contains("gpu0_name=Adaptateur graphique (1af4:1050)"));
        for hidden in [
            "load", "vram", "temp", "freq", "power", "perf", "oc_", "encoder",
        ] {
            assert!(
                !text.contains(&format!("gpu0_{hidden}")),
                "{hidden} affiché"
            );
        }
        assert!(prime_env(&gpus, Preference::Performance).is_empty());
    }

    #[test]
    fn amdgpu_complet_publie_capteurs_et_reglages() {
        let fs = Fake::new("amd");
        fs.card("card0", "0x1002", "0x73bf", "amdgpu", "0000:03:00.0");
        let d = "sys/class/drm/card0/device";
        fs.put(&format!("{d}/gpu_busy_percent"), "37\n");
        fs.put(&format!("{d}/mem_info_vram_used"), "1073741824\n");
        fs.put(&format!("{d}/mem_info_vram_total"), "17179869184\n");
        fs.put(&format!("{d}/hwmon/hwmon3/temp1_input"), "56000\n");
        fs.put(&format!("{d}/hwmon/hwmon3/freq1_input"), "2100000000\n");
        fs.put(&format!("{d}/hwmon/hwmon3/power1_average"), "145000000\n");
        fs.put(&format!("{d}/hwmon/hwmon3/power1_cap"), "255000000\n");
        fs.put(&format!("{d}/hwmon/hwmon3/power1_cap_min"), "0\n");
        fs.put(&format!("{d}/hwmon/hwmon3/power1_cap_max"), "293000000\n");
        fs.put(
            &format!("{d}/hwmon/hwmon3/power1_cap_default"),
            "255000000\n",
        );
        fs.put(&format!("{d}/power_dpm_force_performance_level"), "auto\n");
        fs.put(
            &format!("{d}/pp_od_clk_voltage"),
            "OD_SCLK:\n0: 500Mhz\n1: 2250Mhz\nOD_MCLK:\n1: 1000MHz\nOD_RANGE:\nSCLK:     500Mhz       2800Mhz\nMCLK:     674Mhz       1200Mhz\n",
        );
        fs.put(
            "usr/share/misc/pci.ids",
            "1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73bf  Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]\n10de  NVIDIA Corporation\n",
        );
        let gpus = with_writable(|| probe(&fs.0));
        let text = report(&gpus);
        for line in [
            "gpu0_name=Radeon RX 6800/6800 XT / 6900 XT",
            "gpu0_load=37",
            "gpu0_vram_used_mb=1024",
            "gpu0_vram_total_mb=16384",
            "gpu0_temp_c=56",
            "gpu0_freq_mhz=2100",
            "gpu0_power_w=145",
            "gpu0_power_limit_w=255",
            "gpu0_power_limit_min_w=0",
            "gpu0_power_limit_max_w=293",
            "gpu0_power_limit_default_w=255",
            "gpu0_perf_level=auto",
            "gpu0_oc_sclk_mhz=2250",
            "gpu0_oc_sclk_max_mhz=2800",
            "gpu0_role=single",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "manque {line} dans\n{text}"
            );
        }
        // Pas d'encodeur exposé par sysfs : masqué.
        assert!(!text.contains("encoder"));

        let root = fs.0.clone();
        with_writable(|| {
            assert!(apply(&root, "card0", "power-limit", "400").is_err());
            assert!(apply(&root, "card0", "power-limit", "abc").is_err());
            apply(&root, "card0", "power-limit", "200").unwrap();
            apply(&root, "card0", "perf", "high").unwrap();
            assert!(apply(&root, "card0", "perf", "turbo; rm").is_err());
            assert!(apply(&root, "card0", "oc-sclk", "9000").is_err());
            apply(&root, "card0", "oc-sclk", "2400").unwrap();
            assert!(apply(&root, "card9", "perf", "auto").is_err());
        });
        let hw = fs.0.join(d).join("hwmon/hwmon3/power1_cap");
        assert_eq!(std::fs::read_to_string(hw).unwrap(), "200000000\n");
        let perf = fs.0.join(d).join("power_dpm_force_performance_level");
        assert_eq!(std::fs::read_to_string(perf).unwrap(), "high\n");
        // Le dernier mot écrit est la validation `c`.
        let od = fs.0.join(d).join("pp_od_clk_voltage");
        assert_eq!(std::fs::read_to_string(od).unwrap(), "c\n");
    }

    #[test]
    fn reglages_masques_si_non_inscriptibles() {
        let fs = Fake::new("ro");
        fs.card("card0", "0x1002", "0x73bf", "amdgpu", "0000:03:00.0");
        let d = "sys/class/drm/card0/device";
        fs.put(&format!("{d}/hwmon/hwmon0/power1_cap"), "255000000\n");
        fs.put(&format!("{d}/hwmon/hwmon0/power1_cap_min"), "0\n");
        fs.put(&format!("{d}/hwmon/hwmon0/power1_cap_max"), "293000000\n");
        fs.put(&format!("{d}/power_dpm_force_performance_level"), "auto\n");
        for f in [
            "hwmon/hwmon0/power1_cap",
            "power_dpm_force_performance_level",
        ] {
            let p = fs.0.join(d).join(f);
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o444);
            std::fs::set_permissions(&p, perm).unwrap();
        }
        std::env::set_var("WINLUX_NO_NVML", "1");
        // root ignore les permissions : le test n'a de sens qu'en utilisateur.
        if unsafe { libc::geteuid() } != 0 {
            let gpus = probe(&fs.0);
            assert!(gpus[0].power_limit.is_none());
            assert!(gpus[0].perf_level.is_none());
        }
    }

    #[test]
    fn overclocking_ancien_format_masque() {
        assert!(parse_od(
            "OD_SCLK:\n0: 300MHz 750mV\n1: 600MHz 769mV\nOD_RANGE:\nSCLK: 300MHz 2000MHz\n"
        )
        .is_none());
        assert!(
            parse_od("OD_SCLK_OFFSET:\n0Mhz\nOD_RANGE:\nSCLK_OFFSET: -500Mhz 1000Mhz\n").is_none()
        );
        assert!(parse_od("").is_none());
        assert_eq!(
            parse_od("OD_SCLK:\n0: 800Mhz\n1: 1900Mhz\nOD_RANGE:\nSCLK: 800Mhz 2150Mhz\n"),
            Some(OdRange {
                current_max_mhz: 1900,
                min_mhz: 800,
                max_mhz: 2150
            })
        );
    }

    #[test]
    fn portable_intel_nvidia_hybride() {
        let fs = Fake::new("hybride");
        fs.card("card0", "0x8086", "0x9a49", "i915", "0000:00:02.0");
        fs.put("sys/class/drm/card0/gt_act_freq_mhz", "350\n");
        fs.put("sys/class/drm/card0/gt_RP0_freq_mhz", "1350\n");
        fs.put("sys/class/drm/card0/device/boot_vga", "1\n");
        fs.card("card1", "0x10de", "0x2520", "nvidia", "0000:01:00.0");
        fs.put("sys/class/drm/renderD128/dev", "226:128\n");
        let gpus = with_writable(|| probe(&fs.0));
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].role, Role::Integrated);
        assert_eq!(gpus[1].role, Role::Dedicated);
        assert!(gpus[0].boot_vga);
        let text = report(&gpus);
        assert!(text.contains("gpu_hybrid=1\n"));
        assert!(text.contains("gpu0_freq_mhz=350\n"));
        assert!(text.contains("gpu0_freq_max_mhz=1350\n"));
        // i915 n'expose ni charge ni VRAM : masquées.
        assert!(!text.contains("gpu0_load"));
        assert!(!text.contains("gpu0_vram"));
        // Sans NVML : aucune valeur NVIDIA inventée.
        assert!(!text.contains("gpu1_load"));

        let perf = prime_env(&gpus, Preference::Performance);
        assert!(perf.contains(&("__NV_PRIME_RENDER_OFFLOAD", "1".into())));
        assert!(perf.contains(&("__GLX_VENDOR_LIBRARY_NAME", "nvidia".into())));
        let eco = prime_env(&gpus, Preference::PowerSaving);
        assert!(eco.contains(&("DRI_PRIME", "pci-0000_00_02_0".into())));
        assert!(eco.contains(&("__VK_LAYER_NV_optimus", "non_NVIDIA_only".into())));
        assert!(prime_env(&gpus, Preference::Auto).is_empty());
    }

    #[test]
    fn roles_amd_et_cas_ambigus() {
        let fs = Fake::new("amdamd");
        fs.card("card0", "0x1002", "0x1681", "amdgpu", "0000:05:00.0");
        fs.put("sys/class/drm/card0/device/mem_info_vram_used", "0\n");
        fs.put(
            "sys/class/drm/card0/device/mem_info_vram_total",
            "536870912\n",
        );
        fs.card("card1", "0x1002", "0x73df", "amdgpu", "0000:03:00.0");
        fs.put("sys/class/drm/card1/device/mem_info_vram_used", "0\n");
        fs.put(
            "sys/class/drm/card1/device/mem_info_vram_total",
            "12884901888\n",
        );
        let gpus = with_writable(|| probe(&fs.0));
        assert_eq!(gpus[0].role, Role::Integrated);
        assert_eq!(gpus[1].role, Role::Dedicated);
        let env = prime_env(&gpus, Preference::Performance);
        assert_eq!(env, vec![("DRI_PRIME", "pci-0000_03_00_0".to_owned())]);

        // Deux cartes dédiées identiques : aucun rôle, sélection masquée.
        let fs2 = Fake::new("deuxnv");
        fs2.card("card0", "0x10de", "0x2684", "nvidia", "0000:01:00.0");
        fs2.card("card1", "0x10de", "0x2684", "nvidia", "0000:02:00.0");
        let gpus = with_writable(|| probe(&fs2.0));
        assert!(!hybrid(&gpus));
        assert!(prime_env(&gpus, Preference::Performance).is_empty());
    }

    #[test]
    fn base_pci_ids() {
        let db = "# commentaire\n8086  Intel Corporation\n\t9a49  TigerLake-LP GT2 [Iris Xe Graphics]\n\t9a60  TigerLake-H GT1 [UHD Graphics]\n10de  NVIDIA Corporation\n\t2520  GA106M [GeForce RTX 3060 Mobile / Max-Q]\n";
        assert_eq!(
            lookup_pci_ids(db, 0x8086, 0x9a49).as_deref(),
            Some("Iris Xe Graphics")
        );
        assert_eq!(
            lookup_pci_ids(db, 0x10de, 0x2520).as_deref(),
            Some("GeForce RTX 3060 Mobile / Max-Q")
        );
        assert_eq!(lookup_pci_ids(db, 0x10de, 0x9a49), None);
        assert_eq!(lookup_pci_ids(db, 0x1002, 0x1234), None);
    }

    #[test]
    fn preferences_reconnues() {
        assert_eq!(
            Preference::parse("dedicated"),
            Some(Preference::Performance)
        );
        assert_eq!(
            Preference::parse("power-saving"),
            Some(Preference::PowerSaving)
        );
        assert_eq!(Preference::parse("auto"), Some(Preference::Auto));
        assert_eq!(Preference::parse("$(id)"), None);
    }
}
