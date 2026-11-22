//! Classe Windows Runtime
//! `Windows.Security.Authentication.Web.Core.WebAuthenticationCoreManager`
//! (#2729).
//!
//! Teams la demande à chaque `auth.login` de sa page (passe teams-al) ;
//! refusée (REGDB_E_CLASSNOTREG), la page lève « user missing » et
//! n'atteint jamais son écran de connexion.
//!
//! La documentation publique place la classe dans
//! `Windows.Foundation.UniversalApiContract` dès la version 1.0 (Windows 10,
//! 10.0.10240.0) ; Winlux annonce ce contrat (#2702), il doit donc la servir.
//! Elle « contains core methods for obtaining tokens from web account
//! providers ».
//!
//! Winlux sert le fournisseur de comptes Microsoft (#2511, étape 1 du plan
//! validé par damien le 22 novembre), encore sans réseau, sans compte et
//! sans jeton (objets dans `winrt_wam_objets.rs`) :
//! - `FindAccountProviderAsync("https://login.microsoft.com")`, avec ou sans
//!   autorité `common`, `organizations` ou `consumers` (nom seul ou adresse
//!   `https://login.microsoftonline.com/<nom>`), et sa variante `ForUser` :
//!   ce fournisseur ;
//! - tout autre fournisseur (dont « https://login.windows.local », comptes
//!   de domaine) et `FindSystemAccountProviderAsync` : opération terminée
//!   de résultat nul (aucun fournisseur). Déduction : Learn ne dit pas ce
//!   que rend une recherche vaine, et un résultat nul n'invente rien ;
//! - `FindAllAccountsAsync` sur ce fournisseur : aucun compte (statut
//!   Success, liste vide) ;
//! - `GetTokenSilentlyAsync` d'une requête sur ce fournisseur :
//!   `UserInteractionRequired`, puisqu'aucun compte n'est connecté ;
//! - `RequestTokenAsync` : `ProviderError` (E_NOTIMPL), la fenêtre de
//!   connexion n'étant pas encore servie (étape 2) ;
//! - `FindAccountAsync` : opération terminée, résultat nul (aucun compte) ;
//! - une requête ou un fournisseur qui ne vient pas de Winlux : E_INVALIDARG ;
//! - `IWebAuthenticationCoreManagerStatics3` (seulement
//!   `CreateWebAccountMonitor`) n'est pas servie : sa QI est refusée et
//!   journalisée.
//!
//! Chaque appel est écrit une fois au journal (avertissement), avec
//! l'identifiant du fournisseur et l'autorité demandés, jamais un compte ni
//! un jeton.
//!
//! Sources :
//! - la documentation publique de Microsoft (`WebAuthenticationCoreManager`,
//!   `FindAccountProviderAsync`, `IAsyncOperation<TResult>`, `IAsyncInfo`) ;
//! - pour les identifiants d'interface et l'ordre des méthodes, windows-rs
//!   0.58, `Windows/Security/Authentication/Web/Core`,
//!   `Windows/Security/Credentials` et `Windows/Foundation` (licence MIT) ;
//! - les identifiants des interfaces paramétrées `IAsyncOperation<…>` sont
//!   calculés selon l'algorithme public des interfaces paramétrées
//!   (SHA-1 de la signature `pinterface(…)`), vérifié sur
//!   `IAsyncOperation<bool>` {CDB5EFB3-5788-509D-9BE1-71CCB8A3362A}.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, IID_IUNKNOWN,
    S_OK,
};

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
/// IActivationFactory {00000035-0000-0000-C000-000000000046}.
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
/// IAsyncInfo {00000036-0000-0000-C000-000000000046}.
const IID_IASYNCINFO: Guid = g(0x0000_0036, 0x0000, 0x0000, 0xC000_0000_0000_0046);

const IID_STATICS: Guid = g(0x6ACA_7C92, 0xA581, 0x4479, 0x9C10_752E_FF44_FD34);
const IID_STATICS2: Guid = g(0xF584_184A, 0x8B57, 0x4820, 0xB6A4_70A5_B6FC_F44A);
const IID_STATICS4: Guid = g(0x54E6_33FE, 0x96E0, 0x41E8, 0x9832_1298_897C_2AAF);

/// IAsyncOperation<WebAccountProvider> : signature
/// `pinterface({9fc2b0bb-e446-44e2-aa61-9cab8f636af2};rc(Windows.Security.Credentials.WebAccountProvider;{29dcc8c3-7ab9-4a7c-a336-b942f9dbf7c7}))`.
const IID_OP_PROVIDER: Guid = g(0x88C6_6009, 0x12F7, 0x58E2, 0x8DBE_6EFC_620C_85BA);
/// IAsyncOperation<WebAccount> : signature
/// `pinterface({9fc2b0bb-e446-44e2-aa61-9cab8f636af2};rc(Windows.Security.Credentials.WebAccount;{69473eb2-8031-49be-80bb-96cb46d99aba}))`.
const IID_OP_ACCOUNT: Guid = g(0xACD7_6B54, 0x297F, 0x5A18, 0x9143_20A3_09E2_DFD3);

const CLASS: &str = "Windows.Security.Authentication.Web.Core.WebAuthenticationCoreManager";

/// AsyncStatus.Completed.
const COMPLETED: i32 = 1;
/// E_ILLEGAL_DELEGATE_ASSIGNMENT : Learn (`IAsyncOperation<TResult>.Completed`)
/// dit « You're not allowed to set the Completed property more than once »
/// sans nommer l'erreur ; ce code, dont c'est le nom, est une déduction.
const E_ILLEGAL_DELEGATE_ASSIGNMENT: i32 = 0x8000_0018u32 as i32;

unsafe fn hstr_in(h: usize) -> String {
    let s = String::from_utf16_lossy(super::electron_win32::hstring_slice(h));
    s.chars().take(120).collect()
}

/// Écrit l'appel une fois.
fn note(call: String) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.len() >= 100 || seen.contains(&call) {
        return;
    }
    crate::log_warn!("com", "WebAuthenticationCoreManager : {call} (#2729)");
    seen.push(call);
}

// ============================================================
//  Opération asynchrone déjà terminée, de résultat nul
// ============================================================

#[repr(C)]
struct Op {
    v_op: &'static VTable<9>,
    v_info: &'static VTable<11>,
    refs: AtomicU32,
    /// IID de l'interface IAsyncOperation<T> servie.
    iid: Guid,
    completed: AtomicU32,
    /// Résultat retenu (une référence), ou nul.
    result: usize,
}

const PTR: usize = std::mem::size_of::<usize>();

unsafe fn op_of(this: usize, info: bool) -> *mut Op {
    (if info { this - PTR } else { this }) as *mut Op
}

/// Nouvelle opération : rend son interface IAsyncOperation<T>. Elle
/// reprend la référence de `result`.
unsafe fn new_op(iid: Guid, result: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        super::winrt_wam_objets::release(result);
        return E_POINTER;
    }
    let op = Box::into_raw(Box::new(Op {
        v_op: &OP_VTBL,
        v_info: &INFO_VTBL,
        refs: AtomicU32::new(1),
        iid,
        completed: AtomicU32::new(0),
        result,
    }));
    *out = op as usize;
    S_OK
}

unsafe fn op_qi(op: *mut Op, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT, (*op).iid].contains(&iid) {
        op as usize
    } else if iid == IID_IASYNCINFO {
        op as usize + PTR
    } else {
        super::comobj::log_no_interface("WebAuthenticationCoreManager (opération)", &iid);
        *out = 0;
        return E_NOINTERFACE;
    };
    (*op).refs.fetch_add(1, Ordering::AcqRel);
    S_OK
}

unsafe fn op_release(op: *mut Op) -> u32 {
    let n = (*op).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let op = Box::from_raw(op);
        super::winrt_wam_objets::release(op.result);
    }
    n
}

macro_rules! op_iunknown {
    ($qi:ident, $add:ident, $rel:ident, $info:expr) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
            op_qi(op_of(this, $info), iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            (*op_of(this, $info)).refs.fetch_add(1, Ordering::AcqRel) + 1
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            op_release(op_of(this, $info))
        }
    };
}
op_iunknown!(op_qi_o, op_add_o, op_rel_o, false);
op_iunknown!(op_qi_i, op_add_i, op_rel_i, true);

unsafe extern "win64" fn no_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn op_class_name(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0; // HSTRING nul : chaîne vide.
    S_OK
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}

/// put_Completed : l'opération est déjà terminée, le délégué est appelé
/// aussitôt avec AsyncStatus.Completed. Learn : « If the Completed property
/// is set after the action has already completed, then the action behaves
/// as if it had completed immediately after the handler was received »,
/// le délégué pouvant être appelé avant le retour, sur le même fil. Un
/// second délégué est refusé.
unsafe extern "win64" fn op_put_completed(this: usize, handler: usize) -> i32 {
    let op = op_of(this, false);
    if handler == 0 {
        return E_POINTER;
    }
    if (*op).completed.swap(1, Ordering::AcqRel) != 0 {
        return E_ILLEGAL_DELEGATE_ASSIGNMENT;
    }
    // AsyncOperationCompletedHandler<T>::Invoke(asyncInfo, status) : entrée 3.
    let invoke: unsafe extern "win64" fn(usize, usize, i32) -> i32 =
        std::mem::transmute(*(*(handler as *const *const usize)).add(3));
    invoke(handler, this, COMPLETED);
    S_OK
}
unsafe extern "win64" fn op_get_completed(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
/// GetResults : le résultat retenu (une référence de plus), ou nul.
unsafe extern "win64" fn op_get_results(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let r = (*op_of(this, false)).result;
    super::winrt_wam_objets::add_ref(r);
    *out = r;
    S_OK
}

unsafe extern "win64" fn info_id(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    // Identifiant propre à l'opération.
    *out = (this >> 4) as u32;
    S_OK
}
unsafe extern "win64" fn info_status(_this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = COMPLETED;
    S_OK
}
unsafe extern "win64" fn info_error(_this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = S_OK;
    S_OK
}
unsafe extern "win64" fn info_noop(_this: usize) -> i32 {
    S_OK
}

/// IAsyncOperation<T>, dans l'ordre de windows-rs.
static OP_VTBL: VTable<9> = VTable([
    op_qi_o as *const (),
    op_add_o as *const (),
    op_rel_o as *const (),
    no_iids as *const (),
    op_class_name as *const (),
    trust_level as *const (),
    op_put_completed as *const (),
    op_get_completed as *const (),
    op_get_results as *const (),
]);
/// IAsyncInfo : Id, Status, ErrorCode, Cancel, Close.
static INFO_VTBL: VTable<11> = VTable([
    op_qi_i as *const (),
    op_add_i as *const (),
    op_rel_i as *const (),
    no_iids as *const (),
    op_class_name as *const (),
    trust_level as *const (),
    info_id as *const (),
    info_status as *const (),
    info_error as *const (),
    info_noop as *const (),
    info_noop as *const (),
]);

// ============================================================
//  Fabrique statique
// ============================================================

#[repr(C)]
struct Factory {
    v_activation: &'static VTable<7>,
    v_statics: &'static VTable<13>,
    v_statics2: &'static VTable<7>,
    v_statics4: &'static VTable<11>,
}

static FACTORY: Factory = Factory {
    v_activation: &ACTIVATION_VTBL,
    v_statics: &STATICS_VTBL,
    v_statics2: &STATICS2_VTBL,
    v_statics4: &STATICS4_VTBL,
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
    } else if iid == IID_STATICS {
        base + PTR
    } else if iid == IID_STATICS2 {
        base + 2 * PTR
    } else if iid == IID_STATICS4 {
        base + 3 * PTR
    } else {
        super::comobj::log_no_interface("WebAuthenticationCoreManager", &iid as *const Guid);
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
unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}
/// ActivateInstance : classe statique.
unsafe extern "win64" fn fac_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

/// Fournisseur de comptes Microsoft servi par Winlux.
const MICROSOFT: &str = "https://login.microsoft.com";

/// Autorité acceptée pour le fournisseur Microsoft : vide, ou `common`,
/// `organizations`, `consumers`, en nom seul ou en dernier segment d'une
/// adresse.
fn microsoft_authority(authority: &str) -> bool {
    let last = authority
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    authority.is_empty() || ["common", "organizations", "consumers"].contains(&last)
}

/// Recherche de fournisseur : le fournisseur Microsoft, ou aucun.
unsafe fn find(call: String, id: usize, authority: usize, out: *mut usize) -> i32 {
    let id_text = hstr_in(id);
    let auth_text = if authority == 0 {
        String::new()
    } else {
        hstr_in(authority)
    };
    if id_text == MICROSOFT && microsoft_authority(&auth_text) {
        note(format!("{call} → fournisseur Microsoft (sans compte)"));
        let auth: Vec<u16> = auth_text.encode_utf16().collect();
        let id: Vec<u16> = id_text.encode_utf16().collect();
        return new_op(
            IID_OP_PROVIDER,
            super::winrt_wam_objets::new_provider(&id, &auth),
            out,
        );
    }
    no_provider(call, out)
}

/// Aucun fournisseur : opération terminée, résultat nul.
unsafe fn no_provider(call: String, out: *mut usize) -> i32 {
    note(format!("{call} → aucun fournisseur"));
    new_op(IID_OP_PROVIDER, 0, out)
}

/// Argument qui ne vient pas d'un fournisseur rendu par Winlux.
unsafe fn refused(call: &str, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    note(format!("{call} → E_INVALIDARG (fournisseur inconnu)"));
    E_INVALIDARG
}

/// Demande de jeton : `UserInteractionRequired` en silence (aucun compte),
/// `ProviderError` pour une connexion (pas encore servie).
unsafe fn token(call: &str, request: usize, silent: bool, out: *mut usize) -> i32 {
    let ours = request != 0 && super::winrt_wam_objets::request_provider(request).is_some();
    if !ours {
        return refused(call, out);
    }
    use super::winrt_wam_objets as o;
    let result = if silent {
        note(format!("{call} → UserInteractionRequired (aucun compte)"));
        o::token_result(o::STATUS_USER_INTERACTION_REQUIRED, "")
    } else {
        note(format!(
            "{call} → ProviderError (connexion pas encore servie)"
        ));
        o::token_result(
            o::STATUS_PROVIDER_ERROR,
            "La connexion n'est pas encore servie par Winlux.",
        )
    };
    new_op(o::generic().op_token, result, out)
}

unsafe extern "win64" fn get_token_silently(_t: usize, req: usize, out: *mut usize) -> i32 {
    token("GetTokenSilentlyAsync", req, true, out)
}
unsafe extern "win64" fn get_token_silently_account(
    _t: usize,
    req: usize,
    _acc: usize,
    out: *mut usize,
) -> i32 {
    token("GetTokenSilentlyAsync(compte)", req, true, out)
}
unsafe extern "win64" fn request_token(_t: usize, req: usize, out: *mut usize) -> i32 {
    token("RequestTokenAsync", req, false, out)
}
unsafe extern "win64" fn request_token_account(
    _t: usize,
    req: usize,
    _acc: usize,
    out: *mut usize,
) -> i32 {
    token("RequestTokenAsync(compte)", req, false, out)
}
/// FindAccountAsync : aucun compte.
unsafe extern "win64" fn find_account(_t: usize, _prov: usize, _id: usize, out: *mut usize) -> i32 {
    note("FindAccountAsync → aucun compte".to_string());
    new_op(IID_OP_ACCOUNT, 0, out)
}
unsafe extern "win64" fn find_provider(_t: usize, id: usize, out: *mut usize) -> i32 {
    find(
        format!("FindAccountProviderAsync(« {} »)", hstr_in(id)),
        id,
        0,
        out,
    )
}
unsafe extern "win64" fn find_provider_authority(
    _t: usize,
    id: usize,
    auth: usize,
    out: *mut usize,
) -> i32 {
    find(
        format!(
            "FindAccountProviderAsync(« {} », « {} »)",
            hstr_in(id),
            hstr_in(auth)
        ),
        id,
        auth,
        out,
    )
}
unsafe extern "win64" fn find_provider_user(
    _t: usize,
    id: usize,
    auth: usize,
    _user: usize,
    out: *mut usize,
) -> i32 {
    find(
        format!(
            "FindAccountProviderAsync(« {} », « {} », utilisateur)",
            hstr_in(id),
            hstr_in(auth)
        ),
        id,
        auth,
        out,
    )
}
/// FindAllAccountsAsync : aucun compte connu du fournisseur Microsoft.
unsafe fn all_accounts(call: &str, provider: usize, out: *mut usize) -> i32 {
    use super::winrt_wam_objets as o;
    if provider == 0 || o::provider_info(provider).is_none() {
        return refused(call, out);
    }
    note(format!("{call} → aucun compte"));
    new_op(o::generic().op_find_all, o::no_accounts(), out)
}
unsafe extern "win64" fn find_all_accounts(_t: usize, prov: usize, out: *mut usize) -> i32 {
    all_accounts("FindAllAccountsAsync", prov, out)
}
unsafe extern "win64" fn find_all_accounts_client(
    _t: usize,
    prov: usize,
    _client: usize,
    out: *mut usize,
) -> i32 {
    all_accounts("FindAllAccountsAsync(clientId)", prov, out)
}
unsafe extern "win64" fn find_system_provider(_t: usize, id: usize, out: *mut usize) -> i32 {
    no_provider(
        format!("FindSystemAccountProviderAsync(« {} »)", hstr_in(id)),
        out,
    )
}
unsafe extern "win64" fn find_system_provider_authority(
    _t: usize,
    id: usize,
    auth: usize,
    out: *mut usize,
) -> i32 {
    no_provider(
        format!(
            "FindSystemAccountProviderAsync(« {} », « {} »)",
            hstr_in(id),
            hstr_in(auth)
        ),
        out,
    )
}
unsafe extern "win64" fn find_system_provider_user(
    _t: usize,
    id: usize,
    auth: usize,
    _user: usize,
    out: *mut usize,
) -> i32 {
    no_provider(
        format!(
            "FindSystemAccountProviderAsync(« {} », « {} », utilisateur)",
            hstr_in(id),
            hstr_in(auth)
        ),
        out,
    )
}

macro_rules! inspectable {
    () => {
        [
            fac_qi as *const (),
            fac_add_ref as *const (),
            fac_release as *const (),
            no_iids as *const (),
            class_name as *const (),
            trust_level as *const (),
        ]
    };
}

const fn with<const N: usize>(head: [*const (); 6], tail: &[*const ()]) -> [*const (); N] {
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

static ACTIVATION_VTBL: VTable<7> = VTable(with::<7>(inspectable!(), &[fac_activate as *const ()]));
/// IWebAuthenticationCoreManagerStatics, dans l'ordre de windows-rs.
static STATICS_VTBL: VTable<13> = VTable(with::<13>(
    inspectable!(),
    &[
        get_token_silently as *const (),
        get_token_silently_account as *const (),
        request_token as *const (),
        request_token_account as *const (),
        find_account as *const (),
        find_provider as *const (),
        find_provider_authority as *const (),
    ],
));
/// IWebAuthenticationCoreManagerStatics2.
static STATICS2_VTBL: VTable<7> = VTable(with::<7>(
    inspectable!(),
    &[find_provider_user as *const ()],
));
/// IWebAuthenticationCoreManagerStatics4.
static STATICS4_VTBL: VTable<11> = VTable(with::<11>(
    inspectable!(),
    &[
        find_all_accounts as *const (),
        find_all_accounts_client as *const (),
        find_system_provider as *const (),
        find_system_provider_authority as *const (),
        find_system_provider_user as *const (),
    ],
));

/// `RoGetActivationFactory` pour WebAuthenticationCoreManager, `None` pour
/// une autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return super::winrt_wam_objets::activation_factory(class, iid, out);
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

    unsafe fn slot(obj: usize, n: usize) -> usize {
        *(*(obj as *const *const usize)).add(n)
    }

    static CALLS: AtomicU32 = AtomicU32::new(0);
    static LAST_STATUS: AtomicU32 = AtomicU32::new(99);

    unsafe extern "win64" fn invoke(_this: usize, _info: usize, status: i32) -> i32 {
        CALLS.fetch_add(1, Ordering::SeqCst);
        LAST_STATUS.store(status as u32, Ordering::SeqCst);
        S_OK
    }
    static HANDLER_VTBL: VTable<4> = VTable([
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
        invoke as *const (),
    ]);

    /// #2729 : la fabrique sert Statics, Statics2 et Statics4 par leurs
    /// entrées de vtable ; une recherche de fournisseur rend une opération
    /// terminée de résultat nul, dont le délégué est appelé une fois ; les
    /// demandes de jeton sont refusées sans rien rendre.
    #[test]
    fn gestionnaire_comptes_web_2729() {
        unsafe {
            let mut st = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS.as_ptr(), &mut st),
                Some(S_OK)
            );
            assert_eq!(st, factory_base() + PTR);
            let mut st4 = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS4.as_ptr(), &mut st4),
                Some(S_OK)
            );
            let mut st2 = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS2.as_ptr(), &mut st2),
                Some(S_OK)
            );
            let statics3 = g(0x2404_EEB2, 0x8924, 0x4D93, 0xAB3A_9968_8B41_9D56);
            let mut no = 1usize;
            assert_eq!(
                activation_factory(CLASS, statics3.as_ptr(), &mut no),
                Some(E_NOINTERFACE)
            );
            assert_eq!(no, 0);
            assert!(activation_factory("Autre.Classe", IID_STATICS.as_ptr(), &mut no).is_none());

            // FindAccountProviderWithAuthorityAsync : entrée 12 de Statics.
            type Find2 = unsafe extern "win64" fn(usize, usize, usize, *mut usize) -> i32;
            let f: Find2 = std::mem::transmute(slot(st, 12));
            let mut op = 0usize;
            assert_eq!(
                f(
                    st,
                    hs("https://login.microsoft.com"),
                    hs("organizations"),
                    &mut op
                ),
                S_OK
            );
            assert_ne!(op, 0);
            // QI : IAsyncOperation<WebAccountProvider> et IAsyncInfo.
            type Qi = unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32;
            let qi: Qi = std::mem::transmute(slot(op, 0));
            let mut same = 0usize;
            assert_eq!(qi(op, IID_OP_PROVIDER.as_ptr(), &mut same), S_OK);
            assert_eq!(same, op);
            let mut info = 0usize;
            assert_eq!(qi(op, IID_IASYNCINFO.as_ptr(), &mut info), S_OK);
            assert_eq!(info, op + PTR);
            let mut other = 7usize;
            assert_eq!(qi(op, IID_OP_ACCOUNT.as_ptr(), &mut other), E_NOINTERFACE);
            assert_eq!(other, 0);
            type Out32 = unsafe extern "win64" fn(usize, *mut i32) -> i32;
            let status: Out32 = std::mem::transmute(slot(info, 7));
            let mut s = 0i32;
            assert_eq!(status(info, &mut s), S_OK);
            assert_eq!(s, COMPLETED);
            // put_Completed (entrée 6) appelle le délégué aussitôt, une fois.
            let handler_obj: &'static VTable<4> = &HANDLER_VTBL;
            let handler = &handler_obj as *const &VTable<4> as usize;
            type Put = unsafe extern "win64" fn(usize, usize) -> i32;
            let put: Put = std::mem::transmute(slot(op, 6));
            assert_eq!(put(op, handler), S_OK);
            assert_eq!(CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(LAST_STATUS.load(Ordering::SeqCst), COMPLETED as u32);
            assert_eq!(put(op, handler), E_ILLEGAL_DELEGATE_ASSIGNMENT);
            assert_eq!(CALLS.load(Ordering::SeqCst), 1);
            // GetResults (entrée 8) : le fournisseur Microsoft, sans compte.
            type Get = unsafe extern "win64" fn(usize, *mut usize) -> i32;
            let get: Get = std::mem::transmute(slot(op, 8));
            let mut provider = 0usize;
            assert_eq!(get(op, &mut provider), S_OK);
            assert_eq!(
                super::super::winrt_wam_objets::provider_info(provider),
                Some(("https://login.microsoft.com".into(), "organizations".into()))
            );
            type Rel = unsafe extern "win64" fn(usize) -> u32;
            let rel_info: Rel = std::mem::transmute(slot(info, 2));
            assert_eq!(rel_info(info), 2);
            let rel: Rel = std::mem::transmute(slot(op, 2));
            assert_eq!(rel(same), 1);
            assert_eq!(rel(op), 0);

            // FindSystemAccountProviderAsync : entrée 8 de Statics4.
            type Find1 = unsafe extern "win64" fn(usize, usize, *mut usize) -> i32;
            let f: Find1 = std::mem::transmute(slot(st4, 8));
            let mut op = 0usize;
            assert_eq!(f(st4, hs("https://login.microsoft.com"), &mut op), S_OK);
            assert_ne!(op, 0);
            let rel: Rel = std::mem::transmute(slot(op, 2));
            assert_eq!(rel(op), 0);

            // Autre fournisseur : opération terminée, aucun résultat.
            let f2: Find2 = std::mem::transmute(slot(st, 12));
            let mut op = 0usize;
            assert_eq!(
                f2(
                    st,
                    hs("https://login.windows.local"),
                    hs("organizations"),
                    &mut op
                ),
                S_OK
            );
            let get: Get = std::mem::transmute(slot(op, 8));
            let mut r = 7usize;
            assert_eq!(get(op, &mut r), S_OK);
            assert_eq!(r, 0);
            let rel: Rel = std::mem::transmute(slot(op, 2));
            assert_eq!(rel(op), 0);

            // Requête créée par la fabrique sur le fournisseur Microsoft.
            use super::super::winrt_wam_objets as o;
            let mut fac = 0usize;
            let iid_factory = g(0x6CF2_141C, 0x0FF0, 0x4C67, 0xB84F_99DD_BE4A_72C9);
            assert_eq!(
                activation_factory(o::CLASS_REQUEST, iid_factory.as_ptr(), &mut fac),
                Some(S_OK)
            );
            type Create = unsafe extern "win64" fn(usize, usize, usize, usize, *mut usize) -> i32;
            let create: Create = std::mem::transmute(slot(fac, 6));
            let mut request = 0usize;
            assert_eq!(
                create(fac, provider, hs("openid"), hs("client"), &mut request),
                S_OK
            );
            let status_of = |op: usize| -> i32 {
                let get: Get = std::mem::transmute(slot(op, 8));
                let mut res = 0usize;
                assert_eq!(get(op, &mut res), S_OK);
                assert_ne!(res, 0);
                let st: Out32 = std::mem::transmute(slot(res, 7));
                let mut s = -1i32;
                assert_eq!(st(res, &mut s), S_OK);
                o::release(res);
                let rel: Rel = std::mem::transmute(slot(op, 2));
                assert_eq!(rel(op), 0);
                s
            };

            // GetTokenSilentlyAsync (entrée 6 de Statics) : interaction requise.
            let f: Find1 = std::mem::transmute(slot(st, 6));
            let mut op = 0usize;
            assert_eq!(f(st, request, &mut op), S_OK);
            assert_eq!(status_of(op), o::STATUS_USER_INTERACTION_REQUIRED);
            // RequestTokenAsync (entrée 8 de Statics) : erreur du fournisseur.
            let f: Find1 = std::mem::transmute(slot(st, 8));
            let mut op = 0usize;
            assert_eq!(f(st, request, &mut op), S_OK);
            assert_eq!(status_of(op), o::STATUS_PROVIDER_ERROR);
            // Une requête étrangère est refusée, rien rendu.
            let mut op = 5usize;
            assert_eq!(f(st, handler, &mut op), E_INVALIDARG);
            assert_eq!(op, 0);

            // FindAllAccountsAsync (entrée 6 de Statics4) : aucun compte.
            let f: Find1 = std::mem::transmute(slot(st4, 6));
            let mut op = 0usize;
            assert_eq!(f(st4, provider, &mut op), S_OK);
            assert_eq!(status_of(op), 0); // FindAllWebAccountsStatus.Success
            let mut op = 5usize;
            assert_eq!(f(st4, handler, &mut op), E_INVALIDARG);
            assert_eq!(op, 0);
            o::release(request);
            o::release(provider);
        }
    }
}
