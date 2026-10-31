//! Points de restauration syst\xe8me : liste, cr\xe9ation, restauration et
//! suppression de snapshots de la configuration Winlux.  Communique avec
//! le script `winlux-restore` via des fichiers sous /run/winlux/.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const MAX_POINTS: usize = 32;
const NAME_LEN: usize = 64;
const DATE_LEN: usize = 24;
const SIZE_LEN: usize = 16;
const ID_LEN: usize = 32;

#[derive(Clone, Copy)]
struct RestorePoint {
    id: [u8; ID_LEN],
    id_len: usize,
    name: [u8; NAME_LEN],
    name_len: usize,
    date: [u8; DATE_LEN],
    date_len: usize,
    size: [u8; SIZE_LEN],
    size_len: usize,
}

const EMPTY_POINT: RestorePoint = RestorePoint {
    id: [0; ID_LEN],
    id_len: 0,
    name: [0; NAME_LEN],
    name_len: 0,
    date: [0; DATE_LEN],
    date_len: 0,
    size: [0; SIZE_LEN],
    size_len: 0,
};

static mut POINTS: [RestorePoint; MAX_POINTS] = [EMPTY_POINT; MAX_POINTS];
static mut POINT_COUNT: usize = 0;
static mut SELECTED: i32 = -1;
static mut HOVER_ROW: i32 = -1;
static mut HOVER_BTN: i32 = -1;
static mut SCROLL_Y: i32 = 0;
static mut LAST_REFRESH: u32 = 0;

// Dialog state: 0=none, 1=confirm restore, 2=confirm delete, 3=creating, 4=done
static mut DIALOG: u8 = 0;
static mut DIALOG_HOVER: i32 = -1;
static mut STATUS_MSG: [u8; 80] = [0; 80];
static mut STATUS_LEN: usize = 0;
static mut TOTAL_SIZE: [u8; 16] = [0; 16];
static mut TOTAL_SIZE_LEN: usize = 0;

unsafe fn read_file_buf(path: *const i8, buf: &mut [u8]) -> usize {
    let f = CreateFileA(
        path,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    let mut read = 0u32;
    ReadFile(
        f,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
    read as usize
}

unsafe fn refresh() {
    let t = GetTickCount();
    if t.wrapping_sub(LAST_REFRESH) < 2000 && POINT_COUNT > 0 {
        return;
    }
    LAST_REFRESH = t;
    POINT_COUNT = 0;

    let mut fd: Win32FindDataA = core::mem::zeroed();
    let search = FindFirstFileA(c"Z:\\var\\lib\\winlux\\restore\\rp-*".as_ptr(), &mut fd);
    if search.is_null() || search == -1isize as Handle {
        set_status(b"Aucun point de restauration");
        return;
    }
    loop {
        if POINT_COUNT >= MAX_POINTS {
            break;
        }
        let name_bytes = &fd.file_name;
        let mut nlen = 0;
        while nlen < 260 && name_bytes[nlen] != 0 {
            nlen += 1;
        }
        if nlen < 3 {
            if FindNextFileA(search, &mut fd) == 0 {
                break;
            }
            continue;
        }

        let p = &mut POINTS[POINT_COUNT];
        *p = EMPTY_POINT;
        let id_n = nlen.min(ID_LEN - 1);
        for i in 0..id_n {
            p.id[i] = name_bytes[i] as u8;
        }
        p.id_len = id_n;

        // Read metadata files
        let base = b"Z:\\var\\lib\\winlux\\restore\\";
        let mut path_buf = [0u8; 256];
        let base_len = base.len();
        path_buf[..base_len].copy_from_slice(base);
        for i in 0..id_n {
            path_buf[base_len + i] = p.id[i];
        }

        // name
        let suffix = b"\\meta\\name\0";
        let off = base_len + id_n;
        path_buf[off..off + suffix.len()].copy_from_slice(suffix);
        let n = read_file_buf(path_buf.as_ptr().cast(), &mut p.name);
        p.name_len = trim_len(&p.name, n);
        if p.name_len == 0 {
            p.name[..id_n].copy_from_slice(&p.id[..id_n]);
            p.name_len = id_n;
        }

        // date
        let suffix = b"\\meta\\date\0";
        path_buf[off..off + suffix.len()].copy_from_slice(suffix);
        let n = read_file_buf(path_buf.as_ptr().cast(), &mut p.date);
        p.date_len = trim_len(&p.date, n);

        // size
        let suffix = b"\\meta\\size\0";
        path_buf[off..off + suffix.len()].copy_from_slice(suffix);
        let n = read_file_buf(path_buf.as_ptr().cast(), &mut p.size);
        p.size_len = trim_len(&p.size, n);

        POINT_COUNT += 1;
        if FindNextFileA(search, &mut fd) == 0 {
            break;
        }
    }
    FindClose(search);

    let mut msg = [0u8; 80];
    let count_s = fmt_dec(POINT_COUNT as u32);
    let pre = b"Points : ";
    msg[..pre.len()].copy_from_slice(pre);
    let off = pre.len();
    msg[off..off + count_s.1].copy_from_slice(&count_s.0[..count_s.1]);
    STATUS_MSG = msg;
    STATUS_LEN = off + count_s.1;
}

fn trim_len(buf: &[u8], n: usize) -> usize {
    let mut len = n.min(buf.len());
    while len > 0 && (buf[len - 1] == b'\n' || buf[len - 1] == b'\r' || buf[len - 1] == 0) {
        len -= 1;
    }
    len
}

fn fmt_dec(mut v: u32) -> ([u8; 12], usize) {
    let mut buf = [0u8; 12];
    if v == 0 {
        buf[0] = b'0';
        return (buf, 1);
    }
    let mut i = 12;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let mut out = [0u8; 12];
    let len = 12 - i;
    out[..len].copy_from_slice(&buf[i..]);
    (out, len)
}

unsafe fn set_status(msg: &[u8]) {
    let n = msg.len().min(STATUS_MSG.len());
    STATUS_MSG = [0; 80];
    STATUS_MSG[..n].copy_from_slice(&msg[..n]);
    STATUS_LEN = n;
}

unsafe fn run_restore_cmd(args: *const i8) -> bool {
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-restore".as_ptr(),
        args,
        core::ptr::null(),
        0,
    ) as isize
        > 32
}

// ---------- Rendering ----------

const ROW_H: i32 = 36;
const HEADER_H: i32 = 48;
const BTN_H: i32 = 32;
const BTN_W: i32 = 130;
const BTN_GAP: i32 = 8;
const STATUS_H: i32 = 28;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let cl = f.client;
    fill(g.hdc, g.b, cl.left, cl.top, cl.right, cl.bottom, p.window);

    SetBkMode(g.hdc, 1);

    // Header
    SelectObject(g.hdc, g.f.large);
    SetTextColor(g.hdc, p.text);
    text(
        g.hdc,
        g.f.large,
        p.text,
        cl.left + 16,
        cl.top + 12,
        b"Restauration syst\xE8me",
    );

    SelectObject(g.hdc, g.f.small);
    SetTextColor(g.hdc, p.text2);
    text(
        g.hdc,
        g.f.small,
        p.text2,
        cl.left + 16,
        cl.top + 36,
        b"Cr\xE9ez et restaurez des points de sauvegarde de la configuration.",
    );

    // Buttons row
    let btn_y = cl.top + HEADER_H;
    let btns: [&[u8]; 3] = [b"Cr\xE9er un point", b"Restaurer", b"Supprimer"];
    for (i, label) in btns.iter().enumerate() {
        let bx = cl.left + 16 + (BTN_W + BTN_GAP) * i as i32;
        let hover = HOVER_BTN == i as i32;
        let bg = if hover { p.hover } else { p.surface };
        fill(g.hdc, g.b, bx, btn_y, bx + BTN_W, btn_y + BTN_H, bg);
        // border
        let br = rect(bx, btn_y, bx + BTN_W, btn_y + BTN_H);
        let border_brush = g.b.get(p.border);
        FrameRect(g.hdc, &br, border_brush);
        let tc = if i == 0 { p.accent } else { p.text };
        text(g.hdc, g.f.body, tc, bx + 10, btn_y + 8, label);
    }

    // Column headers
    let list_top = btn_y + BTN_H + 10;
    fill(
        g.hdc,
        g.b,
        cl.left,
        list_top,
        cl.right,
        list_top + 24,
        p.surface,
    );
    SelectObject(g.hdc, g.f.semibold);
    SetTextColor(g.hdc, p.text2);
    TextOutA(g.hdc, cl.left + 16, list_top + 4, c"Nom".as_ptr(), -1);
    TextOutA(g.hdc, cl.left + 260, list_top + 4, c"Date".as_ptr(), -1);
    TextOutA(g.hdc, cl.left + 440, list_top + 4, c"Taille".as_ptr(), -1);

    // List
    let list_y = list_top + 24;
    let visible_h = cl.bottom - STATUS_H - list_y;
    let max_visible = (visible_h / ROW_H).max(1);

    SelectObject(g.hdc, g.f.body);
    for i in 0..POINT_COUNT {
        let iy = i as i32;
        let y = list_y + (iy - SCROLL_Y) * ROW_H;
        if y + ROW_H <= list_y || y >= list_y + visible_h {
            continue;
        }
        let selected = SELECTED == iy;
        let hover = HOVER_ROW == iy;
        let bg = if selected {
            p.selected
        } else if hover {
            p.hover
        } else if iy % 2 == 0 {
            p.window
        } else {
            p.surface
        };
        fill(g.hdc, g.b, cl.left, y, cl.right, y + ROW_H, bg);

        let pt = &POINTS[i];
        let tc = if selected { p.accent } else { p.text };
        text(
            g.hdc,
            g.f.body,
            tc,
            cl.left + 16,
            y + 10,
            &pt.name[..pt.name_len],
        );
        text(
            g.hdc,
            g.f.caption,
            p.text2,
            cl.left + 260,
            y + 10,
            &pt.date[..pt.date_len],
        );
        text(
            g.hdc,
            g.f.caption,
            p.text3,
            cl.left + 440,
            y + 10,
            &pt.size[..pt.size_len],
        );
    }

    if POINT_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            cl.left + 16,
            list_y + 16,
            b"Aucun point de restauration disponible.",
        );
    }

    // Status bar
    let sy = cl.bottom - STATUS_H;
    fill(g.hdc, g.b, cl.left, sy, cl.right, cl.bottom, p.surface);
    fill(g.hdc, g.b, cl.left, sy, cl.right, sy + 1, p.border);
    text(
        g.hdc,
        g.f.small,
        p.text2,
        cl.left + 12,
        sy + 7,
        &STATUS_MSG[..STATUS_LEN],
    );

    // Dialog overlay
    if DIALOG > 0 {
        draw_dialog(g, &cl);
    }
}

unsafe fn draw_dialog(g: &mut Gfx, cl: &Rect) {
    let p = pal();
    // Semi-transparent overlay via a dark fill
    fill(g.hdc, g.b, cl.left, cl.top, cl.right, cl.bottom, p.shadow);

    let dw = 360;
    let dh = 160;
    let dx = (cl.left + cl.right - dw) / 2;
    let dy = (cl.top + cl.bottom - dh) / 2;
    fill(g.hdc, g.b, dx, dy, dx + dw, dy + dh, p.window);
    let br = rect(dx, dy, dx + dw, dy + dh);
    let border_brush = g.b.get(p.border);
    FrameRect(g.hdc, &br, border_brush);

    let msg: &[u8] = match DIALOG {
        1 => b"Restaurer ce point de sauvegarde ?",
        2 => b"Supprimer ce point de sauvegarde ?",
        3 => b"Op\xE9ration en cours...",
        4 => b"Op\xE9ration termin\xE9e.",
        _ => b"",
    };
    text(g.hdc, g.f.semibold, p.text, dx + 24, dy + 24, msg);

    if DIALOG == 1 || DIALOG == 2 {
        let sub: &[u8] = if DIALOG == 1 {
            b"La configuration sera restaur\xE9e \xE0 l'\xE9tat sauvegard\xE9."
        } else {
            b"Cette action est irr\xE9versible."
        };
        text(g.hdc, g.f.body, p.text2, dx + 24, dy + 52, sub);

        // Confirm / Cancel buttons
        let by = dy + dh - 48;
        let confirm_bg = if DIALOG_HOVER == 0 {
            p.hover
        } else {
            p.surface
        };
        let cancel_bg = if DIALOG_HOVER == 1 {
            p.hover
        } else {
            p.surface
        };
        fill(g.hdc, g.b, dx + 24, by, dx + 144, by + BTN_H, confirm_bg);
        fill(g.hdc, g.b, dx + 160, by, dx + 280, by + BTN_H, cancel_bg);
        let cb = rect(dx + 24, by, dx + 144, by + BTN_H);
        let ab = rect(dx + 160, by, dx + 280, by + BTN_H);
        FrameRect(g.hdc, &cb, border_brush);
        FrameRect(g.hdc, &ab, border_brush);
        text(g.hdc, g.f.body, p.accent, dx + 44, by + 8, b"Confirmer");
        text(g.hdc, g.f.body, p.text, dx + 190, by + 8, b"Annuler");
    } else if DIALOG == 4 {
        let by = dy + dh - 48;
        let ok_bg = if DIALOG_HOVER == 0 {
            p.hover
        } else {
            p.surface
        };
        fill(g.hdc, g.b, dx + 130, by, dx + 230, by + BTN_H, ok_bg);
        let ob = rect(dx + 130, by, dx + 230, by + BTN_H);
        FrameRect(g.hdc, &ob, border_brush);
        text(g.hdc, g.f.body, p.accent, dx + 170, by + 8, b"OK");
    }
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let cl = f.client;
    let btn_y = cl.top + HEADER_H;
    let list_top = btn_y + BTN_H + 10 + 24;
    let visible_h = cl.bottom - STATUS_H - list_top;

    // Dialog interaction
    if DIALOG > 0 {
        return dialog_mouse(ev, x, y, cl);
    }

    let mut changed = false;

    // Button hover
    let old_hover = HOVER_BTN;
    HOVER_BTN = -1;
    if y >= btn_y && y < btn_y + BTN_H {
        for i in 0..3 {
            let bx = cl.left + 16 + (BTN_W + BTN_GAP) * i;
            if x >= bx && x < bx + BTN_W {
                HOVER_BTN = i;
            }
        }
    }
    if HOVER_BTN != old_hover {
        changed = true;
    }

    // Row hover
    let old_row = HOVER_ROW;
    HOVER_ROW = -1;
    if y >= list_top && y < list_top + visible_h && x >= cl.left && x < cl.right {
        let row = (y - list_top) / ROW_H + SCROLL_Y;
        if row >= 0 && (row as usize) < POINT_COUNT {
            HOVER_ROW = row;
        }
    }
    if HOVER_ROW != old_row {
        changed = true;
    }

    match ev {
        Mouse::Down => {
            if HOVER_ROW >= 0 {
                SELECTED = HOVER_ROW;
                changed = true;
            }
            if HOVER_BTN == 0 {
                // Create restore point
                if run_restore_cmd(c"create".as_ptr()) {
                    set_status(b"Point de restauration en cours de cr\xE9ation...");
                    LAST_REFRESH = 0;
                }
                changed = true;
            } else if HOVER_BTN == 1 && SELECTED >= 0 {
                DIALOG = 1;
                DIALOG_HOVER = -1;
                changed = true;
            } else if HOVER_BTN == 2 && SELECTED >= 0 {
                DIALOG = 2;
                DIALOG_HOVER = -1;
                changed = true;
            }
        }
        Mouse::Wheel(delta) => {
            let max_scroll = (POINT_COUNT as i32 - visible_h / ROW_H).max(0);
            SCROLL_Y = (SCROLL_Y - delta).clamp(0, max_scroll);
            changed = true;
        }
        _ => {}
    }
    changed
}

unsafe fn dialog_mouse(ev: Mouse, x: i32, y: i32, cl: Rect) -> bool {
    let dw = 360;
    let dh = 160;
    let dx = (cl.left + cl.right - dw) / 2;
    let dy = (cl.top + cl.bottom - dh) / 2;
    let by = dy + dh - 48;
    let mut changed = false;

    let old_hover = DIALOG_HOVER;
    DIALOG_HOVER = -1;

    if DIALOG == 1 || DIALOG == 2 {
        if y >= by && y < by + BTN_H {
            if x >= dx + 24 && x < dx + 144 {
                DIALOG_HOVER = 0;
            } else if x >= dx + 160 && x < dx + 280 {
                DIALOG_HOVER = 1;
            }
        }
    } else if DIALOG == 4 {
        if y >= by && y < by + BTN_H && x >= dx + 130 && x < dx + 230 {
            DIALOG_HOVER = 0;
        }
    }

    if DIALOG_HOVER != old_hover {
        changed = true;
    }

    if ev == Mouse::Down {
        if DIALOG == 1 && DIALOG_HOVER == 0 {
            // Confirm restore
            let sel = SELECTED as usize;
            if sel < POINT_COUNT {
                let mut cmd = [0u8; 128];
                let prefix = b"restore ";
                cmd[..prefix.len()].copy_from_slice(prefix);
                let id = &POINTS[sel].id[..POINTS[sel].id_len];
                cmd[prefix.len()..prefix.len() + id.len()].copy_from_slice(id);
                cmd[prefix.len() + id.len()] = 0;
                run_restore_cmd(cmd.as_ptr().cast());
                DIALOG = 3;
                set_status(b"Restauration en cours...");
            }
            changed = true;
        } else if DIALOG == 2 && DIALOG_HOVER == 0 {
            // Confirm delete
            let sel = SELECTED as usize;
            if sel < POINT_COUNT {
                let mut cmd = [0u8; 128];
                let prefix = b"delete ";
                cmd[..prefix.len()].copy_from_slice(prefix);
                let id = &POINTS[sel].id[..POINTS[sel].id_len];
                cmd[prefix.len()..prefix.len() + id.len()].copy_from_slice(id);
                cmd[prefix.len() + id.len()] = 0;
                run_restore_cmd(cmd.as_ptr().cast());
                SELECTED = -1;
                LAST_REFRESH = 0;
                DIALOG = 0;
                set_status(b"Point supprim\xE9.");
            }
            changed = true;
        } else if DIALOG_HOVER == 1 {
            // Cancel
            DIALOG = 0;
            changed = true;
        } else if DIALOG == 4 && DIALOG_HOVER == 0 {
            DIALOG = 0;
            LAST_REFRESH = 0;
            changed = true;
        }
    }

    changed
}

pub unsafe fn key(vk: u32, _ch: u16) -> bool {
    if DIALOG > 0 {
        if vk == 0x1B {
            // Escape
            DIALOG = 0;
            return true;
        }
        if vk == 0x0D && (DIALOG == 1 || DIALOG == 2) {
            // Enter = confirm
            DIALOG_HOVER = 0;
            return dialog_mouse(Mouse::Down, 0, 0, rect(0, 0, 800, 600));
        }
        return false;
    }

    match vk {
        0x26 => {
            // Up
            if SELECTED > 0 {
                SELECTED -= 1;
            }
            true
        }
        0x28 => {
            // Down
            if SELECTED < POINT_COUNT as i32 - 1 {
                SELECTED += 1;
            }
            true
        }
        0x2E => {
            // Delete
            if SELECTED >= 0 {
                DIALOG = 2;
                DIALOG_HOVER = -1;
            }
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let old_count = POINT_COUNT;
    refresh();
    if DIALOG == 3 {
        DIALOG = 4;
        set_status(b"Restauration termin\xE9e.");
        return true;
    }
    POINT_COUNT != old_count
}
