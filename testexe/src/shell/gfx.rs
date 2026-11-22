//! Primitives de dessin partagées par le bureau, le gestionnaire de fenêtres
//! et les applications intégrées (Explorer, Paramètres) : pinceaux et polices
//! d'une peinture, formes arrondies et icônes vectorielles.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

use winlux_testexe::*;

/// Logo et identité visuelle Winlux.
#[path = "brand.rs"]
#[allow(dead_code)]
pub mod brand;

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
pub const fn rgba(r: u32, g: u32, b: u32, a: u32) -> u32 {
    r | (g << 8) | (b << 16) | (a << 24)
}
pub const fn alpha_of(c: u32) -> u32 {
    (c >> 24) & 0xFF
}
pub fn blend(fg: u32, bg: u32, a: i32) -> u32 {
    let a = a.clamp(0, 255);
    let inv = 255 - a;
    let ch =
        |s: u32| ((((fg >> s) & 0xFF) as i32 * a + ((bg >> s) & 0xFF) as i32 * inv) / 255) as u32;
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}

/// Pinceaux d'une peinture, libérés ensemble à la fin.
pub struct Brushes {
    items: [(u32, Handle); 192],
    len: usize,
}
impl Brushes {
    pub fn new() -> Self {
        Brushes {
            items: [(0, core::ptr::null_mut()); 192],
            len: 0,
        }
    }
    pub unsafe fn get(&mut self, color: u32) -> Handle {
        let color = chrome(color);
        let hash = (color.wrapping_mul(2654435761) >> 24) as usize;
        let start = hash % self.items.len();
        let mut i = start;
        loop {
            if i >= self.len {
                break;
            }
            if self.items[i].0 == color {
                return self.items[i].1;
            }
            i += 1;
            if i == self.items.len() {
                i = 0;
            }
            if i == start {
                break;
            }
        }
        for j in 0..self.len.min(start) {
            if self.items[j].0 == color {
                return self.items[j].1;
            }
        }
        let brush = CreateSolidBrush(color);
        if self.len == self.items.len() {
            DeleteObject(self.items[self.len - 1].1);
            self.len -= 1;
        }
        self.items[self.len] = (color, brush);
        self.len += 1;
        brush
    }
}
impl Brushes {
    pub unsafe fn release(&mut self) {
        for i in 0..self.len {
            DeleteObject(self.items[i].1);
        }
        self.len = 0;
    }
}

#[derive(Clone, Copy)]
pub struct Fonts {
    pub title: Handle,
    pub body: Handle,
    pub semibold: Handle,
    pub small: Handle,
    pub icon: Handle,
    pub large: Handle,
    pub caption: Handle,
    pub display: Handle,
}
impl Fonts {
    pub unsafe fn new() -> Self {
        let f = |h: i32, w: i32| {
            CreateFontA(
                h,
                0,
                0,
                0,
                w,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                c"DejaVu Sans".as_ptr(),
            )
        };
        Fonts {
            title: f(15, 600),
            body: f(13, 400),
            semibold: f(13, 600),
            small: f(11, 400),
            icon: f(14, 700),
            large: f(22, 600),
            caption: f(12, 400),
            display: f(28, 300),
        }
    }
}
impl Fonts {
    pub unsafe fn release(&self) {
        for f in [
            self.title,
            self.body,
            self.semibold,
            self.small,
            self.icon,
            self.large,
            self.caption,
            self.display,
        ] {
            DeleteObject(f);
        }
    }
}

pub unsafe fn text(hdc: Handle, font: Handle, color: u32, x: i32, y: i32, s: &[u8]) {
    SelectObject(hdc, font);
    SetTextColor(hdc, chrome(color));
    if SPAN_CAPTURE && !s.is_empty() {
        capture_span(hdc, x, y, s);
    }
    if AUDIT_ON {
        audit_text(hdc, chrome(color), x, y, s);
    }
    TextOutA(
        hdc,
        x,
        y,
        s.as_ptr() as *const core::ffi::c_char,
        s.len() as i32,
    );
}
/// Largeur approximative d'un texte (DejaVu/Segoe : ~0,56 em par glyphe).
static mut MEASURE_DC: Handle = core::ptr::null_mut();
static mut MEASURE_FONTS: [(i32, Handle); 8] = [(0, core::ptr::null_mut()); 8];

/// Largeur réelle d'un texte (police DejaVu Sans normale de cette hauteur),
/// mesurée par le runtime ; estimation à 0,56 em si la mesure échoue.
pub fn text_width(s: &[u8], height: i32) -> i32 {
    unsafe {
        if MEASURE_DC.is_null() {
            MEASURE_DC = CreateCompatibleDC(core::ptr::null_mut());
        }
        let slot = match MEASURE_FONTS.iter().position(|&(h, _)| h == height) {
            Some(i) => i,
            None => match MEASURE_FONTS.iter().position(|&(h, _)| h == 0) {
                Some(i) => {
                    MEASURE_FONTS[i] = (
                        height,
                        CreateFontA(
                            height,
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
                            0,
                            c"DejaVu Sans".as_ptr(),
                        ),
                    );
                    i
                }
                None => return (s.len() as i32 * height * 56) / 100,
            },
        };
        SelectObject(MEASURE_DC, MEASURE_FONTS[slot].1);
        let mut size = [0i32; 2];
        if GetTextExtentPoint32A(
            MEASURE_DC,
            s.as_ptr() as *const core::ffi::c_char,
            s.len() as i32,
            &mut size,
        ) != 0
        {
            size[0]
        } else {
            (s.len() as i32 * height * 56) / 100
        }
    }
}
pub unsafe fn text_centered(
    hdc: Handle,
    font: Handle,
    height: i32,
    color: u32,
    cx: i32,
    y: i32,
    s: &[u8],
) {
    text(hdc, font, color, cx - text_width(s, height) / 2, y, s);
}

pub unsafe fn fill(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    if AUDIT_ON {
        note_fill(hdc, l, t, r, bt, chrome(color));
    }
    FillRect(
        hdc,
        &Rect {
            left: l,
            top: t,
            right: r,
            bottom: bt,
        },
        b.get(color),
    );
}
pub unsafe fn ellipse(hdc: Handle, b: &mut Brushes, l: i32, t: i32, r: i32, bt: i32, color: u32) {
    if AUDIT_ON {
        note_fill(hdc, l, t, r, bt, chrome(color));
    }
    SelectObject(hdc, b.get(color));
    // Sans plume : la plume par défaut d'un DC est noire (comme sous
    // Windows) et cernait chaque disque d'un trait noir.
    SelectObject(hdc, GetStockObject(NULL_PEN));
    Ellipse(hdc, l, t, r, bt);
}
/// Objet de stock NULL_PEN (aucun contour).
const NULL_PEN: i32 = 8;

pub unsafe fn round_rect(
    hdc: Handle,
    b: &mut Brushes,
    l: i32,
    t: i32,
    r: i32,
    bt: i32,
    radius: i32,
    color: u32,
) {
    let radius = radius.min((r - l) / 2).min((bt - t) / 2).max(1);
    if AUDIT_ON {
        note_fill(hdc, l, t, r, bt, chrome(color));
    }
    SelectObject(hdc, b.get(color));
    // Sans plume : RoundRect trace maintenant le contour à la plume du DC
    // (noire par défaut, comme sous Windows), d'où un liseré noir partout.
    SelectObject(hdc, GetStockObject(NULL_PEN));
    // Le shell a été dessiné avec des coins de rayon 2 × radius ; RoundRect
    // prend maintenant la largeur de l'ellipse comme Windows (#866).
    RoundRect(hdc, l, t, r, bt, radius * 4, radius * 4);
}
/// Surface flottante : liseré fin et fond, sans ombre lourde.
pub unsafe fn panel(
    hdc: Handle,
    b: &mut Brushes,
    l: i32,
    t: i32,
    r: i32,
    bt: i32,
    radius: i32,
    surface: u32,
    border: u32,
) {
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

// ---------- Contraste du texte (WCAG 2) ----------

/// Composante sRGB linéarisée (× 1 000 000) : ((v/255 + 0,055) / 1,055)^2,4.
const LINEAR: [u32; 256] = [
    0, 304, 607, 911, 1214, 1518, 1821, 2125, 2428, 2732, 3035, 3347, 3677, 4025, 4391, 4777, 5182,
    5605, 6049, 6512, 6995, 7499, 8023, 8568, 9134, 9721, 10330, 10960, 11612, 12286, 12983, 13702,
    14444, 15209, 15996, 16807, 17642, 18500, 19382, 20289, 21219, 22174, 23153, 24158, 25187,
    26241, 27321, 28426, 29557, 30713, 31896, 33105, 34340, 35601, 36889, 38204, 39546, 40915,
    42311, 43735, 45186, 46665, 48172, 49707, 51269, 52861, 54480, 56128, 57805, 59511, 61246,
    63010, 64803, 66626, 68478, 70360, 72272, 74214, 76185, 78187, 80220, 82283, 84376, 86500,
    88656, 90842, 93059, 95307, 97587, 99899, 102242, 104616, 107023, 109462, 111932, 114435,
    116971, 119538, 122139, 124772, 127438, 130136, 132868, 135633, 138432, 141263, 144128, 147027,
    149960, 152926, 155926, 158961, 162029, 165132, 168269, 171441, 174647, 177888, 181164, 184475,
    187821, 191202, 194618, 198069, 201556, 205079, 208637, 212231, 215861, 219526, 223228, 226966,
    230740, 234551, 238398, 242281, 246201, 250158, 254152, 258183, 262251, 266356, 270498, 274677,
    278894, 283149, 287441, 291771, 296138, 300544, 304987, 309469, 313989, 318547, 323143, 327778,
    332452, 337164, 341914, 346704, 351533, 356400, 361307, 366253, 371238, 376262, 381326, 386429,
    391572, 396755, 401978, 407240, 412543, 417885, 423268, 428690, 434154, 439657, 445201, 450786,
    456411, 462077, 467784, 473531, 479320, 485150, 491021, 496933, 502886, 508881, 514918, 520996,
    527115, 533276, 539479, 545724, 552011, 558340, 564712, 571125, 577580, 584078, 590619, 597202,
    603827, 610496, 617207, 623960, 630757, 637597, 644480, 651406, 658375, 665387, 672443, 679542,
    686685, 693872, 701102, 708376, 715694, 723055, 730461, 737910, 745404, 752942, 760525, 768151,
    775822, 783538, 791298, 799103, 806952, 814847, 822786, 830770, 838799, 846873, 854993, 863157,
    871367, 879622, 887923, 896269, 904661, 913099, 921582, 930111, 938686, 947307, 955973, 964686,
    973445, 982251, 991102, 1000000,
];

/// Luminance relative d'une couleur COLORREF, × 1 000 000.
pub fn luminance(c: u32) -> u32 {
    let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
    ((LINEAR[r as usize] as u64 * 2126
        + LINEAR[g as usize] as u64 * 7152
        + LINEAR[b as usize] as u64 * 722)
        / 10_000) as u32
}

/// Rapport de contraste WCAG × 100 (450 = 4,5:1).
pub fn contrast(a: u32, b: u32) -> u32 {
    let (x, y) = (luminance(a) as u64, luminance(b) as u64);
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    ((hi + 50_000) * 100 / (lo + 50_000)) as u32
}

/// Noir ou blanc, celui qui se lit le mieux sur `bg`.
pub fn text_on(bg: u32) -> u32 {
    let (black, white) = (rgb(0, 0, 0), rgb(0xFF, 0xFF, 0xFF));
    if contrast(black, bg) >= contrast(white, bg) {
        black
    } else {
        white
    }
}

/// `color` rapprochée du noir ou du blanc juste assez pour atteindre
/// `minimum` (× 100) sur chacun des fonds `bgs`.
pub fn readable(mut color: u32, bgs: &[u32], minimum: u32) -> u32 {
    let Some(&first) = bgs.first() else {
        return color;
    };
    let target = text_on(first);
    let mut step = 0;
    while step < 20 && bgs.iter().any(|&bg| contrast(color, bg) < minimum) {
        color = mix(color, target, 1, 10);
        step += 1;
    }
    color
}

// ---------- Thème ----------

/// Mode sombre (par défaut) ou clair, partagé par toutes les fenêtres.
pub static mut DARK: bool = true;
/// Couleur d'accentuation (COLORREF), violet du logo Prisme par défaut
/// (identité propre, distincte du bleu de Windows).
pub static mut ACCENT: u32 = rgb(0xB6, 0x9C, 0xFF);
/// Couleur d'accentuation choisie (teinte « de base » de la palette).
pub static mut ACCENT_BASE: u32 = DEFAULT_ACCENT;
pub const DEFAULT_ACCENT: u32 = rgb(0x7C, 0x4D, 0xFF);
/// Accent sur la barre de titre et la bordure de la fenêtre active.
pub static mut ACCENT_TITLES: bool = false;
/// Thème de contraste actif (0 = aucun), copie de `A11Y.contrast`.
pub static mut HIGH_CONTRAST: bool = false;
/// Réglages visuels partagés (source unique).
pub static mut ENABLE_BLUR: bool = true;
pub static mut ENABLE_SMOOTH_ANIM: bool = true;
pub static mut ENABLE_THUMBNAILS: bool = true;
pub static mut AUTO_THEME: bool = false;

// Transition fluide entre thèmes.
static mut THEME_ANIM_START: u32 = 0;
static mut THEME_ANIM_DUR: i32 = 300;
static mut PREV_DARK: bool = true;
static mut TRANSITIONING: bool = false;

/// Palette des fenêtres façon Windows 11 (fond « Mica », surfaces, textes).
#[derive(Clone, Copy)]
pub struct Palette {
    pub window: u32,
    pub mica: u32,
    pub surface: u32,
    pub surface2: u32,
    pub border: u32,
    pub hover: u32,
    pub pressed: u32,
    pub selected: u32,
    pub text: u32,
    pub text2: u32,
    pub text3: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub shadow: u32,
}

/// Couleurs d'un thème de contraste : fond, texte, lien, inactif,
/// fond et texte sélectionnés (mêmes teintes que le runtime).
#[derive(Clone, Copy)]
pub struct Contrast {
    pub background: u32,
    pub text: u32,
    pub hyperlink: u32,
    pub disabled: u32,
    pub selected_bg: u32,
    pub selected_text: u32,
}

pub const fn contrast_colors(theme: u8) -> Option<Contrast> {
    Some(match theme {
        // Aquatique : blanc sur noir, liens cyan.
        1 => Contrast {
            background: rgb(0x20, 0x20, 0x20),
            text: rgb(0xFF, 0xFF, 0xFF),
            hyperlink: rgb(0x75, 0xE9, 0xFC),
            disabled: rgb(0xA6, 0xA6, 0xA6),
            selected_bg: rgb(0x8E, 0xE3, 0xF0),
            selected_text: rgb(0x26, 0x3B, 0x50),
        },
        // Désert : noir sur blanc cassé.
        2 => Contrast {
            background: rgb(0xFF, 0xFA, 0xEF),
            text: rgb(0x3D, 0x3D, 0x3D),
            hyperlink: rgb(0x1C, 0x5E, 0x75),
            disabled: rgb(0x67, 0x67, 0x67),
            selected_bg: rgb(0x90, 0x39, 0x09),
            selected_text: rgb(0xFF, 0xF5, 0xE3),
        },
        // Crépuscule : turquoise sur ardoise.
        3 => Contrast {
            background: rgb(0x2D, 0x32, 0x36),
            text: rgb(0xB6, 0xF6, 0xF0),
            hyperlink: rgb(0x70, 0xEB, 0xDE),
            disabled: rgb(0xBD, 0xBD, 0xBD),
            selected_bg: rgb(0xA1, 0xBF, 0xDE),
            selected_text: rgb(0x21, 0x2D, 0x3B),
        },
        // Ciel nocturne : blanc sur noir, accents violets.
        4 => Contrast {
            background: rgb(0x00, 0x00, 0x00),
            text: rgb(0xFF, 0xFF, 0xFF),
            hyperlink: rgb(0x80, 0x80, 0xFF),
            disabled: rgb(0xD6, 0xB4, 0xFD),
            selected_bg: rgb(0xD6, 0xB4, 0xFD),
            selected_text: rgb(0x2B, 0x2B, 0x2B),
        },
        _ => return None,
    })
}

/// Palette Winlux dérivée d'un thème de contraste : aplats sans dégradé,
/// bordures de la couleur du texte, aucune teinte atténuée pour le texte.
pub fn contrast_palette(c: &Contrast) -> Palette {
    Palette {
        window: c.background,
        mica: c.background,
        surface: c.background,
        surface2: mix(c.background, c.text, 1, 12),
        border: c.text,
        hover: mix(c.background, c.hyperlink, 2, 10),
        pressed: mix(c.background, c.hyperlink, 3, 10),
        selected: mix(c.background, c.selected_bg, 3, 10),
        text: c.text,
        text2: c.text,
        text3: c.disabled,
        accent: c.hyperlink,
        on_accent: c.background,
        shadow: c.background,
    }
}

fn base_pal(dark: bool) -> Palette {
    let accent = unsafe { ACCENT };
    if let Some(c) = contrast_colors(unsafe { A11Y.contrast }) {
        return contrast_palette(&c);
    }
    if dark {
        Palette {
            window: rgb(0x20, 0x20, 0x20),
            mica: rgb(0x1C, 0x1F, 0x26),
            surface: rgb(0x2B, 0x2B, 0x2B),
            surface2: rgb(0x32, 0x32, 0x32),
            border: rgb(0x3D, 0x3D, 0x3D),
            hover: rgb(0x38, 0x3A, 0x3F),
            pressed: rgb(0x2E, 0x30, 0x34),
            selected: mix(rgb(0x2B, 0x2B, 0x2B), accent, 3, 10),
            text: rgb(0xF2, 0xF2, 0xF2),
            text2: rgb(0xC5, 0xC5, 0xC5),
            // #8A8A8A ne faisait que 3,3:1 sur un survol (#383A3F).
            text3: rgb(0xA4, 0xA4, 0xA4),
            accent,
            on_accent: rgb(0x00, 0x00, 0x00),
            shadow: rgb(0x08, 0x0C, 0x18),
        }
    } else {
        let accent = if unsafe { ACCENT_BASE } == DEFAULT_ACCENT {
            rgb(0x6A, 0x3D, 0xF0)
        } else {
            mix(unsafe { ACCENT_BASE }, 0, 1, 10)
        };
        Palette {
            window: rgb(0xF9, 0xF9, 0xF9),
            mica: rgb(0xEE, 0xF1, 0xF8),
            surface: rgb(0xFF, 0xFF, 0xFF),
            surface2: rgb(0xF5, 0xF5, 0xF5),
            border: rgb(0xE0, 0xE0, 0xE0),
            hover: rgb(0xEA, 0xEA, 0xEA),
            pressed: rgb(0xE2, 0xE2, 0xE2),
            selected: mix(rgb(0xFF, 0xFF, 0xFF), accent, 2, 10),
            text: rgb(0x1A, 0x1A, 0x1A),
            text2: rgb(0x42, 0x42, 0x42),
            // #808080 ne faisait que 3,8:1 sur le fond des fenêtres.
            text3: rgb(0x64, 0x64, 0x64),
            accent,
            on_accent: rgb(0xFF, 0xFF, 0xFF),
            shadow: rgb(0x2A, 0x30, 0x40),
        }
    }
}

/// Palette lisible quelle que soit la couleur d'accentuation : l'accent
/// sert de texte (liens, onglet actif) et de fond (boutons principaux), et
/// la ligne sélectionnée en est teintée. Un jaune ou un orange tombait à
/// 2:1 sur fond clair ; le texte gris sur une sélection teintée à moins de 2:1.
fn legible(mut p: Palette) -> Palette {
    let surfaces = [p.window, p.mica, p.surface, p.surface2, p.hover, p.pressed];
    p.accent = readable(p.accent, &surfaces, 450);
    p.on_accent = text_on(p.accent);
    // Sélection : teinte de l'accent réduite tant qu'un texte y passe sous 4,5:1.
    let base = p.surface;
    let mut tint = 3;
    loop {
        p.selected = mix(base, p.accent, tint, 10);
        if tint == 1
            || [p.text, p.text2, p.text3]
                .iter()
                .all(|&t| contrast(t, p.selected) >= 450)
        {
            break;
        }
        tint -= 1;
    }
    p.text3 = readable(p.text3, &[p.selected], 450);
    p.text2 = readable(p.text2, &[p.selected], 450);
    p
}

/// Dernière palette calculée (thème, accent, contraste) : `pal()` est appelé
/// à chaque dessin, le calcul des contrastes n'est refait qu'au changement.
static mut PAL_CACHE: Option<((bool, u32, u32, u8), Palette)> = None;

fn raw_pal(dark: bool) -> Palette {
    unsafe {
        let key = (dark, ACCENT, ACCENT_BASE, A11Y.contrast);
        if let Some((k, p)) = PAL_CACHE {
            if k == key {
                return p;
            }
        }
        let p = if contrast_colors(A11Y.contrast).is_some() {
            // Thèmes de contraste : teintes de Windows gardées, seuls les
            // survols/sélections mélangés au fond sont rattrapés.
            let mut p = base_pal(dark);
            let all = [p.window, p.surface2, p.hover, p.pressed, p.selected];
            p.text = readable(p.text, &all, 450);
            p.text2 = p.text;
            p.text3 = readable(p.text3, &all, 450);
            p.accent = readable(p.accent, &all[..4], 450);
            p
        } else {
            legible(base_pal(dark))
        };
        PAL_CACHE = Some((key, p));
        p
    }
}

fn mix_pal(a: &Palette, b: &Palette, t: i32, n: i32) -> Palette {
    Palette {
        window: mix(a.window, b.window, t, n),
        mica: mix(a.mica, b.mica, t, n),
        surface: mix(a.surface, b.surface, t, n),
        surface2: mix(a.surface2, b.surface2, t, n),
        border: mix(a.border, b.border, t, n),
        hover: mix(a.hover, b.hover, t, n),
        pressed: mix(a.pressed, b.pressed, t, n),
        selected: mix(a.selected, b.selected, t, n),
        text: mix(a.text, b.text, t, n),
        text2: mix(a.text2, b.text2, t, n),
        text3: mix(a.text3, b.text3, t, n),
        accent: mix(a.accent, b.accent, t, n),
        on_accent: mix(a.on_accent, b.on_accent, t, n),
        shadow: mix(a.shadow, b.shadow, t, n),
    }
}

pub fn pal() -> Palette {
    unsafe {
        let target = raw_pal(DARK);
        if !TRANSITIONING {
            return target;
        }
        let elapsed = GetTickCount().wrapping_sub(THEME_ANIM_START) as i32;
        if elapsed >= THEME_ANIM_DUR {
            TRANSITIONING = false;
            return target;
        }
        let e = ease_out(elapsed, THEME_ANIM_DUR);
        let prev = raw_pal(PREV_DARK);
        mix_pal(&prev, &target, e, 1000)
    }
}

/// Applique une couleur d'accentuation : la variante claire sert au thème
/// sombre (texte noir dessus), la teinte de base au thème clair.
pub unsafe fn set_accent(base: u32) {
    ACCENT_BASE = base;
    ACCENT = if base == DEFAULT_ACCENT {
        rgb(0xB6, 0x9C, 0xFF)
    } else {
        mix(base, rgb(0xFF, 0xFF, 0xFF), 4, 10)
    };
}

/// Vrai si une transition de thème est en cours (la boucle doit repeindre).
pub fn theme_animating() -> bool {
    unsafe { TRANSITIONING }
}

/// Bascule le thème avec transition fluide.
pub unsafe fn set_theme(dark: bool) {
    if DARK == dark {
        return;
    }
    PREV_DARK = DARK;
    DARK = dark;
    TRANSITIONING = animations();
    THEME_ANIM_START = GetTickCount();
}

/// Vérifie l'heure et bascule le thème si auto-thème est activé.
/// Jour (8h–20h) → clair, nuit → sombre.
pub unsafe fn check_auto_theme() {
    if !AUTO_THEME {
        return;
    }
    let mut st = SystemTime::default();
    GetLocalTime(&mut st);
    let dark = st.hour < 8 || st.hour >= 20;
    set_theme(dark);
}

/// Trait épais entre deux points (petits carrés le long du segment).
pub unsafe fn line(
    hdc: Handle,
    b: &mut Brushes,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    t: i32,
    color: u32,
) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    if dx == 0 || dy == 0 {
        fill(
            hdc,
            b,
            x0.min(x1),
            y0.min(y1),
            x0.max(x1) + t,
            y0.max(y1) + t,
            color,
        );
        return;
    }
    let pen = CreatePen(0, t, color);
    if !pen.is_null() {
        let previous = SelectObject(hdc, pen);
        MoveToEx(hdc, x0, y0, core::ptr::null_mut());
        LineTo(hdc, x1, y1);
        SelectObject(hdc, previous);
        DeleteObject(pen);
    }
}

/// Anneau (cercle évidé) dessiné par deux disques.
pub unsafe fn ring(
    hdc: Handle,
    b: &mut Brushes,
    l: i32,
    t: i32,
    r: i32,
    bt: i32,
    w: i32,
    color: u32,
    inside_color: u32,
) {
    ellipse(hdc, b, l, t, r, bt, color);
    ellipse(hdc, b, l + w, t + w, r - w, bt - w, inside_color);
}

/// Bouton plat arrondi : survol et appui comme dans Windows 11.
pub unsafe fn flat_button(hdc: Handle, b: &mut Brushes, r: &Rect, hover: bool, pressed: bool) {
    let p = pal();
    if pressed {
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 4, p.pressed);
    } else if hover {
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 4, p.hover);
    }
}

static mut GLYPH_BG: u32 = u32::MAX;
/// Fond sous les icônes évidées (par défaut le fond « Mica » des fenêtres).
pub fn glyph_bg() -> u32 {
    unsafe {
        if GLYPH_BG != u32::MAX {
            GLYPH_BG
        } else {
            pal().mica
        }
    }
}
pub fn set_glyph_bg(color: Option<u32>) {
    unsafe {
        GLYPH_BG = color.unwrap_or(u32::MAX);
    }
}

static mut GLYPH_FG: u32 = u32::MAX;
/// Couleur des icônes au trait (par défaut la couleur du texte).
pub fn glyph_fg() -> u32 {
    unsafe {
        if GLYPH_FG != u32::MAX {
            GLYPH_FG
        } else {
            pal().text
        }
    }
}
pub fn set_glyph_fg(color: Option<u32>) {
    unsafe {
        GLYPH_FG = color.unwrap_or(u32::MAX);
    }
}

pub fn inside(r: &Rect, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
pub const fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect {
    Rect {
        left: l,
        top: t,
        right: r,
        bottom: b,
    }
}

/// Texte tronqué avec « … » pour tenir dans `max_w` (estimation de largeur).
pub unsafe fn text_fit(
    hdc: Handle,
    font: Handle,
    height: i32,
    color: u32,
    x: i32,
    y: i32,
    s: &[u8],
    max_w: i32,
) {
    if text_width(s, height) <= max_w {
        text(hdc, font, color, x, y, s);
        return;
    }
    let mut buf = [0u8; 160];
    // Plus long préfixe qui tient avec « … » (recherche dichotomique).
    let (mut lo, mut hi) = (0usize, s.len().min(158));
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        buf[..mid].copy_from_slice(&s[..mid]);
        buf[mid] = 0x85;
        if text_width(&buf[..mid + 1], height) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let keep = lo;
    buf[..keep].copy_from_slice(&s[..keep]);
    buf[keep] = 0x85; // « … » en Windows-1252
    text(hdc, font, color, x, y, &buf[..keep + 1]);
}

pub unsafe fn draw_tooltip(
    hdc: Handle,
    b: &mut Brushes,
    f: &Fonts,
    s: &[u8],
    mx: i32,
    my: i32,
    screen_w: i32,
) {
    if s.is_empty() {
        return;
    }
    let tw = text_width(s, 11) + 16;
    let th = 24;
    let tx = if mx + tw + 4 < screen_w {
        mx + 12
    } else {
        mx - tw - 4
    };
    let ty = my - th - 4;
    let p = pal();
    round_rect(
        hdc,
        b,
        tx - 1,
        ty - 1,
        tx + tw + 1,
        ty + th + 1,
        4,
        p.border,
    );
    round_rect(hdc, b, tx, ty, tx + tw, ty + th, 4, p.surface);
    text(hdc, f.small, p.text, tx + 8, ty + 5, s);
}

/// Courbe d'animation « ease-out » cubique, en millièmes (0..=1000).
pub fn ease_out(t: i32, duration: i32) -> i32 {
    if !animations() {
        return 1000;
    }
    let t = t.clamp(0, duration.max(1)) * 1000 / duration.max(1);
    let u = 1000 - t;
    1000 - u * u / 1000 * u / 1000
}

pub fn ease_in_out(t: i32, duration: i32) -> i32 {
    if !animations() {
        return 1000;
    }
    let t = t.clamp(0, duration.max(1)) * 1000 / duration.max(1);
    if t < 500 {
        4 * t * t / 1000 * t / 1000_000
    } else {
        let u = 2000 - 2 * t;
        1000 - u * u / 1000 * u / 8_000_000
    }
}

pub fn spring(t: i32, duration: i32) -> i32 {
    if !animations() {
        return 1000;
    }
    let t = t.clamp(0, duration.max(1)) * 1000 / duration.max(1);
    if t == 0 {
        return 0;
    }
    if t >= 1000 {
        return 1000;
    }
    let decay = 1000 - (1000 - t) * (1000 - t) / 1000;
    let angle = t * 540 / 1000;
    let osc = sin_milli(angle);
    decay + osc * (1000 - t) / 3000
}

/// Sinus en millièmes pour un angle en degrés (approximation de Bhaskara).
pub fn sin_milli(deg: i32) -> i32 {
    let d = deg.rem_euclid(360);
    let (x, sign) = if d < 180 { (d, 1) } else { (d - 180, -1) };
    let p = x * (180 - x);
    sign * (4000 * p) / (40500 - p)
}
pub fn cos_milli(deg: i32) -> i32 {
    sin_milli(deg + 90)
}

// ---------- Icônes vectorielles ----------

#[derive(Clone, Copy)]
pub enum Glyph {
    Start,
    Search,
    Folder,
    Browser,
    Terminal,
    Store,
    Settings,
    Computer,
    Trash,
    Notes,
    Photos,
    Music,
    Mail,
    Calc,
    Exe,
    Home,
    Desktop,
    Download,
    Video,
    Drive,
    LinuxDrive,
    Star,
    File,
    Image,
    Audio,
    Archive,
    Code,
    Pdf,
    Text,
    Back,
    Forward,
    Up,
    Refresh,
    Plus,
    Close,
    Minimize,
    Maximize,
    Restore,
    Cut,
    Copy,
    Paste,
    Delete,
    Rename,
    Sort,
    View,
    Split,
    Chevron,
    ChevronDown,
    Grid,
    List,
    Details,
    Tiles,
    Tools,
    Wifi,
    Ethernet,
    /// Fichier image disque (iso, vhd, qcow2…) : un disque optique.
    DiskImage,
    /// Image disque montée : lecteur amovible avec un disque.
    Removable,
    /// Lecteur réseau (FTP, SFTP, SMB…).
    NetDrive,
    /// Bouton Éjecter.
    Eject,
    Clock,
    Shield,
}

pub unsafe fn glyph(hdc: Handle, b: &mut Brushes, f: &Fonts, g: Glyph, x: i32, y: i32, s: i32) {
    match g {
        Glyph::Start => {
            // Logo Winlux « Prisme » (bouton Démarrer, menus, lecteur système).
            brand::logo(hdc, b, x, y, s, 1000, None);
        }
        Glyph::Search => {
            let r = s * 2 / 3;
            ellipse(hdc, b, x, y, x + r, y + r, rgb(0xF2, 0xF2, 0xF2));
            ellipse(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                x + r - s / 8,
                y + r - s / 8,
                rgb(0x2A, 0x2D, 0x34),
            );
            let t = (s / 9).max(2);
            for k in 0..(s / 3) {
                fill(
                    hdc,
                    b,
                    x + r - t + k - 1,
                    y + r - t + k - 1,
                    x + r + k,
                    y + r + k,
                    rgb(0xF2, 0xF2, 0xF2),
                );
            }
        }
        Glyph::Folder => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 8,
                x + s / 2,
                y + s / 3,
                2,
                rgb(0xE0, 0x9E, 0x2A),
            );
            round_rect(
                hdc,
                b,
                x,
                y + s / 4,
                x + s,
                y + s * 7 / 8,
                3,
                rgb(0xF4, 0xB8, 0x3E),
            );
            fill(
                hdc,
                b,
                x,
                y + s / 3,
                x + s,
                y + s * 3 / 8,
                rgb(0xFF, 0xD1, 0x66),
            );
        }
        Glyph::Browser => {
            ellipse(hdc, b, x, y, x + s, y + s, rgb(0x0F, 0x9D, 0x8A));
            ellipse(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                x + s * 7 / 8,
                y + s * 7 / 8,
                rgb(0x1A, 0x73, 0xE8),
            );
            ellipse(
                hdc,
                b,
                x + s / 3,
                y + s / 4,
                x + s * 3 / 4,
                y + s * 2 / 3,
                rgb(0x7F, 0xE0, 0xD0),
            );
            ellipse(
                hdc,
                b,
                x + s * 2 / 5,
                y + s / 3,
                x + s * 2 / 3,
                y + s * 3 / 5,
                rgb(0x1A, 0x73, 0xE8),
            );
        }
        Glyph::Terminal => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 8,
                x + s,
                y + s * 7 / 8,
                3,
                rgb(0x38, 0x3B, 0x44),
            );
            round_rect(
                hdc,
                b,
                x + 1,
                y + s / 8 + 1,
                x + s - 1,
                y + s * 7 / 8 - 1,
                2,
                rgb(0x0C, 0x0C, 0x0C),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s / 8 + 1,
                x + s - 2,
                y + s / 4,
                rgb(0x4C, 0x2F, 0x8A),
            );
            text(
                hdc,
                f.icon,
                rgb(0xCC, 0xCC, 0xCC),
                x + s / 6,
                y + s / 3,
                b">_",
            );
        }
        Glyph::Store => {
            let sc = rgb(0x0F, 0x6C, 0xBD);
            let w = rgb(0xFF, 0xFF, 0xFF);
            // Un sac représente le magasin, pas un bouclier de sécurité.
            ellipse(hdc, b, x + s / 3, y + s / 12, x + s * 2 / 3, y + s / 2, sc);
            ellipse(
                hdc,
                b,
                x + s * 5 / 12,
                y + s / 6,
                x + s * 7 / 12,
                y + s * 5 / 12,
                glyph_bg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 6,
                y + s / 3,
                x + s * 5 / 6,
                y + s * 11 / 12,
                3,
                sc,
            );
            let t = (s / 12).max(1);
            fill(
                hdc,
                b,
                x + s / 2 - t,
                y + s / 2,
                x + s / 2 + t,
                y + s * 3 / 4,
                w,
            );
            fill(
                hdc,
                b,
                x + s * 3 / 8,
                y + s * 5 / 8 - t,
                x + s * 5 / 8,
                y + s * 5 / 8 + t,
                w,
            );
        }
        Glyph::Settings => {
            let gc = rgb(0x9A, 0xA3, 0xAE);
            let cx = x + s / 2;
            let cy = y + s / 2;
            let tw = (s / 7).max(2);
            fill(hdc, b, cx - tw, y, cx + tw, y + s / 4, gc);
            fill(hdc, b, cx - tw, y + s * 3 / 4, cx + tw, y + s, gc);
            fill(hdc, b, x, cy - tw, x + s / 4, cy + tw, gc);
            fill(hdc, b, x + s * 3 / 4, cy - tw, x + s, cy + tw, gc);
            let d = s / 4;
            let dt = (s / 8).max(1);
            fill(
                hdc,
                b,
                cx - d - dt,
                cy - d - dt,
                cx - d + dt,
                cy - d + dt,
                gc,
            );
            fill(
                hdc,
                b,
                cx + d - dt,
                cy - d - dt,
                cx + d + dt,
                cy - d + dt,
                gc,
            );
            fill(
                hdc,
                b,
                cx - d - dt,
                cy + d - dt,
                cx - d + dt,
                cy + d + dt,
                gc,
            );
            fill(
                hdc,
                b,
                cx + d - dt,
                cy + d - dt,
                cx + d + dt,
                cy + d + dt,
                gc,
            );
            let r = s * 3 / 8;
            ellipse(hdc, b, cx - r, cy - r, cx + r, cy + r, gc);
            let ir = s * 3 / 16;
            ellipse(hdc, b, cx - ir, cy - ir, cx + ir, cy + ir, glyph_bg());
        }
        Glyph::Computer => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 10,
                x + s,
                y + s * 3 / 4,
                3,
                rgb(0x3A, 0x3F, 0x48),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s / 10 + 2,
                x + s - 2,
                y + s * 3 / 4 - 2,
                rgb(0x3D, 0x9B, 0xF5),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s * 2 / 5,
                x + s - 2,
                y + s * 3 / 4 - 2,
                rgb(0x2B, 0x7F, 0xDA),
            );
            fill(
                hdc,
                b,
                x + s * 2 / 5,
                y + s * 3 / 4,
                x + s * 3 / 5,
                y + s * 7 / 8,
                rgb(0x8C, 0x93, 0x9C),
            );
            round_rect(
                hdc,
                b,
                x + s / 4,
                y + s * 7 / 8 - 1,
                x + s * 3 / 4,
                y + s,
                2,
                rgb(0xB8, 0xBE, 0xC6),
            );
        }
        Glyph::Trash => {
            let lid = rgb(0xC8, 0xCD, 0xD4);
            let body = rgb(0xA8, 0xAE, 0xB8);
            fill(
                hdc,
                b,
                x + s * 2 / 5,
                y + s / 12,
                x + s * 3 / 5,
                y + s * 3 / 16,
                lid,
            );
            fill(
                hdc,
                b,
                x + s / 8,
                y + s * 3 / 16,
                x + s * 7 / 8,
                y + s / 4 + 1,
                lid,
            );
            round_rect(
                hdc,
                b,
                x + s / 5,
                y + s * 5 / 16,
                x + s * 4 / 5,
                y + s * 9 / 10,
                3,
                body,
            );
        }
        Glyph::Notes => {
            round_rect(
                hdc,
                b,
                x + s / 8,
                y,
                x + s * 7 / 8,
                y + s,
                3,
                rgb(0xF5, 0xF7, 0xFA),
            );
            fill(
                hdc,
                b,
                x + s / 8,
                y,
                x + s * 7 / 8,
                y + s / 5,
                rgb(0x2B, 0x88, 0xD8),
            );
            for k in 0..4 {
                let ly = y + s * 2 / 5 + k * s / 7;
                fill(
                    hdc,
                    b,
                    x + s / 4,
                    ly,
                    x + s * 3 / 4,
                    ly + 2,
                    rgb(0x9A, 0xA3, 0xAE),
                );
            }
        }
        Glyph::Photos => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 8,
                x + s,
                y + s * 7 / 8,
                4,
                rgb(0x2F, 0x86, 0xD6),
            );
            ellipse(
                hdc,
                b,
                x + s * 3 / 5,
                y + s / 4,
                x + s * 4 / 5,
                y + s * 9 / 20,
                rgb(0xFF, 0xD3, 0x4F),
            );
            ellipse(
                hdc,
                b,
                x + s / 10,
                y + s / 2,
                x + s * 3 / 4,
                y + s * 6 / 5,
                rgb(0x34, 0xC7, 0x59),
            );
            ellipse(
                hdc,
                b,
                x + s / 2,
                y + s * 3 / 5,
                x + s * 11 / 10,
                y + s * 6 / 5,
                rgb(0x2A, 0xA8, 0x4A),
            );
            // Recadre les collines dans la vignette.
            fill(
                hdc,
                b,
                x - s / 4,
                y + s * 7 / 8,
                x + s * 5 / 4,
                y + s * 5 / 4,
                rgb(0x24, 0x27, 0x2F),
            );
            fill(
                hdc,
                b,
                x + s,
                y,
                x + s * 5 / 4,
                y + s * 5 / 4,
                rgb(0x24, 0x27, 0x2F),
            );
        }
        Glyph::Music => {
            round_rect(hdc, b, x, y, x + s, y + s, 5, rgb(0xE8, 0x3E, 0x6C));
            ellipse(
                hdc,
                b,
                x + s / 4,
                y + s / 2,
                x + s / 2,
                y + s * 3 / 4,
                rgb(0xFF, 0xFF, 0xFF),
            );
            fill(
                hdc,
                b,
                x + s / 2 - 2,
                y + s / 5,
                x + s / 2,
                y + s * 5 / 8,
                rgb(0xFF, 0xFF, 0xFF),
            );
            fill(
                hdc,
                b,
                x + s / 2 - 2,
                y + s / 5,
                x + s * 3 / 4,
                y + s / 5 + 3,
                rgb(0xFF, 0xFF, 0xFF),
            );
        }
        Glyph::Mail => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 5,
                x + s,
                y + s * 4 / 5,
                3,
                rgb(0x10, 0x6E, 0xBE),
            );
            for k in 0..(s / 2) {
                fill(
                    hdc,
                    b,
                    x + k,
                    y + s / 5 + k * 3 / 5,
                    x + s - k,
                    y + s / 5 + k * 3 / 5 + 2,
                    rgb(0x5C, 0xB4, 0xFF),
                );
            }
        }
        Glyph::Calc => {
            round_rect(
                hdc,
                b,
                x + s / 8,
                y,
                x + s * 7 / 8,
                y + s,
                3,
                rgb(0x3B, 0x40, 0x48),
            );
            fill(
                hdc,
                b,
                x + s / 4,
                y + s / 8,
                x + s * 3 / 4,
                y + s / 3,
                rgb(0xC8, 0xE6, 0xC9),
            );
            for r in 0..3 {
                for c in 0..3 {
                    let bx = x + s / 4 + c * s / 6;
                    let by = y + s * 5 / 12 + r * s / 6;
                    fill(
                        hdc,
                        b,
                        bx,
                        by,
                        bx + s / 9,
                        by + s / 9,
                        if c == 2 {
                            rgb(0xFF, 0x98, 0x00)
                        } else {
                            rgb(0xC0, 0xC6, 0xCE)
                        },
                    );
                }
            }
        }
        Glyph::Exe => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 8,
                x + s,
                y + s * 7 / 8,
                3,
                rgb(0x3B, 0x40, 0x48),
            );
            fill(
                hdc,
                b,
                x + 1,
                y + s / 8 + 1,
                x + s - 1,
                y + s / 3,
                rgb(0x60, 0x6A, 0x78),
            );
            glyph(hdc, b, f, Glyph::Start, x + s / 3, y + s * 2 / 5, s / 3);
        }
        Glyph::Home => {
            let c = rgb(0x4C, 0xA6, 0xF0);
            for k in 0..(s / 2) {
                fill(
                    hdc,
                    b,
                    x + s / 2 - k,
                    y + s / 10 + k,
                    x + s / 2 + k,
                    y + s / 10 + k + 1,
                    c,
                );
            }
            fill(
                hdc,
                b,
                x + s / 6,
                y + s / 2,
                x + s * 5 / 6,
                y + s * 9 / 10,
                c,
            );
            fill(
                hdc,
                b,
                x + s * 5 / 12,
                y + s * 2 / 3,
                x + s * 7 / 12,
                y + s * 9 / 10,
                rgb(0x1E, 0x5F, 0xA8),
            );
        }
        Glyph::Desktop => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 8,
                x + s,
                y + s * 3 / 4,
                2,
                rgb(0x2B, 0x7F, 0xDA),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s / 8 + 2,
                x + s - 2,
                y + s * 3 / 4 - 2,
                rgb(0x5C, 0xB8, 0xFF),
            );
            fill(
                hdc,
                b,
                x + s / 4,
                y + s * 7 / 8 - 2,
                x + s * 3 / 4,
                y + s * 7 / 8,
                rgb(0x9A, 0xA3, 0xAE),
            );
        }
        Glyph::Download => {
            let c = rgb(0x2E, 0xA0, 0x5A);
            fill(hdc, b, x + s * 7 / 16, y, x + s * 9 / 16, y + s * 5 / 8, c);
            for k in 0..(s / 4) {
                fill(
                    hdc,
                    b,
                    x + s / 4 + k,
                    y + s * 3 / 8 + k,
                    x + s * 3 / 4 - k,
                    y + s * 3 / 8 + k + 1,
                    c,
                );
            }
            fill(hdc, b, x + s / 8, y + s * 7 / 8, x + s * 7 / 8, y + s, c);
        }
        Glyph::Video => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 6,
                x + s,
                y + s * 5 / 6,
                3,
                rgb(0x8E, 0x4E, 0xD8),
            );
            for k in 0..(s / 4) {
                fill(
                    hdc,
                    b,
                    x + s * 3 / 8 + k,
                    y + s / 4 + k,
                    x + s * 3 / 8 + k + 1,
                    y + s * 3 / 4 - k,
                    rgb(0xFF, 0xFF, 0xFF),
                );
            }
        }
        Glyph::Drive | Glyph::LinuxDrive => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 3,
                x + s,
                y + s * 5 / 6,
                3,
                rgb(0x6B, 0x73, 0x80),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s / 3 + 2,
                x + s - 2,
                y + s / 2,
                rgb(0x9A, 0xA3, 0xAE),
            );
            fill(
                hdc,
                b,
                x + s * 3 / 4,
                y + s * 2 / 3 - 1,
                x + s * 7 / 8,
                y + s * 2 / 3 + 2,
                rgb(0x5C, 0xE0, 0x7A),
            );
            if matches!(g, Glyph::Drive) {
                glyph(hdc, b, f, Glyph::Start, x + s / 8, y + s / 8, s / 3);
            } else {
                ellipse(
                    hdc,
                    b,
                    x + s / 8,
                    y,
                    x + s * 7 / 16,
                    y + s * 3 / 8,
                    rgb(0xF0, 0xA0, 0x20),
                );
                ellipse(
                    hdc,
                    b,
                    x + s * 3 / 16,
                    y + s / 8,
                    x + s * 3 / 8,
                    y + s * 5 / 16,
                    rgb(0x20, 0x22, 0x28),
                );
            }
        }
        Glyph::Star => {
            let c = rgb(0xF4, 0xC4, 0x30);
            for k in 0..(s / 2) {
                let half = k * 2 / 3 + 1;
                fill(
                    hdc,
                    b,
                    x + s / 2 - half,
                    y + k,
                    x + s / 2 + half,
                    y + k + 1,
                    c,
                );
                fill(
                    hdc,
                    b,
                    x + s / 2 - half,
                    y + s - k - 1,
                    x + s / 2 + half,
                    y + s - k,
                    c,
                );
            }
            fill(hdc, b, x, y + s * 3 / 8, x + s, y + s * 5 / 8, c);
        }
        Glyph::DiskImage => {
            let c = rgb(0xB8, 0xC2, 0xCF);
            ellipse(hdc, b, x, y, x + s, y + s, rgb(0x8A, 0x96, 0xA6));
            ellipse(hdc, b, x + 1, y + 1, x + s - 1, y + s - 1, c);
            // Reflet irisé façon disque optique.
            ellipse(
                hdc,
                b,
                x + s / 6,
                y + s / 6,
                x + s / 2,
                y + s / 2,
                rgb(0xD6, 0xE4, 0xF5),
            );
            ellipse(
                hdc,
                b,
                x + s * 3 / 8,
                y + s * 3 / 8,
                x + s * 5 / 8,
                y + s * 5 / 8,
                rgb(0x6B, 0x73, 0x80),
            );
            ellipse(
                hdc,
                b,
                x + s * 7 / 16,
                y + s * 7 / 16,
                x + s * 9 / 16,
                y + s * 9 / 16,
                rgb(0xF4, 0xF6, 0xF9),
            );
        }
        Glyph::Removable | Glyph::NetDrive => {
            round_rect(
                hdc,
                b,
                x,
                y + s / 3,
                x + s,
                y + s * 5 / 6,
                3,
                rgb(0x6B, 0x73, 0x80),
            );
            fill(
                hdc,
                b,
                x + 2,
                y + s / 3 + 2,
                x + s - 2,
                y + s / 2,
                rgb(0x9A, 0xA3, 0xAE),
            );
            fill(
                hdc,
                b,
                x + s * 3 / 4,
                y + s * 2 / 3 - 1,
                x + s * 7 / 8,
                y + s * 2 / 3 + 2,
                rgb(0x5C, 0xE0, 0x7A),
            );
            if matches!(g, Glyph::Removable) {
                glyph(hdc, b, f, Glyph::DiskImage, x + s / 16, y, s * 3 / 8);
            } else {
                // Prise réseau : trois postes reliés sous le lecteur.
                let c = rgb(0x2B, 0x7F, 0xDA);
                let t = (s / 16).max(1);
                fill(
                    hdc,
                    b,
                    x + s / 2 - t / 2,
                    y + s * 5 / 6,
                    x + s / 2 + t / 2 + 1,
                    y + s * 11 / 12,
                    c,
                );
                fill(
                    hdc,
                    b,
                    x + s / 6,
                    y + s * 11 / 12,
                    x + s * 5 / 6,
                    y + s * 11 / 12 + t,
                    c,
                );
                for k in [x + s / 6, x + s / 2, x + s * 5 / 6] {
                    fill(
                        hdc,
                        b,
                        k - s / 16,
                        y + s * 11 / 12 - t,
                        k + s / 16 + 1,
                        y + s,
                        c,
                    );
                }
                ellipse(hdc, b, x + s / 16, y, x + s * 3 / 8, y + s * 5 / 16, c);
            }
        }
        Glyph::Eject => {
            let c = rgb(0x5C, 0x66, 0x73);
            let h = s * 3 / 8;
            for k in 0..h {
                let half = (k * s / 2) / h.max(1);
                fill(
                    hdc,
                    b,
                    x + s / 2 - half,
                    y + s / 8 + k,
                    x + s / 2 + half + 1,
                    y + s / 8 + k + 1,
                    c,
                );
            }
            fill(
                hdc,
                b,
                x + s / 8,
                y + s * 5 / 8,
                x + s * 7 / 8,
                y + s * 3 / 4,
                c,
            );
        }
        Glyph::File
        | Glyph::Image
        | Glyph::Audio
        | Glyph::Archive
        | Glyph::Code
        | Glyph::Pdf
        | Glyph::Text => {
            let (l, r) = (x + s / 6, x + s * 5 / 6);
            fill(hdc, b, l, y, r - s / 4, y + s, rgb(0xF4, 0xF6, 0xF9));
            fill(hdc, b, l, y + s / 4, r, y + s, rgb(0xF4, 0xF6, 0xF9));
            fill(
                hdc,
                b,
                r - s / 4,
                y,
                r - s / 4 + 1,
                y + s / 4,
                rgb(0xC8, 0xCD, 0xD4),
            );
            fill(
                hdc,
                b,
                r - s / 4,
                y + s / 4 - 1,
                r,
                y + s / 4,
                rgb(0xC8, 0xCD, 0xD4),
            );
            let w = r - l;
            match g {
                Glyph::Image => {
                    fill(
                        hdc,
                        b,
                        l + w / 8,
                        y + s * 2 / 5,
                        r - w / 8,
                        y + s * 7 / 8,
                        rgb(0x3D, 0x9B, 0xF5),
                    );
                    ellipse(
                        hdc,
                        b,
                        l + w / 5,
                        y + s * 3 / 5,
                        l + w * 4 / 5,
                        y + s * 6 / 5,
                        rgb(0x34, 0xC7, 0x59),
                    );
                    fill(hdc, b, l, y + s * 7 / 8, r, y + s, rgb(0xF4, 0xF6, 0xF9));
                }
                Glyph::Audio => {
                    ellipse(
                        hdc,
                        b,
                        l + w / 4,
                        y + s * 5 / 8,
                        l + w / 2,
                        y + s * 7 / 8,
                        rgb(0xE8, 0x3E, 0x6C),
                    );
                    fill(
                        hdc,
                        b,
                        l + w / 2 - 2,
                        y + s * 3 / 8,
                        l + w / 2,
                        y + s * 3 / 4,
                        rgb(0xE8, 0x3E, 0x6C),
                    );
                    fill(
                        hdc,
                        b,
                        l + w / 2 - 2,
                        y + s * 3 / 8,
                        l + w * 3 / 4,
                        y + s * 3 / 8 + 2,
                        rgb(0xE8, 0x3E, 0x6C),
                    );
                }
                Glyph::Archive => {
                    fill(
                        hdc,
                        b,
                        l + w / 2 - 2,
                        y,
                        l + w / 2 + 2,
                        y + s * 5 / 8,
                        rgb(0xE0, 0x9E, 0x2A),
                    );
                    fill(
                        hdc,
                        b,
                        l + w / 2 - 3,
                        y + s * 5 / 8,
                        l + w / 2 + 3,
                        y + s * 3 / 4,
                        rgb(0x8A, 0x5A, 0x10),
                    );
                }
                // Centré sur la page : calé à gauche, « </> » dépassait à
                // droite et l'icône du Bloc-notes paraissait décalée dans
                // son bouton de barre des tâches.
                Glyph::Code => {
                    // Encre calculée contre la page réellement peinte : en
                    // contraste élevé, page et encre étaient converties
                    // séparément (cyan sur blanc, 1,4:1).
                    let page = chrome(rgb(0xF4, 0xF6, 0xF9));
                    let ink = readable(chrome(rgb(0x1F, 0x6F, 0xC8)), &[page], 450);
                    let mapped = CHROME_MAP;
                    CHROME_MAP = false;
                    text_centered(hdc, f.small, 11, ink, (l + r) / 2, y + s * 2 / 5, b"</>");
                    CHROME_MAP = mapped;
                }
                Glyph::Pdf => {
                    fill(
                        hdc,
                        b,
                        l,
                        y + s / 2,
                        r,
                        y + s * 3 / 4,
                        rgb(0xD9, 0x3A, 0x2E),
                    );
                    if s >= 32 {
                        text_centered(
                            hdc,
                            f.small,
                            11,
                            rgb(0xFF, 0xFF, 0xFF),
                            (l + r) / 2,
                            y + s / 2 + 2,
                            b"PDF",
                        );
                    }
                }
                Glyph::Text => {
                    for k in 0..4 {
                        let ly = y + s * 3 / 8 + k * s / 7;
                        fill(
                            hdc,
                            b,
                            l + w / 6,
                            ly,
                            r - w / 6,
                            ly + 1.max(s / 24),
                            rgb(0x9A, 0xA3, 0xAE),
                        );
                    }
                }
                _ => {}
            }
        }
        Glyph::Back | Glyph::Forward | Glyph::Up => {
            let c = glyph_fg();
            let t = (s / 10).max(1);
            match g {
                Glyph::Back => {
                    line(hdc, b, x + s / 8, y + s / 2, x + s * 7 / 8, y + s / 2, t, c);
                    line(hdc, b, x + s / 8, y + s / 2, x + s / 2, y + s / 8, t, c);
                    line(hdc, b, x + s / 8, y + s / 2, x + s / 2, y + s * 7 / 8, t, c);
                }
                Glyph::Forward => {
                    line(hdc, b, x + s / 8, y + s / 2, x + s * 7 / 8, y + s / 2, t, c);
                    line(
                        hdc,
                        b,
                        x + s * 7 / 8 - t,
                        y + s / 2,
                        x + s / 2,
                        y + s / 8,
                        t,
                        c,
                    );
                    line(
                        hdc,
                        b,
                        x + s * 7 / 8 - t,
                        y + s / 2,
                        x + s / 2,
                        y + s * 7 / 8,
                        t,
                        c,
                    );
                }
                _ => {
                    line(hdc, b, x + s / 2, y + s / 8, x + s / 2, y + s * 7 / 8, t, c);
                    line(hdc, b, x + s / 2, y + s / 8, x + s / 8, y + s / 2, t, c);
                    line(
                        hdc,
                        b,
                        x + s / 2,
                        y + s / 8,
                        x + s * 7 / 8 - t,
                        y + s / 2,
                        t,
                        c,
                    );
                }
            }
        }
        Glyph::Refresh => {
            ring(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                x + s * 7 / 8,
                y + s * 7 / 8,
                (s / 10).max(1),
                glyph_fg(),
                glyph_bg(),
            );
            fill(hdc, b, x + s / 2, y, x + s, y + s / 2, glyph_bg());
            for k in 0..(s / 4) {
                fill(
                    hdc,
                    b,
                    x + s / 2 + k,
                    y + s / 8 - s / 8 + k,
                    x + s / 2 + k + 1,
                    y + s / 2 - k,
                    glyph_fg(),
                );
            }
        }
        Glyph::Plus => {
            let c = glyph_fg();
            let t = (s / 9).max(1);
            fill(
                hdc,
                b,
                x + s / 2 - t / 2,
                y + s / 8,
                x + s / 2 + (t + 1) / 2,
                y + s * 7 / 8,
                c,
            );
            fill(
                hdc,
                b,
                x + s / 8,
                y + s / 2 - t / 2,
                x + s * 7 / 8,
                y + s / 2 + (t + 1) / 2,
                c,
            );
        }
        Glyph::Close => {
            let c = glyph_fg();
            let t = (s / 12).max(1);
            line(
                hdc,
                b,
                x + s / 6,
                y + s / 6,
                x + s * 5 / 6 - t,
                y + s * 5 / 6 - t,
                t,
                c,
            );
            line(
                hdc,
                b,
                x + s * 5 / 6 - t,
                y + s / 6,
                x + s / 6,
                y + s * 5 / 6 - t,
                t,
                c,
            );
        }
        Glyph::Minimize => fill(
            hdc,
            b,
            x + s / 6,
            y + s / 2,
            x + s * 5 / 6,
            y + s / 2 + 1,
            pal().text,
        ),
        Glyph::Maximize => {
            round_rect(
                hdc,
                b,
                x + s / 6,
                y + s / 6,
                x + s * 5 / 6,
                y + s * 5 / 6,
                2,
                glyph_fg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 6 + 1,
                y + s / 6 + 1,
                x + s * 5 / 6 - 1,
                y + s * 5 / 6 - 1,
                1,
                glyph_bg(),
            );
        }
        Glyph::Restore => {
            round_rect(
                hdc,
                b,
                x + s / 3,
                y + s / 6,
                x + s * 5 / 6,
                y + s * 2 / 3,
                2,
                glyph_fg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 3 + 1,
                y + s / 6 + 1,
                x + s * 5 / 6 - 1,
                y + s * 2 / 3 - 1,
                1,
                glyph_bg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 6,
                y + s / 3,
                x + s * 2 / 3,
                y + s * 5 / 6,
                2,
                glyph_fg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 6 + 1,
                y + s / 3 + 1,
                x + s * 2 / 3 - 1,
                y + s * 5 / 6 - 1,
                1,
                glyph_bg(),
            );
        }
        Glyph::Cut => {
            let p = pal();
            let t = (s / 12).max(1);
            line(
                hdc,
                b,
                x + s / 4,
                y,
                x + s * 2 / 3,
                y + s * 2 / 3,
                t,
                glyph_fg(),
            );
            line(
                hdc,
                b,
                x + s * 3 / 4,
                y,
                x + s / 3,
                y + s * 2 / 3,
                t,
                glyph_fg(),
            );
            ring(
                hdc,
                b,
                x,
                y + s / 2,
                x + s * 2 / 5,
                y + s * 9 / 10,
                t + 1,
                p.accent,
                glyph_bg(),
            );
            ring(
                hdc,
                b,
                x + s * 3 / 5,
                y + s / 2,
                x + s,
                y + s * 9 / 10,
                t + 1,
                p.accent,
                glyph_bg(),
            );
        }
        Glyph::Copy => {
            round_rect(hdc, b, x + s / 3, y, x + s, y + s * 2 / 3, 2, glyph_fg());
            round_rect(
                hdc,
                b,
                x + s / 3 + 1,
                y + 1,
                x + s - 1,
                y + s * 2 / 3 - 1,
                1,
                glyph_bg(),
            );
            round_rect(hdc, b, x, y + s / 3, x + s * 2 / 3, y + s, 2, glyph_fg());
            round_rect(
                hdc,
                b,
                x + 1,
                y + s / 3 + 1,
                x + s * 2 / 3 - 1,
                y + s - 1,
                1,
                glyph_bg(),
            );
        }
        Glyph::Paste => {
            let p = pal();
            round_rect(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                x + s * 7 / 8,
                y + s,
                2,
                p.text2,
            );
            round_rect(
                hdc,
                b,
                x + s / 8 + 1,
                y + s / 8 + 1,
                x + s * 7 / 8 - 1,
                y + s - 1,
                1,
                glyph_bg(),
            );
            round_rect(hdc, b, x + s / 3, y, x + s * 2 / 3, y + s / 4, 2, p.accent);
        }
        Glyph::Delete => {
            fill(
                hdc,
                b,
                x + s / 8,
                y + s / 6,
                x + s * 7 / 8,
                y + s / 6 + (s / 10).max(1),
                glyph_fg(),
            );
            fill(
                hdc,
                b,
                x + s * 3 / 8,
                y,
                x + s * 5 / 8,
                y + s / 6,
                glyph_fg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 4,
                y + s / 3,
                x + s * 3 / 4,
                y + s,
                2,
                glyph_fg(),
            );
            round_rect(
                hdc,
                b,
                x + s / 4 + 1,
                y + s / 3 + 1,
                x + s * 3 / 4 - 1,
                y + s - 1,
                1,
                glyph_bg(),
            );
        }
        Glyph::Rename => {
            let p = pal();
            let t = (s / 10).max(1);
            line(
                hdc,
                b,
                x + s / 8,
                y + s * 7 / 8 - t,
                x + s * 3 / 4,
                y + s / 4,
                t + 1,
                glyph_fg(),
            );
            fill(
                hdc,
                b,
                x + s * 5 / 8,
                y + s / 8,
                x + s * 7 / 8,
                y + s * 3 / 8,
                p.accent,
            );
        }
        Glyph::Sort => {
            let t = (s / 10).max(1);
            fill(
                hdc,
                b,
                x + s / 4,
                y + s / 8,
                x + s / 4 + t,
                y + s * 7 / 8,
                glyph_fg(),
            );
            line(
                hdc,
                b,
                x + s / 4,
                y + s / 8,
                x,
                y + s * 3 / 8,
                t,
                glyph_fg(),
            );
            line(
                hdc,
                b,
                x + s / 4,
                y + s / 8,
                x + s / 2,
                y + s * 3 / 8,
                t,
                glyph_fg(),
            );
            fill(
                hdc,
                b,
                x + s * 3 / 4,
                y + s / 8,
                x + s * 3 / 4 + t,
                y + s * 7 / 8,
                glyph_fg(),
            );
            line(
                hdc,
                b,
                x + s * 3 / 4,
                y + s * 7 / 8 - t,
                x + s / 2,
                y + s * 5 / 8,
                t,
                glyph_fg(),
            );
            line(
                hdc,
                b,
                x + s * 3 / 4,
                y + s * 7 / 8 - t,
                x + s - t,
                y + s * 5 / 8,
                t,
                glyph_fg(),
            );
        }
        Glyph::View | Glyph::Grid => {
            let c = glyph_fg();
            let h = s * 3 / 8;
            for (ox, oy) in [(0, 0), (s - h, 0), (0, s - h), (s - h, s - h)] {
                round_rect(hdc, b, x + ox, y + oy, x + ox + h, y + oy + h, 2, c);
            }
        }
        Glyph::List => {
            let c = glyph_fg();
            for k in 0..4 {
                let ly = y + s / 8 + k * s / 4;
                fill(hdc, b, x, ly, x + s / 6, ly + 2, c);
                fill(hdc, b, x + s / 4, ly, x + s, ly + 2, c);
            }
        }
        Glyph::Details => {
            let c = glyph_fg();
            for k in 0..4 {
                let ly = y + s / 8 + k * s / 4;
                fill(hdc, b, x, ly, x + s * 3 / 8, ly + 2, c);
                fill(hdc, b, x + s / 2, ly, x + s * 3 / 4, ly + 2, c);
                fill(hdc, b, x + s * 13 / 16, ly, x + s, ly + 2, c);
            }
        }
        Glyph::Tiles => {
            let c = glyph_fg();
            for k in 0..2 {
                let ly = y + k * s / 2 + s / 16;
                round_rect(hdc, b, x, ly, x + s * 3 / 8, ly + s * 3 / 8, 2, c);
                fill(hdc, b, x + s / 2, ly + s / 16, x + s, ly + s / 16 + 2, c);
                fill(
                    hdc,
                    b,
                    x + s / 2,
                    ly + s / 4,
                    x + s * 3 / 4,
                    ly + s / 4 + 2,
                    c,
                );
            }
        }
        Glyph::Split => {
            round_rect(hdc, b, x, y + s / 8, x + s, y + s * 7 / 8, 2, glyph_fg());
            fill(
                hdc,
                b,
                x + 1,
                y + s / 8 + 1,
                x + s / 2 - 1,
                y + s * 7 / 8 - 1,
                glyph_bg(),
            );
            fill(
                hdc,
                b,
                x + s / 2 + 1,
                y + s / 8 + 1,
                x + s - 1,
                y + s * 7 / 8 - 1,
                glyph_bg(),
            );
        }
        Glyph::Chevron => {
            let c = pal().text2;

            let t = (s / 8).max(1);
            line(hdc, b, x + s / 3, y + s / 6, x + s * 2 / 3, y + s / 2, t, c);
            line(
                hdc,
                b,
                x + s * 2 / 3,
                y + s / 2,
                x + s / 3,
                y + s * 5 / 6,
                t,
                c,
            );
        }
        Glyph::ChevronDown => {
            let c = pal().text2;
            let t = (s / 8).max(1);
            line(hdc, b, x + s / 6, y + s / 3, x + s / 2, y + s * 2 / 3, t, c);
            line(
                hdc,
                b,
                x + s / 2,
                y + s * 2 / 3,
                x + s * 5 / 6,
                y + s / 3,
                t,
                c,
            );
        }
        Glyph::Tools => {
            let c = pal().text;
            fill(hdc, b, x + s / 6, y + s / 4, x + s / 3, y + s * 3 / 4, c);
            fill(
                hdc,
                b,
                x + s * 2 / 5,
                y + s / 4,
                x + s * 3 / 5,
                y + s * 3 / 4,
                c,
            );
            fill(
                hdc,
                b,
                x + s * 2 / 3,
                y + s / 4,
                x + s * 5 / 6,
                y + s * 3 / 4,
                c,
            );
        }
        Glyph::Clock => {
            let p = pal();
            ellipse(hdc, b, x + 1, y + 1, x + s - 1, y + s - 1, p.accent);
            let t = (s / 9).max(2);
            ellipse(hdc, b, x + 1 + t, y + 1 + t, x + s - 1 - t, y + s - 1 - t, glyph_bg());
            let cx = x + s / 2;
            let cy = y + s / 2;
            let w = (s / 14).max(1);
            fill(hdc, b, cx - w, y + s / 4, cx + w, cy + w, p.text);
            fill(hdc, b, cx - w, cy - w, x + s * 2 / 3 + 1, cy + w, p.text);
        }
        Glyph::Shield => {
            let p = pal();
            let cx = x + s / 2;
            let top = y + s / 10;
            let mid = y + s * 11 / 20;
            fill(hdc, b, x + s / 6, top + 2, x + s * 5 / 6, mid, p.accent);
            fill(hdc, b, cx - 1, top, cx + 1, top + 2, p.accent);
            let h = (y + s * 19 / 20 - mid).max(1);
            for i in 0..h {
                let half = (s / 3) * (h - i) / h;
                fill(hdc, b, cx - half, mid + i, cx + half, mid + i + 1, p.accent);
            }
            // Coche claire au centre.
            let w = (s / 12).max(1);
            for i in 0..s / 6 {
                fill(hdc, b, x + s / 3 + i, y + s / 2 + i, x + s / 3 + i + w, y + s / 2 + i + w, p.on_accent);
            }
            for i in 0..s / 3 {
                fill(hdc, b, x + s / 2 + i - 1, y + s * 2 / 3 - i, x + s / 2 + i - 1 + w, y + s * 2 / 3 - i + w, p.on_accent);
            }
        }
        Glyph::Wifi => {
            let c = pal().text;
            let cx = x + s / 2;
            let cy = y + s * 3 / 4;
            let t = (s / 10).max(1);
            ellipse(hdc, b, cx - t, cy - t, cx + t, cy + t, c);
            for k in 1..4i32 {
                let r = s / 5 * k;
                let arc_t = t;
                for a in 0..16i32 {
                    let deg = 225 + a * 90 / 15;
                    let x0 = cx + (r as i32 * cos_milli(deg)) / 1000;
                    let y0 = cy - (r as i32 * sin_milli(deg)) / 1000;
                    let x1 = cx + ((r + arc_t) as i32 * cos_milli(deg)) / 1000;
                    let y1 = cy - ((r + arc_t) as i32 * sin_milli(deg)) / 1000;
                    fill(
                        hdc,
                        b,
                        x0.min(x1),
                        y0.min(y1),
                        x0.max(x1) + 1,
                        y0.max(y1) + 1,
                        c,
                    );
                }
            }
        }
        Glyph::Ethernet => {
            let c = pal().text;
            let t = (s / 8).max(1);
            let cx = x + s / 2;
            fill(hdc, b, cx - t, y + s / 6, cx + t, y + s * 5 / 6, c);
            fill(
                hdc,
                b,
                x + s / 5,
                y + s / 3,
                x + s * 4 / 5,
                y + s / 3 + t,
                c,
            );
            for &dx in &[s / 5, s * 4 / 5 - t] {
                fill(hdc, b, x + dx, y + s / 3, x + dx + t, y + s / 2, c);
                fill(
                    hdc,
                    b,
                    x + dx - t,
                    y + s / 2,
                    x + dx + 2 * t,
                    y + s / 2 + t * 2,
                    c,
                );
            }
        }
    }
}

// ---------- Accessibilité (réglages partagés) ----------

/// Réglages d'accessibilité, enregistrés dans
/// `C:\.local\share\winlux\accessibility.ini` et relus par tous les
/// processus Winlux (bureau, applications intégrées, runtime).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct A11y {
    /// Thème de contraste : 0 = aucun, 1..=4 (Aquatique, Désert, Crépuscule, Ciel nocturne).
    pub contrast: u8,
    /// Taille du pointeur : 1 (normal) à 5.
    pub pointer_size: u8,
    /// Couleur du pointeur : 0 blanc, 1 noir, 2 vert vif.
    pub pointer_color: u8,
    /// Épaisseur du curseur de texte, en pixels (1 à 6).
    pub caret_width: u8,
    pub animations: bool,
    pub magnifier: bool,
    /// Zoom de la loupe en pourcentage (125 à 1600).
    pub mag_zoom: u16,
    /// 0 = plein écran, 1 = lentille, 2 = ancré.
    pub mag_mode: u8,
    pub mag_smooth: bool,
    pub mag_invert: bool,
    pub narrator: bool,
    /// Débit de la voix : 1 (lent) à 10 (rapide).
    pub narrator_rate: u8,
    /// Lire les caractères saisis.
    pub narrator_echo: bool,
    /// Lire le texte sous le pointeur.
    pub narrator_pointer: bool,
    /// Sous-titres des sons système.
    pub captions: bool,
    /// Faire clignoter l'écran lors des alertes sonores.
    pub flash: bool,
    pub sticky: bool,
    pub filter: bool,
    /// Touches lentes : durée d'appui minimale (ms).
    pub slow_ms: u16,
    /// Touches rebond : délai pendant lequel une frappe répétée est ignorée (ms).
    pub bounce_ms: u16,
}

pub const A11Y_DEFAULT: A11y = A11y {
    contrast: 0,
    pointer_size: 1,
    pointer_color: 0,
    caret_width: 1,
    animations: true,
    magnifier: false,
    mag_zoom: 200,
    mag_mode: 0,
    mag_smooth: true,
    mag_invert: false,
    narrator: false,
    narrator_rate: 5,
    narrator_echo: true,
    narrator_pointer: true,
    captions: false,
    flash: false,
    sticky: false,
    filter: false,
    slow_ms: 300,
    bounce_ms: 500,
};

pub static mut A11Y: A11y = A11Y_DEFAULT;

const A11Y_FILE: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\accessibility.ini";

fn parse_num(s: &[u8]) -> Option<u32> {
    if s.is_empty() || s.len() > 6 {
        return None;
    }
    let mut n = 0u32;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (c - b'0') as u32;
    }
    Some(n)
}

/// Analyse le contenu du fichier ; les valeurs hors bornes sont ignorées.
pub fn a11y_parse(data: &[u8]) -> A11y {
    let mut a = A11Y_DEFAULT;
    for line in data.split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(eq) = line.iter().position(|&c| c == b'=') else {
            continue;
        };
        let (key, value) = (&line[..eq], &line[eq + 1..]);
        let Some(n) = parse_num(value) else {
            continue;
        };
        let flag = n == 1;
        match key {
            b"contrast" if n <= 4 => a.contrast = n as u8,
            b"pointer_size" if (1..=5).contains(&n) => a.pointer_size = n as u8,
            b"pointer_color" if n <= 2 => a.pointer_color = n as u8,
            b"caret_width" if (1..=6).contains(&n) => a.caret_width = n as u8,
            b"animations" if n <= 1 => a.animations = flag,
            b"magnifier" if n <= 1 => a.magnifier = flag,
            b"mag_zoom" if (100..=1600).contains(&n) => a.mag_zoom = n as u16,
            b"mag_mode" if n <= 2 => a.mag_mode = n as u8,
            b"mag_smooth" if n <= 1 => a.mag_smooth = flag,
            b"mag_invert" if n <= 1 => a.mag_invert = flag,
            b"narrator" if n <= 1 => a.narrator = flag,
            b"narrator_rate" if (1..=10).contains(&n) => a.narrator_rate = n as u8,
            b"narrator_echo" if n <= 1 => a.narrator_echo = flag,
            b"narrator_pointer" if n <= 1 => a.narrator_pointer = flag,
            b"captions" if n <= 1 => a.captions = flag,
            b"flash" if n <= 1 => a.flash = flag,
            b"sticky" if n <= 1 => a.sticky = flag,
            b"filter" if n <= 1 => a.filter = flag,
            b"slow_ms" if n <= 2000 => a.slow_ms = n as u16,
            b"bounce_ms" if n <= 2000 => a.bounce_ms = n as u16,
            _ => {}
        }
    }
    a
}

struct Writer {
    buf: [u8; 512],
    len: usize,
}

impl Writer {
    fn push(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
    }
    fn entry(&mut self, key: &[u8], value: u32) {
        self.push(key);
        self.push(b"=");
        let mut digits = [0u8; 10];
        let mut n = value;
        let mut i = digits.len();
        loop {
            i -= 1;
            digits[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        self.push(&digits[i..]);
        self.push(b"\n");
    }
}

/// Sérialise les réglages au format `clé=valeur` (une ligne par réglage).
pub fn a11y_format(a: &A11y, out: &mut [u8; 512]) -> usize {
    let mut w = Writer {
        buf: [0; 512],
        len: 0,
    };
    w.push(b"# Winlux - Param\xE8tres > Accessibilit\xE9\n");
    w.entry(b"contrast", a.contrast as u32);
    w.entry(b"pointer_size", a.pointer_size as u32);
    w.entry(b"pointer_color", a.pointer_color as u32);
    w.entry(b"caret_width", a.caret_width as u32);
    w.entry(b"animations", a.animations as u32);
    w.entry(b"magnifier", a.magnifier as u32);
    w.entry(b"mag_zoom", a.mag_zoom as u32);
    w.entry(b"mag_mode", a.mag_mode as u32);
    w.entry(b"mag_smooth", a.mag_smooth as u32);
    w.entry(b"mag_invert", a.mag_invert as u32);
    w.entry(b"narrator", a.narrator as u32);
    w.entry(b"narrator_rate", a.narrator_rate as u32);
    w.entry(b"narrator_echo", a.narrator_echo as u32);
    w.entry(b"narrator_pointer", a.narrator_pointer as u32);
    w.entry(b"captions", a.captions as u32);
    w.entry(b"flash", a.flash as u32);
    w.entry(b"sticky", a.sticky as u32);
    w.entry(b"filter", a.filter as u32);
    w.entry(b"slow_ms", a.slow_ms as u32);
    w.entry(b"bounce_ms", a.bounce_ms as u32);
    out.copy_from_slice(&w.buf);
    w.len
}

/// Applique les réglages aux interrupteurs historiques du thème.
unsafe fn a11y_apply() {
    HIGH_CONTRAST = A11Y.contrast != 0;
    ENABLE_SMOOTH_ANIM = A11Y.animations;
}

/// Relit le fichier partagé ; vrai si un réglage a changé (repeindre).
pub unsafe fn a11y_reload() -> bool {
    let h = CreateFileA(
        A11Y_FILE.as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    let next = if h.is_null() || h as isize == -1 {
        A11Y_DEFAULT
    } else {
        let mut data = [0u8; 1024];
        let mut read = 0;
        ReadFile(h, data.as_mut_ptr(), 1024, &mut read, core::ptr::null_mut());
        CloseHandle(h);
        a11y_parse(&data[..(read as usize).min(1024)])
    };
    let changed = next != A11Y;
    A11Y = next;
    a11y_apply();
    changed
}

/// Enregistre les réglages courants pour tous les processus Winlux.
pub unsafe fn a11y_save() -> bool {
    a11y_apply();
    for dir in [
        c"C:\\.local",
        c"C:\\.local\\share",
        c"C:\\.local\\share\\winlux",
    ] {
        CreateDirectoryA(dir.as_ptr(), core::ptr::null_mut());
    }
    let mut data = [0u8; 512];
    let n = a11y_format(&A11Y, &mut data);
    let h = CreateFileA(
        A11Y_FILE.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return false;
    }
    let mut written = 0;
    let ok = WriteFile(
        h,
        data.as_ptr(),
        n as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    ok != 0 && written == n as u32
}

static mut A11Y_POLLED: u32 = 0;

/// Relit les réglages au plus une fois par seconde (applications autonomes).
pub unsafe fn a11y_poll() -> bool {
    let t = GetTickCount();
    if A11Y_POLLED != 0 && t.wrapping_sub(A11Y_POLLED) < 1000 {
        return false;
    }
    A11Y_POLLED = t;
    a11y_reload()
}

/// Faux si l'utilisateur a désactivé les animations : les courbes
/// d'accélération renvoient alors directement leur état final.
pub fn animations() -> bool {
    unsafe { A11Y.animations && ENABLE_SMOOTH_ANIM }
}

/// Épaisseur du curseur de texte (au moins `min` pixels).
pub fn caret_w(min: i32) -> i32 {
    (unsafe { A11Y.caret_width } as i32).max(min)
}

// ----- Touches rémanentes : modificateurs verrouillés par le bureau -----

/// Modificateurs maintenus logiquement : Maj, Ctrl, Alt, Windows.
pub static mut STICKY_LATCHED: u8 = 0;
/// Modificateurs verrouillés (appui double) jusqu'au prochain appui.
pub static mut STICKY_LOCKED: u8 = 0;

pub fn modifier_bit(vk: i32) -> u8 {
    match vk {
        0x10 | 0xA0 | 0xA1 => 1,
        0x11 | 0xA2 | 0xA3 => 2,
        0x12 | 0xA4 | 0xA5 => 4,
        0x5B | 0x5C => 8,
        _ => 0,
    }
}

/// `GetKeyState` qui tient compte des touches rémanentes : un modificateur
/// relâché mais verrouillé est vu comme enfoncé par les raccourcis.
pub unsafe fn key_state(vk: i32) -> i16 {
    let state = GetKeyState(vk);
    let bit = modifier_bit(vk);
    if bit != 0 && (STICKY_LATCHED | STICKY_LOCKED) & bit != 0 {
        state | i16::MIN
    } else {
        state
    }
}

// ----- Texte affiché, relu par le Narrateur -----

#[derive(Clone, Copy)]
pub struct Span {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub len: u8,
    pub text: [u8; 96],
}

pub const SPAN_MAX: usize = 512;
const EMPTY_SPAN: Span = Span {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
    len: 0,
    text: [0; 96],
};
/// Textes dessinés lors de la dernière peinture (ordre d'affichage).
pub static mut SPANS: [Span; SPAN_MAX] = [EMPTY_SPAN; SPAN_MAX];
pub static mut SPAN_COUNT: usize = 0;
/// Actif seulement quand le Narrateur fonctionne : aucune mesure sinon.
pub static mut SPAN_CAPTURE: bool = false;

unsafe fn capture_span(hdc: Handle, x: i32, y: i32, s: &[u8]) {
    if SPAN_COUNT >= SPAN_MAX || s.iter().all(|&c| c <= b' ') {
        return;
    }
    let mut size = [0i32; 2];
    GetTextExtentPoint32A(
        hdc,
        s.as_ptr() as *const core::ffi::c_char,
        s.len() as i32,
        &mut size,
    );
    let n = s.len().min(96);
    let span = &mut SPANS[SPAN_COUNT];
    span.x = x;
    span.y = y;
    span.w = size[0].max(1);
    span.h = size[1].max(10);
    span.len = n as u8;
    span.text[..n].copy_from_slice(&s[..n]);
    SPAN_COUNT += 1;
}

/// Vrai pendant le dessin de la barre des tâches, du menu Démarrer et des
/// panneaux du bureau, dont les teintes sombres sont fixes : en contraste
/// élevé, `chrome` les ramène aux couleurs du thème.
pub static mut CHROME_MAP: bool = false;

/// Couleur fixe du bureau convertie dans le thème de contraste actif :
/// surfaces sombres → fond, gris → texte inactif, clair → texte,
/// teintes vives → couleur des liens.
pub fn chrome(c: u32) -> u32 {
    let theme = unsafe {
        if !CHROME_MAP {
            return c;
        }
        A11Y.contrast
    };
    let Some(t) = contrast_colors(theme) else {
        return c;
    };
    let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
    let lum = (r * 30 + g * 59 + b * 11) / 100;
    let sat = r.max(g).max(b) - r.min(g).min(b);
    if sat > 90 && lum > 0x40 {
        t.hyperlink
    } else if lum < 0x60 {
        // Garde un léger relief entre surfaces, survols et bordures.
        mix(
            t.background,
            t.text,
            (lum as i32 - 0x18).clamp(0, 0x48),
            0x48 * 6,
        )
    } else if lum < 0xB8 {
        t.disabled
    } else {
        t.text
    }
}

// ----- Contrôle du contraste du texte réellement dessiné -----
//
// Avec WINLUX_CONTRAST_AUDIT=fichier, chaque aplat (fill, round_rect,
// ellipse) est noté ; chaque texte est comparé au dernier aplat peint sous
// son centre dans le même contexte de dessin. Les textes sous 4,5:1 (3:1
// pour le gros texte) sont ajoutés au fichier : test-pe-text-contrast.py
// fait défiler les applications dans chaque thème et lit ce rapport.

pub static mut AUDIT_ON: bool = false;
const AUDIT_FILLS: usize = 4096;
/// Couleur « inconnue » : image ou capture sous le texte, rien à mesurer.
pub const AUDIT_IMAGE: u32 = 0xFF00_0000;
static mut FILLS: [(Handle, Rect, u32); AUDIT_FILLS] = [(
    core::ptr::null_mut(),
    Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    },
    0,
); AUDIT_FILLS];
static mut FILL_NEXT: usize = 0;
static mut FILL_LEN: usize = 0;
static mut AUDIT_SEEN: [u32; 512] = [0; 512];
static mut AUDIT_SEEN_LEN: usize = 0;
static mut AUDIT_PATH: [u8; 260] = [0; 260];

/// Thème et accent imposés (contrôle du contraste, captures) :
/// WINLUX_THEME=light|dark, WINLUX_ACCENT=RRGGBB.
pub unsafe fn forced_look() {
    let mut v = [0u8; 8];
    match GetEnvironmentVariableA(c"WINLUX_THEME".as_ptr(), v.as_mut_ptr().cast(), 8) {
        5 if v[..5] == *b"light" => DARK = false,
        4 if v[..4] == *b"dark" => DARK = true,
        _ => {}
    }
    if GetEnvironmentVariableA(c"WINLUX_ACCENT".as_ptr(), v.as_mut_ptr().cast(), 8) == 6 {
        let mut value = 0u32;
        for &c in &v[..6] {
            let digit = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => return,
            };
            value = value << 4 | digit as u32;
        }
        set_accent(rgb(value >> 16, (value >> 8) & 0xFF, value & 0xFF));
    }
}

/// Active le contrôle si WINLUX_CONTRAST_AUDIT désigne un fichier.
pub unsafe fn audit_init() {
    let n = GetEnvironmentVariableA(
        c"WINLUX_CONTRAST_AUDIT".as_ptr(),
        AUDIT_PATH.as_mut_ptr().cast(),
        259,
    ) as usize;
    AUDIT_ON = n > 0 && n < 259;
    // Seuil ×100 modifiable (vérification de l'outil lui-même : 2100 note tout).
    let mut v = [0u8; 8];
    let len = GetEnvironmentVariableA(
        c"WINLUX_CONTRAST_MINIMUM".as_ptr(),
        v.as_mut_ptr().cast(),
        8,
    ) as usize;
    if len > 0 && len < 6 && v[..len].iter().all(u8::is_ascii_digit) {
        AUDIT_MINIMUM = v[..len].iter().fold(0, |n, &c| n * 10 + (c - b'0') as u32);
    }
}
static mut AUDIT_MINIMUM: u32 = 450;

unsafe fn note_fill(hdc: Handle, l: i32, t: i32, r: i32, b: i32, color: u32) {
    if r - l < 2 || b - t < 2 {
        return; // traits et bordures : pas un fond de texte
    }
    FILLS[FILL_NEXT] = (
        hdc,
        Rect {
            left: l,
            top: t,
            right: r,
            bottom: b,
        },
        color,
    );
    FILL_NEXT = (FILL_NEXT + 1) % AUDIT_FILLS;
    FILL_LEN = (FILL_LEN + 1).min(AUDIT_FILLS);
}

/// Image dessinée par l'application : le texte posé dessus n'est pas mesuré.
pub unsafe fn audit_image(hdc: Handle, l: i32, t: i32, r: i32, b: i32) {
    if AUDIT_ON {
        note_fill(hdc, l, t, r, b, AUDIT_IMAGE);
    }
}

unsafe fn fill_under(hdc: Handle, x: i32, y: i32) -> Option<u32> {
    for i in 0..FILL_LEN {
        let (h, r, c) = FILLS[(FILL_NEXT + AUDIT_FILLS - 1 - i) % AUDIT_FILLS];
        if h == hdc && x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return (c != AUDIT_IMAGE).then_some(c & 0x00FF_FFFF);
        }
    }
    None
}

unsafe fn audit_text(hdc: Handle, color: u32, x: i32, y: i32, s: &[u8]) {
    if s.iter().all(|&c| c <= b' ') {
        return;
    }
    let mut size = [0i32; 2];
    GetTextExtentPoint32A(hdc, s.as_ptr().cast(), s.len() as i32, &mut size);
    let Some(bg) = fill_under(hdc, x + size[0].max(2) / 2, y + size[1].max(2) / 2) else {
        return;
    };
    let ratio = contrast(color & 0x00FF_FFFF, bg);
    // Gros texte (≥ 24 px de haut environ) : 3:1 suffit.
    let minimum = if size[1] >= 30 && AUDIT_MINIMUM == 450 {
        300
    } else {
        AUDIT_MINIMUM
    };
    if ratio >= minimum {
        return;
    }
    let mut hash = 2166136261u32 ^ color ^ bg.rotate_left(7);
    for &c in s {
        hash = (hash ^ c as u32).wrapping_mul(16777619);
    }
    if AUDIT_SEEN[..AUDIT_SEEN_LEN].contains(&hash) || AUDIT_SEEN_LEN >= AUDIT_SEEN.len() {
        return;
    }
    AUDIT_SEEN[AUDIT_SEEN_LEN] = hash;
    AUDIT_SEEN_LEN += 1;
    let mut line = [0u8; 200];
    let mut n = 0;
    let mut put = |bytes: &[u8]| {
        for &c in bytes {
            if n < line.len() - 1 {
                line[n] = c;
                n += 1;
            }
        }
    };
    let digits = [
        b'0' + (ratio / 100 % 10) as u8,
        b'.',
        b'0' + (ratio / 10 % 10) as u8,
        b'0' + (ratio % 10) as u8,
    ];
    if ratio >= 1000 {
        put(b"1");
    }
    put(&digits);
    put(b"\t");
    for c in [color, bg] {
        let hex = b"0123456789abcdef";
        let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
        put(b"#");
        for v in [r, g, b] {
            put(&[hex[(v >> 4) as usize], hex[(v & 15) as usize]]);
        }
        put(b"\t");
    }
    put(&s[..s.len().min(80)]);
    put(b"\n");
    let h = CreateFileA(
        AUDIT_PATH.as_ptr().cast(),
        0x4000_0000,
        3,
        core::ptr::null_mut(),
        4, // OPEN_ALWAYS
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return;
    }
    SetFilePointer(h, 0, core::ptr::null_mut(), 2);
    let mut written = 0u32;
    WriteFile(
        h,
        line.as_ptr(),
        n as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
}

// ---------- Focus clavier entre les contrôles (#28) ----------
//
// Chaque application déclare ses contrôles pendant qu'elle se dessine
// (`focus_zone`, `focus_field`), dans l'ordre du dessin. La fenêtre garde
// la liste du dernier dessin : Tab et Maj+Tab passent d'un contrôle à
// l'autre, Entrée ou Espace l'activent (clic simulé en son centre), et un
// cadre à la couleur d'accent montre le contrôle actif. Comme sous Windows,
// le cadre n'apparaît qu'après une touche et disparaît au premier clic.

const FOCUS_APPS: usize = 32;
const FOCUS_MAX: usize = 96;

#[derive(Clone, Copy)]
struct FocusList {
    rects: [Rect; FOCUS_MAX],
    field: [bool; FOCUS_MAX],
    n: usize,
}

const EMPTY_FOCUS: FocusList = FocusList {
    rects: [Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }; FOCUS_MAX],
    field: [false; FOCUS_MAX],
    n: 0,
};

/// Liste en cours de dessin, et celle de chaque fenêtre au dernier dessin.
static mut FOCUS_DRAWING: FocusList = EMPTY_FOCUS;
static mut FOCUS_DRAW_APP: usize = usize::MAX;
static mut FOCUS_LISTS: [FocusList; FOCUS_APPS] = [EMPTY_FOCUS; FOCUS_APPS];
static mut FOCUS_CURRENT: [i32; FOCUS_APPS] = [-1; FOCUS_APPS];
static mut FOCUS_SHOWN: [bool; FOCUS_APPS] = [false; FOCUS_APPS];

/// Ce que la fenêtre doit faire d'une touche.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusKey {
    /// Touche laissée à l'application.
    Pass,
    /// Le focus a changé : redessiner.
    Moved,
    /// Clic simulé au point donné (activer un bouton, entrer dans un champ),
    /// puis redessiner.
    Click(i32, i32),
}

/// Début du dessin de la fenêtre `app`.
pub unsafe fn focus_begin(app: usize) {
    FOCUS_DRAW_APP = app;
    FOCUS_DRAWING.n = 0;
}

unsafe fn focus_push(r: &Rect, field: bool) {
    if FOCUS_DRAW_APP >= FOCUS_APPS || r.right - r.left < 4 || r.bottom - r.top < 4 {
        return;
    }
    let l = &mut *core::ptr::addr_of_mut!(FOCUS_DRAWING);
    // Un même contrôle déclaré deux fois (survol redessiné) ne compte qu'une fois.
    let same = |a: &Rect| a.left == r.left && a.top == r.top && a.right == r.right && a.bottom == r.bottom;
    if l.n > 0 && same(&l.rects[l.n - 1]) {
        // Déclaré d'abord comme zone, puis comme champ : il devient champ.
        l.field[l.n - 1] |= field;
        return;
    }
    // Les grands panneaux cliquables (liste entière, fond) ne sont pas des
    // arrêts de tabulation : leurs lignes le sont.
    if r.right - r.left > 320 && r.bottom - r.top > 160 {
        return;
    }
    if l.n < FOCUS_MAX {
        l.rects[l.n] = *r;
        l.field[l.n] = field;
        l.n += 1;
    }
}

/// Retire le dernier contrôle déclaré s'il occupe `r` (bouton grisé).
pub unsafe fn focus_unzone(r: &Rect) {
    let l = &mut *core::ptr::addr_of_mut!(FOCUS_DRAWING);
    if l.n > 0 {
        let a = &l.rects[l.n - 1];
        if a.left == r.left && a.top == r.top && a.right == r.right && a.bottom == r.bottom {
            l.n -= 1;
        }
    }
}

/// Contrôle atteignable au clavier (bouton, lien, onglet, case…).
pub unsafe fn focus_zone(r: &Rect) {
    focus_push(r, false);
}

/// Champ de saisie : y arriver par Tab y place le curseur (clic simulé),
/// et Espace ou Entrée restent à l'application.
pub unsafe fn focus_field(r: &Rect) {
    focus_push(r, true);
}

/// Boîte modale dessinée par-dessus : seuls ses contrôles, déclarés ensuite,
/// restent atteignables.
pub unsafe fn focus_modal() {
    FOCUS_DRAWING.n = 0;
}

/// Fin du dessin : garde la liste et dessine le cadre de focus.
pub unsafe fn focus_end(hdc: Handle, b: &mut Brushes) {
    let app = FOCUS_DRAW_APP;
    FOCUS_DRAW_APP = usize::MAX;
    if app >= FOCUS_APPS {
        return;
    }
    FOCUS_LISTS[app] = FOCUS_DRAWING;
    let n = FOCUS_LISTS[app].n as i32;
    if FOCUS_CURRENT[app] >= n {
        FOCUS_CURRENT[app] = n - 1;
    }
    let cur = FOCUS_CURRENT[app];
    if !FOCUS_SHOWN[app] || cur < 0 {
        return;
    }
    let r = FOCUS_LISTS[app].rects[cur as usize];
    let c = pal().accent;
    // Cadre de 2 px juste autour du contrôle.
    fill(hdc, b, r.left - 2, r.top - 2, r.right + 2, r.top, c);
    fill(hdc, b, r.left - 2, r.bottom, r.right + 2, r.bottom + 2, c);
    fill(hdc, b, r.left - 2, r.top, r.left, r.bottom, c);
    fill(hdc, b, r.right, r.top, r.right + 2, r.bottom, c);
}

/// Nombre de contrôles déclarés par `app` à son dernier dessin.
pub unsafe fn focus_count(app: usize) -> usize {
    if app < FOCUS_APPS {
        FOCUS_LISTS[app].n
    } else {
        0
    }
}

/// Contrôle actif (index) et cadre visible, pour les tests.
pub unsafe fn focus_state(app: usize) -> (i32, bool) {
    if app < FOCUS_APPS {
        (FOCUS_CURRENT[app], FOCUS_SHOWN[app])
    } else {
        (-1, false)
    }
}

/// Rectangle du contrôle `i` de `app` au dernier dessin.
pub unsafe fn focus_rect(app: usize, i: usize) -> Option<Rect> {
    (app < FOCUS_APPS && i < FOCUS_LISTS[app].n).then(|| FOCUS_LISTS[app].rects[i])
}

/// Touche reçue par la fenêtre `app` : Tab, Maj+Tab, Entrée, Espace.
/// Ctrl+Tab reste à l'application (changer d'onglet).
pub unsafe fn focus_key(app: usize, vk: u32, ch: u16) -> FocusKey {
    if app >= FOCUS_APPS {
        return FocusKey::Pass;
    }
    let l = &FOCUS_LISTS[app];
    let n = l.n as i32;
    if n == 0 {
        return FocusKey::Pass;
    }
    let down = |k: i32| (key_state(k) as u16 & 0x8000) != 0;
    if vk == 0x09 && ch == 0 {
        if down(0x11) || down(0x12) {
            return FocusKey::Pass;
        }
        let back = down(0x10);
        let cur = FOCUS_CURRENT[app];
        // Après un clic, Tab repart du contrôle cliqué, comme sous Windows.
        let next = if cur < 0 || cur >= n {
            if back { n - 1 } else { 0 }
        } else if back {
            (cur + n - 1) % n
        } else {
            (cur + 1) % n
        };
        FOCUS_CURRENT[app] = next;
        FOCUS_SHOWN[app] = true;
        let i = next as usize;
        if l.field[i] {
            let r = l.rects[i];
            return FocusKey::Click((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        }
        return FocusKey::Moved;
    }
    if vk == 0x09 || ch == 9 {
        // Le caractère Tab qui suit la touche est déjà traité.
        return if FOCUS_SHOWN[app] && !down(0x11) {
            FocusKey::Moved
        } else {
            FocusKey::Pass
        };
    }
    let cur = FOCUS_CURRENT[app];
    if !FOCUS_SHOWN[app] || cur < 0 || cur >= n || l.field[cur as usize] {
        return FocusKey::Pass;
    }
    let r = l.rects[cur as usize];
    match (vk, ch) {
        (0x0D, 0) | (0x20, 0) => FocusKey::Click((r.left + r.right) / 2, (r.top + r.bottom) / 2),
        // Le caractère produit par Entrée ou Espace ne doit pas aller plus loin.
        (0, 13) | (0, 32) => FocusKey::Moved,
        (0x1B, 0) => {
            FOCUS_SHOWN[app] = false;
            FocusKey::Pass
        }
        _ => FocusKey::Pass,
    }
}

/// Flèches Haut/Bas dans un menu ouvert (#1024) : la sélection passe à
/// l'entrée précédente ou suivante, comme Maj+Tab et Tab. Sans cadre
/// affiché, Bas part de la première entrée et Haut de la dernière.
pub unsafe fn focus_step(app: usize, back: bool) -> bool {
    if app >= FOCUS_APPS {
        return false;
    }
    let n = FOCUS_LISTS[app].n as i32;
    if n == 0 {
        return false;
    }
    let cur = FOCUS_CURRENT[app];
    FOCUS_CURRENT[app] = if !FOCUS_SHOWN[app] || cur < 0 || cur >= n {
        if back { n - 1 } else { 0 }
    } else if back {
        (cur + n - 1) % n
    } else {
        (cur + 1) % n
    };
    FOCUS_SHOWN[app] = true;
    true
}

/// Clic de souris dans `app` : le cadre s'efface et le focus suit le
/// contrôle cliqué (Tab repartira de là).
pub unsafe fn focus_mouse_down(app: usize, x: i32, y: i32) {
    if app >= FOCUS_APPS {
        return;
    }
    FOCUS_SHOWN[app] = false;
    let l = &FOCUS_LISTS[app];
    if let Some(i) = (0..l.n).rev().find(|&i| inside(&l.rects[i], x, y)) {
        FOCUS_CURRENT[app] = i as i32;
    }
}
