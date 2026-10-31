//! Calculatrice avec Chronomètre, Minuteur et Réveil
//! Mode simple avec onglets pour basculer entre les modes

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

static mut MODE: u32 = 0;

static mut CALC_DISPLAY: [u8; 32] = [0; 32];
static mut CALC_DISPLAY_LEN: usize = 0;
static mut CALC_OPERATOR: u8 = 0;
static mut CALC_PREV: f64 = 0.0;
static mut CALC_NEW: bool = true;

static mut STOPWATCH_RUNNING: bool = false;
static mut STOPWATCH_START: u32 = 0;
static mut STOPWATCH_ELAPSED: u32 = 0;
static mut STOPWATCH_LAPS: [[u8; 16]; 5] = [[0; 16]; 5];
static mut STOPWATCH_LAP_COUNT: usize = 0;

static mut TIMER_SECONDS: u32 = 300;
static mut TIMER_RUNNING: bool = false;
static mut TIMER_START: u32 = 0;
static mut TIMER_REMAINING: u32 = 300;

static mut ALARM_HOURS: u32 = 7;
static mut ALARM_MINUTES: u32 = 0;
static mut ALARM_ENABLED: bool = false;
static mut ALARM_TRIGGERED: bool = false;

fn format_time(total_seconds: u32) -> [u8; 16] {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let mut result = [0u8; 16];
    let mut pos = 0;

    if hours > 0 {
        result[pos] = b'0' + (hours / 10) as u8;
        pos += 1;
        result[pos] = b'0' + (hours % 10) as u8;
        pos += 1;
        result[pos] = b':';
        pos += 1;
    }

    result[pos] = b'0' + (minutes / 10) as u8;
    pos += 1;
    result[pos] = b'0' + (minutes % 10) as u8;
    pos += 1;
    result[pos] = b':';
    pos += 1;

    result[pos] = b'0' + (seconds / 10) as u8;
    pos += 1;
    result[pos] = b'0' + (seconds % 10) as u8;

    result
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        rgb(0x1E, 0x1E, 0x1E),
    );

    let tabs: [&[u8]; 4] = [b"Calculatrice", b"Chronometre", b"Minuteur", b"Reveil"];
    let tab_w = (r.right - r.left) / 4;
    for i in 0..4 {
        let tab_x = r.left + (i as i32) * tab_w;
        let tab_bg = if (i as u32) == MODE {
            rgb(0x2D, 0x2D, 0x30)
        } else {
            rgb(0x25, 0x25, 0x26)
        };
        fill(g.hdc, g.b, tab_x, r.top, tab_x + tab_w, r.top + 32, tab_bg);
        text(
            g.hdc,
            g.f.body,
            rgb(0xCC, 0xCC, 0xCC),
            tab_x + 8,
            r.top + 8,
            tabs[i],
        );
    }

    match MODE {
        0 => draw_calculator(g, f),
        1 => draw_stopwatch(g, f),
        2 => draw_timer(g, f),
        3 => draw_alarm(g, f),
        _ => {}
    }
}

unsafe fn draw_calculator(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let y = r.top + 40;

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        y,
        r.right - 16,
        y + 60,
        rgb(0x00, 0x00, 0x00),
    );
    text(
        g.hdc,
        g.f.title,
        rgb(0xFF, 0xFF, 0xFF),
        r.right - 24,
        y + 8,
        &CALC_DISPLAY[..CALC_DISPLAY_LEN],
    );

    let buttons: [[&[u8]; 4]; 5] = [
        [b"C", b"\xB1", b"%", b"/"],
        [b"7", b"8", b"9", b"*"],
        [b"4", b"5", b"6", b"-"],
        [b"1", b"2", b"3", b"+"],
        [b"0", b".", b"=", b""],
    ];

    let btn_w = (r.right - r.left - 32) / 4;
    let btn_h = 50;

    for row in 0..5 {
        for col in 0..4 {
            let bx = r.left + 16 + (col as i32) * btn_w;
            let by = y + 70 + (row as i32) * (btn_h + 4);

            if buttons[row][col].is_empty() {
                continue;
            }

            let btn_color = match buttons[row][col] {
                b"=" => rgb(0x0A, 0x84, 0xFF),
                b"C" | b"\xB1" | b"%" => rgb(0x3E, 0x3E, 0x42),
                b"/" | b"*" | b"-" | b"+" => rgb(0x5A, 0x5A, 0x5C),
                _ => rgb(0x3C, 0x3C, 0x3C),
            };

            fill(g.hdc, g.b, bx, by, bx + btn_w - 4, by + btn_h, btn_color);
            text(
                g.hdc,
                g.f.body,
                rgb(0xFF, 0xFF, 0xFF),
                bx + 8,
                by + 16,
                buttons[row][col],
            );
        }
    }
}

unsafe fn draw_stopwatch(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let y = r.top + 40;

    let now = GetTickCount();
    let elapsed = if STOPWATCH_RUNNING {
        STOPWATCH_ELAPSED + (now - STOPWATCH_START)
    } else {
        STOPWATCH_ELAPSED
    };
    let time_str = format_time(elapsed / 1000);

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        y,
        r.right - 16,
        y + 80,
        rgb(0x00, 0x00, 0x00),
    );
    text_centered(
        g.hdc,
        g.f.title,
        32,
        rgb(0xFF, 0xFF, 0xFF),
        (r.left + r.right) / 2,
        y + 24,
        &time_str,
    );

    let btn_w = 100;
    let by = y + 100;

    let btn_labels: [&[u8]; 3] = if STOPWATCH_RUNNING {
        [b"Pause", b"Tour", b"Reset"]
    } else {
        [b"Demarrer", b"Tour", b"Reset"]
    };

    for i in 0..3 {
        let bx = r.left + 16 + (i as i32) * (btn_w + 8);
        fill(
            g.hdc,
            g.b,
            bx,
            by,
            bx + btn_w,
            by + 40,
            rgb(0x0A, 0x84, 0xFF),
        );
        text_centered(
            g.hdc,
            g.f.body,
            11,
            rgb(0xFF, 0xFF, 0xFF),
            bx + btn_w / 2,
            by + 12,
            btn_labels[i],
        );
    }

    if STOPWATCH_LAP_COUNT > 0 {
        let lap_y = by + 60;
        text(
            g.hdc,
            g.f.small,
            rgb(0xCC, 0xCC, 0xCC),
            r.left + 16,
            lap_y,
            b"Tours :",
        );
        for i in 0..STOPWATCH_LAP_COUNT.min(5) {
            text(
                g.hdc,
                g.f.small,
                rgb(0xCC, 0xCC, 0xCC),
                r.left + 16,
                lap_y + 20 + (i as i32) * 20,
                &STOPWATCH_LAPS[i],
            );
        }
    }
}

unsafe fn draw_timer(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let y = r.top + 40;

    let remaining = if TIMER_RUNNING {
        let elapsed = (GetTickCount() - TIMER_START) / 1000;
        if elapsed >= TIMER_SECONDS {
            TIMER_REMAINING = 0;
            TIMER_RUNNING = false;
        } else {
            TIMER_REMAINING = TIMER_SECONDS - elapsed;
        }
        TIMER_REMAINING
    } else {
        TIMER_REMAINING
    };

    let time_str = format_time(remaining);

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        y,
        r.right - 16,
        y + 80,
        rgb(0x00, 0x00, 0x00),
    );
    text_centered(
        g.hdc,
        g.f.title,
        32,
        rgb(0xFF, 0xFF, 0xFF),
        (r.left + r.right) / 2,
        y + 24,
        &time_str,
    );

    let btn_w = 60;
    let ctrl_y = y + 100;

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        ctrl_y,
        r.left + 16 + btn_w,
        ctrl_y + 40,
        rgb(0x3C, 0x3C, 0x3C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w / 2,
        ctrl_y + 12,
        &format_time(TIMER_SECONDS),
    );

    fill(
        g.hdc,
        g.b,
        r.left + 16 + btn_w + 8,
        ctrl_y,
        r.left + 16 + btn_w * 2 + 4,
        ctrl_y + 40,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w + 8 + btn_w / 2,
        ctrl_y + 12,
        b"+60s",
    );

    fill(
        g.hdc,
        g.b,
        r.left + 16 + btn_w * 2 + 12,
        ctrl_y,
        r.left + 16 + btn_w * 3 + 8,
        ctrl_y + 40,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w * 2 + 12 + btn_w / 2,
        ctrl_y + 12,
        b"-60s",
    );

    let start_btn: &[u8] = if TIMER_RUNNING { b"Pause" } else { b"Demarrer" };
    fill(
        g.hdc,
        g.b,
        r.left + 16,
        ctrl_y + 50,
        r.left + 16 + 100,
        ctrl_y + 90,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + 50,
        ctrl_y + 62,
        start_btn,
    );

    fill(
        g.hdc,
        g.b,
        r.left + 16 + 108,
        ctrl_y + 50,
        r.left + 16 + 208,
        ctrl_y + 90,
        rgb(0x5A, 0x5A, 0x5C),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + 158,
        ctrl_y + 62,
        b"Reset",
    );
}

unsafe fn draw_alarm(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let y = r.top + 40;

    let alarm_str = format_time(ALARM_HOURS * 3600 + ALARM_MINUTES * 60);

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        y,
        r.right - 16,
        y + 80,
        rgb(0x00, 0x00, 0x00),
    );
    text_centered(
        g.hdc,
        g.f.title,
        32,
        rgb(0xFF, 0xFF, 0xFF),
        (r.left + r.right) / 2,
        y + 24,
        &alarm_str,
    );

    let status: &[u8] = if ALARM_ENABLED {
        if ALARM_TRIGGERED {
            b"ALARME !"
        } else {
            b"Active"
        }
    } else {
        b"Desactive"
    };
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xCC, 0xCC, 0xCC),
        (r.left + r.right) / 2,
        y + 60,
        status,
    );

    let btn_w = 100;
    let ctrl_y = y + 100;

    fill(
        g.hdc,
        g.b,
        r.left + 16,
        ctrl_y,
        r.left + 16 + btn_w,
        ctrl_y + 40,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w / 2,
        ctrl_y + 12,
        b"+1h",
    );

    fill(
        g.hdc,
        g.b,
        r.left + 16 + btn_w + 8,
        ctrl_y,
        r.left + 16 + btn_w * 2 + 8,
        ctrl_y + 40,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w + 8 + btn_w / 2,
        ctrl_y + 12,
        b"+1m",
    );

    fill(
        g.hdc,
        g.b,
        r.left + 16 + btn_w * 2 + 16,
        ctrl_y,
        r.left + 16 + btn_w * 3 + 16,
        ctrl_y + 40,
        rgb(0x0A, 0x84, 0xFF),
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + btn_w * 2 + 16 + btn_w / 2,
        ctrl_y + 12,
        b"-1m",
    );

    let toggle_label: &[u8] = if ALARM_ENABLED {
        b"Desactiver"
    } else {
        b"Activer"
    };
    let toggle_color = if ALARM_ENABLED {
        rgb(0xFF, 0x45, 0x3A)
    } else {
        rgb(0x0A, 0x84, 0xFF)
    };
    fill(
        g.hdc,
        g.b,
        r.left + 16,
        ctrl_y + 50,
        r.left + 16 + 150,
        ctrl_y + 90,
        toggle_color,
    );
    text_centered(
        g.hdc,
        g.f.body,
        11,
        rgb(0xFF, 0xFF, 0xFF),
        r.left + 16 + 75,
        ctrl_y + 62,
        toggle_label,
    );
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let r = f.client;
    let y_tabs = r.top + 32;

    if y < y_tabs {
        let tab_w = (r.right - r.left) / 4;
        let clicked_tab = ((x - r.left) / tab_w) as u32;
        if clicked_tab < 4 {
            MODE = clicked_tab;
            return true;
        }
    }

    match MODE {
        0 => calc_mouse(ev, x, y, f),
        1 => stopwatch_mouse(ev, x, y, f),
        2 => timer_mouse(ev, x, y, f),
        3 => alarm_mouse(ev, x, y, f),
        _ => false,
    }
}

unsafe fn calc_mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if ev != Mouse::LeftClick {
        return false;
    }

    let r = f.client;
    let y_start = r.top + 40 + 70;
    let btn_w = (r.right - r.left - 32) / 4;
    let btn_h = 50;

    if y < y_start || y > y_start + 5 * (btn_h + 4) {
        return false;
    }

    let row = ((y - y_start) / (btn_h + 4)) as usize;
    let col = ((x - r.left - 16) / btn_w) as usize;

    if row >= 5 || col >= 4 {
        return false;
    }

    let buttons: [[&[u8]; 4]; 5] = [
        [b"C", b"\xB1", b"%", b"/"],
        [b"7", b"8", b"9", b"*"],
        [b"4", b"5", b"6", b"-"],
        [b"1", b"2", b"3", b"+"],
        [b"0", b".", b"=", b""],
    ];

    let key = buttons[row][col];
    if key.is_empty() {
        return false;
    }

    calc_key(key);
    true
}

unsafe fn calc_key(key: &[u8]) {
    match key {
        b"C" => {
            CALC_DISPLAY = [0; 32];
            CALC_DISPLAY_LEN = 0;
            CALC_OPERATOR = 0;
            CALC_PREV = 0.0;
            CALC_NEW = true;
        }
        b"=" => {
            if CALC_OPERATOR != 0 && CALC_DISPLAY_LEN > 0 {
                let current = parse_display();
                let result = match CALC_OPERATOR {
                    1 => CALC_PREV + current,
                    2 => CALC_PREV - current,
                    3 => CALC_PREV * current,
                    4 => {
                        if current != 0.0 {
                            CALC_PREV / current
                        } else {
                            0.0
                        }
                    }
                    _ => current,
                };
                CALC_OPERATOR = 0;
                CALC_PREV = 0.0;
                set_display(result);
                CALC_NEW = true;
            }
        }
        b"+" | b"-" | b"*" | b"/" => {
            if CALC_DISPLAY_LEN > 0 {
                CALC_PREV = parse_display();
                CALC_OPERATOR = match key {
                    b"+" => 1,
                    b"-" => 2,
                    b"*" => 3,
                    b"/" => 4,
                    _ => 0,
                };
                CALC_NEW = true;
            }
        }
        b"." => {
            if CALC_NEW {
                CALC_DISPLAY = [0; 32];
                CALC_DISPLAY[0] = b'0';
                CALC_DISPLAY[1] = b'.';
                CALC_DISPLAY_LEN = 2;
                CALC_NEW = false;
            } else if !CALC_DISPLAY[..CALC_DISPLAY_LEN].contains(&b'.') {
                CALC_DISPLAY[CALC_DISPLAY_LEN] = b'.';
                CALC_DISPLAY_LEN += 1;
            }
        }
        _ => {
            if key.len() == 1 && key[0].is_ascii_digit() {
                if CALC_NEW {
                    CALC_DISPLAY = [0; 32];
                    CALC_DISPLAY_LEN = 0;
                    CALC_NEW = false;
                }
                if CALC_DISPLAY_LEN < 31 {
                    CALC_DISPLAY[CALC_DISPLAY_LEN] = key[0];
                    CALC_DISPLAY_LEN += 1;
                }
            }
        }
    }
}

unsafe fn parse_display() -> f64 {
    let mut result = 0.0;
    let mut decimal = false;
    let mut divisor = 1.0;
    for i in 0..CALC_DISPLAY_LEN {
        let byte = CALC_DISPLAY[i];
        if byte == b'.' {
            decimal = true;
        } else if byte.is_ascii_digit() {
            let digit = (byte - b'0') as f64;
            if decimal {
                divisor *= 10.0;
                result += digit / divisor;
            } else {
                result = result * 10.0 + digit;
            }
        }
    }
    result
}

unsafe fn set_display(value: f64) {
    CALC_DISPLAY = [0; 32];
    CALC_DISPLAY_LEN = 0;

    if value == 0.0 {
        CALC_DISPLAY[0] = b'0';
        CALC_DISPLAY_LEN = 1;
        return;
    }

    let mut v = value.abs();

    while v >= 1.0 && CALC_DISPLAY_LEN < 15 {
        let digit = ((v as u64) % 10) as u8 + b'0';
        let len = CALC_DISPLAY_LEN;
        let mut i = len;
        while i > 0 {
            CALC_DISPLAY[i] = CALC_DISPLAY[i - 1];
            i -= 1;
        }
        CALC_DISPLAY[0] = digit;
        CALC_DISPLAY_LEN += 1;
        v = ((v as u64) / 10) as f64;
    }

    let frac = value - (value as i64) as f64;
    if frac.abs() > 0.001 && CALC_DISPLAY_LEN < 30 {
        CALC_DISPLAY[CALC_DISPLAY_LEN] = b'.';
        CALC_DISPLAY_LEN += 1;
        let mut f = frac.abs();
        let mut count = 0;
        while f > 0.001 && CALC_DISPLAY_LEN < 31 && count < 6 {
            f *= 10.0;
            let digit = f as u8;
            CALC_DISPLAY[CALC_DISPLAY_LEN] = digit + b'0';
            CALC_DISPLAY_LEN += 1;
            f -= digit as f64;
            count += 1;
        }
    }

    if value < 0.0 && CALC_DISPLAY_LEN < 31 {
        let len = CALC_DISPLAY_LEN;
        let mut i = len;
        while i > 0 {
            CALC_DISPLAY[i] = CALC_DISPLAY[i - 1];
            i -= 1;
        }
        CALC_DISPLAY[0] = b'-';
        CALC_DISPLAY_LEN += 1;
    }
}

unsafe fn stopwatch_mouse(ev: Mouse, _x: i32, _y: i32, f: &Frame) -> bool {
    if ev != Mouse::LeftClick {
        return false;
    }

    let r = f.client;
    let btn_y = r.top + 40 + 100;
    let btn_w = 100;

    if _y >= btn_y && _y <= btn_y + 40 {
        if _x >= r.left + 16 && _x < r.left + 16 + btn_w {
            STOPWATCH_RUNNING = !STOPWATCH_RUNNING;
            if STOPWATCH_RUNNING {
                STOPWATCH_START = GetTickCount();
            } else {
                STOPWATCH_ELAPSED += GetTickCount() - STOPWATCH_START;
            }
            return true;
        } else if _x >= r.left + 16 + btn_w + 8 && _x < r.left + 16 + btn_w * 2 + 8 {
            if STOPWATCH_RUNNING || STOPWATCH_ELAPSED > 0 {
                let now = if STOPWATCH_RUNNING {
                    STOPWATCH_ELAPSED + (GetTickCount() - STOPWATCH_START)
                } else {
                    STOPWATCH_ELAPSED
                };
                let lap_str = format_time(now / 1000);
                if STOPWATCH_LAP_COUNT < 5 {
                    STOPWATCH_LAPS[STOPWATCH_LAP_COUNT] = lap_str;
                    STOPWATCH_LAP_COUNT += 1;
                }
            }
            return true;
        } else if _x >= r.left + 16 + btn_w * 2 + 16 && _x < r.left + 16 + btn_w * 3 + 16 {
            STOPWATCH_RUNNING = false;
            STOPWATCH_ELAPSED = 0;
            STOPWATCH_LAP_COUNT = 0;
            return true;
        }
    }

    false
}

unsafe fn timer_mouse(ev: Mouse, _x: i32, _y: i32, f: &Frame) -> bool {
    if ev != Mouse::LeftClick {
        return false;
    }

    let r = f.client;
    let btn_y = r.top + 40 + 100;
    let btn_w = 60;

    if _y >= btn_y && _y <= btn_y + 40 {
        if _x >= r.left + 16 + btn_w + 8 && _x < r.left + 16 + btn_w * 2 + 4 {
            TIMER_SECONDS = TIMER_SECONDS.saturating_add(60);
            TIMER_REMAINING = TIMER_SECONDS;
            return true;
        } else if _x >= r.left + 16 + btn_w * 2 + 12 && _x < r.left + 16 + btn_w * 3 + 8 {
            TIMER_SECONDS = TIMER_SECONDS.saturating_sub(60);
            TIMER_REMAINING = TIMER_SECONDS;
            return true;
        }
    }

    if _y >= btn_y + 50 && _y <= btn_y + 90 {
        if _x >= r.left + 16 && _x < r.left + 16 + 100 {
            TIMER_RUNNING = !TIMER_RUNNING;
            if TIMER_RUNNING {
                TIMER_START = GetTickCount();
            }
            return true;
        } else if _x >= r.left + 16 + 108 && _x < r.left + 16 + 208 {
            TIMER_RUNNING = false;
            TIMER_SECONDS = 300;
            TIMER_REMAINING = 300;
            return true;
        }
    }

    false
}

unsafe fn alarm_mouse(ev: Mouse, _x: i32, _y: i32, f: &Frame) -> bool {
    if ev != Mouse::LeftClick {
        return false;
    }

    let r = f.client;
    let btn_y = r.top + 40 + 100;
    let btn_w = 100;

    if _y >= btn_y && _y <= btn_y + 40 {
        if _x >= r.left + 16 && _x < r.left + 16 + btn_w {
            ALARM_HOURS = (ALARM_HOURS + 1) % 24;
            return true;
        } else if _x >= r.left + 16 + btn_w + 8 && _x < r.left + 16 + btn_w * 2 + 8 {
            ALARM_MINUTES = (ALARM_MINUTES + 10) % 60;
            return true;
        } else if _x >= r.left + 16 + btn_w * 2 + 16 && _x < r.left + 16 + btn_w * 3 + 16 {
            ALARM_MINUTES = if ALARM_MINUTES >= 10 {
                ALARM_MINUTES - 10
            } else {
                ALARM_MINUTES + 50
            };
            return true;
        }
    }

    if _y >= btn_y + 50 && _y <= btn_y + 90 {
        if _x >= r.left + 16 && _x < r.left + 16 + 150 {
            ALARM_ENABLED = !ALARM_ENABLED;
            ALARM_TRIGGERED = false;
            return true;
        }
    }

    false
}
