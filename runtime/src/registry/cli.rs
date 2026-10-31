//! `reg.exe` et `regedit.exe` intégrés au runtime, comme msiexec : les
//! installateurs qui lancent `reg add …` ou `regedit /s réglages.reg`
//! écrivent dans le même registre que les applis.

use super::regfile::{self, type_name, value_text};
use super::{
    canonical, display_path, join, multi_bytes, read, utf16z, write, Value, REG_BINARY, REG_DWORD,
    REG_EXPAND_SZ, REG_MULTI_SZ, REG_NONE, REG_QWORD, REG_SZ,
};
use std::path::{Path, PathBuf};

fn base_name(exe: &Path) -> String {
    let text = exe.to_string_lossy();
    let name = text.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

pub fn is_reg(exe: &Path) -> bool {
    base_name(exe) == "reg"
}

pub fn is_regedit(exe: &Path) -> bool {
    matches!(base_name(exe).as_str(), "regedit" | "regedt32")
}

fn file_path(arg: &str) -> PathBuf {
    if arg.starts_with('/') && Path::new(arg).exists() {
        PathBuf::from(arg)
    } else {
        crate::paths::to_linux(arg)
    }
}

fn error(msg: &str) -> i32 {
    eprintln!("ERREUR : {msg}");
    1
}

/// `regedit [/s] fichier.reg`, `regedit /e fichier.reg [clé]`, ou l'éditeur.
pub fn regedit_main(args: &[String]) -> i32 {
    let mut silent = false;
    let mut export = false;
    let mut rest = Vec::new();
    for a in args {
        match a.to_ascii_lowercase().as_str() {
            "/s" | "-s" => silent = true,
            "/e" | "-e" => export = true,
            "/m" | "-m" | "/c" | "-c" => {}
            _ => rest.push(a.clone()),
        }
    }
    if export {
        let Some(file) = rest.first() else {
            return error("regedit /e : fichier de destination manquant");
        };
        let key = match rest.get(1) {
            Some(k) => match canonical(k) {
                Some(k) => Some(k),
                None => return error("racine du registre inconnue"),
            },
            None => None,
        };
        return match regfile::export_file(&file_path(file), key.as_deref()) {
            Ok(_) => 0,
            Err(e) => error(&e),
        };
    }
    match (rest.first(), silent) {
        (Some(file), true) => match regfile::import_file(&file_path(file)) {
            Ok(_) => 0,
            Err(e) => error(&e),
        },
        // Sans /s, l'éditeur graphique demande confirmation avant d'importer.
        (file, false) => launch_editor(file.map(|f| file_path(f))),
        (None, true) => 0,
    }
}

fn launch_editor(import: Option<PathBuf>) -> i32 {
    let mut cmd = std::process::Command::new("/usr/bin/winlux-shell-app");
    cmd.arg("regedit");
    if let Some(p) = import {
        cmd.env("WINLUX_REGEDIT_IMPORT", crate::paths::to_windows(&p));
    }
    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => error(&format!("éditeur du registre indisponible : {e}")),
    }
}

struct Opts {
    key: Option<String>,
    value: Option<String>,
    all_values: bool,
    kind: u32,
    data: Option<String>,
    separator: String,
    recursive: bool,
    rest: Vec<String>,
}

fn options(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        key: None,
        value: None,
        all_values: false,
        kind: REG_SZ,
        data: None,
        separator: "\\0".into(),
        recursive: false,
        rest: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let next = |it: &mut std::slice::Iter<String>| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("valeur manquante après {a}"))
        };
        match a.to_ascii_lowercase().as_str() {
            "/v" => o.value = Some(next(&mut it)?),
            "/ve" => o.value = Some(String::new()),
            "/va" => o.all_values = true,
            "/t" => {
                o.kind = match next(&mut it)?.to_ascii_uppercase().as_str() {
                    "REG_SZ" => REG_SZ,
                    "REG_EXPAND_SZ" => REG_EXPAND_SZ,
                    "REG_MULTI_SZ" => REG_MULTI_SZ,
                    "REG_BINARY" => REG_BINARY,
                    "REG_DWORD" => REG_DWORD,
                    "REG_QWORD" => REG_QWORD,
                    "REG_NONE" => REG_NONE,
                    t => return Err(format!("type inconnu {t}")),
                }
            }
            "/d" => o.data = Some(next(&mut it)?),
            "/s" => o.recursive = true,
            "/se" => o.separator = next(&mut it)?,
            "/f" | "/y" | "/reg:32" | "/reg:64" => {}
            _ if o.key.is_none() => o.key = Some(a.clone()),
            _ => o.rest.push(a.clone()),
        }
    }
    Ok(o)
}

fn parse_number(text: &str) -> Option<u64> {
    let t = text.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => t.parse().ok(),
    }
}

/// Données d'une valeur saisie en ligne de commande.
pub fn encode_data(kind: u32, text: &str, separator: &str) -> Result<Vec<u8>, String> {
    Ok(match kind {
        REG_SZ | REG_EXPAND_SZ => utf16z(text),
        REG_MULTI_SZ => {
            let items: Vec<String> = if text.is_empty() {
                Vec::new()
            } else {
                text.split(separator).map(str::to_string).collect()
            };
            multi_bytes(&items)
        }
        REG_DWORD => {
            let n = parse_number(text)
                .filter(|&n| n <= u32::MAX as u64)
                .ok_or("nombre DWORD invalide")?;
            (n as u32).to_le_bytes().to_vec()
        }
        REG_QWORD => parse_number(text)
            .ok_or("nombre QWORD invalide")?
            .to_le_bytes()
            .to_vec(),
        _ => {
            let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if !clean.len().is_multiple_of(2) {
                return Err("données hexadécimales invalides".into());
            }
            (0..clean.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&clean[i..i + 2], 16))
                .collect::<Result<_, _>>()
                .map_err(|_| "données hexadécimales invalides")?
        }
    })
}

fn print_value(v: &Value) {
    let name = if v.name.is_empty() {
        "(par défaut)"
    } else {
        &v.name
    };
    println!("    {name}    {}    {}", type_name(v.kind), value_text(v));
}

pub fn reg_main(args: &[String]) -> i32 {
    let Some(command) = args.first() else {
        return error("usage : reg query|add|delete|import|export …");
    };
    let o = match options(&args[1..]) {
        Ok(o) => o,
        Err(e) => return error(&e),
    };
    let key = || {
        o.key
            .as_deref()
            .and_then(canonical)
            .ok_or_else(|| "clé absente ou racine inconnue".to_string())
    };
    let result: Result<(), String> = match command.to_ascii_lowercase().as_str() {
        "query" => key().and_then(|k| query(&k, o.value.as_deref(), o.recursive)),
        "add" => key().and_then(|k| {
            let default = if matches!(o.kind, REG_DWORD | REG_QWORD) {
                "0"
            } else {
                ""
            };
            let bytes = encode_data(o.kind, o.data.as_deref().unwrap_or(default), &o.separator)?;
            write(|d| {
                d.ensure(&k);
                if let Some(name) = &o.value {
                    d.set(&k, name, o.kind, bytes.clone());
                } else if o.data.is_some() {
                    d.set(&k, "", o.kind, bytes.clone());
                }
                Ok(())
            })
            .map_err(|e| format!("écriture impossible (erreur {e})"))
        }),
        "delete" => key().and_then(|k| {
            write(|d| {
                if !d.exists(&k) {
                    return Err(super::ERROR_FILE_NOT_FOUND);
                }
                if let Some(name) = &o.value {
                    if !d.delete_value(&k, name) {
                        return Err(super::ERROR_FILE_NOT_FOUND);
                    }
                } else if o.all_values {
                    for v in d.values(&k) {
                        d.delete_value(&k, &v.name);
                    }
                } else if !d.delete_tree(&k) {
                    return Err(super::ERROR_ACCESS_DENIED);
                }
                Ok(())
            })
            .map_err(|e| match e {
                super::ERROR_FILE_NOT_FOUND => "clé ou valeur introuvable".to_string(),
                e => format!("suppression impossible (erreur {e})"),
            })
        }),
        "import" => match o.key.as_deref() {
            Some(f) => regfile::import_file(&file_path(f)).map(|_| ()),
            None => Err("fichier .reg manquant".into()),
        },
        "export" => match o.rest.first() {
            Some(f) => {
                key().and_then(|k| regfile::export_file(&file_path(f), Some(&k)).map(|_| ()))
            }
            None => Err("fichier de destination manquant".into()),
        },
        other => Err(format!("commande inconnue « {other} »")),
    };
    match result {
        Ok(()) => {
            if command.eq_ignore_ascii_case("query") {
                0
            } else {
                println!("L'opération a réussi.");
                0
            }
        }
        Err(e) => error(&e),
    }
}

fn query(key: &str, value: Option<&str>, recursive: bool) -> Result<(), String> {
    read(|d| {
        if !d.exists(key) {
            return Err(super::ERROR_FILE_NOT_FOUND);
        }
        fn walk(d: &super::Data, key: &str, value: Option<&str>, recursive: bool) {
            println!();
            println!("{}", display_path(key));
            for v in d.values(key) {
                if value.is_none_or(|n| n.eq_ignore_ascii_case(&v.name)) {
                    print_value(&v);
                }
            }
            for child in d.subkeys(key) {
                let path = join(key, &child);
                if recursive {
                    walk(d, &path, value, true);
                } else if value.is_none() {
                    println!("{}", display_path(&path));
                }
            }
        }
        if let Some(name) = value {
            if d.get(key, name).is_none() {
                return Err(super::ERROR_FILE_NOT_FOUND);
            }
        }
        walk(d, key, value, recursive);
        Ok(())
    })
    .map_err(|_| "clé ou valeur introuvable".to_string())
}
