use super::*;
use crate::dlls::comobj::pinterface_iid;

unsafe fn slot(obj: usize, n: usize) -> *const () {
    *(*(obj as *const *const *const ())).add(n)
}
unsafe fn qi_of(obj: usize, iid: &Guid) -> (i32, usize) {
    let f: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
        std::mem::transmute(slot(obj, 0));
    let mut out = 0usize;
    (f(obj, iid.as_ptr(), &mut out), out)
}
unsafe fn call_out(obj: usize, n: usize) -> (i32, usize) {
    let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 = std::mem::transmute(slot(obj, n));
    let mut out = 7usize;
    (f(obj, &mut out), out)
}
unsafe fn call_bool(obj: usize, n: usize) -> (i32, u8) {
    let f: unsafe extern "win64" fn(usize, *mut u8) -> i32 = std::mem::transmute(slot(obj, n));
    let mut out = 7u8;
    (f(obj, &mut out), out)
}

/// Parcourt une collection vide : First, HasCurrent faux, Current
/// E_BOUNDS, MoveNext faux, GetMany 0.
unsafe fn empty(iterable: usize, iid_iterable: &Guid, iid_iterator: &Guid) {
    assert_eq!(qi_of(iterable, iid_iterable), (S_OK, iterable));
    let (hr, it) = call_out(iterable, 6);
    assert_eq!(hr, S_OK);
    assert_eq!(qi_of(it, iid_iterator), (S_OK, it));
    assert_eq!(call_out(it, 6), (E_BOUNDS, 0));
    assert_eq!(call_bool(it, 7), (S_OK, 0));
    assert_eq!(call_bool(it, 8), (S_OK, 0));
    let many: unsafe extern "win64" fn(usize, u32, usize, *mut u32) -> i32 =
        std::mem::transmute(slot(it, 9));
    let mut n = 9u32;
    assert_eq!(many(it, 4, 0, &mut n), S_OK);
    assert_eq!(n, 0);
}

/// #2740 : l'outil de déploiement d'Office obtient la fabrique, une
/// instance, et des recherches vides ; les opérations de déploiement
/// rendent E_NOTIMPL.
#[test]
fn package_manager_sans_paquet_installe() {
    unsafe {
        assert!(
            activation_factory("Windows.Other", IID_ACTIVATION_FACTORY.as_ptr(), &mut 0).is_none()
        );
        let mut f = 0usize;
        assert_eq!(
            activation_factory(CLASS, IID_ACTIVATION_FACTORY.as_ptr(), &mut f),
            Some(S_OK)
        );
        let (hr, inst) = call_out(f, 6);
        assert_eq!(hr, S_OK);
        let (hr, pm) = qi_of(inst, &IID_PACKAGE_MANAGER);
        assert_eq!(hr, S_OK);
        let (hr, pm2) = qi_of(pm, &IID_PACKAGE_MANAGER2);
        assert_eq!(hr, S_OK);
        assert_ne!(pm2, pm);
        // Retour à IPackageManager depuis IPackageManager2.
        assert_eq!(qi_of(pm2, &IID_PACKAGE_MANAGER), (S_OK, pm));
        assert_eq!(qi_of(pm, &[1u8; 16]).0, E_NOINTERFACE);

        // FindPackages (slot 11) et FindPackagesWithPackageTypes (slot 9 d'IPackageManager2).
        let (hr, all) = call_out(pm, 11);
        assert_eq!(hr, S_OK);
        empty(all, &IID_ITERABLE_PACKAGE, &IID_ITERATOR_PACKAGE);
        let typed: unsafe extern "win64" fn(usize, u32, *mut usize) -> i32 =
            std::mem::transmute(slot(pm2, 9));
        let mut col = 0usize;
        assert_eq!(typed(pm2, 1, &mut col), S_OK);
        empty(col, &IID_ITERABLE_PACKAGE, &IID_ITERATOR_PACKAGE);

        // FindUsers (slot 15) et FindPackageByPackageFullName (slot 17).
        let one: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
            std::mem::transmute(slot(pm, 15));
        let mut users = 0usize;
        assert_eq!(one(pm, 0, &mut users), S_OK);
        empty(users, &IID_ITERABLE_USER, &IID_ITERATOR_USER);
        let by_name: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
            std::mem::transmute(slot(pm, 17));
        let mut pkg = 5usize;
        assert_eq!(by_name(pm, 0, &mut pkg), S_OK);
        assert_eq!(pkg, 0);

        // RemovePackageAsync (slot 8) : E_NOTIMPL, opération nulle.
        let remove: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
            std::mem::transmute(slot(pm, 8));
        let mut op = 5usize;
        assert_eq!(remove(pm, 0, &mut op), E_NOTIMPL);
        assert_eq!(op, 0);
    }
}

/// Les IID des collections suivent l'algorithme des pinterface (IIterable
/// {faa585ea-…}, IIterator {6a79e863-…}) avec la signature de la classe
/// (rc(nom;{interface par défaut})).
#[test]
fn iid_des_collections_de_paquets() {
    let pkg = "rc(Windows.ApplicationModel.Package;{163c792f-bd75-413c-bf23-b1fe7b95d825})";
    let user = "rc(Windows.Management.Deployment.PackageUserInformation;{f6383423-fa09-4cbc-9055-15ca275e2e7e})";
    let iterable = |t: &str| {
        pinterface_iid(&format!(
            "pinterface({{faa585ea-6214-4217-afda-7f46de5869b3}};{t})"
        ))
    };
    let iterator = |t: &str| {
        pinterface_iid(&format!(
            "pinterface({{6a79e863-4300-459a-9966-cbb660963ee1}};{t})"
        ))
    };
    assert_eq!(iterable(pkg), IID_ITERABLE_PACKAGE);
    assert_eq!(iterator(pkg), IID_ITERATOR_PACKAGE);
    assert_eq!(iterable(user), IID_ITERABLE_USER);
    assert_eq!(iterator(user), IID_ITERATOR_USER);
}
