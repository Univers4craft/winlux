//! Objets de données OLE et dépôt de fichiers, écrits de zéro.
//!
//! - `IDataObject` / `IEnumFORMATETC` maison au-dessus d'un contenu de
//!   presse-papiers (`clipboard::Contents`) ;
//! - presse-papiers OLE réel : `OleSetClipboard` copie les formats de l'objet
//!   de l'appli dans le presse-papiers partagé (donc aussi vers Linux grâce au
//!   pont), `OleGetClipboard` rend un objet de données sur son contenu ;
//! - réception d'un glisser-déposer venu de Linux (XDND, voir `xdnd.rs`) :
//!   `IDropTarget` enregistré par `RegisterDragDrop`, sinon `WM_DROPFILES`
//!   pour les fenêtres qui ont appelé `DragAcceptFiles` ;
//! - `DragQueryFileA/W`, `DragQueryPoint`, `DragFinish`, `ReleaseStgMedium`.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use super::clipboard::{self as cb, Contents};
use super::comobj::{
    guid, read_guid, Guid, VTable, E_NOTIMPL, E_POINTER, IID_IUNKNOWN, S_FALSE, S_OK,
};
use super::lock;

pub const IID_IDATAOBJECT: Guid = guid(0x0000_010E, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IENUMFORMATETC: Guid = guid(0x0000_0103, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const DV_E_FORMATETC: i32 = 0x8004_0064u32 as i32;
const DV_E_TYMED: i32 = 0x8004_0069u32 as i32;
const OLE_E_ADVISENOTSUPPORTED: i32 = 0x8004_0003u32 as i32;
const DATA_S_SAMEFORMATETC: i32 = 0x0004_0130;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const TYMED_HGLOBAL: u32 = 1;
const DATADIR_GET: u32 = 1;
const DVASPECT_CONTENT: u32 = 1;
pub const WM_DROPFILES: u32 = 0x0233;
const WS_EX_ACCEPTFILES: u32 = 0x10;
pub const DROPEFFECT_NONE: u32 = 0;
pub const DROPEFFECT_COPY: u32 = 1;
/// Message privé posté par le backend X11 quand des données Linux sont
/// déposées sur une fenêtre ; traité dans `DispatchMessage`, jamais vu par l'appli.
pub const WM_WINLUX_DROP: u32 = 0x0001_0D0D;

/// FORMATETC (x64).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FormatEtc {
    pub cf: u16,
    pub ptd: usize,
    pub aspect: u32,
    pub lindex: i32,
    pub tymed: u32,
}

/// STGMEDIUM (x64).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct StgMedium {
    pub tymed: u32,
    pub handle: usize,
    pub unk_for_release: usize,
}

fn alloc_hglobal(bytes: &[u8]) -> usize {
    match super::kernel32::create_global_memory(bytes.len(), 0x0002) {
        Some((h, mem)) => {
            lock(&mem).bytes.copy_from_slice(bytes);
            h
        }
        None => 0,
    }
}

fn hglobal_bytes(h: usize) -> Option<Vec<u8>> {
    let mem = super::kernel32::lookup_global_memory(h)?;
    let bytes = lock(&mem).bytes.clone();
    Some(bytes)
}

/// Appelle la méthode `index` de la table virtuelle d'un objet COM invité.
unsafe fn vfn(obj: usize, index: usize) -> usize {
    let vtbl = *(obj as *const *const usize);
    *vtbl.add(index)
}

unsafe fn com_release(obj: usize) {
    if obj != 0 {
        let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(vfn(obj, 2));
        f(obj);
    }
}

unsafe fn com_addref(obj: usize) {
    if obj != 0 {
        let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(vfn(obj, 1));
        f(obj);
    }
}

// ---------------------------------------------------------------------------
// IDataObject maison
// ---------------------------------------------------------------------------

#[repr(C)]
struct DataObject {
    vtbl: *const VTable<12>,
    refs: AtomicU32,
    contents: Mutex<Contents>,
}

/// Crée un `IDataObject` (référence initiale : 1) sur un contenu.
pub fn new_data_object(contents: Contents) -> usize {
    Box::into_raw(Box::new(DataObject {
        vtbl: &DATA_VTBL,
        refs: AtomicU32::new(1),
        contents: Mutex::new(contents),
    })) as usize
}

unsafe extern "win64" fn do_qi(this: *mut DataObject, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IDATAOBJECT => {
            (*this).refs.fetch_add(1, Ordering::AcqRel);
            *out = this as usize;
            S_OK
        }
        _ => {
            *out = 0;
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn do_addref(this: *mut DataObject) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn do_release(this: *mut DataObject) -> u32 {
    let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        drop(Box::from_raw(this));
    }
    left
}
unsafe extern "win64" fn do_get_data(
    this: *mut DataObject,
    fe: *const FormatEtc,
    med: *mut StgMedium,
) -> i32 {
    if fe.is_null() || med.is_null() {
        return E_POINTER;
    }
    let fe = *fe;
    if fe.tymed & TYMED_HGLOBAL == 0 {
        return DV_E_TYMED;
    }
    let Some(bytes) = lock(&(*this).contents).render(fe.cf as u32) else {
        return DV_E_FORMATETC;
    };
    let h = alloc_hglobal(&bytes);
    if h == 0 {
        return E_OUTOFMEMORY;
    }
    *med = StgMedium {
        tymed: TYMED_HGLOBAL,
        handle: h,
        unk_for_release: 0,
    };
    S_OK
}
unsafe extern "win64" fn do_get_data_here(
    _t: *mut DataObject,
    _fe: *const FormatEtc,
    _m: *mut StgMedium,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn do_query_get_data(this: *mut DataObject, fe: *const FormatEtc) -> i32 {
    if fe.is_null() {
        return E_POINTER;
    }
    let fe = *fe;
    if fe.tymed & TYMED_HGLOBAL == 0 {
        return DV_E_TYMED;
    }
    if lock(&(*this).contents)
        .available()
        .contains(&(fe.cf as u32))
    {
        S_OK
    } else {
        DV_E_FORMATETC
    }
}
unsafe extern "win64" fn do_canonical(
    _t: *mut DataObject,
    _in: *const FormatEtc,
    out: *mut FormatEtc,
) -> i32 {
    if !out.is_null() {
        (*out).ptd = 0;
    }
    DATA_S_SAMEFORMATETC
}
unsafe extern "win64" fn do_set_data(
    this: *mut DataObject,
    fe: *const FormatEtc,
    med: *const StgMedium,
    release: i32,
) -> i32 {
    if fe.is_null() || med.is_null() {
        return E_POINTER;
    }
    let (fe, m) = (*fe, *med);
    if m.tymed != TYMED_HGLOBAL {
        return DV_E_TYMED;
    }
    let Some(bytes) = hglobal_bytes(m.handle) else {
        return DV_E_FORMATETC;
    };
    {
        let mut c = lock(&(*this).contents);
        let cf = fe.cf as u32;
        c.formats.retain(|(f, _)| *f != cf);
        c.formats.push((cf, bytes));
    }
    if release != 0 {
        release_stg_medium(med as *mut StgMedium);
    }
    S_OK
}
unsafe extern "win64" fn do_enum(this: *mut DataObject, dir: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if dir != DATADIR_GET {
        *out = 0;
        return E_NOTIMPL;
    }
    let formats = lock(&(*this).contents).available();
    *out = new_enum(formats, 0);
    S_OK
}
unsafe extern "win64" fn do_dadvise(
    _t: *mut DataObject,
    _f: *const FormatEtc,
    _a: u32,
    _s: usize,
    _c: *mut u32,
) -> i32 {
    OLE_E_ADVISENOTSUPPORTED
}
unsafe extern "win64" fn do_dunadvise(_t: *mut DataObject, _c: u32) -> i32 {
    OLE_E_ADVISENOTSUPPORTED
}
unsafe extern "win64" fn do_enum_dadvise(_t: *mut DataObject, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    OLE_E_ADVISENOTSUPPORTED
}

static DATA_VTBL: VTable<12> = VTable([
    do_qi as *const (),
    do_addref as *const (),
    do_release as *const (),
    do_get_data as *const (),
    do_get_data_here as *const (),
    do_query_get_data as *const (),
    do_canonical as *const (),
    do_set_data as *const (),
    do_enum as *const (),
    do_dadvise as *const (),
    do_dunadvise as *const (),
    do_enum_dadvise as *const (),
]);

#[repr(C)]
struct EnumFormat {
    vtbl: *const VTable<7>,
    refs: AtomicU32,
    formats: Vec<u32>,
    pos: AtomicUsize,
}

fn new_enum(formats: Vec<u32>, pos: usize) -> usize {
    Box::into_raw(Box::new(EnumFormat {
        vtbl: &ENUM_VTBL,
        refs: AtomicU32::new(1),
        formats,
        pos: AtomicUsize::new(pos),
    })) as usize
}

unsafe extern "win64" fn ef_qi(this: *mut EnumFormat, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IENUMFORMATETC => {
            (*this).refs.fetch_add(1, Ordering::AcqRel);
            *out = this as usize;
            S_OK
        }
        _ => {
            *out = 0;
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn ef_addref(this: *mut EnumFormat) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn ef_release(this: *mut EnumFormat) -> u32 {
    let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        drop(Box::from_raw(this));
    }
    left
}
unsafe extern "win64" fn ef_next(
    this: *mut EnumFormat,
    celt: u32,
    out: *mut FormatEtc,
    fetched: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let e = &*this;
    let mut n = 0u32;
    while n < celt {
        let pos = e.pos.load(Ordering::Acquire);
        let Some(&cf) = e.formats.get(pos) else {
            break;
        };
        e.pos.store(pos + 1, Ordering::Release);
        *out.add(n as usize) = FormatEtc {
            cf: cf as u16,
            ptd: 0,
            aspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        };
        n += 1;
    }
    if !fetched.is_null() {
        *fetched = n;
    }
    if n == celt {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn ef_skip(this: *mut EnumFormat, celt: u32) -> i32 {
    let e = &*this;
    let pos = e.pos.load(Ordering::Acquire) + celt as usize;
    e.pos.store(pos.min(e.formats.len()), Ordering::Release);
    if pos <= e.formats.len() {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn ef_reset(this: *mut EnumFormat) -> i32 {
    (*this).pos.store(0, Ordering::Release);
    S_OK
}
unsafe extern "win64" fn ef_clone(this: *mut EnumFormat, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let e = &*this;
    *out = new_enum(e.formats.clone(), e.pos.load(Ordering::Acquire));
    S_OK
}

static ENUM_VTBL: VTable<7> = VTable([
    ef_qi as *const (),
    ef_addref as *const (),
    ef_release as *const (),
    ef_next as *const (),
    ef_skip as *const (),
    ef_reset as *const (),
    ef_clone as *const (),
]);

// ---------------------------------------------------------------------------
// Lecture d'un IDataObject quelconque (celui de l'appli)
// ---------------------------------------------------------------------------

/// Copie tous les formats HGLOBAL d'un objet de données invité.
///
/// # Safety
/// `obj` doit être un `IDataObject` valide.
pub unsafe fn read_data_object(obj: usize) -> Vec<(u32, Vec<u8>)> {
    let mut wanted: Vec<u32> = Vec::new();
    let mut en: usize = 0;
    let enum_fn: extern "win64" fn(usize, u32, *mut usize) -> i32 =
        std::mem::transmute(vfn(obj, 8));
    if enum_fn(obj, DATADIR_GET, &mut en) == S_OK && en != 0 {
        let next: extern "win64" fn(usize, u32, *mut FormatEtc, *mut u32) -> i32 =
            std::mem::transmute(vfn(en, 3));
        let mut fe = FormatEtc::default();
        let mut got = 0u32;
        let mut guard = 0;
        while next(en, 1, &mut fe, &mut got) == S_OK && got == 1 && guard < 256 {
            if fe.tymed & TYMED_HGLOBAL != 0 && !wanted.contains(&(fe.cf as u32)) {
                wanted.push(fe.cf as u32);
            }
            guard += 1;
        }
        com_release(en);
    }
    if wanted.is_empty() {
        wanted = vec![
            cb::CF_UNICODETEXT,
            cb::CF_TEXT,
            cb::CF_DIB,
            cb::CF_HDROP,
            cb::registered_id(crate::clipbridge::FORMAT_HTML),
            cb::registered_id(crate::clipbridge::FORMAT_PNG),
        ];
    }
    let get: extern "win64" fn(usize, *const FormatEtc, *mut StgMedium) -> i32 =
        std::mem::transmute(vfn(obj, 3));
    let mut out = Vec::new();
    for cf in wanted {
        let fe = FormatEtc {
            cf: cf as u16,
            ptd: 0,
            aspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        };
        let mut med = StgMedium::default();
        if get(obj, &fe, &mut med) != S_OK {
            continue;
        }
        if med.tymed == TYMED_HGLOBAL {
            if let Some(bytes) = hglobal_bytes(med.handle) {
                out.push((cf, bytes));
            }
        }
        release_stg_medium(&mut med);
    }
    out
}

// ---------------------------------------------------------------------------
// Presse-papiers OLE
// ---------------------------------------------------------------------------

pub unsafe extern "win64" fn ole_set_clipboard(obj: usize) -> i32 {
    let formats = if obj == 0 {
        Vec::new()
    } else {
        read_data_object(obj)
    };
    if cb::open_clipboard(0) == 0 {
        return 0x8004_00D0u32 as i32; // CLIPBRD_E_CANT_OPEN
    }
    cb::empty_clipboard();
    for (cf, bytes) in formats {
        let h = alloc_hglobal(&bytes);
        if h != 0 {
            cb::set_clipboard_data(cf, h);
        }
    }
    cb::close_clipboard();
    S_OK
}

pub unsafe extern "win64" fn ole_get_clipboard(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let contents = cb::load(&cb::shared_path())
        .map(|(c, _)| c)
        .unwrap_or_default();
    *out = new_data_object(contents);
    S_OK
}

pub extern "win64" fn ole_flush_clipboard() -> i32 {
    S_OK
}

pub extern "win64" fn ole_is_current_clipboard(_obj: usize) -> i32 {
    S_FALSE
}

pub unsafe extern "win64" fn release_stg_medium(med: *mut StgMedium) {
    if med.is_null() {
        return;
    }
    let m = *med;
    if m.unk_for_release != 0 {
        com_release(m.unk_for_release);
    } else if m.tymed == TYMED_HGLOBAL && m.handle != 0 {
        super::kernel32::free_global_memory(m.handle);
    }
    *med = StgMedium::default();
}

// ---------------------------------------------------------------------------
// Cibles de dépôt
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Registry {
    /// hwnd → IDropTarget (référence tenue).
    targets: HashMap<usize, usize>,
    /// hwnd ayant appelé DragAcceptFiles(TRUE).
    files: HashMap<usize, ()>,
    /// hwnd → fenêtre de premier niveau (calculé à l'enregistrement).
    roots: HashMap<usize, usize>,
    drops: HashMap<usize, PendingDrop>,
    next: usize,
}

struct PendingDrop {
    contents: Contents,
    screen: (i32, i32),
    client: (i32, i32),
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry::default()))
}

fn root_of(hwnd: usize) -> usize {
    let mut h = hwnd;
    for _ in 0..64 {
        match super::gui::window_parent(h) {
            Some(p) if p != 0 => h = p,
            _ => break,
        }
    }
    h
}

pub unsafe extern "win64" fn register_drag_drop(hwnd: usize, target: usize) -> i32 {
    if target == 0 {
        return E_POINTER;
    }
    if !super::gui::is_window(hwnd) {
        return 0x8004_0100u32 as i32; // DRAGDROP_E_INVALIDHWND
    }
    let root = root_of(hwnd);
    let mut r = lock(registry());
    if r.targets.contains_key(&hwnd) {
        return 0x8004_0101u32 as i32; // DRAGDROP_E_ALREADYREGISTERED
    }
    com_addref(target);
    r.targets.insert(hwnd, target);
    r.roots.insert(hwnd, root);
    S_OK
}

pub unsafe extern "win64" fn revoke_drag_drop(hwnd: usize) -> i32 {
    let target = lock(registry()).targets.remove(&hwnd);
    match target {
        Some(t) => {
            com_release(t);
            S_OK
        }
        None => 0x8004_0100u32 as i32, // DRAGDROP_E_NOTREGISTERED
    }
}

pub extern "win64" fn drag_accept_files(hwnd: usize, accept: i32) {
    let root = root_of(hwnd);
    let mut r = lock(registry());
    if accept != 0 {
        r.files.insert(hwnd, ());
        r.roots.insert(hwnd, root);
    } else {
        r.files.remove(&hwnd);
    }
}

/// La fenêtre de premier niveau (ou une de ses filles) accepte-t-elle un dépôt ?
/// Appelé par le backend X11 : ne touche qu'au registre local.
pub fn accepts(top: usize) -> bool {
    let r = lock(registry());
    r.targets
        .keys()
        .chain(r.files.keys())
        .any(|h| r.roots.get(h) == Some(&top) || *h == top)
}

/// Enregistre un dépôt arrivé de Linux ; le message renvoyé est à poster
/// à la fenêtre de premier niveau.
pub fn queue_drop(
    top: usize,
    contents: Contents,
    screen: (i32, i32),
    client: (i32, i32),
) -> (usize, u32, usize) {
    let mut r = lock(registry());
    r.next = r.next.wrapping_add(1).max(1);
    let id = r.next;
    r.drops.insert(
        id,
        PendingDrop {
            contents,
            screen,
            client,
        },
    );
    (top, WM_WINLUX_DROP, id)
}

/// Fenêtre qui reçoit le dépôt : une cible OLE de préférence, sinon une
/// fenêtre qui accepte les fichiers ; la fenêtre elle-même avant ses filles.
fn pick(top: usize) -> Option<(usize, Option<usize>)> {
    let r = lock(registry());
    let belongs = |h: &usize| *h == top || r.roots.get(h) == Some(&top);
    if let Some(t) = r.targets.get(&top) {
        return Some((top, Some(*t)));
    }
    if let Some((h, t)) = r.targets.iter().find(|(h, _)| belongs(h)) {
        return Some((*h, Some(*t)));
    }
    if r.files.contains_key(&top) {
        return Some((top, None));
    }
    if let Some(h) = r.files.keys().find(|h| belongs(h)) {
        return Some((*h, None));
    }
    drop(r);
    let ex = super::gui::window_ex_style(top).unwrap_or(0);
    (ex & WS_EX_ACCEPTFILES != 0).then_some((top, None))
}

type DragFn = extern "win64" fn(usize, usize, u32, u64, *mut u32) -> i32;
type OverFn = extern "win64" fn(usize, u32, u64, *mut u32) -> i32;

fn pointl(p: (i32, i32)) -> u64 {
    (p.0 as u32 as u64) | ((p.1 as u32 as u64) << 32)
}

/// Livre un dépôt à l'appli. Appelé depuis `DispatchMessage`, sur le thread
/// de la fenêtre, sans verrou tenu. Renvoie `true` si le message était privé.
pub fn dispatch_private(hwnd: usize, message: u32, wparam: usize) -> bool {
    if message != WM_WINLUX_DROP {
        return false;
    }
    let Some(d) = lock(registry()).drops.remove(&wparam) else {
        return true;
    };
    let data = new_data_object(d.contents.clone());
    deliver(hwnd, data, &d.contents, d.screen, d.client, DROPEFFECT_COPY);
    unsafe { com_release(data) };
    true
}

/// Dépose `data` (IDataObject) sur la fenêtre de premier niveau `top` de ce
/// processus. Renvoie l'effet retenu par la cible.
pub fn deliver(
    top: usize,
    data: usize,
    contents: &Contents,
    screen: (i32, i32),
    client: (i32, i32),
    allowed: u32,
) -> u32 {
    let Some((target_hwnd, target)) = pick(top) else {
        return DROPEFFECT_NONE;
    };
    let first = |e: u32| -> u32 {
        [DROPEFFECT_COPY, 2, 4]
            .into_iter()
            .find(|b| e & b != 0)
            .unwrap_or(DROPEFFECT_NONE)
    };
    match target {
        Some(t) => unsafe {
            let pt = pointl(screen);
            let enter: DragFn = std::mem::transmute(vfn(t, 3));
            let over: OverFn = std::mem::transmute(vfn(t, 4));
            let leave: extern "win64" fn(usize) -> i32 = std::mem::transmute(vfn(t, 5));
            let drop_fn: DragFn = std::mem::transmute(vfn(t, 6));
            // MK_LBUTTON pendant le survol, puis bouton relâché au dépôt.
            let mut effect = allowed;
            enter(t, data, 1, pt, &mut effect);
            if effect & allowed != DROPEFFECT_NONE {
                effect = allowed;
                over(t, 1, pt, &mut effect);
            }
            if effect & allowed != DROPEFFECT_NONE {
                effect = allowed;
                drop_fn(t, data, 0, pt, &mut effect);
                first(effect & allowed)
            } else {
                leave(t);
                DROPEFFECT_NONE
            }
        },
        None => {
            let Some(hdrop) = contents.get(cb::CF_HDROP) else {
                return DROPEFFECT_NONE;
            };
            let mut bytes = hdrop.to_vec();
            if bytes.len() >= 20 {
                bytes[4..8].copy_from_slice(&client.0.to_le_bytes());
                bytes[8..12].copy_from_slice(&client.1.to_le_bytes());
                bytes[12..16].copy_from_slice(&0u32.to_le_bytes());
            }
            let h = alloc_hglobal(&bytes);
            if h == 0 {
                return DROPEFFECT_NONE;
            }
            match super::gui::wnd_proc_of(target_hwnd) {
                Some(wp) => unsafe {
                    super::gui::call_wndproc(wp, target_hwnd, WM_DROPFILES, h, 0);
                    first(allowed)
                },
                None => {
                    super::kernel32::free_global_memory(h);
                    DROPEFFECT_NONE
                }
            }
        }
    }
}

/// Fenêtre de premier niveau de ce processus affichée dans la fenêtre X `xwin`.
pub fn local_top_for(xwin: u64) -> Option<usize> {
    let tops: Vec<usize> = {
        let r = lock(registry());
        let mut v: Vec<usize> = r.roots.values().copied().collect();
        v.extend(r.targets.keys().chain(r.files.keys()).copied());
        v.sort_unstable();
        v.dedup();
        v
    };
    tops.into_iter()
        .find(|t| super::gui::native_window(*t) == Some(xwin))
}

// ---------------------------------------------------------------------------
// shell32 : DragQueryFile & co
// ---------------------------------------------------------------------------

fn hdrop_files(hdrop: usize) -> Vec<String> {
    hglobal_bytes(hdrop)
        .map(|b| crate::clipbridge::hdrop_parse(&b))
        .unwrap_or_default()
}

pub unsafe extern "win64" fn drag_query_file_w(
    hdrop: usize,
    index: u32,
    buf: *mut u16,
    cch: u32,
) -> u32 {
    let files = hdrop_files(hdrop);
    if index == u32::MAX {
        return files.len() as u32;
    }
    let Some(name) = files.get(index as usize) else {
        return 0;
    };
    let w: Vec<u16> = name.encode_utf16().collect();
    if buf.is_null() || cch == 0 {
        return w.len() as u32;
    }
    let n = w.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

pub unsafe extern "win64" fn drag_query_file_a(
    hdrop: usize,
    index: u32,
    buf: *mut u8,
    cch: u32,
) -> u32 {
    let files = hdrop_files(hdrop);
    if index == u32::MAX {
        return files.len() as u32;
    }
    let Some(name) = files.get(index as usize) else {
        return 0;
    };
    let a = super::encode_ansi(name);
    if buf.is_null() || cch == 0 {
        return a.len() as u32;
    }
    let n = a.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

pub unsafe extern "win64" fn drag_query_point(hdrop: usize, pt: *mut i32) -> i32 {
    let Some(b) = hglobal_bytes(hdrop).filter(|b| b.len() >= 20) else {
        return 0;
    };
    if !pt.is_null() {
        *pt = i32::from_le_bytes(b[4..8].try_into().unwrap());
        *pt.add(1) = i32::from_le_bytes(b[8..12].try_into().unwrap());
    }
    i32::from(u32::from_le_bytes(b[12..16].try_into().unwrap()) == 0)
}

pub extern "win64" fn drag_finish(hdrop: usize) {
    super::kernel32::free_global_memory(hdrop);
}

pub fn ole32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "OleSetClipboard"=>fn ole_set_clipboard,"OleGetClipboard"=>fn ole_get_clipboard,
        "OleFlushClipboard"=>fn ole_flush_clipboard,"OleIsCurrentClipboard"=>fn ole_is_current_clipboard,
        "ReleaseStgMedium"=>fn release_stg_medium,
        "RegisterDragDrop"=>fn register_drag_drop,"RevokeDragDrop"=>fn revoke_drag_drop,
        "DoDragDrop"=>fn super::xdnd::do_drag_drop
    }
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DragAcceptFiles"=>fn drag_accept_files,
        "DragQueryFileA"=>fn drag_query_file_a,"DragQueryFileW"=>fn drag_query_file_w,
        "DragQueryFile"=>fn drag_query_file_a,
        "DragQueryPoint"=>fn drag_query_point,"DragFinish"=>fn drag_finish
    }
}

#[allow(dead_code)]
fn _assert_layout() {
    const _: () = assert!(std::mem::size_of::<FormatEtc>() == 32);
    const _: () = assert!(std::mem::size_of::<StgMedium>() == 24);
    let _ = std::ptr::null::<c_void>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objet_de_donnees_maison_rend_et_enumere_les_formats() {
        let contents = Contents {
            seq: 1,
            formats: vec![(cb::CF_UNICODETEXT, vec![b'h', 0, b'i', 0, 0, 0])],
        };
        let obj = new_data_object(contents);
        unsafe {
            let mut fe = FormatEtc {
                cf: cb::CF_TEXT as u16,
                aspect: 1,
                lindex: -1,
                tymed: TYMED_HGLOBAL,
                ptd: 0,
            };
            assert_eq!(do_query_get_data(obj as *mut DataObject, &fe), S_OK);
            let mut med = StgMedium::default();
            assert_eq!(do_get_data(obj as *mut DataObject, &fe, &mut med), S_OK);
            assert_eq!(hglobal_bytes(med.handle).unwrap(), b"hi\0");
            release_stg_medium(&mut med);
            fe.cf = cb::CF_DIB as u16;
            assert_eq!(
                do_query_get_data(obj as *mut DataObject, &fe),
                DV_E_FORMATETC
            );
            // Lecture générique, comme pour un objet fourni par une appli.
            let all = read_data_object(obj);
            assert!(all.iter().any(|(f, d)| *f == cb::CF_TEXT && d == b"hi\0"));
            assert_eq!(all.len(), 3);
            assert_eq!(do_release(obj as *mut DataObject), 0);
        }
    }

    #[test]
    fn drag_query_file_lit_un_hdrop() {
        let h = alloc_hglobal(&crate::clipbridge::hdrop_build(&[
            "C:\\a.txt".into(),
            "Z:\\tmp\\b".into(),
        ]));
        unsafe {
            assert_eq!(drag_query_file_w(h, u32::MAX, std::ptr::null_mut(), 0), 2);
            let mut buf = [0u16; 64];
            assert_eq!(drag_query_file_w(h, 1, buf.as_mut_ptr(), 64), 8);
            assert_eq!(String::from_utf16_lossy(&buf[..8]), "Z:\\tmp\\b");
            let mut small = [0u8; 4];
            assert_eq!(drag_query_file_a(h, 0, small.as_mut_ptr(), 4), 3);
            assert_eq!(&small, b"C:\\\0");
        }
        drag_finish(h);
    }
}
