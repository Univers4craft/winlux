//! Gestionnaire de listes de réseaux (NetworkListManager) pour les
//! programmes x86 (#2459).
//!
//! Description des interfaces servies par `dlls/netlistmgr.rs` pour les
//! mandataires COM de `com.rs` : l'outil de déploiement d'Office (x86) crée
//! cet objet ; sans description, wow32 le refusait (« interface COM
//! {DCB00000-…} non décrite pour les programmes 32 bits »).
//!
//! Ordre des méthodes et paramètres : documentation publique
//! d'INetworkListManager, IEnumNetworks, IEnumNetworkConnections,
//! IConnectionPointContainer et IConnectionPoint. Conventions de
//! `com_netfw.rs` : VARIANT_BOOL*, NLM_CONNECTIVITY*, DWORD* et les
//! énumérations sont `I` ; un GUID passé par valeur est `GuidIn`.

use super::com::{Iface, A, A::*, IDISPATCH, IUNKNOWN};
use crate::dlls::netlistmgr as nlm;

const ONE: Option<&[A]> = Some(&[I]);
const NONE: Option<&[A]> = Some(&[]);

pub(super) static INETWORKLISTMANAGER: Iface = Iface {
    name: "INetworkListManager",
    iid: nlm::IID_INETWORK_LIST_MANAGER,
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[I, Out(&IENUMNETWORKS)]),        // GetNetworks(Flags, ppEnum)
        Some(&[GuidIn, Out(&IUNKNOWN)]),        // GetNetwork(gdNetworkId, ppNetwork)
        Some(&[Out(&IENUMNETWORKCONNECTIONS)]), // GetNetworkConnections
        Some(&[GuidIn, Out(&IUNKNOWN)]),        // GetNetworkConnection
        ONE,                                    // get_IsConnectedToInternet
        ONE,                                    // get_IsConnected
        ONE,                                    // GetConnectivity
        ONE,                                    // SetSimulatedProfileInfo
        NONE,                                   // ClearSimulatedProfileInfo
    ],
};

/// IEnumNetworks et IEnumNetworkConnections : l'énumération du runtime est
/// toujours vide, Next n'écrit jamais dans le tableau (`I`).
pub(super) static IENUMNETWORKS: Iface = Iface {
    name: "IEnumNetworks",
    iid: nlm::IID_IENUM_NETWORKS,
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[Out(&IUNKNOWN)]),      // get__NewEnum
        Some(&[I, I, I]),             // Next(celt, rgelt, pceltFetched)
        ONE,                          // Skip
        NONE,                         // Reset
        Some(&[Out(&IENUMNETWORKS)]), // Clone
    ],
};

pub(super) static IENUMNETWORKCONNECTIONS: Iface = Iface {
    name: "IEnumNetworkConnections",
    iid: nlm::IID_IENUM_NETWORK_CONNECTIONS,
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[Out(&IUNKNOWN)]),                // get__NewEnum
        Some(&[I, I, I]),                       // Next
        ONE,                                    // Skip
        NONE,                                   // Reset
        Some(&[Out(&IENUMNETWORKCONNECTIONS)]), // Clone
    ],
};

pub(super) static ICONNECTIONPOINTCONTAINER: Iface = Iface {
    name: "IConnectionPointContainer",
    iid: nlm::IID_ICONNECTION_POINT_CONTAINER,
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[Out(&IUNKNOWN)]),            // EnumConnectionPoints
        Some(&[I, Out(&ICONNECTIONPOINT)]), // FindConnectionPoint(riid, ppCP)
    ],
};

pub(super) static ICONNECTIONPOINT: Iface = Iface {
    name: "IConnectionPoint",
    iid: nlm::IID_ICONNECTION_POINT,
    parent: Some(&IUNKNOWN),
    methods: &[
        ONE,                                      // GetConnectionInterface(IID*)
        Some(&[Out(&ICONNECTIONPOINTCONTAINER)]), // GetConnectionPointContainer
        Some(&[In(&IUNKNOWN), I]),                // Advise(pUnkSink, pdwCookie)
        ONE,                                      // Unadvise(dwCookie)
        Some(&[Out(&IUNKNOWN)]),                  // EnumConnections
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &INETWORKLISTMANAGER,
    &IENUMNETWORKS,
    &IENUMNETWORKCONNECTIONS,
    &ICONNECTIONPOINTCONTAINER,
    &ICONNECTIONPOINT,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// #2459 : chaque interface du NetworkListManager est décrite en entier,
    /// avec autant de méthodes que la table du runtime, et se retrouve par
    /// son IID.
    #[test]
    fn gestionnaire_de_reseaux_decrit() {
        for (f, n) in [
            (&INETWORKLISTMANAGER, 16),
            (&IENUMNETWORKS, 12),
            (&IENUMNETWORKCONNECTIONS, 12),
            (&ICONNECTIONPOINTCONTAINER, 5),
            (&ICONNECTIONPOINT, 8),
        ] {
            assert_eq!(f.count(), n, "{}", f.name);
            // IDispatch (3 à 6) reste non décrite, comme ailleurs.
            let first = if f.parent.is_some_and(|p| std::ptr::eq(p, &IDISPATCH)) {
                7
            } else {
                3
            };
            for i in first..f.count() {
                assert!(f.method(i).is_some(), "{} n° {i}", f.name);
            }
            assert!(super::super::com::by_iid(&f.iid).is_some_and(|g| std::ptr::eq(g, f)));
        }
        // GetNetwork : GUID par valeur (4 cases) + pointeur rendu.
        assert_eq!(
            super::super::com::slots(INETWORKLISTMANAGER.method(8).unwrap()),
            5
        );
    }
}
