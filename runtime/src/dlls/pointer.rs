//! Stylet : API pointeur de Windows 8 et suivants (#1199).
//!
//! Le serveur X donne la pression et l'inclinaison d'une tablette graphique
//! dans les valuateurs XInput 2 (« Abs Pressure », « Abs Tilt X/Y ») ;
//! `gui_x11` les relève par les événements bruts et les dépose ici. Chaque
//! message souris venu du stylet est alors précédé du WM_POINTER*
//! équivalent, que GetPointerInfo / GetPointerPenInfo décrivent.
//!
//! Comme sous Windows, le message souris qui suit est « promu » : il porte
//! la signature 0xFF515700 (GetMessageExtraInfo) et n'est livré que si la
//! fenêtre a laissé le WM_POINTER* à DefWindowProc. Un programme qui gère
//! WM_POINTER (Photoshop, Krita) ne reçoit donc pas le trait deux fois.
//!
//! Structures POINTER_INFO / POINTER_PEN_INFO / POINTER_DEVICE_INFO écrites
//! octet par octet d'après winuser.h (documentation publique), en 32 et en
//! 64 bits (`wow32::pointer_size`).

use crate::dlls::gui::{self, Msg, PendingEvent};
use crate::process::set_last_error;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const WM_POINTERUPDATE: u32 = 0x0245;
pub const WM_POINTERDOWN: u32 = 0x0246;
pub const WM_POINTERUP: u32 = 0x0247;

/// MI_WP_SIGNATURE : message souris produit par un stylet.
pub const PEN_SIGNATURE: u32 = 0xFF51_5700;
/// Marque interne d'un message souris promu depuis la souris elle-même
/// (EnableMouseInPointer) : GetMessageExtraInfo y rend 0.
const MOUSE_PROMOTED: u32 = 0x5750_4D00;

pub const MOUSE_ID: u32 = 1;
pub const PEN_ID: u32 = 2;
const PT_PEN: u32 = 3;
const PT_MOUSE: u32 = 4;
/// HANDLE du stylet (sourceDevice, GetPointerDevices).
const PEN_DEVICE: usize = 0x5045_4E01;
const MOUSE_DEVICE: usize = 0x4D4F_5501;

// POINTER_FLAGS
const PF_NEW: u32 = 0x1;
const PF_INRANGE: u32 = 0x2;
const PF_INCONTACT: u32 = 0x4;
const PF_FIRSTBUTTON: u32 = 0x10;
const PF_SECONDBUTTON: u32 = 0x20;
const PF_THIRDBUTTON: u32 = 0x40;
const PF_PRIMARY: u32 = 0x2000;
const PF_CONFIDENCE: u32 = 0x4000;
const PF_DOWN: u32 = 0x1_0000;
const PF_UPDATE: u32 = 0x2_0000;
const PF_UP: u32 = 0x4_0000;

/// Durée pendant laquelle un événement brut du stylet attribue au stylet
/// les mouvements du pointeur système qui suivent.
const PEN_RECENT_MS: u128 = 250;

/// État courant du stylet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pen {
    /// Pression de 0 à 1.
    pub pressure: f32,
    /// Inclinaison en degrés, de -90 à 90 (X vers la droite, Y vers soi).
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub eraser: bool,
    /// Bouton latéral (barrel) tenu.
    pub barrel: bool,
    pub contact: bool,
}

#[derive(Clone, Copy, Default)]
struct Last {
    hwnd: usize,
    x: i32,
    y: i32,
    flags: u32,
    change: u32,
    frame: u32,
    time: u32,
    pen: Pen,
}

struct Shared {
    pen: Pen,
    /// Dernier événement brut venu d'un stylet, et si le dernier événement
    /// brut (tous périphériques) venait bien de lui.
    last_pen: Option<Instant>,
    from_pen: bool,
    /// Un stylet a été vu depuis le démarrage (GetPointerDevices).
    seen: bool,
    pen_last: Option<Last>,
    mouse_last: Option<Last>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    pen: Pen {
        pressure: 0.0,
        tilt_x: 0.0,
        tilt_y: 0.0,
        eraser: false,
        barrel: false,
        contact: false,
    },
    last_pen: None,
    from_pen: false,
    seen: false,
    pen_last: None,
    mouse_last: None,
});
static MOUSE_IN_POINTER: AtomicBool = AtomicBool::new(false);
static FRAME: AtomicU32 = AtomicU32::new(0);

fn shared() -> std::sync::MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Relevés du serveur X ────────────────────────────────────────────────

/// Valuateurs d'un événement brut du stylet ; un axe absent garde sa
/// dernière valeur (XInput n'envoie que les axes qui changent).
pub fn note_pen(pressure: Option<f32>, tilt_x: Option<f32>, tilt_y: Option<f32>, eraser: bool) {
    let mut s = shared();
    if let Some(p) = pressure {
        s.pen.pressure = p.clamp(0.0, 1.0);
    }
    if let Some(t) = tilt_x {
        s.pen.tilt_x = t.clamp(-90.0, 90.0);
    }
    if let Some(t) = tilt_y {
        s.pen.tilt_y = t.clamp(-90.0, 90.0);
    }
    s.pen.eraser = eraser;
    s.last_pen = Some(Instant::now());
    s.from_pen = true;
    s.seen = true;
}

/// Bouton brut du stylet : 1 = pointe posée, 2 et 3 = boutons latéraux.
pub fn note_pen_button(button: u32, down: bool) {
    let mut s = shared();
    match button {
        1 => s.pen.contact = down,
        2 | 3 => s.pen.barrel = down,
        _ => {}
    }
    s.last_pen = Some(Instant::now());
    s.from_pen = true;
    s.seen = true;
}

/// Le dernier événement brut vient d'une souris ou d'un pavé tactile.
pub fn note_other_device() {
    shared().from_pen = false;
}

/// Le pointeur système bouge en ce moment sous l'effet du stylet.
pub fn pen_in_use() -> bool {
    let s = shared();
    s.from_pen
        && s.last_pen
            .is_some_and(|t| t.elapsed().as_millis() <= PEN_RECENT_MS)
}

/// Axes XInput d'un stylet : numéro de valuateur, minimum, maximum.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PenAxes {
    pub pressure: Option<(u16, f64, f64)>,
    pub tilt_x: Option<(u16, f64, f64)>,
    pub tilt_y: Option<(u16, f64, f64)>,
    /// Outil gomme (le pilote wacom en fait un périphérique à part).
    pub eraser: bool,
}

/// Valeurs d'un événement brut : `mask` dit quels valuateurs sont présents,
/// `values` les donne dans l'ordre. Pression ramenée à 0..1, inclinaison en
/// degrés (les pilotes libinput et wacom la donnent déjà en degrés).
pub fn decode_valuators(
    axes: &PenAxes,
    mask: &[u32],
    values: &[f64],
) -> (Option<f32>, Option<f32>, Option<f32>) {
    let value_of = |axis: Option<(u16, f64, f64)>| -> Option<(f64, f64, f64)> {
        let (number, min, max) = axis?;
        let word = *mask.get(number as usize / 32)?;
        if word & (1 << (number % 32)) == 0 {
            return None;
        }
        // Rang de la valeur : valuateurs présents avant celui-ci.
        let mut rank = 0;
        for n in 0..number as usize {
            if mask.get(n / 32).is_some_and(|w| w & (1 << (n % 32)) != 0) {
                rank += 1;
            }
        }
        values.get(rank).map(|v| (*v, min, max))
    };
    let pressure = value_of(axes.pressure).map(|(v, min, max)| {
        if max > min {
            ((v - min) / (max - min)) as f32
        } else {
            0.0
        }
    });
    let tilt = |axis| {
        value_of(axis).map(|(v, min, max)| {
            if min >= -90.0 && max <= 90.0 {
                v as f32
            } else if max > min {
                (((v - min) / (max - min)) * 180.0 - 90.0) as f32
            } else {
                0.0
            }
        })
    };
    (pressure, tilt(axes.tilt_x), tilt(axes.tilt_y))
}

// ── Messages ────────────────────────────────────────────────────────────

fn tick() -> u32 {
    crate::dlls::kernel32::get_tick_count()
}

fn button_flags(wparam: usize) -> u32 {
    let mut f = 0;
    if wparam & 0x1 != 0 {
        f |= PF_FIRSTBUTTON;
    }
    if wparam & 0x2 != 0 {
        f |= PF_SECONDBUTTON;
    }
    if wparam & 0x10 != 0 {
        f |= PF_THIRDBUTTON;
    }
    f
}

/// Message WM_POINTER* qui précède le message souris `e` (coordonnées
/// client), pour le stylet ou, après EnableMouseInPointer(TRUE), la souris.
/// `screen` : origine de la zone cliente de la fenêtre.
pub fn pointer_for_mouse(e: &PendingEvent, screen: (i32, i32)) -> Option<PendingEvent> {
    let pen = pen_in_use();
    if !pen && !MOUSE_IN_POINTER.load(Ordering::Relaxed) {
        return None;
    }
    let (message, change) = match e.message {
        0x0200 => (WM_POINTERUPDATE, 0),
        0x0201 | 0x0203 => (WM_POINTERDOWN, 1),
        0x0202 => (WM_POINTERUP, 2),
        0x0204 | 0x0206 => (WM_POINTERDOWN, 3),
        0x0205 => (WM_POINTERUP, 4),
        0x0207 | 0x0209 => (WM_POINTERDOWN, 5),
        0x0208 => (WM_POINTERUP, 6),
        _ => return None,
    };
    let x = screen.0 + (e.lparam as u16 as i16) as i32;
    let y = screen.1 + ((e.lparam >> 16) as u16 as i16) as i32;
    let mut s = shared();
    let id = if pen { PEN_ID } else { MOUSE_ID };
    let previous = if pen { s.pen_last } else { s.mouse_last };
    let mut flags = PF_INRANGE | PF_PRIMARY | PF_CONFIDENCE | button_flags(e.wparam);
    if pen {
        // Pointe posée : premier bouton ; bouton latéral : deuxième.
        if s.pen.contact || e.wparam & 0x1 != 0 {
            flags |= PF_INCONTACT | PF_FIRSTBUTTON;
        }
        if s.pen.barrel {
            flags |= PF_SECONDBUTTON;
        }
    } else if e.wparam & 0x13 != 0 {
        flags |= PF_INCONTACT;
    }
    if message == WM_POINTERUP {
        flags &= !PF_INCONTACT;
        if change == 2 {
            flags &= !PF_FIRSTBUTTON;
        }
    }
    if previous.is_none() {
        flags |= PF_NEW;
    }
    flags |= match message {
        WM_POINTERDOWN => PF_DOWN,
        WM_POINTERUP => PF_UP,
        _ => PF_UPDATE,
    };
    let frame = FRAME.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let last = Last {
        hwnd: e.hwnd,
        x,
        y,
        flags,
        change,
        frame,
        time: tick(),
        pen: s.pen,
    };
    if pen {
        s.pen_last = Some(last);
    } else {
        s.mouse_last = Some(last);
    }
    drop(s);
    if pen {
        crate::dlls::wintab32::pen_packet(x, y, &last.pen);
    }
    // wParam : identifiant du pointeur, puis les drapeaux de message
    // (POINTER_MESSAGE_FLAG_* = les 16 bits bas de POINTER_FLAGS).
    Some(PendingEvent {
        hwnd: e.hwnd,
        message,
        wparam: id as usize | (((flags & 0xFFFF) as usize) << 16),
        lparam: ((y as u16 as isize) << 16) | (x as u16 as isize),
    })
}

/// Marque `l_private` du message souris qui suit un WM_POINTER*.
pub fn promoted_mark(pointer: &PendingEvent) -> u32 {
    if pointer.wparam & 0xFFFF == PEN_ID as usize {
        PEN_SIGNATURE
    } else {
        MOUSE_PROMOTED
    }
}

thread_local! {
    /// Promotion en attente sur ce thread : un WM_POINTER* a été pris dans
    /// la file ; `Some(true)` dès que DefWindowProc l'a reçu.
    static AWAITING: Cell<Option<bool>> = const { Cell::new(None) };
    static EXTRA_INFO: Cell<isize> = const { Cell::new(0) };
}

/// Appelé pour chaque message retiré de la file ; `false` : le message
/// souris promu est retenu, car la fenêtre a traité le WM_POINTER* elle-même.
pub fn on_take(m: &Msg) -> bool {
    if (WM_POINTERUPDATE..=WM_POINTERUP).contains(&m.message) {
        AWAITING.with(|a| a.set(Some(false)));
        EXTRA_INFO.with(|x| x.set(0));
        return true;
    }
    let promoted = m.l_private == PEN_SIGNATURE || m.l_private == MOUSE_PROMOTED;
    if promoted && AWAITING.with(|a| a.take()) == Some(false) {
        return false;
    }
    let extra = if m.l_private == PEN_SIGNATURE {
        PEN_SIGNATURE as i32 as isize
    } else {
        0
    };
    EXTRA_INFO.with(|x| x.set(extra));
    true
}

/// DefWindowProc a reçu un WM_POINTER* : le message souris promu suivra.
pub fn on_default(message: u32) {
    if (WM_POINTERUPDATE..=WM_POINTERUP).contains(&message) {
        AWAITING.with(|a| {
            if a.get() == Some(false) {
                a.set(Some(true));
            }
        });
    }
}

extern "win64" fn get_message_extra_info() -> isize {
    EXTRA_INFO.with(|x| x.get())
}

extern "win64" fn set_message_extra_info(value: isize) -> isize {
    EXTRA_INFO.with(|x| x.replace(value))
}

// ── Structures ──────────────────────────────────────────────────────────

fn ptr_size() -> usize {
    crate::wow32::pointer_size()
}

/// Taille de POINTER_INFO : 96 octets en 64 bits, 88 en 32 bits.
pub fn pointer_info_size(ps: usize) -> usize {
    (16 + 2 * ps + 64 + 7) & !7
}

unsafe fn put32(base: *mut u8, at: usize, v: u32) {
    std::ptr::write_unaligned(base.add(at) as *mut u32, v);
}

unsafe fn put_ptr(base: *mut u8, at: usize, v: usize, ps: usize) {
    if ps == 4 {
        put32(base, at, v as u32);
    } else {
        std::ptr::write_unaligned(base.add(at) as *mut u64, v as u64);
    }
}

/// HIMETRIC (centièmes de millimètre) à 96 points par pouce.
fn himetric(px: i32) -> i32 {
    (px as i64 * 2540 / 96) as i32
}

unsafe fn write_info(out: *mut u8, ps: usize, id: u32, last: &Last) {
    std::ptr::write_bytes(out, 0, pointer_info_size(ps));
    put32(out, 0, if id == PEN_ID { PT_PEN } else { PT_MOUSE });
    put32(out, 4, id);
    put32(out, 8, last.frame);
    put32(out, 12, last.flags);
    let device = if id == PEN_ID {
        PEN_DEVICE
    } else {
        MOUSE_DEVICE
    };
    put_ptr(out, 16, device, ps);
    put_ptr(out, 16 + ps, last.hwnd, ps);
    let base = 16 + 2 * ps;
    for (i, (x, y)) in [
        (last.x, last.y),
        (himetric(last.x), himetric(last.y)),
        (last.x, last.y),
        (himetric(last.x), himetric(last.y)),
    ]
    .into_iter()
    .enumerate()
    {
        put32(out, base + i * 8, x as u32);
        put32(out, base + i * 8 + 4, y as u32);
    }
    put32(out, base + 32, last.time);
    put32(out, base + 36, 1); // historyCount
    put32(out, base + 48, last.time.wrapping_mul(10_000)); // PerformanceCount
    put32(out, base + 56, last.change);
}

/// POINTER_PEN_INFO : POINTER_INFO puis penFlags, penMask, pressure
/// (0..1024), rotation, tiltX, tiltY.
unsafe fn write_pen_info(out: *mut u8, ps: usize, last: &Last) {
    write_info(out, ps, PEN_ID, last);
    let at = pointer_info_size(ps);
    let p = &last.pen;
    let mut flags = 0;
    if p.barrel {
        flags |= 0x1; // PEN_FLAG_BARREL
    }
    if p.eraser {
        flags |= 0x2 | 0x4; // PEN_FLAG_INVERTED | PEN_FLAG_ERASER
    }
    put32(out, at, flags);
    put32(out, at + 4, 0x1 | 0x4 | 0x8); // PEN_MASK_PRESSURE | TILT_X | TILT_Y
    put32(out, at + 8, (p.pressure * 1024.0).round() as u32);
    put32(out, at + 12, 0);
    put32(out, at + 16, p.tilt_x.round() as i32 as u32);
    put32(out, at + 20, p.tilt_y.round() as i32 as u32);
}

fn last_for(id: u32) -> Option<Last> {
    let s = shared();
    match id {
        PEN_ID => s.pen_last,
        MOUSE_ID => s.mouse_last,
        _ => None,
    }
}

const ERROR_INVALID_PARAMETER: u32 = 87;

/// Tout identifiant autre que celui du stylet est la souris (PT_MOUSE),
/// comme le faisait la version d'Electron que celle-ci remplace.
unsafe extern "win64" fn get_pointer_type(id: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *out = if id == PEN_ID && last_for(id).is_some() {
        PT_PEN
    } else {
        PT_MOUSE
    };
    1
}

unsafe extern "win64" fn get_pointer_cursor_id(id: u32, out: *mut u32) -> i32 {
    let Some(last) = last_for(id) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    // Curseurs de wintab32 : 1 pointe, 2 gomme ; 0 la souris.
    *out = match id {
        PEN_ID if last.pen.eraser => 2,
        PEN_ID => 1,
        _ => 0,
    };
    1
}

unsafe extern "win64" fn get_pointer_info(id: u32, out: *mut u8) -> i32 {
    match last_for(id) {
        Some(last) if !out.is_null() => {
            write_info(out, ptr_size(), id, &last);
            1
        }
        _ => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

unsafe extern "win64" fn get_pointer_pen_info(id: u32, out: *mut u8) -> i32 {
    match last_for(id) {
        Some(last) if id == PEN_ID && !out.is_null() => {
            write_pen_info(out, ptr_size(), &last);
            1
        }
        _ => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

/// Historique et trames : Winlux livre un message par échantillon, donc
/// une seule entrée, celle du message courant.
unsafe fn one_entry(id: u32, count: *mut u32, out: *mut u8, pen: bool) -> i32 {
    let Some(last) = last_for(id) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if count.is_null() || (pen && id != PEN_ID) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let room = *count;
    *count = 1;
    if out.is_null() || room == 0 {
        return 1;
    }
    if pen {
        write_pen_info(out, ptr_size(), &last);
    } else {
        write_info(out, ptr_size(), id, &last);
    }
    1
}

unsafe extern "win64" fn get_pointer_info_history(id: u32, count: *mut u32, out: *mut u8) -> i32 {
    one_entry(id, count, out, false)
}

unsafe extern "win64" fn get_pointer_pen_info_history(
    id: u32,
    count: *mut u32,
    out: *mut u8,
) -> i32 {
    one_entry(id, count, out, true)
}

unsafe extern "win64" fn get_pointer_frame_info(id: u32, count: *mut u32, out: *mut u8) -> i32 {
    one_entry(id, count, out, false)
}

unsafe extern "win64" fn get_pointer_frame_pen_info(id: u32, count: *mut u32, out: *mut u8) -> i32 {
    one_entry(id, count, out, true)
}

unsafe extern "win64" fn get_pointer_frame_info_history(
    id: u32,
    entries: *mut u32,
    count: *mut u32,
    out: *mut u8,
) -> i32 {
    if !entries.is_null() {
        *entries = 1;
    }
    one_entry(id, count, out, false)
}

unsafe extern "win64" fn get_pointer_frame_pen_info_history(
    id: u32,
    entries: *mut u32,
    count: *mut u32,
    out: *mut u8,
) -> i32 {
    if !entries.is_null() {
        *entries = 1;
    }
    one_entry(id, count, out, true)
}

/// Le message souris est déjà traduit en WM_POINTER* ; rien à ignorer.
extern "win64" fn skip_pointer_frame_messages(_id: u32) -> i32 {
    1
}

extern "win64" fn enable_mouse_in_pointer(enable: i32) -> i32 {
    MOUSE_IN_POINTER.store(enable != 0, Ordering::Relaxed);
    1
}

extern "win64" fn is_mouse_in_pointer_enabled() -> i32 {
    MOUSE_IN_POINTER.load(Ordering::Relaxed) as i32
}

/// POINTER_DEVICE_INFO : displayOrientation, device, pointerDeviceType,
/// monitor, startingCursorId, maxActiveContacts, productString[520].
pub fn device_info_size(ps: usize) -> usize {
    let at = (4 + ps - 1) & !(ps - 1);
    let monitor = (at + ps + 4 + ps - 1) & !(ps - 1);
    (monitor + ps + 4 + 2 + 1040 + ps - 1) & !(ps - 1)
}

unsafe fn write_device(out: *mut u8, ps: usize) {
    std::ptr::write_bytes(out, 0, device_info_size(ps));
    let device_at = (4 + ps - 1) & !(ps - 1);
    put_ptr(out, device_at, PEN_DEVICE, ps);
    put32(out, device_at + ps, 2); // POINTER_DEVICE_TYPE_EXTERNAL_PEN
    let monitor = (device_at + ps + 4 + ps - 1) & !(ps - 1);
    put_ptr(out, monitor, 0x0001_0001, ps); // HMONITOR principal
    put32(out, monitor + ps, 1); // startingCursorId
    std::ptr::write_unaligned(out.add(monitor + ps + 4) as *mut u16, 1);
    let name: Vec<u16> = "Winlux Pen".encode_utf16().collect();
    let text = out.add(monitor + ps + 6) as *mut u16;
    for (i, c) in name.iter().enumerate() {
        std::ptr::write_unaligned(text.add(i), *c);
    }
}

unsafe extern "win64" fn get_pointer_devices(count: *mut u32, out: *mut u8) -> i32 {
    if count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = shared().seen as u32;
    let room = *count;
    *count = n;
    if n == 1 && !out.is_null() && room >= 1 {
        write_device(out, ptr_size());
    }
    1
}

unsafe extern "win64" fn get_pointer_device(device: usize, out: *mut u8) -> i32 {
    if device != PEN_DEVICE || out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    write_device(out, ptr_size());
    1
}

/// Rectangles du périphérique : la tablette couvre l'écran (pilote en
/// mode absolu), en pixels.
unsafe extern "win64" fn get_pointer_device_rects(
    device: usize,
    device_rect: *mut i32,
    display_rect: *mut i32,
) -> i32 {
    if device != PEN_DEVICE {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (w, h) = gui::screen_size();
    for r in [device_rect, display_rect] {
        if !r.is_null() {
            *r = 0;
            *r.add(1) = 0;
            *r.add(2) = w;
            *r.add(3) = h;
        }
    }
    1
}

unsafe extern "win64" fn get_pointer_device_properties(
    _device: usize,
    count: *mut u32,
    _out: *mut u8,
) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    1
}

extern "win64" fn register_pointer_device_notifications(_hwnd: usize, _range: i32) -> i32 {
    1
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetMessageExtraInfo" => fn get_message_extra_info,
        "SetMessageExtraInfo" => fn set_message_extra_info,
        "GetPointerType" => fn get_pointer_type,
        "GetPointerCursorId" => fn get_pointer_cursor_id,
        "GetPointerInfo" => fn get_pointer_info,
        "GetPointerPenInfo" => fn get_pointer_pen_info,
        "GetPointerInfoHistory" => fn get_pointer_info_history,
        "GetPointerPenInfoHistory" => fn get_pointer_pen_info_history,
        "GetPointerFrameInfo" => fn get_pointer_frame_info,
        "GetPointerFramePenInfo" => fn get_pointer_frame_pen_info,
        "GetPointerFrameInfoHistory" => fn get_pointer_frame_info_history,
        "GetPointerFramePenInfoHistory" => fn get_pointer_frame_pen_info_history,
        "SkipPointerFrameMessages" => fn skip_pointer_frame_messages,
        "EnableMouseInPointer" => fn enable_mouse_in_pointer,
        "IsMouseInPointerEnabled" => fn is_mouse_in_pointer_enabled,
        "GetPointerDevices" => fn get_pointer_devices,
        "GetPointerDevice" => fn get_pointer_device,
        "GetPointerDeviceRects" => fn get_pointer_device_rects,
        "GetPointerDeviceProperties" => fn get_pointer_device_properties,
        "RegisterPointerDeviceNotifications" => fn register_pointer_device_notifications,
    }
}

#[cfg(test)]
pub fn reset_for_tests() {
    let mut s = shared();
    s.pen = Pen::default();
    s.last_pen = None;
    s.from_pen = false;
    s.pen_last = None;
    s.mouse_last = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylet_en_messages_pointeur_avec_pression() {
        let _guard = crate::dlls::wintab32::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        assert_eq!(pointer_info_size(8), 96);
        assert_eq!(pointer_info_size(4), 88);
        assert_eq!(device_info_size(8), 1080);
        // Souris seule : aucun WM_POINTER.
        note_other_device();
        let down = PendingEvent {
            hwnd: 0x1234,
            message: 0x0201,
            wparam: 0x1,
            lparam: (20 << 16) | 10,
        };
        assert!(pointer_for_mouse(&down, (100, 200)).is_none());

        // Stylet posé à mi-pression, incliné vers la droite.
        note_pen(Some(0.5), Some(30.0), Some(-10.0), false);
        note_pen_button(1, true);
        let p = pointer_for_mouse(&down, (100, 200)).expect("WM_POINTERDOWN");
        assert_eq!(p.message, WM_POINTERDOWN);
        assert_eq!(p.wparam & 0xFFFF, PEN_ID as usize);
        let flags = (p.wparam >> 16) as u32;
        assert_ne!(flags & PF_INCONTACT, 0);
        assert_ne!(flags & PF_FIRSTBUTTON, 0);
        assert_eq!(p.lparam, (220 << 16) | 110);
        assert_eq!(promoted_mark(&p), PEN_SIGNATURE);

        let mut info = [0u8; 120];
        unsafe {
            let mut kind = 0u32;
            assert_eq!(get_pointer_type(PEN_ID, &mut kind), 1);
            assert_eq!(kind, PT_PEN);
            assert_eq!(get_pointer_pen_info(PEN_ID, info.as_mut_ptr()), 1);
        }
        let u = |at: usize| u32::from_le_bytes(info[at..at + 4].try_into().unwrap());
        assert_eq!(u(0), PT_PEN);
        assert_eq!(u(4), PEN_ID);
        assert_eq!(u(24) as usize, 0x1234); // hwndTarget (64 bits : bas)
        assert_eq!((u(32), u(36)), (110, 220)); // ptPixelLocation
        assert_eq!(u(88), 1); // POINTER_CHANGE_FIRSTBUTTON_DOWN
        assert_eq!(u(104), 512); // pression sur 1024
        assert_eq!(u(112) as i32, 30);
        assert_eq!(u(116) as i32, -10);

        // Message souris promu : retenu si la fenêtre a traité le
        // WM_POINTER*, livré si elle l'a laissé à DefWindowProc.
        let pointer_msg = Msg::new(0x1234, WM_POINTERDOWN, p.wparam, p.lparam);
        let mut mouse_msg = Msg::new(0x1234, 0x0201, 1, down.lparam);
        mouse_msg.l_private = PEN_SIGNATURE;
        assert!(on_take(&pointer_msg));
        assert!(!on_take(&mouse_msg));
        assert!(on_take(&pointer_msg));
        on_default(WM_POINTERDOWN);
        assert!(on_take(&mouse_msg));
        assert_eq!(get_message_extra_info() as u32, PEN_SIGNATURE);
        // Un message ordinaire remet l'information à zéro.
        assert!(on_take(&Msg::new(0x1234, 0x0100, 0, 0)));
        assert_eq!(get_message_extra_info(), 0);
        reset_for_tests();
    }

    #[test]
    fn valuateurs_du_stylet_depuis_xinput() {
        // Valuateurs 0 X, 1 Y, 2 pression 0..65535, 3 et 4 inclinaison en degrés.
        let axes = PenAxes {
            pressure: Some((2, 0.0, 65535.0)),
            tilt_x: Some((3, -64.0, 63.0)),
            tilt_y: Some((4, -64.0, 63.0)),
            eraser: false,
        };
        // Événement avec X, pression et inclinaison Y seulement.
        let (p, tx, ty) = decode_valuators(&axes, &[0b10101], &[100.0, 32767.5, -20.0]);
        assert!((p.unwrap() - 0.5).abs() < 1e-3);
        assert_eq!(tx, None);
        assert_eq!(ty, Some(-20.0));
        // Pression seule absente : les autres restent lisibles.
        let (p, tx, _) = decode_valuators(&axes, &[0b01000], &[12.0]);
        assert_eq!((p, tx), (None, Some(12.0)));
    }
}
