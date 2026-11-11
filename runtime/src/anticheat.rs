//! Logiciels qui embarquent un anti-triche noyau : installation refusée
//! (#1720).
//!
//! Un anti-triche noyau atteste qu'il tourne sur un vrai noyau Windows.
//! Winlux ne charge aucun pilote `.sys`. Sous Winlux, il conclurait au
//! mieux à une panne, au pire à une triche, et le compte du joueur risquerait
//! une suspension. Winlux refuse donc d'installer ces logiciels avant de
//! poser quoi que ce soit, avec un message dans la langue de l'interface.
//! Aucune adaptation ni contournement de l'anti-triche n'est tenté.
//!
//! La détection porte sur la présence de l'anti-triche, jamais sur le nom
//! d'un jeu :
//! - un pilote de la liste dans le dossier du programme, ou dans la table
//!   File d'un paquet MSI ;
//! - un programme d'installation de l'éditeur qui pose ce pilote ;
//! - en dernier recours, la création d'un service pilote de la liste.
//!
//! Easy Anti-Cheat et BattlEye ne sont **pas** dans la liste : leurs éditeurs
//! peuvent activer une prise en charge Linux jeu par jeu, et un jeu innocent
//! ne doit pas être bloqué. Liste et sources : docs/COMPATIBILITE.md,
//! section « Anti-triche noyau : installation refusée ».

use std::path::Path;

/// Un anti-triche dont la présence fait refuser l'installation.
pub struct AntiCheat {
    /// Nom public de la protection, affiché dans le message.
    pub protection: &'static str,
    /// Éditeur de la protection.
    pub publisher: &'static str,
    /// Pilotes noyau qu'elle pose (noms de fichiers, sans casse).
    pub drivers: &'static [&'static str],
    /// Programmes de l'éditeur qui la posent ou l'exigent (motifs Windows,
    /// sans casse).
    pub programs: &'static [&'static str],
    /// Source publique des noms ci-dessus.
    pub source: &'static str,
}

/// Liste explicite. Chaque nom vient d'une documentation publique de
/// l'éditeur ou d'un avis de sécurité public (colonne `source`).
pub const LIST: &[AntiCheat] = &[
    AntiCheat {
        protection: "Riot Vanguard",
        publisher: "Riot Games",
        drivers: &["vgk.sys"],
        programs: &[
            "Install League of Legends*.exe",
            "Install VALORANT*.exe",
            "RiotClientServices.exe",
        ],
        source: "https://support-valorant.riotgames.com (articles Vanguard : pilote vgk, service vgc)",
    },
    AntiCheat {
        protection: "Roblox Hyperion",
        publisher: "Roblox",
        drivers: &[],
        programs: &[
            "RobloxPlayerInstaller*.exe",
            "RobloxPlayerLauncher.exe",
            "RobloxPlayerBeta.exe",
        ],
        source: "https://en.help.roblox.com (installation du lecteur Roblox ; protection Hyperion annoncée par Roblox en 2023)",
    },
    AntiCheat {
        protection: "FACEIT Anti-Cheat",
        publisher: "FACEIT",
        drivers: &["faceit.sys"],
        programs: &[],
        source: "https://support.faceit.com (pilote faceit.sys de l'anti-triche)",
    },
    AntiCheat {
        protection: "XIGNCODE3",
        publisher: "Wellbia",
        drivers: &["xhunter1.sys"],
        programs: &[],
        source: "pilote xhunter1.sys, documenté publiquement (avis de sécurité)",
    },
    AntiCheat {
        protection: "mhyprot",
        publisher: "miHoYo / HoYoverse",
        drivers: &["mhyprot2.sys", "mhyprot3.sys"],
        programs: &[],
        source: "pilotes mhyprot2.sys et mhyprot3.sys, avis de sécurité publics (2022)",
    },
];

/// Code de sortie d'un refus : ERROR_INSTALL_PACKAGE_REJECTED (1625).
pub const REFUSED: i32 = 1625;

/// Marqueur de la ligne écrite sur stderr : winlux-open le cherche pour ne
/// pas afficher en plus une boîte « installation impossible ».
pub const MARKER: &str = "installation refusée (anti-triche noyau)";

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Anti-triche dont `name` est un pilote (chemin Windows ou Linux accepté).
pub fn by_driver(name: &str) -> Option<&'static AntiCheat> {
    let file = file_name(name.trim().trim_matches('"'));
    LIST.iter()
        .find(|a| a.drivers.iter().any(|d| d.eq_ignore_ascii_case(file)))
}

/// Anti-triche dont `name` est un programme d'installation ou client.
pub fn by_program(name: &str) -> Option<&'static AntiCheat> {
    let file = file_name(name);
    LIST.iter()
        .find(|a| a.programs.iter().any(|p| crate::setup::wildcard(p, file)))
}

/// Un pilote de la liste parmi les fichiers posés à côté du programme.
fn driver_in_dir(dir: &Path) -> Option<&'static AntiCheat> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find_map(|e| by_driver(&e.file_name().to_string_lossy()))
}

/// Anti-triche embarqué par le programme ou le paquet `path` (.exe ou .msi).
pub fn in_package(path: &Path) -> Option<&'static AntiCheat> {
    if let Some(a) = by_program(&path.to_string_lossy()) {
        return Some(a);
    }
    let is_msi = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"));
    if is_msi {
        if let Some(a) = crate::msi::install::file_names(path)
            .iter()
            .find_map(|f| by_driver(f))
        {
            return Some(a);
        }
    }
    path.parent().and_then(driver_in_dir)
}

/// Moment du refus : avant toute écriture, ou pendant l'installation (le
/// filet de la création de service).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Before,
    During,
}

fn lang_col(lang: &str) -> usize {
    match lang.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("en") => 1,
        Some("de") => 2,
        Some("es") => 3,
        _ => 0,
    }
}

/// Titre de la boîte, dans la langue de l'interface.
pub fn title(lang: &str) -> &'static str {
    [
        "Installation refusée",
        "Installation blocked",
        "Installation abgelehnt",
        "Instalación rechazada",
    ][lang_col(lang)]
}

/// Bouton de la boîte, dans la langue de l'interface : le bouton standard
/// de GTK suivrait la langue du système (#1776).
pub fn button(lang: &str) -> &'static str {
    ["OK", "OK", "OK", "Aceptar"][lang_col(lang)]
}

/// Message du refus, dans la langue de l'interface : nomme le logiciel,
/// dit la raison et ne laisse pas croire à une panne de Winlux.
pub fn message(lang: &str, software: &str, a: &AntiCheat, stage: Stage) -> String {
    let p = a.protection;
    let (head, why, end) = match lang_col(lang) {
        1 => (
            format!("For the security of your account, Winlux cannot install “{software}”."),
            format!("This software relies on {p}, an anti-cheat that requires the Windows kernel. On Winlux, it could take your computer for a cheater and get your account suspended."),
            if stage == Stage::Before {
                "This is not a Winlux fault. Nothing was installed."
            } else {
                "This is not a Winlux fault. The installation was stopped before the anti-cheat driver was set up."
            },
        ),
        2 => (
            format!("Zur Sicherheit Ihres Kontos kann Winlux „{software}“ nicht installieren."),
            format!("Diese Software setzt {p} voraus, einen Cheat-Schutz, der den Windows-Kernel benötigt. Unter Winlux könnte er Ihren Computer für einen Betrüger halten und Ihr Konto sperren lassen."),
            if stage == Stage::Before {
                "Das ist kein Fehler von Winlux. Es wurde nichts installiert."
            } else {
                "Das ist kein Fehler von Winlux. Die Installation wurde angehalten, bevor der Treiber des Cheat-Schutzes eingerichtet wurde."
            },
        ),
        3 => (
            format!("Por la seguridad de su cuenta, Winlux no puede instalar «{software}»."),
            format!("Este programa depende de {p}, un antitrampas que exige el núcleo de Windows. En Winlux, podría tomar su ordenador por un tramposo y hacer que suspendan su cuenta."),
            if stage == Stage::Before {
                "No es un fallo de Winlux. No se ha instalado nada."
            } else {
                "No es un fallo de Winlux. La instalación se detuvo antes de que se configurara el controlador del antitrampas."
            },
        ),
        _ => (
            format!("Pour la sécurité de votre compte, Winlux ne peut pas installer « {software} »."),
            format!("Ce logiciel s’appuie sur {p}, un anti-triche qui exige le noyau Windows. Sous Winlux, il pourrait prendre votre ordinateur pour un tricheur et faire suspendre votre compte."),
            if stage == Stage::Before {
                "Ce n’est pas une panne de Winlux. Rien n’a été installé."
            } else {
                "Ce n’est pas une panne de Winlux. L’installation a été arrêtée avant la mise en place du pilote de l’anti-triche."
            },
        ),
    };
    format!("{head}\n\n{why}\n\n{end}")
}

/// Script de la boîte : titre, texte et libellé du bouton en arguments.
const DIALOG: &str = "import sys\nimport gi\ngi.require_version('Gtk', '3.0')\nfrom gi.repository import Gtk\n\
title, text, button = sys.argv[1], sys.argv[2], sys.argv[3]\nhead, _, rest = text.partition('\\n\\n')\n\
d = Gtk.MessageDialog(message_type=Gtk.MessageType.WARNING, buttons=Gtk.ButtonsType.NONE, text=head)\n\
d.add_button(button, Gtk.ResponseType.OK)\nd.set_default_response(Gtk.ResponseType.OK)\n\
d.set_title(title)\nd.format_secondary_text(rest)\nd.set_keep_above(True)\nd.run()\n";

/// Boîte GTK (python3 du système), sinon notification. Bloquante : le
/// programme refusé attend que la personne ait lu.
fn show(title: &str, text: &str, button: &str) {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    let shown = std::process::Command::new("python3")
        .args(["-I", "-c", DIALOG, title, text, button])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !shown {
        let _ = std::process::Command::new("notify-send")
            .args(["--app-name=Winlux", "-i", "dialog-warning", title, text])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Prévient la personne (une seule fois par processus) et rend le code de
/// refus. `software` : nom du programme ou du paquet refusé.
pub fn refuse(software: &str, a: &AntiCheat, stage: Stage) -> i32 {
    static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let lang = crate::permissions::interface_lang();
    let text = message(&lang, software, a, stage);
    eprintln!(
        "winlux : {MARKER} : « {software} » embarque {} ({} ; source : {}).",
        a.protection, a.publisher, a.source
    );
    crate::log_warn!(
        "anti-triche",
        "{software} : {} refusé (#1720)",
        a.protection
    );
    if !TOLD.swap(true, std::sync::atomic::Ordering::Relaxed) {
        show(title(&lang), &text, button(&lang));
    }
    REFUSED
}

/// Contrôle d'un lancement ou d'une installation, avant toute écriture.
/// `Some(code)` : refusé, le processus doit s'arrêter avec ce code.
pub fn check_launch(path: &Path) -> Option<i32> {
    if std::env::var_os("WINLUX_SERVICE_NAME").is_some() {
        // Un service déjà inscrit relève du filet de la création de service.
        return None;
    }
    let a = in_package(path)?;
    let software = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(refuse(&software, a, Stage::Before))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_par_pilote_et_par_programme() {
        assert_eq!(
            by_driver("C:\\Program Files\\Riot Vanguard\\VGK.SYS")
                .unwrap()
                .protection,
            "Riot Vanguard"
        );
        assert_eq!(
            by_driver("\"\\SystemRoot\\System32\\drivers\\faceit.sys\"")
                .unwrap()
                .protection,
            "FACEIT Anti-Cheat"
        );
        assert!(by_driver("EasyAntiCheat_EOS.sys").is_none());
        assert!(by_driver("BEDaisy.sys").is_none());
        assert!(by_driver("vgk.sys.txt").is_none());
        assert_eq!(
            by_program("/home/x/Install League of Legends euw.exe")
                .unwrap()
                .protection,
            "Riot Vanguard"
        );
        assert_eq!(
            by_program("RobloxPlayerInstaller-abc123.exe")
                .unwrap()
                .protection,
            "Roblox Hyperion"
        );
        // Un jeu ou un lanceur sans anti-triche noyau n'est jamais bloqué.
        for innocent in [
            "SteamSetup.exe",
            "EpicInstaller-15.17.1.msi",
            "RobloxStudioInstaller.exe",
            "Battle.net-Setup.exe",
            "GalaxySetup.exe",
        ] {
            assert!(by_program(innocent).is_none(), "{innocent}");
        }
    }

    #[test]
    fn pilote_pose_a_cote_du_programme() {
        let dir = std::env::temp_dir().join(format!("winlux-1720-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(in_package(&exe).is_none());
        std::fs::write(dir.join("xhunter1.sys"), b"").unwrap();
        assert_eq!(in_package(&exe).unwrap().protection, "XIGNCODE3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_de_la_boite_valide() {
        let Ok(out) = std::process::Command::new("python3")
            .args([
                "-I",
                "-c",
                "import ast, sys; ast.parse(sys.argv[1])",
                DIALOG,
            ])
            .output()
        else {
            return; // pas de python3 sur la machine de test
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(DIALOG.contains("add_button(button") && !DIALOG.contains("ButtonsType.OK"));
    }

    #[test]
    fn message_en_quatre_langues() {
        let a = &LIST[0];
        for (lang, mots) in [
            (
                "fr_FR.UTF-8",
                [
                    "sécurité de votre compte",
                    "Rien n’a été installé",
                    "pas une panne de Winlux",
                ],
            ),
            (
                "en_US",
                [
                    "security of your account",
                    "Nothing was installed",
                    "not a Winlux fault",
                ],
            ),
            (
                "de_DE",
                [
                    "Sicherheit Ihres Kontos",
                    "nichts installiert",
                    "kein Fehler von Winlux",
                ],
            ),
            (
                "es_ES",
                [
                    "seguridad de su cuenta",
                    "No se ha instalado nada",
                    "no es un fallo de Winlux",
                ],
            ),
        ] {
            let m = message(lang, "Install VALORANT.exe", a, Stage::Before);
            assert!(
                m.contains("Install VALORANT.exe") && m.contains("Riot Vanguard"),
                "{m}"
            );
            for mot in mots {
                assert!(
                    m.to_lowercase().contains(&mot.to_lowercase()),
                    "{lang} : {mot}"
                );
            }
        }
        assert!(message("fr", "x", a, Stage::During).contains("arrêtée avant"));
        assert_eq!(title("de"), "Installation abgelehnt");
        // Bouton dans la langue de l'interface, pas celle du système (#1776).
        assert_eq!(button("es_ES"), "Aceptar");
        assert_eq!(button("de"), "OK");
        assert!(!["fr", "en", "de", "es"]
            .iter()
            .any(|l| button(l) == "Valider"));
    }
}
