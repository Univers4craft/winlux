//! Petites DLL système importées directement par de vieux programmes
//! (WinSCP, 7-Zip, #486) : sans elles, le programme ne se charge même pas.
//!
//! - `mpr.dll` : réseau Windows (WNet*). Les lecteurs réseau de Winlux
//!   (`mounts`, type « network ») répondent à WNetGetConnection ; le
//!   voisinage réseau n'est pas parcouru (ERROR_NO_NETWORK).
//! - `shfolder.dll` : SHGetFolderPath, redistribuable d'avant Windows 2000,
//!   qui renvoie vers shell32.
//! - `msi.dll` : fonctions de Windows Installer interrogées par les
//!   applications ; aucun composant « annoncé » n'existe sous Winlux.

const ERROR_SUCCESS: u32 = 0;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_BAD_DEVICE: u32 = 1200;
const ERROR_NO_NETWORK: u32 = 1222;
const ERROR_NOT_CONNECTED: u32 = 2250;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_UNKNOWN_PRODUCT: u32 = 1605;
const ERROR_FUNCTION_FAILED: u32 = 1627;
const INSTALLSTATE_UNKNOWN: i32 = -1;

// ---------- mpr.dll ----------

/// `\\serveur\partage` d'un lecteur réseau monté par Winlux (`X:`).
pub fn remote_name(letter: char) -> Option<String> {
    let m = crate::mounts::list()
        .into_iter()
        .find(|m| m.kind == "network" && m.letter.eq_ignore_ascii_case(&letter))?;
    // smb://utilisateur@hote:445/partage/sous → \\hote\partage\sous
    let rest = m
        .source
        .split_once("://")
        .map_or(m.source.as_str(), |(_, r)| r);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.split(':').next().unwrap_or(authority);
    if host.is_empty() {
        return None;
    }
    let path = path.trim_matches('/').replace('/', "\\");
    Some(if path.is_empty() {
        format!("\\\\{host}")
    } else {
        format!("\\\\{host}\\{path}")
    })
}

fn drive_letter(local: &str) -> Option<char> {
    let mut c = local.chars();
    let l = c.next()?;
    (l.is_ascii_alphabetic() && c.next() == Some(':') && c.next().is_none()).then_some(l)
}

unsafe fn connection(local: &str, remote: &str, buf: *mut u16, len: *mut u32) -> u32 {
    let _ = local;
    if len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let w: Vec<u16> = remote.encode_utf16().collect();
    if buf.is_null() || (*len as usize) <= w.len() {
        *len = w.len() as u32 + 1;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    ERROR_SUCCESS
}

/// `WNetGetConnectionW(L"X:", remote, &len)`.
pub unsafe extern "win64" fn wnet_get_connection_w(
    local: *const u16,
    remote: *mut u16,
    len: *mut u32,
) -> u32 {
    if local.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let name = super::wstr(local);
    let Some(letter) = drive_letter(&name) else {
        return ERROR_BAD_DEVICE;
    };
    match remote_name(letter) {
        Some(r) => connection(&name, &r, remote, len),
        None => ERROR_NOT_CONNECTED,
    }
}

pub unsafe extern "win64" fn wnet_get_connection_a(
    local: *const u8,
    remote: *mut u8,
    len: *mut u32,
) -> u32 {
    if local.is_null() || len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let name = super::cstr(local);
    let Some(letter) = drive_letter(&name) else {
        return ERROR_BAD_DEVICE;
    };
    let Some(r) = remote_name(letter) else {
        return ERROR_NOT_CONNECTED;
    };
    let b = super::encode_ansi(&r);
    if remote.is_null() || (*len as usize) <= b.len() {
        *len = b.len() as u32 + 1;
        return ERROR_MORE_DATA;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), remote, b.len());
    *remote.add(b.len()) = 0;
    ERROR_SUCCESS
}

/// Énumération du voisinage réseau : pas de navigateur réseau.
pub extern "win64" fn wnet_open_enum(
    _scope: u32,
    _kind: u32,
    _usage: u32,
    _resource: usize,
    handle: *mut usize,
) -> u32 {
    if !handle.is_null() {
        unsafe { *handle = 0 };
    }
    ERROR_NO_NETWORK
}
pub extern "win64" fn wnet_enum_resource(
    _handle: usize,
    _count: *mut u32,
    _buf: usize,
    _size: *mut u32,
) -> u32 {
    ERROR_NO_MORE_ITEMS
}
pub extern "win64" fn wnet_close_enum(_handle: usize) -> u32 {
    ERROR_SUCCESS
}
/// WNetGetUniversalName, WNetGetResourceParent, WNetGetResourceInformation :
/// quatre arguments, pas de réseau Windows à interroger.
pub extern "win64" fn wnet_not_connected4(_a: usize, _b: u32, _c: usize, _d: usize) -> u32 {
    ERROR_NOT_CONNECTED
}
pub extern "win64" fn wnet_no_network4(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    ERROR_NO_NETWORK
}
/// WNetAddConnection2 : un lecteur réseau se monte dans Paramètres.
pub extern "win64" fn wnet_add_connection2(
    _resource: usize,
    _password: usize,
    _user: usize,
    _flags: u32,
) -> u32 {
    ERROR_NO_NETWORK
}
pub extern "win64" fn wnet_cancel_connection2(_name: usize, _flags: u32, _force: i32) -> u32 {
    ERROR_NOT_CONNECTED
}
/// WNetGetLastError : aucune erreur propre au fournisseur.
pub extern "win64" fn wnet_get_last_error(
    error: *mut u32,
    _buf: usize,
    _len: u32,
    _name: usize,
    _name_len: u32,
) -> u32 {
    if !error.is_null() {
        unsafe { *error = 0 };
    }
    ERROR_SUCCESS
}

pub fn mpr_exports() -> super::Exports {
    crate::exports! {
        "WNetGetConnectionW" => fn wnet_get_connection_w,
        "WNetGetConnectionA" => fn wnet_get_connection_a,
        "WNetOpenEnumW" => fn wnet_open_enum,
        "WNetOpenEnumA" => fn wnet_open_enum,
        "WNetEnumResourceW" => fn wnet_enum_resource,
        "WNetEnumResourceA" => fn wnet_enum_resource,
        "WNetCloseEnum" => fn wnet_close_enum,
        "WNetGetUniversalNameW" => fn wnet_not_connected4,
        "WNetGetUniversalNameA" => fn wnet_not_connected4,
        "WNetGetResourceParentW" => fn wnet_no_network4,
        "WNetGetResourceParentA" => fn wnet_no_network4,
        "WNetGetResourceInformationW" => fn wnet_no_network4,
        "WNetGetResourceInformationA" => fn wnet_no_network4,
        "WNetAddConnection2W" => fn wnet_add_connection2,
        "WNetAddConnection2A" => fn wnet_add_connection2,
        "WNetCancelConnection2W" => fn wnet_cancel_connection2,
        "WNetCancelConnection2A" => fn wnet_cancel_connection2,
        "WNetGetLastErrorW" => fn wnet_get_last_error,
        "WNetGetLastErrorA" => fn wnet_get_last_error,
    }
}

// ---------- msi.dll ----------

/// MsiGetComponentPath(produit, composant, tampon, taille) : aucun produit
/// installé par Windows Installer n'est « annoncé » sous Winlux.
pub extern "win64" fn msi_get_component_path(_p: usize, _c: usize, _b: usize, _s: usize) -> i32 {
    INSTALLSTATE_UNKNOWN
}
/// MsiGetShortcutTarget(raccourci, produit, fonctionnalité, composant) :
/// un raccourci ordinaire, pas un raccourci annoncé.
pub extern "win64" fn msi_get_shortcut_target(_l: usize, _p: usize, _f: usize, _c: usize) -> u32 {
    ERROR_FUNCTION_FAILED
}
pub extern "win64" fn msi_query_product_state(_p: usize) -> i32 {
    INSTALLSTATE_UNKNOWN
}
pub extern "win64" fn msi_locate_component(_c: usize, _b: usize, _s: usize) -> i32 {
    INSTALLSTATE_UNKNOWN
}
pub extern "win64" fn msi_get_product_info(_p: usize, _a: usize, _b: usize, _s: usize) -> u32 {
    ERROR_UNKNOWN_PRODUCT
}

pub fn msi_exports() -> super::Exports {
    // Fonctions de session des actions personnalisées DLL (#833).
    let mut all = crate::msi::session::exports();
    all.extend(crate::msi::session_db::exports());
    all.extend(crate::exports! {
        "MsiGetComponentPathW" => fn msi_get_component_path,
        "MsiGetComponentPathA" => fn msi_get_component_path,
        "MsiGetShortcutTargetW" => fn msi_get_shortcut_target,
        "MsiGetShortcutTargetA" => fn msi_get_shortcut_target,
        "MsiQueryProductStateW" => fn msi_query_product_state,
        "MsiQueryProductStateA" => fn msi_query_product_state,
        "MsiLocateComponentW" => fn msi_locate_component,
        "MsiLocateComponentA" => fn msi_locate_component,
        "MsiGetProductInfoW" => fn msi_get_product_info,
        "MsiGetProductInfoA" => fn msi_get_product_info,
        // Importées par numéro (WinSCP, bibliothèque d'import de C++Builder) :
        // deux fonctions de quatre arguments dont l'échec est toujours sûr ;
        // 1627 n'est ni ERROR_SUCCESS ni un INSTALLSTATE « présent ».
        "#70" => fn msi_get_shortcut_target,
        "#205" => fn msi_get_shortcut_target,
    });
    all
}

// ---------- shfolder.dll ----------

pub fn shfolder_exports() -> super::Exports {
    crate::exports! {
        "SHGetFolderPathA" => fn super::knownfolders::sh_get_folder_path_a,
        "SHGetFolderPathW" => fn super::knownfolders::sh_get_folder_path_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lettres_de_lecteur() {
        assert_eq!(drive_letter("Z:"), Some('Z'));
        assert_eq!(drive_letter("Z:\\"), None);
        assert_eq!(drive_letter("ZZ"), None);
    }
}
