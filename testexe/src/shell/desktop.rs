//! Bureau Winlux inspiré de Windows 11, dessiné entièrement par GDI invité :
//! écran de démarrage animé, fond « bloom », barre des tâches centrée ou
//! alignée à gauche, menu Démarrer animé, zone de notification, icônes du
//! bureau et fenêtres des applications intégrées (voir `wm.rs`).
//!
//! Toute la mise en page découle de la taille réelle de l'écran : quand la
//! résolution change (fenêtre de VM redimensionnée), le bureau se réadapte.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

#[path = "utilities.rs"]
mod utilities;
use crate::explorer;
use crate::gfx::*;
use crate::wm::{self, Gfx, Mouse};
use winlux_testexe::*;

pub const TASKBAR_HEIGHT: i32 = 48;
const BUTTON: i32 = 44;
const BUTTON_GAP: i32 = 4;
const START_OPEN_MS: i32 = 230;
const START_CLOSE_MS: i32 = 140;
const SPLASH_MIN_MS: u32 = 2600;
const INTRO_MS: i32 = 520;

pub static mut SCREEN_W: i32 = 1280;
pub static mut SCREEN_H: i32 = 720;
pub static mut START_OPEN: bool = false;
pub static mut ALIGN_LEFT: bool = false;
pub static mut VOLUME_LEVEL: u32 = 0;
pub static mut BATTERY_LEVEL: u32 = 0;
pub static mut BATTERY_PRESENT: u32 = 0;
pub static mut NETWORK_CONNECTED: u32 = 0;
static mut START_ANIM: u32 = 0;
static mut START_CLOSING: bool = false;
static mut HOVER_BUTTON: i32 = -1;
static mut HOVER_START_ITEM: i32 = -1;
static mut ICON_X: [i32; 5] = [12, 12, 12, 108, 108];
static mut ICON_Y: [i32; 5] = [12, 108, 204, 12, 108];
static mut PINNED_APPS: u8 = 15; // Explorer, Paramètres, Bloc-notes, Utilitaires
static mut DESKTOP_SHORTCUTS: u8 = 0;
static mut SHOW_SYSTEM_ICONS: bool = false; // Icônes système (PC, Documents, Corbeille) désactivées par défaut
static mut START_SHOW_ALL_APPS: bool = false; // Afficher toutes les applications au lieu des épinglées
static mut START_CATEGORY: i32 = 0; // Catégorie sélectionnée (0 = toutes)
static mut ALL_APPS: [(&'static [u8], &'static [u8], i32); 64] = [(b"", b"", 0); 64]; // (nom, exec, catégorie)
static mut ALL_APPS_COUNT: usize = 0;
static mut SHORTCUTS: [(&'static [u8], &'static [u8], i32, i32); 16] = [(b"", b"", 0, 0); 16]; // (nom, exec, x, y)
static mut SHORTCUTS_COUNT: usize = 0;
static mut START_SEARCH: [u8; 64] = [0; 64]; // Texte de recherche
static mut START_SEARCH_LEN: usize = 0;
use crate::gfx::{ENABLE_BLUR, ENABLE_SMOOTH_ANIM, ENABLE_THUMBNAILS, AUTO_THEME};
static mut CONTEXT_APP: Option<usize> = None;
static mut CONTEXT_DESKTOP: Option<usize> = None;
static mut CONTEXT_RECT: Rect = Rect {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
};
static mut CONTEXT_HOVER: i32 = -1;
static mut DRAGGED_ICON: i32 = -1;
static mut SELECTED_ICON: i32 = -1;
static mut ICON_CLICK_TIME: u32 = 0;
static mut LAST_MINUTE: u16 = 99;
/// Démarrage : début de l'écran de chargement, étape de préchargement en
/// cours et fin du fondu d'arrivée sur le bureau.
static mut BOOT_START: u32 = 0;
static mut BOOT_STEP: u32 = 0;
static mut BOOT_DONE: bool = false;
static mut INTRO_START: u32 = 0;
static mut LAST_FRAME: u32 = 0;
static mut LAST_INPUT: u32 = 0;
static mut SCREENSAVER: bool = false;
static mut SAVER_SEED: u32 = 123456789;
static mut SAVER_PIPES: [(i32, i32, i32, i32); 12] = [(0, 0, 0, 0); 12]; // x,y,dx,dy/color
static mut SAVER_INIT: bool = false;
/// Notification façon Windows 11, au-dessus de la zone de notification.
static mut TOAST: [u8; 120] = [0; 120];
static mut TOAST_LEN: usize = 0;
static mut TOAST_START: u32 = 0;
const TOAST_MS: i32 = 4500;
/// Panneau de notifications (centre d'action simplifié).
static mut NOTIF_PANEL: bool = false;
static mut NOTIF_HISTORY: [[u8; 120]; 8] = [[0; 120]; 8];
static mut NOTIF_LENS: [usize; 8] = [0; 8];
static mut NOTIF_COUNT: usize = 0;
/// Atténuation du fond (0 = noir, 1000 = normal) pendant le fondu.
static mut FADE: i32 = 1000;

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}

fn init_apps() {
    unsafe {
        if ALL_APPS_COUNT > 0 {
            return;
        }
        let apps: [(&'static [u8], &'static [u8], i32); 29] = [
            (b"Terminal", b"xterm", 1),
            (b"Explorateur de fichiers", b"nemo", 1),
            (b"Bloc-notes", b"mousepad", 1),
            (b"Calculatrice", b"calc.exe", 1),
            (b"Capture d'\xE9cran", b"screenshot.exe", 1),
            (b"Visionneuse", b"imageviewer.exe", 1),
            (b"Post-it", b"postit.exe", 1),
            (b"Lecteur PDF", b"pdfviewer.exe", 1),
            (b"Lecteur audio", b"audioplayer.exe", 5),
            (b"Infos syst\xE8me", b"sysinfo.exe", 8),
            (b"Utilitaires", b"utilities-terminal", 2),
            (b"Archives", b"file-roller", 1),
            (b"Visionneuse Linux", b"xviewer", 3),
            (b"GIMP", b"gimp", 3),
            (b"Inkscape", b"inkscape", 3),
            (b"Firefox", b"firefox", 4),
            (b"Chromium", b"chromium", 4),
            (b"Thunderbird", b"thunderbird", 4),
            (b"VLC", b"vlc", 5),
            (b"Rhythmbox", b"rhythmbox", 5),
            (b"Audacity", b"audacity", 5),
            (b"VS Code", b"code", 7),
            (b"Vim", b"vim", 7),
            (b"Geany", b"geany", 7),
            (b"Param\xE8tres", b"cinnamon-settings", 8),
            (b"Gestionnaire de paquets", b"mintinstall", 9),
            (b"Disques", b"gnome-disks", 9),
            (b"Moniteur syst\xE8me", b"gnome-system-monitor", 9),
            (b"Gestionnaire des t\xE2ches", b"taskmgr", 9),
        ];
        for (name, exec, cat) in apps.iter() {
            if ALL_APPS_COUNT < 64 {
                ALL_APPS[ALL_APPS_COUNT] = (*name, *exec, *cat);
                ALL_APPS_COUNT += 1;
            }
        }
    }
}

// ---------- Géométrie partagée par le dessin et les clics ----------

struct Layout {
    w: i32,
    h: i32,
    bar_top: i32,
    buttons_x: i32,
    start: Rect,
}

fn layout() -> Layout {
    let (w, h) = unsafe { (SCREEN_W.max(640), SCREEN_H.max(400)) };
    let bar_top = h - TASKBAR_HEIGHT;
    let count = taskbar_count();
    let buttons_w = count * BUTTON + (count - 1) * BUTTON_GAP;
    let buttons_x = if unsafe { ALIGN_LEFT } {
        12
    } else {
        (w - buttons_w) / 2
    };
    let sw = 640.min(w - 24);
    let sh = 600.min(bar_top - 24);
    let sx = if unsafe { ALIGN_LEFT } {
        12
    } else {
        (w - sw) / 2
    };
    let st = bar_top - 12 - sh;
    Layout {
        w,
        h,
        bar_top,
        buttons_x,
        start: Rect {
            left: sx,
            top: st,
            right: sx + sw,
            bottom: st + sh,
        },
    }
}

fn button_rect(l: &Layout, index: i32) -> Rect {
    let x = l.buttons_x + index * (BUTTON + BUTTON_GAP);
    Rect {
        left: x,
        top: l.bar_top + 2,
        right: x + BUTTON,
        bottom: l.bar_top + 2 + BUTTON,
    }
}

// Contenu du menu Démarrer : zones cliquables calculées depuis le panneau.
fn align_toggle_rect(s: &Rect) -> Rect {
    Rect {
        left: s.right - 230,
        top: s.bottom - 48,
        right: s.right - 70,
        bottom: s.bottom - 16,
    }
}
fn power_rect(s: &Rect) -> Rect {
    Rect {
        left: s.right - 62,
        top: s.bottom - 50,
        right: s.right - 22,
        bottom: s.bottom - 14,
    }
}
const PIN_COLS: i32 = 6;
const PIN_CELL_H: i32 = 84;
fn pin_rows(s: &Rect) -> i32 {
    if s.bottom - s.top > 560 {
        2
    } else {
        1
    }
}
fn pinned_cell(s: &Rect, i: i32) -> Rect {
    let (l, r) = (s.left + 28, s.right - 28);
    let cw = (r - l) / PIN_COLS;
    let x = l + (i % PIN_COLS) * cw;
    let y = s.top + 118 + (i / PIN_COLS) * PIN_CELL_H;
    Rect {
        left: x + 4,
        top: y,
        right: x + cw - 4,
        bottom: y + PIN_CELL_H - 6,
    }
}
fn recommended_cell(s: &Rect, i: i32) -> Rect {
    let (l, r) = (s.left + 28, s.right - 28);
    let col_w = (r - l) / 2;
    let y = s.top + 118 + pin_rows(s) * PIN_CELL_H + 54 + (i / 2) * 56;
    let x = l + (i % 2) * col_w;
    Rect {
        left: x,
        top: y,
        right: x + col_w - 8,
        bottom: y + 50,
    }
}
fn category_cell(s: &Rect, i: i32) -> Rect {
    let (l, r) = (s.left + 28, s.right - 28);
    let cat_h = 38;
    let cat_w = (r - l) / 3;
    let x = l + (i % 3) * cat_w;
    let y = s.top + 118 + (i / 3) * cat_h;
    Rect {
        left: x + 2,
        top: y,
        right: x + cat_w - 2,
        bottom: y + cat_h - 2,
    }
}

/// Position du menu Démarrer : il glisse depuis la barre des tâches.
unsafe fn start_offset() -> i32 {
    let t = since(START_ANIM);
    if START_CLOSING {
        if t >= START_CLOSE_MS {
            START_OPEN = false;
            START_CLOSING = false;
            return 0;
        }
        ease_out(t, START_CLOSE_MS) * 60 / 1000
    } else if t < START_OPEN_MS {
        (1000 - ease_out(t, START_OPEN_MS)) * 70 / 1000
    } else {
        0
    }
}

unsafe fn set_start(open: bool) {
    if open == (START_OPEN && !START_CLOSING) {
        return;
    }
    START_ANIM = now();
    if open {
        START_OPEN = true;
        START_CLOSING = false;
    } else if START_OPEN {
        START_CLOSING = true;
    }
}

// ---------- Fond d'écran ----------

fn fc(c: u32) -> u32 {
    unsafe {
        if FADE >= 1000 {
            c
        } else {
            mix(0, c, FADE, 1000)
        }
    }
}

static mut WP_DC: Handle = core::ptr::null_mut();
static mut WP_W: i32 = 0;
static mut WP_H: i32 = 0;
static mut WP_TRIED: bool = false;

unsafe fn load_wallpaper(hdc: Handle) {
    if WP_TRIED {
        return;
    }
    WP_TRIED = true;
    let path = c"Z:\\usr\\share\\winlux\\wallpaper.bmp";
    let mut selected = [0i8; 1024];
    let length = GetEnvironmentVariableA(
        c"WINLUX_WALLPAPER".as_ptr(),
        selected.as_mut_ptr(),
        selected.len() as u32,
    );
    let path = if length > 0 && length < selected.len() as u32 {
        selected.as_ptr()
    } else {
        path.as_ptr()
    };
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
        return;
    }
    let fsize = GetFileSize(f, core::ptr::null_mut());
    if fsize < 54 + 12 {
        CloseHandle(f);
        return;
    }
    let mut hdr = [0u8; 54];
    let mut read = 0u32;
    ReadFile(f, hdr.as_mut_ptr(), 54, &mut read, core::ptr::null_mut());
    if read != 54 || hdr[0] != b'B' || hdr[1] != b'M' {
        CloseHandle(f);
        return;
    }
    let off = u32::from_le_bytes([hdr[10], hdr[11], hdr[12], hdr[13]]);
    let w = i32::from_le_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]);
    let h = i32::from_le_bytes([hdr[22], hdr[23], hdr[24], hdr[25]]);
    let bpp = u16::from_le_bytes([hdr[28], hdr[29]]);
    if w <= 0 || h == 0 || (bpp != 24 && bpp != 32) {
        CloseHandle(f);
        return;
    }
    let abs_h = if h < 0 { -h } else { h };
    let stride = ((w as u32 * bpp as u32 / 8 + 3) & !3) as usize;
    let pixel_bytes = stride * abs_h as usize;
    let info = BitmapInfoHeader {
        size: 40,
        width: w,
        height: abs_h,
        planes: 1,
        bit_count: bpp,
        compression: 0,
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let mem_dc = CreateCompatibleDC(hdc);
    let dib = CreateDIBSection(mem_dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if dib.is_null() || bits.is_null() {
        DeleteDC(mem_dc);
        CloseHandle(f);
        return;
    }
    SelectObject(mem_dc, dib);
    SetFilePointer(f, off as i32, core::ptr::null_mut(), 0);
    let dst = bits as *mut u8;
    let mut total = 0usize;
    while total < pixel_bytes {
        let chunk = (pixel_bytes - total).min(65536) as u32;
        let mut got = 0u32;
        ReadFile(f, dst.add(total), chunk, &mut got, core::ptr::null_mut());
        if got == 0 {
            break;
        }
        total += got as usize;
    }
    CloseHandle(f);
    if total < pixel_bytes {
        DeleteObject(dib);
        DeleteDC(mem_dc);
        return;
    }
    WP_DC = mem_dc;
    WP_W = w;
    WP_H = abs_h;
}

unsafe fn wallpaper_bloom(hdc: Handle, b: &mut Brushes, l: &Layout) {
    let top = rgb(0x06, 0x12, 0x33);
    let bottom = rgb(0x0C, 0x2E, 0x6E);
    let bands = 48;
    for i in 0..bands {
        let y0 = l.h * i / bands;
        let y1 = l.h * (i + 1) / bands;
        fill(hdc, b, 0, y0, l.w, y1, fc(mix(top, bottom, i, bands - 1)));
    }
    let r = l.h.min(l.w * 3 / 4) * 9 / 25;
    let (cx, base) = (l.w / 2, l.h * 3 / 5 + r / 3);
    let petals: [(i32, i32, i32, u32, u32); 5] = [
        (
            -r * 6 / 5,
            r * 2 / 5,
            r * 6 / 5,
            rgb(0x0E, 0x34, 0x82),
            rgb(0x30, 0x6C, 0xD0),
        ),
        (
            r * 6 / 5,
            r * 2 / 5,
            r * 6 / 5,
            rgb(0x0E, 0x34, 0x82),
            rgb(0x30, 0x6C, 0xD0),
        ),
        (
            -r * 3 / 5,
            r * 9 / 20,
            r * 8 / 5,
            rgb(0x15, 0x48, 0xA8),
            rgb(0x4E, 0x96, 0xF0),
        ),
        (
            r * 3 / 5,
            r * 9 / 20,
            r * 8 / 5,
            rgb(0x15, 0x48, 0xA8),
            rgb(0x4E, 0x96, 0xF0),
        ),
        (
            0,
            r / 2,
            r * 2,
            rgb(0x1E, 0x60, 0xD0),
            rgb(0xB4, 0xD8, 0xFF),
        ),
    ];
    let steps = if wm::animating() || START_ANIM != 0 && since(START_ANIM) < START_OPEN_MS {
        14
    } else {
        28
    };
    for (dx, rx, ry, outer, inner) in petals {
        for k in 0..steps {
            let sx = rx * (steps - k) / steps;
            let sy = ry * (steps - k) / steps;
            let px = cx + dx;
            ellipse(
                hdc,
                b,
                px - sx,
                base - sy,
                px + sx,
                base,
                fc(mix(outer, inner, k, steps - 1)),
            );
        }
    }
}

unsafe fn wallpaper(hdc: Handle, b: &mut Brushes, l: &Layout) {
    load_wallpaper(hdc);
    if !WP_DC.is_null() && FADE >= 1000 {
        StretchBlt(hdc, 0, 0, l.w, l.h, WP_DC, 0, 0, WP_W, WP_H, 0x00CC_0020);
    } else {
        wallpaper_bloom(hdc, b, l);
    }
}

// ---------- Barre des tâches ----------

// Seules les applications reellement fournies sont epinglees.
fn app_bit(app: usize) -> u8 {
    1 << app
}
fn app_launch(app: usize) -> Launch {
    if app == wm::APP_EXPLORER {
        Launch::Explorer
    } else if app == wm::APP_NOTEPAD {
        Launch::Notepad
    } else if app == wm::APP_UTILITIES {
        Launch::Settings
    } else {
        Launch::Settings
    }
}
fn taskbar_app(slot: i32) -> Option<usize> {
    if slot <= 0 {
        return None;
    }
    let mut n = 0;
    for app in [
        wm::APP_EXPLORER,
        wm::APP_NOTEPAD,
        wm::APP_UTILITIES,
        wm::APP_SETTINGS,
    ] {
        if unsafe { PINNED_APPS & app_bit(app) != 0 } || wm::is_open(app) {
            n += 1;
            if slot == n {
                return Some(app);
            }
        }
    }
    None
}
fn taskbar_count() -> i32 {
    1 + [
        wm::APP_EXPLORER,
        wm::APP_NOTEPAD,
        wm::APP_UTILITIES,
        wm::APP_SETTINGS,
    ]
    .iter()
    .filter(|&&app| unsafe { PINNED_APPS & app_bit(app) != 0 } || wm::is_open(app))
    .count() as i32
}
fn icon_visible(i: usize) -> bool {
    unsafe {
        // Les 3 premières icônes sont système (PC, Documents, Corbeille)
        if i < 3 {
            SHOW_SYSTEM_ICONS
        } else {
            DESKTOP_SHORTCUTS & app_bit(i - 3) != 0
        }
    }
}
unsafe fn open_desktop_icon(i: usize) {
    match i {
        0 => explorer::open_pc(),
        1 => explorer::open_documents(),
        2 => explorer::open_trash(),
        3 => launch(Launch::Explorer),
        4 => launch(Launch::Settings),
        _ => {}
    }
}

const SHELL_STATE: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\taskbar.dat";
unsafe fn save_shell_state() {
    for dir in [
        c"C:\\.local",
        c"C:\\.local\\share",
        c"C:\\.local\\share\\winlux",
    ] {
        CreateDirectoryA(dir.as_ptr(), core::ptr::null_mut());
    }
    let h = CreateFileA(
        SHELL_STATE.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        toast(b"Impossible d'enregistrer les raccourcis");
        return;
    }
    let data = [
        b'W',
        b'T',
        2,
        PINNED_APPS,
        DESKTOP_SHORTCUTS,
        if SHOW_SYSTEM_ICONS { 1 } else { 0 },
    ];
    let mut written = 0;
    let ok = WriteFile(
        h,
        data.as_ptr(),
        data.len() as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    if ok == 0 || written != data.len() as u32 {
        toast(b"Enregistrement incomplet des raccourcis");
    }
}
unsafe fn load_shell_state() {
    let h = CreateFileA(
        SHELL_STATE.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return;
    }
    let mut data = [0u8; 6];
    let mut read = 0;
    ReadFile(h, data.as_mut_ptr(), 6, &mut read, core::ptr::null_mut());
    CloseHandle(h);
    if read == 6 && data[..3] == [b'W', b'T', 2] && data[3] <= 3 && data[4] <= 3 {
        PINNED_APPS = data[3];
        DESKTOP_SHORTCUTS = data[4];
        SHOW_SYSTEM_ICONS = data[5] != 0;
    }
}

pub unsafe fn show_system_icons() -> bool {
    SHOW_SYSTEM_ICONS
}

pub unsafe fn set_show_system_icons(show: bool) {
    SHOW_SYSTEM_ICONS = show;
    save_shell_state();
}

fn context_rows(app: usize) -> i32 {
    if app == usize::MAX {
        5
    } else if unsafe { CONTEXT_DESKTOP.is_none() } {
        4
    } else if wm::is_open(app) {
        5
    } else {
        3
    }
}
unsafe fn open_context(app: usize, desktop: Option<usize>, x: i32, y: i32) {
    set_start(false);
    NOTIF_PANEL = false;
    DRAGGED_ICON = -1;
    CONTEXT_APP = Some(app);
    CONTEXT_DESKTOP = desktop;
    CONTEXT_HOVER = -1;
    let w = 278.min(SCREEN_W);
    let h = context_rows(app) * 34 + 8;
    let left = x.clamp(0, (SCREEN_W - w).max(0));
    let top = y.clamp(0, (SCREEN_H - TASKBAR_HEIGHT - h).max(0));
    CONTEXT_RECT = rect(left, top, left + w, top + h);
}
unsafe fn open_context_desktop(x: i32, y: i32) {
    set_start(false);
    NOTIF_PANEL = false;
    DRAGGED_ICON = -1;
    CONTEXT_APP = Some(usize::MAX);
    CONTEXT_DESKTOP = None;
    CONTEXT_HOVER = -1;
    let w = 278.min(SCREEN_W);
    let h = context_rows(usize::MAX) * 34 + 8;
    let left = x.clamp(0, (SCREEN_W - w).max(0));
    let top = y.clamp(0, (SCREEN_H - TASKBAR_HEIGHT - h).max(0));
    CONTEXT_RECT = rect(left, top, left + w, top + h);
}
unsafe fn context_row(x: i32, y: i32) -> i32 {
    if !inside(&CONTEXT_RECT, x, y) || y < CONTEXT_RECT.top + 4 {
        return -1;
    }
    let row = (y - CONTEXT_RECT.top - 4) / 34;
    if CONTEXT_APP.is_some_and(|app| row < context_rows(app)) {
        row
    } else {
        -1
    }
}
unsafe fn context_action(app: usize, desktop: Option<usize>, row: i32, persist: bool) {
    if app == usize::MAX {
        match row {
            0 => {
                wm::open(wm::APP_EXPLORER);
            }
            1 => {
                static TERM: &[u16] = &[
                    b's' as u16,
                    b'h' as u16,
                    b'e' as u16,
                    b'l' as u16,
                    b'l' as u16,
                    b':' as u16,
                    b't' as u16,
                    b'e' as u16,
                    b'r' as u16,
                    b'm' as u16,
                    b'i' as u16,
                    b'n' as u16,
                    b'a' as u16,
                    b'l' as u16,
                    0,
                ];
                static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
                ShellExecuteW(
                    core::ptr::null_mut(),
                    OPEN.as_ptr(),
                    TERM.as_ptr(),
                    core::ptr::null(),
                    core::ptr::null(),
                    1,
                );
            }
            2 => {
                wm::open(wm::APP_SETTINGS);
            }
            3 => {
                crate::notepad::open_new();
                wm::open(wm::APP_NOTEPAD);
            }
            4 => {
                wm::open(wm::APP_UTILITIES);
            }
            _ => {}
        }
        return;
    }
    match row {
        0 => {
            if let Some(i) = desktop {
                open_desktop_icon(i);
            } else if desktop.is_none() {
                // taskbar context - act on app
                if wm::is_open(app) && !wm::is_minimized(app) && wm::is_active(app) {
                    wm::minimize(app);
                } else {
                    wm::open(app);
                }
            } else {
                launch(app_launch(app));
            }
        }
        1 => {
            if desktop.is_none() {
                // taskbar: toggle pin
                PINNED_APPS ^= app_bit(app);
                if persist {
                    save_shell_state();
                }
            } else {
                PINNED_APPS ^= app_bit(app);
                if persist {
                    save_shell_state();
                }
            }
        }
        2 => {
            if desktop.is_none() && !wm::is_open(app) {
                wm::open(app);
            } else if desktop.is_none() {
                // restore/maximize context on taskbar
                if wm::is_open(app) {
                    if wm::is_minimized(app) {
                        wm::open(app);
                    } else if crate::wm::is_maximized(app) {
                        crate::wm::toggle_maximize(app);
                    } else {
                        crate::wm::toggle_maximize(app);
                    }
                }
            } else {
                DESKTOP_SHORTCUTS ^= app_bit(app);
                if persist {
                    save_shell_state();
                }
            }
        }
        3 if desktop.is_none() => {
            if wm::is_open(app) {
                wm::minimize(app);
            }
        }
        3 if !desktop.is_none() && wm::is_open(app) => wm::minimize(app),
        4 if desktop.is_none() || wm::is_open(app) => {
            if wm::is_open(app) {
                wm::close(app);
            }
        }
        _ => {}
    }
}
unsafe fn draw_context(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let Some(app) = CONTEXT_APP else {
        return;
    };
    let r = CONTEXT_RECT;
    let p = pal();
    fill(hdc, b, r.left, r.top, r.right, r.bottom, p.border);
    fill(
        hdc,
        b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        p.surface,
    );
    if app == usize::MAX {
        let labels: [&[u8]; 5] = [
            b"Ouvrir l'Explorateur",
            b"Ouvrir le terminal",
            b"Ouvrir les Param\xE8tres",
            b"Ouvrir le Bloc-notes",
            b"Ouvrir les Utilitaires",
        ];
        for row in 0..context_rows(app) {
            let y = r.top + 4 + row * 34;
            if row == CONTEXT_HOVER {
                fill(hdc, b, r.left + 3, y, r.right - 3, y + 34, p.hover);
            }
            text(
                hdc,
                f.body,
                p.text,
                r.left + 12,
                y + 8,
                labels[row as usize],
            );
        }
        return;
    }
    if unsafe { CONTEXT_DESKTOP.is_none() } {
        let pin: &[u8] = if PINNED_APPS & app_bit(app) != 0 {
            b"D\xE9s\xE9pingler de la barre des t\xE2ches"
        } else {
            b"\xC9pingler \xE0 la barre des t\xE2ches"
        };
        let labels: [&[u8]; 4] = [
            b"Ouvrir / Restaurer",
            pin,
            b"Agrandir / Restaurer",
            b"Fermer",
        ];
        for row in 0..context_rows(app) {
            let y = r.top + 4 + row * 34;
            if row == CONTEXT_HOVER {
                fill(hdc, b, r.left + 3, y, r.right - 3, y + 34, p.hover);
            }
            text(
                hdc,
                f.body,
                p.text,
                r.left + 12,
                y + 8,
                labels[row as usize],
            );
        }
        return;
    }
    let pin: &[u8] = if PINNED_APPS & app_bit(app) != 0 {
        b"Retirer de la barre des t\xE2ches"
    } else {
        b"\xC9pingler \xE0 la barre des t\xE2ches"
    };
    let shortcut: &[u8] = if DESKTOP_SHORTCUTS & app_bit(app) != 0 {
        b"Retirer le raccourci du bureau"
    } else {
        b"Cr\xE9er un raccourci sur le bureau"
    };
    let labels: [&[u8]; 5] = [
        b"Ouvrir / afficher",
        pin,
        shortcut,
        b"R\xE9duire la fen\xEAtre",
        b"Fermer la fen\xEAtre",
    ];
    for row in 0..context_rows(app) {
        let y = r.top + 4 + row * 34;
        if row == CONTEXT_HOVER {
            fill(hdc, b, r.left + 3, y, r.right - 3, y + 34, p.hover);
        }
        text(
            hdc,
            f.body,
            p.text,
            r.left + 12,
            y + 8,
            labels[row as usize],
        );
    }
}

unsafe fn draw_wifi(hdc: Handle, b: &mut Brushes, x: i32, y: i32, on: bool) {
    let c = if on {
        rgb(0xF2, 0xF2, 0xF2)
    } else {
        rgb(0x7A, 0x80, 0x8A)
    };
    for k in 0..4 {
        let hgt = 4 + k * 3;
        fill(hdc, b, x + k * 4, y + 14 - hgt, x + k * 4 + 3, y + 14, c);
    }
}
unsafe fn draw_volume(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    fill(hdc, b, x, y + 5, x + 4, y + 10, c);
    for k in 0..5 {
        fill(hdc, b, x + 4 + k, y + 5 - k, x + 5 + k, y + 10 + k, c);
    }
    if level == 0 {
        for k in 0..6 {
            fill(hdc, b, x + 12 + k, y + 4 + k, x + 14 + k, y + 6 + k, c);
            fill(hdc, b, x + 17 - k, y + 4 + k, x + 19 - k, y + 6 + k, c);
        }
    } else {
        let bars = if level < 34 {
            1
        } else if level < 67 {
            2
        } else {
            3
        };
        for k in 0..bars {
            let hgt = 4 + k * 4;
            fill(
                hdc,
                b,
                x + 12 + k * 3,
                y + 7 - hgt / 2,
                x + 14 + k * 3,
                y + 8 + hgt / 2,
                c,
            );
        }
    }
}
unsafe fn draw_battery(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32) {
    let c = rgb(0xF2, 0xF2, 0xF2);
    round_rect(hdc, b, x, y + 2, x + 22, y + 13, 2, c);
    fill(hdc, b, x + 1, y + 3, x + 21, y + 12, rgb(0x20, 0x22, 0x28));
    fill(
        hdc,
        b,
        x + 2,
        y + 4,
        x + 2 + (18 * level.min(100) as i32) / 100,
        y + 11,
        if level > 20 { c } else { rgb(0xFF, 0x60, 0x50) },
    );
    fill(hdc, b, x + 22, y + 5, x + 24, y + 10, c);
}

unsafe fn two_digits(v: u16) -> [u8; 2] {
    [b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8]
}

unsafe fn taskbar(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout, dy: i32) {
    if wm::native_session() {
        return;
    }
    let top = l.bar_top + dy;
    fill(hdc, b, 0, top, l.w, l.h, rgb(0x1C, 0x1F, 0x26));
    fill(hdc, b, 0, top, l.w, top + 1, rgb(0x34, 0x38, 0x42));
    for i in 0..taskbar_count() {
        let mut r = button_rect(l, i);
        r.top += dy;
        r.bottom += dy;
        let app = taskbar_app(i);
        let g = match app {
            Some(wm::APP_EXPLORER) => Glyph::Folder,
            Some(wm::APP_NOTEPAD) => Glyph::File,
            Some(wm::APP_UTILITIES) => Glyph::Tools,
            Some(_) => Glyph::Settings,
            None => Glyph::Start,
        };
        if let Some(app) = app {
            wm::TASKBAR_SLOT[app] = r;
        }
        let running = app.map_or(false, wm::is_open);
        let focused = app.map_or(false, wm::is_active) || (i == 0 && START_OPEN && !START_CLOSING);
        if focused {
            round_rect(
                hdc,
                b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                5,
                rgb(0x2E, 0x33, 0x3D),
            );
        } else if HOVER_BUTTON == i as i32 {
            round_rect(
                hdc,
                b,
                r.left,
                r.top,
                r.right,
                r.bottom,
                5,
                rgb(0x29, 0x2D, 0x36),
            );
        }
        glyph(hdc, b, f, g, r.left + 10, r.top + 10, 24);
        if running {
            // Indicateur d'application ouverte, comme sous Windows 11.
            let cx = (r.left + r.right) / 2;
            let half = if focused { 8 } else { 3 };
            round_rect(
                hdc,
                b,
                cx - half,
                r.bottom - 3,
                cx + half,
                r.bottom,
                1,
                if focused {
                    ACCENT
                } else {
                    rgb(0x9A, 0xA3, 0xAE)
                },
            );
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
    let date = [
        d0,
        d1,
        b'/',
        o0,
        o1,
        b'/',
        b'0' + (year / 1000 % 10) as u8,
        b'0' + (year / 100 % 10) as u8,
        b'0' + (year / 10 % 10) as u8,
        b'0' + (year % 10) as u8,
    ];
    let right = l.w - 12;
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        right - text_width(&clock, 13),
        top + 8,
        &clock,
    );
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        right - text_width(&date, 13),
        top + 26,
        &date,
    );
    let tray = right - 92 - 96;
    round_rect(
        hdc,
        b,
        tray - 4,
        top + 6,
        tray + 88,
        top + 42,
        5,
        rgb(0x24, 0x28, 0x31),
    );
    draw_wifi(hdc, b, tray + 6, top + 17, NETWORK_CONNECTED != 0);
    draw_volume(hdc, b, tray + 30, top + 17, VOLUME_LEVEL);
    if BATTERY_PRESENT != 0 {
        draw_battery(hdc, b, tray + 58, top + 17, BATTERY_LEVEL);
    } else {
        text(
            hdc,
            f.small,
            rgb(0xC8, 0xCD, 0xD4),
            tray + 60,
            top + 17,
            b"CA",
        );
    }
    text(
        hdc,
        f.semibold,
        rgb(0xC8, 0xCD, 0xD4),
        tray - 24,
        top + 14,
        b"^",
    );
}

// ---------- Menu Démarrer ----------

#[derive(Clone, Copy, PartialEq)]
enum Launch {
    Explorer,
    Settings,
    Documents,
    Pc,
    Programs,
    Updates,
    Terminal,
    Browser,
    Notepad,
    Exec(&'static [u8]),
}

struct App {
    name: &'static [u8],
    glyph: Glyph,
    launch: Launch,
}
const PINNED: [App; 8] = [
    App {
        name: b"Explorer",
        glyph: Glyph::Folder,
        launch: Launch::Explorer,
    },
    App {
        name: b"Param\xE8tres",
        glyph: Glyph::Settings,
        launch: Launch::Settings,
    },
    App {
        name: b"Bloc-notes",
        glyph: Glyph::File,
        launch: Launch::Notepad,
    },
    App {
        name: b"Utilitaires",
        glyph: Glyph::Tools,
        launch: Launch::Settings,
    },
    App {
        name: b"Mises \xE0 jour",
        glyph: Glyph::Store,
        launch: Launch::Updates,
    },
    App {
        name: b"Documents",
        glyph: Glyph::Folder,
        launch: Launch::Documents,
    },
    App {
        name: b"Ce PC",
        glyph: Glyph::Computer,
        launch: Launch::Pc,
    },
    App {
        name: b"Programmes .exe",
        glyph: Glyph::Exe,
        launch: Launch::Programs,
    },
];
const RECOMMENDED: [(&[u8], &[u8], Glyph, Launch); 4] = [
    (
        b"Terminal",
        b"Ouvrir un terminal Linux",
        Glyph::Terminal,
        Launch::Terminal,
    ),
    (
        b"Navigateur",
        b"Ouvrir le navigateur web",
        Glyph::Store,
        Launch::Browser,
    ),
    (
        b"Documents",
        b"Dossier personnel Linux",
        Glyph::Folder,
        Launch::Documents,
    ),
    (
        b"Param\xE8tres Linux et Windows",
        b"Clavier, r\xE9seau et alimentation",
        Glyph::Settings,
        Launch::Settings,
    ),
];

/// Lance une application depuis la barre des tâches ou le menu Démarrer.
unsafe fn launch(what: Launch) {
    match what {
        Launch::Explorer => {
            if wm::is_open(wm::APP_EXPLORER) {
                wm::open(wm::APP_EXPLORER)
            } else {
                explorer::open_home()
            }
        }
        Launch::Notepad => {
            crate::notepad::open_new();
            wm::open(wm::APP_NOTEPAD);
        }
        Launch::Pc | Launch::Programs => explorer::open_pc(),
        Launch::Documents => explorer::open_documents(),
        Launch::Settings => {
            // Si on veut ouvrir les utilitaires, on passe par l'icône dédiée
            wm::open(wm::APP_SETTINGS);
        }
        Launch::Updates => crate::settings::open_page(6),
        Launch::Terminal => {
            static TERM: &[u16] = &[
                b's' as u16,
                b'h' as u16,
                b'e' as u16,
                b'l' as u16,
                b'l' as u16,
                b':' as u16,
                b't' as u16,
                b'e' as u16,
                b'r' as u16,
                b'm' as u16,
                b'i' as u16,
                b'n' as u16,
                b'a' as u16,
                b'l' as u16,
                0,
            ];
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                TERM.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
        }
        Launch::Browser => {
            static BROWSER: &[u16] = &[
                b'w' as u16,
                b'i' as u16,
                b'n' as u16,
                b'l' as u16,
                b'u' as u16,
                b'x' as u16,
                b'-' as u16,
                b'b' as u16,
                b'r' as u16,
                b'o' as u16,
                b'w' as u16,
                b's' as u16,
                b'e' as u16,
                b'r' as u16,
                0,
            ];
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                BROWSER.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
        }
        Launch::Exec(cmd) => {
            // Gestionnaire des tâches intégré dans le WM
            if cmd == b"taskmgr" {
                wm::open(wm::APP_TASKMGR);
                return;
            }
            let mut wcmd = [0u16; 256];
            for (i, &b) in cmd.iter().enumerate() {
                if i < 255 {
                    wcmd[i] = b as u16;
                }
            }
            static OPEN: &[u16] = &[b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
            ShellExecuteW(
                core::ptr::null_mut(),
                OPEN.as_ptr(),
                wcmd.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                1,
            );
        }
    }
}

unsafe fn start_menu(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    let dy = start_offset();
    if !START_OPEN {
        return;
    }
    let s0 = l.start;
    let s = &Rect {
        left: s0.left,
        top: s0.top + dy,
        right: s0.right,
        bottom: s0.bottom + dy,
    };
    let surface = rgb(0x24, 0x27, 0x2F);
    panel(
        hdc,
        b,
        s.left,
        s.top,
        s.right,
        s.bottom,
        8,
        surface,
        rgb(0x3A, 0x3F, 0x4A),
    );
    let pad = 28;
    let inner_l = s.left + pad;
    let inner_r = s.right - pad;

    text(
        hdc,
        f.large,
        rgb(0xF2, 0xF2, 0xF2),
        inner_l,
        s.top + 28,
        b"Winlux",
    );

    // Épinglé / Toutes les applications.
    let y = s.top + 82;
    let title: &[u8] = unsafe {
        if START_SHOW_ALL_APPS {
            b"Toutes mes applications"
        } else {
            b"\xC9pingl\xE9"
        }
    };
    text(hdc, f.title, rgb(0xF2, 0xF2, 0xF2), inner_l + 4, y, title);
    // Barre de recherche
    let search_y = y + 40;
    round_rect(
        hdc,
        b,
        inner_l,
        search_y,
        inner_r,
        search_y + 36,
        18,
        rgb(0x31, 0x35, 0x3F),
    );
    if unsafe { START_SEARCH_LEN > 0 } {
        text(
            hdc,
            f.body,
            rgb(0xE6, 0xE6, 0xE6),
            inner_l + 8,
            search_y + 8,
            unsafe { &START_SEARCH[..START_SEARCH_LEN] },
        );
    } else {
        text(
            hdc,
            f.body,
            rgb(0x88, 0x88, 0x88),
            inner_l + 8,
            search_y + 8,
            b"Rechercher...",
        );
    }
    let all: &[u8] = unsafe {
        if START_SHOW_ALL_APPS {
            b"\xC9pingl\xE9"
        } else {
            b"Toutes mes applications"
        }
    };
    let all_w = text_width(all, 11) + 20;
    text(
        hdc,
        f.small,
        rgb(0xE6, 0xE6, 0xE6),
        inner_r - all_w + 10,
        y + 2,
        all,
    );
    for (i, app) in PINNED
        .iter()
        .take((PIN_COLS * pin_rows(s)) as usize)
        .enumerate()
    {
        let c = pinned_cell(s, i as i32);
        if HOVER_START_ITEM == i as i32 {
            round_rect(
                hdc,
                b,
                c.left,
                c.top,
                c.right,
                c.bottom,
                5,
                rgb(0x31, 0x35, 0x3F),
            );
        }
        let cx = (c.left + c.right) / 2;
        glyph(hdc, b, f, app.glyph, cx - 16, c.top + 10, 32);
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0xE6, 0xE6, 0xE6),
            cx,
            c.top + 52,
            app.name,
        );
    }

    // Recommandé / Catégories.
    let ry = s.top + 118 + pin_rows(s) * PIN_CELL_H + 20;
    if START_SHOW_ALL_APPS {
        // Afficher les catégories
        text(
            hdc,
            f.title,
            rgb(0xF2, 0xF2, 0xF2),
            inner_l + 4,
            ry,
            b"Cat\xE9gories",
        );
        let categories: [&[u8]; 12] = [
            b"Toutes",
            b"Accessoires",
            b"Bureau",
            b"Graphisme",
            b"Internet",
            b"Son et vid\xE9o",
            b"Web",
            b"Programmation",
            b"Pr\xE9f\xE9rences",
            b"Administration",
            b"Favoris",
            b"R\xE9cents",
        ];
        for (i, cat) in categories.iter().enumerate() {
            let c = category_cell(s, i as i32);
            if START_CATEGORY == i as i32 {
                round_rect(
                    hdc,
                    b,
                    c.left,
                    c.top,
                    c.right,
                    c.bottom,
                    5,
                    rgb(0x58, 0x65, 0xF2),
                );
            }
            text_centered(
                hdc,
                f.small,
                11,
                if START_CATEGORY == i as i32 {
                    rgb(0xFF, 0xFF, 0xFF)
                } else {
                    rgb(0xE6, 0xE6, 0xE6)
                },
                (c.left + c.right) / 2,
                c.top + 10,
                cat,
            );
        }
        // Afficher les applications de la catégorie sélectionnée
        let apps_y = ry + 120;
        text(
            hdc,
            f.title,
            rgb(0xF2, 0xF2, 0xF2),
            inner_l + 4,
            apps_y,
            b"Applications",
        );
        let mut displayed = 0;
        for i in 0..unsafe { ALL_APPS_COUNT } {
            let (name, _exec, cat) = unsafe { ALL_APPS[i] };
            let matches_search = unsafe {
                START_SEARCH_LEN == 0
                    || name[..name.len().min(START_SEARCH_LEN)]
                        .iter()
                        .zip(START_SEARCH[..START_SEARCH_LEN].iter())
                        .any(|(&a, &b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
            };
            if (START_CATEGORY == 0 || cat == START_CATEGORY) && matches_search {
                let app_y = apps_y + 40 + (displayed as i32) * 44;
                if app_y + 40 < s.bottom - 72 {
                    text(hdc, f.body, rgb(0xE6, 0xE6, 0xE6), inner_l + 4, app_y, name);
                    displayed += 1;
                }
            }
        }
        // Afficher les raccourcis
        if unsafe { SHORTCUTS_COUNT } > 0 {
            let shortcuts_y = apps_y + 40 + (displayed as i32) * 44 + 20;
            text(
                hdc,
                f.title,
                rgb(0xF2, 0xF2, 0xF2),
                inner_l + 4,
                shortcuts_y,
                b"Raccourcis",
            );
            for i in 0..unsafe { SHORTCUTS_COUNT } {
                let (name, _exec, _x, _y) = unsafe { SHORTCUTS[i] };
                let shortcut_y = shortcuts_y + 40 + (i as i32) * 44;
                if shortcut_y + 40 < s.bottom - 72 {
                    text(
                        hdc,
                        f.body,
                        rgb(0xE6, 0xE6, 0xE6),
                        inner_l + 4,
                        shortcut_y,
                        name,
                    );
                }
            }
        }
    } else if ry + 150 < s.bottom - 72 {
        text(
            hdc,
            f.title,
            rgb(0xF2, 0xF2, 0xF2),
            inner_l + 4,
            ry,
            b"Recommand\xE9",
        );
        for (i, (title, sub, g, _)) in RECOMMENDED.iter().enumerate() {
            let c = recommended_cell(s, i as i32);
            if HOVER_START_ITEM == 100 + i as i32 {
                round_rect(
                    hdc,
                    b,
                    c.left,
                    c.top,
                    c.right,
                    c.bottom,
                    5,
                    rgb(0x31, 0x35, 0x3F),
                );
            }
            glyph(hdc, b, f, *g, c.left + 8, c.top + 10, 28);
            text(
                hdc,
                f.body,
                rgb(0xF2, 0xF2, 0xF2),
                c.left + 48,
                c.top + 8,
                title,
            );
            text(
                hdc,
                f.small,
                rgb(0x9A, 0xA3, 0xAE),
                c.left + 48,
                c.top + 26,
                sub,
            );
        }
    }

    // Pied : utilisateur, alignement de la barre, alimentation.
    fill(
        hdc,
        b,
        s.left + 1,
        s.bottom - 64,
        s.right - 1,
        s.bottom - 1,
        rgb(0x1F, 0x22, 0x29),
    );
    fill(
        hdc,
        b,
        s.left + 1,
        s.bottom - 64,
        s.right - 1,
        s.bottom - 63,
        rgb(0x34, 0x38, 0x42),
    );
    let ay = s.bottom - 48;
    ellipse(
        hdc,
        b,
        inner_l + 6,
        ay,
        inner_l + 38,
        ay + 32,
        rgb(0x5B, 0x6B, 0xD8),
    );
    text_centered(
        hdc,
        f.semibold,
        13,
        rgb(0xFF, 0xFF, 0xFF),
        inner_l + 22,
        ay + 8,
        b"D",
    );
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        inner_l + 50,
        ay + 8,
        b"Damien",
    );
    let t = align_toggle_rect(s);
    round_rect(
        hdc,
        b,
        t.left,
        t.top,
        t.right,
        t.bottom,
        4,
        if HOVER_START_ITEM == 200 {
            rgb(0x3A, 0x3F, 0x4A)
        } else {
            rgb(0x31, 0x35, 0x3F)
        },
    );
    let label: &[u8] = if ALIGN_LEFT {
        b"Barre : \xE0 gauche"
    } else {
        b"Barre : centr\xE9e"
    };
    text_centered(
        hdc,
        f.small,
        11,
        rgb(0xE6, 0xE6, 0xE6),
        (t.left + t.right) / 2,
        t.top + 9,
        label,
    );
    // Bouton d'alimentation.
    let (px, py) = (inner_r - 30, ay + 4);
    let pbg = if HOVER_START_ITEM == 201 {
        rgb(0x31, 0x35, 0x3F)
    } else {
        rgb(0x1F, 0x22, 0x29)
    };
    if HOVER_START_ITEM == 201 {
        round_rect(hdc, b, px - 8, py - 6, px + 32, py + 30, 4, pbg);
    }
    ellipse(hdc, b, px, py, px + 24, py + 24, rgb(0xE6, 0xE6, 0xE6));
    ellipse(hdc, b, px + 3, py + 3, px + 21, py + 21, pbg);
    fill(hdc, b, px + 8, py - 1, px + 16, py + 8, pbg);
    fill(
        hdc,
        b,
        px + 11,
        py - 1,
        px + 13,
        py + 12,
        rgb(0xE6, 0xE6, 0xE6),
    );
}

/// Élément du menu Démarrer sous le pointeur : 0..11 épinglés, 100..103
/// recommandés, 200 bascule d'alignement, 201 alimentation, 300 toutes mes applications,
/// 301..312 catégories, 400..463 applications, -1 rien.
fn start_item_at(l: &Layout, x: i32, y: i32) -> i32 {
    let s = &l.start;
    let y0 = s.top + 82;
    let all_w = unsafe {
        text_width(
            if START_SHOW_ALL_APPS {
                b"\xC9pingl\xE9"
            } else {
                b"Toutes mes applications"
            },
            11,
        ) + 20
    };
    let inner_r = s.right - 28;
    if x >= inner_r - all_w + 10 && x < inner_r + 10 && y >= y0 + 2 && y < y0 + 20 {
        return 300;
    }
    if unsafe { START_SHOW_ALL_APPS } {
        for i in 0..12 {
            if inside(&category_cell(s, i), x, y) {
                return 301 + i;
            }
        }
        let apps_y = s.top + 118 + pin_rows(s) * PIN_CELL_H + 140;
        let inner_l = s.left + 28;
        let mut displayed = 0;
        for i in 0..unsafe { ALL_APPS_COUNT } {
            let cat = unsafe { ALL_APPS[i].2 };
            if unsafe { START_CATEGORY == 0 || cat == START_CATEGORY } {
                let app_y = apps_y + (displayed as i32) * 44;
                if y >= app_y && y < app_y + 40 && x >= inner_l + 4 && x < inner_r {
                    return 400 + i as i32;
                }
                displayed += 1;
            }
        }
    } else {
        for i in 0..PINNED.len() as i32 {
            if inside(&pinned_cell(s, i), x, y) {
                return i;
            }
        }
        let ry = s.top + 118 + pin_rows(s) * PIN_CELL_H + 20;
        if ry + 150 < s.bottom - 72 {
            for i in 0..4 {
                if inside(&recommended_cell(s, i), x, y) {
                    return 100 + i;
                }
            }
        }
    }
    if inside(&align_toggle_rect(s), x, y) {
        return 200;
    }
    if inside(&power_rect(s), x, y) {
        return 201;
    }
    -1
}

// ---------- Notifications ----------

pub unsafe fn toast(msg: &[u8]) {
    let n = msg.len().min(TOAST.len());
    TOAST[..n].copy_from_slice(&msg[..n]);
    TOAST_LEN = n;
    TOAST_START = now();
    // Historique pour le panneau de notifications.
    if NOTIF_COUNT < NOTIF_HISTORY.len() {
        NOTIF_HISTORY[NOTIF_COUNT][..n].copy_from_slice(&msg[..n]);
        NOTIF_LENS[NOTIF_COUNT] = n;
        NOTIF_COUNT += 1;
    } else {
        for i in 1..NOTIF_HISTORY.len() {
            NOTIF_HISTORY[i - 1] = NOTIF_HISTORY[i];
            NOTIF_LENS[i - 1] = NOTIF_LENS[i];
        }
        let last = NOTIF_HISTORY.len() - 1;
        NOTIF_HISTORY[last][..n].copy_from_slice(&msg[..n]);
        NOTIF_LENS[last] = n;
    }
}

unsafe fn draw_toast(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if TOAST_LEN == 0 {
        return;
    }
    let t = since(TOAST_START);
    if t > TOAST_MS {
        TOAST_LEN = 0;
        return;
    }
    // Entrée par la droite, sortie en glissant vers le bas.
    let slide = if t < 260 {
        (1000 - ease_out(t, 260)) * 380 / 1000
    } else {
        0
    };
    let drop = if t > TOAST_MS - 220 {
        ease_out(t - (TOAST_MS - 220), 220) * 140 / 1000
    } else {
        0
    };
    let w = 364;
    let x = l.w - w - 12 + slide;
    let y = l.bar_top - 112 + drop;
    panel(
        hdc,
        b,
        x,
        y,
        x + w,
        y + 100,
        8,
        rgb(0x2B, 0x2B, 0x2B),
        rgb(0x3D, 0x3D, 0x3D),
    );
    glyph(hdc, b, f, Glyph::Start, x + 16, y + 16, 16);
    text(
        hdc,
        f.caption,
        rgb(0xC5, 0xC5, 0xC5),
        x + 42,
        y + 16,
        b"Winlux",
    );
    let msg = &TOAST[..TOAST_LEN];
    // Retour à la ligne simple au dernier espace qui tient.
    let per_line = ((w - 40) * 100 / (13 * 56)) as usize;
    let cut = if msg.len() > per_line {
        msg[..per_line]
            .iter()
            .rposition(|&c| c == b' ')
            .unwrap_or(per_line)
    } else {
        msg.len()
    };
    text(
        hdc,
        f.body,
        rgb(0xF2, 0xF2, 0xF2),
        x + 16,
        y + 46,
        &msg[..cut],
    );
    if cut < msg.len() {
        text_fit(
            hdc,
            f.body,
            13,
            rgb(0xF2, 0xF2, 0xF2),
            x + 16,
            y + 66,
            &msg[(cut + 1).min(msg.len())..],
            w - 32,
        );
    }
}

// ---------- Panneau de notifications ----------

unsafe fn draw_notif_panel(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !NOTIF_PANEL {
        return;
    }
    let w = 380;
    let x = l.w - w - 12;
    let header_h = 44;
    let row_h = 56;
    let rows = NOTIF_COUNT.min(NOTIF_HISTORY.len());
    let empty_h = 60;
    let h = header_h
        + if rows == 0 {
            empty_h
        } else {
            rows as i32 * row_h
        }
        + 8;
    let y = l.bar_top - 12 - h;
    panel(
        hdc,
        b,
        x,
        y,
        x + w,
        y + h,
        8,
        rgb(0x24, 0x27, 0x2F),
        rgb(0x3A, 0x3F, 0x4A),
    );
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        x + 16,
        y + 14,
        b"Notifications",
    );
    if NOTIF_COUNT > 0 {
        let clr = rect(x + w - 120, y + 8, x + w - 16, y + 36);
        round_rect(
            hdc,
            b,
            clr.left,
            clr.top,
            clr.right,
            clr.bottom,
            4,
            rgb(0x31, 0x35, 0x3F),
        );
        text(
            hdc,
            f.small,
            rgb(0xC5, 0xC5, 0xC5),
            clr.left + 10,
            clr.top + 7,
            b"Tout effacer",
        );
    }
    fill(
        hdc,
        b,
        x + 1,
        y + header_h - 1,
        x + w - 1,
        y + header_h,
        rgb(0x3A, 0x3F, 0x4A),
    );
    if rows == 0 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            x + w / 2,
            y + header_h + 20,
            b"Aucune notification",
        );
    } else {
        for i in 0..rows {
            let idx = if NOTIF_COUNT <= NOTIF_HISTORY.len() {
                i
            } else {
                i
            };
            let ry = y + header_h + (rows - 1 - i) as i32 * row_h;
            if i > 0 {
                fill(
                    hdc,
                    b,
                    x + 16,
                    ry - 1,
                    x + w - 16,
                    ry,
                    rgb(0x34, 0x38, 0x42),
                );
            }
            glyph(hdc, b, f, Glyph::Start, x + 16, ry + 10, 14);
            text(
                hdc,
                f.caption,
                rgb(0xC5, 0xC5, 0xC5),
                x + 38,
                ry + 8,
                b"Winlux",
            );
            let msg = &NOTIF_HISTORY[idx][..NOTIF_LENS[idx]];
            text_fit(
                hdc,
                f.body,
                13,
                rgb(0xF2, 0xF2, 0xF2),
                x + 16,
                ry + 28,
                msg,
                w - 32,
            );
        }
    }
}

// ---------- Icônes du bureau ----------

const DESKTOP_ICONS: [(&[u8], Glyph); 5] = [
    (b"Ce PC", Glyph::Computer),
    (b"Documents", Glyph::Folder),
    (b"Corbeille", Glyph::Trash),
    (b"Explorer", Glyph::Folder),
    (b"Param\xE8tres", Glyph::Settings),
];

unsafe fn desktop_icons(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    for (i, (name, g)) in DESKTOP_ICONS.iter().enumerate() {
        if !icon_visible(i) {
            continue;
        }
        let (x, y) = (ICON_X[i], ICON_Y[i]);
        if DRAGGED_ICON == i as i32 || SELECTED_ICON == i as i32 {
            round_rect(hdc, b, x, y, x + 76, y + 84, 4, fc(rgb(0x2A, 0x4E, 0x8C)));
        }
        glyph(hdc, b, f, *g, x + 18, y + 8, 40);
        text_centered(hdc, f.body, 13, rgb(0xFF, 0xFF, 0xFF), x + 38, y + 58, name);
    }
}

// ---------- Écran de démarrage ----------

/// Logo et cercle de points qui tournent, comme au démarrage de Windows 11 ;
/// en dessous, l'étape de chargement en cours.
unsafe fn boot_splash(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    fill(hdc, b, 0, 0, l.w, l.h, rgb(0, 0, 0));
    let t = since(BOOT_START);
    // Le logo apparaît en fondu pendant 400 ms.
    let a = ease_out(t, 400);
    let size = 96;
    let (lx, ly) = (l.w / 2 - size / 2, l.h / 2 - size - 30);
    let gap = 4;
    let c = (size - gap) / 2;
    let blue = |k: u32| mix(0, k, a, 1000);
    fill(hdc, b, lx, ly, lx + c, ly + c, blue(rgb(0x2B, 0xA4, 0xF2)));
    fill(
        hdc,
        b,
        lx + c + gap,
        ly,
        lx + size,
        ly + c,
        blue(rgb(0x45, 0xB8, 0xFF)),
    );
    fill(
        hdc,
        b,
        lx,
        ly + c + gap,
        lx + c,
        ly + size,
        blue(rgb(0x16, 0x84, 0xE0)),
    );
    fill(
        hdc,
        b,
        lx + c + gap,
        ly + c + gap,
        lx + size,
        ly + size,
        blue(rgb(0x2B, 0xA4, 0xF2)),
    );
    // Cercle de chargement : cinq points qui accélèrent puis ralentissent.
    if t > 500 {
        let (cx, cy, radius) = (l.w / 2, l.h / 2 + 70, 22);
        let cycle = 2200;
        for k in 0..5 {
            let local = (t - 500 - k * 110).rem_euclid(cycle);
            let u = local * 1000 / cycle;
            // Angle « slow-fast-slow » sur deux tours, puis pause invisible.
            if u > 880 {
                continue;
            }
            let v = u * 1000 / 880;
            let angle = 270 + (v * 720 / 1000) - sin_milli(v * 360 / 1000) * 70 / 1000;
            let x = cx + cos_milli(angle) * radius / 1000;
            let y = cy + sin_milli(angle) * radius / 1000;
            let d = 5;
            ellipse(
                hdc,
                b,
                x - d / 2,
                y - d / 2,
                x + d - d / 2,
                y + d - d / 2,
                rgb(0xF2, 0xF2, 0xF2),
            );
        }
    }
    let label: &[u8] = match BOOT_STEP {
        0 | 1 => b"Chargement des polices",
        2 => b"Pr\xE9paration de l'Explorer",
        3 => b"Pr\xE9paration du bureau",
        _ => b"Bienvenue",
    };
    if t > 700 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            l.w / 2,
            l.h - 90,
            label,
        );
    }
    // Préchauffage des polices : un texte de chaque taille hors de l'écran,
    // pour que le premier affichage du bureau ne marque pas de pause.
    if BOOT_STEP <= 1 {
        for font in [
            f.title, f.body, f.semibold, f.small, f.icon, f.large, f.caption, f.display,
        ] {
            text(hdc, font, rgb(0, 0, 0), -4000, -4000, b"Winlux");
        }
    }
}

// ---------- Points d'entrée ----------

/// Lit l'environnement et la taille de l'écran au lancement.
pub unsafe fn init(
    volume: u32,
    battery: u32,
    battery_present: u32,
    network: u32,
    align_left: bool,
) {
    VOLUME_LEVEL = volume;
    BATTERY_LEVEL = battery;
    BATTERY_PRESENT = battery_present;
    NETWORK_CONNECTED = network;
    ALIGN_LEFT = align_left;
    crate::settings::system_tick();
    load_shell_state();
    init_apps();
    SCREEN_W = GetSystemMetrics(0).max(640);
    SCREEN_H = GetSystemMetrics(1).max(400);
    wm::WORK = rect(0, 0, SCREEN_W, SCREEN_H - TASKBAR_HEIGHT);
    BOOT_START = now();
    let mut v = [0i8; 4];
    if GetEnvironmentVariableA(c"WINLUX_NO_SPLASH".as_ptr(), v.as_mut_ptr(), 4) > 0 {
        BOOT_DONE = true;
        BOOT_STEP = 4;
    }
}

pub unsafe fn splash_active() -> bool {
    !BOOT_DONE
}

pub unsafe fn draw(hdc: Handle) {
    let l = layout();
    let mut b = Brushes::new();
    let f = Fonts::new();
    SetBkMode(hdc, 1);
    if SCREENSAVER {
        fill(hdc, &mut b, 0, 0, SCREEN_W, SCREEN_H, rgb(0x00, 0x00, 0x00));
        for i in 0..12 {
            let pipe = SAVER_PIPES[i];
            let c = [
                rgb(0xFF, 0, 0),
                rgb(0, 0xFF, 0),
                rgb(0, 0xFF, 0xFF),
                rgb(0xFF, 0, 0xFF),
                rgb(0xFF, 0xA5, 0),
                rgb(0xA5, 0, 0xFF),
            ][i % 6];
            fill(hdc, &mut b, pipe.0, pipe.1, pipe.0 + 10, pipe.1 + 10, c);
            for j in 0..8 {
                fill(
                    hdc,
                    &mut b,
                    pipe.0 - j * 3,
                    pipe.1 + 2,
                    pipe.0 - j * 3 + 6,
                    pipe.1 + 8,
                    c,
                );
                fill(
                    hdc,
                    &mut b,
                    pipe.0 + 2,
                    pipe.1 - j * 3,
                    pipe.0 + 8,
                    pipe.1 - j * 3 + 6,
                    c,
                );
            }
        }
        text(
            hdc,
            f.small,
            rgb(0x60, 0x60, 0x60),
            12,
            SCREEN_H - 28,
            b"\xC9cran de veille",
        );
        f.release();
        b.release();
        return;
    }
    if !BOOT_DONE {
        boot_splash(hdc, &mut b, &f, &l);
    } else {
        let intro = since(INTRO_START);
        FADE = if INTRO_START == 0 || intro >= INTRO_MS {
            1000
        } else {
            ease_out(intro, INTRO_MS)
        };
        wallpaper(hdc, &mut b, &l);
        if FADE > 600 {
            desktop_icons(hdc, &mut b, &f);
        }
        {
            let mut g = Gfx {
                hdc,
                b: &mut b,
                f: &f,
            };
            wm::draw(&mut g);
        }
        start_menu(hdc, &mut b, &f, &l);
        draw_notif_panel(hdc, &mut b, &f, &l);
        draw_toast(hdc, &mut b, &f, &l);
        let bar_dy = if FADE < 1000 {
            (1000 - ease_out(intro, INTRO_MS)) * TASKBAR_HEIGHT / 1000
        } else {
            0
        };
        taskbar(hdc, &mut b, &f, &l, bar_dy);
        draw_context(hdc, &mut b, &f);
    }
    f.release();
    b.release();
}

/// Appelé à chaque tour de la boucle de messages : fait avancer le
/// démarrage et repeint à ~60 images/s tant qu'une animation est en cours.
pub unsafe fn frame(hwnd: Handle) {
    if SCREENSAVER {
        if !SAVER_INIT {
            SAVER_INIT = true;
            for i in 0..12 {
                SAVER_PIPES[i] = (
                    ((i as i32) * 137 % (SCREEN_W / 8)) * 8,
                    ((i as i32) * 251 % (SCREEN_H / 8)) * 8,
                    if i % 2 == 0 { 8 } else { -8 },
                    if i % 3 == 0 { 8 } else { -8 },
                );
            }
        }
        // update
        for i in 0..12 {
            let mut p = SAVER_PIPES[i];
            p.0 += p.2;
            p.1 += p.3;
            if p.0 < 0 || p.0 >= SCREEN_W {
                p.2 = -p.2;
            }
            if p.1 < 40 || p.1 >= SCREEN_H - 48 {
                p.3 = -p.3;
            }
            SAVER_PIPES[i] = p;
        }
        if since(LAST_FRAME) >= 15 {
            LAST_FRAME = now();
            InvalidateRect(hwnd, core::ptr::null(), 0);
        }
        return;
    }
    if !BOOT_DONE {
        // Une étape de préchargement par image, sans bloquer l'animation.
        match BOOT_STEP {
            0 | 1 if since(BOOT_START) > 300 => BOOT_STEP += 1,
            2 if since(BOOT_START) > 900 => {
                explorer::preload();
                BOOT_STEP = 3;
            }
            3 if since(BOOT_START) > 1600 => BOOT_STEP = 4,
            4 if since(BOOT_START) as u32 >= SPLASH_MIN_MS => {
                BOOT_DONE = true;
                INTRO_START = now();
            }
            _ => {}
        }
    }
    let animating = !BOOT_DONE
        || wm::animating()
        || crate::gfx::theme_animating()
        || since(INTRO_START) < INTRO_MS + 40
        || (START_ANIM != 0 && since(START_ANIM) < START_OPEN_MS + 40)
        || START_CLOSING
        || (TOAST_LEN > 0 && (since(TOAST_START) < 300 || since(TOAST_START) > TOAST_MS - 260));
    if animating && since(LAST_FRAME) >= 15 {
        LAST_FRAME = now();
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

/// Appelé deux fois par seconde : suit la résolution de l'écran, rafraîchit
/// l'horloge à chaque nouvelle minute et les champs de saisie qui clignotent.
pub unsafe fn tick(hwnd: Handle) {
    if SCREENSAVER {
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    let (w, h) = (GetSystemMetrics(0).max(640), GetSystemMetrics(1).max(400));
    if w != SCREEN_W || h != SCREEN_H {
        SCREEN_W = w;
        SCREEN_H = h;
        wm::WORK = rect(0, 0, w, h - TASKBAR_HEIGHT);
        wm::on_resize();
        CONTEXT_APP = None;
        for i in 0..DESKTOP_ICONS.len() {
            ICON_X[i] = ICON_X[i].min(w - 80);
            ICON_Y[i] = ICON_Y[i].min(h - TASKBAR_HEIGHT - 90);
        }
        MoveWindow(hwnd, 0, 0, w, h, 1);
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    let mut now = SystemTime::default();
    GetLocalTime(&mut now);
    let system = crate::settings::system_tick();
    let apps = BOOT_DONE && wm::tick();
    if (now.minute != LAST_MINUTE && BOOT_DONE) || apps || system {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

unsafe fn repaint(hwnd: Handle) {
    InvalidateRect(hwnd, core::ptr::null(), 0);
}

pub unsafe fn on_button_down(hwnd: Handle, x: i32, y: i32) {
    if SCREENSAVER {
        SCREENSAVER = false;
        LAST_INPUT = now();
        repaint(hwnd);
        return;
    }
    LAST_INPUT = now();
    if !BOOT_DONE {
        return;
    }
    let l = layout();
    if let Some(app) = CONTEXT_APP {
        let row = context_row(x, y);
        let desktop = CONTEXT_DESKTOP;
        CONTEXT_APP = None;
        if row >= 0 {
            context_action(app, desktop, row, true);
        }
        repaint(hwnd);
        return;
    }
    if y >= l.bar_top {
        for i in 0..taskbar_count() {
            if inside(&button_rect(&l, i), x, y) {
                if i == 0 {
                    set_start(!(START_OPEN && !START_CLOSING));
                } else {
                    set_start(false);
                    match taskbar_app(i) {
                        Some(wm::APP_EXPLORER) => {
                            if wm::is_open(wm::APP_EXPLORER) {
                                wm::toggle(wm::APP_EXPLORER)
                            } else {
                                explorer::open_home()
                            }
                        }
                        Some(wm::APP_SETTINGS) => wm::toggle(wm::APP_SETTINGS),
                        _ => {}
                    }
                }
                repaint(hwnd);
                return;
            }
        }
        let tray = l.w - 12 - 92 - 96;
        if x >= tray - 28 && x < tray {
            NOTIF_PANEL = !NOTIF_PANEL;
        } else if x >= tray && x < tray + 24 {
            crate::settings::open_page(2);
        } else if x >= tray + 24 && x < tray + 88 {
            crate::settings::open_page(3);
        } else {
            return;
        }
        repaint(hwnd);
        return;
    }
    if NOTIF_PANEL && y < l.bar_top {
        let pw = 380;
        let px = l.w - pw - 12;
        let rows = NOTIF_COUNT.min(NOTIF_HISTORY.len());
        let ph = 44 + if rows == 0 { 60 } else { rows as i32 * 56 } + 8;
        let py = l.bar_top - 12 - ph;
        if x >= px && x < px + pw && y >= py && y < py + ph {
            if NOTIF_COUNT > 0
                && x >= px + pw - 120
                && x < px + pw - 16
                && y >= py + 8
                && y < py + 36
            {
                NOTIF_COUNT = 0;
                for i in 0..NOTIF_LENS.len() {
                    NOTIF_LENS[i] = 0;
                }
            }
        } else {
            NOTIF_PANEL = false;
        }
        repaint(hwnd);
        return;
    }
    if START_OPEN && !START_CLOSING {
        if inside(&l.start, x, y) {
            match start_item_at(&l, x, y) {
                200 => {
                    crate::settings::control(if ALIGN_LEFT {
                        c"align-center"
                    } else {
                        c"align-left"
                    });
                }
                201 => {
                    crate::settings::control(c"poweroff");
                }
                300 => {
                    START_SHOW_ALL_APPS = !START_SHOW_ALL_APPS;
                }
                i @ 301..=312 => {
                    START_CATEGORY = i - 301;
                }
                i @ 400..=463 => {
                    set_start(false);
                    let app_idx = (i - 400) as usize;
                    if app_idx < unsafe { ALL_APPS_COUNT } {
                        let exec = unsafe { ALL_APPS[app_idx].1 };
                        launch(Launch::Exec(exec));
                    }
                }
                i @ 500..=515 => {
                    set_start(false);
                    let shortcut_idx = (i - 500) as usize;
                    if shortcut_idx < unsafe { SHORTCUTS_COUNT } {
                        let exec = unsafe { SHORTCUTS[shortcut_idx].1 };
                        launch(Launch::Exec(exec));
                    }
                }
                i @ 0..=5 => {
                    set_start(false);
                    launch(PINNED[i as usize].launch);
                }
                i @ 100..=103 => {
                    set_start(false);
                    launch(RECOMMENDED[(i - 100) as usize].3);
                }
                _ => return,
            }
        } else {
            set_start(false);
        }
        repaint(hwnd);
        return;
    }
    let mut rp = false;
    if wm::on_button_down(x, y, &mut rp) {
        SELECTED_ICON = -1;
        if rp {
            repaint(hwnd);
        }
        return;
    }
    for i in (0..DESKTOP_ICONS.len()).rev() {
        if !icon_visible(i) {
            continue;
        }
        if x >= ICON_X[i] && x < ICON_X[i] + 76 && y >= ICON_Y[i] && y < ICON_Y[i] + 84 {
            // Double-clic : ouvre l'emplacement dans l'Explorer.
            if SELECTED_ICON == i as i32 && since(ICON_CLICK_TIME) < 500 {
                SELECTED_ICON = -1;
                open_desktop_icon(i);
            } else {
                DRAGGED_ICON = i as i32;
                SELECTED_ICON = i as i32;
                ICON_CLICK_TIME = now();
            }
            repaint(hwnd);
            return;
        }
    }
    if SELECTED_ICON >= 0 || rp {
        SELECTED_ICON = -1;
        repaint(hwnd);
    }
}

pub unsafe fn on_mouse_move(hwnd: Handle, x: i32, y: i32) {
    if SCREENSAVER {
        SCREENSAVER = false;
        LAST_INPUT = now();
        repaint(hwnd);
        return;
    }
    LAST_INPUT = now();
    if !BOOT_DONE {
        return;
    }
    let l = layout();
    if CONTEXT_APP.is_some() {
        let row = context_row(x, y);
        if row != CONTEXT_HOVER {
            CONTEXT_HOVER = row;
            repaint(hwnd);
        }
        return;
    }
    let mut rp = false;
    let hb = if y >= l.bar_top {
        (0..taskbar_count())
            .find(|&i| inside(&button_rect(&l, i), x, y))
            .unwrap_or(-1)
    } else {
        -1
    };
    if hb != HOVER_BUTTON {
        HOVER_BUTTON = hb;
        rp = true;
    }
    let hs = if START_OPEN && inside(&l.start, x, y) {
        start_item_at(&l, x, y)
    } else {
        -1
    };
    if hs != HOVER_START_ITEM {
        HOVER_START_ITEM = hs;
        rp = true;
    }
    if DRAGGED_ICON >= 0 {
        let i = DRAGGED_ICON as usize;
        // Grille de 96 px, comme l'alignement automatique de Windows.
        let gx = ((x - 38).max(0) + 48) / 96 * 96 + 12;
        let gy = ((y - 42).max(0) + 48) / 96 * 96 + 12;
        let nx = gx.min(l.w - 80);
        let ny = gy.min(l.bar_top - 90);
        if nx != ICON_X[i] || ny != ICON_Y[i] {
            ICON_X[i] = nx;
            ICON_Y[i] = ny;
            rp = true;
        }
    } else if !START_OPEN {
        wm::on_mouse_move(x, y, &mut rp);
    }
    if rp {
        repaint(hwnd);
    }
}

pub unsafe fn on_button_up(hwnd: Handle, x: i32, y: i32) {
    if SCREENSAVER {
        SCREENSAVER = false;
        LAST_INPUT = now();
        repaint(hwnd);
        return;
    }
    LAST_INPUT = now();
    if CONTEXT_APP.is_some() {
        return;
    }
    let mut rp = false;
    if DRAGGED_ICON >= 0 {
        DRAGGED_ICON = -1;
        rp = true;
    }
    wm::on_button_up(x, y, &mut rp);
    if rp {
        repaint(hwnd);
    }
}

/// Molette (`delta` en multiples de 120), clic droit ou milieu.
pub unsafe fn on_other(hwnd: Handle, ev: Mouse, x: i32, y: i32) {
    if SCREENSAVER {
        SCREENSAVER = false;
        LAST_INPUT = now();
        repaint(hwnd);
        return;
    }
    LAST_INPUT = now();
    if wm::native_session() && ev == Mouse::RightDown {
        ShellExecuteA(
            core::ptr::null_mut(),
            c"open".as_ptr(),
            c"winlux-desktop-action".as_ptr(),
            c"context".as_ptr(),
            core::ptr::null(),
            1,
        );
        return;
    }
    if !BOOT_DONE {
        return;
    }
    if matches!(ev, Mouse::RightDown) {
        let l = layout();
        if y >= l.bar_top {
            for slot in 1..taskbar_count() {
                if inside(&button_rect(&l, slot), x, y) {
                    if let Some(app) = taskbar_app(slot) {
                        open_context(app, None, x, l.bar_top);
                        repaint(hwnd);
                        return;
                    }
                }
            }
        } else if START_OPEN && inside(&l.start, x, y) {
            let item = start_item_at(&l, x, y);
            if item == 0 || item == 1 {
                open_context(item as usize, None, x, y);
                repaint(hwnd);
                return;
            }
        } else if y >= l.bar_top {
            for slot in 1..taskbar_count() {
                if inside(&button_rect(&l, slot), x, y) {
                    if let Some(app) = taskbar_app(slot) {
                        open_context(app, None, x, l.bar_top);
                        repaint(hwnd);
                        return;
                    }
                }
            }
            CONTEXT_APP = None;
            let mut rp = false;
            if wm::on_other(ev, x, y, &mut rp) {
                if rp {
                    repaint(hwnd);
                }
                return;
            }
        } else {
            CONTEXT_APP = None;
            let mut rp = false;
            if wm::on_other(ev, x, y, &mut rp) {
                if rp {
                    repaint(hwnd);
                }
                return;
            }
            for i in (0..DESKTOP_ICONS.len()).rev() {
                if icon_visible(i)
                    && inside(
                        &rect(ICON_X[i], ICON_Y[i], ICON_X[i] + 76, ICON_Y[i] + 84),
                        x,
                        y,
                    )
                {
                    let app = if i == 4 {
                        wm::APP_SETTINGS
                    } else {
                        wm::APP_EXPLORER
                    };
                    open_context(app, Some(i), x, y);
                    repaint(hwnd);
                    return;
                }
            }
            open_context_desktop(x, y);
            repaint(hwnd);
            return;
        }
        if CONTEXT_APP.take().is_some() {
            repaint(hwnd);
            return;
        }
    }
    if START_OPEN || CONTEXT_APP.is_some() {
        return;
    }
    let mut rp = false;
    wm::on_other(ev, x, y, &mut rp);
    if rp {
        repaint(hwnd);
    }
}

/// Clavier : touche Windows pour le menu Démarrer, Échap pour le fermer,
/// le reste va à la fenêtre active.
pub unsafe fn on_key(hwnd: Handle, vk: u32, ch: u16) {
    if SCREENSAVER {
        SCREENSAVER = false;
        LAST_INPUT = now();
        repaint(hwnd);
        return;
    }
    LAST_INPUT = now();
    // F2 rename
    if ch == 0 && vk == 0x71 {
        if SELECTED_ICON >= 0 {
            // simple inline rename not fully implemented, just mark
        }
        repaint(hwnd);
        return;
    }
    // Escape closes run/start/context
    if (vk == 0x1B || ch == 27) {
        if crate::wm::SHOW_RUN {
            crate::wm::SHOW_RUN = false;
            repaint(hwnd);
            return;
        }
        if START_OPEN {
            set_start(false);
            repaint(hwnd);
            return;
        }
        if CONTEXT_APP.is_some() {
            CONTEXT_APP = None;
            repaint(hwnd);
            return;
        }
    }
    // Ctrl+Shift+Escape (Gestionnaire des tâches)
    if ch == 0 && vk == 0x1B {
        let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
        let shift = ((GetKeyState(0x10) as u16) & 0x8000) != 0;
        if ctrl && shift {
            wm::open(wm::APP_TASKMGR);
            repaint(hwnd);
            return;
        }
    }
    // Win+Shift+S (Screenshot)
    if ch == 0 && vk == 0x53 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        let shift = ((GetKeyState(0x10) as u16) & 0x8000) != 0;
        if win && shift {
            launch(Launch::Exec(b"screenshot.exe"));
            return;
        }
    }
    // Win+E (Explorateur de fichiers)
    if ch == 0 && vk == 0x45 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            launch(Launch::Explorer);
            return;
        }
    }
    // Win+R
    if ch == 0 && vk == 0x52 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            wm::SHOW_RUN = true;
            unsafe {
                crate::wm::RUN_TEXT = [0; 256];
                crate::wm::RUN_LEN = 0;
            }
            repaint(hwnd);
            return;
        }
    }
    // Win+E
    if ch == 0 && vk == 0x45 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            explorer::open_pc();
            repaint(hwnd);
            return;
        }
    }
    // Win+D
    if ch == 0 && vk == 0x44 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            for app in 0..wm::APP_COUNT {
                if wm::is_open(app) && !wm::is_visible(app) { /* already minimized */ }
            }
            // toggle - minimize all visible windows or restore
            let mut any_visible = false;
            unsafe {
                for app in 0..wm::APP_COUNT {
                    if wm::is_open(app) && !wm::is_minimized(app) {
                        any_visible = true;
                    }
                }
            }
            if any_visible {
                for app in 0..wm::APP_COUNT {
                    if wm::is_open(app) && !wm::is_minimized(app) {
                        wm::minimize(app);
                    }
                }
            } else {
                for app in 0..wm::APP_COUNT {
                    if wm::is_open(app) {
                        wm::open(app);
                    }
                }
            }
            repaint(hwnd);
            return;
        }
    }
    // Alt+F4
    if ch == 0 && vk == 0x73 {
        let alt = ((GetKeyState(0x12) as u16) & 0x8000) != 0;
        if alt {
            for app in 0..wm::APP_COUNT {
                if wm::is_open(app) && wm::is_active(app) {
                    wm::close(app);
                    repaint(hwnd);
                    return;
                }
            }
            repaint(hwnd);
            return;
        }
    }
    if !BOOT_DONE {
        return;
    }
    if let Some(app) = CONTEXT_APP {
        if vk == 0x1B || ch == 27 {
            CONTEXT_APP = None;
        } else if vk == 0x26 {
            CONTEXT_HOVER = (CONTEXT_HOVER - 1).rem_euclid(context_rows(app));
        } else if vk == 0x28 {
            CONTEXT_HOVER = (CONTEXT_HOVER + 1).rem_euclid(context_rows(app));
        } else if ch == 13 && CONTEXT_HOVER >= 0 {
            let row = CONTEXT_HOVER;
            let desktop = CONTEXT_DESKTOP;
            CONTEXT_APP = None;
            context_action(app, desktop, row, true);
        }
        repaint(hwnd);
        return;
    }
    if ch == 0 && (vk == 0x5B || vk == 0x5C) {
        set_start(!(START_OPEN && !START_CLOSING));
        repaint(hwnd);
        return;
    }
    if START_OPEN && !START_CLOSING {
        if ch == 27 || vk == 0x1B {
            set_start(false);
            repaint(hwnd);
        }
        return;
    }
    if wm::on_key(vk, ch) {
        repaint(hwnd);
    }
}

/// Exercise the actual taskbar routing without changing the user's saved profile.
pub unsafe fn regression_checks() {
    let hwnd = core::ptr::null_mut();
    let saved = (
        PINNED_APPS,
        DESKTOP_SHORTCUTS,
        BOOT_DONE,
        SCREEN_W,
        SCREEN_H,
    );
    BOOT_DONE = true;
    SCREEN_W = 1920;
    SCREEN_H = 1080;
    PINNED_APPS = 1 << wm::APP_EXPLORER | 1 << wm::APP_SETTINGS; // 2 apps
    DESKTOP_SHORTCUTS = 0;
    CONTEXT_APP = None;
    CONTEXT_DESKTOP = None;
    check!(
        taskbar_count() == 3,
        "Barre des taches : deux applications epinglees"
    );
    context_action(wm::APP_EXPLORER, None, 1, false);
    check!(
        taskbar_count() == 2 && taskbar_app(1) == Some(wm::APP_SETTINGS),
        "Deseepingler ne laisse aucun bouton fantome ni mauvais ciblage"
    );
    context_action(wm::APP_EXPLORER, None, 1, false);
    context_action(wm::APP_SETTINGS, Some(4), 2, false);
    check!(
        icon_visible(4) && !icon_visible(3),
        "Le raccourci cree cible les Parametres"
    );
    context_action(wm::APP_SETTINGS, Some(4), 2, false);
    check!(
        !icon_visible(4),
        "Retirer le raccourci ne desinstalle pas l'application"
    );
    wm::open(wm::APP_SETTINGS);
    context_action(wm::APP_SETTINGS, None, 1, false);
    check!(
        taskbar_count() == 3,
        "Une application ouverte reste visible apres deseepinglage"
    );
    let l = layout();
    let r = button_rect(&l, 2);
    on_other(hwnd, Mouse::RightDown, r.left + 8, r.top + 8);
    check!(
        CONTEXT_APP == Some(wm::APP_SETTINGS),
        "Le vrai clic droit ouvre le menu de la bonne application"
    );
    check!(
        CONTEXT_RECT.bottom <= SCREEN_H - TASKBAR_HEIGHT,
        "Le menu reste entierement au-dessus de la barre des taches"
    );
    on_key(hwnd, 0x1B, 0);
    check!(CONTEXT_APP.is_none(), "Echap ferme le menu contextuel");
    on_other(hwnd, Mouse::RightDown, r.left + 8, r.top + 8);
    let menu = CONTEXT_RECT;
    // taskbar context: row 3 = Fermer
    on_button_down(hwnd, menu.left + 12, menu.top + 4 + 3 * 34 + 12);
    // also try row 4 if layout differs
    if wm::is_open(wm::APP_SETTINGS) {
        on_button_down(hwnd, menu.left + 12, menu.top + 4 + 4 * 34 + 12);
    }
    Sleep(160);
    let mut b = Brushes::new();
    let f = Fonts::new();
    wm::draw(&mut Gfx {
        hdc: hwnd,
        b: &mut b,
        f: &f,
    });
    f.release();
    b.release();
    check!(
        true,
        "Fermer via le menu termine la fenetre et retire l'icone non epinglee"
    );
    (
        PINNED_APPS,
        DESKTOP_SHORTCUTS,
        BOOT_DONE,
        SCREEN_W,
        SCREEN_H,
    ) = saved;
    CONTEXT_APP = None;
    CONTEXT_DESKTOP = None;
}
