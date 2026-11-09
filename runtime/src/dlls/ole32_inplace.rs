//! Insertion depuis un fichier et activation sur place (#1109).
//!
//! - `GetClassFile` : CLSID d'un docfile, sinon par l'extension
//!   (`HKCR\.ext` → identifiant de programme → `CLSID`).
//! - `OleCreateFromFile` : un docfile est copié dans le stockage puis
//!   chargé par `OleLoad` ; sinon la classe associée est créée, initialisée
//!   (`IPersistStorage::InitNew`) et chargée (`IPersistFile::Load`). Sans
//!   classe associée, l'objet « paquet » (CLSID_Package) est demandé : il
//!   n'existe que si un serveur l'inscrit, Winlux n'en fournit pas (format
//!   natif non publié) et rend alors REGDB_E_CLASSNOTREG.
//! - Côté système de l'activation sur place : descripteurs de menu OLE
//!   (`OleCreateMenuDescriptor`, `OleSetMenuDescriptor`,
//!   `OleDestroyMenuDescriptor`), `OleTranslateAccelerator` et
//!   `IsAccelerator`. Les interfaces IOleInPlace* sont celles de l'objet et
//!   du conteneur.
//!
//! Sources : documentation publique d'OLE et en-têtes mingw-w64 (ole2.h,
//! oleidl.h).

use super::ole32::Guid;
use super::ole32_embed::{hand_out, query, release, set_client_site, slot};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const MK_E_INVALIDEXTENSION: i32 = 0x8004_01E6u32 as i32;
const MK_E_CANTOPENFILE: i32 = 0x8004_01EAu32 as i32;
const STGM_READ_SHARE_EXCLUSIVE: u32 = 0x10;
const CLSCTX_SERVER_AND_HANDLER: u32 = 0x1 | 0x2 | 0x4;

const fn ole_guid(data1: u32) -> Guid {
    Guid {
        data1,
        data2: 0,
        data3: 0,
        data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
    }
}
const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_IPERSISTSTORAGE: Guid = ole_guid(0x10A);
const IID_IPERSISTFILE: Guid = ole_guid(0x10B);
/// CLSID_Package {0003000C-0000-0000-C000-000000000046}.
pub const CLSID_PACKAGE: Guid = ole_guid(0x0003_000C);

unsafe fn wide(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)))
}

// ── GetClassFile ────────────────────────────────────────────────────

/// Classe associée à l'extension d'un nom de fichier, lue par `lookup`
/// (chemin de clé, nom de valeur) : `HKCR\.ext` → ProgID → `ProgID\CLSID`.
fn class_of_extension(file: &str, lookup: impl Fn(&str, &str) -> Option<String>) -> Option<Guid> {
    let name = file.rsplit(['\\', '/']).next().unwrap_or(file);
    let ext = name
        .rfind('.')
        .map(|i| &name[i..])
        .filter(|e| e.len() > 1)?;
    let prog = lookup(&format!("hkcr\\{}", ext.to_lowercase()), "")?;
    let prog = prog.trim();
    if prog.is_empty() {
        return None;
    }
    let text = lookup(&format!("hkcr\\{}\\clsid", prog.to_lowercase()), "")?;
    super::ole32::parse_guid(text.trim())
}

/// CLSID racine d'un docfile, s'il en a un.
unsafe fn class_of_docfile(file: *const u16) -> Option<Guid> {
    if super::ole32_storage::stg_is_storage_file(file) != S_OK {
        return None;
    }
    let mut stg = 0usize;
    if super::ole32_storage::stg_open_storage(file, 0, STGM_READ_SHARE_EXCLUSIVE, 0, 0, &mut stg)
        < 0
    {
        return None;
    }
    let mut clsid = Guid::default();
    let hr = super::ole32_storage::read_class_stg(stg, &mut clsid);
    release(stg);
    (hr >= 0 && clsid != Guid::default()).then_some(clsid)
}

pub unsafe extern "win64" fn get_class_file(file: *const u16, clsid: *mut Guid) -> i32 {
    if clsid.is_null() {
        return E_POINTER;
    }
    std::ptr::write_unaligned(clsid, Guid::default());
    let Some(name) = wide(file) else {
        return E_INVALIDARG;
    };
    if !crate::paths::to_linux(&name).is_file() {
        return MK_E_CANTOPENFILE;
    }
    if let Some(g) = class_of_docfile(file) {
        std::ptr::write_unaligned(clsid, g);
        return S_OK;
    }
    match class_of_extension(&name, crate::registry::get_string) {
        Some(g) => {
            std::ptr::write_unaligned(clsid, g);
            S_OK
        }
        None => MK_E_INVALIDEXTENSION,
    }
}

// ── OleCreateFromFile ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn ole_create_from_file(
    _clsid: *const Guid,
    file: *const u16,
    riid: *const Guid,
    _render: u32,
    _format: usize,
    site: usize,
    storage: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if riid.is_null() || storage == 0 {
        return E_INVALIDARG;
    }
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let Some(name) = wide(file) else {
        return E_INVALIDARG;
    };
    if !crate::paths::to_linux(&name).is_file() {
        return MK_E_CANTOPENFILE;
    }
    // Docfile : son contenu devient celui de l'objet incorporé.
    if class_of_docfile(file).is_some() {
        let mut src = 0usize;
        let hr = super::ole32_storage::stg_open_storage(
            file,
            0,
            STGM_READ_SHARE_EXCLUSIVE,
            0,
            0,
            &mut src,
        );
        if hr < 0 {
            return hr;
        }
        let copy: unsafe extern "win64" fn(usize, u32, usize, usize, usize) -> i32 =
            std::mem::transmute(slot(src, 7));
        let hr = copy(src, 0, 0, 0, storage);
        release(src);
        if hr < 0 {
            return hr;
        }
        return super::ole32_embed::ole_load(storage, riid, site, out);
    }
    let clsid = class_of_extension(&name, crate::registry::get_string).unwrap_or(CLSID_PACKAGE);
    let mut unk = 0usize;
    let hr = super::ole32::co_create_instance(
        &clsid,
        0,
        CLSCTX_SERVER_AND_HANDLER,
        &IID_IUNKNOWN,
        (&mut unk as *mut usize).cast(),
    );
    if hr < 0 {
        return hr;
    }
    let hr = set_client_site(unk, site);
    if hr < 0 {
        release(unk);
        return hr;
    }
    let ps = query(unk, &IID_IPERSISTSTORAGE);
    if ps != 0 {
        let init: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(slot(ps, 5));
        let hr = init(ps, storage);
        release(ps);
        if hr < 0 {
            release(unk);
            return hr;
        }
    }
    let pf = query(unk, &IID_IPERSISTFILE);
    if pf == 0 {
        release(unk);
        return E_NOINTERFACE;
    }
    let load: unsafe extern "win64" fn(usize, *const u16, u32) -> i32 =
        std::mem::transmute(slot(pf, 5));
    let hr = load(pf, file, 0);
    release(pf);
    if hr < 0 {
        release(unk);
        return hr;
    }
    hand_out(unk, riid, out)
}

// ── Accélérateurs ───────────────────────────────────────────────────

fn key_down(vk: usize) -> bool {
    super::gui::get_key_state(vk) < 0
}

/// Commande de la table `haccel` (au plus `count` entrées) pour ce message.
unsafe fn accel_command(haccel: usize, count: i32, msg: *const super::gui::Msg) -> Option<u16> {
    let m = msg.as_ref()?;
    let mut t = super::accel::table(haccel)?;
    if count >= 0 {
        t.truncate(count as usize);
    }
    super::accel::match_accel(
        &t,
        m.message,
        m.wparam,
        (key_down(0x10), key_down(0x11), key_down(0x12)),
    )
}

/// IsAccelerator : TRUE et la commande si le message est un raccourci.
pub unsafe extern "win64" fn is_accelerator(
    haccel: usize,
    count: i32,
    msg: *const super::gui::Msg,
    cmd: *mut u16,
) -> i32 {
    let found = accel_command(haccel, count, msg);
    if !cmd.is_null() {
        *cmd = found.unwrap_or(0);
    }
    found.is_some() as i32
}

/// OleTranslateAccelerator : un raccourci du conteneur (OLEINPLACEFRAMEINFO :
/// haccel @16, cAccelEntries @24) part vers IOleInPlaceFrame::TranslateAccelerator.
pub unsafe extern "win64" fn ole_translate_accelerator(
    frame: usize,
    info: *const u8,
    msg: *const super::gui::Msg,
) -> i32 {
    if frame == 0 || info.is_null() || msg.is_null() {
        return E_INVALIDARG;
    }
    let haccel = std::ptr::read_unaligned(info.add(16) as *const usize);
    let count = std::ptr::read_unaligned(info.add(24) as *const u32) as i32;
    let Some(cmd) = accel_command(haccel, count, msg) else {
        return S_FALSE;
    };
    let f: unsafe extern "win64" fn(usize, *const super::gui::Msg, u16) -> i32 =
        std::mem::transmute(slot(frame, 14));
    f(frame, msg, cmd)
}

// ── Descripteurs de menu ────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Descriptor {
    menu: usize,
    /// OLEMENUGROUPWIDTHS : File, Edit, Container, Object, Window, Help ;
    /// les groupes impairs (Edit, Object, Help) sont ceux de l'objet.
    widths: [i32; 6],
}

struct Hook {
    old_proc: usize,
    desc: Descriptor,
    active: usize,
    /// Le menu ouvert en dernier appartient à l'objet.
    object_menu: bool,
}

fn descriptors() -> &'static Mutex<HashMap<usize, Descriptor>> {
    static D: OnceLock<Mutex<HashMap<usize, Descriptor>>> = OnceLock::new();
    D.get_or_init(Default::default)
}
fn hooks() -> &'static Mutex<HashMap<usize, Hook>> {
    static H: OnceLock<Mutex<HashMap<usize, Hook>>> = OnceLock::new();
    H.get_or_init(Default::default)
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Le menu de rang `index` (barre combinée) appartient-il à l'objet ?
fn is_object_group(widths: &[i32; 6], index: usize) -> bool {
    let mut end = 0usize;
    for (g, w) in widths.iter().enumerate() {
        end += (*w).max(0) as usize;
        if index < end {
            return g % 2 == 1;
        }
    }
    false
}

pub unsafe extern "win64" fn ole_create_menu_descriptor(menu: usize, widths: *const i32) -> usize {
    if menu == 0 || widths.is_null() {
        return 0;
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x4F4C_0010);
    let h = NEXT.fetch_add(0x10, std::sync::atomic::Ordering::Relaxed);
    let widths = std::ptr::read_unaligned(widths as *const [i32; 6]);
    lock(descriptors()).insert(h, Descriptor { menu, widths });
    h
}

pub extern "win64" fn ole_destroy_menu_descriptor(holemenu: usize) -> i32 {
    lock(descriptors()).remove(&holemenu);
    S_OK
}

const WM_COMMAND: u32 = 0x0111;
const WM_INITMENUPOPUP: u32 = 0x0117;
const WM_MENUSELECT: u32 = 0x011F;
const WM_NCDESTROY: u32 = 0x0082;
const GWLP_WNDPROC: i32 = -4;
const MF_POPUP: usize = 0x10;

/// Rang du sous-menu `popup` dans la barre combinée, s'il en est un.
fn top_index(menu: usize, popup: usize) -> Option<usize> {
    (0..64).find(|&i| super::menus::get_sub_menu(menu, i as i32) == popup)
}

/// Procédure de la fenêtre cadre pendant l'activation sur place : les menus
/// et commandes du groupe de l'objet partent vers la fenêtre de l'objet.
unsafe extern "win64" fn frame_proc(hwnd: usize, msg: u32, wparam: usize, lparam: isize) -> isize {
    let (old, forward) = {
        let mut h = lock(hooks());
        let Some(hook) = h.get_mut(&hwnd) else {
            return 0;
        };
        match msg {
            WM_INITMENUPOPUP if (lparam >> 16) & 0xFFFF == 0 => {
                if let Some(i) = top_index(hook.desc.menu, wparam) {
                    hook.object_menu = is_object_group(&hook.desc.widths, i);
                }
            }
            WM_MENUSELECT
                if (wparam >> 16) & MF_POPUP != 0 && lparam as usize == hook.desc.menu =>
            {
                hook.object_menu = is_object_group(&hook.desc.widths, wparam & 0xFFFF);
            }
            _ => {}
        }
        let menu_msg = matches!(msg, WM_INITMENUPOPUP | WM_MENUSELECT)
            || (msg == WM_COMMAND && (wparam >> 16) == 0 && lparam == 0);
        let fwd = (menu_msg && hook.object_menu && hook.active != 0).then_some(hook.active);
        let old = hook.old_proc;
        if msg == WM_NCDESTROY {
            h.remove(&hwnd);
        }
        (old, fwd)
    };
    if let Some(target) = forward {
        return super::user32::send_message_w(target, msg, wparam, lparam);
    }
    super::user32::call_window_proc(old, hwnd, msg, wparam, lparam)
}

fn unhook(frame: usize) {
    let hook = lock(hooks()).remove(&frame);
    if let Some(hook) = hook {
        unsafe {
            super::user32::set_window_long_ptr_w(frame, GWLP_WNDPROC, hook.old_proc as isize);
        }
    }
}

/// OleSetMenuDescriptor : installe (ou retire, sans descripteur) le routage
/// des menus de la fenêtre cadre vers la fenêtre de l'objet actif.
pub unsafe extern "win64" fn ole_set_menu_descriptor(
    holemenu: usize,
    frame: usize,
    active: usize,
    _frame_cb: usize,
    _active_obj: usize,
) -> i32 {
    if frame == 0 {
        return E_INVALIDARG;
    }
    if holemenu == 0 {
        unhook(frame);
        return S_OK;
    }
    let Some(desc) = lock(descriptors()).get(&holemenu).copied() else {
        return E_INVALIDARG;
    };
    if active == 0 {
        return E_INVALIDARG;
    }
    let mut h = lock(hooks());
    if let Some(hook) = h.get_mut(&frame) {
        hook.desc = desc;
        hook.active = active;
        hook.object_menu = false;
        return S_OK;
    }
    drop(h);
    let old =
        super::user32::set_window_long_ptr_w(frame, GWLP_WNDPROC, frame_proc as *const () as isize);
    lock(hooks()).insert(
        frame,
        Hook {
            old_proc: old as usize,
            desc,
            active,
            object_menu: false,
        },
    );
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetClassFile" => fn get_class_file,
        "OleCreateFromFile" => fn ole_create_from_file,
        "IsAccelerator" => fn is_accelerator,
        "OleTranslateAccelerator" => fn ole_translate_accelerator,
        "OleCreateMenuDescriptor" => fn ole_create_menu_descriptor,
        "OleDestroyMenuDescriptor" => fn ole_destroy_menu_descriptor,
        "OleSetMenuDescriptor" => fn ole_set_menu_descriptor
    }
}

#[cfg(test)]
mod tests;
