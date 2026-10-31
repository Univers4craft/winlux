//! API `user32` de Winlux : fenêtres et messages, reliées au noyau `gui`.
//!
//! Les fonctions suivent les conventions Win32 (A/W, `set_last_error`, pointeurs
//! guests). Le rendu est délégué au backend du noyau (headless aujourd'hui).

use crate::dlls::gui::{
    self, ClassInfo, CreateStructA, Msg, PaintStruct, Point, Rect, WndClassA, WndClassExA,
};
use std::ffi::c_void;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOREDRAW: u32 = 0x0008;
const SWP_SHOWWINDOW: u32 = 0x0040;
const SWP_HIDEWINDOW: u32 = 0x0080;
const SWP_FRAMECHANGED: u32 = 0x0020;

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

unsafe fn register_class_common(
    name: String,
    wnd_proc: usize,
    background: usize,
    instance: usize,
) -> usize {
    if name.is_empty() || wnd_proc == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if gui::register_class(
        &name,
        ClassInfo {
            wnd_proc,
            background,
            instance,
        },
    ) {
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
    class: String,
    title: String,
    style: u32,
    ex_style: u32,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    parent: usize,
    instance: usize,
) -> usize {
    let Some(info) = gui::lookup_class(&class) else {
        crate::process::set_last_error(1407); // ERROR_CLASS_DOES_NOT_EXIST
        return 0;
    };
    let hwnd = gui::alloc_hwnd();
    // Tailles CW_USEDEFAULT : fenêtre raisonnable par défaut.
    let w = if cx == gui::CW_USEDEFAULT || cx <= 0 {
        800
    } else {
        cx
    };
    let h = if cy == gui::CW_USEDEFAULT || cy <= 0 {
        600
    } else {
        cy
    };
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

    gui::insert_window(
        hwnd,
        gui::Window {
            class: class.clone(),
            wnd_proc: info.wnd_proc,
            background: info.background,
            title: title.clone(),
            rect,
            visible,
            parent,
            owner: std::thread::current().id(),
            needs_paint: visible,
            paint_queued: false,
            style,
            ex_style,
            pending: Vec::new(),
        },
    );

    // WM_CREATE synchrone (le wndproc peut refuser la fenêtre).
    let created = crate::dlls::gui::call_wndproc(
        info.wnd_proc,
        hwnd,
        gui::WM_CREATE,
        0,
        &cs as *const CreateStructA as isize,
    );
    if created == -1 {
        gui::destroy_window(hwnd);
        return 0;
    }

    with_backend(|b| {
        b.create_window(hwnd, &title, rect);
        if visible {
            b.show_window(hwnd, true);
        }
    });

    // WM_SIZE puis, si visible, demande de peinture.
    gui::post_to_window(
        hwnd,
        Msg {
            hwnd,
            message: gui::WM_SIZE,
            wparam: 0,
            lparam: (((h as u32 as usize) << 16) | (w as u32 as usize)) as isize,
            time: now_tick(),
            pt: Point::default(),
            l_private: 0,
        },
    );
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
    ex_style: u32,
    class: *const u8,
    title: *const u8,
    style: u32,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    parent: usize,
    menu: usize,
    instance: usize,
    param: *mut c_void,
) -> usize {
    let _ = param;
    let _ = menu;
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    create_window_common(
        read_class_name_a(class),
        read_class_name_a(title),
        style,
        ex_style,
        x,
        y,
        cx,
        cy,
        parent,
        instance,
    )
}

pub unsafe extern "win64" fn create_window_ex_w(
    ex_style: u32,
    class: *const u16,
    title: *const u16,
    style: u32,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    parent: usize,
    menu: usize,
    instance: usize,
    param: *mut c_void,
) -> usize {
    let _ = param;
    let _ = menu;
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    create_window_common(
        read_class_name_w(class),
        read_class_name_w(title),
        style,
        ex_style,
        x,
        y,
        cx,
        cy,
        parent,
        instance,
    )
}

// ---------- Boucle de messages ----------

unsafe fn write_msg(out: *mut Msg, m: &Msg) {
    if !out.is_null() {
        std::ptr::write_unaligned(
            out,
            Msg {
                hwnd: m.hwnd,
                message: m.message,
                wparam: m.wparam,
                lparam: m.lparam,
                time: m.time,
                pt: m.pt,
                l_private: m.l_private,
            },
        );
    }
}

pub unsafe extern "win64" fn get_message_a(out: *mut Msg, hwnd: usize, min: u32, max: u32) -> i32 {
    loop {
        gui::pump_backend();
        let q = gui::current_queue();
        // Un minuteur très en retard passe avant les entrées (anti-famine).
        if let Some(m) = gui::due_timer(hwnd, min, max, true) {
            write_msg(out, &m);
            return 1;
        }
        // WM_QUIT (posted par PostQuitMessage) termine la boucle (renvoie 0).
        if let Some(m) = gui::take_message(&q, hwnd, min, max) {
            if m.message == gui::WM_QUIT {
                return 0;
            }
            gui::observe_key_message(&m);
            write_msg(out, &m);
            return 1;
        }
        // File vide : en headless on rend la main (fin de boucle); avec un
        // backend réel on attend les prochains événements hôte, comme Win32.
        let wait = gui::next_timer_ms(hwnd).map_or(50, |ms| ms.min(50));
        if !gui::wait_backend(wait) {
            return 0;
        }
        // WM_TIMER est le message le moins prioritaire, comme sous Windows :
        // seulement quand la file est vide.
        if gui::peek_message(&q, hwnd, min, max).is_none() {
            if let Some(m) = gui::due_timer(hwnd, min, max, false) {
                write_msg(out, &m);
                return 1;
            }
        }
    }
}

pub unsafe extern "win64" fn get_message_w(out: *mut Msg, hwnd: usize, min: u32, max: u32) -> i32 {
    get_message_a(out, hwnd, min, max)
}

pub unsafe extern "win64" fn peek_message_a(
    out: *mut Msg,
    hwnd: usize,
    min: u32,
    max: u32,
    remove: u32,
) -> i32 {
    gui::pump_backend();
    let q = gui::current_queue();
    if remove & 0x1 != 0 {
        if let Some(m) = gui::take_message(&q, hwnd, min, max) {
            if m.message == gui::WM_QUIT {
                // PM_REMOVE sur WM_QUIT : on le remet et on renvoie -1 (fin).
                gui::prepend_message(&q, m);
                return -1;
            }
            gui::observe_key_message(&m);
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

pub unsafe extern "win64" fn peek_message_w(
    out: *mut Msg,
    hwnd: usize,
    min: u32,
    max: u32,
    remove: u32,
) -> i32 {
    peek_message_a(out, hwnd, min, max, remove)
}

pub unsafe extern "win64" fn translate_message(msg: *const Msg) -> i32 {
    if msg.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let m = &*msg;
    // Win32 : une pression de touche devient un message de caractère quand la
    // disposition clavier en produit un. Le backend X11 a mémorisé le caractère
    // effectif (majuscules, ponctuation, accents) ; à défaut on retombe sur la
    // disposition US non modifiée.
    if m.message == gui::WM_KEYDOWN {
        if let Some(ch) = gui::take_key_char(m.hwnd, m.wparam, m.lparam) {
            return gui::post_to_window(
                m.hwnd,
                gui::Msg::new(m.hwnd, gui::WM_CHAR, ch as usize, m.lparam),
            ) as i32;
        }
    }
    0
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

pub unsafe extern "win64" fn dispatch_message_w(msg: *const Msg) -> isize {
    dispatch_message_a(msg)
}

pub unsafe extern "win64" fn post_quit_message(code: i32) {
    let q = gui::current_queue();
    gui::post_quit(&q, code);
}

pub unsafe extern "win64" fn post_message_a(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> i32 {
    if hwnd == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let m = Msg {
        hwnd,
        message,
        wparam,
        lparam,
        time: now_tick(),
        pt: Point::default(),
        l_private: 0,
    };
    if gui::post_to_window(hwnd, m) {
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn post_message_w(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> i32 {
    post_message_a(hwnd, message, wparam, lparam)
}

pub unsafe extern "win64" fn send_message_a(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if hwnd == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    match gui::wnd_proc_of(hwnd) {
        Some(wp) => gui::call_wndproc(wp, hwnd, message, wparam, lparam),
        None => gui::def_window_proc(hwnd, message, wparam, lparam),
    }
}

pub unsafe extern "win64" fn send_message_w(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    send_message_a(hwnd, message, wparam, lparam)
}

// ---------- Propriétés de fenêtre ----------

pub unsafe extern "win64" fn def_window_proc_a(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    gui::def_window_proc(hwnd, message, wparam, lparam)
}

pub unsafe extern "win64" fn def_window_proc_w(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
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
    if gui::destroy_window(hwnd) {
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn is_window(hwnd: usize) -> i32 {
    gui::wnd_proc_of(hwnd).is_some() as i32
}

pub extern "win64" fn get_focus() -> usize {
    gui::focused_window()
}

pub extern "win64" fn get_key_state(vk: i32) -> i16 {
    if !(0..256).contains(&vk) {
        return 0;
    }
    gui::get_key_state(vk as usize)
}

pub extern "win64" fn get_async_key_state(vk: i32) -> i16 {
    if !(0..256).contains(&vk) {
        return 0;
    }
    gui::get_async_key_state(vk as usize)
}

pub unsafe extern "win64" fn get_keyboard_state(out: *mut u8) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let state = gui::get_keyboard_state();
    std::ptr::copy_nonoverlapping(state.as_ptr(), out, state.len());
    1
}

pub unsafe extern "win64" fn set_focus(hwnd: usize) -> usize {
    if hwnd != 0 {
        let Some(owner) = gui::window_owner(hwnd) else {
            crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
            return 0;
        };
        if owner != std::thread::current().id() {
            crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
            return 0;
        }
    }
    let Some(old) = gui::set_focus(hwnd) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if old == hwnd {
        return old;
    }
    if hwnd == 0 {
        gui::clear_current_thread_keyboard_state();
        gui::clear_async_key_down_states();
    }

    // Met à jour le backend avant les callbacks : un SetFocus réentrant depuis
    // un WndProc pourra ainsi le remplacer sans être écrasé au retour.
    with_backend(|backend| backend.set_focus(hwnd));
    // Traite les FocusIn/FocusOut éventuellement synchrones générés par X11 ;
    // le noyau connaît déjà le nouvel état et élimine ainsi les doublons.
    gui::pump_backend();
    if old != 0 {
        if let Some(wp) = gui::wnd_proc_of(old) {
            gui::call_wndproc(wp, old, gui::WM_KILLFOCUS, hwnd, 0);
        }
    }
    if hwnd != 0 && gui::focused_window() == hwnd {
        if let Some(wp) = gui::wnd_proc_of(hwnd) {
            gui::call_wndproc(wp, hwnd, gui::WM_SETFOCUS, old, 0);
        }
    }
    old
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

fn pack_i16_pair(low: i32, high: i32) -> isize {
    (((high as u16 as u32) << 16) | low as u16 as u32) as isize
}

/// Positionne/redimensionne une fenêtre et publie `WM_MOVE`/`WM_SIZE`.
/// Le z-order et les notifications WINDOWPOS avancées ne sont pas encore gérés.
pub unsafe extern "win64" fn set_window_pos(
    hwnd: usize,
    _insert_after: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> i32 {
    let Some(old) = gui::window_rect(hwnd) else {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    };
    let left = if flags & SWP_NOMOVE != 0 { old.left } else { x };
    let top = if flags & SWP_NOMOVE != 0 { old.top } else { y };
    let width = if flags & SWP_NOSIZE != 0 {
        old.width()
    } else {
        cx
    };
    let height = if flags & SWP_NOSIZE != 0 {
        old.height()
    } else {
        cy
    };
    if width <= 0 || height <= 0 {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let Some(right) = left.checked_add(width) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(bottom) = top.checked_add(height) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let rect = Rect {
        left,
        top,
        right,
        bottom,
    };
    let moved = left != old.left || top != old.top;
    let resized = width != old.width() || height != old.height();

    if moved || resized {
        if gui::set_window_rect(hwnd, rect).is_none() {
            crate::process::set_last_error(1400);
            return 0;
        }
        // Le verrou de la table HWND est relâché avant celui du backend.
        with_backend(|backend| backend.set_window_rect(hwnd, rect));
        if resized {
            gui::post_to_window(
                hwnd,
                Msg::new(hwnd, gui::WM_SIZE, 0, pack_i16_pair(width, height)),
            );
        }
        if moved {
            gui::post_to_window(
                hwnd,
                Msg::new(hwnd, gui::WM_MOVE, 0, pack_i16_pair(left, top)),
            );
        }
    }

    let mut visibility_changed = false;
    if flags & SWP_SHOWWINDOW != 0 {
        visibility_changed = !gui::set_visible(hwnd, true);
        with_backend(|backend| backend.show_window(hwnd, true));
    } else if flags & SWP_HIDEWINDOW != 0 {
        visibility_changed = gui::set_visible(hwnd, false);
        with_backend(|backend| backend.show_window(hwnd, false));
    }

    if flags & SWP_NOREDRAW == 0
        && gui::window_rect(hwnd).is_some()
        && (moved || resized || visibility_changed || flags & SWP_FRAMECHANGED != 0)
    {
        // N'invalide pas une fenêtre masquée.
        if flags & SWP_HIDEWINDOW == 0 {
            gui::request_paint(hwnd);
        }
    }
    1
}

/// Déplace/redimensionne une fenêtre (raccourci Win32 de `SetWindowPos`).
pub unsafe extern "win64" fn move_window(
    hwnd: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    repaint: i32,
) -> i32 {
    set_window_pos(
        hwnd,
        0,
        x,
        y,
        width,
        height,
        if repaint != 0 { 0 } else { SWP_NOREDRAW },
    )
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

pub unsafe extern "win64" fn set_window_text_w(hwnd: usize, text: *const u16) -> i32 {
    let t = read_class_name_w(text);
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

pub unsafe extern "win64" fn get_window_text_w(hwnd: usize, buf: *mut u16, cap: i32) -> i32 {
    match gui::title_of(hwnd) {
        Some(t) => crate::dlls::put_w(&t, buf, cap.max(0) as u32) as i32,
        None => 0,
    }
}

pub unsafe extern "win64" fn get_window_text_length_a(hwnd: usize) -> i32 {
    gui::title_of(hwnd).map(|t| t.len() as i32).unwrap_or(0)
}

pub unsafe extern "win64" fn get_window_text_length_w(hwnd: usize) -> i32 {
    gui::title_of(hwnd)
        .map(|t| t.encode_utf16().count() as i32)
        .unwrap_or(0)
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
    std::ptr::write_unaligned(
        ps,
        PaintStruct {
            hdc,
            erase: 1,
            paint: rect,
            restore: 0,
            inc_update: 0,
            reserved: [0; 32],
        },
    );
    hdc
}

pub unsafe extern "win64" fn end_paint(hwnd: usize, ps: *const PaintStruct) -> i32 {
    let hdc = if ps.is_null() { 0 } else { (*ps).hdc };
    gui::end_paint(hwnd, hdc) as i32
}

// ---------- DC et primitives de dessin ----------

/// DC ciblant une fenêtre (le `hwnd` 0 désigne le bureau, sans rendu).
pub extern "win64" fn get_dc(hwnd: usize) -> usize {
    if hwnd != 0 && gui::wnd_proc_of(hwnd).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    gui::create_dc(hwnd)
}

pub unsafe extern "win64" fn release_dc(hwnd: usize, hdc: usize) -> i32 {
    if gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    gui::free_dc(hdc);
    gui::flush_paint(hwnd);
    1
}

/// Remplit un rectangle avec la couleur du pinceau (`FillRect`, user32).
pub unsafe extern "win64" fn fill_rect(hdc: usize, rect: *const Rect, brush: usize) -> i32 {
    if rect.is_null() || gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    if let Some(color) = gui::object_color(brush) {
        gui::record(hdc, gui::DrawCmd::FillRect { rect: *rect, color });
    }
    1
}

/// Cadre d'un pixel avec la couleur du pinceau (`FrameRect`, user32).
pub unsafe extern "win64" fn frame_rect(hdc: usize, rect: *const Rect, brush: usize) -> i32 {
    if rect.is_null() || gui::dc_window(hdc).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    if let Some(color) = gui::object_color(brush) {
        gui::record(
            hdc,
            gui::DrawCmd::FrameRect {
                rect: *rect,
                color,
                width: 1,
            },
        );
    }
    1
}

pub unsafe extern "win64" fn invalidate_rect(hwnd: usize, _rect: *const Rect, _erase: i32) -> i32 {
    if gui::wnd_proc_of(hwnd).is_some() {
        // Le backend X11 efface la zone (→ Expose) et le noyau publie WM_PAINT.
        with_backend(|b| b.invalidate(hwnd));
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
        0 | 16 | 78 => gui::screen_size().0, // SM_CXSCREEN, SM_CXFULLSCREEN, SM_CXVIRTUALSCREEN
        1 | 17 | 79 => gui::screen_size().1, // SM_CYSCREEN, SM_CYFULLSCREEN, SM_CYVIRTUALSCREEN
        2 => 4,                              // SM_CYCAPTION
        3 => 4,                              // SM_CXBORDER
        4 => 4,                              // SM_CYBORDER
        7 => 32,                             // SM_CYHSCROLL
        8 => 16,                             // SM_CYVSCROLL
        90 => 96,                            // LOGPIXELSX
        91 => 96,                            // LOGPIXELSY
        _ => 0,
    }
}

pub extern "win64" fn get_sys_color(index: i32) -> u32 {
    gui::theme::system_color(index).unwrap_or(0)
}

pub extern "win64" fn get_sys_color_brush(index: i32) -> usize {
    let Some(color) = gui::theme::system_color(index) else {
        return 0;
    };
    gui::system_color_brush(index as usize, color)
}

pub unsafe extern "win64" fn message_box_a(
    _hwnd: usize,
    text: *const u8,
    caption: *const u8,
    _typ: u32,
) -> i32 {
    eprintln!(
        "winlux: MessageBoxA [{}] {}",
        crate::dlls::cstr(caption),
        crate::dlls::cstr(text)
    );
    1
}

pub unsafe extern "win64" fn message_box_w(
    _hwnd: usize,
    text: *const u16,
    caption: *const u16,
    _typ: u32,
) -> i32 {
    eprintln!(
        "winlux: MessageBoxW [{}] {}",
        crate::dlls::wstr(caption),
        crate::dlls::wstr(text)
    );
    1
}

// =====================================================================
// GetWindowLong / SetWindowLong  (A/W + Ptr variants)
// =====================================================================

const GWL_WNDPROC: i32 = -4;
const GWL_HINSTANCE: i32 = -6;
const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const GWL_USERDATA: i32 = -21;
const GWL_ID: i32 = -12;
const _GWLP_WNDPROC: i32 = -4;
const _GWLP_HINSTANCE: i32 = -6;
const _GWLP_USERDATA: i32 = -21;
const _GWLP_ID: i32 = -12;
const DWL_DLGPROC: i32 = 4;
const DWL_MSGRESULT: i32 = 0;
const DWL_USER: i32 = 8;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn user_data_map() -> &'static Mutex<HashMap<usize, isize>> {
    static M: OnceLock<Mutex<HashMap<usize, isize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

use super::lock;

unsafe fn get_window_long_impl(hwnd: usize, index: i32) -> isize {
    match index {
        GWL_STYLE => gui::window_style(hwnd).unwrap_or(0) as isize,
        GWL_EXSTYLE => gui::window_ex_style(hwnd).unwrap_or(0) as isize,
        GWL_WNDPROC => gui::wnd_proc_of(hwnd).unwrap_or(0) as isize,
        GWL_USERDATA => lock(user_data_map()).get(&hwnd).copied().unwrap_or(0),
        GWL_HINSTANCE => 0x0040_0000_isize,
        GWL_ID | DWL_DLGPROC | DWL_MSGRESULT | DWL_USER => 0,
        _ => {
            // Offset positif = données supplémentaires de classe (cbWndExtra)
            if index >= 0 {
                lock(user_data_map())
                    .get(&(hwnd.wrapping_add(index as usize)))
                    .copied()
                    .unwrap_or(0)
            } else {
                0
            }
        }
    }
}

unsafe fn set_window_long_impl(hwnd: usize, index: i32, value: isize) -> isize {
    let old = get_window_long_impl(hwnd, index);
    match index {
        GWL_STYLE => {
            gui::set_window_style(hwnd, value as u32);
        }
        GWL_EXSTYLE => {
            gui::set_window_ex_style(hwnd, value as u32);
        }
        GWL_WNDPROC => {
            gui::set_wnd_proc(hwnd, value as usize);
        }
        GWL_USERDATA => {
            lock(user_data_map()).insert(hwnd, value);
        }
        _ => {
            if index >= 0 {
                lock(user_data_map()).insert(hwnd.wrapping_add(index as usize), value);
            }
        }
    }
    old
}

pub unsafe extern "win64" fn get_window_long_a(hwnd: usize, index: i32) -> i32 {
    get_window_long_impl(hwnd, index) as i32
}

pub unsafe extern "win64" fn get_window_long_w(hwnd: usize, index: i32) -> i32 {
    get_window_long_impl(hwnd, index) as i32
}

pub unsafe extern "win64" fn set_window_long_a(hwnd: usize, index: i32, value: i32) -> i32 {
    set_window_long_impl(hwnd, index, value as isize) as i32
}

pub unsafe extern "win64" fn set_window_long_w(hwnd: usize, index: i32, value: i32) -> i32 {
    set_window_long_impl(hwnd, index, value as isize) as i32
}

pub unsafe extern "win64" fn get_window_long_ptr_a(hwnd: usize, index: i32) -> isize {
    get_window_long_impl(hwnd, index)
}

pub unsafe extern "win64" fn get_window_long_ptr_w(hwnd: usize, index: i32) -> isize {
    get_window_long_impl(hwnd, index)
}

pub unsafe extern "win64" fn set_window_long_ptr_a(hwnd: usize, index: i32, value: isize) -> isize {
    set_window_long_impl(hwnd, index, value)
}

pub unsafe extern "win64" fn set_window_long_ptr_w(hwnd: usize, index: i32, value: isize) -> isize {
    set_window_long_impl(hwnd, index, value)
}

// =====================================================================
// GetClassLongA/W, GetClassLongPtrA/W, GetClassNameA/W, GetClassInfoExA/W
// =====================================================================

pub unsafe extern "win64" fn get_class_long_a(hwnd: usize, index: i32) -> u32 {
    let _ = (hwnd, index);
    0
}

pub unsafe extern "win64" fn get_class_long_w(hwnd: usize, index: i32) -> u32 {
    let _ = (hwnd, index);
    0
}

pub unsafe extern "win64" fn get_class_long_ptr_a(hwnd: usize, index: i32) -> usize {
    let _ = (hwnd, index);
    0
}

pub unsafe extern "win64" fn get_class_long_ptr_w(hwnd: usize, index: i32) -> usize {
    let _ = (hwnd, index);
    0
}

pub unsafe extern "win64" fn get_class_name_a(hwnd: usize, buf: *mut u8, max_count: i32) -> i32 {
    if let Some(name) = gui::window_class(hwnd) {
        crate::dlls::put_a(&name, buf, max_count as u32) as i32
    } else {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        0
    }
}

pub unsafe extern "win64" fn get_class_name_w(hwnd: usize, buf: *mut u16, max_count: i32) -> i32 {
    if let Some(name) = gui::window_class(hwnd) {
        crate::dlls::put_w(&name, buf, max_count as u32) as i32
    } else {
        crate::process::set_last_error(1400);
        0
    }
}

pub unsafe extern "win64" fn get_class_info_ex_a(
    _instance: usize,
    _name: *const u8,
    _out: *mut u8,
) -> i32 {
    // Stub : renvoie 0 (non trouvé)
    0
}

pub unsafe extern "win64" fn get_class_info_ex_w(
    _instance: usize,
    _name: *const u16,
    _out: *mut u8,
) -> i32 {
    0
}

// =====================================================================
// AdjustWindowRect / AdjustWindowRectEx
// =====================================================================

pub unsafe extern "win64" fn adjust_window_rect(rect: *mut Rect, style: u32, menu: i32) -> i32 {
    adjust_window_rect_ex(rect, style, menu, 0)
}

pub unsafe extern "win64" fn adjust_window_rect_ex(
    rect: *mut Rect,
    style: u32,
    _menu: i32,
    _ex_style: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    // Marge simple pour les bordures de fenêtre
    let border = if style & 0x0080_0000 != 0 { 8 } else { 0 }; // WS_BORDER
    let caption = if style & 0x00C0_0000 != 0 { 31 } else { 0 }; // WS_CAPTION
    (*rect).left -= border;
    (*rect).top -= border + caption;
    (*rect).right += border;
    (*rect).bottom += border;
    1
}

// =====================================================================
// Requêtes d'état de fenêtre
// =====================================================================

pub unsafe extern "win64" fn is_window_visible(hwnd: usize) -> i32 {
    gui::is_window_visible(hwnd) as i32
}

pub unsafe extern "win64" fn is_window_enabled(_hwnd: usize) -> i32 {
    1 // Toutes les fenêtres sont « enabled » pour l'instant
}

pub unsafe extern "win64" fn enable_window(_hwnd: usize, _enable: i32) -> i32 {
    0 // Était « enabled » avant
}

pub unsafe extern "win64" fn is_iconic(_hwnd: usize) -> i32 {
    0 // Pas de minimisation dans Winlux pour l'instant
}

pub unsafe extern "win64" fn is_zoomed(_hwnd: usize) -> i32 {
    0
}

// =====================================================================
// GetParent / SetParent
// =====================================================================

pub extern "win64" fn get_parent(hwnd: usize) -> usize {
    gui::window_parent(hwnd).unwrap_or(0)
}

pub extern "win64" fn set_parent(hwnd: usize, new_parent: usize) -> usize {
    gui::set_window_parent(hwnd, new_parent).unwrap_or(0)
}

// =====================================================================
// FindWindowA/W, EnumWindows, GetWindow
// =====================================================================

pub unsafe extern "win64" fn find_window_a(class: *const u8, title: *const u8) -> usize {
    let cls = if class.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(class))
    };
    let ttl = if title.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(title))
    };
    gui::find_window(cls.as_deref(), ttl.as_deref()).unwrap_or(0)
}

pub unsafe extern "win64" fn find_window_w(class: *const u16, title: *const u16) -> usize {
    let cls = if class.is_null() {
        None
    } else {
        Some(crate::dlls::wstr(class))
    };
    let ttl = if title.is_null() {
        None
    } else {
        Some(crate::dlls::wstr(title))
    };
    gui::find_window(cls.as_deref(), ttl.as_deref()).unwrap_or(0)
}

pub unsafe extern "win64" fn find_window_ex_a(
    parent: usize,
    after: usize,
    class: *const u8,
    title: *const u8,
) -> usize {
    // Simplifié : ignore parent/after et cherche globalement
    let _ = (parent, after);
    find_window_a(class, title)
}

pub unsafe extern "win64" fn find_window_ex_w(
    parent: usize,
    after: usize,
    class: *const u16,
    title: *const u16,
) -> usize {
    let _ = (parent, after);
    find_window_w(class, title)
}

pub unsafe extern "win64" fn enum_windows(callback: usize, lparam: isize) -> i32 {
    if callback == 0 {
        return 0;
    }
    let windows = gui::enum_top_level_windows();
    type EnumCallback = unsafe extern "win64" fn(usize, isize) -> i32;
    let cb: EnumCallback = std::mem::transmute(callback);
    for hwnd in windows {
        if cb(hwnd, lparam) == 0 {
            break;
        }
    }
    1
}

pub unsafe extern "win64" fn enum_child_windows(
    _parent: usize,
    _callback: usize,
    _lparam: isize,
) -> i32 {
    // Stub : pas d'enfants réels
    1
}

pub extern "win64" fn get_window(hwnd: usize, cmd: u32) -> usize {
    gui::get_window_relative(hwnd, cmd).unwrap_or(0)
}

pub extern "win64" fn get_top_window(hwnd: usize) -> usize {
    gui::get_window_relative(hwnd, 5).unwrap_or(0) // GW_CHILD
}

// =====================================================================
// Input : capture, timers, curseur, clavier
// =====================================================================

pub extern "win64" fn set_capture(hwnd: usize) -> usize {
    gui::set_capture(hwnd)
}

pub extern "win64" fn release_capture() -> i32 {
    gui::release_capture() as i32
}

pub extern "win64" fn get_capture() -> usize {
    gui::get_capture()
}

pub extern "win64" fn set_timer(hwnd: usize, id: usize, elapse: u32, callback: usize) -> usize {
    gui::set_timer(hwnd, id, elapse, callback)
}

pub extern "win64" fn kill_timer(hwnd: usize, id: usize) -> i32 {
    gui::kill_timer(hwnd, id) as i32
}

pub extern "win64" fn map_virtual_key_a(code: u32, map_type: u32) -> u32 {
    map_virtual_key_impl(code, map_type)
}

pub extern "win64" fn map_virtual_key_w(code: u32, map_type: u32) -> u32 {
    map_virtual_key_impl(code, map_type)
}

pub extern "win64" fn map_virtual_key_ex_a(code: u32, map_type: u32, _hkl: usize) -> u32 {
    map_virtual_key_impl(code, map_type)
}

pub extern "win64" fn map_virtual_key_ex_w(code: u32, map_type: u32, _hkl: usize) -> u32 {
    map_virtual_key_impl(code, map_type)
}

fn map_virtual_key_impl(code: u32, map_type: u32) -> u32 {
    // Mappage simplifié VK <-> scan code
    match map_type {
        0 => code, // MAPVK_VK_TO_VSC
        1 => code, // MAPVK_VSC_TO_VK
        2 => {
            if (0x30..=0x39).contains(&code) || (0x41..=0x5A).contains(&code) {
                code
            } else {
                0
            }
        }
        3 => code, // MAPVK_VSC_TO_VK_EX
        _ => 0,
    }
}

pub unsafe extern "win64" fn get_cursor_pos(point: *mut Point) -> i32 {
    if point.is_null() {
        return 0;
    }
    // Position par défaut au centre de l'écran
    let (w, h) = gui::screen_size();
    (*point).x = w / 2;
    (*point).y = h / 2;
    1
}

pub unsafe extern "win64" fn set_cursor_pos(_x: i32, _y: i32) -> i32 {
    1 // Stub : succès
}

pub unsafe extern "win64" fn screen_to_client(hwnd: usize, point: *mut Point) -> i32 {
    if point.is_null() {
        return 0;
    }
    if let Some(r) = gui::window_rect(hwnd) {
        (*point).x -= r.left;
        (*point).y -= r.top;
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn client_to_screen(hwnd: usize, point: *mut Point) -> i32 {
    if point.is_null() {
        return 0;
    }
    if let Some(r) = gui::window_rect(hwnd) {
        (*point).x += r.left;
        (*point).y += r.top;
        1
    } else {
        0
    }
}

pub extern "win64" fn load_cursor_a(_instance: usize, name: usize) -> usize {
    gui::load_cursor(_instance, name)
}

pub extern "win64" fn load_cursor_w(_instance: usize, name: usize) -> usize {
    gui::load_cursor(_instance, name)
}

pub extern "win64" fn set_cursor(cursor: usize) -> usize {
    gui::set_cursor(cursor)
}

pub extern "win64" fn show_cursor(show: i32) -> i32 {
    gui::show_cursor(show != 0)
}

pub extern "win64" fn get_double_click_time() -> u32 {
    500 // Valeur Windows par défaut
}

pub extern "win64" fn swap_mouse_button(_swap: i32) -> i32 {
    0 // Pas de swap
}

// =====================================================================
// Dialog / Menu stubs
// =====================================================================

pub extern "win64" fn create_menu() -> usize {
    // Handle factice
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x0010_0000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub extern "win64" fn create_popup_menu() -> usize {
    create_menu()
}

pub unsafe extern "win64" fn append_menu_a(
    menu: usize,
    flags: u32,
    id: usize,
    text: *const u8,
) -> i32 {
    let _ = (menu, flags, id, text);
    1
}

pub unsafe extern "win64" fn append_menu_w(
    menu: usize,
    flags: u32,
    id: usize,
    text: *const u16,
) -> i32 {
    let _ = (menu, flags, id, text);
    1
}

pub unsafe extern "win64" fn insert_menu_a(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    text: *const u8,
) -> i32 {
    let _ = (menu, pos, flags, id, text);
    1
}

pub unsafe extern "win64" fn insert_menu_w(
    menu: usize,
    pos: u32,
    flags: u32,
    id: usize,
    text: *const u16,
) -> i32 {
    let _ = (menu, pos, flags, id, text);
    1
}

pub unsafe extern "win64" fn insert_menu_item_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    1
}

pub unsafe extern "win64" fn insert_menu_item_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    1
}

pub extern "win64" fn destroy_menu(_menu: usize) -> i32 {
    1
}

pub extern "win64" fn get_menu(hwnd: usize) -> usize {
    let _ = hwnd;
    0
}

pub extern "win64" fn set_menu(hwnd: usize, menu: usize) -> i32 {
    let _ = (hwnd, menu);
    1
}

pub extern "win64" fn get_sub_menu(menu: usize, pos: i32) -> usize {
    let _ = (menu, pos);
    0
}

pub extern "win64" fn get_system_menu(hwnd: usize, revert: i32) -> usize {
    let _ = (hwnd, revert);
    0
}

pub unsafe extern "win64" fn track_popup_menu(
    menu: usize,
    flags: u32,
    x: i32,
    y: i32,
    _reserved: i32,
    hwnd: usize,
    _rect: *const Rect,
) -> i32 {
    let _ = (menu, flags, x, y, hwnd);
    0 // Rien sélectionné
}

pub unsafe extern "win64" fn track_popup_menu_ex(
    menu: usize,
    flags: u32,
    x: i32,
    y: i32,
    hwnd: usize,
    _params: *const c_void,
) -> i32 {
    let _ = (menu, flags, x, y, hwnd);
    0
}

pub unsafe extern "win64" fn check_menu_item(menu: usize, id: u32, flags: u32) -> u32 {
    let _ = (menu, id, flags);
    0
}

pub unsafe extern "win64" fn enable_menu_item(menu: usize, id: u32, flags: u32) -> i32 {
    let _ = (menu, id, flags);
    1
}

pub extern "win64" fn get_menu_item_count(menu: usize) -> i32 {
    let _ = menu;
    0
}

pub unsafe extern "win64" fn get_menu_item_id(menu: usize, pos: i32) -> u32 {
    let _ = (menu, pos);
    0xFFFF_FFFF
}

pub unsafe extern "win64" fn delete_menu(menu: usize, pos: u32, flags: u32) -> i32 {
    let _ = (menu, pos, flags);
    1
}

pub unsafe extern "win64" fn remove_menu(menu: usize, pos: u32, flags: u32) -> i32 {
    let _ = (menu, pos, flags);
    1
}

pub unsafe extern "win64" fn draw_menu_bar(hwnd: usize) -> i32 {
    let _ = hwnd;
    1
}

pub unsafe extern "win64" fn set_menu_item_info_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    1
}

pub unsafe extern "win64" fn set_menu_item_info_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *const c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    1
}

pub unsafe extern "win64" fn get_menu_item_info_a(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *mut c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    0
}

pub unsafe extern "win64" fn get_menu_item_info_w(
    menu: usize,
    item: u32,
    by_pos: i32,
    info: *mut c_void,
) -> i32 {
    let _ = (menu, item, by_pos, info);
    0
}

// ---------- Dialogs ----------

pub unsafe extern "win64" fn dialog_box_param_a(
    _instance: usize,
    _template: *const u8,
    _parent: usize,
    _dlg_proc: usize,
    _init_param: isize,
) -> isize {
    -1 // IDCANCEL-ish
}

pub unsafe extern "win64" fn dialog_box_param_w(
    _instance: usize,
    _template: *const u16,
    _parent: usize,
    _dlg_proc: usize,
    _init_param: isize,
) -> isize {
    -1
}

pub unsafe extern "win64" fn create_dialog_param_a(
    _instance: usize,
    _template: *const u8,
    _parent: usize,
    _dlg_proc: usize,
    _init_param: isize,
) -> usize {
    0
}

pub unsafe extern "win64" fn create_dialog_param_w(
    _instance: usize,
    _template: *const u16,
    _parent: usize,
    _dlg_proc: usize,
    _init_param: isize,
) -> usize {
    0
}

pub extern "win64" fn get_dlg_item(_dlg: usize, _id: i32) -> usize {
    0
}

pub extern "win64" fn end_dialog(_dlg: usize, _result: isize) -> i32 {
    1
}

pub unsafe extern "win64" fn set_dlg_item_text_a(_dlg: usize, _id: i32, _text: *const u8) -> i32 {
    1
}

pub unsafe extern "win64" fn set_dlg_item_text_w(_dlg: usize, _id: i32, _text: *const u16) -> i32 {
    1
}

pub unsafe extern "win64" fn get_dlg_item_text_a(
    _dlg: usize,
    _id: i32,
    buf: *mut u8,
    max: i32,
) -> u32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    0
}

pub unsafe extern "win64" fn get_dlg_item_text_w(
    _dlg: usize,
    _id: i32,
    buf: *mut u16,
    max: i32,
) -> u32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    0
}

pub unsafe extern "win64" fn get_dlg_ctrl_id(_hwnd: usize) -> i32 {
    0
}

pub unsafe extern "win64" fn is_dialog_message_a(_dlg: usize, _msg: *const Msg) -> i32 {
    0
}

pub unsafe extern "win64" fn is_dialog_message_w(_dlg: usize, _msg: *const Msg) -> i32 {
    0
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

// =====================================================================
// Drawing helpers
// =====================================================================

pub unsafe extern "win64" fn draw_text_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    rect: *mut Rect,
    format: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let len = if count == -1 {
        let mut p = text;
        let mut n = 0;
        while !p.is_null() && *p != 0 {
            p = p.add(1);
            n += 1;
        }
        n
    } else {
        count as usize
    };
    let bytes = std::slice::from_raw_parts(text, len);
    let s = crate::dlls::decode_ansi(bytes);
    draw_text_impl(hdc, &s, &mut *rect, format)
}

pub unsafe extern "win64" fn draw_text_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    rect: *mut Rect,
    format: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    let len = if count == -1 {
        let mut p = text;
        let mut n = 0;
        while !p.is_null() && *p != 0 {
            p = p.add(1);
            n += 1;
        }
        n
    } else {
        count as usize
    };
    let chars: Vec<u16> = std::slice::from_raw_parts(text, len).to_vec();
    let s = String::from_utf16_lossy(&chars);
    draw_text_impl(hdc, &s, &mut *rect, format)
}

fn draw_text_impl(hdc: usize, text: &str, rect: &mut Rect, format: u32) -> i32 {
    const DT_CALCRECT: u32 = 0x0400;
    const DT_CENTER: u32 = 0x0001;
    const DT_VCENTER: u32 = 0x0004;
    const DT_SINGLELINE: u32 = 0x0020;
    const DT_NOCLIP: u32 = 0x0100;
    const DT_NOPREFIX: u32 = 0x0800;

    let font_handle = gui::dc_font(hdc);
    let font = gui::font_spec(font_handle).unwrap_or_default();
    let (tw, th) = gui::measure_text(&font, text);

    if format & DT_CALCRECT != 0 {
        rect.right = rect.left + tw;
        rect.bottom = rect.top + th;
        return th;
    }

    let mut x = rect.left;
    let mut y = rect.top;

    if format & DT_CENTER != 0 {
        x = rect.left + (rect.width() - tw) / 2;
    }
    if format & DT_SINGLELINE != 0 && format & DT_VCENTER != 0 {
        y = rect.top + (rect.height() - th) / 2;
    }

    let color = gui::dc_text_color(hdc);
    let bk_mode = gui::dc_bk_mode(hdc);
    let bk = if bk_mode == 2 {
        Some(gui::dc_bk_color(hdc))
    } else {
        None
    };

    let _ = (DT_NOCLIP, DT_NOPREFIX);
    gui::record(
        hdc,
        gui::DrawCmd::Text {
            x,
            y,
            text: text.to_string(),
            color,
            bk,
            font,
        },
    );

    th
}

pub extern "win64" fn get_window_dc(hwnd: usize) -> usize {
    gui::create_dc(hwnd)
}

pub unsafe extern "win64" fn get_update_rect(hwnd: usize, rect: *mut Rect, _erase: i32) -> i32 {
    // Renvoie le rectangle client comme zone de mise à jour
    if let Some(r) = gui::window_client_rect(hwnd) {
        if !rect.is_null() {
            *rect = r;
        }
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn get_update_rgn(_hwnd: usize, _rgn: usize, _erase: i32) -> i32 {
    1 // SIMPLEREGION
}

pub unsafe extern "win64" fn redraw_window(
    hwnd: usize,
    _rect: *const Rect,
    _rgn: usize,
    _flags: u32,
) -> i32 {
    gui::request_paint(hwnd);
    1
}

pub unsafe extern "win64" fn map_window_points(
    from: usize,
    to: usize,
    points: *mut Point,
    count: u32,
) -> i32 {
    if points.is_null() || count == 0 {
        return 0;
    }
    let from_offset = if from == 0 {
        (0i32, 0i32)
    } else {
        gui::window_rect(from).map_or((0, 0), |r| (r.left, r.top))
    };
    let to_offset = if to == 0 {
        (0i32, 0i32)
    } else {
        gui::window_rect(to).map_or((0, 0), |r| (r.left, r.top))
    };
    let dx = from_offset.0 - to_offset.0;
    let dy = from_offset.1 - to_offset.1;
    for i in 0..count as usize {
        let p = &mut *points.add(i);
        p.x += dx;
        p.y += dy;
    }
    (((dx as i16 as u16 as u32) << 16) | (dy as i16 as u16 as u32)) as i32
}

// =====================================================================
// System / Misc
// =====================================================================

pub unsafe extern "win64" fn system_parameters_info_a(
    action: u32,
    param: u32,
    data: *mut c_void,
    _update: u32,
) -> i32 {
    let _ = (action, param, data);
    // SPI_GETWORKAREA = 0x0030
    if action == 0x0030 && !data.is_null() {
        let rect = data as *mut Rect;
        let (w, h) = gui::screen_size();
        (*rect) = Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        return 1;
    }
    1 // Stub : succès générique
}

pub unsafe extern "win64" fn system_parameters_info_w(
    action: u32,
    param: u32,
    data: *mut c_void,
    _update: u32,
) -> i32 {
    system_parameters_info_a(action, param, data, _update)
}

pub unsafe extern "win64" fn get_window_thread_process_id(hwnd: usize, pid: *mut u32) -> u32 {
    if !pid.is_null() {
        *pid = std::process::id();
    }
    // Thread ID : on retourne un ID basé sur le propriétaire
    let _owner = gui::window_owner(hwnd);
    // Utiliser le PID comme thread ID fictif
    std::process::id()
}

pub extern "win64" fn get_active_window() -> usize {
    gui::focused_window()
}

pub extern "win64" fn set_active_window(hwnd: usize) -> usize {
    let prev = gui::focused_window();
    gui::set_focus(hwnd);
    prev
}

pub extern "win64" fn bring_window_to_top(hwnd: usize) -> i32 {
    let _ = hwnd;
    1
}

pub extern "win64" fn set_foreground_window(hwnd: usize) -> i32 {
    gui::set_focus(hwnd);
    1
}

// =====================================================================
// Hooks stubs
// =====================================================================

pub unsafe extern "win64" fn set_windows_hook_ex_a(
    _id: i32,
    _proc: usize,
    _module: usize,
    _thread: u32,
) -> usize {
    // Handle de hook factice
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x00F0_0000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub unsafe extern "win64" fn set_windows_hook_ex_w(
    _id: i32,
    _proc: usize,
    _module: usize,
    _thread: u32,
) -> usize {
    set_windows_hook_ex_a(_id, _proc, _module, _thread)
}

pub extern "win64" fn unhook_windows_hook_ex(_hook: usize) -> i32 {
    1
}

pub unsafe extern "win64" fn call_next_hook_ex(
    _hook: usize,
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let _ = (code, wparam, lparam);
    0
}

// =====================================================================
// Misc window management
// =====================================================================

pub extern "win64" fn get_window_placement(_hwnd: usize, _wp: *mut c_void) -> i32 {
    0
}

pub extern "win64" fn set_window_placement(_hwnd: usize, _wp: *const c_void) -> i32 {
    1
}

pub extern "win64" fn get_last_active_popup(hwnd: usize) -> usize {
    hwnd
}

pub unsafe extern "win64" fn load_icon_a(_instance: usize, _name: *const u8) -> usize {
    0x0001_0001
}

pub unsafe extern "win64" fn load_icon_w(_instance: usize, _name: *const u16) -> usize {
    0x0001_0001
}

pub unsafe extern "win64" fn load_image_a(
    _instance: usize,
    _name: *const u8,
    _typ: u32,
    _cx: i32,
    _cy: i32,
    _flags: u32,
) -> usize {
    0
}

pub unsafe extern "win64" fn load_image_w(
    _instance: usize,
    _name: *const u16,
    _typ: u32,
    _cx: i32,
    _cy: i32,
    _flags: u32,
) -> usize {
    0
}

pub unsafe extern "win64" fn load_string_a(
    _instance: usize,
    _id: u32,
    buf: *mut u8,
    max: i32,
) -> i32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    0
}

pub unsafe extern "win64" fn load_string_w(
    _instance: usize,
    _id: u32,
    buf: *mut u16,
    max: i32,
) -> i32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    0
}

pub unsafe extern "win64" fn load_accelerators_a(_instance: usize, _name: *const u8) -> usize {
    0
}

pub unsafe extern "win64" fn load_accelerators_w(_instance: usize, _name: *const u16) -> usize {
    0
}

pub unsafe extern "win64" fn translate_accelerator_a(
    _hwnd: usize,
    _accel: usize,
    _msg: *const Msg,
) -> i32 {
    0
}

pub unsafe extern "win64" fn translate_accelerator_w(
    _hwnd: usize,
    _accel: usize,
    _msg: *const Msg,
) -> i32 {
    0
}

pub extern "win64" fn get_keyboard_layout(_thread: u32) -> usize {
    0x0409_0409 // en-US
}

pub extern "win64" fn get_keyboard_layout_name_a(_name: *mut u8) -> i32 {
    0
}

pub extern "win64" fn get_keyboard_layout_name_w(_name: *mut u16) -> i32 {
    0
}

pub extern "win64" fn get_key_name_text_a(_lparam: i32, _buf: *mut u8, _max: i32) -> i32 {
    0
}

pub extern "win64" fn get_key_name_text_w(_lparam: i32, _buf: *mut u16, _max: i32) -> i32 {
    0
}

pub extern "win64" fn vk_key_scan_a(ch: u8) -> i16 {
    ch as i16
}

pub extern "win64" fn vk_key_scan_w(ch: u16) -> i16 {
    ch as i16
}

pub unsafe extern "win64" fn to_unicode_ex(
    vk: u32,
    scan: u32,
    state: *const u8,
    buf: *mut u16,
    buf_size: i32,
    _flags: u32,
    _hkl: usize,
) -> i32 {
    let _ = (scan, state);
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    if (0x20..=0x7E).contains(&vk) {
        *buf = vk as u16;
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn to_unicode(
    vk: u32,
    scan: u32,
    state: *const u8,
    buf: *mut u16,
    buf_size: i32,
    flags: u32,
) -> i32 {
    to_unicode_ex(vk, scan, state, buf, buf_size, flags, 0)
}

pub extern "win64" fn register_window_message_a(_name: *const u8) -> u32 {
    0xC000
}

pub extern "win64" fn register_window_message_w(_name: *const u16) -> u32 {
    0xC000
}

pub extern "win64" fn get_message_pos() -> u32 {
    0
}

pub extern "win64" fn get_message_time() -> i32 {
    now_tick() as i32
}

pub extern "win64" fn reply_message(_result: isize) -> i32 {
    1
}

pub extern "win64" fn in_send_message() -> i32 {
    0
}

pub extern "win64" fn wait_message() -> i32 {
    1
}

pub unsafe extern "win64" fn msg_wait_for_multiple_objects(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    timeout: u32,
    wake_mask: u32,
) -> u32 {
    let _ = (count, handles, wait_all, timeout, wake_mask);
    count // WAIT_OBJECT_0 + count = signalé par message
}

pub unsafe extern "win64" fn msg_wait_for_multiple_objects_ex(
    count: u32,
    handles: *const usize,
    timeout: u32,
    wake_mask: u32,
    _flags: u32,
) -> u32 {
    msg_wait_for_multiple_objects(count, handles, 0, timeout, wake_mask)
}

// ---------- Scroll bar stubs ----------

pub extern "win64" fn get_scroll_info(_hwnd: usize, _bar: i32, _info: *mut c_void) -> i32 {
    0
}

pub extern "win64" fn set_scroll_info(
    _hwnd: usize,
    _bar: i32,
    _info: *const c_void,
    _redraw: i32,
) -> i32 {
    0
}

pub extern "win64" fn get_scroll_pos(_hwnd: usize, _bar: i32) -> i32 {
    0
}

pub extern "win64" fn set_scroll_pos(_hwnd: usize, _bar: i32, _pos: i32, _redraw: i32) -> i32 {
    0
}

pub extern "win64" fn set_scroll_range(
    _hwnd: usize,
    _bar: i32,
    _min: i32,
    _max: i32,
    _redraw: i32,
) -> i32 {
    1
}

pub extern "win64" fn show_scroll_bar(_hwnd: usize, _bar: i32, _show: i32) -> i32 {
    1
}

pub extern "win64" fn enable_scroll_bar(_hwnd: usize, _bar: u32, _arrows: u32) -> i32 {
    1
}

// ---------- Caret stubs ----------

pub extern "win64" fn create_caret(_hwnd: usize, _bitmap: usize, _width: i32, _height: i32) -> i32 {
    1
}

pub extern "win64" fn destroy_caret() -> i32 {
    1
}

pub extern "win64" fn show_caret(_hwnd: usize) -> i32 {
    1
}

pub extern "win64" fn hide_caret(_hwnd: usize) -> i32 {
    1
}

pub extern "win64" fn set_caret_pos(_x: i32, _y: i32) -> i32 {
    1
}

pub unsafe extern "win64" fn get_caret_pos(point: *mut Point) -> i32 {
    if !point.is_null() {
        (*point).x = 0;
        (*point).y = 0;
    }
    1
}

pub extern "win64" fn set_caret_blink_time(_ms: u32) -> i32 {
    1
}

pub extern "win64" fn get_caret_blink_time() -> u32 {
    530
}

// ---------- Property stubs ----------

pub unsafe extern "win64" fn get_prop_a(_hwnd: usize, _name: *const u8) -> usize {
    0
}

pub unsafe extern "win64" fn get_prop_w(_hwnd: usize, _name: *const u16) -> usize {
    0
}

pub unsafe extern "win64" fn set_prop_a(_hwnd: usize, _name: *const u8, _data: usize) -> i32 {
    1
}

pub unsafe extern "win64" fn set_prop_w(_hwnd: usize, _name: *const u16, _data: usize) -> i32 {
    1
}

pub unsafe extern "win64" fn remove_prop_a(_hwnd: usize, _name: *const u8) -> usize {
    0
}

pub unsafe extern "win64" fn remove_prop_w(_hwnd: usize, _name: *const u16) -> usize {
    0
}

// ---------- Misc ----------

pub extern "win64" fn unregister_class_a(_name: *const u8, _instance: usize) -> i32 {
    1
}

pub extern "win64" fn unregister_class_w(_name: *const u16, _instance: usize) -> i32 {
    1
}

pub extern "win64" fn monitor_from_window(_hwnd: usize, _flags: u32) -> usize {
    1 // Handle de moniteur factice
}

pub extern "win64" fn monitor_from_point(_pt_x: i32, _pt_y: i32, _flags: u32) -> usize {
    1
}

pub unsafe extern "win64" fn get_monitor_info_a(monitor: usize, info: *mut c_void) -> i32 {
    let _ = monitor;
    if info.is_null() {
        return 0;
    }
    let (w, h) = gui::screen_size();
    // MONITORINFO : cbSize(4) + rcMonitor(16) + rcWork(16) + dwFlags(4) = 40 bytes
    let ptr = info as *mut u8;
    let size = *(ptr as *const u32);
    if size >= 40 {
        let rc = ptr.add(4) as *mut Rect;
        *rc = Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let work = ptr.add(20) as *mut Rect;
        *work = Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let flags = ptr.add(36) as *mut u32;
        *flags = 1; // MONITORINFOF_PRIMARY
    }
    1
}

pub unsafe extern "win64" fn get_monitor_info_w(monitor: usize, info: *mut c_void) -> i32 {
    get_monitor_info_a(monitor, info)
}

pub extern "win64" fn enum_display_monitors(
    _hdc: usize,
    _clip: *const Rect,
    _proc: usize,
    _data: isize,
) -> i32 {
    // Appeler le callback avec notre unique moniteur
    // Simplification : retourne 1 (succès) sans appeler
    1
}

pub extern "win64" fn get_dc_ex(_hwnd: usize, _rgn: usize, _flags: u32) -> usize {
    gui::create_dc(_hwnd)
}

pub extern "win64" fn begin_defer_window_pos(_count: i32) -> usize {
    1 // Handle factice
}

pub extern "win64" fn defer_window_pos(
    hdwp: usize,
    _hwnd: usize,
    _after: usize,
    _x: i32,
    _y: i32,
    _cx: i32,
    _cy: i32,
    _flags: u32,
) -> usize {
    hdwp
}

pub extern "win64" fn end_defer_window_pos(_hdwp: usize) -> i32 {
    1
}

pub extern "win64" fn char_next_a(ptr: *const u8) -> *const u8 {
    if ptr.is_null() {
        return ptr;
    }
    unsafe {
        if *ptr == 0 {
            ptr
        } else {
            ptr.add(1)
        }
    }
}

pub extern "win64" fn char_prev_a(start: *const u8, current: *const u8) -> *const u8 {
    if current <= start {
        start
    } else {
        unsafe { current.sub(1) }
    }
}

pub extern "win64" fn wsprintfa(_buf: *mut u8, _fmt: *const u8) -> i32 {
    // Très simplifié : ne format pas, retourne 0
    0
}

pub extern "win64" fn wsprintfw(_buf: *mut u16, _fmt: *const u16) -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        // --- Classes & fenêtres ---
        "RegisterClassA"=>fn register_class_a, "RegisterClassW"=>fn register_class_w,
        "RegisterClassExA"=>fn register_class_ex_a, "RegisterClassExW"=>fn register_class_ex_w,
        "UnregisterClassA"=>fn unregister_class_a, "UnregisterClassW"=>fn unregister_class_w,
        "CreateWindowExA"=>fn create_window_ex_a, "CreateWindowExW"=>fn create_window_ex_w,
        "DefWindowProcA"=>fn def_window_proc_a, "DefWindowProcW"=>fn def_window_proc_w,
        // --- Message loop ---
        "GetMessageA"=>fn get_message_a, "GetMessageW"=>fn get_message_w,
        "PeekMessageA"=>fn peek_message_a, "PeekMessageW"=>fn peek_message_w,
        "TranslateMessage"=>fn translate_message,
        "DispatchMessageA"=>fn dispatch_message_a, "DispatchMessageW"=>fn dispatch_message_w,
        "PostQuitMessage"=>fn post_quit_message,
        "PostMessageA"=>fn post_message_a, "PostMessageW"=>fn post_message_w,
        "SendMessageA"=>fn send_message_a, "SendMessageW"=>fn send_message_w,
        "RegisterWindowMessageA"=>fn register_window_message_a,
        "RegisterWindowMessageW"=>fn register_window_message_w,
        "GetMessagePos"=>fn get_message_pos, "GetMessageTime"=>fn get_message_time,
        "ReplyMessage"=>fn reply_message, "InSendMessage"=>fn in_send_message,
        "WaitMessage"=>fn wait_message,
        "MsgWaitForMultipleObjects"=>fn msg_wait_for_multiple_objects,
        "MsgWaitForMultipleObjectsEx"=>fn msg_wait_for_multiple_objects_ex,
        // --- Window state ---
        "ShowWindow"=>fn show_window,
        "UpdateWindow"=>fn update_window, "DestroyWindow"=>fn destroy_window,
        "IsWindow"=>fn is_window,
        "IsWindowVisible"=>fn is_window_visible, "IsWindowEnabled"=>fn is_window_enabled,
        "EnableWindow"=>fn enable_window,
        "IsIconic"=>fn is_iconic, "IsZoomed"=>fn is_zoomed,
        "GetWindowRect"=>fn get_window_rect, "GetClientRect"=>fn get_client_rect,
        "GetFocus"=>fn get_focus, "SetFocus"=>fn set_focus,
        "GetActiveWindow"=>fn get_active_window, "SetActiveWindow"=>fn set_active_window,
        "GetForegroundWindow"=>fn get_foreground_window,
        "SetForegroundWindow"=>fn set_foreground_window,
        "BringWindowToTop"=>fn bring_window_to_top,
        "GetDesktopWindow"=>fn get_desktop_window,
        "GetLastActivePopup"=>fn get_last_active_popup,
        // --- GetWindowLong / SetWindowLong ---
        "GetWindowLongA"=>fn get_window_long_a, "GetWindowLongW"=>fn get_window_long_w,
        "SetWindowLongA"=>fn set_window_long_a, "SetWindowLongW"=>fn set_window_long_w,
        "GetWindowLongPtrA"=>fn get_window_long_ptr_a, "GetWindowLongPtrW"=>fn get_window_long_ptr_w,
        "SetWindowLongPtrA"=>fn set_window_long_ptr_a, "SetWindowLongPtrW"=>fn set_window_long_ptr_w,
        // --- Class info ---
        "GetClassLongA"=>fn get_class_long_a, "GetClassLongW"=>fn get_class_long_w,
        "GetClassLongPtrA"=>fn get_class_long_ptr_a, "GetClassLongPtrW"=>fn get_class_long_ptr_w,
        "GetClassNameA"=>fn get_class_name_a, "GetClassNameW"=>fn get_class_name_w,
        "GetClassInfoExA"=>fn get_class_info_ex_a, "GetClassInfoExW"=>fn get_class_info_ex_w,
        // --- Window text ---
        "SetWindowTextA"=>fn set_window_text_a, "SetWindowTextW"=>fn set_window_text_w,
        "GetWindowTextA"=>fn get_window_text_a, "GetWindowTextW"=>fn get_window_text_w,
        "GetWindowTextLengthA"=>fn get_window_text_length_a,
        "GetWindowTextLengthW"=>fn get_window_text_length_w,
        // --- Window geometry ---
        "SetWindowPos"=>fn set_window_pos, "MoveWindow"=>fn move_window,
        "AdjustWindowRect"=>fn adjust_window_rect, "AdjustWindowRectEx"=>fn adjust_window_rect_ex,
        "MapWindowPoints"=>fn map_window_points,
        "ScreenToClient"=>fn screen_to_client, "ClientToScreen"=>fn client_to_screen,
        // --- Window navigation ---
        "GetParent"=>fn get_parent, "SetParent"=>fn set_parent,
        "FindWindowA"=>fn find_window_a, "FindWindowW"=>fn find_window_w,
        "FindWindowExA"=>fn find_window_ex_a, "FindWindowExW"=>fn find_window_ex_w,
        "EnumWindows"=>fn enum_windows, "EnumChildWindows"=>fn enum_child_windows,
        "GetWindow"=>fn get_window, "GetTopWindow"=>fn get_top_window,
        "GetWindowThreadProcessId"=>fn get_window_thread_process_id,
        "GetWindowPlacement"=>fn get_window_placement,
        "SetWindowPlacement"=>fn set_window_placement,
        // --- Painting ---
        "BeginPaint"=>fn begin_paint, "EndPaint"=>fn end_paint,
        "InvalidateRect"=>fn invalidate_rect, "ValidateRect"=>fn validate_rect,
        "GetDC"=>fn get_dc, "GetDCEx"=>fn get_dc_ex,
        "GetWindowDC"=>fn get_window_dc,
        "ReleaseDC"=>fn release_dc,
        "FillRect"=>fn fill_rect, "FrameRect"=>fn frame_rect,
        "DrawTextA"=>fn draw_text_a, "DrawTextW"=>fn draw_text_w,
        "GetUpdateRect"=>fn get_update_rect, "GetUpdateRgn"=>fn get_update_rgn,
        "RedrawWindow"=>fn redraw_window,
        // --- Keyboard input ---
        "GetKeyState"=>fn get_key_state, "GetAsyncKeyState"=>fn get_async_key_state,
        "GetKeyboardState"=>fn get_keyboard_state,
        "MapVirtualKeyA"=>fn map_virtual_key_a, "MapVirtualKeyW"=>fn map_virtual_key_w,
        "MapVirtualKeyExA"=>fn map_virtual_key_ex_a, "MapVirtualKeyExW"=>fn map_virtual_key_ex_w,
        "GetKeyboardLayout"=>fn get_keyboard_layout,
        "GetKeyboardLayoutNameA"=>fn get_keyboard_layout_name_a,
        "GetKeyboardLayoutNameW"=>fn get_keyboard_layout_name_w,
        "GetKeyNameTextA"=>fn get_key_name_text_a, "GetKeyNameTextW"=>fn get_key_name_text_w,
        "VkKeyScanA"=>fn vk_key_scan_a, "VkKeyScanW"=>fn vk_key_scan_w,
        "ToUnicode"=>fn to_unicode, "ToUnicodeEx"=>fn to_unicode_ex,
        // --- Mouse / Cursor ---
        "SetCapture"=>fn set_capture, "ReleaseCapture"=>fn release_capture,
        "GetCapture"=>fn get_capture,
        "GetCursorPos"=>fn get_cursor_pos, "SetCursorPos"=>fn set_cursor_pos,
        "LoadCursorA"=>fn load_cursor_a, "LoadCursorW"=>fn load_cursor_w,
        "SetCursor"=>fn set_cursor, "ShowCursor"=>fn show_cursor,
        "GetDoubleClickTime"=>fn get_double_click_time,
        "SwapMouseButton"=>fn swap_mouse_button,
        // --- Timer ---
        "SetTimer"=>fn set_timer, "KillTimer"=>fn kill_timer,
        // --- Menu ---
        "CreateMenu"=>fn create_menu, "CreatePopupMenu"=>fn create_popup_menu,
        "DestroyMenu"=>fn destroy_menu,
        "AppendMenuA"=>fn append_menu_a, "AppendMenuW"=>fn append_menu_w,
        "InsertMenuA"=>fn insert_menu_a, "InsertMenuW"=>fn insert_menu_w,
        "InsertMenuItemA"=>fn insert_menu_item_a, "InsertMenuItemW"=>fn insert_menu_item_w,
        "DeleteMenu"=>fn delete_menu, "RemoveMenu"=>fn remove_menu,
        "GetMenu"=>fn get_menu, "SetMenu"=>fn set_menu,
        "GetSubMenu"=>fn get_sub_menu, "GetSystemMenu"=>fn get_system_menu,
        "TrackPopupMenu"=>fn track_popup_menu, "TrackPopupMenuEx"=>fn track_popup_menu_ex,
        "CheckMenuItem"=>fn check_menu_item, "EnableMenuItem"=>fn enable_menu_item,
        "GetMenuItemCount"=>fn get_menu_item_count, "GetMenuItemID"=>fn get_menu_item_id,
        "SetMenuItemInfoA"=>fn set_menu_item_info_a, "SetMenuItemInfoW"=>fn set_menu_item_info_w,
        "GetMenuItemInfoA"=>fn get_menu_item_info_a, "GetMenuItemInfoW"=>fn get_menu_item_info_w,
        "DrawMenuBar"=>fn draw_menu_bar,
        // --- Dialog ---
        "DialogBoxParamA"=>fn dialog_box_param_a, "DialogBoxParamW"=>fn dialog_box_param_w,
        "CreateDialogParamA"=>fn create_dialog_param_a, "CreateDialogParamW"=>fn create_dialog_param_w,
        "GetDlgItem"=>fn get_dlg_item, "EndDialog"=>fn end_dialog,
        "SetDlgItemTextA"=>fn set_dlg_item_text_a, "SetDlgItemTextW"=>fn set_dlg_item_text_w,
        "GetDlgItemTextA"=>fn get_dlg_item_text_a, "GetDlgItemTextW"=>fn get_dlg_item_text_w,
        "GetDlgCtrlID"=>fn get_dlg_ctrl_id,
        "IsDialogMessageA"=>fn is_dialog_message_a, "IsDialogMessageW"=>fn is_dialog_message_w,
        "SendDlgItemMessageA"=>fn send_dlg_item_message_a, "SendDlgItemMessageW"=>fn send_dlg_item_message_w,
        // --- System ---
        "GetSystemMetrics"=>fn get_system_metrics,
        "GetSysColor"=>fn get_sys_color, "GetSysColorBrush"=>fn get_sys_color_brush,
        "SystemParametersInfoA"=>fn system_parameters_info_a,
        "SystemParametersInfoW"=>fn system_parameters_info_w,
        "MessageBoxA"=>fn message_box_a, "MessageBoxW"=>fn message_box_w,
        // --- Hooks ---
        "SetWindowsHookExA"=>fn set_windows_hook_ex_a, "SetWindowsHookExW"=>fn set_windows_hook_ex_w,
        "UnhookWindowsHookEx"=>fn unhook_windows_hook_ex,
        "CallNextHookEx"=>fn call_next_hook_ex,
        // --- Resources ---
        "LoadIconA"=>fn load_icon_a, "LoadIconW"=>fn load_icon_w,
        "LoadImageA"=>fn load_image_a, "LoadImageW"=>fn load_image_w,
        "LoadStringA"=>fn load_string_a, "LoadStringW"=>fn load_string_w,
        "LoadAcceleratorsA"=>fn load_accelerators_a, "LoadAcceleratorsW"=>fn load_accelerators_w,
        "TranslateAcceleratorA"=>fn translate_accelerator_a,
        "TranslateAcceleratorW"=>fn translate_accelerator_w,
        // --- Scroll ---
        "GetScrollInfo"=>fn get_scroll_info, "SetScrollInfo"=>fn set_scroll_info,
        "GetScrollPos"=>fn get_scroll_pos, "SetScrollPos"=>fn set_scroll_pos,
        "SetScrollRange"=>fn set_scroll_range,
        "ShowScrollBar"=>fn show_scroll_bar, "EnableScrollBar"=>fn enable_scroll_bar,
        // --- Caret ---
        "CreateCaret"=>fn create_caret, "DestroyCaret"=>fn destroy_caret,
        "ShowCaret"=>fn show_caret, "HideCaret"=>fn hide_caret,
        "SetCaretPos"=>fn set_caret_pos, "GetCaretPos"=>fn get_caret_pos,
        "SetCaretBlinkTime"=>fn set_caret_blink_time, "GetCaretBlinkTime"=>fn get_caret_blink_time,
        // --- Property ---
        "GetPropA"=>fn get_prop_a, "GetPropW"=>fn get_prop_w,
        "SetPropA"=>fn set_prop_a, "SetPropW"=>fn set_prop_w,
        "RemovePropA"=>fn remove_prop_a, "RemovePropW"=>fn remove_prop_w,
        // --- Monitor ---
        "MonitorFromWindow"=>fn monitor_from_window, "MonitorFromPoint"=>fn monitor_from_point,
        "GetMonitorInfoA"=>fn get_monitor_info_a, "GetMonitorInfoW"=>fn get_monitor_info_w,
        "EnumDisplayMonitors"=>fn enum_display_monitors,
        // --- Deferred window pos ---
        "BeginDeferWindowPos"=>fn begin_defer_window_pos,
        "DeferWindowPos"=>fn defer_window_pos,
        "EndDeferWindowPos"=>fn end_defer_window_pos,
        // --- Misc ---
        "CharNextA"=>fn char_next_a, "CharPrevA"=>fn char_prev_a,
        "wsprintfA"=>fn wsprintfa, "wsprintfW"=>fn wsprintfw,
    }
}
