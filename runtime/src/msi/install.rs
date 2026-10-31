//! Installation et désinstallation d'un paquet .msi : propriétés, dossiers,
//! fonctionnalités et composants, fichiers des cabinets, registre,
//! raccourcis du menu Démarrer, inscription dans « Programmes installés ».

use super::cab::Cabinet;
use super::cond::Context;
use super::db::Database;
use crate::dlls::advapi32::{apply_registry_edits, registry_key_is_empty, RegEdit};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const ERROR_INSTALL_FAILURE: i32 = 1603;
pub const ERROR_UNKNOWN_PRODUCT: i32 = 1605;
pub const ERROR_INSTALL_PACKAGE_OPEN_FAILED: i32 = 1619;
pub const ERROR_INSTALL_PACKAGE_INVALID: i32 = 1620;

#[derive(Default)]
pub struct Options {
    pub props: Vec<(String, String)>,
    pub quiet: bool,
    pub admin: bool,
}

/// Journal : écran et, si demandé (`/l*v fichier`), un fichier.
#[derive(Default)]
pub struct Logger {
    pub file: Option<std::fs::File>,
    pub quiet: bool,
}

impl Logger {
    pub fn log(&mut self, msg: &str) {
        if !self.quiet {
            println!("{msg}");
        }
        if let Some(f) = self.file.as_mut() {
            use std::io::Write;
            let _ = writeln!(f, "{msg}");
        }
    }
}

fn long_name(s: &str) -> &str {
    s.rsplit('|').next().unwrap_or(s)
}

fn with_slash(mut s: String) -> String {
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// Dossier « Bureau » de l'utilisateur Linux (XDG), en chemin Windows.
fn desktop_dir() -> String {
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
    with_slash(crate::paths::to_windows(&dir))
}

fn language_id() -> &'static str {
    let lang = std::env::var("LANG").unwrap_or_default();
    match lang.get(..2).unwrap_or("") {
        "en" => "1033",
        "es" => "3082",
        "de" => "1031",
        "it" => "1040",
        _ => "1036",
    }
}

/// Propriétés fournies par le système (version, dossiers standard…).
fn system_props(ctx: &mut Context, db: &Database, opts: &Options) {
    let x64 = db.is_64bit();
    let pf32 = "C:\\Program Files (x86)\\".to_string();
    let pf64 = "C:\\Program Files\\".to_string();
    let roaming = "C:\\AppData\\Roaming\\";
    let menu = format!("{roaming}Microsoft\\Windows\\Start Menu\\");
    let folders: Vec<(&str, String)> = vec![
        ("ROOTDRIVE", "C:\\".into()),
        ("ProgramFilesFolder", pf32.clone()),
        ("ProgramFiles64Folder", pf64.clone()),
        (
            "ProgramFiles6432Folder",
            if x64 { pf64.clone() } else { pf32.clone() },
        ),
        ("CommonFilesFolder", format!("{pf32}Common Files\\")),
        ("CommonFiles64Folder", format!("{pf64}Common Files\\")),
        ("WindowsFolder", "C:\\windows\\".into()),
        ("WindowsVolume", "C:\\".into()),
        ("SystemFolder", "C:\\windows\\system32\\".into()),
        ("System64Folder", "C:\\windows\\system32\\".into()),
        ("System16Folder", "C:\\windows\\system\\".into()),
        ("FontsFolder", "C:\\windows\\Fonts\\".into()),
        ("TempFolder", "C:\\AppData\\Local\\Temp\\".into()),
        ("AppDataFolder", roaming.into()),
        ("LocalAppDataFolder", "C:\\AppData\\Local\\".into()),
        ("CommonAppDataFolder", "C:\\ProgramData\\".into()),
        ("PersonalFolder", "C:\\Documents\\".into()),
        ("MyPicturesFolder", "C:\\Pictures\\".into()),
        ("DesktopFolder", desktop_dir()),
        ("StartMenuFolder", menu.clone()),
        ("ProgramMenuFolder", format!("{menu}Programs\\")),
        ("StartupFolder", format!("{menu}Programs\\Startup\\")),
        (
            "AdminToolsFolder",
            format!("{menu}Programs\\Administrative Tools\\"),
        ),
        (
            "SendToFolder",
            format!("{roaming}Microsoft\\Windows\\SendTo\\"),
        ),
        ("FavoritesFolder", "C:\\Favorites\\".into()),
        (
            "TemplateFolder",
            format!("{roaming}Microsoft\\Windows\\Templates\\"),
        ),
        (
            "RecentFolder",
            format!("{roaming}Microsoft\\Windows\\Recent\\"),
        ),
        (
            "NetHoodFolder",
            format!("{roaming}Microsoft\\Windows\\Network Shortcuts\\"),
        ),
        (
            "PrintHoodFolder",
            format!("{roaming}Microsoft\\Windows\\Printer Shortcuts\\"),
        ),
    ];
    let user = std::env::var("USER").unwrap_or_else(|_| "winlux".into());
    let lang = language_id();
    let fixed: Vec<(&str, String)> = vec![
        ("VersionNT", "603".into()),
        ("VersionNT64", "603".into()),
        ("WindowsBuild", "9600".into()),
        ("ServicePackLevel", "0".into()),
        ("Msix64", "6".into()),
        ("Privileged", "1".into()),
        ("AdminUser", "1".into()),
        ("LogonUser", user.clone()),
        ("USERNAME", user),
        ("ComputerName", "WINLUX".into()),
        ("SystemLanguageID", lang.into()),
        ("UserLanguageID", lang.into()),
        ("UILevel", if opts.quiet { "2" } else { "5" }.into()),
        ("ProductState", "-1".into()),
        ("SourceDir", String::new()),
    ];
    for (k, v) in folders.into_iter().chain(fixed) {
        ctx.props.insert(k.to_string(), v);
    }
}

struct DirInfo {
    target: String,
    source: String,
}

fn resolve_dirs(db: &Database, ctx: &Context, admin: bool) -> HashMap<String, DirInfo> {
    let mut rows: HashMap<String, (String, String)> = HashMap::new();
    for r in db.rows("Directory") {
        rows.insert(
            r.str("Directory"),
            (r.str("Directory_Parent"), r.str("DefaultDir")),
        );
    }
    let mut out: HashMap<String, DirInfo> = HashMap::new();
    fn walk(
        key: &str,
        rows: &HashMap<String, (String, String)>,
        ctx: &Context,
        admin: bool,
        out: &mut HashMap<String, DirInfo>,
        depth: usize,
    ) -> (String, String) {
        if let Some(d) = out.get(key) {
            return (d.target.clone(), d.source.clone());
        }
        let (parent, default) = rows.get(key).cloned().unwrap_or_default();
        let target_part = default.split(':').next().unwrap_or("").to_string();
        let source_part = default
            .split(':')
            .nth(1)
            .unwrap_or(&target_part)
            .to_string();
        let root = parent.is_empty() || parent == key || depth > 64;
        let (ptarget, psource) = if root {
            let t = ctx.props.get("TARGETDIR").filter(|v| !v.is_empty());
            (
                with_slash(t.cloned().unwrap_or_else(|| "C:\\".into())),
                String::new(),
            )
        } else {
            walk(&parent, rows, ctx, admin, out, depth + 1)
        };
        let name = long_name(&target_part);
        let mut target = if root || name == "." || name.is_empty() {
            ptarget
        } else {
            format!("{ptarget}{name}\\")
        };
        // Une propriété du même nom (dossier standard, INSTALLDIR=… sur la
        // ligne de commande) remplace le chemin calculé.
        if !(admin && key == "TARGETDIR") {
            if let Some(v) = ctx.props.get(key).filter(|v| !v.is_empty()) {
                target = with_slash(v.clone());
            }
        }
        let sname = long_name(&source_part);
        let source = if root || sname == "." || sname.is_empty() || sname == "SourceDir" {
            psource
        } else {
            format!("{psource}{sname}/")
        };
        out.insert(
            key.to_string(),
            DirInfo {
                target: target.clone(),
                source: source.clone(),
            },
        );
        (target, source)
    }
    let keys: Vec<String> = rows.keys().cloned().collect();
    for k in keys {
        walk(&k, &rows, ctx, admin, &mut out, 0);
    }
    out
}

/// Actions personnalisées sûres : affectation de propriété (51) et de
/// dossier (35) avant le calcul des chemins.
fn run_property_actions(db: &Database, ctx: &mut Context, log: &mut Logger) {
    let actions: HashMap<String, (i32, String, String)> = db
        .rows("CustomAction")
        .map(|r| {
            (
                r.str("Action"),
                (r.int("Type").unwrap_or(0), r.str("Source"), r.str("Target")),
            )
        })
        .collect();
    let mut seq: Vec<(i32, String, String)> = db
        .rows("InstallExecuteSequence")
        .map(|r| {
            (
                r.int("Sequence").unwrap_or(0),
                r.str("Action"),
                r.str("Condition"),
            )
        })
        .collect();
    seq.sort();
    let cost = seq
        .iter()
        .find(|s| s.1 == "CostFinalize")
        .map(|s| s.0)
        .unwrap_or(1000);
    for (n, action, cond) in &seq {
        let Some((kind, source, target)) = actions.get(action) else {
            continue;
        };
        if !ctx.eval(cond) {
            continue;
        }
        match kind & 0x3F {
            51 | 35 if *n <= cost => {
                let value = ctx.format(target);
                ctx.props.insert(source.clone(), value);
            }
            51 | 35 => {}
            _ => log.log(&format!("Action personnalisée ignorée : {action}")),
        }
    }
}

fn selected_components(db: &Database, ctx: &Context) -> HashSet<String> {
    let install_level: i32 = ctx.get("INSTALLLEVEL").parse().unwrap_or(1);
    let addlocal = ctx.get("ADDLOCAL");
    let remove = ctx.get("REMOVE");
    let all = addlocal.eq_ignore_ascii_case("ALL");
    let added: HashSet<&str> = addlocal.split(',').map(str::trim).collect();
    let removed: HashSet<&str> = remove.split(',').map(str::trim).collect();
    let mut levels: HashMap<String, (i32, String)> = db
        .rows("Feature")
        .map(|r| {
            (
                r.str("Feature"),
                (r.int("Level").unwrap_or(1), r.str("Feature_Parent")),
            )
        })
        .collect();
    for r in db.rows("Condition") {
        if ctx.eval(&r.str("Condition")) {
            if let Some(l) = levels.get_mut(&r.str("Feature_")) {
                l.0 = r.int("Level").unwrap_or(l.0);
            }
        }
    }
    let on = |name: &str, levels: &HashMap<String, (i32, String)>| -> bool {
        let mut cur = name.to_string();
        for _ in 0..32 {
            let Some((level, parent)) = levels.get(&cur) else {
                return true;
            };
            let chosen = if removed.contains(cur.as_str()) || remove.eq_ignore_ascii_case("ALL") {
                false
            } else if all || added.contains(cur.as_str()) {
                *level > 0
            } else {
                *level > 0 && *level <= install_level
            };
            if !chosen {
                return false;
            }
            if parent.is_empty() || *parent == cur {
                return true;
            }
            cur = parent.clone();
        }
        true
    };
    let features: HashSet<String> = levels.keys().filter(|f| on(f, &levels)).cloned().collect();
    let mut comps: HashSet<String> = if levels.is_empty() {
        db.rows("Component").map(|r| r.str("Component")).collect()
    } else {
        db.rows("FeatureComponents")
            .filter(|r| features.contains(&r.str("Feature_")))
            .map(|r| r.str("Component_"))
            .collect()
    };
    for r in db.rows("Component") {
        if !ctx.eval(&r.str("Condition")) {
            comps.remove(&r.str("Component"));
        }
    }
    comps
}

/// Raccourci Windows (.lnk) minimal : chemin cible, arguments, dossier.
pub fn shortcut_bytes(target: &str, args: &str, workdir: &str, desc: &str) -> Vec<u8> {
    let mut flags: u32 = 0x08 | 0x80;
    if !desc.is_empty() {
        flags |= 0x04;
    }
    if !workdir.is_empty() {
        flags |= 0x10;
    }
    if !args.is_empty() {
        flags |= 0x20;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x4Cu32.to_le_bytes());
    out.extend_from_slice(&[
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ]);
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0x20u32.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[0; 2 + 2 + 4 + 4]);
    let mut put = |s: &str| {
        let units: Vec<u16> = s.encode_utf16().collect();
        out.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for u in units {
            out.extend_from_slice(&u.to_le_bytes());
        }
    };
    if !desc.is_empty() {
        put(desc);
    }
    put(target);
    if !workdir.is_empty() {
        put(workdir);
    }
    if !args.is_empty() {
        put(args);
    }
    out
}

/// Ce que l'installation a créé, pour pouvoir le retirer ensuite.
#[derive(Default)]
pub struct Manifest {
    pub product: String,
    pub upgrade: String,
    pub name: String,
    pub version: String,
    pub files: Vec<PathBuf>,
    pub dirs: Vec<PathBuf>,
    pub values: Vec<(String, String)>,
    pub keys: Vec<String>,
}

pub fn manifest_dir() -> PathBuf {
    crate::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp/winlux"))
        .join("msi")
}

fn manifest_path(code: &str) -> PathBuf {
    let safe: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    manifest_dir().join(format!("{}.txt", safe.to_ascii_uppercase()))
}

impl Manifest {
    fn save(&self) -> std::io::Result<()> {
        let mut text = String::from("WINLUX-MSI-1\n");
        for (k, v) in [
            ("product", &self.product),
            ("upgrade", &self.upgrade),
            ("name", &self.name),
            ("version", &self.version),
        ] {
            text.push_str(&format!("{k}\t{}\n", clean(v)));
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
        let path = manifest_path(&self.product);
        std::fs::create_dir_all(manifest_dir())?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    pub fn load(path: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(path).ok()?;
        if !text.starts_with("WINLUX-MSI-1") {
            return None;
        }
        let mut m = Manifest::default();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            match parts.as_slice() {
                ["product", v] => m.product = v.to_string(),
                ["upgrade", v] => m.upgrade = v.to_string(),
                ["name", v] => m.name = v.to_string(),
                ["version", v] => m.version = v.to_string(),
                ["file", v] => m.files.push(PathBuf::from(v)),
                ["dir", v] => m.dirs.push(PathBuf::from(v)),
                ["value", k, n] => m.values.push((k.to_string(), n.to_string())),
                ["key", k] => m.keys.push(k.to_string()),
                _ => {}
            }
        }
        Some(m)
    }

    pub fn all() -> Vec<Manifest> {
        let Ok(dir) = std::fs::read_dir(manifest_dir()) else {
            return Vec::new();
        };
        dir.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
            .filter_map(|e| Manifest::load(&e.path()))
            .collect()
    }

    pub fn find(code: &str) -> Option<Manifest> {
        Manifest::load(&manifest_path(code))
    }
}

/// Crée un dossier et retient ceux qui n'existaient pas.
fn mkdirs(path: &Path, created: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut p = path.to_path_buf();
    while !p.exists() {
        missing.push(p.clone());
        if !p.pop() {
            break;
        }
    }
    std::fs::create_dir_all(path)?;
    created.extend(missing.into_iter().rev());
    Ok(())
}

fn find_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn registry_value(raw: &str) -> (u32, Vec<u8>) {
    if let Some(hex) = raw.strip_prefix("#x").or_else(|| raw.strip_prefix("#X")) {
        let bytes = (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect();
        return (3, bytes);
    }
    if let Some(rest) = raw.strip_prefix("#%") {
        return (2, utf16z(rest));
    }
    if let Some(rest) = raw.strip_prefix("##") {
        return (1, utf16z(&format!("#{rest}")));
    }
    if let Some(rest) = raw.strip_prefix('#') {
        if let Ok(n) = rest.trim().parse::<i64>() {
            return (4, (n as u32).to_le_bytes().to_vec());
        }
    }
    if raw.contains('\0') {
        let mut bytes = Vec::new();
        for part in raw.split('\0').filter(|p| !p.is_empty()) {
            bytes.extend(utf16z(part));
        }
        bytes.extend([0, 0]);
        return (7, bytes);
    }
    (1, utf16z(raw))
}

fn hive(root: i32, all_users: bool) -> &'static str {
    match root {
        0 => "hkcr",
        1 => "hkcu",
        2 => "hklm",
        3 => "hku",
        _ if all_users => "hklm",
        _ => "hkcu",
    }
}

fn uninstall_key(code: &str) -> String {
    format!("hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\{code}")
}

/// Code produit, nom et version d'un paquet.
pub fn package_info(msi: &Path) -> Result<(String, String, String), i32> {
    let data = std::fs::read(msi).map_err(|_| ERROR_INSTALL_PACKAGE_OPEN_FAILED)?;
    let db = Database::parse(&data).map_err(|_| ERROR_INSTALL_PACKAGE_INVALID)?;
    let get = |p: &str| clean(&db.property(p).unwrap_or_default());
    Ok((
        get("ProductCode"),
        get("ProductName"),
        get("ProductVersion"),
    ))
}

/// Code produit d'un paquet (pour `/x paquet.msi`).
pub fn product_of(msi: &Path) -> Result<String, i32> {
    package_info(msi).map(|info| info.0)
}

pub fn install(msi: &Path, opts: &Options, log: &mut Logger) -> i32 {
    let data = match std::fs::read(msi) {
        Ok(d) => d,
        Err(e) => {
            log.log(&format!("Impossible d'ouvrir {} : {e}", msi.display()));
            return ERROR_INSTALL_PACKAGE_OPEN_FAILED;
        }
    };
    let db = match Database::parse(&data) {
        Ok(db) => db,
        Err(e) => {
            log.log(&format!("{} : {e}", msi.display()));
            return ERROR_INSTALL_PACKAGE_INVALID;
        }
    };
    match install_db(&db, msi, opts, log) {
        Ok(()) => 0,
        Err(e) => {
            log.log(&format!("Échec de l'installation : {e}"));
            ERROR_INSTALL_FAILURE
        }
    }
}

fn install_db(db: &Database, msi: &Path, opts: &Options, log: &mut Logger) -> Result<(), String> {
    let mut ctx = Context::default();
    if !opts.admin {
        system_props(&mut ctx, db, opts);
    }
    for r in db.rows("Property") {
        ctx.props.insert(r.str("Property"), r.str("Value"));
    }
    for (k, v) in &opts.props {
        ctx.props.insert(k.clone(), v.clone());
    }
    let msi_dir = msi
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    ctx.props.insert(
        "SourceDir".into(),
        with_slash(crate::paths::to_windows(&msi_dir)),
    );
    ctx.props
        .insert("OriginalDatabase".into(), crate::paths::to_windows(msi));
    if opts.admin && ctx.get("TARGETDIR").is_empty() {
        return Err(
            "TARGETDIR=dossier est obligatoire pour une installation administrative".into(),
        );
    }
    let name = ctx.get("ProductName");
    let code = ctx.get("ProductCode");
    let version = ctx.get("ProductVersion");
    let upgrade = ctx.get("UpgradeCode");
    log.log(&format!("Installation de {name} {version}"));
    if !opts.admin {
        // Réinstallation ou mise à niveau : on retire d'abord l'ancienne version.
        for m in Manifest::all() {
            let same = m.product.eq_ignore_ascii_case(&code);
            let upgraded = !upgrade.is_empty() && m.upgrade.eq_ignore_ascii_case(&upgrade);
            if same || upgraded {
                log.log(&format!(
                    "Retrait de la version installée {} {}",
                    m.name, m.version
                ));
                remove_manifest(&m, log);
            }
        }
        for r in db.rows("LaunchCondition") {
            if !ctx.eval(&r.str("Condition")) {
                log.log(&format!(
                    "Condition non vérifiée, installation poursuivie : {}",
                    ctx.format(&r.str("Description"))
                ));
            }
        }
        run_property_actions(db, &mut ctx, log);
    }
    let dirs = resolve_dirs(db, &ctx, opts.admin);
    for (k, d) in &dirs {
        ctx.props.insert(k.clone(), d.target.clone());
    }
    let comps = if opts.admin {
        db.rows("Component").map(|r| r.str("Component")).collect()
    } else {
        selected_components(db, &ctx)
    };
    let comp_dir: HashMap<String, String> = db
        .rows("Component")
        .map(|r| {
            let dir = dirs
                .get(&r.str("Directory_"))
                .map(|d| d.target.clone())
                .unwrap_or_else(|| "C:\\".into());
            (r.str("Component"), dir)
        })
        .collect();
    let comp_source: HashMap<String, String> = db
        .rows("Component")
        .map(|r| {
            let src = dirs
                .get(&r.str("Directory_"))
                .map(|d| d.source.clone())
                .unwrap_or_default();
            (r.str("Component"), src)
        })
        .collect();
    for c in &comps {
        if let Some(d) = comp_dir.get(c) {
            ctx.components.insert(c.clone(), d.clone());
        }
    }
    // Fichiers à poser.
    struct Wanted {
        dest: PathBuf,
        seq: i32,
        compressed: bool,
        source: String,
    }
    let short_names = db.word_count & 1 != 0;
    let compressed_default = db.word_count & 2 != 0;
    let mut wanted: HashMap<String, Wanted> = HashMap::new();
    for r in db.rows("File") {
        let comp = r.str("Component_");
        if !comps.contains(&comp) {
            continue;
        }
        let key = r.str("File");
        let file_name = r.str("FileName");
        let long = long_name(&file_name).to_string();
        let short = file_name.split('|').next().unwrap_or(&long).to_string();
        let dir = comp_dir
            .get(&comp)
            .cloned()
            .unwrap_or_else(|| "C:\\".into());
        let win = format!("{dir}{long}");
        ctx.files.insert(key.clone(), win.clone());
        let attrs = r.int("Attributes").unwrap_or(0);
        let compressed = attrs & 0x4000 != 0 || (compressed_default && attrs & 0x2000 == 0);
        let src_dir = comp_source.get(&comp).cloned().unwrap_or_default();
        wanted.insert(
            key,
            Wanted {
                dest: crate::paths::to_linux(&win),
                seq: r.int("Sequence").unwrap_or(0),
                compressed,
                source: format!("{src_dir}{}", if short_names { short } else { long }),
            },
        );
    }
    let mut manifest = Manifest {
        product: code.clone(),
        upgrade: upgrade.clone(),
        name: name.clone(),
        version: version.clone(),
        ..Manifest::default()
    };
    let mut media: Vec<(i32, String)> = db
        .rows("Media")
        .map(|r| (r.int("LastSequence").unwrap_or(0), r.str("Cabinet")))
        .collect();
    media.sort();
    let media_of = |seq: i32| -> Option<&String> {
        media
            .iter()
            .find(|m| seq <= m.0)
            .or(media.last())
            .map(|m| &m.1)
    };
    let mut by_cab: HashMap<String, Vec<String>> = HashMap::new();
    for (key, w) in &wanted {
        if w.compressed {
            if let Some(cab) = media_of(w.seq).filter(|c| !c.is_empty()) {
                by_cab.entry(cab.clone()).or_default().push(key.clone());
            }
        }
    }
    let total = wanted.len().max(1);
    let mut done = 0usize;
    let mut cabs: Vec<&String> = by_cab.keys().collect();
    cabs.sort();
    for cab in cabs {
        let external;
        let bytes: &[u8] = if let Some(stream) = cab.strip_prefix('#') {
            db.streams
                .get(stream)
                .ok_or_else(|| format!("cabinet intégré {stream} absent"))?
        } else {
            let path = find_insensitive(&msi_dir, cab)
                .ok_or_else(|| format!("cabinet {cab} introuvable à côté du paquet"))?;
            external = std::fs::read(&path).map_err(|e| format!("{cab} : {e}"))?;
            &external
        };
        let cabinet = Cabinet::parse(bytes)?;
        let keys: HashSet<&String> = by_cab[cab].iter().collect();
        let mut error = None;
        cabinet
            .extract(&mut |name, data| {
                if !keys.contains(&name.to_string()) {
                    return Ok(());
                }
                let w = &wanted[name];
                if let Some(parent) = w.dest.parent() {
                    mkdirs(parent, &mut manifest.dirs).map_err(|e| e.to_string())?;
                }
                if let Err(e) = std::fs::write(&w.dest, data) {
                    error = Some(format!("{} : {e}", w.dest.display()));
                    return Err(e.to_string());
                }
                manifest.files.push(w.dest.clone());
                done += 1;
                if done.is_multiple_of(25) || done == total {
                    log.log(&format!("Progression {}%", done * 100 / total));
                }
                Ok(())
            })
            .map_err(|e| error.take().unwrap_or(e))?;
    }
    for w in wanted.values().filter(|w| !w.compressed) {
        let src = w
            .source
            .split('/')
            .filter(|p| !p.is_empty())
            .try_fold(msi_dir.clone(), |dir, part| find_insensitive(&dir, part))
            .ok_or_else(|| format!("fichier source {} introuvable", w.source))?;
        if let Some(parent) = w.dest.parent() {
            mkdirs(parent, &mut manifest.dirs).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&src, &w.dest).map_err(|e| format!("{} : {e}", w.dest.display()))?;
        manifest.files.push(w.dest.clone());
    }
    for r in db.rows("DuplicateFile") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let Some(src) = wanted.get(&r.str("File_")).map(|w| w.dest.clone()) else {
            continue;
        };
        let folder = r.str("DestFolder");
        let dir = if folder.is_empty() {
            comp_dir
                .get(&r.str("Component_"))
                .cloned()
                .unwrap_or_default()
        } else {
            with_slash(ctx.get(&folder))
        };
        let dest_name = r.str("DestName");
        let name = if dest_name.is_empty() {
            src.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            long_name(&dest_name).to_string()
        };
        let dest = crate::paths::to_linux(&format!("{dir}{name}"));
        if let Some(parent) = dest.parent() {
            mkdirs(parent, &mut manifest.dirs).map_err(|e| e.to_string())?;
        }
        if std::fs::copy(&src, &dest).is_ok() {
            manifest.files.push(dest);
        }
    }
    for r in db.rows("CreateFolder") {
        if comps.contains(&r.str("Component_")) {
            if let Some(d) = dirs.get(&r.str("Directory_")) {
                let _ = mkdirs(&crate::paths::to_linux(&d.target), &mut manifest.dirs);
            }
        }
    }
    if opts.admin {
        log.log("Installation administrative terminée.");
        return Ok(());
    }
    // Registre.
    let all_users = ctx.get("ALLUSERS") == "1" || ctx.get("ALLUSERS") == "2";
    let mut edits = Vec::new();
    for r in db.rows("Registry") {
        if !comps.contains(&r.str("Component_")) {
            continue;
        }
        let key = format!(
            "{}\\{}",
            hive(r.int("Root").unwrap_or(-1), all_users),
            ctx.format(&r.str("Key")).trim_matches('\\')
        )
        .to_lowercase();
        let name = ctx.format(&r.str("Name"));
        let value = r.get("Value");
        if matches!(name.as_str(), "+" | "-" | "*") && value.is_null() {
            edits.push(RegEdit::Key(key.clone()));
            manifest.keys.push(key);
            continue;
        }
        let (kind, bytes) = registry_value(&ctx.format(&value.text()));
        edits.push(RegEdit::Set {
            key: key.clone(),
            name: name.clone(),
            kind,
            bytes,
        });
        manifest.keys.push(key.clone());
        manifest.values.push((key, name));
    }
    // Raccourcis (menu Démarrer, Bureau).
    let features: HashSet<String> = db.rows("Feature").map(|r| r.str("Feature")).collect();
    let keypath: HashMap<String, String> = db
        .rows("Component")
        .map(|r| (r.str("Component"), r.str("KeyPath")))
        .collect();
    let mut main_exe = String::new();
    for r in db.rows("Shortcut") {
        let comp = r.str("Component_");
        if !comps.contains(&comp) {
            continue;
        }
        let raw_target = r.str("Target");
        let target = if features.contains(&raw_target) {
            keypath
                .get(&comp)
                .and_then(|k| ctx.files.get(k))
                .cloned()
                .unwrap_or_default()
        } else {
            ctx.format(&raw_target)
        };
        if target.is_empty() {
            continue;
        }
        if main_exe.is_empty() && target.to_ascii_lowercase().ends_with(".exe") {
            main_exe = target.clone();
        }
        let Some(dir) = dirs.get(&r.str("Directory_")) else {
            continue;
        };
        let workdir = {
            let w = r.str("WkDir");
            if w.is_empty() {
                String::new()
            } else {
                ctx.get(&w)
            }
        };
        let file =
            crate::paths::to_linux(&format!("{}{}.lnk", dir.target, long_name(&r.str("Name"))));
        if let Some(parent) = file.parent() {
            mkdirs(parent, &mut manifest.dirs).map_err(|e| e.to_string())?;
        }
        let bytes = shortcut_bytes(
            &target,
            &ctx.format(&r.str("Arguments")),
            &workdir,
            &ctx.format(&r.str("Description")),
        );
        std::fs::write(&file, bytes).map_err(|e| format!("{} : {e}", file.display()))?;
        manifest.files.push(file);
    }
    for (table, what) in [
        ("ServiceInstall", "services Windows"),
        ("Environment", "variables d'environnement"),
        ("IniFile", "fichiers .ini"),
    ] {
        let n = db.rows(table).count();
        if n > 0 {
            log.log(&format!("{n} entrée(s) de {what} ignorée(s)"));
        }
    }
    // Inscription dans « Programmes installés ».
    if !code.is_empty() {
        let key = uninstall_key(&code);
        let size_kb: u64 = manifest
            .files
            .iter()
            .filter_map(|f| f.metadata().ok())
            .map(|m| m.len())
            .sum::<u64>()
            / 1024;
        let location = [
            "ARPINSTALLLOCATION",
            "INSTALLDIR",
            "INSTALLFOLDER",
            "APPLICATIONFOLDER",
            "INSTALLLOCATION",
        ]
        .iter()
        .map(|p| ctx.get(p))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
        let mut sz = |n: &str, v: &str| {
            if !v.is_empty() {
                edits.push(RegEdit::Set {
                    key: key.clone(),
                    name: n.into(),
                    kind: 1,
                    bytes: utf16z(v),
                });
            }
        };
        sz("DisplayName", &name);
        sz("DisplayVersion", &version);
        sz("Publisher", &ctx.get("Manufacturer"));
        sz("InstallLocation", &location);
        sz("DisplayIcon", &main_exe);
        sz("URLInfoAbout", &ctx.get("ARPURLINFOABOUT"));
        sz("HelpLink", &ctx.get("ARPHELPLINK"));
        sz("UninstallString", &format!("MsiExec.exe /X{code}"));
        sz("QuietUninstallString", &format!("MsiExec.exe /X{code} /qn"));
        let day = {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let (y, m, d) = civil(secs / 86400);
            format!("{y:04}{m:02}{d:02}")
        };
        sz("InstallDate", &day);
        let mut dword = |n: &str, v: u32| {
            edits.push(RegEdit::Set {
                key: key.clone(),
                name: n.into(),
                kind: 4,
                bytes: v.to_le_bytes().to_vec(),
            })
        };
        dword("EstimatedSize", size_kb.min(u32::MAX as u64) as u32);
        dword("WindowsInstaller", 1);
        dword("NoModify", 1);
        if ctx.get("ARPSYSTEMCOMPONENT") == "1" {
            dword("SystemComponent", 1);
        }
    }
    if !apply_registry_edits(&edits) {
        return Err("écriture du registre impossible".into());
    }
    if !code.is_empty() {
        manifest
            .save()
            .map_err(|e| format!("journal d'installation : {e}"))?;
    }
    log.log(&format!("{name} est installé."));
    Ok(())
}

/// Date civile (année, mois, jour) d'un nombre de jours depuis 1970.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn remove_manifest(m: &Manifest, log: &mut Logger) {
    for f in &m.files {
        let _ = std::fs::remove_file(f);
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
    edits.push(RegEdit::DeleteTree(uninstall_key(&m.product)));
    apply_registry_edits(&edits);
    let mut keys = m.keys.clone();
    keys.sort_by_key(|k| std::cmp::Reverse(k.matches('\\').count()));
    keys.dedup();
    let empty: Vec<RegEdit> = keys
        .into_iter()
        .filter(|k| registry_key_is_empty(k))
        .map(RegEdit::DeleteTree)
        .collect();
    if !empty.is_empty() {
        apply_registry_edits(&empty);
    }
    let _ = std::fs::remove_file(manifest_path(&m.product));
    log.log(&format!("{} est désinstallé.", m.name));
}

pub fn uninstall(code: &str, log: &mut Logger) -> i32 {
    match Manifest::find(code) {
        Some(m) => {
            remove_manifest(&m, log);
            0
        }
        None => {
            log.log(&format!("Produit inconnu : {code}"));
            ERROR_UNKNOWN_PRODUCT
        }
    }
}
