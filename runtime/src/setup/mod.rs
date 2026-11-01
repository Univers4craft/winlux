//! Installateurs Windows NSIS et Inno Setup, installés par Winlux lui-même.
//!
//! Les deux formats d'installateurs les plus répandus sont des `.exe` 32 bits
//! qui contiennent leurs fichiers compressés et un « script » d'installation.
//! Plutôt que d'exécuter leur code Windows, Winlux lit ce contenu (format
//! publié par chaque projet), extrait les fichiers, écrit le registre, crée
//! les raccourcis et inscrit l'application dans « Applications installées ».
//! Tout ce qui est créé est noté dans un journal pour une désinstallation
//! propre (`~/.local/share/winlux/setup/`).
//!
//! `winlux setup.exe` (double-clic) passe ici automatiquement ;
//! `winlux --setup …` donne accès aux options (voir `USAGE`).

pub mod inno;
mod inno_layout;
pub mod lzma;
pub mod nsis;
pub mod programs;

use crate::dlls::advapi32::{apply_registry_edits, registry_key_is_empty, RegEdit};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const USAGE: &str = "Winlux : installateurs NSIS et Inno Setup
  winlux --setup installateur.exe [/D=dossier]   installer
  winlux --setup --info installateur.exe          type, nom, version, dossier
  winlux --setup --list                           applications installées ainsi
  winlux --setup --uninstall IDENTIFIANT|désinstalleur.exe
  winlux --setup --programs [--out FICHIER]       toutes les applications inscrites
  winlux --setup --remove CLÉ [--out FICHIER]     désinstaller l'une d'elles";

/// Type d'installateur reconnu dans un `.exe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Nsis,
    Inno,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Nsis => "nsis",
            Kind::Inno => "inno",
        }
    }
}

/// Reconnaît un installateur NSIS ou Inno Setup.
pub fn detect(data: &[u8]) -> Option<Kind> {
    if data.len() < 64 || &data[..2] != b"MZ" {
        return None;
    }
    if inno::find_offsets(data).is_some() {
        return Some(Kind::Inno);
    }
    if nsis::find_header(data).is_some() {
        return Some(Kind::Nsis);
    }
    None
}

/// Lit le début d'un fichier et dit s'il s'agit d'un installateur.
pub fn detect_file(path: &Path) -> Option<Kind> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() < 4096 {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    detect(&data)
}

/// Résumé d'un installateur (pour l'interface et `--info`).
pub struct Info {
    pub kind: Kind,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub default_dir: String,
}

/// Journal d'exécution (stderr et fichier facultatif).
pub struct Logger {
    pub quiet: bool,
}

impl Logger {
    pub fn log(&mut self, msg: &str) {
        if !self.quiet {
            eprintln!("setup : {msg}");
        }
    }
}

/// Options d'installation communes.
#[derive(Default, Clone)]
pub struct Options {
    /// Dossier d'installation imposé (chemin Windows).
    pub dir: Option<String>,
    /// Pas de raccourci sur le Bureau.
    pub no_desktop: bool,
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// Texte UTF-16 petit-boutiste (arrêté au premier nul si `nul`).
pub fn utf16_text(b: &[u8], nul: bool) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| !nul || u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn utf16z(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    v.extend([0, 0]);
    v
}

/// Dossiers Windows standard tels que Winlux les présente (mêmes conventions
/// que le msiexec intégré). Chemins sans barre finale.
pub struct Folders {
    pub x64: bool,
}

impl Folders {
    pub fn program_files(&self) -> &'static str {
        if self.x64 {
            "C:\\Program Files"
        } else {
            "C:\\Program Files (x86)"
        }
    }

    pub fn start_menu() -> &'static str {
        "C:\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu"
    }

    pub fn programs() -> String {
        format!("{}\\Programs", Self::start_menu())
    }

    /// Bureau XDG de l'utilisateur, en chemin Windows.
    pub fn desktop() -> String {
        let home = crate::paths::drive_c();
        let mut dir = home.join("Desktop");
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("XDG_DESKTOP_DIR=") {
                    let v = v.trim_matches('"');
                    let home_s = std::env::var("HOME").unwrap_or_default();
                    dir = PathBuf::from(v.replace("$HOME", &home_s));
                }
            }
        } else if home.join("Bureau").is_dir() {
            dir = home.join("Bureau");
        }
        crate::paths::to_windows(&dir)
            .trim_end_matches('\\')
            .to_string()
    }

    /// Dossier temporaire Windows.
    pub fn temp() -> &'static str {
        "C:\\AppData\\Local\\Temp"
    }
}

/// Langue de l'utilisateur (code Windows), pour choisir les messages.
pub fn user_language() -> u16 {
    let lang = std::env::var("LANGUAGE")
        .ok()
        .filter(|l| !l.is_empty())
        .or_else(|| std::env::var("LC_ALL").ok().filter(|l| !l.is_empty()))
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    match lang.get(..2).unwrap_or("") {
        "en" => 0x0409,
        "es" => 0x0C0A,
        "de" => 0x0407,
        "it" => 0x0410,
        _ => 0x040C,
    }
}

/// Ce que l'installation a créé, pour la désinstallation.
#[derive(Default, Debug)]
pub struct Manifest {
    /// Nom de la clé `Uninstall\…` (identifiant de l'application).
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// Dossier d'installation (chemin Windows).
    pub location: String,
    /// Désinstalleur écrit par l'installateur (chemin Linux).
    pub uninstaller: Option<PathBuf>,
    /// Clé complète d'inscription dans « Applications installées ».
    pub uninstall_key: String,
    pub files: Vec<PathBuf>,
    pub dirs: Vec<PathBuf>,
    pub values: Vec<(String, String)>,
    /// Clés à retirer si elles sont vides à la désinstallation.
    pub keys: Vec<String>,
    /// Clés à retirer entièrement.
    pub trees: Vec<String>,
    /// Motifs à effacer à la désinstallation (chemin Windows, type).
    pub deletes: Vec<(String, u8)>,
}

pub fn manifest_dir() -> PathBuf {
    crate::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp/winlux"))
        .join("setup")
}

fn manifest_path(id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    manifest_dir().join(format!("{safe}.txt"))
}

impl Manifest {
    pub fn save(&self) -> std::io::Result<()> {
        let mut text = String::from("WINLUX-SETUP-1\n");
        for (k, v) in [
            ("id", &self.id),
            ("kind", &self.kind),
            ("name", &self.name),
            ("version", &self.version),
            ("publisher", &self.publisher),
            ("location", &self.location),
            ("uninstallkey", &self.uninstall_key),
        ] {
            text.push_str(&format!("{k}\t{}\n", clean(v)));
        }
        if let Some(u) = &self.uninstaller {
            text.push_str(&format!("uninstaller\t{}\n", clean(&u.to_string_lossy())));
        }
        for f in &self.files {
            text.push_str(&format!("file\t{}\n", clean(&f.to_string_lossy())));
        }
        for d in &self.dirs {
            text.push_str(&format!("dir\t{}\n", clean(&d.to_string_lossy())));
        }
        for (k, n) in &self.values {
            text.push_str(&format!("value\t{}\t{}\n", clean(k), clean(n)));
        }
        for k in &self.keys {
            text.push_str(&format!("key\t{}\n", clean(k)));
        }
        for k in &self.trees {
            text.push_str(&format!("tree\t{}\n", clean(k)));
        }
        for (p, t) in &self.deletes {
            text.push_str(&format!("delete\t{t}\t{}\n", clean(p)));
        }
        let path = manifest_path(&self.id);
        std::fs::create_dir_all(manifest_dir())?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    pub fn load(path: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(path).ok()?;
        if !text.starts_with("WINLUX-SETUP-1") {
            return None;
        }
        let mut m = Manifest::default();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            match parts.as_slice() {
                ["id", v] => m.id = v.to_string(),
                ["kind", v] => m.kind = v.to_string(),
                ["name", v] => m.name = v.to_string(),
                ["version", v] => m.version = v.to_string(),
                ["publisher", v] => m.publisher = v.to_string(),
                ["location", v] => m.location = v.to_string(),
                ["uninstallkey", v] => m.uninstall_key = v.to_string(),
                ["uninstaller", v] => m.uninstaller = Some(PathBuf::from(v)),
                ["file", v] => m.files.push(PathBuf::from(v)),
                ["dir", v] => m.dirs.push(PathBuf::from(v)),
                ["value", k, n] => m.values.push((k.to_string(), n.to_string())),
                ["key", k] => m.keys.push(k.to_string()),
                ["tree", k] => m.trees.push(k.to_string()),
                ["delete", t, p] => m.deletes.push((p.to_string(), t.parse().unwrap_or(0))),
                _ => {}
            }
        }
        Some(m)
    }

    pub fn all() -> Vec<Manifest> {
        let Ok(dir) = std::fs::read_dir(manifest_dir()) else {
            return Vec::new();
        };
        let mut v: Vec<Manifest> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
            .filter_map(|e| Manifest::load(&e.path()))
            .collect();
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    pub fn find(id: &str) -> Option<Manifest> {
        Manifest::load(&manifest_path(id)).or_else(|| {
            Manifest::all()
                .into_iter()
                .find(|m| m.id.eq_ignore_ascii_case(id) || m.name.eq_ignore_ascii_case(id))
        })
    }

    /// Manifeste dont `exe` est le désinstalleur.
    pub fn of_uninstaller(exe: &Path) -> Option<Manifest> {
        let target = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
        Manifest::all().into_iter().find(|m| {
            m.uninstaller
                .as_ref()
                .is_some_and(|u| std::fs::canonicalize(u).unwrap_or_else(|_| u.clone()) == target)
        })
    }
}

/// Opérations d'installation qui alimentent le manifeste.
pub struct Installer {
    pub manifest: Manifest,
    pub edits: Vec<RegEdit>,
    files_seen: HashSet<PathBuf>,
    pub log: Logger,
}

/// Clé de registre normalisée (`hklm\software\…`).
pub fn reg_key(root: &str, sub: &str) -> String {
    let sub = sub.trim_matches('\\');
    if sub.is_empty() {
        root.to_string()
    } else {
        format!("{root}\\{sub}").to_lowercase()
    }
}

impl Installer {
    pub fn new(kind: Kind, log: Logger) -> Installer {
        Installer {
            manifest: Manifest {
                kind: kind.name().into(),
                ..Default::default()
            },
            edits: Vec::new(),
            files_seen: HashSet::new(),
            log,
        }
    }

    /// Crée un dossier (chemin Windows) et retient ceux qui n'existaient pas.
    pub fn mkdirs_win(&mut self, win: &str) -> std::io::Result<PathBuf> {
        let path = crate::paths::to_linux(win);
        self.mkdirs(&path)?;
        Ok(path)
    }

    pub fn mkdirs(&mut self, path: &Path) -> std::io::Result<()> {
        let mut missing = Vec::new();
        let mut p = path.to_path_buf();
        while !p.exists() {
            missing.push(p.clone());
            if !p.pop() {
                break;
            }
        }
        std::fs::create_dir_all(path)?;
        for d in missing.into_iter().rev() {
            if !self.manifest.dirs.contains(&d) {
                self.manifest.dirs.push(d);
            }
        }
        Ok(())
    }

    /// Ouvre un fichier en écriture (chemin Windows), dossiers compris.
    pub fn create_file(&mut self, win: &str) -> std::io::Result<(PathBuf, std::fs::File)> {
        let path = crate::paths::to_linux(win);
        if let Some(parent) = path.parent() {
            self.mkdirs(parent)?;
        }
        // Un fichier en lecture seule d'une version précédente.
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perm = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(&path, perm);
        }
        let f = std::fs::File::create(&path)?;
        self.note_file(&path);
        Ok((path, f))
    }

    pub fn note_file(&mut self, path: &Path) {
        if self.files_seen.insert(path.to_path_buf()) {
            self.manifest.files.push(path.to_path_buf());
        }
    }

    pub fn write_file(&mut self, win: &str, data: &[u8]) -> std::io::Result<PathBuf> {
        use std::io::Write;
        let (path, mut f) = self.create_file(win)?;
        f.write_all(data)?;
        Ok(path)
    }

    /// Raccourci Windows (.lnk, ou .url pour une adresse web).
    pub fn shortcut(
        &mut self,
        link: &str,
        target: &str,
        args: &str,
        workdir: &str,
        desc: &str,
    ) -> std::io::Result<()> {
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            let link = link
                .strip_suffix(".lnk")
                .map(|l| format!("{l}.url"))
                .unwrap_or_else(|| link.to_string());
            let body = format!("[InternetShortcut]\r\nURL={target}\r\n");
            self.write_file(&link, body.as_bytes())?;
            return Ok(());
        }
        let bytes = crate::msi::shortcut_bytes(target, args, workdir, desc);
        self.write_file(link, &bytes)?;
        Ok(())
    }

    /// Écrit une valeur de registre (type Windows, octets bruts).
    pub fn reg_set(&mut self, key: &str, name: &str, kind: u32, bytes: Vec<u8>, track: bool) {
        let key = key.to_lowercase();
        self.reg_note_key(&key);
        if track {
            let v = (key.clone(), name.to_lowercase());
            if !self.manifest.values.contains(&v) {
                self.manifest.values.push(v);
            }
        }
        self.edits.push(RegEdit::Set {
            key,
            name: name.into(),
            kind,
            bytes,
        });
    }

    pub fn reg_str(&mut self, key: &str, name: &str, value: &str) {
        self.reg_set(key, name, 1, utf16z(value), false);
    }

    pub fn reg_dword(&mut self, key: &str, name: &str, value: u32) {
        self.reg_set(key, name, 4, value.to_le_bytes().to_vec(), false);
    }

    /// Retient les clés créées par l'installation (la clé et ses parents
    /// qui n'existaient pas) : retirées à la désinstallation si vides.
    pub fn reg_note_key(&mut self, key: &str) {
        let key = key.to_lowercase();
        let parts: Vec<&str> = key.split('\\').collect();
        for n in 2..=parts.len() {
            let k = parts[..n].join("\\");
            if !self.manifest.keys.contains(&k) && !crate::dlls::advapi32::registry_key_exists(&k) {
                self.manifest.keys.push(k);
            }
        }
        self.edits.push(RegEdit::Key(key));
    }

    pub fn reg_delete_value(&mut self, key: &str, name: &str) {
        self.edits.push(RegEdit::DeleteValue {
            key: key.to_lowercase(),
            name: name.into(),
        });
    }

    pub fn reg_delete_tree(&mut self, key: &str) {
        self.edits.push(RegEdit::DeleteTree(key.to_lowercase()));
    }

    /// Applique les écritures en attente (le registre est relu par les
    /// lectures suivantes du script).
    pub fn flush_registry(&mut self) -> bool {
        if self.edits.is_empty() {
            return true;
        }
        let ok = apply_registry_edits(&self.edits);
        self.edits.clear();
        ok
    }

    /// Inscrit l'application dans « Applications installées » si le script
    /// ne l'a pas fait, puis enregistre le manifeste.
    pub fn finish(mut self) -> Result<Manifest, String> {
        if !self.flush_registry() {
            return Err("écriture du registre impossible".into());
        }
        if self.manifest.id.is_empty() {
            self.manifest.id = self.manifest.name.clone();
        }
        if self.manifest.uninstall_key.is_empty() {
            let key = format!(
                "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{}",
                self.manifest.id.to_lowercase()
            );
            self.manifest.uninstall_key = key.clone();
            let m = &self.manifest;
            let (name, version, publisher, location) = (
                m.name.clone(),
                m.version.clone(),
                m.publisher.clone(),
                m.location.clone(),
            );
            self.reg_str(&key, "DisplayName", &name);
            if !version.is_empty() {
                self.reg_str(&key, "DisplayVersion", &version);
            }
            if !publisher.is_empty() {
                self.reg_str(&key, "Publisher", &publisher);
            }
            if !location.is_empty() {
                self.reg_str(&key, "InstallLocation", &location);
            }
            let id = self.manifest.id.clone();
            self.reg_str(
                &key,
                "UninstallString",
                &format!("winlux --setup --uninstall \"{id}\""),
            );
            self.reg_dword(&key, "NoModify", 1);
            self.reg_dword(&key, "NoRepair", 1);
            if !self.flush_registry() {
                return Err("écriture du registre impossible".into());
            }
        }
        // Date et taille affichées par « Applications installées ».
        let key = self.manifest.uninstall_key.clone();
        let has =
            |name: &str| crate::dlls::advapi32::read_registry_value_internal(&key, name).is_some();
        let size_known = crate::dlls::advapi32::read_registry_value_internal(&key, "EstimatedSize")
            .is_some_and(|b| b.iter().any(|&x| x != 0));
        let (date, size) = (has("InstallDate"), size_known);
        if !date {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let (y, m, d) = crate::msi::civil((secs / 86_400) as i64);
            self.reg_str(&key, "InstallDate", &format!("{y:04}{m:02}{d:02}"));
        }
        if !size {
            let bytes: u64 = self
                .manifest
                .files
                .iter()
                .filter_map(|f| std::fs::metadata(f).ok())
                .map(|m| m.len())
                .sum();
            self.reg_dword(
                &key,
                "EstimatedSize",
                (bytes / 1024).min(u32::MAX as u64) as u32,
            );
        }
        if (!date || !size) && !self.flush_registry() {
            return Err("écriture du registre impossible".into());
        }
        self.manifest
            .save()
            .map_err(|e| format!("journal d'installation : {e}"))?;
        Ok(self.manifest)
    }
}

/// Efface les fichiers correspondant à un motif Windows (`*` et `?` dans le
/// dernier composant). `kind` : 0 fichiers, 1 fichiers et sous-dossiers,
/// 2 dossier s'il est vide.
pub fn delete_pattern(pattern: &str, kind: u8) {
    let path = crate::paths::to_linux(pattern);
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let name = name.to_string_lossy().to_string();
    if kind == 2 {
        let _ = std::fs::remove_dir(&path);
        return;
    }
    let matches: Vec<PathBuf> = if name.contains('*') || name.contains('?') {
        std::fs::read_dir(parent)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| wildcard(&name, &e.file_name().to_string_lossy()))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![path.clone()]
    };
    for m in matches {
        if m.is_dir() {
            if kind == 1 {
                let _ = std::fs::remove_dir_all(&m);
            }
        } else {
            let _ = std::fs::remove_file(&m);
        }
    }
}

/// Correspondance de motif Windows (insensible à la casse).
pub fn wildcard(pattern: &str, name: &str) -> bool {
    fn rec(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => rec(&p[1..], n) || (!n.is_empty() && rec(p, &n[1..])),
            (Some('?'), Some(_)) => rec(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && rec(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // « *.* » désigne tout fichier, avec ou sans extension.
    if pattern == "*.*" {
        return true;
    }
    rec(&p, &n)
}

/// Retire tout ce qu'une installation a créé.
pub fn remove(m: &Manifest, log: &mut Logger) {
    for (pattern, kind) in &m.deletes {
        delete_pattern(pattern, *kind);
    }
    for f in &m.files {
        let _ = std::fs::remove_file(f);
    }
    if let Some(u) = &m.uninstaller {
        let _ = std::fs::remove_file(u);
    }
    let mut dirs = m.dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        let _ = std::fs::remove_dir(d); // seulement si vide
    }
    let mut edits: Vec<RegEdit> = m
        .values
        .iter()
        .map(|(k, n)| RegEdit::DeleteValue {
            key: k.clone(),
            name: n.clone(),
        })
        .collect();
    for t in &m.trees {
        edits.push(RegEdit::DeleteTree(t.clone()));
    }
    if !m.uninstall_key.is_empty() {
        edits.push(RegEdit::DeleteTree(m.uninstall_key.clone()));
    }
    apply_registry_edits(&edits);
    let mut keys = m.keys.clone();
    keys.sort_by_key(|k| std::cmp::Reverse(k.matches('\\').count()));
    keys.dedup();
    for k in keys {
        if registry_key_is_empty(&k) {
            apply_registry_edits(&[RegEdit::DeleteTree(k)]);
        }
    }
    let _ = std::fs::remove_file(manifest_path(&m.id));
    log.log(&format!("{} est désinstallé.", m.name));
}

/// Informations sur un installateur.
pub fn info(path: &Path) -> Result<Info, String> {
    let data = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    match detect(&data) {
        Some(Kind::Inno) => inno::info(&data),
        Some(Kind::Nsis) => nsis::info(&data, path),
        None => Err("ce programme n'est pas un installateur NSIS ou Inno Setup".into()),
    }
}

/// Installe. Renvoie le manifeste enregistré.
pub fn install(path: &Path, opts: &Options, log: Logger) -> Result<Manifest, String> {
    let data = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let kind = detect(&data).ok_or("ce programme n'est pas un installateur NSIS ou Inno Setup")?;
    let mut inst = Installer::new(kind, log);
    match kind {
        Kind::Inno => inno::install(&data, path, opts, &mut inst)?,
        Kind::Nsis => nsis::install(&data, path, opts, &mut inst)?,
    }
    // Réinstallation : les fichiers de l'ancienne version absents de la
    // nouvelle sont retirés, le reste est conservé.
    if let Some(old) = Manifest::find(&inst.manifest.id) {
        let keep: HashSet<&PathBuf> = inst.manifest.files.iter().collect();
        for f in old.files.iter().filter(|f| !keep.contains(f)) {
            let _ = std::fs::remove_file(f);
        }
        for d in old.dirs {
            if !inst.manifest.dirs.contains(&d) {
                inst.manifest.dirs.push(d);
            }
        }
    }
    let name = inst.manifest.name.clone();
    let m = inst.finish()?;
    eprintln!("setup : {name} est installé.");
    Ok(m)
}

/// Lit un argument chemin (Linux existant ou Windows).
pub(crate) fn host_path(arg: &str) -> PathBuf {
    let p = Path::new(arg);
    if p.exists() {
        return p.to_path_buf();
    }
    crate::paths::to_linux(arg)
}

pub fn uninstall(target: &str, log: &mut Logger) -> i32 {
    let p = host_path(target);
    let m = if p.is_file() {
        Manifest::of_uninstaller(&p)
    } else {
        Manifest::find(target)
    };
    match m {
        Some(m) => {
            remove(&m, log);
            0
        }
        None => {
            log.log(&format!("application inconnue : {target}"));
            1
        }
    }
}

pub fn cli(args: &[String]) -> i32 {
    let mut log = Logger { quiet: false };
    let mut opts = Options::default();
    let mut file = None;
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|k| args.get(k + 1))
        .cloned();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let lower = a.to_ascii_lowercase();
        match lower.as_str() {
            "--list" => {
                for m in Manifest::all() {
                    println!("{}\t{}\t{}\t{}", m.id, m.name, m.version, m.kind);
                }
                return 0;
            }
            "--info" => {
                let Some(p) = args.get(i + 1) else {
                    eprintln!("{USAGE}");
                    return 2;
                };
                return match info(&host_path(p)) {
                    Ok(inf) => {
                        println!(
                            "{}\t{}\t{}\t{}\t{}",
                            inf.kind.name(),
                            inf.name,
                            inf.version,
                            inf.publisher,
                            inf.default_dir
                        );
                        0
                    }
                    Err(e) => {
                        eprintln!("setup : {e}");
                        1
                    }
                };
            }
            "--programs" => {
                let text = programs::format_list(&programs::list());
                match &out {
                    Some(o) => programs::write_out(o, &text),
                    None => print!("{text}"),
                }
                return 0;
            }
            "--remove" => {
                let Some(k) = args.get(i + 1) else {
                    eprintln!("{USAGE}");
                    return 2;
                };
                let (rc, msg) = programs::uninstall_key(k);
                match &out {
                    Some(o) => {
                        let mut text = format!("code={rc}\nmessage={msg}\n");
                        text.push_str(&programs::format_list(&programs::list()));
                        programs::write_out(o, &text);
                    }
                    None => eprintln!("setup : {msg}"),
                }
                return rc;
            }
            "--uninstall" | "/uninstall" | "-u" => {
                let Some(t) = args.get(i + 1) else {
                    eprintln!("{USAGE}");
                    return 2;
                };
                return uninstall(t, &mut log);
            }
            "--help" | "-h" | "/?" => {
                println!("{USAGE}");
                return 0;
            }
            "/s" | "/silent" | "/verysilent" | "/sp-" | "/suppressmsgboxes" | "/norestart"
            | "--quiet" => log.quiet = lower == "--quiet",
            "/nodesktop" | "--no-desktop" => opts.no_desktop = true,
            _ => {
                if let Some(d) = a.strip_prefix("/D=").or_else(|| a.strip_prefix("/DIR=")) {
                    opts.dir = Some(d.trim_matches('"').to_string());
                } else if let Some(d) = a.strip_prefix("--dir=") {
                    opts.dir = Some(d.to_string());
                } else if lower.starts_with('/') && !Path::new(a).exists() {
                    // Option propre à l'installateur : ignorée.
                } else if file.is_none() {
                    file = Some(a.clone());
                }
            }
        }
        i += 1;
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        return 2;
    };
    match install(&host_path(&file), &opts, log) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("setup : échec de l'installation : {e}");
            1
        }
    }
}

/// Lecteur d'octets à partir d'un flux (lecture des structures).
pub struct Stream<R: Read> {
    pub inner: R,
}

impl<R: Read> Stream<R> {
    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut v = vec![0u8; n];
        self.inner
            .read_exact(&mut v)
            .map_err(|e| format!("données tronquées : {e}"))?;
        Ok(v)
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

pub fn le_u32(d: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(off..off + 4)?.try_into().ok()?))
}

pub fn le_u64(d: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(off..off + 8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard("*.txt", "Lisez-moi.TXT"));
        assert!(wildcard("*.*", "fichier"));
        assert!(wildcard("unins???.*", "unins000.dat"));
        assert!(!wildcard("*.dll", "app.exe"));
    }
}

/// Appelé avant le chargement d'un `.exe` : désinstalleur connu ou
/// installateur NSIS/Inno Setup. `None` : programme ordinaire.
pub fn launch(exe: &Path, args: &[String]) -> Option<i32> {
    if !exe.is_file() {
        return None;
    }
    if let Some(m) = Manifest::of_uninstaller(exe) {
        let mut log = Logger { quiet: false };
        remove(&m, &mut log);
        return Some(0);
    }
    detect_file(exe)?;
    let mut cli_args: Vec<String> = args.to_vec();
    cli_args.push(exe.to_string_lossy().into_owned());
    Some(cli(&cli_args))
}
