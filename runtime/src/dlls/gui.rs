//! Noyau fenêtres et messages de Winlux.
//!
//! Ce module implémente la couche Win32 « objects + messages » : classes de
//! fenêtres, table des fenêtres (HWND), file de messages par thread et appel
//! du `WndProc` invité. Le rendu est délégué à un `GuiBackend` ; le backend
//! par défaut est *headless* (aucun affichage), de quoi faire tourner la
//! boucle de messages d'une application sans serveur graphique. Un backend
//! X11 branchera ici.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

pub const MAX_BITMAP_PIXELS: usize = 16 * 1024 * 1024;

pub fn bilinear_colorref(pixels: &[u32], width: usize, height: usize, x: f32, y: f32) -> u32 {
    let x = x.clamp(0.0, width.saturating_sub(1) as f32);
    let y = y.clamp(0.0, height.saturating_sub(1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let channel = |shift: u32| {
        let value = |px: u32| ((px >> shift) & 0xff) as f32;
        let top = value(pixels[y0 * width + x0]) * (1.0 - fx) + value(pixels[y0 * width + x1]) * fx;
        let bottom =
            value(pixels[y1 * width + x0]) * (1.0 - fx) + value(pixels[y1 * width + x1]) * fx;
        (top * (1.0 - fy) + bottom * fy).round() as u32
    };
    channel(0) | (channel(8) << 8) | (channel(16) << 16)
}

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
pub const WM_KILLFOCUS: u32 = 0x0008;
pub const WM_ACTIVATEAPP: u32 = 0x001C;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUIT: u32 = 0x0012;
#[allow(dead_code)]
pub const WM_ERASEBKGND: u32 = 0x0014;
#[allow(dead_code)]
pub const WM_SHOWWINDOW: u32 = 0x0018;
pub const WM_NCACTIVATE: u32 = 0x0086;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
#[allow(dead_code)]
pub const WM_CHAR: u32 = 0x0102;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONDOWN: u32 = 0x0204;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MBUTTONDOWN: u32 = 0x0207;
pub const WM_MBUTTONUP: u32 = 0x0208;
pub const WM_MOUSEWHEEL: u32 = 0x020A;
pub const WM_MOUSEHWHEEL: u32 = 0x020E;

#[allow(dead_code)]
pub const SW_HIDE: i32 = 0;
#[allow(dead_code)]
pub const SW_SHOWNORMAL: i32 = 1;
#[allow(dead_code)]
pub const SW_SHOW: i32 = 5;

pub const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;

// ---------- Thème Winlux clair inspiré de Windows 11 ----------
///
/// Palette utilisée par tout le rendu de Winlux (fond de fenêtre, accent,
/// dégradés). Une teinte Windows `COLORREF` est encodée `0x00BBGGRR`.
pub mod theme {
    #![allow(dead_code)] // palette complète utilisée progressivement par le rendu
    /// Surfaces claires proches de la palette Windows 11, sans ressource Microsoft.
    pub const BG_DEEP: u32 = 0x00F3_F3F3; // #F3F3F3
    pub const BG_SURFACE: u32 = 0x00FF_FFFF; // #FFFFFF
    pub const BG_ELEVATED: u32 = 0x00FA_FAFA; // #FAFAFA
    /// Accent principal bleu.
    pub const ACCENT: u32 = 0x00F0_3D6A; // #6A3DF0, violet Prisme
    pub const ACCENT_VIOLET: u32 = 0x00E6_8A74; // #748AE6
    pub const ACCENT_BLUE: u32 = 0x00E6_8A74; // #748AE6
    pub const ACCENT_CYAN: u32 = 0x00F9_9AB1; // #B19AF9 (thème sombre)
    /// Texte.
    pub const TEXT: u32 = 0x001F_1F1F; // #1F1F1F
    pub const TEXT_DIM: u32 = 0x0060_6060; // #606060
    /// Sélection / survol.
    pub const SELECTION: u32 = 0x00F5_E7D6; // #D6E7F5
    pub const HOVER: u32 = 0x00ED_EDED; // #EDEDED
    /// Dégradé très léger du fond d'application.
    pub const GRADIENT_TOP: u32 = 0x00FF_FBF7; // #F7FBFF
    pub const GRADIENT_BOTTOM: u32 = BG_DEEP;

    /// Sous-ensemble de la palette `GetSysColor`, harmonisé avec le thème.
    pub fn system_color(index: i32) -> Option<u32> {
        Some(match index {
            0 => 0x00C0_C0C0,  // COLOR_SCROLLBAR
            1 => BG_DEEP,      // COLOR_BACKGROUND / DESKTOP
            2 => GRADIENT_TOP, // COLOR_ACTIVECAPTION
            3 => BG_ELEVATED,  // COLOR_INACTIVECAPTION
            4 => BG_SURFACE,   // COLOR_MENU
            5 => BG_SURFACE,   // COLOR_WINDOW
            6 => BG_DEEP,      // COLOR_WINDOWFRAME
            7 | 8 | 9 | 14 | 18 | 23 => TEXT,
            10 => ACCENT,           // COLOR_ACTIVEBORDER
            11 => BG_ELEVATED,      // COLOR_INACTIVEBORDER
            12 => BG_DEEP,          // COLOR_APPWORKSPACE
            13 => ACCENT,           // COLOR_HIGHLIGHT
            15 => BG_ELEVATED,      // COLOR_BTNFACE
            16 | 21 => BG_DEEP,     // COLOR_BTNSHADOW / 3DDKSHADOW
            17 | 19 => TEXT_DIM,    // COLOR_GRAYTEXT / INACTIVECAPTIONTEXT
            20 | 22 => 0x00FF_FFFF, // COLOR_BTNHIGHLIGHT / 3DLIGHT
            24 => BG_ELEVATED,      // COLOR_INFOBK
            26 => ACCENT_VIOLET,    // COLOR_HOTLIGHT
            27 => GRADIENT_BOTTOM,  // COLOR_GRADIENTACTIVECAPTION
            28 => BG_ELEVATED,      // COLOR_GRADIENTINACTIVECAPTION
            29 => HOVER,            // COLOR_MENUHILIGHT
            30 => BG_SURFACE,       // COLOR_MENUBAR
            _ => return None,
        })
    }

    /// Convertit un COLORREF Win32 (0x00BBGGRR) en entier RGB (0x00RRGGBB).
    pub fn colorref_to_rgb(c: u32) -> u32 {
        let r = c & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = (c >> 16) & 0xFF;
        (r << 16) | (g << 8) | b
    }

    /// Interpole linéairement deux COLORREF (composante par composante),
    /// utilisé pour le dégradé discret du fond d'application.
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

/// Un écran physique, en coordonnées logiques de l'écran X.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub rect: Rect,
    pub primary: bool,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn from_size(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
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

impl Msg {
    /// Message neutre : horodatage, position et donnée privée à zéro.
    pub fn new(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> Msg {
        Msg {
            hwnd,
            message,
            wparam,
            lparam,
            time: 0,
            pt: Point::default(),
            l_private: 0,
        }
    }
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
    /// Un `WM_PAINT` est déjà en file d'attente (coalescence).
    pub paint_queued: bool,
    /// Commandes GDI accumulées depuis le dernier `EndPaint`.
    pub pending: Vec<DrawCmd>,
    /// Entre `BeginPaint` et `EndPaint` : les dessins faits par un autre DC
    /// (`GetDC`) attendent `EndPaint` pour partir avec la peinture complète.
    pub painting: bool,
    /// Hauteur de la barre de menus (0 sans menu) : elle est au-dessus de
    /// la zone client, dans le rectangle de la fenêtre.
    pub menu_bar: i32,
}

impl Window {
    /// Rectangle client : la zone intérieure, sans décorations.
    pub fn client_rect(&self) -> Rect {
        Rect::from_size(
            0,
            0,
            self.rect.width(),
            (self.rect.height() - self.menu_bar).max(0),
        )
    }
}

use super::lock;

fn classes() -> &'static Mutex<HashMap<String, ClassInfo>> {
    static C: OnceLock<Mutex<HashMap<String, ClassInfo>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn windows() -> &'static Mutex<HashMap<usize, Window>> {
    static W: OnceLock<Mutex<HashMap<usize, Window>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Nouveau HWND : unique entre les processus du compte, comme sous Windows
/// où une fenêtre vaut pour tous (#1228).
pub fn alloc_hwnd() -> usize {
    super::gui_shared::alloc_hwnd()
}

/// Fenêtre d'un autre processus du compte (processus GPU de Chromium qui
/// reçoit le HWND du navigateur, #1228).
pub fn foreign_window(hwnd: usize) -> Option<super::gui_shared::Foreign> {
    if lock(windows()).contains_key(&hwnd) {
        return None;
    }
    super::gui_shared::foreign(hwnd)
}

// ---------- Files de messages par thread ----------

pub struct Queue {
    pub items: Mutex<VecDeque<Msg>>,
    pub changed: Condvar,
    /// Descripteur d'éveil (`eventfd`) : signalé à chaque dépôt pour sortir
    /// un `GetMessage` bloqué dans `poll` sur la connexion X11.
    wake: WakeFd,
}

/// `eventfd` non bloquant servant d'éveil à une file de messages.
struct WakeFd(i32);

impl WakeFd {
    fn new() -> WakeFd {
        // SAFETY : appel système sans pointeur ; -1 signifie « pas d'éveil ».
        WakeFd(unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) })
    }

    fn signal(&self) {
        if self.0 >= 0 {
            let one: u64 = 1;
            // SAFETY : écriture de 8 octets depuis une variable locale.
            unsafe { libc::write(self.0, (&one as *const u64).cast(), 8) };
        }
    }

    fn drain(&self) {
        if self.0 >= 0 {
            let mut v: u64 = 0;
            // SAFETY : lecture de 8 octets dans une variable locale.
            unsafe { libc::read(self.0, (&mut v as *mut u64).cast(), 8) };
        }
    }
}

impl Drop for WakeFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            // SAFETY : descripteur ouvert par `new`, fermé une seule fois.
            unsafe { libc::close(self.0) };
        }
    }
}

fn new_queue() -> Arc<Queue> {
    Arc::new(Queue {
        items: Mutex::new(VecDeque::new()),
        changed: Condvar::new(),
        wake: WakeFd::new(),
    })
}

/// Réveille le thread propriétaire de la file (dépôt, minuteur, peinture).
pub fn wake_thread(tid: ThreadId) {
    queue_for(tid).wake.signal();
}

fn queues() -> &'static Mutex<HashMap<ThreadId, Arc<Queue>>> {
    static Q: OnceLock<Mutex<HashMap<ThreadId, Arc<Queue>>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Identifiant Win32 d'un thread → son identifiant Rust, pour
/// `PostThreadMessage` : un thread y figure dès qu'il a une file de messages.
fn win32_threads() -> &'static Mutex<HashMap<u32, ThreadId>> {
    static T: OnceLock<Mutex<HashMap<u32, ThreadId>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Thread Rust portant la file de messages du thread Win32 `tid`.
pub fn thread_of_win32(tid: u32) -> Option<ThreadId> {
    lock(win32_threads()).get(&tid).copied()
}

/// Vrai si le thread appelant a déjà une file de messages (IsGUIThread).
pub fn current_thread_has_queue() -> bool {
    lock(queues()).contains_key(&std::thread::current().id())
}

pub fn current_queue() -> Arc<Queue> {
    let tid = std::thread::current().id();
    thread_local!(static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if !REGISTERED.with(|r| r.replace(true)) {
        lock(win32_threads()).insert(crate::dlls::kernel32::get_current_thread_id(), tid);
    }
    let mut map = lock(queues());
    if let Some(q) = map.get(&tid) {
        return q.clone();
    }
    let q = new_queue();
    map.insert(tid, q.clone());
    q
}

fn queue_for(tid: ThreadId) -> Arc<Queue> {
    let mut map = lock(queues());
    if let Some(q) = map.get(&tid) {
        return q.clone();
    }
    let q = new_queue();
    map.insert(tid, q.clone());
    q
}

/// Publie un message dans la file du thread cible. Renvoie faux si la fenêtre
/// n'existe plus (comportement Win32 de `PostMessage`).
pub fn post_to_thread(tid: ThreadId, msg: Msg) -> bool {
    let q = queue_for(tid);
    lock(&q.items).push_back(msg);
    q.changed.notify_all();
    q.wake.signal();
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

// ---------- Focus clavier ----------

fn focused_windows() -> &'static Mutex<HashMap<ThreadId, usize>> {
    static F: OnceLock<Mutex<HashMap<ThreadId, usize>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

/// HWND ayant le focus sur le thread appelant, ou zéro.
pub fn focused_window() -> usize {
    lock(focused_windows())
        .get(&std::thread::current().id())
        .copied()
        .unwrap_or(0)
}

/// Change le focus du thread appelant ; `None` indique une HWND absente ou
/// appartenant à un autre thread, sinon renvoie l'ancien focus (zéro possible).
pub fn set_focus(hwnd: usize) -> Option<usize> {
    let tid = std::thread::current().id();
    if hwnd != 0 && window_owner(hwnd).as_ref() != Some(&tid) {
        return None;
    }
    let old = {
        let mut map = lock(focused_windows());
        let old = map.get(&tid).copied().unwrap_or(0);
        if hwnd == 0 {
            map.remove(&tid);
        } else {
            map.insert(tid, hwnd);
        }
        old
    };
    if hwnd != 0 {
        activate_top_level(hwnd);
    }
    Some(old)
}

/// Monte au sommet de l'ordre Z la fenêtre de premier niveau qui contient `hwnd`.
fn activate_top_level(hwnd: usize) {
    let top = {
        let map = lock(windows());
        let mut h = hwnd;
        while let Some(parent) = map.get(&h).map(|w| w.parent).filter(|&p| p != 0) {
            h = parent;
        }
        map.contains_key(&h).then_some(h)
    };
    if let Some(top) = top {
        bring_to_top(top);
    }
}

/// Applique un changement de focus reçu du backend ; `None` signifie que
/// l'événement ne change pas l'état connu (ou que la fenêtre est détruite).
pub fn set_focus_from_backend(hwnd: usize) -> Option<usize> {
    let owner = window_owner(hwnd)?;
    let old = {
        let mut map = lock(focused_windows());
        let old = map.get(&owner).copied().unwrap_or(0);
        if old == hwnd {
            return None;
        }
        map.insert(owner, hwnd);
        old
    };
    activate_top_level(hwnd);
    Some(old)
}

/// Efface le focus reçu d'un backend uniquement si cette HWND le détient encore.
pub fn clear_focus_from_backend(hwnd: usize) -> bool {
    let Some(owner) = window_owner(hwnd) else {
        return false;
    };
    let mut map = lock(focused_windows());
    if map.get(&owner).copied() == Some(hwnd) {
        map.remove(&owner);
        true
    } else {
        false
    }
}

fn clear_focus_for_destroy(hwnd: usize) -> bool {
    let mut map = lock(focused_windows());
    let was_focused = map.values().any(|&focused| focused == hwnd);
    map.retain(|_, focused| *focused != hwnd);
    was_focused
}

// ---------- Clavier ----------

pub const VK_BACK: usize = 0x08;
pub const VK_TAB: usize = 0x09;
pub const VK_RETURN: usize = 0x0D;
pub const VK_ESCAPE: usize = 0x1B;
pub const VK_SPACE: usize = 0x20;
pub const VK_CAPITAL: usize = 0x14;
pub const VK_NUMLOCK: usize = 0x90;
pub const VK_SCROLL: usize = 0x91;

const KEY_STATE_COUNT: usize = 256;

fn thread_keyboard_states() -> &'static Mutex<HashMap<ThreadId, [u8; KEY_STATE_COUNT]>> {
    static K: OnceLock<Mutex<HashMap<ThreadId, [u8; KEY_STATE_COUNT]>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
struct AsyncKeyboardState {
    keys: [u8; KEY_STATE_COUNT],
    pressed_since_query: [bool; KEY_STATE_COUNT],
}

impl Default for AsyncKeyboardState {
    fn default() -> Self {
        Self {
            keys: [0; KEY_STATE_COUNT],
            pressed_since_query: [false; KEY_STATE_COUNT],
        }
    }
}

fn async_keyboard_state() -> &'static Mutex<AsyncKeyboardState> {
    static K: OnceLock<Mutex<AsyncKeyboardState>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(AsyncKeyboardState::default()))
}

fn is_toggle_key(vk: usize) -> bool {
    matches!(vk, VK_CAPITAL | VK_NUMLOCK | VK_SCROLL)
}

fn transition_key_state(byte: &mut u8, vk: usize, down: bool) {
    if down {
        if *byte & 0x80 == 0 && is_toggle_key(vk) {
            *byte ^= 0x01;
        }
        *byte |= 0x80;
    } else {
        *byte &= !0x80;
    }
}

/// Met à jour l'état Win32 du thread quand il retire WM_KEYDOWN/WM_KEYUP.
/// Ainsi `GetKeyState` reflète l'événement actuellement traité, même si X11 a
/// déjà livré la pression et le relâchement dans un seul tour de boucle.
/// Les boutons de la souris suivent la même règle (VK_LBUTTON…) : Qt (VLC)
/// lit l'état des boutons au moment du clic et ignorait sinon tout clic.
pub fn observe_key_message(msg: &Msg) {
    // Messages souris : MK_* de wParam donne l'état des boutons (VK_LBUTTON…)
    // que WPF relit par GetKeyState pendant WM_LBUTTONDOWN (#268).
    if (0x0200..=0x020D).contains(&msg.message) && msg.message != 0x020A {
        const BUTTONS: [(usize, usize); 5] = [
            (0x01, 0x01), // MK_LBUTTON → VK_LBUTTON
            (0x02, 0x02), // MK_RBUTTON → VK_RBUTTON
            (0x10, 0x04), // MK_MBUTTON → VK_MBUTTON
            (0x20, 0x05), // MK_XBUTTON1 → VK_XBUTTON1
            (0x40, 0x06), // MK_XBUTTON2 → VK_XBUTTON2
        ];
        for (mk, vk) in BUTTONS {
            let down = msg.wparam & mk != 0;
            if (get_key_state(vk) < 0) != down {
                update_thread_key_state(vk, down);
            }
        }
        // Le bouton du message lui-même, même si wParam ne le porte pas.
        if let Some((vk, down)) = mouse_button_transition(msg.message, msg.wparam) {
            update_thread_key_state(vk, down);
        }
        return;
    }
    if msg.wparam >= KEY_STATE_COUNT {
        return;
    }
    match msg.message {
        WM_KEYDOWN | 0x0104 => update_thread_key_state(msg.wparam, true),
        WM_KEYUP | 0x0105 => update_thread_key_state(msg.wparam, false),
        _ => return,
    }
    // Windows tient aussi l'état du côté gauche ou droit de Maj, Ctrl et
    // Alt : Qt lit GetKeyState(VK_LCONTROL…) pour ses raccourcis (Ctrl+O).
    if let Some(side) = sided_modifier(msg.wparam, msg.lparam) {
        update_thread_key_state(side, matches!(msg.message, WM_KEYDOWN | 0x0104));
    }
}

/// VK_LSHIFT/VK_RSHIFT, VK_LCONTROL/VK_RCONTROL ou VK_LMENU/VK_RMENU d'un
/// message clavier portant VK_SHIFT, VK_CONTROL ou VK_MENU. Le côté se lit
/// dans lParam : code de balayage 0x36 pour Maj droite, bit étendu (24) pour
/// Ctrl et Alt droits.
pub fn sided_modifier(vk: usize, lparam: isize) -> Option<usize> {
    let scan = (lparam >> 16) & 0xFF;
    let extended = lparam & (1 << 24) != 0;
    match vk {
        0x10 => Some(if scan == 0x36 { 0xA1 } else { 0xA0 }),
        0x11 => Some(if extended { 0xA3 } else { 0xA2 }),
        0x12 => Some(if extended { 0xA5 } else { 0xA4 }),
        _ => None,
    }
}

/// Touche virtuelle d'un message de bouton de souris et son nouvel état.
pub fn mouse_button_transition(message: u32, wparam: usize) -> Option<(usize, bool)> {
    let xbutton = if (wparam >> 16) & 0xFFFF == 2 { 6 } else { 5 };
    Some(match message {
        0x0201 | 0x0203 => (1, true),
        0x0202 => (1, false),
        0x0204 | 0x0206 => (2, true),
        0x0205 => (2, false),
        0x0207 | 0x0209 => (4, true),
        0x0208 => (4, false),
        0x020B | 0x020D => (xbutton, true),
        0x020C => (xbutton, false),
        _ => return None,
    })
}

fn update_thread_key_state(vk: usize, down: bool) {
    let tid = std::thread::current().id();
    let mut states = lock(thread_keyboard_states());
    let keys = states.entry(tid).or_insert([0; KEY_STATE_COUNT]);
    transition_key_state(&mut keys[vk], vk, down);
}

/// `SHORT` GetKeyState : bit 15 = pressé, bit 0 = bascule (Caps/Num/Scroll).
pub fn get_key_state(vk: usize) -> i16 {
    if vk >= KEY_STATE_COUNT {
        return 0;
    }
    let tid = std::thread::current().id();
    let byte = lock(thread_keyboard_states())
        .get(&tid)
        .map(|keys| keys[vk])
        .unwrap_or(0);
    byte as i8 as i16
}

/// Copie l'état clavier des 256 touches virtuelles du thread courant.
pub fn get_keyboard_state() -> [u8; KEY_STATE_COUNT] {
    let tid = std::thread::current().id();
    lock(thread_keyboard_states())
        .get(&tid)
        .copied()
        .unwrap_or([0; KEY_STATE_COUNT])
}

/// Enregistre l'état hôte consulté par `GetAsyncKeyState`.
pub fn update_async_key_state(vk: usize, down: bool) {
    if vk >= KEY_STATE_COUNT {
        return;
    }
    let mut state = lock(async_keyboard_state());
    let was_down = state.keys[vk] & 0x80 != 0;
    transition_key_state(&mut state.keys[vk], vk, down);
    if down && !was_down {
        state.pressed_since_query[vk] = true;
    }
}

/// `SHORT` GetAsyncKeyState : état courant en bit 15, pression depuis le
/// dernier appel en bit 0 (ce dernier est consommé par la lecture).
pub fn get_async_key_state(vk: usize) -> i16 {
    if vk >= KEY_STATE_COUNT {
        return 0;
    }
    // Boutons de la souris : un clic d'automate (pression et relâchement
    // dans le même lot X11) doit encore se lire pressé pendant que le
    // programme traite son WM_xBUTTONDOWN.
    let held = matches!(vk, 1 | 2 | 4 | 5 | 6) && get_key_state(vk) as u16 & 0x8000 != 0;
    let mut state = lock(async_keyboard_state());
    let down = held || state.keys[vk] & 0x80 != 0;
    let pressed = std::mem::replace(&mut state.pressed_since_query[vk], false);
    (((if down { 1u16 } else { 0 }) << 15) | if pressed { 1 } else { 0 }) as i16
}

/// Boutons de la souris (VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1/2) :
/// leur état suit les messages de souris, pas le focus clavier.
fn is_mouse_button(vk: usize) -> bool {
    matches!(vk, 1 | 2 | 4 | 5 | 6)
}

/// Touches relâchées à la perte du focus clavier. Les boutons de la souris
/// restent : le clic qui donne le focus (SetFocus de la liste pendant son
/// WM_LBUTTONDOWN) est toujours tenu, et DragDetect le lit (#1138).
fn clear_thread_keyboard_state(owner: &ThreadId) {
    if let Some(keys) = lock(thread_keyboard_states()).get_mut(owner) {
        for (vk, byte) in keys.iter_mut().enumerate() {
            if !is_mouse_button(vk) {
                *byte &= 0x01; // conserve les états de bascule
            }
        }
    }
}

pub fn clear_thread_keyboard_state_for_window(hwnd: usize) {
    if let Some(owner) = window_owner(hwnd) {
        clear_thread_keyboard_state(&owner);
    }
}

pub fn clear_current_thread_keyboard_state() {
    clear_thread_keyboard_state(&std::thread::current().id());
}

pub fn clear_async_key_down_states() {
    let mut state = lock(async_keyboard_state());
    for (vk, byte) in state.keys.iter_mut().enumerate() {
        if !is_mouse_button(vk) {
            *byte &= 0x01;
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct KeyCharKey {
    hwnd: usize,
    vk: usize,
    lparam: isize,
}

/// Caractères en attente de `TranslateMessage`, associés au message clavier
/// précis. La file conserve l'ordre lorsque plusieurs frappes identiques sont
/// récupérées par un seul appel à `PeekMessage`.
fn key_chars() -> &'static Mutex<HashMap<KeyCharKey, VecDeque<Option<u16>>>> {
    static K: OnceLock<Mutex<HashMap<KeyCharKey, VecDeque<Option<u16>>>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Enregistre le caractère produit par une pression (ou `None` si elle n'en
/// produit pas, par exemple quand Ctrl est enfoncé).
pub fn set_key_char(hwnd: usize, vk: usize, lparam: isize, ch: Option<u16>) {
    let key = KeyCharKey { hwnd, vk, lparam };
    let mut pending = lock(key_chars());
    let chars = pending.entry(key).or_default();
    // Une application qui ne traduit jamais ses messages clavier ne doit pas
    // faire croître cette file sans limite.
    if chars.len() == 32 {
        chars.pop_front();
    }
    chars.push_back(ch);
}

/// Caractère à synthétiser pour un `WM_KEYDOWN`, consommé une seule fois.
pub fn take_key_char(hwnd: usize, vk: usize, lparam: isize) -> Option<u16> {
    let key = KeyCharKey { hwnd, vk, lparam };
    let queued = {
        let mut pending = lock(key_chars());
        let result = pending.get_mut(&key).and_then(VecDeque::pop_front);
        if pending.get(&key).is_some_and(VecDeque::is_empty) {
            pending.remove(&key);
        }
        result
    };
    match queued {
        Some(ch) => ch,
        None => vk_to_char(vk),
    }
}

/// Accent isolé à taper avant le caractère d'une frappe (touche morte
/// suivie d'une lettre qu'elle ne peut pas accentuer : « ^x »).
fn key_prefixes() -> &'static Mutex<HashMap<KeyCharKey, u16>> {
    static K: OnceLock<Mutex<HashMap<KeyCharKey, u16>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_key_prefix(hwnd: usize, vk: usize, lparam: isize, ch: u16) {
    lock(key_prefixes()).insert(KeyCharKey { hwnd, vk, lparam }, ch);
}

pub fn take_key_prefix(hwnd: usize, vk: usize, lparam: isize) -> Option<u16> {
    lock(key_prefixes()).remove(&KeyCharKey { hwnd, vk, lparam })
}

fn discard_key_chars(hwnd: usize) {
    lock(key_prefixes()).retain(|key, _| key.hwnd != hwnd);
    lock(key_chars()).retain(|key, _| key.hwnd != hwnd);
}

/// Traduction par défaut d'une touche virtuelle, utilisée quand le backend n'a
/// rien enregistré (message clavier synthétique) : disposition US non modifiée.
pub fn vk_to_char(vk: usize) -> Option<u16> {
    match vk {
        VK_BACK | VK_TAB | VK_RETURN | VK_ESCAPE | VK_SPACE => Some(vk as u16),
        0x30..=0x39 => Some(vk as u16),        // chiffres
        0x41..=0x5A => Some(vk as u16 + 0x20), // lettres (minuscules)
        _ => None,
    }
}

// ---------- Appel du WndProc invité ----------

/// Appelle le `WndProc` de la classe (code invité, convention win64).
///
/// # Sécurité
/// `wnd_proc` doit être une adresse de fonction invité valide. L'appel se fait
/// sans verrou tenu pour permettre les réentrances (le wndproc peut rappeler des API).
pub unsafe fn call_wndproc(
    wnd_proc: usize,
    hwnd: usize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let f: extern "win64" fn(usize, u32, usize, isize) -> isize = std::mem::transmute(wnd_proc);
    // WINLUX_WINDOW_DIAG=1 : messages d'état de fenêtre reçus par le
    // programme (affichage, taille, activation, focus), pour les jeux qui
    // se croient cachés ou réduits (#1421).
    if crate::dlls::user32::window_diag()
        && matches!(
            msg,
            0x0003
                | 0x0005
                | 0x0006
                | 0x0007
                | 0x0008
                | 0x0018
                | 0x001C
                | 0x0021
                | 0x0046
                | 0x0047
                | 0x0086
                | 0x0201
                | 0x0202
        )
    {
        eprintln!(
            "winlux[fenetre] message {hwnd:#x} {msg:#06x} wparam={wparam:#x} lparam={lparam:#x}"
        );
    }
    f(hwnd, msg, wparam, lparam)
}

// ---------- Dessin : HDC, objets GDI, commandes ----------

/// Commande de rendu produite par les primitives GDI invitées. La liste est
/// conservée par le backend et rejouée à chaque `WM_PAINT` (Expose X11), ce qui
/// donne un vrai cycle de rendu sans dépendre d'un GDI invité complet.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    /// Remplissage d'un rectangle logique (`COLORREF`).
    FillRect { rect: Rect, color: u32 },
    /// Cadre de `width` pixels autour du rectangle.
    FrameRect { rect: Rect, color: u32, width: u16 },
    /// Texte ANSI dont `x,y` désignent le coin haut-gauche. `bk` est la couleur
    /// de fond posée derrière les glyphes (`SetBkColor`), ou `None` quand le mode
    /// de fond est transparent (`SetBkMode(TRANSPARENT)`).
    Text {
        x: i32,
        y: i32,
        text: String,
        color: u32,
        bk: Option<u32>,
        font: FontSpec,
    },
    /// Ellipse inscrite dans le rectangle.
    Ellipse { rect: Rect, color: u32 },
    /// ROP binaire entre le pinceau courant et le contenu existant.
    PatternRect {
        rect: Rect,
        color: u32,
        rop: RasterOp,
    },
    /// Copie de pixels entre deux fenêtres X11 (SRCCOPY).
    Blit {
        source_hwnd: usize,
        source: Rect,
        destination: Rect,
        stretch: bool,
        /// Interpolation bilinéaire (mode HALFTONE) au lieu du plus proche voisin.
        smooth: bool,
        rop: RasterOp,
    },
    /// Copie depuis une bitmap mémoire (pixels stockés en COLORREF).
    Bitmap {
        rect: Rect,
        pixels: Vec<u32>,
        rop: RasterOp,
    },
    /// Ligne entre deux points.
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
        width: u16,
    },
    /// Rectangle arrondi inscrit dans le rectangle.
    RoundRect {
        rect: Rect,
        color: u32,
        rx: i32,
        ry: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterOp {
    Copy,
    And,
    Or,
    Xor,
    Invert,
    CopyInverted,
    Clear,
    Set,
    SrcErase,    // (NOT source) AND destination
    NotSrcErase, // NOT (source OR destination)
    MergePaint,  // (NOT source) OR destination
    PatCopy,     // pattern
    PatPaint,    // DPSnoo
    DstCopy,     // destination
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapSnapshot {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

/// Sous-ensemble des paramètres de CreateFontA/W utile au rendu X11.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontSpec {
    pub height: i32,
    pub width: i32,
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub face: String,
}

impl Default for FontSpec {
    fn default() -> Self {
        Self {
            height: 13,
            width: 0,
            weight: 400,
            italic: false,
            underline: false,
            strikeout: false,
            face: "fixed".to_string(),
        }
    }
}

/// État d'un `HDC` invité. Les HDC de fenêtre pointent sur la table des HWND ;
/// les DC mémoire vivent seuls dans la table et ne rendent pas.
#[derive(Clone, Copy)]
pub struct DcState {
    pub hwnd: usize,
    pub text_color: u32,
    pub bk_color: u32,
    /// 1 = transparent, 2 = opaque (constantes Win32 `TRANSPARENT`/`OPAQUE`).
    pub bk_mode: u32,
    pub brush: usize,
    pub pen: usize,
    pub font: usize,
    pub bitmap: usize,
    /// Mode de `SetStretchBltMode` (4 = HALFTONE : agrandissement lissé).
    pub stretch_mode: i32,
    /// Mode de `SetTextAlign` (TA_LEFT|TA_TOP = 0 par défaut).
    pub text_align: u32,
    /// DC de l'écran entier (`GetDC(NULL)`), par opposition à un DC
    /// mémoire, lui aussi sans fenêtre.
    pub screen: bool,
    /// Origines de fenêtre et de viewport (SetWindowOrgEx,
    /// SetViewportOrgEx), en MM_TEXT : point logique p → p − fenêtre +
    /// viewport. La VCL de Delphi s'en sert pour dessiner ses libellés
    /// dans le DC du parent (#1232).
    pub window_org: (i32, i32),
    pub viewport_org: (i32, i32),
}

/// Pixels d'une DIB section : tampon à nous, ou vue d'une section
/// (CreateDIBSection avec hSection, #996) que l'appelant partage avec un
/// autre processus. La vue est rendue à la destruction de la bitmap.
pub enum DibBits {
    Owned(Vec<u8>),
    /// `base` : adresse rendue par MapViewOfFile, `ptr` : premier pixel.
    View {
        base: usize,
        ptr: *mut u8,
        len: usize,
    },
}

// La vue appartient à la bitmap, protégée par le verrou des objets GDI.
unsafe impl Send for DibBits {}

impl std::ops::Deref for DibBits {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            DibBits::Owned(v) => v,
            DibBits::View { ptr, len, .. } => unsafe { std::slice::from_raw_parts(*ptr, *len) },
        }
    }
}

impl std::ops::DerefMut for DibBits {
    fn deref_mut(&mut self) -> &mut [u8] {
        match self {
            DibBits::Owned(v) => v,
            DibBits::View { ptr, len, .. } => unsafe { std::slice::from_raw_parts_mut(*ptr, *len) },
        }
    }
}

impl Clone for DibBits {
    fn clone(&self) -> Self {
        DibBits::Owned(self.to_vec())
    }
}

impl Drop for DibBits {
    fn drop(&mut self) {
        if let DibBits::View { base, .. } = self {
            unsafe {
                crate::dlls::mmap::unmap_view_of_file(*base);
            }
        }
    }
}

/// Objet GDI créé par l'invité (`CreateSolidBrush`, `CreatePen`).
#[derive(Clone)]
pub enum GdiObject {
    Brush(u32),
    SystemBrush(u32),
    Pen {
        width: u16,
        color: u32,
    },
    Font(FontSpec),
    /// Région rectangulaire (`CreateRectRgn` et consorts) : boîte englobante.
    Region(Rect),
    Bitmap {
        width: usize,
        height: usize,
        pixels: Vec<u32>,
        dib_bits: Option<DibBits>,
        dib_bit_count: u16,
        dib_stride: usize,
        top_down: bool,
    },
}

#[derive(Clone, Debug)]
pub enum GdiObjectInfo {
    Brush {
        style: u32,
        color: u32,
        hatch: usize,
    },
    Pen {
        style: u32,
        width: i32,
        color: u32,
    },
    Font(FontSpec),
    Region(Rect),
    Bitmap {
        width: usize,
        height: usize,
        width_bytes: usize,
        bits_per_pixel: u16,
        bits: usize,
    },
}

fn dcs() -> &'static Mutex<HashMap<usize, DcState>> {
    static D: OnceLock<Mutex<HashMap<usize, DcState>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(HashMap::new()))
}

fn objects() -> &'static Mutex<HashMap<usize, GdiObject>> {
    static O: OnceLock<Mutex<HashMap<usize, GdiObject>>> = OnceLock::new();
    O.get_or_init(|| Mutex::new(HashMap::new()))
}

fn system_brushes() -> &'static Mutex<HashMap<usize, usize>> {
    static B: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Compteur d'handles invités : HDC à partir de `0x0011_0000`, objets GDI à
/// partir de `0x0021_0000`. Les valeurs faibles restent réservées aux objets
/// « stock » (`GetStockObject` renvoie `index + 1`).
pub(crate) fn next_handle(base: usize) -> usize {
    static A: OnceLock<Mutex<usize>> = OnceLock::new();
    let a = A.get_or_init(|| Mutex::new(0x0011_0000));
    let mut v = lock(a);
    if *v < base {
        *v = base;
    }
    let h = *v;
    *v += 1;
    h
}

/// Crée un DC associé à une fenêtre (ou mémoire si `hwnd == 0`).
pub fn create_dc(hwnd: usize) -> usize {
    let hdc = next_handle(0x0011_0000);
    lock(dcs()).insert(
        hdc,
        DcState {
            hwnd,
            text_color: 0x0000_0000,
            bk_color: 0x00FF_FFFF,
            bk_mode: 2,
            brush: 1, // WHITE_BRUSH stock, comme Windows
            pen: 8,   // BLACK_PEN stock
            font: 14, // SYSTEM_FONT stock
            bitmap: 0,
            stretch_mode: 1, // BLACKONWHITE, valeur Windows par défaut
            text_align: 0,
            screen: false,
            window_org: (0, 0),
            viewport_org: (0, 0),
        },
    );
    hdc
}

/// DC de l'écran (`GetDC(NULL)`, `GetWindowDC(NULL)`) : dessine comme un DC
/// sans fenêtre, mais `WindowFromDC` le rattache au bureau.
pub fn create_screen_dc() -> usize {
    let hdc = create_dc(0);
    if let Some(d) = lock(dcs()).get_mut(&hdc) {
        d.screen = true;
    }
    hdc
}

/// Fenêtre d'un DC au sens de `WindowFromDC` : celle du DC, le bureau pour
/// un DC d'écran, rien pour un DC mémoire ou inconnu.
pub fn window_from_dc(hdc: usize) -> usize {
    match lock(dcs()).get(&hdc) {
        Some(d) if d.hwnd != 0 => d.hwnd,
        Some(d) if d.screen => crate::dlls::user32::get_desktop_window(),
        _ => 0,
    }
}

/// Retire un DC (`ReleaseDC`, fin de `BeginPaint`).
pub fn free_dc(hdc: usize) {
    lock(dcs()).remove(&hdc);
    lock(clips()).remove(&hdc);
}

// ---------- Découpage (clipping) des DC ----------

/// Zone de découpage de chaque DC qui en a une : une liste de rectangles
/// disjoints, en coordonnées logiques du DC. Absent = pas de découpage
/// (toute la surface). `IntersectClipRect`, `ExcludeClipRect`,
/// `SelectClipRgn` et `ETO_CLIPPED` la remplissent ; `record` l'applique.
fn clips() -> &'static Mutex<HashMap<usize, Vec<Rect>>> {
    static C: OnceLock<Mutex<HashMap<usize, Vec<Rect>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Surface « illimitée » d'un DC sans découpage.
const CLIP_ALL: Rect = Rect {
    left: -0x4000_0000,
    top: -0x4000_0000,
    right: 0x4000_0000,
    bottom: 0x4000_0000,
};

pub fn intersect_rects(a: &Rect, b: &Rect) -> Option<Rect> {
    let r = Rect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (r.left < r.right && r.top < r.bottom).then_some(r)
}

/// `a` privé de `b` : au plus quatre rectangles disjoints.
pub fn subtract_rect(a: &Rect, b: &Rect) -> Vec<Rect> {
    let Some(i) = intersect_rects(a, b) else {
        return vec![*a];
    };
    let mut out = Vec::new();
    let mut push = |r: Rect| {
        if r.left < r.right && r.top < r.bottom {
            out.push(r);
        }
    };
    push(Rect {
        bottom: i.top,
        ..*a
    });
    push(Rect {
        top: i.bottom,
        ..*a
    });
    push(Rect {
        top: i.top,
        bottom: i.bottom,
        right: i.left,
        ..*a
    });
    push(Rect {
        top: i.top,
        bottom: i.bottom,
        left: i.right,
        ..*a
    });
    out
}

/// Code de complexité Win32 d'une zone : NULLREGION, SIMPLEREGION ou
/// COMPLEXREGION.
fn clip_complexity(rects: &[Rect]) -> i32 {
    match rects.len() {
        0 => 1,
        1 => 2,
        _ => 3,
    }
}

/// Zone de découpage actuelle du DC (`None` : aucune).
pub fn dc_clip(hdc: usize) -> Option<Vec<Rect>> {
    lock(clips()).get(&hdc).cloned()
}

/// Remplace la zone de découpage (`None` l'enlève) ; renvoie sa complexité.
pub fn set_dc_clip(hdc: usize, rects: Option<Vec<Rect>>) -> i32 {
    let mut map = lock(clips());
    match rects {
        Some(r) => {
            let code = clip_complexity(&r);
            map.insert(hdc, r);
            code
        }
        None => {
            map.remove(&hdc);
            2
        }
    }
}

/// `IntersectClipRect`.
pub fn intersect_dc_clip(hdc: usize, r: Rect) -> i32 {
    let current = dc_clip(hdc).unwrap_or_else(|| vec![CLIP_ALL]);
    let next: Vec<Rect> = current
        .iter()
        .filter_map(|c| intersect_rects(c, &r))
        .collect();
    set_dc_clip(hdc, Some(next))
}

/// `ExcludeClipRect`.
pub fn exclude_dc_clip(hdc: usize, r: Rect) -> i32 {
    let current = dc_clip(hdc).unwrap_or_else(|| vec![CLIP_ALL]);
    let next: Vec<Rect> = current.iter().flat_map(|c| subtract_rect(c, &r)).collect();
    set_dc_clip(hdc, Some(next))
}

/// Boîte englobante de la zone de découpage (`GetClipBox`).
pub fn dc_clip_box(hdc: usize) -> Option<Rect> {
    let rects = dc_clip(hdc)?;
    let mut it = rects.iter();
    let first = *it.next()?;
    Some(it.fold(first, |a, r| Rect {
        left: a.left.min(r.left),
        top: a.top.min(r.top),
        right: a.right.max(r.right),
        bottom: a.bottom.max(r.bottom),
    }))
}

/// Les quatre bandes d'un cadre, pour les découper comme des remplissages.
fn frame_bands(rect: &Rect, width: i32) -> [Rect; 4] {
    let w = width.max(1);
    [
        Rect {
            bottom: (rect.top + w).min(rect.bottom),
            ..*rect
        },
        Rect {
            top: (rect.bottom - w).max(rect.top),
            ..*rect
        },
        Rect {
            top: rect.top + w,
            bottom: rect.bottom - w,
            right: (rect.left + w).min(rect.right),
            ..*rect
        },
        Rect {
            top: rect.top + w,
            bottom: rect.bottom - w,
            left: (rect.right - w).max(rect.left),
            ..*rect
        },
    ]
}

/// Boîte approximative d'une commande, pour écarter ce qui tombe hors zone.
fn cmd_bounds(cmd: &DrawCmd) -> Option<Rect> {
    Some(match cmd {
        DrawCmd::FillRect { rect, .. }
        | DrawCmd::FrameRect { rect, .. }
        | DrawCmd::Ellipse { rect, .. }
        | DrawCmd::PatternRect { rect, .. }
        | DrawCmd::Bitmap { rect, .. }
        | DrawCmd::RoundRect { rect, .. } => *rect,
        DrawCmd::Blit { destination, .. } => *destination,
        DrawCmd::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ..
        } => {
            let w = (*width as i32).max(1);
            Rect {
                left: (*x1).min(*x2) - w,
                top: (*y1).min(*y2) - w,
                right: (*x1).max(*x2) + w + 1,
                bottom: (*y1).max(*y2) + w + 1,
            }
        }
        DrawCmd::Text {
            x, y, text, font, ..
        } => {
            let (w, h) = measure_text(font, text);
            Rect::from_size(*x, *y, w.max(1), h.max(1))
        }
    })
}

/// Applique une zone de découpage à une commande : les remplissages et
/// cadres sont coupés exactement, le reste est gardé s'il touche la zone et
/// écarté sinon (approximation suffisante pour le texte des terminaux et
/// les bordures peintes autour d'eux, comme PuTTY).
pub fn clip_cmd(cmd: DrawCmd, clip: &[Rect]) -> Vec<DrawCmd> {
    match cmd {
        DrawCmd::FillRect { rect, color } => clip
            .iter()
            .filter_map(|c| intersect_rects(c, &rect))
            .map(|rect| DrawCmd::FillRect { rect, color })
            .collect(),
        DrawCmd::PatternRect { rect, color, rop } => clip
            .iter()
            .filter_map(|c| intersect_rects(c, &rect))
            .map(|rect| DrawCmd::PatternRect { rect, color, rop })
            .collect(),
        DrawCmd::FrameRect { rect, color, width } => {
            // Entièrement visible : le cadre reste un cadre.
            if clip.iter().any(|c| intersect_rects(c, &rect) == Some(rect)) {
                return vec![DrawCmd::FrameRect { rect, color, width }];
            }
            frame_bands(&rect, width as i32)
                .iter()
                .filter(|b| b.left < b.right && b.top < b.bottom)
                .flat_map(|b| clip.iter().filter_map(move |c| intersect_rects(c, b)))
                .map(|rect| DrawCmd::FillRect { rect, color })
                .collect()
        }
        other => match cmd_bounds(&other) {
            Some(b) if !clip.iter().any(|c| intersect_rects(c, &b).is_some()) => Vec::new(),
            _ => vec![other],
        },
    }
}

/// Type GDI d'un HDC pour GetObjectType : OBJ_MEMDC (10) pour un DC
/// mémoire (sans fenêtre), OBJ_DC (3) sinon ; `None` si ce n'est pas un DC.
/// System.Drawing refuse un DC dont le type est 0 (#666).
pub fn dc_object_type(hdc: usize) -> Option<u32> {
    lock(dcs())
        .get(&hdc)
        .map(|d| if d.hwnd == 0 { 10 } else { 3 })
}

/// Fenêtre cible d'un HDC, `None` si l'invalide.
pub fn dc_window(hdc: usize) -> Option<usize> {
    lock(dcs()).get(&hdc).map(|d| d.hwnd)
}

/// Applique une transformation à l'état du DC et renvoie sa valeur de retour ;
/// `default` est servi si le handle n'existe plus.
fn with_dc<T>(hdc: usize, f: impl FnOnce(&mut DcState) -> T, default: T) -> T {
    let mut map = lock(dcs());
    match map.get_mut(&hdc) {
        Some(d) => f(d),
        None => default,
    }
}

pub fn set_dc_text_color(hdc: usize, color: u32) -> u32 {
    with_dc(
        hdc,
        |d| {
            let old = d.text_color;
            d.text_color = color;
            old
        },
        0,
    )
}

pub fn set_dc_bk_color(hdc: usize, color: u32) -> u32 {
    with_dc(
        hdc,
        |d| {
            let old = d.bk_color;
            d.bk_color = color;
            old
        },
        0,
    )
}

pub fn set_dc_bk_mode(hdc: usize, mode: u32) -> u32 {
    with_dc(
        hdc,
        |d| {
            let old = d.bk_mode;
            d.bk_mode = mode;
            old
        },
        0,
    )
}

/// Change le mode d'agrandissement du DC et renvoie l'ancien (0 si invalide).
pub fn set_dc_stretch_mode(hdc: usize, mode: i32) -> i32 {
    with_dc(
        hdc,
        |d| {
            let old = d.stretch_mode;
            d.stretch_mode = mode;
            old
        },
        0,
    )
}

/// Mode d'agrandissement courant (`GetStretchBltMode`).
pub fn dc_stretch_mode(hdc: usize) -> i32 {
    with_dc(hdc, |d| d.stretch_mode, 0)
}

/// Couleur de texte courante d'un DC (utilisée par `TextOutA`).
pub fn dc_text_color(hdc: usize) -> u32 {
    with_dc(hdc, |d| d.text_color, 0)
}

/// Handle du pinceau sélectionné dans un DC.
pub fn dc_brush(hdc: usize) -> usize {
    with_dc(hdc, |d| d.brush, 0)
}

pub fn dc_pen(hdc: usize) -> usize {
    with_dc(hdc, |d| d.pen, 0)
}

/// Handle de la police sélectionnée dans le DC.
pub fn dc_font(hdc: usize) -> usize {
    with_dc(hdc, |d| d.font, 0)
}

/// Bitmap mémoire sélectionnée dans le DC (zéro si aucune).
pub fn dc_bitmap(hdc: usize) -> usize {
    with_dc(hdc, |d| d.bitmap, 0)
}

/// Couleur de fond courante d'un DC (`SetBkColor`).
pub fn dc_bk_color(hdc: usize) -> u32 {
    with_dc(hdc, |d| d.bk_color, 0x00FF_FFFF)
}

/// Mode de fond courant d'un DC (2 = opaque, 1 = transparent).
pub fn dc_text_align(hdc: usize) -> u32 {
    with_dc(hdc, |d| d.text_align, 0)
}

/// Change l'alignement du texte et renvoie l'ancien ; GDI_ERROR sans DC.
pub fn set_dc_text_align(hdc: usize, align: u32) -> u32 {
    with_dc(
        hdc,
        |d| std::mem::replace(&mut d.text_align, align),
        u32::MAX,
    )
}

pub fn dc_bk_mode(hdc: usize) -> u32 {
    with_dc(hdc, |d| d.bk_mode, 2)
}

/// Sélectionne un pinceau ou une plume selon la nature de l'objet.
pub fn select_object(hdc: usize, obj: usize) -> usize {
    let kind_pen =
        is_stock_pen(obj) || matches!(lock(objects()).get(&obj), Some(GdiObject::Pen { .. }));
    let kind_font = !kind_pen
        && (is_stock_font(obj) || matches!(lock(objects()).get(&obj), Some(GdiObject::Font(_))));
    let kind_bitmap = !kind_pen
        && !kind_font
        && (obj == DEFAULT_BITMAP
            || matches!(lock(objects()).get(&obj), Some(GdiObject::Bitmap { .. })));
    with_dc(
        hdc,
        |d| {
            if kind_pen {
                let old = d.pen;
                d.pen = obj;
                old
            } else if kind_font {
                let old = d.font;
                d.font = obj;
                old
            } else if kind_bitmap {
                // Un DC mémoire neuf contient la bitmap 1x1 du stock :
                // SelectObject la rend (jamais 0, que la VCL de Delphi
                // prend pour un échec : #753), et la resélectionner vide le DC.
                let old = d.bitmap;
                d.bitmap = if obj == DEFAULT_BITMAP { 0 } else { obj };
                if old == 0 {
                    DEFAULT_BITMAP
                } else {
                    old
                }
            } else {
                let old = d.brush;
                d.brush = obj;
                old
            }
        },
        0,
    )
}

/// Bitmap 1x1 monochrome du stock (`GetStockObject(21)`), celle d'un DC
/// mémoire neuf.
pub const DEFAULT_BITMAP: usize = 22;

/// Objets « stock » plume : `GetStockObject(WHITE_PEN)` → 7, `BLACK_PEN` → 8,
/// `NULL_PEN` → 9.
fn is_stock_pen(handle: usize) -> bool {
    matches!(handle, 7..=9)
}

fn is_stock_font(handle: usize) -> bool {
    matches!(handle, 11 | 12 | 13 | 14 | 15 | 17 | 18)
}

/// Région rectangulaire : un vrai handle de la table d'objets GDI (jamais un
/// handle « stock » comme 1 = WHITE_BRUSH), accepté par `DeleteObject`.
pub fn create_region(rect: Rect) -> usize {
    let h = next_handle(0x0021_0000);
    lock(objects()).insert(h, GdiObject::Region(normalize_rect(rect)));
    h
}

/// Boîte englobante d'une région, `None` si le handle n'en est pas une.
pub fn region_rect(handle: usize) -> Option<Rect> {
    match lock(objects()).get(&handle) {
        Some(GdiObject::Region(rect)) => Some(*rect),
        _ => None,
    }
}

/// Remplace la boîte d'une région existante ; faux si le handle est inconnu.
pub fn set_region_rect(handle: usize, rect: Rect) -> bool {
    match lock(objects()).get_mut(&handle) {
        Some(GdiObject::Region(current)) => {
            *current = normalize_rect(rect);
            true
        }
        _ => false,
    }
}

/// Rectangle ordonné (gauche ≤ droite, haut ≤ bas), comme GDI le fait pour
/// les régions.
fn normalize_rect(rect: Rect) -> Rect {
    Rect {
        left: rect.left.min(rect.right),
        top: rect.top.min(rect.bottom),
        right: rect.left.max(rect.right),
        bottom: rect.top.max(rect.bottom),
    }
}

pub fn create_solid_brush(color: u32) -> usize {
    let h = next_handle(0x0021_0000);
    lock(objects()).insert(h, GdiObject::Brush(color));
    h
}

/// Handle stable et non supprimable du pinceau correspondant à une couleur
/// système Windows.
pub fn system_color_brush(index: usize, color: u32) -> usize {
    let mut cache = lock(system_brushes());
    if let Some(&handle) = cache.get(&index) {
        // Le thème de contraste peut changer pendant l'exécution : le pinceau
        // système garde son handle mais suit la couleur courante.
        lock(objects()).insert(handle, GdiObject::SystemBrush(color));
        return handle;
    }
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(handle, GdiObject::SystemBrush(color));
    cache.insert(index, handle);
    handle
}

pub fn create_pen(style: u32, width: i32, color: u32) -> usize {
    // Les styles pointillés ne sont pas distingués : toute plume est pleine.
    let _ = style;
    let h = next_handle(0x0021_0000);
    lock(objects()).insert(
        h,
        GdiObject::Pen {
            width: width.max(1) as u16,
            color,
        },
    );
    h
}

pub fn create_font(spec: FontSpec) -> usize {
    trace_font_once(&spec.face, spec.weight, spec.italic);
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(handle, GdiObject::Font(spec));
    handle
}

/// WINLUX_TRACE : police retenue pour chaque nom demandé, une seule fois
/// par nom (#1670 : vgui de Steam ne trouvait aucune police).
fn trace_font_once(face: &str, weight: i32, italic: bool) {
    if !crate::log::enabled(crate::log::Level::Trace) {
        return;
    }
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let key = face.to_ascii_lowercase();
    {
        let mut seen = lock(&SEEN);
        if seen.contains(&key) {
            return;
        }
        seen.push(key);
    }
    let weight = if weight <= 0 { 400 } else { weight as u32 };
    let file = super::dwrite::gdi_face(face, weight, italic).map(|i| i.path);
    crate::trace!("CreateFont({face}) → {file:?}");
}

/// Crée une bitmap compatible 32 bits dont les pixels internes sont des COLORREF.
pub fn create_compatible_bitmap(width: i32, height: i32) -> Option<usize> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > MAX_BITMAP_PIXELS {
        return None;
    }
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(
        handle,
        GdiObject::Bitmap {
            width: width as usize,
            height: height as usize,
            pixels: vec![0; pixels],
            dib_bits: None,
            dib_bit_count: 32,
            dib_stride: width as usize * 4,
            top_down: false,
        },
    );
    Some(handle)
}

/// Crée une DIB section 24/32bpp et expose son tampon BGR(X) stable à l'invité.
pub fn create_dib_section(
    width: i32,
    height: i32,
    bit_count: u16,
    top_down: bool,
) -> Option<(usize, *mut u8)> {
    create_dib_section_with_palette(width, height, bit_count, top_down, Vec::new())
}

/// Tables de couleurs des DIB sections à palette (1, 4 et 8 bits par
/// pixel), en COLORREF, indexées par l'adresse du tampon de pixels : le
/// tampon ne bouge jamais tant que la bitmap existe.
fn dib_palettes() -> &'static Mutex<HashMap<usize, Vec<u32>>> {
    static P: OnceLock<Mutex<HashMap<usize, Vec<u32>>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

fn palette_of(bytes: &[u8]) -> Vec<u32> {
    lock(dib_palettes())
        .get(&(bytes.as_ptr() as usize))
        .cloned()
        .unwrap_or_default()
}

/// Table de couleurs (COLORREF) de la DIB sélectionnée dans `hdc`, comme
/// GetDIBColorTable ; vide pour une DIB sans palette.
pub fn dc_dib_color_table(hdc: usize) -> Vec<u32> {
    let bitmap = dc_bitmap(hdc);
    let objects = lock(objects());
    match objects.get(&bitmap) {
        Some(GdiObject::Bitmap {
            dib_bits: Some(bytes),
            dib_bit_count,
            ..
        }) if *dib_bit_count <= 8 => palette_of(bytes),
        _ => Vec::new(),
    }
}

/// Remplace des entrées de la table de couleurs de la DIB sélectionnée
/// dans `hdc` (SetDIBColorTable) ; rend le nombre d'entrées changées.
pub fn set_dc_dib_color_table(hdc: usize, start: usize, colors: &[u32]) -> usize {
    let bitmap = dc_bitmap(hdc);
    let objects = lock(objects());
    let Some(GdiObject::Bitmap {
        dib_bits: Some(bytes),
        dib_bit_count,
        ..
    }) = objects.get(&bitmap)
    else {
        return 0;
    };
    if *dib_bit_count > 8 {
        return 0;
    }
    let mut palettes = lock(dib_palettes());
    let Some(table) = palettes.get_mut(&(bytes.as_ptr() as usize)) else {
        return 0;
    };
    let mut n = 0;
    for (i, &c) in colors.iter().enumerate() {
        if let Some(slot) = table.get_mut(start + i) {
            *slot = c & 0x00FF_FFFF;
            n += 1;
        }
    }
    n
}

/// DIB section de 1, 4, 8, 16, 24 ou 32 bits par pixel. `palette`
/// (COLORREF) sert aux formats à palette ; 16 bits est en 5-5-5.
pub fn create_dib_section_with_palette(
    width: i32,
    height: i32,
    bit_count: u16,
    top_down: bool,
    palette: Vec<u32>,
) -> Option<(usize, *mut u8)> {
    create_dib_section_in(width, height, bit_count, top_down, palette, None)
}

/// Comme `create_dib_section_with_palette`, les pixels pouvant vivre dans
/// une section (`hSection`, `offset`) projetée pour la durée de la bitmap.
pub fn create_dib_section_in(
    width: i32,
    height: i32,
    bit_count: u16,
    top_down: bool,
    mut palette: Vec<u32>,
    section: Option<(usize, u32)>,
) -> Option<(usize, *mut u8)> {
    if width <= 0 || height <= 0 || !matches!(bit_count, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    if bit_count <= 8 {
        let entries = 1usize << bit_count;
        if palette.is_empty() && bit_count == 1 {
            palette = vec![0, 0x00FF_FFFF];
        }
        palette.resize(entries, 0);
    }
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > MAX_BITMAP_PIXELS {
        return None;
    }
    let stride = (width as usize * bit_count as usize).div_ceil(32) * 4;
    let bytes = stride.checked_mul(height as usize)?;
    let mut dib_bits = match section {
        None => DibBits::Owned(vec![0u8; bytes]),
        Some((handle, offset)) => {
            // FILE_MAP_READ | FILE_MAP_WRITE ; le contenu de la section est
            // gardé tel quel, comme sous Windows. L'offset (multiple de 4)
            // est ramené à la granularité d'allocation de MapViewOfFile.
            let skip = (offset % 0x1_0000) as usize;
            let base = unsafe {
                crate::dlls::mmap::map_view_of_file(
                    handle,
                    0x6,
                    0,
                    offset - skip as u32,
                    bytes + skip,
                )
            };
            if base == 0 {
                return None;
            }
            DibBits::View {
                base,
                ptr: (base + skip) as *mut u8,
                len: bytes,
            }
        }
    };
    let bits_ptr = dib_bits.as_mut_ptr();
    {
        let mut palettes = lock(dib_palettes());
        if bit_count <= 8 {
            palettes.insert(bits_ptr as usize, palette.clone());
        } else {
            palettes.remove(&(bits_ptr as usize));
        }
    }
    // Pixels de départ : index 0 de la palette, comme le tampon à zéro.
    let first = palette.first().copied().unwrap_or(0);
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(
        handle,
        GdiObject::Bitmap {
            width: width as usize,
            height: height as usize,
            pixels: vec![first; pixels],
            dib_bits: Some(dib_bits),
            dib_bit_count: bit_count,
            dib_stride: stride,
            top_down,
        },
    );
    Some((handle, bits_ptr))
}

fn bitmap_color_at(
    width: usize,
    height: usize,
    pixels: &[u32],
    dib_bits: Option<&[u8]>,
    dib_bit_count: u16,
    dib_stride: usize,
    top_down: bool,
    x: usize,
    y: usize,
) -> u32 {
    let index = if top_down { y } else { height - 1 - y } * width + x;
    if let Some(bytes) = dib_bits {
        let row = if top_down { y } else { height - 1 - y };
        if dib_bit_count <= 16 {
            return low_depth_color(bytes, dib_bit_count, row * dib_stride, x);
        }
        let bytes_per_pixel = (dib_bit_count / 8) as usize;
        let offset = row * dib_stride + x * bytes_per_pixel;
        let b = bytes[offset] as u32;
        let g = bytes[offset + 1] as u32;
        let r = bytes[offset + 2] as u32;
        (b << 16) | (g << 8) | r
    } else {
        pixels[index]
    }
}

/// Couleur (COLORREF) d'un pixel d'une DIB de 1, 4, 8 ou 16 bits.
fn low_depth_color(bytes: &[u8], bit_count: u16, row_start: usize, x: usize) -> u32 {
    if bit_count == 16 {
        let o = row_start + x * 2;
        let v = u16::from_le_bytes([bytes[o], bytes[o + 1]]) as u32;
        let scale = |c: u32| (c << 3) | (c >> 2);
        let (r, g, b) = (scale((v >> 10) & 31), scale((v >> 5) & 31), scale(v & 31));
        return (b << 16) | (g << 8) | r;
    }
    let bits = bit_count as usize;
    let bit = x * bits;
    let byte = bytes[row_start + bit / 8];
    let index = (byte >> (8 - bits - bit % 8)) & ((1u16 << bits) - 1) as u8;
    lock(dib_palettes())
        .get(&(bytes.as_ptr() as usize))
        .and_then(|p| p.get(index as usize).copied())
        .unwrap_or(0)
}

/// Écrit `color` dans une DIB de 1, 4, 8 ou 16 bits : l'entrée de palette
/// la plus proche pour les formats à palette.
fn set_low_depth_color(bytes: &mut [u8], bit_count: u16, row_start: usize, x: usize, color: u32) {
    if bit_count == 16 {
        let c = |shift: u32| ((color >> shift) & 0xFF) >> 3;
        let v = ((c(0) << 10) | (c(8) << 5) | c(16)) as u16;
        let o = row_start + x * 2;
        bytes[o..o + 2].copy_from_slice(&v.to_le_bytes());
        return;
    }
    let distance = |p: u32| {
        let d = |shift: u32| ((p >> shift) & 0xFF) as i32 - ((color >> shift) & 0xFF) as i32;
        d(0) * d(0) + d(8) * d(8) + d(16) * d(16)
    };
    let index = lock(dib_palettes())
        .get(&(bytes.as_ptr() as usize))
        .and_then(|table| {
            table
                .iter()
                .enumerate()
                .min_by_key(|(_, &p)| distance(p))
                .map(|(i, _)| i)
        })
        .unwrap_or(0) as u8;
    let bits = bit_count as usize;
    let bit = x * bits;
    let shift = 8 - bits - bit % 8;
    let mask = (((1u16 << bits) - 1) as u8) << shift;
    let b = &mut bytes[row_start + bit / 8];
    *b = (*b & !mask) | ((index << shift) & mask);
}

fn set_bitmap_color(
    width: usize,
    height: usize,
    pixels: &mut [u32],
    dib_bits: &mut Option<DibBits>,
    dib_bit_count: u16,
    dib_stride: usize,
    top_down: bool,
    x: usize,
    y: usize,
    color: u32,
) {
    let index = if top_down { y } else { height - 1 - y } * width + x;
    pixels[index] = color;
    if let Some(bytes) = dib_bits {
        let row = if top_down { y } else { height - 1 - y };
        if dib_bit_count <= 16 {
            set_low_depth_color(bytes, dib_bit_count, row * dib_stride, x, color);
            return;
        }
        let bytes_per_pixel = (dib_bit_count / 8) as usize;
        let offset = row * dib_stride + x * bytes_per_pixel;
        bytes[offset] = ((color >> 16) & 0xFF) as u8; // B
        bytes[offset + 1] = ((color >> 8) & 0xFF) as u8; // G
        bytes[offset + 2] = (color & 0xFF) as u8; // R
        if bytes_per_pixel == 4 {
            bytes[offset + 3] = 0;
        }
    }
}

/// Écrit une ligne de pixels (COLORREF, alpha dans l'octet haut) dans une
/// bitmap ; `y` est compté depuis le haut. Sert à SetDIBits.
pub fn write_bitmap_row(handle: usize, y: usize, row: &[u32]) -> bool {
    let mut objects = lock(objects());
    let Some(GdiObject::Bitmap {
        width,
        height,
        pixels,
        dib_bits,
        dib_bit_count,
        dib_stride,
        top_down,
    }) = objects.get_mut(&handle)
    else {
        return false;
    };
    let (w, h, bits, stride, td) = (*width, *height, *dib_bit_count, *dib_stride, *top_down);
    if y >= h {
        return false;
    }
    for (x, &c) in row.iter().take(w).enumerate() {
        set_bitmap_color(
            w,
            h,
            pixels,
            dib_bits,
            bits,
            stride,
            td,
            x,
            y,
            c & 0x00FF_FFFF,
        );
        if bits == 32 {
            if let Some(bytes) = dib_bits.as_mut() {
                let r = if td { y } else { h - 1 - y };
                if let Some(a) = bytes.get_mut(r * stride + x * 4 + 3) {
                    *a = (c >> 24) as u8;
                }
            }
        }
    }
    true
}

/// Pixels d'une bitmap, ligne du haut d'abord, en COLORREF avec l'alpha
/// dans l'octet haut. Le booléen dit si la bitmap porte un canal alpha
/// (DIB 32 bits) ; sinon l'octet haut est nul.
pub fn bitmap_pixels(handle: usize) -> Option<(usize, usize, Vec<u32>, bool)> {
    let objects = lock(objects());
    let Some(GdiObject::Bitmap {
        width,
        height,
        pixels,
        dib_bits,
        dib_bit_count,
        dib_stride,
        top_down,
    }) = objects.get(&handle)
    else {
        return None;
    };
    let (w, h) = (*width, *height);
    let alpha = dib_bits.is_some() && *dib_bit_count == 32;
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let mut c = bitmap_color_at(
                w,
                h,
                pixels,
                dib_bits.as_deref(),
                *dib_bit_count,
                *dib_stride,
                *top_down,
                x,
                y,
            ) & 0x00FF_FFFF;
            if alpha {
                let bytes = dib_bits.as_deref().unwrap_or(&[]);
                let row = if *top_down { y } else { h - 1 - y };
                let a = bytes
                    .get(row * dib_stride + x * 4 + 3)
                    .copied()
                    .unwrap_or(0);
                c |= (a as u32) << 24;
            }
            out.push(c);
        }
    }
    Some((w, h, out, alpha))
}

/// Bitmaps que Winlux garde en mémoire comme des DIB mais présente comme
/// des bitmaps dépendantes du périphérique (DDB) : celles de GetIconInfo,
/// comme sous Windows. wxWidgets en tire une DIB à lui puis supprime
/// l'originale ; s'il la voyait comme une DIB section, il supprimait sa
/// propre copie (#793).
fn device_dependent() -> &'static Mutex<std::collections::HashSet<usize>> {
    static S: OnceLock<Mutex<std::collections::HashSet<usize>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Bitmaps monochromes (`CreateBitmap` à 1 bit par pixel) : une copie
/// couleur vers elles suit la règle de Windows (couleur de fond du DC
/// source → blanc, toute autre → noir). wxWidgets en tire ses masques.
fn monochrome() -> &'static Mutex<std::collections::HashSet<usize>> {
    static S: OnceLock<Mutex<std::collections::HashSet<usize>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn mark_monochrome(handle: usize) {
    lock(monochrome()).insert(handle);
}

pub fn is_monochrome(handle: usize) -> bool {
    lock(monochrome()).contains(&handle)
}

/// Comme `bitmap_from_pixels`, mais vue par le programme comme une DDB.
pub fn ddb_from_pixels(width: usize, height: usize, pixels: &[u32]) -> Option<usize> {
    let h = bitmap_from_pixels(width, height, pixels)?;
    lock(device_dependent()).insert(h);
    Some(h)
}

/// Vrai pour une bitmap présentée comme DDB (voir `ddb_from_pixels`).
pub fn is_device_dependent(handle: usize) -> bool {
    lock(device_dependent()).contains(&handle)
}

/// Sens d'une DIB section : `Some(true)` haut-bas, `Some(false)` bas-haut,
/// `None` pour une bitmap qui n'est pas une DIB section.
pub fn dib_top_down(handle: usize) -> Option<bool> {
    match lock(objects()).get(&handle) {
        Some(GdiObject::Bitmap {
            dib_bits: Some(_),
            top_down,
            ..
        }) => Some(*top_down),
        _ => None,
    }
}

/// Crée une DIB section 32 bits bas-haut à partir de pixels COLORREF dont
/// l'octet haut est l'alpha.
pub fn bitmap_from_pixels(width: usize, height: usize, pixels: &[u32]) -> Option<usize> {
    if pixels.len() < width * height {
        return None;
    }
    // Bas-haut, comme les DIB que Windows rend pour une icône : wxWidgets
    // refuse de convertir une DIB haut-bas (image vide, puis écriture à
    // l'adresse -1 dans wxImageList::Add, #793).
    let (handle, _) = create_dib_section(width as i32, height as i32, 32, false)?;
    let mut objects = lock(objects());
    if let Some(GdiObject::Bitmap {
        pixels: store,
        dib_bits: Some(bytes),
        ..
    }) = objects.get_mut(&handle)
    {
        for (i, &c) in pixels[..width * height].iter().enumerate() {
            let (x, y) = (i % width, i / width);
            let o = ((height - 1 - y) * width + x) * 4;
            bytes[o] = (c >> 16) as u8;
            bytes[o + 1] = (c >> 8) as u8;
            bytes[o + 2] = c as u8;
            bytes[o + 3] = (c >> 24) as u8;
            store[i] = c & 0x00FF_FFFF;
        }
    }
    Some(handle)
}

/// Copie une zone de la bitmap sélectionnée dans un DC mémoire. Les pixels
/// hors limites sont noirs, comme une surface nouvellement créée.
pub fn bitmap_snapshot(hdc: usize, rect: Rect) -> Option<BitmapSnapshot> {
    let bitmap = dc_bitmap(hdc);
    if bitmap == 0 || rect.width() <= 0 || rect.height() <= 0 {
        return None;
    }
    let width = rect.width() as usize;
    let height = rect.height() as usize;
    let count = width.checked_mul(height)?;
    if count > MAX_BITMAP_PIXELS {
        return None;
    }
    let objects = lock(objects());
    let Some(GdiObject::Bitmap {
        width: bitmap_width,
        height: bitmap_height,
        pixels,
        dib_bits,
        dib_bit_count,
        dib_stride,
        top_down,
    }) = objects.get(&bitmap)
    else {
        return None;
    };
    let mut copy = vec![0; count];
    for y in 0..height {
        let sy = rect.top as i64 + y as i64;
        if sy < 0 || sy >= *bitmap_height as i64 {
            continue;
        }
        for x in 0..width {
            let sx = rect.left as i64 + x as i64;
            if sx >= 0 && sx < *bitmap_width as i64 {
                copy[y * width + x] = bitmap_color_at(
                    *bitmap_width,
                    *bitmap_height,
                    pixels,
                    dib_bits.as_deref(),
                    *dib_bit_count,
                    *dib_stride,
                    *top_down,
                    sx as usize,
                    sy as usize,
                );
            }
        }
    }
    Some(BitmapSnapshot {
        width,
        height,
        pixels: copy,
    })
}

/// Description d'une police créée, ou police système par défaut pour un objet
/// stock. `None` indique un handle non-police.
pub fn font_spec(handle: usize) -> Option<FontSpec> {
    match lock(objects()).get(&handle) {
        Some(GdiObject::Font(spec)) => Some(spec.clone()),
        Some(_) => None,
        None if is_stock_font(handle) => Some(FontSpec::default()),
        None => None,
    }
}

/// Détails Win32 de l'objet utilisés par `GetObjectA/W`.
pub fn object_info(handle: usize) -> Option<GdiObjectInfo> {
    {
        let map = lock(objects());
        if let Some(object) = map.get(&handle) {
            return Some(match object {
                GdiObject::Brush(color) | GdiObject::SystemBrush(color) => GdiObjectInfo::Brush {
                    style: 0,
                    color: *color,
                    hatch: 0,
                },
                GdiObject::Pen { width, color } => GdiObjectInfo::Pen {
                    style: 0,
                    width: *width as i32,
                    color: *color,
                },
                GdiObject::Font(spec) => GdiObjectInfo::Font(spec.clone()),
                GdiObject::Region(rect) => GdiObjectInfo::Region(*rect),
                GdiObject::Bitmap {
                    width,
                    height,
                    dib_bits,
                    dib_stride,
                    dib_bit_count,
                    ..
                } => GdiObjectInfo::Bitmap {
                    width: *width,
                    height: *height,
                    width_bytes: *dib_stride,
                    bits_per_pixel: *dib_bit_count,
                    bits: dib_bits
                        .as_ref()
                        .map(|data| data.as_ptr() as usize)
                        .unwrap_or(0),
                },
            });
        }
    }
    if (1..=6).contains(&handle) {
        return Some(GdiObjectInfo::Brush {
            style: if handle == 6 { 1 } else { 0 }, // BS_NULL for NULL_BRUSH
            color: object_color(handle).unwrap_or(0),
            hatch: 0,
        });
    }
    if (7..=9).contains(&handle) {
        return Some(GdiObjectInfo::Pen {
            style: if handle == 9 { 5 } else { 0 }, // PS_NULL for NULL_PEN
            width: 1,
            color: object_color(handle).unwrap_or(0),
        });
    }
    if is_stock_font(handle) {
        return Some(GdiObjectInfo::Font(FontSpec::default()));
    }
    if handle == DEFAULT_BITMAP {
        return Some(GdiObjectInfo::Bitmap {
            width: 1,
            height: 1,
            width_bytes: 2,
            bits_per_pixel: 1,
            bits: 0,
        });
    }
    None
}

/// Supprime un objet créé par l'invité. Les objets « stock » ne sont pas
/// réellement supprimés (Win32 l'interdit) mais on répond succès : beaucoup de
/// programmes appellent `DeleteObject` sans distinguer les deux cas.
pub fn delete_object(obj: usize) -> bool {
    if lock(dcs()).values().any(|dc| dc.bitmap == obj) {
        return false;
    }
    let mut map = lock(objects());
    match map.get(&obj) {
        Some(GdiObject::SystemBrush(_)) => false,
        Some(_) => {
            lock(device_dependent()).remove(&obj);
            lock(monochrome()).remove(&obj);
            map.remove(&obj).is_some()
        }
        None => (1..=9).contains(&obj) || obj == DEFAULT_BITMAP,
    }
}

/// Couleur d'un pinceau/plume, `None` pour les objets « null » du stock.
pub fn object_color(handle: usize) -> Option<u32> {
    {
        let map = lock(objects());
        if let Some(object) = map.get(&handle) {
            return match object {
                GdiObject::Brush(c) | GdiObject::SystemBrush(c) => Some(*c),
                GdiObject::Pen { color, .. } => Some(*color),
                GdiObject::Font(_) | GdiObject::Bitmap { .. } | GdiObject::Region(_) => None,
            };
        }
    }
    // Objets du stock (`GetStockObject` renvoie `index + 1`) : pinceaux puis
    // plumes ; `NULL_BRUSH` et `NULL_PEN` ne peignent rien.
    match handle {
        1 => Some(0x00FF_FFFF), // WHITE_BRUSH
        2 => Some(0x00C0_C0C0), // LTGRAY_BRUSH
        3 => Some(0x0080_8080), // GRAY_BRUSH
        4 => Some(0x0040_4040), // DKGRAY_BRUSH
        5 => Some(0x0000_0000), // BLACK_BRUSH
        6 => None,              // NULL_BRUSH
        7 => Some(0x00FF_FFFF), // WHITE_PEN
        8 => Some(0x0000_0000), // BLACK_PEN
        9 => None,              // NULL_PEN
        _ => None,
    }
}

/// Largeur d'une plume sélectionnée (les plumes simples font 1 px).
pub fn object_pen_width(handle: usize) -> u16 {
    match lock(objects()).get(&handle) {
        Some(GdiObject::Pen { width, .. }) => *width,
        _ => 1,
    }
}

fn dc_state_offset(d: &DcState) -> (i32, i32) {
    (
        d.viewport_org.0 - d.window_org.0,
        d.viewport_org.1 - d.window_org.1,
    )
}

/// Décalage logique → périphérique du DC (origines de viewport et de
/// fenêtre, MM_TEXT).
pub fn dc_offset(hdc: usize) -> (i32, i32) {
    lock(dcs()).get(&hdc).map(dc_state_offset).unwrap_or((0, 0))
}

/// Origines (fenêtre, viewport) du DC.
pub fn dc_origins(hdc: usize) -> ((i32, i32), (i32, i32)) {
    lock(dcs())
        .get(&hdc)
        .map(|d| (d.window_org, d.viewport_org))
        .unwrap_or(((0, 0), (0, 0)))
}

/// Pose l'origine de fenêtre (`viewport` faux) ou de viewport ; rend
/// l'ancienne, ou `None` si le DC n'existe pas.
pub fn set_dc_origin(hdc: usize, viewport: bool, x: i32, y: i32) -> Option<(i32, i32)> {
    let mut m = lock(dcs());
    let d = m.get_mut(&hdc)?;
    let slot = if viewport {
        &mut d.viewport_org
    } else {
        &mut d.window_org
    };
    Some(std::mem::replace(slot, (x, y)))
}

/// Rectangle logique → périphérique.
pub fn rect_to_device(hdc: usize, r: Rect) -> Rect {
    let (dx, dy) = dc_offset(hdc);
    offset_rect(r, dx, dy)
}

fn offset_rect(r: Rect, dx: i32, dy: i32) -> Rect {
    Rect {
        left: r.left + dx,
        top: r.top + dy,
        right: r.right + dx,
        bottom: r.bottom + dy,
    }
}

/// Commande de dessin passée des coordonnées logiques du DC à ses
/// coordonnées de périphérique. La source d'une copie reste telle quelle.
fn translate_cmd(cmd: DrawCmd, (dx, dy): (i32, i32)) -> DrawCmd {
    if dx == 0 && dy == 0 {
        return cmd;
    }
    let o = |r: Rect| offset_rect(r, dx, dy);
    match cmd {
        DrawCmd::FillRect { rect, color } => DrawCmd::FillRect {
            rect: o(rect),
            color,
        },
        DrawCmd::FrameRect { rect, color, width } => DrawCmd::FrameRect {
            rect: o(rect),
            color,
            width,
        },
        DrawCmd::Text {
            x,
            y,
            text,
            color,
            bk,
            font,
        } => DrawCmd::Text {
            x: x + dx,
            y: y + dy,
            text,
            color,
            bk,
            font,
        },
        DrawCmd::Ellipse { rect, color } => DrawCmd::Ellipse {
            rect: o(rect),
            color,
        },
        DrawCmd::PatternRect { rect, color, rop } => DrawCmd::PatternRect {
            rect: o(rect),
            color,
            rop,
        },
        DrawCmd::Blit {
            source_hwnd,
            source,
            destination,
            stretch,
            smooth,
            rop,
        } => DrawCmd::Blit {
            source_hwnd,
            source,
            destination: o(destination),
            stretch,
            smooth,
            rop,
        },
        DrawCmd::Bitmap { rect, pixels, rop } => DrawCmd::Bitmap {
            rect: o(rect),
            pixels,
            rop,
        },
        DrawCmd::Line {
            x1,
            y1,
            x2,
            y2,
            color,
            width,
        } => DrawCmd::Line {
            x1: x1 + dx,
            y1: y1 + dy,
            x2: x2 + dx,
            y2: y2 + dy,
            color,
            width,
        },
        DrawCmd::RoundRect {
            rect,
            color,
            rx,
            ry,
        } => DrawCmd::RoundRect {
            rect: o(rect),
            color,
            rx,
            ry,
        },
    }
}

thread_local! {
    /// Diagnostic WINLUX_WINDOW_DIAG : vrai pendant un WM_DRAWITEM ou un
    /// TVN_GETDISPINFO, chaque commande de dessin est alors journalisée.
    static WATCH_DRAWS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Journalise les commandes de dessin pendant `f` (WINLUX_WINDOW_DIAG).
pub fn watch_draws<R>(on: bool, f: impl FnOnce() -> R) -> R {
    let old = WATCH_DRAWS.with(|w| w.replace(on));
    let r = f();
    WATCH_DRAWS.with(|w| w.set(old));
    r
}

/// Ajoute une commande de dessin à la file d'attente de peinture de la fenêtre
/// cible du DC. Renvoie faux si le DC ou la fenêtre n'existe plus.
/// Diagnostic WINLUX_D2D_TRACE (#1609) : fenêtre, bitmap sélectionné,
/// décalage et découpe d'un DC, pour savoir où va une image présentée.
pub fn describe_dc(hdc: usize) -> String {
    let known = lock(dcs())
        .get(&hdc)
        .map(|d| (d.hwnd, d.bitmap, dc_state_offset(d)));
    let Some((hwnd, bitmap, offset)) = known else {
        return format!("dc {hdc:#x} inconnu");
    };
    let size = match lock(objects()).get(&bitmap) {
        Some(GdiObject::Bitmap {
            width,
            height,
            dib_bits,
            ..
        }) => format!("{width}x{height} dib {}", dib_bits.is_some()),
        _ => "-".to_string(),
    };
    format!(
        "fenêtre {hwnd:#x} bitmap {bitmap:#x} ({size}) décalage {offset:?} découpe {:?}",
        dc_clip(hdc)
    )
}

pub fn record(hdc: usize, cmd: DrawCmd) -> bool {
    if WATCH_DRAWS.with(|w| w.get()) {
        let known = lock(dcs())
            .get(&hdc)
            .map(|d| (d.hwnd, d.bitmap, dc_state_offset(d)));
        let clip = dc_clip(hdc);
        let mut desc = format!("{cmd:?}");
        if desc.len() > 160 {
            let mut cut = 160;
            while !desc.is_char_boundary(cut) {
                cut -= 1;
            }
            desc.truncate(cut);
        }
        eprintln!("winlux[fenetre]   dessin hdc {hdc:#x} dc {known:?} découpe {clip:?} : {desc}");
    }
    // DC imprimante : la commande part dans la page en cours (PDF).
    let Some(cmd) = crate::dlls::winspool::capture(hdc, cmd) else {
        return true;
    };
    let (target, bitmap, offset) = match lock(dcs()).get(&hdc) {
        Some(d) => (d.hwnd, d.bitmap, dc_state_offset(d)),
        None => return false,
    };
    let cmd = translate_cmd(cmd, offset);
    let cmds = match dc_clip(hdc) {
        Some(clip) => clip_cmd(cmd, &clip),
        None => vec![cmd],
    };
    if target == 0 {
        if bitmap == 0 {
            return false;
        }
        let mut ok = true;
        for cmd in &cmds {
            ok &= draw_to_bitmap(bitmap, cmd);
        }
        return ok;
    }
    {
        let mut map = lock(windows());
        if let Some(w) = map.get_mut(&target) {
            // Une image copiée hors WM_PAINT (BitBlt, StretchDIBits…) se
            // voit tout de suite sous Windows. SDL garde le DC de sa
            // fenêtre (CS_OWNDC) sans jamais appeler ReleaseDC : sa surface
            // logicielle restait en attente, fenêtre blanche (#1706).
            let image = !w.painting && cmds.iter().any(is_image_cmd);
            w.pending.extend(cmds);
            drop(map);
            if image {
                flush_paint_over(target);
            }
            return true;
        }
    }
    // Fenêtre d'un autre processus du compte : le processus GPU de Chromium
    // peint les surgissants du navigateur en logiciel (GetDC, puis BitBlt
    // depuis une section DIB). Les images vont tout de suite sur sa fenêtre
    // X11, commune à tous les clients du serveur (#1345).
    match foreign_window(target) {
        Some(f) => {
            for cmd in &cmds {
                paint_foreign(f.xid, cmd);
            }
            true
        }
        None => false,
    }
}

/// Peint une commande sur la fenêtre X11 d'un autre processus. Seules les
/// images (BitBlt, StretchBlt, SetDIBitsToDevice, StretchDIBits depuis la
/// mémoire) passent : c'est ce qu'envoie une sortie logicielle.
fn paint_foreign(xid: u64, cmd: &DrawCmd) {
    if let DrawCmd::Bitmap { rect, pixels, rop } = cmd {
        #[cfg(test)]
        FOREIGN_PAINTS.with(|p| p.borrow_mut().push((xid, *rect, pixels.len())));
        with_backend(|b| b.paint_foreign(xid, *rect, pixels, *rop));
    }
}

#[cfg(test)]
thread_local! {
    /// Images envoyées à des fenêtres étrangères (xid, rectangle, pixels).
    pub(crate) static FOREIGN_PAINTS: std::cell::RefCell<Vec<(u64, Rect, usize)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn draw_to_bitmap(handle: usize, cmd: &DrawCmd) -> bool {
    // Texte : glyphes préparés avant de prendre le verrou des objets GDI
    // (la police se résout à partir des fichiers TrueType du système).
    let glyphs = match cmd {
        DrawCmd::Text { text, font, .. } => crate::dlls::usp10::layout_pixels(font, text),
        _ => None,
    };
    let mut map = lock(objects());
    let Some(GdiObject::Bitmap {
        width,
        height,
        pixels,
        dib_bits,
        dib_bit_count,
        dib_stride,
        top_down,
    }) = map.get_mut(&handle)
    else {
        return false;
    };
    let bounds = Rect::from_size(0, 0, *width as i32, *height as i32);
    let mut put = |x: i32, y: i32, source: u32, rop: RasterOp| {
        if x >= 0 && y >= 0 && x < bounds.right && y < bounds.bottom {
            let old = bitmap_color_at(
                *width,
                *height,
                pixels,
                dib_bits.as_deref(),
                *dib_bit_count,
                *dib_stride,
                *top_down,
                x as usize,
                y as usize,
            );
            set_bitmap_color(
                *width,
                *height,
                pixels,
                dib_bits,
                *dib_bit_count,
                *dib_stride,
                *top_down,
                x as usize,
                y as usize,
                raster_color(source, old, rop),
            );
        }
    };
    match cmd {
        DrawCmd::FillRect { rect, color } => {
            for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                for x in rect.left.max(0)..rect.right.min(bounds.right) {
                    put(x, y, *color, RasterOp::Copy);
                }
            }
        }
        DrawCmd::FrameRect {
            rect,
            color,
            width: line,
        } => {
            let line = (*line as i32).max(1);
            for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                for x in rect.left.max(0)..rect.right.min(bounds.right) {
                    if x < rect.left + line
                        || x >= rect.right - line
                        || y < rect.top + line
                        || y >= rect.bottom - line
                    {
                        put(x, y, *color, RasterOp::Copy);
                    }
                }
            }
        }
        DrawCmd::Ellipse { rect, color } => {
            let w = rect.width();
            let h = rect.height();
            if w > 0 && h > 0 {
                let rx = w as f64 / 2.0;
                let ry = h as f64 / 2.0;
                for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                    for x in rect.left.max(0)..rect.right.min(bounds.right) {
                        let dx = (x as f64 + 0.5 - rect.left as f64 - rx) / rx;
                        let dy = (y as f64 + 0.5 - rect.top as f64 - ry) / ry;
                        if dx * dx + dy * dy <= 1.0 {
                            put(x, y, *color, RasterOp::Copy);
                        }
                    }
                }
            }
        }
        DrawCmd::PatternRect { rect, color, rop } => {
            for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                for x in rect.left.max(0)..rect.right.min(bounds.right) {
                    put(x, y, *color, *rop);
                }
            }
        }
        DrawCmd::Bitmap {
            rect,
            pixels: source,
            rop,
        } => {
            let sw = rect.width().max(0) as usize;
            let sh = rect.height().max(0) as usize;
            let Some(required) = sw.checked_mul(sh) else {
                return false;
            };
            if required > source.len() {
                return false;
            }
            for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                for x in rect.left.max(0)..rect.right.min(bounds.right) {
                    let sx = (x - rect.left) as usize;
                    let sy = (y - rect.top) as usize;
                    put(x, y, source[sy * sw + sx], *rop);
                }
            }
        }
        DrawCmd::Line {
            x1,
            y1,
            x2,
            y2,
            color,
            width,
        } => {
            // Bresenham borné : la ligne est d'abord clippée au rectangle du
            // DC (élargi de l'épaisseur du trait), en i64, pour que des
            // coordonnées lointaines ne coûtent ni débordement ni millions
            // d'itérations hors champ.
            let pen = (*width).max(1) as i64;
            let clip = Rect {
                left: 0,
                top: 0,
                right: bounds.right,
                bottom: bounds.bottom,
            };
            let Some((cx1, cy1, cx2, cy2)) =
                clip_line(*x1 as i64, *y1 as i64, *x2 as i64, *y2 as i64, &clip, pen)
            else {
                return true;
            };
            let (mut cx, mut cy) = (cx1, cy1);
            let dx = (cx2 - cx1).abs();
            let dy = -(cy2 - cy1).abs();
            let sx = if cx1 < cx2 { 1 } else { -1 };
            let sy = if cy1 < cy2 { 1 } else { -1 };
            let mut err = dx + dy;
            loop {
                for oy in 0..pen {
                    for ox in 0..pen {
                        let (px, py) = (cx + ox, cy + oy);
                        if px >= 0
                            && py >= 0
                            && px < bounds.right as i64
                            && py < bounds.bottom as i64
                        {
                            put(px as i32, py as i32, *color, RasterOp::Copy);
                        }
                    }
                }
                if cx == cx2 && cy == cy2 {
                    break;
                }
                let e2 = 2 * err;
                if e2 >= dy {
                    err += dy;
                    cx += sx;
                }
                if e2 <= dx {
                    err += dx;
                    cy += sy;
                }
            }
        }
        DrawCmd::RoundRect {
            rect,
            color,
            rx,
            ry,
        } => {
            let w = rect.width();
            let h = rect.height();
            if w > 0 && h > 0 {
                let rx = (*rx).min(w / 2).max(0) as f64;
                let ry = (*ry).min(h / 2).max(0) as f64;
                for y in rect.top.max(0)..rect.bottom.min(bounds.bottom) {
                    for x in rect.left.max(0)..rect.right.min(bounds.right) {
                        let lx = (x - rect.left) as f64;
                        let ly = (y - rect.top) as f64;
                        let inside = if lx < rx && ly < ry {
                            let dx = (rx - lx) / rx;
                            let dy = (ry - ly) / ry;
                            dx * dx + dy * dy <= 1.0
                        } else if lx > w as f64 - rx && ly < ry {
                            let dx = (lx - (w as f64 - rx)) / rx;
                            let dy = (ry - ly) / ry;
                            dx * dx + dy * dy <= 1.0
                        } else if lx < rx && ly > h as f64 - ry {
                            let dx = (rx - lx) / rx;
                            let dy = (ly - (h as f64 - ry)) / ry;
                            dx * dx + dy * dy <= 1.0
                        } else if lx > w as f64 - rx && ly > h as f64 - ry {
                            let dx = (lx - (w as f64 - rx)) / rx;
                            let dy = (ly - (h as f64 - ry)) / ry;
                            dx * dx + dy * dy <= 1.0
                        } else {
                            true
                        };
                        if inside {
                            put(x, y, *color, RasterOp::Copy);
                        }
                    }
                }
            }
        }
        DrawCmd::Text {
            x, y, color, bk, ..
        } => {
            // Texte dans une image hors écran (double tampon des programmes :
            // onglets et barre d'état de Notepad++…), lissé sur le fond.
            let Some((glyphs, advance, line_h)) = glyphs else {
                return true;
            };
            if let Some(bk) = bk {
                for py in (*y).max(0)..(y + line_h).min(bounds.bottom) {
                    for px in (*x).max(0)..(x + advance).min(bounds.right) {
                        put(px, py, *bk, RasterOp::Copy);
                    }
                }
            }
            for (gx, gy, r) in glyphs {
                for row in 0..r.h {
                    let py = y + gy + row as i32;
                    if py < 0 || py >= bounds.bottom {
                        continue;
                    }
                    for col in 0..r.w {
                        let a = r.cov[row * r.w + col] as u32;
                        let px = x + gx + col as i32;
                        if a == 0 || px < 0 || px >= bounds.right {
                            continue;
                        }
                        let (ux, uy) = (px as usize, py as usize);
                        let old = bitmap_color_at(
                            *width,
                            *height,
                            pixels,
                            dib_bits.as_deref(),
                            *dib_bit_count,
                            *dib_stride,
                            *top_down,
                            ux,
                            uy,
                        );
                        let mix = |shift: u32| {
                            let o = (old >> shift) & 0xFF;
                            let n = (color >> shift) & 0xFF;
                            ((n * a + o * (255 - a)) / 255) << shift
                        };
                        set_bitmap_color(
                            *width,
                            *height,
                            pixels,
                            dib_bits,
                            *dib_bit_count,
                            *dib_stride,
                            *top_down,
                            ux,
                            uy,
                            mix(0) | mix(8) | mix(16),
                        );
                    }
                }
            }
        }
        DrawCmd::Blit { .. } => {}
    }
    true
}

/// Clippe le segment (x1,y1)-(x2,y2) au rectangle `clip` élargi de `pen - 1`
/// vers la gauche et le haut (le trait s'étend vers la droite et le bas),
/// par Cohen–Sutherland en i64. `None` si rien n'est visible.
fn clip_line(
    mut x1: i64,
    mut y1: i64,
    mut x2: i64,
    mut y2: i64,
    clip: &Rect,
    pen: i64,
) -> Option<(i64, i64, i64, i64)> {
    let (xmin, ymin) = (clip.left as i64 - (pen - 1), clip.top as i64 - (pen - 1));
    let (xmax, ymax) = (clip.right as i64 - 1, clip.bottom as i64 - 1);
    if xmax < xmin || ymax < ymin {
        return None;
    }
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const BOTTOM: u8 = 4;
    const TOP: u8 = 8;
    let code = |x: i64, y: i64| -> u8 {
        let mut c = 0;
        if x < xmin {
            c |= LEFT;
        } else if x > xmax {
            c |= RIGHT;
        }
        if y < ymin {
            c |= TOP;
        } else if y > ymax {
            c |= BOTTOM;
        }
        c
    };
    let (mut c1, mut c2) = (code(x1, y1), code(x2, y2));
    loop {
        if c1 | c2 == 0 {
            return Some((x1, y1, x2, y2));
        }
        if c1 & c2 != 0 {
            return None;
        }
        let out = if c1 != 0 { c1 } else { c2 };
        let (dx, dy) = (x2 - x1, y2 - y1);
        // Division entière : les coordonnées restent sur la droite à un pixel
        // près, suffisant pour un tracé Bresenham.
        let along = |a: i64, num: i64, den: i64, span: i64| -> i64 {
            (a as i128 + num as i128 * span as i128 / den as i128) as i64
        };
        let (x, y) = if out & TOP != 0 {
            (along(x1, dx, dy, ymin - y1), ymin)
        } else if out & BOTTOM != 0 {
            (along(x1, dx, dy, ymax - y1), ymax)
        } else if out & RIGHT != 0 {
            (xmax, along(y1, dy, dx, xmax - x1))
        } else {
            (xmin, along(y1, dy, dx, xmin - x1))
        };
        if out == c1 {
            x1 = x;
            y1 = y;
            c1 = code(x1, y1);
        } else {
            x2 = x;
            y2 = y;
            c2 = code(x2, y2);
        }
    }
}

fn raster_color(source: u32, destination: u32, rop: RasterOp) -> u32 {
    const RGB_MASK: u32 = 0x00FF_FFFF;
    match rop {
        RasterOp::Copy => source,
        RasterOp::And => source & destination,
        RasterOp::Or => source | destination,
        RasterOp::Xor => source ^ destination,
        RasterOp::Invert => !destination & RGB_MASK,
        RasterOp::CopyInverted => !source & RGB_MASK,
        RasterOp::Clear => 0,
        RasterOp::Set => RGB_MASK,
        RasterOp::SrcErase => (!source & RGB_MASK) & destination,
        RasterOp::NotSrcErase => !(source | destination) & RGB_MASK,
        RasterOp::MergePaint => (!source & RGB_MASK) | destination,
        RasterOp::PatCopy => source, // pattern copié tel quel
        RasterOp::PatPaint => (source | (!destination & RGB_MASK)) & destination, // DPSnoo
        RasterOp::DstCopy => destination,
    }
}

/// Commande qui pose une image entière (copie de bitmap ou de fenêtre).
fn is_image_cmd(cmd: &DrawCmd) -> bool {
    matches!(cmd, DrawCmd::Blit { .. } | DrawCmd::Bitmap { .. })
}

/// GdiFlush : les dessins faits hors `WM_PAINT` sur les fenêtres de ce
/// processus partent à l'écran.
pub fn flush_all_paint_over() {
    let hwnds: Vec<usize> = lock(windows())
        .iter()
        .filter(|(_, w)| !w.painting && !w.pending.is_empty())
        .map(|(&h, _)| h)
        .collect();
    for hwnd in hwnds {
        flush_paint_over(hwnd);
    }
}

/// Retire les commandes accumulées depuis le dernier `EndPaint`.
pub fn take_pending(hwnd: usize) -> Vec<DrawCmd> {
    lock(windows())
        .get_mut(&hwnd)
        .map(|w| std::mem::take(&mut w.pending))
        .unwrap_or_default()
}

// ---------- Backend ----------

/// Événement traduit par le backend, à publier dans la file du thread cible.
pub struct PendingEvent {
    pub hwnd: usize,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
}

/// Titre d'un menu de la barre : libellé sans « & » et étendue horizontale
/// en coordonnées fenêtre logiques.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuBarItem {
    pub label: String,
    pub left: i32,
    pub right: i32,
    pub enabled: bool,
}

/// Rendu et événements hôte. `headless` n'affiche rien ; X11 traduit les
/// événements du serveur en messages Win32.
pub const WINDOW_NORMAL: u8 = 0;
pub const WINDOW_MINIMIZED: u8 = 1;
pub const WINDOW_MAXIMIZED: u8 = 2;

/// wParam de WM_SIZE selon l'état de la fenêtre : SIZE_RESTORED (0),
/// SIZE_MINIMIZED (1), SIZE_MAXIMIZED (2). WPF ne tient son WindowState
/// qu'ainsi : sans lui, le bouton « Restaurer » restait sans effet (#1328).
pub fn size_kind(state: u8) -> usize {
    match state {
        WINDOW_MAXIMIZED => 2,
        WINDOW_MINIMIZED => 1,
        _ => 0,
    }
}

pub trait GuiBackend: Send {
    /// Traduit les événements hôte en messages Win32 (vidés à chaque boucle).
    fn pump(&mut self) -> Vec<PendingEvent> {
        Vec::new()
    }
    fn create_window(&mut self, _hwnd: usize, _title: &str, _rect: Rect) {}
    /// Contrôle `WS_CHILD` : sous-fenêtre placée dans la zone client du
    /// parent (coordonnées relatives). Sans prise en charge, fenêtre isolée.
    fn create_child_window(&mut self, hwnd: usize, _parent: usize, rect: Rect) {
        self.create_window(hwnd, "", rect);
    }
    fn destroy_window(&mut self, _hwnd: usize) {}
    /// Taille logique d'une fenêtre X11 d'un autre processus (#1228).
    fn foreign_size(&mut self, _xid: u64) -> Option<(i32, i32)> {
        None
    }
    /// Image (pixels COLORREF, rectangle logique) posée sur la fenêtre X11
    /// d'un autre processus (#1345).
    fn paint_foreign(&mut self, _xid: u64, _rect: Rect, _pixels: &[u32], _rop: RasterOp) {}
    /// Marque une fenêtre X où une chaîne d'échange Direct3D présente (#1374).
    fn mark_direct3d(&mut self, _xid: u64) {}
    fn show_window(&mut self, _hwnd: usize, _show: bool) {}
    fn set_title(&mut self, _hwnd: usize, _title: &str) {}
    fn set_window_rect(&mut self, _hwnd: usize, _rect: Rect) {}
    fn set_focus(&mut self, _hwnd: usize) {}
    /// Barre de menus d'une fenêtre de premier niveau : libellés des menus
    /// et hauteur (0 : pas de barre). La zone client commence sous la barre.
    fn set_menu_bar(
        &mut self,
        _hwnd: usize,
        _items: &[MenuBarItem],
        _height: i32,
        _hot: Option<usize>,
    ) {
    }
    /// Passe une fenêtre devant ses voisines (liste déroulante ouverte).
    fn raise_window(&mut self, _hwnd: usize) {}
    /// Passe une fenêtre derrière ses voisines (ordre Win32 des enfants).
    fn lower_window(&mut self, _hwnd: usize) {}
    /// Place une fenêtre juste derrière sa voisine `sibling` (SetWindowPos).
    fn stack_below(&mut self, _hwnd: usize, _sibling: usize) {}
    /// SetParent entre deux fenêtres connues : la sous-fenêtre suit son
    /// nouveau parent, à la place `rect` (coordonnées client du parent).
    fn reparent_window(&mut self, _hwnd: usize, _parent: usize, _rect: Rect) {}
    /// Fenêtre surgissante (menu) : sans cadre, hors du gestionnaire de
    /// fenêtres. À appeler avant de l'afficher.
    fn set_popup(&mut self, _hwnd: usize) {}
    /// Forme d'une fenêtre en couches (UpdateLayeredWindow avec alpha) :
    /// seuls ces rectangles (coordonnées client) restent visibles et
    /// cliquables ; `None` rend la fenêtre rectangulaire.
    fn set_shape(&mut self, _hwnd: usize, _rects: Option<&[Rect]>) {}
    /// Forme X (SHAPE) posée sur la fenêtre X11 d'un autre processus (#1357).
    fn shape_foreign(&mut self, _xid: u64, _rects: Option<&[Rect]>) {}
    /// Cadre du gestionnaire de fenêtres d'une fenêtre de premier niveau :
    /// 0 aucun, 1 bordure seule, 2 complet (barre de titre). Une WS_POPUP
    /// sans WS_CAPTION n'a pas de barre de titre sous Windows.
    fn set_decorations(&mut self, _hwnd: usize, _level: u8) {}
    /// Fenêtre possédée (hwndParent d'une fenêtre de premier niveau) : le
    /// gestionnaire la garde au-dessus de sa propriétaire (WM_TRANSIENT_FOR).
    fn set_owner(&mut self, _hwnd: usize, _owner: usize) {}
    /// Fenêtre possédée avec barre de titre : boîte de dialogue pour le
    /// gestionnaire (_NET_WM_WINDOW_TYPE_DIALOG).
    fn set_dialog_type(&mut self, _hwnd: usize) {}
    /// Fenêtre outil avec barre de titre (WS_EX_TOOLWINDOW) : barre d'outils
    /// détachée pour le gestionnaire (_NET_WM_WINDOW_TYPE_TOOLBAR), dont la
    /// barre de titre n'a que le bouton Fermer (#1761).
    fn set_tool_type(&mut self, _hwnd: usize) {}
    /// Épaisseur du cadre du gestionnaire autour d'une fenêtre de premier
    /// niveau : gauche, droite, haut, bas (_NET_FRAME_EXTENTS, #1752).
    fn frame_extents(&mut self, _hwnd: usize) -> Option<[i32; 4]> {
        None
    }
    /// Affichage sans activation (SW_SHOWNOACTIVATE, SW_SHOWNA) : le
    /// gestionnaire ne doit pas lui donner le focus en l'affichant.
    fn set_no_initial_focus(&mut self, _hwnd: usize) {}
    /// État d'une fenêtre de premier niveau (ShowWindow, WM_SYSCOMMAND) :
    /// WINDOW_NORMAL, WINDOW_MINIMIZED ou WINDOW_MAXIMIZED, demandé au
    /// gestionnaire de fenêtres.
    fn set_window_state(&mut self, _hwnd: usize, _state: u8) {}
    /// État actuel vu du gestionnaire (IsIconic, IsZoomed).
    fn window_state(&mut self, _hwnd: usize) -> u8 {
        WINDOW_NORMAL
    }
    /// Déplacement (`edge` = HTCAPTION) ou redimensionnement (HTLEFT…
    /// HTBOTTOMRIGHT) mené par le gestionnaire de fenêtres à la souris, comme
    /// la boucle modale de DefWindowProc sur WM_NCLBUTTONDOWN.
    fn begin_move_resize(&mut self, _hwnd: usize, _edge: u32) {}
    /// Capture du pointeur pour un menu : les clics hors du programme
    /// arrivent à `hwnd` (coordonnées hors de la fenêtre).
    fn grab_pointer(&mut self, _hwnd: usize, _grab: bool) {}
    /// Raccourci global (RegisterHotKey) : prise de la combinaison sur tout
    /// l'écran. Faux si un autre programme ou le gestionnaire de fenêtres la
    /// tient déjà. Sans affichage réel, rien à prendre : accepté.
    fn grab_hotkey(&mut self, _mods: u32, _vk: u32, _grab: bool) -> bool {
        true
    }
    fn invalidate(&mut self, _hwnd: usize) {}
    /// Lit une zone de la fenêtre en pixels COLORREF (pour les blits vers un DC mémoire).
    fn capture(&mut self, _hwnd: usize, _rect: Rect) -> Option<BitmapSnapshot> {
        None
    }
    /// Lit une zone de l'écran entier (racine), fenêtres des autres programmes comprises.
    fn capture_screen(&mut self, _rect: Rect) -> Option<BitmapSnapshot> {
        None
    }
    /// Joue la liste de commandes GDI d'une fenêtre (thème + contenu). Le
    /// backend X11 conserve la liste pour la rejouer à chaque Expose; headless
    /// ignore le rendu.
    fn draw(&mut self, _hwnd: usize, _cmds: &[DrawCmd]) {}
    /// Pose des commandes par-dessus le contenu actuel de la fenêtre (dessin
    /// hors `WM_PAINT`) ; par défaut, comme `draw`.
    fn draw_over(&mut self, hwnd: usize, cmds: &[DrawCmd]) {
        self.draw(hwnd, cmds)
    }
    /// Attend des événements hôte pendant `timeout_ms`; renvoie `true` si le
    /// backend peut en produire (X11) et `false` s'il est headless, auquel cas
    /// une boucle `GetMessage` vide doit se terminer.
    fn wait(&mut self, _timeout_ms: u32) -> bool {
        false
    }
    /// Descripteur de la connexion hôte à surveiller par `poll` (X11), `None`
    /// en headless : `GetMessage` bloque alors sans tenir le verrou du backend.
    fn event_fd(&mut self) -> Option<i32> {
        None
    }
    /// Lit sans bloquer les événements déjà arrivés ; vrai si au moins un
    /// attend d'être pompé.
    fn poll_pending(&mut self) -> bool {
        false
    }
    /// Taille actuelle de l'écran en unités logiques Win32. Elle suit les
    /// changements de résolution (RandR) pour SM_CXSCREEN/SM_CYSCREEN.
    fn screen_size(&mut self) -> Option<(i32, i32)> {
        None
    }
    /// Zone de travail (écran moins les barres qui réservent leur place) en
    /// unités logiques Win32, pour SPI_GETWORKAREA.
    fn work_area(&mut self) -> Option<Rect> {
        None
    }
    /// Écrans branchés et allumés (vide si le backend ne sait pas).
    fn monitors(&mut self) -> Vec<Monitor> {
        Vec::new()
    }
    /// Identifiant de la fenêtre native (X11) d'un HWND, pour Vulkan/GLX.
    fn native_window(&mut self, _hwnd: usize) -> Option<u64> {
        None
    }
    /// Largeur et hauteur logiques d'un texte dans une police GDI.
    fn measure_text(&mut self, _font: &FontSpec, _text: &str) -> Option<(i32, i32)> {
        None
    }
}

struct HeadlessBackend;

impl GuiBackend for HeadlessBackend {}

/// Construit le backend : X11 si `DISPLAY` est présent et qu'on n'a pas forcé
/// `WINLUX_GUI=headless`; sinon headless. Retombe sur headless si la connexion
/// échoue.
fn build_backend() -> Box<dyn GuiBackend> {
    let force_headless = std::env::var("WINLUX_GUI")
        .map(|v| v == "headless")
        .unwrap_or(false);
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

/// Taille de l'écran (logique) ou 1920×1080 sans affichage réel.
/// Mesure d'un texte (GetTextExtentPoint32) : le backend si possible, sinon
/// une estimation à ~0,55 em par caractère.
pub fn measure_text(font: &FontSpec, text: &str) -> (i32, i32) {
    let mut out = None;
    with_backend(|b| out = b.measure_text(font, text));
    out.unwrap_or_else(|| {
        let h = if font.height == 0 {
            13
        } else {
            font.height.abs()
        };
        (text.chars().count() as i32 * h * 55 / 100, h)
    })
}

/// Zone client d'une fenêtre d'un autre processus : taille de sa fenêtre
/// X11, lue sur le serveur (#1228).
pub fn foreign_client_rect(hwnd: usize) -> Option<Rect> {
    let f = foreign_window(hwnd)?;
    let mut size = None;
    with_backend(|b| size = b.foreign_size(f.xid));
    let (w, h) = size?;
    Some(Rect {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    })
}

/// Fenêtre X11 réelle d'un HWND (`None` en headless ou HWND inconnu).
/// Direct3D en a besoin pour créer un contexte GLX ou une surface Vulkan.
pub fn native_window(hwnd: usize) -> Option<u64> {
    let mut id = None;
    with_backend(|b| id = b.native_window(hwnd));
    // Fenêtre d'un autre processus du compte : les fenêtres X11 sont
    // communes à tous les clients du serveur, Direct3D peut y présenter
    // (processus GPU de Chromium, #1228).
    id.or_else(|| foreign_window(hwnd).map(|f| f.xid))
}

/// Une chaîne d'échange Direct3D présente dans la fenêtre X `xid`, de ce
/// processus ou d'un autre : son propriétaire ne doit plus la repeindre en
/// GDI à chaque exposition, seulement demander WM_PAINT (#1374).
pub fn mark_direct3d_window(xid: u64) {
    with_backend(|b| b.mark_direct3d(xid));
}

pub fn monitors() -> Vec<Monitor> {
    let mut list = Vec::new();
    with_backend(|b| list = b.monitors());
    list
}

/// Zone de travail : l'écran entier si le gestionnaire de fenêtres n'en
/// publie pas.
pub fn work_area() -> Rect {
    let mut area = None;
    with_backend(|b| area = b.work_area());
    area.unwrap_or_else(|| {
        let (right, bottom) = screen_size();
        Rect {
            left: 0,
            top: 0,
            right,
            bottom,
        }
    })
}

pub fn screen_size() -> (i32, i32) {
    let mut size = None;
    with_backend(|b| size = b.screen_size());
    size.unwrap_or((1920, 1080))
}

/// Le programme a-t-il le focus du bureau ? Sert à n'envoyer
/// WM_ACTIVATEAPP qu'au passage d'un programme à l'autre.
static APP_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// WM_ACTIVATEAPP à toutes les fenêtres de premier niveau du programme
/// quand il gagne ou perd le focus face à un autre programme. Notepad++ y
/// vérifie si ses fichiers ont changé sur le disque (#769).
fn activate_app(active: bool) {
    use std::sync::atomic::Ordering;
    if APP_ACTIVE.swap(active, Ordering::Relaxed) == active {
        return;
    }
    let mut tops: Vec<usize> = lock(windows())
        .iter()
        .filter(|(_, w)| w.parent == 0 || w.style & 0x4000_0000 == 0)
        .map(|(&h, _)| h)
        .collect();
    tops.sort_unstable();
    for top in tops {
        post_to_window(top, Msg::new(top, WM_ACTIVATEAPP, active as usize, 0));
    }
}

/// Fenêtre de premier niveau active du programme (0 : aucune).
static ACTIVE_TOP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Fenêtre de premier niveau qui contient `hwnd`.
pub fn top_level_of(hwnd: usize) -> usize {
    let map = lock(windows());
    let mut h = hwnd;
    for _ in 0..64 {
        match map.get(&h) {
            Some(w) if w.parent != 0 && w.style & 0x4000_0000 != 0 => h = w.parent,
            Some(_) => return h,
            None => return 0,
        }
    }
    h
}

/// Changement de fenêtre active, comme Windows : l'ancienne reçoit
/// WM_NCACTIVATE(FALSE) puis WM_ACTIVATE(WA_INACTIVE), la nouvelle
/// WM_NCACTIVATE(TRUE) puis WM_ACTIVATE(WA_ACTIVE), avant WM_SETFOCUS.
/// SDL ne donne le focus clavier à sa fenêtre qu'à WM_ACTIVATE : sans lui,
/// Xonotic se croyait inactif et ne présentait plus que son écran de
/// chargement (#1421).
/// SetFocus vers une autre fenêtre de premier niveau : la fenêtre active
/// change tout de suite, et l'appelant envoie lui-même (de façon synchrone,
/// comme Windows) WM_NCACTIVATE et WM_ACTIVATE. Renvoie l'ancienne fenêtre
/// active, ou `None` si `new_top` l'était déjà.
///
/// Postés plus tard depuis le FocusIn de X11, ces messages arrivaient
/// périmés : wxWidgets rend le focus à la fenêtre que WM_ACTIVATE déclare
/// active, ce qui relançait l'échange suivant. Audacity et sa boîte de
/// bienvenue se renvoyaient ainsi l'activation sans fin, et la boîte
/// n'était dessinée qu'en partie (#1504).
pub fn activate_top_for_focus(new_top: usize) -> Option<usize> {
    use std::sync::atomic::Ordering;
    let old = ACTIVE_TOP.swap(new_top, Ordering::Relaxed);
    (old != new_top).then_some(old)
}

/// Fenêtre de premier niveau qu'un clic sur `hwnd` doit activer, parce que
/// le gestionnaire de fenêtres ne le fera pas (#1804). Sous Windows, un clic
/// sur une fenêtre sans WS_EX_NOACTIVATE l'active avant WM_LBUTTONDOWN ;
/// chez nous, cette activation vient du FocusIn que donne le gestionnaire,
/// et une fenêtre surgissante hors gestionnaire (override-redirect) n'en
/// reçoit jamais. La boîte « Erreur du service Steam » (vguiPopupWindow,
/// WS_POPUP + WS_EX_TOOLWINDOW) restait ainsi inactive, et vgui ignorait
/// ses boutons. Les menus et listes intégrés de Winlux ne s'activent pas.
pub fn click_activation_target(hwnd: usize) -> Option<usize> {
    use std::sync::atomic::Ordering;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_DISABLED: u32 = 0x0800_0000;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    let top = top_level_of(hwnd);
    if top == 0 || ACTIVE_TOP.load(Ordering::Relaxed) == top {
        return None;
    }
    let map = lock(windows());
    let w = map.get(&top)?;
    let eligible = w.visible
        && w.style & (WS_CHILD | WS_DISABLED) == 0
        && w.ex_style & WS_EX_NOACTIVATE == 0
        && crate::dlls::user32::floating_popup(w.style, w.ex_style)
        && crate::dlls::controls::builtin_class(&w.class).is_none()
        && crate::dlls::controls::builtin_class_of_proc(w.wnd_proc).is_none();
    eligible.then_some(top)
}

/// Fenêtre de premier niveau active (0 : le programme n'a pas le focus).
pub fn active_top() -> usize {
    ACTIVE_TOP.load(std::sync::atomic::Ordering::Relaxed)
}

fn change_active_top(new_top: usize) {
    use std::sync::atomic::Ordering;
    let old = ACTIVE_TOP.swap(new_top, Ordering::Relaxed);
    if old == new_top {
        return;
    }
    if old != 0 && is_window(old) {
        post_to_window(old, Msg::new(old, WM_NCACTIVATE, 0, 0));
        post_to_window(old, Msg::new(old, WM_ACTIVATE, 0, new_top as isize));
    }
    if new_top != 0 {
        post_to_window(new_top, Msg::new(new_top, WM_NCACTIVATE, 1, 0));
        post_to_window(new_top, Msg::new(new_top, WM_ACTIVATE, 1, old as isize));
        crate::dlls::chromium_win32::notify_win_event(
            crate::dlls::chromium_win32::EVENT_SYSTEM_FOREGROUND,
            new_top,
        );
    }
}

/// Fenêtre de premier niveau désactivée (propriétaire d'une boîte modale) :
/// sous Windows, ni elle ni ses filles ne reçoivent clic ou clavier. Une
/// fille seule désactivée garde le comportement des contrôles.
fn input_disabled(hwnd: usize) -> bool {
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_DISABLED: u32 = 0x0800_0000;
    let map = lock(windows());
    let mut h = hwnd;
    for _ in 0..64 {
        let Some(w) = map.get(&h) else {
            return false;
        };
        if w.style & WS_CHILD == 0 || w.parent == 0 {
            return w.style & WS_DISABLED != 0;
        }
        h = w.parent;
    }
    false
}

/// Message du backend qu'une fenêtre désactivée par une boîte modale ne doit
/// pas recevoir : `None` s'il est livré, `Some(true)` s'il est refusé et
/// ramène la boîte au premier plan, `Some(false)` s'il est seulement refusé.
///
/// La croix du gestionnaire de fenêtres (WM_DELETE_WINDOW, traduit en
/// WM_CLOSE) en fait partie : sous Windows, la croix d'une fenêtre désactivée
/// est inerte. Audacity fermait sinon le projet sous sa boîte « Attention »
/// encore modale ; la boucle modale de la boîte reprenait ensuite sur des
/// objets libérés et le tas était corrompu (#1376).
fn refused_while_disabled(hwnd: usize, message: u32) -> Option<bool> {
    let mouse = (0x0200..=0x020E).contains(&message)
        || (0x00A0..=0x00AD).contains(&message)
        || (0x0245..=0x024A).contains(&message);
    if !(mouse || message == WM_SETFOCUS || message == WM_CLOSE) || !input_disabled(hwnd) {
        return None;
    }
    let press = matches!(message, 0x0201 | 0x0204 | 0x0207 | 0x00A1 | 0x00A4 | 0x00A7);
    Some(press || message == WM_SETFOCUS || message == WM_CLOSE)
}

/// Boîte active d'une fenêtre désactivée : la fenêtre possédée, visible et
/// active la plus récente (la boîte modale qui l'a désactivée).
fn enabled_owned_popup(hwnd: usize) -> Option<usize> {
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_DISABLED: u32 = 0x0800_0000;
    let map = lock(windows());
    let mut top = hwnd;
    for _ in 0..64 {
        match map.get(&top) {
            Some(w) if w.style & WS_CHILD != 0 && w.parent != 0 => top = w.parent,
            _ => break,
        }
    }
    map.iter()
        .filter(|(&h, w)| {
            h != top && w.parent == top && w.visible && w.style & (WS_CHILD | WS_DISABLED) == 0
        })
        .map(|(&h, _)| h)
        .max()
}

pub const WM_MOUSELEAVE: u32 = 0x02A3;

/// Fenêtres qui ont demandé WM_MOUSELEAVE par TrackMouseEvent(TME_LEAVE)
/// (#1254). Le suivi est à usage unique, comme sous Windows : il s'arrête
/// dès que le message est posté.
fn leave_tracking() -> &'static Mutex<Vec<usize>> {
    static T: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Vec::new()))
}

/// Dernière fenêtre qui a reçu un message souris (0 : aucune).
static LAST_MOUSE_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Le pointeur est-il sur `hwnd`, d'après le dernier message souris ?
pub fn pointer_over(hwnd: usize) -> bool {
    LAST_MOUSE_HWND.load(std::sync::atomic::Ordering::Relaxed) == hwnd
}

/// TrackMouseEvent(TME_LEAVE) : suivre `hwnd`, ou poster tout de suite
/// WM_MOUSELEAVE si le pointeur n'est déjà plus dessus.
pub fn track_mouse_leave(hwnd: usize, pointer_inside: bool) {
    let mut t = lock(leave_tracking());
    t.retain(|&h| h != hwnd);
    if pointer_inside {
        t.push(hwnd);
    } else {
        drop(t);
        post_to_window(hwnd, Msg::new(hwnd, WM_MOUSELEAVE, 0, 0));
    }
}

/// TME_CANCEL : ne plus suivre `hwnd`.
pub fn cancel_mouse_leave(hwnd: usize) {
    lock(leave_tracking()).retain(|&h| h != hwnd);
}

/// TME_QUERY : `hwnd` attend-il un WM_MOUSELEAVE ?
pub fn mouse_leave_tracked(hwnd: usize) -> bool {
    lock(leave_tracking()).contains(&hwnd)
}

pub const WM_MOUSEHOVER: u32 = 0x02A1;

/// Demande TrackMouseEvent(TME_HOVER) en cours (#1591) : WM_MOUSEHOVER est
/// posté quand le pointeur reste `ms` millisecondes dans le rectangle de
/// survol (SPI_GETMOUSEHOVERWIDTH/HEIGHT, 4 pixels) autour de `anchor`.
/// À usage unique, comme sous Windows ; les info-bulles WinForms (Paint.NET)
/// n'apparaissent qu'à ce message.
struct Hover {
    hwnd: usize,
    ms: u32,
    anchor: (i32, i32),
    since: std::time::Instant,
}

fn hover_tracking() -> &'static Mutex<Vec<Hover>> {
    static T: OnceLock<Mutex<Vec<Hover>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Vec::new()))
}

static HOVER_THREAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Délai par défaut de survol (HOVER_DEFAULT), SPI_GETMOUSEHOVERTIME.
pub const HOVER_DEFAULT_MS: u32 = 400;

/// TrackMouseEvent(TME_HOVER) : suivre le survol de `hwnd`.
pub fn track_mouse_hover(hwnd: usize, ms: u32) {
    let ms = if ms == u32::MAX {
        HOVER_DEFAULT_MS
    } else {
        ms.max(1)
    };
    let mut t = lock(hover_tracking());
    t.retain(|h| h.hwnd != hwnd);
    t.push(Hover {
        hwnd,
        ms,
        anchor: cursor_screen_pos(),
        since: std::time::Instant::now(),
    });
    if !HOVER_THREAD.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("winlux-survol".into())
            .spawn(hover_loop);
        if spawned.is_err() {
            HOVER_THREAD.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// TME_CANCEL | TME_HOVER : ne plus suivre le survol de `hwnd`.
pub fn cancel_mouse_hover(hwnd: usize) {
    lock(hover_tracking()).retain(|h| h.hwnd != hwnd);
}

/// TME_QUERY : délai de survol suivi pour `hwnd`, s'il y en a un.
pub fn mouse_hover_tracked(hwnd: usize) -> Option<u32> {
    lock(hover_tracking())
        .iter()
        .find(|h| h.hwnd == hwnd)
        .map(|h| h.ms)
}

/// Un pas du suivi : les demandes dont le pointeur est resté immobile assez
/// longtemps sont retirées et rendues avec la position du pointeur ; celles
/// dont la fenêtre n'est plus sous le pointeur sont abandonnées.
fn hover_tick(
    list: &mut Vec<Hover>,
    pos: (i32, i32),
    now: std::time::Instant,
    over: impl Fn(usize) -> bool,
) -> Vec<(usize, (i32, i32))> {
    const HALF: i32 = 2; // la moitié des 4 pixels du rectangle de survol
    let mut fired = vec![];
    list.retain_mut(|h| {
        if !over(h.hwnd) {
            return false;
        }
        if (pos.0 - h.anchor.0).abs() > HALF || (pos.1 - h.anchor.1).abs() > HALF {
            h.anchor = pos;
            h.since = now;
            return true;
        }
        if now.duration_since(h.since).as_millis() >= u128::from(h.ms) {
            fired.push((h.hwnd, pos));
            return false;
        }
        true
    });
    fired
}

fn hover_loop() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(25));
        let (fired, stop) = {
            let mut t = lock(hover_tracking());
            let fired = hover_tick(
                &mut t,
                cursor_screen_pos(),
                std::time::Instant::now(),
                pointer_over,
            );
            let stop = t.is_empty();
            if stop {
                // Sous le verrou : une demande arrivée ensuite relance un fil.
                HOVER_THREAD.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            (fired, stop)
        };
        for (hwnd, (x, y)) in fired {
            let (left, top) = screen_rect(hwnd).map(|r| (r.left, r.top)).unwrap_or((0, 0));
            let lparam = (((y - top) as u16 as usize) << 16) | ((x - left) as u16 as usize);
            post_to_window(hwnd, Msg::new(hwnd, WM_MOUSEHOVER, 0, lparam as isize));
        }
        if stop {
            return;
        }
    }
}

/// Le pointeur est maintenant sur `now` (0 : hors de toute fenêtre du
/// programme) : les fenêtres suivies qu'il a quittées reçoivent
/// WM_MOUSELEAVE. Sans lui, WPF rapportait la souris à la première
/// fenêtre survolée et ignorait les clics dans les suivantes.
fn mouse_moved_to(now: usize) {
    LAST_MOUSE_HWND.store(now, std::sync::atomic::Ordering::Relaxed);
    let left: Vec<usize> = {
        let mut t = lock(leave_tracking());
        let left = t.iter().copied().filter(|&h| h != now).collect();
        t.retain(|&h| h == now);
        left
    };
    for h in left {
        post_to_window(h, Msg::new(h, WM_MOUSELEAVE, 0, 0));
    }
}

/// Interroge le backend et publie les messages traduits dans les files cibles.
pub fn pump_backend() {
    let events = {
        let mut b = lock(backend());
        b.pump()
    };
    // Un FocusOut suivi d'un FocusIn dans le même lot : le focus change de
    // fenêtre sans quitter le programme.
    let refocused = events.iter().any(|e| e.message == WM_SETFOCUS);
    for e in &events {
        if let Some((vk, down)) = mouse_button_transition(e.message, e.wparam) {
            update_async_key_state(vk, down);
        }
    }
    // Stylet (#1199) : chaque message souris qu'il produit est précédé de
    // son WM_POINTER*, et marqué « promu » (GetMessageExtraInfo).
    let events: Vec<(PendingEvent, u32)> = events
        .into_iter()
        .flat_map(|e| {
            let origin = screen_rect(e.hwnd).map(|r| (r.left, r.top));
            match origin.and_then(|o| crate::dlls::pointer::pointer_for_mouse(&e, o)) {
                Some(p) => {
                    let mark = crate::dlls::pointer::promoted_mark(&p);
                    vec![(p, 0), (e, mark)]
                }
                None => vec![(e, 0)],
            }
        })
        .collect();
    for (e, promoted) in events {
        if e.message == WM_HOTKEY && e.hwnd == 0 {
            hotkey_pressed(e.lparam as u32, e.wparam as u32);
            continue;
        }
        // Fenêtre désactivée par une boîte modale : ni clic ni focus. Un
        // clic ou une activation ramène la boîte au premier plan, comme sous
        // Windows. Le clic livré à la fenêtre principale d'Audacity pendant
        // la boîte Tonalité y laissait un geste à moitié fait, et la piste
        // générée n'était plus jamais redessinée (#1120).
        let mouse = (0x0200..=0x020E).contains(&e.message)
            || (0x00A0..=0x00AD).contains(&e.message)
            || (0x0245..=0x024A).contains(&e.message);
        // Souris sur une autre fenêtre (hors molette, livrée à la fenêtre
        // qui a le focus), ou sortie de la fenêtre signalée par le backend.
        if e.message == WM_MOUSELEAVE {
            if mouse_leave_tracked(e.hwnd) {
                cancel_mouse_leave(e.hwnd);
                post_to_window(e.hwnd, Msg::new(e.hwnd, WM_MOUSELEAVE, 0, 0));
            }
            continue;
        }
        if mouse && !matches!(e.message, 0x020A | 0x020E) {
            mouse_moved_to(e.hwnd);
        }
        if let Some(raise) = refused_while_disabled(e.hwnd, e.message) {
            if raise {
                if let Some(popup) = enabled_owned_popup(e.hwnd) {
                    let mut b = lock(backend());
                    b.raise_window(popup);
                    b.set_focus(popup);
                }
            }
            continue;
        }
        if e.message == WM_SETFOCUS {
            activate_app(true);
            change_active_top(top_level_of(e.hwnd));
            let change = set_focus_from_backend(e.hwnd);
            if let Some(old) = change {
                if old != 0 {
                    post_to_window(old, Msg::new(old, WM_KILLFOCUS, e.hwnd, 0));
                }
                post_to_window(e.hwnd, Msg::new(e.hwnd, WM_SETFOCUS, old, 0));
            }
            continue;
        }
        if e.message == WM_KILLFOCUS {
            clear_thread_keyboard_state_for_window(e.hwnd);
            clear_async_key_down_states();
            let cleared = clear_focus_from_backend(e.hwnd);
            if cleared {
                post_to_window(e.hwnd, Msg::new(e.hwnd, WM_KILLFOCUS, 0, 0));
            }
            if !refocused {
                change_active_top(0);
                activate_app(false);
            }
            continue;
        }
        if e.message == WM_SIZE {
            // Réduite : Windows annonce 0x0, la taille restaurée est gardée.
            if e.wparam != SIZE_MINIMIZED {
                let packed = e.lparam as u32;
                update_window_size(e.hwnd, (packed & 0xFFFF) as i32, (packed >> 16) as i32);
            }
            geometry_win_events(e.hwnd, Some(e.wparam));
        } else if e.message == WM_MOVE {
            let packed = e.lparam as u32;
            let x = (packed as u16 as i16) as i32;
            let y = ((packed >> 16) as u16 as i16) as i32;
            update_window_position(e.hwnd, x, y);
            geometry_win_events(e.hwnd, None);
        }
        // Taille ou position changée par le gestionnaire de fenêtres (agrandir,
        // restaurer, glisser) : marquée pour que DispatchMessage envoie
        // d'abord WM_WINDOWPOSCHANGED, comme Windows (#1476).
        let geometry = matches!(e.message, WM_SIZE | WM_MOVE) && promoted == 0;
        let m = Msg {
            hwnd: e.hwnd,
            message: e.message,
            wparam: e.wparam,
            lparam: e.lparam,
            time: 0,
            pt: Point::default(),
            l_private: if geometry { BACKEND_GEOMETRY } else { promoted },
        };
        if e.message == WM_PAINT {
            request_paint(e.hwnd);
        } else {
            post_to_window(e.hwnd, m);
        }
    }
}

const SIZE_MINIMIZED: usize = 1;

/// Événements SetWinEventHook d'un changement de géométrie venu du
/// gestionnaire de fenêtres (fenêtres de premier niveau) : déplacement ou
/// taille (EVENT_OBJECT_LOCATIONCHANGE), début et fin de réduction
/// (EVENT_SYSTEM_MINIMIZESTART / MINIMIZEEND), comme Windows (#1476, #1498).
fn geometry_win_events(hwnd: usize, size_kind: Option<usize>) {
    use crate::dlls::chromium_win32 as w;
    if window_parent(hwnd).unwrap_or(0) != 0 {
        return;
    }
    static MINIMIZED: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    if let Some(kind) = size_kind {
        let mut minimized = lock(&MINIMIZED);
        let was = minimized.contains(&hwnd);
        let now = kind == SIZE_MINIMIZED;
        if now && !was {
            minimized.push(hwnd);
            drop(minimized);
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZESTART, hwnd);
            return;
        }
        if was && !now {
            minimized.retain(|&h| h != hwnd);
            drop(minimized);
            w::notify_win_event(w::EVENT_SYSTEM_MINIMIZEEND, hwnd);
            // Restaurée : tout est à redessiner, comme sous Windows.
            request_paint(hwnd);
        }
    }
    w::notify_win_event(w::EVENT_OBJECT_LOCATIONCHANGE, hwnd);
}

// ---------- Raccourcis globaux (RegisterHotKey) ----------

pub const WM_HOTKEY: u32 = 0x0312;
pub const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;
pub const ERROR_HOTKEY_NOT_REGISTERED: u32 = 1419;
const ERROR_INVALID_WINDOW_HANDLE: u32 = 1400;

struct HotKey {
    hwnd: usize,
    id: i32,
    /// MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN (sans MOD_NOREPEAT).
    mods: u32,
    vk: u32,
    thread: ThreadId,
}

fn hotkeys() -> &'static Mutex<Vec<HotKey>> {
    static H: OnceLock<Mutex<Vec<HotKey>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(Vec::new()))
}

/// RegisterHotKey : la combinaison est refusée (ERROR_HOTKEY_ALREADY_REGISTERED)
/// si un raccourci du programme ou un autre client de l'affichage (le
/// gestionnaire de fenêtres, un autre programme) la tient déjà.
pub fn register_hotkey(hwnd: usize, id: i32, mods: u32, vk: u32) -> Result<(), u32> {
    if hwnd != 0 && !is_window(hwnd) {
        return Err(ERROR_INVALID_WINDOW_HANDLE);
    }
    let mods = mods & 0xF;
    let vk = vk & 0xFF;
    if lock(hotkeys()).iter().any(|h| h.mods == mods && h.vk == vk) {
        return Err(ERROR_HOTKEY_ALREADY_REGISTERED);
    }
    let mut grabbed = false;
    with_backend(|b| grabbed = b.grab_hotkey(mods, vk, true));
    if !grabbed {
        return Err(ERROR_HOTKEY_ALREADY_REGISTERED);
    }
    lock(hotkeys()).push(HotKey {
        hwnd,
        id,
        mods,
        vk,
        thread: std::thread::current().id(),
    });
    Ok(())
}

/// UnregisterHotKey : `hwnd` nul désigne les raccourcis du thread appelant.
pub fn unregister_hotkey(hwnd: usize, id: i32) -> Result<(), u32> {
    let me = std::thread::current().id();
    let (mods, vk, still_used) = {
        let mut list = lock(hotkeys());
        let Some(pos) = list
            .iter()
            .position(|h| h.hwnd == hwnd && h.id == id && (hwnd != 0 || h.thread == me))
        else {
            return Err(ERROR_HOTKEY_NOT_REGISTERED);
        };
        let h = list.remove(pos);
        let used = list.iter().any(|o| o.mods == h.mods && o.vk == h.vk);
        (h.mods, h.vk, used)
    };
    if !still_used {
        with_backend(|b| {
            b.grab_hotkey(mods, vk, false);
        });
    }
    Ok(())
}

/// Combinaison prise reçue de l'affichage : WM_HOTKEY à la fenêtre (ou au
/// thread si `hwnd` est nul), wParam = id, lParam = MAKELPARAM(mods, vk).
pub fn hotkey_pressed(mods: u32, vk: u32) -> bool {
    let target = lock(hotkeys())
        .iter()
        .find(|h| h.mods == mods & 0xF && h.vk == vk)
        .map(|h| (h.hwnd, h.id, h.thread));
    let Some((hwnd, id, thread)) = target else {
        return false;
    };
    let lparam = ((vk << 16) | (mods & 0xF)) as isize;
    let msg = Msg::new(hwnd, WM_HOTKEY, id as u32 as usize, lparam);
    if hwnd == 0 {
        post_to_thread(thread, msg)
    } else {
        post_to_window(hwnd, msg)
    }
}

/// Attend des événements hôte (au plus `timeout_ms`). Renvoie `false` si le
/// backend est headless : une boucle `GetMessage` vide doit alors se terminer.
/// Vrai avec un affichage réel (X11) ; faux en headless, où une boucle
/// `GetMessage` vide se termine au lieu d'attendre.
pub fn backend_is_interactive() -> bool {
    lock(backend()).event_fd().is_some()
}

pub fn wait_backend(timeout_ms: u32) -> bool {
    let fd = {
        let mut b = lock(backend());
        let Some(fd) = b.event_fd() else {
            return b.wait(timeout_ms);
        };
        if b.poll_pending() {
            return true;
        }
        fd
    };
    // Le verrou du backend est relâché : les autres threads peuvent peindre
    // pendant que celui-ci dort sur la connexion X11 et sur son éveil.
    let q = current_queue();
    poll_fds(&[fd, q.wake.0], timeout_ms);
    q.wake.drain();
    true
}

/// Attend que l'un des descripteurs soit lisible (au plus `timeout_ms`) ;
/// les descripteurs négatifs sont ignorés par `poll`.
fn poll_fds(fds: &[i32], timeout_ms: u32) {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let timeout = timeout_ms.min(i32::MAX as u32) as i32;
    // SAFETY : `pfds` est un tableau valide de `len` entrées pour la durée de l'appel.
    unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout) };
}

/// Transmet les commandes accumulées par les appels GDI invités au backend de
/// rendu, puis les oublie. Appelé à la fin d'un cycle de peinture.
pub fn flush_paint(hwnd: usize) {
    let cmds = take_pending(hwnd);
    if cmds.is_empty() {
        return;
    }
    with_backend(|b| b.draw(hwnd, &cmds));
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
                // La zone est invalidée même si un WM_PAINT attend déjà : une
                // ValidateRect entre les deux l'aurait sinon fait sauter, et
                // cette nouvelle invalidation était perdue.
                w.needs_paint = true;
                if w.paint_queued {
                    return;
                }
                w.paint_queued = true;
                Some(w.owner)
            }
            None => None,
        }
    };
    if let Some(tid) = owner {
        post_to_thread(
            tid,
            Msg {
                hwnd,
                message: WM_PAINT,
                wparam: 0,
                lparam: 0,
                time: 0,
                pt: Point::default(),
                l_private: SYSTEM_PAINT,
            },
        );
    }
}

/// Marque des WM_PAINT mis en file par Winlux lui-même (et non postés par
/// l'appli) : seuls ceux-là sont abandonnés une fois la fenêtre peinte.
const SYSTEM_PAINT: u32 = 0x5750;
/// `l_private` d'un WM_SIZE ou WM_MOVE venu du gestionnaire de fenêtres, et
/// non de SetWindowPos (qui envoie lui-même WM_WINDOWPOSCHANGED).
pub const BACKEND_GEOMETRY: u32 = 0x5751;

/// Fenêtres en cours de destruction : encore dans la table (IsWindow,
/// GetWindowThreadProcessId…) jusqu'après leur WM_NCDESTROY, comme sous
/// Windows, mais un second DestroyWindow ne les reprend pas (#1079).
fn destroying() -> &'static Mutex<HashSet<usize>> {
    static S: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn destroy_window(hwnd: usize) -> bool {
    // La fenêtre reste dans la table pendant WM_DESTROY et WM_NCDESTROY :
    // WinForms y compare le fil de la fenêtre au fil appelant (#1079).
    let (wnd_proc, owner) = {
        let map = lock(windows());
        match map.get(&hwnd) {
            Some(w) => (Some(w.wnd_proc), Some(w.owner)),
            // Inconnue de la table : on nettoie quand même ce que le
            // serveur X et les files en gardent, comme avant.
            None => (None, None),
        }
    };
    if !lock(destroying()).insert(hwnd) {
        // DestroyWindow rappelé pendant la destruction de la même fenêtre.
        return true;
    }
    // Les DC de la fenêtre ne servent plus (le code invité ne doit pas peindre).
    lock(dcs()).retain(|_, d| d.hwnd != hwnd);
    discard_key_chars(hwnd);
    let had_focus = clear_focus_for_destroy(hwnd);
    // WM_DESTROY synchrone auprès du wndproc de la fenêtre détruite.
    if let Some(wp) = wnd_proc {
        if had_focus {
            if let Some(owner) = owner.as_ref() {
                clear_thread_keyboard_state(owner);
            }
            clear_async_key_down_states();
            unsafe { call_wndproc(wp, hwnd, WM_KILLFOCUS, 0, 0) };
        }
        unsafe { call_wndproc(wp, hwnd, WM_DESTROY, 0, 0) };
    }
    // Les enfants sont détruits après le WM_DESTROY du parent (ordre Win32)
    // mais avant sa fenêtre X, qui emporterait leurs sous-fenêtres.
    let children: Vec<usize> = lock(windows())
        .iter()
        .filter(|(_, w)| w.parent == hwnd)
        .map(|(&h, _)| h)
        .collect();
    for c in children {
        destroy_window(c);
    }
    with_backend(|b| b.destroy_window(hwnd));
    super::gui_shared::withdraw(hwnd);
    // Plus aucun rappel sur une HWND morte : minuteurs, capture de la souris,
    // ordre Z et messages encore en file pour cette fenêtre.
    remove_window_timers(hwnd);
    let _ = CAPTURE_HWND.compare_exchange(
        hwnd,
        0,
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
    );
    lock(z_order()).retain(|&h| h != hwnd);
    if let Some(owner) = owner.as_ref() {
        let q = queue_for(*owner);
        lock(&q.items).retain(|m| m.hwnd != hwnd);
    }
    // Dernier message de la fenêtre : le programme y libère ses objets.
    if let Some(wp) = wnd_proc {
        unsafe { call_wndproc(wp, hwnd, 0x0082, 0, 0) }; // WM_NCDESTROY
    }
    // La HWND n'est plus valide qu'à partir d'ici.
    lock(windows()).remove(&hwnd);
    lock(destroying()).remove(&hwnd);
    lock(window_props()).retain(|(h, _), _| *h != hwnd);
    true
}

/// Propriétés de fenêtre (SetProp/GetProp) : clé = nom en minuscules, ou
/// `#n` pour un atome entier. La VCL y range l'objet Delphi de chaque
/// contrôle (ControlAtom) et le retrouve pour refléter WM_NOTIFY,
/// WM_DRAWITEM… (#1249, #1250).
fn window_props() -> &'static Mutex<HashMap<(usize, String), usize>> {
    static S: OnceLock<Mutex<HashMap<(usize, String), usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_window_prop(hwnd: usize, key: String, data: usize) {
    lock(window_props()).insert((hwnd, key), data);
}

pub fn window_prop(hwnd: usize, key: &str) -> Option<usize> {
    lock(window_props()).get(&(hwnd, key.to_string())).copied()
}

/// Propriétés d'une fenêtre, triées par nom (EnumPropsEx).
pub fn window_prop_list(hwnd: usize) -> Vec<(String, usize)> {
    let mut list: Vec<(String, usize)> = lock(window_props())
        .iter()
        .filter(|((h, _), _)| *h == hwnd)
        .map(|((_, k), v)| (k.clone(), *v))
        .collect();
    list.sort();
    list
}

pub fn remove_window_prop(hwnd: usize, key: &str) -> Option<usize> {
    lock(window_props()).remove(&(hwnd, key.to_string()))
}

/// Remplace la procédure d'une classe réenregistrée après UnregisterClass.
pub fn replace_class(name: &str, info: ClassInfo) {
    lock(classes()).insert(name.to_string(), info);
}

pub fn register_class(name: &str, info: ClassInfo) -> bool {
    let mut map = lock(classes());
    if map.contains_key(name) {
        return false;
    }
    map.insert(name.to_string(), info);
    true
}

fn class_styles() -> &'static Mutex<HashMap<String, u32>> {
    static S: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Mémorise le style de classe (CS_DBLCLKS…) d'une classe enregistrée.
pub fn set_class_style(name: &str, style: u32) {
    lock(class_styles())
        .entry(name.to_ascii_lowercase())
        .or_insert(style);
}

/// Style de classe mémorisé (0 si inconnu), pour GetClassInfoEx.
pub fn class_style(name: &str) -> u32 {
    lock(class_styles())
        .get(&name.to_ascii_lowercase())
        .copied()
        .unwrap_or(0)
}

fn class_cursors() -> &'static Mutex<HashMap<String, usize>> {
    static S: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Mémorise le curseur de classe (hCursor de WNDCLASS) ; le premier
/// enregistrement l'emporte, comme pour le style.
pub fn set_class_cursor(name: &str, cursor: usize) {
    lock(class_cursors())
        .entry(name.to_ascii_lowercase())
        .or_insert(cursor);
}

/// Curseur de classe d'une fenêtre : celui de RegisterClass, sinon celui du
/// contrôle prédéfini (barre en I pour une zone de texte, flèche ailleurs).
/// 0 quand la classe a été enregistrée sans curseur : l'application le
/// choisit elle-même par SetCursor.
pub fn class_cursor_of(hwnd: usize) -> usize {
    let Some(class) = window_class(hwnd).map(|c| c.to_ascii_lowercase()) else {
        return 0;
    };
    if let Some(&cursor) = lock(class_cursors()).get(&class) {
        return cursor;
    }
    if class == "edit" || class.starts_with("richedit") {
        load_cursor(0, 32513)
    } else {
        load_cursor(0, 32512)
    }
}

/// La fenêtre reçoit-elle des WM_xBUTTONDBLCLK ? Oui si sa classe porte
/// CS_DBLCLKS, comme les contrôles prédéfinis listes, boutons et zones de
/// texte sous Windows.
pub fn wants_double_clicks(hwnd: usize) -> bool {
    const CS_DBLCLKS: u32 = 0x0008;
    let Some(class) = lock(windows())
        .get(&hwnd)
        .map(|w| w.class.to_ascii_lowercase())
    else {
        return false;
    };
    match lock(class_styles()).get(&class) {
        Some(style) => style & CS_DBLCLKS != 0,
        // Contrôles prédéfinis qui traitent le double-clic ; les autres
        // (barres de défilement, onglets…) reçoivent deux appuis simples.
        None => matches!(
            class.as_str(),
            "button"
                | "static"
                | "edit"
                | "listbox"
                | "combobox"
                | "comboboxex32"
                | "syslistview32"
                | "systreeview32"
                | "toolbarwindow32"
        ),
    }
}

pub fn lookup_class(name: &str) -> Option<ClassInfo> {
    let registered = lock(classes()).get(name).map(|c| ClassInfo {
        wnd_proc: c.wnd_proc,
        background: c.background,
        instance: c.instance,
    });
    // Classes prédéfinies de Windows (BUTTON, EDIT…), sans tenir compte de
    // la casse ; une classe enregistrée du même nom par le programme gagne.
    registered.or_else(|| {
        crate::dlls::controls::builtin_class(name).map(|wnd_proc| ClassInfo {
            wnd_proc,
            background: 0,
            instance: 0,
        })
    })
}

// ---------- Accès utilisés par user32 ----------

/// Identifiant Win32 (GetCurrentThreadId) de chaque thread qui a créé une
/// fenêtre : GetWindowThreadProcessId le rend, et WPF le compare au sien.
fn thread_tids() -> &'static Mutex<HashMap<ThreadId, u32>> {
    static T: OnceLock<Mutex<HashMap<ThreadId, u32>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Thread Win32 propriétaire d'une fenêtre (celui qui l'a créée).
pub fn window_thread_id(hwnd: usize) -> Option<u32> {
    let owner = lock(windows()).get(&hwnd).map(|w| w.owner)?;
    lock(thread_tids()).get(&owner).copied()
}

/// Insère une fenêtre déjà construite dans la table ; une fenêtre de premier
/// niveau arrive au sommet de l'ordre Z.
pub fn insert_window(hwnd: usize, w: Window) {
    if w.owner == std::thread::current().id() {
        lock(thread_tids()).insert(w.owner, crate::dlls::kernel32::get_current_thread_id());
    }
    let top_level = w.parent == 0;
    lock(windows()).insert(hwnd, w);
    if top_level {
        bring_to_top(hwnd);
    }
}

/// Ordre Z des fenêtres de premier niveau, du sommet vers le fond. Une seule
/// clé : la dernière fenêtre créée ou activée est en tête, sans dépendre de
/// l'itération d'une table de hachage.
fn z_order() -> &'static Mutex<Vec<usize>> {
    static Z: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
    Z.get_or_init(|| Mutex::new(Vec::new()))
}

/// Place la fenêtre au sommet de l'ordre Z (création, activation).
pub fn bring_to_top(hwnd: usize) {
    let mut z = lock(z_order());
    z.retain(|&h| h != hwnd);
    z.insert(0, hwnd);
}

/// Fenêtre de premier niveau au sens de Windows : sans parent, ou sans
/// WS_CHILD. Une fenêtre possédée (boîte, fenêtre surgissante) garde sa
/// propriétaire dans `parent`, mais reste de premier niveau : EnumWindows,
/// WindowFromPoint et GetForegroundWindow la voient. La boîte « Erreur du
/// service Steam », possédée, n'était jamais trouvée sous le pointeur, et
/// vgui ignorait les clics sur ses boutons (#1804).
fn is_top_level(w: &Window) -> bool {
    w.parent == 0 || w.style & 0x4000_0000 == 0
}

/// Fenêtres de premier niveau du sommet vers le fond (ordre d'`EnumWindows`).
fn top_level_in_z_order(map: &HashMap<usize, Window>) -> Vec<usize> {
    let z = lock(z_order());
    let mut out: Vec<usize> = z
        .iter()
        .copied()
        .filter(|h| map.get(h).is_some_and(is_top_level))
        .collect();
    // Fenêtres insérées sans passer par `insert_window` : après, par handle.
    let mut rest: Vec<usize> = map
        .iter()
        .filter(|(h, w)| is_top_level(w) && !out.contains(h))
        .map(|(h, _)| *h)
        .collect();
    rest.sort_unstable();
    out.extend(rest);
    out
}

/// Expose le backend pour les wrappers (user32/gdi32).
pub fn with_backend_public<F: FnOnce(&mut dyn GuiBackend)>(f: F) {
    with_backend(f);
}

/// Capture d'une zone de l'écran (unités logiques), `None` sans affichage réel.
pub fn capture_screen(rect: Rect) -> Option<BitmapSnapshot> {
    // Lire l'écran montre aussi les fenêtres des autres programmes.
    if !crate::permissions::allowed(crate::permissions::Capability::Screen) {
        return None;
    }
    let mut captured = None;
    with_backend(|backend| captured = backend.capture_screen(rect));
    captured
}

pub fn capture_window(hwnd: usize, rect: Rect) -> Option<BitmapSnapshot> {
    let mut captured = None;
    with_backend(|backend| captured = backend.capture(hwnd, rect));
    captured
}

/// wndproc d'une fenêtre, si elle existe.
pub fn wnd_proc_of(hwnd: usize) -> Option<usize> {
    lock(windows()).get(&hwnd).map(|w| w.wnd_proc)
}

pub fn get_window_style(hwnd: usize) -> u32 {
    lock(windows()).get(&hwnd).map(|w| w.style).unwrap_or(0)
}

// set_window_style est défini plus bas avec return de l'ancien style

/// Renvoie l'origine (left, top) de la fenêtre en coordonnées écran.
/// Origine de la zone client en coordonnées écran.
pub fn window_origin(hwnd: usize) -> (i32, i32) {
    let bar = menu_bar_height(hwnd);
    screen_rect(hwnd)
        .map(|r| (r.left, r.top + bar))
        .unwrap_or((0, 0))
}

pub fn window_rect(hwnd: usize) -> Option<Rect> {
    lock(windows()).get(&hwnd).map(|w| w.rect)
}

/// Vrai pour une fenêtre `WS_CHILD` rattachée à un parent : son rectangle
/// est relatif à la zone client de ce parent.
fn is_child(w: &Window) -> bool {
    w.parent != 0 && w.style & 0x4000_0000 != 0
}

/// Rectangle en coordonnées écran (GetWindowRect) : les enfants ajoutent
/// l'origine de chacun de leurs parents.
pub fn screen_rect(hwnd: usize) -> Option<Rect> {
    let map = lock(windows());
    let w = map.get(&hwnd)?;
    let mut rect = w.rect;
    let mut cur = w;
    let mut depth = 0;
    while is_child(cur) && depth < 64 {
        let Some(parent) = map.get(&cur.parent) else {
            break;
        };
        rect.left += parent.rect.left;
        rect.right += parent.rect.left;
        rect.top += parent.rect.top + parent.menu_bar;
        rect.bottom += parent.rect.top + parent.menu_bar;
        cur = parent;
        depth += 1;
    }
    Some(rect)
}

pub fn window_owner(hwnd: usize) -> Option<ThreadId> {
    lock(windows()).get(&hwnd).map(|w| w.owner)
}

/// Remplace le rectangle logique et renvoie son ancienne valeur.
pub fn set_window_rect(hwnd: usize, rect: Rect) -> Option<Rect> {
    let mut map = lock(windows());
    let window = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut window.rect, rect))
}

/// Met à jour la taille client après un `ConfigureNotify` X11 externe.
/// WM_SIZE du backend : taille de la zone client ; la fenêtre ajoute sa
/// barre de menus.
fn update_window_size(hwnd: usize, width: i32, height: i32) {
    if let Some(window) = lock(windows()).get_mut(&hwnd) {
        window.rect.right = window.rect.left.saturating_add(width);
        window.rect.bottom = window
            .rect
            .top
            .saturating_add(height)
            .saturating_add(window.menu_bar);
    }
}

/// Hauteur de la barre de menus d'une fenêtre (0 sans menu).
pub fn menu_bar_height(hwnd: usize) -> i32 {
    lock(windows()).get(&hwnd).map_or(0, |w| w.menu_bar)
}

/// Fixe la hauteur de la barre de menus ; renvoie l'ancienne.
pub fn set_menu_bar_height(hwnd: usize, height: i32) -> Option<i32> {
    let mut map = lock(windows());
    let w = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut w.menu_bar, height.max(0)))
}

fn update_window_position(hwnd: usize, x: i32, y: i32) {
    if let Some(window) = lock(windows()).get_mut(&hwnd) {
        let width = window.rect.width();
        let height = window.rect.height();
        window.rect.left = x;
        window.rect.top = y;
        window.rect.right = x.saturating_add(width);
        window.rect.bottom = y.saturating_add(height);
    }
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
/// Drapeau « visible » propre à la fenêtre (sans regarder ses parents).
pub fn visible_flag(hwnd: usize) -> Option<bool> {
    lock(windows()).get(&hwnd).map(|w| w.visible)
}

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

/// Fenêtres de premier niveau visibles du fil appelant, du sommet vers le
/// fond, avec leur style : candidates à l'activation quand la fenêtre qui
/// avait le focus disparaît (#1311).
pub fn thread_visible_top_levels() -> Vec<(usize, u32)> {
    let tid = std::thread::current().id();
    let map = lock(windows());
    top_level_in_z_order(&map)
        .into_iter()
        .filter_map(|h| {
            map.get(&h)
                .filter(|w| w.owner == tid && w.visible)
                .map(|w| (h, w.style))
        })
        .collect()
}

/// Fenêtre de premier niveau au sommet de l'ordre Z (visible de préférence).
pub fn first_window() -> Option<usize> {
    let map = lock(windows());
    let order = top_level_in_z_order(&map);
    order
        .iter()
        .copied()
        .find(|h| map.get(h).is_some_and(|w| w.visible))
        .or_else(|| order.first().copied())
}

/// Ouvre un cycle de peinture : marque la zone comme validée et alloue un vrai
/// `HDC` invité ciblant la fenêtre.
pub fn begin_paint(hwnd: usize) -> Option<(usize, Rect)> {
    let (rect, background) = {
        let mut map = lock(windows());
        let w = map.get_mut(&hwnd)?;
        w.needs_paint = false;
        w.paint_queued = false;
        w.painting = true;
        (w.client_rect(), w.background)
    };
    let hdc = create_dc(hwnd);
    // Une fenêtre qui contient des contrôles standard (boîte de réglages
    // classique) est effacée avec le pinceau de sa classe, comme sous
    // Windows : sans cela le dégradé du thème apparaîtrait entre les
    // contrôles, peints, eux, sur la couleur des boîtes de dialogue.
    if background != 0 && has_control_children(hwnd) {
        if let Some(color) = brush_color(background) {
            record(hdc, DrawCmd::FillRect { rect, color });
        }
    }
    Some((hdc, rect))
}

/// Couleur d'un pinceau de classe : un vrai pinceau, ou `COLOR_* + 1`.
fn brush_color(brush: usize) -> Option<u32> {
    if (1..=31).contains(&brush) {
        Some(crate::dlls::user32::get_sys_color(brush as i32 - 1))
    } else {
        object_color(brush)
    }
}

/// Couleur unie du fond du parent d'un contrôle, quand ce parent est effacé
/// avec le pinceau de sa classe (voir `begin_paint`) ; None s'il garde le
/// dégradé du thème.
pub fn parent_backdrop_color(hwnd: usize) -> Option<u32> {
    let parent = window_parent(hwnd).filter(|&p| p != 0)?;
    let background = lock(windows()).get(&parent)?.background;
    if background == 0 || !has_control_children(parent) {
        return None;
    }
    brush_color(background)
}

/// WM_ERASEBKGND envoyé par le programme lui-même avec son propre DC :
/// peindre le fond de la classe dedans, comme DefWindowProc. Notepad++
/// lit ainsi la couleur de fond du dialogue « À propos » dans une image
/// hors écran pour le fond de son lien (#768). Winlux peint le fond des
/// fenêtres lui-même, il n'envoie jamais ce message.
pub fn erase_background(hwnd: usize, hdc: usize) -> bool {
    if hdc == 0 {
        return false;
    }
    let Some((background, rect)) = lock(windows())
        .get(&hwnd)
        .map(|w| (w.background, w.client_rect()))
    else {
        return false;
    };
    let Some(color) = (background != 0).then(|| brush_color(background)).flatten() else {
        return false;
    };
    record(hdc, DrawCmd::FillRect { rect, color });
    true
}

fn has_control_children(hwnd: usize) -> bool {
    child_windows(hwnd)
        .into_iter()
        .any(|c| crate::dlls::controls::kind_of(c).is_some())
}

/// Clôt le cycle : libère le DC et rejoue les commandes GDI chez le backend.
pub fn end_paint(hwnd: usize, hdc: usize) -> bool {
    if hdc != 0 {
        free_dc(hdc);
    }
    if let Some(w) = lock(windows()).get_mut(&hwnd) {
        w.painting = false;
    }
    flush_paint(hwnd);
    true
}

/// Fin d'un dessin fait hors `WM_PAINT` (`GetDC` … `ReleaseDC`) : sous
/// Windows, ces pixels s'ajoutent à ce que la fenêtre montre déjà. Les
/// commandes sont donc posées par-dessus le contenu actuel, sans repeindre
/// le fond ni effacer le reste (le terminal de PuTTY ne redessine que les
/// lignes qui changent). Pendant un `BeginPaint`, elles attendent `EndPaint`.
pub fn flush_paint_over(hwnd: usize) {
    let cmds = {
        let mut map = lock(windows());
        let Some(w) = map.get_mut(&hwnd) else {
            return;
        };
        if w.painting {
            return;
        }
        std::mem::take(&mut w.pending)
    };
    if cmds.is_empty() {
        return;
    }
    with_backend(|b| b.draw_over(hwnd, &cmds));
}

// ---------- File : prise / dépôt ----------

/// Retire et renvoie le premier message correspondant au filtre.
pub fn take_message(q: &Queue, hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    let taken = loop {
        let m = {
            let mut items = lock(&q.items);
            let idx = items.iter().position(|m| match_filter(m, hwnd, min, max))?;
            items.remove(idx)?
        };
        // Un WM_PAINT dont la fenêtre a déjà été peinte entre-temps (BeginPaint
        // via UpdateWindow, ValidateRect) n'existe plus sous Windows : le
        // délivrer relançait une peinture de trop (#796).
        if m.message == WM_PAINT && m.l_private == SYSTEM_PAINT {
            let mut map = lock(windows());
            if let Some(w) = map.get_mut(&m.hwnd).filter(|w| !w.needs_paint) {
                // Le WM_PAINT quitte la file : sans cela, `request_paint`
                // le croyait toujours en attente et n'en postait plus aucun.
                w.paint_queued = false;
                continue;
            }
        }
        // Message souris promu d'un WM_POINTER* que la fenêtre a traité
        // elle-même : Windows ne le produit pas (#1199).
        if !crate::dlls::pointer::on_take(&m) {
            continue;
        }
        break m;
    };
    if taken.message == WM_PAINT {
        // Le WM_PAINT n'est plus en file ; la zone reste invalidée tant que
        // BeginPaint/ValidateRect n'a pas été appelé (voir `pending_paint`).
        if let Some(w) = lock(windows()).get_mut(&taken.hwnd) {
            w.paint_queued = false;
        }
    }
    Some(taken)
}

/// `WM_PAINT` synthétisé quand la file est vide, comme sous Windows : une
/// fenêtre visible du thread appelant dont la zone est toujours invalidée (pas
/// de `BeginPaint`/`ValidateRect` depuis `InvalidateRect`) le reçoit à nouveau.
pub fn pending_paint(hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    if !(min == 0 && max == 0) && !(min..=max).contains(&WM_PAINT) {
        return None;
    }
    let tid = std::thread::current().id();
    let map = lock(windows());
    let order = lock(z_order());
    let candidates = order.iter().copied().chain(map.keys().copied());
    let target = candidates.filter(|h| hwnd == 0 || *h == hwnd).find(|h| {
        map.get(h)
            .is_some_and(|w| w.owner == tid && w.visible && w.needs_paint && !w.paint_queued)
    })?;
    Some(Msg::new(target, WM_PAINT, 0, 0))
}

/// Zone invalidée d'une fenêtre visible du thread appelant (UpdateWindow).
pub fn paint_pending_here(hwnd: usize) -> bool {
    let tid = std::thread::current().id();
    lock(windows())
        .get(&hwnd)
        .is_some_and(|w| w.owner == tid && w.visible && w.needs_paint)
}

/// `ValidateRect` : la fenêtre n'a plus besoin d'être repeinte.
pub fn validate_window(hwnd: usize) -> bool {
    match lock(windows()).get_mut(&hwnd) {
        Some(w) => {
            w.needs_paint = false;
            true
        }
        None => false,
    }
}

/// Remet un message en tête de file (pour WM_QUIT non retiré).
pub fn prepend_message(q: &Queue, m: Msg) {
    lock(&q.items).push_front(m);
}

/// Regarde sans retirer.
pub fn peek_message(q: &Queue, hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    let items = lock(&q.items);
    items
        .iter()
        .find(|m| match_filter(m, hwnd, min, max))
        .copied()
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
    q.wake.signal();
}

// ---------- Défaut ----------

std::thread_local! {
    /// WM_NCCALCSIZE est passé à DefWindowProc (cadre standard demandé).
    static DEFAULT_NCCALCSIZE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Indicateur « WM_NCCALCSIZE laissé à DefWindowProc » du thread : remis à
/// zéro avant l'envoi du message, lu après (#1131).
pub fn reset_default_nccalcsize() {
    DEFAULT_NCCALCSIZE.with(|c| c.set(false));
}
pub fn default_nccalcsize_seen() -> bool {
    DEFAULT_NCCALCSIZE.with(|c| c.get())
}

/// `DefWindowProc` minimal : gère WM_CLOSE et laisse le reste au programme.
pub fn def_window_proc(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> isize {
    crate::dlls::pointer::on_default(message);
    match message {
        // WM_WINDOWPOSCHANGED : WM_MOVE et WM_SIZE du SetWindowPos en cours
        // (#2044).
        0x0047 => {
            crate::dlls::user32::window_pos_changed_default(hwnd);
            0
        }
        // WM_SETCURSOR : le parent d'une fenêtre enfant décide d'abord ; puis
        // le curseur de classe dans la zone cliente, la flèche ailleurs.
        0x0020 => {
            const WS_CHILD: u32 = 0x4000_0000;
            const HTCLIENT: isize = 1;
            let child = window_style(hwnd).is_some_and(|s| s & WS_CHILD != 0);
            if let Some(parent) = window_parent(hwnd).filter(|&p| child && p != 0) {
                let handled =
                    unsafe { crate::dlls::user32::send_message_w(parent, 0x0020, wparam, lparam) };
                if handled != 0 {
                    return handled;
                }
            }
            // Message remonté d'un enfant (wParam = la fenêtre sous le
            // pointeur) : le parent ne choisit rien, l'enfant applique son
            // propre curseur de classe (la barre en I d'une zone de texte).
            if wparam != hwnd {
                return 0;
            }
            if lparam & 0xFFFF == HTCLIENT {
                let cursor = class_cursor_of(hwnd);
                if cursor == 0 {
                    return 0;
                }
                set_cursor(cursor);
                return 1;
            }
            set_cursor(load_cursor(0, 32512));
            0
        }
        // WM_MOUSEACTIVATE : le parent d'une fenêtre enfant décide d'abord,
        // sinon MA_ACTIVATE, comme Windows (#1804).
        0x0021 => {
            const WS_CHILD: u32 = 0x4000_0000;
            const MA_ACTIVATE: isize = 1;
            let child = window_style(hwnd).is_some_and(|s| s & WS_CHILD != 0);
            if let Some(parent) = window_parent(hwnd).filter(|&p| child && p != 0) {
                let answer =
                    unsafe { crate::dlls::user32::send_message_w(parent, 0x0021, wparam, lparam) };
                if answer != 0 {
                    return answer;
                }
            }
            MA_ACTIVATE
        }
        // WM_NCCALCSIZE : une fenêtre de premier niveau n'a pas de zone non
        // cliente à elle (le cadre est celui du gestionnaire de fenêtres), le
        // rectangle proposé reste donc la zone cliente.
        0x0083 => {
            DEFAULT_NCCALCSIZE.with(|c| c.set(true));
            0
        }
        // WM_SETTEXT / WM_GETTEXT / WM_GETTEXTLENGTH : le titre de la fenêtre,
        // en A ou en W selon la fonction qui a envoyé le message.
        0x000C => {
            let text = if lparam == 0 {
                String::new()
            } else if crate::dlls::controls::message_is_wide(hwnd) {
                unsafe { crate::dlls::wstr(lparam as *const u16) }
            } else {
                unsafe { crate::dlls::cstr(lparam as *const u8) }
            };
            if set_title(hwnd, &text) {
                with_backend(|b| b.set_title(hwnd, &text));
            }
            1
        }
        0x000D => {
            let text = title_of(hwnd).unwrap_or_default();
            if lparam == 0 || wparam == 0 {
                0
            } else if crate::dlls::controls::message_is_wide(hwnd) {
                unsafe {
                    crate::dlls::put_text_w(&text, lparam as *mut u16, wparam as u32) as isize
                }
            } else {
                unsafe { crate::dlls::put_text_a(&text, lparam as *mut u8, wparam as u32) as isize }
            }
        }
        0x000E => {
            let text = title_of(hwnd).unwrap_or_default();
            if crate::dlls::controls::message_is_wide(hwnd) {
                text.encode_utf16().count() as isize
            } else {
                crate::dlls::encode_ansi(&text).len() as isize
            }
        }
        WM_CLOSE => {
            destroy_window(hwnd);
            0
        }
        WM_ERASEBKGND => {
            erase_background(hwnd, wparam);
            1
        }
        // WM_PRINT : la fenêtre se peint dans le DC fourni (fond puis zone
        // client), comme le fait DefWindowProc ; Notepad++ peint ainsi ses
        // onglets et sa barre d'état dans une image hors écran.
        0x0317 => {
            if wparam != 0 {
                if lparam & 0x8 != 0 {
                    unsafe { crate::dlls::user32::send_message_w(hwnd, WM_ERASEBKGND, wparam, 0) };
                }
                if lparam & 0x4 != 0 {
                    unsafe {
                        crate::dlls::user32::send_message_w(hwnd, 0x0318, wparam, lparam & 0x4)
                    };
                }
            }
            0
        }
        // WM_NCLBUTTONDOWN sur la légende ou un bord (fenêtre sans cadre qui
        // dessine sa propre barre de titre) : le gestionnaire de fenêtres
        // déplace ou redimensionne, comme la boucle modale de Windows.
        0x00A1 if wparam == 2 || (10..=17).contains(&wparam) => {
            with_backend(|b| b.begin_move_resize(hwnd, wparam as u32));
            0
        }
        // WM_NCLBUTTONDBLCLK sur la légende : WM_SYSCOMMAND SC_MAXIMIZE ou
        // SC_RESTORE, comme Windows (#1477).
        0x00A3 if wparam == 2 => {
            let mut state = WINDOW_NORMAL;
            with_backend(|b| state = b.window_state(hwnd));
            let command = if state == WINDOW_MAXIMIZED {
                0xF120
            } else {
                0xF030
            };
            unsafe { crate::dlls::user32::send_message_w(hwnd, 0x0112, command, lparam) };
            0
        }
        // WM_SYSCOMMAND : réduire, agrandir, restaurer, déplacer, fermer.
        0x0112 => {
            match wparam & 0xFFF0 {
                0xF020 => with_backend(|b| b.set_window_state(hwnd, WINDOW_MINIMIZED)),
                0xF030 => with_backend(|b| b.set_window_state(hwnd, WINDOW_MAXIMIZED)),
                0xF120 => with_backend(|b| b.set_window_state(hwnd, WINDOW_NORMAL)),
                0xF010 => with_backend(|b| b.begin_move_resize(hwnd, 2)),
                0xF000 => {
                    let edge = (wparam & 0xF) as u32;
                    // SC_SIZE + WMSZ_* (1 gauche … 8 bas-droite) → HT*.
                    let ht = [0, 10, 11, 12, 13, 14, 15, 16, 17];
                    if let Some(&h) = ht.get(edge as usize).filter(|&&h| h != 0) {
                        with_backend(|b| b.begin_move_resize(hwnd, h));
                    }
                }
                0xF060 => unsafe {
                    crate::dlls::user32::post_message_w(hwnd, 0x0010, 0, 0);
                },
                _ => {}
            }
            0
        }
        // WM_NCLBUTTONDOWN sur HTMENU : clic dans la barre de menus.
        0x00A1 if wparam == 5 => {
            let x = lparam as u32 as u16 as i16 as i32;
            let y = (lparam as u32 >> 16) as u16 as i16 as i32;
            if let Some(index) = crate::dlls::menus::bar_hit(hwnd, x, y) {
                crate::dlls::menus::open_bar_menu(hwnd, index);
            }
            0
        }
        // Menu contextuel (#1139) : DefWindowProc le demande au relâchement
        // du bouton droit (coordonnées écran), à Maj+F10 et à la touche
        // Applications (-1 : au clavier). Une fenêtre enfant qui ne le traite
        // pas le passe à son parent.
        WM_RBUTTONUP => {
            let x = lparam as u32 as u16 as i16 as i32;
            let y = (lparam as u32 >> 16) as u16 as i16 as i32;
            let (ox, oy) = screen_rect(hwnd).map_or((0, 0), |r| (r.left, r.top));
            send_context_menu(hwnd, hwnd, pack_point(x + ox, y + oy));
            0
        }
        0x00A5 => {
            // WM_NCRBUTTONUP : déjà en coordonnées écran.
            send_context_menu(hwnd, hwnd, lparam);
            0
        }
        WM_KEYDOWN | 0x0104 if wparam == 0x79 && get_key_state(0x10) < 0 => {
            send_context_menu(hwnd, hwnd, -1);
            0
        }
        0x0101 if wparam == 0x5D => {
            send_context_menu(hwnd, hwnd, -1);
            0
        }
        WM_CONTEXTMENU => {
            let child = window_style(hwnd).is_some_and(|s| s & 0x4000_0000 != 0);
            let parent = window_parent(hwnd).unwrap_or(0);
            if child && parent != 0 {
                send_context_menu(parent, wparam, lparam);
            }
            0
        }
        // WM_NCCREATE : accepter la création (FALSE l'annulerait).
        0x0081 => 1,
        // Comme BeginPaint/EndPaint de DefWindowProc : la zone est validée,
        // sinon la fenêtre recevrait WM_PAINT sans fin.
        // Une fenêtre à contrôles standard sans WM_PAINT à elle (boîte
        // « À propos » de Notepad++) est effacée avec le pinceau de sa
        // classe, comme le BeginPaint de Windows : sinon le dégradé du thème
        // restait visible autour de contrôles peints, eux, sur un fond uni
        // (#745).
        WM_PAINT => {
            if let Some((hdc, _)) = begin_paint(hwnd) {
                end_paint(hwnd, hdc);
            }
            validate_window(hwnd);
            0
        }
        _ => 0,
    }
}

pub const WM_CONTEXTMENU: u32 = 0x007B;

fn pack_point(x: i32, y: i32) -> isize {
    (((y as u16 as u32) << 16) | x as u16 as u32) as i32 as isize
}

fn send_context_menu(target: usize, hwnd: usize, lparam: isize) {
    unsafe { crate::dlls::user32::send_message_w(target, WM_CONTEXTMENU, hwnd, lparam) };
}

// ---------- Accesseurs étendus pour les couches user32/gdi32 ----------

/// Renvoie le style WS_* de la fenêtre.
pub fn window_style(hwnd: usize) -> Option<u32> {
    lock(windows()).get(&hwnd).map(|w| w.style)
}

/// Enfant transparent qui laisse voir son parent : WS_CHILD et
/// WS_EX_TRANSPARENT, hors contrôles intégrés (qui peignent eux-mêmes).
/// Sous Windows, la chaîne d'échange présentée dans le parent passe devant
/// le contenu GDI d'un tel enfant ; en X11 sa fenêtre la masquerait. La
/// « Chrome Legacy Window » de Chromium couvre ainsi toute la zone client
/// où le processus GPU présente la page (#1337).
pub fn is_see_through_child(hwnd: usize) -> bool {
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_EX_TRANSPARENT: u32 = 0x20;
    let see_through = lock(windows())
        .get(&hwnd)
        .is_some_and(|w| w.style & WS_CHILD != 0 && w.ex_style & WS_EX_TRANSPARENT != 0);
    see_through && super::controls::kind_of(hwnd).is_none()
}

/// Renvoie le style étendu WS_EX_* de la fenêtre.
pub fn window_ex_style(hwnd: usize) -> Option<u32> {
    lock(windows()).get(&hwnd).map(|w| w.ex_style)
}

/// Met à jour le style WS_* et renvoie l'ancien.
pub fn set_window_style(hwnd: usize, style: u32) -> Option<u32> {
    let mut map = lock(windows());
    let w = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut w.style, style))
}

/// Met à jour le style étendu et renvoie l'ancien.
pub fn set_window_ex_style(hwnd: usize, ex_style: u32) -> Option<u32> {
    let mut map = lock(windows());
    let w = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut w.ex_style, ex_style))
}

/// Nom de classe d'une fenêtre.
pub fn window_class(hwnd: usize) -> Option<String> {
    lock(windows()).get(&hwnd).map(|w| w.class.clone())
}

/// Le hwnd parent d'une fenêtre.
pub fn window_parent(hwnd: usize) -> Option<usize> {
    lock(windows()).get(&hwnd).map(|w| w.parent)
}

/// Modifie le parent d'une fenêtre ; renvoie l'ancien.
pub fn set_window_parent(hwnd: usize, new_parent: usize) -> Option<usize> {
    let mut map = lock(windows());
    let w = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut w.parent, new_parent))
}

/// La fenêtre est-elle visible ?
pub fn is_window_visible(hwnd: usize) -> bool {
    lock(windows()).get(&hwnd).is_some_and(|w| w.visible)
}

/// La fenêtre existe-t-elle ?
#[allow(dead_code)]
pub fn is_window(hwnd: usize) -> bool {
    lock(windows()).contains_key(&hwnd)
}

/// Modifie le wndproc ; renvoie l'ancien.
pub fn set_wnd_proc(hwnd: usize, new_proc: usize) -> Option<usize> {
    let mut map = lock(windows());
    let w = map.get_mut(&hwnd)?;
    Some(std::mem::replace(&mut w.wnd_proc, new_proc))
}

/// Enfants directs d'une fenêtre, dans l'ordre de création (ordre Z des
/// contrôles d'un dialogue, ordre de la touche Tab).
pub fn child_windows(parent: usize) -> Vec<usize> {
    let mut v: Vec<usize> = lock(windows())
        .iter()
        .filter(|(_, w)| w.parent == parent)
        .map(|(h, _)| *h)
        .collect();
    v.sort_unstable();
    v
}

/// Énumère les HWNDs de toutes les fenêtres de premier niveau.
pub fn enum_top_level_windows() -> Vec<usize> {
    top_level_in_z_order(&lock(windows()))
}

/// FindWindowEx (#2106) : parmi les enfants directs de `parent` (0 : les
/// fenêtres de premier niveau, dans l'ordre Z, que `keep` filtre), la
/// première qui suit `after` (0 : depuis le début) et dont la classe
/// (sans casse) et le titre correspondent. `after` absent de la liste :
/// aucune, une boucle d'énumération s'arrête toujours.
pub fn find_window_ex(
    parent: usize,
    after: usize,
    class: Option<&str>,
    title: Option<&str>,
    keep: impl Fn(usize) -> bool,
) -> Option<usize> {
    let map = lock(windows());
    let list: Vec<usize> = if parent == 0 {
        top_level_in_z_order(&map)
            .into_iter()
            .filter(|&h| keep(h))
            .collect()
    } else {
        let mut v: Vec<usize> = map
            .iter()
            .filter(|(_, w)| w.parent == parent && !is_top_level(w))
            .map(|(h, _)| *h)
            .collect();
        v.sort_unstable();
        v
    };
    let start = if after == 0 {
        0
    } else {
        list.iter().position(|&h| h == after)? + 1
    };
    list[start..].iter().copied().find(|h| {
        map.get(h).is_some_and(|w| {
            class.is_none_or(|c| c.eq_ignore_ascii_case(&w.class))
                && title.is_none_or(|t| w.title == t)
        })
    })
}

/// Renvoie un « voisin » de la fenêtre (GetWindow / GW_*).
pub fn get_window_relative(hwnd: usize, cmd: u32) -> Option<usize> {
    let map = lock(windows());
    if !map.contains_key(&hwnd) {
        return None;
    }
    // GW_HWNDFIRST=0 GW_HWNDLAST=1 GW_HWNDNEXT=2 GW_HWNDPREV=3
    // GW_OWNER=4 GW_CHILD=5
    // Fratrie dans un ordre stable : ordre Z pour le premier niveau, handle
    // croissant pour les enfants d'un même parent.
    let siblings = |parent: usize| -> Vec<usize> {
        if parent == 0 {
            top_level_in_z_order(&map)
        } else {
            let mut v: Vec<usize> = map
                .iter()
                .filter(|(_, w)| w.parent == parent)
                .map(|(h, _)| *h)
                .collect();
            v.sort_unstable();
            v
        }
    };
    match cmd {
        0..=3 => {
            let parent = map
                .get(&hwnd)
                .filter(|w| !is_top_level(w))
                .map_or(0, |w| w.parent);
            let list = siblings(parent);
            let pos = list.iter().position(|&h| h == hwnd)?;
            match cmd {
                0 => list.first().copied(),
                1 => list.last().copied(),
                2 => list.get(pos + 1).copied(),
                _ => pos.checked_sub(1).and_then(|p| list.get(p).copied()),
            }
        }
        4 => {
            // GW_OWNER : même chose que parent ici
            map.get(&hwnd)
                .and_then(|w| if w.parent != 0 { Some(w.parent) } else { None })
        }
        5 => siblings(hwnd).first().copied(), // GW_CHILD : premier enfant
        _ => None,
    }
}

/// Sauvegarde DC : empile l'état courant, renvoie l'identifiant de pile.
pub fn save_dc(hdc: usize) -> i32 {
    let dcs_lock = lock(dcs());
    if let Some(dc) = dcs_lock.get(&hdc) {
        let state = *dc;
        drop(dcs_lock);
        let stack = dc_save_stack();
        let mut stk = lock(stack);
        let entry = stk.entry(hdc).or_default();
        entry.push(state);
        entry.len() as i32
    } else {
        0
    }
}

/// Restaure DC depuis la pile.
pub fn restore_dc(hdc: usize, saved_dc: i32) -> bool {
    let stack = dc_save_stack();
    let mut stk = lock(stack);
    let entry = match stk.get_mut(&hdc) {
        Some(e) if !e.is_empty() => e,
        _ => return false,
    };
    if saved_dc == -1 {
        if let Some(state) = entry.pop() {
            let mut dcs_lock = lock(dcs());
            if let Some(dc) = dcs_lock.get_mut(&hdc) {
                *dc = state;
            }
            return true;
        }
        false
    } else if saved_dc > 0 && (saved_dc as usize) <= entry.len() {
        let idx = (saved_dc as usize) - 1;
        let state = entry[idx];
        entry.truncate(idx);
        let mut dcs_lock = lock(dcs());
        if let Some(dc) = dcs_lock.get_mut(&hdc) {
            *dc = state;
        }
        true
    } else {
        false
    }
}

fn dc_save_stack() -> &'static Mutex<HashMap<usize, Vec<DcState>>> {
    static S: OnceLock<Mutex<HashMap<usize, Vec<DcState>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------- Capture souris ----------

static CAPTURE_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_capture(hwnd: usize) -> usize {
    CAPTURE_HWND.swap(hwnd, std::sync::atomic::Ordering::Relaxed)
}

pub fn release_capture() -> bool {
    CAPTURE_HWND.swap(0, std::sync::atomic::Ordering::Relaxed) != 0
}

pub fn get_capture() -> usize {
    CAPTURE_HWND.load(std::sync::atomic::Ordering::Relaxed)
}

// ---------- Timers ----------

use std::sync::atomic::AtomicUsize;

static NEXT_TIMER_ID: AtomicUsize = AtomicUsize::new(1);

#[allow(dead_code)]
struct TimerEntry {
    /// Thread qui reçoit les WM_TIMER : celui de la fenêtre, sinon le créateur.
    owner: ThreadId,
    hwnd: usize,
    id: usize,
    interval_ms: u32,
    callback: usize,
    next_fire: u64,
}

fn timers() -> &'static Mutex<Vec<TimerEntry>> {
    static T: OnceLock<Mutex<Vec<TimerEntry>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn set_timer(hwnd: usize, id: usize, interval: u32, callback: usize) -> usize {
    let current = std::thread::current().id();
    let owner = if hwnd == 0 {
        current
    } else {
        match window_owner(hwnd) {
            Some(owner) => owner,
            // Win32 : SetTimer échoue sur une HWND inconnue.
            None => return 0,
        }
    };
    let mut t = lock(timers());
    // Remplacer un timer existant avec le même (thread, hwnd, id). Sans
    // fenêtre, l'identifiant est attribué par le système.
    let real_id = if hwnd == 0 {
        NEXT_TIMER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    } else {
        t.retain(|e| !(e.owner == owner && e.hwnd == hwnd && e.id == id));
        id
    };
    t.push(TimerEntry {
        owner,
        hwnd,
        id: real_id,
        interval_ms: interval,
        callback,
        next_fire: crate::dlls::kernel32::get_tick_count64() + interval.max(10) as u64,
    });
    drop(t);
    if owner != current {
        // Le thread propriétaire dort peut-être sans échéance : le réveiller.
        wake_thread(owner);
    }
    real_id
}

/// Retire tous les minuteurs d'une fenêtre détruite.
fn remove_window_timers(hwnd: usize) {
    lock(timers()).retain(|e| e.hwnd != hwnd);
}

/// Prochain WM_TIMER échu correspondant au filtre (`overdue_only` : seulement
/// un minuteur en retard d'au moins une période, pour qu'un flot continu
/// d'entrées ne l'affame pas). Le minuteur est réarmé à la période suivante.
pub fn due_timer(hwnd: usize, min: u32, max: u32, overdue_only: bool) -> Option<Msg> {
    const WM_TIMER: u32 = 0x0113;
    if !(min == 0 && max == 0) && !(min..=max).contains(&WM_TIMER) {
        return None;
    }
    let now = crate::dlls::kernel32::get_tick_count64();
    let tid = std::thread::current().id();
    let mut t = lock(timers());
    let e = t.iter_mut().find(|e| {
        let interval = e.interval_ms.max(10) as u64;
        let limit = if overdue_only {
            e.next_fire.saturating_add(interval)
        } else {
            e.next_fire
        };
        e.owner == tid && (hwnd == 0 || e.hwnd == hwnd) && now >= limit
    })?;
    let interval = e.interval_ms.max(10) as u64;
    e.next_fire = if e.next_fire == 0 || now >= e.next_fire + 4 * interval {
        now + interval
    } else {
        e.next_fire + interval
    };
    Some(Msg {
        hwnd: e.hwnd,
        message: WM_TIMER,
        wparam: e.id,
        lparam: e.callback as isize,
        time: now as u32,
        pt: Point::default(),
        l_private: 0,
    })
}

/// WM_TIMER échu sans réarmer le minuteur (`PeekMessage(PM_NOREMOVE)`) :
/// Windows le montre aussi, et WinForms n'appelle GetMessage qu'après l'avoir
/// vu par ce moyen.
pub fn peek_due_timer(hwnd: usize, min: u32, max: u32) -> Option<Msg> {
    const WM_TIMER: u32 = 0x0113;
    if !(min == 0 && max == 0) && !(min..=max).contains(&WM_TIMER) {
        return None;
    }
    let now = crate::dlls::kernel32::get_tick_count64();
    let tid = std::thread::current().id();
    let t = lock(timers());
    let e = t
        .iter()
        .find(|e| e.owner == tid && (hwnd == 0 || e.hwnd == hwnd) && now >= e.next_fire)?;
    Some(Msg {
        hwnd: e.hwnd,
        message: WM_TIMER,
        wparam: e.id,
        lparam: e.callback as isize,
        time: now as u32,
        pt: Point::default(),
        l_private: 0,
    })
}

/// Millisecondes avant le prochain minuteur du thread appelant (pour borner
/// l'attente).
pub fn next_timer_ms(hwnd: usize) -> Option<u32> {
    let now = crate::dlls::kernel32::get_tick_count64();
    let tid = std::thread::current().id();
    lock(timers())
        .iter()
        .filter(|e| e.owner == tid && (hwnd == 0 || e.hwnd == hwnd))
        .map(|e| e.next_fire.saturating_sub(now).min(u32::MAX as u64) as u32)
        .min()
}

/// `KillTimer` : seuls les minuteurs du thread appelant sont concernés.
pub fn kill_timer(hwnd: usize, id: usize) -> bool {
    let tid = std::thread::current().id();
    let mut t = lock(timers());
    let before = t.len();
    t.retain(|e| !(e.owner == tid && e.hwnd == hwnd && e.id == id));
    t.len() < before
}

// ---------- Curseur ----------

pub static CURSOR_HANDLE: AtomicUsize = AtomicUsize::new(0x0001_0001);
static SHOW_CURSOR_COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn load_cursor(_instance: usize, name: usize) -> usize {
    match name {
        32512 => 0x0001_0001, // IDC_ARROW
        32513 => 0x0001_0002, // IDC_IBEAM
        32514 => 0x0001_0003, // IDC_WAIT
        32515 => 0x0001_0004, // IDC_CROSS
        32642 => 0x0001_0005, // IDC_SIZENWSE
        32643 => 0x0001_0006, // IDC_SIZENESW
        32644 => 0x0001_0007, // IDC_SIZEWE
        32645 => 0x0001_0008, // IDC_SIZENS
        32646 => 0x0001_0009, // IDC_SIZEALL
        32649 => 0x0001_000A, // IDC_HAND
        _ => 0x0001_0001,
    }
}

pub fn set_cursor(cursor: usize) -> usize {
    CURSOR_HANDLE.swap(cursor, std::sync::atomic::Ordering::Relaxed)
}

pub fn show_cursor(show: bool) -> i32 {
    if show {
        SHOW_CURSOR_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
    } else {
        SHOW_CURSOR_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) - 1
    }
}

// ---------- DC map mode / viewport ----------

/// Renvoie le map mode du DC (par défaut MM_TEXT = 1).
pub fn dc_map_mode(hdc: usize) -> i32 {
    // Pas stocké dans DcState pour l'instant, toujours MM_TEXT.
    let _ = hdc;
    1
}

/// Définit le map mode ; renvoie l'ancien.
pub fn set_dc_map_mode(hdc: usize, _mode: i32) -> i32 {
    // stub : on retourne toujours MM_TEXT
    let _ = hdc;
    1
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn wm_size_dit_l_etat_de_la_fenetre() {
        assert_eq!(size_kind(WINDOW_NORMAL), 0);
        assert_eq!(size_kind(WINDOW_MINIMIZED), 1);
        assert_eq!(size_kind(WINDOW_MAXIMIZED), 2);
    }

    use super::*;
    use std::sync::atomic::Ordering;

    static KEYBOARD_GLOBAL_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn boutons_souris_dans_get_key_state() {
        // Thread neuf : son état clavier lui est propre.
        std::thread::spawn(|| {
            let msg = |message: u32, wparam: usize| Msg {
                hwnd: 0,
                message,
                wparam,
                lparam: 0,
                time: 0,
                pt: Point::default(),
                l_private: 0,
            };
            observe_key_message(&msg(0x0201, 0x0001));
            assert!(get_key_state(0x01) < 0);
            assert!(get_key_state(0x02) >= 0);
            // Glissé avec le bouton droit en plus, puis relâchements.
            observe_key_message(&msg(0x0200, 0x0003));
            assert!(get_key_state(0x02) < 0);
            observe_key_message(&msg(0x0202, 0x0002));
            assert!(get_key_state(0x01) >= 0);
            observe_key_message(&msg(0x0205, 0));
            assert!(get_key_state(0x02) >= 0);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn les_enfants_sont_places_dans_la_zone_client_du_parent() {
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
        let fenetre = |parent: usize, style: u32, rect: Rect| {
            let h = alloc_hwnd();
            insert_window(
                h,
                Window {
                    class: "Placement".to_string(),
                    wnd_proc: proc_addr as usize,
                    background: 0,
                    title: String::new(),
                    rect,
                    visible: true,
                    parent,
                    owner: std::thread::current().id(),
                    needs_paint: false,
                    paint_queued: false,
                    style,
                    ex_style: 0,
                    pending: Vec::new(),
                    menu_bar: 0,
                    painting: false,
                },
            );
            h
        };
        let haut = fenetre(0, 0, Rect::from_size(100, 50, 400, 300));
        let groupe = fenetre(haut, 0x4000_0000, Rect::from_size(10, 20, 200, 100));
        let bouton = fenetre(groupe, 0x4000_0000, Rect::from_size(5, 6, 80, 24));
        assert_eq!(screen_rect(haut), Some(Rect::from_size(100, 50, 400, 300)));
        assert_eq!(screen_rect(bouton), Some(Rect::from_size(115, 76, 80, 24)));
        assert_eq!(window_origin(bouton), (115, 76));
        // Fenêtre possédée (popup, sans WS_CHILD) : déjà en coordonnées écran.
        let popup = fenetre(haut, 0, Rect::from_size(300, 300, 50, 50));
        assert_eq!(screen_rect(popup), Some(Rect::from_size(300, 300, 50, 50)));
        for h in [bouton, groupe, popup, haut] {
            destroy_window(h);
        }
    }

    pub(crate) fn fenetre_essai(style: u32, ex_style: u32, parent: usize) -> Window {
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
        Window {
            class: "Chrome_RenderWidgetHostHWND".to_string(),
            wnd_proc: proc_addr as usize,
            background: 6,
            title: String::new(),
            rect: Rect::from_size(0, 37, 1000, 763),
            visible: true,
            parent,
            owner: std::thread::current().id(),
            needs_paint: false,
            paint_queued: false,
            style,
            ex_style,
            pending: Vec::new(),
            menu_bar: 0,
            painting: false,
        }
    }

    /// « Chrome Legacy Window » : WS_CHILD | WS_VISIBLE, WS_EX_TRANSPARENT,
    /// au-dessus de la zone client où le processus GPU présente (#1337).
    #[test]
    fn enfant_transparent_laisse_voir_le_parent() {
        // Fenêtres de premier niveau : l'ordre Z est global aux tests.
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let parent = alloc_hwnd();
        insert_window(parent, fenetre_essai(0x06c8_0000, 0, 0));
        let legacy = alloc_hwnd();
        insert_window(legacy, fenetre_essai(0x4600_0000, 0x20, parent));
        let opaque = alloc_hwnd();
        insert_window(opaque, fenetre_essai(0x4600_0000, 0, parent));
        let haut = alloc_hwnd();
        insert_window(haut, fenetre_essai(0x06c8_0000, 0x20, 0));
        assert!(is_see_through_child(legacy));
        assert!(!is_see_through_child(opaque));
        assert!(!is_see_through_child(haut));
        assert!(!is_see_through_child(parent));
        for h in [legacy, opaque, haut, parent] {
            destroy_window(h);
        }
    }

    #[test]
    fn wm_activateapp_au_passage_d_un_programme_a_l_autre() {
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
        let h = alloc_hwnd();
        insert_window(
            h,
            Window {
                class: "Activation".to_string(),
                wnd_proc: proc_addr as usize,
                background: 0,
                title: String::new(),
                rect: Rect::from_size(0, 0, 100, 100),
                visible: true,
                parent: 0,
                owner: std::thread::current().id(),
                needs_paint: false,
                paint_queued: false,
                style: 0,
                ex_style: 0,
                pending: Vec::new(),
                menu_bar: 0,
                painting: false,
            },
        );
        let q = current_queue();
        let recus = || {
            lock(&q.items)
                .iter()
                .filter(|m| m.hwnd == h && m.message == WM_ACTIVATEAPP)
                .map(|m| m.wparam)
                .collect::<Vec<_>>()
        };
        APP_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
        activate_app(true);
        activate_app(true);
        assert_eq!(recus(), vec![1]);
        activate_app(false);
        assert_eq!(recus(), vec![1, 0]);
        lock(&q.items).retain(|m| m.hwnd != h);
        destroy_window(h);
    }

    #[test]
    fn file_messages_par_thread_est_isolee() {
        let q = current_queue();
        lock(&q.items).clear();
        post_to_thread(
            std::thread::current().id(),
            Msg {
                hwnd: 1,
                message: WM_PAINT,
                wparam: 0,
                lparam: 0,
                time: 0,
                pt: Point::default(),
                l_private: 0,
            },
        );
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

    #[test]
    fn les_caracteres_clavier_sont_fifo_et_associes_au_message() {
        let hwnd = usize::MAX - 0x100;
        let vk = 0x41; // VK_A
        let lparam = (30isize << 16) | 1;
        set_key_char(hwnd, vk, lparam, Some(b'a' as u16));
        set_key_char(hwnd, vk, lparam, Some(b'A' as u16));
        assert_eq!(take_key_char(hwnd, vk, lparam), Some(b'a' as u16));
        assert_eq!(take_key_char(hwnd, vk, lparam), Some(b'A' as u16));

        // Une traduction X11 explicite « sans caractère » bloque le repli VK_A.
        let ctrl_lparam = (30isize << 16) | 2;
        set_key_char(hwnd, vk, ctrl_lparam, None);
        assert_eq!(take_key_char(hwnd, vk, ctrl_lparam), None);
        assert_eq!(take_key_char(hwnd, vk, ctrl_lparam), Some(b'a' as u16));

        discard_key_chars(hwnd);
    }

    #[test]
    fn translate_message_publie_wm_char_uniquement_pour_une_touche_traduite() {
        let hwnd = fenetre_de_test();
        let q = current_queue();

        let key = Msg::new(hwnd, WM_KEYDOWN, 0x41, (30isize << 16) | 1);
        assert_eq!(unsafe { crate::dlls::user32::translate_message(&key) }, 1);
        let ch = take_message(&q, hwnd, WM_CHAR, WM_CHAR).expect("WM_CHAR généré");
        assert_eq!(ch.wparam, b'a' as usize);
        assert_eq!(ch.lparam, key.lparam);

        let arrow = Msg::new(hwnd, WM_KEYDOWN, 0x25, (75isize << 16) | 1);
        assert_eq!(unsafe { crate::dlls::user32::translate_message(&arrow) }, 0);
        let ctrl_a_lparam = (30isize << 16) | 2;
        set_key_char(hwnd, 0x41, ctrl_a_lparam, None);
        let ctrl_a = Msg::new(hwnd, WM_KEYDOWN, 0x41, ctrl_a_lparam);
        assert_eq!(
            unsafe { crate::dlls::user32::translate_message(&ctrl_a) },
            0
        );
        let paint = Msg::new(hwnd, WM_PAINT, 0, 0);
        assert_eq!(unsafe { crate::dlls::user32::translate_message(&paint) }, 0);
        assert!(take_message(&q, hwnd, WM_CHAR, WM_CHAR).is_none());

        destroy_window(hwnd);
    }

    #[test]
    fn le_focus_est_isole_par_thread_et_les_evenements_x_sont_dedoublonnes() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let hwnd = fenetre_de_test();
        assert_eq!(focused_window(), 0);
        assert_eq!(set_focus(hwnd), Some(0));
        assert_eq!(focused_window(), hwnd);
        assert_eq!(set_focus(hwnd), Some(hwnd));
        // L'événement FocusIn correspondant à SetFocus ne doit pas notifier
        // une deuxième fois le noyau d'un changement déjà appliqué.
        assert_eq!(set_focus_from_backend(hwnd), None);
        assert!(clear_focus_from_backend(hwnd));
        assert_eq!(focused_window(), 0);
        assert_eq!(set_focus_from_backend(hwnd), Some(0));
        assert_eq!(focused_window(), hwnd);
        destroy_window(hwnd);
        assert_eq!(focused_window(), 0);
    }

    /// Qt lit GetAsyncKeyState(VK_LBUTTON) en traitant WM_LBUTTONDOWN : un
    /// clic dont pression et relâchement sont arrivés ensemble doit encore
    /// s'y lire pressé, puis relâché au WM_LBUTTONUP.
    #[test]
    fn bouton_de_souris_lu_presse_pendant_son_message() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        update_async_key_state(1, true);
        update_async_key_state(1, false);
        observe_key_message(&Msg::new(0, 0x0201, 1, 0));
        assert_ne!(get_async_key_state(1) as u16 & 0x8000, 0);
        assert_ne!(get_key_state(1) as u16 & 0x8000, 0);
        observe_key_message(&Msg::new(0, 0x0202, 0, 0));
        assert_eq!(get_async_key_state(1) as u16 & 0x8000, 0);
        assert_eq!(mouse_button_transition(0x020B, 2 << 16), Some((6, true)));
        assert_eq!(mouse_button_transition(0x0204, 2), Some((2, true)));
        assert_eq!(mouse_button_transition(0x0200, 0), None);
    }

    /// RegisterHotKey : une combinaison n'a qu'un titulaire, WM_HOTKEY va
    /// à la file du thread quand hwnd est nul (#1168).
    #[test]
    fn raccourci_global_unique_et_wm_hotkey() {
        let (ctrl_alt, f12) = (3, 0x7B);
        assert_eq!(register_hotkey(0, 7, ctrl_alt | 0x4000, f12), Ok(()));
        assert_eq!(
            register_hotkey(0, 8, ctrl_alt, f12),
            Err(ERROR_HOTKEY_ALREADY_REGISTERED)
        );
        assert_eq!(register_hotkey(0x7fff_0001, 9, 1, 0x20), Err(1400));
        assert!(hotkey_pressed(ctrl_alt, f12));
        let msg = lock(&current_queue().items)
            .iter()
            .find(|m| m.message == WM_HOTKEY)
            .map(|m| (m.hwnd, m.wparam, m.lparam));
        assert_eq!(msg, Some((0, 7, (0x7B << 16) | 3)));
        lock(&current_queue().items).retain(|m| m.message != WM_HOTKEY);
        assert_eq!(unregister_hotkey(0, 7), Ok(()));
        assert_eq!(unregister_hotkey(0, 7), Err(ERROR_HOTKEY_NOT_REGISTERED));
        assert!(!hotkey_pressed(ctrl_alt, f12));
        assert_eq!(register_hotkey(0, 8, ctrl_alt, f12), Ok(()));
        assert_eq!(unregister_hotkey(0, 8), Ok(()));
    }

    /// Le clic qui donne le focus reste tenu : SetFocus pendant
    /// WM_LBUTTONDOWN ne relâche pas VK_LBUTTON (DragDetect, #1138).
    #[test]
    fn perte_du_focus_garde_les_boutons_de_souris() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        observe_key_message(&Msg::new(0, 0x0201, 1, 0));
        update_thread_key_state(0x41, true);
        clear_current_thread_keyboard_state();
        clear_async_key_down_states();
        assert!(get_key_state(1) < 0, "VK_LBUTTON encore pressé");
        assert!(get_key_state(0x41) >= 0, "la touche A est relâchée");
        observe_key_message(&Msg::new(0, 0x0202, 0, 0));
        assert!(get_key_state(1) >= 0);
    }

    /// Qt lit GetKeyState(VK_LCONTROL…) pour ses raccourcis : Ctrl+O sans
    /// côté gauche/droit restait une simple frappe de « o ».
    #[test]
    fn modificateurs_gauche_et_droit_suivis() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let gauche = 0x1D << 16;
        let droite = (0x1D << 16) | (1 << 24);
        observe_key_message(&Msg::new(0, WM_KEYDOWN, 0x11, gauche));
        assert!(get_key_state(0x11) < 0 && get_key_state(0xA2) < 0);
        assert!(get_key_state(0xA3) >= 0);
        observe_key_message(&Msg::new(0, WM_KEYUP, 0x11, gauche));
        assert!(get_key_state(0x11) >= 0 && get_key_state(0xA2) >= 0);
        observe_key_message(&Msg::new(0, 0x0104, 0x12, droite));
        assert!(get_key_state(0xA5) < 0);
        observe_key_message(&Msg::new(0, 0x0105, 0x12, droite));
        assert!(get_key_state(0xA5) >= 0 && get_key_state(0x12) >= 0);
        assert_eq!(sided_modifier(0x10, 0x36 << 16), Some(0xA1));
        assert_eq!(sided_modifier(0x10, 0x2A << 16), Some(0xA0));
        assert_eq!(sided_modifier(0x41, 0), None);
    }

    #[test]
    fn etats_clavier_thread_async_et_touches_a_bascule() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let vk = 0xE7;
        update_thread_key_state(vk, true);
        assert_ne!(get_key_state(vk) as u16 & 0x8000, 0);
        assert_ne!(get_keyboard_state()[vk] & 0x80, 0);
        update_thread_key_state(vk, false);
        assert_eq!(get_key_state(vk) as u16 & 0x8000, 0);

        update_async_key_state(vk, true);
        assert_eq!(get_async_key_state(vk) as u16 & 0x8001, 0x8001);
        assert_eq!(get_async_key_state(vk) as u16 & 0x8001, 0x8000);
        update_async_key_state(vk, false);
        assert_eq!(get_async_key_state(vk), 0);

        update_thread_key_state(VK_CAPITAL, true);
        assert_eq!(get_key_state(VK_CAPITAL) as u16 & 0x8001, 0x8001);
        update_thread_key_state(VK_CAPITAL, false);
        assert_eq!(get_key_state(VK_CAPITAL) as u16 & 0x8001, 1);
        update_thread_key_state(VK_CAPITAL, true);
        assert_eq!(get_key_state(VK_CAPITAL) as u16 & 0x8001, 0x8000);
        update_thread_key_state(VK_CAPITAL, false);
    }

    /// WndProc inerte : `destroy_window` l'appelle de façon synchrone.
    extern "win64" fn wnd_proc_inerte(
        _hwnd: usize,
        _msg: u32,
        _wparam: usize,
        _lparam: isize,
    ) -> isize {
        0
    }

    static PEINTURES: AtomicUsize = AtomicUsize::new(0);

    extern "win64" fn wnd_proc_compte_peintures(
        hwnd: usize,
        msg: u32,
        _wparam: usize,
        _lparam: isize,
    ) -> isize {
        if msg == WM_PAINT {
            PEINTURES.fetch_add(1, Ordering::SeqCst);
            validate_window(hwnd);
        }
        0
    }

    /// #796 : UpdateWindow peint tout de suite une fenêtre invalidée, ne fait
    /// rien sur une fenêtre valide, et le WM_PAINT resté en file n'est plus
    /// délivré une seconde fois.
    #[test]
    fn update_window_peint_tout_de_suite_et_une_seule_fois() {
        let hwnd = fenetre_de_test();
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize =
            wnd_proc_compte_peintures;
        lock(windows()).get_mut(&hwnd).unwrap().wnd_proc = proc_addr as usize;
        let q = current_queue();
        PEINTURES.store(0, Ordering::SeqCst);
        request_paint(hwnd);
        assert_eq!(unsafe { crate::dlls::user32::update_window(hwnd) }, 1);
        assert_eq!(PEINTURES.load(Ordering::SeqCst), 1);
        assert!(take_message(&q, hwnd, WM_PAINT, WM_PAINT).is_none());
        unsafe { crate::dlls::user32::update_window(hwnd) };
        assert_eq!(PEINTURES.load(Ordering::SeqCst), 1);
        // Un WM_PAINT posté par l'appli elle-même reste délivré.
        post_to_thread(std::thread::current().id(), Msg::new(hwnd, WM_PAINT, 0, 0));
        assert!(take_message(&q, hwnd, WM_PAINT, WM_PAINT).is_some());
        destroy_window(hwnd);
    }

    /// InvalidateRect, ValidateRect puis InvalidateRect : la seconde
    /// invalidation donne bien un WM_PAINT, même si le premier attendait
    /// encore dans la file.
    #[test]
    fn invalidation_apres_validation_repeint() {
        let hwnd = fenetre_de_test();
        let q = current_queue();
        request_paint(hwnd);
        validate_window(hwnd);
        request_paint(hwnd);
        assert!(take_message(&q, hwnd, WM_PAINT, WM_PAINT).is_some());
        assert!(begin_paint(hwnd).is_some());
        // Message sauté (fenêtre déjà validée) : une invalidation suivante
        // en poste un nouveau.
        request_paint(hwnd);
        validate_window(hwnd);
        assert!(take_message(&q, hwnd, WM_PAINT, WM_PAINT).is_none());
        request_paint(hwnd);
        assert!(take_message(&q, hwnd, WM_PAINT, WM_PAINT).is_some());
        destroy_window(hwnd);
    }

    /// (fenêtre, message, fil rendu par GetWindowThreadProcessId, IsWindow).
    static DESTRUCTION: Mutex<Vec<(usize, u32, u32, i32)>> = Mutex::new(Vec::new());

    extern "win64" fn wnd_proc_destruction(hwnd: usize, msg: u32, _: usize, _: isize) -> isize {
        if msg == WM_DESTROY || msg == 0x0082 {
            let tid = unsafe {
                crate::dlls::user32::get_window_thread_process_id(hwnd, std::ptr::null_mut())
            };
            let alive = unsafe { crate::dlls::user32::is_window(hwnd) };
            lock(&DESTRUCTION).push((hwnd, msg, tid, alive));
            // WinForms détruit parfois de nouveau la fenêtre pendant WM_DESTROY.
            if msg == WM_DESTROY {
                assert!(destroy_window(hwnd));
            }
        }
        0
    }

    #[test]
    fn la_fenetre_existe_encore_pendant_wm_destroy() {
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_destruction;
        let parent = fenetre_de_test();
        let enfant = fenetre_de_test();
        for h in [parent, enfant] {
            lock(windows()).get_mut(&h).unwrap().wnd_proc = proc_addr as usize;
        }
        lock(windows()).get_mut(&enfant).unwrap().parent = parent;
        let tid = crate::dlls::kernel32::get_current_thread_id();
        assert!(destroy_window(parent));
        let vus: Vec<_> = lock(&DESTRUCTION)
            .iter()
            .filter(|e| e.0 == parent || e.0 == enfant)
            .copied()
            .collect();
        // Ordre Win32 : WM_DESTROY du parent, puis de l'enfant, puis les
        // WM_NCDESTROY ; la fenêtre et son fil restent connus jusqu'au bout.
        assert_eq!(
            vus,
            vec![
                (parent, WM_DESTROY, tid, 1),
                (enfant, WM_DESTROY, tid, 1),
                (enfant, 0x0082, tid, 1),
                (parent, 0x0082, tid, 1),
            ]
        );
        for h in [parent, enfant] {
            assert_eq!(unsafe { crate::dlls::user32::is_window(h) }, 0);
        }
    }

    fn fenetre_de_test() -> usize {
        let hwnd = alloc_hwnd();
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
        insert_window(
            hwnd,
            Window {
                class: "Test".to_string(),
                wnd_proc: proc_addr as usize,
                background: 0,
                title: "test".to_string(),
                rect: Rect::from_size(0, 0, 200, 100),
                visible: true,
                parent: 0,
                owner: std::thread::current().id(),
                needs_paint: false,
                paint_queued: false,
                style: 0,
                ex_style: 0,
                pending: Vec::new(),
                menu_bar: 0,
                painting: false,
            },
        );
        hwnd
    }

    #[test]
    fn les_objets_gdi_personnels_portent_leur_couleur() {
        let brush = create_solid_brush(0x00C8_6BF0);
        assert_ne!(brush, 0);
        assert_eq!(object_color(brush), Some(0x00C8_6BF0));
        let pen = create_pen(0, 3, 0x0012_3456);
        assert_eq!(object_color(pen), Some(0x0012_3456));
        assert_eq!(object_pen_width(pen), 3);
        assert!(delete_object(brush));
        assert_eq!(object_color(brush), None);
    }

    #[test]
    fn police_gdi_est_creee_stockee_et_selectionnee_par_le_dc() {
        let spec = FontSpec {
            height: 18,
            width: 0,
            weight: 700,
            italic: true,
            underline: false,
            strikeout: false,
            face: "fixed".to_string(),
        };
        let font = create_font(spec.clone());
        let dc = create_dc(0);
        assert_ne!(font, 0);
        assert_eq!(select_object(dc, font), 14); // SYSTEM_FONT stock
        assert_eq!(dc_font(dc), font);
        assert_eq!(font_spec(font), Some(spec));
        assert_eq!(select_object(dc, 14), font);
        assert!(delete_object(font));
        assert_eq!(font_spec(font), None);
        free_dc(dc);
    }

    #[test]
    fn wm_erasebkgnd_peint_le_fond_de_classe_dans_un_dc_memoire() {
        let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
        let h = alloc_hwnd();
        insert_window(
            h,
            Window {
                class: "Fond".to_string(),
                wnd_proc: proc_addr as usize,
                background: 16, // COLOR_BTNFACE + 1
                title: String::new(),
                rect: Rect::from_size(0, 0, 8, 8),
                visible: true,
                parent: 0,
                owner: std::thread::current().id(),
                needs_paint: false,
                paint_queued: false,
                style: 0,
                ex_style: 0,
                pending: Vec::new(),
                menu_bar: 0,
                painting: false,
            },
        );
        let dc = create_dc(0);
        let bmp = create_compatible_bitmap(8, 8).unwrap();
        select_object(dc, bmp);
        assert!(erase_background(h, dc));
        let pixel = bitmap_snapshot(dc, Rect::from_size(2, 2, 1, 1))
            .unwrap()
            .pixels[0];
        assert_eq!(pixel & 0x00FF_FFFF, crate::dlls::user32::get_sys_color(15));
        select_object(dc, DEFAULT_BITMAP);
        delete_object(bmp);
        free_dc(dc);
        destroy_window(h);
    }

    #[test]
    fn dc_memoire_neuf_rend_la_bitmap_du_stock() {
        let dc = create_dc(0);
        let bmp = create_compatible_bitmap(4, 3).unwrap();
        // Jamais 0 : la VCL de Delphi lève EOutOfResources sinon (WinSCP).
        assert_eq!(select_object(dc, bmp), DEFAULT_BITMAP);
        assert_eq!(dc_bitmap(dc), bmp);
        assert_eq!(select_object(dc, DEFAULT_BITMAP), bmp);
        assert_eq!(dc_bitmap(dc), 0);
        assert!(matches!(
            object_info(DEFAULT_BITMAP),
            Some(GdiObjectInfo::Bitmap {
                width: 1,
                height: 1,
                ..
            })
        ));
        assert!(delete_object(DEFAULT_BITMAP));
        assert!(delete_object(bmp));
        free_dc(dc);
    }

    #[test]
    fn les_rops_simples_combinent_source_et_destination() {
        assert_eq!(
            raster_color(0x00FF_0000, 0x0000_FF00, RasterOp::Copy),
            0x00FF_0000
        );
        assert_eq!(raster_color(0x00FF_0000, 0x0000_FF00, RasterOp::And), 0);
        assert_eq!(
            raster_color(0x00FF_0000, 0x0000_FF00, RasterOp::Or),
            0x00FF_FF00
        );
        assert_eq!(
            raster_color(0x00FF_0000, 0x0000_FF00, RasterOp::Xor),
            0x00FF_FF00
        );
        assert_eq!(raster_color(0, 0x0012_3456, RasterOp::Invert), 0x00ED_CBA9);
        assert_eq!(
            raster_color(0x0012_3456, 0, RasterOp::CopyInverted),
            0x00ED_CBA9
        );
        assert_eq!(raster_color(0x00FF_FFFF, 0, RasterOp::Set), 0x00FF_FFFF);
        assert_eq!(raster_color(0x00FF_FFFF, 0x00FF_FFFF, RasterOp::Clear), 0);
    }

    #[test]
    fn les_objets_stock_ont_les_couleurs_win32() {
        // GetStockObject renvoie index + 1 (convention Win32) : BLACK_BRUSH est
        // l'index 4, NULL_BRUSH l'index 5, BLACK_PEN l'index 7.
        let stock = crate::dlls::gdi32::get_stock_object;
        assert_eq!(object_color(stock(4)), Some(0x0000_0000)); // BLACK_BRUSH
        assert_eq!(object_color(stock(0)), Some(0x00FF_FFFF)); // WHITE_BRUSH
        assert_eq!(object_color(stock(5)), None); // NULL_BRUSH
        assert_eq!(object_pen_width(stock(7)), 1); // BLACK_PEN
    }

    #[test]
    fn pinceau_de_couleur_systeme_est_stable_et_non_supprimable() {
        let color = theme::system_color(29).expect("COLOR_MENUHILIGHT");
        let first = system_color_brush(29, color);
        let second = system_color_brush(29, color);
        assert_eq!(first, second);
        assert_eq!(object_color(first), Some(color));
        assert!(!delete_object(first));
        assert_eq!(object_color(first), Some(color));
    }

    #[test]
    fn un_dc_retient_couleurs_et_objet_selectionne() {
        let hwnd = fenetre_de_test();
        let hdc = create_dc(hwnd);
        assert_eq!(dc_window(hdc), Some(hwnd));
        // Valeurs initiales Win32 : texte noir, fond blanc, mode opaque.
        assert_eq!(set_dc_text_color(hdc, 0x00FF_FFFF), 0);
        assert_eq!(dc_text_color(hdc), 0x00FF_FFFF);
        assert_eq!(set_dc_text_color(hdc, 0x0000_00FF), 0x00FF_FFFF);
        assert_eq!(set_dc_bk_color(hdc, 0x0012_3456), 0x00FF_FFFF);

        // Une plume va dans l'emplacement plume, un pinceau dans l'emplacement
        // pinceau.
        let pen = create_pen(0, 2, 0x00AA_BBCC);
        let brush = create_solid_brush(0x0011_2233);
        assert_eq!(select_object(hdc, pen), 8); // BLACK_PEN stock initial
        assert_eq!(select_object(hdc, brush), 1); // WHITE_BRUSH stock initial
        assert_eq!(dc_brush(hdc), brush);

        free_dc(hdc);
        assert_eq!(dc_window(hdc), None);
        destroy_window(hwnd);
    }

    #[test]
    fn le_decoupage_retire_la_zone_exclue_des_remplissages() {
        // Bordure de PuTTY : IntersectClipRect(rcPaint), ExcludeClipRect(grille),
        // puis Rectangle(rcPaint) : seule la bordure est peinte (#771).
        let hwnd = fenetre_de_test();
        let hdc = create_dc(hwnd);
        let all = Rect::from_size(0, 0, 100, 50);
        let grid = Rect::from_size(2, 2, 96, 46);
        assert_eq!(intersect_dc_clip(hdc, all), 2);
        assert_eq!(exclude_dc_clip(hdc, grid), 3);
        let _ = take_pending(hwnd);
        assert!(record(
            hdc,
            DrawCmd::FillRect {
                rect: all,
                color: 0
            }
        ));
        let cmds = take_pending(hwnd);
        let area: i32 = cmds
            .iter()
            .map(|c| match c {
                DrawCmd::FillRect { rect, .. } => {
                    assert!(intersect_rects(rect, &grid).is_none());
                    rect.width() * rect.height()
                }
                _ => panic!("remplissage attendu"),
            })
            .sum();
        assert_eq!(area, 100 * 50 - 96 * 46);
        // Un texte entièrement dans la zone exclue disparaît.
        assert!(record(
            hdc,
            DrawCmd::Text {
                x: 40,
                y: 20,
                text: "x".into(),
                color: 0,
                bk: None,
                font: FontSpec::default(),
            }
        ));
        assert!(take_pending(hwnd).is_empty());
        // SelectClipRgn(NULL) : plus de découpage.
        assert_eq!(set_dc_clip(hdc, None), 2);
        assert!(record(
            hdc,
            DrawCmd::FillRect {
                rect: all,
                color: 0
            }
        ));
        assert_eq!(take_pending(hwnd).len(), 1);
        free_dc(hdc);
        destroy_window(hwnd);
    }

    #[test]
    fn soustraction_de_rectangles() {
        let a = Rect::from_size(0, 0, 10, 10);
        assert_eq!(subtract_rect(&a, &Rect::from_size(20, 20, 5, 5)), vec![a]);
        assert!(subtract_rect(&a, &Rect::from_size(-1, -1, 12, 12)).is_empty());
        let pieces = subtract_rect(&a, &Rect::from_size(0, 0, 10, 5));
        assert_eq!(pieces, vec![Rect::from_size(0, 5, 10, 5)]);
    }

    #[test]
    fn une_image_hors_wm_paint_part_sans_release_dc() {
        // SDL garde le DC de sa fenêtre (CS_OWNDC) et y copie sa surface
        // logicielle par BitBlt sans jamais appeler ReleaseDC (#1706).
        let hwnd = fenetre_de_test();
        let hdc = create_dc(hwnd);
        let rect = Rect::from_size(0, 0, 2, 2);
        let image = || DrawCmd::Bitmap {
            rect,
            pixels: vec![0x0028_28C8; 4],
            rop: RasterOp::Copy,
        };
        let _ = take_pending(hwnd);
        // Un remplissage attend ReleaseDC ou GdiFlush…
        assert!(record(hdc, DrawCmd::FillRect { rect, color: 0 }));
        // … mais l'image l'emporte avec elle vers l'écran.
        assert!(record(hdc, image()));
        assert!(take_pending(hwnd).is_empty());
        // GdiFlush vide aussi les dessins en attente.
        assert!(record(hdc, DrawCmd::FillRect { rect, color: 0 }));
        flush_all_paint_over();
        assert!(take_pending(hwnd).is_empty());
        free_dc(hdc);
        // Pendant BeginPaint, l'image attend EndPaint.
        let (paint_dc, _) = begin_paint(hwnd).expect("cycle de peinture");
        assert!(record(paint_dc, image()));
        assert_eq!(take_pending(hwnd).len(), 1);
        end_paint(hwnd, paint_dc);
        destroy_window(hwnd);
    }

    #[test]
    fn les_commandes_de_dessin_sont_accumulees_puis_prises() {
        let hwnd = fenetre_de_test();
        let hdc = create_dc(hwnd);
        let rect = Rect::from_size(10, 10, 40, 20);
        assert!(record(
            hdc,
            DrawCmd::FillRect {
                rect,
                color: 0x0000_00FF
            }
        ));
        assert!(record(
            hdc,
            DrawCmd::FrameRect {
                rect,
                color: 0x0000_00FF,
                width: 1
            }
        ));
        assert!(record(
            hdc,
            DrawCmd::Text {
                x: 4,
                y: 8,
                text: "salut".to_string(),
                color: 0x00FF_FFFF,
                bk: None,
                font: FontSpec::default(),
            }
        ));
        assert_eq!(take_pending(hwnd).len(), 3);
        // La prise vide la file : le prochain `EndPaint` ne rejoue rien.
        assert!(take_pending(hwnd).is_empty());

        // Un DC mémoire (pas de fenêtre cible) n'accumule pas de rendu.
        let mem = create_dc(0);
        assert!(!record(mem, DrawCmd::FillRect { rect, color: 0 }));
        free_dc(mem);
        free_dc(hdc);
        destroy_window(hwnd);
    }

    #[test]
    fn un_dc_inconnu_ne_recoit_aucune_commande() {
        assert!(!record(
            0x00DE_AD00,
            DrawCmd::FillRect {
                rect: Rect::from_size(0, 0, 1, 1),
                color: 0,
            }
        ));
        assert_eq!(dc_window(0x00DE_AD00), None);
        // Sans DC, les réglages sont sans effet et renvoient la valeur neutre.
        assert_eq!(set_dc_bk_mode(0x00DE_AD00, 1), 0);
    }

    #[test]
    fn la_ligne_est_clippee_au_rectangle_du_dc() {
        let clip = Rect::from_size(0, 0, 100, 50);
        // Entièrement visible : inchangée.
        assert_eq!(clip_line(1, 2, 30, 40, &clip, 1), Some((1, 2, 30, 40)));
        // Entièrement hors champ : rien à tracer.
        assert_eq!(clip_line(-10, -10, -5, -20, &clip, 1), None);
        assert_eq!(clip_line(200, 0, 300, 49, &clip, 1), None);
        // Traverse le rectangle : extrémités ramenées sur ses bords.
        assert_eq!(
            clip_line(-100, 25, 300, 25, &clip, 1),
            Some((0, 25, 99, 25))
        );
        // Coordonnées extrêmes : pas de débordement, segment borné au DC.
        let far = i32::MAX as i64;
        let (x1, y1, x2, y2) = clip_line(-far, -far, far, far, &clip, 1).unwrap();
        assert!((0..100).contains(&x1) && (0..50).contains(&y1));
        assert!((0..100).contains(&x2) && (0..50).contains(&y2));
        assert_eq!(
            clip_line(i32::MIN as i64, 0, i32::MAX as i64, 0, &clip, 1),
            Some((0, 0, 99, 0))
        );
        // Trait épais : une origine un peu à gauche déborde encore dans le DC.
        assert_eq!(clip_line(-2, 10, -2, 20, &clip, 3), Some((-2, 10, -2, 20)));
        assert_eq!(clip_line(-3, 10, -3, 20, &clip, 3), None);
        // DC vide : rien.
        assert_eq!(clip_line(0, 0, 1, 1, &Rect::default(), 1), None);
    }

    #[test]
    fn les_minuteurs_appartiennent_a_leur_thread() {
        let id = set_timer(0, 0, 0, 0);
        assert_ne!(id, 0);
        // Un autre thread ne voit ni n'arrête ce minuteur.
        let other = std::thread::spawn(move || (next_timer_ms(0), kill_timer(0, id)))
            .join()
            .unwrap();
        assert_eq!(other, (None, false));
        assert!(next_timer_ms(0).is_some_and(|ms| ms <= 10));
        std::thread::sleep(std::time::Duration::from_millis(15));
        let m = due_timer(0, 0, 0, false).expect("WM_TIMER échu sur le thread créateur");
        assert_eq!((m.message, m.wparam), (0x0113, id));
        assert!(kill_timer(0, id));
        assert!(!kill_timer(0, id));
        // Un minuteur de fenêtre appartient au thread de la fenêtre et
        // disparaît avec elle ; une HWND inconnue est refusée.
        let hwnd = fenetre_de_test();
        assert_eq!(set_timer(hwnd, 7, 1000, 0), 7);
        assert!(next_timer_ms(hwnd).is_some());
        destroy_window(hwnd);
        assert_eq!(next_timer_ms(hwnd), None);
        assert_eq!(set_timer(hwnd, 7, 1000, 0), 0);
    }

    #[test]
    fn detruire_une_fenetre_libere_capture_et_messages() {
        let hwnd = fenetre_de_test();
        let q = current_queue();
        set_capture(hwnd);
        assert_eq!(get_capture(), hwnd);
        assert!(post_to_window(hwnd, Msg::new(hwnd, WM_KEYDOWN, 0x41, 0)));
        assert!(peek_message(&q, hwnd, 0, 0).is_some());
        assert!(enum_top_level_windows().contains(&hwnd));
        assert!(destroy_window(hwnd));
        assert_eq!(get_capture(), 0);
        assert!(peek_message(&q, hwnd, 0, 0).is_none());
        assert!(!enum_top_level_windows().contains(&hwnd));
        assert!(!post_to_window(hwnd, Msg::new(hwnd, WM_KEYDOWN, 0x41, 0)));
    }

    #[test]
    fn wm_paint_est_reemis_tant_que_la_fenetre_ne_valide_pas() {
        let hwnd = fenetre_de_test();
        let q = current_queue();
        assert!(pending_paint(hwnd, 0, 0).is_none());
        request_paint(hwnd);
        let m = take_message(&q, hwnd, 0, 0).expect("WM_PAINT en file");
        assert_eq!(m.message, WM_PAINT);
        // Sans BeginPaint, la zone reste invalidée : WM_PAINT revient dès que
        // la file est vide, mais pas pour un filtre qui l'exclut.
        assert_eq!(pending_paint(hwnd, 0, 0).map(|m| m.hwnd), Some(hwnd));
        assert_eq!(pending_paint(0, 0, 0).map(|m| m.hwnd), Some(hwnd));
        assert!(pending_paint(hwnd, WM_KEYDOWN, WM_KEYDOWN).is_none());
        assert!(validate_window(hwnd));
        assert!(pending_paint(hwnd, 0, 0).is_none());
        // BeginPaint valide aussi.
        request_paint(hwnd);
        assert!(take_message(&q, hwnd, 0, 0).is_some());
        let (hdc, _) = begin_paint(hwnd).unwrap();
        end_paint(hwnd, hdc);
        assert!(pending_paint(hwnd, 0, 0).is_none());
        // DefWindowProc valide lui-même (comme BeginPaint/EndPaint sous Windows).
        request_paint(hwnd);
        assert!(take_message(&q, hwnd, 0, 0).is_some());
        def_window_proc(hwnd, WM_PAINT, 0, 0);
        assert!(pending_paint(hwnd, 0, 0).is_none());
        destroy_window(hwnd);
        assert!(pending_paint(hwnd, 0, 0).is_none());
    }

    #[test]
    fn setfocus_active_sa_fenetre_une_seule_fois() {
        // #1504 : SetFocus active la fenêtre de premier niveau tout de
        // suite ; le FocusIn de X11 qui suit ne doit plus rien reposter.
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let a = fenetre_de_test();
        let b = fenetre_de_test();
        let fille = fenetre_de_test();
        set_window_parent(fille, b);
        set_window_style(fille, 0x4000_0000); // WS_CHILD
        assert_eq!(top_level_of(fille), b);
        activate_top_for_focus(a);
        assert_eq!(activate_top_for_focus(top_level_of(fille)), Some(a));
        assert_eq!(activate_top_for_focus(b), None, "déjà active");
        for h in [fille, b, a] {
            destroy_window(h);
        }
        activate_top_for_focus(0);
    }

    #[test]
    fn un_clic_active_une_fenetre_surgissante_hors_gestionnaire() {
        // #1804 : boîte vgui de Steam, WS_POPUP + WS_EX_TOOLWINDOW, possédée.
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let principale = fenetre_de_test();
        let boite = fenetre_de_test();
        let bouton = fenetre_de_test();
        set_window_parent(boite, principale);
        set_window_style(boite, 0x8200_0000); // WS_POPUP | WS_CLIPCHILDREN
        set_window_ex_style(boite, 0x80); // WS_EX_TOOLWINDOW
        set_window_parent(bouton, boite);
        set_window_style(bouton, 0x4000_0000); // WS_CHILD
                                               // Possédée, elle reste de premier niveau : EnumWindows, donc
                                               // WindowFromPoint, la trouve ; le bouton (WS_CHILD) non.
        let z = enum_top_level_windows();
        assert!(z.contains(&boite) && !z.contains(&bouton));
        assert!(z.iter().position(|&h| h == boite) < z.iter().position(|&h| h == principale));
        assert_eq!(get_window_relative(boite, 4), Some(principale)); // GW_OWNER
        activate_top_for_focus(principale);
        assert_eq!(click_activation_target(boite), Some(boite));
        assert_eq!(
            click_activation_target(bouton),
            Some(boite),
            "clic sur un enfant"
        );
        assert_eq!(
            click_activation_target(principale),
            None,
            "déjà active, ou laissée au gestionnaire de fenêtres"
        );
        activate_top_for_focus(boite);
        assert_eq!(click_activation_target(boite), None, "déjà active");
        assert_eq!(active_top(), boite);
        activate_top_for_focus(principale);
        set_window_ex_style(boite, 0x80 | 0x0800_0000); // + WS_EX_NOACTIVATE
        assert_eq!(click_activation_target(boite), None);
        set_window_ex_style(boite, 0x80);
        set_window_style(boite, 0x8A00_0000); // + WS_DISABLED
        assert_eq!(click_activation_target(boite), None);
        set_window_style(boite, 0x8200_0000);
        // Fenêtre à barre de titre : le gestionnaire de fenêtres l'active.
        set_window_style(boite, 0x80C0_0000);
        assert_eq!(click_activation_target(boite), None);
        // WM_MOUSEACTIVATE laissé à DefWindowProc : MA_ACTIVATE.
        set_window_style(boite, 0x8200_0000);
        assert_eq!(def_window_proc(boite, 0x0021, boite, 0x0201_0001), 1);
        for h in [bouton, boite, principale] {
            destroy_window(h);
        }
        activate_top_for_focus(0);
    }

    #[test]
    fn un_menu_integre_ne_s_active_pas_au_clic() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let menu = fenetre_de_test();
        if let Some(w) = lock(windows()).get_mut(&menu) {
            w.class = crate::dlls::controls::popup::CLASS.to_string();
        }
        set_window_style(menu, 0x8000_0000);
        set_window_ex_style(menu, 0x88);
        activate_top_for_focus(0);
        assert_eq!(click_activation_target(menu), None);
        destroy_window(menu);
    }

    #[test]
    fn une_fenetre_desactivee_par_une_boite_modale_renvoie_a_la_boite() {
        // #1120 : la fenêtre principale d'Audacity, désactivée par la boîte
        // Tonalité, ne doit recevoir ni clic ni focus.
        let main = fenetre_de_test();
        let boite = fenetre_de_test();
        let fille = fenetre_de_test();
        set_window_parent(boite, main);
        set_window_parent(fille, main);
        set_window_style(fille, 0x4000_0000); // WS_CHILD
        assert!(!input_disabled(fille));
        set_window_style(main, 0x0800_0000); // WS_DISABLED (EnableWindow FALSE)
        assert!(input_disabled(main));
        assert!(
            input_disabled(fille),
            "une fille suit sa fenêtre principale"
        );
        assert!(!input_disabled(boite));
        assert_eq!(enabled_owned_popup(fille), Some(boite));
        // #1376 : la croix d'une fenêtre désactivée ne la ferme pas, elle
        // ramène la boîte ; la boîte elle-même se ferme normalement.
        assert_eq!(refused_while_disabled(main, WM_CLOSE), Some(true));
        assert_eq!(refused_while_disabled(fille, 0x0201), Some(true));
        assert_eq!(refused_while_disabled(fille, 0x0200), Some(false));
        assert_eq!(refused_while_disabled(boite, WM_CLOSE), None);
        assert_eq!(
            refused_while_disabled(main, 0x0100),
            None,
            "clavier : focus déjà sur la boîte"
        );
        set_window_style(main, 0);
        assert_eq!(refused_while_disabled(main, WM_CLOSE), None);
        for h in [fille, boite, main] {
            destroy_window(h);
        }
    }

    #[test]
    fn l_ordre_z_est_stable_et_suit_l_activation() {
        let _guard = lock(&KEYBOARD_GLOBAL_TEST_LOCK);
        let a = fenetre_de_test();
        let b = fenetre_de_test();
        let c = fenetre_de_test();
        let pos = |list: &[usize], h: usize| list.iter().position(|&x| x == h).unwrap();
        // Création : la plus récente au sommet.
        let z = enum_top_level_windows();
        assert!(pos(&z, c) < pos(&z, b) && pos(&z, b) < pos(&z, a));
        // Seul l'ordre relatif de a, b et c compte : d'autres tests qui
        // tournent en parallèle créent et détruisent leurs propres fenêtres
        // de premier niveau entre deux énumérations (#1484).
        let les_notres = |list: Vec<usize>| -> Vec<usize> {
            list.into_iter().filter(|h| [a, b, c].contains(h)).collect()
        };
        assert_eq!(
            les_notres(z),
            les_notres(enum_top_level_windows()),
            "ordre reproductible"
        );
        // Activation : la fenêtre activée passe devant, les autres gardent leur ordre.
        assert_eq!(set_focus(a), Some(0));
        let z = enum_top_level_windows();
        assert!(pos(&z, a) < pos(&z, c) && pos(&z, c) < pos(&z, b));
        bring_to_top(b);
        let z = enum_top_level_windows();
        assert!(pos(&z, b) < pos(&z, a) && pos(&z, a) < pos(&z, c));
        // Enfants : ordre stable par handle, navigation GW_* cohérente.
        let enfant = |parent: usize| {
            let h = alloc_hwnd();
            let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = wnd_proc_inerte;
            insert_window(
                h,
                Window {
                    class: "Enfant".to_string(),
                    wnd_proc: proc_addr as usize,
                    background: 0,
                    title: String::new(),
                    rect: Rect::from_size(0, 0, 10, 10),
                    visible: true,
                    parent,
                    owner: std::thread::current().id(),
                    needs_paint: false,
                    paint_queued: false,
                    style: 0x4000_0000, // WS_CHILD
                    ex_style: 0,
                    pending: Vec::new(),
                    menu_bar: 0,
                    painting: false,
                },
            );
            h
        };
        let d = enfant(a);
        let e = enfant(a);
        assert_eq!(get_window_relative(a, 5), Some(d)); // GW_CHILD
        assert_eq!(get_window_relative(d, 0), Some(d)); // GW_HWNDFIRST
        assert_eq!(get_window_relative(d, 1), Some(e)); // GW_HWNDLAST
        assert_eq!(get_window_relative(d, 2), Some(e)); // GW_HWNDNEXT
        assert_eq!(get_window_relative(e, 3), Some(d)); // GW_HWNDPREV
        assert_eq!(get_window_relative(e, 2), None);
        assert_eq!(get_window_relative(d, 4), Some(a)); // GW_OWNER
        assert!(!enum_top_level_windows().contains(&d));
        set_focus(0);
        for h in [a, b, c] {
            destroy_window(h);
        }
        let z = enum_top_level_windows();
        assert!(!z.contains(&a) && !z.contains(&b) && !z.contains(&c));
        assert_eq!(get_window_relative(d, 5), None);
    }

    #[test]
    fn une_region_est_un_vrai_objet_gdi() {
        let rgn = create_region(Rect {
            left: 10,
            top: 20,
            right: 5,
            bottom: 40,
        });
        assert!(
            rgn > 0x0010_0000,
            "pas un objet stock comme 1 = WHITE_BRUSH"
        );
        assert_eq!(
            region_rect(rgn),
            Some(Rect {
                left: 5,
                top: 20,
                right: 10,
                bottom: 40
            })
        );
        assert!(matches!(object_info(rgn), Some(GdiObjectInfo::Region(_))));
        assert_eq!(object_color(rgn), None);
        assert!(set_region_rect(rgn, Rect::from_size(0, 0, 3, 3)));
        assert!(delete_object(rgn));
        assert_eq!(region_rect(rgn), None);
        assert!(!set_region_rect(rgn, Rect::default()));
    }
}

// ---------- État brut clavier/souris pour DirectInput ----------
// DirectInput lit des touches physiques (codes de balayage) et des
// déplacements relatifs, indépendamment de la disposition du clavier.

struct RawInput {
    /// Touches enfoncées, indexées par code DIK (jeu 1, 0x80 = étendu).
    keys: [bool; 256],
    mouse_last: Option<(i32, i32)>,
    dx: i32,
    dy: i32,
    wheel: i32,
    /// Boutons DirectInput : 0 gauche, 1 droit, 2 milieu, 3-4 latéraux.
    buttons: [bool; 8],
}

fn raw_input() -> &'static Mutex<RawInput> {
    static R: OnceLock<Mutex<RawInput>> = OnceLock::new();
    R.get_or_init(|| {
        Mutex::new(RawInput {
            keys: [false; 256],
            mouse_last: None,
            dx: 0,
            dy: 0,
            wheel: 0,
            buttons: [false; 8],
        })
    })
}

/// Code evdev (keycode X11 − 8) vers code DIK DirectInput.
pub fn evdev_to_dik(code: u32) -> Option<u8> {
    Some(match code {
        1..=88 => code as u8,
        96 => 0x9C,  // pavé Entrée
        97 => 0x9D,  // Ctrl droit
        98 => 0xB5,  // pavé /
        99 => 0xB7,  // Impr écran
        100 => 0xB8, // Alt Gr
        102 => 0xC7, // Début
        103 => 0xC8, // Haut
        104 => 0xC9, // Page préc.
        105 => 0xCB, // Gauche
        106 => 0xCD, // Droite
        107 => 0xCF, // Fin
        108 => 0xD0, // Bas
        109 => 0xD1, // Page suiv.
        110 => 0xD2, // Inser
        111 => 0xD3, // Suppr
        119 => 0xC5, // Pause
        125 => 0xDB, // Windows gauche
        126 => 0xDC, // Windows droite
        127 => 0xDD, // Menu contextuel
        _ => return None,
    })
}

/// Appelé par le serveur graphique à chaque touche (keycode X11).
pub fn raw_key(x11_keycode: u8, down: bool) {
    if let Some(dik) = evdev_to_dik((x11_keycode as u32).saturating_sub(8)) {
        lock(raw_input()).keys[dik as usize] = down;
    }
}

pub fn raw_keys() -> [bool; 256] {
    lock(raw_input()).keys
}

/// Position écran du pointeur : accumule le déplacement relatif.
/// Dernière position connue du pointeur, en coordonnées écran logiques
/// (mouvements et clics reçus par une fenêtre Winlux).
static CURSOR_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// Tests qui posent puis relisent la position du pointeur (partagée).
#[cfg(test)]
pub static CURSOR_TEST_LOCK: Mutex<()> = Mutex::new(());

pub fn set_cursor_screen_pos(x: i32, y: i32) {
    *lock(&CURSOR_POS) = Some((x, y));
}

/// Position du pointeur pour GetCursorPos ; centre de l'écran tant que la
/// souris n'a encore rien fait sur une fenêtre Winlux.
pub fn cursor_screen_pos() -> (i32, i32) {
    let pos = lock(&CURSOR_POS).unwrap_or_else(|| {
        let (w, h) = screen_size();
        (w / 2, h / 2)
    });
    if crate::thunks::api_trace_active() {
        // Une note à chaque changement seulement : GetCursorPos est appelé
        // à chaque survol (#1222).
        static LAST: Mutex<Option<(i32, i32)>> = Mutex::new(None);
        let mut last = lock(&LAST);
        if *last != Some(pos) {
            *last = Some(pos);
            crate::thunks::api_note(&format!("GetCursorPos → ({}, {}) (#1222)", pos.0, pos.1));
        }
    }
    pos
}

/// Échelle de l'affichage en millièmes (1000 = 100 %), posée par le moteur
/// X11 à sa création ; lue sans prendre le verrou du moteur.
static DISPLAY_SCALE_MILLI: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1000);

pub fn set_display_scale(scale: f32) {
    let milli = (scale * 1000.0).round().clamp(1.0, 100_000.0) as u32;
    DISPLAY_SCALE_MILLI.store(milli, std::sync::atomic::Ordering::Relaxed);
}

/// Point de l'écran X (pixels réels) en coordonnées écran logiques de Win32.
pub fn logical_point(x: i32, y: i32) -> (i32, i32) {
    let scale = DISPLAY_SCALE_MILLI.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
    (
        (x as f32 / scale).round() as i32,
        (y as f32 / scale).round() as i32,
    )
}

pub fn raw_mouse_motion(x: i32, y: i32) {
    let mut r = lock(raw_input());
    if let Some((lx, ly)) = r.mouse_last {
        r.dx += x - lx;
        r.dy += y - ly;
    }
    r.mouse_last = Some((x, y));
}

/// Bouton X11 (1 gauche, 2 milieu, 3 droit, 4/5 molette, 8/9 latéraux).
pub fn raw_mouse_button(detail: u8, down: bool) {
    let vk = match detail {
        1 => 0x01,
        3 => 0x02,
        2 => 0x04,
        8 => 0x05,
        9 => 0x06,
        _ => 0,
    };
    if vk != 0 {
        update_async_key_state(vk, down);
    }
    let mut r = lock(raw_input());
    match detail {
        4 if down => r.wheel += 120,
        5 if down => r.wheel -= 120,
        1 => r.buttons[0] = down,
        3 => r.buttons[1] = down,
        2 => r.buttons[2] = down,
        8 => r.buttons[3] = down,
        9 => r.buttons[4] = down,
        _ => {}
    }
}

/// Déplacement (dx, dy, molette) depuis le dernier appel, et boutons.
pub fn take_raw_mouse() -> (i32, i32, i32, [bool; 8]) {
    let mut r = lock(raw_input());
    let out = (r.dx, r.dy, r.wheel, r.buttons);
    r.dx = 0;
    r.dy = 0;
    r.wheel = 0;
    out
}

#[cfg(test)]
mod tests_survol {
    use super::{hover_tick, Hover};
    use std::time::{Duration, Instant};

    /// Paint.NET (#1591) : WM_MOUSEHOVER après le délai sans bouger, une
    /// seule fois ; un mouvement hors du rectangle relance l'attente ; une
    /// fenêtre quittée abandonne le suivi.
    #[test]
    fn survol_poste_une_fois_apres_le_delai() {
        let t0 = Instant::now();
        let mut list = vec![Hover {
            hwnd: 7,
            ms: 400,
            anchor: (100, 100),
            since: t0,
        }];
        let over = |h: usize| h == 7;
        assert!(hover_tick(&mut list, (101, 99), t0 + Duration::from_millis(300), over).is_empty());
        // Sortie du rectangle de 4 pixels : l'attente repart de là.
        assert!(
            hover_tick(&mut list, (110, 100), t0 + Duration::from_millis(390), over).is_empty()
        );
        assert!(
            hover_tick(&mut list, (110, 100), t0 + Duration::from_millis(700), over).is_empty()
        );
        assert_eq!(
            hover_tick(&mut list, (111, 101), t0 + Duration::from_millis(800), over),
            vec![(7, (111, 101))]
        );
        assert!(list.is_empty());
        let mut list = vec![Hover {
            hwnd: 8,
            ms: 400,
            anchor: (0, 0),
            since: t0,
        }];
        assert!(hover_tick(&mut list, (0, 0), t0 + Duration::from_secs(1), over).is_empty());
        assert!(list.is_empty());
    }
}
