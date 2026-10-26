//! Gestionnaire de fenêtres du bureau Winlux : les applications intégrées
//! (Explorer, Paramètres) sont des fenêtres dessinées dans le bureau, avec
//! barre de titre Windows 11, boutons réduire/agrandir/fermer, déplacement,
//! redimensionnement par les bords, ancrage (haut = plein écran, côtés =
//! moitié d'écran) et animations d'ouverture, de fermeture et de réduction.
//!
//! Contrat d'une application (voir `explorer.rs`, `settings.rs`) :
//! - `draw(g, frame)` dessine le contenu ; `frame.title` est la partie libre de
//!   la barre de titre (l'application peut y mettre des onglets si
//!   `CUSTOM_TITLE` est vrai), `frame.client` la zone sous la barre ;
//! - `mouse(ev, x, y, frame) -> bool` reçoit les événements souris en
//!   coordonnées écran et renvoie vrai s'il faut repeindre ; un clic dans
//!   `frame.title` non consommé (faux) déplace la fenêtre ;
//! - `key(vk, ch) -> bool` reçoit le clavier quand la fenêtre est active
//!   (`ch` non nul pour un caractère tapé, `vk` sinon) ;
//! - `tick(frame) -> bool` est appelé deux fois par seconde.

use crate::gfx::*;
use winlux_testexe::*;

pub const APP_EXPLORER: usize = 0;
pub const APP_SETTINGS: usize = 1;
pub const APP_COUNT: usize = 2;

pub const TITLE_H: i32 = 40;
const CAPTION_W: i32 = 46;
const BORDER: i32 = 6;
const OPEN_MS: i32 = 190;
const CLOSE_MS: i32 = 130;
const MINIMIZE_MS: i32 = 210;

#[derive(Clone, Copy, PartialEq)]
pub enum Mouse {
    Down,
    Up,
    Move,
    Wheel(i32),
    DoubleClick,
    RightDown,
    MiddleDown,
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub outer: Rect,
    pub title: Rect,
    pub client: Rect,
}

#[derive(Clone, Copy, PartialEq)]
enum Anim {
    None,
    Open,
    Close,
    Minimize,
    Restore,
}

#[derive(Clone, Copy)]
struct Win {
    open: bool,
    minimized: bool,
    maximized: bool,
    rect: Rect,
    restore: Rect,
    anim: Anim,
    anim_start: u32,
}

const CLOSED: Win = Win {
    open: false,
    minimized: false,
    maximized: false,
    rect: rect(0, 0, 0, 0),
    restore: rect(0, 0, 0, 0),
    anim: Anim::None,
    anim_start: 0,
};

static mut WINS: [Win; APP_COUNT] = [CLOSED; APP_COUNT];
/// Ordre d'empilement, du fond vers le premier plan.
static mut Z: [usize; APP_COUNT] = [0, 1];
/// Fenêtre active (au premier plan et non réduite), ou `usize::MAX`.
static mut ACTIVE: usize = usize::MAX;

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Move,
    Resize {
        left: bool,
        top: bool,
        right: bool,
        bottom: bool,
    },
}
static mut DRAG: Drag = Drag::None;
static mut DRAG_APP: usize = 0;
static mut DRAG_X: i32 = 0;
static mut DRAG_Y: i32 = 0;
static mut DRAG_RECT: Rect = rect(0, 0, 0, 0);
/// Zone d'ancrage prévisualisée pendant un déplacement.
static mut SNAP_PREVIEW: Option<Rect> = None;
/// Application qui reçoit la souris pendant un appui commencé chez elle.
static mut CAPTURE: usize = usize::MAX;
static mut HOVER_CAPTION: i32 = -1;
static mut LAST_CLICK_TIME: u32 = 0;
static mut LAST_CLICK_X: i32 = -100;
static mut LAST_CLICK_Y: i32 = -100;
/// Zone de travail (écran moins la barre des tâches) et bouton de barre des
/// tâches de chaque application, cible de l'animation de réduction.
pub static mut WORK: Rect = rect(0, 0, 1280, 672);
pub static mut TASKBAR_SLOT: [Rect; APP_COUNT] = [rect(0, 0, 0, 0); APP_COUNT];
pub static mut MOUSE_X: i32 = -1;
pub static mut MOUSE_Y: i32 = -1;

pub fn title_of(app: usize) -> &'static [u8] {
    match app {
        APP_EXPLORER => b"Explorer",
        _ => b"Param\xE8tres",
    }
}
fn glyph_of(app: usize) -> Glyph {
    match app {
        APP_EXPLORER => Glyph::Folder,
        _ => Glyph::Settings,
    }
}
fn custom_title(app: usize) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::CUSTOM_TITLE,
        _ => crate::settings::CUSTOM_TITLE,
    }
}

unsafe fn app_draw(app: usize, g: &mut Gfx, f: &Frame) {
    match app {
        APP_EXPLORER => crate::explorer::draw(g, f),
        _ => crate::settings::draw(g, f),
    }
}
unsafe fn app_mouse(app: usize, ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::mouse(ev, x, y, f),
        _ => crate::settings::mouse(ev, x, y, f),
    }
}
unsafe fn app_key(app: usize, vk: u32, ch: u16) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::key(vk, ch),
        _ => crate::settings::key(vk, ch),
    }
}
unsafe fn app_tick(app: usize, f: &Frame) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::tick(f),
        _ => crate::settings::tick(f),
    }
}

/// Contexte de dessin passé aux applications.
pub struct Gfx<'a> {
    pub hdc: Handle,
    pub b: &'a mut Brushes,
    pub f: &'a Fonts,
}

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn elapsed(start: u32) -> i32 {
    now().wrapping_sub(start).min(100_000) as i32
}

pub fn frame_of(r: &Rect) -> Frame {
    let title = rect(
        r.left + 1,
        r.top + 1,
        r.right - 3 * CAPTION_W - 1,
        r.top + TITLE_H,
    );
    Frame {
        outer: *r,
        title,
        client: rect(r.left + 1, r.top + TITLE_H, r.right - 1, r.bottom - 1),
    }
}

pub fn is_open(app: usize) -> bool {
    unsafe { WINS[app].open }
}
pub fn is_visible(app: usize) -> bool {
    unsafe { WINS[app].open && !WINS[app].minimized }
}
pub fn is_active(app: usize) -> bool {
    unsafe { ACTIVE == app }
}

/// Vrai tant qu'une animation est en cours : la boucle principale repeint
/// alors à ~60 images par seconde.
pub fn animating() -> bool {
    unsafe {
        for w in WINS.iter() {
            if w.anim != Anim::None {
                let d = match w.anim {
                    Anim::Close => CLOSE_MS,
                    Anim::Minimize | Anim::Restore => MINIMIZE_MS,
                    _ => OPEN_MS,
                };
                if elapsed(w.anim_start) <= d + 20 {
                    return true;
                }
            }
        }
        false
    }
}

unsafe fn raise(app: usize) {
    let mut k = 0;
    let mut order = [0usize; APP_COUNT];
    for &a in Z.iter() {
        if a != app {
            order[k] = a;
            k += 1;
        }
    }
    order[APP_COUNT - 1] = app;
    Z = order;
    ACTIVE = app;
}

unsafe fn refresh_active() {
    ACTIVE = usize::MAX;
    for &a in Z.iter().rev() {
        if WINS[a].open && !WINS[a].minimized {
            ACTIVE = a;
            return;
        }
    }
}

fn default_rect(app: usize) -> Rect {
    let wk = unsafe { WORK };
    let (ww, wh) = (wk.right - wk.left, wk.bottom - wk.top);
    let (w, h) = match app {
        APP_EXPLORER => (1100, 680),
        _ => (1040, 700),
    };
    let (w, h) = (w.min(ww - 40).max(480), h.min(wh - 40).max(320));
    let offset = app as i32 * 36;
    let x = wk.left + (ww - w) / 2 + offset - 18;
    let y = wk.top + (wh - h) / 2 + offset - 18;
    rect(
        x.max(wk.left),
        y.max(wk.top),
        x.max(wk.left) + w,
        y.max(wk.top) + h,
    )
}

/// Ouvre l'application, ou la ramène au premier plan si elle est déjà ouverte.
pub unsafe fn open(app: usize) {
    let w = &mut WINS[app];
    if !w.open {
        *w = Win {
            open: true,
            minimized: false,
            maximized: false,
            rect: default_rect(app),
            restore: default_rect(app),
            anim: Anim::Open,
            anim_start: now(),
        };
    } else if w.minimized {
        w.minimized = false;
        w.anim = Anim::Restore;
        w.anim_start = now();
    }
    raise(app);
}

/// Clic sur le bouton de la barre des tâches : ouvre, réduit ou restaure.
pub unsafe fn toggle(app: usize) {
    if WINS[app].open && !WINS[app].minimized && ACTIVE == app {
        minimize(app);
    } else {
        open(app);
    }
}

pub unsafe fn minimize(app: usize) {
    let w = &mut WINS[app];
    if !w.open || w.minimized {
        return;
    }
    w.minimized = true;
    w.anim = Anim::Minimize;
    w.anim_start = now();
    refresh_active();
}

pub unsafe fn close(app: usize) {
    let w = &mut WINS[app];
    if !w.open {
        return;
    }
    w.anim = Anim::Close;
    w.anim_start = now();
    if CAPTURE == app {
        CAPTURE = usize::MAX;
    }
}

pub unsafe fn toggle_maximize(app: usize) {
    let w = &mut WINS[app];
    if w.maximized {
        w.maximized = false;
        w.rect = w.restore;
    } else {
        w.restore = w.rect;
        w.maximized = true;
        w.rect = WORK;
    }
    w.anim = Anim::Open;
    w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
}

/// Suit un changement de résolution : les fenêtres restent dans l'écran.
pub unsafe fn on_resize() {
    for w in WINS.iter_mut() {
        if !w.open {
            continue;
        }
        if w.maximized {
            w.rect = WORK;
            continue;
        }
        let (rw, rh) = (
            (w.rect.right - w.rect.left).min(WORK.right - WORK.left),
            (w.rect.bottom - w.rect.top).min(WORK.bottom - WORK.top),
        );
        let x = w.rect.left.clamp(WORK.left, WORK.right - rw);
        let y = w.rect.top.clamp(WORK.top, WORK.bottom - rh);
        w.rect = rect(x, y, x + rw, y + rh);
    }
}

// ---------- Dessin ----------

fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + (b - a) * t / 1000
}
fn lerp_rect(a: &Rect, b: &Rect, t: i32) -> Rect {
    rect(
        lerp(a.left, b.left, t),
        lerp(a.top, b.top, t),
        lerp(a.right, b.right, t),
        lerp(a.bottom, b.bottom, t),
    )
}
fn scaled(r: &Rect, milli: i32, dy: i32) -> Rect {
    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    let (hw, hh) = (
        (r.right - r.left) * milli / 2000,
        (r.bottom - r.top) * milli / 2000,
    );
    rect(cx - hw, cy - hh + dy, cx + hw, cy + hh + dy)
}

fn caption_rect(r: &Rect, index: i32) -> Rect {
    let right = r.right - 1 - (2 - index) * CAPTION_W;
    rect(right - CAPTION_W, r.top + 1, right, r.top + TITLE_H - 8)
}

unsafe fn draw_window(g: &mut Gfx, app: usize) {
    let w = WINS[app];
    let p = pal();
    let t = elapsed(w.anim_start);
    // Rectangle affiché : animé, ou définitif.
    let (shown, content) = match w.anim {
        Anim::Open if t < OPEN_MS => {
            let e = ease_out(t, OPEN_MS);
            (
                scaled(&w.rect, 940 + e * 60 / 1000, (1000 - e) * 18 / 1000),
                e > 700,
            )
        }
        Anim::Close if t < CLOSE_MS => {
            let e = ease_out(t, CLOSE_MS);
            (scaled(&w.rect, 1000 - e * 60 / 1000, e * 10 / 1000), false)
        }
        Anim::Close => {
            WINS[app].open = false;
            WINS[app].anim = Anim::None;
            refresh_active();
            return;
        }
        Anim::Minimize if t < MINIMIZE_MS => (
            lerp_rect(&w.rect, &TASKBAR_SLOT[app], ease_out(t, MINIMIZE_MS)),
            false,
        ),
        Anim::Restore if t < MINIMIZE_MS => (
            lerp_rect(&TASKBAR_SLOT[app], &w.rect, ease_out(t, MINIMIZE_MS)),
            false,
        ),
        _ => {
            if w.anim != Anim::None {
                WINS[app].anim = Anim::None;
            }
            if w.minimized {
                return;
            }
            (w.rect, true)
        }
    };
    if w.minimized && w.anim != Anim::Minimize {
        return;
    }
    let radius = if w.maximized { 0 } else { 8 };
    let active = ACTIVE == app;
    // Liseré fin et fond « Mica » (pas d'ombre lourde).
    round_rect(
        g.hdc,
        g.b,
        shown.left,
        shown.top,
        shown.right,
        shown.bottom,
        radius,
        if active {
            mix(p.border, p.accent, 2, 10)
        } else {
            p.border
        },
    );
    round_rect(
        g.hdc,
        g.b,
        shown.left + 1,
        shown.top + 1,
        shown.right - 1,
        shown.bottom - 1,
        radius,
        p.mica,
    );
    if !content || shown.right - shown.left < 200 || shown.bottom - shown.top < 120 {
        glyph(
            g.hdc,
            g.b,
            g.f,
            glyph_of(app),
            (shown.left + shown.right) / 2 - 16,
            (shown.top + shown.bottom) / 2 - 16,
            32,
        );
        return;
    }
    let fr = frame_of(&shown);
    fill(
        g.hdc,
        g.b,
        fr.client.left,
        fr.client.top,
        fr.client.right,
        fr.client.bottom - radius,
        p.window,
    );
    if radius > 0 {
        round_rect(
            g.hdc,
            g.b,
            fr.client.left,
            fr.client.bottom - radius * 2,
            fr.client.right,
            fr.client.bottom,
            radius - 1,
            p.window,
        );
    }
    if !custom_title(app) {
        glyph(
            g.hdc,
            g.b,
            g.f,
            glyph_of(app),
            fr.title.left + 14,
            fr.title.top + 11,
            16,
        );
        text(
            g.hdc,
            g.f.caption,
            if active { p.text } else { p.text3 },
            fr.title.left + 42,
            fr.title.top + 12,
            title_of(app),
        );
    }
    app_draw(app, g, &fr);
    // Boutons de légende : réduire, agrandir/restaurer, fermer.
    for i in 0..3 {
        let r = caption_rect(&shown, i);
        let hover = HOVER_CAPTION == i && ACTIVE == app;
        if hover {
            let c = if i == 2 {
                rgb(0xC4, 0x2B, 0x1C)
            } else {
                p.hover
            };
            if i == 2 && radius > 0 {
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom + 8,
                    radius - 1,
                    c,
                );
                fill(g.hdc, g.b, r.left, r.top, r.right - radius, r.bottom, c);
                fill(g.hdc, g.b, r.left, r.top + radius, r.right, r.bottom, c);
            } else {
                fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, c);
            }
        }
        let gl = match i {
            0 => Glyph::Minimize,
            1 => {
                if w.maximized {
                    Glyph::Restore
                } else {
                    Glyph::Maximize
                }
            }
            _ => Glyph::Close,
        };
        let bg = if hover {
            if i == 2 {
                rgb(0xC4, 0x2B, 0x1C)
            } else {
                p.hover
            }
        } else {
            p.mica
        };
        set_glyph_bg(Some(bg));
        glyph(
            g.hdc,
            g.b,
            g.f,
            gl,
            (r.left + r.right) / 2 - 7,
            (r.top + r.bottom) / 2 - 7,
            14,
        );
        set_glyph_bg(None);
    }
}

/// Dessine toutes les fenêtres, de la plus ancienne à celle au premier plan.
pub unsafe fn draw(g: &mut Gfx) {
    if let Some(r) = SNAP_PREVIEW {
        let p = pal();
        round_rect(
            g.hdc,
            g.b,
            r.left + 6,
            r.top + 6,
            r.right - 6,
            r.bottom - 6,
            8,
            mix(p.accent, p.mica, 6, 10),
        );
        round_rect(
            g.hdc,
            g.b,
            r.left + 8,
            r.top + 8,
            r.right - 8,
            r.bottom - 8,
            7,
            mix(p.accent, p.mica, 8, 10),
        );
    }
    for &app in Z.iter() {
        if WINS[app].open {
            draw_window(g, app);
        }
    }
}

// ---------- Souris et clavier ----------

unsafe fn top_at(x: i32, y: i32) -> Option<usize> {
    for &app in Z.iter().rev() {
        let w = &WINS[app];
        if w.open && !w.minimized && w.anim != Anim::Close {
            let r = w.rect;
            if x >= r.left - BORDER
                && x < r.right + BORDER
                && y >= r.top - BORDER
                && y < r.bottom + BORDER
            {
                return Some(app);
            }
        }
    }
    None
}

unsafe fn snap_target(x: i32, y: i32) -> Option<Rect> {
    let wk = WORK;
    let mid = (wk.left + wk.right) / 2;
    if y <= wk.top + 2 {
        Some(wk)
    } else if x <= wk.left + 2 {
        Some(rect(wk.left, wk.top, mid, wk.bottom))
    } else if x >= wk.right - 3 {
        Some(rect(mid, wk.top, wk.right, wk.bottom))
    } else {
        None
    }
}

/// Bouton gauche enfoncé : renvoie vrai si une fenêtre l'a pris.
pub unsafe fn on_button_down(x: i32, y: i32, repaint: &mut bool) -> bool {
    let Some(app) = top_at(x, y) else {
        if ACTIVE != usize::MAX {
            ACTIVE = usize::MAX;
            *repaint = true;
        }
        return false;
    };
    if ACTIVE != app || Z[APP_COUNT - 1] != app {
        raise(app);
        *repaint = true;
    }
    let w = WINS[app];
    let r = w.rect;
    let fr = frame_of(&r);
    // Double-clic : même endroit en moins de 500 ms.
    let t = now();
    let double = t.wrapping_sub(LAST_CLICK_TIME) < 500
        && (x - LAST_CLICK_X).abs() < 5
        && (y - LAST_CLICK_Y).abs() < 5;
    LAST_CLICK_TIME = if double { 0 } else { t };
    LAST_CLICK_X = x;
    LAST_CLICK_Y = y;
    // Bords : redimensionnement.
    if !w.maximized {
        let left = x < r.left + 4;
        let right = x >= r.right - 4;
        let top = y < r.top + 4;
        let bottom = y >= r.bottom - 4;
        if left || right || top || bottom {
            DRAG = Drag::Resize {
                left,
                top,
                right,
                bottom,
            };
            DRAG_APP = app;
            DRAG_X = x;
            DRAG_Y = y;
            DRAG_RECT = r;
            return true;
        }
    }
    for i in 0..3 {
        if inside(&caption_rect(&r, i), x, y) {
            match i {
                0 => minimize(app),
                1 => toggle_maximize(app),
                _ => close(app),
            }
            *repaint = true;
            return true;
        }
    }
    let ev = if double {
        Mouse::DoubleClick
    } else {
        Mouse::Down
    };
    if inside(&fr.client, x, y) || (custom_title(app) && inside(&fr.title, x, y)) {
        if app_mouse(app, ev, x, y, &fr) {
            *repaint = true;
            CAPTURE = app;
            return true;
        }
        if inside(&fr.client, x, y) {
            CAPTURE = app;
            return true;
        }
    }
    if y < r.top + TITLE_H {
        if double {
            toggle_maximize(app);
            *repaint = true;
            return true;
        }
        DRAG = Drag::Move;
        DRAG_APP = app;
        DRAG_X = x;
        DRAG_Y = y;
        DRAG_RECT = r;
    }
    true
}

pub unsafe fn on_mouse_move(x: i32, y: i32, repaint: &mut bool) -> bool {
    MOUSE_X = x;
    MOUSE_Y = y;
    match DRAG {
        Drag::Move => {
            let w = &mut WINS[DRAG_APP];
            if w.maximized {
                // Sortir du plein écran en tirant la barre de titre.
                if (y - DRAG_Y).abs() < 6 && (x - DRAG_X).abs() < 6 {
                    return true;
                }
                let (rw, rh) = (
                    w.restore.right - w.restore.left,
                    w.restore.bottom - w.restore.top,
                );
                w.maximized = false;
                let left = x - rw * (x - w.rect.left) / (w.rect.right - w.rect.left).max(1);
                w.rect = rect(left, w.rect.top, left + rw, w.rect.top + rh);
                DRAG_RECT = w.rect;
                DRAG_X = x;
                DRAG_Y = y;
            }
            let (dx, dy) = (x - DRAG_X, y - DRAG_Y);
            let nt = (DRAG_RECT.top + dy).clamp(WORK.top, WORK.bottom - TITLE_H);
            w.rect = rect(
                DRAG_RECT.left + dx,
                nt,
                DRAG_RECT.right + dx,
                nt + DRAG_RECT.bottom - DRAG_RECT.top,
            );
            SNAP_PREVIEW = snap_target(x, y);
            *repaint = true;
            true
        }
        Drag::Resize {
            left,
            top,
            right,
            bottom,
        } => {
            let w = &mut WINS[DRAG_APP];
            let (dx, dy) = (x - DRAG_X, y - DRAG_Y);
            let mut r = DRAG_RECT;
            if left {
                r.left = (r.left + dx).min(r.right - 480);
            }
            if right {
                r.right = (r.right + dx).max(r.left + 480);
            }
            if top {
                r.top = (r.top + dy).clamp(WORK.top, r.bottom - 320);
            }
            if bottom {
                r.bottom = (r.bottom + dy).max(r.top + 320);
            }
            w.rect = r;
            *repaint = true;
            true
        }
        Drag::None => {
            let mut hover = -1;
            if let Some(app) = top_at(x, y) {
                if app == ACTIVE {
                    for i in 0..3 {
                        if inside(&caption_rect(&WINS[app].rect, i), x, y) {
                            hover = i;
                        }
                    }
                }
            }
            if hover != HOVER_CAPTION {
                HOVER_CAPTION = hover;
                *repaint = true;
            }
            let target = if CAPTURE != usize::MAX {
                Some(CAPTURE)
            } else {
                top_at(x, y)
            };
            if let Some(app) = target {
                if app_mouse(app, Mouse::Move, x, y, &frame_of(&WINS[app].rect)) {
                    *repaint = true;
                }
                return true;
            }
            false
        }
    }
}

pub unsafe fn on_button_up(x: i32, y: i32, repaint: &mut bool) -> bool {
    if DRAG != Drag::None {
        if DRAG == Drag::Move {
            if let Some(target) = SNAP_PREVIEW.take() {
                let w = &mut WINS[DRAG_APP];
                w.restore = DRAG_RECT;
                if target.left == WORK.left && target.right == WORK.right {
                    w.maximized = true;
                }
                w.rect = target;
                w.anim = Anim::Open;
                w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
            }
        }
        DRAG = Drag::None;
        *repaint = true;
        return true;
    }
    if CAPTURE != usize::MAX {
        let app = CAPTURE;
        CAPTURE = usize::MAX;
        if WINS[app].open && app_mouse(app, Mouse::Up, x, y, &frame_of(&WINS[app].rect)) {
            *repaint = true;
        }
        return true;
    }
    false
}

/// Molette, clic droit ou milieu : vers la fenêtre sous le pointeur.
pub unsafe fn on_other(ev: Mouse, x: i32, y: i32, repaint: &mut bool) -> bool {
    let Some(app) = top_at(x, y) else {
        return false;
    };
    if !matches!(ev, Mouse::Wheel(_)) && ACTIVE != app {
        raise(app);
        *repaint = true;
    }
    if app_mouse(app, ev, x, y, &frame_of(&WINS[app].rect)) {
        *repaint = true;
    }
    true
}

pub unsafe fn on_key(vk: u32, ch: u16) -> bool {
    if ACTIVE == usize::MAX {
        return false;
    }
    app_key(ACTIVE, vk, ch)
}

pub unsafe fn tick() -> bool {
    let mut repaint = false;
    for app in 0..APP_COUNT {
        if WINS[app].open && !WINS[app].minimized {
            repaint |= app_tick(app, &frame_of(&WINS[app].rect));
        }
    }
    repaint
}
