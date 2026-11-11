//! Relève la signature de chaque fonction `extern "win64"` du runtime.
//!
//! La couche 32 bits (`src/wow32`, #232) en a besoin pour appeler ces
//! fonctions depuis du code x86 : combien de cases de 4 octets lire sur la
//! pile 32 bits, lesquelles sont des flottants, et où rendre le résultat
//! (eax, edx:eax ou ST0). Le tableau produit est inclus par `wow32::sigs`.
//!
//! Codes d'une case : `i` entier ou pointeur sur 32 bits, `q` entier 64 bits
//! (deux cases), `d` double (deux cases), `f` float, `c` pointeur de
//! fonction (rappel), `p` pointeur vers une case de la taille d'un pointeur
//! (HANDLE, SIZE_T ou pointeur rendus par l'appelé : 4 octets en x86, 8 en
//! 64 bits), `?` type inconnu ou structure passée par valeur.
//! Retour : `v` rien, sinon le même code.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `src/dlls/kernel32.rs` → `dlls::kernel32` ; `src/clr/mod.rs` → `clr`.
fn module_path(src: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(src).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".rs").to_string();
    }
    if matches!(
        parts.last().map(String::as_str),
        Some("mod" | "main" | "lib")
    ) {
        parts.pop();
    }
    parts.join("::")
}

/// Découpe sur les virgules de premier niveau.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut prev = ' ';
    for c in s.chars() {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            // « -> » n'ouvre ni ne ferme rien.
            '>' if prev != '-' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        prev = c;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn strip_comments(t: &str) -> String {
    // Commentaires de fin de ligne seulement, et seulement quand aucun
    // guillemet ouvert ne précède « // » sur la ligne (URL dans une chaîne).
    let mut out = String::with_capacity(t.len());
    for line in t.lines() {
        let mut cut = line.len();
        let mut search = 0;
        while let Some(p) = line[search..].find("//") {
            let at = search + p;
            if line[..at].matches('"').count() % 2 == 0 {
                cut = at;
                break;
            }
            search = at + 2;
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Type de la taille d'un pointeur : `usize`, `isize`, pointeur, ou alias de l'un d'eux.
fn pointer_sized(t: &str, aliases: &HashMap<String, String>, depth: u32) -> bool {
    if t.starts_with('*') {
        return true;
    }
    let last = t.rsplit("::").next().unwrap_or(t);
    match last {
        "usize" | "isize" => true,
        _ => match aliases.get(last) {
            Some(target) if depth < 8 => pointer_sized(target.trim(), aliases, depth + 1),
            _ => false,
        },
    }
}

fn classify(ty: &str, aliases: &HashMap<String, String>, depth: u32) -> char {
    let t: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.trim();
    // Pointeur vers une case de la taille d'un pointeur (`*mut usize`,
    // `*mut HANDLE`, `*mut *mut u16`…) : 4 octets côté x86, 8 côté 64 bits.
    if let Some(inner) = t.strip_prefix("*mut ") {
        if pointer_sized(inner.trim(), aliases, depth) {
            return 'p';
        }
    }
    if t.starts_with('*') || t.starts_with('&') {
        return 'i';
    }
    // `Option<fn(…)>`, `Option<&T>`… : même disposition que le type porté
    // (pointeur nul pour None). Sans cela, les Enum* et CreateThreadpool*
    // à rappel restaient « pas encore disponibles en 32 bits ».
    if let Some(inner) = t.strip_prefix("Option<").and_then(|r| r.strip_suffix('>')) {
        return match classify(inner, aliases, depth + 1) {
            'p' => 'i',
            k => k,
        };
    }
    if t.contains("fn(") || t.contains("fn (") {
        return 'c';
    }
    let last = t.rsplit("::").next().unwrap_or(t);
    match last {
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "usize" | "isize" | "bool" | "c_int"
        | "c_uint" | "c_char" | "c_uchar" | "c_short" | "c_ushort" => 'i',
        "u64" | "i64" | "c_ulonglong" | "c_longlong" => 'q',
        "f64" | "c_double" => 'd',
        "f32" | "c_float" => 'f',
        _ => match aliases.get(last) {
            Some(target) if depth < 8 => classify(target, aliases, depth + 1),
            _ => '?',
        },
    }
}

/// Commit de construction, affiché par `winlux --version` (#1266) : sans
/// lui, deux binaires de deux têtes de master disaient tous deux « v0.2.20 »
/// et rien ne montrait lequel tournait vraiment dans une VM. `WINLUX_COMMIT`
/// l'impose (construction hors dépôt git) ; sinon `git rev-parse HEAD`, plus
/// « +modifié » si des fichiers suivis diffèrent de ce commit.
fn build_commit(manifest: &Path) {
    println!("cargo:rerun-if-env-changed=WINLUX_COMMIT");
    let git = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(manifest)
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    // Un commit ou un changement de branche relance ce script : HEAD, la
    // référence de la branche et packed-refs (les sources sont déjà suivies).
    let mut watched = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(reference);
    }
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            let path = manifest.join(path);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    let commit = match std::env::var("WINLUX_COMMIT") {
        Ok(forced) if !forced.trim().is_empty() => forced.trim().to_string(),
        _ => match git(&["rev-parse", "--short=12", "HEAD"]) {
            Some(head) if !head.is_empty() => {
                let modified = git(&["status", "--porcelain", "--untracked-files=no"])
                    .is_some_and(|status| !status.is_empty());
                if modified {
                    format!("{head}+modifié")
                } else {
                    head
                }
            }
            _ => "inconnu".to_string(),
        },
    };
    println!("cargo:rustc-env=WINLUX_BUILD_COMMIT={commit}");
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    build_commit(&manifest);
    let src = manifest.join("src");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    let mut files = Vec::new();
    walk(&src, &mut files);

    let texts: Vec<(String, String)> = files
        .iter()
        .filter(|f| !f.starts_with(src.join("remote")))
        .filter_map(|f| {
            let raw = std::fs::read(f).ok()?;
            Some((
                module_path(&src, f),
                strip_comments(&String::from_utf8_lossy(&raw)),
            ))
        })
        .collect();

    // Alias de types (`type HANDLE = usize;`), tous modules confondus.
    let mut aliases: HashMap<String, String> = HashMap::new();
    for (_, t) in &texts {
        for line in t.lines() {
            let l = line.trim();
            let l = l
                .strip_prefix("pub(crate) ")
                .or_else(|| l.strip_prefix("pub "))
                .unwrap_or(l);
            let Some(rest) = l.strip_prefix("type ") else {
                continue;
            };
            let Some((name, target)) = rest.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.contains('<') {
                continue;
            }
            let target = target.trim().trim_end_matches(';').trim();
            aliases.insert(name.to_string(), target.to_string());
        }
    }

    let mut out =
        String::from("// Généré par build.rs : signatures des fonctions extern \"win64\".\n");
    out.push_str("pub static SIGS: &[(&str, &str, &str)] = &[\n");
    let needle = "extern \"win64\" fn ";
    for (module, t) in &texts {
        let mut from = 0;
        while let Some(pos) = t[from..].find(needle) {
            let start = from + pos + needle.len();
            from = start;
            let rest = &t[start..];
            let name_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..name_end];
            let after = rest[name_end..].trim_start();
            // Fonctions génériques : pas de signature unique.
            if name.is_empty() || !after.starts_with('(') {
                continue;
            }
            let open = start + name_end + (rest[name_end..].len() - after.len());
            let bytes = t.as_bytes();
            let mut depth = 0i32;
            let mut j = open;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let params = &t[open + 1..j];
            let tail = &t[j + 1..];
            let body_at = tail.find(['{', ';']).unwrap_or(tail.len());
            let head = tail[..body_at].trim();
            let head = head.split(" where").next().unwrap_or(head).trim();
            let ret = match head.strip_prefix("->") {
                None => 'v',
                Some(r) => {
                    let r = r.trim();
                    if r == "!" || r == "()" {
                        'v'
                    } else {
                        match classify(r, &aliases, 0) {
                            'c' | 'p' => 'i',
                            k => k,
                        }
                    }
                }
            };
            let mut kinds = String::new();
            for p in split_top(params) {
                let p = p.trim();
                if p.is_empty() {
                    continue;
                }
                if p == "..." || p.ends_with("...") {
                    kinds.push('?');
                    continue;
                }
                let ty = p.split_once(':').map(|(_, t)| t).unwrap_or(p);
                kinds.push(classify(ty, &aliases, 0));
            }
            let _ = writeln!(out, "    ({module:?}, {name:?}, \"{kinds}>{ret}\"),");
        }
    }
    out.push_str("];\n");

    // Tables de méthodes écrites comme des structures dont chaque champ est
    // un `unsafe extern "win64" fn(…)` (Direct3D 11, DXGI) : signature de
    // chaque champ dans l'ordre, pour leurs passerelles x86 (#1090).
    out.push_str("pub static VTABLE_SIGS: &[(&str, &str, &[&str])] = &[\n");
    for (module, t) in &texts {
        let mut from = 0;
        while let Some(pos) = t[from..].find("struct ") {
            let start = from + pos + "struct ".len();
            from = start;
            let rest = &t[start..];
            let name_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..name_end];
            let after = rest[name_end..].trim_start();
            if name.is_empty() || !after.starts_with('{') {
                continue;
            }
            let open = start + name_end + (rest[name_end..].len() - after.len());
            let bytes = t.as_bytes();
            let mut depth = 0i32;
            let mut j = open;
            while j < bytes.len() {
                match bytes[j] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let fields = split_top(&t[open + 1..j]);
            let mut sigs = Vec::new();
            let mut all_fns = !fields.is_empty();
            for f in &fields {
                let f = f.trim();
                if f.is_empty() {
                    continue;
                }
                let Some((_, ty)) = f.split_once(':') else {
                    all_fns = false;
                    break;
                };
                let mut ty = ty.trim();
                // Type de fonction nommé (`type SetCb1 = unsafe extern "win64" fn(…)`).
                if let Some(target) = aliases.get(ty.rsplit("::").next().unwrap_or(ty)) {
                    if target.contains("extern \"win64\" fn") {
                        ty = target.as_str();
                    }
                }
                let Some(at) = ty.find("extern \"win64\" fn") else {
                    // Table prolongée par une sous-structure (`ext: ext::DeviceExt`) :
                    // ses champs suivent, résolus à l'exécution.
                    let last = ty.rsplit("::").next().unwrap_or(ty);
                    if last.starts_with(|c: char| c.is_ascii_uppercase())
                        && last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        sigs.push(format!("@{last}"));
                        continue;
                    }
                    all_fns = false;
                    break;
                };
                let sig = &ty[at + "extern \"win64\" fn".len()..];
                let Some(op) = sig.find('(') else {
                    all_fns = false;
                    break;
                };
                let sb = sig.as_bytes();
                let mut d = 0i32;
                let mut k = op;
                while k < sb.len() {
                    match sb[k] {
                        b'(' => d += 1,
                        b')' => {
                            d -= 1;
                            if d == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                let params = &sig[op + 1..k.min(sig.len())];
                let tail = sig.get(k + 1..).unwrap_or("").trim();
                let ret = match tail.strip_prefix("->") {
                    None => 'v',
                    Some(r) => {
                        let r = r.trim();
                        if r == "()" {
                            'v'
                        } else {
                            match classify(r, &aliases, 0) {
                                'c' | 'p' => 'i',
                                k => k,
                            }
                        }
                    }
                };
                let mut kinds = String::new();
                for p in split_top(params) {
                    let p = p.trim();
                    if !p.is_empty() {
                        kinds.push(classify(p, &aliases, 0));
                    }
                }
                sigs.push(format!("{kinds}>{ret}"));
            }
            if all_fns && sigs.iter().filter(|s| !s.starts_with('@')).count() >= 3 {
                let list: Vec<String> = sigs.iter().map(|s| format!("{s:?}")).collect();
                let _ = writeln!(out, "    ({module:?}, {name:?}, &[{}]),", list.join(", "));
            }
        }
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("win64_sigs.rs");
    std::fs::write(dest, out).unwrap();
}
