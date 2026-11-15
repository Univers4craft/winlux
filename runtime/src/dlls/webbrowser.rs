//! Contrôle navigateur web : CLSID_WebBrowser (ProgID « Shell.Explorer »,
//! « Shell.Explorer.2 »), #2228.
//!
//! Des installeurs et des fenêtres de licence (WinRAR, Eula.exe d'Acrobat
//! Reader) placent ce contrôle ActiveX dans une boîte de dialogue pour
//! afficher un contrat en HTML. Sans lui, CoCreateInstance rendait
//! REGDB_E_CLASSNOTREG : Eula.exe ferme sa boîte (-1), et WinRAR se replie
//! sur un affichage texte qui perd les entités et des mots.
//!
//! Première étape, de mesure : l'objet se laisse créer et activer sur
//! place (fenêtre fille, rectangles, site client), accepte l'inscription
//! aux événements DWebBrowserEvents2 et note chaque appel. Ce qu'il ne sait
//! pas encore faire rend E_NOTIMPL et le journalise avec son fichier et sa
//! ligne, et chaque interface refusée est notée : une passe de banc dit
//! ainsi quels chemins Eula.exe et l'installeur WinRAR empruntent avant
//! qu'on écrive le rendu.
//!
//! Méthodes, ordre des tables et identifiants : en-têtes MinGW-w64
//! (exdisp.h, oleidl.h, ocidl.h) et documentation Microsoft Learn
//! d'IWebBrowser2, IOleObject, IOleInPlaceObject et IConnectionPoint.

mod texte;

use super::comobj::{guid, guid_str, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const CONNECT_E_NOCONNECTION: i32 = 0x8004_0200u32 as i32;
const OLEOBJ_E_NOVERBS: i32 = 0x8004_0180u32 as i32;

const fn ole_guid(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

/// CLSID_WebBrowser {8856F961-340A-11D0-A96B-00C04FD705A2} (exdisp.h).
pub const CLSID_WEB_BROWSER: Guid = guid(
    0x8856_F961,
    0x340A,
    0x11D0,
    [0xA9, 0x6B, 0x00, 0xC0, 0x4F, 0xD7, 0x05, 0xA2],
);
/// CLSID_WebBrowser_V1 {EAB22AC3-30C1-11CF-A7EB-0000C05BAE0B} (exdisp.h).
pub const CLSID_WEB_BROWSER_V1: Guid = guid(
    0xEAB2_2AC3,
    0x30C1,
    0x11CF,
    [0xA7, 0xEB, 0x00, 0x00, 0xC0, 0x5B, 0xAE, 0x0B],
);
const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_IDISPATCH: Guid = ole_guid(0x0002_0400);
const IID_IWEB_BROWSER: Guid = guid(
    0xEAB2_2AC1,
    0x30C1,
    0x11CF,
    [0xA7, 0xEB, 0x00, 0x00, 0xC0, 0x5B, 0xAE, 0x0B],
);
const IID_IWEB_BROWSER_APP: Guid = ole_guid(0x0002_DF05);
const IID_IWEB_BROWSER2: Guid = guid(
    0xD30C_1661,
    0xCDAF,
    0x11D0,
    [0x8A, 0x3E, 0x00, 0xC0, 0x4F, 0xC9, 0xE2, 0x6E],
);
const DIID_DWEB_BROWSER_EVENTS2: Guid = guid(
    0x34A7_15A0,
    0x6587,
    0x11D0,
    [0x92, 0x4A, 0x00, 0x20, 0xAF, 0xC7, 0xAC, 0x4D],
);
const IID_IOLE_OBJECT: Guid = ole_guid(0x112);
const IID_IOLE_IN_PLACE_OBJECT: Guid = ole_guid(0x113);
const IID_IOLE_WINDOW: Guid = ole_guid(0x114);
const IID_IOLE_IN_PLACE_ACTIVE_OBJECT: Guid = ole_guid(0x117);
const IID_IOLE_IN_PLACE_SITE: Guid = ole_guid(0x119);
const IID_IVIEW_OBJECT: Guid = ole_guid(0x10D);
const IID_IVIEW_OBJECT2: Guid = ole_guid(0x127);
const IID_IPERSIST: Guid = ole_guid(0x10C);
const IID_IPERSIST_STREAM_INIT: Guid = guid(
    0x7FD5_2380,
    0x4E07,
    0x101B,
    [0xAE, 0x2D, 0x08, 0x00, 0x2B, 0x2E, 0xC7, 0x13],
);
const fn ocx_guid(d1: u32) -> Guid {
    guid(
        d1,
        0xBAB4,
        0x101A,
        [0xB6, 0x9C, 0x00, 0xAA, 0x00, 0x34, 0x1D, 0x07],
    )
}
const IID_ICONNECTION_POINT_CONTAINER: Guid = ocx_guid(0xB196_B284);
const IID_ICONNECTION_POINT: Guid = ocx_guid(0xB196_B286);
const IID_IOLE_CONTROL: Guid = ocx_guid(0xB196_B288);

/// ProgID du contrôle (HKCR\Shell.Explorer[.2]\CLSID sous Windows).
const PROG_IDS: [(&str, Guid); 2] = [
    ("Shell.Explorer", CLSID_WEB_BROWSER),
    ("Shell.Explorer.2", CLSID_WEB_BROWSER),
];

/// CLSID d'un ProgID du contrôle, sans tenir compte de la casse.
pub fn clsid_for_prog_id(name: &str) -> Option<super::ole32::Guid> {
    PROG_IDS
        .iter()
        .find(|(p, _)| p.eq_ignore_ascii_case(name))
        // SAFETY: même disposition mémoire (16 octets) que le Guid d'ole32.
        .map(|(_, g)| unsafe { std::mem::transmute::<Guid, super::ole32::Guid>(*g) })
}

// OLEMISC_* (oleidl.h) : contrôle fenêtré, site client posé avant
// l'initialisation, activé dès qu'il est visible.
const OLEMISC_RECOMPOSEONRESIZE: u32 = 0x1;
const OLEMISC_CANTLINKINSIDE: u32 = 0x10;
const OLEMISC_INSIDEOUT: u32 = 0x80;
const OLEMISC_ACTIVATEWHENVISIBLE: u32 = 0x100;
const OLEMISC_SETCLIENTSITEFIRST: u32 = 0x2_0000;
const MISC_STATUS: u32 = OLEMISC_RECOMPOSEONRESIZE
    | OLEMISC_CANTLINKINSIDE
    | OLEMISC_INSIDEOUT
    | OLEMISC_ACTIVATEWHENVISIBLE
    | OLEMISC_SETCLIENTSITEFIRST;

// Verbes OLEIVERB_* (oleidl.h).
const OLEIVERB_PRIMARY: i32 = 0;
const OLEIVERB_SHOW: i32 = -1;
const OLEIVERB_UIACTIVATE: i32 = -4;
const OLEIVERB_INPLACEACTIVATE: i32 = -5;
const OLEIVERB_HIDE: i32 = -3;

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Default)]
struct State {
    /// IOleClientSite retenu (AddRef).
    client_site: usize,
    /// Fenêtre fille créée à l'activation sur place.
    hwnd: usize,
    parent: usize,
    pos: Rect,
    /// Étendue HIMETRIC posée par SetExtent.
    extent: (i32, i32),
    /// Abonnés DWebBrowserEvents2 : (jeton, IDispatch retenu).
    sinks: Vec<(u32, usize)>,
    next_cookie: u32,
    /// Dernière adresse demandée par Navigate/Navigate2.
    url: String,
    /// Zone de texte en lecture seule qui montre la page.
    edit: usize,
    /// Texte de la page chargée, posé dans `edit` dès qu'elle existe.
    text: Option<String>,
    /// Document rendu par get_Document (non retenu : c'est lui qui retient
    /// le contrôle), 0 s'il n'existe pas.
    document: usize,
}

/// L'objet : une table par interface, dans l'ordre de `SLOT_*`.
#[repr(C)]
struct Browser {
    vtbls: [*const (); SLOTS],
    refs: AtomicU32,
    state: Mutex<State>,
}

const SLOT_BROWSER: usize = 0;
const SLOT_OLE_OBJECT: usize = 1;
const SLOT_IN_PLACE: usize = 2;
const SLOT_ACTIVE: usize = 3;
const SLOT_VIEW: usize = 4;
const SLOT_PERSIST: usize = 5;
const SLOT_CPC: usize = 6;
const SLOT_CONTROL: usize = 7;
const SLOT_CP: usize = 8;
const SLOTS: usize = 9;

/// Objet à partir d'un pointeur d'interface pris dans la table `slot`.
unsafe fn from(this: usize, slot: usize) -> &'static Browser {
    &*((this - slot * std::mem::size_of::<usize>()) as *const Browser)
}

fn iface(b: &Browser, slot: usize) -> *mut c_void {
    (&b.vtbls[slot]) as *const *const () as *mut c_void
}

fn state(b: &Browser) -> std::sync::MutexGuard<'_, State> {
    b.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Appel non encore servi : journalisé avec son nom, son fichier, sa ligne
/// et ses appelants (#2547).
fn not_impl(method: &str, line: u32) -> i32 {
    crate::log_warn!(
        "webbrowser",
        "{method} : E_NOTIMPL ({}:{line}), appelants {}",
        file!(),
        callers()
    );
    E_NOTIMPL
}

/// Appel servi : noté au niveau info (WINLUX_LOG=info).
fn note(what: std::fmt::Arguments<'_>) {
    crate::log_info!("webbrowser", "{what}");
}

fn callers() -> String {
    let calls = super::seh::caller_stack(3);
    if calls.is_empty() {
        "aucun".into()
    } else {
        calls.join(" < ")
    }
}

unsafe fn method(obj: usize, index: usize) -> usize {
    *(*(obj as *const *const usize)).add(index)
}

unsafe fn com_add_ref(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(method(obj, 1));
    f(obj);
}

unsafe fn com_release(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(method(obj, 2));
    f(obj);
}

unsafe fn com_qi(obj: usize, iid: &Guid) -> usize {
    let f: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(method(obj, 0));
    let mut out = 0usize;
    if f(obj, iid, &mut out) < 0 {
        0
    } else {
        out
    }
}

// ── IUnknown, commun à toutes les tables ────────────────────────────

unsafe fn query(b: &Browser, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let g = std::ptr::read_unaligned(iid);
    let slot = match g {
        x if x == IID_IUNKNOWN
            || x == IID_IDISPATCH
            || x == IID_IWEB_BROWSER
            || x == IID_IWEB_BROWSER_APP
            || x == IID_IWEB_BROWSER2 =>
        {
            SLOT_BROWSER
        }
        x if x == IID_IOLE_OBJECT => SLOT_OLE_OBJECT,
        x if x == IID_IOLE_IN_PLACE_OBJECT || x == IID_IOLE_WINDOW => SLOT_IN_PLACE,
        x if x == IID_IOLE_IN_PLACE_ACTIVE_OBJECT => SLOT_ACTIVE,
        x if x == IID_IVIEW_OBJECT || x == IID_IVIEW_OBJECT2 => SLOT_VIEW,
        x if x == IID_IPERSIST || x == IID_IPERSIST_STREAM_INIT => SLOT_PERSIST,
        x if x == IID_ICONNECTION_POINT_CONTAINER => SLOT_CPC,
        x if x == IID_IOLE_CONTROL => SLOT_CONTROL,
        _ => {
            super::comobj::log_no_interface("WebBrowser", iid);
            return E_NOINTERFACE;
        }
    };
    b.refs.fetch_add(1, Ordering::SeqCst);
    *out = iface(b, slot);
    S_OK
}

unsafe fn release_obj(b: &Browser) -> u32 {
    let n = b.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let (site, hwnd, sinks) = {
            let mut s = state(b);
            (
                std::mem::take(&mut s.client_site),
                std::mem::take(&mut s.hwnd),
                std::mem::take(&mut s.sinks),
            )
        };
        if hwnd != 0 {
            super::user32::destroy_window(hwnd);
        }
        for (_, sink) in sinks {
            com_release(sink);
        }
        if site != 0 {
            com_release(site);
        }
        drop(Box::from_raw(b as *const Browser as *mut Browser));
    }
    n
}

macro_rules! unknown_for {
    ($slot:expr, $qi:ident, $add:ident, $rel:ident) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
            query(from(this, $slot), iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            from(this, $slot).refs.fetch_add(1, Ordering::SeqCst) + 1
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            release_obj(from(this, $slot))
        }
    };
}
unknown_for!(SLOT_BROWSER, wb_qi, wb_add_ref, wb_release);
unknown_for!(SLOT_OLE_OBJECT, oo_qi, oo_add_ref, oo_release);
unknown_for!(SLOT_IN_PLACE, ip_qi, ip_add_ref, ip_release);
unknown_for!(SLOT_ACTIVE, ao_qi, ao_add_ref, ao_release);
unknown_for!(SLOT_VIEW, vo_qi, vo_add_ref, vo_release);
unknown_for!(SLOT_PERSIST, ps_qi, ps_add_ref, ps_release);
unknown_for!(SLOT_CPC, cpc_qi, cpc_add_ref, cpc_release);
unknown_for!(SLOT_CONTROL, oc_qi, oc_add_ref, oc_release);

/// Méthodes pas encore servies : une fonction par méthode, pour que le
/// journal la nomme. En x64, l'appelant nettoie la pile : une seule
/// signature convient à toutes.
macro_rules! not_impl_fns {
    ($($name:ident),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            extern "win64" fn $name(_this: usize) -> i32 {
                not_impl(stringify!($name), line!())
            }
        )*
    };
}

/// Propriétés d'apparence acceptées sans effet (barres, mode silencieux…) :
/// le contrôle incorporé n'a ni barre ni boîte de script à cacher.
macro_rules! accepted_fns {
    ($($name:ident),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            extern "win64" fn $name(_this: usize, value: i64) -> i32 {
                note(format_args!("{} ({value:#x}) accepté", stringify!($name)));
                S_OK
            }
        )*
    };
}

// ── IWebBrowser2 (IDispatch, IWebBrowser, IWebBrowserApp) ─────────────

not_impl_fns!(
    GetTypeInfo,
    GetIDsOfNames,
    Invoke,
    GoBack,
    GoForward,
    GoHome,
    GoSearch,
    Refresh,
    Refresh2,
    Stop,
    get_Application,
    get_Parent,
    get_Container,
    get_TopLevelContainer,
    get_Type,
    get_Left,
    put_Left,
    get_Top,
    put_Top,
    get_Width,
    put_Width,
    get_Height,
    put_Height,
    get_LocationName,
    get_LocationURL,
    get_Busy,
    Quit,
    ClientToWindow,
    PutProperty,
    GetProperty,
    get_Name,
    get_FullName,
    get_Path,
    get_Visible,
    get_StatusBar,
    get_StatusText,
    put_StatusText,
    get_ToolBar,
    get_MenuBar,
    get_FullScreen,
    put_FullScreen,
    QueryStatusWB,
    ExecWB,
    ShowBrowserBar,
    get_ReadyState,
    get_Offline,
    get_Silent,
    get_RegisterAsBrowser,
    get_RegisterAsDropTarget,
    get_TheaterMode,
    get_AddressBar,
    get_Resizable,
);
accepted_fns!(
    put_Visible,
    put_StatusBar,
    put_ToolBar,
    put_MenuBar,
    put_Offline,
    put_Silent,
    put_RegisterAsBrowser,
    put_RegisterAsDropTarget,
    put_TheaterMode,
    put_AddressBar,
    put_Resizable,
);

unsafe extern "win64" fn get_type_info_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    // Pas d'information de type : la documentation d'IDispatch le permet.
    *count = 0;
    S_OK
}

unsafe fn wide_text(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// Chaîne d'un VARIANT d'adresse : VT_BSTR, ou VT_BYREF|VT_VARIANT qui
/// pointe sur un VT_BSTR (oaidl.h : vt sur 2 octets, valeur à +8).
unsafe fn variant_text(v: *const u8) -> Option<String> {
    if v.is_null() {
        return None;
    }
    let vt = std::ptr::read_unaligned(v as *const u16);
    let val = std::ptr::read_unaligned(v.add(8) as *const usize);
    match vt {
        8 => Some(wide_text(val as *const u16)),
        0x400C if val != 0 => variant_text(val as *const u8),
        0x4008 if val != 0 => Some(wide_text(*(val as *const *const u16))),
        _ => None,
    }
}

unsafe fn navigate_to(this: usize, url: String, via: &str) -> i32 {
    let b = from(this, SLOT_BROWSER);
    let page = load(&url);
    match &page {
        Ok(html) => note(format_args!(
            "{via} « {url} » : {} caractères chargés",
            html.chars().count()
        )),
        Err(why) => crate::log_warn!(
            "webbrowser",
            "{via} « {url} » : {why}, appelants {}",
            callers()
        ),
    }
    let text = match page {
        Ok(html) => texte::html_to_text(&html),
        Err(_) => String::new(),
    };
    let hwnd = {
        let mut s = state(b);
        s.url = url;
        s.text = Some(text);
        s.hwnd
    };
    show_text(b);
    // Événements de fin de navigation : après le retour de Navigate, par
    // la file de la fenêtre ; sans fenêtre, tout de suite.
    b.refs.fetch_add(1, Ordering::SeqCst);
    if hwnd == 0
        || super::user32::post_message_w(hwnd, WM_FIRE_EVENTS, 0, b as *const Browser as isize) == 0
    {
        fire_navigation_events(b);
        release_obj(b);
    }
    S_OK
}

/// Contenu HTML d'une adresse : about:blank, fichier (file:// ou chemin
/// Windows) ou ressource d'un module (res://, type RT_HTML par défaut).
unsafe fn load(url: &str) -> Result<String, String> {
    let lower = url.to_ascii_lowercase();
    if lower.is_empty() || lower.starts_with("about:") {
        return Ok(String::new());
    }
    if let Some(rest) = strip_scheme(url, "res://") {
        return load_resource(rest);
    }
    let path = match strip_scheme(url, "file:") {
        Some(rest) => file_url_path(rest),
        None if url.as_bytes().get(1) == Some(&b':') || url.starts_with("\\\\") => url.to_string(),
        None => return Err("schéma non servi (seuls about:, file: et res: le sont)".into()),
    };
    read_file(&path).map(|b| texte::decode_page(&b))
}

fn strip_scheme<'a>(url: &'a str, scheme: &str) -> Option<&'a str> {
    let head = url.get(..scheme.len())?;
    head.eq_ignore_ascii_case(scheme)
        .then(|| &url[scheme.len()..])
}

/// Chemin Windows d'une adresse file: (RFC 8089) : barres obliques
/// retournées, octets %XX décodés en UTF-8.
fn file_url_path(rest: &str) -> String {
    let rest = rest.trim_start_matches('/');
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'/' { b'\\' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Lit un fichier par nos propres CreateFileW et ReadFile.
unsafe fn read_file(path: &str) -> Result<Vec<u8>, String> {
    let w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING.
    let h = super::kernel32::create_file_w(w.as_ptr(), 0x8000_0000, 1, 0, 3, 0, 0);
    if h == usize::MAX {
        return Err(format!(
            "fichier « {path} » illisible (erreur {})",
            crate::process::last_error()
        ));
    }
    let mut data = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        if super::kernel32::read_file(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, 0) == 0
            || n == 0
        {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
    }
    super::kernel32::close_handle(h);
    Ok(data)
}

/// Ressource d'une adresse res:// : `module/[type/]nom` (type RT_HTML, 23,
/// par défaut ; « #N » ou « N » pour un type ou un nom numérique).
unsafe fn load_resource(rest: &str) -> Result<String, String> {
    let mut parts: Vec<&str> = rest.rsplitn(3, '/').collect();
    parts.reverse();
    let (module, ty, name) = match parts.as_slice() {
        [m, n] => (*m, "23", *n),
        [m, t, n] => (*m, *t, *n),
        _ => return Err("adresse res:// sans nom de ressource".into()),
    };
    let module = file_url_path(module);
    let wm: Vec<u16> = module.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hmod = super::kernel32::get_module_handle_w(wm.as_ptr());
    if hmod == 0 {
        // LOAD_LIBRARY_AS_DATAFILE
        hmod = super::kernel32::load_library_ex_w(wm.as_ptr(), 0, 2);
    }
    if hmod == 0 {
        return Err(format!("module « {module} » introuvable"));
    }
    let id = |s: &str| -> (usize, Vec<u16>) {
        let n = s.trim_start_matches('#');
        match n.parse::<u16>() {
            Ok(v) => (v as usize, Vec::new()),
            Err(_) => {
                let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                (0, w)
            }
        }
    };
    let (ty_id, ty_w) = id(ty);
    let (name_id, name_w) = id(name);
    let ty_p = if ty_w.is_empty() {
        ty_id
    } else {
        ty_w.as_ptr() as usize
    };
    let name_p = if name_w.is_empty() {
        name_id
    } else {
        name_w.as_ptr() as usize
    };
    let res = super::resource::find_resource_w(hmod, name_p as *const u16, ty_p as *const u16);
    if res == 0 {
        return Err(format!("ressource « {ty}/{name} » absente de « {module} »"));
    }
    let size = super::resource::sizeof_resource(hmod, res) as usize;
    let data = super::resource::lock_resource(super::resource::load_resource(hmod, res));
    if data == 0 {
        return Err(format!("ressource « {ty}/{name} » illisible"));
    }
    Ok(texte::decode_page(std::slice::from_raw_parts(
        data as *const u8,
        size,
    )))
}

const WM_SETTEXT: u32 = 0x000C;
const WM_SETFONT: u32 = 0x0030;
const ES_MULTILINE: u32 = 0x4;
const ES_AUTOVSCROLL: u32 = 0x40;
const ES_READONLY: u32 = 0x800;
const WS_VSCROLL: u32 = 0x0020_0000;
/// DEFAULT_GUI_FONT
const DEFAULT_GUI_FONT: i32 = 17;

/// Pose le texte de la page dans la zone de lecture, créée au besoin dans
/// la fenêtre du contrôle (si elle existe déjà).
unsafe fn show_text(b: &Browser) {
    let (hwnd, mut edit, text, pos) = {
        let s = state(b);
        (s.hwnd, s.edit, s.text.clone(), s.pos)
    };
    let Some(text) = text else {
        return;
    };
    if hwnd == 0 {
        return;
    }
    if edit == 0 {
        let class: Vec<u16> = "EDIT\0".encode_utf16().collect();
        edit = super::user32::create_window_ex_w(
            0,
            class.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
            0,
            0,
            pos.right - pos.left,
            pos.bottom - pos.top,
            hwnd,
            0,
            0,
            std::ptr::null_mut(),
        );
        let font = super::gdi32::get_stock_object(DEFAULT_GUI_FONT);
        super::user32::send_message_w(edit, WM_SETFONT, font, 0);
        state(b).edit = edit;
    }
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    super::user32::send_message_w(edit, WM_SETTEXT, 0, w.as_ptr() as isize);
}

/// Place la zone de lecture sur toute la fenêtre du contrôle.
unsafe fn fit_edit(b: &Browser, r: Rect) {
    let edit = state(b).edit;
    if edit != 0 {
        super::user32::move_window(edit, 0, 0, r.right - r.left, r.bottom - r.top, 1);
    }
}

unsafe extern "win64" fn navigate(this: usize, url: *const u16) -> i32 {
    if url.is_null() {
        return E_INVALIDARG;
    }
    navigate_to(this, wide_text(url), "Navigate")
}

unsafe extern "win64" fn navigate2(this: usize, url: *const u8) -> i32 {
    match variant_text(url) {
        Some(u) => navigate_to(this, u, "Navigate2"),
        None => {
            let vt = if url.is_null() {
                0
            } else {
                std::ptr::read_unaligned(url as *const u16)
            };
            crate::log_warn!(
                "webbrowser",
                "Navigate2 : adresse de type VARIANT {vt:#x} non lue : E_NOTIMPL ({}:{}), appelants {}",
                file!(),
                line!(),
                callers()
            );
            E_NOTIMPL
        }
    }
}

unsafe extern "win64" fn get_hwnd(this: usize, out: *mut isize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // IWebBrowser2::get_HWND : la documentation le réserve à la fenêtre
    // d'Internet Explorer ; un contrôle incorporé rend E_FAIL.
    let _ = this;
    *out = 0;
    0x8000_4005u32 as i32
}

static WB_VT: VTable<71> = VTable([
    wb_qi as *const (),
    wb_add_ref as *const (),
    wb_release as *const (),
    get_type_info_count as *const (),
    GetTypeInfo as *const (),
    GetIDsOfNames as *const (),
    Invoke as *const (),
    GoBack as *const (),
    GoForward as *const (),
    GoHome as *const (),
    GoSearch as *const (),
    navigate as *const (),
    Refresh as *const (),
    Refresh2 as *const (),
    Stop as *const (),
    get_Application as *const (),
    get_Parent as *const (),
    get_Container as *const (),
    get_document as *const (),
    get_TopLevelContainer as *const (),
    get_Type as *const (),
    get_Left as *const (),
    put_Left as *const (),
    get_Top as *const (),
    put_Top as *const (),
    get_Width as *const (),
    put_Width as *const (),
    get_Height as *const (),
    put_Height as *const (),
    get_LocationName as *const (),
    get_LocationURL as *const (),
    get_Busy as *const (),
    Quit as *const (),
    ClientToWindow as *const (),
    PutProperty as *const (),
    GetProperty as *const (),
    get_Name as *const (),
    get_hwnd as *const (),
    get_FullName as *const (),
    get_Path as *const (),
    get_Visible as *const (),
    put_Visible as *const (),
    get_StatusBar as *const (),
    put_StatusBar as *const (),
    get_StatusText as *const (),
    put_StatusText as *const (),
    get_ToolBar as *const (),
    put_ToolBar as *const (),
    get_MenuBar as *const (),
    put_MenuBar as *const (),
    get_FullScreen as *const (),
    put_FullScreen as *const (),
    navigate2 as *const (),
    QueryStatusWB as *const (),
    ExecWB as *const (),
    ShowBrowserBar as *const (),
    get_ReadyState as *const (),
    get_Offline as *const (),
    put_Offline as *const (),
    get_Silent as *const (),
    put_Silent as *const (),
    get_RegisterAsBrowser as *const (),
    put_RegisterAsBrowser as *const (),
    get_RegisterAsDropTarget as *const (),
    put_RegisterAsDropTarget as *const (),
    get_TheaterMode as *const (),
    put_TheaterMode as *const (),
    get_AddressBar as *const (),
    put_AddressBar as *const (),
    get_Resizable as *const (),
    put_Resizable as *const (),
]);

// ── IOleObject ──────────────────────────────────────────────────────

not_impl_fns!(
    SetMoniker,
    GetMoniker,
    InitFromData,
    GetClipboardData,
    Update,
    IsUpToDate,
    GetUserType,
    EnumAdvise,
    SetColorScheme,
);

unsafe extern "win64" fn set_client_site(this: usize, site: usize) -> i32 {
    let b = from(this, SLOT_OLE_OBJECT);
    if site != 0 {
        com_add_ref(site);
    }
    let old = std::mem::replace(&mut state(b).client_site, site);
    if old != 0 {
        com_release(old);
    }
    note(format_args!("SetClientSite({site:#x})"));
    S_OK
}

unsafe extern "win64" fn get_client_site(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let site = state(from(this, SLOT_OLE_OBJECT)).client_site;
    if site != 0 {
        com_add_ref(site);
    }
    *out = site;
    S_OK
}

/// Méthode acceptée sans effet propre (SetHostNames, InitNew…).
extern "win64" fn accept(_this: usize) -> i32 {
    S_OK
}

unsafe extern "win64" fn close(this: usize, _save: u32) -> i32 {
    let b = from(this, SLOT_OLE_OBJECT);
    let hwnd = {
        let mut s = state(b);
        s.edit = 0;
        std::mem::take(&mut s.hwnd)
    };
    if hwnd != 0 {
        super::user32::destroy_window(hwnd);
    }
    note(format_args!("Close"));
    S_OK
}

const CLASS: &str = "Shell Embedding";
const WM_SIZE: u32 = 0x0005;
/// Événements de navigation à envoyer (lParam : le contrôle, retenu).
const WM_FIRE_EVENTS: u32 = 0x8000 + 0x2228;
const GW_CHILD: u32 = 5;

/// Fenêtre du contrôle : la zone de lecture suit sa taille, quel que soit
/// celui qui la redimensionne (SetObjectRects ou l'hôte directement), et
/// les événements de navigation partent de sa file, après le retour de
/// Navigate, comme ceux d'Internet Explorer.
unsafe extern "win64" fn embed_wnd_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_SIZE => {
            let child = super::user32::get_window(hwnd, GW_CHILD);
            if child != 0 {
                let (w, h) = ((lp & 0xffff) as i32, ((lp >> 16) & 0xffff) as i32);
                super::user32::move_window(child, 0, 0, w, h, 1);
            }
            0
        }
        WM_FIRE_EVENTS if lp != 0 => {
            let b = &*(lp as *const Browser);
            fire_navigation_events(b);
            release_obj(b);
            0
        }
        _ => super::user32::def_window_proc_w(hwnd, msg, wp, lp),
    }
}

/// DWebBrowserEvents2 (exdisp.h, DISPID de exdispid.h) : NavigateComplete2
/// (252), DownloadComplete (104) puis DocumentComplete (259), chacun avec
/// (pDisp, URL) pour 252 et 259. DISPPARAMS : arguments en ordre inverse.
unsafe fn fire_navigation_events(b: &Browser) {
    let (url, sinks) = {
        let s = state(b);
        (s.url.clone(), s.sinks.clone())
    };
    if sinks.is_empty() {
        return;
    }
    let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let bstr = super::oleaut32::sys_alloc_string(wide.as_ptr());
    // VARIANT x64 : vt (u16) à 0, valeur à +8, 24 octets.
    let mut url_var = [0u64; 3];
    *(url_var.as_mut_ptr() as *mut u16) = VT_BSTR;
    url_var[1] = bstr as u64;
    let me = iface(b, SLOT_BROWSER) as u64;
    let mut args = [[0u64; 3]; 2];
    *(args[0].as_mut_ptr() as *mut u16) = VT_VARIANT | VT_BYREF;
    args[0][1] = url_var.as_mut_ptr() as u64;
    *(args[1].as_mut_ptr() as *mut u16) = VT_DISPATCH;
    args[1][1] = me;
    for (_, sink) in &sinks {
        com_add_ref(*sink);
    }
    for (_, sink) in &sinks {
        for (id, n) in [(252, 2u32), (104, 0), (259, 2)] {
            let hr = invoke_event(
                *sink,
                id,
                if n == 0 {
                    std::ptr::null_mut()
                } else {
                    args.as_mut_ptr()
                },
                n,
            );
            note(format_args!("événement {id} envoyé : {hr:#x}"));
        }
    }
    for (_, sink) in &sinks {
        com_release(*sink);
    }
    super::oleaut32::sys_free_string(bstr);
}

const VT_BSTR: u16 = 8;
const VT_DISPATCH: u16 = 9;
const VT_VARIANT: u16 = 12;
const VT_BYREF: u16 = 0x4000;

/// IDispatch::Invoke(dispid, IID_NULL, 0, DISPATCH_METHOD, &params, NULL,
/// NULL, NULL).
unsafe fn invoke_event(disp: usize, id: i32, args: *mut [u64; 3], count: u32) -> i32 {
    #[repr(C)]
    struct DispParams {
        rgvarg: *mut [u64; 3],
        named: *mut i32,
        args: u32,
        named_args: u32,
    }
    let mut p = DispParams {
        rgvarg: args,
        named: std::ptr::null_mut(),
        args: count,
        named_args: 0,
    };
    let null = Guid::default();
    let invoke: unsafe extern "win64" fn(
        usize,
        i32,
        *const Guid,
        u32,
        u16,
        *mut DispParams,
        *mut c_void,
        *mut c_void,
        *mut u32,
    ) -> i32 = std::mem::transmute(method(disp, 6));
    invoke(
        disp,
        id,
        &null,
        0,
        1,
        &mut p,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    )
}

unsafe fn ensure_class() {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }
    let name: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
    let wc = super::gui::WndClassExA {
        size: std::mem::size_of::<super::gui::WndClassExA>() as u32,
        style: 0,
        wnd_proc: embed_wnd_proc as *const () as usize,
        cls_extra: 0,
        wnd_extra: 0,
        instance: 0,
        icon: 0,
        cursor: 0,
        // COLOR_WINDOW + 1 : fond blanc de page.
        background: 6,
        menu_name: std::ptr::null(),
        class_name: name.as_ptr() as *const u8,
        icon_sm: 0,
    };
    super::user32::register_class_ex_w(&wc);
    // Le nom de classe reste lu par la table des classes.
    std::mem::forget(name);
}

/// Active sur place : fenêtre fille dans `parent` au rectangle `pos`, en
/// prévenant le site (IOleInPlaceSite) s'il en a un.
unsafe fn in_place_activate(b: &Browser, parent: usize, pos: Option<Rect>) -> i32 {
    let site = state(b).client_site;
    let mut parent = parent;
    let mut pos = pos;
    let ips = if site != 0 {
        com_qi(site, &IID_IOLE_IN_PLACE_SITE)
    } else {
        0
    };
    if ips != 0 {
        // IOleInPlaceSite : 3 GetWindow, 5 CanInPlaceActivate,
        // 6 OnInPlaceActivate, 8 GetWindowContext.
        let can: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(method(ips, 5));
        if can(ips) != S_OK {
            com_release(ips);
            note(format_args!("DoVerb : CanInPlaceActivate refusé"));
            return E_NOTIMPL;
        }
        if parent == 0 {
            let get_window: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
                std::mem::transmute(method(ips, 3));
            get_window(ips, &mut parent);
        }
        if pos.is_none() {
            let ctx: unsafe extern "win64" fn(
                usize,
                *mut usize,
                *mut usize,
                *mut Rect,
                *mut Rect,
                *mut u8,
            ) -> i32 = std::mem::transmute(method(ips, 8));
            let (mut frame, mut doc) = (0usize, 0usize);
            let (mut r, mut clip) = (Rect::default(), Rect::default());
            // OLEINPLACEFRAMEINFO : cb (4) puis 4 champs, 32 octets en x64.
            let mut info = [0u8; 32];
            info[..4].copy_from_slice(&32u32.to_le_bytes());
            if ctx(
                ips,
                &mut frame,
                &mut doc,
                &mut r,
                &mut clip,
                info.as_mut_ptr(),
            ) >= 0
            {
                pos = Some(r);
                if frame != 0 {
                    com_release(frame);
                }
                if doc != 0 {
                    com_release(doc);
                }
            }
        }
        let on: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(method(ips, 6));
        on(ips);
        com_release(ips);
    }
    let mut r = pos.unwrap_or_default();
    if (r.right <= r.left || r.bottom <= r.top) && parent != 0 {
        // Rectangle vide (l'installeur WinRAR passe 0,0,0,0) : la fenêtre
        // hôte entière, sinon la page resterait invisible.
        let mut c = Rect::default();
        if super::user32::get_client_rect(parent, (&mut c as *mut Rect).cast()) != 0 {
            note(format_args!(
                "rectangle vide : zone cliente de l'hôte, {c:?}"
            ));
            r = c;
        }
    }
    let existing = state(b).hwnd;
    if existing != 0 {
        super::user32::move_window(
            existing,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            1,
        );
        super::user32::show_window(existing, SW_SHOW);
        state(b).pos = r;
        fit_edit(b, r);
        return S_OK;
    }
    ensure_class();
    let name: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = super::user32::create_window_ex_w(
        0,
        name.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        parent,
        0,
        0,
        std::ptr::null_mut(),
    );
    {
        let mut s = state(b);
        s.hwnd = hwnd;
        s.parent = parent;
        s.pos = r;
    }
    note(format_args!(
        "activé sur place : fenêtre {hwnd:#x} dans {parent:#x}, {r:?}"
    ));
    show_text(b);
    S_OK
}

unsafe extern "win64" fn do_verb(
    this: usize,
    verb: i32,
    _msg: usize,
    _site: usize,
    _index: i32,
    parent: usize,
    rect: *const Rect,
) -> i32 {
    let b = from(this, SLOT_OLE_OBJECT);
    let pos = if rect.is_null() {
        None
    } else {
        Some(std::ptr::read_unaligned(rect))
    };
    note(format_args!("DoVerb({verb}, parent {parent:#x}, {pos:?})"));
    match verb {
        OLEIVERB_PRIMARY | OLEIVERB_SHOW | OLEIVERB_UIACTIVATE | OLEIVERB_INPLACEACTIVATE => {
            in_place_activate(b, parent, pos)
        }
        OLEIVERB_HIDE => {
            let hwnd = state(b).hwnd;
            if hwnd != 0 {
                super::user32::show_window(hwnd, SW_HIDE);
            }
            S_OK
        }
        _ => {
            crate::log_warn!(
                "webbrowser",
                "DoVerb({verb}) : verbe non servi ({}:{}), appelants {}",
                file!(),
                line!(),
                callers()
            );
            OLEOBJ_E_NOVERBS
        }
    }
}

unsafe extern "win64" fn enum_verbs(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_impl("EnumVerbs", line!())
}

unsafe extern "win64" fn get_user_class_id(_this: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_unaligned(out, CLSID_WEB_BROWSER);
    S_OK
}

unsafe extern "win64" fn set_extent(this: usize, _aspect: u32, size: *const i32) -> i32 {
    if size.is_null() {
        return E_INVALIDARG;
    }
    state(from(this, SLOT_OLE_OBJECT)).extent = (*size, *size.add(1));
    S_OK
}

unsafe extern "win64" fn get_extent(this: usize, _aspect: u32, size: *mut i32) -> i32 {
    if size.is_null() {
        return E_POINTER;
    }
    let (cx, cy) = state(from(this, SLOT_OLE_OBJECT)).extent;
    *size = cx;
    *size.add(1) = cy;
    S_OK
}

unsafe extern "win64" fn advise(_this: usize, _sink: usize, cookie: *mut u32) -> i32 {
    // IAdviseSink n'est pas retenu : le contrôle ne change pas encore de
    // vue ni de données à signaler.
    if !cookie.is_null() {
        *cookie = 1;
    }
    S_OK
}

unsafe extern "win64" fn get_misc_status(_this: usize, _aspect: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = MISC_STATUS;
    S_OK
}

static OO_VT: VTable<24> = VTable([
    oo_qi as *const (),
    oo_add_ref as *const (),
    oo_release as *const (),
    set_client_site as *const (),
    get_client_site as *const (),
    accept as *const (), // SetHostNames
    close as *const (),
    SetMoniker as *const (),
    GetMoniker as *const (),
    InitFromData as *const (),
    GetClipboardData as *const (),
    do_verb as *const (),
    enum_verbs as *const (),
    Update as *const (),
    IsUpToDate as *const (),
    get_user_class_id as *const (),
    GetUserType as *const (),
    set_extent as *const (),
    get_extent as *const (),
    advise as *const (),
    accept as *const (), // Unadvise
    EnumAdvise as *const (),
    get_misc_status as *const (),
    SetColorScheme as *const (),
]);

// ── IOleInPlaceObject et IOleInPlaceActiveObject ────────────────────

unsafe fn get_window_of(b: &Browser, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let hwnd = state(b).hwnd;
    *out = hwnd;
    if hwnd == 0 {
        0x8000_4005u32 as i32 // E_FAIL : pas encore actif
    } else {
        S_OK
    }
}

unsafe extern "win64" fn ip_get_window(this: usize, out: *mut usize) -> i32 {
    get_window_of(from(this, SLOT_IN_PLACE), out)
}

unsafe extern "win64" fn ao_get_window(this: usize, out: *mut usize) -> i32 {
    get_window_of(from(this, SLOT_ACTIVE), out)
}

unsafe extern "win64" fn in_place_deactivate(this: usize) -> i32 {
    let b = from(this, SLOT_IN_PLACE);
    let hwnd = state(b).hwnd;
    if hwnd != 0 {
        super::user32::show_window(hwnd, SW_HIDE);
    }
    note(format_args!("InPlaceDeactivate"));
    S_OK
}

unsafe extern "win64" fn set_object_rects(
    this: usize,
    pos: *const Rect,
    _clip: *const Rect,
) -> i32 {
    if pos.is_null() {
        return E_INVALIDARG;
    }
    let b = from(this, SLOT_IN_PLACE);
    let r = std::ptr::read_unaligned(pos);
    let hwnd = {
        let mut s = state(b);
        s.pos = r;
        s.hwnd
    };
    if hwnd != 0 {
        super::user32::move_window(hwnd, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
        fit_edit(b, r);
    }
    note(format_args!("SetObjectRects({r:?})"));
    S_OK
}

not_impl_fns!(ContextSensitiveHelp, ReactivateAndUndo);

static IP_VT: VTable<9> = VTable([
    ip_qi as *const (),
    ip_add_ref as *const (),
    ip_release as *const (),
    ip_get_window as *const (),
    ContextSensitiveHelp as *const (),
    in_place_deactivate as *const (),
    accept as *const (), // UIDeactivate
    set_object_rects as *const (),
    ReactivateAndUndo as *const (),
]);

extern "win64" fn translate_accelerator(_this: usize, _msg: usize) -> i32 {
    // Rien à traduire : le message suit son cours chez le conteneur.
    S_FALSE
}

static AO_VT: VTable<10> = VTable([
    ao_qi as *const (),
    ao_add_ref as *const (),
    ao_release as *const (),
    ao_get_window as *const (),
    ContextSensitiveHelp as *const (),
    translate_accelerator as *const (),
    accept as *const (), // OnFrameWindowActivate
    accept as *const (), // OnDocWindowActivate
    accept as *const (), // ResizeBorder
    accept as *const (), // EnableModeless
]);

// ── IViewObject2 ────────────────────────────────────────────────────

not_impl_fns!(Draw, GetColorSet, Freeze, Unfreeze, GetAdvise, VoGetExtent);

static VO_VT: VTable<10> = VTable([
    vo_qi as *const (),
    vo_add_ref as *const (),
    vo_release as *const (),
    Draw as *const (),
    GetColorSet as *const (),
    Freeze as *const (),
    Unfreeze as *const (),
    accept as *const (), // SetAdvise
    GetAdvise as *const (),
    VoGetExtent as *const (),
]);

// ── IPersistStreamInit ──────────────────────────────────────────────

not_impl_fns!(Load, Save, GetSizeMax);

unsafe extern "win64" fn ps_get_class_id(_this: usize, out: *mut Guid) -> i32 {
    get_user_class_id(0, out)
}

extern "win64" fn is_dirty(_this: usize) -> i32 {
    S_FALSE
}

static PS_VT: VTable<9> = VTable([
    ps_qi as *const (),
    ps_add_ref as *const (),
    ps_release as *const (),
    ps_get_class_id as *const (),
    is_dirty as *const (),
    Load as *const (),
    Save as *const (),
    GetSizeMax as *const (),
    accept as *const (), // InitNew
]);

// ── IConnectionPointContainer et IConnectionPoint ───────────────────

not_impl_fns!(EnumConnectionPoints, EnumConnections);

unsafe extern "win64" fn find_connection_point(
    this: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let b = from(this, SLOT_CPC);
    let g = std::ptr::read_unaligned(iid);
    if g != DIID_DWEB_BROWSER_EVENTS2 {
        crate::log_warn!(
            "webbrowser",
            "FindConnectionPoint {} : CONNECT_E_NOCONNECTION ({}:{}), appelants {}",
            guid_str(&g),
            file!(),
            line!(),
            callers()
        );
        return CONNECT_E_NOCONNECTION;
    }
    b.refs.fetch_add(1, Ordering::SeqCst);
    *out = iface(b, SLOT_CP);
    S_OK
}

static CPC_VT: VTable<5> = VTable([
    cpc_qi as *const (),
    cpc_add_ref as *const (),
    cpc_release as *const (),
    EnumConnectionPoints as *const (),
    find_connection_point as *const (),
]);

unsafe extern "win64" fn cp_qi(this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let g = std::ptr::read_unaligned(iid);
    if g == IID_IUNKNOWN || g == IID_ICONNECTION_POINT {
        let b = from(this, SLOT_CP);
        b.refs.fetch_add(1, Ordering::SeqCst);
        *out = iface(b, SLOT_CP);
        return S_OK;
    }
    super::comobj::log_no_interface("WebBrowser (point de connexion)", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn cp_add_ref(this: usize) -> u32 {
    from(this, SLOT_CP).refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "win64" fn cp_release(this: usize) -> u32 {
    release_obj(from(this, SLOT_CP))
}

unsafe extern "win64" fn get_connection_interface(_this: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_unaligned(out, DIID_DWEB_BROWSER_EVENTS2);
    S_OK
}

unsafe extern "win64" fn get_connection_point_container(this: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let b = from(this, SLOT_CP);
    b.refs.fetch_add(1, Ordering::SeqCst);
    *out = iface(b, SLOT_CPC);
    S_OK
}

unsafe extern "win64" fn cp_advise(this: usize, sink: usize, cookie: *mut u32) -> i32 {
    if cookie.is_null() {
        return E_POINTER;
    }
    *cookie = 0;
    if sink == 0 {
        return E_POINTER;
    }
    let disp = com_qi(sink, &IID_IDISPATCH);
    if disp == 0 {
        // CONNECT_E_CANNOTCONNECT : l'abonné doit être un IDispatch.
        return 0x8004_0202u32 as i32;
    }
    let b = from(this, SLOT_CP);
    let mut s = state(b);
    s.next_cookie += 1;
    let c = s.next_cookie;
    s.sinks.push((c, disp));
    *cookie = c;
    note(format_args!("Advise DWebBrowserEvents2 : jeton {c}"));
    S_OK
}

unsafe extern "win64" fn cp_unadvise(this: usize, cookie: u32) -> i32 {
    let b = from(this, SLOT_CP);
    let gone = {
        let mut s = state(b);
        let pos = s.sinks.iter().position(|(c, _)| *c == cookie);
        pos.map(|i| s.sinks.remove(i).1)
    };
    match gone {
        Some(sink) => {
            com_release(sink);
            S_OK
        }
        None => CONNECT_E_NOCONNECTION,
    }
}

static CP_VT: VTable<8> = VTable([
    cp_qi as *const (),
    cp_add_ref as *const (),
    cp_release as *const (),
    get_connection_interface as *const (),
    get_connection_point_container as *const (),
    cp_advise as *const (),
    cp_unadvise as *const (),
    EnumConnections as *const (),
]);

// ── IOleControl ─────────────────────────────────────────────────────

not_impl_fns!(GetControlInfo, OnMnemonic);

static OC_VT: VTable<7> = VTable([
    oc_qi as *const (),
    oc_add_ref as *const (),
    oc_release as *const (),
    GetControlInfo as *const (),
    OnMnemonic as *const (),
    accept as *const (), // OnAmbientPropertyChange
    accept as *const (), // FreezeEvents
]);

// ── Création ────────────────────────────────────────────────────────

fn new_browser() -> *mut Browser {
    Box::into_raw(Box::new(Browser {
        vtbls: [
            &WB_VT as *const _ as *const (),
            &OO_VT as *const _ as *const (),
            &IP_VT as *const _ as *const (),
            &AO_VT as *const _ as *const (),
            &VO_VT as *const _ as *const (),
            &PS_VT as *const _ as *const (),
            &CPC_VT as *const _ as *const (),
            &OC_VT as *const _ as *const (),
            &CP_VT as *const _ as *const (),
        ],
        refs: AtomicU32::new(1),
        state: Mutex::new(State::default()),
    }))
}

/// CoCreateInstance de CLSID_WebBrowser (et de sa version 1).
///
/// # Safety
/// `out` doit être valide en écriture.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    if *clsid != CLSID_WEB_BROWSER && *clsid != CLSID_WEB_BROWSER_V1 {
        return None;
    }
    let iid = iid as *const _ as *const Guid;
    let b = new_browser();
    let hr = query(&*b, iid, out);
    note(format_args!(
        "créé ({}), interface {}, appelants {}",
        guid_str(clsid),
        guid_str(&*iid),
        callers()
    ));
    release_obj(&*b);
    Some(hr)
}

mod document;

/// IWebBrowser2::get_Document : le document de la page (#2228).
unsafe extern "win64" fn get_document(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let b = from(this, SLOT_BROWSER);
    *out = document::document_of(b);
    note(format_args!("get_Document rendu"));
    S_OK
}

#[cfg(test)]
mod tests;
