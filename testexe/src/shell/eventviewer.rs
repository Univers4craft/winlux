//! Observateur d'\xE9v\xE9nements (Event Viewer) fa\xE7on Windows 11 : journal
//! centralis\xE9 Winlux avec filtre par cat\xE9gorie, niveau et source.
//! Lit les fichiers /run/winlux/journal/winlux.log et crash.log.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const SIDEBAR_W: i32 = 160;
const TOOLBAR_H: i32 = 36;
const ROW_H: i32 = 22;
const HEADER_H: i32 = 24;
const DETAIL_H: i32 = 80;
const MAX_ENTRIES: usize = 512;
const MAX_MSG: usize = 200;
const BUF_CAP: usize = 32768;

// Cat\xE9gories
const CAT_ALL: usize = 0;
const CAT_SYSTEM: usize = 1;
const CAT_APP: usize = 2;
const CAT_SECURITY: usize = 3;
const CAT_WINLUX: usize = 4;
const CAT_CRASH: usize = 5;
const CAT_NAMES: [&[u8]; 6] = [
    b"Tous",
    b"Syst\xE8me",
    b"Applications",
    b"S\xE9curit\xE9",
    b"Winlux",
    b"Crash",
];

#[derive(Clone, Copy)]
struct Entry {
    level: u8, // 0=error, 1=warn, 2=info, 3=debug, 4=trace, 5=crash
    cat: usize,
    time: [u8; 8], // HH:MM:SS
    source: [u8; 24],
    source_len: usize,
    msg: [u8; MAX_MSG],
    msg_len: usize,
}

const EMPTY_ENTRY: Entry = Entry {
    level: 2,
    cat: CAT_WINLUX,
    time: *b"00:00:00",
    source: [0; 24],
    source_len: 0,
    msg: [0; MAX_MSG],
    msg_len: 0,
};

static mut ENTRIES: [Entry; MAX_ENTRIES] = [EMPTY_ENTRY; MAX_ENTRIES];
static mut ENTRY_COUNT: usize = 0;
static mut SELECTED_CAT: usize = CAT_ALL;
static mut SELECTED_ROW: i32 = -1;
static mut SCROLL: i32 = 0;
static mut HOVER_ROW: i32 = -1;
static mut HOVER_CAT: i32 = -1;
static mut HOVER_BTN: i32 = -1;
static mut LAST_REFRESH: u32 = 0;

fn level_name(l: u8) -> &'static [u8] {
    match l {
        0 => b"Erreur",
        1 => b"Avertis.",
        2 => b"Info",
        3 => b"D\xE9boguer",
        4 => b"Trace",
        5 => b"Crash",
        _ => b"?",
    }
}

fn level_color(l: u8) -> u32 {
    match l {
        0 | 5 => rgb(0xE8, 0x11, 0x23), // rouge
        1 => rgb(0xFF, 0xB9, 0x00),     // orange
        2 => rgb(0x00, 0x78, 0xD4),     // bleu
        _ => rgb(0x80, 0x80, 0x80),     // gris
    }
}

fn categorize_source(src: &[u8]) -> usize {
    if src.starts_with(b"kernel") || src.starts_with(b"init") || src.starts_with(b"udev") {
        CAT_SYSTEM
    } else if src.starts_with(b"loader") || src.starts_with(b"managed") || src.starts_with(b"pe") {
        CAT_APP
    } else if src.starts_with(b"auth") || src.starts_with(b"secur") {
        CAT_SECURITY
    } else {
        CAT_WINLUX
    }
}

unsafe fn load_entries() {
    ENTRY_COUNT = 0;

    // Lire winlux.log
    let mut buf = [0u8; BUF_CAP];
    let n = read_file(c"Z:\\run\\winlux\\journal\\winlux.log".as_ptr(), &mut buf);
    if n > 0 {
        parse_log(&buf[..n]);
    }

    // Lire crash.log
    let mut cbuf = [0u8; BUF_CAP];
    let cn = read_file(c"Z:\\run\\winlux\\journal\\crash.log".as_ptr(), &mut cbuf);
    if cn > 0 {
        parse_crash(&cbuf[..cn]);
    }
}

unsafe fn read_file(path: *const core::ffi::c_char, buf: &mut [u8]) -> usize {
    let file = CreateFileA(
        path,
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if file.is_null() || file as isize == -1 {
        return 0;
    }
    let size = GetFileSize(file, core::ptr::null_mut());
    if size != u32::MAX && size > buf.len() as u32 {
        SetFilePointer(
            file,
            (size - buf.len() as u32) as i32,
            core::ptr::null_mut(),
            0,
        );
    }
    let mut read = 0u32;
    ReadFile(
        file,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    );
    CloseHandle(file);
    read as usize
}

// Format : HH:MM:SS [level] source: message
unsafe fn parse_log(data: &[u8]) {
    for line in data.split(|&b| b == b'\n') {
        if line.len() < 16 || ENTRY_COUNT >= MAX_ENTRIES {
            continue;
        }
        let mut e = EMPTY_ENTRY;

        // Temps HH:MM:SS
        if line.len() >= 8 && line[2] == b':' && line[5] == b':' {
            e.time.copy_from_slice(&line[..8]);
        } else {
            continue;
        }

        // [level]
        let rest = if line.len() > 10 {
            &line[9..]
        } else {
            continue;
        };
        if rest.first() != Some(&b'[') {
            continue;
        }
        let bracket_end = match rest.iter().position(|&b| b == b']') {
            Some(p) => p,
            None => continue,
        };
        let lvl = &rest[1..bracket_end];
        e.level = match lvl {
            b"error" => 0,
            b"warn" => 1,
            b"info" => 2,
            b"debug" => 3,
            b"trace" => 4,
            _ => 2,
        };

        // source: message
        let after = if rest.len() > bracket_end + 2 {
            &rest[bracket_end + 2..]
        } else {
            continue;
        };
        if let Some(colon) = after.iter().position(|&b| b == b':') {
            let src = &after[..colon];
            let slen = src.len().min(e.source.len());
            e.source[..slen].copy_from_slice(&src[..slen]);
            e.source_len = slen;
            e.cat = categorize_source(src);
            let msg_start = (colon + 1).min(after.len());
            let msg = after[msg_start..].trim_ascii_start();
            let mlen = msg.len().min(MAX_MSG);
            e.msg[..mlen].copy_from_slice(&msg[..mlen]);
            e.msg_len = mlen;
        } else {
            let mlen = after.len().min(MAX_MSG);
            e.msg[..mlen].copy_from_slice(&after[..mlen]);
            e.msg_len = mlen;
            e.source_len = 0;
            e.cat = CAT_WINLUX;
        }

        ENTRIES[ENTRY_COUNT] = e;
        ENTRY_COUNT += 1;
    }
}

// Format : [CRASH] ts=... sig=... stop=STOP_CODE exception=EX code=0xCODE exe=NAME
unsafe fn parse_crash(data: &[u8]) {
    for line in data.split(|&b| b == b'\n') {
        if !line.starts_with(b"[CRASH]") || ENTRY_COUNT >= MAX_ENTRIES {
            continue;
        }
        let mut e = EMPTY_ENTRY;
        e.level = 5;
        e.cat = CAT_CRASH;
        e.time = *b"--:--:--";

        // Extraire stop= et exe=
        if let Some(pos) = find_field(line, b"stop=") {
            let val = extract_field(&line[pos..]);
            let slen = val.len().min(e.source.len());
            e.source[..slen].copy_from_slice(&val[..slen]);
            e.source_len = slen;
        }
        if let Some(pos) = find_field(line, b"exe=") {
            let val = extract_field(&line[pos..]);
            let prefix = b"Crash: ";
            let plen = prefix.len();
            e.msg[..plen].copy_from_slice(prefix);
            let vlen = val.len().min(MAX_MSG - plen);
            e.msg[plen..plen + vlen].copy_from_slice(&val[..vlen]);
            e.msg_len = plen + vlen;
        } else {
            let default = b"Crash syst\xE8me";
            let dlen = default.len().min(MAX_MSG);
            e.msg[..dlen].copy_from_slice(&default[..dlen]);
            e.msg_len = dlen;
        }

        ENTRIES[ENTRY_COUNT] = e;
        ENTRY_COUNT += 1;
    }
}

fn find_field(data: &[u8], key: &[u8]) -> Option<usize> {
    data.windows(key.len())
        .position(|w| w == key)
        .map(|p| p + key.len())
}

fn extract_field(data: &[u8]) -> &[u8] {
    let end = data
        .iter()
        .position(|&b| b == b' ' || b == b'\n' || b == b'\r')
        .unwrap_or(data.len());
    &data[..end]
}

fn visible_count() -> usize {
    unsafe {
        let mut count = 0;
        for i in 0..ENTRY_COUNT {
            if SELECTED_CAT == CAT_ALL || ENTRIES[i].cat == SELECTED_CAT {
                count += 1;
            }
        }
        count
    }
}

fn visible_index(nth: usize) -> Option<usize> {
    unsafe {
        let mut count = 0;
        for i in (0..ENTRY_COUNT).rev() {
            if SELECTED_CAT == CAT_ALL || ENTRIES[i].cat == SELECTED_CAT {
                if count == nth {
                    return Some(i);
                }
                count += 1;
            }
        }
        None
    }
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();

    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);

    // Barre lat\xE9rale
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.left + SIDEBAR_W,
        r.bottom,
        p.surface,
    );
    fill(
        g.hdc,
        g.b,
        r.left + SIDEBAR_W,
        r.top,
        r.left + SIDEBAR_W + 1,
        r.bottom,
        p.border,
    );

    SetBkMode(g.hdc, 1);
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        r.left + 12,
        r.top + 10,
        b"Cat\xE9gories",
    );

    for i in 0..CAT_NAMES.len() {
        let y = r.top + 36 + i as i32 * 28;
        let bg = if SELECTED_CAT == i {
            p.selected
        } else if HOVER_CAT == i as i32 {
            p.hover
        } else {
            p.surface
        };
        fill(
            g.hdc,
            g.b,
            r.left + 4,
            y,
            r.left + SIDEBAR_W - 4,
            y + 24,
            bg,
        );
        if SELECTED_CAT == i {
            fill(g.hdc, g.b, r.left + 4, y + 4, r.left + 8, y + 20, p.accent);
        }
        text(g.hdc, g.f.body, p.text, r.left + 16, y + 5, CAT_NAMES[i]);

        // Compteur
        let mut cnt = 0u32;
        for j in 0..ENTRY_COUNT {
            if i == CAT_ALL || ENTRIES[j].cat == i {
                cnt += 1;
            }
        }
        let mut nbuf = [0u8; 8];
        let nlen = fmt_u32(cnt, &mut nbuf);
        text(
            g.hdc,
            g.f.small,
            p.text3,
            r.left + SIDEBAR_W - 32,
            y + 6,
            &nbuf[..nlen],
        );
    }

    let cx = r.left + SIDEBAR_W + 1;
    let cw = r.right - cx;

    // Barre d'outils
    fill(g.hdc, g.b, cx, r.top, r.right, r.top + TOOLBAR_H, p.surface);
    fill(
        g.hdc,
        g.b,
        cx,
        r.top + TOOLBAR_H,
        r.right,
        r.top + TOOLBAR_H + 1,
        p.border,
    );

    let btn_names: [&[u8]; 2] = [b"Actualiser", b"Effacer"];
    for (i, name) in btn_names.iter().enumerate() {
        let bx = cx + 8 + i as i32 * 100;
        let by = r.top + 6;
        let bg = if HOVER_BTN == i as i32 {
            p.hover
        } else {
            p.surface
        };
        fill(g.hdc, g.b, bx, by, bx + 90, by + 24, bg);
        frame_rect(g.hdc, g.b, bx, by, bx + 90, by + 24, p.border);
        text(g.hdc, g.f.small, p.text, bx + 8, by + 5, name);
    }

    // En-t\xEAte de colonnes
    let ty = r.top + TOOLBAR_H + 1;
    fill(g.hdc, g.b, cx, ty, r.right, ty + HEADER_H, p.surface2);
    fill(
        g.hdc,
        g.b,
        cx,
        ty + HEADER_H,
        r.right,
        ty + HEADER_H + 1,
        p.border,
    );

    let col_x = [cx + 8, cx + 68, cx + 140, cx + 220];
    let col_names: [&[u8]; 4] = [b"Niveau", b"Heure", b"Source", b"Message"];
    for (i, name) in col_names.iter().enumerate() {
        text(g.hdc, g.f.small, p.text2, col_x[i], ty + 5, name);
    }

    // Liste des entr\xE9es (d\xE9filante, du plus r\xE9cent au plus ancien)
    let list_top = ty + HEADER_H + 1;
    let list_bottom = r.bottom - DETAIL_H;
    let vis = visible_count();
    let max_vis = ((list_bottom - list_top) / ROW_H).max(0) as usize;
    let max_scroll = (vis as i32 - max_vis as i32).max(0);
    if SCROLL > max_scroll {
        SCROLL = max_scroll;
    }

    SetBkMode(g.hdc, 1);
    let mut drawn = 0usize;
    for row_idx in 0..max_vis {
        let entry_vis = SCROLL as usize + row_idx;
        let ei = match visible_index(entry_vis) {
            Some(i) => i,
            None => break,
        };
        let e = &ENTRIES[ei];
        let y = list_top + drawn as i32 * ROW_H;
        if y + ROW_H > list_bottom {
            break;
        }

        let bg = if SELECTED_ROW == row_idx as i32 {
            p.selected
        } else if HOVER_ROW == row_idx as i32 {
            p.hover
        } else if drawn % 2 == 0 {
            p.mica
        } else {
            p.surface2
        };
        fill(g.hdc, g.b, cx, y, r.right, y + ROW_H, bg);

        // Indicateur de niveau (point color\xE9)
        let lc = level_color(e.level);
        fill(g.hdc, g.b, col_x[0], y + 7, col_x[0] + 8, y + 15, lc);
        text(
            g.hdc,
            g.f.small,
            p.text,
            col_x[0] + 12,
            y + 4,
            level_name(e.level),
        );

        // Heure
        text(g.hdc, g.f.small, p.text2, col_x[1], y + 4, &e.time);

        // Source
        if e.source_len > 0 {
            text(
                g.hdc,
                g.f.small,
                p.text,
                col_x[2],
                y + 4,
                &e.source[..e.source_len],
            );
        }

        // Message (tronqu\xE9)
        let msg_w = (cw - (col_x[3] - cx) - 8).max(0);
        let max_chars = (msg_w / 6).max(0) as usize;
        let mlen = e.msg_len.min(max_chars);
        if mlen > 0 {
            text(g.hdc, g.f.small, p.text2, col_x[3], y + 4, &e.msg[..mlen]);
        }

        drawn += 1;
    }

    // Panneau de d\xE9tail en bas
    fill(
        g.hdc,
        g.b,
        cx,
        list_bottom,
        r.right,
        list_bottom + 1,
        p.border,
    );
    fill(
        g.hdc,
        g.b,
        cx,
        list_bottom + 1,
        r.right,
        r.bottom,
        p.surface,
    );
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        cx + 8,
        list_bottom + 6,
        b"D\xE9tails",
    );

    if SELECTED_ROW >= 0 {
        let entry_vis = SCROLL as usize + SELECTED_ROW as usize;
        if let Some(ei) = visible_index(entry_vis) {
            let e = &ENTRIES[ei];
            let dy = list_bottom + 24;
            text(g.hdc, g.f.small, p.text3, cx + 8, dy, b"Niveau:");
            text(g.hdc, g.f.small, p.text, cx + 60, dy, level_name(e.level));
            text(g.hdc, g.f.small, p.text3, cx + 140, dy, b"Heure:");
            text(g.hdc, g.f.small, p.text, cx + 186, dy, &e.time);
            if e.source_len > 0 {
                text(g.hdc, g.f.small, p.text3, cx + 260, dy, b"Source:");
                text(
                    g.hdc,
                    g.f.small,
                    p.text,
                    cx + 310,
                    dy,
                    &e.source[..e.source_len],
                );
            }
            // Message complet sur deux lignes
            let full_msg = &e.msg[..e.msg_len];
            let line1_max = ((cw - 16) / 6).max(0) as usize;
            let l1 = full_msg.len().min(line1_max);
            text(g.hdc, g.f.small, p.text, cx + 8, dy + 18, &full_msg[..l1]);
            if full_msg.len() > l1 {
                let l2 = (full_msg.len() - l1).min(line1_max);
                text(
                    g.hdc,
                    g.f.small,
                    p.text,
                    cx + 8,
                    dy + 34,
                    &full_msg[l1..l1 + l2],
                );
            }
        }
    }
}

fn frame_rect(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    unsafe {
        fill(hdc, b, l, t, r, t + 1, color);
        fill(hdc, b, l, bt - 1, r, bt, color);
        fill(hdc, b, l, t, l + 1, bt, color);
        fill(hdc, b, r - 1, t, r, bt, color);
    }
}

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, f: &Frame) -> bool {
    let r = f.client;
    let p = pal();
    let _ = p;
    let mut dirty = false;

    // Cat\xE9gories
    let old_cat = HOVER_CAT;
    HOVER_CAT = -1;
    for i in 0..CAT_NAMES.len() {
        let y = r.top + 36 + i as i32 * 28;
        if mx >= r.left && mx < r.left + SIDEBAR_W && my >= y && my < y + 24 {
            HOVER_CAT = i as i32;
        }
    }
    if HOVER_CAT != old_cat {
        dirty = true;
    }

    // Boutons
    let cx = r.left + SIDEBAR_W + 1;
    let old_btn = HOVER_BTN;
    HOVER_BTN = -1;
    for i in 0..2 {
        let bx = cx + 8 + i * 100;
        let by = r.top + 6;
        if mx >= bx && mx < bx + 90 && my >= by && my < by + 24 {
            HOVER_BTN = i;
        }
    }
    if HOVER_BTN != old_btn {
        dirty = true;
    }

    // Lignes
    let list_top = r.top + TOOLBAR_H + 1 + HEADER_H + 1;
    let list_bottom = r.bottom - DETAIL_H;
    let old_hover = HOVER_ROW;
    HOVER_ROW = -1;
    if mx > cx && my >= list_top && my < list_bottom {
        HOVER_ROW = (my - list_top) / ROW_H;
    }
    if HOVER_ROW != old_hover {
        dirty = true;
    }

    match ev {
        Mouse::Down => {
            if HOVER_CAT >= 0 {
                SELECTED_CAT = HOVER_CAT as usize;
                SCROLL = 0;
                SELECTED_ROW = -1;
                dirty = true;
            }
            if HOVER_BTN == 0 {
                load_entries();
                dirty = true;
            } else if HOVER_BTN == 1 {
                // Effacer le journal via winlux-journal
                ShellExecuteA(
                    core::ptr::null_mut(),
                    c"open".as_ptr(),
                    c"winlux-journal".as_ptr(),
                    c"clear".as_ptr(),
                    core::ptr::null(),
                    0,
                );
                ENTRY_COUNT = 0;
                SELECTED_ROW = -1;
                dirty = true;
            }
            if HOVER_ROW >= 0 {
                SELECTED_ROW = HOVER_ROW;
                dirty = true;
            }
        }
        Mouse::Wheel(delta) => {
            SCROLL = (SCROLL - delta).max(0);
            let vis = visible_count();
            let list_h = list_bottom - list_top;
            let max_vis = (list_h / ROW_H).max(0) as i32;
            let max_scroll = (vis as i32 - max_vis).max(0);
            if SCROLL > max_scroll {
                SCROLL = max_scroll;
            }
            dirty = true;
        }
        _ => {}
    }

    dirty
}

pub unsafe fn key(vk: u32, _ch: u16) -> bool {
    match vk {
        0x26 => {
            // Up
            if SELECTED_ROW > 0 {
                SELECTED_ROW -= 1;
                return true;
            }
        }
        0x28 => {
            // Down
            let vis = visible_count() as i32;
            if SELECTED_ROW < vis - 1 {
                SELECTED_ROW += 1;
                return true;
            }
        }
        0x74 => {
            // F5
            load_entries();
            return true;
        }
        _ => {}
    }
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let now = GetTickCount();
    if now.wrapping_sub(LAST_REFRESH) > 5000 {
        LAST_REFRESH = now;
        let old_count = ENTRY_COUNT;
        load_entries();
        return ENTRY_COUNT != old_count;
    }
    false
}

fn fmt_u32(mut n: u32, buf: &mut [u8; 8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut i = 8;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let len = 8 - i;
    buf.copy_within(i..8, 0);
    len
}
