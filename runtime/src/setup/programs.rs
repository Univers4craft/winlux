//! « Applications installées » : liste des clés `Uninstall` du registre
//! (paquets msi, installateurs NSIS/Inno, programmes qui s'inscrivent
//! eux-mêmes) et désinstallation de l'une d'elles.
//!
//! `winlux --setup --programs [--out FICHIER]` écrit une ligne par
//! application ; `winlux --setup --remove CLÉ [--out FICHIER]` la
//! désinstalle avec le bon outil (msiexec intégré, journal d'installation,
//! ou désinstalleur de l'application) puis écrit `message=…`.

use super::{host_path, remove, Logger, Manifest};
use std::path::Path;

const ROOTS: [&str; 3] = [
    "hklm\\software\\microsoft\\windows\\currentversion\\uninstall",
    "hklm\\software\\wow6432node\\microsoft\\windows\\currentversion\\uninstall",
    "hkcu\\software\\microsoft\\windows\\currentversion\\uninstall",
];

#[derive(Debug, Default, Clone)]
pub struct Program {
    /// Chemin complet de la clé `Uninstall\…`.
    pub key: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// Taille estimée en Kio (0 : inconnue).
    pub size_kb: u32,
    /// AAAAMMJJ, vide si inconnue.
    pub date: String,
    /// msi, nsis, inno, setup ou exe.
    pub kind: String,
    pub uninstall: String,
    pub quiet_uninstall: String,
    pub msi: bool,
}

fn string(d: &crate::registry::Data, key: &str, name: &str) -> String {
    d.get(key, name)
        .map(|v| {
            if crate::registry::is_string(v.kind) {
                crate::registry::string_of(&v.bytes)
            } else if v.bytes.len() >= 4 {
                u32::from_le_bytes([v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3]]).to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn dword(d: &crate::registry::Data, key: &str, name: &str) -> u32 {
    match d.get(key, name) {
        Some(v) if v.kind == 4 && v.bytes.len() >= 4 => {
            u32::from_le_bytes([v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3]])
        }
        Some(v) if crate::registry::is_string(v.kind) => crate::registry::string_of(&v.bytes)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

/// Code produit `{…}` si la chaîne en contient un.
fn product_code(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let end = start + s[start..].find('}')?;
    let code = &s[start..=end];
    (code.len() == 38).then(|| code.to_string())
}

/// Toutes les applications inscrites, triées par nom.
pub fn list() -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    let _ = crate::registry::read(|d| {
        for root in ROOTS {
            let Some(root) = crate::registry::canonical(root) else {
                continue;
            };
            if !d.exists(&root) {
                continue;
            }
            for sub in d.subkeys(&root) {
                let key = crate::registry::join(&root, &sub);
                let name = string(d, &key, "DisplayName");
                if name.is_empty()
                    || dword(d, &key, "SystemComponent") == 1
                    || !string(d, &key, "ParentKeyName").is_empty()
                {
                    continue;
                }
                let uninstall = string(d, &key, "UninstallString");
                let msi = dword(d, &key, "WindowsInstaller") == 1
                    || uninstall.to_ascii_lowercase().contains("msiexec");
                if uninstall.is_empty() && !msi {
                    continue;
                }
                let lower = sub.to_ascii_lowercase();
                let kind = if msi {
                    "msi"
                } else if let Some(m) = Manifest::find(&sub) {
                    if m.kind.is_empty() {
                        "setup"
                    } else if m.kind == "nsis" {
                        "nsis"
                    } else {
                        "inno"
                    }
                } else if lower.ends_with("_is1") {
                    "inno"
                } else {
                    "exe"
                };
                // Même application inscrite deux fois (32 et 64 bits) : une
                // seule ligne.
                if out.iter().any(|p| p.name == name && p.key.ends_with(&sub)) {
                    continue;
                }
                out.push(Program {
                    name,
                    version: string(d, &key, "DisplayVersion"),
                    publisher: string(d, &key, "Publisher"),
                    size_kb: dword(d, &key, "EstimatedSize"),
                    date: string(d, &key, "InstallDate"),
                    kind: kind.to_string(),
                    quiet_uninstall: string(d, &key, "QuietUninstallString"),
                    uninstall,
                    msi,
                    key,
                });
            }
        }
        Ok(())
    });
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Découpe une ligne de commande Windows (guillemets).
fn split_command(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            ' ' | '\t' if !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            _ => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Programme sans guillemets dont le chemin contient des espaces :
/// `C:\Program Files\App\uninst.exe /S`.
fn split_unquoted_exe(s: &str) -> Option<(String, Vec<String>)> {
    let lower = s.to_ascii_lowercase();
    let end = lower.find(".exe")? + 4;
    let exe = s[..end].trim().to_string();
    Some((exe, split_command(&s[end..])))
}

/// Désinstalle l'application inscrite sous `key`. Renvoie (code, message).
pub fn uninstall_key(key: &str) -> (i32, String) {
    let Some(key) = crate::registry::canonical(key) else {
        return (1, format!("clé invalide : {key}"));
    };
    let Some(p) = list()
        .into_iter()
        .find(|p| p.key.eq_ignore_ascii_case(&key))
    else {
        return (1, "Cette application n'est plus installée.".into());
    };
    let sub = key.rsplit('\\').next().unwrap_or_default().to_string();
    let mut log = Logger { quiet: true };
    // Installée par Winlux (NSIS/Inno) : journal d'installation.
    if let Some(m) = Manifest::find(&sub) {
        remove(&m, &mut log);
        return (0, format!("{} a été désinstallé.", p.name));
    }
    // Paquet Windows Installer : msiexec intégré.
    if p.msi {
        let code = product_code(&sub).or_else(|| product_code(&p.uninstall));
        let Some(code) = code else {
            return (1, format!("Code produit introuvable pour {}.", p.name));
        };
        let rc = crate::msi::cli(&["/x".into(), code, "/qn".into()]);
        return if rc == 0 {
            (0, format!("{} a été désinstallé.", p.name))
        } else {
            (
                rc,
                format!("La désinstallation de {} a échoué ({rc}).", p.name),
            )
        };
    }
    // Inscription laissée par `winlux --setup` (sans désinstalleur).
    let command = if p.quiet_uninstall.is_empty() {
        p.uninstall.clone()
    } else {
        p.quiet_uninstall.clone()
    };
    let parts = split_command(&command);
    if parts.len() >= 4
        && parts[0].eq_ignore_ascii_case("winlux")
        && parts[1] == "--setup"
        && parts[2] == "--uninstall"
    {
        let mut log = Logger { quiet: true };
        let rc = super::uninstall(&parts[3], &mut log);
        return (rc, format!("{} a été désinstallé.", p.name));
    }
    // Désinstalleur de l'application : lancé par Winlux.
    let (exe, args) = if command.trim_start().starts_with('"') {
        let mut parts = parts;
        if parts.is_empty() {
            return (1, format!("Aucun désinstalleur pour {}.", p.name));
        }
        let exe = parts.remove(0);
        (exe, parts)
    } else {
        match split_unquoted_exe(&command) {
            Some(v) => v,
            None => return (1, format!("Désinstalleur non reconnu : {command}")),
        }
    };
    let host = host_path(&exe);
    if !host.is_file() {
        return (
            1,
            format!("Le désinstalleur de {} est introuvable : {exe}", p.name),
        );
    }
    if let Some(rc) = super::launch(&host, &args) {
        return (rc, format!("{} a été désinstallé.", p.name));
    }
    let me = std::env::current_exe().unwrap_or_else(|_| "winlux".into());
    match std::process::Command::new(me)
        .arg(&host)
        .args(&args)
        .status()
    {
        Ok(s) if s.success() => (0, format!("{} a été désinstallé.", p.name)),
        Ok(s) => (
            s.code().unwrap_or(1),
            format!("Le désinstalleur de {} s'est terminé en erreur.", p.name),
        ),
        Err(e) => (1, format!("Lancement impossible du désinstalleur : {e}")),
    }
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

pub fn format_list(list: &[Program]) -> String {
    let mut s = String::new();
    for p in list {
        s.push_str(&format!(
            "app={}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            clean(&p.key),
            clean(&p.name),
            clean(&p.version),
            clean(&p.publisher),
            p.size_kb,
            clean(&p.date),
            p.kind
        ));
    }
    s.push_str("done=1\n");
    s
}

/// Écrit `text` dans `out` (chemin Linux ou Windows) en une fois.
pub fn write_out(out: &str, text: &str) {
    let path = host_path(out);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, Path::new(&path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(
            split_command("\"C:\\Program Files\\App\\unins000.exe\" /SILENT"),
            vec!["C:\\Program Files\\App\\unins000.exe", "/SILENT"]
        );
        assert_eq!(
            split_command("winlux --setup --uninstall \"Mon appli\""),
            vec!["winlux", "--setup", "--uninstall", "Mon appli"]
        );
        let (exe, args) = split_unquoted_exe("C:\\Program Files\\App\\uninst.exe /S").unwrap();
        assert_eq!(exe, "C:\\Program Files\\App\\uninst.exe");
        assert_eq!(args, vec!["/S"]);
        assert_eq!(
            product_code("MsiExec.exe /X{12345678-1234-1234-1234-123456789ABC}").as_deref(),
            Some("{12345678-1234-1234-1234-123456789ABC}")
        );
    }
}
