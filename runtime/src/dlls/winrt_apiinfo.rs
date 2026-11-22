//! Classe Windows Runtime `Windows.Foundation.Metadata.ApiInformation`
//! (#2297).
//!
//! Teams la demande juste avant de mourir sur `hresult_class_not_registered`
//! (passe teams-ac, forme C). OBS la demandait déjà, sans effet visible.
//!
//! La documentation publique dit que la classe « enables you to detect
//! whether a specified member, type, or API contract is present ». Chaque
//! méthode rend « True if the specified … is present; otherwise, false ».
//! Elle ne dit pas comment la présence est établie. Winlux répond d'après ce
//! qu'il sert vraiment :
//! - `IsTypePresent` : vrai pour une classe que `RoGetActivationFactory`
//!   connaît (fabrique servie par Winlux, ou classe déclarée par le paquet
//!   ou par le manifeste de l'exe), faux sinon. Une interface, une
//!   énumération ou une structure, qui n'ont pas de fabrique, sont donc
//!   dites absentes ;
//! - `IsMethodPresent`, `IsEventPresent`, `IsPropertyPresent`,
//!   `IsReadOnlyPropertyPresent`, `IsWriteablePropertyPresent` et
//!   `IsEnumNamedValuePresent` : la réponse de `IsTypePresent` pour le type
//!   nommé. Winlux ne tient pas d'inventaire des membres ;
//! - `IsApiContractPresent` : vrai pour `Windows.Foundation.UniversalApiContract`
//!   jusqu'à la version 10.0, celle que la documentation publique donne pour
//!   le build 10.0.19041.0 (page « Device families overview » de Learn), dont
//!   le build annoncé par Winlux (10.0.19045) est une mise à jour ; faux
//!   au-delà et pour tout autre contrat, dont aucune source publique lue ne
//!   donne la version pour ce build (#2702). Les questions par type gardent
//!   leur sonde réelle.
//!
//! Chaque question et sa réponse sont écrites une fois au journal (avertis-
//! sement), pour savoir ce qu'un programme cherche.
//!
//! Sources :
//! - la documentation publique de Microsoft (`ApiInformation` et ses
//!   méthodes) ;
//! - pour l'identifiant d'interface et l'ordre des méthodes, windows-rs
//!   0.58, `Windows/Foundation/Metadata` (licence MIT).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
/// IActivationFactory {00000035-0000-0000-C000-000000000046}.
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
const IID_API_INFORMATION: Guid = g(0x9974_39FE, 0xF681, 0x4A11, 0xB416_C13A_47E8_BA36);

const CLASS: &str = "Windows.Foundation.Metadata.ApiInformation";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn hstr_in(h: usize) -> String {
    String::from_utf16_lossy(super::electron_win32::hstring_slice(h))
}

// ============================================================
//  Présence d'un type
// ============================================================

/// Réponse en cache, par nom de type.
fn cache() -> &'static Mutex<HashMap<String, bool>> {
    static C: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Un nom de type Windows Runtime : des segments séparés par des points,
/// sans espace ni caractère de contrôle.
fn plausible_type_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 512
        && name.split('.').all(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '`')
        })
}

/// Vrai si `RoGetActivationFactory` connaît la classe `name`.
fn type_present(name: &str) -> bool {
    if !plausible_type_name(name) {
        return false;
    }
    if let Some(&v) = lock(cache()).get(name) {
        return v;
    }
    // Classe d'application : déclarée suffit, sans charger sa DLL.
    let present = super::package_classes::declared(name) || unsafe { probe(name) };
    lock(cache()).insert(name.to_string(), present);
    present
}

/// Demande la fabrique sans rien journaliser, puis la rend.
unsafe fn probe(name: &str) -> bool {
    let w: Vec<u16> = name.encode_utf16().collect();
    let mut h = 0usize;
    if super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut h) < 0 {
        return false;
    }
    let mut factory = 0usize;
    let hr = super::electron_win32::quiet_activation_factory(
        h,
        IID_ACTIVATION_FACTORY.as_ptr() as usize,
        &mut factory,
    );
    super::electron_win32::WindowsDeleteString(h);
    if hr >= 0 && factory != 0 {
        let release: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(*(*(factory as *const *const usize)).add(2));
        release(factory);
    }
    // Une fabrique qui refuse IActivationFactory existe quand même.
    hr != REGDB_E_CLASSNOTREG
}

/// Écrit la question et sa réponse, une fois chacune.
fn note(question: &str, answer: bool) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = lock(&SEEN);
    if seen.len() >= 300 || seen.iter().any(|q| q == question) {
        return;
    }
    seen.push(question.to_string());
    crate::log_warn!("com", "ApiInformation : {question} → {answer}");
}

unsafe fn answer(out: *mut u8, question: String, value: bool) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    note(&question, value);
    *out = value as u8;
    S_OK
}

// ============================================================
//  Fabrique statique
// ============================================================

#[repr(C)]
struct Factory {
    v_activation: &'static VTable<7>,
    v_statics: &'static VTable<16>,
}

static FACTORY: Factory = Factory {
    v_activation: &ACTIVATION_VTBL,
    v_statics: &STATICS_VTBL,
};

fn factory_base() -> usize {
    std::ptr::addr_of!(FACTORY) as usize
}

unsafe extern "win64" fn fac_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let base = factory_base();
    *out = if [
        IID_ACTIVATION_FACTORY,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else if iid == IID_API_INFORMATION {
        base + std::mem::size_of::<usize>()
    } else {
        super::comobj::log_no_interface("ApiInformation", &iid as *const Guid);
        *out = 0;
        return E_NOINTERFACE;
    };
    S_OK
}
/// Fabrique statique : jamais libérée.
extern "win64" fn fac_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn fac_release(_this: usize) -> u32 {
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
unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}
/// ActivateInstance : ApiInformation est une classe statique.
unsafe extern "win64" fn fac_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

unsafe extern "win64" fn is_type_present(_this: usize, t: usize, out: *mut u8) -> i32 {
    let t = hstr_in(t);
    let v = type_present(&t);
    answer(out, format!("IsTypePresent(« {t} »)"), v)
}

/// Membre `m` du type `t` : présent si le type l'est.
macro_rules! member {
    ($name:ident, $label:literal) => {
        unsafe extern "win64" fn $name(_this: usize, t: usize, m: usize, out: *mut u8) -> i32 {
            let (t, m) = (hstr_in(t), hstr_in(m));
            let v = type_present(&t);
            answer(out, format!(concat!($label, "(« {} », « {} »)"), t, m), v)
        }
    };
}
member!(is_method_present, "IsMethodPresent");
member!(is_event_present, "IsEventPresent");
member!(is_property_present, "IsPropertyPresent");
member!(is_read_only_property_present, "IsReadOnlyPropertyPresent");
member!(is_writeable_property_present, "IsWriteablePropertyPresent");
member!(is_enum_named_value_present, "IsEnumNamedValuePresent");

unsafe extern "win64" fn is_method_present_arity(
    _this: usize,
    t: usize,
    m: usize,
    arity: u32,
    out: *mut u8,
) -> i32 {
    let (t, m) = (hstr_in(t), hstr_in(m));
    let v = type_present(&t);
    answer(
        out,
        format!("IsMethodPresent(« {t} », « {m} », {arity})"),
        v,
    )
}
unsafe extern "win64" fn is_contract_major(
    _this: usize,
    c: usize,
    major: u16,
    out: *mut u8,
) -> i32 {
    let c = hstr_in(c);
    let v = contract_present(&c, major, 0);
    answer(out, format!("IsApiContractPresent(« {c} », {major})"), v)
}
unsafe extern "win64" fn is_contract_major_minor(
    _this: usize,
    c: usize,
    major: u16,
    minor: u16,
    out: *mut u8,
) -> i32 {
    let c = hstr_in(c);
    let v = contract_present(&c, major, minor);
    answer(
        out,
        format!("IsApiContractPresent(« {c} », {major}, {minor})"),
        v,
    )
}

/// Version de `Windows.Foundation.UniversalApiContract` livrée avec le build
/// 10.0.19041.0 : « was at version 10.0 when it shipped with version
/// 10.0.19041.0 » (Learn, « Device families overview ») (#2702).
const UNIVERSAL_API_CONTRACT: (u16, u16) = (10, 0);

/// Le contrat `name` est-il présent dans la version `major.minor` ? Les
/// versions d'un contrat s'ajoutent les unes aux autres (déduction : la
/// documentation ne dit pas explicitement qu'une version inférieure répond
/// vrai).
fn contract_present(name: &str, major: u16, minor: u16) -> bool {
    name == "Windows.Foundation.UniversalApiContract" && (major, minor) <= UNIVERSAL_API_CONTRACT
}

static ACTIVATION_VTBL: VTable<7> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    fac_activate as *const (),
]);
/// IApiInformationStatics, dans l'ordre de windows-rs.
static STATICS_VTBL: VTable<16> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    is_type_present as *const (),
    is_method_present as *const (),
    is_method_present_arity as *const (),
    is_event_present as *const (),
    is_property_present as *const (),
    is_read_only_property_present as *const (),
    is_writeable_property_present as *const (),
    is_enum_named_value_present as *const (),
    is_contract_major as *const (),
    is_contract_major_minor as *const (),
]);

/// `RoGetActivationFactory` pour ApiInformation, `None` pour une autre
/// classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(fac_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs(s: &str) -> usize {
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut h = 0usize;
        unsafe {
            super::super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut h)
        };
        h
    }

    #[test]
    fn noms_de_type() {
        assert!(plausible_type_name("Windows.Foundation.Uri"));
        assert!(plausible_type_name("Windows.Foundation.Collections.IMap`2"));
        assert!(!plausible_type_name(""));
        assert!(!plausible_type_name("Windows..Uri"));
        assert!(!plausible_type_name("Windows.Foundation Uri"));
    }

    /// #2297 : la fabrique sert IApiInformationStatics ; un type est
    /// présent si Winlux sert sa classe ; UniversalApiContract est présent
    /// (#2702), les autres contrats non.
    #[test]
    fn api_information_2297() {
        unsafe {
            let mut fac = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_API_INFORMATION.as_ptr(), &mut fac),
                Some(S_OK)
            );
            assert_eq!(fac, factory_base() + std::mem::size_of::<usize>());
            assert!(
                activation_factory("Autre.Classe", IID_API_INFORMATION.as_ptr(), &mut fac)
                    .is_none()
            );
            let mut b = 7u8;
            assert_eq!(
                is_type_present(fac, hs("Windows.Foundation.Uri"), &mut b),
                S_OK
            );
            assert_eq!(b, 1);
            assert_eq!(is_type_present(fac, hs(CLASS), &mut b), S_OK);
            assert_eq!(b, 1);
            assert_eq!(
                is_type_present(
                    fac,
                    hs("Windows.UI.Composition.CompositionCapabilities"),
                    &mut b
                ),
                S_OK
            );
            assert_eq!(b, 0);
            assert_eq!(is_type_present(fac, hs("pas un type"), &mut b), S_OK);
            assert_eq!(b, 0);
            assert_eq!(
                is_method_present(fac, hs("Windows.Foundation.Uri"), hs("CombineUri"), &mut b),
                S_OK
            );
            assert_eq!(b, 1);
            assert_eq!(
                is_method_present_arity(fac, hs("Windows.Inexistant.Classe"), hs("M"), 2, &mut b),
                S_OK
            );
            assert_eq!(b, 0);
            b = 7;
            assert_eq!(
                is_contract_major(
                    fac,
                    hs("Windows.Foundation.UniversalApiContract"),
                    1,
                    &mut b
                ),
                S_OK
            );
            assert_eq!(b, 1);
            assert_eq!(
                is_contract_major_minor(
                    fac,
                    hs("Windows.Foundation.FoundationContract"),
                    1,
                    0,
                    &mut b
                ),
                S_OK
            );
            assert_eq!(b, 0);
            assert_eq!(
                is_type_present(fac, hs("Windows.Foundation.Uri"), std::ptr::null_mut()),
                E_POINTER
            );
        }
    }

    /// #2702 : UniversalApiContract jusqu'à 10.0, comme le build 19041 que
    /// Winlux annonce ; Win2D demande la version 2 avant de servir
    /// CanvasComposition.
    #[test]
    fn contrat_universel_2702() {
        let u = "Windows.Foundation.UniversalApiContract";
        assert!(contract_present(u, 1, 0));
        assert!(contract_present(u, 2, 0));
        assert!(contract_present(u, 10, 0));
        assert!(!contract_present(u, 10, 1));
        assert!(!contract_present(u, 11, 0));
        assert!(!contract_present(
            "Windows.Foundation.FoundationContract",
            1,
            0
        ));
        assert!(!contract_present(
            "windows.foundation.universalapicontract",
            2,
            0
        ));
        unsafe {
            let mut fac = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_API_INFORMATION.as_ptr(), &mut fac),
                Some(S_OK)
            );
            let mut b = 7u8;
            assert_eq!(is_contract_major(fac, hs(u), 2, &mut b), S_OK);
            assert_eq!(b, 1);
            assert_eq!(is_contract_major_minor(fac, hs(u), 10, 1, &mut b), S_OK);
            assert_eq!(b, 0);
            assert_eq!(is_contract_major(fac, hs(u), 11, &mut b), S_OK);
            assert_eq!(b, 0);
        }
    }
}
