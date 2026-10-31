//! `winlux --power COMMANDE` : interface de ligne de commande utilisée par
//! `winlux-control` (boutons des Paramètres) et par l'init.

use super::{
    apply_profile_at, delay_label, next_delay, read_status_at, root, sleep, status_lines, Action,
    Config, Profile,
};
use std::path::Path;

const USAGE: &str = "usage: winlux --power status | daemon | apply | profile NOM | \
set CL\u{c9} VALEUR | suspend | hibernate | hibernate-setup\n\
  profils : performance, balanced, saver\n\
  cl\u{e9}s : screen-ac, screen-dc, sleep-ac, sleep-dc (secondes ou next),\n\
         lid, button (nothing, sleep, hibernate, shutdown, display-off ou next),\n\
         brightness (5-100, + ou -)";

fn refresh(root: &Path) {
    let status = read_status_at(root);
    let text = status_lines(&status, &Config::load_at(root), root);
    let dir = super::run_dir_at(root);
    if std::fs::create_dir_all(&dir).is_ok() {
        let tmp = dir.join(format!("power.txt.{}", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("power.txt"));
        }
    }
}

fn next_action(current: Action, allow_display: bool) -> Action {
    let list: Vec<Action> = Action::ALL
        .into_iter()
        .filter(|a| allow_display || *a != Action::DisplayOff)
        .collect();
    let i = list.iter().position(|a| *a == current).unwrap_or(0);
    list[(i + 1) % list.len()]
}

/// Applique `set CLÉ VALEUR` à une configuration ; renvoie le message.
pub fn set(
    config: &mut Config,
    key: &str,
    value: &str,
    has_battery: bool,
) -> Result<String, String> {
    let delay = |current: u32| -> Result<u32, String> {
        if value == "next" {
            Ok(next_delay(current))
        } else {
            value
                .parse::<u32>()
                .map(|d| d.min(86_400))
                .map_err(|_| format!("d\u{e9}lai invalide : {value}"))
        }
    };
    match key {
        "screen-ac" => {
            config.screen_ac = delay(config.screen_ac)?;
            Ok(format!(
                "\u{c9}cran sur secteur : {}",
                delay_label(config.screen_ac)
            ))
        }
        "screen-dc" => {
            config.screen_dc = delay(config.screen_dc)?;
            Ok(format!(
                "\u{c9}cran sur batterie : {}",
                delay_label(config.screen_dc)
            ))
        }
        "sleep-ac" => {
            config.sleep_ac = delay(config.sleep_ac)?;
            Ok(format!(
                "Veille sur secteur : {}",
                delay_label(config.sleep_ac)
            ))
        }
        "sleep-dc" => {
            config.sleep_dc = delay(config.sleep_dc)?;
            Ok(format!(
                "Veille sur batterie : {}",
                delay_label(config.sleep_dc)
            ))
        }
        "lid" => {
            config.lid = if value == "next" {
                next_action(config.lid, true)
            } else {
                Action::parse(value).ok_or(format!("action inconnue : {value}"))?
            };
            Ok(format!("Fermeture du capot : {}", config.lid.label()))
        }
        "button" => {
            let action = if value == "next" {
                next_action(config.button_action(has_battery), true)
            } else {
                Action::parse(value).ok_or(format!("action inconnue : {value}"))?
            };
            config.button = Some(action);
            Ok(format!("Bouton d'alimentation : {}", action.label()))
        }
        "brightness" => {
            config.brightness = match value {
                "+" => (config.brightness + 10).min(100),
                "-" => config.brightness.saturating_sub(10).max(5),
                v => v
                    .parse::<u32>()
                    .map_err(|_| format!("luminosit\u{e9} invalide : {v}"))?
                    .clamp(5, 100),
            };
            Ok(format!("Luminosit\u{e9} : {} %", config.brightness))
        }
        _ => Err(format!("cl\u{e9} inconnue : {key}")),
    }
}

pub fn main(args: &[String]) -> i32 {
    let root = root();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    match cmd {
        "status" => {
            print!(
                "{}",
                status_lines(&read_status_at(&root), &Config::load_at(&root), &root)
            );
            0
        }
        "daemon" => super::daemon::run(&root),
        "apply" => {
            let config = Config::load_at(&root);
            let a = apply_profile_at(&root, &config);
            println!(
                "Mode {} appliqu\u{e9} : gouverneur {} ({} c\u{153}ur(s)), luminosit\u{e9} {}, {} p\u{e9}riph\u{e9}rique(s)",
                config.profile.label(),
                a.governor.unwrap_or("non g\u{e9}r\u{e9} par ce mat\u{e9}riel"),
                a.cpus,
                a.brightness
                    .map_or_else(|| "sans r\u{e9}tro\u{e9}clairage".into(), |b| format!("{b} %")),
                a.devices
            );
            refresh(&root);
            0
        }
        "profile" => {
            let Some(profile) = args.get(1).and_then(|p| Profile::parse(p)) else {
                eprintln!("{USAGE}");
                return 2;
            };
            let mut config = Config::load_at(&root);
            config.profile = profile;
            if let Err(e) = config.save_at(&root) {
                eprintln!("Enregistrement impossible : {e}");
                return 1;
            }
            let a = apply_profile_at(&root, &config);
            println!(
                "Mode {} : gouverneur {}, {} p\u{e9}riph\u{e9}rique(s) r\u{e9}gl\u{e9}(s)",
                profile.label(),
                a.governor
                    .unwrap_or("non g\u{e9}r\u{e9} par ce mat\u{e9}riel"),
                a.devices
            );
            refresh(&root);
            0
        }
        "set" => {
            let (Some(key), Some(value)) = (args.get(1), args.get(2)) else {
                eprintln!("{USAGE}");
                return 2;
            };
            let mut config = Config::load_at(&root);
            let has_battery = read_status_at(&root).battery.is_some();
            match set(&mut config, key, value, has_battery) {
                Ok(msg) => {
                    if let Err(e) = config.save_at(&root) {
                        eprintln!("Enregistrement impossible : {e}");
                        return 1;
                    }
                    if key == "brightness" {
                        let target = config.brightness * config.profile.brightness_factor() / 100;
                        if super::set_brightness_at(&root, target).is_none() {
                            println!("{msg} (aucun r\u{e9}tro\u{e9}clairage r\u{e9}glable sur cet \u{e9}cran)");
                            refresh(&root);
                            return 0;
                        }
                    }
                    println!("{msg}");
                    refresh(&root);
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
            }
        }
        "suspend" | "hibernate" => {
            let result = if cmd == "suspend" {
                sleep::suspend_at(&root)
            } else {
                sleep::hibernate_at(&root)
            };
            refresh(&root);
            match result {
                Ok(()) => {
                    println!("Reprise effectu\u{e9}e.");
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        "hibernate-setup" => match sleep::hibernate_setup_at(&root) {
            Ok(msg) => {
                println!("{msg}");
                refresh(&root);
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}
