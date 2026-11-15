//! Classe Windows Runtime `Windows.Networking.Connectivity.NetworkInformation`
//! (#2215) : fabrique INetworkInformationStatics, profils de connexion
//! (ConnectionProfile : IConnectionProfile) et coût de connexion
//! (ConnectionCost : IConnectionCost, IConnectionCost2). Teams et son SDK de
//! télémétrie la lisent au démarrage ; sans elle, Teams s'arrête sur un
//! hresult_class_not_registered non rattrapé.
//!
//! L'état servi est celui de Linux, lu comme le fait netlistmgr.rs : une
//! route par défaut vaut « InternetAccess », une autre route hors boucle
//! locale « LocalAccess », aucune route « pas de profil ». Le profil porte le
//! nom de l'interface de la route. Le coût est « Unrestricted », sans
//! itinérance ni limite de données : Winlux n'a pas de forfait mobile à lire.
//! NetworkStatusChanged rend un jeton sans jamais appeler le gestionnaire.
//!
//! Pas encore servies (E_NOTIMPL, journalisé une fois chacune) :
//! GetLanIdentifiers, GetHostNames, GetProxyConfigurationAsync,
//! GetSortedEndpointPairs, et sur un profil GetNetworkNames,
//! GetDataPlanStatus, NetworkAdapter, GetLocalUsage…,
//! NetworkSecuritySettings.
//!
//! Sources : documentation publique Microsoft (NetworkInformation,
//! GetInternetConnectionProfile « or null if there's no connection profile
//! with a suitable connection », GetConnectionProfiles « The list may be
//! empty », NetworkConnectivityLevel None = 0 … InternetAccess = 3,
//! NetworkCostType Unknown = 0 … Variable = 3, ConnectionCost) ; IID et ordre
//! des méthodes publiés par windows-rs (MIT). Les IID d'IVectorView,
//! IIterable et IIterator<ConnectionProfile> suivent l'algorithme public des
//! pinterface (UUID v5), le même qui donne la valeur publiée de
//! IVectorView<String>.

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Mutex;

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_STATICS: Guid = g(0x5074_f851, 0x950d, 0x4165, 0x9c15_3656_1948_1eea);
const IID_CONNECTION_PROFILE: Guid = g(0x71ba_143c, 0x598e, 0x49d0, 0x84eb_8feb_aedc_c195);
const IID_CONNECTION_COST: Guid = g(0xbad7_d829, 0x3416, 0x4b10, 0xa202_bac0_b075_bdae);
const IID_CONNECTION_COST2: Guid = g(0x8e11_3a05, 0xe209, 0x4549, 0xbb25_5e0d_b691_cb05);
const IID_VIEW_PROFILE: Guid = g(0x6db1_b492, 0x3852, 0x5df8, 0xa29d_6944_002f_58d4);
const IID_ITERABLE_PROFILE: Guid = g(0x34da_bef9, 0x87d0, 0x5b1c, 0xa7ac_9d29_0ade_b0c8);
const IID_ITERATOR_PROFILE: Guid = g(0x8991_3732, 0xa08b, 0x5cb2, 0xaf16_bbbb_2223_839e);

const CLASS: &str = "Windows.Networking.Connectivity.NetworkInformation";
const PROFILE_CLASS: &str = "Windows.Networking.Connectivity.ConnectionProfile";
const COST_CLASS: &str = "Windows.Networking.Connectivity.ConnectionCost";
const VIEW_CLASS: &str =
    "Windows.Foundation.Collections.IVectorView`1<Windows.Networking.Connectivity.ConnectionProfile>";

/// NetworkConnectivityLevel.
const LOCAL_ACCESS: i32 = 1;
const INTERNET_ACCESS: i32 = 3;
/// NetworkCostType.Unrestricted.
const COST_UNRESTRICTED: i32 = 1;

static NEXT_TOKEN: AtomicI64 = AtomicI64::new(1);

/// Méthodes non servies (une ligne de journal chacune).
fn not_implemented(method: &str) -> i32 {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.iter().any(|m| m == method) {
        seen.push(method.to_string());
        crate::log_warn!(
            "com",
            "NetworkInformation : {method} pas encore servie (E_NOTIMPL)"
        );
    }
    E_NOTIMPL
}

/// Interfaces refusées (une ligne de journal par IID et par objet).
fn refused(object: &str, iid: &Guid) {
    crate::log_warn!(
        "com",
        "NetworkInformation : {object} refuse QueryInterface {} (E_NOINTERFACE)",
        super::comobj::guid_str(iid)
    );
}

unsafe fn hstring(text: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

unsafe fn put<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    out.write(v);
    S_OK
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
    put(level, 0) // BaseTrust
}

// ============================================================
//  État du réseau
// ============================================================

/// Profil de la session : (nom de l'interface, niveau de connectivité), ou
/// None sans route hors boucle locale.
fn current_profile() -> Option<(String, i32)> {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    profile_from(&read("/proc/net/route"), &read("/proc/net/ipv6_route"))
}

fn profile_from(route: &str, ipv6_route: &str) -> Option<(String, i32)> {
    let level = super::netlistmgr::connectivity_from(route, ipv6_route);
    if level == 0 {
        return None;
    }
    let internet = level & super::netlistmgr::NLM_INTERNET != 0;
    // Interface de la route par défaut, sinon la première route active.
    let mut name = None;
    for l in route.lines().skip(1) {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 4 || f[0] == "lo" {
            continue;
        }
        if f[1] == "00000000" || name.is_none() {
            name = Some(f[0].to_string());
        }
        if f[1] == "00000000" {
            break;
        }
    }
    if name.is_none() {
        name = ipv6_route
            .lines()
            .filter_map(|l| l.split_whitespace().nth(9))
            .find(|i| *i != "lo")
            .map(str::to_string);
    }
    Some((
        name.unwrap_or_default(),
        if internet {
            INTERNET_ACCESS
        } else {
            LOCAL_ACCESS
        },
    ))
}

// ============================================================
//  Objets à une seule interface WinRT, comptés
// ============================================================

#[repr(C)]
struct Object {
    vt: *const (),
    refs: AtomicU32,
    kind: Kind,
}

enum Kind {
    Profile {
        name: String,
        level: i32,
    },
    /// Itérateur sur une vue de profils, qu'il tient par une référence.
    Iter {
        view: usize,
        pos: AtomicU32,
    },
}

fn boxed<const N: usize>(vt: &'static VTable<N>, kind: Kind) -> usize {
    Box::into_raw(Box::new(Object {
        vt: vt as *const VTable<N> as *const (),
        refs: AtomicU32::new(1),
        kind,
    })) as usize
}

unsafe fn obj<'a>(this: usize) -> &'a Object {
    &*(this as *const Object)
}

unsafe fn release_obj(this: usize) -> u32 {
    let n = obj(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let o = Box::from_raw(this as *mut Object);
        if let Kind::Iter { view, .. } = o.kind {
            view_release(view);
        }
    }
    n
}

unsafe extern "win64" fn o_add_ref(this: usize) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn o_release(this: usize) -> u32 {
    release_obj(this)
}

unsafe fn qi_single(this: usize, iid: *const u8, out: *mut usize, own: &[Guid], name: &str) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if own.contains(&iid) || [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) {
        obj(this).refs.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        refused(name, &iid);
        E_NOINTERFACE
    }
}

// --- ConnectionProfile ---------------------------------------------------

unsafe extern "win64" fn p_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    qi_single(
        this,
        iid,
        out,
        &[IID_CONNECTION_PROFILE],
        "ConnectionProfile",
    )
}
unsafe extern "win64" fn p_class(_this: usize, out: *mut usize) -> i32 {
    hstring(PROFILE_CLASS, out)
}
unsafe extern "win64" fn p_name(this: usize, out: *mut usize) -> i32 {
    match &obj(this).kind {
        Kind::Profile { name, .. } => hstring(name, out),
        _ => E_INVALIDARG,
    }
}
unsafe extern "win64" fn p_level(this: usize, out: *mut i32) -> i32 {
    match &obj(this).kind {
        Kind::Profile { level, .. } => put(out, *level),
        _ => E_INVALIDARG,
    }
}
unsafe extern "win64" fn p_cost(_this: usize, out: *mut usize) -> i32 {
    put(out, new_cost())
}
extern "win64" fn p_network_names(_this: usize) -> i32 {
    not_implemented("ConnectionProfile.GetNetworkNames")
}
extern "win64" fn p_data_plan(_this: usize) -> i32 {
    not_implemented("ConnectionProfile.GetDataPlanStatus")
}
extern "win64" fn p_adapter(_this: usize) -> i32 {
    not_implemented("ConnectionProfile.NetworkAdapter")
}
extern "win64" fn p_usage(_this: usize) -> i32 {
    not_implemented("ConnectionProfile.GetLocalUsage")
}
extern "win64" fn p_security(_this: usize) -> i32 {
    not_implemented("ConnectionProfile.NetworkSecuritySettings")
}

static PROFILE_VTBL: VTable<15> = VTable([
    p_qi as *const (),
    o_add_ref as *const (),
    o_release as *const (),
    get_iids as *const (),
    p_class as *const (),
    trust_level as *const (),
    p_name as *const (),
    p_level as *const (),
    p_network_names as *const (),
    p_cost as *const (),
    p_data_plan as *const (),
    p_adapter as *const (),
    p_usage as *const (), // GetLocalUsage
    p_usage as *const (), // GetLocalUsagePerRoamingStates
    p_security as *const (),
]);

fn new_profile(name: String, level: i32) -> usize {
    boxed(&PROFILE_VTBL, Kind::Profile { name, level })
}

// --- ConnectionCost : IConnectionCost à l'objet, IConnectionCost2 à +8 ---

#[repr(C)]
struct Cost {
    vt_cost: &'static VTable<10>,
    vt_cost2: &'static VTable<7>,
    refs: AtomicU32,
}

fn new_cost() -> usize {
    Box::into_raw(Box::new(Cost {
        vt_cost: &COST_VTBL,
        vt_cost2: &COST2_VTBL,
        refs: AtomicU32::new(1),
    })) as usize
}
unsafe fn cost_of<'a>(base: usize) -> &'a Cost {
    &*(base as *const Cost)
}
unsafe fn cost_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_CONNECTION_COST2 {
        base + 8
    } else if [
        IID_CONNECTION_COST,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        refused("ConnectionCost", &iid);
        return E_NOINTERFACE;
    };
    cost_of(base).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}
unsafe fn cost_release(base: usize) -> u32 {
    let n = cost_of(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Cost));
    }
    n
}
unsafe extern "win64" fn c_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    cost_qi(this, iid, out)
}
unsafe extern "win64" fn c_add_ref(this: usize) -> u32 {
    cost_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn c_release(this: usize) -> u32 {
    cost_release(this)
}
unsafe extern "win64" fn c2_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    cost_qi(this - 8, iid, out)
}
unsafe extern "win64" fn c2_add_ref(this: usize) -> u32 {
    c_add_ref(this - 8)
}
unsafe extern "win64" fn c2_release(this: usize) -> u32 {
    cost_release(this - 8)
}
unsafe extern "win64" fn c_class(_this: usize, out: *mut usize) -> i32 {
    hstring(COST_CLASS, out)
}
unsafe extern "win64" fn c_type(_this: usize, out: *mut i32) -> i32 {
    put(out, COST_UNRESTRICTED)
}
unsafe extern "win64" fn c_false(_this: usize, out: *mut u8) -> i32 {
    put(out, 0)
}

static COST_VTBL: VTable<10> = VTable([
    c_qi as *const (),
    c_add_ref as *const (),
    c_release as *const (),
    get_iids as *const (),
    c_class as *const (),
    trust_level as *const (),
    c_type as *const (),  // NetworkCostType
    c_false as *const (), // Roaming
    c_false as *const (), // OverDataLimit
    c_false as *const (), // ApproachingDataLimit
]);
static COST2_VTBL: VTable<7> = VTable([
    c2_qi as *const (),
    c2_add_ref as *const (),
    c2_release as *const (),
    get_iids as *const (),
    c_class as *const (),
    trust_level as *const (),
    c_false as *const (), // BackgroundDataUsageRestricted
]);

// --- IVectorView<ConnectionProfile> et IIterable --------------------------

#[repr(C)]
struct View {
    vt_view: &'static VTable<10>,
    vt_iterable: &'static VTable<7>,
    refs: AtomicU32,
    items: Vec<usize>,
}

unsafe fn view_of<'a>(base: usize) -> &'a View {
    &*(base as *const View)
}
fn new_view(items: Vec<usize>) -> usize {
    Box::into_raw(Box::new(View {
        vt_view: &VIEW_VTBL,
        vt_iterable: &ITERABLE_VTBL,
        refs: AtomicU32::new(1),
        items,
    })) as usize
}
unsafe fn view_release(base: usize) -> u32 {
    let n = view_of(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let v = Box::from_raw(base as *mut View);
        for p in v.items {
            release_obj(p);
        }
    }
    n
}
unsafe fn view_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_ITERABLE_PROFILE {
        base + 8
    } else if [
        IID_VIEW_PROFILE,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        refused("IVectorView<ConnectionProfile>", &iid);
        return E_NOINTERFACE;
    };
    view_of(base).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}
unsafe extern "win64" fn v_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    view_qi(this, iid, out)
}
unsafe extern "win64" fn v_add_ref(this: usize) -> u32 {
    view_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn v_release(this: usize) -> u32 {
    view_release(this)
}
unsafe extern "win64" fn i_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    view_qi(this - 8, iid, out)
}
unsafe extern "win64" fn i_add_ref(this: usize) -> u32 {
    v_add_ref(this - 8)
}
unsafe extern "win64" fn i_release(this: usize) -> u32 {
    view_release(this - 8)
}
unsafe extern "win64" fn view_class(_this: usize, out: *mut usize) -> i32 {
    hstring(VIEW_CLASS, out)
}
unsafe fn item_out(items: &[usize], index: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match items.get(index) {
        Some(&p) => {
            obj(p).refs.fetch_add(1, Ordering::SeqCst);
            *out = p;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn v_get_at(this: usize, index: u32, out: *mut usize) -> i32 {
    item_out(&view_of(this).items, index as usize, out)
}
unsafe extern "win64" fn v_size(this: usize, out: *mut u32) -> i32 {
    put(out, view_of(this).items.len() as u32)
}
unsafe extern "win64" fn v_index_of(
    this: usize,
    value: usize,
    index: *mut u32,
    found: *mut u8,
) -> i32 {
    if index.is_null() || found.is_null() {
        return E_POINTER;
    }
    let pos = view_of(this).items.iter().position(|&p| p == value);
    *index = pos.unwrap_or(0) as u32;
    *found = u8::from(pos.is_some());
    S_OK
}
unsafe fn copy_many(
    items: &[usize],
    start: usize,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (cap > 0 && out.is_null()) {
        return E_POINTER;
    }
    let mut n = 0;
    for (i, _) in items.iter().enumerate().skip(start).take(cap as usize) {
        item_out(items, i, out.add(n));
        n += 1;
    }
    *actual = n as u32;
    S_OK
}
unsafe extern "win64" fn v_get_many(
    this: usize,
    start: u32,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    let items = &view_of(this).items;
    if start as usize > items.len() {
        if !actual.is_null() {
            *actual = 0;
        }
        return E_BOUNDS;
    }
    copy_many(items, start as usize, cap, out, actual)
}
unsafe extern "win64" fn i_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = this - 8;
    view_of(base).refs.fetch_add(1, Ordering::SeqCst);
    *out = boxed(
        &ITERATOR_VTBL,
        Kind::Iter {
            view: base,
            pos: AtomicU32::new(0),
        },
    );
    S_OK
}

static VIEW_VTBL: VTable<10> = VTable([
    v_qi as *const (),
    v_add_ref as *const (),
    v_release as *const (),
    get_iids as *const (),
    view_class as *const (),
    trust_level as *const (),
    v_get_at as *const (),
    v_size as *const (),
    v_index_of as *const (),
    v_get_many as *const (),
]);
static ITERABLE_VTBL: VTable<7> = VTable([
    i_qi as *const (),
    i_add_ref as *const (),
    i_release as *const (),
    get_iids as *const (),
    view_class as *const (),
    trust_level as *const (),
    i_first as *const (),
]);

// --- IIterator<ConnectionProfile> -------------------------------------------

unsafe fn iter_parts<'a>(this: usize) -> (&'a [usize], &'a AtomicU32) {
    match &obj(this).kind {
        Kind::Iter { view, pos } => (&view_of(*view).items, pos),
        _ => unreachable!("objet IIterator sans vue"),
    }
}
unsafe extern "win64" fn r_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    qi_single(
        this,
        iid,
        out,
        &[IID_ITERATOR_PROFILE],
        "IIterator<ConnectionProfile>",
    )
}
unsafe extern "win64" fn r_current(this: usize, out: *mut usize) -> i32 {
    let (items, pos) = iter_parts(this);
    item_out(items, pos.load(Ordering::SeqCst) as usize, out)
}
unsafe extern "win64" fn r_has_current(this: usize, out: *mut u8) -> i32 {
    let (items, pos) = iter_parts(this);
    put(
        out,
        u8::from((pos.load(Ordering::SeqCst) as usize) < items.len()),
    )
}
unsafe extern "win64" fn r_move_next(this: usize, out: *mut u8) -> i32 {
    let (items, pos) = iter_parts(this);
    let len = items.len() as u32;
    let next = (pos.load(Ordering::SeqCst) + 1).min(len);
    pos.store(next, Ordering::SeqCst);
    put(out, u8::from(next < len))
}
unsafe extern "win64" fn r_get_many(
    this: usize,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    let (items, pos) = iter_parts(this);
    let p = pos.load(Ordering::SeqCst);
    let hr = copy_many(items, p as usize, cap, out, actual);
    if hr == S_OK {
        pos.store(p + *actual, Ordering::SeqCst);
    }
    hr
}
static ITERATOR_VTBL: VTable<10> = VTable([
    r_qi as *const (),
    o_add_ref as *const (),
    o_release as *const (),
    get_iids as *const (),
    view_class as *const (),
    trust_level as *const (),
    r_current as *const (),
    r_has_current as *const (),
    r_move_next as *const (),
    r_get_many as *const (),
]);

// ============================================================
//  INetworkInformationStatics
// ============================================================

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<14>,
}
static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};

unsafe extern "win64" fn st_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_STATICS,
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
        refused("la fabrique", &iid);
        E_NOINTERFACE
    }
}
/// Fabrique statique : jamais libérée.
extern "win64" fn st_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn st_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn st_class(_this: usize, out: *mut usize) -> i32 {
    hstring(CLASS, out)
}
unsafe extern "win64" fn st_profiles(_this: usize, out: *mut usize) -> i32 {
    let items = current_profile()
        .map(|(n, l)| vec![new_profile(n, l)])
        .unwrap_or_default();
    put(out, new_view(items))
}
unsafe extern "win64" fn st_internet_profile(_this: usize, out: *mut usize) -> i32 {
    // Sans connexion convenable : S_OK et null, comme le documente Microsoft.
    put(out, current_profile().map_or(0, |(n, l)| new_profile(n, l)))
}
extern "win64" fn st_lan_identifiers(_this: usize) -> i32 {
    not_implemented("GetLanIdentifiers")
}
extern "win64" fn st_host_names(_this: usize) -> i32 {
    not_implemented("GetHostNames")
}
extern "win64" fn st_proxy(_this: usize) -> i32 {
    not_implemented("GetProxyConfigurationAsync")
}
extern "win64" fn st_sorted(_this: usize) -> i32 {
    not_implemented("GetSortedEndpointPairs")
}
unsafe extern "win64" fn st_add_status(_this: usize, handler: usize, token: *mut i64) -> i32 {
    if handler == 0 {
        return E_INVALIDARG;
    }
    put(token, NEXT_TOKEN.fetch_add(1, Ordering::SeqCst))
}
extern "win64" fn st_remove_status(_this: usize, _token: i64) -> i32 {
    S_OK
}

static STATICS_VTBL: VTable<14> = VTable([
    st_qi as *const (),
    st_add_ref as *const (),
    st_release as *const (),
    get_iids as *const (),
    st_class as *const (),
    trust_level as *const (),
    st_profiles as *const (),
    st_internet_profile as *const (),
    st_lan_identifiers as *const (),
    st_host_names as *const (),
    st_proxy as *const (),
    st_sorted as *const (),
    st_add_status as *const (),
    st_remove_status as *const (),
]);

/// `RoGetActivationFactory` pour NetworkInformation, `None` pour une autre
/// classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(st_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn text(h: usize) -> String {
        String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h))
    }

    const ROUTE: &str = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n\
        wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\n\
        wlan0\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\n";
    const LOCAL: &str = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n\
        eth0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\n";
    const NONE: &str = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n";

    #[test]
    fn profil_selon_les_routes_2215() {
        assert_eq!(
            profile_from(ROUTE, ""),
            Some(("wlan0".into(), INTERNET_ACCESS))
        );
        assert_eq!(profile_from(LOCAL, ""), Some(("eth0".into(), LOCAL_ACCESS)));
        assert_eq!(profile_from(NONE, ""), None);
    }

    /// Teams (#2215) : fabrique, profil, coût, vue des profils.
    #[test]
    fn fabrique_profil_et_cout_2215() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            let mut other = 0usize;
            assert!(
                activation_factory("Windows.Other", IID_STATICS.as_ptr(), &mut other).is_none()
            );

            let p = new_profile("wlan0".into(), INTERNET_ACCESS);
            let mut h = 0usize;
            assert_eq!(p_name(p, &mut h), S_OK);
            assert_eq!(text(h), "wlan0");
            let mut level = 0;
            assert_eq!(p_level(p, &mut level), S_OK);
            assert_eq!(level, 3);
            let mut cost = 0usize;
            assert_eq!(p_cost(p, &mut cost), S_OK);
            let mut t = 0;
            assert_eq!(c_type(cost, &mut t), S_OK);
            assert_eq!(t, COST_UNRESTRICTED);
            let mut c2 = 0usize;
            assert_eq!(c_qi(cost, IID_CONNECTION_COST2.as_ptr(), &mut c2), S_OK);
            assert_eq!(c2, cost + 8);
            let mut b = 1u8;
            let get: unsafe extern "win64" fn(usize, *mut u8) -> i32 =
                std::mem::transmute((*(c2 as *const &VTable<7>)).0[6]);
            assert_eq!(get(c2, &mut b), S_OK);
            assert_eq!(b, 0);
            assert_eq!(c2_release(c2), 1);
            assert_eq!(c_release(cost), 0);

            // Vue d'un profil, puis itération.
            let v = new_view(vec![p]);
            let mut n = 0u32;
            assert_eq!(v_size(v, &mut n), S_OK);
            assert_eq!(n, 1);
            let mut got = 0usize;
            assert_eq!(v_get_at(v, 0, &mut got), S_OK);
            assert_eq!(got, p);
            assert_eq!(o_release(got), 1);
            assert_eq!(v_get_at(v, 1, &mut got), E_BOUNDS);
            let mut it = 0usize;
            assert_eq!(i_first(v + 8, &mut it), S_OK);
            let mut has = 0u8;
            assert_eq!(r_has_current(it, &mut has), S_OK);
            assert_eq!(has, 1);
            assert_eq!(r_move_next(it, &mut has), S_OK);
            assert_eq!(has, 0);
            assert_eq!(o_release(it), 0);
            assert_eq!(v_release(v), 0);

            // Méthodes de la fabrique : profils et événement.
            let mut view = 0usize;
            assert_eq!(st_profiles(f, &mut view), S_OK);
            assert_ne!(view, 0);
            v_release(view);
            let mut token = 0i64;
            assert_eq!(st_add_status(f, 0x1000, &mut token), S_OK);
            assert_ne!(token, 0);
            assert_eq!(st_remove_status(f, token), S_OK);
            assert_eq!(st_add_status(f, 0, &mut token), E_INVALIDARG);
        }
    }
}
