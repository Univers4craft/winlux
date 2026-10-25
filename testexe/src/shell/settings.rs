//! Paramètres de Winlux — interface complète façon Windows 11 :
//! panneau latéral de navigation, catégories, cartes, bascules, barres
//! de progression et informations système.
//!
//! Contrat d'application : draw, mouse, key, tick.

use winlux_testexe::*;
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};

pub const CUSTOM_TITLE: bool = false;

// ── Constantes de mise en page ──────────────────────────────────────

const NAV_W: i32 = 220;
const CARD_RADIUS: i32 = 6;
const ROW_H: i32 = 40;
const SECTION_GAP: i32 = 28;
const TOGGLE_W: i32 = 40;
const TOGGLE_H: i32 = 20;
const SLIDER_W: i32 = 180;
const SLIDER_H: i32 = 4;

// ── Catégories ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Cat {
    System, Bluetooth, Network, Personalization, Apps, Accounts,
    Time, Gaming, Accessibility, Privacy, Update, About,
}
const CATS: [Cat; 12] = [
    Cat::System, Cat::Bluetooth, Cat::Network, Cat::Personalization,
    Cat::Apps, Cat::Accounts, Cat::Time, Cat::Gaming,
    Cat::Accessibility, Cat::Privacy, Cat::Update, Cat::About,
];

fn cat_label(c: Cat) -> &'static [u8] {
    match c {
        Cat::System          => b"Syst\xE8me",
        Cat::Bluetooth       => b"Bluetooth et appareils",
        Cat::Network         => b"R\xE9seau et Internet",
        Cat::Personalization => b"Personnalisation",
        Cat::Apps            => b"Applications",
        Cat::Accounts        => b"Comptes",
        Cat::Time            => b"Heure et langue",
        Cat::Gaming          => b"Jeux",
        Cat::Accessibility   => b"Accessibilit\xE9",
        Cat::Privacy         => b"Confidentialit\xE9 et s\xE9curit\xE9",
        Cat::Update          => b"Windows Update",
        Cat::About           => b"\xC0 propos",
    }
}

fn cat_glyph(c: Cat) -> Glyph {
    match c {
        Cat::System          => Glyph::Computer,
        Cat::Bluetooth       => Glyph::Settings,
        Cat::Network         => Glyph::Browser,
        Cat::Personalization => Glyph::Photos,
        Cat::Apps            => Glyph::Grid,
        Cat::Accounts        => Glyph::Star,
        Cat::Time            => Glyph::Settings,
        Cat::Gaming          => Glyph::Terminal,
        Cat::Accessibility   => Glyph::Star,
        Cat::Privacy         => Glyph::Settings,
        Cat::Update          => Glyph::Refresh,
        Cat::About           => Glyph::Computer,
    }
}

// ── État global ─────────────────────────────────────────────────────

static mut CUR_CAT: Cat = Cat::System;
static mut SCROLL_Y: i32 = 0;
static mut NAV_HOVER: i32 = -1;
static mut HOVER_ROW: i32 = -1;
static mut SLIDER_DRAG: i32 = -1;

// Bascules (toggles)
static mut DARK_MODE: bool = true;
static mut NIGHT_LIGHT: bool = false;
static mut BLUETOOTH_ON: bool = true;
static mut WIFI_ON: bool = true;
static mut AIRPLANE_MODE: bool = false;
static mut GAME_BAR: bool = false;
static mut GAME_MODE: bool = false;
static mut NARRATOR: bool = false;
static mut MAGNIFIER: bool = false;
static mut HIGH_CONTRAST: bool = false;
static mut LOC_SERVICES: bool = true;
static mut CAMERA_ACCESS: bool = true;
static mut MIC_ACCESS: bool = true;
static mut TASKBAR_LEFT: bool = false;
static mut NOTIF_ON: bool = true;
static mut FOCUS_ASSIST: bool = false;
static mut BATTERY_SAVER: bool = false;
static mut STORAGE_SENSE: bool = false;
static mut NEARBY_SHARE: bool = false;
static mut AUTO_UPDATE: bool = true;

// Curseurs (sliders)
static mut BRIGHTNESS: i32 = 75;
static mut VOLUME: i32 = 78;
static mut MOUSE_SPEED: i32 = 50;

// ── Dessin ──────────────────────────────────────────────────────────

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let c = f.client;

    // Fond « Mica »
    fill(g.hdc, &mut g.b, c.left, c.top, c.right, c.bottom, p.mica);

    // ── Panneau latéral ──
    let nav_r = c.left + NAV_W;
    fill(g.hdc, &mut g.b, c.left, c.top, nav_r, c.bottom, p.surface);
    fill(g.hdc, &mut g.b, nav_r, c.top, nav_r + 1, c.bottom, p.border);

    // Titre du panneau
    text(g.hdc, g.f.large, p.text, c.left + 20, c.top + 16, b"Param\xE8tres");

    // Entrées de navigation
    let nav_top = c.top + 56;
    for (i, &cat) in CATS.iter().enumerate() {
        let y = nav_top + i as i32 * 36;
        if y + 36 < c.top || y > c.bottom { continue; }

        let selected = cat == CUR_CAT;
        let hover = NAV_HOVER == i as i32;
        let row = rect(c.left + 4, y, nav_r - 4, y + 34);

        if selected {
            round_rect(g.hdc, &mut g.b, row.left, row.top, row.right, row.bottom, 4, p.selected);
            // Indicateur d'accent à gauche
            fill(g.hdc, &mut g.b, c.left + 4, y + 8, c.left + 7, y + 26, p.accent);
        } else if hover {
            round_rect(g.hdc, &mut g.b, row.left, row.top, row.right, row.bottom, 4, p.hover);
        }

        let col = if selected { p.text } else { p.text2 };
        set_glyph_bg(Some(if selected { p.selected } else if hover { p.hover } else { p.surface }));
        glyph(g.hdc, &mut g.b, &g.f, cat_glyph(cat), c.left + 16, y + 9, 16);
        set_glyph_bg(None);
        text_fit(g.hdc, g.f.body, 13, col, c.left + 40, y + 10, cat_label(cat), NAV_W - 56);
    }

    // ── Zone principale ──
    let content_l = nav_r + 24;
    let content_r = c.right - 16;
    let content_w = content_r - content_l;
    let card_w = content_w.min(640);
    let card_l = content_l;

    // Titre de la catégorie
    let base_y = c.top + 16 - SCROLL_Y;
    text(g.hdc, g.f.large, p.text, card_l, base_y, cat_label(CUR_CAT));

    let mut y = base_y + 40;

    match CUR_CAT {
        Cat::System => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"\xC9cran");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Mode sombre", &mut DARK_MODE, 0);
            y = draw_slider_row(g, &p, card_l, y, card_w, b"Luminosit\xE9", &mut BRIGHTNESS, 0);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"\xC9clairage nocturne", &mut NIGHT_LIGHT, 1);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Son");
            y = draw_slider_row(g, &p, card_l, y, card_w, b"Volume", &mut VOLUME, 1);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Notifications");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Notifications", &mut NOTIF_ON, 2);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Concentration", &mut FOCUS_ASSIST, 3);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Alimentation et batterie");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"\xC9conomiseur de batterie", &mut BATTERY_SAVER, 4);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Stockage");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Assistant de stockage", &mut STORAGE_SENSE, 5);
            y = draw_storage_bar(g, &p, card_l, y, card_w);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Bluetooth => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Bluetooth");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Bluetooth", &mut BLUETOOTH_ON, 0);
            y = draw_info_row(g, &p, card_l, y, card_w, b"Appareils connect\xE9s", b"Aucun");
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Souris");
            y = draw_slider_row(g, &p, card_l, y, card_w, b"Vitesse du pointeur", &mut MOUSE_SPEED, 2);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Network => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Wi-Fi");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Wi-Fi", &mut WIFI_ON, 0);
            y = draw_info_row(g, &p, card_l, y, card_w, b"R\xE9seau connect\xE9", b"Winlux-Network");
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Mode avion");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Mode avion", &mut AIRPLANE_MODE, 1);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Partage de proximit\xE9");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Partage de proximit\xE9", &mut NEARBY_SHARE, 2);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Personalization => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Th\xE8me");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Mode sombre", &mut DARK_MODE, 0);
            y = draw_color_row(g, &p, card_l, y, card_w);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Barre des t\xE2ches");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Aligner \xE0 gauche", &mut TASKBAR_LEFT, 1);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Apps => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Applications install\xE9es");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Folder, b"Explorateur de fichiers", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Browser, b"Navigateur", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Terminal, b"Terminal", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Settings, b"Param\xE8tres", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Calc, b"Calculatrice", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Notes, b"Bloc-notes", b"Syst\xE8me");
            y = draw_app_row(g, &p, card_l, y, card_w, Glyph::Mail, b"Courrier", b"Syst\xE8me");
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Accounts => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Votre compte");
            y = draw_account_banner(g, &p, card_l, y, card_w);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Time => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Date et heure");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Fuseau horaire", b"Europe/Paris (UTC+1)");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Format 24 heures", b"Oui");
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Langue");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Langue d'affichage", b"Fran\xE7ais (France)");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Disposition du clavier", b"Fran\xE7ais (AZERTY)");
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Gaming => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Xbox Game Bar");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Activer la Game Bar", &mut GAME_BAR, 0);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Mode jeu");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Mode jeu", &mut GAME_MODE, 1);
            y = draw_info_row(g, &p, card_l, y, card_w, b"Optimise les performances", b"pour les jeux");
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Accessibility => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Vision");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Narrateur", &mut NARRATOR, 0);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Loupe", &mut MAGNIFIER, 1);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Contraste \xE9lev\xE9", &mut HIGH_CONTRAST, 2);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Privacy => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Autorisations");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Services de localisation", &mut LOC_SERVICES, 0);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Acc\xE8s \xE0 la cam\xE9ra", &mut CAMERA_ACCESS, 1);
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Acc\xE8s au microphone", &mut MIC_ACCESS, 2);
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::Update => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Windows Update");
            y = draw_toggle_row(g, &p, card_l, y, card_w, b"Mises \xE0 jour automatiques", &mut AUTO_UPDATE, 0);
            y = draw_info_row(g, &p, card_l, y, card_w, b"\xC9tat", b"Vous \xEAtes \xE0 jour");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Derni\xE8re v\xE9rification", b"Aujourd'hui");
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
        Cat::About => {
            y = draw_card_header(g, &p, card_l, y, card_w, b"Sp\xE9cifications de l'appareil");
            y = draw_about_section(g, &p, card_l, y, card_w);
            y = draw_card_end(g, &p, card_l, y, card_w);

            y += SECTION_GAP;
            y = draw_card_header(g, &p, card_l, y, card_w, b"Sp\xE9cifications de Winlux");
            y = draw_info_row(g, &p, card_l, y, card_w, b"\xC9dition", b"Winlux 1.0");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Version", b"24H2");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Build", b"26100.1");
            y = draw_info_row(g, &p, card_l, y, card_w, b"Runtime", b"winlux-runtime (Rust, natif)");
            y = draw_card_end(g, &p, card_l, y, card_w);
        }
    }

    // Suppression d'avertissement sur y non lu
    let _ = y;
}

// ── Composants de carte ─────────────────────────────────────────────

/// En-tête d'une carte : fond arrondi en haut, titre en gras.
unsafe fn draw_card_header(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32, label: &[u8]) -> i32 {
    // Fond arrondi du haut de la carte
    round_rect(g.hdc, &mut g.b, x, y, x + w, y + 8 + CARD_RADIUS, CARD_RADIUS, p.surface);
    fill(g.hdc, &mut g.b, x, y + CARD_RADIUS, x + w, y + 8 + CARD_RADIUS, p.surface);
    text(g.hdc, g.f.semibold, p.text, x + 16, y + 12, label);
    // Liseré supérieur
    let accent_w = 32.min(w / 4);
    round_rect(g.hdc, &mut g.b, x + 16, y, x + 16 + accent_w, y + 3, 2, p.accent);
    y + 36
}

/// Bas arrondi de la carte (ferme le rectangle).
unsafe fn draw_card_end(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + CARD_RADIUS, p.surface);
    round_rect(g.hdc, &mut g.b, x, y, x + w, y + CARD_RADIUS + 4, CARD_RADIUS, p.surface);
    // Bordure fine
    // (dessinée par le bord de la card)
    y + CARD_RADIUS + 4
}

/// Ligne avec une bascule on/off.
unsafe fn draw_toggle_row(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32, label: &[u8], val: &mut bool, _id: i32) -> i32 {
    // Fond
    let hover = HOVER_ROW == y;
    if hover {
        fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H, p.hover);
    } else {
        fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H, p.surface);
    }
    // Séparateur
    fill(g.hdc, &mut g.b, x + 16, y + ROW_H - 1, x + w - 16, y + ROW_H, p.border);
    // Texte
    text(g.hdc, g.f.body, p.text, x + 16, y + 12, label);
    // Toggle
    let tx = x + w - 16 - TOGGLE_W;
    let ty = y + (ROW_H - TOGGLE_H) / 2;
    draw_toggle(g, p, tx, ty, *val);
    y + ROW_H
}

/// Dessine un toggle switch.
unsafe fn draw_toggle(g: &mut Gfx, p: &Palette, x: i32, y: i32, on: bool) {
    let bg = if on { p.accent } else { p.border };
    round_rect(g.hdc, &mut g.b, x, y, x + TOGGLE_W, y + TOGGLE_H, TOGGLE_H / 2, bg);
    let knob_x = if on { x + TOGGLE_W - TOGGLE_H + 2 } else { x + 2 };
    let knob_col = if on { p.on_accent } else { p.text3 };
    ellipse(g.hdc, &mut g.b, knob_x, y + 2, knob_x + TOGGLE_H - 4, y + TOGGLE_H - 2, knob_col);
}

/// Ligne avec un curseur (slider).
unsafe fn draw_slider_row(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32, label: &[u8], val: &mut i32, _id: i32) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H + 8, p.surface);
    fill(g.hdc, &mut g.b, x + 16, y + ROW_H + 7, x + w - 16, y + ROW_H + 8, p.border);
    text(g.hdc, g.f.body, p.text, x + 16, y + 6, label);
    // Valeur
    let mut buf = [0u8; 8];
    let vlen = fmt_int(&mut buf, *val);
    text(g.hdc, g.f.body, p.text2, x + w - 52, y + 6, &buf[..vlen]);
    text(g.hdc, g.f.body, p.text3, x + w - 36, y + 6, b"%");
    // Rail
    let sx = x + 16;
    let sy = y + 32;
    let sw = SLIDER_W.min(w - 48);
    fill(g.hdc, &mut g.b, sx, sy, sx + sw, sy + SLIDER_H, p.border);
    // Remplissage
    let fill_w = sw * (*val).clamp(0, 100) / 100;
    fill(g.hdc, &mut g.b, sx, sy, sx + fill_w, sy + SLIDER_H, p.accent);
    // Poignée
    let kx = sx + fill_w;
    ellipse(g.hdc, &mut g.b, kx - 7, sy - 5, kx + 7, sy + SLIDER_H + 5, p.accent);
    ellipse(g.hdc, &mut g.b, kx - 4, sy - 2, kx + 4, sy + SLIDER_H + 2, p.surface);
    y + ROW_H + 8
}

/// Ligne d'information (label + valeur).
unsafe fn draw_info_row(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32, label: &[u8], val: &[u8]) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H, p.surface);
    fill(g.hdc, &mut g.b, x + 16, y + ROW_H - 1, x + w - 16, y + ROW_H, p.border);
    text(g.hdc, g.f.body, p.text, x + 16, y + 12, label);
    let vw = text_width(val, 13);
    text(g.hdc, g.f.body, p.text3, x + w - 16 - vw, y + 12, val);
    y + ROW_H
}

/// Barre de stockage (environ 60 % utilisés).
unsafe fn draw_storage_bar(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + 48, p.surface);
    let bx = x + 16;
    let by = y + 8;
    let bw = w - 32;
    round_rect(g.hdc, &mut g.b, bx, by, bx + bw, by + 12, 6, p.border);
    round_rect(g.hdc, &mut g.b, bx, by, bx + bw * 58 / 100, by + 12, 6, p.accent);
    text(g.hdc, g.f.small, p.text3, bx, by + 18, b"117 Go utilis\xE9s sur 200 Go");
    y + 48
}

/// Ligne d'application installée.
unsafe fn draw_app_row(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32, icon: Glyph, name: &[u8], info: &[u8]) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H + 4, p.surface);
    fill(g.hdc, &mut g.b, x + 16, y + ROW_H + 3, x + w - 16, y + ROW_H + 4, p.border);
    set_glyph_bg(Some(p.surface));
    glyph(g.hdc, &mut g.b, &g.f, icon, x + 16, y + 12, 18);
    set_glyph_bg(None);
    text(g.hdc, g.f.body, p.text, x + 44, y + 8, name);
    text(g.hdc, g.f.small, p.text3, x + 44, y + 24, info);
    y + ROW_H + 4
}

/// Bannière du compte utilisateur.
unsafe fn draw_account_banner(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + 72, p.surface);
    // Avatar : cercle avec initiale
    ellipse(g.hdc, &mut g.b, x + 20, y + 12, x + 60, y + 52, p.accent);
    text_centered(g.hdc, g.f.large, 22, p.on_accent, x + 40, y + 18, b"D");
    text(g.hdc, g.f.semibold, p.text, x + 72, y + 16, b"Utilisateur Winlux");
    text(g.hdc, g.f.body, p.text3, x + 72, y + 36, b"Compte local");
    y + 72
}

/// Rangée de pastilles de couleur pour le choix d'accent.
unsafe fn draw_color_row(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32) -> i32 {
    fill(g.hdc, &mut g.b, x, y, x + w, y + ROW_H + 8, p.surface);
    fill(g.hdc, &mut g.b, x + 16, y + ROW_H + 7, x + w - 16, y + ROW_H + 8, p.border);
    text(g.hdc, g.f.body, p.text, x + 16, y + 6, b"Couleur d'accentuation");
    let colors: [u32; 8] = [
        rgb(0x4C, 0xC2, 0xFF), rgb(0xFF, 0x88, 0x3E), rgb(0x00, 0xCC, 0x6A),
        rgb(0xE8, 0x3E, 0x6C), rgb(0xBB, 0x86, 0xFC), rgb(0xFF, 0xB9, 0x00),
        rgb(0x00, 0xB2, 0x94), rgb(0xFF, 0x63, 0x47),
    ];
    let accent = ACCENT;
    for (i, &col) in colors.iter().enumerate() {
        let cx = x + 16 + i as i32 * 30;
        let cy = y + 28;
        ellipse(g.hdc, &mut g.b, cx, cy, cx + 22, cy + 22, col);
        if col == accent {
            ring(g.hdc, &mut g.b, cx - 2, cy - 2, cx + 24, cy + 24, 2, p.text, col);
        }
    }
    y + ROW_H + 8
}

/// Section « À propos » avec info système.
unsafe fn draw_about_section(g: &mut Gfx, p: &Palette, x: i32, y: i32, w: i32) -> i32 {
    let mut cy = y;
    cy = draw_info_row(g, p, x, cy, w, b"Nom de l'appareil", b"Winlux-PC");
    cy = draw_info_row(g, p, x, cy, w, b"Processeur", b"Linux (natif)");
    cy = draw_info_row(g, p, x, cy, w, b"RAM install\xE9e", b"Voir /proc/meminfo");
    cy = draw_info_row(g, p, x, cy, w, b"Type du syst\xE8me", b"Syst\xE8me d'exploitation 64 bits");
    cy
}

/// Formate un entier positif en décimal dans un buffer.
fn fmt_int(buf: &mut [u8; 8], mut v: i32) -> usize {
    if v <= 0 { buf[0] = b'0'; return 1; }
    let mut tmp = [0u8; 8];
    let mut i = 0usize;
    while v > 0 && i < 8 { tmp[i] = b'0' + (v % 10) as u8; v /= 10; i += 1; }
    for j in 0..i { buf[j] = tmp[i - 1 - j]; }
    i
}

// ── Interaction souris ──────────────────────────────────────────────

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, f: &Frame) -> bool {
    let c = f.client;
    let nav_r = c.left + NAV_W;

    match ev {
        Mouse::Move => {
            // Survol navigation
            let nav_top = c.top + 56;
            let old_hover = NAV_HOVER;
            NAV_HOVER = -1;
            if mx >= c.left && mx < nav_r && my >= nav_top {
                let idx = (my - nav_top) / 36;
                if idx >= 0 && (idx as usize) < CATS.len() {
                    NAV_HOVER = idx;
                }
            }
            // Survol rangées
            HOVER_ROW = -1;
            // Glissement curseur
            if SLIDER_DRAG >= 0 {
                let sx = nav_r + 40;
                let sw = SLIDER_W.min((c.right - c.left - NAV_W) - 72);
                let pct = ((mx - sx) * 100 / sw.max(1)).clamp(0, 100);
                match SLIDER_DRAG {
                    0 => BRIGHTNESS = pct,
                    1 => VOLUME = pct,
                    2 => MOUSE_SPEED = pct,
                    _ => {}
                }
                return true;
            }
            return old_hover != NAV_HOVER;
        }
        Mouse::Down => {
            // Clic dans la navigation
            if mx >= c.left && mx < nav_r {
                let nav_top = c.top + 56;
                let idx = (my - nav_top) / 36;
                if idx >= 0 && (idx as usize) < CATS.len() {
                    CUR_CAT = CATS[idx as usize];
                    SCROLL_Y = 0;
                    return true;
                }
                return false;
            }
            // Zone contenu : toggles, sliders, couleurs
            let content_l = nav_r + 24;
            let content_w = (c.right - 16) - content_l;
            let card_w = content_w.min(640);
            let base_y = c.top + 16 - SCROLL_Y;

            // Détection clics toggles et sliders dans la catégorie courante
            return handle_content_click(mx, my, content_l, card_w, base_y, c);
        }
        Mouse::Up => {
            if SLIDER_DRAG >= 0 { SLIDER_DRAG = -1; return true; }
            // Appliquer les effets des toggles
            apply_toggle_effects();
        }
        Mouse::Wheel(delta) => {
            SCROLL_Y = (SCROLL_Y - delta * 40).max(0);
            return true;
        }
        _ => {}
    }
    false
}

/// Gère un clic dans la zone de contenu. Renvoie vrai si un élément est touché.
unsafe fn handle_content_click(mx: i32, my: i32, card_l: i32, card_w: i32, base_y: i32, _c: Rect) -> bool {
    // Calcul des positions des toggles / sliders selon la catégorie
    // On simule le layout draw pour retrouver les coordonnées.
    let mut y = base_y + 40;
    let toggle_x = card_l + card_w - 16 - TOGGLE_W;

    match CUR_CAT {
        Cat::System => {
            y += 36; // header
            if try_toggle(mx, my, toggle_x, y, &mut DARK_MODE) { return true; } y += ROW_H;
            if try_slider(mx, my, card_l + 16, y + 26, 0) { return true; } y += ROW_H + 8;
            if try_toggle(mx, my, toggle_x, y, &mut NIGHT_LIGHT) { return true; } y += ROW_H;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36; // card end + gap + header
            if try_slider(mx, my, card_l + 16, y + 26, 1) { return true; } y += ROW_H + 8;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut NOTIF_ON) { return true; } y += ROW_H;
            if try_toggle(mx, my, toggle_x, y, &mut FOCUS_ASSIST) { return true; } y += ROW_H;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut BATTERY_SAVER) { return true; } y += ROW_H;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut STORAGE_SENSE) { return true; }
        }
        Cat::Bluetooth => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut BLUETOOTH_ON) { return true; } y += ROW_H;
            y += ROW_H + CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_slider(mx, my, card_l + 16, y + 26, 2) { return true; }
        }
        Cat::Network => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut WIFI_ON) { return true; } y += ROW_H;
            y += ROW_H + CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut AIRPLANE_MODE) { return true; } y += ROW_H;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut NEARBY_SHARE) { return true; }
        }
        Cat::Personalization => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut DARK_MODE) { return true; } y += ROW_H;
            // Couleurs d'accent
            if my >= y + 28 && my < y + 50 {
                let colors: [u32; 8] = [
                    rgb(0x4C, 0xC2, 0xFF), rgb(0xFF, 0x88, 0x3E), rgb(0x00, 0xCC, 0x6A),
                    rgb(0xE8, 0x3E, 0x6C), rgb(0xBB, 0x86, 0xFC), rgb(0xFF, 0xB9, 0x00),
                    rgb(0x00, 0xB2, 0x94), rgb(0xFF, 0x63, 0x47),
                ];
                for (i, &col) in colors.iter().enumerate() {
                    let cx = card_l + 16 + i as i32 * 30;
                    if mx >= cx && mx < cx + 22 {
                        ACCENT = col;
                        return true;
                    }
                }
            }
            y += ROW_H + 8 + CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut TASKBAR_LEFT) { return true; }
        }
        Cat::Gaming => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut GAME_BAR) { return true; } y += ROW_H;
            y += CARD_RADIUS + 4 + SECTION_GAP + 36;
            if try_toggle(mx, my, toggle_x, y, &mut GAME_MODE) { return true; }
        }
        Cat::Accessibility => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut NARRATOR) { return true; } y += ROW_H;
            if try_toggle(mx, my, toggle_x, y, &mut MAGNIFIER) { return true; } y += ROW_H;
            if try_toggle(mx, my, toggle_x, y, &mut HIGH_CONTRAST) { return true; }
        }
        Cat::Privacy => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut LOC_SERVICES) { return true; } y += ROW_H;
            if try_toggle(mx, my, toggle_x, y, &mut CAMERA_ACCESS) { return true; } y += ROW_H;
            if try_toggle(mx, my, toggle_x, y, &mut MIC_ACCESS) { return true; }
        }
        Cat::Update => {
            y += 36;
            if try_toggle(mx, my, toggle_x, y, &mut AUTO_UPDATE) { return true; }
        }
        _ => {}
    }
    false
}

/// Test de clic sur un toggle : retourne vrai si touché.
unsafe fn try_toggle(mx: i32, my: i32, tx: i32, row_y: i32, val: &mut bool) -> bool {
    let ty = row_y + (ROW_H - TOGGLE_H) / 2;
    if mx >= tx && mx <= tx + TOGGLE_W && my >= ty && my <= ty + TOGGLE_H {
        *val = !*val;
        true
    } else {
        false
    }
}

/// Test de clic sur un slider : retourne vrai si touché (lance le drag).
unsafe fn try_slider(mx: i32, my: i32, sx: i32, sy: i32, id: i32) -> bool {
    if my >= sy - 10 && my <= sy + SLIDER_H + 10 && mx >= sx && mx <= sx + SLIDER_W {
        SLIDER_DRAG = id;
        true
    } else {
        false
    }
}

/// Applique les effets secondaires des bascules (thème, barre des tâches…).
unsafe fn apply_toggle_effects() {
    // Synchronise le mode sombre
    DARK = DARK_MODE;
    // Synchronise l'alignement de la barre des tâches
    crate::desktop::ALIGN_LEFT = TASKBAR_LEFT;
    // Volume
    crate::desktop::VOLUME_LEVEL = VOLUME as u32;
}

// ── Clavier ─────────────────────────────────────────────────────────

pub unsafe fn key(_vk: u32, _ch: u16) -> bool { false }

// ── Tick ────────────────────────────────────────────────────────────

pub unsafe fn tick(_f: &Frame) -> bool { false }
