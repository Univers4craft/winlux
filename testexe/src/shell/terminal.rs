//! Terminal intégré Winlux — émulateur VT100/xterm avec onglets, PTY natif,
//! coloration ANSI, copier/coller et redimensionnement dynamique.

use crate::gfx::*;
use crate::wm;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

const MAX_COLS: usize = 200;
const MAX_ROWS: usize = 64;
const SCROLLBACK: usize = 1000;
const MAX_TABS: usize = 6;
const CELL_W: i32 = 8;
const CELL_H: i32 = 16;
const TAB_H: i32 = 36;
const TAB_W: i32 = 160;
const TAB_CLOSE: i32 = 16;
const TAB_GAP: i32 = 2;

const ATTR_BOLD: u8 = 1;
const ATTR_UNDERLINE: u8 = 2;
const ATTR_REVERSE: u8 = 4;

const DEFAULT_FG: u8 = 7;
const DEFAULT_BG: u8 = 0;

fn term_color(idx: u8) -> u32 {
    match idx {
        0 => rgb(12, 12, 12),
        1 => rgb(197, 15, 31),
        2 => rgb(19, 161, 14),
        3 => rgb(193, 156, 0),
        4 => rgb(0, 55, 218),
        5 => rgb(136, 23, 152),
        6 => rgb(58, 150, 221),
        7 => rgb(204, 204, 204),
        8 => rgb(118, 118, 118),
        9 => rgb(231, 72, 86),
        10 => rgb(22, 198, 12),
        11 => rgb(249, 241, 165),
        12 => rgb(59, 120, 255),
        13 => rgb(180, 0, 158),
        14 => rgb(97, 214, 214),
        15 => rgb(242, 242, 242),
        n => {
            if n < 232 {
                let i = (n - 16) as u32;
                let r = (i / 36) * 51;
                let g = ((i / 6) % 6) * 51;
                let b = (i % 6) * 51;
                rgb(r, g, b)
            } else {
                let v = 8 + ((n - 232) as u32) * 10;
                rgb(v, v, v)
            }
        }
    }
}

#[derive(Copy, Clone)]
struct Cell {
    ch: u8,
    fg: u8,
    bg: u8,
    attr: u8,
}
impl Cell {
    const fn blank() -> Self {
        Cell {
            ch: b' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            attr: 0,
        }
    }
}

const BLANK_ROW: [Cell; MAX_COLS] = [Cell::blank(); MAX_COLS];

#[derive(Copy, Clone, PartialEq)]
enum Parse {
    Normal,
    Escape,
    Csi,
    Osc,
    OscEsc,
}

struct Screen {
    cells: [[Cell; MAX_COLS]; MAX_ROWS],
    scroll: [[Cell; MAX_COLS]; SCROLLBACK],
    scroll_len: usize,
    cols: usize,
    rows: usize,
    cx: usize,
    cy: usize,
    cursor_vis: bool,
    saved_cx: usize,
    saved_cy: usize,
    saved_fg: u8,
    saved_bg: u8,
    saved_attr: u8,
    fg: u8,
    bg: u8,
    attr: u8,
    scroll_top: usize,
    scroll_bot: usize,
    state: Parse,
    params: [u16; 16],
    nparam: usize,
    private: bool,
    osc_buf: [u8; 256],
    osc_len: usize,
    alt_cells: [[Cell; MAX_COLS]; MAX_ROWS],
    alt_cx: usize,
    alt_cy: usize,
    using_alt: bool,
    title: [u8; 64],
    title_len: usize,
    view_off: usize,
    wrap_next: bool,
}

impl Screen {
    const fn new() -> Self {
        Screen {
            cells: [[Cell::blank(); MAX_COLS]; MAX_ROWS],
            scroll: [[Cell::blank(); MAX_COLS]; SCROLLBACK],
            scroll_len: 0,
            cols: 80,
            rows: 24,
            cx: 0,
            cy: 0,
            cursor_vis: true,
            saved_cx: 0,
            saved_cy: 0,
            saved_fg: DEFAULT_FG,
            saved_bg: DEFAULT_BG,
            saved_attr: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            attr: 0,
            scroll_top: 0,
            scroll_bot: 23,
            state: Parse::Normal,
            params: [0; 16],
            nparam: 0,
            private: false,
            osc_buf: [0; 256],
            osc_len: 0,
            alt_cells: [[Cell::blank(); MAX_COLS]; MAX_ROWS],
            alt_cx: 0,
            alt_cy: 0,
            using_alt: false,
            title: [0; 64],
            title_len: 0,
            view_off: 0,
            wrap_next: false,
        }
    }

    fn reset_params(&mut self) {
        self.params = [0; 16];
        self.nparam = 0;
        self.private = false;
    }

    fn param(&self, idx: usize, def: u16) -> u16 {
        if idx < self.nparam && self.params[idx] != 0 {
            self.params[idx]
        } else {
            def
        }
    }

    fn scroll_up(&mut self, top: usize, bot: usize, n: usize) {
        if top >= bot || bot >= MAX_ROWS {
            return;
        }
        for _ in 0..n {
            if !self.using_alt && top == self.scroll_top {
                let idx = self.scroll_len % SCROLLBACK;
                self.scroll[idx] = self.cells[top];
                self.scroll_len += 1;
            }
            let mut r = top;
            while r < bot {
                self.cells[r] = self.cells[r + 1];
                r += 1;
            }
            self.cells[bot] = BLANK_ROW;
            for c in 0..self.cols {
                self.cells[bot][c].fg = self.fg;
                self.cells[bot][c].bg = self.bg;
            }
        }
    }

    fn scroll_down(&mut self, top: usize, bot: usize, n: usize) {
        if top >= bot || bot >= MAX_ROWS {
            return;
        }
        for _ in 0..n {
            let mut r = bot;
            while r > top {
                self.cells[r] = self.cells[r - 1];
                r -= 1;
            }
            self.cells[top] = BLANK_ROW;
            for c in 0..self.cols {
                self.cells[top][c].fg = self.fg;
                self.cells[top][c].bg = self.bg;
            }
        }
    }

    fn linefeed(&mut self) {
        self.wrap_next = false;
        if self.cy == self.scroll_bot {
            self.scroll_up(self.scroll_top, self.scroll_bot, 1);
        } else if self.cy < self.rows.saturating_sub(1) {
            self.cy += 1;
        }
    }

    fn reverse_lf(&mut self) {
        self.wrap_next = false;
        if self.cy == self.scroll_top {
            self.scroll_down(self.scroll_top, self.scroll_bot, 1);
        } else if self.cy > 0 {
            self.cy -= 1;
        }
    }

    fn put_char(&mut self, ch: u8) {
        if self.wrap_next {
            self.cx = 0;
            self.linefeed();
            self.wrap_next = false;
        }
        if self.cy < MAX_ROWS && self.cx < MAX_COLS {
            self.cells[self.cy][self.cx] = Cell {
                ch,
                fg: self.fg,
                bg: self.bg,
                attr: self.attr,
            };
        }
        if self.cx + 1 >= self.cols {
            self.wrap_next = true;
        } else {
            self.cx += 1;
        }
    }

    fn erase_cells(&mut self, row: usize, from: usize, to: usize) {
        if row >= MAX_ROWS {
            return;
        }
        for c in from..to.min(MAX_COLS) {
            self.cells[row][c] = Cell {
                ch: b' ',
                fg: self.fg,
                bg: self.bg,
                attr: 0,
            };
        }
    }

    fn save_cursor(&mut self) {
        self.saved_cx = self.cx;
        self.saved_cy = self.cy;
        self.saved_fg = self.fg;
        self.saved_bg = self.bg;
        self.saved_attr = self.attr;
    }

    fn restore_cursor(&mut self) {
        self.cx = self.saved_cx;
        self.cy = self.saved_cy;
        self.fg = self.saved_fg;
        self.bg = self.saved_bg;
        self.attr = self.saved_attr;
        self.wrap_next = false;
    }

    fn full_reset(&mut self) {
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.attr = 0;
        self.cx = 0;
        self.cy = 0;
        self.cursor_vis = true;
        self.scroll_top = 0;
        self.scroll_bot = self.rows.saturating_sub(1);
        self.wrap_next = false;
        for r in 0..self.rows {
            self.cells[r] = BLANK_ROW;
        }
        self.state = Parse::Normal;
    }

    fn sgr(&mut self) {
        let n = if self.nparam == 0 { 1 } else { self.nparam };
        let mut i = 0;
        while i < n {
            let p = self.params[i];
            match p {
                0 => {
                    self.fg = DEFAULT_FG;
                    self.bg = DEFAULT_BG;
                    self.attr = 0;
                }
                1 => self.attr |= ATTR_BOLD,
                4 => self.attr |= ATTR_UNDERLINE,
                7 => self.attr |= ATTR_REVERSE,
                22 => self.attr &= !ATTR_BOLD,
                24 => self.attr &= !ATTR_UNDERLINE,
                27 => self.attr &= !ATTR_REVERSE,
                30..=37 => self.fg = (p - 30) as u8,
                38 => {
                    if i + 1 < n && self.params[i + 1] == 5 && i + 2 < n {
                        self.fg = self.params[i + 2] as u8;
                        i += 2;
                    } else if i + 1 < n && self.params[i + 1] == 2 && i + 4 < n {
                        let r = self.params[i + 2] as u8;
                        let g = self.params[i + 3] as u8;
                        let b = self.params[i + 4] as u8;
                        self.fg = closest_color(r, g, b);
                        i += 4;
                    }
                }
                39 => self.fg = DEFAULT_FG,
                40..=47 => self.bg = (p - 40) as u8,
                48 => {
                    if i + 1 < n && self.params[i + 1] == 5 && i + 2 < n {
                        self.bg = self.params[i + 2] as u8;
                        i += 2;
                    } else if i + 1 < n && self.params[i + 1] == 2 && i + 4 < n {
                        let r = self.params[i + 2] as u8;
                        let g = self.params[i + 3] as u8;
                        let b = self.params[i + 4] as u8;
                        self.bg = closest_color(r, g, b);
                        i += 4;
                    }
                }
                49 => self.bg = DEFAULT_BG,
                90..=97 => self.fg = (p - 90 + 8) as u8,
                100..=107 => self.bg = (p - 100 + 8) as u8,
                _ => {}
            }
            i += 1;
        }
    }

    fn exec_csi(&mut self, cmd: u8) {
        let p0 = self.param(0, 1) as usize;
        match cmd {
            b'A' => {
                self.cy = self.cy.saturating_sub(p0);
                self.wrap_next = false;
            }
            b'B' => {
                self.cy = (self.cy + p0).min(self.rows.saturating_sub(1));
                self.wrap_next = false;
            }
            b'C' => {
                self.cx = (self.cx + p0).min(self.cols.saturating_sub(1));
                self.wrap_next = false;
            }
            b'D' => {
                self.cx = self.cx.saturating_sub(p0);
                self.wrap_next = false;
            }
            b'E' => {
                self.cy = (self.cy + p0).min(self.rows.saturating_sub(1));
                self.cx = 0;
                self.wrap_next = false;
            }
            b'F' => {
                self.cy = self.cy.saturating_sub(p0);
                self.cx = 0;
                self.wrap_next = false;
            }
            b'G' | b'`' => {
                self.cx = (p0.saturating_sub(1)).min(self.cols.saturating_sub(1));
                self.wrap_next = false;
            }
            b'H' | b'f' => {
                let row = self.param(0, 1) as usize;
                let col = self.param(1, 1) as usize;
                self.cy = (row.saturating_sub(1)).min(self.rows.saturating_sub(1));
                self.cx = (col.saturating_sub(1)).min(self.cols.saturating_sub(1));
                self.wrap_next = false;
            }
            b'J' => {
                let mode = self.param(0, 0);
                match mode {
                    0 => {
                        self.erase_cells(self.cy, self.cx, self.cols);
                        for r in (self.cy + 1)..self.rows {
                            self.erase_cells(r, 0, self.cols);
                        }
                    }
                    1 => {
                        for r in 0..self.cy {
                            self.erase_cells(r, 0, self.cols);
                        }
                        self.erase_cells(self.cy, 0, self.cx + 1);
                    }
                    2 | 3 => {
                        for r in 0..self.rows {
                            self.erase_cells(r, 0, self.cols);
                        }
                    }
                    _ => {}
                }
            }
            b'K' => {
                let mode = self.param(0, 0);
                match mode {
                    0 => self.erase_cells(self.cy, self.cx, self.cols),
                    1 => self.erase_cells(self.cy, 0, self.cx + 1),
                    2 => self.erase_cells(self.cy, 0, self.cols),
                    _ => {}
                }
            }
            b'L' => {
                let n = p0.min(self.scroll_bot.saturating_sub(self.cy) + 1);
                self.scroll_down(self.cy, self.scroll_bot, n);
            }
            b'M' => {
                let n = p0.min(self.scroll_bot.saturating_sub(self.cy) + 1);
                self.scroll_up(self.cy, self.scroll_bot, n);
            }
            b'P' => {
                let n = p0.min(self.cols.saturating_sub(self.cx));
                if self.cy < MAX_ROWS {
                    let mut c = self.cx;
                    while c + n < self.cols {
                        self.cells[self.cy][c] = self.cells[self.cy][c + n];
                        c += 1;
                    }
                    while c < self.cols {
                        self.cells[self.cy][c] = Cell::blank();
                        c += 1;
                    }
                }
            }
            b'@' => {
                let n = p0.min(self.cols.saturating_sub(self.cx));
                if self.cy < MAX_ROWS {
                    let mut c = self.cols.saturating_sub(1);
                    while c >= self.cx + n {
                        self.cells[self.cy][c] = self.cells[self.cy][c - n];
                        c -= 1;
                    }
                    for c in self.cx..(self.cx + n).min(self.cols) {
                        self.cells[self.cy][c] = Cell::blank();
                    }
                }
            }
            b'X' => {
                let n = p0.min(self.cols.saturating_sub(self.cx));
                self.erase_cells(self.cy, self.cx, self.cx + n);
            }
            b'S' => self.scroll_up(self.scroll_top, self.scroll_bot, p0),
            b'T' => self.scroll_down(self.scroll_top, self.scroll_bot, p0),
            b'd' => {
                self.cy = (p0.saturating_sub(1)).min(self.rows.saturating_sub(1));
                self.wrap_next = false;
            }
            b'm' => self.sgr(),
            b'r' => {
                let top = self.param(0, 1) as usize;
                let bot = self.param(1, self.rows as u16) as usize;
                self.scroll_top = top.saturating_sub(1).min(self.rows.saturating_sub(1));
                self.scroll_bot = bot.saturating_sub(1).min(self.rows.saturating_sub(1));
                if self.scroll_top >= self.scroll_bot {
                    self.scroll_top = 0;
                    self.scroll_bot = self.rows.saturating_sub(1);
                }
                self.cx = 0;
                self.cy = 0;
                self.wrap_next = false;
            }
            b'h' | b'l' => {
                let set = cmd == b'h';
                if self.private {
                    let mode = self.param(0, 0);
                    match mode {
                        25 => self.cursor_vis = set,
                        1049 => {
                            if set && !self.using_alt {
                                self.alt_cells = self.cells;
                                self.alt_cx = self.cx;
                                self.alt_cy = self.cy;
                                self.using_alt = true;
                                for r in 0..self.rows {
                                    self.erase_cells(r, 0, self.cols);
                                }
                                self.cx = 0;
                                self.cy = 0;
                            } else if !set && self.using_alt {
                                self.cells = self.alt_cells;
                                self.cx = self.alt_cx;
                                self.cy = self.alt_cy;
                                self.using_alt = false;
                            }
                        }
                        _ => {}
                    }
                }
            }
            b'n' => {
                if self.param(0, 0) == 6 {
                    unsafe {
                        let tab = &TABS[ACTIVE_TAB];
                        if tab.active && tab.pty != 0 {
                            let mut resp = [0u8; 32];
                            let mut len = 0usize;
                            resp[len] = 0x1b;
                            len += 1;
                            resp[len] = b'[';
                            len += 1;
                            len += write_num(&mut resp[len..], (self.cy + 1) as u32);
                            resp[len] = b';';
                            len += 1;
                            len += write_num(&mut resp[len..], (self.cx + 1) as u32);
                            resp[len] = b'R';
                            len += 1;
                            let mut written = 0u32;
                            WinluxWritePty(
                                tab.pty,
                                resp.as_ptr(),
                                len as u32,
                                &mut written,
                            );
                        }
                    }
                }
            }
            b'c' => {
                if !self.private && self.param(0, 0) == 0 {
                    unsafe {
                        let tab = &TABS[ACTIVE_TAB];
                        if tab.active && tab.pty != 0 {
                            let resp = b"\x1b[?1;2c";
                            let mut written = 0u32;
                            WinluxWritePty(
                                tab.pty,
                                resp.as_ptr(),
                                resp.len() as u32,
                                &mut written,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn process(&mut self, b: u8) {
        match self.state {
            Parse::Normal => match b {
                0x1b => self.state = Parse::Escape,
                b'\r' => {
                    self.cx = 0;
                    self.wrap_next = false;
                }
                b'\n' | 0x0b | 0x0c => self.linefeed(),
                b'\t' => {
                    self.cx = ((self.cx / 8) + 1) * 8;
                    if self.cx >= self.cols {
                        self.cx = self.cols.saturating_sub(1);
                    }
                    self.wrap_next = false;
                }
                0x08 => {
                    if self.cx > 0 {
                        self.cx -= 1;
                    }
                    self.wrap_next = false;
                }
                0x07 => {}
                0x00..=0x1f => {}
                _ => self.put_char(b),
            },
            Parse::Escape => match b {
                b'[' => {
                    self.state = Parse::Csi;
                    self.reset_params();
                }
                b']' => {
                    self.state = Parse::Osc;
                    self.osc_len = 0;
                }
                b'c' => {
                    self.full_reset();
                    self.state = Parse::Normal;
                }
                b'D' => {
                    self.linefeed();
                    self.state = Parse::Normal;
                }
                b'M' => {
                    self.reverse_lf();
                    self.state = Parse::Normal;
                }
                b'7' => {
                    self.save_cursor();
                    self.state = Parse::Normal;
                }
                b'8' => {
                    self.restore_cursor();
                    self.state = Parse::Normal;
                }
                b'(' | b')' | b'*' | b'+' => {
                    self.state = Parse::Normal;
                }
                _ => self.state = Parse::Normal,
            },
            Parse::Csi => {
                if b >= b'0' && b <= b'9' {
                    if self.nparam == 0 {
                        self.nparam = 1;
                    }
                    let i = self.nparam - 1;
                    if i < 16 {
                        self.params[i] = self.params[i].saturating_mul(10).saturating_add((b - b'0') as u16);
                    }
                } else if b == b';' {
                    if self.nparam < 16 {
                        self.nparam += 1;
                    }
                } else if b == b'?' {
                    self.private = true;
                } else if b == b'>' || b == b'!' {
                    // ignore modifier
                } else {
                    if self.nparam == 0 && b != b'm' {
                        self.nparam = 1;
                    }
                    self.exec_csi(b);
                    self.state = Parse::Normal;
                }
            }
            Parse::Osc => match b {
                0x07 => {
                    self.handle_osc();
                    self.state = Parse::Normal;
                }
                0x1b => self.state = Parse::OscEsc,
                _ => {
                    if self.osc_len < 256 {
                        self.osc_buf[self.osc_len] = b;
                        self.osc_len += 1;
                    }
                }
            },
            Parse::OscEsc => {
                if b == b'\\' {
                    self.handle_osc();
                }
                self.state = Parse::Normal;
            }
        }
    }

    fn handle_osc(&mut self) {
        if self.osc_len >= 2 && (self.osc_buf[0] == b'0' || self.osc_buf[0] == b'2') && self.osc_buf[1] == b';' {
            let len = (self.osc_len - 2).min(63);
            self.title[..len].copy_from_slice(&self.osc_buf[2..2 + len]);
            self.title_len = len;
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let new_cols = cols.min(MAX_COLS).max(1);
        let new_rows = rows.min(MAX_ROWS).max(1);
        if new_rows > self.rows {
            for r in self.rows..new_rows {
                self.cells[r] = BLANK_ROW;
            }
        }
        self.cols = new_cols;
        self.rows = new_rows;
        if self.cx >= new_cols {
            self.cx = new_cols - 1;
        }
        if self.cy >= new_rows {
            self.cy = new_rows - 1;
        }
        self.scroll_top = 0;
        self.scroll_bot = new_rows.saturating_sub(1);
        self.wrap_next = false;
    }
}

fn closest_color(r: u8, g: u8, b: u8) -> u8 {
    let ri = ((r as u16 + 25) / 51).min(5) as u8;
    let gi = ((g as u16 + 25) / 51).min(5) as u8;
    let bi = ((b as u16 + 25) / 51).min(5) as u8;
    16 + ri * 36 + gi * 6 + bi
}

fn write_num(buf: &mut [u8], val: u32) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut n = 0;
    let mut v = val;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    for i in 0..n {
        buf[i] = tmp[n - 1 - i];
    }
    n
}

struct Tab {
    active: bool,
    pty: usize,
    screen: Screen,
}

impl Tab {
    const fn empty() -> Self {
        Tab {
            active: false,
            pty: 0,
            screen: Screen::new(),
        }
    }
}

static mut TABS: [Tab; MAX_TABS] = [
    Tab::empty(),
    Tab::empty(),
    Tab::empty(),
    Tab::empty(),
    Tab::empty(),
    Tab::empty(),
];
static mut ACTIVE_TAB: usize = 0;
static mut TAB_COUNT: usize = 0;
static mut FONT: Handle = core::ptr::null_mut();
static mut BOLD_FONT: Handle = core::ptr::null_mut();
static mut HOVER_TAB: i32 = -1;
static mut HOVER_CLOSE: i32 = -1;
static mut HOVER_NEW: bool = false;
static mut LAST_COLS: usize = 0;
static mut LAST_ROWS: usize = 0;
static mut SEL_START: (usize, usize) = (0, 0);
static mut SEL_END: (usize, usize) = (0, 0);
static mut SELECTING: bool = false;
static mut HAS_SEL: bool = false;

pub const CUSTOM_TITLE: bool = true;

unsafe fn ensure_font() {
    if FONT.is_null() {
        FONT = CreateFontA(
            CELL_H,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            49,
            c"DejaVu Sans Mono".as_ptr(),
        );
        BOLD_FONT = CreateFontA(
            CELL_H,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            49,
            c"DejaVu Sans Mono".as_ptr(),
        );
    }
}

unsafe fn open_tab() -> bool {
    if TAB_COUNT >= MAX_TABS {
        return false;
    }
    let mut slot = MAX_TABS;
    for i in 0..MAX_TABS {
        if !TABS[i].active {
            slot = i;
            break;
        }
    }
    if slot >= MAX_TABS {
        return false;
    }
    let cols = if LAST_COLS > 0 { LAST_COLS } else { 80 };
    let rows = if LAST_ROWS > 0 { LAST_ROWS } else { 24 };
    let pty = WinluxCreatePty(cols as u32, rows as u32);
    if pty == 0 {
        return false;
    }
    TABS[slot] = Tab {
        active: true,
        pty,
        screen: Screen::new(),
    };
    TABS[slot].screen.resize(cols, rows);
    TAB_COUNT += 1;
    ACTIVE_TAB = slot;
    true
}

unsafe fn close_tab(idx: usize) {
    if idx >= MAX_TABS || !TABS[idx].active {
        return;
    }
    WinluxClosePty(TABS[idx].pty);
    TABS[idx].active = false;
    TABS[idx].pty = 0;
    TAB_COUNT -= 1;
    if TAB_COUNT == 0 {
        wm::close(wm::APP_TERMINAL);
        return;
    }
    if ACTIVE_TAB == idx {
        for i in 0..MAX_TABS {
            if TABS[i].active {
                ACTIVE_TAB = i;
                break;
            }
        }
    }
}

pub unsafe fn init() {
    TAB_COUNT = 0;
    ACTIVE_TAB = 0;
    for i in 0..MAX_TABS {
        TABS[i].active = false;
    }
    open_tab();
}

pub unsafe fn init_at(dir: &[u8]) {
    init();
    if TAB_COUNT > 0 && dir.len() > 0 {
        let mut cmd = [0u8; 1100];
        let mut len = 0;
        let prefix = b"cd '";
        cmd[..prefix.len()].copy_from_slice(prefix);
        len += prefix.len();
        let dlen = dir.len().min(1024);
        cmd[len..len + dlen].copy_from_slice(&dir[..dlen]);
        len += dlen;
        cmd[len] = b'\'';
        len += 1;
        cmd[len] = b'\n';
        len += 1;
        let mut written = 0u32;
        WinluxWritePty(TABS[ACTIVE_TAB].pty, cmd.as_ptr(), len as u32, &mut written);
    }
}

/// Lecture non-bloquante de tous les PTY actifs. Renvoie vrai si l'écran a changé.
pub unsafe fn poll() -> bool {
    let mut changed = false;
    for i in 0..MAX_TABS {
        if !TABS[i].active {
            continue;
        }
        let mut buf = [0u8; 4096];
        let mut total_read = 0u32;
        loop {
            let mut n = 0u32;
            if WinluxReadPty(TABS[i].pty, buf.as_mut_ptr(), buf.len() as u32, &mut n) == 0
                || n == 0
            {
                break;
            }
            for j in 0..n as usize {
                TABS[i].screen.process(buf[j]);
            }
            total_read += n;
            if total_read > 16384 {
                break;
            }
        }
        if total_read > 0 {
            TABS[i].screen.view_off = 0;
            changed = true;
        }
        if TABS[i].pty != 0 && WinluxPtyAlive(TABS[i].pty) == 0 {
            close_tab(i);
            changed = true;
        }
    }
    changed
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    ensure_font();
    let _p = pal();
    let cl = f.client;
    let vis_cols = ((cl.right - cl.left) / CELL_W).max(1) as usize;
    let vis_rows = ((cl.bottom - cl.top) / CELL_H).max(1) as usize;

    let new_cols = vis_cols.min(MAX_COLS);
    let new_rows = vis_rows.min(MAX_ROWS);
    if new_cols != LAST_COLS || new_rows != LAST_ROWS {
        LAST_COLS = new_cols;
        LAST_ROWS = new_rows;
        for i in 0..MAX_TABS {
            if TABS[i].active {
                TABS[i].screen.resize(new_cols, new_rows);
                WinluxResizePty(TABS[i].pty, new_cols as u32, new_rows as u32);
            }
        }
    }

    if TAB_COUNT == 0 {
        fill(g.hdc, g.b, cl.left, cl.top, cl.right, cl.bottom, term_color(0));
        return;
    }

    let scr = &TABS[ACTIVE_TAB].screen;
    let bg_base = term_color(DEFAULT_BG);
    fill(g.hdc, g.b, cl.left, cl.top, cl.right, cl.bottom, bg_base);

    let old_bk = SetBkMode(g.hdc, 2);
    let mut char_buf = [0u8; 1];

    for row in 0..scr.rows.min(vis_rows) {
        let y = cl.top + (row as i32) * CELL_H;
        if y + CELL_H <= cl.top || y >= cl.bottom {
            continue;
        }
        let mut col = 0;
        while col < scr.cols.min(vis_cols) {
            let cell = &scr.cells[row][col];
            let (mut fg_c, mut bg_c) = (cell.fg, cell.bg);
            if cell.attr & ATTR_REVERSE != 0 {
                core::mem::swap(&mut fg_c, &mut bg_c);
            }
            if cell.attr & ATTR_BOLD != 0 && fg_c < 8 {
                fg_c += 8;
            }
            let fg_rgb = term_color(fg_c);
            let bg_rgb = term_color(bg_c);

            let selected = HAS_SEL && in_selection(col, row);
            let (draw_fg, draw_bg) = if selected {
                (term_color(DEFAULT_BG), rgb(58, 150, 221))
            } else {
                (fg_rgb, bg_rgb)
            };

            let x = cl.left + (col as i32) * CELL_W;

            let mut run_len = 1usize;
            while col + run_len < scr.cols.min(vis_cols) {
                let nc = &scr.cells[row][col + run_len];
                let (mut nfg, mut nbg) = (nc.fg, nc.bg);
                if nc.attr & ATTR_REVERSE != 0 {
                    core::mem::swap(&mut nfg, &mut nbg);
                }
                if nc.attr & ATTR_BOLD != 0 && nfg < 8 {
                    nfg += 8;
                }
                let ns = HAS_SEL && in_selection(col + run_len, row);
                let (ndf, ndb) = if ns {
                    (term_color(DEFAULT_BG), rgb(58, 150, 221))
                } else {
                    (term_color(nfg), term_color(nbg))
                };
                if ndf != draw_fg || ndb != draw_bg || (nc.attr & ATTR_BOLD) != (cell.attr & ATTR_BOLD) {
                    break;
                }
                run_len += 1;
            }

            if draw_bg != bg_base {
                fill(
                    g.hdc,
                    g.b,
                    x,
                    y,
                    x + (run_len as i32) * CELL_W,
                    y + CELL_H,
                    draw_bg,
                );
            }

            SetBkColor(g.hdc, draw_bg);
            let font = if cell.attr & ATTR_BOLD != 0 {
                BOLD_FONT
            } else {
                FONT
            };
            SelectObject(g.hdc, font);
            SetTextColor(g.hdc, draw_fg);
            for k in 0..run_len {
                let c = scr.cells[row][col + k].ch;
                if c > b' ' {
                    char_buf[0] = c;
                    TextOutA(
                        g.hdc,
                        x + (k as i32) * CELL_W,
                        y,
                        char_buf.as_ptr() as *const _,
                        1,
                    );
                }
            }

            if cell.attr & ATTR_UNDERLINE != 0 {
                let uy = y + CELL_H - 2;
                fill(
                    g.hdc,
                    g.b,
                    x,
                    uy,
                    x + (run_len as i32) * CELL_W,
                    uy + 1,
                    draw_fg,
                );
            }

            col += run_len;
        }
    }

    if scr.cursor_vis && TABS[ACTIVE_TAB].active {
        let cx = cl.left + (scr.cx as i32) * CELL_W;
        let cy = cl.top + (scr.cy as i32) * CELL_H;
        if cx < cl.right && cy < cl.bottom {
            fill(g.hdc, g.b, cx, cy, cx + CELL_W, cy + CELL_H, rgb(204, 204, 204));
            if scr.cy < MAX_ROWS && scr.cx < MAX_COLS {
                let under = scr.cells[scr.cy][scr.cx].ch;
                if under > b' ' {
                    char_buf[0] = under;
                    SelectObject(g.hdc, FONT);
                    SetTextColor(g.hdc, term_color(DEFAULT_BG));
                    SetBkColor(g.hdc, rgb(204, 204, 204));
                    TextOutA(g.hdc, cx, cy, char_buf.as_ptr() as *const _, 1);
                }
            }
        }
    }

    SetBkMode(g.hdc, old_bk as i32);

    draw_tabs(g, f);
}

unsafe fn draw_tabs(g: &mut Gfx, f: &Frame) {
    let tl = f.title;
    let p = pal();
    let tab_y = tl.top;
    let tab_h = tl.bottom - tl.top;

    let mut x = tl.left + 8;
    let mut idx = 0;
    for i in 0..MAX_TABS {
        if !TABS[i].active {
            continue;
        }
        let sel = i == ACTIVE_TAB;
        let hover = HOVER_TAB == idx;
        let bg = if sel {
            term_color(DEFAULT_BG)
        } else if hover {
            p.hover
        } else {
            p.surface
        };
        let fg = if sel {
            rgb(204, 204, 204)
        } else {
            p.text
        };

        let tw = TAB_W.min(tl.right - x - 40);
        if tw < 40 {
            break;
        }
        fill(g.hdc, g.b, x, tab_y + 4, x + tw, tab_y + tab_h, bg);
        if sel {
            fill(g.hdc, g.b, x, tab_y + 2, x + tw, tab_y + 4, p.accent);
        }

        let label = tab_title(i);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            fg,
            x + 10,
            tab_y + (tab_h - 13) / 2,
            label,
            tw - 30,
        );

        let close_x = x + tw - 22;
        let close_y = tab_y + (tab_h - TAB_CLOSE) / 2;
        let close_hover = HOVER_CLOSE == idx;
        if close_hover {
            fill(
                g.hdc,
                g.b,
                close_x - 2,
                close_y - 2,
                close_x + TAB_CLOSE + 2,
                close_y + TAB_CLOSE + 2,
                if sel { rgb(60, 60, 60) } else { p.pressed },
            );
        }
        draw_close_icon(g, close_x, close_y, TAB_CLOSE, if sel { rgb(180, 180, 180) } else { p.text2 });

        x += tw + TAB_GAP;
        idx += 1;
    }

    if TAB_COUNT < MAX_TABS {
        let plus_w = 28;
        let plus_bg = if HOVER_NEW { p.hover } else { p.surface };
        fill(g.hdc, g.b, x, tab_y + 6, x + plus_w, tab_y + tab_h - 2, plus_bg);
        let plus_fg = p.text2;
        text(g.hdc, g.f.icon, plus_fg, x + 7, tab_y + (tab_h - 14) / 2, b"+");
    }
}

unsafe fn draw_close_icon(g: &mut Gfx, x: i32, y: i32, s: i32, color: u32) {
    let m = s / 4;
    for d in 0..s - 2 * m {
        let px = x + m + d;
        let py1 = y + m + d;
        let py2 = y + s - m - 1 - d;
        fill(g.hdc, g.b, px, py1, px + 1, py1 + 1, color);
        fill(g.hdc, g.b, px, py2, px + 1, py2 + 1, color);
    }
}

fn tab_title(idx: usize) -> &'static [u8] {
    unsafe {
        let scr = &TABS[idx].screen;
        if scr.title_len > 0 {
            &scr.title[..scr.title_len]
        } else {
            b"Terminal"
        }
    }
}

unsafe fn in_selection(col: usize, row: usize) -> bool {
    let (sc, sr) = SEL_START;
    let (ec, er) = SEL_END;
    let (s, e) = if (sr, sc) <= (er, ec) {
        ((sc, sr), (ec, er))
    } else {
        ((ec, er), (sc, sr))
    };
    if row < s.1 || row > e.1 {
        return false;
    }
    if row == s.1 && row == e.1 {
        return col >= s.0 && col <= e.0;
    }
    if row == s.1 {
        return col >= s.0;
    }
    if row == e.1 {
        return col <= e.0;
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let tl = f.title;
    let cl = f.client;

    if y >= tl.top && y < tl.bottom {
        return mouse_tabs(ev, x, y, f);
    }

    if y >= cl.top && y < cl.bottom && x >= cl.left && x < cl.right {
        let col = ((x - cl.left) / CELL_W) as usize;
        let row = ((y - cl.top) / CELL_H) as usize;

        match ev {
            Mouse::Down => {
                SELECTING = true;
                HAS_SEL = false;
                SEL_START = (col, row);
                SEL_END = (col, row);
                return true;
            }
            Mouse::Move if SELECTING => {
                SEL_END = (col, row);
                HAS_SEL = SEL_START != SEL_END;
                return true;
            }
            Mouse::Up if SELECTING => {
                SELECTING = false;
                SEL_END = (col, row);
                HAS_SEL = SEL_START != SEL_END;
                return true;
            }
            Mouse::RightDown => {
                if HAS_SEL {
                    copy_selection();
                    HAS_SEL = false;
                    return true;
                }
                paste_clipboard();
                return true;
            }
            Mouse::Wheel(delta) => {
                let scr = &mut TABS[ACTIVE_TAB].screen;
                if delta > 0 {
                    scr.view_off = (scr.view_off + 3).min(scr.scroll_len);
                } else {
                    scr.view_off = scr.view_off.saturating_sub(3);
                }
                return true;
            }
            _ => {}
        }
    }
    false
}

unsafe fn mouse_tabs(ev: Mouse, x: i32, _y: i32, f: &Frame) -> bool {
    let tl = f.title;
    let mut tx = tl.left + 8;
    let _tab_h = tl.bottom - tl.top;
    let old_hover = HOVER_TAB;
    let old_close = HOVER_CLOSE;
    let old_new = HOVER_NEW;
    HOVER_TAB = -1;
    HOVER_CLOSE = -1;
    HOVER_NEW = false;

    let mut idx = 0i32;
    for i in 0..MAX_TABS {
        if !TABS[i].active {
            continue;
        }
        let tw = TAB_W.min(tl.right - tx - 40);
        if tw < 40 {
            break;
        }
        if x >= tx && x < tx + tw {
            HOVER_TAB = idx;
            let close_x = tx + tw - 22;
            if x >= close_x && x < close_x + TAB_CLOSE + 4 {
                HOVER_CLOSE = idx;
            }
        }
        tx += tw + TAB_GAP;
        idx += 1;
    }

    if TAB_COUNT < MAX_TABS && x >= tx && x < tx + 28 {
        HOVER_NEW = true;
    }

    match ev {
        Mouse::Down => {
            if HOVER_CLOSE >= 0 {
                let mut real = 0usize;
                let mut cnt = 0i32;
                for i in 0..MAX_TABS {
                    if TABS[i].active {
                        if cnt == HOVER_CLOSE {
                            real = i;
                            break;
                        }
                        cnt += 1;
                    }
                }
                close_tab(real);
                HOVER_TAB = -1;
                HOVER_CLOSE = -1;
                return true;
            }
            if HOVER_NEW {
                open_tab();
                return true;
            }
            if HOVER_TAB >= 0 {
                let mut cnt = 0i32;
                for i in 0..MAX_TABS {
                    if TABS[i].active {
                        if cnt == HOVER_TAB {
                            ACTIVE_TAB = i;
                            break;
                        }
                        cnt += 1;
                    }
                }
                return true;
            }
            return false;
        }
        Mouse::Move => {
            return HOVER_TAB != old_hover || HOVER_CLOSE != old_close || HOVER_NEW != old_new;
        }
        _ => {}
    }
    false
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if TAB_COUNT == 0 || !TABS[ACTIVE_TAB].active {
        return false;
    }
    let pty = TABS[ACTIVE_TAB].pty;
    if pty == 0 {
        return false;
    }

    HAS_SEL = false;
    let mut buf = [0u8; 8];
    let mut len = 0usize;

    if ch != 0 {
        let c = ch as u8;
        if ch == 22 {
            paste_clipboard();
            return true;
        }
        if ch == 3 {
            buf[0] = 3;
            len = 1;
        } else if ch < 256 {
            buf[0] = c;
            len = 1;
        }
    } else {
        match vk {
            0x26 => {
                buf[..3].copy_from_slice(b"\x1b[A");
                len = 3;
            }
            0x28 => {
                buf[..3].copy_from_slice(b"\x1b[B");
                len = 3;
            }
            0x27 => {
                buf[..3].copy_from_slice(b"\x1b[C");
                len = 3;
            }
            0x25 => {
                buf[..3].copy_from_slice(b"\x1b[D");
                len = 3;
            }
            0x24 => {
                buf[..4].copy_from_slice(b"\x1b[1~");
                len = 4;
            }
            0x23 => {
                buf[..4].copy_from_slice(b"\x1b[4~");
                len = 4;
            }
            0x2E => {
                buf[..4].copy_from_slice(b"\x1b[3~");
                len = 4;
            }
            0x21 => {
                buf[..4].copy_from_slice(b"\x1b[5~");
                len = 4;
            }
            0x22 => {
                buf[..4].copy_from_slice(b"\x1b[6~");
                len = 4;
            }
            0x70..=0x7B => {
                let n = vk - 0x70 + 1;
                if n <= 5 {
                    buf[0] = 0x1b;
                    buf[1] = b'[';
                    buf[2] = b'1';
                    len = 3;
                    if n > 1 {
                        buf[len] = b'0' + (n as u8 - 1);
                        len += 1;
                    }
                    buf[len] = b'~';
                    len += 1;
                }
            }
            0x09 => {
                buf[0] = b'\t';
                len = 1;
            }
            _ => {}
        }
    }

    if len > 0 {
        let mut written = 0u32;
        WinluxWritePty(pty, buf.as_ptr(), len as u32, &mut written);
        return true;
    }
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    poll()
}

unsafe fn copy_selection() {
    if !HAS_SEL || TAB_COUNT == 0 {
        return;
    }
    let scr = &TABS[ACTIVE_TAB].screen;
    let (s, e) = if (SEL_START.1, SEL_START.0) <= (SEL_END.1, SEL_END.0) {
        (SEL_START, SEL_END)
    } else {
        (SEL_END, SEL_START)
    };

    let mut buf = [0u8; 8192];
    let mut len = 0;
    for row in s.1..=e.1.min(scr.rows.saturating_sub(1)) {
        let start_col = if row == s.1 { s.0 } else { 0 };
        let end_col = if row == e.1 {
            e.0
        } else {
            scr.cols.saturating_sub(1)
        };
        let mut last_nonspace = start_col;
        for col in start_col..=end_col.min(scr.cols.saturating_sub(1)) {
            if scr.cells[row][col].ch != b' ' {
                last_nonspace = col + 1;
            }
        }
        for col in start_col..last_nonspace {
            if len < buf.len() {
                buf[len] = scr.cells[row][col].ch;
                len += 1;
            }
        }
        if row < e.1 && len < buf.len() {
            buf[len] = b'\n';
            len += 1;
        }
    }
    if len > 0 {
        let mem = GlobalAlloc(0x0042, len + 1);
        if !mem.is_null() {
            let ptr = GlobalLock(mem) as *mut u8;
            if !ptr.is_null() {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, len);
                *ptr.add(len) = 0;
                GlobalUnlock(mem);
                OpenClipboard(core::ptr::null_mut());
                EmptyClipboard();
                SetClipboardData(1, mem as usize);
                CloseClipboard();
            }
        }
    }
}

unsafe fn paste_clipboard() {
    if TAB_COUNT == 0 || !TABS[ACTIVE_TAB].active {
        return;
    }
    let pty = TABS[ACTIVE_TAB].pty;
    if pty == 0 {
        return;
    }
    OpenClipboard(core::ptr::null_mut());
    let data = GetClipboardData(1);
    if data != 0 {
        let ptr = GlobalLock(data as Handle) as *const u8;
        if !ptr.is_null() {
            let mut len = 0;
            while *ptr.add(len) != 0 && len < 8192 {
                len += 1;
            }
            if len > 0 {
                let mut written = 0u32;
                WinluxWritePty(pty, ptr, len as u32, &mut written);
            }
            GlobalUnlock(data as Handle);
        }
    }
    CloseClipboard();
}

// Clipboard FFI (from user32 — declared here to avoid modifying lib.rs too much)
#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    fn OpenClipboard(owner: Handle) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, mem: usize) -> usize;
    fn GetClipboardData(format: u32) -> usize;
}
