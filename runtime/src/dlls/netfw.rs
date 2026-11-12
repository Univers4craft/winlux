//! Pare-feu Windows, ancienne interface : CLSID_NetFwMgr (INetFwMgr,
//! INetFwPolicy, INetFwProfile, INetFwAuthorizedApplications) et
//! CLSID_NetFwAuthorizedApplication (INetFwAuthorizedApplication), #1919.
//!
//! SteamService.exe /install crée HNetCfg.FwMgr pour autoriser steam.exe.
//! Sans la classe, CoCreateInstance rendait REGDB_E_CLASSNOTREG, Steam
//! notait « Failed to add firewall exception » et `/install` finissait en
//! erreur.
//!
//! Le pare-feu de Winlux filtre par profil réseau (public ou privé), pas par
//! programme : une autorisation demandée est acceptée et retenue pour la
//! durée du processus (Item, get_Count la retrouvent), sans autre effet.
//! Le profil est déclaré actif et les exceptions permises, ce qui est vrai
//! du filtrage de Winlux. Aucune question n'est posée (#1733).
//!
//! Méthodes, ordre et constantes : netfw.h de MinGW-w64 (domaine public) et
//! documentation Microsoft Learn d'INetFwMgr et INetFwAuthorizedApplication.

use super::comobj::{guid, log_no_interface, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND) : programme non autorisé.
const HR_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
const VARIANT_TRUE: i16 = -1;
const VT_BOOL: u16 = 11;

pub const CLSID_NET_FW_MGR: Guid = guid(
    0x304C_E942,
    0x6E39,
    0x40D8,
    [0x94, 0x3A, 0xB9, 0x13, 0xC4, 0x0C, 0x9C, 0xD4],
);
pub const CLSID_NET_FW_AUTHORIZED_APPLICATION: Guid = guid(
    0xEC98_46B3,
    0x2762,
    0x4A6B,
    [0xA2, 0x14, 0x6A, 0xCB, 0x60, 0x34, 0x62, 0xD2],
);
pub const IID_INET_FW_MGR: Guid = guid(
    0xF789_8AF5,
    0xCAC4,
    0x4632,
    [0xA2, 0xEC, 0xDA, 0x06, 0xE5, 0x11, 0x1A, 0xF2],
);
pub const IID_INET_FW_POLICY: Guid = guid(
    0xD46D_2478,
    0x9AC9,
    0x4008,
    [0x9D, 0xC7, 0x55, 0x63, 0xCE, 0x55, 0x36, 0xCC],
);
pub const IID_INET_FW_PROFILE: Guid = guid(
    0x174A_0DDA,
    0xE9F9,
    0x449D,
    [0x99, 0x3B, 0x21, 0xAB, 0x66, 0x7C, 0xA4, 0x56],
);
pub const IID_INET_FW_AUTHORIZED_APPLICATIONS: Guid = guid(
    0x644E_FD52,
    0xCCF9,
    0x486C,
    [0x97, 0xA2, 0x39, 0xF3, 0x52, 0x57, 0x0B, 0x30],
);
pub const IID_INET_FW_AUTHORIZED_APPLICATION: Guid = guid(
    0xB5E6_4FFA,
    0xC2C5,
    0x444E,
    [0xA3, 0x01, 0xFB, 0x5E, 0x00, 0x01, 0x80, 0x50],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = guid(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// NET_FW_PROFILE_STANDARD, NET_FW_IP_VERSION_ANY, NET_FW_SCOPE_ALL.
const PROFILE_STANDARD: u32 = 1;
const IP_VERSION_ANY: u32 = 2;
const SCOPE_ALL: u32 = 0;

/// Propriétés d'une autorisation de programme.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AppData {
    name: String,
    image: String,
    ip_version: u32,
    scope: u32,
    remote: String,
    enabled: bool,
}

impl Default for AppData {
    fn default() -> Self {
        AppData {
            name: String::new(),
            image: String::new(),
            ip_version: IP_VERSION_ANY,
            scope: SCOPE_ALL,
            remote: "*".into(),
            enabled: true,
        }
    }
}

/// Autorisations accordées pendant la vie du processus.
static AUTHORIZED: Mutex<Vec<AppData>> = Mutex::new(Vec::new());

fn authorized() -> std::sync::MutexGuard<'static, Vec<AppData>> {
    AUTHORIZED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Les chemins Windows ne distinguent pas la casse.
fn same_image(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

// ----- Conversions -----

pub(super) unsafe fn bstr_in(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while n < 32_768 && *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

pub(super) unsafe fn bstr_out(out: *mut *mut u16, s: &str) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    *out = super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32);
    S_OK
}

pub(super) unsafe fn bool_out(out: *mut i16, v: bool) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if v { VARIANT_TRUE } else { 0 };
    S_OK
}

pub(super) unsafe fn u32_out(out: *mut u32, v: u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = v;
    S_OK
}

/// VARIANT de type VT_BOOL (disposition x64 : vt à 0, valeur à 8).
pub(super) unsafe fn variant_bool(out: *mut u8, v: bool) {
    if out.is_null() {
        return;
    }
    std::ptr::write_bytes(out, 0, 24);
    *(out as *mut u16) = VT_BOOL;
    *(out.add(8) as *mut i16) = if v { VARIANT_TRUE } else { 0 };
}

// ----- Objets sans état : gestionnaire, stratégie, profil, liste -----

#[repr(C)]
struct Static {
    vtbl: *const (),
}
unsafe impl Sync for Static {}

static MGR: Static = Static {
    vtbl: &MGR_VT as *const _ as *const (),
};
static POLICY: Static = Static {
    vtbl: &POLICY_VT as *const _ as *const (),
};
static PROFILE: Static = Static {
    vtbl: &PROFILE_VT as *const _ as *const (),
};
static APPS: Static = Static {
    vtbl: &APPS_VT as *const _ as *const (),
};

fn as_ptr(s: &'static Static) -> *mut c_void {
    s as *const Static as *mut c_void
}

pub(super) extern "win64" fn static_add_ref(_: usize) -> u32 {
    2
}
pub(super) extern "win64" fn static_release(_: usize) -> u32 {
    1
}
pub(super) unsafe extern "win64" fn type_info_count(_: usize, out: *mut u32) -> i32 {
    u32_out(out, 0)
}
/// Méthodes non prises en charge : chacune rend E_NOTIMPL et le dit dans
/// la trace, pour savoir laquelle un programme attendait (#1919).
macro_rules! unsupported {
    ($($f:ident => $label:literal;)*) => {
        $(
            pub(super) extern "win64" fn $f(_: usize) -> i32 {
                crate::trace!("{} : non pris en charge (E_NOTIMPL)", $label);
                E_NOTIMPL
            }
        )*
    };
}
unsupported! {
    get_type_info => "IDispatch::GetTypeInfo (pare-feu)";
    get_ids_of_names => "IDispatch::GetIDsOfNames (pare-feu)";
    invoke => "IDispatch::Invoke (pare-feu)";
    profile_remote_admin => "INetFwProfile::get_RemoteAdminSettings";
    profile_icmp => "INetFwProfile::get_IcmpSettings";
    profile_open_ports => "INetFwProfile::get_GloballyOpenPorts";
    profile_services => "INetFwProfile::get_Services";
    policy2_service_restriction => "INetFwPolicy2::get_ServiceRestriction";
}
/// Méthode acceptée sans effet. En x64, l'appelant nettoie la pile : une
/// seule fonction convient à toutes les signatures.
pub(super) extern "win64" fn ok(_: usize) -> i32 {
    S_OK
}

/// QueryInterface commun : `own` est l'IID propre de l'objet.
unsafe fn static_qi(
    obj: &'static Static,
    own: Guid,
    name: &str,
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
    let want = *iid;
    if want == IID_IUNKNOWN || want == IID_IDISPATCH || want == own {
        *out = as_ptr(obj);
        return S_OK;
    }
    log_no_interface(name, iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn mgr_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(&MGR, IID_INET_FW_MGR, "INetFwMgr", iid, out)
}
unsafe extern "win64" fn policy_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(&POLICY, IID_INET_FW_POLICY, "INetFwPolicy", iid, out)
}
unsafe extern "win64" fn profile_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(&PROFILE, IID_INET_FW_PROFILE, "INetFwProfile", iid, out)
}
unsafe extern "win64" fn apps_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(
        &APPS,
        IID_INET_FW_AUTHORIZED_APPLICATIONS,
        "INetFwAuthorizedApplications",
        iid,
        out,
    )
}

unsafe fn give(out: *mut *mut c_void, obj: &'static Static) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = as_ptr(obj);
    S_OK
}

static MGR_VT: VTable<12> = VTable([
    mgr_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    mgr_local_policy as *const (),
    mgr_profile_type as *const (),
    ok as *const (), // RestoreDefaults
    mgr_is_port_allowed as *const (),
    mgr_is_icmp_allowed as *const (),
]);

unsafe extern "win64" fn mgr_local_policy(_: usize, out: *mut *mut c_void) -> i32 {
    give(out, &POLICY)
}
unsafe extern "win64" fn mgr_profile_type(_: usize, out: *mut u32) -> i32 {
    u32_out(out, PROFILE_STANDARD)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn mgr_is_port_allowed(
    _: usize,
    _image: *const u16,
    _ip_version: u32,
    _port: i32,
    _local: *const u16,
    _protocol: u32,
    allowed: *mut u8,
    restricted: *mut u8,
) -> i32 {
    variant_bool(allowed, true);
    variant_bool(restricted, false);
    S_OK
}
unsafe extern "win64" fn mgr_is_icmp_allowed(
    _: usize,
    _ip_version: u32,
    _local: *const u16,
    _kind: u8,
    allowed: *mut u8,
    restricted: *mut u8,
) -> i32 {
    variant_bool(allowed, true);
    variant_bool(restricted, false);
    S_OK
}

static POLICY_VT: VTable<9> = VTable([
    policy_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    policy_current_profile as *const (),
    policy_profile_by_type as *const (),
]);

unsafe extern "win64" fn policy_current_profile(_: usize, out: *mut *mut c_void) -> i32 {
    give(out, &PROFILE)
}
unsafe extern "win64" fn policy_profile_by_type(
    _: usize,
    _kind: u32,
    out: *mut *mut c_void,
) -> i32 {
    give(out, &PROFILE)
}

static PROFILE_VT: VTable<21> = VTable([
    profile_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    mgr_profile_type as *const (),     // get_Type
    bool_true as *const (),            // get_FirewallEnabled
    ok as *const (),                   // put_FirewallEnabled
    bool_false as *const (),           // get_ExceptionsNotAllowed
    ok as *const (),                   // put_ExceptionsNotAllowed
    bool_false as *const (),           // get_NotificationsDisabled
    ok as *const (),                   // put_NotificationsDisabled
    bool_false as *const (),           // get_UnicastResponsesToMulticastBroadcastDisabled
    ok as *const (),                   // put_UnicastResponsesToMulticastBroadcastDisabled
    profile_remote_admin as *const (), // get_RemoteAdminSettings
    profile_icmp as *const (),         // get_IcmpSettings
    profile_open_ports as *const (),   // get_GloballyOpenPorts
    profile_services as *const (),     // get_Services
    profile_apps as *const (),         // get_AuthorizedApplications
]);

unsafe extern "win64" fn bool_true(_: usize, out: *mut i16) -> i32 {
    bool_out(out, true)
}
unsafe extern "win64" fn bool_false(_: usize, out: *mut i16) -> i32 {
    bool_out(out, false)
}
unsafe extern "win64" fn profile_apps(_: usize, out: *mut *mut c_void) -> i32 {
    give(out, &APPS)
}

static APPS_VT: VTable<12> = VTable([
    apps_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    apps_count as *const (),
    apps_add as *const (),
    apps_remove as *const (),
    apps_item as *const (),
    apps_new_enum as *const (), // get__NewEnum
]);

unsafe extern "win64" fn apps_count(_: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = authorized().len() as i32;
    S_OK
}

/// Lit une propriété BSTR d'un INetFwAuthorizedApplication quelconque
/// (le nôtre, ou celui d'un programme x86 derrière son mandataire).
pub(super) unsafe fn read_bstr_prop(app: usize, index: usize) -> Option<String> {
    let vt = *(app as *const *const usize);
    let get: unsafe extern "win64" fn(usize, *mut *mut u16) -> i32 =
        std::mem::transmute(*vt.add(index));
    let mut b: *mut u16 = std::ptr::null_mut();
    if get(app, &mut b) < 0 {
        return None;
    }
    let s = bstr_in(b);
    super::oleaut32::sys_free_string(b);
    Some(s)
}

pub(super) unsafe fn read_u32_prop(app: usize, index: usize) -> Option<u32> {
    let vt = *(app as *const *const usize);
    let get: unsafe extern "win64" fn(usize, *mut u32) -> i32 = std::mem::transmute(*vt.add(index));
    let mut v = 0u32;
    (get(app, &mut v) >= 0).then_some(v)
}

unsafe extern "win64" fn apps_add(_: usize, app: usize) -> i32 {
    if app == 0 {
        return E_POINTER;
    }
    let mut data = AppData::default();
    // Indices d'INetFwAuthorizedApplication : get_Name 7,
    // get_ProcessImageFileName 9, get_IpVersion 11, get_Scope 13,
    // get_RemoteAddresses 15, get_Enabled 17.
    if let Some(s) = read_bstr_prop(app, 9) {
        data.image = s;
    }
    if data.image.is_empty() {
        return E_INVALIDARG;
    }
    if let Some(s) = read_bstr_prop(app, 7) {
        data.name = s;
    }
    if let Some(v) = read_u32_prop(app, 11) {
        data.ip_version = v;
    }
    if let Some(v) = read_u32_prop(app, 13) {
        data.scope = v;
    }
    if let Some(s) = read_bstr_prop(app, 15) {
        data.remote = s;
    }
    if let Some(v) = read_u32_prop(app, 17) {
        data.enabled = v as u16 != 0;
    }
    crate::trace!(
        "INetFwAuthorizedApplications::Add({}, « {} ») : accepté (pare-feu par profil, pas par programme)",
        data.image,
        data.name
    );
    let mut list = authorized();
    list.retain(|a| !same_image(&a.image, &data.image));
    list.push(data);
    S_OK
}

unsafe extern "win64" fn apps_remove(_: usize, image: *const u16) -> i32 {
    let image = bstr_in(image);
    authorized().retain(|a| !same_image(&a.image, &image));
    S_OK
}

unsafe extern "win64" fn apps_item(_: usize, image: *const u16, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let image = bstr_in(image);
    let found = authorized()
        .iter()
        .find(|a| same_image(&a.image, &image))
        .cloned();
    match found {
        Some(data) => {
            *out = new_app(data) as *mut c_void;
            S_OK
        }
        None => HR_NOT_FOUND,
    }
}

/// get__NewEnum : IEnumVARIANT sur une copie des autorisations retenues.
/// SteamService parcourt la liste et notait « Failed to add firewall
/// exception » tant qu'elle rendait E_NOTIMPL (#2036).
unsafe extern "win64" fn apps_new_enum(_: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let items: Vec<usize> = authorized()
        .iter()
        .map(|a| new_app(a.clone()) as usize)
        .collect();
    crate::trace!(
        "INetFwAuthorizedApplications::get__NewEnum : {} autorisation(s)",
        items.len()
    );
    *out = super::enum_variant::new_enum(items);
    S_OK
}

// ----- INetFwAuthorizedApplication : objet compté, propriétés modifiables -----

#[repr(C)]
struct App {
    vtbl: *const (),
    refs: AtomicU32,
    data: Mutex<AppData>,
}

fn new_app(data: AppData) -> *mut App {
    Box::into_raw(Box::new(App {
        vtbl: APP_VT.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        data: Mutex::new(data),
    }))
}

unsafe fn app_data(this: *mut App) -> std::sync::MutexGuard<'static, AppData> {
    (*this).data.lock().unwrap_or_else(|e| e.into_inner())
}

static APP_VT: VTable<19> = VTable([
    app_qi as *const (),
    app_add_ref as *const (),
    app_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    app_get_name as *const (),
    app_put_name as *const (),
    app_get_image as *const (),
    app_put_image as *const (),
    app_get_ip_version as *const (),
    app_put_ip_version as *const (),
    app_get_scope as *const (),
    app_put_scope as *const (),
    app_get_remote as *const (),
    app_put_remote as *const (),
    app_get_enabled as *const (),
    app_put_enabled as *const (),
]);

unsafe extern "win64" fn app_qi(this: *mut App, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let want = *iid;
    if want == IID_IUNKNOWN || want == IID_IDISPATCH || want == IID_INET_FW_AUTHORIZED_APPLICATION {
        app_add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    log_no_interface("INetFwAuthorizedApplication", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn app_add_ref(this: *mut App) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn app_release(this: *mut App) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

unsafe extern "win64" fn app_get_name(this: *mut App, out: *mut *mut u16) -> i32 {
    let s = app_data(this).name.clone();
    bstr_out(out, &s)
}
unsafe extern "win64" fn app_put_name(this: *mut App, v: *const u16) -> i32 {
    app_data(this).name = bstr_in(v);
    S_OK
}
unsafe extern "win64" fn app_get_image(this: *mut App, out: *mut *mut u16) -> i32 {
    let s = app_data(this).image.clone();
    bstr_out(out, &s)
}
unsafe extern "win64" fn app_put_image(this: *mut App, v: *const u16) -> i32 {
    let s = bstr_in(v);
    if s.is_empty() {
        return E_INVALIDARG;
    }
    app_data(this).image = s;
    S_OK
}
unsafe extern "win64" fn app_get_ip_version(this: *mut App, out: *mut u32) -> i32 {
    let v = app_data(this).ip_version;
    u32_out(out, v)
}
unsafe extern "win64" fn app_put_ip_version(this: *mut App, v: u32) -> i32 {
    app_data(this).ip_version = v;
    S_OK
}
unsafe extern "win64" fn app_get_scope(this: *mut App, out: *mut u32) -> i32 {
    let v = app_data(this).scope;
    u32_out(out, v)
}
unsafe extern "win64" fn app_put_scope(this: *mut App, v: u32) -> i32 {
    app_data(this).scope = v;
    S_OK
}
unsafe extern "win64" fn app_get_remote(this: *mut App, out: *mut *mut u16) -> i32 {
    let s = app_data(this).remote.clone();
    bstr_out(out, &s)
}
unsafe extern "win64" fn app_put_remote(this: *mut App, v: *const u16) -> i32 {
    app_data(this).remote = bstr_in(v);
    S_OK
}
unsafe extern "win64" fn app_get_enabled(this: *mut App, out: *mut i16) -> i32 {
    let v = app_data(this).enabled;
    bool_out(out, v)
}
unsafe extern "win64" fn app_put_enabled(this: *mut App, v: i16) -> i32 {
    app_data(this).enabled = v != 0;
    S_OK
}

/// CoCreateInstance de CLSID_NetFwMgr et CLSID_NetFwAuthorizedApplication.
///
/// # Safety
/// `out` doit être valide en écriture.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    let iid = iid as *const _ as *const Guid;
    if *clsid == CLSID_NET_FW_MGR {
        return Some(mgr_qi(0, iid, out));
    }
    if *clsid == CLSID_NET_FW_AUTHORIZED_APPLICATION {
        let app = new_app(AppData::default());
        let hr = app_qi(app, iid, out);
        app_release(app);
        return Some(hr);
    }
    super::netfw2::create_builtin(clsid, iid, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(x: &Guid) -> &super::super::ole32::Guid {
        unsafe { &*(x as *const Guid as *const super::super::ole32::Guid) }
    }

    unsafe fn method<F>(obj: *mut c_void, index: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(index))
    }

    unsafe fn bstr(s: &str) -> *mut u16 {
        let w: Vec<u16> = s.encode_utf16().collect();
        super::super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32)
    }

    /// Le parcours de SteamService : FwMgr → LocalPolicy → CurrentProfile
    /// → AuthorizedApplications → Add, puis Item retrouve l'autorisation.
    #[test]
    fn autorisation_de_programme_acceptee_et_retrouvee() {
        const S_FALSE: i32 = 1;
        unsafe {
            let mut mgr: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(g(&CLSID_NET_FW_MGR), g(&IID_INET_FW_MGR), &mut mgr),
                Some(S_OK)
            );
            let get: extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 = method(mgr, 7);
            let mut policy = std::ptr::null_mut();
            assert_eq!(get(mgr, &mut policy), S_OK);
            let get: extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 = method(policy, 7);
            let mut profile = std::ptr::null_mut();
            assert_eq!(get(policy, &mut profile), S_OK);
            let enabled: extern "win64" fn(*mut c_void, *mut i16) -> i32 = method(profile, 8);
            let mut on = 0i16;
            assert_eq!(enabled(profile, &mut on), S_OK);
            assert_eq!(on, VARIANT_TRUE);
            let get: extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 = method(profile, 20);
            let mut apps = std::ptr::null_mut();
            assert_eq!(get(profile, &mut apps), S_OK);

            let mut app: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(
                    g(&CLSID_NET_FW_AUTHORIZED_APPLICATION),
                    g(&IID_INET_FW_AUTHORIZED_APPLICATION),
                    &mut app
                ),
                Some(S_OK)
            );
            let put: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(app, 10);
            let image = bstr("C:\\Program Files (x86)\\Steam\\steam.exe");
            assert_eq!(put(app, image), S_OK);
            let put: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(app, 8);
            assert_eq!(put(app, bstr("Steam")), S_OK);

            let add: extern "win64" fn(*mut c_void, *mut c_void) -> i32 = method(apps, 8);
            assert_eq!(add(apps, app), S_OK);
            let count: extern "win64" fn(*mut c_void, *mut i32) -> i32 = method(apps, 7);
            let mut n = 0;
            assert_eq!(count(apps, &mut n), S_OK);
            assert!(n >= 1);

            let item: extern "win64" fn(*mut c_void, *mut u16, *mut *mut c_void) -> i32 =
                method(apps, 10);
            let mut found = std::ptr::null_mut();
            let upper = bstr("C:\\PROGRAM FILES (X86)\\STEAM\\STEAM.EXE");
            assert_eq!(item(apps, upper, &mut found), S_OK);
            let name: extern "win64" fn(*mut c_void, *mut *mut u16) -> i32 = method(found, 7);
            let mut b = std::ptr::null_mut();
            assert_eq!(name(found, &mut b), S_OK);
            assert_eq!(bstr_in(b), "Steam");
            let mut missing = std::ptr::null_mut();
            assert_eq!(
                item(apps, bstr("C:\\autre.exe"), &mut missing),
                HR_NOT_FOUND
            );
            assert!(missing.is_null());

            // _NewEnum : IEnumVARIANT qui rend l'autorisation (#2036).
            let new_enum: extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 =
                method(apps, 11);
            let mut unk = std::ptr::null_mut();
            assert_eq!(new_enum(apps, &mut unk), S_OK);
            let qi: extern "win64" fn(
                *mut c_void,
                *const super::super::ole32::Guid,
                *mut *mut c_void,
            ) -> i32 = method(unk, 0);
            let mut en = std::ptr::null_mut();
            assert_eq!(
                qi(
                    unk,
                    g(&super::super::enum_variant::IID_IENUM_VARIANT),
                    &mut en
                ),
                S_OK
            );
            let next: extern "win64" fn(*mut c_void, u32, *mut u8, *mut u32) -> i32 = method(en, 3);
            let mut v = [0u8; 24];
            let mut got = 0u32;
            assert_eq!(next(en, 1, v.as_mut_ptr(), &mut got), S_OK);
            assert_eq!((got, *(v.as_ptr() as *const u16)), (1, 9));
            let listed = *(v.as_ptr().add(8) as *const *mut c_void);
            let mut b2 = std::ptr::null_mut();
            let image_of: extern "win64" fn(*mut c_void, *mut *mut u16) -> i32 = method(listed, 9);
            assert_eq!(image_of(listed, &mut b2), S_OK);
            assert!(bstr_in(b2).eq_ignore_ascii_case("C:\\Program Files (x86)\\Steam\\steam.exe"));
            assert_eq!(next(en, 1, v.as_mut_ptr(), &mut got), S_FALSE);
            let rel: extern "win64" fn(*mut c_void) -> u32 = method(listed, 2);
            rel(listed);
            let rel_en: extern "win64" fn(*mut c_void) -> u32 = method(en, 2);
            rel_en(en);
            rel_en(unk);

            let remove: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(apps, 9);
            assert_eq!(remove(apps, image), S_OK);
            assert_eq!(item(apps, image, &mut missing), HR_NOT_FOUND);

            let release: extern "win64" fn(*mut c_void) -> u32 = method(found, 2);
            release(found);
            release(app);
            // Une autre classe n'est pas prise.
            let mut other: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(g(&IID_INET_FW_MGR), g(&IID_IUNKNOWN), &mut other),
                None
            );
        }
    }
}
