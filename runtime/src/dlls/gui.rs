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
    pub const ACCENT: u32 = 0x00C0_6700; // #0067C0
    pub const ACCENT_VIOLET: u32 = 0x00E6_8A74; // #748AE6
    pub const ACCENT_BLUE: u32 = 0x00E6_8A74; // #748AE6
    pub const ACCENT_CYAN: u32 = 0x00FF_CD60; // #60CDFF
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
    /// Commandes GDI accumulées depuis le dernier `EndPaint`.
    pub pending: Vec<DrawCmd>,
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
    let q = Arc::new(Queue {
        items: Mutex::new(VecDeque::new()),
        changed: Condvar::new(),
    });
    map.insert(tid, q.clone());
    q
}

fn queue_for(tid: ThreadId) -> Arc<Queue> {
    let mut map = lock(queues());
    if let Some(q) = map.get(&tid) {
        return q.clone();
    }
    let q = Arc::new(Queue {
        items: Mutex::new(VecDeque::new()),
        changed: Condvar::new(),
    });
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
    let mut map = lock(focused_windows());
    let old = map.get(&tid).copied().unwrap_or(0);
    if hwnd == 0 {
        map.remove(&tid);
    } else {
        map.insert(tid, hwnd);
    }
    Some(old)
}

/// Applique un changement de focus reçu du backend ; `None` signifie que
/// l'événement ne change pas l'état connu (ou que la fenêtre est détruite).
pub fn set_focus_from_backend(hwnd: usize) -> Option<usize> {
    let owner = window_owner(hwnd)?;
    let mut map = lock(focused_windows());
    let old = map.get(&owner).copied().unwrap_or(0);
    if old == hwnd {
        return None;
    }
    map.insert(owner, hwnd);
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
pub fn observe_key_message(msg: &Msg) {
    if msg.wparam >= KEY_STATE_COUNT {
        return;
    }
    match msg.message {
        WM_KEYDOWN => update_thread_key_state(msg.wparam, true),
        WM_KEYUP => update_thread_key_state(msg.wparam, false),
        _ => {}
    }
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
    let mut state = lock(async_keyboard_state());
    let down = state.keys[vk] & 0x80 != 0;
    let pressed = std::mem::replace(&mut state.pressed_since_query[vk], false);
    (((if down { 1u16 } else { 0 }) << 15) | if pressed { 1 } else { 0 }) as i16
}

fn clear_thread_keyboard_state(owner: &ThreadId) {
    if let Some(keys) = lock(thread_keyboard_states()).get_mut(owner) {
        for byte in keys {
            *byte &= 0x01; // conserve les états de bascule
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
    for byte in &mut state.keys {
        *byte &= 0x01;
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

fn discard_key_chars(hwnd: usize) {
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
    Bitmap {
        width: usize,
        height: usize,
        pixels: Vec<u32>,
        dib_bits: Option<Vec<u8>>,
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
fn next_handle(base: usize) -> usize {
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
            brush: 5, // BLACK_BRUSH stock
            pen: 8,   // BLACK_PEN stock
            font: 14, // SYSTEM_FONT stock
            bitmap: 0,
        },
    );
    hdc
}

/// Retire un DC (`ReleaseDC`, fin de `BeginPaint`).
pub fn free_dc(hdc: usize) {
    lock(dcs()).remove(&hdc);
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
        && matches!(lock(objects()).get(&obj), Some(GdiObject::Bitmap { .. }));
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
                let old = d.bitmap;
                d.bitmap = obj;
                old
            } else {
                let old = d.brush;
                d.brush = obj;
                old
            }
        },
        0,
    )
}

/// Objets « stock » plume : `GetStockObject(WHITE_PEN)` → 7, `BLACK_PEN` → 8,
/// `NULL_PEN` → 9.
fn is_stock_pen(handle: usize) -> bool {
    matches!(handle, 7..=9)
}

fn is_stock_font(handle: usize) -> bool {
    matches!(handle, 11 | 12 | 13 | 14 | 15 | 17 | 18)
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
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(handle, GdiObject::Font(spec));
    handle
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
    if width <= 0 || height <= 0 || !matches!(bit_count, 24 | 32) {
        return None;
    }
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > MAX_BITMAP_PIXELS {
        return None;
    }
    let stride = (width as usize * bit_count as usize).div_ceil(32) * 4;
    let bytes = stride.checked_mul(height as usize)?;
    let mut dib_bits = vec![0u8; bytes];
    let bits_ptr = dib_bits.as_mut_ptr();
    let handle = next_handle(0x0021_0000);
    lock(objects()).insert(
        handle,
        GdiObject::Bitmap {
            width: width as usize,
            height: height as usize,
            pixels: vec![0; pixels],
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
        let bytes_per_pixel = (dib_bit_count / 8) as usize;
        let row = if top_down { y } else { height - 1 - y };
        let offset = row * dib_stride + x * bytes_per_pixel;
        let b = bytes[offset] as u32;
        let g = bytes[offset + 1] as u32;
        let r = bytes[offset + 2] as u32;
        (b << 16) | (g << 8) | r
    } else {
        pixels[index]
    }
}

fn set_bitmap_color(
    width: usize,
    height: usize,
    pixels: &mut [u32],
    dib_bits: &mut Option<Vec<u8>>,
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
        let bytes_per_pixel = (dib_bit_count / 8) as usize;
        let row = if top_down { y } else { height - 1 - y };
        let offset = row * dib_stride + x * bytes_per_pixel;
        bytes[offset] = ((color >> 16) & 0xFF) as u8; // B
        bytes[offset + 1] = ((color >> 8) & 0xFF) as u8; // G
        bytes[offset + 2] = (color & 0xFF) as u8; // R
        if bytes_per_pixel == 4 {
            bytes[offset + 3] = 0;
        }
    }
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
        Some(_) => map.remove(&obj).is_some(),
        None => (1..=9).contains(&obj),
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
                GdiObject::Font(_) | GdiObject::Bitmap { .. } => None,
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

/// Ajoute une commande de dessin à la file d'attente de peinture de la fenêtre
/// cible du DC. Renvoie faux si le DC ou la fenêtre n'existe plus.
pub fn record(hdc: usize, cmd: DrawCmd) -> bool {
    let (target, bitmap) = match lock(dcs()).get(&hdc) {
        Some(d) => (d.hwnd, d.bitmap),
        None => return false,
    };
    if target == 0 {
        return bitmap != 0 && draw_to_bitmap(bitmap, &cmd);
    }
    let mut map = lock(windows());
    match map.get_mut(&target) {
        Some(w) => {
            w.pending.push(cmd);
            true
        }
        None => false,
    }
}

fn draw_to_bitmap(handle: usize, cmd: &DrawCmd) -> bool {
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
            // Bresenham simplifié
            let (mut cx, mut cy) = (*x1, *y1);
            let dx = (*x2 - *x1).abs();
            let dy = -(*y2 - *y1).abs();
            let sx = if *x1 < *x2 { 1 } else { -1 };
            let sy = if *y1 < *y2 { 1 } else { -1 };
            let mut err = dx + dy;
            loop {
                if cx >= 0 && cy >= 0 && cx < bounds.right && cy < bounds.bottom {
                    for oy in 0..(*width).max(1) as i32 {
                        for ox in 0..(*width).max(1) as i32 {
                            let (px, py) = (cx + ox, cy + oy);
                            if px < bounds.right && py < bounds.bottom {
                                put(px, py, *color, RasterOp::Copy);
                            }
                        }
                    }
                }
                if cx == *x2 && cy == *y2 {
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
        DrawCmd::Text { .. } | DrawCmd::Blit { .. } => {}
    }
    true
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
    fn set_window_rect(&mut self, _hwnd: usize, _rect: Rect) {}
    fn set_focus(&mut self, _hwnd: usize) {}
    fn invalidate(&mut self, _hwnd: usize) {}
    /// Lit une zone de la fenêtre en pixels COLORREF (pour les blits vers un DC mémoire).
    fn capture(&mut self, _hwnd: usize, _rect: Rect) -> Option<BitmapSnapshot> {
        None
    }
    /// Joue la liste de commandes GDI d'une fenêtre (thème + contenu). Le
    /// backend X11 conserve la liste pour la rejouer à chaque Expose; headless
    /// ignore le rendu.
    fn draw(&mut self, _hwnd: usize, _cmds: &[DrawCmd]) {}
    /// Attend des événements hôte pendant `timeout_ms`; renvoie `true` si le
    /// backend peut en produire (X11) et `false` s'il est headless, auquel cas
    /// une boucle `GetMessage` vide doit se terminer.
    fn wait(&mut self, _timeout_ms: u32) -> bool {
        false
    }
    /// Taille actuelle de l'écran en unités logiques Win32. Elle suit les
    /// changements de résolution (RandR) pour SM_CXSCREEN/SM_CYSCREEN.
    fn screen_size(&mut self) -> Option<(i32, i32)> {
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

pub fn screen_size() -> (i32, i32) {
    let mut size = None;
    with_backend(|b| size = b.screen_size());
    size.unwrap_or((1920, 1080))
}

/// Interroge le backend et publie les messages traduits dans les files cibles.
pub fn pump_backend() {
    let events = {
        let mut b = lock(backend());
        b.pump()
    };
    for e in events {
        if e.message == WM_SETFOCUS {
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
            continue;
        }
        if e.message == WM_SIZE {
            let packed = e.lparam as u32;
            update_window_size(e.hwnd, (packed & 0xFFFF) as i32, (packed >> 16) as i32);
        } else if e.message == WM_MOVE {
            let packed = e.lparam as u32;
            let x = (packed as u16 as i16) as i32;
            let y = ((packed >> 16) as u16 as i16) as i32;
            update_window_position(e.hwnd, x, y);
        }
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
                w.needs_paint = true;
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
                l_private: 0,
            },
        );
    }
}

pub fn destroy_window(hwnd: usize) -> bool {
    // Capture du wndproc et du parent avant retrait de la table.
    let (wnd_proc, parent, owner) = {
        let mut map = lock(windows());
        match map.remove(&hwnd) {
            Some(w) => (Some(w.wnd_proc), w.parent, Some(w.owner)),
            None => (None, 0, None),
        }
    };
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
    let _ = owner;
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
    lock(classes()).get(name).map(|c| ClassInfo {
        wnd_proc: c.wnd_proc,
        background: c.background,
        instance: c.instance,
    })
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
pub fn window_origin(hwnd: usize) -> (i32, i32) {
    lock(windows())
        .get(&hwnd)
        .map(|w| (w.rect.left, w.rect.top))
        .unwrap_or((0, 0))
}

pub fn window_rect(hwnd: usize) -> Option<Rect> {
    lock(windows()).get(&hwnd).map(|w| w.rect)
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
fn update_window_size(hwnd: usize, width: i32, height: i32) {
    if let Some(window) = lock(windows()).get_mut(&hwnd) {
        window.rect.right = window.rect.left.saturating_add(width);
        window.rect.bottom = window.rect.top.saturating_add(height);
    }
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

/// Ouvre un cycle de peinture : marque la zone comme validée et alloue un vrai
/// `HDC` invité ciblant la fenêtre.
pub fn begin_paint(hwnd: usize) -> Option<(usize, Rect)> {
    let rect = {
        let mut map = lock(windows());
        let w = map.get_mut(&hwnd)?;
        w.needs_paint = false;
        w.client_rect()
    };
    Some((create_dc(hwnd), rect))
}

/// Clôt le cycle : libère le DC et rejoue les commandes GDI chez le backend.
pub fn end_paint(_hwnd: usize, hdc: usize) -> bool {
    if hdc != 0 {
        free_dc(hdc);
    }
    flush_paint(_hwnd);
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

// ---------- Accesseurs étendus pour les couches user32/gdi32 ----------

/// Renvoie le style WS_* de la fenêtre.
pub fn window_style(hwnd: usize) -> Option<u32> {
    lock(windows()).get(&hwnd).map(|w| w.style)
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

/// Énumère les HWNDs de toutes les fenêtres de premier niveau.
pub fn enum_top_level_windows() -> Vec<usize> {
    lock(windows())
        .iter()
        .filter(|(_, w)| w.parent == 0)
        .map(|(h, _)| *h)
        .collect()
}

/// Cherche une fenêtre par nom de classe et/ou titre.
pub fn find_window(class: Option<&str>, title: Option<&str>) -> Option<usize> {
    lock(windows())
        .iter()
        .find(|(_, w)| {
            if let Some(c) = class {
                if !c.eq_ignore_ascii_case(&w.class) {
                    return false;
                }
            }
            if let Some(t) = title {
                if w.title != t {
                    return false;
                }
            }
            true
        })
        .map(|(h, _)| *h)
}

/// Renvoie un « voisin » de la fenêtre (GetWindow / GW_*).
pub fn get_window_relative(hwnd: usize, cmd: u32) -> Option<usize> {
    let map = lock(windows());
    if !map.contains_key(&hwnd) {
        return None;
    }
    // GW_HWNDFIRST=0 GW_HWNDLAST=1 GW_HWNDNEXT=2 GW_HWNDPREV=3
    // GW_OWNER=4 GW_CHILD=5
    match cmd {
        4 => {
            // GW_OWNER : même chose que parent ici
            map.get(&hwnd)
                .and_then(|w| if w.parent != 0 { Some(w.parent) } else { None })
        }
        5 => {
            // GW_CHILD : premier enfant
            let parent = hwnd;
            map.iter()
                .find(|(_, w)| w.parent == parent)
                .map(|(h, _)| *h)
        }
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
    let mut t = lock(timers());
    // Remplacer un timer existant avec le même (hwnd, id)
    t.retain(|e| !(e.hwnd == hwnd && e.id == id));
    let real_id = if id == 0 {
        NEXT_TIMER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    } else {
        id
    };
    t.push(TimerEntry {
        hwnd,
        id: real_id,
        interval_ms: interval,
        callback,
        next_fire: 0,
    });
    real_id
}

pub fn kill_timer(hwnd: usize, id: usize) -> bool {
    let mut t = lock(timers());
    let before = t.len();
    t.retain(|e| !(e.hwnd == hwnd && e.id == id));
    t.len() < before
}

// ---------- Curseur ----------

static CURSOR_HANDLE: AtomicUsize = AtomicUsize::new(0x0001_0000);
static SHOW_CURSOR_COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn load_cursor(_instance: usize, _name: usize) -> usize {
    // Renvoie un handle de curseur factice
    0x0001_0000
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
mod tests {
    use super::*;

    static KEYBOARD_GLOBAL_TEST_LOCK: Mutex<()> = Mutex::new(());

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
                style: 0,
                ex_style: 0,
                pending: Vec::new(),
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
        assert_eq!(select_object(hdc, brush), 5); // BLACK_BRUSH stock initial
        assert_eq!(dc_brush(hdc), brush);

        free_dc(hdc);
        assert_eq!(dc_window(hdc), None);
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
}
