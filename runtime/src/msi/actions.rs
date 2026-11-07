//! Actions personnalisées EXE de `InstallExecuteSequence` (#833).
//!
//! Types pris en charge (bits 0-5 de `CustomAction.Type`) :
//! - 2 : exécutable stocké dans la table `Binary`, extrait dans un dossier
//!   temporaire puis lancé avec `Target` comme paramètres ;
//! - 18 : fichier installé par le paquet (`Source` = clé de la table File) ;
//! - 34 : `Source` = dossier de travail, `Target` = ligne de commande ;
//! - 50 : `Source` = propriété qui contient le chemin du programme.
//!
//! Drapeaux : 0x40 (code de retour ignoré), 0x80 (asynchrone : sans
//! attendre), 0x400 (différé : exécuté à sa place, sans retour arrière ;
//! les actions de retour arrière 0x100 et de validation 0x200 ne sont
//! jamais lancées). Les programmes passent par `setup::run`, comme ceux
//! des installateurs NSIS et Inno : un `.exe` est exécuté par Winlux,
//! jamais un programme Linux ni un outil système absent.
//!
//! Actions DLL de WiX reconnues sans exécuter leur code (comme les
//! greffons NSIS) : `WixQuietExec`/`CAQuietExec` (et leurs variantes 64
//! bits et « Silent ») lancent la ligne de commande rangée dans la
//! propriété du nom de l'action (`CustomActionData` pour une action
//! différée). Les autres actions DLL (1, 17) et les scripts (5, 6, 21, 22,
//! 37, 38) sont notés dans le journal.

use super::cond::Context;
use super::db::Database;
use super::install::Logger;
use crate::dlls::advapi32::RegEdit;
use crate::setup::run::{self, Outcome, Target};
use std::collections::{HashMap, HashSet};

/// Quand lancer une action par rapport à la pose des fichiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Avant `InstallFiles` (seulement ce qui ne dépend pas des fichiers).
    BeforeFiles,
    /// Après la pose des fichiers et du registre.
    AfterFiles,
    /// Désinstallation : avant le retrait des fichiers.
    Uninstall,
}

struct Action {
    kind: i32,
    source: String,
    target: String,
}

/// Points d'entrée de WiX qui exécutent une commande sans fenêtre.
const QUIET_EXEC: &[&str] = &[
    "CAQuietExec",
    "CAQuietExec64",
    "WixQuietExec",
    "WixQuietExec64",
    "WixSilentExec",
    "WixSilentExec64",
];

fn is_quiet_exec(a: &Action) -> bool {
    a.kind & 0x3F == 1 && QUIET_EXEC.contains(&a.target.as_str())
}

/// `WixShellExec` : ouvre un fichier ou lance l'application à la fin de
/// l'interface d'installation ; jamais en installation silencieuse.
fn is_shell_exec(a: &Action) -> bool {
    a.kind & 0x3F == 1 && a.target == "WixShellExec"
}

fn actions(db: &Database) -> HashMap<String, Action> {
    db.rows("CustomAction")
        .map(|r| {
            (
                r.str("Action"),
                Action {
                    kind: r.int("Type").unwrap_or(0),
                    source: r.str("Source"),
                    target: r.str("Target"),
                },
            )
        })
        .collect()
}

/// Séquence d'exécution triée : (numéro, action, condition).
fn sequence(db: &Database) -> Vec<(i32, String, String)> {
    let mut seq: Vec<(i32, String, String)> = db
        .rows("InstallExecuteSequence")
        .map(|r| {
            (
                r.int("Sequence").unwrap_or(0),
                r.str("Action"),
                r.str("Condition"),
            )
        })
        .collect();
    seq.sort();
    seq
}

/// Ce qu'une action EXE lancerait : programme, paramètres, dossier.
fn command(
    db: &Database,
    ctx: &Context,
    name: &str,
    a: &Action,
    temp: &std::path::Path,
) -> Result<(Target, String, String), String> {
    match a.kind & 0x3F {
        2 => {
            let data = db
                .streams
                .get(&format!("Binary.{}", a.source))
                .or_else(|| db.streams.get(&a.source))
                .ok_or_else(|| format!("binaire {} absent du paquet", a.source))?;
            if data.get(..2) != Some(b"MZ") {
                return Err(format!("binaire {} : pas un programme Windows", a.source));
            }
            std::fs::create_dir_all(temp).map_err(|e| e.to_string())?;
            let exe = temp.join(format!("{name}.exe"));
            std::fs::write(&exe, data).map_err(|e| e.to_string())?;
            Ok((Target::Pe(exe), ctx.format(&a.target), String::new()))
        }
        18 => {
            let file = ctx
                .files
                .get(&a.source)
                .ok_or_else(|| format!("fichier {} non installé", a.source))?;
            let dir = file
                .rsplit_once('\\')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            Ok((run::resolve(file, &dir)?, ctx.format(&a.target), dir))
        }
        34 => {
            let dir = ctx.get(&a.source);
            let line = ctx.format(&a.target);
            let p = run::parse(&line, &dir);
            Ok((run::resolve(&p.program, &dir)?, p.params, dir))
        }
        50 => {
            let exe = ctx.get(&a.source);
            if exe.is_empty() {
                return Err(format!("propriété {} vide", a.source));
            }
            Ok((
                run::resolve(&exe, "")?,
                ctx.format(&a.target),
                String::new(),
            ))
        }
        other => Err(format!("type {other} non pris en charge")),
    }
}

/// Lance les actions EXE de la phase voulue. `skip` : fichier (chemin
/// Windows) à ne pas lancer (programme de l'application qui se configure
/// lui-même, voir `self_setup_exe`). Renvoie le nombre d'actions lancées.
pub fn run_phase(
    db: &Database,
    ctx: &Context,
    phase: Phase,
    skip: Option<&str>,
    log: &mut Logger,
) -> usize {
    let actions = actions(db);
    if actions.is_empty() {
        return 0;
    }
    let seq = sequence(db);
    let files_at = seq
        .iter()
        .find(|s| s.1 == "InstallFiles")
        .map(|s| s.0)
        .unwrap_or(4000);
    let temp = std::env::temp_dir().join(format!("winlux-msi-ca-{}", std::process::id()));
    // Les affectations de propriété (51) placées après le calcul des
    // chemins préparent souvent la ligne de commande d'une action
    // différée (CustomActionData) : rejouées ici à leur place.
    let mut ctx = ctx.clone();
    let ctx = &mut ctx;
    let mut launched = 0;
    for (n, name, cond) in &seq {
        let Some(a) = actions.get(name) else {
            continue;
        };
        let base = a.kind & 0x3F;
        let in_phase = match phase {
            Phase::BeforeFiles => *n < files_at,
            Phase::AfterFiles | Phase::Uninstall => true,
        };
        if base == 51 && in_phase && ctx.eval(cond) {
            let value = ctx.format(&a.target);
            ctx.props.insert(a.source.clone(), value);
            continue;
        }
        if !matches!(base, 2 | 18 | 34 | 50) && !is_quiet_exec(a) {
            continue; // DLL et scripts notés par `unsupported`
        }
        let wanted = match phase {
            Phase::BeforeFiles => *n < files_at && base != 18,
            Phase::AfterFiles => *n >= files_at || base == 18,
            Phase::Uninstall => true,
        };
        if !wanted || !ctx.eval(cond) {
            continue;
        }
        if a.kind & 0x400 != 0 && a.kind & 0x300 != 0 {
            continue; // retour arrière ou validation
        }
        if is_quiet_exec(a) {
            let line = ctx.get(name);
            if line.trim().is_empty() {
                continue;
            }
            let (outcome, _) = run::command_line(&line, "", true, true);
            if matches!(outcome, Outcome::Exited(_)) {
                launched += 1;
            }
            log.log(&format!(
                "Action personnalisée {name} ({}) : {}",
                a.target,
                run::describe(line.trim(), &outcome)
            ));
            continue;
        }
        let (target, params, dir) = match command(db, ctx, name, a, &temp) {
            Ok(c) => c,
            Err(e) => {
                log.log(&format!("Action personnalisée {name} non lancée : {e}"));
                continue;
            }
        };
        if let (Some(s), Target::Pe(p)) = (skip, &target) {
            if crate::paths::to_linux(s) == *p {
                continue;
            }
        }
        let wait = a.kind & 0x80 == 0;
        let (outcome, _) = run::spawn(&target, &params, &dir, wait, false);
        let what = match &target {
            Target::Pe(p) => format!("{} {params}", p.display()),
            Target::Builtin(b) => format!("{b} {params}"),
        };
        match &outcome {
            Outcome::Exited(c) if *c != 0 && a.kind & 0x40 == 0 => log.log(&format!(
                "Action personnalisée {name} en échec (code {c}), installation poursuivie : {}",
                what.trim()
            )),
            Outcome::Exited(_) | Outcome::Started => {
                launched += 1;
                log.log(&format!("Action personnalisée {name} : {}", what.trim()))
            }
            other => log.log(&format!(
                "Action personnalisée {name} : {}",
                run::describe(what.trim(), other)
            )),
        }
    }
    let _ = std::fs::remove_dir_all(&temp);
    launched
}

/// Actions de la séquence ni EXE ni propriété : notées dans le journal.
pub fn unsupported(db: &Database, ctx: &Context) -> Vec<String> {
    let actions = actions(db);
    sequence(db)
        .into_iter()
        .filter_map(|(_, name, cond)| {
            let a = actions.get(&name)?;
            let base = a.kind & 0x3F;
            if matches!(base, 2 | 18 | 34 | 50 | 51 | 35) || is_quiet_exec(a) || !ctx.eval(&cond) {
                return None;
            }
            if is_shell_exec(a) {
                return Some(format!("{name} (ouverture en fin d'installation)"));
            }
            let what = match base {
                1 | 17 => "DLL",
                5 | 6 | 21 | 22 | 37 | 38 => "script",
                19 => "erreur",
                7 | 23 | 39 => "installation imbriquée",
                _ => "autre",
            };
            Some(format!("{name} ({what}, type {})", a.kind))
        })
        .collect()
}

/// Services Windows déclarés par le paquet (`ServiceInstall`) : inscrits
/// sous `HKLM\SYSTEM\CurrentControlSet\Services\<Nom>`, là où le
/// gestionnaire de services de Winlux les lit (`advapi32_services.rs`), et
/// retirés avec l'application. Le démarrage demandé par `ServiceControl`
/// est laissé au gestionnaire de services (journalisé).
pub fn services(
    db: &Database,
    ctx: &Context,
    comps: &HashSet<String>,
    edits: &mut Vec<RegEdit>,
    keys: &mut Vec<String>,
    values: &mut Vec<(String, String)>,
    log: &mut Logger,
) {
    let keypath: HashMap<String, String> = db
        .rows("Component")
        .map(|r| (r.str("Component"), r.str("KeyPath")))
        .collect();
    for r in db.rows("ServiceInstall") {
        let comp = r.str("Component_");
        if !comps.contains(&comp) {
            continue;
        }
        let name = ctx.format(&r.str("Name"));
        let Some(exe) = keypath.get(&comp).and_then(|k| ctx.files.get(k)) else {
            log.log(&format!(
                "Service {name} : programme du composant {comp} introuvable"
            ));
            continue;
        };
        let args = ctx.format(&r.str("Arguments"));
        let image = format!("\"{exe}\" {args}").trim().to_string();
        let key = format!("hklm\\SYSTEM\\CurrentControlSet\\Services\\{name}");
        let display = ctx.format(&r.str("DisplayName"));
        let mut sz = |n: &str, v: &str| {
            let bytes: Vec<u8> = v
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|u| u.to_le_bytes())
                .collect();
            edits.push(RegEdit::Set {
                key: key.clone(),
                name: n.into(),
                kind: 1,
                bytes,
            });
            values.push((key.clone(), n.into()));
        };
        sz("ImagePath", &image);
        sz(
            "DisplayName",
            if display.is_empty() { &name } else { &display },
        );
        let description = ctx.format(&r.str("Description"));
        if !description.is_empty() {
            sz("Description", &description);
        }
        let account = ctx.format(&r.str("StartName"));
        sz(
            "ObjectName",
            if account.is_empty() {
                "LocalSystem"
            } else {
                &account
            },
        );
        for (n, v) in [
            ("Type", r.int("ServiceType").unwrap_or(0x10)),
            ("Start", r.int("StartType").unwrap_or(3)),
            ("ErrorControl", r.int("ErrorControl").unwrap_or(1)),
        ] {
            edits.push(RegEdit::Set {
                key: key.clone(),
                name: n.into(),
                kind: 4,
                bytes: (v as u32).to_le_bytes().to_vec(),
            });
            values.push((key.clone(), n.into()));
        }
        keys.push(key);
        log.log(&format!("Service inscrit : {name} ({image})"));
    }
    for r in db.rows("ServiceControl") {
        if comps.contains(&r.str("Component_")) && r.int("Event").unwrap_or(0) & 1 != 0 {
            log.log(&format!(
                "Service {} : démarrage laissé au gestionnaire de services",
                ctx.format(&r.str("Name"))
            ));
        }
    }
}
