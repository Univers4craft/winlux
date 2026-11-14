//! msiexec intégré à Winlux : installe, désinstalle et extrait les paquets
//! Windows Installer (.msi) sans composant tiers.
//!
//! `winlux msiexec.exe /i paquet.msi /qn INSTALLDIR=C:\App` (ou
//! `winlux --msiexec …`) accepte les options usuelles de msiexec.

pub mod actions;
pub(crate) mod cab;
mod cfb;
mod cond;
mod db;
pub mod extra;
pub(crate) mod install;
pub use install::{civil, shortcut_bytes};
mod lzx;
pub mod script;
pub mod session;
pub mod session_db;
pub mod summary;

use install::{Logger, Options};
use std::path::{Path, PathBuf};

const ERROR_INVALID_COMMAND_LINE: i32 = 1639;

const USAGE: &str = "Winlux msiexec
  /i paquet.msi        installer (aussi /package)
  /x paquet.msi|{code} désinstaller (aussi /uninstall)
  /a paquet.msi        installation administrative (TARGETDIR=dossier)
  /p correctif.msp     refusé : correctifs .msp pas encore pris en charge
  /q, /qn, /qb, /passive, /quiet   sans interface
  /l*v journal.txt     écrire le journal
  PROPRIETE=valeur     propriété publique (INSTALLDIR=…, ADDLOCAL=ALL…)
  --list               paquets installés par msiexec
  --info paquet.msi    code produit, nom et version du paquet";

/// Chemin donné en argument : chemin Linux existant ou chemin Windows.
fn host_path(arg: &str) -> PathBuf {
    let p = Path::new(arg);
    if p.exists() {
        return p.to_path_buf();
    }
    crate::paths::to_linux(arg)
}

enum Mode {
    Install(String),
    Admin(String),
    Uninstall(String),
    Patch(String),
}

/// Erreur Windows Installer : correctif impossible à traiter.
const ERROR_PATCH_PACKAGE_UNSUPPORTED: i32 = 1637;

/// Correctifs (.msp) et transformations (.mst) ne sont pas encore appliqués
/// (#1715) : on le dit dans le journal au lieu de les ignorer en silence,
/// et l'installation se poursuit avec la version de base.
fn refuse_patches(props: &[(String, String)], log: &mut install::Logger) {
    for (k, v) in props {
        if k.eq_ignore_ascii_case("PATCH") && !v.is_empty() {
            log.log(&format!(
                "correctif .msp non pris en charge, installation de la version de base : {v} (#1715)"
            ));
        } else if k.eq_ignore_ascii_case("TRANSFORMS") && !v.is_empty() {
            log.log(&format!(
                "transformation .mst non prise en charge, ignorée : {v} (#1715)"
            ));
        }
    }
}

pub fn cli(args: &[String]) -> i32 {
    let mut mode = None;
    let mut opts = Options::default();
    let mut log_file = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let lower = a.to_ascii_lowercase();
        let next = |i: &mut usize| -> Option<String> {
            *i += 1;
            args.get(*i).cloned()
        };
        if lower == "--info" {
            // Code produit, nom et version d'un paquet (utilisé par le Store).
            let Some(path) = args.get(i + 1) else {
                return ERROR_INVALID_COMMAND_LINE;
            };
            return match install::package_info(&host_path(path)) {
                Ok((code, name, version)) => {
                    println!("{code}\t{name}\t{version}");
                    0
                }
                Err(code) => code,
            };
        }
        if lower == "--list" {
            for m in install::Manifest::all() {
                println!("{}\t{}\t{}", m.product, m.name, m.version);
            }
            return 0;
        }
        if lower.starts_with('/') || lower.starts_with('-') {
            let opt = &lower[1..];
            match opt {
                "i" | "package" | "f" | "fa" | "fomus" | "famus" => {
                    mode = next(&mut i).map(Mode::Install)
                }
                "a" => mode = next(&mut i).map(Mode::Admin),
                "p" | "update" => mode = next(&mut i).map(Mode::Patch),
                "x" | "uninstall" => mode = next(&mut i).map(Mode::Uninstall),
                "quiet" | "passive" => opts.quiet = true,
                "norestart" | "forcerestart" | "promptrestart" => {}
                "?" | "h" | "help" => {
                    println!("{USAGE}");
                    return 0;
                }
                _ if opt.starts_with('q') => opts.quiet = true,
                _ if opt.starts_with('l') || opt == "log" => log_file = next(&mut i),
                _ if opt.starts_with('x') && a.len() > 2 => {
                    mode = Some(Mode::Uninstall(a[2..].to_string()))
                }
                _ if opt.starts_with('i') && a.len() > 2 => {
                    mode = Some(Mode::Install(a[2..].to_string()))
                }
                _ => {
                    eprintln!("msiexec : option inconnue {a}\n{USAGE}");
                    return ERROR_INVALID_COMMAND_LINE;
                }
            }
        } else if let Some((k, v)) = a.split_once('=') {
            if k.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                && !k.is_empty()
            {
                opts.props
                    .push((k.to_string(), v.trim_matches('"').to_string()));
            } else {
                eprintln!("msiexec : argument inattendu {a}");
                return ERROR_INVALID_COMMAND_LINE;
            }
        } else if mode.is_none() && a.to_ascii_lowercase().ends_with(".msi") {
            mode = Some(Mode::Install(a.clone())); // msiexec paquet.msi
        } else {
            eprintln!("msiexec : argument inattendu {a}\n{USAGE}");
            return ERROR_INVALID_COMMAND_LINE;
        }
        i += 1;
    }
    let mut log = Logger {
        file: log_file.and_then(|f| std::fs::File::create(host_path(&f)).ok()),
        quiet: false,
    };
    match mode {
        None => {
            eprintln!("{USAGE}");
            ERROR_INVALID_COMMAND_LINE
        }
        Some(Mode::Install(p)) | Some(Mode::Admin(p))
            if crate::anticheat::check_launch(&host_path(&p)).is_some() =>
        {
            crate::anticheat::REFUSED
        }
        Some(Mode::Install(p)) | Some(Mode::Admin(p))
            if crate::setup::elevation::require(&host_path(&p), &opts.props).is_err() =>
        {
            // Élévation refusée (#2180) : rien n'est installé.
            crate::uac::CANCELLED
        }
        Some(Mode::Install(p)) => {
            // Sans /q ni /passive, msiexec montre la boîte de licence : la
            // page du contrat s'ouvre, pas d'installation sans acceptation (#2123).
            let flags: Vec<String> = if opts.quiet {
                vec!["/q".into()]
            } else {
                Vec::new()
            };
            if let Some(code) = crate::setup::license::require_acceptance(&host_path(&p), &flags) {
                return code;
            }
            refuse_patches(&opts.props, &mut log);
            install::install(&host_path(&p), &opts, &mut log)
        }
        Some(Mode::Patch(p)) => {
            log.log(&format!(
                "correctif .msp non pris en charge, rien n'est modifié : {p} (#1715)"
            ));
            ERROR_PATCH_PACKAGE_UNSUPPORTED
        }
        Some(Mode::Admin(p)) => {
            refuse_patches(&opts.props, &mut log);
            opts.admin = true;
            install::install(&host_path(&p), &opts, &mut log)
        }
        Some(Mode::Uninstall(target)) => {
            let code = if target.starts_with('{') {
                target
            } else {
                match install::product_of(&host_path(&target)) {
                    Ok(code) => code,
                    Err(code) => return code,
                }
            };
            install::uninstall(&code, &mut log)
        }
    }
}

/// Vrai si `winlux` est invoqué sur msiexec.exe (qui n'existe pas en tant
/// que fichier PE : c'est ce module qui le remplace).
pub fn is_msiexec(exe: &Path) -> bool {
    let text = exe.to_string_lossy();
    let name = text.rsplit(['/', '\\']).next().unwrap_or("");
    name.eq_ignore_ascii_case("msiexec.exe") || name.eq_ignore_ascii_case("msiexec")
}

#[cfg(test)]
mod tests;
