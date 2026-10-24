//! Bureau Winlux inspiré de Windows 11, dessiné entièrement par GDI invité :
//! fond « bloom », barre des tâches centrée ou alignée à gauche, menu
//! Démarrer flottant, zone de notification et icônes du bureau.
//!
//! Toute la mise en page découle de la taille réelle de l'écran : quand la
//! résolution change (fenêtre de VM redimensionnée), le bureau se réadapte.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

use winlux_testexe::*;

pub const TASKBAR_HEIGHT: i32 = 48;
const BUTTON: i32 = 44;
const BUTTON_GAP: i32 = 4;
const TASKBAR_BUTTONS: i32 = 7;

pub static mut SCREEN_W: i32 = 1280;
pub static mut SCREEN_H: i32 = 720;
pub static mut START_OPEN: bool = false;
pub static mut ALIGN_LEFT: bool = false;
pub static mut ACTIVE_BUTTON: i32 = -1;
pub static mut BOOT_SPLASH_UNTIL: u32 = 0;
pub static mut VOLUME_LEVEL: u32 = 78;
pub static mut BATTERY_LEVEL: u32 = 73;
pub static mut BATTERY_PRESENT: u32 = 0;
pub static mut NETWORK_CONNECTED: u32 = 1;
static mut ICON_X: [i32; 3] = [12, 12, 12];
static mut ICON_Y: [i32; 3] = [12, 108, 204];
static mut DRAGGED_ICON: i32 = -1;
static mut LAST_MINUTE: u16 = 99;

const fn rgb(r: u32, g: u32, b: u32) -> u32 { r | (g << 8) | (b << 16) }

/// Pinceaux d'une peinture, libérés ensemble à la fin.
struct Brushes { items: [(u32, Handle); 96], len: usize }
impl Brushes {
    fn new() -> Self { Brushes { items: [(0, core::ptr::null_mut()); 96], len: 0 } }
    unsafe fn get(&mut self, color: u32) -> Handle {
        for i in 0..self.len { if self.items[i].0 == color { return self.items[i].1; } }
        let brush = CreateSolidBrush(color);
        if self.len < self.items.len() { self.items[self.len] = (color, brush); self.len += 1; }
        brush
    }
}
impl Brushes {
    unsafe fn release(&mut self) { for i in 0..self.len { DeleteObject(self.items[i].1); } self.len = 0; }
}

struct Fonts { title: Handle, body: Handle, semibold: Handle, small: Handle, icon: Handle }
impl Fonts {
    unsafe fn new() -> Self {
        let f = |h: i32, w: i32| CreateFontA(h, 0, 0, 0, w, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
        Fonts { title: f(15, 600), body: f(13, 400), semibold: f(13, 600), small: f(11, 400), icon: f(14, 700) }
    }
}
impl Fonts {
    unsafe fn release(&self) { for f in [self.title, self.body, self.semibold, self.small, self.icon] { DeleteObject(f); } }
}

unsafe fn text(hdc: Handle, font: Handle, color: u32, x: i32, y: i32, s: &[u8]) {
    SelectObject(hdc, font);
    SetTextColor(hdc, color);
    TextOutA(hdc, x, y, s.as_ptr() as *const core::ffi::c_char, s.len() as i32);
}
/// Largeur approximative d'un texte (DejaVu/Segoe : ~0,56 em par glyphe).
fn text_width(s: &[u8], height: i32) -> i32 { (s.len() as i32 * height * 56) / 100 }
unsafe fn text_centered(hdc: Handle, font: Handle, height: i32, color: u32, cx: i32, y: i32, s: &[u8]) {
    text(hdc, font, color, cx - text_width(s, height) / 2, y, s);
}

unsafe fn fill(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    FillRect(hdc, &Rect { left: l, top: t, right: r, bottom: bt }, b.get(color));
}
unsafe fn ellipse(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    SelectObject(hdc, b.get(color));
    Ellipse(hdc, l, t, r, bt);
}
unsafe fn round_rect(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, radius: i32, color: u32) {
    let radius = radius.min((r - l) / 2).min((bt - t) / 2).max(1);
    fill(hdc, b, l + radius, t, r - radius, bt, color);
    fill(hdc, b, l, t + radius, r, bt - radius, color);
    ellipse(hdc, b, l, t, l + radius * 2, t + radius * 2, color);
    ellipse(hdc, b, r - radius * 2, t, r, t + radius * 2, color);
    ellipse(hdc, b, l, bt - radius * 2, l + radius * 2, bt, color);
    ellipse(hdc, b, r - radius * 2, bt - radius * 2, r, bt, color);
}
/// Surface flottante : ombre douce, liseré et fond.
unsafe fn panel(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, radius: i32, surface: u32, border: u32) {
    round_rect(hdc, b, l - 2, t + 2, r + 2, bt + 6, radius + 2, rgb(8, 14, 32));
    round_rect(hdc, b, l - 1, t, r + 1, bt + 3, radius + 1, rgb(14, 22, 44));
    round_rect(hdc, b, l, t, r, bt, radius, border);
    round_rect(hdc, b, l + 1, t + 1, r - 1, bt - 1, radius - 1, surface);
}

fn mix(a: u32, c: u32, t: i32, n: i32) -> u32 {
    let ch = |shift: u32| {
        let x = ((a >> shift) & 0xff) as i32;
        let y = ((c >> shift) & 0xff) as i32;
        (x + (y - x) * t / n.max(1)) as u32
    };
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}

// ---------- Géométrie partagée par le dessin et les clics ----------

struct Layout { w: i32, h: i32, bar_top: i32, buttons_x: i32, start: Rect }

fn layout() -> Layout {
    let (w, h) = unsafe { (SCREEN_W.max(640), SCREEN_H.max(400)) };
    let bar_top = h - TASKBAR_HEIGHT;
    let buttons_w = TASKBAR_BUTTONS * BUTTON + (TASKBAR_BUTTONS - 1) * BUTTON_GAP;
    let buttons_x = if unsafe { ALIGN_LEFT } { 12 } else { (w - buttons_w) / 2 };
    let sw = 640.min(w - 24);
    let sh = 600.min(bar_top - 24);
    let sx = if unsafe { ALIGN_LEFT } { 12 } else { (w - sw) / 2 };
    let st = bar_top - 12 - sh;
    Layout { w, h, bar_top, buttons_x, start: Rect { left: sx, top: st, right: sx + sw, bottom: st + sh } }
}

fn button_rect(l: &Layout, index: i32) -> Rect {
    let x = l.buttons_x + index * (BUTTON + BUTTON_GAP);
    Rect { left: x, top: l.bar_top + 2, right: x + BUTTON, bottom: l.bar_top + 2 + BUTTON }
}
fn inside(r: &Rect, x: i32, y: i32) -> bool { x >= r.left && x < r.right && y >= r.top && y < r.bottom }

// Contenu du menu Démarrer : zones cliquables calculées depuis le panneau.
fn align_toggle_rect(l: &Layout) -> Rect {
    let s = &l.start;
    Rect { left: s.right - 230, top: s.bottom - 48, right: s.right - 70, bottom: s.bottom - 16 }
}

// ---------- Icônes vectorielles ----------

#[derive(Clone, Copy)]
enum Glyph { Start, Search, Folder, Browser, Terminal, Store, Settings, Computer, Trash, Notes, Photos, Music, Mail, Calc, Exe }

unsafe fn glyph(hdc: Handle, b: &mut Brushes, f: &Fonts, g: Glyph, x: i32, y: i32, s: i32) {
    match g {
        Glyph::Start => {
            let gap = (s / 12).max(1);
            let c = (s - gap) / 2;
            fill(hdc, b, x, y, x + c, y + c, rgb(0x2B, 0xA4, 0xF2));
            fill(hdc, b, x + c + gap, y, x + s, y + c, rgb(0x45, 0xB8, 0xFF));
            fill(hdc, b, x, y + c + gap, x + c, y + s, rgb(0x16, 0x84, 0xE0));
            fill(hdc, b, x + c + gap, y + c + gap, x + s, y + s, rgb(0x2B, 0xA4, 0xF2));
        }
        Glyph::Search => {
            let r = s * 2 / 3;
            ellipse(hdc, b, x, y, x + r, y + r, rgb(0xF2, 0xF2, 0xF2));
            ellipse(hdc, b, x + s / 8, y + s / 8, x + r - s / 8, y + r - s / 8, rgb(0x2A, 0x2D, 0x34));
            let t = (s / 9).max(2);
            for k in 0..(s / 3) { fill(hdc, b, x + r - t + k - 1, y + r - t + k - 1, x + r + k, y + r + k, rgb(0xF2, 0xF2, 0xF2)); }
        }
        Glyph::Folder => {
            round_rect(hdc, b, x, y + s / 8, x + s / 2, y + s / 3, 2, rgb(0xE0, 0x9E, 0x2A));
            round_rect(hdc, b, x, y + s / 4, x + s, y + s * 7 / 8, 3, rgb(0xF4, 0xB8, 0x3E));
            fill(hdc, b, x, y + s / 3, x + s, y + s * 3 / 8, rgb(0xFF, 0xD1, 0x66));
        }
        Glyph::Browser => {
            ellipse(hdc, b, x, y, x + s, y + s, rgb(0x0F, 0x9D, 0x8A));
            ellipse(hdc, b, x + s / 8, y + s / 8, x + s * 7 / 8, y + s * 7 / 8, rgb(0x1A, 0x73, 0xE8));
            ellipse(hdc, b, x + s / 3, y + s / 4, x + s * 3 / 4, y + s * 2 / 3, rgb(0x7F, 0xE0, 0xD0));
            ellipse(hdc, b, x + s * 2 / 5, y + s / 3, x + s * 2 / 3, y + s * 3 / 5, rgb(0x1A, 0x73, 0xE8));
        }
        Glyph::Terminal => {
            round_rect(hdc, b, x, y + s / 8, x + s, y + s * 7 / 8, 3, rgb(0x5A, 0x5F, 0x6B));
            round_rect(hdc, b, x + 1, y + s / 8 + 1, x + s - 1, y + s * 7 / 8 - 1, 3, rgb(0x16, 0x18, 0x1D));
            text(hdc, f.icon, rgb(0xE6, 0xE6, 0xE6), x + s / 6, y + s / 4, b">_");
        }
        Glyph::Store => {
            round_rect(hdc, b, x + s / 8, y + s / 4, x + s * 7 / 8, y + s, 3, rgb(0x0F, 0x6C, 0xBD));
            ellipse(hdc, b, x + s / 3, y, x + s * 2 / 3, y + s / 2, rgb(0x0F, 0x6C, 0xBD));
            ellipse(hdc, b, x + s * 3 / 8, y + s / 12, x + s * 5 / 8, y + s * 5 / 12, rgb(0x20, 0x24, 0x2C));
            fill(hdc, b, x + s / 8, y + s / 4, x + s * 7 / 8, y + s / 2, rgb(0x0F, 0x6C, 0xBD));
            let c = s / 6;
            let (cx, cy) = (x + s / 2, y + s * 5 / 8);
            fill(hdc, b, cx - c - 1, cy - c - 1, cx - 1, cy - 1, rgb(0xF2, 0x50, 0x22));
            fill(hdc, b, cx + 1, cy - c - 1, cx + c + 1, cy - 1, rgb(0x7F, 0xBA, 0x00));
            fill(hdc, b, cx - c - 1, cy + 1, cx - 1, cy + c + 1, rgb(0x00, 0xA4, 0xEF));
            fill(hdc, b, cx + 1, cy + 1, cx + c + 1, cy + c + 1, rgb(0xFF, 0xB9, 0x00));
        }
        Glyph::Settings => {
            let c = s / 2;
            let t = s / 7;
            fill(hdc, b, x + c - t, y, x + c + t, y + s, rgb(0x9A, 0xA3, 0xAE));
            fill(hdc, b, x, y + c - t, x + s, y + c + t, rgb(0x9A, 0xA3, 0xAE));
            ellipse(hdc, b, x + s / 10, y + s / 10, x + s * 9 / 10, y + s * 9 / 10, rgb(0x9A, 0xA3, 0xAE));
            ellipse(hdc, b, x + s * 3 / 10, y + s * 3 / 10, x + s * 7 / 10, y + s * 7 / 10, rgb(0x3B, 0x40, 0x48));
        }
        Glyph::Computer => {
            round_rect(hdc, b, x, y + s / 10, x + s, y + s * 3 / 4, 3, rgb(0x3A, 0x3F, 0x48));
            fill(hdc, b, x + 2, y + s / 10 + 2, x + s - 2, y + s * 3 / 4 - 2, rgb(0x3D, 0x9B, 0xF5));
            fill(hdc, b, x + 2, y + s * 2 / 5, x + s - 2, y + s * 3 / 4 - 2, rgb(0x2B, 0x7F, 0xDA));
            fill(hdc, b, x + s * 2 / 5, y + s * 3 / 4, x + s * 3 / 5, y + s * 7 / 8, rgb(0x8C, 0x93, 0x9C));
            round_rect(hdc, b, x + s / 4, y + s * 7 / 8 - 1, x + s * 3 / 4, y + s, 2, rgb(0xB8, 0xBE, 0xC6));
        }
        Glyph::Trash => {
            round_rect(hdc, b, x + s / 6, y + s / 4, x + s * 5 / 6, y + s, 3, rgb(0xD8, 0xDE, 0xE6));
            fill(hdc, b, x + s / 10, y + s / 8, x + s * 9 / 10, y + s / 4, rgb(0xEE, 0xF1, 0xF5));
            fill(hdc, b, x + s * 2 / 5, y + s / 20, x + s * 3 / 5, y + s / 8, rgb(0xEE, 0xF1, 0xF5));
            for k in 1..4 { let lx = x + s / 6 + k * (s * 2 / 3) / 4; fill(hdc, b, lx - 1, y + s / 3, lx + 1, y + s * 9 / 10, rgb(0x9A, 0xA3, 0xAE)); }
        }
        Glyph::Notes => {
            round_rect(hdc, b, x + s / 8, y, x + s * 7 / 8, y + s, 3, rgb(0xF5, 0xF7, 0xFA));
            fill(hdc, b, x + s / 8, y, x + s * 7 / 8, y + s / 5, rgb(0x2B, 0x88, 0xD8));
            for k in 0..4 { let ly = y + s * 2 / 5 + k * s / 7; fill(hdc, b, x + s / 4, ly, x + s * 3 / 4, ly + 2, rgb(0x9A, 0xA3, 0xAE)); }
        }
        Glyph::Photos => {
            round_rect(hdc, b, x, y + s / 8, x + s, y + s * 7 / 8, 4, rgb(0x2F, 0x86, 0xD6));
            ellipse(hdc, b, x + s * 3 / 5, y + s / 4, x + s * 4 / 5, y + s * 9 / 20, rgb(0xFF, 0xD3, 0x4F));
            ellipse(hdc, b, x + s / 10, y + s / 2, x + s * 3 / 4, y + s * 6 / 5, rgb(0x34, 0xC7, 0x59));
            ellipse(hdc, b, x + s / 2, y + s * 3 / 5, x + s * 11 / 10, y + s * 6 / 5, rgb(0x2A, 0xA8, 0x4A));
            // Recadre les collines dans la vignette.
            fill(hdc, b, x - s / 4, y + s * 7 / 8, x + s * 5 / 4, y + s * 5 / 4, rgb(0x24, 0x27, 0x2F));
            fill(hdc, b, x + s, y, x + s * 5 / 4, y + s * 5 / 4, rgb(0x24, 0x27, 0x2F));
        }
        Glyph::Music => {
            round_rect(hdc, b, x, y, x + s, y + s, 5, rgb(0xE8, 0x3E, 0x6C));
            ellipse(hdc, b, x + s / 4, y + s / 2, x + s / 2, y + s * 3 / 4, rgb(0xFF, 0xFF, 0xFF));
            fill(hdc, b, x + s / 2 - 2, y + s / 5, x + s / 2, y + s * 5 / 8, rgb(0xFF, 0xFF, 0xFF));
            fill(hdc, b, x + s / 2 - 2, y + s / 5, x + s * 3 / 4, y + s / 5 + 3, rgb(0xFF, 0xFF, 0xFF));
        }
        Glyph::Mail => {
            round_rect(hdc, b, x, y + s / 5, x + s, y + s * 4 / 5, 3, rgb(0x10, 0x6E, 0xBE));
            for k in 0..(s / 2) { fill(hdc, b, x + k, y + s / 5 + k * 3 / 5, x + s - k, y + s / 5 + k * 3 / 5 + 2, rgb(0x5C, 0xB4, 0xFF)); }
        }
        Glyph::Calc => {
            round_rect(hdc, b, x + s / 8, y, x + s * 7 / 8, y + s, 3, rgb(0x3B, 0x40, 0x48));
            fill(hdc, b, x + s / 4, y + s / 8, x + s * 3 / 4, y + s / 3, rgb(0xC8, 0xE6, 0xC9));
            for r in 0..3 { for c in 0..3 {
                let bx = x + s / 4 + c * s / 6;
                let by = y + s * 5 / 12 + r * s / 6;
                fill(hdc, b, bx, by, bx + s / 9, by + s / 9, if c == 2 { rgb(0xFF, 0x98, 0x00) } else { rgb(0xC0, 0xC6, 0xCE) });
            } }
        }
        Glyph::Exe => {
            round_rect(hdc, b, x, y + s / 8, x + s, y + s * 7 / 8, 3, rgb(0x3B, 0x40, 0x48));
            fill(hdc, b, x + 1, y + s / 8 + 1, x + s - 1, y + s / 3, rgb(0x60, 0x6A, 0x78));
            glyph(hdc, b, f, Glyph::Start, x + s / 3, y + s * 2 / 5, s / 3);
        }
    }
}

// ---------- Fond d'écran « bloom » ----------

unsafe fn wallpaper(hdc: Handle, b: &mut Brushes, l: &Layout) {
    let top = rgb(0x06, 0x12, 0x33);
    let bottom = rgb(0x0C, 0x2E, 0x6E);
    let bands = 48;
    for i in 0..bands {
        let y0 = l.h * i / bands;
        let y1 = l.h * (i + 1) / bands;
        fill(hdc, b, 0, y0, l.w, y1, mix(top, bottom, i, bands - 1));
    }
    // Fleur stylisée : pétales verticaux en éventail, du plus sombre à
    // l'extérieur au plus clair au centre, éclairés depuis leur base.
    let r = l.h.min(l.w * 3 / 4) * 9 / 25;
    let (cx, base) = (l.w / 2, l.h * 3 / 5 + r / 3);
    let petals: [(i32, i32, i32, u32, u32); 5] = [
        (-r * 6 / 5, r * 2 / 5, r * 6 / 5, rgb(0x0E, 0x34, 0x82), rgb(0x30, 0x6C, 0xD0)),
        (r * 6 / 5, r * 2 / 5, r * 6 / 5, rgb(0x0E, 0x34, 0x82), rgb(0x30, 0x6C, 0xD0)),
        (-r * 3 / 5, r * 9 / 20, r * 8 / 5, rgb(0x15, 0x48, 0xA8), rgb(0x4E, 0x96, 0xF0)),
        (r * 3 / 5, r * 9 / 20, r * 8 / 5, rgb(0x15, 0x48, 0xA8), rgb(0x4E, 0x96, 0xF0)),
        (0, r / 2, r * 2, rgb(0x1E, 0x60, 0xD0), rgb(0xB4, 0xD8, 0xFF)),
    ];
    for (dx, rx, ry, outer, inner) in petals {
        let steps = 28;
        for k in 0..steps {
            let sx = rx * (steps - k) / steps;
            let sy = ry * (steps - k) / steps;
            let px = cx + dx;
            ellipse(hdc, b, px - sx, base - sy, px + sx, base, mix(outer, inner, k, steps - 1));
        }
    }
}

// ---------- Barre des tâches ----------

const TASKBAR_GLYPHS: [Glyph; 7] = [Glyph::Start, Glyph::Search, Glyph::Folder, Glyph::Browser, Glyph::Terminal, Glyph::Store, Glyph::Settings];

unsafe fn draw_wifi(hdc: Handle, b: &mut Brushes, x: i32, y: i32, on: bool) {
    let c = if on { rgb(0xF2, 0xF2, 0xF2) } else { rgb(0x7A, 0x80, 0x8A) };
    for k in 0..4 { let hgt = 4 + k * 3; fill(hdc, b, x + k * 4, y + 14 - hgt, x + k * 4 + 3, y + 14, c); }
}
unsafe fn draw_volume(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    fill(hdc, b, x, y + 5, x + 4, y + 10, c);
    for k in 0..5 { fill(hdc, b, x + 4 + k, y + 5 - k, x + 5 + k, y + 10 + k, c); }
    if level == 0 {
        for k in 0..6 { fill(hdc, b, x + 12 + k, y + 4 + k, x + 14 + k, y + 6 + k, c); fill(hdc, b, x + 17 - k, y + 4 + k, x + 19 - k, y + 6 + k, c); }
    } else {
        let bars = if level < 34 { 1 } else if level < 67 { 2 } else { 3 };
        for k in 0..bars { let hgt = 4 + k * 4; fill(hdc, b, x + 12 + k * 3, y + 7 - hgt / 2, x + 14 + k * 3, y + 8 + hgt / 2, c); }
    }
}
unsafe fn draw_battery(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    round_rect(hdc, b, x, y + 2, x + 22, y + 13, 2, c);
    fill(hdc, b, x + 1, y + 3, x + 21, y + 12, rgb(0x20, 0x22, 0x28));
    fill(hdc, b, x + 2, y + 4, x + 2 + (18 * level.min(100) as i32) / 100, y + 11, if level > 20 { c } else { rgb(0xFF, 0x60, 0x50) });
    fill(hdc, b, x + 22, y + 5, x + 24, y + 10, c);
}

unsafe fn two_digits(v: u16) -> [u8; 2] { [b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8] }

unsafe fn taskbar(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    fill(hdc, b, 0, l.bar_top, l.w, l.h, rgb(0x1C, 0x1F, 0x26));
    fill(hdc, b, 0, l.bar_top, l.w, l.bar_top + 1, rgb(0x34, 0x38, 0x42));
    for (i, g) in TASKBAR_GLYPHS.iter().enumerate() {
        let r = button_rect(l, i as i32);
        let active = (i == 0 && START_OPEN) || ACTIVE_BUTTON == i as i32;
        if active { round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, rgb(0x2E, 0x33, 0x3D)); }
        glyph(hdc, b, f, *g, r.left + 10, r.top + 10, 24);
        if i >= 2 && i <= 4 {
            // Indicateur d'application ouverte, comme sous Windows 11.
            let cx = (r.left + r.right) / 2;
            let half = if active { 8 } else { 3 };
            round_rect(hdc, b, cx - half, r.bottom - 3, cx + half, r.bottom, 1, if active { rgb(0x4C, 0xC2, 0xFF) } else { rgb(0x9A, 0xA3, 0xAE) });
        }
    }

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
    let date = [d0, d1, b'/', o0, o1, b'/', b'0' + (year / 1000 % 10) as u8, b'0' + (year / 100 % 10) as u8, b'0' + (year / 10 % 10) as u8, b'0' + (year % 10) as u8];
    let right = l.w - 12;
    text(hdc, f.body, rgb(0xF2, 0xF2, 0xF2), right - text_width(&clock, 13), l.bar_top + 8, &clock);
    text(hdc, f.body, rgb(0xF2, 0xF2, 0xF2), right - text_width(&date, 13), l.bar_top + 26, &date);
    let tray = right - 92 - 96;
    round_rect(hdc, b, tray - 4, l.bar_top + 6, tray + 88, l.bar_top + 42, 5, rgb(0x24, 0x28, 0x31));
    draw_wifi(hdc, b, tray + 6, l.bar_top + 17, NETWORK_CONNECTED != 0);
    draw_volume(hdc, b, tray + 30, l.bar_top + 17, VOLUME_LEVEL);
    if BATTERY_PRESENT != 0 { draw_battery(hdc, b, tray + 58, l.bar_top + 17, BATTERY_LEVEL); }
    else { text(hdc, f.small, rgb(0xC8, 0xCD, 0xD4), tray + 60, l.bar_top + 17, b"CA"); }
    text(hdc, f.semibold, rgb(0xC8, 0xCD, 0xD4), tray - 24, l.bar_top + 14, b"^");
}

// ---------- Menu Démarrer ----------

struct App { name: &'static [u8], glyph: Glyph }
const PINNED: [App; 12] = [
    App { name: b"Explorateur", glyph: Glyph::Folder },
    App { name: b"Navigateur", glyph: Glyph::Browser },
    App { name: b"Terminal", glyph: Glyph::Terminal },
    App { name: b"Logith\xE8que", glyph: Glyph::Store },
    App { name: b"Param\xE8tres", glyph: Glyph::Settings },
    App { name: b"Bloc-notes", glyph: Glyph::Notes },
    App { name: b"Photos", glyph: Glyph::Photos },
    App { name: b"Musique", glyph: Glyph::Music },
    App { name: b"Courrier", glyph: Glyph::Mail },
    App { name: b"Calculatrice", glyph: Glyph::Calc },
    App { name: b"Ce PC", glyph: Glyph::Computer },
    App { name: b"Programmes .exe", glyph: Glyph::Exe },
];
const RECOMMENDED: [(&[u8], &[u8], Glyph); 4] = [
    (b"Bienvenue dans Winlux", b"Prise en main", Glyph::Start),
    (b"Applications Windows", b"Ouvrez un .exe d'un double-clic", Glyph::Exe),
    (b"Documents", b"Dossier personnel Linux", Glyph::Folder),
    (b"Notes de version", b"Mises \xE0 jour du syst\xE8me", Glyph::Notes),
];

unsafe fn start_menu(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    let s = &l.start;
    let surface = rgb(0x24, 0x27, 0x2F);
    panel(hdc, b, s.left, s.top, s.right, s.bottom, 8, surface, rgb(0x3A, 0x3F, 0x4A));
    let pad = 28;
    let inner_l = s.left + pad;
    let inner_r = s.right - pad;

    // Recherche.
    round_rect(hdc, b, inner_l, s.top + 24, inner_r, s.top + 60, 18, rgb(0x1A, 0x1D, 0x23));
    round_rect(hdc, b, inner_l + 1, s.top + 25, inner_r - 1, s.top + 59, 17, rgb(0x2E, 0x32, 0x3B));
    fill(hdc, b, inner_l + 18, s.top + 57, inner_r - 18, s.top + 59, rgb(0x4C, 0xC2, 0xFF));
    glyph(hdc, b, f, Glyph::Search, inner_l + 14, s.top + 34, 16);
    text(hdc, f.body, rgb(0x9A, 0xA3, 0xAE), inner_l + 42, s.top + 34, b"Rechercher des applications, param\xE8tres et documents");

    // Épinglé.
    let mut y = s.top + 82;
    text(hdc, f.title, rgb(0xF2, 0xF2, 0xF2), inner_l + 4, y, b"\xC9pingl\xE9");
    let all = b"Toutes les applications  >";
    let all_w = text_width(all, 11) + 20;
    round_rect(hdc, b, inner_r - all_w, y - 4, inner_r, y + 22, 4, rgb(0x31, 0x35, 0x3F));
    text(hdc, f.small, rgb(0xE6, 0xE6, 0xE6), inner_r - all_w + 10, y + 2, all);
    y += 36;
    let cols = 6;
    let cell_w = (inner_r - inner_l) / cols;
    let cell_h = 84;
    let rows = if s.bottom - s.top > 560 { 2 } else { 1 };
    for (i, app) in PINNED.iter().take((cols * rows) as usize).enumerate() {
        let c = i as i32 % cols;
        let r = i as i32 / cols;
        let cx = inner_l + c * cell_w + cell_w / 2;
        let top = y + r * cell_h;
        glyph(hdc, b, f, app.glyph, cx - 16, top + 10, 32);
        text_centered(hdc, f.small, 11, rgb(0xE6, 0xE6, 0xE6), cx, top + 52, app.name);
    }
    y += rows * cell_h + 20;

    // Recommandé.
    if y + 150 < s.bottom - 72 {
        text(hdc, f.title, rgb(0xF2, 0xF2, 0xF2), inner_l + 4, y, b"Recommand\xE9");
        y += 34;
        let col_w = (inner_r - inner_l) / 2;
        for (i, (title, sub, g)) in RECOMMENDED.iter().enumerate() {
            let x = inner_l + (i as i32 % 2) * col_w;
            let top = y + (i as i32 / 2) * 56;
            glyph(hdc, b, f, *g, x + 8, top + 8, 28);
            text(hdc, f.body, rgb(0xF2, 0xF2, 0xF2), x + 48, top + 6, title);
            text(hdc, f.small, rgb(0x9A, 0xA3, 0xAE), x + 48, top + 24, sub);
        }
    }

    // Pied : utilisateur, alignement de la barre, alimentation.
    fill(hdc, b, s.left + 1, s.bottom - 64, s.right - 1, s.bottom - 1, rgb(0x1F, 0x22, 0x29));
    fill(hdc, b, s.left + 1, s.bottom - 64, s.right - 1, s.bottom - 63, rgb(0x34, 0x38, 0x42));
    let ay = s.bottom - 48;
    ellipse(hdc, b, inner_l + 6, ay, inner_l + 38, ay + 32, rgb(0x5B, 0x6B, 0xD8));
    text_centered(hdc, f.semibold, 13, rgb(0xFF, 0xFF, 0xFF), inner_l + 22, ay + 8, b"D");
    text(hdc, f.body, rgb(0xF2, 0xF2, 0xF2), inner_l + 50, ay + 8, b"Damien");
    let t = align_toggle_rect(l);
    round_rect(hdc, b, t.left, t.top, t.right, t.bottom, 4, rgb(0x31, 0x35, 0x3F));
    let label: &[u8] = if ALIGN_LEFT { b"Barre : \xE0 gauche" } else { b"Barre : centr\xE9e" };
    text_centered(hdc, f.small, 11, rgb(0xE6, 0xE6, 0xE6), (t.left + t.right) / 2, t.top + 9, label);
    // Bouton d'alimentation.
    let (px, py) = (inner_r - 30, ay + 4);
    ellipse(hdc, b, px, py, px + 24, py + 24, rgb(0xE6, 0xE6, 0xE6));
    ellipse(hdc, b, px + 3, py + 3, px + 21, py + 21, rgb(0x1F, 0x22, 0x29));
    fill(hdc, b, px + 8, py - 1, px + 16, py + 8, rgb(0x1F, 0x22, 0x29));
    fill(hdc, b, px + 11, py - 1, px + 13, py + 12, rgb(0xE6, 0xE6, 0xE6));
}

// ---------- Icônes du bureau ----------

const DESKTOP_ICONS: [(&[u8], Glyph); 3] = [(b"Ce PC", Glyph::Computer), (b"Documents", Glyph::Folder), (b"Corbeille", Glyph::Trash)];

unsafe fn desktop_icons(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    for (i, (name, g)) in DESKTOP_ICONS.iter().enumerate() {
        let (x, y) = (ICON_X[i], ICON_Y[i]);
        if DRAGGED_ICON == i as i32 { round_rect(hdc, b, x, y, x + 76, y + 84, 4, rgb(0x2A, 0x4E, 0x8C)); }
        glyph(hdc, b, f, *g, x + 18, y + 8, 40);
        text_centered(hdc, f.small, 11, rgb(0x00, 0x00, 0x00), x + 39, y + 59, name);
        text_centered(hdc, f.small, 11, rgb(0xFF, 0xFF, 0xFF), x + 38, y + 58, name);
    }
}

// ---------- Écran de démarrage ----------

unsafe fn boot_splash(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    fill(hdc, b, 0, 0, l.w, l.h, rgb(0, 0, 0));
    glyph(hdc, b, f, Glyph::Start, l.w / 2 - 40, l.h / 2 - 90, 80);
    let phase = (GetTickCount() / 120 % 8) as i32;
    for k in 0..5 {
        let x = l.w / 2 - 40 + k * 18 + phase;
        ellipse(hdc, b, x, l.h / 2 + 50, x + 6, l.h / 2 + 56, rgb(0xE6, 0xE6, 0xE6));
    }
}

// ---------- Points d'entrée ----------

/// Lit l'environnement et la taille de l'écran au lancement.
pub unsafe fn init(volume: u32, battery: u32, battery_present: u32, network: u32, align_left: bool) {
    VOLUME_LEVEL = volume;
    BATTERY_LEVEL = battery;
    BATTERY_PRESENT = battery_present;
    NETWORK_CONNECTED = network;
    ALIGN_LEFT = align_left;
    SCREEN_W = GetSystemMetrics(0).max(640);
    SCREEN_H = GetSystemMetrics(1).max(400);
    BOOT_SPLASH_UNTIL = GetTickCount().wrapping_add(1400);
}

pub unsafe fn splash_active() -> bool {
    BOOT_SPLASH_UNTIL != 0 && (GetTickCount().wrapping_sub(BOOT_SPLASH_UNTIL) as i32) < 0
}

pub unsafe fn draw(hdc: Handle) {
    let l = layout();
    let mut b = Brushes::new();
    let f = Fonts::new();
    SetBkMode(hdc, 1);
    if splash_active() {
        boot_splash(hdc, &mut b, &f, &l);
    } else {
        wallpaper(hdc, &mut b, &l);
        desktop_icons(hdc, &mut b, &f);
        if START_OPEN { start_menu(hdc, &mut b, &f, &l); }
        taskbar(hdc, &mut b, &f, &l);
    }
    f.release();
    b.release();
}

/// Appelé régulièrement par la boucle de messages : suit la résolution de
/// l'écran et rafraîchit l'horloge à chaque nouvelle minute.
pub unsafe fn tick(hwnd: Handle) {
    let (w, h) = (GetSystemMetrics(0).max(640), GetSystemMetrics(1).max(400));
    if w != SCREEN_W || h != SCREEN_H {
        SCREEN_W = w;
        SCREEN_H = h;
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
    if now.minute != LAST_MINUTE && !splash_active() {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

pub unsafe fn on_button_down(hwnd: Handle, x: i32, y: i32) {
    let l = layout();
    if y >= l.bar_top {
        for i in 0..TASKBAR_BUTTONS {
            if inside(&button_rect(&l, i), x, y) {
                if i == 0 { START_OPEN = !START_OPEN; } else { ACTIVE_BUTTON = if ACTIVE_BUTTON == i { -1 } else { i }; START_OPEN = false; }
                InvalidateRect(hwnd, core::ptr::null(), 0);
                return;
            }
        }
        let tray = l.w - 12 - 92 - 96;
        if x >= tray && x < tray + 24 { NETWORK_CONNECTED ^= 1; }
        else if x >= tray + 24 && x < tray + 52 { VOLUME_LEVEL = match VOLUME_LEVEL { 67..=100 => 45, 34..=66 => 18, 1..=33 => 0, _ => 82 }; }
        else { return; }
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    if START_OPEN {
        if inside(&align_toggle_rect(&l), x, y) {
            ALIGN_LEFT = !ALIGN_LEFT;
        } else if !inside(&l.start, x, y) {
            START_OPEN = false;
        } else {
            return;
        }
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    for i in (0..3).rev() {
        if x >= ICON_X[i] && x < ICON_X[i] + 76 && y >= ICON_Y[i] && y < ICON_Y[i] + 84 {
            DRAGGED_ICON = i as i32;
            InvalidateRect(hwnd, core::ptr::null(), 0);
            return;
        }
    }
}

pub unsafe fn on_mouse_move(hwnd: Handle, x: i32, y: i32) {
    if DRAGGED_ICON < 0 { return; }
    let l = layout();
    let i = DRAGGED_ICON as usize;
    // Grille de 96 px, comme l'alignement automatique de Windows.
    let gx = ((x - 38).max(0) + 48) / 96 * 96 + 12;
    let gy = ((y - 42).max(0) + 48) / 96 * 96 + 12;
    let nx = gx.min(l.w - 80);
    let ny = gy.min(l.bar_top - 90);
    if nx != ICON_X[i] || ny != ICON_Y[i] {
        ICON_X[i] = nx;
        ICON_Y[i] = ny;
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

pub unsafe fn on_button_up(hwnd: Handle) {
    if DRAGGED_ICON >= 0 {
        DRAGGED_ICON = -1;
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}
