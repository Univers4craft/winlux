//! Disposition clavier vue par les .exe : celle de Linux (Paramètres,
//! /etc/default/keyboard), traduite en identifiant Windows (KLID, HKL), et
//! touches mortes (^, ¨, `, ´, ~) composées comme sous Windows.

use std::sync::OnceLock;

/// Disposition XKB → (KLID Windows, langue de saisie).
fn klid_of(layout: &str, variant: &str) -> Option<(u32, u16)> {
    Some(match (layout, variant) {
        ("fr", "bepo" | "bepo_afnor") => (0x0002_040C, 0x040C),
        ("fr", _) => (0x0000_040C, 0x040C),
        ("be", _) => (0x0000_080C, 0x080C),
        ("ch", "fr" | "fr_nodeadkeys" | "fr_mac") => (0x0000_100C, 0x100C),
        ("ch", _) => (0x0000_0807, 0x0807),
        ("ca", "multix") => (0x0001_1009, 0x0C0C),
        ("ca", "eng") => (0x0000_0409, 0x1009),
        ("ca", _) => (0x0000_1009, 0x0C0C),
        ("us", "intl" | "altgr-intl") => (0x0002_0409, 0x0409),
        ("us", "dvorak") => (0x0001_0409, 0x0409),
        ("us", _) => (0x0000_0409, 0x0409),
        ("gb", _) => (0x0000_0809, 0x0809),
        ("de", _) => (0x0000_0407, 0x0407),
        ("at", _) => (0x0000_0407, 0x0C07),
        ("es", _) => (0x0000_040A, 0x0C0A),
        ("latam", _) => (0x0000_080A, 0x080A),
        ("it", _) => (0x0000_0410, 0x0410),
        ("pt", _) => (0x0000_0816, 0x0816),
        ("br", _) => (0x0001_0416, 0x0416),
        _ => return None,
    })
}

/// Disposition active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Identifiant de disposition (« 0000040C » pour l'AZERTY français).
    pub klid: u32,
    /// Langue de saisie (mot bas du HKL).
    pub lang: u16,
}

impl Layout {
    /// HKL Windows : langue de saisie en mot bas, disposition en mot haut.
    pub fn hkl(&self) -> usize {
        ((self.klid as usize & 0xFFFF) << 16) | self.lang as usize
    }
    pub fn name(&self) -> String {
        format!("{:08X}", self.klid)
    }
}

/// Disposition d'après les réglages XKB (première de la liste « fr,us »).
pub fn from_xkb(layout: &str, variant: &str) -> Option<Layout> {
    let layout = layout
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let variant = variant
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    klid_of(&layout, &variant).map(|(klid, lang)| Layout { klid, lang })
}

fn conf(path: &str, key: &str) -> Option<String> {
    let root = std::env::var("WINLUX_CONTROL_ROOT").unwrap_or_default();
    let text = std::fs::read_to_string(format!("{root}{path}")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key)
            .then(|| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    })
}

/// Disposition du système : Paramètres (`/etc/winlux/input.conf`), puis
/// /etc/default/keyboard, puis la disposition par défaut de la locale.
pub fn current() -> Layout {
    static CURRENT: OnceLock<Layout> = OnceLock::new();
    *CURRENT.get_or_init(|| {
        let variant = conf("/etc/default/keyboard", "XKBVARIANT").unwrap_or_default();
        let candidates = [
            std::env::var("WINLUX_KEYBOARD").ok(),
            conf("/etc/winlux/input.conf", "XKB_LAYOUT"),
            conf("/etc/default/keyboard", "XKBLAYOUT"),
            std::env::var("XKB_DEFAULT_LAYOUT").ok(),
        ];
        candidates
            .iter()
            .flatten()
            .find_map(|l| from_xkb(l, &variant))
            .unwrap_or_else(|| {
                let loc = crate::intl::locale::current();
                Layout {
                    klid: loc.keyboard,
                    lang: loc.lcid as u16,
                }
            })
    })
}

// ---------------------------------------------------------------------
// Touches mortes
// ---------------------------------------------------------------------

/// Accent (forme isolée) d'un keysym X11 de touche morte.
pub fn dead_accent(keysym: u32) -> Option<char> {
    Some(match keysym {
        0xFE50 => '`',
        0xFE51 => '\u{b4}',
        0xFE52 => '^',
        0xFE53 => '~',
        0xFE54 => '\u{af}',
        0xFE57 => '\u{a8}',
        0xFE58 => '\u{b0}',
        0xFE5A => '\u{2c7}',
        0xFE5B => '\u{b8}',
        _ => return None,
    })
}

/// Lettre accentuée produite par une touche morte suivie d'une lettre.
pub fn compose(accent: char, base: char) -> Option<char> {
    let (from, to): (&str, &str) = match accent {
        '`' => ("aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        '\u{b4}' => ("aeiouyAEIOUYcnszCNSZ", "áéíóúýÁÉÍÓÚÝćńśźĆŃŚŹ"),
        '^' => ("aeiouAEIOUwyWY", "âêîôûÂÊÎÔÛŵŷŴŶ"),
        '~' => ("anoANO", "ãñõÃÑÕ"),
        '\u{a8}' => ("aeiouyAEIOUY", "äëïöüÿÄËÏÖÜŸ"),
        '\u{b0}' => ("auAU", "åůÅŮ"),
        '\u{2c7}' => ("cdenrstzCDENRSTZ", "čďěňřšťžČĎĚŇŘŠŤŽ"),
        '\u{b8}' => ("cgklnrstCGKLNRST", "çģķļņŗşţÇĢĶĻŅŖŞŢ"),
        '\u{af}' => ("aeiouAEIOU", "āēīōūĀĒĪŌŪ"),
        _ => return None,
    };
    from.chars()
        .position(|c| c == base)
        .and_then(|i| to.chars().nth(i))
}

/// État d'une touche morte en attente, comme le pilote clavier Windows.
#[derive(Default, Debug)]
pub struct DeadKeys {
    pending: Option<char>,
}

/// Ce qu'une frappe produit : jusqu'à deux caractères (accent isolé suivi
/// de la lettre quand ils ne se combinent pas).
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Typed {
    pub prefix: Option<char>,
    pub ch: Option<char>,
    /// La frappe est une touche morte (WM_DEADCHAR sous Windows).
    pub dead: bool,
}

impl DeadKeys {
    /// `keysym` est le symbole X11 effectif, `ch` le caractère qu'il produit
    /// seul (`None` pour Maj, Ctrl… qui ne consomment pas l'accent).
    pub fn feed(&mut self, keysym: u32, ch: Option<char>) -> Typed {
        if let Some(accent) = dead_accent(keysym) {
            return match self.pending.take() {
                // Deux touches mortes : les deux accents, comme Windows.
                Some(first) => Typed {
                    prefix: Some(first),
                    ch: Some(accent),
                    dead: false,
                },
                None => {
                    self.pending = Some(accent);
                    Typed {
                        dead: true,
                        ..Typed::default()
                    }
                }
            };
        }
        let Some(c) = ch else {
            return Typed::default();
        };
        match self.pending.take() {
            None => Typed {
                ch: Some(c),
                ..Typed::default()
            },
            Some(accent) => match compose(accent, c) {
                Some(done) => Typed {
                    ch: Some(done),
                    ..Typed::default()
                },
                None if c == ' ' => Typed {
                    ch: Some(accent),
                    ..Typed::default()
                },
                // Retour arrière, Échap… annulent l'accent sans le taper.
                None if (c as u32) < 0x20 => Typed {
                    ch: Some(c),
                    ..Typed::default()
                },
                None => Typed {
                    prefix: Some(accent),
                    ch: Some(c),
                    dead: false,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispositions_xkb_vers_windows() {
        let fr = from_xkb("fr", "").unwrap();
        assert_eq!((fr.hkl(), fr.name()), (0x040C_040C, "0000040C".to_string()));
        let us = from_xkb("us,fr", "intl").unwrap();
        assert_eq!(us.name(), "00020409");
        assert_eq!(us.hkl(), 0x0409_0409);
        assert_eq!(from_xkb("ch", "fr").unwrap().klid, 0x100C);
        assert_eq!(from_xkb("de", "").unwrap().hkl(), 0x0407_0407);
        assert!(from_xkb("xx", "").is_none());
    }

    #[test]
    fn touches_mortes_comme_windows() {
        let mut d = DeadKeys::default();
        // ^ puis e → ê
        assert!(d.feed(0xFE52, None).dead);
        assert_eq!(d.feed(b'e' as u32, Some('e')).ch, Some('ê'));
        // Maj ne consomme pas l'accent : ¨ (Maj+^) puis Maj+i → Ï
        assert!(d.feed(0xFE57, None).dead);
        assert_eq!(d.feed(0xFFE1, None), Typed::default());
        assert_eq!(d.feed(b'I' as u32, Some('I')).ch, Some('Ï'));
        // ^ puis espace → ^ seul
        d.feed(0xFE52, None);
        assert_eq!(d.feed(b' ' as u32, Some(' ')).ch, Some('^'));
        // ^ puis x → « ^x »
        d.feed(0xFE52, None);
        let t = d.feed(b'x' as u32, Some('x'));
        assert_eq!((t.prefix, t.ch), (Some('^'), Some('x')));
        // ^ puis ^ → « ^^ »
        d.feed(0xFE52, None);
        let t = d.feed(0xFE52, None);
        assert_eq!((t.prefix, t.ch), (Some('^'), Some('^')));
        // Sans touche morte, la lettre passe telle quelle.
        assert_eq!(d.feed(b'a' as u32, Some('a')).ch, Some('a'));
        assert_eq!(compose('~', 'n'), Some('ñ'));
        assert_eq!(compose('\u{b8}', 'c'), Some('ç'));
    }
}
