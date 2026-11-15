//! WebView2 maison (#236) : `WebView2Loader.dll` et les interfaces
//! `ICoreWebView2*`, sur le moteur web WebKitGTK de l'hôte.
//!
//! Le vrai WebView2 repose sur Edge, exclu par la règle du projet. Une
//! application qui charge `WebView2Loader.dll` (la sienne, posée à côté de
//! son .exe, ou un `Microsoft.Web.WebView2.Core` .NET qui la charge) reçoit
//! donc celle de Winlux, qui passe avant le fichier. Les tables virtuelles
//! suivent l'IDL public du SDK (`webview2_iids.rs`) ; les méthodes des
//! versions récentes que Winlux ne rend pas encore répondent E_NOTIMPL.
//!
//! Chaque contrôleur lance `winlux-webview` (scripts/winlux-webview.py) :
//! une vue WebKitGTK placée dans la fenêtre X11 de l'application, pilotée
//! par des lignes JSON. Les événements reviennent sur un fil de lecture et
//! sont rejoués sur le fil de l'application (celui qui a créé
//! l'environnement), par une fenêtre cachée et son WndProc, comme les
//! rappels de WebView2 arrivent dans la boucle de messages.

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::comobj::{
    read_guid, Guid, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::webview2_iids::*;
use super::Exports;

const E_FAIL: i32 = 0x8000_4005u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND) : moteur absent ou refusé.
const E_ENGINE_MISSING: i32 = 0x8007_0002u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_INVALID_STATE) : contrôleur fermé.
const E_CLOSED: i32 = 0x8007_139Fu32 as i32;
/// Version annoncée du moteur : les applications comparent la leur à une
/// version minimale de WebView2 (format a.b.c.d).
const ENGINE_VERSION: &str = "130.0.2849.56";
const WM_WEBVIEW: u32 = 0x8000 + 0x2E5;

// ============================================================
//  Outils : chaînes, JSON, appels COM
// ============================================================

fn wstr(p: *const u16) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { super::wstr(p) }
    }
}

/// Chaîne allouée par CoTaskMemAlloc (l'application la libère).
unsafe fn co_string(s: &str, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let Some(alloc) = super::lookup("ole32", "CoTaskMemAlloc") else {
        return E_FAIL;
    };
    let alloc: unsafe extern "win64" fn(usize) -> *mut u16 = std::mem::transmute(alloc);
    let p = alloc(w.len() * 2);
    if p.is_null() {
        *out = std::ptr::null_mut();
        return 0x8007_000Eu32 as i32;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    S_OK
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                o.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Valeur d'un objet JSON plat (les messages de winlux-webview).
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

fn parse_flat(text: &str) -> Option<HashMap<String, Val>> {
    let mut chars = text.trim().chars().peekable();
    let mut map = HashMap::new();
    fn ws(c: &mut std::iter::Peekable<std::str::Chars>) {
        while c.peek().is_some_and(|x| x.is_whitespace()) {
            c.next();
        }
    }
    fn string(c: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
        if c.next()? != '"' {
            return None;
        }
        let mut s = String::new();
        loop {
            match c.next()? {
                '"' => return Some(s),
                '\\' => match c.next()? {
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| c.next()).collect();
                        let mut code = u32::from_str_radix(&hex, 16).ok()?;
                        if (0xD800..0xDC00).contains(&code) {
                            // Paire de substitution 😀.
                            if c.next()? != '\\' || c.next()? != 'u' {
                                return None;
                            }
                            let low: String = (0..4).filter_map(|_| c.next()).collect();
                            let low = u32::from_str_radix(&low, 16).ok()?;
                            code = 0x10000
                                + ((code - 0xD800) << 10)
                                + (low.wrapping_sub(0xDC00) & 0x3FF);
                        }
                        s.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => s.push(other),
                },
                ch => s.push(ch),
            }
        }
    }
    ws(&mut chars);
    if chars.next()? != '{' {
        return None;
    }
    loop {
        ws(&mut chars);
        if chars.peek() == Some(&'}') {
            break;
        }
        let key = string(&mut chars)?;
        ws(&mut chars);
        if chars.next()? != ':' {
            return None;
        }
        ws(&mut chars);
        let v = match chars.peek()? {
            '"' => Val::Str(string(&mut chars)?),
            't' | 'f' | 'n' => {
                let word: String =
                    std::iter::from_fn(|| chars.next_if(|c| c.is_ascii_alphabetic())).collect();
                match word.as_str() {
                    "true" => Val::Bool(true),
                    "false" => Val::Bool(false),
                    _ => Val::Null,
                }
            }
            _ => {
                let num: String = std::iter::from_fn(|| {
                    chars.next_if(|c| c.is_ascii_digit() || "+-.eE".contains(*c))
                })
                .collect();
                Val::Num(num.parse().ok()?)
            }
        };
        map.insert(key, v);
        ws(&mut chars);
        match chars.next()? {
            ',' => continue,
            '}' => break,
            _ => return None,
        }
    }
    Some(map)
}

fn get_str(m: &HashMap<String, Val>, k: &str) -> Option<String> {
    match m.get(k)? {
        Val::Str(s) => Some(s.clone()),
        _ => None,
    }
}
fn get_num(m: &HashMap<String, Val>, k: &str) -> f64 {
    match m.get(k) {
        Some(Val::Num(n)) => *n,
        _ => 0.0,
    }
}
fn get_bool(m: &HashMap<String, Val>, k: &str) -> bool {
    matches!(m.get(k), Some(Val::Bool(true)))
}

/// IUnknown::AddRef / Release d'un objet de l'application.
unsafe fn add_ref(obj: usize) {
    if obj != 0 {
        let vt = *(obj as *const *const usize);
        let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(1));
        f(obj);
    }
}
unsafe fn release(obj: usize) {
    if obj != 0 {
        let vt = *(obj as *const *const usize);
        let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(2));
        f(obj);
    }
}
/// Invoke(handler, a, b) : tous les gestionnaires WebView2 ont Invoke en 4e place.
unsafe fn invoke(handler: usize, a: usize, b: usize) -> i32 {
    if handler == 0 {
        return E_POINTER;
    }
    let vt = *(handler as *const *const usize);
    let f: extern "win64" fn(usize, usize, usize) -> i32 = std::mem::transmute(*vt.add(3));
    f(handler, a, b)
}

// ============================================================
//  Objets COM
// ============================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Environment,
    Controller,
    WebView,
    Settings,
    NavStarting,
    NavCompleted,
    Message,
    ContentLoading,
    SourceChanged,
    ControllerOptions,
    ProcessCollection,
}

impl Family {
    fn iids(self) -> &'static [Guid] {
        match self {
            Family::Environment => &ENVIRONMENT_IIDS,
            Family::Controller => &CONTROLLER_IIDS,
            Family::WebView => &WEBVIEW_IIDS,
            Family::Settings => &SETTINGS_IIDS,
            Family::NavStarting => &NAV_STARTING_ARGS_IIDS,
            Family::NavCompleted => &NAV_COMPLETED_ARGS_IIDS,
            Family::Message => &MESSAGE_ARGS_IIDS,
            Family::ContentLoading => &CONTENT_LOADING_ARGS_IIDS,
            Family::SourceChanged => &SOURCE_CHANGED_ARGS_IIDS,
            Family::ControllerOptions => &CONTROLLER_OPTIONS_IIDS,
            Family::ProcessCollection => &PROCESS_COLLECTION_IIDS,
        }
    }
}

/// Données d'un objet « arguments d'événement ».
#[derive(Default, Clone)]
struct Args {
    uri: String,
    nav: u64,
    ok: bool,
    status: i32,
    json: String,
    string: Option<String>,
}

/// Réglages d'un ICoreWebView2ControllerOptions (#2570).
#[derive(Default, Clone)]
struct ControllerOpts {
    profile: String,
    private: bool,
    locale: String,
    background: Option<u32>,
}

#[repr(C)]
struct Com {
    vtbl: *const usize,
    refs: AtomicU32,
    family: Family,
    env: Option<Arc<Env>>,
    view: Option<Arc<View>>,
    args: Args,
    opts: Mutex<ControllerOpts>,
    /// Objets hôtes de la vue (AddHostObjectToScript, #2596).
    hosts: Mutex<Vec<HostObject>>,
}

fn new_com(family: Family, env: Option<Arc<Env>>, view: Option<Arc<View>>, args: Args) -> usize {
    let b = Box::new(Com {
        vtbl: vtable(family).as_ptr(),
        refs: AtomicU32::new(1),
        family,
        env,
        view,
        args,
        opts: Mutex::new(ControllerOpts::default()),
        hosts: Mutex::new(Vec::new()),
    });
    Box::into_raw(b) as usize
}

unsafe fn com<'a>(this: usize) -> &'a Com {
    &*(this as *const Com)
}

extern "win64" fn query_interface(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    unsafe {
        if out.is_null() {
            return E_POINTER;
        }
        *out = 0;
        let Some(g) = read_guid(iid) else {
            return E_POINTER;
        };
        let c = com(this);
        if g == IID_IUNKNOWN || c.family.iids().contains(&g) {
            c.refs.fetch_add(1, Ordering::Relaxed);
            *out = this;
            return S_OK;
        }
        super::comobj::log_no_interface(&format!("WebView2 {:?}", c.family), iid);
        E_NOINTERFACE
    }
}
extern "win64" fn com_add_ref(this: usize) -> u32 {
    unsafe { com(this).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}
extern "win64" fn com_release(this: usize) -> u32 {
    unsafe {
        let left = com(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            let c = Box::from_raw(this as *mut Com);
            super::lock(&c.hosts).clear();
            // Le contrôleur libéré par l'application ferme sa vue.
            if c.family == Family::Controller {
                if let Some(v) = c.view.as_ref() {
                    v.close();
                }
            }
        }
        left
    }
}
extern "win64" fn not_implemented() -> i32 {
    E_NOTIMPL
}

/// Méthodes `add_X` de chaque famille (rang après IUnknown ; `remove_X`
/// suit toujours au rang suivant), dans l'ordre de l'IDL public WebView2
/// (SDK 1.0.2849.39, chaînes de `webview2_iids.rs`) (#2547).
fn event_slots(family: Family) -> &'static [usize] {
    match family {
        // ICoreWebView2 … ICoreWebView2_27 : NavigationStarting (4),
        // ContentLoading, SourceChanged, HistoryChanged, NavigationCompleted,
        // FrameNavigationStarting, FrameNavigationCompleted,
        // ScriptDialogOpening, PermissionRequested, ProcessFailed (22),
        // WebMessageReceived (31), NewWindowRequested (41),
        // DocumentTitleChanged (43), ContainsFullScreenElementChanged (49),
        // WebResourceRequested (52), WindowCloseRequested (56),
        // WebResourceResponseReceived (58), DOMContentLoaded (61),
        // FrameCreated (70), DownloadStarting (72),
        // ClientCertificateRequested (74), IsMutedChanged (78),
        // IsDocumentPlayingAudioChanged (82),
        // IsDefaultDownloadDialogOpenChanged (85),
        // BasicAuthenticationRequested (94), ContextMenuRequested (97),
        // StatusBarTextChanged (99), ServerCertificateErrorDetected (103),
        // FaviconChanged (106), LaunchingExternalUriScheme (114),
        // NotificationReceived (123), SaveAsUIShowing (125),
        // SaveFileSecurityCheckStarting (128), ScreenCaptureStarting (130).
        Family::WebView => &[
            4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 31, 41, 43, 49, 52, 56, 58, 61, 70, 72, 74, 78,
            82, 85, 94, 97, 99, 103, 106, 114, 123, 125, 128, 130,
        ],
        // ICoreWebView2Controller … Controller4 : ZoomFactorChanged (6),
        // MoveFocusRequested (10), GotFocus (12), LostFocus (14),
        // AcceleratorKeyPressed (16), RasterizationScaleChanged (29).
        Family::Controller => &[6, 10, 12, 14, 16, 29],
        // ICoreWebView2Environment … Environment14 :
        // NewBrowserVersionAvailable (3), BrowserProcessExited (9),
        // ProcessInfosChanged (13).
        Family::Environment => &[3, 9, 13],
        _ => &[],
    }
}

/// Table virtuelle d'une famille : IUnknown, les méthodes écrites, et
/// E_NOTIMPL pour le reste de la chaîne de versions.
fn vtable(family: Family) -> &'static [usize] {
    static TABLES: OnceLock<Mutex<HashMap<u8, &'static [usize]>>> = OnceLock::new();
    let mut t = super::lock(TABLES.get_or_init(|| Mutex::new(HashMap::new())));
    let key = family as u8;
    if let Some(v) = t.get(&key) {
        return v;
    }
    let (slots, methods): (usize, Vec<(usize, usize)>) = match family {
        Family::Environment => (ENVIRONMENT_SLOTS, environment_methods()),
        Family::Controller => (CONTROLLER_SLOTS, controller_methods()),
        Family::WebView => (WEBVIEW_SLOTS, webview_methods()),
        Family::Settings => (SETTINGS_SLOTS, settings_methods()),
        Family::NavStarting => (NAV_STARTING_ARGS_SLOTS, nav_starting_methods()),
        Family::NavCompleted => (NAV_COMPLETED_ARGS_SLOTS, nav_completed_methods()),
        Family::Message => (MESSAGE_ARGS_SLOTS, message_methods()),
        Family::ContentLoading => (CONTENT_LOADING_ARGS_SLOTS, content_loading_methods()),
        Family::SourceChanged => (SOURCE_CHANGED_ARGS_SLOTS, source_changed_methods()),
        Family::ControllerOptions => (CONTROLLER_OPTIONS_SLOTS, controller_options_methods()),
        Family::ProcessCollection => (PROCESS_COLLECTION_SLOTS, process_collection_methods()),
    };
    let mut v = vec![not_implemented as *const () as usize; slots];
    v[0] = query_interface as *const () as usize;
    v[1] = com_add_ref as *const () as usize;
    v[2] = com_release as *const () as usize;
    // Toute inscription à un événement est acceptée, même pour un
    // événement que Winlux ne déclenche jamais (#2547) : add_X rend un jeton,
    // remove_X l'accepte. Un E_NOTIMPL ici fait lever une exception à
    // l'application qui s'abonne (Teams, #2511).
    for &add in event_slots(family) {
        v[3 + add] = add_ignored as *const () as usize;
        v[3 + add + 1] = remove_ignored as *const () as usize;
    }
    for (i, f) in methods {
        v[3 + i] = f;
    }
    let leaked: &'static [usize] = Box::leak(v.into_boxed_slice());
    t.insert(key, leaked);
    leaked
}

// ============================================================
//  Fil de l'application : file des rappels
// ============================================================

type Job = Box<dyn FnOnce() + Send>;

fn queue() -> &'static Mutex<VecDeque<(usize, Job)>> {
    static Q: OnceLock<Mutex<VecDeque<(usize, Job)>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(VecDeque::new()))
}

extern "win64" fn dispatcher_proc(hwnd: usize, msg: u32, wp: usize, lp: usize) -> isize {
    if msg == WM_WEBVIEW {
        run_jobs(hwnd);
        return 0;
    }
    unsafe { super::user32::def_window_proc_w(hwnd, msg, wp, lp as isize) }
}

fn run_jobs(hwnd: usize) {
    loop {
        let job = {
            let mut q = super::lock(queue());
            let i = q.iter().position(|(h, _)| *h == hwnd);
            i.and_then(|i| q.remove(i))
        };
        match job {
            Some((_, f)) => f(),
            None => break,
        }
    }
}

/// Fenêtre cachée du fil courant, qui reçoit les rappels.
fn dispatcher_window() -> usize {
    thread_local!(static WIN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });
    WIN.with(|w| {
        if w.get() != 0 {
            return w.get();
        }
        unsafe {
            static REGISTERED: AtomicBool = AtomicBool::new(false);
            let class: Vec<u16> = "WinluxWebView2Dispatch\0".encode_utf16().collect();
            if !REGISTERED.swap(true, Ordering::SeqCst) {
                let wc = super::gui::WndClassExA {
                    size: std::mem::size_of::<super::gui::WndClassExA>() as u32,
                    style: 0,
                    wnd_proc: dispatcher_proc as *const () as usize,
                    cls_extra: 0,
                    wnd_extra: 0,
                    instance: 0,
                    icon: 0,
                    cursor: 0,
                    background: 0,
                    menu_name: std::ptr::null(),
                    class_name: class.as_ptr() as *const u8,
                    icon_sm: 0,
                };
                super::user32::register_class_ex_w(&wc);
            }
            let h = super::user32::create_window_ex_w(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            w.set(h);
            h
        }
    })
}

/// Rejoue `job` sur le fil de l'application (fenêtre `hwnd`).
fn post(hwnd: usize, job: Job) {
    super::lock(queue()).push_back((hwnd, job));
    unsafe {
        super::user32::post_message_w(hwnd, WM_WEBVIEW, 0, 0);
    }
}

// ============================================================
//  Environnement
// ============================================================

struct Env {
    dispatcher: usize,
    user_data: String,
    language: String,
}

/// Commande du moteur web : `WINLUX_WEBVIEW_HELPER` (« python3.12 x.py »
/// dans les tests), sinon /usr/bin/winlux-webview.
fn helper_command() -> Vec<String> {
    std::env::var("WINLUX_WEBVIEW_HELPER")
        .ok()
        .map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["/usr/bin/winlux-webview".to_string()])
}

fn helper_available() -> bool {
    let cmd = helper_command();
    std::path::Path::new(&cmd[0]).is_file()
        || std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&cmd[0]).is_file()))
}

unsafe extern "win64" fn create_environment_with_options(
    _browser_folder: *const u16,
    user_data_folder: *const u16,
    options: usize,
    handler: usize,
) -> i32 {
    if handler == 0 {
        return E_INVALIDARG;
    }
    let mut language = String::new();
    if options != 0 {
        // ICoreWebView2EnvironmentOptions::get_Language (place 5).
        let vt = *(options as *const *const usize);
        let get: extern "win64" fn(usize, *mut *mut u16) -> i32 = std::mem::transmute(*vt.add(5));
        let mut p: *mut u16 = std::ptr::null_mut();
        if get(options, &mut p) == S_OK && !p.is_null() {
            language = wstr(p);
        }
    }
    let env = Arc::new(Env {
        dispatcher: dispatcher_window(),
        user_data: crate::paths::to_linux(&wstr(user_data_folder))
            .to_string_lossy()
            .into_owned(),
        language,
    });
    crate::trace!("WebView2 : environnement (données {:?})", env.user_data);
    add_ref(handler);
    let obj = new_com(
        Family::Environment,
        Some(env.clone()),
        None,
        Args::default(),
    );
    // Comme WebView2 : le rappel arrive par la boucle de messages.
    post(
        env.dispatcher,
        Box::new(move || unsafe {
            invoke(handler, S_OK as usize, obj);
            release(obj);
            release(handler);
        }),
    );
    S_OK
}

unsafe extern "win64" fn create_environment(handler: usize) -> i32 {
    create_environment_with_options(std::ptr::null(), std::ptr::null(), 0, handler)
}

unsafe extern "win64" fn get_available_version(_folder: *const u16, out: *mut *mut u16) -> i32 {
    co_string(ENGINE_VERSION, out)
}

unsafe extern "win64" fn get_available_version_with_options(
    _folder: *const u16,
    _options: usize,
    out: *mut *mut u16,
) -> i32 {
    co_string(ENGINE_VERSION, out)
}

fn version_parts(v: &str) -> Vec<u64> {
    v.split(|c: char| c == '.' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .take(4)
        .map(|p| {
            p.trim_start_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

unsafe extern "win64" fn compare_browser_versions(
    a: *const u16,
    b: *const u16,
    result: *mut i32,
) -> i32 {
    if result.is_null() {
        return E_POINTER;
    }
    let (x, y) = (version_parts(&wstr(a)), version_parts(&wstr(b)));
    *result = match x.cmp(&y) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    S_OK
}

fn environment_methods() -> Vec<(usize, usize)> {
    vec![
        (0, env_create_controller as *const () as usize),
        (2, env_browser_version as *const () as usize),
        (3, add_ignored as *const () as usize),
        (4, remove_ignored as *const () as usize),
        // ICoreWebView2Environment10 (#2570).
        (17, env_create_controller_options as *const () as usize),
        (18, env_create_controller_with_options as *const () as usize),
        // ICoreWebView2Environment8 / 13 : processus du moteur (#2581).
        (15, env_get_process_infos as *const () as usize),
        (22, env_get_process_extended_infos as *const () as usize),
    ]
}

/// ICoreWebView2Environment8::GetProcessInfos : collection des processus
/// du navigateur. Le moteur de Winlux (WebKitGTK) n'expose pas de
/// processus au sens de WebView2 : la collection est vide (#2581).
unsafe extern "win64" fn env_get_process_infos(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_com(Family::ProcessCollection, None, None, Args::default());
    S_OK
}

/// ICoreWebView2Environment13::GetProcessExtendedInfos(handler) : rappel
/// asynchrone (boucle de messages, comme les autres rappels) avec S_OK et
/// une collection vide (#2581).
unsafe extern "win64" fn env_get_process_extended_infos(this: usize, handler: usize) -> i32 {
    if handler == 0 {
        return E_INVALIDARG;
    }
    let Some(env) = com(this).env.clone() else {
        return E_FAIL;
    };
    add_ref(handler);
    let obj = new_com(Family::ProcessCollection, None, None, Args::default());
    post(
        env.dispatcher,
        Box::new(move || unsafe {
            invoke(handler, S_OK as usize, obj);
            release(obj);
            release(handler);
        }),
    );
    S_OK
}

// --- ICoreWebView2ProcessInfoCollection / …ExtendedInfoCollection ---

fn process_collection_methods() -> Vec<(usize, usize)> {
    vec![
        (0, collection_count as *const () as usize),
        (1, collection_value_at as *const () as usize),
    ]
}
unsafe extern "win64" fn collection_count(_this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn collection_value_at(_this: usize, _index: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_INVALIDARG
}

/// ICoreWebView2Environment10::CreateCoreWebView2ControllerOptions : un
/// objet de réglages neuf (profil vide, navigation privée désactivée).
unsafe extern "win64" fn env_create_controller_options(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_com(
        Family::ControllerOptions,
        com(this).env.clone(),
        None,
        Args::default(),
    );
    S_OK
}

/// ICoreWebView2Environment10::CreateCoreWebView2ControllerWithOptions :
/// comme CreateCoreWebView2Controller. Les réglages (profil, navigation
/// privée, langue des scripts, couleur de fond) restent dans leur objet :
/// le moteur WebKitGTK de Winlux n'a qu'un profil par dossier de données,
/// et la couleur se règle ensuite par put_DefaultBackgroundColor.
unsafe extern "win64" fn env_create_controller_with_options(
    this: usize,
    parent: usize,
    _options: usize,
    handler: usize,
) -> i32 {
    env_create_controller(this, parent, handler)
}

// --- ICoreWebView2ControllerOptions (…3) ---

fn controller_options_methods() -> Vec<(usize, usize)> {
    vec![
        (0, opt_get_profile as *const () as usize),
        (1, opt_put_profile as *const () as usize),
        (2, opt_get_private as *const () as usize),
        (3, opt_put_private as *const () as usize),
        (4, opt_get_locale as *const () as usize),
        (5, opt_put_locale as *const () as usize),
        (6, opt_get_background as *const () as usize),
        (7, opt_put_background as *const () as usize),
    ]
}
unsafe extern "win64" fn opt_get_profile(this: usize, out: *mut *mut u16) -> i32 {
    let v = super::lock(&com(this).opts).profile.clone();
    co_string(&v, out)
}
unsafe extern "win64" fn opt_put_profile(this: usize, v: *const u16) -> i32 {
    super::lock(&com(this).opts).profile = wstr(v);
    S_OK
}
unsafe extern "win64" fn opt_get_private(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(super::lock(&com(this).opts).private);
    S_OK
}
unsafe extern "win64" fn opt_put_private(this: usize, on: i32) -> i32 {
    super::lock(&com(this).opts).private = on != 0;
    S_OK
}
unsafe extern "win64" fn opt_get_locale(this: usize, out: *mut *mut u16) -> i32 {
    let v = super::lock(&com(this).opts).locale.clone();
    co_string(&v, out)
}
unsafe extern "win64" fn opt_put_locale(this: usize, v: *const u16) -> i32 {
    super::lock(&com(this).opts).locale = wstr(v);
    S_OK
}
unsafe extern "win64" fn opt_get_background(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // COREWEBVIEW2_COLOR : blanc opaque par défaut (documentation de
    // DefaultBackgroundColor).
    *out = super::lock(&com(this).opts)
        .background
        .unwrap_or(0xFFFF_FFFF);
    S_OK
}
unsafe extern "win64" fn opt_put_background(this: usize, color: u32) -> i32 {
    super::lock(&com(this).opts).background = Some(color);
    S_OK
}

unsafe extern "win64" fn env_browser_version(_this: usize, out: *mut *mut u16) -> i32 {
    co_string(ENGINE_VERSION, out)
}

unsafe extern "win64" fn env_create_controller(this: usize, parent: usize, handler: usize) -> i32 {
    let c = com(this);
    let Some(env) = c.env.clone() else {
        return E_FAIL;
    };
    if handler == 0 {
        return E_INVALIDARG;
    }
    add_ref(handler);
    match View::start(env.clone(), parent, handler) {
        Ok(()) => S_OK,
        Err(code) => {
            post(
                env.dispatcher,
                Box::new(move || unsafe {
                    invoke(handler, code as u32 as usize, 0);
                    release(handler);
                }),
            );
            S_OK
        }
    }
}

// ============================================================
//  Vue : un moteur winlux-webview par contrôleur
// ============================================================

#[derive(Default)]
struct ViewState {
    bounds: [i32; 4],
    visible: bool,
    zoom: f64,
    source: String,
    title: String,
    can_back: bool,
    can_forward: bool,
    script_enabled: bool,
    messages_enabled: bool,
    devtools: bool,
    context_menus: bool,
    user_agent: String,
    background: u32,
    /// Hôtes virtuels → dossiers (SetVirtualHostNameToFolderMapping).
    hosts: Vec<(String, String)>,
}

struct View {
    env: Arc<Env>,
    parent: AtomicUsize,
    input: Mutex<Option<std::process::ChildStdin>>,
    child: Mutex<Option<std::process::Child>>,
    state: Mutex<ViewState>,
    /// Gestionnaires : (événement, jeton, gestionnaire de l'appli).
    handlers: Mutex<Vec<(&'static str, i64, usize)>>,
    pending_exec: Mutex<HashMap<u64, usize>>,
    next_exec: AtomicU64,
    /// Rappel « contrôleur créé », en attente de `ready`.
    pending_create: AtomicUsize,
    controller: AtomicUsize,
    webview: AtomicUsize,
    settings: AtomicUsize,
    closed: AtomicBool,
    pid: AtomicU32,
}

static TOKENS: AtomicI64 = AtomicI64::new(1);

impl View {
    fn start(env: Arc<Env>, parent: usize, handler: usize) -> Result<(), i32> {
        if !helper_available() {
            crate::log_warn!("webview2", "moteur web absent (winlux-webview introuvable)");
            return Err(E_ENGINE_MISSING);
        }
        let mut rect = super::gui::Rect::default();
        unsafe { super::user32::get_client_rect(parent, &mut rect) };
        let bounds = [rect.left, rect.top, rect.right, rect.bottom];
        let cmd = helper_command();
        let mut command = std::process::Command::new(&cmd[0]);
        command
            .args(&cmd[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .env_remove(crate::dlls::inherit::ENV)
            .env_remove(crate::dlls::handle_broker::ENV);
        if !env.language.is_empty() {
            command.env("WINLUX_WEBVIEW_LANG", &env.language);
        }
        if !env.user_data.is_empty() {
            command.env("WINLUX_WEBVIEW_DATA", &env.user_data);
        }
        let mut child = command.spawn().map_err(|e| {
            crate::log_warn!("webview2", "lancement du moteur web impossible : {e}");
            E_ENGINE_MISSING
        })?;
        let input = child.stdin.take();
        let output = child.stdout.take().ok_or(E_FAIL)?;
        let view = Arc::new(View {
            env: env.clone(),
            parent: AtomicUsize::new(parent),
            input: Mutex::new(input),
            pid: AtomicU32::new(child.id()),
            child: Mutex::new(Some(child)),
            state: Mutex::new(ViewState {
                bounds,
                visible: true,
                zoom: 1.0,
                script_enabled: true,
                messages_enabled: true,
                devtools: true,
                context_menus: true,
                background: 0xFFFF_FFFF,
                ..Default::default()
            }),
            handlers: Mutex::new(Vec::new()),
            pending_exec: Mutex::new(HashMap::new()),
            next_exec: AtomicU64::new(1),
            pending_create: AtomicUsize::new(handler),
            controller: AtomicUsize::new(0),
            webview: AtomicUsize::new(0),
            settings: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        });
        let xid = super::gui::native_window(parent).unwrap_or(0);
        view.send(&format!(
            "{{\"op\":\"create\",\"parent\":{xid},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"visible\":true}}",
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1]
        ));
        let reader = view.clone();
        std::thread::Builder::new()
            .name("winlux-webview".into())
            .spawn(move || reader.read_events(output))
            .map_err(|_| E_FAIL)?;
        Ok(())
    }

    fn send(&self, line: &str) {
        if let Some(i) = super::lock(&self.input).as_mut() {
            let _ = i.write_all(line.as_bytes());
            let _ = i.write_all(b"\n");
            let _ = i.flush();
        }
    }

    fn send_bounds(&self) {
        let s = super::lock(&self.state);
        let b = s.bounds;
        drop(s);
        self.send(&format!(
            "{{\"op\":\"bounds\",\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            b[0],
            b[1],
            (b[2] - b[0]).max(0),
            (b[3] - b[1]).max(0)
        ));
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.send("{\"op\":\"close\"}");
        *super::lock(&self.input) = None;
        if let Some(mut c) = super::lock(&self.child).take() {
            std::thread::spawn(move || {
                for _ in 0..30 {
                    if matches!(c.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let _ = c.kill();
                let _ = c.wait();
            });
        }
    }

    fn read_events(self: Arc<Self>, output: std::process::ChildStdout) {
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            let Some(ev) = parse_flat(&line) else {
                continue;
            };
            let view = self.clone();
            post(self.env.dispatcher, Box::new(move || view.on_event(ev)));
        }
        // Moteur parti : un contrôleur jamais prêt échoue.
        let view = self.clone();
        post(
            self.env.dispatcher,
            Box::new(move || {
                let h = view.pending_create.swap(0, Ordering::SeqCst);
                if h != 0 {
                    unsafe {
                        invoke(h, E_ENGINE_MISSING as u32 as usize, 0);
                        release(h);
                    }
                }
            }),
        );
    }

    /// Déclenche les gestionnaires de `event` (sur le fil de l'appli).
    fn fire(&self, event: &str, args: usize) {
        let list: Vec<usize> = super::lock(&self.handlers)
            .iter()
            .filter(|(e, _, _)| *e == event)
            .map(|(_, _, h)| *h)
            .collect();
        let sender = self.webview.load(Ordering::SeqCst);
        for h in list {
            unsafe { invoke(h, sender, args) };
        }
    }

    fn fire_args(self: &Arc<Self>, event: &str, family: Family, args: Args) {
        let obj = new_com(family, None, Some(self.clone()), args);
        self.fire(event, obj);
        unsafe { release(obj) };
    }

    fn on_event(self: Arc<Self>, ev: HashMap<String, Val>) {
        let kind = get_str(&ev, "ev").unwrap_or_default();
        let nav = get_num(&ev, "nav") as u64;
        match kind.as_str() {
            "ready" => {
                let ctrl = new_com(
                    Family::Controller,
                    Some(self.env.clone()),
                    Some(self.clone()),
                    Args::default(),
                );
                let web = new_com(
                    Family::WebView,
                    Some(self.env.clone()),
                    Some(self.clone()),
                    Args::default(),
                );
                let settings = new_com(Family::Settings, None, Some(self.clone()), Args::default());
                self.controller.store(ctrl, Ordering::SeqCst);
                self.webview.store(web, Ordering::SeqCst);
                self.settings.store(settings, Ordering::SeqCst);
                let h = self.pending_create.swap(0, Ordering::SeqCst);
                crate::trace!("WebView2 : moteur prêt (fenêtre X {})", get_num(&ev, "xid"));
                if h != 0 {
                    unsafe {
                        invoke(h, S_OK as usize, ctrl);
                        release(h);
                        // Le contrôleur reste à l'application (sa référence).
                    }
                } else {
                    unsafe { release(ctrl) };
                }
            }
            "error" => {
                crate::log_warn!(
                    "webview2",
                    "moteur web : {}",
                    get_str(&ev, "code").unwrap_or_default()
                );
            }
            "nav_starting" => {
                let uri = self.public_uri(&get_str(&ev, "uri").unwrap_or_default());
                self.fire_args(
                    "NavigationStarting",
                    Family::NavStarting,
                    Args {
                        uri,
                        nav,
                        ..Default::default()
                    },
                );
            }
            "content_loading" => self.fire_args(
                "ContentLoading",
                Family::ContentLoading,
                Args {
                    nav,
                    ..Default::default()
                },
            ),
            "source" => {
                let uri = self.public_uri(&get_str(&ev, "uri").unwrap_or_default());
                let changed = {
                    let mut s = super::lock(&self.state);
                    let changed = s.source != uri;
                    s.source = uri;
                    changed
                };
                if changed {
                    self.fire_args(
                        "SourceChanged",
                        Family::SourceChanged,
                        Args {
                            ok: true,
                            ..Default::default()
                        },
                    );
                }
            }
            "history" => {
                {
                    let mut s = super::lock(&self.state);
                    s.can_back = get_bool(&ev, "back");
                    s.can_forward = get_bool(&ev, "forward");
                }
                self.fire("HistoryChanged", 0);
            }
            "nav_completed" => self.fire_args(
                "NavigationCompleted",
                Family::NavCompleted,
                Args {
                    nav,
                    ok: get_bool(&ev, "ok"),
                    status: if get_bool(&ev, "ok") { 0 } else { 2 },
                    ..Default::default()
                },
            ),
            "title" => {
                super::lock(&self.state).title = get_str(&ev, "title").unwrap_or_default();
                self.fire("DocumentTitleChanged", 0);
            }
            "message" => {
                if !super::lock(&self.state).messages_enabled {
                    return;
                }
                let source = self.public_uri(&get_str(&ev, "source").unwrap_or_default());
                self.fire_args(
                    "WebMessageReceived",
                    Family::Message,
                    Args {
                        uri: source,
                        json: get_str(&ev, "json").unwrap_or_else(|| "null".into()),
                        string: get_str(&ev, "string"),
                        ..Default::default()
                    },
                );
            }
            "exec_result" => {
                let id = get_num(&ev, "id") as u64;
                if let Some(h) = super::lock(&self.pending_exec).remove(&id) {
                    let json = get_str(&ev, "json").unwrap_or_else(|| "null".into());
                    let w: Vec<u16> = json.encode_utf16().chain(std::iter::once(0)).collect();
                    unsafe {
                        invoke(h, S_OK as usize, w.as_ptr() as usize);
                        release(h);
                    }
                }
            }
            _ => {}
        }
    }

    /// Adresse vue par l'application : l'hôte virtuel reprend sa forme
    /// https:// (le moteur le sert sous winlux-local://).
    fn public_uri(&self, uri: &str) -> String {
        match uri.strip_prefix("winlux-local://") {
            Some(rest) => format!("https://{rest}"),
            None => uri.to_string(),
        }
    }

    fn add_handler(&self, event: &'static str, handler: usize, token: *mut i64) -> i32 {
        if handler == 0 {
            return E_INVALIDARG;
        }
        unsafe { add_ref(handler) };
        let t = TOKENS.fetch_add(1, Ordering::Relaxed);
        super::lock(&self.handlers).push((event, t, handler));
        if !token.is_null() {
            unsafe { *token = t };
        }
        S_OK
    }

    fn remove_handler(&self, token: i64) -> i32 {
        let mut list = super::lock(&self.handlers);
        if let Some(i) = list.iter().position(|(_, t, _)| *t == token) {
            let (_, _, h) = list.remove(i);
            drop(list);
            unsafe { release(h) };
        }
        S_OK
    }
}

unsafe fn view_of<'a>(this: usize) -> Option<&'a Arc<View>> {
    com(this).view.as_ref()
}

unsafe extern "win64" fn add_ignored(_this: usize, _handler: usize, token: *mut i64) -> i32 {
    if !token.is_null() {
        *token = TOKENS.fetch_add(1, Ordering::Relaxed);
    }
    S_OK
}
unsafe extern "win64" fn remove_ignored(_this: usize, _token: i64) -> i32 {
    S_OK
}
unsafe extern "win64" fn ok0(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn ok1(_this: usize, _a: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn ok2(_this: usize, _a: usize, _b: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn get_false(_this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

// ============================================================
//  ICoreWebView2Controller
// ============================================================

fn controller_methods() -> Vec<(usize, usize)> {
    vec![
        (0, ctrl_get_visible as *const () as usize),
        (1, ctrl_put_visible as *const () as usize),
        (2, ctrl_get_bounds as *const () as usize),
        (3, ctrl_put_bounds as *const () as usize),
        (4, ctrl_get_zoom as *const () as usize),
        (5, ctrl_put_zoom as *const () as usize),
        (6, add_ignored as *const () as usize),
        (7, remove_ignored as *const () as usize),
        (8, ctrl_set_bounds_zoom as *const () as usize),
        (9, ok1 as *const () as usize),
        (10, add_ignored as *const () as usize),
        (11, remove_ignored as *const () as usize),
        (12, add_ignored as *const () as usize),
        (13, remove_ignored as *const () as usize),
        (14, add_ignored as *const () as usize),
        (15, remove_ignored as *const () as usize),
        (16, add_ignored as *const () as usize),
        (17, remove_ignored as *const () as usize),
        (18, ctrl_get_parent as *const () as usize),
        (19, ctrl_put_parent as *const () as usize),
        (20, ok0 as *const () as usize),
        (21, ctrl_close as *const () as usize),
        (22, ctrl_core_webview as *const () as usize),
        // ICoreWebView2Controller2 : couleur de fond.
        (23, ctrl_get_background as *const () as usize),
        (24, ctrl_put_background as *const () as usize),
    ]
}

unsafe extern "win64" fn ctrl_get_visible(this: usize, out: *mut i32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(super::lock(&v.state).visible);
    S_OK
}
unsafe extern "win64" fn ctrl_put_visible(this: usize, on: i32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    super::lock(&v.state).visible = on != 0;
    v.send(&format!("{{\"op\":\"visible\",\"on\":{}}}", on != 0));
    S_OK
}
unsafe extern "win64" fn ctrl_get_bounds(this: usize, out: *mut [i32; 4]) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = super::lock(&v.state).bounds;
    S_OK
}
/// put_Bounds(RECT) : un RECT de 16 octets passe par adresse en x64.
unsafe extern "win64" fn ctrl_put_bounds(this: usize, rect: *const [i32; 4]) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if rect.is_null() {
        return E_INVALIDARG;
    }
    super::lock(&v.state).bounds = std::ptr::read_unaligned(rect);
    v.send_bounds();
    S_OK
}
unsafe extern "win64" fn ctrl_get_zoom(this: usize, out: *mut f64) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = super::lock(&v.state).zoom;
    S_OK
}
unsafe extern "win64" fn ctrl_put_zoom(this: usize, zoom: f64) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    super::lock(&v.state).zoom = zoom;
    v.send(&format!("{{\"op\":\"settings\",\"zoom\":{zoom}}}"));
    S_OK
}
unsafe extern "win64" fn ctrl_set_bounds_zoom(
    this: usize,
    rect: *const [i32; 4],
    zoom: f64,
) -> i32 {
    ctrl_put_bounds(this, rect);
    ctrl_put_zoom(this, zoom)
}
unsafe extern "win64" fn ctrl_get_parent(this: usize, out: *mut usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = v.parent.load(Ordering::SeqCst);
    S_OK
}
unsafe extern "win64" fn ctrl_put_parent(this: usize, parent: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.parent.store(parent, Ordering::SeqCst);
    let xid = super::gui::native_window(parent).unwrap_or(0);
    v.send(&format!("{{\"op\":\"reparent\",\"parent\":{xid}}}"));
    v.send_bounds();
    S_OK
}
unsafe extern "win64" fn ctrl_close(this: usize) -> i32 {
    if let Some(v) = view_of(this) {
        v.close();
    }
    S_OK
}
unsafe extern "win64" fn ctrl_core_webview(this: usize, out: *mut usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    if v.closed.load(Ordering::SeqCst) {
        *out = 0;
        return E_CLOSED;
    }
    let w = v.webview.load(Ordering::SeqCst);
    add_ref(w);
    *out = w;
    S_OK
}
unsafe extern "win64" fn ctrl_get_background(this: usize, out: *mut u32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = super::lock(&v.state).background;
    S_OK
}
/// put_DefaultBackgroundColor(COREWEBVIEW2_COLOR {A,R,G,B}) : 4 octets, par valeur.
unsafe extern "win64" fn ctrl_put_background(this: usize, color: u32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    super::lock(&v.state).background = color;
    let [a, r, g, b] = color.to_le_bytes();
    v.send(&format!(
        "{{\"op\":\"background\",\"a\":{a},\"r\":{r},\"g\":{g},\"b\":{b}}}"
    ));
    S_OK
}

// ============================================================
//  ICoreWebView2 (…_27)
// ============================================================

fn webview_methods() -> Vec<(usize, usize)> {
    let mut m = vec![
        (0, web_settings as *const () as usize),
        (1, web_source as *const () as usize),
        (2, web_navigate as *const () as usize),
        (3, web_navigate_to_string as *const () as usize),
        (4, add_navigation_starting as *const () as usize),
        (5, web_remove as *const () as usize),
        (6, add_content_loading as *const () as usize),
        (7, web_remove as *const () as usize),
        (8, add_source_changed as *const () as usize),
        (9, web_remove as *const () as usize),
        (10, add_history_changed as *const () as usize),
        (11, web_remove as *const () as usize),
        (12, add_navigation_completed as *const () as usize),
        (13, web_remove as *const () as usize),
        (24, web_add_script as *const () as usize),
        (25, ok1 as *const () as usize),
        (26, web_execute_script as *const () as usize),
        (28, web_reload as *const () as usize),
        (29, web_post_json as *const () as usize),
        (30, web_post_string as *const () as usize),
        (31, add_web_message as *const () as usize),
        (32, web_remove as *const () as usize),
        (34, web_process_id as *const () as usize),
        (35, web_can_go_back as *const () as usize),
        (36, web_can_go_forward as *const () as usize),
        (37, web_go_back as *const () as usize),
        (38, web_go_forward as *const () as usize),
        (40, web_stop as *const () as usize),
        (43, add_title_changed as *const () as usize),
        (44, web_remove as *const () as usize),
        (45, web_title as *const () as usize),
        (46, web_add_host_object as *const () as usize),
        (47, web_remove_host_object as *const () as usize),
        (48, ok0 as *const () as usize),
        (51, get_false as *const () as usize),
        (54, ok2 as *const () as usize),
        (55, ok2 as *const () as usize),
    ];
    // Événements acceptés mais jamais déclenchés (cadres, dialogues,
    // autorisations, nouvelles fenêtres, requêtes, fermeture…).
    for (add, remove) in [
        (14, 15),
        (16, 17),
        (18, 19),
        (20, 21),
        (22, 23),
        (41, 42),
        (49, 50),
        (52, 53),
        (56, 57),
    ] {
        m.push((add, add_ignored as *const () as usize));
        m.push((remove, remove_ignored as *const () as usize));
    }
    // ICoreWebView2_2 : DOMContentLoaded (accepté), Environment.
    let base2 = 58;
    m.push((base2, add_ignored as *const () as usize));
    m.push((base2 + 1, remove_ignored as *const () as usize));
    m.push((base2 + 3, add_ignored as *const () as usize));
    m.push((base2 + 4, remove_ignored as *const () as usize));
    m.push((base2 + 6, web_environment as *const () as usize));
    // ICoreWebView2_3 : hôtes virtuels.
    let base3 = base2 + 7;
    m.push((base3 + 2, get_false as *const () as usize));
    m.push((base3 + 3, web_map_host as *const () as usize));
    m.push((base3 + 4, web_unmap_host as *const () as usize));
    m
}

unsafe extern "win64" fn web_settings(this: usize, out: *mut usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    let s = v.settings.load(Ordering::SeqCst);
    add_ref(s);
    *out = s;
    S_OK
}
unsafe extern "win64" fn web_source(this: usize, out: *mut *mut u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let s = super::lock(&v.state).source.clone();
    co_string(if s.is_empty() { "about:blank" } else { &s }, out)
}
unsafe extern "win64" fn web_navigate(this: usize, uri: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let uri = wstr(uri);
    if uri.is_empty() {
        return E_INVALIDARG;
    }
    v.send(&format!(
        "{{\"op\":\"navigate\",\"uri\":{}}}",
        json_escape(&uri)
    ));
    S_OK
}
unsafe extern "win64" fn web_navigate_to_string(this: usize, html: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send(&format!(
        "{{\"op\":\"html\",\"html\":{}}}",
        json_escape(&wstr(html))
    ));
    S_OK
}
unsafe extern "win64" fn web_remove(this: usize, token: i64) -> i32 {
    match view_of(this) {
        Some(v) => v.remove_handler(token),
        None => S_OK,
    }
}
macro_rules! adder {
    ($name:ident, $event:literal) => {
        unsafe extern "win64" fn $name(this: usize, handler: usize, token: *mut i64) -> i32 {
            match view_of(this) {
                Some(v) => v.add_handler($event, handler, token),
                None => E_FAIL,
            }
        }
    };
}
adder!(add_navigation_starting, "NavigationStarting");
adder!(add_content_loading, "ContentLoading");
adder!(add_source_changed, "SourceChanged");
adder!(add_history_changed, "HistoryChanged");
adder!(add_navigation_completed, "NavigationCompleted");
adder!(add_web_message, "WebMessageReceived");
adder!(add_title_changed, "DocumentTitleChanged");

unsafe extern "win64" fn web_add_script(this: usize, js: *const u16, handler: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send(&format!(
        "{{\"op\":\"script_doc\",\"js\":{}}}",
        json_escape(&wstr(js))
    ));
    if handler != 0 {
        add_ref(handler);
        let id = format!("winlux-script-{}", TOKENS.fetch_add(1, Ordering::Relaxed));
        post(
            v.env.dispatcher,
            Box::new(move || unsafe {
                let w: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                invoke(handler, S_OK as usize, w.as_ptr() as usize);
                release(handler);
            }),
        );
    }
    S_OK
}
unsafe extern "win64" fn web_execute_script(this: usize, js: *const u16, handler: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let id = v.next_exec.fetch_add(1, Ordering::Relaxed);
    if handler != 0 {
        add_ref(handler);
        super::lock(&v.pending_exec).insert(id, handler);
    }
    v.send(&format!(
        "{{\"op\":\"exec\",\"id\":{id},\"js\":{}}}",
        json_escape(&wstr(js))
    ));
    S_OK
}
unsafe extern "win64" fn web_reload(this: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send("{\"op\":\"reload\"}");
    S_OK
}
unsafe extern "win64" fn web_stop(this: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send("{\"op\":\"stop\"}");
    S_OK
}
unsafe extern "win64" fn web_go_back(this: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send("{\"op\":\"back\"}");
    S_OK
}
unsafe extern "win64" fn web_go_forward(this: usize) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    v.send("{\"op\":\"forward\"}");
    S_OK
}
unsafe extern "win64" fn web_post_json(this: usize, json: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let text = wstr(json);
    if text.trim().is_empty() {
        return E_INVALIDARG;
    }
    v.send(&format!(
        "{{\"op\":\"post\",\"json\":{}}}",
        json_escape(&text)
    ));
    S_OK
}
unsafe extern "win64" fn web_post_string(this: usize, s: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let as_json = json_escape(&wstr(s));
    v.send(&format!(
        "{{\"op\":\"post\",\"json\":{}}}",
        json_escape(&as_json)
    ));
    S_OK
}
unsafe extern "win64" fn web_process_id(this: usize, out: *mut u32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = v.pid.load(Ordering::SeqCst);
    S_OK
}
unsafe extern "win64" fn web_can_go_back(this: usize, out: *mut i32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(super::lock(&v.state).can_back);
    S_OK
}
unsafe extern "win64" fn web_can_go_forward(this: usize, out: *mut i32) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(super::lock(&v.state).can_forward);
    S_OK
}
/// Objet hôte retenu par une vue (#2596) : le VARIANT de l'application,
/// copié avec sa propre référence (interface AddRef, BSTR et tableau
/// dupliqués), libéré au remplacement, au retrait ou avec la vue.
struct HostObject {
    name: String,
    vt: u16,
    data: usize,
}

const VT_DECIMAL: u16 = 14;
const VT_DISPATCH: u16 = 9;
const VT_UNKNOWN: u16 = 13;
const VT_BSTR: u16 = 8;
const VT_RECORD: u16 = 36;
const VT_ARRAY: u16 = 0x2000;
const VT_BYREF: u16 = 0x4000;
/// {00020400-0000-0000-C000-000000000046}
const IID_IDISPATCH_BYTES: [u8; 16] = [
    0x00, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

impl Drop for HostObject {
    fn drop(&mut self) {
        unsafe {
            if self.vt & VT_ARRAY != 0 {
                super::oleaut32::safe_array_destroy(self.data);
            } else if self.vt == VT_DISPATCH {
                release(self.data);
            } else if self.vt == VT_BSTR {
                super::oleaut32::sys_free_string(self.data as *mut u16);
            }
        }
    }
}

/// Copie le VARIANT `v` avec sa propre référence. Types refusés par la
/// documentation publique (IUnknown générique, VT_DECIMAL, VT_RECORD,
/// tableaux par référence) : None.
unsafe fn host_object(name: String, v: *const u8) -> Option<HostObject> {
    let vt = *(v as *const u16);
    let data = *(v.add(8) as *const usize);
    if vt & VT_BYREF != 0 || vt == VT_DECIMAL || vt & !VT_ARRAY == VT_RECORD {
        return None;
    }
    if vt & VT_ARRAY != 0 {
        let mut copy = 0usize;
        if data == 0 || super::oleaut32::safe_array_copy(data, &mut copy) != S_OK {
            return None;
        }
        return Some(HostObject {
            name,
            vt,
            data: copy,
        });
    }
    match vt {
        VT_DISPATCH => {
            add_ref(data);
            Some(HostObject { name, vt, data })
        }
        // Un IUnknown qui implémente IDispatch est traité comme IDispatch.
        VT_UNKNOWN => {
            if data == 0 {
                return None;
            }
            let qi: extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
                std::mem::transmute(**(data as *const *const usize));
            let mut disp = 0usize;
            (qi(data, IID_IDISPATCH_BYTES.as_ptr(), &mut disp) == S_OK && disp != 0).then_some(
                HostObject {
                    name,
                    vt: VT_DISPATCH,
                    data: disp,
                },
            )
        }
        VT_BSTR => {
            let copy = if data == 0 {
                0
            } else {
                super::oleaut32::sys_alloc_string(data as *const u16) as usize
            };
            Some(HostObject {
                name,
                vt,
                data: copy,
            })
        }
        _ => Some(HostObject { name, vt, data }),
    }
}

/// ICoreWebView2::AddHostObjectToScript (#2596). Un même nom remplace
/// l'objet précédent. Le pont vers `chrome.webview.hostObjects` (appels
/// IDispatch depuis le script) n'est pas encore là.
unsafe extern "win64" fn web_add_host_object(
    this: usize,
    name: *const u16,
    object: *const u8,
) -> i32 {
    if name.is_null() || object.is_null() {
        return E_INVALIDARG;
    }
    let name = wstr(name);
    if name.is_empty() {
        return E_INVALIDARG;
    }
    let Some(obj) = host_object(name, object) else {
        return E_INVALIDARG;
    };
    let mut hosts = super::lock(&com(this).hosts);
    // L'ancien objet est libéré hors du verrou.
    let old = hosts
        .iter()
        .position(|h| h.name == obj.name)
        .map(|i| hosts.remove(i));
    hosts.push(obj);
    drop(hosts);
    drop(old);
    S_OK
}

/// ICoreWebView2::RemoveHostObjectFromScript (#2596). Un nom inconnu ne
/// change rien (la documentation publique ne dit pas ce qu'il rend).
unsafe extern "win64" fn web_remove_host_object(this: usize, name: *const u16) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    let name = wstr(name);
    let mut hosts = super::lock(&com(this).hosts);
    let old = hosts
        .iter()
        .position(|h| h.name == name)
        .map(|i| hosts.remove(i));
    drop(hosts);
    drop(old);
    S_OK
}

unsafe extern "win64" fn web_title(this: usize, out: *mut *mut u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let t = super::lock(&v.state).title.clone();
    co_string(&t, out)
}
unsafe extern "win64" fn web_environment(this: usize, out: *mut usize) -> i32 {
    let c = com(this);
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_com(Family::Environment, c.env.clone(), None, Args::default());
    S_OK
}
unsafe extern "win64" fn web_map_host(
    this: usize,
    host: *const u16,
    folder: *const u16,
    _kind: i32,
) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let host = wstr(host).to_ascii_lowercase();
    let folder = crate::paths::to_linux(&wstr(folder))
        .to_string_lossy()
        .into_owned();
    if host.is_empty() {
        return E_INVALIDARG;
    }
    super::lock(&v.state)
        .hosts
        .push((host.clone(), folder.clone()));
    v.send(&format!(
        "{{\"op\":\"map\",\"host\":{},\"folder\":{}}}",
        json_escape(&host),
        json_escape(&folder)
    ));
    S_OK
}
unsafe extern "win64" fn web_unmap_host(this: usize, host: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let host = wstr(host).to_ascii_lowercase();
    super::lock(&v.state).hosts.retain(|(h, _)| *h != host);
    v.send(&format!(
        "{{\"op\":\"unmap\",\"host\":{}}}",
        json_escape(&host)
    ));
    S_OK
}

// ============================================================
//  ICoreWebView2Settings (…9)
// ============================================================

fn settings_methods() -> Vec<(usize, usize)> {
    let mut m = vec![
        (0, set_get_script as *const () as usize),
        (1, set_put_script as *const () as usize),
        (2, set_get_messages as *const () as usize),
        (3, set_put_messages as *const () as usize),
        (8, set_get_devtools as *const () as usize),
        (9, set_put_devtools as *const () as usize),
        (10, set_get_menus as *const () as usize),
        (11, set_put_menus as *const () as usize),
        // ICoreWebView2Settings2 : agent utilisateur.
        (18, set_get_user_agent as *const () as usize),
        (19, set_put_user_agent as *const () as usize),
    ];
    // Les autres réglages booléens : lus vrais, écrits sans effet.
    for (get, put) in [(4, 5), (6, 7), (12, 13), (14, 15), (16, 17)] {
        m.push((get, get_true as *const () as usize));
        m.push((put, ok1 as *const () as usize));
    }
    m
}
unsafe extern "win64" fn get_true(_this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 1;
    S_OK
}
macro_rules! bool_setting {
    ($get:ident, $put:ident, $field:ident, $key:literal) => {
        unsafe extern "win64" fn $get(this: usize, out: *mut i32) -> i32 {
            let Some(v) = view_of(this) else {
                return E_FAIL;
            };
            if out.is_null() {
                return E_POINTER;
            }
            *out = i32::from(super::lock(&v.state).$field);
            S_OK
        }
        unsafe extern "win64" fn $put(this: usize, on: i32) -> i32 {
            let Some(v) = view_of(this) else {
                return E_FAIL;
            };
            super::lock(&v.state).$field = on != 0;
            if !$key.is_empty() {
                v.send(&format!("{{\"op\":\"settings\",\"{}\":{}}}", $key, on != 0));
            }
            S_OK
        }
    };
}
bool_setting!(set_get_script, set_put_script, script_enabled, "script");
bool_setting!(set_get_messages, set_put_messages, messages_enabled, "");
bool_setting!(set_get_devtools, set_put_devtools, devtools, "devtools");
bool_setting!(set_get_menus, set_put_menus, context_menus, "menus");

unsafe extern "win64" fn set_get_user_agent(this: usize, out: *mut *mut u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let ua = super::lock(&v.state).user_agent.clone();
    co_string(&ua, out)
}
unsafe extern "win64" fn set_put_user_agent(this: usize, ua: *const u16) -> i32 {
    let Some(v) = view_of(this) else {
        return E_FAIL;
    };
    let ua = wstr(ua);
    super::lock(&v.state).user_agent = ua.clone();
    v.send(&format!(
        "{{\"op\":\"settings\",\"user_agent\":{}}}",
        json_escape(&ua)
    ));
    S_OK
}

// ============================================================
//  Arguments d'événements
// ============================================================

unsafe fn args<'a>(this: usize) -> &'a Args {
    &com(this).args
}
unsafe extern "win64" fn args_uri(this: usize, out: *mut *mut u16) -> i32 {
    co_string(&args(this).uri, out)
}
unsafe extern "win64" fn args_nav_id(this: usize, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = args(this).nav;
    S_OK
}
unsafe extern "win64" fn args_ok(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(args(this).ok);
    S_OK
}
unsafe extern "win64" fn args_status(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = args(this).status;
    S_OK
}
unsafe extern "win64" fn args_json(this: usize, out: *mut *mut u16) -> i32 {
    co_string(&args(this).json, out)
}
unsafe extern "win64" fn args_try_string(this: usize, out: *mut *mut u16) -> i32 {
    match args(this).string.clone() {
        Some(s) => co_string(&s, out),
        None => {
            if !out.is_null() {
                *out = std::ptr::null_mut();
            }
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn args_null_out(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}

fn nav_starting_methods() -> Vec<(usize, usize)> {
    vec![
        (0, args_uri as *const () as usize),
        (1, get_false as *const () as usize),
        (2, get_false as *const () as usize),
        (3, args_null_out as *const () as usize),
        (4, get_false as *const () as usize),
        (5, ok1 as *const () as usize),
        (6, args_nav_id as *const () as usize),
    ]
}
fn nav_completed_methods() -> Vec<(usize, usize)> {
    vec![
        (0, args_ok as *const () as usize),
        (1, args_status as *const () as usize),
        (2, args_nav_id as *const () as usize),
    ]
}
fn message_methods() -> Vec<(usize, usize)> {
    vec![
        (0, args_uri as *const () as usize),
        (1, args_json as *const () as usize),
        (2, args_try_string as *const () as usize),
    ]
}
fn content_loading_methods() -> Vec<(usize, usize)> {
    vec![
        (0, get_false as *const () as usize),
        (1, args_nav_id as *const () as usize),
    ]
}
fn source_changed_methods() -> Vec<(usize, usize)> {
    vec![(0, args_ok as *const () as usize)]
}

// ============================================================
//  WebView2Loader.dll
// ============================================================

pub fn exports() -> Exports {
    crate::exports! {
        "CreateCoreWebView2EnvironmentWithOptions" => fn create_environment_with_options,
        "CreateCoreWebView2Environment" => fn create_environment,
        "GetAvailableCoreWebView2BrowserVersionString" => fn get_available_version,
        "GetAvailableCoreWebView2BrowserVersionStringWithOptions" => fn get_available_version_with_options,
        "CompareBrowserVersions" => fn compare_browser_versions,
    }
}

#[allow(dead_code)]
fn _unused(_: *mut c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_plat() {
        let m =
            parse_flat(r#"{"ev":"message","json":"{\"a\":1}","string":null,"nav":3,"ok":true}"#)
                .unwrap();
        assert_eq!(get_str(&m, "ev").as_deref(), Some("message"));
        assert_eq!(get_str(&m, "json").as_deref(), Some("{\"a\":1}"));
        assert_eq!(m.get("string"), Some(&Val::Null));
        assert_eq!(get_num(&m, "nav"), 3.0);
        assert!(get_bool(&m, "ok"));
        let m = parse_flat(r#"{"title":"été 😀"}"#).unwrap();
        assert_eq!(get_str(&m, "title").as_deref(), Some("été 😀"));
    }

    #[test]
    fn echappement_json() {
        let s = "a\"b\\c\nd\u{1}";
        let e = json_escape(s);
        let m = parse_flat(&format!("{{\"v\":{e}}}")).unwrap();
        assert_eq!(get_str(&m, "v").as_deref(), Some(s));
    }

    #[test]
    fn versions_comparees() {
        let mut r = 9;
        let a: Vec<u16> = "86.0.616.0\0".encode_utf16().collect();
        let b: Vec<u16> = "130.0.2849.56\0".encode_utf16().collect();
        unsafe { compare_browser_versions(a.as_ptr(), b.as_ptr(), &mut r) };
        assert_eq!(r, -1);
        unsafe { compare_browser_versions(b.as_ptr(), b.as_ptr(), &mut r) };
        assert_eq!(r, 0);
    }

    #[test]
    fn tables_aux_bonnes_tailles() {
        assert_eq!(vtable(Family::WebView).len(), WEBVIEW_SLOTS);
        assert_eq!(vtable(Family::Controller).len(), CONTROLLER_SLOTS);
        // put_Bounds est la 7e méthode du contrôleur (3 + 3).
        assert_eq!(
            vtable(Family::Controller)[6],
            ctrl_put_bounds as *const () as usize
        );
        // ExecuteScript : 3 + 26.
        assert_eq!(
            vtable(Family::WebView)[29],
            web_execute_script as *const () as usize
        );
        // SetVirtualHostNameToFolderMapping : ICoreWebView2_3, 4e méthode.
        assert_eq!(
            vtable(Family::WebView)[3 + 58 + 7 + 3],
            web_map_host as *const () as usize
        );
    }

    /// #2547 : aucune inscription à un événement ne rend E_NOTIMPL, et
    /// l'inscription rend un jeton que la désinscription accepte.
    #[test]
    fn inscriptions_aux_evenements_acceptees_2547() {
        let stub = not_implemented as *const () as usize;
        for (family, slots) in [
            (Family::WebView, WEBVIEW_SLOTS),
            (Family::Controller, CONTROLLER_SLOTS),
            (Family::Environment, ENVIRONMENT_SLOTS),
        ] {
            let v = vtable(family);
            for &add in event_slots(family) {
                assert!(3 + add + 1 < slots);
                assert_ne!(v[3 + add], stub, "add_ au rang {add}");
                assert_ne!(v[3 + add + 1], stub, "remove_ au rang {}", add + 1);
            }
        }
        // ICoreWebView2Environment5 : add_BrowserProcessExited (rang 9).
        let add: unsafe extern "win64" fn(usize, usize, *mut i64) -> i32 =
            unsafe { std::mem::transmute(vtable(Family::Environment)[3 + 9]) };
        let remove: unsafe extern "win64" fn(usize, i64) -> i32 =
            unsafe { std::mem::transmute(vtable(Family::Environment)[3 + 10]) };
        let mut token = 0i64;
        assert_eq!(unsafe { add(0, 0, &mut token) }, S_OK);
        assert_ne!(token, 0);
        assert_eq!(unsafe { remove(0, token) }, S_OK);
        // ICoreWebView2Controller3 : add_RasterizationScaleChanged (rang 29).
        assert_eq!(
            vtable(Family::Controller)[3 + 29],
            add_ignored as *const () as usize
        );
    }

    /// #2570 : ICoreWebView2Environment10 rend un objet de réglages de
    /// contrôleur dont les propriétés se relisent.
    #[test]
    fn reglages_de_controleur_2570() {
        let env = vtable(Family::Environment);
        assert_eq!(
            env[3 + 17],
            env_create_controller_options as *const () as usize
        );
        assert_eq!(
            env[3 + 18],
            env_create_controller_with_options as *const () as usize
        );
        let envobj = new_com(Family::Environment, None, None, Args::default());
        let mut opts = 0usize;
        assert_eq!(
            unsafe { env_create_controller_options(envobj, &mut opts) },
            S_OK
        );
        assert_ne!(opts, 0);
        let vt = vtable(Family::ControllerOptions);
        assert_eq!(vt.len(), CONTROLLER_OPTIONS_SLOTS);
        let mut out = 0usize;
        let iid = CONTROLLER_OPTIONS_IIDS[2].as_ptr();
        assert_eq!(query_interface(opts, iid, &mut out), S_OK);
        unsafe {
            assert_eq!(opt_put_private(opts, 1), S_OK);
            let mut on = 0i32;
            assert_eq!(opt_get_private(opts, &mut on), S_OK);
            assert_eq!(on, 1);
            let mut color = 0u32;
            assert_eq!(opt_get_background(opts, &mut color), S_OK);
            assert_eq!(color, 0xFFFF_FFFF);
            assert_eq!(opt_put_background(opts, 0x00FF_0000), S_OK);
            assert_eq!(opt_get_background(opts, &mut color), S_OK);
            assert_eq!(color, 0x00FF_0000);
            com_release(out);
            com_release(opts);
            com_release(envobj);
        }
    }

    /// #2581 : GetProcessInfos et GetProcessExtendedInfos rendent une
    /// collection vide au lieu d'E_NOTIMPL.
    #[test]
    fn processus_du_moteur_2581() {
        let env = vtable(Family::Environment);
        assert_eq!(env[3 + 15], env_get_process_infos as *const () as usize);
        assert_eq!(
            env[3 + 22],
            env_get_process_extended_infos as *const () as usize
        );
        let mut list = 0usize;
        assert_eq!(unsafe { env_get_process_infos(0, &mut list) }, S_OK);
        let mut out = 0usize;
        for iid in PROCESS_COLLECTION_IIDS.iter() {
            assert_eq!(query_interface(list, iid.as_ptr(), &mut out), S_OK);
            com_release(out);
        }
        let mut n = 7u32;
        assert_eq!(unsafe { collection_count(list, &mut n) }, S_OK);
        assert_eq!(n, 0);
        assert_eq!(
            unsafe { collection_value_at(list, 0, &mut out) },
            E_INVALIDARG
        );
        assert_eq!(out, 0);
        com_release(list);
        // Sans gestionnaire : E_INVALIDARG, rien n'est posté.
        assert_eq!(
            unsafe { env_get_process_extended_infos(0, 0) },
            E_INVALIDARG
        );
    }

    /// #2596 : AddHostObjectToScript garde sa référence, remplace un même
    /// nom, refuse un IUnknown sans IDispatch ; RemoveHostObjectFromScript
    /// et la fin de la vue la rendent.
    #[test]
    fn objets_hotes_2596() {
        static REFS: AtomicU32 = AtomicU32::new(1);
        static DISPATCH: AtomicBool = AtomicBool::new(true);
        extern "win64" fn qi(this: usize, _iid: *const u8, out: *mut usize) -> i32 {
            if !DISPATCH.load(Ordering::SeqCst) {
                unsafe { *out = 0 };
                return E_NOINTERFACE;
            }
            REFS.fetch_add(1, Ordering::SeqCst);
            unsafe { *out = this };
            S_OK
        }
        extern "win64" fn add(_this: usize) -> u32 {
            REFS.fetch_add(1, Ordering::SeqCst) + 1
        }
        extern "win64" fn rel(_this: usize) -> u32 {
            REFS.fetch_sub(1, Ordering::SeqCst) - 1
        }
        let vt: &'static [usize; 3] = Box::leak(Box::new([
            qi as *const () as usize,
            add as *const () as usize,
            rel as *const () as usize,
        ]));
        let obj: &'static usize = Box::leak(Box::new(vt.as_ptr() as usize));
        let obj = obj as *const usize as usize;
        let web = vtable(Family::WebView);
        assert_eq!(web[3 + 46], web_add_host_object as *const () as usize);
        assert_eq!(web[3 + 47], web_remove_host_object as *const () as usize);
        let view = new_com(Family::WebView, None, None, Args::default());
        let name: Vec<u16> = "teams\0".encode_utf16().collect();
        let mut v = [0u8; 24];
        let variant = |v: &mut [u8; 24], vt: u16| {
            v[..2].copy_from_slice(&vt.to_le_bytes());
            v[8..16].copy_from_slice(&obj.to_le_bytes());
        };
        variant(&mut v, VT_DISPATCH);
        unsafe {
            assert_eq!(web_add_host_object(view, name.as_ptr(), v.as_ptr()), S_OK);
            assert_eq!(REFS.load(Ordering::SeqCst), 2);
            // Même nom : l'objet précédent est rendu.
            assert_eq!(web_add_host_object(view, name.as_ptr(), v.as_ptr()), S_OK);
            assert_eq!(REFS.load(Ordering::SeqCst), 2);
            assert_eq!(web_remove_host_object(view, name.as_ptr()), S_OK);
            assert_eq!(REFS.load(Ordering::SeqCst), 1);
            // IUnknown qui implémente IDispatch : accepté ; sinon refusé.
            variant(&mut v, VT_UNKNOWN);
            assert_eq!(web_add_host_object(view, name.as_ptr(), v.as_ptr()), S_OK);
            assert_eq!(REFS.load(Ordering::SeqCst), 2);
            DISPATCH.store(false, Ordering::SeqCst);
            let other: Vec<u16> = "autre\0".encode_utf16().collect();
            assert_eq!(
                web_add_host_object(view, other.as_ptr(), v.as_ptr()),
                E_INVALIDARG
            );
            variant(&mut v, VT_DECIMAL);
            assert_eq!(
                web_add_host_object(view, other.as_ptr(), v.as_ptr()),
                E_INVALIDARG
            );
            // Un nom inconnu ne change rien.
            assert_eq!(web_remove_host_object(view, other.as_ptr()), S_OK);
            assert_eq!(REFS.load(Ordering::SeqCst), 2);
        }
        // La vue libérée rend l'objet encore retenu.
        com_release(view);
        assert_eq!(REFS.load(Ordering::SeqCst), 1);
    }
}
