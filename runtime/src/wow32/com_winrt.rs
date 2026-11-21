//! WinRT pour les programmes x86 (#2740).
//!
//! Un objet WinRT est un objet COM dont l'interface de base est
//! IInspectable. Ces descriptions permettent aux mandataires de `com.rs`
//! de rendre à un programme x86 les fabriques et les objets du runtime
//! (RoGetActivationFactory, RoActivateInstance) au lieu de les refuser
//! (#2428). L'outil de déploiement d'Office (x86) demande la fabrique de
//! Windows.Management.Deployment.PackageManager.
//!
//! Ordre des méthodes et paramètres : pages IInspectable et
//! IActivationFactory de Microsoft Learn (inspectable.h,
//! activation.h). Un HSTRING et une énumération sont `I` ; un HSTRING rendu
//! est `OutPtr`.

use super::com::{Iface, A::*, IUNKNOWN};
use crate::dlls::comobj::guid;

/// IInspectable {AF86E2E0-B12D-4C6A-9C5A-D7AA65101E90}.
pub(super) static IINSPECTABLE: Iface = Iface {
    name: "IInspectable",
    iid: guid(
        0xAF86_E2E0,
        0xB12D,
        0x4C6A,
        [0x9C, 0x5A, 0xD7, 0xAA, 0x65, 0x10, 0x1E, 0x90],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, OutPtr]), // GetIids(ULONG*, IID**)
        Some(&[OutPtr]),    // GetRuntimeClassName(HSTRING*)
        Some(&[I]),         // GetTrustLevel(TrustLevel*)
    ],
};

/// IActivationFactory {00000035-0000-0000-C000-000000000046}.
pub(super) static IACTIVATIONFACTORY: Iface = Iface {
    name: "IActivationFactory",
    iid: guid(0x35, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]),
    parent: Some(&IINSPECTABLE),
    methods: &[
        Some(&[Out(&IINSPECTABLE)]), // ActivateInstance(IInspectable**)
    ],
};

pub(super) static ALL: &[&Iface] = &[&IINSPECTABLE, &IACTIVATIONFACTORY];

#[cfg(test)]
mod tests {
    use super::super::com::{api_kinds, by_iid, slots};
    use super::*;

    /// #2802 : un programme x86 reçoit les fabriques et les objets WinRT du
    /// runtime par des mandataires. IInspectable a 3 méthodes après
    /// IUnknown, IActivationFactory une de plus (Learn, inspectable.h et
    /// activation.h) ; chacune se retrouve par son IID.
    #[test]
    fn winrt_decrit_pour_le_code_x86() {
        assert_eq!(IINSPECTABLE.count(), 6);
        assert_eq!(IACTIVATIONFACTORY.count(), 7);
        for f in ALL {
            for i in 0..f.count() {
                assert!(f.method(i).is_some(), "{} méthode {i}", f.name);
            }
            assert!(by_iid(&f.iid).is_some_and(|g| std::ptr::eq(g, *f)));
        }
        // GetIids(ULONG*, IID**), GetRuntimeClassName(HSTRING*),
        // GetTrustLevel(TrustLevel*), ActivateInstance(IInspectable**).
        assert_eq!(slots(IINSPECTABLE.method(3).unwrap()), 2);
        assert_eq!(slots(IINSPECTABLE.method(4).unwrap()), 1);
        assert_eq!(slots(IINSPECTABLE.method(5).unwrap()), 1);
        assert_eq!(slots(IACTIVATIONFACTORY.method(6).unwrap()), 1);
        // Les ensembles api-ms-win-core-winrt-* sont servis par ole32.
        // RoGetActivationFactory(HSTRING, REFIID, void**) : 12 octets ;
        // RoActivateInstance(HSTRING, IInspectable**) : 8 octets.
        for dll in ["ole32", "combase"] {
            let (k, r) = api_kinds(dll, "RoGetActivationFactory").unwrap();
            assert_eq!(slots(k), 3);
            assert!(r.is_none());
            let (k, r) = api_kinds(dll, "RoActivateInstance").unwrap();
            assert_eq!(slots(k), 2);
            assert!(r.is_none());
        }
    }
}
