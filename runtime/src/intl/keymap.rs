//! Caractères des touches selon la disposition réelle : `ToUnicode`,
//! `VkKeyScan` et `MapVirtualKey` comme sous Windows, pour l'AZERTY
//! français et belge, le QWERTZ allemand et suisse, et les claviers
//! espagnol, italien, portugais, britannique et américain.
//!
//! Chaque disposition décrit les quatre rangées de touches de caractères
//! (codes de balayage du jeu 1) en trois niveaux : normal, Maj et AltGr.
//! Une espace marque l'absence de caractère. Les codes de touches
//! virtuelles (VK) suivent la règle de Windows : lettre → VK de la lettre,
//! chiffres de la rangée du haut → VK du chiffre, `,` `.` `-` `+`/`=` →
//! VK_OEM_COMMA, PERIOD, MINUS, PLUS ; les autres gardent le VK_OEM_* de
//! leur position sur un clavier américain.

use std::cell::Cell;

/// Codes de balayage des rangées : `²1234567890-=`, `AZERTY…`, `QSDF…`
/// (avec la touche à gauche d'Entrée), `<WXCV…` (touche ISO 0x56 en tête).
const ROWS: [&[u8]; 4] = [
    &[
        0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
    ],
    &[
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    ],
    &[
        0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2B,
    ],
    &[
        0x56, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35,
    ],
];

/// VK_OEM_* des positions américaines, dans l'ordre de `ROWS`.
const US_OEM: [&[u8]; 4] = [
    &[0xC0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xBD, 0xBB],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xDB, 0xDD],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0xBA, 0xDE, 0xDC],
    &[0xE2, 0, 0, 0, 0, 0, 0, 0, 0xBC, 0xBE, 0xBF],
];

pub struct Table {
    /// Trois niveaux par rangée : normal, Maj, AltGr.
    rows: [[&'static str; 3]; 4],
    /// Touches mortes (accents qui attendent la lettre suivante).
    dead: &'static str,
}

const US: Table = Table {
    rows: [
        ["`1234567890-=", "~!@#$%^&*()_+", "             "],
        ["qwertyuiop[]", "QWERTYUIOP{}", "            "],
        ["asdfghjkl;'\\", "ASDFGHJKL:\"|", "            "],
        ["\\zxcvbnm,./", "|ZXCVBNM<>?", "           "],
    ],
    dead: "",
};

const GB: Table = Table {
    rows: [
        ["`1234567890-=", "¬!\"£$%^&*()_+", "¦   €        "],
        ["qwertyuiop[]", "QWERTYUIOP{}", "  é  ú í ó  "],
        ["asdfghjkl;'#", "ASDFGHJKL:@~", "á           "],
        ["\\zxcvbnm,./", "|ZXCVBNM<>?", "           "],
    ],
    dead: "",
};

const FR: Table = Table {
    rows: [
        ["²&é\"'(-è_çà)=", " 1234567890°+", "  ~#{[|`\\^@]}"],
        ["azertyuiop^$", "AZERTYUIOP¨£", "  €        ¤"],
        ["qsdfghjklmù*", "QSDFGHJKLM%µ", "            "],
        ["<wxcvbn,;:!", ">WXCVBN?./§", "           "],
    ],
    dead: "^¨~`",
};

const BE: Table = Table {
    rows: [
        ["²&é\"'(§è!çà)-", "³1234567890°_", " |@#  ^  {}  "],
        ["azertyuiop^$", "AZERTYUIOP¨*", "  €       []"],
        ["qsdfghjklmùµ", "QSDFGHJKLM%£", "          ´`"],
        ["<wxcvbn,;:=", ">WXCVBN?./+", "\\          "],
    ],
    dead: "^¨´`",
};

const DE: Table = Table {
    rows: [
        ["^1234567890ß´", "°!\"§$%&/()=?`", "  ²³   {[]}\\ "],
        ["qwertzuiopü+", "QWERTZUIOPÜ*", "@ €        ~"],
        ["asdfghjklöä#", "ASDFGHJKLÖÄ'", "            "],
        ["<yxcvbnm,.-", ">YXCVBNM;:_", "|      µ   "],
    ],
    dead: "^´`",
};

const CH_DE: Table = Table {
    rows: [
        ["§1234567890'^", "°+\"*ç%&/()=?`", " ¦@#   |¢  ´~"],
        ["qwertzuiopü¨", "QWERTZUIOPè!", "  €       []"],
        ["asdfghjklöä$", "ASDFGHJKLéà£", "          {}"],
        ["<yxcvbnm,.-", ">YXCVBNM;:_", "\\          "],
    ],
    dead: "^`¨´~",
};

const CH_FR: Table = Table {
    rows: [
        ["§1234567890'^", "°+\"*ç%&/()=?`", " ¦@#   |¢  ´~"],
        ["qwertzuiopè¨", "QWERTZUIOPü!", "  €       []"],
        ["asdfghjkléà$", "ASDFGHJKLöä£", "          {}"],
        ["<yxcvbnm,.-", ">YXCVBNM;:_", "\\          "],
    ],
    dead: "^`¨´~",
};

const ES: Table = Table {
    rows: [
        ["º1234567890'¡", "ª!\"·$%&/()=?¿", "\\|@#~€¬      "],
        ["qwertyuiop`+", "QWERTYUIOP^*", "  €       []"],
        ["asdfghjklñ´ç", "ASDFGHJKLÑ¨Ç", "          {}"],
        ["<zxcvbnm,.-", ">ZXCVBNM;:_", "           "],
    ],
    dead: "`^´¨",
};

const IT: Table = Table {
    rows: [
        ["\\1234567890'ì", "|!\"£$%&/()=?^", "             "],
        ["qwertyuiopè+", "QWERTYUIOPé*", "  €       []"],
        ["asdfghjklòàù", "ASDFGHJKLç°§", "         @# "],
        ["<zxcvbnm,.-", ">ZXCVBNM;:_", "           "],
    ],
    dead: "",
};

const PT: Table = Table {
    rows: [
        ["\\1234567890'«", "|!\"#$%&/()=?»", "  @£§€ {[]}  "],
        ["qwertyuiop+´", "QWERTYUIOP*`", "  €         "],
        ["asdfghjklçº~", "ASDFGHJKLÇª^", "            "],
        ["<zxcvbnm,.-", ">ZXCVBNM;:_", "           "],
    ],
    dead: "´`~^",
};

/// Table d'une disposition Windows (KLID) ; QWERTY américain sinon.
pub fn table(klid: u32) -> &'static Table {
    match klid {
        0x0000_040C => &FR,
        0x0000_080C => &BE,
        0x0000_0407 => &DE,
        0x0000_0807 => &CH_DE,
        0x0000_100C => &CH_FR,
        0x0000_040A | 0x0000_080A => &ES,
        0x0000_0410 => &IT,
        0x0000_0816 => &PT,
        0x0000_0809 => &GB,
        _ => &US,
    }
}

/// Une touche de caractère de la disposition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub scan: u8,
    pub vk: u8,
    /// Normal, Maj, AltGr.
    pub chars: [Option<char>; 3],
}

impl Table {
    fn keys(&self) -> impl Iterator<Item = Key> + '_ {
        (0..4).flat_map(move |r| {
            let levels: Vec<Vec<char>> = self.rows[r].iter().map(|s| s.chars().collect()).collect();
            (0..ROWS[r].len()).map(move |i| {
                let at = |level: usize| levels[level].get(i).copied().filter(|&c| c != ' ');
                let base = at(0);
                let vk = match base {
                    Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase() as u8,
                    _ if r == 0 && (1..=10).contains(&i) => b"1234567890"[i - 1],
                    Some(',') => 0xBC,
                    Some('.') => 0xBE,
                    Some('-') => 0xBD,
                    Some('+' | '=') => 0xBB,
                    _ => US_OEM[r][i],
                };
                Key {
                    scan: ROWS[r][i],
                    vk,
                    chars: [base, at(1), at(2)],
                }
            })
        })
    }
    fn is_dead(&self, c: char) -> bool {
        self.dead.contains(c)
    }
}

/// Touches hors caractères : (VK, code de balayage).
const SPECIAL: [(u8, u8); 40] = [
    (0x1B, 0x01), // Échap
    (0x08, 0x0E), // Retour arrière
    (0x09, 0x0F), // Tab
    (0x0D, 0x1C), // Entrée
    (0x11, 0x1D), // Ctrl
    (0x10, 0x2A), // Maj
    (0x12, 0x38), // Alt
    (0x20, 0x39), // Espace
    (0x14, 0x3A), // Verr. Maj
    (0x70, 0x3B),
    (0x71, 0x3C),
    (0x72, 0x3D),
    (0x73, 0x3E),
    (0x74, 0x3F),
    (0x75, 0x40),
    (0x76, 0x41),
    (0x77, 0x42),
    (0x78, 0x43),
    (0x79, 0x44),
    (0x90, 0x45), // Verr. Num
    (0x91, 0x46), // Arrêt défil
    (0x67, 0x47),
    (0x68, 0x48),
    (0x69, 0x49),
    (0x6D, 0x4A),
    (0x64, 0x4B),
    (0x65, 0x4C),
    (0x66, 0x4D),
    (0x6B, 0x4E),
    (0x61, 0x4F),
    (0x62, 0x50),
    (0x63, 0x51),
    (0x60, 0x52),
    (0x6E, 0x53),
    (0x7A, 0x57), // F11
    (0x7B, 0x58), // F12
    (0xA0, 0x2A), // Maj gauche
    (0xA1, 0x36), // Maj droite
    (0xA2, 0x1D), // Ctrl gauche
    (0xA4, 0x38), // Alt gauche
];

/// MapVirtualKey(Ex) pour la disposition `klid`.
pub fn map_virtual_key(klid: u32, code: u32, kind: u32) -> u32 {
    let t = table(klid);
    match kind {
        // MAPVK_VK_TO_VSC (et _EX) : code de balayage.
        0 | 4 => {
            let vk = match code {
                0xA3 => 0x11,
                0xA5 => 0x12,
                other => other,
            };
            if let Some(k) = t.keys().find(|k| k.vk as u32 == vk) {
                return k.scan as u32;
            }
            SPECIAL
                .iter()
                .find(|(v, _)| *v as u32 == vk)
                .map_or(0, |(_, s)| *s as u32)
        }
        // MAPVK_VSC_TO_VK (1) sans distinction gauche/droite, _EX (3) avec.
        1 | 3 => {
            let scan = code & 0xFF;
            if let Some(k) = t.keys().find(|k| k.scan as u32 == scan) {
                return k.vk as u32;
            }
            match (scan, kind) {
                (0x2A, 3) => 0xA0,
                (0x36, 3) => 0xA1,
                (0x1D, 3) => 0xA2,
                (0x38, 3) => 0xA4,
                (0x36, _) => 0x10,
                _ => SPECIAL
                    .iter()
                    .find(|(v, s)| *s as u32 == scan && *v < 0xA0)
                    .map_or(0, |(v, _)| *v as u32),
            }
        }
        // MAPVK_VK_TO_CHAR : caractère non modifié (lettres en majuscules),
        // bit de poids fort pour une touche morte.
        2 => {
            if let Some(k) = t.keys().find(|k| k.vk as u32 == code) {
                let Some(c) = k.chars[0] else {
                    return 0;
                };
                let c = if c.is_alphabetic() {
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c
                };
                let dead = if t.is_dead(c) { 0x8000_0000 } else { 0 };
                return c as u32 | dead;
            }
            match code {
                0x60..=0x69 => b'0' as u32 + code - 0x60,
                0x6A => '*' as u32,
                0x6B => '+' as u32,
                0x6D => '-' as u32,
                0x6E => '.' as u32,
                0x6F => '/' as u32,
                0x08 | 0x09 | 0x0D | 0x1B | 0x20 => code,
                _ => 0,
            }
        }
        _ => 0,
    }
}

/// VkKeyScan(Ex) : VK en octet bas, Maj (1), Ctrl (2), Alt (4) en octet
/// haut ; -1 si le caractère n'est sur aucune touche.
pub fn vk_key_scan(klid: u32, ch: char) -> i16 {
    let t = table(klid);
    for k in t.keys() {
        for (level, shift) in [(0usize, 0u8), (1, 1), (2, 6)] {
            if k.chars[level] == Some(ch) {
                return i16::from_le_bytes([k.vk, shift]);
            }
        }
    }
    match ch {
        ' ' => 0x20,
        '\r' | '\n' => 0x0D,
        '\t' => 0x09,
        '\x08' => 0x08,
        '\x1b' => 0x1B,
        // Ctrl + lettre : caractères de commande 1 à 26.
        '\x01'..='\x1a' => i16::from_le_bytes([b'A' + ch as u8 - 1, 2]),
        _ => -1,
    }
}

thread_local! {
    /// Accent d'une touche morte en attente (état du clavier du thread).
    static PENDING: Cell<Option<char>> = const { Cell::new(None) };
}

/// Résultat de ToUnicode : caractères produits et code de retour Windows
/// (-1 touche morte, 0 rien, n caractères).
#[derive(Debug, PartialEq)]
pub struct Typed {
    pub chars: Vec<char>,
    pub code: i32,
}

fn pressed(state: &[u8; 256], vk: usize) -> bool {
    state[vk] & 0x80 != 0
}

/// ToUnicode(Ex) : `state` est le tableau de GetKeyboardState.
/// `flags` & 4 : ne pas modifier l'état des touches mortes (Windows 10+).
pub fn to_unicode(klid: u32, vk: u32, scan: u32, state: &[u8; 256], flags: u32) -> Typed {
    let t = table(klid);
    let none = Typed {
        chars: Vec::new(),
        code: 0,
    };
    if scan & 0x8000 != 0 {
        return none; // touche relâchée
    }
    let shift = pressed(state, 0x10) || pressed(state, 0xA0) || pressed(state, 0xA1);
    let ctrl = pressed(state, 0x11) || pressed(state, 0xA2) || pressed(state, 0xA3);
    let alt = pressed(state, 0x12) || pressed(state, 0xA4) || pressed(state, 0xA5);
    let caps = state[0x14] & 1 != 0;
    let altgr = (ctrl && alt) || pressed(state, 0xA5);
    let key = t
        .keys()
        .find(|k| k.scan as u32 == scan & 0xFF && scan & 0xFF != 0)
        .or_else(|| t.keys().find(|k| k.vk as u32 == vk));
    let produced = match key {
        Some(k) if altgr => k.chars[2],
        Some(_) if ctrl || alt => {
            // Ctrl + lettre : caractère de commande ; Alt seul : rien.
            if ctrl && !alt && (0x41..=0x5A).contains(&vk) {
                return Typed {
                    chars: vec![char::from(vk as u8 - 0x40)],
                    code: 1,
                };
            }
            None
        }
        Some(k) => {
            let letter = k.chars[0].is_some_and(char::is_alphabetic);
            let upper = shift ^ (caps && letter);
            if upper {
                k.chars[1]
            } else {
                k.chars[0]
            }
        }
        None => match vk {
            0x20 => Some(' '),
            0x0D => Some('\r'),
            0x09 if !ctrl && !alt => Some('\t'),
            0x08 => Some('\x08'),
            0x1B => Some('\x1b'),
            0x60..=0x69 => char::from_digit(vk - 0x60, 10),
            0x6A => Some('*'),
            0x6B => Some('+'),
            0x6D => Some('-'),
            0x6E => Some(if crate::intl::locale::current().decimal == "," {
                ','
            } else {
                '.'
            }),
            0x6F => Some('/'),
            _ => None,
        },
    };
    let Some(c) = produced else {
        return none;
    };
    let keep = flags & 4 != 0;
    let pending = PENDING.with(Cell::get);
    if key.is_some() && t.is_dead(c) && pending.is_none() {
        if !keep {
            PENDING.with(|p| p.set(Some(c)));
        }
        return Typed {
            chars: vec![c],
            code: -1,
        };
    }
    if let Some(accent) = pending {
        if !keep {
            PENDING.with(|p| p.set(None));
        }
        if c == ' ' {
            return Typed {
                chars: vec![accent],
                code: 1,
            };
        }
        return match super::keyboard::compose(accent, c) {
            Some(composed) => Typed {
                chars: vec![composed],
                code: 1,
            },
            None => Typed {
                chars: vec![accent, c],
                code: 2,
            },
        };
    }
    Typed {
        chars: vec![c],
        code: 1,
    }
}

/// Disposition active (pour les appels sans HKL).
pub fn current_klid() -> u32 {
    super::keyboard::current().klid
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(keys: &[usize]) -> [u8; 256] {
        let mut s = [0u8; 256];
        for &k in keys {
            s[k] = 0x80;
        }
        s
    }

    #[test]
    fn rangees_completes() {
        for t in [&US, &GB, &FR, &BE, &DE, &CH_DE, &CH_FR, &ES, &IT, &PT] {
            for (r, levels) in t.rows.iter().enumerate() {
                for level in levels {
                    assert_eq!(level.chars().count(), ROWS[r].len(), "rangée {r} : {level}");
                }
            }
        }
    }

    #[test]
    fn azerty_francais() {
        const FRK: u32 = 0x0000_040C;
        // La touche A de l'AZERTY est à la place du Q américain (0x10).
        let a = to_unicode(FRK, 0x41, 0x10, &state(&[]), 0);
        assert_eq!(
            a,
            Typed {
                chars: vec!['a'],
                code: 1
            }
        );
        let one = to_unicode(FRK, 0x31, 0x02, &state(&[0x10]), 0);
        assert_eq!(one.chars, vec!['1'], "Maj + & donne 1");
        let e_acute = to_unicode(FRK, 0x32, 0x03, &state(&[]), 0);
        assert_eq!(e_acute.chars, vec!['é']);
        let euro = to_unicode(FRK, 0x45, 0x12, &state(&[0x11, 0x12]), 0);
        assert_eq!(euro.chars, vec!['€'], "AltGr + E");
        let caps = to_unicode(FRK, 0x45, 0x12, &state(&[]).tap_caps(), 0);
        assert_eq!(caps.chars, vec!['E'], "Verr. Maj");
        // ^ puis e : ê ; ^ puis x : ^x ; ^ puis espace : ^.
        assert_eq!(to_unicode(FRK, 0xDD, 0x1A, &state(&[]), 0).code, -1);
        assert_eq!(to_unicode(FRK, 0x45, 0x12, &state(&[]), 0).chars, vec!['ê']);
        assert_eq!(to_unicode(FRK, 0xDD, 0x1A, &state(&[]), 0).code, -1);
        assert_eq!(
            to_unicode(FRK, 0x58, 0x2D, &state(&[]), 0),
            Typed {
                chars: vec!['^', 'x'],
                code: 2
            }
        );
        // Ctrl + C reste un caractère de commande.
        assert_eq!(
            to_unicode(FRK, 0x43, 0x2E, &state(&[0x11]), 0).chars,
            vec!['\x03']
        );
        assert_eq!(vk_key_scan(FRK, 'a'), 0x41);
        assert_eq!(vk_key_scan(FRK, '1'), 0x0131, "Maj + touche 1");
        assert_eq!(vk_key_scan(FRK, '€'), 0x0645, "AltGr (Ctrl+Alt) + E");
        assert_eq!(vk_key_scan(FRK, 'm'), 0x4D);
        assert_eq!(map_virtual_key(FRK, 0x4D, 0), 0x27, "M est à droite de L");
        assert_eq!(map_virtual_key(FRK, 0x10, 1), 0x41, "scan 0x10 → VK_A");
        assert_eq!(map_virtual_key(FRK, 0xBC, 2), ',' as u32);
        let circumflex = map_virtual_key(FRK, 0x1A, 1);
        assert_eq!(
            map_virtual_key(FRK, circumflex, 2),
            0x8000_0000 | '^' as u32,
            "touche morte"
        );
    }

    #[test]
    fn qwertz_allemand_et_qwerty() {
        const DEK: u32 = 0x0000_0407;
        assert_eq!(
            to_unicode(DEK, 0x5A, 0x15, &state(&[]), 0).chars,
            vec!['z'],
            "Z à la place du Y"
        );
        assert_eq!(
            to_unicode(DEK, 0xBA, 0x1A, &state(&[0x10]), 0).chars,
            vec!['Ü']
        );
        assert_eq!(
            to_unicode(DEK, 0x51, 0x10, &state(&[0xA5]), 0).chars,
            vec!['@'],
            "AltGr + Q"
        );
        assert_ne!(vk_key_scan(DEK, 'ß'), -1);
        assert_eq!(vk_key_scan(0x0409, 'A'), 0x0141);
        assert_eq!(vk_key_scan(0x0409, '€'), -1);
        assert_eq!(map_virtual_key(0x0409, 0x2A, 3), 0xA0);
        assert_eq!(map_virtual_key(0x0409, 0x2A, 1), 0x10);
        assert_eq!(map_virtual_key(0x0409, 0x0D, 0), 0x1C);
        assert_eq!(
            to_unicode(0x0409, 0x61, 0, &state(&[]), 0).chars,
            vec!['1'],
            "pavé numérique"
        );
    }

    trait Caps {
        fn tap_caps(self) -> Self;
    }
    impl Caps for [u8; 256] {
        fn tap_caps(mut self) -> Self {
            self[0x14] = 1;
            self
        }
    }
}
