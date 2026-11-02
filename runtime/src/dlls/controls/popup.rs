//! Menus surgissants : `TrackPopupMenu(Ex)` affiche un vrai menu (classe
//! `#32768`), avec sous-menus, coches, éléments grisés, raccourcis clavier
//! et lettres soulignées, dans le style des menus de Windows 11.
//!
//! La boucle est modale, comme sous Windows : les entrées souris et clavier
//! vont au menu, les autres messages (peinture, minuteurs, réseau) sont
//! distribués normalement. Un clic hors du menu ou Échap l'annule.
//!
//! Sans affichage : `WINLUX_POPUPMENU=<identifiant>` choisit cette commande
//! (si elle existe et n'est pas grisée) ; sans variable, le menu est annulé.

use super::*;
use crate::dlls::menus::{self, Item};
use std::cell::RefCell;

const MF_GRAYED: u32 = 0x1;
const MF_DISABLED: u32 = 0x2;
const MF_BITMAP: u32 = 0x4;
const MF_CHECKED: u32 = 0x8;
const MF_POPUP: u32 = 0x10;
const MF_OWNERDRAW: u32 = 0x100;
const MF_RADIOCHECK: u32 = 0x200;
const MF_SEPARATOR: u32 = 0x800;

const TPM_CENTERALIGN: u32 = 0x4;
const TPM_RIGHTALIGN: u32 = 0x8;
const TPM_VCENTERALIGN: u32 = 0x10;
const TPM_BOTTOMALIGN: u32 = 0x20;
const TPM_NONOTIFY: u32 = 0x80;
const TPM_RETURNCMD: u32 = 0x100;

const WM_COMMAND: u32 = 0x0111;
const WM_SYSCOMMAND: u32 = 0x0112;
const WM_INITMENUPOPUP: u32 = 0x0117;
const WM_ENTERMENULOOP: u32 = 0x0211;
const WM_EXITMENULOOP: u32 = 0x0212;

const ITEM_H: i32 = 28;
const SEP_H: i32 = 9;
const PAD: i32 = 4;
const CHECK_W: i32 = 32;
const ARROW_W: i32 = 24;
const MENU_BG: u32 = 0x00F9_F9F9;

pub(crate) const CLASS: &str = "#32768";

struct Level {
    hwnd: usize,
    menu: usize,
    items: Vec<Item>,
    /// (haut, bas) de chaque élément, en coordonnées client.
    rows: Vec<(i32, i32)>,
    /// Position de l'écran du coin haut gauche.
    origin: (i32, i32),
    width: i32,
    sel: i32,
}

thread_local! {
    static STACK: RefCell<Vec<Level>> = const { RefCell::new(Vec::new()) };
}

fn font() -> usize {
    gdi32::get_stock_object(17) // DEFAULT_GUI_FONT
}

fn enabled(it: &Item) -> bool {
    it.flags & (MF_GRAYED | MF_DISABLED) == 0
}

fn selectable(it: &Item) -> bool {
    it.flags & MF_SEPARATOR == 0
}

/// « &Copier\tCtrl+C » → (« Copier », « Ctrl+C »).
fn split_label(text: &str) -> (String, String) {
    let mut parts = text.splitn(2, '\t');
    let label = strip_prefix(parts.next().unwrap_or(""));
    let accel = parts.next().unwrap_or("").to_string();
    (label, accel)
}

/// Lettre soulignée (« &Fermer » → 'f').
fn mnemonic(text: &str) -> Option<char> {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => continue,
                Some(m) => return m.to_lowercase().next(),
                None => return None,
            }
        }
    }
    None
}

fn layout(items: &[Item]) -> (i32, i32, Vec<(i32, i32)>) {
    let f = font();
    let mut y = PAD;
    let mut rows = Vec::with_capacity(items.len());
    let mut label_w = 0;
    let mut accel_w = 0;
    for it in items {
        let h = if it.flags & MF_SEPARATOR != 0 {
            SEP_H
        } else {
            ITEM_H
        };
        rows.push((y, y + h));
        y += h;
        if it.flags & (MF_SEPARATOR | MF_BITMAP | MF_OWNERDRAW) == 0 {
            let (label, accel) = split_label(&it.text);
            label_w = label_w.max(measure(f, &label).0);
            if !accel.is_empty() {
                accel_w = accel_w.max(measure(f, &accel).0 + 32);
            }
        }
    }
    let w = (CHECK_W + label_w + accel_w + ARROW_W + 2 * PAD).max(160);
    (w, y + PAD, rows)
}

/// Le menu tient à l'écran : il bascule à gauche ou en haut s'il dépasse.
fn place(x: i32, y: i32, w: i32, h: i32, flags: u32, flip_x: Option<i32>) -> (i32, i32) {
    let area = gui::work_area();
    let mut px = if flags & TPM_RIGHTALIGN != 0 {
        x - w
    } else if flags & TPM_CENTERALIGN != 0 {
        x - w / 2
    } else {
        x
    };
    let mut py = if flags & TPM_BOTTOMALIGN != 0 {
        y - h
    } else if flags & TPM_VCENTERALIGN != 0 {
        y - h / 2
    } else {
        y
    };
    if px + w > area.right {
        px = flip_x.map_or(area.right - w, |fx| fx - w);
    }
    if py + h > area.bottom {
        py = (area.bottom - h).max(area.top);
    }
    (px.max(area.left), py.max(area.top))
}

unsafe fn open_level(menu: usize, x: i32, y: i32, flags: u32, flip_x: Option<i32>, owner: usize) {
    let index = STACK.with(|s| s.borrow().len());
    if flags & TPM_NONOTIFY == 0 && owner != 0 {
        user32::send_message_w(owner, WM_INITMENUPOPUP, menu, index as isize);
    }
    let items = menus::items(menu).unwrap_or_default();
    let (w, h, rows) = layout(&items);
    let (px, py) = place(x, y, w, h, flags, flip_x);
    let class: Vec<u16> = CLASS.encode_utf16().chain([0]).collect();
    let hwnd = user32::create_window_ex_w(
        0x0000_0008 | 0x0000_0080, // WS_EX_TOPMOST | WS_EX_TOOLWINDOW
        class.as_ptr(),
        [0u16].as_ptr(),
        0x8000_0000, // WS_POPUP
        px,
        py,
        w,
        h,
        owner,
        0,
        0,
        std::ptr::null_mut(),
    );
    if hwnd == 0 {
        return;
    }
    STACK.with(|s| {
        s.borrow_mut().push(Level {
            hwnd,
            menu,
            items,
            rows,
            origin: (px, py),
            width: w,
            sel: -1,
        })
    });
    user32::show_window(hwnd, 8); // SW_SHOWNA
    user32::update_window(hwnd);
}

unsafe fn close_from(depth: usize) {
    let closed: Vec<usize> = STACK.with(|s| {
        let mut s = s.borrow_mut();
        let n = s.len();
        if depth >= n {
            return Vec::new();
        }
        s.drain(depth..).map(|l| l.hwnd).collect()
    });
    for h in closed.into_iter().rev() {
        user32::destroy_window(h);
    }
}

fn depth() -> usize {
    STACK.with(|s| s.borrow().len())
}

/// Niveau et élément sous un point de l'écran (le plus haut niveau d'abord).
fn hit(sx: i32, sy: i32) -> Option<(usize, i32)> {
    STACK.with(|s| {
        let s = s.borrow();
        for (d, l) in s.iter().enumerate().rev() {
            let (x, y) = (sx - l.origin.0, sy - l.origin.1);
            let h = l.rows.last().map_or(0, |r| r.1) + PAD;
            if x < 0 || y < 0 || x >= l.width || y >= h {
                continue;
            }
            let item = l
                .rows
                .iter()
                .position(|&(t, b)| y >= t && y < b)
                .map_or(-1, |i| i as i32);
            return Some((d, item));
        }
        None
    })
}

fn set_sel(d: usize, item: i32) {
    let changed = STACK.with(|s| {
        let mut s = s.borrow_mut();
        let l = s.get_mut(d)?;
        if l.sel == item {
            return None;
        }
        l.sel = item;
        Some(l.hwnd)
    });
    if let Some(h) = changed {
        unsafe {
            user32::invalidate_rect(h, std::ptr::null(), 0);
            user32::update_window(h);
        }
    }
}

fn item_at(d: usize, i: i32) -> Option<(Item, (i32, i32), (i32, i32), i32)> {
    STACK.with(|s| {
        let s = s.borrow();
        let l = s.get(d)?;
        let it = l.items.get(usize::try_from(i).ok()?)?.clone();
        Some((it, l.rows[i as usize], l.origin, l.width))
    })
}

/// Ouvre le sous-menu de l'élément choisi du niveau `d`, à sa droite.
unsafe fn open_submenu(d: usize, owner: usize, flags: u32) {
    let sel = STACK.with(|s| s.borrow().get(d).map_or(-1, |l| l.sel));
    let Some((it, (top, _), origin, width)) = item_at(d, sel) else {
        return;
    };
    if it.flags & MF_POPUP == 0 || it.submenu == 0 || !enabled(&it) {
        return;
    }
    let already = STACK.with(|s| s.borrow().get(d + 1).map(|l| l.menu));
    if already == Some(it.submenu) {
        return;
    }
    close_from(d + 1);
    open_level(
        it.submenu,
        origin.0 + width - 4,
        origin.1 + top - PAD,
        flags & TPM_NONOTIFY,
        Some(origin.0 + 4),
        owner,
    );
}

fn step(d: usize, delta: i32) {
    let next = STACK.with(|s| {
        let s = s.borrow();
        let l = s.get(d)?;
        let n = l.items.len() as i32;
        if n == 0 {
            return None;
        }
        let mut i = l.sel;
        for _ in 0..n {
            i = if i < 0 && delta < 0 {
                n - 1
            } else {
                (i + delta).rem_euclid(n)
            };
            if selectable(&l.items[i as usize]) {
                return Some(i);
            }
        }
        None
    });
    if let Some(i) = next {
        set_sel(d, i);
    }
}

enum Outcome {
    Continue,
    Cancel,
    Pick(u32),
}

/// Valide l'élément `i` du niveau `d` : commande, ou ouverture du sous-menu.
unsafe fn activate(d: usize, i: i32, owner: usize, flags: u32) -> Outcome {
    let Some((it, ..)) = item_at(d, i) else {
        return Outcome::Continue;
    };
    if !selectable(&it) || !enabled(&it) {
        return Outcome::Continue;
    }
    if it.flags & MF_POPUP != 0 {
        set_sel(d, i);
        open_submenu(d, owner, flags);
        step(d + 1, 1);
        return Outcome::Continue;
    }
    Outcome::Pick(it.id)
}

fn screen_point(m: &gui::Msg) -> Option<(i32, i32)> {
    let (x, y) = point(m.lparam);
    STACK.with(|s| {
        s.borrow()
            .iter()
            .find(|l| l.hwnd == m.hwnd)
            .map(|l| (l.origin.0 + x, l.origin.1 + y))
    })
}

unsafe fn handle(m: &gui::Msg, owner: usize, flags: u32, armed: &mut bool) -> Outcome {
    match m.message {
        // Souris : seuls les messages du menu ont des coordonnées exploitables ;
        // un clic dans une autre fenêtre ferme le menu.
        0x0200 => {
            if let Some((sx, sy)) = screen_point(m) {
                if let Some((d, i)) = hit(sx, sy) {
                    *armed = true;
                    let ok = item_at(d, i).is_some_and(|(it, ..)| selectable(&it));
                    set_sel(d, if ok { i } else { -1 });
                    if ok {
                        close_from(d + 2);
                        open_submenu(d, owner, flags);
                        if item_at(d, i).is_some_and(|(it, ..)| it.flags & MF_POPUP == 0) {
                            close_from(d + 1);
                        }
                    }
                }
            }
            Outcome::Continue
        }
        0x0201 | 0x0204 | 0x0207 => match screen_point(m).and_then(|(x, y)| hit(x, y)) {
            Some(_) => {
                *armed = true;
                Outcome::Continue
            }
            None => Outcome::Cancel,
        },
        0x0202 | 0x0205 => {
            if !*armed {
                // Le relâchement du clic qui a ouvert le menu.
                return Outcome::Continue;
            }
            match screen_point(m).and_then(|(x, y)| hit(x, y)) {
                Some((d, i)) => activate(d, i, owner, flags),
                None => Outcome::Continue,
            }
        }
        0x0203 | 0x0206 | 0x0208 | 0x0209 | 0x020A..=0x020E => Outcome::Continue,
        // Clavier, quelle que soit la fenêtre qui a le focus.
        0x0100 | 0x0104 => {
            let d = depth().saturating_sub(1);
            match m.wparam as u32 {
                0x1B => {
                    // Échap : referme le dernier niveau.
                    if d == 0 {
                        return Outcome::Cancel;
                    }
                    close_from(d);
                }
                0x26 => step(d, -1),
                0x28 => step(d, 1),
                0x27 => {
                    let sel = STACK.with(|s| s.borrow().get(d).map_or(-1, |l| l.sel));
                    if item_at(d, sel).is_some_and(|(it, ..)| it.flags & MF_POPUP != 0) {
                        open_submenu(d, owner, flags);
                        step(d + 1, 1);
                    }
                }
                0x25 => {
                    if d > 0 {
                        close_from(d);
                    }
                }
                0x0D | 0x20 => {
                    let sel = STACK.with(|s| s.borrow().get(d).map_or(-1, |l| l.sel));
                    return activate(d, sel, owner, flags);
                }
                0x12 => return Outcome::Cancel, // Alt
                _ => {}
            }
            Outcome::Continue
        }
        0x0102 | 0x0106 => {
            let Some(ch) = char::from_u32(m.wparam as u32).and_then(|c| c.to_lowercase().next())
            else {
                return Outcome::Continue;
            };
            let d = depth().saturating_sub(1);
            let found = STACK.with(|s| {
                s.borrow().get(d).and_then(|l| {
                    l.items
                        .iter()
                        .position(|it| mnemonic(&it.text) == Some(ch))
                        .map(|i| i as i32)
                })
            });
            match found {
                Some(i) => activate(d, i, owner, flags),
                None => Outcome::Continue,
            }
        }
        0x0101 | 0x0103 | 0x0105 | 0x0107..=0x0109 => Outcome::Continue,
        _ => {
            user32::translate_message(m);
            user32::dispatch_message_w(m);
            Outcome::Continue
        }
    }
}

fn headless_pick(menu: usize) -> u32 {
    let Some(want) = std::env::var("WINLUX_POPUPMENU")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
    else {
        return 0;
    };
    fn find(menu: usize, want: u32) -> bool {
        menus::items(menu).unwrap_or_default().iter().any(|it| {
            if it.flags & MF_POPUP != 0 {
                enabled(it) && find(it.submenu, want)
            } else {
                it.id == want && enabled(it) && selectable(it)
            }
        })
    }
    if find(menu, want) {
        want
    } else {
        0
    }
}

/// `TrackPopupMenu(Ex)` : renvoie l'identifiant choisi (0 si annulé).
pub unsafe fn track(menu: usize, flags: u32, x: i32, y: i32, owner: usize) -> u32 {
    if menus::items(menu).is_none() {
        crate::process::set_last_error(1401); // ERROR_INVALID_MENU_HANDLE
        return 0;
    }
    if depth() > 0 {
        return 0; // un seul menu à la fois, comme sous Windows
    }
    let notify = flags & TPM_NONOTIFY == 0 && owner != 0;
    if notify {
        user32::send_message_w(owner, WM_ENTERMENULOOP, 1, 0);
    }
    let picked = if !gui::backend_is_interactive() {
        if notify {
            user32::send_message_w(owner, WM_INITMENUPOPUP, menu, 0);
        }
        headless_pick(menu)
    } else {
        open_level(menu, x, y, flags, None, owner);
        let first = STACK.with(|s| s.borrow().first().map_or(0, |l| l.hwnd));
        gui::with_backend_public(|b| b.grab_pointer(first, true));
        let mut armed = false;
        let mut picked = 0;
        while depth() > 0 {
            let mut m: gui::Msg = std::mem::zeroed();
            let r = user32::get_message_w(&mut m, 0, 0, 0);
            if r <= 0 {
                // WM_QUIT : il reste dans la file pour la boucle du programme.
                break;
            }
            match handle(&m, owner, flags, &mut armed) {
                Outcome::Continue => {}
                Outcome::Cancel => break,
                Outcome::Pick(id) => {
                    picked = id;
                    break;
                }
            }
        }
        gui::with_backend_public(|b| b.grab_pointer(first, false));
        close_from(0);
        picked
    };
    if notify {
        user32::send_message_w(owner, WM_EXITMENULOOP, 1, 0);
    }
    if picked == 0 {
        return 0;
    }
    if flags & TPM_RETURNCMD != 0 {
        return picked;
    }
    if flags & TPM_NONOTIFY == 0 && owner != 0 {
        let msg = if menus::is_system_menu(menu) {
            WM_SYSCOMMAND
        } else {
            WM_COMMAND
        };
        user32::post_message_w(owner, msg, picked as usize, 0);
    }
    1
}

fn paint_level(hwnd: usize) {
    let Some((items, rows, sel, width)) = STACK.with(|s| {
        s.borrow()
            .iter()
            .find(|l| l.hwnd == hwnd)
            .map(|l| (l.items.clone(), l.rows.clone(), l.sel, l.width))
    }) else {
        return;
    };
    let mut ps: PaintStruct = unsafe { std::mem::zeroed() };
    let hdc = unsafe { user32::begin_paint(hwnd, &mut ps) };
    if hdc == 0 {
        return;
    }
    let p = Painter { hdc, font: font() };
    let r = client(hwnd);
    p.fill(r, MENU_BG);
    p.frame(r, CONTROL_BORDER);
    for (i, (it, &(top, bottom))) in items.iter().zip(rows.iter()).enumerate() {
        if it.flags & MF_SEPARATOR != 0 {
            let y = top + SEP_H / 2;
            p.line(PAD + 8, y, width - PAD - 8, y, CONTROL_BORDER, 1);
            continue;
        }
        let on = enabled(it);
        if i as i32 == sel {
            let hr = Rect {
                left: PAD,
                top: top + 1,
                right: width - PAD,
                bottom: bottom - 1,
            };
            p.solid(hr, SELECTION_SOFT, 4, false);
        }
        let color = if on { TEXT } else { TEXT_DISABLED };
        let cy = (top + bottom) / 2;
        if it.flags & MF_CHECKED != 0 {
            let cx = PAD + CHECK_W / 2;
            if it.flags & MF_RADIOCHECK != 0 {
                p.solid(Rect::from_size(cx - 3, cy - 3, 7, 7), color, 0, true);
            } else {
                p.line(cx - 5, cy, cx - 2, cy + 3, color, 2);
                p.line(cx - 2, cy + 3, cx + 5, cy - 4, color, 2);
            }
        }
        let (label, accel) = split_label(&it.text);
        let text_r = Rect {
            left: PAD + CHECK_W,
            top,
            right: width - PAD - ARROW_W,
            bottom,
        };
        p.text(text_r, &label, color, 0);
        if !accel.is_empty() {
            p.text(text_r, &accel, TEXT_DISABLED, 2);
        }
        if it.flags & MF_POPUP != 0 {
            let ax = width - PAD - ARROW_W / 2;
            p.line(ax - 2, cy - 4, ax + 2, cy, color, 1);
            p.line(ax + 2, cy, ax - 2, cy + 4, color, 1);
        }
    }
    unsafe {
        user32::end_paint(hwnd, &ps);
    }
}

/// Procédure de la classe `#32768`.
pub(super) unsafe extern "win64" fn popup_proc(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    match msg {
        WM_PAINT => {
            paint_level(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        0x0021 => 3, // WM_MOUSEACTIVATE : MA_NOACTIVATE
        _ => gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libelles_raccourcis_et_lettres() {
        assert_eq!(
            split_label("&Copier\tCtrl+C"),
            ("Copier".to_string(), "Ctrl+C".to_string())
        );
        assert_eq!(mnemonic("&Fermer"), Some('f'));
        assert_eq!(mnemonic("Nou&veau"), Some('v'));
        assert_eq!(mnemonic("A && B"), None);
    }

    #[test]
    fn disposition_separateurs_et_placement() {
        let items = vec![
            Item {
                id: 1,
                text: "&Copier".into(),
                ..Default::default()
            },
            Item {
                flags: MF_SEPARATOR,
                ..Default::default()
            },
            Item {
                id: 2,
                text: "Coller".into(),
                ..Default::default()
            },
        ];
        let (w, h, rows) = layout(&items);
        assert!(w >= 160);
        assert_eq!(rows, vec![(4, 32), (32, 41), (41, 69)]);
        assert_eq!(h, 73);
        let area = gui::work_area();
        let (x, y) = place(area.right - 10, area.bottom - 10, w, h, 0, None);
        assert!(x + w <= area.right && y + h <= area.bottom);
    }

    #[test]
    fn sans_affichage_la_variable_choisit_la_commande() {
        let m = menus::create();
        menus::append(m, 0, 41, Some("&Coller".into()));
        menus::append(m, MF_GRAYED, 42, Some("Grisé".into()));
        let sub = menus::create();
        menus::append(sub, 0, 43, Some("Profond".into()));
        menus::append(m, MF_POPUP, sub, Some("Sous-menu".into()));
        unsafe {
            std::env::set_var("WINLUX_POPUPMENU", "43");
            assert_eq!(track(m, TPM_RETURNCMD, 10, 10, 0), 43);
            std::env::set_var("WINLUX_POPUPMENU", "42");
            assert_eq!(track(m, TPM_RETURNCMD, 10, 10, 0), 0);
            std::env::remove_var("WINLUX_POPUPMENU");
            assert_eq!(track(m, TPM_RETURNCMD, 10, 10, 0), 0);
            assert_eq!(track(0xDEAD, TPM_RETURNCMD, 0, 0, 0), 0);
        }
    }
}
