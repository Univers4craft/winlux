//! Classe Windows Runtime `Windows.Management.Deployment.PackageManager`
//! (#2740).
//!
//! L'outil de déploiement d'Office (setup.exe, x86), lancé en
//! administrateur, demande cette fabrique puis s'arrête sur terminate()
//! quand elle manque. Learn, PackageManager : classe activable par un
//! constructeur par défaut ; FindPackages « requires administrative
//! privileges », d'où la seule instance élevée qui la demande.
//!
//! Winlux n'installe aucun paquet MSIX : chaque recherche rend une vraie
//! collection vide (IIterable dont l'itérateur n'a pas d'élément courant),
//! FindPackageByPackageFullName et FindPackageByUserSecurityIdPackageFullName
//! rendent null, FindUsers une collection vide. Les opérations de
//! déploiement (Add, Update, Remove, Stage, Register, Cleanup,
//! StageUserData) et SetPackageState rendent E_NOTIMPL, journalisé une fois
//! chacune avec ce fichier et la ligne.
//!
//! Sources : Learn (PackageManager, ses méthodes, IIterable et IIterator) ;
//! IID et ordre des méthodes d'IPackageManager et d'IPackageManager2,
//! IID d'IPackage et d'IPackageUserInformation : windows-rs 0.62.2,
//! `Windows/Management/Deployment` et `Windows/ApplicationModel` (MIT). Les
//! IID d'IIterable et IIterator<Package> et <PackageUserInformation>
//! suivent l'algorithme public des pinterface, vérifié par le test.

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
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
pub(crate) const IID_PACKAGE_MANAGER: Guid = g(0x9a7d_4b65, 0x5e8f, 0x4fc7, 0xa2e5_7f69_25cb_8b53);
pub(crate) const IID_PACKAGE_MANAGER2: Guid = g(0xf7aa_d08d, 0x0840, 0x46f2, 0xb5d8_cad4_7693_a095);
pub(crate) const IID_ITERABLE_PACKAGE: Guid = g(0x69ad_6aa7, 0x0c49, 0x5f27, 0xa5eb_ef4d_5946_7b6d);
pub(crate) const IID_ITERATOR_PACKAGE: Guid = g(0x0217_f069, 0x025c, 0x5ee6, 0xa87f_e782_e3b6_23ae);
pub(crate) const IID_ITERABLE_USER: Guid = g(0x3413_48b9, 0x52c8, 0x5b57, 0x9e91_f19f_2a05_b188);
pub(crate) const IID_ITERATOR_USER: Guid = g(0x7566_0566, 0xae43, 0x5858, 0xada6_d57d_dae9_0277);

const CLASS: &str = "Windows.Management.Deployment.PackageManager";
const ITERABLE_PACKAGE_CLASS: &str =
    "Windows.Foundation.Collections.IIterable`1<Windows.ApplicationModel.Package>";
const ITERATOR_PACKAGE_CLASS: &str =
    "Windows.Foundation.Collections.IIterator`1<Windows.ApplicationModel.Package>";
const ITERABLE_USER_CLASS: &str =
    "Windows.Foundation.Collections.IIterable`1<Windows.Management.Deployment.PackageUserInformation>";
const ITERATOR_USER_CLASS: &str =
    "Windows.Foundation.Collections.IIterator`1<Windows.Management.Deployment.PackageUserInformation>";

/// Méthode non servie : une ligne de journal par méthode, avec sa place.
fn not_implemented(method: &str, line: u32) -> i32 {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.iter().any(|m| m == method) {
        seen.push(method.to_string());
        crate::log_warn!(
            "com",
            "PackageManager : {method} pas encore servie (E_NOTIMPL, {}:{line})",
            file!()
        );
    }
    E_NOTIMPL
}

unsafe fn put<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    out.write(v);
    S_OK
}

unsafe fn hstring(text: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

// ============================================================
//  Objets statiques : aucun état, jamais libérés
// ============================================================

#[repr(C)]
struct Single<const N: usize> {
    vtbl: &'static VTable<N>,
}
#[repr(C)]
struct Manager {
    v1: &'static VTable<22>,
    v2: &'static VTable<16>,
}

static FACTORY: Single<7> = Single {
    vtbl: &FACTORY_VTBL,
};
static MANAGER: Manager = Manager {
    v1: &MANAGER_VTBL,
    v2: &MANAGER2_VTBL,
};
static ITERABLE_PACKAGE: Single<7> = Single {
    vtbl: &ITERABLE_VTBL,
};
static ITERATOR_PACKAGE: Single<10> = Single {
    vtbl: &ITERATOR_VTBL,
};
static ITERABLE_USER: Single<7> = Single {
    vtbl: &ITERABLE_VTBL,
};
static ITERATOR_USER: Single<10> = Single {
    vtbl: &ITERATOR_VTBL,
};

fn addr<T>(o: &'static T) -> usize {
    o as *const T as usize
}

/// Interfaces de chaque objet : (adresse, IID servis, nom de classe).
fn object(this: usize) -> Option<(&'static [Guid], &'static str)> {
    let manager = addr(&MANAGER);
    if this == manager || this == manager + std::mem::size_of::<usize>() {
        return Some((&[IID_PACKAGE_MANAGER], CLASS));
    }
    let table: [(usize, &'static [Guid], &'static str); 5] = [
        (addr(&FACTORY), &[IID_ACTIVATION_FACTORY], CLASS),
        (
            addr(&ITERABLE_PACKAGE),
            &[IID_ITERABLE_PACKAGE],
            ITERABLE_PACKAGE_CLASS,
        ),
        (
            addr(&ITERATOR_PACKAGE),
            &[IID_ITERATOR_PACKAGE],
            ITERATOR_PACKAGE_CLASS,
        ),
        (
            addr(&ITERABLE_USER),
            &[IID_ITERABLE_USER],
            ITERABLE_USER_CLASS,
        ),
        (
            addr(&ITERATOR_USER),
            &[IID_ITERATOR_USER],
            ITERATOR_USER_CLASS,
        ),
    ];
    table
        .iter()
        .find(|(a, _, _)| *a == this)
        .map(|&(_, iids, name)| (iids, name))
}

unsafe extern "win64" fn qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(iid) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    let manager = addr(&MANAGER);
    let is_manager = this == manager || this == manager + std::mem::size_of::<usize>();
    if is_manager && iid == IID_PACKAGE_MANAGER2 {
        *out = manager + std::mem::size_of::<usize>();
        return S_OK;
    }
    let Some((iids, name)) = object(this) else {
        return E_NOINTERFACE;
    };
    if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) || iids.contains(&iid) {
        *out = if is_manager { manager } else { this };
        return S_OK;
    }
    super::comobj::log_no_interface(name, &iid as *const Guid);
    E_NOINTERFACE
}
extern "win64" fn add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn release(_this: usize) -> u32 {
    1
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
unsafe extern "win64" fn class_name(this: usize, out: *mut usize) -> i32 {
    hstring(object(this).map_or(CLASS, |(_, n)| n), out)
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    put(level, 0) // BaseTrust
}

/// ActivateInstance : le constructeur par défaut de PackageManager.
unsafe extern "win64" fn activate(_this: usize, out: *mut usize) -> i32 {
    put(out, addr(&MANAGER))
}

macro_rules! inspectable {
    () => {
        [
            qi as *const (),
            add_ref as *const (),
            release as *const (),
            get_iids as *const (),
            class_name as *const (),
            trust_level as *const (),
        ]
    };
}

const fn join<const N: usize>(head: [*const (); 6], tail: &[*const ()]) -> [*const (); N] {
    let mut v = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < 6 {
        v[i] = head[i];
        i += 1;
    }
    while i < N {
        v[i] = tail[i - 6];
        i += 1;
    }
    v
}

static FACTORY_VTBL: VTable<7> = VTable(join(inspectable!(), &[activate as *const ()]));

// ============================================================
//  IPackageManager et IPackageManager2
// ============================================================

/// Collection vide de paquets.
unsafe fn no_packages(out: *mut usize) -> i32 {
    put(out, addr(&ITERABLE_PACKAGE))
}

unsafe extern "win64" fn add_package(
    _t: usize,
    _u: usize,
    _d: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("AddPackageAsync", line!())
}
unsafe extern "win64" fn update_package(
    _t: usize,
    _u: usize,
    _d: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("UpdatePackageAsync", line!())
}
unsafe extern "win64" fn remove_package(_t: usize, _n: usize, out: *mut usize) -> i32 {
    let _ = put(out, 0);
    not_implemented("RemovePackageAsync", line!())
}
unsafe extern "win64" fn stage_package(_t: usize, _u: usize, _d: usize, out: *mut usize) -> i32 {
    let _ = put(out, 0);
    not_implemented("StagePackageAsync", line!())
}
unsafe extern "win64" fn register_package(
    _t: usize,
    _u: usize,
    _d: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("RegisterPackageAsync", line!())
}
unsafe extern "win64" fn find_packages(_t: usize, out: *mut usize) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_packages_1(_t: usize, _a: usize, out: *mut usize) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_packages_2(_t: usize, _a: usize, _b: usize, out: *mut usize) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_packages_3(
    _t: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    out: *mut usize,
) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_users(_t: usize, _n: usize, out: *mut usize) -> i32 {
    put(out, addr(&ITERABLE_USER))
}
unsafe extern "win64" fn set_package_state(_t: usize, _n: usize, _s: i32) -> i32 {
    not_implemented("SetPackageState", line!())
}
/// Aucun paquet installé : Package null (Learn : « the package … or
/// null »).
unsafe extern "win64" fn find_package_1(_t: usize, _n: usize, out: *mut usize) -> i32 {
    put(out, 0)
}
unsafe extern "win64" fn find_package_2(_t: usize, _a: usize, _n: usize, out: *mut usize) -> i32 {
    put(out, 0)
}
unsafe extern "win64" fn cleanup_package(_t: usize, _n: usize, _s: usize, out: *mut usize) -> i32 {
    let _ = put(out, 0);
    not_implemented("CleanupPackageForUserAsync", line!())
}

static MANAGER_VTBL: VTable<22> = VTable(join(
    inspectable!(),
    &[
        add_package as *const (),
        update_package as *const (),
        remove_package as *const (),
        stage_package as *const (),
        register_package as *const (),
        find_packages as *const (),     // FindPackages
        find_packages_1 as *const (),   // ByUserSecurityId
        find_packages_2 as *const (),   // ByNamePublisher
        find_packages_3 as *const (),   // ByUserSecurityIdNamePublisher
        find_users as *const (),        // FindUsers
        set_package_state as *const (), // SetPackageState
        find_package_1 as *const (),    // FindPackageByPackageFullName
        cleanup_package as *const (),   // CleanupPackageForUserAsync
        find_packages_1 as *const (),   // ByPackageFamilyName
        find_packages_2 as *const (),   // ByUserSecurityIdPackageFamilyName
        find_package_2 as *const (),    // FindPackageByUserSecurityIdPackageFullName
    ],
));

unsafe extern "win64" fn remove_with_options(
    _t: usize,
    _n: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("RemovePackageWithOptionsAsync", line!())
}
unsafe extern "win64" fn stage_with_options(
    _t: usize,
    _u: usize,
    _d: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("StagePackageWithOptionsAsync", line!())
}
unsafe extern "win64" fn register_by_full_name(
    _t: usize,
    _n: usize,
    _d: usize,
    _o: u32,
    out: *mut usize,
) -> i32 {
    let _ = put(out, 0);
    not_implemented("RegisterPackageByFullNameAsync", line!())
}
unsafe extern "win64" fn find_types_0(_t: usize, _k: u32, out: *mut usize) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_types_1(_t: usize, _a: usize, _k: u32, out: *mut usize) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_types_2(
    _t: usize,
    _a: usize,
    _b: usize,
    _k: u32,
    out: *mut usize,
) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn find_types_3(
    _t: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _k: u32,
    out: *mut usize,
) -> i32 {
    no_packages(out)
}
unsafe extern "win64" fn stage_user_data(_t: usize, _n: usize, out: *mut usize) -> i32 {
    let _ = put(out, 0);
    not_implemented("StageUserDataAsync", line!())
}

static MANAGER2_VTBL: VTable<16> = VTable(join(
    inspectable!(),
    &[
        remove_with_options as *const (),
        stage_with_options as *const (),
        register_by_full_name as *const (),
        find_types_0 as *const (), // FindPackagesWithPackageTypes
        find_types_1 as *const (), // ByUserSecurityId…
        find_types_2 as *const (), // ByNamePublisher…
        find_types_3 as *const (), // ByUserSecurityIdNamePublisher…
        find_types_1 as *const (), // ByPackageFamilyName…
        find_types_2 as *const (), // ByUserSecurityIdPackageFamilyName…
        stage_user_data as *const (),
    ],
));

// ============================================================
//  Collections vides : IIterable<T> et IIterator<T>
// ============================================================

/// IIterable::First : l'itérateur vide du même type d'élément.
unsafe extern "win64" fn first(this: usize, out: *mut usize) -> i32 {
    let it = if this == addr(&ITERABLE_USER) {
        addr(&ITERATOR_USER)
    } else {
        addr(&ITERATOR_PACKAGE)
    };
    put(out, it)
}
/// Current sans élément : E_BOUNDS (Learn, IIterator.Current : l'itérateur
/// est après la fin).
unsafe extern "win64" fn current(_this: usize, out: *mut usize) -> i32 {
    let _ = put(out, 0);
    E_BOUNDS
}
unsafe extern "win64" fn no_more(_this: usize, out: *mut u8) -> i32 {
    put(out, 0)
}
unsafe extern "win64" fn get_many(
    _this: usize,
    _capacity: u32,
    _items: usize,
    actual: *mut u32,
) -> i32 {
    put(actual, 0)
}

static ITERABLE_VTBL: VTable<7> = VTable(join(inspectable!(), &[first as *const ()]));
static ITERATOR_VTBL: VTable<10> = VTable(join(
    inspectable!(),
    &[
        current as *const (),
        no_more as *const (), // HasCurrent
        no_more as *const (), // MoveNext
        get_many as *const (),
    ],
));

/// Fabrique de PackageManager ; `None` pour une autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(qi(addr(&FACTORY), iid, out))
}

#[cfg(test)]
mod tests;
