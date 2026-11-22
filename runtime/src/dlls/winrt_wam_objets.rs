//! Objets du gestionnaire de comptes web (#2511, étape 1 du plan
//! « Fournisseur de comptes Microsoft pour Teams ») : fournisseur
//! `WebAccountProvider`, requête `WebTokenRequest` et sa fabrique,
//! résultats `FindAllAccountsResult` et `WebTokenRequestResult`, erreur
//! `WebProviderError`, et les collections qu'ils portent.
//!
//! Aucun réseau, aucun compte, aucun jeton : le fournisseur Microsoft est
//! trouvé, il ne connaît encore aucun compte (`FindAllAccountsAsync` rend
//! une liste vide), une demande de jeton en silence répond
//! `UserInteractionRequired`, et une demande de connexion répond
//! `ProviderError` (la connexion n'est pas encore servie).
//!
//! Ce que dit la documentation publique de Microsoft (WebAccountProvider,
//! WebTokenRequest, WebTokenRequestResult, WebTokenRequestStatus,
//! FindAllAccountsResult, FindAllWebAccountsStatus, WebProviderError,
//! « Web Account Manager ») :
//! - un fournisseur a un `Id`, un `DisplayName` et une `Authority` ;
//! - une requête porte son fournisseur, sa portée (`Scope`), son
//!   identifiant de client (`ClientId`), son type d'invite et deux
//!   dictionnaires de chaînes (`Properties`, `AppProperties`) ;
//! - `UserInteractionRequired` dit qu'il faut passer par
//!   `RequestTokenAsync` ; `ProviderError` porte un `WebProviderError`.
//!
//! Choix de Winlux là où la documentation se tait (inférences nommées) :
//! - `DisplayName` rend l'identifiant du fournisseur, sans nom de marque ;
//!   `DisplayPurpose` est vide, `IconUri` et `User` sont nuls ;
//! - `IsSystemProvider` vaut false : ce fournisseur n'est pas celui du
//!   système ;
//! - la connexion refusée rend `ProviderError` avec le code E_NOTIMPL ;
//! - `GetView` des dictionnaires, `WebProviderError.Properties` et
//!   `InvalidateCacheAsync` ne sont pas encore servis (E_NOTIMPL,
//!   journalisé une fois) ; les dictionnaires ne servent pas IIterable.
//!
//! Sources : la documentation citée ; identifiants d'interface et ordre des
//! méthodes d'après windows-rs 0.58 (licence MIT,
//! `Windows/Security/Credentials`, `Windows/Security/Authentication/Web/Core`,
//! `Windows/Foundation/Collections`) ; les identifiants des interfaces
//! paramétrées sont calculés selon le système de types publié de Windows
//! Runtime (`pinterface_iid`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, pinterface_iid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL,
    E_POINTER, IID_IUNKNOWN, S_OK,
};

const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);

const IID_PROVIDER: Guid = g(0x29dc_c8c3, 0x7ab9, 0x4a7c, 0xa336_b942_f9db_f7c7);
const IID_PROVIDER2: Guid = g(0x4a01_eb05, 0x4e42, 0x41d4, 0xb518_e008_a516_3614);
const IID_PROVIDER3: Guid = g(0xda1c_518b, 0x970d, 0x4d49, 0x825c_f270_6f8c_a7fe);
const IID_PROVIDER4: Guid = g(0x718f_d8db, 0xe796, 0x4210, 0xb74e_84d2_9894_b080);
const IID_REQUEST: Guid = g(0xb77b_4d68, 0xadcb, 0x4673, 0xb364_0cf7_b35c_af97);
const IID_REQUEST2: Guid = g(0xd700_c079, 0x30c8, 0x4397, 0x9654_961c_3be8_b855);
const IID_REQUEST3: Guid = g(0x5a75_5b51, 0x3bb1, 0x41a5, 0xa63d_90bc_32c7_db9a);
const IID_REQUEST_FACTORY: Guid = g(0x6cf2_141c, 0x0ff0, 0x4c67, 0xb84f_99dd_be4a_72c9);
const IID_FIND_ALL_RESULT: Guid = g(0xa581_2b5d, 0xb72e, 0x420c, 0x86ab_aac0_d7b7_261f);
const IID_TOKEN_RESULT: Guid = g(0xc12a_8305, 0xd1f8, 0x4483, 0x8d54_38fe_2927_84ff);
const IID_PROVIDER_ERROR: Guid = g(0xdb19_1bb1, 0x50c5, 0x4809, 0x8dca_09c9_9410_245c);

const CLASS_PROVIDER: &str = "Windows.Security.Credentials.WebAccountProvider";
pub(crate) const CLASS_REQUEST: &str = "Windows.Security.Authentication.Web.Core.WebTokenRequest";
const CLASS_FIND_ALL_RESULT: &str =
    "Windows.Security.Authentication.Web.Core.FindAllAccountsResult";
const CLASS_TOKEN_RESULT: &str = "Windows.Security.Authentication.Web.Core.WebTokenRequestResult";
const CLASS_PROVIDER_ERROR: &str = "Windows.Security.Authentication.Web.Core.WebProviderError";

/// WebTokenRequestStatus.
pub(crate) const STATUS_USER_INTERACTION_REQUIRED: i32 = 3;
pub(crate) const STATUS_PROVIDER_ERROR: i32 = 5;

/// Identifiants des interfaces paramétrées.
pub(crate) struct Generic {
    pub(crate) op_find_all: Guid,
    pub(crate) op_token: Guid,
    accounts_view: Guid,
    responses_view: Guid,
    string_map: Guid,
    string_map_view: Guid,
}

pub(crate) fn generic() -> &'static Generic {
    static G: OnceLock<Generic> = OnceLock::new();
    G.get_or_init(|| {
        let op = |rc: &str| {
            pinterface_iid(&format!(
                "pinterface({{9fc2b0bb-e446-44e2-aa61-9cab8f636af2}};{rc})"
            ))
        };
        let view = |rc: &str| {
            pinterface_iid(&format!(
                "pinterface({{bbe1fa4c-b0e3-4583-baef-1f1b2e483e56}};{rc})"
            ))
        };
        Generic {
            op_find_all: op("rc(Windows.Security.Authentication.Web.Core.FindAllAccountsResult;{a5812b5d-b72e-420c-86ab-aac0d7b7261f})"),
            op_token: op("rc(Windows.Security.Authentication.Web.Core.WebTokenRequestResult;{c12a8305-d1f8-4483-8d54-38fe292784ff})"),
            accounts_view: view("rc(Windows.Security.Credentials.WebAccount;{69473eb2-8031-49be-80bb-96cb46d99aba})"),
            responses_view: view("rc(Windows.Security.Authentication.Web.Core.WebTokenResponse;{67a7c5ca-83f6-44c6-a3b1-0eb69e41fa8a})"),
            string_map: pinterface_iid(
                "pinterface({3c2925fe-8519-45c1-aa79-197b6718c1c1};string;string)",
            ),
            string_map_view: pinterface_iid(
                "pinterface({e480ce40-a338-4ada-adcf-272272e48cb9};string;string)",
            ),
        }
    })
}

// ============================================================
//  Outils
// ============================================================

const W: usize = std::mem::size_of::<usize>();

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) unsafe fn hstr_units(h: usize) -> Vec<u16> {
    super::electron_win32::hstring_slice(h).to_vec()
}
unsafe fn hstr_out(units: &[u16], out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    super::electron_win32::WindowsCreateString(units.as_ptr(), units.len() as u32, out)
}
unsafe fn class_out(class: &str, out: *mut usize) -> i32 {
    let w: Vec<u16> = class.encode_utf16().collect();
    hstr_out(&w, out)
}
unsafe fn slot(obj: usize, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}
pub(crate) unsafe fn add_ref(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 1));
        f(obj);
    }
}
pub(crate) unsafe fn release(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 2));
        f(obj);
    }
}
unsafe fn put_ptr(out: *mut usize, v: usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    add_ref(v);
    *out = v;
    S_OK
}
unsafe fn put_null(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
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
unsafe extern "win64" fn no_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
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
const BASE: [Guid; 3] = [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT];

/// Table de N entrées : les six d'IInspectable puis les méthodes.
const fn with<const N: usize>(head: [*const (); 6], tail: &[*const ()]) -> [*const (); N] {
    let mut v = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < 6 {
        v[i] = head[i];
        i += 1;
    }
    while i < N {
        v[i] = tail[i - 6];
        i += 1;
    }
    v
}

/// Objet compté à plusieurs tables : QI, AddRef et Release de la table
/// placée `$off` octets après le début de l'objet.
macro_rules! thunks {
    ($ty:ty, $qi_base:ident, $drop:ident, $qi:ident, $add:ident, $rel:ident, $off:expr) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
            $qi_base(this - $off, iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            (*((this - $off) as *const $ty))
                .refs
                .fetch_add(1, Ordering::AcqRel)
                + 1
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            let base = this - $off;
            let n = (*(base as *const $ty)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
            if n == 0 {
                $drop(base);
            }
            n
        }
    };
}

macro_rules! head {
    ($qi:ident, $add:ident, $rel:ident, $class:ident) => {
        [
            $qi as *const (),
            $add as *const (),
            $rel as *const (),
            no_iids as *const (),
            $class as *const (),
            trust_level as *const (),
        ]
    };
}

// ============================================================
//  WebAccountProvider
// ============================================================

#[repr(C)]
struct Provider {
    v1: &'static VTable<9>,
    v2: &'static VTable<8>,
    v3: &'static VTable<7>,
    v4: &'static VTable<7>,
    refs: AtomicU32,
    id: Vec<u16>,
    authority: Vec<u16>,
}

/// Nouveau fournisseur (interface IWebAccountProvider, une référence).
pub(crate) fn new_provider(id: &[u16], authority: &[u16]) -> usize {
    Box::into_raw(Box::new(Provider {
        v1: &PROVIDER_VTBL,
        v2: &PROVIDER2_VTBL,
        v3: &PROVIDER3_VTBL,
        v4: &PROVIDER4_VTBL,
        refs: AtomicU32::new(1),
        id: id.to_vec(),
        authority: authority.to_vec(),
    })) as usize
}

/// Base d'un fournisseur rendu par Winlux, depuis l'une de ses tables.
unsafe fn provider_base(obj: usize) -> Option<usize> {
    if obj == 0 {
        return None;
    }
    let vt = *(obj as *const usize);
    [
        &PROVIDER_VTBL as *const _ as usize,
        &PROVIDER2_VTBL as *const _ as usize,
        &PROVIDER3_VTBL as *const _ as usize,
        &PROVIDER4_VTBL as *const _ as usize,
    ]
    .iter()
    .position(|&v| v == vt)
    .map(|i| obj - i * W)
}

/// Identifiant et autorité d'un fournisseur rendu par Winlux.
pub(crate) unsafe fn provider_info(obj: usize) -> Option<(String, String)> {
    let p = &*(provider_base(obj)? as *const Provider);
    Some((
        String::from_utf16_lossy(&p.id),
        String::from_utf16_lossy(&p.authority),
    ))
}

unsafe fn provider_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    *out = if iid == IID_PROVIDER || BASE.contains(&iid) {
        base
    } else if iid == IID_PROVIDER2 {
        base + W
    } else if iid == IID_PROVIDER3 {
        base + 2 * W
    } else if iid == IID_PROVIDER4 {
        base + 3 * W
    } else {
        super::comobj::log_no_interface("WebAccountProvider", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    (*(base as *const Provider))
        .refs
        .fetch_add(1, Ordering::AcqRel);
    S_OK
}
unsafe fn provider_drop(base: usize) {
    drop(Box::from_raw(base as *mut Provider));
}
thunks!(
    Provider,
    provider_qi,
    provider_drop,
    p1_qi,
    p1_add,
    p1_rel,
    0
);
thunks!(
    Provider,
    provider_qi,
    provider_drop,
    p2_qi,
    p2_add,
    p2_rel,
    W
);
thunks!(
    Provider,
    provider_qi,
    provider_drop,
    p3_qi,
    p3_add,
    p3_rel,
    2 * W
);
thunks!(
    Provider,
    provider_qi,
    provider_drop,
    p4_qi,
    p4_add,
    p4_rel,
    3 * W
);

unsafe fn prov<'a>(this: usize) -> &'a Provider {
    &*(provider_base(this).unwrap_or(this) as *const Provider)
}
unsafe extern "win64" fn provider_class(_this: usize, out: *mut usize) -> i32 {
    class_out(CLASS_PROVIDER, out)
}
unsafe extern "win64" fn provider_id(this: usize, out: *mut usize) -> i32 {
    hstr_out(&prov(this).id, out)
}
/// DisplayName : l'identifiant, sans nom de marque (choix de Winlux).
unsafe extern "win64" fn provider_display_name(this: usize, out: *mut usize) -> i32 {
    hstr_out(&prov(this).id, out)
}
unsafe extern "win64" fn provider_null(_this: usize, out: *mut usize) -> i32 {
    put_null(out)
}
unsafe extern "win64" fn provider_empty(_this: usize, out: *mut usize) -> i32 {
    hstr_out(&[], out)
}
unsafe extern "win64" fn provider_authority(this: usize, out: *mut usize) -> i32 {
    hstr_out(&prov(this).authority, out)
}
unsafe extern "win64" fn provider_is_system(_this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

/// IWebAccountProvider : Id, DisplayName, IconUri.
static PROVIDER_VTBL: VTable<9> = VTable(with::<9>(
    head!(p1_qi, p1_add, p1_rel, provider_class),
    &[
        provider_id as *const (),
        provider_display_name as *const (),
        provider_null as *const (),
    ],
));
/// IWebAccountProvider2 : DisplayPurpose, Authority.
static PROVIDER2_VTBL: VTable<8> = VTable(with::<8>(
    head!(p2_qi, p2_add, p2_rel, provider_class),
    &[provider_empty as *const (), provider_authority as *const ()],
));
/// IWebAccountProvider3 : User.
static PROVIDER3_VTBL: VTable<7> = VTable(with::<7>(
    head!(p3_qi, p3_add, p3_rel, provider_class),
    &[provider_null as *const ()],
));
/// IWebAccountProvider4 : IsSystemProvider.
static PROVIDER4_VTBL: VTable<7> = VTable(with::<7>(
    head!(p4_qi, p4_add, p4_rel, provider_class),
    &[provider_is_system as *const ()],
));

// ============================================================
//  IMap<String, String>
// ============================================================

#[repr(C)]
struct StrMap {
    v: &'static VTable<13>,
    refs: AtomicU32,
    items: Mutex<BTreeMap<Vec<u16>, Vec<u16>>>,
    /// Vue figée (IMapView) rendue par GetView, en lecture seule.
    view: bool,
}

fn new_str_map() -> usize {
    Box::into_raw(Box::new(StrMap {
        v: &STR_MAP_VTBL,
        refs: AtomicU32::new(1),
        items: Mutex::new(BTreeMap::new()),
        view: false,
    })) as usize
}
unsafe fn smap<'a>(this: usize) -> &'a StrMap {
    &*(this as *const StrMap)
}
unsafe fn str_map_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let own = if smap(base).view {
        generic().string_map_view
    } else {
        generic().string_map
    };
    if iid != own && !BASE.contains(&iid) {
        super::comobj::log_no_interface("IMap<String, String>", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    }
    smap(base).refs.fetch_add(1, Ordering::AcqRel);
    *out = base;
    S_OK
}
unsafe fn str_map_drop(base: usize) {
    drop(Box::from_raw(base as *mut StrMap));
}
thunks!(StrMap, str_map_qi, str_map_drop, sm_qi, sm_add, sm_rel, 0);
unsafe extern "win64" fn str_map_class(this: usize, out: *mut usize) -> i32 {
    if smap(this).view {
        class_out(
            "Windows.Foundation.Collections.IMapView`2<String, String>",
            out,
        )
    } else {
        class_out("Windows.Foundation.Collections.IMap`2<String, String>", out)
    }
}
unsafe extern "win64" fn sm_lookup(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let v = lock(&smap(this).items).get(&hstr_units(key)).cloned();
    match v {
        Some(v) => hstr_out(&v, out),
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn sm_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&smap(this).items).len() as u32;
    S_OK
}
unsafe extern "win64" fn sm_has_key(this: usize, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&smap(this).items).contains_key(&hstr_units(key)) as u8;
    S_OK
}
/// GetView : copie figée du dictionnaire (Learn, IMap.GetView).
unsafe extern "win64" fn sm_get_view(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items = lock(&smap(this).items).clone();
    *out = Box::into_raw(Box::new(StrMap {
        v: &STR_MAP_VIEW_VTBL,
        refs: AtomicU32::new(1),
        items: Mutex::new(items),
        view: true,
    })) as usize;
    S_OK
}
/// IMapView.Split : une vue qui ne se coupe pas rend deux nuls (Learn).
unsafe extern "win64" fn smv_split(_this: usize, first: *mut usize, second: *mut usize) -> i32 {
    if first.is_null() || second.is_null() {
        return E_POINTER;
    }
    *first = 0;
    *second = 0;
    S_OK
}
unsafe extern "win64" fn sm_insert(this: usize, key: usize, value: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let old = lock(&smap(this).items).insert(hstr_units(key), hstr_units(value));
    *out = old.is_some() as u8;
    S_OK
}
unsafe extern "win64" fn sm_remove(this: usize, key: usize) -> i32 {
    match lock(&smap(this).items).remove(&hstr_units(key)) {
        Some(_) => S_OK,
        None => E_BOUNDS,
    }
}
unsafe extern "win64" fn sm_clear(this: usize) -> i32 {
    lock(&smap(this).items).clear();
    S_OK
}
/// IMap<String, String> : Lookup, Size, HasKey, GetView, Insert, Remove,
/// Clear.
static STR_MAP_VTBL: VTable<13> = VTable(with::<13>(
    head!(sm_qi, sm_add, sm_rel, str_map_class),
    &[
        sm_lookup as *const (),
        sm_size as *const (),
        sm_has_key as *const (),
        sm_get_view as *const (),
        sm_insert as *const (),
        sm_remove as *const (),
        sm_clear as *const (),
    ],
));
/// IMapView<String, String> : Lookup, Size, HasKey, Split.
static STR_MAP_VIEW_VTBL: VTable<13> = VTable(with::<13>(
    head!(sm_qi, sm_add, sm_rel, str_map_class),
    &[
        sm_lookup as *const (),
        sm_size as *const (),
        sm_has_key as *const (),
        smv_split as *const (),
        // Au-delà de IMapView : jamais appelées par un client correct.
        pl_beyond as *const (),
        pl_beyond as *const (),
        pl_beyond as *const (),
    ],
));

// ============================================================
//  WebTokenRequest et sa fabrique
// ============================================================

#[repr(C)]
struct Request {
    v1: &'static VTable<11>,
    v2: &'static VTable<7>,
    v3: &'static VTable<8>,
    refs: AtomicU32,
    provider: usize,
    scope: Vec<u16>,
    client_id: Vec<u16>,
    prompt: i32,
    properties: usize,
    app_properties: usize,
    correlation: Mutex<Vec<u16>>,
}

unsafe fn new_request(provider: usize, scope: &[u16], client_id: &[u16], prompt: i32) -> usize {
    add_ref(provider);
    Box::into_raw(Box::new(Request {
        v1: &REQUEST_VTBL,
        v2: &REQUEST2_VTBL,
        v3: &REQUEST3_VTBL,
        refs: AtomicU32::new(1),
        provider,
        scope: scope.to_vec(),
        client_id: client_id.to_vec(),
        prompt,
        properties: new_str_map(),
        app_properties: new_str_map(),
        correlation: Mutex::new(Vec::new()),
    })) as usize
}

unsafe fn request_base(obj: usize) -> Option<usize> {
    if obj == 0 {
        return None;
    }
    let vt = *(obj as *const usize);
    [
        &REQUEST_VTBL as *const _ as usize,
        &REQUEST2_VTBL as *const _ as usize,
        &REQUEST3_VTBL as *const _ as usize,
    ]
    .iter()
    .position(|&v| v == vt)
    .map(|i| obj - i * W)
}

/// Fournisseur d'une requête créée par Winlux (sans référence en plus).
pub(crate) unsafe fn request_provider(obj: usize) -> Option<usize> {
    Some((*(request_base(obj)? as *const Request)).provider)
}

unsafe fn req<'a>(this: usize) -> &'a Request {
    &*(request_base(this).unwrap_or(this) as *const Request)
}
unsafe fn request_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    *out = if iid == IID_REQUEST || BASE.contains(&iid) {
        base
    } else if iid == IID_REQUEST2 {
        base + W
    } else if iid == IID_REQUEST3 {
        base + 2 * W
    } else {
        super::comobj::log_no_interface("WebTokenRequest", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    (*(base as *const Request))
        .refs
        .fetch_add(1, Ordering::AcqRel);
    S_OK
}
unsafe fn request_drop(base: usize) {
    let r = Box::from_raw(base as *mut Request);
    release(r.provider);
    release(r.properties);
    release(r.app_properties);
}
thunks!(Request, request_qi, request_drop, r1_qi, r1_add, r1_rel, 0);
thunks!(Request, request_qi, request_drop, r2_qi, r2_add, r2_rel, W);
thunks!(
    Request,
    request_qi,
    request_drop,
    r3_qi,
    r3_add,
    r3_rel,
    2 * W
);

unsafe extern "win64" fn request_class(_this: usize, out: *mut usize) -> i32 {
    class_out(CLASS_REQUEST, out)
}
unsafe extern "win64" fn request_provider_get(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, req(this).provider)
}
unsafe extern "win64" fn request_scope(this: usize, out: *mut usize) -> i32 {
    hstr_out(&req(this).scope, out)
}
unsafe extern "win64" fn request_client_id(this: usize, out: *mut usize) -> i32 {
    hstr_out(&req(this).client_id, out)
}
unsafe extern "win64" fn request_prompt(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = req(this).prompt;
    S_OK
}
unsafe extern "win64" fn request_properties(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, req(this).properties)
}
unsafe extern "win64" fn request_app_properties(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, req(this).app_properties)
}
unsafe extern "win64" fn request_correlation(this: usize, out: *mut usize) -> i32 {
    let c = lock(&req(this).correlation).clone();
    hstr_out(&c, out)
}
unsafe extern "win64" fn request_set_correlation(this: usize, value: usize) -> i32 {
    *lock(&req(this).correlation) = hstr_units(value);
    S_OK
}

/// IWebTokenRequest : WebAccountProvider, Scope, ClientId, PromptType,
/// Properties.
static REQUEST_VTBL: VTable<11> = VTable(with::<11>(
    head!(r1_qi, r1_add, r1_rel, request_class),
    &[
        request_provider_get as *const (),
        request_scope as *const (),
        request_client_id as *const (),
        request_prompt as *const (),
        request_properties as *const (),
    ],
));
/// IWebTokenRequest2 : AppProperties.
static REQUEST2_VTBL: VTable<7> = VTable(with::<7>(
    head!(r2_qi, r2_add, r2_rel, request_class),
    &[request_app_properties as *const ()],
));
/// IWebTokenRequest3 : CorrelationId, SetCorrelationId.
static REQUEST3_VTBL: VTable<8> = VTable(with::<8>(
    head!(r3_qi, r3_add, r3_rel, request_class),
    &[
        request_correlation as *const (),
        request_set_correlation as *const (),
    ],
));

/// Fabrique de WebTokenRequest : IActivationFactory (pas de constructeur
/// par défaut) et IWebTokenRequestFactory.
#[repr(C)]
struct RequestFactory {
    v_activation: &'static VTable<7>,
    v_factory: &'static VTable<10>,
}
static REQUEST_FACTORY: RequestFactory = RequestFactory {
    v_activation: &REQ_ACTIVATION_VTBL,
    v_factory: &REQ_FACTORY_VTBL,
};
fn request_factory_base() -> usize {
    std::ptr::addr_of!(REQUEST_FACTORY) as usize
}
unsafe extern "win64" fn rf_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let base = request_factory_base();
    *out = if iid == IID_ACTIVATION_FACTORY || BASE.contains(&iid) {
        base
    } else if iid == IID_REQUEST_FACTORY {
        base + W
    } else {
        super::comobj::log_no_interface("WebTokenRequest (fabrique)", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    S_OK
}
/// Fabrique statique : jamais libérée.
extern "win64" fn rf_add(_this: usize) -> u32 {
    2
}
extern "win64" fn rf_rel(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn rf_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe fn create(
    provider: usize,
    scope: usize,
    client: usize,
    prompt: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if provider == 0 {
        return E_INVALIDARG;
    }
    let scope = if scope == 0 {
        Vec::new()
    } else {
        hstr_units(scope)
    };
    let client = if client == 0 {
        Vec::new()
    } else {
        hstr_units(client)
    };
    *out = new_request(provider, &scope, &client, prompt);
    S_OK
}
unsafe extern "win64" fn rf_create(
    _t: usize,
    provider: usize,
    scope: usize,
    client: usize,
    out: *mut usize,
) -> i32 {
    create(provider, scope, client, 0, out)
}
unsafe extern "win64" fn rf_create_prompt(
    _t: usize,
    provider: usize,
    scope: usize,
    client: usize,
    prompt: i32,
    out: *mut usize,
) -> i32 {
    create(provider, scope, client, prompt, out)
}
unsafe extern "win64" fn rf_create_provider(_t: usize, provider: usize, out: *mut usize) -> i32 {
    create(provider, 0, 0, 0, out)
}
unsafe extern "win64" fn rf_create_scope(
    _t: usize,
    provider: usize,
    scope: usize,
    out: *mut usize,
) -> i32 {
    create(provider, scope, 0, 0, out)
}
unsafe extern "win64" fn rf_class(_this: usize, out: *mut usize) -> i32 {
    class_out(CLASS_REQUEST, out)
}
static REQ_ACTIVATION_VTBL: VTable<7> = VTable(with::<7>(
    head!(rf_qi, rf_add, rf_rel, rf_class),
    &[rf_activate as *const ()],
));
/// IWebTokenRequestFactory : Create, CreateWithPromptType,
/// CreateWithProvider, CreateWithScope.
static REQ_FACTORY_VTBL: VTable<10> = VTable(with::<10>(
    head!(rf_qi, rf_add, rf_rel, rf_class),
    &[
        rf_create as *const (),
        rf_create_prompt as *const (),
        rf_create_provider as *const (),
        rf_create_scope as *const (),
    ],
));

/// `RoGetActivationFactory` pour WebTokenRequest, `None` pour une autre
/// classe.
pub(crate) unsafe fn activation_factory(
    class: &str,
    iid: *const u8,
    out: *mut usize,
) -> Option<i32> {
    (class == CLASS_REQUEST).then(|| rf_qi(0, iid, out))
}

// ============================================================
//  Vue de vecteur vide (IVectorView<T>)
// ============================================================

#[repr(C)]
struct EmptyView {
    v: &'static VTable<10>,
    refs: AtomicU32,
    iid: Guid,
}
fn new_empty_view(iid: Guid) -> usize {
    Box::into_raw(Box::new(EmptyView {
        v: &EMPTY_VIEW_VTBL,
        refs: AtomicU32::new(1),
        iid,
    })) as usize
}
unsafe fn empty_view_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let me = &*(base as *const EmptyView);
    if iid != me.iid && !BASE.contains(&iid) {
        super::comobj::log_no_interface("IVectorView (comptes web)", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    }
    me.refs.fetch_add(1, Ordering::AcqRel);
    *out = base;
    S_OK
}
unsafe fn empty_view_drop(base: usize) {
    drop(Box::from_raw(base as *mut EmptyView));
}
thunks!(
    EmptyView,
    empty_view_qi,
    empty_view_drop,
    ev_qi,
    ev_add,
    ev_rel,
    0
);
unsafe extern "win64" fn ev_class(_this: usize, out: *mut usize) -> i32 {
    class_out("Windows.Foundation.Collections.IVectorView`1", out)
}
unsafe extern "win64" fn ev_get_at(_this: usize, _i: u32, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_BOUNDS
}
unsafe extern "win64" fn ev_size(_this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
unsafe extern "win64" fn ev_index_of(
    _this: usize,
    _v: usize,
    index: *mut u32,
    found: *mut u8,
) -> i32 {
    if index.is_null() || found.is_null() {
        return E_POINTER;
    }
    *index = 0;
    *found = 0;
    S_OK
}
unsafe extern "win64" fn ev_get_many(
    _this: usize,
    _start: u32,
    _cap: u32,
    _items: usize,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
/// IVectorView<T> : GetAt, Size, IndexOf, GetMany.
static EMPTY_VIEW_VTBL: VTable<10> = VTable(with::<10>(
    head!(ev_qi, ev_add, ev_rel, ev_class),
    &[
        ev_get_at as *const (),
        ev_size as *const (),
        ev_index_of as *const (),
        ev_get_many as *const (),
    ],
));

// ============================================================
//  Résultats et erreur
// ============================================================

/// Objet à une table : FindAllAccountsResult, WebTokenRequestResult ou
/// WebProviderError.
#[repr(C)]
struct Plain {
    v: &'static VTable<10>,
    refs: AtomicU32,
    iid: Guid,
    class: &'static str,
    /// Collection (Accounts, ResponseData) ou nul.
    items: usize,
    /// Erreur du fournisseur (ResponseError, ProviderError) ou nul.
    error: usize,
    status: i32,
    message: Vec<u16>,
}

struct PlainArgs {
    v: &'static VTable<10>,
    iid: Guid,
    class: &'static str,
    items: usize,
    error: usize,
    status: i32,
}

fn new_plain(a: PlainArgs, message: &str) -> usize {
    Box::into_raw(Box::new(Plain {
        v: a.v,
        refs: AtomicU32::new(1),
        iid: a.iid,
        class: a.class,
        items: a.items,
        error: a.error,
        status: a.status,
        message: message.encode_utf16().collect(),
    })) as usize
}
unsafe fn plain<'a>(this: usize) -> &'a Plain {
    &*(this as *const Plain)
}
unsafe fn plain_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    let iid = match read_iid(iid, out) {
        Ok(g) => g,
        Err(hr) => return hr,
    };
    let me = plain(base);
    if iid != me.iid && !BASE.contains(&iid) {
        super::comobj::log_no_interface(me.class, &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    }
    me.refs.fetch_add(1, Ordering::AcqRel);
    *out = base;
    S_OK
}
unsafe fn plain_drop(base: usize) {
    let p = Box::from_raw(base as *mut Plain);
    release(p.items);
    release(p.error);
}
thunks!(Plain, plain_qi, plain_drop, pl_qi, pl_add, pl_rel, 0);
unsafe extern "win64" fn pl_class(this: usize, out: *mut usize) -> i32 {
    class_out(plain(this).class, out)
}
unsafe extern "win64" fn pl_items(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, plain(this).items)
}
unsafe extern "win64" fn pl_status(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = plain(this).status;
    S_OK
}
unsafe extern "win64" fn pl_error(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, plain(this).error)
}
/// InvalidateCacheAsync : aucun jeton n'est gardé, il n'y a rien à
/// effacer ; l'action rendue est déjà terminée.
unsafe extern "win64" fn pl_invalidate_cache(_this: usize, out: *mut usize) -> i32 {
    super::composition::completed_action(out as *mut *mut std::ffi::c_void)
}
unsafe extern "win64" fn pl_message(this: usize, out: *mut usize) -> i32 {
    hstr_out(&plain(this).message, out)
}
/// Properties : le dictionnaire de l'erreur, le même à chaque appel.
unsafe extern "win64" fn pl_error_properties(this: usize, out: *mut usize) -> i32 {
    put_ptr(out, plain(this).items)
}
/// Entrée au-delà de la dernière méthode de l'interface : jamais appelée
/// par un client correct.
unsafe extern "win64" fn pl_beyond(_this: usize) -> i32 {
    E_NOTIMPL
}

/// IFindAllAccountsResult : Accounts, Status, ProviderError.
static FIND_ALL_VTBL: VTable<10> = VTable(with::<10>(
    head!(pl_qi, pl_add, pl_rel, pl_class),
    &[
        pl_items as *const (),
        pl_status as *const (),
        pl_error as *const (),
        pl_beyond as *const (),
    ],
));
/// IWebTokenRequestResult : ResponseData, ResponseStatus, ResponseError,
/// InvalidateCacheAsync.
static TOKEN_RESULT_VTBL: VTable<10> = VTable(with::<10>(
    head!(pl_qi, pl_add, pl_rel, pl_class),
    &[
        pl_items as *const (),
        pl_status as *const (),
        pl_error as *const (),
        pl_invalidate_cache as *const (),
    ],
));
/// IWebProviderError : ErrorCode, ErrorMessage, Properties.
static PROVIDER_ERROR_VTBL: VTable<10> = VTable(with::<10>(
    head!(pl_qi, pl_add, pl_rel, pl_class),
    &[
        pl_status as *const (),
        pl_message as *const (),
        pl_error_properties as *const (),
        pl_beyond as *const (),
    ],
));

/// FindAllAccountsResult : aucun compte, statut Success (0).
pub(crate) fn no_accounts() -> usize {
    new_plain(
        PlainArgs {
            v: &FIND_ALL_VTBL,
            iid: IID_FIND_ALL_RESULT,
            class: CLASS_FIND_ALL_RESULT,
            items: new_empty_view(generic().accounts_view),
            error: 0,
            status: 0,
        },
        "",
    )
}

/// WebTokenRequestResult sans jeton, de statut `status`. Avec
/// `ProviderError`, l'erreur porte le code E_NOTIMPL et `message`.
pub(crate) fn token_result(status: i32, message: &str) -> usize {
    let error = if status == STATUS_PROVIDER_ERROR {
        new_plain(
            PlainArgs {
                v: &PROVIDER_ERROR_VTBL,
                iid: IID_PROVIDER_ERROR,
                class: CLASS_PROVIDER_ERROR,
                // Properties : dictionnaire vide, gardé par l'erreur.
                items: new_str_map(),
                error: 0,
                status: E_NOTIMPL,
            },
            message,
        )
    } else {
        0
    };
    new_plain(
        PlainArgs {
            v: &TOKEN_RESULT_VTBL,
            iid: IID_TOKEN_RESULT,
            class: CLASS_TOKEN_RESULT,
            items: new_empty_view(generic().responses_view),
            error,
            status,
        },
        "",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs(s: &str) -> usize {
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut h = 0usize;
        unsafe {
            super::super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut h)
        };
        h
    }
    unsafe fn text(h: usize) -> String {
        String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h))
    }
    unsafe fn get_h(obj: usize, n: usize) -> String {
        let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
            std::mem::transmute(slot(obj, n));
        let mut h = 0usize;
        assert_eq!(f(obj, &mut h), S_OK);
        text(h)
    }
    unsafe fn get_p(obj: usize, n: usize) -> usize {
        let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
            std::mem::transmute(slot(obj, n));
        let mut p = 1usize;
        assert_eq!(f(obj, &mut p), S_OK);
        p
    }
    unsafe fn get_i(obj: usize, n: usize) -> i32 {
        let f: unsafe extern "win64" fn(usize, *mut i32) -> i32 = std::mem::transmute(slot(obj, n));
        let mut v = -1i32;
        assert_eq!(f(obj, &mut v), S_OK);
        v
    }
    unsafe fn qi(obj: usize, iid: &Guid) -> usize {
        let f: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
            std::mem::transmute(slot(obj, 0));
        let mut p = 0usize;
        assert_eq!(f(obj, iid.as_ptr(), &mut p), S_OK);
        p
    }

    #[test]
    fn requete_de_jeton_et_fournisseur_2511() {
        unsafe {
            let id: Vec<u16> = "https://login.microsoft.com".encode_utf16().collect();
            let auth: Vec<u16> = "organizations".encode_utf16().collect();
            let p = new_provider(&id, &auth);
            assert_eq!(get_h(p, 6), "https://login.microsoft.com");
            let p2 = qi(p, &IID_PROVIDER2);
            assert_eq!(p2, p + W);
            assert_eq!(get_h(p2, 7), "organizations");
            let p4 = qi(p, &IID_PROVIDER4);
            let f: unsafe extern "win64" fn(usize, *mut u8) -> i32 =
                std::mem::transmute(slot(p4, 6));
            let mut sys = 9u8;
            assert_eq!(f(p4, &mut sys), S_OK);
            assert_eq!(sys, 0);
            assert_eq!(
                provider_info(p2),
                Some(("https://login.microsoft.com".into(), "organizations".into()))
            );

            // La fabrique crée une requête qui garde ses arguments.
            let mut fac = 0usize;
            assert_eq!(
                activation_factory(CLASS_REQUEST, IID_REQUEST_FACTORY.as_ptr(), &mut fac),
                Some(S_OK)
            );
            let create: unsafe extern "win64" fn(
                usize,
                usize,
                usize,
                usize,
                i32,
                *mut usize,
            ) -> i32 = std::mem::transmute(slot(fac, 7));
            let mut r = 0usize;
            assert_eq!(create(fac, 0, hs("s"), hs("c"), 0, &mut r), E_INVALIDARG);
            assert_eq!(
                create(fac, p, hs("openid"), hs("client-x"), 1, &mut r),
                S_OK
            );
            assert_eq!(get_h(r, 7), "openid");
            assert_eq!(get_h(r, 8), "client-x");
            assert_eq!(get_i(r, 9), 1);
            assert_eq!(request_provider(r), Some(p));
            let rp = get_p(r, 6);
            assert_eq!(rp, p);
            release(rp);

            // Properties : un dictionnaire de chaînes.
            let props = get_p(r, 10);
            assert_eq!(qi(props, &generic().string_map), props);
            release(props);
            let ins: unsafe extern "win64" fn(usize, usize, usize, *mut u8) -> i32 =
                std::mem::transmute(slot(props, 10));
            let mut replaced = 9u8;
            assert_eq!(
                ins(props, hs("authority"), hs("https://x/"), &mut replaced),
                S_OK
            );
            assert_eq!(replaced, 0);
            let look: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
                std::mem::transmute(slot(props, 6));
            let mut v = 0usize;
            assert_eq!(look(props, hs("authority"), &mut v), S_OK);
            assert_eq!(text(v), "https://x/");
            assert_eq!(look(props, hs("absent"), &mut v), E_BOUNDS);
            release(props);

            // CorrelationId.
            let r3 = qi(r, &IID_REQUEST3);
            let set: unsafe extern "win64" fn(usize, usize) -> i32 =
                std::mem::transmute(slot(r3, 7));
            assert_eq!(set(r3, hs("abc")), S_OK);
            assert_eq!(get_h(r3, 6), "abc");
            release(r3);
            release(r);
            release(p4);
            release(p2);
            release(p);
        }
    }

    #[test]
    fn resultats_sans_compte_ni_jeton_2511() {
        unsafe {
            let all = no_accounts();
            assert_eq!(get_i(all, 7), 0); // FindAllWebAccountsStatus.Success
            let accounts = get_p(all, 6);
            let size: unsafe extern "win64" fn(usize, *mut u32) -> i32 =
                std::mem::transmute(slot(accounts, 7));
            let mut n = 9u32;
            assert_eq!(size(accounts, &mut n), S_OK);
            assert_eq!(n, 0);
            assert_eq!(qi(accounts, &generic().accounts_view), accounts);
            release(accounts); // celle de la QI
            release(accounts);
            assert_eq!(get_p(all, 8), 0);
            release(all);

            let silent = token_result(STATUS_USER_INTERACTION_REQUIRED, "");
            assert_eq!(get_i(silent, 7), STATUS_USER_INTERACTION_REQUIRED);
            assert_eq!(get_p(silent, 8), 0);
            release(silent);

            let refused = token_result(STATUS_PROVIDER_ERROR, "connexion pas encore servie");
            assert_eq!(get_i(refused, 7), STATUS_PROVIDER_ERROR);
            let err = get_p(refused, 8);
            assert_eq!(get_i(err, 6), E_NOTIMPL);
            assert_eq!(get_h(err, 7), "connexion pas encore servie");
            release(err);
            release(refused);
        }
    }

    #[test]
    fn vue_proprietes_et_cache_2948() {
        unsafe {
            // GetView : vue figée, en lecture seule, qui ne se coupe pas.
            let map = new_str_map();
            let ins: unsafe extern "win64" fn(usize, usize, usize, *mut u8) -> i32 =
                std::mem::transmute(slot(map, 10));
            let mut replaced = 9u8;
            assert_eq!(
                ins(map, hs("authority"), hs("https://x/"), &mut replaced),
                S_OK
            );
            let view = get_p(map, 9);
            assert_eq!(qi(view, &generic().string_map_view), view);
            release(view);
            let q: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
                std::mem::transmute(slot(view, 0));
            let mut no = 7usize;
            assert_eq!(
                q(view, generic().string_map.as_ptr(), &mut no),
                E_NOINTERFACE
            );
            assert_eq!(no, 0);
            assert_eq!(ins(map, hs("après"), hs("x"), &mut replaced), S_OK);
            let size: unsafe extern "win64" fn(usize, *mut u32) -> i32 =
                std::mem::transmute(slot(view, 7));
            let mut n = 0u32;
            assert_eq!(size(view, &mut n), S_OK);
            assert_eq!(n, 1);
            let look: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
                std::mem::transmute(slot(view, 6));
            let mut v = 0usize;
            assert_eq!(look(view, hs("authority"), &mut v), S_OK);
            assert_eq!(text(v), "https://x/");
            let split: unsafe extern "win64" fn(usize, *mut usize, *mut usize) -> i32 =
                std::mem::transmute(slot(view, 9));
            let (mut a, mut b) = (5usize, 5usize);
            assert_eq!(split(view, &mut a, &mut b), S_OK);
            assert_eq!((a, b), (0, 0));
            release(view);
            release(map);

            // Properties de l'erreur : le même dictionnaire à chaque appel.
            let refused = token_result(STATUS_PROVIDER_ERROR, "connexion pas encore servie");
            let err = get_p(refused, 8);
            let props = get_p(err, 8);
            assert_eq!(qi(props, &generic().string_map), props);
            release(props);
            assert_eq!(get_p(err, 8), props);
            release(props);
            release(props);
            release(err);
            // InvalidateCacheAsync : action déjà terminée.
            let inv = get_p(refused, 9);
            assert_ne!(inv, 0);
            release(inv);
            release(refused);
        }
    }
}
