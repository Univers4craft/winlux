//! Classe Windows Runtime `Windows.Storage.ApplicationData` (#2122) :
//! `ApplicationData.Current` d'un programme empaqueté (MSIX) et ses
//! dossiers LocalFolder, RoamingFolder, TemporaryFolder, LocalCacheFolder.
//! Teams la lit au démarrage ; sans elle il lève std::runtime_error et
//! s'arrête avant sa fenêtre.
//!
//! Sources : documentation publique de Microsoft (ApplicationData,
//! StorageFolder, IStorageItem, emplacements des données d'application
//! `%LOCALAPPDATA%\Packages\<famille>\LocalState`, `RoamingState`,
//! `TempState`, `LocalCache`) ; identifiants d'interface et ordre des
//! méthodes tels que les publie windows-rs (licence MIT,
//! crates/libs/windows/src/Windows/Storage/mod.rs).
//!
//! Le dossier de la famille est dérivé de l'identité du paquet lue dans
//! son AppxManifest (GetCurrentPackageFamilyName), créé au premier usage
//! et réservé au compte (0700) : ce sont les données privées de
//! l'application. Hors paquet, `Current` rend APPMODEL_ERROR_NO_PACKAGE,
//! comme Windows.
//!
//! Pas encore servis (E_NOTIMPL, journalisé une fois par méthode) : les
//! méthodes asynchrones (StorageFolder, ClearAsync…), LocalSettings et
//! RoamingSettings.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
/// HRESULT_FROM_WIN32(APPMODEL_ERROR_NO_PACKAGE = 15700).
pub const E_NO_PACKAGE: i32 = 0x8007_3D54u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_ACCESS_DENIED).
const E_ACCESS_DENIED: i32 = 0x8007_0005u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_APPLICATION_DATA_STATICS: Guid = g(0x5612_147b, 0xe843, 0x45e3, 0x94d8_0616_9e3c_8e17);
const IID_APPLICATION_DATA: Guid = g(0xc3da_6fb7, 0xb744, 0x4b45, 0xb0b8_223a_0938_d0dc);
const IID_APPLICATION_DATA2: Guid = g(0x9e65_cd69, 0x0ba3, 0x4e32, 0xbe29_b02d_e660_7638);
const IID_APPLICATION_DATA3: Guid = g(0xdc22_2cf4, 0x2772, 0x4c1d, 0xaa2c_c9f7_43ad_e8d1);
const IID_STORAGE_FOLDER: Guid = g(0x72d1_cb78, 0xb3ef, 0x4f75, 0xa80b_6fd9_dae2_944b);
const IID_STORAGE_ITEM: Guid = g(0x4207_a996, 0xca2f, 0x42f7, 0xbde8_8b10_457a_7f30);

const CLASS: &str = "Windows.Storage.ApplicationData";
const FOLDER_CLASS: &str = "Windows.Storage.StorageFolder";

/// FileAttributes.Directory, StorageItemTypes.Folder.
const FILE_ATTRIBUTE_DIRECTORY: u32 = 16;
const STORAGE_ITEM_FOLDER: u32 = 2;

/// Méthodes non servies déjà signalées (une ligne de journal chacune).
fn not_implemented(method: &str) -> i32 {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.iter().any(|m| m == method) {
        seen.push(method.to_string());
        crate::log_warn!(
            "com",
            "ApplicationData : {method} pas encore servie (E_NOTIMPL)"
        );
    }
    E_NOTIMPL
}

unsafe fn hstring(text: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

// ============================================================
//  Emplacements
// ============================================================

/// Dossiers d'un paquet, sous `%LOCALAPPDATA%\Packages\<famille>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Local,
    Roaming,
    Temp,
    LocalCache,
}

impl Kind {
    fn dir(self) -> &'static str {
        match self {
            Kind::Local => "LocalState",
            Kind::Roaming => "RoamingState",
            Kind::Temp => "TempState",
            Kind::LocalCache => "LocalCache",
        }
    }
}

/// Famille du paquet courant, lue dans son AppxManifest.
fn current_family() -> Option<String> {
    let f = crate::dlls::lookup("kernel32", "GetCurrentPackageFamilyName")?;
    let f: unsafe extern "win64" fn(*mut u32, *mut u16) -> u32 = unsafe { std::mem::transmute(f) };
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    if unsafe { f(&mut len, buf.as_mut_ptr()) } != 0 {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let fam = String::from_utf16_lossy(&buf[..n]);
    // Une famille ne contient ni séparateur ni « .. » : jamais hors de Packages.
    (!fam.is_empty() && !fam.contains(['\\', '/', ':']) && fam != "." && fam != "..").then_some(fam)
}

/// Chemin Windows d'un dossier de paquet.
pub fn folder_path(family: &str, kind: Kind) -> String {
    format!(
        "{}\\AppData\\Local\\Packages\\{family}\\{}",
        crate::paths::profile_windows(),
        kind.dir()
    )
}

/// Dossier Linux de `%LOCALAPPDATA%\Packages`.
fn packages_root() -> std::path::PathBuf {
    crate::paths::to_linux(&format!(
        "{}\\AppData\\Local\\Packages",
        crate::paths::profile_windows()
    ))
}

/// Crée le dossier s'il manque. Le dossier de la famille et le sous-dossier
/// sont réservés au compte (0700), même s'ils existaient avant.
pub fn ensure_private_dir(family: &str, kind: Kind) -> std::io::Result<()> {
    ensure_private_dir_in(&packages_root(), family, kind)
}

fn ensure_private_dir_in(root: &std::path::Path, family: &str, kind: Kind) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let family_dir = root.join(family);
    let leaf = family_dir.join(kind.dir());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&leaf)?;
    for d in [&family_dir, &leaf] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

// ============================================================
//  StorageFolder : IStorageFolder (défaut) et IStorageItem
// ============================================================

/// Objet à deux tables : IStorageFolder à l'adresse de l'objet,
/// IStorageItem huit octets plus loin.
#[repr(C)]
struct Folder {
    vt_folder: &'static VTable<16>,
    vt_item: &'static VTable<16>,
    refs: AtomicU32,
    path: String,
}

unsafe fn folder_of(this: usize, item: bool) -> &'static Folder {
    &*((if item { this - 8 } else { this }) as *const Folder)
}

fn new_folder(path: String) -> usize {
    Box::into_raw(Box::new(Folder {
        vt_folder: &FOLDER_VTBL,
        vt_item: &ITEM_VTBL,
        refs: AtomicU32::new(1),
        path,
    })) as usize
}

unsafe fn folder_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_STORAGE_ITEM {
        base + 8
    } else if [
        IID_STORAGE_FOLDER,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        0
    };
    if *out == 0 {
        return E_NOINTERFACE;
    }
    folder_of(base, false).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}

unsafe fn folder_release(base: usize) -> u32 {
    let f = folder_of(base, false);
    let n = f.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Folder));
    }
    n
}

unsafe extern "win64" fn f_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    folder_qi(this, iid, out)
}
unsafe extern "win64" fn f_add_ref(this: usize) -> u32 {
    folder_of(this, false).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn f_release(this: usize) -> u32 {
    folder_release(this)
}
unsafe extern "win64" fn i_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    folder_qi(this - 8, iid, out)
}
unsafe extern "win64" fn i_add_ref(this: usize) -> u32 {
    folder_of(this, true).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn i_release(this: usize) -> u32 {
    folder_release(this - 8)
}

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}
unsafe extern "win64" fn folder_class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(FOLDER_CLASS, out)
}

/// Méthodes asynchrones d'IStorageFolder (CreateFileAsync…).
extern "win64" fn folder_async(_this: usize) -> i32 {
    not_implemented("StorageFolder (méthode asynchrone)")
}
extern "win64" fn item_async(_this: usize) -> i32 {
    not_implemented("IStorageItem (méthode asynchrone)")
}

unsafe extern "win64" fn item_name(this: usize, out: *mut usize) -> i32 {
    let p = &folder_of(this, true).path;
    hstring(p.rsplit('\\').next().unwrap_or(p), out)
}
unsafe extern "win64" fn item_path(this: usize, out: *mut usize) -> i32 {
    hstring(&folder_of(this, true).path, out)
}
unsafe extern "win64" fn item_attributes(_this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = FILE_ATTRIBUTE_DIRECTORY;
    S_OK
}
/// DateCreated : DateTime.UniversalTime, unités de 100 ns depuis 1601.
unsafe extern "win64" fn item_date_created(this: usize, out: *mut i64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let path = crate::paths::to_linux(&folder_of(this, true).path);
    let t = std::fs::metadata(path)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64 / 100 + 116_444_736_000_000_000);
    *out = t;
    S_OK
}
unsafe extern "win64" fn item_is_of_type(_this: usize, kind: u32, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = u8::from(kind == STORAGE_ITEM_FOLDER);
    S_OK
}

static FOLDER_VTBL: VTable<16> = VTable([
    f_qi as *const (),
    f_add_ref as *const (),
    f_release as *const (),
    get_iids as *const (),
    folder_class_name as *const (),
    trust_level as *const (),
    folder_async as *const (), // CreateFileAsyncOverloadDefaultOptions
    folder_async as *const (), // CreateFileAsync
    folder_async as *const (), // CreateFolderAsyncOverloadDefaultOptions
    folder_async as *const (), // CreateFolderAsync
    folder_async as *const (), // GetFileAsync
    folder_async as *const (), // GetFolderAsync
    folder_async as *const (), // GetItemAsync
    folder_async as *const (), // GetFilesAsync…
    folder_async as *const (), // GetFoldersAsync…
    folder_async as *const (), // GetItemsAsync…
]);

static ITEM_VTBL: VTable<16> = VTable([
    i_qi as *const (),
    i_add_ref as *const (),
    i_release as *const (),
    get_iids as *const (),
    folder_class_name as *const (),
    trust_level as *const (),
    item_async as *const (), // RenameAsyncOverloadDefaultOptions
    item_async as *const (), // RenameAsync
    item_async as *const (), // DeleteAsyncOverloadDefaultOptions
    item_async as *const (), // DeleteAsync
    item_async as *const (), // GetBasicPropertiesAsync
    item_name as *const (),
    item_path as *const (),
    item_attributes as *const (),
    item_date_created as *const (),
    item_is_of_type as *const (),
]);

// ============================================================
//  ApplicationData : IApplicationData, 2 et 3
// ============================================================

/// Objet unique du processus : trois tables aux décalages 0, 8 et 16.
#[repr(C)]
struct AppData {
    vt1: &'static VTable<19>,
    vt2: &'static VTable<7>,
    vt3: &'static VTable<9>,
}

static APP_DATA: AppData = AppData {
    vt1: &APP_DATA_VTBL,
    vt2: &APP_DATA2_VTBL,
    vt3: &APP_DATA3_VTBL,
};

fn app_data_base() -> usize {
    std::ptr::addr_of!(APP_DATA) as usize
}

unsafe fn app_data_qi(iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let base = app_data_base();
    *out = if iid == IID_APPLICATION_DATA2 {
        base + 8
    } else if iid == IID_APPLICATION_DATA3 {
        base + 16
    } else if [
        IID_APPLICATION_DATA,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        0
    };
    if *out == 0 {
        E_NOINTERFACE
    } else {
        S_OK
    }
}

unsafe extern "win64" fn ad_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    app_data_qi(iid, out)
}
extern "win64" fn ad_add_ref(_this: usize) -> u32 {
    1
}
extern "win64" fn ad_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn ad_class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(CLASS, out)
}

/// StorageFolder d'un dossier du paquet, créé au premier usage.
unsafe fn folder_getter(kind: Kind, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(family) = current_family() else {
        return E_NO_PACKAGE;
    };
    if let Err(e) = ensure_private_dir(&family, kind) {
        crate::log_warn!(
            "com",
            "ApplicationData : dossier {} non créé : {e}",
            kind.dir()
        );
        return E_ACCESS_DENIED;
    }
    *out = new_folder(folder_path(&family, kind));
    S_OK
}

unsafe extern "win64" fn ad_version(_this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
extern "win64" fn ad_set_version_async(_this: usize) -> i32 {
    not_implemented("SetVersionAsync")
}
extern "win64" fn ad_clear_all_async(_this: usize) -> i32 {
    not_implemented("ClearAllAsync")
}
extern "win64" fn ad_clear_async(_this: usize) -> i32 {
    not_implemented("ClearAsync")
}
unsafe extern "win64" fn ad_local_settings(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("LocalSettings")
}
unsafe extern "win64" fn ad_roaming_settings(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("RoamingSettings")
}
unsafe extern "win64" fn ad_local_folder(_this: usize, out: *mut usize) -> i32 {
    folder_getter(Kind::Local, out)
}
unsafe extern "win64" fn ad_roaming_folder(_this: usize, out: *mut usize) -> i32 {
    folder_getter(Kind::Roaming, out)
}
unsafe extern "win64" fn ad_temporary_folder(_this: usize, out: *mut usize) -> i32 {
    folder_getter(Kind::Temp, out)
}
/// DataChanged : l'abonnement est accepté ; Winlux ne synchronise pas de
/// données itinérantes entre appareils, l'évènement ne vient donc jamais
/// de lui-même.
unsafe extern "win64" fn ad_add_data_changed(
    _this: usize,
    _handler: usize,
    token: *mut i64,
) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    static NEXT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
    *token = NEXT.fetch_add(1, Ordering::SeqCst);
    S_OK
}
extern "win64" fn ad_remove_data_changed(_this: usize, _token: i64) -> i32 {
    S_OK
}
extern "win64" fn ad_signal_data_changed(_this: usize) -> i32 {
    not_implemented("SignalDataChanged")
}
/// RoamingStorageQuota (Ko) : aucune donnée itinérante synchronisée.
unsafe extern "win64" fn ad_roaming_quota(_this: usize, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

unsafe extern "win64" fn ad2_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    app_data_qi(iid, out)
}
unsafe extern "win64" fn ad2_local_cache_folder(_this: usize, out: *mut usize) -> i32 {
    folder_getter(Kind::LocalCache, out)
}
unsafe extern "win64" fn ad3_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    app_data_qi(iid, out)
}
unsafe extern "win64" fn ad3_publisher_cache(_this: usize, _name: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    not_implemented("GetPublisherCacheFolder")
}
extern "win64" fn ad3_clear_publisher_cache(_this: usize) -> i32 {
    not_implemented("ClearPublisherCacheFolderAsync")
}
/// SharedLocalFolder : nul tant que la stratégie de groupe ne l'active pas
/// (documentation d'ApplicationData.SharedLocalFolder).
unsafe extern "win64" fn ad3_shared_local_folder(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

static APP_DATA_VTBL: VTable<19> = VTable([
    ad_qi as *const (),
    ad_add_ref as *const (),
    ad_release as *const (),
    get_iids as *const (),
    ad_class_name as *const (),
    trust_level as *const (),
    ad_version as *const (),
    ad_set_version_async as *const (),
    ad_clear_all_async as *const (),
    ad_clear_async as *const (),
    ad_local_settings as *const (),
    ad_roaming_settings as *const (),
    ad_local_folder as *const (),
    ad_roaming_folder as *const (),
    ad_temporary_folder as *const (),
    ad_add_data_changed as *const (),
    ad_remove_data_changed as *const (),
    ad_signal_data_changed as *const (),
    ad_roaming_quota as *const (),
]);

static APP_DATA2_VTBL: VTable<7> = VTable([
    ad2_qi as *const (),
    ad_add_ref as *const (),
    ad_release as *const (),
    get_iids as *const (),
    ad_class_name as *const (),
    trust_level as *const (),
    ad2_local_cache_folder as *const (),
]);

static APP_DATA3_VTBL: VTable<9> = VTable([
    ad3_qi as *const (),
    ad_add_ref as *const (),
    ad_release as *const (),
    get_iids as *const (),
    ad_class_name as *const (),
    trust_level as *const (),
    ad3_publisher_cache as *const (),
    ad3_clear_publisher_cache as *const (),
    ad3_shared_local_folder as *const (),
]);

// ============================================================
//  Fabrique : IApplicationDataStatics
// ============================================================

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<7>,
}
static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};
static STATICS_VTBL: VTable<7> = VTable([
    st_qi as *const (),
    ad_add_ref as *const (),
    ad_release as *const (),
    get_iids as *const (),
    ad_class_name as *const (),
    trust_level as *const (),
    st_current as *const (),
]);

unsafe extern "win64" fn st_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_APPLICATION_DATA_STATICS,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = std::ptr::addr_of!(STATICS) as usize;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}

/// ApplicationData.Current : l'objet du paquet, ou APPMODEL_ERROR_NO_PACKAGE.
unsafe extern "win64" fn st_current(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if current_family().is_none() {
        *out = 0;
        return E_NO_PACKAGE;
    }
    *out = app_data_base();
    S_OK
}

/// `RoGetActivationFactory` pour `Windows.Storage.ApplicationData`, `None`
/// pour une autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(st_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    unsafe fn call_get(obj: usize, slot: usize, out: *mut usize) -> i32 {
        let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
            std::mem::transmute(*(*(obj as *const *const usize)).add(slot));
        f(obj, out)
    }

    #[test]
    fn fabrique_et_hors_paquet() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_APPLICATION_DATA_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert_ne!(f, 0);
            assert!(activation_factory(
                "Windows.Storage.StorageFile",
                IID_IUNKNOWN.as_ptr(),
                &mut f
            )
            .is_none());
            // Le programme de test n'est pas empaqueté.
            let mut cur = 1usize;
            assert_eq!(st_current(f, &mut cur), E_NO_PACKAGE);
            assert_eq!(cur, 0);
            let mut folder = 1usize;
            assert_eq!(ad_local_folder(app_data_base(), &mut folder), E_NO_PACKAGE);
        }
    }

    #[test]
    fn interfaces_d_application_data() {
        unsafe {
            let base = app_data_base();
            let mut p = 0usize;
            assert_eq!(app_data_qi(IID_APPLICATION_DATA2.as_ptr(), &mut p), S_OK);
            assert_eq!(p, base + 8);
            assert_eq!(app_data_qi(IID_APPLICATION_DATA3.as_ptr(), &mut p), S_OK);
            assert_eq!(p, base + 16);
            let mut shared = 1usize;
            assert_eq!(call_get(p, 8, &mut shared), S_OK);
            assert_eq!(shared, 0);
            let mut v = 9u32;
            assert_eq!(ad_version(base, &mut v), S_OK);
            assert_eq!(v, 0);
            let mut settings = 1usize;
            assert_eq!(call_get(base, 10, &mut settings), E_NOTIMPL);
            assert_eq!(settings, 0);
            assert_eq!(
                app_data_qi(IID_STORAGE_ITEM.as_ptr(), &mut p),
                E_NOINTERFACE
            );
        }
    }

    #[test]
    fn dossier_de_paquet_prive_et_storage_item() {
        let _teb = crate::process::test_teb();
        let dir = std::env::temp_dir().join(format!("winlux-2122-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let family = "Winlux.Essai_abcdefghjkmnp";
        assert!(folder_path(family, Kind::Local)
            .ends_with("\\AppData\\Local\\Packages\\Winlux.Essai_abcdefghjkmnp\\LocalState"));
        ensure_private_dir_in(&dir, family, Kind::Local).unwrap();
        let leaf = dir.join(family).join("LocalState");
        for d in [&leaf, &dir.join(family)] {
            let mode = std::fs::metadata(d).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{d:?}");
        }
        // Un dossier existant trop ouvert est refermé.
        std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private_dir_in(&dir, family, Kind::Local).unwrap();
        assert_eq!(
            std::fs::metadata(&leaf).unwrap().permissions().mode() & 0o777,
            0o700
        );

        unsafe {
            let path = folder_path(family, Kind::Local);
            let folder = new_folder(path.clone());
            let mut item = 0usize;
            assert_eq!(f_qi(folder, IID_STORAGE_ITEM.as_ptr(), &mut item), S_OK);
            assert_eq!(item, folder + 8);
            let mut h = 0usize;
            assert_eq!(call_get(item, 12, &mut h), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h)),
                path
            );
            assert_eq!(call_get(item, 11, &mut h), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h)),
                "LocalState"
            );
            let mut attrs = 0u32;
            assert_eq!(item_attributes(item, &mut attrs), S_OK);
            assert_eq!(attrs, FILE_ATTRIBUTE_DIRECTORY);
            let mut is_folder = 0u8;
            assert_eq!(
                item_is_of_type(item, STORAGE_ITEM_FOLDER, &mut is_folder),
                S_OK
            );
            assert_eq!(is_folder, 1);
            let mut created = 0i64;
            // Dossier absent sur cette machine de test : date nulle.
            assert_eq!(item_date_created(item, &mut created), S_OK);
            assert!(created >= 0);
            let mut back = 0usize;
            assert_eq!(i_qi(item, IID_STORAGE_FOLDER.as_ptr(), &mut back), S_OK);
            assert_eq!(back, folder);
            // Trois références : création, deux QueryInterface.
            assert_eq!(i_release(item), 2);
            assert_eq!(f_release(back), 1);
            assert_eq!(f_release(folder), 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
