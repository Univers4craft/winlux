//! API `user32` de Winlux : fenêtres et messages, reliées au noyau `gui`.
//!
//! Les fonctions suivent les conventions Win32 (A/W, `set_last_error`, pointeurs
//! guests). Le rendu est délégué au backend du noyau (headless aujourd'hui).

use crate::dlls::gui::{
    self, ClassInfo, CreateStructA, Msg, PaintStruct, Point, Rect, WndClassA, WndClassExA,
};
use std::ffi::c_void;

// ---------- Utilitaires ----------

fn now_tick() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}

unsafe fn read_class_name_a(p: *const u8) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(p)
    }
}

unsafe fn read_class_name_w(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(p)
    }
}

// ---------- Enregistrement de classes ----------

unsafe fn register_class_common(name: String, wnd_proc: usize, background: usize, instance: usize) -> usize {
    if name.is_empty() || wnd_proc == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if gui::register_class(&name, ClassInfo { wnd_proc, background, instance }) {
        // ATOM : petit entier stable, jamais nul.
        1
    } else {
        // Classe déjà enregistrée : Win32 renvoie une erreur mais garde la classe.
        1
    }
}

pub unsafe extern "win64" fn register_class_a(wc: *const WndClassA) -> u16 {
    if wc.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wc;
    let name = read_class_name_a(c.class_name);
    register_class_common(name, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_w(wc: *const WndClassA) -> u16 {
    // La disposition est identique ; seul le nom de classe change (UTF-16).
    if wc.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wc;
    let name = read_class_name_w(c.class_name as *const u16);
    register_class_common(name, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_ex_a(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_a(c.class_name);
    register_class_common(name, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_ex_w(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_w(c.class_name as *const u16);
    register_class_common(name, c.wnd_proc, c.background, c.instance) as u16
}

// ---------- Création de fenêtres ----------

unsafe fn create_window_common(
    class: String, title: String, style: u32, ex_style: u32,
    x: i32, y: i32, cx: i32, cy: i32, parent: usize, instance: usize,
) -> usize {
    let Some(info) = gui::lookup_class(&class) else {
        crate::process::set_last_error(1407); // ERROR_CLASS_DOES_NOT_EXIST
        return 0;
    };
    let hwnd = gui::alloc_hwnd();
    // Tailles CW_USEDEFAULT : fenêtre raisonnable par défaut.
    let w = if cx == gui::CW_USEDEFAULT || cx <= 0 { 800 } else { cx };
    let h = if cy == gui::CW_USEDEFAULT || cy <= 0 { 600 } else { cy };
    let px = if x == gui::CW_USEDEFAULT { 100 } else { x };
    let py = if y == gui::CW_USEDEFAULT { 100 } else { y };
    let rect = Rect::from_size(px, py, w, h);
    let visible = style & 0x1000_0000 != 0; // WS_VISIBLE

    let cs = CreateStructA {
        create_params: std::ptr::null_mut(),
        instance,
        menu: 0,
        parent,
        cy: h,
        cx: w,
        y: py,
        x: px,
        style: style as i32,
        name: std::ptr::null(),
        class: std::ptr::null(),
        ex_style,
    };

    gui::insert_window(hwnd, gui::Window {
        class: class.clone(),
        wnd_proc: info.wnd_proc,
        background: info.background,
        title: title.clone(),
        rect,
        visible,
        parent,
        owner: std::thread::current().id(),
        needs_paint: visible,
        style,
        ex_style,
    });

    // WM_CREATE synchrone (le wndproc peut refuser la fenêtre).
    let created = crate::dlls::gui::call_wndproc(info.wnd_proc, hwnd, gui::WM_CREATE, 0, &cs as *const CreateStructA as isize);
    if created == -1 {
        gui::destroy_window(hwnd);
        return 0;
    }

    with_backend(|b| b.create_window(hwnd, &title, rect));

    // WM_SIZE puis, si visible, demande de peinture.
    gui::post_to_window(hwnd, Msg {
        hwnd, message: gui::WM_SIZE, wparam: 0, lparam: (((h as u32 as usize) << 16) | (w as u32 as usize)) as isize,
        time: now_tick(), pt: Point::default(), l_private: 0,
    });
    if visible {
        gui::request_paint(hwnd);
    }
    hwnd
}

fn with_backend<F: FnOnce(&mut dyn gui::GuiBackend)>(f: F) {
    // Réexpose le backend du noyau via une petite fonction utilitaire locale.
    gui::with_backend_public(f);
}

pub unsafe extern "win64" fn create_window_ex_a(
    ex_style: u32, class: *const u8, title: *const u8, style: u32,
    x: i32, y: i32, cx: i32, cy: i32, parent: usize, menu: usize, instance: usize, param: *mut c_void,
) -> usize {
    let _ = param;
    let _ = menu;
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    create_window_common(read_class_name_a(class), read_class_name_a(title), style, ex_style, x, y, cx, cy, parent, instance)
}

pub unsafe extern "win64" fn create_window_ex_w(
    ex_style: u32, class: *const u16, title: *const u16, style: u32,
    x: i32, y: i32, cx: i32, cy: i32, parent: usize, menu: usize, instance: usize, param: *mut c_void,
) -> usize {
    let _ = param;
    let _ = menu;
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    create_window_common(read_class_name_w(class), read_class_name_w(title), style, ex_style, x, y, cx, cy, parent, instance)
}

// ---------- Boucle de messages ----------

unsafe fn write_msg(out: *mut Msg, m: &Msg) {
    if !out.is_null() {
        std::ptr::write_unaligned(out, Msg {
            hwnd: m.hwnd, message: m.message, wparam: m.wparam, lparam: m.lparam,
            time: m.time, pt: m.pt, l_private: m.l_private,
        });
    }
}

pub unsafe extern "win64" fn get_message_a(out: *mut Msg, hwnd: usize, min: u32, max: u32) -> i32 {
    gui::pump_backend();
    let q = gui::current_queue();
    // WM_QUIT (posted par PostQuitMessage) termine la boucle (renvoie 0).
    if let Some(m) = gui::take_message(&q, hwnd, min, max) {
        if m.message == gui::WM_QUIT {
            return 0;
        }
        write_msg(out, &m);
        return 1;
    }
    // File vide : en headless, on termine proprement plutôt que de bloquer.
    if !gui::any_visible_window() {
        return 0;
    }
    0
}

pub unsafe extern "win64" fn peek_message_a(out: *mut Msg, hwnd: usize, min: u32, max: u32, remove: u32) -> i32 {
    gui::pump_backend();
    let q = gui::current_queue();
    if remove & 0x1 != 0 {
        if let Some(m) = gui::take_message(&q, hwnd, min, max) {
            if m.message == gui::WM_QUIT {
                // PM_REMOVE sur WM_QUIT : on le remet et on renvoie -1 (fin).
                gui::prepend_message(&q, m);
                return -1;
            }
            write_msg(out, &m);
            return 1;
        }
        return 0;
    }
    if let Some(m) = gui::peek_message(&q, hwnd, min, max) {
        write_msg(out, &m);
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn translate_message(_msg: *const Msg) -> i32 {
    // Pas de synthèse de WM_CHAR pour l'instant.
    1
}

pub unsafe extern "win64" fn dispatch_message_a(msg: *const Msg) -> isize {
    if msg.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let m = &*msg;
    let wnd_proc = gui::wnd_proc_of(m.hwnd);
    match wnd_proc {
        Some(wp) => gui::call_wndproc(wp, m.hwnd, m.message, m.wparam, m.lparam),
        None => gui::def_window_proc(m.hwnd, m.message, m.wparam, m.lparam),
    }
}

pub unsafe extern "win64" fn post_quit_message(code: i32) {
    let q = gui::current_queue();
    gui::post_quit(&q, code);
}

pub unsafe extern "win64" fn post_message_a(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> i32 {
    if hwnd == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let m = Msg { hwnd, message, wparam, lparam, time: now_tick(), pt: Point::default(), l_private: 0 };
    if gui::post_to_window(hwnd, m) { 1 } else { 0 }
}

pub unsafe extern "win64" fn send_message_a(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> isize {
    if hwnd == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    match gui::wnd_proc_of(hwnd) {
        Some(wp) => gui::call_wndproc(wp, hwnd, message, wparam, lparam),
        None => gui::def_window_proc(hwnd, message, wparam, lparam),
    }
}

// ---------- Propriétés de fenêtre ----------

pub unsafe extern "win64" fn def_window_proc_a(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> isize {
    gui::def_window_proc(hwnd, message, wparam, lparam)
}

pub unsafe extern "win64" fn def_window_proc_w(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> isize {
    gui::def_window_proc(hwnd, message, wparam, lparam)
}

pub unsafe extern "win64" fn show_window(hwnd: usize, cmd: i32) -> i32 {
    let was = gui::set_visible(hwnd, cmd != gui::SW_HIDE);
    with_backend(|b| b.show_window(hwnd, cmd != gui::SW_HIDE));
    if cmd != gui::SW_HIDE {
        gui::request_paint(hwnd);
    }
    was as i32
}

pub unsafe extern "win64" fn update_window(hwnd: usize) -> i32 {
    if gui::wnd_proc_of(hwnd).is_some() {
        gui::request_paint(hwnd);
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn destroy_window(hwnd: usize) -> i32 {
    if gui::destroy_window(hwnd) { 1 } else { 0 }
}

pub unsafe extern "win64" fn is_window(hwnd: usize) -> i32 {
    gui::wnd_proc_of(hwnd).is_some() as i32
}

pub unsafe extern "win64" fn get_window_rect(hwnd: usize, out: *mut Rect) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    match gui::window_rect(hwnd) {
        Some(r) => {
            std::ptr::write_unaligned(out, r);
            1
        }
        None => 0,
    }
}

pub unsafe extern "win64" fn get_client_rect(hwnd: usize, out: *mut Rect) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    match gui::window_client_rect(hwnd) {
        Some(r) => {
            std::ptr::write_unaligned(out, r);
            1
        }
        None => 0,
    }
}

pub unsafe extern "win64" fn set_window_text_a(hwnd: usize, text: *const u8) -> i32 {
    let t = read_class_name_a(text);
    if gui::set_title(hwnd, &t) {
        with_backend(|b| b.set_title(hwnd, &t));
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn get_window_text_a(hwnd: usize, buf: *mut u8, cap: i32) -> i32 {
    match gui::title_of(hwnd) {
        Some(t) => crate::dlls::put_a(&t, buf, cap.max(0) as u32) as i32,
        None => 0,
    }
}

pub unsafe extern "win64" fn get_window_text_length_a(hwnd: usize) -> i32 {
    gui::title_of(hwnd).map(|t| t.len() as i32).unwrap_or(0)
}

// ---------- Peinture ----------

pub unsafe extern "win64" fn begin_paint(hwnd: usize, ps: *mut PaintStruct) -> usize {
    if ps.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let (hdc, rect) = match gui::begin_paint(hwnd) {
        Some(v) => v,
        None => return 0,
    };
    std::ptr::write_unaligned(ps, PaintStruct {
        hdc,
        erase: 1,
        paint: rect,
        restore: 0,
        inc_update: 0,
        reserved: [0; 32],
    });
    hdc
}

pub unsafe extern "win64" fn end_paint(hwnd: usize, _ps: *const PaintStruct) -> i32 {
    gui::end_paint(hwnd) as i32
}

pub unsafe extern "win64" fn invalidate_rect(hwnd: usize, _rect: *const Rect, _erase: i32) -> i32 {
    if gui::wnd_proc_of(hwnd).is_some() {
        gui::request_paint(hwnd);
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn validate_rect(_hwnd: usize, _rect: *const Rect) {
    // L'aire est déjà validée par end_paint.
}

// ---------- Divers ----------

pub extern "win64" fn get_desktop_window() -> usize {
    1
}
pub extern "win64" fn get_foreground_window() -> usize {
    gui::first_window().unwrap_or(1)
}

pub extern "win64" fn get_system_metrics(index: i32) -> i32 {
    use crate::dlls::gui::theme;
    let _ = theme::ACCENT; // thème présent et lié
    match index {
        0 => 1920, // SM_CXSCREEN
        1 => 1080, // SM_CYSCREEN
        2 => 4,    // SM_CYCAPTION
        3 => 4,    // SM_CXBORDER
        4 => 4,    // SM_CYBORDER
        7 => 32,   // SM_CYHSCROLL
        8 => 16,   // SM_CYVSCROLL
        90 => 96,  // LOGPIXELSX
        91 => 96,  // LOGPIXELSY
        _ => 0,
    }
}

pub unsafe extern "win64" fn message_box_a(_hwnd: usize, text: *const u8, caption: *const u8, _typ: u32) -> i32 {
    eprintln!("winlux: MessageBoxA [{}] {}", crate::dlls::cstr(caption), crate::dlls::cstr(text));
    1
}

pub unsafe extern "win64" fn message_box_w(_hwnd: usize, text: *const u16, caption: *const u16, _typ: u32) -> i32 {
    eprintln!("winlux: MessageBoxW [{}] {}", crate::dlls::wstr(caption), crate::dlls::wstr(text));
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterClassA"=>fn register_class_a, "RegisterClassW"=>fn register_class_w,
        "RegisterClassExA"=>fn register_class_ex_a, "RegisterClassExW"=>fn register_class_ex_w,
        "CreateWindowExA"=>fn create_window_ex_a, "CreateWindowExW"=>fn create_window_ex_w,
        "DefWindowProcA"=>fn def_window_proc_a, "DefWindowProcW"=>fn def_window_proc_w,
        "GetMessageA"=>fn get_message_a, "PeekMessageA"=>fn peek_message_a,
        "TranslateMessage"=>fn translate_message, "DispatchMessageA"=>fn dispatch_message_a,
        "PostQuitMessage"=>fn post_quit_message, "PostMessageA"=>fn post_message_a,
        "SendMessageA"=>fn send_message_a, "ShowWindow"=>fn show_window,
        "UpdateWindow"=>fn update_window, "DestroyWindow"=>fn destroy_window,
        "IsWindow"=>fn is_window, "GetWindowRect"=>fn get_window_rect,
        "GetClientRect"=>fn get_client_rect, "SetWindowTextA"=>fn set_window_text_a,
        "GetWindowTextA"=>fn get_window_text_a, "GetWindowTextLengthA"=>fn get_window_text_length_a,
        "BeginPaint"=>fn begin_paint, "EndPaint"=>fn end_paint,
        "InvalidateRect"=>fn invalidate_rect, "ValidateRect"=>fn validate_rect,
        "GetDesktopWindow"=>fn get_desktop_window, "GetForegroundWindow"=>fn get_foreground_window,
        "GetSystemMetrics"=>fn get_system_metrics,
        "MessageBoxA"=>fn message_box_a, "MessageBoxW"=>fn message_box_w,
    }
}
