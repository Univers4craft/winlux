//! wintab32.dll maison (#1199) : l'interface de tablette WinTab 1.4, écrite
//! d'après sa spécification publique (wintab.h, pktdef.h).
//!
//! Une seule tablette, celle que le serveur X décrit (voir `pointer`) :
//! la tablette couvre l'écran, dix unités par pixel, origine en bas à
//! gauche comme le veut WinTab ; pression 0..1023 ; orientation en dixièmes
//! de degré (azimut 0..3599, altitude 0..900). Trois curseurs : 0 souris,
//! 1 pointe du stylet, 2 gomme.
//!
//! Les paquets ne vont qu'au contexte activé le plus haut (WTOverlap), dans
//! sa file (WTPacketsGet, WTPacket…), avec WT_PACKET posté à sa fenêtre
//! quand CXO_MESSAGES est demandé.

use crate::dlls::gui::{self, Msg};
use crate::dlls::pointer::Pen;
use std::collections::VecDeque;
use std::sync::Mutex;

#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

// Catégories et index de WTInfo.
const WTI_INTERFACE: u32 = 1;
const WTI_STATUS: u32 = 2;
const WTI_DEFCONTEXT: u32 = 3;
const WTI_DEFSYSCTX: u32 = 4;
const WTI_DEVICES: u32 = 100;
const WTI_CURSORS: u32 = 200;
const WTI_DDCTXS: u32 = 400;
const WTI_DSCTXS: u32 = 500;

// Options de contexte.
const CXO_SYSTEM: u32 = 0x1;
const CXO_PEN: u32 = 0x2;
const CXO_MESSAGES: u32 = 0x4;
const CXO_CSRMESSAGES: u32 = 0x8;

// Champs de paquet (WTPKT).
const PK_CONTEXT: u32 = 0x1;
const PK_STATUS: u32 = 0x2;
const PK_TIME: u32 = 0x4;
const PK_CHANGED: u32 = 0x8;
const PK_SERIAL_NUMBER: u32 = 0x10;
const PK_CURSOR: u32 = 0x20;
const PK_BUTTONS: u32 = 0x40;
const PK_X: u32 = 0x80;
const PK_Y: u32 = 0x100;
const PK_Z: u32 = 0x200;
const PK_NORMAL_PRESSURE: u32 = 0x400;
const PK_TANGENT_PRESSURE: u32 = 0x800;
const PK_ORIENTATION: u32 = 0x1000;
const PK_ROTATION: u32 = 0x2000;
const PK_SUPPORTED: u32 = PK_CONTEXT
    | PK_STATUS
    | PK_TIME
    | PK_CHANGED
    | PK_SERIAL_NUMBER
    | PK_CURSOR
    | PK_BUTTONS
    | PK_X
    | PK_Y
    | PK_NORMAL_PRESSURE
    | PK_ORIENTATION;

const TPS_QUEUE_ERR: u32 = 0x2;
const TPS_INVERT: u32 = 0x10;

const WT_DEFBASE: u32 = 0x7FF0;
const WT_PACKET: u32 = 0;
const WT_CTXOPEN: u32 = 1;
const WT_CTXCLOSE: u32 = 2;
const WT_PROXIMITY: u32 = 5;
const WT_CSRCHANGE: u32 = 7;

/// Unités de tablette par pixel d'écran.
const UNITS_PER_PIXEL: i32 = 10;
const PRESSURE_MAX: u32 = 1023;
const DEVICE_NAME: &str = "Winlux Pen";

/// Champs de LOGCONTEXT après le nom, dans l'ordre de wintab.h
/// (lcOptions … lcSysSensY), tous sur 4 octets.
const FIELDS: usize = 33;
const F_OPTIONS: usize = 0;
const F_MSGBASE: usize = 3;
const F_PKTDATA: usize = 6;
const F_PKTMODE: usize = 7;
const F_INORGX: usize = 11;
const F_INEXTX: usize = 14;
const F_OUTORGX: usize = 17;
const F_OUTEXTX: usize = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Packet {
    serial: u32,
    time: u32,
    cursor: u32,
    status: u32,
    buttons: u32,
    /// Coordonnées de tablette (origine en bas à gauche).
    x: i32,
    y: i32,
    pressure: u32,
    azimuth: i32,
    altitude: i32,
}

struct Context {
    handle: usize,
    hwnd: usize,
    name: Vec<u16>,
    fields: [u32; FIELDS],
    enabled: bool,
    queue: VecDeque<Packet>,
    queue_size: usize,
    /// Dernier paquet produit (modes relatifs, changement de curseur).
    previous: Option<Packet>,
}

struct State {
    /// Du plus haut au plus bas (WTOverlap).
    contexts: Vec<Context>,
    next_handle: usize,
    serial: u32,
    last_time: Option<std::time::Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    contexts: Vec::new(),
    next_handle: 0x5754_0001,
    serial: 0,
    last_time: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn ptr_size() -> usize {
    crate::wow32::pointer_size()
}

fn tablet_extent() -> (i32, i32) {
    let (w, h) = gui::screen_size();
    (w.max(1) * UNITS_PER_PIXEL, h.max(1) * UNITS_PER_PIXEL)
}

/// Contexte par défaut : numérique (WTI_DEFCONTEXT) ou système
/// (WTI_DEFSYSCTX, sortie en pixels d'écran).
fn default_fields(system: bool) -> [u32; FIELDS] {
    let (w, h) = gui::screen_size();
    let (tw, th) = tablet_extent();
    let (ow, oh) = if system { (w, h) } else { (tw, th) };
    let one = 0x1_0000u32; // FIX32 1.0
    [
        if system { CXO_SYSTEM } else { 0 },
        0,
        0,
        WT_DEFBASE,
        0,
        100,
        PK_SUPPORTED,
        0,
        PK_SUPPORTED,
        u32::MAX,
        u32::MAX,
        0,
        0,
        0,
        tw as u32,
        th as u32,
        0,
        0,
        0,
        0,
        ow as u32,
        oh as u32,
        0,
        one,
        one,
        one,
        system as u32,
        0,
        0,
        w as u32,
        h as u32,
        one,
        one,
    ]
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

// ── Écriture des réponses ───────────────────────────────────────────────

/// Copie `bytes` si `out` n'est pas nul ; renvoie la taille, comme WTInfo.
unsafe fn give(out: *mut u8, bytes: &[u8]) -> u32 {
    if !out.is_null() {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    }
    bytes.len() as u32
}

fn text_bytes(s: &str, wide_chars: bool) -> Vec<u8> {
    if wide_chars {
        s.encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect()
    } else {
        s.bytes().chain([0]).collect()
    }
}

fn axis(min: i32, max: i32, units: u32, resolution: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(16);
    v.extend(min.to_le_bytes());
    v.extend(max.to_le_bytes());
    v.extend(units.to_le_bytes());
    v.extend(resolution.to_le_bytes());
    v
}

/// LOGCONTEXTA (172 octets) ou LOGCONTEXTW (212 octets).
fn logcontext_bytes(name: &[u16], fields: &[u32; FIELDS], wide_chars: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(212);
    for i in 0..40 {
        let c = name.get(i).copied().filter(|_| i < 39).unwrap_or(0);
        if wide_chars {
            v.extend(c.to_le_bytes());
        } else {
            v.push(if c < 0x80 { c as u8 } else { b'?' });
        }
    }
    for f in fields {
        v.extend(f.to_le_bytes());
    }
    v
}

unsafe fn read_logcontext(ptr: *const u8, wide_chars: bool) -> (Vec<u16>, [u32; FIELDS]) {
    let name_len = if wide_chars { 80 } else { 40 };
    let mut name = Vec::new();
    for i in 0..40 {
        let c = if wide_chars {
            std::ptr::read_unaligned(ptr.add(i * 2) as *const u16)
        } else {
            *ptr.add(i) as u16
        };
        if c == 0 {
            break;
        }
        name.push(c);
    }
    let mut fields = [0u32; FIELDS];
    for (i, f) in fields.iter_mut().enumerate() {
        *f = std::ptr::read_unaligned(ptr.add(name_len + i * 4) as *const u32);
    }
    (name, fields)
}

fn cursor_name(index: u32) -> &'static str {
    match index {
        0 => "Winlux Cursor",
        1 => "Winlux Pen",
        _ => "Winlux Eraser",
    }
}

fn info_bytes(category: u32, index: u32, wide_chars: bool) -> Option<Vec<u8>> {
    let u = |v: u32| v.to_le_bytes().to_vec();
    let (tw, th) = tablet_extent();
    // Lignes par centimètre : dix unités par pixel à 96 points par pouce.
    let lines_per_cm = ((96.0 / 2.54) * UNITS_PER_PIXEL as f64) as u32;
    match category {
        WTI_INTERFACE => match index {
            1 => Some(text_bytes("Winlux WinTab", wide_chars)),
            2 => Some(0x0104u16.to_le_bytes().to_vec()),
            3 => Some(0x0100u16.to_le_bytes().to_vec()),
            4 => Some(u(1)),
            5 => Some(u(3)),
            6 => Some(u(16)),
            7 => Some(u(CXO_SYSTEM | CXO_PEN | CXO_MESSAGES | CXO_CSRMESSAGES)),
            8..=10 => Some(u(0)),
            _ => None,
        },
        WTI_STATUS => match index {
            1 => Some(u(state().contexts.len() as u32)),
            _ => None,
        },
        WTI_DEFCONTEXT | WTI_DEFSYSCTX | WTI_DDCTXS | WTI_DSCTXS => {
            let system = matches!(category, WTI_DEFSYSCTX | WTI_DSCTXS);
            let fields = default_fields(system);
            match index {
                0 => Some(logcontext_bytes(&wide(DEVICE_NAME), &fields, wide_chars)),
                1 => Some(text_bytes(DEVICE_NAME, wide_chars)),
                2..=34 => Some(u(fields[index as usize - 2])),
                _ => None,
            }
        }
        WTI_DEVICES => match index {
            1 => Some(text_bytes(DEVICE_NAME, wide_chars)),
            2 => Some(u(0)),
            3 => Some(u(3)),
            4 => Some(u(0)),
            5 => Some(u(100)),
            6 => Some(u(PK_SUPPORTED)),
            7 | 8 => Some(u(0)),
            9..=11 => Some(u(0)),
            12 => Some(axis(0, tw - 1, 2, lines_per_cm << 16)),
            13 => Some(axis(0, th - 1, 2, lines_per_cm << 16)),
            15 => Some(axis(0, PRESSURE_MAX as i32, 0, 0)),
            17 => {
                let mut v = axis(0, 3599, 3, 3600 << 16);
                v.extend(axis(-900, 900, 3, 3600 << 16));
                v.extend(axis(0, 0, 0, 0));
                Some(v)
            }
            19 => Some(text_bytes("WINLUX0001", wide_chars)),
            _ => None,
        },
        c if (WTI_CURSORS..WTI_CURSORS + 3).contains(&c) => {
            let n = c - WTI_CURSORS;
            match index {
                1 => Some(text_bytes(cursor_name(n), wide_chars)),
                2 => Some(u(1)),
                3 => Some(u(PK_SUPPORTED)),
                4 | 5 => Some(vec![3]),
                6 => {
                    let mut v = Vec::new();
                    for name in ["Tip", "Barrel 1", "Barrel 2"] {
                        v.extend(text_bytes(name, wide_chars));
                    }
                    v.extend(if wide_chars { vec![0, 0] } else { vec![0] });
                    Some(v)
                }
                7 => Some((0u8..32).collect()),
                // SBN_LCLICK, SBN_RCLICK, SBN_MCLICK.
                8 => {
                    let mut v = vec![0u8; 32];
                    v[..3].copy_from_slice(&[1, 4, 7]);
                    Some(v)
                }
                9 => Some(vec![0]),
                10 => Some(
                    [0u32, PRESSURE_MAX]
                        .iter()
                        .flat_map(|x| x.to_le_bytes())
                        .collect(),
                ),
                12 => Some(vec![0xFF]),
                15 => Some(u(0)),
                16 => Some(u(n)),
                17..=19 => Some(u(0)),
                20 => Some(u(0)),
                _ => None,
            }
        }
        _ => None,
    }
}

unsafe fn wt_info(category: u32, index: u32, out: *mut u8, wide_chars: bool) -> u32 {
    // Catégorie 0 : le plus grand résultat possible (taille seulement).
    if category == 0 {
        return 1040;
    }
    match info_bytes(category, index, wide_chars) {
        Some(bytes) => give(out, &bytes),
        None => 0,
    }
}

unsafe extern "win64" fn wt_info_a(category: u32, index: u32, out: *mut u8) -> u32 {
    wt_info(category, index, out, false)
}

unsafe extern "win64" fn wt_info_w(category: u32, index: u32, out: *mut u8) -> u32 {
    wt_info(category, index, out, true)
}

// ── Contextes ───────────────────────────────────────────────────────────

fn post(hwnd: usize, message: u32, wparam: usize, lparam: isize) {
    if hwnd != 0 {
        gui::post_to_window(hwnd, Msg::new(hwnd, message, wparam, lparam));
    }
}

fn open_context(hwnd: usize, name: Vec<u16>, fields: [u32; FIELDS], enable: bool) -> usize {
    let mut fields = fields;
    // Champs non pris en charge (Z, pressions tangentielle, rotation) :
    // retirés, comme le fait un pilote qui n'a pas ces axes.
    fields[F_PKTDATA] &= PK_SUPPORTED;
    let mut s = state();
    let handle = s.next_handle;
    s.next_handle += 1;
    let base = fields[F_MSGBASE];
    let options = fields[F_OPTIONS];
    s.contexts.insert(
        0,
        Context {
            handle,
            hwnd,
            name,
            fields,
            enabled: enable,
            queue: VecDeque::new(),
            queue_size: 8,
            previous: None,
        },
    );
    drop(s);
    if options & CXO_MESSAGES != 0 {
        post(hwnd, base + WT_CTXOPEN, handle, 0);
    }
    handle
}

unsafe fn wt_open(hwnd: usize, ctx: *const u8, enable: i32, wide_chars: bool) -> usize {
    if ctx.is_null() {
        return 0;
    }
    let (name, fields) = read_logcontext(ctx, wide_chars);
    open_context(hwnd, name, fields, enable != 0)
}

unsafe extern "win64" fn wt_open_a(hwnd: usize, ctx: *const u8, enable: i32) -> usize {
    wt_open(hwnd, ctx, enable, false)
}

unsafe extern "win64" fn wt_open_w(hwnd: usize, ctx: *const u8, enable: i32) -> usize {
    wt_open(hwnd, ctx, enable, true)
}

fn with_context<R>(handle: usize, f: impl FnOnce(&mut Context) -> R) -> Option<R> {
    let mut s = state();
    s.contexts.iter_mut().find(|c| c.handle == handle).map(f)
}

extern "win64" fn wt_close(handle: usize) -> i32 {
    let mut s = state();
    let Some(i) = s.contexts.iter().position(|c| c.handle == handle) else {
        return 0;
    };
    let ctx = s.contexts.remove(i);
    drop(s);
    if ctx.fields[F_OPTIONS] & CXO_MESSAGES != 0 {
        post(ctx.hwnd, ctx.fields[F_MSGBASE] + WT_CTXCLOSE, handle, 0);
    }
    1
}

extern "win64" fn wt_enable(handle: usize, enable: i32) -> i32 {
    with_context(handle, |c| c.enabled = enable != 0).is_some() as i32
}

extern "win64" fn wt_overlap(handle: usize, to_top: i32) -> i32 {
    let mut s = state();
    let Some(i) = s.contexts.iter().position(|c| c.handle == handle) else {
        return 0;
    };
    let ctx = s.contexts.remove(i);
    if to_top != 0 {
        s.contexts.insert(0, ctx);
    } else {
        s.contexts.push(ctx);
    }
    1
}

unsafe fn wt_get(handle: usize, out: *mut u8, wide_chars: bool) -> i32 {
    if out.is_null() {
        return 0;
    }
    match with_context(handle, |c| logcontext_bytes(&c.name, &c.fields, wide_chars)) {
        Some(bytes) => {
            give(out, &bytes);
            1
        }
        None => 0,
    }
}

unsafe extern "win64" fn wt_get_a(handle: usize, out: *mut u8) -> i32 {
    wt_get(handle, out, false)
}

unsafe extern "win64" fn wt_get_w(handle: usize, out: *mut u8) -> i32 {
    wt_get(handle, out, true)
}

unsafe fn wt_set(handle: usize, ctx: *const u8, wide_chars: bool) -> i32 {
    if ctx.is_null() {
        return 0;
    }
    let (name, mut fields) = read_logcontext(ctx, wide_chars);
    fields[F_PKTDATA] &= PK_SUPPORTED;
    with_context(handle, |c| {
        c.name = name;
        c.fields = fields;
    })
    .is_some() as i32
}

unsafe extern "win64" fn wt_set_a(handle: usize, ctx: *const u8) -> i32 {
    wt_set(handle, ctx, false)
}

unsafe extern "win64" fn wt_set_w(handle: usize, ctx: *const u8) -> i32 {
    wt_set(handle, ctx, true)
}

extern "win64" fn wt_queue_size_get(handle: usize) -> i32 {
    with_context(handle, |c| c.queue_size as i32).unwrap_or(0)
}

extern "win64" fn wt_queue_size_set(handle: usize, size: i32) -> i32 {
    if !(1..=1024).contains(&size) {
        return 0;
    }
    with_context(handle, |c| {
        c.queue_size = size as usize;
        c.queue.clear();
    })
    .is_some() as i32
}

/// Boîte de réglage du pilote : Winlux n'en a pas (Paramètres > Stylet).
extern "win64" fn wt_config(_handle: usize, _hwnd: usize) -> i32 {
    0
}

extern "win64" fn wt_ext(_handle: usize, _ext: u32, _data: usize) -> i32 {
    0
}

extern "win64" fn wt_save(_handle: usize, _data: usize) -> i32 {
    0
}

extern "win64" fn wt_restore(_hwnd: usize, _data: usize, _enable: i32) -> usize {
    0
}

// ── Paquets ─────────────────────────────────────────────────────────────

/// Taille d'un paquet pour `pkt_data` (PACKET de pktdef.h, alignement C).
fn packet_size(pkt_data: u32, ps: usize) -> usize {
    let mut size = 0;
    if pkt_data & PK_CONTEXT != 0 {
        size += ps;
    }
    for bit in [
        PK_STATUS,
        PK_TIME,
        PK_CHANGED,
        PK_SERIAL_NUMBER,
        PK_CURSOR,
        PK_BUTTONS,
        PK_X,
        PK_Y,
        PK_Z,
        PK_NORMAL_PRESSURE,
        PK_TANGENT_PRESSURE,
    ] {
        if pkt_data & bit != 0 {
            size += 4;
        }
    }
    if pkt_data & PK_ORIENTATION != 0 {
        size += 12;
    }
    if pkt_data & PK_ROTATION != 0 {
        size += 12;
    }
    let align = if pkt_data & PK_CONTEXT != 0 { ps } else { 4 };
    (size + align - 1) & !(align - 1)
}

/// Coordonnée de tablette → coordonnée du contexte (lcIn* → lcOut*). Une
/// étendue de sortie négative retourne l'axe (origine en haut pour Y) :
/// la dernière unité de la tablette donne alors 0.
fn scale_axis(value: i32, in_org: i32, in_ext: i32, out_org: i32, out_ext: i32) -> i32 {
    let in_ext = (in_ext as i64).abs().max(1);
    let rel = (value as i64 - in_org as i64).clamp(0, in_ext);
    let span = (out_ext as i64).abs();
    let scaled = rel * span / in_ext;
    if out_ext < 0 {
        (out_org as i64 + (span - 1 - scaled).max(0)) as i32
    } else {
        (out_org as i64 + scaled) as i32
    }
}

fn format_packet(ctx: &Context, p: &Packet, previous: Option<&Packet>, ps: usize) -> Vec<u8> {
    let f = &ctx.fields;
    let data = f[F_PKTDATA];
    let mode = f[F_PKTMODE];
    let mut v = Vec::with_capacity(packet_size(data, ps));
    let x = scale_axis(
        p.x,
        f[F_INORGX] as i32,
        f[F_INEXTX] as i32,
        f[F_OUTORGX] as i32,
        f[F_OUTEXTX] as i32,
    );
    let y = scale_axis(
        p.y,
        f[F_INORGX + 1] as i32,
        f[F_INEXTX + 1] as i32,
        f[F_OUTORGX + 1] as i32,
        f[F_OUTEXTX + 1] as i32,
    );
    let prev_xy = previous.map(|q| {
        (
            scale_axis(
                q.x,
                f[F_INORGX] as i32,
                f[F_INEXTX] as i32,
                f[F_OUTORGX] as i32,
                f[F_OUTEXTX] as i32,
            ),
            scale_axis(
                q.y,
                f[F_INORGX + 1] as i32,
                f[F_INEXTX + 1] as i32,
                f[F_OUTORGX + 1] as i32,
                f[F_OUTEXTX + 1] as i32,
            ),
        )
    });
    if data & PK_CONTEXT != 0 {
        if ps == 4 {
            v.extend((ctx.handle as u32).to_le_bytes());
        } else {
            v.extend((ctx.handle as u64).to_le_bytes());
        }
    }
    let mut push = |bit: u32, value: u32| {
        if data & bit != 0 {
            v.extend(value.to_le_bytes());
        }
    };
    push(PK_STATUS, p.status);
    push(PK_TIME, p.time);
    push(PK_CHANGED, data);
    push(PK_SERIAL_NUMBER, p.serial);
    push(PK_CURSOR, p.cursor);
    let buttons = if mode & PK_BUTTONS != 0 {
        // Mode relatif : numéro du bouton qui change, TBN_DOWN (2) ou TBN_UP (1).
        let before = previous.map_or(0, |q| q.buttons);
        let changed = before ^ p.buttons;
        if changed == 0 {
            0
        } else {
            let n = changed.trailing_zeros();
            let kind = if p.buttons & (1 << n) != 0 { 2 } else { 1 };
            n | (kind << 16)
        }
    } else {
        p.buttons
    };
    push(PK_BUTTONS, buttons);
    let (px, py) = prev_xy.unwrap_or((x, y));
    push(PK_X, if mode & PK_X != 0 { x - px } else { x } as u32);
    push(PK_Y, if mode & PK_Y != 0 { y - py } else { y } as u32);
    push(PK_NORMAL_PRESSURE, p.pressure);
    if data & PK_ORIENTATION != 0 {
        v.extend(p.azimuth.to_le_bytes());
        v.extend(p.altitude.to_le_bytes());
        v.extend(0i32.to_le_bytes());
    }
    v.resize(packet_size(data, ps), 0);
    v
}

/// Azimut (sens horaire depuis le haut) et altitude en dixièmes de degré,
/// depuis l'inclinaison X/Y ; la gomme donne une altitude négative.
fn orientation(pen: &Pen) -> (i32, i32) {
    let tx = pen.tilt_x.to_radians().tan() as f64;
    let ty = pen.tilt_y.to_radians().tan() as f64;
    let lean = (tx * tx + ty * ty).sqrt();
    let altitude = 90.0 - lean.atan().to_degrees();
    let mut azimuth = if lean == 0.0 {
        0.0
    } else {
        tx.atan2(-ty).to_degrees()
    };
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    let altitude = (altitude * 10.0).round() as i32;
    (
        (azimuth * 10.0).round() as i32 % 3600,
        if pen.eraser { -altitude } else { altitude },
    )
}

/// Échantillon du stylet en (x, y) écran : paquet pour le contexte activé
/// le plus haut.
pub fn pen_packet(x: i32, y: i32, pen: &Pen) {
    let (_, h) = gui::screen_size();
    let (azimuth, altitude) = orientation(pen);
    let mut s = state();
    if s.contexts.is_empty() {
        return;
    }
    s.serial = s.serial.wrapping_add(1);
    let now = std::time::Instant::now();
    let entering = s
        .last_time
        .is_none_or(|t| now.duration_since(t).as_millis() > 500);
    s.last_time = Some(now);
    let serial = s.serial;
    let p = Packet {
        serial,
        time: crate::dlls::kernel32::get_tick_count(),
        cursor: if pen.eraser { 2 } else { 1 },
        status: if pen.eraser { TPS_INVERT } else { 0 },
        buttons: (pen.contact as u32) | ((pen.barrel as u32) << 1),
        x: x * UNITS_PER_PIXEL + UNITS_PER_PIXEL / 2,
        y: (h - 1 - y) * UNITS_PER_PIXEL + UNITS_PER_PIXEL / 2,
        pressure: if pen.contact {
            (pen.pressure * PRESSURE_MAX as f32).round() as u32
        } else {
            0
        },
        azimuth,
        altitude,
    };
    let Some(ctx) = s.contexts.iter_mut().find(|c| c.enabled) else {
        return;
    };
    let cursor_changed = ctx.previous.is_none_or(|q| q.cursor != p.cursor);
    let mut p = p;
    if ctx.queue.len() >= ctx.queue_size {
        ctx.queue.pop_front();
        p.status |= TPS_QUEUE_ERR;
    }
    ctx.queue.push_back(p);
    ctx.previous_set(p);
    let (hwnd, base, options, handle) = (
        ctx.hwnd,
        ctx.fields[F_MSGBASE],
        ctx.fields[F_OPTIONS],
        ctx.handle,
    );
    drop(s);
    if options & CXO_MESSAGES != 0 {
        if entering {
            // Le stylet entre dans le contexte et en proximité.
            post(hwnd, base + WT_PROXIMITY, handle, 0x0001_0001);
        }
        if cursor_changed && options & CXO_CSRMESSAGES != 0 {
            post(hwnd, base + WT_CSRCHANGE, serial as usize, handle as isize);
        }
        post(hwnd, base + WT_PACKET, serial as usize, handle as isize);
    }
}

impl Context {
    fn previous_set(&mut self, p: Packet) {
        self.previous = Some(p);
    }

    /// Paquets formatés, chacun avec celui qui le précède (modes relatifs).
    fn formatted(&self, packets: &[Packet], ps: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut before: Option<Packet> = None;
        for p in packets {
            out.extend(format_packet(self, p, before.as_ref(), ps));
            before = Some(*p);
        }
        out
    }
}

/// Copie des paquets dans le tampon de l'application.
unsafe fn copy_packets(ctx: &Context, packets: &[Packet], out: *mut u8) {
    if !out.is_null() && !packets.is_empty() {
        let bytes = ctx.formatted(packets, ptr_size());
        give(out, &bytes);
    }
}

unsafe extern "win64" fn wt_packets_get(handle: usize, max: i32, out: *mut u8) -> i32 {
    with_context(handle, |c| {
        let n = (max.max(0) as usize).min(c.queue.len());
        let taken: Vec<Packet> = c.queue.drain(..n).collect();
        copy_packets(c, &taken, out);
        n as i32
    })
    .unwrap_or(0)
}

unsafe extern "win64" fn wt_packets_peek(handle: usize, max: i32, out: *mut u8) -> i32 {
    with_context(handle, |c| {
        let n = (max.max(0) as usize).min(c.queue.len());
        let seen: Vec<Packet> = c.queue.iter().take(n).copied().collect();
        copy_packets(c, &seen, out);
        n as i32
    })
    .unwrap_or(0)
}

/// WTPacket : le paquet de ce numéro ; lui et les plus anciens quittent la file.
unsafe extern "win64" fn wt_packet(handle: usize, serial: u32, out: *mut u8) -> i32 {
    with_context(handle, |c| {
        let Some(i) = c.queue.iter().position(|p| p.serial == serial) else {
            return 0;
        };
        let taken: Vec<Packet> = c.queue.drain(..=i).collect();
        if let Some(last) = taken.last() {
            copy_packets(c, std::slice::from_ref(last), out);
        }
        1
    })
    .unwrap_or(0)
}

unsafe fn data_range(
    handle: usize,
    begin: u32,
    end: u32,
    max: i32,
    out: *mut u8,
    count: *mut i32,
    remove: bool,
) -> i32 {
    with_context(handle, |c| {
        let (Some(first), Some(last)) = (
            c.queue.iter().position(|p| p.serial == begin),
            c.queue.iter().position(|p| p.serial == end),
        ) else {
            return 0;
        };
        if last < first {
            return 0;
        }
        let range: Vec<Packet> = c.queue.range(first..=last).copied().collect();
        let n = (max.max(0) as usize).min(range.len());
        copy_packets(c, &range[..n], out);
        if !count.is_null() {
            *count = n as i32;
        }
        if remove {
            c.queue.drain(..first + n);
        }
        n as i32
    })
    .unwrap_or(0)
}

unsafe extern "win64" fn wt_data_get(
    handle: usize,
    begin: u32,
    end: u32,
    max: i32,
    out: *mut u8,
    count: *mut i32,
) -> i32 {
    data_range(handle, begin, end, max, out, count, true)
}

unsafe extern "win64" fn wt_data_peek(
    handle: usize,
    begin: u32,
    end: u32,
    max: i32,
    out: *mut u8,
    count: *mut i32,
) -> i32 {
    data_range(handle, begin, end, max, out, count, false)
}

unsafe extern "win64" fn wt_queue_packets_ex(handle: usize, old: *mut u32, new: *mut u32) -> i32 {
    with_context(handle, |c| {
        let (Some(first), Some(last)) = (c.queue.front(), c.queue.back()) else {
            return 0;
        };
        if !old.is_null() {
            *old = first.serial;
        }
        if !new.is_null() {
            *new = last.serial;
        }
        1
    })
    .unwrap_or(0)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WTInfoA" => fn wt_info_a,
        "WTInfoW" => fn wt_info_w,
        "WTOpenA" => fn wt_open_a,
        "WTOpenW" => fn wt_open_w,
        "WTClose" => fn wt_close,
        "WTEnable" => fn wt_enable,
        "WTOverlap" => fn wt_overlap,
        "WTGetA" => fn wt_get_a,
        "WTGetW" => fn wt_get_w,
        "WTSetA" => fn wt_set_a,
        "WTSetW" => fn wt_set_w,
        "WTPacketsGet" => fn wt_packets_get,
        "WTPacketsPeek" => fn wt_packets_peek,
        "WTPacket" => fn wt_packet,
        "WTDataGet" => fn wt_data_get,
        "WTDataPeek" => fn wt_data_peek,
        "WTQueuePacketsEx" => fn wt_queue_packets_ex,
        "WTQueueSizeGet" => fn wt_queue_size_get,
        "WTQueueSizeSet" => fn wt_queue_size_set,
        "WTConfig" => fn wt_config,
        "WTExtGet" => fn wt_ext,
        "WTExtSet" => fn wt_ext,
        "WTSave" => fn wt_save,
        "WTRestore" => fn wt_restore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tablette_wintab_paquets_avec_pression() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        state().contexts.clear();
        // Interface et contexte par défaut.
        let mut ctx = [0u8; 212];
        unsafe {
            assert_eq!(wt_info_w(WTI_DEFCONTEXT, 0, std::ptr::null_mut()), 212);
            assert_eq!(wt_info_a(WTI_DEFCONTEXT, 0, std::ptr::null_mut()), 172);
            assert_eq!(wt_info_w(WTI_DEFSYSCTX, 0, ctx.as_mut_ptr()), 212);
            let mut pressure = [0u8; 16];
            assert_eq!(wt_info_w(WTI_DEVICES, 15, pressure.as_mut_ptr()), 16);
            assert_eq!(i32::from_le_bytes(pressure[4..8].try_into().unwrap()), 1023);
        }
        // Contexte système, Y vers le bas (étendue négative, comme le font
        // les programmes), paquet : contexte, bouton, X, Y, pression,
        // orientation.
        let (w, h) = gui::screen_size();
        let field = |i: usize| 80 + i * 4;
        let data = PK_CONTEXT | PK_BUTTONS | PK_X | PK_Y | PK_NORMAL_PRESSURE | PK_ORIENTATION;
        ctx[field(F_PKTDATA)..field(F_PKTDATA) + 4].copy_from_slice(&data.to_le_bytes());
        ctx[field(F_OUTEXTX + 1)..field(F_OUTEXTX + 1) + 4].copy_from_slice(&(-h).to_le_bytes());
        let handle = unsafe { wt_open_w(0, ctx.as_ptr(), 1) };
        assert_ne!(handle, 0);
        let pen = Pen {
            pressure: 0.5,
            tilt_x: 0.0,
            tilt_y: 0.0,
            eraser: false,
            barrel: false,
            contact: true,
        };
        pen_packet(w / 2, 10, &pen);
        pen_packet(w / 2 + 1, 11, &pen);
        let size = packet_size(data, 8);
        assert_eq!(size, 8 + 4 * 4 + 12 + 4); // HCTX + 4 champs + orientation, aligné sur 8
        let mut buf = vec![0u8; size * 4];
        let n = unsafe { wt_packets_get(handle, 4, buf.as_mut_ptr()) };
        assert_eq!(n, 2);
        let u = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
        assert_eq!(u(0) as usize, handle);
        assert_eq!(u(8), 1); // pointe posée
        assert_eq!(u(12), w / 2);
        assert_eq!(u(16), 10);
        assert_eq!(u(20), 512);
        assert_eq!(u(28), 900); // stylet droit : altitude 90°
        assert_eq!(u(size + 12), w / 2 + 1);
        assert_eq!(u(size + 16), 11);
        // File vidée ; azimut d'un stylet penché vers la droite : 90°.
        assert_eq!(unsafe { wt_packets_get(handle, 4, buf.as_mut_ptr()) }, 0);
        let leaning = Pen {
            tilt_x: 45.0,
            ..pen
        };
        assert_eq!(orientation(&leaning), (900, 450));
        assert_eq!(wt_close(handle), 1);
        assert_eq!(wt_close(handle), 0);
    }
}
