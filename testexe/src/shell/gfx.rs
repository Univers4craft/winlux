//! Primitives de dessin partagées par le bureau, le gestionnaire de fenêtres
//! et les applications intégrées (Explorer, Paramètres) : pinceaux et polices
//! d'une peinture, formes arrondies et icônes vectorielles.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

use winlux_testexe::*;

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 { r | (g << 8) | (b << 16) }

/// Pinceaux d'une peinture, libérés ensemble à la fin.
pub struct Brushes { items: [(u32, Handle); 192], len: usize }
impl Brushes {
    pub fn new() -> Self { Brushes { items: [(0, core::ptr::null_mut()); 192], len: 0 } }
    pub unsafe fn get(&mut self, color: u32) -> Handle {
        for i in 0..self.len { if self.items[i].0 == color { return self.items[i].1; } }
        let brush = CreateSolidBrush(color);
        if self.len == self.items.len() { DeleteObject(self.items[self.len - 1].1); self.len -= 1; }
        self.items[self.len] = (color, brush);
        self.len += 1;
        brush
    }
}
impl Brushes {
    pub unsafe fn release(&mut self) { for i in 0..self.len { DeleteObject(self.items[i].1); } self.len = 0; }
}

pub struct Fonts { pub title: Handle, pub body: Handle, pub semibold: Handle, pub small: Handle, pub icon: Handle, pub large: Handle, pub caption: Handle, pub display: Handle }
impl Fonts {
    pub unsafe fn new() -> Self {
        let f = |h: i32, w: i32| CreateFontA(h, 0, 0, 0, w, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr());
        Fonts { title: f(15, 600), body: f(13, 400), semibold: f(13, 600), small: f(11, 400), icon: f(14, 700), large: f(22, 600), caption: f(12, 400), display: f(28, 300) }
    }
}
impl Fonts {
    pub unsafe fn release(&self) { for f in [self.title, self.body, self.semibold, self.small, self.icon, self.large, self.caption, self.display] { DeleteObject(f); } }
}

pub unsafe fn text(hdc: Handle, font: Handle, color: u32, x: i32, y: i32, s: &[u8]) {
    SelectObject(hdc, font);
    SetTextColor(hdc, color);
    TextOutA(hdc, x, y, s.as_ptr() as *const core::ffi::c_char, s.len() as i32);
}
/// Largeur approximative d'un texte (DejaVu/Segoe : ~0,56 em par glyphe).
static mut MEASURE_DC: Handle = core::ptr::null_mut();
static mut MEASURE_FONTS: [(i32, Handle); 8] = [(0, core::ptr::null_mut()); 8];

/// Largeur réelle d'un texte (police Segoe UI normale de cette hauteur),
/// mesurée par le runtime ; estimation à 0,56 em si la mesure échoue.
pub fn text_width(s: &[u8], height: i32) -> i32 {
    unsafe {
        if MEASURE_DC.is_null() { MEASURE_DC = CreateCompatibleDC(core::ptr::null_mut()); }
        let slot = match MEASURE_FONTS.iter().position(|&(h, _)| h == height) {
            Some(i) => i,
            None => match MEASURE_FONTS.iter().position(|&(h, _)| h == 0) {
                Some(i) => { MEASURE_FONTS[i] = (height, CreateFontA(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, c"Segoe UI".as_ptr())); i }
                None => return (s.len() as i32 * height * 56) / 100,
            },
        };
        SelectObject(MEASURE_DC, MEASURE_FONTS[slot].1);
        let mut size = [0i32; 2];
        if GetTextExtentPoint32A(MEASURE_DC, s.as_ptr() as *const core::ffi::c_char, s.len() as i32, &mut size) != 0 { size[0] }
        else { (s.len() as i32 * height * 56) / 100 }
    }
}
pub unsafe fn text_centered(hdc: Handle, font: Handle, height: i32, color: u32, cx: i32, y: i32, s: &[u8]) {
    text(hdc, font, color, cx - text_width(s, height) / 2, y, s);
}

pub unsafe fn fill(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    FillRect(hdc, &Rect { left: l, top: t, right: r, bottom: bt }, b.get(color));
}
pub unsafe fn ellipse(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    SelectObject(hdc, b.get(color));
    Ellipse(hdc, l, t, r, bt);
}
pub unsafe fn round_rect(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, radius: i32, color: u32) {
    let radius = radius.min((r - l) / 2).min((bt - t) / 2).max(1);
    fill(hdc, b, l + radius, t, r - radius, bt, color);
    fill(hdc, b, l, t + radius, r, bt - radius, color);
    ellipse(hdc, b, l, t, l + radius * 2, t + radius * 2, color);
    ellipse(hdc, b, r - radius * 2, t, r, t + radius * 2, color);
    ellipse(hdc, b, l, bt - radius * 2, l + radius * 2, bt, color);
    ellipse(hdc, b, r - radius * 2, bt - radius * 2, r, bt, color);
}
/// Surface flottante : ombre douce, liseré et fond.
pub unsafe fn panel(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, radius: i32, surface: u32, border: u32) {
    round_rect(hdc, b, l - 2, t + 2, r + 2, bt + 6, radius + 2, rgb(8, 14, 32));
    round_rect(hdc, b, l - 1, t, r + 1, bt + 3, radius + 1, rgb(14, 22, 44));
    round_rect(hdc, b, l, t, r, bt, radius, border);
    round_rect(hdc, b, l + 1, t + 1, r - 1, bt - 1, radius - 1, surface);
}

pub fn mix(a: u32, c: u32, t: i32, n: i32) -> u32 {
    let ch = |shift: u32| {
        let x = ((a >> shift) & 0xff) as i32;
        let y = ((c >> shift) & 0xff) as i32;
        (x + (y - x) * t / n.max(1)) as u32
    };
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}

// ---------- Thème ----------

/// Mode sombre (par défaut) ou clair, partagé par toutes les fenêtres.
pub static mut DARK: bool = true;
/// Couleur d'accentuation (COLORREF), bleu Windows par défaut.
pub static mut ACCENT: u32 = rgb(0x4C, 0xC2, 0xFF);

/// Palette des fenêtres façon Windows 11 (fond « Mica », surfaces, textes).
#[derive(Clone, Copy)]
pub struct Palette {
    pub window: u32, pub mica: u32, pub surface: u32, pub surface2: u32, pub border: u32,
    pub hover: u32, pub pressed: u32, pub selected: u32, pub text: u32, pub text2: u32,
    pub text3: u32, pub accent: u32, pub on_accent: u32, pub shadow: u32,
}

pub fn pal() -> Palette {
    let accent = unsafe { ACCENT };
    if unsafe { DARK } {
        Palette {
            window: rgb(0x20, 0x20, 0x20), mica: rgb(0x1C, 0x1F, 0x26), surface: rgb(0x2B, 0x2B, 0x2B),
            surface2: rgb(0x32, 0x32, 0x32), border: rgb(0x3D, 0x3D, 0x3D), hover: rgb(0x38, 0x3A, 0x3F),
            pressed: rgb(0x2E, 0x30, 0x34), selected: mix(rgb(0x2B, 0x2B, 0x2B), accent, 3, 10),
            text: rgb(0xF2, 0xF2, 0xF2), text2: rgb(0xC5, 0xC5, 0xC5), text3: rgb(0x8A, 0x8A, 0x8A),
            accent, on_accent: rgb(0x00, 0x00, 0x00), shadow: rgb(0x08, 0x0C, 0x18),
        }
    } else {
        let accent = mix(accent, rgb(0, 0x3E, 0x92), 6, 10);
        Palette {
            window: rgb(0xF9, 0xF9, 0xF9), mica: rgb(0xEE, 0xF1, 0xF8), surface: rgb(0xFF, 0xFF, 0xFF),
            surface2: rgb(0xF5, 0xF5, 0xF5), border: rgb(0xE0, 0xE0, 0xE0), hover: rgb(0xEA, 0xEA, 0xEA),
            pressed: rgb(0xE2, 0xE2, 0xE2), selected: mix(rgb(0xFF, 0xFF, 0xFF), accent, 2, 10),
            text: rgb(0x1A, 0x1A, 0x1A), text2: rgb(0x42, 0x42, 0x42), text3: rgb(0x80, 0x80, 0x80),
            accent, on_accent: rgb(0xFF, 0xFF, 0xFF), shadow: rgb(0x2A, 0x30, 0x40),
        }
    }
}

/// Trait épais entre deux points (petits carrés le long du segment).
pub unsafe fn line(hdc: Handle, b: &mut Brushes, x0: i32, y0: i32, x1: i32, y1: i32, t: i32, color: u32) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs()).max(1);
    if dx == 0 || dy == 0 {
        fill(hdc, b, x0.min(x1), y0.min(y1), x0.max(x1) + t, y0.max(y1) + t, color);
        return;
    }
    for i in 0..=n {
        let x = x0 + dx * i / n;
        let y = y0 + dy * i / n;
        fill(hdc, b, x, y, x + t, y + t, color);
    }
}

/// Anneau (cercle évidé) dessiné par deux disques.
pub unsafe fn ring(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, w: i32, color: u32, inside_color: u32) {
    ellipse(hdc, b, l, t, r, bt, color);
    ellipse(hdc, b, l + w, t + w, r - w, bt - w, inside_color);
}

/// Bouton plat arrondi : survol et appui comme dans Windows 11.
pub unsafe fn flat_button(hdc: Handle, b: &mut Brushes, r: &Rect, hover: bool, pressed: bool) {
    let p = pal();
    if pressed { round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 4, p.pressed); }
    else if hover { round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 4, p.hover); }
}

static mut GLYPH_BG: u32 = u32::MAX;
/// Fond sous les icônes évidées (par défaut le fond « Mica » des fenêtres).
pub fn glyph_bg() -> u32 { unsafe { if GLYPH_BG != u32::MAX { GLYPH_BG } else { pal().mica } } }
pub fn set_glyph_bg(color: Option<u32>) { unsafe { GLYPH_BG = color.unwrap_or(u32::MAX); } }

static mut GLYPH_FG: u32 = u32::MAX;
/// Couleur des icônes au trait (par défaut la couleur du texte).
pub fn glyph_fg() -> u32 { unsafe { if GLYPH_FG != u32::MAX { GLYPH_FG } else { pal().text } } }
pub fn set_glyph_fg(color: Option<u32>) { unsafe { GLYPH_FG = color.unwrap_or(u32::MAX); } }

pub fn inside(r: &Rect, x: i32, y: i32) -> bool { x >= r.left && x < r.right && y >= r.top && y < r.bottom }
pub const fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect { Rect { left: l, top: t, right: r, bottom: b } }

/// Texte tronqué avec « … » pour tenir dans `max_w` (estimation de largeur).
pub unsafe fn text_fit(hdc: Handle, font: Handle, height: i32, color: u32, x: i32, y: i32, s: &[u8], max_w: i32) {
    if text_width(s, height) <= max_w { text(hdc, font, color, x, y, s); return; }
    let mut buf = [0u8; 160];
    // Plus long préfixe qui tient avec « … » (recherche dichotomique).
    let (mut lo, mut hi) = (0usize, s.len().min(158));
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        buf[..mid].copy_from_slice(&s[..mid]);
        buf[mid] = 0x85;
        if text_width(&buf[..mid + 1], height) <= max_w { lo = mid; } else { hi = mid - 1; }
    }
    let keep = lo;
    buf[..keep].copy_from_slice(&s[..keep]);
    buf[keep] = 0x85; // « … » en Windows-1252
    text(hdc, font, color, x, y, &buf[..keep + 1]);
}

/// Courbe d'animation « ease-out » cubique, en millièmes (0..=1000).
pub fn ease_out(t: i32, duration: i32) -> i32 {
    let t = t.clamp(0, duration.max(1)) * 1000 / duration.max(1);
    let u = 1000 - t;
    1000 - u * u / 1000 * u / 1000
}

/// Sinus en millièmes pour un angle en degrés (approximation de Bhaskara).
pub fn sin_milli(deg: i32) -> i32 {
    let d = deg.rem_euclid(360);
    let (x, sign) = if d < 180 { (d, 1) } else { (d - 180, -1) };
    let p = x * (180 - x);
    sign * (4000 * p) / (40500 - p)
}
pub fn cos_milli(deg: i32) -> i32 { sin_milli(deg + 90) }

// ---------- Icônes vectorielles ----------

#[derive(Clone, Copy)]
pub enum Glyph {
    Start, Search, Folder, Browser, Terminal, Store, Settings, Computer, Trash, Notes, Photos, Music, Mail, Calc, Exe,
    Edge, Home, Desktop, Download, Video, Drive, LinuxDrive, Star, File, Image, Audio, Archive, Code, Pdf, Text,
    Back, Forward, Up, Refresh, Plus, Close, Minimize, Maximize, Restore, Cut, Copy, Paste, Delete, Rename, Sort, View, Split, Chevron, ChevronDown, Grid, List, Details, Tiles,
}

pub unsafe fn glyph(hdc: Handle, b: &mut Brushes, f: &Fonts, g: Glyph, x: i32, y: i32, s: i32) {
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
        Glyph::Edge => {
            ellipse(hdc, b, x, y, x + s, y + s, rgb(0x0B, 0x53, 0x9F));
            ellipse(hdc, b, x + s / 16, y + s / 3, x + s * 15 / 16, y + s * 31 / 32, rgb(0x1F, 0xB5, 0x7E));
            ellipse(hdc, b, x + s / 8, y + s / 8, x + s * 7 / 8, y + s * 3 / 4, rgb(0x13, 0x7C, 0xE0));
            ellipse(hdc, b, x + s * 3 / 8, y + s * 3 / 8, x + s * 13 / 16, y + s * 13 / 16, rgb(0x5E, 0xE0, 0xC0));
            ellipse(hdc, b, x + s * 7 / 16, y + s * 7 / 16, x + s * 3 / 4, y + s * 11 / 16, rgb(0x13, 0x7C, 0xE0));
        }
        Glyph::Home => {
            let c = rgb(0x4C, 0xA6, 0xF0);
            for k in 0..(s / 2) { fill(hdc, b, x + s / 2 - k, y + s / 10 + k, x + s / 2 + k, y + s / 10 + k + 1, c); }
            fill(hdc, b, x + s / 6, y + s / 2, x + s * 5 / 6, y + s * 9 / 10, c);
            fill(hdc, b, x + s * 5 / 12, y + s * 2 / 3, x + s * 7 / 12, y + s * 9 / 10, rgb(0x1E, 0x5F, 0xA8));
        }
        Glyph::Desktop => {
            round_rect(hdc, b, x, y + s / 8, x + s, y + s * 3 / 4, 2, rgb(0x2B, 0x7F, 0xDA));
            fill(hdc, b, x + 2, y + s / 8 + 2, x + s - 2, y + s * 3 / 4 - 2, rgb(0x5C, 0xB8, 0xFF));
            fill(hdc, b, x + s / 4, y + s * 7 / 8 - 2, x + s * 3 / 4, y + s * 7 / 8, rgb(0x9A, 0xA3, 0xAE));
        }
        Glyph::Download => {
            let c = rgb(0x2E, 0xA0, 0x5A);
            fill(hdc, b, x + s * 7 / 16, y, x + s * 9 / 16, y + s * 5 / 8, c);
            for k in 0..(s / 4) { fill(hdc, b, x + s / 4 + k, y + s * 3 / 8 + k, x + s * 3 / 4 - k, y + s * 3 / 8 + k + 1, c); }
            fill(hdc, b, x + s / 8, y + s * 7 / 8, x + s * 7 / 8, y + s, c);
        }
        Glyph::Video => {
            round_rect(hdc, b, x, y + s / 6, x + s, y + s * 5 / 6, 3, rgb(0x8E, 0x4E, 0xD8));
            for k in 0..(s / 4) { fill(hdc, b, x + s * 3 / 8 + k, y + s / 4 + k, x + s * 3 / 8 + k + 1, y + s * 3 / 4 - k, rgb(0xFF, 0xFF, 0xFF)); }
        }
        Glyph::Drive | Glyph::LinuxDrive => {
            round_rect(hdc, b, x, y + s / 3, x + s, y + s * 5 / 6, 3, rgb(0x6B, 0x73, 0x80));
            fill(hdc, b, x + 2, y + s / 3 + 2, x + s - 2, y + s / 2, rgb(0x9A, 0xA3, 0xAE));
            fill(hdc, b, x + s * 3 / 4, y + s * 2 / 3 - 1, x + s * 7 / 8, y + s * 2 / 3 + 2, rgb(0x5C, 0xE0, 0x7A));
            if matches!(g, Glyph::Drive) { glyph(hdc, b, f, Glyph::Start, x + s / 8, y + s / 8, s / 3); }
            else { ellipse(hdc, b, x + s / 8, y, x + s * 7 / 16, y + s * 3 / 8, rgb(0xF0, 0xA0, 0x20)); ellipse(hdc, b, x + s * 3 / 16, y + s / 8, x + s * 3 / 8, y + s * 5 / 16, rgb(0x20, 0x22, 0x28)); }
        }
        Glyph::Star => {
            let c = rgb(0xF4, 0xC4, 0x30);
            for k in 0..(s / 2) {
                let half = k * 2 / 3 + 1;
                fill(hdc, b, x + s / 2 - half, y + k, x + s / 2 + half, y + k + 1, c);
                fill(hdc, b, x + s / 2 - half, y + s - k - 1, x + s / 2 + half, y + s - k, c);
            }
            fill(hdc, b, x, y + s * 3 / 8, x + s, y + s * 5 / 8, c);
        }
        Glyph::File | Glyph::Image | Glyph::Audio | Glyph::Archive | Glyph::Code | Glyph::Pdf | Glyph::Text => {
            let (l, r) = (x + s / 6, x + s * 5 / 6);
            fill(hdc, b, l, y, r - s / 4, y + s, rgb(0xF4, 0xF6, 0xF9));
            fill(hdc, b, l, y + s / 4, r, y + s, rgb(0xF4, 0xF6, 0xF9));
            fill(hdc, b, r - s / 4, y, r - s / 4 + 1, y + s / 4, rgb(0xC8, 0xCD, 0xD4));
            fill(hdc, b, r - s / 4, y + s / 4 - 1, r, y + s / 4, rgb(0xC8, 0xCD, 0xD4));
            let w = r - l;
            match g {
                Glyph::Image => {
                    fill(hdc, b, l + w / 8, y + s * 2 / 5, r - w / 8, y + s * 7 / 8, rgb(0x3D, 0x9B, 0xF5));
                    ellipse(hdc, b, l + w / 5, y + s * 3 / 5, l + w * 4 / 5, y + s * 6 / 5, rgb(0x34, 0xC7, 0x59));
                    fill(hdc, b, l, y + s * 7 / 8, r, y + s, rgb(0xF4, 0xF6, 0xF9));
                }
                Glyph::Audio => {
                    ellipse(hdc, b, l + w / 4, y + s * 5 / 8, l + w / 2, y + s * 7 / 8, rgb(0xE8, 0x3E, 0x6C));
                    fill(hdc, b, l + w / 2 - 2, y + s * 3 / 8, l + w / 2, y + s * 3 / 4, rgb(0xE8, 0x3E, 0x6C));
                    fill(hdc, b, l + w / 2 - 2, y + s * 3 / 8, l + w * 3 / 4, y + s * 3 / 8 + 2, rgb(0xE8, 0x3E, 0x6C));
                }
                Glyph::Archive => {
                    fill(hdc, b, l + w / 2 - 2, y, l + w / 2 + 2, y + s * 5 / 8, rgb(0xE0, 0x9E, 0x2A));
                    fill(hdc, b, l + w / 2 - 3, y + s * 5 / 8, l + w / 2 + 3, y + s * 3 / 4, rgb(0x8A, 0x5A, 0x10));
                }
                Glyph::Code => text(hdc, f.small, rgb(0x2B, 0x7F, 0xDA), l + w / 8, y + s * 2 / 5, b"</>"),
                Glyph::Pdf => {
                    fill(hdc, b, l, y + s / 2, r, y + s * 3 / 4, rgb(0xD9, 0x3A, 0x2E));
                    if s >= 32 { text(hdc, f.small, rgb(0xFF, 0xFF, 0xFF), l + w / 8, y + s / 2 + 2, b"PDF"); }
                }
                Glyph::Text => {
                    for k in 0..4 { let ly = y + s * 3 / 8 + k * s / 7; fill(hdc, b, l + w / 6, ly, r - w / 6, ly + 1.max(s / 24), rgb(0x9A, 0xA3, 0xAE)); }
                }
                _ => {}
            }
        }
        Glyph::Back | Glyph::Forward | Glyph::Up => {
            let c = glyph_fg();
            let t = (s / 10).max(1);
            match g {
                Glyph::Back => { line(hdc, b, x + s / 8, y + s / 2, x + s * 7 / 8, y + s / 2, t, c); line(hdc, b, x + s / 8, y + s / 2, x + s / 2, y + s / 8, t, c); line(hdc, b, x + s / 8, y + s / 2, x + s / 2, y + s * 7 / 8, t, c); }
                Glyph::Forward => { line(hdc, b, x + s / 8, y + s / 2, x + s * 7 / 8, y + s / 2, t, c); line(hdc, b, x + s * 7 / 8 - t, y + s / 2, x + s / 2, y + s / 8, t, c); line(hdc, b, x + s * 7 / 8 - t, y + s / 2, x + s / 2, y + s * 7 / 8, t, c); }
                _ => { line(hdc, b, x + s / 2, y + s / 8, x + s / 2, y + s * 7 / 8, t, c); line(hdc, b, x + s / 2, y + s / 8, x + s / 8, y + s / 2, t, c); line(hdc, b, x + s / 2, y + s / 8, x + s * 7 / 8 - t, y + s / 2, t, c); }
            }
        }
        Glyph::Refresh => {
            
            ring(hdc, b, x + s / 8, y + s / 8, x + s * 7 / 8, y + s * 7 / 8, (s / 10).max(1), glyph_fg(), glyph_bg());
            fill(hdc, b, x + s / 2, y, x + s, y + s / 2, glyph_bg());
            for k in 0..(s / 4) { fill(hdc, b, x + s / 2 + k, y + s / 8 - s / 8 + k, x + s / 2 + k + 1, y + s / 2 - k, glyph_fg()); }
        }
        Glyph::Plus => { let c = glyph_fg(); let t = (s / 9).max(1); fill(hdc, b, x + s / 2 - t / 2, y + s / 8, x + s / 2 + (t + 1) / 2, y + s * 7 / 8, c); fill(hdc, b, x + s / 8, y + s / 2 - t / 2, x + s * 7 / 8, y + s / 2 + (t + 1) / 2, c); }
        Glyph::Close => { let c = glyph_fg(); let t = (s / 12).max(1); line(hdc, b, x + s / 6, y + s / 6, x + s * 5 / 6 - t, y + s * 5 / 6 - t, t, c); line(hdc, b, x + s * 5 / 6 - t, y + s / 6, x + s / 6, y + s * 5 / 6 - t, t, c); }
        Glyph::Minimize => fill(hdc, b, x + s / 6, y + s / 2, x + s * 5 / 6, y + s / 2 + 1, pal().text),
        Glyph::Maximize => { round_rect(hdc, b, x + s / 6, y + s / 6, x + s * 5 / 6, y + s * 5 / 6, 2, glyph_fg()); round_rect(hdc, b, x + s / 6 + 1, y + s / 6 + 1, x + s * 5 / 6 - 1, y + s * 5 / 6 - 1, 1, glyph_bg()); }
        Glyph::Restore => {
            
            round_rect(hdc, b, x + s / 3, y + s / 6, x + s * 5 / 6, y + s * 2 / 3, 2, glyph_fg()); round_rect(hdc, b, x + s / 3 + 1, y + s / 6 + 1, x + s * 5 / 6 - 1, y + s * 2 / 3 - 1, 1, glyph_bg());
            round_rect(hdc, b, x + s / 6, y + s / 3, x + s * 2 / 3, y + s * 5 / 6, 2, glyph_fg()); round_rect(hdc, b, x + s / 6 + 1, y + s / 3 + 1, x + s * 2 / 3 - 1, y + s * 5 / 6 - 1, 1, glyph_bg());
        }
        Glyph::Cut => {
            let p = pal(); let t = (s / 12).max(1);
            line(hdc, b, x + s / 4, y, x + s * 2 / 3, y + s * 2 / 3, t, glyph_fg()); line(hdc, b, x + s * 3 / 4, y, x + s / 3, y + s * 2 / 3, t, glyph_fg());
            ring(hdc, b, x, y + s / 2, x + s * 2 / 5, y + s * 9 / 10, t + 1, p.accent, glyph_bg()); ring(hdc, b, x + s * 3 / 5, y + s / 2, x + s, y + s * 9 / 10, t + 1, p.accent, glyph_bg());
        }
        Glyph::Copy => {
            
            round_rect(hdc, b, x + s / 3, y, x + s, y + s * 2 / 3, 2, glyph_fg()); round_rect(hdc, b, x + s / 3 + 1, y + 1, x + s - 1, y + s * 2 / 3 - 1, 1, glyph_bg());
            round_rect(hdc, b, x, y + s / 3, x + s * 2 / 3, y + s, 2, glyph_fg()); round_rect(hdc, b, x + 1, y + s / 3 + 1, x + s * 2 / 3 - 1, y + s - 1, 1, glyph_bg());
        }
        Glyph::Paste => {
            let p = pal();
            round_rect(hdc, b, x + s / 8, y + s / 8, x + s * 7 / 8, y + s, 2, p.text2); round_rect(hdc, b, x + s / 8 + 1, y + s / 8 + 1, x + s * 7 / 8 - 1, y + s - 1, 1, glyph_bg());
            round_rect(hdc, b, x + s / 3, y, x + s * 2 / 3, y + s / 4, 2, p.accent);
        }
        Glyph::Delete => {
            
            fill(hdc, b, x + s / 8, y + s / 6, x + s * 7 / 8, y + s / 6 + (s / 10).max(1), glyph_fg());
            fill(hdc, b, x + s * 3 / 8, y, x + s * 5 / 8, y + s / 6, glyph_fg());
            round_rect(hdc, b, x + s / 4, y + s / 3, x + s * 3 / 4, y + s, 2, glyph_fg()); round_rect(hdc, b, x + s / 4 + 1, y + s / 3 + 1, x + s * 3 / 4 - 1, y + s - 1, 1, glyph_bg());
        }
        Glyph::Rename => {
            let p = pal(); let t = (s / 10).max(1);
            line(hdc, b, x + s / 8, y + s * 7 / 8 - t, x + s * 3 / 4, y + s / 4, t + 1, glyph_fg());
            fill(hdc, b, x + s * 5 / 8, y + s / 8, x + s * 7 / 8, y + s * 3 / 8, p.accent);
        }
        Glyph::Sort => {
            let t = (s / 10).max(1);
            fill(hdc, b, x + s / 4, y + s / 8, x + s / 4 + t, y + s * 7 / 8, glyph_fg()); line(hdc, b, x + s / 4, y + s / 8, x, y + s * 3 / 8, t, glyph_fg()); line(hdc, b, x + s / 4, y + s / 8, x + s / 2, y + s * 3 / 8, t, glyph_fg());
            fill(hdc, b, x + s * 3 / 4, y + s / 8, x + s * 3 / 4 + t, y + s * 7 / 8, glyph_fg()); line(hdc, b, x + s * 3 / 4, y + s * 7 / 8 - t, x + s / 2, y + s * 5 / 8, t, glyph_fg()); line(hdc, b, x + s * 3 / 4, y + s * 7 / 8 - t, x + s - t, y + s * 5 / 8, t, glyph_fg());
        }
        Glyph::View | Glyph::Grid => {
            let c = glyph_fg(); let h = s * 3 / 8;
            for (ox, oy) in [(0, 0), (s - h, 0), (0, s - h), (s - h, s - h)] { round_rect(hdc, b, x + ox, y + oy, x + ox + h, y + oy + h, 2, c); }
        }
        Glyph::List => { let c = glyph_fg(); for k in 0..4 { let ly = y + s / 8 + k * s / 4; fill(hdc, b, x, ly, x + s / 6, ly + 2, c); fill(hdc, b, x + s / 4, ly, x + s, ly + 2, c); } }
        Glyph::Details => { let c = glyph_fg(); for k in 0..4 { let ly = y + s / 8 + k * s / 4; fill(hdc, b, x, ly, x + s * 3 / 8, ly + 2, c); fill(hdc, b, x + s / 2, ly, x + s * 3 / 4, ly + 2, c); fill(hdc, b, x + s * 13 / 16, ly, x + s, ly + 2, c); } }
        Glyph::Tiles => { let c = glyph_fg(); for k in 0..2 { let ly = y + k * s / 2 + s / 16; round_rect(hdc, b, x, ly, x + s * 3 / 8, ly + s * 3 / 8, 2, c); fill(hdc, b, x + s / 2, ly + s / 16, x + s, ly + s / 16 + 2, c); fill(hdc, b, x + s / 2, ly + s / 4, x + s * 3 / 4, ly + s / 4 + 2, c); } }
        Glyph::Split => { round_rect(hdc, b, x, y + s / 8, x + s, y + s * 7 / 8, 2, glyph_fg()); fill(hdc, b, x + 1, y + s / 8 + 1, x + s / 2 - 1, y + s * 7 / 8 - 1, glyph_bg()); fill(hdc, b, x + s / 2 + 1, y + s / 8 + 1, x + s - 1, y + s * 7 / 8 - 1, glyph_bg()); }
        Glyph::Chevron => { let c = pal().text2; let t = (s / 8).max(1); line(hdc, b, x + s / 3, y + s / 6, x + s * 2 / 3, y + s / 2, t, c); line(hdc, b, x + s * 2 / 3, y + s / 2, x + s / 3, y + s * 5 / 6, t, c); }
        Glyph::ChevronDown => { let c = pal().text2; let t = (s / 8).max(1); line(hdc, b, x + s / 6, y + s / 3, x + s / 2, y + s * 2 / 3, t, c); line(hdc, b, x + s / 2, y + s * 2 / 3, x + s * 5 / 6, y + s / 3, t, c); }

    }
}
