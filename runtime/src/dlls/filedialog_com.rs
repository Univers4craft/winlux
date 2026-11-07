//! Boîtes « Ouvrir » et « Enregistrer sous » de Vista (`CLSID_FileOpenDialog`,
//! `CLSID_FileSaveDialog`) : IFileDialog, IFileOpenDialog, IFileSaveDialog,
//! IFileDialogCustomize, plus IShellItem, IShellItemArray et
//! `SHCreateItemFromParsingName` (#699).
//!
//! Notepad++ 8 et la plupart des programmes récents passent par ces objets
//! COM plutôt que par `GetOpenFileName`. `Show` emprunte le même chemin que
//! comdlg32 : l'Explorateur Winlux (un seul Explorateur, #598/#652), sinon la
//! boîte intégrée (sans affichage, tests `WINLUX_FILEDLG`). Le choix d'un
//! dossier (FOS_PICKFOLDERS) ouvre l'Explorateur en mode « dossier ».
//!
//! Les contrôles ajoutés par IFileDialogCustomize (cases, listes) ne sont pas
//! affichés : leurs valeurs initiales sont rendues telles quelles, ce qui
//! donne le comportement par défaut du programme.

use super::audio_com::{
    guid, put, read_guid, Com, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::comdlg32_files::{self as files, PickRequest};
use super::ole32::Guid;
use crate::paths;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

pub const CLSID_FILE_OPEN_DIALOG: Guid = guid(
    0xDC1C_5A9C,
    0xE88A,
    0x4DDE,
    [0xA5, 0xA1, 0x60, 0xF8, 0x2A, 0x20, 0xAE, 0xF7],
);
pub const CLSID_FILE_SAVE_DIALOG: Guid = guid(
    0xC0B4_E2F3,
    0xBA21,
    0x4773,
    [0x8D, 0xBA, 0x33, 0x5E, 0xC9, 0x46, 0xEB, 0x8B],
);
const IID_IMODAL_WINDOW: Guid = guid(
    0xB4DB_1657,
    0x70D7,
    0x485E,
    [0x8E, 0x3E, 0x6F, 0xCB, 0x5A, 0x5C, 0x18, 0x02],
);
const IID_IFILE_DIALOG: Guid = guid(
    0x42F8_5136,
    0xDB7E,
    0x439C,
    [0x85, 0xF1, 0xE4, 0x07, 0x5D, 0x13, 0x5F, 0xC8],
);
const IID_IFILE_OPEN_DIALOG: Guid = guid(
    0xD57C_7288,
    0xD4AD,
    0x4768,
    [0xBE, 0x02, 0x9D, 0x96, 0x95, 0x32, 0xD9, 0x60],
);
const IID_IFILE_SAVE_DIALOG: Guid = guid(
    0x84BC_CD23,
    0x5FDE,
    0x4CDB,
    [0xAE, 0xA4, 0xAF, 0x64, 0xB8, 0x3D, 0x78, 0xAB],
);
const IID_IFILE_DIALOG_CUSTOMIZE: Guid = guid(
    0xE6FD_D21A,
    0x163F,
    0x4975,
    [0x9C, 0x8C, 0xA6, 0x9F, 0x1B, 0xA3, 0x70, 0x34],
);
const IID_ISHELL_ITEM: Guid = guid(
    0x4382_6D1E,
    0xE718,
    0x42EE,
    [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE],
);
const IID_ISHELL_ITEM_ARRAY: Guid = guid(
    0xB63E_A76D,
    0x1F85,
    0x456F,
    [0xA1, 0x9C, 0x48, 0x15, 0x9E, 0xFA, 0x85, 0x8B],
);

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;
/// HRESULT_FROM_WIN32(ERROR_CANCELLED) : l'utilisateur a annulé.
pub const HR_CANCELLED: i32 = 0x8007_04C7u32 as i32;
const HR_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;

pub const FOS_OVERWRITEPROMPT: u32 = 0x2;
pub const FOS_PICKFOLDERS: u32 = 0x20;
pub const FOS_ALLOWMULTISELECT: u32 = 0x200;
pub const FOS_PATHMUSTEXIST: u32 = 0x800;
pub const FOS_FILEMUSTEXIST: u32 = 0x1000;
pub const FOS_CREATEPROMPT: u32 = 0x2000;
/// Bits de FILEOPENDIALOGOPTIONS identiques à ceux d'OPENFILENAME.
const FOS_SAME_AS_OFN: u32 = FOS_OVERWRITEPROMPT
    | FOS_ALLOWMULTISELECT
    | FOS_PATHMUSTEXIST
    | FOS_FILEMUSTEXIST
    | FOS_CREATEPROMPT;

const SIGDN_NORMALDISPLAY: u32 = 0;
const SIGDN_PARENTRELATIVEPARSING: u32 = 0x8001_8001;
const SIGDN_PARENTRELATIVEFORADDRESSBAR: u32 = 0x8001_C001;
const SIGDN_PARENTRELATIVEEDITING: u32 = 0x8003_1001;
const SIGDN_PARENTRELATIVE: u32 = 0x8008_0001;

// ---------- État d'une boîte ----------

#[derive(Default)]
pub(crate) struct DialogState {
    pub save: bool,
    pub options: u32,
    pub title: Option<String>,
    pub filters: Vec<(String, String)>,
    /// Type choisi, à partir de 1 (0 : aucun).
    pub filter_index: u32,
    pub folder: Option<String>,
    pub default_folder: Option<String>,
    pub file_name: String,
    pub default_ext: Option<String>,
    pub results: Vec<String>,
    /// Cases et listes d'IFileDialogCustomize : (id → état).
    pub checks: HashMap<u32, bool>,
    pub selected: HashMap<u32, u32>,
    pub edits: HashMap<u32, String>,
}

type Shared = Arc<Mutex<DialogState>>;

pub(crate) struct Dialog {
    state: Shared,
}

struct Customize {
    dialog: *mut Com<Dialog>,
}

pub(crate) struct ShellItem {
    /// Chemin Windows (C:\Users\…).
    pub path: String,
}

struct ItemArray {
    items: Vec<String>,
}

fn lock(s: &Shared) -> std::sync::MutexGuard<'_, DialogState> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn co_string(s: &str, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let p = super::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if p.is_null() {
        *out = std::ptr::null_mut();
        return 0x8007_000Eu32 as i32;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    S_OK
}

unsafe fn opt_w(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| super::wstr(p))
}

/// Nouvel IShellItem pour un chemin Windows.
pub(crate) fn new_item(path: &str) -> *mut Com<ShellItem> {
    let mut path = path.replace('/', "\\");
    while path.len() > 3 && path.ends_with('\\') {
        path.pop();
    }
    Com::create(ITEM_VT.ptr(), ShellItem { path })
}

/// Chemin Windows d'un IShellItem venu du programme (le nôtre, ou un autre
/// objet qui sait rendre SIGDN_FILESYSPATH).
unsafe fn item_path(item: usize) -> Option<String> {
    if item == 0 {
        return None;
    }
    let vtbl = *(item as *const *const usize);
    if vtbl as *const () == ITEM_VT.ptr() {
        return Some((*(item as *mut Com<ShellItem>)).inner.path.clone());
    }
    let get: unsafe extern "win64" fn(usize, u32, *mut *mut u16) -> i32 =
        std::mem::transmute(*vtbl.add(5));
    let mut p: *mut u16 = std::ptr::null_mut();
    if get(item, SIGDN_FILESYSPATH, &mut p) < 0 || p.is_null() {
        return None;
    }
    let s = super::wstr(p);
    super::ole32::co_task_mem_free(p as *mut c_void);
    Some(s)
}

const SIGDN_FILESYSPATH: u32 = 0x8005_8000;

// ---------- Création ----------

/// Classes de boîtes de fichiers créées par CoCreateInstance.
pub unsafe fn create_builtin(clsid: &Guid, riid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    let save = if *clsid == CLSID_FILE_OPEN_DIALOG {
        false
    } else if *clsid == CLSID_FILE_SAVE_DIALOG {
        true
    } else {
        return None;
    };
    let state = Arc::new(Mutex::new(DialogState {
        save,
        options: if save {
            FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST
        } else {
            FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST
        },
        ..Default::default()
    }));
    let d = Com::create(DIALOG_VT.ptr(), Dialog { state });
    let hr = dialog_qi(d, riid, out);
    Com::release(d);
    Some(hr)
}

// ---------- IFileDialog / IFileOpenDialog / IFileSaveDialog ----------

unsafe extern "win64" fn dialog_qi(
    this: *mut Com<Dialog>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(g) = read_guid(riid) else {
        return E_INVALIDARG;
    };
    let save = lock(&(*this).inner.state).save;
    if g == IID_IUNKNOWN
        || g == IID_IMODAL_WINDOW
        || g == IID_IFILE_DIALOG
        || (!save && g == IID_IFILE_OPEN_DIALOG)
        || (save && g == IID_IFILE_SAVE_DIALOG)
    {
        Com::add_ref(this);
        return put(out, this);
    }
    if g == IID_IFILE_DIALOG_CUSTOMIZE {
        Com::add_ref(this);
        let c = Com::create(CUSTOM_VT.ptr(), Customize { dialog: this });
        return put(out, c);
    }
    E_NOINTERFACE
}

unsafe extern "win64" fn dialog_add_ref(this: *mut Com<Dialog>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn dialog_release(this: *mut Com<Dialog>) -> u32 {
    Com::release(this)
}

/// IModalWindow::Show : ouvre la boîte et garde le choix.
unsafe extern "win64" fn dialog_show(this: *mut Com<Dialog>, owner: usize) -> i32 {
    let state = (*this).inner.state.clone();
    let spec = lock(&state).clone_spec();
    match show(owner, &spec) {
        Ok((paths, filter)) => {
            let mut s = lock(&state);
            if let Some(first) = paths.first() {
                s.file_name = first.rsplit('\\').next().unwrap_or(first).to_string();
            }
            if !s.filters.is_empty() {
                s.filter_index = filter as u32 + 1;
            }
            s.results = paths;
            S_OK
        }
        Err(hr) => hr,
    }
}

impl DialogState {
    fn clone_spec(&self) -> DialogState {
        DialogState {
            save: self.save,
            options: self.options,
            title: self.title.clone(),
            filters: self.filters.clone(),
            filter_index: self.filter_index,
            folder: self.folder.clone(),
            default_folder: self.default_folder.clone(),
            file_name: self.file_name.clone(),
            default_ext: self.default_ext.clone(),
            ..Default::default()
        }
    }
}

/// Montre la boîte décrite par `s`. Rend les chemins Windows choisis et le
/// type (à partir de 0), ou le HRESULT d'annulation.
pub(crate) unsafe fn show(owner: usize, s: &DialogState) -> Result<(Vec<String>, usize), i32> {
    let folder_mode = s.options & FOS_PICKFOLDERS != 0;
    let dir = s.folder.clone().or_else(|| s.default_folder.clone());
    let title = s
        .title
        .clone()
        .unwrap_or_else(|| default_title(s.save, folder_mode));
    if folder_mode {
        return pick_folder(owner, &title, dir, &s.file_name);
    }
    // Même chemin que GetOpenFileNameW / GetSaveFileNameW : l'Explorateur
    // Winlux, ou la boîte intégrée en repli.
    let filter: Vec<u16> = s
        .filters
        .iter()
        .flat_map(|(d, p)| {
            d.encode_utf16()
                .chain([0])
                .chain(p.encode_utf16())
                .chain([0])
        })
        .chain([0, 0])
        .collect();
    const MAX: usize = 32 * 1024;
    let mut file = vec![0u16; MAX];
    for (i, c) in s.file_name.encode_utf16().take(MAX - 1).enumerate() {
        file[i] = c;
    }
    let wide = |t: &str| -> Vec<u16> { t.encode_utf16().chain([0]).collect() };
    let dir_w = dir.as_deref().map(wide);
    let title_w = wide(&title);
    let ext_w = s
        .default_ext
        .as_deref()
        .map(|e| wide(e.trim_start_matches('.')));
    let mut ofn = [0u8; 152];
    let w = |buf: &mut [u8; 152], off: usize, v: &[u8]| buf[off..off + v.len()].copy_from_slice(v);
    w(&mut ofn, 0, &152u32.to_le_bytes());
    w(&mut ofn, 8, &(owner as u64).to_le_bytes());
    if !s.filters.is_empty() {
        w(&mut ofn, 24, &(filter.as_ptr() as u64).to_le_bytes());
        w(&mut ofn, 44, &s.filter_index.max(1).to_le_bytes());
    }
    w(&mut ofn, 48, &(file.as_mut_ptr() as u64).to_le_bytes());
    w(&mut ofn, 56, &(MAX as u32).to_le_bytes());
    if let Some(d) = &dir_w {
        w(&mut ofn, 80, &(d.as_ptr() as u64).to_le_bytes());
    }
    w(&mut ofn, 88, &(title_w.as_ptr() as u64).to_le_bytes());
    // OFN_EXPLORER | options communes.
    let flags = 0x0008_0000 | (s.options & FOS_SAME_AS_OFN);
    w(&mut ofn, 96, &flags.to_le_bytes());
    if let Some(e) = &ext_w {
        w(&mut ofn, 104, &(e.as_ptr() as u64).to_le_bytes());
    }
    if files::file_dialog(ofn.as_mut_ptr(), true, s.save) == 0 {
        return Err(HR_CANCELLED);
    }
    let index = u32::from_le_bytes(ofn[44..48].try_into().unwrap_or_default()) as usize;
    let paths = split_result(&file);
    if paths.is_empty() {
        return Err(HR_CANCELLED);
    }
    Ok((paths, index.saturating_sub(1)))
}

/// `lpstrFile` rendu par comdlg32 : un chemin, ou le dossier puis les noms.
pub(crate) fn split_result(buf: &[u16]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, &c) in buf.iter().enumerate() {
        if c == 0 {
            if i == start {
                break;
            }
            parts.push(String::from_utf16_lossy(&buf[start..i]));
            start = i + 1;
        }
    }
    if parts.len() <= 1 {
        return parts;
    }
    let dir = parts[0].trim_end_matches('\\').to_string();
    parts[1..].iter().map(|n| format!("{dir}\\{n}")).collect()
}

fn default_title(save: bool, folder: bool) -> String {
    use super::comdlg32_ui::tr;
    match (folder, save) {
        (true, _) => tr(
            "Sélectionner un dossier",
            "Select Folder",
            "Ordner auswählen",
            "Seleccionar carpeta",
        ),
        (false, true) => tr(
            "Enregistrer sous",
            "Save As",
            "Speichern unter",
            "Guardar como",
        ),
        (false, false) => tr("Ouvrir", "Open", "Öffnen", "Abrir"),
    }
    .to_string()
}

/// FOS_PICKFOLDERS : l'Explorateur en mode « dossier ». Sans Explorateur,
/// `WINLUX_FILEDLG` (tests) donne le dossier, sinon la boîte est annulée.
unsafe fn pick_folder(
    owner: usize,
    title: &str,
    dir: Option<String>,
    name: &str,
) -> Result<(Vec<String>, usize), i32> {
    let start = dir
        .filter(|d| paths::to_linux(d).is_dir())
        .or_else(|| paths::known_folder("Documents").map(|d| paths::to_windows(&d)))
        .unwrap_or_else(|| paths::to_windows(&paths::home()));
    if let Some(app) = files::explorer_app() {
        let req = PickRequest {
            save: false,
            folder: true,
            flags: 0,
            title: title.to_string(),
            filters: Vec::new(),
            filter: 0,
            dir: start.clone(),
            name: name.to_string(),
            def_ext: None,
        };
        if let Some(chosen) = files::pick_with_explorer(owner, &req, &app) {
            return chosen.map(|(p, _)| (p, 0)).ok_or(HR_CANCELLED);
        }
    }
    let Ok(typed) = std::env::var("WINLUX_FILEDLG") else {
        return Err(HR_CANCELLED);
    };
    let full = files::resolve(&start, &typed);
    if paths::to_linux(&full).is_dir() {
        Ok((vec![full], 0))
    } else {
        Err(HR_CANCELLED)
    }
}

unsafe extern "win64" fn dialog_set_file_types(
    this: *mut Com<Dialog>,
    n: u32,
    specs: *const [*const u16; 2],
) -> i32 {
    if specs.is_null() && n > 0 {
        return E_INVALIDARG;
    }
    let mut v = Vec::new();
    for i in 0..n as usize {
        let [name, spec] = *specs.add(i);
        v.push((super::wstr(name), super::wstr(spec)));
    }
    let mut s = lock(&(*this).inner.state);
    if !s.filters.is_empty() {
        return E_UNEXPECTED; // Windows : une seule fois.
    }
    s.filters = v;
    if s.filter_index == 0 && !s.filters.is_empty() {
        s.filter_index = 1;
    }
    S_OK
}

unsafe extern "win64" fn dialog_set_file_type_index(this: *mut Com<Dialog>, i: u32) -> i32 {
    lock(&(*this).inner.state).filter_index = i;
    S_OK
}

unsafe extern "win64" fn dialog_get_file_type_index(this: *mut Com<Dialog>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&(*this).inner.state).filter_index;
    S_OK
}

unsafe extern "win64" fn dialog_advise(
    _this: *mut Com<Dialog>,
    events: usize,
    cookie: *mut u32,
) -> i32 {
    if events == 0 || cookie.is_null() {
        return E_INVALIDARG;
    }
    *cookie = 1;
    S_OK
}

unsafe extern "win64" fn dialog_unadvise(_this: *mut Com<Dialog>, _cookie: u32) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_set_options(this: *mut Com<Dialog>, options: u32) -> i32 {
    lock(&(*this).inner.state).options = options;
    S_OK
}

unsafe extern "win64" fn dialog_get_options(this: *mut Com<Dialog>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&(*this).inner.state).options;
    S_OK
}

unsafe extern "win64" fn dialog_set_default_folder(this: *mut Com<Dialog>, item: usize) -> i32 {
    let Some(p) = item_path(item) else {
        return E_INVALIDARG;
    };
    lock(&(*this).inner.state).default_folder = Some(p);
    S_OK
}

unsafe extern "win64" fn dialog_set_folder(this: *mut Com<Dialog>, item: usize) -> i32 {
    let Some(p) = item_path(item) else {
        return E_INVALIDARG;
    };
    lock(&(*this).inner.state).folder = Some(p);
    S_OK
}

unsafe extern "win64" fn dialog_get_folder(this: *mut Com<Dialog>, out: *mut *mut c_void) -> i32 {
    let s = lock(&(*this).inner.state);
    let dir = s
        .results
        .first()
        .and_then(|p| p.rfind('\\').map(|i| p[..i.max(3)].to_string()))
        .or_else(|| s.folder.clone())
        .or_else(|| s.default_folder.clone())
        .or_else(|| paths::known_folder("Documents").map(|d| paths::to_windows(&d)));
    drop(s);
    match dir {
        Some(d) => put(out, new_item(&d)),
        None => E_FAIL,
    }
}

unsafe extern "win64" fn dialog_get_current_selection(
    this: *mut Com<Dialog>,
    out: *mut *mut c_void,
) -> i32 {
    dialog_get_result(this, out)
}

unsafe extern "win64" fn dialog_set_file_name(this: *mut Com<Dialog>, name: *const u16) -> i32 {
    lock(&(*this).inner.state).file_name = opt_w(name).unwrap_or_default();
    S_OK
}

unsafe extern "win64" fn dialog_get_file_name(this: *mut Com<Dialog>, out: *mut *mut u16) -> i32 {
    let name = lock(&(*this).inner.state).file_name.clone();
    co_string(&name, out)
}

unsafe extern "win64" fn dialog_set_title(this: *mut Com<Dialog>, title: *const u16) -> i32 {
    lock(&(*this).inner.state).title = opt_w(title).filter(|t| !t.is_empty());
    S_OK
}

unsafe extern "win64" fn dialog_set_label(_this: *mut Com<Dialog>, _label: *const u16) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_get_result(this: *mut Com<Dialog>, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let first = lock(&(*this).inner.state).results.first().cloned();
    match first {
        Some(p) => put(out, new_item(&p)),
        None => E_UNEXPECTED,
    }
}

unsafe extern "win64" fn dialog_add_place(
    _this: *mut Com<Dialog>,
    _item: usize,
    _where: u32,
) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_set_default_extension(
    this: *mut Com<Dialog>,
    ext: *const u16,
) -> i32 {
    lock(&(*this).inner.state).default_ext = opt_w(ext).filter(|e| !e.is_empty());
    S_OK
}

unsafe extern "win64" fn dialog_close(_this: *mut Com<Dialog>, _hr: i32) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_set_client_guid(_this: *mut Com<Dialog>, _g: *const Guid) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_clear_client_data(_this: *mut Com<Dialog>) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_set_filter(_this: *mut Com<Dialog>, _filter: usize) -> i32 {
    S_OK
}

/// IFileOpenDialog::GetResults / GetSelectedItems ;
/// IFileSaveDialog::SetSaveAsItem pour la boîte Enregistrer (même place).
unsafe extern "win64" fn dialog_slot27(this: *mut Com<Dialog>, arg: usize) -> i32 {
    let save = lock(&(*this).inner.state).save;
    if save {
        // SetSaveAsItem(IShellItem*) : dossier et nom proposés.
        if let Some(p) = item_path(arg) {
            let mut s = lock(&(*this).inner.state);
            if let Some(i) = p.rfind('\\') {
                s.folder = Some(p[..i.max(3)].to_string());
                s.file_name = p[i + 1..].to_string();
            }
        }
        return S_OK;
    }
    let out = arg as *mut *mut c_void;
    if out.is_null() {
        return E_POINTER;
    }
    let items = lock(&(*this).inner.state).results.clone();
    if items.is_empty() {
        *out = std::ptr::null_mut();
        return E_UNEXPECTED;
    }
    put(out, Com::create(ARRAY_VT.ptr(), ItemArray { items }))
}

unsafe extern "win64" fn dialog_slot28(this: *mut Com<Dialog>, arg: usize) -> i32 {
    if lock(&(*this).inner.state).save {
        return S_OK; // SetProperties
    }
    dialog_slot27(this, arg) // GetSelectedItems
}

unsafe extern "win64" fn dialog_set_collected_properties(
    _this: *mut Com<Dialog>,
    _list: usize,
    _append: i32,
) -> i32 {
    S_OK
}

unsafe extern "win64" fn dialog_get_properties(
    _this: *mut Com<Dialog>,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn dialog_apply_properties(
    _this: *mut Com<Dialog>,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
) -> i32 {
    E_NOTIMPL
}

static DIALOG_VT: VTable<32> = VTable([
    dialog_qi as *const (),
    dialog_add_ref as *const (),
    dialog_release as *const (),
    dialog_show as *const (),
    dialog_set_file_types as *const (),
    dialog_set_file_type_index as *const (),
    dialog_get_file_type_index as *const (),
    dialog_advise as *const (),
    dialog_unadvise as *const (),
    dialog_set_options as *const (),
    dialog_get_options as *const (),
    dialog_set_default_folder as *const (),
    dialog_set_folder as *const (),
    dialog_get_folder as *const (),
    dialog_get_current_selection as *const (),
    dialog_set_file_name as *const (),
    dialog_get_file_name as *const (),
    dialog_set_title as *const (),
    dialog_set_label as *const (), // SetOkButtonLabel
    dialog_set_label as *const (), // SetFileNameLabel
    dialog_get_result as *const (),
    dialog_add_place as *const (),
    dialog_set_default_extension as *const (),
    dialog_close as *const (),
    dialog_set_client_guid as *const (),
    dialog_clear_client_data as *const (),
    dialog_set_filter as *const (),
    dialog_slot27 as *const (),
    dialog_slot28 as *const (),
    dialog_set_collected_properties as *const (),
    dialog_get_properties as *const (),
    dialog_apply_properties as *const (),
]);

// ---------- IFileDialogCustomize ----------

unsafe fn custom_state(this: *mut Com<Customize>) -> Shared {
    (*(*this).inner.dialog).inner.state.clone()
}

unsafe extern "win64" fn custom_qi(
    this: *mut Com<Customize>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    match read_guid(riid) {
        Some(g) if g == IID_IFILE_DIALOG_CUSTOMIZE => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => dialog_qi((*this).inner.dialog, riid, out),
    }
}

unsafe extern "win64" fn custom_add_ref(this: *mut Com<Customize>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn custom_release(this: *mut Com<Customize>) -> u32 {
    let dialog = (*this).inner.dialog;
    let left = Com::release(this);
    if left == 0 {
        Com::release(dialog);
    }
    left
}

unsafe extern "win64" fn custom_ok(_this: *mut Com<Customize>) -> i32 {
    S_OK
}

/// AddCheckButton(id, label, checked).
unsafe extern "win64" fn custom_add_check(
    this: *mut Com<Customize>,
    id: u32,
    _label: *const u16,
    checked: i32,
) -> i32 {
    lock(&custom_state(this)).checks.insert(id, checked != 0);
    S_OK
}

/// AddEditBox(id, text).
unsafe extern "win64" fn custom_add_edit(
    this: *mut Com<Customize>,
    id: u32,
    text: *const u16,
) -> i32 {
    lock(&custom_state(this))
        .edits
        .insert(id, opt_w(text).unwrap_or_default());
    S_OK
}

/// GetControlState(id, *state) : CDCS_ENABLEDVISIBLE.
unsafe extern "win64" fn custom_get_state(
    _this: *mut Com<Customize>,
    _id: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 3;
    S_OK
}

unsafe extern "win64" fn custom_get_edit(
    this: *mut Com<Customize>,
    id: u32,
    out: *mut *mut u16,
) -> i32 {
    let t = lock(&custom_state(this))
        .edits
        .get(&id)
        .cloned()
        .unwrap_or_default();
    co_string(&t, out)
}

unsafe extern "win64" fn custom_set_edit(
    this: *mut Com<Customize>,
    id: u32,
    text: *const u16,
) -> i32 {
    custom_add_edit(this, id, text)
}

unsafe extern "win64" fn custom_get_check(
    this: *mut Com<Customize>,
    id: u32,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = i32::from(
        lock(&custom_state(this))
            .checks
            .get(&id)
            .copied()
            .unwrap_or(false),
    );
    S_OK
}

unsafe extern "win64" fn custom_set_check(this: *mut Com<Customize>, id: u32, checked: i32) -> i32 {
    lock(&custom_state(this)).checks.insert(id, checked != 0);
    S_OK
}

/// AddControlItem(id, item, label) : le premier élément devient la sélection.
unsafe extern "win64" fn custom_add_item(
    this: *mut Com<Customize>,
    id: u32,
    item: u32,
    _label: *const u16,
) -> i32 {
    lock(&custom_state(this)).selected.entry(id).or_insert(item);
    S_OK
}

unsafe extern "win64" fn custom_get_item_state(
    _this: *mut Com<Customize>,
    _id: u32,
    _item: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 3;
    S_OK
}

unsafe extern "win64" fn custom_get_selected(
    this: *mut Com<Customize>,
    id: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match lock(&custom_state(this)).selected.get(&id) {
        Some(&i) => {
            *out = i;
            S_OK
        }
        None => E_FAIL,
    }
}

unsafe extern "win64" fn custom_set_selected(this: *mut Com<Customize>, id: u32, item: u32) -> i32 {
    lock(&custom_state(this)).selected.insert(id, item);
    S_OK
}

static CUSTOM_VT: VTable<30> = VTable([
    custom_qi as *const (),
    custom_add_ref as *const (),
    custom_release as *const (),
    custom_ok as *const (), // EnableOpenDropDown
    custom_ok as *const (), // AddMenu
    custom_ok as *const (), // AddPushButton
    custom_ok as *const (), // AddComboBox
    custom_ok as *const (), // AddRadioButtonList
    custom_add_check as *const (),
    custom_add_edit as *const (),
    custom_ok as *const (), // AddSeparator
    custom_ok as *const (), // AddText
    custom_ok as *const (), // SetControlLabel
    custom_get_state as *const (),
    custom_ok as *const (), // SetControlState
    custom_get_edit as *const (),
    custom_set_edit as *const (),
    custom_get_check as *const (),
    custom_set_check as *const (),
    custom_add_item as *const (),
    custom_ok as *const (), // RemoveControlItem
    custom_ok as *const (), // RemoveAllControlItems
    custom_get_item_state as *const (),
    custom_ok as *const (), // SetControlItemState
    custom_get_selected as *const (),
    custom_set_selected as *const (),
    custom_ok as *const (), // StartVisualGroup
    custom_ok as *const (), // EndVisualGroup
    custom_ok as *const (), // MakeProminent
    custom_ok as *const (), // SetControlItemText
]);

// ---------- IShellItem ----------

unsafe extern "win64" fn item_qi(
    this: *mut Com<ShellItem>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_ISHELL_ITEM => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => E_NOINTERFACE,
    }
}

unsafe extern "win64" fn item_add_ref(this: *mut Com<ShellItem>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn item_release(this: *mut Com<ShellItem>) -> u32 {
    Com::release(this)
}

unsafe extern "win64" fn item_bind(
    _this: *mut Com<ShellItem>,
    _bc: usize,
    _bhid: *const Guid,
    _riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn item_get_parent(this: *mut Com<ShellItem>, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let p = &(*this).inner.path;
    match p.rfind('\\').filter(|_| p.len() > 3) {
        Some(i) => put(out, new_item(&p[..i.max(3)])),
        None => {
            *out = std::ptr::null_mut();
            E_FAIL
        }
    }
}

/// Nom affiché, nom relatif au parent ou chemin complet selon `sigdn`.
pub(crate) fn display_name(path: &str, sigdn: u32) -> String {
    let leaf = || {
        path.trim_end_matches('\\')
            .rsplit('\\')
            .next()
            .unwrap_or(path)
            .to_string()
    };
    match sigdn {
        SIGDN_NORMALDISPLAY
        | SIGDN_PARENTRELATIVEPARSING
        | SIGDN_PARENTRELATIVEFORADDRESSBAR
        | SIGDN_PARENTRELATIVEEDITING
        | SIGDN_PARENTRELATIVE
            if path.len() > 3 =>
        {
            leaf()
        }
        _ => path.to_string(),
    }
}

unsafe extern "win64" fn item_get_display_name(
    this: *mut Com<ShellItem>,
    sigdn: u32,
    out: *mut *mut u16,
) -> i32 {
    co_string(&display_name(&(*this).inner.path, sigdn), out)
}

/// SFGAO_FOLDER | SFGAO_FILESYSTEM | SFGAO_FILESYSANCESTOR selon le chemin.
unsafe extern "win64" fn item_get_attributes(
    this: *mut Com<ShellItem>,
    mask: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let linux = paths::to_linux(&(*this).inner.path);
    let mut a = 0x4000_0000; // SFGAO_FILESYSTEM
    if linux.is_dir() {
        a |= 0x2000_0000 | 0x1000_0000; // SFGAO_FOLDER | SFGAO_FILESYSANCESTOR
    }
    *out = a & mask;
    if *out == mask {
        S_OK
    } else {
        1
    }
}

unsafe extern "win64" fn item_compare(
    this: *mut Com<ShellItem>,
    other: usize,
    _hint: u32,
    order: *mut i32,
) -> i32 {
    if order.is_null() {
        return E_POINTER;
    }
    let a = (*this).inner.path.to_lowercase();
    let b = item_path(other).unwrap_or_default().to_lowercase();
    *order = a.cmp(&b) as i32;
    if *order == 0 {
        S_OK
    } else {
        1
    }
}

static ITEM_VT: VTable<8> = VTable([
    item_qi as *const (),
    item_add_ref as *const (),
    item_release as *const (),
    item_bind as *const (),
    item_get_parent as *const (),
    item_get_display_name as *const (),
    item_get_attributes as *const (),
    item_compare as *const (),
]);

// ---------- IShellItemArray ----------

unsafe extern "win64" fn array_qi(
    this: *mut Com<ItemArray>,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_ISHELL_ITEM_ARRAY => {
            Com::add_ref(this);
            put(out, this)
        }
        _ => E_NOINTERFACE,
    }
}

unsafe extern "win64" fn array_add_ref(this: *mut Com<ItemArray>) -> u32 {
    Com::add_ref(this)
}

unsafe extern "win64" fn array_release(this: *mut Com<ItemArray>) -> u32 {
    Com::release(this)
}

unsafe extern "win64" fn array_notimpl(
    _this: *mut Com<ItemArray>,
    _a: usize,
    _b: usize,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

unsafe extern "win64" fn array_get_attributes(
    _this: *mut Com<ItemArray>,
    _flags: u32,
    _mask: u32,
    out: *mut u32,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

unsafe extern "win64" fn array_get_count(this: *mut Com<ItemArray>, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*this).inner.items.len() as u32;
    S_OK
}

unsafe extern "win64" fn array_get_item_at(
    this: *mut Com<ItemArray>,
    i: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items: &Vec<String> = &(*this).inner.items;
    match items.get(i as usize) {
        Some(p) => put(out, new_item(p)),
        None => {
            *out = std::ptr::null_mut();
            E_INVALIDARG
        }
    }
}

unsafe extern "win64" fn array_enum(_this: *mut Com<ItemArray>, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

static ARRAY_VT: VTable<10> = VTable([
    array_qi as *const (),
    array_add_ref as *const (),
    array_release as *const (),
    array_notimpl as *const (), // BindToHandler
    array_notimpl as *const (), // GetPropertyStore
    array_notimpl as *const (), // GetPropertyDescriptionList
    array_get_attributes as *const (),
    array_get_count as *const (),
    array_get_item_at as *const (),
    array_enum as *const (),
]);

// ---------- shell32 ----------

/// SHCreateItemFromParsingName : IShellItem d'un chemin existant.
pub unsafe extern "win64" fn sh_create_item_from_parsing_name(
    path: *const u16,
    _bc: usize,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(p) = opt_w(path).filter(|p| !p.is_empty()) else {
        return E_INVALIDARG;
    };
    let windows = if p.starts_with('/') {
        paths::to_windows(std::path::Path::new(&p))
    } else {
        p
    };
    if !paths::to_linux(&windows).exists() {
        return HR_FILE_NOT_FOUND;
    }
    let item = new_item(&windows);
    let hr = item_qi(item, riid, out);
    Com::release(item);
    hr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_affiches_et_resultats_multiples() {
        assert_eq!(
            display_name("C:\\Users\\d\\note.txt", SIGDN_NORMALDISPLAY),
            "note.txt"
        );
        assert_eq!(
            display_name("C:\\Users\\d\\note.txt", SIGDN_FILESYSPATH),
            "C:\\Users\\d\\note.txt"
        );
        assert_eq!(display_name("C:\\", SIGDN_NORMALDISPLAY), "C:\\");
        let one: Vec<u16> = "C:\\a\\b.txt\0\0".encode_utf16().collect();
        assert_eq!(split_result(&one), vec!["C:\\a\\b.txt"]);
        let many: Vec<u16> = "C:\\a\0x.txt\0y.txt\0\0".encode_utf16().collect();
        assert_eq!(split_result(&many), vec!["C:\\a\\x.txt", "C:\\a\\y.txt"]);
    }

    #[test]
    fn boite_ouvrir_com_par_la_boite_integree() {
        unsafe {
            let dir =
                std::env::temp_dir().join(format!("winlux-ifiledialog-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("lisez.txt"), "x").unwrap();
            let mut out: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_FILE_SAVE_DIALOG, &IID_IFILE_OPEN_DIALOG, &mut out),
                Some(E_NOINTERFACE)
            );
            assert_eq!(
                create_builtin(&CLSID_FILE_OPEN_DIALOG, &IID_IFILE_OPEN_DIALOG, &mut out),
                Some(S_OK)
            );
            let d = out as *mut Com<Dialog>;
            // Dossier de départ par SHCreateItemFromParsingName.
            let win = paths::to_windows(&dir);
            let w: Vec<u16> = win.encode_utf16().chain([0]).collect();
            let mut item: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                sh_create_item_from_parsing_name(w.as_ptr(), 0, &IID_ISHELL_ITEM, &mut item),
                S_OK
            );
            assert_eq!(dialog_set_folder(d, item as usize), S_OK);
            item_release(item as *mut Com<ShellItem>);
            let name: Vec<u16> = "Texte\0".encode_utf16().collect();
            let spec: Vec<u16> = "*.txt\0".encode_utf16().collect();
            let specs = [[name.as_ptr(), spec.as_ptr()]];
            assert_eq!(dialog_set_file_types(d, 1, specs.as_ptr()), S_OK);
            // IFileDialogCustomize : la case garde sa valeur initiale.
            let mut c: *mut c_void = std::ptr::null_mut();
            assert_eq!(dialog_qi(d, &IID_IFILE_DIALOG_CUSTOMIZE, &mut c), S_OK);
            let c = c as *mut Com<Customize>;
            custom_add_check(c, 7, std::ptr::null(), 1);
            let mut checked = 0;
            custom_get_check(c, 7, &mut checked);
            assert_eq!(checked, 1);
            custom_release(c);
            // Sans affichage : WINLUX_FILEDLG choisit le fichier, comme
            // pour GetOpenFileNameW.
            std::env::set_var("WINLUX_FILEDLG", "lisez.txt");
            let hr = dialog_show(d, 0);
            std::env::remove_var("WINLUX_FILEDLG");
            assert_eq!(hr, S_OK);
            let mut res: *mut c_void = std::ptr::null_mut();
            assert_eq!(dialog_get_result(d, &mut res), S_OK);
            let mut p: *mut u16 = std::ptr::null_mut();
            item_get_display_name(res as *mut Com<ShellItem>, SIGDN_FILESYSPATH, &mut p);
            let chosen = super::super::wstr(p);
            assert!(chosen.ends_with("\\lisez.txt"), "{chosen}");
            assert_eq!(paths::to_linux(&chosen), dir.join("lisez.txt"));
            item_release(res as *mut Com<ShellItem>);
            let mut arr: *mut c_void = std::ptr::null_mut();
            assert_eq!(dialog_slot27(d, &mut arr as *mut _ as usize), S_OK);
            let mut n = 0;
            array_get_count(arr as *mut Com<ItemArray>, &mut n);
            assert_eq!(n, 1);
            array_release(arr as *mut Com<ItemArray>);
            // Annulation : HRESULT_FROM_WIN32(ERROR_CANCELLED).
            let mut d2: *mut c_void = std::ptr::null_mut();
            create_builtin(&CLSID_FILE_OPEN_DIALOG, &IID_IFILE_DIALOG, &mut d2);
            assert_eq!(dialog_show(d2 as *mut Com<Dialog>, 0), HR_CANCELLED);
            dialog_release(d2 as *mut Com<Dialog>);
            dialog_release(d);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
