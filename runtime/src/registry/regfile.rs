//! Fichiers .reg : lecture (REGEDIT4 et « Windows Registry Editor Version
//! 5.00 ») et écriture (version 5.00, UTF-16LE avec BOM, comme regedit).

use super::{
    canonical, display_path, from_ansi, is_string, join, multi_of, read, string_of, utf16z, write,
    Data, Value, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD, REG_SZ,
};
use std::path::Path;

/// Une opération décrite par un fichier .reg.
#[derive(Debug, PartialEq)]
pub enum Op {
    Key(String),
    DeleteKey(String),
    Set(String, String, u32, Vec<u8>),
    DeleteValue(String, String),
}

/// Texte d'un fichier .reg quel que soit son encodage.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return String::from_utf16_lossy(&super::utf16_units(rest));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => crate::dlls::decode_ansi(bytes),
    }
}

/// Lit une chaîne entre guillemets (`\\` et `\"` échappés) au début de `s`.
fn quoted(s: &str) -> Option<(String, &str)> {
    let mut chars = s.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &s[i + 2..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| u8::from_str_radix(p, 16).map_err(|_| format!("octet invalide « {p} »")))
        .collect()
}

/// Analyse le texte d'un fichier .reg.
pub fn parse(text: &str) -> Result<Vec<Op>, String> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    let header = lines
        .by_ref()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let unicode = match header {
        "Windows Registry Editor Version 5.00" => true,
        "REGEDIT4" => false,
        _ => return Err("ce fichier n'est pas un fichier de registre (.reg)".into()),
    };
    // Recolle les lignes terminées par « \ » (données hexadécimales longues).
    let mut logical = Vec::new();
    let mut pending = String::new();
    for line in lines {
        let line = if pending.is_empty() {
            line
        } else {
            line.trim_start()
        };
        if let Some(head) = line.strip_suffix('\\') {
            if head.trim_end().ends_with(',') || head.trim_end().ends_with(':') {
                pending.push_str(head);
                continue;
            }
        }
        pending.push_str(line);
        logical.push(std::mem::take(&mut pending));
    }
    if !pending.is_empty() {
        logical.push(pending);
    }

    let mut ops = Vec::new();
    let mut current: Option<String> = None;
    for (n, line) in logical.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let err = |m: &str| format!("ligne {} : {m}", n + 2);
        if let Some(inner) = line.strip_prefix('[') {
            let inner = inner
                .strip_suffix(']')
                .ok_or_else(|| err("« ] » manquant"))?;
            let (delete, name) = match inner.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, inner),
            };
            let path = canonical(name).ok_or_else(|| err("racine du registre inconnue"))?;
            if delete {
                ops.push(Op::DeleteKey(path));
                current = None;
            } else {
                ops.push(Op::Key(path.clone()));
                current = Some(path);
            }
            continue;
        }
        let Some(key) = current.clone() else {
            // Valeurs sous une clé supprimée ou avant toute clé : ignorées
            // comme le fait regedit.
            continue;
        };
        let (name, rest) = if let Some(rest) = line.strip_prefix('@') {
            (String::new(), rest)
        } else {
            quoted(line).ok_or_else(|| err("nom de valeur mal formé"))?
        };
        let data = rest
            .trim_start()
            .strip_prefix('=')
            .ok_or_else(|| err("« = » manquant"))?
            .trim();
        if data == "-" {
            ops.push(Op::DeleteValue(key, name));
            continue;
        }
        let (kind, bytes) = if data.starts_with('"') {
            let (s, _) = quoted(data).ok_or_else(|| err("chaîne non terminée"))?;
            (REG_SZ, utf16z(&s))
        } else if let Some(v) = data.strip_prefix("dword:") {
            let v = u32::from_str_radix(v.trim(), 16).map_err(|_| err("dword invalide"))?;
            (REG_DWORD, v.to_le_bytes().to_vec())
        } else if let Some(v) = data.strip_prefix("hex:") {
            (REG_BINARY, hex_bytes(v).map_err(|e| err(&e))?)
        } else if let Some(v) = data.strip_prefix("hex(") {
            let (k, v) = v
                .split_once("):")
                .ok_or_else(|| err("type hex(…) invalide"))?;
            let kind = u32::from_str_radix(k, 16).map_err(|_| err("type hex(…) invalide"))?;
            let bytes = hex_bytes(v).map_err(|e| err(&e))?;
            // REGEDIT4 : chaînes en ANSI, à convertir vers l'UTF-16 du stockage.
            let bytes = if !unicode && is_string(kind) {
                from_ansi(kind, &bytes)
            } else {
                bytes
            };
            (kind, bytes)
        } else {
            return Err(err("type de données inconnu"));
        };
        ops.push(Op::Set(key, name, kind, bytes));
    }
    Ok(ops)
}

/// Applique les opérations en une seule écriture du registre.
pub fn apply(ops: &[Op]) -> Result<usize, String> {
    write(|d| {
        for op in ops {
            match op {
                Op::Key(k) => {
                    d.ensure(k);
                }
                Op::DeleteKey(k) => {
                    d.delete_tree(k);
                }
                Op::Set(k, n, kind, b) => d.set(k, n, *kind, b.clone()),
                Op::DeleteValue(k, n) => {
                    d.delete_value(k, n);
                }
            }
        }
        Ok(ops.len())
    })
    .map_err(|e| format!("écriture du registre impossible (erreur {e})"))
}

pub fn import_file(path: &Path) -> Result<usize, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    apply(&parse(&decode(&bytes))?)
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Données hexadécimales coupées à 80 colonnes comme regedit.
fn hex_line(prefix: &str, bytes: &[u8]) -> String {
    let mut out = String::from(prefix);
    let mut col = prefix.len();
    for (i, b) in bytes.iter().enumerate() {
        let last = i + 1 == bytes.len();
        let item = if last {
            format!("{b:02x}")
        } else {
            format!("{b:02x},")
        };
        if col + item.len() > 77 && !last {
            out.push_str("\\\r\n  ");
            col = 2;
        }
        out.push_str(&item);
        col += item.len();
    }
    out
}

pub fn value_line(v: &Value) -> String {
    let name = if v.name.is_empty() {
        "@".to_string()
    } else {
        format!("\"{}\"", escape(&v.name))
    };
    let prefix = format!("{name}=");
    let clean_sz = v.kind == REG_SZ && {
        let units = super::utf16_units(&v.bytes);
        units.last() == Some(&0) && !units[..units.len() - 1].contains(&0)
    };
    if clean_sz {
        format!("{prefix}\"{}\"", escape(&string_of(&v.bytes)))
    } else if v.kind == REG_DWORD && v.bytes.len() == 4 {
        let n = u32::from_le_bytes(v.bytes[..4].try_into().unwrap_or([0; 4]));
        format!("{prefix}dword:{n:08x}")
    } else if v.kind == REG_BINARY {
        hex_line(&format!("{prefix}hex:"), &v.bytes)
    } else {
        hex_line(&format!("{prefix}hex({:x}):", v.kind), &v.bytes)
    }
}

fn export_key(d: &Data, path: &str, out: &mut String, count: &mut usize) {
    out.push_str(&format!("[{}]\r\n", display_path(path)));
    for v in d.values(path) {
        out.push_str(&value_line(&v));
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    *count += 1;
    for child in d.subkeys(path) {
        export_key(d, &join(path, &child), out, count);
    }
}

/// Texte .reg (version 5.00) d'une clé, ou de tout le registre.
pub fn export_text(d: &Data, key: Option<&str>) -> Result<(String, usize), String> {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    let mut count = 0;
    match key {
        Some(k) => {
            if !d.exists(k) {
                return Err("clé introuvable".into());
            }
            export_key(d, k, &mut out, &mut count);
        }
        None => {
            for root in super::ROOTS {
                export_key(d, root, &mut out, &mut count);
            }
        }
    }
    Ok((out, count))
}

pub fn export_file(path: &Path, key: Option<&str>) -> Result<usize, String> {
    let (text, count) = read(|d| Ok(export_text(d, key)))
        .map_err(|e| format!("lecture du registre (erreur {e})"))??;
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(count)
}

/// Texte lisible d'une valeur (regedit, `reg query`).
pub fn value_text(v: &Value) -> String {
    match v.kind {
        REG_SZ | REG_EXPAND_SZ => string_of(&v.bytes),
        REG_MULTI_SZ => multi_of(&v.bytes).join("\\0"),
        REG_DWORD if v.bytes.len() == 4 => {
            let n = u32::from_le_bytes(v.bytes[..4].try_into().unwrap_or([0; 4]));
            format!("0x{n:08x} ({n})")
        }
        REG_QWORD if v.bytes.len() == 8 => {
            let n = u64::from_le_bytes(v.bytes[..8].try_into().unwrap_or([0; 8]));
            format!("0x{n:016x} ({n})")
        }
        _ => v
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub fn type_name(kind: u32) -> &'static str {
    match kind {
        0 => "REG_NONE",
        1 => "REG_SZ",
        2 => "REG_EXPAND_SZ",
        3 => "REG_BINARY",
        4 => "REG_DWORD",
        5 => "REG_DWORD_BIG_ENDIAN",
        6 => "REG_LINK",
        7 => "REG_MULTI_SZ",
        8 => "REG_RESOURCE_LIST",
        11 => "REG_QWORD",
        _ => "REG_UNKNOWN",
    }
}
