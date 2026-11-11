//! wldap32.dll, client LDAP de Windows (#1765). Winlux n'a pas de client
//! LDAP : la DLL existe pour que les programmes qui l'importent sans s'en
//! servir démarrent (libcurl d'OBS, qui ne l'appelle que pour une URL
//! ldap://), et chaque fonction rend la réponse documentée d'un client
//! sans connexion. `ldap_init` et ses variantes rendent NULL avec
//! LDAP_NOT_SUPPORTED dans `LdapGetLastError` ; une opération sur une
//! session rend LDAP_PARAM_ERROR, puisque aucune session n'a été donnée ;
//! les lectures de résultat rendent NULL ou 0, les libérations ne font rien.
//!
//! Sources : documentation publique de Winldap.h (learn.microsoft.com) et
//! winldap.h de mingw-w64 (domaine public) pour les codes ; toutes les
//! fonctions sont `__cdecl` (LDAPAPI). Numéros d'ordre lus dans la
//! bibliothèque d'importation c/um/x64/Wldap32.Lib du paquet NuGet
//! Microsoft.Windows.SDK.CPP.x64 10.0.26100.4654, qui n'importe que par
//! numéro.

use std::cell::Cell;
use std::sync::OnceLock;

const LDAP_SUCCESS: u32 = 0x00;
const LDAP_OPERATIONS_ERROR: u32 = 0x01;
const LDAP_PROTOCOL_ERROR: u32 = 0x02;
const LDAP_NO_SUCH_OBJECT: u32 = 0x20;
const LDAP_INVALID_CREDENTIALS: u32 = 0x31;
const LDAP_UNAVAILABLE: u32 = 0x34;
const LDAP_SERVER_DOWN: u32 = 0x51;
const LDAP_LOCAL_ERROR: u32 = 0x52;
const LDAP_TIMEOUT: u32 = 0x55;
const LDAP_FILTER_ERROR: u32 = 0x57;
const LDAP_PARAM_ERROR: u32 = 0x59;
const LDAP_NO_MEMORY: u32 = 0x5a;
const LDAP_CONNECT_ERROR: u32 = 0x5b;
const LDAP_NOT_SUPPORTED: u32 = 0x5c;

const ERROR_SUCCESS: u32 = 0;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const ERROR_TIMEOUT: u32 = 1460;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_BAD_NET_RESP: u32 = 58;
const ERROR_GEN_FAILURE: u32 = 31;
const ERROR_FILE_NOT_FOUND: u32 = 2;

thread_local! {
    static LAST: Cell<u32> = const { Cell::new(LDAP_SUCCESS) };
}

fn fail(code: u32) -> u32 {
    LAST.with(|l| l.set(code));
    code
}

/// Numéros d'ordre de Wldap32.Lib (SDK 10.0.26100).
pub const ORDINALS: &[(u16, &str)] = &[
    (10, "ldap_abandon"),
    (12, "ldap_get_optionW"),
    (13, "ldap_unbind"),
    (14, "ldap_set_optionW"),
    (16, "LdapGetLastError"),
    (18, "LdapMapErrorToWin32"),
    (22, "ldap_err2string"),
    (25, "ldap_open"),
    (26, "ldap_first_entry"),
    (27, "ldap_next_entry"),
    (30, "ldap_get_dn"),
    (32, "ldap_first_attribute"),
    (33, "ldap_next_attribute"),
    (34, "ldap_get_values"),
    (35, "ldap_get_values_len"),
    (36, "ldap_count_entries"),
    (37, "ldap_count_values"),
    (38, "ldap_value_free"),
    (40, "ldap_result"),
    (41, "ldap_msgfree"),
    (45, "ldap_bind_s"),
    (46, "ldap_unbind_s"),
    (50, "ldap_search_s"),
    (60, "ldap_simple_bind_s"),
    (72, "ldap_bind_sA"),
    (73, "ldap_bind_sW"),
    (77, "ldap_count_values_len"),
    (79, "ldap_value_free_len"),
    (88, "ldap_connect"),
    (96, "ldap_count_valuesA"),
    (97, "ldap_count_valuesW"),
    (117, "ldap_err2stringA"),
    (118, "ldap_err2stringW"),
    (126, "ldap_first_attributeA"),
    (127, "ldap_first_attributeW"),
    (132, "ldap_get_dnA"),
    (133, "ldap_get_dnW"),
    (136, "ldap_get_option"),
    (137, "ldap_get_optionA"),
    (139, "ldap_get_valuesA"),
    (140, "ldap_get_valuesW"),
    (141, "ldap_get_values_lenA"),
    (142, "ldap_get_values_lenW"),
    (143, "ldap_init"),
    (144, "ldap_initA"),
    (145, "ldap_initW"),
    (146, "ldap_memfreeA"),
    (147, "ldap_memfreeW"),
    (166, "ldap_next_attributeA"),
    (167, "ldap_next_attributeW"),
    (169, "ldap_openA"),
    (170, "ldap_openW"),
    (195, "ldap_search_ext_s"),
    (196, "ldap_search_ext_sA"),
    (200, "ldap_memfree"),
    (203, "ldap_search_ext_sW"),
    (207, "ldap_search_sA"),
    (208, "ldap_search_sW"),
    (211, "ldap_set_option"),
    (212, "ldap_set_optionA"),
    (215, "ldap_simple_bind_sA"),
    (216, "ldap_simple_bind_sW"),
    (217, "ldap_sslinit"),
    (218, "ldap_sslinitA"),
    (219, "ldap_sslinitW"),
    (223, "ldap_value_freeA"),
    (224, "ldap_value_freeW"),
    (301, "ber_free"),
];

/// ldap_init / ldap_open (hôte, port) et ldap_sslinit (hôte, port, ssl) :
/// pas de client LDAP, donc pas de session.
unsafe extern "win64" fn ldap_init(_host: usize, _port: u32, _secure: i32) -> usize {
    fail(LDAP_NOT_SUPPORTED);
    0
}

/// ldap_connect, ldap_set_option, ldap_get_option, ldap_bind_s,
/// ldap_simple_bind_s : la session donnée n'a pas pu venir de nous.
unsafe extern "win64" fn ldap_session_op(_ld: usize) -> u32 {
    fail(LDAP_PARAM_ERROR)
}

/// ldap_search_s(ld, base, scope, filter, attrs, attrsonly, res).
unsafe extern "win64" fn ldap_search_s(
    _ld: usize,
    _base: usize,
    _scope: u32,
    _filter: usize,
    _attrs: usize,
    _attrsonly: u32,
    res: *mut usize,
) -> u32 {
    if !res.is_null() {
        crate::wow32::write_ptr(res as usize, 0);
    }
    fail(LDAP_PARAM_ERROR)
}

/// ldap_search_ext_s(ld, base, scope, filter, attrs, attrsonly, server,
/// client, timeout, sizelimit, res).
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn ldap_search_ext_s(
    _ld: usize,
    _base: usize,
    _scope: u32,
    _filter: usize,
    _attrs: usize,
    _attrsonly: u32,
    _server: usize,
    _client: usize,
    _timeout: usize,
    _sizelimit: u32,
    res: *mut usize,
) -> u32 {
    if !res.is_null() {
        crate::wow32::write_ptr(res as usize, 0);
    }
    fail(LDAP_PARAM_ERROR)
}

/// ldap_result(ld, msgid, all, timeout, res) : -1 en cas d'erreur.
unsafe extern "win64" fn ldap_result(
    _ld: usize,
    _msgid: u32,
    _all: u32,
    _timeout: usize,
    res: *mut usize,
) -> u32 {
    if !res.is_null() {
        crate::wow32::write_ptr(res as usize, 0);
    }
    fail(LDAP_PARAM_ERROR);
    u32::MAX
}

/// ldap_abandon(ld, msgid) : -1 en cas d'erreur.
unsafe extern "win64" fn ldap_abandon(_ld: usize, _msgid: u32) -> i32 {
    fail(LDAP_PARAM_ERROR);
    -1
}

/// ldap_unbind(_s) : aucune session à fermer.
unsafe extern "win64" fn ldap_unbind(_ld: usize) -> u32 {
    fail(LDAP_PARAM_ERROR)
}

/// ldap_first_entry, ldap_next_entry, ldap_get_dn, ldap_first_attribute,
/// ldap_next_attribute, ldap_get_values(_len) : aucun résultat.
unsafe extern "win64" fn ldap_no_result(_ld: usize, _entry: usize) -> usize {
    fail(LDAP_PARAM_ERROR);
    0
}

/// ldap_count_entries(ld, res) : -1 en cas d'erreur.
unsafe extern "win64" fn ldap_count_entries(_ld: usize, _res: usize) -> u32 {
    fail(LDAP_PARAM_ERROR);
    u32::MAX
}

/// ldap_count_values(_len)(vals) : un tableau absent compte 0 valeur.
unsafe extern "win64" fn ldap_count_values(_vals: usize) -> u32 {
    0
}

/// ldap_msgfree, ldap_value_free(_len) : rien n'a été alloué.
unsafe extern "win64" fn ldap_free_ok(_p: usize) -> u32 {
    LDAP_SUCCESS
}

/// ldap_memfree, ber_free : rien n'a été alloué.
unsafe extern "win64" fn ldap_free_void(_p: usize, _fbuf: i32) {}

unsafe extern "win64" fn ldap_get_last_error() -> u32 {
    LAST.with(|l| l.get())
}

unsafe extern "win64" fn ldap_map_error_to_win32(code: u32) -> u32 {
    match code {
        LDAP_SUCCESS => ERROR_SUCCESS,
        LDAP_NOT_SUPPORTED => ERROR_NOT_SUPPORTED,
        LDAP_PARAM_ERROR | LDAP_FILTER_ERROR => ERROR_INVALID_PARAMETER,
        LDAP_NO_MEMORY => ERROR_NOT_ENOUGH_MEMORY,
        LDAP_TIMEOUT => ERROR_TIMEOUT,
        LDAP_INVALID_CREDENTIALS => ERROR_ACCESS_DENIED,
        LDAP_SERVER_DOWN | LDAP_CONNECT_ERROR | LDAP_UNAVAILABLE => ERROR_BAD_NET_RESP,
        LDAP_NO_SUCH_OBJECT => ERROR_FILE_NOT_FOUND,
        _ => ERROR_GEN_FAILURE,
    }
}

/// Message d'une erreur LDAP (noms de la RFC 4511 et de Winldap.h).
fn message(code: u32) -> &'static str {
    match code {
        LDAP_SUCCESS => "Success",
        LDAP_OPERATIONS_ERROR => "Operations Error",
        LDAP_PROTOCOL_ERROR => "Protocol Error",
        LDAP_NO_SUCH_OBJECT => "No Such Object",
        LDAP_INVALID_CREDENTIALS => "Invalid Credentials",
        LDAP_UNAVAILABLE => "Unavailable",
        LDAP_SERVER_DOWN => "Server Down",
        LDAP_LOCAL_ERROR => "Local Error",
        LDAP_TIMEOUT => "Timeout",
        LDAP_FILTER_ERROR => "Filter Error",
        LDAP_PARAM_ERROR => "Parameter Error",
        LDAP_NO_MEMORY => "No Memory",
        LDAP_CONNECT_ERROR => "Connect Error",
        LDAP_NOT_SUPPORTED => "Not Supported",
        _ => "Unknown Error",
    }
}

const KNOWN: [u32; 14] = [
    LDAP_SUCCESS,
    LDAP_OPERATIONS_ERROR,
    LDAP_PROTOCOL_ERROR,
    LDAP_NO_SUCH_OBJECT,
    LDAP_INVALID_CREDENTIALS,
    LDAP_UNAVAILABLE,
    LDAP_SERVER_DOWN,
    LDAP_LOCAL_ERROR,
    LDAP_TIMEOUT,
    LDAP_FILTER_ERROR,
    LDAP_PARAM_ERROR,
    LDAP_NO_MEMORY,
    LDAP_CONNECT_ERROR,
    LDAP_NOT_SUPPORTED,
];

/// Chaînes allouées dans le tas (sous 4 Go pour un programme x86), une
/// fois pour toutes : le pointeur rendu reste valable.
fn messages() -> &'static (Vec<(u32, usize)>, Vec<(u32, usize)>) {
    static M: OnceLock<(Vec<(u32, usize)>, Vec<(u32, usize)>)> = OnceLock::new();
    M.get_or_init(|| {
        let mut a = Vec::new();
        let mut w = Vec::new();
        for code in KNOWN.into_iter().chain([u32::MAX]) {
            let s = message(code);
            let narrow: &'static mut [u8] =
                Box::leak(s.bytes().chain([0]).collect::<Vec<u8>>().into_boxed_slice());
            let wide: &'static mut [u16] = Box::leak(
                s.encode_utf16()
                    .chain([0])
                    .collect::<Vec<u16>>()
                    .into_boxed_slice(),
            );
            a.push((code, narrow.as_ptr() as usize));
            w.push((code, wide.as_ptr() as usize));
        }
        (a, w)
    })
}

fn lookup(table: &[(u32, usize)], code: u32) -> usize {
    let code = if KNOWN.contains(&code) {
        code
    } else {
        u32::MAX
    };
    table
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(0, |(_, p)| *p)
}

unsafe extern "win64" fn ldap_err2string_a(code: u32) -> usize {
    lookup(&messages().0, code)
}

unsafe extern "win64" fn ldap_err2string_w(code: u32) -> usize {
    lookup(&messages().1, code)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "ldap_init" => fn ldap_init,
        "ldap_initA" => fn ldap_init,
        "ldap_initW" => fn ldap_init,
        "ldap_open" => fn ldap_init,
        "ldap_openA" => fn ldap_init,
        "ldap_openW" => fn ldap_init,
        "ldap_sslinit" => fn ldap_init,
        "ldap_sslinitA" => fn ldap_init,
        "ldap_sslinitW" => fn ldap_init,
        "ldap_connect" => fn ldap_session_op,
        "ldap_set_option" => fn ldap_session_op,
        "ldap_set_optionA" => fn ldap_session_op,
        "ldap_set_optionW" => fn ldap_session_op,
        "ldap_get_option" => fn ldap_session_op,
        "ldap_get_optionA" => fn ldap_session_op,
        "ldap_get_optionW" => fn ldap_session_op,
        "ldap_bind_s" => fn ldap_session_op,
        "ldap_bind_sA" => fn ldap_session_op,
        "ldap_bind_sW" => fn ldap_session_op,
        "ldap_simple_bind_s" => fn ldap_session_op,
        "ldap_simple_bind_sA" => fn ldap_session_op,
        "ldap_simple_bind_sW" => fn ldap_session_op,
        "ldap_search_s" => fn ldap_search_s,
        "ldap_search_sA" => fn ldap_search_s,
        "ldap_search_sW" => fn ldap_search_s,
        "ldap_search_ext_s" => fn ldap_search_ext_s,
        "ldap_search_ext_sA" => fn ldap_search_ext_s,
        "ldap_search_ext_sW" => fn ldap_search_ext_s,
        "ldap_result" => fn ldap_result,
        "ldap_abandon" => fn ldap_abandon,
        "ldap_unbind" => fn ldap_unbind,
        "ldap_unbind_s" => fn ldap_unbind,
        "ldap_first_entry" => fn ldap_no_result,
        "ldap_next_entry" => fn ldap_no_result,
        "ldap_get_dn" => fn ldap_no_result,
        "ldap_get_dnA" => fn ldap_no_result,
        "ldap_get_dnW" => fn ldap_no_result,
        "ldap_first_attribute" => fn ldap_no_result,
        "ldap_first_attributeA" => fn ldap_no_result,
        "ldap_first_attributeW" => fn ldap_no_result,
        "ldap_next_attribute" => fn ldap_no_result,
        "ldap_next_attributeA" => fn ldap_no_result,
        "ldap_next_attributeW" => fn ldap_no_result,
        "ldap_get_values" => fn ldap_no_result,
        "ldap_get_valuesA" => fn ldap_no_result,
        "ldap_get_valuesW" => fn ldap_no_result,
        "ldap_get_values_len" => fn ldap_no_result,
        "ldap_get_values_lenA" => fn ldap_no_result,
        "ldap_get_values_lenW" => fn ldap_no_result,
        "ldap_count_entries" => fn ldap_count_entries,
        "ldap_count_values" => fn ldap_count_values,
        "ldap_count_valuesA" => fn ldap_count_values,
        "ldap_count_valuesW" => fn ldap_count_values,
        "ldap_count_values_len" => fn ldap_count_values,
        "ldap_msgfree" => fn ldap_free_ok,
        "ldap_value_free" => fn ldap_free_ok,
        "ldap_value_freeA" => fn ldap_free_ok,
        "ldap_value_freeW" => fn ldap_free_ok,
        "ldap_value_free_len" => fn ldap_free_ok,
        "ldap_memfree" => fn ldap_free_void,
        "ldap_memfreeA" => fn ldap_free_void,
        "ldap_memfreeW" => fn ldap_free_void,
        "ber_free" => fn ldap_free_void,
        "LdapGetLastError" => fn ldap_get_last_error,
        "LdapMapErrorToWin32" => fn ldap_map_error_to_win32,
        "ldap_err2string" => fn ldap_err2string_a,
        "ldap_err2stringA" => fn ldap_err2string_a,
        "ldap_err2stringW" => fn ldap_err2string_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ldap_sans_connexion() {
        unsafe {
            let host = b"example.org\0";
            assert_eq!(ldap_init(host.as_ptr() as usize, 389, 0), 0);
            assert_eq!(ldap_get_last_error(), LDAP_NOT_SUPPORTED);
            assert_eq!(
                ldap_map_error_to_win32(LDAP_NOT_SUPPORTED),
                ERROR_NOT_SUPPORTED
            );
            assert_eq!(ldap_session_op(0), LDAP_PARAM_ERROR);
            let mut res = 1usize;
            assert_eq!(ldap_search_s(0, 0, 0, 0, 0, 0, &mut res), LDAP_PARAM_ERROR);
            assert_eq!(res, 0);
            assert_eq!(ldap_count_entries(0, 0), u32::MAX);
            assert_eq!(ldap_no_result(0, 0), 0);
            let s = ldap_err2string_a(LDAP_NOT_SUPPORTED) as *const std::ffi::c_char;
            assert_eq!(
                std::ffi::CStr::from_ptr(s).to_str().unwrap(),
                "Not Supported"
            );
            let u = ldap_err2string_a(0x7777) as *const std::ffi::c_char;
            assert_eq!(
                std::ffi::CStr::from_ptr(u).to_str().unwrap(),
                "Unknown Error"
            );
            let w = ldap_err2string_w(LDAP_SUCCESS) as *const u16;
            assert_eq!(*w, u16::from(b'S'));
        }
        // Chaque numéro d'ordre désigne un export servi.
        let e = exports();
        for (_, name) in ORDINALS {
            assert!(e.contains_key(name), "{name}");
        }
    }
}
