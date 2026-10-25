//! Bureau Winlux inspiré de Windows 11, dessiné entièrement par GDI invité :
//! écran de démarrage animé, fond « bloom », barre des tâches centrée ou
//! alignée à gauche, menu Démarrer animé, zone de notification, icônes du
//! bureau et fenêtres des applications intégrées (voir `wm.rs`).
//!
//! Toute la mise en page découle de la taille réelle de l'écran : quand la
//! résolution change (fenêtre de VM redimensionnée), le bureau se réadapte.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

use crate::explorer;
use crate::gfx::*;
use crate::wm::{self, Gfx, Mouse};
use winlux_testexe::*;

pub const TASKBAR_HEIGHT: i32 = 48;
const BUTTON: i32 = 44;
const BUTTON_GAP: i32 = 4;
const TASKBAR_BUTTONS: i32 = 3;
const START_OPEN_MS: i32 = 230;
const START_CLOSE_MS: i32 = 140;
const SPLASH_MIN_MS: u32 = 2600;
const INTRO_MS: i32 = 520;

pub static mut SCREEN_W: i32 = 1280;
pub static mut SCREEN_H: i32 = 720;
pub static mut START_OPEN: bool = false;
pub static mut ALIGN_LEFT: bool = false;
pub static mut VOLUME_LEVEL: u32 = 0;
pub static mut BATTERY_LEVEL: u32 = 0;
pub static mut BATTERY_PRESENT: u32 = 0;
pub static mut NETWORK_CONNECTED: u32 = 0;
static mut START_ANIM: u32 = 0;
static mut START_CLOSING: bool = false;
static mut HOVER_BUTTON: i32 = -1;
static mut HOVER_START_ITEM: i32 = -1;
static mut ICON_X: [i32; 3] = [12, 12, 12];
static mut ICON_Y: [i32; 3] = [12, 108, 204];
static mut DRAGGED_ICON: i32 = -1;
static mut SELECTED_ICON: i32 = -1;
static mut ICON_CLICK_TIME: u32 = 0;
static mut LAST_MINUTE: u16 = 99;
/// Démarrage : début de l'écran de chargement, étape de préchargement en
/// cours et fin du fondu d'arrivée sur le bureau.
static mut BOOT_START: u32 = 0;
static mut BOOT_STEP: u32 = 0;
static mut BOOT_DONE: bool = false;
static mut INTRO_START: u32 = 0;
static mut LAST_FRAME: u32 = 0;
/// Notification façon Windows 11, au-dessus de la zone de notification.
static mut TOAST: [u8; 120] = [0; 120];
static mut TOAST_LEN: usize = 0;
static mut TOAST_START: u32 = 0;
const TOAST_MS: i32 = 4500;
/// Panneau de notifications (centre d'action simplifié).
static mut NOTIF_PANEL: bool = false;
static mut NOTIF_HISTORY: [[u8; 120]; 8] = [[0; 120]; 8];
static mut NOTIF_LENS: [usize; 8] = [0; 8];
static mut NOTIF_COUNT: usize = 0;
/// Atténuation du fond (0 = noir, 1000 = normal) pendant le fondu.
static mut FADE: i32 = 1000;

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}

// ---------- Géométrie partagée par le dessin et les clics ----------

struct Layout {
    w: i32,
    h: i32,
    bar_top: i32,
    buttons_x: i32,
    start: Rect,
}

fn layout() -> Layout {
    let (w, h) = unsafe { (SCREEN_W.max(640), SCREEN_H.max(400)) };
    let bar_top = h - TASKBAR_HEIGHT;
    let buttons_w = TASKBAR_BUTTONS * BUTTON + (TASKBAR_BUTTONS - 1) * BUTTON_GAP;
    let buttons_x = if unsafe { ALIGN_LEFT } {
        12
    } else {
        (w - buttons_w) / 2
    };
    let sw = 640.min(w - 24);
    let sh = 600.min(bar_top - 24);
    let sx = if unsafe { ALIGN_LEFT } {
        12
    } else {
        (w - sw) / 2
    };
    let st = bar_top - 12 - sh;
    Layout {
        w,
        h,
        bar_top,
        buttons_x,
        start: Rect {
            left: sx,
            top: st,
            right: sx + sw,
            bottom: st + sh,
        },
    }
}

fn button_rect(l: &Layout, index: i32) -> Rect {
    let x = l.buttons_x + index * (BUTTON + BUTTON_GAP);
    Rect {
        left: x,
        top: l.bar_top + 2,
        right: x + BUTTON,
        bottom: l.bar_top + 2 + BUTTON,
    }
}

// Contenu du menu Démarrer : zones cliquables calculées depuis le panneau.
fn align_toggle_rect(s: &Rect) -> Rect {
    Rect {
        left: s.right - 230,
        top: s.bottom - 48,
        right: s.right - 70,
        bottom: s.bottom - 16,
    }
}
fn power_rect(s: &Rect) -> Rect {
    Rect {
        left: s.right - 62,
        top: s.bottom - 50,
        right: s.right - 22,
        bottom: s.bottom - 14,
    }
}
const PIN_COLS: i32 = 6;
const PIN_CELL_H: i32 = 84;
fn pin_rows(s: &Rect) -> i32 {
    if s.bottom - s.top > 560 {
        2
    } else {
        1
    }
}
fn pinned_cell(s: &Rect, i: i32) -> Rect {
    let (l, r) = (s.left + 28, s.right - 28);
    let cw = (r - l) / PIN_COLS;
    let x = l + (i % PIN_COLS) * cw;
    let y = s.top + 118 + (i / PIN_COLS) * PIN_CELL_H;
    Rect {
        left: x + 4,
        top: y,
        right: x + cw - 4,
        bottom: y + PIN_CELL_H - 6,
    }
}
fn recommended_cell(s: &Rect, i: i32) -> Rect {
    let (l, r) = (s.left + 28, s.right - 28);
    let col_w = (r - l) / 2;
    let y = s.top + 118 + pin_rows(s) * PIN_CELL_H + 54 + (i / 2) * 56;
    let x = l + (i % 2) * col_w;
    Rect {
        left: x,
        top: y,
        right: x + col_w - 8,
        bottom: y + 50,
    }
}

/// Position du menu Démarrer : il glisse depuis la barre des tâches.
unsafe fn start_offset() -> i32 {
    let t = since(START_ANIM);
    if START_CLOSING {
        if t >= START_CLOSE_MS {
            START_OPEN = false;
            START_CLOSING = false;
            return 0;
        }
        ease_out(t, START_CLOSE_MS) * 60 / 1000
    } else if t < START_OPEN_MS {
        (1000 - ease_out(t, START_OPEN_MS)) * 70 / 1000
    } else {
        0
    }
}

unsafe fn set_start(open: bool) {
    if open == (START_OPEN && !START_CLOSING) {
        return;
    }
    START_ANIM = now();
    if open {
        START_OPEN = true;
        START_CLOSING = false;
    } else if START_OPEN {
        START_CLOSING = true;
    }
}

// ---------- Fond d'écran ----------

fn fc(c: u32) -> u32 {
    unsafe {
        if FADE >= 1000 {
            c
        } else {
            mix(0, c, FADE, 1000)
        }
    }
}

static mut WP_DC: Handle = core::ptr::null_mut();
static mut WP_W: i32 = 0;
static mut WP_H: i32 = 0;
static mut WP_TRIED: bool = false;

unsafe fn load_wallpaper(hdc: Handle) {
    if WP_TRIED {
        return;
    }
    WP_TRIED = true;
    let path = c"Z:\\usr\\share\\winlux\\wallpaper.bmp";
    let f = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return;
    }
    let fsize = GetFileSize(f, core::ptr::null_mut());
    if fsize < 54 + 12 {
        CloseHandle(f);
        return;
    }
    let mut hdr = [0u8; 54];
    let mut read = 0u32;
    ReadFile(f, hdr.as_mut_ptr(), 54, &mut read, core::ptr::null_mut());
    if read != 54 || hdr[0] != b'B' || hdr[1] != b'M' {
        CloseHandle(f);
        return;
    }
    let off = u32::from_le_bytes([hdr[10], hdr[11], hdr[12], hdr[13]]);
    let w = i32::from_le_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]);
    let h = i32::from_le_bytes([hdr[22], hdr[23], hdr[24], hdr[25]]);
    let bpp = u16::from_le_bytes([hdr[28], hdr[29]]);
    if w <= 0 || h == 0 || (bpp != 24 && bpp != 32) {
        CloseHandle(f);
        return;
    }
    let abs_h = if h < 0 { -h } else { h };
    let stride = ((w as u32 * bpp as u32 / 8 + 3) & !3) as usize;
    let pixel_bytes = stride * abs_h as usize;
    let info = BitmapInfoHeader {
        size: 40,
        width: w,
        height: abs_h,
        planes: 1,
        bit_count: bpp,
        compression: 0,
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let mem_dc = CreateCompatibleDC(hdc);
    let dib = CreateDIBSection(mem_dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if dib.is_null() || bits.is_null() {
        DeleteDC(mem_dc);
        CloseHandle(f);
        return;
    }
    SelectObject(mem_dc, dib);
    SetFilePointer(f, off as i32, core::ptr::null_mut(), 0);
    let dst = bits as *mut u8;
    let mut total = 0usize;
    while total < pixel_bytes {
        let chunk = (pixel_bytes - total).min(65536) as u32;
        let mut got = 0u32;
        ReadFile(f, dst.add(total), chunk, &mut got, core::ptr::null_mut());
        if got == 0 {
            break;
        }
        total += got as usize;
    }
    CloseHandle(f);
    if total < pixel_bytes {
        DeleteObject(dib);
        DeleteDC(mem_dc);
        return;
    }
    WP_DC = mem_dc;
    WP_W = w;
    WP_H = abs_h;
}

unsafe fn wallpaper_bloom(hdc: Handle, b: &mut Brushes, l: &Layout) {
    let top = rgb(0x06, 0x12, 0x33);
    let bottom = rgb(0x0C, 0x2E, 0x6E);
    let bands = 48;
    for i in 0..bands {
        let y0 = l.h * i / bands;
        let y1 = l.h * (i + 1) / bands;
        fill(hdc, b, 0, y0, l.w, y1, fc(mix(top, bottom, i, bands - 1)));
    }
    let r = l.h.min(l.w * 3 / 4) * 9 / 25;
    let (cx, base) = (l.w / 2, l.h * 3 / 5 + r / 3);
    let petals: [(i32, i32, i32, u32, u32); 5] = [
        (
            -r * 6 / 5,
            r * 2 / 5,
            r * 6 / 5,
            rgb(0x0E, 0x34, 0x82),
            rgb(0x30, 0x6C, 0xD0),
        ),
        (
            r * 6 / 5,
            r * 2 / 5,
            r * 6 / 5,
            rgb(0x0E, 0x34, 0x82),
            rgb(0x30, 0x6C, 0xD0),
        ),
        (
            -r * 3 / 5,
            r * 9 / 20,
            r * 8 / 5,
            rgb(0x15, 0x48, 0xA8),
            rgb(0x4E, 0x96, 0xF0),
        ),
        (
            r * 3 / 5,
            r * 9 / 20,
            r * 8 / 5,
            rgb(0x15, 0x48, 0xA8),
            rgb(0x4E, 0x96, 0xF0),
        ),
        (
            0,
            r / 2,
            r * 2,
            rgb(0x1E, 0x60, 0xD0),
            rgb(0xB4, 0xD8, 0xFF),
        ),
    ];
    let steps = if wm::animating() || START_ANIM != 0 && since(START_ANIM) < START_OPEN_MS {
        14
    } else {
        28
    };
    for (dx, rx, ry, outer, inner) in petals {
        for k in 0..steps {
            let sx = rx * (steps - k) / steps;
            let sy = ry * (steps - k) / steps;
            let px = cx + dx;
            ellipse(
                hdc,
                b,
                px - sx,
                base - sy,
                px + sx,
                base,
                fc(mix(outer, inner, k, steps - 1)),
            );
        }
    }
}

unsafe fn wallpaper(hdc: Handle, b: &mut Brushes, l: &Layout) {
    load_wallpaper(hdc);
    if !WP_DC.is_null() && FADE >= 1000 {
        StretchBlt(hdc, 0, 0, l.w, l.h, WP_DC, 0, 0, WP_W, WP_H, 0x00CC_0020);
    } else {
        wallpaper_bloom(hdc, b, l);
    }
}

// ---------- Barre des tâches ----------

// Seules les applications reellement fournies sont epinglees.
const TASKBAR_GLYPHS: [Glyph; 3] = [Glyph::Start, Glyph::Folder, Glyph::Settings];

unsafe fn draw_wifi(hdc: Handle, b: &mut Brushes, x: i32, y: i32, on: bool) {
    let c = if on {
        rgb(0xF2, 0xF2, 0xF2)
    } else {
        rgb(0x7A, 0x80, 0x8A)
    };
    for k in 0..4 {
        let hgt = 4 + k * 3;
        fill(hdc, b, x + k * 4, y + 14 - hgt, x + k * 4 + 3, y + 14, c);
    }
}
unsafe fn draw_volume(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    fill(hdc, b, x, y + 5, x + 4, y + 10, c);
    for k in 0..5 {
        fill(hdc, b, x + 4 + k, y + 5 - k, x + 5 + k, y + 10 + k, c);
    }
    if level == 0 {
        for k in 0..6 {
            fill(hdc, b, x + 12 + k, y + 4 + k, x + 14 + k, y + 6 + k, c);
            fill(hdc, b, x + 17 - k, y + 4 + k, x + 19 - k, y + 6 + k, c);
        }
    } else {
        let bars = if level < 34 {
            1
        } else if level < 67 {
            2
        } else {
            3
        };
        for k in 0..bars {
            let hgt = 4 + k * 4;
            fill(
                hdc,
                b,
                x + 12 + k * 3,
                y + 7 - hgt / 2,
                x + 14 + k * 3,
                y + 8 + hgt / 2,
                c,
            );
        }
    }
}
unsafe fn draw_battery(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    round_rect(hdc, b, x, y + 2, x + 22, y + 13, 2, c);
    fill(hdc, b, x + 1, y + 3, x + 21, y + 12, rgb(0x20, 0x22, 0x28));
    fill(
        hdc,
        b,
        x + 2,
        y + 4,
        x + 2 + (18 * level.min(100) as i32) / 100,
        y + 11,
        if level > 20 { c } else { rgb(0xFF, 0x60, 0x50) },
    );
    fill(hdc, b, x + 22, y + 5, x + 24, y + 10, c);
}

unsafe fn two_digits(v: u16) -> [u8; 2] {
    [b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8]
}

unsafe fn taskbar(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout, dy: i32) {
    let top = l.bar_top + dy;
    fill(hdc, b, 0, top, l.w, l.h, rgb(0x1C, 0x1F, 0x26));
    fill(hdc, b, 0, top, l.w, top + 1, rgb(0x34, 0x38, 0x42));
    for (i, g) in TASKBAR_GLYPHS.iter().enumerate() {
        let mut r = button_rect(l, i as i32);
        r.top += dy;
        r.bottom += dy;
        let app = match i {
            1 => Some(wm::APP_EXPLORER),
            2 => Some(wm::APP_SETTINGS),
            _ => None,
        };
        let running = app.map_or(false, wm::is_open);
        let focused = app.map_or(false, wm::is_active) || (i == 0 && START_OPEN && !START_CLOSING);
        if focused {
            round_rect(
                hdc,
                b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                5,
                rgb(0x2E, 0x33, 0x3D),
            );
        } else if HOVER_BUTTON == i as i32 {
            round_rect(
                hdc,
                b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                5,
                rgb(0x29, 0x2D, 0x36),
            );
        }
        glyph(hdc, b, f, *g, r.left + 10, r.top + 10, 24);
        if running {
            // Indicateur d'application ouverte, comme sous Windows 11.
            let cx = (r.left + r.right) / 2;
            let half = if focused { 8 } else { 3 };
            round_rect(
                hdc,
                b,
                cx - half,
                r.bottom - 3,
                cx + half,
                r.bottom,
                1,
                if focused {
                    ACCENT
                } else {
                    rgb(0x9A, 0xA3, 0xAE)
                },
            );
        }
    }
    wm::TASKBAR_SLOT[wm::APP_EXPLORER] = button_rect(l, 1);
    wm::TASKBAR_SLOT[wm::APP_SETTINGS] = button_rect(l, 2);

    // Zone de notification : chevron, réseau, volume, batterie, horloge.
    let mut now = SystemTime::default();
    GetLocalTime(&mut now);
    LAST_MINUTE = now.minute;
    let [h0, h1] = two_digits(now.hour);
    let [m0, m1] = two_digits(now.minute);
    let [d0, d1] = two_digits(now.day);
    let [o0, o1] = two_digits(now.month);
    let year = now.year;
    let clock = [h0, h1, b':', m0, m1];
    let date = [
        d0,
        d1,
        b'/',
        o0,
        o1,
        b'/',
        b'0' + (year / 1000 % 10) as u8,
        b'0' + (year / 100 % 10) as u8,
        b'0' + (year / 10 % 10) as u8,
        b'0' + (year % 10) as u8,
    ];
    let right = l.w - 12;
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        right - text_width(&clock, 13),
        top + 8,
        &clock,
    );
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        right - text_width(&date, 13),
        top + 26,
        &date,
    );
    let tray = right - 92 - 96;
    round_rect(
        hdc,
        b,
        tray - 4,
        top + 6,
        tray + 88,
        top + 42,
        5,
        rgb(0x24, 0x28, 0x31),
    );
    draw_wifi(hdc, b, tray + 6, top + 17, NETWORK_CONNECTED != 0);
    draw_volume(hdc, b, tray + 30, top + 17, VOLUME_LEVEL);
    if BATTERY_PRESENT != 0 {
        draw_battery(hdc, b, tray + 58, top + 17, BATTERY_LEVEL);
    } else {
        text(
            hdc,
            f.small,
            rgb(0xC8, 0xCD, 0xD4),
            tray + 60,
            top + 17,
            b"CA",
        );
    }
    text(
        hdc,
        f.semibold,
        rgb(0xC8, 0xCD, 0xD4),
        tray - 24,
        top + 14,
        b"^",
    );
}

// ---------- Menu Démarrer ----------

#[derive(Clone, Copy, PartialEq)]
enum Launch {
    Explorer,
    Settings,
    Documents,
    Pc,
    Programs,
    Updates,
    Terminal,
    Browser,
}

struct App {
    name: &'static [u8],
    glyph: Glyph,
    launch: Launch,
}
const PINNED: [App; 6] = [
    App {
        name: b"Explorer",
        glyph: Glyph::Folder,
        launch: Launch::Explorer,
    },
    App {
        name: b"Param\xE8tres",
        glyph: Glyph::Settings,
        launch: Launch::Settings,
    },
    App {
        name: b"Mises \xE0 jour",
        glyph: Glyph::Store,
        launch: Launch::Updates,
    },
    App {
        name: b"Documents",
        glyph: Glyph::Folder,
        launch: Launch::Documents,
    },
    App {
        name: b"Ce PC",
        glyph: Glyph::Computer,
        launch: Launch::Pc,
    },
    App {
        name: b"Programmes .exe",
        glyph: Glyph::Exe,
        launch: Launch::Programs,
    },
];
const RECOMMENDED: [(&[u8], &[u8], Glyph, Launch); 4] = [
    (
        b"Terminal",
        b"Ouvrir un terminal Linux",
        Glyph::Computer,
        Launch::Terminal,
    ),
    (
        b"Navigateur",
        b"Ouvrir le navigateur web",
        Glyph::Store,
        Launch::Browser,
    ),
    (
        b"Documents",
        b"Dossier personnel Linux",
        Glyph::Folder,
        Launch::Documents,
    ),
    (
        b"Param\xE8tres Linux et Windows",
        b"Clavier, r\xE9seau et alimentation",
        Glyph::Settings,
        Launch::Settings,
    ),
];

/// Lance une application depuis la barre des tâches ou le menu Démarrer.
unsafe fn launch(what: Launch) {
    match what {
        Launch::Explorer => {
            if wm::is_open(wm::APP_EXPLORER) {
                wm::open(wm::APP_EXPLORER)
            } else {
                explorer::open_home()
            }
        }
        Launch::Pc | Launch::Programs => explorer::open_pc(),
        Launch::Documents => explorer::open_documents(),
        Launch::Settings => wm::open(wm::APP_SETTINGS),
        Launch::Updates => crate::settings::open_page(6),
        Launch::Terminal => {
            static TERM: &[u16] = &[
                b'x' as u16,
                b't' as u16,
                b'e' as u16,
                b'r' as u16,
                b'm' as u16,
                0,
            ];
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                TERM.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
        }
        Launch::Browser => {
            static BROWSER: &[u16] = &[
                b'w' as u16,
                b'i' as u16,
                b'n' as u16,
                b'l' as u16,
                b'u' as u16,
                b'x' as u16,
                b'-' as u16,
                b'b' as u16,
                b'r' as u16,
                b'o' as u16,
                b'w' as u16,
                b's' as u16,
                b'e' as u16,
                b'r' as u16,
                0,
            ];
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                BROWSER.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
        }
    }
}

unsafe fn start_menu(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    let dy = start_offset();
    if !START_OPEN {
        return;
    }
    let s0 = l.start;
    let s = &Rect {
        left: s0.left,
        top: s0.top + dy,
        right: s0.right,
        bottom: s0.bottom + dy,
    };
    let surface = rgb(0x24, 0x27, 0x2F);
    panel(
        hdc,
        b,
        s.left,
        s.top,
        s.right,
        s.bottom,
        8,
        surface,
        rgb(0x3A, 0x3F, 0x4A),
    );
    let pad = 28;
    let inner_l = s.left + pad;
    let inner_r = s.right - pad;

    text(
        hdc,
        f.large,
        rgb(0xF2, 0xF2, 0xF2),
        inner_l,
        s.top + 28,
        b"Winlux",
    );

    // Épinglé.
    let y = s.top + 82;
    text(
        hdc,
        f.title,
        rgb(0xF2, 0xF2, 0xF2),
        inner_l + 4,
        y,
        b"\xC9pingl\xE9",
    );
    let all = b"Applications int\xE9gr\xE9es";
    let all_w = text_width(all, 11) + 20;
    text(
        hdc,
        f.small,
        rgb(0xE6, 0xE6, 0xE6),
        inner_r - all_w + 10,
        y + 2,
        all,
    );
    for (i, app) in PINNED
        .iter()
        .take((PIN_COLS * pin_rows(s)) as usize)
        .enumerate()
    {
        let c = pinned_cell(s, i as i32);
        if HOVER_START_ITEM == i as i32 {
            round_rect(
                hdc,
                b,
                c.left,
                c.top,
                c.right,
                c.bottom,
                5,
                rgb(0x31, 0x35, 0x3F),
            );
        }
        let cx = (c.left + c.right) / 2;
        glyph(hdc, b, f, app.glyph, cx - 16, c.top + 10, 32);
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0xE6, 0xE6, 0xE6),
            cx,
            c.top + 52,
            app.name,
        );
    }

    // Recommandé.
    let ry = s.top + 118 + pin_rows(s) * PIN_CELL_H + 20;
    if ry + 150 < s.bottom - 72 {
        text(
            hdc,
            f.title,
            rgb(0xF2, 0xF2, 0xF2),
            inner_l + 4,
            ry,
            b"Recommand\xE9",
        );
        for (i, (title, sub, g, _)) in RECOMMENDED.iter().enumerate() {
            let c = recommended_cell(s, i as i32);
            if HOVER_START_ITEM == 100 + i as i32 {
                round_rect(
                    hdc,
                    b,
                    c.left,
                    c.top,
                    c.right,
                    c.bottom,
                    5,
                    rgb(0x31, 0x35, 0x3F),
                );
            }
            glyph(hdc, b, f, *g, c.left + 8, c.top + 10, 28);
            text(
                hdc,
                f.body,
                rgb(0xF2, 0xF2, 0xF2),
                c.left + 48,
                c.top + 8,
                title,
            );
            text(
                hdc,
                f.small,
                rgb(0x9A, 0xA3, 0xAE),
                c.left + 48,
                c.top + 26,
                sub,
            );
        }
    }

    // Pied : utilisateur, alignement de la barre, alimentation.
    fill(
        hdc,
        b,
        s.left + 1,
        s.bottom - 64,
        s.right - 1,
        s.bottom - 1,
        rgb(0x1F, 0x22, 0x29),
    );
    fill(
        hdc,
        b,
        s.left + 1,
        s.bottom - 64,
        s.right - 1,
        s.bottom - 63,
        rgb(0x34, 0x38, 0x42),
    );
    let ay = s.bottom - 48;
    ellipse(
        hdc,
        b,
        inner_l + 6,
        ay,
        inner_l + 38,
        ay + 32,
        rgb(0x5B, 0x6B, 0xD8),
    );
    text_centered(
        hdc,
        f.semibold,
        13,
        rgb(0xFF, 0xFF, 0xFF),
        inner_l + 22,
        ay + 8,
        b"D",
    );
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        inner_l + 50,
        ay + 8,
        b"Damien",
    );
    let t = align_toggle_rect(s);
    round_rect(
        hdc,
        b,
        t.left,
        t.top,
        t.right,
        t.bottom,
        4,
        if HOVER_START_ITEM == 200 {
            rgb(0x3A, 0x3F, 0x4A)
        } else {
            rgb(0x31, 0x35, 0x3F)
        },
    );
    let label: &[u8] = if ALIGN_LEFT {
        b"Barre : \xE0 gauche"
    } else {
        b"Barre : centr\xE9e"
    };
    text_centered(
        hdc,
        f.small,
        11,
        rgb(0xE6, 0xE6, 0xE6),
        (t.left + t.right) / 2,
        t.top + 9,
        label,
    );
    // Bouton d'alimentation.
    let (px, py) = (inner_r - 30, ay + 4);
    let pbg = if HOVER_START_ITEM == 201 {
        rgb(0x31, 0x35, 0x3F)
    } else {
        rgb(0x1F, 0x22, 0x29)
    };
    if HOVER_START_ITEM == 201 {
        round_rect(hdc, b, px - 8, py - 6, px + 32, py + 30, 4, pbg);
    }
    ellipse(hdc, b, px, py, px + 24, py + 24, rgb(0xE6, 0xE6, 0xE6));
    ellipse(hdc, b, px + 3, py + 3, px + 21, py + 21, pbg);
    fill(hdc, b, px + 8, py - 1, px + 16, py + 8, pbg);
    fill(
        hdc,
        b,
        px + 11,
        py - 1,
        px + 13,
        py + 12,
        rgb(0xE6, 0xE6, 0xE6),
    );
}

/// Élément du menu Démarrer sous le pointeur : 0..11 épinglés, 100..103
/// recommandés, 200 bascule d'alignement, 201 alimentation, -1 rien.
fn start_item_at(l: &Layout, x: i32, y: i32) -> i32 {
    let s = &l.start;
    for i in 0..PINNED.len() as i32 {
        if inside(&pinned_cell(s, i), x, y) {
            return i;
        }
    }
    let ry = s.top + 118 + pin_rows(s) * PIN_CELL_H + 20;
    if ry + 150 < s.bottom - 72 {
        for i in 0..4 {
            if inside(&recommended_cell(s, i), x, y) {
                return 100 + i;
            }
        }
    }
    if inside(&align_toggle_rect(s), x, y) {
        return 200;
    }
    if inside(&power_rect(s), x, y) {
        return 201;
    }
    -1
}

// ---------- Notifications ----------

pub unsafe fn toast(msg: &[u8]) {
    let n = msg.len().min(TOAST.len());
    TOAST[..n].copy_from_slice(&msg[..n]);
    TOAST_LEN = n;
    TOAST_START = now();
    // Historique pour le panneau de notifications.
    if NOTIF_COUNT < NOTIF_HISTORY.len() {
        NOTIF_HISTORY[NOTIF_COUNT][..n].copy_from_slice(&msg[..n]);
        NOTIF_LENS[NOTIF_COUNT] = n;
        NOTIF_COUNT += 1;
    } else {
        for i in 1..NOTIF_HISTORY.len() {
            NOTIF_HISTORY[i - 1] = NOTIF_HISTORY[i];
            NOTIF_LENS[i - 1] = NOTIF_LENS[i];
        }
        let last = NOTIF_HISTORY.len() - 1;
        NOTIF_HISTORY[last][..n].copy_from_slice(&msg[..n]);
        NOTIF_LENS[last] = n;
    }
}

unsafe fn draw_toast(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if TOAST_LEN == 0 {
        return;
    }
    let t = since(TOAST_START);
    if t > TOAST_MS {
        TOAST_LEN = 0;
        return;
    }
    // Entrée par la droite, sortie en glissant vers le bas.
    let slide = if t < 260 {
        (1000 - ease_out(t, 260)) * 380 / 1000
    } else {
        0
    };
    let drop = if t > TOAST_MS - 220 {
        ease_out(t - (TOAST_MS - 220), 220) * 140 / 1000
    } else {
        0
    };
    let w = 364;
    let x = l.w - w - 12 + slide;
    let y = l.bar_top - 112 + drop;
    panel(
        hdc,
        b,
        x,
        y,
        x + w,
        y + 100,
        8,
        rgb(0x2B, 0x2B, 0x2B),
        rgb(0x3D, 0x3D, 0x3D),
    );
    glyph(hdc, b, f, Glyph::Start, x + 16, y + 16, 16);
    text(
        hdc,
        f.caption,
        rgb(0xC5, 0xC5, 0xC5),
        x + 42,
        y + 16,
        b"Winlux",
    );
    let msg = &TOAST[..TOAST_LEN];
    // Retour à la ligne simple au dernier espace qui tient.
    let per_line = ((w - 40) * 100 / (13 * 56)) as usize;
    let cut = if msg.len() > per_line {
        msg[..per_line]
            .iter()
            .rposition(|&c| c == b' ')
            .unwrap_or(per_line)
    } else {
        msg.len()
    };
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        x + 16,
        y + 46,
        &msg[..cut],
    );
    if cut < msg.len() {
        text_fit(
            hdc,
            f.body,
            13,
            rgb(0xF2, 0xF2, 0xF2),
            x + 16,
            y + 66,
            &msg[(cut + 1).min(msg.len())..],
            w - 32,
        );
    }
}

// ---------- Panneau de notifications ----------

unsafe fn draw_notif_panel(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !NOTIF_PANEL {
        return;
    }
    let w = 380;
    let x = l.w - w - 12;
    let header_h = 44;
    let row_h = 56;
    let rows = NOTIF_COUNT.min(NOTIF_HISTORY.len());
    let empty_h = 60;
    let h = header_h
        + if rows == 0 {
            empty_h
        } else {
            rows as i32 * row_h
        }
        + 8;
    let y = l.bar_top - 12 - h;
    panel(
        hdc,
        b,
        x,
        y,
        x + w,
        y + h,
        8,
        rgb(0x24, 0x27, 0x2F),
        rgb(0x3A, 0x3F, 0x4A),
    );
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        x + 16,
        y + 14,
        b"Notifications",
    );
    if NOTIF_COUNT > 0 {
        let clr = rect(x + w - 120, y + 8, x + w - 16, y + 36);
        round_rect(
            hdc,
            b,
            clr.left,
            clr.top,
            clr.right,
            clr.bottom,
            4,
            rgb(0x31, 0x35, 0x3F),
        );
        text(
            hdc,
            f.small,
            rgb(0xC5, 0xC5, 0xC5),
            clr.left + 10,
            clr.top + 7,
            b"Tout effacer",
        );
    }
    fill(
        hdc,
        b,
        x + 1,
        y + header_h - 1,
        x + w - 1,
        y + header_h,
        rgb(0x3A, 0x3F, 0x4A),
    );
    if rows == 0 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            x + w / 2,
            y + header_h + 20,
            b"Aucune notification",
        );
    } else {
        for i in 0..rows {
            let idx = if NOTIF_COUNT <= NOTIF_HISTORY.len() {
                i
            } else {
                i
            };
            let ry = y + header_h + (rows - 1 - i) as i32 * row_h;
            if i > 0 {
                fill(
                    hdc,
                    b,
                    x + 16,
                    ry - 1,
                    x + w - 16,
                    ry,
                    rgb(0x34, 0x38, 0x42),
                );
            }
            glyph(hdc, b, f, Glyph::Start, x + 16, ry + 10, 14);
            text(
                hdc,
                f.caption,
                rgb(0xC5, 0xC5, 0xC5),
                x + 38,
                ry + 8,
                b"Winlux",
            );
            let msg = &NOTIF_HISTORY[idx][..NOTIF_LENS[idx]];
            text_fit(
                hdc,
                f.body,
                13,
                rgb(0xF2, 0xF2, 0xF2),
                x + 16,
                ry + 28,
                msg,
                w - 32,
            );
        }
    }
}

// ---------- Icônes du bureau ----------

const DESKTOP_ICONS: [(&[u8], Glyph); 3] = [
    (b"Ce PC", Glyph::Computer),
    (b"Documents", Glyph::Folder),
    (b"Corbeille", Glyph::Trash),
];

unsafe fn desktop_icons(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    for (i, (name, g)) in DESKTOP_ICONS.iter().enumerate() {
        let (x, y) = (ICON_X[i], ICON_Y[i]);
        if DRAGGED_ICON == i as i32 || SELECTED_ICON == i as i32 {
            round_rect(hdc, b, x, y, x + 76, y + 84, 4, fc(rgb(0x2A, 0x4E, 0x8C)));
        }
        glyph(hdc, b, f, *g, x + 18, y + 8, 40);
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0x00, 0x00, 0x00),
            x + 39,
            y + 59,
            name,
        );
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            x + 38,
            y + 58,
            name,
        );
    }
}

// ---------- Écran de démarrage ----------

/// Logo et cercle de points qui tournent, comme au démarrage de Windows 11 ;
/// en dessous, l'étape de chargement en cours.
unsafe fn boot_splash(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    fill(hdc, b, 0, 0, l.w, l.h, rgb(0, 0, 0));
    let t = since(BOOT_START);
    // Le logo apparaît en fondu pendant 400 ms.
    let a = ease_out(t, 400);
    let size = 96;
    let (lx, ly) = (l.w / 2 - size / 2, l.h / 2 - size - 30);
    let gap = 4;
    let c = (size - gap) / 2;
    let blue = |k: u32| mix(0, k, a, 1000);
    fill(hdc, b, lx, ly, lx + c, ly + c, blue(rgb(0x2B, 0xA4, 0xF2)));
    fill(
        hdc,
        b,
        lx + c + gap,
        ly,
        lx + size,
        ly + c,
        blue(rgb(0x45, 0xB8, 0xFF)),
    );
    fill(
        hdc,
        b,
        lx,
        ly + c + gap,
        lx + c,
        ly + size,
        blue(rgb(0x16, 0x84, 0xE0)),
    );
    fill(
        hdc,
        b,
        lx + c + gap,
        ly + c + gap,
        lx + size,
        ly + size,
        blue(rgb(0x2B, 0xA4, 0xF2)),
    );
    // Cercle de chargement : cinq points qui accélèrent puis ralentissent.
    if t > 500 {
        let (cx, cy, radius) = (l.w / 2, l.h / 2 + 70, 22);
        let cycle = 2200;
        for k in 0..5 {
            let local = (t - 500 - k * 110).rem_euclid(cycle);
            let u = local * 1000 / cycle;
            // Angle « slow-fast-slow » sur deux tours, puis pause invisible.
            if u > 880 {
                continue;
            }
            let v = u * 1000 / 880;
            let angle = 270 + (v * 720 / 1000) - sin_milli(v * 360 / 1000) * 70 / 1000;
            let x = cx + cos_milli(angle) * radius / 1000;
            let y = cy + sin_milli(angle) * radius / 1000;
            let d = 5;
            ellipse(
                hdc,
                b,
                x - d / 2,
                y - d / 2,
                x + d - d / 2,
                y + d - d / 2,
                rgb(0xF2, 0xF2, 0xF2),
            );
        }
    }
    let label: &[u8] = match BOOT_STEP {
        0 | 1 => b"Chargement des polices",
        2 => b"Pr\xE9paration de l'Explorer",
        3 => b"Pr\xE9paration du bureau",
        _ => b"Bienvenue",
    };
    if t > 700 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            l.w / 2,
            l.h - 90,
            label,
        );
    }
    // Préchauffage des polices : un texte de chaque taille hors de l'écran,
    // pour que le premier affichage du bureau ne marque pas de pause.
    if BOOT_STEP <= 1 {
        for font in [
            f.title, f.body, f.semibold, f.small, f.icon, f.large, f.caption, f.display,
        ] {
            text(hdc, font, rgb(0, 0, 0), -4000, -4000, b"Winlux");
        }
    }
}

// ---------- Points d'entrée ----------

/// Lit l'environnement et la taille de l'écran au lancement.
pub unsafe fn init(
    volume: u32,
    battery: u32,
    battery_present: u32,
    network: u32,
    align_left: bool,
) {
    VOLUME_LEVEL = volume;
    BATTERY_LEVEL = battery;
    BATTERY_PRESENT = battery_present;
    NETWORK_CONNECTED = network;
    ALIGN_LEFT = align_left;
    crate::settings::system_tick();
    SCREEN_W = GetSystemMetrics(0).max(640);
    SCREEN_H = GetSystemMetrics(1).max(400);
    wm::WORK = rect(0, 0, SCREEN_W, SCREEN_H - TASKBAR_HEIGHT);
    BOOT_START = now();
    let mut v = [0i8; 4];
    if GetEnvironmentVariableA(c"WINLUX_NO_SPLASH".as_ptr(), v.as_mut_ptr(), 4) > 0 {
        BOOT_DONE = true;
        BOOT_STEP = 4;
    }
}

pub unsafe fn splash_active() -> bool {
    !BOOT_DONE
}

pub unsafe fn draw(hdc: Handle) {
    let l = layout();
    let mut b = Brushes::new();
    let f = Fonts::new();
    SetBkMode(hdc, 1);
    if !BOOT_DONE {
        boot_splash(hdc, &mut b, &f, &l);
    } else {
        let intro = since(INTRO_START);
        FADE = if INTRO_START == 0 || intro >= INTRO_MS {
            1000
        } else {
            ease_out(intro, INTRO_MS)
        };
        wallpaper(hdc, &mut b, &l);
        if FADE > 600 {
            desktop_icons(hdc, &mut b, &f);
        }
        {
            let mut g = Gfx {
                hdc,
                b: &mut b,
                f: &f,
            };
            wm::draw(&mut g);
        }
        start_menu(hdc, &mut b, &f, &l);
        draw_notif_panel(hdc, &mut b, &f, &l);
        draw_toast(hdc, &mut b, &f, &l);
        let bar_dy = if FADE < 1000 {
            (1000 - ease_out(intro, INTRO_MS)) * TASKBAR_HEIGHT / 1000
        } else {
            0
        };
        taskbar(hdc, &mut b, &f, &l, bar_dy);
    }
    f.release();
    b.release();
}

/// Appelé à chaque tour de la boucle de messages : fait avancer le
/// démarrage et repeint à ~60 images/s tant qu'une animation est en cours.
pub unsafe fn frame(hwnd: Handle) {
    if !BOOT_DONE {
        // Une étape de préchargement par image, sans bloquer l'animation.
        match BOOT_STEP {
            0 | 1 if since(BOOT_START) > 300 => BOOT_STEP += 1,
            2 if since(BOOT_START) > 900 => {
                explorer::preload();
                BOOT_STEP = 3;
            }
            3 if since(BOOT_START) > 1600 => BOOT_STEP = 4,
            4 if since(BOOT_START) as u32 >= SPLASH_MIN_MS => {
                BOOT_DONE = true;
                INTRO_START = now();
            }
            _ => {}
        }
    }
    let animating = !BOOT_DONE
        || wm::animating()
        || since(INTRO_START) < INTRO_MS + 40
        || (START_ANIM != 0 && since(START_ANIM) < START_OPEN_MS + 40)
        || START_CLOSING
        || (TOAST_LEN > 0 && (since(TOAST_START) < 300 || since(TOAST_START) > TOAST_MS - 260));
    if animating && since(LAST_FRAME) >= 15 {
        LAST_FRAME = now();
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

/// Appelé deux fois par seconde : suit la résolution de l'écran, rafraîchit
/// l'horloge à chaque nouvelle minute et les champs de saisie qui clignotent.
pub unsafe fn tick(hwnd: Handle) {
    let (w, h) = (GetSystemMetrics(0).max(640), GetSystemMetrics(1).max(400));
    if w != SCREEN_W || h != SCREEN_H {
        SCREEN_W = w;
        SCREEN_H = h;
        wm::WORK = rect(0, 0, w, h - TASKBAR_HEIGHT);
        wm::on_resize();
        for i in 0..3 {
            ICON_X[i] = ICON_X[i].min(w - 80);
            ICON_Y[i] = ICON_Y[i].min(h - TASKBAR_HEIGHT - 90);
        }
        MoveWindow(hwnd, 0, 0, w, h, 1);
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    let mut now = SystemTime::default();
    GetLocalTime(&mut now);
    let system = crate::settings::system_tick();
    let apps = BOOT_DONE && wm::tick();
    if (now.minute != LAST_MINUTE && BOOT_DONE) || apps || system {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

unsafe fn repaint(hwnd: Handle) {
    InvalidateRect(hwnd, core::ptr::null(), 0);
}

pub unsafe fn on_button_down(hwnd: Handle, x: i32, y: i32) {
    if !BOOT_DONE {
        return;
    }
    let l = layout();
    if y >= l.bar_top {
        for i in 0..TASKBAR_BUTTONS {
            if inside(&button_rect(&l, i), x, y) {
                if i == 0 {
                    set_start(!(START_OPEN && !START_CLOSING));
                } else {
                    set_start(false);
                    match i {
                        1 => {
                            if wm::is_open(wm::APP_EXPLORER) {
                                wm::toggle(wm::APP_EXPLORER)
                            } else {
                                explorer::open_home()
                            }
                        }
                        2 => wm::toggle(wm::APP_SETTINGS),
                        _ => {}
                    }
                }
                repaint(hwnd);
                return;
            }
        }
        let tray = l.w - 12 - 92 - 96;
        if x >= tray - 28 && x < tray {
            NOTIF_PANEL = !NOTIF_PANEL;
        } else if x >= tray && x < tray + 24 {
            crate::settings::open_page(2);
        } else if x >= tray + 24 && x < tray + 88 {
            crate::settings::open_page(3);
        } else {
            return;
        }
        repaint(hwnd);
        return;
    }
    if NOTIF_PANEL && y < l.bar_top {
        let pw = 380;
        let px = l.w - pw - 12;
        let rows = NOTIF_COUNT.min(NOTIF_HISTORY.len());
        let ph = 44 + if rows == 0 { 60 } else { rows as i32 * 56 } + 8;
        let py = l.bar_top - 12 - ph;
        if x >= px && x < px + pw && y >= py && y < py + ph {
            if NOTIF_COUNT > 0
                && x >= px + pw - 120
                && x < px + pw - 16
                && y >= py + 8
                && y < py + 36
            {
                NOTIF_COUNT = 0;
                for i in 0..NOTIF_LENS.len() {
                    NOTIF_LENS[i] = 0;
                }
            }
        } else {
            NOTIF_PANEL = false;
        }
        repaint(hwnd);
        return;
    }
    if START_OPEN && !START_CLOSING {
        if inside(&l.start, x, y) {
            match start_item_at(&l, x, y) {
                200 => {
                    crate::settings::control(if ALIGN_LEFT {
                        c"align-center"
                    } else {
                        c"align-left"
                    });
                }
                201 => {
                    crate::settings::control(c"poweroff");
                }
                i @ 0..=5 => {
                    set_start(false);
                    launch(PINNED[i as usize].launch);
                }
                i @ 100..=103 => {
                    set_start(false);
                    launch(RECOMMENDED[(i - 100) as usize].3);
                }
                _ => return,
            }
        } else {
            set_start(false);
        }
        repaint(hwnd);
        return;
    }
    let mut rp = false;
    if wm::on_button_down(x, y, &mut rp) {
        SELECTED_ICON = -1;
        if rp {
            repaint(hwnd);
        }
        return;
    }
    for i in (0..3).rev() {
        if x >= ICON_X[i] && x < ICON_X[i] + 76 && y >= ICON_Y[i] && y < ICON_Y[i] + 84 {
            // Double-clic : ouvre l'emplacement dans l'Explorer.
            if SELECTED_ICON == i as i32 && since(ICON_CLICK_TIME) < 500 {
                SELECTED_ICON = -1;
                match i {
                    0 => explorer::open_pc(),
                    1 => explorer::open_documents(),
                    _ => explorer::open_trash(),
                }
            } else {
                DRAGGED_ICON = i as i32;
                SELECTED_ICON = i as i32;
                ICON_CLICK_TIME = now();
            }
            repaint(hwnd);
            return;
        }
    }
    if SELECTED_ICON >= 0 || rp {
        SELECTED_ICON = -1;
        repaint(hwnd);
    }
}

pub unsafe fn on_mouse_move(hwnd: Handle, x: i32, y: i32) {
    if !BOOT_DONE {
        return;
    }
    let l = layout();
    let mut rp = false;
    let hb = if y >= l.bar_top {
        (0..TASKBAR_BUTTONS)
            .find(|&i| inside(&button_rect(&l, i), x, y))
            .unwrap_or(-1)
    } else {
        -1
    };
    if hb != HOVER_BUTTON {
        HOVER_BUTTON = hb;
        rp = true;
    }
    let hs = if START_OPEN && inside(&l.start, x, y) {
        start_item_at(&l, x, y)
    } else {
        -1
    };
    if hs != HOVER_START_ITEM {
        HOVER_START_ITEM = hs;
        rp = true;
    }
    if DRAGGED_ICON >= 0 {
        let i = DRAGGED_ICON as usize;
        // Grille de 96 px, comme l'alignement automatique de Windows.
        let gx = ((x - 38).max(0) + 48) / 96 * 96 + 12;
        let gy = ((y - 42).max(0) + 48) / 96 * 96 + 12;
        let nx = gx.min(l.w - 80);
        let ny = gy.min(l.bar_top - 90);
        if nx != ICON_X[i] || ny != ICON_Y[i] {
            ICON_X[i] = nx;
            ICON_Y[i] = ny;
            rp = true;
        }
    } else if !START_OPEN {
        wm::on_mouse_move(x, y, &mut rp);
    }
    if rp {
        repaint(hwnd);
    }
}

pub unsafe fn on_button_up(hwnd: Handle, x: i32, y: i32) {
    let mut rp = false;
    if DRAGGED_ICON >= 0 {
        DRAGGED_ICON = -1;
        rp = true;
    }
    wm::on_button_up(x, y, &mut rp);
    if rp {
        repaint(hwnd);
    }
}

/// Molette (`delta` en multiples de 120), clic droit ou milieu.
pub unsafe fn on_other(hwnd: Handle, ev: Mouse, x: i32, y: i32) {
    if !BOOT_DONE || START_OPEN {
        return;
    }
    let mut rp = false;
    wm::on_other(ev, x, y, &mut rp);
    if rp {
        repaint(hwnd);
    }
}

/// Clavier : touche Windows pour le menu Démarrer, Échap pour le fermer,
/// le reste va à la fenêtre active.
pub unsafe fn on_key(hwnd: Handle, vk: u32, ch: u16) {
    if !BOOT_DONE {
        return;
    }
    if ch == 0 && (vk == 0x5B || vk == 0x5C) {
        set_start(!(START_OPEN && !START_CLOSING));
        repaint(hwnd);
        return;
    }
    if START_OPEN && !START_CLOSING {
        if ch == 27 || vk == 0x1B {
            set_start(false);
            repaint(hwnd);
        }
        return;
    }
    if wm::on_key(vk, ch) {
        repaint(hwnd);
    }
}
