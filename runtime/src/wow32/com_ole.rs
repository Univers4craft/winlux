//! Contexte de liaison et table des objets actifs (IBindCtx,
//! IRunningObjectTable) pour les programmes x86 (#2428, lot C).
//!
//! Description des interfaces d'`objidl.h` (en-têtes publics du SDK
//! Windows ; mêmes déclarations dans mingw-w64, domaine public) pour les
//! mandataires COM de `com.rs`. Sans elles, CreateBindCtx rendait au code
//! x86 les 4 octets bas de l'adresse d'un objet à table win64, et
//! GetRunningObjectTable, MkParseDisplayName, CoGetObject et les fonctions
//! de création de monikers n'étaient pas décrites du tout. Les objets
//! eux-mêmes sont ceux de `dlls/ole32_moniker.rs`.
//!
//! Conventions :
//! - BIND_OPTS* est `I` : le runtime copie les octets selon cbStruct, sans
//!   les interpréter, donc un BIND_OPTS2/3 x86 fait l'aller-retour intact ;
//!   les champs DWORD occupent les mêmes positions en x86 et en x64 ;
//! - FILETIME* et DWORD* sont `I` (données sans pointeur) ;
//! - une méthode qui rend un énumérateur (IEnumString, IEnumMoniker) n'est
//!   pas décrite : leur Next rend un tableau de chaînes ou d'interfaces
//!   qu'aucun genre `A` ne convertit ; le code x86 reçoit E_NOTIMPL.

use super::com::{Iface, A::*, IMONIKER, IUNKNOWN};
use crate::dlls::comobj::{guid, Guid};

const fn ole(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

/// IBindCtx {0000000E-0000-0000-C000-000000000046}.
pub(super) static IBINDCTX: Iface = Iface {
    name: "IBindCtx",
    iid: ole(0xE),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IUNKNOWN)]),             // RegisterObjectBound(punk)
        Some(&[In(&IUNKNOWN)]),             // RevokeObjectBound(punk)
        Some(&[]),                          // ReleaseBoundObjects
        Some(&[I]),                         // SetBindOptions(BIND_OPTS*)
        Some(&[I]),                         // GetBindOptions(BIND_OPTS*)
        Some(&[Out(&IRUNNINGOBJECTTABLE)]), // GetRunningObjectTable
        Some(&[I, In(&IUNKNOWN)]),          // RegisterObjectParam(key, punk)
        Some(&[I, Out(&IUNKNOWN)]),         // GetObjectParam(key, ppunk)
        None,                               // EnumObjectParam(IEnumString**)
        Some(&[I]),                         // RevokeObjectParam(key)
    ],
};

/// IRunningObjectTable {00000010-0000-0000-C000-000000000046}.
pub(super) static IRUNNINGOBJECTTABLE: Iface = Iface {
    name: "IRunningObjectTable",
    iid: ole(0x10),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, In(&IUNKNOWN), In(&IMONIKER), I]), // Register(flags, punk, pmk, DWORD*)
        Some(&[I]),                                  // Revoke(cookie)
        Some(&[In(&IMONIKER)]),                      // IsRunning
        Some(&[In(&IMONIKER), Out(&IUNKNOWN)]),      // GetObject
        Some(&[I, I]),                               // NoteChangeTime(cookie, FILETIME*)
        Some(&[In(&IMONIKER), I]),                   // GetTimeOfLastChange(pmk, FILETIME*)
        None,                                        // EnumRunning(IEnumMoniker**)
    ],
};

pub(super) static ALL: &[&Iface] = &[&IBINDCTX, &IRUNNINGOBJECTTABLE];

#[cfg(test)]
mod tests {
    use super::super::com::{api_kinds, by_iid, slots, A};
    use super::*;

    #[test]
    fn contexte_de_liaison_decrit_pour_le_code_x86() {
        // IUnknown (3) + méthodes propres, comme les tables du runtime.
        assert_eq!(IBINDCTX.count(), 13);
        assert_eq!(IRUNNINGOBJECTTABLE.count(), 10);
        assert!(by_iid(&ole(0xE)).is_some_and(|f| std::ptr::eq(f, &IBINDCTX)));
        assert!(by_iid(&ole(0x10)).is_some_and(|f| std::ptr::eq(f, &IRUNNINGOBJECTTABLE)));
        // Seuls les énumérateurs restent non décrits.
        let vides = |f: &'static Iface| (3..f.count()).filter(|&i| f.method(i).is_none()).count();
        assert_eq!(vides(&IBINDCTX), 1);
        assert_eq!(vides(&IRUNNINGOBJECTTABLE), 1);
        // IMoniker reçoit un vrai IBindCtx et un vrai IMoniker à gauche.
        let bind = IMONIKER.method(8).expect("BindToObject");
        assert!(matches!(bind[0], In(f) if std::ptr::eq(f, &IBINDCTX)));
        assert!(matches!(bind[1], In(f) if std::ptr::eq(f, &IMONIKER)));
    }

    #[test]
    fn fonctions_du_lot_c_decrites_a_la_taille_de_stdcall() {
        let rend =
            |k: &[A], f: &'static Iface| matches!(k.last(), Some(Out(g)) if std::ptr::eq(*g, f));
        for (f, iface) in [
            ("CreateBindCtx", Some(&IBINDCTX)),
            ("GetRunningObjectTable", Some(&IRUNNINGOBJECTTABLE)),
            ("CreateFileMoniker", Some(&IMONIKER)),
            ("CreateItemMoniker", Some(&IMONIKER)),
            ("CreatePointerMoniker", Some(&IMONIKER)),
            ("MkParseDisplayName", Some(&IMONIKER)),
            ("CoGetObject", None),
            ("OleLoadFromStream", None),
        ] {
            let (k, r) = api_kinds("ole32", f).unwrap_or_else(|| panic!("{f} non décrite"));
            assert!(r.is_none(), "{f}");
            let octets = super::super::stdcall_bytes("ole32", f).unwrap_or_else(|| panic!("{f}"));
            assert_eq!(slots(k) * 4, octets as usize, "{f}");
            match iface {
                Some(i) => assert!(rend(k, i), "{f}"),
                None => assert!(matches!(k[k.len() - 2..], [Iid, OutQ]), "{f}"),
            }
        }
    }
}
