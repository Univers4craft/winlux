//! Menus Win32 : vrais objets menu (éléments, sous-menus, états) derrière
//! CreateMenu, AppendMenu, InsertMenu, ModifyMenu, DeleteMenu, RemoveMenu,
//! GetMenuItemCount, Get/SetMenuItemInfo, CheckMenuItem, EnableMenuItem…
//!
//! Les anciennes versions vides renvoyaient toujours « réussi » : PuTTY vide
//! un menu par `while (DeleteMenu(m, 0, MF_BYPOSITION))` et tournait sans
//! fin. Ici, supprimer un élément qui n'existe pas échoue, comme sous
//! Windows.

use super::{lock, Exports};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const MF_GRAYED: u32 = 0x1;
const MF_DISABLED: u32 = 0x2;
const MF_CHECKED: u32 = 0x8;
const MF_POPUP: u32 = 0x10;
const MF_OWNERDRAW: u32 = 0x100;
const MF_BYPOSITION: u32 = 0x400;
const MF_SEPARATOR: u32 = 0x800;
const MF_RADIOCHECK: u32 = 0x200;
const MF_DEFAULT: u32 = 0x1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub id: u32,
    pub flags: u32,
    pub text: String,
    pub submenu: usize,
    pub data: usize,
}

#[derive(Default)]
struct Menus {
    menus: HashMap<usize, Vec<Item>>,
    next: usize,
    window_menu: HashMap<usize, usize>,
    system_menu: HashMap<usize, usize>,
}

fn menus() -> &'static Mutex<Menus> {
    static M: OnceLock<Mutex<Menus>> = OnceLock::new();
    M.get_or_init(|| {
        Mutex::new(Menus {
            next: 0x0010_0000,
            ..Default::default()
        })
    })
}

pub fn create() -> usize {
    let mut m = lock(menus());
    m.next += 4;
    let h = m.next;
    m.menus.insert(h, Vec::new());
    h
}

/// Éléments d'un menu (copie), pour l'affichage.
pub fn items(menu: usize) -> Option<Vec<Item>> {
    lock(menus()).menus.get(&menu).cloned()
}

/// Menu système d'une fenêtre (ou l'un de ses sous-menus) : ses commandes
/// partent en WM_SYSCOMMAND.
pub fn is_system_menu(menu: usize) -> bool {
    lock(menus()).system_menu.values().any(|&h| h == menu)
}

/// Position d'un élément : par position, ou par identifiant de commande
/// (cherché aussi dans les sous-menus). Renvoie (menu, position).
fn locate(m: &Menus, menu: usize, item: u32, by_pos: bool) -> Option<(usize, usize)> {
    let list = m.menus.get(&menu)?;
    if by_pos {
        return ((item as usize) < list.len()).then_some((menu, item as usize));
    }
    // Parcours dans l'ordre des éléments, en descendant dans chaque
    // sous-menu à sa place : Notepad++ a « + » (41001) et « × » (41003) au
    // bout de sa barre, mais « Nouveau » et « Fermer » du menu Fichier,
    // rencontrés avant, sont ceux que ModifyMenu traduit (#709).
    for (i, it) in list.iter().enumerate() {
        if it.submenu != 0 {
            if let Some(found) = locate(m, it.submenu, item, false) {
                return Some(found);
            }
        } else if it.id == item {
            return Some((menu, i));
        }
    }
    // Un sous-menu se désigne aussi par son handle.
    list.iter()
        .position(|it| it.submenu != 0 && it.submenu as u32 == item)
        .map(|i| (menu, i))
}

fn destroy_tree(m: &mut Menus, menu: usize) {
    if let Some(list) = m.menus.remove(&menu) {
        for it in list {
            if it.submenu != 0 {
                destroy_tree(m, it.submenu);
            }
        }
    }
}

fn make_item(flags: u32, id: usize, text: Option<String>) -> Item {
    let popup = flags & MF_POPUP != 0;
    Item {
        id: if popup { u32::MAX } else { id as u32 },
        flags: flags & !MF_BYPOSITION,
        text: if flags & (MF_SEPARATOR | MF_OWNERDRAW | 0x4) == 0 {
            text.unwrap_or_default()
        } else {
            String::new()
        },
        submenu: if popup { id } else { 0 },
        data: 0,
    }
}

unsafe fn text_a(p: *const u8, flags: u32) -> Option<String> {
    (flags & (MF_SEPARATOR | MF_OWNERDRAW | 0x4) == 0 && !p.is_null()).then(|| super::cstr(p))
}
unsafe fn text_w(p: *const u16, flags: u32) -> Option<String> {
    (flags & (MF_SEPARATOR | MF_OWNERDRAW | 0x4) == 0 && !p.is_null()).then(|| super::wstr(p))
}

pub fn append(menu: usize, flags: u32, id: usize, text: Option<String>) -> bool {
    let mut m = lock(menus());
    match m.menus.get_mut(&menu) {
        Some(list) => {
            list.push(make_item(flags, id, text));
            true
        }
        None => false,
    }
}

pub fn insert(menu: usize, pos: u32, flags: u32, id: usize, text: Option<String>) -> bool {
    let mut m = lock(menus());
    let by_pos = flags & MF_BYPOSITION != 0;
    let at = if pos == u32::MAX && by_pos {
        None
    } else {
        locate(&m, menu, pos, by_pos)
    };
    let item = make_item(flags, id, text);
    match at {
        Some((owner, i)) => {
            if let Some(l) = m.menus.get_mut(&owner) {
                l.insert(i, item);
            }
            true
        }
        None => match m.menus.get_mut(&menu) {
            // Position inconnue : ajouté à la fin, comme Windows.
            Some(l) => {
                l.push(item);
                true
            }
            None => false,
        },
    }
}

/// Retire un élément ; `destroy` détruit aussi son sous-menu (DeleteMenu).
pub fn remove(menu: usize, pos: u32, flags: u32, destroy: bool) -> bool {
    let mut m = lock(menus());
    let Some((owner, i)) = locate(&m, menu, pos, flags & MF_BYPOSITION != 0) else {
        return false;
    };
    let item = m.menus.get_mut(&owner).map(|l| l.remove(i));
    if destroy {
        if let Some(sub) = item.map(|it| it.submenu).filter(|&s| s != 0) {
            destroy_tree(&mut m, sub);
        }
    }
    true
}

/// Change les drapeaux d'état (MF_CHECKED, MF_GRAYED…) ; renvoie l'ancien
/// état masqué par `mask`, ou -1 si l'élément n'existe pas.
fn set_state(menu: usize, item: u32, flags: u32, mask: u32) -> i32 {
    let mut m = lock(menus());
    let Some((owner, i)) = locate(&m, menu, item, flags & MF_BYPOSITION != 0) else {
        return -1;
    };
    let Some(it) = m.menus.get_mut(&owner).and_then(|l| l.get_mut(i)) else {
        return -1;
    };
    let old = it.flags & mask;
    it.flags = (it.flags & !mask) | (flags & mask);
    old as i32
}

fn system_menu_items() -> Vec<(u32, &'static str)> {
    let fr = super::dialogs::session_lang() == "fr";
    vec![
        (0xF120, if fr { "&Restaurer" } else { "&Restore" }),
        (0xF010, if fr { "&Déplacer" } else { "&Move" }),
        (0xF000, if fr { "&Taille" } else { "&Size" }),
        (0xF020, if fr { "Réd&uire" } else { "Mi&nimize" }),
        (0xF030, if fr { "Agrandi&r" } else { "Ma&ximize" }),
        (0, ""),
        (
            0xF060,
            if fr {
                "&Fermer\tAlt+F4"
            } else {
                "&Close\tAlt+F4"
            },
        ),
    ]
}

// ---------- Fonctions exportées ----------

extern "win64" fn create_menu() -> usize {
    create()
}
unsafe extern "win64" fn destroy_menu(menu: usize) -> i32 {
    let mut m = lock(menus());
    if !m.menus.contains_key(&menu) {
        return 0;
    }
    destroy_tree(&mut m, menu);
    m.window_menu.retain(|_, v| *v != menu);
    1
}
extern "win64" fn is_menu(menu: usize) -> i32 {
    lock(menus()).menus.contains_key(&menu) as i32
}
unsafe extern "win64" fn append_menu_a(menu: usize, flags: u32, id: usize, t: *const u8) -> i32 {
    append(menu, flags, id, text_a(t, flags)) as i32
}
unsafe extern "win64" fn append_menu_w(menu: usize, flags: u32, id: usize, t: *const u16) -> i32 {
    append(menu, flags, id, text_w(t, flags)) as i32
}
unsafe extern "win64" fn insert_menu_a(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    t: *const u8,
) -> i32 {
    insert(menu, pos, flags, id, text_a(t, flags)) as i32
}
unsafe extern "win64" fn insert_menu_w(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    t: *const u16,
) -> i32 {
    insert(menu, pos, flags, id, text_w(t, flags)) as i32
}
unsafe fn modify(menu: usize, pos: u32, flags: u32, id: usize, text: Option<String>) -> i32 {
    let mut m = lock(menus());
    let Some((owner, i)) = locate(&m, menu, pos, flags & MF_BYPOSITION != 0) else {
        return 0;
    };
    let mut item = make_item(flags, id, text);
    if let Some(slot) = m.menus.get_mut(&owner).and_then(|l| l.get_mut(i)) {
        // Sans MF_POPUP, un menu déroulant garde son sous-menu : Notepad++
        // traduit ainsi les titres de sa barre (ModifyMenu MF_BYPOSITION |
        // MF_STRING, identifiant 0), qui ne s'ouvraient plus (#709).
        if flags & MF_POPUP == 0 && slot.submenu != 0 {
            item.submenu = slot.submenu;
            item.id = slot.id;
            item.flags |= MF_POPUP;
        }
        *slot = item;
    }
    1
}
unsafe extern "win64" fn modify_menu_a(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    t: *const u8,
) -> i32 {
    modify(menu, pos, flags, id, text_a(t, flags))
}
unsafe extern "win64" fn modify_menu_w(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    t: *const u16,
) -> i32 {
    modify(menu, pos, flags, id, text_w(t, flags))
}
extern "win64" fn delete_menu(menu: usize, pos: u32, flags: u32) -> i32 {
    remove(menu, pos, flags, true) as i32
}
extern "win64" fn remove_menu(menu: usize, pos: u32, flags: u32) -> i32 {
    remove(menu, pos, flags, false) as i32
}
extern "win64" fn get_menu_item_count(menu: usize) -> i32 {
    lock(menus())
        .menus
        .get(&menu)
        .map_or(-1, |l| l.len() as i32)
}
extern "win64" fn get_menu_item_id(menu: usize, pos: i32) -> u32 {
    let m = lock(menus());
    m.menus
        .get(&menu)
        .and_then(|l| l.get(pos as usize))
        .map_or(
            u32::MAX,
            |it| if it.submenu != 0 { u32::MAX } else { it.id },
        )
}
extern "win64" fn get_sub_menu(menu: usize, pos: i32) -> usize {
    let m = lock(menus());
    m.menus
        .get(&menu)
        .and_then(|l| l.get(pos as usize))
        .map_or(0, |it| it.submenu)
}
extern "win64" fn get_menu_state(menu: usize, item: u32, flags: u32) -> u32 {
    let m = lock(menus());
    let Some((owner, i)) = locate(&m, menu, item, flags & MF_BYPOSITION != 0) else {
        return u32::MAX;
    };
    let it = &m.menus[&owner][i];
    if it.submenu != 0 {
        let n = m.menus.get(&it.submenu).map_or(0, |l| l.len() as u32);
        (n << 8) | (it.flags & 0xFF)
    } else {
        it.flags & 0xFFFF
    }
}
extern "win64" fn check_menu_item(menu: usize, item: u32, flags: u32) -> u32 {
    set_state(menu, item, flags, MF_CHECKED) as u32
}
extern "win64" fn enable_menu_item(menu: usize, item: u32, flags: u32) -> i32 {
    set_state(menu, item, flags, MF_GRAYED | MF_DISABLED)
}
extern "win64" fn check_menu_radio_item(
    menu: usize,
    first: u32,
    last: u32,
    check: u32,
    flags: u32,
) -> i32 {
    let mut m = lock(menus());
    let by_pos = flags & MF_BYPOSITION != 0;
    let Some((owner, _)) = locate(&m, menu, check, by_pos) else {
        return 0;
    };
    let Some(list) = m.menus.get_mut(&owner) else {
        return 0;
    };
    for (i, it) in list.iter_mut().enumerate() {
        let key = if by_pos { i as u32 } else { it.id };
        if key >= first && key <= last {
            if key == check {
                it.flags |= MF_CHECKED | MF_RADIOCHECK;
            } else {
                it.flags &= !MF_CHECKED;
            }
        }
    }
    1
}
unsafe fn menu_string(menu: usize, item: u32, flags: u32) -> Option<String> {
    let m = lock(menus());
    let (owner, i) = locate(&m, menu, item, flags & MF_BYPOSITION != 0)?;
    Some(m.menus[&owner][i].text.clone())
}
unsafe extern "win64" fn get_menu_string_a(
    menu: usize,
    item: u32,
    buf: *mut u8,
    cap: i32,
    flags: u32,
) -> i32 {
    let Some(s) = menu_string(menu, item, flags) else {
        return 0;
    };
    if buf.is_null() || cap <= 0 {
        return s.len() as i32;
    }
    super::put_text_a(&s, buf, cap as u32) as i32
}
unsafe extern "win64" fn get_menu_string_w(
    menu: usize,
    item: u32,
    buf: *mut u16,
    cap: i32,
    flags: u32,
) -> i32 {
    let Some(s) = menu_string(menu, item, flags) else {
        return 0;
    };
    if buf.is_null() || cap <= 0 {
        return s.encode_utf16().count() as i32;
    }
    super::put_text_w(&s, buf, cap as u32) as i32
}

// MENUITEMINFO (64 bits) : cbSize 0, fMask 4, fType 8, fState 12, wID 16,
// hSubMenu 24, hbmpChecked 32, hbmpUnchecked 40, dwItemData 48,
// dwTypeData 56, cch 64, hbmpItem 72.
const MIIM_STATE: u32 = 0x1;
const MIIM_ID: u32 = 0x2;
const MIIM_SUBMENU: u32 = 0x4;
const MIIM_TYPE: u32 = 0x10;
const MIIM_DATA: u32 = 0x20;
const MIIM_STRING: u32 = 0x40;
const MIIM_FTYPE: u32 = 0x100;
const TYPE_BITS: u32 = MF_SEPARATOR | MF_OWNERDRAW | MF_RADIOCHECK | 0x4 | 0x20 | 0x40 | 0x4000;
const STATE_BITS: u32 = MF_GRAYED | MF_DISABLED | MF_CHECKED | 0x80 | MF_DEFAULT;

unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
    std::ptr::read_unaligned(p.add(off) as *const T)
}
unsafe fn wr<T: Copy>(p: *mut u8, off: usize, v: T) {
    std::ptr::write_unaligned(p.add(off) as *mut T, v)
}

unsafe fn apply_info(it: &mut Item, info: *const u8, wide: bool) {
    let mask: u32 = rd(info, 4);
    let ty: u32 = rd(info, 8);
    if mask & (MIIM_FTYPE | MIIM_TYPE) != 0 {
        it.flags = (it.flags & !TYPE_BITS) | (ty & TYPE_BITS);
    }
    if mask & MIIM_STATE != 0 {
        let st: u32 = rd(info, 12);
        it.flags = (it.flags & !STATE_BITS) | (st & STATE_BITS);
    }
    if mask & MIIM_ID != 0 {
        it.id = rd(info, 16);
    }
    if mask & MIIM_SUBMENU != 0 {
        it.submenu = rd(info, 24);
        if it.submenu != 0 {
            it.flags |= MF_POPUP;
        } else {
            it.flags &= !MF_POPUP;
        }
    }
    if mask & MIIM_DATA != 0 {
        it.data = rd(info, 48);
    }
    let text_ok = mask & MIIM_STRING != 0 || (mask & MIIM_TYPE != 0 && ty & TYPE_BITS == 0);
    if text_ok {
        let p: usize = rd(info, 56);
        if p != 0 {
            it.text = if wide {
                super::wstr(p as *const u16)
            } else {
                super::cstr(p as *const u8)
            };
        }
    }
}

unsafe fn insert_item(menu: usize, item: u32, by_pos: i32, info: *const u8, wide: bool) -> i32 {
    if info.is_null() {
        return 0;
    }
    let mut it = Item {
        id: 0,
        ..Default::default()
    };
    apply_info(&mut it, info, wide);
    let mut m = lock(menus());
    let at = locate(&m, menu, item, by_pos != 0);
    match at {
        Some((owner, i)) => {
            if let Some(l) = m.menus.get_mut(&owner) {
                l.insert(i, it);
            }
            1
        }
        None => match m.menus.get_mut(&menu) {
            Some(l) => {
                l.push(it);
                1
            }
            None => 0,
        },
    }
}
unsafe extern "win64" fn insert_menu_item_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const u8,
) -> i32 {
    insert_item(menu, item, by_pos, info, false)
}
unsafe extern "win64" fn insert_menu_item_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const u8,
) -> i32 {
    insert_item(menu, item, by_pos, info, true)
}
unsafe fn set_item_info(menu: usize, item: u32, by_pos: i32, info: *const u8, wide: bool) -> i32 {
    if info.is_null() {
        return 0;
    }
    let mut m = lock(menus());
    let Some((owner, i)) = locate(&m, menu, item, by_pos != 0) else {
        return 0;
    };
    match m.menus.get_mut(&owner).and_then(|l| l.get_mut(i)) {
        Some(it) => {
            apply_info(it, info, wide);
            1
        }
        None => 0,
    }
}
unsafe extern "win64" fn set_menu_item_info_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const u8,
) -> i32 {
    set_item_info(menu, item, by_pos, info, false)
}
unsafe extern "win64" fn set_menu_item_info_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const u8,
) -> i32 {
    set_item_info(menu, item, by_pos, info, true)
}
unsafe fn get_item_info(menu: usize, item: u32, by_pos: i32, info: *mut u8, wide: bool) -> i32 {
    if info.is_null() {
        return 0;
    }
    let it = {
        let m = lock(menus());
        let Some((owner, i)) = locate(&m, menu, item, by_pos != 0) else {
            return 0;
        };
        m.menus[&owner][i].clone()
    };
    let mask: u32 = rd(info, 4);
    if mask & (MIIM_FTYPE | MIIM_TYPE) != 0 {
        wr::<u32>(info, 8, it.flags & TYPE_BITS);
    }
    if mask & MIIM_STATE != 0 {
        wr::<u32>(info, 12, it.flags & STATE_BITS);
    }
    if mask & MIIM_ID != 0 {
        wr::<u32>(info, 16, it.id);
    }
    if mask & MIIM_SUBMENU != 0 {
        wr::<usize>(info, 24, it.submenu);
    }
    if mask & MIIM_DATA != 0 {
        wr::<usize>(info, 48, it.data);
    }
    if mask & (MIIM_STRING | MIIM_TYPE) != 0 {
        let p: usize = rd(info, 56);
        let cap: u32 = rd(info, 64);
        let n = if wide {
            it.text.encode_utf16().count()
        } else {
            it.text.len()
        };
        if p != 0 && cap > 0 {
            if wide {
                super::put_text_w(&it.text, p as *mut u16, cap);
            } else {
                super::put_text_a(&it.text, p as *mut u8, cap);
            }
        }
        wr::<u32>(info, 64, n as u32);
    }
    1
}
unsafe extern "win64" fn get_menu_item_info_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *mut u8,
) -> i32 {
    get_item_info(menu, item, by_pos, info, false)
}
unsafe extern "win64" fn get_menu_item_info_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *mut u8,
) -> i32 {
    get_item_info(menu, item, by_pos, info, true)
}
extern "win64" fn get_menu(hwnd: usize) -> usize {
    lock(menus()).window_menu.get(&hwnd).copied().unwrap_or(0)
}
extern "win64" fn set_menu(hwnd: usize, menu: usize) -> i32 {
    {
        let mut m = lock(menus());
        if menu == 0 {
            m.window_menu.remove(&hwnd);
        } else {
            m.window_menu.insert(hwnd, menu);
        }
    }
    sync_bar(hwnd, None);
    1
}
/// DrawMenuBar : la barre suit les menus ajoutés, renommés ou grisés.
extern "win64" fn draw_menu_bar(hwnd: usize) -> i32 {
    if !crate::dlls::gui::is_window(hwnd) {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    sync_bar(hwnd, None);
    1
}

// ---------- Barre de menus d'une fenêtre ----------

/// Hauteur de la barre de menus (SM_CYMENU à 96 ppp).
pub const BAR_HEIGHT: i32 = 20;
/// Marge de part et d'autre d'un libellé de la barre.
const BAR_PAD: i32 = 7;
const WS_CHILD: u32 = 0x4000_0000;
const WM_INITMENU: u32 = 0x0116;

fn bars() -> &'static Mutex<HashMap<usize, Vec<crate::dlls::gui::MenuBarItem>>> {
    static B: OnceLock<Mutex<HashMap<usize, Vec<crate::dlls::gui::MenuBarItem>>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Police de la barre : celle des menus surgissants (DEFAULT_GUI_FONT).
pub fn bar_font() -> crate::dlls::gui::FontSpec {
    crate::dlls::gui::font_spec(crate::dlls::gdi32::get_stock_object(17)).unwrap_or_default()
}

/// Place les libellés du menu de premier niveau de gauche à droite.
fn layout_bar(menu: usize) -> Vec<crate::dlls::gui::MenuBarItem> {
    let font = bar_font();
    let mut x = 0;
    items(menu)
        .unwrap_or_default()
        .iter()
        .filter(|it| it.flags & MF_SEPARATOR == 0)
        .map(|it| {
            let label =
                crate::dlls::controls::strip_prefix(it.text.split('\t').next().unwrap_or(""));
            let (w, _) = crate::dlls::gui::measure_text(&font, &label);
            let item = crate::dlls::gui::MenuBarItem {
                label,
                left: x,
                right: x + w + BAR_PAD * 2,
                enabled: it.flags & (MF_GRAYED | MF_DISABLED) == 0,
            };
            x = item.right;
            item
        })
        .collect()
}

/// Recalcule la barre d'une fenêtre de premier niveau et la transmet à
/// l'affichage ; `hot` surligne le menu ouvert. La zone client commence
/// sous la barre (GetClientRect, WM_SIZE, coordonnées des enfants).
pub fn sync_bar(hwnd: usize, hot: Option<usize>) {
    use crate::dlls::gui;
    let child = gui::window_style(hwnd).is_some_and(|s| s & WS_CHILD != 0)
        && gui::window_parent(hwnd).is_some_and(|p| p != 0);
    let menu = lock(menus()).window_menu.get(&hwnd).copied().unwrap_or(0);
    let (list, height) = if menu != 0 && !child {
        (layout_bar(menu), BAR_HEIGHT)
    } else {
        (Vec::new(), 0)
    };
    let Some(old) = gui::set_menu_bar_height(hwnd, height) else {
        return;
    };
    if list.is_empty() {
        lock(bars()).remove(&hwnd);
    } else {
        lock(bars()).insert(hwnd, list.clone());
    }
    gui::with_backend_public(|b| b.set_menu_bar(hwnd, &list, height, hot));
    if old != height {
        if let Some(c) = gui::window_client_rect(hwnd) {
            let lparam = ((c.height() as u16 as isize) << 16) | (c.width() as u16 as isize);
            gui::post_to_window(hwnd, gui::Msg::new(hwnd, gui::WM_SIZE, 0, lparam));
        }
    }
}

/// Menu de la barre sous un point écran, par position (None hors barre).
pub fn bar_hit(hwnd: usize, screen_x: i32, screen_y: i32) -> Option<usize> {
    use crate::dlls::gui;
    let r = gui::screen_rect(hwnd)?;
    let (x, y) = (screen_x - r.left, screen_y - r.top);
    if y < 0 || y >= gui::menu_bar_height(hwnd) {
        return None;
    }
    lock(bars())
        .get(&hwnd)?
        .iter()
        .position(|it| x >= it.left && x < it.right)
}

/// Clic dans la barre (WM_NCLBUTTONDOWN, HTMENU) : ouvre le sous-menu sous
/// son titre ; le choix arrive en WM_COMMAND comme sous Windows.
pub fn open_bar_menu(hwnd: usize, index: usize) {
    use crate::dlls::gui;
    let menu = lock(menus()).window_menu.get(&hwnd).copied().unwrap_or(0);
    let entries: Vec<Item> = items(menu)
        .unwrap_or_default()
        .into_iter()
        .filter(|it| it.flags & MF_SEPARATOR == 0)
        .collect();
    let Some(entry) = entries.get(index).cloned() else {
        return;
    };
    if entry.flags & (MF_GRAYED | MF_DISABLED) != 0 {
        return;
    }
    let Some(slot) = lock(bars()).get(&hwnd).and_then(|b| b.get(index).cloned()) else {
        return;
    };
    let Some(r) = gui::screen_rect(hwnd) else {
        return;
    };
    unsafe {
        crate::dlls::user32::send_message_w(hwnd, WM_INITMENU, menu, 0);
    }
    if entry.flags & MF_POPUP == 0 {
        // Commande directement dans la barre (« Aide » sans sous-menu).
        gui::post_to_window(hwnd, gui::Msg::new(hwnd, 0x0111, entry.id as usize, 0));
        return;
    }
    sync_bar(hwnd, Some(index));
    unsafe {
        crate::dlls::controls::popup::track(
            entry.submenu,
            0,
            r.left + slot.left,
            r.top + gui::menu_bar_height(hwnd),
            hwnd,
        );
    }
    sync_bar(hwnd, None);
}
/// Menu système d'une fenêtre (Restaurer, Déplacer… Fermer), créé à la
/// demande ; les programmes y ajoutent leurs propres commandes.
extern "win64" fn get_system_menu(hwnd: usize, revert: i32) -> usize {
    if revert != 0 {
        let old = lock(menus()).system_menu.remove(&hwnd);
        if let Some(h) = old {
            let mut m = lock(menus());
            destroy_tree(&mut m, h);
        }
        return 0;
    }
    if let Some(h) = lock(menus()).system_menu.get(&hwnd).copied() {
        return h;
    }
    let h = create();
    for (id, text) in system_menu_items() {
        if id == 0 {
            append(h, MF_SEPARATOR, 0, None);
        } else {
            append(h, 0, id as usize, Some(text.to_string()));
        }
    }
    lock(menus()).system_menu.insert(hwnd, h);
    h
}
extern "win64" fn get_menu_default_item(menu: usize, by_pos: u32, _flags: u32) -> u32 {
    let m = lock(menus());
    m.menus
        .get(&menu)
        .and_then(|l| {
            l.iter()
                .enumerate()
                .find(|(_, it)| it.flags & MF_DEFAULT != 0)
        })
        .map_or(
            u32::MAX,
            |(i, it)| if by_pos != 0 { i as u32 } else { it.id },
        )
}
extern "win64" fn set_menu_default_item(menu: usize, item: u32, by_pos: u32) -> i32 {
    let mut m = lock(menus());
    let Some(list) = m.menus.get_mut(&menu) else {
        return 0;
    };
    for (i, it) in list.iter_mut().enumerate() {
        let key = if by_pos != 0 { i as u32 } else { it.id };
        if key == item && item != u32::MAX {
            it.flags |= MF_DEFAULT;
        } else {
            it.flags &= !MF_DEFAULT;
        }
    }
    1
}

/// Exports de user32 qui remplacent les versions vides.
pub fn user32_overrides() -> Exports {
    crate::exports! {
        "CreateMenu" => fn create_menu,
        "CreatePopupMenu" => fn create_menu,
        "DestroyMenu" => fn destroy_menu,
        "IsMenu" => fn is_menu,
        "AppendMenuA" => fn append_menu_a,
        "AppendMenuW" => fn append_menu_w,
        "InsertMenuA" => fn insert_menu_a,
        "InsertMenuW" => fn insert_menu_w,
        "ModifyMenuA" => fn modify_menu_a,
        "ModifyMenuW" => fn modify_menu_w,
        "DeleteMenu" => fn delete_menu,
        "RemoveMenu" => fn remove_menu,
        "GetMenuItemCount" => fn get_menu_item_count,
        "GetMenuItemID" => fn get_menu_item_id,
        "GetSubMenu" => fn get_sub_menu,
        "GetMenuState" => fn get_menu_state,
        "CheckMenuItem" => fn check_menu_item,
        "EnableMenuItem" => fn enable_menu_item,
        "CheckMenuRadioItem" => fn check_menu_radio_item,
        "GetMenuStringA" => fn get_menu_string_a,
        "GetMenuStringW" => fn get_menu_string_w,
        "InsertMenuItemA" => fn insert_menu_item_a,
        "InsertMenuItemW" => fn insert_menu_item_w,
        "SetMenuItemInfoA" => fn set_menu_item_info_a,
        "SetMenuItemInfoW" => fn set_menu_item_info_w,
        "GetMenuItemInfoA" => fn get_menu_item_info_a,
        "GetMenuItemInfoW" => fn get_menu_item_info_w,
        "GetMenu" => fn get_menu,
        "SetMenu" => fn set_menu,
        "DrawMenuBar" => fn draw_menu_bar,
        "GetSystemMenu" => fn get_system_menu,
        "GetMenuDefaultItem" => fn get_menu_default_item,
        "SetMenuDefaultItem" => fn set_menu_default_item,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vider_un_menu_par_deletemenu_se_termine() {
        let m = create();
        append(m, 0, 10, Some("Nouvelle session".into()));
        append(m, MF_SEPARATOR, 0, None);
        append(m, 0, 11, Some("Copier tout".into()));
        let mut n = 0;
        while delete_menu(m, 0, MF_BYPOSITION) != 0 {
            n += 1;
            assert!(n < 10, "boucle sans fin");
        }
        assert_eq!(n, 3);
        assert_eq!(get_menu_item_count(m), 0);
        assert_eq!(delete_menu(m, 0, MF_BYPOSITION), 0);
        assert_eq!(get_menu_item_count(0xDEAD), -1);
    }

    #[test]
    fn sous_menus_commandes_et_etats() {
        let root = create();
        let sub = create();
        append(sub, 0, 21, Some("Copier".into()));
        append(root, MF_POPUP, sub, Some("Édition".into()));
        append(root, 0, 30, Some("Quitter".into()));
        assert_eq!(get_sub_menu(root, 0), sub);
        assert_eq!(get_menu_item_id(root, 0), u32::MAX);
        assert_eq!(get_menu_item_id(root, 1), 30);
        // Par commande, cherché aussi dans le sous-menu.
        assert_eq!(check_menu_item(root, 21, MF_CHECKED), 0);
        assert_eq!(get_menu_state(root, 21, 0) & MF_CHECKED, MF_CHECKED);
        assert_eq!(enable_menu_item(root, 30, MF_GRAYED), 0);
        assert_eq!(get_menu_state(root, 30, 0) & MF_GRAYED, MF_GRAYED);
        assert_eq!(check_menu_item(root, 99, MF_CHECKED), u32::MAX);
        assert!(insert(root, 30, 0, 29, Some("Avant".into())));
        assert_eq!(get_menu_item_id(root, 1), 29);
        // Supprimer le sous-menu par position le détruit.
        assert_eq!(delete_menu(root, 0, MF_BYPOSITION), 1);
        assert_eq!(is_menu(sub), 0);
    }

    /// Notepad++ traduit les titres de sa barre par ModifyMenu sans
    /// MF_POPUP : chaque titre doit garder son sous-menu (#709).
    #[test]
    fn modifier_le_titre_d_un_menu_garde_son_sous_menu() {
        let root = create();
        let sub = create();
        append(sub, 0, 21, Some("Copy".into()));
        append(root, MF_POPUP, sub, Some("&Edit".into()));
        let fr: Vec<u16> = "&Édition\0".encode_utf16().collect();
        assert_eq!(
            unsafe { modify_menu_w(root, 0, MF_BYPOSITION, 0, fr.as_ptr()) },
            1
        );
        assert_eq!(get_sub_menu(root, 0), sub);
        assert_eq!(get_menu_item_id(root, 0), u32::MAX);
        let mut buf = [0u16; 32];
        let n = unsafe { get_menu_string_w(root, 0, buf.as_mut_ptr(), 32, MF_BYPOSITION) };
        assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "&Édition");

        // Par commande, l'ordre des éléments décide : le « + » de la barre
        // (même commande que Fichier > Nouveau) vient après.
        append(root, 0, 21, Some("+".into()));
        let copie: Vec<u16> = "Copier\0".encode_utf16().collect();
        assert_eq!(unsafe { modify_menu_w(root, 21, 0, 21, copie.as_ptr()) }, 1);
        let n = unsafe { get_menu_string_w(sub, 0, buf.as_mut_ptr(), 32, MF_BYPOSITION) };
        assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Copier");
        let n = unsafe { get_menu_string_w(root, 1, buf.as_mut_ptr(), 32, MF_BYPOSITION) };
        assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "+");
    }

    #[test]
    fn menu_systeme_par_fenetre() {
        let h = get_system_menu(0x7777, 0);
        assert!(h != 0);
        assert_eq!(get_system_menu(0x7777, 0), h);
        assert_eq!(get_menu_item_count(h), 7);
        append(h, 0, 0x150, Some("Nouvelle session".into()));
        assert_eq!(get_menu_item_count(h), 8);
        assert_eq!(get_system_menu(0x7777, 1), 0);
        assert_eq!(is_menu(h), 0);
    }
}
