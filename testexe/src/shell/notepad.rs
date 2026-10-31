//! Bloc-notes intégré type Notepad++, éditeur texte simple.

use crate::gfx::*;
use crate::wm;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

const MAX_TEXT: usize = 8192;
const MAX_LINES: usize = 512;
const FONT_W: i32 = 8;
const FONT_H: i32 = 16;

#[derive(Clone)]
pub struct Doc {
    pub text: [u8; MAX_TEXT],
    pub len: usize,
    pub lines: [usize; MAX_LINES],
    pub line_count: usize,
    pub cursor: usize,
    pub scroll_y: i32,
    pub modified: bool,
}

pub static mut DOC: Doc = Doc {
    text: [0; MAX_TEXT],
    len: 0,
    lines: [0; MAX_LINES],
    line_count: 1,
    cursor: 0,
    scroll_y: 0,
    modified: false,
};

pub const CUSTOM_TITLE: bool = false;
static mut EDITOR: bool = false;
static mut FOCUSED: bool = true;

fn rebuild_lines() {
    unsafe {
        DOC.lines[0] = 0;
        DOC.line_count = 1;
        let mut i = 0;
        while i < DOC.len && DOC.line_count < MAX_LINES {
            if DOC.text[i] == b'\n' {
                DOC.lines[DOC.line_count] = i + 1;
                DOC.line_count += 1;
            }
            i += 1;
        }
    }
}

fn line_of(pos: usize) -> i32 {
    unsafe {
        let mut l = 0;
        while l < DOC.line_count.saturating_sub(1) && DOC.lines[l + 1] <= pos {
            l += 1;
        }
        l as i32
    }
}

fn col_of(pos: usize) -> i32 {
    unsafe {
        let l = line_of(pos) as usize;
        (pos as i32) - (DOC.lines[l] as i32)
    }
}

pub unsafe fn open_new() {
    DOC.text = [0; MAX_TEXT];
    DOC.len = 0;
    DOC.cursor = 0;
    DOC.scroll_y = 0;
    DOC.modified = false;
    rebuild_lines();
    EDITOR = true;
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        rgb(0x1E, 0x1E, 0x1E),
    );

    // Menu bar
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.top + 24,
        rgb(0x2D, 0x2D, 0x30),
    );
    let items: [&[u8]; 4] = [b"Fichier", b"Edition", b"Affichage", b"Aide"];
    let mut x = r.left + 8;
    for (i, item) in items.iter().enumerate() {
        if i == 0 {
            fill(
                g.hdc,
                g.b,
                x - 4,
                r.top + 2,
                x + text_width(*item, 11) + 4,
                r.top + 22,
                rgb(0x3E, 0x3E, 0x42),
            );
        }
        text(g.hdc, g.f.small, rgb(0xCC, 0xCC, 0xCC), x, r.top + 5, *item);
        x += text_width(*item, 11) + 16;
    }

    // Toolbar
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top + 24,
        r.right,
        r.top + 52,
        rgb(0x2D, 0x2D, 0x30),
    );
    let tools: [&[u8]; 10] = [
        b"Nouveau",
        b"Ouvrir",
        b"Enregistrer",
        b"|",
        b"Couper",
        b"Copier",
        b"Coller",
        b"|",
        b"Annuler",
        b"Refaire",
    ];
    let mut x = r.left + 8;
    for tool in &tools {
        if *tool == b"|" {
            line(
                g.hdc,
                g.b,
                x + 2,
                r.top + 30,
                x + 2,
                r.top + 46,
                1,
                rgb(0x5A, 0x5A, 0x5A),
            );
            x += 8;
        } else {
            round_rect(
                g.hdc,
                g.b,
                x,
                r.top + 28,
                x + text_width(*tool, 10) + 16,
                r.top + 48,
                3,
                rgb(0x3C, 0x3C, 0x3C),
            );
            text(
                g.hdc,
                g.f.small,
                rgb(0xD4, 0xD4, 0xD4),
                x + 8,
                r.top + 33,
                *tool,
            );
            x += text_width(*tool, 10) + 24;
        }
    }

    // Editor area
    let ey = r.top + 52;
    let ex = r.left + 48;
    fill(
        g.hdc,
        g.b,
        r.left,
        ey,
        ex,
        r.bottom - 24,
        rgb(0x1E, 0x1E, 0x1E),
    );
    for i in 0..((r.bottom - 24 - ey) / 16 + 2).max(0) as i32 {
        let num = ((i - DOC.scroll_y / 16) as usize).wrapping_add(1);
        if num <= DOC.line_count {
            let s = format_num(num);
            text(
                g.hdc,
                g.f.small,
                rgb(0x85, 0x85, 0x85),
                ex - 40,
                ey + i * 16 + 2,
                &s,
            );
        }
    }

    // Text
    let mut line_idx = ((-DOC.scroll_y) / 16).max(0) as usize;
    let mut y = ey - DOC.scroll_y % 16;
    while y < r.bottom - 24 && line_idx < DOC.line_count {
        let start = DOC.lines[line_idx];
        let mut end = DOC.len;
        if line_idx + 1 < DOC.line_count {
            end = DOC.lines[line_idx + 1];
            if end > 0 && DOC.text[end - 1] == b'\n' {
                end -= 1;
            }
        }
        if start < end {
            let mut s = [0u8; 512];
            let n = (end - start).min(511);
            for j in 0..n {
                s[j] = DOC.text[start + j];
            }
            s[n] = 0;
            // Simple ASCII text rendering
            let mut tx = ex;
            for j in 0..n {
                let c = DOC.text[start + j];
                if c >= 0x20 && c < 0x7F {
                    draw_char(g, c, tx, y + 2);
                    tx += 8;
                } else if c == b'\t' {
                    tx += 32;
                }
            }
        }
        line_idx += 1;
        y += 16;
        if y >= r.bottom - 24 {
            break;
        }
    }

    // Cursor
    if FOCUSED {
        let cl = line_of(DOC.cursor);
        let cc = col_of(DOC.cursor);
        let cy = ey + cl * 16 - DOC.scroll_y;
        if cy >= ey && cy < r.bottom - 24 {
            fill(
                g.hdc,
                g.b,
                ex + cc * 8,
                cy + 2,
                ex + cc * 8 + 2,
                cy + 18,
                rgb(0xAE, 0xAF, 0xAD),
            );
        }
    }

    // Status bar
    fill(
        g.hdc,
        g.b,
        r.left,
        r.bottom - 24,
        r.right,
        r.bottom,
        rgb(0x00, 0x7A, 0xCC),
    );
    let cl = line_of(DOC.cursor) + 1;
    let cc = col_of(DOC.cursor) + 1;
    let mut status = [0u8; 64];
    let st = b"Ligne ";
    for i in 0..6 {
        status[i] = st[i];
    }
    write_num(&mut status, 6, cl as u32);
    status[12] = b',';
    status[13] = b' ';
    status[14] = b' ';
    status[15] = b'C';
    status[16] = b'o';
    status[17] = b'l';
    status[18] = b' ';
    write_num(&mut status, 19, cc as u32);
    status[25] = b' ';
    status[26] = b' ';
    status[27] = b'U';
    status[28] = b'T';
    status[29] = b'F';
    status[30] = b'-';
    status[31] = b'8';
    status[32] = 0;
    text(
        g.hdc,
        g.f.small,
        rgb(0xFF, 0xFF, 0xFF),
        r.right - 200,
        r.bottom - 19,
        &status,
    );
}

fn format_num(n: usize) -> [u8; 6] {
    let mut s = [b' '; 6];
    let mut v = n;
    let mut i = 4;
    if v == 0 {
        s[4] = b'0';
        return s;
    }
    while v > 0 && i < 6 {
        s[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == 0 {
            break;
        }
        i -= 1;
    }
    // reverse
    let mut r = [b' '; 6];
    let mut j = 0;
    let mut k = 5;
    while k > i {
        r[j] = s[k];
        j += 1;
        k -= 1;
    }
    r[0] = s[i];
    if j == 0 {
        r[1] = s[i + 1];
        r[2] = s[i + 2];
    }
    r
}

fn write_num(buf: &mut [u8; 64], start: usize, mut v: u32) {
    if v == 0 {
        buf[start] = b'0';
        return;
    }
    let mut tmp = [0u8; 12];
    let mut i = 0;
    while v > 0 {
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    let mut j = 0;
    while j < i {
        buf[start + j] = tmp[i - 1 - j];
        j += 1;
    }
}

unsafe fn draw_char(g: &mut Gfx, c: u8, x: i32, y: i32) {
    // Simple 8x16 bitmap-like rendering
    let p = match c {
        b' ' => return,
        b'W' => rgb(0x9C, 0xDC, 0xFF),
        b'S' => rgb(0x9C, 0xDC, 0xFF),
        b'c' | b'r' | b't' | b'f' | b'c' => rgb(0x9C, 0xDC, 0xFF),
        _ => rgb(0xD4, 0xD4, 0xD4),
    };
    // Very simple placeholder - just draw small rects for common chars
    match c {
        b'{' | b'}' | b'[' | b']' | b'(' | b')' => fill(g.hdc, g.b, x + 2, y, x + 6, y + 1, p),
        _ => fill(g.hdc, g.b, x, y + 6, x + 6, y + 7, p),
    }
}

pub unsafe fn mouse(ev: Mouse, _x: i32, _y: i32, f: &Frame) -> bool {
    match ev {
        Mouse::Wheel(delta) => {
            let visible_lines = (f.client.bottom - f.client.top - 52 - 24) / FONT_H;
            DOC.scroll_y -= delta / 120 * FONT_H * 3;
            let max_scroll = ((DOC.line_count as i32 - visible_lines + 1) * FONT_H).max(0);
            DOC.scroll_y = DOC.scroll_y.clamp(0, max_scroll);
            true
        }
        _ => true,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if ch >= 0x20 && ch < 0x7F && DOC.len < MAX_TEXT - 1 {
        if DOC.cursor < DOC.len {
            for i in (DOC.cursor..DOC.len).rev() {
                DOC.text[i + 1] = DOC.text[i];
            }
        }
        DOC.text[DOC.cursor] = ch as u8;
        DOC.len += 1;
        DOC.cursor += 1;
        DOC.modified = true;
        rebuild_lines();
        return true;
    }
    if ch == 8 {
        if DOC.cursor > 0 {
            for i in DOC.cursor..DOC.len {
                DOC.text[i - 1] = DOC.text[i];
            }
            DOC.len -= 1;
            DOC.cursor -= 1;
            DOC.modified = true;
            rebuild_lines();
        }
        return true;
    }
    if ch == 13 || vk == 0x0D {
        if DOC.len < MAX_TEXT - 1 {
            if DOC.cursor < DOC.len {
                for i in (DOC.cursor..DOC.len).rev() {
                    DOC.text[i + 1] = DOC.text[i];
                }
            }
            DOC.text[DOC.cursor] = b'\n';
            DOC.len += 1;
            DOC.cursor += 1;
            DOC.modified = true;
            rebuild_lines();
        }
        return true;
    }
    if vk == 0x25 {
        // left
        if DOC.cursor > 0 {
            DOC.cursor -= 1;
        }
        return true;
    }
    if vk == 0x27 {
        // right
        if DOC.cursor < DOC.len {
            DOC.cursor += 1;
        }
        return true;
    }
    if vk == 0x26 {
        // up
        let l = line_of(DOC.cursor) as i32;
        if l > 0 {
            let c = col_of(DOC.cursor);
            let prev_start = DOC.lines[(l - 1) as usize];
            let prev_end = DOC.lines[l as usize];
            let prev_len = if prev_end > 0 && DOC.text[prev_end - 1] == b'\n' {
                prev_end - prev_start - 1
            } else {
                prev_end - prev_start
            };
            DOC.cursor = prev_start + (c as usize).min(prev_len);
        }
        return true;
    }
    if vk == 0x28 {
        // down
        let l = line_of(DOC.cursor) as i32;
        if l < (DOC.line_count as i32) - 1 {
            let c = col_of(DOC.cursor);
            let next_start = DOC.lines[(l + 1) as usize];
            let next_end = if (l + 2) as usize <= DOC.line_count {
                DOC.lines[(l + 2) as usize]
            } else {
                DOC.len
            };
            let next_len = if next_end > next_start && DOC.text[next_end - 1] == b'\n' {
                next_end - next_start - 1
            } else {
                next_end - next_start
            };
            DOC.cursor = next_start + (c as usize).min(next_len);
        }
        return true;
    }
    if vk == 0x24 {
        // home
        let l = line_of(DOC.cursor) as usize;
        DOC.cursor = DOC.lines[l];
        return true;
    }
    if vk == 0x23 {
        // end
        let l = line_of(DOC.cursor) as usize;
        let end = if l + 1 < DOC.line_count {
            DOC.lines[l + 1]
        } else {
            DOC.len
        };
        DOC.cursor = if end > 0 && DOC.text[end - 1] == b'\n' {
            end - 1
        } else {
            end
        };
        return true;
    }
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    false
}
