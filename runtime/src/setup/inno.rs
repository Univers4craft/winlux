//! Installateurs Inno Setup (versions 5.5 à 7.1, Unicode).
//!
//! Un `setup.exe` Inno contient une table de positions (« rDlPtS ») qui
//! désigne deux zones : `setup-0` (en-tête et listes d'entrées : fichiers,
//! raccourcis, registre…, compressées en LZMA) et `setup-1` (les fichiers,
//! en blocs LZMA/LZMA2). La disposition des structures change d'une version
//! à l'autre : elle est décrite version par version dans `inno_layout.rs`.

use super::inno_layout::LAYOUTS;
use super::lzma::{Lzma2Reader, LzmaReader};
use super::{le_u32, le_u64, utf16z, Folders, Info, Installer, Kind, Options, Stream};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::Path;

/// Nature d'un champ dans les structures d'Inno Setup.
pub enum Field {
    /// Chaîne Unicode (longueur en octets puis UTF-16).
    Str,
    /// Chaîne d'octets.
    Ansi,
    /// Valeur binaire de taille fixe.
    Fixed(usize),
    /// Ensemble de drapeaux (taille, noms des bits).
    Set(usize, &'static [&'static str]),
    /// Énumération sur un octet.
    Enum(&'static [&'static str]),
}

pub struct Layout {
    pub id: &'static str,
    /// Taille de l'en-tête de chiffrement (6.5 et plus), 0 sinon.
    pub encryption_header: usize,
    pub records: [Option<&'static [(&'static str, Field)]>; 16],
}

const R_HEADER: usize = 0;
const R_LANGUAGE: usize = 1;
const R_CUSTOM_MESSAGE: usize = 2;
const R_PERMISSION: usize = 3;
const R_TYPE: usize = 4;
const R_COMPONENT: usize = 5;
const R_TASK: usize = 6;
const R_DIR: usize = 7;
const R_ISSIG: usize = 8;
const R_FILE: usize = 9;
const R_ICON: usize = 10;
const R_INI: usize = 11;
const R_REGISTRY: usize = 12;
const R_DELETE: usize = 13;
const R_RUN: usize = 14;
const R_FILE_LOCATION: usize = 15;

const OFFSET_TABLE_ID: &[u8; 12] = b"rDlPtS\xcd\xe6\xd7\x7b\x0b\x2a";

/// Version de Windows présentée aux installateurs (Windows 10 22H2).
const WIN_VERSION: u32 = (10 << 24) | 19045;

/// Positions des zones `setup-0` et `setup-1` dans l'exécutable.
pub fn find_offsets(data: &[u8]) -> Option<(u64, u64)> {
    let mut start = 0;
    while let Some(i) = find(&data[start..], OFFSET_TABLE_ID) {
        let at = start + i;
        start = at + 1;
        let version = le_u32(data, at + 12)?;
        let (o0, o1, crc_at) = match version {
            1 => (
                le_u32(data, at + 32)? as u64,
                le_u32(data, at + 36)? as u64,
                at + 40,
            ),
            2 => (le_u64(data, at + 40)?, le_u64(data, at + 48)?, at + 60),
            _ => continue,
        };
        let crc = le_u32(data, crc_at)?;
        if crate::archive::crc32::crc32(&data[at..crc_at]) != crc {
            continue;
        }
        if (o0 as usize) < data.len() {
            return Some((o0, o1));
        }
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Une entrée lue : valeurs dans l'ordre de la disposition.
pub struct Rec {
    layout: &'static [(&'static str, Field)],
    values: Vec<Value>,
}

enum Value {
    Text(String),
    Bytes(Vec<u8>),
}

impl Rec {
    fn index(&self, name: &str) -> Option<usize> {
        self.layout.iter().position(|(n, _)| *n == name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.index(name).is_some()
    }

    /// Texte d'un champ chaîne (vide s'il n'existe pas dans cette version).
    pub fn s(&self, name: &str) -> String {
        match self.index(name).map(|i| &self.values[i]) {
            Some(Value::Text(t)) => t.clone(),
            Some(Value::Bytes(b)) => b.iter().map(|&c| c as char).collect(),
            None => String::new(),
        }
    }

    fn raw(&self, name: &str) -> Option<&[u8]> {
        match &self.values[self.index(name)?] {
            Value::Bytes(b) => Some(b),
            Value::Text(_) => None,
        }
    }

    /// Entier signé (petit-boutiste) d'un champ fixe.
    pub fn int(&self, name: &str) -> i64 {
        let Some(b) = self.raw(name) else { return 0 };
        match b.len() {
            1 => b[0] as i64,
            2 => i16::from_le_bytes([b[0], b[1]]) as i64,
            4 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
            8 => i64::from_le_bytes(b.try_into().unwrap_or([0; 8])),
            _ => 0,
        }
    }

    /// Drapeau d'un ensemble, désigné par la fin de son nom (`ChunkCompressed`
    /// trouve `floChunkCompressed` comme `foChunkCompressed`).
    pub fn flag(&self, name: &str, member: &str) -> bool {
        let Some(i) = self.index(name) else {
            return false;
        };
        let Field::Set(_, names) = &self.layout[i].1 else {
            return false;
        };
        let Some(bit) = names.iter().position(|n| strip_prefix(n) == member) else {
            return false;
        };
        let Value::Bytes(b) = &self.values[i] else {
            return false;
        };
        b.get(bit / 8)
            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
    }

    /// Nom (sans préfixe) de la valeur d'une énumération.
    pub fn enum_name(&self, name: &str) -> &'static str {
        let Some(i) = self.index(name) else { return "" };
        let Field::Enum(names) = &self.layout[i].1 else {
            return "";
        };
        let Value::Bytes(b) = &self.values[i] else {
            return "";
        };
        names
            .get(b.first().copied().unwrap_or(0) as usize)
            .map(|n| strip_prefix(n))
            .unwrap_or("")
    }

    /// Conditions de version de Windows : vrai si l'entrée s'applique.
    fn version_ok(&self) -> bool {
        if !self.has("MinVersion.NTVersion") {
            return true;
        }
        let min = self.int("MinVersion.NTVersion") as u32;
        let below = self.int("OnlyBelowVersion.NTVersion") as u32;
        if min == 0 && self.int("MinVersion.WinVersion") != 0 {
            return false; // réservé à Windows 9x
        }
        if min != 0 && WIN_VERSION < min {
            return false;
        }
        !(below != 0 && WIN_VERSION >= below)
    }
}

/// Retire le préfixe minuscule d'un nom Pascal (`floChunk` → `Chunk`).
fn strip_prefix(n: &str) -> &str {
    let i = n
        .find(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
        .unwrap_or(0);
    &n[i..]
}

fn read_rec<R: Read>(
    s: &mut Stream<R>,
    layout: &'static [(&'static str, Field)],
) -> Result<Rec, String> {
    let mut values = Vec::with_capacity(layout.len());
    for (_, f) in layout {
        match f {
            Field::Str | Field::Ansi => {
                let len = s.u32()? as usize;
                if len > 64 << 20 {
                    return Err("structure illisible".into());
                }
                let b = s.bytes(len)?;
                if matches!(f, Field::Str) {
                    if !len.is_multiple_of(2) {
                        return Err("structure illisible".into());
                    }
                    values.push(Value::Text(super::utf16_text(&b, false)));
                } else {
                    values.push(Value::Bytes(b));
                }
            }
            Field::Fixed(n) | Field::Set(n, _) => values.push(Value::Bytes(s.bytes(*n)?)),
            Field::Enum(_) => values.push(Value::Bytes(s.bytes(1)?)),
        }
    }
    Ok(Rec { layout, values })
}

/// Lit un bloc compressé (`TCompressedBlockReader`) et renvoie son contenu
/// stocké, s'il est compressé, et la position qui suit.
fn read_block(data: &[u8], off: usize) -> Result<(Vec<u8>, bool, usize), String> {
    let crc = le_u32(data, off).ok_or("bloc tronqué")?;
    let crc32 = crate::archive::crc32::crc32;
    // Taille stockée sur 8 octets (7.x) ou 4 (avant).
    let (stored, compressed, mut p) =
        if data.len() >= off + 13 && crc32(&data[off + 4..off + 13]) == crc {
            (
                le_u64(data, off + 4).unwrap_or(0),
                data[off + 12] != 0,
                off + 13,
            )
        } else if data.len() >= off + 9 && crc32(&data[off + 4..off + 9]) == crc {
            (
                le_u32(data, off + 4).unwrap_or(0) as u64,
                data[off + 8] != 0,
                off + 9,
            )
        } else {
            return Err("en-tête de bloc invalide".into());
        };
    let end = p
        .checked_add(stored as usize)
        .filter(|&e| e <= data.len())
        .ok_or("bloc tronqué")?;
    let mut out = Vec::with_capacity(stored as usize);
    while p < end {
        let ccrc = le_u32(data, p).ok_or("bloc tronqué")?;
        p += 4;
        let len = (end - p).min(4096);
        let chunk = &data[p..p + len];
        if crc32(chunk) != ccrc {
            return Err("bloc corrompu".into());
        }
        out.extend_from_slice(chunk);
        p += len;
    }
    Ok((out, compressed, end))
}

fn block_reader(stored: Vec<u8>, compressed: bool) -> Result<Box<dyn Read>, String> {
    if compressed {
        Ok(Box::new(
            LzmaReader::new(io::Cursor::new(stored)).map_err(|e| e.to_string())?,
        ))
    } else {
        Ok(Box::new(io::Cursor::new(stored)))
    }
}

/// Contenu lu d'un installateur.
pub struct Setup {
    layout: &'static Layout,
    pub header: Rec,
    pub entries: Vec<Vec<Rec>>,
    pub locations: Vec<Rec>,
    offset1: u64,
}

fn parse(data: &[u8]) -> Result<Setup, String> {
    let (o0, o1) = find_offsets(data).ok_or("table Inno Setup introuvable")?;
    let o0 = o0 as usize;
    let id_bytes = data.get(o0..o0 + 64).ok_or("installateur tronqué")?;
    let id: String = id_bytes
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let id = id.trim().to_string();
    let layout = LAYOUTS
        .iter()
        .find(|l| l.id == id)
        .ok_or_else(|| format!("version d'Inno Setup non prise en charge : {id}"))?;
    let mut off = o0 + 64;
    if layout.encryption_header > 0 {
        let use_byte = *data.get(off + 4).ok_or("installateur tronqué")?;
        if use_byte != 0 {
            return Err("installateur protégé par mot de passe : non pris en charge".into());
        }
        off += 4 + layout.encryption_header;
    }
    let (stored, compressed, next) = read_block(data, off)?;
    let mut s = Stream {
        inner: block_reader(stored, compressed)?,
    };
    let header = read_rec(
        &mut s,
        layout.records[R_HEADER].ok_or("disposition absente")?,
    )?;
    let count = |name: &str| header.int(name).max(0) as usize;
    let order = [
        (R_LANGUAGE, "NumLanguageEntries"),
        (R_CUSTOM_MESSAGE, "NumCustomMessageEntries"),
        (R_PERMISSION, "NumPermissionEntries"),
        (R_TYPE, "NumTypeEntries"),
        (R_COMPONENT, "NumComponentEntries"),
        (R_TASK, "NumTaskEntries"),
        (R_DIR, "NumDirEntries"),
        (R_ISSIG, "NumISSigKeyEntries"),
        (R_FILE, "NumFileEntries"),
        (R_ICON, "NumIconEntries"),
        (R_INI, "NumIniEntries"),
        (R_REGISTRY, "NumRegistryEntries"),
        (R_DELETE, "NumInstallDeleteEntries"),
        (R_DELETE, "NumUninstallDeleteEntries"),
        (R_RUN, "NumRunEntries"),
        (R_RUN, "NumUninstallRunEntries"),
    ];
    let mut entries = Vec::new();
    for (rec, name) in order {
        let n = count(name);
        let mut list = Vec::with_capacity(n.min(100_000));
        if n > 0 {
            let lay = layout.records[rec].ok_or("disposition absente")?;
            for _ in 0..n {
                list.push(read_rec(&mut s, lay)?);
            }
        }
        entries.push(list);
    }
    let (stored, compressed, _) = read_block(data, next)?;
    let mut s = Stream {
        inner: block_reader(stored, compressed)?,
    };
    let lay = layout.records[R_FILE_LOCATION].ok_or("disposition absente")?;
    let mut locations = Vec::new();
    for _ in 0..count("NumFileLocationEntries") {
        locations.push(read_rec(&mut s, lay)?);
    }
    Ok(Setup {
        layout,
        header,
        entries,
        locations,
        offset1: o1,
    })
}

// Index des listes dans `Setup::entries`.
const E_LANGUAGE: usize = 0;
const E_CUSTOM_MESSAGE: usize = 1;
const E_TYPE: usize = 3;
const E_COMPONENT: usize = 4;
const E_TASK: usize = 5;
const E_DIR: usize = 6;
const E_FILE: usize = 8;
const E_ICON: usize = 9;
const E_INI: usize = 10;
const E_REGISTRY: usize = 11;
const E_INSTALL_DELETE: usize = 12;
const E_UNINSTALL_DELETE: usize = 13;
const E_RUN: usize = 14;

/// Contexte d'expansion des constantes `{app}`, `{group}`…
struct Ctx {
    app: String,
    group: String,
    x64: bool,
    lang: usize,
    lang_name: String,
    src: String,
    srcexe: String,
    uninstexe: String,
    messages: HashMap<String, String>,
    components: Vec<String>,
    tasks: Vec<String>,
    app_name: String,
}

impl Ctx {
    fn folder(&self, name: &str) -> Option<String> {
        let pf = Folders { x64: self.x64 }.program_files().to_string();
        let roaming = "C:\\AppData\\Roaming";
        Some(match name.to_ascii_lowercase().as_str() {
            "app" => self.app.clone(),
            "group" => format!("{}\\{}", Folders::programs(), self.group),
            "pf" | "commonpf" | "autopf" => pf,
            "pf32" | "commonpf32" | "autopf32" => "C:\\Program Files (x86)".into(),
            "pf64" | "commonpf64" | "autopf64" => "C:\\Program Files".into(),
            "userpf" => "C:\\AppData\\Local\\Programs".into(),
            "cf" | "commoncf" | "autocf" => format!("{pf}\\Common Files"),
            "cf32" | "commoncf32" | "autocf32" => "C:\\Program Files (x86)\\Common Files".into(),
            "cf64" | "commoncf64" | "autocf64" => "C:\\Program Files\\Common Files".into(),
            "usercf" => "C:\\AppData\\Local\\Programs\\Common".into(),
            "win" => "C:\\windows".into(),
            "sys" | "sysnative" => "C:\\windows\\system32".into(),
            "syswow64" => "C:\\windows\\syswow64".into(),
            "fonts" | "commonfonts" | "autofonts" => "C:\\windows\\Fonts".into(),
            "userfonts" => "C:\\AppData\\Local\\Microsoft\\Windows\\Fonts".into(),
            "tmp" => Folders::temp().into(),
            "src" => self.src.clone(),
            "srcexe" => self.srcexe.clone(),
            "uninstallexe" => self.uninstexe.clone(),
            "sd" => "C:".into(),
            "userdesktop" | "commondesktop" | "autodesktop" | "desktop" => Folders::desktop(),
            "userprograms" | "commonprograms" | "autoprograms" => Folders::programs(),
            "userstartmenu" | "commonstartmenu" | "autostartmenu" => Folders::start_menu().into(),
            "userstartup" | "commonstartup" | "autostartup" => {
                format!("{}\\Startup", Folders::programs())
            }
            "userappdata" | "appdata" => roaming.into(),
            "commonappdata" | "autoappdata" => "C:\\ProgramData".into(),
            "localappdata" => "C:\\AppData\\Local".into(),
            "userdocs" | "commondocs" | "autodocs" | "mydocs" => "C:\\Documents".into(),
            "userfavorites" | "commonfavorites" | "autofavorites" => "C:\\Favorites".into(),
            "usertemplates" | "commontemplates" | "autotemplates" => {
                format!("{roaming}\\Microsoft\\Windows\\Templates")
            }
            "usersendto" | "sendto" => format!("{roaming}\\Microsoft\\Windows\\SendTo"),
            "userinfopath" => format!("{roaming}\\Microsoft\\InfoPath"),
            "dao" => format!("{pf}\\Common Files\\Microsoft Shared\\DAO"),
            "dotnet20" | "dotnet40" | "dotnet2032" | "dotnet2064" | "dotnet4032" | "dotnet4064" => {
                "C:\\windows\\Microsoft.NET\\Framework64\\v4.0.30319".into()
            }
            "computername" => "WINLUX".into(),
            "username" => std::env::var("USER").unwrap_or_else(|_| "winlux".into()),
            "groupname" => self.group.clone(),
            "language" => self.lang_name.clone(),
            "hwnd" | "wizardhwnd" => "0".into(),
            "sysuserinfoname" | "userinfoname" => {
                std::env::var("USER").unwrap_or_else(|_| "winlux".into())
            }
            "sysuserinfoorg" | "userinfoorg" | "userinfoserial" => String::new(),
            "log" => String::new(),
            "olddata" => String::new(),
            "\\" => "\\".into(),
            _ => return None,
        })
    }

    /// Développe les constantes Inno (`{app}\x`, `{cm:Nom}`, `{%VAR|déf}`…).
    fn expand(&self, s: &str) -> String {
        let mut out = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c != '{' {
                out.push(c);
                i += 1;
                continue;
            }
            if chars.get(i + 1) == Some(&'{') {
                out.push('{');
                i += 2;
                continue;
            }
            // Constante jusqu'à l'accolade fermante correspondante.
            let mut depth = 0;
            let mut j = i;
            while j < chars.len() {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
            out.push_str(&self.constant(&inner));
            i = j + 1;
        }
        out
    }

    fn constant(&self, c: &str) -> String {
        if let Some(v) = self.folder(c) {
            return v;
        }
        if let Some(rest) = c.strip_prefix("cm:") {
            let mut parts = rest.split(',');
            let name = parts.next().unwrap_or("").trim().to_lowercase();
            let mut msg = self.messages.get(&name).cloned().unwrap_or_default();
            for (n, arg) in parts.enumerate() {
                msg = msg.replace(&format!("%{}", n + 1), &self.expand(arg));
            }
            return msg;
        }
        if let Some(rest) = c.strip_prefix('%') {
            let (var, def) = rest.split_once('|').unwrap_or((rest, ""));
            return std::env::var(var).unwrap_or_else(|_| self.expand(def));
        }
        if let Some(rest) = c.strip_prefix("reg:") {
            // {reg:HKxx\Clé,Valeur|défaut}
            let (path, def) = rest.split_once('|').unwrap_or((rest, ""));
            if let Some((key, name)) = path.split_once(',') {
                if let Some((root, sub)) = key.split_once('\\') {
                    let k = super::reg_key(root_name(root), &self.expand(sub));
                    if let Some(b) = crate::dlls::advapi32::read_registry_value_internal(&k, name) {
                        return super::utf16_text(&b, true);
                    }
                }
            }
            return self.expand(def);
        }
        for pre in ["param:", "ini:", "drive:", "code:"] {
            if let Some(rest) = c.strip_prefix(pre) {
                let def = rest.split_once('|').map(|x| x.1).unwrap_or("");
                return if pre == "drive:" {
                    "C:".into()
                } else {
                    self.expand(def)
                };
            }
        }
        String::new()
    }

    /// Entrée retenue (composants, tâches, langues, versions, Check).
    fn wanted(&self, r: &Rec, default: bool) -> bool {
        let ok = self.wanted_inner(r, default);
        if std::env::var_os("WINLUX_SETUP_DEBUG").is_some() {
            let first = r.values.first().map(|v| match v {
                Value::Text(t) => t.clone(),
                Value::Bytes(_) => String::new(),
            });
            eprintln!(
                "setup : {} {:?} composants={:?} tâches={:?} check={:?}",
                if ok { "+" } else { "-" },
                first.unwrap_or_default(),
                r.s("Components"),
                r.s("Tasks"),
                r.s("Check")
            );
        }
        ok
    }

    fn wanted_inner(&self, r: &Rec, default: bool) -> bool {
        if !r.version_ok() {
            return false;
        }
        let langs = r.s("Languages");
        if !langs.trim().is_empty()
            && !langs
                .split_whitespace()
                .any(|l| l.eq_ignore_ascii_case(&self.lang_name))
        {
            return false;
        }
        let comps = r.s("Components");
        if !comps.trim().is_empty() && !eval(&comps, &self.components) {
            return false;
        }
        let tasks = r.s("Tasks");
        if !tasks.trim().is_empty() && !eval(&tasks, &self.tasks) {
            return false;
        }
        self.check(&r.s("Check"), default)
    }

    /// Expression `Check` : fonctions usuelles évaluées, les fonctions du
    /// script Pascal de l'installateur valent `default`.
    fn check(&self, expr: &str, default: bool) -> bool {
        let toks = tokenize(expr);
        if toks.is_empty() {
            return true;
        }
        let mut i = 0;
        self.c_or(&toks, &mut i).unwrap_or(default)
    }

    fn c_or(&self, t: &[Tok], i: &mut usize) -> Option<bool> {
        let mut v = self.c_and(t, i);
        while matches!(t.get(*i), Some(Tok::Id(x)) if x == "or") {
            *i += 1;
            let r = self.c_and(t, i);
            v = match (v, r) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            };
        }
        v
    }

    fn c_and(&self, t: &[Tok], i: &mut usize) -> Option<bool> {
        let mut v = self.c_not(t, i);
        while matches!(t.get(*i), Some(Tok::Id(x)) if x == "and") {
            *i += 1;
            let r = self.c_not(t, i);
            v = match (v, r) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            };
        }
        v
    }

    fn c_not(&self, t: &[Tok], i: &mut usize) -> Option<bool> {
        match t.get(*i) {
            Some(Tok::Id(x)) if x == "not" => {
                *i += 1;
                self.c_not(t, i).map(|v| !v)
            }
            Some(Tok::LParen) => {
                *i += 1;
                let v = self.c_or(t, i);
                if matches!(t.get(*i), Some(Tok::RParen)) {
                    *i += 1;
                }
                v
            }
            Some(Tok::Id(name)) => {
                let name = name.clone();
                *i += 1;
                let mut args = Vec::new();
                if matches!(t.get(*i), Some(Tok::LParen)) {
                    *i += 1;
                    let mut depth = 0;
                    let mut cur = String::new();
                    let mut call: Option<String> = None;
                    while let Some(tok) = t.get(*i) {
                        *i += 1;
                        match tok {
                            Tok::LParen => depth += 1,
                            Tok::RParen if depth == 0 => break,
                            Tok::RParen => depth -= 1,
                            Tok::Comma if depth == 0 => {
                                args.push(std::mem::take(&mut cur));
                                call = None;
                            }
                            Tok::Str(sv) => {
                                cur = if call.as_deref() == Some("expandconstant") {
                                    self.expand(sv)
                                } else {
                                    sv.clone()
                                }
                            }
                            Tok::Id(f) => call = Some(f.clone()),
                            Tok::Comma => {}
                        }
                    }
                    args.push(cur);
                }
                self.function(&name, &args)
            }
            _ => {
                *i += 1;
                None
            }
        }
    }

    fn function(&self, name: &str, args: &[String]) -> Option<bool> {
        let arg = || args.first().map(|a| a.to_lowercase()).unwrap_or_default();
        Some(match name {
            "true" => true,
            "false" => false,
            "is64bitinstallmode" => self.x64,
            "iswin64"
            | "isadmin"
            | "isadmininstallmode"
            | "isadminloggedon"
            | "ispoweruserloggedon" => true,
            "isx64" | "isx64os" | "isx64compatible" | "isx86compatible" => true,
            "isx86" | "isx86os" | "isarm64" | "isarm32compatible" | "isia64" | "isarm32" => false,
            "wizardnoicons" | "wizardsilent" | "isuninstaller" | "wizardisrestarting" => false,
            "iscomponentselected" | "wizardiscomponentselected" => eval(&arg(), &self.components),
            "istaskselected" | "wizardistaskselected" => eval(&arg(), &self.tasks),
            "fileexists" | "fileorfolderexists" => {
                crate::paths::to_linux(&args.first().cloned().unwrap_or_default()).exists()
            }
            "direxists" => {
                crate::paths::to_linux(&args.first().cloned().unwrap_or_default()).is_dir()
            }
            _ => return None,
        })
    }
}

enum Tok {
    Id(String),
    Str(String),
    LParen,
    RParen,
    Comma,
}

fn tokenize(expr: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => out.push(Tok::LParen),
            ')' => out.push(Tok::RParen),
            ',' => out.push(Tok::Comma),
            '\'' => {
                let mut sv = String::new();
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            sv.push('\'');
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    sv.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Str(sv));
            }
            c if c.is_alphanumeric() || c == '_' => {
                let mut id = String::new();
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    id.push(chars[i].to_ascii_lowercase());
                    i += 1;
                }
                out.push(Tok::Id(id));
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn root_name(root: &str) -> &'static str {
    match root.to_ascii_uppercase().as_str() {
        "HKCR" | "HKEY_CLASSES_ROOT" => "hkcr",
        "HKCU" | "HKEY_CURRENT_USER" => "hkcu",
        "HKU" | "HKEY_USERS" => "hku",
        _ => "hklm",
    }
}

/// Évalue une expression de composants/tâches (`a or b\c and not d`).
fn eval(expr: &str, selected: &[String]) -> bool {
    let tokens: Vec<String> = expr
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('!', " not ")
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    // Grammaire : or > and > not > atome.
    fn or(t: &[String], i: &mut usize, sel: &[String]) -> bool {
        let mut v = and(t, i, sel);
        while t.get(*i).map(String::as_str) == Some("or") {
            *i += 1;
            let r = and(t, i, sel);
            v = v || r;
        }
        v
    }
    fn and(t: &[String], i: &mut usize, sel: &[String]) -> bool {
        let mut v = not(t, i, sel);
        loop {
            match t.get(*i).map(String::as_str) {
                Some("and") => {
                    *i += 1;
                    let r = not(t, i, sel);
                    v = v && r;
                }
                // Liste séparée par des espaces : « ou » implicite.
                Some(x) if x != "or" && x != ")" => {
                    let r = not(t, i, sel);
                    v = v || r;
                }
                _ => break,
            }
        }
        v
    }
    fn not(t: &[String], i: &mut usize, sel: &[String]) -> bool {
        match t.get(*i).map(String::as_str) {
            Some("not") => {
                *i += 1;
                !not(t, i, sel)
            }
            Some("(") => {
                *i += 1;
                let v = or(t, i, sel);
                if t.get(*i).map(String::as_str) == Some(")") {
                    *i += 1;
                }
                v
            }
            Some(name) => {
                *i += 1;
                sel.iter().any(|s| s == name)
            }
            None => false,
        }
    }
    let mut i = 0;
    or(&tokens, &mut i, selected)
}

/// Choix de la langue : celle de l'utilisateur si l'installateur la propose.
fn pick_language(setup: &Setup) -> usize {
    let want = super::user_language();
    let langs = &setup.entries[E_LANGUAGE];
    langs
        .iter()
        .position(|l| (l.int("LanguageID") as u16) == want)
        .or_else(|| {
            langs
                .iter()
                .position(|l| (l.int("LanguageID") as u16) & 0x3FF == want & 0x3FF)
        })
        .or_else(|| {
            langs
                .iter()
                .position(|l| l.int("LanguageID") as u16 == 0x0409)
        })
        .unwrap_or(0)
}

fn make_ctx(setup: &Setup, path: &Path, opts: &Options) -> Ctx {
    let h = &setup.header;
    let lang = pick_language(setup);
    let lang_name = setup.entries[E_LANGUAGE]
        .get(lang)
        .map(|l| l.s("Name"))
        .unwrap_or_default();
    let mut messages = HashMap::new();
    for m in &setup.entries[E_CUSTOM_MESSAGE] {
        let idx = m.int("LangIndex");
        if idx == -1 || idx as usize == lang {
            let name = m.s("Name").to_lowercase();
            if idx == -1 && messages.contains_key(&name) {
                continue;
            }
            messages.insert(name, m.s("Value"));
        }
    }
    // Mode 64 bits : expression (6.3+) ou ensemble d'architectures.
    let x64 = if h.has("ArchitecturesInstallIn64BitMode") {
        let s = h.s("ArchitecturesInstallIn64BitMode").to_ascii_lowercase();
        !s.is_empty() && (s.contains("x64") || s.contains("win64"))
            || h.flag("ArchitecturesInstallIn64BitMode", "X64")
    } else {
        false
    };
    let srcexe = crate::paths::to_windows(path);
    let src = srcexe
        .rsplit_once('\\')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();
    // Type d'installation par défaut : le premier proposé.
    let types: Vec<&Rec> = setup.entries[E_TYPE]
        .iter()
        .filter(|t| t.version_ok())
        .collect();
    let typ = types.first().map(|t| t.s("Name").to_lowercase());
    let mut components = Vec::new();
    for c in &setup.entries[E_COMPONENT] {
        let ctypes = c.s("Types").to_lowercase();
        let on = match &typ {
            Some(t) => ctypes.split_whitespace().any(|x| x == t),
            None => true,
        } || c.flag("Options", "Fixed");
        if on && c.version_ok() {
            components.push(c.s("Name").to_lowercase());
        }
    }
    let mut ctx = Ctx {
        app: String::new(),
        group: String::new(),
        x64,
        lang,
        lang_name,
        src,
        srcexe,
        uninstexe: String::new(),
        messages,
        components,
        tasks: Vec::new(),
        app_name: h.s("AppName"),
    };
    // Tâches cochées par défaut (pas de raccourci Bureau si demandé).
    let mut tasks = Vec::new();
    for t in &setup.entries[E_TASK] {
        let name = t.s("Name").to_lowercase();
        let comps = t.s("Components");
        let comp_ok = comps.trim().is_empty() || eval(&comps, &ctx.components);
        let desktop = name.contains("desktop");
        if !t.flag("Options", "Unchecked")
            && comp_ok
            && t.version_ok()
            && ctx.check(&t.s("Check"), true)
            && !(desktop && opts.no_desktop)
        {
            tasks.push(name);
        }
    }
    ctx.tasks = tasks;
    ctx.app_name = ctx.expand(&ctx.app_name);
    ctx.group = ctx.expand(&h.s("DefaultGroupName"));
    ctx.app = match &opts.dir {
        Some(d) => d.clone(),
        None => ctx.expand(&h.s("DefaultDirName")),
    };
    ctx.app = ctx.app.trim_end_matches('\\').to_string();
    ctx
}

pub fn info(data: &[u8]) -> Result<Info, String> {
    let setup = parse(data)?;
    let ctx = make_ctx(&setup, Path::new("setup.exe"), &Options::default());
    Ok(Info {
        kind: Kind::Inno,
        name: ctx.app_name.clone(),
        version: ctx.expand(&setup.header.s("AppVersion")),
        publisher: ctx.expand(&setup.header.s("AppPublisher")),
        default_dir: ctx.app.clone(),
    })
}

/// Décompresseur d'un bloc de fichiers de `setup-1`.
fn chunk_reader<'a>(
    setup: &Setup,
    data: &'a [u8],
    loc: &Rec,
) -> Result<Box<dyn Read + 'a>, String> {
    if loc.flag("Flags", "ChunkEncrypted") {
        return Err("fichiers chiffrés (mot de passe) : non pris en charge".into());
    }
    let start = setup
        .offset1
        .checked_add(loc.int("StartOffset") as u64)
        .ok_or("position invalide")? as usize;
    let size = loc.int("ChunkCompressedSize") as usize;
    if data.get(start..start + 4) != Some(b"zlb\x1a") {
        return Err("bloc de fichiers invalide".into());
    }
    let raw = data
        .get(start + 4..start + 4 + size)
        .ok_or("installateur tronqué (fichiers externes .bin non pris en charge)")?;
    if !loc.flag("Flags", "ChunkCompressed") {
        return Ok(Box::new(raw));
    }
    match setup.header.enum_name("CompressMethod") {
        "LZMA" => Ok(Box::new(LzmaReader::new(raw).map_err(|e| e.to_string())?)),
        "LZMA2" => Ok(Box::new(Lzma2Reader::new(raw).map_err(|e| e.to_string())?)),
        "Zip" => Ok(Box::new(crate::archive::inflate::Inflater::new(
            raw.get(2..).ok_or("bloc zlib invalide")?,
        ))),
        "Stored" => Ok(Box::new(raw)),
        m => Err(format!("compression « {m} » non prise en charge")),
    }
}

/// Filtre « CALL » d'Inno (version 3), par tampons de 64 Kio.
fn untransform_calls(buf: &mut [u8], addr_offset: u32) {
    if buf.len() < 5 {
        return;
    }
    let size = buf.len() - 4;
    let mut i = 0;
    while i < size {
        if buf[i] == 0xE8 || buf[i] == 0xE9 {
            i += 1;
            if buf[i + 3] == 0 || buf[i + 3] == 0xFF {
                let addr = (addr_offset.wrapping_add(i as u32 + 4)) & 0xFF_FFFF;
                let mut rel = buf[i] as u32 | (buf[i + 1] as u32) << 8 | (buf[i + 2] as u32) << 16;
                rel = rel.wrapping_sub(addr);
                if rel & 0x80_0000 != 0 {
                    buf[i + 3] = !buf[i + 3];
                }
                buf[i] = rel as u8;
                buf[i + 1] = (rel >> 8) as u8;
                buf[i + 2] = (rel >> 16) as u8;
            }
            i += 4;
        } else {
            i += 1;
        }
    }
}

/// Copie `size` octets du bloc vers `out` (filtre CALL éventuel).
fn copy_file(r: &mut dyn Read, size: u64, calls: bool, out: &mut dyn Write) -> io::Result<()> {
    let mut left = size;
    let mut buf = vec![0u8; 65536];
    let mut addr: u32 = 0;
    while left > 0 {
        let n = (left as usize).min(buf.len());
        r.read_exact(&mut buf[..n])?;
        if calls {
            untransform_calls(&mut buf[..n], addr);
            addr = addr.wrapping_add(n as u32);
        }
        out.write_all(&buf[..n])?;
        left -= n as u64;
    }
    Ok(())
}

fn skip(r: &mut dyn Read, mut n: u64) -> io::Result<()> {
    let mut buf = vec![0u8; 65536];
    while n > 0 {
        let k = (n as usize).min(buf.len());
        r.read_exact(&mut buf[..k])?;
        n -= k as u64;
    }
    Ok(())
}

fn hive(root: i64, x64_admin: bool) -> &'static str {
    match root as u32 & 0x7FFF_FFFF {
        0 => "hkcr",
        1 => "hkcu",
        3 => "hku",
        5 => "hkcc",
        2 => "hklm",
        _ if x64_admin => "hklm",
        _ => "hklm",
    }
}

pub fn install(
    data: &[u8],
    path: &Path,
    opts: &Options,
    inst: &mut Installer,
) -> Result<(), String> {
    let setup = parse(data)?;
    let mut ctx = make_ctx(&setup, path, opts);
    let h = &setup.header;
    let app_id = {
        let id = ctx.expand(&h.s("AppId"));
        if id.is_empty() {
            ctx.app_name.clone()
        } else {
            id
        }
    };
    let version = ctx.expand(&h.s("AppVersion"));
    let publisher = ctx.expand(&h.s("AppPublisher"));
    inst.log.log(&format!(
        "{} {version} (Inno Setup, format « {} ») vers {}",
        ctx.app_name, setup.layout.id, ctx.app
    ));
    inst.manifest.name = ctx.app_name.clone();
    inst.manifest.version = version.clone();
    inst.manifest.publisher = publisher.clone();
    inst.manifest.location = ctx.app.clone();
    inst.manifest.id = format!("{app_id}_is1");

    // Désinstalleur : {app}\unins000.exe (UninstallFilesDir).
    let uninstallable = {
        let u = h.s("Uninstallable").to_ascii_lowercase();
        !(u == "no" || u == "false" || u == "0")
    };
    let unins_dir = {
        let d = ctx.expand(&h.s("UninstallFilesDir"));
        if d.is_empty() {
            ctx.app.clone()
        } else {
            d
        }
    };
    ctx.uninstexe = format!("{unins_dir}\\unins000.exe");

    // Suppressions demandées avant l'installation.
    for d in &setup.entries[E_INSTALL_DELETE] {
        if ctx.wanted(d, false) {
            let kind = d.int("DeleteType") as u8;
            super::delete_pattern(&ctx.expand(&d.s("Name")), kind);
        }
    }
    inst.mkdirs_win(&ctx.app)
        .map_err(|e| format!("{} : {e}", ctx.app))?;
    for d in &setup.entries[E_DIR] {
        if ctx.wanted(d, false) {
            let dir = ctx.expand(&d.s("DirName"));
            if !d.flag("Options", "DeleteAfterInstall") {
                inst.mkdirs_win(&dir).map_err(|e| format!("{dir} : {e}"))?;
            }
        }
    }

    // Fichiers : regroupés par bloc compressé, dans l'ordre du bloc.
    let mut wanted_files: HashMap<usize, Vec<String>> = HashMap::new();
    let mut installed = 0usize;
    for f in &setup.entries[E_FILE] {
        if !ctx.wanted(f, true) {
            continue;
        }
        let dest = ctx.expand(&f.s("DestName"));
        if f.enum_name("FileType") == "UninstExe" {
            continue;
        }
        if f.flag("Options", "DeleteAfterInstall")
            || dest
                .to_ascii_lowercase()
                .starts_with(&Folders::temp().to_ascii_lowercase())
        {
            continue;
        }
        if f.flag("Options", "OnlyIfDestFileExists") && !crate::paths::to_linux(&dest).exists() {
            continue;
        }
        if f.flag("Options", "OnlyIfDoesntExist") && crate::paths::to_linux(&dest).exists() {
            continue;
        }
        let loc = f.int("LocationEntry");
        if loc < 0 {
            // Fichier externe (copié depuis le dossier de l'installateur).
            let src = crate::paths::to_linux(&ctx.expand(&f.s("SourceFilename")));
            if let Ok(bytes) = std::fs::read(&src) {
                inst.write_file(&dest, &bytes)
                    .map_err(|e| format!("{dest} : {e}"))?;
                installed += 1;
            }
            continue;
        }
        wanted_files.entry(loc as usize).or_default().push(dest);
    }
    let mut locs: Vec<usize> = wanted_files.keys().copied().collect();
    locs.sort_by_key(|&i| {
        let l = &setup.locations[i];
        (
            l.int("FirstSlice"),
            l.int("StartOffset"),
            l.int("ChunkSuboffset"),
        )
    });
    let mut current: Option<(i64, i64, Box<dyn Read>, u64)> = None;
    for i in locs {
        let loc = setup.locations.get(i).ok_or("entrée de fichier invalide")?;
        let key = (loc.int("FirstSlice"), loc.int("StartOffset"));
        let sub = loc.int("ChunkSuboffset") as u64;
        let reuse = matches!(&current, Some((a, b, _, pos)) if (*a, *b) == key && *pos <= sub);
        if !reuse {
            current = Some((key.0, key.1, chunk_reader(&setup, data, loc)?, 0));
        }
        let Some((_, _, reader, pos)) = current.as_mut() else {
            continue;
        };
        skip(reader.as_mut(), sub - *pos).map_err(|e| format!("bloc de fichiers : {e}"))?;
        let size = loc.int("OriginalSize") as u64;
        let dests = &wanted_files[&i];
        let first = &dests[0];
        let (p, mut file) = inst
            .create_file(first)
            .map_err(|e| format!("{first} : {e}"))?;
        copy_file(
            reader.as_mut(),
            size,
            loc.flag("Flags", "CallInstructionOptimized"),
            &mut file,
        )
        .map_err(|e| format!("{first} : {e}"))?;
        drop(file);
        *pos = sub + size;
        installed += 1;
        for other in &dests[1..] {
            let target = crate::paths::to_linux(other);
            if let Some(parent) = target.parent() {
                inst.mkdirs(parent).map_err(|e| format!("{other} : {e}"))?;
            }
            std::fs::copy(&p, &target).map_err(|e| format!("{other} : {e}"))?;
            inst.note_file(&target);
            installed += 1;
        }
    }
    inst.log.log(&format!("{installed} fichiers installés"));

    // Raccourcis.
    for icon in &setup.entries[E_ICON] {
        if !ctx.wanted(icon, false) {
            continue;
        }
        let name = ctx.expand(&icon.s("IconName"));
        let target = ctx.expand(&icon.s("Filename"));
        if icon.flag("Options", "CreateOnlyIfFileExists")
            && !crate::paths::to_linux(&target).exists()
        {
            continue;
        }
        let link = format!("{name}.lnk");
        let workdir = ctx.expand(&icon.s("WorkingDir"));
        if let Err(e) = inst.shortcut(
            &link,
            &target,
            &ctx.expand(&icon.s("Parameters")),
            &workdir,
            &ctx.expand(&icon.s("Comment")),
        ) {
            inst.log.log(&format!("raccourci {link} : {e}"));
        }
    }

    // Fichiers INI.
    for e in &setup.entries[E_INI] {
        if !ctx.wanted(e, false) {
            continue;
        }
        let file = ctx.expand(&e.s("Filename"));
        let section = ctx.expand(&e.s("Section"));
        let key = ctx.expand(&e.s("Entry"));
        let value = ctx.expand(&e.s("Value"));
        if let Err(err) = write_ini(inst, &file, &section, &key, &value) {
            inst.log.log(&format!("{file} : {err}"));
        }
    }

    // Registre.
    for r in &setup.entries[E_REGISTRY] {
        if !ctx.wanted(r, false) {
            continue;
        }
        let root = hive(r.int("RootKey"), true);
        let key = super::reg_key(root, &ctx.expand(&r.s("Subkey")));
        if r.flag("Options", "DeleteKey") {
            inst.reg_delete_tree(&key);
        }
        if r.flag("Options", "UninsDeleteEntireKey") && !inst.manifest.trees.contains(&key) {
            inst.manifest.trees.push(key.clone());
        }
        let name = ctx.expand(&r.s("ValueName"));
        if r.flag("Options", "DeleteValue") {
            inst.reg_delete_value(&key, &name);
        }
        let track = r.flag("Options", "UninsDeleteValue") || r.flag("Options", "UninsClearValue");
        let raw = r.s("ValueData");
        match r.enum_name("Typ") {
            "None" => inst.reg_note_key(&key),
            "String" => inst.reg_set(&key, &name, 1, utf16z(&ctx.expand(&raw)), track),
            "ExpandString" => inst.reg_set(&key, &name, 2, utf16z(&ctx.expand(&raw)), track),
            "MultiString" => {
                let mut bytes = Vec::new();
                for part in ctx.expand(&raw).split('\0').filter(|p| !p.is_empty()) {
                    bytes.extend(utf16z(part));
                }
                bytes.extend([0, 0]);
                inst.reg_set(&key, &name, 7, bytes, track)
            }
            "DWord" => {
                let v = parse_int(&ctx.expand(&raw)) as u32;
                inst.reg_set(&key, &name, 4, v.to_le_bytes().to_vec(), track)
            }
            "QWord" => {
                let v = parse_int(&ctx.expand(&raw));
                inst.reg_set(&key, &name, 11, v.to_le_bytes().to_vec(), track)
            }
            "Binary" => {
                let bytes = raw.chars().map(|c| c as u32 as u8).collect();
                inst.reg_set(&key, &name, 3, bytes, track)
            }
            _ => {}
        }
    }

    // Suppressions à faire à la désinstallation.
    for d in &setup.entries[E_UNINSTALL_DELETE] {
        if ctx.wanted(d, false) {
            inst.manifest
                .deletes
                .push((ctx.expand(&d.s("Name")), d.int("DeleteType") as u8));
        }
    }
    for r in &setup.entries[E_RUN] {
        if ctx.wanted(r, false) && !r.flag("Options", "PostInstall") {
            inst.log.log(&format!(
                "programme d'installation annexe ignoré : {} {}",
                ctx.expand(&r.s("Name")),
                ctx.expand(&r.s("Parameters"))
            ));
        }
    }

    // Désinstalleur et inscription dans « Applications installées ».
    if uninstallable {
        // Le désinstalleur est reconnu par son chemin (manifeste) : on y
        // écrit le chargeur de l'installateur, sans les données.
        let o0 = find_offsets(data).map(|o| o.0 as usize).unwrap_or(0);
        let stub = &data[..data.len().min(o0)];
        let p = inst
            .write_file(&ctx.uninstexe, stub)
            .map_err(|e| format!("{} : {e}", ctx.uninstexe))?;
        inst.manifest.files.retain(|f| f != &p);
        inst.manifest.uninstaller = Some(p);
        let create_key = {
            let c = h.s("CreateUninstallRegKey").to_ascii_lowercase();
            !(c == "no" || c == "false" || c == "0")
        };
        if create_key {
            let key = format!(
                "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{}",
                inst.manifest.id.to_lowercase()
            );
            inst.manifest.uninstall_key = key.clone();
            let display = {
                let d = ctx.expand(&h.s("UninstallDisplayName"));
                if !d.is_empty() {
                    d
                } else {
                    let v = ctx.expand(&h.s("AppVerName"));
                    if v.is_empty() {
                        ctx.app_name.clone()
                    } else {
                        v
                    }
                }
            };
            let icon = {
                let i = ctx.expand(&h.s("UninstallDisplayIcon"));
                if i.is_empty() {
                    ctx.uninstexe.clone()
                } else {
                    i
                }
            };
            let size_kb: u64 = inst
                .manifest
                .files
                .iter()
                .filter_map(|f| f.metadata().ok())
                .map(|m| m.len())
                .sum::<u64>()
                / 1024;
            let unins = ctx.uninstexe.clone();
            let pairs = [
                ("DisplayName", display.clone()),
                ("DisplayVersion", version.clone()),
                ("Publisher", publisher.clone()),
                ("InstallLocation", format!("{}\\", ctx.app)),
                ("DisplayIcon", icon),
                ("URLInfoAbout", ctx.expand(&h.s("AppPublisherURL"))),
                ("HelpLink", ctx.expand(&h.s("AppSupportURL"))),
                ("URLUpdateInfo", ctx.expand(&h.s("AppUpdatesURL"))),
                ("Comments", ctx.expand(&h.s("AppComments"))),
                ("Contact", ctx.expand(&h.s("AppContact"))),
                ("UninstallString", format!("\"{unins}\"")),
                ("QuietUninstallString", format!("\"{unins}\" /SILENT")),
                ("Inno Setup: App Path", ctx.app.clone()),
                ("Inno Setup: Icon Group", ctx.group.clone()),
                ("Inno Setup: Language", ctx.lang_name.clone()),
                (
                    "Inno Setup: Setup Version",
                    setup.layout.id[23..]
                        .trim_end_matches(')')
                        .replace(") (u", "")
                        .to_string(),
                ),
            ];
            for (n, v) in pairs {
                if !v.is_empty() {
                    inst.reg_str(&key, n, &v);
                }
            }
            inst.reg_dword(&key, "EstimatedSize", size_kb.min(u32::MAX as u64) as u32);
            inst.reg_dword(&key, "NoModify", 1);
            inst.reg_dword(&key, "NoRepair", 1);
            let _ = ctx.lang;
        }
    }
    Ok(())
}

fn parse_int(s: &str) -> u64 {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("$").or_else(|| s.strip_prefix("0x")) {
        return u64::from_str_radix(h, 16).unwrap_or(0);
    }
    s.parse::<i64>().map(|v| v as u64).unwrap_or(0)
}

/// Écrit `clé=valeur` dans une section d'un fichier INI (créé au besoin).
pub fn write_ini(
    inst: &mut Installer,
    file: &str,
    section: &str,
    key: &str,
    value: &str,
) -> io::Result<()> {
    let path = crate::paths::to_linux(file);
    let existed = path.exists();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = format!("[{section}]");
    let start = lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case(&header));
    let entry = format!("{key}={value}");
    match start {
        Some(s) => {
            let end = lines[s + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map(|e| s + 1 + e)
                .unwrap_or(lines.len());
            let found = lines[s + 1..end].iter().position(|l| {
                l.split_once('=')
                    .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
            });
            if key.is_empty() {
            } else if let Some(f) = found {
                lines[s + 1 + f] = entry;
            } else {
                lines.insert(end, entry);
            }
        }
        None => {
            lines.push(header);
            if !key.is_empty() {
                lines.push(entry);
            }
        }
    }
    let body = lines.join("\r\n") + "\r\n";
    if existed {
        std::fs::write(&path, body)
    } else {
        inst.write_file(file, body.as_bytes()).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        let sel = vec!["main".to_string(), "help\\fr".to_string()];
        assert!(eval("main", &sel));
        assert!(eval("extra or main", &sel));
        assert!(!eval("main and not help\\fr", &sel));
        assert!(eval("extra main", &sel));
        assert!(eval("!extra", &sel));
        assert!(!eval("(extra or other) and main", &sel));
    }

    #[test]
    fn calls_filter_roundtrip() {
        // Instruction CALL relative -0x10 placée à l'octet 3.
        let mut buf = vec![0x90, 0x90, 0x90, 0xE8, 0xF0, 0xFF, 0xFF, 0xFF, 0x90, 0x90];
        let orig = buf.clone();
        // Encodage (inverse du décodage) tel que l'applique le compilateur.
        let i = 4usize;
        let addr = (i as u32 + 4) & 0xFF_FFFF;
        let mut rel = buf[i] as u32 | (buf[i + 1] as u32) << 8 | (buf[i + 2] as u32) << 16;
        if rel & 0x80_0000 != 0 {
            buf[i + 3] = !buf[i + 3];
        }
        rel = rel.wrapping_add(addr);
        buf[i] = rel as u8;
        buf[i + 1] = (rel >> 8) as u8;
        buf[i + 2] = (rel >> 16) as u8;
        untransform_calls(&mut buf, 0);
        assert_eq!(buf, orig);
    }

    #[test]
    fn every_layout_has_core_records() {
        for l in LAYOUTS {
            for r in [R_HEADER, R_FILE, R_FILE_LOCATION, R_REGISTRY, R_ICON] {
                assert!(l.records[r].is_some(), "{} {r}", l.id);
            }
        }
    }
}
