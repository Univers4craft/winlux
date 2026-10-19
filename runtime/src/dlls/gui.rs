//! Noyau fenêtres et messages de Winlux.
//!
//! Ce module implémente la couche Win32 « objects + messages » : classes de
//! fenêtres, table des fenêtres (HWND), file de messages par thread et appel
//! du `WndProc` invité. Le rendu est délégué à un `GuiBackend` ; le backend
//! par défaut est *headless* (aucun affichage), de quoi faire tourner la
//! boucle de messages d'une application sans serveur graphique. Un backend
//! X11 branchera ici.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

// ---------- Constantes de messages ----------

pub const WM_CREATE: u32 = 0x0001;
pub const WM_DESTROY: u32 = 0x0002;
#[allow(dead_code)] // publié par DefWindowProc lors d'un déplacement
pub const WM_MOVE: u32 = 0x0003;
pub const WM_SIZE: u32 = 0x0005;
// Constantes réservées aux prochaines étapes (focus, clavier, effacement).
#[allow(dead_code)]
pub const WM_ACTIVATE: u32 = 0x0006;
#[allow(dead_code)]
pub const WM_SETFOCUS: u32 = 0x0007;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUIT: u32 = 0x0012;
#[allow(dead_code)]
pub const WM_ERASEBKGND: u32 = 0x0014;
#[allow(dead_code)]
pub const WM_SHOWWINDOW: u32 = 0x0018;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
#[allow(dead_code)]
pub const WM_CHAR: u32 = 0x0102;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;

#[allow(dead_code)]
pub const SW_HIDE: i32 = 0;
#[allow(dead_code)]
pub const SW_SHOWNORMAL: i32 = 1;
#[allow(dead_code)]
pub const SW_SHOW: i32 = 5;

pub const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;

// ---------- Thème « blob » violet-bleu ----------
///
/// Palette utilisée par tout le rendu de Winlux (fond de fenêtre, accent,
/// dégradés). Une teinte Windows `COLORREF` est encodée `0x00BBGGRR`.
pub mod theme {
    #![allow(dead_code)] // palette complète utilisée progressivement par le rendu
    /// Fond profond (indigo-violet sombre), le « blob » dans son ensemble.
    pub const BG_DEEP: u32 = 0x002A_1B3D;      // #3D1B2A -> indigo sombre
    pub const BG_SURFACE: u32 = 0x003A_2555;    // surface de fenêtre
    pub const BG_ELEVATED: u32 = 0x004B_2F72;   // surfaces surélevées
    /// Accent principal (violet).
    pub const ACCENT: u32 = 0x00B4_4BEE;        // #EE4BB4 -> magenta-violet
    pub const ACCENT_VIOLET: u32 = 0x00C8_6BF0; // #F06BC8 -> violet vif
    /// Accent secondaire (bleu).
    pub const ACCENT_BLUE: u32 = 0x00E9_6BB5;   // #B56BE9 -> bleu-violet
    pub const ACCENT_CYAN: u32 = 0x00F0_9A6B;   // #6B9AF0 -> bleu clair
    /// Texte.
    pub const TEXT: u32 = 0x00F4_EEFB;          // presque blanc teinté
    pub const TEXT_DIM: u32 = 0x00B9_A8CC;      // texte atténué
    /// Sélection / survol.
    pub const SELECTION: u32 = 0x009A_4BE0;
    pub const HOVER: u32 = 0x007A_3BC0;
    /// Arrête du dégradé « blob » (haut → bas), du violet au bleu.
    pub const GRADIENT_TOP: u32 = 0x00D8_4BF0;  // #F04BD8
    pub const GRADIENT_BOTTOM: u32 = 0x00C8_6B4B; // #4B6BC8 -> bleu

    /// Convertit un COLORREF Win32 (0x00BBGGRR) en entier RGB (0x00RRGGBB).
    pub fn colorref_to_rgb(c: u32) -> u32 {
        let r = c & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = (c >> 16) & 0xFF;
        (r << 16) | (g << 8) | b
    }

    /// Interpole linéairement deux COLORREF (composante par composante),
    /// utilisé pour les dégradés de type « blob ».
    pub fn lerp_colorref(a: u32, b: u32, t: f32) -> u32 {
        let t = t.clamp(0.0, 1.0);
        let ch = |shift: u8| {
            let x = ((a >> shift) & 0xFF) as f32;
            let y = ((b >> shift) & 0xFF) as f32;
            (x + (y - x) * t) as u32
        };
        (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }
}


// ---------- Structures Win32 (x86-64) ----------

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn from_size(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { left: x, top: y, right: x + w, bottom: y + h }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Msg {
    pub hwnd: usize,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: Point,
    pub l_private: u32,
}

#[repr(C)]
pub struct WndClassA {
    pub style: u32,
    pub wnd_proc: usize,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: usize,
    pub icon: usize,
    pub cursor: usize,
    pub background: usize,
    pub menu_name: *const u8,
    pub class_name: *const u8,
}

#[repr(C)]
pub struct WndClassExA {
    pub size: u32,
    pub style: u32,
    pub wnd_proc: usize,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: usize,
    pub icon: usize,
    pub cursor: usize,
    pub background: usize,
    pub menu_name: *const u8,
    pub class_name: *const u8,
    pub icon_sm: usize,
}

#[repr(C)]
pub struct PaintStruct {
    pub hdc: usize,
    pub erase: i32,
    pub paint: Rect,
    pub restore: i32,
    pub inc_update: i32,
    pub reserved: [u8; 32],
}

#[repr(C)]
pub struct CreateStructA {
    pub create_params: *mut std::ffi::c_void,
    pub instance: usize,
    pub menu: usize,
    pub parent: usize,
    pub cy: i32,
    pub cx: i32,
    pub y: i32,
    pub x: i32,
    pub style: i32,
    pub name: *const u8,
    pub class: *const u8,
    pub ex_style: u32,
}

// ---------- État interne ----------

pub struct ClassInfo {
    pub wnd_proc: usize,
    pub background: usize,
    pub instance: usize,
}

pub struct Window {
    #[allow(dead_code)] // conservés pour les prochaines étapes (style de classe, fond)
    pub class: String,
    pub wnd_proc: usize,
    #[allow(dead_code)]
    pub background: usize,
    pub title: String,
    pub rect: Rect,
    pub visible: bool,
    pub parent: usize,
    pub owner: ThreadId,
    pub needs_paint: bool,
    #[allow(dead_code)] // style WS_* utile pour GetWindowLong et les futures branches
    pub style: u32,
    #[allow(dead_code)]
    pub ex_style: u32,
}

impl Window {
    /// Rectangle client : la zone intérieure, sans décorations.
    pub fn client_rect(&self) -> Rect {
        Rect::from_size(0, 0, self.rect.width(), self.rect.height())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn classes() -> &'static Mutex<HashMap<String, ClassInfo>> {
    static C: OnceLock<Mutex<HashMap<String, ClassInfo>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn windows() -> &'static Mutex<HashMap<usize, Window>> {
    static W: OnceLock<Mutex<HashMap<usize, Window>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_hwnd() -> &'static std::sync::atomic::AtomicUsize {
    static N: OnceLock<std::sync::atomic::AtomicUsize> = OnceLock::new();
    N.get_or_init(|| std::sync::atomic::AtomicUsize::new(0x0004_0000))
}

pub fn alloc_hwnd() -> usize {
    next_hwnd().fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// ---------- Files de messages par thread ----------

pub struct Queue {
    pub items: Mutex<VecDeque<Msg>>,
    pub changed: Condvar,
}

fn queues() -> &'static Mutex<HashMap<ThreadId, Arc<Queue>>> {
    static Q: OnceLock<Mutex<HashMap<ThreadId, Arc<Queue>>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn current_queue() -> Arc<Queue> {
    let tid = std::thread::current().id();
    let mut map = lock(queues());
    if let Some(q) = map.get(&tid) {
        return q.clone();
    }
    let q = Arc::new(Queue { items: Mutex::new(VecDeque::new()), changed: Condvar::new() });
    map.insert(tid, q.clone());
    q
}

fn queue_for(tid: ThreadId) -> Arc<Queue> {
    let mut map = lock(queues());
    if let Some(q) = map.get(&tid) {
        return q.clone();
    }
    let q = Arc::new(Queue { items: Mutex::new(VecDeque::new()), changed: Condvar::new() });
    map.insert(tid, q.clone());
    q
}

/// Publie un message dans la file du thread cible. Renvoie faux si la fenêtre
/// n'existe plus (comportement Win32 de `PostMessage`).
pub fn post_to_thread(tid: ThreadId, msg: Msg) -> bool {
    let q = queue_for(tid);
    lock(&q.items).push_back(msg);
    q.changed.notify_all();
    true
}

/// Publie un message pour une fenêtre donnée (son thread propriétaire).
pub fn post_to_window(hwnd: usize, msg: Msg) -> bool {
    let owner = { lock(windows()).get(&hwnd).map(|w| w.owner) };
    match owner {
        Some(tid) => post_to_thread(tid, msg),
        None => false,
    }
}

// ---------- Appel du WndProc invité ----------

/// Appelle le `WndProc` de la classe (code invité, convention win64).
///
/// # Sécurité
/// `wnd_proc` doit être une adresse de fonction invité valide. L'appel se fait
/// sans verrou tenu pour permettre les réentrances (le wndproc peut rappeler des API).
pub unsafe fn call_wndproc(wnd_proc: usize, hwnd: usize, msg: u32, wparam: usize, lparam: isize) -> isize {
    let f: extern "win64" fn(usize, u32, usize, isize) -> isize = std::mem::transmute(wnd_proc);
    f(hwnd, msg, wparam, lparam)
}

// ---------- Backend ----------

/// Événement traduit par le backend, à publier dans la file du thread cible.
pub struct PendingEvent {
    pub hwnd: usize,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
}

/// Rendu et événements hôte. `headless` n'affiche rien ; X11 traduit les
/// événements du serveur en messages Win32.
pub trait GuiBackend: Send {
    /// Traduit les événements hôte en messages Win32 (vidés à chaque boucle).
    fn pump(&mut self) -> Vec<PendingEvent> {
        Vec::new()
    }
    fn create_window(&mut self, _hwnd: usize, _title: &str, _rect: Rect) {}
    fn destroy_window(&mut self, _hwnd: usize) {}
    fn show_window(&mut self, _hwnd: usize, _show: bool) {}
    fn set_title(&mut self, _hwnd: usize, _title: &str) {}
    fn invalidate(&mut self, _hwnd: usize) {}
    /// Attend des événements hôte pendant `timeout_ms`; renvoie `true` si le
    /// backend peut en produire (X11) et `false` s'il est headless, auquel cas
    /// une boucle `GetMessage` vide doit se terminer.
    fn wait(&mut self, _timeout_ms: u32) -> bool {
        false
    }
}

struct HeadlessBackend;

impl GuiBackend for HeadlessBackend {}

/// Construit le backend : X11 si `DISPLAY` est présent et qu'on n'a pas forcé
/// `WINLUX_GUI=headless`; sinon headless. Retombe sur headless si la connexion
/// échoue.
fn build_backend() -> Box<dyn GuiBackend> {
    let force_headless = std::env::var("WINLUX_GUI").map(|v| v == "headless").unwrap_or(false);
    if !force_headless && std::env::var_os("DISPLAY").is_some() {
        if let Some(b) = crate::dlls::gui_x11::X11Backend::connect() {
            crate::trace!("backend graphique : X11");
            return Box::new(b);
        }
        crate::trace!("connexion X11 impossible, bascule headless");
    }
    Box::new(HeadlessBackend)
}

fn backend() -> &'static Mutex<Box<dyn GuiBackend>> {
    static B: OnceLock<Mutex<Box<dyn GuiBackend>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(build_backend()))
}

/// Force l'initialisation du backend (au premier appel GUI).
#[allow(dead_code)]
pub fn init() {
    let _ = backend();
}

fn with_backend<F: FnOnce(&mut dyn GuiBackend)>(f: F) {
    let mut b = lock(backend());
    f(b.as_mut());
}

/// Interroge le backend et publie les messages traduits dans les files cibles.
pub fn pump_backend() {
    let events = {
        let mut b = lock(backend());
        b.pump()
    };
    for e in events {
        let m = Msg {
            hwnd: e.hwnd,
            message: e.message,
            wparam: e.wparam,
            lparam: e.lparam,
            time: 0,
            pt: Point::default(),
            l_private: 0,
        };
        if e.message == WM_PAINT {
            request_paint(e.hwnd);
        } else {
            post_to_window(e.hwnd, m);
        }
    }
}

/// Attend des événements hôte (au plus `timeout_ms`). Renvoie `false` si le
/// backend est headless : une boucle `GetMessage` vide doit alors se terminer.
pub fn wait_backend(timeout_ms: u32) -> bool {
    let mut b = lock(backend());
    b.wait(timeout_ms)
}

// ---------- Opérations sur les fenêtres ----------

/// Nombre de fenêtres actuellement enregistrées (diagnostic, shell).
#[allow(dead_code)]
pub fn window_count() -> usize {
    lock(windows()).len()
}

/// Vrai si au moins une fenêtre est visible (sert à distinguer une boucle de
/// messages active d'une fin de programme en backend headless).
#[allow(dead_code)]
pub fn any_visible_window() -> bool {
    lock(windows()).values().any(|w| w.visible)
}

/// Marque une fenêtre comme nécessitant un `WM_PAINT` et notifie son thread.
pub fn request_paint(hwnd: usize) {
    let owner = {
        let mut map = lock(windows());
        match map.get_mut(&hwnd) {
            Some(w) => {
                w.needs_paint = true;
                Some(w.owner)
            }
            None => None,
        }
    };
    if let Some(tid) = owner {
        post_to_thread(
            tid,
            Msg { hwnd, message: WM_PAINT, wparam: 0, lparam: 0, time: 0, pt: Point::default(), l_private: 0 },
        );
    }
}

pub fn destroy_window(hwnd: usize) -> bool {
    // Capture du wndproc et du parent avant retrait de la table.
    let (wnd_proc, parent) = {
        let mut map = lock(windows());
        match map.remove(&hwnd) {
            Some(w) => (Some(w.wnd_proc), w.parent),
            None => (None, 0),
        }
    };
    // WM_DESTROY synchrone auprès du wndproc de la fenêtre détruite.
    if let Some(wp) = wnd_proc {
        unsafe { call_wndproc(wp, hwnd, WM_DESTROY, 0, 0) };
    }
    with_backend(|b| b.destroy_window(hwnd));
    // Les enfants sont détruits récursivement.
    let children: Vec<usize> = lock(windows())
        .iter()
        .filter(|(_, w)| w.parent == hwnd)
        .map(|(&h, _)| h)
        .collect();
    for c in children {
        destroy_window(c);
    }
    let _ = parent;
    true
}

pub fn register_class(name: &str, info: ClassInfo) -> bool {
    let mut map = lock(classes());
    if map.contains_key(name) {
        return false;
    }
    map.insert(name.to_string(), info);
    true
}

pub fn lookup_class(name: &str) -> Option<ClassInfo> {
    lock(classes()).get(name).map(|c| ClassInfo { wnd_proc: c.wnd_proc, background: c.background, instance: c.instance })
}

// ---------- Accès utilisés par user32 ----------

/// Insère une fenêtre déjà construite dans la table.
pub fn insert_window(hwnd: usize, w: Window) {
    lock(windows()).insert(hwnd, w);
}

/// Expose le backend pour les wrappers (user32/gdi32).
pub fn with_backend_public<F: FnOnce(&mut dyn GuiBackend)>(f: F) {
    with_backend(f);
}

/// wndproc d'une fenêtre, si elle existe.
pub fn wnd_proc_of(hwnd: usize) -> Option<usize> {
    lock(windows()).get(&hwnd).map(|w| w.wnd_proc)
}

pub fn window_rect(hwnd: usize) -> Option<Rect> {
    lock(windows()).get(&hwnd).map(|w| w.rect)
}

pub fn window_client_rect(hwnd: usize) -> Option<Rect> {
    lock(windows()).get(&hwnd).map(|w| w.client_rect())
}

pub fn title_of(hwnd: usize) -> Option<String> {
    lock(windows()).get(&hwnd).map(|w| w.title.clone())
}

pub fn set_title(hwnd: usize, title: &str) -> bool {
    let mut map = lock(windows());
    match map.get_mut(&hwnd) {
        Some(w) => {
            w.title = title.to_string();
            true
        }
        None => false,
    }
}

/// Change la visibilité ; renvoie l'état précédent.
pub fn set_visible(hwnd: usize, visible: bool) -> bool {
    let mut map = lock(windows());
    match map.get_mut(&hwnd) {
        Some(w) => {
            let was = w.visible;
            w.visible = visible;
            was
        }
        None => false,
    }
}

pub fn first_window() -> Option<usize> {
    lock(windows()).keys().next().copied()
}

/// Recycle un HDC factice ; en headless on ne rend pas, donc une valeur stable.
pub fn begin_paint(hwnd: usize) -> Option<(usize, Rect)> {
    let mut map = lock(windows());
    match map.get_mut(&hwnd) {
        Some(w) => {
            w.needs_paint = false;
            Some((hwnd, w.client_rect()))
        }
        None => None,
    }
}

pub fn end_paint(_hwnd: usize) -> bool {
    true
}

// ---------- File : prise / dépôt ----------

/// Retire et renvoie le premier message correspondant au filtre.
pub fn take_message(q: &Queue, hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    let mut items = lock(&q.items);
    let idx = items.iter().position(|m| match_filter(m, hwnd, min, max));
    idx.map(|i| items.remove(i).unwrap())
}

/// Remet un message en tête de file (pour WM_QUIT non retiré).
pub fn prepend_message(q: &Queue, m: Msg) {
    lock(&q.items).push_front(m);
}

/// Regarde sans retirer.
pub fn peek_message(q: &Queue, hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    let items = lock(&q.items);
    items.iter().find(|m| match_filter(m, hwnd, min, max)).copied()
}

fn match_filter(m: &Msg, hwnd: usize, min: u32, max: u32) -> bool {
    // hwnd = 0 : toutes fenêtres ; min/max = 0 : tous messages.
    if hwnd != 0 && m.hwnd != hwnd {
        return false;
    }
    if min == 0 && max == 0 {
        return true;
    }
    m.message >= min && m.message <= max
}

/// Publie un WM_QUIT dans la file courante (fin de boucle).
pub fn post_quit(q: &Queue, code: i32) {
    lock(&q.items).push_back(Msg {
        hwnd: 0,
        message: WM_QUIT,
        wparam: code as usize,
        lparam: 0,
        time: 0,
        pt: Point::default(),
        l_private: 0,
    });
    q.changed.notify_all();
}

// ---------- Défaut ----------

/// `DefWindowProc` minimal : gère WM_CLOSE et laisse le reste au programme.
pub fn def_window_proc(hwnd: usize, message: u32, _wparam: usize, _lparam: isize) -> isize {
    match message {
        WM_CLOSE => {
            destroy_window(hwnd);
            0
        }
        WM_ERASEBKGND => 1, // on « efface » en peignant nous-mêmes
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_messages_par_thread_est_isolee() {
        let q = current_queue();
        lock(&q.items).clear();
        post_to_thread(std::thread::current().id(), Msg {
            hwnd: 1,
            message: WM_PAINT,
            wparam: 0,
            lparam: 0,
            time: 0,
            pt: Point::default(),
            l_private: 0,
        });
        assert_eq!(lock(&q.items).len(), 1);
        lock(&q.items).clear();
    }

    #[test]
    fn les_rects_calculent_leur_taille() {
        let r = Rect::from_size(10, 20, 100, 50);
        assert_eq!(r.width(), 100);
        assert_eq!(r.height(), 50);
        assert_eq!(r.right, 110);
        assert_eq!(r.bottom, 70);
    }
}
