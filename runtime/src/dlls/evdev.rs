//! Accès aux manettes, joysticks et volants via l'interface evdev de Linux
//! (`/dev/input/event*`). Partagé par XInput et DirectInput.
//!
//! L'état est lu sans consommer la file d'événements : `EVIOCGKEY` donne
//! les boutons enfoncés et `EVIOCGABS` la position courante de chaque axe.
//! Les vibrations passent par le retour de force du noyau (`EVIOCSFF`).

use std::ffi::c_void;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

// ---------- Constantes de linux/input-event-codes.h ----------
pub const EV_KEY: u32 = 0x01;
pub const EV_ABS: u32 = 0x03;
pub const EV_FF: u32 = 0x15;

pub const ABS_X: usize = 0x00;
pub const ABS_Y: usize = 0x01;
pub const ABS_Z: usize = 0x02;
pub const ABS_RX: usize = 0x03;
pub const ABS_RY: usize = 0x04;
pub const ABS_RZ: usize = 0x05;
pub const ABS_THROTTLE: usize = 0x06;
pub const ABS_RUDDER: usize = 0x07;
pub const ABS_WHEEL: usize = 0x08;
pub const ABS_GAS: usize = 0x09;
pub const ABS_BRAKE: usize = 0x0a;
pub const ABS_HAT0X: usize = 0x10;
pub const ABS_HAT0Y: usize = 0x11;
pub const ABS_CNT: usize = 0x40;

pub const BTN_MISC: u16 = 0x100;
pub const BTN_JOYSTICK: u16 = 0x120;
pub const BTN_SOUTH: u16 = 0x130;
pub const BTN_EAST: u16 = 0x131;
pub const BTN_NORTH: u16 = 0x133;
pub const BTN_WEST: u16 = 0x134;
pub const BTN_TL: u16 = 0x136;
pub const BTN_TR: u16 = 0x137;
pub const BTN_TL2: u16 = 0x138;
pub const BTN_TR2: u16 = 0x139;
pub const BTN_SELECT: u16 = 0x13a;
pub const BTN_START: u16 = 0x13b;
pub const BTN_MODE: u16 = 0x13c;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;
const BTN_TOOL_PEN: u16 = 0x140;
const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;
pub const BTN_DPAD_UP: u16 = 0x220;
pub const BTN_DPAD_DOWN: u16 = 0x221;
pub const BTN_DPAD_LEFT: u16 = 0x222;
pub const BTN_DPAD_RIGHT: u16 = 0x223;
pub const BTN_TRIGGER_HAPPY: u16 = 0x2c0;
pub const KEY_CNT: usize = 0x300;
const FF_RUMBLE: usize = 0x50;
const FF_CNT: usize = 0x80;

const KEY_BYTES: usize = KEY_CNT / 8;

// ---------- ioctl (x86_64, vérifiés avec linux/input.h) ----------
const fn ioc_read(nr: u32, size: u32) -> u64 {
    ((2u64) << 30) | ((size as u64) << 16) | (0x45 << 8) | nr as u64
}
const EVIOCGID: u64 = ioc_read(0x02, 8);
const fn eviocgname(len: u32) -> u64 {
    ioc_read(0x06, len)
}
const fn eviocgkey(len: u32) -> u64 {
    ioc_read(0x18, len)
}
const fn eviocgbit(ev: u32, len: u32) -> u64 {
    ioc_read(0x20 + ev, len)
}
const fn eviocgabs(abs: u32) -> u64 {
    ioc_read(0x40 + abs, 24)
}
/// `_IOW('E', 0x80, struct ff_effect)` avec `sizeof(ff_effect) == 48`.
const EVIOCSFF: u64 = 0x4030_4580;

/// Plage et valeur d'un axe absolu (`struct input_absinfo`).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct AbsInfo {
    pub value: i32,
    pub min: i32,
    pub max: i32,
    pub flat: i32,
}

impl AbsInfo {
    /// Position normalisée dans [-1, 1] (0 au centre de la plage).
    pub fn signed(&self) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        let span = (self.max - self.min) as f32;
        (((self.value - self.min) as f32 / span) * 2.0 - 1.0).clamp(-1.0, 1.0)
    }
    /// Position normalisée dans [0, 1] (gâchettes, pédales).
    pub fn unsigned(&self) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        ((self.value - self.min) as f32 / (self.max - self.min) as f32).clamp(0.0, 1.0)
    }
}

/// Famille de périphérique de jeu, déduite des capacités evdev.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gamepad,
    Joystick,
    Wheel,
}

/// État instantané : boutons (bitmap des codes KEY_*/BTN_*) et axes.
#[derive(Clone)]
pub struct Snapshot {
    pub keys: [u8; KEY_BYTES],
    pub abs: [AbsInfo; ABS_CNT],
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            keys: [0; KEY_BYTES],
            abs: [AbsInfo::default(); ABS_CNT],
        }
    }
}

impl Snapshot {
    pub fn key(&self, code: u16) -> bool {
        bit(&self.keys, code as usize)
    }
    #[cfg(test)]
    pub fn set_key(&mut self, code: u16, down: bool) {
        let (i, m) = (code as usize / 8, 1u8 << (code % 8));
        if i < self.keys.len() {
            if down {
                self.keys[i] |= m;
            } else {
                self.keys[i] &= !m;
            }
        }
    }
}

fn bit(bits: &[u8], n: usize) -> bool {
    bits.get(n / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
}

/// Un périphérique evdev ouvert et ses capacités.
pub struct Device {
    pub path: PathBuf,
    fd: OwnedFd,
    writable: bool,
    pub name: String,
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    /// Codes de boutons pris en charge (≥ BTN_MISC), triés.
    pub buttons: Vec<u16>,
    /// Axes absolus pris en charge, triés.
    pub axes: Vec<usize>,
    pub rumble: bool,
    pub kind: Kind,
    /// Effet de vibration téléversé dans le noyau (-1 = aucun).
    effect_id: std::sync::atomic::AtomicI32,
}

impl Device {
    /// Ouvre `path` et le garde seulement s'il s'agit d'une manette,
    /// d'un joystick ou d'un volant.
    pub fn open(path: &Path) -> Option<Device> {
        let (fd, writable) = open_fd(path)?;
        let raw = fd.as_raw_fd();
        let mut keys = [0u8; KEY_BYTES];
        let mut absbits = [0u8; ABS_CNT / 8];
        let mut ffbits = [0u8; FF_CNT / 8];
        unsafe {
            if libc::ioctl(
                raw,
                eviocgbit(EV_KEY, KEY_BYTES as u32) as _,
                keys.as_mut_ptr(),
            ) < 0
            {
                return None;
            }
            libc::ioctl(
                raw,
                eviocgbit(EV_ABS, absbits.len() as u32) as _,
                absbits.as_mut_ptr(),
            );
            libc::ioctl(
                raw,
                eviocgbit(EV_FF, ffbits.len() as u32) as _,
                ffbits.as_mut_ptr(),
            );
        }
        let buttons: Vec<u16> = (BTN_MISC as usize..KEY_CNT)
            .filter(|&c| bit(&keys, c))
            .map(|c| c as u16)
            .collect();
        let axes: Vec<usize> = (0..ABS_CNT).filter(|&a| bit(&absbits, a)).collect();
        let mut name_buf = [0u8; 256];
        let n = unsafe {
            libc::ioctl(
                raw,
                eviocgname(name_buf.len() as u32) as _,
                name_buf.as_mut_ptr(),
            )
        };
        let name = if n > 0 {
            let end = name_buf.iter().position(|&b| b == 0).unwrap_or(n as usize);
            String::from_utf8_lossy(&name_buf[..end]).into_owned()
        } else {
            String::from("Contr\u{f4}leur de jeu")
        };
        let kind = classify(&name, &buttons, &axes)?;
        let mut id = [0u16; 4];
        unsafe { libc::ioctl(raw, EVIOCGID as _, id.as_mut_ptr()) };
        Some(Device {
            path: path.to_path_buf(),
            fd,
            writable,
            name,
            bustype: id[0],
            vendor: id[1],
            product: id[2],
            buttons,
            axes,
            rumble: bit(&ffbits, FF_RUMBLE),
            kind,
            effect_id: std::sync::atomic::AtomicI32::new(-1),
        })
    }

    /// Lit l'état courant. `None` si la manette a été débranchée.
    pub fn snapshot(&self) -> Option<Snapshot> {
        let raw = self.fd.as_raw_fd();
        let mut s = Snapshot::default();
        unsafe {
            if libc::ioctl(raw, eviocgkey(KEY_BYTES as u32) as _, s.keys.as_mut_ptr()) < 0 {
                return None;
            }
            for &a in &self.axes {
                let mut info = [0i32; 6];
                if libc::ioctl(raw, eviocgabs(a as u32) as _, info.as_mut_ptr()) < 0 {
                    return None;
                }
                s.abs[a] = AbsInfo {
                    value: info[0],
                    min: info[1],
                    max: info[2],
                    flat: info[4],
                };
            }
        }
        Some(s)
    }

    /// Vibration double moteur (0..=65535 chacun). (0, 0) arrête l'effet.
    pub fn rumble(&self, strong: u16, weak: u16) -> bool {
        if !self.rumble || !self.writable {
            return false;
        }
        use std::sync::atomic::Ordering;
        let raw = self.fd.as_raw_fd();
        let id = self.effect_id.load(Ordering::Relaxed);
        if strong == 0 && weak == 0 {
            if id >= 0 {
                write_event(raw, EV_FF as u16, id as u16, 0);
            }
            return true;
        }
        // struct ff_effect : type, id, direction, trigger, replay, union.
        let mut effect = [0u8; 48];
        effect[0..2].copy_from_slice(&(FF_RUMBLE as u16).to_ne_bytes());
        effect[2..4].copy_from_slice(&(id as i16).to_ne_bytes());
        // replay.length = 0 : durée infinie jusqu'au prochain appel.
        effect[16..18].copy_from_slice(&strong.to_ne_bytes());
        effect[18..20].copy_from_slice(&weak.to_ne_bytes());
        if unsafe { libc::ioctl(raw, EVIOCSFF as _, effect.as_mut_ptr()) } < 0 {
            return false;
        }
        let new_id = i16::from_ne_bytes([effect[2], effect[3]]) as i32;
        self.effect_id.store(new_id, Ordering::Relaxed);
        write_event(raw, EV_FF as u16, new_id as u16, 1)
    }

    /// Identifiant stable dérivé du chemin (`event12` → 12).
    pub fn event_number(&self) -> u32 {
        event_number(&self.path).unwrap_or(0)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.rumble(0, 0);
    }
}

fn open_fd(path: &Path) -> Option<(OwnedFd, bool)> {
    use std::os::unix::fs::OpenOptionsExt;
    let rw = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path);
    match rw {
        Ok(f) => Some((f.into(), true)),
        Err(_) => std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .ok()
            .map(|f| (f.into(), false)),
    }
}

fn write_event(fd: i32, ty: u16, code: u16, value: i32) -> bool {
    // struct input_event : timeval (16 octets), type, code, value.
    let mut ev = [0u8; 24];
    ev[16..18].copy_from_slice(&ty.to_ne_bytes());
    ev[18..20].copy_from_slice(&code.to_ne_bytes());
    ev[20..24].copy_from_slice(&value.to_ne_bytes());
    unsafe { libc::write(fd, ev.as_ptr() as *const c_void, ev.len()) == ev.len() as isize }
}

/// Décide si les capacités décrivent un contrôleur de jeu. Les souris,
/// pavés tactiles, tablettes et capteurs de mouvement sont écartés.
pub fn classify(name: &str, buttons: &[u16], axes: &[usize]) -> Option<Kind> {
    let has = |c: u16| buttons.binary_search(&c).is_ok();
    if has(BTN_TOUCH) || has(BTN_TOOL_FINGER) || has(BTN_TOOL_PEN) {
        return None;
    }
    let game_buttons = buttons.iter().any(|&b| {
        (BTN_JOYSTICK..=BTN_THUMBR).contains(&b)
            || (BTN_TRIGGER_HAPPY..BTN_TRIGGER_HAPPY + 0x28).contains(&b)
    });
    if !game_buttons {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let wheel = axes.contains(&ABS_WHEEL)
        || axes.contains(&ABS_GAS)
        || axes.contains(&ABS_BRAKE)
        || lower.contains("wheel")
        || lower.contains("racing");
    if wheel {
        Some(Kind::Wheel)
    } else if has(BTN_SOUTH) {
        Some(Kind::Gamepad)
    } else if axes.contains(&ABS_X) || has(BTN_JOYSTICK) {
        Some(Kind::Joystick)
    } else {
        None
    }
}

fn event_number(path: &Path) -> Option<u32> {
    path.file_name()?
        .to_str()?
        .strip_prefix("event")?
        .parse()
        .ok()
}

/// Dossier des périphériques d'entrée ; `WINLUX_INPUT_DIR` permet aux
/// tests de pointer ailleurs.
pub fn input_dir() -> PathBuf {
    std::env::var_os("WINLUX_INPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev/input"))
}

/// Chemins `eventN` présents, triés par numéro.
pub fn event_paths() -> Vec<PathBuf> {
    let mut paths: Vec<(u32, PathBuf)> = std::fs::read_dir(input_dir())
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    event_number(&p).map(|n| (n, p))
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths.into_iter().map(|(_, p)| p).collect()
}

/// Tous les contrôleurs de jeu branchés.
pub fn scan() -> Vec<Device> {
    event_paths()
        .iter()
        .filter_map(|p| Device::open(p))
        .collect()
}

/// Les fabricants dont le bouton « nord » (BTN_NORTH) est vraiment celui du
/// haut ; le pilote xpad, lui, envoie BTN_X (même code) pour le X d'Xbox.
pub fn positional_face_buttons(vendor: u16) -> bool {
    matches!(vendor, 0x054c | 0x057e) // Sony, Nintendo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classe_manettes_joysticks_volants_et_ecarte_les_souris() {
        let pad = [BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR];
        assert_eq!(
            classify("Xbox Wireless Controller", &pad, &[ABS_X, ABS_Y]),
            Some(Kind::Gamepad)
        );
        assert_eq!(
            classify(
                "Thrustmaster T.16000M",
                &[BTN_JOYSTICK, 0x121],
                &[ABS_X, ABS_Y]
            ),
            Some(Kind::Joystick)
        );
        assert_eq!(
            classify(
                "Logitech G29 Driving Force Racing Wheel",
                &[BTN_JOYSTICK],
                &[ABS_X]
            ),
            Some(Kind::Wheel)
        );
        // Souris (BTN_LEFT = 0x110) et pavé tactile de DualSense.
        assert_eq!(classify("Mouse", &[0x110, 0x111], &[]), None);
        assert_eq!(
            classify(
                "DualSense Touchpad",
                &[0x110, BTN_TOOL_FINGER, BTN_TOUCH],
                &[ABS_X]
            ),
            None
        );
        assert_eq!(
            classify("Motion Sensors", &[], &[ABS_X, ABS_Y, ABS_Z]),
            None
        );
    }

    #[test]
    fn normalise_les_axes() {
        let a = AbsInfo {
            value: 128,
            min: 0,
            max: 255,
            flat: 0,
        };
        assert!(a.signed().abs() < 0.01);
        let t = AbsInfo {
            value: 1023,
            min: 0,
            max: 1023,
            flat: 0,
        };
        assert_eq!(t.unsigned(), 1.0);
        assert_eq!(AbsInfo::default().signed(), 0.0);
    }

    #[test]
    fn dossier_vide_ne_donne_aucun_peripherique() {
        let dir = std::env::temp_dir().join(format!("winlux-evdev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("event0"), b"pas un peripherique").unwrap();
        assert!(Device::open(&dir.join("event0")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
