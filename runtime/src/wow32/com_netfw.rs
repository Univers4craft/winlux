//! Pare-feu, ancienne interface HNetCfg.FwMgr, pour les programmes x86
//! (#1919).
//!
//! Description des interfaces de `netfw.h` (mingw-w64, domaine public) pour
//! les mandataires COM de `com.rs` : SteamService.exe est un programme x86.
//! Les objets eux-mêmes sont ceux de `dlls/netfw.rs`.
//!
//! Conventions : un BSTR passé est un pointeur (`I`), un BSTR rendu
//! (`BSTR*`) une valeur de la taille d'un pointeur (`OutPtr`) ; les
//! énumérations, LONG et VARIANT_BOOL, passés ou rendus par pointeur, sont
//! `I` ; un BYTE passé occupe une case de 4 octets ; un VARIANT* rendu est
//! `Var`.

use super::com::{Iface, A, A::*, IDISPATCH, IUNKNOWN};
use crate::dlls::{netfw, netfw2};

const BS_GET: Option<&[A]> = Some(&[OutPtr]);
const ONE: Option<&[A]> = Some(&[I]);
const NONE: Option<&[A]> = Some(&[]);

pub(super) static INETFWMGR: Iface = Iface {
    name: "INetFwMgr",
    iid: netfw::IID_INET_FW_MGR,
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[Out(&INETFWPOLICY)]),      // get_LocalPolicy
        ONE,                              // get_CurrentProfileType
        NONE,                             // RestoreDefaults
        Some(&[I, I, I, I, I, Var, Var]), // IsPortAllowed
        Some(&[I, I, I, Var, Var]),       // IsIcmpTypeAllowed
    ],
};

pub(super) static INETFWPOLICY: Iface = Iface {
    name: "INetFwPolicy",
    iid: netfw::IID_INET_FW_POLICY,
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[Out(&INETFWPROFILE)]),    // get_CurrentProfile
        Some(&[I, Out(&INETFWPROFILE)]), // GetProfileByType
    ],
};

pub(super) static INETFWPROFILE: Iface = Iface {
    name: "INetFwProfile",
    iid: netfw::IID_INET_FW_PROFILE,
    parent: Some(&IDISPATCH),
    methods: &[
        ONE,                                         // get_Type
        ONE,                                         // get_FirewallEnabled
        ONE,                                         // put_FirewallEnabled
        ONE,                                         // get_ExceptionsNotAllowed
        ONE,                                         // put_ExceptionsNotAllowed
        ONE,                                         // get_NotificationsDisabled
        ONE,                                         // put_NotificationsDisabled
        ONE,  // get_UnicastResponsesToMulticastBroadcastDisabled
        ONE,  // put_UnicastResponsesToMulticastBroadcastDisabled
        None, // get_RemoteAdminSettings (non servi)
        None, // get_IcmpSettings
        None, // get_GloballyOpenPorts
        None, // get_Services
        Some(&[Out(&INETFWAUTHORIZEDAPPLICATIONS)]), // get_AuthorizedApplications
    ],
};

pub(super) static INETFWAUTHORIZEDAPPLICATIONS: Iface = Iface {
    name: "INetFwAuthorizedApplications",
    iid: netfw::IID_INET_FW_AUTHORIZED_APPLICATIONS,
    parent: Some(&IDISPATCH),
    methods: &[
        ONE,                                           // get_Count
        Some(&[In(&INETFWAUTHORIZEDAPPLICATION)]),     // Add
        ONE,                                           // Remove
        Some(&[I, Out(&INETFWAUTHORIZEDAPPLICATION)]), // Item
        Some(&[Out(&IUNKNOWN)]),                       // get__NewEnum
    ],
};

pub(super) static INETFWAUTHORIZEDAPPLICATION: Iface = Iface {
    name: "INetFwAuthorizedApplication",
    iid: netfw::IID_INET_FW_AUTHORIZED_APPLICATION,
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET, // get_Name
        ONE,    // put_Name
        BS_GET, // get_ProcessImageFileName
        ONE,    // put_ProcessImageFileName
        ONE,    // get_IpVersion
        ONE,    // put_IpVersion
        ONE,    // get_Scope
        ONE,    // put_Scope
        BS_GET, // get_RemoteAddresses
        ONE,    // put_RemoteAddresses
        ONE,    // get_Enabled
        ONE,    // put_Enabled
    ],
};

pub(super) static INETFWPOLICY2: Iface = Iface {
    name: "INetFwPolicy2",
    iid: netfw2::IID_INET_FW_POLICY2,
    parent: Some(&IDISPATCH),
    methods: &[
        ONE,                        // get_CurrentProfileTypes
        Some(&[I, I]),              // get_FirewallEnabled
        Some(&[I, I]),              // put_FirewallEnabled
        Some(&[I, Var]),            // get_ExcludedInterfaces
        Some(&[I, VarIn]),          // put_ExcludedInterfaces
        Some(&[I, I]),              // get_BlockAllInboundTraffic
        Some(&[I, I]),              // put_BlockAllInboundTraffic
        Some(&[I, I]),              // get_NotificationsDisabled
        Some(&[I, I]),              // put_NotificationsDisabled
        Some(&[I, I]),              // get_UnicastResponsesToMulticastBroadcastDisabled
        Some(&[I, I]),              // put_UnicastResponsesToMulticastBroadcastDisabled
        Some(&[Out(&INETFWRULES)]), // get_Rules
        None,                       // get_ServiceRestriction (non servi)
        Some(&[I, I, I]),           // EnableRuleGroup
        Some(&[I, I, I]),           // IsRuleGroupEnabled
        NONE,                       // RestoreLocalFirewallDefaults
        Some(&[I, I]),              // get_DefaultInboundAction
        Some(&[I, I]),              // put_DefaultInboundAction
        Some(&[I, I]),              // get_DefaultOutboundAction
        Some(&[I, I]),              // put_DefaultOutboundAction
        Some(&[I, I]),              // get_IsRuleGroupCurrentlyEnabled
        ONE,                        // get_LocalPolicyModifyState
    ],
};

pub(super) static INETFWRULES: Iface = Iface {
    name: "INetFwRules",
    iid: netfw2::IID_INET_FW_RULES,
    parent: Some(&IDISPATCH),
    methods: &[
        ONE,                          // get_Count
        Some(&[In(&INETFWRULE)]),     // Add
        ONE,                          // Remove
        Some(&[I, Out(&INETFWRULE)]), // Item
        Some(&[Out(&IUNKNOWN)]),      // get__NewEnum
    ],
};

/// INetFwRule : 18 propriétés, get_ puis put_. Chaînes : BSTR rendu
/// (`OutPtr`) et passé (`I`) ; Interfaces : VARIANT (`Var` / `VarIn`) ;
/// le reste (LONG, VARIANT_BOOL, énumérations) : `I`.
pub(super) static INETFWRULE: Iface = Iface {
    name: "INetFwRule",
    iid: netfw2::IID_INET_FW_RULE,
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET,
        ONE, // Name
        BS_GET,
        ONE, // Description
        BS_GET,
        ONE, // ApplicationName
        BS_GET,
        ONE, // ServiceName
        ONE,
        ONE, // Protocol
        BS_GET,
        ONE, // LocalPorts
        BS_GET,
        ONE, // RemotePorts
        BS_GET,
        ONE, // LocalAddresses
        BS_GET,
        ONE, // RemoteAddresses
        BS_GET,
        ONE, // IcmpTypesAndCodes
        ONE,
        ONE, // Direction
        Some(&[Var]),
        Some(&[VarIn]), // Interfaces
        BS_GET,
        ONE, // InterfaceTypes
        ONE,
        ONE, // Enabled
        BS_GET,
        ONE, // Grouping
        ONE,
        ONE, // Profiles
        ONE,
        ONE, // EdgeTraversal
        ONE,
        ONE, // Action
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &INETFWPOLICY2,
    &INETFWRULES,
    &INETFWRULE,
    &INETFWMGR,
    &INETFWPOLICY,
    &INETFWPROFILE,
    &INETFWAUTHORIZEDAPPLICATIONS,
    &INETFWAUTHORIZEDAPPLICATION,
];
