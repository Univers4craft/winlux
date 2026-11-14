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
const ERROR_UNKNOWN_COMPONENT: u32 = 1607;
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

/// MsiSourceList* : Winlux ne répare ni ne réinstalle un produit depuis sa
/// source d'origine, la liste des sources n'a donc rien à retenir.
/// L'ajout, l'effacement et le réglage réussissent (Acrobat inscrit son
/// cache d'installation par MsiSourceListAddSourceW, #1757) ; la liste lue
/// est vide.
pub extern "win64" fn msi_source_list_ok() -> u32 {
    crate::trace!("MsiSourceList : liste des sources non conservée");
    0
}
pub extern "win64" fn msi_source_list_empty() -> u32 {
    ERROR_NO_MORE_ITEMS
}
pub extern "win64" fn msi_source_list_get_info() -> u32 {
    ERROR_UNKNOWN_PROPERTY
}
const ERROR_UNKNOWN_PROPERTY: u32 = 1608;

pub fn msi_exports() -> super::Exports {
    // Fonctions de session des actions personnalisées DLL (#833).
    let mut all = crate::msi::session::exports();
    all.extend(crate::msi::session_db::exports());
    all.extend(crate::msi::summary::exports());
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
        "MsiEnumComponentQualifiersW" => fn msi_enum_component_qualifiers,
        "MsiEnumComponentQualifiersA" => fn msi_enum_component_qualifiers,
        "MsiEnumRelatedProductsW" => fn msi_enum_related_products,
        "MsiEnumRelatedProductsA" => fn msi_enum_related_products,
        "MsiSourceListAddSourceW" => fn msi_source_list_ok,
        "MsiSourceListAddSourceA" => fn msi_source_list_ok,
        "MsiSourceListAddSourceExW" => fn msi_source_list_ok,
        "MsiSourceListAddSourceExA" => fn msi_source_list_ok,
        "MsiSourceListClearAllW" => fn msi_source_list_ok,
        "MsiSourceListClearAllA" => fn msi_source_list_ok,
        "MsiSourceListClearAllExW" => fn msi_source_list_ok,
        "MsiSourceListClearAllExA" => fn msi_source_list_ok,
        "MsiSourceListClearSourceW" => fn msi_source_list_ok,
        "MsiSourceListClearSourceA" => fn msi_source_list_ok,
        "MsiSourceListForceResolutionW" => fn msi_source_list_ok,
        "MsiSourceListForceResolutionA" => fn msi_source_list_ok,
        "MsiSourceListForceResolutionExW" => fn msi_source_list_ok,
        "MsiSourceListForceResolutionExA" => fn msi_source_list_ok,
        "MsiSourceListSetInfoW" => fn msi_source_list_ok,
        "MsiSourceListSetInfoA" => fn msi_source_list_ok,
        "MsiSourceListAddMediaDiskW" => fn msi_source_list_ok,
        "MsiSourceListAddMediaDiskA" => fn msi_source_list_ok,
        "MsiSourceListClearMediaDiskW" => fn msi_source_list_ok,
        "MsiSourceListClearMediaDiskA" => fn msi_source_list_ok,
        "MsiSourceListEnumSourcesW" => fn msi_source_list_empty,
        "MsiSourceListEnumSourcesA" => fn msi_source_list_empty,
        "MsiSourceListEnumMediaDisksW" => fn msi_source_list_empty,
        "MsiSourceListEnumMediaDisksA" => fn msi_source_list_empty,
        "MsiSourceListGetInfoW" => fn msi_source_list_get_info,
        "MsiSourceListGetInfoA" => fn msi_source_list_get_info,
    });
    with_msi_ordinals(all)
}

/// Ordinaux de msi.dll (`msi_ordinals.tsv`, tirés de `Msi.Lib` du SDK
/// Windows public, identiques en x86 et x64) : WixCA.dll et les actions
/// personnalisées compilées avec la bibliothèque d'import du SDK importent
/// msi.dll par numéro (#1607), WinSCP aussi (C++Builder). Chaque fonction
/// servie par nom l'est aussi sous « #n ».
fn with_msi_ordinals(mut all: super::Exports) -> super::Exports {
    for line in include_str!("msi_ordinals.tsv").lines() {
        let Some((n, name)) = line.split_once('\t') else {
            continue;
        };
        if let (Ok(n), Some(&addr)) = (n.parse::<u16>(), all.get(name)) {
            all.insert(Box::leak(format!("#{n}").into_boxed_str()), addr);
        }
    }
    all
}

/// MsiEnumComponentQualifiersA/W(szComponent, iIndex, lpQualifierBuf,
/// pcchQualifierBuf, lpApplicationDataBuf, pcchApplicationDataBuf) : aucun
/// composant n'est inscrit dans Winlux, donc aucun qualificatif à énumérer.
/// La documentation rend ERROR_UNKNOWN_COMPONENT pour un composant inconnu,
/// ce que voit l'outil de déploiement d'Office sur une machine sans Office
/// (ordinal 39, #2200).
pub extern "win64" fn msi_enum_component_qualifiers(
    component: usize,
    _index: u32,
    _qualifier: usize,
    pcch_qualifier: usize,
    _app_data: usize,
    _pcch_app_data: usize,
) -> u32 {
    if component == 0 || pcch_qualifier == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    ERROR_UNKNOWN_COMPONENT
}

/// MsiEnumRelatedProducts : aucun produit apparenté inscrit.
pub extern "win64" fn msi_enum_related_products(_u: usize, _r: u32, _i: u32, _b: usize) -> u32 {
    ERROR_NO_MORE_ITEMS
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
    fn qualificatifs_de_composant_par_ordinal() {
        // Outil de déploiement d'Office (#2200) : msi!#39 en chargement
        // différé ; 38 et 39 d'après Msi.Lib x86 et x64 du SDK public.
        let e = msi_exports();
        assert_eq!(e.get("#39"), e.get("MsiEnumComponentQualifiersW"));
        assert_eq!(e.get("#38"), e.get("MsiEnumComponentQualifiersA"));
        assert!(e.get("#39").is_some());
        let component: Vec<u16> = "{00000000-0000-0000-0000-000000000000}\0"
            .encode_utf16()
            .collect();
        let mut buf = [0u16; 64];
        let mut cch = buf.len() as u32;
        let r = msi_enum_component_qualifiers(
            component.as_ptr() as usize,
            0,
            buf.as_mut_ptr() as usize,
            &mut cch as *mut u32 as usize,
            0,
            0,
        );
        assert_eq!(r, ERROR_UNKNOWN_COMPONENT);
        assert_eq!(
            msi_enum_component_qualifiers(0, 0, 0, 0, 0, 0),
            ERROR_INVALID_PARAMETER
        );
    }

    #[test]
    fn msi_par_ordinal_comme_wixca() {
        let e = msi_exports();
        // Ordinaux importés par WixCA.dll (iCUE, #1607), d'après Msi.Lib.
        for (n, name) in [
            (8, "MsiCloseHandle"),
            (17, "MsiCreateRecord"),
            (26, "MsiDatabaseGetPrimaryKeysW"),
            (32, "MsiDatabaseOpenViewW"),
            (34, "MsiDoActionW"),
            (47, "MsiEvaluateConditionW"),
            (49, "MsiGetActiveDatabase"),
            (51, "MsiGetComponentStateW"),
            (64, "MsiGetMode"),
            (73, "MsiGetPropertyA"),
            (74, "MsiGetPropertyW"),
            (80, "MsiGetTargetPathW"),
            (103, "MsiProcessMessage"),
            (116, "MsiRecordGetInteger"),
            (118, "MsiRecordGetStringW"),
            (119, "MsiRecordIsNull"),
            (120, "MsiRecordReadStream"),
            (121, "MsiRecordSetInteger"),
            (124, "MsiRecordSetStringA"),
            (125, "MsiRecordSetStringW"),
            (143, "MsiSetMode"),
            (145, "MsiSetPropertyW"),
            (159, "MsiViewExecute"),
            (160, "MsiViewFetch"),
            (162, "MsiViewGetErrorW"),
            (163, "MsiViewModify"),
            (166, "MsiViewGetColumnInfo"),
            (171, "MsiFormatRecordW"),
            (70, "MsiGetProductInfoW"),
            // Acrobat, CacheSetupFiles_SetSourceList (#1757).
            (211, "MsiSourceListAddSourceW"),
            (256, "MsiSourceListAddSourceExW"),
            (268, "MsiSourceListSetInfoW"),
            (205, "MsiEnumRelatedProductsW"),
            // Base hors session et résumé (panda3d, electron-winstaller, #1657).
            (20, "MsiDatabaseCommit"),
            (77, "MsiGetSummaryInformationA"),
            (92, "MsiOpenDatabaseW"),
            (148, "MsiSummaryInfoGetPropertyA"),
            (149, "MsiSummaryInfoGetPropertyCount"),
            (151, "MsiSummaryInfoPersist"),
            (152, "MsiSummaryInfoSetPropertyA"),
            (153, "MsiSummaryInfoSetPropertyW"),
            (219, "MsiGetFileHashW"),
        ] {
            let by_name = e.get(name).copied();
            assert!(by_name.is_some(), "{name}");
            assert_eq!(
                e.get(format!("#{n}").as_str()).copied(),
                by_name,
                "#{n} {name}"
            );
        }
    }

    #[test]
    fn lettres_de_lecteur() {
        assert_eq!(drive_letter("Z:"), Some('Z'));
        assert_eq!(drive_letter("Z:\\"), None);
        assert_eq!(drive_letter("ZZ"), None);
    }
}
