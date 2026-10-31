//! Centre de notifications Windows 11 : panneau latéral droit avec calendrier,
//! notifications empilées par application, et boutons d'actions rapides
//! (Wi-Fi, Bluetooth, mode avion, veilleuse, etc.).
//!
//! Le panneau glisse depuis le bord droit avec une animation ease-out et se
//! ferme au clic à l'extérieur ou sur Échap.

use crate::desktop;
use crate::gfx::*;
use crate::settings;
use winlux_testexe::*;

const PANEL_W: i32 = 380;
const ANIM_OPEN_MS: i32 = 220;
const ANIM_CLOSE_MS: i32 = 160;
const QUICK_COLS: i32 = 3;
const QUICK_ROWS: i32 = 2;
const QUICK_CELL_W: i32 = 108;
const QUICK_CELL_H: i32 = 40;
const QUICK_GAP: i32 = 8;
const CAL_CELL: i32 = 40;
const MAX_NOTIFS: usize = 16;
const MAX_MSG_LEN: usize = 120;
const MAX_APP_LEN: usize = 32;

pub static mut OPEN: bool = false;
static mut CLOSING: bool = false;
static mut ANIM_START: u32 = 0;
static mut HOVER_QUICK: i32 = -1;
static mut HOVER_NOTIF_DISMISS: i32 = -1;
static mut HOVER_CAL_NAV: i32 = -1; // 0 = prev, 1 = next
static mut CAL_MONTH_OFFSET: i32 = 0;

// Quick actions toggle state.
static mut QA_WIFI: bool = true;
static mut QA_BLUETOOTH: bool = false;
static mut QA_AIRPLANE: bool = false;
static mut QA_NIGHT_LIGHT: bool = false;
static mut QA_BATTERY_SAVER: bool = false;
static mut QA_FOCUS_ASSIST: bool = false;

struct QuickAction {
    label: &'static [u8],
    state: unsafe fn() -> bool,
    toggle: unsafe fn(),
}

const QUICK_ACTIONS: [QuickAction; 6] = [
    QuickAction {
        label: b"Wi-Fi",
        state: || unsafe { QA_WIFI },
        toggle: || unsafe { QA_WIFI = !QA_WIFI },
    },
    QuickAction {
        label: b"Bluetooth",
        state: || unsafe { QA_BLUETOOTH },
        toggle: || unsafe { QA_BLUETOOTH = !QA_BLUETOOTH },
    },
    QuickAction {
        label: b"Mode avion",
        state: || unsafe { QA_AIRPLANE },
        toggle: || unsafe { QA_AIRPLANE = !QA_AIRPLANE },
    },
    QuickAction {
        label: b"Veilleuse",
        state: || unsafe { QA_NIGHT_LIGHT },
        toggle: || unsafe { QA_NIGHT_LIGHT = !QA_NIGHT_LIGHT },
    },
    QuickAction {
        label: b"\xC9co. batterie",
        state: || unsafe { QA_BATTERY_SAVER },
        toggle: || unsafe { QA_BATTERY_SAVER = !QA_BATTERY_SAVER },
    },
    QuickAction {
        label: b"Ne pas d\xE9ranger",
        state: || unsafe { QA_FOCUS_ASSIST },
        toggle: || unsafe { QA_FOCUS_ASSIST = !QA_FOCUS_ASSIST },
    },
];

// -- Notification storage --

#[derive(Clone, Copy)]
struct Notification {
    app: [u8; MAX_APP_LEN],
    app_len: usize,
    msg: [u8; MAX_MSG_LEN],
    msg_len: usize,
    time: u32,
}

impl Notification {
    const fn empty() -> Self {
        Self {
            app: [0; MAX_APP_LEN],
            app_len: 0,
            msg: [0; MAX_MSG_LEN],
            msg_len: 0,
            time: 0,
        }
    }
}

static mut NOTIFS: [Notification; MAX_NOTIFS] = [Notification::empty(); MAX_NOTIFS];
static mut NOTIF_COUNT: usize = 0;

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}

// -- Public API --

pub unsafe fn push(app: &[u8], msg: &[u8]) {
    let mut n = Notification::empty();
    let al = app.len().min(MAX_APP_LEN);
    n.app[..al].copy_from_slice(&app[..al]);
    n.app_len = al;
    let ml = msg.len().min(MAX_MSG_LEN);
    n.msg[..ml].copy_from_slice(&msg[..ml]);
    n.msg_len = ml;
    n.time = now();
    if NOTIF_COUNT < MAX_NOTIFS {
        NOTIFS[NOTIF_COUNT] = n;
        NOTIF_COUNT += 1;
    } else {
        for i in 1..MAX_NOTIFS {
            NOTIFS[i - 1] = NOTIFS[i];
        }
        NOTIFS[MAX_NOTIFS - 1] = n;
    }
}

pub unsafe fn push_simple(msg: &[u8]) {
    push(b"Winlux", msg);
}

pub unsafe fn set_open(open: bool) {
    if open == (OPEN && !CLOSING) {
        return;
    }
    ANIM_START = now();
    if open {
        OPEN = true;
        CLOSING = false;
    } else if OPEN {
        CLOSING = true;
    }
}

pub unsafe fn toggle() {
    set_open(!(OPEN && !CLOSING));
}

pub fn is_open() -> bool {
    unsafe { OPEN }
}

pub unsafe fn animating() -> bool {
    if !OPEN {
        return false;
    }
    let t = since(ANIM_START);
    if CLOSING {
        t < ANIM_CLOSE_MS
    } else {
        t < ANIM_OPEN_MS
    }
}

fn panel_slide() -> i32 {
    unsafe {
        let t = since(ANIM_START);
        if CLOSING {
            if t >= ANIM_CLOSE_MS {
                OPEN = false;
                CLOSING = false;
                return PANEL_W;
            }
            ease_out(t, ANIM_CLOSE_MS) * PANEL_W / 1000
        } else if t < ANIM_OPEN_MS {
            (1000 - ease_out(t, ANIM_OPEN_MS)) * PANEL_W / 1000
        } else {
            0
        }
    }
}

// -- Layout --

struct PanelLayout {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    quick_y: i32,
    cal_y: i32,
    notif_y: i32,
}

fn panel_layout(screen_w: i32, bar_top: i32) -> PanelLayout {
    let w = PANEL_W.min(screen_w - 24);
    let h = bar_top - 24;
    let x = screen_w - w - 12;
    let y = 12;
    let quick_y = y + 16;
    let quick_h = QUICK_ROWS * QUICK_CELL_H + (QUICK_ROWS - 1) * QUICK_GAP;
    let cal_y = quick_y + quick_h + 20;
    let cal_h = 24 + 7 * 24 + 8;
    let notif_y = cal_y + cal_h + 16;
    PanelLayout {
        x,
        y,
        w,
        h,
        quick_y,
        cal_y,
        notif_y,
    }
}

fn quick_rect(pl: &PanelLayout, i: i32) -> Rect {
    let col = i % QUICK_COLS;
    let row = i / QUICK_COLS;
    let total_w = QUICK_COLS * QUICK_CELL_W + (QUICK_COLS - 1) * QUICK_GAP;
    let ox = pl.x + (pl.w - total_w) / 2;
    let x = ox + col * (QUICK_CELL_W + QUICK_GAP);
    let y = pl.quick_y + row * (QUICK_CELL_H + QUICK_GAP);
    rect(x, y, x + QUICK_CELL_W, y + QUICK_CELL_H)
}

// -- Calendar helpers --

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(m: i32, y: i32) -> i32 {
    match m {
        1 => 31,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 30,
    }
}

fn day_of_week(y: i32, m: i32, d: i32) -> i32 {
    // Zeller-like: 0 = Monday.
    let (y2, m2) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let q = d;
    let k = y2 % 100;
    let j = y2 / 100;
    let h = (q + (13 * (m2 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // h: 0=Sat, 1=Sun, 2=Mon, ..., 6=Fri -> convert to 0=Mon
    ((h + 5) % 7)
}

fn month_name(m: i32) -> &'static [u8] {
    match m {
        1 => b"Janvier",
        2 => b"F\xE9vrier",
        3 => b"Mars",
        4 => b"Avril",
        5 => b"Mai",
        6 => b"Juin",
        7 => b"Juillet",
        8 => b"Ao\xFBt",
        9 => b"Septembre",
        10 => b"Octobre",
        11 => b"Novembre",
        12 => b"D\xE9cembre",
        _ => b"?",
    }
}

// -- Drawing --

unsafe fn draw_quick_actions(hdc: Handle, b: &mut Brushes, f: &Fonts, pl: &PanelLayout) {
    for (i, qa) in QUICK_ACTIONS.iter().enumerate() {
        let r = quick_rect(pl, i as i32);
        let active = (qa.state)();
        let bg = if active {
            ACCENT
        } else if HOVER_QUICK == i as i32 {
            rgb(0x38, 0x3C, 0x46)
        } else {
            rgb(0x2F, 0x33, 0x3D)
        };
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 6, bg);
        let tc = if active {
            rgb(0x00, 0x00, 0x00)
        } else {
            rgb(0xE6, 0xE6, 0xE6)
        };
        text_centered(
            hdc,
            f.small,
            11,
            tc,
            (r.left + r.right) / 2,
            r.top + 13,
            qa.label,
        );
    }
}

unsafe fn draw_calendar(hdc: Handle, b: &mut Brushes, f: &Fonts, pl: &PanelLayout) {
    let mut st = SystemTime::default();
    GetLocalTime(&mut st);
    let today_d = st.day as i32;
    let today_m = st.month as i32;
    let today_y = st.year as i32;

    let mut m = today_m + CAL_MONTH_OFFSET;
    let mut y = today_y;
    while m < 1 {
        m += 12;
        y -= 1;
    }
    while m > 12 {
        m -= 12;
        y += 1;
    }

    let cx = pl.x + pl.w / 2;
    let cy = pl.cal_y;

    // Month/year header with nav arrows.
    let name = month_name(m);
    let mut hdr = [0u8; 32];
    let mut hl = 0;
    for &c in name {
        if hl < 28 {
            hdr[hl] = c;
            hl += 1;
        }
    }
    hdr[hl] = b' ';
    hl += 1;
    let ys = [
        b'0' + (y / 1000 % 10) as u8,
        b'0' + (y / 100 % 10) as u8,
        b'0' + (y / 10 % 10) as u8,
        b'0' + (y % 10) as u8,
    ];
    for &c in &ys {
        if hl < 32 {
            hdr[hl] = c;
            hl += 1;
        }
    }
    let hdr_slice = &hdr[..hl];

    // Arrow left.
    let arrow_y = cy + 2;
    let arrow_l_bg = if HOVER_CAL_NAV == 0 {
        rgb(0x38, 0x3C, 0x46)
    } else {
        rgb(0x24, 0x27, 0x2F)
    };
    round_rect(
        hdc,
        b,
        pl.x + 12,
        arrow_y,
        pl.x + 36,
        arrow_y + 20,
        4,
        arrow_l_bg,
    );
    text(
        hdc,
        f.body,
        rgb(0xE6, 0xE6, 0xE6),
        pl.x + 18,
        arrow_y + 2,
        b"<",
    );

    text_centered(hdc, f.semibold, 13, rgb(0xF2, 0xF2, 0xF2), cx, cy + 2, hdr_slice);

    // Arrow right.
    let arrow_r_bg = if HOVER_CAL_NAV == 1 {
        rgb(0x38, 0x3C, 0x46)
    } else {
        rgb(0x24, 0x27, 0x2F)
    };
    round_rect(
        hdc,
        b,
        pl.x + pl.w - 36,
        arrow_y,
        pl.x + pl.w - 12,
        arrow_y + 20,
        4,
        arrow_r_bg,
    );
    text(
        hdc,
        f.body,
        rgb(0xE6, 0xE6, 0xE6),
        pl.x + pl.w - 30,
        arrow_y + 2,
        b">",
    );

    // Day-of-week headers.
    let dow_labels: [&[u8]; 7] = [b"Lu", b"Ma", b"Me", b"Je", b"Ve", b"Sa", b"Di"];
    let grid_w = 7 * CAL_CELL;
    let gx = pl.x + (pl.w - grid_w) / 2;
    let gy = cy + 26;
    for (d, label) in dow_labels.iter().enumerate() {
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0x9A, 0xA3, 0xAE),
            gx + d as i32 * CAL_CELL + CAL_CELL / 2,
            gy,
            label,
        );
    }

    // Day grid.
    let first_dow = day_of_week(y, m, 1);
    let dim = days_in_month(m, y);
    for day in 1..=dim {
        let pos = first_dow + day - 1;
        let col = pos % 7;
        let row = pos / 7;
        let dx = gx + col * CAL_CELL;
        let dy = gy + 18 + row * 24;
        let is_today = day == today_d && m == today_m && y == today_y;
        if is_today {
            ellipse(
                hdc,
                b,
                dx + CAL_CELL / 2 - 12,
                dy - 2,
                dx + CAL_CELL / 2 + 12,
                dy + 18,
                ACCENT,
            );
        }
        let mut buf = [0u8; 2];
        let len;
        if day >= 10 {
            buf[0] = b'0' + (day / 10) as u8;
            buf[1] = b'0' + (day % 10) as u8;
            len = 2;
        } else {
            buf[0] = b'0' + day as u8;
            len = 1;
        }
        let tc = if is_today {
            rgb(0x00, 0x00, 0x00)
        } else {
            rgb(0xE6, 0xE6, 0xE6)
        };
        text_centered(
            hdc,
            f.small,
            11,
            tc,
            dx + CAL_CELL / 2,
            dy,
            &buf[..len],
        );
    }
}

unsafe fn draw_notifications(hdc: Handle, b: &mut Brushes, f: &Fonts, pl: &PanelLayout) {
    let y0 = pl.notif_y;
    let count = NOTIF_COUNT;

    // Section header.
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        pl.x + 16,
        y0,
        b"Notifications",
    );
    if count > 0 {
        let clr_x = pl.x + pl.w - 100;
        round_rect(
            hdc,
            b,
            clr_x,
            y0 - 2,
            clr_x + 88,
            y0 + 18,
            4,
            rgb(0x31, 0x35, 0x3F),
        );
        text(
            hdc,
            f.small,
            rgb(0xC5, 0xC5, 0xC5),
            clr_x + 8,
            y0 + 1,
            b"Tout effacer",
        );
    }

    fill(
        hdc,
        b,
        pl.x + 16,
        y0 + 22,
        pl.x + pl.w - 16,
        y0 + 23,
        rgb(0x3A, 0x3F, 0x4A),
    );

    if count == 0 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            pl.x + pl.w / 2,
            y0 + 48,
            b"Aucune notification",
        );
        return;
    }

    let max_visible = ((pl.y + pl.h - y0 - 30) / 64).max(1) as usize;
    let start = if count > max_visible {
        count - max_visible
    } else {
        0
    };
    for (vi, i) in (start..count).enumerate() {
        let ny = y0 + 30 + vi as i32 * 64;
        if ny + 60 > pl.y + pl.h {
            break;
        }
        let n = &NOTIFS[i];

        // Card background.
        round_rect(
            hdc,
            b,
            pl.x + 12,
            ny,
            pl.x + pl.w - 12,
            ny + 58,
            6,
            rgb(0x2B, 0x2F, 0x38),
        );

        // App icon placeholder (small Winlux logo).
        glyph(hdc, b, f, Glyph::Start, pl.x + 20, ny + 6, 12);

        // App name.
        text(
            hdc,
            f.caption,
            rgb(0xC5, 0xC5, 0xC5),
            pl.x + 38,
            ny + 6,
            &n.app[..n.app_len],
        );

        // Dismiss button (X).
        let dismiss_x = pl.x + pl.w - 36;
        let dismiss_bg = if HOVER_NOTIF_DISMISS == vi as i32 {
            rgb(0x44, 0x48, 0x52)
        } else {
            rgb(0x2B, 0x2F, 0x38)
        };
        round_rect(
            hdc,
            b,
            dismiss_x,
            ny + 4,
            dismiss_x + 20,
            ny + 20,
            3,
            dismiss_bg,
        );
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0x9A, 0xA3, 0xAE),
            dismiss_x + 10,
            ny + 5,
            b"x",
        );

        // Message.
        text_fit(
            hdc,
            f.body,
            13,
            rgb(0xF2, 0xF2, 0xF2),
            pl.x + 20,
            ny + 28,
            &n.msg[..n.msg_len],
            pl.w - 48,
        );

        // Timestamp.
        let ago = since(n.time) / 1000;
        let mut ts = [0u8; 16];
        let tl;
        if ago < 60 {
            ts[..11].copy_from_slice(b"\xC0 l'instant");
            tl = 11;
        } else if ago < 3600 {
            let mins = ago / 60;
            if mins < 10 {
                ts[0] = b'0' + mins as u8;
                ts[1..5].copy_from_slice(b" min");
                tl = 5;
            } else {
                ts[0] = b'0' + (mins / 10) as u8;
                ts[1] = b'0' + (mins % 10) as u8;
                ts[2..6].copy_from_slice(b" min");
                tl = 6;
            }
        } else {
            let hrs = ago / 3600;
            if hrs < 10 {
                ts[0] = b'0' + hrs as u8;
                ts[1] = b'h';
                tl = 2;
            } else {
                ts[0] = b'0' + (hrs / 10) as u8;
                ts[1] = b'0' + (hrs % 10) as u8;
                ts[2] = b'h';
                tl = 3;
            }
        };
        text(
            hdc,
            f.small,
            rgb(0x6A, 0x70, 0x7A),
            pl.x + 20,
            ny + 44,
            &ts[..tl],
        );
    }
}

pub unsafe fn draw(hdc: Handle, b: &mut Brushes, f: &Fonts, screen_w: i32, bar_top: i32) {
    if !OPEN {
        return;
    }
    let slide = panel_slide();
    let pl = panel_layout(screen_w, bar_top);
    let dx = slide;

    // Background panel.
    panel(
        hdc,
        b,
        pl.x + dx,
        pl.y,
        pl.x + pl.w + dx,
        pl.y + pl.h,
        10,
        rgb(0x24, 0x27, 0x2F),
        rgb(0x3A, 0x3F, 0x4A),
    );

    // Offset layout for drawing.
    let dpl = PanelLayout {
        x: pl.x + dx,
        w: pl.w,
        y: pl.y,
        h: pl.h,
        quick_y: pl.quick_y,
        cal_y: pl.cal_y,
        notif_y: pl.notif_y,
    };

    draw_quick_actions(hdc, b, f, &dpl);
    draw_calendar(hdc, b, f, &dpl);
    draw_notifications(hdc, b, f, &dpl);
}

// -- Input --

pub unsafe fn on_click(screen_w: i32, bar_top: i32, x: i32, y: i32) -> bool {
    if !OPEN {
        return false;
    }
    let pl = panel_layout(screen_w, bar_top);
    if x < pl.x || x >= pl.x + pl.w || y < pl.y || y >= pl.y + pl.h {
        set_open(false);
        return true;
    }

    // Quick action clicks.
    for i in 0..QUICK_ACTIONS.len() as i32 {
        let r = quick_rect(&pl, i);
        if inside(&r, x, y) {
            (QUICK_ACTIONS[i as usize].toggle)();
            return true;
        }
    }

    // Calendar nav arrows.
    let arrow_y = pl.cal_y + 2;
    if x >= pl.x + 12 && x < pl.x + 36 && y >= arrow_y && y < arrow_y + 20 {
        CAL_MONTH_OFFSET -= 1;
        return true;
    }
    if x >= pl.x + pl.w - 36 && x < pl.x + pl.w - 12 && y >= arrow_y && y < arrow_y + 20 {
        CAL_MONTH_OFFSET += 1;
        return true;
    }

    // Clear all notifications.
    if NOTIF_COUNT > 0 {
        let clr_x = pl.x + pl.w - 100;
        if x >= clr_x && x < clr_x + 88 && y >= pl.notif_y - 2 && y < pl.notif_y + 18 {
            NOTIF_COUNT = 0;
            return true;
        }
    }

    // Dismiss individual notification.
    let max_visible = ((pl.h - (pl.notif_y - pl.y) - 30) / 64).max(1) as usize;
    let start = if NOTIF_COUNT > max_visible {
        NOTIF_COUNT - max_visible
    } else {
        0
    };
    for (vi, i) in (start..NOTIF_COUNT).enumerate() {
        let ny = pl.notif_y + 30 + vi as i32 * 64;
        let dismiss_x = pl.x + pl.w - 36;
        if x >= dismiss_x && x < dismiss_x + 20 && y >= ny + 4 && y < ny + 20 {
            // Remove notification at index i.
            for j in i..NOTIF_COUNT.saturating_sub(1) {
                NOTIFS[j] = NOTIFS[j + 1];
            }
            if NOTIF_COUNT > 0 {
                NOTIF_COUNT -= 1;
            }
            return true;
        }
    }

    true
}

pub unsafe fn on_mouse_move(screen_w: i32, bar_top: i32, x: i32, y: i32) -> bool {
    if !OPEN {
        return false;
    }
    let pl = panel_layout(screen_w, bar_top);
    let mut changed = false;

    // Quick action hover.
    let mut hq: i32 = -1;
    for i in 0..QUICK_ACTIONS.len() as i32 {
        if inside(&quick_rect(&pl, i), x, y) {
            hq = i;
            break;
        }
    }
    if hq != HOVER_QUICK {
        HOVER_QUICK = hq;
        changed = true;
    }

    // Calendar nav hover.
    let arrow_y = pl.cal_y + 2;
    let mut hcn: i32 = -1;
    if x >= pl.x + 12 && x < pl.x + 36 && y >= arrow_y && y < arrow_y + 20 {
        hcn = 0;
    } else if x >= pl.x + pl.w - 36 && x < pl.x + pl.w - 12 && y >= arrow_y && y < arrow_y + 20 {
        hcn = 1;
    }
    if hcn != HOVER_CAL_NAV {
        HOVER_CAL_NAV = hcn;
        changed = true;
    }

    // Notification dismiss hover.
    let max_visible = ((pl.h - (pl.notif_y - pl.y) - 30) / 64).max(1) as usize;
    let start = if NOTIF_COUNT > max_visible {
        NOTIF_COUNT - max_visible
    } else {
        0
    };
    let mut hnd: i32 = -1;
    for (vi, _) in (start..NOTIF_COUNT).enumerate() {
        let ny = pl.notif_y + 30 + vi as i32 * 64;
        let dismiss_x = pl.x + pl.w - 36;
        if x >= dismiss_x && x < dismiss_x + 20 && y >= ny + 4 && y < ny + 20 {
            hnd = vi as i32;
            break;
        }
    }
    if hnd != HOVER_NOTIF_DISMISS {
        HOVER_NOTIF_DISMISS = hnd;
        changed = true;
    }

    changed
}

pub unsafe fn notification_count() -> usize {
    NOTIF_COUNT
}
