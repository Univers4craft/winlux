//! winmm, lot F de #1410 (#1615) : manettes de l'API joy* et variantes A
//! du mélangeur.
//!
//! Les manettes viennent d'evdev, comme pour DirectInput et XInput. Les
//! structures (JOYCAPS, JOYINFO, JOYINFOEX, MIXERLINEA, MIXERCONTROLA) sont
//! celles de mmsystem.h / joystickapi.h, compactées sur 1 octet.

#![allow(non_snake_case)]

use super::evdev::{self, Device};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const JOYERR_NOERROR: u32 = 0;
const JOYERR_PARMS: u32 = 165;
const JOYERR_UNPLUGGED: u32 = 167;
const MMSYSERR_INVALPARAM: u32 = 11;
const MIXERR_INVALCONTROL: u32 = 1025;
/// Nombre de manettes que l'API joy* adresse (JOYSTICKID1 à 16).
const MAX_JOYSTICKS: usize = 16;
const JOY_POVCENTERED: u32 = 0xFFFF;

// ── Manettes ────────────────────────────────────────────────────────────

/// Liste des manettes, relue au plus toutes les deux secondes : joyGetPosEx
/// est appelé à chaque image.
fn with_devices<R>(f: impl FnOnce(&[Device]) -> R) -> R {
    static CACHE: Mutex<Option<(Instant, Vec<Device>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if c.as_ref()
        .is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2))
    {
        *c = Some((Instant::now(), evdev::scan()));
    }
    f(&c.as_ref().unwrap().1)
}

/// Axes X, Y, Z, R, U, V de joystickapi.h.
const AXES: [usize; 6] = [
    evdev::ABS_X,
    evdev::ABS_Y,
    evdev::ABS_Z,
    evdev::ABS_RZ,
    evdev::ABS_RX,
    evdev::ABS_RY,
];

struct Caps {
    vendor: u16,
    product: u16,
    name: String,
    buttons: u32,
    axes: u32,
    flags: u32,
}

fn caps_of(d: &Device) -> Caps {
    let has = |a: usize| d.axes.contains(&a);
    let mut flags = 0;
    for (bit, a) in [(1, AXES[2]), (2, AXES[3]), (4, AXES[4]), (8, AXES[5])] {
        if has(a) {
            flags |= bit; // JOYCAPS_HASZ, HASR, HASU, HASV
        }
    }
    if has(evdev::ABS_HAT0X) {
        flags |= 0x10 | 0x20; // JOYCAPS_HASPOV | JOYCAPS_POV4DIR
    }
    Caps {
        vendor: d.vendor,
        product: d.product,
        name: d.name.clone(),
        buttons: d.buttons.len().min(32) as u32,
        axes: AXES.iter().filter(|&&a| has(a)).count() as u32,
        flags,
    }
}

fn caps(id: u32) -> Result<Caps, u32> {
    if id as usize >= MAX_JOYSTICKS {
        return Err(JOYERR_PARMS);
    }
    with_devices(|d| d.get(id as usize).map(caps_of)).ok_or(JOYERR_UNPLUGGED)
}

/// JOYCAPS : wMid, wPid, szPname[32], wXmin…wZmax, wNumButtons,
/// wPeriodMin/Max, wRmin…wVmax, wCaps, wMaxAxes, wNumAxes, wMaxButtons,
/// szRegKey[32], szOEMVxD[260]. 728 octets en W, 404 en A.
unsafe fn write_caps(out: *mut u8, c: &Caps, wide: bool) {
    let ch = if wide { 2 } else { 1 };
    let size = 4 + 32 * ch + 24 + 12 + 24 + 16 + 32 * ch + 260 * ch;
    std::ptr::write_bytes(out, 0, size);
    let put16 = |at: usize, v: u16| std::ptr::write_unaligned(out.add(at) as *mut u16, v);
    let put = |at: usize, v: u32| std::ptr::write_unaligned(out.add(at) as *mut u32, v);
    let text = |at: usize, n: usize, s: &str| {
        if wide {
            for (i, u) in s.encode_utf16().take(n - 1).enumerate() {
                std::ptr::write_unaligned(out.add(at + 2 * i) as *mut u16, u);
            }
        } else {
            for (i, b) in crate::dlls::encode_ansi(s)
                .into_iter()
                .take(n - 1)
                .enumerate()
            {
                *out.add(at + i) = b;
            }
        }
    };
    put16(0, c.vendor);
    put16(2, c.product);
    text(4, 32, &c.name);
    let mut at = 4 + 32 * ch;
    // X, Y, Z : 0 à 65535.
    for _ in 0..3 {
        put(at, 0);
        put(at + 4, 0xFFFF);
        at += 8;
    }
    put(at, c.buttons);
    put(at + 4, 10); // wPeriodMin (ms)
    put(at + 8, 1000); // wPeriodMax
    at += 12;
    // R, U, V.
    for _ in 0..3 {
        put(at, 0);
        put(at + 4, 0xFFFF);
        at += 8;
    }
    put(at, c.flags);
    put(at + 4, 6); // wMaxAxes
    put(at + 8, c.axes);
    put(at + 12, 32); // wMaxButtons
}

unsafe fn dev_caps(id: usize, out: *mut u8, size: u32, wide: bool) -> u32 {
    let need = if wide { 728 } else { 404 };
    if out.is_null() || (size as usize) < need {
        return MMSYSERR_INVALPARAM;
    }
    match caps(id as u32) {
        Ok(c) => {
            write_caps(out, &c, wide);
            JOYERR_NOERROR
        }
        Err(e) => e,
    }
}

unsafe extern "win64" fn joyGetDevCapsW(id: usize, out: *mut u8, size: u32) -> u32 {
    dev_caps(id, out, size, true)
}
unsafe extern "win64" fn joyGetDevCapsA(id: usize, out: *mut u8, size: u32) -> u32 {
    dev_caps(id, out, size, false)
}
extern "win64" fn joyGetNumDevs() -> u32 {
    MAX_JOYSTICKS as u32
}

struct Position {
    axes: [u32; 6],
    buttons: u32,
    pressed: u32,
    pov: u32,
}

fn position(id: u32) -> Result<Position, u32> {
    if id as usize >= MAX_JOYSTICKS {
        return Err(JOYERR_PARMS);
    }
    with_devices(|devices| {
        let d = devices.get(id as usize).ok_or(JOYERR_UNPLUGGED)?;
        let s = d.snapshot().ok_or(JOYERR_UNPLUGGED)?;
        let mut axes = [0x7FFF; 6];
        for (i, &a) in AXES.iter().enumerate() {
            if d.axes.contains(&a) {
                axes[i] = (s.abs[a].unsigned() * 65535.0).round() as u32;
            }
        }
        let mut buttons = 0u32;
        for (i, &b) in d.buttons.iter().take(32).enumerate() {
            if s.key(b) {
                buttons |= 1 << i;
            }
        }
        let pov = if d.axes.contains(&evdev::ABS_HAT0X) {
            let x = s.abs[evdev::ABS_HAT0X].value.signum();
            let y = s.abs[evdev::ABS_HAT0Y].value.signum();
            match (x, y) {
                (0, -1) => 0,
                (1, -1) => 4500,
                (1, 0) => 9000,
                (1, 1) => 13500,
                (0, 1) => 18000,
                (-1, 1) => 22500,
                (-1, 0) => 27000,
                (-1, -1) => 31500,
                _ => JOY_POVCENTERED,
            }
        } else {
            JOY_POVCENTERED
        };
        Ok(Position {
            axes,
            buttons,
            pressed: buttons.count_ones(),
            pov,
        })
    })
}

/// JOYINFO { wXpos, wYpos, wZpos, wButtons } : quatre premiers boutons.
unsafe extern "win64" fn joyGetPos(id: u32, out: *mut u32) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    match position(id) {
        Ok(p) => {
            for i in 0..3 {
                std::ptr::write_unaligned(out.add(i), p.axes[i]);
            }
            std::ptr::write_unaligned(out.add(3), p.buttons & 0xF);
            JOYERR_NOERROR
        }
        Err(e) => e,
    }
}

/// JOYINFOEX (52 octets) : dwSize, dwFlags, X, Y, Z, R, U, V, dwButtons,
/// dwButtonNumber, dwPOV, deux réservés.
unsafe extern "win64" fn joyGetPosEx(id: u32, out: *mut u32) -> u32 {
    if out.is_null() || std::ptr::read_unaligned(out) < 52 {
        return JOYERR_PARMS;
    }
    match position(id) {
        Ok(p) => {
            for i in 0..6 {
                std::ptr::write_unaligned(out.add(2 + i), p.axes[i]);
            }
            std::ptr::write_unaligned(out.add(8), p.buttons);
            std::ptr::write_unaligned(out.add(9), p.pressed);
            std::ptr::write_unaligned(out.add(10), p.pov);
            JOYERR_NOERROR
        }
        Err(e) => e,
    }
}

static THRESHOLDS: [AtomicU32; MAX_JOYSTICKS] = [const { AtomicU32::new(0) }; MAX_JOYSTICKS];

unsafe extern "win64" fn joyGetThreshold(id: u32, out: *mut u32) -> u32 {
    if out.is_null() || id as usize >= MAX_JOYSTICKS {
        return JOYERR_PARMS;
    }
    *out = THRESHOLDS[id as usize].load(Ordering::Relaxed);
    JOYERR_NOERROR
}

extern "win64" fn joySetThreshold(id: u32, threshold: u32) -> u32 {
    if id as usize >= MAX_JOYSTICKS {
        return JOYERR_PARMS;
    }
    THRESHOLDS[id as usize].store(threshold, Ordering::Relaxed);
    JOYERR_NOERROR
}

// ── Mélangeur : variantes A ─────────────────────────────────────────────

/// Copie `chars` caractères UTF-16 de `src` en ANSI vers `dst`.
unsafe fn narrow(src: *const u8, dst: *mut u8, chars: usize) {
    let units: Vec<u16> = (0..chars)
        .map(|i| std::ptr::read_unaligned(src.add(2 * i) as *const u16))
        .take_while(|&c| c != 0)
        .collect();
    let bytes = crate::dlls::encode_ansi(&String::from_utf16_lossy(&units));
    std::ptr::write_bytes(dst, 0, chars);
    let n = bytes.len().min(chars - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, n);
}

/// MIXERLINEA : jusqu'à cControls comme la variante W (20 + dwUser + 16),
/// puis szShortName[16], szName[64] et Target { dwType, dwDeviceID, wMid,
/// wPid, vDriverVersion, szPname[32] } en ANSI.
unsafe extern "win64" fn mixerGetLineInfoA(obj: usize, line: *mut u8, flags: u32) -> u32 {
    if line.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let ps = crate::wow32::pointer_size();
    let o = 20 + ps;
    let size_a = o + 16 + 16 + 64 + 16 + 32;
    if (std::ptr::read_unaligned(line as *const u32) as usize) < size_a {
        return MMSYSERR_INVALPARAM;
    }
    let size_w = 276 + ps;
    let mut w = vec![0u8; size_w];
    std::ptr::copy_nonoverlapping(line, w.as_mut_ptr(), o + 4);
    // Target (recherche par MIXER_GETLINEINFOF_TARGETTYPE) : dwType…vDriverVersion.
    std::ptr::copy_nonoverlapping(line.add(o + 96), w.as_mut_ptr().add(o + 176), 16);
    std::ptr::write_unaligned(w.as_mut_ptr() as *mut u32, size_w as u32);
    let r = super::icue_win32::mixerGetLineInfoW(obj, w.as_mut_ptr(), flags);
    if r != 0 {
        return r;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr().add(4), line.add(4), o + 12);
    narrow(w.as_ptr().add(o + 16), line.add(o + 16), 16);
    narrow(w.as_ptr().add(o + 48), line.add(o + 32), 64);
    std::ptr::copy_nonoverlapping(w.as_ptr().add(o + 176), line.add(o + 96), 16);
    narrow(w.as_ptr().add(o + 192), line.add(o + 112), 32);
    0
}

/// MIXERCONTROLA (148 octets) depuis MIXERCONTROLW (228).
unsafe fn control_to_a(w: *const u8, a: *mut u8) {
    std::ptr::write_bytes(a, 0, 148);
    std::ptr::copy_nonoverlapping(w, a, 20);
    std::ptr::write_unaligned(a as *mut u32, 148);
    narrow(w.add(20), a.add(20), 16);
    narrow(w.add(52), a.add(36), 64);
    std::ptr::copy_nonoverlapping(w.add(180), a.add(100), 48); // Bounds, Metrics
}

/// MIXERLINECONTROLSA : même en-tête que la variante W, tableau de
/// MIXERCONTROLA.
unsafe extern "win64" fn mixerGetLineControlsA(obj: usize, lc: *mut u8, flags: u32) -> u32 {
    if lc.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let ps = crate::wow32::pointer_size();
    let count = std::ptr::read_unaligned(lc.add(12) as *const u32).max(1) as usize;
    let cb = std::ptr::read_unaligned(lc.add(16) as *const u32) as usize;
    let arr = if ps == 4 {
        std::ptr::read_unaligned(lc.add(20) as *const u32) as usize
    } else {
        std::ptr::read_unaligned(lc.add(20) as *const u64) as usize
    } as *mut u8;
    if arr.is_null() || cb < 148 {
        return MMSYSERR_INVALPARAM;
    }
    let mut controls = vec![0u8; count * 228];
    let mut header = vec![0u8; 20 + ps];
    std::ptr::copy_nonoverlapping(lc, header.as_mut_ptr(), 20 + ps);
    std::ptr::write_unaligned(header.as_mut_ptr().add(16) as *mut u32, 228);
    let p = controls.as_mut_ptr() as usize;
    if ps == 4 {
        std::ptr::write_unaligned(header.as_mut_ptr().add(20) as *mut u32, p as u32);
    } else {
        std::ptr::write_unaligned(header.as_mut_ptr().add(20) as *mut u64, p as u64);
    }
    let r = super::icue_win32::mixerGetLineControlsW(obj, header.as_mut_ptr(), flags);
    if r != 0 {
        return r;
    }
    // dwLineID rendu par MIXER_GETLINECONTROLSF_ONEBYID.
    std::ptr::copy_nonoverlapping(header.as_ptr().add(4), lc.add(4), 4);
    let filled = if flags & 0xF == 0 { count } else { 1 };
    for i in 0..filled {
        control_to_a(controls.as_ptr().add(i * 228), arr.add(i * cb));
    }
    0
}

/// mixerGetControlDetailsA : les valeurs ne contiennent pas de texte ; les
/// contrôles de Winlux n'ont pas de liste (MIXER_GETCONTROLDETAILSF_LISTTEXT).
unsafe extern "win64" fn mixerGetControlDetailsA(obj: usize, cd: *mut u8, flags: u32) -> u32 {
    if flags & 0xF == 1 {
        return MIXERR_INVALCONTROL;
    }
    super::icue_win32::mixerGetControlDetailsW(obj, cd, flags)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "joyGetNumDevs" => fn joyGetNumDevs,
        "joyGetDevCapsA" => fn joyGetDevCapsA, "joyGetDevCapsW" => fn joyGetDevCapsW,
        "joyGetPos" => fn joyGetPos, "joyGetPosEx" => fn joyGetPosEx,
        "joyGetThreshold" => fn joyGetThreshold, "joySetThreshold" => fn joySetThreshold,
        "mixerGetLineInfoA" => fn mixerGetLineInfoA,
        "mixerGetLineControlsA" => fn mixerGetLineControlsA,
        "mixerGetControlDetailsA" => fn mixerGetControlDetailsA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manettes_absentes_et_melangeur_ansi() {
        unsafe {
            assert_eq!(joyGetNumDevs(), 16);
            let mut caps = [0u8; 404];
            assert_eq!(joyGetDevCapsA(99, caps.as_mut_ptr(), 404), JOYERR_PARMS);
            assert_eq!(
                joyGetDevCapsA(0, caps.as_mut_ptr(), 10),
                MMSYSERR_INVALPARAM
            );
            assert_eq!(joySetThreshold(1, 250), 0);
            let mut t = 0u32;
            assert_eq!(joyGetThreshold(1, &mut t), 0);
            assert_eq!(t, 250);
            let mut info = [0u32; 13];
            info[0] = 52;
            assert_eq!(joyGetPosEx(20, info.as_mut_ptr()), JOYERR_PARMS);
            // MIXERLINEA, destination 0.
            let ps = crate::wow32::pointer_size();
            let size = 20 + ps + 144;
            let mut line = vec![0u8; size];
            line[..4].copy_from_slice(&(size as u32).to_le_bytes());
            assert_eq!(mixerGetLineInfoA(0, line.as_mut_ptr(), 0), 0);
            let o = 20 + ps;
            assert_eq!(&line[o + 32..o + 45], b"Haut-parleurs");
            assert_eq!(&line[o + 112..o + 118], b"Winlux");
            // Contrôles de la ligne en MIXERCONTROLA.
            let id = u32::from_le_bytes(line[12..16].try_into().unwrap());
            let mut controls = vec![0u8; 2 * 148];
            let mut lc = vec![0u8; 20 + ps];
            lc[..4].copy_from_slice(&((20 + ps) as u32).to_le_bytes());
            lc[4..8].copy_from_slice(&id.to_le_bytes());
            lc[12..16].copy_from_slice(&2u32.to_le_bytes());
            lc[16..20].copy_from_slice(&148u32.to_le_bytes());
            lc[20..28].copy_from_slice(&(controls.as_mut_ptr() as u64).to_le_bytes());
            assert_eq!(mixerGetLineControlsA(0, lc.as_mut_ptr(), 0), 0);
            assert_eq!(&controls[36..42], b"Volume");
            assert_eq!(
                u32::from_le_bytes(controls[104..108].try_into().unwrap()),
                0xFFFF
            );
            assert_eq!(&controls[148 + 36..148 + 40], b"Muet");
        }
    }
}
