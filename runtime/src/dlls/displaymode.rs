//! Définition d'écran et rampe gamma des jeux (#271) :
//! `EnumDisplaySettings(Ex)A/W`, `ChangeDisplaySettings(Ex)A/W`,
//! `SetDeviceGammaRamp`, `GetDeviceGammaRamp`.
//!
//! Un jeu en plein écran « exclusif » passe en 1280x720 à 144 Hz, ou
//! assombrit l'image par sa rampe gamma. Winlux applique ces demandes avec
//! RandR (`xrandr --output … --mode … --rate …`, et la rampe de la CRTC) et
//! les **défait à la sortie du programme**, même s'il plante : au premier
//! changement, un petit gardien (`winlux --display-restore`) est lancé avec
//! l'état d'origine ; il attend la fermeture de son entrée standard (qui
//! survient à la mort du programme, quelle qu'en soit la cause) et remet
//! l'écran comme il était. Rien n'est jamais écrit dans la configuration
//! d'écran mémorisée (CDS_UPDATEREGISTRY est traité comme un changement
//! temporaire, ce que font les jeux de toute façon).
#![allow(non_snake_case)]

use std::io::Write;
use std::sync::Mutex;

const DISP_CHANGE_SUCCESSFUL: i32 = 0;
const DISP_CHANGE_FAILED: i32 = -1;
const DISP_CHANGE_BADMODE: i32 = -2;
const CDS_TEST: u32 = 0x2;
const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
const ENUM_REGISTRY_SETTINGS: u32 = u32::MAX - 1;
const DM_BITSPERPEL: u32 = 0x4_0000;
const DM_PELSWIDTH: u32 = 0x8_0000;
const DM_PELSHEIGHT: u32 = 0x10_0000;
const DM_DISPLAYFREQUENCY: u32 = 0x40_0000;
const WM_DISPLAYCHANGE: u32 = 0x007E;

// ============================================================
//  xrandr --query
// ============================================================

#[derive(Clone, Debug, PartialEq)]
pub struct ModeLine {
    pub w: u32,
    pub h: u32,
    /// Fréquences en hertz (arrondies), dans l'ordre de xrandr.
    pub rates: Vec<(u32, String)>,
    pub current: Option<u32>,
    pub preferred: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Screen {
    pub name: String,
    pub primary: bool,
    pub active: bool,
    pub x: i32,
    pub y: i32,
    pub modes: Vec<ModeLine>,
}

fn xrandr_bin() -> String {
    std::env::var("WINLUX_XRANDR").unwrap_or_else(|_| "xrandr".into())
}

fn xrandr(args: &[String]) -> Option<String> {
    let out = std::process::Command::new(xrandr_bin())
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Analyse `xrandr --query`, fréquences comprises (« 60.00*+ 59.94 »).
pub fn parse(text: &str) -> Vec<Screen> {
    let mut out: Vec<Screen> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.get(1) != Some(&"connected") {
                if words.get(1) == Some(&"disconnected") {
                    out.push(Screen {
                        name: String::new(),
                        ..Default::default()
                    });
                }
                continue;
            }
            let mut s = Screen {
                name: words[0].to_string(),
                primary: words.get(2) == Some(&"primary"),
                ..Default::default()
            };
            if let Some(g) = words.iter().find(|w| w.contains('x') && w.contains('+')) {
                let mut parts = g.split('+');
                let _size = parts.next();
                s.x = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.y = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.active = true;
            }
            out.push(s);
            continue;
        }
        let Some(s) = out.last_mut() else { continue };
        if s.name.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(size) = words.next() else { continue };
        let Some((w, h)) = size.split_once('x') else {
            continue;
        };
        let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.trim_end_matches('i').parse::<u32>()) else {
            continue;
        };
        if size.ends_with('i') {
            continue; // entrelacé
        }
        let mut m = ModeLine {
            w,
            h,
            rates: Vec::new(),
            current: None,
            preferred: false,
        };
        for r in words {
            let clean = r.trim_end_matches(['*', '+']);
            let Ok(hz) = clean.parse::<f64>() else {
                continue;
            };
            let hz_i = hz.round() as u32;
            if r.contains('*') {
                m.current = Some(hz_i);
            }
            if r.contains('+') {
                m.preferred = true;
            }
            if !m.rates.iter().any(|(x, _)| *x == hz_i) {
                m.rates.push((hz_i, clean.to_string()));
            }
        }
        if !m.rates.is_empty() && !s.modes.iter().any(|x| x.w == w && x.h == h) {
            s.modes.push(m);
        }
    }
    out.retain(|s| !s.name.is_empty());
    out
}

pub fn screens() -> Vec<Screen> {
    xrandr(&["--query".into()])
        .map(|t| parse(&t))
        .unwrap_or_default()
}

/// \\.\DISPLAY1 = écran principal, puis les autres écrans allumés.
pub fn ordered(mut list: Vec<Screen>) -> Vec<Screen> {
    list.retain(|s| s.active);
    list.sort_by_key(|s| (!s.primary, s.x, s.y));
    list
}

fn screen_for(device: Option<String>) -> Option<Screen> {
    let list = ordered(screens());
    let index = device
        .as_deref()
        .and_then(|d| {
            d.to_ascii_uppercase()
                .strip_prefix("\\\\.\\DISPLAY")?
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(1);
    list.into_iter().nth(index.max(1) - 1)
}

/// Modes énumérés : chaque (définition, fréquence), du plus grand au plus
/// petit, en 32 bits par pixel seulement, comme Windows 8 et suivants (SDL3
/// journalisait chaque mode 16 bits comme « bits < 8 »). Une demande en
/// 16 bits reste acceptée par ChangeDisplaySettings.
pub fn mode_list(s: &Screen) -> Vec<(u32, u32, u32, u32)> {
    let mut v = Vec::new();
    for m in &s.modes {
        for (hz, _) in &m.rates {
            v.push((m.w, m.h, *hz, 32));
        }
    }
    v
}

pub fn current(s: &Screen) -> Option<(u32, u32, u32)> {
    s.modes
        .iter()
        .find_map(|m| m.current.map(|hz| (m.w, m.h, hz)))
}

// ============================================================
//  DEVMODE
// ============================================================

/// Décalage des champs après dmDeviceName (64 octets en W, 32 en A).
fn field(wide: bool, off_w: usize) -> usize {
    if wide {
        off_w
    } else {
        off_w - 32
    }
}

unsafe fn write_devmode(
    dm: *mut u8,
    wide: bool,
    name: &str,
    w: u32,
    h: u32,
    hz: u32,
    bpp: u32,
    x: i32,
    y: i32,
) -> bool {
    let declared = (dm.add(field(wide, 68)) as *const u16).read_unaligned() as usize;
    let full = if wide { 220 } else { 156 };
    let size = if declared == 0 {
        full
    } else {
        declared.min(full)
    };
    if size < field(wide, 188) {
        return false;
    }
    std::ptr::write_bytes(dm, 0, size);
    for (i, c) in name.encode_utf16().take(31).enumerate() {
        if wide {
            (dm.add(i * 2) as *mut u16).write_unaligned(c);
        } else {
            *dm.add(i) = c as u8;
        }
    }
    (dm.add(field(wide, 64)) as *mut u16).write_unaligned(0x0401);
    (dm.add(field(wide, 68)) as *mut u16).write_unaligned(size as u16);
    (dm.add(field(wide, 72)) as *mut u32).write_unaligned(
        0x20 | DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | 0x20_0000,
    );
    (dm.add(field(wide, 76)) as *mut i32).write_unaligned(x); // dmPosition
    (dm.add(field(wide, 80)) as *mut i32).write_unaligned(y);
    (dm.add(field(wide, 166)) as *mut u16).write_unaligned(96);
    (dm.add(field(wide, 168)) as *mut u32).write_unaligned(bpp);
    (dm.add(field(wide, 172)) as *mut u32).write_unaligned(w);
    (dm.add(field(wide, 176)) as *mut u32).write_unaligned(h);
    (dm.add(field(wide, 184)) as *mut u32).write_unaligned(hz);
    true
}

unsafe fn device_name_w(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut n = 0;
    while *p.add(n) != 0 && n < 64 {
        n += 1;
    }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)))
}
unsafe fn device_name_a(p: *const u8) -> Option<String> {
    (!p.is_null()).then(|| {
        std::ffi::CStr::from_ptr(p as *const libc::c_char)
            .to_string_lossy()
            .into_owned()
    })
}

unsafe fn enum_settings(device: Option<String>, mode: u32, dm: *mut u8, wide: bool) -> i32 {
    if dm.is_null() {
        return 0;
    }
    let Some(s) = screen_for(device) else {
        // Pas de serveur X ni de xrandr : l'écran tel que l'interface le voit.
        if mode != ENUM_CURRENT_SETTINGS && mode != ENUM_REGISTRY_SETTINGS && mode != 0 {
            return 0;
        }
        let (w, h) = crate::dlls::gui::screen_size();
        return write_devmode(
            dm,
            wide,
            "\\\\.\\DISPLAY1",
            w.max(1) as u32,
            h.max(1) as u32,
            60,
            32,
            0,
            0,
        ) as i32;
    };
    let name = "\\\\.\\DISPLAY1";
    let (w, h, hz, bpp) = match mode {
        ENUM_CURRENT_SETTINGS | ENUM_REGISTRY_SETTINGS => {
            let saved = if mode == ENUM_REGISTRY_SETTINGS {
                crate::dlls::lock(&SAVED)
                    .iter()
                    .find(|o| o.output == s.name)
                    .map(|o| (o.w, o.h, o.hz))
            } else {
                None
            };
            let Some((w, h, hz)) = saved.or_else(|| current(&s)) else {
                return 0;
            };
            (w, h, hz, 32)
        }
        n => match mode_list(&s).get(n as usize) {
            Some(m) => *m,
            None => return 0,
        },
    };
    write_devmode(dm, wide, name, w, h, hz, bpp, s.x, s.y) as i32
}

pub unsafe extern "win64" fn EnumDisplaySettingsW(
    device: *const u16,
    mode: u32,
    dm: *mut u8,
) -> i32 {
    enum_settings(device_name_w(device), mode, dm, true)
}
pub unsafe extern "win64" fn EnumDisplaySettingsA(
    device: *const u8,
    mode: u32,
    dm: *mut u8,
) -> i32 {
    enum_settings(device_name_a(device), mode, dm, false)
}
pub unsafe extern "win64" fn EnumDisplaySettingsExW(
    device: *const u16,
    mode: u32,
    dm: *mut u8,
    _f: u32,
) -> i32 {
    enum_settings(device_name_w(device), mode, dm, true)
}
pub unsafe extern "win64" fn EnumDisplaySettingsExA(
    device: *const u8,
    mode: u32,
    dm: *mut u8,
    _f: u32,
) -> i32 {
    enum_settings(device_name_a(device), mode, dm, false)
}

// ============================================================
//  ChangeDisplaySettings et gardien de restauration
// ============================================================

#[derive(Clone, Debug, PartialEq)]
struct SavedMode {
    output: String,
    w: u32,
    h: u32,
    hz: u32,
    rate: String,
}
static SAVED: Mutex<Vec<SavedMode>> = Mutex::new(Vec::new());
static GUARD: Mutex<Option<std::process::ChildStdin>> = Mutex::new(None);

/// Envoie une ligne au gardien (lancé au premier besoin).
fn tell_guard(line: &str) {
    let mut g = crate::dlls::lock(&GUARD);
    if g.is_none() {
        use std::os::unix::process::CommandExt;
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--display-restore")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Hors du groupe du programme : un Ctrl+C ne doit pas le tuer avant
        // qu'il ait remis l'écran.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        match cmd.spawn() {
            Ok(mut child) => *g = child.stdin.take(),
            Err(_) => return,
        }
    }
    if let Some(stdin) = g.as_mut() {
        let _ = writeln!(stdin, "{line}");
        let _ = stdin.flush();
    }
}

fn mode_args(output: &str, w: u32, h: u32, rate: &str) -> Vec<String> {
    vec![
        "--output".into(),
        output.into(),
        "--mode".into(),
        format!("{w}x{h}"),
        "--rate".into(),
        rate.into(),
    ]
}

/// Choix du mode demandé : définition exacte, fréquence demandée (sinon la
/// fréquence actuelle si elle existe, sinon la plus haute).
pub fn pick(s: &Screen, w: u32, h: u32, hz: Option<u32>) -> Option<(u32, String)> {
    let m = s.modes.iter().find(|m| m.w == w && m.h == h)?;
    let cur = current(s).map(|c| c.2);
    match hz.filter(|f| *f > 1) {
        Some(f) => m.rates.iter().find(|(r, _)| *r == f).cloned(),
        None => cur
            .and_then(|c| m.rates.iter().find(|(r, _)| *r == c).cloned())
            .or_else(|| m.rates.iter().max_by_key(|(r, _)| *r).cloned()),
    }
}

unsafe fn change(device: Option<String>, dm: *const u8, flags: u32, wide: bool) -> i32 {
    let Some(s) = screen_for(device) else {
        return DISP_CHANGE_FAILED;
    };
    let fields = if dm.is_null() {
        0
    } else {
        (dm.add(field(wide, 72)) as *const u32).read_unaligned()
    };
    // DEVMODE nul (ou sans champ) : retour au mode d'origine.
    if fields & (DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL) == 0 {
        if flags & CDS_TEST != 0 {
            return DISP_CHANGE_SUCCESSFUL;
        }
        let saved = crate::dlls::lock(&SAVED)
            .iter()
            .find(|o| o.output == s.name)
            .cloned();
        if let Some(o) = saved {
            if xrandr(&mode_args(&o.output, o.w, o.h, &o.rate)).is_none() {
                return DISP_CHANGE_FAILED;
            }
            crate::dlls::lock(&SAVED).retain(|x| x.output != o.output);
            tell_guard(&format!("forget {}", o.output));
            notify(o.w, o.h);
        }
        return DISP_CHANGE_SUCCESSFUL;
    }
    let Some((cw, ch, chz)) = current(&s) else {
        return DISP_CHANGE_FAILED;
    };
    let get = |off: usize, bit: u32, default: u32| {
        if fields & bit != 0 {
            (dm.add(field(wide, off)) as *const u32).read_unaligned()
        } else {
            default
        }
    };
    let w = get(172, DM_PELSWIDTH, cw);
    let h = get(176, DM_PELSHEIGHT, ch);
    let bpp = get(168, DM_BITSPERPEL, 32);
    let hz = (fields & DM_DISPLAYFREQUENCY != 0).then(|| get(184, DM_DISPLAYFREQUENCY, chz));
    if ![8, 16, 24, 32].contains(&bpp) {
        return DISP_CHANGE_BADMODE;
    }
    let Some((hz, rate)) = pick(&s, w, h, hz) else {
        return DISP_CHANGE_BADMODE;
    };
    if flags & CDS_TEST != 0 {
        return DISP_CHANGE_SUCCESSFUL;
    }
    if (w, h, hz) == (cw, ch, chz) {
        return DISP_CHANGE_SUCCESSFUL;
    }
    {
        let mut saved = crate::dlls::lock(&SAVED);
        if !saved.iter().any(|o| o.output == s.name) {
            let rate_text = s
                .modes
                .iter()
                .find(|m| m.w == cw && m.h == ch)
                .and_then(|m| m.rates.iter().find(|(r, _)| *r == chz))
                .map(|(_, t)| t.clone())
                .unwrap_or_else(|| chz.to_string());
            saved.push(SavedMode {
                output: s.name.clone(),
                w: cw,
                h: ch,
                hz: chz,
                rate: rate_text.clone(),
            });
            drop(saved);
            tell_guard(&format!("mode {} {cw}x{ch} {rate_text}", s.name));
        }
    }
    if xrandr(&mode_args(&s.name, w, h, &rate)).is_none() {
        return DISP_CHANGE_FAILED;
    }
    notify(w, h);
    DISP_CHANGE_SUCCESSFUL
}

/// WM_DISPLAYCHANGE à la fenêtre du programme.
fn notify(w: u32, h: u32) {
    if let Some(hwnd) = crate::dlls::gui::first_window() {
        let lparam = ((h as isize & 0xFFFF) << 16) | (w as isize & 0xFFFF);
        unsafe {
            crate::dlls::user32::post_message_w(hwnd, WM_DISPLAYCHANGE, 32, lparam);
        }
    }
}

pub unsafe extern "win64" fn ChangeDisplaySettingsExW(
    device: *const u16,
    dm: *const u8,
    _hwnd: usize,
    flags: u32,
    _param: usize,
) -> i32 {
    change(device_name_w(device), dm, flags, true)
}
pub unsafe extern "win64" fn ChangeDisplaySettingsExA(
    device: *const u8,
    dm: *const u8,
    _hwnd: usize,
    flags: u32,
    _param: usize,
) -> i32 {
    change(device_name_a(device), dm, flags, false)
}
pub unsafe extern "win64" fn ChangeDisplaySettingsW(dm: *const u8, flags: u32) -> i32 {
    change(None, dm, flags, true)
}
pub unsafe extern "win64" fn ChangeDisplaySettingsA(dm: *const u8, flags: u32) -> i32 {
    change(None, dm, flags, false)
}

// ============================================================
//  Rampe gamma (RandR, CRTC de l'écran principal)
// ============================================================

/// Ramène une rampe de `from.len()` points à `n` points (interpolation).
pub fn resample(from: &[u16], n: usize) -> Vec<u16> {
    if from.is_empty() || n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            if n == 1 {
                return from[0];
            }
            let pos = i as f64 * (from.len() - 1) as f64 / (n - 1) as f64;
            let (a, frac) = (pos.floor() as usize, pos.fract());
            let b = (a + 1).min(from.len() - 1);
            (from[a] as f64 * (1.0 - frac) + from[b] as f64 * frac).round() as u16
        })
        .collect()
}

struct Gamma {
    conn: x11rb::rust_connection::RustConnection,
    crtcs: Vec<u32>,
}

fn gamma_target() -> Option<Gamma> {
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt as _;
    let (conn, screen) = x11rb::rust_connection::RustConnection::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    conn.randr_query_version(1, 3).ok()?.reply().ok()?;
    let res = conn
        .randr_get_screen_resources_current(root)
        .ok()?
        .reply()
        .ok()?;
    let primary = conn
        .randr_get_output_primary(root)
        .ok()?
        .reply()
        .ok()?
        .output;
    let mut crtcs = Vec::new();
    for out in &res.outputs {
        let Ok(info) = conn
            .randr_get_output_info(*out, res.config_timestamp)
            .ok()?
            .reply()
        else {
            continue;
        };
        if info.crtc != 0 && (*out == primary || primary == 0) {
            crtcs.push(info.crtc);
        }
    }
    if crtcs.is_empty() {
        crtcs = res.crtcs.iter().copied().take(1).collect();
    }
    Some(Gamma { conn, crtcs })
}

static GAMMA_SAVED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn hex(v: &[u16]) -> String {
    v.iter().map(|x| format!("{x:04x}")).collect()
}
fn unhex(s: &str) -> Vec<u16> {
    (0..s.len() / 4)
        .filter_map(|i| u16::from_str_radix(&s[i * 4..i * 4 + 4], 16).ok())
        .collect()
}

pub unsafe extern "win64" fn SetDeviceGammaRamp(_hdc: usize, ramp: *const u16) -> i32 {
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt as _;
    if ramp.is_null() {
        return 0;
    }
    let src = std::slice::from_raw_parts(ramp, 768);
    let Some(g) = gamma_target() else { return 0 };
    for crtc in &g.crtcs {
        let Ok(Ok(cur)) = g.conn.randr_get_crtc_gamma(*crtc).map(|c| c.reply()) else {
            continue;
        };
        let size = cur.red.len();
        if size == 0 {
            continue;
        }
        {
            let mut saved = crate::dlls::lock(&GAMMA_SAVED);
            if !saved.contains(crtc) {
                saved.push(*crtc);
                tell_guard(&format!(
                    "gamma {crtc} {} {} {}",
                    hex(&cur.red),
                    hex(&cur.green),
                    hex(&cur.blue)
                ));
            }
        }
        let r = resample(&src[0..256], size);
        let gr = resample(&src[256..512], size);
        let b = resample(&src[512..768], size);
        let _ = g.conn.randr_set_crtc_gamma(*crtc, &r, &gr, &b);
    }
    let _ = g.conn.flush();
    1
}

pub unsafe extern "win64" fn GetDeviceGammaRamp(_hdc: usize, ramp: *mut u16) -> i32 {
    use x11rb::protocol::randr::ConnectionExt as _;
    if ramp.is_null() {
        return 0;
    }
    let out = std::slice::from_raw_parts_mut(ramp, 768);
    let read = gamma_target().and_then(|g| {
        let crtc = *g.crtcs.first()?;
        g.conn.randr_get_crtc_gamma(crtc).ok()?.reply().ok()
    });
    match read {
        Some(c) if !c.red.is_empty() => {
            out[0..256].copy_from_slice(&resample(&c.red, 256));
            out[256..512].copy_from_slice(&resample(&c.green, 256));
            out[512..768].copy_from_slice(&resample(&c.blue, 256));
        }
        _ => {
            // Sans RandR : la rampe neutre, comme un écran non étalonné.
            for i in 0..256 {
                let v = (i * 257) as u16;
                out[i] = v;
                out[256 + i] = v;
                out[512 + i] = v;
            }
        }
    }
    1
}

// ============================================================
//  Gardien : winlux --display-restore
// ============================================================

/// Lit l'état d'origine sur l'entrée standard jusqu'à sa fermeture (fin du
/// programme surveillé), puis remet définitions et rampes.
pub fn restore_guard() -> i32 {
    use std::io::BufRead;
    let mut modes: Vec<(String, String, String)> = Vec::new();
    let mut gammas: Vec<(u32, Vec<u16>, Vec<u16>, Vec<u16>)> = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["mode", out, size, rate] => {
                if !modes.iter().any(|m| m.0 == *out) {
                    modes.push((out.to_string(), size.to_string(), rate.to_string()));
                }
            }
            ["forget", out] => modes.retain(|m| m.0 != *out),
            ["gamma", crtc, r, g, b] => {
                if let Ok(c) = crtc.parse() {
                    if !gammas.iter().any(|x| x.0 == c) {
                        gammas.push((c, unhex(r), unhex(g), unhex(b)));
                    }
                }
            }
            _ => {}
        }
    }
    for (out, size, rate) in &modes {
        let _ = std::process::Command::new(xrandr_bin())
            .args(["--output", out, "--mode", size, "--rate", rate])
            .status();
    }
    if !gammas.is_empty() {
        use x11rb::connection::Connection;
        use x11rb::protocol::randr::ConnectionExt as _;
        if let Ok((conn, _)) = x11rb::rust_connection::RustConnection::connect(None) {
            for (crtc, r, g, b) in &gammas {
                let _ = conn.randr_set_crtc_gamma(*crtc, r, g, b);
            }
            let _ = conn.flush();
        }
    }
    0
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "EnumDisplaySettingsW" => fn EnumDisplaySettingsW,
        "EnumDisplaySettingsA" => fn EnumDisplaySettingsA,
        "EnumDisplaySettingsExW" => fn EnumDisplaySettingsExW,
        "EnumDisplaySettingsExA" => fn EnumDisplaySettingsExA,
        "ChangeDisplaySettingsW" => fn ChangeDisplaySettingsW,
        "ChangeDisplaySettingsA" => fn ChangeDisplaySettingsA,
        "ChangeDisplaySettingsExW" => fn ChangeDisplaySettingsExW,
        "ChangeDisplaySettingsExA" => fn ChangeDisplaySettingsExA,
    }
}

pub fn gdi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetDeviceGammaRamp" => fn SetDeviceGammaRamp,
        "GetDeviceGammaRamp" => fn GetDeviceGammaRamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "Screen 0: minimum 320 x 200, current 2560 x 1440, maximum 16384 x 16384
DP-1 connected primary 2560x1440+0+0 (normal left inverted right x axis y axis) 597mm x 336mm
   2560x1440    143.97*+ 119.88    59.95
   1920x1080    119.88    60.00    59.94
   1280x720      60.00    59.94
   1920x1080i    60.00
HDMI-1 disconnected (normal left inverted right x axis y axis)
";

    #[test]
    fn analyse_les_modes_et_frequences() {
        let s = parse(QUERY);
        assert_eq!(s.len(), 1);
        assert!(s[0].primary && s[0].active);
        assert_eq!(s[0].modes.len(), 3);
        assert_eq!(current(&s[0]), Some((2560, 1440, 144)));
        assert_eq!(
            s[0].modes[1].rates.iter().map(|r| r.0).collect::<Vec<_>>(),
            vec![120, 60]
        );
        assert_eq!(mode_list(&s[0]).len(), 3 + 2 + 1);
        assert_eq!(
            pick(&s[0], 1920, 1080, Some(60)),
            Some((60, "60.00".into()))
        );
        assert_eq!(pick(&s[0], 1920, 1080, None), Some((120, "119.88".into())));
        assert_eq!(pick(&s[0], 1234, 567, None), None);
    }

    #[test]
    fn reechantillonne_la_rampe() {
        let id: Vec<u16> = (0..256).map(|i| (i * 257) as u16).collect();
        let r = resample(&id, 1024);
        assert_eq!(r.len(), 1024);
        assert_eq!((r[0], r[1023]), (0, 65535));
        assert_eq!(resample(&r, 256), id);
        assert_eq!(unhex(&hex(&[1, 0xABCD])), vec![1, 0xABCD]);
    }
}
