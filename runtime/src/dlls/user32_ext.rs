//! Extensions user32 : fonctions nécessaires aux vraies applications GUI.
//!
//! GetWindowLong / SetWindowLong, menus, timers, curseurs, LoadImage,
//! DrawText, scrollbars, clipboard, AdjustWindowRect, dialogues, etc.

use std::collections::HashMap;
use std::sync::Mutex;

// ── Window Long (GWL_STYLE, GWL_EXSTYLE, GWL_USERDATA, DWLP…) ──

static WINDOW_LONGS: Mutex<Option<HashMap<(usize, i32), isize>>> = Mutex::new(None);

fn wl() -> std::sync::MutexGuard<'static, Option<HashMap<(usize, i32), isize>>> {
    let mut g = WINDOW_LONGS.lock().unwrap();
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    g
}

pub unsafe extern "win64" fn get_window_long_a(hwnd: usize, index: i32) -> isize {
    if hwnd == 0 {
        return 0;
    }
    // Pour GWL_STYLE (-16), retourner le style courant du gui module.
    if index == -16 {
        // GWL_STYLE
        return crate::dlls::gui::get_window_style(hwnd) as isize;
    }
    wl().as_ref()
        .and_then(|m| m.get(&(hwnd, index)).copied())
        .unwrap_or(0)
}

pub unsafe extern "win64" fn get_window_long_w(hwnd: usize, index: i32) -> isize {
    get_window_long_a(hwnd, index)
}

pub unsafe extern "win64" fn set_window_long_a(hwnd: usize, index: i32, value: isize) -> isize {
    if hwnd == 0 {
        return 0;
    }
    let old = get_window_long_a(hwnd, index);
    if index == -16 {
        // GWL_STYLE
        crate::dlls::gui::set_window_style(hwnd, value as u32);
    }
    wl().as_mut().unwrap().insert((hwnd, index), value);
    old
}

pub unsafe extern "win64" fn set_window_long_w(hwnd: usize, index: i32, value: isize) -> isize {
    set_window_long_a(hwnd, index, value)
}

// Pointeur étendu (GWLP_ pour 64 bits)
pub unsafe extern "win64" fn get_window_long_ptr_a(hwnd: usize, index: i32) -> isize {
    get_window_long_a(hwnd, index)
}
pub unsafe extern "win64" fn get_window_long_ptr_w(hwnd: usize, index: i32) -> isize {
    get_window_long_a(hwnd, index)
}
pub unsafe extern "win64" fn set_window_long_ptr_a(hwnd: usize, index: i32, value: isize) -> isize {
    set_window_long_a(hwnd, index, value)
}
pub unsafe extern "win64" fn set_window_long_ptr_w(hwnd: usize, index: i32, value: isize) -> isize {
    set_window_long_a(hwnd, index, value)
}

// ── Curseurs ────────────────────────────────────────────────────────

static mut CURRENT_CURSOR: usize = 1; // factice non nul

pub extern "win64" fn load_cursor_a(_hinst: usize, _name: *const u8) -> usize {
    1
}
pub extern "win64" fn load_cursor_w(_hinst: usize, _name: *const u16) -> usize {
    1
}
pub unsafe extern "win64" fn set_cursor(cursor: usize) -> usize {
    let old = CURRENT_CURSOR;
    CURRENT_CURSOR = cursor;
    old
}
pub unsafe extern "win64" fn get_cursor() -> usize {
    CURRENT_CURSOR
}
pub unsafe extern "win64" fn get_cursor_pos(pt: *mut [i32; 2]) -> i32 {
    if !pt.is_null() {
        (*pt) = [0, 0];
    }
    1
}
pub unsafe extern "win64" fn set_cursor_pos(_x: i32, _y: i32) -> i32 {
    1
}
pub extern "win64" fn show_cursor(_show: i32) -> i32 {
    1
}
pub extern "win64" fn clip_cursor(_rect: usize) -> i32 {
    1
}

// ── Icônes et images ────────────────────────────────────────────────

pub extern "win64" fn load_icon_a(_hinst: usize, _name: *const u8) -> usize {
    1
}
pub extern "win64" fn load_icon_w(_hinst: usize, _name: *const u16) -> usize {
    1
}
pub extern "win64" fn load_image_a(
    _hinst: usize,
    _name: *const u8,
    _ty: u32,
    _cx: i32,
    _cy: i32,
    _flags: u32,
) -> usize {
    0
}
pub extern "win64" fn load_image_w(
    _hinst: usize,
    _name: *const u16,
    _ty: u32,
    _cx: i32,
    _cy: i32,
    _flags: u32,
) -> usize {
    0
}
pub extern "win64" fn destroy_icon(_icon: usize) -> i32 {
    1
}
pub extern "win64" fn destroy_cursor(_cursor: usize) -> i32 {
    1
}
pub extern "win64" fn copy_icon(_icon: usize) -> usize {
    _icon
}

// ── Timers ──────────────────────────────────────────────────────────
// SetTimer/KillTimer sont fournis par user32.rs (gui::set_timer), qui
// déclenche réellement WM_TIMER. Une ancienne version bouchon de ce fichier
// les remplaçait et empêchait tous les minuteurs des applications de sonner.

// ── Menus ───────────────────────────────────────────────────────────

static MENU_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0xA000);

pub extern "win64" fn create_menu() -> usize {
    MENU_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
pub extern "win64" fn create_popup_menu() -> usize {
    MENU_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
pub extern "win64" fn destroy_menu(_hmenu: usize) -> i32 {
    1
}
pub extern "win64" fn get_menu(_hwnd: usize) -> usize {
    0
}
pub extern "win64" fn set_menu(_hwnd: usize, _hmenu: usize) -> i32 {
    1
}
pub extern "win64" fn draw_menu_bar(_hwnd: usize) -> i32 {
    1
}
pub unsafe extern "win64" fn append_menu_a(
    _hmenu: usize,
    _flags: u32,
    _id: usize,
    _text: *const u8,
) -> i32 {
    1
}
pub unsafe extern "win64" fn append_menu_w(
    _hmenu: usize,
    _flags: u32,
    _id: usize,
    _text: *const u16,
) -> i32 {
    1
}
pub unsafe extern "win64" fn insert_menu_a(
    _hmenu: usize,
    _pos: u32,
    _flags: u32,
    _id: usize,
    _text: *const u8,
) -> i32 {
    1
}
pub unsafe extern "win64" fn insert_menu_w(
    _hmenu: usize,
    _pos: u32,
    _flags: u32,
    _id: usize,
    _text: *const u16,
) -> i32 {
    1
}
pub unsafe extern "win64" fn insert_menu_item_a(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: usize,
) -> i32 {
    1
}
pub unsafe extern "win64" fn insert_menu_item_w(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: usize,
) -> i32 {
    1
}
pub extern "win64" fn delete_menu(_hmenu: usize, _pos: u32, _flags: u32) -> i32 {
    1
}
pub extern "win64" fn remove_menu(_hmenu: usize, _pos: u32, _flags: u32) -> i32 {
    1
}
pub extern "win64" fn enable_menu_item(_hmenu: usize, _id: u32, _enable: u32) -> i32 {
    0
}
pub extern "win64" fn check_menu_item(_hmenu: usize, _id: u32, _check: u32) -> u32 {
    0
}
pub extern "win64" fn check_menu_radio_item(
    _hmenu: usize,
    _first: u32,
    _last: u32,
    _check: u32,
    _flags: u32,
) -> i32 {
    1
}
pub extern "win64" fn get_sub_menu(_hmenu: usize, _pos: i32) -> usize {
    0
}
pub extern "win64" fn get_menu_item_count(_hmenu: usize) -> i32 {
    0
}
pub unsafe extern "win64" fn get_menu_item_info_a(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: *mut u8,
) -> i32 {
    0
}
pub unsafe extern "win64" fn get_menu_item_info_w(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: *mut u8,
) -> i32 {
    0
}
pub unsafe extern "win64" fn set_menu_item_info_a(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: *const u8,
) -> i32 {
    1
}
pub unsafe extern "win64" fn set_menu_item_info_w(
    _hmenu: usize,
    _item: u32,
    _by_pos: i32,
    _info: *const u8,
) -> i32 {
    1
}
pub unsafe extern "win64" fn track_popup_menu(
    _hmenu: usize,
    _flags: u32,
    _x: i32,
    _y: i32,
    _reserved: i32,
    _hwnd: usize,
    _rect: usize,
) -> i32 {
    0
}
pub unsafe extern "win64" fn track_popup_menu_ex(
    _hmenu: usize,
    _flags: u32,
    _x: i32,
    _y: i32,
    _hwnd: usize,
    _params: usize,
) -> i32 {
    0
}
pub extern "win64" fn get_system_menu(_hwnd: usize, _revert: i32) -> usize {
    0
}

// ── DrawText ────────────────────────────────────────────────────────

pub unsafe extern "win64" fn draw_text_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    rect: *mut [i32; 4],
    _format: u32,
) -> i32 {
    if text.is_null() || rect.is_null() {
        return 0;
    }
    let n = if count < 0 {
        crate::printf::narrow_len(text) as i32
    } else {
        count
    };
    // Déléguer à TextOutA au point haut-gauche du rectangle.
    if n > 0 {
        crate::dlls::gdi32::text_out_a(hdc, (*rect)[0], (*rect)[1], text, n);
    }
    (*rect)[3] - (*rect)[1] // hauteur
}

pub unsafe extern "win64" fn draw_text_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    rect: *mut [i32; 4],
    format: u32,
) -> i32 {
    if text.is_null() || rect.is_null() {
        return 0;
    }
    let n = if count < 0 {
        crate::printf::wide_len(text)
    } else {
        count as usize
    };
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
    let bytes = crate::dlls::encode_ansi(&s);
    draw_text_a(hdc, bytes.as_ptr(), bytes.len() as i32, rect, format)
}

pub unsafe extern "win64" fn draw_text_ex_a(
    hdc: usize,
    text: *mut u8,
    count: i32,
    rect: *mut [i32; 4],
    format: u32,
    _params: usize,
) -> i32 {
    draw_text_a(hdc, text, count, rect, format)
}
pub unsafe extern "win64" fn draw_text_ex_w(
    hdc: usize,
    text: *mut u16,
    count: i32,
    rect: *mut [i32; 4],
    format: u32,
    _params: usize,
) -> i32 {
    draw_text_w(hdc, text, count, rect, format)
}

// ── AdjustWindowRect ────────────────────────────────────────────────

pub unsafe extern "win64" fn adjust_window_rect(
    rect: *mut [i32; 4],
    _style: u32,
    _menu: i32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    // Bordure simple : 8 pixels de chaque côté, 30 pour la barre de titre.
    (*rect)[0] -= 8;
    (*rect)[1] -= 30;
    (*rect)[2] += 8;
    (*rect)[3] += 8;
    1
}

pub unsafe extern "win64" fn adjust_window_rect_ex(
    rect: *mut [i32; 4],
    style: u32,
    menu: i32,
    _ex_style: u32,
) -> i32 {
    adjust_window_rect(rect, style, menu)
}

// ── Coordonnées et hiérarchie de fenêtres ───────────────────────────

pub unsafe extern "win64" fn screen_to_client(hwnd: usize, pt: *mut [i32; 2]) -> i32 {
    if pt.is_null() || hwnd == 0 {
        return 0;
    }
    let (x, y) = crate::dlls::gui::window_origin(hwnd);
    (*pt)[0] -= x;
    (*pt)[1] -= y;
    1
}

pub unsafe extern "win64" fn client_to_screen(hwnd: usize, pt: *mut [i32; 2]) -> i32 {
    if pt.is_null() || hwnd == 0 {
        return 0;
    }
    let (x, y) = crate::dlls::gui::window_origin(hwnd);
    (*pt)[0] += x;
    (*pt)[1] += y;
    1
}

pub unsafe extern "win64" fn map_window_points(
    from: usize,
    to: usize,
    pts: *mut [i32; 2],
    count: u32,
) -> i32 {
    if pts.is_null() || count == 0 {
        return 0;
    }
    let (fx, fy) = if from == 0 {
        (0, 0)
    } else {
        crate::dlls::gui::window_origin(from)
    };
    let (tx, ty) = if to == 0 {
        (0, 0)
    } else {
        crate::dlls::gui::window_origin(to)
    };
    let dx = fx - tx;
    let dy = fy - ty;
    for i in 0..count as usize {
        (*pts.add(i))[0] += dx;
        (*pts.add(i))[1] += dy;
    }
    (((dx as i16 as u16 as u32) << 16) | (dy as i16 as u16 as u32)) as i32
}

pub extern "win64" fn get_parent(_hwnd: usize) -> usize {
    0
}
pub extern "win64" fn set_parent(_child: usize, _parent: usize) -> usize {
    0
}
pub extern "win64" fn get_window(_hwnd: usize, _cmd: u32) -> usize {
    0
}
pub extern "win64" fn find_window_a(_class: *const u8, _title: *const u8) -> usize {
    0
}
pub extern "win64" fn find_window_w(_class: *const u16, _title: *const u16) -> usize {
    0
}
pub extern "win64" fn find_window_ex_a(
    _parent: usize,
    _after: usize,
    _class: *const u8,
    _title: *const u8,
) -> usize {
    0
}
pub extern "win64" fn find_window_ex_w(
    _parent: usize,
    _after: usize,
    _class: *const u16,
    _title: *const u16,
) -> usize {
    0
}
pub extern "win64" fn is_window_visible(_hwnd: usize) -> i32 {
    1
}
pub extern "win64" fn is_window_enabled(_hwnd: usize) -> i32 {
    1
}
pub extern "win64" fn enable_window(_hwnd: usize, _enable: i32) -> i32 {
    0
}
pub extern "win64" fn set_active_window(_hwnd: usize) -> usize {
    0
}
pub extern "win64" fn get_active_window() -> usize {
    0
}
pub extern "win64" fn bring_window_to_top(_hwnd: usize) -> i32 {
    1
}
pub extern "win64" fn set_foreground_window(_hwnd: usize) -> i32 {
    1
}

// ── Capture souris ──────────────────────────────────────────────────

static mut CAPTURE_HWND: usize = 0;

pub unsafe extern "win64" fn set_capture(hwnd: usize) -> usize {
    let old = CAPTURE_HWND;
    CAPTURE_HWND = hwnd;
    old
}
pub unsafe extern "win64" fn release_capture() -> i32 {
    CAPTURE_HWND = 0;
    1
}
pub unsafe extern "win64" fn get_capture() -> usize {
    CAPTURE_HWND
}

// ── Scrollbars ──────────────────────────────────────────────────────

pub unsafe extern "win64" fn set_scroll_info(
    _hwnd: usize,
    _bar: i32,
    _info: *const u8,
    _redraw: i32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn get_scroll_info(_hwnd: usize, _bar: i32, _info: *mut u8) -> i32 {
    0
}
pub unsafe extern "win64" fn set_scroll_pos(
    _hwnd: usize,
    _bar: i32,
    _pos: i32,
    _redraw: i32,
) -> i32 {
    0
}
pub unsafe extern "win64" fn get_scroll_pos(_hwnd: usize, _bar: i32) -> i32 {
    0
}
pub unsafe extern "win64" fn set_scroll_range(
    _hwnd: usize,
    _bar: i32,
    _min: i32,
    _max: i32,
    _redraw: i32,
) -> i32 {
    1
}
pub extern "win64" fn scroll_window(
    _hwnd: usize,
    _dx: i32,
    _dy: i32,
    _rect: usize,
    _clip: usize,
) -> i32 {
    1
}
pub extern "win64" fn scroll_window_ex(
    _hwnd: usize,
    _dx: i32,
    _dy: i32,
    _scroll: usize,
    _clip: usize,
    _rgn: usize,
    _update: usize,
    _flags: u32,
) -> i32 {
    1
}
pub extern "win64" fn show_scroll_bar(_hwnd: usize, _bar: i32, _show: i32) -> i32 {
    1
}
pub extern "win64" fn enable_scroll_bar(_hwnd: usize, _flags: u32, _arrows: u32) -> i32 {
    1
}

// ── Accélérateurs ───────────────────────────────────────────────────

pub extern "win64" fn load_accelerators_a(_hinst: usize, _name: *const u8) -> usize {
    0
}
pub extern "win64" fn load_accelerators_w(_hinst: usize, _name: *const u16) -> usize {
    0
}
pub extern "win64" fn create_accelerator_table_a(_accel: usize, _count: i32) -> usize {
    0
}
pub extern "win64" fn create_accelerator_table_w(_accel: usize, _count: i32) -> usize {
    0
}
pub extern "win64" fn destroy_accelerator_table(_table: usize) -> i32 {
    1
}
pub unsafe extern "win64" fn translate_accelerator_a(
    _hwnd: usize,
    _table: usize,
    _msg: *const u8,
) -> i32 {
    0
}
pub unsafe extern "win64" fn translate_accelerator_w(
    _hwnd: usize,
    _table: usize,
    _msg: *const u8,
) -> i32 {
    0
}

// ── Dialogues ───────────────────────────────────────────────────────

pub extern "win64" fn dialog_box_param_a(
    _inst: usize,
    _tmpl: *const u8,
    _parent: usize,
    _proc: usize,
    _param: isize,
) -> isize {
    -1
}
pub extern "win64" fn dialog_box_param_w(
    _inst: usize,
    _tmpl: *const u16,
    _parent: usize,
    _proc: usize,
    _param: isize,
) -> isize {
    -1
}
pub extern "win64" fn create_dialog_param_a(
    _inst: usize,
    _tmpl: *const u8,
    _parent: usize,
    _proc: usize,
    _param: isize,
) -> usize {
    0
}
pub extern "win64" fn create_dialog_param_w(
    _inst: usize,
    _tmpl: *const u16,
    _parent: usize,
    _proc: usize,
    _param: isize,
) -> usize {
    0
}
pub extern "win64" fn end_dialog(_dlg: usize, _result: isize) -> i32 {
    1
}
pub extern "win64" fn is_dialog_message_a(_dlg: usize, _msg: usize) -> i32 {
    0
}
pub extern "win64" fn is_dialog_message_w(_dlg: usize, _msg: usize) -> i32 {
    0
}
pub extern "win64" fn get_dlg_item(_dlg: usize, _id: i32) -> usize {
    0
}
pub unsafe extern "win64" fn get_dlg_item_text_a(
    _dlg: usize,
    _id: i32,
    _buf: *mut u8,
    _max: i32,
) -> u32 {
    0
}
pub extern "win64" fn set_dlg_item_text_a(_dlg: usize, _id: i32, _text: *const u8) -> i32 {
    1
}
pub extern "win64" fn set_dlg_item_text_w(_dlg: usize, _id: i32, _text: *const u16) -> i32 {
    1
}
pub unsafe extern "win64" fn send_dlg_item_message_a(
    _dlg: usize,
    _id: i32,
    _msg: u32,
    _wp: usize,
    _lp: isize,
) -> isize {
    0
}
pub unsafe extern "win64" fn send_dlg_item_message_w(
    _dlg: usize,
    _id: i32,
    _msg: u32,
    _wp: usize,
    _lp: isize,
) -> isize {
    0
}
pub extern "win64" fn check_dlg_button(_dlg: usize, _id: i32, _check: u32) -> i32 {
    1
}
pub extern "win64" fn is_dlg_button_checked(_dlg: usize, _id: i32) -> u32 {
    0
}

// ── Divers ──────────────────────────────────────────────────────────

pub extern "win64" fn get_class_name_a(_hwnd: usize, _buf: *mut u8, _max: i32) -> i32 {
    0
}
pub extern "win64" fn get_class_name_w(_hwnd: usize, _buf: *mut u16, _max: i32) -> i32 {
    0
}
pub extern "win64" fn get_class_long_a(_hwnd: usize, _index: i32) -> usize {
    0
}
pub extern "win64" fn get_class_long_w(_hwnd: usize, _index: i32) -> usize {
    0
}
pub extern "win64" fn get_class_long_ptr_a(_hwnd: usize, _index: i32) -> usize {
    0
}
pub extern "win64" fn get_class_long_ptr_w(_hwnd: usize, _index: i32) -> usize {
    0
}
pub extern "win64" fn set_class_long_ptr_a(_hwnd: usize, _index: i32, _new: isize) -> usize {
    0
}
pub extern "win64" fn set_class_long_ptr_w(_hwnd: usize, _index: i32, _new: isize) -> usize {
    0
}
pub extern "win64" fn enum_windows(_callback: usize, _lparam: isize) -> i32 {
    1
}
pub extern "win64" fn enum_child_windows(_parent: usize, _callback: usize, _lparam: isize) -> i32 {
    1
}
pub extern "win64" fn get_prop_a(_hwnd: usize, _name: *const u8) -> usize {
    0
}
pub extern "win64" fn get_prop_w(_hwnd: usize, _name: *const u16) -> usize {
    0
}
pub extern "win64" fn set_prop_a(_hwnd: usize, _name: *const u8, _data: usize) -> i32 {
    1
}
pub extern "win64" fn set_prop_w(_hwnd: usize, _name: *const u16, _data: usize) -> i32 {
    1
}
pub extern "win64" fn remove_prop_a(_hwnd: usize, _name: *const u8) -> usize {
    0
}
pub extern "win64" fn remove_prop_w(_hwnd: usize, _name: *const u16) -> usize {
    0
}
pub extern "win64" fn get_dc_ex(_hwnd: usize, _rgn: usize, _flags: u32) -> usize {
    crate::dlls::user32::get_dc(_hwnd)
}
pub extern "win64" fn redraw_window(_hwnd: usize, _rect: usize, _rgn: usize, _flags: u32) -> i32 {
    unsafe { crate::dlls::user32::invalidate_rect(_hwnd, std::ptr::null(), 1) };
    1
}
pub extern "win64" fn get_update_rect(_hwnd: usize, _rect: usize, _erase: i32) -> i32 {
    0
}
pub extern "win64" fn map_virtual_key_a(_code: u32, _map_type: u32) -> u32 {
    0
}
pub extern "win64" fn map_virtual_key_w(_code: u32, _map_type: u32) -> u32 {
    0
}
pub extern "win64" fn to_unicode_ex(
    _vk: u32,
    _scan: u32,
    _state: *const u8,
    _buf: *mut u16,
    _size: i32,
    _flags: u32,
    _layout: usize,
) -> i32 {
    0
}
pub extern "win64" fn register_window_message_a(_msg: *const u8) -> u32 {
    0xC001
}
pub extern "win64" fn register_window_message_w(_msg: *const u16) -> u32 {
    0xC001
}
pub extern "win64" fn reply_message(_result: isize) -> i32 {
    1
}
pub extern "win64" fn wait_message() -> i32 {
    1
}
pub extern "win64" fn msg_wait_for_multiple_objects(
    _count: u32,
    _handles: usize,
    _all: i32,
    _ms: u32,
    _mask: u32,
) -> u32 {
    0
}
pub extern "win64" fn msg_wait_for_multiple_objects_ex(
    _count: u32,
    _handles: usize,
    _ms: u32,
    _mask: u32,
    _flags: u32,
) -> u32 {
    0
}
pub extern "win64" fn get_window_dc(_hwnd: usize) -> usize {
    crate::dlls::user32::get_dc(_hwnd)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetWindowLongA"=>fn get_window_long_a, "GetWindowLongW"=>fn get_window_long_w,
        "SetWindowLongA"=>fn set_window_long_a, "SetWindowLongW"=>fn set_window_long_w,
        "GetWindowLongPtrA"=>fn get_window_long_ptr_a, "GetWindowLongPtrW"=>fn get_window_long_ptr_w,
        "SetWindowLongPtrA"=>fn set_window_long_ptr_a, "SetWindowLongPtrW"=>fn set_window_long_ptr_w,
        "LoadCursorA"=>fn load_cursor_a, "LoadCursorW"=>fn load_cursor_w,
        "SetCursor"=>fn set_cursor, "GetCursor"=>fn get_cursor,
        "GetCursorPos"=>fn get_cursor_pos, "SetCursorPos"=>fn set_cursor_pos,
        "ShowCursor"=>fn show_cursor, "ClipCursor"=>fn clip_cursor,
        "LoadIconA"=>fn load_icon_a, "LoadIconW"=>fn load_icon_w,
        "LoadImageA"=>fn load_image_a, "LoadImageW"=>fn load_image_w,
        "DestroyIcon"=>fn destroy_icon, "DestroyCursor"=>fn destroy_cursor, "CopyIcon"=>fn copy_icon,
        "CreateMenu"=>fn create_menu, "CreatePopupMenu"=>fn create_popup_menu, "DestroyMenu"=>fn destroy_menu,
        "GetMenu"=>fn get_menu, "SetMenu"=>fn set_menu, "DrawMenuBar"=>fn draw_menu_bar,
        "AppendMenuA"=>fn append_menu_a, "AppendMenuW"=>fn append_menu_w,
        "InsertMenuA"=>fn insert_menu_a, "InsertMenuW"=>fn insert_menu_w,
        "InsertMenuItemA"=>fn insert_menu_item_a, "InsertMenuItemW"=>fn insert_menu_item_w,
        "DeleteMenu"=>fn delete_menu, "RemoveMenu"=>fn remove_menu,
        "EnableMenuItem"=>fn enable_menu_item, "CheckMenuItem"=>fn check_menu_item,
        "CheckMenuRadioItem"=>fn check_menu_radio_item,
        "GetSubMenu"=>fn get_sub_menu, "GetMenuItemCount"=>fn get_menu_item_count,
        "GetMenuItemInfoA"=>fn get_menu_item_info_a, "GetMenuItemInfoW"=>fn get_menu_item_info_w,
        "SetMenuItemInfoA"=>fn set_menu_item_info_a, "SetMenuItemInfoW"=>fn set_menu_item_info_w,
        "TrackPopupMenu"=>fn track_popup_menu, "TrackPopupMenuEx"=>fn track_popup_menu_ex,
        "GetSystemMenu"=>fn get_system_menu,
        "DrawTextA"=>fn draw_text_a, "DrawTextW"=>fn draw_text_w,
        "DrawTextExA"=>fn draw_text_ex_a, "DrawTextExW"=>fn draw_text_ex_w,
        "AdjustWindowRect"=>fn adjust_window_rect, "AdjustWindowRectEx"=>fn adjust_window_rect_ex,
        "ScreenToClient"=>fn screen_to_client, "ClientToScreen"=>fn client_to_screen,
        "MapWindowPoints"=>fn map_window_points,
        "GetParent"=>fn get_parent, "SetParent"=>fn set_parent, "GetWindow"=>fn get_window,
        "FindWindowA"=>fn find_window_a, "FindWindowW"=>fn find_window_w,
        "FindWindowExA"=>fn find_window_ex_a, "FindWindowExW"=>fn find_window_ex_w,
        "IsWindowVisible"=>fn is_window_visible, "IsWindowEnabled"=>fn is_window_enabled,
        "EnableWindow"=>fn enable_window,
        "SetActiveWindow"=>fn set_active_window, "GetActiveWindow"=>fn get_active_window,
        "BringWindowToTop"=>fn bring_window_to_top, "SetForegroundWindow"=>fn set_foreground_window,
        "SetCapture"=>fn set_capture, "ReleaseCapture"=>fn release_capture, "GetCapture"=>fn get_capture,
        "SetScrollInfo"=>fn set_scroll_info, "GetScrollInfo"=>fn get_scroll_info,
        "SetScrollPos"=>fn set_scroll_pos, "GetScrollPos"=>fn get_scroll_pos,
        "SetScrollRange"=>fn set_scroll_range,
        "ScrollWindow"=>fn scroll_window, "ScrollWindowEx"=>fn scroll_window_ex,
        "ShowScrollBar"=>fn show_scroll_bar, "EnableScrollBar"=>fn enable_scroll_bar,
        "LoadAcceleratorsA"=>fn load_accelerators_a, "LoadAcceleratorsW"=>fn load_accelerators_w,
        "CreateAcceleratorTableA"=>fn create_accelerator_table_a,
        "CreateAcceleratorTableW"=>fn create_accelerator_table_w,
        "DestroyAcceleratorTable"=>fn destroy_accelerator_table,
        "TranslateAcceleratorA"=>fn translate_accelerator_a,
        "TranslateAcceleratorW"=>fn translate_accelerator_w,
        "DialogBoxParamA"=>fn dialog_box_param_a, "DialogBoxParamW"=>fn dialog_box_param_w,
        "CreateDialogParamA"=>fn create_dialog_param_a, "CreateDialogParamW"=>fn create_dialog_param_w,
        "EndDialog"=>fn end_dialog,
        "IsDialogMessageA"=>fn is_dialog_message_a, "IsDialogMessageW"=>fn is_dialog_message_w,
        "GetDlgItem"=>fn get_dlg_item,
        "GetDlgItemTextA"=>fn get_dlg_item_text_a,
        "SetDlgItemTextA"=>fn set_dlg_item_text_a, "SetDlgItemTextW"=>fn set_dlg_item_text_w,
        "SendDlgItemMessageA"=>fn send_dlg_item_message_a,
        "SendDlgItemMessageW"=>fn send_dlg_item_message_w,
        "CheckDlgButton"=>fn check_dlg_button, "IsDlgButtonChecked"=>fn is_dlg_button_checked,
        "GetClassNameA"=>fn get_class_name_a, "GetClassNameW"=>fn get_class_name_w,
        "GetClassLongA"=>fn get_class_long_a, "GetClassLongW"=>fn get_class_long_w,
        "GetClassLongPtrA"=>fn get_class_long_ptr_a, "GetClassLongPtrW"=>fn get_class_long_ptr_w,
        "SetClassLongPtrA"=>fn set_class_long_ptr_a, "SetClassLongPtrW"=>fn set_class_long_ptr_w,
        "EnumWindows"=>fn enum_windows, "EnumChildWindows"=>fn enum_child_windows,
        "GetPropA"=>fn get_prop_a, "GetPropW"=>fn get_prop_w,
        "SetPropA"=>fn set_prop_a, "SetPropW"=>fn set_prop_w,
        "RemovePropA"=>fn remove_prop_a, "RemovePropW"=>fn remove_prop_w,
        "GetDCEx"=>fn get_dc_ex, "RedrawWindow"=>fn redraw_window, "GetUpdateRect"=>fn get_update_rect,
        "MapVirtualKeyA"=>fn map_virtual_key_a, "MapVirtualKeyW"=>fn map_virtual_key_w,
        "ToUnicodeEx"=>fn to_unicode_ex,
        "RegisterWindowMessageA"=>fn register_window_message_a,
        "RegisterWindowMessageW"=>fn register_window_message_w,
        "ReplyMessage"=>fn reply_message, "WaitMessage"=>fn wait_message,
        "MsgWaitForMultipleObjects"=>fn msg_wait_for_multiple_objects,
        "MsgWaitForMultipleObjectsEx"=>fn msg_wait_for_multiple_objects_ex,
        "MonitorFromPoint"=>fn crate::dlls::monitors::monitor_from_point_packed,
        "MonitorFromRect"=>fn crate::dlls::monitors::monitor_from_rect,
        "MonitorFromWindow"=>fn crate::dlls::monitors::monitor_from_window,
        "GetMonitorInfoA"=>fn crate::dlls::monitors::get_monitor_info_a,
        "GetMonitorInfoW"=>fn crate::dlls::monitors::get_monitor_info_w,
        "GetWindowDC"=>fn get_window_dc,
    }
}
