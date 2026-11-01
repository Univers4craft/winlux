//! Tables Windows Installer complémentaires : recherche de l'existant
//! (`AppSearch` avec `RegLocator`, `IniLocator`, `DrLocator`), fichiers
//! `.ini` (`IniFile`), variables d'environnement (`Environment`),
//! suppressions de fichiers et de registre (`RemoveFile`, `RemoveRegistry`).

use super::cond::Context;
use super::db::Database;
use super::install::{hive, Logger, Manifest};
use crate::dlls::advapi32::{read_registry_value_internal, RegEdit};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Valeur de registre lue comme le fait `RegLocator` de type « brut » :
/// chaîne telle quelle, entier préfixé de `#`, binaire en `#x…`.
fn raw_value(key: &str, name: &str) -> Option<(u32, Vec<u8>, String)> {
    let bytes = read_registry_value_internal(key, name)?;
    let kind = crate::registry::read(|d| {
        Ok(crate::registry::canonical(key)
            .and_then(|k| d.get(&k, name).map(|v| v.kind))
            .unwrap_or(1))
    })
    .unwrap_or(1);
    let text = match kind {
        1 | 2 => crate::registry::string_of(&bytes),
        4 if bytes.len() >= 4 => format!(
            "#{}",
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        ),
        7 => crate::registry::multi_of(&bytes).join("[~]"),
        _ => {
            let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
            format!("#x{hex}")
        }
    };
    Some((kind, bytes, text))
}

/// Lit `[section] key=` dans un fichier .ini (champ `field`, 0 = tout).
fn ini_read(path: &Path, section: &str, key: &str, field: i32) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            inside = l
                .trim_start_matches('[')
                .trim_end_matches(']')
                .eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.trim();
                    return Some(if field > 0 {
                        v.split(',')
                            .nth(field as usize - 1)
                            .unwrap_or("")
                            .trim()
                            .to_string()
                    } else {
                        v.to_string()
                    });
                }
            }
        }
    }
    None
}

/// Chemin trouvé, vérifié selon le type de recherche (0 dossier, 1 fichier).
fn checked_path(path: &str, kind: i32, file_name: Option<&str>) -> Option<String> {
    let mut p = path.trim().trim_matches('"').to_string();
    if p.is_empty() {
        return None;
    }
    match kind & 0x0F {
        0 => {
            // Dossier : la valeur peut désigner un fichier, on garde son dossier.
            let host = crate::paths::to_linux(&p);
            if host.is_file() {
                if let Some(i) = p.rfind('\\') {
                    p.truncate(i);
                }
            } else if !host.is_dir() {
                return None;
            }
            if !p.ends_with('\\') {
                p.push('\\');
            }
            Some(p)
        }
        1 => {
            if let Some(name) = file_name {
                if crate::paths::to_linux(&p).is_dir() {
                    if !p.ends_with('\\') {
                        p.push('\\');
                    }
                    p.push_str(name);
                }
            }
            crate::paths::to_linux(&p).is_file().then_some(p)
        }
        _ => Some(p),
    }
}

/// Recherche dans un dossier (DrLocator, profondeur limitée).
fn search_dir(dir: &Path, name: Option<&str>, depth: i32) -> Option<PathBuf> {
    match name {
        None => dir.is_dir().then(|| dir.to_path_buf()),
        Some(n) => {
            let direct = dir.join(n);
            if direct.exists() {
                return Some(direct);
            }
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let fname = e.file_name().to_string_lossy().to_string();
                    if fname.eq_ignore_ascii_case(n) {
                        return Some(e.path());
                    }
                }
            }
            if depth > 0 {
                let rd = std::fs::read_dir(dir).ok()?;
                for e in rd.flatten() {
                    if e.path().is_dir() {
                        if let Some(f) = search_dir(&e.path(), name, depth - 1) {
                            return Some(f);
                        }
                    }
                }
            }
            None
        }
    }
}

fn long_name(s: &str) -> &str {
    s.split('|').nth(1).unwrap_or(s)
}

/// Action `AppSearch` : renseigne des propriétés à partir du registre, de
/// fichiers .ini ou du disque (ancienne installation, dossier choisi…).
pub fn app_search(db: &Database, ctx: &mut Context, all_users: bool, log: &mut Logger) {
    let signatures: Vec<(String, String)> = db
        .rows("Signature")
        .map(|r| {
            (
                r.str("Signature"),
                long_name(&r.str("FileName")).to_string(),
            )
        })
        .collect();
    let file_of = |sig: &str| -> Option<String> {
        signatures
            .iter()
            .find(|(s, _)| s == sig)
            .map(|(_, f)| f.clone())
    };
    let searches: Vec<(String, String)> = db
        .rows("AppSearch")
        .map(|r| (r.str("Property"), r.str("Signature_")))
        .collect();
    for (prop, sig) in searches {
        let file = file_of(&sig);
        let mut found: Option<String> = None;
        if let Some(r) = db.rows("RegLocator").find(|r| r.str("Signature_") == sig) {
            let key = format!(
                "{}\\{}",
                hive(r.int("Root").unwrap_or(-1), all_users),
                ctx.format(&r.str("Key")).trim_matches('\\')
            );
            let name = ctx.format(&r.str("Name"));
            let kind = r.int("Type").unwrap_or(0);
            if let Some((_, _, text)) = raw_value(&key, &name) {
                found = if kind & 0x0F == 2 {
                    Some(text)
                } else {
                    checked_path(&text, kind, file.as_deref())
                };
            }
        } else if let Some(r) = db.rows("IniLocator").find(|r| r.str("Signature_") == sig) {
            let ini = long_name(&r.str("FileName")).to_string();
            let path = crate::paths::to_linux(&format!("{}{ini}", ctx.get("WindowsFolder")));
            let kind = r.int("Type").unwrap_or(0);
            if let Some(v) = ini_read(
                &path,
                &ctx.format(&r.str("Section")),
                &ctx.format(&r.str("Key")),
                r.int("Field").unwrap_or(0),
            ) {
                found = if kind & 0x0F == 2 {
                    Some(v)
                } else {
                    checked_path(&v, kind, file.as_deref())
                };
            }
        } else if let Some(r) = db.rows("DrLocator").find(|r| r.str("Signature_") == sig) {
            let parent = r.str("Parent");
            let base = if parent.is_empty() {
                ctx.format(&r.str("Path"))
            } else {
                let p = ctx.get(&parent);
                format!("{}{}", p, ctx.format(&r.str("Path")))
            };
            if !base.is_empty() {
                let host = crate::paths::to_linux(&base);
                let depth = r.int("Depth").unwrap_or(0).clamp(0, 4);
                if let Some(hit) = search_dir(&host, file.as_deref(), depth) {
                    let mut w = crate::paths::to_windows(&hit);
                    if hit.is_dir() && !w.ends_with('\\') {
                        w.push('\\');
                    }
                    found = Some(w);
                }
            }
        } else if db.rows("CompLocator").any(|r| r.str("Signature_") == sig) {
            log.log(&format!(
                "AppSearch {prop} : recherche de composant ignorée"
            ));
        }
        if let Some(v) = found.filter(|v| !v.is_empty()) {
            log.log(&format!("AppSearch : {prop} = {v}"));
            ctx.props.insert(prop, v);
        }
    }
}

// ── Fichiers .ini ────────────────────────────────────────────────────

/// Modifie `[section] key` d'un fichier .ini. `mode` : 0 ajoute ou remplace,
/// 1 crée seulement si absente, 3 ajoute une valeur à une liste (`a,b`),
/// 4 retire la ligne, 5 retire une valeur de la liste.
pub fn ini_edit(path: &Path, section: &str, key: &str, value: &str, mode: i32) -> bool {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = |l: &str| {
        let t = l.trim();
        t.starts_with('[')
            && t.trim_start_matches('[')
                .trim_end_matches(']')
                .eq_ignore_ascii_case(section)
    };
    let start = lines.iter().position(|l| header(l));
    let entry = |v: &str| format!("{key}={v}");
    let (start, end) = match start {
        Some(s) => {
            let e = lines[s + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map_or(lines.len(), |e| s + 1 + e);
            (s, e)
        }
        None => {
            if mode >= 4 {
                return false;
            }
            lines.push(format!("[{section}]"));
            (lines.len() - 1, lines.len())
        }
    };
    let found = lines[start + 1..end].iter().position(|l| {
        l.split_once('=')
            .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
    });
    let current = found.and_then(|f| {
        lines[start + 1 + f]
            .split_once('=')
            .map(|(_, v)| v.trim().to_string())
    });
    match (mode, found) {
        (0, Some(f)) => lines[start + 1 + f] = entry(value),
        (0 | 1 | 3, None) => lines.insert(end, entry(value)),
        (1, Some(_)) => return false,
        (3, Some(f)) => {
            let cur = current.unwrap_or_default();
            if cur.split(',').any(|p| p.trim().eq_ignore_ascii_case(value)) {
                return false;
            }
            let joined = if cur.is_empty() {
                value.to_string()
            } else {
                format!("{cur},{value}")
            };
            lines[start + 1 + f] = entry(&joined);
        }
        (4, Some(f)) => {
            lines.remove(start + 1 + f);
        }
        (5, Some(f)) => {
            let rest: Vec<&str> = current
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|p| !p.trim().eq_ignore_ascii_case(value))
                .collect();
            if rest.is_empty() || rest.iter().all(|p| p.trim().is_empty()) {
                lines.remove(start + 1 + f);
            } else {
                lines[start + 1 + f] = entry(&rest.join(","));
            }
        }
        _ => return false,
    }
    // Section vidée : on la retire aussi.
    if mode >= 4 {
        let s = lines.iter().position(|l| header(l));
        if let Some(s) = s {
            let e = lines[s + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map_or(lines.len(), |e| s + 1 + e);
            if lines[s + 1..e].iter().all(|l| l.trim().is_empty()) {
                lines.drain(s..e);
            }
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = lines.join("\r\n");
    if !out.is_empty() {
        out.push_str("\r\n");
    }
    std::fs::write(path, out).is_ok()
}

/// Table `IniFile` : écrit les entrées et note comment les retirer.
pub fn ini_files(
    db: &Database,
    ctx: &Context,
    comps: &HashSet<String>,
    manifest: &mut Manifest,
    created_dirs: &mut Vec<PathBuf>,
) {
    for r in db.rows("IniFile") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let name = long_name(&r.str("FileName")).to_string();
        let dir_prop = r.str("DirProperty");
        let dir = if dir_prop.is_empty() {
            ctx.get("WindowsFolder")
        } else {
            let d = ctx.get(&dir_prop);
            if d.is_empty() {
                ctx.get("WindowsFolder")
            } else {
                d
            }
        };
        let dir = if dir.ends_with('\\') {
            dir
        } else {
            format!("{dir}\\")
        };
        let path = crate::paths::to_linux(&format!("{dir}{name}"));
        let section = ctx.format(&r.str("Section"));
        let key = ctx.format(&r.str("Key"));
        let value = ctx.format(&r.str("Value"));
        let mode = r.int("Action").unwrap_or(0);
        if let Some(parent) = path.parent() {
            let mut missing = Vec::new();
            let mut p = parent.to_path_buf();
            while !p.exists() {
                missing.push(p.clone());
                if !p.pop() {
                    break;
                }
            }
            created_dirs.extend(missing.into_iter().rev());
        }
        let existed = path.exists();
        if !ini_edit(&path, &section, &key, &value, mode) {
            continue;
        }
        if !existed {
            manifest.files.push(path);
        } else {
            // Retrait à la désinstallation : la ligne, ou la valeur ajoutée.
            let undo = if mode == 3 { 5 } else { 4 };
            manifest.inis.push((path, section, key, value, undo));
        }
    }
}

// ── Variables d'environnement ────────────────────────────────────────

const USER_ENV: &str = "hkcu\\environment";
const SYSTEM_ENV: &str = "hklm\\system\\currentcontrolset\\control\\session manager\\environment";

/// Table `Environment` : variables écrites dans le registre (comme Windows),
/// avec ajout en tête ou en queue d'une liste (`[~];valeur`).
pub fn environment(
    db: &Database,
    ctx: &Context,
    comps: &HashSet<String>,
    edits: &mut Vec<RegEdit>,
    manifest: &mut Manifest,
) {
    for r in db.rows("Environment") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let raw_name = r.str("Name");
        let flags: String = raw_name
            .chars()
            .take_while(|c| matches!(c, '=' | '+' | '-' | '!' | '*'))
            .collect();
        let name = raw_name[flags.len()..].to_string();
        if name.is_empty() {
            continue;
        }
        let key = if flags.contains('*') {
            SYSTEM_ENV
        } else {
            USER_ENV
        };
        let previous = raw_value(key, &name);
        let before = previous.as_ref().map(|p| p.2.clone());
        if flags.contains('!') {
            // Variable retirée à l'installation.
            if let Some((kind, bytes, _)) = previous {
                edits.push(RegEdit::DeleteValue {
                    key: key.into(),
                    name: name.clone(),
                });
                manifest
                    .restores
                    .push((key.into(), name, Some((kind, bytes))));
            }
            continue;
        }
        // `[~]` marque la valeur existante : repéré avant le formatage, qui
        // le transformerait en caractère nul.
        let raw = r.get("Value").text();
        let value = if let Some(rest) = raw.strip_prefix("[~]") {
            format!("[~]{}", ctx.format(rest))
        } else if let Some(rest) = raw.strip_suffix("[~]") {
            format!("{}[~]", ctx.format(rest))
        } else {
            ctx.format(&raw)
        };
        let (new, appended) = if let Some(rest) = value.strip_prefix("[~]") {
            // [~];valeur : ajout en queue.
            let sep = rest.chars().next().unwrap_or(';');
            let part = rest.trim_start_matches(sep);
            match &before {
                Some(b) if !b.is_empty() => (format!("{b}{sep}{part}"), true),
                _ => (part.to_string(), false),
            }
        } else if let Some(rest) = value.strip_suffix("[~]") {
            // valeur;[~] : ajout en tête.
            let sep = rest.chars().last().unwrap_or(';');
            let part = rest.trim_end_matches(sep);
            match &before {
                Some(b) if !b.is_empty() => (format!("{part}{sep}{b}"), true),
                _ => (part.to_string(), false),
            }
        } else {
            (value.clone(), false)
        };
        if flags.contains('+') && before.is_some() && !appended {
            continue;
        }
        let kind = if new.contains('%') { 2 } else { 1 };
        edits.push(RegEdit::Set {
            key: key.into(),
            name: name.clone(),
            kind,
            bytes: utf16z(&new),
        });
        match previous {
            Some((k, b, _)) => manifest.restores.push((key.into(), name, Some((k, b)))),
            None => manifest.values.push((key.into(), name)),
        }
    }
}

// ── Suppressions ─────────────────────────────────────────────────────

/// Table `RemoveFile` : suppressions à l'installation (mode 1 ou 3) et
/// notées pour la désinstallation (mode 2 ou 3).
pub fn remove_files(
    db: &Database,
    ctx: &Context,
    comps: &HashSet<String>,
    manifest: &mut Manifest,
) {
    for r in db.rows("RemoveFile") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let dir = ctx.get(&r.str("DirProperty"));
        if dir.is_empty() {
            continue;
        }
        let dir = if dir.ends_with('\\') {
            dir
        } else {
            format!("{dir}\\")
        };
        let name = long_name(&r.str("FileName")).to_string();
        let target = if name.is_empty() {
            dir.trim_end_matches('\\').to_string()
        } else {
            format!("{dir}{name}")
        };
        let kind = if name.is_empty() { 2 } else { 0 };
        let mode = r.int("InstallMode").unwrap_or(1);
        if mode & 1 != 0 {
            crate::setup::delete_pattern(&target, kind);
        }
        if mode & 2 != 0 {
            manifest.later.push((target, kind));
        }
    }
}

/// Table `RemoveRegistry` : valeurs ou clés retirées à l'installation.
pub fn remove_registry(
    db: &Database,
    ctx: &Context,
    comps: &HashSet<String>,
    all_users: bool,
    edits: &mut Vec<RegEdit>,
) {
    for r in db.rows("RemoveRegistry") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let key = format!(
            "{}\\{}",
            hive(r.int("Root").unwrap_or(-1), all_users),
            ctx.format(&r.str("Key")).trim_matches('\\')
        )
        .to_lowercase();
        let name = ctx.format(&r.str("Name"));
        if name == "-" {
            edits.push(RegEdit::DeleteTree(key));
        } else {
            edits.push(RegEdit::DeleteValue { key, name });
        }
    }
}

/// Ce qui s'annule à la désinstallation, en plus des fichiers et clés.
pub fn undo(m: &Manifest) -> Vec<RegEdit> {
    for (path, section, key, value, mode) in &m.inis {
        ini_edit(path, section, key, value, *mode);
    }
    for (target, kind) in &m.later {
        crate::setup::delete_pattern(target, *kind);
    }
    m.restores
        .iter()
        .map(|(key, name, prev)| match prev {
            Some((kind, bytes)) => RegEdit::Set {
                key: key.clone(),
                name: name.clone(),
                kind: *kind,
                bytes: bytes.clone(),
            },
            None => RegEdit::DeleteValue {
                key: key.clone(),
                name: name.clone(),
            },
        })
        .collect()
}

/// Variables d'environnement Windows (registre) visibles par les .exe :
/// celles du système puis de l'utilisateur, sans écraser l'environnement
/// Linux déjà présent (PATH, HOME…).
pub fn apply_registry_environment() {
    let mut vars: Vec<(String, String)> = Vec::new();
    let _ = crate::registry::read(|d| {
        for key in [SYSTEM_ENV, USER_ENV] {
            let Some(k) = crate::registry::canonical(key) else {
                continue;
            };
            for v in d.values(&k) {
                if crate::registry::is_string(v.kind) && !v.name.is_empty() {
                    let text = crate::registry::string_of(&v.bytes);
                    let text = if v.kind == 2 {
                        crate::registry::expand(&text)
                    } else {
                        text
                    };
                    vars.push((v.name.clone(), text));
                }
            }
        }
        Ok(())
    });
    for (name, value) in vars {
        if name.eq_ignore_ascii_case("path") || name.contains('=') || name.contains('\0') {
            continue;
        }
        if std::env::var_os(&name).is_none() {
            std::env::set_var(&name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini() {
        let dir = std::env::temp_dir().join(format!("winlux-ini-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("app.ini");
        let _ = std::fs::remove_file(&f);
        assert!(ini_edit(&f, "Général", "Langue", "fr", 0));
        assert!(!ini_edit(&f, "Général", "Langue", "en", 1));
        assert!(ini_edit(&f, "Général", "Modules", "a", 3));
        assert!(ini_edit(&f, "Général", "Modules", "b", 3));
        assert_eq!(
            ini_read(&f, "général", "modules", 0).as_deref(),
            Some("a,b")
        );
        assert_eq!(ini_read(&f, "Général", "Modules", 2).as_deref(), Some("b"));
        assert!(ini_edit(&f, "Général", "Modules", "b", 5));
        assert_eq!(ini_read(&f, "Général", "Modules", 0).as_deref(), Some("a"));
        assert!(ini_edit(&f, "Général", "Modules", "a", 5));
        assert!(ini_edit(&f, "Général", "Langue", "", 4));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
