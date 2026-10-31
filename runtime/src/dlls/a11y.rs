//! Réglages d'accessibilité partagés par le bureau Winlux et les `.exe`.
//!
//! Le bureau (page Paramètres > Accessibilité) écrit
//! `C:\.local\share\winlux\accessibility.ini` (dans le dossier personnel).
//! Chaque processus Winlux relit ce fichier quand il change : les couleurs
//! système des programmes Win32 suivent le thème de contraste, et le
//! pointeur X11 suit la taille et la couleur choisies.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// Réglages lus par le runtime (le bureau en lit davantage lui-même).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// 0 = aucun, 1 = Aquatique, 2 = Désert, 3 = Crépuscule, 4 = Ciel nocturne.
    pub contrast: u8,
    /// Multiplicateur de taille du pointeur, de 1 à 5.
    pub pointer_size: u8,
    /// 0 = blanc, 1 = noir, 2 = vert vif.
    pub pointer_color: u8,
    /// Épaisseur du curseur de texte en pixels (1 à 6).
    pub caret_width: u8,
    /// Animations de l'interface.
    pub animations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            contrast: 0,
            pointer_size: 1,
            pointer_color: 0,
            caret_width: 1,
            animations: true,
        }
    }
}

fn number(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

/// Analyse le fichier `clé=valeur` ; les valeurs hors bornes sont ignorées.
pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(n) = number(value) else {
            continue;
        };
        match key.trim() {
            "contrast" if n <= 4 => s.contrast = n as u8,
            "pointer_size" if (1..=5).contains(&n) => s.pointer_size = n as u8,
            "pointer_color" if n <= 2 => s.pointer_color = n as u8,
            "caret_width" if (1..=6).contains(&n) => s.caret_width = n as u8,
            "animations" if n <= 1 => s.animations = n == 1,
            _ => {}
        }
    }
    s
}

/// Emplacement du fichier partagé avec le bureau.
pub fn path() -> PathBuf {
    crate::paths::drive_c().join(".local/share/winlux/accessibility.ini")
}

struct Cache {
    checked: Option<Instant>,
    modified: Option<SystemTime>,
    settings: Settings,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    checked: None,
    modified: None,
    settings: Settings {
        contrast: 0,
        pointer_size: 1,
        pointer_color: 0,
        caret_width: 1,
        animations: true,
    },
});

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

/// Palette d'un thème de contraste (COLORREF 0x00BBGGRR).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContrastColors {
    pub background: u32,
    pub text: u32,
    pub hyperlink: u32,
    pub disabled: u32,
    pub selected_bg: u32,
    pub selected_text: u32,
    pub button_bg: u32,
    pub button_text: u32,
}

const fn bgr(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

/// Les quatre thèmes de contraste de Windows 11, mêmes teintes que le bureau.
pub fn contrast_colors(theme: u8) -> Option<ContrastColors> {
    Some(match theme {
        // Aquatique : blanc sur noir, liens cyan.
        1 => ContrastColors {
            background: bgr(0x202020),
            text: bgr(0xFFFFFF),
            hyperlink: bgr(0x75E9FC),
            disabled: bgr(0xA6A6A6),
            selected_bg: bgr(0x8EE3F0),
            selected_text: bgr(0x263B50),
            button_bg: bgr(0x202020),
            button_text: bgr(0xFFFFFF),
        },
        // Désert : noir sur blanc cassé.
        2 => ContrastColors {
            background: bgr(0xFFFAEF),
            text: bgr(0x3D3D3D),
            hyperlink: bgr(0x1C5E75),
            disabled: bgr(0x676767),
            selected_bg: bgr(0x903909),
            selected_text: bgr(0xFFF5E3),
            button_bg: bgr(0xFFFAEF),
            button_text: bgr(0x202020),
        },
        // Crépuscule : texte turquoise sur ardoise.
        3 => ContrastColors {
            background: bgr(0x2D3236),
            text: bgr(0xB6F6F0),
            hyperlink: bgr(0x70EBDE),
            disabled: bgr(0xBDBDBD),
            selected_bg: bgr(0xA1BFDE),
            selected_text: bgr(0x212D3B),
            button_bg: bgr(0x2D3236),
            button_text: bgr(0xB6F6F0),
        },
        // Ciel nocturne : blanc sur noir, accents violets.
        4 => ContrastColors {
            background: bgr(0x000000),
            text: bgr(0xFFFFFF),
            hyperlink: bgr(0x8080FF),
            disabled: bgr(0xD6B4FD),
            selected_bg: bgr(0xD6B4FD),
            selected_text: bgr(0x2B2B2B),
            button_bg: bgr(0x000000),
            button_text: bgr(0xFFFFFF),
        },
        _ => return None,
    })
}

/// Couleur `GetSysColor` imposée par le thème de contraste actif.
pub fn system_color(theme: u8, index: i32) -> Option<u32> {
    let c = contrast_colors(theme)?;
    Some(match index {
        // Fonds : bureau, fenêtres, menus, info-bulles, légendes inactives.
        1 | 3 | 4 | 5 | 11 | 12 | 24 | 28 | 30 => c.background,
        15 => c.button_bg,
        // Textes et bordures pleines.
        0 | 6 | 7 | 8 | 9 | 10 | 16 | 20 | 21 | 22 | 23 => c.text,
        18 => c.button_text,
        13 | 29 | 2 | 27 => c.selected_bg,
        14 => c.selected_text,
        17 | 19 => c.disabled,
        26 => c.hyperlink,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lecture_des_reglages_et_bornes() {
        let s = parse("contrast=3\npointer_size=4\npointer_color=2\ncaret_width=5\nanimations=0\n");
        assert_eq!(
            s,
            Settings {
                contrast: 3,
                pointer_size: 4,
                pointer_color: 2,
                caret_width: 5,
                animations: false,
            }
        );
        // Valeurs invalides : on garde les valeurs par défaut.
        let s = parse("contrast=9\npointer_size=0\ncaret_width=x\nbruit\n");
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn couleurs_systeme_en_contraste_eleve() {
        assert_eq!(system_color(0, 5), None);
        // Aquatique : fenêtre sombre, texte blanc (COLORREF).
        assert_eq!(system_color(1, 5), Some(0x0020_2020));
        assert_eq!(system_color(1, 8), Some(0x00FF_FFFF));
        // Désert : fond clair, sélection brune lisible.
        assert_eq!(system_color(2, 5), Some(0x00EF_FAFF));
        assert_eq!(system_color(2, 13), Some(0x0009_3990));
        for theme in 1..=4 {
            let c = contrast_colors(theme).unwrap();
            assert_ne!(c.background, c.text, "thème {theme} lisible");
        }
    }
}
