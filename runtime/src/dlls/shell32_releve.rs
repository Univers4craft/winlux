//! shell32 : les fonctions du lot D du relevé d'imports (#1512).
//!
//! Importées par pywin32 (module shell), Qt 6 et sharp. Elles s'appuient
//! sur ce que Winlux a déjà : les identifiants d'éléments « WLXP » (un
//! chemin Windows complet par élément), IShellItem et IShellItemArray de
//! filedialog_com, IShellFolder de shell_folder_com, les objets de données
//! OLE et la corbeille freedesktop de l'Explorateur.

#![allow(clippy::missing_safety_doc)]

use super::apps_win32::{path_from_pidl, pidl_from_path};
use super::audio_com::{Com, VTable};
use super::comobj::{guid, Guid};
use crate::dlls::{lookup, wstr, Exports};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),* $(,)?) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002_u32 as i32;
const E_POINTER: i32 = 0x8000_4003_u32 as i32;
const E_FAIL: i32 = 0x8000_4005_u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000E_u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
const HR_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;
const HR_NOT_SUPPORTED: i32 = 0x8007_0032_u32 as i32;
const HR_CANCELLED: i32 = 0x8007_04C7_u32 as i32;
const STG_E_ACCESSDENIED: i32 = 0x8003_0005_u32 as i32;

const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPROPERTYSTORE: Guid = guid(
    0x886D_8EEB,
    0x8CF2,
    0x4446,
    [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
);
const IID_IQUERYASSOCIATIONS: Guid = guid(
    0xC46C_A590,
    0x3C3F,
    0x11D2,
    [0xBE, 0xE6, 0x00, 0x00, 0xF8, 0x05, 0xCA, 0x57],
);
/// FMTID_Storage (propkey.h) : System.ItemNameDisplay (10), System.Size
/// (12), System.DateModified (14).
const FMTID_STORAGE: Guid = guid(
    0xB725_F130,
    0x47EF,
    0x101A,
    [0xA5, 0xF1, 0x02, 0x60, 0x8C, 0x9E, 0xEB, 0xAC],
);

unsafe fn read_iid(p: *const u8) -> Option<Guid> {
    (!p.is_null()).then(|| std::ptr::read_unaligned(p as *const Guid))
}

unsafe fn clear_out(out: *mut *mut c_void) -> bool {
    if out.is_null() {
        return false;
    }
    *out = std::ptr::null_mut();
    true
}

/// Chemin absolu d'un identifiant, ou du dossier parent si l'identifiant
/// est vide.
unsafe fn pidl_path(parent: *const u8, pidl: *const u8) -> Option<String> {
    path_from_pidl(pidl).or_else(|| path_from_pidl(parent))
}

fn existing(path: &str) -> bool {
    crate::paths::to_linux(path).exists()
}

fn join(dir: &str, name: &str) -> String {
    if name.is_empty() {
        return dir.to_string();
    }
    format!(
        "{}\\{}",
        dir.trim_end_matches(['\\', '/']),
        name.trim_start_matches(['\\', '/'])
    )
}

unsafe fn item_for(path: &str, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if !existing(path) {
        return HR_FILE_NOT_FOUND;
    }
    let item = super::filedialog_com::new_item(path);
    let hr = super::filedialog_com::item_qi(item, riid.cast(), out);
    Com::release(item);
    hr
}

unsafe fn put_pidl(path: &str, out: *mut usize) -> i32 {
    match pidl_from_path(path) {
        0 => E_OUTOFMEMORY,
        p => {
            *out = p;
            S_OK
        }
    }
}

const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_FILESYSTEM: u32 = 0x4000_0000;
const SFGAO_STREAM: u32 = 0x0040_0000;

fn attributes(path: &str) -> u32 {
    let dir = crate::paths::to_linux(path).is_dir();
    SFGAO_FILESYSTEM | if dir { SFGAO_FOLDER } else { SFGAO_STREAM }
}

// ── Identifiants d'éléments ─────────────────────────────────────────────

/// SHParseDisplayName : identifiant d'un chemin existant.
unsafe extern "win64" fn sh_parse_display_name(
    name: *const u16,
    _bc: usize,
    out: *mut usize,
    want: u32,
    got: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if !got.is_null() {
        *got = 0;
    }
    let text = wstr(name);
    if text.is_empty() {
        return E_INVALIDARG;
    }
    let path = crate::paths::full_windows(&text);
    if !existing(&path) {
        return HR_FILE_NOT_FOUND;
    }
    if !got.is_null() {
        *got = attributes(&path) & want;
    }
    put_pidl(&path, out)
}

/// SHILCreateFromPath (ordinal 28) : même chose, attributs en entrée et
/// en sortie.
unsafe extern "win64" fn sh_il_create_from_path(
    name: *const u16,
    out: *mut usize,
    inout: *mut u32,
) -> i32 {
    let want = if inout.is_null() { 0 } else { *inout };
    sh_parse_display_name(name, 0, out, want, inout)
}

/// SHGetFolderLocation : identifiant d'un dossier CSIDL.
unsafe extern "win64" fn sh_get_folder_location(
    _hwnd: usize,
    csidl: i32,
    _token: usize,
    _reserved: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    match super::knownfolders::csidl_path(csidl) {
        Some(p) => put_pidl(&p, out),
        None => E_INVALIDARG,
    }
}

/// SHGetNameFromIDList : nom selon SIGDN, alloué par CoTaskMemAlloc.
unsafe extern "win64" fn sh_get_name_from_id_list(
    pidl: *const u8,
    sigdn: u32,
    out: *mut *mut u16,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(path) = path_from_pidl(pidl) else {
        return E_INVALIDARG;
    };
    co_string(&super::filedialog_com::display_name(&path, sigdn), out)
}

unsafe fn co_string(s: &str, out: *mut *mut u16) -> i32 {
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let p = super::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    S_OK
}

/// SHGetIDListFromObject : identifiant d'un IShellItem, d'un IShellFolder
/// de Winlux, ou de tout objet qui rend son chemin.
unsafe extern "win64" fn sh_get_id_list_from_object(obj: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if obj == 0 {
        return E_INVALIDARG;
    }
    let path =
        super::shell_folder_com::folder_path(obj).or_else(|| super::filedialog_com::item_path(obj));
    match path {
        Some(p) => put_pidl(&p, out),
        None => E_NOINTERFACE,
    }
}

/// SHBindToParent : IShellFolder du dossier parent, et le dernier élément
/// de l'identifiant (qui porte le chemin complet, valable dans ce dossier).
unsafe extern "win64" fn sh_bind_to_parent(
    pidl: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
    last: *mut *const u8,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    if !last.is_null() {
        *last = std::ptr::null();
    }
    let Some(path) = path_from_pidl(pidl) else {
        return E_INVALIDARG;
    };
    let parent = match path.trim_end_matches('\\').rfind('\\') {
        Some(i) if i >= 2 => {
            let p = &path[..i];
            if p.len() == 2 {
                format!("{p}\\")
            } else {
                p.to_string()
            }
        }
        _ => path.clone(),
    };
    let hr = super::shell_folder_com::folder_for(&parent, riid.cast(), out);
    if hr >= 0 && !last.is_null() {
        // Dernier élément non vide.
        let mut item = pidl;
        let mut found = pidl;
        for _ in 0..64 {
            let cb = std::ptr::read_unaligned(item as *const u16) as usize;
            if cb == 0 {
                break;
            }
            found = item;
            item = item.add(cb);
        }
        *last = found;
    }
    hr
}

// ── IShellItem et IShellItemArray ───────────────────────────────────────

unsafe extern "win64" fn sh_create_item_from_id_list(
    pidl: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match path_from_pidl(pidl) {
        Some(p) => item_for(&p, riid, out),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn sh_create_item_with_parent(
    parent: *const u8,
    _folder: usize,
    pidl: *const u8,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match pidl_path(parent, pidl) {
        Some(p) => item_for(&p, riid, out),
        None => E_INVALIDARG,
    }
}

const IID_ISHELLITEM: Guid = guid(
    0x43826D1E,
    0xE718,
    0x42EE,
    [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE],
);

/// SHCreateShellItem : l'élément `pidl`, relatif à `parent` (un
/// identifiant ou un IShellFolder de Winlux).
unsafe extern "win64" fn sh_create_shell_item(
    parent: *const u8,
    folder: usize,
    pidl: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let path = pidl_path(parent, pidl).or_else(|| super::shell_folder_com::folder_path(folder));
    match path {
        Some(p) => item_for(&p, IID_ISHELLITEM.as_ptr(), out),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn sh_create_item_from_relative_name(
    parent: usize,
    name: *const u16,
    _bc: usize,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let Some(dir) = super::filedialog_com::item_path(parent) else {
        return E_INVALIDARG;
    };
    item_for(&join(&dir, &wstr(name)), riid, out)
}

unsafe extern "win64" fn sh_create_item_in_known_folder(
    id: *const u8,
    flags: u32,
    name: *const u16,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let Some(g) = super::knownfolders::guid_text(id) else {
        return E_INVALIDARG;
    };
    let Some(dir) = super::knownfolders::known_folder_path(&g, flags) else {
        return HR_FILE_NOT_FOUND;
    };
    item_for(&join(&dir, &wstr(name)), riid, out)
}

const IID_ISHELLITEMARRAY: Guid = guid(
    0xB63E_A76D,
    0x1F85,
    0x456F,
    [0xA1, 0x9C, 0x48, 0x15, 0x9E, 0xFA, 0x85, 0x8B],
);

unsafe fn array_of(paths: Vec<String>, riid: *const u8, out: *mut *mut c_void) -> i32 {
    super::filedialog_com::new_item_array(paths, riid.cast(), out)
}

unsafe fn pidl_list(count: u32, list: *const *const u8, parent: *const u8) -> Option<Vec<String>> {
    if count > 0 && list.is_null() {
        return None;
    }
    (0..count as usize)
        .map(|i| pidl_path(parent, *list.add(i)))
        .collect()
}

unsafe extern "win64" fn sh_create_shell_item_array(
    parent: *const u8,
    folder: usize,
    count: u32,
    list: *const *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let parent_path =
        path_from_pidl(parent).or_else(|| super::shell_folder_com::folder_path(folder));
    let mut paths = Vec::new();
    for i in 0..count as usize {
        if list.is_null() {
            return E_INVALIDARG;
        }
        match path_from_pidl(*list.add(i)).or_else(|| parent_path.clone()) {
            Some(p) => paths.push(p),
            None => return E_INVALIDARG,
        }
    }
    array_of(paths, IID_ISHELLITEMARRAY.as_ptr(), out)
}

unsafe extern "win64" fn sh_create_shell_item_array_from_id_lists(
    count: u32,
    list: *const *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match pidl_list(count, list, std::ptr::null()) {
        Some(paths) => array_of(paths, IID_ISHELLITEMARRAY.as_ptr(), out),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn sh_create_shell_item_array_from_shell_item(
    item: usize,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match super::filedialog_com::item_path(item) {
        Some(p) => array_of(vec![p], riid, out),
        None => E_INVALIDARG,
    }
}

/// Fichiers d'un IDataObject (format CF_HDROP).
unsafe fn data_object_files(obj: usize) -> Vec<String> {
    super::dataobject::read_data_object(obj)
        .into_iter()
        .find(|(f, _)| *f == super::clipboard::CF_HDROP)
        .map(|(_, b)| crate::clipbridge::hdrop_parse(&b))
        .unwrap_or_default()
}

unsafe extern "win64" fn sh_create_shell_item_array_from_data_object(
    obj: usize,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let files = data_object_files(obj);
    if files.is_empty() {
        return E_INVALIDARG;
    }
    array_of(files, riid, out)
}

/// SHCreateDataObject : objet de données CF_HDROP des éléments donnés.
unsafe extern "win64" fn sh_create_data_object(
    folder: *const u8,
    count: u32,
    list: *const *const u8,
    _inner: usize,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let Some(paths) = pidl_list(count, list, folder) else {
        return E_INVALIDARG;
    };
    let mut contents = super::clipboard::Contents::default();
    if !paths.is_empty() {
        contents.formats.push((
            super::clipboard::CF_HDROP,
            crate::clipbridge::hdrop_build(&paths),
        ));
    }
    let obj = super::dataobject::new_data_object(contents);
    let vtbl = *(obj as *const *const usize);
    let qi: unsafe extern "win64" fn(usize, *const u8, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtbl);
    let release: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtbl.add(2));
    let hr = qi(obj, riid, out);
    release(obj);
    hr
}

// ── Propriétés ──────────────────────────────────────────────────────────

/// PROPERTYKEY (20 octets) et PROPVARIANT (24 octets en 64 bits).
type PropKey = [u8; 20];
type PropVar = [u8; 24];

fn window_props() -> &'static Mutex<HashMap<usize, Vec<(PropKey, PropVar)>>> {
    static M: std::sync::OnceLock<Mutex<HashMap<usize, Vec<(PropKey, PropVar)>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

struct Props {
    /// Fenêtre dont les propriétés sont partagées (0 : magasin local).
    window: usize,
    local: Vec<(PropKey, PropVar)>,
    writable: bool,
}

impl Drop for Props {
    fn drop(&mut self) {
        for (_, v) in &mut self.local {
            unsafe { clear_var(v) };
        }
    }
}

unsafe fn clear_var(v: &mut PropVar) {
    api!(
        "ole32",
        "PropVariantClear",
        fn(*mut u8) -> i32,
        v.as_mut_ptr()
    );
}

unsafe fn with_props<R>(p: &mut Props, f: impl FnOnce(&mut Vec<(PropKey, PropVar)>) -> R) -> R {
    if p.window == 0 {
        f(&mut p.local)
    } else {
        let mut map = window_props().lock().unwrap_or_else(|e| e.into_inner());
        f(map.entry(p.window).or_default())
    }
}

unsafe extern "win64" fn ps_qi(
    this: *mut Com<Props>,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match read_iid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IPROPERTYSTORE => {
            Com::add_ref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => E_NOINTERFACE,
    }
}

unsafe extern "win64" fn ps_add_ref(this: *mut Com<Props>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn ps_release(this: *mut Com<Props>) -> u32 {
    Com::release(this)
}

unsafe extern "win64" fn ps_get_count(this: *mut Com<Props>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_props(&mut (*this).inner, |v| v.len() as u32);
    S_OK
}

unsafe extern "win64" fn ps_get_at(this: *mut Com<Props>, i: u32, key: *mut PropKey) -> i32 {
    if key.is_null() {
        return E_POINTER;
    }
    match with_props(&mut (*this).inner, |v| v.get(i as usize).map(|e| e.0)) {
        Some(k) => {
            std::ptr::write_unaligned(key, k);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn ps_get_value(
    this: *mut Com<Props>,
    key: *const PropKey,
    out: *mut u8,
) -> i32 {
    if key.is_null() || out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_bytes(out, 0, 24);
    let k = std::ptr::read_unaligned(key);
    with_props(&mut (*this).inner, |v| match v.iter().find(|e| e.0 == k) {
        Some(e) => super::wpf_native::prop_variant_copy(out, e.1.as_ptr()),
        None => S_OK, // VT_EMPTY
    })
}

unsafe extern "win64" fn ps_set_value(
    this: *mut Com<Props>,
    key: *const PropKey,
    val: *const u8,
) -> i32 {
    if key.is_null() || val.is_null() {
        return E_POINTER;
    }
    if !(*this).inner.writable {
        return STG_E_ACCESSDENIED;
    }
    let k = std::ptr::read_unaligned(key);
    let mut copy: PropVar = [0; 24];
    let hr = super::wpf_native::prop_variant_copy(copy.as_mut_ptr(), val);
    if hr < 0 {
        return hr;
    }
    with_props(&mut (*this).inner, |v| {
        if let Some(e) = v.iter_mut().find(|e| e.0 == k) {
            clear_var(&mut e.1);
            e.1 = copy;
        } else {
            v.push((k, copy));
        }
    });
    S_OK
}

unsafe extern "win64" fn ps_commit(_this: *mut Com<Props>) -> i32 {
    S_OK
}

static PROPS_VT: VTable<8> = VTable([
    ps_qi as *const (),
    ps_add_ref as *const (),
    ps_release as *const (),
    ps_get_count as *const (),
    ps_get_at as *const (),
    ps_get_value as *const (),
    ps_set_value as *const (),
    ps_commit as *const (),
]);

unsafe fn new_store(p: Props, riid: *const u8, out: *mut *mut c_void) -> i32 {
    let obj = Com::create(PROPS_VT.ptr(), p);
    let hr = ps_qi(obj, riid, out);
    Com::release(obj);
    hr
}

/// Magasin de propriétés en mémoire, vide et modifiable
/// (PSCreateMemoryPropertyStore, lot F #1615).
pub(crate) unsafe fn memory_property_store(riid: *const u8, out: *mut *mut c_void) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    new_store(
        Props {
            window: 0,
            local: Vec::new(),
            writable: true,
        },
        riid,
        out,
    )
}

/// SHGetPropertyStoreForWindow : propriétés de la fenêtre (Qt y pose
/// System.AppUserModel.ID), gardées tant que le programme tourne.
unsafe extern "win64" fn sh_get_property_store_for_window(
    hwnd: usize,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    if api!("user32", "IsWindow", fn(usize) -> i32, hwnd).unwrap_or(0) == 0 {
        return E_INVALIDARG;
    }
    new_store(
        Props {
            window: hwnd,
            local: Vec::new(),
            writable: true,
        },
        riid,
        out,
    )
}

fn key(fmtid: &Guid, pid: u32) -> PropKey {
    let mut k = [0u8; 20];
    k[..16].copy_from_slice(fmtid);
    k[16..].copy_from_slice(&pid.to_le_bytes());
    k
}

/// PROPVARIANT VT_UI8 / VT_FILETIME.
fn var_u64(vt: u16, v: u64) -> PropVar {
    let mut p = [0u8; 24];
    p[..2].copy_from_slice(&vt.to_le_bytes());
    p[8..16].copy_from_slice(&v.to_le_bytes());
    p
}

/// Propriétés de base d'un fichier : nom affiché, taille, date de
/// modification (celles que l'Explorateur montre).
pub fn file_properties(path: &str) -> Option<Vec<(PropKey, u16, Result<u64, String>)>> {
    let meta = std::fs::metadata(crate::paths::to_linux(path)).ok()?;
    let name = super::filedialog_com::display_name(path, 0);
    let mut out = vec![(key(&FMTID_STORAGE, 10), 31u16, Err(name))];
    if meta.is_file() {
        out.push((key(&FMTID_STORAGE, 12), 21, Ok(meta.len()))); // VT_UI8
    }
    if let Ok(t) = meta.modified() {
        let since = t
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64 / 100)
            .unwrap_or(0);
        out.push((
            key(&FMTID_STORAGE, 14),
            64,
            Ok(since + 116_444_736_000_000_000),
        )); // VT_FILETIME
    }
    Some(out)
}

unsafe extern "win64" fn sh_get_property_store_from_parsing_name(
    path: *const u16,
    _bc: usize,
    flags: u32,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let p = crate::paths::full_windows(&wstr(path));
    let Some(props) = file_properties(&p) else {
        return HR_FILE_NOT_FOUND;
    };
    let mut local = Vec::new();
    for (k, vt, v) in props {
        let var = match v {
            Ok(n) => var_u64(vt, n),
            Err(text) => {
                let mut var = [0u8; 24];
                let mut s: *mut u16 = std::ptr::null_mut();
                if co_string(&text, &mut s) < 0 {
                    continue;
                }
                var[..2].copy_from_slice(&vt.to_le_bytes());
                var[8..16].copy_from_slice(&(s as usize).to_le_bytes());
                var
            }
        };
        local.push((k, var));
    }
    // GPS_READWRITE (2) : les propriétés de base ne s'écrivent pas.
    new_store(
        Props {
            window: 0,
            local,
            writable: flags & 2 != 0,
        },
        riid,
        out,
    )
}

/// SHSetDefaultProperties : Winlux ne définit aucune propriété par défaut
/// pour les nouveaux fichiers ; il n'y a rien à poser.
extern "win64" fn sh_set_default_properties(
    _hwnd: usize,
    _item: usize,
    _flags: u32,
    _ops: usize,
) -> i32 {
    S_OK
}

// ── IQueryAssociations ──────────────────────────────────────────────────

struct Assoc {
    class: Vec<u16>,
}

unsafe extern "win64" fn qa_qi(
    this: *mut Com<Assoc>,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    match read_iid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IQUERYASSOCIATIONS => {
            Com::add_ref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => E_NOINTERFACE,
    }
}

unsafe extern "win64" fn qa_add_ref(this: *mut Com<Assoc>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn qa_release(this: *mut Com<Assoc>) -> u32 {
    Com::release(this)
}

unsafe extern "win64" fn qa_init(
    this: *mut Com<Assoc>,
    _flags: u32,
    assoc: *const u16,
    _key: usize,
    _hwnd: usize,
) -> i32 {
    if assoc.is_null() {
        return E_INVALIDARG;
    }
    (*this).inner.class = wstr(assoc).encode_utf16().chain([0]).collect();
    S_OK
}

unsafe extern "win64" fn qa_get_string(
    this: *mut Com<Assoc>,
    flags: u32,
    kind: u32,
    extra: *const u16,
    out: *mut u16,
    cch: *mut u32,
) -> i32 {
    super::shlwapi_reg::assoc_query_string_w(
        flags,
        kind,
        (*this).inner.class.as_ptr(),
        extra,
        out,
        cch,
    )
}

unsafe extern "win64" fn qa_get_key(
    this: *mut Com<Assoc>,
    flags: u32,
    kind: u32,
    extra: *const u16,
    out: *mut usize,
) -> i32 {
    super::shlwapi_reg::assoc_query_key_w(flags, kind, (*this).inner.class.as_ptr(), extra, out)
}

unsafe extern "win64" fn qa_not_impl(_this: *mut Com<Assoc>) -> i32 {
    E_NOTIMPL
}

static ASSOC_VT: VTable<8> = VTable([
    qa_qi as *const (),
    qa_add_ref as *const (),
    qa_release as *const (),
    qa_init as *const (),
    qa_get_string as *const (),
    qa_get_key as *const (),
    qa_not_impl as *const (), // GetData
    qa_not_impl as *const (), // GetEnum
]);

/// AssocCreateForClasses : IQueryAssociations sur la première classe
/// nommée (extension, ProgID ou « * »), au-dessus d'AssocQueryString.
/// Objet IQueryAssociations neuf, sans classe (AssocCreate de shlwapi,
/// lot E, #1614) : IQueryAssociations::Init la fixe ensuite.
pub(crate) unsafe fn assoc_create_empty(riid: *const u8, out: *mut *mut c_void) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    let obj = Com::create(
        ASSOC_VT.ptr(),
        Assoc {
            class: "*".encode_utf16().chain([0]).collect(),
        },
    );
    let hr = qa_qi(obj, riid, out);
    Com::release(obj);
    hr
}

unsafe extern "win64" fn assoc_create_for_classes(
    list: *const u8,
    count: u32,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !clear_out(out) {
        return E_POINTER;
    }
    if list.is_null() || count == 0 {
        return E_INVALIDARG;
    }
    // ASSOCIATIONELEMENT : ac, hkClass, pszClass (24 octets).
    let class = (0..count as usize)
        .map(|i| std::ptr::read_unaligned(list.add(i * 24 + 16) as *const *const u16))
        .find(|p| !p.is_null())
        .map(|p| wstr(p))
        .unwrap_or_else(|| "*".into());
    let obj = Com::create(
        ASSOC_VT.ptr(),
        Assoc {
            class: class.encode_utf16().chain([0]).collect(),
        },
    );
    let hr = qa_qi(obj, riid, out);
    Com::release(obj);
    hr
}

// ── Corbeille ───────────────────────────────────────────────────────────

fn trash_dir() -> std::path::PathBuf {
    crate::paths::home().join(".local/share/Trash")
}

fn tree_size(p: &std::path::Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| tree_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Éléments et taille de la corbeille freedesktop (celle de
/// l'Explorateur).
pub fn trash_usage(trash: &std::path::Path) -> (u64, u64) {
    let files = trash.join("files");
    let Ok(rd) = std::fs::read_dir(&files) else {
        return (0, 0);
    };
    let mut count = 0;
    let mut size = 0;
    for e in rd.flatten() {
        count += 1;
        size += tree_size(&e.path());
    }
    (size, count)
}

/// SHQueryRecycleBinW : SHQUERYRBINFO, 24 octets en 64 bits (i64Size à 8,
/// i64NumItems à 16) ; 20 octets en 32 bits, où shellapi.h est compacté
/// par `pshpack1.h` (i64Size à 4, i64NumItems à 12, #1841).
pub(super) unsafe extern "win64" fn sh_query_recycle_bin_w(
    _root: *const u16,
    info: *mut u8,
) -> i32 {
    let (need, size_at, count_at) = if crate::wow32::pointer_size() == 4 {
        (20, 4, 12)
    } else {
        (24, 8, 16)
    };
    if info.is_null() || std::ptr::read_unaligned(info as *const u32) < need {
        return E_INVALIDARG;
    }
    let (size, count) = trash_usage(&trash_dir());
    std::ptr::write_unaligned(info.add(size_at) as *mut u64, size);
    std::ptr::write_unaligned(info.add(count_at) as *mut u64, count);
    S_OK
}

/// SHEmptyRecycleBinW : vide la corbeille. Sans SHERB_NOCONFIRMATION,
/// l'utilisateur confirme d'abord (Non par défaut).
pub(super) unsafe extern "win64" fn sh_empty_recycle_bin_w(
    hwnd: usize,
    _root: *const u16,
    flags: u32,
) -> i32 {
    let trash = trash_dir();
    let (_, count) = trash_usage(&trash);
    if count == 0 {
        return S_OK;
    }
    if flags & 1 == 0 {
        let tr = super::comdlg32_ui::tr;
        let text = tr(
            "Supprimer définitivement tous les éléments de la Corbeille ?",
            "Permanently delete all items in the Recycle Bin?",
            "Alle Elemente im Papierkorb endgültig löschen?",
            "¿Eliminar permanentemente todos los elementos de la Papelera de reciclaje?",
        );
        let title = tr(
            "Vider la Corbeille",
            "Empty Recycle Bin",
            "Papierkorb leeren",
            "Vaciar Papelera de reciclaje",
        );
        let t: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let c: Vec<u16> = title.encode_utf16().chain([0]).collect();
        // MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2
        let answer = api!(
            "user32",
            "MessageBoxW",
            fn(usize, *const u16, *const u16, u32) -> i32,
            hwnd,
            t.as_ptr(),
            c.as_ptr(),
            0x4 | 0x30 | 0x100
        )
        .unwrap_or(7);
        if answer != 6 {
            return HR_CANCELLED;
        }
    }
    let mut ok = true;
    for sub in ["files", "info"] {
        if let Ok(rd) = std::fs::read_dir(trash.join(sub)) {
            for e in rd.flatten() {
                let p = e.path();
                let r = if p.is_dir() && !p.is_symlink() {
                    std::fs::remove_dir_all(&p)
                } else {
                    std::fs::remove_file(&p)
                };
                ok &= r.is_ok();
            }
        }
    }
    if ok {
        S_OK
    } else {
        E_FAIL
    }
}

// ── Divers ──────────────────────────────────────────────────────────────

/// SHGetSettings : SHELLFLAGSTATE, réglages de l'Explorateur Winlux
/// (extensions affichées, fichiers cachés masqués, double-clic, info-bulles).
unsafe extern "win64" fn sh_get_settings(out: *mut u32, mask: u32) {
    if out.is_null() {
        return;
    }
    // Bits de SHELLFLAGSTATE dans l'ordre de la structure, et le masque
    // SSF_* qui les demande.
    const BITS: [(u32, bool); 15] = [
        (0x0001, false),      // fShowAllObjects
        (0x0002, true),       // fShowExtensions
        (0x8000, false),      // fNoConfirmRecycle
        (0x0020, false),      // fShowSysFiles
        (0x0008, false),      // fShowCompColor
        (0x0080, true),       // fDoubleClickInWebView
        (0x0200, false),      // fDesktopHTML
        (0x0400, false),      // fWin95Classic
        (0x0800, false),      // fDontPrettyPath
        (0x0100, false),      // fShowAttribCol
        (0x1000, false),      // fMapNetDrvBtn
        (0x2000, true),       // fShowInfoTip
        (0x4000, false),      // fHideIcons
        (0x0080_0000, false), // fAutoCheckSelect
        (0x0100_0000, false), // fIconsOnly
    ];
    let mut v = 0u32;
    for (i, (ssf, on)) in BITS.iter().enumerate() {
        if mask & ssf != 0 && *on {
            v |= 1 << i;
        }
    }
    std::ptr::write_unaligned(out, v);
}

/// SHGetInstanceExplorer : rend E_FAIL hors du processus de l'Explorateur,
/// ce qui est toujours le cas d'un programme.
unsafe extern "win64" fn sh_get_instance_explorer(out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_FAIL
}

/// SHUpdateImageW (ordinal 192) : Winlux n'a pas de cache d'icônes
/// système à invalider.
extern "win64" fn sh_update_image_w(_hash: *const u16, _index: i32, _flags: u32, _image: i32) {}

/// SHSetFolderPathW (ordinal 232) : les dossiers de l'utilisateur sont
/// ceux de la session Linux (xdg-user-dirs) ; un programme ne les déplace
/// pas.
extern "win64" fn sh_set_folder_path_w(
    _csidl: i32,
    _token: usize,
    _flags: u32,
    _path: *const u16,
) -> i32 {
    HR_NOT_SUPPORTED
}

/// Vues de dossier, menus contextuels et extracteurs d'icônes par défaut
/// appartiennent à l'Explorateur Winlux, qui n'est pas chargé dans le
/// programme.
unsafe extern "win64" fn not_in_process(_a: usize, _b: usize, out: *mut *mut c_void) -> i32 {
    clear_out(out);
    E_NOTIMPL
}

/// Même refus pour les fonctions à deux arguments, `SHCreateShellFolderView`
/// et `SHCreateDefaultExtractIcon` (shlobj_core.h), dont la sortie est le
/// deuxième argument (#2453).
unsafe extern "win64" fn not_in_process2(_a: usize, out: *mut *mut c_void) -> i32 {
    clear_out(out);
    E_NOTIMPL
}

pub fn exports() -> Exports {
    let mut m = crate::exports! {
        "SHParseDisplayName" => fn sh_parse_display_name,
        "SHILCreateFromPath" => fn sh_il_create_from_path,
        "SHGetFolderLocation" => fn sh_get_folder_location,
        "SHGetNameFromIDList" => fn sh_get_name_from_id_list,
        "SHGetIDListFromObject" => fn sh_get_id_list_from_object,
        "SHBindToParent" => fn sh_bind_to_parent,
        "SHCreateItemFromIDList" => fn sh_create_item_from_id_list,
        "SHCreateItemWithParent" => fn sh_create_item_with_parent,
        "SHCreateShellItem" => fn sh_create_shell_item,
        "SHCreateItemFromRelativeName" => fn sh_create_item_from_relative_name,
        "SHCreateItemInKnownFolder" => fn sh_create_item_in_known_folder,
        "SHCreateShellItemArray" => fn sh_create_shell_item_array,
        "SHCreateShellItemArrayFromIDLists" => fn sh_create_shell_item_array_from_id_lists,
        "SHCreateShellItemArrayFromShellItem" => fn sh_create_shell_item_array_from_shell_item,
        "SHCreateShellItemArrayFromDataObject" => fn sh_create_shell_item_array_from_data_object,
        "SHCreateDataObject" => fn sh_create_data_object,
        "SHGetPropertyStoreForWindow" => fn sh_get_property_store_for_window,
        "SHGetPropertyStoreFromParsingName" => fn sh_get_property_store_from_parsing_name,
        "SHSetDefaultProperties" => fn sh_set_default_properties,
        "AssocCreateForClasses" => fn assoc_create_for_classes,
        "SHQueryRecycleBinW" => fn sh_query_recycle_bin_w,
        "SHEmptyRecycleBinW" => fn sh_empty_recycle_bin_w,
        "SHGetSettings" => fn sh_get_settings,
        "SHGetInstanceExplorer" => fn sh_get_instance_explorer,
        "SHUpdateImageW" => fn sh_update_image_w,
        "SHSetFolderPathW" => fn sh_set_folder_path_w,
        "SHCreateShellFolderView" => fn not_in_process2,
        "SHCreateDefaultContextMenu" => fn not_in_process,
        "SHCreateDefaultExtractIcon" => fn not_in_process2,
    };
    // Variantes ANSI qui manquaient (SHFileOperationA de l'ODT, #1841).
    for (k, v) in super::shell32_ansi::exports() {
        m.entry(k).or_insert(v);
    }
    // SHRestricted, aussi par l'ordinal 100 (#1750).
    for (k, v) in super::shell_ordinaux::shell32() {
        m.entry(k).or_insert(v);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refus_hors_processus_met_la_bonne_sortie_a_zero() {
        // #2453 : deux arguments pour la vue de dossier et l'extracteur
        // d'icônes, trois pour le menu contextuel (shlobj_core.h).
        let m = exports();
        for name in ["SHCreateShellFolderView", "SHCreateDefaultExtractIcon"] {
            let f: unsafe extern "win64" fn(usize, *mut *mut c_void) -> i32 =
                unsafe { std::mem::transmute(m[name]) };
            let mut out = 0x1234usize as *mut c_void;
            assert_eq!(unsafe { f(0, &mut out) }, E_NOTIMPL, "{name}");
            assert!(out.is_null(), "{name}");
        }
        let f: unsafe extern "win64" fn(usize, usize, *mut *mut c_void) -> i32 =
            unsafe { std::mem::transmute(m["SHCreateDefaultContextMenu"]) };
        let mut out = 0x1234usize as *mut c_void;
        assert_eq!(unsafe { f(0, 0, &mut out) }, E_NOTIMPL);
        assert!(out.is_null());
    }

    #[test]
    fn corbeille_comptee() {
        let dir = std::env::temp_dir().join(format!("winlux-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files/dossier")).unwrap();
        std::fs::write(dir.join("files/a.txt"), b"12345").unwrap();
        std::fs::write(dir.join("files/dossier/b.bin"), [0u8; 10]).unwrap();
        assert_eq!(trash_usage(&dir), (15, 2));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(trash_usage(&dir), (0, 0));
    }

    #[test]
    fn reglages_de_l_explorateur() {
        let mut v = 0u32;
        unsafe { sh_get_settings(&mut v, 0x0002 | 0x0001 | 0x2000) };
        // fShowExtensions (bit 1) et fShowInfoTip (bit 11), pas
        // fShowAllObjects.
        assert_eq!(v, (1 << 1) | (1 << 11));
    }

    #[test]
    fn cle_de_propriete() {
        let k = key(&FMTID_STORAGE, 12);
        assert_eq!(&k[..4], &0xB725_F130u32.to_le_bytes());
        assert_eq!(&k[16..], &12u32.to_le_bytes());
    }
}
