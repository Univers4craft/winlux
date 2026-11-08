//! Espace de noms du Shell : le dossier racine que rend SHGetDesktopFolder
//! (IShellFolder) et l'énumérateur de ses éléments (IEnumIDList).
//!
//! Les identifiants d'éléments sont ceux de Winlux (`apps_win32`) : un
//! SHITEMID « WLXP » qui porte le chemin Windows complet. Un identifiant
//! relatif rendu par un sous-dossier porte donc lui aussi le chemin complet,
//! et un identifiant formé de plusieurs éléments (ILCombine, ou un programme
//! qui les met bout à bout lui-même) désigne le chemin de son dernier
//! élément.
//!
//! Seul le système de fichiers est parcouru : le Bureau énumère le dossier
//! Bureau de l'utilisateur, sans « Ce PC » ni la Corbeille. Les objets
//! d'interface utilisateur (GetUIObjectOf, CreateViewObject) ne sont pas
//! fournis, E_NOINTERFACE est une réponse prévue.
//!
//! WinSCP x86 appelle ParseDisplayName sur l'objet rendu sans regarder le
//! HRESULT de SHGetDesktopFolder (#1107). Méthodes, ordre et valeurs
//! (SHCONTF, SHGDN, SFGAO, STRRET) : documentation Microsoft Learn
//! d'IShellFolder et d'IEnumIDList, shobjidl.h et shtypes.h de MinGW-w64
//! (domaine public).

use super::comobj::{guid, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND).
const E_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_DIRECTORY) : l'élément n'est pas un dossier.
const E_NOT_A_FOLDER: i32 = 0x8007_010Bu32 as i32;

const SHCONTF_FOLDERS: u32 = 0x20;
const SHCONTF_NONFOLDERS: u32 = 0x40;
const SHCONTF_INCLUDEHIDDEN: u32 = 0x80;

const SHGDN_INFOLDER: u32 = 0x1;
const SHGDN_FORPARSING: u32 = 0x8000;

const SFGAO_CANCOPY: u32 = 0x1;
const SFGAO_CANMOVE: u32 = 0x2;
const SFGAO_CANRENAME: u32 = 0x10;
const SFGAO_CANDELETE: u32 = 0x20;
const SFGAO_READONLY: u32 = 0x4_0000;
const SFGAO_HIDDEN: u32 = 0x8_0000;
const SFGAO_FILESYSANCESTOR: u32 = 0x1000_0000;
const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_FILESYSTEM: u32 = 0x4000_0000;
const SFGAO_HASSUBFOLDER: u32 = 0x8000_0000;

/// STRRET_WSTR : chaîne allouée par CoTaskMemAlloc.
const STRRET_WSTR: u32 = 0;

const CSIDL_DESKTOPDIRECTORY: i32 = 0x10;

pub const IID_ISHELL_FOLDER: Guid = guid(0x0002_14E6, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IENUM_ID_LIST: Guid = guid(0x0002_14F2, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// Dossier : son chemin Windows (le Bureau : le dossier Bureau de
/// l'utilisateur).
#[repr(C)]
struct Folder {
    vtbl: *const (),
    refs: AtomicU32,
    path: String,
}

/// Énumérateur : chemins complets des éléments, figés à la création.
#[repr(C)]
struct Enum {
    vtbl: *const (),
    refs: AtomicU32,
    items: Vec<String>,
    pos: AtomicUsize,
}

static FOLDER_VT: VTable<16> = VTable([
    folder_qi as *const (),
    folder_add_ref as *const (),
    folder_release as *const (),
    parse_display_name as *const (),
    enum_objects as *const (),
    bind_to_object as *const (),
    bind_to_storage as *const (),
    compare_ids as *const (),
    create_view_object as *const (),
    get_attributes_of as *const (),
    get_ui_object_of as *const (),
    get_display_name_of as *const (),
    set_name_of as *const (),
    // Réserve : un IShellFolder n'a que 13 méthodes.
    std::ptr::null(),
    std::ptr::null(),
    std::ptr::null(),
]);

static ENUM_VT: VTable<7> = VTable([
    enum_qi as *const (),
    enum_add_ref as *const (),
    enum_release as *const (),
    enum_next as *const (),
    enum_skip as *const (),
    enum_reset as *const (),
    enum_clone as *const (),
]);

fn new_folder(path: String) -> *mut Folder {
    Box::into_raw(Box::new(Folder {
        vtbl: &FOLDER_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        path,
    }))
}

fn desktop_path() -> String {
    super::knownfolders::csidl_path(CSIDL_DESKTOPDIRECTORY).unwrap_or_else(|| "C:\\".into())
}

/// SHGetDesktopFolder : un nouveau dossier racine, une référence.
pub unsafe fn desktop(out: *mut usize) -> u32 {
    if out.is_null() {
        return E_INVALIDARG as u32;
    }
    *out = new_folder(desktop_path()) as usize;
    0
}

fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 2 && b[1] == b':') || p.starts_with("\\\\")
}

fn join(dir: &str, name: &str) -> String {
    format!("{}\\{}", dir.trim_end_matches(['\\', '/']), name)
}

fn base_name(path: &str) -> String {
    path.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Chemin désigné par `pidl` dans le dossier `f` : un identifiant vide
/// désigne le dossier lui-même.
unsafe fn item_path(f: &Folder, pidl: *const u8) -> Option<String> {
    if pidl.is_null() {
        return None;
    }
    if std::ptr::read_unaligned(pidl as *const u16) == 0 {
        return Some(f.path.clone());
    }
    super::apps_win32::path_from_pidl(pidl)
}

fn is_dir(path: &str) -> bool {
    std::fs::metadata(crate::paths::to_linux(path)).is_ok_and(|m| m.is_dir())
        || (path.len() <= 3 && path.ends_with(":\\"))
}

/// Attributs Shell d'un fichier ou d'un dossier existant.
fn attributes(path: &str) -> Option<u32> {
    let linux = crate::paths::to_linux(path);
    let meta = std::fs::metadata(&linux).ok();
    let dir = match &meta {
        Some(m) => m.is_dir(),
        None if path.len() <= 3 && path.ends_with(":\\") => true,
        None => return None,
    };
    let mut a =
        SFGAO_FILESYSTEM | SFGAO_CANCOPY | SFGAO_CANMOVE | SFGAO_CANRENAME | SFGAO_CANDELETE;
    if dir {
        a |= SFGAO_FOLDER | SFGAO_FILESYSANCESTOR;
        let sub = std::fs::read_dir(&linux)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            })
            .unwrap_or(false);
        if sub {
            a |= SFGAO_HASSUBFOLDER;
        }
    }
    if meta.is_some_and(|m| m.permissions().readonly()) {
        a |= SFGAO_READONLY;
    }
    if base_name(path).starts_with('.') {
        a |= SFGAO_HIDDEN;
    }
    Some(a)
}

/// Éléments d'un dossier, triés (dossiers d'abord, puis par nom sans
/// tenir compte de la casse), filtrés par les drapeaux SHCONTF.
fn list(dir: &str, flags: u32) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(crate::paths::to_linux(dir)) else {
        return Vec::new();
    };
    let mut v: Vec<(bool, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let d = e
                .metadata()
                .map(|m| m.is_dir())
                .unwrap_or_else(|_| e.file_type().is_ok_and(|t| t.is_dir()));
            let wanted = if d {
                flags & SHCONTF_FOLDERS != 0
            } else {
                flags & SHCONTF_NONFOLDERS != 0
            };
            let hidden = name.starts_with('.');
            (wanted && (!hidden || flags & SHCONTF_INCLUDEHIDDEN != 0)).then_some((d, name))
        })
        .collect();
    v.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    v.into_iter().map(|(_, n)| join(dir, &n)).collect()
}

unsafe fn co_task_wstr(s: &str) -> usize {
    let w: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    let Some(alloc) = super::lookup("ole32", "CoTaskMemAlloc") else {
        return 0;
    };
    // SAFETY: export de Winlux, signature de CoTaskMemAlloc.
    let alloc: unsafe extern "win64" fn(usize) -> *mut u16 = std::mem::transmute(alloc);
    let p = alloc(w.len() * 2);
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    }
    p as usize
}

// ── IShellFolder ────────────────────────────────────────────────────────

unsafe extern "win64" fn folder_qi(
    this: *mut Folder,
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
    let iid = std::ptr::read_unaligned(iid);
    if iid != IID_IUNKNOWN && iid != IID_ISHELL_FOLDER {
        return E_NOINTERFACE;
    }
    folder_add_ref(this);
    *out = this as *mut c_void;
    S_OK
}

unsafe extern "win64" fn folder_add_ref(this: *mut Folder) -> u32 {
    (*this).refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "win64" fn folder_release(this: *mut Folder) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

/// ParseDisplayName : chemin absolu, ou nom relatif au dossier. Le
/// chemin doit exister.
unsafe extern "win64" fn parse_display_name(
    this: *mut Folder,
    _hwnd: usize,
    _bind: usize,
    name: *const u16,
    eaten: *mut u32,
    out: *mut usize,
    attrs: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if name.is_null() {
        return E_INVALIDARG;
    }
    let raw = super::wstr(name);
    let text = raw.trim();
    if text.is_empty() {
        return E_INVALIDARG;
    }
    let path = if is_absolute(text) {
        text.to_string()
    } else {
        join(&(*this).path, text)
    };
    let path = crate::paths::full_windows(&path);
    let Some(a) = attributes(&path) else {
        return E_FILE_NOT_FOUND;
    };
    let pidl = super::apps_win32::pidl_from_path(&path);
    if pidl == 0 {
        return E_OUTOFMEMORY;
    }
    *out = pidl;
    if !eaten.is_null() {
        *eaten = raw.encode_utf16().count() as u32;
    }
    if !attrs.is_null() {
        *attrs &= a;
    }
    S_OK
}

unsafe extern "win64" fn enum_objects(
    this: *mut Folder,
    _hwnd: usize,
    flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_enum(list(&(*this).path, flags), 0) as usize;
    S_OK
}

unsafe extern "win64" fn bind_to_object(
    this: *mut Folder,
    pidl: *const u8,
    _bind: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(path) = item_path(&*this, pidl) else {
        return E_INVALIDARG;
    };
    if attributes(&path).is_none() {
        return E_FILE_NOT_FOUND;
    }
    if !is_dir(&path) {
        return E_NOT_A_FOLDER;
    }
    let f = new_folder(path);
    let r = folder_qi(f, iid, out);
    folder_release(f);
    r
}

unsafe extern "win64" fn bind_to_storage(
    _this: *mut Folder,
    _pidl: *const u8,
    _bind: usize,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

/// CompareIDs : dossiers d'abord, puis nom sans tenir compte de la casse.
/// Le résultat est dans le code (16 bits bas) d'un HRESULT de succès.
unsafe extern "win64" fn compare_ids(
    this: *mut Folder,
    _column: isize,
    a: *const u8,
    b: *const u8,
) -> i32 {
    let (Some(pa), Some(pb)) = (item_path(&*this, a), item_path(&*this, b)) else {
        return E_INVALIDARG;
    };
    let n = is_dir(&pb)
        .cmp(&is_dir(&pa))
        .then_with(|| pa.to_lowercase().cmp(&pb.to_lowercase())) as i8;
    (n as i16 as u16) as i32
}

unsafe extern "win64" fn create_view_object(
    _this: *mut Folder,
    _hwnd: usize,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}

/// GetAttributesOf : attributs communs à tous les éléments (aucun
/// élément : le dossier lui-même).
unsafe extern "win64" fn get_attributes_of(
    this: *mut Folder,
    count: u32,
    pidls: *const usize,
    inout: *mut u32,
) -> i32 {
    if inout.is_null() {
        return E_POINTER;
    }
    let mut all = u32::MAX;
    if count == 0 || pidls.is_null() {
        all = attributes(&(*this).path).unwrap_or(0);
    } else {
        for i in 0..count as usize {
            let Some(path) = item_path(&*this, *pidls.add(i) as *const u8) else {
                return E_INVALIDARG;
            };
            all &= attributes(&path).unwrap_or(0);
        }
    }
    *inout &= all;
    S_OK
}

unsafe extern "win64" fn get_ui_object_of(
    _this: *mut Folder,
    _hwnd: usize,
    _count: u32,
    _pidls: *const usize,
    _iid: *const Guid,
    _reserved: usize,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}

/// GetDisplayNameOf : chemin complet (SHGDN_FORPARSING), sinon le nom de
/// l'élément, en STRRET_WSTR.
unsafe extern "win64" fn get_display_name_of(
    this: *mut Folder,
    pidl: *const u8,
    flags: u32,
    ret: *mut u8,
) -> i32 {
    if ret.is_null() {
        return E_POINTER;
    }
    let Some(path) = item_path(&*this, pidl) else {
        return E_INVALIDARG;
    };
    let text = if flags & SHGDN_FORPARSING != 0 && flags & SHGDN_INFOLDER == 0 {
        path
    } else {
        base_name(&path)
    };
    let s = co_task_wstr(&text);
    if s == 0 {
        return E_OUTOFMEMORY;
    }
    // STRRET x64 : uType, puis l'union alignée sur 8.
    std::ptr::write_unaligned(ret as *mut u32, STRRET_WSTR);
    std::ptr::write_unaligned(ret.add(8) as *mut usize, s);
    S_OK
}

unsafe extern "win64" fn set_name_of(
    _this: *mut Folder,
    _hwnd: usize,
    _pidl: *const u8,
    _name: *const u16,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

// ── IEnumIDList ─────────────────────────────────────────────────────────

fn new_enum(items: Vec<String>, pos: usize) -> *mut Enum {
    Box::into_raw(Box::new(Enum {
        vtbl: &ENUM_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        items,
        pos: AtomicUsize::new(pos),
    }))
}

unsafe extern "win64" fn enum_qi(this: *mut Enum, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let iid = std::ptr::read_unaligned(iid);
    if iid != IID_IUNKNOWN && iid != IID_IENUM_ID_LIST {
        return E_NOINTERFACE;
    }
    enum_add_ref(this);
    *out = this as *mut c_void;
    S_OK
}

unsafe extern "win64" fn enum_add_ref(this: *mut Enum) -> u32 {
    (*this).refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe extern "win64" fn enum_release(this: *mut Enum) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

unsafe extern "win64" fn enum_next(
    this: *mut Enum,
    count: u32,
    out: *mut usize,
    fetched: *mut u32,
) -> i32 {
    if out.is_null() || (count > 1 && fetched.is_null()) {
        return E_POINTER;
    }
    let e = &*this;
    let mut n = 0usize;
    while n < count as usize {
        let i = e.pos.load(Ordering::SeqCst);
        let Some(path) = e.items.get(i) else {
            break;
        };
        let p = super::apps_win32::pidl_from_path(path);
        if p == 0 {
            break;
        }
        e.pos.store(i + 1, Ordering::SeqCst);
        *out.add(n) = p;
        n += 1;
    }
    if !fetched.is_null() {
        *fetched = n as u32;
    }
    if n == count as usize {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn enum_skip(this: *mut Enum, count: u32) -> i32 {
    let e = &*this;
    let i = e.pos.load(Ordering::SeqCst);
    let j = (i + count as usize).min(e.items.len());
    e.pos.store(j, Ordering::SeqCst);
    if j - i == count as usize {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn enum_reset(this: *mut Enum) -> i32 {
    (*this).pos.store(0, Ordering::SeqCst);
    S_OK
}

unsafe extern "win64" fn enum_clone(this: *mut Enum, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let e = &*this;
    *out = new_enum(e.items.clone(), e.pos.load(Ordering::SeqCst)) as usize;
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn method(obj: usize, idx: usize) -> *const () {
        let vt = *(obj as *const *const *const ());
        *vt.add(idx)
    }

    #[test]
    fn bureau_analyse_et_enumere() {
        let dir = std::env::temp_dir().join(format!("winlux-sf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sous")).unwrap();
        std::fs::write(dir.join("b.txt"), b"x").unwrap();
        std::fs::write(dir.join(".cache"), b"x").unwrap();
        let win = crate::paths::to_windows(&dir);
        unsafe {
            let mut root = 0usize;
            assert_eq!(desktop(&mut root), 0);
            assert_ne!(root, 0);
            let parse: unsafe extern "win64" fn(
                usize,
                usize,
                usize,
                *const u16,
                *mut u32,
                *mut usize,
                *mut u32,
            ) -> i32 = std::mem::transmute(method(root, 3));
            let w: Vec<u16> = win.encode_utf16().chain(Some(0)).collect();
            let mut pidl = 0usize;
            let mut attrs = SFGAO_FOLDER | SFGAO_FILESYSTEM | SFGAO_CANCOPY;
            let r = parse(
                root,
                0,
                0,
                w.as_ptr(),
                std::ptr::null_mut(),
                &mut pidl,
                &mut attrs,
            );
            assert_eq!(r, 0);
            assert_ne!(pidl, 0);
            assert_eq!(attrs, SFGAO_FOLDER | SFGAO_FILESYSTEM | SFGAO_CANCOPY);

            // Nom inconnu : erreur et identifiant nul.
            let bad: Vec<u16> = format!("{win}\\absent")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut none = 1usize;
            let r = parse(
                root,
                0,
                0,
                bad.as_ptr(),
                std::ptr::null_mut(),
                &mut none,
                std::ptr::null_mut(),
            );
            assert_eq!(r, E_FILE_NOT_FOUND);
            assert_eq!(none, 0);

            // Sous-dossier, puis ses éléments.
            let bind: unsafe extern "win64" fn(
                usize,
                usize,
                usize,
                *const Guid,
                *mut usize,
            ) -> i32 = std::mem::transmute(method(root, 5));
            let mut sub = 0usize;
            assert_eq!(bind(root, pidl, 0, &IID_ISHELL_FOLDER, &mut sub), 0);
            let enum_objects: unsafe extern "win64" fn(usize, usize, u32, *mut usize) -> i32 =
                std::mem::transmute(method(sub, 4));
            let mut en = 0usize;
            assert_eq!(
                enum_objects(sub, 0, SHCONTF_FOLDERS | SHCONTF_NONFOLDERS, &mut en),
                0
            );
            let next: unsafe extern "win64" fn(usize, u32, *mut usize, *mut u32) -> i32 =
                std::mem::transmute(method(en, 3));
            #[repr(align(8))]
            struct Out([usize; 4]);
            let mut items = Out([0; 4]);
            let mut got = 0u32;
            assert_eq!(next(en, 4, items.0.as_mut_ptr(), &mut got), S_FALSE);
            assert_eq!(got, 2, "« .cache » est caché");
            let first = super::super::apps_win32::path_from_pidl(items.0[0] as *const u8).unwrap();
            assert!(first.ends_with("\\Sous"), "dossiers d'abord : {first}");

            // Nom affiché et chemin complet.
            let name_of: unsafe extern "win64" fn(usize, usize, u32, *mut u8) -> i32 =
                std::mem::transmute(method(sub, 11));
            #[repr(align(8))]
            struct StrRet([u8; 272]);
            let mut sr = StrRet([0; 272]);
            assert_eq!(name_of(sub, items.0[1], 0, sr.0.as_mut_ptr()), 0);
            let s = std::ptr::read_unaligned(sr.0.as_ptr().add(8) as *const *const u16);
            assert_eq!(super::super::wstr(s), "b.txt");
            assert_eq!(
                name_of(sub, items.0[1], SHGDN_FORPARSING, sr.0.as_mut_ptr()),
                0
            );
            let s = std::ptr::read_unaligned(sr.0.as_ptr().add(8) as *const *const u16);
            assert!(super::super::wstr(s).ends_with("\\b.txt"));

            // Attributs : le fichier n'est pas un dossier.
            let attrs_of: unsafe extern "win64" fn(usize, u32, *const usize, *mut u32) -> i32 =
                std::mem::transmute(method(sub, 9));
            let mut a = SFGAO_FOLDER | SFGAO_FILESYSTEM;
            assert_eq!(attrs_of(sub, 1, &items.0[1], &mut a), 0);
            assert_eq!(a, SFGAO_FILESYSTEM);

            // Comparaison : le dossier passe avant le fichier.
            let cmp: unsafe extern "win64" fn(usize, isize, usize, usize) -> i32 =
                std::mem::transmute(method(sub, 7));
            assert_eq!(cmp(sub, 0, items.0[0], items.0[1]), 0xFFFF);
            assert_eq!(cmp(sub, 0, items.0[1], items.0[1]), 0);

            enum_release(en as *mut Enum);
            folder_release(sub as *mut Folder);
            folder_release(root as *mut Folder);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
