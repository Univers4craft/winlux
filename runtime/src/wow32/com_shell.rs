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
pub(super) static ALL: &[&Iface] = &[&ISHELLITEM, &ISHELLITEMARRAY];

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
}
