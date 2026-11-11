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
use crate::dlls::netfw;

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

pub(super) static ALL: &[&Iface] = &[
    &INETFWMGR,
    &INETFWPOLICY,
    &INETFWPROFILE,
    &INETFWAUTHORIZEDAPPLICATIONS,
    &INETFWAUTHORIZEDAPPLICATION,
];
