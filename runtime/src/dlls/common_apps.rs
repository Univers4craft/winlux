//! Fonctions Win32 communes à plusieurs logiciels gratuits connus
//! (Notepad++ 8, Audacity 3.7, Claude pour Windows) : sous-classement des
//! fenêtres (comctl32), icônes mises à l'échelle, msimg32 (AlphaBlend,
//! GradientFill, TransparentBlt), oleacc, couleurs HLS et chemins de
//! shlwapi, petites fonctions de user32, gdi32, kernel32 et uxtheme.
//!
//! Écrit d'après la documentation publique de Microsoft (salle blanche,
//! voir AGENTS.md). Les fonctions s'appuient sur celles que Winlux
//! fournit déjà, retrouvées par leur nom d'export.

use crate::dlls::gui::{self, DrawCmd, Rect};
use crate::dlls::icons::{self, Image};
use crate::dlls::{lookup, Exports};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const E_FAIL: i32 = 0x8000_4005_u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
const WS_CHILD: u32 = 0x4000_0000;
const WM_NCDESTROY: u32 = 0x0082;

/// Appelle une fonction déjà exportée par Winlux. `None` si elle n'existe pas.
macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                // SAFETY: l'adresse vient de la table d'exports de Winlux et la
                // signature est celle de l'API Windows documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ── comctl32 : sous-classement ──────────────────────────────────────────

#[derive(Clone, Copy)]
struct Sub {
    proc_: usize,
    id: usize,
    data: usize,
}

/// Chaîne de sous-classement d'une fenêtre : `subs[0]` est appelée en
/// premier (la plus récente), `orig` est la procédure d'avant.
struct Chain {
    orig: usize,
    subs: Vec<Sub>,
}

fn chains() -> &'static Mutex<HashMap<usize, Chain>> {
    static M: OnceLock<Mutex<HashMap<usize, Chain>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// (fenêtre, rang suivant) des procédures en cours, pour DefSubclassProc.
    static DEPTH: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

type SubclassProc = unsafe extern "win64" fn(usize, u32, usize, isize, usize, usize) -> isize;

unsafe fn call_at(hwnd: usize, idx: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let step = {
        let map = lock(chains());
        match map.get(&hwnd) {
            Some(c) => match c.subs.get(idx) {
                Some(s) => Ok(*s),
                None => Err(c.orig),
            },
            None => Err(gui::wnd_proc_of(hwnd).unwrap_or(0)),
        }
    };
    match step {
        Ok(s) => {
            DEPTH.with(|d| d.borrow_mut().push((hwnd, idx + 1)));
            let f: SubclassProc = std::mem::transmute(s.proc_);
            let r = f(hwnd, msg, wp, lp, s.id, s.data);
            DEPTH.with(|d| {
                d.borrow_mut().pop();
            });
            r
        }
        Err(orig) if orig != 0 && orig != subclass_wndproc as *const () as usize => {
            gui::call_wndproc(orig, hwnd, msg, wp, lp)
        }
        Err(_) => api!(
            "user32",
            "DefWindowProcW",
            fn(usize, u32, usize, isize) -> isize,
            hwnd,
            msg,
            wp,
            lp
        )
        .unwrap_or(0),
    }
}

/// Procédure posée sur une fenêtre sous-classée : elle parcourt la chaîne.
unsafe extern "win64" fn subclass_wndproc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let r = call_at(hwnd, 0, msg, wp, lp);
    if msg == WM_NCDESTROY {
        lock(chains()).remove(&hwnd);
    }
    r
}

unsafe extern "win64" fn set_window_subclass(
    hwnd: usize,
    proc_: usize,
    id: usize,
    data: usize,
) -> i32 {
    if proc_ == 0 || !gui::is_window(hwnd) {
        return 0;
    }
    let mut map = lock(chains());
    let chain = match map.get_mut(&hwnd) {
        Some(c) => c,
        None => {
            let me = subclass_wndproc as *const () as usize;
            let orig = gui::set_wnd_proc(hwnd, me).unwrap_or(0);
            map.entry(hwnd).or_insert(Chain {
                orig,
                subs: Vec::new(),
            })
        }
    };
    match chain
        .subs
        .iter_mut()
        .find(|s| s.proc_ == proc_ && s.id == id)
    {
        Some(s) => s.data = data,
        None => chain.subs.insert(0, Sub { proc_, id, data }),
    }
    1
}

unsafe extern "win64" fn get_window_subclass(
    hwnd: usize,
    proc_: usize,
    id: usize,
    data: *mut usize,
) -> i32 {
    let map = lock(chains());
    match map
        .get(&hwnd)
        .and_then(|c| c.subs.iter().find(|s| s.proc_ == proc_ && s.id == id))
    {
        Some(s) => {
            if !data.is_null() {
                *data = s.data;
            }
            1
        }
        None => {
            if !data.is_null() {
                *data = 0;
            }
            0
        }
    }
}

extern "win64" fn remove_window_subclass(hwnd: usize, proc_: usize, id: usize) -> i32 {
    let mut map = lock(chains());
    let Some(c) = map.get_mut(&hwnd) else {
        return 0;
    };
    let before = c.subs.len();
    c.subs.retain(|s| !(s.proc_ == proc_ && s.id == id));
    let found = c.subs.len() != before;
    let me = subclass_wndproc as *const () as usize;
    if c.subs.is_empty() && gui::wnd_proc_of(hwnd) == Some(me) {
        let orig = c.orig;
        // Hors d'un appel en cours, on rend sa procédure à la fenêtre.
        let busy = DEPTH.with(|d| d.borrow().iter().any(|(h, _)| *h == hwnd));
        if !busy {
            gui::set_wnd_proc(hwnd, orig);
            map.remove(&hwnd);
        }
    }
    found as i32
}

unsafe extern "win64" fn def_subclass_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let idx = DEPTH.with(|d| {
        d.borrow()
            .iter()
            .rev()
            .find(|(h, _)| *h == hwnd)
            .map(|(_, i)| *i)
    });
    call_at(hwnd, idx.unwrap_or(usize::MAX), msg, wp, lp)
}

// ── comctl32 : icônes et souris ─────────────────────────────────────────

const IMAGE_ICON: u32 = 1;

unsafe extern "win64" fn load_icon_with_scale_down(
    inst: usize,
    name: *const u16,
    cx: i32,
    cy: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let h = icons::load_image_w(inst, name, IMAGE_ICON, cx, cy, 0);
    *out = h;
    if h != 0 {
        S_OK
    } else {
        0x8007_0716_u32 as i32 // HRESULT_FROM_WIN32(ERROR_RESOURCE_NAME_NOT_FOUND)
    }
}

/// LoadIconMetric : LIM_SMALL (0) donne 16 × 16, LIM_LARGE (1) 32 × 32.
unsafe extern "win64" fn load_icon_metric(
    inst: usize,
    name: *const u16,
    lims: i32,
    out: *mut usize,
) -> i32 {
    let s = if lims == 0 { 16 } else { 32 };
    load_icon_with_scale_down(inst, name, s, s, out)
}

unsafe extern "win64" fn track_mouse_event_cc(tme: *mut u8) -> i32 {
    api!("user32", "TrackMouseEvent", fn(*mut u8) -> i32, tme).unwrap_or(1)
}

// ── user32 ──────────────────────────────────────────────────────────────

const GA_PARENT: u32 = 1;
const GA_ROOT: u32 = 2;
const GA_ROOTOWNER: u32 = 3;

fn is_child_style(h: usize) -> bool {
    gui::window_style(h).unwrap_or(0) & WS_CHILD != 0
}

/// Fenêtre de premier niveau qui contient `h` (en remontant les WS_CHILD).
fn root_of(mut h: usize) -> usize {
    for _ in 0..256 {
        let p = gui::window_parent(h).unwrap_or(0);
        if p == 0 || !is_child_style(h) {
            break;
        }
        h = p;
    }
    h
}

extern "win64" fn get_ancestor(hwnd: usize, flags: u32) -> usize {
    if !gui::is_window(hwnd) {
        crate::process::set_last_error(1400);
        return 0;
    }
    match flags {
        GA_PARENT => {
            let p = gui::window_parent(hwnd).unwrap_or(0);
            if is_child_style(hwnd) && p != 0 {
                p
            } else {
                // SAFETY: GetDesktopWindow ne prend pas d'argument.
                unsafe { api!("user32", "GetDesktopWindow", fn() -> usize).unwrap_or(0) }
            }
        }
        GA_ROOT => root_of(hwnd),
        GA_ROOTOWNER => {
            let mut r = root_of(hwnd);
            for _ in 0..64 {
                let owner = gui::get_window_relative(r, 4) // GW_OWNER
                    .filter(|&o| o != 0)
                    .or_else(|| gui::window_parent(r).filter(|&p| p != 0));
                match owner {
                    Some(o) if gui::is_window(o) && o != r => r = root_of(o),
                    _ => break,
                }
            }
            r
        }
        _ => 0,
    }
}

extern "win64" fn is_child(parent: usize, hwnd: usize) -> i32 {
    let mut cur = hwnd;
    for _ in 0..256 {
        if !is_child_style(cur) {
            return 0;
        }
        match gui::window_parent(cur) {
            Some(p) if p == parent && p != 0 => return 1,
            Some(p) if p != 0 => cur = p,
            _ => return 0,
        }
    }
    0
}

unsafe extern "win64" fn enum_thread_windows(tid: u32, cb: usize, lp: isize) -> i32 {
    if cb == 0 {
        return 0;
    }
    let f: unsafe extern "win64" fn(usize, isize) -> i32 = std::mem::transmute(cb);
    for h in gui::enum_top_level_windows() {
        let owner = api!(
            "user32",
            "GetWindowThreadProcessId",
            fn(usize, *mut u32) -> u32,
            h,
            std::ptr::null_mut()
        )
        .unwrap_or(0);
        if owner == tid && f(h, lp) == 0 {
            break;
        }
    }
    1
}

unsafe extern "win64" fn child_window_from_point(parent: usize, pt: u64) -> usize {
    api!(
        "user32",
        "ChildWindowFromPointEx",
        fn(usize, u64, u32) -> usize,
        parent,
        pt,
        0
    )
    .unwrap_or(0)
}

pub(crate) fn lower_char(c: u16) -> u16 {
    let s = String::from_utf16_lossy(&[c]);
    let mut low = s.chars().flat_map(char::to_lowercase);
    match (low.next(), low.next()) {
        (Some(l), None) if (l as u32) <= 0xFFFF => l as u16,
        _ => c,
    }
}

/// CharLowerW : un caractère si le mot fort est nul, sinon la chaîne en place.
unsafe extern "win64" fn char_lower_w(p: *mut u16) -> *mut u16 {
    if (p as usize) >> 16 == 0 {
        return lower_char(p as usize as u16) as usize as *mut u16;
    }
    let mut q = p;
    while *q != 0 {
        *q = lower_char(*q);
        q = q.add(1);
    }
    p
}

unsafe extern "win64" fn char_lower_buff_w(p: *mut u16, n: u32) -> u32 {
    if p.is_null() {
        return 0;
    }
    for i in 0..n as usize {
        *p.add(i) = lower_char(*p.add(i));
    }
    n
}

fn wchar(c: u16) -> Option<char> {
    char::from_u32(c as u32)
}

extern "win64" fn is_char_alpha_w(c: u16) -> i32 {
    wchar(c).is_some_and(char::is_alphabetic) as i32
}

extern "win64" fn is_char_alpha_numeric_w(c: u16) -> i32 {
    wchar(c).is_some_and(char::is_alphanumeric) as i32
}

extern "win64" fn is_char_lower_w(c: u16) -> i32 {
    wchar(c).is_some_and(char::is_lowercase) as i32
}

extern "win64" fn is_char_upper_w(c: u16) -> i32 {
    wchar(c).is_some_and(char::is_uppercase) as i32
}

/// Le presse-papiers prévient déjà par WM_CLIPBOARDUPDATE ; la vieille
/// chaîne de visionneuses reste vide.
/// SetClipboardViewer : rend la fenêtre inscrite avant (la suivante de
/// la chaîne) ; GetClipboardViewer la retrouve.
extern "win64" fn set_clipboard_viewer(hwnd: usize) -> usize {
    super::user32_releve::swap_clipboard_viewer(hwnd)
}

extern "win64" fn change_clipboard_chain(_remove: usize, _next: usize) -> i32 {
    0
}

extern "win64" fn lock_window_update(_hwnd: usize) -> i32 {
    1
}

extern "win64" fn notify_win_event(_ev: u32, _hwnd: usize, _obj: i32, _child: i32) {}

extern "win64" fn set_menu_item_bitmaps(_m: usize, _p: u32, _f: u32, _u: usize, _c: usize) -> i32 {
    1
}

/// mouse_event : seul le déplacement absolu ou relatif du pointeur est rendu.
unsafe extern "win64" fn mouse_event(flags: u32, dx: i32, dy: i32, _data: u32, _extra: usize) {
    const MOVE: u32 = 0x1;
    const ABSOLUTE: u32 = 0x8000;
    if flags & MOVE == 0 {
        return;
    }
    let (x, y) = if flags & ABSOLUTE != 0 {
        let (w, h) = gui::screen_size();
        (dx * w / 65536, dy * h / 65536)
    } else {
        let mut pt = [0i32; 2];
        api!(
            "user32",
            "GetCursorPos",
            fn(*mut i32) -> i32,
            pt.as_mut_ptr()
        );
        (pt[0] + dx, pt[1] + dy)
    };
    api!("user32", "SetCursorPos", fn(i32, i32) -> i32, x, y);
}

unsafe extern "win64" fn get_scroll_range(
    hwnd: usize,
    bar: i32,
    min: *mut i32,
    max: *mut i32,
) -> i32 {
    // SCROLLINFO : cbSize, fMask, nMin, nMax, nPage, nPos, nTrackPos.
    let mut si = [28u32, 1, 0, 0, 0, 0, 0];
    let ok = api!(
        "user32",
        "GetScrollInfo",
        fn(usize, i32, *mut u32) -> i32,
        hwnd,
        bar,
        si.as_mut_ptr()
    )
    .unwrap_or(0);
    if !min.is_null() {
        *min = if ok != 0 { si[2] as i32 } else { 0 };
    }
    if !max.is_null() {
        *max = if ok != 0 { si[3] as i32 } else { 0 };
    }
    1
}

/// GetComboBoxInfo : zone du texte, bouton à droite, fenêtre d'édition.
unsafe extern "win64" fn get_combo_box_info(hwnd: usize, info: *mut u8) -> i32 {
    if info.is_null() || !gui::is_window(hwnd) {
        return 0;
    }
    let mut rc = Rect::default();
    api!(
        "user32",
        "GetClientRect",
        fn(usize, *mut Rect) -> i32,
        hwnd,
        &mut rc
    );
    let button = 17.min(rc.right);
    let item = Rect {
        left: 3,
        top: 3,
        right: (rc.right - button - 1).max(3),
        bottom: (rc.bottom - 3).max(3),
    };
    let btn = Rect {
        left: rc.right - button,
        top: 0,
        right: rc.right,
        bottom: rc.bottom,
    };
    let edit = gui::child_windows(hwnd).first().copied().unwrap_or(0);
    // COMBOBOXINFO x64 : cbSize, rcItem @4, rcButton @20, stateButton @36,
    // hwndCombo @40, hwndItem @48, hwndList @56.
    (info.add(4) as *mut Rect).write_unaligned(item);
    (info.add(20) as *mut Rect).write_unaligned(btn);
    (info.add(36) as *mut u32).write_unaligned(0);
    (info.add(40) as *mut usize).write_unaligned(hwnd);
    // hwndItem : le champ de saisie. Seule une liste déroulante sans saisie
    // (CBS_DROPDOWNLIST) se désigne elle-même. Nos listes modifiables se
    // dessinent sans fenêtre Edit fille : rendre la liste elle-même faisait
    // que Notepad++ sous-classait la liste au lieu de son champ, et son
    // CB_GETDROPPEDSTATE se rappelait sans fin (débordement de pile, #1126).
    let drop_list = gui::window_style(hwnd).is_some_and(|s| s & 3 == 3);
    let item_hwnd = if edit != 0 {
        edit
    } else if drop_list {
        hwnd
    } else {
        0
    };
    (info.add(48) as *mut usize).write_unaligned(item_hwnd);
    (info.add(56) as *mut usize).write_unaligned(0);
    1
}

/// Pas de barre de menus dessinée par l'appli : l'appel échoue proprement.
extern "win64" fn get_menu_bar_info(_hwnd: usize, _obj: i32, _item: i32, _info: *mut u8) -> i32 {
    0
}

/// DrawFrameControl : boutons, cases et flèches dessinés au style Winlux.
unsafe extern "win64" fn draw_frame_control(hdc: usize, rc: *mut Rect, ty: u32, state: u32) -> i32 {
    const DFC_BUTTON: u32 = 4;
    const DFC_SCROLL: u32 = 3;
    const DFCS_BUTTONCHECK: u32 = 0x0;
    const DFCS_BUTTONRADIO: u32 = 0x4;
    const DFCS_BUTTONPUSH: u32 = 0x10;
    const DFCS_CHECKED: u32 = 0x400;
    const DFCS_PUSHED: u32 = 0x200;
    const DFCS_INACTIVE: u32 = 0x100;
    if rc.is_null() {
        return 0;
    }
    let r = *rc;
    if r.right <= r.left || r.bottom <= r.top {
        return 1;
    }
    let face = crate::dlls::user32::get_sys_color(15); // COLOR_BTNFACE
    let fill = |rect: Rect, color: u32| gui::record(hdc, DrawCmd::FillRect { rect, color });
    let frame = |rect: Rect, color: u32| {
        gui::record(
            hdc,
            DrawCmd::FrameRect {
                rect,
                color,
                width: 1,
            },
        )
    };
    let ink = if state & DFCS_INACTIVE != 0 {
        0x00A0_A0A0
    } else {
        0x0020_2020
    };
    match ty {
        DFC_BUTTON if state & 0xFF == DFCS_BUTTONPUSH => {
            fill(r, face);
            frame(
                r,
                if state & DFCS_PUSHED != 0 {
                    0x0060_6060
                } else {
                    0x00B0_B0B0
                },
            );
        }
        DFC_BUTTON if state & 0xFF == DFCS_BUTTONCHECK || state & 0xFF == DFCS_BUTTONRADIO => {
            fill(r, 0x00FF_FFFF);
            frame(r, 0x0080_8080);
            if state & DFCS_CHECKED != 0 {
                let w = r.right - r.left;
                let h = r.bottom - r.top;
                let inner = Rect {
                    left: r.left + w / 4,
                    top: r.top + h / 4,
                    right: r.right - w / 4,
                    bottom: r.bottom - h / 4,
                };
                fill(inner, 0x00F0_3D6A); // violet Winlux (COLORREF BGR)
            }
        }
        DFC_SCROLL => {
            fill(r, face);
            frame(r, 0x00B0_B0B0);
            // Flèche : DFCS_SCROLLUP 0, DOWN 1, LEFT 2, RIGHT 3, COMBOBOX 5.
            let cx = (r.left + r.right) / 2;
            let cy = (r.top + r.bottom) / 2;
            let s = ((r.right - r.left).min(r.bottom - r.top) / 4).max(1);
            for i in 0..s {
                let line = match state & 0xF {
                    0 => Rect {
                        left: cx - i,
                        top: cy - s / 2 + i,
                        right: cx + i + 1,
                        bottom: cy - s / 2 + i + 1,
                    },
                    2 => Rect {
                        left: cx - s / 2 + i,
                        top: cy - i,
                        right: cx - s / 2 + i + 1,
                        bottom: cy + i + 1,
                    },
                    3 => Rect {
                        left: cx + s / 2 - i,
                        top: cy - i,
                        right: cx + s / 2 - i + 1,
                        bottom: cy + i + 1,
                    },
                    _ => Rect {
                        left: cx - i,
                        top: cy + s / 2 - i,
                        right: cx + i + 1,
                        bottom: cy + s / 2 - i + 1,
                    },
                };
                fill(line, ink);
            }
        }
        _ => {
            fill(r, face);
            frame(r, 0x00B0_B0B0);
        }
    }
    1
}

// ── gdi32 / msimg32 ─────────────────────────────────────────────────────

/// AlphaBlend : la source vient de la bitmap du DC mémoire. Sur une cible
/// mémoire, le mélange est exact ; sur une fenêtre, les pixels à moins de
/// 50 % d'opacité sont omis.
unsafe extern "win64" fn alpha_blend(
    dst: usize,
    xd: i32,
    yd: i32,
    wd: i32,
    hd: i32,
    src: usize,
    xs: i32,
    ys: i32,
    ws: i32,
    hs: i32,
    ftn: u32,
) -> i32 {
    const AC_SRC_ALPHA: u32 = 0x01;
    if wd <= 0 || hd <= 0 || ws <= 0 || hs <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let constant = (ftn >> 16) & 0xFF;
    let per_pixel = (ftn >> 24) & AC_SRC_ALPHA != 0;
    if constant == 0 {
        return 1;
    }
    let bmp = gui::dc_bitmap(src);
    let Some((bw, bh, px, has_alpha)) = (bmp != 0).then(|| gui::bitmap_pixels(bmp)).flatten()
    else {
        // Source fenêtre : simple copie quand elle est surtout opaque.
        if constant >= 128 {
            return api!(
                "gdi32",
                "StretchBlt",
                fn(usize, i32, i32, i32, i32, usize, i32, i32, i32, i32, u32) -> i32,
                dst,
                xd,
                yd,
                wd,
                hd,
                src,
                xs,
                ys,
                ws,
                hs,
                0x00CC_0020
            )
            .unwrap_or(0);
        }
        return 1;
    };
    let mut img = Image {
        w: ws as usize,
        h: hs as usize,
        px: Vec::with_capacity((ws * hs) as usize),
    };
    for y in ys..ys + hs {
        for x in xs..xs + ws {
            let p = if x >= 0 && y >= 0 && (x as usize) < bw && (y as usize) < bh {
                px[y as usize * bw + x as usize]
            } else {
                0
            };
            let mut a = if per_pixel && has_alpha { p >> 24 } else { 255 };
            let mut c = p & 0x00FF_FFFF;
            if per_pixel && has_alpha && a > 0 && a < 255 {
                // Couleurs prémultipliées : on les ramène à pleine intensité.
                let un = |v: u32| ((v * 255) / a).min(255);
                c = un(c & 0xFF) | (un((c >> 8) & 0xFF) << 8) | (un((c >> 16) & 0xFF) << 16);
            }
            a = a * constant / 255;
            img.px.push(c | (a << 24));
        }
    }
    let img = img.scaled(wd as usize, hd as usize);
    blend_image(dst, xd, yd, &img) as i32
}

/// Fond une image (COLORREF + alpha non prémultiplié) dans un DC : dans la
/// bitmap mémoire sélectionnée, ou à l'écran.
pub(crate) fn blend_image(dst: usize, xd: i32, yd: i32, img: &Image) -> bool {
    let (wd, hd) = (img.w as i32, img.h as i32);
    if gui::dc_bitmap(dst) != 0 {
        let rect = Rect {
            left: xd,
            top: yd,
            right: xd + wd,
            bottom: yd + hd,
        };
        if let Some(under) = gui::bitmap_snapshot(dst, rect) {
            let pixels = img
                .px
                .iter()
                .zip(under.pixels.iter())
                .map(|(&s, &d)| mix(s, d, s >> 24))
                .collect();
            return gui::record(
                dst,
                DrawCmd::Bitmap {
                    rect,
                    pixels,
                    rop: gui::RasterOp::Copy,
                },
            );
        }
    }
    icons::draw(dst, xd, yd, wd, hd, img, None)
}

fn mix(s: u32, d: u32, a: u32) -> u32 {
    let m = |sh: u32| ((((s >> sh) & 0xFF) * a + ((d >> sh) & 0xFF) * (255 - a)) / 255) << sh;
    m(0) | m(8) | m(16)
}

/// TransparentBlt : copie en sautant la couleur clé.
unsafe extern "win64" fn transparent_blt(
    dst: usize,
    xd: i32,
    yd: i32,
    wd: i32,
    hd: i32,
    src: usize,
    xs: i32,
    ys: i32,
    ws: i32,
    hs: i32,
    key: u32,
) -> i32 {
    if wd <= 0 || hd <= 0 || ws <= 0 || hs <= 0 {
        return 0;
    }
    let rect = Rect {
        left: xs,
        top: ys,
        right: xs + ws,
        bottom: ys + hs,
    };
    let Some(snap) = gui::bitmap_snapshot(src, rect) else {
        return 0;
    };
    let img = Image {
        w: snap.width,
        h: snap.height,
        px: snap
            .pixels
            .iter()
            .map(|&p| {
                let c = p & 0x00FF_FFFF;
                if c == key & 0x00FF_FFFF {
                    0
                } else {
                    c | 0xFF00_0000
                }
            })
            .collect(),
    };
    icons::draw(dst, xd, yd, wd, hd, &img, None) as i32
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TriVertex {
    x: i32,
    y: i32,
    r: u16,
    g: u16,
    b: u16,
    a: u16,
}

fn vcolor(v: &TriVertex) -> [f32; 3] {
    [v.r as f32 / 256.0, v.g as f32 / 256.0, v.b as f32 / 256.0]
}

fn colorref(c: [f32; 3]) -> u32 {
    let q = |v: f32| v.clamp(0.0, 255.0) as u32;
    q(c[0]) | (q(c[1]) << 8) | (q(c[2]) << 16)
}

/// Pixels d'un dégradé rectangulaire (horizontal ou vertical).
fn gradient_rect(a: &TriVertex, b: &TriVertex, vertical: bool) -> Option<(Rect, Vec<u32>)> {
    let rect = Rect {
        left: a.x.min(b.x),
        top: a.y.min(b.y),
        right: a.x.max(b.x),
        bottom: a.y.max(b.y),
    };
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > gui::MAX_BITMAP_PIXELS as i64 {
        return None;
    }
    let (ca, cb) = (vcolor(a), vcolor(b));
    let steps = if vertical { h } else { w };
    let line: Vec<u32> = (0..steps)
        .map(|i| {
            let t = if steps > 1 {
                i as f32 / (steps - 1) as f32
            } else {
                0.0
            };
            colorref([0, 1, 2].map(|k| ca[k] + (cb[k] - ca[k]) * t))
        })
        .collect();
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            px.push(line[if vertical { y } else { x } as usize]);
        }
    }
    Some((rect, px))
}

unsafe extern "win64" fn gradient_fill(
    hdc: usize,
    verts: *const TriVertex,
    nverts: u32,
    mesh: *const u32,
    nmesh: u32,
    mode: u32,
) -> i32 {
    const GRADIENT_FILL_RECT_V: u32 = 1;
    const GRADIENT_FILL_TRIANGLE: u32 = 2;
    if verts.is_null() || mesh.is_null() {
        return 0;
    }
    let v = std::slice::from_raw_parts(verts, nverts as usize);
    let get = |i: u32| v.get(i as usize);
    for m in 0..nmesh as usize {
        if mode == GRADIENT_FILL_TRIANGLE {
            let t = std::slice::from_raw_parts(mesh.add(m * 3), 3);
            let (Some(a), Some(b), Some(c)) = (get(t[0]), get(t[1]), get(t[2])) else {
                return 0;
            };
            let left = a.x.min(b.x).min(c.x);
            let top = a.y.min(b.y).min(c.y);
            let w = a.x.max(b.x).max(c.x) - left;
            let h = a.y.max(b.y).max(c.y) - top;
            if w <= 0 || h <= 0 || (w as i64) * (h as i64) > gui::MAX_BITMAP_PIXELS as i64 {
                continue;
            }
            let area = ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)) as f32;
            if area == 0.0 {
                continue;
            }
            let mut img = Image {
                w: w as usize,
                h: h as usize,
                px: Vec::with_capacity((w * h) as usize),
            };
            for y in 0..h {
                for x in 0..w {
                    let (px_, py) = ((left + x) as f32 + 0.5, (top + y) as f32 + 0.5);
                    let e = |p: &TriVertex, q: &TriVertex| {
                        ((q.x - p.x) as f32 * (py - p.y as f32)
                            - (px_ - p.x as f32) * (q.y - p.y) as f32)
                            / area
                    };
                    let (wa, wb, wc) = (e(b, c), e(c, a), e(a, b));
                    if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                        img.px.push(0);
                    } else {
                        let (ca, cb, cc) = (vcolor(a), vcolor(b), vcolor(c));
                        let col = colorref([0, 1, 2].map(|k| ca[k] * wa + cb[k] * wb + cc[k] * wc));
                        img.px.push(col | 0xFF00_0000);
                    }
                }
            }
            icons::draw(hdc, left, top, w, h, &img, None);
        } else {
            let r = std::slice::from_raw_parts(mesh.add(m * 2), 2);
            let (Some(a), Some(b)) = (get(r[0]), get(r[1])) else {
                return 0;
            };
            if let Some((rect, pixels)) = gradient_rect(a, b, mode == GRADIENT_FILL_RECT_V) {
                gui::record(
                    hdc,
                    DrawCmd::Bitmap {
                        rect,
                        pixels,
                        rop: gui::RasterOp::Copy,
                    },
                );
            }
        }
    }
    1
}

/// Longueur de l'en-tête d'une BITMAPINFO (palette et masques compris).
unsafe fn bitmap_info_len(bmi: *const u8) -> usize {
    let size = (bmi as *const u32).read_unaligned() as usize;
    let bpp = (bmi.add(14) as *const u16).read_unaligned() as usize;
    let compression = (bmi.add(16) as *const u32).read_unaligned();
    let used = (bmi.add(32) as *const u32).read_unaligned() as usize;
    let colors = if bpp <= 8 {
        if used != 0 {
            used
        } else {
            1 << bpp
        }
    } else {
        used
    };
    let masks = if compression == 3 && size == 40 {
        12
    } else {
        0
    };
    size + masks + colors * 4
}

/// SetDIBits : écrit des lignes DIB dans une bitmap existante.
unsafe extern "win64" fn set_di_bits(
    _hdc: usize,
    bmp: usize,
    start: u32,
    lines: u32,
    bits: *const u8,
    bmi: *const u8,
    _usage: u32,
) -> i32 {
    if bits.is_null() || bmi.is_null() || lines == 0 {
        return 0;
    }
    let Some((_, bh, _, _)) = gui::bitmap_pixels(bmp) else {
        return 0;
    };
    let w = (bmi.add(4) as *const i32).read_unaligned();
    let h = (bmi.add(8) as *const i32).read_unaligned();
    let bpp = (bmi.add(14) as *const u16).read_unaligned() as usize;
    if w <= 0 || h == 0 || bpp == 0 {
        return 0;
    }
    let lines = lines.min(h.unsigned_abs().saturating_sub(start)) as usize;
    if lines == 0 {
        return 0;
    }
    let stride = (w as usize * bpp).div_ceil(32) * 4;
    let head = bitmap_info_len(bmi);
    let mut buf = std::slice::from_raw_parts(bmi, head).to_vec();
    // La bande décodée ne couvre que les lignes fournies.
    let band_h = if h < 0 { -(lines as i32) } else { lines as i32 };
    buf[8..12].copy_from_slice(&band_h.to_le_bytes());
    buf.extend_from_slice(std::slice::from_raw_parts(bits, stride * lines));
    let Some(img) = icons::decode_dib(&buf, false) else {
        return 0;
    };
    let full_h = h.unsigned_abs() as usize;
    for r in 0..lines.min(img.h) {
        // Ligne r de la bande (haut d'abord) → ligne de balayage DIB.
        let scan = if h < 0 {
            start as usize + r
        } else {
            start as usize + (lines - 1 - r)
        };
        let y = if h < 0 { scan } else { full_h - 1 - scan };
        if y < bh {
            let row: Vec<u32> = img.px[r * img.w..(r + 1) * img.w]
                .iter()
                .map(|&p| p | 0xFF00_0000)
                .collect();
            gui::write_bitmap_row(bmp, y, &row);
        }
    }
    lines as i32
}

/// OffsetWindowOrgEx : déplace l'origine de fenêtre du DC (#1232).
unsafe extern "win64" fn offset_window_org_ex(
    hdc: usize,
    dx: i32,
    dy: i32,
    prev: *mut crate::dlls::gui::Point,
) -> i32 {
    crate::dlls::gdi32::offset_origin(hdc, false, dx, dy, prev)
}

/// RectVisible : la zone de découpe d'un DC couvre tout l'écran.
unsafe extern "win64" fn rect_visible(_hdc: usize, rc: *const Rect) -> i32 {
    if rc.is_null() {
        return 0;
    }
    let r = *rc;
    (r.right > r.left && r.bottom > r.top) as i32
}

extern "win64" fn pt_visible(_hdc: usize, _x: i32, _y: i32) -> i32 {
    1
}

// ── kernel32 ────────────────────────────────────────────────────────────

/// lstrcpyn : au plus n − 1 caractères, toujours terminé par 0.
unsafe extern "win64" fn lstrcpyn_w(dst: *mut u16, src: *const u16, n: i32) -> *mut u16 {
    if dst.is_null() || n <= 0 {
        return dst;
    }
    let mut i = 0usize;
    if !src.is_null() {
        while (i as i32) < n - 1 && *src.add(i) != 0 {
            *dst.add(i) = *src.add(i);
            i += 1;
        }
    }
    *dst.add(i) = 0;
    dst
}

unsafe extern "win64" fn lstrcpyn_a(dst: *mut u8, src: *const u8, n: i32) -> *mut u8 {
    if dst.is_null() || n <= 0 {
        return dst;
    }
    let mut i = 0usize;
    if !src.is_null() {
        while (i as i32) < n - 1 && *src.add(i) != 0 {
            *dst.add(i) = *src.add(i);
            i += 1;
        }
    }
    *dst.add(i) = 0;
    dst
}

/// LCMapStringA : minuscules et majuscules ASCII, copie sinon.
unsafe extern "win64" fn lc_map_string_a(
    _locale: u32,
    flags: u32,
    src: *const u8,
    count: i32,
    dst: *mut u8,
    cap: i32,
) -> i32 {
    const LOWER: u32 = 0x100;
    const UPPER: u32 = 0x200;
    if src.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let len = if count < 0 {
        let mut n = 0usize;
        while *src.add(n) != 0 {
            n += 1;
        }
        n + 1
    } else {
        count as usize
    };
    if cap == 0 {
        return len as i32;
    }
    if dst.is_null() || (cap as usize) < len {
        crate::process::set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    for i in 0..len {
        let c = *src.add(i);
        *dst.add(i) = if flags & LOWER != 0 {
            c.to_ascii_lowercase()
        } else if flags & UPPER != 0 {
            c.to_ascii_uppercase()
        } else {
            c
        };
    }
    len as i32
}

unsafe extern "win64" fn get_string_type_ex_w(
    _locale: u32,
    ty: u32,
    src: *const u16,
    n: i32,
    out: *mut u16,
) -> i32 {
    api!(
        "kernel32",
        "GetStringTypeW",
        fn(u32, *const u16, i32, *mut u16) -> i32,
        ty,
        src,
        n,
        out
    )
    .unwrap_or(0)
}

pub(crate) unsafe extern "win64" fn get_string_type_ex_a(
    _locale: u32,
    ty: u32,
    src: *const u8,
    n: i32,
    out: *mut u16,
) -> i32 {
    if src.is_null() || out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let len = if n < 0 {
        let mut k = 0usize;
        while *src.add(k) != 0 {
            k += 1;
        }
        k + 1
    } else {
        n as usize
    };
    // Une valeur par octet, comme la version ANSI de Windows.
    let wide: Vec<u16> = (0..len).map(|i| *src.add(i) as u16).collect();
    get_string_type_ex_w(0, ty, wide.as_ptr(), len as i32, out)
}

/// Redémarrage après plantage : Winlux ne relance pas les applis.
extern "win64" fn register_application_restart(_cmd: *const u16, _flags: u32) -> i32 {
    S_OK
}

extern "win64" fn unregister_application_restart() -> i32 {
    S_OK
}

unsafe extern "win64" fn get_application_restart_settings(
    _process: usize,
    _cmd: *mut u16,
    size: *mut u32,
    _flags: *mut u32,
) -> i32 {
    if !size.is_null() {
        *size = 0;
    }
    0x8007_0490_u32 as i32 // HRESULT_FROM_WIN32(ERROR_NOT_FOUND)
}

// ── shlwapi ─────────────────────────────────────────────────────────────

const HLS_MAX: i32 = 240;

/// COLORREF → (teinte, luminosité, saturation) sur 0..240.
pub fn rgb_to_hls(c: u32) -> (u16, u16, u16) {
    let r = (c & 0xFF) as i32;
    let g = ((c >> 8) & 0xFF) as i32;
    let b = ((c >> 16) & 0xFF) as i32;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = ((max + min) * HLS_MAX + 255) / 510;
    if max == min {
        return (160, l as u16, 0); // gris : teinte indéfinie (160 comme Windows)
    }
    let d = max - min;
    let s = if l <= HLS_MAX / 2 {
        (d * HLS_MAX + (max + min) / 2) / (max + min)
    } else {
        (d * HLS_MAX + (510 - max - min) / 2) / (510 - max - min)
    };
    let part = |v: i32| ((max - v) * (HLS_MAX / 6) + d / 2) / d;
    let (rd, gd, bd) = (part(r), part(g), part(b));
    let mut h = if r == max {
        bd - gd
    } else if g == max {
        HLS_MAX / 3 + rd - bd
    } else {
        2 * HLS_MAX / 3 + gd - rd
    };
    if h < 0 {
        h += HLS_MAX;
    }
    if h >= HLS_MAX {
        h -= HLS_MAX;
    }
    (h as u16, l as u16, s as u16)
}

fn hue_to_rgb(n1: i32, n2: i32, mut h: i32) -> i32 {
    if h < 0 {
        h += HLS_MAX;
    }
    if h > HLS_MAX {
        h -= HLS_MAX;
    }
    if h < HLS_MAX / 6 {
        n1 + ((n2 - n1) * h + HLS_MAX / 12) / (HLS_MAX / 6)
    } else if h < HLS_MAX / 2 {
        n2
    } else if h < 2 * HLS_MAX / 3 {
        n1 + ((n2 - n1) * (2 * HLS_MAX / 3 - h) + HLS_MAX / 12) / (HLS_MAX / 6)
    } else {
        n1
    }
}

/// (teinte, luminosité, saturation) sur 0..240 → COLORREF.
pub fn hls_to_rgb(h: u16, l: u16, s: u16) -> u32 {
    let (h, l, s) = (h as i32, (l as i32).min(HLS_MAX), (s as i32).min(HLS_MAX));
    if s == 0 {
        let v = (l * 255 / HLS_MAX) as u32;
        return v | (v << 8) | (v << 16);
    }
    let m2 = if l <= HLS_MAX / 2 {
        (l * (HLS_MAX + s) + HLS_MAX / 2) / HLS_MAX
    } else {
        l + s - (l * s + HLS_MAX / 2) / HLS_MAX
    };
    let m1 = 2 * l - m2;
    let c = |v: i32| ((v * 255 + HLS_MAX / 2) / HLS_MAX).clamp(0, 255) as u32;
    let r = c(hue_to_rgb(m1, m2, h + HLS_MAX / 3));
    let g = c(hue_to_rgb(m1, m2, h));
    let b = c(hue_to_rgb(m1, m2, h - HLS_MAX / 3));
    r | (g << 8) | (b << 16)
}

unsafe extern "win64" fn color_rgb_to_hls(c: u32, h: *mut u16, l: *mut u16, s: *mut u16) {
    let (hh, ll, ss) = rgb_to_hls(c);
    if !h.is_null() {
        *h = hh;
    }
    if !l.is_null() {
        *l = ll;
    }
    if !s.is_null() {
        *s = ss;
    }
}

extern "win64" fn color_hls_to_rgb(h: u16, l: u16, s: u16) -> u32 {
    hls_to_rgb(h, l, s)
}

/// ColorAdjustLuma : `n` en dixièmes de pour cent ; avec `scale`, la
/// variation est proportionnelle à l'écart restant.
pub extern "win64" fn color_adjust_luma(c: u32, n: i32, scale: i32) -> u32 {
    if n == 0 {
        return c;
    }
    let (h, l, s) = rgb_to_hls(c);
    let l = l as i32;
    let nl = if scale != 0 {
        if n > 0 {
            l + (HLS_MAX - l) * n / 1000
        } else {
            l + l * n / 1000
        }
    } else {
        l + n * HLS_MAX / 1000
    };
    hls_to_rgb(h, nl.clamp(0, HLS_MAX) as u16, s)
}

unsafe fn read_w(p: *const u16) -> Vec<u16> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0;
    while *p.add(i) != 0 {
        v.push(*p.add(i));
        i += 1;
    }
    v
}

unsafe fn write_w(dst: *mut u16, s: &[u16]) {
    std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
    *dst.add(s.len()) = 0;
}

fn file_part(path: &[u16]) -> usize {
    path.iter()
        .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16 || c == b':' as u16)
        .map_or(0, |i| i + 1)
}

unsafe extern "win64" fn path_add_extension_w(path: *mut u16, ext: *const u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let p = read_w(path);
    let name = &p[file_part(&p)..];
    if name.contains(&(b'.' as u16)) {
        return 1;
    }
    let e = if ext.is_null() {
        ".exe".encode_utf16().collect()
    } else {
        read_w(ext)
    };
    if p.len() + e.len() >= 260 {
        return 0;
    }
    write_w(path.add(p.len()), &e);
    1
}

/// Raccourcit un chemin en « début…\fichier » pour tenir dans `max`
/// caractères (zéro final compris).
pub fn compact_path(path: &[u16], max: usize) -> Vec<u16> {
    if max == 0 {
        return Vec::new();
    }
    let limit = max - 1;
    if path.len() <= limit {
        return path.to_vec();
    }
    let dots: Vec<u16> = "...".encode_utf16().collect();
    let cut = file_part(path);
    let name = &path[cut..];
    let sep = if cut > 0 {
        &path[cut - 1..cut]
    } else {
        &[][..]
    };
    if name.len() + sep.len() + dots.len() <= limit {
        let keep = limit - name.len() - sep.len() - dots.len();
        let mut out = path[..keep].to_vec();
        out.extend_from_slice(&dots);
        out.extend_from_slice(sep);
        out.extend_from_slice(name);
        return out;
    }
    if limit <= dots.len() {
        return dots[..limit].to_vec();
    }
    let mut out = name[..limit - dots.len()].to_vec();
    out.extend_from_slice(&dots);
    out
}

unsafe extern "win64" fn path_compact_path_ex_w(
    out: *mut u16,
    src: *const u16,
    max: u32,
    _flags: u32,
) -> i32 {
    if out.is_null() || src.is_null() || max == 0 {
        return 0;
    }
    write_w(out, &compact_path(&read_w(src), max as usize));
    1
}

unsafe extern "win64" fn path_get_drive_number_w(path: *const u16) -> i32 {
    if path.is_null() {
        return -1;
    }
    let p = read_w(path);
    let p = if p.starts_with(&"\\\\?\\".encode_utf16().collect::<Vec<_>>()) {
        &p[4..]
    } else {
        &p[..]
    };
    match p {
        [c, b, ..] if *b == b':' as u16 && (*c as u8 as char).is_ascii_alphabetic() && *c < 128 => {
            (((*c as u8) | 0x20) - b'a') as i32
        }
        _ => -1,
    }
}

unsafe extern "win64" fn path_is_network_path_w(path: *const u16) -> i32 {
    let p = read_w(path);
    (p.len() >= 2 && p[0] == b'\\' as u16 && p[1] == b'\\' as u16) as i32
}

unsafe extern "win64" fn path_strip_path_w(path: *mut u16) {
    if path.is_null() {
        return;
    }
    let p = read_w(path);
    let cut = file_part(&p);
    if cut > 0 {
        write_w(path, &p[cut..]);
    }
}

// ── shell32 ─────────────────────────────────────────────────────────────

// ── uxtheme ─────────────────────────────────────────────────────────────

unsafe extern "win64" fn draw_theme_text_ex(
    _theme: usize,
    hdc: usize,
    _part: i32,
    _state: i32,
    text: *const u16,
    cch: i32,
    flags: u32,
    rc: *mut Rect,
    opts: *const u32,
) -> i32 {
    const DTT_TEXTCOLOR: u32 = 1;
    const DTT_CALCRECT: u32 = 0x200;
    const DT_CALCRECT: u32 = 0x400;
    let mut flags = flags;
    let mut old = None;
    if !opts.is_null() && *opts >= 12 {
        let f = *opts.add(1);
        if f & DTT_TEXTCOLOR != 0 {
            old = api!(
                "gdi32",
                "SetTextColor",
                fn(usize, u32) -> u32,
                hdc,
                *opts.add(2)
            );
        }
        if f & DTT_CALCRECT != 0 {
            flags |= DT_CALCRECT;
        }
    }
    api!(
        "user32",
        "DrawTextW",
        fn(usize, *const u16, i32, *mut Rect, u32) -> i32,
        hdc,
        text,
        cch,
        rc,
        flags
    );
    if let Some(c) = old {
        api!("gdi32", "SetTextColor", fn(usize, u32) -> u32, hdc, c);
    }
    S_OK
}

extern "win64" fn draw_theme_parent_background(_hwnd: usize, _hdc: usize, _rc: usize) -> i32 {
    S_OK
}

extern "win64" fn enable_theme_dialog_texture(_hwnd: usize, _flags: u32) -> i32 {
    S_OK
}

unsafe extern "win64" fn get_theme_background_content_rect(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    bounds: *const Rect,
    out: *mut Rect,
) -> i32 {
    if bounds.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = *bounds;
    S_OK
}

unsafe extern "win64" fn get_theme_transition_duration(
    _theme: usize,
    _part: i32,
    _from: i32,
    _to: i32,
    _prop: i32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    S_OK
}

/// BeginBufferedAnimation sans durée : comme Windows dans ce cas, seul
/// l'état final est peint, directement dans le DC cible.
unsafe extern "win64" fn begin_buffered_animation(
    _hwnd: usize,
    hdc: usize,
    _rc: *const Rect,
    _fmt: u32,
    _paint: usize,
    _anim: usize,
    from: *mut usize,
    to: *mut usize,
) -> usize {
    if !from.is_null() {
        *from = 0;
    }
    if !to.is_null() {
        *to = hdc;
    }
    if hdc == 0 {
        0
    } else {
        hdc | 1
    }
}

extern "win64" fn end_buffered_animation(_h: usize, _update: i32) -> i32 {
    S_OK
}

extern "win64" fn buffered_paint_render_animation(_hwnd: usize, _hdc: usize) -> i32 {
    0
}

extern "win64" fn buffered_paint_stop_all_animations(_hwnd: usize) -> i32 {
    S_OK
}

// ── oleacc ──────────────────────────────────────────────────────────────

/// Pas encore d'arbre d'accessibilité : les applis gardent leur fenêtre.
unsafe extern "win64" fn create_std_accessible_object(
    _hwnd: usize,
    _id: i32,
    _riid: *const u8,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

extern "win64" fn lresult_from_object(_riid: *const u8, _wp: usize, _obj: usize) -> isize {
    E_NOTIMPL as isize
}

unsafe extern "win64" fn object_from_lresult(
    _lr: isize,
    _riid: *const u8,
    _wp: usize,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_FAIL
}

unsafe extern "win64" fn accessible_object_from_window(
    _hwnd: usize,
    _id: u32,
    _riid: *const u8,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_FAIL
}

unsafe extern "win64" fn get_role_text_w(_role: u32, buf: *mut u16, cap: u32) -> u32 {
    if !buf.is_null() && cap > 0 {
        *buf = 0;
    }
    0
}

// ── Tables ──────────────────────────────────────────────────────────────

/// Numéros d'ordre publiés des fonctions importées sans nom
/// (comctl32 v6, shell32).
pub const ORDINALS: &[(&str, u16, &str)] = &[
    ("winspool", 204, "SetDefaultPrinterW"),
    ("shlwapi", 515, "SHGetViewStatePropertyBag"),
    ("oleaut32", 185, "VariantTimeToSystemTime"),
    ("oleaut32", 314, "VarBstrCmp"),
    ("comctl32", 16, "CreateUpDownControl"),
    ("comctl32", 17, "InitCommonControls"),
    ("comctl32", 344, "TaskDialog"),
    ("comctl32", 345, "TaskDialogIndirect"),
    ("comctl32", 380, "LoadIconMetric"),
    ("comctl32", 381, "LoadIconWithScaleDown"),
    ("comctl32", 410, "SetWindowSubclass"),
    ("comctl32", 411, "GetWindowSubclass"),
    ("comctl32", 412, "RemoveWindowSubclass"),
    ("comctl32", 413, "DefSubclassProc"),
    ("shell32", 165, "SHCreateDirectory"),
    // Lot D du relevé (#1512) : numéros lus dans les bibliothèques
    // d'importation c/um/x64 (Cabinet.Lib, dsound.lib, Uxtheme.lib,
    // shell32.lib) du paquet NuGet Microsoft.Windows.SDK.CPP.x64
    // 10.0.26100.4654.
    ("cabinet", 10, "FCICreate"),
    ("cabinet", 11, "FCIAddFile"),
    ("cabinet", 12, "FCIFlushFolder"),
    ("cabinet", 13, "FCIFlushCabinet"),
    ("cabinet", 14, "FCIDestroy"),
    ("cabinet", 20, "FDICreate"),
    ("cabinet", 21, "FDIIsCabinet"),
    ("cabinet", 22, "FDICopy"),
    ("cabinet", 23, "FDIDestroy"),
    ("dsound", 1, "DirectSoundCreate"),
    ("dsound", 2, "DirectSoundEnumerateA"),
    ("dsound", 3, "DirectSoundEnumerateW"),
    ("dsound", 6, "DirectSoundCaptureCreate"),
    ("dsound", 7, "DirectSoundCaptureEnumerateA"),
    ("dsound", 8, "DirectSoundCaptureEnumerateW"),
    ("dsound", 11, "DirectSoundCreate8"),
    ("dsound", 12, "DirectSoundCaptureCreate8"),
    ("uxtheme", 47, "DrawThemeBackgroundEx"),
    ("shell32", 28, "SHILCreateFromPath"),
    ("shell32", 192, "SHUpdateImageW"),
    ("shell32", 232, "SHSetFolderPathW"),
    ("shell32", 256, "SHCreateShellFolderView"),
];

pub fn all() -> Vec<(&'static str, Exports)> {
    vec![
        (
            "comctl32",
            crate::exports! {
                "SetWindowSubclass" => fn set_window_subclass,
                "GetWindowSubclass" => fn get_window_subclass,
                "RemoveWindowSubclass" => fn remove_window_subclass,
                "DefSubclassProc" => fn def_subclass_proc,
                "LoadIconWithScaleDown" => fn load_icon_with_scale_down,
                "LoadIconMetric" => fn load_icon_metric,
                "_TrackMouseEvent" => fn track_mouse_event_cc,
            },
        ),
        (
            "user32",
            crate::exports! {
                "GetAncestor" => fn get_ancestor,
                "IsChild" => fn is_child,
                "EnumThreadWindows" => fn enum_thread_windows,
                "ChildWindowFromPoint" => fn child_window_from_point,
                "CharLowerW" => fn char_lower_w,
                "CharLowerBuffW" => fn char_lower_buff_w,
                "IsCharAlphaW" => fn is_char_alpha_w,
                "IsCharAlphaNumericW" => fn is_char_alpha_numeric_w,
                "IsCharLowerW" => fn is_char_lower_w,
                "IsCharUpperW" => fn is_char_upper_w,
                "SetClipboardViewer" => fn set_clipboard_viewer,
                "ChangeClipboardChain" => fn change_clipboard_chain,
                "LockWindowUpdate" => fn lock_window_update,
                "NotifyWinEvent" => fn notify_win_event,
                "SetMenuItemBitmaps" => fn set_menu_item_bitmaps,
                "mouse_event" => fn mouse_event,
                "GetScrollRange" => fn get_scroll_range,
                "GetComboBoxInfo" => fn get_combo_box_info,
                "GetMenuBarInfo" => fn get_menu_bar_info,
                "DrawFrameControl" => fn draw_frame_control,
            },
        ),
        (
            "gdi32",
            crate::exports! {
                "GdiAlphaBlend" => fn alpha_blend,
                "GdiTransparentBlt" => fn transparent_blt,
                "GdiGradientFill" => fn gradient_fill,
                "SetDIBits" => fn set_di_bits,
                "OffsetWindowOrgEx" => fn offset_window_org_ex,
                "RectVisible" => fn rect_visible,
                "PtVisible" => fn pt_visible,
            },
        ),
        (
            "msimg32",
            crate::exports! {
                "AlphaBlend" => fn alpha_blend,
                "TransparentBlt" => fn transparent_blt,
                "GradientFill" => fn gradient_fill,
            },
        ),
        (
            "kernel32",
            crate::exports! {
                "lstrcpynW" => fn lstrcpyn_w,
                "lstrcpynA" => fn lstrcpyn_a,
                "LCMapStringA" => fn lc_map_string_a,
                "GetStringTypeExW" => fn get_string_type_ex_w,
                "GetStringTypeExA" => fn get_string_type_ex_a,
                "RegisterApplicationRestart" => fn register_application_restart,
                "UnregisterApplicationRestart" => fn unregister_application_restart,
                "GetApplicationRestartSettings" => fn get_application_restart_settings,
            },
        ),
        (
            "shlwapi",
            crate::exports! {
                "ColorRGBToHLS" => fn color_rgb_to_hls,
                "ColorHLSToRGB" => fn color_hls_to_rgb,
                "ColorAdjustLuma" => fn color_adjust_luma,
                "PathAddExtensionW" => fn path_add_extension_w,
                "PathCompactPathExW" => fn path_compact_path_ex_w,
                "PathGetDriveNumberW" => fn path_get_drive_number_w,
                "PathIsNetworkPathW" => fn path_is_network_path_w,
                "PathStripPathW" => fn path_strip_path_w,
            },
        ),
        (
            "shell32",
            crate::exports! {
                "SHCreateItemFromParsingName" => fn super::filedialog_com::sh_create_item_from_parsing_name,
            },
        ),
        (
            "uxtheme",
            crate::exports! {
                "DrawThemeTextEx" => fn draw_theme_text_ex,
                "DrawThemeParentBackground" => fn draw_theme_parent_background,
                "EnableThemeDialogTexture" => fn enable_theme_dialog_texture,
                "GetThemeBackgroundContentRect" => fn get_theme_background_content_rect,
                "GetThemeTransitionDuration" => fn get_theme_transition_duration,
                "BeginBufferedAnimation" => fn begin_buffered_animation,
                "EndBufferedAnimation" => fn end_buffered_animation,
                "BufferedPaintRenderAnimation" => fn buffered_paint_render_animation,
                "BufferedPaintStopAllAnimations" => fn buffered_paint_stop_all_animations,
            },
        ),
        (
            "oleacc",
            crate::exports! {
                "CreateStdAccessibleObject" => fn create_std_accessible_object,
                "LresultFromObject" => fn lresult_from_object,
                "ObjectFromLresult" => fn object_from_lresult,
                "AccessibleObjectFromWindow" => fn accessible_object_from_window,
                "GetRoleTextW" => fn get_role_text_w,
                "GetStateTextW" => fn get_role_text_w,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn champ_d_une_liste_modifiable_jamais_la_liste_elle_meme() {
        // #1126 : Notepad++ sous-classe hwndItem et y envoie CB_GETDROPPEDSTATE
        // à la liste ; si hwndItem est la liste, l'appel se rappelle sans fin.
        unsafe {
            let class = w("COMBOBOX\0");
            let make = |style: u32| {
                crate::dlls::user32::create_window_ex_w(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    style,
                    0,
                    0,
                    200,
                    24,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            for (style, list) in [(0x2u32, false), (0x3, true)] {
                let h = make(style);
                assert_ne!(h, 0);
                let mut info = [0u8; 64];
                info[..4].copy_from_slice(&64u32.to_le_bytes());
                assert_eq!(get_combo_box_info(h, info.as_mut_ptr()), 1);
                let item = usize::from_le_bytes(info[48..56].try_into().unwrap());
                assert_eq!(item == h, list, "style {style:#x}");
                crate::dlls::user32::destroy_window(h);
            }
        }
    }

    #[test]
    fn couleurs_hls_aller_retour() {
        assert_eq!(rgb_to_hls(0x0000_00FF), (0, 120, 240)); // rouge
        assert_eq!(rgb_to_hls(0x00FF_FFFF), (160, 240, 0)); // blanc
        assert_eq!(rgb_to_hls(0), (160, 0, 0)); // noir
        for c in [
            0x0000_00FFu32,
            0x0000_FF00,
            0x00FF_0000,
            0x00F0_3D6A,
            0x0080_8080,
        ] {
            let (h, l, s) = rgb_to_hls(c);
            let back = hls_to_rgb(h, l, s);
            for sh in [0, 8, 16] {
                let a = ((c >> sh) & 0xFF) as i32;
                let b = ((back >> sh) & 0xFF) as i32;
                assert!((a - b).abs() <= 3, "{c:06X} → {back:06X}");
            }
        }
    }

    #[test]
    fn luminosite_ajustee() {
        let gris = 0x0080_8080;
        let clair = color_adjust_luma(gris, 500, 1);
        assert!((clair & 0xFF) > 0x80);
        let sombre = color_adjust_luma(gris, -500, 1);
        assert!((sombre & 0xFF) < 0x80);
        assert_eq!(color_adjust_luma(gris, 0, 1), gris);
    }

    #[test]
    fn chemins_compactes() {
        let p = w("C:\\Users\\damien\\Documents\\un-tres-long-dossier\\fichier.txt");
        let c = compact_path(&p, 30);
        assert!(c.len() <= 29);
        assert!(String::from_utf16_lossy(&c).ends_with("...\\fichier.txt"));
        assert_eq!(compact_path(&w("C:\\a.txt"), 30), w("C:\\a.txt"));
        assert_eq!(
            String::from_utf16_lossy(&compact_path(&w("tres-long-nom.txt"), 8)),
            "tres..."
        );
    }

    #[test]
    fn chemins_divers() {
        unsafe {
            let mut buf = [0u16; 64];
            write_w(buf.as_mut_ptr(), &w("C:\\dossier\\nom"));
            assert_eq!(path_add_extension_w(buf.as_mut_ptr(), std::ptr::null()), 1);
            assert_eq!(
                String::from_utf16_lossy(&read_w(buf.as_ptr())),
                "C:\\dossier\\nom.exe"
            );
            path_strip_path_w(buf.as_mut_ptr());
            assert_eq!(String::from_utf16_lossy(&read_w(buf.as_ptr())), "nom.exe");
            let d = [w("d:\\x"), vec![0]].concat();
            assert_eq!(path_get_drive_number_w(d.as_ptr()), 3);
            let n = [w("\\\\serveur\\partage"), vec![0]].concat();
            assert_eq!(path_is_network_path_w(n.as_ptr()), 1);
            assert_eq!(path_get_drive_number_w(n.as_ptr()), -1);
        }
    }

    #[test]
    fn lstrcpyn_tronque() {
        unsafe {
            let src = [w("bonjour"), vec![0]].concat();
            let mut dst = [0xFFFFu16; 8];
            lstrcpyn_w(dst.as_mut_ptr(), src.as_ptr(), 4);
            assert_eq!(&dst[..4], &w("bon\0")[..]);
            let mut a = [0xFFu8; 8];
            lstrcpyn_a(a.as_mut_ptr(), c"bonjour".as_ptr() as *const u8, 3);
            assert_eq!(&a[..3], b"bo\0");
        }
    }

    #[test]
    fn caracteres() {
        assert_eq!(lower_char('É' as u16), 'é' as u16);
        assert_eq!(is_char_alpha_w('é' as u16), 1);
        assert_eq!(is_char_alpha_w('1' as u16), 0);
        assert_eq!(is_char_alpha_numeric_w('1' as u16), 1);
        assert_eq!(is_char_lower_w('a' as u16), 1);
    }

    #[test]
    fn degrade_horizontal() {
        let a = TriVertex {
            x: 0,
            y: 0,
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        let b = TriVertex {
            x: 4,
            y: 2,
            r: 0xFF00,
            g: 0,
            b: 0,
            a: 0,
        };
        let (rect, px) = gradient_rect(&a, &b, false).unwrap();
        assert_eq!((rect.right, rect.bottom), (4, 2));
        assert_eq!(px[0], 0);
        assert_eq!(px[3] & 0xFF, 0xFF);
        assert_eq!(px[4], px[0]);
    }

    #[test]
    fn ordinaux_publies() {
        assert!(crate::dlls::lookup("comctl32", "#410").is_some());
        assert!(crate::dlls::lookup("comctl32", "#381").is_some());
        assert!(crate::dlls::lookup("msimg32", "AlphaBlend").is_some());
        assert!(crate::dlls::lookup("oleacc", "LresultFromObject").is_some());
        assert!(crate::dlls::lookup("user32", "GetAncestor").is_some());
        assert_eq!(get_ancestor(0xDEAD_0000, GA_ROOT), 0);
    }
}
