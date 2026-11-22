//! Contrats api-ms-win-security-isolatedcontainer-l1-1-0 et -l1-1-1
//! (isolatedapplauncher.h) : un processus Winlux ne tourne jamais dans un
//! conteneur isolé ni dans un conteneur Microsoft Defender Application
//! Guard, la réponse est donc « non » (Refs #2902).
//!
//! Source : pages sdk-api nf-isolatedapplauncher-isprocessinisolatedcontainer
//! et -isprocessinwdagcontainer, qui ne nomment que ces contrats pour
//! emplacement (aucune DLL hôte) : le module porte donc le nom du contrat
//! l1-1-0, et l1-1-1 s'y résout. Liste des fonctions : .def de mingw-w64.
//! Inférence : un pointeur de sortie nul rend E_INVALIDARG, les pages ne
//! nommant que « un HRESULT de succès ou d'erreur ».

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

/// `IsProcessInIsolatedContainer(BOOL*)`.
pub unsafe extern "win64" fn is_process_in_isolated_container(out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    out.write_unaligned(0);
    S_OK
}

/// `IsProcessInWDAGContainer(réservé, BOOL*)`.
pub unsafe extern "win64" fn is_process_in_wdag_container(_reserved: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    out.write_unaligned(0);
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsProcessInIsolatedContainer" => fn is_process_in_isolated_container,
        "IsProcessInWDAGContainer" => fn is_process_in_wdag_container,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jamais_dans_un_conteneur_isole_2902() {
        let mut v = 7i32;
        unsafe {
            assert_eq!(is_process_in_isolated_container(&mut v), S_OK);
            assert_eq!(v, 0);
            v = 7;
            assert_eq!(is_process_in_wdag_container(0, &mut v), S_OK);
            assert_eq!(v, 0);
            assert_eq!(
                is_process_in_isolated_container(std::ptr::null_mut()),
                E_INVALIDARG
            );
            assert_eq!(
                is_process_in_wdag_container(0, std::ptr::null_mut()),
                E_INVALIDARG
            );
        }
        for (contrat, f) in [
            (
                "api-ms-win-security-isolatedcontainer-l1-1-0.dll",
                "IsProcessInIsolatedContainer",
            ),
            (
                "api-ms-win-security-isolatedcontainer-l1-1-1.dll",
                "IsProcessInIsolatedContainer",
            ),
            (
                "api-ms-win-security-isolatedcontainer-l1-1-1.dll",
                "IsProcessInWDAGContainer",
            ),
        ] {
            let n = crate::dlls::normalize(contrat);
            assert!(crate::dlls::is_builtin(&n), "{contrat}");
            assert!(crate::dlls::lookup(&n, f).is_some(), "{contrat}!{f}");
        }
    }
}
