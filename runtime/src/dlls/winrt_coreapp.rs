//! Classe Windows Runtime `Windows.ApplicationModel.Core.CoreApplication`
//! (#2681, Refs #2511).
//!
//! Teams demande la fabrique de cette classe avec l'interface statique
//! `ICoreApplication`. Quand elle est inconnue, C++/WinRT lève
//! `hresult_class_not_registered`, et Teams meurt en une seconde (passe de
//! vérification du marshaleur, #2674).
//!
//! La documentation publique dit : « This API is supported in Universal
//! Windows Platform apps only, except for the Properties property, which is
//! always available. » Une application de bureau comme Teams n'a donc droit
//! qu'à `Properties`. L'interface est servie entière, d'après ce que dit la
//! documentation de chaque membre :
//! - `Id` : l'identifiant d'application, relatif au paquet, du processus ;
//! - `Properties` : un ensemble de propriétés (`IPropertySet`) que
//!   l'application et ses cadriciels utilisent tant que le processus vit.
//!   C'est un seul objet par processus. Il porte `IPropertySet`,
//!   `IObservableMap<String, Object>`, `IMap<String, Object>` et
//!   `IIterable<IKeyValuePair<String, Object>>`, avec la vue
//!   `IMapView<String, Object>`, le parcours et l'événement `MapChanged` ;
//! - `Suspending` et `Resuming` : l'abonnement est accepté, mais
//!   l'événement n'est jamais levé, car un programme de bureau n'est pas
//!   suspendu par Winlux ;
//! - `GetCurrentView` : « If no such view exists, then the method fails ».
//!   Un programme de bureau n'a pas de CoreApplicationView, donc l'appel
//!   échoue toujours ;
//! - `Run` et `RunWithActivationFactories` : réservées aux applications
//!   UWP, elles échouent.
//!
//! Choix de Winlux là où la documentation se tait :
//! - `GetCurrentView` rend E_FAIL ;
//! - `Run` et `RunWithActivationFactories` rendent E_ILLEGAL_METHOD_CALL ;
//! - `Id` d'un programme qui n'est pas empaqueté rend
//!   HRESULT_FROM_WIN32(APPMODEL_ERROR_NO_PACKAGE), comme les fonctions
//!   d'identité de paquet ;
//! - `Lookup` et `Remove` d'une clé absente rendent E_BOUNDS ;
//! - le parcours suit l'ordre des clés ; un parcours fait une copie, et
//!   ne voit donc pas les changements faits ensuite.
//!
//! Sources :
//! - la documentation publique de Microsoft (`CoreApplication`,
//!   `CoreApplication.Properties`, `IPropertySet`, `IMap`, `IMapView`,
//!   `IObservableMap`, `IIterator`, `CollectionChange`) ;
//! - pour les identifiants d'interface et l'ordre des méthodes,
//!   windows-rs 0.58, `Windows/ApplicationModel/Core` et
//!   `Windows/Foundation/Collections` (licence MIT) ;
//! - les identifiants des interfaces paramétrées sont calculés selon le
//!   système de types publié de Windows Runtime (`pinterface_iid`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, pinterface_iid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL,
    IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;
const E_ILLEGAL_METHOD_CALL: i32 = 0x8000_000Eu32 as i32;
/// HRESULT_FROM_WIN32(APPMODEL_ERROR_NO_PACKAGE = 15700).
const E_NO_PACKAGE: i32 = 0x8007_3D54u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
/// IActivationFactory {00000035-0000-0000-C000-000000000046}.
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
const IID_CORE_APPLICATION: Guid = g(0x0AAC_F7A4, 0x5E1D, 0x49DF, 0x8034_FB6A_68BC_5ED1);
const IID_PROPERTY_SET: Guid = g(0x8A43_ED9F, 0xF4E6, 0x4421, 0xACF9_1DAB_2986_820C);

const CLASS: &str = "Windows.ApplicationModel.Core.CoreApplication";
const CLASS_SET: &str = "Windows.Foundation.Collections.PropertySet";

// ============================================================
//  Identifiants des interfaces paramétrées
// ============================================================

struct Generic {
    map: Guid,
    map_view: Guid,
    observable: Guid,
    iterable: Guid,
    iterator: Guid,
    kvp: Guid,
    changed_args: Guid,
}

fn generic() -> &'static Generic {
    static G: OnceLock<Generic> = OnceLock::new();
    G.get_or_init(|| {
        let object = "cinterface(IInspectable)";
        let kvp = format!("pinterface({{02b51929-c1c4-4a7e-8940-0312b5c18500}};string;{object})");
        Generic {
            map: pinterface_iid(&format!(
                "pinterface({{3c2925fe-8519-45c1-aa79-197b6718c1c1}};string;{object})"
            )),
            map_view: pinterface_iid(&format!(
                "pinterface({{e480ce40-a338-4ada-adcf-272272e48cb9}};string;{object})"
            )),
            observable: pinterface_iid(&format!(
                "pinterface({{65df2bf5-bf39-41b5-aebc-5a9d865e472b}};string;{object})"
            )),
            iterable: pinterface_iid(&format!(
                "pinterface({{faa585ea-6214-4217-afda-7f46de5869b3}};{kvp})"
            )),
            iterator: pinterface_iid(&format!(
                "pinterface({{6a79e863-4300-459a-9966-cbb660963ee1}};{kvp})"
            )),
            kvp: pinterface_iid(&kvp),
            changed_args: pinterface_iid(
                "pinterface({9939f4df-050a-4c0f-aa60-77075f9c4777};string)",
            ),
        }
    })
}

// ============================================================
//  Outils
// ============================================================

unsafe fn hstr_in(h: usize) -> String {
    String::from_utf16_lossy(super::electron_win32::hstring_slice(h))
}
unsafe fn hstr_out(s: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

/// Appelle la méthode `n` de la table d'un objet COM.
unsafe fn slot(obj: usize, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}
unsafe fn add_ref(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 1));
        f(obj);
    }
}
unsafe fn release(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 2));
        f(obj);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn read_iid(iid: *const u8, out: *mut usize) -> Result<Guid, i32> {
    if out.is_null() {
        return Err(E_POINTER);
    }
    match read_guid(iid) {
        Some(g) => Ok(g),
        None => {
            *out = 0;
            Err(E_INVALIDARG)
        }
    }
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

const W: usize = std::mem::size_of::<usize>();

// ============================================================
//  Identifiant d'application
// ============================================================

/// Identifiant `<Application Id>` du programme `exe` (chemin Windows), dans
/// un paquet installé dans `dir`. L'application dont l'exécutable est ce
/// programme ; à défaut, la seule application du paquet.
fn pick_app(apps: &[crate::setup::msix::App], dir: &str, exe: &str) -> Option<String> {
    let norm = |s: &str| s.replace('/', "\\").to_ascii_lowercase();
    let exe = norm(exe);
    let rel = exe
        .strip_prefix(&format!("{}\\", norm(dir).trim_end_matches('\\')))
        .unwrap_or(&exe)
        .to_string();
    apps.iter()
        .find(|a| norm(&a.executable) == rel)
        .or_else(|| (apps.len() == 1).then(|| &apps[0]))
        .map(|a| a.id.clone())
}

fn app_id() -> Option<String> {
    let (pkg, dir) = super::kernel32_misc::current_package()?;
    let exe = crate::loader::main_module()?.path;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    pick_app(&pkg.apps, dir, &crate::paths::to_windows(&exe))
}

// ============================================================
//  PropertySet
// ============================================================

/// Cinq interfaces sur un même objet : `this` pointe sur le champ de la
/// table demandée.
#[repr(C)]
struct SetObj {
    v_set: &'static VTable<6>,
    v_observable: &'static VTable<8>,
    v_map: &'static VTable<13>,
    v_iterable: &'static VTable<7>,
    refs: AtomicU32,
    /// Valeurs IInspectable retenues (AddRef), ou 0 pour une valeur nulle.
    items: Mutex<BTreeMap<String, usize>>,
    handlers: Mutex<Vec<(u64, usize)>>,
    /// Nom de classe rendu par GetRuntimeClassName.
    class: &'static str,
    /// Appelé après chaque changement, hors de tout verrou : les réglages
    /// d'ApplicationData s'en servent pour s'enregistrer (#2194).
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

const OFF_OBSERVABLE: usize = W;
const OFF_MAP: usize = 2 * W;
const OFF_ITERABLE: usize = 3 * W;

fn new_set() -> usize {
    new_set_with(CLASS_SET, None)
}

/// Ensemble de propriétés d'une autre classe (les réglages
/// `ApplicationDataContainerSettings`, #2194), avec un rappel après chaque
/// changement. Rend l'interface IPropertySet, avec une référence.
pub(crate) fn new_set_with(
    class: &'static str,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
) -> usize {
    Box::into_raw(Box::new(SetObj {
        v_set: &SET_VTBL,
        v_observable: &OBSERVABLE_VTBL,
        v_map: &MAP_VTBL,
        v_iterable: &ITERABLE_VTBL,
        refs: AtomicU32::new(1),
        items: Mutex::new(BTreeMap::new()),
        handlers: Mutex::new(Vec::new()),
        class,
        on_change,
    })) as usize
}

/// IMap<String, Object>, pour les tests des modules qui utilisent
/// `new_set_with`.
#[cfg(test)]
pub(crate) fn map_iid() -> Guid {
    generic().map
}

/// Paires d'un ensemble créé par `new_set_with`, chaque valeur retenue une
/// fois de plus.
pub(crate) unsafe fn set_entries(base: usize) -> Vec<(String, usize)> {
    snapshot(base)
}

/// Pose une valeur sans lever MapChanged ni appeler le rappel (relecture
/// des réglages enregistrés) ; la référence de `value` est reprise.
pub(crate) unsafe fn set_put_quiet(base: usize, key: String, value: usize) {
    if let Some(old) = lock(&set(base).items).insert(key, value) {
        release(old);
    }
}

/// Retenir et libérer un ensemble par son interface IPropertySet.
pub(crate) unsafe fn set_add_ref_ptr(base: usize) {
    set_add_ref_base(base);
}
pub(crate) unsafe fn set_release_ptr(base: usize) {
    set_release_base(base);
}

unsafe fn set<'a>(base: usize) -> &'a SetObj {
    &*(base as *const SetObj)
}

unsafe fn set_qi_base(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let gen = generic();
    let p = if [
        IID_PROPERTY_SET,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else if iid == gen.observable {
        base + OFF_OBSERVABLE
    } else if iid == gen.map {
        base + OFF_MAP
    } else if iid == gen.iterable {
        base + OFF_ITERABLE
    } else {
        super::comobj::log_no_interface("PropertySet", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    set(base).refs.fetch_add(1, Ordering::Relaxed);
    *out = p;
    S_OK
}
unsafe fn set_add_ref_base(base: usize) -> u32 {
    set(base).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe fn set_release_base(base: usize) -> u32 {
    let n = set(base).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let obj = Box::from_raw(base as *mut SetObj);
        for (_, v) in std::mem::take(&mut *lock(&obj.items)) {
            release(v);
        }
        for (_, h) in std::mem::take(&mut *lock(&obj.handlers)) {
            release(h);
        }
    }
    n
}

macro_rules! set_thunks {
    ($qi:ident, $add:ident, $rel:ident, $off:expr) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
            set_qi_base(this - $off, iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            set_add_ref_base(this - $off)
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            set_release_base(this - $off)
        }
    };
}
set_thunks!(set_qi, set_add_ref, set_release, 0);
set_thunks!(obs_qi, obs_add_ref, obs_release, OFF_OBSERVABLE);
set_thunks!(map_qi, map_add_ref, map_release, OFF_MAP);
set_thunks!(itb_qi, itb_add_ref, itb_release, OFF_ITERABLE);

/// Les quatre tables partagent ce nom : `this` est ramené à l'objet par sa
/// table.
unsafe fn set_base_of(this: usize) -> usize {
    let vt = *(this as *const usize);
    if vt == &OBSERVABLE_VTBL as *const _ as usize {
        this - OFF_OBSERVABLE
    } else if vt == &MAP_VTBL as *const _ as usize {
        this - OFF_MAP
    } else if vt == &ITERABLE_VTBL as *const _ as usize {
        this - OFF_ITERABLE
    } else {
        this
    }
}
unsafe extern "win64" fn set_class_name(this: usize, out: *mut usize) -> i32 {
    hstr_out(set(set_base_of(this)).class, out)
}

/// Lève `MapChanged` vers chaque abonné, hors de tout verrou, après avoir
/// appelé le rappel de changement.
unsafe fn raise(base: usize, change: i32, key: &str) {
    if let Some(f) = set(base).on_change.as_ref() {
        f();
    }
    let handlers: Vec<usize> = lock(&set(base).handlers)
        .iter()
        .map(|&(_, h)| {
            add_ref(h);
            h
        })
        .collect();
    if handlers.is_empty() {
        return;
    }
    let args = new_changed_args(change, key);
    for h in handlers {
        // MapChangedEventHandler<String, Object>::Invoke(sender, args).
        let f: unsafe extern "win64" fn(usize, usize, usize) -> i32 =
            std::mem::transmute(slot(h, 3));
        f(h, base + OFF_OBSERVABLE, args);
        release(h);
    }
    release(args);
}

/// CollectionChange : Reset, ItemInserted, ItemRemoved, ItemChanged.
const CHANGE_RESET: i32 = 0;
const CHANGE_INSERTED: i32 = 1;
const CHANGE_REMOVED: i32 = 2;
const CHANGE_CHANGED: i32 = 3;

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

unsafe extern "win64" fn obs_map_changed(this: usize, handler: usize, token: *mut u64) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    if handler == 0 {
        return E_INVALIDARG;
    }
    add_ref(handler);
    let t = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    lock(&set(this - OFF_OBSERVABLE).handlers).push((t, handler));
    *token = t;
    S_OK
}
unsafe extern "win64" fn obs_remove_map_changed(this: usize, token: u64) -> i32 {
    let removed = {
        let mut hs = lock(&set(this - OFF_OBSERVABLE).handlers);
        hs.iter()
            .position(|&(t, _)| t == token)
            .map(|i| hs.remove(i).1)
    };
    if let Some(h) = removed {
        release(h);
    }
    S_OK
}

unsafe extern "win64" fn map_lookup(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match lock(&set(this - OFF_MAP).items).get(&hstr_in(key)) {
        Some(&v) => {
            add_ref(v);
            *out = v;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn map_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&set(this - OFF_MAP).items).len() as u32;
    S_OK
}
unsafe extern "win64" fn map_has_key(this: usize, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&set(this - OFF_MAP).items).contains_key(&hstr_in(key)) as u8;
    S_OK
}
unsafe extern "win64" fn map_get_view(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_view(snapshot(this - OFF_MAP));
    S_OK
}
unsafe extern "win64" fn map_insert(this: usize, key: usize, value: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = this - OFF_MAP;
    let key = hstr_in(key);
    add_ref(value);
    let old = lock(&set(base).items).insert(key.clone(), value);
    *out = old.is_some() as u8;
    if let Some(old) = old {
        release(old);
    }
    raise(
        base,
        if *out != 0 {
            CHANGE_CHANGED
        } else {
            CHANGE_INSERTED
        },
        &key,
    );
    S_OK
}
unsafe extern "win64" fn map_remove(this: usize, key: usize) -> i32 {
    let base = this - OFF_MAP;
    let key = hstr_in(key);
    let old = lock(&set(base).items).remove(&key);
    match old {
        Some(v) => {
            release(v);
            raise(base, CHANGE_REMOVED, &key);
            S_OK
        }
        None => E_BOUNDS,
    }
}
unsafe extern "win64" fn map_clear(this: usize) -> i32 {
    let base = this - OFF_MAP;
    for (_, v) in std::mem::take(&mut *lock(&set(base).items)) {
        release(v);
    }
    raise(base, CHANGE_RESET, "");
    S_OK
}

unsafe extern "win64" fn itb_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_iterator(snapshot(this - OFF_ITERABLE));
    S_OK
}

/// Copie des paires, chaque valeur retenue une fois de plus.
unsafe fn snapshot(base: usize) -> Vec<(String, usize)> {
    lock(&set(base).items)
        .iter()
        .map(|(k, &v)| {
            add_ref(v);
            (k.clone(), v)
        })
        .collect()
}

static SET_VTBL: VTable<6> = VTable([
    set_qi as *const (),
    set_add_ref as *const (),
    set_release as *const (),
    get_iids as *const (),
    set_class_name as *const (),
    trust_level as *const (),
]);
static OBSERVABLE_VTBL: VTable<8> = VTable([
    obs_qi as *const (),
    obs_add_ref as *const (),
    obs_release as *const (),
    get_iids as *const (),
    set_class_name as *const (),
    trust_level as *const (),
    obs_map_changed as *const (),
    obs_remove_map_changed as *const (),
]);
static MAP_VTBL: VTable<13> = VTable([
    map_qi as *const (),
    map_add_ref as *const (),
    map_release as *const (),
    get_iids as *const (),
    set_class_name as *const (),
    trust_level as *const (),
    map_lookup as *const (),
    map_size as *const (),
    map_has_key as *const (),
    map_get_view as *const (),
    map_insert as *const (),
    map_remove as *const (),
    map_clear as *const (),
]);
static ITERABLE_VTBL: VTable<7> = VTable([
    itb_qi as *const (),
    itb_add_ref as *const (),
    itb_release as *const (),
    get_iids as *const (),
    set_class_name as *const (),
    trust_level as *const (),
    itb_first as *const (),
]);

// ============================================================
//  Vue figée : IMapView<String, Object> et IIterable
// ============================================================

#[repr(C)]
struct ViewObj {
    v_view: &'static VTable<10>,
    v_iterable: &'static VTable<7>,
    refs: AtomicU32,
    items: Vec<(String, usize)>,
}

fn new_view(items: Vec<(String, usize)>) -> usize {
    Box::into_raw(Box::new(ViewObj {
        v_view: &VIEW_VTBL,
        v_iterable: &VIEW_ITERABLE_VTBL,
        refs: AtomicU32::new(1),
        items,
    })) as usize
}
unsafe fn view<'a>(base: usize) -> &'a ViewObj {
    &*(base as *const ViewObj)
}
unsafe fn view_qi_base(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let gen = generic();
    let p = if [
        gen.map_view,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else if iid == gen.iterable {
        base + W
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    view(base).refs.fetch_add(1, Ordering::Relaxed);
    *out = p;
    S_OK
}
unsafe fn view_release_base(base: usize) -> u32 {
    let n = view(base).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let obj = Box::from_raw(base as *mut ViewObj);
        for &(_, v) in &obj.items {
            release(v);
        }
    }
    n
}
macro_rules! view_thunks {
    ($qi:ident, $add:ident, $rel:ident, $off:expr) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
            view_qi_base(this - $off, iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            view(this - $off).refs.fetch_add(1, Ordering::Relaxed) + 1
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            view_release_base(this - $off)
        }
    };
}
view_thunks!(view_qi, view_add_ref, view_release, 0);
view_thunks!(vit_qi, vit_add_ref, vit_release, W);

unsafe extern "win64" fn view_class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out(
        "Windows.Foundation.Collections.IMapView`2<String, Object>",
        out,
    )
}
unsafe fn view_find(base: usize, key: usize) -> Option<usize> {
    let key = hstr_in(key);
    view(base)
        .items
        .binary_search_by(|(k, _)| k.as_str().cmp(&key))
        .ok()
        .map(|i| view(base).items[i].1)
}
unsafe extern "win64" fn view_lookup(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match view_find(this, key) {
        Some(v) => {
            add_ref(v);
            *out = v;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn view_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = view(this).items.len() as u32;
    S_OK
}
unsafe extern "win64" fn view_has_key(this: usize, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = view_find(this, key).is_some() as u8;
    S_OK
}
/// Split : la documentation permet de rendre deux vues nulles (« If the
/// IMapView instance cannot be split, then both the first and second
/// parameters are null »). Winlux coupe en deux moitiés quand il y a au
/// moins deux éléments.
unsafe extern "win64" fn view_split(this: usize, first: *mut usize, second: *mut usize) -> i32 {
    if first.is_null() || second.is_null() {
        return E_POINTER;
    }
    let items = &view(this).items;
    if items.len() < 2 {
        *first = 0;
        *second = 0;
        return S_OK;
    }
    let half = items.len() / 2;
    let take = |part: &[(String, usize)]| -> Vec<(String, usize)> {
        part.iter()
            .map(|(k, v)| {
                add_ref(*v);
                (k.clone(), *v)
            })
            .collect()
    };
    *first = new_view(take(&items[..half]));
    *second = new_view(take(&items[half..]));
    S_OK
}
unsafe extern "win64" fn vit_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items = view(this - W)
        .items
        .iter()
        .map(|(k, v)| {
            add_ref(*v);
            (k.clone(), *v)
        })
        .collect();
    *out = new_iterator(items);
    S_OK
}

static VIEW_VTBL: VTable<10> = VTable([
    view_qi as *const (),
    view_add_ref as *const (),
    view_release as *const (),
    get_iids as *const (),
    view_class_name as *const (),
    trust_level as *const (),
    view_lookup as *const (),
    view_size as *const (),
    view_has_key as *const (),
    view_split as *const (),
]);
static VIEW_ITERABLE_VTBL: VTable<7> = VTable([
    vit_qi as *const (),
    vit_add_ref as *const (),
    vit_release as *const (),
    get_iids as *const (),
    view_class_name as *const (),
    trust_level as *const (),
    vit_first as *const (),
]);

// ============================================================
//  Parcours : IIterator<IKeyValuePair<String, Object>>
// ============================================================

#[repr(C)]
struct IterObj {
    vtbl: &'static VTable<10>,
    refs: AtomicU32,
    items: Vec<(String, usize)>,
    pos: AtomicU32,
}

fn new_iterator(items: Vec<(String, usize)>) -> usize {
    Box::into_raw(Box::new(IterObj {
        vtbl: &ITERATOR_VTBL,
        refs: AtomicU32::new(1),
        items,
        pos: AtomicU32::new(0),
    })) as usize
}
unsafe fn iter<'a>(this: usize) -> &'a IterObj {
    &*(this as *const IterObj)
}
unsafe extern "win64" fn it_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    if ![
        generic().iterator,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = 0;
        return E_NOINTERFACE;
    }
    iter(this).refs.fetch_add(1, Ordering::Relaxed);
    *out = this;
    S_OK
}
unsafe extern "win64" fn it_add_ref(this: usize) -> u32 {
    iter(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn it_release(this: usize) -> u32 {
    let n = iter(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let obj = Box::from_raw(this as *mut IterObj);
        for &(_, v) in &obj.items {
            release(v);
        }
    }
    n
}
unsafe extern "win64" fn it_class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out("Windows.Foundation.Collections.IIterator`1", out)
}
unsafe fn it_pair(this: usize, i: usize) -> usize {
    let (k, v) = &iter(this).items[i];
    add_ref(*v);
    new_pair(k.clone(), *v)
}
/// Current : E_BOUNDS une fois la fin atteinte (« If the iterator is
/// at the end of the collection, … HasCurrent is false »).
unsafe extern "win64" fn it_current(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let pos = iter(this).pos.load(Ordering::Relaxed) as usize;
    if pos >= iter(this).items.len() {
        *out = 0;
        return E_BOUNDS;
    }
    *out = it_pair(this, pos);
    S_OK
}
unsafe extern "win64" fn it_has_current(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = ((iter(this).pos.load(Ordering::Relaxed) as usize) < iter(this).items.len()) as u8;
    S_OK
}
unsafe extern "win64" fn it_move_next(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let len = iter(this).items.len();
    let pos = (iter(this).pos.load(Ordering::Relaxed) as usize + 1).min(len);
    iter(this).pos.store(pos as u32, Ordering::Relaxed);
    *out = (pos < len) as u8;
    S_OK
}
/// GetMany : remplit au plus `capacity` paires à partir de la position
/// courante, et avance d'autant.
unsafe extern "win64" fn it_get_many(
    this: usize,
    capacity: u32,
    items: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (capacity > 0 && items.is_null()) {
        return E_POINTER;
    }
    let len = iter(this).items.len();
    let pos = iter(this).pos.load(Ordering::Relaxed) as usize;
    let n = (len.saturating_sub(pos)).min(capacity as usize);
    for i in 0..n {
        *items.add(i) = it_pair(this, pos + i);
    }
    iter(this).pos.store((pos + n) as u32, Ordering::Relaxed);
    *actual = n as u32;
    S_OK
}

static ITERATOR_VTBL: VTable<10> = VTable([
    it_qi as *const (),
    it_add_ref as *const (),
    it_release as *const (),
    get_iids as *const (),
    it_class_name as *const (),
    trust_level as *const (),
    it_current as *const (),
    it_has_current as *const (),
    it_move_next as *const (),
    it_get_many as *const (),
]);

// ============================================================
//  IKeyValuePair<String, Object>
// ============================================================

#[repr(C)]
struct PairObj {
    vtbl: &'static VTable<8>,
    refs: AtomicU32,
    key: String,
    value: usize,
}

fn new_pair(key: String, value: usize) -> usize {
    Box::into_raw(Box::new(PairObj {
        vtbl: &PAIR_VTBL,
        refs: AtomicU32::new(1),
        key,
        value,
    })) as usize
}
unsafe fn pair<'a>(this: usize) -> &'a PairObj {
    &*(this as *const PairObj)
}
unsafe extern "win64" fn pair_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    if ![
        generic().kvp,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = 0;
        return E_NOINTERFACE;
    }
    pair(this).refs.fetch_add(1, Ordering::Relaxed);
    *out = this;
    S_OK
}
unsafe extern "win64" fn pair_add_ref(this: usize) -> u32 {
    pair(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn pair_release(this: usize) -> u32 {
    let n = pair(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let obj = Box::from_raw(this as *mut PairObj);
        release(obj.value);
    }
    n
}
unsafe extern "win64" fn pair_class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out("Windows.Foundation.Collections.IKeyValuePair`2", out)
}
unsafe extern "win64" fn pair_key(this: usize, out: *mut usize) -> i32 {
    hstr_out(&pair(this).key, out)
}
unsafe extern "win64" fn pair_value(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let v = pair(this).value;
    add_ref(v);
    *out = v;
    S_OK
}

static PAIR_VTBL: VTable<8> = VTable([
    pair_qi as *const (),
    pair_add_ref as *const (),
    pair_release as *const (),
    get_iids as *const (),
    pair_class_name as *const (),
    trust_level as *const (),
    pair_key as *const (),
    pair_value as *const (),
]);

// ============================================================
//  IMapChangedEventArgs<String>
// ============================================================

#[repr(C)]
struct ArgsObj {
    vtbl: &'static VTable<8>,
    refs: AtomicU32,
    change: i32,
    key: String,
}

fn new_changed_args(change: i32, key: &str) -> usize {
    Box::into_raw(Box::new(ArgsObj {
        vtbl: &ARGS_VTBL,
        refs: AtomicU32::new(1),
        change,
        key: key.to_string(),
    })) as usize
}
unsafe fn args<'a>(this: usize) -> &'a ArgsObj {
    &*(this as *const ArgsObj)
}
unsafe extern "win64" fn args_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    if ![
        generic().changed_args,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = 0;
        return E_NOINTERFACE;
    }
    args(this).refs.fetch_add(1, Ordering::Relaxed);
    *out = this;
    S_OK
}
unsafe extern "win64" fn args_add_ref(this: usize) -> u32 {
    args(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn args_release(this: usize) -> u32 {
    let n = args(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut ArgsObj));
    }
    n
}
unsafe extern "win64" fn args_class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out("Windows.Foundation.Collections.IMapChangedEventArgs`1", out)
}
unsafe extern "win64" fn args_change(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = args(this).change;
    S_OK
}
unsafe extern "win64" fn args_key(this: usize, out: *mut usize) -> i32 {
    hstr_out(&args(this).key, out)
}

static ARGS_VTBL: VTable<8> = VTable([
    args_qi as *const (),
    args_add_ref as *const (),
    args_release as *const (),
    get_iids as *const (),
    args_class_name as *const (),
    trust_level as *const (),
    args_change as *const (),
    args_key as *const (),
]);

// ============================================================
//  Fabrique (statique) : IActivationFactory et ICoreApplication
// ============================================================

#[repr(C)]
struct Factory {
    v_activation: &'static VTable<7>,
    v_core: &'static VTable<15>,
}

static FACTORY: Factory = Factory {
    v_activation: &ACTIVATION_VTBL,
    v_core: &CORE_VTBL,
};

fn factory_base() -> usize {
    std::ptr::addr_of!(FACTORY) as usize
}

unsafe extern "win64" fn fac_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let base = factory_base();
    *out = if [
        IID_ACTIVATION_FACTORY,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else if iid == IID_CORE_APPLICATION {
        base + W
    } else {
        super::comobj::log_no_interface("CoreApplication", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    S_OK
}
/// Fabrique statique : jamais libérée.
extern "win64" fn fac_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn fac_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn fac_class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out(CLASS, out)
}
/// ActivateInstance : CoreApplication est une classe statique.
unsafe extern "win64" fn fac_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

unsafe extern "win64" fn core_id(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match app_id() {
        Some(id) => hstr_out(&id, out),
        None => {
            *out = 0;
            E_NO_PACKAGE
        }
    }
}
/// Suspending et Resuming : abonnement accepté, jamais levé.
unsafe extern "win64" fn core_add_event(_this: usize, handler: usize, token: *mut u64) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    if handler == 0 {
        return E_INVALIDARG;
    }
    *token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn core_remove_event(_this: usize, _token: u64) -> i32 {
    S_OK
}
/// L'ensemble de propriétés du processus, créé au premier appel.
fn properties() -> usize {
    static SET: OnceLock<usize> = OnceLock::new();
    *SET.get_or_init(new_set)
}
unsafe extern "win64" fn core_properties(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = properties();
    set_add_ref_base(base);
    *out = base;
    S_OK
}
unsafe extern "win64" fn core_get_current_view(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_FAIL
}
unsafe extern "win64" fn core_run(_this: usize, _source: usize) -> i32 {
    E_ILLEGAL_METHOD_CALL
}

static ACTIVATION_VTBL: VTable<7> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    fac_class_name as *const (),
    trust_level as *const (),
    fac_activate as *const (),
]);
/// ICoreApplication : Id, Suspending, RemoveSuspending, Resuming,
/// RemoveResuming, Properties, GetCurrentView, Run,
/// RunWithActivationFactories.
static CORE_VTBL: VTable<15> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    fac_class_name as *const (),
    trust_level as *const (),
    core_id as *const (),
    core_add_event as *const (),
    core_remove_event as *const (),
    core_add_event as *const (),
    core_remove_event as *const (),
    core_properties as *const (),
    core_get_current_view as *const (),
    core_run as *const (),
    core_run as *const (),
]);

/// `RoGetActivationFactory` pour CoreApplication, `None` pour une autre
/// classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(fac_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<T: Copy>(obj: usize, n: usize) -> T {
        let p = slot(obj, n);
        std::mem::transmute_copy(&p)
    }
    fn hs(s: &str) -> usize {
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut h = 0usize;
        unsafe {
            super::super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut h)
        };
        h
    }
    unsafe fn qi(obj: usize, iid: &Guid) -> usize {
        let mut out = 0usize;
        let f: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = call(obj, 0);
        assert_eq!(f(obj, iid.as_ptr(), &mut out), S_OK);
        out
    }

    /// Identifiants calculés des interfaces paramétrées, comparés aux
    /// valeurs que publie le SDK pour PropertySet.
    #[test]
    fn identifiants_parametres() {
        let gen = generic();
        let s = |g: &Guid| super::super::comobj::guid_str(g).to_ascii_lowercase();
        assert_eq!(s(&gen.map), "{1b0d3570-0877-5ec2-8a2c-3b9539506aca}");
        assert_eq!(s(&gen.observable), "{236aac9d-fb12-5c4d-a41c-9e445fb4d7ec}");
        assert_eq!(s(&gen.iterable), "{fe2f3d47-5d47-5499-8374-430c7cda0204}");
    }

    #[test]
    fn identifiant_d_application() {
        use crate::setup::msix::App;
        let app = |id: &str, exe: &str| App {
            id: id.into(),
            executable: exe.into(),
            display_name: String::new(),
            hidden: false,
        };
        let apps = [
            app("MSTeams", "ms-teams.exe"),
            app("Outil", "bin/outil.exe"),
        ];
        let dir = "C:\\Program Files\\WindowsApps\\P_1.0_x64__x";
        assert_eq!(
            pick_app(&apps, dir, &format!("{dir}\\MS-Teams.exe")).as_deref(),
            Some("MSTeams")
        );
        assert_eq!(
            pick_app(&apps, dir, &format!("{dir}\\bin\\outil.exe")).as_deref(),
            Some("Outil")
        );
        assert_eq!(pick_app(&apps, dir, &format!("{dir}\\autre.exe")), None);
        assert_eq!(
            pick_app(&apps[..1], dir, &format!("{dir}\\autre.exe")).as_deref(),
            Some("MSTeams")
        );
    }

    static CHANGES: Mutex<Vec<(i32, String)>> = Mutex::new(Vec::new());
    unsafe extern "win64" fn on_changed(_h: usize, sender: usize, a: usize) -> i32 {
        assert_ne!(sender, 0);
        let mut c = 0i32;
        let mut k = 0usize;
        assert_eq!(args_change(a, &mut c), S_OK);
        assert_eq!(args_key(a, &mut k), S_OK);
        CHANGES.lock().unwrap().push((c, hstr_in(k)));
        S_OK
    }
    extern "win64" fn noop_ref(_h: usize) -> u32 {
        1
    }

    /// #2511 : la fabrique sert ICoreApplication ; Properties rend le même
    /// ensemble à chaque appel, qui range, rend, parcourt, retire et
    /// prévient ses abonnés comme le dit la documentation publique.
    #[test]
    fn core_application_proprietes_2681() {
        unsafe {
            let mut fac = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_CORE_APPLICATION.as_ptr(), &mut fac),
                Some(S_OK)
            );
            assert!(
                activation_factory("Autre.Classe", IID_CORE_APPLICATION.as_ptr(), &mut fac)
                    .is_none()
            );
            // Méthodes réservées aux applications UWP.
            let mut v = 7usize;
            assert_eq!(core_get_current_view(fac, &mut v), E_FAIL);
            assert_eq!(v, 0);
            assert_eq!(core_run(fac, 0), E_ILLEGAL_METHOD_CALL);
            let mut t = 0u64;
            assert_eq!(core_add_event(fac, 0x1234, &mut t), S_OK);
            assert_ne!(t, 0);
            assert_eq!(core_remove_event(fac, t), S_OK);
            // Le test ne tourne pas dans un paquet.
            let mut id = 7usize;
            assert_eq!(core_id(fac, &mut id), E_NO_PACKAGE);

            let mut p1 = 0usize;
            let mut p2 = 0usize;
            let props: unsafe extern "win64" fn(usize, *mut usize) -> i32 = call(fac, 11);
            assert_eq!(props(fac, &mut p1), S_OK);
            assert_eq!(props(fac, &mut p2), S_OK);
            assert_eq!(p1, p2);
            set_release_base(p2);

            // Un ensemble neuf pour le reste, sans toucher celui du processus.
            let base = new_set();
            let gen = generic();
            let map = qi(base, &gen.map);
            assert_eq!(map, base + OFF_MAP);
            let obs = qi(base, &gen.observable);
            let itb = qi(base, &gen.iterable);
            assert_eq!(qi(map, &IID_PROPERTY_SET), base);

            let handler_vt: &'static [usize] = Box::leak(
                vec![
                    0,
                    noop_ref as *const () as usize,
                    noop_ref as *const () as usize,
                    on_changed as *const () as usize,
                ]
                .into_boxed_slice(),
            );
            let handler: &'static *const usize = Box::leak(Box::new(handler_vt.as_ptr()));
            let handler = handler as *const *const usize as usize;
            let mut tok = 0u64;
            assert_eq!(obs_map_changed(obs, handler, &mut tok), S_OK);

            // Insert : nouvelle clé, puis remplacement.
            let value = new_set(); // un objet IInspectable quelconque
            let mut replaced = 7u8;
            assert_eq!(map_insert(map, hs("b"), value, &mut replaced), S_OK);
            assert_eq!(replaced, 0);
            assert_eq!(map_insert(map, hs("a"), 0, &mut replaced), S_OK);
            assert_eq!(map_insert(map, hs("a"), value, &mut replaced), S_OK);
            assert_eq!(replaced, 1);
            let mut n = 0u32;
            assert_eq!(map_size(map, &mut n), S_OK);
            assert_eq!(n, 2);
            let mut has = 0u8;
            assert_eq!(map_has_key(map, hs("a"), &mut has), S_OK);
            assert_eq!(has, 1);
            let mut got = 0usize;
            assert_eq!(map_lookup(map, hs("b"), &mut got), S_OK);
            assert_eq!(got, value);
            release(got);
            assert_eq!(map_lookup(map, hs("zz"), &mut got), E_BOUNDS);
            assert_eq!(got, 0);

            // Parcours, dans l'ordre des clés.
            let mut it = 0usize;
            assert_eq!(itb_first(itb, &mut it), S_OK);
            let mut keys = Vec::new();
            loop {
                let mut hc = 0u8;
                assert_eq!(it_has_current(it, &mut hc), S_OK);
                if hc == 0 {
                    break;
                }
                let mut kv = 0usize;
                assert_eq!(it_current(it, &mut kv), S_OK);
                let mut k = 0usize;
                assert_eq!(pair_key(kv, &mut k), S_OK);
                keys.push(hstr_in(k));
                pair_release(kv);
                let mut more = 0u8;
                assert_eq!(it_move_next(it, &mut more), S_OK);
            }
            assert_eq!(keys, ["a", "b"]);
            let mut kv = 7usize;
            assert_eq!(it_current(it, &mut kv), E_BOUNDS);
            it_release(it);

            // Vue figée et Split.
            let mut v = 0usize;
            assert_eq!(map_get_view(map, &mut v), S_OK);
            assert_eq!(view_size(v, &mut n), S_OK);
            assert_eq!(n, 2);
            assert_eq!(view_has_key(v, hs("b"), &mut has), S_OK);
            assert_eq!(has, 1);
            let (mut f, mut s) = (0usize, 0usize);
            assert_eq!(view_split(v, &mut f, &mut s), S_OK);
            assert_eq!((view(f).items.len(), view(s).items.len()), (1, 1));
            view_release_base(f);
            view_release_base(s);
            let mut it = 0usize;
            assert_eq!(vit_first(v + W, &mut it), S_OK);
            let mut many = [0usize; 4];
            let mut got_n = 0u32;
            assert_eq!(it_get_many(it, 4, many.as_mut_ptr(), &mut got_n), S_OK);
            assert_eq!(got_n, 2);
            for p in &many[..2] {
                pair_release(*p);
            }
            it_release(it);
            view_release_base(v);

            // Retraits.
            assert_eq!(map_remove(map, hs("a")), S_OK);
            assert_eq!(map_remove(map, hs("a")), E_BOUNDS);
            assert_eq!(map_clear(map), S_OK);
            assert_eq!(map_size(map, &mut n), S_OK);
            assert_eq!(n, 0);

            assert_eq!(obs_remove_map_changed(obs, tok), S_OK);
            assert_eq!(map_insert(map, hs("c"), 0, &mut replaced), S_OK);
            assert_eq!(
                *CHANGES.lock().unwrap(),
                [
                    (CHANGE_INSERTED, "b".to_string()),
                    (CHANGE_INSERTED, "a".to_string()),
                    (CHANGE_CHANGED, "a".to_string()),
                    (CHANGE_REMOVED, "a".to_string()),
                    (CHANGE_RESET, String::new()),
                ]
            );

            set_release_base(value);
            release(itb);
            release(obs);
            release(map);
            set_release_base(base);
        }
    }
}
