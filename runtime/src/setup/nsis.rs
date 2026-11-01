//! Installateurs NSIS 3 (Nullsoft Scriptable Install System).
//!
//! Un installateur NSIS contient, après son programme d'amorce, un en-tête
//! (`firstheader`) puis un bloc compressé (zlib ou LZMA, « solide » ou non)
//! qui renferme le script compilé : sections, instructions (`entries`),
//! chaînes, tables de langues, puis les données des fichiers. Winlux lit ce
//! script et l'interprète lui-même : extraction des fichiers, registre,
//! raccourcis, désinstalleur. Les appels aux greffons Windows (DLL) et les
//! programmes externes sont ignorés et notés dans le journal.

use super::lzma::{BcjReader, LzmaReader};
use super::{le_u32, Folders, Info, Installer, Kind, Options};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Position de l'en-tête NSIS (`firstheader`), alignée sur 512 octets.
pub fn find_header(data: &[u8]) -> Option<usize> {
    let mut off = 512;
    while off + 28 <= data.len() {
        if data[off + 4..off + 8] == [0xEF, 0xBE, 0xAD, 0xDE]
            && &data[off + 8..off + 20] == b"NullsoftInst"
        {
            return Some(off);
        }
        off += 512;
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Method {
    Stored,
    Deflate,
    Lzma,
    /// LZMA précédé d'un octet de filtre (1 = BCJ x86).
    LzmaFilter,
}

fn guess_method(d: &[u8]) -> Method {
    if d.len() >= 6 && d[0] == 0x5D && d[1] == 0 && d[2] == 0 && d[5] == 0 {
        Method::Lzma
    } else if d.len() >= 7 && d[0] <= 1 && d[1] == 0x5D && d[2] == 0 && d[3] == 0 {
        Method::LzmaFilter
    } else {
        Method::Deflate
    }
}

fn decoder<'a>(method: Method, raw: &'a [u8]) -> io::Result<Box<dyn Read + 'a>> {
    Ok(match method {
        Method::Stored => Box::new(raw),
        Method::Deflate => Box::new(crate::archive::inflate::Inflater::new(raw)),
        Method::Lzma => Box::new(LzmaReader::new(raw)?),
        Method::LzmaFilter => {
            let filter = raw[0] == 1;
            let l = LzmaReader::new(&raw[1..])?;
            if filter {
                Box::new(BcjReader::new(l))
            } else {
                Box::new(l)
            }
        }
    })
}

/// Accès aux données (fichiers) de l'installateur.
enum Data<'a> {
    /// Flux unique : en-tête puis fichiers, chacun précédé de sa taille.
    Solid {
        raw: &'a [u8],
        method: Method,
        reader: Option<Box<dyn Read + 'a>>,
        pos: u64,
        base: u64,
    },
    /// Blocs séparés : `[taille | bit 31 compressé][données]`.
    Blocks { raw: &'a [u8], method: Method },
}

impl<'a> Data<'a> {
    /// Contenu du bloc de données situé à `offset`.
    fn read(&mut self, offset: u32) -> io::Result<Vec<u8>> {
        let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
        match self {
            Data::Blocks { raw, method } => {
                let off = offset as usize;
                let len = le_u32(raw, off).ok_or_else(|| bad("bloc hors limites"))?;
                let size = (len & 0x7FFF_FFFF) as usize;
                let body = raw
                    .get(off + 4..off + 4 + size)
                    .ok_or_else(|| bad("bloc tronqué"))?;
                if len & 0x8000_0000 == 0 {
                    return Ok(body.to_vec());
                }
                let mut out = Vec::new();
                let mut r = decoder(*method, body)?;
                match r.read_to_end(&mut out) {
                    Ok(_) => Ok(out),
                    // LZMA sans marqueur de fin : la fin du bloc suffit.
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof && !out.is_empty() => {
                        Ok(out)
                    }
                    Err(e) => Err(e),
                }
            }
            Data::Solid {
                raw,
                method,
                reader,
                pos,
                base,
            } => {
                let target = *base + offset as u64;
                if reader.is_none() || *pos > target {
                    *reader = Some(decoder(*method, raw)?);
                    *pos = 0;
                }
                let r = reader.as_mut().ok_or_else(|| bad("flux"))?;
                let mut buf = vec![0u8; 65536];
                while *pos < target {
                    let n = ((target - *pos) as usize).min(buf.len());
                    r.read_exact(&mut buf[..n])?;
                    *pos += n as u64;
                }
                let mut len = [0u8; 4];
                r.read_exact(&mut len)?;
                let size = u32::from_le_bytes(len) as usize & 0x7FFF_FFFF;
                let mut out = vec![0u8; size];
                r.read_exact(&mut out)?;
                *pos += 4 + size as u64;
                Ok(out)
            }
        }
    }
}

const NB_PAGES: usize = 0;
const NB_SECTIONS: usize = 1;
const NB_ENTRIES: usize = 2;
const NB_STRINGS: usize = 3;
const NB_LANGTABLES: usize = 4;

/// Script compilé lu depuis l'en-tête.
struct Script {
    header: Vec<u8>,
    unicode: bool,
    blocks: [(usize, usize); 8],
    lang_table: usize,
    lang_id: u16,
    section_size: usize,
}

impl Script {
    fn int(&self, off: usize) -> i32 {
        le_u32(&self.header, off).unwrap_or(0) as i32
    }

    fn entry(&self, i: usize) -> Option<[i32; 7]> {
        let (off, num) = self.blocks[NB_ENTRIES];
        if i >= num {
            return None;
        }
        let base = off + i * 28;
        let mut e = [0i32; 7];
        for (k, v) in e.iter_mut().enumerate() {
            *v = self.int(base + 4 * k);
        }
        Some(e)
    }

    /// Chaîne de langue `index` dans la table choisie.
    fn lang_offset(&self, index: i32) -> i32 {
        let (tab_off, _) = self.blocks[NB_LANGTABLES];
        self.int(tab_off + self.lang_table + 10 + 4 * index as usize)
    }

    /// Caractère `i` (unités de 1 ou 2 octets) de la table des chaînes.
    fn ch(&self, i: usize) -> u16 {
        let base = self.blocks[NB_STRINGS].0;
        if self.unicode {
            let p = base + 2 * i;
            match self.header.get(p..p + 2) {
                Some(b) => u16::from_le_bytes([b[0], b[1]]),
                None => 0,
            }
        } else {
            self.header.get(base + i).copied().unwrap_or(0) as u16
        }
    }

    /// Chaîne brute (sans codes) à une position de la table.
    fn plain(&self, mut i: usize) -> String {
        let mut units = Vec::new();
        loop {
            let c = self.ch(i);
            if c == 0 || units.len() > 4096 {
                break;
            }
            units.push(c);
            i += 1;
        }
        if self.unicode {
            String::from_utf16_lossy(&units)
        } else {
            units.iter().map(|&u| cp1252(u as u8)).collect()
        }
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&b) {
        char::from_u32(HIGH[(b - 0x80) as usize] as u32).unwrap_or('?')
    } else {
        b as char
    }
}

// Variables prédéfinies.
const V_CMDLINE: usize = 20;
const V_INSTDIR: usize = 21;
const V_OUTDIR: usize = 22;
const V_EXEDIR: usize = 23;
const V_LANGUAGE: usize = 24;
const V_TEMP: usize = 25;
const V_PLUGINSDIR: usize = 26;
const V_EXEPATH: usize = 27;
const V_EXEFILE: usize = 28;
const V_HWNDPARENT: usize = 29;
const V_OUTDIR2: usize = 31;

// Drapeaux d'exécution (`exec_flags_t`).
const F_ALL_USER: usize = 1;
const F_ERROR: usize = 2;
const F_SILENT: usize = 8;

/// Fin anticipée de l'exécution.
enum Stop {
    Abort,
    Quit,
}

/// Interpréteur du script NSIS.
struct Vm<'a, 'b> {
    s: &'a Script,
    data: Data<'a>,
    stub: &'a [u8],
    inst: Option<&'b mut Installer>,
    vars: Vec<String>,
    flags: [i32; 16],
    last_flags: [i32; 16],
    stack: Vec<String>,
    sections: Vec<[i32; 6]>,
    steps: u64,
    depth: u32,
    /// Écritures de registre faites par le script (clé, nom) → valeur texte.
    reg: HashMap<(String, String), String>,
    uninstall_ids: Vec<String>,
    skipped: Vec<String>,
}

fn hex_int(s: &str) -> i64 {
    // `myatoi` : décimal, 0x hexadécimal, 0 octal, signe.
    let t = s.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        let d: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        i64::from_str_radix(&d, 16).unwrap_or(0)
    } else if t.starts_with('0') && t.len() > 1 {
        let d: String = t.chars().take_while(|c| ('0'..='7').contains(c)).collect();
        i64::from_str_radix(&d, 8).unwrap_or(0)
    } else {
        let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        d.parse().unwrap_or(0)
    };
    if neg {
        -v
    } else {
        v
    }
}

impl<'a, 'b> Vm<'a, 'b> {
    /// Développe la chaîne d'index `ptr` (codes de variables, dossiers…).
    fn string(&self, ptr: i32) -> String {
        self.string_depth(ptr, 0)
    }

    fn string_depth(&self, ptr: i32, depth: u32) -> String {
        if depth > 8 {
            return String::new();
        }
        let ptr = if ptr < 0 {
            self.s.lang_offset(-(ptr + 1))
        } else {
            ptr
        };
        if ptr < 0 {
            return String::new();
        }
        let mut i = ptr as usize;
        let mut out = String::new();
        let mut units: Vec<u16> = Vec::new();
        let flush = |units: &mut Vec<u16>, out: &mut String, uni: bool| {
            if uni {
                out.push_str(&String::from_utf16_lossy(units));
            } else {
                out.extend(units.iter().map(|&u| cp1252(u as u8)));
            }
            units.clear();
        };
        loop {
            let c = self.s.ch(i);
            i += 1;
            if c == 0 || out.len() + units.len() > 8192 {
                break;
            }
            if c > 4 {
                units.push(c);
                continue;
            }
            if c == 4 {
                units.push(self.s.ch(i));
                i += 1;
                continue;
            }
            // Paramètre sur 2 octets : une unité en Unicode, deux en ANSI.
            let (lo, hi) = if self.s.unicode {
                let p = self.s.ch(i);
                i += 1;
                ((p & 0xFF) as u8, (p >> 8) as u8)
            } else {
                let a = self.s.ch(i) as u8;
                let b = self.s.ch(i + 1) as u8;
                i += 2;
                (a, b)
            };
            let n = (((hi & 0x7F) as i32) << 7) | (lo & 0x7F) as i32;
            flush(&mut units, &mut out, self.s.unicode);
            match c {
                1 => out.push_str(&self.string_depth(-n - 1, depth + 1)),
                2 => out.push_str(&self.shell_folder(lo, hi)),
                3 => {
                    if n as usize == V_HWNDPARENT {
                        out.push('0');
                    } else {
                        out.push_str(self.vars.get(n as usize).map(String::as_str).unwrap_or(""));
                    }
                }
                _ => {}
            }
        }
        flush(&mut units, &mut out, self.s.unicode);
        out
    }

    /// Dossier spécial (CSIDL) d'un code NS_SHELL_CODE.
    fn shell_folder(&self, cur: u8, all: u8) -> String {
        if cur & 0x80 != 0 {
            // Valeur lue dans HKLM\…\CurrentVersion (ProgramFilesDir…).
            let name = self.s.plain((cur & 0x3F) as usize).to_ascii_lowercase();
            let x64 = cur & 0x40 != 0;
            let pf = Folders { x64 }.program_files();
            return if name.contains("common") {
                format!("{pf}\\Common Files")
            } else {
                pf.to_string()
            };
        }
        let id = if self.flags[F_ALL_USER] != 0 {
            all
        } else {
            cur
        };
        let roaming = "C:\\AppData\\Roaming";
        match id & 0x7F {
            0x00 | 0x10 | 0x19 => Folders::desktop(),
            0x02 | 0x17 => Folders::programs(),
            0x05 | 0x2E => "C:\\Documents".into(),
            0x06 | 0x1F => "C:\\Favorites".into(),
            0x07 | 0x18 => format!("{}\\Startup", Folders::programs()),
            0x08 => format!("{roaming}\\Microsoft\\Windows\\Recent"),
            0x09 => format!("{roaming}\\Microsoft\\Windows\\SendTo"),
            0x0B | 0x16 => Folders::start_menu().into(),
            0x0D => "C:\\Music".into(),
            0x0E => "C:\\Videos".into(),
            0x14 => "C:\\windows\\Fonts".into(),
            0x15 | 0x2D => format!("{roaming}\\Microsoft\\Windows\\Templates"),
            0x1A => {
                if id == all && self.flags[F_ALL_USER] != 0 || all == 0x1A && cur == 0x1A {
                    // $QUICKLAUNCH : même dossier pour les deux contextes.
                    format!("{roaming}\\Microsoft\\Internet Explorer\\Quick Launch")
                } else {
                    roaming.into()
                }
            }
            0x1C => "C:\\AppData\\Local".into(),
            0x20 => "C:\\AppData\\Local\\Microsoft\\Windows\\INetCache".into(),
            0x21 => "C:\\AppData\\Local\\Microsoft\\Windows\\INetCookies".into(),
            0x22 => "C:\\AppData\\Local\\Microsoft\\Windows\\History".into(),
            0x23 => "C:\\ProgramData".into(),
            0x24 => "C:\\windows".into(),
            0x25 => "C:\\windows\\system32".into(),
            0x26 => Folders { x64: false }.program_files().into(),
            0x27 => "C:\\Pictures".into(),
            0x28 => "C:".into(),
            0x2B => format!("{}\\Common Files", Folders { x64: false }.program_files()),
            0x2F | 0x30 => format!("{}\\Administrative Tools", Folders::programs()),
            0x38 | 0x39 => "C:\\windows\\Resources".into(),
            0x3B => "C:\\AppData\\Local\\Microsoft\\Windows\\Burn\\Burn".into(),
            _ => "C:".into(),
        }
    }

    fn int(&self, ptr: i32) -> i64 {
        hex_int(&self.string(ptr))
    }

    fn set_var(&mut self, idx: i32, v: String) {
        if idx >= 0 {
            let i = idx as usize;
            if i >= self.vars.len() {
                self.vars.resize(i + 1, String::new());
            }
            self.vars[i] = v;
        }
    }

    fn error(&mut self) {
        self.flags[F_ERROR] += 1;
    }

    /// Adresse de saut (encodée +1, négative : lue dans une variable).
    fn resolve(&self, v: i32) -> i64 {
        if v < 0 {
            hex_int(
                self.vars
                    .get((-(v + 1)) as usize)
                    .map(String::as_str)
                    .unwrap_or(""),
            )
        } else {
            v as i64
        }
    }

    fn run(&mut self, start: i64) -> Result<(), Stop> {
        if start < 0 {
            return Ok(());
        }
        self.depth += 1;
        if self.depth > 64 {
            self.depth -= 1;
            return Ok(());
        }
        let mut pos = start as usize;
        let result = loop {
            self.steps += 1;
            if self.steps > 2_000_000 {
                break Err(Stop::Quit);
            }
            let Some(e) = self.s.entry(pos) else {
                break Ok(());
            };
            if e[0] == 1 {
                break Ok(()); // EW_RET
            }
            match self.exec(&e) {
                Err(s) => break Err(s),
                Ok(jump) => {
                    let target = self.resolve(jump);
                    if target <= 0 {
                        pos += 1;
                    } else {
                        pos = (target - 1) as usize;
                    }
                }
            }
        };
        self.depth -= 1;
        result
    }

    /// Chemin Windows « validé » (comme `validate_filename`).
    fn path(&self, ptr: i32) -> String {
        let p = self.string(ptr);
        let mut out: String = p
            .chars()
            .enumerate()
            .filter(|&(i, c)| !(c < ' ' || "*?|<>/\"".contains(c) || (c == ':' && i != 1)))
            .map(|(_, c)| c)
            .collect();
        while out.ends_with('\\') || out.ends_with(' ') {
            out.pop();
        }
        out
    }

    fn relative(&self, p: String) -> String {
        let b = p.as_bytes();
        if (b.len() >= 2 && b[1] == b':') || p.starts_with("\\\\") {
            p
        } else {
            format!("{}\\{p}", self.vars[V_OUTDIR])
        }
    }

    fn in_plugins(&self, win: &str) -> bool {
        let pd = self.vars[V_PLUGINSDIR].to_ascii_lowercase();
        !pd.is_empty() && win.to_ascii_lowercase().starts_with(&pd)
    }

    fn hive(&self, root: i32) -> &'static str {
        if root == 0 || root == 0x4000_0000 || root == 0x2000_0000 || root == 0x6000_0000 {
            return if self.flags[F_ALL_USER] != 0 {
                "hklm"
            } else {
                "hkcu"
            };
        }
        match (root as u32) & 0x9FFF_FFFF {
            0x8000_0000 => "hkcr",
            0x8000_0001 => "hkcu",
            0x8000_0003 => "hku",
            0x8000_0005 => "hkcc",
            _ => "hklm",
        }
    }

    fn reg_read(&self, key: &str, name: &str) -> Option<(u32, Vec<u8>)> {
        if let Some(v) = self.reg.get(&(key.to_string(), name.to_lowercase())) {
            return Some((1, super::utf16z(v)));
        }
        let b = crate::dlls::advapi32::read_registry_value_internal(key, name)?;
        Some((0, b))
    }

    fn note_uninstall_key(&mut self, key: &str) {
        let marker = "software\\microsoft\\windows\\currentversion\\uninstall\\";
        if let Some(i) = key.find(marker) {
            let id = key[i + marker.len()..]
                .split('\\')
                .next()
                .unwrap_or("")
                .to_string();
            if !id.is_empty() && !self.uninstall_ids.contains(&id) {
                self.uninstall_ids.push(id);
            }
        }
    }

    fn skip(&mut self, what: String) {
        if !self.skipped.contains(&what) {
            self.skipped.push(what);
        }
    }

    /// Exécute une instruction ; renvoie l'adresse de saut (0 : suivante).
    fn exec(&mut self, e: &[i32; 7]) -> Result<i32, Stop> {
        let p = &e[1..];
        match e[0] {
            2 => return Ok(p[0]), // EW_NOP / Goto
            3 => return Err(Stop::Abort),
            4 => return Err(Stop::Quit),
            5 => {
                // EW_CALL
                let target = self.resolve(p[0]) - 1;
                self.run(target)?;
            }
            6..=10 => {} // texte, pause, fenêtre, détails, attributs
            11 => {
                // EW_CREATEDIR (CreateDirectory / SetOutPath)
                let dir = self.path(p[0]);
                if let Some(inst) = self.inst.as_deref_mut() {
                    let plugins = {
                        let pd = self.vars[V_PLUGINSDIR].to_ascii_lowercase();
                        !pd.is_empty() && dir.to_ascii_lowercase().starts_with(&pd)
                    };
                    let res = if plugins {
                        std::fs::create_dir_all(crate::paths::to_linux(&dir))
                            .map(|_| PathBuf::new())
                    } else {
                        inst.mkdirs_win(&dir)
                    };
                    if res.is_err() {
                        self.error();
                    }
                }
                if p[1] != 0 {
                    self.vars[V_OUTDIR] = dir;
                }
            }
            12 => {
                // EW_IFFILEEXISTS
                let f = self.string(p[0]);
                return Ok(if file_exists(&f) { p[1] } else { p[2] });
            }
            13 => {
                // EW_SETFLAG
                let i = (p[0] as usize).min(15);
                if p[2] <= 0 {
                    self.last_flags[i] = self.flags[i];
                    self.flags[i] = self.int(p[1]) as i32;
                } else {
                    self.flags[i] = self.last_flags[i];
                }
            }
            14 => {
                // EW_IFFLAG
                let i = (p[2] as usize).min(15);
                let f = if self.flags[i] != 0 { p[0] } else { p[1] };
                self.flags[i] &= p[3];
                return Ok(f);
            }
            15 => {
                let v = self.flags[(p[1] as usize).min(15)].to_string();
                self.set_var(p[0], v);
            }
            16 => {
                // EW_RENAME
                let from = self.path(p[0]);
                let to = self.path(p[1]);
                if self.inst.is_some() {
                    let a = crate::paths::to_linux(&from);
                    let b = crate::paths::to_linux(&to);
                    if std::fs::rename(&a, &b).is_ok() {
                        if let Some(inst) = self.inst.as_deref_mut() {
                            inst.note_file(&b);
                        }
                    } else {
                        self.error();
                    }
                }
            }
            17 => {
                let v = self.string(p[1]);
                self.set_var(p[0], v);
            }
            18 => {
                self.set_var(p[0], String::new());
                self.error();
            }
            19 => {
                let dir = {
                    let d = self.string(p[1]);
                    if d.is_empty() {
                        self.vars[V_TEMP].clone()
                    } else {
                        d
                    }
                };
                let name = format!("{dir}\\ns{:04X}.tmp", self.steps as u16);
                self.set_var(p[0], name);
            }
            20 => {
                // EW_EXTRACTFILE
                let name = self.relative(self.path(p[1]));
                let overwrite = p[0] & 7;
                if self.inst.is_some() {
                    let exists = crate::paths::to_linux(&name).exists();
                    if !(overwrite == 1 && exists) {
                        match self.data.read(p[2] as u32) {
                            Ok(bytes) => {
                                let plugins = self.in_plugins(&name);
                                let res = if plugins {
                                    let path = crate::paths::to_linux(&name);
                                    path.parent()
                                        .map(std::fs::create_dir_all)
                                        .transpose()
                                        .and_then(|_| std::fs::write(&path, &bytes))
                                } else if let Some(inst) = self.inst.as_deref_mut() {
                                    inst.write_file(&name, &bytes).map(|_| ())
                                } else {
                                    Ok(())
                                };
                                if let Err(err) = res {
                                    if let Some(inst) = self.inst.as_deref_mut() {
                                        inst.log.log(&format!("{name} : {err}"));
                                    }
                                    if overwrite == 2 {
                                        self.error();
                                    }
                                }
                            }
                            Err(err) => {
                                if let Some(inst) = self.inst.as_deref_mut() {
                                    inst.log
                                        .log(&format!("{name} : données illisibles ({err})"));
                                }
                                return Err(Stop::Abort);
                            }
                        }
                    }
                }
            }
            21 => {
                // EW_DELETEFILE
                let f = self.string(p[0]);
                if self.inst.is_some() && self.safe_to_delete(&f) {
                    super::delete_pattern(&f, 0);
                }
            }
            22 => {
                // EW_MESSAGEBOX : réponse par défaut (/SD) ou positive.
                let style = p[0];
                let sd = (style >> 21) & 0xF;
                let v = if sd != 0 {
                    sd
                } else {
                    match style & 0xF {
                        2 => 5,     // Abandonner/Réessayer/Ignorer → Ignorer
                        3 | 4 => 6, // Oui
                        5 => 2,     // Réessayer/Annuler → Annuler
                        6 => 11,    // Continuer
                        _ => 1,     // OK
                    }
                };
                let text = self.string(p[1]);
                if let Some(inst) = self.inst.as_deref_mut() {
                    inst.log.log(&format!(
                        "message : {}",
                        text.replace("\r\n", " ").replace('\n', " ")
                    ));
                }
                if v == p[2] {
                    return Ok(p[3]);
                }
                if v == p[4] {
                    return Ok(p[5]);
                }
            }
            23 => {
                // EW_RMDIR (retrait sur le dossier d'installation seulement)
                let d = self.path(p[0]);
                if self.inst.is_some() && self.safe_to_delete(&d) {
                    super::delete_pattern(&d, if p[1] & 2 != 0 { 1 } else { 2 });
                    if p[1] & 2 != 0 {
                        let _ = std::fs::remove_dir_all(crate::paths::to_linux(&d));
                    }
                }
            }
            24 => {
                let v = self.string(p[1]).chars().count().to_string();
                self.set_var(p[0], v);
            }
            25 => {
                // EW_ASSIGNVAR (StrCpy avec longueur et départ)
                let src: Vec<char> = self.string(p[1]).chars().collect();
                let lens = self.string(p[2]);
                let mut newlen = if lens.is_empty() {
                    src.len() as i64
                } else {
                    hex_int(&lens)
                };
                let mut start = self.int(p[3]);
                let srclen = src.len() as i64;
                let mut out = String::new();
                if newlen != 0 {
                    if start < 0 {
                        start += srclen;
                    }
                    if start >= 0 {
                        let start = start.min(srclen) as usize;
                        let rest: Vec<char> = src[start..].to_vec();
                        if newlen < 0 {
                            newlen += rest.len() as i64;
                        }
                        let n = newlen.clamp(0, rest.len() as i64) as usize;
                        out = rest[..n].iter().collect();
                    }
                }
                self.set_var(p[0], out);
            }
            26 => {
                // EW_STRCMP
                let a = self.string(p[0]);
                let b = self.string(p[1]);
                let eq = if p[4] != 0 {
                    a == b
                } else {
                    a.to_lowercase() == b.to_lowercase()
                };
                return Ok(if eq { p[2] } else { p[3] });
            }
            27 => {
                // EW_READENVSTR
                let s = self.string(p[1]);
                let v = if p[2] != 0 {
                    std::env::var(&s).ok()
                } else {
                    Some(expand_env(&s))
                };
                match v {
                    Some(v) => self.set_var(p[0], v),
                    None => {
                        self.set_var(p[0], String::new());
                        self.error();
                    }
                }
            }
            28 => {
                // EW_INTCMP
                let a = self.int(p[0]);
                let b = self.int(p[1]);
                let unsigned = p[5] & 1 != 0;
                let (a, b) = if p[5] as i16 as i32 & 0x8000 != 0 {
                    (a, b)
                } else if unsigned {
                    (a as u32 as i64, b as u32 as i64)
                } else {
                    (a as i32 as i64, b as i32 as i64)
                };
                return Ok(if a < b {
                    p[3]
                } else if a > b {
                    p[4]
                } else {
                    p[2]
                });
            }
            29 => {
                // EW_INTOP
                let v = self.int(p[1]) as i32;
                let v2 = self.int(p[2]) as i32;
                let r = match p[3] {
                    0 => v.wrapping_add(v2),
                    1 => v.wrapping_sub(v2),
                    2 => v.wrapping_mul(v2),
                    3 => {
                        if v2 == 0 {
                            self.error();
                            0
                        } else {
                            v.wrapping_div(v2)
                        }
                    }
                    4 => v | v2,
                    5 => v & v2,
                    6 => v ^ v2,
                    7 => (v == 0) as i32,
                    8 => (v != 0 || v2 != 0) as i32,
                    9 => (v != 0 && v2 != 0) as i32,
                    10 => {
                        if v2 == 0 {
                            self.error();
                            0
                        } else {
                            v.wrapping_rem(v2)
                        }
                    }
                    11 => v.wrapping_shl(v2 as u32),
                    12 => v.wrapping_shr(v2 as u32),
                    13 => ((v as u32).wrapping_shr(v2 as u32)) as i32,
                    _ => v,
                };
                self.set_var(p[0], r.to_string());
            }
            30 => {
                let fmt = self.string(p[1]);
                let v = self.int(p[2]);
                self.set_var(p[0], format_int(&fmt, v));
            }
            31 => {
                // EW_PUSHPOP
                if p[2] != 0 {
                    let n = p[2] as usize;
                    if n < self.stack.len() {
                        let top = self.stack.len() - 1;
                        self.stack.swap(top, top - n);
                    } else {
                        self.error();
                    }
                } else if p[1] != 0 {
                    match self.stack.pop() {
                        Some(v) => self.set_var(p[0], v),
                        None => self.error(),
                    }
                } else {
                    let v = self.string(p[0]);
                    self.stack.push(v);
                }
            }
            32 => self.set_var(p[0], "0".into()), // FindWindow
            33 => {
                if p[0] >= 0 {
                    self.set_var(p[0], "0".into());
                }
            }
            34 => return Ok(p[2]), // IsWindow : jamais
            35 => self.set_var(p[0], "0".into()),
            36 | 37 | 39 => {}
            38 => self.set_var(p[0], "0".into()),
            40 | 41 => {
                // ShellExec / Exec : programmes externes non lancés.
                let what = if e[0] == 40 {
                    format!("{} {}", self.string(p[2]), self.string(p[3]))
                } else {
                    self.string(p[0])
                };
                self.skip(format!("programme ignoré : {}", what.trim()));
                if e[0] == 41 && p[2] >= 0 && p[1] != 0 {
                    self.set_var(p[2], "0".into());
                }
            }
            42 | 43 => {
                self.set_var(p[1], "0".into());
                self.set_var(p[2], "0".into());
                self.error();
            }
            44 => {
                // EW_REGISTERDLL : greffon NSIS ou inscription COM.
                let dll = self.path(p[0]);
                let func = self.string(p[1]);
                let name = dll.rsplit('\\').next().unwrap_or("").to_string();
                self.skip(format!("greffon ignoré : {name}::{func}"));
                if !self.in_plugins(&dll) {
                    self.error();
                }
            }
            45 => {
                // EW_CREATESHORTCUT
                let link = self.path(p[0]);
                let target = self.path(p[1]);
                let args = self.string(p[2]);
                let desc = self.string(p[5]);
                let workdir = if p[4] & 0x8000 != 0 {
                    String::new()
                } else {
                    self.vars[V_OUTDIR].clone()
                };
                if let Some(inst) = self.inst.as_deref_mut() {
                    if inst
                        .shortcut(&link, &target, &args, &workdir, &desc)
                        .is_err()
                    {
                        self.flags[F_ERROR] += 1;
                    }
                }
            }
            46 => {
                // EW_COPYFILES
                let from = self.string(p[0]);
                let to = self.string(p[1]);
                if let Some(inst) = self.inst.as_deref_mut() {
                    let src = crate::paths::to_linux(&from);
                    let dest = crate::paths::to_linux(&to);
                    let target = if dest.is_dir() {
                        dest.join(src.file_name().unwrap_or_default())
                    } else {
                        dest
                    };
                    if src.is_file() && std::fs::copy(&src, &target).is_ok() {
                        inst.note_file(&target);
                    } else {
                        self.flags[F_ERROR] += 1;
                    }
                }
            }
            47 => {}
            48 => {
                // EW_WRITEINI
                if p[0] != 0 && p[1] != 0 && p[4] != 0 {
                    let sec = self.string(p[0]);
                    let key = self.string(p[1]);
                    let val = self.string(p[2]);
                    let file = self.path(p[3]);
                    if let Some(inst) = self.inst.as_deref_mut() {
                        if super::inno::write_ini(inst, &file, &sec, &key, &val).is_err() {
                            self.flags[F_ERROR] += 1;
                        }
                    }
                }
            }
            49 => {
                // EW_READINISTR
                let sec = self.string(p[1]);
                let key = self.string(p[2]);
                let file = self.path(p[3]);
                match read_ini(&file, &sec, &key) {
                    Some(v) => self.set_var(p[0], v),
                    None => {
                        self.set_var(p[0], String::new());
                        self.error();
                    }
                }
            }
            50 => {
                // EW_DELREG
                let root = self.hive(p[0]);
                let key = super::reg_key(root, &self.string(p[1]));
                let name = self.string(p[2]);
                if let Some(inst) = self.inst.as_deref_mut() {
                    if p[3] == 0 {
                        inst.reg_delete_value(&key, &name);
                    } else if !key.ends_with("\\software") && key.matches('\\').count() >= 2 {
                        inst.reg_delete_tree(&key);
                    }
                    inst.flush_registry();
                }
            }
            51 => {
                // EW_WRITEREG
                let root = self.hive(p[0]);
                let key = super::reg_key(root, &self.string(p[1]));
                let name = self.string(p[2]);
                let (kind, bytes, text) = match p[4] {
                    4 => {
                        let v = self.int(p[3]) as u32;
                        (4, v.to_le_bytes().to_vec(), v.to_string())
                    }
                    3 => {
                        let b = if self.inst.is_some() {
                            self.data.read(p[3] as u32).unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        (p[5] as u32, b, String::new())
                    }
                    _ => {
                        let s = self.string(p[3]);
                        let kind = if p[5] == 2 { 2 } else { 1 };
                        (kind, super::utf16z(&s), s)
                    }
                };
                self.note_uninstall_key(&key);
                self.reg.insert((key.clone(), name.to_lowercase()), text);
                if let Some(inst) = self.inst.as_deref_mut() {
                    inst.reg_set(&key, &name, kind, bytes, true);
                    inst.flush_registry();
                }
            }
            52 => {
                // EW_READREGSTR
                let root = self.hive(p[0]);
                let key = super::reg_key(root, &self.string(p[1]));
                let name = self.string(p[2]);
                let want_int = p[3] != 0;
                match self.reg_read(&key, &name) {
                    Some((_, b)) => {
                        let v = if want_int && b.len() == 4 {
                            u32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string()
                        } else {
                            super::utf16_text(&b, true)
                        };
                        self.set_var(p[0], v);
                    }
                    None => {
                        self.set_var(p[0], String::new());
                        self.error();
                    }
                }
            }
            53 => {
                self.set_var(p[0], String::new());
                self.error();
            }
            54 | 58 | 59 => {}
            55 => {
                self.set_var(p[3], String::new());
                self.error();
            }
            56 | 57 | 68 | 69 => self.error(),
            60 => {
                self.set_var(p[0], String::new());
                self.error();
            }
            61 => {
                self.set_var(p[1], String::new());
                self.set_var(p[2], String::new());
                self.error();
            }
            62 => {
                // EW_WRITEUNINSTALLER
                let mut name = self.path(p[0]);
                let b = name.as_bytes();
                if !(b.len() >= 2 && b[1] == b':') {
                    name = format!("{}\\{name}", self.vars[V_INSTDIR]);
                }
                if let Some(inst) = self.inst.as_deref_mut() {
                    match inst.write_file(&name, self.stub) {
                        Ok(path) => {
                            inst.manifest.files.retain(|f| f != &path);
                            inst.manifest.uninstaller = Some(path);
                        }
                        Err(_) => self.flags[F_ERROR] += 1,
                    }
                }
            }
            63 => {
                // EW_SECTIONSET
                let x = self.int(p[0]);
                if x < 0 || x as usize >= self.sections.len() {
                    self.error();
                } else if p[2] >= 0 {
                    let idx = (p[2] as usize).min(5);
                    let v = if idx == 0 {
                        self.string(self.sections[x as usize][0])
                    } else {
                        self.sections[x as usize][idx].to_string()
                    };
                    self.set_var(p[1], v);
                } else {
                    let idx = (-p[2] - 1) as usize;
                    if idx > 0 && idx < 6 {
                        let v = self.int(p[1]) as i32;
                        self.sections[x as usize][idx] = v;
                    }
                }
            }
            64 => {
                if p[3] != 0 && p[2] == 0 {
                    self.set_var(p[1], "0".into());
                }
            }
            65 => self.error(), // GetOSInfo
            _ => {}
        }
        Ok(0)
    }

    /// N'efface que dans le dossier d'installation, les menus ou le Bureau.
    fn safe_to_delete(&self, win: &str) -> bool {
        let w = win.to_ascii_lowercase();
        let inst = self.vars[V_INSTDIR].to_ascii_lowercase();
        let roots = [
            inst.as_str(),
            &Folders::programs().to_ascii_lowercase(),
            &Folders::desktop().to_ascii_lowercase(),
            &self.vars[V_PLUGINSDIR].to_ascii_lowercase(),
        ]
        .map(|s| s.to_string());
        roots
            .iter()
            .any(|r| r.len() > 3 && w.starts_with(r.as_str()) && w.len() > r.len())
    }
}

fn file_exists(win: &str) -> bool {
    let path = crate::paths::to_linux(win);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.contains('*') || name.contains('?') {
        return path
            .parent()
            .and_then(|d| std::fs::read_dir(d).ok())
            .is_some_and(|rd| {
                rd.flatten()
                    .any(|e| super::wildcard(&name, &e.file_name().to_string_lossy()))
            });
    }
    path.exists()
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) => {
                let name = &after[..j];
                let v = match name.to_ascii_uppercase().as_str() {
                    "PROGRAMFILES" | "PROGRAMW6432" => "C:\\Program Files".to_string(),
                    "PROGRAMFILES(X86)" => "C:\\Program Files (x86)".to_string(),
                    "SYSTEMROOT" | "WINDIR" => "C:\\windows".to_string(),
                    "APPDATA" => "C:\\AppData\\Roaming".to_string(),
                    "LOCALAPPDATA" => "C:\\AppData\\Local".to_string(),
                    "TEMP" | "TMP" => Folders::temp().to_string(),
                    "USERPROFILE" | "HOMEDRIVE" => "C:".to_string(),
                    _ => std::env::var(name).unwrap_or_else(|_| format!("%{name}%")),
                };
                out.push_str(&v);
                rest = &after[j + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn format_int(fmt: &str, v: i64) -> String {
    // Formats usuels d'IntFmt : %d %u %x %X %c, largeur et zéros.
    let Some(i) = fmt.find('%') else {
        return fmt.to_string();
    };
    let spec: String = fmt[i + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || "-0lI".contains(*c))
        .collect();
    let conv = fmt[i + 1 + spec.len()..].chars().next().unwrap_or('d');
    let zero = spec.starts_with('0');
    let width: usize = spec
        .trim_start_matches(['0', '-'])
        .trim_end_matches(['l', 'I'])
        .parse()
        .unwrap_or(0);
    let body = match conv {
        'x' => format!("{:x}", v as u32),
        'X' => format!("{:X}", v as u32),
        'u' => format!("{}", v as u32),
        'c' => char::from_u32(v as u32)
            .map(String::from)
            .unwrap_or_default(),
        _ => format!("{}", v as i32),
    };
    let padded = if body.len() < width {
        let pad = if zero { '0' } else { ' ' };
        std::iter::repeat_n(pad, width - body.len()).collect::<String>() + &body
    } else {
        body
    };
    let end = i + 1 + spec.len() + conv.len_utf8();
    format!("{}{}{}", &fmt[..i], padded, fmt.get(end..).unwrap_or(""))
}

fn read_ini(file: &str, sec: &str, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(crate::paths::to_linux(file)).ok()?;
    let mut in_sec = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_sec = t.trim_matches(['[', ']']).eq_ignore_ascii_case(sec);
        } else if in_sec {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Lit l'en-tête et prépare l'accès aux données.
fn load(data: &[u8]) -> Result<(Script, Data<'_>, usize), String> {
    let fh = find_header(data).ok_or("en-tête NSIS introuvable")?;
    let flags = le_u32(data, fh).unwrap_or(0);
    if flags & 1 != 0 {
        return Err("ce fichier est un désinstalleur NSIS".into());
    }
    let header_len = le_u32(data, fh + 20).ok_or("en-tête tronqué")? as usize;
    let total = le_u32(data, fh + 24).ok_or("en-tête tronqué")? as usize;
    let start = fh + 28;
    let end = (fh + total).min(data.len());
    let body = data.get(start..end).ok_or("installateur tronqué")?;
    let first = le_u32(body, 0).ok_or("installateur tronqué")?;
    let solid_lzma = matches!(guess_method(body), Method::Lzma | Method::LzmaFilter);
    let (header, mut data_src) =
        if !solid_lzma && first & 0x8000_0000 != 0 && ((first & 0x7FFF_FFFF) as usize) < body.len()
        {
            // Blocs séparés, en-tête compressé.
            let size = (first & 0x7FFF_FFFF) as usize;
            let method = guess_method(&body[4..]);
            let mut r = decoder(method, &body[4..4 + size]).map_err(|e| e.to_string())?;
            let mut h = vec![0u8; header_len];
            r.read_exact(&mut h).map_err(|e| format!("en-tête : {e}"))?;
            (
                h,
                Data::Blocks {
                    raw: &body[4 + size..],
                    method,
                },
            )
        } else if first as usize == header_len {
            // En-tête stocké sans compression.
            let h = body
                .get(4..4 + header_len)
                .ok_or("en-tête tronqué")?
                .to_vec();
            (
                h,
                Data::Blocks {
                    raw: &body[4 + header_len..],
                    method: Method::Stored,
                },
            )
        } else {
            // Compression « solide ».
            let method = guess_method(body);
            let mut r = decoder(method, body).map_err(|e| e.to_string())?;
            let mut len = [0u8; 4];
            r.read_exact(&mut len)
                .map_err(|e| format!("en-tête : {e}"))?;
            let hlen = u32::from_le_bytes(len) as usize;
            if hlen != header_len {
                return Err("compression NSIS non reconnue".into());
            }
            let mut h = vec![0u8; header_len];
            r.read_exact(&mut h).map_err(|e| format!("en-tête : {e}"))?;
            (
                h,
                Data::Solid {
                    raw: body,
                    method,
                    reader: Some(r),
                    pos: 4 + header_len as u64,
                    base: 4 + header_len as u64,
                },
            )
        };
    if let Data::Solid { .. } = &data_src {
    } else if let Data::Blocks { method, .. } = &mut data_src {
        if *method == Method::Stored {
            // Données compressées malgré un en-tête stocké : détecté au vol.
            *method = Method::Deflate;
        }
    }
    let mut blocks = [(0usize, 0usize); 8];
    for (i, b) in blocks.iter_mut().enumerate() {
        let off = le_u32(&header, 4 + 8 * i).unwrap_or(0) as usize;
        let num = le_u32(&header, 8 + 8 * i).unwrap_or(0) as usize;
        *b = (off, num);
    }
    if blocks[NB_PAGES].0 < 68
        || blocks[NB_ENTRIES].0 >= header.len()
        || blocks[NB_STRINGS].0 >= header.len()
    {
        return Err("format NSIS non pris en charge (NSIS 3 32 bits attendu)".into());
    }
    // Chaînes Unicode : la table commence par un caractère nul sur 2 octets
    // et contient beaucoup d'octets nuls aux positions impaires.
    let strs = &header[blocks[NB_STRINGS].0..blocks[NB_LANGTABLES].0.max(blocks[NB_STRINGS].0)];
    let sample = &strs[..strs.len().min(4096)];
    let zeros_odd = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let unicode = sample.len() >= 4 && zeros_odd * 2 > sample.len() / 2 * 3 / 2;
    // Taille d'une section : (entrées - sections) / nombre.
    let (soff, snum) = blocks[NB_SECTIONS];
    let section_size = (blocks[NB_ENTRIES].0.saturating_sub(soff))
        .checked_div(snum)
        .unwrap_or(24);
    // Table de langue : celle de l'utilisateur, sinon l'anglais, sinon la première.
    let langtable_size = le_u32(&header, 100).unwrap_or(0) as usize;
    let (loff, lnum) = blocks[NB_LANGTABLES];
    let want = super::user_language();
    let ids: Vec<u16> = (0..lnum)
        .map(|i| {
            let p = loff + i * langtable_size;
            header
                .get(p..p + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .unwrap_or(0)
        })
        .collect();
    let pick = ids
        .iter()
        .position(|&id| id == want)
        .or_else(|| ids.iter().position(|&id| id & 0x3FF == want & 0x3FF))
        .or_else(|| ids.iter().position(|&id| id == 0x0409))
        .unwrap_or(0);
    let script = Script {
        header,
        unicode,
        blocks,
        lang_table: pick * langtable_size,
        lang_id: ids.get(pick).copied().unwrap_or(0x0409),
        section_size,
    };
    Ok((script, data_src, fh))
}

/// Exécute .onInit puis les sections cochées. `inst` absent : simulation
/// (pour lire le nom et le dossier sans rien écrire).
fn execute(
    data: &[u8],
    path: &Path,
    opts: &Options,
    inst: Option<&mut Installer>,
) -> Result<(String, String, String, String, Vec<String>, Vec<String>), String> {
    let (script, data_src, fh) = load(data)?;
    let h = &script;
    let mut vars = vec![String::new(); 64];
    let exe = crate::paths::to_windows(path);
    let (exedir, exefile) = exe
        .rsplit_once('\\')
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or_default();
    let temp = Folders::temp().to_string();
    let plugins_dir = if inst.is_some() {
        let d = std::env::temp_dir().join(format!("winlux-nsis-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        Some(d)
    } else {
        None
    };
    vars[V_CMDLINE] = format!("\"{exe}\"");
    vars[V_EXEDIR] = exedir;
    vars[V_EXEPATH] = exe.clone();
    vars[V_EXEFILE] = exefile;
    vars[V_TEMP] = temp.clone();
    vars[V_LANGUAGE] = script.lang_id.to_string();
    vars[V_PLUGINSDIR] = plugins_dir
        .as_ref()
        .map(|d| crate::paths::to_windows(d))
        .unwrap_or_else(|| format!("{temp}\\nsWinlux.tmp"));
    let mut sections = Vec::new();
    let (soff, snum) = script.blocks[NB_SECTIONS];
    for i in 0..snum {
        let b = soff + i * script.section_size;
        let mut s = [0i32; 6];
        for (k, v) in s.iter_mut().enumerate() {
            *v = script.int(b + 4 * k);
        }
        sections.push(s);
    }
    let stub = &data[..fh];
    let mut vm = Vm {
        s: &script,
        data: data_src,
        stub,
        inst,
        vars,
        flags: [0; 16],
        last_flags: [0; 16],
        stack: Vec::new(),
        sections,
        steps: 0,
        depth: 0,
        reg: HashMap::new(),
        uninstall_ids: Vec::new(),
        skipped: Vec::new(),
    };
    vm.flags[F_SILENT] = 1;
    // Dossier par défaut (InstallDir, ou InstallDirRegKey s'il existe).
    let hsize = h.blocks[NB_PAGES].0;
    let dir_ptr = h.int(hsize - 20);
    let mut instdir = vm.path(dir_ptr);
    let reg_key_ptr = h.int(72);
    if reg_key_ptr > 0 {
        let root = vm.hive(h.int(68));
        let key = super::reg_key(root, &vm.string(reg_key_ptr));
        let name = vm.string(h.int(76));
        if let Some((_, b)) = vm.reg_read(&key, &name) {
            let v = super::utf16_text(&b, true);
            if !v.is_empty() {
                instdir = v.trim_matches('"').trim_end_matches('\\').to_string();
            }
        }
    }
    vm.vars[V_INSTDIR] = instdir;
    // .onInit
    let on_init = h.int(108);
    if on_init >= 0 {
        match vm.run(on_init as i64) {
            Err(Stop::Quit) => {
                return Err("l'installateur s'est arrêté pendant son initialisation".into())
            }
            Err(Stop::Abort) | Ok(()) => {}
        }
    }
    if let Some(d) = &opts.dir {
        vm.vars[V_INSTDIR] = d.clone();
    }
    vm.vars[V_OUTDIR] = vm.vars[V_INSTDIR].clone();
    vm.vars[V_OUTDIR2] = vm.vars[V_INSTDIR].clone();
    let instdir = vm.vars[V_INSTDIR].clone();
    {
        // Sections cochées, dans l'ordre (sans écriture en simulation).
        let list = vm.sections.clone();
        for (i, s) in list.iter().enumerate() {
            let flags = vm.sections[i][2];
            let code = s[3];
            if flags & 1 == 0 || flags & 6 != 0 || code < 0 {
                continue;
            }
            vm.flags[F_ERROR] = 0;
            match vm.run(code as i64) {
                Ok(()) => {}
                Err(Stop::Abort) => {
                    return Err("l'installateur a interrompu l'installation".into());
                }
                Err(Stop::Quit) => break,
            }
        }
    }
    // Nom, version, éditeur : inscription « Uninstall » du script.
    let id = vm.uninstall_ids.first().cloned().unwrap_or_default();
    let ukey = format!(
        "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{}",
        id.to_lowercase()
    );
    let get = |vm: &Vm, n: &str| -> String {
        for root in ["hklm", "hkcu"] {
            let k = ukey.replacen("hklm", root, 1);
            if let Some(v) = vm.reg.get(&(k, n.to_lowercase())) {
                return v.clone();
            }
        }
        String::new()
    };
    let mut name = get(&vm, "DisplayName");
    if name.is_empty() {
        name = vm.string(-3); // LANG_NAME : « Name » du script
    }
    if name.is_empty() {
        name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    let version = get(&vm, "DisplayVersion");
    let publisher = get(&vm, "Publisher");
    let skipped = std::mem::take(&mut vm.skipped);
    let ids = vm.uninstall_ids.clone();
    if let Some(d) = plugins_dir {
        let _ = std::fs::remove_dir_all(d);
    }
    Ok((name, version, publisher, instdir, ids, skipped))
}

pub fn info(data: &[u8], path: &Path) -> Result<Info, String> {
    let (name, version, publisher, dir, _, _) = execute(data, path, &Options::default(), None)?;
    Ok(Info {
        kind: Kind::Nsis,
        name,
        version,
        publisher,
        default_dir: dir,
    })
}

pub fn install(
    data: &[u8],
    path: &Path,
    opts: &Options,
    inst: &mut Installer,
) -> Result<(), String> {
    let (name, version, publisher, dir, ids, skipped) = execute(data, path, opts, Some(inst))?;
    for s in &skipped {
        inst.log.log(s);
    }
    inst.log.log(&format!("{name} {version} (NSIS) vers {dir}"));
    inst.manifest.name = name;
    inst.manifest.version = version;
    inst.manifest.publisher = publisher;
    inst.manifest.location = dir;
    if let Some(id) = ids.first() {
        inst.manifest.id = id.clone();
        let uk_m = format!(
            "software\\microsoft\\windows\\currentversion\\uninstall\\{}",
            id.to_lowercase()
        );
        // Clé réellement utilisée (HKLM ou HKCU).
        let key = inst
            .manifest
            .keys
            .iter()
            .find(|k| k.ends_with(&uk_m))
            .cloned()
            .unwrap_or_else(|| format!("hklm\\{uk_m}"));
        inst.manifest.uninstall_key = key;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atoi_like_nsis() {
        assert_eq!(hex_int("0x1F"), 31);
        assert_eq!(hex_int("-12abc"), -12);
        assert_eq!(hex_int("010"), 8);
        assert_eq!(hex_int(""), 0);
    }

    #[test]
    fn intfmt() {
        assert_eq!(format_int("%08X", 255), "000000FF");
        assert_eq!(format_int("v%d.", 7), "v7.");
    }
}
