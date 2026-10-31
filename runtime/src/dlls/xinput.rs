//! xinput1_4.dll (et 1_3, 1_2, 1_1, 9_1_0) : manettes Xbox et compatibles,
//! lues via evdev. Jusqu'à quatre joueurs ; chaque manette garde son
//! emplacement tant qu'elle reste branchée.

use super::evdev::{self, Device, Snapshot};
use super::lock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const ERROR_SUCCESS: u32 = 0;
const ERROR_BAD_ARGUMENTS: u32 = 160;
const ERROR_EMPTY: u32 = 4306;
const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;

pub const XUSER_MAX_COUNT: usize = 4;
const XUSER_INDEX_ANY: u32 = 0xFF;

pub const DPAD_UP: u16 = 0x0001;
pub const DPAD_DOWN: u16 = 0x0002;
pub const DPAD_LEFT: u16 = 0x0004;
pub const DPAD_RIGHT: u16 = 0x0008;
pub const START: u16 = 0x0010;
pub const BACK: u16 = 0x0020;
pub const LEFT_THUMB: u16 = 0x0040;
pub const RIGHT_THUMB: u16 = 0x0080;
pub const LEFT_SHOULDER: u16 = 0x0100;
pub const RIGHT_SHOULDER: u16 = 0x0200;
pub const GUIDE: u16 = 0x0400;
pub const A: u16 = 0x1000;
pub const B: u16 = 0x2000;
pub const X: u16 = 0x4000;
pub const Y: u16 = 0x8000;

const TRIGGER_THRESHOLD: u8 = 30;

/// `XINPUT_GAMEPAD`.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Gamepad {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

#[repr(C)]
struct XState {
    packet: u32,
    gamepad: Gamepad,
}

#[repr(C)]
struct Vibration {
    left: u16,
    right: u16,
}

#[repr(C)]
struct Capabilities {
    ty: u8,
    sub_type: u8,
    flags: u16,
    gamepad: Gamepad,
    vibration: Vibration,
}

#[repr(C)]
struct Keystroke {
    virtual_key: u16,
    unicode: u16,
    flags: u16,
    user_index: u8,
    hid_code: u8,
}

fn axis_i16(v: f32) -> i16 {
    (v * 32767.5 - 0.5).round().clamp(-32768.0, 32767.0) as i16
}

fn trigger_u8(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Traduit l'état evdev en `XINPUT_GAMEPAD` (mise en page Xbox).
pub fn map_gamepad(s: &Snapshot, axes: &[usize], positional: bool) -> Gamepad {
    use evdev::*;
    let mut b = 0u16;
    let (north, west) = if positional { (Y, X) } else { (X, Y) };
    for (code, mask) in [
        (BTN_SOUTH, A),
        (BTN_EAST, B),
        (BTN_NORTH, north),
        (BTN_WEST, west),
        (BTN_TL, LEFT_SHOULDER),
        (BTN_TR, RIGHT_SHOULDER),
        (BTN_SELECT, BACK),
        (BTN_START, START),
        (BTN_MODE, GUIDE),
        (BTN_THUMBL, LEFT_THUMB),
        (BTN_THUMBR, RIGHT_THUMB),
        (BTN_DPAD_UP, DPAD_UP),
        (BTN_DPAD_DOWN, DPAD_DOWN),
        (BTN_DPAD_LEFT, DPAD_LEFT),
        (BTN_DPAD_RIGHT, DPAD_RIGHT),
    ] {
        if s.key(code) {
            b |= mask;
        }
    }
    let has = |a: usize| axes.contains(&a);
    if has(ABS_HAT0X) {
        let v = s.abs[ABS_HAT0X].value;
        if v < 0 {
            b |= DPAD_LEFT;
        } else if v > 0 {
            b |= DPAD_RIGHT;
        }
    }
    if has(ABS_HAT0Y) {
        let v = s.abs[ABS_HAT0Y].value;
        if v < 0 {
            b |= DPAD_UP;
        } else if v > 0 {
            b |= DPAD_DOWN;
        }
    }
    let stick = |a: usize, invert: bool| {
        if !has(a) {
            return 0;
        }
        let v = s.abs[a].signed();
        axis_i16(if invert { -v } else { v })
    };
    let trigger = |a: usize, digital: u16| {
        if has(a) {
            trigger_u8(s.abs[a].unsigned())
        } else if s.key(digital) {
            255
        } else {
            0
        }
    };
    Gamepad {
        buttons: b,
        left_trigger: trigger(ABS_Z, BTN_TL2),
        right_trigger: trigger(ABS_RZ, BTN_TR2),
        // Linux : Y positif vers le bas ; XInput : vers le haut.
        thumb_lx: stick(ABS_X, false),
        thumb_ly: stick(ABS_Y, true),
        thumb_rx: stick(ABS_RX, false),
        thumb_ry: stick(ABS_RY, true),
    }
}

struct Slot {
    device: Device,
    last: Gamepad,
    packet: u32,
    /// Boutons déjà signalés par XInputGetKeystroke.
    keystroke_buttons: u32,
}

struct Pads {
    slots: [Option<Slot>; XUSER_MAX_COUNT],
    last_scan: Option<Instant>,
}

fn pads() -> &'static Mutex<Pads> {
    static P: OnceLock<Mutex<Pads>> = OnceLock::new();
    P.get_or_init(|| {
        Mutex::new(Pads {
            slots: [None, None, None, None],
            last_scan: None,
        })
    })
}

static ENABLED: AtomicBool = AtomicBool::new(true);

impl Pads {
    /// Ajoute les manettes nouvellement branchées (au plus toutes les 2 s,
    /// ou tout de suite après un débranchement).
    fn refresh(&mut self, force: bool) {
        let due = self
            .last_scan
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(2));
        if !force && !due {
            return;
        }
        self.last_scan = Some(Instant::now());
        if self.slots.iter().all(Option::is_some) {
            return;
        }
        for path in evdev::event_paths() {
            if self.slots.iter().flatten().any(|s| s.device.path == path) {
                continue;
            }
            let Some(free) = self.slots.iter().position(Option::is_none) else {
                break;
            };
            if let Some(device) = Device::open(&path) {
                if device.kind == evdev::Kind::Gamepad {
                    self.slots[free] = Some(Slot {
                        device,
                        last: Gamepad::default(),
                        packet: 0,
                        keystroke_buttons: 0,
                    });
                }
            }
        }
    }

    /// État courant de la manette `i` ; libère l'emplacement si débranchée.
    fn poll(&mut self, i: usize) -> Option<(u32, Gamepad)> {
        if self.slots[i].is_none() {
            self.refresh(false);
        }
        let slot = self.slots[i].as_mut()?;
        let Some(snap) = slot.device.snapshot() else {
            self.slots[i] = None;
            self.last_scan = None;
            return None;
        };
        let positional = evdev::positional_face_buttons(slot.device.vendor);
        let pad = map_gamepad(&snap, &slot.device.axes, positional);
        if pad != slot.last {
            slot.last = pad;
            slot.packet = slot.packet.wrapping_add(1);
        }
        Some((slot.packet, pad))
    }
}

unsafe fn get_state(index: u32, out: *mut XState, with_guide: bool) -> u32 {
    if index as usize >= XUSER_MAX_COUNT || out.is_null() {
        return ERROR_BAD_ARGUMENTS;
    }
    let Some((packet, mut pad)) = lock(pads()).poll(index as usize) else {
        return ERROR_DEVICE_NOT_CONNECTED;
    };
    if !with_guide {
        pad.buttons &= !GUIDE;
    }
    if !ENABLED.load(Ordering::Relaxed) {
        // XInputEnable(FALSE) : état neutre, manette toujours « connectée ».
        pad = Gamepad::default();
    }
    *out = XState {
        packet,
        gamepad: pad,
    };
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn xinput_get_state(index: u32, out: *mut u8) -> u32 {
    get_state(index, out as *mut XState, false)
}

/// Ordinal 100 (non documenté) : identique mais avec le bouton Guide.
pub unsafe extern "win64" fn xinput_get_state_ex(index: u32, out: *mut u8) -> u32 {
    get_state(index, out as *mut XState, true)
}

pub unsafe extern "win64" fn xinput_set_state(index: u32, vibration: *const u8) -> u32 {
    if index as usize >= XUSER_MAX_COUNT || vibration.is_null() {
        return ERROR_BAD_ARGUMENTS;
    }
    let v = &*(vibration as *const Vibration);
    let mut p = lock(pads());
    if p.poll(index as usize).is_none() {
        return ERROR_DEVICE_NOT_CONNECTED;
    }
    if let Some(slot) = &p.slots[index as usize] {
        let (l, r) = if ENABLED.load(Ordering::Relaxed) {
            (v.left, v.right)
        } else {
            (0, 0)
        };
        // Moteur gauche = basse fréquence (fort), droit = haute fréquence.
        slot.device.rumble(l, r);
    }
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn xinput_get_capabilities(index: u32, flags: u32, out: *mut u8) -> u32 {
    if index as usize >= XUSER_MAX_COUNT || out.is_null() || flags & !1 != 0 {
        return ERROR_BAD_ARGUMENTS;
    }
    let mut p = lock(pads());
    if p.poll(index as usize).is_none() {
        return ERROR_DEVICE_NOT_CONNECTED;
    }
    let rumble = p.slots[index as usize]
        .as_ref()
        .is_some_and(|s| s.device.rumble);
    let wireless = p.slots[index as usize]
        .as_ref()
        .is_some_and(|s| s.device.bustype == 0x05); // BUS_BLUETOOTH
    *(out as *mut Capabilities) = Capabilities {
        ty: 1,       // XINPUT_DEVTYPE_GAMEPAD
        sub_type: 1, // XINPUT_DEVSUBTYPE_GAMEPAD
        flags: if rumble { 0x0001 } else { 0 } | if wireless { 0x0002 } else { 0 },
        gamepad: Gamepad {
            buttons: 0xF3FF,
            left_trigger: 0xFF,
            right_trigger: 0xFF,
            thumb_lx: -64,
            thumb_ly: -64,
            thumb_rx: -64,
            thumb_ry: -64,
        },
        vibration: Vibration {
            left: if rumble { 0xFFFF } else { 0 },
            right: if rumble { 0xFFFF } else { 0 },
        },
    };
    ERROR_SUCCESS
}

pub extern "win64" fn xinput_enable(enable: i32) {
    ENABLED.store(enable != 0, Ordering::Relaxed);
    if enable == 0 {
        let p = lock(pads());
        for slot in p.slots.iter().flatten() {
            slot.device.rumble(0, 0);
        }
    }
}

pub unsafe extern "win64" fn xinput_get_battery_information(
    index: u32,
    _dev_type: u8,
    out: *mut u8,
) -> u32 {
    if index as usize >= XUSER_MAX_COUNT || out.is_null() {
        return ERROR_BAD_ARGUMENTS;
    }
    let mut p = lock(pads());
    if p.poll(index as usize).is_none() {
        *out = 0; // BATTERY_TYPE_DISCONNECTED
        *out.add(1) = 0;
        return ERROR_DEVICE_NOT_CONNECTED;
    }
    let wireless = p.slots[index as usize]
        .as_ref()
        .is_some_and(|s| s.device.bustype == 0x05);
    let level = if wireless {
        battery_level(index as usize)
    } else {
        3
    };
    *out = if wireless { 2 } else { 1 }; // ALKALINE / WIRED
    *out.add(1) = level;
    ERROR_SUCCESS
}

/// Niveau de batterie d'une manette sans fil, lu dans power_supply.
fn battery_level(_index: usize) -> u8 {
    let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else {
        return 3;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !(name.contains("controller")
            || name.starts_with("ps-controller")
            || name.contains("hid"))
        {
            continue;
        }
        if let Ok(c) = std::fs::read_to_string(e.path().join("capacity")) {
            let c: u32 = c.trim().parse().unwrap_or(100);
            return match c {
                0..=10 => 0,
                11..=40 => 1,
                41..=70 => 2,
                _ => 3,
            };
        }
    }
    3
}

const VK_PAD: [(u32, u16); 18] = [
    (A as u32, 0x5800),
    (B as u32, 0x5801),
    (X as u32, 0x5802),
    (Y as u32, 0x5803),
    (RIGHT_SHOULDER as u32, 0x5804),
    (LEFT_SHOULDER as u32, 0x5805),
    (0x1_0000, 0x5806), // gâchette gauche
    (0x2_0000, 0x5807), // gâchette droite
    (DPAD_UP as u32, 0x5810),
    (DPAD_DOWN as u32, 0x5811),
    (DPAD_LEFT as u32, 0x5812),
    (DPAD_RIGHT as u32, 0x5813),
    (START as u32, 0x5814),
    (BACK as u32, 0x5815),
    (LEFT_THUMB as u32, 0x5816),
    (RIGHT_THUMB as u32, 0x5817),
    (GUIDE as u32, 0x5818),
    (0, 0),
];

/// Boutons + gâchettes (bits 16 et 17) vus comme des touches.
fn key_bits(p: &Gamepad) -> u32 {
    let mut bits = p.buttons as u32;
    if p.left_trigger > TRIGGER_THRESHOLD {
        bits |= 0x1_0000;
    }
    if p.right_trigger > TRIGGER_THRESHOLD {
        bits |= 0x2_0000;
    }
    bits
}

/// Premier changement de bouton depuis le dernier appel : (VK, KEYDOWN/KEYUP).
pub fn next_keystroke(previous: &mut u32, current: u32) -> Option<(u16, u16)> {
    for &(mask, vk) in VK_PAD.iter().take_while(|(m, _)| *m != 0) {
        let was = *previous & mask != 0;
        let now = current & mask != 0;
        if was != now {
            *previous ^= mask;
            return Some((vk, if now { 1 } else { 2 }));
        }
    }
    None
}

pub unsafe extern "win64" fn xinput_get_keystroke(index: u32, _reserved: u32, out: *mut u8) -> u32 {
    if out.is_null() || (index as usize >= XUSER_MAX_COUNT && index != XUSER_INDEX_ANY) {
        return ERROR_BAD_ARGUMENTS;
    }
    let mut p = lock(pads());
    let users: Vec<usize> = if index == XUSER_INDEX_ANY {
        (0..XUSER_MAX_COUNT).collect()
    } else {
        vec![index as usize]
    };
    let mut any = false;
    for i in users {
        let Some((_, pad)) = p.poll(i) else { continue };
        any = true;
        let Some(slot) = p.slots[i].as_mut() else {
            continue;
        };
        if let Some((vk, flags)) = next_keystroke(&mut slot.keystroke_buttons, key_bits(&pad)) {
            *(out as *mut Keystroke) = Keystroke {
                virtual_key: vk,
                unicode: 0,
                flags,
                user_index: i as u8,
                hid_code: 0,
            };
            return ERROR_SUCCESS;
        }
    }
    if any {
        ERROR_EMPTY
    } else {
        ERROR_DEVICE_NOT_CONNECTED
    }
}

pub unsafe extern "win64" fn xinput_get_audio_device_ids(
    index: u32,
    render: *mut u16,
    render_count: *mut u32,
    capture: *mut u16,
    capture_count: *mut u32,
) -> u32 {
    if index as usize >= XUSER_MAX_COUNT {
        return ERROR_BAD_ARGUMENTS;
    }
    // Aucun casque branché sur la manette : chaînes vides.
    for (s, n) in [(render, render_count), (capture, capture_count)] {
        if !s.is_null() && !n.is_null() && *n > 0 {
            *s = 0;
        }
        if !n.is_null() {
            *n = 0;
        }
    }
    if lock(pads()).poll(index as usize).is_some() {
        ERROR_SUCCESS
    } else {
        ERROR_DEVICE_NOT_CONNECTED
    }
}

pub unsafe extern "win64" fn xinput_get_dsound_audio_device_guids(
    index: u32,
    render: *mut [u8; 16],
    capture: *mut [u8; 16],
) -> u32 {
    if index as usize >= XUSER_MAX_COUNT {
        return ERROR_BAD_ARGUMENTS;
    }
    for g in [render, capture] {
        if !g.is_null() {
            *g = [0; 16]; // GUID_NULL : pas de casque
        }
    }
    if lock(pads()).poll(index as usize).is_some() {
        ERROR_SUCCESS
    } else {
        ERROR_DEVICE_NOT_CONNECTED
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "XInputGetState" => fn xinput_get_state,
        "#100" => fn xinput_get_state_ex,
        "XInputSetState" => fn xinput_set_state,
        "XInputGetCapabilities" => fn xinput_get_capabilities,
        "XInputEnable" => fn xinput_enable,
        "XInputGetBatteryInformation" => fn xinput_get_battery_information,
        "XInputGetKeystroke" => fn xinput_get_keystroke,
        "XInputGetAudioDeviceIds" => fn xinput_get_audio_device_ids,
        "XInputGetDSoundAudioDeviceGuids" => fn xinput_get_dsound_audio_device_guids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::evdev::{self as ev, AbsInfo};

    fn stick(value: i32) -> AbsInfo {
        AbsInfo {
            value,
            min: -32768,
            max: 32767,
            flat: 128,
        }
    }

    #[test]
    fn traduit_une_manette_xbox_evdev() {
        let mut s = Snapshot::default();
        s.set_key(ev::BTN_SOUTH, true);
        s.set_key(ev::BTN_NORTH, true);
        s.set_key(ev::BTN_START, true);
        s.abs[ev::ABS_X] = stick(32767);
        s.abs[ev::ABS_Y] = stick(-32768); // poussé vers le haut
        s.abs[ev::ABS_Z] = AbsInfo {
            value: 1023,
            min: 0,
            max: 1023,
            flat: 0,
        };
        s.abs[ev::ABS_HAT0X] = AbsInfo {
            value: -1,
            min: -1,
            max: 1,
            flat: 0,
        };
        let axes = [ev::ABS_X, ev::ABS_Y, ev::ABS_Z, ev::ABS_HAT0X];
        let p = map_gamepad(&s, &axes, false);
        assert_eq!(p.buttons, A | X | START | DPAD_LEFT);
        assert_eq!(p.thumb_lx, 32767);
        assert_eq!(p.thumb_ly, 32767);
        assert_eq!(p.left_trigger, 255);
        assert_eq!(p.right_trigger, 0);
        // DualSense : le bouton du haut (triangle) est Y.
        assert_eq!(map_gamepad(&s, &axes, true).buttons & (X | Y), Y);
    }

    #[test]
    fn gachettes_numeriques_sans_axe() {
        let mut s = Snapshot::default();
        s.set_key(ev::BTN_TR2, true);
        let p = map_gamepad(&s, &[], false);
        assert_eq!((p.left_trigger, p.right_trigger), (0, 255));
    }

    #[test]
    fn frappes_de_touches_successives() {
        let mut prev = 0;
        assert_eq!(
            next_keystroke(&mut prev, A as u32 | B as u32),
            Some((0x5800, 1))
        );
        assert_eq!(
            next_keystroke(&mut prev, A as u32 | B as u32),
            Some((0x5801, 1))
        );
        assert_eq!(next_keystroke(&mut prev, A as u32 | B as u32), None);
        assert_eq!(next_keystroke(&mut prev, B as u32), Some((0x5800, 2)));
    }

    #[test]
    fn manette_absente_et_arguments_invalides() {
        std::env::set_var("WINLUX_INPUT_DIR", "/nonexistent-winlux-input");
        let mut state = [0u8; 16];
        unsafe {
            assert_eq!(
                xinput_get_state(0, state.as_mut_ptr()),
                ERROR_DEVICE_NOT_CONNECTED
            );
            assert_eq!(xinput_get_state(4, state.as_mut_ptr()), ERROR_BAD_ARGUMENTS);
            let mut caps = [0u8; 20];
            assert_eq!(
                xinput_get_capabilities(3, 0, caps.as_mut_ptr()),
                ERROR_DEVICE_NOT_CONNECTED
            );
        }
        assert_eq!(std::mem::size_of::<XState>(), 16);
        assert_eq!(std::mem::size_of::<Capabilities>(), 20);
        assert_eq!(std::mem::size_of::<Keystroke>(), 8);
    }
}
