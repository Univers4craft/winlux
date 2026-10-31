//! msiexec intégré à Winlux : installe, désinstalle et extrait les paquets
//! Windows Installer (.msi) sans composant tiers.
//!
//! `winlux msiexec.exe /i paquet.msi /qn INSTALLDIR=C:\App` (ou
//! `winlux --msiexec …`) accepte les options usuelles de msiexec.

mod cab;
mod cfb;
mod cond;
mod db;
mod install;
pub use install::{civil, shortcut_bytes};
mod lzx;

use install::{Logger, Options};
use std::path::{Path, PathBuf};

const ERROR_INVALID_COMMAND_LINE: i32 = 1639;

const USAGE: &str = "Winlux msiexec
  /i paquet.msi        installer (aussi /package)
  /x paquet.msi|{code} désinstaller (aussi /uninstall)
  /a paquet.msi        installation administrative (TARGETDIR=dossier)
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
        Some(Mode::Install(p)) => install::install(&host_path(&p), &opts, &mut log),
        Some(Mode::Admin(p)) => {
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
