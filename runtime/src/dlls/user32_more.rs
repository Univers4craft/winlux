//! user32 : fonctions complémentaires demandées par wxWidgets (Audacity) :
//! informations de menu, MDI, DrawState, régions de mise à jour, mise en
//! page du processus, animation de fenêtre, saisie simulée.

use super::lock;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Fonction exportée par Winlux, cherchée dans user32 puis gdi32.
fn export(name: &str) -> Option<usize> {
    super::lookup("user32", name).or_else(|| super::lookup("gdi32", name))
}

macro_rules! call {
    ($name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match export($name) {
            Some(addr) => {
                // SAFETY: adresse de la table d'exports de Winlux, signature
                // de l'API documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// --- MENUINFO ---------------------------------------------------------------------------

/// MENUINFO sans cbSize : fMask, dwStyle, cyMax, hbrBack, dwContextHelpID, dwMenuData.
#[derive(Clone, Copy, Default)]
struct MenuInfo {
    style: u32,
    cy_max: u32,
    back: usize,
    help: u32,
    data: usize,
}

fn menu_infos() -> &'static Mutex<HashMap<usize, MenuInfo>> {
    static M: OnceLock<Mutex<HashMap<usize, MenuInfo>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

const MIM_MAXHEIGHT: u32 = 0x01;
const MIM_BACKGROUND: u32 = 0x02;
const MIM_HELPID: u32 = 0x04;
const MIM_MENUDATA: u32 = 0x08;
const MIM_STYLE: u32 = 0x10;

unsafe extern "win64" fn set_menu_info(menu: usize, mi: *const u8) -> i32 {
    if menu == 0 || mi.is_null() {
        return 0;
    }
    let mask = (mi.add(4) as *const u32).read_unaligned();
    let mut m = lock(menu_infos());
    let e = m.entry(menu).or_default();
    if mask & MIM_STYLE != 0 {
        e.style = (mi.add(8) as *const u32).read_unaligned();
    }
    if mask & MIM_MAXHEIGHT != 0 {
        e.cy_max = (mi.add(12) as *const u32).read_unaligned();
    }
    if mask & MIM_BACKGROUND != 0 {
        e.back = (mi.add(16) as *const usize).read_unaligned();
    }
    if mask & MIM_HELPID != 0 {
        e.help = (mi.add(24) as *const u32).read_unaligned();
    }
    if mask & MIM_MENUDATA != 0 {
        e.data = (mi.add(32) as *const usize).read_unaligned();
    }
    1
}

unsafe extern "win64" fn get_menu_info(menu: usize, mi: *mut u8) -> i32 {
    if menu == 0 || mi.is_null() {
        return 0;
    }
    let mask = (mi.add(4) as *const u32).read_unaligned();
    let e = lock(menu_infos()).get(&menu).copied().unwrap_or_default();
    if mask & MIM_STYLE != 0 {
        (mi.add(8) as *mut u32).write_unaligned(e.style);
    }
    if mask & MIM_MAXHEIGHT != 0 {
        (mi.add(12) as *mut u32).write_unaligned(e.cy_max);
    }
    if mask & MIM_BACKGROUND != 0 {
        (mi.add(16) as *mut usize).write_unaligned(e.back);
    }
    if mask & MIM_HELPID != 0 {
        (mi.add(24) as *mut u32).write_unaligned(e.help);
    }
    if mask & MIM_MENUDATA != 0 {
        (mi.add(32) as *mut usize).write_unaligned(e.data);
    }
    1
}

// --- Casse ANSI -------------------------------------------------------------------------

/// CharLowerA / CharUpperA : chaîne en place, ou un seul caractère quand le
/// mot haut du pointeur est nul.
unsafe fn char_case(p: usize, upper: bool) -> usize {
    let conv = |b: u8| {
        if upper {
            b.to_ascii_uppercase()
        } else {
            b.to_ascii_lowercase()
        }
    };
    if p >> 16 == 0 {
        return conv(p as u8) as usize;
    }
    let mut q = p as *mut u8;
    while *q != 0 {
        *q = conv(*q);
        q = q.add(1);
    }
    p
}

unsafe extern "win64" fn char_lower_a(p: usize) -> usize {
    char_case(p, false)
}

unsafe extern "win64" fn char_upper_a(p: usize) -> usize {
    char_case(p, true)
}

// --- MDI --------------------------------------------------------------------------------

unsafe extern "win64" fn def_frame_proc_w(
    hwnd: usize,
    _client: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    call!(
        "DefWindowProcW",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn def_frame_proc_a(
    hwnd: usize,
    _client: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    call!(
        "DefWindowProcA",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn def_mdi_child_proc_w(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    call!(
        "DefWindowProcW",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn def_mdi_child_proc_a(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    call!(
        "DefWindowProcA",
        fn(usize, u32, usize, isize) -> isize,
        hwnd,
        msg,
        wp,
        lp
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn translate_mdi_sys_accel(_client: usize, _msg: usize) -> i32 {
    0
}

// --- DrawState --------------------------------------------------------------------------

const DST_COMPLEX: u32 = 0;
const DST_TEXT: u32 = 1;
const DST_PREFIXTEXT: u32 = 2;
const DST_ICON: u32 = 3;
const DST_BITMAP: u32 = 4;

#[allow(clippy::too_many_arguments)]
unsafe fn draw_state(
    hdc: usize,
    ldata: isize,
    wdata: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
    cb: usize,
    wide: bool,
) -> i32 {
    match flags & 0xF {
        DST_COMPLEX => {
            if cb == 0 {
                return 0;
            }
            let f: unsafe extern "win64" fn(usize, isize, usize, i32, i32) -> i32 =
                std::mem::transmute(cb);
            f(hdc, ldata, wdata, cx, cy)
        }
        DST_TEXT | DST_PREFIXTEXT => {
            let mut r = [
                x,
                y,
                x + cx.max(1) * if cx > 0 { 1 } else { 4096 },
                y + cy.max(1) * if cy > 0 { 1 } else { 4096 },
            ];
            // DT_NOCLIP | DT_SINGLELINE, et DT_NOPREFIX pour DST_TEXT.
            let fmt = 0x0100 | 0x0020 | if flags & 0xF == DST_TEXT { 0x0800 } else { 0 };
            let n = if wdata == 0 { -1 } else { wdata as i32 };
            if wide {
                call!(
                    "DrawTextW",
                    fn(usize, isize, i32, *mut [i32; 4], u32) -> i32,
                    hdc,
                    ldata,
                    n,
                    &mut r,
                    fmt
                );
            } else {
                call!(
                    "DrawTextA",
                    fn(usize, isize, i32, *mut [i32; 4], u32) -> i32,
                    hdc,
                    ldata,
                    n,
                    &mut r,
                    fmt
                );
            }
            1
        }
        DST_ICON => call!(
            "DrawIconEx",
            fn(usize, i32, i32, usize, i32, i32, u32, usize, u32) -> i32,
            hdc,
            x,
            y,
            ldata as usize,
            cx,
            cy,
            0,
            0,
            3
        )
        .unwrap_or(0),
        DST_BITMAP => {
            let mem = call!("CreateCompatibleDC", fn(usize) -> usize, hdc).unwrap_or(0);
            if mem == 0 {
                return 0;
            }
            let old = call!(
                "SelectObject",
                fn(usize, usize) -> usize,
                mem,
                ldata as usize
            )
            .unwrap_or(0);
            let (mut w, mut h) = (cx, cy);
            if w <= 0 || h <= 0 {
                // BITMAP : bmWidth à 4, bmHeight à 8.
                let mut bm = [0u8; 32];
                call!(
                    "GetObjectW",
                    fn(usize, i32, *mut u8) -> i32,
                    ldata as usize,
                    32,
                    bm.as_mut_ptr()
                );
                w = i32::from_le_bytes(bm[4..8].try_into().unwrap_or_default());
                h = i32::from_le_bytes(bm[8..12].try_into().unwrap_or_default());
            }
            call!(
                "BitBlt",
                fn(usize, i32, i32, i32, i32, usize, i32, i32, u32) -> i32,
                hdc,
                x,
                y,
                w,
                h,
                mem,
                0,
                0,
                0x00CC_0020
            );
            call!("SelectObject", fn(usize, usize) -> usize, mem, old);
            call!("DeleteDC", fn(usize) -> i32, mem);
            1
        }
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_state_w(
    hdc: usize,
    _br: usize,
    cb: usize,
    ldata: isize,
    wdata: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> i32 {
    draw_state(hdc, ldata, wdata, x, y, cx, cy, flags, cb, true)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn draw_state_a(
    hdc: usize,
    _br: usize,
    cb: usize,
    ldata: isize,
    wdata: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> i32 {
    draw_state(hdc, ldata, wdata, x, y, cx, cy, flags, cb, false)
}

// --- Régions de mise à jour --------------------------------------------------------------

unsafe extern "win64" fn invalidate_rgn(hwnd: usize, rgn: usize, erase: i32) -> i32 {
    let r = super::gui::region_rect(rgn);
    let p = r
        .as_ref()
        .map_or(std::ptr::null(), |r| r as *const super::gui::Rect);
    call!(
        "InvalidateRect",
        fn(usize, *const super::gui::Rect, i32) -> i32,
        hwnd,
        p,
        erase
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn validate_rgn(hwnd: usize, rgn: usize) -> i32 {
    let r = super::gui::region_rect(rgn);
    let p = r
        .as_ref()
        .map_or(std::ptr::null(), |r| r as *const super::gui::Rect);
    call!(
        "ValidateRect",
        fn(usize, *const super::gui::Rect) -> i32,
        hwnd,
        p
    )
    .unwrap_or(0)
}

// --- Processus, animation, saisie --------------------------------------------------------

unsafe extern "win64" fn get_process_default_layout(out: *mut u32) -> i32 {
    if out.is_null() {
        return 0;
    }
    *out = 0;
    1
}

unsafe extern "win64" fn set_process_default_layout(_l: u32) -> i32 {
    1
}

/// AnimateWindow : l'animation est immédiate (montrer ou cacher).
unsafe extern "win64" fn animate_window(hwnd: usize, _ms: u32, flags: u32) -> i32 {
    const AW_HIDE: u32 = 0x0001_0000;
    const AW_ACTIVATE: u32 = 0x0002_0000;
    let cmd = if flags & AW_HIDE != 0 {
        0
    } else if flags & AW_ACTIVATE != 0 {
        5
    } else {
        8
    };
    call!("ShowWindow", fn(usize, i32) -> i32, hwnd, cmd);
    1
}

unsafe extern "win64" fn load_cursor_from_file_w(path: *const u16) -> usize {
    // IMAGE_CURSOR, LR_LOADFROMFILE | LR_DEFAULTSIZE.
    call!(
        "LoadImageW",
        fn(usize, *const u16, u32, i32, i32, u32) -> usize,
        0,
        path,
        2,
        0,
        0,
        0x50
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn wait_for_input_idle(_proc: usize, _ms: u32) -> u32 {
    0
}

/// ExitWindowsEx : un programme ne peut pas éteindre Winlux (droit refusé).
unsafe extern "win64" fn exit_windows_ex(_flags: u32, _reason: u32) -> i32 {
    crate::process::set_last_error(5);
    0
}

/// keybd_event / SendInput : pas de saisie simulée vers le serveur X.
unsafe extern "win64" fn keybd_event(_vk: u8, _scan: u8, _flags: u32, _extra: usize) {}

unsafe extern "win64" fn send_input(_n: u32, _inputs: *const u8, _size: i32) -> u32 {
    crate::process::set_last_error(5);
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetMenuInfo" => fn set_menu_info, "GetMenuInfo" => fn get_menu_info,
        "CharLowerA" => fn char_lower_a, "CharUpperA" => fn char_upper_a,
        "DefFrameProcW" => fn def_frame_proc_w, "DefFrameProcA" => fn def_frame_proc_a,
        "DefMDIChildProcW" => fn def_mdi_child_proc_w, "DefMDIChildProcA" => fn def_mdi_child_proc_a,
        "TranslateMDISysAccel" => fn translate_mdi_sys_accel,
        "DrawStateW" => fn draw_state_w, "DrawStateA" => fn draw_state_a,
        "InvalidateRgn" => fn invalidate_rgn, "ValidateRgn" => fn validate_rgn,
        "GetProcessDefaultLayout" => fn get_process_default_layout,
        "SetProcessDefaultLayout" => fn set_process_default_layout,
        "AnimateWindow" => fn animate_window, "LoadCursorFromFileW" => fn load_cursor_from_file_w,
        "WaitForInputIdle" => fn wait_for_input_idle, "ExitWindowsEx" => fn exit_windows_ex,
        "keybd_event" => fn keybd_event, "SendInput" => fn send_input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casse_ansi_et_infos_de_menu() {
        unsafe {
            let mut s = *b"Ab-c\0";
            char_upper_a(s.as_mut_ptr() as usize);
            assert_eq!(&s, b"AB-C\0");
            assert_eq!(char_lower_a(b'Q' as usize), b'q' as usize);
            let mut mi = [0u8; 40];
            mi[0] = 40;
            mi[4] = (MIM_STYLE | MIM_MENUDATA) as u8;
            mi[8] = 0x10;
            mi[32] = 0x42;
            assert_eq!(set_menu_info(0x1234, mi.as_ptr()), 1);
            let mut out = [0u8; 40];
            out[4] = (MIM_STYLE | MIM_MENUDATA) as u8;
            assert_eq!(get_menu_info(0x1234, out.as_mut_ptr()), 1);
            assert_eq!(out[8], 0x10);
            assert_eq!(out[32], 0x42);
        }
    }
}
