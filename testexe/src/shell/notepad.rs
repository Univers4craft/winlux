//! Éditeur de texte avancé avec coloration syntaxique, onglets multiples,
//! numéros de ligne et rechercher/remplacer. Style Notepad++ simplifié.

use crate::gfx::*;
use crate::wm;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

const MAX_TEXT: usize = 32768;
const MAX_LINES: usize = 2048;
const MAX_TABS: usize = 8;
const MAX_NAME: usize = 48;
const GUTTER_W: i32 = 52;
const TAB_H: i32 = 28;
const MENU_H: i32 = 24;
const FIND_H: i32 = 32;
const STATUS_H: i32 = 22;
const CHAR_H: i32 = 15;

// Syntax highlighting colors (editor-specific, not theme chrome)
const TEXT_KEYWORD: u32 = 0x00C586C0; // purple-pink
const TEXT_TYPE: u32 = 0x004EC9B0; // teal
const TEXT_STRING: u32 = 0x006AC3CE; // light teal (escaped repr)
const TEXT_COMMENT: u32 = 0x006A9955; // green
const TEXT_NUMBER: u32 = 0x00B5CEA8; // light green
const TEXT_PREPROC: u32 = 0x009CDCFE; // light blue

static mut HOVER_TAB: i32 = -1;

static mut MONO_FONT: Handle = core::ptr::null_mut();

unsafe fn mono() -> Handle {
    if MONO_FONT.is_null() {
        MONO_FONT = CreateFontA(
            CHAR_H,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            1, // FIXED_PITCH
            c"DejaVu Sans Mono".as_ptr(),
        );
    }
    MONO_FONT
}

fn char_w() -> i32 {
    text_width(b"M", CHAR_H)
}

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Plain,
    Rust,
    C,
    Python,
    Js,
    Shell,
}

struct Doc {
    text: [u8; MAX_TEXT],
    len: usize,
    lines: [usize; MAX_LINES],
    line_count: usize,
    cursor: usize,
    anchor: usize, // selection anchor (== cursor when no selection)
    scroll_y: i32,
    scroll_x: i32,
    modified: bool,
    name: [u8; MAX_NAME],
    name_len: usize,
    lang: Lang,
}

impl Doc {
    const fn new() -> Self {
        Doc {
            text: [0; MAX_TEXT],
            len: 0,
            lines: [0; MAX_LINES],
            line_count: 1,
            cursor: 0,
            anchor: 0,
            scroll_y: 0,
            scroll_x: 0,
            modified: false,
            name: [0; MAX_NAME],
            name_len: 0,
            lang: Lang::Plain,
        }
    }
}

struct State {
    tabs: [Doc; MAX_TABS],
    tab_count: usize,
    active: usize,
    // find/replace bar
    find_open: bool,
    replace_open: bool,
    find_buf: [u8; 128],
    find_len: usize,
    replace_buf: [u8; 128],
    replace_len: usize,
    find_focused: bool, // true = find field, false = replace field
    find_matches: usize,
    // new tab counter
    counter: usize,
    // cursor blink
    blink_on: bool,
    blink_tick: u32,
}

static mut ST: State = State {
    tabs: [const { Doc::new() }; MAX_TABS],
    tab_count: 0,
    active: 0,
    find_open: false,
    replace_open: false,
    find_buf: [0; 128],
    find_len: 0,
    replace_buf: [0; 128],
    replace_len: 0,
    find_focused: true,
    find_matches: 0,
    counter: 0,
    blink_on: true,
    blink_tick: 0,
};

pub const CUSTOM_TITLE: bool = true;

fn doc() -> &'static mut Doc {
    unsafe { &mut ST.tabs[ST.active] }
}

fn rebuild_lines(d: &mut Doc) {
    d.lines[0] = 0;
    d.line_count = 1;
    let mut i = 0;
    while i < d.len && d.line_count < MAX_LINES {
        if d.text[i] == b'\n' {
            d.lines[d.line_count] = i + 1;
            d.line_count += 1;
        }
        i += 1;
    }
}

fn line_of(d: &Doc, pos: usize) -> usize {
    let mut l = 0;
    while l + 1 < d.line_count && d.lines[l + 1] <= pos {
        l += 1;
    }
    l
}

fn col_of(d: &Doc, pos: usize) -> usize {
    let l = line_of(d, pos);
    pos - d.lines[l]
}

fn line_end(d: &Doc, l: usize) -> usize {
    if l + 1 < d.line_count {
        let e = d.lines[l + 1];
        if e > 0 && d.text[e - 1] == b'\n' {
            e - 1
        } else {
            e
        }
    } else {
        d.len
    }
}

fn sel_range(d: &Doc) -> (usize, usize) {
    if d.cursor <= d.anchor {
        (d.cursor, d.anchor)
    } else {
        (d.anchor, d.cursor)
    }
}

fn has_sel(d: &Doc) -> bool {
    d.cursor != d.anchor
}

fn delete_sel(d: &mut Doc) {
    if !has_sel(d) {
        return;
    }
    let (a, b) = sel_range(d);
    let gap = b - a;
    let mut i = a;
    while i + gap < d.len {
        d.text[i] = d.text[i + gap];
        i += 1;
    }
    d.len -= gap;
    d.cursor = a;
    d.anchor = a;
    d.modified = true;
    rebuild_lines(d);
}

/// Insère un bloc de texte en une fois (collage).
fn insert_bytes(d: &mut Doc, bytes: &[u8]) {
    delete_sel(d);
    let room = (MAX_TEXT - 1).saturating_sub(d.len);
    let mut clean = [0u8; 4096];
    let mut n = 0;
    for &c in bytes {
        if n == clean.len() || n == room {
            break;
        }
        if c == b'\r' {
            continue;
        }
        clean[n] = c;
        n += 1;
    }
    if n == 0 {
        return;
    }
    d.text.copy_within(d.cursor..d.len, d.cursor + n);
    d.text[d.cursor..d.cursor + n].copy_from_slice(&clean[..n]);
    d.len += n;
    d.cursor += n;
    d.anchor = d.cursor;
    d.modified = true;
    rebuild_lines(d);
}

unsafe fn copy_selection(cut: bool) {
    let d = doc();
    if !has_sel(d) {
        return;
    }
    let (a, b) = sel_range(d);
    let mut buf = [0u8; 8192];
    let mut n = 0;
    for &c in &d.text[a..b] {
        // Fins de ligne Windows dans le presse-papiers.
        if c == b'\n' && n + 2 <= buf.len() {
            buf[n] = b'\r';
            buf[n + 1] = b'\n';
            n += 2;
        } else if n < buf.len() {
            buf[n] = c;
            n += 1;
        }
    }
    crate::cliphist::set_text(&buf[..n]);
    if cut {
        delete_sel(d);
    }
}

/// Colle le texte du presse-papiers au curseur (Ctrl+V et historique Win+V).
pub unsafe fn paste_from_clipboard() {
    let mut buf = [0u8; 4096];
    let n = crate::cliphist::get_text(&mut buf);
    if n > 0 {
        insert_bytes(doc(), &buf[..n]);
    }
}

fn insert_char(d: &mut Doc, c: u8) {
    delete_sel(d);
    if d.len >= MAX_TEXT - 1 {
        return;
    }
    let mut i = d.len;
    while i > d.cursor {
        d.text[i] = d.text[i - 1];
        i -= 1;
    }
    d.text[d.cursor] = c;
    d.len += 1;
    d.cursor += 1;
    d.anchor = d.cursor;
    d.modified = true;
    rebuild_lines(d);
}

fn detect_lang(name: &[u8], len: usize) -> Lang {
    if len < 2 {
        return Lang::Plain;
    }
    let mut dot = len;
    let mut i = len;
    while i > 0 {
        i -= 1;
        if name[i] == b'.' {
            dot = i;
            break;
        }
    }
    if dot >= len {
        return Lang::Plain;
    }
    let ext = &name[dot + 1..len];
    if ext == b"rs" {
        return Lang::Rust;
    }
    if ext == b"c" || ext == b"h" || ext == b"cpp" || ext == b"hpp" || ext == b"cc" {
        return Lang::C;
    }
    if ext == b"py" {
        return Lang::Python;
    }
    if ext == b"js" || ext == b"ts" || ext == b"jsx" || ext == b"tsx" || ext == b"json" {
        return Lang::Js;
    }
    if ext == b"sh" || ext == b"bash" {
        return Lang::Shell;
    }
    Lang::Plain
}

pub unsafe fn load_into_active(data: &[u8], name: &[u8]) {
    let d = doc();
    d.text = [0; MAX_TEXT];
    d.len = 0;
    d.cursor = 0;
    d.anchor = 0;
    d.scroll_y = 0;
    d.scroll_x = 0;
    d.modified = false;
    let mut i = 0;
    while i < data.len() && d.len < MAX_TEXT - 1 {
        if data[i] == b'\r' {
            i += 1;
            continue;
        }
        d.text[d.len] = data[i];
        d.len += 1;
        i += 1;
    }
    d.cursor = d.len;
    let n = name.len().min(MAX_NAME);
    for j in 0..n {
        d.name[j] = name[j];
    }
    d.name_len = n;
    d.lang = detect_lang(&d.name, d.name_len);
    rebuild_lines(d);
}

pub unsafe fn open_new() {
    if ST.tab_count >= MAX_TABS {
        return;
    }
    let idx = ST.tab_count;
    ST.tabs[idx] = Doc::new();
    ST.counter += 1;
    let prefix = tr4(&[
        b"Sans titre ",
        b"Untitled ",
        b"Sin t\xEDtulo ",
        b"Unbenannt ",
    ]);
    let mut n = 0;
    while n < prefix.len() && n < MAX_NAME {
        ST.tabs[idx].name[n] = prefix[n];
        n += 1;
    }
    let mut v = ST.counter;
    let mut digits = [0u8; 6];
    let mut dc = 0;
    if v == 0 {
        digits[0] = b'0';
        dc = 1;
    } else {
        while v > 0 && dc < 6 {
            digits[dc] = b'0' + (v % 10) as u8;
            v /= 10;
            dc += 1;
        }
    }
    let mut j = 0;
    while j < dc && n < MAX_NAME {
        ST.tabs[idx].name[n] = digits[dc - 1 - j];
        n += 1;
        j += 1;
    }
    ST.tabs[idx].name_len = n;
    ST.tab_count += 1;
    ST.active = idx;
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let cw = char_w();
    let d = &ST.tabs[ST.active];
    let r = f.client;
    let tr = f.title;
    let p = pal();

    // === Tab bar (in title area) ===
    fill(g.hdc, g.b, tr.left, tr.top, tr.right, tr.bottom, p.surface);
    let mut tx = tr.left + 4;
    for i in 0..ST.tab_count {
        let t = &ST.tabs[i];
        let name = &t.name[..t.name_len];
        let tw = text_width(name, 11) + 24;
        let bg = if i == ST.active {
            p.window
        } else if HOVER_TAB == i as i32 {
            p.hover
        } else {
            p.surface
        };
        let fg = if i == ST.active { p.text } else { p.text3 };
        fill(g.hdc, g.b, tx, tr.top, tx + tw, tr.bottom, bg);
        if i == ST.active {
            fill(g.hdc, g.b, tx, tr.bottom - 2, tx + tw, tr.bottom, p.accent);
        }
        let label_y = tr.top + (tr.bottom - tr.top - 11) / 2;
        if t.modified {
            text(g.hdc, g.f.small, fg, tx + 6, label_y, b"\xB7 ");
            text(g.hdc, g.f.small, fg, tx + 16, label_y, name);
        } else {
            text(g.hdc, g.f.small, fg, tx + 8, label_y, name);
        }
        // Close button
        let cx = tx + tw - 16;
        let cy = tr.top + (tr.bottom - tr.top) / 2;
        text(g.hdc, g.f.small, p.text3, cx - 2, cy - 6, b"x");
        tx += tw + 2;
    }
    // "+" button
    if ST.tab_count < MAX_TABS {
        round_rect(
            g.hdc,
            g.b,
            tx + 2,
            tr.top + 6,
            tx + 22,
            tr.bottom - 6,
            4,
            p.surface2,
        );
        text(g.hdc, g.f.small, p.text3, tx + 7, tr.top + 8, b"+");
    }

    // === Menu bar ===
    let my = r.top;
    fill(g.hdc, g.b, r.left, my, r.right, my + MENU_H, p.surface);
    let menus: [&[u8]; 4] = [
        tr4(&[b"Fichier", b"File", b"Archivo", b"Datei"]),
        tr4(&[b"\xC9dition", b"Edit", b"Edici\xF3n", b"Bearbeiten"]),
        tr4(&[b"Affichage", b"View", b"Ver", b"Ansicht"]),
        tr4(&[b"Aide", b"Help", b"Ayuda", b"Hilfe"]),
    ];
    let mut mx = r.left + 8;
    for item in &menus {
        text(g.hdc, g.f.small, p.text2, mx, my + 6, *item);
        mx += text_width(*item, 11) + 16;
    }

    // === Find/Replace bar ===
    let find_y = my + MENU_H;
    let ey = if ST.find_open {
        let fh = if ST.replace_open { FIND_H * 2 } else { FIND_H };
        fill(g.hdc, g.b, r.left, find_y, r.right, find_y + fh, p.surface2);
        // Find field
        let field_x = r.left + 80;
        let field_w = (r.right - r.left - 160).min(400).max(120);
        round_rect(
            g.hdc,
            g.b,
            field_x,
            find_y + 4,
            field_x + field_w,
            find_y + FIND_H - 4,
            4,
            p.surface,
        );
        text(
            g.hdc,
            g.f.small,
            p.text2,
            r.left + 8,
            find_y + 9,
            tr4(&[b"Chercher:", b"Find:", b"Buscar:", b"Suchen:"]),
        );
        text(
            g.hdc,
            mono(),
            p.text,
            field_x + 4,
            find_y + 8,
            &ST.find_buf[..ST.find_len],
        );
        // Cursor in find field
        if ST.find_focused {
            let fcx = field_x + 4 + text_width(&ST.find_buf[..ST.find_len], CHAR_H);
            fill(
                g.hdc,
                g.b,
                fcx,
                find_y + 7,
                fcx + 1,
                find_y + FIND_H - 7,
                p.text,
            );
        }
        // Match count
        let mut mbuf = [0u8; 32];
        let mc = format_usize(ST.find_matches, &mut mbuf);
        text(
            g.hdc,
            g.f.small,
            p.text3,
            field_x + field_w + 8,
            find_y + 9,
            &mbuf[..mc],
        );
        // Close button
        text(g.hdc, g.f.small, p.text3, r.right - 24, find_y + 9, b"x");

        if ST.replace_open {
            let ry = find_y + FIND_H;
            round_rect(
                g.hdc,
                g.b,
                field_x,
                ry + 4,
                field_x + field_w,
                ry + FIND_H - 4,
                4,
                p.surface,
            );
            text(
                g.hdc,
                g.f.small,
                p.text2,
                r.left + 8,
                ry + 9,
                tr4(&[b"Remplacer:", b"Replace:", b"Reemplazar:", b"Ersetzen:"]),
            );
            text(
                g.hdc,
                mono(),
                p.text,
                field_x + 4,
                ry + 8,
                &ST.replace_buf[..ST.replace_len],
            );
            if !ST.find_focused {
                let rcx = field_x + 4 + text_width(&ST.replace_buf[..ST.replace_len], CHAR_H);
                fill(g.hdc, g.b, rcx, ry + 7, rcx + 1, ry + FIND_H - 7, p.text);
            }
            // "Remplacer" button
            let bx = field_x + field_w + 8;
            round_rect(
                g.hdc,
                g.b,
                bx,
                ry + 4,
                bx + 72,
                ry + FIND_H - 4,
                4,
                p.surface,
            );
            text(
                g.hdc,
                g.f.small,
                p.text2,
                bx + 6,
                ry + 9,
                tr4(&[b"Rempl.", b"Repl.", b"Reempl.", b"Ersetz."]),
            );
            // "Tout remplacer" button
            round_rect(
                g.hdc,
                g.b,
                bx + 78,
                ry + 4,
                bx + 150,
                ry + FIND_H - 4,
                4,
                p.surface,
            );
            text(
                g.hdc,
                g.f.small,
                p.text2,
                bx + 84,
                ry + 9,
                tr4(&[b"Tout", b"All", b"Todo", b"Alle"]),
            );
        }

        find_y + fh
    } else {
        find_y
    };

    // === Editor background ===
    fill(
        g.hdc,
        g.b,
        r.left,
        ey,
        r.right,
        r.bottom - STATUS_H,
        p.window,
    );

    // === Line number gutter ===
    fill(
        g.hdc,
        g.b,
        r.left,
        ey,
        r.left + GUTTER_W,
        r.bottom - STATUS_H,
        p.surface,
    );
    line(
        g.hdc,
        g.b,
        r.left + GUTTER_W,
        ey,
        r.left + GUTTER_W,
        r.bottom - STATUS_H,
        1,
        p.border,
    );

    let editor_x = r.left + GUTTER_W + 4;
    let editor_y = ey;
    let editor_h = r.bottom - STATUS_H - ey;
    let visible_lines = (editor_h / CHAR_H).max(1) as usize;

    // Count find matches
    if ST.find_open && ST.find_len > 0 {
        ST.find_matches = count_matches(d);
    } else {
        ST.find_matches = 0;
    }

    // === Draw lines with syntax highlighting ===
    let first_line = (d.scroll_y / CHAR_H).max(0) as usize;
    let (sel_a, sel_b) = sel_range(d);
    let mut y = editor_y - (d.scroll_y % CHAR_H);
    let mut li = first_line;

    while li < d.line_count && y < r.bottom - STATUS_H {
        // Line number
        let mut nbuf = [0u8; 32];
        let nc = format_usize(li + 1, &mut nbuf);
        let nw = text_width(&nbuf[..nc], CHAR_H);
        text(
            g.hdc,
            mono(),
            p.text3,
            r.left + GUTTER_W - 4 - nw,
            y,
            &nbuf[..nc],
        );

        let ls = d.lines[li];
        let le = line_end(d, li);

        // Selection highlight for this line
        if sel_a != sel_b {
            let line_sa = if sel_a < ls {
                0
            } else if sel_a > le {
                le - ls
            } else {
                sel_a - ls
            };
            let line_sb = if sel_b < ls {
                0
            } else if sel_b > le {
                le - ls
            } else {
                sel_b - ls
            };
            if line_sa < line_sb {
                let sx = editor_x + (line_sa as i32) * cw - d.scroll_x;
                let ex = editor_x + (line_sb as i32) * cw - d.scroll_x;
                fill(g.hdc, g.b, sx, y, ex, y + CHAR_H, p.selected);
            }
            // If selection spans past this line end, highlight to edge
            if sel_a <= le && sel_b > le && li + 1 < d.line_count {
                let sx = editor_x + ((le - ls) as i32) * cw - d.scroll_x;
                fill(g.hdc, g.b, sx, y, sx + cw, y + CHAR_H, p.selected);
            }
        }

        // Draw characters with syntax coloring
        if ls < le {
            draw_line_highlighted(g, d, ls, le, editor_x - d.scroll_x, y);
        }

        // Find match highlight
        if ST.find_open && ST.find_len > 0 && ls < le {
            highlight_find_matches(g, d, ls, le, editor_x - d.scroll_x, y, cw);
        }

        li += 1;
        y += CHAR_H;
    }

    // === Cursor ===
    if ST.blink_on {
        let cl = line_of(d, d.cursor);
        let cc = col_of(d, d.cursor) as i32;
        let cy = editor_y + (cl as i32) * CHAR_H - d.scroll_y;
        if cy >= editor_y && cy < r.bottom - STATUS_H {
            let cx = editor_x + cc * cw - d.scroll_x;
            fill(
                g.hdc,
                g.b,
                cx,
                cy,
                cx + crate::gfx::caret_w(2),
                cy + CHAR_H,
                p.text2,
            );
        }
    }

    // === Status bar ===
    fill(
        g.hdc,
        g.b,
        r.left,
        r.bottom - STATUS_H,
        r.right,
        r.bottom,
        p.accent,
    );
    let cl = line_of(d, d.cursor) + 1;
    let cc = col_of(d, d.cursor) + 1;
    let mut sbuf = [0u8; 80];
    let sn = format_status(&mut sbuf, cl, cc, d.lang);
    text(
        g.hdc,
        g.f.small,
        p.on_accent,
        r.right - 280,
        r.bottom - STATUS_H + 5,
        &sbuf[..sn],
    );
    // Left side: tab name
    text(
        g.hdc,
        g.f.small,
        p.on_accent,
        r.left + 8,
        r.bottom - STATUS_H + 5,
        &d.name[..d.name_len],
    );
}

unsafe fn draw_line_highlighted(g: &mut Gfx, d: &Doc, start: usize, end: usize, x0: i32, y: i32) {
    let cw = char_w();
    let lang = d.lang;
    let mut i = start;
    let mut cx = x0;

    while i < end {
        // Determine token color
        let (color, skip) = token_color(d, i, end, lang);

        // Draw characters in this token
        let tok_end = i + skip;
        while i < tok_end && i < end {
            let ch = d.text[i];
            if ch == b'\t' {
                cx += cw * 4;
            } else if ch >= 0x20 {
                let buf = [ch];
                text(g.hdc, mono(), color, cx, y, &buf);
                cx += cw;
            }
            i += 1;
        }
    }
}

fn token_color(d: &Doc, pos: usize, line_end: usize, lang: Lang) -> (u32, usize) {
    let text_default = unsafe { pal().text };
    if lang == Lang::Plain {
        return (text_default, 1);
    }

    let c = d.text[pos];

    // Line comment
    if pos + 1 < line_end && d.text[pos] == b'/' && d.text[pos + 1] == b'/' {
        return (TEXT_COMMENT, line_end - pos);
    }
    // Python/shell comment
    if (lang == Lang::Python || lang == Lang::Shell) && c == b'#' {
        return (TEXT_COMMENT, line_end - pos);
    }

    // String literals
    if c == b'"' || c == b'\'' {
        let q = c;
        let mut i = pos + 1;
        while i < line_end {
            if d.text[i] == b'\\' && i + 1 < line_end {
                i += 2;
                continue;
            }
            if d.text[i] == q {
                return (TEXT_STRING, i - pos + 1);
            }
            i += 1;
        }
        return (TEXT_STRING, line_end - pos);
    }

    // Numbers
    if c >= b'0' && c <= b'9' {
        let mut i = pos + 1;
        while i < line_end
            && (d.text[i].is_ascii_alphanumeric() || d.text[i] == b'.' || d.text[i] == b'_')
        {
            i += 1;
        }
        return (TEXT_NUMBER, i - pos);
    }

    // Preprocessor (C)
    if lang == Lang::C && c == b'#' {
        return (TEXT_PREPROC, line_end - pos);
    }

    // Keywords / identifiers
    if c.is_ascii_alphabetic() || c == b'_' {
        let mut i = pos + 1;
        while i < line_end && (d.text[i].is_ascii_alphanumeric() || d.text[i] == b'_') {
            i += 1;
        }
        let word = &d.text[pos..i];
        let color = classify_word(word, lang);
        return (color, i - pos);
    }

    (text_default, 1)
}

fn classify_word(w: &[u8], lang: Lang) -> u32 {
    let text_default = unsafe { pal().text };
    match lang {
        Lang::Rust => {
            if matches!(
                w,
                b"fn"
                    | b"let"
                    | b"mut"
                    | b"if"
                    | b"else"
                    | b"match"
                    | b"for"
                    | b"while"
                    | b"loop"
                    | b"return"
                    | b"break"
                    | b"continue"
                    | b"pub"
                    | b"use"
                    | b"mod"
                    | b"struct"
                    | b"enum"
                    | b"impl"
                    | b"trait"
                    | b"const"
                    | b"static"
                    | b"unsafe"
                    | b"where"
                    | b"as"
                    | b"in"
                    | b"ref"
                    | b"self"
                    | b"Self"
                    | b"super"
                    | b"crate"
                    | b"type"
                    | b"async"
                    | b"await"
                    | b"move"
                    | b"dyn"
                    | b"extern"
            ) {
                TEXT_KEYWORD
            } else if matches!(
                w,
                b"u8"
                    | b"u16"
                    | b"u32"
                    | b"u64"
                    | b"u128"
                    | b"usize"
                    | b"i8"
                    | b"i16"
                    | b"i32"
                    | b"i64"
                    | b"i128"
                    | b"isize"
                    | b"f32"
                    | b"f64"
                    | b"bool"
                    | b"char"
                    | b"str"
                    | b"String"
                    | b"Vec"
                    | b"Option"
                    | b"Result"
                    | b"Box"
                    | b"true"
                    | b"false"
                    | b"None"
                    | b"Some"
                    | b"Ok"
                    | b"Err"
            ) {
                TEXT_TYPE
            } else {
                text_default
            }
        }
        Lang::C => {
            if matches!(
                w,
                b"if"
                    | b"else"
                    | b"for"
                    | b"while"
                    | b"do"
                    | b"switch"
                    | b"case"
                    | b"break"
                    | b"continue"
                    | b"return"
                    | b"goto"
                    | b"typedef"
                    | b"struct"
                    | b"union"
                    | b"enum"
                    | b"extern"
                    | b"static"
                    | b"const"
                    | b"volatile"
                    | b"register"
                    | b"sizeof"
                    | b"inline"
                    | b"restrict"
                    | b"define"
                    | b"include"
                    | b"ifdef"
                    | b"ifndef"
                    | b"endif"
                    | b"pragma"
            ) {
                TEXT_KEYWORD
            } else if matches!(
                w,
                b"int"
                    | b"char"
                    | b"short"
                    | b"long"
                    | b"float"
                    | b"double"
                    | b"void"
                    | b"unsigned"
                    | b"signed"
                    | b"size_t"
                    | b"bool"
                    | b"uint8_t"
                    | b"uint16_t"
                    | b"uint32_t"
                    | b"uint64_t"
                    | b"int8_t"
                    | b"int16_t"
                    | b"int32_t"
                    | b"int64_t"
                    | b"NULL"
                    | b"true"
                    | b"false"
            ) {
                TEXT_TYPE
            } else {
                text_default
            }
        }
        Lang::Python => {
            if matches!(
                w,
                b"def"
                    | b"class"
                    | b"if"
                    | b"elif"
                    | b"else"
                    | b"for"
                    | b"while"
                    | b"return"
                    | b"break"
                    | b"continue"
                    | b"pass"
                    | b"import"
                    | b"from"
                    | b"as"
                    | b"with"
                    | b"try"
                    | b"except"
                    | b"finally"
                    | b"raise"
                    | b"yield"
                    | b"lambda"
                    | b"global"
                    | b"nonlocal"
                    | b"assert"
                    | b"del"
                    | b"in"
                    | b"not"
                    | b"and"
                    | b"or"
                    | b"is"
                    | b"async"
                    | b"await"
            ) {
                TEXT_KEYWORD
            } else if matches!(
                w,
                b"True"
                    | b"False"
                    | b"None"
                    | b"int"
                    | b"float"
                    | b"str"
                    | b"bool"
                    | b"list"
                    | b"dict"
                    | b"tuple"
                    | b"set"
                    | b"type"
                    | b"self"
                    | b"cls"
            ) {
                TEXT_TYPE
            } else {
                text_default
            }
        }
        Lang::Js => {
            if matches!(
                w,
                b"function"
                    | b"var"
                    | b"let"
                    | b"const"
                    | b"if"
                    | b"else"
                    | b"for"
                    | b"while"
                    | b"do"
                    | b"switch"
                    | b"case"
                    | b"break"
                    | b"continue"
                    | b"return"
                    | b"throw"
                    | b"try"
                    | b"catch"
                    | b"finally"
                    | b"new"
                    | b"delete"
                    | b"typeof"
                    | b"instanceof"
                    | b"in"
                    | b"of"
                    | b"class"
                    | b"extends"
                    | b"import"
                    | b"export"
                    | b"default"
                    | b"async"
                    | b"await"
                    | b"yield"
                    | b"from"
            ) {
                TEXT_KEYWORD
            } else if matches!(
                w,
                b"true"
                    | b"false"
                    | b"null"
                    | b"undefined"
                    | b"NaN"
                    | b"this"
                    | b"super"
                    | b"console"
                    | b"window"
                    | b"document"
            ) {
                TEXT_TYPE
            } else {
                text_default
            }
        }
        Lang::Shell => {
            if matches!(
                w,
                b"if"
                    | b"then"
                    | b"else"
                    | b"elif"
                    | b"fi"
                    | b"for"
                    | b"while"
                    | b"do"
                    | b"done"
                    | b"case"
                    | b"esac"
                    | b"in"
                    | b"function"
                    | b"return"
                    | b"exit"
                    | b"export"
                    | b"local"
                    | b"readonly"
                    | b"declare"
                    | b"set"
                    | b"unset"
                    | b"source"
            ) {
                TEXT_KEYWORD
            } else {
                text_default
            }
        }
        Lang::Plain => text_default,
    }
}

unsafe fn highlight_find_matches(
    g: &mut Gfx,
    d: &Doc,
    ls: usize,
    le: usize,
    x0: i32,
    y: i32,
    cw: i32,
) {
    if ST.find_len == 0 {
        return;
    }
    let p = pal();
    let mut i = ls;
    while i + ST.find_len <= le {
        if bytes_eq(&d.text[i..i + ST.find_len], &ST.find_buf[..ST.find_len]) {
            let cx = x0 + ((i - ls) as i32) * cw;
            let ex = cx + (ST.find_len as i32) * cw;
            fill(g.hdc, g.b, cx, y, ex, y + CHAR_H, p.hover);
            for j in 0..ST.find_len {
                let ch = d.text[i + j];
                if ch >= 0x20 {
                    let buf = [ch];
                    text(g.hdc, mono(), p.accent, cx + (j as i32) * cw, y, &buf);
                }
            }
            i += ST.find_len;
        } else {
            i += 1;
        }
    }
}

fn count_matches(d: &Doc) -> usize {
    unsafe {
        if ST.find_len == 0 {
            return 0;
        }
        let mut count = 0;
        let mut i = 0;
        while i + ST.find_len <= d.len {
            if bytes_eq(&d.text[i..i + ST.find_len], &ST.find_buf[..ST.find_len]) {
                count += 1;
                i += ST.find_len;
            } else {
                i += 1;
            }
        }
        count
    }
}

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn format_usize(v: usize, buf: &mut [u8; 32]) -> usize {
    if v == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 12];
    let mut n = 0;
    let mut val = v;
    while val > 0 && n < 12 {
        tmp[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    for i in 0..n {
        buf[i] = tmp[n - 1 - i];
    }
    n
}

fn format_status(buf: &mut [u8; 80], line: usize, col: usize, lang: Lang) -> usize {
    let mut n = 0;
    let prefix = b"Ln ";
    for &b in prefix {
        buf[n] = b;
        n += 1;
    }
    let mut tmp = [0u8; 32];
    let c = format_usize(line, &mut tmp);
    for i in 0..c {
        buf[n] = tmp[i];
        n += 1;
    }
    let sep = b", Col ";
    for &b in sep {
        buf[n] = b;
        n += 1;
    }
    let c = format_usize(col, &mut tmp);
    for i in 0..c {
        buf[n] = tmp[i];
        n += 1;
    }
    let sp = b"   UTF-8   ";
    for &b in sp {
        buf[n] = b;
        n += 1;
    }
    let lang_name: &[u8] = match lang {
        Lang::Plain => tr4(&[
            b"Texte brut",
            b"Plain text",
            b"Texto sin formato",
            b"Nur-Text",
        ]),
        Lang::Rust => b"Rust",
        Lang::C => b"C/C++",
        Lang::Python => b"Python",
        Lang::Js => b"JavaScript",
        Lang::Shell => b"Shell",
    };
    for &b in lang_name.iter() {
        if n < 80 {
            buf[n] = b;
            n += 1;
        }
    }
    n
}

fn ensure_cursor_visible(d: &mut Doc, editor_h: i32) {
    let cl = line_of(d, d.cursor) as i32;
    let cy = cl * CHAR_H;
    if cy < d.scroll_y {
        d.scroll_y = cy;
    }
    let visible = (editor_h / CHAR_H).max(1);
    if cy >= d.scroll_y + (visible - 1) * CHAR_H {
        d.scroll_y = cy - (visible - 2) * CHAR_H;
    }
    if d.scroll_y < 0 {
        d.scroll_y = 0;
    }
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let cw = char_w();
    let r = f.client;
    let tr = f.title;

    // === Tab hover tracking ===
    if ev == Mouse::Move {
        let old = HOVER_TAB;
        HOVER_TAB = -1;
        if y >= tr.top && y < tr.bottom {
            let mut tx = tr.left + 4;
            for i in 0..ST.tab_count {
                let t = &ST.tabs[i];
                let tw = text_width(&t.name[..t.name_len], 11) + 24;
                if x >= tx && x < tx + tw && i != ST.active {
                    HOVER_TAB = i as i32;
                    break;
                }
                tx += tw + 2;
            }
        }
        if HOVER_TAB != old {
            return true;
        }
    }

    // === Tab bar clicks ===
    if y >= tr.top && y < tr.bottom {
        match ev {
            Mouse::Down => {
                let mut tx = tr.left + 4;
                for i in 0..ST.tab_count {
                    let t = &ST.tabs[i];
                    let tw = text_width(&t.name[..t.name_len], 11) + 24;
                    if x >= tx && x < tx + tw {
                        // Close button area (last 16px)
                        if x >= tx + tw - 16 {
                            close_tab(i);
                            return true;
                        }
                        ST.active = i;
                        return true;
                    }
                    tx += tw + 2;
                }
                // "+" button
                if ST.tab_count < MAX_TABS && x >= tx + 2 && x < tx + 22 {
                    open_new();
                    return true;
                }
                return false; // let wm handle title drag
            }
            _ => return false,
        }
    }

    // === Find bar clicks ===
    let find_bar_bottom = if ST.find_open {
        let my = r.top + MENU_H;
        let fh = if ST.replace_open { FIND_H * 2 } else { FIND_H };
        my + fh
    } else {
        r.top + MENU_H
    };

    if ST.find_open && y >= r.top + MENU_H && y < find_bar_bottom {
        match ev {
            Mouse::Down => {
                let my = r.top + MENU_H;
                // Close button
                if x >= r.right - 24 && y < my + FIND_H {
                    ST.find_open = false;
                    ST.replace_open = false;
                    return true;
                }
                if ST.replace_open {
                    // Replace buttons
                    let field_x = r.left + 80;
                    let field_w = (r.right - r.left - 160).min(400).max(120);
                    let ry = my + FIND_H;
                    let bx = field_x + field_w + 8;
                    if y >= ry && y < ry + FIND_H {
                        if x >= bx && x < bx + 72 {
                            replace_next();
                            return true;
                        }
                        if x >= bx + 78 && x < bx + 150 {
                            replace_all();
                            return true;
                        }
                        ST.find_focused = false;
                        return true;
                    }
                }
                ST.find_focused = y < r.top + MENU_H + FIND_H;
                return true;
            }
            _ => return true,
        }
    }

    // === Editor area clicks ===
    let ey = find_bar_bottom;
    let editor_x = r.left + GUTTER_W + 4;

    match ev {
        Mouse::Down => {
            if y >= ey && y < r.bottom - STATUS_H && x >= editor_x {
                let d = doc();
                let shift = (crate::gfx::key_state(0x10) as u16 & 0x8000) != 0;
                let line_idx = ((y - ey + d.scroll_y) / CHAR_H).max(0) as usize;
                let col_idx = ((x - editor_x + d.scroll_x + cw / 2) / cw).max(0) as usize;
                let li = line_idx.min(d.line_count.saturating_sub(1));
                let le = line_end(d, li);
                let ls = d.lines[li];
                let max_col = le - ls;
                let pos = ls + col_idx.min(max_col);
                d.cursor = pos;
                if !shift {
                    d.anchor = pos;
                }
                ST.blink_on = true;
                return true;
            }
        }
        Mouse::Wheel(delta) => {
            let d = doc();
            d.scroll_y -= delta * CHAR_H * 3;
            let max_scroll = ((d.line_count as i32) * CHAR_H).saturating_sub(CHAR_H);
            if d.scroll_y > max_scroll {
                d.scroll_y = max_scroll;
            }
            if d.scroll_y < 0 {
                d.scroll_y = 0;
            }
            return true;
        }
        _ => {}
    }
    true
}

unsafe fn close_tab(idx: usize) {
    if ST.tab_count <= 1 {
        // Don't close last tab, just clear it
        ST.tabs[0] = Doc::new();
        let name = tr4(&[
            b"Sans titre 1",
            b"Untitled 1",
            b"Sin t\xEDtulo 1",
            b"Unbenannt 1",
        ]);
        for i in 0..name.len() {
            ST.tabs[0].name[i] = name[i];
        }
        ST.tabs[0].name_len = name.len();
        ST.active = 0;
        return;
    }
    // Shift tabs left
    let mut i = idx;
    while i + 1 < ST.tab_count {
        // Can't move Doc (no Copy), so copy field by field
        let src = i + 1;
        ST.tabs[i].text = ST.tabs[src].text;
        ST.tabs[i].len = ST.tabs[src].len;
        ST.tabs[i].lines = ST.tabs[src].lines;
        ST.tabs[i].line_count = ST.tabs[src].line_count;
        ST.tabs[i].cursor = ST.tabs[src].cursor;
        ST.tabs[i].anchor = ST.tabs[src].anchor;
        ST.tabs[i].scroll_y = ST.tabs[src].scroll_y;
        ST.tabs[i].scroll_x = ST.tabs[src].scroll_x;
        ST.tabs[i].modified = ST.tabs[src].modified;
        ST.tabs[i].name = ST.tabs[src].name;
        ST.tabs[i].name_len = ST.tabs[src].name_len;
        ST.tabs[i].lang = ST.tabs[src].lang;
        i += 1;
    }
    ST.tab_count -= 1;
    if ST.active >= ST.tab_count {
        ST.active = ST.tab_count - 1;
    }
}

unsafe fn find_next() {
    if ST.find_len == 0 {
        return;
    }
    let d = doc();
    let start = d.cursor;
    let mut i = start;
    loop {
        i += 1;
        if i + ST.find_len > d.len {
            i = 0;
        }
        if i == start {
            return;
        }
        if bytes_eq(&d.text[i..i + ST.find_len], &ST.find_buf[..ST.find_len]) {
            d.cursor = i;
            d.anchor = i + ST.find_len;
            ensure_cursor_visible(d, 400);
            return;
        }
    }
}

unsafe fn find_prev() {
    if ST.find_len == 0 {
        return;
    }
    let d = doc();
    let start = if d.cursor > 0 { d.cursor - 1 } else { d.len };
    let mut i = start;
    loop {
        if i + ST.find_len <= d.len
            && bytes_eq(&d.text[i..i + ST.find_len], &ST.find_buf[..ST.find_len])
        {
            d.cursor = i;
            d.anchor = i + ST.find_len;
            ensure_cursor_visible(d, 400);
            return;
        }
        if i == 0 {
            i = d.len;
        }
        i -= 1;
        if i == start {
            return;
        }
    }
}

unsafe fn replace_next() {
    if ST.find_len == 0 {
        return;
    }
    let d = doc();
    // If current selection matches find text, replace it
    let (a, b) = sel_range(d);
    if b - a == ST.find_len && bytes_eq(&d.text[a..b], &ST.find_buf[..ST.find_len]) {
        // Delete selection
        d.cursor = a;
        d.anchor = b;
        delete_sel(d);
        // Insert replacement
        for j in 0..ST.replace_len {
            if d.len >= MAX_TEXT - 1 {
                break;
            }
            let mut k = d.len;
            while k > d.cursor {
                d.text[k] = d.text[k - 1];
                k -= 1;
            }
            d.text[d.cursor] = ST.replace_buf[j];
            d.len += 1;
            d.cursor += 1;
        }
        d.anchor = d.cursor;
        d.modified = true;
        rebuild_lines(d);
    }
    find_next();
}

unsafe fn replace_all() {
    if ST.find_len == 0 {
        return;
    }
    let d = doc();
    let mut i = 0;
    let mut changed = false;
    while i + ST.find_len <= d.len {
        if bytes_eq(&d.text[i..i + ST.find_len], &ST.find_buf[..ST.find_len]) {
            // Delete match
            let gap = ST.find_len;
            let mut k = i;
            while k + gap < d.len {
                d.text[k] = d.text[k + gap];
                k += 1;
            }
            d.len -= gap;
            // Insert replacement
            for j in 0..ST.replace_len {
                if d.len >= MAX_TEXT - 1 {
                    break;
                }
                let mut k = d.len;
                while k > i + j {
                    d.text[k] = d.text[k - 1];
                    k -= 1;
                }
                d.text[i + j] = ST.replace_buf[j];
                d.len += 1;
            }
            i += ST.replace_len;
            changed = true;
        } else {
            i += 1;
        }
    }
    if changed {
        d.modified = true;
        rebuild_lines(d);
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    let ctrl = (crate::gfx::key_state(0x11) as u16 & 0x8000) != 0;
    let shift = (crate::gfx::key_state(0x10) as u16 & 0x8000) != 0;

    // Find bar keyboard handling
    if ST.find_open {
        // Escape closes find bar
        if vk == 0x1B {
            ST.find_open = false;
            ST.replace_open = false;
            return true;
        }
        // Tab switches focus between find and replace fields
        if vk == 0x09 && ST.replace_open {
            ST.find_focused = !ST.find_focused;
            return true;
        }
        // Enter in find field = find next
        if (ch == 13 || vk == 0x0D) && ST.find_focused {
            if shift {
                find_prev();
            } else {
                find_next();
            }
            return true;
        }
        // Enter in replace field = replace next
        if (ch == 13 || vk == 0x0D) && !ST.find_focused {
            replace_next();
            return true;
        }
        // Typing into find/replace fields
        let (buf, len) = if ST.find_focused {
            (&mut ST.find_buf as &mut [u8], &mut ST.find_len)
        } else {
            (&mut ST.replace_buf as &mut [u8], &mut ST.replace_len)
        };
        if ch >= 0x20 && ch < 0x7F && *len < 127 {
            buf[*len] = ch as u8;
            *len += 1;
            return true;
        }
        if ch == 8 && *len > 0 {
            *len -= 1;
            return true;
        }
        // Let other keys fall through to editor
    }

    // Ctrl shortcuts
    if ctrl {
        match vk {
            0x46 => {
                // Ctrl+F
                ST.find_open = true;
                ST.replace_open = false;
                ST.find_focused = true;
                return true;
            }
            0x48 => {
                // Ctrl+H
                ST.find_open = true;
                ST.replace_open = true;
                ST.find_focused = true;
                return true;
            }
            0x4E => {
                // Ctrl+N
                open_new();
                wm::open(wm::APP_NOTEPAD);
                return true;
            }
            0x57 => {
                // Ctrl+W
                close_tab(ST.active);
                return true;
            }
            0x41 => {
                // Ctrl+A
                let d = doc();
                d.anchor = 0;
                d.cursor = d.len;
                return true;
            }
            0x43 | 0x58 => {
                // Ctrl+C / Ctrl+X : presse-papiers système (et historique Win+V)
                copy_selection(vk == 0x58);
                return true;
            }
            0x56 => {
                // Ctrl+V
                paste_from_clipboard();
                return true;
            }
            0x47 => {
                // Ctrl+G - go to line (toggle)
                // Find next match (F3-like)
                find_next();
                return true;
            }
            _ => {}
        }
    }

    // F3 = find next, Shift+F3 = find prev
    if vk == 0x72 {
        if shift {
            find_prev();
        } else {
            find_next();
        }
        return true;
    }

    let d = doc();
    ST.blink_on = true;

    // Printable characters
    if ch >= 0x20 && ch < 0x7F {
        insert_char(d, ch as u8);
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Backspace
    if ch == 8 {
        if has_sel(d) {
            delete_sel(d);
        } else if d.cursor > 0 {
            d.anchor = d.cursor;
            let mut i = d.cursor;
            while i < d.len {
                d.text[i - 1] = d.text[i];
                i += 1;
            }
            d.len -= 1;
            d.cursor -= 1;
            d.anchor = d.cursor;
            d.modified = true;
            rebuild_lines(d);
        }
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Delete
    if vk == 0x2E {
        if has_sel(d) {
            delete_sel(d);
        } else if d.cursor < d.len {
            let mut i = d.cursor;
            while i + 1 < d.len {
                d.text[i] = d.text[i + 1];
                i += 1;
            }
            d.len -= 1;
            d.modified = true;
            rebuild_lines(d);
        }
        return true;
    }

    // Enter
    if ch == 13 || vk == 0x0D {
        insert_char(d, b'\n');
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Tab
    if vk == 0x09 {
        for _ in 0..4 {
            insert_char(d, b' ');
        }
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Arrow keys with shift for selection
    if vk == 0x25 {
        // Left
        if d.cursor > 0 {
            d.cursor -= 1;
        }
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }
    if vk == 0x27 {
        // Right
        if d.cursor < d.len {
            d.cursor += 1;
        }
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }
    if vk == 0x26 {
        // Up
        let l = line_of(d, d.cursor);
        if l > 0 {
            let c = col_of(d, d.cursor);
            let prev_start = d.lines[l - 1];
            let prev_len = line_end(d, l - 1) - prev_start;
            d.cursor = prev_start + c.min(prev_len);
        }
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }
    if vk == 0x28 {
        // Down
        let l = line_of(d, d.cursor);
        if l + 1 < d.line_count {
            let c = col_of(d, d.cursor);
            let next_start = d.lines[l + 1];
            let next_len = line_end(d, l + 1) - next_start;
            d.cursor = next_start + c.min(next_len);
        }
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Home
    if vk == 0x24 {
        let l = line_of(d, d.cursor);
        d.cursor = d.lines[l];
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }
    // End
    if vk == 0x23 {
        let l = line_of(d, d.cursor);
        d.cursor = line_end(d, l);
        if !shift {
            d.anchor = d.cursor;
        }
        ensure_cursor_visible(d, 400);
        return true;
    }

    // Page Up / Page Down
    if vk == 0x21 {
        // Page Up
        let lines = 20;
        let l = line_of(d, d.cursor);
        let c = col_of(d, d.cursor);
        let new_l = if l >= lines { l - lines } else { 0 };
        let new_start = d.lines[new_l];
        let new_len = line_end(d, new_l) - new_start;
        d.cursor = new_start + c.min(new_len);
        if !shift {
            d.anchor = d.cursor;
        }
        d.scroll_y = (d.scroll_y - lines as i32 * CHAR_H).max(0);
        return true;
    }
    if vk == 0x22 {
        // Page Down
        let lines = 20;
        let l = line_of(d, d.cursor);
        let c = col_of(d, d.cursor);
        let new_l = (l + lines).min(d.line_count.saturating_sub(1));
        let new_start = d.lines[new_l];
        let new_len = line_end(d, new_l) - new_start;
        d.cursor = new_start + c.min(new_len);
        if !shift {
            d.anchor = d.cursor;
        }
        d.scroll_y += lines as i32 * CHAR_H;
        let max_scroll = ((d.line_count as i32) * CHAR_H).saturating_sub(CHAR_H);
        if d.scroll_y > max_scroll {
            d.scroll_y = max_scroll;
        }
        return true;
    }

    // Escape
    if vk == 0x1B {
        if has_sel(d) {
            d.anchor = d.cursor;
            return true;
        }
    }

    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    ST.blink_tick += 1;
    if ST.blink_tick >= 2 {
        ST.blink_tick = 0;
        ST.blink_on = !ST.blink_on;
        return true;
    }
    false
}
