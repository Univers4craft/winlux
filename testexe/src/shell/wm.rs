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
use crate::notepad;
use crate::utilities;
use winlux_testexe::*;

pub const APP_EXPLORER: usize = 0;
pub const APP_SETTINGS: usize = 1;
pub const APP_NOTEPAD: usize = 2;
pub const APP_TASKMGR: usize = 3;
pub const APP_UTILITIES: usize = 4;
pub const APP_CLEANUP: usize = 5;
pub const APP_COUNT: usize = 6;

// Native session: each built-in application owns a separate X11/Win32 window.
// The Linux window manager handles its frame, stacking and taskbar lifecycle.
static mut NATIVE_APP: usize = usize::MAX;
pub unsafe fn native_session() -> bool {
    GetEnvironmentVariableA(c"WINLUX_NATIVE_SHELL".as_ptr(), core::ptr::null_mut(), 0) > 0
}
pub unsafe fn native_app() -> Option<usize> {
    (NATIVE_APP != usize::MAX).then_some(NATIVE_APP)
}
pub unsafe fn native_init() {
    let mut mode = [0i8; 32];
    let n = GetEnvironmentVariableA(c"WINLUX_SHELL_APP".as_ptr(), mode.as_mut_ptr(), 32);
    NATIVE_APP = if n == 8 && mode[0] == b'e' as i8 {
        APP_EXPLORER
    } else if n == 8 && mode[0] == b's' as i8 {
        APP_SETTINGS
    } else {
        usize::MAX
    };
    if NATIVE_APP == APP_EXPLORER {
        let mut path = [0u8; 1024];
        let n = GetEnvironmentVariableA(
            c"WINLUX_SHELL_PATH".as_ptr(),
            path.as_mut_ptr().cast(),
            path.len() as u32,
        );
        if n > 0 && (n as usize) < path.len() {
            crate::explorer::open_at(&path[..n as usize]);
        } else {
            crate::explorer::preload();
            open(APP_EXPLORER);
        }
    } else if NATIVE_APP == APP_SETTINGS {
        let mut page = [0i8; 8];
        let n = GetEnvironmentVariableA(c"WINLUX_SETTINGS_PAGE".as_ptr(), page.as_mut_ptr(), 8);
        let mut value = 0usize;
        for &digit in page.iter().take((n as usize).min(7)) {
            if (b'0' as i8..=b'9' as i8).contains(&digit) {
                value = value * 10 + (digit as usize - b'0' as usize);
            }
        }
        crate::settings::open_page(value);
    }
}
pub unsafe fn native_launch(app: usize) {
    let args = match app {
        APP_EXPLORER => c"explorer",
        APP_SETTINGS => c"settings",
        _ => c"editor",
    };
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-shell-app".as_ptr(),
        args.as_ptr(),
        core::ptr::null(),
        1,
    );
}
pub unsafe fn native_settings(page: usize) -> bool {
    if !native_session() || NATIVE_APP == APP_SETTINGS {
        return false;
    }
    let value = [
        b'0' + (page.min(99) / 10) as u8,
        b'0' + (page.min(99) % 10) as u8,
        0,
    ];
    SetEnvironmentVariableA(c"WINLUX_SETTINGS_PAGE".as_ptr(), value.as_ptr().cast());
    native_launch(APP_SETTINGS);
    SetEnvironmentVariableA(c"WINLUX_SETTINGS_PAGE".as_ptr(), core::ptr::null());
    true
}
pub unsafe fn native_explorer(path: &[u8]) -> bool {
    if !native_session() || NATIVE_APP == APP_EXPLORER {
        return false;
    }
    let mut value = [0u8; 1024];
    let n = path.len().min(value.len() - 1);
    value[..n].copy_from_slice(&path[..n]);
    SetEnvironmentVariableA(c"WINLUX_SHELL_PATH".as_ptr(), value.as_ptr().cast());
    native_launch(APP_EXPLORER);
    SetEnvironmentVariableA(c"WINLUX_SHELL_PATH".as_ptr(), core::ptr::null());
    true
}
unsafe fn native_frame(hwnd: Handle) -> Frame {
    let mut outer = rect(0, 0, 900, 620);
    GetClientRect(hwnd, &mut outer);
    let title_h = if NATIVE_APP == APP_EXPLORER { 40 } else { 0 };
    Frame {
        outer,
        title: rect(0, 0, outer.right, title_h),
        client: rect(0, title_h, outer.right, outer.bottom),
    }
}
pub unsafe fn native_draw(hwnd: Handle, hdc: Handle) {
    let f = native_frame(hwnd);
    let fonts = Fonts::new();
    let mut brushes = Brushes::new();
    SetBkMode(hdc, 1);
    fill(
        hdc,
        &mut brushes,
        0,
        0,
        f.outer.right,
        f.outer.bottom,
        pal().window,
    );
    app_draw(
        NATIVE_APP,
        &mut Gfx {
            hdc,
            b: &mut brushes,
            f: &fonts,
        },
        &f,
    );
    brushes.release();
    fonts.release();
}
pub unsafe fn native_mouse(hwnd: Handle, mut ev: Mouse, x: i32, y: i32) {
    MOUSE_X = x;
    MOUSE_Y = y;
    if ev == Mouse::Down {
        if now().wrapping_sub(LAST_CLICK_TIME) < 450
            && (x - LAST_CLICK_X).abs() < 5
            && (y - LAST_CLICK_Y).abs() < 5
        {
            ev = Mouse::DoubleClick;
            LAST_CLICK_TIME = 0;
        } else {
            LAST_CLICK_TIME = now();
            LAST_CLICK_X = x;
            LAST_CLICK_Y = y;
        }
    }
    if app_mouse(NATIVE_APP, ev, x, y, &native_frame(hwnd)) {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}
pub unsafe fn native_key(hwnd: Handle, vk: u32, ch: u16) {
    if app_key(NATIVE_APP, vk, ch) {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}
pub unsafe fn native_tick(hwnd: Handle) {
    let changed = crate::settings::system_tick();
    if app_tick(NATIVE_APP, &native_frame(hwnd)) || changed {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

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
static mut Z: [usize; APP_COUNT] = [0, 1, 2, 3, 4, 5];
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
static mut ALT_TAB: bool = false;
static mut ALT_TAB_INDEX: i32 = 0;
pub static mut SHOW_RUN: bool = false;
pub static mut RUN_TEXT: [u8; 256] = [0; 256];
pub static mut RUN_LEN: usize = 0;
/// Zone de travail (écran moins la barre des tâches) et bouton de barre des
/// tâches de chaque application, cible de l'animation de réduction.
pub static mut WORK: Rect = rect(0, 0, 1280, 672);
pub static mut TASKBAR_SLOT: [Rect; APP_COUNT] = [rect(0, 0, 0, 0); APP_COUNT];
pub static mut MOUSE_X: i32 = -1;
pub static mut MOUSE_Y: i32 = -1;

// ---------- Snap layouts ----------
/// Overlay de snap layout visible quand on survole le bouton agrandir.
static mut SNAP_LAYOUT_VISIBLE: bool = false;
static mut SNAP_LAYOUT_APP: usize = usize::MAX;
/// Disposition survolée (0..2 = layout, -1 aucun) et zone survolée dans le layout.
static mut SNAP_LAYOUT_HOVER: i32 = -1;
static mut SNAP_LAYOUT_ZONE: i32 = -1;

const SNAP_OVERLAY_W: i32 = 198;
const SNAP_OVERLAY_H: i32 = 164;
const SNAP_THUMB_W: i32 = 54;
const SNAP_THUMB_H: i32 = 38;
const SNAP_THUMB_GAP: i32 = 6;

fn snap_overlay_rect(app_rect: &Rect) -> Rect {
    let cx = app_rect.right - 2 * CAPTION_W - CAPTION_W / 2;
    let top = app_rect.top + TITLE_H + 2;
    rect(cx - SNAP_OVERLAY_W / 2, top, cx + SNAP_OVERLAY_W / 2, top + SNAP_OVERLAY_H)
}

fn snap_layout_thumb(overlay: &Rect, layout: i32) -> Rect {
    let x = overlay.left + 12 + layout * (SNAP_THUMB_W + SNAP_THUMB_GAP + 4);
    let y = overlay.top + 12;
    rect(x, y, x + SNAP_THUMB_W, y + SNAP_THUMB_H)
}

fn snap_zone_rects(layout: i32) -> ([Rect; 4], usize) {
    let wk = unsafe { WORK };
    let mid_x = (wk.left + wk.right) / 2;
    let mid_y = (wk.top + wk.bottom) / 2;
    match layout {
        0 => {
            let zones = [
                rect(wk.left, wk.top, mid_x, wk.bottom),
                rect(mid_x, wk.top, wk.right, wk.bottom),
                rect(0, 0, 0, 0),
                rect(0, 0, 0, 0),
            ];
            (zones, 2)
        }
        1 => {
            let zones = [
                rect(wk.left, wk.top, mid_x, wk.bottom),
                rect(mid_x, wk.top, wk.right, mid_y),
                rect(mid_x, mid_y, wk.right, wk.bottom),
                rect(0, 0, 0, 0),
            ];
            (zones, 3)
        }
        2 => {
            let zones = [
                rect(wk.left, wk.top, mid_x, mid_y),
                rect(mid_x, wk.top, wk.right, mid_y),
                rect(wk.left, mid_y, mid_x, wk.bottom),
                rect(mid_x, mid_y, wk.right, wk.bottom),
            ];
            (zones, 4)
        }
        _ => ([rect(0, 0, 0, 0); 4], 0),
    }
}

fn snap_thumb_zone_rect(thumb: &Rect, layout: i32, zone: i32) -> Rect {
    let mx = (thumb.left + thumb.right) / 2;
    let my = (thumb.top + thumb.bottom) / 2;
    match layout {
        0 => match zone {
            0 => rect(thumb.left + 2, thumb.top + 2, mx - 1, thumb.bottom - 2),
            _ => rect(mx + 1, thumb.top + 2, thumb.right - 2, thumb.bottom - 2),
        },
        1 => match zone {
            0 => rect(thumb.left + 2, thumb.top + 2, mx - 1, thumb.bottom - 2),
            1 => rect(mx + 1, thumb.top + 2, thumb.right - 2, my - 1),
            _ => rect(mx + 1, my + 1, thumb.right - 2, thumb.bottom - 2),
        },
        2 => match zone {
            0 => rect(thumb.left + 2, thumb.top + 2, mx - 1, my - 1),
            1 => rect(mx + 1, thumb.top + 2, thumb.right - 2, my - 1),
            2 => rect(thumb.left + 2, my + 1, mx - 1, thumb.bottom - 2),
            _ => rect(mx + 1, my + 1, thumb.right - 2, thumb.bottom - 2),
        },
        _ => rect(0, 0, 0, 0),
    }
}

pub unsafe fn close_snap_layout() {
    SNAP_LAYOUT_VISIBLE = false;
    SNAP_LAYOUT_APP = usize::MAX;
    SNAP_LAYOUT_HOVER = -1;
    SNAP_LAYOUT_ZONE = -1;
}

pub fn snap_layout_active() -> bool {
    unsafe { SNAP_LAYOUT_VISIBLE }
}

// ---------- Bureaux virtuels ----------
pub const MAX_DESKTOPS: usize = 8;
static mut DESKTOP_COUNT: usize = 1;
static mut CURRENT_DESKTOP: usize = 0;
static mut WIN_DESKTOP: [usize; APP_COUNT] = [0; APP_COUNT];
pub static mut TASK_VIEW: bool = false;
static mut TASK_VIEW_HOVER: i32 = -1;
static mut TASK_VIEW_DESK_HOVER: i32 = -1;

pub fn desktop_count() -> usize {
    unsafe { DESKTOP_COUNT }
}
pub fn current_desktop() -> usize {
    unsafe { CURRENT_DESKTOP }
}

pub unsafe fn switch_desktop(idx: usize) {
    if idx >= DESKTOP_COUNT { return; }
    CURRENT_DESKTOP = idx;
    refresh_active();
}

pub unsafe fn add_desktop() {
    if DESKTOP_COUNT < MAX_DESKTOPS {
        DESKTOP_COUNT += 1;
    }
}

pub unsafe fn remove_desktop(idx: usize) {
    if DESKTOP_COUNT <= 1 || idx >= DESKTOP_COUNT { return; }
    for app in 0..APP_COUNT {
        if WIN_DESKTOP[app] == idx {
            WIN_DESKTOP[app] = if idx > 0 { idx - 1 } else { 0 };
        } else if WIN_DESKTOP[app] > idx {
            WIN_DESKTOP[app] -= 1;
        }
    }
    DESKTOP_COUNT -= 1;
    if CURRENT_DESKTOP >= DESKTOP_COUNT {
        CURRENT_DESKTOP = DESKTOP_COUNT - 1;
    }
    refresh_active();
}

pub unsafe fn move_to_desktop(app: usize, desk: usize) {
    if desk < DESKTOP_COUNT {
        WIN_DESKTOP[app] = desk;
    }
}

fn on_current_desktop(app: usize) -> bool {
    unsafe { WIN_DESKTOP[app] == CURRENT_DESKTOP }
}

pub fn title_of(app: usize) -> &'static [u8] {
    match app {
        APP_EXPLORER => b"Explorer",
        APP_SETTINGS => b"Param\xE8tres",
        APP_NOTEPAD => b"Bloc-notes",
        APP_TASKMGR => b"Gestionnaire des t\xE2ches",
        APP_UTILITIES => b"Utilitaires",
        APP_CLEANUP => b"Nettoyage de disque",
        _ => b"Param\xE8tres",
    }
}
fn glyph_of(app: usize) -> Glyph {
    match app {
        APP_EXPLORER => Glyph::Folder,
        APP_SETTINGS => Glyph::Settings,
        APP_NOTEPAD => Glyph::File,
        APP_TASKMGR => Glyph::Computer,
        APP_UTILITIES => Glyph::Tools,
        APP_CLEANUP => Glyph::Drive,
        _ => Glyph::Settings,
    }
}
fn custom_title(app: usize) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::CUSTOM_TITLE,
        APP_SETTINGS => crate::settings::CUSTOM_TITLE,
        APP_NOTEPAD => notepad::CUSTOM_TITLE,
        APP_TASKMGR => crate::taskmgr::CUSTOM_TITLE,
        APP_UTILITIES => utilities::CUSTOM_TITLE,
        APP_CLEANUP => crate::diskcleaner::CUSTOM_TITLE,
        _ => crate::settings::CUSTOM_TITLE,
    }
}

unsafe fn app_draw(app: usize, g: &mut Gfx, f: &Frame) {
    match app {
        APP_EXPLORER => crate::explorer::draw(g, f),
        APP_SETTINGS => crate::settings::draw(g, f),
        APP_NOTEPAD => notepad::draw(g, f),
        APP_TASKMGR => crate::taskmgr::draw(g, f),
        APP_UTILITIES => utilities::draw(g, f),
        APP_CLEANUP => crate::diskcleaner::draw(g, f),
        _ => crate::settings::draw(g, f),
    }
}
unsafe fn app_mouse(app: usize, ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::mouse(ev, x, y, f),
        APP_SETTINGS => crate::settings::mouse(ev, x, y, f),
        APP_NOTEPAD => notepad::mouse(ev, x, y, f),
        APP_TASKMGR => crate::taskmgr::mouse(ev, x, y, f),
        APP_UTILITIES => utilities::mouse(ev, x, y, f),
        APP_CLEANUP => crate::diskcleaner::mouse(ev, x, y, f),
        _ => crate::settings::mouse(ev, x, y, f),
    }
}
unsafe fn app_key(app: usize, vk: u32, ch: u16) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::key(vk, ch),
        APP_SETTINGS => crate::settings::key(vk, ch),
        APP_NOTEPAD => notepad::key(vk, ch),
        APP_TASKMGR => crate::taskmgr::key(vk, ch),
        APP_UTILITIES => utilities::key(vk, ch),
        APP_CLEANUP => crate::diskcleaner::key(vk, ch),
        _ => crate::settings::key(vk, ch),
    }
}
unsafe fn app_tick(app: usize, f: &Frame) -> bool {
    match app {
        APP_EXPLORER => crate::explorer::tick(f),
        APP_SETTINGS => crate::settings::tick(f),
        APP_NOTEPAD => notepad::tick(f),
        APP_TASKMGR => crate::taskmgr::tick(f),
        APP_UTILITIES => utilities::tick(f),
        APP_CLEANUP => crate::diskcleaner::tick(f),
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
    unsafe { WINS[app].open && !WINS[app].minimized && on_current_desktop(app) }
}
pub fn is_active(app: usize) -> bool {
    unsafe { ACTIVE == app }
}
pub fn is_minimized(app: usize) -> bool {
    unsafe { WINS[app].minimized }
}
pub fn is_maximized(app: usize) -> bool {
    unsafe { WINS[app].maximized }
}
pub unsafe fn WINS_REF(app: usize) -> bool {
    WINS[app].minimized
}

/// Vrai tant qu'une animation est en cours : la boucle principale repeint
/// alors à ~60 images par seconde.
pub fn animating() -> bool {
    unsafe {
        if TASK_VIEW || SNAP_LAYOUT_VISIBLE {
            return true;
        }
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
        if WINS[a].open && !WINS[a].minimized && on_current_desktop(a) {
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
        APP_NOTEPAD => (900, 620),
        APP_TASKMGR => (1000, 650),
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
    if native_session() && NATIVE_APP != app {
        native_launch(app);
        return;
    }
    let w = &mut WINS[app];
    if !w.open {
        WIN_DESKTOP[app] = CURRENT_DESKTOP;
        *w = Win {
            open: true,
            minimized: false,
            maximized: false,
            rect: default_rect(app),
            restore: default_rect(app),
            anim: Anim::Open,
            anim_start: now(),
        };
    } else if !on_current_desktop(app) {
        WIN_DESKTOP[app] = CURRENT_DESKTOP;
        if w.minimized {
            w.minimized = false;
            w.anim = Anim::Restore;
            w.anim_start = now();
        }
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

unsafe fn draw_snap_layout(g: &mut Gfx) {
    let p = pal();
    let r = snap_overlay_rect(&WINS[SNAP_LAYOUT_APP].rect);
    panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface, p.border);
    text(g.hdc, g.f.small, p.text, r.left + 14, r.top + 56, b"Dispositions d'ancrage");
    for layout in 0..3i32 {
        let t = snap_layout_thumb(&r, layout);
        let selected = SNAP_LAYOUT_HOVER == layout;
        let bg = if selected { mix(p.hover, p.accent, 3, 10) } else { p.surface2 };
        round_rect(g.hdc, g.b, t.left, t.top, t.right, t.bottom, 4, bg);
        let (_zones, _count) = snap_zone_rects(layout);
        let zone_count = match layout { 0 => 2, 1 => 3, _ => 4 };
        for z in 0..zone_count {
            let zr = snap_thumb_zone_rect(&t, layout, z);
            let highlighted = selected && SNAP_LAYOUT_ZONE == z;
            let c = if highlighted { p.accent } else { mix(p.border, p.accent, 2, 10) };
            round_rect(g.hdc, g.b, zr.left, zr.top, zr.right, zr.bottom, 2, c);
        }
    }
    // Preview of selected zone
    if SNAP_LAYOUT_HOVER >= 0 && SNAP_LAYOUT_ZONE >= 0 {
        let (zones, count) = snap_zone_rects(SNAP_LAYOUT_HOVER);
        if (SNAP_LAYOUT_ZONE as usize) < count {
            let zr = zones[SNAP_LAYOUT_ZONE as usize];
            round_rect(
                g.hdc, g.b,
                zr.left + 4, zr.top + 4, zr.right - 4, zr.bottom - 4,
                8, mix(p.accent, p.mica, 5, 10),
            );
            round_rect(
                g.hdc, g.b,
                zr.left + 6, zr.top + 6, zr.right - 6, zr.bottom - 6,
                7, mix(p.accent, p.mica, 7, 10),
            );
        }
    }
}

unsafe fn draw_task_view(g: &mut Gfx) {
    let p = pal();
    let wk = WORK;
    // Fond semi-transparent
    fill(g.hdc, g.b, wk.left, wk.top, wk.right, wk.bottom, mix(0, p.mica, 8, 10));
    // Titre
    text(g.hdc, g.f.large, p.text, wk.left + 40, wk.top + 20, b"Vue des t\xE2ches");
    // Miniatures des fenêtres du bureau courant
    let mut visible = [usize::MAX; APP_COUNT];
    let mut vis_count = 0;
    for &a in Z.iter().rev() {
        if WINS[a].open && on_current_desktop(a) {
            visible[vis_count] = a;
            vis_count += 1;
        }
    }
    let area_y = wk.top + 60;
    let area_h = (wk.bottom - wk.top) / 2 - 40;
    let thumb_w = 200.min((wk.right - wk.left - 60) / vis_count.max(1) as i32 - 12);
    let thumb_h = area_h.min(140);
    let total_w = vis_count as i32 * (thumb_w + 12) - 12;
    let start_x = (wk.left + wk.right - total_w) / 2;
    for i in 0..vis_count {
        let app = visible[i];
        let x = start_x + i as i32 * (thumb_w + 12);
        let y = area_y + (area_h - thumb_h) / 2;
        let hover = TASK_VIEW_HOVER == i as i32;
        let bg = if hover { mix(p.surface2, p.accent, 3, 10) } else { p.surface };
        round_rect(g.hdc, g.b, x, y, x + thumb_w, y + thumb_h, 8, p.border);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + thumb_w - 1, y + thumb_h - 1, 7, bg);
        let cx = x + thumb_w / 2;
        glyph(g.hdc, g.b, g.f, glyph_of(app), cx - 16, y + thumb_h / 2 - 24, 32);
        text_centered(
            g.hdc, g.f.small, 11, p.text,
            cx, y + thumb_h / 2 + 16, title_of(app),
        );
        if hover {
            round_rect(g.hdc, g.b, x, y, x + thumb_w, y + 3, 2, p.accent);
        }
    }
    if vis_count == 0 {
        text_centered(
            g.hdc, g.f.body, 13, p.text3,
            (wk.left + wk.right) / 2, area_y + area_h / 2,
            b"Aucune fen\xEAtre sur ce bureau",
        );
    }
    // Barre des bureaux en bas
    let desk_bar_h = 100;
    let desk_bar_y = wk.bottom - desk_bar_h - 20;
    let desk_w = 120;
    let desk_h = 70;
    let total_desk = DESKTOP_COUNT as i32 * (desk_w + 12) + desk_w / 2;
    let desk_start = (wk.left + wk.right - total_desk) / 2;
    text_centered(
        g.hdc, g.f.caption, 12, p.text,
        (wk.left + wk.right) / 2, desk_bar_y - 20, b"Bureaux",
    );
    for d in 0..DESKTOP_COUNT {
        let x = desk_start + d as i32 * (desk_w + 12);
        let y = desk_bar_y;
        let selected = d == CURRENT_DESKTOP;
        let hover = TASK_VIEW_DESK_HOVER == d as i32;
        let bg = if selected { mix(p.surface, p.accent, 4, 10) } else if hover { p.surface2 } else { p.surface };
        let border_c = if selected { p.accent } else { p.border };
        round_rect(g.hdc, g.b, x, y, x + desk_w, y + desk_h, 6, border_c);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + desk_w - 1, y + desk_h - 1, 5, bg);
        let mut label = [b'B', b'u', b'r', b'e', b'a', b'u', b' ', b'0' + (d + 1) as u8 % 10];
        if d + 1 >= 10 { label[7] = b'?'; }
        text_centered(
            g.hdc, g.f.small, 11,
            if selected { p.accent } else { p.text },
            x + desk_w / 2, y + desk_h / 2 - 6, &label,
        );
        // Nombre de fenêtres sur ce bureau
        let mut wcount = 0u8;
        for app in 0..APP_COUNT {
            if WINS[app].open && WIN_DESKTOP[app] == d { wcount += 1; }
        }
        if wcount > 0 {
            let dot_label = [b'0' + wcount % 10];
            text_centered(
                g.hdc, g.f.caption, 12, p.text3,
                x + desk_w / 2, y + desk_h / 2 + 10, &dot_label,
            );
        }
    }
    // Bouton « + » pour ajouter un bureau
    if DESKTOP_COUNT < MAX_DESKTOPS {
        let x = desk_start + DESKTOP_COUNT as i32 * (desk_w + 12);
        let y = desk_bar_y;
        let hover = TASK_VIEW_DESK_HOVER == DESKTOP_COUNT as i32;
        let bg = if hover { p.surface2 } else { p.surface };
        round_rect(g.hdc, g.b, x, y, x + desk_w / 2, y + desk_h, 6, p.border);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + desk_w / 2 - 1, y + desk_h - 1, 5, bg);
        text_centered(
            g.hdc, g.f.large, 22, p.text,
            x + desk_w / 4, y + desk_h / 2 - 11, b"+",
        );
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
        if WINS[app].open && on_current_desktop(app) {
            draw_window(g, app);
        }
    }
    // Snap layout overlay
    if SNAP_LAYOUT_VISIBLE && SNAP_LAYOUT_APP != usize::MAX && WINS[SNAP_LAYOUT_APP].open {
        draw_snap_layout(g);
    }
    // Task view (multi-bureau)
    if TASK_VIEW {
        draw_task_view(g);
    }
    if ALT_TAB {
        let p = pal();
        let sw = 420;
        let sh = 140;
        let cx = (WORK.left + WORK.right - sw) / 2;
        let cy = (WORK.top + WORK.bottom - sh) / 2;
        round_rect(
            g.hdc,
            g.b,
            cx,
            cy,
            cx + sw,
            cy + sh,
            12,
            mix(p.border, p.accent, 3, 10),
        );
        round_rect(
            g.hdc,
            g.b,
            cx + 1,
            cy + 1,
            cx + sw - 1,
            cy + sh - 1,
            11,
            p.surface,
        );
        text(
            g.hdc,
            g.f.caption,
            p.text,
            cx + 16,
            cy + 12,
            b"Basculer entre les fen\xEAtres",
        );
        let mut list = [usize::MAX; APP_COUNT];
        let mut count = 0;
        for &a in Z.iter().rev() {
            if WINS[a].open && !WINS[a].minimized {
                list[count] = a;
                count += 1;
            }
        }
        let mut i = 0;
        while i < count {
            let app = list[i];
            let x = cx + 16 + i as i32 * 92;
            let y = cy + 48;
            if (i as i32) == ALT_TAB_INDEX {
                round_rect(
                    g.hdc,
                    g.b,
                    x - 4,
                    y - 4,
                    x + 84,
                    y + 76,
                    8,
                    mix(p.hover, p.accent, 2, 10),
                );
            }
            glyph(g.hdc, g.b, g.f, glyph_of(app), x + 26, y + 8, 32);
            let t = title_of(app);
            // simple text clipping by drawing truncated
            text(
                g.hdc,
                g.f.small,
                p.text,
                x - 2,
                y + 52,
                if t.len() > 10 { b"..." } else { t },
            );
            i += 1;
        }
    }
    if crate::wm::SHOW_RUN {
        let p = pal();
        let sw = 420;
        let sh = 110;
        let cx = (WORK.left + WORK.right - sw) / 2;
        let cy = WORK.top + 60;
        round_rect(
            g.hdc,
            g.b,
            cx,
            cy,
            cx + sw,
            cy + sh,
            8,
            mix(p.border, p.accent, 2, 10),
        );
        round_rect(
            g.hdc,
            g.b,
            cx + 1,
            cy + 1,
            cx + sw - 1,
            cy + sh - 1,
            7,
            p.surface,
        );
        text(g.hdc, g.f.caption, p.text, cx + 16, cy + 12, b"Ex\xE9cuter");
        text(g.hdc, g.f.small, p.text3, cx + 16, cy + 42, b"Ouvrir :");
        fill(
            g.hdc,
            g.b,
            cx + 16,
            cy + 64,
            cx + sw - 80,
            cy + 88,
            p.window,
        );
        fill(
            g.hdc,
            g.b,
            cx + 16,
            cy + 64,
            cx + sw - 80,
            cy + 65,
            p.border,
        );
        fill(
            g.hdc,
            g.b,
            cx + 16,
            cy + 87,
            cx + sw - 80,
            cy + 88,
            p.border,
        );
        fill(g.hdc, g.b, cx + 16, cy + 65, cx + 17, cy + 87, p.border);
        fill(
            g.hdc,
            g.b,
            cx + sw - 81,
            cy + 65,
            cx + sw - 80,
            cy + 87,
            p.border,
        );
        if crate::wm::RUN_LEN > 0 {
            let mut buf = [0u8; 260];
            let mut j = 0;
            while j < crate::wm::RUN_LEN && j < 255 {
                buf[j] = crate::wm::RUN_TEXT[j];
                j += 1;
            }
            buf[j] = 0;
            text(g.hdc, g.f.body, p.text, cx + 22, cy + 70, &buf);
        }
        round_rect(
            g.hdc,
            g.b,
            cx + sw - 70,
            cy + 64,
            cx + sw - 16,
            cy + 88,
            4,
            p.accent,
        );
        text(
            g.hdc,
            g.f.small,
            rgb(0xFF, 0xFF, 0xFF),
            cx + sw - 58,
            cy + 70,
            b"OK",
        );
    }
}

// ---------- Souris et clavier ----------

unsafe fn top_at(x: i32, y: i32) -> Option<usize> {
    for &app in Z.iter().rev() {
        let w = &WINS[app];
        if w.open && !w.minimized && w.anim != Anim::Close && on_current_desktop(app) {
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
    // Snap layout click
    if SNAP_LAYOUT_VISIBLE && SNAP_LAYOUT_APP != usize::MAX {
        let overlay = snap_overlay_rect(&WINS[SNAP_LAYOUT_APP].rect);
        if inside(&overlay, x, y) {
            if SNAP_LAYOUT_HOVER >= 0 && SNAP_LAYOUT_ZONE >= 0 {
                let (zones, count) = snap_zone_rects(SNAP_LAYOUT_HOVER);
                if (SNAP_LAYOUT_ZONE as usize) < count {
                    let app = SNAP_LAYOUT_APP;
                    let target = zones[SNAP_LAYOUT_ZONE as usize];
                    let w = &mut WINS[app];
                    w.restore = w.rect;
                    w.maximized = target.left == WORK.left && target.right == WORK.right
                        && target.top == WORK.top && target.bottom == WORK.bottom;
                    w.rect = target;
                    w.anim = Anim::Open;
                    w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
                }
            }
            close_snap_layout();
            *repaint = true;
            return true;
        }
        close_snap_layout();
        *repaint = true;
    }
    // Task view click
    if TASK_VIEW {
        let wk = WORK;
        // Check window thumbnails
        let mut vis = [usize::MAX; APP_COUNT];
        let mut vc = 0;
        for &a in Z.iter().rev() {
            if WINS[a].open && on_current_desktop(a) {
                vis[vc] = a;
                vc += 1;
            }
        }
        let area_y = wk.top + 60;
        let area_h = (wk.bottom - wk.top) / 2 - 40;
        let tw = 200.min((wk.right - wk.left - 60) / vc.max(1) as i32 - 12);
        let th = area_h.min(140);
        let total = vc as i32 * (tw + 12) - 12;
        let sx = (wk.left + wk.right - total) / 2;
        for i in 0..vc {
            let bx = sx + i as i32 * (tw + 12);
            let by = area_y + (area_h - th) / 2;
            if x >= bx && x < bx + tw && y >= by && y < by + th {
                let app = vis[i];
                raise(app);
                TASK_VIEW = false;
                *repaint = true;
                return true;
            }
        }
        // Check desktop bar
        let desk_bar_y = wk.bottom - 120;
        let desk_w = 120i32;
        let desk_h = 70i32;
        let total_desk = DESKTOP_COUNT as i32 * (desk_w + 12) + desk_w / 2;
        let ds = (wk.left + wk.right - total_desk) / 2;
        for d in 0..=DESKTOP_COUNT {
            let dx = ds + d as i32 * (desk_w + 12);
            let dw = if d == DESKTOP_COUNT { desk_w / 2 } else { desk_w };
            if x >= dx && x < dx + dw && y >= desk_bar_y && y < desk_bar_y + desk_h {
                if d == DESKTOP_COUNT {
                    add_desktop();
                } else {
                    switch_desktop(d);
                }
                *repaint = true;
                return true;
            }
        }
        // Click elsewhere closes task view
        TASK_VIEW = false;
        TASK_VIEW_HOVER = -1;
        TASK_VIEW_DESK_HOVER = -1;
        *repaint = true;
        return true;
    }
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
            // Snap layout overlay interaction
            if SNAP_LAYOUT_VISIBLE && SNAP_LAYOUT_APP != usize::MAX && WINS[SNAP_LAYOUT_APP].open {
                let overlay = snap_overlay_rect(&WINS[SNAP_LAYOUT_APP].rect);
                if inside(&overlay, x, y) {
                    let mut found_layout = -1i32;
                    let mut found_zone = -1i32;
                    for layout in 0..3i32 {
                        let t = snap_layout_thumb(&overlay, layout);
                        if inside(&t, x, y) {
                            found_layout = layout;
                            let zone_count = match layout { 0 => 2, 1 => 3, _ => 4 };
                            for z in 0..zone_count {
                                let zr = snap_thumb_zone_rect(&t, layout, z);
                                if inside(&zr, x, y) {
                                    found_zone = z;
                                }
                            }
                        }
                    }
                    if found_layout != SNAP_LAYOUT_HOVER || found_zone != SNAP_LAYOUT_ZONE {
                        SNAP_LAYOUT_HOVER = found_layout;
                        SNAP_LAYOUT_ZONE = found_zone;
                        *repaint = true;
                    }
                    return true;
                } else {
                    // Mouse left overlay — check if still on maximize button
                    let cap = caption_rect(&WINS[SNAP_LAYOUT_APP].rect, 1);
                    if !inside(&cap, x, y) {
                        close_snap_layout();
                        *repaint = true;
                    }
                }
            }
            // Task view mouse
            if TASK_VIEW {
                let wk = WORK;
                let mut vis = [usize::MAX; APP_COUNT];
                let mut vc = 0;
                for &a in Z.iter().rev() {
                    if WINS[a].open && on_current_desktop(a) {
                        vis[vc] = a;
                        vc += 1;
                    }
                }
                let area_y = wk.top + 60;
                let area_h = (wk.bottom - wk.top) / 2 - 40;
                let tw = 200.min((wk.right - wk.left - 60) / vc.max(1) as i32 - 12);
                let th = area_h.min(140);
                let total = vc as i32 * (tw + 12) - 12;
                let sx = (wk.left + wk.right - total) / 2;
                let mut new_hover = -1i32;
                for i in 0..vc {
                    let bx = sx + i as i32 * (tw + 12);
                    let by = area_y + (area_h - th) / 2;
                    if x >= bx && x < bx + tw && y >= by && y < by + th {
                        new_hover = i as i32;
                    }
                }
                if new_hover != TASK_VIEW_HOVER { TASK_VIEW_HOVER = new_hover; *repaint = true; }
                // Desktop bar hover
                let desk_bar_y = wk.bottom - 120;
                let desk_w = 120i32;
                let desk_h = 70i32;
                let total_desk = DESKTOP_COUNT as i32 * (desk_w + 12) + desk_w / 2;
                let ds = (wk.left + wk.right - total_desk) / 2;
                let mut new_dh = -1i32;
                for d in 0..=DESKTOP_COUNT {
                    let dx = ds + d as i32 * (desk_w + 12);
                    let dw = if d == DESKTOP_COUNT { desk_w / 2 } else { desk_w };
                    if x >= dx && x < dx + dw && y >= desk_bar_y && y < desk_bar_y + desk_h {
                        new_dh = d as i32;
                    }
                }
                if new_dh != TASK_VIEW_DESK_HOVER { TASK_VIEW_DESK_HOVER = new_dh; *repaint = true; }
                return true;
            }
            let mut hover = -1;
            if let Some(app) = top_at(x, y) {
                if app == ACTIVE {
                    for i in 0..3 {
                        if inside(&caption_rect(&WINS[app].rect, i), x, y) {
                            hover = i;
                        }
                    }
                    // Show snap layout on maximize button hover
                    if hover == 1 && !SNAP_LAYOUT_VISIBLE {
                        SNAP_LAYOUT_VISIBLE = true;
                        SNAP_LAYOUT_APP = app;
                        SNAP_LAYOUT_HOVER = -1;
                        SNAP_LAYOUT_ZONE = -1;
                        *repaint = true;
                    } else if hover != 1 && SNAP_LAYOUT_VISIBLE && SNAP_LAYOUT_APP == app {
                        let overlay = snap_overlay_rect(&WINS[app].rect);
                        if !inside(&overlay, x, y) {
                            close_snap_layout();
                            *repaint = true;
                        }
                    }
                }
            } else if SNAP_LAYOUT_VISIBLE {
                close_snap_layout();
                *repaint = true;
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

/// Ancrage Win+flèches : gauche, droite, haut (maximiser), bas (restaurer).
pub unsafe fn snap_arrow(vk: u32) -> bool {
    if ACTIVE == usize::MAX { return false; }
    let app = ACTIVE;
    let wk = WORK;
    let mid_x = (wk.left + wk.right) / 2;
    let w = &mut WINS[app];
    match vk {
        0x25 => { // Left
            w.restore = w.rect;
            w.maximized = false;
            w.rect = rect(wk.left, wk.top, mid_x, wk.bottom);
            w.anim = Anim::Open;
            w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
            true
        }
        0x27 => { // Right
            w.restore = w.rect;
            w.maximized = false;
            w.rect = rect(mid_x, wk.top, wk.right, wk.bottom);
            w.anim = Anim::Open;
            w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
            true
        }
        0x26 => { // Up = maximize
            if !w.maximized {
                w.restore = w.rect;
                w.maximized = true;
                w.rect = wk;
                w.anim = Anim::Open;
                w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
            }
            true
        }
        0x28 => { // Down = restore or minimize
            if w.maximized {
                w.maximized = false;
                w.rect = w.restore;
                w.anim = Anim::Open;
                w.anim_start = now().wrapping_sub(OPEN_MS as u32 / 2);
            } else {
                minimize(app);
            }
            true
        }
        _ => false,
    }
}

pub unsafe fn on_key(vk: u32, ch: u16) -> bool {
    // Escape closes snap layout and task view
    if (vk == 0x1B || ch == 27) && SNAP_LAYOUT_VISIBLE {
        close_snap_layout();
        return true;
    }
    if (vk == 0x1B || ch == 27) && TASK_VIEW {
        TASK_VIEW = false;
        TASK_VIEW_HOVER = -1;
        TASK_VIEW_DESK_HOVER = -1;
        return true;
    }
    if vk == 0x12 {
        // Alt
        if ch == 0 {
            // track alt key via keydown/up context - simplified
        }
    }
    if vk == 0x09 && ch == 0 {
        // Tab
        let alt_down = ((GetKeyState(0x12) as u16) & 0x8000) != 0;
        if alt_down {
            let mut list = [usize::MAX; APP_COUNT];
            let mut count = 0;
            for &a in Z.iter().rev() {
                if WINS[a].open && !WINS[a].minimized {
                    list[count] = a;
                    count += 1;
                }
            }
            if count > 0 {
                ALT_TAB = true;
                ALT_TAB_INDEX = (ALT_TAB_INDEX + 1) % count as i32;
                return true;
            }
        } else if ALT_TAB {
            let mut list = [usize::MAX; APP_COUNT];
            let mut count = 0;
            for &a in Z.iter().rev() {
                if WINS[a].open && !WINS[a].minimized {
                    list[count] = a;
                    count += 1;
                }
            }
            if count > 0 && ALT_TAB_INDEX >= 0 && (ALT_TAB_INDEX as usize) < count {
                let app = list[ALT_TAB_INDEX as usize];
                raise(app);
            }
            ALT_TAB = false;
            ALT_TAB_INDEX = 0;
            return true;
        }
    }
    if vk == 0x12 && ch == 0 {
        // if alt released
        let alt_down = ((GetKeyState(0x12) as u16) & 0x8000) != 0;
        if !alt_down && ALT_TAB {
            let mut list = [usize::MAX; APP_COUNT];
            let mut count = 0;
            for &a in Z.iter().rev() {
                if WINS[a].open && !WINS[a].minimized {
                    list[count] = a;
                    count += 1;
                }
            }
            if count > 0 && ALT_TAB_INDEX >= 0 && (ALT_TAB_INDEX as usize) < count {
                let app = list[ALT_TAB_INDEX as usize];
                raise(app);
            }
            ALT_TAB = false;
            ALT_TAB_INDEX = 0;
            return true;
        }
    }
    if ALT_TAB {
        return true;
    }
    if crate::wm::SHOW_RUN {
        if vk == 0x1B || ch == 27 {
            crate::wm::SHOW_RUN = false;
            return true;
        }
        if ch == 8 {
            if crate::wm::RUN_LEN > 0 {
                crate::wm::RUN_LEN -= 1;
                crate::wm::RUN_TEXT[crate::wm::RUN_LEN] = 0;
            }
            return true;
        }
        if ch == 13 || vk == 0x0D {
            // try to run
            crate::wm::SHOW_RUN = false;
            let mut cmd = [0u16; 512];
            let mut i = 0;
            while i < crate::wm::RUN_LEN && i < 255 {
                cmd[i] = crate::wm::RUN_TEXT[i] as u16;
                i += 1;
            }
            cmd[i] = 0;
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                cmd.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
            return true;
        }
        if ch >= 0x20 && ch < 0x7F && crate::wm::RUN_LEN < 255 {
            crate::wm::RUN_TEXT[crate::wm::RUN_LEN] = ch as u8;
            crate::wm::RUN_LEN += 1;
            crate::wm::RUN_TEXT[crate::wm::RUN_LEN] = 0;
            return true;
        }
        return true;
    }
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
