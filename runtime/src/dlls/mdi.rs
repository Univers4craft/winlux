//! Interface multidocument (MDI) : classe système `MDICLIENT`,
//! `DefFrameProc` et `DefMDIChildProc` (#2321).
//!
//! Source : documentation publique de Microsoft Learn, « About the
//! Multiple-Document Interface », « Using the Multiple Document Interface »,
//! les pages des messages `WM_MDI*`, `CLIENTCREATESTRUCT`, `MDICREATESTRUCT`,
//! `DefFrameProc` et `DefMDIChildProc`.
//!
//! Sans cette classe, `CMDIFrameWnd::CreateClient` de MFC échouait (erreur
//! 1407), le cadre renvoyait -1 et l'application sortait aussitôt en 0
//! (Pythonwin). Limites : le sous-menu Fenêtre ne reçoit pas la liste des
//! enfants, et la réduction en icône d'un enfant n'est pas gérée.

use super::gui::{self, Rect};
use super::user32;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_SETFOCUS: u32 = 0x0007;
const WM_CLOSE: u32 = 0x0010;
const WM_CHILDACTIVATE: u32 = 0x0022;
const WM_NCACTIVATE: u32 = 0x0086;
const WM_COMMAND: u32 = 0x0111;
const WM_SYSCOMMAND: u32 = 0x0112;
pub const WM_MDICREATE: u32 = 0x0220;
pub const WM_MDIDESTROY: u32 = 0x0221;
pub const WM_MDIACTIVATE: u32 = 0x0222;
pub const WM_MDIRESTORE: u32 = 0x0223;
pub const WM_MDINEXT: u32 = 0x0224;
pub const WM_MDIMAXIMIZE: u32 = 0x0225;
pub const WM_MDITILE: u32 = 0x0226;
pub const WM_MDICASCADE: u32 = 0x0227;
pub const WM_MDIICONARRANGE: u32 = 0x0228;
pub const WM_MDIGETACTIVE: u32 = 0x0229;
pub const WM_MDISETMENU: u32 = 0x0230;
pub const WM_MDIREFRESHMENU: u32 = 0x0234;

const SC_SIZE: usize = 0xF000;
const SC_MINIMIZE: usize = 0xF020;
const SC_MAXIMIZE: usize = 0xF030;
const SC_NEXTWINDOW: usize = 0xF040;
const SC_PREVWINDOW: usize = 0xF050;
const SC_CLOSE: usize = 0xF060;
const SC_RESTORE: usize = 0xF120;

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
const WS_MINIMIZE: u32 = 0x2000_0000;
const WS_MAXIMIZE: u32 = 0x0100_0000;
const WS_HSCROLL: u32 = 0x0010_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_EX_MDICHILD: u32 = 0x0000_0040;
/// Style du client : les enfants prennent n'importe quel style.
pub const MDIS_ALLCHILDSTYLES: u32 = 0x0001;
const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
const GWLP_ID: i32 = -12;
const GW_HWNDNEXT: u32 = 2;
const GW_CHILD: u32 = 5;
/// Décalage de la mise en cascade (barre de titre + bord).
const CASCADE_STEP: i32 = 26;

#[derive(Default, Clone, Copy)]
struct Client {
    frame: usize,
    window_menu: usize,
    first_child: u32,
    active: usize,
    maximized: bool,
    /// Place d'un enfant agrandi, rendue par WM_MDIRESTORE.
    restore: Option<(usize, Rect)>,
}

fn clients() -> &'static Mutex<HashMap<usize, Client>> {
    static C: OnceLock<Mutex<HashMap<usize, Client>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(client: usize) -> Option<Client> {
    clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&client)
        .copied()
}

fn update(client: usize, f: impl FnOnce(&mut Client)) {
    if let Some(c) = clients()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&client)
    {
        f(c);
    }
}

/// La fenêtre est-elle un client MDI ?
pub fn is_client(hwnd: usize) -> bool {
    get(hwnd).is_some()
}

/// Enfants du client, dans l'ordre de création.
fn children(client: usize) -> Vec<usize> {
    let mut v = Vec::new();
    let mut h = user32::get_window(client, GW_CHILD);
    while h != 0 && v.len() < 4096 {
        v.push(h);
        h = user32::get_window(h, GW_HWNDNEXT);
    }
    v
}

fn client_area(client: usize) -> Rect {
    gui::window_client_rect(client).unwrap_or_default()
}

/// Lit une case de la taille d'un pointeur du programme (x86 ou x64).
unsafe fn ptr_at(base: usize, index: usize) -> usize {
    crate::wow32::read_ptr(base + index * crate::wow32::pointer_size())
}

unsafe fn place(hwnd: usize, r: Rect) {
    user32::set_window_pos(
        hwnd,
        0,
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        0x0014,
    );
}

/// Active `child` : WM_MDIACTIVATE (wParam = enfant désactivé, lParam =
/// enfant activé) et WM_NCACTIVATE aux deux, focus au nouvel enfant.
unsafe fn activate(client: usize, child: usize) {
    let Some(c) = get(client) else { return };
    if child == c.active || (child != 0 && !gui::is_window(child)) {
        return;
    }
    let old = c.active;
    update(client, |c| c.active = child);
    if old != 0 && gui::is_window(old) {
        user32::send_message_w(old, WM_NCACTIVATE, 0, 0);
        user32::send_message_w(old, WM_MDIACTIVATE, old, child as isize);
    }
    if child != 0 {
        user32::send_message_w(child, WM_NCACTIVATE, 1, 0);
        user32::send_message_w(child, WM_MDIACTIVATE, old, child as isize);
        // Un enfant agrandi passe la main agrandi (comportement documenté
        // de WM_MDIACTIVATE quand l'enfant actif est agrandi).
        if c.maximized && old != 0 && old != child {
            maximize(client, child);
        }
        user32::set_focus(child);
    }
}

unsafe fn maximize(client: usize, child: usize) {
    if child == 0 {
        return;
    }
    let area = client_area(client);
    let c = get(client).unwrap_or_default();
    // Rend d'abord sa place à l'ancien enfant agrandi.
    if let Some((prev, r)) = c.restore.filter(|(p, _)| *p != child) {
        if gui::is_window(prev) {
            place(prev, r);
        }
    }
    // Rectangle d'un enfant : relatif à la zone client du parent.
    let saved = gui::window_rect(child);
    update(client, |c| {
        c.maximized = true;
        if c.restore.is_none_or(|(p, _)| p != child) {
            c.restore = saved.map(|r| (child, r));
        }
    });
    place(child, area);
}

unsafe fn restore(client: usize, child: usize) {
    let c = get(client).unwrap_or_default();
    if let Some((p, r)) = c.restore.filter(|(p, _)| *p == child) {
        if gui::is_window(p) {
            place(p, r);
        }
    }
    update(client, |c| {
        c.maximized = false;
        c.restore = None;
    });
}

/// WM_MDICREATE : `mcs` pointe un MDICREATESTRUCT (szClass, szTitle,
/// hOwner, x, y, cx, cy, style, lParam). Rend l'enfant, ou 0.
unsafe fn create_child(client: usize, mcs: usize) -> usize {
    if mcs == 0 {
        return 0;
    }
    let p = crate::wow32::pointer_size();
    let class = ptr_at(mcs, 0);
    let title = ptr_at(mcs, 1);
    let owner = ptr_at(mcs, 2);
    let int = |i: usize| std::ptr::read_unaligned((mcs + 3 * p + 4 * i) as *const i32);
    let (mut x, mut y, mut cx, mut cy) = (int(0), int(1), int(2), int(3));
    let asked = int(4) as u32;
    let all_styles = gui::window_style(client).unwrap_or(0) & MDIS_ALLCHILDSTYLES != 0;
    let style = if all_styles {
        asked
    } else {
        asked & (WS_MINIMIZE | WS_MAXIMIZE | WS_HSCROLL | WS_VSCROLL)
            | WS_CAPTION
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
    } | WS_CHILD
        | WS_CLIPSIBLINGS
        | WS_CLIPCHILDREN;
    let existing = children(client);
    let area = client_area(client);
    let n = existing.len() as i32;
    if x == CW_USEDEFAULT {
        x = (n * CASCADE_STEP) % (area.right / 2).max(1);
        y = (n * CASCADE_STEP) % (area.bottom / 2).max(1);
    }
    if cx == CW_USEDEFAULT {
        cx = (area.right * 3 / 4).max(100);
        cy = (area.bottom * 3 / 4).max(80);
    }
    let c = get(client).unwrap_or_default();
    let id = c.first_child as usize + existing.len();
    let wide = super::controls::message_is_wide(client);
    let hwnd = if wide {
        user32::create_window_ex_w(
            WS_EX_MDICHILD,
            class as *const u16,
            title as *const u16,
            style & !WS_VISIBLE,
            x,
            y,
            cx,
            cy,
            client,
            id,
            owner,
            mcs as *mut _,
        )
    } else {
        user32::create_window_ex_a(
            WS_EX_MDICHILD,
            class as *const u8,
            title as *const u8,
            style & !WS_VISIBLE,
            x,
            y,
            cx,
            cy,
            client,
            id,
            owner,
            mcs as *mut _,
        )
    };
    if hwnd == 0 {
        return 0;
    }
    if style & WS_MAXIMIZE != 0 || c.maximized {
        maximize(client, hwnd);
    }
    user32::show_window(hwnd, 5); // SW_SHOW
    activate(client, hwnd);
    hwnd
}

unsafe fn destroy_child(client: usize, child: usize) {
    if child == 0 || !gui::is_window(child) {
        return;
    }
    let was_active = get(client).is_some_and(|c| c.active == child);
    if get(client).is_some_and(|c| c.restore.is_some_and(|(p, _)| p == child)) {
        update(client, |c| c.restore = None);
    }
    if was_active {
        let next = children(client).into_iter().rev().find(|&h| h != child);
        let maximized = get(client).is_some_and(|c| c.maximized);
        update(client, |c| c.maximized = false);
        match next {
            Some(n) => {
                activate(client, n);
                if maximized {
                    maximize(client, n);
                }
            }
            None => {
                user32::send_message_w(child, WM_MDIACTIVATE, child, 0);
                update(client, |c| c.active = 0);
            }
        }
    }
    user32::destroy_window(child);
    // Renumérote les enfants restants (idFirstChild + rang, comme Windows).
    if let Some(c) = get(client) {
        for (i, h) in children(client).into_iter().enumerate() {
            user32::set_window_long_ptr_w(h, GWLP_ID, (c.first_child as usize + i) as isize);
        }
    }
}

unsafe fn next_child(client: usize, from: usize, previous: bool) {
    let list: Vec<usize> = children(client)
        .into_iter()
        .filter(|&h| gui::visible_flag(h) == Some(true))
        .collect();
    if list.len() < 2 {
        return;
    }
    let from = if from == 0 {
        get(client).map_or(0, |c| c.active)
    } else {
        from
    };
    let pos = list.iter().position(|&h| h == from).unwrap_or(0);
    let n = list.len();
    let target = if previous {
        list[(pos + n - 1) % n]
    } else {
        list[(pos + 1) % n]
    };
    activate(client, target);
}

unsafe fn tile(client: usize, horizontal: bool) {
    let list: Vec<usize> = children(client)
        .into_iter()
        .filter(|&h| gui::visible_flag(h) == Some(true))
        .collect();
    if list.is_empty() {
        return;
    }
    restore(client, get(client).map_or(0, |c| c.active));
    let area = client_area(client);
    let n = list.len() as i32;
    for (i, &h) in list.iter().enumerate() {
        let i = i as i32;
        let r = if horizontal {
            let hgt = area.bottom / n;
            Rect {
                left: 0,
                top: i * hgt,
                right: area.right,
                bottom: (i + 1) * hgt,
            }
        } else {
            let w = area.right / n;
            Rect {
                left: i * w,
                top: 0,
                right: (i + 1) * w,
                bottom: area.bottom,
            }
        };
        place(h, r);
    }
}

unsafe fn cascade(client: usize) {
    restore(client, get(client).map_or(0, |c| c.active));
    let area = client_area(client);
    let (w, h) = ((area.right * 3 / 4).max(100), (area.bottom * 3 / 4).max(80));
    let list: Vec<usize> = children(client)
        .into_iter()
        .filter(|&h| gui::visible_flag(h) == Some(true))
        .collect();
    for (i, &child) in list.iter().enumerate() {
        let o = i as i32 * CASCADE_STEP;
        place(
            child,
            Rect {
                left: o,
                top: o,
                right: o + w,
                bottom: o + h,
            },
        );
    }
}

/// Procédure de la classe système `MDICLIENT`.
pub(super) unsafe extern "win64" fn mdiclient_proc(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    match msg {
        WM_CREATE => {
            // lpCreateParams → CLIENTCREATESTRUCT { hWindowMenu, idFirstChild }.
            let mut c = Client {
                frame: gui::window_parent(hwnd).unwrap_or(0),
                ..Default::default()
            };
            if lp != 0 {
                let ccs = ptr_at(lp as usize, 0);
                if ccs != 0 {
                    c.window_menu = ptr_at(ccs, 0);
                    c.first_child = std::ptr::read_unaligned(
                        (ccs + crate::wow32::pointer_size()) as *const u32,
                    );
                }
            }
            clients()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(hwnd, c);
            0
        }
        WM_DESTROY => {
            clients()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&hwnd);
            gui::def_window_proc(hwnd, msg, wp, lp)
        }
        WM_SIZE => {
            if let Some(c) = get(hwnd).filter(|c| c.maximized && c.active != 0) {
                place(c.active, client_area(hwnd));
            }
            gui::def_window_proc(hwnd, msg, wp, lp)
        }
        WM_SETFOCUS => {
            if let Some(a) = get(hwnd).map(|c| c.active).filter(|&a| a != 0) {
                user32::set_focus(a);
            }
            0
        }
        WM_MDICREATE => create_child(hwnd, lp as usize) as isize,
        WM_MDIDESTROY => {
            destroy_child(hwnd, wp);
            0
        }
        WM_MDIACTIVATE => {
            activate(hwnd, wp);
            0
        }
        WM_MDIGETACTIVE => {
            let c = get(hwnd).unwrap_or_default();
            if lp != 0 {
                std::ptr::write_unaligned(lp as *mut i32, c.maximized as i32);
            }
            c.active as isize
        }
        WM_MDINEXT => {
            next_child(hwnd, wp, lp != 0);
            0
        }
        WM_MDIMAXIMIZE => {
            maximize(hwnd, wp);
            0
        }
        WM_MDIRESTORE => {
            restore(hwnd, wp);
            0
        }
        WM_MDITILE => {
            // MDITILE_HORIZONTAL = 1 ; sinon côte à côte (MDITILE_VERTICAL).
            tile(hwnd, wp & 1 != 0);
            1
        }
        WM_MDICASCADE => {
            cascade(hwnd);
            1
        }
        WM_MDIICONARRANGE => 0,
        WM_MDISETMENU => {
            let c = get(hwnd).unwrap_or_default();
            let old = if c.frame != 0 {
                user32::get_menu(c.frame)
            } else {
                0
            };
            if wp != 0 && c.frame != 0 {
                user32::set_menu(c.frame, wp);
            }
            if lp != 0 {
                update(hwnd, |c| c.window_menu = lp as usize);
            }
            old as isize
        }
        WM_MDIREFRESHMENU => {
            let frame = get(hwnd).map_or(0, |c| c.frame);
            if frame != 0 {
                user32::draw_menu_bar(frame);
                return user32::get_menu(frame) as isize;
            }
            0
        }
        _ => gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

/// DefFrameProc : WM_SIZE adapte le client à la zone cliente du cadre,
/// WM_SETFOCUS passe le focus au client, WM_COMMAND d'un identifiant
/// d'enfant active cet enfant ; le reste va à DefWindowProc.
pub unsafe fn def_frame_proc(frame: usize, client: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if client != 0 && is_client(client) {
        match msg {
            WM_SIZE => {
                let (w, h) = ((lp & 0xFFFF) as i32, ((lp >> 16) & 0xFFFF) as i32);
                user32::move_window(client, 0, 0, w, h, 1);
                return 0;
            }
            WM_SETFOCUS => {
                user32::set_focus(client);
                return 0;
            }
            WM_COMMAND => {
                let id = (wp & 0xFFFF) as u32;
                let c = get(client).unwrap_or_default();
                if c.first_child != 0 && id >= c.first_child {
                    let target = children(client)
                        .into_iter()
                        .find(|&h| user32::get_window_long_ptr_w(h, GWLP_ID) as u32 == id);
                    if let Some(t) = target {
                        activate(client, t);
                        return 0;
                    }
                } else if c.active != 0 {
                    // Les commandes de menu système de l'enfant agrandi.
                    if (wp & 0xFFFF) >= 0xF000 {
                        return user32::send_message_w(c.active, WM_SYSCOMMAND, wp, lp);
                    }
                }
            }
            _ => {}
        }
    }
    gui::def_window_proc(frame, msg, wp, lp)
}

/// DefMDIChildProc : activation par le client, fermeture par
/// WM_MDIDESTROY, commandes système agrandir, restaurer, suivant.
pub unsafe fn def_mdi_child_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let client = gui::window_parent(hwnd).unwrap_or(0);
    if !is_client(client) {
        return gui::def_window_proc(hwnd, msg, wp, lp);
    }
    match msg {
        WM_CHILDACTIVATE => {
            activate(client, hwnd);
            0
        }
        WM_SETFOCUS => {
            if get(client).is_some_and(|c| c.active != hwnd) {
                activate(client, hwnd);
            }
            gui::def_window_proc(hwnd, msg, wp, lp)
        }
        WM_CLOSE => {
            user32::send_message_w(client, WM_MDIDESTROY, hwnd, 0);
            0
        }
        WM_SYSCOMMAND => match wp & 0xFFF0 {
            SC_MAXIMIZE => {
                maximize(client, hwnd);
                0
            }
            SC_RESTORE => {
                restore(client, hwnd);
                0
            }
            SC_NEXTWINDOW => {
                next_child(client, hwnd, false);
                0
            }
            SC_PREVWINDOW => {
                next_child(client, hwnd, true);
                0
            }
            SC_CLOSE => {
                user32::send_message_w(hwnd, WM_CLOSE, 0, 0);
                0
            }
            SC_MINIMIZE | SC_SIZE => 0,
            _ => gui::def_window_proc(hwnd, msg, wp, lp),
        },
        _ => gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// MFC (`CMDIFrameWnd::CreateClient`) crée MDICLIENT avec
    /// WS_EX_CLIENTEDGE et MDIS_ALLCHILDSTYLES ; un enfant créé par
    /// WM_MDICREATE apparaît, devient actif, et WM_MDIDESTROY le retire.
    #[test]
    fn client_mdi_cree_active_et_detruit_ses_enfants() {
        unsafe {
            let frame = user32::create_window_ex_w(
                0,
                wide("STATIC").as_ptr(),
                std::ptr::null(),
                0x00CF_0000,
                0,
                0,
                640,
                480,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(frame, 0);
            // CLIENTCREATESTRUCT { hWindowMenu, idFirstChild }.
            #[repr(C)]
            struct Ccs {
                menu: usize,
                first: u32,
            }
            let mut ccs = Ccs {
                menu: 0,
                first: 0xFF00,
            };
            let class = wide("MDIClient");
            let client = user32::create_window_ex_w(
                0x200,
                class.as_ptr(),
                std::ptr::null(),
                0x5600_0001,
                0,
                0,
                400,
                300,
                frame,
                0xE900,
                0,
                &mut ccs as *mut Ccs as *mut _,
            );
            assert_ne!(client, 0, "classe MDICLIENT fournie (#2321)");
            assert!(is_client(client));

            #[repr(C)]
            struct Mcs {
                class: *const u16,
                title: *const u16,
                owner: usize,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                style: u32,
                param: isize,
            }
            let static_ = wide("STATIC");
            let t1 = wide("Doc 1");
            let mut mcs = Mcs {
                class: static_.as_ptr(),
                title: t1.as_ptr(),
                owner: 0,
                x: CW_USEDEFAULT,
                y: CW_USEDEFAULT,
                cx: CW_USEDEFAULT,
                cy: CW_USEDEFAULT,
                style: 0,
                param: 0,
            };
            let c1 = user32::send_message_w(client, WM_MDICREATE, 0, &mut mcs as *mut Mcs as isize)
                as usize;
            assert_ne!(c1, 0, "un enfant MDI apparaît");
            assert_eq!(gui::window_parent(c1), Some(client));
            assert!(gui::window_ex_style(c1).unwrap() & WS_EX_MDICHILD != 0);
            assert_eq!(gui::visible_flag(c1), Some(true));
            assert_eq!(user32::get_window_long_ptr_w(c1, GWLP_ID), 0xFF00);
            let mut max = 7i32;
            assert_eq!(
                user32::send_message_w(client, WM_MDIGETACTIVE, 0, &mut max as *mut i32 as isize)
                    as usize,
                c1
            );
            assert_eq!(max, 0);

            let t2 = wide("Doc 2");
            mcs.title = t2.as_ptr();
            let c2 = user32::send_message_w(client, WM_MDICREATE, 0, &mut mcs as *mut Mcs as isize)
                as usize;
            assert_ne!(c2, 0);
            assert_eq!(user32::get_window_long_ptr_w(c2, GWLP_ID), 0xFF01);
            assert_eq!(
                user32::send_message_w(client, WM_MDIGETACTIVE, 0, 0) as usize,
                c2
            );

            // Agrandir : l'enfant couvre le client.
            user32::send_message_w(client, WM_MDIMAXIMIZE, c2, 0);
            user32::send_message_w(client, WM_MDIGETACTIVE, 0, &mut max as *mut i32 as isize);
            assert_eq!(max, 1);
            let area = client_area(client);
            let r = gui::window_client_rect(c2).unwrap();
            assert!(r.right - r.left <= area.right && r.right > 0);
            user32::send_message_w(client, WM_MDIRESTORE, c2, 0);

            // WM_MDINEXT et l'activation par le menu Fenêtre (DefFrameProc).
            user32::send_message_w(client, WM_MDINEXT, 0, 0);
            assert_eq!(
                user32::send_message_w(client, WM_MDIGETACTIVE, 0, 0) as usize,
                c1
            );
            def_frame_proc(frame, client, WM_COMMAND, 0xFF01, 0);
            assert_eq!(
                user32::send_message_w(client, WM_MDIGETACTIVE, 0, 0) as usize,
                c2
            );

            // DefFrameProc(WM_SIZE) adapte le client au cadre.
            def_frame_proc(frame, client, WM_SIZE, 0, (200 << 16) | 300);
            let r = gui::window_client_rect(client).unwrap();
            assert_eq!((r.right - r.left, r.bottom - r.top), (300, 200));

            // WM_CLOSE de l'enfant passe par WM_MDIDESTROY.
            def_mdi_child_proc(c2, WM_CLOSE, 0, 0);
            assert!(!gui::is_window(c2));
            assert_eq!(
                user32::send_message_w(client, WM_MDIGETACTIVE, 0, 0) as usize,
                c1
            );
            user32::send_message_w(client, WM_MDIDESTROY, c1, 0);
            assert!(!gui::is_window(c1));
            assert_eq!(user32::send_message_w(client, WM_MDIGETACTIVE, 0, 0), 0);

            user32::destroy_window(frame);
            assert!(!is_client(client));
        }
    }
}
