//! Overlay de performances en jeu : images/s, temps d'image, charge CPU/GPU.
//!
//! Réglé uniquement depuis Paramètres > Graphismes (fichier
//! `/etc/winlux/overlay.conf` écrit par `winlux-control`). Désactivé par
//! défaut ; une fois activé, le raccourci choisi l'affiche ou le masque dans
//! la fenêtre du programme. Le bureau Winlux lui-même n'est jamais concerné.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const VK_F12: usize = 0x7B;
const VK_O: usize = 0x4F;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    ShiftF12,
    CtrlShiftO,
    AltF12,
}

impl Hotkey {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "shift-f12" => Some(Hotkey::ShiftF12),
            "ctrl-shift-o" => Some(Hotkey::CtrlShiftO),
            "alt-f12" => Some(Hotkey::AltF12),
            _ => None,
        }
    }
    pub fn matches(self, vk: usize, ctrl: bool, shift: bool, alt: bool) -> bool {
        match self {
            Hotkey::ShiftF12 => vk == VK_F12 && shift && !ctrl && !alt,
            Hotkey::CtrlShiftO => vk == VK_O && ctrl && shift && !alt,
            Hotkey::AltF12 => vk == VK_F12 && alt && !ctrl && !shift,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub hotkey: Hotkey,
    pub position: Position,
    /// Afficher dès l'ouverture de la fenêtre, sans attendre le raccourci.
    pub autoshow: bool,
    pub fps: bool,
    pub frametime: bool,
    pub cpu: bool,
    pub gpu: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: false,
            hotkey: Hotkey::ShiftF12,
            position: Position::TopLeft,
            autoshow: false,
            fps: true,
            frametime: true,
            cpu: true,
            gpu: true,
        }
    }
}

pub fn parse(text: &str) -> Config {
    let mut c = Config::default();
    for line in text.lines().take(64) {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let flag = match value {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
        match (key, flag) {
            ("OVERLAY_ENABLED", Some(f)) => c.enabled = f,
            ("OVERLAY_AUTOSHOW", Some(f)) => c.autoshow = f,
            ("OVERLAY_FPS", Some(f)) => c.fps = f,
            ("OVERLAY_FRAMETIME", Some(f)) => c.frametime = f,
            ("OVERLAY_CPU", Some(f)) => c.cpu = f,
            ("OVERLAY_GPU", Some(f)) => c.gpu = f,
            ("OVERLAY_HOTKEY", _) => {
                if let Some(h) = Hotkey::parse(value) {
                    c.hotkey = h;
                }
            }
            ("OVERLAY_POSITION", _) => {
                c.position = match value {
                    "top-right" => Position::TopRight,
                    "bottom-left" => Position::BottomLeft,
                    "bottom-right" => Position::BottomRight,
                    "top-left" => Position::TopLeft,
                    _ => c.position,
                }
            }
            _ => {}
        }
    }
    c
}

fn config_path() -> PathBuf {
    std::env::var_os("WINLUX_OVERLAY_CONF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/winlux/overlay.conf"))
}

/// Images récentes d'une fenêtre : 2 s de profondeur au plus.
#[derive(Default)]
pub struct FrameClock {
    frames: VecDeque<Instant>,
}

impl FrameClock {
    pub fn tick(&mut self, now: Instant) {
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2))
            || self.frames.len() > 1000
        {
            self.frames.pop_front();
        }
    }
    /// Images affichées pendant la dernière seconde.
    pub fn fps(&self, now: Instant) -> u32 {
        self.frames
            .iter()
            .filter(|t| now.duration_since(**t) <= Duration::from_secs(1))
            .count() as u32
    }
    /// Temps moyen entre les dernières images (ms, une décimale).
    pub fn frametime_ms(&self) -> Option<f32> {
        let n = self.frames.len().min(30);
        if n < 2 {
            return None;
        }
        let first = self.frames[self.frames.len() - n];
        let last = *self.frames.back()?;
        Some(last.duration_since(first).as_secs_f32() * 1000.0 / (n - 1) as f32)
    }
}

/// Charge CPU globale entre deux lectures de `/proc/stat` (en %).
pub fn cpu_percent(prev: (u64, u64), stat: &str) -> Option<((u64, u64), u32)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }
    let total: u64 = values.iter().sum();
    // idle + iowait
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let (pt, pi) = prev;
    let dt = total.saturating_sub(pt);
    let di = idle.saturating_sub(pi);
    let pct = if dt == 0 || pt == 0 {
        0
    } else {
        ((dt - di.min(dt)) * 100 / dt) as u32
    };
    Some(((total, idle), pct))
}

pub struct Overlay {
    config: Config,
    config_mtime: Option<SystemTime>,
    last_config_check: Option<Instant>,
    visible: bool,
    allowed: bool,
    clocks: HashMap<usize, FrameClock>,
    cpu_prev: (u64, u64),
    cpu_pct: Option<u32>,
    gpu: Option<crate::gpu::Gpu>,
    gpu_probed: bool,
    last_sample: Option<Instant>,
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay {
    pub fn new() -> Self {
        // Le bureau Winlux et ses applications ne sont pas des jeux.
        let allowed = std::env::var_os("WINLUX_DESKTOP_DEMO").is_none()
            && std::env::var_os("WINLUX_SHELL_APP").is_none();
        let mut o = Overlay {
            config: Config::default(),
            config_mtime: None,
            last_config_check: None,
            visible: false,
            allowed,
            clocks: HashMap::new(),
            cpu_prev: (0, 0),
            cpu_pct: None,
            gpu: None,
            gpu_probed: false,
            last_sample: None,
        };
        o.reload(Instant::now());
        o.visible = o.config.enabled && o.config.autoshow;
        o
    }

    fn reload(&mut self, now: Instant) {
        if !self.allowed {
            return;
        }
        if self
            .last_config_check
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(2))
        {
            return;
        }
        self.last_config_check = Some(now);
        let path = config_path();
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if mtime == self.config_mtime {
            return;
        }
        self.config_mtime = mtime;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let was = self.config.enabled;
        self.config = parse(&text);
        if !self.config.enabled {
            self.visible = false;
        } else if !was && self.config.autoshow {
            self.visible = true;
        }
    }

    /// Le raccourci a été pressé : `true` si la touche est consommée.
    pub fn hotkey(&mut self, vk: usize, ctrl: bool, shift: bool, alt: bool) -> bool {
        self.reload(Instant::now());
        if !self.allowed
            || !self.config.enabled
            || !self.config.hotkey.matches(vk, ctrl, shift, alt)
        {
            return false;
        }
        self.visible = !self.visible;
        true
    }

    pub fn frame(&mut self, hwnd: usize) {
        if !self.allowed {
            return;
        }
        let now = Instant::now();
        self.clocks.entry(hwnd).or_default().tick(now);
        self.reload(now);
    }

    pub fn forget(&mut self, hwnd: usize) {
        self.clocks.remove(&hwnd);
    }

    pub fn position(&self) -> Position {
        self.config.position
    }

    fn sample(&mut self, now: Instant) {
        if self
            .last_sample
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1))
        {
            return;
        }
        self.last_sample = Some(now);
        if self.config.cpu {
            if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
                if let Some((prev, pct)) = cpu_percent(self.cpu_prev, &stat) {
                    let first = self.cpu_prev == (0, 0);
                    self.cpu_prev = prev;
                    self.cpu_pct = (!first).then_some(pct);
                }
            }
        }
        if self.config.gpu {
            if !self.gpu_probed {
                self.gpu_probed = true;
                let gpus = crate::gpu::probe(&crate::gpu::system_root());
                // Le GPU qui rend le jeu : le dédié s'il existe.
                self.gpu = gpus
                    .iter()
                    .find(|g| g.role == crate::gpu::Role::Dedicated)
                    .or_else(|| gpus.iter().find(|g| g.reading.load_pct.is_some()))
                    .cloned();
            } else if let Some(g) = self.gpu.as_mut() {
                crate::gpu::refresh(g);
            }
        }
    }

    /// Lignes à dessiner pour cette fenêtre ; vide si l'overlay est masqué.
    /// Une mesure que la machine ne fournit pas n'apparaît pas.
    pub fn lines(&mut self, hwnd: usize) -> Vec<String> {
        if !self.allowed || !self.visible || !self.config.enabled {
            return Vec::new();
        }
        let now = Instant::now();
        self.sample(now);
        let mut out = Vec::new();
        let clock = self.clocks.get(&hwnd);
        if self.config.fps {
            out.push(format!("FPS     {}", clock.map_or(0, |c| c.fps(now))));
        }
        if self.config.frametime {
            if let Some(ms) = clock.and_then(FrameClock::frametime_ms) {
                out.push(format!("Image   {ms:.1} ms"));
            }
        }
        if self.config.cpu {
            if let Some(p) = self.cpu_pct {
                out.push(format!("CPU     {p} %"));
            }
        }
        if self.config.gpu {
            if let Some(g) = &self.gpu {
                if let Some(p) = g.reading.load_pct {
                    out.push(format!("GPU     {p} %"));
                }
                if let Some(t) = g.reading.temp_c {
                    out.push(format!("GPU     {t} °C"));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_bornee_et_par_defaut_desactivee() {
        assert!(!parse("").enabled);
        let c = parse(
            "OVERLAY_ENABLED=1\nOVERLAY_HOTKEY=ctrl-shift-o\nOVERLAY_POSITION=bottom-right\nOVERLAY_CPU=0\nOVERLAY_AUTOSHOW=1\n",
        );
        assert!(c.enabled && c.autoshow && !c.cpu && c.fps && c.gpu);
        assert_eq!(c.hotkey, Hotkey::CtrlShiftO);
        assert_eq!(c.position, Position::BottomRight);
        let bad = parse("OVERLAY_ENABLED=oui\nOVERLAY_HOTKEY=$(reboot)\nOVERLAY_POSITION=x\n");
        assert_eq!(bad, Config::default());
    }

    #[test]
    fn raccourcis_exacts() {
        assert!(Hotkey::ShiftF12.matches(VK_F12, false, true, false));
        assert!(!Hotkey::ShiftF12.matches(VK_F12, true, true, false));
        assert!(!Hotkey::ShiftF12.matches(VK_F12, false, false, false));
        assert!(Hotkey::CtrlShiftO.matches(VK_O, true, true, false));
        assert!(Hotkey::AltF12.matches(VK_F12, false, false, true));
        assert!(!Hotkey::AltF12.matches(VK_O, false, false, true));
    }

    #[test]
    fn images_par_seconde_et_temps_image() {
        let mut c = FrameClock::default();
        let t0 = Instant::now();
        assert_eq!(c.frametime_ms(), None);
        for i in 0..=60u64 {
            c.tick(t0 + Duration::from_micros(i * 16_667));
        }
        let now = t0 + Duration::from_millis(1000);
        assert!((59..=61).contains(&c.fps(now)), "{}", c.fps(now));
        let ms = c.frametime_ms().unwrap();
        assert!((16.0..17.5).contains(&ms), "{ms}");
        // Les images anciennes sont oubliées.
        c.tick(t0 + Duration::from_secs(10));
        assert_eq!(c.fps(t0 + Duration::from_secs(10)), 1);
    }

    #[test]
    fn charge_cpu_depuis_proc_stat() {
        let (prev, _) = cpu_percent((0, 0), "cpu  100 0 100 800 0 0 0 0 0 0\n").unwrap();
        let (_, pct) = cpu_percent(prev, "cpu  150 0 150 900 0 0 0 0 0 0\ncpu0 1 1 1 1\n").unwrap();
        assert_eq!(pct, 50);
        assert!(cpu_percent((0, 0), "intr 1 2 3\n").is_none());
    }

    #[test]
    fn masque_tant_que_non_active() {
        let mut o = Overlay {
            config: Config::default(),
            config_mtime: None,
            last_config_check: Some(Instant::now()),
            visible: true,
            allowed: true,
            clocks: HashMap::new(),
            cpu_prev: (0, 0),
            cpu_pct: None,
            gpu: None,
            gpu_probed: true,
            last_sample: None,
        };
        assert!(o.lines(1).is_empty());
        assert!(!o.hotkey(VK_F12, false, true, false));
        o.config.enabled = true;
        o.config.cpu = false;
        o.config.gpu = false;
        o.frame(1);
        assert_eq!(o.lines(1), vec!["FPS     1".to_owned()]);
        assert!(o.hotkey(VK_F12, false, true, false));
        assert!(o.lines(1).is_empty());
        // Le bureau n'affiche jamais l'overlay.
        o.allowed = false;
        assert!(!o.hotkey(VK_F12, false, true, false));
    }
}
