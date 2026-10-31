//! Base de données d'un paquet .msi : pool de chaînes, catalogue des tables
//! et des colonnes, lignes stockées colonne par colonne.

use super::cfb::Compound;
use std::collections::HashMap;

const MSITYPE_VALID: u32 = 0x0100;
const MSITYPE_STRING: u32 = 0x0800;
const MSITYPE_NULLABLE: u32 = 0x1000;
const MSITYPE_TEMPORARY: u32 = 0x4000;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i32),
    Str(String),
}

impl Value {
    pub fn text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(n) => n.to_string(),
            Value::Str(s) => s.clone(),
        }
    }

    pub fn int(&self) -> Option<i32> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Null => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null) || matches!(self, Value::Str(s) if s.is_empty())
    }
}

#[derive(Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// Ligne d'une table, lue par nom de colonne.
pub struct Row<'a> {
    table: &'a Table,
    values: &'a [Value],
}

impl Row<'_> {
    pub fn get(&self, name: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.table
            .col(name)
            .and_then(|i| self.values.get(i))
            .unwrap_or(&NULL)
    }

    pub fn str(&self, name: &str) -> String {
        self.get(name).text()
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        self.get(name).int()
    }
}

pub struct Database {
    pub tables: HashMap<String, Table>,
    pub streams: HashMap<String, Vec<u8>>,
    /// Modèle du résumé (« x64;1033 », « Intel;1036 »…).
    pub template: String,
    /// Mot du résumé : bit 0 noms courts, bit 1 fichiers compressés.
    pub word_count: i32,
}

fn decode(bytes: &[u8], codepage: u32) -> String {
    if codepage == 65001 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if codepage == 0 || codepage == 1252 || codepage == 28591 {
        return crate::dlls::decode_ansi(bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => crate::dlls::decode_ansi(bytes),
    }
}

fn strings(compound: &Compound) -> Result<(Vec<String>, usize), String> {
    let pool = compound
        .streams
        .get("!_StringPool")
        .ok_or("pool de chaînes absent")?;
    let data = compound
        .streams
        .get("!_StringData")
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let words: Vec<u16> = pool
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    if words.len() < 2 {
        return Ok((vec![String::new()], 2));
    }
    let codepage = words[0] as u32 | (((words[1] & 0x7FFF) as u32) << 16);
    let refsize = if words[1] & 0x8000 != 0 { 3 } else { 2 };
    let mut out = vec![String::new()];
    let mut offset = 0usize;
    let mut i = 1;
    let count = words.len() / 2;
    while i < count {
        let len = words[i * 2] as usize;
        let refs = words[i * 2 + 1];
        if len == 0 && refs == 0 {
            out.push(String::new());
            i += 1;
            continue;
        }
        let len = if len == 0 {
            // Chaîne de plus de 64 Ko : longueur sur l'entrée suivante.
            let hi = *words.get(i * 2 + 3).ok_or("pool de chaînes tronqué")? as usize;
            let lo = *words.get(i * 2 + 2).ok_or("pool de chaînes tronqué")? as usize;
            i += 2;
            (hi << 16) | lo
        } else {
            i += 1;
            len
        };
        let bytes = data
            .get(offset..offset + len)
            .ok_or("données de chaînes tronquées")?;
        out.push(decode(bytes, codepage));
        offset += len;
    }
    Ok((out, refsize))
}

fn read_uint(bytes: &[u8], at: usize, size: usize) -> Option<u32> {
    let b = bytes.get(at..at + size)?;
    Some(match size {
        2 => u16::from_le_bytes([b[0], b[1]]) as u32,
        3 => b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16,
        _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    })
}

struct Column {
    name: String,
    kind: u32,
}

fn column_size(kind: u32, refsize: usize) -> usize {
    if kind & !MSITYPE_NULLABLE == MSITYPE_STRING | MSITYPE_VALID {
        2 // binaire : la donnée est dans un flux séparé
    } else if kind & MSITYPE_STRING != 0 {
        refsize
    } else if kind & 0xFF <= 2 {
        2
    } else {
        4
    }
}

fn read_table(
    bytes: &[u8],
    columns: &[Column],
    strings: &[String],
    refsize: usize,
) -> Vec<Vec<Value>> {
    let stored: Vec<&Column> = columns
        .iter()
        .filter(|c| c.kind & MSITYPE_TEMPORARY == 0)
        .collect();
    let row_size: usize = stored.iter().map(|c| column_size(c.kind, refsize)).sum();
    if row_size == 0 {
        return Vec::new();
    }
    let n = bytes.len() / row_size;
    let mut rows = vec![Vec::with_capacity(columns.len()); n];
    let mut base = 0;
    for c in columns {
        if c.kind & MSITYPE_TEMPORARY != 0 {
            rows.iter_mut().for_each(|r| r.push(Value::Null));
            continue;
        }
        let size = column_size(c.kind, refsize);
        let binary = c.kind & !MSITYPE_NULLABLE == MSITYPE_STRING | MSITYPE_VALID;
        for (r, row) in rows.iter_mut().enumerate() {
            let raw = read_uint(bytes, base + r * size, size).unwrap_or(0);
            row.push(if binary {
                if raw == 0 {
                    Value::Null
                } else {
                    Value::Int(1)
                }
            } else if c.kind & MSITYPE_STRING != 0 {
                strings
                    .get(raw as usize)
                    .filter(|s| !s.is_empty())
                    .map_or(Value::Null, |s| Value::Str(s.clone()))
            } else if raw == 0 {
                Value::Null
            } else if size == 2 {
                Value::Int(raw as i32 - 0x8000)
            } else {
                Value::Int((raw ^ 0x8000_0000) as i32)
            });
        }
        base += size * n;
    }
    rows
}

fn summary(stream: &[u8]) -> (String, i32) {
    let mut template = String::new();
    let mut words = 0;
    let Some(section) = read_uint(stream, 44, 4).map(|v| v as usize) else {
        return (template, words);
    };
    let count = read_uint(stream, section + 4, 4).unwrap_or(0) as usize;
    for i in 0..count.min(64) {
        let Some(pid) = read_uint(stream, section + 8 + i * 8, 4) else {
            break;
        };
        let off = section + read_uint(stream, section + 12 + i * 8, 4).unwrap_or(0) as usize;
        let kind = read_uint(stream, off, 4).unwrap_or(0);
        match (pid, kind) {
            (7, 30) => {
                let len = read_uint(stream, off + 4, 4).unwrap_or(0) as usize;
                if let Some(b) = stream.get(off + 8..off + 8 + len.min(256)) {
                    template = decode(b.split(|&c| c == 0).next().unwrap_or(b), 1252);
                }
            }
            (15, 3) => words = read_uint(stream, off + 4, 4).unwrap_or(0) as i32,
            _ => {}
        }
    }
    (template, words)
}

impl Database {
    pub fn parse(data: &[u8]) -> Result<Database, String> {
        let compound = Compound::parse(data)?;
        let (strings, refsize) = strings(&compound)?;
        let col_defs = [
            Column {
                name: "Table".into(),
                kind: MSITYPE_STRING | MSITYPE_VALID | 64,
            },
            Column {
                name: "Number".into(),
                kind: MSITYPE_VALID | 2,
            },
            Column {
                name: "Name".into(),
                kind: MSITYPE_STRING | MSITYPE_VALID | 64,
            },
            Column {
                name: "Type".into(),
                kind: MSITYPE_VALID | 2,
            },
        ];
        let raw_columns = compound
            .streams
            .get("!_Columns")
            .ok_or("catalogue des colonnes absent")?;
        let mut defs: HashMap<String, Vec<(i32, Column)>> = HashMap::new();
        for row in read_table(raw_columns, &col_defs, &strings, refsize) {
            let (Value::Str(table), Some(number), Value::Str(name), Some(kind)) =
                (&row[0], row[1].int(), &row[2], row[3].int())
            else {
                continue;
            };
            defs.entry(table.clone()).or_default().push((
                number,
                Column {
                    name: name.clone(),
                    kind: (kind & 0xFFFF) as u32,
                },
            ));
        }
        let mut tables = HashMap::new();
        for (name, mut cols) in defs {
            cols.sort_by_key(|c| c.0);
            let cols: Vec<Column> = cols.into_iter().map(|c| c.1).collect();
            let rows = compound
                .streams
                .get(&format!("!{name}"))
                .map(|b| read_table(b, &cols, &strings, refsize))
                .unwrap_or_default();
            tables.insert(
                name,
                Table {
                    columns: cols.into_iter().map(|c| c.name).collect(),
                    rows,
                },
            );
        }
        let (template, word_count) = compound
            .streams
            .get("\u{5}SummaryInformation")
            .map(|s| summary(s))
            .unwrap_or_default();
        Ok(Database {
            tables,
            streams: compound.streams,
            template,
            word_count,
        })
    }

    pub fn rows<'a>(&'a self, table: &str) -> impl Iterator<Item = Row<'a>> + 'a {
        self.tables
            .get(table)
            .into_iter()
            .flat_map(|t| t.rows.iter().map(move |values| Row { table: t, values }))
    }

    pub fn property(&self, name: &str) -> Option<String> {
        self.rows("Property")
            .find(|r| r.str("Property") == name)
            .map(|r| r.str("Value"))
    }

    pub fn is_64bit(&self) -> bool {
        let platform = self.template.split(';').next().unwrap_or("");
        platform.eq_ignore_ascii_case("x64")
            || platform.eq_ignore_ascii_case("Intel64")
            || platform.eq_ignore_ascii_case("Arm64")
            || platform.eq_ignore_ascii_case("AMD64")
    }
}
