//! Pare-feu Windows, interface de Vista et suivants : CLSID_NetFwPolicy2
//! (INetFwPolicy2, INetFwRules) et CLSID_NetFwRule (INetFwRule), #1919.
//!
//! Après INetFwAuthorizedApplications::Add (`netfw.rs`), SteamService.exe
//! /install crée NetFwPolicy2 pour poser ses règles ; sans la classe,
//! CoCreateInstance rendait REGDB_E_CLASSNOTREG.
//!
//! Comme pour l'ancienne interface, le pare-feu de Winlux filtre par profil
//! réseau, pas par règle de programme : une règle ajoutée est acceptée,
//! tracée et retenue pour la vie du processus (Item, get_Count, Remove la
//! retrouvent), sans autre effet et sans question (#1733).
//!
//! Méthodes, ordre et constantes : netfw.h de MinGW-w64 (domaine public) et
//! documentation Microsoft Learn d'INetFwPolicy2, INetFwRules, INetFwRule.

use super::comobj::{guid, log_no_interface, Guid, VTable};
use super::netfw::{
    bool_out, bstr_in, bstr_out, get_ids_of_names, get_type_info, invoke, ok,
    policy2_service_restriction, read_bstr_prop, read_u32_prop, rules_new_enum, static_add_ref,
    static_release, type_info_count, u32_out,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const HR_NOT_FOUND: i32 = 0x8007_0002u32 as i32;

pub const CLSID_NET_FW_POLICY2: Guid = guid(
    0xE2B3_C97F,
    0x6AE1,
    0x41AC,
    [0x81, 0x7A, 0xF6, 0xF9, 0x21, 0x66, 0xD7, 0xDD],
);
pub const CLSID_NET_FW_RULE: Guid = guid(
    0x2C5B_C43E,
    0x3369,
    0x4C33,
    [0xAB, 0x0C, 0xBE, 0x94, 0x69, 0x67, 0x7A, 0xF4],
);
pub const IID_INET_FW_POLICY2: Guid = guid(
    0x9832_5047,
    0xC671,
    0x4174,
    [0x8D, 0x81, 0xDE, 0xFC, 0xD3, 0xF0, 0x31, 0x86],
);
pub const IID_INET_FW_RULES: Guid = guid(
    0x9C4C_6277,
    0x5027,
    0x441E,
    [0xAF, 0xAE, 0xCA, 0x1F, 0x54, 0x2D, 0xA0, 0x09],
);
pub const IID_INET_FW_RULE: Guid = guid(
    0xAF23_0D27,
    0xBABA,
    0x4E42,
    [0xAC, 0xED, 0xF5, 0x24, 0xF2, 0x2C, 0xFC, 0xE2],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = guid(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// NET_FW_PROFILE2_PRIVATE, NET_FW_ACTION_BLOCK / ALLOW.
const PROFILE2_PRIVATE: u32 = 2;
const ACTION_BLOCK: u32 = 0;
const ACTION_ALLOW: u32 = 1;

// ----- Propriétés d'une règle -----

/// Nature de la propriété `k` d'INetFwRule (get_ en 7 + 2k, put_ en 8 + 2k).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Str,
    Num,
    Bool,
    Variant,
}

/// Name, Description, ApplicationName, ServiceName, Protocol, LocalPorts,
/// RemotePorts, LocalAddresses, RemoteAddresses, IcmpTypesAndCodes,
/// Direction, Interfaces, InterfaceTypes, Enabled, Grouping, Profiles,
/// EdgeTraversal, Action.
const KINDS: [Kind; 18] = [
    Kind::Str,
    Kind::Str,
    Kind::Str,
    Kind::Str,
    Kind::Num,
    Kind::Str,
    Kind::Str,
    Kind::Str,
    Kind::Str,
    Kind::Str,
    Kind::Num,
    Kind::Variant,
    Kind::Str,
    Kind::Bool,
    Kind::Str,
    Kind::Num,
    Kind::Bool,
    Kind::Num,
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Prop {
    S(String),
    N(u32),
}

type RuleData = Vec<Prop>;

/// Valeurs d'une règle neuve (documentation d'INetFwRule) : protocole
/// quelconque (256), entrante (1), désactivée, tous profils, autoriser.
fn default_rule() -> RuleData {
    KINDS
        .iter()
        .enumerate()
        .map(|(k, kind)| match (k, kind) {
            (4, _) => Prop::N(256),
            (10, _) => Prop::N(1),
            (15, _) => Prop::N(0x7FFF_FFFF),
            (17, _) => Prop::N(ACTION_ALLOW),
            (7 | 8, _) => Prop::S("*".into()),
            (12, _) => Prop::S("All".into()),
            (_, Kind::Str) => Prop::S(String::new()),
            _ => Prop::N(0),
        })
        .collect()
}

fn name_of(r: &RuleData) -> &str {
    match &r[0] {
        Prop::S(s) => s,
        Prop::N(_) => "",
    }
}

/// Les noms de règle se comparent sans la casse, accents compris.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Règles ajoutées pendant la vie du processus.
static RULES: Mutex<Vec<RuleData>> = Mutex::new(Vec::new());

fn rules() -> std::sync::MutexGuard<'static, Vec<RuleData>> {
    RULES.lock().unwrap_or_else(|e| e.into_inner())
}

// ----- Objets sans état : stratégie et liste de règles -----

#[repr(C)]
struct Static {
    vtbl: *const (),
}
unsafe impl Sync for Static {}

static POLICY2: Static = Static {
    vtbl: &POLICY2_VT as *const _ as *const (),
};
static RULES_OBJ: Static = Static {
    vtbl: &RULES_VT as *const _ as *const (),
};

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
        *out = obj as *const Static as *mut c_void;
        return S_OK;
    }
    log_no_interface(name, iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn policy2_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(&POLICY2, IID_INET_FW_POLICY2, "INetFwPolicy2", iid, out)
}
unsafe extern "win64" fn rules_qi(_: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    static_qi(&RULES_OBJ, IID_INET_FW_RULES, "INetFwRules", iid, out)
}

static POLICY2_VT: VTable<29> = VTable([
    policy2_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    policy2_profile_types as *const (),
    policy2_true as *const (),          // get_FirewallEnabled
    ok as *const (),                    // put_FirewallEnabled
    policy2_variant_empty as *const (), // get_ExcludedInterfaces
    ok as *const (),                    // put_ExcludedInterfaces
    policy2_false as *const (),         // get_BlockAllInboundTraffic
    ok as *const (),
    policy2_false as *const (), // get_NotificationsDisabled
    ok as *const (),
    policy2_false as *const (), // get_UnicastResponsesToMulticastBroadcastDisabled
    ok as *const (),
    policy2_rules as *const (),
    policy2_service_restriction as *const (), // get_ServiceRestriction
    ok as *const (),                          // EnableRuleGroup
    policy2_group_enabled as *const (),       // IsRuleGroupEnabled
    ok as *const (),                          // RestoreLocalFirewallDefaults
    policy2_inbound as *const (),
    ok as *const (),
    policy2_outbound as *const (),
    ok as *const (),
    policy2_group_current as *const (), // get_IsRuleGroupCurrentlyEnabled
    policy2_modify_state as *const (),
]);

unsafe extern "win64" fn policy2_profile_types(_: usize, out: *mut u32) -> i32 {
    u32_out(out, PROFILE2_PRIVATE)
}
unsafe extern "win64" fn policy2_true(_: usize, _profile: u32, out: *mut i16) -> i32 {
    bool_out(out, true)
}
unsafe extern "win64" fn policy2_false(_: usize, _profile: u32, out: *mut i16) -> i32 {
    bool_out(out, false)
}
unsafe extern "win64" fn policy2_variant_empty(_: usize, _profile: u32, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_bytes(out, 0, 24);
    S_OK
}
unsafe extern "win64" fn policy2_rules(_: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = &RULES_OBJ as *const Static as *mut c_void;
    S_OK
}
unsafe extern "win64" fn policy2_group_enabled(
    _: usize,
    _mask: u32,
    _group: *const u16,
    out: *mut i16,
) -> i32 {
    bool_out(out, false)
}
unsafe extern "win64" fn policy2_inbound(_: usize, _profile: u32, out: *mut u32) -> i32 {
    u32_out(out, ACTION_BLOCK)
}
unsafe extern "win64" fn policy2_outbound(_: usize, _profile: u32, out: *mut u32) -> i32 {
    u32_out(out, ACTION_ALLOW)
}
unsafe extern "win64" fn policy2_group_current(_: usize, _group: *const u16, out: *mut i16) -> i32 {
    bool_out(out, false)
}
unsafe extern "win64" fn policy2_modify_state(_: usize, out: *mut u32) -> i32 {
    // NET_FW_MODIFY_STATE_OK.
    u32_out(out, 0)
}

static RULES_VT: VTable<12> = VTable([
    rules_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    type_info_count as *const (),
    get_type_info as *const (),
    get_ids_of_names as *const (),
    invoke as *const (),
    rules_count as *const (),
    rules_add as *const (),
    rules_remove as *const (),
    rules_item as *const (),
    rules_new_enum as *const (), // get__NewEnum
]);

unsafe extern "win64" fn rules_count(_: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = rules().len() as i32;
    S_OK
}

/// Lit les propriétés de n'importe quel INetFwRule (le nôtre, ou celui d'un
/// programme x86 derrière son mandataire) par sa table de méthodes.
unsafe fn read_rule(rule: usize) -> RuleData {
    let mut data = default_rule();
    for (k, kind) in KINDS.iter().enumerate() {
        let get = 7 + 2 * k;
        match kind {
            Kind::Str => {
                if let Some(s) = read_bstr_prop(rule, get) {
                    data[k] = Prop::S(s);
                }
            }
            Kind::Num => {
                if let Some(v) = read_u32_prop(rule, get) {
                    data[k] = Prop::N(v);
                }
            }
            Kind::Bool => {
                if let Some(v) = read_u32_prop(rule, get) {
                    data[k] = Prop::N((v as u16 != 0) as u32);
                }
            }
            Kind::Variant => {}
        }
    }
    data
}

unsafe extern "win64" fn rules_add(_: usize, rule: usize) -> i32 {
    if rule == 0 {
        return E_POINTER;
    }
    let data = read_rule(rule);
    let text = |k: usize| match &data[k] {
        Prop::S(s) => s.clone(),
        Prop::N(n) => n.to_string(),
    };
    crate::trace!(
        "INetFwRules::Add(« {} », programme {}, direction {}, action {}) : acceptée (pare-feu par profil, pas par règle)",
        text(0),
        text(2),
        text(10),
        text(17)
    );
    rules().push(data);
    S_OK
}

unsafe extern "win64" fn rules_remove(_: usize, name: *const u16) -> i32 {
    let name = bstr_in(name);
    let mut list = rules();
    if let Some(i) = list.iter().position(|r| same_name(name_of(r), &name)) {
        list.remove(i);
    }
    S_OK
}

unsafe extern "win64" fn rules_item(_: usize, name: *const u16, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let name = bstr_in(name);
    let found = rules()
        .iter()
        .find(|r| same_name(name_of(r), &name))
        .cloned();
    match found {
        Some(data) => {
            *out = new_rule(data) as *mut c_void;
            S_OK
        }
        None => HR_NOT_FOUND,
    }
}

// ----- INetFwRule : objet compté -----

#[repr(C)]
struct Rule {
    vtbl: *const (),
    refs: AtomicU32,
    data: Mutex<RuleData>,
}

fn new_rule(data: RuleData) -> *mut Rule {
    Box::into_raw(Box::new(Rule {
        vtbl: RULE_VT.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        data: Mutex::new(data),
    }))
}

unsafe extern "win64" fn rule_qi(this: *mut Rule, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let want = *iid;
    if want == IID_IUNKNOWN || want == IID_IDISPATCH || want == IID_INET_FW_RULE {
        rule_add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    log_no_interface("INetFwRule", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn rule_add_ref(this: *mut Rule) -> u32 {
    (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn rule_release(this: *mut Rule) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

unsafe fn rule_get(this: *mut Rule, k: usize, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    let value = (*this).data.lock().unwrap_or_else(|e| e.into_inner())[k].clone();
    match (KINDS[k], value) {
        (Kind::Str, Prop::S(s)) => bstr_out(out as *mut *mut u16, &s),
        (Kind::Bool, Prop::N(n)) => bool_out(out as *mut i16, n != 0),
        (Kind::Num, Prop::N(n)) => u32_out(out as *mut u32, n),
        _ => {
            // Interfaces : VARIANT vide (toutes les interfaces).
            std::ptr::write_bytes(out as *mut u8, 0, 24);
            S_OK
        }
    }
}

unsafe fn rule_put(this: *mut Rule, k: usize, v: usize) -> i32 {
    let value = match KINDS[k] {
        Kind::Str => Prop::S(bstr_in(v as *const u16)),
        Kind::Num => Prop::N(v as u32),
        Kind::Bool => Prop::N((v as u16 != 0) as u32),
        Kind::Variant => return S_OK,
    };
    (*this).data.lock().unwrap_or_else(|e| e.into_inner())[k] = value;
    S_OK
}

macro_rules! rule_props {
    ($($k:literal => $get:ident, $put:ident;)*) => {
        $(
            unsafe extern "win64" fn $get(this: *mut Rule, out: usize) -> i32 {
                rule_get(this, $k, out)
            }
            unsafe extern "win64" fn $put(this: *mut Rule, v: usize) -> i32 {
                rule_put(this, $k, v)
            }
        )*
        static RULE_VT: VTable<43> = VTable([
            rule_qi as *const (),
            rule_add_ref as *const (),
            rule_release as *const (),
            type_info_count as *const (),
            get_type_info as *const (),
            get_ids_of_names as *const (),
            invoke as *const (),
            $($get as *const (), $put as *const (),)*
        ]);
    };
}

rule_props! {
    0 => get_name, put_name;
    1 => get_description, put_description;
    2 => get_application, put_application;
    3 => get_service, put_service;
    4 => get_protocol, put_protocol;
    5 => get_local_ports, put_local_ports;
    6 => get_remote_ports, put_remote_ports;
    7 => get_local_addresses, put_local_addresses;
    8 => get_remote_addresses, put_remote_addresses;
    9 => get_icmp, put_icmp;
    10 => get_direction, put_direction;
    11 => get_interfaces, put_interfaces;
    12 => get_interface_types, put_interface_types;
    13 => get_enabled, put_enabled;
    14 => get_grouping, put_grouping;
    15 => get_profiles, put_profiles;
    16 => get_edge_traversal, put_edge_traversal;
    17 => get_action, put_action;
}

/// CoCreateInstance de CLSID_NetFwPolicy2 et CLSID_NetFwRule (appelé par
/// `netfw::create_builtin`).
///
/// # Safety
/// `out` doit être valide en écriture.
pub(super) unsafe fn create_builtin(
    clsid: &Guid,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    if *clsid == CLSID_NET_FW_POLICY2 {
        return Some(policy2_qi(0, iid, out));
    }
    if *clsid == CLSID_NET_FW_RULE {
        let rule = new_rule(default_rule());
        let hr = rule_qi(rule, iid, out);
        rule_release(rule);
        return Some(hr);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn method<F>(obj: *mut c_void, index: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(index))
    }

    unsafe fn bstr(s: &str) -> *mut u16 {
        let w: Vec<u16> = s.encode_utf16().collect();
        super::super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32)
    }

    /// Le parcours de SteamService : NetFwPolicy2 → get_Rules, NetFwRule
    /// rempli, Rules.Add, puis Item retrouve la règle.
    #[test]
    fn regle_de_pare_feu_acceptee_et_retrouvee() {
        unsafe {
            let mut policy: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_NET_FW_POLICY2, &IID_INET_FW_POLICY2, &mut policy),
                Some(S_OK)
            );
            let get_rules: extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 =
                method(policy, 18);
            let mut list = std::ptr::null_mut();
            assert_eq!(get_rules(policy, &mut list), S_OK);
            let enabled: extern "win64" fn(*mut c_void, u32, *mut i16) -> i32 = method(policy, 8);
            let mut on = 0i16;
            assert_eq!(enabled(policy, 2, &mut on), S_OK);
            assert_eq!(on, -1);

            let mut rule: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_NET_FW_RULE, &IID_INET_FW_RULE, &mut rule),
                Some(S_OK)
            );
            let put_s: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(rule, 8);
            assert_eq!(put_s(rule, bstr("Steam (règle 1919)")), S_OK);
            let put_s: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(rule, 12);
            assert_eq!(put_s(rule, bstr("C:\\Steam\\steam.exe")), S_OK);
            let put_b: extern "win64" fn(*mut c_void, i16) -> i32 = method(rule, 34);
            assert_eq!(put_b(rule, -1), S_OK);
            let get_proto: extern "win64" fn(*mut c_void, *mut u32) -> i32 = method(rule, 15);
            let mut proto = 0;
            assert_eq!(get_proto(rule, &mut proto), S_OK);
            assert_eq!(proto, 256, "protocole quelconque par défaut");

            let add: extern "win64" fn(*mut c_void, *mut c_void) -> i32 = method(list, 8);
            assert_eq!(add(list, rule), S_OK);
            let item: extern "win64" fn(*mut c_void, *mut u16, *mut *mut c_void) -> i32 =
                method(list, 10);
            let mut found = std::ptr::null_mut();
            assert_eq!(item(list, bstr("STEAM (RÈGLE 1919)"), &mut found), S_OK);
            let get_app: extern "win64" fn(*mut c_void, *mut *mut u16) -> i32 = method(found, 11);
            let mut b = std::ptr::null_mut();
            assert_eq!(get_app(found, &mut b), S_OK);
            assert_eq!(bstr_in(b), "C:\\Steam\\steam.exe");
            let get_en: extern "win64" fn(*mut c_void, *mut i16) -> i32 = method(found, 33);
            let mut en = 0i16;
            assert_eq!(get_en(found, &mut en), S_OK);
            assert_eq!(en, -1);

            let remove: extern "win64" fn(*mut c_void, *mut u16) -> i32 = method(list, 9);
            assert_eq!(remove(list, bstr("Steam (règle 1919)")), S_OK);
            let mut missing = std::ptr::null_mut();
            assert_eq!(
                item(list, bstr("Steam (règle 1919)"), &mut missing),
                HR_NOT_FOUND
            );

            let release: extern "win64" fn(*mut c_void) -> u32 = method(found, 2);
            release(found);
            release(rule);
        }
    }
}
