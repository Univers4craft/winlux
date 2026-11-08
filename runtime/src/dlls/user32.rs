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

// ATOM de classe : chaque nom reçoit le sien (0xC000 et plus, comme les
// atomes de chaînes de Windows). Chromium (Electron) passe ensuite
// MAKEINTATOM(atome) à CreateWindowExW au lieu du nom.
fn class_atoms() -> &'static std::sync::Mutex<Vec<String>> {
    static ATOMS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    ATOMS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn class_atom(name: &str) -> u16 {
    let mut atoms = class_atoms().lock().unwrap_or_else(|e| e.into_inner());
    let index = match atoms.iter().position(|n| n.eq_ignore_ascii_case(name)) {
        Some(i) => i,
        None => {
            atoms.push(name.to_string());
            atoms.len() - 1
        }
    };
    0xC000u16.wrapping_add(index as u16)
}

fn class_name_of_atom(atom: usize) -> String {
    let atoms = class_atoms().lock().unwrap_or_else(|e| e.into_inner());
    atom.checked_sub(0xC000)
        .and_then(|i| atoms.get(i).cloned())
        .unwrap_or_else(|| format!("#{atom}"))
}

unsafe fn read_class_name_a(p: *const u8) -> String {
    if p.is_null() {
        String::new()
    } else if (p as usize) < 0x1_0000 {
        class_name_of_atom(p as usize)
    } else {
        crate::dlls::cstr(p)
    }
}

unsafe fn read_class_name_w(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else if (p as usize) < 0x1_0000 {
        class_name_of_atom(p as usize)
    } else {
        crate::dlls::wstr(p)
    }
}

// ---------- Enregistrement de classes ----------

/// Menu de classe (lpszMenuName) : numéro de ressource ou nom UTF-16.
enum ClassMenu {
    Id(usize),
    Name(Vec<u16>),
}

fn class_menus() -> &'static Mutex<HashMap<String, (usize, ClassMenu)>> {
    static M: OnceLock<Mutex<HashMap<String, (usize, ClassMenu)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Retient le menu d'une classe ; `wide` : nom en UTF-16.
unsafe fn remember_class_menu(class: &str, instance: usize, name: *const u8, wide: bool) {
    let key = class.to_ascii_lowercase();
    let entry = if name.is_null() {
        None
    } else if (name as usize) >> 16 == 0 {
        Some(ClassMenu::Id(name as usize))
    } else if wide {
        Some(ClassMenu::Name(
            crate::dlls::wstr(name as *const u16)
                .encode_utf16()
                .chain([0])
                .collect(),
        ))
    } else {
        Some(ClassMenu::Name(
            crate::dlls::cstr(name).encode_utf16().chain([0]).collect(),
        ))
    };
    let mut map = lock(class_menus());
    match entry {
        Some(m) => {
            map.insert(key, (instance, m));
        }
        None => {
            map.remove(&key);
        }
    }
}

/// Charge le menu de la classe d'une fenêtre de premier niveau.
unsafe fn load_class_menu(class: &str) -> usize {
    let map = lock(class_menus());
    let Some((inst, m)) = map.get(&class.to_ascii_lowercase()) else {
        return 0;
    };
    let name = match m {
        ClassMenu::Id(id) => *id,
        ClassMenu::Name(v) => v.as_ptr() as usize,
    };
    let inst = *inst;
    let Some(load) = crate::dlls::lookup("user32", "LoadMenuW") else {
        return 0;
    };
    let f: unsafe extern "win64" fn(usize, usize) -> usize = std::mem::transmute(load);
    f(inst, name)
}

unsafe fn register_class_common(
    name: String,
    style: u32,
    wnd_proc: usize,
    background: usize,
    instance: usize,
) -> usize {
    if name.is_empty() || wnd_proc == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    // Classe déjà enregistrée : Win32 renvoie une erreur mais garde la
    // classe ; on rend son atome.
    gui::set_class_style(&name, style);
    gui::register_class(
        &name,
        ClassInfo {
            wnd_proc,
            background,
            instance,
        },
    );
    class_atom(&name) as usize
}

pub unsafe extern "win64" fn register_class_a(wc: *const WndClassA) -> u16 {
    if wc.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wc;
    let name = read_class_name_a(c.class_name);
    remember_class_menu(&name, c.instance, c.menu_name, false);
    register_class_common(name, c.style, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_w(wc: *const WndClassA) -> u16 {
    // La disposition est identique ; seul le nom de classe change (UTF-16).
    if wc.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wc;
    let name = read_class_name_w(c.class_name as *const u16);
    remember_class_menu(&name, c.instance, c.menu_name, true);
    register_class_common(name, c.style, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_ex_a(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_a(c.class_name);
    remember_class_menu(&name, c.instance, c.menu_name, false);
    register_class_common(name, c.style, c.wnd_proc, c.background, c.instance) as u16
}

pub unsafe extern "win64" fn register_class_ex_w(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_w(c.class_name as *const u16);
    remember_class_menu(&name, c.instance, c.menu_name, true);
    register_class_common(name, c.style, c.wnd_proc, c.background, c.instance) as u16
}

// ---------- Création de fenêtres ----------

/// `WINLUX_WINDOW_DIAG=1` : création, placement et affichage des fenêtres
/// sur la sortie d'erreur (diagnostic des fenêtres filles absentes).
fn window_diag() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("WINLUX_WINDOW_DIAG").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

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
    menu: usize,
    create_params: *mut c_void,
) -> usize {
    let diag = window_diag().then(|| format!("{class:?} {title:?} style={style:#x} ex={ex_style:#x} ({x},{y} {cx}x{cy}) parent={parent:#x}"));
    let hwnd = create_window_inner(
        class,
        title,
        style,
        ex_style,
        x,
        y,
        cx,
        cy,
        parent,
        instance,
        menu,
        create_params,
    );
    if let Some(d) = diag {
        eprintln!("winlux[fenetre] creer {d} -> {hwnd:#x}");
    }
    hwnd
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_window_inner(
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
    menu: usize,
    create_params: *mut c_void,
) -> usize {
    let Some(info) = gui::lookup_class(&class) else {
        crate::process::set_last_error(1407); // ERROR_CLASS_DOES_NOT_EXIST
        return 0;
    };
    // Comme Windows : une fenêtre WS_CHILD sans parent est refusée, au lieu
    // de devenir une fenêtre isolée sans titre dans la barre des tâches.
    if style & WS_CHILD != 0 && parent == 0 {
        crate::process::set_last_error(1406); // ERROR_TLW_WITH_WSCHILD
        return 0;
    }
    let hwnd = gui::alloc_hwnd();
    // Tailles CW_USEDEFAULT : fenêtre raisonnable par défaut. Un contrôle
    // enfant garde la taille demandée, même nulle (barre d'état, flèches
    // haut/bas : le contrôle se place lui-même pendant WM_CREATE).
    let child = style & WS_CHILD != 0 && parent != 0;
    let (w, h, px, py) = if child {
        let size = |v: i32| if v == gui::CW_USEDEFAULT { 0 } else { v.max(0) };
        let pos = |v: i32| if v == gui::CW_USEDEFAULT { 0 } else { v };
        (size(cx), size(cy), pos(x), pos(y))
    } else {
        (
            if cx == gui::CW_USEDEFAULT || cx <= 0 {
                800
            } else {
                cx
            },
            if cy == gui::CW_USEDEFAULT || cy <= 0 {
                600
            } else {
                cy
            },
            if x == gui::CW_USEDEFAULT { 100 } else { x },
            if y == gui::CW_USEDEFAULT { 100 } else { y },
        )
    };
    let rect = Rect::from_size(px, py, w, h);
    let visible = style & 0x1000_0000 != 0; // WS_VISIBLE

    // Un contrôle enfant reçoit son identifiant dans hMenu (GetDlgItem).
    if style & WS_CHILD != 0 {
        set_control_id(hwnd, menu as isize);
    }
    let class_z: Vec<u8> = class.bytes().chain([0]).collect();
    let title_z: Vec<u8> = title.bytes().chain([0]).collect();
    let cs = CreateStructA {
        create_params,
        instance,
        menu,
        parent,
        cy: h,
        cx: w,
        y: py,
        x: px,
        style: style as i32,
        name: title_z.as_ptr().cast(),
        class: class_z.as_ptr().cast(),
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
            menu_bar: 0,
            painting: false,
        },
    );

    // La fenêtre X existe avant WM_CREATE, comme la HWND sous Windows : un
    // programme y crée souvent ses enfants (panneaux, barres d'outils de
    // 7-Zip). Créée après, chaque enfant serait devenu une fenêtre isolée,
    // sans titre, dans la barre des tâches, et le parent serait resté blanc.
    // Elle n'est montrée qu'après WM_CREATE.
    with_backend(|b| {
        if child {
            b.create_child_window(hwnd, parent, rect);
            // Win32 : un nouvel enfant passe derrière ses voisins (le premier
            // créé reste devant). Les contrôles de fond (cadres de groupe,
            // onglets), transparents sous Windows mais opaques en X11,
            // restent tout au fond pour ne jamais masquer leur contenu.
            b.lower_window(hwnd);
            for sibling in gui::child_windows(parent) {
                if sibling != hwnd && super::controls::is_backdrop(sibling) {
                    b.lower_window(sibling);
                }
            }
        } else {
            b.create_window(hwnd, &title, rect);
            if class.eq_ignore_ascii_case(super::controls::popup::CLASS) {
                // Menu surgissant : ni cadre ni placement du gestionnaire.
                b.set_popup(hwnd);
            } else if let Some(level) = popup_decorations(style) {
                b.set_decorations(hwnd, level);
            }
            // Fenêtre possédée : toujours au-dessus de sa propriétaire, comme
            // sous Windows (boîte « Vie privée » de VLC, #1068).
            let owner = owner_top_level(parent);
            if owner != 0 {
                b.set_owner(hwnd, owner);
            }
        }
    });

    // Barre de menus d'une fenêtre de premier niveau : hMenu, sinon le
    // menu de la classe (Notepad++ le déclare dans WNDCLASSEX).
    if !child {
        let bar = if menu != 0 {
            menu
        } else {
            load_class_menu(&class)
        };
        if bar != 0 {
            if let Some(f) = crate::dlls::lookup("user32", "SetMenu") {
                let set: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(f);
                set(hwnd, bar);
            }
        }
    }

    // WM_NCCREATE puis WM_CREATE, synchrones : beaucoup de programmes y
    // rangent leur objet C++ (lpCreateParams) ; FALSE ou -1 refuse la fenêtre.
    // La procédure est relue avant chaque message : un programme peut sous-
    // classer la fenêtre dès WM_NCCREATE (WinForms y appelle AssignHandle,
    // puis SetWindowLongPtr(GWLP_WNDPROC) ; lui renvoyer WM_CREATE par la
    // procédure de classe lève « Window handle already exists. », #666).
    let current_proc = || gui::wnd_proc_of(hwnd).unwrap_or(info.wnd_proc);
    let nc = crate::dlls::gui::call_wndproc(
        current_proc(),
        hwnd,
        WM_NCCREATE,
        0,
        &cs as *const CreateStructA as isize,
    );
    if nc == 0 {
        gui::destroy_window(hwnd);
        forget_window_longs(hwnd);
        return 0;
    }
    // Procédure actuelle de la fenêtre, pas celle de la classe : WinForms
    // sous-classe la fenêtre (SetWindowLongPtr(GWLP_WNDPROC)) dès le
    // premier message et refuse d'en recevoir un second par la procédure de
    // classe (« Le handle de fenêtre existe déjà », #666).
    let created = crate::dlls::gui::call_wndproc(
        current_proc(),
        hwnd,
        gui::WM_CREATE,
        0,
        &cs as *const CreateStructA as isize,
    );
    if created == -1 {
        gui::destroy_window(hwnd);
        return 0;
    }

    // WM_CREATE a pu déplacer la fenêtre ou la montrer (SetWindowPos,
    // ShowWindow) : la fenêtre X suit sa place actuelle.
    // (Certains contrôles changent leur taille sans SetWindowPos, comme la
    // COMBOBOX fermée.)
    let initial = rect;
    let rect = gui::window_rect(hwnd).unwrap_or(rect);
    let shown = gui::is_window_visible(hwnd);
    // WM_SIZE donne la zone client : la barre de menus en est retirée.
    let (w, h) = (
        rect.width(),
        (rect.height() - gui::menu_bar_height(hwnd)).max(0),
    );
    with_backend(|b| {
        if rect != initial {
            b.set_window_rect(hwnd, rect);
        }
        if shown {
            b.show_window(hwnd, true);
        }
    });
    let visible = shown;

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
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    super::controls::with_charset(false, || {
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
            menu,
            param,
        )
    })
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
    if class.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    super::controls::with_charset(true, || {
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
            menu,
            param,
        )
    })
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
    let r = get_message_inner(out, hwnd, min, max);
    // GetMessage retire toujours le message : les crochets WH_GETMESSAGE le
    // voient avec PM_REMOVE, y compris WM_QUIT.
    call_get_message_hooks(out, PM_REMOVE);
    r
}

unsafe fn get_message_inner(out: *mut Msg, hwnd: usize, min: u32, max: u32) -> i32 {
    loop {
        gui::pump_backend();
        super::ws2_32::pump_async();
        let q = gui::current_queue();
        // Un minuteur très en retard passe avant les entrées (anti-famine).
        if let Some(m) = gui::due_timer(hwnd, min, max, true) {
            write_msg(out, &m);
            return 1;
        }
        // WM_QUIT (posted par PostQuitMessage) termine la boucle (renvoie 0).
        if let Some(m) = gui::take_message(&q, hwnd, min, max) {
            if m.message == gui::WM_QUIT {
                // Win32 renseigne le MSG (wParam = code de sortie) avant de
                // renvoyer 0 : `return msg.wParam` dans l'appli lit cette valeur.
                write_msg(out, &m);
                return 0;
            }
            gui::observe_key_message(&m);
            write_msg(out, &m);
            return 1;
        }
        // File vide : une zone encore invalidée (pas de BeginPaint) reçoit
        // de nouveau WM_PAINT, comme sous Windows. En headless, la boucle
        // vide doit se terminer : pas de réémission sans fin.
        if gui::backend_is_interactive() {
            if let Some(m) = gui::pending_paint(hwnd, min, max) {
                write_msg(out, &m);
                return 1;
            }
        }
        // File vide : en headless on rend la main (fin de boucle); avec un
        // backend réel on dort jusqu'au prochain événement hôte, dépôt d'un
        // autre thread ou minuteur (borne de sûreté : 500 ms).
        // Sockets WSAAsyncSelect : sondées toutes les 10 ms au plus.
        let cap = if super::ws2_32::async_active() {
            10
        } else {
            500
        };
        let wait = gui::next_timer_ms(hwnd).map_or(cap, |ms| ms.min(cap));
        if !gui::wait_backend(wait) {
            // Fin de boucle headless : comme pour un vrai WM_QUIT, le MSG est
            // renseigné (wParam 0). Sinon `ExitProcess(msg.wParam)` reprenait
            // le wParam du dernier message, l'identifiant d'un minuteur par
            // exemple, et le programme sortait au hasard avec le code 1.
            write_msg(
                out,
                &gui::Msg {
                    hwnd: 0,
                    message: gui::WM_QUIT,
                    wparam: 0,
                    lparam: 0,
                    time: 0,
                    pt: Default::default(),
                    l_private: 0,
                },
            );
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
    let r = peek_message_inner(out, hwnd, min, max, remove);
    if r != 0 {
        call_get_message_hooks(out, (remove & PM_REMOVE as u32) as usize);
    }
    r
}

unsafe fn peek_message_inner(out: *mut Msg, hwnd: usize, min: u32, max: u32, remove: u32) -> i32 {
    gui::pump_backend();
    super::ws2_32::pump_async();
    let q = gui::current_queue();
    if remove & 0x1 != 0 {
        if let Some(m) = gui::take_message(&q, hwnd, min, max) {
            if m.message == gui::WM_QUIT {
                // Comme Win32 : le MSG est rempli et la fonction renvoie « vrai »
                // afin que la boucle classique `while (PeekMessage(...)) { if
                // (msg.message == WM_QUIT) break; }` voie bien WM_QUIT. Le
                // message reste en tête de file pour qu'un GetMessage ultérieur
                // termine lui aussi.
                write_msg(out, &m);
                gui::prepend_message(&q, m);
                return 1;
            }
            gui::observe_key_message(&m);
            write_msg(out, &m);
            return 1;
        }
    } else if let Some(m) = gui::peek_message(&q, hwnd, min, max) {
        write_msg(out, &m);
        return 1;
    }
    // File vide : WM_PAINT tant que la fenêtre n'a pas validé sa zone.
    if let Some(m) = gui::pending_paint(hwnd, min, max) {
        write_msg(out, &m);
        return 1;
    }
    // Puis WM_TIMER, comme GetMessage : sans lui, un programme qui attend
    // par MsgWaitForMultipleObjects (réveillé par le minuteur échu) puis
    // vide sa file par PeekMessage tournerait sans fin (PuTTY).
    let timer = if remove & 0x1 != 0 {
        gui::due_timer(hwnd, min, max, false)
    } else {
        gui::peek_due_timer(hwnd, min, max)
    };
    if let Some(m) = timer {
        write_msg(out, &m);
        return 1;
    }
    0
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
        if let Some(prefix) = gui::take_key_prefix(m.hwnd, m.wparam, m.lparam) {
            gui::post_to_window(
                m.hwnd,
                gui::Msg::new(m.hwnd, gui::WM_CHAR, prefix as usize, m.lparam),
            );
        }
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
    if super::dataobject::dispatch_private(m.hwnd, m.message, m.wparam) {
        return 0;
    }
    // WM_TIMER d'un minuteur à fonction de rappel : lParam est la TIMERPROC,
    // appelée à la place de la procédure de fenêtre.
    if m.message == 0x0113 && m.lparam != 0 {
        let callback: unsafe extern "win64" fn(usize, u32, usize, u32) =
            std::mem::transmute(m.lparam as usize);
        callback(m.hwnd, m.message, m.wparam, m.time);
        return 0;
    }
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
    super::controls::with_charset(false, || send_message(hwnd, message, wparam, lparam))
}

unsafe fn send_message(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> isize {
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
    super::controls::with_charset(true, || send_message(hwnd, message, wparam, lparam))
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
    if window_diag() {
        eprintln!("winlux[fenetre] afficher {hwnd:#x} cmd={cmd}");
    }
    let was = gui::set_visible(hwnd, cmd != gui::SW_HIDE);
    // SW_SHOWMINNOACTIVE (7), SW_SHOWNA (8), SW_SHOWNOACTIVATE (4) : la
    // fenêtre s'affiche sans prendre le focus (#1127).
    if matches!(cmd, 4 | 7 | 8) && !was {
        with_backend(|b| b.set_no_initial_focus(hwnd));
    }
    with_backend(|b| b.show_window(hwnd, cmd != gui::SW_HIDE));
    // Réduire, agrandir, restaurer : demandé au gestionnaire de fenêtres
    // (fenêtres de premier niveau seulement).
    if gui::window_parent(hwnd).unwrap_or(0) == 0 {
        let state = match cmd {
            2 | 6 | 7 | 11 => Some(gui::WINDOW_MINIMIZED),
            3 => Some(gui::WINDOW_MAXIMIZED),
            9 => Some(gui::WINDOW_NORMAL),
            _ => None,
        };
        if let Some(state) = state {
            with_backend(|b| b.set_window_state(hwnd, state));
        }
    }
    if cmd != gui::SW_HIDE {
        gui::request_paint(hwnd);
    }
    was as i32
}

/// `UpdateWindow` : si la fenêtre a une zone invalidée, WM_PAINT lui est
/// envoyé tout de suite, hors file, comme sous Windows ; sinon rien. Avant
/// (#796), on invalidait toujours la fenêtre et on attendait la file : le
/// « Refresh(); Update(); » que FileZilla fait dans le WM_PAINT de ses
/// listes de chemins (avec un drapeau anti-réentrance) relançait alors une
/// peinture à chaque tour, sans fin, et les minuteurs n'étaient plus servis.
pub unsafe extern "win64" fn update_window(hwnd: usize) -> i32 {
    thread_local! {
        static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }
    if gui::wnd_proc_of(hwnd).is_none() {
        return 0;
    }
    // Une peinture qui se relance elle-même sans garde ne doit pas épuiser
    // la pile : au-delà de quelques niveaux, la file prend le relais.
    if gui::paint_pending_here(hwnd) && DEPTH.with(|d| d.get()) < 4 {
        DEPTH.with(|d| d.set(d.get() + 1));
        send_message_w(hwnd, gui::WM_PAINT, 0, 0);
        DEPTH.with(|d| d.set(d.get() - 1));
    }
    1
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
    super::dialogs::note_focus(hwnd);
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
    match gui::screen_rect(hwnd) {
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
    if window_diag() {
        eprintln!("winlux[fenetre] placer {hwnd:#x} ({x},{y} {cx}x{cy}) flags={flags:#x}");
    }
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
    // Windows accepte une taille nulle et ramène une taille négative à 0 :
    // wxWidgets remet ainsi ses contrôles à 0×0 avant de les placer.
    let (width, mut height) = (width.max(0), height.max(0));
    if flags & SWP_NOSIZE == 0 {
        if let Some(h) = super::controls::forced_height(hwnd, height) {
            height = h;
        }
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
                Msg::new(
                    hwnd,
                    gui::WM_SIZE,
                    0,
                    pack_i16_pair(width, (height - gui::menu_bar_height(hwnd)).max(0)),
                ),
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
        super::controls::text_changed(hwnd);
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn set_window_text_w(hwnd: usize, text: *const u16) -> i32 {
    let t = read_class_name_w(text);
    if gui::set_title(hwnd, &t) {
        with_backend(|b| b.set_title(hwnd, &t));
        super::controls::text_changed(hwnd);
        1
    } else {
        0
    }
}

pub unsafe extern "win64" fn get_window_text_a(hwnd: usize, buf: *mut u8, cap: i32) -> i32 {
    match gui::title_of(hwnd) {
        Some(t) => crate::dlls::put_text_a(&t, buf, cap.max(0) as u32) as i32,
        None => 0,
    }
}

pub unsafe extern "win64" fn get_window_text_w(hwnd: usize, buf: *mut u16, cap: i32) -> i32 {
    match gui::title_of(hwnd) {
        Some(t) => crate::dlls::put_text_w(&t, buf, cap.max(0) as u32) as i32,
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
    gui::flush_paint_over(hwnd);
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

pub unsafe extern "win64" fn validate_rect(hwnd: usize, _rect: *const Rect) -> i32 {
    // Toute la fenêtre est validée (pas de régions partielles) : plus de WM_PAINT
    // tant qu'elle n'est pas invalidée à nouveau.
    gui::validate_window(hwnd) as i32
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
        // SM_CXSCREEN/SM_CYSCREEN : écran principal, comme Windows.
        0 | 16 => crate::dlls::monitors::list()[0].rect.width(),
        1 | 17 => crate::dlls::monitors::list()[0].rect.height(),
        // Écran virtuel (tous les écrans) et nombre d'écrans.
        76 => crate::dlls::monitors::virtual_rect().left,
        77 => crate::dlls::monitors::virtual_rect().top,
        78 => crate::dlls::monitors::virtual_rect().width(),
        79 => crate::dlls::monitors::virtual_rect().height(),
        80 => crate::dlls::monitors::count(), // SM_CMONITORS
        81 => 1,                              // SM_SAMEDISPLAYFORMAT
        2 => 4,                               // SM_CYCAPTION
        3 => 4,                               // SM_CXBORDER
        4 => 4,                               // SM_CYBORDER
        7 => 32,                              // SM_CYHSCROLL
        8 => 16,                              // SM_CYVSCROLL
        19 => 1,                              // SM_MOUSEPRESENT
        23 => i32::from(crate::dlls::mouse::current().swap_buttons), // SM_SWAPBUTTON
        36 | 37 => 4,                         // SM_CXDOUBLECLK / SM_CYDOUBLECLK
        43 => 3,                              // SM_CMOUSEBUTTONS
        75 => 1,                              // SM_MOUSEWHEELPRESENT
        // Icônes : 32 px, petite icône 16 px.
        11 | 12 => 32, // SM_CXICON / SM_CYICON
        49 | 50 => 16, // SM_CXSMICON / SM_CYSMICON
        // Taille minimale d'une fenêtre et de son suivi au redimensionnement.
        28 | 34 => 136, // SM_CXMIN / SM_CXMINTRACK
        29 | 35 => 39,  // SM_CYMIN / SM_CYMINTRACK
        // Taille maximale de suivi (bureau virtuel + cadre) et fenêtre
        // agrandie. WinForms borne la taille d'un Form par SM_CXMAXTRACK :
        // à 0, chaque Form était créé en 0×0 (#666).
        59 => crate::dlls::monitors::virtual_rect().width() + 12, // SM_CXMAXTRACK
        60 => crate::dlls::monitors::virtual_rect().height() + 12, // SM_CYMAXTRACK
        61 => crate::dlls::monitors::list()[0].rect.width() + 16, // SM_CXMAXIMIZED
        62 => crate::dlls::monitors::list()[0].rect.height() + 16, // SM_CYMAXIMIZED
        // SM_CXMENUCHECK / SM_CYMENUCHECK : taille de la case des cases à
        // cocher et boutons radio à 96 ppp. wxWidgets l'ajoute à la largeur
        // du libellé ; à 0, ses libellés finissaient en « … » (#842).
        71 | 72 => 15,
        90 => 96, // LOGPIXELSX
        91 => 96, // LOGPIXELSY
        _ => 0,
    }
}

/// Couleur système, remplacée par le thème de contraste élevé s'il est actif.
fn system_color(index: i32) -> Option<u32> {
    crate::dlls::a11y::system_color(crate::dlls::a11y::current().contrast, index)
        .or_else(|| gui::theme::system_color(index))
}

pub extern "win64" fn get_sys_color(index: i32) -> u32 {
    system_color(index).unwrap_or(0)
}

pub extern "win64" fn get_sys_color_brush(index: i32) -> usize {
    let Some(color) = system_color(index) else {
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

/// Station de fenêtres du processus : une seule, interactive (« WinSta0 »).
const WINSTA0: usize = 0x5753_0001;

pub extern "win64" fn get_process_window_station() -> usize {
    WINSTA0
}

/// GetUserObjectInformation(A/W) : UOI_FLAGS (1) indique une station
/// visible (WSF_VISIBLE), UOI_NAME (2) son nom.
unsafe fn user_object_information(
    obj: usize,
    index: i32,
    out: *mut u8,
    len: u32,
    needed: *mut u32,
    wide: bool,
) -> i32 {
    if obj == 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    let data: Vec<u8> = match index {
        1 => [0u32, 0, 1].iter().flat_map(|v| v.to_le_bytes()).collect(),
        2 => {
            let name = if obj == WINSTA0 { "WinSta0" } else { "Default" };
            if wide {
                name.encode_utf16()
                    .chain(Some(0))
                    .flat_map(|c| c.to_le_bytes())
                    .collect()
            } else {
                name.bytes().chain(Some(0)).collect()
            }
        }
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    if !needed.is_null() {
        *needed = data.len() as u32;
    }
    if out.is_null() || (len as usize) < data.len() {
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), out, data.len());
    1
}
pub unsafe extern "win64" fn get_user_object_information_w(
    obj: usize,
    index: i32,
    out: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    user_object_information(obj, index, out, len, needed, true)
}
pub unsafe extern "win64" fn get_user_object_information_a(
    obj: usize,
    index: i32,
    out: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    user_object_information(obj, index, out, len, needed, false)
}

/// CallWindowProc(A/W) : appelle la procédure de fenêtre précédente
/// (sous-classement par SetWindowLongPtr(GWLP_WNDPROC)).
pub unsafe extern "win64" fn call_window_proc(
    prev: usize,
    hwnd: usize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if prev == 0 {
        return 0;
    }
    gui::call_wndproc(prev, hwnd, msg, wparam, lparam)
}

/// `MessageBeep` : son système Winlux correspondant à l'icône (erreur,
/// question, avertissement, information ou son par défaut). Comme Windows,
/// la fonction réussit même sans carte son.
pub unsafe extern "win64" fn message_beep(kind: u32) -> i32 {
    crate::dlls::winmm::play_system_sound(crate::dlls::winmm::system_sound_for_beep(kind));
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
const GWLP_HWNDPARENT: i32 = -8;
const _GWLP_WNDPROC: i32 = -4;
const _GWLP_HINSTANCE: i32 = -6;
const _GWLP_USERDATA: i32 = -21;
const _GWLP_ID: i32 = -12;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// GWLP_USERDATA, GWLP_ID et données de fenêtre (cbWndExtra, DWLP_* des
/// dialogues), par fenêtre et par index. Les HWND se suivent : une clé
/// « hwnd + index » mélangeait les données de fenêtres voisines.
fn user_data_map() -> &'static Mutex<HashMap<(usize, i32), isize>> {
    static M: OnceLock<Mutex<HashMap<(usize, i32), isize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

const WS_CHILD: u32 = 0x4000_0000;
const WM_NCCREATE: u32 = 0x0081;

fn set_control_id(hwnd: usize, id: isize) {
    lock(user_data_map()).insert((hwnd, GWL_ID), id);
}

/// Identifiant de contrôle d'une fenêtre enfant (0 s'il n'en a pas).
pub fn control_id(hwnd: usize) -> isize {
    lock(user_data_map())
        .get(&(hwnd, GWL_ID))
        .copied()
        .unwrap_or(0)
}

/// Efface les données d'une fenêtre détruite.
pub fn forget_window_longs(hwnd: usize) {
    lock(user_data_map()).retain(|(h, _), _| *h != hwnd);
}

use super::lock;

unsafe fn get_window_long_impl(hwnd: usize, index: i32) -> isize {
    match index {
        GWL_STYLE => gui::window_style(hwnd).unwrap_or(0) as isize,
        GWL_EXSTYLE => gui::window_ex_style(hwnd).unwrap_or(0) as isize,
        GWL_WNDPROC => gui::wnd_proc_of(hwnd).unwrap_or(0) as isize,
        GWL_HINSTANCE => 0x0040_0000_isize,
        // Parent d'une fille, propriétaire d'une fenêtre de premier niveau.
        GWLP_HWNDPARENT => lock(user_data_map())
            .get(&(hwnd, index))
            .copied()
            .unwrap_or_else(|| gui::window_parent(hwnd).unwrap_or(0) as isize),
        // GWLP_USERDATA, GWLP_ID et index positifs : données de la fenêtre
        // (cbWndExtra ; DWLP_MSGRESULT 0, DWLP_DLGPROC 8, DWLP_USER 16 en x64).
        GWL_USERDATA | GWL_ID => lock(user_data_map())
            .get(&(hwnd, index))
            .copied()
            .unwrap_or(0),
        _ if index >= 0 => lock(user_data_map())
            .get(&(hwnd, index))
            .copied()
            .unwrap_or(0),
        _ => 0,
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
        GWL_USERDATA | GWL_ID => {
            lock(user_data_map()).insert((hwnd, index), value);
        }
        // Qt 5 donne son propriétaire à une boîte après la création, par
        // SetWindowLongPtr(GWLP_HWNDPARENT) : la boîte « Vie privée » de VLC
        // doit alors rester au-dessus de la fenêtre principale (#1068).
        GWLP_HWNDPARENT if gui::window_style(hwnd).is_some_and(|s| s & WS_CHILD == 0) => {
            lock(user_data_map()).insert((hwnd, index), value);
            let owner = owner_top_level(value as usize);
            if owner != 0 && owner != hwnd {
                with_backend(|b| b.set_owner(hwnd, owner));
            }
        }
        _ => {
            if index >= 0 {
                lock(user_data_map()).insert((hwnd, index), value);
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
        crate::dlls::put_text_a(&name, buf, max_count.max(0) as u32) as i32
    } else {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        0
    }
}

pub unsafe extern "win64" fn get_class_name_w(hwnd: usize, buf: *mut u16, max_count: i32) -> i32 {
    if let Some(name) = gui::window_class(hwnd) {
        crate::dlls::put_text_w(&name, buf, max_count.max(0) as u32) as i32
    } else {
        crate::process::set_last_error(1400);
        0
    }
}

/// RealGetWindowClass : le type de fenêtre. Winlux ne superclasse pas les
/// contrôles prédéfinis, donc c'est le nom de classe, comme GetClassName.
/// Le sélecteur de fichiers de Qt (VLC) l'appelle à la fermeture.
pub unsafe extern "win64" fn real_get_window_class_w(hwnd: usize, buf: *mut u16, max: u32) -> u32 {
    get_class_name_w(hwnd, buf, max.min(i32::MAX as u32) as i32) as u32
}

pub unsafe extern "win64" fn real_get_window_class_a(hwnd: usize, buf: *mut u8, max: u32) -> u32 {
    get_class_name_a(hwnd, buf, max.min(i32::MAX as u32) as i32) as u32
}

/// Remplit un WNDCLASSEX (A ou W, même disposition en x64) pour la classe
/// `name`, enregistrée ou prédéfinie (BUTTON, EDIT…). WinForms en dérive ses
/// classes « WindowsForms10.BUTTON… » par GetClassInfo (#666).
unsafe fn fill_class_info_ex(name: &str, name_ptr: usize, out: *mut u8) -> i32 {
    const ERROR_CLASS_DOES_NOT_EXIST: u32 = 1411;
    if out.is_null() || name.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(info) = gui::lookup_class(name) else {
        crate::process::set_last_error(ERROR_CLASS_DOES_NOT_EXIST);
        return 0;
    };
    let mut style = gui::class_style(name);
    if style == 0 && crate::dlls::controls::builtin_class(name).is_some() {
        // Contrôles prédéfinis : CS_DBLCLKS | CS_PARENTDC, plus
        // CS_VREDRAW | CS_HREDRAW, comme BUTTON sous Windows.
        style = 0x0008 | 0x0080 | 0x0001 | 0x0002;
    }
    // WNDCLASSEX x64 : cbSize(0) style(4) lpfnWndProc(8) cbClsExtra(16)
    // cbWndExtra(20) hInstance(24) hIcon(32) hCursor(40) hbrBackground(48)
    // lpszMenuName(56) lpszClassName(64) hIconSm(72). cbSize est laissé.
    std::ptr::write_bytes(out.add(4), 0, 76);
    out.add(4).cast::<u32>().write_unaligned(style);
    out.add(8).cast::<usize>().write_unaligned(info.wnd_proc);
    out.add(24).cast::<usize>().write_unaligned(info.instance);
    out.add(48).cast::<usize>().write_unaligned(info.background);
    out.add(64).cast::<usize>().write_unaligned(name_ptr);
    class_atom(name) as i32
}

pub unsafe extern "win64" fn get_class_info_ex_a(
    _instance: usize,
    name: *const u8,
    out: *mut u8,
) -> i32 {
    fill_class_info_ex(&read_class_name_a(name), name as usize, out)
}

pub unsafe extern "win64" fn get_class_info_ex_w(
    _instance: usize,
    name: *const u16,
    out: *mut u8,
) -> i32 {
    fill_class_info_ex(&read_class_name_w(name), name as usize, out)
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
    menu: i32,
    _ex_style: u32,
) -> i32 {
    if rect.is_null() {
        return 0;
    }
    // Une fenêtre de premier niveau n'a pas de cadre à elle : la barre de
    // titre et les bords sont ceux du gestionnaire de fenêtres, hors du
    // rectangle (GetWindowRect = zone client + barre de menus). Annoncer des
    // marges faisait calculer à Qt (VLC) une zone client plus petite que la
    // fenêtre, avec une bande noire à droite et en bas.
    const WS_CHILD: u32 = 0x4000_0000;
    if style & WS_CHILD == 0 {
        if menu != 0 {
            (*rect).top -= crate::dlls::menus::BAR_HEIGHT;
        }
        return 1;
    }
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

const WS_DISABLED: u32 = 0x0800_0000;

pub unsafe extern "win64" fn is_window_enabled(hwnd: usize) -> i32 {
    gui::window_style(hwnd).is_some_and(|s| s & WS_DISABLED == 0) as i32
}

/// Active ou désactive une fenêtre ; renvoie vrai si elle était désactivée.
pub unsafe extern "win64" fn enable_window(hwnd: usize, enable: i32) -> i32 {
    let Some(style) = gui::window_style(hwnd) else {
        return 0;
    };
    let was_disabled = style & WS_DISABLED != 0;
    let new = if enable != 0 {
        style & !WS_DISABLED
    } else {
        style | WS_DISABLED
    };
    if new != style {
        gui::set_window_style(hwnd, new);
        if enable == 0 && gui::focused_window() == hwnd {
            set_focus(0);
        }
        send_message_w(hwnd, 0x000A, (enable != 0) as usize, 0); // WM_ENABLE
        invalidate_rect(hwnd, std::ptr::null(), 1);
    }
    was_disabled as i32
}

pub unsafe extern "win64" fn is_iconic(hwnd: usize) -> i32 {
    let mut state = gui::WINDOW_NORMAL;
    with_backend(|b| state = b.window_state(hwnd));
    (state == gui::WINDOW_MINIMIZED) as i32
}

pub unsafe extern "win64" fn is_zoomed(hwnd: usize) -> i32 {
    let mut state = gui::WINDOW_NORMAL;
    with_backend(|b| state = b.window_state(hwnd));
    (state == gui::WINDOW_MAXIMIZED) as i32
}

// =====================================================================
// GetParent / SetParent
// =====================================================================

pub extern "win64" fn get_parent(hwnd: usize) -> usize {
    gui::window_parent(hwnd).unwrap_or(0)
}

pub extern "win64" fn set_parent(hwnd: usize, new_parent: usize) -> usize {
    let old = gui::set_window_parent(hwnd, new_parent).unwrap_or(0);
    if new_parent != 0 && new_parent != old {
        // Sans cela la fenêtre X restait dans l'ancien parent (bandes d'une
        // ReBar, panneaux déplacés d'une fenêtre à l'autre).
        let rect = gui::window_rect(hwnd).unwrap_or_default();
        with_backend(|b| b.reparent_window(hwnd, new_parent, rect));
    }
    old
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

/// ReleaseCapture réussit aussi quand aucune fenêtre n'a la capture : WPF
/// lève une Win32Exception sur un FALSE (#268).
pub extern "win64" fn release_capture() -> i32 {
    gui::release_capture();
    1
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
    crate::intl::keymap::map_virtual_key(crate::intl::keymap::current_klid(), code, map_type)
}

pub unsafe extern "win64" fn get_cursor_pos(point: *mut Point) -> i32 {
    if point.is_null() {
        return 0;
    }
    // Position réelle du pointeur (#268 : WPF compare WindowFromPoint de
    // cette position à sa fenêtre et ignorait tous les clics).
    let (x, y) = gui::cursor_screen_pos();
    (*point).x = x;
    (*point).y = y;
    1
}

pub unsafe extern "win64" fn set_cursor_pos(x: i32, y: i32) -> i32 {
    gui::set_cursor_screen_pos(x, y);
    1
}

pub unsafe extern "win64" fn screen_to_client(hwnd: usize, point: *mut Point) -> i32 {
    if point.is_null() {
        return 0;
    }
    if let Some(r) = gui::screen_rect(hwnd) {
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
    if let Some(r) = gui::screen_rect(hwnd) {
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
    // Paramètres > Souris (500 ms par défaut, comme Windows).
    crate::dlls::mouse::current().double_click
}

pub extern "win64" fn swap_mouse_button(_swap: i32) -> i32 {
    // Les boutons sont déjà inversés par libinput quand le réglage gaucher est
    // actif : on renvoie l'état précédent sans le modifier pour tout le bureau.
    i32::from(crate::dlls::mouse::current().swap_buttons)
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
    super::controls::popup::track(menu, flags, x, y, hwnd) as i32
}

pub unsafe extern "win64" fn track_popup_menu_ex(
    menu: usize,
    flags: u32,
    x: i32,
    y: i32,
    hwnd: usize,
    _params: *const c_void,
) -> i32 {
    super::controls::popup::track(menu, flags, x, y, hwnd) as i32
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

/// GetDlgItem : enfant direct de `dlg` dont l'identifiant de contrôle est `id`.
pub extern "win64" fn get_dlg_item(dlg: usize, id: i32) -> usize {
    let found = gui::child_windows(dlg)
        .into_iter()
        .find(|h| control_id(*h) as i32 == id);
    if found.is_none() {
        crate::process::set_last_error(1421); // ERROR_CONTROL_ID_NOT_FOUND
    }
    found.unwrap_or(0)
}

pub extern "win64" fn end_dialog(_dlg: usize, _result: isize) -> i32 {
    1
}

pub unsafe extern "win64" fn set_dlg_item_text_a(dlg: usize, id: i32, text: *const u8) -> i32 {
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => set_window_text_a(item, text),
    }
}

pub unsafe extern "win64" fn set_dlg_item_text_w(dlg: usize, id: i32, text: *const u16) -> i32 {
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => set_window_text_w(item, text),
    }
}

pub unsafe extern "win64" fn get_dlg_item_text_a(
    dlg: usize,
    id: i32,
    buf: *mut u8,
    max: i32,
) -> u32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => get_window_text_a(item, buf, max).max(0) as u32,
    }
}

pub unsafe extern "win64" fn get_dlg_item_text_w(
    dlg: usize,
    id: i32,
    buf: *mut u16,
    max: i32,
) -> u32 {
    if !buf.is_null() && max > 0 {
        *buf = 0;
    }
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => get_window_text_w(item, buf, max).max(0) as u32,
    }
}

pub unsafe extern "win64" fn get_dlg_ctrl_id(hwnd: usize) -> i32 {
    control_id(hwnd) as i32
}

pub unsafe extern "win64" fn is_dialog_message_a(_dlg: usize, _msg: *const Msg) -> i32 {
    0
}

pub unsafe extern "win64" fn is_dialog_message_w(_dlg: usize, _msg: *const Msg) -> i32 {
    0
}

pub unsafe extern "win64" fn send_dlg_item_message_a(
    dlg: usize,
    id: i32,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => send_message_a(item, msg, wp, lp),
    }
}

pub unsafe extern "win64" fn send_dlg_item_message_w(
    dlg: usize,
    id: i32,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    match get_dlg_item(dlg, id) {
        0 => 0,
        item => send_message_w(item, msg, wp, lp),
    }
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
    // Win32 : toute valeur négative (-1 en pratique) signifie « chaîne terminée par NUL ».
    let len = if count < 0 {
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
    // Win32 : toute valeur négative (-1 en pratique) signifie « chaîne terminée par NUL ».
    let len = if count < 0 {
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

/// `GetUpdateRgn` : la zone à repeindre est tout le rectangle client.
/// wxHtmlWindow ne dessine que les cellules comprises dans la boîte de cette
/// région ; laissée vide, le titre de l'accueil d'Audacity restait blanc (#1054).
pub unsafe extern "win64" fn get_update_rgn(hwnd: usize, rgn: usize, _erase: i32) -> i32 {
    const SIMPLEREGION: i32 = 2;
    match gui::window_client_rect(hwnd) {
        Some(r) if gui::set_region_rect(rgn, r) => SIMPLEREGION,
        _ => 0, // ERROR
    }
}

/// `RedrawWindow` : RDW_VALIDATE (sans RDW_INVALIDATE) valide la fenêtre,
/// sinon elle est invalidée comme avant ; RDW_UPDATENOW peint tout de suite.
pub unsafe extern "win64" fn redraw_window(
    hwnd: usize,
    _rect: *const Rect,
    _rgn: usize,
    flags: u32,
) -> i32 {
    const RDW_INVALIDATE: u32 = 0x1;
    const RDW_INTERNALPAINT: u32 = 0x2;
    const RDW_VALIDATE: u32 = 0x8;
    const RDW_UPDATENOW: u32 = 0x100;
    if flags & RDW_VALIDATE != 0 && flags & (RDW_INVALIDATE | RDW_INTERNALPAINT) == 0 {
        gui::validate_window(hwnd);
    } else {
        gui::request_paint(hwnd);
    }
    if flags & RDW_UPDATENOW != 0 {
        update_window(hwnd);
    }
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
        gui::screen_rect(from).map_or((0, 0), |r| (r.left, r.top))
    };
    let to_offset = if to == 0 {
        (0i32, 0i32)
    } else {
        gui::screen_rect(to).map_or((0, 0), |r| (r.left, r.top))
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

/// Police système des métriques de SystemParametersInfo (Segoe UI 9 pt à
/// 96 ppp, comme Windows 10/11). Écrite en LOGFONTW (92 octets) ou LOGFONTA
/// (60 octets) à `out`.
unsafe fn write_system_logfont(out: *mut u8, wide: bool, weight: i32) {
    let face = "Segoe UI";
    if wide {
        let mut f = [0u16; 32];
        for (d, c) in f.iter_mut().zip(face.encode_utf16()) {
            *d = c;
        }
        std::ptr::write_unaligned(
            out.cast::<crate::dlls::gdi32::LogFontW>(),
            crate::dlls::gdi32::LogFontW {
                height: -12,
                width: 0,
                escapement: 0,
                orientation: 0,
                weight,
                italic: 0,
                underline: 0,
                strikeout: 0,
                charset: 1,
                out_precision: 0,
                clip_precision: 0,
                quality: 5, // CLEARTYPE_QUALITY
                pitch_and_family: 0,
                face: f,
            },
        );
    } else {
        let mut f = [0u8; 32];
        f[..face.len()].copy_from_slice(face.as_bytes());
        std::ptr::write_unaligned(
            out.cast::<crate::dlls::gdi32::LogFontA>(),
            crate::dlls::gdi32::LogFontA {
                height: -12,
                width: 0,
                escapement: 0,
                orientation: 0,
                weight,
                italic: 0,
                underline: 0,
                strikeout: 0,
                charset: 1,
                out_precision: 0,
                clip_precision: 0,
                quality: 5,
                pitch_and_family: 0,
                face: f,
            },
        );
    }
}

/// SPI_GETNONCLIENTMETRICS, SPI_GETICONTITLELOGFONT et SPI_GETICONMETRICS :
/// structures qui contiennent des LOGFONT. Sans elles, l'appli lit une
/// police non initialisée (wxWidgets prend lfMessageFont : texte barré,
/// souligné, échelle démesurée, #797). Mêmes dispositions en x86 et x64
/// (aucun pointeur dedans).
unsafe fn system_parameters_fonts(
    action: u32,
    param: u32,
    data: *mut c_void,
    wide: bool,
) -> Option<i32> {
    let lf = if wide { 92usize } else { 60 };
    let out = data.cast::<u8>();
    match action {
        // SPI_GETICONTITLELOGFONT : uiParam = taille du LOGFONT.
        0x001F => {
            if data.is_null() || (param != 0 && (param as usize) < lf) {
                return Some(0);
            }
            write_system_logfont(out, wide, 400);
            Some(1)
        }
        // SPI_GETNONCLIENTMETRICS : cbSize en tête (avec ou sans
        // iPaddedBorderWidth, ajouté par Vista).
        0x0029 => {
            if data.is_null() {
                return Some(0);
            }
            let full = 4 + 5 * 4 + lf + 2 * 4 + lf + 2 * 4 + 3 * lf;
            let cb = *data.cast::<u32>() as usize;
            if cb != full && cb != full + 4 {
                crate::process::set_last_error(87);
                return Some(0);
            }
            std::ptr::write_bytes(out.add(4), 0, cb - 4);
            let ints = |o: usize, v: &[i32]| {
                for (k, x) in v.iter().enumerate() {
                    std::ptr::write_unaligned(out.add(o + 4 * k).cast::<i32>(), *x);
                }
            };
            // Valeurs de Windows 10 à 96 ppp : iBorderWidth, iScrollWidth,
            // iScrollHeight, iCaptionWidth, iCaptionHeight.
            ints(4, &[1, 17, 17, 36, 23]);
            let mut o = 24;
            write_system_logfont(out.add(o), wide, 400); // lfCaptionFont
            o += lf;
            ints(o, &[22, 22]); // iSmCaptionWidth/Height
            o += 8;
            write_system_logfont(out.add(o), wide, 400); // lfSmCaptionFont
            o += lf;
            ints(o, &[19, 19]); // iMenuWidth/Height
            o += 8;
            for _ in 0..3 {
                // lfMenuFont, lfStatusFont, lfMessageFont
                write_system_logfont(out.add(o), wide, 400);
                o += lf;
            }
            if cb == full + 4 {
                ints(o, &[4]); // iPaddedBorderWidth
            }
            Some(1)
        }
        // SPI_GETICONMETRICS : cbSize, iHorzSpacing, iVertSpacing,
        // iTitleWrap, lfFont.
        0x002D => {
            if data.is_null() || *data.cast::<u32>() as usize != 16 + lf {
                return Some(0);
            }
            for (k, v) in [75i32, 75, 1].iter().enumerate() {
                std::ptr::write_unaligned(out.add(4 + 4 * k).cast::<i32>(), *v);
            }
            write_system_logfont(out.add(16), wide, 400);
            Some(1)
        }
        _ => None,
    }
}

pub unsafe extern "win64" fn system_parameters_info_a(
    action: u32,
    param: u32,
    data: *mut c_void,
    update: u32,
) -> i32 {
    if let Some(r) = system_parameters_fonts(action, param, data, false) {
        return r;
    }
    system_parameters_info_common(action, param, data, update)
}

unsafe fn system_parameters_info_common(
    action: u32,
    _param: u32,
    data: *mut c_void,
    _update: u32,
) -> i32 {
    let a11y = crate::dlls::a11y::current();
    match action {
        // SPI_GETHIGHCONTRAST : HIGHCONTRAST { cbSize, dwFlags, lpszDefaultScheme }.
        0x0042 if !data.is_null() => {
            let flags = data.cast::<u32>().add(1);
            // HCF_AVAILABLE | HCF_HIGHCONTRASTON si un thème est actif.
            *flags = 0x2 | u32::from(a11y.contrast != 0);
            return 1;
        }
        // SPI_GETCLIENTAREAANIMATION : BOOL.
        0x1042 if !data.is_null() => {
            *data.cast::<i32>() = i32::from(a11y.animations);
            return 1;
        }
        // Souris (Paramètres > Souris) : SPI_GETMOUSETRAILS, SPI_GETWHEELSCROLLLINES,
        // SPI_GETMOUSESPEED (1 à 20) et SPI_GETWHEELSCROLLCHARS (3, comme Windows).
        0x005E | 0x0068 | 0x0070 | 0x006C if !data.is_null() => {
            let mouse = crate::dlls::mouse::current();
            *data.cast::<u32>() = match action {
                0x005E => mouse.trails,
                0x0068 => mouse.wheel_lines,
                0x0070 => mouse.speed,
                _ => 3,
            };
            return 1;
        }
        // SPI_GETCARETWIDTH : DWORD en pixels.
        0x2006 if !data.is_null() => {
            *data.cast::<u32>() = u32::from(a11y.caret_width);
            return 1;
        }
        _ => {}
    }
    // SPI_GETWORKAREA = 0x0030
    if action == 0x0030 && !data.is_null() {
        // Sans la barre des tâches : une fenêtre placée ici n'est pas cachée dessous.
        *(data as *mut Rect) = gui::work_area();
        return 1;
    }
    1 // Stub : succès générique
}

pub unsafe extern "win64" fn system_parameters_info_w(
    action: u32,
    param: u32,
    data: *mut c_void,
    update: u32,
) -> i32 {
    if let Some(r) = system_parameters_fonts(action, param, data, true) {
        return r;
    }
    system_parameters_info_common(action, param, data, update)
}

/// Thread créateur de la fenêtre (son GetCurrentThreadId) et processus.
/// WPF refuse d'attacher son rendu à une fenêtre d'un autre thread (#268).
pub unsafe extern "win64" fn get_window_thread_process_id(hwnd: usize, pid: *mut u32) -> u32 {
    let Some(tid) = gui::window_thread_id(hwnd) else {
        if gui::wnd_proc_of(hwnd).is_none() {
            crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
            return 0;
        }
        if !pid.is_null() {
            *pid = std::process::id();
        }
        return std::process::id();
    };
    if !pid.is_null() {
        *pid = std::process::id();
    }
    tid
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
    if gui::wnd_proc_of(hwnd).is_none() {
        return 0;
    }
    gui::bring_to_top(hwnd);
    1
}

pub extern "win64" fn set_foreground_window(hwnd: usize) -> i32 {
    gui::set_focus(hwnd);
    1
}

/// `AllowSetForegroundWindow(dwProcessId)` : sous Windows, autorise un
/// autre processus (ou tous, `ASFW_ANY`) à passer au premier plan. Winlux
/// ne restreint pas ce passage d'un processus à l'autre : rien à accorder,
/// l'appel réussit toujours (#1074, Flow Launcher).
pub extern "win64" fn allow_set_foreground_window(_process_id: u32) -> i32 {
    1
}

/// `LockSetForegroundWindow(uLockCode)` (`LSFW_LOCK` 1, `LSFW_UNLOCK` 2) :
/// sans verrou de premier plan dans Winlux, l'appel réussit sans effet.
pub extern "win64" fn lock_set_foreground_window(lock_code: u32) -> i32 {
    if matches!(lock_code, 1 | 2) {
        1
    } else {
        crate::process::set_last_error(87);
        0
    }
}

// =====================================================================
// Hooks stubs
// =====================================================================

const WH_GETMESSAGE: i32 = 3;
const PM_REMOVE: usize = 1;

/// Crochet installé par SetWindowsHookEx. Seul WH_GETMESSAGE est appelé :
/// Qt 5 y relance la livraison de ses événements différés (clics, touches,
/// repeints) ; sans lui, une fenêtre Qt s'affiche mais ne réagit plus (#938).
#[derive(Clone, Copy)]
struct Hook {
    handle: usize,
    id: i32,
    proc_: usize,
    /// Fil visé ; 0 : tous les fils du processus.
    thread: u32,
}

fn hooks() -> &'static Mutex<Vec<Hook>> {
    static H: OnceLock<Mutex<Vec<Hook>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(Vec::new()))
}

thread_local! {
    /// Crochet en cours d'exécution sur ce fil : CallNextHookEx part de lui.
    static CURRENT_HOOK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub unsafe extern "win64" fn set_windows_hook_ex_a(
    id: i32,
    proc_: usize,
    _module: usize,
    thread: u32,
) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x00F0_0000);
    let handle = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if id == WH_GETMESSAGE && proc_ != 0 {
        // Le plus récent passe en premier, comme sous Windows.
        hooks().lock().unwrap_or_else(|e| e.into_inner()).insert(
            0,
            Hook {
                handle,
                id,
                proc_,
                thread,
            },
        );
    }
    handle
}

pub unsafe extern "win64" fn set_windows_hook_ex_w(
    id: i32,
    proc_: usize,
    module: usize,
    thread: u32,
) -> usize {
    set_windows_hook_ex_a(id, proc_, module, thread)
}

pub extern "win64" fn unhook_windows_hook_ex(hook: usize) -> i32 {
    hooks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|h| h.handle != hook);
    1
}

/// Crochet de type `id` qui suit `after` (0 : le premier) pour ce fil.
fn next_hook(id: i32, after: usize) -> Option<Hook> {
    let tid = super::kernel32::get_current_thread_id();
    let list = hooks().lock().unwrap_or_else(|e| e.into_inner());
    let mine = list
        .iter()
        .filter(|h| h.id == id && (h.thread == 0 || h.thread == tid));
    if after == 0 {
        return mine.copied().next();
    }
    mine.skip_while(|h| h.handle != after).nth(1).copied()
}

unsafe fn run_hook(h: Hook, code: i32, wparam: usize, lparam: isize) -> isize {
    let f: extern "win64" fn(i32, usize, isize) -> isize = std::mem::transmute(h.proc_);
    let prev = CURRENT_HOOK.with(|c| c.replace(h.handle));
    let r = f(code, wparam, lparam);
    CURRENT_HOOK.with(|c| c.set(prev));
    r
}

/// Passe le message que GetMessage/PeekMessage va rendre aux crochets
/// WH_GETMESSAGE du fil (HC_ACTION, wParam = PM_REMOVE ou PM_NOREMOVE).
/// Le crochet peut modifier le MSG en place.
unsafe fn call_get_message_hooks(msg: *mut Msg, remove: usize) {
    if msg.is_null() {
        return;
    }
    if let Some(h) = next_hook(WH_GETMESSAGE, 0) {
        run_hook(h, 0, remove, msg as isize);
    }
}

pub unsafe extern "win64" fn call_next_hook_ex(
    _hook: usize,
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // Le paramètre hhk est ignoré par Windows : la chaîne repart du crochet
    // en cours d'exécution sur ce fil.
    let current = CURRENT_HOOK.with(|c| c.get());
    if current == 0 {
        return 0;
    }
    let id = hooks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|h| h.handle == current)
        .map(|h| h.id);
    match id.and_then(|id| next_hook(id, current)) {
        Some(h) => run_hook(h, code, wparam, lparam),
        None => 0,
    }
}

// =====================================================================
// Misc window management
// =====================================================================

/// WINDOWPLACEMENT (44 octets) : longueur, drapeaux, showCmd, points
/// réduit/agrandi, rectangle de la fenêtre restaurée.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowPlacement {
    pub length: u32,
    pub flags: u32,
    pub show_cmd: u32,
    pub min_pos: Point,
    pub max_pos: Point,
    pub normal: Rect,
}

/// Contenu rendu par GetWindowPlacement pour un état et un rectangle donnés.
/// Winlux ne garde pas de rectangle « restauré » à part : c'est le rectangle
/// courant de la fenêtre.
fn placement_of(state: u8, rect: Rect) -> WindowPlacement {
    let show_cmd = match state {
        gui::WINDOW_MINIMIZED => 2, // SW_SHOWMINIMIZED
        gui::WINDOW_MAXIMIZED => 3, // SW_SHOWMAXIMIZED
        _ => 1,                     // SW_SHOWNORMAL
    };
    WindowPlacement {
        length: std::mem::size_of::<WindowPlacement>() as u32,
        flags: 0,
        show_cmd,
        min_pos: Point { x: -1, y: -1 },
        max_pos: Point { x: -1, y: -1 },
        normal: rect,
    }
}

/// GetWindowPlacement (#1073) : WPF (WindowChrome) la lit à chaque WM_SIZE
/// et lève une exception si elle échoue.
pub unsafe extern "win64" fn get_window_placement(hwnd: usize, wp: *mut WindowPlacement) -> i32 {
    if wp.is_null() {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let Some(rect) = gui::screen_rect(hwnd) else {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    };
    let mut state = gui::WINDOW_NORMAL;
    with_backend(|b| state = b.window_state(hwnd));
    std::ptr::write_unaligned(wp, placement_of(state, rect));
    1
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

/// LoadStringA/W : les tables de chaînes du module (voir resource.rs).
pub unsafe extern "win64" fn load_string_a(
    instance: usize,
    id: u32,
    buf: *mut u8,
    max: i32,
) -> i32 {
    super::resource::load_string_a(instance, id, buf, max)
}

pub unsafe extern "win64" fn load_string_w(
    instance: usize,
    id: u32,
    buf: *mut u16,
    max: i32,
) -> i32 {
    super::resource::load_string_w(instance, id, buf, max)
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

pub extern "win64" fn get_key_name_text_a(_lparam: i32, _buf: *mut u8, _max: i32) -> i32 {
    0
}

pub extern "win64" fn get_key_name_text_w(_lparam: i32, _buf: *mut u16, _max: i32) -> i32 {
    0
}

/// VkKeyScan(Ex) : touche et modificateurs d'un caractère dans la
/// disposition réelle (voir intl::keymap).
pub extern "win64" fn vk_key_scan_a(ch: u8) -> i16 {
    let text = crate::intl::codepage::decode(crate::intl::codepage::resolve(0), &[ch], false)
        .unwrap_or_default();
    text.chars().next().map_or(-1, |c| {
        crate::intl::keymap::vk_key_scan(crate::intl::keymap::current_klid(), c)
    })
}

pub extern "win64" fn vk_key_scan_w(ch: u16) -> i16 {
    char::from_u32(ch as u32).map_or(-1, |c| {
        crate::intl::keymap::vk_key_scan(crate::intl::keymap::current_klid(), c)
    })
}

pub extern "win64" fn vk_key_scan_ex_a(ch: u8, _hkl: usize) -> i16 {
    vk_key_scan_a(ch)
}

pub extern "win64" fn vk_key_scan_ex_w(ch: u16, _hkl: usize) -> i16 {
    vk_key_scan_w(ch)
}

/// ToUnicodeEx : caractères de la touche selon la disposition réelle,
/// touches mortes comprises (-1 puis la lettre composée).
pub unsafe extern "win64" fn to_unicode_ex(
    vk: u32,
    scan: u32,
    state: *const u8,
    buf: *mut u16,
    buf_size: i32,
    flags: u32,
    _hkl: usize,
) -> i32 {
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    let mut keys = [0u8; 256];
    if !state.is_null() {
        std::ptr::copy_nonoverlapping(state, keys.as_mut_ptr(), 256);
    }
    let typed = crate::intl::keymap::to_unicode(
        crate::intl::keymap::current_klid(),
        vk,
        scan,
        &keys,
        flags,
    );
    let mut n = 0usize;
    for c in &typed.chars {
        let mut units = [0u16; 2];
        for u in c.encode_utf16(&mut units).iter() {
            if n < buf_size as usize {
                *buf.add(n) = *u;
                n += 1;
            }
        }
    }
    if n < buf_size as usize {
        *buf.add(n) = 0;
    }
    typed.code
}

/// ToAscii(Ex) : comme ToUnicode, dans la page ANSI ; renvoie 1 ou 2
/// caractères, -1 pour une touche morte.
pub unsafe extern "win64" fn to_ascii_ex(
    vk: u32,
    scan: u32,
    state: *const u8,
    out: *mut u16,
    flags: u32,
    hkl: usize,
) -> i32 {
    let mut wide = [0u16; 4];
    let code = to_unicode_ex(vk, scan, state, wide.as_mut_ptr(), 4, flags, hkl);
    if code == 0 || out.is_null() {
        return code;
    }
    let count = code.unsigned_abs().min(2) as usize;
    let text = String::from_utf16_lossy(&wide[..count]);
    let bytes = crate::intl::codepage::encode(crate::intl::codepage::resolve(0), &text, b'?', true)
        .map(|e| e.bytes)
        .unwrap_or_default();
    let out = out as *mut u8;
    for (i, b) in bytes.iter().take(2).enumerate() {
        *out.add(i) = *b;
    }
    code
}

pub unsafe extern "win64" fn to_ascii(
    vk: u32,
    scan: u32,
    state: *const u8,
    out: *mut u16,
    flags: u32,
) -> i32 {
    to_ascii_ex(vk, scan, state, out, flags, 0)
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

/// Message enregistré : un numéro de 0xC000 à 0xFFFF propre au nom (sans
/// tenir compte de la casse) et identique dans tous les processus, comme
/// l'atome global de Windows. Tous les noms rendaient 0xC000 : WinForms
/// confondait « WinFormsMouseEnter » et « WinFormsUnSubclass » et rendait au
/// bouton sa procédure d'origine au premier survol (#666).
fn register_window_message(name: String) -> u32 {
    if name.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    super::clipboard::registered_id(&name)
}

pub unsafe extern "win64" fn register_window_message_a(name: *const u8) -> u32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    register_window_message(crate::dlls::cstr(name))
}

pub unsafe extern "win64" fn register_window_message_w(name: *const u16) -> u32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    register_window_message(crate::dlls::wstr(name))
}

/// GetMessagePos : position du pointeur (écran) au dernier message lu.
pub extern "win64" fn get_message_pos() -> u32 {
    let (x, y) = gui::cursor_screen_pos();
    (x as u16 as u32) | ((y as u16 as u32) << 16)
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

/// `WaitMessage` : dort jusqu'à ce qu'un message, un WM_PAINT ou un
/// minuteur soit disponible. Répondre tout de suite faisait tourner à 100 %
/// la boucle `PeekMessage(PM_NOREMOVE)` / `WaitMessage` de WinForms.
pub extern "win64" fn wait_message() -> i32 {
    loop {
        gui::pump_backend();
        super::ws2_32::pump_async();
        let q = gui::current_queue();
        if gui::peek_message(&q, 0, 0, 0).is_some()
            || gui::pending_paint(0, 0, 0).is_some()
            || gui::peek_due_timer(0, 0, 0).is_some()
        {
            return 1;
        }
        let cap = if super::ws2_32::async_active() {
            10
        } else {
            500
        };
        let wait = gui::next_timer_ms(0).map_or(cap, |ms| ms.min(cap));
        if !gui::wait_backend(wait) {
            // Sans affichage, rien ne viendra : rendre la main.
            return 1;
        }
    }
}

pub unsafe extern "win64" fn msg_wait_for_multiple_objects(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    timeout: u32,
    wake_mask: u32,
) -> u32 {
    const WAIT_TIMEOUT: u32 = 258;
    if count > 63 || (count > 0 && handles.is_null()) {
        crate::process::set_last_error(87);
        return u32::MAX;
    }
    let hs: Vec<usize> = (0..count as usize)
        .map(|i| std::ptr::read_unaligned(handles.add(i)))
        .collect();
    let start = std::time::Instant::now();
    loop {
        // 1. Les objets. Une socket associée par WSAEventSelect rend son
        //    événement prêt : PuTTY attend ainsi le réseau (avant, on
        //    répondait toujours « un message attend » et il tournait à vide).
        if wait_all != 0 {
            let all = !hs.is_empty()
                && hs.iter().all(|&h| {
                    crate::dlls::ws2_32::handle_ready(h)
                        || (!crate::dlls::ws2_32::is_wsa_event(h)
                            && crate::dlls::kernel32::lookup_sync(h).is_some()
                            && crate::dlls::kernel32::wait_for_single_object(h, 0) == 0)
                });
            if all {
                return 0;
            }
        } else {
            for (i, &h) in hs.iter().enumerate() {
                if crate::dlls::ws2_32::handle_ready(h) {
                    return i as u32;
                }
                if crate::dlls::ws2_32::is_wsa_event(h) {
                    continue;
                }
                if crate::dlls::kernel32::lookup_sync(h).is_none()
                    && !crate::dlls::console_input::is_console_input(h)
                {
                    crate::process::set_last_error(6);
                    return u32::MAX;
                }
                if crate::dlls::kernel32::wait_for_single_object(h, 0) == 0 {
                    return i as u32;
                }
            }
        }
        // 2. La file de messages du thread (messages, WM_PAINT, minuteurs).
        if wake_mask != 0 && message_waiting() {
            return count;
        }
        // 3. Le délai.
        let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
        if timeout != u32::MAX && elapsed >= timeout {
            return WAIT_TIMEOUT;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            timeout - elapsed
        };
        // Tranches courtes : les objets et les sockets n'ont pas de
        // descripteur commun avec la connexion X11.
        let slice = left.min(gui::next_timer_ms(0).unwrap_or(10)).clamp(1, 10);
        if !gui::wait_backend(slice) {
            std::thread::sleep(std::time::Duration::from_millis(slice as u64));
        }
    }
}

/// Un message, un WM_PAINT en attente ou un minuteur échu pour ce thread ?
fn message_waiting() -> bool {
    gui::pump_backend();
    super::ws2_32::pump_async();
    let q = gui::current_queue();
    gui::peek_message(&q, 0, 0, 0).is_some()
        || gui::pending_paint(0, 0, 0).is_some()
        || gui::next_timer_ms(0) == Some(0)
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

/// Barres de défilement d'une fenêtre (SB_HORZ 0, SB_VERT 1) : min, max,
/// page, position. Elles ne sont pas encore dessinées, mais un programme
/// relit ce qu'il a posé (#711).
#[derive(Clone, Copy, Default)]
struct WindowBar {
    min: i32,
    max: i32,
    page: u32,
    pos: i32,
}

fn window_bars() -> &'static Mutex<HashMap<(usize, i32), WindowBar>> {
    static S: OnceLock<Mutex<HashMap<(usize, i32), WindowBar>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_bar<R>(hwnd: usize, bar: i32, f: impl FnOnce(&mut WindowBar) -> R) -> Option<R> {
    if !(0..=1).contains(&bar) || hwnd == 0 {
        return None;
    }
    let mut m = window_bars().lock().unwrap_or_else(|e| e.into_inner());
    Some(f(m.entry((hwnd, bar)).or_default()))
}

impl WindowBar {
    fn clamp(&mut self) {
        let top = (self.max - (self.page as i32 - 1).max(0)).max(self.min);
        self.pos = self.pos.clamp(self.min, top);
    }
}

/// SCROLLINFO : cbSize, fMask, nMin, nMax, nPage, nPos, nTrackPos.
pub unsafe extern "win64" fn get_scroll_info(hwnd: usize, bar: i32, info: *mut c_void) -> i32 {
    if bar == 2 {
        // SB_CTL : le contrôle SCROLLBAR lui-même.
        return super::controls::scroll_info(hwnd, info.cast(), false, false).unwrap_or(0);
    }
    if info.is_null() {
        return 0;
    }
    let si = info as *mut i32;
    let mask = *si.add(1) as u32;
    with_bar(hwnd, bar, |b| {
        if mask & 0x1 != 0 {
            *si.add(2) = b.min;
            *si.add(3) = b.max;
        }
        if mask & 0x2 != 0 {
            *si.add(4) = b.page as i32;
        }
        if mask & 0x4 != 0 {
            *si.add(5) = b.pos;
        }
        if mask & 0x10 != 0 {
            *si.add(6) = b.pos;
        }
        1
    })
    .unwrap_or(0)
}

pub unsafe extern "win64" fn set_scroll_info(
    hwnd: usize,
    bar: i32,
    info: *const c_void,
    redraw: i32,
) -> i32 {
    if bar == 2 {
        return super::controls::scroll_info(hwnd, info as *mut _, true, redraw != 0).unwrap_or(0);
    }
    if info.is_null() {
        return 0;
    }
    let si = info as *const i32;
    let mask = *si.add(1) as u32;
    with_bar(hwnd, bar, |b| {
        if mask & 0x1 != 0 {
            b.min = *si.add(2);
            b.max = *si.add(3);
        }
        if mask & 0x2 != 0 {
            b.page = *si.add(4) as u32;
        }
        if mask & 0x4 != 0 {
            b.pos = *si.add(5);
        }
        b.clamp();
        b.pos
    })
    .unwrap_or(0)
}

pub unsafe extern "win64" fn get_scroll_pos(hwnd: usize, bar: i32) -> i32 {
    if bar == 2 {
        let mut si = [28i32, 0x4, 0, 0, 0, 0, 0];
        return match super::controls::scroll_info(hwnd, si.as_mut_ptr().cast(), false, false) {
            Some(_) => si[5],
            None => 0,
        };
    }
    with_bar(hwnd, bar, |b| b.pos).unwrap_or(0)
}

/// Rend l'ancienne position.
pub unsafe extern "win64" fn set_scroll_pos(hwnd: usize, bar: i32, pos: i32, redraw: i32) -> i32 {
    if bar == 2 {
        let old = get_scroll_pos(hwnd, bar);
        let mut si = [28i32, 0x4, 0, 0, 0, pos, 0];
        super::controls::scroll_info(hwnd, si.as_mut_ptr().cast(), true, redraw != 0);
        return old;
    }
    with_bar(hwnd, bar, |b| {
        let old = b.pos;
        b.pos = pos;
        b.clamp();
        old
    })
    .unwrap_or(0)
}

pub unsafe extern "win64" fn set_scroll_range(
    hwnd: usize,
    bar: i32,
    min: i32,
    max: i32,
    redraw: i32,
) -> i32 {
    if bar == 2 {
        let mut si = [28i32, 0x1, min, max, 0, 0, 0];
        return super::controls::scroll_info(hwnd, si.as_mut_ptr().cast(), true, redraw != 0)
            .map_or(0, |_| 1);
    }
    with_bar(hwnd, bar, |b| {
        b.min = min;
        b.max = max;
        b.clamp();
        1
    })
    .unwrap_or(0)
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

pub extern "win64" fn get_dc_ex(_hwnd: usize, _rgn: usize, _flags: u32) -> usize {
    gui::create_dc(_hwnd)
}

/// Positions en attente de chaque BeginDeferWindowPos, appliquées ensemble
/// par EndDeferWindowPos. wxWidgets place ainsi tous les enfants d'un
/// panneau quand ses sizers le redimensionnent : sans cela, ils restaient
/// empilés en (0,0).
type Deferred = Vec<(usize, usize, i32, i32, i32, i32, u32)>;

fn deferred() -> &'static Mutex<HashMap<usize, Deferred>> {
    static D: OnceLock<Mutex<HashMap<usize, Deferred>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(HashMap::new()))
}

pub extern "win64" fn begin_defer_window_pos(count: i32) -> usize {
    if count < 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x00D0_0001);
    let handle = NEXT.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
    deferred()
        .lock()
        .unwrap()
        .insert(handle, Vec::with_capacity(count as usize));
    handle
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn defer_window_pos(
    hdwp: usize,
    hwnd: usize,
    after: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> usize {
    if gui::window_rect(hwnd).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    let mut all = deferred().lock().unwrap();
    let Some(list) = all.get_mut(&hdwp) else {
        crate::process::set_last_error(6); // ERROR_INVALID_HANDLE
        return 0;
    };
    // Une seconde demande pour la même fenêtre remplace la première.
    list.retain(|e| e.0 != hwnd);
    list.push((hwnd, after, x, y, cx, cy, flags));
    hdwp
}

pub extern "win64" fn end_defer_window_pos(hdwp: usize) -> i32 {
    let Some(list) = deferred().lock().unwrap().remove(&hdwp) else {
        crate::process::set_last_error(6);
        return 0;
    };
    for (hwnd, after, x, y, cx, cy, flags) in list {
        unsafe { set_window_pos(hwnd, after, x, y, cx, cy, flags) };
    }
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

/// Limite documentée de `wsprintf` : 1024 caractères, terminateur compris.
const WSPRINTF_MAX: usize = 1024;

/// `wsprintfA(buf, fmt, ...)` : formatage façon printf (%s %d %i %u %x %X %c
/// %% %ld %lu, largeur et remplissage par zéros) via le moteur de la CRT,
/// tronqué à 1023 caractères. Renvoie le nombre de caractères écrits.
///
/// # Sécurité
/// `args` pointe sur les registres sauvés par le pont variadique : tampon,
/// format puis les arguments annoncés par le format.
pub unsafe extern "win64" fn imp_wsprintfa(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let dst = v.next_u64() as *mut u8;
    let fmt = v.next_ptr::<u8>();
    if dst.is_null() || fmt.is_null() {
        return 0;
    }
    let mut b = crate::dlls::crt_stdio::format_narrow(fmt, &mut v);
    b.truncate(WSPRINTF_MAX - 1);
    std::ptr::copy_nonoverlapping(b.as_ptr(), dst, b.len());
    *dst.add(b.len()) = 0;
    b.len() as i32
}

/// Variante large de [`imp_wsprintfa`] (`%s` attend une chaîne UTF-16).
///
/// # Sécurité
/// Mêmes conditions que [`imp_wsprintfa`].
pub unsafe extern "win64" fn imp_wsprintfw(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let dst = v.next_u64() as *mut u16;
    let fmt = v.next_ptr::<u16>();
    if dst.is_null() || fmt.is_null() {
        return 0;
    }
    let mut w = crate::dlls::crt::format_wide(fmt, &mut v);
    w.truncate(WSPRINTF_MAX - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    *dst.add(w.len()) = 0;
    w.len() as i32
}

crate::varargs_thunk!(wsprintfa_thunk => imp_wsprintfa);
crate::varargs_thunk!(wsprintfw_thunk => imp_wsprintfw);

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        // --- Classes & fenêtres ---
        "RegisterClassA"=>fn register_class_a, "RegisterClassW"=>fn register_class_w,
        "RegisterClassExA"=>fn register_class_ex_a, "RegisterClassExW"=>fn register_class_ex_w,
        "UnregisterClassA"=>fn unregister_class_a, "UnregisterClassW"=>fn unregister_class_w,
        "CreateWindowExA"=>fn create_window_ex_a, "CreateWindowExW"=>fn create_window_ex_w,
        "DefWindowProcA"=>fn def_window_proc_a, "DefWindowProcW"=>fn def_window_proc_w,
        "CallWindowProcA"=>fn call_window_proc, "CallWindowProcW"=>fn call_window_proc,
        "GetProcessWindowStation"=>fn get_process_window_station,
        "GetUserObjectInformationW"=>fn get_user_object_information_w, "GetUserObjectInformationA"=>fn get_user_object_information_a,
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
        "AllowSetForegroundWindow"=>fn allow_set_foreground_window,
        "LockSetForegroundWindow"=>fn lock_set_foreground_window,
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
        "RealGetWindowClassA"=>fn real_get_window_class_a, "RealGetWindowClassW"=>fn real_get_window_class_w,
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
        "GetKeyNameTextA"=>fn get_key_name_text_a, "GetKeyNameTextW"=>fn get_key_name_text_w,
        "VkKeyScanA"=>fn vk_key_scan_a, "VkKeyScanW"=>fn vk_key_scan_w,
        "ToUnicode"=>fn to_unicode, "ToUnicodeEx"=>fn to_unicode_ex,
        "ToAscii"=>fn to_ascii, "ToAsciiEx"=>fn to_ascii_ex,
        "VkKeyScanExA"=>fn vk_key_scan_ex_a, "VkKeyScanExW"=>fn vk_key_scan_ex_w,
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
        "MessageBoxA"=>fn message_box_a, "MessageBoxW"=>fn message_box_w, "MessageBeep"=>fn message_beep,
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
        "MonitorFromWindow"=>fn crate::dlls::monitors::monitor_from_window,
        "MonitorFromPoint"=>fn crate::dlls::monitors::monitor_from_point_packed,
        "MonitorFromRect"=>fn crate::dlls::monitors::monitor_from_rect,
        "GetMonitorInfoA"=>fn crate::dlls::monitors::get_monitor_info_a, "GetMonitorInfoW"=>fn crate::dlls::monitors::get_monitor_info_w,
        "EnumDisplayMonitors"=>fn crate::dlls::monitors::enum_display_monitors,
        // --- Deferred window pos ---
        "BeginDeferWindowPos"=>fn begin_defer_window_pos,
        "DeferWindowPos"=>fn defer_window_pos,
        "EndDeferWindowPos"=>fn end_defer_window_pos,
        // --- Misc ---
        "CharNextA"=>fn char_next_a, "CharPrevA"=>fn char_prev_a,
        "wsprintfA"=>fn wsprintfa_thunk, "wsprintfW"=>fn wsprintfw_thunk,
    }
}

/// Fenêtre de premier niveau qui contient `hwnd` (un propriétaire désigné
/// par une fenêtre enfant est remplacé par sa fenêtre principale).
fn owner_top_level(mut hwnd: usize) -> usize {
    for _ in 0..64 {
        let Some(style) = gui::window_style(hwnd) else {
            return 0;
        };
        if style & WS_CHILD == 0 {
            return hwnd;
        }
        match gui::window_parent(hwnd) {
            Some(p) if p != 0 => hwnd = p,
            _ => return 0,
        }
    }
    0
}

/// Cadre d'une fenêtre WS_POPUP sans barre de titre (WS_CAPTION) : la boîte
/// « À propos » de Notepad++ n'a qu'une bordure fine (#705). `None` : cadre
/// complet, comme toute fenêtre WS_OVERLAPPED ou avec WS_CAPTION.
fn popup_decorations(style: u32) -> Option<u8> {
    const WS_POPUP: u32 = 0x8000_0000;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_THICKFRAME: u32 = 0x0004_0000;
    const WS_BORDER: u32 = 0x0080_0000;
    const WS_DLGFRAME: u32 = 0x0040_0000;
    if style & WS_POPUP == 0 || style & WS_CAPTION == WS_CAPTION {
        return None;
    }
    Some(u8::from(
        style & (WS_BORDER | WS_DLGFRAME | WS_THICKFRAME) != 0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GetWindowPlacement : 44 octets, showCmd selon l'état, rectangle courant.
    #[test]
    fn placement_de_fenetre() {
        assert_eq!(std::mem::size_of::<WindowPlacement>(), 44);
        let r = Rect {
            left: 10,
            top: 20,
            right: 330,
            bottom: 220,
        };
        let p = placement_of(gui::WINDOW_MAXIMIZED, r);
        assert_eq!((p.length, p.show_cmd, p.normal), (44, 3, r));
        assert_eq!(placement_of(gui::WINDOW_MINIMIZED, r).show_cmd, 2);
        assert_eq!(placement_of(gui::WINDOW_NORMAL, r).show_cmd, 1);
        unsafe {
            assert_eq!(get_window_placement(0x7fff_0001, std::ptr::null_mut()), 0);
            let mut wp = WindowPlacement::default();
            assert_eq!(get_window_placement(0x7fff_0001, &mut wp), 0);
        }
    }

    /// #842 : wxWidgets ajoute SM_CXMENUCHECK à la largeur des libellés
    /// de cases à cocher ; à 0, ils étaient tronqués.
    #[test]
    fn taille_des_cases_a_cocher() {
        assert_eq!(get_system_metrics(71), 15);
        assert_eq!(get_system_metrics(72), 15);
    }

    /// #797 : lfMessageFont de NONCLIENTMETRICSW est une vraie police (Segoe
    /// UI, ni barrée ni soulignée), pas le contenu non initialisé du tampon.
    #[test]
    fn nonclientmetrics_donne_une_police_propre() {
        let mut buf = vec![0xAAu8; 504];
        buf[..4].copy_from_slice(&504u32.to_le_bytes());
        let r = unsafe { system_parameters_info_w(0x29, 504, buf.as_mut_ptr().cast(), 0) };
        assert_eq!(r, 1);
        let msg = 408;
        assert_eq!(
            i32::from_le_bytes(buf[msg..msg + 4].try_into().unwrap()),
            -12
        );
        assert_eq!(&buf[msg + 20..msg + 23], &[0, 0, 0]); // italique, souligné, barré
        let face: Vec<u16> = buf[msg + 28..msg + 44]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16_lossy(&face), "Segoe UI");
        assert_eq!(i32::from_le_bytes(buf[500..504].try_into().unwrap()), 4);
        // Version A : 344 octets, lfMessageFont à 280.
        let mut a = vec![0xAAu8; 344];
        a[..4].copy_from_slice(&344u32.to_le_bytes());
        assert_eq!(
            unsafe { system_parameters_info_a(0x29, 344, a.as_mut_ptr().cast(), 0) },
            1
        );
        assert_eq!(&a[280 + 28..280 + 36], b"Segoe UI");
        assert_eq!(&a[280 + 20..280 + 23], &[0, 0, 0]);
    }

    /// Qt lit la police de l'interface dans NONCLIENTMETRICS ; une hauteur
    /// nulle donnait des fenêtres de 5 millions de pixels à VLC (#850).
    /// Le cadre des fenêtres de premier niveau appartient au gestionnaire de
    /// fenêtres : AdjustWindowRectEx n'ajoute que la barre de menus, sinon
    /// Qt (VLC) dessine une zone client plus petite que la fenêtre.
    #[test]
    fn adjust_window_rect_sans_cadre_pour_le_premier_niveau() {
        let mut r = Rect::from_size(0, 0, 884, 513);
        unsafe { adjust_window_rect_ex(&mut r, 0x00CF_0000, 0, 0) };
        assert_eq!(r, Rect::from_size(0, 0, 884, 513));
        unsafe { adjust_window_rect_ex(&mut r, 0x00CF_0000, 1, 0) };
        assert_eq!(r.top, -crate::dlls::menus::BAR_HEIGHT);
        let mut c = Rect::from_size(0, 0, 10, 10);
        unsafe { adjust_window_rect_ex(&mut c, 0x4080_0000, 0, 0) }; // enfant WS_BORDER
        assert!(c.left < 0);
    }

    #[test]
    fn nonclientmetrics_et_police_des_icones_remplies() {
        unsafe {
            let mut w = vec![0u8; 504];
            w[..4].copy_from_slice(&504u32.to_le_bytes());
            assert_eq!(
                system_parameters_info_w(0x29, 504, w.as_mut_ptr().cast(), 0),
                1
            );
            let lf_message = 24 + 92 + 8 + 92 + 8 + 92 + 92;
            assert_eq!(
                i32::from_le_bytes(w[lf_message..lf_message + 4].try_into().unwrap()),
                -12
            );
            assert_eq!(
                u16::from_le_bytes([w[lf_message + 28], w[lf_message + 29]]),
                b'S' as u16
            );
            assert_eq!(i32::from_le_bytes(w[20..24].try_into().unwrap()), 23);
            let mut a = vec![0u8; 340];
            a[..4].copy_from_slice(&340u32.to_le_bytes());
            assert_eq!(
                system_parameters_info_a(0x29, 340, a.as_mut_ptr().cast(), 0),
                1
            );
            let lf_message = 24 + 60 + 8 + 60 + 8 + 60 + 60;
            assert_eq!(
                i32::from_le_bytes(a[lf_message..lf_message + 4].try_into().unwrap()),
                -12
            );
            assert_eq!(a[lf_message + 28], b'S');
            // Taille inconnue : refusée.
            let mut bad = vec![0u8; 64];
            bad[..4].copy_from_slice(&64u32.to_le_bytes());
            assert_eq!(
                system_parameters_info_w(0x29, 64, bad.as_mut_ptr().cast(), 0),
                0
            );
            let mut lf = [0u8; 92];
            assert_eq!(
                system_parameters_info_w(0x1F, 92, lf.as_mut_ptr().cast(), 0),
                1
            );
            assert_eq!(i32::from_le_bytes(lf[..4].try_into().unwrap()), -12);
        }
    }

    /// Boucle vide en headless : GetMessage renvoie 0 avec un WM_QUIT de
    /// wParam 0, pas le wParam d'un message précédent (code de sortie au
    /// hasard de voicerecorder.exe dans la VM de validation).
    #[test]
    fn fin_de_boucle_headless_renseigne_wm_quit() {
        if gui::backend_is_interactive() {
            return;
        }
        let (r, message, wparam) = std::thread::spawn(|| unsafe {
            let mut m = gui::Msg {
                hwnd: 0,
                message: 0x0113,
                wparam: 1,
                lparam: 0,
                time: 0,
                pt: Default::default(),
                l_private: 0,
            };
            let r = get_message_a(&mut m, 0, 0, 0);
            (r, m.message, m.wparam)
        })
        .join()
        .unwrap();
        assert_eq!((r, message, wparam), (0, gui::WM_QUIT, 0));
    }

    /// Chromium (Electron) crée ses fenêtres avec MAKEINTATOM(atome rendu
    /// par RegisterClassExW) : chaque classe a son atome, relu en nom.
    #[test]
    fn atome_de_classe_relu_comme_nom() {
        let a = class_atom("Chrome_WidgetWin_0");
        let b = class_atom("Chrome_MessageWindow");
        assert!(a >= 0xC000 && b >= 0xC000 && a != b);
        assert_eq!(class_atom("chrome_widgetwin_0"), a);
        unsafe {
            assert_eq!(
                read_class_name_w(a as usize as *const u16),
                "Chrome_WidgetWin_0"
            );
            assert_eq!(
                read_class_name_a(b as usize as *const u8),
                "Chrome_MessageWindow"
            );
        }
    }

    /// Un message enregistré par nom : distinct d'un nom à l'autre, même
    /// numéro quelle que soit la casse (#666, WinForms).
    #[test]
    fn messages_enregistres_distincts() {
        let w = |s: &str| {
            let v: Vec<u16> = s.encode_utf16().chain([0]).collect();
            unsafe { register_window_message_w(v.as_ptr()) }
        };
        let enter = w("WinFormsMouseEnter");
        let unsub = w("WinFormsUnSubclass");
        assert!((0xC000..=0xFFFF).contains(&enter));
        assert!((0xC000..=0xFFFF).contains(&unsub));
        assert_ne!(enter, unsub);
        assert_eq!(w("winformsmouseenter"), enter);
        assert_eq!(
            unsafe { register_window_message_a(c"WinFormsMouseEnter".as_ptr().cast()) },
            enter
        );
        assert_eq!(w(""), 0);
    }

    /// GetClassInfoEx sur une classe prédéfinie (WinForms en dérive ses
    /// boutons) et sur une classe inconnue.
    #[test]
    fn infos_de_classe_predefinie() {
        let _teb = crate::process::test_teb();
        let name: Vec<u16> = "BUTTON".encode_utf16().chain([0]).collect();
        let mut wc = [0u8; 80];
        wc[0..4].copy_from_slice(&80u32.to_le_bytes());
        unsafe {
            assert_ne!(get_class_info_ex_w(0, name.as_ptr(), wc.as_mut_ptr()), 0);
        }
        let proc_ = usize::from_le_bytes(wc[8..16].try_into().unwrap());
        assert_eq!(Some(proc_), crate::dlls::controls::builtin_class("BUTTON"));
        assert_ne!(u32::from_le_bytes(wc[4..8].try_into().unwrap()) & 0x0008, 0); // CS_DBLCLKS
        let none: Vec<u16> = "ClasseInexistante666".encode_utf16().chain([0]).collect();
        unsafe {
            assert_eq!(get_class_info_ex_w(0, none.as_ptr(), wc.as_mut_ptr()), 0);
        }
        assert_eq!(crate::process::last_error(), 1411);
    }

    /// WinForms borne un Form par SM_CXMAXTRACK : jamais 0.
    #[test]
    fn release_capture_sans_capture_reussit() {
        assert_eq!(release_capture(), 1);
        assert_eq!(release_capture(), 1);
    }

    #[test]
    fn position_du_pointeur_suivie() {
        unsafe {
            gui::set_cursor_screen_pos(260, 200);
            let mut p = Point { x: 0, y: 0 };
            assert_eq!(get_cursor_pos(&mut p), 1);
            assert_eq!((p.x, p.y), (260, 200));
            assert_eq!(get_message_pos(), 260 | (200 << 16));
            set_cursor_pos(-5, 7);
            get_cursor_pos(&mut p);
            assert_eq!((p.x, p.y), (-5, 7));
            assert_eq!(get_message_pos(), 0xFFFB | (7 << 16));
        }
    }

    #[test]
    fn tailles_de_suivi_des_fenetres() {
        assert!(get_system_metrics(59) >= get_system_metrics(0));
        assert!(get_system_metrics(60) >= get_system_metrics(1));
        assert_eq!((get_system_metrics(34), get_system_metrics(35)), (136, 39));
        assert_eq!((get_system_metrics(11), get_system_metrics(49)), (32, 16));
    }

    static CREATE_SEEN_BY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "win64" fn proc_sous_classe(h: usize, msg: u32, w: usize, l: isize) -> isize {
        if msg == gui::WM_CREATE {
            CREATE_SEEN_BY.store(2, std::sync::atomic::Ordering::SeqCst);
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// Procédure de classe qui sous-classe la fenêtre dès WM_NCCREATE, comme
    /// NativeWindow de WinForms.
    unsafe extern "win64" fn proc_de_classe(h: usize, msg: u32, w: usize, l: isize) -> isize {
        if msg == WM_NCCREATE {
            set_window_long_ptr_w(h, GWL_WNDPROC, proc_sous_classe as *const () as isize);
            return 1;
        }
        if msg == gui::WM_CREATE {
            CREATE_SEEN_BY.store(1, std::sync::atomic::Ordering::SeqCst);
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// WM_CREATE va à la procédure courante, pas à celle de la classe (#666).
    #[test]
    fn wm_create_suit_la_sous_classe_posee_en_wm_nccreate() {
        let name: Vec<u16> = "Winlux666SousClasse".encode_utf16().chain([0]).collect();
        unsafe {
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_de_classe as *const () as usize,
                cls_extra: 0,
                wnd_extra: 0,
                instance: 0,
                icon: 0,
                cursor: 0,
                background: 0,
                menu_name: std::ptr::null(),
                class_name: name.as_ptr().cast(),
            };
            assert_ne!(register_class_w(&wc), 0);
            let hwnd = create_window_ex_w(
                0,
                name.as_ptr(),
                name.as_ptr(),
                0,
                10,
                10,
                320,
                200,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            assert_eq!(CREATE_SEEN_BY.load(std::sync::atomic::Ordering::SeqCst), 2);
            destroy_window(hwnd);
        }
    }

    #[test]
    fn peek_message_rend_wm_timer_comme_get_message() {
        unsafe {
            // Minuteur de thread (hwnd 0), comme SetTimer(NULL, …).
            let id = set_timer(0, 0, 30, 0);
            assert_ne!(id, 0);
            std::thread::sleep(std::time::Duration::from_millis(40));
            // MsgWait voit le minuteur échu…
            assert_eq!(
                msg_wait_for_multiple_objects(0, std::ptr::null(), 0, 0, 0x04FF),
                0
            );
            // …et PeekMessage(PM_REMOVE) le rend, sinon la boucle tournerait.
            let mut m: Msg = std::mem::zeroed();
            assert_eq!(peek_message_a(&mut m, 0, 0, 0, 1), 1);
            let msg = std::ptr::read_unaligned(&m).message;
            assert_eq!(msg, 0x0113);
            assert_eq!(
                msg_wait_for_multiple_objects(0, std::ptr::null(), 0, 0, 0x04FF),
                258
            );
            kill_timer(0, id);
        }
    }

    #[test]
    fn msg_wait_respecte_le_delai_et_les_evenements() {
        unsafe {
            let debut = std::time::Instant::now();
            assert_eq!(
                msg_wait_for_multiple_objects(0, std::ptr::null(), 0, 30, 0x04FF),
                258
            );
            assert!(debut.elapsed().as_millis() >= 30);
            let ev = crate::dlls::kernel32::create_event_a(0, 1, 0, std::ptr::null());
            let hs = [ev];
            assert_eq!(
                msg_wait_for_multiple_objects(1, hs.as_ptr(), 0, 0, 0x04FF),
                258
            );
            crate::dlls::kernel32::set_event(ev);
            assert_eq!(
                msg_wait_for_multiple_objects(1, hs.as_ptr(), 0, 1000, 0x04FF),
                0
            );
            // Un message posté réveille l'attente (WAIT_OBJECT_0 + count).
            crate::dlls::kernel32::reset_event(ev);
            let m = Msg {
                hwnd: 0,
                message: 0x0400,
                wparam: 0,
                lparam: 0,
                time: 0,
                pt: Point::default(),
                l_private: 0,
            };
            assert!(gui::post_to_thread(std::thread::current().id(), m));
            assert_eq!(
                msg_wait_for_multiple_objects(1, hs.as_ptr(), 0, 1000, 0x04FF),
                1
            );
            let q = gui::current_queue();
            assert!(gui::take_message(&q, 0, 0, 0).is_some());
        }
    }

    #[test]
    fn wsprintf_formate_et_termine_par_nul() {
        let mut buf = [0xAAu8; 64];
        let fmt = b"%s=%d %05u %x %X %c%% %ld\0";
        let texte = b"id\0";
        let args: [u64; 9] = [
            buf.as_mut_ptr() as u64,
            fmt.as_ptr() as u64,
            texte.as_ptr() as u64,
            (-42i32) as u32 as u64,
            7,
            255,
            255,
            b'Z' as u64,
            123_456,
        ];
        let n = unsafe { imp_wsprintfa(args.as_ptr()) };
        let attendu = b"id=-42 00007 ff FF Z% 123456";
        assert_eq!(n as usize, attendu.len());
        assert_eq!(&buf[..attendu.len()], attendu);
        assert_eq!(buf[attendu.len()], 0);

        let mut large = [0xAAAAu16; 64];
        let wfmt: Vec<u16> = "%s %d\0".encode_utf16().collect();
        let wtexte: Vec<u16> = "été\0".encode_utf16().collect();
        let args: [u64; 4] = [
            large.as_mut_ptr() as u64,
            wfmt.as_ptr() as u64,
            wtexte.as_ptr() as u64,
            3,
        ];
        let n = unsafe { imp_wsprintfw(args.as_ptr()) };
        assert_eq!(n, 5);
        assert_eq!(String::from_utf16_lossy(&large[..5]), "été 3");
        assert_eq!(large[5], 0);

        // Pointeurs nuls : rien d'écrit.
        let args: [u64; 2] = [0, fmt.as_ptr() as u64];
        assert_eq!(unsafe { imp_wsprintfa(args.as_ptr()) }, 0);
    }

    #[test]
    fn draw_text_negatif_signifie_chaine_terminee_par_nul() {
        const DT_CALCRECT: u32 = 0x0400;
        let texte = b"abc\0ignore";
        let mut nul = Rect::default();
        let mut negatif = Rect::default();
        let mut explicite = Rect::default();
        let h1 = unsafe { draw_text_a(0, texte.as_ptr(), -1, &mut nul, DT_CALCRECT) };
        let h2 = unsafe { draw_text_a(0, texte.as_ptr(), -7, &mut negatif, DT_CALCRECT) };
        let h3 = unsafe { draw_text_a(0, texte.as_ptr(), 3, &mut explicite, DT_CALCRECT) };
        assert!(nul.right > 0 && nul.bottom > 0);
        assert_eq!((h1, nul), (h2, negatif));
        assert_eq!((h1, nul), (h3, explicite));
        let large: Vec<u16> = "abc\0ignore".encode_utf16().collect();
        let mut w = Rect::default();
        let hw = unsafe { draw_text_w(0, large.as_ptr(), -3, &mut w, DT_CALCRECT) };
        assert_eq!((hw, w), (h1, nul));
    }
}

#[cfg(test)]
mod tests_messages {
    use super::*;

    unsafe extern "win64" fn echo_proc(_h: usize, msg: u32, w: usize, l: isize) -> isize {
        msg as isize + w as isize + l
    }

    #[test]
    fn station_interactive_et_procedure_precedente() {
        unsafe {
            let ws = get_process_window_station();
            let mut flags = [0u8; 12];
            let mut need = 0;
            assert_eq!(
                get_user_object_information_w(ws, 1, flags.as_mut_ptr(), 12, &mut need),
                1
            );
            assert_eq!((need, flags[8] & 1), (12, 1)); // WSF_VISIBLE
            let mut name = [0u8; 4];
            assert_eq!(
                get_user_object_information_w(ws, 2, name.as_mut_ptr(), 4, &mut need),
                0
            );
            assert_eq!(need, 16); // « WinSta0 » + zéro, en UTF-16
            assert_eq!(
                get_user_object_information_w(0, 1, flags.as_mut_ptr(), 12, &mut need),
                0
            );
            assert_eq!(
                call_window_proc(echo_proc as *const () as usize, 1, 2, 3, 4),
                9
            );
            assert_eq!(call_window_proc(0, 1, 2, 3, 4), 0);
        }
    }

    thread_local! {
        static VUS: std::cell::RefCell<Vec<(usize, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    unsafe extern "win64" fn crochet(code: i32, w: usize, l: isize) -> isize {
        let m = &mut *(l as *mut Msg);
        VUS.with(|v| v.borrow_mut().push((w, m.message)));
        m.wparam = 77; // un crochet peut modifier le MSG
        call_next_hook_ex(0, code, w, l)
    }

    unsafe extern "win64" fn crochet_suivant(_code: i32, w: usize, _l: isize) -> isize {
        VUS.with(|v| v.borrow_mut().push((w, 0xFFFF)));
        0
    }

    /// Une boîte possédée reste au-dessus de la fenêtre principale qui
    /// contient son propriétaire, même désigné par un enfant (#1068).
    #[test]
    fn proprietaire_ramene_a_sa_fenetre_principale() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let top = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                50,
                50,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            let child = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_CHILD,
                0,
                0,
                10,
                10,
                top,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(top, 0);
            assert_ne!(child, 0);
            assert_eq!(owner_top_level(child), top);
            assert_eq!(owner_top_level(top), top);
            assert_eq!(owner_top_level(0x7777_0000), 0);
            destroy_window(top);
        }
    }

    #[test]
    fn real_get_window_class_rend_le_nom_de_classe() {
        unsafe {
            let class: Vec<u16> = "BUTTON\0".encode_utf16().collect();
            let hwnd = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                10,
                10,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            let mut buf = [0u16; 32];
            let n = real_get_window_class_w(hwnd, buf.as_mut_ptr(), 32);
            assert_eq!(
                String::from_utf16_lossy(&buf[..n as usize]).to_ascii_uppercase(),
                "BUTTON"
            );
            let mut a = [0u8; 32];
            assert_eq!(real_get_window_class_a(hwnd, a.as_mut_ptr(), 32), n);
            assert_eq!(
                real_get_window_class_w(0x7777_0000, buf.as_mut_ptr(), 32),
                0
            );
            destroy_window(hwnd);
        }
    }

    #[test]
    fn proprietaire_donne_apres_creation_par_gwlp_hwndparent() {
        // #1068 : Qt 5 rattache sa boîte à la fenêtre principale après coup.
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let make = |style: u32| {
                create_window_ex_w(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    style,
                    0,
                    0,
                    100,
                    50,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let main = make(0x00CF_0000);
            let dialog = make(0x80C8_0000);
            assert_ne!(main, 0);
            assert_ne!(dialog, 0);
            assert_eq!(set_window_long_ptr_w(dialog, -8, main as isize), 0);
            assert_eq!(get_window_long_ptr_w(dialog, -8), main as isize);
            destroy_window(dialog);
            destroy_window(main);
        }
    }

    #[test]
    fn region_a_repeindre_couvre_la_zone_client() {
        // #1054 : wxHtmlWindow lit GetUpdateRgn puis GetRgnBox et ne dessine
        // que ce qui tombe dedans ; une région vide le laissait blanc.
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                120,
                40,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            let rgn = crate::dlls::gdi32::create_rect_rgn(0, 0, 0, 0);
            assert_eq!(get_update_rgn(hwnd, rgn, 0), 2); // SIMPLEREGION
            let mut r = Rect::default();
            crate::dlls::gdi32::get_rgn_box(rgn, &mut r);
            let client = gui::window_client_rect(hwnd).unwrap();
            assert_eq!(
                (r.left, r.top, r.right, r.bottom),
                (client.left, client.top, client.right, client.bottom)
            );
            assert!(r.right > r.left && r.bottom > r.top);
            destroy_window(hwnd);
        }
    }

    #[test]
    fn crochet_getmessage_voit_chaque_message_retire() {
        // Qt 5 relance ses événements différés depuis ce crochet (#938).
        unsafe {
            let tid = crate::dlls::kernel32::get_current_thread_id();
            let q = gui::current_queue();
            let h2 = set_windows_hook_ex_w(3, crochet_suivant as *const () as usize, 0, tid);
            let h1 = set_windows_hook_ex_w(3, crochet as *const () as usize, 0, tid);
            gui::post_to_thread(std::thread::current().id(), Msg::new(0, 0x8001, 5, 0));
            let mut m: Msg = std::mem::zeroed();
            assert_eq!(peek_message_w(&mut m, 0, 0x8001, 0x8001, 0), 1);
            assert_eq!(peek_message_w(&mut m, 0, 0x8001, 0x8001, 1), 1);
            assert_eq!(m.wparam, 77);
            assert_eq!(
                VUS.with(|v| v.borrow().clone()),
                vec![(0, 0x8001), (0, 0xFFFF), (1, 0x8001), (1, 0xFFFF)]
            );
            assert_eq!(unhook_windows_hook_ex(h1), 1);
            assert_eq!(unhook_windows_hook_ex(h2), 1);
            gui::post_to_thread(std::thread::current().id(), Msg::new(0, 0x8001, 5, 0));
            assert_eq!(peek_message_w(&mut m, 0, 0x8001, 0x8001, 1), 1);
            assert_eq!(m.wparam, 5);
            assert_eq!(VUS.with(|v| v.borrow().len()), 4);
            drop(q);
        }
    }
}

#[cfg(test)]
mod tests_winforms {
    use super::*;

    #[test]
    fn messages_enregistres_distincts_et_stables() {
        let w = |s: &str| {
            let v: Vec<u16> = s.encode_utf16().chain([0]).collect();
            unsafe { register_window_message_w(v.as_ptr()) }
        };
        let a = w("WinFormsUnSubclass");
        let b = w("WindowsForms12_ThreadCallbackMessage");
        assert!((0xC000..=0xFFFF).contains(&a) && (0xC000..=0xFFFF).contains(&b));
        assert_ne!(a, b, "deux noms, deux messages");
        assert_eq!(a, w("winformsunsubclass"), "sans casse");
        assert_eq!(a, unsafe {
            register_window_message_a(c"WinFormsUnSubclass".as_ptr().cast())
        });
        assert_eq!(w(""), 0);
    }

    #[test]
    fn get_class_info_ex_classe_predefinie_et_inconnue() {
        let mut wc = [0u8; 80];
        wc[..4].copy_from_slice(&80u32.to_le_bytes());
        let name: Vec<u16> = "BUTTON".encode_utf16().chain([0]).collect();
        let atom = unsafe { get_class_info_ex_w(0, name.as_ptr(), wc.as_mut_ptr().cast()) };
        assert_ne!(atom, 0);
        let proc_ = usize::from_le_bytes(wc[8..16].try_into().unwrap());
        assert_ne!(proc_, 0, "lpfnWndProc rempli");
        let unknown: Vec<u16> = "PasUneClasse_269".encode_utf16().chain([0]).collect();
        let r = unsafe { get_class_info_ex_w(0, unknown.as_ptr(), wc.as_mut_ptr().cast()) };
        assert_eq!(r, 0);
    }

    #[test]
    fn premier_plan_autorise_et_verrou_sans_effet() {
        // ASFW_ANY (-1) et un identifiant quelconque (#1074).
        assert_eq!(allow_set_foreground_window(u32::MAX), 1);
        assert_eq!(allow_set_foreground_window(4242), 1);
        assert_eq!(lock_set_foreground_window(1), 1);
        assert_eq!(lock_set_foreground_window(2), 1);
        assert_eq!(lock_set_foreground_window(7), 0);
    }
}
