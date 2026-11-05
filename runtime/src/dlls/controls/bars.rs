//! Barres de comctl32 : barre d'outils (ToolbarWindow32), barre de bandes
//! (ReBarWindow32) et liste déroulante étendue (ComboBoxEx32).
//!
//! 7-Zip, les explorateurs de fichiers et beaucoup de programmes MFC/WTL
//! créent ces trois classes dans le WM_CREATE de leur fenêtre principale.
//! Absentes, CreateWindowEx échouait, le programme abandonnait sa mise en
//! page et sa fenêtre restait blanche.
//!
//! Les listes d'images de comctl32 ne gardent pas encore leurs pixels :
//! les boutons montrent leur texte, toujours, même quand le programme
//! demande des icônes seules (TB_SETMAXTEXTROWS 0), pour rester utilisables.

use super::*;

const WM_NOTIFY: u32 = 0x004E;

#[repr(C)]
#[derive(Clone, Copy)]
struct NmHdr {
    hwnd_from: usize,
    id_from: usize,
    code: i32,
}

fn notify_parent<T>(from: usize, target: usize, code: i32, data: &mut T) -> isize {
    let hdr = data as *mut T as *mut NmHdr;
    let id = user32::control_id(from) as usize;
    unsafe {
        (*hdr).hwnd_from = from;
        (*hdr).id_from = id;
        (*hdr).code = code;
    }
    send(target, WM_NOTIFY, id, data as *mut T as isize)
}

/// Texte lu en A ou W.
unsafe fn read_text(ptr: usize, wide: bool) -> Option<String> {
    if ptr == 0 || ptr == usize::MAX {
        return None;
    }
    Some(if wide {
        crate::dlls::wstr(ptr as *const u16)
    } else {
        crate::dlls::cstr(ptr as *const u8)
    })
}

/// Copie `s` dans un tampon A ou W de `cap` caractères ; renvoie la longueur.
unsafe fn write_text(ptr: usize, cap: i32, s: &str, wide: bool) -> isize {
    if ptr == 0 || cap <= 0 {
        return s.chars().count() as isize;
    }
    (if wide {
        crate::dlls::put_text_w(s, ptr as *mut u16, cap as u32)
    } else {
        crate::dlls::put_text_a(s, ptr as *mut u8, cap as u32)
    }) as isize
}

fn makelong(lo: i32, hi: i32) -> isize {
    ((lo as u16 as u32) | ((hi as u16 as u32) << 16)) as isize
}

pub(super) fn class_kind(name: &str) -> Option<Kind> {
    Some(match name {
        "toolbarwindow32" => Kind::ToolBar,
        "rebarwindow32" => Kind::ReBar,
        "comboboxex32" => Kind::ComboBoxEx,
        _ => return None,
    })
}

#[derive(Clone, Default)]
struct Button {
    image: i32,
    cmd: i32,
    state: u8,
    style: u8,
    data: usize,
    text: String,
    width: i32,
}

#[derive(Clone, Default)]
struct Band {
    child: usize,
    style: u32,
    text: String,
    image: i32,
    cx_min: u32,
    cy_min: u32,
    cx: u32,
    cy_child: u32,
    cy_max: u32,
    cy_integral: u32,
    cx_ideal: u32,
    id: u32,
    param: isize,
    header: u32,
    back: u32,
    fore: u32,
}

#[derive(Clone, Default)]
struct BarState {
    // Barre d'outils
    buttons: Vec<Button>,
    strings: Vec<String>,
    bitmap: (i32, i32),
    button_size: Option<(i32, i32)>,
    button_width: (i32, i32),
    images: i32,
    image_list: usize,
    hot_list: usize,
    disabled_list: usize,
    tooltips: usize,
    ex_style: u32,
    struct_size: usize,
    pressed: Option<usize>,
    indent: i32,
    // Bandes
    bands: Vec<Band>,
    last_height: i32,
    bk_color: u32,
    text_color: u32,
    // ComboBoxEx
    combo: usize,
    // Commun
    notify: usize,
    unicode: bool,
}

fn states() -> &'static Mutex<HashMap<usize, BarState>> {
    static S: OnceLock<Mutex<HashMap<usize, BarState>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with<R>(hwnd: usize, f: impl FnOnce(&mut BarState) -> R) -> R {
    let mut map = lock(states());
    f(map.entry(hwnd).or_default())
}

fn snap(hwnd: usize) -> BarState {
    lock(states()).get(&hwnd).cloned().unwrap_or_default()
}

pub(super) fn forget(hwnd: usize) {
    lock(states()).remove(&hwnd);
}

fn notify_target(hwnd: usize) -> usize {
    let t = snap(hwnd).notify;
    if t != 0 {
        t
    } else {
        parent(hwnd)
    }
}

pub(super) fn init(kind: Kind, hwnd: usize) {
    let wide = snapshot(hwnd).map(|c| c.wide).unwrap_or(true);
    with(hwnd, |s| {
        s.notify = parent(hwnd);
        s.bitmap = (16, 15);
        s.struct_size = 32;
        s.unicode = wide;
        s.bk_color = 0xFF00_0000; // CLR_DEFAULT
        s.text_color = 0xFF00_0000;
        s.button_width = (0, i32::MAX);
    });
    match kind {
        Kind::ToolBar => toolbar_autosize(hwnd),
        Kind::ReBar => rebar_layout(hwnd),
        Kind::ComboBoxEx => unsafe { cbex_create(hwnd, wide) },
        _ => {}
    }
}

pub(super) fn paint(kind: Kind, hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    match kind {
        Kind::ToolBar => paint_toolbar(hwnd, c, p, r),
        Kind::ReBar => paint_rebar(hwnd, p, r),
        Kind::ComboBoxEx => p.fill(r, face_color()),
        _ => {}
    }
}

/// Messages traités avant la procédure commune des contrôles (texte, police
/// et taille d'une ComboBoxEx vont à sa liste déroulante).
pub(super) unsafe fn pre_proc(
    kind: Kind,
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> Option<isize> {
    if kind != Kind::ComboBoxEx {
        return None;
    }
    let combo = snap(hwnd).combo;
    if combo == 0 {
        return None;
    }
    match msg {
        WM_SETTEXT | WM_GETTEXT | WM_GETTEXTLENGTH => Some(send(combo, msg, wp, lp)),
        WM_SETFONT => {
            with_ctl(hwnd, |c| c.font = wp);
            Some(send(combo, msg, wp, lp))
        }
        WM_SETFOCUS => {
            user32::set_focus(combo);
            Some(0)
        }
        WM_ENABLE => {
            let current = style(combo);
            let disabled = style(hwnd) & WS_DISABLED;
            gui::set_window_style(combo, (current & !WS_DISABLED) | disabled);
            invalidate(combo);
            Some(0)
        }
        _ => None,
    }
}

pub(super) unsafe fn proc_impl(
    kind: Kind,
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> Option<isize> {
    // CCM_SETUNICODEFORMAT / CCM_GETUNICODEFORMAT / CCM_SETVERSION / CCM_GETVERSION
    match msg {
        0x2005 => return Some(with(hwnd, |s| std::mem::replace(&mut s.unicode, wp != 0)) as isize),
        0x2006 => return Some(snap(hwnd).unicode as isize),
        0x2007 | 0x2008 => return Some(6),
        _ => {}
    }
    match kind {
        Kind::ToolBar => toolbar_proc(hwnd, msg, wp, lp),
        Kind::ReBar => rebar_proc(hwnd, msg, wp, lp),
        Kind::ComboBoxEx => cbex_proc(hwnd, msg, wp, lp),
        _ => None,
    }
}

// =====================================================================
// ToolbarWindow32
// =====================================================================

const TBSTATE_CHECKED: u8 = 0x01;
const TBSTATE_PRESSED: u8 = 0x02;
const TBSTATE_ENABLED: u8 = 0x04;
const TBSTATE_HIDDEN: u8 = 0x08;
const TBSTATE_INDETERMINATE: u8 = 0x10;
const TBSTATE_WRAP: u8 = 0x20;
const TBSTATE_MARKED: u8 = 0x80;
const BTNS_SEP: u8 = 0x01;
const BTNS_CHECK: u8 = 0x02;
const BTNS_GROUP: u8 = 0x04;
const BTNS_DROPDOWN: u8 = 0x08;
const BTNS_AUTOSIZE: u8 = 0x10;
const BTNS_WHOLEDROPDOWN: u8 = 0x80;
const TBSTYLE_WRAPABLE: u32 = 0x0200;
const TBSTYLE_LIST: u32 = 0x1000;
const CCS_BOTTOM: u32 = 0x3;
const CCS_NORESIZE: u32 = 0x4;
const CCS_NOPARENTALIGN: u32 = 0x8;
const CCS_VERT: u32 = 0x80;
const TBN_DROPDOWN: i32 = -710;
const TBN_HOTITEMCHANGE: i32 = -713;

/// TBBUTTON (64 bits).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TbButton {
    bitmap: i32,
    command: i32,
    state: u8,
    style: u8,
    reserved: [u8; 6],
    data: usize,
    string: isize,
}

/// NMTOOLBARA/W (64 bits).
#[repr(C)]
struct NmToolbar {
    hdr: NmHdr,
    item: i32,
    button: TbButton,
    cch: i32,
    text: usize,
    rect: Rect,
}

unsafe fn read_button(hwnd: usize, ptr: usize, wide: bool) -> Button {
    let b = std::ptr::read_unaligned(ptr as *const TbButton);
    let text = if (b.string as usize) >> 16 != 0 {
        read_text(b.string as usize, wide).unwrap_or_default()
    } else if b.string >= 0 {
        snap(hwnd)
            .strings
            .get(b.string as usize)
            .cloned()
            .unwrap_or_default()
    } else {
        String::new()
    };
    Button {
        image: b.bitmap,
        cmd: b.command,
        state: b.state,
        style: b.style,
        data: b.data,
        text: strip_prefix(&text),
        width: 0,
    }
}

fn index_of(s: &BarState, cmd: i32) -> Option<usize> {
    s.buttons.iter().position(|b| b.cmd == cmd)
}

fn toolbar_font(hwnd: usize) -> usize {
    snapshot(hwnd)
        .map(|c| font_of(&c))
        .unwrap_or_else(|| gdi32::get_stock_object(17))
}

/// Place de chaque bouton dans la zone client (vide pour un bouton caché).
fn toolbar_layout(hwnd: usize, s: &BarState) -> Vec<Rect> {
    let font = toolbar_font(hwnd);
    let lh = line_height(font);
    let has_text = s.buttons.iter().any(|b| !b.text.is_empty());
    let list = style(hwnd) & TBSTYLE_LIST != 0;
    let vertical = style(hwnd) & CCS_VERT != 0;
    let mut h = if has_text { lh + 14 } else { s.bitmap.1 + 9 }.max(24);
    let mut uniform = 0;
    let width_of = |b: &Button| -> i32 {
        let w = if b.text.is_empty() {
            s.bitmap.0 + 9
        } else {
            measure(font, &b.text).0 + if list { 20 } else { 16 }
        };
        w.max(24)
    };
    for b in &s.buttons {
        if b.style & BTNS_SEP == 0 && b.style & BTNS_AUTOSIZE == 0 {
            uniform = i32::max(uniform, width_of(b));
        }
    }
    if let Some((bw, bh)) = s.button_size {
        h = h.max(bh);
        uniform = uniform.max(bw);
    }
    let uniform = uniform.clamp(s.button_width.0, s.button_width.1.max(s.button_width.0));
    let avail = client(hwnd).width();
    let wrap = style(hwnd) & TBSTYLE_WRAPABLE != 0 && avail > 0;
    let mut out = Vec::with_capacity(s.buttons.len());
    let (mut x, mut y) = (s.indent + 2, 2);
    for b in &s.buttons {
        if b.state & TBSTATE_HIDDEN != 0 {
            out.push(Rect::default());
            continue;
        }
        let mut w = if b.style & BTNS_SEP != 0 {
            if b.image > 0 {
                b.image
            } else {
                8
            }
        } else if b.width > 0 {
            b.width
        } else if b.style & BTNS_AUTOSIZE != 0 {
            width_of(b)
        } else {
            uniform
        };
        if b.style & (BTNS_DROPDOWN | BTNS_WHOLEDROPDOWN) != 0 && b.width == 0 {
            w += 14;
        }
        if vertical {
            let r = Rect::from_size(
                2,
                y,
                uniform.max(w),
                if b.style & BTNS_SEP != 0 { 8 } else { h },
            );
            y = r.bottom;
            out.push(r);
            continue;
        }
        if wrap && x > s.indent + 2 && x + w > avail {
            x = s.indent + 2;
            y += h + 2;
        }
        out.push(Rect::from_size(x, y, w, h));
        x += w;
        if b.state & TBSTATE_WRAP != 0 {
            x = s.indent + 2;
            y += h + 2;
        }
    }
    out
}

fn toolbar_extent(hwnd: usize, s: &BarState) -> (i32, i32) {
    let rects = toolbar_layout(hwnd, s);
    let w = rects.iter().map(|r| r.right).max().unwrap_or(0) + 2;
    let font = toolbar_font(hwnd);
    let empty_h = (s.bitmap.1 + 9).max(24).max(line_height(font) + 14);
    let h = rects
        .iter()
        .filter(|r| r.height() > 0)
        .map(|r| r.bottom)
        .max()
        .unwrap_or(empty_h)
        + 2;
    (w.max(1), h.max(1))
}

/// TB_AUTOSIZE : hauteur ajustée aux boutons et, sauf CCS_NOPARENTALIGN,
/// barre collée en haut (ou en bas) du parent sur toute sa largeur.
fn toolbar_autosize(hwnd: usize) {
    let st = style(hwnd);
    if st & CCS_NORESIZE != 0 {
        return;
    }
    let s = snap(hwnd);
    let (_, h) = toolbar_extent(hwnd, &s);
    let Some(cur) = gui::window_rect(hwnd) else {
        return;
    };
    let (x, y, w) = if st & CCS_NOPARENTALIGN != 0 {
        (cur.left, cur.top, cur.width().max(1))
    } else {
        let pr = gui::window_client_rect(parent(hwnd)).unwrap_or_default();
        let y = if st & 0x3 == CCS_BOTTOM {
            pr.height() - h
        } else {
            0
        };
        (0, y, pr.width().max(1))
    };
    if Rect::from_size(x, y, w, h) != cur {
        unsafe {
            user32::set_window_pos(hwnd, 0, x, y, w, h, 0x14);
        }
    }
    invalidate(hwnd);
}

fn paint_toolbar(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, 0, face_color(), p, r);
    let s = snap(hwnd);
    let rects = toolbar_layout(hwnd, &s);
    let _ = c;
    for (i, (b, br)) in s.buttons.iter().zip(&rects).enumerate() {
        if br.width() <= 0 {
            continue;
        }
        if b.style & BTNS_SEP != 0 {
            let cx = (br.left + br.right) / 2;
            if style(hwnd) & CCS_VERT != 0 {
                let cy = (br.top + br.bottom) / 2;
                p.line(br.left + 2, cy, br.right - 2, cy, CONTROL_BORDER, 1);
            } else {
                p.line(cx, br.top + 4, cx, br.bottom - 4, CONTROL_BORDER, 1);
            }
            continue;
        }
        let enabled = b.state & TBSTATE_ENABLED != 0 && enabled(hwnd);
        let down = s.pressed == Some(i) || b.state & TBSTATE_PRESSED != 0;
        let checked = b.state & TBSTATE_CHECKED != 0;
        let inner = Rect {
            left: br.left + 1,
            top: br.top + 1,
            right: br.right - 1,
            bottom: br.bottom - 1,
        };
        if down {
            p.shape(inner, CONTROL_PRESSED, CONTROL_BORDER, 4, false);
        } else if checked || b.state & TBSTATE_MARKED != 0 {
            p.shape(inner, SELECTION_SOFT, CONTROL_BORDER, 4, false);
        }
        let color = if enabled { TEXT } else { TEXT_DISABLED };
        let mut tr = Rect {
            left: br.left + 4,
            right: br.right - 4,
            ..*br
        };
        if b.style & (BTNS_DROPDOWN | BTNS_WHOLEDROPDOWN) != 0 {
            tr.right -= 12;
            p.text(
                Rect {
                    left: tr.right,
                    right: br.right - 2,
                    ..*br
                },
                "▾",
                color,
                1,
            );
        }
        // Image de la liste (désactivée si elle existe et que le bouton l'est).
        let use_disabled = !enabled
            && s.disabled_list != 0
            && crate::dlls::icons::list_image(s.disabled_list, b.image).is_some();
        let list = if use_disabled {
            s.disabled_list
        } else {
            s.image_list
        };
        let size = (list != 0 && b.image >= 0)
            .then(|| crate::dlls::icons::list_size(list))
            .flatten();
        if let Some((iw, ih)) = size {
            let x = if b.text.is_empty() {
                (tr.left + tr.right - iw) / 2
            } else {
                tr.left
            };
            let y = if b.text.is_empty() {
                (tr.top + tr.bottom - ih) / 2
            } else {
                tr.top + 3
            };
            let bg = (!down && !checked).then(face_color);
            if !enabled && !use_disabled {
                // Pas d'image grisée fournie : l'image normale, estompée.
                if let Some(mut img) = crate::dlls::icons::list_image(list, b.image) {
                    for px in &mut img.px {
                        let a = (*px >> 24) * 2 / 5;
                        let g = ((*px & 0xFF) + ((*px >> 8) & 0xFF) + ((*px >> 16) & 0xFF)) / 3;
                        *px = (a << 24) | (g << 16) | (g << 8) | g;
                    }
                    crate::dlls::icons::draw(p.hdc, x, y, iw, ih, &img, bg);
                }
            } else {
                crate::dlls::icons::draw_list_image(list, b.image, p.hdc, x, y, bg);
            }
            if !b.text.is_empty() {
                let below = Rect { top: y + ih, ..tr };
                p.text(below, &b.text, color, 1);
            }
        } else if b.text.is_empty() {
            // Pas d'image affichable : petit repère pour que le bouton se voie.
            let cx = (tr.left + tr.right) / 2;
            let cy = (tr.top + tr.bottom) / 2;
            p.shape(Rect::from_size(cx - 3, cy - 3, 6, 6), color, color, 3, true);
        } else {
            p.text(tr, &b.text, color, 1);
        }
        if b.state & TBSTATE_INDETERMINATE != 0 {
            p.line(
                inner.left + 3,
                inner.bottom - 3,
                inner.right - 3,
                inner.bottom - 3,
                TEXT_DISABLED,
                1,
            );
        }
    }
}

/// Image d'un TBADDBITMAP : HBITMAP (hInst nul) ou ressource bitmap. La
/// couleur du coin haut-gauche sert de fond transparent, comme les bandes
/// classiques des barres d'outils.
fn tb_bitmap_image(inst: usize, id: usize) -> Option<crate::dlls::icons::Image> {
    const HINST_COMMCTRL: usize = usize::MAX;
    if inst == HINST_COMMCTRL {
        return None;
    }
    let mut img = if inst == 0 {
        let (w, h, px, alpha) = gui::bitmap_pixels(id)?;
        let px = if alpha {
            px
        } else {
            px.iter().map(|p| p | 0xFF00_0000).collect()
        };
        crate::dlls::icons::Image { w, h, px }
    } else {
        crate::dlls::icons::load_bitmap_resource(inst, id)?
    };
    let has_alpha = img.px.iter().any(|p| p >> 24 != 0xFF);
    if !has_alpha {
        if let Some(&key) = img.px.first() {
            for p in &mut img.px {
                if *p == key {
                    *p = 0;
                }
            }
        }
    }
    Some(img)
}

fn hit_button(hwnd: usize, s: &BarState, x: i32, y: i32) -> Option<usize> {
    toolbar_layout(hwnd, s)
        .iter()
        .position(|r| r.width() > 0 && x >= r.left && x < r.right && y >= r.top && y < r.bottom)
}

fn tb_notify_button(hwnd: usize, code: i32, i: usize, s: &BarState, rect: Rect) -> isize {
    let b = &s.buttons[i];
    let mut nm = NmToolbar {
        hdr: NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        },
        item: b.cmd,
        button: TbButton {
            bitmap: b.image,
            command: b.cmd,
            state: b.state,
            style: b.style,
            reserved: [0; 6],
            data: b.data,
            string: -1,
        },
        cch: 0,
        text: 0,
        rect,
    };
    notify_parent(hwnd, notify_target(hwnd), code, &mut nm)
}

fn tb_click(hwnd: usize, i: usize) {
    let (cmd, style) = {
        let s = snap(hwnd);
        let Some(b) = s.buttons.get(i) else {
            return;
        };
        (b.cmd, b.style)
    };
    if style & BTNS_CHECK != 0 {
        with(hwnd, |s| {
            let group = style & BTNS_GROUP != 0;
            let was = s.buttons[i].state & TBSTATE_CHECKED != 0;
            if group {
                // Groupe : boutons BTNS_GROUP voisins, un seul enfoncé.
                let (mut a, mut z) = (i, i);
                while a > 0 && s.buttons[a - 1].style & BTNS_GROUP != 0 {
                    a -= 1;
                }
                while z + 1 < s.buttons.len() && s.buttons[z + 1].style & BTNS_GROUP != 0 {
                    z += 1;
                }
                for b in &mut s.buttons[a..=z] {
                    b.state &= !TBSTATE_CHECKED;
                }
                s.buttons[i].state |= TBSTATE_CHECKED;
            } else if was {
                s.buttons[i].state &= !TBSTATE_CHECKED;
            } else {
                s.buttons[i].state |= TBSTATE_CHECKED;
            }
        });
    }
    send(
        notify_target(hwnd),
        WM_COMMAND,
        cmd as u16 as usize,
        hwnd as isize,
    );
}

unsafe fn add_strings(hwnd: usize, wp: usize, lp: isize, wide: bool) -> isize {
    let mut list = Vec::new();
    if wp != 0 && (lp as usize) >> 16 == 0 {
        // Ressource chaîne : son premier caractère sépare les libellés.
        let mut buf = [0u16; 1024];
        let n = crate::dlls::resource::load_string_w(wp, lp as u32, buf.as_mut_ptr(), 1024);
        let s = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let mut chars = s.chars();
        if let Some(sep) = chars.next() {
            let rest: String = chars.collect();
            list.extend(rest.split(sep).filter(|t| !t.is_empty()).map(String::from));
        }
    } else if lp != 0 {
        let mut p = lp as usize;
        loop {
            let s = read_text(p, wide).unwrap_or_default();
            if s.is_empty() {
                break;
            }
            let units = if wide {
                s.encode_utf16().count() * 2 + 2
            } else {
                crate::dlls::encode_ansi(&s).len() + 1
            };
            list.push(s);
            p += units;
            if list.len() > 4096 {
                break;
            }
        }
    }
    if list.is_empty() {
        return -1;
    }
    with(hwnd, |s| {
        let first = s.strings.len() as isize;
        s.strings.extend(list);
        first
    })
}

/// TBBUTTONINFOA/W (64 bits) : cbSize, dwMask, idCommand, iImage, fsState,
/// fsStyle, cx, lParam, pszText, cchText.
unsafe fn button_info(hwnd: usize, wp: usize, lp: isize, set: bool, wide: bool) -> isize {
    if lp == 0 {
        return -1;
    }
    let p = lp as *mut u8;
    let mask = *(p.add(4) as *const u32);
    let s = snap(hwnd);
    let idx = if mask & 0x8000_0000 != 0 {
        Some(wp).filter(|&i| i < s.buttons.len())
    } else {
        index_of(&s, wp as i32)
    };
    let Some(i) = idx else {
        return -1;
    };
    if set {
        let text = if mask & 0x2 != 0 {
            read_text(*(p.add(32) as *const usize), wide)
        } else {
            None
        };
        with(hwnd, |s| {
            let b = &mut s.buttons[i];
            if mask & 0x20 != 0 {
                b.cmd = *(p.add(8) as *const i32);
            }
            if mask & 0x1 != 0 {
                b.image = *(p.add(12) as *const i32);
            }
            if mask & 0x4 != 0 {
                b.state = *p.add(16);
            }
            if mask & 0x8 != 0 {
                b.style = *p.add(17);
            }
            if mask & 0x40 != 0 {
                b.width = *(p.add(18) as *const u16) as i32;
            }
            if mask & 0x10 != 0 {
                b.data = *(p.add(24) as *const usize);
            }
            if let Some(t) = text {
                b.text = strip_prefix(&t);
            }
        });
        invalidate(hwnd);
        return 1;
    }
    let b = &s.buttons[i];
    let rects = toolbar_layout(hwnd, &s);
    if mask & 0x20 != 0 {
        *(p.add(8) as *mut i32) = b.cmd;
    }
    if mask & 0x1 != 0 {
        *(p.add(12) as *mut i32) = b.image;
    }
    if mask & 0x4 != 0 {
        *p.add(16) = b.state;
    }
    if mask & 0x8 != 0 {
        *p.add(17) = b.style;
    }
    if mask & 0x40 != 0 {
        *(p.add(18) as *mut u16) = rects[i].width().max(0) as u16;
    }
    if mask & 0x10 != 0 {
        *(p.add(24) as *mut usize) = b.data;
    }
    if mask & 0x2 != 0 {
        let buf = *(p.add(32) as *const usize);
        let cap = *(p.add(40) as *const i32);
        write_text(buf, cap, &b.text, wide);
    }
    i as isize
}

fn set_state_bit(hwnd: usize, cmd: i32, bit: u8, on: bool) -> isize {
    let done = with(hwnd, |s| match index_of(s, cmd) {
        Some(i) => {
            if on {
                s.buttons[i].state |= bit;
            } else {
                s.buttons[i].state &= !bit;
            }
            true
        }
        None => false,
    });
    if done {
        invalidate(hwnd);
    }
    done as isize
}

fn state_bit(hwnd: usize, cmd: i32, bit: u8) -> isize {
    let s = snap(hwnd);
    match index_of(&s, cmd) {
        Some(i) => (s.buttons[i].state & bit) as isize,
        None => -1,
    }
}

unsafe fn toolbar_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let wide = |m: u32, w: u32| m == w;
    let r = match msg {
        0x0401 => set_state_bit(hwnd, wp as i32, TBSTATE_ENABLED, lp as u16 != 0), // TB_ENABLEBUTTON
        0x0402 => set_state_bit(hwnd, wp as i32, TBSTATE_CHECKED, lp as u16 != 0), // TB_CHECKBUTTON
        0x0403 => set_state_bit(hwnd, wp as i32, TBSTATE_PRESSED, lp as u16 != 0), // TB_PRESSBUTTON
        0x0404 => set_state_bit(hwnd, wp as i32, TBSTATE_HIDDEN, lp as u16 != 0),  // TB_HIDEBUTTON
        0x0405 => set_state_bit(hwnd, wp as i32, TBSTATE_INDETERMINATE, lp as u16 != 0),
        0x0406 => set_state_bit(hwnd, wp as i32, TBSTATE_MARKED, lp as u16 != 0), // TB_MARKBUTTON
        0x0409 => state_bit(hwnd, wp as i32, TBSTATE_ENABLED), // TB_ISBUTTONENABLED
        0x040A => state_bit(hwnd, wp as i32, TBSTATE_CHECKED),
        0x040B => state_bit(hwnd, wp as i32, TBSTATE_PRESSED),
        0x040C => state_bit(hwnd, wp as i32, TBSTATE_HIDDEN),
        0x040D => state_bit(hwnd, wp as i32, TBSTATE_INDETERMINATE),
        0x040E => state_bit(hwnd, wp as i32, TBSTATE_MARKED),
        0x0411 => {
            // TB_SETSTATE
            let done = with(hwnd, |s| match index_of(s, wp as i32) {
                Some(i) => {
                    s.buttons[i].state = lp as u8;
                    true
                }
                None => false,
            });
            invalidate(hwnd);
            done as isize
        }
        0x0412 => {
            let s = snap(hwnd);
            index_of(&s, wp as i32).map_or(-1, |i| s.buttons[i].state as isize)
        }
        0x0413 => {
            // TB_ADDBITMAP : index de la première image ajoutée. Les
            // images vont dans une liste interne (cases de TB_SETBITMAPSIZE).
            let strip = if lp != 0 {
                let p = lp as *const usize;
                tb_bitmap_image(*p, *p.add(1))
            } else {
                None
            };
            let (first, list, size) = with(hwnd, |s| {
                let first = s.images;
                s.images += wp as i32;
                if s.image_list == 0 && strip.is_some() {
                    let (w, h) = s.bitmap;
                    s.image_list = crate::dlls::icons::create_list(w as usize, h as usize, 0x21);
                }
                (first, s.image_list, s.bitmap)
            });
            if let (Some(img), true) = (strip, list != 0) {
                let have = crate::dlls::icons::image_list_get_image_count(list);
                // Garde les index alignés si des images manquaient avant.
                for _ in have..first {
                    crate::dlls::icons::list_add(
                        list,
                        crate::dlls::icons::Image::blank(size.0 as usize, size.1 as usize),
                    );
                }
                crate::dlls::icons::list_add_strip(list, &img);
            }
            first as isize
        }
        0x0414 | 0x0444 | 0x0415 | 0x0443 => {
            // TB_ADDBUTTONSA/W (nombre, tableau), TB_INSERTBUTTONA/W (index, bouton)
            let w = msg == 0x0444 || msg == 0x0443;
            let insert = msg == 0x0415 || msg == 0x0443;
            if lp == 0 {
                return Some(0);
            }
            let stride = snap(hwnd).struct_size.clamp(24, 64);
            let n = if insert { 1 } else { wp.min(4096) };
            let new: Vec<Button> = (0..n)
                .map(|i| read_button(hwnd, lp as usize + i * stride, w))
                .collect();
            with(hwnd, |s| {
                let at = if insert {
                    wp.min(s.buttons.len())
                } else {
                    s.buttons.len()
                };
                s.buttons.splice(at..at, new);
            });
            invalidate(hwnd);
            1
        }
        0x0416 => {
            // TB_DELETEBUTTON
            let ok = with(hwnd, |s| {
                if wp < s.buttons.len() {
                    s.buttons.remove(wp);
                    true
                } else {
                    false
                }
            });
            invalidate(hwnd);
            ok as isize
        }
        0x0417 => {
            // TB_GETBUTTON
            let s = snap(hwnd);
            match s.buttons.get(wp) {
                Some(b) if lp != 0 => {
                    std::ptr::write_unaligned(
                        lp as *mut TbButton,
                        TbButton {
                            bitmap: b.image,
                            command: b.cmd,
                            state: b.state,
                            style: b.style,
                            reserved: [0; 6],
                            data: b.data,
                            string: -1,
                        },
                    );
                    1
                }
                _ => 0,
            }
        }
        0x0418 => snap(hwnd).buttons.len() as isize, // TB_BUTTONCOUNT
        0x0419 => index_of(&snap(hwnd), wp as i32).map_or(-1, |i| i as isize),
        0x041A | 0x044C | 0x041B => 0, // TB_SAVERESTORE, TB_CUSTOMIZE
        0x041C | 0x044D => add_strings(hwnd, wp, lp, wide(msg, 0x044D)),
        0x041D | 0x0433 => {
            // TB_GETITEMRECT (index) / TB_GETRECT (commande)
            let s = snap(hwnd);
            let i = if msg == 0x041D {
                Some(wp).filter(|&i| i < s.buttons.len())
            } else {
                index_of(&s, wp as i32)
            };
            match i {
                Some(i) if lp != 0 => {
                    *(lp as *mut Rect) = toolbar_layout(hwnd, &s)[i];
                    1
                }
                _ => 0,
            }
        }
        0x041E => {
            with(hwnd, |s| s.struct_size = wp);
            0
        }
        0x041F => {
            let (w, h) = point(lp);
            with(hwnd, |s| s.button_size = Some((w, h)));
            invalidate(hwnd);
            1
        }
        0x0420 => {
            let (w, h) = point(lp);
            with(hwnd, |s| s.bitmap = (w.max(1), h.max(1)));
            invalidate(hwnd);
            1
        }
        0x0421 => {
            toolbar_autosize(hwnd);
            0
        }
        0x0423 => snap(hwnd).tooltips as isize,
        0x0424 => {
            with(hwnd, |s| s.tooltips = wp);
            0
        }
        0x0425 => with(hwnd, |s| std::mem::replace(&mut s.notify, wp)) as isize, // TB_SETPARENT
        0x0427 => 0,                                                             // TB_SETROWS
        0x0428 => {
            // TB_GETROWS
            let s = snap(hwnd);
            let mut tops: Vec<i32> = toolbar_layout(hwnd, &s)
                .iter()
                .filter(|r| r.width() > 0)
                .map(|r| r.top)
                .collect();
            tops.dedup();
            tops.len().max(1) as isize
        }
        0x0429 => 0, // TB_GETBITMAPFLAGS
        0x042A => {
            // TB_SETCMDID
            with(hwnd, |s| match s.buttons.get_mut(wp) {
                Some(b) => {
                    b.cmd = lp as i32;
                    1
                }
                None => 0,
            })
        }
        0x042B => {
            // TB_CHANGEBITMAP
            with(hwnd, |s| match index_of(s, wp as i32) {
                Some(i) => {
                    s.buttons[i].image = lp as i32;
                    1
                }
                None => 0,
            })
        }
        0x042C => {
            let s = snap(hwnd);
            index_of(&s, wp as i32).map_or(-1, |i| s.buttons[i].image as isize)
        }
        0x042D | 0x044B => {
            // TB_GETBUTTONTEXTA/W
            let s = snap(hwnd);
            match index_of(&s, wp as i32) {
                Some(i) => {
                    let t = &s.buttons[i].text;
                    if lp != 0 {
                        write_text(lp as usize, i32::MAX / 4, t, msg == 0x044B)
                    } else {
                        t.chars().count() as isize
                    }
                }
                None => -1,
            }
        }
        0x042E => 1, // TB_REPLACEBITMAP
        0x042F => {
            with(hwnd, |s| s.indent = wp as i32);
            invalidate(hwnd);
            1
        }
        0x0430 => with(hwnd, |s| std::mem::replace(&mut s.image_list, lp as usize)) as isize,
        0x0431 => snap(hwnd).image_list as isize,
        0x0432 => 0, // TB_LOADIMAGES
        0x0434 => with(hwnd, |s| std::mem::replace(&mut s.hot_list, lp as usize)) as isize,
        0x0435 => snap(hwnd).hot_list as isize,
        0x0436 => with(hwnd, |s| {
            std::mem::replace(&mut s.disabled_list, lp as usize)
        }) as isize,
        0x0437 => snap(hwnd).disabled_list as isize,
        0x0438 => {
            gui::set_window_style(hwnd, lp as u32);
            invalidate(hwnd);
            0
        }
        0x0439 => style(hwnd) as isize,
        0x043A => {
            // TB_GETBUTTONSIZE
            let s = snap(hwnd);
            let r = toolbar_layout(hwnd, &s)
                .into_iter()
                .zip(&s.buttons)
                .find(|(r, b)| r.width() > 0 && b.style & BTNS_SEP == 0)
                .map(|(r, _)| r);
            match r {
                Some(r) => makelong(r.width(), r.height()),
                None => {
                    let (w, h) = s.button_size.unwrap_or((24, 22));
                    makelong(w, h)
                }
            }
        }
        0x043B => {
            let (lo, hi) = point(lp);
            with(hwnd, |s| {
                s.button_width = (lo.max(0), if hi <= 0 { i32::MAX } else { hi })
            });
            invalidate(hwnd);
            1
        }
        0x043C => 0, // TB_SETMAXTEXTROWS : le texte reste affiché (pas d'images)
        0x043D => 1, // TB_GETTEXTROWS
        0x043F | 0x0441 => button_info(hwnd, wp, lp, false, msg == 0x043F),
        0x0440 | 0x0442 => button_info(hwnd, wp, lp, true, msg == 0x0440),
        0x0445 => {
            // TB_HITTEST
            if lp == 0 {
                return Some(-1);
            }
            let pt = *(lp as *const [i32; 2]);
            let s = snap(hwnd);
            hit_button(hwnd, &s, pt[0], pt[1]).map_or(-1, |i| i as isize)
        }
        0x0446 => 0,          // TB_SETDRAWTEXTFLAGS
        0x0447 => -1,         // TB_GETHOTITEM
        0x0448 => -1,         // TB_SETHOTITEM
        0x044E | 0x045A => 0, // TB_MAPACCELERATOR
        0x0453 | 0x0463 => {
            // TB_GETMAXSIZE (SIZE*) / TB_GETIDEALSIZE (hauteur?, SIZE*)
            if lp == 0 {
                return Some(0);
            }
            let (w, h) = toolbar_extent(hwnd, &snap(hwnd));
            let out = lp as *mut [i32; 2];
            if msg == 0x0463 && wp != 0 {
                (*out)[1] = h;
            } else if msg == 0x0463 {
                (*out)[0] = w;
            } else {
                *out = [w, h];
            }
            1
        }
        0x0454 => with(hwnd, |s| std::mem::replace(&mut s.ex_style, lp as u32)) as isize,
        0x0455 => snap(hwnd).ex_style as isize,
        0x0456 => makelong(7, 6), // TB_GETPADDING
        0x0457 => makelong(7, 6), // TB_SETPADDING : ancien remplissage
        0x045B | 0x045C => {
            // TB_GETSTRINGW/A : wp = MAKEWPARAM(taille, index)
            let s = snap(hwnd);
            let (cap, idx) = ((wp & 0xFFFF) as i32, (wp >> 16) & 0xFFFF);
            match s.strings.get(idx) {
                Some(t) => write_text(lp as usize, cap, t, msg == 0x045B),
                None => -1,
            }
        }
        0x0460 | 0x0465 | 0x0466 => 0, // TB_SETLISTGAP, TB_GETMETRICS, TB_SETMETRICS
        WM_SIZE => {
            if style(hwnd) & (CCS_NORESIZE | CCS_NOPARENTALIGN) == 0 {
                toolbar_autosize(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let (x, y) = point(lp);
            let s = snap(hwnd);
            if let Some(i) = hit_button(hwnd, &s, x, y) {
                let b = &s.buttons[i];
                if b.style & BTNS_SEP == 0 && b.state & TBSTATE_ENABLED != 0 {
                    let rect = toolbar_layout(hwnd, &s)[i];
                    let arrow = b.style & BTNS_WHOLEDROPDOWN != 0
                        || (b.style & BTNS_DROPDOWN != 0 && x >= rect.right - 14);
                    if arrow {
                        tb_notify_button(hwnd, TBN_DROPDOWN, i, &s, rect);
                    } else {
                        with(hwnd, |s| s.pressed = Some(i));
                        gui::set_capture(hwnd);
                        invalidate(hwnd);
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let Some(i) = with(hwnd, |s| s.pressed.take()) else {
                return Some(0);
            };
            gui::release_capture();
            invalidate(hwnd);
            let (x, y) = point(lp);
            if hit_button(hwnd, &snap(hwnd), x, y) == Some(i) {
                tb_click(hwnd, i);
            }
            0
        }
        WM_MOUSEMOVE => {
            let _ = TBN_HOTITEMCHANGE;
            0
        }
        // Contrôles posés sur la barre (liste d'adresse de 7-Zip) : leurs
        // notifications vont au destinataire de la barre.
        WM_COMMAND | WM_NOTIFY => send(notify_target(hwnd), msg, wp, lp),
        WM_GETDLGCODE => 0,
        _ if (0x0400..0x0500).contains(&msg) => 0,
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// ReBarWindow32
// =====================================================================

const RBBS_BREAK: u32 = 0x1;
const RBBS_HIDDEN: u32 = 0x8;
const RBBS_VARIABLEHEIGHT: u32 = 0x40;
const RBBS_NOGRIPPER: u32 = 0x100;
const RBBS_HIDETITLE: u32 = 0x400;
const RBS_BANDBORDERS: u32 = 0x400;
const RBN_HEIGHTCHANGE: i32 = -831;

/// REBARBANDINFOA/W (64 bits) : lit les champs indiqués par fMask.
unsafe fn read_band(p: *const u8, band: &mut Band, wide: bool) {
    let size = *(p as *const u32) as usize;
    let mask = *(p.add(4) as *const u32);
    let field = |off: usize| size >= off + 4;
    if mask & 0x1 != 0 {
        band.style = *(p.add(8) as *const u32);
    }
    if mask & 0x2 != 0 {
        band.fore = *(p.add(12) as *const u32);
        band.back = *(p.add(16) as *const u32);
    }
    if mask & 0x4 != 0 {
        band.text = read_text(*(p.add(24) as *const usize), wide).unwrap_or_default();
    }
    if mask & 0x8 != 0 {
        band.image = *(p.add(36) as *const i32);
    }
    if mask & 0x10 != 0 {
        band.child = *(p.add(40) as *const usize);
    }
    if mask & 0x20 != 0 {
        band.cx_min = *(p.add(48) as *const u32);
        band.cy_min = *(p.add(52) as *const u32);
        if field(76) {
            band.cy_child = *(p.add(76) as *const u32);
            band.cy_max = *(p.add(80) as *const u32);
            band.cy_integral = *(p.add(84) as *const u32);
        }
    }
    if mask & 0x40 != 0 {
        band.cx = *(p.add(56) as *const u32);
    }
    if mask & 0x100 != 0 {
        band.id = *(p.add(72) as *const u32);
    }
    if mask & 0x200 != 0 && field(88) {
        band.cx_ideal = *(p.add(88) as *const u32);
    }
    if mask & 0x400 != 0 && size >= 104 {
        band.param = *(p.add(96) as *const isize);
    }
    if mask & 0x800 != 0 && field(104) {
        band.header = *(p.add(104) as *const u32);
    }
}

unsafe fn write_band(p: *mut u8, band: &Band, wide: bool) {
    let size = *(p as *const u32) as usize;
    let mask = *(p.add(4) as *const u32);
    let field = |off: usize| size >= off + 4;
    if mask & 0x1 != 0 {
        *(p.add(8) as *mut u32) = band.style;
    }
    if mask & 0x2 != 0 {
        *(p.add(12) as *mut u32) = band.fore;
        *(p.add(16) as *mut u32) = band.back;
    }
    if mask & 0x4 != 0 {
        let buf = *(p.add(24) as *const usize);
        let cap = *(p.add(32) as *const u32) as i32;
        write_text(buf, cap, &band.text, wide);
    }
    if mask & 0x8 != 0 {
        *(p.add(36) as *mut i32) = band.image;
    }
    if mask & 0x10 != 0 {
        *(p.add(40) as *mut usize) = band.child;
    }
    if mask & 0x20 != 0 {
        *(p.add(48) as *mut u32) = band.cx_min;
        *(p.add(52) as *mut u32) = band.cy_min;
        if field(76) {
            *(p.add(76) as *mut u32) = band.cy_child;
            *(p.add(80) as *mut u32) = band.cy_max;
            *(p.add(84) as *mut u32) = band.cy_integral;
        }
    }
    if mask & 0x40 != 0 {
        *(p.add(56) as *mut u32) = band.cx;
    }
    if mask & 0x100 != 0 {
        *(p.add(72) as *mut u32) = band.id;
    }
    if mask & 0x200 != 0 && field(88) {
        *(p.add(88) as *mut u32) = band.cx_ideal;
    }
    if mask & 0x400 != 0 && size >= 104 {
        *(p.add(96) as *mut isize) = band.param;
    }
    if mask & 0x800 != 0 && field(104) {
        *(p.add(104) as *mut u32) = band.header;
    }
}

fn band_header(hwnd: usize, b: &Band) -> i32 {
    if b.header > 0 {
        return b.header as i32;
    }
    let mut w = if b.style & RBBS_NOGRIPPER != 0 { 2 } else { 8 };
    if !b.text.is_empty() && b.style & RBBS_HIDETITLE == 0 {
        w += measure(toolbar_font(hwnd), &b.text).0 + 8;
    }
    w
}

fn band_child_h(b: &Band) -> i32 {
    let h = if b.style & RBBS_VARIABLEHEIGHT != 0 && b.cy_child > 0 {
        b.cy_child.max(b.cy_min)
    } else {
        b.cy_min
    };
    if h == 0 && b.child != 0 {
        gui::window_rect(b.child).map_or(22, |r| r.height())
    } else {
        h as i32
    }
}

/// Rectangle de chaque bande (vide si cachée), en coordonnées client.
fn band_rects(hwnd: usize, s: &BarState) -> Vec<Rect> {
    let width = client(hwnd).width().max(1);
    let mut out = vec![Rect::default(); s.bands.len()];
    // Rangées : une nouvelle commence à RBBS_BREAK.
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (i, b) in s.bands.iter().enumerate() {
        if b.style & RBBS_HIDDEN != 0 {
            continue;
        }
        if rows.is_empty() || b.style & RBBS_BREAK != 0 {
            rows.push(Vec::new());
        }
        rows.last_mut().unwrap().push(i);
    }
    let mut y = 0;
    for row in rows {
        let h = row
            .iter()
            .map(|&i| band_child_h(&s.bands[i]))
            .max()
            .unwrap_or(0)
            + 4;
        let mut x = 0;
        for (k, &i) in row.iter().enumerate() {
            let b = &s.bands[i];
            let min = band_header(hwnd, b) + b.cx_min as i32 + 4;
            let w = if k + 1 == row.len() {
                (width - x).max(min.min(width - x).max(1))
            } else {
                (b.cx as i32).max(min)
            };
            out[i] = Rect::from_size(x, y, w, h);
            x += w;
        }
        y += h;
    }
    out
}

fn bar_height(hwnd: usize, s: &BarState) -> i32 {
    band_rects(hwnd, s)
        .iter()
        .map(|r| r.bottom)
        .max()
        .unwrap_or(0)
}

/// Place les enfants des bandes et ajuste la hauteur de la barre ; prévient
/// le parent (RBN_HEIGHTCHANGE) quand elle change.
fn rebar_layout(hwnd: usize) {
    let s = snap(hwnd);
    let rects = band_rects(hwnd, &s);
    for (b, r) in s.bands.iter().zip(&rects) {
        if b.child == 0 {
            continue;
        }
        if r.width() <= 0 {
            unsafe {
                user32::show_window(b.child, 0);
            }
            continue;
        }
        let ch = band_child_h(b).min(r.height() - 4).max(1);
        let x = r.left + band_header(hwnd, b);
        let y = r.top + (r.height() - ch) / 2;
        let w = (r.right - x - 2).max(1);
        unsafe {
            user32::set_window_pos(b.child, 0, x, y, w, ch, 0x14);
            if !gui::is_window_visible(b.child) {
                user32::show_window(b.child, 5);
            }
        }
    }
    let h = rects.iter().map(|r| r.bottom).max().unwrap_or(0);
    let st = style(hwnd);
    if st & CCS_NORESIZE == 0 {
        if let Some(cur) = gui::window_rect(hwnd) {
            let border = if st & WS_BORDER != 0 { 2 } else { 0 };
            let (x, y, w) = if st & CCS_NOPARENTALIGN != 0 {
                (cur.left, cur.top, cur.width())
            } else {
                let pr = gui::window_client_rect(parent(hwnd)).unwrap_or_default();
                let total = h + border;
                let y = if st & 0x3 == CCS_BOTTOM {
                    pr.height() - total
                } else {
                    0
                };
                (0, y, pr.width())
            };
            let total = (h + border).max(1);
            if Rect::from_size(x, y, w.max(1), total) != cur {
                unsafe {
                    user32::set_window_pos(hwnd, 0, x, y, w.max(1), total, 0x14);
                }
            }
        }
    }
    let changed = with(hwnd, |s| std::mem::replace(&mut s.last_height, h) != h);
    if changed {
        let mut hdr = NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        };
        notify_parent(hwnd, notify_target(hwnd), RBN_HEIGHTCHANGE, &mut hdr);
    }
    invalidate(hwnd);
}

fn paint_rebar(hwnd: usize, p: &Painter, r: Rect) {
    let s = snap(hwnd);
    let bk = if s.bk_color & 0xFF00_0000 == 0 {
        s.bk_color
    } else {
        face_color()
    };
    p.fill(r, bk);
    let rects = band_rects(hwnd, &s);
    let borders = style(hwnd) & RBS_BANDBORDERS != 0;
    for (b, br) in s.bands.iter().zip(&rects) {
        if br.width() <= 0 {
            continue;
        }
        if b.style & RBBS_NOGRIPPER == 0 {
            let x = br.left + 3;
            p.line(x, br.top + 5, x, br.bottom - 5, CONTROL_BORDER, 1);
        }
        if !b.text.is_empty() && b.style & RBBS_HIDETITLE == 0 {
            let left = br.left + if b.style & RBBS_NOGRIPPER == 0 { 8 } else { 2 };
            p.text(
                Rect {
                    left,
                    right: br.left + band_header(hwnd, b),
                    ..*br
                },
                &b.text,
                TEXT,
                0,
            );
        }
        if borders {
            if br.left > 0 {
                p.line(
                    br.left,
                    br.top + 2,
                    br.left,
                    br.bottom - 2,
                    CONTROL_BORDER,
                    1,
                );
            }
            if br.top > 0 {
                p.line(br.left, br.top, br.right, br.top, CONTROL_BORDER, 1);
            }
        }
    }
}

unsafe fn rebar_insert(hwnd: usize, wp: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let mut band = Band::default();
    read_band(lp as *const u8, &mut band, wide);
    if band.child != 0 {
        user32::set_parent(band.child, hwnd);
    }
    with(hwnd, |s| {
        let at = if wp as isize == -1 || wp > s.bands.len() {
            s.bands.len()
        } else {
            wp
        };
        s.bands.insert(at, band);
    });
    rebar_layout(hwnd);
    1
}

unsafe fn rebar_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let r = match msg {
        0x0401 | 0x040A => rebar_insert(hwnd, wp, lp, msg == 0x040A), // RB_INSERTBANDA/W
        0x0402 => {
            // RB_DELETEBAND
            let gone = with(hwnd, |s| (wp < s.bands.len()).then(|| s.bands.remove(wp)));
            match gone {
                Some(b) => {
                    if b.child != 0 {
                        user32::show_window(b.child, 0);
                    }
                    rebar_layout(hwnd);
                    1
                }
                None => 0,
            }
        }
        0x0403 | 0x0404 => 1, // RB_GETBARINFO / RB_SETBARINFO
        0x0406 | 0x040B => {
            // RB_SETBANDINFOA/W
            if lp == 0 {
                return Some(0);
            }
            let old_child = snap(hwnd).bands.get(wp).map(|b| b.child);
            let Some(old_child) = old_child else {
                return Some(0);
            };
            let mut band = snap(hwnd).bands[wp].clone();
            read_band(lp as *const u8, &mut band, msg == 0x040B);
            if band.child != 0 && band.child != old_child {
                user32::set_parent(band.child, hwnd);
            }
            with(hwnd, |s| s.bands[wp] = band);
            rebar_layout(hwnd);
            1
        }
        0x041C | 0x041D | 0x0405 => {
            // RB_GETBANDINFOW/A (0x0405 : ancien RB_GETBANDINFO)
            let s = snap(hwnd);
            match s.bands.get(wp) {
                Some(b) if lp != 0 => {
                    write_band(lp as *mut u8, b, msg == 0x041C);
                    1
                }
                _ => 0,
            }
        }
        0x0407 => with(hwnd, |s| std::mem::replace(&mut s.notify, wp)) as isize, // RB_SETPARENT
        0x0408 => {
            // RB_HITTEST : RBHITTESTINFO { POINT, flags, iBand }
            if lp == 0 {
                return Some(-1);
            }
            let p = lp as *mut i32;
            let (x, y) = (*p, *p.add(1));
            let s = snap(hwnd);
            let i = band_rects(hwnd, &s)
                .iter()
                .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom);
            *p.add(2) = if i.is_some() { 4 } else { 1 }; // RBHT_CLIENT / RBHT_NOWHERE
            *p.add(3) = i.map_or(-1, |i| i as i32);
            i.map_or(-1, |i| i as isize)
        }
        0x0409 => {
            // RB_GETRECT
            let s = snap(hwnd);
            match band_rects(hwnd, &s).get(wp) {
                Some(r) if lp != 0 => {
                    *(lp as *mut Rect) = *r;
                    1
                }
                _ => 0,
            }
        }
        0x040C => snap(hwnd).bands.len() as isize, // RB_GETBANDCOUNT
        0x040D => {
            // RB_GETROWCOUNT
            let s = snap(hwnd);
            let mut tops: Vec<i32> = band_rects(hwnd, &s)
                .iter()
                .filter(|r| r.width() > 0)
                .map(|r| r.top)
                .collect();
            tops.dedup();
            tops.len() as isize
        }
        0x040E => {
            let s = snap(hwnd);
            band_rects(hwnd, &s)
                .get(wp)
                .map_or(0, |r| r.height() as isize)
        }
        0x0410 => snap(hwnd)
            .bands
            .iter()
            .position(|b| b.id as usize == wp)
            .map_or(-1, |i| i as isize),
        0x0411 => snap(hwnd).tooltips as isize,
        0x0412 => {
            with(hwnd, |s| s.tooltips = wp);
            0
        }
        0x0413 => {
            let old = with(hwnd, |s| std::mem::replace(&mut s.bk_color, lp as u32));
            invalidate(hwnd);
            old as isize
        }
        0x0414 => snap(hwnd).bk_color as isize,
        0x0415 => with(hwnd, |s| std::mem::replace(&mut s.text_color, lp as u32)) as isize,
        0x0416 => snap(hwnd).text_color as isize,
        0x0417 => {
            // RB_SIZETORECT
            if lp != 0 {
                let r = *(lp as *const Rect);
                user32::set_window_pos(
                    hwnd,
                    0,
                    r.left,
                    r.top,
                    r.width().max(1),
                    r.height().max(1),
                    0x14,
                );
            }
            rebar_layout(hwnd);
            1
        }
        0x0418..=0x041A => 0,                             // glisser une bande
        0x041B => bar_height(hwnd, &snap(hwnd)) as isize, // RB_GETBARHEIGHT
        0x041E | 0x041F => {
            rebar_layout(hwnd);
            0
        }
        0x0422 => {
            // RB_GETBANDBORDERS
            let s = snap(hwnd);
            match s.bands.get(wp) {
                Some(b) if lp != 0 => {
                    *(lp as *mut Rect) = Rect {
                        left: band_header(hwnd, b),
                        top: 2,
                        right: 2,
                        bottom: 2,
                    };
                    1
                }
                _ => 0,
            }
        }
        0x0423 => {
            // RB_SHOWBAND
            let ok = with(hwnd, |s| match s.bands.get_mut(wp) {
                Some(b) => {
                    if lp != 0 {
                        b.style &= !RBBS_HIDDEN;
                    } else {
                        b.style |= RBBS_HIDDEN;
                    }
                    true
                }
                None => false,
            });
            rebar_layout(hwnd);
            ok as isize
        }
        0x0425 | 0x0426 => 0, // palette
        0x0427 => {
            // RB_MOVEBAND
            let ok = with(hwnd, |s| {
                let to = lp as usize;
                if wp < s.bands.len() && to < s.bands.len() {
                    let b = s.bands.remove(wp);
                    s.bands.insert(to, b);
                    true
                } else {
                    false
                }
            });
            rebar_layout(hwnd);
            ok as isize
        }
        0x0428 => {
            if lp != 0 {
                *(lp as *mut [i32; 4]) = [2, 0, 2, 0];
            }
            0
        }
        0x0429..=0x042B => 0,
        0x042C => {
            // RB_SETBANDWIDTH
            let ok = with(hwnd, |s| match s.bands.get_mut(wp) {
                Some(b) => {
                    b.cx = lp as u32;
                    true
                }
                None => false,
            });
            rebar_layout(hwnd);
            ok as isize
        }
        WM_SIZE => {
            // Enfants replacés ; la hauteur suit les bandes.
            let s = snap(hwnd);
            let rects = band_rects(hwnd, &s);
            for (b, r) in s.bands.iter().zip(&rects) {
                if b.child != 0 && r.width() > 0 {
                    let ch = band_child_h(b).min(r.height() - 4).max(1);
                    let x = r.left + band_header(hwnd, b);
                    user32::set_window_pos(
                        b.child,
                        0,
                        x,
                        r.top + (r.height() - ch) / 2,
                        (r.right - x - 2).max(1),
                        ch,
                        0x14,
                    );
                }
            }
            invalidate(hwnd);
            0
        }
        // Les enfants des bandes parlent au parent de la barre.
        WM_COMMAND | WM_NOTIFY => send(notify_target(hwnd), msg, wp, lp),
        WM_GETDLGCODE => 0,
        _ if (0x0400..0x0500).contains(&msg) => 0,
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// ComboBoxEx32
// =====================================================================

const CBEIF_TEXT: u32 = 0x1;
const CBEIF_IMAGE: u32 = 0x2;
const CBEIF_SELECTEDIMAGE: u32 = 0x4;
const CBEIF_OVERLAY: u32 = 0x8;
const CBEIF_INDENT: u32 = 0x10;
const CBEIF_LPARAM: u32 = 0x20;
const CBEN_ENDEDITA: i32 = -805;
const CBEN_ENDEDITW: i32 = -806;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CB_INSERTSTRING: u32 = 0x014A;
const CB_DELETESTRING: u32 = 0x0144;
const CB_GETCURSEL: u32 = 0x0147;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETITEMDATA: u32 = 0x0150;
const CB_SETITEMDATA: u32 = 0x0151;

/// COMBOBOXEXITEMA/W (64 bits).
#[repr(C)]
#[derive(Clone, Copy)]
struct CbeItem {
    mask: u32,
    item: isize,
    text: usize,
    text_max: i32,
    image: i32,
    selected_image: i32,
    overlay: i32,
    indent: i32,
    param: isize,
}

unsafe fn cbex_create(hwnd: usize, wide: bool) {
    let r = client(hwnd);
    // La liste garde le type (CBS_SIMPLE/DROPDOWN/DROPDOWNLIST) et le tri.
    let combo_style =
        0x4000_0000 | 0x1000_0000 | 0x0020_0000 | 0x0001_0000 | 0x40 | (style(hwnd) & 0x0000_0303);
    let class: Vec<u16> = "ComboBox".encode_utf16().chain([0]).collect();
    let empty = [0u16];
    let id = user32::control_id(hwnd) as usize;
    let combo = with_charset(wide, || {
        user32::create_window_ex_w(
            0,
            class.as_ptr(),
            empty.as_ptr(),
            combo_style,
            0,
            0,
            r.width().max(1),
            r.height().max(1),
            hwnd,
            id,
            0,
            std::ptr::null_mut(),
        )
    });
    with(hwnd, |s| s.combo = combo);
    // Comme la liste fermée, la ComboBoxEx prend la hauteur du champ.
    if let (Some(cr), Some(me)) = (gui::window_rect(combo), gui::window_rect(hwnd)) {
        if cr.height() > 0 && cr.height() != me.height() {
            gui::set_window_rect(
                hwnd,
                Rect {
                    bottom: me.top + cr.height(),
                    ..me
                },
            );
        }
    }
}

/// Envoie un message à la liste dans le jeu de caractères donné.
fn combo_send(combo: usize, wide: bool, msg: u32, wp: usize, lp: isize) -> isize {
    with_charset(wide, || send(combo, msg, wp, lp))
}

fn combo_text(combo: usize, index: isize) -> String {
    let n = combo_send(combo, true, CB_GETLBTEXTLEN, index as usize, 0);
    if n < 0 {
        return String::new();
    }
    let mut buf = vec![0u16; n as usize + 1];
    combo_send(
        combo,
        true,
        CB_GETLBTEXT,
        index as usize,
        buf.as_mut_ptr() as isize,
    );
    String::from_utf16_lossy(&buf[..n as usize])
}

fn combo_edit_text(combo: usize) -> String {
    let mut buf = vec![0u16; 1024];
    let n = combo_send(combo, true, WM_GETTEXT, 1024, buf.as_mut_ptr() as isize);
    String::from_utf16_lossy(&buf[..n.clamp(0, 1023) as usize])
}

unsafe fn cbex_insert(hwnd: usize, combo: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return -1;
    }
    let it = std::ptr::read_unaligned(lp as *const CbeItem);
    let text = if it.mask & CBEIF_TEXT != 0 {
        read_text(it.text, wide).unwrap_or_default()
    } else {
        String::new()
    };
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let idx = combo_send(
        combo,
        true,
        CB_INSERTSTRING,
        it.item as usize,
        w.as_ptr() as isize,
    );
    if idx >= 0 && it.mask & CBEIF_LPARAM != 0 {
        combo_send(combo, true, CB_SETITEMDATA, idx as usize, it.param);
    }
    let _ = (
        CBEIF_IMAGE,
        CBEIF_SELECTEDIMAGE,
        CBEIF_OVERLAY,
        CBEIF_INDENT,
        hwnd,
    );
    idx
}

unsafe fn cbex_set(combo: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let it = std::ptr::read_unaligned(lp as *const CbeItem);
    if it.item == -1 {
        if it.mask & CBEIF_TEXT != 0 {
            let text = read_text(it.text, wide).unwrap_or_default();
            let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
            combo_send(combo, true, WM_SETTEXT, 0, w.as_ptr() as isize);
        }
        return 1;
    }
    let i = it.item as usize;
    if it.mask & CBEIF_TEXT != 0 {
        let text = read_text(it.text, wide).unwrap_or_default();
        let data = combo_send(combo, true, CB_GETITEMDATA, i, 0);
        let sel = combo_send(combo, true, CB_GETCURSEL, 0, 0);
        if combo_send(combo, true, CB_DELETESTRING, i, 0) < 0 {
            return 0;
        }
        let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
        combo_send(combo, true, CB_INSERTSTRING, i, w.as_ptr() as isize);
        combo_send(combo, true, CB_SETITEMDATA, i, data);
        if sel == i as isize {
            combo_send(combo, true, CB_SETCURSEL, i, 0);
        }
    }
    if it.mask & CBEIF_LPARAM != 0 {
        combo_send(combo, true, CB_SETITEMDATA, i, it.param);
    }
    1
}

unsafe fn cbex_get(combo: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let p = lp as *mut CbeItem;
    let it = std::ptr::read_unaligned(p);
    let index = if it.item == -1 {
        combo_send(combo, true, CB_GETCURSEL, 0, 0)
    } else {
        it.item
    };
    if it.mask & CBEIF_TEXT != 0 {
        let text = if it.item == -1 {
            combo_edit_text(combo)
        } else {
            combo_text(combo, index)
        };
        write_text(it.text, it.text_max, &text, wide);
    }
    if it.mask & CBEIF_LPARAM != 0 {
        let data = if index >= 0 {
            combo_send(combo, true, CB_GETITEMDATA, index as usize, 0)
        } else {
            0
        };
        (*p).param = data;
    }
    if it.mask & (CBEIF_IMAGE | CBEIF_SELECTEDIMAGE | CBEIF_OVERLAY) != 0 {
        (*p).image = 0;
        (*p).selected_image = 0;
        (*p).overlay = 0;
    }
    if it.mask & CBEIF_INDENT != 0 {
        (*p).indent = 0;
    }
    (it.item == -1 || index >= 0) as isize
}

/// Entrée ou Échap dans le champ d'une ComboBoxEx : CBEN_ENDEDIT au parent.
/// Renvoie vrai si la touche est prise.
pub(super) fn combo_end_edit(cbex: usize, vk: usize) -> bool {
    let why = match vk {
        VK_RETURN => 2, // CBENF_RETURN
        VK_ESCAPE => 3, // CBENF_ESCAPE
        _ => return false,
    };
    let s = snap(cbex);
    if s.combo == 0 {
        return false;
    }
    let text = combo_edit_text(s.combo);
    let sel = combo_send(s.combo, true, CB_GETCURSEL, 0, 0) as i32;
    let target = parent(cbex);
    if s.unicode {
        #[repr(C)]
        struct EndEditW {
            hdr: NmHdr,
            changed: i32,
            new_sel: i32,
            text: [u16; 260],
            why: i32,
        }
        let mut nm = EndEditW {
            hdr: NmHdr {
                hwnd_from: 0,
                id_from: 0,
                code: 0,
            },
            changed: 1,
            new_sel: sel,
            text: [0; 260],
            why,
        };
        unsafe {
            crate::dlls::put_text_w(&text, nm.text.as_mut_ptr(), 260);
        }
        notify_parent(cbex, target, CBEN_ENDEDITW, &mut nm);
        true
    } else {
        #[repr(C)]
        struct EndEditA {
            hdr: NmHdr,
            changed: i32,
            new_sel: i32,
            text: [u8; 260],
            why: i32,
        }
        let mut nm = EndEditA {
            hdr: NmHdr {
                hwnd_from: 0,
                id_from: 0,
                code: 0,
            },
            changed: 1,
            new_sel: sel,
            text: [0; 260],
            why,
        };
        unsafe {
            crate::dlls::put_text_a(&text, nm.text.as_mut_ptr(), 260);
        }
        notify_parent(cbex, target, CBEN_ENDEDITA, &mut nm);
        true
    }
}

unsafe fn cbex_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let s = snap(hwnd);
    let combo = s.combo;
    let r = match msg {
        0x0401 | 0x040B => cbex_insert(hwnd, combo, lp, msg == 0x040B), // CBEM_INSERTITEMA/W
        0x0402 => with(hwnd, |s| std::mem::replace(&mut s.image_list, lp as usize)) as isize,
        0x0403 => s.image_list as isize,
        0x0404 | 0x040D => cbex_get(combo, lp, msg == 0x040D), // CBEM_GETITEMA/W
        0x0405 | 0x040C => cbex_set(combo, lp, msg == 0x040C), // CBEM_SETITEMA/W
        0x0406 => combo as isize,                              // CBEM_GETCOMBOCONTROL
        0x0407 => {
            // CBEM_GETEDITCONTROL : le champ fait partie de la liste Winlux.
            if style(hwnd) & 0x3 == 0x3 {
                0
            } else {
                combo as isize
            }
        }
        0x0408 | 0x040E => {
            // CBEM_SETEXSTYLE / CBEM_SETEXTENDEDSTYLE (masque, style)
            with(hwnd, |s| {
                let old = s.ex_style;
                let mask = if msg == 0x040E && wp != 0 {
                    wp as u32
                } else {
                    u32::MAX
                };
                s.ex_style = (old & !mask) | (lp as u32 & mask);
                old as isize
            })
        }
        0x0409 => s.ex_style as isize,
        0x040A => 0, // CBEM_HASEDITCHANGED
        0x200B => 0, // CBEM_SETWINDOWTHEME
        // Messages CB_* : la liste déroulante répond.
        0x0140..=0x0165 => send(combo, msg, wp, lp),
        WM_COMMAND if lp as usize == combo => {
            // Notification de la liste : relayée au parent avec notre identifiant.
            let id = user32::control_id(hwnd) as usize & 0xFFFF;
            send(
                parent(hwnd),
                WM_COMMAND,
                (wp & 0xFFFF_0000) | id,
                hwnd as isize,
            )
        }
        WM_COMMAND | WM_NOTIFY => send(parent(hwnd), msg, wp, lp),
        WM_SIZE => {
            let r = client(hwnd);
            if combo != 0 {
                if let Some(cr) = gui::window_rect(combo) {
                    user32::set_window_pos(
                        combo,
                        0,
                        0,
                        0,
                        r.width().max(1),
                        cr.height().max(1),
                        0x14,
                    );
                }
            }
            0
        }
        WM_GETDLGCODE => DLGC_WANTARROWS | DLGC_WANTCHARS,
        _ => return None,
    };
    Some(r)
}
