//! Réglages de la souris partagés par Paramètres et les `.exe`.
//!
//! Paramètres > Clavier, souris et pavé tactile (winlux-mouse) écrit
//! `~/.local/share/winlux/mouse.ini` : vitesse du double-clic, lignes par cran
//! de molette, boutons inversés (gaucher), traînée et vitesse du pointeur.
//! Les fonctions Win32 correspondantes (GetDoubleClickTime,
//! SystemParametersInfo SPI_GETWHEELSCROLLLINES, GetSystemMetrics
//! SM_SWAPBUTTON…) lisent ce fichier, relu quand il change.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Délai du double-clic en millisecondes (200 à 900).
    pub double_click: u32,
    /// Lignes défilées par cran de molette (1 à 100).
    pub wheel_lines: u32,
    /// Bouton principal à droite.
    pub swap_buttons: bool,
    /// Longueur de la traînée du pointeur (0 : aucune, 2 à 7).
    pub trails: u32,
    /// Vitesse du pointeur, de 1 à 20 (10 par défaut), comme SPI_GETMOUSESPEED.
    pub speed: u32,
}

pub const DEFAULT: Settings = Settings {
    double_click: 500,
    wheel_lines: 3,
    swap_buttons: false,
    trails: 0,
    speed: 10,
};

impl Default for Settings {
    fn default() -> Self {
        DEFAULT
    }
}

/// Analyse le fichier `clé=valeur` ; les valeurs hors bornes sont ignorées.
pub fn parse(text: &str) -> Settings {
    let mut s = DEFAULT;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Ok(n) = value.trim().parse::<u32>() else {
            continue;
        };
        match key.trim() {
            "double_click" if (200..=900).contains(&n) => s.double_click = n,
            "wheel_lines" if (1..=100).contains(&n) => s.wheel_lines = n,
            "swap_buttons" if n <= 1 => s.swap_buttons = n == 1,
            "trails" if n <= 7 && n != 1 => s.trails = n,
            "speed" if (1..=20).contains(&n) => s.speed = n,
            _ => {}
        }
    }
    s
}

pub fn path() -> PathBuf {
    crate::paths::drive_c().join(".local/share/winlux/mouse.ini")
}

struct Cache {
    checked: Option<Instant>,
    modified: Option<SystemTime>,
    settings: Settings,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    checked: None,
    modified: None,
    settings: DEFAULT,
});

/// Délai de double-clic fixé par SetDoubleClickTime : il vaut pour ce
/// programme seulement (0 = celui de Paramètres > Souris).
static DOUBLE_CLICK_OVERRIDE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_double_click_override(ms: u32) {
    DOUBLE_CLICK_OVERRIDE.store(ms, std::sync::atomic::Ordering::Relaxed);
}

/// Délai de double-clic en vigueur pour ce programme.
pub fn double_click_time() -> u32 {
    match DOUBLE_CLICK_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => current().double_click,
        ms => ms,
    }
}

/// Réglages courants, relus au plus deux fois par seconde.
pub fn current() -> Settings {
    let mut cache = match CACHE.lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    let now = Instant::now();
    if cache
        .checked
        .is_some_and(|t| now.duration_since(t) < Duration::from_millis(500))
    {
        return cache.settings;
    }
    cache.checked = Some(now);
    let file = path();
    let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    if modified != cache.modified {
        cache.modified = modified;
        cache.settings = std::fs::read_to_string(&file)
            .map(|t| parse(&t))
            .unwrap_or_default();
    }
    cache.settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_windows() {
        assert_eq!(parse(""), DEFAULT);
        assert_eq!(DEFAULT.double_click, 500);
        assert_eq!(DEFAULT.wheel_lines, 3);
    }

    #[test]
    fn reads_values_and_rejects_out_of_range() {
        let s = parse(
            "# Winlux\ndouble_click=350\nwheel_lines=8\nswap_buttons=1\ntrails=5\nspeed=14\n",
        );
        assert_eq!(
            s,
            Settings {
                double_click: 350,
                wheel_lines: 8,
                swap_buttons: true,
                trails: 5,
                speed: 14
            }
        );
        let s = parse("double_click=50\nwheel_lines=0\nswap_buttons=2\ntrails=1\nspeed=99\n");
        assert_eq!(s, DEFAULT);
    }
}
