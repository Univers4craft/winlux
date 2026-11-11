//! activeds et aclui (#1393) : annuaire Active Directory et éditeur de
//! droits d'accès, importés seulement par ordinal par `adsi.pyd` et
//! `authorization.pyd` de pywin32.
//!
//! Un poste Winlux n'appartient à aucun domaine et n'a pas d'éditeur de
//! listes de contrôle d'accès : ces DLL répondent comme un Windows hors
//! domaine, sans prétendre avoir agi.
//! - ADsGetObject et ADsOpenObject rendent ERROR_NO_SUCH_DOMAIN (en
//!   HRESULT), avec un objet nul ;
//! - la mémoire ADSI (AllocADsMem, FreeADsMem…) et la dernière erreur
//!   (ADsSetLastError, ADsGetLastError) fonctionnent ;
//! - EditSecurity n'affiche rien et rend FALSE avec ERROR_NOT_SUPPORTED.
//!
//! Provenance : noms et signatures d'après la documentation publique
//! d'ADSI et d'aclui ; ordinaux d'après les bibliothèques d'importation
//! `ActiveDS.Lib` et `AclUI.Lib` du SDK Windows public (paquet NuGet
//! Microsoft.Windows.SDK.CPP.x64 10.0.26100.4654).

use std::sync::Mutex;

const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_NO_SUCH_DOMAIN: u32 = 1355;
const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
const E_POINTER: i32 = 0x8000_4003_u32 as i32;

fn hresult_from_win32(e: u32) -> i32 {
    (0x8007_0000 | (e & 0xFFFF)) as i32
}

unsafe fn null_out(out: *mut usize) {
    if !out.is_null() {
        crate::wow32::write_ptr(out as usize, 0);
    }
}

// ── activeds ───────────────────────────────────────────────────────────

/// ADsGetObject (#3) : aucun annuaire joignable hors domaine.
pub unsafe extern "win64" fn ads_get_object(path: *const u16, _iid: usize, out: *mut usize) -> i32 {
    null_out(out);
    if path.is_null() || out.is_null() {
        return E_POINTER;
    }
    hresult_from_win32(ERROR_NO_SUCH_DOMAIN)
}

/// ADsOpenObject (#9) : comme ADsGetObject, avec des identifiants.
pub unsafe extern "win64" fn ads_open_object(
    path: *const u16,
    _user: *const u16,
    _password: *const u16,
    _reserved: u32,
    _iid: usize,
    out: *mut usize,
) -> i32 {
    ads_get_object(path, 0, out)
}

/// ADsBuildEnumerator (#4) : aucun conteneur ADSI ne peut exister.
pub unsafe extern "win64" fn ads_build_enumerator(container: usize, out: *mut usize) -> i32 {
    null_out(out);
    if container == 0 || out.is_null() {
        return E_POINTER;
    }
    E_NOTIMPL
}

/// ADsFreeEnumerator (#5).
pub extern "win64" fn ads_free_enumerator(_enumerator: usize) -> i32 {
    E_NOTIMPL
}

/// ADsEnumerateNext (#6).
pub unsafe extern "win64" fn ads_enumerate_next(
    _e: usize,
    _count: u32,
    _vars: usize,
    fetched: *mut u32,
) -> i32 {
    if !fetched.is_null() {
        std::ptr::write_unaligned(fetched, 0);
    }
    E_NOTIMPL
}

/// Dernière erreur étendue d'ADSI : code, texte, fournisseur.
static LAST: Mutex<(u32, Vec<u16>, Vec<u16>)> = Mutex::new((0, Vec::new(), Vec::new()));

unsafe fn wide(p: *const u16) -> Vec<u16> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0;
    while *p.add(i) != 0 && i < 4096 {
        v.push(*p.add(i));
        i += 1;
    }
    v
}

/// ADsSetLastError (#12).
pub unsafe extern "win64" fn ads_set_last_error(err: u32, text: *const u16, provider: *const u16) {
    let mut l = LAST.lock().unwrap_or_else(|e| e.into_inner());
    *l = (err, wide(text), wide(provider));
}

unsafe fn copy_wide(src: &[u16], buf: *mut u16, len: u32) {
    if buf.is_null() || len == 0 {
        return;
    }
    let n = src.len().min(len as usize - 1);
    std::ptr::copy_nonoverlapping(src.as_ptr(), buf, n);
    *buf.add(n) = 0;
}

/// ADsGetLastError (#13).
pub unsafe extern "win64" fn ads_get_last_error(
    err: *mut u32,
    text: *mut u16,
    text_len: u32,
    provider: *mut u16,
    provider_len: u32,
) -> i32 {
    let l = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if !err.is_null() {
        std::ptr::write_unaligned(err, l.0);
    }
    copy_wide(&l.1, text, text_len);
    copy_wide(&l.2, provider, provider_len);
    S_OK
}

/// AllocADsMem (#14) : bloc mis à zéro.
pub extern "win64" fn alloc_ads_mem(size: u32) -> usize {
    // SAFETY : allocation sans pointeur d'entrée.
    unsafe { libc::calloc(1, (size as usize).max(1)) as usize }
}

/// FreeADsMem (#15).
pub extern "win64" fn free_ads_mem(p: usize) -> i32 {
    // SAFETY : bloc rendu par AllocADsMem ou ReallocADsMem (contrat ADSI).
    unsafe { libc::free(p as *mut libc::c_void) };
    1
}

/// ReallocADsMem (#16) : nouvelle taille, contenu gardé jusqu'à
/// `old_size`.
pub extern "win64" fn realloc_ads_mem(p: usize, old_size: u32, new_size: u32) -> usize {
    let q = alloc_ads_mem(new_size);
    if q != 0 && p != 0 {
        // SAFETY : `p` porte au moins `old_size` octets (contrat ADSI).
        unsafe {
            std::ptr::copy_nonoverlapping(
                p as *const u8,
                q as *mut u8,
                old_size.min(new_size) as usize,
            );
            libc::free(p as *mut libc::c_void);
        }
    }
    q
}

/// AllocADsStr (#17) : copie d'une chaîne large.
pub unsafe extern "win64" fn alloc_ads_str(s: *const u16) -> usize {
    if s.is_null() {
        return 0;
    }
    let v = wide(s);
    let q = alloc_ads_mem(((v.len() + 1) * 2) as u32);
    if q != 0 {
        std::ptr::copy_nonoverlapping(v.as_ptr(), q as *mut u16, v.len());
    }
    q
}

/// FreeADsStr (#18).
pub extern "win64" fn free_ads_str(p: usize) -> i32 {
    free_ads_mem(p)
}

// ── aclui ──────────────────────────────────────────────────────────────

/// EditSecurity (#2) : pas d'éditeur de droits d'accès.
pub extern "win64" fn edit_security(_owner: usize, _info: usize) -> i32 {
    crate::process::set_last_error(ERROR_NOT_SUPPORTED);
    0
}

/// CreateSecurityPage (#1) : aucune page de propriétés.
pub extern "win64" fn create_security_page(_info: usize) -> usize {
    crate::process::set_last_error(ERROR_NOT_SUPPORTED);
    0
}

/// EditSecurityAdvanced (#3).
pub extern "win64" fn edit_security_advanced(_owner: usize, _info: usize, _page: u32) -> i32 {
    E_NOTIMPL
}

/// Ajoute à `e` l'export « #N » de chaque fonction nommée.
fn with_ordinals(mut e: crate::dlls::Exports, ordinals: &[(u16, &str)]) -> crate::dlls::Exports {
    for &(n, name) in ordinals {
        if let Some(&addr) = e.get(name) {
            e.insert(Box::leak(format!("#{n}").into_boxed_str()), addr);
        }
    }
    e
}

pub fn activeds_exports() -> crate::dlls::Exports {
    let e = crate::exports! {
        "ADsGetObject" => fn ads_get_object,
        "ADsOpenObject" => fn ads_open_object,
        "ADsBuildEnumerator" => fn ads_build_enumerator,
        "ADsFreeEnumerator" => fn ads_free_enumerator,
        "ADsEnumerateNext" => fn ads_enumerate_next,
        "ADsSetLastError" => fn ads_set_last_error,
        "ADsGetLastError" => fn ads_get_last_error,
        "AllocADsMem" => fn alloc_ads_mem,
        "FreeADsMem" => fn free_ads_mem,
        "ReallocADsMem" => fn realloc_ads_mem,
        "AllocADsStr" => fn alloc_ads_str,
        "FreeADsStr" => fn free_ads_str,
    };
    with_ordinals(
        e,
        &[
            (3, "ADsGetObject"),
            (4, "ADsBuildEnumerator"),
            (5, "ADsFreeEnumerator"),
            (6, "ADsEnumerateNext"),
            (9, "ADsOpenObject"),
            (12, "ADsSetLastError"),
            (13, "ADsGetLastError"),
            (14, "AllocADsMem"),
            (15, "FreeADsMem"),
            (16, "ReallocADsMem"),
            (17, "AllocADsStr"),
            (18, "FreeADsStr"),
        ],
    )
}

pub fn aclui_exports() -> crate::dlls::Exports {
    let e = crate::exports! {
        "CreateSecurityPage" => fn create_security_page,
        "EditSecurity" => fn edit_security,
        "EditSecurityAdvanced" => fn edit_security_advanced,
    };
    with_ordinals(
        e,
        &[
            (1, "CreateSecurityPage"),
            (2, "EditSecurity"),
            (3, "EditSecurityAdvanced"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activeds_et_aclui_hors_domaine() {
        let _teb = crate::process::test_teb();
        // Ordinaux importés par adsi.pyd et authorization.pyd (pywin32 312).
        let a = activeds_exports();
        for o in ["#3", "#4", "#9", "#13", "#15"] {
            assert!(a.contains_key(o), "{o}");
        }
        assert_eq!(a.get("#3"), a.get("ADsGetObject"));
        assert_eq!(a.get("#15"), a.get("FreeADsMem"));
        let c = aclui_exports();
        assert_eq!(c.get("#2"), c.get("EditSecurity"));
        assert!(c.get("#2").is_some());
        assert_eq!(crate::dlls::normalize("ACTIVEDS.dll"), "activeds");
        unsafe {
            let path: Vec<u16> = "LDAP://rootDSE\0".encode_utf16().collect();
            let mut obj = 1usize;
            assert_eq!(
                ads_get_object(path.as_ptr(), 0, &mut obj),
                0x8007_054B_u32 as i32
            );
            assert_eq!(obj, 0);
            let text: Vec<u16> = "erreur\0".encode_utf16().collect();
            ads_set_last_error(5, text.as_ptr(), std::ptr::null());
            let (mut err, mut buf) = (0u32, [0u16; 4]);
            assert_eq!(
                ads_get_last_error(&mut err, buf.as_mut_ptr(), 4, std::ptr::null_mut(), 0),
                S_OK
            );
            assert_eq!((err, &buf[..]), (5, &[0x65, 0x72, 0x72, 0][..]));
            let s = alloc_ads_str(text.as_ptr());
            assert_eq!(*(s as *const u16).add(6), 0);
            assert_eq!(free_ads_str(s), 1);
        }
        assert_eq!(edit_security(0, 0), 0);
        assert_eq!(crate::process::last_error(), ERROR_NOT_SUPPORTED);
    }
}
