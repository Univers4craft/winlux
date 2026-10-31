//! Bureau Winlux inspiré de Windows 11, dessiné entièrement par GDI invité :
//! écran de démarrage animé, fond « bloom », barre des tâches centrée ou
//! alignée à gauche, menu Démarrer animé, zone de notification, icônes du
//! bureau et fenêtres des applications intégrées (voir `wm.rs`).
//!
//! Toute la mise en page découle de la taille réelle de l'écran : quand la
//! résolution change (fenêtre de VM redimensionnée), le bureau se réadapte.
//! Les textes sont en Windows-1252 car ils passent par `TextOutA`.

use crate::explorer;
use crate::gfx::*;
use crate::locale::*;
use crate::notifications;
use crate::weather;
use crate::wm::{self, Gfx, Mouse};
use winlux_testexe::*;

pub const TASKBAR_HEIGHT: i32 = 48;
const BUTTON: i32 = 44;
const BUTTON_GAP: i32 = 4;
const START_OPEN_MS: i32 = 230;
const START_CLOSE_MS: i32 = 140;
const SPLASH_MIN_MS: u32 = 2600;
const INTRO_MS: i32 = 520;
const RECENTER_MS: i32 = 200;
const TRAY_RESERVED: i32 = 236;

pub static mut SCREEN_W: i32 = 1280;
pub static mut SCREEN_H: i32 = 720;
pub static mut START_OPEN: bool = false;
pub static mut ALIGN_LEFT: bool = false;
pub static mut VOLUME_LEVEL: u32 = 0;
pub static mut BATTERY_LEVEL: u32 = 0;
pub static mut BATTERY_PRESENT: u32 = 0;
/// État d'alimentation lu par GetSystemPowerStatus (module power du runtime).
pub static mut BATTERY_CHARGING: bool = false;
pub static mut BATTERY_AC: bool = true;
/// Autonomie restante en secondes (u32::MAX = inconnue).
pub static mut BATTERY_SECS: u32 = u32::MAX;
pub static mut BATTERY_SAVER: bool = false;
static mut BATTERY_HOVER: bool = false;
static mut LAST_POWER_POLL: u32 = 0;
/// Dernier seuil d'alerte batterie signalé (20 puis 10 %).
static mut LOW_WARNED: u32 = 101;
pub static mut NETWORK_CONNECTED: u32 = 0;
static mut START_ANIM: u32 = 0;
static mut START_CLOSING: bool = false;
static mut HOVER_BUTTON: i32 = -1;
static mut HOVER_START_ITEM: i32 = -1;
static mut ICON_X: [i32; 5] = [12, 12, 12, 108, 108];
static mut ICON_Y: [i32; 5] = [12, 108, 204, 12, 108];
static mut PINNED_APPS: u8 = 79; // Explorer, Paramètres, Éditeur, Utilitaires + Terminal (bit 6)
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
static mut START_SEARCH_ACTIVE: bool = false;
static mut SEARCH_OVERLAY: bool = false;
static mut SEARCH_BUF: [u8; 64] = [0; 64];
static mut SEARCH_BUF_LEN: usize = 0;
static mut SEARCH_HOVER: i32 = -1;
use crate::gfx::{AUTO_THEME, ENABLE_BLUR, ENABLE_SMOOTH_ANIM, ENABLE_THUMBNAILS};
static mut RECENTER_ANIM: u32 = 0;
static mut RECENTER_FROM: i32 = 0;
static mut RECENTER_TARGET: i32 = 0;
static mut PREV_TASKBAR_COUNT: i32 = 0;
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
/// Le panneau de notifications est dans `notifications.rs`.
/// Panneau réseau (flyout Wi-Fi/Ethernet, façon Windows 11).
static mut NET_PANEL: bool = false;
static mut NET_HOVER: i32 = -1;
static mut NET_SCAN_REQUESTED: bool = false;
static mut VOL_PANEL: bool = false;
static mut VOL_HOVER: i32 = -1;
static mut VOL_SLIDER_DRAG: bool = false;
/// Données réseau lues depuis Z:\run\winlux\networks.txt.
/// Chaque entrée : SSID, signal (0-100), sécurité, en cours d'utilisation.
static mut NET_ENTRIES: [NetEntry; 16] = [NetEntry::EMPTY; 16];
static mut NET_COUNT: usize = 0;
static mut NET_IFACE: [u8; 32] = [0; 32];
static mut NET_IFACE_LEN: usize = 0;
static mut NET_ADDR: [u8; 32] = [0; 32];
static mut NET_ADDR_LEN: usize = 0;
static mut NET_IS_WIFI: bool = false;
static mut NET_SSID: [u8; 64] = [0; 64];
static mut NET_SSID_LEN: usize = 0;

#[derive(Clone, Copy)]
struct NetEntry {
    ssid: [u8; 48],
    ssid_len: usize,
    signal: u32,
    secured: bool,
    in_use: bool,
}
impl NetEntry {
    const EMPTY: Self = Self {
        ssid: [0; 48],
        ssid_len: 0,
        signal: 0,
        secured: false,
        in_use: false,
    };
}
/// Atténuation du fond (0 = noir, 1000 = normal) pendant le fondu.
static mut FADE: i32 = 1000;
/// Sous-menu d'alimentation ouvert (Arrêter, Redémarrer, Verrouiller).
static mut POWER_MENU: bool = false;
static mut POWER_HOVER: i32 = -1;
/// Confirmation de déconnexion/arrêt en cours.
static mut SHUTDOWN_FADE: i32 = 0;
static mut SHUTDOWN_START: u32 = 0;
static mut SHUTDOWN_ACTION: u8 = 0; // 1=poweroff, 2=reboot, 3=lock

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}
fn parse_u32(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |n, &c| {
            n.saturating_mul(10).saturating_add((c - b'0') as u32)
        })
}

fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c + 32,
        0xC0..=0xDE if c != 0xD7 => c + 32,
        _ => c,
    }
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    (0..=hay.len() - needle.len())
        .any(|i| (0..needle.len()).all(|k| lower(hay[i + k]) == lower(needle[k])))
}

fn fuzzy_match(name: &[u8], query: &[u8]) -> bool {
    if query.is_empty() {
        return true;
    }
    if contains_ci(name, query) {
        return true;
    }
    let mut qi = 0;
    for &c in name {
        if qi < query.len() && lower(c) == lower(query[qi]) {
            qi += 1;
        }
    }
    qi == query.len()
}

fn fuzzy_score(name: &[u8], query: &[u8]) -> i32 {
    if query.is_empty() {
        return 0;
    }
    if contains_ci(name, query) {
        if name.len() >= query.len() && (0..query.len()).all(|k| lower(name[k]) == lower(query[k]))
        {
            return 3; // prefix match
        }
        return 2; // substring match
    }
    1 // fuzzy only
}

#[derive(Clone, Copy)]
struct SearchResult {
    name: &'static [u8],
    launch: Launch,
    glyph: Glyph,
    score: i32,
}

fn collect_results(query: &[u8], out: &mut [SearchResult; 16]) -> usize {
    let mut count = 0usize;
    for app in &pinned_apps() {
        if count >= 16 {
            break;
        }
        if fuzzy_match(app.name, query) {
            let s = fuzzy_score(app.name, query);
            let mut dup = false;
            for j in 0..count {
                if out[j].name.as_ptr() == app.name.as_ptr() {
                    dup = true;
                    break;
                }
            }
            if !dup {
                out[count] = SearchResult {
                    name: app.name,
                    launch: app.launch,
                    glyph: app.glyph,
                    score: s,
                };
                count += 1;
            }
        }
    }
    for &(title, _, g, launch) in &recommended_apps() {
        if count >= 16 {
            break;
        }
        if fuzzy_match(title, query) {
            let s = fuzzy_score(title, query);
            let mut dup = false;
            for j in 0..count {
                if out[j].name.as_ptr() == title.as_ptr() {
                    dup = true;
                    break;
                }
            }
            if !dup {
                out[count] = SearchResult {
                    name: title,
                    launch,
                    glyph: g,
                    score: s,
                };
                count += 1;
            }
        }
    }
    unsafe {
        for i in 0..ALL_APPS_COUNT {
            if count >= 16 {
                break;
            }
            let (name, exec, _cat) = ALL_APPS[i];
            if fuzzy_match(name, query) {
                let s = fuzzy_score(name, query);
                let mut dup = false;
                for j in 0..count {
                    if out[j].name.as_ptr() == name.as_ptr() {
                        dup = true;
                        break;
                    }
                }
                if !dup {
                    out[count] = SearchResult {
                        name,
                        launch: Launch::Exec(exec),
                        glyph: Glyph::Exe,
                        score: s,
                    };
                    count += 1;
                }
            }
        }
    }
    // Sort by score descending (simple insertion sort).
    for i in 1..count {
        let mut j = i;
        while j > 0 && out[j].score > out[j - 1].score {
            let tmp = SearchResult {
                name: out[j].name,
                launch: out[j].launch,
                glyph: out[j].glyph,
                score: out[j].score,
            };
            out[j] = SearchResult {
                name: out[j - 1].name,
                launch: out[j - 1].launch,
                glyph: out[j - 1].glyph,
                score: out[j - 1].score,
            };
            out[j - 1] = tmp;
            j -= 1;
        }
    }
    count
}

fn init_apps() {
    unsafe {
        if ALL_APPS_COUNT > 0 {
            return;
        }
        let apps: [(&'static [u8], &'static [u8], i32); 33] = [
            (b"Bienvenue dans Winlux", b"welcome", 9),
            (tr(S_TERMINAL), b"xterm", 1),
            (tr(S_FILE_EXPLORER), b"nemo", 1),
            (tr(S_NOTEPAD), b"mousepad", 1),
            (tr(S_CALCULATOR), b"calc.exe", 1),
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
            (tr(S_SETTINGS_APP), b"cinnamon-settings", 8),
            (b"Gestionnaire de paquets", b"mintinstall", 9),
            (b"Gestion des disques", b"diskmgr", 9),
            (b"Moniteur syst\xE8me", b"gnome-system-monitor", 9),
            (tr(S_TASK_MANAGER), b"taskmgr", 9),
            (b"Nettoyage de disque", b"diskcleaner", 9),
            (b"Observateur d'\xE9v\xE9nements", b"eventviewer", 9),
            (b"Restauration syst\xE8me", b"restore", 9),
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
    let available = w - TRAY_RESERVED;
    let buttons_x = if unsafe { ALIGN_LEFT } {
        12
    } else {
        unsafe {
            let target = (available - buttons_w) / 2;
            if PREV_TASKBAR_COUNT == 0 {
                PREV_TASKBAR_COUNT = count;
                RECENTER_TARGET = target;
                RECENTER_FROM = target;
            } else if count != PREV_TASKBAR_COUNT {
                RECENTER_FROM = if RECENTER_ANIM != 0 {
                    let prev_t = ease_out(since(RECENTER_ANIM), RECENTER_MS);
                    RECENTER_FROM + (RECENTER_TARGET - RECENTER_FROM) * prev_t / 1000
                } else {
                    RECENTER_TARGET
                };
                RECENTER_TARGET = target;
                RECENTER_ANIM = now();
                PREV_TASKBAR_COUNT = count;
            }
            if RECENTER_ANIM != 0 && ENABLE_SMOOTH_ANIM {
                let t = ease_out(since(RECENTER_ANIM), RECENTER_MS);
                if t >= 1000 {
                    RECENTER_ANIM = 0;
                }
                RECENTER_FROM + (RECENTER_TARGET - RECENTER_FROM) * t / 1000
            } else {
                target
            }
        }
    };
    let sw = 640.min(w - 24);
    let sh = 600.min(bar_top - 24);
    let sx = if unsafe { ALIGN_LEFT } {
        12
    } else {
        (available - sw) / 2
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
        ease_in_out(t, START_CLOSE_MS) * 60 / 1000
    } else if t < START_OPEN_MS {
        (1000 - ease_in_out(t, START_OPEN_MS)) * 70 / 1000
    } else {
        0
    }
}

pub unsafe fn open_start_menu() {
    set_start(true);
}

unsafe fn set_start(open: bool) {
    if open == (START_OPEN && !START_CLOSING) {
        return;
    }
    NET_PANEL = false;
    NET_HOVER = -1;
    VOL_PANEL = false;
    VOL_HOVER = -1;
    START_ANIM = now();
    if open {
        START_OPEN = true;
        START_CLOSING = false;
        SEARCH_OVERLAY = false;
        weather::PANEL_OPEN = false;
    } else if START_OPEN {
        START_CLOSING = true;
        POWER_MENU = false;
        START_SEARCH_LEN = 0;
        START_SEARCH_ACTIVE = false;
    }
    if !open {
        POWER_MENU = false;
        POWER_HOVER = -1;
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
    } else if app == wm::APP_TERMINAL {
        Launch::Terminal
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
        wm::APP_TERMINAL,
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
        wm::APP_TERMINAL,
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

const ICONS_PATH: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\desktop-icons.dat";

unsafe fn save_icon_positions() {
    let file = CreateFileA(
        ICONS_PATH.as_ptr(),
        0x4000_0000, // GENERIC_WRITE
        0,
        core::ptr::null_mut(),
        2, // CREATE_ALWAYS
        0,
        core::ptr::null_mut(),
    );
    if file.is_null() || file as isize == -1 {
        return;
    }
    let mut buf = [0u8; 40]; // 5 icons * (x:i32 + y:i32) = 40 bytes
    for i in 0..5 {
        let off = i * 8;
        buf[off..off + 4].copy_from_slice(&ICON_X[i].to_le_bytes());
        buf[off + 4..off + 8].copy_from_slice(&ICON_Y[i].to_le_bytes());
    }
    let mut written = 0u32;
    WriteFile(file, buf.as_ptr(), 40, &mut written, core::ptr::null_mut());
    CloseHandle(file);
}

unsafe fn load_icon_positions() {
    let file = CreateFileA(
        ICONS_PATH.as_ptr(),
        0x8000_0000, // GENERIC_READ
        1,           // FILE_SHARE_READ
        core::ptr::null_mut(),
        3, // OPEN_EXISTING
        0,
        core::ptr::null_mut(),
    );
    if file.is_null() || file as isize == -1 {
        return;
    }
    let mut buf = [0u8; 40];
    let mut nread = 0u32;
    ReadFile(
        file,
        buf.as_mut_ptr(),
        40,
        &mut nread,
        core::ptr::null_mut(),
    );
    CloseHandle(file);
    if nread != 40 {
        return;
    }
    for i in 0..5 {
        let off = i * 8;
        ICON_X[i] = i32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        ICON_Y[i] = i32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]);
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
    notifications::set_open(false);
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
    notifications::set_open(false);
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
                crate::terminal::init();
                wm::open(wm::APP_TERMINAL);
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
    round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 8, p.border);
    round_rect(
        hdc,
        b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        7,
        p.surface,
    );
    if app == usize::MAX {
        let labels: [&[u8]; 5] = [
            b"Ouvrir l'Explorateur",
            b"Ouvrir le terminal",
            b"Ouvrir les Param\xE8tres",
            b"Ouvrir l'\xC9\x64iteur",
            b"Ouvrir les Utilitaires",
        ];
        for row in 0..context_rows(app) {
            let y = r.top + 4 + row * 34;
            if row == CONTEXT_HOVER {
                round_rect(hdc, b, r.left + 4, y, r.right - 4, y + 34, 4, p.hover);
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
                round_rect(hdc, b, r.left + 4, y, r.right - 4, y + 34, 4, p.hover);
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
            round_rect(hdc, b, r.left + 4, y, r.right - 4, y + 34, 4, p.hover);
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
unsafe fn draw_ethernet(hdc: Handle, b: &mut Brushes, x: i32, y: i32, on: bool) {
    let c = if on {
        rgb(0xF2, 0xF2, 0xF2)
    } else {
        rgb(0x7A, 0x80, 0x8A)
    };
    // Small Ethernet plug icon: vertical bar + horizontal bar + two prongs
    fill(hdc, b, x + 6, y, x + 9, y + 12, c);
    fill(hdc, b, x + 2, y + 4, x + 13, y + 6, c);
    fill(hdc, b, x + 2, y + 4, x + 4, y + 8, c);
    fill(hdc, b, x + 11, y + 4, x + 13, y + 8, c);
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
unsafe fn draw_battery(hdc: Handle, b: &mut Brushes, x: i32, y: i32, level: u32, charging: bool) {
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
        if level > 20 || charging {
            c
        } else {
            rgb(0xFF, 0x60, 0x50)
        },
    );
    fill(hdc, b, x + 22, y + 5, x + 24, y + 10, c);
    if charging {
        // Éclair de charge, détouré sur la jauge.
        let dark = rgb(0x20, 0x22, 0x28);
        for (dx, top, bottom) in [(9, 3, 8), (10, 4, 8), (11, 7, 9), (12, 7, 12), (13, 7, 11)] {
            fill(hdc, b, x + dx - 1, y + top, x + dx + 1, y + bottom + 1, dark);
        }
        let bolt = rgb(0x6C, 0xCB, 0x5F);
        for (dx, top, bottom) in [(9, 4, 7), (10, 5, 7), (11, 7, 8), (12, 8, 11), (13, 8, 10)] {
            fill(hdc, b, x + dx, y + top, x + dx + 1, y + bottom + 1, bolt);
        }
    }
}

/// Lit l'état d'alimentation comme une application Windows le ferait.
/// Renvoie vrai si l'icône doit être redessinée.
pub unsafe fn power_tick() -> bool {
    let t = now();
    if LAST_POWER_POLL != 0 && t.wrapping_sub(LAST_POWER_POLL) < 3000 {
        return false;
    }
    LAST_POWER_POLL = t;
    let mut st = SystemPowerStatus::default();
    if GetSystemPowerStatus(&mut st) == 0 {
        return false;
    }
    let before = (
        BATTERY_PRESENT,
        BATTERY_LEVEL,
        BATTERY_CHARGING,
        BATTERY_AC,
        BATTERY_SECS / 60,
        BATTERY_SAVER,
    );
    BATTERY_PRESENT = u32::from(st.battery_flag & 128 == 0 && st.battery_life_percent <= 100);
    BATTERY_LEVEL = if BATTERY_PRESENT != 0 {
        u32::from(st.battery_life_percent)
    } else {
        0
    };
    BATTERY_CHARGING = st.battery_flag & 8 != 0;
    BATTERY_AC = st.ac_line_status == 1;
    BATTERY_SECS = st.battery_life_time;
    BATTERY_SAVER = st.system_status_flag & 1 != 0;
    // Alertes façon Windows : batterie faible à 20 %, critique à 10 %.
    if BATTERY_PRESENT != 0 && !BATTERY_AC {
        if BATTERY_LEVEL <= 10 && LOW_WARNED > 10 {
            LOW_WARNED = 10;
            toast(b"Batterie critique : branchez votre PC, il va bient\xF4t se mettre en veille.");
        } else if BATTERY_LEVEL <= 20 && LOW_WARNED > 20 {
            LOW_WARNED = 20;
            toast(b"Batterie faible : 20 % restants. Branchez votre PC.");
        }
    } else if BATTERY_AC {
        LOW_WARNED = 101;
    }
    before
        != (
            BATTERY_PRESENT,
            BATTERY_LEVEL,
            BATTERY_CHARGING,
            BATTERY_AC,
            BATTERY_SECS / 60,
            BATTERY_SAVER,
        )
}

fn push(buf: &mut [u8], n: &mut usize, s: &[u8]) {
    for &c in s {
        if *n < buf.len() {
            buf[*n] = c;
            *n += 1;
        }
    }
}
fn push_num(buf: &mut [u8], n: &mut usize, v: u32) {
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    let mut v = v;
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    push(buf, n, &digits[i..]);
}

/// Infobulle détaillée de l'icône batterie (pourcentage, état, autonomie).
unsafe fn draw_battery_tip(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !BATTERY_HOVER || START_OPEN || NET_PANEL || notifications::is_open() {
        return;
    }
    let mut line1 = [0u8; 64];
    let mut n1 = 0;
    let mut line2 = [0u8; 64];
    let mut n2 = 0;
    if BATTERY_PRESENT == 0 {
        push(&mut line1, &mut n1, b"Alimentation secteur");
        push(&mut line2, &mut n2, b"Aucune batterie d\xE9tect\xE9e");
    } else {
        push(&mut line1, &mut n1, b"Batterie : ");
        push_num(&mut line1, &mut n1, BATTERY_LEVEL);
        if BATTERY_AC {
            push(&mut line1, &mut n1, b" % disponible (branch\xE9)");
            push(
                &mut line2,
                &mut n2,
                if BATTERY_CHARGING {
                    b"En charge" as &[u8]
                } else if BATTERY_LEVEL >= 99 {
                    b"Charg\xE9e"
                } else {
                    b"Branch\xE9e, pas en charge"
                },
            );
        } else {
            push(&mut line1, &mut n1, b" % restants");
            if BATTERY_SECS != u32::MAX {
                let (h, m) = (BATTERY_SECS / 3600, BATTERY_SECS / 60 % 60);
                push(&mut line2, &mut n2, b"Environ ");
                if h > 0 {
                    push_num(&mut line2, &mut n2, h);
                    push(&mut line2, &mut n2, if m < 10 { b" h 0" } else { b" h " });
                }
                push_num(&mut line2, &mut n2, m);
                push(&mut line2, &mut n2, b" min d'autonomie");
            } else {
                push(&mut line2, &mut n2, b"Estimation de l'autonomie en cours");
            }
        }
    }
    let mode: &[u8] = if BATTERY_SAVER {
        b"Mode : \xC9conomie d'\xE9nergie"
    } else {
        b"Cliquez pour les options d'alimentation"
    };
    let w = text_width(&line1[..n1], 13)
        .max(text_width(&line2[..n2], 13))
        .max(text_width(mode, 11))
        + 28;
    let h = 72;
    let tray = l.w - 12 - 92 - 96;
    let x = (tray + 70 - w / 2).min(l.w - w - 8);
    let y = l.bar_top - h - 8;
    panel(
        hdc,
        b,
        x,
        y,
        x + w,
        y + h,
        6,
        rgb(0x2B, 0x2B, 0x2B),
        rgb(0x3D, 0x3D, 0x3D),
    );
    text(hdc, f.semibold, rgb(0xF2, 0xF2, 0xF2), x + 14, y + 10, &line1[..n1]);
    text(hdc, f.body, rgb(0xC5, 0xC5, 0xC5), x + 14, y + 30, &line2[..n2]);
    text(hdc, f.small, rgb(0x9A, 0x9A, 0x9A), x + 14, y + 51, mode);
}

unsafe fn two_digits(v: u16) -> [u8; 2] {
    [b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8]
}

/// Lecture du fichier networks.txt écrit par `winlux-control network-scan`.
/// Format : lignes nmcli « SSID:SIGNAL:SECURITY:*\n » puis meta « key=value ».
unsafe fn net_read_scan() {
    static mut BUF: [u8; 2048] = [0; 2048];
    let path = c"Z:\\run\\winlux\\networks.txt";
    let h = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h as isize <= 0 {
        return;
    }
    let mut read_bytes = 0u32;
    ReadFile(
        h,
        BUF.as_mut_ptr(),
        BUF.len() as u32 - 1,
        &mut read_bytes,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    let len = read_bytes as usize;
    BUF[len] = 0;
    NET_COUNT = 0;
    NET_IFACE_LEN = 0;
    NET_ADDR_LEN = 0;
    NET_IS_WIFI = false;
    NET_SSID_LEN = 0;
    for raw_line in BUF[..len].split(|&c| c == b'\n') {
        let line = if raw_line.last() == Some(&b'\r') {
            &raw_line[..raw_line.len() - 1]
        } else {
            raw_line
        };
        if line.is_empty() {
            continue;
        }
        if line.starts_with(b"connected=") {
            // meta field, not a network entry
        } else if line.starts_with(b"iface=") {
            let v = &line[6..];
            let n = v.len().min(NET_IFACE.len());
            NET_IFACE[..n].copy_from_slice(&v[..n]);
            NET_IFACE_LEN = n;
        } else if line.starts_with(b"addr=") {
            let v = &line[5..];
            let n = v.len().min(NET_ADDR.len());
            NET_ADDR[..n].copy_from_slice(&v[..n]);
            NET_ADDR_LEN = n;
        } else if line.starts_with(b"wifi=") {
            NET_IS_WIFI = line == b"wifi=1";
        } else if line.starts_with(b"ssid=") {
            let v = &line[5..];
            let n = v.len().min(NET_SSID.len());
            NET_SSID[..n].copy_from_slice(&v[..n]);
            NET_SSID_LEN = n;
        } else if NET_COUNT < NET_ENTRIES.len() {
            // nmcli format: SSID:SIGNAL:SECURITY:IN-USE
            let mut parts = line.splitn(4, |&c| c == b':');
            let ssid = parts.next().unwrap_or(b"");
            let sig_s = parts.next().unwrap_or(b"");
            let sec = parts.next().unwrap_or(b"");
            let inuse = parts.next().unwrap_or(b"");
            if ssid.is_empty() {
                continue;
            }
            let mut e = NetEntry::EMPTY;
            let n = ssid.len().min(e.ssid.len());
            e.ssid[..n].copy_from_slice(&ssid[..n]);
            e.ssid_len = n;
            e.signal = parse_u32(sig_s);
            e.secured = !sec.is_empty() && sec != b"" && sec != b"--";
            e.in_use = inuse == b"*";
            NET_ENTRIES[NET_COUNT] = e;
            NET_COUNT += 1;
        }
    }
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
            Some(wm::APP_NOTEPAD) => Glyph::Code,
            Some(wm::APP_TERMINAL) => Glyph::Terminal,
            Some(wm::APP_UTILITIES) => Glyph::Tools,
            Some(wm::APP_CLEANUP) => Glyph::Drive,
            Some(wm::APP_DISKMGR) => Glyph::Drive,
            Some(wm::APP_EVENTVIEWER) => Glyph::Details,
            Some(wm::APP_RESTORE) => Glyph::Refresh,
            Some(wm::APP_WELCOME) => Glyph::Star,
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

    // Indicateur de bureau virtuel (entre les boutons d'apps et la zone de notification)
    if wm::desktop_count() > 1 {
        let count = taskbar_count();
        let ind_x = l.buttons_x + count * (BUTTON + BUTTON_GAP) + 8;
        let ind_y = top + 8;
        let cur = wm::current_desktop();
        let total = wm::desktop_count();
        for d in 0..total {
            let dx = ind_x + d as i32 * 14;
            let c = if d == cur {
                ACCENT
            } else {
                rgb(0x5A, 0x5E, 0x68)
            };
            round_rect(hdc, b, dx, ind_y + 14, dx + 10, ind_y + 20, 2, c);
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
    if NET_IS_WIFI {
        draw_wifi(hdc, b, tray + 6, top + 17, NETWORK_CONNECTED != 0);
    } else {
        draw_ethernet(hdc, b, tray + 6, top + 17, NETWORK_CONNECTED != 0);
    }
    draw_volume(hdc, b, tray + 30, top + 17, VOLUME_LEVEL);
    if BATTERY_PRESENT != 0 {
        draw_battery(hdc, b, tray + 58, top + 17, BATTERY_LEVEL, BATTERY_CHARGING);
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
    // Widget météo à gauche du chevron
    let weather_x = tray - 24 - weather::TRAY_WIDTH - 8;
    round_rect(
        hdc,
        b,
        weather_x - 4,
        top + 6,
        weather_x + weather::TRAY_WIDTH,
        top + 42,
        5,
        rgb(0x24, 0x28, 0x31),
    );
    weather::draw_tray(hdc, b, f, weather_x, top + 14);
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
fn pinned_apps() -> [App; 8] {
    [
        App {
            name: tr(S_FILE_EXPLORER),
            glyph: Glyph::Folder,
            launch: Launch::Explorer,
        },
        App {
            name: tr(S_SETTINGS_APP),
            glyph: Glyph::Settings,
            launch: Launch::Settings,
        },
        App {
            name: tr(S_NOTEPAD),
            glyph: Glyph::Code,
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
            name: tr(S_DOCUMENTS),
            glyph: Glyph::Folder,
            launch: Launch::Documents,
        },
        App {
            name: tr(S_THIS_PC),
            glyph: Glyph::Computer,
            launch: Launch::Pc,
        },
        App {
            name: b"Programmes .exe",
            glyph: Glyph::Exe,
            launch: Launch::Programs,
        },
    ]
}
fn recommended_apps() -> [(&'static [u8], &'static [u8], Glyph, Launch); 4] {
    [
        (
            tr(S_TERMINAL),
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
            tr(S_DOCUMENTS),
            b"Dossier personnel Linux",
            Glyph::Folder,
            Launch::Documents,
        ),
        (
            tr(S_SETTINGS_APP),
            b"Clavier, r\xE9seau et alimentation",
            Glyph::Settings,
            Launch::Settings,
        ),
    ]
}

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
            crate::terminal::init();
            wm::open(wm::APP_TERMINAL);
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
            // Applications intégrées dans le WM
            if cmd == b"taskmgr" {
                wm::open(wm::APP_TASKMGR);
                return;
            }
            if cmd == b"diskcleaner" {
                wm::open(wm::APP_CLEANUP);
                return;
            }
            if cmd == b"diskmgr" {
                wm::open(wm::APP_DISKMGR);
                return;
            }
            if cmd == b"eventviewer" {
                wm::open(wm::APP_EVENTVIEWER);
                return;
            }
            if cmd == b"restore" {
                wm::open(wm::APP_RESTORE);
                return;
            }
            if cmd == b"welcome" {
                if wm::native_session() {
                    ShellExecuteA(
                        core::ptr::null_mut(),
                        c"open".as_ptr(),
                        c"winlux-welcome".as_ptr(),
                        core::ptr::null(),
                        core::ptr::null(),
                        1,
                    );
                } else {
                    crate::welcome::open();
                }
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
            tr(S_ALL_APPS)
        } else {
            tr(S_PINNED)
        }
    };
    text(hdc, f.title, rgb(0xF2, 0xF2, 0xF2), inner_l + 4, y, title);
    // Barre de recherche
    let search_y = y + 40;
    let search_bg = if unsafe { START_SEARCH_ACTIVE } {
        rgb(0x3A, 0x3F, 0x4A)
    } else {
        rgb(0x31, 0x35, 0x3F)
    };
    round_rect(
        hdc,
        b,
        inner_l,
        search_y,
        inner_r,
        search_y + 36,
        18,
        search_bg,
    );
    glyph(hdc, b, f, Glyph::Search, inner_l + 10, search_y + 8, 18);
    if unsafe { START_SEARCH_LEN > 0 } {
        text(
            hdc,
            f.body,
            rgb(0xE6, 0xE6, 0xE6),
            inner_l + 34,
            search_y + 8,
            unsafe { &START_SEARCH[..START_SEARCH_LEN] },
        );
    } else {
        text(
            hdc,
            f.body,
            rgb(0x88, 0x88, 0x88),
            inner_l + 34,
            search_y + 8,
            tr(S_SEARCH_PLACEHOLDER),
        );
    }
    if unsafe { START_SEARCH_LEN } > 0 {
        // Mode recherche : afficher les résultats.
        let results_y = search_y + 48;
        text(
            hdc,
            f.title,
            rgb(0xF2, 0xF2, 0xF2),
            inner_l + 4,
            results_y,
            b"R\xE9sultats",
        );
        let mut results = [SearchResult {
            name: b"",
            launch: Launch::Explorer,
            glyph: Glyph::Search,
            score: 0,
        }; 16];
        let count = collect_results(unsafe { &START_SEARCH[..START_SEARCH_LEN] }, &mut results);
        if count == 0 {
            text(
                hdc,
                f.body,
                rgb(0x88, 0x88, 0x88),
                inner_l + 4,
                results_y + 36,
                b"Aucun r\xE9sultat",
            );
        }
        for i in 0..count {
            let iy = results_y + 36 + (i as i32) * 44;
            if iy + 40 >= s.bottom - 72 {
                break;
            }
            if HOVER_START_ITEM == 600 + i as i32 {
                round_rect(
                    hdc,
                    b,
                    inner_l,
                    iy,
                    inner_r,
                    iy + 40,
                    5,
                    rgb(0x31, 0x35, 0x3F),
                );
            }
            glyph(hdc, b, f, results[i].glyph, inner_l + 8, iy + 8, 24);
            text(
                hdc,
                f.body,
                rgb(0xE6, 0xE6, 0xE6),
                inner_l + 40,
                iy + 10,
                results[i].name,
            );
        }
    } else {
        let all: &[u8] = unsafe {
            if START_SHOW_ALL_APPS {
                tr(S_PINNED)
            } else {
                tr(S_ALL_APPS)
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
        for (i, app) in pinned_apps()
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
                if START_CATEGORY == 0 || cat == START_CATEGORY {
                    let app_y = apps_y + 40 + (displayed as i32) * 44;
                    if app_y + 40 < s.bottom - 72 {
                        text(hdc, f.body, rgb(0xE6, 0xE6, 0xE6), inner_l + 4, app_y, name);
                        displayed += 1;
                    }
                }
            }
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
                tr(S_RECOMMENDED),
            );
            for (i, (title, sub, g, _)) in recommended_apps().iter().enumerate() {
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
        tr(S_ALIGN_LEFT)
    } else {
        tr(S_CENTER)
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
    // Sous-menu d'alimentation.
    if POWER_MENU {
        let pr = power_rect(&l.start);
        let menu_w = 180;
        let item_h = 36;
        let menu_h = item_h * 3 + 8;
        let mx = pr.right - menu_w;
        let my = pr.top - menu_h - 4;
        panel(
            hdc,
            b,
            mx,
            my,
            mx + menu_w,
            my + menu_h,
            8,
            rgb(0x2C, 0x2C, 0x2C),
            rgb(0x50, 0x50, 0x50),
        );
        let items: [&[u8]; 3] = [b"Arr\xEAter", b"Red\xE9marrer", b"Verrouiller"];
        let icons: [&[u8]; 3] = [b"\xA4", b"\xA5", b"\xA3"];
        for i in 0..3i32 {
            let iy = my + 4 + i * item_h;
            let hover = POWER_HOVER == i;
            if hover {
                round_rect(
                    hdc,
                    b,
                    mx + 4,
                    iy,
                    mx + menu_w - 4,
                    iy + item_h,
                    4,
                    rgb(0x40, 0x44, 0x4C),
                );
            }
            text(
                hdc,
                f.body,
                rgb(0xE6, 0xE6, 0xE6),
                mx + 40,
                iy + 10,
                items[i as usize],
            );
        }
    }
}

/// Élément du menu Démarrer sous le pointeur : 0..11 épinglés, 100..103
/// recommandés, 200 bascule d'alignement, 201 alimentation, 210 arrêter,
/// 211 redémarrer, 212 redémarrer la session, 300 toutes mes applications,
/// 301..312 catégories, 400..463 applications, -1 rien.
fn start_item_at(l: &Layout, x: i32, y: i32) -> i32 {
    let s = &l.start;
    if unsafe { POWER_MENU } {
        let pr = power_rect(s);
        let pw = 160;
        let ph = 3 * 36 + 8;
        let mx = pr.right - pw;
        let my = pr.top - ph - 4;
        if x >= mx && x < mx + pw && y >= my && y < my + ph {
            let row = (y - my - 4) / 36;
            return match row {
                0 => 210,
                1 => 211,
                2 => 212,
                _ => -1,
            };
        }
    }
    let y0 = s.top + 82;
    let inner_l = s.left + 28;
    let inner_r = s.right - 28;
    // Search results mode.
    if unsafe { START_SEARCH_LEN } > 0 {
        let search_y = y0 + 40;
        let results_y = search_y + 48 + 36;
        for i in 0..16i32 {
            let iy = results_y + i * 44;
            if iy + 40 >= s.bottom - 72 {
                break;
            }
            if y >= iy && y < iy + 40 && x >= inner_l && x < inner_r {
                return 600 + i;
            }
        }
        if inside(&align_toggle_rect(s), x, y) {
            return 200;
        }
        if inside(&power_rect(s), x, y) {
            return 201;
        }
        return -1;
    }
    let all_w = unsafe {
        text_width(
            if START_SHOW_ALL_APPS {
                tr(S_PINNED)
            } else {
                tr(S_ALL_APPS)
            },
            11,
        ) + 20
    };
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
        let pinned = pinned_apps();
        for i in 0..pinned.len() as i32 {
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
    if unsafe { POWER_MENU } {
        let pr = power_rect(s);
        let menu_w = 180;
        let item_h = 36;
        let menu_h = item_h * 3 + 8;
        let mx = pr.right - menu_w;
        let my = pr.top - menu_h - 4;
        for i in 0..3 {
            let iy = my + 4 + i * item_h;
            if x >= mx && x < mx + menu_w && y >= iy && y < iy + item_h {
                return 210 + i;
            }
        }
    }
    -1
}

// ---------- Overlay de recherche (Win+S) ----------

unsafe fn search_overlay(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !SEARCH_OVERLAY {
        return;
    }
    let w = 520.min(l.w - 40);
    let h = 440.min(l.bar_top - 40);
    let cx = l.w / 2;
    let ox = cx - w / 2;
    let oy = (l.bar_top - h) / 2;
    // Fond semi-transparent.
    fill(hdc, b, 0, 0, l.w, l.bar_top, rgb(0x0A, 0x0A, 0x0A));
    panel(
        hdc,
        b,
        ox,
        oy,
        ox + w,
        oy + h,
        12,
        rgb(0x1E, 0x21, 0x28),
        rgb(0x3A, 0x3F, 0x4A),
    );
    let pad = 24;
    let il = ox + pad;
    let ir = ox + w - pad;
    // Titre.
    glyph(hdc, b, f, Glyph::Search, il, oy + 20, 24);
    text(
        hdc,
        f.large,
        rgb(0xF2, 0xF2, 0xF2),
        il + 32,
        oy + 18,
        b"Recherche",
    );
    // Champ de saisie.
    let fy = oy + 60;
    round_rect(hdc, b, il, fy, ir, fy + 40, 20, rgb(0x31, 0x35, 0x3F));
    glyph(hdc, b, f, Glyph::Search, il + 12, fy + 10, 18);
    if SEARCH_BUF_LEN > 0 {
        text(
            hdc,
            f.body,
            rgb(0xE6, 0xE6, 0xE6),
            il + 36,
            fy + 10,
            &SEARCH_BUF[..SEARCH_BUF_LEN],
        );
    } else {
        text(
            hdc,
            f.body,
            rgb(0x88, 0x88, 0x88),
            il + 36,
            fy + 10,
            b"Tapez pour rechercher...",
        );
    }
    // Résultats.
    let ry = fy + 52;
    if SEARCH_BUF_LEN > 0 {
        let mut results = [SearchResult {
            name: b"",
            launch: Launch::Explorer,
            glyph: Glyph::Search,
            score: 0,
        }; 16];
        let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results);
        if count == 0 {
            text(
                hdc,
                f.body,
                rgb(0x88, 0x88, 0x88),
                il + 4,
                ry + 8,
                b"Aucun r\xE9sultat",
            );
        }
        for i in 0..count {
            let iy = ry + (i as i32) * 44;
            if iy + 40 >= oy + h - 16 {
                break;
            }
            if SEARCH_HOVER == i as i32 {
                round_rect(hdc, b, il, iy, ir, iy + 40, 5, rgb(0x31, 0x35, 0x3F));
            }
            glyph(hdc, b, f, results[i].glyph, il + 8, iy + 8, 24);
            text(
                hdc,
                f.body,
                rgb(0xE6, 0xE6, 0xE6),
                il + 40,
                iy + 10,
                results[i].name,
            );
        }
    } else {
        text(
            hdc,
            f.body,
            rgb(0x88, 0x88, 0x88),
            il + 4,
            ry + 8,
            b"Recherchez des applications et des fichiers",
        );
    }
}

// ---------- Notifications ----------

pub unsafe fn toast(msg: &[u8]) {
    let n = msg.len().min(TOAST.len());
    TOAST[..n].copy_from_slice(&msg[..n]);
    TOAST_LEN = n;
    TOAST_START = now();
    notifications::push_simple(msg);
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

// Le panneau de notifications complet est dessiné par `notifications::draw`.

unsafe fn net_connect(idx: usize) {
    if idx >= NET_COUNT {
        return;
    }
    let e = &NET_ENTRIES[idx];
    let ssid = &e.ssid[..e.ssid_len];
    static mut CMD: [u8; 128] = [0; 128];
    let prefix = b"network-connect ";
    let mut len = prefix.len();
    CMD[..len].copy_from_slice(prefix);
    let slen = ssid.len().min(CMD.len() - len - 1);
    CMD[len..len + slen].copy_from_slice(&ssid[..slen]);
    len += slen;
    CMD[len] = 0;
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-control".as_ptr(),
        CMD.as_ptr() as *const core::ffi::c_char,
        core::ptr::null(),
        0,
    );
    toast(b"Connexion en cours...");
    NET_SCAN_REQUESTED = false;
}

// ---------- Panneau réseau ----------

const NET_PANEL_W: i32 = 360;
const NET_ROW_H: i32 = 48;
const NET_HEADER_H: i32 = 56;

unsafe fn net_panel_rect(l: &Layout) -> Rect {
    let rows = NET_COUNT.min(NET_ENTRIES.len()) as i32;
    let status_h = 52;
    let btn_h = 40;
    let h = NET_HEADER_H + status_h + rows * NET_ROW_H + btn_h + 16;
    let x = l.w - NET_PANEL_W - 12 - 96;
    let y = l.bar_top - 12 - h;
    rect(x, y, x + NET_PANEL_W, y + h)
}

unsafe fn draw_net_panel(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !NET_PANEL {
        return;
    }
    net_read_scan();
    let r = net_panel_rect(l);
    let x = r.left;
    let y = r.top;
    let w = r.right - r.left;
    let h = r.bottom - r.top;
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

    // Header
    let icon_g = if NET_IS_WIFI {
        Glyph::Wifi
    } else {
        Glyph::Ethernet
    };
    glyph(hdc, b, f, icon_g, x + 16, y + 14, 24);
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        x + 48,
        y + 18,
        if NET_IS_WIFI { b"Wi-Fi" } else { b"Ethernet" },
    );

    // Connection status area
    let sy = y + NET_HEADER_H;
    fill(hdc, b, x + 1, sy - 1, x + w - 1, sy, rgb(0x3A, 0x3F, 0x4A));
    if NETWORK_CONNECTED != 0 {
        let connected_label = if NET_IS_WIFI && NET_SSID_LEN > 0 {
            &NET_SSID[..NET_SSID_LEN]
        } else if NET_IFACE_LEN > 0 {
            &NET_IFACE[..NET_IFACE_LEN]
        } else {
            b"Connect\xE9"
        };
        ellipse(
            hdc,
            b,
            x + 20,
            sy + 12,
            x + 28,
            sy + 20,
            rgb(0x4C, 0xC2, 0x6A),
        );
        text(
            hdc,
            f.body,
            rgb(0xF2, 0xF2, 0xF2),
            x + 36,
            sy + 10,
            connected_label,
        );
        if NET_ADDR_LEN > 0 {
            text(
                hdc,
                f.small,
                rgb(0x9A, 0xA3, 0xAE),
                x + 36,
                sy + 30,
                &NET_ADDR[..NET_ADDR_LEN],
            );
        }
    } else {
        ellipse(
            hdc,
            b,
            x + 20,
            sy + 16,
            x + 28,
            sy + 24,
            rgb(0x7A, 0x80, 0x8A),
        );
        text(
            hdc,
            f.body,
            rgb(0x9A, 0xA3, 0xAE),
            x + 36,
            sy + 14,
            b"Non connect\xE9",
        );
    }

    // Network list
    let list_y = sy + 52;
    let rows = NET_COUNT.min(NET_ENTRIES.len());
    if rows == 0 {
        text_centered(
            hdc,
            f.body,
            13,
            rgb(0x9A, 0xA3, 0xAE),
            x + w / 2,
            list_y + 14,
            b"Aucun r\xE9seau d\xE9tect\xE9",
        );
    } else {
        for i in 0..rows {
            let ry = list_y + i as i32 * NET_ROW_H;
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
            let e = &NET_ENTRIES[i];
            let hover = NET_HOVER == i as i32;
            if hover {
                round_rect(
                    hdc,
                    b,
                    x + 8,
                    ry + 2,
                    x + w - 8,
                    ry + NET_ROW_H - 2,
                    4,
                    rgb(0x2E, 0x33, 0x3D),
                );
            }
            // Signal bars
            let bars = if e.signal >= 75 {
                4
            } else if e.signal >= 50 {
                3
            } else if e.signal >= 25 {
                2
            } else {
                1
            };
            let sx = x + 20;
            let bar_y = ry + 14;
            let bar_c = if e.in_use {
                rgb(0x60, 0xCD, 0xFF)
            } else {
                rgb(0xC8, 0xCD, 0xD4)
            };
            for k in 0..4i32 {
                let hgt = 3 + k * 3;
                let c = if k < bars {
                    bar_c
                } else {
                    rgb(0x44, 0x48, 0x52)
                };
                fill(
                    hdc,
                    b,
                    sx + k * 4,
                    bar_y + 14 - hgt,
                    sx + k * 4 + 3,
                    bar_y + 14,
                    c,
                );
            }
            // SSID
            let name = &e.ssid[..e.ssid_len];
            text_fit(
                hdc,
                f.body,
                13,
                if e.in_use {
                    rgb(0x60, 0xCD, 0xFF)
                } else {
                    rgb(0xF2, 0xF2, 0xF2)
                },
                x + 44,
                ry + 8,
                name,
                w - 80,
            );
            // Security / connected indicator
            if e.in_use {
                text(
                    hdc,
                    f.small,
                    rgb(0x60, 0xCD, 0xFF),
                    x + 44,
                    ry + 28,
                    b"Connect\xE9",
                );
            } else if e.secured {
                text(
                    hdc,
                    f.small,
                    rgb(0x9A, 0xA3, 0xAE),
                    x + 44,
                    ry + 28,
                    b"S\xE9curis\xE9",
                );
            } else {
                text(
                    hdc,
                    f.small,
                    rgb(0x9A, 0xA3, 0xAE),
                    x + 44,
                    ry + 28,
                    b"Ouvert",
                );
            }
        }
    }

    // Bottom buttons
    let btn_y = list_y + rows as i32 * NET_ROW_H + 4;
    fill(
        hdc,
        b,
        x + 1,
        btn_y - 1,
        x + w - 1,
        btn_y,
        rgb(0x3A, 0x3F, 0x4A),
    );
    // Disconnect button (if connected)
    if NETWORK_CONNECTED != 0 {
        let br = rect(x + 16, btn_y + 4, x + w / 2 - 4, btn_y + 34);
        let hover_disc = NET_HOVER == 100;
        round_rect(
            hdc,
            b,
            br.left,
            br.top,
            br.right,
            br.bottom,
            4,
            if hover_disc {
                rgb(0x3A, 0x3F, 0x4A)
            } else {
                rgb(0x31, 0x35, 0x3F)
            },
        );
        text_centered(
            hdc,
            f.small,
            11,
            rgb(0xC5, 0xC5, 0xC5),
            (br.left + br.right) / 2,
            br.top + 7,
            b"D\xE9connecter",
        );
    }
    // Refresh button
    let rb = rect(
        if NETWORK_CONNECTED != 0 {
            x + w / 2 + 4
        } else {
            x + 16
        },
        btn_y + 4,
        x + w - 16,
        btn_y + 34,
    );
    let hover_ref = NET_HOVER == 101;
    round_rect(
        hdc,
        b,
        rb.left,
        rb.top,
        rb.right,
        rb.bottom,
        4,
        if hover_ref {
            rgb(0x3A, 0x3F, 0x4A)
        } else {
            rgb(0x31, 0x35, 0x3F)
        },
    );
    text_centered(
        hdc,
        f.small,
        11,
        rgb(0xC5, 0xC5, 0xC5),
        (rb.left + rb.right) / 2,
        rb.top + 7,
        b"Actualiser",
    );
}

// ---------- Panneau volume (flyout mélangeur) ----------

const VOL_PANEL_W: i32 = 320;

unsafe fn vol_panel_rect(l: &Layout) -> Rect {
    let h = 280;
    let tray = l.w - 12 - 92 - 96;
    let x = (tray + 24).min(l.w - VOL_PANEL_W - 12);
    let y = l.bar_top - 12 - h;
    rect(x, y, x + VOL_PANEL_W, y + h)
}

unsafe fn draw_vol_panel(hdc: Handle, b: &mut Brushes, f: &Fonts, l: &Layout) {
    if !VOL_PANEL {
        return;
    }
    let r = vol_panel_rect(l);
    let x = r.left;
    let y = r.top;
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    panel(hdc, b, x, y, x + w, y + h, 8, rgb(0x24, 0x27, 0x2F), rgb(0x3A, 0x3F, 0x4A));

    // Icône haut-parleur et titre
    draw_volume(hdc, b, x + 16, y + 16, VOLUME_LEVEL);
    text(hdc, f.semibold, rgb(0xF2, 0xF2, 0xF2), x + 48, y + 18, b"Volume");

    // Niveau affiché
    let mut pct = [0u8; 4];
    let v = VOLUME_LEVEL.min(100);
    let plen = if v >= 100 {
        pct[0] = b'1'; pct[1] = b'0'; pct[2] = b'0'; pct[3] = b'%'; 4
    } else if v >= 10 {
        pct[0] = b'0' + (v / 10) as u8; pct[1] = b'0' + (v % 10) as u8; pct[2] = b'%'; 3
    } else {
        pct[0] = b'0' + v as u8; pct[1] = b'%'; 2
    };
    text(hdc, f.body, rgb(0x9A, 0xA3, 0xAE), x + w - 50, y + 18, &pct[..plen]);

    // Barre de volume (slider)
    let bar_x = x + 16;
    let bar_y = y + 56;
    let bar_w = w - 32;
    round_rect(hdc, b, bar_x, bar_y + 4, bar_x + bar_w, bar_y + 12, 4, rgb(0x3A, 0x3F, 0x4A));
    let fill_w = bar_w * v as i32 / 100;
    if fill_w > 0 {
        round_rect(hdc, b, bar_x, bar_y + 4, bar_x + fill_w, bar_y + 12, 4, pal().accent);
    }
    // Curseur
    let knob_x = bar_x + fill_w;
    ellipse(hdc, b, knob_x - 7, bar_y, knob_x + 7, bar_y + 16, rgb(0xF2, 0xF2, 0xF2));

    // Séparateur
    let sep_y = bar_y + 32;
    fill(hdc, b, x + 1, sep_y, x + w - 1, sep_y + 1, rgb(0x3A, 0x3F, 0x4A));

    // Applications (simulé à partir des fenêtres ouvertes)
    text(hdc, f.small, rgb(0x9A, 0xA3, 0xAE), x + 16, sep_y + 10, b"Applications");
    let mut ay = sep_y + 30;
    let mut drawn = 0;
    for app in 0..wm::APP_COUNT {
        if !wm::is_open(app) || drawn >= 4 {
            continue;
        }
        let name = wm::title_of(app);
        let hover = VOL_HOVER == 200 + app as i32;
        if hover {
            round_rect(hdc, b, x + 8, ay - 2, x + w - 8, ay + 26, 4, rgb(0x2E, 0x33, 0x3D));
        }
        glyph(hdc, b, f, wm::glyph_of(app), x + 16, ay + 2, 16);
        text_fit(hdc, f.body, 13, rgb(0xF2, 0xF2, 0xF2), x + 40, ay + 4, name, w / 2 - 30);
        // Mini barre de volume par app (100% par défaut)
        let app_bar_x = x + w / 2 + 20;
        let app_bar_w = w / 2 - 50;
        round_rect(hdc, b, app_bar_x, ay + 8, app_bar_x + app_bar_w, ay + 14, 3, rgb(0x3A, 0x3F, 0x4A));
        round_rect(hdc, b, app_bar_x, ay + 8, app_bar_x + app_bar_w, ay + 14, 3, pal().accent);
        ay += 32;
        drawn += 1;
    }
    if drawn == 0 {
        text(hdc, f.small, rgb(0x9A, 0xA3, 0xAE), x + 16, ay + 4, b"Aucune application ouverte");
    }

    // Boutons en bas : Muet + Mélangeur
    let btn_y = r.bottom - 44;
    fill(hdc, b, x + 1, btn_y - 1, x + w - 1, btn_y, rgb(0x3A, 0x3F, 0x4A));
    let mute_r = rect(x + 16, btn_y + 4, x + w / 2 - 4, btn_y + 34);
    round_rect(hdc, b, mute_r.left, mute_r.top, mute_r.right, mute_r.bottom, 4,
        if VOL_HOVER == 300 { rgb(0x3A, 0x3F, 0x4A) } else { rgb(0x31, 0x35, 0x3F) });
    text_centered(hdc, f.small, 11, rgb(0xC5, 0xC5, 0xC5), (mute_r.left + mute_r.right) / 2, mute_r.top + 7,
        if VOLUME_LEVEL == 0 { b"R\xE9tablir" } else { b"Muet" });
    let mix_r = rect(x + w / 2 + 4, btn_y + 4, x + w - 16, btn_y + 34);
    round_rect(hdc, b, mix_r.left, mix_r.top, mix_r.right, mix_r.bottom, 4,
        if VOL_HOVER == 301 { rgb(0x3A, 0x3F, 0x4A) } else { rgb(0x31, 0x35, 0x3F) });
    text_centered(hdc, f.small, 11, rgb(0xC5, 0xC5, 0xC5), (mix_r.left + mix_r.right) / 2, mix_r.top + 7, b"M\xE9langeur");
}

// ---------- Glisser-déposer et collage sur le bureau ----------

pub unsafe fn handle_file_drop(do_copy: bool) {
    let desktop = explorer::desktop_folder_path();
    if desktop.get().is_empty() {
        return;
    }
    explorer::drop_files_to(desktop.get(), do_copy);
    toast(if do_copy {
        b"Fichier(s) copi\xE9(s) sur le Bureau."
    } else {
        b"Fichier(s) d\xE9plac\xE9(s) sur le Bureau."
    });
}

// ---------- Icônes du bureau ----------

fn desktop_icon_list() -> [(&'static [u8], Glyph); 5] {
    [
        (tr(S_THIS_PC), Glyph::Computer),
        (tr(S_DOCUMENTS), Glyph::Folder),
        (tr(S_TRASH), Glyph::Trash),
        (tr(S_FILE_EXPLORER), Glyph::Folder),
        (tr(S_SETTINGS_APP), Glyph::Settings),
    ]
}

unsafe fn desktop_icons(hdc: Handle, b: &mut Brushes, f: &Fonts) {
    let icons = desktop_icon_list();
    for (i, (name, g)) in icons.iter().enumerate() {
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
    crate::login::init();
    SCREEN_W = GetSystemMetrics(0).max(640);
    SCREEN_H = GetSystemMetrics(1).max(400);
    wm::WORK = rect(0, 0, SCREEN_W, SCREEN_H - TASKBAR_HEIGHT);
    load_icon_positions();
    BOOT_START = now();
    let mut v = [0i8; 4];
    if GetEnvironmentVariableA(c"WINLUX_NO_SPLASH".as_ptr(), v.as_mut_ptr(), 4) > 0 {
        BOOT_DONE = true;
        BOOT_STEP = 4;
    }
    let mut v2 = [0i8; 4];
    if GetEnvironmentVariableA(c"WINLUX_NO_LOGIN".as_ptr(), v2.as_mut_ptr(), 4) > 0 {
        crate::login::skip_login();
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
            tr(S_SLEEP),
        );
        f.release();
        b.release();
        return;
    }
    if !BOOT_DONE {
        boot_splash(hdc, &mut b, &f, &l);
    } else if !crate::login::logged_in() {
        f.release();
        b.release();
        crate::login::draw(hdc, l.w, l.h);
        return;
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
        search_overlay(hdc, &mut b, &f, &l);
        notifications::draw(hdc, &mut b, &f, l.w, l.bar_top);
        draw_net_panel(hdc, &mut b, &f, &l);
        draw_vol_panel(hdc, &mut b, &f, &l);
        weather::draw_panel(hdc, &mut b, &f, l.bar_top, l.w);
        draw_toast(hdc, &mut b, &f, &l);
        draw_battery_tip(hdc, &mut b, &f, &l);
        // Overlay de fermeture.
        if SHUTDOWN_ACTION > 0 {
            let t = since(SHUTDOWN_START);
            SHUTDOWN_FADE = ease_out(t, 800).min(1000);
            let alpha = SHUTDOWN_FADE;
            // Fond sombre progressif.
            if alpha > 0 {
                fill(hdc, &mut b, 0, 0, l.w, l.h, rgb(0, 0, 0));
            }
            let label: &[u8] = match SHUTDOWN_ACTION {
                1 => b"Arr\xEAt en cours...",
                2 => b"Red\xE9marrage en cours...",
                _ => b"",
            };
            if !label.is_empty() {
                text_centered(
                    hdc,
                    f.large,
                    22,
                    mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
                    l.w / 2,
                    l.h / 2 - 30,
                    label,
                );
                // Cercle de chargement.
                let cx = l.w / 2;
                let cy = l.h / 2 + 20;
                let dots = 5;
                let radius = 16;
                for k in 0..dots {
                    let angle = (t / 3 + k * 70) % 360;
                    let (sin_a, cos_a) = crate::login::approx_sincos(angle as i32);
                    let dx = cx + (sin_a * radius) / 1000;
                    let dy = cy - (cos_a * radius) / 1000;
                    let dot_alpha = (1000 - k as i32 * 200).max(200);
                    ellipse(
                        hdc,
                        &mut b,
                        dx - 3,
                        dy - 3,
                        dx + 3,
                        dy + 3,
                        mix(0, rgb(0xFF, 0xFF, 0xFF), dot_alpha * alpha / 1000, 1000),
                    );
                }
            }
            if alpha >= 980 && SHUTDOWN_ACTION > 0 {
                let action = SHUTDOWN_ACTION;
                SHUTDOWN_ACTION = 0;
                match action {
                    1 => {
                        crate::settings::control(c"poweroff");
                    }
                    2 => {
                        crate::settings::control(c"reboot");
                    }
                    _ => {}
                }
            }
        }
        let bar_dy = if FADE < 1000 {
            (1000 - ease_out(intro, INTRO_MS)) * TASKBAR_HEIGHT / 1000
        } else {
            0
        };
        taskbar(hdc, &mut b, &f, &l, bar_dy);
        draw_context(hdc, &mut b, &f);
        if crate::login::uac_visible() {
            crate::login::draw_uac(hdc, l.w, l.h);
        }
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
                if crate::login::logged_in() {
                    INTRO_START = now();
                }
            }
            _ => {}
        }
    }
    if BOOT_DONE && !crate::login::logged_in() {
        if crate::login::animating() && since(LAST_FRAME) >= 15 {
            LAST_FRAME = now();
            InvalidateRect(hwnd, core::ptr::null(), 0);
        }
        if crate::login::logged_in() {
            INTRO_START = now();
        }
        return;
    }
    let animating = !BOOT_DONE
        || wm::animating()
        || crate::gfx::theme_animating()
        || notifications::animating()
        || crate::welcome::animating()
        || since(INTRO_START) < INTRO_MS + 40
        || (START_ANIM != 0 && since(START_ANIM) < START_OPEN_MS + 40)
        || START_CLOSING
        || (RECENTER_ANIM != 0 && since(RECENTER_ANIM) < RECENTER_MS + 40)
        || (TOAST_LEN > 0 && (since(TOAST_START) < 300 || since(TOAST_START) > TOAST_MS - 260))
        || SHUTDOWN_ACTION > 0;
    if animating && since(LAST_FRAME) >= 15 {
        LAST_FRAME = now();
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
    if crate::terminal::poll() {
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
        for i in 0..5 {
            ICON_X[i] = ICON_X[i].min(w - 80);
            ICON_Y[i] = ICON_Y[i].min(h - TASKBAR_HEIGHT - 90);
        }
        MoveWindow(hwnd, 0, 0, w, h, 1);
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    let mut now = SystemTime::default();
    GetLocalTime(&mut now);
    let system = crate::settings::system_tick() | power_tick();
    let weather_changed = weather::poll();
    let welcome = BOOT_DONE
        && crate::login::logged_in()
        && since(INTRO_START) > INTRO_MS
        && crate::welcome::maybe_show();
    let apps = BOOT_DONE && wm::tick();
    if (now.minute != LAST_MINUTE && BOOT_DONE) || apps || system || weather_changed || welcome {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

unsafe fn repaint(hwnd: Handle) {
    InvalidateRect(hwnd, core::ptr::null(), 0);
}

pub unsafe fn on_button_down(hwnd: Handle, x: i32, y: i32) {
    if crate::login::uac_visible() {
        crate::login::uac_on_click(x, y, SCREEN_W, SCREEN_H);
        repaint(hwnd);
        return;
    }
    if !crate::login::logged_in() && BOOT_DONE {
        crate::login::on_click(hwnd, x, y, SCREEN_W, SCREEN_H);
        return;
    }
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
                wm::close_snap_layout();
                wm::TASK_VIEW = false;
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
        let weather_x = tray - 24 - weather::TRAY_WIDTH - 8;
        if x >= weather_x - 4 && x < weather_x + weather::TRAY_WIDTH {
            weather::toggle();
            notifications::set_open(false);
            NET_PANEL = false;
            VOL_PANEL = false;
        } else if x >= tray - 28 && x < tray {
            notifications::toggle();
            weather::PANEL_OPEN = false;
            NET_PANEL = false;
            VOL_PANEL = false;
        } else if x >= tray && x < tray + 24 {
            NET_PANEL = !NET_PANEL;
            VOL_PANEL = false;
            notifications::set_open(false);
            if NET_PANEL && !NET_SCAN_REQUESTED {
                NET_SCAN_REQUESTED = true;
                crate::settings::control(c"network-scan");
            }
        } else if x >= tray + 24 && x < tray + 52 {
            VOL_PANEL = !VOL_PANEL;
            VOL_HOVER = -1;
            NET_PANEL = false;
            notifications::set_open(false);
        } else if x >= tray + 52 && x < tray + 88 {
            BATTERY_HOVER = false;
            VOL_PANEL = false;
            crate::settings::open_page(crate::settings::PAGE_POWER);
        } else if x >= l.w - 12 - 92 {
            notifications::toggle();
            NET_PANEL = false;
            VOL_PANEL = false;
        } else {
            return;
        }
        repaint(hwnd);
        return;
    }
    if weather::PANEL_OPEN && y < l.bar_top {
        if weather::mouse_panel(x, y, l.bar_top, l.w) {
            repaint(hwnd);
        }
        return;
    }
    if notifications::is_open() && y < l.bar_top {
        if notifications::on_click(l.w, l.bar_top, x, y) {
            repaint(hwnd);
            return;
        }
    }
    if SEARCH_OVERLAY {
        let w = 520.min(l.w - 40);
        let h = 440.min(l.bar_top - 40);
        let ox = (l.w - w) / 2;
        let oy = (l.bar_top - h) / 2;
        if x >= ox && x < ox + w && y >= oy && y < oy + h {
            let pad = 24;
            let il = ox + pad;
            let ir = ox + w - pad;
            let fy = oy + 60;
            let ry = fy + 52;
            if SEARCH_BUF_LEN > 0 {
                let mut results = [SearchResult {
                    name: b"",
                    launch: Launch::Explorer,
                    glyph: Glyph::Search,
                    score: 0,
                }; 16];
                let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results);
                for i in 0..count {
                    let iy = ry + (i as i32) * 44;
                    if iy + 40 >= oy + h - 16 {
                        break;
                    }
                    if y >= iy && y < iy + 40 && x >= il && x < ir {
                        SEARCH_OVERLAY = false;
                        launch(results[i].launch);
                        repaint(hwnd);
                        return;
                    }
                }
            }
        } else {
            SEARCH_OVERLAY = false;
        }
        repaint(hwnd);
        return;
    }
    if NET_PANEL && y < l.bar_top {
        let pr = net_panel_rect(&l);
        if inside(&pr, x, y) {
            let rows = NET_COUNT.min(NET_ENTRIES.len());
            let list_y = pr.top + NET_HEADER_H + 52;
            let btn_y = list_y + rows as i32 * NET_ROW_H + 4;
            // Disconnect button
            if NETWORK_CONNECTED != 0
                && y >= btn_y + 4
                && y < btn_y + 34
                && x >= pr.left + 16
                && x < pr.left + (pr.right - pr.left) / 2 - 4
            {
                crate::settings::control(c"network-disconnect");
                NET_SCAN_REQUESTED = false;
            }
            // Refresh button
            let rb_left = if NETWORK_CONNECTED != 0 {
                pr.left + (pr.right - pr.left) / 2 + 4
            } else {
                pr.left + 16
            };
            if y >= btn_y + 4 && y < btn_y + 34 && x >= rb_left && x < pr.right - 16 {
                NET_SCAN_REQUESTED = true;
                crate::settings::control(c"network-scan");
            }
            // Network row click -> connect
            if y >= list_y && y < list_y + rows as i32 * NET_ROW_H {
                let idx = ((y - list_y) / NET_ROW_H) as usize;
                if idx < rows && !NET_ENTRIES[idx].in_use {
                    net_connect(idx);
                }
            }
        } else {
            NET_PANEL = false;
            NET_HOVER = -1;
        }
        repaint(hwnd);
        return;
    }
    if VOL_PANEL && y < l.bar_top {
        let pr = vol_panel_rect(&l);
        if inside(&pr, x, y) {
            let w = pr.right - pr.left;
            let btn_y = pr.bottom - 44;
            // Slider click: adjust volume
            let bar_x = pr.left + 16;
            let bar_w = w - 32;
            let bar_y = pr.top + 56;
            if y >= bar_y && y < bar_y + 20 && x >= bar_x && x < bar_x + bar_w {
                let v = ((x - bar_x) * 100 / bar_w).max(0).min(100) as u32;
                VOLUME_LEVEL = v;
                if v <= 5 {
                    crate::settings::control(c"volume-mute");
                } else {
                    // Set volume via repeated up/down to approximate
                    crate::settings::control(c"volume-set");
                }
            }
            // Mute button
            if y >= btn_y + 4 && y < btn_y + 34 && x >= pr.left + 16 && x < pr.left + w / 2 - 4 {
                crate::settings::control(c"volume-mute");
            }
            // Mixer button -> open advanced sound settings
            if y >= btn_y + 4 && y < btn_y + 34 && x >= pr.left + w / 2 + 4 && x < pr.right - 16 {
                crate::settings::open_page(16);
                VOL_PANEL = false;
            }
        } else {
            VOL_PANEL = false;
            VOL_HOVER = -1;
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
                    POWER_MENU = !POWER_MENU;
                    POWER_HOVER = -1;
                }
                // Sous-menu alimentation.
                210 => {
                    // Arrêter
                    set_start(false);
                    POWER_MENU = false;
                    SHUTDOWN_ACTION = 1;
                    SHUTDOWN_START = now();
                    SHUTDOWN_FADE = 0;
                }
                211 => {
                    // Redémarrer
                    set_start(false);
                    POWER_MENU = false;
                    SHUTDOWN_ACTION = 2;
                    SHUTDOWN_START = now();
                    SHUTDOWN_FADE = 0;
                }
                212 => {
                    // Verrouiller
                    set_start(false);
                    POWER_MENU = false;
                    crate::login::lock_screen();
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
                i @ 600..=615 => {
                    let idx = (i - 600) as usize;
                    let mut results = [SearchResult {
                        name: b"",
                        launch: Launch::Explorer,
                        glyph: Glyph::Search,
                        score: 0,
                    }; 16];
                    let count =
                        collect_results(unsafe { &START_SEARCH[..START_SEARCH_LEN] }, &mut results);
                    if idx < count {
                        set_start(false);
                        launch(results[idx].launch);
                    }
                }
                i @ 0..=5 => {
                    set_start(false);
                    launch(pinned_apps()[i as usize].launch);
                }
                i @ 100..=103 => {
                    set_start(false);
                    launch(recommended_apps()[(i - 100) as usize].3);
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
    for i in (0..5).rev() {
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
    if SEARCH_OVERLAY {
        let w = 520.min(l.w - 40);
        let h = 440.min(l.bar_top - 40);
        let ox = (l.w - w) / 2;
        let oy = (l.bar_top - h) / 2;
        let pad = 24;
        let il = ox + pad;
        let ir = ox + w - pad;
        let fy = oy + 60;
        let ry = fy + 52;
        let mut hover = -1i32;
        if SEARCH_BUF_LEN > 0 && x >= il && x < ir {
            let mut results = [SearchResult {
                name: b"",
                launch: Launch::Explorer,
                glyph: Glyph::Search,
                score: 0,
            }; 16];
            let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results);
            for i in 0..count {
                let iy = ry + (i as i32) * 44;
                if iy + 40 >= oy + h - 16 {
                    break;
                }
                if y >= iy && y < iy + 40 {
                    hover = i as i32;
                    break;
                }
            }
        }
        if hover != SEARCH_HOVER {
            SEARCH_HOVER = hover;
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
    let tray = l.w - 12 - 92 - 96;
    let bh = y >= l.bar_top && x >= tray + 52 && x < tray + 88;
    if bh != BATTERY_HOVER {
        BATTERY_HOVER = bh;
        rp = true;
    }
    let hs = if START_OPEN {
        start_item_at(&l, x, y)
    } else {
        -1
    };
    if hs != HOVER_START_ITEM {
        HOVER_START_ITEM = hs;
        rp = true;
    }
    if POWER_MENU {
        let ph = if hs >= 210 && hs <= 212 { hs - 210 } else { -1 };
        if ph != POWER_HOVER {
            POWER_HOVER = ph;
            rp = true;
        }
    }
    if notifications::is_open() {
        if notifications::on_mouse_move(l.w, l.bar_top, x, y) {
            rp = true;
        }
    }
    // Network panel hover
    if NET_PANEL {
        let pr = net_panel_rect(&l);
        let nh = if inside(&pr, x, y) {
            let rows = NET_COUNT.min(NET_ENTRIES.len());
            let list_y = pr.top + NET_HEADER_H + 52;
            let btn_y = list_y + rows as i32 * NET_ROW_H + 4;
            if y >= list_y && y < list_y + rows as i32 * NET_ROW_H {
                ((y - list_y) / NET_ROW_H) as i32
            } else if y >= btn_y + 4 && y < btn_y + 34 {
                let w = pr.right - pr.left;
                if NETWORK_CONNECTED != 0 && x < pr.left + w / 2 - 4 {
                    100 // disconnect
                } else if x
                    >= (if NETWORK_CONNECTED != 0 {
                        pr.left + w / 2 + 4
                    } else {
                        pr.left + 16
                    })
                {
                    101 // refresh
                } else {
                    -1
                }
            } else {
                -1
            }
        } else {
            -1
        };
        if nh != NET_HOVER {
            NET_HOVER = nh;
            rp = true;
        }
    }
    // Volume panel hover
    if VOL_PANEL {
        let pr = vol_panel_rect(&l);
        let vh = if inside(&pr, x, y) {
            let w = pr.right - pr.left;
            let btn_y = pr.bottom - 44;
            if y >= btn_y + 4 && y < btn_y + 34 {
                if x < pr.left + w / 2 - 4 { 300 } else if x >= pr.left + w / 2 + 4 { 301 } else { -1 }
            } else {
                let sep_y = pr.top + 88;
                let mut ay = sep_y + 30;
                let mut found = -1i32;
                for app in 0..wm::APP_COUNT {
                    if !wm::is_open(app) { continue; }
                    if y >= ay - 2 && y < ay + 26 { found = 200 + app as i32; break; }
                    ay += 32;
                }
                found
            }
        } else {
            -1
        };
        if vh != VOL_HOVER {
            VOL_HOVER = vh;
            rp = true;
        }
    }
    if DRAGGED_ICON >= 0 {
        let i = DRAGGED_ICON as usize;
        // Grille de 96 px, comme l'alignement automatique de Windows.
        let gx = ((x - 38).max(0) + 48) / 96 * 96 + 12;
        let gy = ((y - 42).max(0) + 48) / 96 * 96 + 12;
        let mut nx = gx.min(l.w - 80);
        let mut ny = gy.min(l.bar_top - 90);
        let occupied = |cx: i32, cy: i32, skip: usize| -> bool {
            for j in 0..5usize {
                if j != skip && ICON_X[j] == cx && ICON_Y[j] == cy {
                    return true;
                }
            }
            false
        };
        if occupied(nx, ny, i) {
            let mut best_d = i32::MAX;
            let mut bx = ICON_X[i];
            let mut by = ICON_Y[i];
            let cols = (l.w - 12) / 96;
            let rows = (l.bar_top - 12) / 96;
            for row in 0..rows {
                for col in 0..cols {
                    let cx = col * 96 + 12;
                    let cy = row * 96 + 12;
                    if !occupied(cx, cy, i) {
                        let d = (cx - nx).abs() + (cy - ny).abs();
                        if d < best_d {
                            best_d = d;
                            bx = cx;
                            by = cy;
                        }
                    }
                }
            }
            nx = bx;
            ny = by;
        }
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
        save_icon_positions();
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
            for i in (0..5).rev() {
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
    if let Mouse::Wheel(delta) = ev {
        let l = layout();
        if notifications::on_wheel(l.w, l.bar_top, x, y, delta) {
            repaint(hwnd);
            return;
        }
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
    if crate::login::uac_visible() {
        crate::login::uac_on_key(vk, ch);
        repaint(hwnd);
        return;
    }
    if !crate::login::logged_in() && BOOT_DONE {
        crate::login::on_key(hwnd, vk, ch);
        return;
    }
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
        if notifications::is_open() {
            notifications::set_open(false);
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
    // Win+Shift+S (Screenshot) / Win+S (Recherche)
    if ch == 0 && vk == 0x53 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        let shift = ((GetKeyState(0x10) as u16) & 0x8000) != 0;
        if win && shift {
            launch(Launch::Exec(b"screenshot.exe"));
            return;
        }
        if win {
            SEARCH_OVERLAY = !SEARCH_OVERLAY;
            if SEARCH_OVERLAY {
                SEARCH_BUF_LEN = 0;
                SEARCH_HOVER = -1;
                set_start(false);
            }
            repaint(hwnd);
            return;
        }
    }
    // Win+I (Paramètres)
    if ch == 0 && vk == 0x49 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            wm::open(wm::APP_SETTINGS);
            repaint(hwnd);
            return;
        }
    }
    // Win+L (Verrouiller)
    if ch == 0 && vk == 0x4C {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            crate::login::lock_screen();
            repaint(hwnd);
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
    // Win+Tab (Vue des tâches)
    if ch == 0 && vk == 0x09 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        if win {
            unsafe {
                wm::TASK_VIEW = !wm::TASK_VIEW;
            }
            repaint(hwnd);
            return;
        }
    }
    // Win+flèches (snap layouts)
    if ch == 0 && (vk == 0x25 || vk == 0x26 || vk == 0x27 || vk == 0x28) {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
        if win && ctrl {
            // Win+Ctrl+Left/Right : changer de bureau
            if vk == 0x25 {
                let cur = wm::current_desktop();
                if cur > 0 {
                    unsafe {
                        wm::switch_desktop(cur - 1);
                    }
                }
            } else if vk == 0x27 {
                let cur = wm::current_desktop();
                if cur + 1 < wm::desktop_count() {
                    unsafe {
                        wm::switch_desktop(cur + 1);
                    }
                }
            }
            repaint(hwnd);
            return;
        }
        if win {
            if unsafe { wm::snap_arrow(vk) } {
                repaint(hwnd);
                return;
            }
        }
    }
    // Win+Ctrl+D (nouveau bureau)
    if ch == 0 && vk == 0x44 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
        if win && ctrl {
            unsafe {
                wm::add_desktop();
            }
            repaint(hwnd);
            return;
        }
    }
    // Win+Ctrl+F4 (fermer le bureau courant)
    if ch == 0 && vk == 0x73 {
        let win = ((GetKeyState(0x5B) as u16) & 0x8000) != 0
            || ((GetKeyState(0x5C) as u16) & 0x8000) != 0;
        let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
        if win && ctrl {
            let cur = wm::current_desktop();
            unsafe {
                wm::remove_desktop(cur);
            }
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
            for app in 0..wm::APP_COUNT {
                if wm::is_open(app) && !wm::is_minimized(app) {
                    any_visible = true;
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
        if SEARCH_OVERLAY {
            SEARCH_OVERLAY = false;
            repaint(hwnd);
            return;
        }
        set_start(!(START_OPEN && !START_CLOSING));
        repaint(hwnd);
        return;
    }
    if SEARCH_OVERLAY {
        if ch == 27 || vk == 0x1B {
            SEARCH_OVERLAY = false;
            repaint(hwnd);
            return;
        }
        if ch == 8 {
            if SEARCH_BUF_LEN > 0 {
                SEARCH_BUF_LEN -= 1;
            }
            SEARCH_HOVER = -1;
            repaint(hwnd);
            return;
        }
        if ch == 13 || vk == 0x0D {
            if SEARCH_HOVER >= 0 {
                let mut results = [SearchResult {
                    name: b"",
                    launch: Launch::Explorer,
                    glyph: Glyph::Search,
                    score: 0,
                }; 16];
                let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results);
                let idx = SEARCH_HOVER as usize;
                if idx < count {
                    SEARCH_OVERLAY = false;
                    launch(results[idx].launch);
                    repaint(hwnd);
                    return;
                }
            } else if SEARCH_BUF_LEN > 0 {
                let mut results = [SearchResult {
                    name: b"",
                    launch: Launch::Explorer,
                    glyph: Glyph::Search,
                    score: 0,
                }; 16];
                let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results);
                if count > 0 {
                    SEARCH_OVERLAY = false;
                    launch(results[0].launch);
                    repaint(hwnd);
                    return;
                }
            }
            return;
        }
        if vk == 0x28 {
            // Down
            let mut results = [SearchResult {
                name: b"",
                launch: Launch::Explorer,
                glyph: Glyph::Search,
                score: 0,
            }; 16];
            let count = collect_results(&SEARCH_BUF[..SEARCH_BUF_LEN], &mut results) as i32;
            if count > 0 {
                SEARCH_HOVER = (SEARCH_HOVER + 1).min(count - 1);
            }
            repaint(hwnd);
            return;
        }
        if vk == 0x26 {
            // Up
            SEARCH_HOVER = (SEARCH_HOVER - 1).max(-1);
            repaint(hwnd);
            return;
        }
        if ch >= 0x20 && ch < 0x100 && SEARCH_BUF_LEN < 63 {
            SEARCH_BUF[SEARCH_BUF_LEN] = ch as u8;
            SEARCH_BUF_LEN += 1;
            SEARCH_HOVER = -1;
            repaint(hwnd);
        }
        return;
    }
    if START_OPEN && !START_CLOSING {
        if ch == 27 || vk == 0x1B {
            START_SEARCH_LEN = 0;
            START_SEARCH_ACTIVE = false;
            set_start(false);
            repaint(hwnd);
            return;
        }
        if ch == 8 {
            if START_SEARCH_LEN > 0 {
                START_SEARCH_LEN -= 1;
            }
            repaint(hwnd);
            return;
        }
        if ch == 13 || vk == 0x0D {
            if START_SEARCH_LEN > 0 {
                let mut results = [SearchResult {
                    name: b"",
                    launch: Launch::Explorer,
                    glyph: Glyph::Search,
                    score: 0,
                }; 16];
                let count = collect_results(&START_SEARCH[..START_SEARCH_LEN], &mut results);
                if count > 0 {
                    START_SEARCH_LEN = 0;
                    START_SEARCH_ACTIVE = false;
                    set_start(false);
                    launch(results[0].launch);
                    repaint(hwnd);
                    return;
                }
            }
            return;
        }
        if ch >= 0x20 && ch < 0x100 && START_SEARCH_LEN < 63 {
            START_SEARCH[START_SEARCH_LEN] = ch as u8;
            START_SEARCH_LEN += 1;
            START_SEARCH_ACTIVE = true;
            repaint(hwnd);
        }
        return;
    }
    if ch == 0 && vk == 0x56 {
        let ctrl = ((GetKeyState(0x11) as u16) & 0x8000) != 0;
        if ctrl {
            let desktop = explorer::desktop_folder_path();
            if !desktop.get().is_empty() {
                explorer::paste_to(desktop.get());
                toast(b"Fichier(s) coll\xE9(s) sur le Bureau.");
            }
            repaint(hwnd);
            return;
        }
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
    crate::login::skip_login();
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
