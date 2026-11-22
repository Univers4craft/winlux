//! gamemode.dll, contrat api-ms-win-gaming-expandedresources-l1-1-0 : le
//! mode Jeu de Windows. Winlux n'accorde jamais de ressources exclusives à
//! un jeu : l'appli tourne toujours en mode partagé (Refs #2902).
//!
//! Source : pages sdk-api nf-expandedresources-* (DLL : Gamemode.dll) ;
//! signatures : expandedresources.h de mingw-w64 (domaine public).
//! GetExpandedResourceExclusiveCpuCount rend 0 « si aucun ensemble de CPU
//! exclusif n'est disponible », d'après sa page.

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

/// `HasExpandedResources(BOOL*)` : FALSE, jamais en mode Jeu. Inférence :
/// un pointeur nul rend E_INVALIDARG.
pub unsafe extern "win64" fn has_expanded_resources(out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    out.write_unaligned(0);
    S_OK
}

/// `GetExpandedResourceExclusiveCpuCount(ULONG*)` : 0.
pub unsafe extern "win64" fn get_expanded_resource_exclusive_cpu_count(out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    out.write_unaligned(0);
    S_OK
}

/// `ReleaseExclusiveCpuSets()` : l'appli partage déjà tous les cœurs.
pub extern "win64" fn release_exclusive_cpu_sets() -> i32 {
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "HasExpandedResources" => fn has_expanded_resources,
        "GetExpandedResourceExclusiveCpuCount" => fn get_expanded_resource_exclusive_cpu_count,
        "ReleaseExclusiveCpuSets" => fn release_exclusive_cpu_sets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jamais_en_mode_jeu_2902() {
        let (mut b, mut n) = (7i32, 7u32);
        unsafe {
            assert_eq!(has_expanded_resources(&mut b), S_OK);
            assert_eq!(b, 0);
            assert_eq!(get_expanded_resource_exclusive_cpu_count(&mut n), S_OK);
            assert_eq!(n, 0);
            assert_eq!(has_expanded_resources(std::ptr::null_mut()), E_INVALIDARG);
        }
        assert_eq!(release_exclusive_cpu_sets(), S_OK);
    }
}
