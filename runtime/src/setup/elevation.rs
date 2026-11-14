//! Élévation des installateurs que Winlux déroule lui-même (#2180).
//!
//! Un bundle Burn, un paquet MSI, un installateur NSIS ou Inno n'est pas
//! exécuté comme un .exe ordinaire : son manifeste ne passait donc jamais
//! par `uac::check_launch`. La même règle s'applique désormais avant de les
//! dérouler, selon ce que l'installateur demande :
//! - bundle Burn « par machine » (`Registration PerMachine="yes"`) ;
//! - paquet MSI avec `ALLUSERS=1` (`2` : le plus haut niveau disponible) ;
//! - installateur dont le manifeste demande `requireAdministrator` ou
//!   `highestAvailable` (NSIS `RequestExecutionLevel admin`, Inno
//!   `PrivilegesRequired=admin`).
//!
//! Compte administrateur : accordée sans question (#1739) et consignée au
//! journal ; compte standard : mot de passe d'un administrateur demandé,
//! refus = ERROR_CANCELLED sans rien installer.

use crate::uac::{self, Level};
use std::path::Path;

/// Niveau d'après ALLUSERS d'un paquet MSI.
fn msi_level(allusers: &str) -> Level {
    match allusers.trim() {
        "1" => Level::RequireAdministrator,
        "2" => Level::HighestAvailable,
        _ => Level::AsInvoker,
    }
}

fn is_msi(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"))
}

/// Niveau demandé par l'installateur. `props` : propriétés de la ligne de
/// commande de msiexec (ALLUSERS=… y prime sur le paquet).
pub fn level(path: &Path, props: &[(String, String)]) -> Level {
    if is_msi(path) {
        let given = props
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("ALLUSERS"))
            .map(|(_, v)| v.clone());
        let value = given.or_else(|| crate::msi::install::allusers(path));
        return msi_level(&value.unwrap_or_default());
    }
    let manifest = crate::loader::ImageFile::open(path)
        .map(|d| uac::level_of_file(&d))
        .unwrap_or(Level::AsInvoker);
    if manifest == Level::AsInvoker && super::burn::per_machine(path) == Some(true) {
        return Level::RequireAdministrator;
    }
    manifest
}

/// Demande l'élévation si l'installateur l'exige ; `Err(1223)` si refusée.
pub fn require(path: &Path, props: &[(String, String)]) -> Result<(), i32> {
    let level = level(path, props);
    if level != Level::AsInvoker {
        crate::log_info!(
            "uac",
            "installateur {} : droits demandés {level:?}",
            path.display()
        );
    }
    uac::request(level, false, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niveau_selon_allusers() {
        assert_eq!(msi_level("1"), Level::RequireAdministrator);
        assert_eq!(msi_level("2"), Level::HighestAvailable);
        assert_eq!(msi_level(""), Level::AsInvoker);
        assert_eq!(msi_level("0"), Level::AsInvoker);
    }

    #[test]
    fn manifeste_de_l_image_seule() {
        // PE minimal : une section de 0x200 octets à 0x200 ; le manifeste
        // collé après l'image (données d'un installateur) ne compte pas.
        let mut pe = vec![0u8; 0x400];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x86] = 1; // une section
        pe[0x94] = 0xF0; // taille de l'en-tête optionnel
        let sec = 0x80 + 24 + 0xF0;
        pe[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
        pe[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes());
        let admin = br#"<requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>"#;
        let mut outside = pe.clone();
        outside.extend_from_slice(admin);
        assert_eq!(uac::level_of_file(&outside), Level::AsInvoker);
        let mut inside = pe;
        inside[0x250..0x250 + admin.len()].copy_from_slice(admin);
        assert_eq!(uac::level_of_file(&inside), Level::RequireAdministrator);
    }
}
