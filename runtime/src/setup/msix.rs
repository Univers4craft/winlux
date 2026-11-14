//! Paquets MSIX et AppX (`.msix`, `.appx`, `.msixbundle`, `.appxbundle`),
//! installés par Winlux lui-même (#272).
//!
//! Un paquet est une archive ZIP : `AppxManifest.xml` décrit l'identité
//! (nom, éditeur, version, architecture), les applications et leurs
//! extensions (types de fichiers, protocoles) ; `AppxBlockMap.xml` donne
//! l'empreinte SHA-256 de chaque bloc de 64 Kio de chaque fichier, vérifiée
//! à l'extraction. Un bundle contient plusieurs paquets : on prend celui
//! de l'architecture x64 (ou neutre).
//!
//! Installation dans `C:\Program Files\WindowsApps\<PackageFullName>`,
//! raccourcis du menu Démarrer, associations de fichiers et de protocoles,
//! dossier de données `%LOCALAPPDATA%\Packages\<PackageFamilyName>`, et
//! entrée dans « Applications installées ». Un programme lancé depuis ce
//! dossier reçoit son identité de paquet (`GetCurrentPackageFullName`…).
//!
//! Signature (`AppxSignature.p7x`, voir `appx_signature`) : un paquet non
//! signé, ou dont la chaîne de certificats n'est pas encore vérifiée (#235),
//! n'est installé qu'après confirmation explicite (`--allow-unverified`) ;
//! un certificat qui ne porte pas l'éditeur du manifeste est refusé.

use super::appx_signature::{self, Signature};
use super::{Installer, Kind, Options};
use crate::archive::zip::{self, ZipEntry};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const BLOCK: usize = 64 * 1024;

/// Extensions reconnues.
pub fn is_package_name(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "msix" | "appx" | "msixbundle" | "appxbundle"
        )
    })
}

/// Le fichier est-il un paquet MSIX/AppX (ou un bundle) ?
pub fn is_package(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let Ok(entries) = zip::list(&mut f) else {
        return false;
    };
    entries.iter().any(|e| {
        e.name.eq_ignore_ascii_case("AppxManifest.xml")
            || e.name
                .eq_ignore_ascii_case("AppxMetadata/AppxBundleManifest.xml")
    })
}

// ============================================================
//  XML minimal (manifestes MSIX)
// ============================================================

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Balises ouvrantes `<prefixe:Nom …>` dont le nom local est `name`.
fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = xml[i..].find('<') {
        let start = i + p;
        let Some(end) = xml[start..].find('>').map(|e| start + e + 1) else {
            break;
        };
        let tag = &xml[start..end];
        let head = tag[1..]
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");
        let local = head.rsplit(':').next().unwrap_or(head);
        if local == name && !head.starts_with('/') {
            out.push(tag);
        }
        i = end;
    }
    out
}

/// Valeur d'un attribut d'une balise (sans préfixe d'espace de noms).
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(p) = rest.find(name) {
        let before = rest[..p].chars().last();
        let after = &rest[p + name.len()..];
        let after_trim = after.trim_start();
        if matches!(before, Some(c) if c.is_whitespace()) && after_trim.starts_with('=') {
            let v = after_trim[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let end = v[1..].find(q)?;
                return Some(decode_entities(&v[1..1 + end]));
            }
        }
        rest = &rest[p + name.len()..];
    }
    None
}

/// Texte du premier élément `name` (nom local).
fn text(xml: &str, name: &str) -> Option<String> {
    let tag = tags(xml, name).into_iter().next()?;
    if tag.ends_with("/>") {
        return None;
    }
    let start = xml.find(tag)? + tag.len();
    let end = xml[start..].find("</")? + start;
    Some(decode_entities(xml[start..end].trim()))
}

/// Blocs `<…Nom …> … </…Nom>` (nom local), pour les éléments répétés.
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut from = 0;
    for tag in tags(xml, name) {
        let Some(p) = xml[from..].find(tag) else {
            continue;
        };
        let start = from + p;
        if tag.ends_with("/>") {
            out.push(tag);
            from = start + tag.len();
            continue;
        }
        let close = format!(":{name}>");
        let plain = format!("</{name}>");
        let end = xml[start..]
            .match_indices("</")
            .map(|(i, _)| start + i)
            .find(|&i| {
                xml[i..].starts_with(&plain)
                    || xml[i..]
                        .split('>')
                        .next()
                        .is_some_and(|t| format!("{t}>").ends_with(&close))
            })
            .unwrap_or(xml.len());
        out.push(&xml[start..end]);
        from = end;
    }
    out
}

// ============================================================
//  Identité
// ============================================================

#[derive(Debug, Default, Clone)]
pub struct App {
    pub id: String,
    pub executable: String,
    pub display_name: String,
    pub hidden: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Package {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub arch: String,
    pub resource_id: String,
    pub display_name: String,
    pub publisher_display: String,
    pub apps: Vec<App>,
    /// (nom de l'association, extensions, exécutable).
    pub file_types: Vec<(String, Vec<String>, String)>,
    /// (protocole, exécutable).
    pub protocols: Vec<(String, String)>,
}

/// Identifiant d'éditeur : 8 premiers octets du SHA-256 de l'éditeur en
/// UTF-16LE, en base 32 de Crockford (13 caractères).
pub fn publisher_id(publisher: &str) -> String {
    let utf16: Vec<u8> = publisher
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let hash = crate::dlls::bcrypt::sha256(&utf16);
    let mut bits = u128::from_be_bytes([
        hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7], 0, 0, 0, 0, 0, 0,
        0, 0,
    ]);
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut out = String::new();
    for _ in 0..13 {
        out.push(ALPHABET[(bits >> 123) as usize & 31] as char);
        bits <<= 5;
    }
    out
}

/// Nom d'une ressource « ms-resource: » illisible sans le fichier .pri.
fn readable(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && !s.starts_with("ms-resource:")).then(|| s.to_string())
}

impl Package {
    pub fn parse(xml: &str) -> Result<Package, String> {
        let id = tags(xml, "Identity")
            .into_iter()
            .next()
            .ok_or("AppxManifest.xml sans élément Identity")?;
        let mut p = Package {
            name: attr(id, "Name").ok_or("Identity sans Name")?,
            publisher: attr(id, "Publisher").ok_or("Identity sans Publisher")?,
            version: attr(id, "Version").unwrap_or_else(|| "1.0.0.0".into()),
            arch: attr(id, "ProcessorArchitecture")
                .unwrap_or_else(|| "neutral".into())
                .to_ascii_lowercase(),
            resource_id: attr(id, "ResourceId").unwrap_or_default(),
            ..Default::default()
        };
        p.display_name = text(xml, "DisplayName")
            .as_deref()
            .and_then(readable)
            .unwrap_or_else(|| p.name.clone());
        p.publisher_display = text(xml, "PublisherDisplayName")
            .as_deref()
            .and_then(readable)
            .unwrap_or_default();
        for block in blocks(xml, "Application") {
            let head = tags(block, "Application").into_iter().next().unwrap_or("");
            let Some(exe) = attr(head, "Executable") else {
                continue;
            };
            let exe = exe.replace('/', "\\");
            let visual = tags(block, "VisualElements")
                .into_iter()
                .next()
                .unwrap_or("");
            let app = App {
                id: attr(head, "Id").unwrap_or_else(|| "App".into()),
                display_name: attr(visual, "DisplayName")
                    .as_deref()
                    .and_then(readable)
                    .unwrap_or_else(|| p.display_name.clone()),
                hidden: attr(visual, "AppListEntry")
                    .is_some_and(|v| v.eq_ignore_ascii_case("none")),
                executable: exe.clone(),
            };
            for fta in blocks(block, "FileTypeAssociation") {
                let head = tags(fta, "FileTypeAssociation")
                    .into_iter()
                    .next()
                    .unwrap_or("");
                let name = attr(head, "Name").unwrap_or_else(|| app.id.to_lowercase());
                let exts: Vec<String> = blocks(fta, "FileType")
                    .iter()
                    .filter_map(|b| {
                        let t = b.find('>').map(|i| &b[i + 1..]).unwrap_or("");
                        let t = t
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase();
                        t.starts_with('.').then_some(t)
                    })
                    .collect();
                if !exts.is_empty() {
                    p.file_types.push((name, exts, exe.clone()));
                }
            }
            for proto in tags(block, "Protocol") {
                if let Some(name) = attr(proto, "Name") {
                    p.protocols.push((name.to_ascii_lowercase(), exe.clone()));
                }
            }
            p.apps.push(app);
        }
        Ok(p)
    }

    pub fn publisher_id(&self) -> String {
        publisher_id(&self.publisher)
    }

    pub fn full_name(&self) -> String {
        format!(
            "{}_{}_{}_{}_{}",
            self.name,
            self.version,
            self.arch,
            self.resource_id,
            self.publisher_id()
        )
    }

    pub fn family_name(&self) -> String {
        format!("{}_{}", self.name, self.publisher_id())
    }
}

// ============================================================
//  Block map
// ============================================================

/// Empreintes attendues par fichier (nom du block map, avec `\`).
fn block_map(xml: &str) -> Vec<(String, u64, Vec<Vec<u8>>)> {
    blocks(xml, "File")
        .into_iter()
        .filter_map(|b| {
            let head = tags(b, "File").into_iter().next()?;
            let name = attr(head, "Name")?;
            let size = attr(head, "Size").and_then(|s| s.parse().ok()).unwrap_or(0);
            let hashes = tags(b, "Block")
                .into_iter()
                .filter_map(|t| attr(t, "Hash"))
                .filter_map(|h| crate::imaging::base64(&h))
                .collect();
            Some((name.replace('/', "\\").to_ascii_lowercase(), size, hashes))
        })
        .collect()
}

/// Nom d'entrée ZIP d'un paquet (encodé en %XX) en chemin relatif.
pub fn decode_name(name: &str) -> String {
    let b = name.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&name[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).replace('/', "\\")
}

/// Fichiers d'empreinte du conteneur, absents du block map :
/// `AppxMetadata\CodeIntegrity.cat` en fait partie, protégé par le condensat
/// `AXCI` de la signature, pas par le block map (#1725).
fn is_container_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace('/', "\\");
    n == "appxblockmap.xml"
        || n == "appxsignature.p7x"
        || n == "[content_types].xml"
        || n == "appxmetadata\\codeintegrity.cat"
}

fn read_entry(f: &mut File, e: &ZipEntry) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(e.usize as usize);
    zip::open_entry(f, e, None)
        .map_err(|err| format!("{} : {err}", e.name))?
        .read_to_end(&mut data)
        .map_err(|err| format!("{} : {err}", e.name))?;
    Ok(data)
}

fn find<'a>(entries: &'a [ZipEntry], name: &str) -> Option<&'a ZipEntry> {
    entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
}

// ============================================================
//  Bundle
// ============================================================

/// Choisit le paquet d'application d'un bundle : x64, puis neutre.
fn bundle_choice(xml: &str) -> Result<String, String> {
    let mut best: Option<(u8, String)> = None;
    let mut seen = Vec::new();
    for tag in tags(xml, "Package") {
        let kind = attr(tag, "Type").unwrap_or_else(|| "application".into());
        if !kind.eq_ignore_ascii_case("application") {
            continue;
        }
        let arch = attr(tag, "Architecture")
            .unwrap_or_else(|| "neutral".into())
            .to_ascii_lowercase();
        let Some(file) = attr(tag, "FileName") else {
            continue;
        };
        seen.push(arch.clone());
        let rank = match arch.as_str() {
            "x64" => 2,
            "neutral" => 1,
            _ => 0,
        };
        if rank > 0 && best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, file));
        }
    }
    best.map(|(_, f)| f).ok_or_else(|| {
        format!(
            "aucun paquet x64 dans ce bundle (architectures : {}) ; les programmes 32 bits ne sont pas encore pris en charge (#232)",
            if seen.is_empty() { "aucune".into() } else { seen.join(", ") }
        )
    })
}

// ============================================================
//  Lecture et installation
// ============================================================

/// Ouvre le paquet (ou le paquet x64 d'un bundle) : chemin du paquet
/// d'application à lire, et fichier temporaire à effacer après.
fn open_app_package(
    path: &Path,
) -> Result<(std::path::PathBuf, Option<std::path::PathBuf>), String> {
    let mut f = File::open(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let entries = zip::list(&mut f).map_err(|e| format!("{} : {e}", path.display()))?;
    if find(&entries, "AppxManifest.xml").is_some() {
        return Ok((path.to_path_buf(), None));
    }
    let bundle = find(&entries, "AppxMetadata/AppxBundleManifest.xml")
        .ok_or("ce fichier n'est pas un paquet MSIX ou AppX")?;
    let xml = String::from_utf8_lossy(&read_entry(&mut f, bundle)?).into_owned();
    let chosen = bundle_choice(&xml)?;
    let inner = entries
        .iter()
        .find(|e| decode_name(&e.name).eq_ignore_ascii_case(&chosen.replace('/', "\\")))
        .ok_or_else(|| format!("{chosen} annoncé par le bundle mais absent"))?;
    let tmp = crate::paths::profile_temp().join(format!(
        "winlux-msix-{}-{}.msix",
        std::process::id(),
        chosen.replace(['/', '\\'], "_")
    ));
    let data = read_entry(&mut f, inner)?;
    std::fs::write(&tmp, data).map_err(|e| format!("{} : {e}", tmp.display()))?;
    Ok((tmp.clone(), Some(tmp)))
}

fn read_package(path: &Path) -> Result<Package, String> {
    let (app, tmp) = open_app_package(path)?;
    let result = (|| {
        let mut f = File::open(&app).map_err(|e| e.to_string())?;
        let entries = zip::list(&mut f).map_err(|e| e.to_string())?;
        let manifest = find(&entries, "AppxManifest.xml").ok_or("AppxManifest.xml absent")?;
        Package::parse(&String::from_utf8_lossy(&read_entry(&mut f, manifest)?))
    })();
    if let Some(t) = tmp {
        let _ = std::fs::remove_file(t);
    }
    result
}

/// Résumé pour `--info`.
pub fn info(path: &Path) -> Result<super::Info, String> {
    let p = read_package(path)?;
    Ok(super::Info {
        kind: Kind::Msix,
        name: p.display_name.clone(),
        version: p.version.clone(),
        publisher: p.publisher_display.clone(),
        default_dir: format!("C:\\Program Files\\WindowsApps\\{}", p.full_name()),
    })
}

/// Signature du paquet tel qu'il a été reçu (celle du bundle pour un
/// bundle : les paquets qu'il contient ne sont pas signés à part).
pub fn signature(path: &Path) -> Result<Signature, String> {
    let pkg = read_package(path)?;
    Ok(appx_signature::examine(path, &pkg.publisher))
}

pub fn install(path: &Path, opts: &Options, inst: &mut Installer) -> Result<(), String> {
    // Comme Windows, pas d'installation silencieuse d'un paquet dont
    // l'éditeur n'est pas vérifié : refus, sauf confirmation explicite.
    let sig = signature(path)?;
    let publisher = read_package(path)?.publisher_display;
    match &sig {
        Signature::Invalid(e) => return Err(format!("signature refusée : {e}")),
        Signature::Absent | Signature::Unverified { .. } if !opts.allow_unverified => {
            return Err(appx_signature::warning(&sig, &publisher))
        }
        Signature::Absent => inst
            .log
            .log("paquet non signé, installé sur confirmation de l'utilisateur"),
        Signature::Unverified { signer, reason } => inst.log.log(&format!(
            "paquet signé au nom de « {signer} », non vérifié ({reason}), installé sur confirmation de l'utilisateur"
        )),
        Signature::Verified { signer } => inst
            .log
            .log(&format!("paquet signé par « {signer} », signature et chaîne vérifiées")),
    }
    let (app, tmp) = open_app_package(path)?;
    let result = install_app_package(&app, inst);
    if let Some(t) = tmp {
        let _ = std::fs::remove_file(t);
    }
    result
}

fn install_app_package(path: &Path, inst: &mut Installer) -> Result<(), String> {
    let mut f = File::open(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let entries = zip::list(&mut f).map_err(|e| format!("{} : {e}", path.display()))?;
    let manifest = find(&entries, "AppxManifest.xml").ok_or("AppxManifest.xml absent")?;
    let pkg = Package::parse(&String::from_utf8_lossy(&read_entry(&mut f, manifest)?))?;
    if !matches!(pkg.arch.as_str(), "x64" | "neutral") {
        return Err(format!(
            "paquet pour l'architecture {} : seuls x64 et neutre sont pris en charge (#232)",
            pkg.arch
        ));
    }
    let map_entry =
        find(&entries, "AppxBlockMap.xml").ok_or("AppxBlockMap.xml absent : paquet incomplet")?;
    let map = block_map(&String::from_utf8_lossy(&read_entry(&mut f, map_entry)?));
    inst.log.log("empreintes du block map vérifiées");
    let full = pkg.full_name();
    let family = pkg.family_name();
    let dir = format!("C:\\Program Files\\WindowsApps\\{full}");
    inst.manifest.id = full.clone();
    inst.manifest.name = pkg.display_name.clone();
    inst.manifest.version = pkg.version.clone();
    inst.manifest.publisher = pkg.publisher_display.clone();
    inst.manifest.location = dir.clone();
    // Vérification complète avant d'écrire quoi que ce soit : un paquet
    // altéré ne laisse aucun fichier derrière lui.
    // Les fichiers listés par le block map doivent tous être présents.
    for (name, _, _) in &map {
        if !entries
            .iter()
            .any(|e| decode_name(&e.name).to_ascii_lowercase() == *name)
        {
            return Err(format!(
                "{name} annoncé par le block map mais absent : paquet altéré"
            ));
        }
    }
    let wanted = |e: &&ZipEntry| {
        let lower = decode_name(&e.name).to_ascii_lowercase();
        !e.is_dir() && lower != "[content_types].xml" && lower != "appxsignature.p7x"
    };
    for e in entries.iter().filter(wanted) {
        let rel = decode_name(&e.name);
        if is_container_file(&rel) {
            continue;
        }
        let lower = rel.to_ascii_lowercase();
        let Some((_, size, hashes)) = map.iter().find(|(n, _, _)| *n == lower) else {
            return Err(format!("{rel} absent du block map : paquet altéré"));
        };
        let data = read_entry(&mut f, e)?;
        let ok = *size == data.len() as u64
            && hashes.len() == data.len().div_ceil(BLOCK)
            && data
                .chunks(BLOCK)
                .zip(hashes)
                .all(|(chunk, h)| crate::dlls::bcrypt::sha256(chunk).as_slice() == h.as_slice());
        if !ok {
            return Err(format!("empreinte de {rel} incorrecte : paquet altéré"));
        }
    }
    inst.mkdirs_win(&dir).map_err(|e| format!("{dir} : {e}"))?;
    for e in entries.iter().filter(wanted) {
        let rel = decode_name(&e.name);
        let data = read_entry(&mut f, e)?;
        inst.write_file(&format!("{dir}\\{rel}"), &data)
            .map_err(|err| format!("{rel} : {err}"))?;
    }
    // Dossiers de données du paquet (ApplicationData).
    let data_root = format!("C:\\AppData\\Local\\Packages\\{family}");
    for sub in [
        "LocalState",
        "RoamingState",
        "TempState",
        "LocalCache",
        "Settings",
    ] {
        inst.mkdirs_win(&format!("{data_root}\\{sub}"))
            .map_err(|e| format!("{data_root} : {e}"))?;
    }
    // Menu Démarrer.
    for app in pkg.apps.iter().filter(|a| !a.hidden) {
        let target = format!("{dir}\\{}", app.executable);
        let link = format!(
            "{}\\{}.lnk",
            super::Folders::programs(),
            app.display_name
                .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        inst.shortcut(&link, &target, "", &dir, &app.display_name)
            .map_err(|e| format!("{link} : {e}"))?;
    }
    // Associations de fichiers et de protocoles.
    for (name, exts, exe) in &pkg.file_types {
        let progid = format!("AppX.{}.{}", pkg.name, name).to_lowercase();
        let key = format!("hklm\\software\\classes\\{progid}");
        inst.reg_str(&key, "", &format!("{} ({name})", pkg.display_name));
        inst.reg_str(
            &format!("{key}\\shell\\open\\command"),
            "",
            &format!("\"{dir}\\{exe}\" \"%1\""),
        );
        inst.manifest.trees.push(key);
        for ext in exts {
            let ext_key = format!("hklm\\software\\classes\\{ext}");
            inst.reg_set(
                &format!("{ext_key}\\OpenWithProgids"),
                &progid,
                0,
                Vec::new(),
                true,
            );
            if crate::dlls::advapi32::read_registry_value_internal(&ext_key, "").is_none() {
                inst.reg_set(&ext_key, "", 1, super::utf16z(&progid), true);
            }
        }
    }
    for (proto, exe) in &pkg.protocols {
        let key = format!("hklm\\software\\classes\\{proto}");
        inst.reg_str(&key, "", &format!("URL:{proto}"));
        inst.reg_str(&key, "URL Protocol", "");
        inst.reg_str(
            &format!("{key}\\shell\\open\\command"),
            "",
            &format!("\"{dir}\\{exe}\" \"%1\""),
        );
        inst.manifest.trees.push(key);
    }
    let key = format!(
        "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{}",
        full.to_lowercase()
    );
    inst.manifest.uninstall_key = key.clone();
    inst.reg_str(&key, "DisplayName", &pkg.display_name);
    inst.reg_str(&key, "DisplayVersion", &pkg.version);
    if !pkg.publisher_display.is_empty() {
        inst.reg_str(&key, "Publisher", &pkg.publisher_display);
    }
    inst.reg_str(&key, "InstallLocation", &dir);
    inst.reg_str(
        &key,
        "UninstallString",
        &format!("winlux --setup --uninstall \"{full}\""),
    );
    inst.reg_str(&key, "PackageFullName", &full);
    inst.reg_dword(&key, "NoModify", 1);
    inst.reg_dword(&key, "NoRepair", 1);
    if let Some(app) = pkg.apps.first() {
        inst.reg_str(&key, "DisplayIcon", &format!("{dir}\\{}", app.executable));
    }
    Ok(())
}

// ============================================================
//  Identité à l'exécution
// ============================================================

/// Identité du paquet installé qui contient `exe` (chemin Linux) :
/// (paquet, dossier Windows du paquet).
pub fn package_of(exe: &Path) -> Option<(Package, String)> {
    let win = crate::paths::to_windows(exe);
    let lower = win.to_ascii_lowercase();
    let marker = "\\program files\\windowsapps\\";
    let at = lower.find(marker)? + marker.len();
    let full = win[at..].split('\\').next()?;
    let dir = format!("{}{full}", &win[..at]);
    let manifest = crate::paths::to_linux(&format!("{dir}\\AppxManifest.xml"));
    let pkg = Package::parse(&std::fs::read_to_string(manifest).ok()?).ok()?;
    pkg.full_name()
        .eq_ignore_ascii_case(full)
        .then_some((pkg, dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
  <Identity Name="Winlux.Bloc" Publisher="CN=Winlux Test" Version="1.2.3.0" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Bloc &amp; notes</DisplayName>
    <PublisherDisplayName>Winlux</PublisherDisplayName>
  </Properties>
  <Applications>
    <Application Id="Bloc" Executable="bin/bloc.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Bloc" Description="x" />
      <Extensions>
        <uap:Extension Category="windows.fileTypeAssociation">
          <uap:FileTypeAssociation Name="texte">
            <uap:SupportedFileTypes>
              <uap:FileType>.wltxt</uap:FileType>
              <uap:FileType ContentType="text/plain">.WLNOTE</uap:FileType>
            </uap:SupportedFileTypes>
          </uap:FileTypeAssociation>
        </uap:Extension>
        <uap:Extension Category="windows.protocol">
          <uap:Protocol Name="winlux-bloc" />
        </uap:Extension>
      </Extensions>
    </Application>
    <Application Id="Aide" Executable="aide.exe">
      <uap:VisualElements DisplayName="ms-resource:Aide" AppListEntry="none" />
    </Application>
  </Applications>
</Package>"#;

    #[test]
    fn identifiant_editeur_microsoft() {
        assert_eq!(
            publisher_id(
                "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US"
            ),
            "8wekyb3d8bbwe"
        );
    }

    #[test]
    fn lit_le_manifeste() {
        let p = Package::parse(MANIFEST).unwrap();
        assert_eq!(p.name, "Winlux.Bloc");
        assert_eq!(p.display_name, "Bloc & notes");
        assert_eq!(p.publisher_display, "Winlux");
        assert_eq!(p.arch, "x64");
        assert_eq!(p.apps.len(), 2);
        assert_eq!(p.apps[0].executable, "bin\\bloc.exe");
        assert!(!p.apps[0].hidden && p.apps[1].hidden);
        assert_eq!(p.apps[1].display_name, "Bloc & notes");
        assert_eq!(p.file_types.len(), 1);
        assert_eq!(p.file_types[0].1, vec![".wltxt", ".wlnote"]);
        assert_eq!(
            p.protocols,
            vec![("winlux-bloc".into(), "bin\\bloc.exe".into())]
        );
        assert!(p.full_name().starts_with("Winlux.Bloc_1.2.3.0_x64__"));
        assert_eq!(p.family_name().len(), "Winlux.Bloc_".len() + 13);
    }

    #[test]
    fn choisit_x64_dans_un_bundle() {
        let xml = r#"<Bundle><Packages>
            <Package Type="application" Architecture="x86" FileName="a_x86.msix"/>
            <Package Type="resource" ResourceId="fr" FileName="fr.msix"/>
            <Package Type="application" Architecture="x64" FileName="a_x64.msix"/>
        </Packages></Bundle>"#;
        assert_eq!(bundle_choice(xml).unwrap(), "a_x64.msix");
        let only32 = r#"<Package Type="application" Architecture="x86" FileName="a.msix"/>"#;
        assert!(bundle_choice(only32).unwrap_err().contains("32 bits"));
    }

    #[test]
    fn catalogue_d_integrite_hors_du_block_map() {
        // #1725 : fichier d'empreinte, comme la signature et le block map.
        assert!(is_container_file("AppxMetadata\\CodeIntegrity.cat"));
        assert!(is_container_file("AppxMetadata/CodeIntegrity.cat"));
        assert!(!is_container_file("AppxMetadata\\Autre.cat"));
        assert!(!is_container_file("AppxManifest.xml"));
    }

    #[test]
    fn noms_encodes() {
        assert_eq!(
            decode_name("Assets/Logo%20grand.png"),
            "Assets\\Logo grand.png"
        );
        assert_eq!(decode_name("caf%C3%A9.txt"), "café.txt");
    }
}
