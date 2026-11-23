//! Fonctions d'atomes de ntdll : NtAddAtom, NtAddAtomEx, NtFindAtom,
//! NtDeleteAtom (#3003) et NtQueryInformationAtom (#3304).
//!
//! Le bac à sable d'Acrobat Reader, du même type que celui de Chromium (#236),
//! cherche NtAddAtomEx dans ntdll avant de poser ses interceptions dans le
//! ntdll de son moteur de rendu. Comme elle était introuvable, il posait 12
//! interceptions puis tuait l'enfant (passes BD et BE du banc 2, 16 nov.).
//! Avec NtOpenMutant et NtCreateMutant servies, il en pose 26 et s'arrête
//! sur NtQueryInformationAtom, la 27e (passe i17 du banc 1, 23 nov., #3304).
//!
//! Provenance : la seule source permise trouvée est le `ntdll.def` i686 de
//! MinGW-w64 (domaine public), repris dans `wow32/stdcall.tsv`. Il donne la
//! taille des arguments en x86 : 12 octets pour NtAddAtom, 16 pour NtAddAtomEx,
//! 12 pour NtFindAtom, 4 pour NtDeleteAtom et 20 pour NtQueryInformationAtom. Il ne dit rien de leur sens, et
//! aucune page de Learn, aucun en-tête de MinGW-w64, aucune métadonnée de
//! windows-sys ni le bac à sable de Chromium ne les déclare. Ces fonctions
//! existent donc, ne lisent aucun de leurs arguments et rendent un refus
//! franc, STATUS_NOT_IMPLEMENTED, jamais un faux succès. Chaque fonction est
//! journalisée une fois, avec son fichier et sa ligne.

use std::sync::Mutex;

const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;

/// Refus franc, journalisé une fois par fonction.
fn refused(name: &'static str, line: u32) -> i32 {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.contains(&name) {
        seen.push(name);
        crate::log_warn!(
            "process",
            "ntdll!{name} pas encore servie (STATUS_NOT_IMPLEMENTED, {}:{line}) : \
             la forme de ses paramètres n'a pas de source permise (#3003)",
            file!()
        );
    }
    STATUS_NOT_IMPLEMENTED
}

// Le nombre de paramètres suit la taille x86 de stdcall.tsv ; aucun n'est lu.

pub unsafe extern "win64" fn nt_add_atom(_a1: usize, _a2: usize, _a3: usize) -> i32 {
    refused("NtAddAtom", line!())
}

pub unsafe extern "win64" fn nt_add_atom_ex(_a1: usize, _a2: usize, _a3: usize, _a4: usize) -> i32 {
    refused("NtAddAtomEx", line!())
}

pub unsafe extern "win64" fn nt_find_atom(_a1: usize, _a2: usize, _a3: usize) -> i32 {
    refused("NtFindAtom", line!())
}

pub unsafe extern "win64" fn nt_delete_atom(_a1: usize) -> i32 {
    refused("NtDeleteAtom", line!())
}

pub unsafe extern "win64" fn nt_query_information_atom(
    _a1: usize,
    _a2: usize,
    _a3: usize,
    _a4: usize,
    _a5: usize,
) -> i32 {
    refused("NtQueryInformationAtom", line!())
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "NtAddAtom" => fn nt_add_atom,
        "NtAddAtomEx" => fn nt_add_atom_ex,
        "NtFindAtom" => fn nt_find_atom,
        "NtDeleteAtom" => fn nt_delete_atom,
        "NtQueryInformationAtom" => fn nt_query_information_atom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3003 : les quatre fonctions d'atomes sont exportées par ntdll et
    /// rendent un refus franc sans toucher à leurs arguments.
    #[test]
    fn atomes_ntdll_exportes_en_refus_franc_3003() {
        let e = exports();
        for name in [
            "NtAddAtom",
            "NtAddAtomEx",
            "NtFindAtom",
            "NtDeleteAtom",
            "NtQueryInformationAtom",
        ] {
            assert!(e.contains_key(name), "{name} absente");
        }
        // Des adresses invalides : un seul accès les ferait planter.
        let bad = 0x10usize;
        unsafe {
            assert_eq!(nt_add_atom(bad, bad, bad), STATUS_NOT_IMPLEMENTED);
            assert_eq!(nt_add_atom_ex(bad, bad, bad, bad), STATUS_NOT_IMPLEMENTED);
            assert_eq!(nt_find_atom(bad, bad, bad), STATUS_NOT_IMPLEMENTED);
            assert_eq!(nt_delete_atom(bad), STATUS_NOT_IMPLEMENTED);
            // #3304 : la 27e interception d'Acrobat.
            assert_eq!(
                nt_query_information_atom(bad, bad, bad, bad, bad),
                STATUS_NOT_IMPLEMENTED
            );
        }
    }

    /// #3003 : la passerelle x86 dépile ce que dit le ntdll.def de MinGW-w64.
    #[test]
    fn atomes_ntdll_tailles_x86_3003() {
        let t = include_str!("../wow32/stdcall.tsv");
        let mut ntdll = false;
        let mut found = Vec::new();
        for line in t.lines() {
            if let Some(d) = line.strip_prefix('[') {
                ntdll = d == "ntdll]";
                continue;
            }
            if ntdll {
                if let Some((f, n)) = line.split_once(' ') {
                    if matches!(
                        f,
                        "NtAddAtom"
                            | "NtAddAtomEx"
                            | "NtFindAtom"
                            | "NtDeleteAtom"
                            | "NtQueryInformationAtom"
                    ) {
                        found.push((f.to_string(), n.to_string()));
                    }
                }
            }
        }
        found.sort();
        let want = [
            ("NtAddAtom", "12"),
            ("NtAddAtomEx", "16"),
            ("NtDeleteAtom", "4"),
            ("NtFindAtom", "12"),
            ("NtQueryInformationAtom", "20"),
        ];
        let want: Vec<(String, String)> = want
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(found, want);
    }
}
