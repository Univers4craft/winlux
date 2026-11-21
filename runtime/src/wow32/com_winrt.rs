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

// Windows.Management.Deployment.PackageManager (#2740), servi par
// `dlls/winrt_package_manager.rs`. Ordre et paramètres : windows-rs 0.62.2
// (MIT), `IPackageManager_Vtbl` et `IPackageManager2_Vtbl`. Chaque
// paramètre tient 4 octets en x86 : HSTRING, interface, énumération
// (DeploymentOptions, PackageTypes, RemovalOptions u32 ; PackageState
// i32). Les interfaces passées (Uri, IIterable<Uri>) restent `I` : le
// runtime ne les lit pas. Une opération asynchrone ou un Package rendu est
// décrit en IInspectable : le runtime rend toujours null pour eux.
const PKG: &Iface = &IITERABLE_PACKAGE;

/// IPackageManager {9A7D4B65-5E8F-4FC7-A2E5-7F6925CB8B53}.
pub(super) static IPACKAGEMANAGER: Iface = Iface {
    name: "IPackageManager",
    iid: crate::dlls::winrt_package_manager::IID_PACKAGE_MANAGER,
    parent: Some(&IINSPECTABLE),
    methods: &[
        Some(&[I, I, I, Out(&IINSPECTABLE)]), // AddPackageAsync
        Some(&[I, I, I, Out(&IINSPECTABLE)]), // UpdatePackageAsync
        Some(&[I, Out(&IINSPECTABLE)]),       // RemovePackageAsync
        Some(&[I, I, Out(&IINSPECTABLE)]),    // StagePackageAsync
        Some(&[I, I, I, Out(&IINSPECTABLE)]), // RegisterPackageAsync
        Some(&[Out(PKG)]),                    // FindPackages
        Some(&[I, Out(PKG)]),                 // FindPackagesByUserSecurityId
        Some(&[I, I, Out(PKG)]),              // FindPackagesByNamePublisher
        Some(&[I, I, I, Out(PKG)]),           // …ByUserSecurityIdNamePublisher
        Some(&[I, Out(&IITERABLE_USER)]),     // FindUsers
        Some(&[I, I]),                        // SetPackageState
        Some(&[I, Out(&IINSPECTABLE)]),       // FindPackageByPackageFullName
        Some(&[I, I, Out(&IINSPECTABLE)]),    // CleanupPackageForUserAsync
        Some(&[I, Out(PKG)]),                 // FindPackagesByPackageFamilyName
        Some(&[I, I, Out(PKG)]),              // …ByUserSecurityIdPackageFamilyName
        Some(&[I, I, Out(&IINSPECTABLE)]),    // FindPackageByUserSecurityIdPackageFullName
    ],
};

/// IPackageManager2 {F7AAD08D-0840-46F2-B5D8-CAD47693A095}.
pub(super) static IPACKAGEMANAGER2: Iface = Iface {
    name: "IPackageManager2",
    iid: crate::dlls::winrt_package_manager::IID_PACKAGE_MANAGER2,
    parent: Some(&IINSPECTABLE),
    methods: &[
        Some(&[I, I, Out(&IINSPECTABLE)]), // RemovePackageWithOptionsAsync
        Some(&[I, I, I, Out(&IINSPECTABLE)]), // StagePackageWithOptionsAsync
        Some(&[I, I, I, Out(&IINSPECTABLE)]), // RegisterPackageByFullNameAsync
        Some(&[I, Out(PKG)]),              // FindPackagesWithPackageTypes
        Some(&[I, I, Out(PKG)]),           // …ByUserSecurityId…
        Some(&[I, I, I, Out(PKG)]),        // …ByNamePublisher…
        Some(&[I, I, I, I, Out(PKG)]),     // …ByUserSecurityIdNamePublisher…
        Some(&[I, I, Out(PKG)]),           // …ByPackageFamilyName…
        Some(&[I, I, I, Out(PKG)]),        // …ByUserSecurityIdPackageFamilyName…
        Some(&[I, Out(&IINSPECTABLE)]),    // StageUserDataAsync
    ],
};

/// IIterator<T> d'une collection : Current (T rendu en IInspectable),
/// HasCurrent(boolean*), MoveNext(boolean*), GetMany(UINT32, T*, UINT32*).
/// GetMany est décrit sans tableau d'interfaces : le runtime ne rend que des
/// collections vides, qui n'y écrivent rien.
macro_rules! iterator {
    ($name:literal, $iid:expr) => {
        Iface {
            name: $name,
            iid: $iid,
            parent: Some(&IINSPECTABLE),
            methods: &[
                Some(&[Out(&IINSPECTABLE)]), // get_Current
                Some(&[I]),                  // get_HasCurrent
                Some(&[I]),                  // MoveNext
                Some(&[I, I, I]),            // GetMany
            ],
        }
    };
}

pub(super) static IITERATOR_PACKAGE: Iface = iterator!(
    "IIterator<Package>",
    crate::dlls::winrt_package_manager::IID_ITERATOR_PACKAGE
);
pub(super) static IITERATOR_USER: Iface = iterator!(
    "IIterator<PackageUserInformation>",
    crate::dlls::winrt_package_manager::IID_ITERATOR_USER
);
/// IIterable<Package> : First(IIterator<Package>**).
pub(super) static IITERABLE_PACKAGE: Iface = Iface {
    name: "IIterable<Package>",
    iid: crate::dlls::winrt_package_manager::IID_ITERABLE_PACKAGE,
    parent: Some(&IINSPECTABLE),
    methods: &[Some(&[Out(&IITERATOR_PACKAGE)])],
};
/// IIterable<PackageUserInformation> : First.
pub(super) static IITERABLE_USER: Iface = Iface {
    name: "IIterable<PackageUserInformation>",
    iid: crate::dlls::winrt_package_manager::IID_ITERABLE_USER,
    parent: Some(&IINSPECTABLE),
    methods: &[Some(&[Out(&IITERATOR_USER)])],
};

pub(super) static ALL: &[&Iface] = &[
    &IINSPECTABLE,
    &IACTIVATIONFACTORY,
    &IPACKAGEMANAGER,
    &IPACKAGEMANAGER2,
    &IITERABLE_PACKAGE,
    &IITERATOR_PACKAGE,
    &IITERABLE_USER,
    &IITERATOR_USER,
];

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
        // #2740 : PackageManager, 16 + 10 méthodes après IInspectable
        // (windows-rs 0.62.2), et ses collections.
        assert_eq!(IPACKAGEMANAGER.count(), 22);
        assert_eq!(IPACKAGEMANAGER2.count(), 16);
        assert_eq!(IITERABLE_PACKAGE.count(), 7);
        assert_eq!(IITERATOR_PACKAGE.count(), 10);
        // FindPackagesByUserSecurityIdNamePublisherWithPackageTypes : 4 + 1.
        assert_eq!(slots(IPACKAGEMANAGER2.method(12).unwrap()), 5);
        // SetPackageState(HSTRING, PackageState).
        assert_eq!(slots(IPACKAGEMANAGER.method(16).unwrap()), 2);
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
