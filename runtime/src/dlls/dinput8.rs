//! dinput8.dll et dinput.dll : DirectInput 8 (et 3 à 7) pour manettes,
//! joysticks, volants, clavier et souris.
//!
//! Les contrôleurs de jeu viennent d'evdev ; le clavier et la souris de
//! l'état brut tenu par le serveur graphique (codes de balayage physiques
//! et déplacements relatifs). Le format de données choisi par le jeu
//! (`SetDataFormat`, par ex. `c_dfDIJoystick2`) est interprété de façon
//! générique, objet par objet, comme le fait DirectInput.

use super::comobj::{self, guid, Guid, VTable, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK};
use super::evdev::{self, Device, Snapshot};
use super::lock;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

// ---------- HRESULT DirectInput ----------
const DI_OK: i32 = 0;
const DI_NOEFFECT: i32 = 1;
const DI_BUFFEROVERFLOW: i32 = 1;
const DIERR_INVALIDPARAM: i32 = 0x8007_0057u32 as i32;
const DIERR_NOTINITIALIZED: i32 = 0x8007_0015u32 as i32;
const DIERR_DEVICENOTREG: i32 = 0x8004_0154u32 as i32;
const DIERR_NOTACQUIRED: i32 = 0x8007_000Cu32 as i32;
const DIERR_INPUTLOST: i32 = 0x8007_001Eu32 as i32;
const DIERR_OBJECTNOTFOUND: i32 = 0x8007_0002u32 as i32;
const DIERR_UNSUPPORTED: i32 = 0x8000_4001u32 as i32;
const DIERR_ACQUIRED: i32 = 0x8007_00AAu32 as i32;

// ---------- GUID ----------
const BFC7: [u8; 8] = [0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00];
pub const GUID_XAXIS: Guid = guid(0xA36D02E0, 0xC9F3, 0x11CF, BFC7);
pub const GUID_YAXIS: Guid = guid(0xA36D02E1, 0xC9F3, 0x11CF, BFC7);
pub const GUID_ZAXIS: Guid = guid(0xA36D02E2, 0xC9F3, 0x11CF, BFC7);
pub const GUID_RXAXIS: Guid = guid(0xA36D02F4, 0xC9F3, 0x11CF, BFC7);
pub const GUID_RYAXIS: Guid = guid(0xA36D02F5, 0xC9F3, 0x11CF, BFC7);
pub const GUID_RZAXIS: Guid = guid(0xA36D02E3, 0xC9F3, 0x11CF, BFC7);
pub const GUID_SLIDER: Guid = guid(0xA36D02E4, 0xC9F3, 0x11CF, BFC7);
pub const GUID_BUTTON: Guid = guid(0xA36D02F0, 0xC9F3, 0x11CF, BFC7);
pub const GUID_KEY: Guid = guid(0x55728220, 0xD33C, 0x11CF, BFC7);
pub const GUID_POV: Guid = guid(0xA36D02F2, 0xC9F3, 0x11CF, BFC7);
pub const GUID_SYSMOUSE: Guid = guid(0x6F1D2B60, 0xD5A0, 0x11CF, BFC7);
pub const GUID_SYSKEYBOARD: Guid = guid(0x6F1D2B61, 0xD5A0, 0x11CF, BFC7);
pub const GUID_JOYSTICK: Guid = guid(0x6F1D2B70, 0xD5A0, 0x11CF, BFC7);
const GUID_SYSMOUSEEM: Guid = guid(0x6F1D2B80, 0xD5A0, 0x11CF, BFC7);
const GUID_SYSKEYBOARDEM: Guid = guid(0x6F1D2B82, 0xD5A0, 0x11CF, BFC7);

const DI8_D4: [u8; 8] = [0xAA, 0x99, 0x5D, 0x64, 0xED, 0x36, 0x97, 0x00];
const IID_IDIRECTINPUT8A: Guid = guid(0xBF798030, 0x483A, 0x4DA2, DI8_D4);
const IID_IDIRECTINPUT8W: Guid = guid(0xBF798031, 0x483A, 0x4DA2, DI8_D4);
const IID_IDIRECTINPUTA: Guid = guid(0x89521360, 0xAA8A, 0x11CF, BFC7);
const IID_IDIRECTINPUTW: Guid = guid(0x89521361, 0xAA8A, 0x11CF, BFC7);
const IID_IDIRECTINPUT2A: Guid = guid(0x5944E662, 0xAA8A, 0x11CF, BFC7);
const IID_IDIRECTINPUT2W: Guid = guid(0x5944E663, 0xAA8A, 0x11CF, BFC7);
const DI7_D4: [u8; 8] = [0x8E, 0x9D, 0x00, 0xC0, 0x4F, 0x68, 0x44, 0xAE];
const IID_IDIRECTINPUT7A: Guid = guid(0x9A4CB684, 0x236D, 0x11D3, DI7_D4);
const IID_IDIRECTINPUT7W: Guid = guid(0x9A4CB685, 0x236D, 0x11D3, DI7_D4);
const DEV8_D4: [u8; 8] = [0xA4, 0x1B, 0x74, 0x8F, 0x73, 0xA3, 0x81, 0x79];
const IID_IDIRECTINPUTDEVICE8A: Guid = guid(0x54D41080, 0xDC15, 0x4833, DEV8_D4);
const IID_IDIRECTINPUTDEVICE8W: Guid = guid(0x54D41081, 0xDC15, 0x4833, DEV8_D4);
const IID_IDIRECTINPUTDEVICEA: Guid = guid(0x5944E680, 0xC92E, 0x11CF, BFC7);
const IID_IDIRECTINPUTDEVICEW: Guid = guid(0x5944E681, 0xC92E, 0x11CF, BFC7);
const IID_IDIRECTINPUTDEVICE2A: Guid = guid(0x5944E682, 0xC92E, 0x11CF, BFC7);
const IID_IDIRECTINPUTDEVICE2W: Guid = guid(0x5944E683, 0xC92E, 0x11CF, BFC7);
const IID_IDIRECTINPUTDEVICE7A: Guid = guid(0x57D7C6BC, 0x2356, 0x11D3, DI7_D4);
const IID_IDIRECTINPUTDEVICE7W: Guid = guid(0x57D7C6BD, 0x2356, 0x11D3, DI7_D4);

// ---------- Types d'objets (DIDFT_*) ----------
const DIDFT_RELAXIS: u32 = 0x01;
const DIDFT_ABSAXIS: u32 = 0x02;
const DIDFT_AXIS: u32 = 0x03;
const DIDFT_PSHBUTTON: u32 = 0x04;
#[cfg(test)]
const DIDFT_BUTTON: u32 = 0x0C;
const DIDFT_POV: u32 = 0x10;
const DIDFT_TYPEMASK: u32 = 0x1F;
const DIDFT_ANYINSTANCE: u32 = 0x00FF_FF00;
const DIDOI_ASPECTPOSITION: u32 = 0x100;

const DIPH_DEVICE: u32 = 0;
const DIPH_BYOFFSET: u32 = 1;
const DIPH_BYID: u32 = 2;

// Propriétés : MAKEDIPROP(n) = (REFGUID)n.
const DIPROP_BUFFERSIZE: usize = 1;
const DIPROP_AXISMODE: usize = 2;
const DIPROP_GRANULARITY: usize = 3;
const DIPROP_RANGE: usize = 4;
const DIPROP_DEADZONE: usize = 5;
const DIPROP_SATURATION: usize = 6;
const DIPROP_FFGAIN: usize = 7;
const DIPROP_AUTOCENTER: usize = 9;
const DIPROP_CALIBRATIONMODE: usize = 10;
const DIPROP_GUIDANDPATH: usize = 12;
const DIPROP_INSTANCENAME: usize = 13;
const DIPROP_PRODUCTNAME: usize = 14;
const DIPROP_JOYSTICKID: usize = 15;
const DIPROP_VIDPID: usize = 24;

/// Famille du périphérique DirectInput.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Keyboard,
    Mouse,
    Game(evdev::Kind),
}

/// Un objet (axe, bouton, chapeau, touche) du périphérique.
#[derive(Clone, Debug)]
struct Obj {
    guid: Guid,
    /// DIDFT_* | instance << 8.
    ty: u32,
    name: String,
    src: Src,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Src {
    Axis(usize),
    Button(u16),
    /// Chapeau : axes evdev HATnX/HATnY.
    Hat(usize),
    /// Croix numérique sans chapeau evdev (BTN_DPAD_*).
    DpadPov,
    Key(u8),
    MouseAxis(u8),
    MouseButton(u8),
}

impl Obj {
    fn instance(&self) -> u32 {
        (self.ty >> 8) & 0xFFFF
    }
    fn is_axis(&self) -> bool {
        self.ty & DIDFT_AXIS != 0
    }
    fn is_pov(&self) -> bool {
        self.ty & DIDFT_POV != 0
    }
}

/// Réglages d'un axe (DIPROP_RANGE, DEADZONE, SATURATION).
#[derive(Clone, Copy, Debug)]
struct AxisProps {
    min: i32,
    max: i32,
    deadzone: u32,
    saturation: u32,
}

impl Default for AxisProps {
    fn default() -> Self {
        Self {
            min: 0,
            max: 65535,
            deadzone: 0,
            saturation: 10000,
        }
    }
}

const AXIS_ORDER: [(usize, Guid, &str); 11] = [
    (evdev::ABS_X, GUID_XAXIS, "Axe X"),
    (evdev::ABS_Y, GUID_YAXIS, "Axe Y"),
    (evdev::ABS_Z, GUID_ZAXIS, "Axe Z"),
    (evdev::ABS_RX, GUID_RXAXIS, "Rotation X"),
    (evdev::ABS_RY, GUID_RYAXIS, "Rotation Y"),
    (evdev::ABS_RZ, GUID_RZAXIS, "Rotation Z"),
    (evdev::ABS_THROTTLE, GUID_SLIDER, "Manette des gaz"),
    (evdev::ABS_RUDDER, GUID_SLIDER, "Palonnier"),
    (evdev::ABS_WHEEL, GUID_SLIDER, "Volant"),
    (evdev::ABS_GAS, GUID_SLIDER, "Acc\u{e9}l\u{e9}rateur"),
    (evdev::ABS_BRAKE, GUID_SLIDER, "Frein"),
];

/// Objets d'un contrôleur de jeu : axes, boutons puis chapeaux.
fn game_objects(axes: &[usize], buttons: &[u16]) -> Vec<Obj> {
    let mut out = Vec::new();
    let mut n = 0;
    for (code, g, name) in AXIS_ORDER {
        if axes.contains(&code) {
            out.push(Obj {
                guid: g,
                ty: DIDFT_ABSAXIS | (n << 8),
                name: name.into(),
                src: Src::Axis(code),
            });
            n += 1;
        }
    }
    let has_hat = axes.contains(&evdev::ABS_HAT0X) || axes.contains(&evdev::ABS_HAT0Y);
    let dpad = !has_hat && buttons.contains(&evdev::BTN_DPAD_UP);
    let mut b = 0;
    for &code in buttons {
        if dpad && (evdev::BTN_DPAD_UP..=evdev::BTN_DPAD_RIGHT).contains(&code) {
            continue;
        }
        out.push(Obj {
            guid: GUID_BUTTON,
            ty: DIDFT_PSHBUTTON | (b << 8),
            name: format!("Bouton {}", b),
            src: Src::Button(code),
        });
        b += 1;
    }
    let mut p = 0;
    for hat in 0..4 {
        let x = evdev::ABS_HAT0X + hat * 2;
        if axes.contains(&x) || axes.contains(&(x + 1)) {
            out.push(Obj {
                guid: GUID_POV,
                ty: DIDFT_POV | (p << 8),
                name: format!("Chapeau {}", p),
                src: Src::Hat(hat),
            });
            p += 1;
        }
    }
    if dpad {
        out.push(Obj {
            guid: GUID_POV,
            ty: DIDFT_POV,
            name: "Croix directionnelle".into(),
            src: Src::DpadPov,
        });
    }
    out
}

fn keyboard_objects() -> Vec<Obj> {
    (1u32..256)
        .filter(|&k| {
            k <= 0x58
                || matches!(k, 0x9C | 0x9D | 0xB5 | 0xB7 | 0xB8 | 0xC5 | 0xC7..=0xD3 | 0xDB..=0xDD)
        })
        .filter(|&k| !matches!(k, 0x54 | 0x55 | 0xCA | 0xCC | 0xCE))
        .map(|k| Obj {
            guid: GUID_KEY,
            ty: DIDFT_PSHBUTTON | (k << 8),
            name: format!("Touche {:#04x}", k),
            src: Src::Key(k as u8),
        })
        .collect()
}

fn mouse_objects() -> Vec<Obj> {
    let mut out = vec![
        Obj {
            guid: GUID_XAXIS,
            ty: DIDFT_RELAXIS,
            name: "Axe X".into(),
            src: Src::MouseAxis(0),
        },
        Obj {
            guid: GUID_YAXIS,
            ty: DIDFT_RELAXIS | (1 << 8),
            name: "Axe Y".into(),
            src: Src::MouseAxis(1),
        },
        Obj {
            guid: GUID_ZAXIS,
            ty: DIDFT_RELAXIS | (2 << 8),
            name: "Molette".into(),
            src: Src::MouseAxis(2),
        },
    ];
    for (i, name) in [
        "Bouton gauche",
        "Bouton droit",
        "Bouton central",
        "Bouton 4",
        "Bouton 5",
    ]
    .iter()
    .enumerate()
    {
        out.push(Obj {
            guid: GUID_BUTTON,
            ty: DIDFT_PSHBUTTON | ((i as u32) << 8),
            name: (*name).into(),
            src: Src::MouseButton(i as u8),
        });
    }
    out
}

/// Angle d'un chapeau en centièmes de degré (0 = haut), ou -1 au centre.
pub fn pov_angle(x: i32, y: i32) -> u32 {
    match (x.signum(), y.signum()) {
        (0, -1) => 0,
        (1, -1) => 4500,
        (1, 0) => 9000,
        (1, 1) => 13500,
        (0, 1) => 18000,
        (-1, 1) => 22500,
        (-1, 0) => 27000,
        (-1, -1) => 31500,
        _ => u32::MAX,
    }
}

/// Valeur d'un axe absolu selon la plage, la zone morte et la saturation.
fn scale_axis(n: f32, p: &AxisProps) -> i32 {
    let dz = p.deadzone.min(10000) as f32 / 10000.0;
    let sat = (p.saturation.clamp(1, 10000) as f32 / 10000.0).max(dz + 0.0001);
    let mag = n.abs();
    let m = if mag <= dz {
        0.0
    } else {
        ((mag - dz) / (sat - dz)).min(1.0)
    };
    let v = m.copysign(n);
    let (min, max) = (p.min as f64, p.max as f64);
    (min + (v as f64 + 1.0) * 0.5 * (max - min)).round() as i32
}

/// Entrée du format de données de l'application.
#[derive(Clone, Copy, Debug)]
struct FormatEntry {
    ofs: u32,
    /// Objet du périphérique correspondant, s'il existe.
    obj: Option<usize>,
    /// Taille écrite : 4 (axe, chapeau) ou 1 (bouton).
    wide: bool,
    pov: bool,
}

/// Associe chaque entrée de DIDATAFORMAT à un objet du périphérique.
fn match_format(objs: &[Obj], entries: &[(Option<Guid>, u32, u32)]) -> Vec<FormatEntry> {
    let mut used = vec![false; objs.len()];
    entries
        .iter()
        .map(|&(g, ofs, ty)| {
            let want = ty & DIDFT_TYPEMASK;
            let any_inst = ty & DIDFT_ANYINSTANCE == DIDFT_ANYINSTANCE;
            let inst = (ty >> 8) & 0xFFFF;
            let found = objs.iter().enumerate().position(|(i, o)| {
                !used[i]
                    && (want == 0 || o.ty & want != 0)
                    && (any_inst || o.instance() == inst)
                    && g.is_none_or(|g| g == o.guid)
            });
            if let Some(i) = found {
                used[i] = true;
            }
            let o = found.map(|i| &objs[i]);
            let axis_or_pov = o.map_or(want & (DIDFT_AXIS | DIDFT_POV) != 0, |o| {
                o.is_axis() || o.is_pov()
            });
            FormatEntry {
                ofs,
                obj: found,
                wide: axis_or_pov,
                pov: o.map_or(want & DIDFT_POV != 0, Obj::is_pov),
            }
        })
        .collect()
}

struct Event {
    ofs: u32,
    data: u32,
    time: u32,
    seq: u32,
}

static SEQUENCE: AtomicU32 = AtomicU32::new(1);

fn tick() -> u32 {
    crate::dlls::kernel32::get_tick_count()
}

/// État d'un périphérique créé par CreateDevice.
struct DevState {
    family: Family,
    device: Option<Device>,
    objs: Vec<Obj>,
    props: Vec<AxisProps>,
    format: Option<Vec<FormatEntry>>,
    data_size: usize,
    relative_mouse: bool,
    acquired: bool,
    buffer_size: usize,
    events: VecDeque<Event>,
    overflow: bool,
    last_values: Vec<u32>,
    /// Déplacement souris accumulé depuis le dernier GetDeviceState.
    mouse_acc: [i32; 3],
    mouse_abs: [i32; 3],
    notify: usize,
    instance: Guid,
    product: Guid,
    dev_type: u32,
    name: String,
    joystick_id: u32,
}

#[repr(C)]
struct DevObj {
    vtbl: *const (),
    refs: AtomicU32,
    wide: bool,
    state: Mutex<DevState>,
}

#[repr(C)]
struct DiObj {
    vtbl: *const (),
    refs: AtomicU32,
    wide: bool,
    /// DirectInput 3 à 7 : anciens types de périphériques.
    legacy: bool,
}

/// Description d'un périphérique énumérable.
struct Instance {
    instance: Guid,
    product: Guid,
    family: Family,
    name: String,
    path: Option<std::path::PathBuf>,
    joystick_id: u32,
}

fn product_guid(vid: u16, pid: u16) -> Guid {
    guid(
        ((pid as u32) << 16) | vid as u32,
        0,
        0,
        [0, 0, b'P', b'I', b'D', b'V', b'I', b'D'],
    )
}

fn joystick_instance_guid(event: u32, vid: u16, pid: u16) -> Guid {
    guid(0x574C_0000 | (event & 0xFFFF), vid, pid, *b"WINLUXJS")
}

fn instances() -> Vec<Instance> {
    let mut out = vec![
        Instance {
            instance: GUID_SYSMOUSE,
            product: GUID_SYSMOUSE,
            family: Family::Mouse,
            name: "Souris".into(),
            path: None,
            joystick_id: 0,
        },
        Instance {
            instance: GUID_SYSKEYBOARD,
            product: GUID_SYSKEYBOARD,
            family: Family::Keyboard,
            name: "Clavier".into(),
            path: None,
            joystick_id: 0,
        },
    ];
    for (i, d) in evdev::scan().into_iter().enumerate() {
        out.push(Instance {
            instance: joystick_instance_guid(d.event_number(), d.vendor, d.product),
            product: product_guid(d.vendor, d.product),
            family: Family::Game(d.kind),
            name: d.name.clone(),
            path: Some(d.path.clone()),
            joystick_id: i as u32,
        });
    }
    out
}

/// DI8DEVTYPE_* (ou DIDEVTYPE_* hérité) avec sous-type.
fn dev_type(f: Family, legacy: bool) -> u32 {
    if legacy {
        match f {
            Family::Mouse => 2 | (2 << 8),
            Family::Keyboard => 3 | (4 << 8),
            Family::Game(evdev::Kind::Gamepad) => 4 | (4 << 8) | 0x10000,
            Family::Game(evdev::Kind::Wheel) => 4 | (6 << 8) | 0x10000,
            Family::Game(evdev::Kind::Joystick) => 4 | (2 << 8) | 0x10000,
        }
    } else {
        match f {
            Family::Mouse => 0x12 | (1 << 8),
            Family::Keyboard => 0x13 | (4 << 8),
            Family::Game(evdev::Kind::Gamepad) => 0x15 | (2 << 8) | 0x10000,
            Family::Game(evdev::Kind::Wheel) => 0x16 | (3 << 8) | 0x10000,
            Family::Game(evdev::Kind::Joystick) => 0x14 | (2 << 8) | 0x10000,
        }
    }
}

/// Filtre `dwDevType` d'EnumDevices.
fn type_matches(filter: u32, f: Family, legacy: bool) -> bool {
    let t = dev_type(f, legacy) & 0xFF;
    let game = matches!(f, Family::Game(_));
    if legacy {
        return filter == 0 || filter & 0xFF == t;
    }
    match filter & 0xFF {
        0 => true,                  // DI8DEVCLASS_ALL
        1 => false,                 // DI8DEVCLASS_DEVICE
        2 => f == Family::Mouse,    // DI8DEVCLASS_POINTER
        3 => f == Family::Keyboard, // DI8DEVCLASS_KEYBOARD
        4 => game,                  // DI8DEVCLASS_GAMECTRL
        x => x == t,
    }
}

/// Remplit DIDEVICEINSTANCE{A,W} selon `size` (versions DX3 et DX5+).
unsafe fn fill_instance(p: *mut u8, size: u32, inst: &Instance, wide: bool, legacy: bool) {
    let ch = if wide { 2 } else { 1 };
    let full = 40 + 2 * 260 * ch + 20;
    let size = size as usize;
    std::ptr::write_bytes(p, 0, size.min(full));
    (p as *mut u32).write_unaligned(size as u32);
    std::ptr::copy_nonoverlapping(inst.instance.as_ptr(), p.add(4), 16);
    std::ptr::copy_nonoverlapping(inst.product.as_ptr(), p.add(20), 16);
    (p.add(36) as *mut u32).write_unaligned(dev_type(inst.family, legacy));
    comobj::write_fixed_str(p.add(40), 260, &inst.name, wide);
    comobj::write_fixed_str(p.add(40 + 260 * ch), 260, &inst.name, wide);
    if size >= full {
        let usage = p.add(40 + 2 * 260 * ch + 16);
        let (page, u) = match inst.family {
            Family::Mouse => (1, 2),
            Family::Keyboard => (1, 6),
            Family::Game(evdev::Kind::Gamepad) => (1, 5),
            Family::Game(_) => (1, 4),
        };
        (usage as *mut u16).write_unaligned(page);
        (usage.add(2) as *mut u16).write_unaligned(u);
    }
}

fn valid_instance_size(size: u32, wide: bool) -> bool {
    let ch = if wide { 2 } else { 1 };
    size == 40 + 2 * 260 * ch || size == 40 + 2 * 260 * ch + 20
}

/// Remplit DIDEVICEOBJECTINSTANCE{A,W}.
unsafe fn fill_object(p: *mut u8, size: u32, o: &Obj, ofs: u32, wide: bool) {
    let ch = if wide { 2 } else { 1 };
    let size = size as usize;
    let full = 32 + 260 * ch + 24;
    std::ptr::write_bytes(p, 0, size.min(full));
    (p as *mut u32).write_unaligned(size as u32);
    std::ptr::copy_nonoverlapping(o.guid.as_ptr(), p.add(4), 16);
    (p.add(20) as *mut u32).write_unaligned(ofs);
    (p.add(24) as *mut u32).write_unaligned(o.ty);
    let flags = if o.is_axis() { DIDOI_ASPECTPOSITION } else { 0 };
    (p.add(28) as *mut u32).write_unaligned(flags);
    comobj::write_fixed_str(p.add(32), 260, &o.name, wide);
    if size >= full {
        let usage = match o.src {
            Src::Axis(a) => 0x30 + a as u16,
            Src::Hat(_) | Src::DpadPov => 0x39,
            _ => 0,
        };
        let tail = p.add(32 + 260 * ch);
        (tail.add(12) as *mut u16).write_unaligned(if usage != 0 { 1 } else { 9 });
        (tail.add(14) as *mut u16).write_unaligned(usage);
    }
}

impl DevState {
    fn new(inst: &Instance, legacy: bool) -> Option<DevState> {
        let (device, objs) = match inst.family {
            Family::Keyboard => (None, keyboard_objects()),
            Family::Mouse => (None, mouse_objects()),
            Family::Game(_) => {
                let d = Device::open(inst.path.as_ref()?)?;
                let objs = game_objects(&d.axes, &d.buttons);
                (Some(d), objs)
            }
        };
        let n = objs.len();
        Some(DevState {
            family: inst.family,
            device,
            props: vec![AxisProps::default(); n],
            objs,
            format: None,
            data_size: 0,
            relative_mouse: true,
            acquired: false,
            buffer_size: 0,
            events: VecDeque::new(),
            overflow: false,
            last_values: Vec::new(),
            mouse_acc: [0; 3],
            mouse_abs: [0; 3],
            notify: 0,
            instance: inst.instance,
            product: inst.product,
            dev_type: dev_type(inst.family, legacy),
            name: inst.name.clone(),
            joystick_id: inst.joystick_id,
        })
    }

    fn instance(&self) -> Instance {
        Instance {
            instance: self.instance,
            product: self.product,
            family: self.family,
            name: self.name.clone(),
            path: self.device.as_ref().map(|d| d.path.clone()),
            joystick_id: self.joystick_id,
        }
    }

    /// Valeur brute (LONG/BYTE/DWORD) de chaque objet. `None` si débranché.
    fn read_values(&mut self) -> Option<Vec<u32>> {
        let snap: Option<Snapshot> = match &self.device {
            Some(d) => Some(d.snapshot()?),
            None => None,
        };
        let keys = if self.family == Family::Keyboard {
            crate::dlls::gui::raw_keys()
        } else {
            [false; 256]
        };
        let mut mouse_delta = [0i32; 3];
        let mut mouse_buttons = [false; 8];
        if self.family == Family::Mouse {
            let (dx, dy, dz, b) = crate::dlls::gui::take_raw_mouse();
            mouse_delta = [dx, dy, dz];
            mouse_buttons = b;
            for i in 0..3 {
                self.mouse_acc[i] += mouse_delta[i];
                self.mouse_abs[i] += mouse_delta[i];
            }
        }
        let mut out = Vec::with_capacity(self.objs.len());
        for (i, o) in self.objs.iter().enumerate() {
            let v = match (o.src, &snap) {
                (Src::Axis(a), Some(s)) => scale_axis(s.abs[a].signed(), &self.props[i]) as u32,
                (Src::Button(c), Some(s)) => {
                    if s.key(c) {
                        0x80
                    } else {
                        0
                    }
                }
                (Src::Hat(h), Some(s)) => pov_angle(
                    s.abs[evdev::ABS_HAT0X + h * 2].value,
                    s.abs[evdev::ABS_HAT0X + h * 2 + 1].value,
                ),
                (Src::DpadPov, Some(s)) => {
                    let x =
                        s.key(evdev::BTN_DPAD_RIGHT) as i32 - s.key(evdev::BTN_DPAD_LEFT) as i32;
                    let y = s.key(evdev::BTN_DPAD_DOWN) as i32 - s.key(evdev::BTN_DPAD_UP) as i32;
                    pov_angle(x, y)
                }
                (Src::Key(k), _) => {
                    if keys[k as usize] {
                        0x80
                    } else {
                        0
                    }
                }
                (Src::MouseAxis(a), _) => {
                    if self.relative_mouse {
                        mouse_delta[a as usize] as u32
                    } else {
                        self.mouse_abs[a as usize] as u32
                    }
                }
                (Src::MouseButton(b), _) => {
                    if mouse_buttons[b as usize] {
                        0x80
                    } else {
                        0
                    }
                }
                _ => 0,
            };
            out.push(v);
        }
        Some(out)
    }

    /// Lit l'état et range les changements dans le tampon d'événements.
    fn sample(&mut self) -> Option<Vec<u32>> {
        let values = self.read_values()?;
        let Some(format) = &self.format else {
            return Some(values);
        };
        let mut changed = false;
        let first = self.last_values.len() != values.len();
        for e in format {
            let Some(i) = e.obj else { continue };
            let v = values[i];
            let rel = matches!(self.objs[i].src, Src::MouseAxis(_)) && self.relative_mouse;
            let differs = if rel {
                v != 0
            } else {
                !first && self.last_values[i] != v
            };
            if differs {
                changed = true;
                let cap = if self.buffer_size == 0 {
                    0
                } else {
                    self.buffer_size
                };
                if self.events.len() >= cap {
                    if cap > 0 {
                        self.events.pop_front();
                    }
                    self.overflow = cap > 0;
                    if cap == 0 {
                        continue;
                    }
                }
                self.events.push_back(Event {
                    ofs: e.ofs,
                    data: v,
                    time: tick(),
                    seq: SEQUENCE.fetch_add(1, Ordering::Relaxed),
                });
            }
        }
        self.last_values = values.clone();
        if changed && self.notify != 0 {
            crate::dlls::kernel32::set_event(self.notify);
        }
        Some(values)
    }

    /// Index de l'objet désigné par (dwObj, dwHow).
    fn find(&self, obj: u32, how: u32) -> Option<usize> {
        match how {
            DIPH_BYOFFSET => self
                .format
                .as_ref()?
                .iter()
                .find(|e| e.ofs == obj)
                .and_then(|e| e.obj),
            DIPH_BYID => self.objs.iter().position(|o| {
                o.ty & DIDFT_TYPEMASK & obj != 0 && o.instance() == (obj >> 8) & 0xFFFF
            }),
            _ => None,
        }
    }

    fn offset_of(&self, i: usize) -> u32 {
        self.format
            .as_ref()
            .and_then(|f| f.iter().find(|e| e.obj == Some(i)).map(|e| e.ofs))
            .unwrap_or(i as u32 * 4)
    }

    fn counts(&self) -> (u32, u32, u32) {
        let axes = self.objs.iter().filter(|o| o.is_axis()).count() as u32;
        let povs = self.objs.iter().filter(|o| o.is_pov()).count() as u32;
        (axes, self.objs.len() as u32 - axes - povs, povs)
    }
}

// ---------- IDirectInputDevice8 ----------

unsafe fn dev<'a>(this: *mut DevObj) -> &'a DevObj {
    &*this
}

const DEVICE_IIDS: [Guid; 8] = [
    IID_IDIRECTINPUTDEVICE8A,
    IID_IDIRECTINPUTDEVICE8W,
    IID_IDIRECTINPUTDEVICEA,
    IID_IDIRECTINPUTDEVICEW,
    IID_IDIRECTINPUTDEVICE2A,
    IID_IDIRECTINPUTDEVICE2W,
    IID_IDIRECTINPUTDEVICE7A,
    IID_IDIRECTINPUTDEVICE7W,
];

unsafe extern "win64" fn d_qi(this: *mut DevObj, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if iid == Some(IID_IUNKNOWN) || iid.is_some_and(|g| DEVICE_IIDS.contains(&g)) {
        dev(this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn d_addref(this: *mut DevObj) -> u32 {
    dev(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn d_release(this: *mut DevObj) -> u32 {
    let n = dev(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}
unsafe extern "win64" fn d_get_capabilities(this: *mut DevObj, caps: *mut u32) -> i32 {
    if caps.is_null() {
        return E_POINTER;
    }
    let size = *caps;
    if size != 24 && size != 44 {
        return DIERR_INVALIDPARAM;
    }
    let s = lock(&dev(this).state);
    let (axes, buttons, povs) = s.counts();
    let polled = if s.device.is_some() { 2 } else { 0 };
    let words = [
        size,
        1 | polled,
        s.dev_type,
        axes,
        buttons,
        povs,
        0,
        0,
        0,
        0,
        0,
    ];
    for (i, w) in words.iter().take(size as usize / 4).enumerate() {
        caps.add(i).write_unaligned(*w);
    }
    DI_OK
}
type EnumObjCb = unsafe extern "win64" fn(*const u8, *mut c_void) -> i32;
unsafe extern "win64" fn d_enum_objects(
    this: *mut DevObj,
    cb: usize,
    ctx: *mut c_void,
    flags: u32,
) -> i32 {
    if cb == 0 {
        return DIERR_INVALIDPARAM;
    }
    let d = dev(this);
    let items: Vec<(Obj, u32)> = {
        let s = lock(&d.state);
        s.objs
            .iter()
            .enumerate()
            .filter(|(_, o)| flags & DIDFT_TYPEMASK == 0 || o.ty & flags & DIDFT_TYPEMASK != 0)
            .map(|(i, o)| (o.clone(), s.offset_of(i)))
            .collect()
    };
    let cb: EnumObjCb = std::mem::transmute(cb);
    let size = if d.wide { 576 } else { 316 };
    let mut buf = vec![0u8; size];
    for (o, ofs) in items {
        fill_object(buf.as_mut_ptr(), size as u32, &o, ofs, d.wide);
        if cb(buf.as_ptr(), ctx) == 0 {
            break;
        }
    }
    DI_OK
}
unsafe extern "win64" fn d_get_property(this: *mut DevObj, prop: usize, hdr: *mut u32) -> i32 {
    if hdr.is_null() {
        return DIERR_INVALIDPARAM;
    }
    let (obj, how) = (*hdr.add(2), *hdr.add(3));
    let d = dev(this);
    let s = lock(&d.state);
    let target = if how == DIPH_DEVICE {
        None
    } else {
        match s.find(obj, how) {
            Some(i) => Some(i),
            None => return DIERR_OBJECTNOTFOUND,
        }
    };
    let data = hdr.add(4);
    match prop {
        DIPROP_RANGE => {
            let p = target.map_or(AxisProps::default(), |i| s.props[i]);
            *data = p.min as u32;
            *data.add(1) = p.max as u32;
        }
        DIPROP_DEADZONE => *data = target.map_or(0, |i| s.props[i].deadzone),
        DIPROP_SATURATION => *data = target.map_or(10000, |i| s.props[i].saturation),
        DIPROP_BUFFERSIZE => *data = s.buffer_size as u32,
        DIPROP_AXISMODE => *data = u32::from(!s.relative_mouse && s.family == Family::Mouse),
        DIPROP_GRANULARITY => {
            *data = if s.family == Family::Mouse && target.is_some_and(|i| i == 2) {
                120
            } else {
                1
            }
        }
        DIPROP_FFGAIN => *data = 10000,
        DIPROP_AUTOCENTER => *data = 0,
        DIPROP_CALIBRATIONMODE => *data = 0,
        DIPROP_JOYSTICKID => *data = s.joystick_id,
        DIPROP_VIDPID => {
            let (v, p) = s.device.as_ref().map_or((0, 0), |d| (d.vendor, d.product));
            *data = ((p as u32) << 16) | v as u32;
        }
        DIPROP_INSTANCENAME | DIPROP_PRODUCTNAME => {
            comobj::write_fixed_str(data as *mut u8, 260, &s.name, true);
        }
        DIPROP_GUIDANDPATH => {
            std::ptr::copy_nonoverlapping(s.instance.as_ptr(), data as *mut u8, 16);
            let path = s
                .device
                .as_ref()
                .map(|d| d.path.display().to_string())
                .unwrap_or_default();
            comobj::write_fixed_str((data as *mut u8).add(16), 260, &path, true);
        }
        _ => return DIERR_UNSUPPORTED,
    }
    DI_OK
}
unsafe extern "win64" fn d_set_property(this: *mut DevObj, prop: usize, hdr: *const u32) -> i32 {
    if hdr.is_null() {
        return DIERR_INVALIDPARAM;
    }
    let (obj, how) = (*hdr.add(2), *hdr.add(3));
    let d = dev(this);
    let mut s = lock(&d.state);
    let targets: Vec<usize> = if how == DIPH_DEVICE {
        (0..s.objs.len()).filter(|&i| s.objs[i].is_axis()).collect()
    } else {
        match s.find(obj, how) {
            Some(i) => vec![i],
            None => return DIERR_OBJECTNOTFOUND,
        }
    };
    let data = hdr.add(4);
    match prop {
        DIPROP_RANGE => {
            let (min, max) = (*data as i32, *data.add(1) as i32);
            if min > max {
                return DIERR_INVALIDPARAM;
            }
            for i in targets {
                s.props[i].min = min;
                s.props[i].max = max;
            }
        }
        DIPROP_DEADZONE | DIPROP_SATURATION => {
            let v = *data;
            if v > 10000 {
                return DIERR_INVALIDPARAM;
            }
            for i in targets {
                if prop == DIPROP_DEADZONE {
                    s.props[i].deadzone = v;
                } else {
                    s.props[i].saturation = v;
                }
            }
        }
        DIPROP_BUFFERSIZE => {
            if s.acquired {
                return DIERR_ACQUIRED;
            }
            s.buffer_size = (*data as usize).min(4096);
        }
        DIPROP_AXISMODE => s.relative_mouse = *data != 1,
        DIPROP_AUTOCENTER | DIPROP_FFGAIN | DIPROP_CALIBRATIONMODE => {}
        _ => return DIERR_UNSUPPORTED,
    }
    DI_OK
}
unsafe extern "win64" fn d_acquire(this: *mut DevObj) -> i32 {
    let mut s = lock(&dev(this).state);
    if s.format.is_none() {
        return DIERR_INVALIDPARAM;
    }
    if s.acquired {
        return comobj::S_FALSE;
    }
    s.acquired = true;
    s.last_values.clear();
    s.mouse_acc = [0; 3];
    if s.family == Family::Mouse {
        crate::dlls::gui::take_raw_mouse();
    }
    let _ = s.sample();
    DI_OK
}
unsafe extern "win64" fn d_unacquire(this: *mut DevObj) -> i32 {
    let mut s = lock(&dev(this).state);
    if !s.acquired {
        return DI_NOEFFECT;
    }
    s.acquired = false;
    DI_OK
}
unsafe extern "win64" fn d_get_device_state(this: *mut DevObj, size: u32, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut s = lock(&dev(this).state);
    if !s.acquired {
        return DIERR_NOTACQUIRED;
    }
    if size as usize != s.data_size {
        return DIERR_INVALIDPARAM;
    }
    let Some(values) = s.sample() else {
        s.acquired = false;
        return DIERR_INPUTLOST;
    };
    std::ptr::write_bytes(out, 0, size as usize);
    let rel = s.relative_mouse && s.family == Family::Mouse;
    let acc = s.mouse_acc;
    for e in s.format.as_ref().into_iter().flatten() {
        let v = match e.obj {
            Some(i) => match s.objs[i].src {
                Src::MouseAxis(a) if rel => acc[a as usize] as u32,
                _ => values[i],
            },
            None if e.pov => u32::MAX,
            None => 0,
        };
        let end = e.ofs as usize + if e.wide { 4 } else { 1 };
        if end > size as usize {
            continue;
        }
        if e.wide {
            (out.add(e.ofs as usize) as *mut u32).write_unaligned(v);
        } else {
            *out.add(e.ofs as usize) = v as u8;
        }
    }
    s.mouse_acc = [0; 3];
    DI_OK
}
unsafe extern "win64" fn d_get_device_data(
    this: *mut DevObj,
    obj_size: u32,
    out: *mut u8,
    inout: *mut u32,
    flags: u32,
) -> i32 {
    if inout.is_null() || (obj_size != 16 && obj_size != 24) {
        return DIERR_INVALIDPARAM;
    }
    let mut s = lock(&dev(this).state);
    if !s.acquired {
        return DIERR_NOTACQUIRED;
    }
    if s.sample().is_none() {
        s.acquired = false;
        return DIERR_INPUTLOST;
    }
    let peek = flags & 1 != 0;
    let want = *inout as usize;
    if out.is_null() {
        // Compte (PEEK) ou vidage (INFINITE sans PEEK).
        *inout = s.events.len() as u32;
        if !peek {
            let n = want.min(s.events.len());
            s.events.drain(..n);
        }
    } else {
        let n = want.min(s.events.len());
        for (k, e) in s.events.iter().take(n).enumerate() {
            let p = out.add(k * obj_size as usize);
            std::ptr::write_bytes(p, 0, obj_size as usize);
            let w = [e.ofs, e.data, e.time, e.seq];
            for (j, v) in w.iter().enumerate() {
                (p.add(j * 4) as *mut u32).write_unaligned(*v);
            }
        }
        if !peek {
            s.events.drain(..n);
        }
        *inout = n as u32;
    }
    if std::mem::take(&mut s.overflow) {
        DI_BUFFEROVERFLOW
    } else {
        DI_OK
    }
}
unsafe extern "win64" fn d_set_data_format(this: *mut DevObj, fmt: *const u8) -> i32 {
    if fmt.is_null() {
        return DIERR_INVALIDPARAM;
    }
    let mut s = lock(&dev(this).state);
    if s.acquired {
        return DIERR_ACQUIRED;
    }
    // DIDATAFORMAT : dwSize, dwObjSize, dwFlags, dwDataSize, dwNumObjs, rgodf.
    let rd = |o: usize| (fmt.add(o) as *const u32).read_unaligned();
    let (obj_size, flags, data_size, count) = (rd(4), rd(8), rd(12), rd(16));
    let rgodf = (fmt.add(24) as *const *const u8).read_unaligned();
    if obj_size < 20 || data_size % 4 != 0 || (count > 0 && rgodf.is_null()) || count > 4096 {
        return DIERR_INVALIDPARAM;
    }
    let mut entries = Vec::with_capacity(count as usize);
    for k in 0..count as usize {
        let e = rgodf.add(k * obj_size as usize);
        let g = comobj::read_guid((e as *const *const u8).read_unaligned());
        let ofs = (e.add(8) as *const u32).read_unaligned();
        let ty = (e.add(12) as *const u32).read_unaligned();
        entries.push((g, ofs, ty));
    }
    let format = match_format(&s.objs, &entries);
    s.data_size = data_size as usize;
    s.format = Some(format);
    if s.family == Family::Mouse {
        s.relative_mouse = flags & 1 == 0; // DIDF_ABSAXIS
    }
    s.last_values.clear();
    s.events.clear();
    DI_OK
}
unsafe extern "win64" fn d_set_event_notification(this: *mut DevObj, ev: usize) -> i32 {
    lock(&dev(this).state).notify = ev;
    DI_OK
}
unsafe extern "win64" fn d_set_cooperative_level(_this: *mut DevObj, _hwnd: usize, _f: u32) -> i32 {
    DI_OK
}
unsafe extern "win64" fn d_get_object_info(
    this: *mut DevObj,
    out: *mut u8,
    obj: u32,
    how: u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let d = dev(this);
    let s = lock(&d.state);
    let Some(i) = s.find(obj, how) else {
        return DIERR_OBJECTNOTFOUND;
    };
    let size = (out as *const u32).read_unaligned();
    let ch = if d.wide { 2 } else { 1 };
    if size != 32 + 260 * ch && size != 32 + 260 * ch + 24 {
        return DIERR_INVALIDPARAM;
    }
    fill_object(out, size, &s.objs[i], s.offset_of(i), d.wide);
    DI_OK
}
unsafe extern "win64" fn d_get_device_info(this: *mut DevObj, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let d = dev(this);
    let size = (out as *const u32).read_unaligned();
    if !valid_instance_size(size, d.wide) {
        return DIERR_INVALIDPARAM;
    }
    let s = lock(&d.state);
    let legacy = s.dev_type & 0xFF < 0x10;
    fill_instance(out, size, &s.instance(), d.wide, legacy);
    DI_OK
}
unsafe extern "win64" fn d_run_control_panel(_this: *mut DevObj, _hwnd: usize, _f: u32) -> i32 {
    DI_OK
}
unsafe extern "win64" fn d_initialize(
    _this: *mut DevObj,
    _h: usize,
    _v: u32,
    _g: *const u8,
) -> i32 {
    DI_OK
}
unsafe extern "win64" fn d_create_effect(
    _this: *mut DevObj,
    _g: *const u8,
    _e: *const u8,
    out: *mut *mut c_void,
    _o: usize,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_enum_effects(_this: *mut DevObj, _cb: usize, _c: usize, _t: u32) -> i32 {
    DI_OK
}
unsafe extern "win64" fn d_get_effect_info(_this: *mut DevObj, _i: *mut u8, _g: *const u8) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_get_ff_state(_this: *mut DevObj, _out: *mut u32) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_send_ff_command(_this: *mut DevObj, _f: u32) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_enum_created_effects(
    _this: *mut DevObj,
    _cb: usize,
    _c: usize,
    _f: u32,
) -> i32 {
    DI_OK
}
unsafe extern "win64" fn d_escape(_this: *mut DevObj, _e: *mut u8) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_poll(this: *mut DevObj) -> i32 {
    let mut s = lock(&dev(this).state);
    if !s.acquired {
        return DIERR_NOTACQUIRED;
    }
    if s.device.is_none() {
        return DI_NOEFFECT;
    }
    if s.sample().is_none() {
        s.acquired = false;
        return DIERR_INPUTLOST;
    }
    DI_OK
}
unsafe extern "win64" fn d_send_device_data(
    _this: *mut DevObj,
    _s: u32,
    _d: *const u8,
    _n: *mut u32,
    _f: u32,
) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_effects_in_file(
    _this: *mut DevObj,
    _f: *const u8,
    _cb: usize,
    _c: usize,
    _fl: u32,
) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_write_effect(
    _this: *mut DevObj,
    _f: *const u8,
    _n: u32,
    _e: *const u8,
    _fl: u32,
) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_build_action_map(
    _this: *mut DevObj,
    _m: *mut u8,
    _u: *const u8,
    _f: u32,
) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_set_action_map(
    _this: *mut DevObj,
    _m: *mut u8,
    _u: *const u8,
    _f: u32,
) -> i32 {
    DIERR_UNSUPPORTED
}
unsafe extern "win64" fn d_get_image_info(_this: *mut DevObj, _h: *mut u8) -> i32 {
    DIERR_UNSUPPORTED
}

static DEVICE_VTBL: VTable<32> = VTable([
    d_qi as *const (),
    d_addref as *const (),
    d_release as *const (),
    d_get_capabilities as *const (),
    d_enum_objects as *const (),
    d_get_property as *const (),
    d_set_property as *const (),
    d_acquire as *const (),
    d_unacquire as *const (),
    d_get_device_state as *const (),
    d_get_device_data as *const (),
    d_set_data_format as *const (),
    d_set_event_notification as *const (),
    d_set_cooperative_level as *const (),
    d_get_object_info as *const (),
    d_get_device_info as *const (),
    d_run_control_panel as *const (),
    d_initialize as *const (),
    d_create_effect as *const (),
    d_enum_effects as *const (),
    d_get_effect_info as *const (),
    d_get_ff_state as *const (),
    d_send_ff_command as *const (),
    d_enum_created_effects as *const (),
    d_escape as *const (),
    d_poll as *const (),
    d_send_device_data as *const (),
    d_effects_in_file as *const (),
    d_write_effect as *const (),
    d_build_action_map as *const (),
    d_set_action_map as *const (),
    d_get_image_info as *const (),
]);

// ---------- IDirectInput8 / IDirectInput7 ----------

const DI_IIDS: [Guid; 8] = [
    IID_IDIRECTINPUT8A,
    IID_IDIRECTINPUT8W,
    IID_IDIRECTINPUTA,
    IID_IDIRECTINPUTW,
    IID_IDIRECTINPUT2A,
    IID_IDIRECTINPUT2W,
    IID_IDIRECTINPUT7A,
    IID_IDIRECTINPUT7W,
];

fn is_wide_iid(g: &Guid) -> bool {
    // Les IID W valent l'IID A + 1 sur le premier octet.
    [
        IID_IDIRECTINPUT8W,
        IID_IDIRECTINPUTW,
        IID_IDIRECTINPUT2W,
        IID_IDIRECTINPUT7W,
        IID_IDIRECTINPUTDEVICE8W,
        IID_IDIRECTINPUTDEVICEW,
        IID_IDIRECTINPUTDEVICE2W,
        IID_IDIRECTINPUTDEVICE7W,
    ]
    .contains(g)
}

unsafe extern "win64" fn i_qi(this: *mut DiObj, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if iid == Some(IID_IUNKNOWN) || iid.is_some_and(|g| DI_IIDS.contains(&g)) {
        (*this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn i_addref(this: *mut DiObj) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn i_release(this: *mut DiObj) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

fn resolve(g: &Guid) -> Option<Instance> {
    let all = instances();
    let alias = if *g == GUID_SYSKEYBOARDEM {
        GUID_SYSKEYBOARD
    } else if *g == GUID_SYSMOUSEEM {
        GUID_SYSMOUSE
    } else {
        *g
    };
    if alias == GUID_JOYSTICK {
        return all
            .into_iter()
            .find(|i| matches!(i.family, Family::Game(_)));
    }
    all.into_iter().find(|i| i.instance == alias).or_else(|| {
        instances()
            .into_iter()
            .find(|i| i.product == alias && matches!(i.family, Family::Game(_)))
    })
}

unsafe fn create_device(di: &DiObj, rguid: *const u8, out: *mut *mut c_void, wide: bool) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(g) = comobj::read_guid(rguid) else {
        return E_POINTER;
    };
    let Some(inst) = resolve(&g) else {
        return DIERR_DEVICENOTREG;
    };
    let Some(state) = DevState::new(&inst, di.legacy) else {
        return DIERR_DEVICENOTREG;
    };
    let obj = Box::new(DevObj {
        vtbl: DEVICE_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        wide,
        state: Mutex::new(state),
    });
    *out = Box::into_raw(obj) as *mut c_void;
    DI_OK
}

unsafe extern "win64" fn i_create_device(
    this: *mut DiObj,
    rguid: *const u8,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    create_device(&*this, rguid, out, (*this).wide)
}

type EnumDevCb = unsafe extern "win64" fn(*const u8, *mut c_void) -> i32;
unsafe extern "win64" fn i_enum_devices(
    this: *mut DiObj,
    ty: u32,
    cb: usize,
    ctx: *mut c_void,
    flags: u32,
) -> i32 {
    if cb == 0 {
        return DIERR_INVALIDPARAM;
    }
    let di = &*this;
    let cb: EnumDevCb = std::mem::transmute(cb);
    let size = if di.wide { 1100 } else { 580 };
    let mut buf = vec![0u8; size];
    for inst in instances() {
        if !type_matches(ty, inst.family, di.legacy) {
            continue;
        }
        if flags & 0x100 != 0 {
            continue; // DIEDFL_FORCEFEEDBACK : aucun retour de force exposé
        }
        fill_instance(buf.as_mut_ptr(), size as u32, &inst, di.wide, di.legacy);
        if cb(buf.as_ptr(), ctx) == 0 {
            break;
        }
    }
    DI_OK
}
unsafe extern "win64" fn i_get_device_status(_this: *mut DiObj, rguid: *const u8) -> i32 {
    match comobj::read_guid(rguid) {
        Some(g) if resolve(&g).is_some() => DI_OK,
        Some(_) => 1, // DI_NOTATTACHED
        None => E_POINTER,
    }
}
unsafe extern "win64" fn i_run_control_panel(_this: *mut DiObj, _h: usize, _f: u32) -> i32 {
    DI_OK
}
unsafe extern "win64" fn i_initialize(_this: *mut DiObj, _h: usize, _v: u32) -> i32 {
    DI_OK
}
unsafe extern "win64" fn i_find_device(
    _this: *mut DiObj,
    _class: *const u8,
    _name: *const u8,
    out: *mut u8,
) -> i32 {
    if !out.is_null() {
        std::ptr::write_bytes(out, 0, 16);
    }
    DIERR_DEVICENOTREG
}
/// IDirectInput8::EnumDevicesBySemantics : aucun mappage d'actions.
unsafe extern "win64" fn i_enum_by_semantics(
    _this: *mut DiObj,
    _user: *const u8,
    _fmt: *mut u8,
    _cb: usize,
    _ctx: usize,
    _flags: u32,
) -> i32 {
    DI_OK
}
unsafe extern "win64" fn i_configure_devices(
    _this: *mut DiObj,
    _cb: usize,
    _p: *mut u8,
    _f: u32,
    _r: usize,
) -> i32 {
    DIERR_UNSUPPORTED
}
/// IDirectInput7::CreateDeviceEx.
unsafe extern "win64" fn i_create_device_ex(
    this: *mut DiObj,
    rguid: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    let wide = comobj::read_guid(riid).map_or((*this).wide, |g| is_wide_iid(&g));
    create_device(&*this, rguid, out, wide)
}

static DI8_VTBL: VTable<11> = VTable([
    i_qi as *const (),
    i_addref as *const (),
    i_release as *const (),
    i_create_device as *const (),
    i_enum_devices as *const (),
    i_get_device_status as *const (),
    i_run_control_panel as *const (),
    i_initialize as *const (),
    i_find_device as *const (),
    i_enum_by_semantics as *const (),
    i_configure_devices as *const (),
]);

static DI7_VTBL: VTable<10> = VTable([
    i_qi as *const (),
    i_addref as *const (),
    i_release as *const (),
    i_create_device as *const (),
    i_enum_devices as *const (),
    i_get_device_status as *const (),
    i_run_control_panel as *const (),
    i_initialize as *const (),
    i_find_device as *const (),
    i_create_device_ex as *const (),
]);

unsafe fn new_di(out: *mut *mut c_void, wide: bool, legacy: bool) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let vtbl = if legacy {
        DI7_VTBL.0.as_ptr() as *const ()
    } else {
        DI8_VTBL.0.as_ptr() as *const ()
    };
    let obj = Box::new(DiObj {
        vtbl,
        refs: AtomicU32::new(1),
        wide,
        legacy,
    });
    *out = Box::into_raw(obj) as *mut c_void;
    DI_OK
}

pub unsafe extern "win64" fn direct_input8_create(
    _hinst: usize,
    version: u32,
    riid: *const u8,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if version < 0x0800 {
        return DIERR_NOTINITIALIZED;
    }
    match comobj::read_guid(riid) {
        Some(g) if g == IID_IDIRECTINPUT8A || g == IID_IUNKNOWN => new_di(out, false, false),
        Some(g) if g == IID_IDIRECTINPUT8W => new_di(out, true, false),
        Some(_) => E_NOINTERFACE,
        None => E_POINTER,
    }
}

pub unsafe extern "win64" fn direct_input_create_a(
    _hinst: usize,
    _version: u32,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    new_di(out, false, true)
}
pub unsafe extern "win64" fn direct_input_create_w(
    _hinst: usize,
    _version: u32,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    new_di(out, true, true)
}
pub unsafe extern "win64" fn direct_input_create_ex(
    _hinst: usize,
    _version: u32,
    riid: *const u8,
    out: *mut *mut c_void,
    _outer: usize,
) -> i32 {
    match comobj::read_guid(riid) {
        Some(g) if DI_IIDS.contains(&g) => new_di(out, is_wide_iid(&g), true),
        _ => E_NOINTERFACE,
    }
}

pub extern "win64" fn dll_can_unload_now() -> i32 {
    comobj::S_FALSE
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectInput8Create" => fn direct_input8_create,
        "DllCanUnloadNow" => fn dll_can_unload_now,
    }
}

pub fn legacy_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectInputCreateA" => fn direct_input_create_a,
        "DirectInputCreateW" => fn direct_input_create_w,
        "DirectInputCreateEx" => fn direct_input_create_ex,
        "DllCanUnloadNow" => fn dll_can_unload_now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// c_dfDIJoystick (partiel) : X, Y, 2 curseurs, 1 chapeau, 4 boutons.
    fn joystick_format() -> Vec<(Option<Guid>, u32, u32)> {
        let any = DIDFT_ANYINSTANCE | 0x8000_0000;
        let mut f = vec![
            (Some(GUID_XAXIS), 0, DIDFT_AXIS | any),
            (Some(GUID_YAXIS), 4, DIDFT_AXIS | any),
            (Some(GUID_ZAXIS), 8, DIDFT_AXIS | any),
            (Some(GUID_SLIDER), 24, DIDFT_AXIS | any),
            (Some(GUID_SLIDER), 28, DIDFT_AXIS | any),
            (Some(GUID_POV), 32, DIDFT_POV | any),
        ];
        for b in 0..4 {
            f.push((None, 48 + b, DIDFT_BUTTON | any));
        }
        f
    }

    #[test]
    fn format_joystick_generique() {
        use evdev::*;
        let objs = game_objects(
            &[ABS_X, ABS_Y, ABS_GAS, ABS_HAT0X, ABS_HAT0Y],
            &[BTN_SOUTH, BTN_EAST],
        );
        let f = match_format(&objs, &joystick_format());
        assert_eq!(objs[f[0].obj.unwrap()].src, Src::Axis(ABS_X));
        assert_eq!(objs[f[1].obj.unwrap()].src, Src::Axis(ABS_Y));
        assert!(f[2].obj.is_none(), "pas d'axe Z");
        assert_eq!(objs[f[3].obj.unwrap()].src, Src::Axis(ABS_GAS));
        assert!(f[4].obj.is_none());
        assert!(f[5].pov && f[5].wide);
        assert_eq!(objs[f[5].obj.unwrap()].src, Src::Hat(0));
        assert_eq!(objs[f[6].obj.unwrap()].src, Src::Button(BTN_SOUTH));
        assert_eq!(objs[f[7].obj.unwrap()].src, Src::Button(BTN_EAST));
        assert!(f[8].obj.is_none() && !f[8].wide);
    }

    #[test]
    fn format_clavier_par_instance() {
        let objs = keyboard_objects();
        let entries: Vec<_> = (0..256u32)
            .map(|n| (Some(GUID_KEY), n, DIDFT_BUTTON | (n << 8) | 0x8000_0000))
            .collect();
        let f = match_format(&objs, &entries);
        assert_eq!(objs[f[0x1E].obj.unwrap()].src, Src::Key(0x1E)); // DIK_A
        assert_eq!(objs[f[0xC8].obj.unwrap()].src, Src::Key(0xC8)); // DIK_UP
        assert!(f[0].obj.is_none());
    }

    #[test]
    fn croix_numerique_devient_un_chapeau() {
        use evdev::*;
        let objs = game_objects(
            &[ABS_X],
            &[
                BTN_SOUTH,
                BTN_DPAD_UP,
                BTN_DPAD_DOWN,
                BTN_DPAD_LEFT,
                BTN_DPAD_RIGHT,
            ],
        );
        assert_eq!(objs.iter().filter(|o| o.is_pov()).count(), 1);
        assert_eq!(
            objs.iter()
                .filter(|o| matches!(o.src, Src::Button(_)))
                .count(),
            1
        );
    }

    #[test]
    fn angles_de_chapeau_et_echelle_des_axes() {
        assert_eq!(pov_angle(0, 0), u32::MAX);
        assert_eq!(pov_angle(0, -1), 0);
        assert_eq!(pov_angle(1, 1), 13500);
        let p = AxisProps {
            min: -1000,
            max: 1000,
            deadzone: 2000,
            saturation: 10000,
        };
        assert_eq!(scale_axis(0.1, &p), 0);
        assert_eq!(scale_axis(1.0, &p), 1000);
        assert_eq!(scale_axis(-1.0, &p), -1000);
        assert_eq!(scale_axis(0.0, &AxisProps::default()), 32768);
    }

    #[test]
    fn clavier_et_souris_toujours_enumeres() {
        std::env::set_var("WINLUX_INPUT_DIR", "/nonexistent-winlux-input");
        unsafe {
            let mut di: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                direct_input8_create(0, 0x0800, IID_IDIRECTINPUT8W.as_ptr(), &mut di, 0),
                DI_OK
            );
            let mut seen: Vec<u32> = Vec::new();
            unsafe extern "win64" fn cb(p: *const u8, ctx: *mut c_void) -> i32 {
                let v = &mut *(ctx as *mut Vec<u32>);
                v.push((p.add(36) as *const u32).read_unaligned());
                1
            }
            let this = di as *mut DiObj;
            i_enum_devices(this, 0, cb as usize, &mut seen as *mut _ as *mut c_void, 0);
            assert_eq!(seen.len(), 2);
            assert_eq!(seen[0] & 0xFF, 0x12);
            assert_eq!(seen[1] & 0xFF, 0x13);

            let mut kb: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                i_create_device(this, GUID_SYSKEYBOARD.as_ptr(), &mut kb, 0),
                DI_OK
            );
            let kbd = kb as *mut DevObj;
            // Format clavier : 256 octets.
            let mut odf: Vec<[u8; 24]> = Vec::new();
            for n in 0..256u32 {
                let mut e = [0u8; 24];
                e[..8].copy_from_slice(&(GUID_KEY.as_ptr() as usize).to_le_bytes());
                e[8..12].copy_from_slice(&n.to_le_bytes());
                e[12..16].copy_from_slice(&(DIDFT_BUTTON | (n << 8) | 0x8000_0000).to_le_bytes());
                odf.push(e);
            }
            let mut fmt = [0u8; 32];
            fmt[..4].copy_from_slice(&32u32.to_le_bytes());
            fmt[4..8].copy_from_slice(&24u32.to_le_bytes());
            fmt[12..16].copy_from_slice(&256u32.to_le_bytes());
            fmt[16..20].copy_from_slice(&256u32.to_le_bytes());
            fmt[24..32].copy_from_slice(&(odf.as_ptr() as usize).to_le_bytes());
            let mut state = [0xAAu8; 256];
            assert_eq!(
                d_get_device_state(kbd, 256, state.as_mut_ptr()),
                DIERR_NOTACQUIRED
            );
            assert_eq!(d_set_data_format(kbd, fmt.as_ptr()), DI_OK);
            assert_eq!(d_acquire(kbd), DI_OK);
            assert_eq!(d_get_device_state(kbd, 256, state.as_mut_ptr()), DI_OK);
            assert_eq!(d_release(kbd), 0);
            let mut none: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                i_create_device(this, GUID_JOYSTICK.as_ptr(), &mut none, 0),
                DIERR_DEVICENOTREG
            );
            assert_eq!(i_release(this), 0);
        }
    }
}
