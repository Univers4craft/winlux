//! user32, gdi32 et uxtheme : les fonctions du lot D du relevé d'imports
//! (#1512).
//!
//! Importées par pywin32, Qt 6, wxPython, panda3d, jdk4py et ffmpeg et
//! absentes jusqu'ici. Chacune s'appuie sur ce que Winlux fait déjà
//! (fenêtres, DC, régions, menus, polices) et suit la documentation
//! publique. Ce qui n'existe pas sous Winlux (écran tactile, stations
//! fenêtre multiples, rotation d'écran) répond comme un Windows qui n'en a
//! pas.

#![allow(clippy::missing_safety_doc)]

use crate::dlls::{cstr, lookup, wstr, Exports};
use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::Mutex;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),* $(,)?) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_WINDOW_HANDLE: u32 = 1400;
const DSTINVERT: u32 = 0x0055_0009;
const SRCCOPY: u32 = 0x00CC_0020;

fn fail(e: u32) -> i32 {
    set_last_error(e);
    0
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

type Rect = [i32; 4];

unsafe fn rect_at(p: *const i32) -> Option<Rect> {
    (!p.is_null()).then(|| std::ptr::read_unaligned(p as *const Rect))
}

unsafe fn is_window(hwnd: usize) -> bool {
    api!("user32", "IsWindow", fn(usize) -> i32, hwnd).unwrap_or(0) != 0
}

unsafe fn window_rect(hwnd: usize) -> Option<Rect> {
    let mut r: Rect = [0; 4];
    (api!(
        "user32",
        "GetWindowRect",
        fn(usize, *mut Rect) -> i32,
        hwnd,
        &mut r
    ) == Some(1))
    .then_some(r)
}

// ── Curseur, clavier ────────────────────────────────────────────────────

/// GetCursorInfo : CURSORINFO (24 octets en 64 bits) avec le curseur
/// courant et la position à l'écran.
unsafe extern "win64" fn get_cursor_info(info: *mut u8) -> i32 {
    if info.is_null() || std::ptr::read_unaligned(info as *const u32) != 24 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let cursor = api!("user32", "GetCursor", fn() -> usize).unwrap_or(0);
    let mut pt = [0i32; 2];
    api!("user32", "GetCursorPos", fn(*mut [i32; 2]) -> i32, &mut pt);
    std::ptr::write_unaligned(info.add(4) as *mut u32, u32::from(cursor != 0)); // CURSOR_SHOWING
    std::ptr::write_unaligned(info.add(8) as *mut usize, cursor);
    std::ptr::write_unaligned(info.add(16) as *mut [i32; 2], pt);
    1
}

/// GetKeyboardType : clavier étendu 101/102 touches, douze touches de
/// fonction (le clavier PC courant).
extern "win64" fn get_keyboard_type(kind: i32) -> i32 {
    match kind {
        0 => 4,
        1 => 0,
        2 => 12,
        _ => 0,
    }
}

/// SetDoubleClickTime : le délai vaut pour le programme qui le règle ;
/// le réglage de Paramètres > Souris reste celui de la session.
extern "win64" fn set_double_click_time(ms: u32) -> i32 {
    crate::dlls::mouse::set_double_click_override(if ms == 0 { 500 } else { ms });
    1
}

// ── Fenêtres ────────────────────────────────────────────────────────────

/// CloseWindow : réduit la fenêtre (elle n'est pas détruite).
unsafe extern "win64" fn close_window(hwnd: usize) -> i32 {
    if !is_window(hwnd) {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    }
    api!("user32", "ShowWindow", fn(usize, i32) -> i32, hwnd, 6); // SW_MINIMIZE
    1
}

/// GetWindowInfo : WINDOWINFO (60 octets).
unsafe extern "win64" fn get_window_info(hwnd: usize, info: *mut u8) -> i32 {
    if info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let Some(win) = window_rect(hwnd) else {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    };
    let mut client: Rect = [0; 4];
    api!(
        "user32",
        "GetClientRect",
        fn(usize, *mut Rect) -> i32,
        hwnd,
        &mut client
    );
    let mut origin = [0i32; 2];
    api!(
        "user32",
        "ClientToScreen",
        fn(usize, *mut [i32; 2]) -> i32,
        hwnd,
        &mut origin
    );
    let client = [
        origin[0],
        origin[1],
        origin[0] + client[2] - client[0],
        origin[1] + client[3] - client[1],
    ];
    let style = api!("user32", "GetWindowLongW", fn(usize, i32) -> i32, hwnd, -16).unwrap_or(0);
    let ex = api!("user32", "GetWindowLongW", fn(usize, i32) -> i32, hwnd, -20).unwrap_or(0);
    let active = api!("user32", "GetActiveWindow", fn() -> usize).unwrap_or(0) == hwnd;
    let atom = api!("user32", "GetClassLongW", fn(usize, i32) -> u32, hwnd, -32).unwrap_or(0);
    std::ptr::write_unaligned(info as *mut u32, 60);
    std::ptr::write_unaligned(info.add(4) as *mut Rect, win);
    std::ptr::write_unaligned(info.add(20) as *mut Rect, client);
    std::ptr::write_unaligned(info.add(36) as *mut i32, style);
    std::ptr::write_unaligned(info.add(40) as *mut i32, ex);
    std::ptr::write_unaligned(info.add(44) as *mut u32, u32::from(active)); // WS_ACTIVECAPTION
    std::ptr::write_unaligned(info.add(48) as *mut u32, (client[0] - win[0]).max(0) as u32);
    std::ptr::write_unaligned(info.add(52) as *mut u32, (win[3] - client[3]).max(0) as u32);
    std::ptr::write_unaligned(info.add(56) as *mut u16, atom as u16);
    std::ptr::write_unaligned(info.add(58) as *mut u16, 0x0400);
    1
}

extern "win64" fn is_hung_app_window(_hwnd: usize) -> i32 {
    0
}

/// SetWindowWord / SetClassWord : les index positifs des octets
/// supplémentaires, par SetWindowLong / SetClassLong.
unsafe extern "win64" fn set_window_word(hwnd: usize, index: i32, value: u16) -> u16 {
    let old = api!(
        "user32",
        "GetWindowLongW",
        fn(usize, i32) -> i32,
        hwnd,
        index
    )
    .unwrap_or(0);
    let new = (old & !0xFFFF) | i32::from(value);
    api!(
        "user32",
        "SetWindowLongW",
        fn(usize, i32, i32) -> i32,
        hwnd,
        index,
        new
    );
    old as u16
}

unsafe extern "win64" fn set_class_word(hwnd: usize, index: i32, value: u16) -> u16 {
    let old = api!(
        "user32",
        "GetClassLongW",
        fn(usize, i32) -> u32,
        hwnd,
        index
    )
    .unwrap_or(0);
    let new = (old & !0xFFFF) | u32::from(value);
    api!(
        "user32",
        "SetClassLongW",
        fn(usize, i32, u32) -> u32,
        hwnd,
        index,
        new
    );
    old as u16
}

/// GetClassInfoA : WNDCLASSA a les mêmes champs que WNDCLASSEXA, sans
/// cbSize ni hIconSm.
unsafe extern "win64" fn get_class_info_a(inst: usize, name: *const u8, out: *mut u8) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut ex = [0u8; 80];
    ex[..4].copy_from_slice(&80u32.to_le_bytes());
    let ok = api!(
        "user32",
        "GetClassInfoExA",
        fn(usize, *const u8, *mut u8) -> i32,
        inst,
        name,
        ex.as_mut_ptr()
    )
    .unwrap_or(0);
    if ok == 0 {
        return 0;
    }
    std::ptr::copy_nonoverlapping(ex.as_ptr().add(4), out, 4);
    std::ptr::write_bytes(out.add(4), 0, 4);
    std::ptr::copy_nonoverlapping(ex.as_ptr().add(8), out.add(8), 64);
    1
}

/// CascadeWindows : en cascade depuis le coin du rectangle (ou de la zone
/// de travail), décalées de la hauteur d'une barre de titre. Sans liste,
/// les fenêtres visibles et non réduites enfants de `parent` (ou de
/// premier niveau).
unsafe extern "win64" fn cascade_windows(
    parent: usize,
    _how: u32,
    area: *const i32,
    count: u32,
    kids: *const usize,
) -> u16 {
    let mut list: Vec<usize> = if !kids.is_null() && count > 0 {
        std::slice::from_raw_parts(kids, count as usize).to_vec()
    } else {
        collect_windows(parent)
    };
    list.retain(|&w| {
        api!("user32", "IsWindowVisible", fn(usize) -> i32, w).unwrap_or(0) != 0
            && api!("user32", "IsIconic", fn(usize) -> i32, w).unwrap_or(0) == 0
    });
    let bounds = rect_at(area).unwrap_or_else(|| {
        let mut r: Rect = [0; 4];
        api!(
            "user32",
            "SystemParametersInfoW",
            fn(u32, u32, *mut Rect, u32) -> i32,
            0x30,
            0,
            &mut r,
            0
        );
        r
    });
    let step = api!("user32", "GetSystemMetrics", fn(i32) -> i32, 4).unwrap_or(23) + 8; // SM_CYCAPTION
    let w = ((bounds[2] - bounds[0]) * 3 / 4).max(100);
    let h = ((bounds[3] - bounds[1]) * 3 / 4).max(80);
    let mut moved = 0u16;
    for (i, &win) in list.iter().enumerate() {
        let off = step * (i as i32 % 10);
        let ok = api!(
            "user32",
            "SetWindowPos",
            fn(usize, usize, i32, i32, i32, i32, u32) -> i32,
            win,
            0,
            bounds[0] + off,
            bounds[1] + off,
            w,
            h,
            0x0004 | 0x0010 // SWP_NOZORDER | SWP_NOACTIVATE
        )
        .unwrap_or(0);
        if ok != 0 {
            moved += 1;
        }
    }
    moved
}

unsafe fn collect_windows(parent: usize) -> Vec<usize> {
    unsafe extern "win64" fn push(hwnd: usize, list: *mut Vec<usize>) -> i32 {
        (*list).push(hwnd);
        1
    }
    let mut list: Vec<usize> = Vec::new();
    let cb = push as unsafe extern "win64" fn(usize, *mut Vec<usize>) -> i32 as usize;
    let lp = &mut list as *mut Vec<usize> as isize;
    if parent != 0 {
        api!(
            "user32",
            "EnumChildWindows",
            fn(usize, usize, isize) -> i32,
            parent,
            cb,
            lp
        );
    } else {
        api!("user32", "EnumWindows", fn(usize, isize) -> i32, cb, lp);
    }
    list
}

/// EnumDesktopWindows : un seul bureau, celui de la session.
unsafe extern "win64" fn enum_desktop_windows(_desk: usize, cb: usize, lp: isize) -> i32 {
    if cb == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    api!("user32", "EnumWindows", fn(usize, isize) -> i32, cb, lp).unwrap_or(0)
}

/// EnumDesktopsW / EnumWindowStationsW : « Default » dans « WinSta0 ».
unsafe extern "win64" fn enum_desktops_w(_winsta: usize, cb: usize, lp: isize) -> i32 {
    enum_one(cb, "Default", lp)
}

unsafe extern "win64" fn enum_window_stations_w(cb: usize, lp: isize) -> i32 {
    enum_one(cb, "WinSta0", lp)
}

unsafe fn enum_one(cb: usize, name: &str, lp: isize) -> i32 {
    if cb == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let f: unsafe extern "win64" fn(*const u16, isize) -> i32 = std::mem::transmute(cb);
    let w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    f(w.as_ptr(), lp)
}

/// SetUserObjectInformationW : seul UOI_FLAGS se règle ; les drapeaux
/// d'une station unique n'ont pas d'effet.
unsafe extern "win64" fn set_user_object_information_w(
    h: usize,
    index: i32,
    info: *const u8,
    len: u32,
) -> i32 {
    if h == 0 || info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match index {
        1 if len >= 12 => 1, // UOI_FLAGS
        _ => fail(ERROR_INVALID_PARAMETER),
    }
}

unsafe extern "win64" fn get_user_object_security(
    h: usize,
    info: *const u32,
    sd: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    let req = if info.is_null() { 0 } else { *info };
    api!(
        "advapi32",
        "GetKernelObjectSecurity",
        fn(usize, u32, *mut u8, u32, *mut u32) -> i32,
        h,
        req,
        sd,
        len,
        needed
    )
    .unwrap_or_else(|| fail(ERROR_NOT_SUPPORTED))
}

unsafe extern "win64" fn set_user_object_security(h: usize, info: *const u32, sd: usize) -> i32 {
    let req = if info.is_null() { 0 } else { *info };
    api!(
        "advapi32",
        "SetKernelObjectSecurity",
        fn(usize, u32, usize) -> i32,
        h,
        req,
        sd
    )
    .unwrap_or_else(|| fail(ERROR_NOT_SUPPORTED))
}

/// UserHandleGrantAccess : aucun objet tâche ne restreint les handles
/// d'interface sous Winlux, l'accès est déjà permis.
extern "win64" fn user_handle_grant_access(_h: usize, _job: usize, _grant: i32) -> i32 {
    1
}

/// EnumPropsExW : propriétés posées par SetProp, dans l'ordre des noms.
unsafe extern "win64" fn enum_props_ex_w(hwnd: usize, cb: usize, lp: usize) -> i32 {
    if cb == 0 {
        return -1;
    }
    let f: unsafe extern "win64" fn(usize, *const u16, usize, usize) -> i32 =
        std::mem::transmute(cb);
    let props = crate::dlls::gui::window_prop_list(hwnd);
    if props.is_empty() {
        return -1;
    }
    let mut last = -1;
    for (key, data) in props {
        let atom = key.strip_prefix('#').and_then(|d| d.parse::<u16>().ok());
        let text: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        let name = match atom {
            Some(a) => a as usize as *const u16,
            None => text.as_ptr(),
        };
        last = f(hwnd, name, data, lp);
        if last == 0 {
            break;
        }
    }
    last
}

/// SetCoalescableTimer : un minuteur ordinaire (la tolérance ne sert qu'à
/// économiser l'énergie).
unsafe extern "win64" fn set_coalescable_timer(
    hwnd: usize,
    id: usize,
    elapse: u32,
    func: usize,
    _tolerance: u32,
) -> usize {
    api!(
        "user32",
        "SetTimer",
        fn(usize, usize, u32, usize) -> usize,
        hwnd,
        id,
        elapse,
        func
    )
    .unwrap_or(0)
}

/// Aucun capteur d'orientation : la préférence est acceptée sans effet,
/// comme sur un écran fixe.
extern "win64" fn set_display_auto_rotation_preferences(_orientation: u32) -> i32 {
    1
}

/// Pas d'écran tactile sous Winlux : aucune entrée tactile n'existe.
extern "win64" fn is_touch_window(_hwnd: usize, _flags: usize) -> i32 {
    0
}

extern "win64" fn no_touch_input(_a: usize, _b: usize, _c: usize, _d: usize) -> i32 {
    fail(ERROR_INVALID_PARAMETER)
}

extern "win64" fn get_touch_input_info(_h: usize, _n: u32, _out: usize, _cb: i32) -> i32 {
    fail(6) // ERROR_INVALID_HANDLE : aucun HTOUCHINPUT n'est jamais émis
}

// ── Menus ───────────────────────────────────────────────────────────────

/// HiliteMenuItem : pose ou retire MFS_HILITE sur l'élément.
unsafe extern "win64" fn hilite_menu_item(_hwnd: usize, menu: usize, item: u32, flags: u32) -> i32 {
    let by_position = flags & 0x400 != 0;
    let state = api!(
        "user32",
        "GetMenuState",
        fn(usize, u32, u32) -> u32,
        menu,
        item,
        flags & 0x400
    )
    .unwrap_or(u32::MAX);
    if state == u32::MAX {
        return 0;
    }
    let hilite = flags & 0x80 != 0;
    let mut mii = [0u8; 80];
    mii[..4].copy_from_slice(&80u32.to_le_bytes());
    mii[4..8].copy_from_slice(&1u32.to_le_bytes()); // MIIM_STATE
    let new = if hilite { state | 0x80 } else { state & !0x80 } & 0xFFFF;
    mii[12..16].copy_from_slice(&new.to_le_bytes());
    api!(
        "user32",
        "SetMenuItemInfoW",
        fn(usize, u32, i32, *const u8) -> i32,
        menu,
        item,
        i32::from(by_position),
        mii.as_ptr()
    )
    .unwrap_or(0)
}

/// GetMenuItemRect : Winlux ne calcule pas la position des éléments de
/// menu hors de son propre rendu ; la fonction échoue comme pour un menu
/// non affiché.
extern "win64" fn get_menu_item_rect(_hwnd: usize, _menu: usize, _item: u32, _out: usize) -> i32 {
    fail(ERROR_INVALID_PARAMETER)
}

// ── Couleurs système ────────────────────────────────────────────────────

fn sys_colors() -> &'static Mutex<HashMap<i32, u32>> {
    static M: std::sync::OnceLock<Mutex<HashMap<i32, u32>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

/// Couleur fixée par SetSysColors dans ce programme.
pub fn sys_color_override(index: i32) -> Option<u32> {
    lock(sys_colors()).get(&index).copied()
}

/// SetSysColors : les couleurs valent pour le programme qui les fixe (le
/// thème de la session reste celui de Paramètres) ; ses fenêtres
/// reçoivent WM_SYSCOLORCHANGE et ses pinceaux système suivent.
unsafe extern "win64" fn set_sys_colors(
    count: i32,
    indexes: *const i32,
    colors: *const u32,
) -> i32 {
    if count < 0 || (count > 0 && (indexes.is_null() || colors.is_null())) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    for i in 0..count as usize {
        let index = *indexes.add(i);
        if !(0..=30).contains(&index) {
            return fail(ERROR_INVALID_PARAMETER);
        }
        lock(sys_colors()).insert(index, *colors.add(i) & 0x00FF_FFFF);
        api!("user32", "GetSysColorBrush", fn(i32) -> usize, index);
    }
    for hwnd in collect_windows(0) {
        api!(
            "user32",
            "PostMessageW",
            fn(usize, u32, usize, isize) -> i32,
            hwnd,
            0x0015,
            0,
            0
        );
    }
    1
}

// ── Presse-papiers ──────────────────────────────────────────────────────

fn clipboard_viewer() -> &'static Mutex<usize> {
    static V: Mutex<usize> = Mutex::new(0);
    &V
}

/// GetClipboardViewer : la dernière fenêtre inscrite par
/// SetClipboardViewer dans ce programme.
extern "win64" fn get_clipboard_viewer() -> usize {
    *lock(clipboard_viewer())
}

pub fn swap_clipboard_viewer(hwnd: usize) -> usize {
    std::mem::replace(&mut *lock(clipboard_viewer()), hwnd)
}

// ── Dessin (user32) ─────────────────────────────────────────────────────

unsafe extern "win64" fn invert_rect(hdc: usize, rect: *const i32) -> i32 {
    let Some(r) = rect_at(rect) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    if r[2] <= r[0] || r[3] <= r[1] {
        return 1;
    }
    api!(
        "gdi32",
        "PatBlt",
        fn(usize, i32, i32, i32, i32, u32) -> i32,
        hdc,
        r[0],
        r[1],
        r[2] - r[0],
        r[3] - r[1],
        DSTINVERT
    )
    .unwrap_or(0)
}

/// PaintDesktop : la zone de découpe du DC prend la couleur du bureau.
unsafe extern "win64" fn paint_desktop(hdc: usize) -> i32 {
    let mut r: Rect = [0; 4];
    api!(
        "gdi32",
        "GetClipBox",
        fn(usize, *mut Rect) -> i32,
        hdc,
        &mut r
    );
    let brush = api!("user32", "GetSysColorBrush", fn(i32) -> usize, 1).unwrap_or(0); // COLOR_DESKTOP
    api!(
        "user32",
        "FillRect",
        fn(usize, *const Rect, usize) -> i32,
        hdc,
        &r,
        brush
    )
    .unwrap_or(0)
}

/// DrawAnimatedRects : l'animation est purement décorative, Windows rend
/// TRUE même quand les animations sont coupées.
extern "win64" fn draw_animated_rects(_hwnd: usize, _ani: i32, _from: usize, _to: usize) -> i32 {
    1
}

/// PrintWindow : copie l'image de la fenêtre (ou de sa zone cliente avec
/// PW_CLIENTONLY) dans le DC donné.
unsafe extern "win64" fn print_window(hwnd: usize, hdc: usize, flags: u32) -> i32 {
    let Some(win) = window_rect(hwnd) else {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    };
    let client_only = flags & 1 != 0;
    let (w, h, src) = if client_only {
        let mut c: Rect = [0; 4];
        api!(
            "user32",
            "GetClientRect",
            fn(usize, *mut Rect) -> i32,
            hwnd,
            &mut c
        );
        (
            c[2] - c[0],
            c[3] - c[1],
            api!("user32", "GetDC", fn(usize) -> usize, hwnd).unwrap_or(0),
        )
    } else {
        (
            win[2] - win[0],
            win[3] - win[1],
            api!("user32", "GetWindowDC", fn(usize) -> usize, hwnd).unwrap_or(0),
        )
    };
    if src == 0 {
        return 0;
    }
    let ok = api!(
        "gdi32",
        "BitBlt",
        fn(usize, i32, i32, i32, i32, usize, i32, i32, u32) -> i32,
        hdc,
        0,
        0,
        w,
        h,
        src,
        0,
        0,
        SRCCOPY
    )
    .unwrap_or(0);
    api!("user32", "ReleaseDC", fn(usize, usize) -> i32, hwnd, src);
    ok
}

fn intersect(a: Rect, b: Rect) -> Rect {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn empty(r: &Rect) -> bool {
    r[2] <= r[0] || r[3] <= r[1]
}

/// Zone découverte d'un défilement : `area` privée de `moved`, bornée par
/// son rectangle englobant (ce que rend lprcUpdate).
fn uncovered(area: Rect, moved: Rect) -> Rect {
    if empty(&moved) {
        return area;
    }
    let full_w = moved[0] <= area[0] && moved[2] >= area[2];
    let full_h = moved[1] <= area[1] && moved[3] >= area[3];
    let mut r = area;
    if full_w {
        if moved[1] > area[1] {
            r[3] = moved[1];
        } else {
            r[1] = moved[3];
        }
    } else if full_h {
        if moved[0] > area[0] {
            r[2] = moved[0];
        } else {
            r[0] = moved[2];
        }
    }
    if empty(&r) {
        [0; 4]
    } else {
        r
    }
}

/// ScrollDC : déplace les pixels de `scroll` (borné par `clip`) de dx, dy
/// et rend la zone découverte.
unsafe extern "win64" fn scroll_dc(
    hdc: usize,
    dx: i32,
    dy: i32,
    scroll: *const i32,
    clip: *const i32,
    update_rgn: usize,
    update: *mut i32,
) -> i32 {
    let mut boxr: Rect = [0; 4];
    api!(
        "gdi32",
        "GetClipBox",
        fn(usize, *mut Rect) -> i32,
        hdc,
        &mut boxr
    );
    let area = rect_at(scroll).unwrap_or(boxr);
    let limit = rect_at(clip).unwrap_or(area);
    let src = intersect(area, limit);
    let dst = intersect([src[0] + dx, src[1] + dy, src[2] + dx, src[3] + dy], limit);
    if !empty(&dst) {
        api!(
            "gdi32",
            "BitBlt",
            fn(usize, i32, i32, i32, i32, usize, i32, i32, u32) -> i32,
            hdc,
            dst[0],
            dst[1],
            dst[2] - dst[0],
            dst[3] - dst[1],
            hdc,
            dst[0] - dx,
            dst[1] - dy,
            SRCCOPY
        );
    }
    let gap = uncovered(src, dst);
    if !update.is_null() {
        std::ptr::write_unaligned(update as *mut Rect, gap);
    }
    if update_rgn != 0 {
        let a = api!(
            "gdi32",
            "CreateRectRgnIndirect",
            fn(*const Rect) -> usize,
            &src
        )
        .unwrap_or(0);
        let b = api!(
            "gdi32",
            "CreateRectRgnIndirect",
            fn(*const Rect) -> usize,
            &dst
        )
        .unwrap_or(0);
        api!(
            "gdi32",
            "CombineRgn",
            fn(usize, usize, usize, i32) -> i32,
            update_rgn,
            a,
            b,
            4
        ); // RGN_DIFF
        for r in [a, b] {
            api!("gdi32", "DeleteObject", fn(usize) -> i32, r);
        }
    }
    1
}

// ── DlgDirList ──────────────────────────────────────────────────────────

const DDL_READONLY: u32 = 0x1;
const DDL_HIDDEN: u32 = 0x2;
const DDL_DIRECTORY: u32 = 0x10;
const DDL_DRIVES: u32 = 0x4000;
const DDL_EXCLUSIVE: u32 = 0x8000;

/// Sépare « C:\dossier\*.txt » en dossier et motif ; un dossier seul
/// garde le motif « *.* ».
fn split_spec(spec: &str) -> (Option<String>, String) {
    let spec = if spec.is_empty() { "*.*" } else { spec };
    if !spec.contains(['*', '?']) && crate::paths::to_linux(spec).is_dir() {
        return (Some(spec.to_string()), "*.*".into());
    }
    match spec.rfind(['\\', '/']) {
        Some(i) => {
            let dir = if i == 2 && spec.as_bytes().get(1) == Some(&b':') {
                &spec[..=i]
            } else {
                &spec[..i.max(1)]
            };
            (Some(dir.to_string()), spec[i + 1..].to_string())
        }
        None if spec.len() >= 2 && spec.as_bytes()[1] == b':' => {
            (Some(spec[..2].to_string()), spec[2..].to_string())
        }
        None => (None, spec.to_string()),
    }
}

/// Les entrées d'une liste DlgDirList : fichiers, « [dossier] », « [-c-] ».
pub fn dir_entries(dir: &std::path::Path, pattern: &str, attrs: u32, root: bool) -> Vec<String> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    if attrs & DDL_DIRECTORY != 0 && !root {
        dirs.push("[..]".to_string());
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let hidden = name.starts_with('.');
            if hidden && attrs & DDL_HIDDEN == 0 {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if attrs & DDL_DIRECTORY != 0 {
                    dirs.push(format!("[{name}]"));
                }
                continue;
            }
            if attrs & DDL_EXCLUSIVE != 0 {
                let ro = e
                    .metadata()
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(false);
                let wanted =
                    (ro && attrs & DDL_READONLY != 0) || (hidden && attrs & DDL_HIDDEN != 0);
                if !wanted {
                    continue;
                }
            }
            if crate::setup::wildcard(pattern, &name) {
                files.push(name);
            }
        }
    }
    files.sort_by_key(|n| n.to_lowercase());
    dirs.sort_by_key(|n| n.to_lowercase());
    let mut out = if attrs & DDL_EXCLUSIVE != 0 && attrs & DDL_DIRECTORY != 0 {
        Vec::new()
    } else {
        files
    };
    out.extend(dirs);
    if attrs & DDL_DRIVES != 0 {
        out.push("[-c-]".into());
    }
    out
}

unsafe fn dlg_dir_fill(
    dlg: usize,
    path: *mut u16,
    list_id: i32,
    static_id: i32,
    attrs: u32,
    combo: bool,
) -> i32 {
    let spec = if path.is_null() {
        String::new()
    } else {
        wstr(path)
    };
    let (dir, pattern) = split_spec(&spec);
    if let Some(d) = dir {
        let w: Vec<u16> = d.encode_utf16().chain(std::iter::once(0)).collect();
        if api!(
            "kernel32",
            "SetCurrentDirectoryW",
            fn(*const u16) -> i32,
            w.as_ptr()
        ) != Some(1)
        {
            return 0;
        }
    }
    if !path.is_null() {
        let cap = spec.encode_utf16().count().max(pattern.len()) as u32 + 1;
        crate::dlls::put_w(&pattern, path, cap);
    }
    let mut buf = vec![0u16; 1024];
    let n = api!(
        "kernel32",
        "GetCurrentDirectoryW",
        fn(u32, *mut u16) -> u32,
        1024,
        buf.as_mut_ptr()
    )
    .unwrap_or(0) as usize;
    let cwd = String::from_utf16_lossy(&buf[..n.min(1024)]);
    let root = cwd.len() <= 3;
    if static_id != 0 {
        let w: Vec<u16> = cwd.encode_utf16().chain(std::iter::once(0)).collect();
        api!(
            "user32",
            "SetDlgItemTextW",
            fn(usize, i32, *const u16) -> i32,
            dlg,
            static_id,
            w.as_ptr()
        );
    }
    if list_id == 0 {
        return 1;
    }
    let (reset, add) = if combo {
        (0x14B, 0x143)
    } else {
        (0x184, 0x180)
    };
    let send = |msg: u32, lp: isize| {
        api!(
            "user32",
            "SendDlgItemMessageW",
            fn(usize, i32, u32, usize, isize) -> isize,
            dlg,
            list_id,
            msg,
            0,
            lp
        )
    };
    send(reset, 0);
    for entry in dir_entries(&crate::paths::to_linux(&cwd), &pattern, attrs, root) {
        let w: Vec<u16> = entry.encode_utf16().chain(std::iter::once(0)).collect();
        send(add, w.as_ptr() as isize);
    }
    1
}

unsafe extern "win64" fn dlg_dir_list_w(
    dlg: usize,
    path: *mut u16,
    list: i32,
    stat: i32,
    attrs: u32,
) -> i32 {
    dlg_dir_fill(dlg, path, list, stat, attrs, false)
}

unsafe extern "win64" fn dlg_dir_list_combo_box_w(
    dlg: usize,
    path: *mut u16,
    list: i32,
    stat: i32,
    attrs: u32,
) -> i32 {
    dlg_dir_fill(dlg, path, list, stat, attrs, true)
}

/// DlgDirSelectEx : texte de l'élément choisi, sans crochets ; rend TRUE
/// pour un dossier ou un lecteur (« c: »).
unsafe fn dlg_dir_select(dlg: usize, out: *mut u16, cap: i32, id: i32, combo: bool) -> i32 {
    let (get_sel, get_text) = if combo {
        (0x147, 0x148)
    } else {
        (0x188, 0x189)
    };
    let send = |msg: u32, wp: usize, lp: isize| {
        api!(
            "user32",
            "SendDlgItemMessageW",
            fn(usize, i32, u32, usize, isize) -> isize,
            dlg,
            id,
            msg,
            wp,
            lp
        )
        .unwrap_or(-1)
    };
    let sel = send(get_sel, 0, 0);
    if sel < 0 {
        return 0;
    }
    let mut buf = vec![0u16; 1024];
    let n = send(get_text, sel as usize, buf.as_mut_ptr() as isize);
    let text = String::from_utf16_lossy(&buf[..n.clamp(0, 1023) as usize]);
    let (value, is_dir) = match text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        Some(inner) => match inner.strip_prefix('-').and_then(|d| d.strip_suffix('-')) {
            Some(drive) => (format!("{drive}:"), true),
            None => (format!("{inner}\\"), true),
        },
        None => (text, false),
    };
    if !out.is_null() && cap > 0 {
        let w: Vec<u16> = value.encode_utf16().take(cap as usize - 1).collect();
        std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
        *out.add(w.len()) = 0;
    }
    i32::from(is_dir)
}

unsafe extern "win64" fn dlg_dir_select_ex_w(dlg: usize, out: *mut u16, cap: i32, id: i32) -> i32 {
    dlg_dir_select(dlg, out, cap, id, false)
}

unsafe extern "win64" fn dlg_dir_select_combo_box_ex_w(
    dlg: usize,
    out: *mut u16,
    cap: i32,
    id: i32,
) -> i32 {
    dlg_dir_select(dlg, out, cap, id, true)
}

// ── Écrans ──────────────────────────────────────────────────────────────

/// EnumDisplayDevicesA : la version W convertie (DISPLAY_DEVICEA, 424
/// octets).
unsafe extern "win64" fn enum_display_devices_a(
    device: *const u8,
    index: u32,
    out: *mut u8,
    flags: u32,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    let name: Option<Vec<u16>> = (!device.is_null()).then(|| {
        cstr(device)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    });
    let mut w = vec![0u8; 840];
    w[..4].copy_from_slice(&840u32.to_le_bytes());
    let ok = api!(
        "user32",
        "EnumDisplayDevicesW",
        fn(*const u16, u32, *mut u8, u32) -> i32,
        name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
        index,
        w.as_mut_ptr(),
        flags
    )
    .unwrap_or(0);
    if ok == 0 {
        return 0;
    }
    let field = |off: usize, chars: usize| -> String {
        let s: Vec<u16> = w[off..off + chars * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&s)
    };
    let put = |off: usize, chars: usize, s: &str| {
        let mut a = crate::dlls::encode_ansi(s);
        a.truncate(chars - 1);
        std::ptr::write_bytes(out.add(off), 0, chars);
        std::ptr::copy_nonoverlapping(a.as_ptr(), out.add(off), a.len());
    };
    put(4, 32, &field(4, 32));
    put(36, 128, &field(68, 128));
    let state = u32::from_le_bytes(w[324..328].try_into().unwrap_or_default());
    std::ptr::write_unaligned(out.add(164) as *mut u32, state);
    put(168, 128, &field(328, 128));
    put(296, 128, &field(584, 128));
    1
}

// ── gdi32 ───────────────────────────────────────────────────────────────

/// ExtEscape : Winlux ne prend en charge aucune séquence d'échappement de
/// pilote ; QUERYESCSUPPORT le dit, les autres rendent 0 (« non
/// implémentée »), comme un pilote sans échappement.
extern "win64" fn ext_escape(
    _hdc: usize,
    _code: i32,
    _cb_in: i32,
    _input: usize,
    _cb_out: i32,
    _out: usize,
) -> i32 {
    0
}

extern "win64" fn cancel_dc(_hdc: usize) -> i32 {
    1
}

fn arc_directions() -> &'static Mutex<HashMap<usize, i32>> {
    static M: std::sync::OnceLock<Mutex<HashMap<usize, i32>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

/// Sens des arcs par DC : AD_COUNTERCLOCKWISE (1) par défaut.
extern "win64" fn get_arc_direction(hdc: usize) -> i32 {
    if hdc == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    lock(arc_directions()).get(&hdc).copied().unwrap_or(1)
}

extern "win64" fn set_arc_direction(hdc: usize, dir: i32) -> i32 {
    if hdc == 0 || !(1..=2).contains(&dir) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    lock(arc_directions()).insert(hdc, dir).unwrap_or(1)
}

/// XFORM : eM11, eM12, eM21, eM22, eDx, eDy.
type Xform = [f32; 6];

fn combine(a: &Xform, b: &Xform) -> Xform {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

unsafe extern "win64" fn combine_transform(
    out: *mut Xform,
    a: *const Xform,
    b: *const Xform,
) -> i32 {
    if out.is_null() || a.is_null() || b.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let r = combine(&std::ptr::read_unaligned(a), &std::ptr::read_unaligned(b));
    std::ptr::write_unaligned(out, r);
    1
}

unsafe extern "win64" fn create_elliptic_rgn_indirect(r: *const i32) -> usize {
    let Some(r) = rect_at(r) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    api!(
        "gdi32",
        "CreateEllipticRgn",
        fn(i32, i32, i32, i32) -> usize,
        r[0],
        r[1],
        r[2],
        r[3]
    )
    .unwrap_or(0)
}

/// InvertRgn : inverse chaque rectangle de la région (GetRegionData).
unsafe extern "win64" fn invert_rgn(hdc: usize, rgn: usize) -> i32 {
    let size = api!(
        "gdi32",
        "GetRegionData",
        fn(usize, u32, *mut u8) -> u32,
        rgn,
        0,
        std::ptr::null_mut()
    )
    .unwrap_or(0);
    if size < 32 {
        return 0;
    }
    let mut data = vec![0u8; size as usize];
    api!(
        "gdi32",
        "GetRegionData",
        fn(usize, u32, *mut u8) -> u32,
        rgn,
        size,
        data.as_mut_ptr()
    );
    let count = u32::from_le_bytes(data[8..12].try_into().unwrap_or_default()) as usize;
    for i in 0..count {
        let at = 32 + i * 16;
        let Some(raw) = data.get(at..at + 16) else {
            break;
        };
        let v: Vec<i32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i32::from_le_bytes(*c))
            .collect();
        let r: Rect = [v[0], v[1], v[2], v[3]];
        invert_rect(hdc, r.as_ptr());
    }
    1
}

/// PlgBlt : un parallélogramme aligné sur les axes (le cas des copies,
/// miroirs et agrandissements) passe par StretchBlt ou MaskBlt ; une
/// rotation ou un cisaillement n'est pas pris en charge.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn plg_blt(
    hdc: usize,
    pts: *const [i32; 2],
    src: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    mask: usize,
    xm: i32,
    ym: i32,
) -> i32 {
    if pts.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let p = std::slice::from_raw_parts(pts, 3);
    // p[0] haut gauche, p[1] haut droit, p[2] bas gauche.
    if p[0][1] != p[1][1] || p[0][0] != p[2][0] {
        return fail(ERROR_NOT_SUPPORTED);
    }
    let dw = p[1][0] - p[0][0];
    let dh = p[2][1] - p[0][1];
    if mask != 0 && dw == w && dh == h {
        return api!(
            "gdi32",
            "MaskBlt",
            fn(usize, i32, i32, i32, i32, usize, i32, i32, usize, i32, i32, u32) -> i32,
            hdc,
            p[0][0],
            p[0][1],
            w,
            h,
            src,
            x,
            y,
            mask,
            xm,
            ym,
            0xAACC_0020 // MAKEROP4(SRCCOPY, dest)
        )
        .unwrap_or(0);
    }
    api!(
        "gdi32",
        "StretchBlt",
        fn(usize, i32, i32, i32, i32, usize, i32, i32, i32, i32, u32) -> i32,
        hdc,
        p[0][0],
        p[0][1],
        dw,
        dh,
        src,
        x,
        y,
        w,
        h,
        SRCCOPY
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn scale_window_ext_ex(
    hdc: usize,
    xn: i32,
    xd: i32,
    yn: i32,
    yd: i32,
    old: *mut [i32; 2],
) -> i32 {
    if xd == 0 || yd == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut cur = [0i32; 2];
    if api!(
        "gdi32",
        "GetWindowExtEx",
        fn(usize, *mut [i32; 2]) -> i32,
        hdc,
        &mut cur
    ) != Some(1)
    {
        return 0;
    }
    if !old.is_null() {
        *old = cur;
    }
    api!(
        "gdi32",
        "SetWindowExtEx",
        fn(usize, i32, i32, *mut [i32; 2]) -> i32,
        hdc,
        cur[0] * xn / xd,
        cur[1] * yn / yd,
        std::ptr::null_mut()
    )
    .unwrap_or(0)
}

/// CreateMetaFileW : un métafichier en mémoire, porté par un métafichier
/// amélioré ; l'écriture d'un .wmf sur disque n'est pas prise en charge.
unsafe extern "win64" fn create_meta_file_w(file: *const u16) -> usize {
    if !file.is_null() && *file != 0 {
        set_last_error(ERROR_NOT_SUPPORTED);
        return 0;
    }
    api!(
        "gdi32",
        "CreateEnhMetaFileW",
        fn(usize, *const u16, usize, *const u16) -> usize,
        0,
        std::ptr::null(),
        0,
        std::ptr::null()
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn close_meta_file(hdc: usize) -> usize {
    api!("gdi32", "CloseEnhMetaFile", fn(usize) -> usize, hdc).unwrap_or(0)
}

/// GetGlyphOutlineA : le caractère (un ou deux octets) converti, puis la
/// version W.
unsafe extern "win64" fn get_glyph_outline_a(
    hdc: usize,
    ch: u32,
    format: u32,
    metrics: usize,
    cb: u32,
    buf: usize,
    mat: usize,
) -> u32 {
    const GGO_GLYPH_INDEX: u32 = 0x80;
    let wide = if format & GGO_GLYPH_INDEX != 0 {
        ch
    } else {
        let bytes: Vec<u8> = if ch > 0xFF {
            vec![(ch >> 8) as u8, ch as u8]
        } else {
            vec![ch as u8]
        };
        let text = crate::dlls::decode_ansi(&bytes);
        text.encode_utf16().next().map_or(ch, u32::from)
    };
    api!(
        "gdi32",
        "GetGlyphOutlineW",
        fn(usize, u32, u32, usize, u32, usize, usize) -> u32,
        hdc,
        wide,
        format,
        metrics,
        cb,
        buf,
        mat
    )
    .unwrap_or(u32::MAX)
}

/// GetCharacterPlacementA : la chaîne convertie et GCP_RESULTSA rempli par
/// la version W (même disposition, sauf lpOutString en octets).
unsafe extern "win64" fn get_character_placement_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    max_extent: i32,
    results: *mut u8,
    flags: u32,
) -> u32 {
    if text.is_null() || count < 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let bytes = std::slice::from_raw_parts(text, count as usize);
    let s = crate::dlls::decode_ansi(bytes);
    let w: Vec<u16> = s.encode_utf16().collect();
    let mut copy = [0u8; 64];
    let mut out_w = vec![0u16; w.len() + 1];
    let out_a = if results.is_null() {
        std::ptr::null_mut()
    } else {
        std::ptr::copy_nonoverlapping(results, copy.as_mut_ptr(), 64);
        let p = std::ptr::read_unaligned(results.add(8) as *const *mut u8);
        if !p.is_null() {
            std::ptr::write_unaligned(
                copy.as_mut_ptr().add(8) as *mut *mut u16,
                out_w.as_mut_ptr(),
            );
        }
        p
    };
    let r = api!(
        "gdi32",
        "GetCharacterPlacementW",
        fn(usize, *const u16, i32, i32, *mut u8, u32) -> u32,
        hdc,
        w.as_ptr(),
        w.len() as i32,
        max_extent,
        if results.is_null() {
            std::ptr::null_mut()
        } else {
            copy.as_mut_ptr()
        },
        flags
    )
    .unwrap_or(0);
    if !results.is_null() {
        // Recopie nGlyphs et nMaxFit, puis la chaîne de sortie en ANSI.
        std::ptr::copy_nonoverlapping(copy.as_ptr().add(56), results.add(56), 8);
        if !out_a.is_null() {
            let n = out_w.iter().position(|&c| c == 0).unwrap_or(out_w.len());
            let a = crate::dlls::encode_ansi(&String::from_utf16_lossy(&out_w[..n]));
            let room = count as usize;
            std::ptr::copy_nonoverlapping(a.as_ptr(), out_a, a.len().min(room));
        }
    }
    r
}

/// Plages Unicode d'une table « cmap » (formats 4 et 12).
pub fn cmap_ranges(cmap: &[u8]) -> Vec<(u32, u32)> {
    let u16_at = |o: usize| cmap.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |o: usize| {
        cmap.get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut best: Option<(u16, usize)> = None;
    for i in 0..u16_at(2).unwrap_or(0) as usize {
        let rec = 4 + i * 8;
        let (Some(pid), Some(eid), Some(off)) = (u16_at(rec), u16_at(rec + 2), u32_at(rec + 4))
        else {
            break;
        };
        let off = off as usize;
        let fmt = u16_at(off).unwrap_or(0);
        let ok = (pid == 3 && (eid == 1 || eid == 10)) || pid == 0;
        if ok && (fmt == 4 || fmt == 12) && best.is_none_or(|(f, _)| fmt > f) {
            best = Some((fmt, off));
        }
    }
    let mut chars: Vec<(u32, u32)> = Vec::new();
    match best {
        Some((4, off)) => {
            let segs = u16_at(off + 6).unwrap_or(0) as usize / 2;
            let ends = off + 14;
            let starts = ends + segs * 2 + 2;
            let deltas = starts + segs * 2;
            let ranges = deltas + segs * 2;
            for s in 0..segs {
                let (Some(end), Some(start)) = (u16_at(ends + s * 2), u16_at(starts + s * 2))
                else {
                    break;
                };
                if start == 0xFFFF {
                    continue;
                }
                let delta = u16_at(deltas + s * 2).unwrap_or(0);
                let ro = u16_at(ranges + s * 2).unwrap_or(0) as usize;
                for c in start..=end {
                    let glyph = if ro == 0 {
                        c.wrapping_add(delta)
                    } else {
                        let at = ranges + s * 2 + ro + (c - start) as usize * 2;
                        match u16_at(at).unwrap_or(0) {
                            0 => 0,
                            g => g.wrapping_add(delta),
                        }
                    };
                    if glyph != 0 {
                        chars.push((c as u32, 1));
                    }
                }
            }
        }
        Some((12, off)) => {
            let groups = u32_at(off + 12).unwrap_or(0) as usize;
            for g in 0..groups.min(100_000) {
                let at = off + 16 + g * 12;
                let (Some(a), Some(b)) = (u32_at(at), u32_at(at + 4)) else {
                    break;
                };
                if b >= a {
                    chars.push((a, b - a + 1));
                }
            }
        }
        _ => {}
    }
    // Fusion des plages contiguës, limitées au plan de base (WCHAR).
    let mut out: Vec<(u32, u32)> = Vec::new();
    chars.sort_unstable();
    for (start, n) in chars {
        if start > 0xFFFF {
            continue;
        }
        let n = n.min(0x1_0000 - start);
        match out.last_mut() {
            Some((s, len)) if *s + *len >= start => {
                *len = (*len).max(start + n - *s);
            }
            _ => out.push((start, n)),
        }
    }
    out
}

/// GetFontUnicodeRanges : GLYPHSET de la police du DC, lu dans sa table
/// « cmap ».
unsafe extern "win64" fn get_font_unicode_ranges(hdc: usize, out: *mut u8) -> u32 {
    let tag = u32::from_le_bytes(*b"cmap");
    let size = api!(
        "gdi32",
        "GetFontData",
        fn(usize, u32, u32, *mut u8, u32) -> u32,
        hdc,
        tag,
        0,
        std::ptr::null_mut(),
        0
    )
    .unwrap_or(u32::MAX);
    if size == u32::MAX || size == 0 {
        return 0;
    }
    let mut cmap = vec![0u8; size as usize];
    api!(
        "gdi32",
        "GetFontData",
        fn(usize, u32, u32, *mut u8, u32) -> u32,
        hdc,
        tag,
        0,
        cmap.as_mut_ptr(),
        size
    );
    // Les plages de plus de 0xFFFF caractères sont coupées (cGlyphs est un
    // WORD).
    let mut ranges: Vec<(u16, u16)> = Vec::new();
    for (s, n) in cmap_ranges(&cmap) {
        let mut start = s;
        let mut left = n;
        while left > 0 {
            let take = left.min(0xFFFF);
            ranges.push((start as u16, take as u16));
            start += take;
            left -= take;
        }
    }
    let need = 16 + ranges.len() * 4;
    if out.is_null() {
        return need as u32;
    }
    let total: u32 = ranges.iter().map(|r| u32::from(r.1)).sum();
    std::ptr::write_unaligned(out as *mut u32, need as u32);
    std::ptr::write_unaligned(out.add(4) as *mut u32, 0);
    std::ptr::write_unaligned(out.add(8) as *mut u32, total);
    std::ptr::write_unaligned(out.add(12) as *mut u32, ranges.len() as u32);
    for (i, (s, n)) in ranges.iter().enumerate() {
        std::ptr::write_unaligned(out.add(16 + i * 4) as *mut u16, *s);
        std::ptr::write_unaligned(out.add(18 + i * 4) as *mut u16, *n);
    }
    need as u32
}

// ── uxtheme ─────────────────────────────────────────────────────────────

const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;

/// DrawThemeBackgroundEx (ordinal 47) : DrawThemeBackground avec, si
/// DTBG_CLIPRECT est posé, le rectangle de découpe de DTBGOPTS.
unsafe extern "win64" fn draw_theme_background_ex(
    theme: usize,
    hdc: usize,
    part: i32,
    state: i32,
    rect: usize,
    opts: *const u8,
) -> i32 {
    let clip = if !opts.is_null() && std::ptr::read_unaligned(opts.add(4) as *const u32) & 1 != 0 {
        opts.add(8) as usize
    } else {
        0
    };
    crate::dlls::uxtheme::DrawThemeBackground(theme, hdc, part, state, rect, clip)
}

/// Aucun tampon de dessin n'est jamais créé (BeginBufferedPaint échoue) :
/// tout HPAINTBUFFER est invalide.
unsafe extern "win64" fn get_buffered_paint_bits(
    _h: usize,
    bits: *mut usize,
    row: *mut i32,
) -> i32 {
    if !bits.is_null() {
        *bits = 0;
    }
    if !row.is_null() {
        *row = 0;
    }
    E_INVALIDARG
}

extern "win64" fn get_buffered_paint_target_rect(_h: usize, _rect: usize) -> i32 {
    E_INVALIDARG
}

/// SetWindowThemeAttribute : WTA_NONCLIENT (WTA_OPTIONS de 8 octets).
/// Le cadre Winlux ne dessine ni icône ni menu système dans la barre de
/// titre d'un programme qui les masque déjà ; l'attribut est accepté.
unsafe extern "win64" fn set_window_theme_attribute(
    hwnd: usize,
    attr: u32,
    value: usize,
    cb: u32,
) -> i32 {
    if !is_window(hwnd) {
        return E_INVALIDARG;
    }
    if attr != 1 || value == 0 || cb != 8 {
        return E_INVALIDARG;
    }
    0
}

pub fn user32_exports() -> Exports {
    crate::exports! {
        "GetCursorInfo" => fn get_cursor_info,
        "GetKeyboardType" => fn get_keyboard_type,
        "SetDoubleClickTime" => fn set_double_click_time,
        "CloseWindow" => fn close_window,
        "GetWindowInfo" => fn get_window_info,
        "IsHungAppWindow" => fn is_hung_app_window,
        "SetWindowWord" => fn set_window_word,
        "SetClassWord" => fn set_class_word,
        "GetClassInfoA" => fn get_class_info_a,
        "CascadeWindows" => fn cascade_windows,
        "EnumDesktopWindows" => fn enum_desktop_windows,
        "EnumDesktopsW" => fn enum_desktops_w,
        "EnumWindowStationsW" => fn enum_window_stations_w,
        "SetUserObjectInformationW" => fn set_user_object_information_w,
        "GetUserObjectSecurity" => fn get_user_object_security,
        "SetUserObjectSecurity" => fn set_user_object_security,
        "UserHandleGrantAccess" => fn user_handle_grant_access,
        "EnumPropsExW" => fn enum_props_ex_w,
        "SetCoalescableTimer" => fn set_coalescable_timer,
        "SetDisplayAutoRotationPreferences" => fn set_display_auto_rotation_preferences,
        "IsTouchWindow" => fn is_touch_window,
        "GetPointerFrameTouchInfo" => fn no_touch_input,
        "GetPointerFrameTouchInfoHistory" => fn no_touch_input,
        "GetTouchInputInfo" => fn get_touch_input_info,
        "HiliteMenuItem" => fn hilite_menu_item,
        "GetMenuItemRect" => fn get_menu_item_rect,
        "GetClipboardViewer" => fn get_clipboard_viewer,
        "SetSysColors" => fn set_sys_colors,
        "InvertRect" => fn invert_rect,
        "PaintDesktop" => fn paint_desktop,
        "DrawAnimatedRects" => fn draw_animated_rects,
        "PrintWindow" => fn print_window,
        "ScrollDC" => fn scroll_dc,
        "DlgDirListW" => fn dlg_dir_list_w,
        "DlgDirListComboBoxW" => fn dlg_dir_list_combo_box_w,
        "DlgDirSelectExW" => fn dlg_dir_select_ex_w,
        "DlgDirSelectComboBoxExW" => fn dlg_dir_select_combo_box_ex_w,
        "EnumDisplayDevicesA" => fn enum_display_devices_a,
    }
}

pub fn gdi32_exports() -> Exports {
    crate::exports! {
        "ExtEscape" => fn ext_escape,
        "CancelDC" => fn cancel_dc,
        "GetArcDirection" => fn get_arc_direction,
        "SetArcDirection" => fn set_arc_direction,
        "CombineTransform" => fn combine_transform,
        "CreateEllipticRgnIndirect" => fn create_elliptic_rgn_indirect,
        "InvertRgn" => fn invert_rgn,
        "PlgBlt" => fn plg_blt,
        "ScaleWindowExtEx" => fn scale_window_ext_ex,
        "CreateMetaFileW" => fn create_meta_file_w,
        "CloseMetaFile" => fn close_meta_file,
        "GetGlyphOutlineA" => fn get_glyph_outline_a,
        "GetCharacterPlacementA" => fn get_character_placement_a,
        "GetFontUnicodeRanges" => fn get_font_unicode_ranges,
    }
}

pub fn uxtheme_exports() -> Exports {
    crate::exports! {
        "DrawThemeBackgroundEx" => fn draw_theme_background_ex,
        "GetBufferedPaintBits" => fn get_buffered_paint_bits,
        "GetBufferedPaintTargetRect" => fn get_buffered_paint_target_rect,
        "SetWindowThemeAttribute" => fn set_window_theme_attribute,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transformations_composees() {
        let scale: Xform = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        let shift: Xform = [1.0, 0.0, 0.0, 1.0, 10.0, 20.0];
        // D'abord l'échelle, puis la translation.
        assert_eq!(combine(&scale, &shift), [2.0, 0.0, 0.0, 3.0, 10.0, 20.0]);
        // D'abord la translation, puis l'échelle : la translation grandit.
        assert_eq!(combine(&shift, &scale), [2.0, 0.0, 0.0, 3.0, 20.0, 60.0]);
    }

    #[test]
    fn zone_decouverte_par_un_defilement() {
        let area = [0, 0, 100, 50];
        // Défilement de 10 vers le haut : la bande du bas se découvre.
        assert_eq!(uncovered(area, [0, 0, 100, 40]), [0, 40, 100, 50]);
        // Vers la droite : la bande de gauche.
        assert_eq!(uncovered(area, [15, 0, 100, 50]), [0, 0, 15, 50]);
        assert_eq!(uncovered(area, [0, 0, 0, 0]), area);
    }

    #[test]
    fn motifs_de_dlgdirlist() {
        assert_eq!(split_spec("*.txt"), (None, "*.txt".into()));
        assert_eq!(
            split_spec("C:\\docs\\*.txt"),
            (Some("C:\\docs".into()), "*.txt".into())
        );
        assert_eq!(
            split_spec("C:\\*.ini"),
            (Some("C:\\".into()), "*.ini".into())
        );
        let dir = std::env::temp_dir().join(format!("winlux-dlgdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sous")).unwrap();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        std::fs::write(dir.join("a.TXT"), "x").unwrap();
        std::fs::write(dir.join("c.ini"), "x").unwrap();
        let got = dir_entries(&dir, "*.txt", DDL_DIRECTORY | DDL_DRIVES, false);
        assert_eq!(got, ["a.TXT", "b.txt", "[..]", "[Sous]", "[-c-]"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plages_unicode_d_une_cmap_format_4() {
        // Une table cmap (3,1) au format 4 : 'A'..'C' et le segment final.
        let mut sub = Vec::new();
        let segs = 2u16;
        for v in [4u16, 0, 0, segs * 2, 0, 0, 0] {
            sub.extend_from_slice(&v.to_be_bytes());
        }
        for v in [0x43u16, 0xFFFF] {
            sub.extend_from_slice(&v.to_be_bytes()); // endCode
        }
        sub.extend_from_slice(&0u16.to_be_bytes());
        for v in [0x41u16, 0xFFFF] {
            sub.extend_from_slice(&v.to_be_bytes()); // startCode
        }
        for v in [(-0x40i16) as u16, 1] {
            sub.extend_from_slice(&v.to_be_bytes()); // idDelta
        }
        for _ in 0..segs {
            sub.extend_from_slice(&0u16.to_be_bytes()); // idRangeOffset
        }
        let mut cmap = Vec::new();
        for v in [0u16, 1, 3, 1] {
            cmap.extend_from_slice(&v.to_be_bytes());
        }
        cmap.extend_from_slice(&12u32.to_be_bytes());
        cmap.extend_from_slice(&sub);
        assert_eq!(cmap_ranges(&cmap), [(0x41, 3)]);
    }
}
