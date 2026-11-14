//! Contrôle de compte d'utilisateur (UAC) maison.
//!
//! Un .exe dont le manifeste demande `requireAdministrator` (installateurs,
//! outils système) ou qu'une autre appli lance avec le verbe `runas`
//! affiche la fenêtre « Voulez-vous autoriser cette application à apporter
//! des modifications à votre appareil ? » (`winlux-uac`). Le mot de passe
//! du compte local est vérifié ; refusé, le programme ne démarre pas
//! (code ERROR_CANCELLED, comme sous Windows).
//!
//! Accepté, le processus reçoit un jeton administrateur (`WINLUX_ELEVATED=1`,
//! hérité par ses enfants) : `IsUserAnAdmin`, `TokenElevation`,
//! `CheckTokenMembership(Administrateurs)` répondent oui. Il reste lancé sous
//! le compte de l'utilisateur : les fichiers qu'il crée lui appartiennent.
//! `C:\Program Files` et `C:\ProgramData` sont dans le profil, donc
//! inscriptibles sans root.
//!
//! Compte administrateur : l'élévation est accordée **sans question** (règle
//! de damien, #1739 : un double-clic lance l'application). La signature de
//! l'éditeur et l'élévation sont consignées au journal. Compte standard
//! (enfant, invité) : la fenêtre demande le mot de passe d'un
//! administrateur, comme sous Windows ; c'est un contrôle d'accès entre
//! comptes, pas une question de confort.
//!
//! `WINLUX_UAC=allow|deny` force la réponse (tests, sessions sans écran).

use std::path::Path;

/// Niveau demandé par le manifeste.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Level {
    AsInvoker,
    HighestAvailable,
    RequireAdministrator,
}

/// Lit `requestedExecutionLevel level="…"` dans un manifeste.
pub fn level_of(manifest: &str) -> Level {
    let lower = manifest.to_ascii_lowercase();
    let Some(at) = lower.find("requestedexecutionlevel") else {
        return Level::AsInvoker;
    };
    let tail = &lower[at + "requestedexecutionlevel".len()..];
    let end = tail.find('>').unwrap_or(tail.len());
    let element = &tail[..end];
    let Some(l) = element.find("level") else {
        return Level::AsInvoker;
    };
    let value = element[l + 5..]
        .trim_start_matches([' ', '\t', '\r', '\n', '='])
        .trim_start_matches(['"', '\'']);
    if value.starts_with("requireadministrator") {
        Level::RequireAdministrator
    } else if value.starts_with("highestavailable") {
        Level::HighestAvailable
    } else {
        Level::AsInvoker
    }
}

/// Faut-il demander l'élévation au lancement ?
pub fn needs_prompt(level: Level, forced: bool, elevated: bool, admin_capable: bool) -> bool {
    if elevated {
        return false;
    }
    forced
        || level == Level::RequireAdministrator
        // highestAvailable : un administrateur est sollicité, un compte
        // standard démarre sans élévation, comme sous Windows.
        || (level == Level::HighestAvailable && admin_capable)
}

fn helper() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("WINLUX_UAC_HELPER") {
        return Some(p.into());
    }
    let paths = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
    std::env::split_paths(&paths)
        .map(|d| d.join("winlux-uac"))
        .find(|p| p.is_file())
}

/// Réponse de l'utilisateur.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Allowed,
    Refused,
    /// Pas d'écran ni de fenêtre UAC : on démarre sans élévation.
    Unavailable,
}

/// Faut-il afficher la fenêtre ? Seulement pour un compte qui ne peut pas
/// devenir administrateur par lui-même (#1739).
pub fn shows_dialog(admin_capable: bool) -> bool {
    !admin_capable
}

fn ask(exe: &Path, admin_capable: bool) -> Answer {
    match std::env::var("WINLUX_UAC").as_deref() {
        Ok("allow") => return Answer::Allowed,
        Ok("deny") => return Answer::Refused,
        _ => {}
    }
    use crate::setup::appx_signature::{examine_pe, Signature};
    let signature = examine_pe(exe);
    crate::log_info!("uac", "signature de {} : {signature:?}", exe.display());
    if !shows_dialog(admin_capable) {
        return Answer::Allowed;
    }
    let has_display = std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty())
        || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty());
    let Some(helper) = helper().filter(|_| has_display) else {
        return Answer::Unavailable;
    };
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut cmd = std::process::Command::new(helper);
    cmd.arg("--app")
        .arg(&name)
        .arg("--path")
        .arg(crate::paths::to_windows(exe));
    // Éditeur vérifié par sa signature Authenticode (#1382) ; sinon la
    // fenêtre dit « Éditeur : inconnu », comme Windows.
    if let Signature::Verified { signer } = signature {
        cmd.arg("--publisher").arg(signer);
    }
    let status = cmd.status();
    match status {
        Ok(s) if s.success() => Answer::Allowed,
        Ok(_) => Answer::Refused,
        Err(_) => Answer::Unavailable,
    }
}

/// ERROR_CANCELLED : l'opération a été annulée par l'utilisateur (199 pour
/// un parent Linux, qui ne voit que 8 bits).
pub const CANCELLED: i32 = 1223;

/// Demande l'élévation pour `exe` au niveau `level` si la règle l'exige.
/// Accordée : le processus et ses enfants reçoivent le jeton administrateur
/// (`WINLUX_ELEVATED=1`). Rend `Err(CANCELLED)` si l'utilisateur refuse.
pub fn request(level: Level, forced: bool, exe: &Path) -> Result<(), i32> {
    let admin_capable = crate::dlls::security::admin_capable();
    if !needs_prompt(
        level,
        forced,
        crate::dlls::security::elevated(),
        admin_capable,
    ) {
        return Ok(());
    }
    match ask(exe, admin_capable) {
        Answer::Allowed => {
            crate::log_info!("uac", "élévation accordée pour {}", exe.display());
            std::env::set_var("WINLUX_ELEVATED", "1");
            Ok(())
        }
        Answer::Refused => {
            crate::log_info!("uac", "élévation refusée pour {}", exe.display());
            Err(CANCELLED)
        }
        Answer::Unavailable => {
            crate::log_warn!(
                "uac",
                "{} demande les droits administrateur, mais aucune fenêtre UAC n'est disponible : lancement sans élévation",
                exe.display()
            );
            Ok(())
        }
    }
}

/// Au lancement d'un .exe : lit le manifeste, demande l'élévation si besoin.
/// Ne revient pas si l'utilisateur refuse.
pub fn check_launch(base: usize, exe: &Path) {
    let level = crate::dlls::resource::manifest(base)
        .map(|m| level_of(&m))
        .unwrap_or(Level::AsInvoker);
    // `runas` (ShellExecuteEx) : le parent pose WINLUX_ELEVATE=1.
    let forced = std::env::var("WINLUX_ELEVATE").is_ok_and(|v| v == "1");
    std::env::remove_var("WINLUX_ELEVATE");
    if let Err(code) = request(level, forced, exe) {
        std::process::exit(code);
    }
}

/// Niveau demandé par le manifeste d'un PE sur disque, lu dans l'image
/// seule (en-têtes et sections) : les données qu'un installateur NSIS ou
/// Inno colle après son image ne comptent pas (#2180).
pub fn level_of_file(data: &[u8]) -> Level {
    let end = pe_image_end(data).unwrap_or(0).min(data.len());
    let image = &data[..end];
    let Some(at) = find(image, b"requestedExecutionLevel") else {
        return Level::AsInvoker;
    };
    let from = at.saturating_sub(16);
    let to = (at + 256).min(image.len());
    level_of(&String::from_utf8_lossy(&image[from..to]))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Fin de l'image PE dans le fichier : fin des données brutes de la
/// dernière section (le reste est une « surcouche » ajoutée au fichier).
fn pe_image_end(d: &[u8]) -> Option<usize> {
    let u16_at = |o: usize| {
        d.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |o: usize| {
        d.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if d.get(..2) != Some(b"MZ") {
        return None;
    }
    let pe = u32_at(0x3c)?;
    if d.get(pe..pe + 4) != Some(b"PE\0\0") {
        return None;
    }
    let sections = u16_at(pe + 6)?;
    let first = pe + 24 + u16_at(pe + 20)?;
    (0..sections.min(96))
        .filter_map(|i| {
            let s = first + 40 * i;
            Some(u32_at(s + 20)? + u32_at(s + 16)?)
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niveau_du_manifeste() {
        let m = r#"<?xml version="1.0"?><assembly><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
            <security><requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
            </requestedPrivileges></security></trustInfo></assembly>"#;
        assert_eq!(level_of(m), Level::RequireAdministrator);
        assert_eq!(
            level_of("<requestedExecutionLevel uiAccess='false' level = 'highestAvailable' />"),
            Level::HighestAvailable
        );
        assert_eq!(
            level_of(r#"<requestedExecutionLevel level="asInvoker"/>"#),
            Level::AsInvoker
        );
        assert_eq!(level_of("<assembly/>"), Level::AsInvoker);
        // « level » d'un autre élément ne compte pas.
        assert_eq!(
            level_of(
                r#"<requestedExecutionLevel uiAccess="false"/><x level="requireAdministrator"/>"#
            ),
            Level::AsInvoker
        );
    }

    #[test]
    fn quand_demander() {
        use Level::*;
        assert!(needs_prompt(RequireAdministrator, false, false, false));
        assert!(
            !needs_prompt(RequireAdministrator, false, true, true),
            "déjà élevé"
        );
        assert!(!needs_prompt(AsInvoker, false, false, true));
        assert!(needs_prompt(AsInvoker, true, false, false), "runas");
        assert!(needs_prompt(HighestAvailable, false, false, true));
        assert!(!needs_prompt(HighestAvailable, false, false, false));
    }

    /// Un administrateur lance sans question ; un compte standard doit
    /// donner le mot de passe d'un administrateur (#1739).
    #[test]
    fn fenetre_seulement_pour_un_compte_standard() {
        assert!(!shows_dialog(true));
        assert!(shows_dialog(false));
    }
}
