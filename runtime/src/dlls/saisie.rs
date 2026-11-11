//! Saisie simulée (#1683, pour les macros de #1655) : SendInput, keybd_event et mouse_event tapent
//! dans la session X de la personne par l'extension XTEST, avec les mêmes
//! droits qu'elle. Aucun périphérique virtuel du noyau n'est créé
//! (/dev/uhid reste fermé aux comptes) : un programme qui tape ne peut
//! atteindre que l'écran de son propre compte.
//!
//! Garde-fous, comme la séparation des bureaux sous Windows :
//! - la première fois, Winlux demande « Autoriser « app » à taper au clavier
//!   et à piloter la souris à votre place ? » (4 langues, refus possible,
//!   révocable dans Paramètres > Confidentialité et sécurité) ; sans réponse
//!   possible, c'est non ;
//! - rien n'est tapé tant que l'écran de verrouillage est affiché
//!   (/run/winlux/lock/active), le mot de passe ne peut donc pas être essayé
//!   par un programme.
//!
//! Les touches virtuelles (VK_*) suivent la disposition du clavier comme sous
//! Windows : VK_A est la touche qui produit « a », où qu'elle soit. Les
//! caractères (KEYEVENTF_UNICODE) passent par leur keysym, avec Maj si la
//! disposition le demande, ou par un keycode libre réaffecté. Les codes de
//! balayage (KEYEVENTF_SCANCODE) sont ceux du jeu 1 du PC, que le noyau
//! Linux numérote de la même façon (keycode X = code noyau + 8).
//!
//! Sources : documentation publique de SendInput, INPUT, KEYBDINPUT,
//! MOUSEINPUT et des codes de touches virtuelles (learn.microsoft.com),
//! protocole XTEST et keysyms de X.Org (keysymdef.h, XF86keysym.h, licence
//! MIT), codes de touches du noyau (input-event-codes.h).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;

const INPUT_MOUSE: u32 = 0;
const INPUT_KEYBOARD: u32 = 1;

const KEYEVENTF_EXTENDEDKEY: u32 = 0x1;
const KEYEVENTF_KEYUP: u32 = 0x2;
const KEYEVENTF_UNICODE: u32 = 0x4;
const KEYEVENTF_SCANCODE: u32 = 0x8;

const MOUSEEVENTF_MOVE: u32 = 0x1;
const MOUSEEVENTF_LEFTDOWN: u32 = 0x2;
const MOUSEEVENTF_LEFTUP: u32 = 0x4;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x8;
const MOUSEEVENTF_RIGHTUP: u32 = 0x10;
const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x20;
const MOUSEEVENTF_MIDDLEUP: u32 = 0x40;
const MOUSEEVENTF_XDOWN: u32 = 0x80;
const MOUSEEVENTF_XUP: u32 = 0x100;
const MOUSEEVENTF_WHEEL: u32 = 0x800;
const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;

/// Taille d'INPUT : 40 octets en x64, 28 en x86 (union à 8 ou à 4).
const INPUT_SIZE_64: usize = 40;
const INPUT_SIZE_32: usize = 28;

/// Touche à presser ou relâcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Vk {
        vk: u16,
        extended: bool,
    },
    Scan {
        scan: u16,
        extended: bool,
    },
    /// Caractère Unicode (paire de substitution déjà réunie).
    Char(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Key {
        down: bool,
        key: Key,
    },
    /// Absolu : coordonnées normalisées 0..65535 ; relatif : pixels.
    Motion {
        absolute: bool,
        x: i32,
        y: i32,
    },
    /// Boutons X : 1 gauche, 2 milieu, 3 droit, 8 et 9 latéraux.
    Button {
        button: u8,
        down: bool,
    },
    /// Molette : delta en 120e de cran, vertical (positif = vers le haut)
    /// ou horizontal (positif = vers la droite).
    Wheel {
        horizontal: bool,
        delta: i32,
    },
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap_or([0; 4]))
}

/// KEYBDINPUT ou keybd_event : une touche.
pub fn keyboard_event(vk: u16, scan: u16, flags: u32) -> Event {
    let down = flags & KEYEVENTF_KEYUP == 0;
    let extended = flags & KEYEVENTF_EXTENDEDKEY != 0;
    let key = if flags & KEYEVENTF_UNICODE != 0 {
        Key::Char(scan as u32)
    } else if flags & KEYEVENTF_SCANCODE != 0 {
        Key::Scan { scan, extended }
    } else {
        Key::Vk { vk, extended }
    };
    Event::Key { down, key }
}

/// MOUSEINPUT ou mouse_event : déplacement, boutons puis molette, dans
/// l'ordre où Windows les applique.
pub fn mouse_events(dx: i32, dy: i32, data: u32, flags: u32, out: &mut Vec<Event>) {
    if flags & MOUSEEVENTF_MOVE != 0 {
        out.push(Event::Motion {
            absolute: flags & MOUSEEVENTF_ABSOLUTE != 0,
            x: dx,
            y: dy,
        });
    }
    let x_button = if data & 2 != 0 { 9 } else { 8 };
    for (bit, button, down) in [
        (MOUSEEVENTF_LEFTDOWN, 1, true),
        (MOUSEEVENTF_LEFTUP, 1, false),
        (MOUSEEVENTF_RIGHTDOWN, 3, true),
        (MOUSEEVENTF_RIGHTUP, 3, false),
        (MOUSEEVENTF_MIDDLEDOWN, 2, true),
        (MOUSEEVENTF_MIDDLEUP, 2, false),
        (MOUSEEVENTF_XDOWN, x_button, true),
        (MOUSEEVENTF_XUP, x_button, false),
    ] {
        if flags & bit != 0 {
            out.push(Event::Button { button, down });
        }
    }
    if flags & MOUSEEVENTF_WHEEL != 0 {
        out.push(Event::Wheel {
            horizontal: false,
            delta: data as i32,
        });
    }
    if flags & MOUSEEVENTF_HWHEEL != 0 {
        out.push(Event::Wheel {
            horizontal: true,
            delta: data as i32,
        });
    }
}

/// Tableau INPUT de SendInput → événements. `size` (cbSize) choisit la
/// disposition x64 ou x86 ; toute autre taille est refusée comme sous
/// Windows (ERROR_INVALID_PARAMETER).
pub fn parse(bytes: &[u8], count: usize, size: usize) -> Result<Vec<Event>, u32> {
    let union = match size {
        INPUT_SIZE_64 => 8,
        INPUT_SIZE_32 => 4,
        _ => return Err(ERROR_INVALID_PARAMETER),
    };
    if bytes.len() < count.saturating_mul(size) {
        return Err(ERROR_INVALID_PARAMETER);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < count {
        let rec = &bytes[i * size..(i + 1) * size];
        let u = &rec[union..];
        match u32_at(rec, 0) {
            INPUT_KEYBOARD => {
                let (vk, scan, flags) = (u16_at(u, 0), u16_at(u, 2), u32_at(u, 4));
                let mut event = keyboard_event(vk, scan, flags);
                // Caractère hors du plan de base : deux entrées (haute puis
                // basse) dans le même sens, réunies en un seul caractère.
                if let Event::Key {
                    down,
                    key: Key::Char(high @ 0xD800..=0xDBFF),
                } = event
                {
                    if i + 1 < count {
                        let next = &bytes[(i + 1) * size..(i + 2) * size];
                        let n = &next[union..];
                        let (nscan, nflags) = (u16_at(n, 2) as u32, u32_at(n, 4));
                        if u32_at(next, 0) == INPUT_KEYBOARD
                            && nflags & KEYEVENTF_UNICODE != 0
                            && (nflags & KEYEVENTF_KEYUP == 0) == down
                            && (0xDC00..=0xDFFF).contains(&nscan)
                        {
                            let c = 0x10000 + ((high - 0xD800) << 10) + (nscan - 0xDC00);
                            event = Event::Key {
                                down,
                                key: Key::Char(c),
                            };
                            i += 1;
                        }
                    }
                }
                out.push(event);
            }
            INPUT_MOUSE => {
                let (dx, dy) = (u32_at(u, 0) as i32, u32_at(u, 4) as i32);
                mouse_events(dx, dy, u32_at(u, 8), u32_at(u, 12), &mut out);
            }
            // INPUT_HARDWARE : messages d'un autre matériel, rien à taper.
            _ => {}
        }
        i += 1;
    }
    Ok(out)
}

// --- Touches virtuelles → keysyms ----------------------------------------------------------

/// Keysyms d'une touche virtuelle sans caractère (le premier présent dans
/// la disposition gagne). Vide pour les lettres, chiffres et touches OEM,
/// cherchées d'après la disposition (`find_vk`).
pub fn vk_keysyms(vk: u16, extended: bool) -> &'static [u32] {
    match vk {
        0x08 => &[0xFF08],                     // VK_BACK
        0x09 => &[0xFF09],                     // VK_TAB
        0x0C => &[0xFF9D, 0xFF0B],             // VK_CLEAR
        0x0D if extended => &[0xFF8D, 0xFF0D], // VK_RETURN du pavé numérique
        0x0D => &[0xFF0D],
        0x10 | 0xA0 => &[0xFFE1],                      // Maj gauche
        0xA1 => &[0xFFE2],                             // Maj droite
        0x11 if extended => &[0xFFE4, 0xFFE3],         // Ctrl droit
        0x11 | 0xA2 => &[0xFFE3],                      // Ctrl gauche
        0xA3 => &[0xFFE4, 0xFFE3],                     // Ctrl droit
        0x12 if extended => &[0xFFEA, 0xFE03, 0xFFE9], // Alt droit (AltGr)
        0x12 | 0xA4 => &[0xFFE9],                      // Alt gauche
        0xA5 => &[0xFFEA, 0xFE03, 0xFFE9],             // Alt droit (AltGr)
        0x13 => &[0xFF13],                             // VK_PAUSE
        0x14 => &[0xFFE5],                             // VK_CAPITAL
        0x1B => &[0xFF1B],                             // VK_ESCAPE
        0x20 => &[0x20],                               // VK_SPACE
        0x21 => &[0xFF55],                             // VK_PRIOR
        0x22 => &[0xFF56],                             // VK_NEXT
        0x23 => &[0xFF57],                             // VK_END
        0x24 => &[0xFF50],                             // VK_HOME
        0x25 => &[0xFF51],                             // VK_LEFT
        0x26 => &[0xFF52],                             // VK_UP
        0x27 => &[0xFF53],                             // VK_RIGHT
        0x28 => &[0xFF54],                             // VK_DOWN
        0x2C => &[0xFF61],                             // VK_SNAPSHOT
        0x2D => &[0xFF63],                             // VK_INSERT
        0x2E => &[0xFFFF],                             // VK_DELETE
        0x5B => &[0xFFEB],                             // VK_LWIN
        0x5C => &[0xFFEC, 0xFFEB],                     // VK_RWIN
        0x5D => &[0xFF67],                             // VK_APPS
        0x5F => &[0x1008_FF2F],                        // VK_SLEEP
        0x60 => &[0xFFB0],                             // VK_NUMPAD0
        0x61 => &[0xFFB1],
        0x62 => &[0xFFB2],
        0x63 => &[0xFFB3],
        0x64 => &[0xFFB4],
        0x65 => &[0xFFB5],
        0x66 => &[0xFFB6],
        0x67 => &[0xFFB7],
        0x68 => &[0xFFB8],
        0x69 => &[0xFFB9],
        0x6A => &[0xFFAA],         // VK_MULTIPLY
        0x6B => &[0xFFAB],         // VK_ADD
        0x6C => &[0xFFAC],         // VK_SEPARATOR
        0x6D => &[0xFFAD],         // VK_SUBTRACT
        0x6E => &[0xFFAE, 0xFFAC], // VK_DECIMAL
        0x6F => &[0xFFAF],         // VK_DIVIDE
        0x70 => &[0xFFBE],         // VK_F1
        0x71 => &[0xFFBF],
        0x72 => &[0xFFC0],
        0x73 => &[0xFFC1],
        0x74 => &[0xFFC2],
        0x75 => &[0xFFC3],
        0x76 => &[0xFFC4],
        0x77 => &[0xFFC5],
        0x78 => &[0xFFC6],
        0x79 => &[0xFFC7],
        0x7A => &[0xFFC8],
        0x7B => &[0xFFC9], // VK_F12
        0x7C => &[0xFFCA], // VK_F13
        0x7D => &[0xFFCB],
        0x7E => &[0xFFCC],
        0x7F => &[0xFFCD],
        0x80 => &[0xFFCE],
        0x81 => &[0xFFCF],
        0x82 => &[0xFFD0],
        0x83 => &[0xFFD1],
        0x84 => &[0xFFD2],
        0x85 => &[0xFFD3],
        0x86 => &[0xFFD4],
        0x87 => &[0xFFD5],                   // VK_F24
        0x90 => &[0xFF7F],                   // VK_NUMLOCK
        0x91 => &[0xFF14],                   // VK_SCROLL
        0xA6 => &[0x1008_FF26],              // VK_BROWSER_BACK
        0xA7 => &[0x1008_FF27],              // VK_BROWSER_FORWARD
        0xA8 => &[0x1008_FF29],              // VK_BROWSER_REFRESH
        0xA9 => &[0x1008_FF28],              // VK_BROWSER_STOP
        0xAA => &[0x1008_FF1B],              // VK_BROWSER_SEARCH
        0xAB => &[0x1008_FF30],              // VK_BROWSER_FAVORITES
        0xAC => &[0x1008_FF18],              // VK_BROWSER_HOME
        0xAD => &[0x1008_FF12],              // VK_VOLUME_MUTE
        0xAE => &[0x1008_FF11],              // VK_VOLUME_DOWN
        0xAF => &[0x1008_FF13],              // VK_VOLUME_UP
        0xB0 => &[0x1008_FF17],              // VK_MEDIA_NEXT_TRACK
        0xB1 => &[0x1008_FF16],              // VK_MEDIA_PREV_TRACK
        0xB2 => &[0x1008_FF15],              // VK_MEDIA_STOP
        0xB3 => &[0x1008_FF14, 0x1008_FF31], // VK_MEDIA_PLAY_PAUSE
        0xB4 => &[0x1008_FF19],              // VK_LAUNCH_MAIL
        0xB5 => &[0x1008_FF32],              // VK_LAUNCH_MEDIA_SELECT
        0xB6 => &[0x1008_FF33],              // VK_LAUNCH_APP1
        0xB7 => &[0x1008_FF1D],              // VK_LAUNCH_APP2
        _ => &[],
    }
}

/// Keysym américain d'une touche à caractère, quand la disposition n'a
/// aucune touche pour ce VK (on réaffecte alors un keycode libre).
pub fn vk_fallback_keysym(vk: u16) -> Option<u32> {
    Some(match vk {
        0x30..=0x39 => vk as u32,
        0x41..=0x5A => vk as u32 + 0x20,
        0xBA => b';' as u32,
        0xBB => b'=' as u32,
        0xBC => b',' as u32,
        0xBD => b'-' as u32,
        0xBE => b'.' as u32,
        0xBF => b'/' as u32,
        0xC0 => b'`' as u32,
        0xDB => b'[' as u32,
        0xDC => b'\\' as u32,
        0xDD => b']' as u32,
        0xDE => b'\'' as u32,
        0xE2 => b'<' as u32,
        _ => return None,
    })
}

/// Keysym d'un caractère Unicode (Latin-1 direct, sinon 0x01000000 + point
/// de code) ; les caractères de contrôle courants deviennent leur touche.
pub fn char_keysym(c: u32) -> Option<u32> {
    Some(match c {
        0x08 => 0xFF08,
        0x09 => 0xFF09,
        0x0A | 0x0D => 0xFF0D,
        0x1B => 0xFF1B,
        0x20..=0x7E | 0xA0..=0xFF => c,
        0x100..=0x10FFFF if !(0xD800..=0xDFFF).contains(&c) => 0x0100_0000 | c,
        _ => return None,
    })
}

/// Code de balayage PC (jeu 1) → code de touche du noyau Linux.
pub fn scan_to_linux(scan: u16, extended: bool) -> Option<u8> {
    let extended = extended || scan & 0xFF00 == 0xE000;
    let scan = (scan & 0xFF) as u8;
    if !extended {
        return (1..=0x58).contains(&scan).then_some(scan);
    }
    Some(match scan {
        0x1C => 96,  // Entrée du pavé
        0x1D => 97,  // Ctrl droit
        0x35 => 98,  // / du pavé
        0x37 => 99,  // Impr. écran
        0x38 => 100, // Alt droit
        0x47 => 102, // Origine
        0x48 => 103, // Haut
        0x49 => 104, // Page préc.
        0x4B => 105, // Gauche
        0x4D => 106, // Droite
        0x4F => 107, // Fin
        0x50 => 108, // Bas
        0x51 => 109, // Page suiv.
        0x52 => 110, // Inser
        0x53 => 111, // Suppr
        0x5B => 125, // Windows gauche
        0x5C => 126, // Windows droite
        0x5D => 127, // Menu
        0x20 => 113, // Muet
        0x2E => 114, // Volume -
        0x30 => 115, // Volume +
        0x22 => 164, // Lecture/pause
        0x24 => 166, // Arrêt
        0x19 => 163, // Piste suivante
        0x10 => 165, // Piste précédente
        _ => return None,
    })
}

// --- Disposition du clavier X ----------------------------------------------------------------

/// Table GetKeyboardMapping : `per` keysyms par keycode à partir de `min`.
pub struct Mapping {
    pub min: u8,
    pub per: usize,
    pub syms: Vec<u32>,
}

impl Mapping {
    fn rows(&self) -> impl Iterator<Item = (u8, &[u32])> {
        let min = self.min as usize;
        self.syms
            .chunks(self.per.max(1))
            .enumerate()
            .filter(move |(i, _)| min + i <= 255)
            .map(move |(i, row)| ((min + i) as u8, row))
    }

    /// Premier keycode dont le keysym de base est l'un des candidats, dans
    /// l'ordre des candidats.
    pub fn find_base(&self, candidates: &[u32]) -> Option<u8> {
        candidates.iter().find_map(|&sym| {
            self.rows()
                .find(|(_, row)| row.first() == Some(&sym))
                .map(|(code, _)| code)
        })
    }

    /// Touche qui produit ce VK dans la disposition (règle du moteur X11 de
    /// Winlux, pour que la touche tapée revienne avec le même VK).
    pub fn find_vk(&self, vk: u16) -> Option<u8> {
        self.rows()
            .find(|(_, row)| {
                row.first().is_some_and(|&b| b != 0)
                    && super::gui_x11::key_levels_to_vk(row[0], row.get(1).copied().unwrap_or(0))
                        == Some(vk as usize)
            })
            .map(|(code, _)| code)
    }

    /// Touche et Maj nécessaires pour ce keysym (niveaux 1 et 2 seulement).
    pub fn find_char(&self, sym: u32) -> Option<(u8, bool)> {
        for (code, row) in self.rows() {
            let base = row.first().copied().unwrap_or(0);
            let shifted = row.get(1).copied().unwrap_or(0);
            if base == sym && sym != 0 {
                return Some((code, false));
            }
            if shifted == sym && sym != 0 {
                return Some((code, true));
            }
            // Lettre seule : la majuscule est au niveau Maj.
            if shifted == 0 && (0x61..=0x7A).contains(&base) && sym == base - 0x20 {
                return Some((code, true));
            }
        }
        None
    }

    /// Keycodes sans aucun keysym, réaffectables.
    pub fn spare(&self) -> Vec<u8> {
        self.rows()
            .filter(|(code, row)| *code >= 8 && row.iter().all(|&s| s == 0))
            .map(|(code, _)| code)
            .collect()
    }
}

// --- Injection XTEST --------------------------------------------------------------------------

struct Injector {
    conn: RustConnection,
    root: Window,
    width: i32,
    height: i32,
    /// Touches et caractères enfoncés : keycode, et Maj ajouté par nous.
    pressed: HashMap<Key, (u8, bool)>,
    /// Keycodes libres réaffectés : keysym → keycode (réutilisés).
    remapped: HashMap<u32, u8>,
    wheel_rest: [i32; 2],
}

impl std::hash::Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        match *self {
            Key::Vk { vk, .. } => (0u8, vk as u32).hash(h),
            Key::Scan { scan, extended } => (1u8, scan as u32 | (extended as u32) << 16).hash(h),
            Key::Char(c) => (2u8, c).hash(h),
        }
    }
}

impl Injector {
    fn open() -> Option<Injector> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        conn.xtest_get_version(2, 2).ok()?.reply().ok()?;
        let s = &conn.setup().roots[screen];
        let (root, width, height) = (s.root, s.width_in_pixels as i32, s.height_in_pixels as i32);
        Some(Injector {
            conn,
            root,
            width,
            height,
            pressed: HashMap::new(),
            remapped: HashMap::new(),
            wheel_rest: [0, 0],
        })
    }

    fn mapping(&self) -> Option<Mapping> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = self
            .conn
            .get_keyboard_mapping(min, max - min + 1)
            .ok()?
            .reply()
            .ok()?;
        Some(Mapping {
            min,
            per: reply.keysyms_per_keycode as usize,
            syms: reply.keysyms,
        })
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) {
        let _ = self
            .conn
            .xtest_fake_input(kind, detail, 0, self.root, x, y, 0);
    }

    fn key_code(&self, down: bool, code: u8) {
        self.fake(if down { 2 } else { 3 }, code, 0, 0);
    }

    /// Réaffecte un keycode libre à ce keysym (comme xdotool).
    fn remap(&mut self, map: &Mapping, sym: u32) -> Option<u8> {
        if let Some(&code) = self.remapped.get(&sym) {
            if map.find_base(&[sym]) == Some(code) {
                return Some(code);
            }
        }
        let used: Vec<u8> = self.remapped.values().copied().collect();
        let spare = map.spare();
        let code = spare
            .iter()
            .copied()
            .find(|c| !used.contains(c))
            .or_else(|| spare.first().copied())?;
        let per = map.per.max(1);
        self.conn
            .change_keyboard_mapping(1, code, per as u8, &vec![sym; per])
            .ok()?;
        self.conn.sync().ok()?;
        self.remapped.retain(|_, c| *c != code);
        self.remapped.insert(sym, code);
        Some(code)
    }

    /// Keycode et Maj d'une touche à presser.
    fn resolve(&mut self, map: &Mapping, key: Key) -> Option<(u8, bool)> {
        match key {
            Key::Vk { vk, extended } => {
                let fixed = vk_keysyms(vk, extended);
                if !fixed.is_empty() {
                    return map
                        .find_base(fixed)
                        .or_else(|| self.remap(map, fixed[0]))
                        .map(|c| (c, false));
                }
                map.find_vk(vk)
                    .or_else(|| {
                        let sym = vk_fallback_keysym(vk)?;
                        self.remap(map, sym)
                    })
                    .map(|c| (c, false))
            }
            Key::Scan { scan, extended } => {
                scan_to_linux(scan, extended).map(|code| (code + 8, false))
            }
            Key::Char(c) => {
                let sym = char_keysym(c)?;
                map.find_char(sym)
                    .or_else(|| self.remap(map, sym).map(|code| (code, false)))
            }
        }
    }

    fn key(&mut self, map: &Mapping, down: bool, key: Key) {
        if !down {
            // Relâcher ce qui a été pressé, même si la disposition a changé.
            let resolved = self
                .pressed
                .remove(&key)
                .or_else(|| self.resolve(map, key).map(|(c, _)| (c, false)));
            if let Some((code, shift_added)) = resolved {
                self.key_code(false, code);
                if shift_added {
                    if let Some(shift) = map.find_base(&[0xFFE1]) {
                        self.key_code(false, shift);
                    }
                }
            }
            return;
        }
        let Some((code, need_shift)) = self.resolve(map, key) else {
            return;
        };
        let shift = map.find_base(&[0xFFE1]);
        let shift_added = need_shift && shift.is_some();
        if let (true, Some(shift)) = (shift_added, shift) {
            self.key_code(true, shift);
        }
        self.key_code(true, code);
        self.pressed.insert(key, (code, shift_added));
    }

    fn apply(&mut self, events: &[Event]) {
        let map = self.mapping();
        for &event in events {
            match event {
                Event::Key { down, key } => {
                    if let Some(map) = &map {
                        self.key(map, down, key);
                    }
                }
                Event::Motion {
                    absolute: true,
                    x,
                    y,
                } => {
                    let px =
                        (x.clamp(0, 65535) as i64 * (self.width - 1).max(0) as i64 / 65535) as i16;
                    let py =
                        (y.clamp(0, 65535) as i64 * (self.height - 1).max(0) as i64 / 65535) as i16;
                    self.fake(6, 0, px, py);
                }
                Event::Motion {
                    absolute: false,
                    x,
                    y,
                } => {
                    self.fake(
                        6,
                        1,
                        x.clamp(-32768, 32767) as i16,
                        y.clamp(-32768, 32767) as i16,
                    );
                }
                Event::Button { button, down } => {
                    self.fake(if down { 4 } else { 5 }, button, 0, 0);
                }
                Event::Wheel { horizontal, delta } => {
                    let slot = horizontal as usize;
                    let total = self.wheel_rest[slot] + delta;
                    let clicks = total / 120;
                    self.wheel_rest[slot] = total - clicks * 120;
                    let button = match (horizontal, clicks > 0) {
                        (false, true) => 4,
                        (false, false) => 5,
                        (true, true) => 7,
                        (true, false) => 6,
                    };
                    for _ in 0..clicks.unsigned_abs().min(64) {
                        self.fake(4, button, 0, 0);
                        self.fake(5, button, 0, 0);
                    }
                }
            }
        }
        // Aller-retour : le serveur a traité la saisie avant que le
        // programme ne lise ses messages.
        let _ = self.conn.sync();
    }
}

fn injector() -> &'static Mutex<Option<Injector>> {
    static I: OnceLock<Mutex<Option<Injector>>> = OnceLock::new();
    I.get_or_init(|| Mutex::new(None))
}

/// L'écran de verrouillage de cet affichage est-il montré ? (même marqueur
/// que scripts/winlux-lock.py : un pid vivant dans /run/winlux/lock/active).
pub fn session_locked() -> bool {
    let root = std::env::var("WINLUX_LOCK_ROOT").unwrap_or_default();
    let display = std::env::var("DISPLAY").unwrap_or_default();
    let number: String = display
        .rsplit(':')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let suffix = if number.is_empty() || number == "0" {
        String::new()
    } else {
        format!("-{number}")
    };
    let Ok(text) = std::fs::read_to_string(format!("{root}/run/winlux/lock/active{suffix}")) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<i32>() else {
        return false;
    };
    // SAFETY: kill(pid, 0) ne fait que vérifier l'existence du processus.
    let r = unsafe { libc::kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Ligne de commande (`/proc/<pid>/cmdline`, arguments séparés par NUL)
/// d'une fenêtre de consentement de Winlux : demande d'autorisation ou
/// contrôle de compte. Personne d'autre que la personne n'y répond.
pub fn is_consent_window(cmdline: &[u8]) -> bool {
    let args: Vec<&[u8]> = cmdline.split(|&b| b == 0).collect();
    let base = |a: &[u8]| -> Vec<u8> {
        let b = a.rsplit(|&c| c == b'/').next().unwrap_or(a);
        b.strip_suffix(b".py").unwrap_or(b).to_vec()
    };
    let Some(first) = args.first().map(|a| base(a)) else {
        return false;
    };
    // Script lancé par son interpréteur : son nom est le deuxième argument.
    let program = if first.starts_with(b"python") {
        args.get(1).map(|a| base(a)).unwrap_or_default()
    } else {
        first
    };
    program == b"winlux-uac" || (program == b"winlux-permission" && args.contains(&&b"ask"[..]))
}

/// Une fenêtre de consentement est-elle ouverte ? Un programme autorisé à
/// taper ne doit pas pouvoir répondre « Autoriser » à la place de la
/// personne, comme les fenêtres d'élévation de Windows échappent à SendInput.
pub fn consent_shown() -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return false;
    };
    dir.flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .any(|e| std::fs::read(e.path().join("cmdline")).is_ok_and(|c| is_consent_window(&c)))
}

/// Verrou partagé avec les fenêtres de consentement (#1696) :
/// `/tmp/winlux-consent-<affichage>.lock` (WINLUX_CONSENT_LOCK pour les
/// tests). Une fenêtre de consentement le prend en exclusif avant de
/// s'afficher et le garde jusqu'à sa fermeture ; la saisie le prend en
/// partagé, sans attendre, du contrôle jusqu'à la fin de l'envoi (après
/// l'aller-retour avec le serveur X). Une fenêtre ne peut donc pas s'ouvrir
/// au milieu d'un envoi, et aucun envoi ne commence tant qu'elle est là.
pub fn consent_lock_path() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("WINLUX_CONSENT_LOCK") {
        return p.into();
    }
    let display = std::env::var("DISPLAY").unwrap_or_default();
    let number: String = display
        .rsplit(':')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let number = if number.is_empty() {
        "0".into()
    } else {
        number
    };
    format!("/tmp/winlux-consent-{number}.lock").into()
}

/// Verrou partagé tenu pendant un envoi ; `None` : une fenêtre de
/// consentement est ouverte, ou le verrou appartient à un autre compte
/// (refus, jamais d'envoi sans garde).
pub fn consent_guard() -> Option<std::fs::File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::os::unix::io::AsRawFd;
    let path = consent_lock_path();
    // Jamais à travers un lien symbolique posé dans /tmp par un autre compte.
    let open = |write: bool| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(write)
            .create(write)
            .truncate(false)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
    };
    // Verrou créé par root (fenêtre d'élévation) : ouvert en lecture seule.
    let file = open(true).or_else(|_| open(false)).ok()?;
    let meta = file.metadata().ok()?;
    // SAFETY: getuid ne peut pas échouer.
    let me = unsafe { libc::getuid() };
    if !meta.is_file() || (meta.uid() != me && meta.uid() != 0) {
        return None;
    }
    // SAFETY: descripteur valide, tenu par `file`.
    let r = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) };
    (r == 0).then_some(file)
}

/// Tape les événements dans la session ; `Err(code)` : rien n'a été tapé.
pub fn inject(events: &[Event]) -> Result<(), u32> {
    if events.is_empty() {
        return Ok(());
    }
    let headless = std::env::var("WINLUX_GUI").is_ok_and(|v| v == "headless");
    if headless || std::env::var_os("DISPLAY").is_none() {
        crate::trace!("saisie simulée : aucun écran X, rien n'est tapé");
        return Err(ERROR_ACCESS_DENIED);
    }
    if session_locked() {
        crate::trace!("saisie simulée refusée : l'écran est verrouillé");
        return Err(ERROR_ACCESS_DENIED);
    }
    if !crate::permissions::allowed(crate::permissions::Capability::Input) {
        crate::trace!("saisie simulée refusée : autorisation « input » non accordée");
        return Err(ERROR_ACCESS_DENIED);
    }
    // Gardé jusqu'à la fin de la fonction, donc après l'aller-retour X.
    let Some(_consent) = consent_guard() else {
        crate::trace!("saisie simulée refusée : une fenêtre d'autorisation s'ouvre ou est ouverte");
        return Err(ERROR_ACCESS_DENIED);
    };
    if consent_shown() {
        crate::trace!("saisie simulée refusée : une fenêtre d'autorisation est ouverte");
        return Err(ERROR_ACCESS_DENIED);
    }
    let mut guard = super::lock(injector());
    if guard.is_none() {
        *guard = Injector::open();
    }
    let Some(inj) = guard.as_mut() else {
        crate::trace!("saisie simulée : extension XTEST absente");
        return Err(ERROR_ACCESS_DENIED);
    };
    inj.apply(events);
    crate::trace!("saisie simulée : {} événement(s) tapé(s)", events.len());
    Ok(())
}

/// SendInput(cInputs, pInputs, cbSize).
pub unsafe extern "win64" fn send_input(n: u32, inputs: *const u8, size: i32) -> u32 {
    if n == 0 || inputs.is_null() || size <= 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let size = size as usize;
    if size != INPUT_SIZE_64 && size != INPUT_SIZE_32 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    // SAFETY: le programme fournit n structures INPUT de cbSize octets.
    let bytes = std::slice::from_raw_parts(inputs, n as usize * size);
    let events = match parse(bytes, n as usize, size) {
        Ok(e) => e,
        Err(code) => {
            crate::process::set_last_error(code);
            return 0;
        }
    };
    match inject(&events) {
        Ok(()) => n,
        Err(code) => {
            crate::process::set_last_error(code);
            0
        }
    }
}

/// keybd_event(bVk, bScan, dwFlags, dwExtraInfo).
pub unsafe extern "win64" fn keybd_event(vk: u8, scan: u8, flags: u32, _extra: usize) {
    let _ = inject(&[keyboard_event(vk as u16, scan as u16, flags)]);
}

/// mouse_event : tapé dans la session ; `false` si rien n'a pu l'être.
pub fn mouse_event(flags: u32, dx: i32, dy: i32, data: u32) -> bool {
    let mut events = Vec::new();
    mouse_events(dx, dy, data, flags, &mut events);
    inject(&events).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input64(kind: u32, fields: &[(usize, u32)]) -> Vec<u8> {
        let mut b = vec![0u8; INPUT_SIZE_64];
        b[0..4].copy_from_slice(&kind.to_le_bytes());
        for &(o, v) in fields {
            b[8 + o..12 + o].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    fn key64(vk: u16, scan: u16, flags: u32) -> Vec<u8> {
        input64(
            INPUT_KEYBOARD,
            &[(0, vk as u32 | (scan as u32) << 16), (4, flags)],
        )
    }

    #[test]
    fn input_x64_et_x86() {
        let mut b = key64(0x41, 0, 0);
        b.extend(key64(0x41, 0, KEYEVENTF_KEYUP));
        let e = parse(&b, 2, 40).unwrap();
        assert_eq!(
            e,
            vec![
                Event::Key {
                    down: true,
                    key: Key::Vk {
                        vk: 0x41,
                        extended: false
                    }
                },
                Event::Key {
                    down: false,
                    key: Key::Vk {
                        vk: 0x41,
                        extended: false
                    }
                },
            ]
        );
        // x86 : union à 4, structure de 28 octets.
        let mut b = vec![0u8; 28];
        b[0..4].copy_from_slice(&INPUT_MOUSE.to_le_bytes());
        b[4..8].copy_from_slice(&10i32.to_le_bytes());
        b[8..12].copy_from_slice(&(-5i32).to_le_bytes());
        b[12..16].copy_from_slice(&(-240i32 as u32).to_le_bytes());
        b[16..20].copy_from_slice(
            &(MOUSEEVENTF_MOVE | MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_WHEEL).to_le_bytes(),
        );
        assert_eq!(
            parse(&b, 1, 28).unwrap(),
            vec![
                Event::Motion {
                    absolute: false,
                    x: 10,
                    y: -5
                },
                Event::Button {
                    button: 1,
                    down: true
                },
                Event::Wheel {
                    horizontal: false,
                    delta: -240
                },
            ]
        );
        assert_eq!(parse(&b, 1, 32), Err(ERROR_INVALID_PARAMETER));
        assert_eq!(parse(&b, 2, 28), Err(ERROR_INVALID_PARAMETER));
    }

    #[test]
    fn unicode_et_paires_de_substitution() {
        let mut b = key64(0, 0xD83D, KEYEVENTF_UNICODE);
        b.extend(key64(0, 0xDE00, KEYEVENTF_UNICODE));
        b.extend(key64(0, 0xE9, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        let e = parse(&b, 3, 40).unwrap();
        assert_eq!(
            e[0],
            Event::Key {
                down: true,
                key: Key::Char(0x1F600)
            }
        );
        assert_eq!(
            e[1],
            Event::Key {
                down: false,
                key: Key::Char(0xE9)
            }
        );
        assert_eq!(char_keysym(0xE9), Some(0xE9));
        assert_eq!(char_keysym(0x20AC), Some(0x0100_20AC));
        assert_eq!(char_keysym(0x0D), Some(0xFF0D));
        assert_eq!(char_keysym(0xD800), None);
    }

    #[test]
    fn boutons_lateraux_et_codes_de_balayage() {
        let mut e = Vec::new();
        mouse_events(0, 0, 2, MOUSEEVENTF_XDOWN, &mut e);
        assert_eq!(
            e,
            vec![Event::Button {
                button: 9,
                down: true
            }]
        );
        assert_eq!(scan_to_linux(0x1E, false), Some(30)); // A
        assert_eq!(scan_to_linux(0x48, true), Some(103)); // flèche haut
        assert_eq!(scan_to_linux(0xE01D, false), Some(97)); // Ctrl droit
        assert_eq!(scan_to_linux(0x48, false), Some(0x48)); // 8 du pavé
        assert_eq!(scan_to_linux(0x7F, false), None);
    }

    /// Disposition AZERTY réduite : la touche « a » est en 24, « & / 1 » en
    /// 10, Maj gauche en 50, un keycode libre en 200.
    fn azerty() -> Mapping {
        let per = 2;
        let mut syms = vec![0u32; (255 - 8 + 1) * per];
        let mut set = |code: usize, a: u32, b: u32| {
            syms[(code - 8) * per] = a;
            syms[(code - 8) * per + 1] = b;
        };
        set(24, 0x61, 0x41);
        set(38, 0x71, 0x51);
        set(10, 0x26, 0x31);
        set(11, 0xE9, 0x32);
        set(50, 0xFFE1, 0);
        set(36, 0xFF0D, 0);
        set(104, 0xFF8D, 0);
        Mapping { min: 8, per, syms }
    }

    #[test]
    fn touches_virtuelles_selon_la_disposition() {
        let m = azerty();
        assert_eq!(m.find_vk(0x41), Some(24)); // VK_A → touche « a »
        assert_eq!(m.find_vk(0x51), Some(38)); // VK_Q → touche « q »
        assert_eq!(m.find_vk(0x31), Some(10)); // VK_1 → « & / 1 »
        assert_eq!(m.find_base(vk_keysyms(0x0D, false)), Some(36));
        assert_eq!(m.find_base(vk_keysyms(0x0D, true)), Some(104));
        assert_eq!(m.find_base(vk_keysyms(0x10, false)), Some(50));
        assert_eq!(m.find_char(0xE9), Some((11, false))); // é sans Maj
        assert_eq!(m.find_char(0x32), Some((11, true))); // 2 avec Maj
        assert_eq!(m.find_char(0x41), Some((24, true))); // A avec Maj
        assert_eq!(m.find_char(0x20AC), None);
        assert!(m.spare().contains(&200));
        assert!(!m.spare().contains(&24));
        assert_eq!(vk_fallback_keysym(0x5A), Some(0x7A));
        assert_eq!(vk_keysyms(0xB3, false)[0], 0x1008_FF14);
    }

    #[test]
    fn fenetres_de_consentement() {
        assert!(is_consent_window(
            b"/usr/bin/python3\0/usr/bin/winlux-uac\0--app\0X\0"
        ));
        assert!(is_consent_window(
            b"/usr/bin/winlux-permission\0ask\0input\0A.exe\0/x\0"
        ));
        assert!(is_consent_window(
            b"python3\0scripts/winlux-permission.py\0ask\0files\0"
        ));
        assert!(!is_consent_window(b"/usr/bin/winlux-permission\0list\0"));
        assert!(!is_consent_window(b"/usr/bin/vim\0winlux-uac\0"));
        assert!(!is_consent_window(b""));
    }

    #[test]
    fn verrou_des_fenetres_de_consentement() {
        use std::os::unix::io::AsRawFd;
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path =
            std::env::temp_dir().join(format!("winlux-consent-test-{}.lock", std::process::id()));
        let old = std::env::var_os("WINLUX_CONSENT_LOCK");
        std::env::set_var("WINLUX_CONSENT_LOCK", &path);
        // Deux envois en même temps : verrous partagés compatibles.
        let a = consent_guard().expect("premier envoi");
        let b = consent_guard().expect("second envoi");
        drop((a, b));
        // Une fenêtre de consentement (verrou exclusif) bloque tout envoi.
        let window = std::fs::File::open(&path).unwrap();
        // SAFETY: descripteur valide.
        assert_eq!(
            unsafe { libc::flock(window.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(consent_guard().is_none());
        drop(window);
        assert!(
            consent_guard().is_some(),
            "fenêtre fermée : l'envoi reprend"
        );
        // Pendant un envoi, la fenêtre attend qu'il se termine.
        let sending = consent_guard().unwrap();
        let window = std::fs::File::open(&path).unwrap();
        // SAFETY: descripteur valide.
        assert_ne!(
            unsafe { libc::flock(window.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        drop(sending);
        // Lien symbolique posé à la place du verrou : refus, jamais suivi.
        let _ = std::fs::remove_file(&path);
        std::os::unix::fs::symlink(std::env::temp_dir(), &path).unwrap();
        assert!(
            consent_guard().is_none(),
            "un lien symbolique n'est jamais suivi"
        );
        match old {
            Some(v) => std::env::set_var("WINLUX_CONSENT_LOCK", v),
            None => std::env::remove_var("WINLUX_CONSENT_LOCK"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ecran_verrouille() {
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("winlux-saisie-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("run/winlux/lock")).unwrap();
        let old_root = std::env::var_os("WINLUX_LOCK_ROOT");
        let old_display = std::env::var_os("DISPLAY");
        std::env::set_var("WINLUX_LOCK_ROOT", &dir);
        std::env::set_var("DISPLAY", ":0");
        assert!(!session_locked());
        std::fs::write(
            dir.join("run/winlux/lock/active"),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        assert!(session_locked());
        std::env::set_var("DISPLAY", ":1");
        assert!(!session_locked(), "le marqueur de :0 ne verrouille pas :1");
        std::fs::write(dir.join("run/winlux/lock/active"), "999999999\n").unwrap();
        std::env::set_var("DISPLAY", ":0");
        assert!(!session_locked(), "un pid mort ne compte pas");
        match old_root {
            Some(v) => std::env::set_var("WINLUX_LOCK_ROOT", v),
            None => std::env::remove_var("WINLUX_LOCK_ROOT"),
        }
        match old_display {
            Some(v) => std::env::set_var("DISPLAY", v),
            None => std::env::remove_var("DISPLAY"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
