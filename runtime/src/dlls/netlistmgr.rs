//! Gestionnaire de listes de réseaux : CLSID_NetworkListManager
//! (INetworkListManager, IConnectionPointContainer, netlistmgr.h).
//!
//! Qt 5 (QNetworkStatusMonitor, module réseau) crée cet objet au démarrage
//! et s'abonne à ses événements. Quand la classe manquait, Qt notait
//! « Could not get a NetworkListManager instance » puis lisait quand même
//! l'objet absent : iCUE 4.18 plantait dans Qt5Network.dll (#1844).
//!
//! L'état servi est celui de Linux : une route par défaut IPv4 ou IPv6 vaut
//! « Internet », une autre route hors boucle locale vaut « réseau local ».
//! Aucun événement n'est émis ; les abonnés sont gardés puis libérés.
//! L'énumération des réseaux et des connexions est vide.
//!
//! Méthodes, ordre et constantes : documentation Microsoft Learn
//! d'INetworkListManager, IConnectionPointContainer, IConnectionPoint,
//! IEnumNetworks, IEnumNetworkConnections et NLM_CONNECTIVITY, et
//! netlistmgr.idl / ocidl.idl de MinGW-w64.

use super::comobj::{guid, log_no_interface, read_guid, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const CONNECT_E_NOCONNECTION: i32 = 0x8004_0200u32 as i32;
const CONNECT_E_CANNOTCONNECT: i32 = 0x8004_0202u32 as i32;

const fn nlm(d1: u32) -> Guid {
    guid(
        d1,
        0x570F,
        0x4A9B,
        [0x8D, 0x69, 0x19, 0x9F, 0xDB, 0xA5, 0x72, 0x3B],
    )
}

pub const CLSID_NETWORK_LIST_MANAGER: Guid = nlm(0xDCB0_0C01);
pub(crate) const IID_INETWORK_LIST_MANAGER: Guid = nlm(0xDCB0_0000);
const IID_INETWORK_LIST_MANAGER_EVENTS: Guid = nlm(0xDCB0_0001);
pub(crate) const IID_IENUM_NETWORKS: Guid = nlm(0xDCB0_0003);
pub(crate) const IID_IENUM_NETWORK_CONNECTIONS: Guid = nlm(0xDCB0_0006);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = guid(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub(crate) const IID_ICONNECTION_POINT_CONTAINER: Guid = guid(
    0xB196_B284,
    0xBAB4,
    0x101A,
    [0xB6, 0x9C, 0x00, 0xAA, 0x00, 0x34, 0x1D, 0x07],
);
pub(crate) const IID_ICONNECTION_POINT: Guid = guid(
    0xB196_B286,
    0xBAB4,
    0x101A,
    [0xB6, 0x9C, 0x00, 0xAA, 0x00, 0x34, 0x1D, 0x07],
);

const NLM_CONNECTIVITY_DISCONNECTED: u32 = 0;
const NLM_CONNECTIVITY_IPV4_LOCALNETWORK: u32 = 0x20;
const NLM_CONNECTIVITY_IPV4_INTERNET: u32 = 0x40;
const NLM_CONNECTIVITY_IPV6_INTERNET: u32 = 0x400;
pub(crate) const NLM_INTERNET: u32 =
    NLM_CONNECTIVITY_IPV4_INTERNET | NLM_CONNECTIVITY_IPV6_INTERNET;

/// Connectivité d'après /proc/net/route et /proc/net/ipv6_route.
pub(crate) fn connectivity_from(route: &str, ipv6_route: &str) -> u32 {
    let mut c = NLM_CONNECTIVITY_DISCONNECTED;
    for l in route.lines().skip(1) {
        let f: Vec<&str> = l.split_whitespace().collect();
        let up = f
            .get(3)
            .and_then(|x| u32::from_str_radix(x, 16).ok())
            .is_some_and(|flags| flags & 1 != 0);
        if f.len() < 4 || f[0] == "lo" || !up {
            continue;
        }
        c |= if f[1] == "00000000" {
            NLM_CONNECTIVITY_IPV4_INTERNET
        } else {
            NLM_CONNECTIVITY_IPV4_LOCALNETWORK
        };
    }
    for l in ipv6_route.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        // dest, longueur, source, longueur, saut, métrique, réf., usage,
        // drapeaux, interface. RTF_UP = 1, RTF_REJECT = 0x200.
        if f.len() < 10 || f[9] == "lo" {
            continue;
        }
        let flags = u32::from_str_radix(f[8], 16).unwrap_or(0);
        if f[0].bytes().all(|b| b == b'0') && f[1] == "00" && flags & 1 != 0 && flags & 0x200 == 0 {
            c |= NLM_CONNECTIVITY_IPV6_INTERNET;
        }
    }
    if c & NLM_CONNECTIVITY_IPV4_INTERNET != 0 {
        c &= !NLM_CONNECTIVITY_IPV4_LOCALNETWORK;
    }
    c
}

fn connectivity() -> u32 {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    connectivity_from(&read("/proc/net/route"), &read("/proc/net/ipv6_route"))
}

// ---------------------------------------------------------------------
// Objet principal : trois faces (gestionnaire, conteneur, point de
// connexion), un compteur et les abonnés.
// ---------------------------------------------------------------------

const PTR: usize = std::mem::size_of::<usize>();

#[repr(C)]
struct Manager {
    vt_manager: *const (),
    vt_container: *const (),
    vt_point: *const (),
    refs: AtomicU32,
    sinks: Mutex<Vec<(u32, usize)>>,
    next_cookie: AtomicU32,
}

unsafe fn addref_unknown(p: usize) {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(1));
    f(p);
}

unsafe fn release_unknown(p: usize) {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vt.add(2));
    f(p);
}

fn new_manager() -> usize {
    Box::into_raw(Box::new(Manager {
        vt_manager: MANAGER_VTBL.0.as_ptr() as *const (),
        vt_container: CONTAINER_VTBL.0.as_ptr() as *const (),
        vt_point: POINT_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        sinks: Mutex::new(Vec::new()),
        next_cookie: AtomicU32::new(1),
    })) as usize
}

unsafe fn mgr<'a>(obj: usize) -> &'a Manager {
    &*(obj as *const Manager)
}

unsafe fn qi(obj: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let face = match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IDISPATCH || g == IID_INETWORK_LIST_MANAGER => obj,
        Some(g) if g == IID_ICONNECTION_POINT_CONTAINER => obj + PTR,
        _ => {
            log_no_interface("NetworkListManager", riid);
            return E_NOINTERFACE;
        }
    };
    mgr(obj).refs.fetch_add(1, Ordering::Relaxed);
    *out = face as *mut c_void;
    S_OK
}

unsafe fn addref(obj: usize) -> u32 {
    mgr(obj).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe fn release(obj: usize) -> u32 {
    let n = mgr(obj).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let m = Box::from_raw(obj as *mut Manager);
        let sinks = std::mem::take(&mut *m.sinks.lock().unwrap_or_else(|e| e.into_inner()));
        drop(m);
        for (_, sink) in sinks {
            release_unknown(sink);
        }
    }
    n
}

// --- INetworkListManager (IDispatch) ---

unsafe extern "win64" fn m_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    qi(this, riid, out)
}
unsafe extern "win64" fn m_addref(this: usize) -> u32 {
    addref(this)
}
unsafe extern "win64" fn m_release(this: usize) -> u32 {
    release(this)
}
/// IDispatch::GetTypeInfoCount : aucune information de type.
unsafe extern "win64" fn get_type_info_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    *count = 0;
    S_OK
}
unsafe extern "win64" fn not_impl_3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl_5(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> i32 {
    E_NOTIMPL
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn not_impl_8(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _h: usize,
) -> i32 {
    E_NOTIMPL
}
/// GetNetworks(Flags, ppEnum) : liste vide.
unsafe extern "win64" fn get_networks(_this: usize, _flags: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_enum();
    S_OK
}
/// GetNetwork / GetNetworkConnection(GUID, pp) : aucun réseau connu.
unsafe extern "win64" fn get_by_id(_this: usize, _id: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_INVALIDARG
}
/// GetNetworkConnections(ppEnum) : liste vide.
unsafe extern "win64" fn get_connections(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_enum();
    S_OK
}
unsafe extern "win64" fn is_connected_to_internet(_this: usize, out: *mut i16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if connectivity() & NLM_INTERNET != 0 {
        -1
    } else {
        0
    };
    S_OK
}
unsafe extern "win64" fn is_connected(_this: usize, out: *mut i16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if connectivity() != 0 { -1 } else { 0 };
    S_OK
}
unsafe extern "win64" fn get_connectivity(_this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = connectivity();
    S_OK
}
/// SetSimulatedProfileInfo : sans objet.
unsafe extern "win64" fn not_impl_1(_this: usize, _a: usize) -> i32 {
    E_NOTIMPL
}
/// ClearSimulatedProfileInfo.
unsafe extern "win64" fn clear_simulated(_this: usize) -> i32 {
    S_OK
}

static MANAGER_VTBL: VTable<16> = VTable([
    m_qi as *const (),
    m_addref as *const (),
    m_release as *const (),
    get_type_info_count as *const (),
    not_impl_3 as *const (), // GetTypeInfo
    not_impl_5 as *const (), // GetIDsOfNames
    not_impl_8 as *const (), // Invoke
    get_networks as *const (),
    get_by_id as *const (), // GetNetwork
    get_connections as *const (),
    get_by_id as *const (), // GetNetworkConnection
    is_connected_to_internet as *const (),
    is_connected as *const (),
    get_connectivity as *const (),
    not_impl_1 as *const (), // SetSimulatedProfileInfo
    clear_simulated as *const (),
]);

// --- IConnectionPointContainer ---

unsafe extern "win64" fn c_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    qi(this - PTR, riid, out)
}
unsafe extern "win64" fn c_addref(this: usize) -> u32 {
    addref(this - PTR)
}
unsafe extern "win64" fn c_release(this: usize) -> u32 {
    release(this - PTR)
}
unsafe extern "win64" fn enum_connection_points(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}
/// FindConnectionPoint(riid, ppCP) : seul INetworkListManagerEvents.
unsafe extern "win64" fn find_connection_point(
    this: usize,
    riid: *const u8,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if read_guid(riid) != Some(IID_INETWORK_LIST_MANAGER_EVENTS) {
        // CONNECT_E_NOCONNECTION : interface d'événements non proposée.
        return CONNECT_E_NOCONNECTION;
    }
    let obj = this - PTR;
    addref(obj);
    *out = obj + 2 * PTR;
    S_OK
}

static CONTAINER_VTBL: VTable<5> = VTable([
    c_qi as *const (),
    c_addref as *const (),
    c_release as *const (),
    enum_connection_points as *const (),
    find_connection_point as *const (),
]);

// --- IConnectionPoint ---

unsafe extern "win64" fn p_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_ICONNECTION_POINT => {
            addref(this - 2 * PTR);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            log_no_interface("NetworkListManager (point de connexion)", riid);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn p_addref(this: usize) -> u32 {
    addref(this - 2 * PTR)
}
unsafe extern "win64" fn p_release(this: usize) -> u32 {
    release(this - 2 * PTR)
}
unsafe extern "win64" fn get_connection_interface(_this: usize, iid: *mut u8) -> i32 {
    if iid.is_null() {
        return E_POINTER;
    }
    std::ptr::copy_nonoverlapping(IID_INETWORK_LIST_MANAGER_EVENTS.as_ptr(), iid, 16);
    S_OK
}
unsafe extern "win64" fn get_connection_point_container(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let obj = this - 2 * PTR;
    addref(obj);
    *out = obj + PTR;
    S_OK
}
/// Advise(pUnkSink, pdwCookie) : l'abonné est gardé jusqu'à Unadvise.
unsafe extern "win64" fn advise(this: usize, sink: usize, cookie: *mut u32) -> i32 {
    if cookie.is_null() {
        return E_POINTER;
    }
    *cookie = 0;
    if sink == 0 {
        return CONNECT_E_CANNOTCONNECT;
    }
    let m = mgr(this - 2 * PTR);
    addref_unknown(sink);
    let c = m.next_cookie.fetch_add(1, Ordering::Relaxed);
    m.sinks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((c, sink));
    *cookie = c;
    S_OK
}
unsafe extern "win64" fn unadvise(this: usize, cookie: u32) -> i32 {
    let m = mgr(this - 2 * PTR);
    let sink = {
        let mut sinks = m.sinks.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pos) = sinks.iter().position(|(c, _)| *c == cookie) else {
            return CONNECT_E_NOCONNECTION;
        };
        sinks.remove(pos).1
    };
    release_unknown(sink);
    S_OK
}
unsafe extern "win64" fn enum_connections(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}

static POINT_VTBL: VTable<8> = VTable([
    p_qi as *const (),
    p_addref as *const (),
    p_release as *const (),
    get_connection_interface as *const (),
    get_connection_point_container as *const (),
    advise as *const (),
    unadvise as *const (),
    enum_connections as *const (),
]);

// ---------------------------------------------------------------------
// Énumérateur vide (IEnumNetworks et IEnumNetworkConnections ont la même
// forme : IDispatch, get__NewEnum, Next, Skip, Reset, Clone).
// ---------------------------------------------------------------------

#[repr(C)]
struct Enum {
    vt: *const (),
    refs: AtomicU32,
}

fn new_enum() -> usize {
    Box::into_raw(Box::new(Enum {
        vt: ENUM_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
    })) as usize
}

unsafe extern "win64" fn e_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match read_guid(riid) {
        Some(g)
            if g == IID_IUNKNOWN
                || g == IID_IDISPATCH
                || g == IID_IENUM_NETWORKS
                || g == IID_IENUM_NETWORK_CONNECTIONS =>
        {
            e_addref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            log_no_interface("NetworkListManager (énumération)", riid);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn e_addref(this: usize) -> u32 {
    (*(this as *const Enum))
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}
unsafe extern "win64" fn e_release(this: usize) -> u32 {
    let n = (*(this as *const Enum)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Enum));
    }
    n
}
unsafe extern "win64" fn e_new_enum(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}
/// Next(celt, rgelt, pceltFetched) : rien à rendre.
unsafe extern "win64" fn e_next(_this: usize, count: u32, _items: usize, fetched: *mut u32) -> i32 {
    if !fetched.is_null() {
        *fetched = 0;
    }
    if count == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn e_skip(_this: usize, count: u32) -> i32 {
    if count == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn e_reset(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn e_clone(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_enum();
    S_OK
}

static ENUM_VTBL: VTable<12> = VTable([
    e_qi as *const (),
    e_addref as *const (),
    e_release as *const (),
    get_type_info_count as *const (),
    not_impl_3 as *const (),
    not_impl_5 as *const (),
    not_impl_8 as *const (),
    e_new_enum as *const (),
    e_next as *const (),
    e_skip as *const (),
    e_reset as *const (),
    e_clone as *const (),
]);

/// Classe servie par CoCreateInstance.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    if *clsid != CLSID_NETWORK_LIST_MANAGER {
        return None;
    }
    if out.is_null() {
        return Some(E_POINTER);
    }
    let obj = new_manager();
    let hr = qi(obj, iid as *const _ as *const u8, out);
    release(obj);
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<F: Copy>(obj: usize, index: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(index))
    }

    #[test]
    fn connectivite_d_apres_les_routes() {
        let entete = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n";
        let defaut = format!("{entete}eth0\t00000000\t0102A8C0\t0003\t0\t0\t100\t00000000\n");
        let local = format!("{entete}eth0\t0002A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\n");
        let boucle = format!("{entete}lo\t00000000\t00000000\t0001\t0\t0\t0\t00000000\n");
        assert_eq!(
            connectivity_from(&defaut, ""),
            NLM_CONNECTIVITY_IPV4_INTERNET
        );
        assert_eq!(
            connectivity_from(&local, ""),
            NLM_CONNECTIVITY_IPV4_LOCALNETWORK
        );
        assert_eq!(
            connectivity_from(&boucle, ""),
            NLM_CONNECTIVITY_DISCONNECTED
        );
        let zero = "00000000000000000000000000000000";
        let v6 = format!("{zero} 00 {zero} 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 eth0\n");
        let rejet = format!("{zero} 00 {zero} 00 {zero} ffffffff 00000001 00000000 00200200 lo\n");
        assert_eq!(connectivity_from("", &v6), NLM_CONNECTIVITY_IPV6_INTERNET);
        assert_eq!(connectivity_from("", &rejet), NLM_CONNECTIVITY_DISCONNECTED);
    }

    #[test]
    fn gestionnaire_points_de_connexion_et_abonnes() {
        unsafe {
            let clsid =
                &*(&CLSID_NETWORK_LIST_MANAGER as *const Guid as *const super::super::ole32::Guid);
            let iid =
                &*(&IID_INETWORK_LIST_MANAGER as *const Guid as *const super::super::ole32::Guid);
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(create_builtin(clsid, iid, &mut p), Some(S_OK));
            let p = p as usize;
            // GetConnectivity (n° 13) : même réponse que la lecture directe.
            let mut c = u32::MAX;
            let f: unsafe extern "win64" fn(usize, *mut u32) -> i32 = call(p, 13);
            assert_eq!(f(p, &mut c), S_OK);
            assert_eq!(c, connectivity());
            // GetNetworkConnections (n° 9) : énumération vide.
            let mut e = 0usize;
            let g: unsafe extern "win64" fn(usize, *mut usize) -> i32 = call(p, 9);
            assert_eq!(g(p, &mut e), S_OK);
            let next: unsafe extern "win64" fn(usize, u32, usize, *mut u32) -> i32 = call(e, 8);
            let mut got = 7u32;
            let mut item = 0usize;
            assert_eq!(
                next(e, 1, &mut item as *mut usize as usize, &mut got),
                S_FALSE
            );
            assert_eq!(got, 0);
            assert_eq!(e_release(e), 0);
            // Conteneur → point de connexion d'INetworkListManagerEvents.
            let mut cpc: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                qi(p, IID_ICONNECTION_POINT_CONTAINER.as_ptr(), &mut cpc),
                S_OK
            );
            let cpc = cpc as usize;
            let find: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = call(cpc, 4);
            let mut cp = 0usize;
            assert_eq!(
                find(cpc, IID_IDISPATCH.as_ptr(), &mut cp),
                CONNECT_E_NOCONNECTION
            );
            assert_eq!(
                find(cpc, IID_INETWORK_LIST_MANAGER_EVENTS.as_ptr(), &mut cp),
                S_OK
            );
            // Advise garde l'abonné (une référence de plus), Unadvise la rend.
            let sink = new_enum();
            let adv: unsafe extern "win64" fn(usize, usize, *mut u32) -> i32 = call(cp, 5);
            let mut cookie = 0u32;
            assert_eq!(adv(cp, sink, &mut cookie), S_OK);
            assert_ne!(cookie, 0);
            assert_eq!((*(sink as *const Enum)).refs.load(Ordering::Relaxed), 2);
            let unadv: unsafe extern "win64" fn(usize, u32) -> i32 = call(cp, 6);
            assert_eq!(unadv(cp, cookie), S_OK);
            assert_eq!(unadv(cp, cookie), CONNECT_E_NOCONNECTION);
            assert_eq!((*(sink as *const Enum)).refs.load(Ordering::Relaxed), 1);
            // Un abonné resté inscrit est libéré avec l'objet.
            assert_eq!(adv(cp, sink, &mut cookie), S_OK);
            assert_eq!(p_release(cp), 2);
            assert_eq!(c_release(cpc), 1);
            assert_eq!(m_release(p), 0);
            assert_eq!(e_release(sink), 0);
        }
    }
}
