//! Installateurs WiX Burn (« bundles ») : vc_redist.x64.exe, les
//! redistribuables .NET/DirectX et beaucoup d'installeurs d'applications.
//!
//! Un bundle est un `.exe` avec une section `.wixburn` qui décrit des
//! « conteneurs » collés à la fin du fichier, chacun un cabinet (CAB) :
//! - le conteneur 0 (interface) contient le manifeste `BurnManifest.xml`,
//!   sous le nom `0` ;
//! - les suivants contiennent les paquets (MSI, cabinets externes, `.exe`).
//!
//! Winlux n'exécute pas le moteur Burn : il lit le manifeste et installe la
//! chaîne lui-même. Les paquets MSI passent par le msiexec maison (les DLL
//! système vont dans `C:\Windows\System32`, où le chargeur les trouve), les
//! paquets `.exe` sont relancés par Winlux avec leur ligne de commande
//! d'installation, les mises à jour Windows (`.msu`, `.msp`) sont ignorées.

use crate::msi::cab::Cabinet;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAGIC: u32 = 0x00f1_4300;

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// En-tête `.wixburn` : taille du moteur, signature d'origine et tailles
/// des conteneurs.
#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    pub stub_size: usize,
    /// Signature Authenticode du moteur (position et taille), 0 sans
    /// signature. Dans un bundle signé (vc_redist.x64.exe), elle est
    /// rangée juste après le conteneur 0 : les suivants viennent après elle.
    pub signature_offset: usize,
    pub signature_size: usize,
    pub containers: Vec<usize>,
}

/// Cherche la section `.wixburn` d'un PE.
pub fn find_section(data: &[u8]) -> Option<Section> {
    if data.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(data, 0x3c)? as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let n = u16::from_le_bytes([*data.get(pe + 6)?, *data.get(pe + 7)?]) as usize;
    let opt = u16::from_le_bytes([*data.get(pe + 20)?, *data.get(pe + 21)?]) as usize;
    let table = pe + 24 + opt;
    for i in 0..n.min(96) {
        let s = table + i * 40;
        if data.get(s..s + 8)? != b".wixburn" {
            continue;
        }
        let raw = u32_at(data, s + 20)? as usize;
        if u32_at(data, raw)? != MAGIC {
            return None;
        }
        let stub_size = u32_at(data, raw + 24)? as usize;
        let signature_offset = u32_at(data, raw + 32)? as usize;
        let signature_size = u32_at(data, raw + 36)? as usize;
        let count = u32_at(data, raw + 44)? as usize;
        if count > 64 {
            return None;
        }
        let containers = (0..count)
            .map(|k| u32_at(data, raw + 48 + 4 * k).map(|v| v as usize))
            .collect::<Option<Vec<_>>>()?;
        return Some(Section {
            stub_size,
            signature_offset,
            signature_size,
            containers,
        });
    }
    None
}

/// Données du conteneur `index` (0 : interface et manifeste).
pub fn container<'a>(data: &'a [u8], s: &Section, index: usize) -> Option<&'a [u8]> {
    let mut start = s.stub_size + s.containers.get(..index)?.iter().sum::<usize>();
    let first_end = s.stub_size + *s.containers.first()?;
    if index > 0 && s.signature_size > 0 && s.signature_offset >= first_end {
        start += s.signature_offset + s.signature_size - first_end;
    }
    data.get(start..start + *s.containers.get(index)?)
}

// ---------- Mini-lecteur XML (suffisant pour BurnManifest.xml) ----------

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub attrs: HashMap<String, String>,
    /// Profondeur dans l'arbre (0 : racine).
    pub depth: usize,
}

impl Element {
    pub fn attr(&self, k: &str) -> &str {
        self.attrs.get(k).map(String::as_str).unwrap_or("")
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Éléments ouvrants dans l'ordre du document, avec leur profondeur.
pub fn elements(xml: &str) -> Vec<Element> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        if rest.starts_with('?') || rest.starts_with('!') {
            match rest.find('>') {
                Some(gt) => rest = &rest[gt + 1..],
                None => break,
            }
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        if let Some(_close) = tag.strip_prefix('/') {
            depth = depth.saturating_sub(1);
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let mut attrs = HashMap::new();
        let mut a = &tag[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim().to_string();
            let after = a[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let body = &after[1..];
            let Some(end) = body.find(q) else { break };
            attrs.insert(key, unescape(&body[..end]));
            a = &body[end + 1..];
        }
        out.push(Element {
            name: tag[..name_end].to_string(),
            attrs,
            depth,
        });
        if !self_closing {
            depth += 1;
        }
    }
    out
}

// ---------- Manifeste ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageKind {
    Msi,
    Exe,
    Msu,
    Msp,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub kind: PackageKind,
    pub id: String,
    pub product_code: String,
    pub install_args: String,
    pub uninstall_args: String,
    /// `InstallCondition` du paquet (vide : toujours installé).
    pub condition: String,
    pub payloads: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Payload {
    pub file_path: String,
    pub source_path: String,
}

#[derive(Debug, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Conteneur → index de pièce jointe.
    pub containers: HashMap<String, usize>,
    pub payloads: HashMap<String, Payload>,
    pub chain: Vec<Package>,
    /// Variables remplies par une `RegistrySearch` → nom de la valeur lue.
    pub registry_searches: HashMap<String, String>,
}

pub fn parse_manifest(xml: &str) -> Manifest {
    let mut m = Manifest::default();
    let els = elements(xml);
    let mut current: Option<(usize, Package)> = None;
    for e in &els {
        if let Some((depth, _)) = &current {
            if e.depth <= *depth {
                m.chain.push(current.take().unwrap().1);
            }
        }
        match e.name.as_str() {
            "Registration" => {
                if m.version.is_empty() {
                    m.version = e.attr("Version").to_string();
                }
            }
            "Arp" => m.name = e.attr("DisplayName").to_string(),
            "RegistrySearch" => {
                m.registry_searches
                    .insert(e.attr("Variable").to_string(), e.attr("Value").to_string());
            }
            "Container" => {
                if let Ok(i) = e.attr("AttachedIndex").parse() {
                    m.containers.insert(e.attr("Id").to_string(), i);
                }
            }
            "Payload" => {
                m.payloads.insert(
                    e.attr("Id").to_string(),
                    Payload {
                        file_path: e.attr("FilePath").to_string(),
                        source_path: e.attr("SourcePath").to_string(),
                    },
                );
            }
            "MsiPackage" | "ExePackage" | "MsuPackage" | "MspPackage" => {
                let kind = match e.name.as_str() {
                    "MsiPackage" => PackageKind::Msi,
                    "ExePackage" => PackageKind::Exe,
                    "MsuPackage" => PackageKind::Msu,
                    _ => PackageKind::Msp,
                };
                current = Some((
                    e.depth,
                    Package {
                        kind,
                        id: e.attr("Id").to_string(),
                        product_code: e.attr("ProductCode").to_string(),
                        install_args: e.attr("InstallArguments").to_string(),
                        uninstall_args: e.attr("UninstallArguments").to_string(),
                        condition: e.attr("InstallCondition").to_string(),
                        payloads: Vec::new(),
                    },
                ));
            }
            "PayloadRef" => {
                if let Some((_, p)) = current.as_mut() {
                    p.payloads.push(e.attr("Id").to_string());
                }
            }
            _ => {}
        }
    }
    if let Some((_, p)) = current {
        m.chain.push(p);
    }
    m
}

// ---------- Conditions d'installation ----------
//
// Sous-ensemble de la syntaxe des conditions Burn : variables, chaînes
// entre guillemets, versions `v10.0`, nombres, `= <> < > <= >=`, `NOT`,
// `AND`, `OR` et parenthèses. Winlux se présente comme un Windows 10 64 bits
// sur x64 : vc_redist n'installe ainsi ni ses paquets ARM64 ni ses mises à
// jour pour Windows 7/8.

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Str(String),
    Version(Vec<u64>),
    Num(i64),
    Empty,
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty(),
            Value::Version(v) => v.iter().any(|n| *n != 0),
            Value::Num(n) => *n != 0,
            Value::Empty => false,
        }
    }
}

fn parse_version(s: &str) -> Option<Vec<u64>> {
    s.split('.').map(|p| p.parse().ok()).collect()
}

fn variable(m: &Manifest, name: &str) -> Value {
    match name {
        "VersionNT" | "VersionNT64" => Value::Version(vec![10, 0, 0, 0]),
        "NativeMachine" => Value::Num(0x8664),
        "ProcessorArchitecture" => Value::Num(9),
        "Privileged" | "AdminUser" => Value::Num(1),
        _ => match m.registry_searches.get(name).map(String::as_str) {
            Some(v) if v.eq_ignore_ascii_case("PROCESSOR_ARCHITECTURE") => {
                Value::Str("AMD64".into())
            }
            _ => Value::Empty,
        },
    }
}

fn tokens(s: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut t = String::from("\"");
            loop {
                match chars.next()? {
                    '"' => break,
                    ch => t.push(ch),
                }
            }
            out.push(t);
        } else if "()".contains(c) {
            out.push(chars.next()?.to_string());
        } else if "=<>~".contains(c) {
            let mut t = String::new();
            while let Some(&ch) = chars.peek() {
                if !"=<>~".contains(ch) {
                    break;
                }
                t.push(ch);
                chars.next();
            }
            out.push(t);
        } else {
            let mut t = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || "()=<>~\"".contains(ch) {
                    break;
                }
                t.push(ch);
                chars.next();
            }
            out.push(t);
        }
    }
    Some(out)
}

struct Eval<'a> {
    m: &'a Manifest,
    t: Vec<String>,
    i: usize,
}

impl Eval<'_> {
    fn peek_word(&self, w: &str) -> bool {
        self.t
            .get(self.i)
            .is_some_and(|t| t.eq_ignore_ascii_case(w))
    }
    fn or(&mut self) -> Option<bool> {
        let mut v = self.and()?;
        while self.peek_word("OR") {
            self.i += 1;
            v |= self.and()?;
        }
        Some(v)
    }
    fn and(&mut self) -> Option<bool> {
        let mut v = self.not()?;
        while self.peek_word("AND") {
            self.i += 1;
            v &= self.not()?;
        }
        Some(v)
    }
    fn not(&mut self) -> Option<bool> {
        if self.peek_word("NOT") {
            self.i += 1;
            return Some(!self.not()?);
        }
        self.comparison()
    }
    fn comparison(&mut self) -> Option<bool> {
        if self.peek_word("(") {
            self.i += 1;
            let v = self.or()?;
            if !self.peek_word(")") {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let left = self.operand()?;
        let op = match self.t.get(self.i).map(String::as_str) {
            Some(o @ ("=" | "<>" | "<" | ">" | "<=" | ">=" | "~=" | "~<>")) => o.to_string(),
            _ => return Some(left.truthy()),
        };
        self.i += 1;
        let right = self.operand()?;
        let ord = match (&left, &right) {
            (Value::Version(a), Value::Version(b)) => {
                let n = a.len().max(b.len());
                let pad = |v: &Vec<u64>| {
                    (0..n)
                        .map(|k| v.get(k).copied().unwrap_or(0))
                        .collect::<Vec<_>>()
                };
                pad(a).cmp(&pad(b))
            }
            (Value::Num(a), Value::Num(b)) => a.cmp(b),
            (Value::Str(a), Value::Str(b)) if op.starts_with('~') => {
                a.to_lowercase().cmp(&b.to_lowercase())
            }
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            (Value::Empty, Value::Empty) => std::cmp::Ordering::Equal,
            _ => return Some(op.ends_with("<>")),
        };
        use std::cmp::Ordering::*;
        Some(match op.trim_start_matches('~') {
            "=" => ord == Equal,
            "<>" => ord != Equal,
            "<" => ord == Less,
            ">" => ord == Greater,
            "<=" => ord != Greater,
            _ => ord != Less,
        })
    }
    fn operand(&mut self) -> Option<Value> {
        let t = self.t.get(self.i)?.clone();
        self.i += 1;
        if let Some(s) = t.strip_prefix('"') {
            return Some(Value::Str(s.to_string()));
        }
        if let Some(v) = t.strip_prefix(['v', 'V']).and_then(parse_version) {
            return Some(Value::Version(v));
        }
        if let Ok(n) = t.parse() {
            return Some(Value::Num(n));
        }
        if t.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Some(variable(self.m, &t));
        }
        None
    }
}

/// Le paquet doit-il être installé ? Une condition illisible l'installe,
/// comme avant la prise en charge des conditions.
pub fn should_install(m: &Manifest, condition: &str) -> bool {
    if condition.trim().is_empty() {
        return true;
    }
    let Some(t) = tokens(condition) else {
        return true;
    };
    let mut e = Eval { m, t, i: 0 };
    match e.or() {
        Some(v) if e.i == e.t.len() => v,
        _ => true,
    }
}

/// Bundle complet : manifeste et contenu de chaque fichier embarqué
/// (par `SourcePath`, comme `a0`, `a1`…).
pub struct Bundle {
    pub manifest: Manifest,
    pub files: HashMap<String, Vec<u8>>,
}

pub fn open(data: &[u8]) -> Result<Bundle, String> {
    let s = find_section(data).ok_or("pas un installateur WiX Burn")?;
    let ux = container(data, &s, 0).ok_or("conteneur d'interface tronqué")?;
    let mut xml = None;
    Cabinet::parse(ux)?.extract(&mut |name, bytes| {
        if name == "0" {
            xml = Some(String::from_utf8_lossy(bytes).into_owned());
        }
        Ok(())
    })?;
    let manifest = parse_manifest(&xml.ok_or("BurnManifest.xml absent")?);
    let mut files = HashMap::new();
    let mut indexes: Vec<usize> = manifest.containers.values().copied().collect();
    indexes.sort_unstable();
    indexes.dedup();
    for i in indexes {
        let Some(c) = container(data, &s, i) else {
            return Err(format!("conteneur {i} tronqué"));
        };
        Cabinet::parse(c)?.extract(&mut |name, bytes| {
            files.insert(name.to_string(), bytes.to_vec());
            Ok(())
        })?;
    }
    Ok(Bundle { manifest, files })
}

pub fn is_bundle_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() < 4096 {
        return false;
    }
    // Projeté, pas lu : lancer claude.exe (300 Mo) en faisait une copie (#997).
    crate::loader::ImageFile::open(path).is_ok_and(|d| find_section(&d).is_some())
}

/// Écrit les fichiers d'un paquet dans `dir`, chemins relatifs conservés
/// (un MSI y retrouve ses cabinets externes). Renvoie le premier fichier.
fn stage(b: &Bundle, p: &Package, dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut first = None;
    for id in &p.payloads {
        let Some(pl) = b.manifest.payloads.get(id) else {
            continue;
        };
        let Some(bytes) = b.files.get(&pl.source_path) else {
            return Err(format!(
                "{} absent du bundle (téléchargement non pris en charge)",
                pl.file_path
            ));
        };
        let rel: PathBuf = pl
            .file_path
            .split(['\\', '/'])
            .filter(|c| !c.is_empty() && *c != "..")
            .collect();
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{} : {e}", path.display()))?;
        first.get_or_insert(path);
    }
    Ok(first)
}

fn split_args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

/// Installe (ou, avec `uninstall`, désinstalle) la chaîne d'un bundle.
pub fn run(path: &Path, uninstall: bool, quiet: bool, log_file: Option<&Path>) -> i32 {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("winlux: {} : {e}", path.display());
            return 1;
        }
    };
    let bundle = match open(&data) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("winlux: {} : {e}", path.display());
            return 1;
        }
    };
    let file = log_file.and_then(|f| {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::File::create(f).ok()
    });
    let mut log = crate::msi::install::Logger { file, quiet };
    let name = if bundle.manifest.name.is_empty() {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        bundle.manifest.name.clone()
    };
    log.log(&format!(
        "{} de {name} {} (WiX Burn, {} paquet(s))",
        if uninstall {
            "Désinstallation"
        } else {
            "Installation"
        },
        bundle.manifest.version,
        bundle.manifest.chain.len()
    ));
    let work = std::env::temp_dir().join(format!("winlux-burn-{}", std::process::id()));
    let mut status = 0;
    let chain: Vec<&Package> = if uninstall {
        bundle.manifest.chain.iter().rev().collect()
    } else {
        bundle.manifest.chain.iter().collect()
    };
    for p in chain {
        if !uninstall && !should_install(&bundle.manifest, &p.condition) {
            log.log(&format!(
                "{} : condition « {} » fausse, paquet ignoré",
                p.id,
                p.condition.trim()
            ));
            continue;
        }
        let dir = work.join(&p.id);
        match p.kind {
            PackageKind::Msi if uninstall => {
                if !p.product_code.is_empty() {
                    let r = crate::msi::install::uninstall(&p.product_code, &mut log);
                    if r != 0 {
                        log.log(&format!("{} : déjà absent ({r})", p.id));
                    }
                }
            }
            PackageKind::Msi => match stage(&bundle, p, &dir) {
                Ok(Some(msi)) => {
                    let opts = crate::msi::install::Options {
                        props: Vec::new(),
                        quiet,
                        admin: false,
                    };
                    let r = crate::msi::install::install(&msi, &opts, &mut log);
                    if r != 0 {
                        status = r;
                    }
                }
                Ok(None) => log.log(&format!("{} : paquet vide, ignoré", p.id)),
                Err(e) => {
                    log.log(&format!("{} : {e}", p.id));
                    status = 1603;
                }
            },
            PackageKind::Exe => {
                if uninstall && p.uninstall_args.is_empty() {
                    continue;
                }
                match stage(&bundle, p, &dir) {
                    Ok(Some(exe)) => {
                        let args = split_args(if uninstall {
                            &p.uninstall_args
                        } else {
                            &p.install_args
                        });
                        let me =
                            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("winlux"));
                        match std::process::Command::new(me)
                            .arg(&exe)
                            .args(&args)
                            .status()
                        {
                            Ok(s) if s.success() || s.code() == Some(3010) => {}
                            Ok(s) => {
                                log.log(&format!("{} : code {}", p.id, s.code().unwrap_or(-1)))
                            }
                            Err(e) => log.log(&format!("{} : {e}", p.id)),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => log.log(&format!("{} : {e}", p.id)),
                }
            }
            PackageKind::Msu | PackageKind::Msp => {
                log.log(&format!(
                    "{} : mise à jour Windows, inutile sous Winlux",
                    p.id
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    if status == 0 {
        log.log(&format!("{name} : terminé."));
    }
    status
}

/// Lancement direct d'un bundle (double-clic, ou par un autre installeur).
/// Options reconnues : /install, /repair, /uninstall, /quiet, /passive, /norestart.
pub fn launch(exe: &Path, args: &[String]) -> Option<i32> {
    if !is_bundle_file(exe) {
        return None;
    }
    let has = |w: &str| {
        args.iter()
            .any(|a| a.trim_start_matches(['/', '-']).eq_ignore_ascii_case(w))
    };
    let quiet = has("quiet") || has("q") || has("passive") || has("silent");
    let log = log_path(exe, args, quiet);
    Some(run(exe, has("uninstall"), quiet, log.as_deref()))
}

/// Journal de l'installation : `/log <fichier>` (ou `/l`), comme Burn ;
/// en mode silencieux sans `/log`, `%TEMP%\<programme>_<date>.log`, pour
/// qu'une installation sans fenêtre laisse quand même une trace.
fn log_path(exe: &Path, args: &[String], quiet: bool) -> Option<PathBuf> {
    let at = args.iter().position(|a| {
        let w = a.trim_start_matches(['/', '-']);
        w.eq_ignore_ascii_case("log") || w.eq_ignore_ascii_case("l")
    });
    if let Some(f) = at.and_then(|i| args.get(i + 1)) {
        return Some(crate::paths::to_linux(f));
    }
    if !quiet {
        return None;
    }
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "burn".into());
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(crate::paths::to_linux("C:\\AppData\\Local\\Temp").join(format!("{stem}_{secs}.log")))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::msi::cab::tests::build_mszip;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<BurnManifest xmlns="http://schemas.microsoft.com/wix/2008/Burn">
  <Registration Id="{X}" Version="14.40.0.0"><Arp DisplayName="Redistribuable &amp; test" /></Registration>
  <Container Id="WixAttachedContainer" Attached="yes" AttachedIndex="1" FilePath="b.exe" />
  <Payload Id="msi" FilePath="packages\min\min.msi" Packaging="embedded" SourcePath="a0" Container="WixAttachedContainer" />
  <Payload Id="cab" FilePath="packages\min\cab1.cab" Packaging="embedded" SourcePath="a1" Container="WixAttachedContainer" />
  <Payload Id="kb" FilePath="kb.msu" Packaging="embedded" SourcePath="a2" Container="WixAttachedContainer" />
  <Chain>
    <MsuPackage Id="kb"><PayloadRef Id="kb" /></MsuPackage>
    <MsiPackage Id="min" ProductCode="{P}">
      <MsiProperty Id="ARPSYSTEMCOMPONENT" Value="1" />
      <PayloadRef Id="msi" />
      <PayloadRef Id="cab" />
    </MsiPackage>
    <ExePackage Id="exe" InstallArguments="/install /quiet" />
  </Chain>
</BurnManifest>"#;

    /// Bundle minimal : un PE avec une section `.wixburn`, suivi de deux cabinets.
    pub fn build_bundle(files: &[(&str, &[u8])]) -> Vec<u8> {
        build_bundle_with(MANIFEST, files)
    }

    pub fn build_bundle_with(manifest: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        let ux = build_mszip(&[("0", manifest.as_bytes())]);
        let attached = build_mszip(files);
        let mut pe = vec![0u8; 0x400];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        pe[0x94..0x96].copy_from_slice(&0xF0u16.to_le_bytes());
        let sec = 0x80 + 24 + 0xF0;
        pe[sec..sec + 8].copy_from_slice(b".wixburn");
        pe[sec + 20..sec + 24].copy_from_slice(&0x300u32.to_le_bytes());
        let h = 0x300;
        pe[h..h + 4].copy_from_slice(&MAGIC.to_le_bytes());
        pe[h + 4..h + 8].copy_from_slice(&2u32.to_le_bytes());
        pe[h + 24..h + 28].copy_from_slice(&0x400u32.to_le_bytes());
        pe[h + 40..h + 44].copy_from_slice(&1u32.to_le_bytes());
        pe[h + 44..h + 48].copy_from_slice(&2u32.to_le_bytes());
        pe[h + 48..h + 52].copy_from_slice(&(ux.len() as u32).to_le_bytes());
        pe[h + 52..h + 56].copy_from_slice(&(attached.len() as u32).to_le_bytes());
        pe.extend_from_slice(&ux);
        pe.extend_from_slice(&attached);
        pe
    }

    #[test]
    fn lit_le_manifeste() {
        let m = parse_manifest(MANIFEST);
        assert_eq!(m.name, "Redistribuable & test");
        assert_eq!(m.version, "14.40.0.0");
        assert_eq!(m.containers.get("WixAttachedContainer"), Some(&1));
        assert_eq!(m.chain.len(), 3);
        assert_eq!(m.chain[0].kind, PackageKind::Msu);
        assert_eq!(m.chain[1].kind, PackageKind::Msi);
        assert_eq!(m.chain[1].product_code, "{P}");
        assert_eq!(m.chain[1].payloads, vec!["msi", "cab"]);
        assert_eq!(m.chain[2].install_args, "/install /quiet");
        assert_eq!(m.payloads["cab"].source_path, "a1");
    }

    #[test]
    fn ouvre_un_bundle_et_place_les_fichiers() {
        let data = build_bundle(&[("a0", b"MSI"), ("a1", b"CAB"), ("a2", b"MSU")]);
        let s = find_section(&data).unwrap();
        assert_eq!(s.stub_size, 0x400);
        assert_eq!(s.containers.len(), 2);
        let b = open(&data).unwrap();
        assert_eq!(b.files["a1"], b"CAB");
        let dir = std::env::temp_dir().join(format!("winlux-burn-test-{}", std::process::id()));
        let first = stage(&b, &b.manifest.chain[1], &dir).unwrap().unwrap();
        assert!(first.ends_with("packages/min/min.msi"));
        assert_eq!(
            std::fs::read(dir.join("packages/min/cab1.cab")).unwrap(),
            b"CAB"
        );
        let _ = std::fs::remove_dir_all(dir);
        assert!(find_section(b"MZ pas un bundle").is_none());
    }

    /// Bundle signé (comme vc_redist.x64.exe) : la signature du moteur est
    /// collée après le conteneur 0, avant les paquets.
    #[test]
    fn ouvre_un_bundle_signe() {
        let mut data = build_bundle(&[("a0", b"MSI"), ("a1", b"CAB"), ("a2", b"MSU")]);
        let s = find_section(&data).unwrap();
        let after_ux = s.stub_size + s.containers[0];
        let signature_offset = (after_ux + 7) & !7;
        let mut signed = data[..after_ux].to_vec();
        signed.resize(signature_offset, 0);
        signed.extend_from_slice(&[0x30; 0x123]);
        signed.extend_from_slice(&data[after_ux..]);
        signed[0x300 + 32..0x300 + 36].copy_from_slice(&(signature_offset as u32).to_le_bytes());
        signed[0x300 + 36..0x300 + 40].copy_from_slice(&0x123u32.to_le_bytes());
        data = signed;
        let b = open(&data).unwrap();
        assert_eq!(b.files["a1"], b"CAB");
        assert_eq!(b.manifest.chain.len(), 3);
    }

    #[test]
    fn conditions_de_vc_redist() {
        let m = parse_manifest(
            r#"<BurnManifest><RegistrySearch Id="r" Variable="Arm64_Check" Root="HKLM" Key="k" Value="PROCESSOR_ARCHITECTURE" /></BurnManifest>"#,
        );
        assert!(should_install(
            &m,
            "(VersionNT64) AND NOT (Arm64_Check = \"ARM64\")"
        ));
        assert!(!should_install(&m, "Arm64_Check = \"ARM64\""));
        assert!(!should_install(&m, "VersionNT = v6.1 AND VersionNT64"));
        assert!(!should_install(&m, "VersionNT = v6.3 AND NOT VersionNT64"));
        assert!(should_install(&m, "VersionNT >= v6.1"));
        assert!(should_install(&m, "NOT Inconnue OR (Inconnue"));
        assert!(should_install(&m, ""));
    }

    #[test]
    fn journal_silencieux_et_option_log() {
        let exe = Path::new("/x/vc_redist.x86.exe");
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(log_path(exe, &a(&["/install"]), false).is_none());
        let q = log_path(exe, &a(&["/install", "/quiet"]), true).unwrap();
        let name = q.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("vc_redist.x86_") && name.ends_with(".log"),
            "{name}"
        );
        assert!(q.to_string_lossy().contains("Temp"), "{}", q.display());
        let l = log_path(exe, &a(&["/quiet", "/log", "/tmp/r.log"]), true).unwrap();
        assert!(l.ends_with("r.log"), "{}", l.display());
    }

    const REDIST_MANIFEST: &str = r#"<BurnManifest>
  <Registration Version="14.40.1"><Arp DisplayName="Winlux Redistribuable" /></Registration>
  <Container Id="WixAttachedContainer" AttachedIndex="1" />
  <Payload Id="msi" FilePath="packages\redist.msi" SourcePath="a0" Container="WixAttachedContainer" />
  <Chain>
    <MsiPackage Id="redist" ProductCode="{7B2C6D1F-2222-4333-8444-A55566667777}"><PayloadRef Id="msi" /></MsiPackage>
  </Chain>
</BurnManifest>"#;

    /// Bundle réel : le MSI (wixl) dépose une DLL dans System32, où le
    /// chargeur la trouve ; /uninstall la retire.
    #[test]
    fn installe_un_redistribuable_dans_system32() {
        let _a = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _b = crate::dlls::advapi32::REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("winlux-burn-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("c")).unwrap();
        let vars = [
            ("WINLUX_DRIVE_C", root.join("c")),
            ("XDG_DATA_HOME", root.join("data")),
            ("XDG_CONFIG_HOME", root.join("config")),
            ("WINLUX_REGISTRY_PATH", root.join("registry.tsv")),
        ];
        let saved: Vec<_> = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();
        for (k, v) in &vars {
            std::env::set_var(k, v);
        }
        let msi: &[u8] = include_bytes!("../msi/testdata/redist.msi");
        let exe = root.join("vc_redist.x64.exe");
        std::fs::write(&exe, build_bundle_with(REDIST_MANIFEST, &[("a0", msi)])).unwrap();
        assert!(is_bundle_file(&exe));
        assert_eq!(launch(&exe, &["/install".into(), "/quiet".into()]), Some(0));
        let found =
            crate::loader::find_dll_file("WINLUX_REDIST_TEST.dll", &root.join("app/app.exe"));
        let ok = found
            .as_ref()
            .is_some_and(|p| std::fs::read(p).unwrap() == b"MZ-winlux-redist");
        assert!(ok, "DLL trouvée dans System32 : {found:?}");
        assert_eq!(
            launch(&exe, &["/uninstall".into(), "/quiet".into()]),
            Some(0)
        );
        let gone =
            crate::loader::find_dll_file("winlux_redist_test.dll", &root.join("app/app.exe"));
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let _ = std::fs::remove_dir_all(&root);
        assert!(gone.is_none(), "désinstallé");
    }
}
