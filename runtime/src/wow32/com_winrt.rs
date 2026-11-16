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
