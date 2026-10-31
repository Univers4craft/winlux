//! Registre Windows de Winlux : un seul magasin persistant, partagé par toutes
//! les applications .exe du profil (et par regedit, reg.exe et msiexec).
//!
//! Stockage : `registry.tsv` dans le dossier de données Winlux, lisible depuis
//! Linux. Chaque écriture relit le fichier sous verrou (`flock`), applique la
//! modification et remplace le fichier de façon atomique ; deux applications
//! qui écrivent en même temps ne s'écrasent donc pas. Les lectures réutilisent
//! la copie en mémoire tant que le fichier n'a pas changé sur le disque.
//!
//! Comme sous Windows :
//! - les noms de clés et de valeurs gardent leur casse mais se comparent sans ;
//! - les chaînes (REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ) sont stockées en
//!   UTF-16LE ; les API « A » les convertissent en Windows-1252 à la volée ;
//! - HKEY_CLASSES_ROOT est la vue fusionnée de HKCU\Software\Classes (prioritaire)
//!   et de HKLM\Software\Classes ; rien n'est stocké sous HKCR lui-même.

pub mod cli;
pub mod regfile;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const REG_NONE: u32 = 0;
pub const REG_SZ: u32 = 1;
pub const REG_EXPAND_SZ: u32 = 2;
pub const REG_BINARY: u32 = 3;
pub const REG_DWORD: u32 = 4;
pub const REG_MULTI_SZ: u32 = 7;
pub const REG_QWORD: u32 = 11;

pub const ERROR_SUCCESS: i32 = 0;
pub const ERROR_FILE_NOT_FOUND: i32 = 2;
pub const ERROR_ACCESS_DENIED: i32 = 5;
pub const ERROR_INVALID_HANDLE: i32 = 6;
pub const ERROR_INVALID_DATA: i32 = 13;
pub const ERROR_INVALID_PARAMETER: i32 = 87;
pub const ERROR_MORE_DATA: i32 = 234;
pub const ERROR_NO_MORE_ITEMS: i32 = 259;
pub const ERROR_ALREADY_EXISTS: i32 = 183;

const HEADER_V1: &str = "WINLUX-REGISTRY-1";
const HEADER_V2: &str = "WINLUX-REGISTRY-2";

/// Racines stockées. HKCR n'en fait pas partie : c'est une vue (voir `HKCR`).
pub const ROOTS: [&str; 4] = ["hkcu", "hklm", "hku", "hkcc"];
pub const CLASSES: &str = "hklm\\software\\classes";
/// Vue fusionnée HKEY_CLASSES_ROOT (rien n'est stocké sous ce nom).
pub const HKCR: &str = "hkcr";
const USER_CLASSES: &str = "hkcu\\Software\\Classes";

#[derive(Clone, Default, Debug)]
pub struct Key {
    /// Chemin complet avec la casse d'origine (`hklm\Software\Éditeur`).
    pub name: String,
    pub values: HashMap<String, Value>,
    /// Dernière modification, en FILETIME (100 ns depuis 1601).
    pub last_write: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub name: String,
    pub kind: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct Data {
    /// Clé = chemin en minuscules.
    pub keys: HashMap<String, Key>,
}

impl Data {
    fn empty() -> Self {
        let mut data = Data {
            keys: HashMap::new(),
        };
        for root in ROOTS {
            data.ensure(root);
        }
        data
    }

    /// Crée la clé et ses parents ; vrai si la clé n'existait pas.
    fn raw_ensure(&mut self, display: &str) -> bool {
        let mut created = false;
        let mut lower = String::new();
        let mut shown = String::new();
        for part in display.split('\\').filter(|p| !p.is_empty()) {
            if !lower.is_empty() {
                lower.push('\\');
                shown.push('\\');
            }
            lower.push_str(&part.to_lowercase());
            shown.push_str(part);
            if !self.keys.contains_key(&lower) {
                created = true;
                self.keys.insert(
                    lower.clone(),
                    Key {
                        name: shown.clone(),
                        values: HashMap::new(),
                        last_write: now_filetime(),
                    },
                );
            } else if let Some(k) = self.keys.get(&lower) {
                // Garder la casse déjà enregistrée pour la suite du chemin.
                shown = k.name.clone();
            }
        }
        created
    }

    fn raw_key(&self, path: &str) -> Option<&Key> {
        self.keys.get(&path.to_lowercase())
    }

    fn raw_key_mut(&mut self, path: &str) -> Option<&mut Key> {
        self.keys.get_mut(&path.to_lowercase())
    }

    /// Noms (casse d'origine) des sous-clés directes, triés sans casse.
    fn raw_subkeys(&self, path: &str) -> Vec<String> {
        let prefix = format!("{}\\", path.to_lowercase());
        let mut out: Vec<(String, String)> = self
            .keys
            .iter()
            .filter_map(|(lower, key)| {
                let rest = lower.strip_prefix(&prefix)?;
                if rest.is_empty() || rest.contains('\\') {
                    return None;
                }
                let shown = key.name.rsplit('\\').next().unwrap_or(rest).to_string();
                Some((rest.to_string(), shown))
            })
            .collect();
        out.sort();
        out.into_iter().map(|(_, s)| s).collect()
    }

    /// Valeurs triées par nom (la valeur par défaut en premier).
    fn raw_values(&self, path: &str) -> Vec<Value> {
        let Some(key) = self.raw_key(path) else {
            return Vec::new();
        };
        let mut v: Vec<(&String, &Value)> = key.values.iter().collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v.into_iter().map(|(_, v)| v.clone()).collect()
    }

    fn raw_get(&self, path: &str, name: &str) -> Option<&Value> {
        self.raw_key(path)?.values.get(&name.to_lowercase())
    }

    fn raw_set(&mut self, path: &str, name: &str, kind: u32, bytes: Vec<u8>) {
        self.raw_ensure(path);
        if let Some(key) = self.raw_key_mut(path) {
            key.values.insert(
                name.to_lowercase(),
                Value {
                    name: name.to_string(),
                    kind,
                    bytes,
                },
            );
            key.last_write = now_filetime();
        }
    }

    fn raw_delete_value(&mut self, path: &str, name: &str) -> bool {
        match self.raw_key_mut(path) {
            Some(key) => {
                let removed = key.values.remove(&name.to_lowercase()).is_some();
                if removed {
                    key.last_write = now_filetime();
                }
                removed
            }
            None => false,
        }
    }

    /// Supprime la clé et toute son arborescence ; faux si elle n'existait pas.
    fn raw_delete_tree(&mut self, path: &str) -> bool {
        let lower = path.to_lowercase();
        if is_root(&lower) || lower == CLASSES {
            return false;
        }
        let prefix = format!("{lower}\\");
        let before = self.keys.len();
        self.keys
            .retain(|k, _| *k != lower && !k.starts_with(&prefix));
        before != self.keys.len()
    }

    /// Copie l'arborescence `from` sous `to` (créée au besoin).
    fn raw_copy_tree(&mut self, from: &str, to: &str) {
        let from_lower = from.to_lowercase();
        let prefix = format!("{from_lower}\\");
        let mut items: Vec<(String, Key)> = self
            .keys
            .iter()
            .filter(|(k, _)| **k == from_lower || k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let from_len = self
            .key(from)
            .map(|k| k.name.len())
            .unwrap_or(from_lower.len());
        for (_, key) in items {
            let tail = key.name.get(from_len..).unwrap_or("");
            let target = format!("{to}{tail}");
            self.raw_ensure(&target);
            if let Some(dst) = self.raw_key_mut(&target) {
                for (n, v) in key.values {
                    dst.values.insert(n, v);
                }
            }
        }
    }

    /// Renomme la dernière composante de `path` en `new_name`.
    fn raw_rename_key(&mut self, path: &str, new_name: &str) -> Result<(), i32> {
        let lower = path.to_lowercase();
        if new_name.is_empty() || new_name.contains('\\') {
            return Err(ERROR_INVALID_PARAMETER);
        }
        if is_root(&lower) || !self.keys.contains_key(&lower) {
            return Err(ERROR_FILE_NOT_FOUND);
        }
        let parent = match self.raw_key(path).and_then(|k| k.name.rsplit_once('\\')) {
            Some((p, _)) => p.to_string(),
            None => return Err(ERROR_ACCESS_DENIED),
        };
        let target = format!("{parent}\\{new_name}");
        if target.to_lowercase() != lower && self.raw_key(&target).is_some() {
            return Err(ERROR_ALREADY_EXISTS);
        }
        let prefix = format!("{lower}\\");
        let moved: Vec<(String, Key)> = self
            .keys
            .iter()
            .filter(|(k, _)| **k == lower || k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let old_len = self.raw_key(path).map(|k| k.name.len()).unwrap_or(0);
        for (k, _) in &moved {
            self.keys.remove(k);
        }
        for (_, mut key) in moved {
            let tail = key.name.get(old_len..).unwrap_or("").to_string();
            key.name = format!("{target}{tail}");
            self.keys.insert(key.name.to_lowercase(), key);
        }
        Ok(())
    }

    fn raw_rename_value(&mut self, path: &str, old: &str, new: &str) -> Result<(), i32> {
        let key = self.raw_key_mut(path).ok_or(ERROR_FILE_NOT_FOUND)?;
        if old.to_lowercase() != new.to_lowercase() && key.values.contains_key(&new.to_lowercase())
        {
            return Err(ERROR_ALREADY_EXISTS);
        }
        let mut value = key
            .values
            .remove(&old.to_lowercase())
            .ok_or(ERROR_FILE_NOT_FOUND)?;
        value.name = new.to_string();
        key.values.insert(new.to_lowercase(), value);
        key.last_write = now_filetime();
        Ok(())
    }

    // ── Vue publique : HKCR (« hkcr\… ») fusionne HKCU puis HKLM ──

    fn classes_pair(path: &str) -> Option<(String, String)> {
        let lower = path.to_lowercase();
        let rest = if lower == HKCR {
            ""
        } else if lower.starts_with("hkcr\\") {
            &path[4..]
        } else {
            return None;
        };
        Some((
            format!("{USER_CLASSES}{rest}"),
            format!("hklm\\Software\\Classes{rest}"),
        ))
    }

    /// Chemin physique lu : la clé de HKCU si elle existe, sinon celle de HKLM.
    pub fn physical_read(&self, path: &str) -> String {
        match Self::classes_pair(path) {
            Some((user, _)) if self.raw_key(&user).is_some() => user,
            Some((_, machine)) => machine,
            None => path.to_string(),
        }
    }

    /// Chemin physique écrit : HKCU si la clé y existe déjà, sinon HKLM.
    pub fn physical_write(&self, path: &str) -> String {
        self.physical_read(path)
    }

    pub fn ensure(&mut self, path: &str) -> bool {
        let p = self.physical_write(path);
        self.raw_ensure(&p)
    }

    pub fn key(&self, path: &str) -> Option<&Key> {
        match Self::classes_pair(path) {
            Some((user, machine)) => self.raw_key(&user).or_else(|| self.raw_key(&machine)),
            None => self.raw_key(path),
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        self.key(path).is_some()
    }

    fn key_mut(&mut self, path: &str) -> Option<&mut Key> {
        let p = self.physical_write(path);
        self.raw_key_mut(&p)
    }

    pub fn subkeys(&self, path: &str) -> Vec<String> {
        match Self::classes_pair(path) {
            Some((user, machine)) => {
                let mut all = self.raw_subkeys(&user);
                for name in self.raw_subkeys(&machine) {
                    if !all.iter().any(|n| n.to_lowercase() == name.to_lowercase()) {
                        all.push(name);
                    }
                }
                all.sort_by_key(|n| n.to_lowercase());
                all
            }
            None => self.raw_subkeys(path),
        }
    }

    pub fn values(&self, path: &str) -> Vec<Value> {
        match Self::classes_pair(path) {
            Some((user, machine)) => {
                let mut all = self.raw_values(&user);
                for v in self.raw_values(&machine) {
                    if !all
                        .iter()
                        .any(|u| u.name.to_lowercase() == v.name.to_lowercase())
                    {
                        all.push(v);
                    }
                }
                all.sort_by_key(|v| v.name.to_lowercase());
                all
            }
            None => self.raw_values(path),
        }
    }

    pub fn get(&self, path: &str, name: &str) -> Option<&Value> {
        match Self::classes_pair(path) {
            Some((user, machine)) => self
                .raw_get(&user, name)
                .or_else(|| self.raw_get(&machine, name)),
            None => self.raw_get(path, name),
        }
    }

    pub fn set(&mut self, path: &str, name: &str, kind: u32, bytes: Vec<u8>) {
        let p = self.physical_write(path);
        self.raw_set(&p, name, kind, bytes)
    }

    pub fn delete_value(&mut self, path: &str, name: &str) -> bool {
        let p = self.physical_write(path);
        self.raw_delete_value(&p, name)
    }

    pub fn has_subkeys(&self, path: &str) -> bool {
        !self.subkeys(path).is_empty()
    }

    /// Supprime l'arborescence ; pour HKCR, dans HKCU et dans HKLM.
    pub fn delete_tree(&mut self, path: &str) -> bool {
        match Self::classes_pair(path) {
            Some((user, machine)) => {
                if path.eq_ignore_ascii_case(HKCR) {
                    return false;
                }
                let a = self.raw_delete_tree(&user);
                self.raw_delete_tree(&machine) || a
            }
            None => self.raw_delete_tree(path),
        }
    }

    pub fn copy_tree(&mut self, from: &str, to: &str) {
        let from = self.physical_read(from);
        let to = self.physical_write(to);
        self.raw_copy_tree(&from, &to)
    }

    pub fn rename_key(&mut self, path: &str, new_name: &str) -> Result<(), i32> {
        let p = self.physical_write(path);
        self.raw_rename_key(&p, new_name)
    }

    pub fn rename_value(&mut self, path: &str, old: &str, new: &str) -> Result<(), i32> {
        let p = self.physical_write(path);
        self.raw_rename_value(&p, old, new)
    }
}

fn is_root(lower: &str) -> bool {
    ROOTS.contains(&lower)
}

pub fn now_filetime() -> u64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    116_444_736_000_000_000 + since.as_nanos() as u64 / 100
}

fn defaults() -> Data {
    let mut d = Data::empty();
    d.ensure("hklm\\Software\\Classes");
    let sz = |s: &str| utf16z(s);
    for (k, n, v) in [
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "ProductName",
            "Winlux",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentVersion",
            "10.0",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentBuildNumber",
            "19045",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentBuild",
            "19045",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "InstallationType",
            "Client",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "EditionID",
            "Professional",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "DisplayVersion",
            "22H2",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows\\CurrentVersion",
            "ProgramFilesDir",
            "C:\\Program Files",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows\\CurrentVersion",
            "ProgramFilesDir (x86)",
            "C:\\Program Files (x86)",
        ),
        (
            "hklm\\Software\\Microsoft\\Windows\\CurrentVersion",
            "CommonFilesDir",
            "C:\\Program Files\\Common Files",
        ),
        (
            "hklm\\System\\CurrentControlSet\\Control\\Session Manager\\Environment",
            "PROCESSOR_ARCHITECTURE",
            "AMD64",
        ),
        ("hklm\\Software\\Classes\\.exe", "", "exefile"),
        ("hklm\\Software\\Classes\\.reg", "", "regfile"),
    ] {
        d.set(k, n, REG_SZ, sz(v));
    }
    for (k, n, v) in [
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentMajorVersionNumber",
            10u32,
        ),
        (
            "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentMinorVersionNumber",
            0,
        ),
    ] {
        d.set(k, n, REG_DWORD, v.to_le_bytes().to_vec());
    }
    d.ensure("hkcu\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders");
    d.ensure("hkcu\\Software\\Classes");
    // Horodatage neutre : les clés par défaut ne sont pas « modifiées ».
    for key in d.keys.values_mut() {
        key.last_write = 0;
    }
    d
}

// ── Chemins ────────────────────────────────────────────────────────

/// Normalise un chemin saisi par une appli, un fichier .reg ou l'utilisateur
/// (`HKEY_LOCAL_MACHINE\…`, `HKLM\…`, `hkcr\…`) en chemin interne
/// `hklm\…` (casse d'origine conservée hors racine). `None` si la racine est
/// inconnue.
pub fn canonical(path: &str) -> Option<String> {
    let path = path.replace('/', "\\");
    let path = path.trim_matches('\\');
    let (root, rest) = match path.split_once('\\') {
        Some((r, rest)) => (r, rest.trim_matches('\\')),
        None => (path, ""),
    };
    let root = match root.to_ascii_uppercase().as_str() {
        "HKEY_CURRENT_USER" | "HKCU" => "hkcu",
        "HKEY_LOCAL_MACHINE" | "HKLM" => "hklm",
        "HKEY_USERS" | "HKU" => "hku",
        "HKEY_CURRENT_CONFIG" | "HKCC" => "hkcc",
        "HKEY_CLASSES_ROOT" | "HKCR" => HKCR,
        _ => return None,
    };
    let rest: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty()).collect();
    if rest.is_empty() {
        Some(root.to_string())
    } else {
        Some(format!("{root}\\{}", rest.join("\\")))
    }
}

/// Chemin interne → nom affiché façon Windows (`HKEY_LOCAL_MACHINE\…`).
pub fn display_path(path: &str) -> String {
    let (root, rest) = match path.split_once('\\') {
        Some((r, rest)) => (r, Some(rest)),
        None => (path, None),
    };
    let long = match root.to_ascii_lowercase().as_str() {
        "hkcu" => "HKEY_CURRENT_USER",
        "hklm" => "HKEY_LOCAL_MACHINE",
        "hku" => "HKEY_USERS",
        "hkcc" => "HKEY_CURRENT_CONFIG",
        "hkcr" => "HKEY_CLASSES_ROOT",
        _ => root,
    };
    match rest {
        Some(r) => format!("{long}\\{r}"),
        None => long.to_string(),
    }
}

/// Joint un chemin interne et une sous-clé relative.
pub fn join(base: &str, sub: &str) -> String {
    let sub = sub.replace('/', "\\");
    let parts: Vec<&str> = sub.split('\\').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        base.to_string()
    } else {
        format!("{base}\\{}", parts.join("\\"))
    }
}

// ── Chaînes ────────────────────────────────────────────────────────

pub fn is_string(kind: u32) -> bool {
    matches!(kind, REG_SZ | REG_EXPAND_SZ | REG_MULTI_SZ)
}

pub fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

pub fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

/// Texte d'une chaîne stockée, sans le zéro final.
pub fn string_of(bytes: &[u8]) -> String {
    let mut units = utf16_units(bytes);
    while units.last() == Some(&0) {
        units.pop();
    }
    String::from_utf16_lossy(&units)
}

/// Éléments d'un REG_MULTI_SZ stocké.
pub fn multi_of(bytes: &[u8]) -> Vec<String> {
    let units = utf16_units(bytes);
    let mut out: Vec<String> = units
        .split(|&u| u == 0)
        .map(String::from_utf16_lossy)
        .collect();
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

pub fn multi_bytes(items: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        out.extend(utf16z(item));
    }
    out.extend([0, 0]);
    out
}

/// Développe les `%VARIABLE%` d'un REG_EXPAND_SZ (variables inconnues gardées).
pub fn expand(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match crate::dlls::kernel32::env_value(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Données envoyées par une API « A » → stockage (UTF-16 pour les chaînes).
pub fn from_ansi(kind: u32, bytes: &[u8]) -> Vec<u8> {
    if !is_string(kind) {
        return bytes.to_vec();
    }
    crate::dlls::decode_ansi(bytes)
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Stockage → données rendues à une API « A ».
pub fn to_ansi(kind: u32, bytes: &[u8]) -> Vec<u8> {
    if !is_string(kind) {
        return bytes.to_vec();
    }
    crate::dlls::encode_ansi(&String::from_utf16_lossy(&utf16_units(bytes)))
}

/// Fichier v1 : les chaînes pouvaient être en ANSI/UTF-8 ou en UTF-16.
fn looks_utf16(bytes: &[u8]) -> bool {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return false;
    }
    let units: Vec<u16> = utf16_units(bytes);
    let text: Vec<u16> = units.iter().copied().filter(|&u| u != 0).collect();
    if text.is_empty() {
        return true;
    }
    let latin = text.iter().filter(|&&u| u < 0x100).count();
    String::from_utf16(&units).is_ok() && latin * 2 >= text.len()
}

fn upgrade_v1_string(bytes: &[u8]) -> Vec<u8> {
    if looks_utf16(bytes) {
        return bytes.to_vec();
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => crate::dlls::decode_ansi(bytes),
    };
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

// ── Fichier ────────────────────────────────────────────────────────

pub fn store_path() -> PathBuf {
    if let Some(p) = std::env::var_os("WINLUX_REGISTRY_PATH") {
        return p.into();
    }
    crate::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("registry.tsv")
}

fn hex(bytes: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(D[(b >> 4) as usize] as char);
        out.push(D[(b & 15) as usize] as char);
    }
    out
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let nib = |x: u8| match x {
        b'0'..=b'9' => Some(x - b'0'),
        b'a'..=b'f' => Some(x - b'a' + 10),
        b'A'..=b'F' => Some(x - b'A' + 10),
        _ => None,
    };
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Some((nib(p[0])? << 4) | nib(p[1])?))
        .collect()
}

fn unhex_text(s: &str) -> Result<String, i32> {
    String::from_utf8(unhex(s).ok_or(ERROR_INVALID_DATA)?).map_err(|_| ERROR_INVALID_DATA)
}

/// Ancien emplacement de HKCR (fichiers v1) → HKLM\Software\Classes.
fn upgrade_v1_path(path: String) -> String {
    let lower = path.to_lowercase();
    if lower == "hkcr" {
        return "hklm\\Software\\Classes".into();
    }
    if lower.starts_with("hkcr\\") {
        return format!("hklm\\Software\\Classes{}", &path[4..]);
    }
    path
}

pub fn parse(text: &str) -> Result<Data, i32> {
    let mut lines = text.lines();
    let v1 = match lines.next() {
        Some(HEADER_V1) => true,
        Some(HEADER_V2) => false,
        _ => return Err(ERROR_INVALID_DATA),
    };
    let mut data = if v1 { defaults() } else { Data::empty() };
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["K", p] | ["K", p, _] => {
                let mut p = unhex_text(p)?;
                if v1 {
                    p = upgrade_v1_path(p);
                }
                data.ensure(&p);
                if let (false, Some(stamp)) = (v1, fields.get(2)) {
                    if let (Ok(t), Some(k)) = (stamp.parse::<u64>(), data.key_mut(&p)) {
                        k.last_write = t;
                    }
                }
            }
            ["V", p, n, kind, value] => {
                let mut p = unhex_text(p)?;
                if v1 {
                    p = upgrade_v1_path(p);
                }
                let n = unhex_text(n)?;
                let kind = kind.parse::<u32>().map_err(|_| ERROR_INVALID_DATA)?;
                let mut bytes = unhex(value).ok_or(ERROR_INVALID_DATA)?;
                if v1 && is_string(kind) {
                    bytes = upgrade_v1_string(&bytes);
                }
                let stamp = data.key(&p).map(|k| k.last_write);
                data.set(&p, &n, kind, bytes);
                if let (Some(t), Some(k)) = (stamp, data.key_mut(&p)) {
                    k.last_write = t;
                }
            }
            [""] => {}
            _ => return Err(ERROR_INVALID_DATA),
        }
    }
    Ok(data)
}

pub fn serialize(data: &Data) -> String {
    let mut text = String::from(HEADER_V2);
    text.push('\n');
    let mut keys: Vec<_> = data.keys.iter().collect();
    keys.sort_by_key(|(k, _)| *k);
    for (_, key) in keys {
        let path = hex(key.name.as_bytes());
        text.push_str(&format!("K\t{path}\t{}\n", key.last_write));
        let mut values: Vec<_> = key.values.iter().collect();
        values.sort_by_key(|(n, _)| *n);
        for (_, value) in values {
            text.push_str(&format!(
                "V\t{path}\t{}\t{}\t{}\n",
                hex(value.name.as_bytes()),
                value.kind,
                hex(&value.bytes)
            ));
        }
    }
    text
}

/// Empreinte du fichier pour savoir si une autre appli l'a modifié.
type Stamp = Option<(u64, i64, i64, u64)>;

fn stamp(path: &Path) -> Stamp {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    Some((m.ino(), m.mtime(), m.mtime_nsec(), m.len()))
}

struct Cache {
    path: PathBuf,
    stamp: Stamp,
    data: Data,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

fn load_from_disk(path: &Path) -> Result<Data, i32> {
    let mut text = String::new();
    match File::open(path) {
        Ok(mut f) => {
            f.read_to_string(&mut text)
                .map_err(|_| ERROR_ACCESS_DENIED)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(defaults()),
        Err(_) => return Err(ERROR_ACCESS_DENIED),
    }
    parse(&text)
}

fn current(cache: &mut Option<Cache>) -> Result<&Data, i32> {
    let path = store_path();
    let now = stamp(&path);
    let fresh = matches!(cache, Some(c) if c.path == path && c.stamp == now && now.is_some());
    if !fresh {
        let data = load_from_disk(&path)?;
        *cache = Some(Cache {
            path,
            stamp: now,
            data,
        });
    }
    Ok(&cache.as_ref().expect("cache rempli").data)
}

fn file_lock(path: &Path) -> Result<File, i32> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ERROR_ACCESS_DENIED)?;
    }
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&f), libc::LOCK_EX) } != 0 {
        return Err(ERROR_ACCESS_DENIED);
    }
    Ok(f)
}

fn write_to_disk(path: &Path, data: &Data) -> Result<(), i32> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("registry.tsv");
    let temp = path.with_file_name(format!("{name}.tmp.{}", std::process::id()));
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    f.write_all(serialize(data).as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| ERROR_ACCESS_DENIED)?;
    std::fs::rename(&temp, path).map_err(|_| ERROR_ACCESS_DENIED)
}

/// Lecture cohérente du registre.
pub fn read<T>(f: impl FnOnce(&Data) -> Result<T, i32>) -> Result<T, i32> {
    let mut cache = crate::dlls::lock(&CACHE);
    f(current(&mut cache)?)
}

/// Modification atomique : relit le fichier sous verrou, applique, écrit.
pub fn write<T>(f: impl FnOnce(&mut Data) -> Result<T, i32>) -> Result<T, i32> {
    let mut cache = crate::dlls::lock(&CACHE);
    let path = store_path();
    let _lock = file_lock(&path)?;
    let mut data = load_from_disk(&path)?;
    let out = f(&mut data)?;
    write_to_disk(&path, &data)?;
    *cache = Some(Cache {
        stamp: stamp(&path),
        path,
        data,
    });
    Ok(out)
}

/// Copie de sauvegarde avant de lancer une appli (registry.tsv.bak).
pub fn backup() {
    let src = store_path();
    if std::fs::metadata(&src).is_ok_and(|m| m.len() > 0) {
        let dst = src.with_extension("tsv.bak");
        if let Err(e) = std::fs::copy(&src, &dst) {
            crate::log_warn!("registry", "sauvegarde impossible de registry.tsv : {e}");
        }
    }
}

/// Attend qu'une autre appli modifie le registre (RegNotifyChangeKeyValue).
pub fn wait_for_change(timeout: Option<std::time::Duration>) {
    let path = store_path();
    let start = stamp(&path);
    let begin = std::time::Instant::now();
    while stamp(&path) == start {
        if timeout.is_some_and(|t| begin.elapsed() >= t) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

// ── Handles ────────────────────────────────────────────────────────

static HANDLES: Mutex<Option<HashMap<usize, String>>> = Mutex::new(None);
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x4000_0000);

pub const HKEY_CURRENT_USER: usize = 0x8000_0001;
pub const HKEY_LOCAL_MACHINE: usize = 0x8000_0002;
pub const HKEY_USERS: usize = 0x8000_0003;

/// Racine prédéfinie (accepte aussi la forme étendue 0xFFFFFFFF8000000x).
pub fn predefined(handle: usize) -> Option<&'static str> {
    if handle >> 32 != 0 && handle >> 32 != 0xFFFF_FFFF {
        return None;
    }
    match handle as u32 {
        0x8000_0000 => Some(HKCR),
        0x8000_0001 => Some("hkcu"),
        0x8000_0002 => Some("hklm"),
        0x8000_0003 => Some("hku"),
        0x8000_0005 => Some("hkcc"),
        _ => None,
    }
}

pub fn handle_path(handle: usize) -> Result<String, i32> {
    if let Some(root) = predefined(handle) {
        return Ok(root.to_string());
    }
    crate::dlls::lock(&HANDLES)
        .as_ref()
        .and_then(|m| m.get(&handle).cloned())
        .ok_or(ERROR_INVALID_HANDLE)
}

pub fn new_handle(path: String) -> usize {
    let h = NEXT_HANDLE.fetch_add(4, Ordering::Relaxed);
    crate::dlls::lock(&HANDLES)
        .get_or_insert_with(HashMap::new)
        .insert(h, path);
    h
}

pub fn close_handle(handle: usize) -> bool {
    crate::dlls::lock(&HANDLES)
        .as_mut()
        .is_some_and(|m| m.remove(&handle).is_some())
}

/// Ouvre une clé existante.
pub fn open(path: &str) -> Result<usize, i32> {
    read(|d| {
        if d.key(path).is_some() {
            Ok(())
        } else {
            Err(ERROR_FILE_NOT_FOUND)
        }
    })?;
    Ok(new_handle(path.to_string()))
}

/// Crée ou ouvre une clé ; renvoie le handle et vrai si elle a été créée.
pub fn create(path: &str) -> Result<(usize, bool), i32> {
    let exists = read(|d| Ok(d.key(path).is_some()))?;
    let created = if exists {
        false
    } else {
        write(|d| Ok(d.ensure(path)))?
    };
    Ok((new_handle(path.to_string()), created))
}

/// Lecture d'une chaîne par le runtime lui-même (COM, services…).
pub fn get_string(path: &str, name: &str) -> Option<String> {
    let path = canonical(path)?;
    read(|d| {
        let v = d.get(&path, name).ok_or(ERROR_FILE_NOT_FOUND)?;
        Ok(if is_string(v.kind) {
            string_of(&v.bytes)
        } else {
            String::new()
        })
    })
    .ok()
}

#[cfg(test)]
mod tests;
