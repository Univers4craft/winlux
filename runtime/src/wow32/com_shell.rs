//! Éléments du shell (IShellItem, IShellItemArray) pour les programmes x86
//! (#2428, lot A).
//!
//! Description des interfaces de `shobjidl_core.h` (en-têtes publics du SDK
//! Windows ; mêmes déclarations dans mingw-w64, domaine public) pour les
//! mandataires COM de `com.rs`. Sans elles, SHCreateItemFromParsingName et
//! ses voisines rendaient au code x86 l'adresse d'un objet à table win64 :
//! le premier appel de méthode faisait mourir le programme sans un mot
//! (famille de #2409). Les objets eux-mêmes sont ceux de
//! `dlls/filedialog_com.rs`.
//!
//! Conventions : un IBindCtx passé est décrit `In(&IUNKNOWN)`, comme dans
//! `IShellFolder` (com.rs) : le runtime ne s'en sert pas ; un REFGUID (bhid)
//! ou un REFPROPERTYKEY est un pointeur vers des données sans pointeur (`I`) ;
//! SFGAOF*, DWORD*, int* aussi ; une chaîne rendue (LPWSTR*) est `OutPtr`.

use super::com::{Iface, A::*, IUNKNOWN};
use crate::dlls::comobj::{guid, Guid};

const fn iid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    guid(d1, d2, d3, d4)
}

/// IShellItem {43826D1E-E718-42EE-BC55-A1E261C37BFE}.
pub(super) static ISHELLITEM: Iface = Iface {
    name: "IShellItem",
    iid: iid(
        0x4382_6D1E,
        0xE718,
        0x42EE,
        [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IUNKNOWN), I, Iid, OutQ]), // BindToHandler(pbc, bhid, riid, ppv)
        Some(&[Out(&ISHELLITEM)]),            // GetParent
        Some(&[I, OutPtr]),                   // GetDisplayName(sigdn, LPWSTR*)
        Some(&[I, I]),                        // GetAttributes(mask, SFGAOF*)
        Some(&[In(&ISHELLITEM), I, I]),       // Compare(psi, hint, int*)
    ],
};

/// IShellItemArray {B63EA76D-1F85-456F-A19C-48159EFA858B}.
pub(super) static ISHELLITEMARRAY: Iface = Iface {
    name: "IShellItemArray",
    iid: iid(
        0xB63E_A76D,
        0x1F85,
        0x456F,
        [0xA1, 0x9C, 0x48, 0x15, 0x9E, 0xFA, 0x85, 0x8B],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IUNKNOWN), I, Iid, OutQ]), // BindToHandler(pbc, bhid, riid, ppv)
        Some(&[I, Iid, OutQ]),                // GetPropertyStore(flags, riid, ppv)
        Some(&[I, Iid, OutQ]),                // GetPropertyDescriptionList(keyType, riid, ppv)
        Some(&[I, I, I]),                     // GetAttributes(flags, mask, SFGAOF*)
        Some(&[I]),                           // GetCount(DWORD*)
        Some(&[I, Out(&ISHELLITEM)]),         // GetItemAt(index, IShellItem**)
        // EnumItems(IEnumShellItems**) : IEnumShellItems::Next rend un
        // tableau d'interfaces, qu'aucun genre de `A` ne décrit encore ; le
        // runtime répond de toute façon E_NOTIMPL (filedialog_com::array_enum),
        // et le mandataire aussi pour une méthode non décrite.
        None,
    ],
};

/// Toutes les interfaces du shell décrites ici, pour `com::by_iid`.
/// IQueryAssociations {C46CA590-3C3F-11D2-BEE6-0000F805CA57} (shlwapi.h ;
/// #2428, lot E). Objet de `dlls/shell32_releve.rs` (ASSOC_VT 8 = 3 + 5).
pub(super) static IQUERYASSOCIATIONS: Iface = Iface {
    name: "IQueryAssociations",
    iid: iid(
        0xC46C_A590,
        0x3C3F,
        0x11D2,
        [0xBE, 0xE6, 0x00, 0x00, 0xF8, 0x05, 0xCA, 0x57],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, I, I, I]),         // Init(flags, pszAssoc, hkProgid, hwnd)
        Some(&[I, I, I, I, I]),      // GetString(flags, str, extra, pszOut, pcchOut)
        Some(&[I, I, I, OutPtr]),    // GetKey(flags, key, extra, HKEY*)
        Some(&[I, I, I, I, I]),      // GetData(flags, data, extra, pvOut, pcbOut)
        Some(&[I, I, I, Iid, OutQ]), // GetEnum(flags, assocenum, extra, riid, ppv)
    ],
};

/// IServiceProvider {6D5140C1-7436-11CE-8034-00AA006009FA} (servprov.h ;
/// #2428, lot E). IUnknown_QueryService le demande à un objet x86.
pub(super) static ISERVICEPROVIDER: Iface = Iface {
    name: "IServiceProvider",
    iid: iid(
        0x6D51_40C1,
        0x7436,
        0x11CE,
        [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, Iid, OutQ]), // QueryService(guidService, riid, ppv)
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &ISHELLITEM,
    &ISHELLITEMARRAY,
    &IQUERYASSOCIATIONS,
    &ISERVICEPROVIDER,
];

#[cfg(test)]
mod tests {
    use super::super::com::{api_kinds, slots};
    use super::*;

    #[test]
    fn elements_du_shell_decrits_pour_le_code_x86() {
        // Nombre de méthodes propres : celui de shobjidl_core.h, et celui des
        // tables de dlls/filedialog_com.rs (ITEM_VT 8 = 3 + 5, ARRAY_VT 10 = 3 + 7).
        assert_eq!(ISHELLITEM.methods.len(), 5);
        assert_eq!(ISHELLITEMARRAY.methods.len(), 7);
        assert!(ISHELLITEM.methods.iter().all(Option::is_some));
        assert_eq!(
            ISHELLITEMARRAY
                .methods
                .iter()
                .filter(|m| m.is_none())
                .count(),
            1
        );
        let mut seen = std::collections::HashSet::new();
        assert!(ALL.iter().all(|f| seen.insert(f.iid)), "IID en double");
    }

    #[test]
    fn fonctions_du_lot_a_decrites_a_la_taille_de_stdcall() {
        // (fonction, octets d'arguments x86 de stdcall.tsv)
        for (f, bytes) in [
            ("SHCreateItemFromParsingName", 16),
            ("SHCreateItemFromIDList", 12),
            ("SHCreateItemWithParent", 20),
            ("SHCreateItemFromRelativeName", 20),
            ("SHCreateItemInKnownFolder", 20),
            ("SHCreateShellItem", 16),
            ("SHCreateShellItemArray", 20),
            ("SHCreateShellItemArrayFromDataObject", 12),
            ("SHCreateShellItemArrayFromIDLists", 12),
            ("SHCreateShellItemArrayFromShellItem", 12),
            ("SHBindToParent", 16),
        ] {
            let (k, ret) = api_kinds("shell32", f).unwrap_or_else(|| panic!("{f} non décrite"));
            assert!(ret.is_none(), "{f}");
            assert_eq!(slots(k) * 4, bytes, "{f}");
            assert_eq!(
                Some(bytes as u16),
                super::super::stdcall_bytes("shell32", f),
                "{f}"
            );
        }
    }

    /// #2428, lot E : associations, objet de données et service demandé à
    /// un objet x86 passent par un mandataire, à la taille de stdcall.tsv.
    #[test]
    fn fonctions_du_lot_e_decrites_a_la_taille_de_stdcall() {
        assert_eq!(IQUERYASSOCIATIONS.methods.len(), 5);
        assert_eq!(ISERVICEPROVIDER.methods.len(), 1);
        for (dll, f, octets) in [
            ("shell32", "SHCreateDataObject", 24),
            ("shlwapi", "AssocCreate", 24),
            ("shlwapi", "IUnknown_QueryService", 16),
            ("shlwapi", "#176", 16),
        ] {
            let (k, r) = api_kinds(dll, f).unwrap_or_else(|| panic!("{f} non décrite"));
            assert!(r.is_none(), "{f}");
            assert_eq!(slots(k) * 4, octets, "{f}");
            if !f.starts_with('#') {
                assert_eq!(
                    super::super::stdcall_bytes(dll, f),
                    Some(octets as u16),
                    "{f}"
                );
            }
            assert!(matches!(k[k.len() - 2..], [Iid, OutQ]), "{f}");
        }
        // L'objet x86 interrogé est converti, pas passé brut.
        let (k, _) = api_kinds("shlwapi", "IUnknown_QueryService").unwrap();
        assert!(matches!(k[0], In(f) if std::ptr::eq(f, &IUNKNOWN)));
        // AssocCreate reçoit son CLSID par valeur : quatre cases x86.
        let (k, _) = api_kinds("shlwapi", "AssocCreate").unwrap();
        assert!(matches!(k[0], GuidIn));
    }
}
