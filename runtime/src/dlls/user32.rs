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
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
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
    cursor: usize,
) -> usize {
    if name.is_empty() || wnd_proc == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    // Classe déjà enregistrée par ce module et pas encore désenregistrée :
    // 0 et ERROR_CLASS_ALREADY_EXISTS, comme sous Windows (#1630). Une
    // classe de même nom venue d'un autre module ou de Winlux garde
    // l'ancienne tolérance et rend son atome.
    let key = (name.to_ascii_lowercase(), instance);
    let info = ClassInfo {
        wnd_proc,
        background,
        instance,
    };
    let again = {
        let mut app = lock(app_classes());
        if app.registered.contains(&key) {
            crate::process::set_last_error(ERROR_CLASS_ALREADY_EXISTS);
            return 0;
        }
        app.registered.insert(key.clone());
        app.unregistered.remove(&key)
    };
    gui::set_class_style(&name, style);
    gui::set_class_cursor(&name, cursor);
    if again {
        // Réenregistrement après UnregisterClass : la nouvelle procédure
        // remplace l'ancienne.
        gui::replace_class(&name, info);
    } else {
        gui::register_class(&name, info);
    }
    class_atom(&name) as usize
}

const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
const ERROR_CLASS_DOES_NOT_EXIST: u32 = 1411;

/// Classes enregistrées par le programme, par (nom en minuscules, module).
#[derive(Default)]
struct AppClasses {
    registered: std::collections::HashSet<(String, usize)>,
    unregistered: std::collections::HashSet<(String, usize)>,
}

fn app_classes() -> &'static Mutex<AppClasses> {
    static C: OnceLock<Mutex<AppClasses>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(AppClasses::default()))
}

/// UnregisterClass : la classe du module peut être enregistrée de nouveau.
/// Les fenêtres encore ouvertes gardent leur procédure.
fn unregister_class(name: Option<String>, instance: usize) -> i32 {
    let Some(name) = name else {
        // Atome : la classe reste connue, rien à retirer.
        return 1;
    };
    let key = (name.to_ascii_lowercase(), instance);
    let mut app = lock(app_classes());
    if app.registered.remove(&key) {
        app.unregistered.insert(key);
        1
    } else {
        crate::process::set_last_error(ERROR_CLASS_DOES_NOT_EXIST);
        0
    }
}

pub unsafe extern "win64" fn register_class_a(wc: *const WndClassA) -> u16 {
    if wc.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wc;
    let name = read_class_name_a(c.class_name);
    remember_class_menu(&name, c.instance, c.menu_name, false);
    register_class_common(
        name,
        c.style,
        c.wnd_proc,
        c.background,
        c.instance,
        c.cursor,
    ) as u16
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
    register_class_common(
        name,
        c.style,
        c.wnd_proc,
        c.background,
        c.instance,
        c.cursor,
    ) as u16
}

pub unsafe extern "win64" fn register_class_ex_a(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_a(c.class_name);
    remember_class_menu(&name, c.instance, c.menu_name, false);
    register_class_common(
        name,
        c.style,
        c.wnd_proc,
        c.background,
        c.instance,
        c.cursor,
    ) as u16
}

pub unsafe extern "win64" fn register_class_ex_w(wcx: *const WndClassExA) -> u16 {
    if wcx.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let c = &*wcx;
    let name = read_class_name_w(c.class_name as *const u16);
    remember_class_menu(&name, c.instance, c.menu_name, true);
    register_class_common(
        name,
        c.style,
        c.wnd_proc,
        c.background,
        c.instance,
        c.cursor,
    ) as u16
}

// ---------- Création de fenêtres ----------

/// `WINLUX_WINDOW_DIAG=1` : création, placement et affichage des fenêtres
/// sur la sortie d'erreur (diagnostic des fenêtres filles absentes).
pub(crate) fn window_diag() -> bool {
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

/// Parent spécial de CreateWindowEx : fenêtre qui ne reçoit que des
/// messages, sans rien d'affiché.
const HWND_MESSAGE: usize = -3isize as usize;

fn message_only_windows() -> &'static std::sync::Mutex<std::collections::HashSet<usize>> {
    static SET: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<usize>>> =
        std::sync::OnceLock::new();
    SET.get_or_init(Default::default)
}

/// `hwnd` est-elle une fenêtre « message seulement » (#1586) ?
pub fn is_message_only(hwnd: usize) -> bool {
    hwnd != 0 && lock(message_only_windows()).contains(&hwnd)
}

/// Taille d'une fenêtre de premier niveau créée en CW_USEDEFAULT : les
/// trois quarts de la zone de travail, au moins 800 × 600 sans dépasser
/// l'écran. Un 800 × 600 fixe faisait passer le ruban de l'Éditeur de
/// ScreenToGif sur deux lignes (#1277).
fn default_window_size() -> (i32, i32) {
    let area = gui::work_area();
    let (aw, ah) = (area.right - area.left, area.bottom - area.top);
    let pick = |full: i32, min: i32| (full * 3 / 4).max(min).min(full.max(1));
    (pick(aw, 800), pick(ah, 600))
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
    let message_root = parent == HWND_MESSAGE;
    // Comme Windows : une fenêtre WS_CHILD sans parent est refusée, au lieu
    // de devenir une fenêtre isolée sans titre dans la barre des tâches.
    if style & WS_CHILD != 0 && parent == 0 {
        crate::process::set_last_error(1406); // ERROR_TLW_WITH_WSCHILD
        return 0;
    }
    // Fenêtre « message seulement » (parent HWND_MESSAGE) et ses enfants :
    // jamais affichées, donc aucune fenêtre X. pygame 32 bits (SDL) en
    // créait une par seconde en boucle, chacune devenue une fenêtre X 1×1
    // (#1586). Le parent rendu par GetParent est nul, comme sous Windows.
    let message_only = parent == HWND_MESSAGE || is_message_only(parent);
    let parent = if parent == HWND_MESSAGE { 0 } else { parent };
    let hwnd = gui::alloc_hwnd();
    if message_only {
        lock(message_only_windows()).insert(hwnd);
    }
    // Tailles CW_USEDEFAULT : fenêtre raisonnable par défaut. Un contrôle
    // enfant garde la taille demandée, même nulle (barre d'état, flèches
    // haut/bas : le contrôle se place lui-même pendant WM_CREATE).
    let child = style & WS_CHILD != 0 && parent != 0 && !message_root;
    let (w, h, px, py) = if child {
        let size = |v: i32| if v == gui::CW_USEDEFAULT { 0 } else { v.max(0) };
        let pos = |v: i32| if v == gui::CW_USEDEFAULT { 0 } else { v };
        (size(cx), size(cy), pos(x), pos(y))
    } else {
        // CW_USEDEFAULT en largeur : taille par défaut, la hauteur est alors
        // ignorée (comme sous Windows). Une taille nulle demandée reste
        // nulle : la fenêtre cachée de TApplication (VCL) est créée en 0×0
        // puis « affichée » par SWP_SHOWWINDOW ; elle devenait une fenêtre
        // vide de 800×600 sans titre (WinSCP, #1276).
        let (w, h) = if cx == gui::CW_USEDEFAULT {
            default_window_size()
        } else {
            (
                cx.max(0),
                if cy == gui::CW_USEDEFAULT {
                    default_window_size().1
                } else {
                    cy.max(0)
                },
            )
        };
        (
            w,
            h,
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
        if message_only {
            return;
        }
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
            if style & 0x8000_0000 != 0 {
                crate::thunks::api_note(&format!(
                    "CreateWindowEx {hwnd:#x} {class:?} : style {style:#x}, ex {ex_style:#x}, parent {parent:#x}, hors gestionnaire {} (#1222)",
                    floating_popup(style, ex_style)
                ));
            }
            if class.eq_ignore_ascii_case(super::controls::popup::CLASS)
                || floating_popup(style, ex_style)
            {
                // Menu surgissant : ni cadre ni placement du gestionnaire.
                b.set_popup(hwnd);
            } else if layered_without_caption(style, ex_style) {
                // Fenêtre en couches sans barre de titre (WPF transparente,
                // Flow Launcher) : le programme dessine tout, cadre compris.
                b.set_decorations(hwnd, 0);
            } else if let Some(level) = popup_decorations(style) {
                b.set_decorations(hwnd, level);
            }
            // Fenêtre possédée : toujours au-dessus de sa propriétaire, comme
            // sous Windows (boîte « Vie privée » de VLC, #1068).
            let owner = owner_top_level(parent);
            if owner != 0 {
                b.set_owner(hwnd, owner);
            }
            if captioned_tool_window(style, ex_style) {
                b.set_tool_type(hwnd);
            } else if owner != 0 && style & 0x00C0_0000 == 0x00C0_0000 {
                b.set_dialog_type(hwnd);
            }
        }
    });
    // Visible des autres processus du compte (#1228) : le processus GPU de
    // Chromium reçoit ce HWND et ANGLE le vérifie par IsWindow.
    publish_shared(hwnd);

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

    // Crochet WH_CBT (HCBT_CREATEWND) avant tout message : MFC y sous-classe
    // la fenêtre (AfxWndProc) et la rattache à son CWnd. Sans lui, une
    // fenêtre MFC garde DefWindowProc et MFC lit un CWnd jamais rempli
    // (Pythonwin, #1654). Une valeur non nulle refuse la création.
    let mut cs = cs;
    let wide_class: Vec<u16> = class.encode_utf16().chain([0]).collect();
    let wide_title: Vec<u16> = title.encode_utf16().chain([0]).collect();
    if super::controls::message_is_wide(hwnd) {
        cs.name = wide_title.as_ptr().cast();
        cs.class = wide_class.as_ptr().cast();
    }
    if cbt_create_window(hwnd, &mut cs) {
        gui::destroy_window(hwnd);
        forget_window_longs(hwnd);
        return 0;
    }
    cs.name = title_z.as_ptr().cast();
    cs.class = class_z.as_ptr().cast();

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
        if shown && !empty_top_level(hwnd) {
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

/// WM_SETCURSOR, que Windows envoie quand il retire de la file un message
/// souris de la zone cliente, la souris n'étant pas capturée : l'application
/// y choisit son curseur (barre en I sur le texte de Sumatra PDF, #1253).
/// Sans lui, le pointeur restait une flèche partout.
unsafe fn send_set_cursor(m: &Msg) {
    const HTCLIENT: isize = 1;
    if m.hwnd == 0 || gui::get_capture() != 0 {
        return;
    }
    if !matches!(m.message, 0x0200 | 0x0201 | 0x0204 | 0x0207 | 0x020B) {
        return;
    }
    send_message_w(
        m.hwnd,
        0x0020,
        m.hwnd,
        ((m.message as isize) << 16) | HTCLIENT,
    );
}

const WA_ACTIVE: usize = 1;
const WA_CLICKACTIVE: usize = 2;

/// Activation au clic, que Windows fait quand il retire de la file un
/// bouton enfoncé : WM_MOUSEACTIVATE à la fenêtre cliquée, puis, sauf
/// MA_NOACTIVATE, WM_NCACTIVATE / WM_ACTIVATE(WA_CLICKACTIVE) et
/// WM_SETFOCUS, avant le WM_LBUTTONDOWN lui-même. Faite ici seulement pour
/// les fenêtres que le gestionnaire de fenêtres n'active pas : la boîte
/// « Erreur du service Steam » ne réagissait à aucun bouton (#1804).
/// MA_ACTIVATEANDEAT active sans retirer le clic (simplification).
unsafe fn click_activate(m: &Msg) {
    const WM_MOUSEACTIVATE: u32 = 0x0021;
    const HTCLIENT: isize = 1;
    const MA_NOACTIVATE: isize = 3;
    const MA_NOACTIVATEANDEAT: isize = 4;
    if m.hwnd == 0
        || gui::get_capture() != 0
        || !matches!(m.message, 0x0201 | 0x0204 | 0x0207 | 0x020B)
    {
        return;
    }
    let Some(top) = gui::click_activation_target(m.hwnd) else {
        return;
    };
    let answer = send_message_w(
        m.hwnd,
        WM_MOUSEACTIVATE,
        top,
        ((m.message as isize) << 16) | HTCLIENT,
    );
    if matches!(answer, MA_NOACTIVATE | MA_NOACTIVATEANDEAT) || !gui::is_window(top) {
        return;
    }
    activate_for_focus(top, WA_CLICKACTIVE);
    // Le focus reste au contrôle de la fenêtre qui l'avait déjà.
    if gui::top_level_of(gui::focused_window()) != top {
        set_focus(top);
    }
}

unsafe fn get_message_inner(out: *mut Msg, hwnd: usize, min: u32, max: u32) -> i32 {
    loop {
        gui::pump_backend();
        super::chromium_win32::deliver_win_events();
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
            click_activate(&m);
            send_set_cursor(&m);
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
    super::chromium_win32::deliver_win_events();
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
            click_activate(&m);
            send_set_cursor(&m);
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
    // ShowWindowAsync venu d'un autre thread (#2110).
    if super::teams_win32::dispatch_private(m.hwnd, m.message, m.wparam) {
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
    recheck_own_frame();
    if m.l_private == gui::BACKEND_GEOMETRY {
        geometry_changed(m.hwnd, m.message);
    }
    let (hwnd, message, wparam, lparam) = caption_click(m.hwnd, m.message, m.wparam, m.lparam)
        .unwrap_or((m.hwnd, m.message, m.wparam, m.lparam));
    let wnd_proc = gui::wnd_proc_of(hwnd);
    match wnd_proc {
        Some(wp) => gui::call_wndproc(wp, hwnd, message, wparam, lparam),
        None => gui::def_window_proc(hwnd, message, wparam, lparam),
    }
}

/// Le gestionnaire de fenêtres a agrandi, restauré, déplacé ou
/// redimensionné une fenêtre de premier niveau : Windows le fait par
/// SetWindowPos, qui envoie WM_WINDOWPOSCHANGED avant WM_SIZE et WM_MOVE.
/// Chromium ne recalcule sa zone cliente (et la taille de sa surface de
/// rendu) que sur WM_WINDOWPOSCHANGED : sans lui, Claude agrandi gardait son
/// contenu en 1000x800 dans un coin, le reste noir, et ses boutons ne
/// répondaient plus (#1476).
unsafe fn geometry_changed(hwnd: usize, message: u32) {
    if crate::wow32::pointer_size() != 8 || gui::window_parent(hwnd).unwrap_or(0) != 0 {
        return;
    }
    let Some(rect) = gui::window_rect(hwnd) else {
        return;
    };
    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE;
    flags |= if message == gui::WM_SIZE {
        SWP_NOMOVE
    } else {
        SWP_NOSIZE
    };
    let mut pos = WindowPos {
        hwnd,
        insert_after: 0,
        x: rect.left,
        y: rect.top,
        cx: rect.width(),
        cy: rect.height(),
        flags,
    };
    send_message_w(hwnd, WM_WINDOWPOSCHANGED, 0, &mut pos as *mut _ as isize);
}

/// Clic gauche dans une fenêtre qui dessine sa propre barre de titre (son
/// cadre a été retiré, voir `frame_changed`) : comme Windows, le point est
/// soumis à WM_NCHITTEST, et sur la légende ou un bord le clic devient
/// WM_NCLBUTTONDOWN en coordonnées écran ; un second clic rapide sur la
/// légende devient WM_NCLBUTTONDBLCLK. DefWindowProc lance alors le
/// déplacement par le gestionnaire de fenêtres, ou agrandit et restaure.
/// Chromium couvre sa page d'un enfant transparent (« Chrome Legacy
/// Window », fenêtre X InputOnly depuis #1337) : c'est lui qui reçoit le
/// clic, mais la question et le message vont à la fenêtre de premier niveau,
/// qui seule connaît ses zones glissables (#1477). Les boutons de légende
/// (HTMINBUTTON, HTMAXBUTTON, HTCLOSE) restent des clics clients : Chromium
/// les traite lui-même. Renvoie la fenêtre et le message à distribuer.
unsafe fn caption_click(
    hwnd: usize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> Option<(usize, u32, usize, isize)> {
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    thread_local! {
        /// Dernier clic sur une légende : instant, point écran, fenêtre.
        static LAST: std::cell::Cell<Option<(std::time::Instant, i32, i32, usize)>> =
            const { std::cell::Cell::new(None) };
    }
    if !matches!(message, WM_LBUTTONDOWN | WM_LBUTTONDBLCLK) {
        return None;
    }
    let own = |h: usize| {
        OWN_FRAME
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&h)
    };
    let top = if own(hwnd) {
        hwnd
    } else if gui::is_see_through_child(hwnd) {
        let top = gui::top_level_of(hwnd);
        if top == 0 || !own(top) {
            return None;
        }
        top
    } else {
        return None;
    };
    let origin = gui::screen_rect(hwnd)?;
    let x = origin.left + (lparam as u32 as u16 as i16 as i32);
    let y = origin.top + ((lparam as u32 >> 16) as u16 as i16 as i32);
    let screen = pack_i16_pair(x, y);
    let hit = send_message_w(top, 0x0084, 0, screen);
    if window_diag() {
        eprintln!(
            "winlux[fenetre] clic {hwnd:#x} → légende de {top:#x} ? WM_NCHITTEST({x},{y}) = {hit}"
        );
    }
    let caption = hit == 2;
    if !caption && !(10..=17).contains(&hit) {
        return None;
    }
    let _ = wparam;
    if !caption {
        LAST.with(|l| l.set(None));
        return Some((top, 0x00A1, hit as usize, screen));
    }
    let now = std::time::Instant::now();
    let double = LAST.with(|l| {
        l.get().is_some_and(|(t, lx, ly, lh)| {
            lh == top
                && now.duration_since(t) <= std::time::Duration::from_millis(500)
                && (x - lx).abs() <= 4
                && (y - ly).abs() <= 4
        })
    });
    if double {
        LAST.with(|l| l.set(None));
        Some((top, 0x00A3, hit as usize, screen))
    } else {
        LAST.with(|l| l.set(Some((now, x, y, top))));
        Some((top, 0x00A1, hit as usize, screen))
    }
}

pub unsafe extern "win64" fn dispatch_message_w(msg: *const Msg) -> isize {
    dispatch_message_a(msg)
}

pub unsafe extern "win64" fn post_quit_message(code: i32) {
    let q = gui::current_queue();
    gui::post_quit(&q, code);
}

const HWND_BROADCAST: usize = 0xFFFF;

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
    // HWND_BROADCAST : chaque fenêtre de premier niveau du processus
    // reçoit le message ; les autres programmes n'en ont pas d'ici. Comme
    // Windows, l'envoi réussit même sans destinataire (BroadcastUnload…
    // d'Acrobat échouait, #1757).
    if hwnd == HWND_BROADCAST {
        for w in gui::enum_top_level_windows() {
            post_message_a(w, message, wparam, lparam);
        }
        return 1;
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

/// Fenêtre de premier niveau sans surface (largeur ou hauteur nulle) : elle
/// peut être « visible » pour le programme, mais n'est pas montrée au
/// gestionnaire de fenêtres (X11 n'accepte pas de fenêtre de taille nulle,
/// le cadre seul apparaissait). Elle l'est dès qu'elle reçoit une taille.
fn empty_top_level(hwnd: usize) -> bool {
    gui::window_style(hwnd).is_some_and(|s| s & WS_CHILD == 0)
        && gui::window_rect(hwnd).is_some_and(|r| r.width() <= 0 || r.height() <= 0)
}

pub unsafe extern "win64" fn show_window(hwnd: usize, cmd: i32) -> i32 {
    if window_diag() {
        eprintln!("winlux[fenetre] afficher {hwnd:#x} cmd={cmd}");
    }
    // WM_SHOWWINDOW avant le changement de visibilité, comme Windows (SDL
    // en tire ses évènements SHOWN et HIDDEN, #1421).
    let show = cmd != gui::SW_HIDE;
    if gui::visible_flag(hwnd).is_some_and(|v| v != show) {
        send_message_w(hwnd, gui::WM_SHOWWINDOW, show as usize, 0);
    }
    if show && gui::visible_flag(hwnd) == Some(false) && !matches!(cmd, 2 | 3 | 6 | 7 | 11) {
        fit_to_work_area(hwnd);
    }
    let was = gui::set_visible(hwnd, show);
    // SW_SHOWMINNOACTIVE (7), SW_SHOWNA (8), SW_SHOWNOACTIVATE (4) : la
    // fenêtre s'affiche sans prendre le focus (#1127).
    if matches!(cmd, 4 | 7 | 8) && !was {
        with_backend(|b| b.set_no_initial_focus(hwnd));
    }
    // SW_MAXIMIZE (3) donne une taille à la fenêtre : elle est montrée.
    let map = cmd != gui::SW_HIDE && (cmd == 3 || !empty_top_level(hwnd));
    with_backend(|b| b.show_window(hwnd, map));
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
    if was != show && gui::window_parent(hwnd).unwrap_or(0) == 0 {
        use super::chromium_win32 as w;
        let event = if show {
            w::EVENT_OBJECT_SHOW
        } else {
            w::EVENT_OBJECT_HIDE
        };
        w::notify_win_event(event, hwnd);
    }
    was as i32
}

/// Fenêtre de premier niveau à barre de titre affichée : si son cadre
/// dépasse la zone de travail de son écran, elle y est ramenée, et
/// rétrécie si elle se redimensionne et n'y tient pas. Audacity restaure la
/// position qu'il a enregistrée sans la contraindre ; sous la barre des
/// tâches, la fenêtre restait coupée et la position fautive était
/// réenregistrée à chaque fermeture (#1752). Les fenêtres sans cadre, les
/// enfants et les fenêtres agrandies ou réduites ne sont pas touchées.
unsafe fn fit_to_work_area(hwnd: usize) {
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_THICKFRAME: u32 = 0x0004_0000;
    const WS_MINIMIZE: u32 = 0x2000_0000;
    const WS_MAXIMIZE: u32 = 0x0100_0000;
    let (Some(style), Some(rect)) = (gui::window_style(hwnd), gui::window_rect(hwnd)) else {
        return;
    };
    if style & WS_CHILD != 0
        || style & WS_CAPTION != WS_CAPTION
        || style & (WS_MINIMIZE | WS_MAXIMIZE) != 0
        || gui::window_parent(hwnd).unwrap_or(0) != 0
        || OWN_FRAME
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&hwnd)
    {
        return;
    }
    let monitors = super::monitors::list();
    let index = super::monitors::from_rect_in(&monitors, &rect, 2);
    let Some(monitor) = monitors.get(index.wrapping_sub(1)) else {
        return;
    };
    let work = super::monitors::work_rect(&monitor.rect, &gui::work_area());
    let resizable = style & WS_THICKFRAME != 0;
    // Loin des bords, même avec un cadre épais : le gestionnaire n'est pas
    // interrogé.
    if super::monitors::fit_in_work_area(&rect, &work, [64; 4], resizable) == rect {
        return;
    }
    let mut frame = None;
    with_backend(|b| frame = b.frame_extents(hwnd));
    let fitted =
        super::monitors::fit_in_work_area(&rect, &work, frame.unwrap_or([0; 4]), resizable);
    if fitted == rect {
        return;
    }
    crate::thunks::api_note(&format!(
        "ShowWindow {hwnd:#x} : ({},{} {}x{}) ramenée dans la zone de travail ({},{} {}x{}) (#1752)",
        rect.left,
        rect.top,
        rect.width(),
        rect.height(),
        fitted.left,
        fitted.top,
        fitted.width(),
        fitted.height()
    ));
    set_window_pos(
        hwnd,
        0,
        fitted.left,
        fitted.top,
        fitted.width(),
        fitted.height(),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
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
    lock(message_only_windows()).remove(&hwnd);
    pending_frame().retain(|(h, _, _)| *h != hwnd);
    OWN_FRAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|&h| h != hwnd);
    // Fenêtre possédée qui a le focus (elle ou une de ses filles) : comme
    // Windows, sa destruction rend la main à son propriétaire. Qt 5 donne
    // le focus à la fenêtre cachée de sa boîte de fichiers native avant de
    // la détruire ; sans ce retour, la fenêtre de VLC restait inactive et sa
    // barre de menus gardait les libellés grisés de l'attente (#1311).
    let owner = owner_to_refocus(hwnd);
    if gui::destroy_window(hwnd) {
        if owner != 0 && gui::focused_window() == 0 && gui::is_window(owner) {
            crate::thunks::api_note(&format!(
                "DestroyWindow {hwnd:#x} : focus rendu à {owner:#x} (#1311)"
            ));
            set_focus(owner);
        }
        1
    } else {
        0
    }
}

/// Fenêtre à qui rendre le focus quand `hwnd` est détruite : seulement pour
/// une fenêtre de premier niveau qui a le focus du fil. Son propriétaire s'il
/// est visible et activé, sinon la fenêtre visible et activée suivante du fil
/// dans l'ordre Z (#1311).
unsafe fn owner_to_refocus(hwnd: usize) -> usize {
    let Some(style) = gui::window_style(hwnd) else {
        return 0;
    };
    if style & WS_CHILD != 0 {
        return 0;
    }
    let focus = gui::focused_window();
    let mut inside = focus;
    for _ in 0..64 {
        if inside == 0 || inside == hwnd {
            break;
        }
        inside = gui::window_parent(inside).unwrap_or(0);
    }
    if focus == 0 || inside != hwnd {
        return 0;
    }
    // Le propriétaire d'abord, comme Windows.
    let owner = owner_top_level(get_window_long_ptr_w(hwnd, GWLP_HWNDPARENT) as usize);
    let candidates = gui::thread_visible_top_levels();
    if owner != 0
        && owner != hwnd
        && candidates
            .iter()
            .any(|&(h, s)| h == owner && s & WS_DISABLED == 0)
    {
        return owner;
    }
    // Sinon la fenêtre suivante dans l'ordre Z : Qt 5 retire le propriétaire
    // de la fenêtre de sa boîte native (SetWindowLongPtr(GWLP_HWNDPARENT, 0))
    // juste avant de la détruire. Une fenêtre à barre de titre ou sans
    // WS_POPUP, pas un menu ni une bulle.
    candidates
        .into_iter()
        .find(|&(h, s)| {
            h != hwnd
                && s & WS_DISABLED == 0
                && (s & 0x00C0_0000 == 0x00C0_0000 || s & 0x8000_0000 == 0)
        })
        .map(|(h, _)| h)
        .unwrap_or(0)
}

pub unsafe extern "win64" fn is_window(hwnd: usize) -> i32 {
    (gui::wnd_proc_of(hwnd).is_some() || gui::foreign_window(hwnd).is_some()) as i32
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

/// Avant de donner le focus à `hwnd`, active sa fenêtre de premier niveau
/// comme SetActiveWindow sous Windows : l'ancienne reçoit WM_NCACTIVATE(FALSE)
/// puis WM_ACTIVATE(WA_INACTIVE), la nouvelle WM_NCACTIVATE(TRUE) puis
/// WM_ACTIVATE(WA_ACTIVE), envoyés et non postés. Le FocusIn de X11 qui suit
/// trouve la fenêtre déjà active et ne poste plus rien (#1504).
unsafe fn activate_for_focus(hwnd: usize, state: usize) {
    let top = gui::top_level_of(hwnd);
    if top == 0 {
        return;
    }
    let Some(prev) = gui::activate_top_for_focus(top) else {
        return;
    };
    let me = std::thread::current().id();
    let deliver = |h: usize, msg: u32, wparam: usize, lparam: isize| {
        if gui::window_owner(h).as_ref() == Some(&me) {
            if let Some(wp) = gui::wnd_proc_of(h) {
                gui::call_wndproc(wp, h, msg, wparam, lparam);
            }
        } else {
            gui::post_to_window(h, gui::Msg::new(h, msg, wparam, lparam));
        }
    };
    if prev != 0 && gui::is_window(prev) {
        deliver(prev, gui::WM_NCACTIVATE, 0, 0);
        deliver(prev, gui::WM_ACTIVATE, 0, top as isize);
    }
    deliver(top, gui::WM_NCACTIVATE, 1, 0);
    deliver(top, gui::WM_ACTIVATE, state, prev as isize);
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
    if hwnd != 0 && gui::focused_window() != hwnd {
        activate_for_focus(hwnd, WA_ACTIVE);
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
    match gui::window_client_rect(hwnd).or_else(|| gui::foreign_client_rect(hwnd)) {
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
    // WM_WINDOWPOSCHANGING, envoyé avant tout changement : la fenêtre peut
    // y corriger la position ou la taille demandées (contraintes de la VCL,
    // de WinForms…), comme sous Windows (#1251).
    let (x, y, cx, cy, flags) = {
        let mut pos = WindowPos {
            hwnd,
            insert_after: _insert_after,
            x,
            y,
            cx,
            cy,
            flags,
        };
        if flags & (SWP_NOMOVE | SWP_NOSIZE) != (SWP_NOMOVE | SWP_NOSIZE) {
            send_message_w(hwnd, WM_WINDOWPOSCHANGING, 0, &mut pos as *mut _ as isize);
            if gui::window_rect(hwnd).is_none() {
                crate::process::set_last_error(1400);
                return 0;
            }
        }
        // La fenêtre ne peut que retirer des changements, pas en ajouter.
        let keep = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOREDRAW;
        (pos.x, pos.y, pos.cx, pos.cy, flags | (pos.flags & keep))
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
        // Une fenêtre visible sans surface est montrée dès qu'elle en a une,
        // et retirée quand elle la perd (#1276).
        let was_empty = old.width() <= 0 || old.height() <= 0;
        let empty = width <= 0 || height <= 0;
        if resized
            && was_empty != empty
            && flags & (SWP_SHOWWINDOW | SWP_HIDEWINDOW) == 0
            && gui::is_window_visible(hwnd)
            && gui::window_style(hwnd).is_some_and(|s| s & WS_CHILD == 0)
        {
            with_backend(|backend| backend.show_window(hwnd, !empty));
        }
        if gui::window_parent(hwnd).unwrap_or(0) == 0 {
            super::chromium_win32::notify_win_event(
                super::chromium_win32::EVENT_OBJECT_LOCATIONCHANGE,
                hwnd,
            );
        }
    }

    // Ordre des enfants : HWND_TOP passe devant les voisins, HWND_BOTTOM
    // derrière, une HWND sœur place juste derrière elle. Le champ « Page »
    // de Sumatra, créé après son cadre, se remet ainsi devant lui (#1377).
    // Les fenêtres de premier niveau restent au gestionnaire de fenêtres.
    if flags & SWP_NOZORDER == 0 {
        restack_child(hwnd, _insert_after);
    }

    let mut visibility_changed = false;
    if flags & SWP_SHOWWINDOW != 0 {
        visibility_changed = !gui::set_visible(hwnd, true);
        let map = !empty_top_level(hwnd);
        with_backend(|backend| backend.show_window(hwnd, map));
    } else if flags & SWP_HIDEWINDOW != 0 {
        visibility_changed = gui::set_visible(hwnd, false);
        with_backend(|backend| backend.show_window(hwnd, false));
    }

    if flags & SWP_FRAMECHANGED != 0 {
        frame_changed(hwnd, rect, flags);
    }

    // WM_WINDOWPOSCHANGED, envoyé une fois la fenêtre placée : la VCL n'y
    // relit qu'ainsi ses Left/Top/Width/Height. Sans lui, chaque propriété
    // lue du DFM effaçait la précédente et le champ du port de WinSCP
    // restait en (0,0) (#1251). Comme sous Windows, DefWindowProc y envoie
    // WM_SIZE et WM_MOVE avant le retour de SetWindowPos : Qt relit sa
    // géométrie sur WM_SIZE, et iCUE redimensionné en deux temps (largeur
    // puis hauteur) retombait à 160x160 quand ils étaient postés (#2044).
    // Une fenêtre qui ne passe pas WM_WINDOWPOSCHANGED à DefWindowProc les
    // reçoit postés, comme avant.
    let size_msg =
        resized.then(|| pack_i16_pair(width, (height - gui::menu_bar_height(hwnd)).max(0)));
    let move_msg = moved.then(|| pack_i16_pair(left, top));
    let mut pending = None;
    if moved || resized || visibility_changed || flags & SWP_FRAMECHANGED != 0 {
        let mut done = flags;
        if !moved {
            done |= SWP_NOMOVE;
        }
        if !resized {
            done |= SWP_NOSIZE;
        }
        let mut pos = WindowPos {
            hwnd,
            insert_after: _insert_after,
            x: rect.left,
            y: rect.top,
            cx: rect.width(),
            cy: rect.height(),
            flags: done,
        };
        pending = Some(push_pending_geometry(hwnd, size_msg, move_msg));
        send_message_w(hwnd, WM_WINDOWPOSCHANGED, 0, &mut pos as *mut _ as isize);
    }
    // Ce que DefWindowProc n'a pas envoyé est posté.
    let (size_msg, move_msg) = match pending {
        Some(id) => pop_pending_geometry(id),
        None => (size_msg, move_msg),
    };
    if gui::window_rect(hwnd).is_some() {
        if let Some(l) = size_msg {
            gui::post_to_window(hwnd, Msg::new(hwnd, gui::WM_SIZE, size_wparam(hwnd), l));
        }
        if let Some(l) = move_msg {
            gui::post_to_window(hwnd, Msg::new(hwnd, gui::WM_MOVE, 0, l));
        }
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

/// WM_SIZE et WM_MOVE dus par un SetWindowPos en cours, en attendant que
/// DefWindowProc(WM_WINDOWPOSCHANGED) les envoie. Une pile : un WM_SIZE peut
/// placer d'autres fenêtres (#2044).
struct PendingGeometry {
    id: u64,
    hwnd: usize,
    size: Option<isize>,
    moved: Option<isize>,
}

thread_local! {
    static PENDING_GEOMETRY: std::cell::RefCell<Vec<PendingGeometry>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn push_pending_geometry(hwnd: usize, size: Option<isize>, moved: Option<isize>) -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    PENDING_GEOMETRY.with(|p| {
        p.borrow_mut().push(PendingGeometry {
            id,
            hwnd,
            size,
            moved,
        })
    });
    id
}

fn pop_pending_geometry(id: u64) -> (Option<isize>, Option<isize>) {
    PENDING_GEOMETRY.with(|p| {
        let mut p = p.borrow_mut();
        match p.iter().rposition(|g| g.id == id) {
            Some(i) => {
                let g = p.remove(i);
                (g.size, g.moved)
            }
            None => (None, None),
        }
    })
}

/// wParam de WM_SIZE selon l'état de la fenêtre.
fn size_wparam(hwnd: usize) -> usize {
    const WS_MINIMIZE: u32 = 0x2000_0000;
    const WS_MAXIMIZE: u32 = 0x0100_0000;
    match gui::window_style(hwnd) {
        Some(s) if s & WS_MINIMIZE != 0 => 1, // SIZE_MINIMIZED
        Some(s) if s & WS_MAXIMIZE != 0 => 2, // SIZE_MAXIMIZED
        _ => 0,                               // SIZE_RESTORED
    }
}

/// DefWindowProc(WM_WINDOWPOSCHANGED) : envoie WM_MOVE puis WM_SIZE dus par
/// le SetWindowPos en cours sur cette fenêtre, comme Windows (#2044). Un
/// WM_WINDOWPOSCHANGED venu du gestionnaire de fenêtres n'a rien en attente :
/// ses WM_SIZE et WM_MOVE sont déjà en file.
pub(crate) fn window_pos_changed_default(hwnd: usize) {
    let due = PENDING_GEOMETRY.with(|p| {
        let mut p = p.borrow_mut();
        p.iter_mut()
            .rev()
            .find(|g| g.hwnd == hwnd)
            .map(|g| (g.size.take(), g.moved.take()))
    });
    let Some((size, moved)) = due else {
        return;
    };
    unsafe {
        if let Some(l) = moved {
            send_message_w(hwnd, gui::WM_MOVE, 0, l);
        }
        if let Some(l) = size {
            if gui::window_rect(hwnd).is_some() {
                send_message_w(hwnd, gui::WM_SIZE, size_wparam(hwnd), l);
            }
        }
    }
}

/// Applique l'ordre demandé à SetWindowPos pour une fenêtre enfant.
fn restack_child(hwnd: usize, insert_after: usize) {
    let parent = gui::window_parent(hwnd).unwrap_or(0);
    if parent == 0 || gui::window_style(hwnd).is_none_or(|s| s & WS_CHILD == 0) {
        return;
    }
    let backdrop = super::controls::is_backdrop(hwnd);
    match insert_after {
        0 if !backdrop => with_backend(|b| b.raise_window(hwnd)), // HWND_TOP
        1 => with_backend(|b| b.lower_window(hwnd)),              // HWND_BOTTOM
        s if s != hwnd && !backdrop && gui::window_parent(s) == Some(parent) => {
            with_backend(|b| b.stack_below(hwnd, s))
        }
        _ => {}
    }
}

const WM_WINDOWPOSCHANGING: u32 = 0x0046;
const WM_WINDOWPOSCHANGED: u32 = 0x0047;

/// Fenêtres dont le cadre du gestionnaire a été retiré par `frame_changed`.
static OWN_FRAME: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

#[repr(C)]
struct WindowPos {
    hwnd: usize,
    insert_after: usize,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
}

#[repr(C)]
struct NcCalcSizeParams {
    rects: [Rect; 3],
    pos: *mut WindowPos,
}

/// SetWindowPos(SWP_FRAMECHANGED) : WM_NCCALCSIZE est envoyé, comme sous
/// Windows. Une fenêtre qui y répond elle-même, sans DefWindowProc, en
/// gardant tout le rectangle pour sa zone cliente dessine sa propre barre de
/// titre (WindowChrome de WPF, ScreenToGif) : le cadre du gestionnaire de
/// fenêtres est alors retiré, sinon deux barres de titre se superposent
/// (#1131). Il revient si la fenêtre rend plus tard la main à DefWindowProc.
unsafe fn frame_changed(hwnd: usize, rect: Rect, flags: u32) {
    // Programme 32 bits : la structure serait à traduire, on n'envoie rien.
    if crate::wow32::pointer_size() != 8 || gui::window_parent(hwnd).unwrap_or(0) != 0 {
        return;
    }
    let mut pos = WindowPos {
        hwnd,
        insert_after: 0,
        x: rect.left,
        y: rect.top,
        cx: rect.width(),
        cy: rect.height(),
        flags,
    };
    let mut params = NcCalcSizeParams {
        rects: [rect, rect, rect],
        pos: &mut pos,
    };
    gui::reset_default_nccalcsize();
    send_message_w(hwnd, 0x0083, 1, &mut params as *mut _ as isize);
    let own = !gui::default_nccalcsize_seen() && params.rects[0] == rect;
    if !own {
        pending_frame().retain(|(h, _, _)| *h != hwnd);
    }
    let mut list = OWN_FRAME.lock().unwrap_or_else(|e| e.into_inner());
    let had = list.contains(&hwnd);
    if own && !had {
        drop(list);
        // Le cadre ne part que si la fenêtre a déjà une barre de titre à
        // elle (#1211). Sinon il reste, et sera retiré dès qu'elle en aura
        // une (voir `recheck_own_frame`).
        if has_own_caption(hwnd) || drawn_whole(hwnd) {
            take_own_frame(hwnd);
        } else {
            let mut pending = pending_frame();
            if !pending.iter().any(|(h, _, _)| *h == hwnd) {
                let now = std::time::Instant::now();
                pending.push((hwnd, now, now));
            }
        }
    } else if !own && had {
        list.retain(|&h| h != hwnd);
        drop(list);
        let style = gui::window_style(hwnd).unwrap_or(0);
        let level = if drawn_whole(hwnd) {
            0
        } else {
            popup_decorations(style).unwrap_or(2)
        };
        with_backend(|b| b.set_decorations(hwnd, level));
    }
}

/// Fenêtres qui ont demandé à dessiner leur propre cadre mais n'ont pas
/// encore de barre de titre (Electron dont la page ne s'est pas chargée) :
/// le cadre du gestionnaire reste, sinon rien ne permettrait de les fermer,
/// réduire ou déplacer (#1211). Avec l'instant du dernier essai et celui de
/// la mise en attente.
static PENDING_FRAME: std::sync::Mutex<Vec<(usize, std::time::Instant, std::time::Instant)>> =
    std::sync::Mutex::new(Vec::new());

fn pending_frame(
) -> std::sync::MutexGuard<'static, Vec<(usize, std::time::Instant, std::time::Instant)>> {
    PENDING_FRAME.lock().unwrap_or_else(|e| e.into_inner())
}

fn take_own_frame(hwnd: usize) {
    pending_frame().retain(|(h, _, _)| *h != hwnd);
    let mut list = OWN_FRAME.lock().unwrap_or_else(|e| e.into_inner());
    if !list.contains(&hwnd) {
        list.push(hwnd);
        drop(list);
        with_backend(|b| b.set_decorations(hwnd, 0));
    }
}

/// La fenêtre répond-elle à WM_NCHITTEST par une barre de titre ou un
/// bouton de légende quelque part dans sa bande du haut ? C'est le signe
/// qu'elle se déplace et se ferme sans le cadre du gestionnaire.
unsafe fn has_own_caption(hwnd: usize) -> bool {
    let Some(r) = gui::window_rect(hwnd) else {
        return false;
    };
    let (w, h) = (r.width(), r.height());
    if w <= 0 || h <= 0 {
        return false;
    }
    for dy in [4, 12, 24] {
        if dy >= h {
            continue;
        }
        for x in [
            r.left + w / 2,
            r.left + 60,
            r.right - 20,
            r.right - 60,
            r.right - 110,
        ] {
            if x <= r.left || x >= r.right {
                continue;
            }
            let lp = pack_i16_pair(x, r.top + dy);
            // HTCAPTION, HTSYSMENU, HTMINBUTTON, HTMAXBUTTON, HTCLOSE.
            if matches!(send_message_w(hwnd, 0x0084, 0, lp), 2 | 3 | 8 | 9 | 20) {
                return true;
            }
        }
    }
    false
}

/// Délai entre deux essais sur une fenêtre en attente : deux fois par
/// seconde pendant la première minute, puis toutes les trois secondes.
fn frame_retry_delay(waiting: std::time::Duration) -> std::time::Duration {
    if waiting < std::time::Duration::from_secs(60) {
        std::time::Duration::from_millis(500)
    } else {
        std::time::Duration::from_secs(3)
    }
}

/// Essai de nouveau sur les fenêtres en attente du thread qui distribue un
/// message, quel que soit ce message et la fenêtre qui le reçoit : la page
/// d'Electron peut se charger bien après la création de la fenêtre (#1211),
/// et attendre que la souris passe sur la fenêtre laissait deux barres de
/// titre à l'écran tant qu'on n'y touchait pas (#1444). La pompe de
/// Chromium distribue sans cesse des messages à sa fenêtre cachée : c'est
/// l'occasion de réessayer.
unsafe fn recheck_own_frame() {
    thread_local! {
        static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if BUSY.with(|b| b.replace(true)) {
        return;
    }
    let due: Vec<usize> = {
        let mut pending = pending_frame();
        if pending.is_empty() {
            BUSY.with(|b| b.set(false));
            return;
        }
        let tid = crate::dlls::kernel32::get_current_thread_id();
        let now = std::time::Instant::now();
        pending
            .iter_mut()
            .filter(|(h, last, since)| {
                now.duration_since(*last) >= frame_retry_delay(now.duration_since(*since))
                    && gui::window_thread_id(*h) == Some(tid)
            })
            .map(|e| {
                e.1 = now;
                e.0
            })
            .collect()
    };
    for h in due {
        if has_own_caption(h) {
            take_own_frame(h);
        }
    }
    BUSY.with(|b| b.set(false));
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
/// Une fenêtre d'un autre processus du compte est valable aussi : le
/// processus GPU de Chromium peint ainsi les surgissants (#1345).
pub extern "win64" fn get_dc(hwnd: usize) -> usize {
    if hwnd != 0 && gui::wnd_proc_of(hwnd).is_none() && gui::foreign_window(hwnd).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    if hwnd == 0 {
        return gui::create_screen_dc();
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
/// GetForegroundWindow : la fenêtre active du programme quand il a le
/// focus (une boîte possédée cliquée, #1804), sinon la plus haute.
pub extern "win64" fn get_foreground_window() -> usize {
    let active = gui::active_top();
    if active != 0 && gui::is_window(active) {
        return active;
    }
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
        68 | 69 => 4,                         // SM_CXDRAG / SM_CYDRAG (#1138)
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
    if let Some(c) = crate::dlls::user32_releve::sys_color_override(index) {
        return Some(c);
    }
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

/// Inscrit (ou met à jour) une fenêtre de ce processus dans l'annuaire
/// partagé avec ses styles (#1228, #1357). Les fenêtres sans fenêtre X11
/// n'y figurent pas.
fn publish_shared(hwnd: usize) {
    if gui::foreign_window(hwnd).is_some() {
        return;
    }
    if let Some(xid) = gui::native_window(hwnd) {
        super::gui_shared::publish(
            hwnd,
            gui::window_thread_id(hwnd).unwrap_or_else(|| super::kernel32::get_current_thread_id()),
            xid,
            gui::window_style(hwnd).unwrap_or(0),
            gui::window_ex_style(hwnd).unwrap_or(0),
        );
    }
}

unsafe fn get_window_long_impl(hwnd: usize, index: i32) -> isize {
    // Fenêtre d'un autre processus : ses styles publiés. Le processus GPU de
    // Chromium y cherche WS_EX_LAYERED pour choisir son chemin de peinture
    // (#1357).
    if matches!(index, GWL_STYLE | GWL_EXSTYLE) && gui::window_style(hwnd).is_none() {
        if let Some(f) = gui::foreign_window(hwnd) {
            return if index == GWL_STYLE {
                f.style as isize
            } else {
                f.ex_style as isize
            };
        }
    }
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
            if gui::set_window_style(hwnd, value as u32).is_some_and(|o| o != value as u32) {
                publish_shared(hwnd);
            }
        }
        GWL_EXSTYLE => {
            if gui::set_window_ex_style(hwnd, value as u32).is_some_and(|o| o != value as u32) {
                publish_shared(hwnd);
            }
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
                let captioned = gui::window_style(hwnd)
                    .is_some_and(|s| s & 0x00C0_0000 == 0x00C0_0000)
                    && !captioned_tool_window(
                        gui::window_style(hwnd).unwrap_or(0),
                        gui::window_ex_style(hwnd).unwrap_or(0),
                    );
                with_backend(|b| {
                    b.set_owner(hwnd, owner);
                    if captioned {
                        b.set_dialog_type(hwnd);
                    }
                });
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

/// Valeur de classe d'une fenêtre (GetClassLong, GetClassLongPtr,
/// GetClassWord) : atome, style, procédure, module, fond, curseur. Index
/// inconnu ou octets supplémentaires de classe (non conservés) : 0. Tout
/// renvoyait 0 et GetClassWord manquait : Steam s'arrêtait en 0xC0000139
/// juste après le clic sur sa boîte de service (#1858).
pub fn class_value(hwnd: usize, index: i32) -> Option<usize> {
    const GCLP_HBRBACKGROUND: i32 = -10;
    const GCLP_HCURSOR: i32 = -12;
    const GCLP_HMODULE: i32 = -16;
    const GCLP_WNDPROC: i32 = -24;
    const GCL_STYLE: i32 = -26;
    const GCW_ATOM: i32 = -32;
    let class = gui::window_class(hwnd)?;
    let info = gui::lookup_class(&class);
    Some(match index {
        GCW_ATOM => class_atom(&class) as usize,
        GCL_STYLE => gui::class_style(&class) as usize,
        GCLP_WNDPROC => info.map_or(0, |i| i.wnd_proc),
        GCLP_HMODULE => info.map_or(0, |i| i.instance),
        GCLP_HBRBACKGROUND => info.map_or(0, |i| i.background),
        GCLP_HCURSOR => gui::class_cursor_of(hwnd),
        _ => 0,
    })
}

fn class_value_or_error(hwnd: usize, index: i32) -> usize {
    class_value(hwnd, index).unwrap_or_else(|| {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        0
    })
}

pub unsafe extern "win64" fn get_class_long_a(hwnd: usize, index: i32) -> u32 {
    class_value_or_error(hwnd, index) as u32
}

pub unsafe extern "win64" fn get_class_long_w(hwnd: usize, index: i32) -> u32 {
    class_value_or_error(hwnd, index) as u32
}

pub unsafe extern "win64" fn get_class_long_ptr_a(hwnd: usize, index: i32) -> usize {
    class_value_or_error(hwnd, index)
}

pub unsafe extern "win64" fn get_class_long_ptr_w(hwnd: usize, index: i32) -> usize {
    class_value_or_error(hwnd, index)
}

/// GetClassWord : seul GCW_ATOM (-32) a un sens hors octets supplémentaires.
pub unsafe extern "win64" fn get_class_word(hwnd: usize, index: i32) -> u16 {
    class_value_or_error(hwnd, index) as u16
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
    let v = gui::is_window_visible(hwnd) as i32;
    if window_diag() {
        eprintln!("winlux[fenetre] IsWindowVisible({hwnd:#x}) -> {v}");
    }
    v
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
    let v = (state == gui::WINDOW_MINIMIZED) as i32;
    if window_diag() {
        eprintln!("winlux[fenetre] IsIconic({hwnd:#x}) -> {v}");
    }
    v
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
    if window_diag() {
        eprintln!("winlux[fenetre] parent {hwnd:#x} : {old:#x} -> {new_parent:#x}");
    }
    if new_parent != 0
        && new_parent != HWND_MESSAGE
        && is_message_only(hwnd)
        && !is_message_only(new_parent)
    {
        // Fenêtre créée sous une fenêtre « message seulement », donc sans
        // fenêtre X (#1586), puis rattachée à une vraie fenêtre : WinForms
        // crée les ToolStrip de Paint.NET (menus, barre d'outils, barre
        // d'état) sous sa WindowsFormsParkingWindow, puis les déplace dans
        // la fenêtre principale (#1677). Elle devient une fenêtre ordinaire.
        leave_message_only(hwnd, new_parent);
    } else if new_parent != 0 && new_parent != old {
        // Sans cela la fenêtre X restait dans l'ancien parent (bandes d'une
        // ReBar, panneaux déplacés d'une fenêtre à l'autre).
        let rect = gui::window_rect(hwnd).unwrap_or_default();
        with_backend(|b| b.reparent_window(hwnd, new_parent, rect));
    }
    old
}

/// Donne une fenêtre X à `hwnd` et à ses descendants, sortis du monde
/// « message seulement » par SetParent, puis demande leur peinture.
fn leave_message_only(hwnd: usize, parent: usize) {
    lock(message_only_windows()).remove(&hwnd);
    let rect = gui::window_rect(hwnd).unwrap_or_default();
    let shown = gui::is_window_visible(hwnd);
    with_backend(|b| {
        b.create_child_window(hwnd, parent, rect);
        if shown {
            b.show_window(hwnd, true);
        }
    });
    for child in gui::child_windows(hwnd) {
        leave_message_only(child, hwnd);
    }
    if shown {
        gui::request_paint(hwnd);
    }
}

// =====================================================================
// FindWindowA/W, EnumWindows, GetWindow
// =====================================================================

/// FindWindowA/W : `class` peut être un atome (MAKEINTATOM, 16 bits hauts
/// nuls), qu'Acrobat passe pour la classe des boîtes de dialogue (#32770) ;
/// il ne se lit pas comme une chaîne (#1754). Comme FindWindowEx(NULL,
/// NULL, …) : fenêtres de premier niveau seulement (#2106).
pub unsafe extern "win64" fn find_window_a(class: *const u8, title: *const u8) -> usize {
    find_window_ex_a(0, 0, class, title)
}

pub unsafe extern "win64" fn find_window_w(class: *const u16, title: *const u16) -> usize {
    find_window_ex_w(0, 0, class, title)
}

/// Recherche commune à FindWindowExA/W (#2106). Parent NULL ou bureau :
/// fenêtres de premier niveau, sans les fenêtres message-only ;
/// HWND_MESSAGE : les seules fenêtres message-only ; sinon les enfants
/// directs. La recherche reprend après `after`, sinon une boucle
/// `while ((h = FindWindowEx(0, h, …)))` tourne sans fin.
fn find_window_among(
    parent: usize,
    after: usize,
    class: Option<&str>,
    title: Option<&str>,
) -> usize {
    let found = match parent {
        0 | 1 => gui::find_window_ex(0, after, class, title, |h| !is_message_only(h)),
        HWND_MESSAGE => gui::find_window_ex(0, after, class, title, is_message_only),
        p => gui::find_window_ex(p, after, class, title, |_| true),
    };
    found.unwrap_or(0)
}

pub unsafe extern "win64" fn find_window_ex_a(
    parent: usize,
    after: usize,
    class: *const u8,
    title: *const u8,
) -> usize {
    let cls = if class.is_null() {
        None
    } else {
        Some(read_class_name_a(class))
    };
    let ttl = (!title.is_null()).then(|| crate::dlls::cstr(title));
    find_window_among(parent, after, cls.as_deref(), ttl.as_deref())
}

pub unsafe extern "win64" fn find_window_ex_w(
    parent: usize,
    after: usize,
    class: *const u16,
    title: *const u16,
) -> usize {
    let cls = if class.is_null() {
        None
    } else {
        Some(read_class_name_w(class))
    };
    let ttl = (!title.is_null()).then(|| crate::dlls::wstr(title));
    find_window_among(parent, after, cls.as_deref(), ttl.as_deref())
}

pub unsafe extern "win64" fn enum_windows(callback: usize, lparam: isize) -> i32 {
    if callback == 0 {
        return 0;
    }
    let windows = gui::enum_top_level_windows();
    if window_diag() {
        // #1886 : ce que voit le suivi d'occultation de Chromium (son fil
        // relit la liste, la visibilité et le rectangle de chaque fenêtre).
        let tid = libc::syscall(libc::SYS_gettid);
        let list: Vec<String> = windows
            .iter()
            .map(|&h| {
                let r = gui::screen_rect(h).map_or("-".into(), |r| {
                    format!("{},{}-{},{}", r.left, r.top, r.right, r.bottom)
                });
                format!(
                    "{h:#x} {} v={} st={:#x} ex={:#x} {r}",
                    gui::window_class(h).unwrap_or_default(),
                    gui::is_window_visible(h) as u8,
                    gui::window_style(h).unwrap_or(0),
                    gui::window_ex_style(h).unwrap_or(0),
                )
            })
            .collect();
        eprintln!(
            "winlux[fenetre] EnumWindows fil {tid} : {}",
            list.join(" | ")
        );
    }
    type EnumCallback = unsafe extern "win64" fn(usize, isize) -> i32;
    let cb: EnumCallback = std::mem::transmute(callback);
    for hwnd in windows {
        if cb(hwnd, lparam) == 0 {
            break;
        }
    }
    1
}

/// EnumChildWindows : chaque descendant de `parent` (enfants, puis leurs
/// propres enfants), jusqu'à ce que le rappel renvoie FALSE. Parent NULL :
/// comme EnumWindows. La liste est relevée avant les appels, une fenêtre
/// créée pendant l'énumération n'y figure pas. Le bouchon ne rappelait
/// jamais : Sumatra PDF n'invalidait pas sa zone de document en passant à
/// l'onglet d'accueil, qui restait affiché avec l'ancienne page.
pub unsafe extern "win64" fn enum_child_windows(
    parent: usize,
    callback: usize,
    lparam: isize,
) -> i32 {
    if parent == 0 {
        return enum_windows(callback, lparam);
    }
    if callback == 0 {
        return 0;
    }
    fn descendants(parent: usize, out: &mut Vec<usize>) {
        for child in gui::child_windows(parent) {
            out.push(child);
            descendants(child, out);
        }
    }
    let mut all = Vec::new();
    descendants(parent, &mut all);
    type EnumCallback = unsafe extern "win64" fn(usize, isize) -> i32;
    let cb: EnumCallback = std::mem::transmute(callback);
    for hwnd in all {
        if gui::is_window(hwnd) && cb(hwnd, lparam) == 0 {
            break;
        }
    }
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

/// WM_CAPTURECHANGED, envoyé à la fenêtre qui perd la capture (#1254) :
/// sans lui, WPF croit garder la capture sur l'élément cliqué et route les
/// clics des autres fenêtres vers lui.
const WM_CAPTURECHANGED: u32 = 0x0215;

fn capture_lost(old: usize, new: usize) {
    if old != 0 && old != new && gui::is_window(old) {
        unsafe { send_message_w(old, WM_CAPTURECHANGED, 0, new as isize) };
    }
}

pub extern "win64" fn set_capture(hwnd: usize) -> usize {
    let old = gui::set_capture(hwnd);
    capture_lost(old, hwnd);
    old
}

/// ReleaseCapture réussit aussi quand aucune fenêtre n'a la capture : WPF
/// lève une Win32Exception sur un FALSE (#268).
pub extern "win64" fn release_capture() -> i32 {
    let old = gui::get_capture();
    gui::release_capture();
    capture_lost(old, 0);
    1
}

/// Détection d'un glisser (DragDetect, et la liste ou l'arborescence avant
/// LVN_BEGINDRAG / TVN_BEGINDRAG, #1138) : vrai dès que le pointeur sort,
/// bouton gauche tenu, du rectangle SM_CXDRAG × SM_CYDRAG centré sur `x`,
/// `y` (coordonnées écran) ; faux au relâchement, à un autre clic ou à
/// Échap. Les déplacements sont consommés, les clics restent dans la file.
pub unsafe fn drag_detect_at(hwnd: usize, x: i32, y: i32) -> bool {
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_MOUSELAST: u32 = 0x020E;
    let cx = get_system_metrics(68).max(1); // SM_CXDRAG
    let cy = get_system_metrics(69).max(1); // SM_CYDRAG
    let previous = gui::set_capture(hwnd);
    let started = std::time::Instant::now();
    let mut dragged = false;
    loop {
        let mut m: Msg = std::mem::zeroed();
        if peek_message_w(&mut m, 0, WM_MOUSEMOVE, WM_MOUSELAST, 0) != 0 {
            if m.message != WM_MOUSEMOVE {
                break;
            }
            peek_message_w(&mut m, 0, WM_MOUSEMOVE, WM_MOUSEMOVE, 1);
            let mut p = Point {
                x: m.lparam as u16 as i16 as i32,
                y: (m.lparam >> 16) as u16 as i16 as i32,
            };
            client_to_screen(m.hwnd, &mut p);
            if (p.x - x).abs() > cx / 2 || (p.y - y).abs() > cy / 2 {
                dragged = true;
                break;
            }
            continue;
        }
        if gui::get_async_key_state(0x1B) < 0 {
            break;
        }
        // Bouton déjà relâché sans message en attente (clic envoyé par
        // SendMessage, automate) : pas de glisser.
        let held = gui::get_key_state(1) < 0 || gui::get_async_key_state(1) < 0;
        if !held || !gui::backend_is_interactive() {
            break;
        }
        // Garde-fou : un bouton « tenu » que le serveur X ne relâche jamais.
        if started.elapsed() > std::time::Duration::from_secs(30) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    gui::release_capture();
    if previous != 0 && previous != hwnd {
        gui::set_capture(previous);
    }
    dragged
}

/// DragDetect(HWND, POINT) : le POINT (8 octets) arrive dans un registre.
pub unsafe extern "win64" fn drag_detect(hwnd: usize, pt: u64) -> i32 {
    drag_detect_at(hwnd, pt as u32 as i32, (pt >> 32) as u32 as i32) as i32
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

/// TrackMouseEvent (#1254) : TME_LEAVE fait poster WM_MOUSELEAVE quand le
/// pointeur quitte la fenêtre ; TME_HOVER fait poster WM_MOUSEHOVER quand il
/// reste immobile dwHoverTime millisecondes (#1591). TME_NONCLIENT est
/// accepté sans effet. La structure fait 24 octets en 64 bits (dwHoverTime
/// à +16) ; wow32 la convertit depuis les 16 octets du x86.
pub unsafe extern "win64" fn track_mouse_event(tme: *mut u8) -> i32 {
    const TME_HOVER: u32 = 0x1;
    const TME_LEAVE: u32 = 0x2;
    const TME_NONCLIENT: u32 = 0x10;
    const TME_QUERY: u32 = 0x4000_0000;
    const TME_CANCEL: u32 = 0x8000_0000;
    if tme.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let size = std::ptr::read_unaligned(tme as *const u32);
    let flags = std::ptr::read_unaligned(tme.add(4) as *const u32);
    let hwnd_at = |p: *mut u8| -> usize {
        if size == 16 {
            std::ptr::read_unaligned(p.add(8) as *const u32) as usize
        } else {
            std::ptr::read_unaligned(p.add(8) as *const usize)
        }
    };
    if size != 16 && size != 24 {
        crate::process::set_last_error(87);
        return 0;
    }
    let hwnd = hwnd_at(tme);
    let hover_at = if size == 16 { 12 } else { 16 };
    if flags & TME_QUERY != 0 {
        let mut out = if gui::mouse_leave_tracked(hwnd) {
            TME_LEAVE
        } else {
            0
        };
        if let Some(ms) = gui::mouse_hover_tracked(hwnd) {
            out |= TME_HOVER;
            std::ptr::write_unaligned(tme.add(hover_at) as *mut u32, ms);
        }
        std::ptr::write_unaligned(tme.add(4) as *mut u32, out);
        return 1;
    }
    if flags & TME_NONCLIENT != 0 {
        return 1;
    }
    if flags & TME_HOVER != 0 {
        if flags & TME_CANCEL != 0 {
            gui::cancel_mouse_hover(hwnd);
        } else if gui::is_window(hwnd) {
            let ms = std::ptr::read_unaligned(tme.add(hover_at) as *const u32);
            gui::track_mouse_hover(hwnd, ms);
        } else {
            crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
            return 0;
        }
    }
    if flags & TME_LEAVE == 0 {
        return 1;
    }
    if flags & TME_CANCEL != 0 {
        gui::cancel_mouse_leave(hwnd);
        return 1;
    }
    if !gui::is_window(hwnd) {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    gui::track_mouse_leave(hwnd, gui::pointer_over(hwnd));
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
    crate::dlls::mouse::double_click_time()
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

/// DrawTextEx : même rendu que DrawText ; les marges de DRAWTEXTPARAMS
/// ne sont pas appliquées (le bouchon précédent ignorait tout le format).
pub unsafe extern "win64" fn draw_text_ex_a(
    hdc: usize,
    text: *const u8,
    count: i32,
    rect: *mut Rect,
    format: u32,
    _params: usize,
) -> i32 {
    draw_text_a(hdc, text, count, rect, format)
}

pub unsafe extern "win64" fn draw_text_ex_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    rect: *mut Rect,
    format: u32,
    _params: usize,
) -> i32 {
    draw_text_w(hdc, text, count, rect, format)
}

/// Retire les préfixes de mnémonique de DrawText : « &F » affiche « F »
/// souligné, « && » affiche « & ». Rend le texte affiché et la position (en
/// caractères) de la lettre à souligner.
pub(crate) fn split_prefix(text: &str) -> (String, Option<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut underline = None;
    let mut count = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            match chars.next() {
                Some('&') => {
                    out.push('&');
                    count += 1;
                }
                Some(c) => {
                    if underline.is_none() {
                        underline = Some(count);
                    }
                    out.push(c);
                    count += 1;
                }
                None => {}
            }
            continue;
        }
        out.push(ch);
        count += 1;
    }
    (out, underline)
}

/// Découpe le texte affiché de DrawText en lignes : coupure sur CR, LF et
/// CR LF hors DT_SINGLELINE, puis, avec DT_WORDBREAK, retour à la ligne aux
/// espaces quand une ligne dépasse `width` (boîte « Information » de WinSCP,
/// #1341). Un mot plus large que `width` reste seul sur sa ligne. Rend
/// chaque ligne avec la position de la lettre soulignée qu'elle contient.
pub(crate) fn draw_text_lines(
    shown: &str,
    underline: Option<usize>,
    single: bool,
    width: Option<i32>,
    measure: &dyn Fn(&str) -> i32,
) -> Vec<(String, Option<usize>)> {
    let chars: Vec<char> = shown.chars().collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if single {
        ranges.push((0, chars.len()));
    } else {
        let mut start = 0;
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '\r' => {
                    ranges.push((start, i));
                    i += if chars.get(i + 1) == Some(&'\n') {
                        2
                    } else {
                        1
                    };
                    start = i;
                }
                '\n' => {
                    ranges.push((start, i));
                    i += 1;
                    start = i;
                }
                _ => i += 1,
            }
        }
        ranges.push((start, chars.len()));
    }
    let text_of = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let mut wrapped: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ranges {
        let Some(w) = width.filter(|w| *w > 0) else {
            wrapped.push((s, e));
            continue;
        };
        if measure(&text_of(s, e)) <= w {
            wrapped.push((s, e));
            continue;
        }
        let mut start = s;
        while start < e {
            // Fin de ligne la plus lointaine, à une frontière de mot, qui tient.
            let mut best = None;
            let mut first_word = None;
            let mut j = start;
            while j <= e {
                if j == e || chars[j] == ' ' {
                    let mut end = j;
                    while end > start && chars[end - 1] == ' ' {
                        end -= 1;
                    }
                    if end > start {
                        if first_word.is_none() {
                            first_word = Some(j);
                        }
                        if measure(&text_of(start, end)) <= w {
                            best = Some(j);
                        } else {
                            break;
                        }
                    }
                }
                j += 1;
            }
            let cut = best.or(first_word).unwrap_or(e);
            let mut end = cut;
            while end > start && chars[end - 1] == ' ' {
                end -= 1;
            }
            wrapped.push((start, end));
            start = cut;
            while start < e && chars[start] == ' ' {
                start += 1;
            }
        }
    }
    wrapped
        .into_iter()
        .map(|(a, b)| {
            let u = underline.filter(|k| *k >= a && *k < b).map(|k| k - a);
            (text_of(a, b), u)
        })
        .collect()
}

fn draw_text_impl(hdc: usize, text: &str, rect: &mut Rect, format: u32) -> i32 {
    const DT_CALCRECT: u32 = 0x0400;
    const DT_CENTER: u32 = 0x0001;
    const DT_RIGHT: u32 = 0x0002;
    const DT_VCENTER: u32 = 0x0004;
    const DT_BOTTOM: u32 = 0x0008;
    const DT_WORDBREAK: u32 = 0x0010;
    const DT_SINGLELINE: u32 = 0x0020;
    const DT_NOPREFIX: u32 = 0x0800;
    const DT_HIDEPREFIX: u32 = 0x0010_0000;
    const DT_PREFIXONLY: u32 = 0x0020_0000;

    let font_handle = gui::dc_font(hdc);
    let font = gui::font_spec(font_handle).unwrap_or_default();
    // Sans DT_NOPREFIX, « & » marque la lettre de raccourci (libellés de la
    // VCL : « &Hostname: » s'affichait tel quel, #1248).
    let (shown, underline) = if format & DT_NOPREFIX != 0 {
        (text.to_string(), None)
    } else {
        split_prefix(text)
    };
    let single = format & DT_SINGLELINE != 0;
    let width = (format & DT_WORDBREAK != 0 && !single).then(|| rect.width());
    let measure = |s: &str| gui::measure_text(&font, s).0;
    let lines = draw_text_lines(&shown, underline, single, width, &measure);
    // Hauteur d'une ligne : celle du texte entier sur une seule ligne, sinon
    // celle d'une ligne type (une ligne vide compte aussi).
    let lh = if lines.len() == 1 {
        gui::measure_text(&font, &shown).1
    } else {
        gui::measure_text(&font, "Ag").1
    };
    let widths: Vec<i32> = lines.iter().map(|(l, _)| measure(l)).collect();
    let tw = widths.iter().copied().max().unwrap_or(0);
    let th = lh * lines.len() as i32;

    if format & DT_CALCRECT != 0 {
        rect.right = rect.left + tw;
        rect.bottom = rect.top + th;
        return th;
    }

    let mut y = rect.top;
    if single {
        if format & DT_VCENTER != 0 {
            y = rect.top + (rect.height() - th) / 2;
        } else if format & DT_BOTTOM != 0 {
            y = rect.bottom - th;
        }
    }

    let color = gui::dc_text_color(hdc);
    let bk_mode = gui::dc_bk_mode(hdc);
    let bk = if bk_mode == 2 {
        Some(gui::dc_bk_color(hdc))
    } else {
        None
    };

    for ((line, underline), lw) in lines.into_iter().zip(widths) {
        let x = if format & DT_CENTER != 0 {
            rect.left + (rect.width() - lw) / 2
        } else if format & DT_RIGHT != 0 {
            rect.right - lw
        } else {
            rect.left
        };
        if let Some(k) = underline.filter(|_| format & DT_HIDEPREFIX == 0) {
            let before: String = line.chars().take(k).collect();
            let upto: String = line.chars().take(k + 1).collect();
            let x1 = x + measure(&before);
            let x2 = x + measure(&upto);
            let base = y + lh - 2;
            gui::record(
                hdc,
                gui::DrawCmd::Line {
                    x1,
                    y1: base,
                    x2: x2 - 1,
                    y2: base,
                    color,
                    width: 1,
                },
            );
        }
        if format & DT_PREFIXONLY == 0 && !line.is_empty() {
            gui::record(
                hdc,
                gui::DrawCmd::Text {
                    x,
                    y,
                    text: line,
                    color,
                    bk,
                    font: font.clone(),
                },
            );
        }
        y += lh;
    }

    th
}

pub extern "win64" fn get_window_dc(hwnd: usize) -> usize {
    if hwnd == 0 {
        return gui::create_screen_dc();
    }
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
        // SPI_GETMOUSEHOVERWIDTH / HEIGHT (4 pixels) et SPI_GETMOUSEHOVERTIME
        // (400 ms), valeurs par défaut documentées : WinForms en fait
        // l'intervalle du minuteur de survol de ses barres d'outils (#1574).
        0x0062 | 0x0064 | 0x0066 if !data.is_null() => {
            *data.cast::<u32>() = if action == 0x0066 { 400 } else { 4 };
            return 1;
        }
        // SPI_GETCARETWIDTH : DWORD en pixels.
        0x2006 if !data.is_null() => {
            *data.cast::<u32>() = u32::from(a11y.caret_width);
            return 1;
        }
        _ => {}
    }
    // Délais et vitesses du clavier et des menus : valeurs par défaut
    // documentées de SystemParametersInfo. Sans elles, le succès générique
    // laissait le tampon à 0, et un minuteur WinForms réglé dessus refuse 0
    // (#1803) : SPI_GETKEYBOARDSPEED (31), SPI_GETKEYBOARDDELAY (1, soit
    // 500 ms), SPI_GETMENUSHOWDELAY (400 ms), SPI_GETFOREGROUNDLOCKTIMEOUT
    // (200 000 ms), SPI_GETFOCUSBORDERWIDTH / HEIGHT (1 pixel),
    // SPI_GETMESSAGEDURATION (5 s) et SPI_GETCARETTIMEOUT (5 000 ms).
    let default = match action {
        0x000A => Some(31),
        0x0016 => Some(1),
        0x006A => Some(400),
        0x2000 => Some(200_000),
        0x200E | 0x2010 => Some(1),
        0x2016 => Some(5),
        0x2022 => Some(5000),
        _ => None,
    };
    if let Some(v) = default {
        if data.is_null() {
            return 0;
        }
        *data.cast::<u32>() = v;
        return 1;
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
            // Fenêtre d'un autre processus du compte (#1228).
            if let Some(f) = gui::foreign_window(hwnd) {
                if !pid.is_null() {
                    *pid = f.pid;
                }
                return f.tid;
            }
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

/// GetActiveWindow : fenêtre de premier niveau active du fil appelant,
/// jamais un contrôle (#1804).
pub extern "win64" fn get_active_window() -> usize {
    let active = gui::active_top();
    if active != 0 && gui::window_owner(active).as_ref() == Some(&std::thread::current().id()) {
        return active;
    }
    gui::top_level_of(gui::focused_window())
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
const WH_CBT: i32 = 5;
const HCBT_CREATEWND: i32 = 3;
const PM_REMOVE: usize = 1;

/// Crochet installé par SetWindowsHookEx. Sont appelés WH_GETMESSAGE (Qt 5
/// y relance la livraison de ses événements différés ; sans lui, une
/// fenêtre Qt s'affiche mais ne réagit plus, #938) et WH_CBT pour
/// HCBT_CREATEWND (MFC y attache ses fenêtres, #1654).
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
    if matches!(id, WH_GETMESSAGE | WH_CBT) && proc_ != 0 {
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

/// CBT_CREATEWND : création en cours et fenêtre après laquelle l'insérer.
#[repr(C)]
struct CbtCreateWnd {
    cs: *mut CreateStructA,
    insert_after: usize,
}

/// HCBT_CREATEWND (wParam = HWND, lParam = CBT_CREATEWND) : vrai si un
/// crochet WH_CBT du fil refuse la création. Le crochet peut changer la
/// CREATESTRUCT, relue ensuite par WM_NCCREATE et WM_CREATE.
unsafe fn cbt_create_window(hwnd: usize, cs: &mut CreateStructA) -> bool {
    let Some(h) = next_hook(WH_CBT, 0) else {
        return false;
    };
    let mut info = CbtCreateWnd {
        cs: cs as *mut CreateStructA,
        insert_after: 0,
    };
    run_hook(
        h,
        HCBT_CREATEWND,
        hwnd,
        &mut info as *mut CbtCreateWnd as isize,
    ) != 0
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
    // Fenêtre enfant : rcNormalPosition est relatif à la zone cliente du
    // parent. 7-Zip ajoute son « top » à la hauteur de la barre d'outils
    // pour placer son panneau : en coordonnées d'écran, la zone d'adresse
    // tombait 200 pixels trop bas (#1326).
    let child = gui::window_style(hwnd).is_some_and(|s| s & WS_CHILD != 0)
        && gui::window_parent(hwnd).unwrap_or(0) != 0;
    let rect = if child {
        gui::window_rect(hwnd)
    } else {
        gui::screen_rect(hwnd)
    };
    let Some(rect) = rect else {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    };
    let mut state = gui::WINDOW_NORMAL;
    with_backend(|b| state = b.window_state(hwnd));
    std::ptr::write_unaligned(wp, placement_of(state, rect));
    1
}

/// SetWindowPlacement : place la fenêtre sur rcNormalPosition puis l'affiche
/// selon showCmd, comme ShowWindow. 7-Zip ne montre sa fenêtre principale
/// que par cet appel : sans affichage, elle restait invisible dès que
/// GetWindowPlacement a réussi (#1159).
pub unsafe extern "win64" fn set_window_placement(hwnd: usize, wp: *const WindowPlacement) -> i32 {
    if wp.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if gui::screen_rect(hwnd).is_none() {
        crate::process::set_last_error(1400);
        return 0;
    }
    let p = std::ptr::read_unaligned(wp);
    let r = p.normal;
    if r.right > r.left && r.bottom > r.top {
        move_window(hwnd, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
    }
    show_window(hwnd, p.show_cmd as i32);
    1
}

/// Raisons de blocage de l'arrêt (ShutdownBlockReasonCreate) : gardées par
/// fenêtre pour ShutdownBlockReasonQuery. Les chargeurs PyInstaller en posent
/// une au démarrage (#1332). La session Winlux ne les montre pas encore.
fn shutdown_reasons() -> &'static std::sync::Mutex<HashMap<usize, String>> {
    static R: std::sync::OnceLock<std::sync::Mutex<HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

pub unsafe extern "win64" fn shutdown_block_reason_create(hwnd: usize, reason: *const u16) -> i32 {
    if gui::window_rect(hwnd).is_none() {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    let text = if reason.is_null() {
        String::new()
    } else {
        super::wstr(reason)
    };
    super::lock(shutdown_reasons()).insert(hwnd, text);
    1
}

pub extern "win64" fn shutdown_block_reason_destroy(hwnd: usize) -> i32 {
    super::lock(shutdown_reasons()).remove(&hwnd);
    1
}

/// ShutdownBlockReasonQuery : avec un tampon nul, *pcch reçoit la longueur
/// à prévoir (zéro compris).
pub unsafe extern "win64" fn shutdown_block_reason_query(
    hwnd: usize,
    buf: *mut u16,
    cch: *mut u32,
) -> i32 {
    if cch.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(text) = super::lock(shutdown_reasons()).get(&hwnd).cloned() else {
        crate::process::set_last_error(2); // ERROR_FILE_NOT_FOUND : aucune raison
        return 0;
    };
    let w: Vec<u16> = text.encode_utf16().collect();
    if buf.is_null() {
        *cch = w.len() as u32 + 1;
        return 1;
    }
    if (*cch as usize) < w.len() + 1 {
        *cch = w.len() as u32 + 1;
        crate::process::set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
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
        //    Les rappels SetWinEventHook en attente sont appelés pendant
        //    l'attente, comme sous Windows.
        if wake_mask != 0 {
            super::chromium_win32::deliver_win_events();
        }
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

// ---------- SetProp / GetProp ----------

/// Clé d'une propriété : un atome (MAKEINTATOM) et la chaîne qu'il nomme
/// désignent la même propriété, sans distinction de casse, comme sous Windows.
unsafe fn prop_key(name: usize, wide: bool) -> Option<String> {
    if name == 0 {
        return None;
    }
    if name < 0x10000 {
        let atom = name as u16;
        if atom >= 0xC000 {
            if let Some(n) = super::kernel32_legacy::atom_name(atom) {
                return Some(n.to_lowercase());
            }
        }
        return Some(format!("#{atom}"));
    }
    let s = if wide {
        super::wstr(name as *const u16)
    } else {
        super::cstr(name as *const u8)
    };
    if let Some(n) = s.strip_prefix('#').and_then(|d| d.parse::<u16>().ok()) {
        if n > 0 && n < 0xC000 {
            return Some(format!("#{n}"));
        }
    }
    Some(s.to_lowercase())
}

unsafe fn set_prop(hwnd: usize, name: usize, wide: bool, data: usize) -> i32 {
    let Some(key) = prop_key(name, wide) else {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    };
    if hwnd == 0 {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return 0;
    }
    gui::set_window_prop(hwnd, key, data);
    1
}

pub unsafe extern "win64" fn get_prop_a(hwnd: usize, name: *const u8) -> usize {
    prop_key(name as usize, false)
        .and_then(|k| gui::window_prop(hwnd, &k))
        .unwrap_or(0)
}

pub unsafe extern "win64" fn get_prop_w(hwnd: usize, name: *const u16) -> usize {
    prop_key(name as usize, true)
        .and_then(|k| gui::window_prop(hwnd, &k))
        .unwrap_or(0)
}

pub unsafe extern "win64" fn set_prop_a(hwnd: usize, name: *const u8, data: usize) -> i32 {
    set_prop(hwnd, name as usize, false, data)
}

pub unsafe extern "win64" fn set_prop_w(hwnd: usize, name: *const u16, data: usize) -> i32 {
    set_prop(hwnd, name as usize, true, data)
}

pub unsafe extern "win64" fn remove_prop_a(hwnd: usize, name: *const u8) -> usize {
    prop_key(name as usize, false)
        .and_then(|k| gui::remove_window_prop(hwnd, &k))
        .unwrap_or(0)
}

pub unsafe extern "win64" fn remove_prop_w(hwnd: usize, name: *const u16) -> usize {
    prop_key(name as usize, true)
        .and_then(|k| gui::remove_window_prop(hwnd, &k))
        .unwrap_or(0)
}

// ---------- Misc ----------

pub unsafe extern "win64" fn unregister_class_a(name: *const u8, instance: usize) -> i32 {
    unregister_class(
        (name as usize > 0xFFFF).then(|| read_class_name_a(name)),
        instance,
    )
}

pub unsafe extern "win64" fn unregister_class_w(name: *const u16, instance: usize) -> i32 {
    unregister_class(
        (name as usize > 0xFFFF).then(|| read_class_name_w(name)),
        instance,
    )
}

pub extern "win64" fn get_dc_ex(hwnd: usize, _rgn: usize, _flags: u32) -> usize {
    if hwnd == 0 {
        return gui::create_screen_dc();
    }
    gui::create_dc(hwnd)
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
        "GetClassWord"=>fn get_class_word,
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
        "TrackMouseEvent"=>fn track_mouse_event,
        "ScreenToClient"=>fn screen_to_client, "ClientToScreen"=>fn client_to_screen,
        // --- Window navigation ---
        "GetParent"=>fn get_parent, "SetParent"=>fn set_parent,
        "FindWindowA"=>fn find_window_a, "FindWindowW"=>fn find_window_w,
        "FindWindowExA"=>fn find_window_ex_a, "FindWindowExW"=>fn find_window_ex_w,
        "EnumWindows"=>fn enum_windows, "EnumChildWindows"=>fn enum_child_windows,
        "GetWindow"=>fn get_window, "GetTopWindow"=>fn get_top_window,
        "GetWindowThreadProcessId"=>fn get_window_thread_process_id,
        "GetWindowPlacement"=>fn get_window_placement,
        "ShutdownBlockReasonCreate"=>fn shutdown_block_reason_create,
        "ShutdownBlockReasonDestroy"=>fn shutdown_block_reason_destroy,
        "ShutdownBlockReasonQuery"=>fn shutdown_block_reason_query,
        "SetWindowPlacement"=>fn set_window_placement,
        // --- Painting ---
        "BeginPaint"=>fn begin_paint, "EndPaint"=>fn end_paint,
        "InvalidateRect"=>fn invalidate_rect, "ValidateRect"=>fn validate_rect,
        "GetDC"=>fn get_dc, "GetDCEx"=>fn get_dc_ex,
        "GetWindowDC"=>fn get_window_dc,
        "ReleaseDC"=>fn release_dc,
        "FillRect"=>fn fill_rect, "FrameRect"=>fn frame_rect,
        "DrawTextA"=>fn draw_text_a, "DrawTextW"=>fn draw_text_w,
        "DrawTextExA"=>fn draw_text_ex_a, "DrawTextExW"=>fn draw_text_ex_w,
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
        "SetCapture"=>fn set_capture, "ReleaseCapture"=>fn release_capture, "DragDetect"=>fn drag_detect,
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

/// Fenêtre surgissante qui n'est ni une boîte ni une fenêtre d'application :
/// WS_POPUP sans barre de titre, ni en couches ni WS_EX_APPWINDOW, et
/// marquée outil (WS_EX_TOOLWINDOW), jamais activée (WS_EX_NOACTIVATE) ou
/// toujours au premier plan (WS_EX_TOPMOST). C'est la forme d'un menu ou
/// d'une bulle dessinés par le programme, comme le menu de l'icône de
/// Claude (Chromium, #1222) : le gestionnaire de fenêtres ne doit ni la
/// recentrer ni lui donner un bouton dans la barre des tâches. La première
/// version exigeait outil ET non activable, ce que ce menu n'a pas.
/// Une fenêtre en couches (Flow Launcher) reste gérée : elle prend le
/// clavier.
pub(crate) fn floating_popup(style: u32, ex_style: u32) -> bool {
    const WS_POPUP: u32 = 0x8000_0000;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_EX_TOPMOST: u32 = 0x0000_0008;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_APPWINDOW: u32 = 0x0004_0000;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    style & WS_POPUP != 0
        && style & WS_CAPTION != WS_CAPTION
        && ex_style & (WS_EX_LAYERED | WS_EX_APPWINDOW) == 0
        && ex_style & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST) != 0
}

/// Fenêtre en couches (WS_EX_LAYERED) sans barre de titre Win32 : tout son
/// rendu, bords compris, vient du programme (UpdateLayeredWindow). Un cadre
/// du gestionnaire de fenêtres y ajoutait un titre et des bords (#1220).
fn layered_without_caption(style: u32, ex_style: u32) -> bool {
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    ex_style & WS_EX_LAYERED != 0 && style & WS_CAPTION != WS_CAPTION && style & WS_CHILD == 0
}

/// La fenêtre qui prend toute sa surface (WM_NCCALCSIZE) dessine-t-elle
/// tout elle-même, sans barre de titre à trouver ? Cas des fenêtres en
/// couches sans titre (#1220) : leur cadre part tout de suite.
fn drawn_whole(hwnd: usize) -> bool {
    layered_without_caption(
        gui::window_style(hwnd).unwrap_or(0),
        gui::window_ex_style(hwnd).unwrap_or(0),
    )
}

/// Cadre d'une fenêtre WS_POPUP sans barre de titre (WS_CAPTION) : la boîte
/// « À propos » de Notepad++ n'a qu'une bordure fine (#705). `None` : cadre
/// complet, comme toute fenêtre WS_OVERLAPPED ou avec WS_CAPTION.
/// Fenêtre outil avec barre de titre (WS_CAPTION + WS_EX_TOOLWINDOW) :
/// palette flottante, que Windows dessine avec le seul bouton Fermer. Pour
/// openbox, c'est une barre d'outils détachée (_NET_WM_WINDOW_TYPE_TOOLBAR),
/// qui n'a ni Réduire ni Agrandir et laisse donc sa place au titre : les
/// panneaux de Paint.NET, larges de 180 px, affichaient « H… ue » (#1761).
fn captioned_tool_window(style: u32, ex_style: u32) -> bool {
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    style & WS_CAPTION == WS_CAPTION && style & WS_CHILD == 0 && ex_style & WS_EX_TOOLWINDOW != 0
}

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

    /// #1761 : une palette flottante (WS_CAPTION + WS_EX_TOOLWINDOW) est une
    /// fenêtre outil ; une fenêtre ordinaire, un enfant ou un menu non.
    #[test]
    fn palette_flottante_reconnue() {
        use super::captioned_tool_window;
        // FormBorderStyle.SizableToolWindow : WS_CAPTION | WS_THICKFRAME | WS_SYSMENU.
        assert!(captioned_tool_window(0x02CC_0000, 0x0001_0180));
        assert!(!captioned_tool_window(0x00CF_0000, 0x0000_0100));
        // Sans barre de titre (fenêtre surgissante outil).
        assert!(!captioned_tool_window(0x8000_0000, 0x0000_0080));
        // Enfant.
        assert!(!captioned_tool_window(0x40C0_0000, 0x0000_0080));
    }

    /// #1630 : une classe déjà inscrite par le même module est refusée
    /// (ERROR_CLASS_ALREADY_EXISTS) jusqu'à son UnregisterClass.
    #[test]
    fn classe_deja_inscrite_refusee_puis_reinscrite() {
        let _teb = crate::process::test_teb();
        let name: Vec<u16> = "Winlux1630Message".encode_utf16().chain([0]).collect();
        let wc = gui::WndClassA {
            style: 0,
            wnd_proc: def_window_proc_w as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: 0x40_0000,
            icon: 0,
            cursor: 0,
            background: 0,
            menu_name: std::ptr::null(),
            class_name: name.as_ptr().cast(),
        };
        unsafe {
            let atom = register_class_w(&wc);
            assert_ne!(atom, 0);
            assert_eq!(register_class_w(&wc), 0);
            assert_eq!(crate::process::last_error(), 1410);
            // Un autre module peut inscrire une classe de même nom.
            let other = gui::WndClassA {
                instance: 0x50_0000,
                ..wc
            };
            assert_ne!(register_class_w(&other), 0);
            assert_eq!(unregister_class_w(name.as_ptr(), 0x40_0000), 1);
            assert_eq!(unregister_class_w(name.as_ptr(), 0x40_0000), 0);
            assert_eq!(crate::process::last_error(), 1411);
            assert_eq!(register_class_w(&wc), atom);
        }
    }
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

    /// #1222 : le menu de l'icône de Claude (WS_POPUP, outil, jamais
    /// activé) échappe au gestionnaire ; une fenêtre outil ordinaire
    /// (Flow Launcher, activable) ou une boîte avec titre reste gérée.
    /// #1220 : Flow Launcher (WPF transparente) n'a pas de cadre openbox.
    #[test]
    fn fenetre_en_couches_sans_cadre() {
        assert!(layered_without_caption(0x0606_0000, 0x0008_0088));
        assert!(layered_without_caption(0x8000_0000, 0x0008_0000));
        assert!(!layered_without_caption(0x00CF_0000, 0x0008_0000));
        assert!(!layered_without_caption(0x0606_0000, 0x0000_0088));
        assert!(!layered_without_caption(0x4000_0000, 0x0008_0000));
    }

    #[test]
    fn menu_dessine_par_le_programme_hors_gestionnaire() {
        assert!(floating_popup(0x8000_0000, 0x0800_0080));
        assert!(floating_popup(0x8000_0000, 0x0000_0080));
        assert!(floating_popup(0x8600_0000, 0x0000_0008));
        assert!(floating_popup(0x8000_0000, 0x0800_0000));
        assert!(
            !floating_popup(0x8000_0000, 0x0008_0088),
            "en couches : Flow"
        );
        assert!(!floating_popup(0x8000_0000, 0x0004_0080), "WS_EX_APPWINDOW");
        assert!(!floating_popup(0x8000_0000, 0));
        assert!(!floating_popup(0x80C0_0000, 0x0800_0080));
        assert!(!floating_popup(0x0000_0000, 0x0800_0080));
    }

    /// #1311 : Qt 5 donne le focus à la fenêtre cachée de sa boîte de
    /// fichiers native puis la détruit ; le propriétaire doit le retrouver
    /// (WM_SETFOCUS), sinon VLC reste inactif et sa barre de menus grisée.
    #[test]
    fn focus_rendu_au_proprietaire_a_la_destruction() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SETFOCUS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "win64" fn proc_(h: usize, m: u32, w: usize, l: isize) -> isize {
            if m == gui::WM_SETFOCUS {
                SETFOCUS.fetch_add(1, Ordering::SeqCst);
            }
            gui::def_window_proc(h, m, w, l)
        }
        let got = std::thread::spawn(|| unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let make = |style: u32| {
                create_window_ex_w(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    style,
                    0,
                    0,
                    80,
                    60,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let main = make(0x10CF_0000);
            set_window_long_ptr_w(main, -4, proc_ as *const () as isize);
            let dialog = make(0x86CC_0000);
            set_window_long_ptr_w(dialog, GWLP_HWNDPARENT, main as isize);
            set_focus(main);
            let before = SETFOCUS.load(Ordering::SeqCst);
            set_focus(dialog);
            assert_eq!(get_focus(), dialog);
            destroy_window(dialog);
            let r = (
                get_focus() == main,
                SETFOCUS.load(Ordering::SeqCst) - before,
            );
            // Propriétaire retiré avant la destruction (Qt 5) : la fenêtre
            // suivante du fil dans l'ordre Z reprend le focus.
            let hidden = make(0x86CC_0000);
            set_window_long_ptr_w(hidden, GWLP_HWNDPARENT, main as isize);
            set_focus(hidden);
            set_window_long_ptr_w(hidden, GWLP_HWNDPARENT, 0);
            destroy_window(hidden);
            assert_eq!(get_focus(), main, "propriétaire retiré");
            // Une fenêtre détruite sans le focus ne le déplace pas.
            let other = make(0x86CC_0000);
            set_window_long_ptr_w(other, GWLP_HWNDPARENT, main as isize);
            set_focus(0);
            destroy_window(other);
            let r2 = get_focus();
            destroy_window(main);
            (r, r2)
        })
        .join()
        .unwrap();
        assert_eq!(got, ((true, 1), 0));
    }

    /// #842 : wxWidgets ajoute SM_CXMENUCHECK à la largeur des libellés
    /// de cases à cocher ; à 0, ils étaient tronqués.
    #[test]
    fn taille_des_cases_a_cocher() {
        assert_eq!(get_system_metrics(71), 15);
        assert_eq!(get_system_metrics(72), 15);
    }

    /// #1574 : délai et zone de survol (minuteur des barres d'outils WinForms).
    #[test]
    fn delai_et_zone_de_survol() {
        for (action, want) in [(0x0066u32, 400u32), (0x0062, 4), (0x0064, 4)] {
            let mut v = 0u32;
            let r = unsafe { system_parameters_info_w(action, 0, (&mut v as *mut u32).cast(), 0) };
            assert_eq!((r, v), (1, want), "action {action:#x}");
        }
    }

    /// #1803 : aucun délai de clavier, de menu ou de message ne reste à 0.
    #[test]
    fn delais_clavier_menus_et_messages() {
        for (action, want) in [
            (0x000Au32, 31u32),
            (0x0016, 1),
            (0x006A, 400),
            (0x2000, 200_000),
            (0x200E, 1),
            (0x2010, 1),
            (0x2016, 5),
            (0x2022, 5000),
        ] {
            let mut v = 0u32;
            let r = unsafe { system_parameters_info_w(action, 0, (&mut v as *mut u32).cast(), 0) };
            assert_eq!((r, v), (1, want), "action {action:#x}");
        }
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
            // FindWindow avec un atome : lu comme tel, jamais déréférencé
            // (Acrobat passe MAKEINTATOM(0x8002), #1754).
            let _ = find_window_a(0x8002 as *const u8, std::ptr::null());
            let _ = find_window_w(0x8002 as *const u16, std::ptr::null());
            let _ = find_window_ex_a(0, 0, a as usize as *const u8, std::ptr::null());
            let _ = find_window_ex_w(0, 0, b as usize as *const u16, std::ptr::null());
            assert_eq!(read_class_name_a(0x8002 as *const u8), "#32770");
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
        let _guard = gui::CURSOR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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

    /// CW_USEDEFAULT suit la zone de travail au lieu d'un 800 × 600 fixe (#1277).
    #[test]
    fn taille_par_defaut_suit_la_zone_de_travail() {
        let area = gui::work_area();
        let (aw, ah) = (area.right - area.left, area.bottom - area.top);
        let (w, h) = default_window_size();
        assert!(w <= aw && h <= ah, "{w}x{h} dans {aw}x{ah}");
        assert!(w >= 800.min(aw) && h >= 600.min(ah));
        assert_eq!(w, (aw * 3 / 4).max(800).min(aw));
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                gui::CW_USEDEFAULT,
                gui::CW_USEDEFAULT,
                gui::CW_USEDEFAULT,
                gui::CW_USEDEFAULT,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            let r = gui::screen_rect(hwnd).expect("rectangle");
            assert_eq!((r.right - r.left, r.bottom - r.top), (w, h));
            destroy_window(hwnd);
        }
    }

    /// GetClassWord et GetClassLong(Ptr) relisent la classe de la fenêtre
    /// (Steam, #1858).
    #[test]
    fn valeurs_de_classe_relues() {
        let class: Vec<u16> = "Static\0".encode_utf16().collect();
        unsafe {
            let h = create_window_ex_w(
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
            assert_ne!(h, 0);
            let atom = class_atom("Static");
            assert_eq!(get_class_word(h, -32), atom);
            assert_eq!(get_class_long_w(h, -32), atom as u32);
            assert_eq!(
                get_class_long_ptr_w(h, -24),
                gui::lookup_class("Static").expect("classe").wnd_proc
            );
            assert_ne!(get_class_long_ptr_w(h, -12), 0, "curseur flèche");
            assert_eq!(get_class_long_ptr_w(h, 0), 0, "octets supplémentaires");
            destroy_window(h);
            assert_eq!(get_class_word(h, -32), 0, "fenêtre détruite");
        }
    }

    /// ShutdownBlockReasonCreate/Query/Destroy (chargeurs PyInstaller, #1332).
    #[test]
    fn raison_de_blocage_de_l_arret() {
        let class: Vec<u16> = "Static\0".encode_utf16().collect();
        let raison: Vec<u16> = "Sauvegarde en cours\0".encode_utf16().collect();
        unsafe {
            let h = create_window_ex_w(
                0,
                class.as_ptr(),
                class.as_ptr(),
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
            let mut n = 0u32;
            assert_eq!(
                shutdown_block_reason_query(h, std::ptr::null_mut(), &mut n),
                0
            );
            assert_eq!(shutdown_block_reason_create(h, raison.as_ptr()), 1);
            assert_eq!(
                shutdown_block_reason_query(h, std::ptr::null_mut(), &mut n),
                1
            );
            assert_eq!(n, 20);
            let mut buf = [0u16; 32];
            n = 32;
            assert_eq!(shutdown_block_reason_query(h, buf.as_mut_ptr(), &mut n), 1);
            assert_eq!(&buf[..20], &raison[..20]);
            assert_eq!(shutdown_block_reason_destroy(h), 1);
            assert_eq!(
                shutdown_block_reason_query(h, std::ptr::null_mut(), &mut n),
                0
            );
            assert_eq!(
                shutdown_block_reason_create(0x7777_0000, raison.as_ptr()),
                0
            );
            destroy_window(h);
        }
    }

    static POS_CHANGES: std::sync::Mutex<Vec<(i32, i32, i32, i32, u32)>> =
        std::sync::Mutex::new(Vec::new());

    unsafe extern "win64" fn proc_vcl_bornes(h: usize, msg: u32, w: usize, l: isize) -> isize {
        if msg == WM_WINDOWPOSCHANGING {
            // Contrainte de largeur maximale, comme Constraints.MaxWidth.
            let pos = &mut *(l as *mut WindowPos);
            if pos.cx > 300 {
                pos.cx = 300;
            }
        }
        if msg == WM_WINDOWPOSCHANGED {
            // UpdateBounds de la VCL : relit la fenêtre déjà placée.
            let pos = &*(l as *const WindowPos);
            let r = gui::window_rect(h).unwrap();
            assert_eq!(
                (pos.x, pos.y, pos.cx, pos.cy),
                (r.left, r.top, r.width(), r.height())
            );
            POS_CHANGES
                .lock()
                .unwrap()
                .push((r.left, r.top, r.width(), r.height(), pos.flags));
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// SetWindowPos envoie WM_WINDOWPOSCHANGING puis WM_WINDOWPOSCHANGED :
    /// la VCL y relit ses bornes, sinon le Top lu du DFM effaçait le Left
    /// (champ du port de WinSCP en (0,0), #1251).
    #[test]
    fn placement_annonce_a_la_fenetre() {
        let name: Vec<u16> = "Winlux1251Bornes".encode_utf16().chain([0]).collect();
        unsafe {
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_vcl_bornes as *const () as usize,
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
            let h = create_window_ex_w(
                0,
                name.as_ptr(),
                name.as_ptr(),
                0,
                0,
                0,
                121,
                14,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(h, 0);
            POS_CHANGES.lock().unwrap().clear();
            // Left := 280, puis Top := 87 avec le Left relu.
            assert_eq!(set_window_pos(h, 0, 280, 0, 121, 14, 0x14), 1);
            let r = gui::window_rect(h).unwrap();
            assert_eq!(set_window_pos(h, 0, r.left, 87, 121, 14, 0x14), 1);
            assert_eq!(set_window_pos(h, 0, 0, 0, 500, 23, 0x16), 1);
            // Rien ne change : pas de WM_WINDOWPOSCHANGED.
            assert_eq!(set_window_pos(h, 0, 0, 0, 300, 23, 0x16), 1);
            let seen = POS_CHANGES.lock().unwrap().clone();
            assert_eq!(seen.len(), 3, "{seen:?}");
            assert_eq!((seen[0].0, seen[0].1), (280, 0));
            assert_eq!((seen[1].0, seen[1].1), (280, 87));
            assert_eq!(seen[1].4 & SWP_NOSIZE, SWP_NOSIZE);
            // La contrainte posée dans WM_WINDOWPOSCHANGING est appliquée.
            assert_eq!(
                (seen[2].0, seen[2].1, seen[2].2, seen[2].3),
                (280, 87, 300, 23)
            );
            destroy_window(h);
        }
    }

    static VUS_2044: std::sync::Mutex<Vec<(u32, isize)>> = std::sync::Mutex::new(Vec::new());
    static BLOQUE_2044: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    unsafe extern "win64" fn proc_qt_2044(h: usize, msg: u32, w: usize, l: isize) -> isize {
        if msg == gui::WM_SIZE || msg == gui::WM_MOVE {
            VUS_2044.lock().unwrap().push((msg, l));
            return 0;
        }
        if msg == WM_WINDOWPOSCHANGED && BLOQUE_2044.load(std::sync::atomic::Ordering::Relaxed) {
            return 0;
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// #2044 : DefWindowProc(WM_WINDOWPOSCHANGED) envoie WM_SIZE avant le
    /// retour de SetWindowPos. Qt relit sa géométrie sur WM_SIZE : iCUE,
    /// redimensionné en largeur puis en hauteur, retombait à 160x160 quand
    /// WM_SIZE était seulement posté. Sans DefWindowProc, il reste posté.
    #[test]
    fn taille_annoncee_avant_le_retour_de_setwindowpos() {
        let name: Vec<u16> = "Winlux2044Qt".encode_utf16().chain([0]).collect();
        unsafe {
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_qt_2044 as *const () as usize,
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
            let h = create_window_ex_w(
                0,
                name.as_ptr(),
                name.as_ptr(),
                0,
                880,
                440,
                160,
                160,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(h, 0);
            let mut m = Msg::new(0, 0, 0, 0);
            while peek_message_w(&mut m, h, 0, 0, 1) != 0 {}
            VUS_2044.lock().unwrap().clear();
            // Largeur seule, puis hauteur seule (QWindow::setWidth / setHeight).
            assert_eq!(set_window_pos(h, 0, 880, 440, 1368, 160, SWP_NOZORDER), 1);
            assert_eq!(
                *VUS_2044.lock().unwrap(),
                [(gui::WM_SIZE, pack_i16_pair(1368, 160))]
            );
            assert_eq!(set_window_pos(h, 0, 880, 440, 1368, 840, SWP_NOZORDER), 1);
            assert_eq!(
                VUS_2044.lock().unwrap()[1],
                (gui::WM_SIZE, pack_i16_pair(1368, 840))
            );
            // Rien de plus en file : pas de second WM_SIZE.
            assert_eq!(peek_message_w(&mut m, h, gui::WM_SIZE, gui::WM_SIZE, 1), 0);
            // Déplacement : WM_MOVE envoyé aussi.
            assert_eq!(
                set_window_pos(h, 0, 10, 20, 0, 0, SWP_NOZORDER | SWP_NOSIZE),
                1
            );
            assert_eq!(
                VUS_2044.lock().unwrap()[2],
                (gui::WM_MOVE, pack_i16_pair(10, 20))
            );
            // WM_WINDOWPOSCHANGED gardé par la fenêtre : WM_SIZE posté.
            VUS_2044.lock().unwrap().clear();
            BLOQUE_2044.store(true, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(
                set_window_pos(h, 0, 0, 0, 300, 200, SWP_NOZORDER | SWP_NOMOVE),
                1
            );
            BLOQUE_2044.store(false, std::sync::atomic::Ordering::Relaxed);
            assert!(VUS_2044.lock().unwrap().is_empty());
            assert_eq!(peek_message_w(&mut m, h, gui::WM_SIZE, gui::WM_SIZE, 1), 1);
            assert_eq!(m.lparam, pack_i16_pair(300, 200));
            destroy_window(h);
        }
    }

    static CAPTURE_CHANGES: std::sync::Mutex<Vec<(usize, isize)>> =
        std::sync::Mutex::new(Vec::new());

    unsafe extern "win64" fn proc_capture(h: usize, msg: u32, w: usize, l: isize) -> isize {
        if msg == WM_CAPTURECHANGED {
            CAPTURE_CHANGES.lock().unwrap().push((h, l));
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// La fenêtre qui perd la capture reçoit WM_CAPTURECHANGED avec la
    /// nouvelle (ou 0), et TrackMouseEvent rend compte du suivi (#1254).
    #[test]
    fn capture_et_sortie_du_pointeur_signalees() {
        let name: Vec<u16> = "Winlux1254Capture".encode_utf16().chain([0]).collect();
        unsafe {
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_capture as *const () as usize,
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
            let make = || {
                create_window_ex_w(
                    0,
                    name.as_ptr(),
                    name.as_ptr(),
                    0,
                    0,
                    0,
                    50,
                    50,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let (a, b) = (make(), make());
            assert!(a != 0 && b != 0);
            set_capture(a);
            set_capture(b);
            release_capture();
            let seen = CAPTURE_CHANGES.lock().unwrap().clone();
            assert!(seen.contains(&(a, b as isize)), "{seen:?}");
            assert!(seen.contains(&(b, 0)), "{seen:?}");

            // TRACKMOUSEEVENT 64 bits : cbSize, dwFlags, hwndTrack, dwHoverTime.
            let mut tme = [0u8; 24];
            tme[..4].copy_from_slice(&24u32.to_le_bytes());
            tme[4..8].copy_from_slice(&2u32.to_le_bytes()); // TME_LEAVE
            tme[8..16].copy_from_slice(&0x7777_0000usize.to_le_bytes());
            assert_eq!(track_mouse_event(tme.as_mut_ptr()), 0);
            gui::track_mouse_leave(a, true);
            tme[4..8].copy_from_slice(&0x4000_0000u32.to_le_bytes()); // TME_QUERY
            tme[8..16].copy_from_slice(&a.to_le_bytes());
            assert_eq!(track_mouse_event(tme.as_mut_ptr()), 1);
            assert_eq!(u32::from_le_bytes(tme[4..8].try_into().unwrap()), 2);
            tme[4..8].copy_from_slice(&0x8000_0002u32.to_le_bytes()); // TME_CANCEL | TME_LEAVE
            assert_eq!(track_mouse_event(tme.as_mut_ptr()), 1);
            assert!(!gui::mouse_leave_tracked(a));
            destroy_window(a);
            destroy_window(b);
        }
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

    unsafe extern "win64" fn proc_cadre_maison(h: usize, msg: u32, w: usize, l: isize) -> isize {
        // WindowChrome de WPF : WM_NCCALCSIZE traité sans DefWindowProc,
        // toute la fenêtre reste zone cliente.
        if msg == 0x0083 {
            return 0;
        }
        // Sa barre de titre maison : HTCAPTION dans la bande du haut.
        if msg == 0x0084 {
            return 2;
        }
        gui::def_window_proc(h, msg, w, l)
    }

    static PAGE_CHARGEE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    unsafe extern "win64" fn proc_electron_vide(h: usize, msg: u32, w: usize, l: isize) -> isize {
        // Electron sans cadre : WM_NCCALCSIZE gardé, mais tant que la page
        // n'est pas chargée, aucune zone glissable ni bouton (HTCLIENT).
        if msg == 0x0083 {
            return 0;
        }
        if msg == 0x0084 {
            return if PAGE_CHARGEE.load(std::sync::atomic::Ordering::SeqCst) {
                2
            } else {
                1
            };
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// #1211 : une fenêtre sans barre de titre à elle garde le cadre du
    /// gestionnaire ; il part quand la barre apparaît et que la souris passe.
    #[test]
    fn cadre_garde_tant_que_la_fenetre_n_a_pas_de_barre() {
        unsafe {
            let name: Vec<u16> = "Winlux1211Electron".encode_utf16().chain([0]).collect();
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_electron_vide as *const () as usize,
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
                0x00CF_0000,
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
            set_window_pos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            );
            let own = |h: usize| OWN_FRAME.lock().unwrap().contains(&h);
            assert!(
                !own(hwnd),
                "cadre retiré alors que rien ne permet de fermer"
            );
            // La page se charge ; la souris passe après le délai.
            PAGE_CHARGEE.store(true, std::sync::atomic::Ordering::SeqCst);
            for (h, t, _) in pending_frame().iter_mut() {
                if *h == hwnd {
                    *t -= std::time::Duration::from_secs(1);
                }
            }
            // Sans que la souris y passe : n'importe quel message distribué
            // par le thread de la fenêtre, ici à une autre fenêtre (#1444).
            let autre = create_window_ex_w(
                0,
                name.as_ptr(),
                name.as_ptr(),
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
            assert_ne!(autre, 0);
            let m = Msg::new(autre, 0x0400, 0, 0);
            dispatch_message_w(&m);
            assert!(own(hwnd));
            assert!(!pending_frame().iter().any(|(h, _, _)| *h == hwnd));
            destroy_window(autre);
            destroy_window(hwnd);
        }
    }

    unsafe extern "win64" fn proc_cadre_standard(h: usize, msg: u32, w: usize, l: isize) -> isize {
        gui::def_window_proc(h, msg, w, l)
    }

    /// #1131 : le cadre du gestionnaire est retiré d'une fenêtre qui dessine
    /// sa propre barre de titre, et seulement d'elle.
    #[test]
    fn wm_nccalcsize_traite_par_l_appli_retire_le_cadre() {
        unsafe {
            let mut fenetres = Vec::new();
            for (nom, proc_) in [
                ("Winlux1131Maison", proc_cadre_maison as *const () as usize),
                (
                    "Winlux1131Standard",
                    proc_cadre_standard as *const () as usize,
                ),
            ] {
                let name: Vec<u16> = nom.encode_utf16().chain([0]).collect();
                let wc = gui::WndClassA {
                    style: 0,
                    wnd_proc: proc_,
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
                    0x00CF_0000, // WS_OVERLAPPEDWINDOW
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
                assert_eq!(
                    set_window_pos(
                        hwnd,
                        0,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED
                    ),
                    1
                );
                fenetres.push(hwnd);
            }
            let own = |h: usize| OWN_FRAME.lock().unwrap().contains(&h);
            assert!(own(fenetres[0]));
            assert!(!own(fenetres[1]));
            for h in fenetres {
                destroy_window(h);
            }
        }
    }

    static VUS_1476: std::sync::Mutex<Vec<(u32, usize, isize, i32, i32)>> =
        std::sync::Mutex::new(Vec::new());

    unsafe extern "win64" fn proc_claude_1476(h: usize, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            0x0083 => return 0,
            // Barre glissable sur les 30 premiers pixels, client ailleurs.
            0x0084 => {
                let y = (l as u32 >> 16) as u16 as i16 as i32;
                let top = gui::window_rect(h).map_or(0, |r| r.top);
                return if y - top < 30 { 2 } else { 1 };
            }
            0x0047 => {
                let pos = &*(l as *const WindowPos);
                VUS_1476
                    .lock()
                    .unwrap()
                    .push((msg, pos.flags as usize, 0, pos.cx, pos.cy));
                return 0;
            }
            0x0005 | 0x0201 | 0x0203 | 0x00A1 | 0x00A3 => {
                VUS_1476.lock().unwrap().push((msg, w, l, 0, 0));
                return 0;
            }
            _ => {}
        }
        gui::def_window_proc(h, msg, w, l)
    }

    /// #1476 : un WM_SIZE venu du gestionnaire (agrandir) est précédé de
    /// WM_WINDOWPOSCHANGED, sur lequel Chromium redimensionne son rendu ;
    /// celui de SetWindowPos ne l'est pas une seconde fois. #1477 : un clic
    /// sur la barre maison devient WM_NCLBUTTONDOWN(HTCAPTION), un clic
    /// dans la page reste un clic client.
    #[test]
    fn fenetre_sans_cadre_agrandie_et_glissee() {
        unsafe {
            let name: Vec<u16> = "Winlux1476Claude".encode_utf16().chain([0]).collect();
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: proc_claude_1476 as *const () as usize,
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
                0x00CF_0000,
                100,
                50,
                1000,
                800,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            set_window_pos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            );
            assert!(OWN_FRAME.lock().unwrap().contains(&hwnd));
            let vus = || std::mem::take(&mut *VUS_1476.lock().unwrap());
            vus();

            // Agrandie par le gestionnaire : 1920x1039.
            gui::set_window_rect(
                hwnd,
                Rect {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1039,
                },
            );
            let mut m = Msg::new(hwnd, gui::WM_SIZE, 2, pack_i16_pair(1920, 1039));
            m.l_private = gui::BACKEND_GEOMETRY;
            dispatch_message_w(&m);
            let got = vus();
            assert_eq!(got.len(), 2, "{got:?}");
            assert_eq!(got[0].0, 0x0047);
            assert_eq!(got[0].1 as u32 & SWP_NOSIZE, 0);
            assert_eq!((got[0].3, got[0].4), (1920, 1039));
            assert_eq!(got[1].0, gui::WM_SIZE);

            // WM_SIZE posté par SetWindowPos : WM_WINDOWPOSCHANGED déjà envoyé.
            dispatch_message_w(&Msg::new(hwnd, gui::WM_SIZE, 0, pack_i16_pair(1920, 1039)));
            let got = vus();
            assert_eq!(got.iter().map(|v| v.0).collect::<Vec<_>>(), [gui::WM_SIZE]);

            // Clic dans la barre maison (y = 10) : légende, coordonnées écran ;
            // un second clic rapide au même endroit : double-clic.
            dispatch_message_w(&Msg::new(hwnd, 0x0201, 1, pack_i16_pair(500, 10)));
            dispatch_message_w(&Msg::new(hwnd, 0x0201, 1, pack_i16_pair(500, 10)));
            // Clic dans la page (y = 400) : clic client inchangé.
            dispatch_message_w(&Msg::new(hwnd, 0x0201, 1, pack_i16_pair(500, 400)));
            let got = vus();
            assert_eq!(
                got.iter().map(|v| (v.0, v.1)).collect::<Vec<_>>(),
                [(0x00A1, 2), (0x00A3, 2), (0x0201, 1)]
            );
            assert_eq!(got[0].2, pack_i16_pair(500, 10));

            // « Chrome Legacy Window » : enfant transparent qui couvre la page
            // et reçoit le clic. La légende est demandée à la fenêtre de
            // premier niveau, qui reçoit WM_NCLBUTTONDOWN.
            let legacy_name: Vec<u16> = "Winlux1477Legacy".encode_utf16().chain([0]).collect();
            let wc = gui::WndClassA {
                style: 0,
                wnd_proc: def_window_proc_w as *const () as usize,
                cls_extra: 0,
                wnd_extra: 0,
                instance: 0,
                icon: 0,
                cursor: 0,
                background: 0,
                menu_name: std::ptr::null(),
                class_name: legacy_name.as_ptr().cast(),
            };
            assert_ne!(register_class_w(&wc), 0);
            let legacy = create_window_ex_w(
                0x20, // WS_EX_TRANSPARENT
                legacy_name.as_ptr(),
                legacy_name.as_ptr(),
                0x5000_0000, // WS_CHILD | WS_VISIBLE
                0,
                0,
                1920,
                1039,
                hwnd,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(legacy, 0);
            dispatch_message_w(&Msg::new(legacy, 0x0201, 1, pack_i16_pair(700, 12)));
            let got = vus();
            assert_eq!(
                got.iter().map(|v| (v.0, v.1, v.2)).collect::<Vec<_>>(),
                [(0x00A1, 2, pack_i16_pair(700, 12))]
            );
            destroy_window(legacy);
            destroy_window(hwnd);
        }
    }

    static WIN_EVENTS_1498: std::sync::Mutex<Vec<(u32, usize)>> = std::sync::Mutex::new(Vec::new());

    unsafe extern "win64" fn crochet_1498(
        _hook: usize,
        event: u32,
        hwnd: usize,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        WIN_EVENTS_1498.lock().unwrap().push((event, hwnd));
    }

    /// #1498 : les événements SetWinEventHook (réduction, déplacement) sont
    /// livrés au thread qui a posé le crochet quand il relève ses messages,
    /// une seule fois par fenêtre tant qu'ils attendent.
    #[test]
    fn crochet_win_event_livre_au_thread_qui_le_pose() {
        use super::super::chromium_win32 as w;
        unsafe {
            let hook = w::SetWinEventHook(
                0x0016,
                0x0017,
                0,
                crochet_1498 as *const () as usize,
                0,
                0,
                0,
            );
            assert_ne!(hook, 0);
            let fenetre = 0x1498_0000;
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZESTART, fenetre);
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZESTART, fenetre);
            // Hors de l'intervalle du crochet : rien.
            w::notify_win_event(w::EVENT_OBJECT_LOCATIONCHANGE, fenetre);
            // Un autre thread ne reçoit rien.
            std::thread::spawn(w::deliver_win_events_for_test)
                .join()
                .unwrap();
            assert!(WIN_EVENTS_1498.lock().unwrap().is_empty());
            let mut m = Msg::new(0, 0, 0, 0);
            peek_message_w(&mut m, 0, 0, 0, 1);
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZEEND, fenetre);
            peek_message_w(&mut m, 0, 0, 0, 1);
            assert_eq!(
                std::mem::take(&mut *WIN_EVENTS_1498.lock().unwrap()),
                [
                    (w::EVENT_SYSTEM_MINIMIZESTART, fenetre),
                    (w::EVENT_SYSTEM_MINIMIZEEND, fenetre)
                ]
            );
            assert_eq!(w::UnhookWinEvent(hook), 1);
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZEEND, fenetre);
            peek_message_w(&mut m, 0, 0, 0, 1);
            assert!(WIN_EVENTS_1498.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn peek_message_rend_wm_timer_comme_get_message() {
        unsafe {
            // Minuteur de thread (hwnd 0), comme SetTimer(NULL, …).
            // Période longue : sous la charge de la suite complète, une
            // période de 30 ms pouvait échoir de nouveau avant la dernière
            // vérification (#1141).
            let id = set_timer(0, 0, 300, 0);
            assert_ne!(id, 0);
            std::thread::sleep(std::time::Duration::from_millis(330));
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

    /// Fenêtre de premier niveau créée en 0×0 (TApplication de la VCL) : elle
    /// garde sa taille nulle ; seul CW_USEDEFAULT donne 800×600 (#1276).
    #[test]
    fn proprietes_de_fenetre_gardees() {
        // La VCL (WinSCP x86) range son objet sous un atome et le relit pour
        // refléter WM_NOTIFY / WM_DRAWITEM à ses contrôles (#1249, #1250).
        let class: Vec<u16> = "Static\0".encode_utf16().collect();
        let hwnd = unsafe {
            create_window_ex_w(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                10,
                10,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(hwnd, 0);
        let nom: Vec<u16> = "ControlOfs0040000000000123\0".encode_utf16().collect();
        let atom = super::super::kernel32_legacy::add_atom_w(nom.as_ptr() as usize);
        assert!(atom >= 0xC000);
        let autre: Vec<u16> = "CONTROLOFS0040000000000123\0".encode_utf16().collect();
        let int7 = b"#7\0";
        unsafe {
            assert_eq!(get_prop_w(hwnd, atom as usize as *const u16), 0);
            assert_eq!(
                set_prop_w(hwnd, atom as usize as *const u16, 0xfee3_54a0),
                1
            );
            assert_eq!(get_prop_w(hwnd, atom as usize as *const u16), 0xfee3_54a0);
            // Le nom, quelle que soit sa casse, désigne la même propriété.
            assert_eq!(get_prop_w(hwnd, autre.as_ptr()), 0xfee3_54a0);
            assert_eq!(set_prop_a(hwnd, 7 as *const u8, 42), 1);
            assert_eq!(get_prop_a(hwnd, int7.as_ptr()), 42);
            assert_eq!(remove_prop_w(hwnd, nom.as_ptr()), 0xfee3_54a0);
            assert_eq!(get_prop_w(hwnd, atom as usize as *const u16), 0);
            destroy_window(hwnd);
            // Détruite : ses propriétés partent avec elle.
            assert_eq!(gui::window_prop(hwnd, "#7"), None);
        }
    }

    #[test]
    fn fenetre_de_taille_nulle_reste_nulle() {
        let class: Vec<u16> = "Static\0".encode_utf16().collect();
        let make = |cx: i32, cy: i32| unsafe {
            create_window_ex_w(
                0x80,
                class.as_ptr(),
                class.as_ptr(),
                0x84ca_0000u32,
                960,
                540,
                cx,
                cy,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        let nulle = make(0, 0);
        assert_ne!(nulle, 0);
        assert_eq!(
            gui::window_rect(nulle).map(|r| (r.width(), r.height())),
            Some((0, 0))
        );
        assert!(empty_top_level(nulle));
        unsafe {
            // SWP_SHOWWINDOW : visible pour le programme, sans surface.
            set_window_pos(nulle, 0, 0, 0, 0, 0, 0x57);
            assert!(gui::is_window_visible(nulle));
            set_window_pos(nulle, 0, 0, 0, 200, 100, 0x16);
        }
        assert!(!empty_top_level(nulle));
        let defaut = make(gui::CW_USEDEFAULT, 0);
        // Taille par défaut (au moins 800×600, ou selon l'écran avec #1277).
        let (w, h) = gui::window_rect(defaut).map_or((0, 0), |r| (r.width(), r.height()));
        assert!(w >= 600 && h >= 400, "{w}x{h}");
        unsafe {
            destroy_window(nulle);
            destroy_window(defaut);
        }
    }

    #[test]
    fn draw_text_retire_les_prefixes_de_mnemonique() {
        assert_eq!(split_prefix("&Hostname:"), ("Hostname:".into(), Some(0)));
        assert_eq!(
            split_prefix("Po&rt number:"),
            ("Port number:".into(), Some(2))
        );
        assert_eq!(split_prefix("A && B"), ("A & B".into(), None));
        assert_eq!(split_prefix("fin&"), ("fin".into(), None));
        // DT_CALCRECT mesure le texte affiché, sans l'esperluette.
        const DT_CALCRECT: u32 = 0x0400;
        const DT_NOPREFIX: u32 = 0x0800;
        let avec = b"&Port\0";
        let sans = b"Port\0";
        let mut r1 = Rect::default();
        let mut r2 = Rect::default();
        let mut r3 = Rect::default();
        unsafe {
            draw_text_a(0, avec.as_ptr(), -1, &mut r1, DT_CALCRECT);
            draw_text_a(0, sans.as_ptr(), -1, &mut r2, DT_CALCRECT);
            draw_text_a(0, avec.as_ptr(), -1, &mut r3, DT_CALCRECT | DT_NOPREFIX);
        }
        assert_eq!(r1.right, r2.right);
        assert!(r3.right > r2.right, "DT_NOPREFIX garde « & »");
    }

    #[test]
    fn draw_text_coupe_les_lignes_et_les_mots() {
        // Chaque caractère mesure 10 : le découpage est indépendant de la police.
        let m = |s: &str| s.chars().count() as i32 * 10;
        let l = |v: Vec<(String, Option<usize>)>| v.into_iter().map(|x| x.0).collect::<Vec<_>>();
        // CR LF, CR et LF coupent hors DT_SINGLELINE (#1341).
        assert_eq!(
            l(draw_text_lines("ab\r\ncd\ref\ngh", None, false, None, &m)),
            vec!["ab", "cd", "ef", "gh"]
        );
        assert_eq!(
            l(draw_text_lines("a\n\nb", None, false, None, &m)),
            vec!["a", "", "b"]
        );
        // DT_SINGLELINE garde une seule ligne.
        assert_eq!(draw_text_lines("ab\ncd", None, true, None, &m).len(), 1);
        // DT_WORDBREAK coupe aux espaces ; un mot trop long reste seul.
        assert_eq!(
            l(draw_text_lines(
                "un deux trois quatre",
                None,
                false,
                Some(90),
                &m
            )),
            vec!["un deux", "trois", "quatre"]
        );
        assert_eq!(
            l(draw_text_lines(
                "anticonstitutionnel ok",
                None,
                false,
                Some(50),
                &m
            )),
            vec!["anticonstitutionnel", "ok"]
        );
        // La lettre soulignée suit sa ligne.
        let v = draw_text_lines("Hôte\nPort", Some(5), false, None, &m);
        assert_eq!(v[0].1, None);
        assert_eq!(v[1].1, Some(0));
        // DT_CALCRECT : largeur de la ligne la plus longue, hauteur de toutes.
        const DT_CALCRECT: u32 = 0x0400;
        let mut une = Rect::default();
        let mut deux = Rect::default();
        let h1 = unsafe { draw_text_a(0, b"abcdef\0".as_ptr(), -1, &mut une, DT_CALCRECT) };
        let h2 = unsafe { draw_text_a(0, b"abcdef\r\nab\0".as_ptr(), -1, &mut deux, DT_CALCRECT) };
        assert_eq!(une.right, deux.right);
        assert_eq!(h2, 2 * h1);
        assert_eq!(deux.bottom, 2 * une.bottom);
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

    /// #1586 : une fenêtre HWND_MESSAGE et ses enfants restent sans fenêtre
    /// affichée, et leur parent rendu est nul.
    #[test]
    fn fenetre_message_seulement() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let make = |style: u32, parent: usize| {
                create_window_ex_w(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    style,
                    0,
                    0,
                    1,
                    1,
                    parent,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let msg = make(0, HWND_MESSAGE);
            assert_ne!(msg, 0);
            assert!(is_message_only(msg));
            assert_eq!(gui::window_parent(msg), Some(0));
            let child = make(WS_CHILD, msg);
            assert_ne!(child, 0);
            assert!(is_message_only(child));
            let top = make(0, 0);
            assert!(!is_message_only(top));
            // #1677 : rattachée à une vraie fenêtre (ToolStrip de WinForms
            // sorti de la fenêtre de parcage), elle redevient ordinaire,
            // avec ses enfants.
            let grand = make(WS_CHILD, child);
            assert!(is_message_only(grand));
            assert_eq!(set_parent(child, top), msg);
            assert!(!is_message_only(child));
            assert!(!is_message_only(grand));
            assert_eq!(gui::window_parent(child), Some(top));
            assert!(is_message_only(msg));
            destroy_window(grand);
            destroy_window(child);
            destroy_window(msg);
            destroy_window(top);
            assert!(!is_message_only(msg));
        }
    }

    /// #2106 : FindWindowEx reprend après `after` et respecte le parent ;
    /// une boucle d'énumération s'arrête, FindWindow ignore les enfants et
    /// les fenêtres message-only.
    #[test]
    fn find_window_ex_enumere_et_s_arrete() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = "winlux-2106\0".encode_utf16().collect();
            let make = |style: u32, parent: usize| {
                create_window_ex_w(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    style,
                    0,
                    0,
                    1,
                    1,
                    parent,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let a = make(0, 0);
            let b = make(0, 0);
            let child1 = make(WS_CHILD, a);
            let child2 = make(WS_CHILD, a);
            let msg = make(0, HWND_MESSAGE);
            assert!([a, b, child1, child2, msg].iter().all(|&h| h != 0));

            // Premier niveau : a et b, chacun une fois, puis NULL.
            let mut seen = Vec::new();
            let mut h = 0;
            loop {
                h = find_window_ex_w(0, h, class.as_ptr(), title.as_ptr());
                if h == 0 {
                    break;
                }
                assert!(seen.len() < 64, "boucle sans fin");
                seen.push(h);
            }
            seen.sort_unstable();
            let mut expected = vec![a, b];
            expected.sort_unstable();
            assert_eq!(seen, expected);
            // Bureau comme parent : pareil que NULL.
            assert!([a, b].contains(&find_window_ex_w(1, 0, class.as_ptr(), title.as_ptr())));

            // Enfants directs de a, dans l'ordre, puis NULL.
            let first = find_window_ex_w(a, 0, class.as_ptr(), std::ptr::null());
            let second = find_window_ex_w(a, first, class.as_ptr(), std::ptr::null());
            assert_eq!((first, second), (child1.min(child2), child1.max(child2)));
            assert_eq!(
                find_window_ex_w(a, second, class.as_ptr(), std::ptr::null()),
                0
            );
            assert_eq!(find_window_ex_w(b, 0, class.as_ptr(), std::ptr::null()), 0);
            // `after` qui n'est pas un enfant du parent : rien.
            assert_eq!(find_window_ex_w(a, b, class.as_ptr(), std::ptr::null()), 0);

            // Message-only : seulement sous HWND_MESSAGE.
            assert_eq!(
                find_window_ex_w(HWND_MESSAGE, 0, class.as_ptr(), title.as_ptr()),
                msg
            );
            assert_eq!(
                find_window_ex_w(HWND_MESSAGE, msg, class.as_ptr(), title.as_ptr()),
                0
            );

            // FindWindow : un des deux premiers niveaux, jamais un enfant.
            let found = find_window_w(class.as_ptr(), title.as_ptr());
            assert!([a, b].contains(&found), "{found:#x}");
            let ansi = find_window_a(c"STATIC".as_ptr().cast(), c"winlux-2106".as_ptr().cast());
            assert!([a, b].contains(&ansi), "{ansi:#x}");

            for h in [child1, child2, a, b, msg] {
                destroy_window(h);
            }
            assert_eq!(find_window_ex_w(0, 0, class.as_ptr(), title.as_ptr()), 0);
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
    fn placement_de_fenetre_deplace_et_affiche() {
        // #1159 : 7-Zip ne montre sa fenêtre que par SetWindowPlacement.
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0x00CF_0000,
                0,
                0,
                100,
                50,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(hwnd, 0);
            assert!(!gui::is_window_visible(hwnd));
            let mut wp = std::mem::zeroed::<WindowPlacement>();
            assert_eq!(get_window_placement(hwnd, &mut wp), 1);
            wp.show_cmd = 1; // SW_SHOWNORMAL
            wp.normal = Rect {
                left: 40,
                top: 30,
                right: 340,
                bottom: 230,
            };
            assert_eq!(set_window_placement(hwnd, &wp), 1);
            assert!(gui::is_window_visible(hwnd));
            let r = gui::window_rect(hwnd).unwrap();
            assert_eq!((r.width(), r.height()), (300, 200));
            assert_eq!(set_window_placement(hwnd, std::ptr::null()), 0);
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

    thread_local! {
        static CBT_VUS: std::cell::RefCell<Vec<(usize, u32, String)>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static CBT_REFUS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    extern "win64" fn proc_mfc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
        if msg == WM_NCCREATE {
            CBT_VUS.with(|v| v.borrow_mut().push((hwnd, msg, String::new())));
        }
        unsafe { def_window_proc_w(hwnd, msg, wp, lp) }
    }

    extern "win64" fn crochet_cbt(code: i32, wp: usize, lp: isize) -> isize {
        unsafe {
            if code == HCBT_CREATEWND {
                let info = &*(lp as *const CbtCreateWnd);
                let class = read_wide_z((*info.cs).class.cast());
                CBT_VUS.with(|v| v.borrow_mut().push((wp, 0, class)));
                if CBT_REFUS.with(|r| r.get()) {
                    return 1;
                }
                // Comme _AfxCbtFilterHook : sous-classement avant tout message.
                set_window_long_ptr_w(wp, -4, proc_mfc as *const () as isize);
            }
            call_next_hook_ex(0, code, wp, lp)
        }
    }

    unsafe fn read_wide_z(p: *const u16) -> String {
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }

    #[test]
    fn crochet_cbt_sous_classe_avant_wm_nccreate() {
        // MFC attache ses fenêtres dans HCBT_CREATEWND ; sa classe garde
        // DefWindowProc (Pythonwin, #1654).
        unsafe {
            let tid = crate::dlls::kernel32::get_current_thread_id();
            let h = set_windows_hook_ex_w(WH_CBT, crochet_cbt as *const () as usize, 0, tid);
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let creer = || {
                create_window_ex_w(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    50,
                    20,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let hwnd = creer();
            assert_ne!(hwnd, 0);
            let vus = CBT_VUS.with(|v| std::mem::take(&mut *v.borrow_mut()));
            assert_eq!(
                vus,
                vec![
                    (hwnd, 0, "STATIC".to_string()),
                    (hwnd, WM_NCCREATE, String::new())
                ]
            );
            destroy_window(hwnd);
            // Une valeur non nulle refuse la fenêtre.
            CBT_REFUS.with(|r| r.set(true));
            assert_eq!(creer(), 0);
            CBT_REFUS.with(|r| r.set(false));
            assert_eq!(unhook_windows_hook_ex(h), 1);
            CBT_VUS.with(|v| v.borrow_mut().clear());
            let hwnd = creer();
            assert_ne!(hwnd, 0);
            assert!(CBT_VUS.with(|v| v.borrow().is_empty()));
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
